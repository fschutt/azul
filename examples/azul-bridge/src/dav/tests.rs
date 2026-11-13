//! WebDAV requests against a drive in memory: what Finder, Explorer and the file managers
//! send, what the bridge answers, what lands in the drive.

use std::{
    io::{Read, Write},
    net::TcpStream,
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use azul_storage::{Drive, ListPage, ListRequest};

use super::*;
use crate::{memory::MemoryDrive, net::bind_loopback};

const USER: &str = "ada@example.org";
const PASSWORD: &str = "k7m2p-9qxat-4ds8w-hb3zn-e6r1v";

fn drive() -> Arc<MemoryDrive> {
    let drive = Arc::new(MemoryDrive::new());
    drive.set_now(1_790_843_400);
    drive.put("docs/a.txt", b"hello world").unwrap();
    drive.put("docs/b b.pdf", b"%PDF-1.4").unwrap();
    drive.put("photos/", b"").unwrap();
    drive.put(".azlin/index.json", b"{}").unwrap();
    drive
}

fn dav_on(drive: Arc<MemoryDrive>, port: u16) -> Dav {
    Dav::new(
        drive,
        Credentials::new(USER, PASSWORD),
        Arc::new(FailureGate::new(100, Duration::from_secs(60), Duration::ZERO)),
        Limits::default(),
        port,
        SwapRecords::in_memory(),
    )
}

fn head(method: &str, target: &str, headers: &[(&str, &str)]) -> Head {
    Head {
        method: method.to_string(),
        target: target.to_string(),
        version: (1, 1),
        headers: headers
            .iter()
            .map(|(n, v)| (n.to_string(), v.to_string()))
            .collect(),
    }
}

fn text(response: &Response) -> String {
    String::from_utf8_lossy(&response.body).into_owned()
}

#[test]
fn paths_are_decoded_and_anything_that_could_leave_the_drive_is_refused() {
    assert_eq!(key_of("/"), Ok((String::new(), true)));
    assert_eq!(key_of("/docs/a%20b.txt"), Ok((String::from("docs/a b.txt"), false)));
    assert_eq!(key_of("/docs/?x=1"), Ok((String::from("docs"), true)));
    assert_eq!(
        key_of("http://127.0.0.1:1180/docs/%C3%BC.txt"),
        Ok((String::from("docs/ü.txt"), false))
    );
    for bad in [
        "/a/../b", "/a/./b", "/a//b", "/%2e%2e/x", "/a%00b", "/a%5cb", "relative", "/%zz", "/%C3%28",
        "/a\u{1}b",
    ] {
        assert_eq!(key_of(bad), Err(Status::BAD_REQUEST), "{bad}");
    }
    assert_eq!(key_of("/.azlin/index.json"), Err(Status::NOT_FOUND));
    assert_eq!(href_of("docs/b b.pdf", false), "/docs/b%20b.pdf");
    assert_eq!(href_of("docs", true), "/docs/");
    assert_eq!(href_of("", true), "/");
}

#[test]
fn propfind_lists_one_level_with_the_common_properties_and_hides_the_sync() {
    let dav = dav_on(drive(), 0);
    let root = dav.respond(&head("PROPFIND", "/", &[("Depth", "1")]), b"");
    assert_eq!(root.status, Status::MULTI_STATUS);
    let body = text(&root);
    assert!(body.contains("<D:href>/</D:href>"), "{body}");
    assert!(body.contains("<D:href>/docs/</D:href>"), "{body}");
    assert!(body.contains("<D:href>/photos/</D:href>"), "{body}");
    assert!(body.contains("<D:resourcetype><D:collection/></D:resourcetype>"), "{body}");
    assert!(!body.contains(".azlin"), "{body}");
    let docs = text(&dav.respond(&head("PROPFIND", "/docs/", &[("Depth", "1")]), b""));
    assert!(docs.contains("<D:href>/docs/a.txt</D:href>"), "{docs}");
    assert!(docs.contains("<D:getcontentlength>11</D:getcontentlength>"), "{docs}");
    assert!(docs.contains("<D:getlastmodified>Thu, 01 Oct 2026 08:30:00 GMT</D:getlastmodified>"), "{docs}");
    assert!(docs.contains("<D:getcontenttype>text/plain; charset=utf-8</D:getcontenttype>"), "{docs}");
    assert!(docs.contains("<D:href>/docs/b%20b.pdf</D:href>"), "{docs}");
    let file = text(&dav.respond(&head("PROPFIND", "/docs/a.txt", &[("Depth", "0")]), b""));
    assert_eq!(file.matches("<D:response>").count(), 1, "{file}");
    let infinite = dav.respond(&head("PROPFIND", "/", &[]), b"");
    assert_eq!(infinite.status, Status::FORBIDDEN);
    assert!(text(&infinite).contains("propfind-finite-depth"));
    let missing = dav.respond(&head("PROPFIND", "/nope", &[("Depth", "0")]), b"");
    assert_eq!(missing.status, Status::NOT_FOUND);
}

#[test]
fn a_property_the_bridge_does_not_have_is_answered_as_not_found() {
    let dav = dav_on(drive(), 0);
    let body = b"<?xml version=\"1.0\"?><D:propfind xmlns:D=\"DAV:\"><D:prop>\
        <D:getcontentlength/><Z:Win32FileAttributes xmlns:Z=\"urn:schemas-microsoft-com:\"/>\
        </D:prop></D:propfind>";
    let answer = text(&dav.respond(&head("PROPFIND", "/docs/a.txt", &[("Depth", "0")]), body));
    assert!(answer.contains("<D:prop><D:getcontentlength>11</D:getcontentlength></D:prop><D:status>HTTP/1.1 200 OK"), "{answer}");
    assert!(
        answer.contains("<x:Win32FileAttributes xmlns:x=\"urn:schemas-microsoft-com:\"/></D:prop><D:status>HTTP/1.1 404 Not Found"),
        "{answer}"
    );
    let names = text(&dav.respond(
        &head("PROPFIND", "/docs/a.txt", &[("Depth", "0")]),
        b"<D:propfind xmlns:D=\"DAV:\"><D:propname/></D:propfind>",
    ));
    assert!(names.contains("<D:getetag/>") && !names.contains("hello"), "{names}");
}

#[test]
fn xml_with_a_dtd_or_entities_is_refused_before_it_is_parsed() {
    let dav = dav_on(drive(), 0);
    let laughs = b"<?xml version=\"1.0\"?><!DOCTYPE lolz [<!ENTITY lol \"lol\"><!ENTITY lol2 \"&lol;&lol;\">]>\
        <D:propfind xmlns:D=\"DAV:\"><D:prop><D:displayname>&lol2;</D:displayname></D:prop></D:propfind>";
    let external = b"<!DOCTYPE x [<!ENTITY e SYSTEM \"file:///etc/passwd\">]><D:propfind xmlns:D=\"DAV:\">&e;</D:propfind>";
    let bodies: [&[u8]; 4] = [
        &laughs[..],
        &external[..],
        &b"<D:propfind xmlns:D=\"DAV:\">"[..],
        &b"\xff\xfe"[..],
    ];
    for body in bodies {
        let answer = dav.respond(&head("PROPFIND", "/", &[("Depth", "0")]), body);
        assert_eq!(answer.status, Status::BAD_REQUEST, "{}", String::from_utf8_lossy(body));
    }
}

#[test]
fn get_and_head_give_the_file_or_one_range_of_it() {
    let dav = dav_on(drive(), 0);
    let whole = dav.respond(&head("GET", "/docs/a.txt", &[]), b"");
    assert_eq!((whole.status, whole.body.as_slice()), (Status::OK, &b"hello world"[..]));
    assert!(whole.header("ETag").is_some());
    let first = dav.respond(&head("GET", "/docs/a.txt", &[("Range", "bytes=0-4")]), b"");
    assert_eq!((first.status, first.body.as_slice()), (Status::PARTIAL, &b"hello"[..]));
    assert_eq!(first.header("Content-Range"), Some("bytes 0-4/11"));
    let last = dav.respond(&head("GET", "/docs/a.txt", &[("Range", "bytes=-5")]), b"");
    assert_eq!(last.body, b"world");
    let open = dav.respond(&head("GET", "/docs/a.txt", &[("Range", "bytes=6-")]), b"");
    assert_eq!(open.body, b"world");
    let outside = dav.respond(&head("GET", "/docs/a.txt", &[("Range", "bytes=20-")]), b"");
    assert_eq!(outside.status, Status::RANGE_NOT_SATISFIABLE);
    assert_eq!(outside.header("Content-Range"), Some("bytes */11"));
    let several = dav.respond(&head("GET", "/docs/a.txt", &[("Range", "bytes=0-1,3-4")]), b"");
    assert_eq!(several.status, Status::OK);
    let only_head = dav.respond(&head("HEAD", "/docs/a.txt", &[]), b"");
    assert_eq!(only_head.header("Content-Length"), Some("11"));
    assert!(only_head.body.is_empty());
    let folder = dav.respond(&head("GET", "/docs/", &[]), b"");
    assert!(text(&folder).contains("a.txt") && text(&folder).contains("b%20b.pdf"));
    assert_eq!(dav.respond(&head("GET", "/nope.txt", &[]), b"").status, Status::NOT_FOUND);
    assert_eq!(dav.respond(&head("GET", "/.azlin/index.json", &[]), b"").status, Status::NOT_FOUND);
}

#[test]
fn put_makes_or_replaces_a_file_in_a_folder_that_is_there() {
    let drive = drive();
    let dav = dav_on(drive.clone(), 0);
    assert_eq!(dav.respond(&head("PUT", "/docs/c.txt", &[]), b"new").status, Status::CREATED);
    assert_eq!(drive.get("docs/c.txt").unwrap(), b"new");
    assert_eq!(dav.respond(&head("PUT", "/docs/c.txt", &[]), b"newer").status, Status::NO_CONTENT);
    assert_eq!(dav.respond(&head("PUT", "/nowhere/x.txt", &[]), b"x").status, Status::CONFLICT);
    assert_eq!(dav.respond(&head("PUT", "/docs/", &[]), b"x").status, Status::METHOD_NOT_ALLOWED);
    assert_eq!(dav.respond(&head("PUT", "/docs", &[]), b"x").status, Status::METHOD_NOT_ALLOWED);
    assert_eq!(dav.respond(&head("PUT", "/top.txt", &[]), b"x").status, Status::CREATED);
}

#[test]
fn mkcol_and_delete_make_and_remove_folders_and_files() {
    let drive = drive();
    let dav = dav_on(drive.clone(), 0);
    assert_eq!(dav.respond(&head("MKCOL", "/new", &[]), b"").status, Status::CREATED);
    assert!(drive.keys().contains(&String::from("new/")));
    assert_eq!(dav.respond(&head("MKCOL", "/new/", &[]), b"").status, Status::METHOD_NOT_ALLOWED);
    assert_eq!(dav.respond(&head("MKCOL", "/a/b", &[]), b"").status, Status::CONFLICT);
    assert_eq!(dav.respond(&head("MKCOL", "/c", &[]), b"<x/>").status, Status::UNSUPPORTED_MEDIA);
    assert_eq!(dav.respond(&head("DELETE", "/docs/a.txt", &[]), b"").status, Status::NO_CONTENT);
    assert!(!drive.keys().contains(&String::from("docs/a.txt")));
    assert_eq!(dav.respond(&head("DELETE", "/docs", &[]), b"").status, Status::NO_CONTENT);
    assert!(!drive.keys().iter().any(|k| k.starts_with("docs/")), "{:?}", drive.keys());
    assert_eq!(dav.respond(&head("DELETE", "/", &[]), b"").status, Status::FORBIDDEN);
    assert_eq!(dav.respond(&head("DELETE", "/docs", &[]), b"").status, Status::NOT_FOUND);
}

#[test]
fn copy_and_move_follow_destination_overwrite_and_depth() {
    let drive = drive();
    let dav = dav_on(drive.clone(), 0);
    let copy = head("COPY", "/docs/a.txt", &[("Destination", "http://127.0.0.1:1180/docs/copy.txt")]);
    assert_eq!(dav.respond(&copy, b"").status, Status::CREATED);
    assert_eq!(drive.get("docs/copy.txt").unwrap(), b"hello world");
    let keep = head(
        "COPY",
        "/docs/a.txt",
        &[("Destination", "/docs/copy.txt"), ("Overwrite", "F")],
    );
    assert_eq!(dav.respond(&keep, b"").status, Status::PRECONDITION_FAILED);
    let replace = head("COPY", "/docs/b%20b.pdf", &[("Destination", "/docs/copy.txt")]);
    assert_eq!(dav.respond(&replace, b"").status, Status::NO_CONTENT);
    assert_eq!(drive.get("docs/copy.txt").unwrap(), b"%PDF-1.4");
    let nowhere = head("MOVE", "/docs/a.txt", &[("Destination", "/x/y/a.txt")]);
    assert_eq!(dav.respond(&nowhere, b"").status, Status::CONFLICT);
    let into_itself = head("MOVE", "/docs/", &[("Destination", "/docs/inner/")]);
    assert_eq!(dav.respond(&into_itself, b"").status, Status::FORBIDDEN);
    let moved = head("MOVE", "/docs/", &[("Destination", "/papers/")]);
    assert_eq!(dav.respond(&moved, b"").status, Status::CREATED);
    assert!(drive.keys().contains(&String::from("papers/a.txt")));
    assert!(!drive.keys().iter().any(|k| k.starts_with("docs/")));
    let shallow = head("COPY", "/papers/", &[("Destination", "/empty/"), ("Depth", "0")]);
    assert_eq!(dav.respond(&shallow, b"").status, Status::CREATED);
    assert_eq!(
        drive.keys().iter().filter(|k| k.starts_with("empty/")).collect::<Vec<_>>(),
        vec!["empty/"]
    );
    let escape = head("COPY", "/papers/a.txt", &[("Destination", "/../etc/passwd")]);
    assert_eq!(dav.respond(&escape, b"").status, Status::BAD_REQUEST);
}

fn lock_body(scope: &str) -> Vec<u8> {
    format!(
        "<?xml version=\"1.0\"?><D:lockinfo xmlns:D=\"DAV:\"><D:lockscope><D:{scope}/></D:lockscope>\
         <D:locktype><D:write/></D:locktype><D:owner><D:href>Finder</D:href></D:owner></D:lockinfo>"
    )
    .into_bytes()
}

#[test]
fn a_lock_keeps_others_out_until_its_token_is_shown_or_it_is_unlocked() {
    let drive = drive();
    let dav = dav_on(drive.clone(), 0);
    let locked = dav.respond(
        &head("LOCK", "/docs/a.txt", &[("Timeout", "Second-120"), ("Depth", "0")]),
        &lock_body("exclusive"),
    );
    assert_eq!(locked.status, Status::OK);
    let token = locked
        .header("Lock-Token")
        .unwrap()
        .trim_matches(|c| c == '<' || c == '>')
        .to_string();
    assert!(token.starts_with("opaquelocktoken:"), "{token}");
    let body = text(&locked);
    assert!(body.contains("<D:exclusive/>") && body.contains("Second-120") && body.contains("Finder"), "{body}");
    assert_eq!(dav.respond(&head("PUT", "/docs/a.txt", &[]), b"x").status, Status::LOCKED);
    assert_eq!(dav.respond(&head("DELETE", "/docs", &[]), b"").status, Status::LOCKED);
    let with_token = format!("(<{token}>)");
    assert_eq!(
        dav.respond(&head("PUT", "/docs/a.txt", &[("If", &with_token)]), b"mine").status,
        Status::NO_CONTENT
    );
    let second = dav.respond(&head("LOCK", "/docs/a.txt", &[]), &lock_body("shared"));
    assert_eq!(second.status, Status::LOCKED);
    let discovery = text(&dav.respond(
        &head("PROPFIND", "/docs/a.txt", &[("Depth", "0")]),
        b"<D:propfind xmlns:D=\"DAV:\"><D:prop><D:lockdiscovery/></D:prop></D:propfind>",
    ));
    assert!(discovery.contains(&token), "{discovery}");
    let refreshed = dav.respond(&head("LOCK", "/docs/a.txt", &[("If", &with_token)]), b"");
    assert_eq!(refreshed.status, Status::OK);
    let unlock = head("UNLOCK", "/docs/a.txt", &[("Lock-Token", &format!("<{token}>"))]);
    assert_eq!(dav.respond(&unlock, b"").status, Status::NO_CONTENT);
    assert_eq!(dav.respond(&unlock, b"").status, Status::CONFLICT);
    assert_eq!(dav.respond(&head("PUT", "/docs/a.txt", &[]), b"free").status, Status::NO_CONTENT);
    // A lock on a name that is not there makes an empty file; two shared locks get along.
    let new = dav.respond(&head("LOCK", "/docs/new.txt", &[]), &lock_body("shared"));
    assert_eq!(new.status, Status::CREATED);
    assert_eq!(drive.get("docs/new.txt").unwrap(), b"");
    let other = dav.respond(&head("LOCK", "/docs/new.txt", &[]), &lock_body("shared"));
    assert_eq!(other.status, Status::OK);
}

#[test]
fn a_lock_of_the_whole_drive_covers_every_file_and_names_its_owner_once() {
    let dav = dav_on(drive(), 0);
    let locked = dav.respond(&head("LOCK", "/", &[("Depth", "infinity")]), &lock_body("exclusive"));
    assert_eq!(locked.status, Status::OK);
    let body = text(&locked);
    assert!(body.contains("<D:owner>Finder</D:owner>"), "{body}");
    assert!(body.contains("<D:lockroot><D:href>/</D:href></D:lockroot>"), "{body}");
    assert_eq!(dav.respond(&head("PUT", "/top.txt", &[]), b"x").status, Status::LOCKED);
    assert_eq!(dav.respond(&head("PUT", "/docs/a.txt", &[]), b"x").status, Status::LOCKED);
    let token = locked
        .header("Lock-Token")
        .unwrap()
        .trim_matches(|c| c == '<' || c == '>')
        .to_string();
    let with_token = format!("(<{token}>)");
    assert_eq!(
        dav.respond(&head("PUT", "/top.txt", &[("If", &with_token)]), b"x").status,
        Status::CREATED
    );
}

#[test]
fn proppatch_is_answered_as_done_for_every_property_it_names() {
    let dav = dav_on(drive(), 0);
    let body = b"<?xml version=\"1.0\"?><D:propertyupdate xmlns:D=\"DAV:\" xmlns:Z=\"urn:schemas-microsoft-com:\">\
        <D:set><D:prop><Z:Win32LastModifiedTime>Thu, 01 Oct 2026 08:30:00 GMT</Z:Win32LastModifiedTime></D:prop></D:set>\
        </D:propertyupdate>";
    let answer = dav.respond(&head("PROPPATCH", "/docs/a.txt", &[]), body);
    assert_eq!(answer.status, Status::MULTI_STATUS);
    let answer = text(&answer);
    assert!(answer.contains("<x:Win32LastModifiedTime xmlns:x=\"urn:schemas-microsoft-com:\"/>"), "{answer}");
    assert!(answer.contains("HTTP/1.1 200 OK"), "{answer}");
    assert_eq!(dav.respond(&head("PROPPATCH", "/nope", &[]), body).status, Status::NOT_FOUND);
}

#[test]
fn only_this_computers_own_address_is_answered() {
    let dav = dav_on(drive(), 1180);
    for (host, ok) in [
        ("127.0.0.1:1180", true),
        ("localhost:1180", true),
        ("[::1]:1180", true),
        ("127.0.0.1:9999", false),
        ("evil.example:1180", false),
        ("127.0.0.1.evil.example:1180", false),
        ("", false),
    ] {
        let h = head("GET", "/", &[("Host", host)]);
        assert_eq!(dav.host_ok(&h), ok, "{host}");
    }
    assert!(!dav.host_ok(&head("GET", "/", &[])));
}

/// One request over a socket and its response: the status code, the headers, the body.
struct Wire {
    stream: TcpStream,
}

impl Wire {
    fn connect(dav: Arc<Dav>) -> Wire {
        let listener = bind_loopback(0).unwrap();
        let addr = listener.local_addr().unwrap();
        std::thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                dav.handle(stream);
            }
        });
        let stream = TcpStream::connect(addr).unwrap();
        stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
        Wire { stream }
    }

    fn send(&mut self, bytes: &[u8]) {
        self.stream.write_all(bytes).unwrap();
    }

    /// The next response: status, headers (lower-case names), body.
    fn response(&mut self) -> (u16, Vec<(String, String)>, Vec<u8>) {
        let mut head = Vec::new();
        let mut byte = [0u8; 1];
        while !head.ends_with(b"\r\n\r\n") {
            let n = self.stream.read(&mut byte).unwrap();
            assert_eq!(n, 1, "the connection closed: {}", String::from_utf8_lossy(&head));
            head.push(byte[0]);
        }
        let text = String::from_utf8_lossy(&head).into_owned();
        let mut lines = text.split("\r\n");
        let status: u16 = lines.next().unwrap()[9..12].parse().unwrap();
        let headers: Vec<(String, String)> = lines
            .filter_map(|l| l.split_once(':'))
            .map(|(n, v)| (n.to_ascii_lowercase(), v.trim().to_string()))
            .collect();
        let length: usize = headers
            .iter()
            .find(|(n, _)| n == "content-length")
            .map_or(0, |(_, v)| v.parse().unwrap());
        let mut body = vec![0u8; if status == 100 { 0 } else { length }];
        self.stream.read_exact(&mut body).unwrap();
        (status, headers, body)
    }

    fn closed(&mut self) -> bool {
        let mut rest = Vec::new();
        let _ = self.stream.read_to_end(&mut rest);
        rest.is_empty()
    }
}

fn basic(user: &str, password: &str) -> String {
    format!("Basic {}", crate::auth::encode_base64(format!("{user}:{password}").as_bytes()))
}

#[test]
fn a_client_is_challenged_then_signs_in_on_the_same_connection() {
    let dav = Arc::new(dav_on(drive(), 0));
    let mut wire = Wire::connect(dav);
    wire.send(b"OPTIONS / HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n");
    let (status, headers, _) = wire.response();
    assert_eq!(status, 200);
    assert!(headers.iter().any(|(n, v)| n == "dav" && v == "1, 2"));
    wire.send(b"PROPFIND / HTTP/1.1\r\nHost: 127.0.0.1\r\nDepth: 0\r\nContent-Length: 0\r\n\r\n");
    let (status, headers, _) = wire.response();
    assert_eq!(status, 401);
    assert!(headers.iter().any(|(n, v)| n == "www-authenticate" && v.starts_with("Basic realm=\"Azlin Bridge\"")));
    let request = format!(
        "PROPFIND / HTTP/1.1\r\nHost: 127.0.0.1\r\nDepth: 0\r\nAuthorization: {}\r\n\r\n",
        basic(USER, PASSWORD)
    );
    wire.send(request.as_bytes());
    let (status, _, body) = wire.response();
    assert_eq!(status, 207, "{}", String::from_utf8_lossy(&body));
}

#[test]
fn windows_explorer_signs_in_with_digest_and_never_sends_the_password() {
    let dav = Arc::new(dav_on(drive(), 0));
    let mut wire = Wire::connect(dav);
    wire.send(b"PROPFIND /docs/ HTTP/1.1\r\nHost: 127.0.0.1\r\nDepth: 0\r\n\r\n");
    let (status, headers, _) = wire.response();
    assert_eq!(status, 401);
    let challenge = headers
        .iter()
        .find(|(n, v)| n == "www-authenticate" && v.starts_with("Digest "))
        .map(|(_, v)| v.clone())
        .expect("a Digest challenge beside Basic");
    let p = crate::digest::parse_params(challenge.trim_start_matches("Digest "));
    let answer = |password: &str| {
        let response = crate::digest::response_for(
            USER,
            &p["realm"],
            password,
            "PROPFIND",
            "/docs/",
            &p["nonce"],
            Some(("00000001", "e2e")),
        );
        format!(
            "PROPFIND /docs/ HTTP/1.1\r\nHost: 127.0.0.1\r\nDepth: 0\r\nAuthorization: Digest \
             username=\"{USER}\", realm=\"{}\", nonce=\"{}\", uri=\"/docs/\", qop=auth, nc=00000001, \
             cnonce=\"e2e\", response=\"{response}\", opaque=\"{}\"\r\n\r\n",
            p["realm"], p["nonce"], p["opaque"]
        )
    };
    wire.send(answer("guess").as_bytes());
    assert_eq!(wire.response().0, 401);
    wire.send(answer(PASSWORD).as_bytes());
    let (status, _, body) = wire.response();
    assert_eq!(status, 207, "{}", String::from_utf8_lossy(&body));
}

#[test]
fn wrong_passwords_a_foreign_host_and_a_web_page_are_turned_away() {
    let dav = Arc::new(dav_on(drive(), 0));
    let mut wire = Wire::connect(dav.clone());
    for _ in 0..3 {
        let request = format!(
            "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: {}\r\n\r\n",
            basic(USER, "guess")
        );
        wire.send(request.as_bytes());
        assert_eq!(wire.response().0, 401);
    }
    assert!(wire.closed(), "the third wrong password closes the connection");

    let mut wire = Wire::connect(dav.clone());
    let request = format!(
        "GET / HTTP/1.1\r\nHost: attacker.example\r\nAuthorization: {}\r\n\r\n",
        basic(USER, PASSWORD)
    );
    wire.send(request.as_bytes());
    assert_eq!(wire.response().0, 403);
    assert!(wire.closed());

    let mut wire = Wire::connect(dav);
    let request = format!(
        "GET / HTTP/1.1\r\nHost: 127.0.0.1\r\nOrigin: http://attacker.example\r\nAuthorization: {}\r\n\r\n",
        basic(USER, PASSWORD)
    );
    wire.send(request.as_bytes());
    assert_eq!(wire.response().0, 403);
}

#[test]
fn a_chunked_put_after_100_continue_lands_in_the_drive() {
    let drive = drive();
    let dav = Arc::new(dav_on(drive.clone(), 0));
    let mut wire = Wire::connect(dav);
    let request = format!(
        "PUT /docs/finder.txt HTTP/1.1\r\nHost: localhost\r\nAuthorization: {}\r\n\
         Transfer-Encoding: chunked\r\nExpect: 100-continue\r\n\r\n",
        basic(USER, PASSWORD)
    );
    wire.send(request.as_bytes());
    assert_eq!(wire.response().0, 100);
    wire.send(b"5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n");
    assert_eq!(wire.response().0, 201);
    assert_eq!(drive.get("docs/finder.txt").unwrap(), b"hello world");
    // Kept alive: a second request on the same connection.
    let get = format!(
        "GET /docs/finder.txt HTTP/1.1\r\nHost: localhost\r\nAuthorization: {}\r\n\r\n",
        basic(USER, PASSWORD)
    );
    wire.send(get.as_bytes());
    let (status, _, body) = wire.response();
    assert_eq!((status, body.as_slice()), (200, &b"hello world"[..]));
}

#[test]
fn a_put_over_the_limit_is_refused_and_the_connection_closed() {
    let limits = Limits {
        put_bytes: 4,
        ..Limits::default()
    };
    let dav = Arc::new(Dav::new(
        drive(),
        Credentials::new(USER, PASSWORD),
        Arc::new(FailureGate::new(100, Duration::from_secs(60), Duration::ZERO)),
        limits,
        0,
        SwapRecords::in_memory(),
    ));
    let mut wire = Wire::connect(dav);
    let request = format!(
        "PUT /docs/big.bin HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: {}\r\nContent-Length: 10\r\n\r\n0123456789",
        basic(USER, PASSWORD)
    );
    wire.send(request.as_bytes());
    assert_eq!(wire.response().0, 413);
    assert!(wire.closed());
}

#[test]
fn the_calendar_and_contacts_port_has_the_same_doors() {
    let pim = crate::pim::Pim::new(drive(), drive(), crate::pim::Names::in_memory(), USER);
    let dav = Arc::new(Dav::for_pim(
        pim,
        Credentials::new(USER, PASSWORD),
        Arc::new(FailureGate::new(100, Duration::from_secs(60), Duration::ZERO)),
        Limits::default(),
        0,
    ));
    let mut wire = Wire::connect(dav.clone());
    wire.send(b"OPTIONS / HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n");
    let (status, headers, _) = wire.response();
    assert_eq!(status, 200);
    assert!(headers.iter().any(|(n, v)| n == "dav" && v.contains("calendar-access")), "{headers:?}");
    wire.send(b"PROPFIND /principal/ HTTP/1.1\r\nHost: 127.0.0.1\r\nDepth: 0\r\nContent-Length: 0\r\n\r\n");
    assert_eq!(wire.response().0, 401);
    let request = format!(
        "PROPFIND /principal/ HTTP/1.1\r\nHost: 127.0.0.1\r\nDepth: 0\r\nAuthorization: {}\r\n\r\n",
        basic(USER, PASSWORD)
    );
    wire.send(request.as_bytes());
    let (status, _, body) = wire.response();
    assert_eq!(status, 207, "{}", String::from_utf8_lossy(&body));
    assert!(String::from_utf8_lossy(&body).contains("<D:href>/principal/</D:href>"));
    // The drive's files are not behind this port.
    let request = format!(
        "GET /docs/a.txt HTTP/1.1\r\nHost: 127.0.0.1\r\nAuthorization: {}\r\n\r\n",
        basic(USER, PASSWORD)
    );
    wire.send(request.as_bytes());
    assert_eq!(wire.response().0, 404);

    let mut wire = Wire::connect(dav);
    let request = format!(
        "PROPFIND /principal/ HTTP/1.1\r\nHost: 127.0.0.1\r\nOrigin: http://attacker.example\r\nDepth: 0\r\nAuthorization: {}\r\n\r\n",
        basic(USER, PASSWORD)
    );
    wire.send(request.as_bytes());
    assert_eq!(wire.response().0, 403);
}

/// A drive in memory that counts how files came: whole (`put`) or streamed (`put_from`).
struct Counting {
    inner: MemoryDrive,
    whole: AtomicUsize,
    streamed: AtomicUsize,
}

impl Drive for Counting {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.inner.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.inner.get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.inner.get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.whole.fetch_add(1, Ordering::SeqCst);
        self.inner.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.inner.delete(key)
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.inner.head(key)
    }
    fn put_from(&self, key: &str, body: &mut dyn Read) -> Result<u64, DriveError> {
        self.streamed.fetch_add(1, Ordering::SeqCst);
        let mut bytes = Vec::new();
        body.read_to_end(&mut bytes)
            .map_err(|e| DriveError::Io(e.to_string()))?;
        self.inner.put(key, &bytes)?;
        Ok(bytes.len() as u64)
    }
}

/// A PUT goes into the drive as a stream (`Drive::put_from`: an encrypted drive spools a big
/// file to a temporary file, a bucket sends it in parts), so the bridge never holds a file of
/// Finder's whole in memory - with a length and chunked.
#[test]
fn a_put_streams_its_body_into_the_drive_and_never_holds_it_whole() {
    let drive = Arc::new(Counting {
        inner: MemoryDrive::new(),
        whole: AtomicUsize::new(0),
        streamed: AtomicUsize::new(0),
    });
    drive.inner.put("docs/", b"").unwrap();
    let dav = Arc::new(Dav::new(
        drive.clone(),
        Credentials::new(USER, PASSWORD),
        Arc::new(FailureGate::new(100, Duration::from_secs(60), Duration::ZERO)),
        Limits::default(),
        0,
        SwapRecords::in_memory(),
    ));
    let mut wire = Wire::connect(dav);
    let body = vec![b'x'; 200 * 1024];
    let request = format!(
        "PUT /docs/big.bin HTTP/1.1\r\nHost: localhost\r\nAuthorization: {}\r\nContent-Length: {}\r\n\r\n",
        basic(USER, PASSWORD),
        body.len()
    );
    wire.send(request.as_bytes());
    wire.send(&body);
    assert_eq!(wire.response().0, 201);
    let chunked = format!(
        "PUT /docs/finder.txt HTTP/1.1\r\nHost: localhost\r\nAuthorization: {}\r\n\
         Transfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n",
        basic(USER, PASSWORD)
    );
    wire.send(chunked.as_bytes());
    assert_eq!(wire.response().0, 201);
    assert_eq!(drive.inner.get("docs/big.bin").unwrap(), body);
    assert_eq!(drive.inner.get("docs/finder.txt").unwrap(), b"hello world");
    assert_eq!(
        (drive.streamed.load(Ordering::SeqCst), drive.whole.load(Ordering::SeqCst)),
        (2, 0),
        "both streamed, neither held whole"
    );
}

#[test]
fn a_chunked_put_over_the_limit_is_refused_while_it_streams_and_nothing_is_kept() {
    let drive = drive();
    let limits = Limits {
        put_bytes: 8,
        ..Limits::default()
    };
    let dav = Arc::new(Dav::new(
        drive.clone(),
        Credentials::new(USER, PASSWORD),
        Arc::new(FailureGate::new(100, Duration::from_secs(60), Duration::ZERO)),
        limits,
        0,
        SwapRecords::in_memory(),
    ));
    let mut wire = Wire::connect(dav);
    let request = format!(
        "PUT /docs/too-big.txt HTTP/1.1\r\nHost: localhost\r\nAuthorization: {}\r\n\
         Transfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n",
        basic(USER, PASSWORD)
    );
    wire.send(request.as_bytes());
    assert_eq!(wire.response().0, 413);
    assert!(wire.closed());
    assert!(drive.head("docs/too-big.txt").is_err(), "nothing kept");
}

/// WebDAV over an encrypted drive opened as the bridge opens an encrypted account's
/// (azul-mail-core's mail_drive): folders, files, ranges, moves and deletes work, and the bucket
/// holds no name and no byte of them in the clear.
#[cfg(feature = "encryption")]
#[test]
fn webdav_over_an_encrypted_drive_keeps_no_name_or_byte_in_the_bucket_in_the_clear() {
    use azmail_core::mail_drive;
    use azul_storage::{
        crypto::{device, keys::RecoveryKdf},
        keyring::{KeyringStore, MemoryKeyring},
        meta::MemoryBucket,
        ops::list_all,
    };
    let keyring: Arc<dyn KeyringStore> = Arc::new(MemoryKeyring::new());
    let bucket = Arc::new(MemoryBucket::new());
    let cheap = RecoveryKdf::with_cost(64, 1, 1).unwrap();
    device::setup_new_drive(bucket.as_ref(), keyring.as_ref(), "d_dav", cheap).unwrap();
    let drive = mail_drive::wrap_auto(
        bucket.clone(),
        "d_dav",
        keyring,
        azul_storage::meta::RestoreSource::none("a test has no token server to ask"),
    );
    let dav = Dav::new(
        drive,
        Credentials::new(USER, PASSWORD),
        Arc::new(FailureGate::new(100, Duration::from_secs(60), Duration::ZERO)),
        Limits::default(),
        0,
        SwapRecords::in_memory(),
    );
    let expect = |response: Response, status: Status| -> Response {
        assert_eq!(response.status, status, "{}", text(&response));
        response
    };
    expect(dav.respond(&head("MKCOL", "/Briefe/", &[]), b""), Status::CREATED);
    expect(
        dav.respond(&head("PUT", "/Briefe/an%20Oma.txt", &[]), b"Liebe Oma, bis bald."),
        Status::CREATED,
    );
    let range = expect(
        dav.respond(&head("GET", "/Briefe/an%20Oma.txt", &[("Range", "bytes=0-4")]), b""),
        Status::PARTIAL,
    );
    assert_eq!(range.body, b"Liebe");
    let listing = text(&expect(
        dav.respond(&head("PROPFIND", "/Briefe/", &[("Depth", "1")]), b""),
        Status::MULTI_STATUS,
    ));
    assert!(listing.contains("<D:href>/Briefe/an%20Oma.txt</D:href>"), "{listing}");
    expect(
        dav.respond(
            &head("MOVE", "/Briefe/an%20Oma.txt", &[("Destination", "/Briefe/Oma.txt")]),
            b"",
        ),
        Status::CREATED,
    );
    assert_eq!(dav.respond(&head("GET", "/Briefe/Oma.txt", &[]), b"").body, b"Liebe Oma, bis bald.");
    let root = text(&dav.respond(&head("PROPFIND", "/", &[("Depth", "1")]), b""));
    assert!(root.contains("<D:href>/Briefe/</D:href>") && !root.contains(".azlin"), "{root}");
    for object in list_all(bucket.as_ref(), "").unwrap() {
        assert!(!object.key.contains("Briefe") && !object.key.contains("Oma"), "{}", object.key);
        let bytes = bucket.get(&object.key).unwrap();
        assert!(!bytes.windows(9).any(|w| w == b"Liebe Oma"), "{} holds the text", object.key);
    }
    expect(dav.respond(&head("DELETE", "/Briefe/", &[]), b""), Status::NO_CONTENT);
    assert_eq!(dav.respond(&head("GET", "/Briefe/Oma.txt", &[]), b"").status, Status::NOT_FOUND);
}

// ==== X-DELTA-SYNC4-17: COPY and MOVE replace a destination only after the new content is whole ====

fn dav_over(drive: Arc<dyn Drive>) -> Dav {
    dav_keeping(drive, SwapRecords::in_memory())
}

/// A server whose swaps are recorded in `records`: a new one over a clone of the same records is the
/// NEXT process of a bridge that was killed (the records are the one thing that outlives it).
fn dav_keeping(drive: Arc<dyn Drive>, records: SwapRecords) -> Dav {
    Dav::new(
        drive,
        Credentials::new(USER, PASSWORD),
        Arc::new(FailureGate::new(
            100,
            Duration::from_secs(60),
            Duration::ZERO,
        )),
        Limits::default(),
        0,
        records,
    )
}

/// A drive in memory that fails where a test says: its `copy` from the call after the first
/// `copies_ok` ones on, and the `rename` calls listed in `rename_fails` (counted from 1). Its
/// `rename` is `MemoryDrive`'s default, copy and delete, so a copy that fails fails a rename too.
struct Flaky {
    inner: MemoryDrive,
    copies_ok: AtomicUsize,
    renames: AtomicUsize,
    rename_fails: Vec<usize>,
}

impl Flaky {
    /// The `drive()` of the other tests, a folder `src/` of two files, a folder `dest/` with one
    /// file and a file `top.txt`.
    fn new(copies_ok: usize, rename_fails: &[usize]) -> Arc<Flaky> {
        let inner = MemoryDrive::new();
        inner.set_now(1_790_843_400);
        for (key, bytes) in [
            ("docs/a.txt", &b"hello world"[..]),
            ("docs/b b.pdf", b"%PDF-1.4"),
            ("photos/", b""),
            ("src/one.txt", b"one"),
            ("src/two.txt", b"two"),
            ("dest/keep.txt", b"old"),
            ("top.txt", b"top"),
        ] {
            inner.put(key, bytes).unwrap();
        }
        Arc::new(Flaky {
            inner,
            copies_ok: AtomicUsize::new(copies_ok),
            renames: AtomicUsize::new(0),
            rename_fails: rename_fails.to_vec(),
        })
    }
}

impl Drive for Flaky {
    fn list(&self, request: &ListRequest) -> Result<ListPage, azul_storage::DriveError> {
        self.inner.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, azul_storage::DriveError> {
        self.inner.get(key)
    }
    fn get_range(
        &self,
        key: &str,
        range: azul_storage::ByteRange,
    ) -> Result<Vec<u8>, azul_storage::DriveError> {
        self.inner.get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), azul_storage::DriveError> {
        self.inner.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), azul_storage::DriveError> {
        self.inner.delete(key)
    }
    fn head(&self, key: &str) -> Result<azul_storage::ObjectInfo, azul_storage::DriveError> {
        self.inner.head(key)
    }
    fn copy(&self, from: &str, to: &str) -> Result<(), azul_storage::DriveError> {
        let left = self.copies_ok.load(Ordering::SeqCst);
        if left == 0 {
            return Err(azul_storage::DriveError::Transport(String::from(
                "the copy failed",
            )));
        }
        if left != usize::MAX {
            self.copies_ok.store(left - 1, Ordering::SeqCst);
        }
        self.inner.copy(from, to)
    }
    fn rename(&self, from: &str, to: &str) -> Result<(), azul_storage::DriveError> {
        let call = self.renames.fetch_add(1, Ordering::SeqCst) + 1;
        if self.rename_fails.contains(&call) {
            return Err(azul_storage::DriveError::Transport(String::from(
                "the rename failed",
            )));
        }
        azul_storage::ops::rename_by_copy(self, from, to)
    }
}

fn dest(url: &str) -> [(&str, &str); 1] {
    [("Destination", url)]
}

/// RED on the code before: the destination was deleted BEFORE the rename, and a destination that
/// is the folder the source lies in took the whole folder with it (`MOVE /docs/a.txt` onto
/// `/docs` deleted `docs/`, then failed to rename a file that was no more).
#[test]
fn x_delta_sync4_17_a_move_or_copy_onto_an_ancestor_is_refused_and_nothing_is_deleted() {
    let drive = drive();
    drive.put("docs/sub/deep.txt", b"deep").unwrap();
    let dav = dav_on(drive.clone(), 0);
    let before = drive.keys();
    for (method, source, destination) in [
        ("MOVE", "/docs/a.txt", "/docs"),
        ("MOVE", "/docs/a.txt", "/docs/"),
        ("COPY", "/docs/a.txt", "/docs"),
        ("MOVE", "/docs/sub/deep.txt", "/docs/"),
        ("MOVE", "/docs/sub/", "/docs"),
        ("COPY", "/docs/sub/", "/docs"),
        ("MOVE", "/docs/", "/docs"),
        ("MOVE", "/docs/", "/docs/sub/"),
    ] {
        let answer = dav.respond(&head(method, source, &dest(destination)), b"");
        assert_eq!(
            answer.status,
            Status::FORBIDDEN,
            "{method} {source} -> {destination}: {}",
            text(&answer)
        );
        assert_eq!(
            drive.keys(),
            before,
            "{method} {source} -> {destination} touched the drive"
        );
    }
    assert_eq!(drive.get("docs/a.txt").unwrap(), b"hello world");
}

/// The same refusal on a real folder (`serve --folder DIR`), whose identity is the disk's: a
/// source and a destination are told apart by the files, not by the spelling.
#[test]
fn x_delta_sync4_17_a_move_of_a_folder_onto_its_own_parent_on_disk_is_refused_and_keeps_it() {
    let dir = azul_storage::testing::TempDir::new("bridge-dav-ancestor");
    let local = azul_storage::LocalDrive::without_manifest(dir.0.clone());
    local.put("x/a/a/file.txt", b"inside").unwrap();
    local.put("x/a/other.txt", b"beside").unwrap();
    let dav = dav_over(Arc::new(local));
    let answer = dav.respond(&head("MOVE", "/x/a/a/", &dest("/x/a")), b"");
    assert_eq!(answer.status, Status::FORBIDDEN, "{}", text(&answer));
    assert_eq!(
        std::fs::read(dir.0.join("x/a/a/file.txt")).unwrap(),
        b"inside"
    );
    assert_eq!(
        std::fs::read(dir.0.join("x/a/other.txt")).unwrap(),
        b"beside"
    );
}

/// A destination spelt in another case is the source on a disk that ignores case: the MOVE
/// deleted it (the "existing destination") and then found nothing to rename. Skipped, said so,
/// on a volume that tells the two names apart.
#[test]
fn x_delta_sync4_17_a_move_that_only_changes_the_case_does_not_delete_the_file_on_a_disk_that_ignores_it(
) {
    let dir = azul_storage::testing::TempDir::new("bridge-dav-case");
    let local = azul_storage::LocalDrive::without_manifest(dir.0.clone());
    local.put("Doc.odt", b"the only copy").unwrap();
    if !dir.0.join("doc.odt").exists() {
        eprintln!("skipped: this volume tells Doc.odt and doc.odt apart");
        return;
    }
    let dav = dav_over(Arc::new(local));
    let answer = dav.respond(&head("MOVE", "/Doc.odt", &dest("/doc.odt")), b"");
    assert_eq!(answer.status, Status::FORBIDDEN, "{}", text(&answer));
    assert_eq!(
        std::fs::read(dir.0.join("Doc.odt")).unwrap(),
        b"the only copy"
    );
}

#[test]
fn x_delta_sync4_17_overwrite_f_onto_an_existing_file_or_folder_is_412_and_changes_nothing() {
    let drive = drive();
    let dav = dav_on(drive.clone(), 0);
    let before = drive.keys();
    for (method, source, destination) in [
        ("COPY", "/docs/a.txt", "/docs/b%20b.pdf"),
        ("MOVE", "/docs/a.txt", "/docs/b%20b.pdf"),
        ("MOVE", "/docs/", "/photos/"),
        ("COPY", "/docs/a.txt", "/photos"),
    ] {
        let answer = dav.respond(
            &head(
                method,
                source,
                &[("Destination", destination), ("Overwrite", "F")],
            ),
            b"",
        );
        assert_eq!(
            answer.status,
            Status::PRECONDITION_FAILED,
            "{method} {source} -> {destination}"
        );
        assert_eq!(drive.keys(), before);
    }
    assert_eq!(drive.get("docs/b b.pdf").unwrap(), b"%PDF-1.4");
}

/// RED on the code before: the destination was deleted, then the copy failed - neither was left.
#[test]
fn x_delta_sync4_17_a_copy_that_fails_leaves_the_old_destination_file_whole() {
    let drive = Flaky::new(0, &[]);
    let dav = dav_over(drive.clone());
    let before = drive.inner.keys();
    let answer = dav.respond(&head("COPY", "/docs/a.txt", &dest("/docs/b%20b.pdf")), b"");
    assert_eq!(answer.status, Status::BAD_GATEWAY, "{}", text(&answer));
    assert_eq!(
        drive.inner.get("docs/b b.pdf").unwrap(),
        b"%PDF-1.4",
        "the destination is as it was"
    );
    assert_eq!(drive.inner.keys(), before);
    // A move is that copy and then the removal of the source: the copy failed, so both stay.
    let answer = dav.respond(&head("MOVE", "/docs/a.txt", &dest("/docs/b%20b.pdf")), b"");
    assert_eq!(answer.status, Status::BAD_GATEWAY, "{}", text(&answer));
    assert_eq!(drive.inner.keys(), before);
    assert_eq!(drive.inner.get("docs/a.txt").unwrap(), b"hello world");
}

#[test]
fn x_delta_sync4_17_a_move_over_a_file_replaces_it_and_removes_the_source_after_the_copy() {
    let drive = Flaky::new(usize::MAX, &[]);
    let dav = dav_over(drive.clone());
    let answer = dav.respond(&head("MOVE", "/docs/a.txt", &dest("/docs/b%20b.pdf")), b"");
    assert_eq!(answer.status, Status::NO_CONTENT, "{}", text(&answer));
    assert_eq!(drive.inner.get("docs/b b.pdf").unwrap(), b"hello world");
    assert!(
        drive.inner.get("docs/a.txt").is_err(),
        "the source went after the copy landed"
    );
    assert!(
        drive.inner.keys().iter().all(|key| !key.contains(".azul-")),
        "{:?}",
        drive.inner.keys()
    );
}

/// RED on the code before: `dest/` was deleted, then the copy of `src/` failed after one file.
#[test]
fn x_delta_sync4_17_a_folder_copy_that_fails_halfway_leaves_the_destination_folder_whole() {
    let drive = Flaky::new(1, &[]);
    let dav = dav_over(drive.clone());
    let before = drive.inner.keys();
    let answer = dav.respond(&head("COPY", "/src/", &dest("/dest/")), b"");
    assert_eq!(answer.status, Status::BAD_GATEWAY, "{}", text(&answer));
    assert_eq!(drive.inner.get("dest/keep.txt").unwrap(), b"old");
    assert_eq!(
        drive.inner.keys(),
        before,
        "no half of the copy and no staging folder is left"
    );
}

#[test]
fn x_delta_sync4_17_a_folder_replaces_a_folder_whole_by_copy_and_by_move() {
    let drive = Flaky::new(usize::MAX, &[]);
    let dav = dav_over(drive.clone());
    let answer = dav.respond(&head("COPY", "/src/", &dest("/dest/")), b"");
    assert_eq!(answer.status, Status::NO_CONTENT, "{}", text(&answer));
    let keys = drive.inner.keys();
    assert!(
        keys.contains(&String::from("dest/one.txt"))
            && keys.contains(&String::from("dest/two.txt")),
        "{keys:?}"
    );
    assert!(
        !keys.contains(&String::from("dest/keep.txt")),
        "the old content is replaced: {keys:?}"
    );
    assert!(
        keys.contains(&String::from("src/one.txt")),
        "a copy keeps the source: {keys:?}"
    );
    assert!(keys.iter().all(|key| !key.contains(".azul-")), "{keys:?}");
    // Now a move over a folder that is there (docs/ over src/).
    let answer = dav.respond(&head("MOVE", "/docs/", &dest("/src/")), b"");
    assert_eq!(answer.status, Status::NO_CONTENT, "{}", text(&answer));
    let keys = drive.inner.keys();
    assert!(
        keys.contains(&String::from("src/a.txt")) && !keys.contains(&String::from("src/one.txt")),
        "{keys:?}"
    );
    assert!(
        keys.iter()
            .all(|key| !key.starts_with("docs/") && !key.contains(".azul-")),
        "{keys:?}"
    );
}

#[test]
fn x_delta_sync4_17_a_file_replaces_a_folder_and_a_folder_replaces_a_file() {
    let drive = Flaky::new(usize::MAX, &[]);
    let dav = dav_over(drive.clone());
    let answer = dav.respond(&head("COPY", "/docs/a.txt", &dest("/photos")), b"");
    assert_eq!(answer.status, Status::NO_CONTENT, "{}", text(&answer));
    assert_eq!(drive.inner.get("photos").unwrap(), b"hello world");
    assert!(!drive.inner.keys().contains(&String::from("photos/")));
    let answer = dav.respond(&head("COPY", "/src/", &dest("/top.txt")), b"");
    assert_eq!(answer.status, Status::NO_CONTENT, "{}", text(&answer));
    let keys = drive.inner.keys();
    assert!(keys.contains(&String::from("top.txt/one.txt")), "{keys:?}");
    assert!(
        !keys.contains(&String::from("top.txt")),
        "the file is replaced by the folder: {keys:?}"
    );
    assert!(keys.iter().all(|key| !key.contains(".azul-")), "{keys:?}");
}

/// The rows of `swap_in`'s crash table that a failing step reaches: the step that moves the old
/// destination aside, and the one that puts the new content in its place. Either way the old
/// destination is under its own name again and nothing this request made is left.
#[test]
fn x_delta_sync4_17_a_swap_step_that_fails_puts_the_old_destination_back() {
    // COPY src/ over dest/: renames are 1 = dest aside, 2 = new content into place (3 = back).
    for failing in [1, 2] {
        let drive = Flaky::new(usize::MAX, &[failing]);
        let dav = dav_over(drive.clone());
        let before = drive.inner.keys();
        let answer = dav.respond(&head("COPY", "/src/", &dest("/dest/")), b"");
        assert_eq!(
            answer.status,
            Status::BAD_GATEWAY,
            "rename {failing}: {}",
            text(&answer)
        );
        assert_eq!(drive.inner.keys(), before, "rename {failing} failed");
        assert_eq!(drive.inner.get("dest/keep.txt").unwrap(), b"old");
    }
    // MOVE src/ over dest/: 1 = src to beside, 2 = dest aside, 3 = beside into place; (4, 5 = undo).
    for failing in [1, 2, 3] {
        let drive = Flaky::new(usize::MAX, &[failing]);
        let dav = dav_over(drive.clone());
        let before = drive.inner.keys();
        let answer = dav.respond(&head("MOVE", "/src/", &dest("/dest/")), b"");
        assert_eq!(
            answer.status,
            Status::BAD_GATEWAY,
            "rename {failing}: {}",
            text(&answer)
        );
        assert_eq!(drive.inner.keys(), before, "rename {failing} failed");
        assert_eq!(drive.inner.get("dest/keep.txt").unwrap(), b"old");
        assert_eq!(drive.inner.get("src/one.txt").unwrap(), b"one");
    }
}

/// A drive in memory that DIES - the process is gone: a panic nothing catches but the test, and
/// nothing after it runs, no undo - right before the `die_at`-th step of a swap: a rename, or the
/// delete of a name this swap staged (`*.azul-*`). The same files as [`Flaky`]'s.
struct Killing {
    inner: MemoryDrive,
    steps: AtomicUsize,
    die_at: AtomicUsize,
}

impl Killing {
    fn new(die_at: usize) -> Arc<Killing> {
        let inner = MemoryDrive::new();
        inner.set_now(1_790_843_400);
        for (key, bytes) in [
            ("src/one.txt", &b"one"[..]),
            ("src/two.txt", b"two"),
            ("dest/keep.txt", b"old"),
            ("top.txt", b"top"),
        ] {
            inner.put(key, bytes).unwrap();
        }
        Arc::new(Killing {
            inner,
            steps: AtomicUsize::new(0),
            die_at: AtomicUsize::new(die_at),
        })
    }

    fn step(&self, what: &str) {
        let n = self.steps.fetch_add(1, Ordering::SeqCst) + 1;
        if n == self.die_at.load(Ordering::SeqCst) {
            panic!("the process died before step {n}: {what}");
        }
    }
}

impl Drive for Killing {
    fn list(&self, request: &ListRequest) -> Result<ListPage, azul_storage::DriveError> {
        self.inner.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, azul_storage::DriveError> {
        self.inner.get(key)
    }
    fn get_range(
        &self,
        key: &str,
        range: azul_storage::ByteRange,
    ) -> Result<Vec<u8>, azul_storage::DriveError> {
        self.inner.get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), azul_storage::DriveError> {
        self.inner.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), azul_storage::DriveError> {
        if key.contains(".azul-") {
            self.step("delete");
        }
        self.inner.delete(key)
    }
    fn head(&self, key: &str) -> Result<azul_storage::ObjectInfo, azul_storage::DriveError> {
        self.inner.head(key)
    }
    fn rename(&self, from: &str, to: &str) -> Result<(), azul_storage::DriveError> {
        self.step("rename");
        self.inner.rename(from, to)
    }
    fn delete_folder(&self, prefix: &str) -> Result<(), azul_storage::DriveError> {
        if prefix.contains(".azul-") {
            self.step("delete_folder");
        }
        self.inner.delete_folder(prefix)
    }
}

/// X-DELTA-APPS3-14, RED on the code before: a kill inside `swap_in` left `*.azul-new-*` /
/// `*.azul-old-*` that nothing found again - a stray copy beside the destination, the source of a
/// MOVE under a staging name with its own name gone (a retried MOVE answered 404 although nothing
/// was lost), the destination's name missing, both full copies for ever. Now the swap writes its
/// intent first and the first request of the next process finishes or undoes it, by the names the
/// intent holds. Every row of the table, by stopping the flow before each step: the destination
/// ends whole - the old content or the new, never neither - the source is where it belongs,
/// nothing is left under a staging name and the intent is gone.
#[test]
fn x_delta_apps3_14_a_kill_inside_a_swap_is_finished_or_undone_by_the_next_start() {
    // COPY src/ over dest/: steps are 1 = dest aside, 2 = new content into place, 3 = old deleted.
    // MOVE src/ over dest/: 1 = src beside, 2 = dest aside, 3 = beside into place, 4 = old deleted.
    for (method, kills) in [("COPY", 3_usize), ("MOVE", 4)] {
        for die_at in 1..=kills {
            let drive = Killing::new(die_at);
            let records = SwapRecords::in_memory();
            let dav = dav_keeping(drive.clone(), records.clone());
            let died = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                dav.respond(&head(method, "/src/", &dest("/dest/")), b"")
            }));
            assert!(
                died.is_err(),
                "{method}: the process died before step {die_at}"
            );
            drop(dav);
            // The next process: a new server over the same drive and the same records; its first
            // request settles it.
            drive.die_at.store(usize::MAX, Ordering::SeqCst);
            let next = dav_keeping(drive.clone(), records.clone());
            let answer = next.respond(&head("OPTIONS", "/", &[]), b"");
            assert_eq!(answer.status, Status::OK, "{method} {die_at}");
            let keys = drive.inner.keys();
            let at = format!("{method}, before step {die_at}: {keys:?}");
            assert!(
                keys.iter().all(|key| !key.contains(".azul-")),
                "nothing under a staging name: {at}"
            );
            assert!(
                records.stamps().unwrap().is_empty(),
                "no intent is left: {at}"
            );
            // Which side of the swap the kill left it on: the first steps are undone, the rest
            // finished.
            let finished = die_at >= if method == "MOVE" { 3 } else { 2 };
            if finished {
                assert_eq!(drive.inner.get("dest/one.txt").unwrap(), b"one", "{at}");
                assert_eq!(drive.inner.get("dest/two.txt").unwrap(), b"two", "{at}");
                assert!(
                    drive.inner.get("dest/keep.txt").is_err(),
                    "the old content is gone: {at}"
                );
            } else {
                assert_eq!(drive.inner.get("dest/keep.txt").unwrap(), b"old", "{at}");
                assert!(drive.inner.get("dest/one.txt").is_err(), "{at}");
            }
            if method == "COPY" || !finished {
                assert_eq!(
                    drive.inner.get("src/one.txt").unwrap(),
                    b"one",
                    "the source stays: {at}"
                );
                assert_eq!(drive.inner.get("src/two.txt").unwrap(), b"two", "{at}");
            } else {
                assert!(
                    drive.inner.get("src/one.txt").is_err(),
                    "the source moved: {at}"
                );
            }
            // Settled once: the next request finds nothing to do.
            assert_eq!(
                next.recover_swaps(u64::MAX / 2).unwrap(),
                Recovery::default(),
                "{at}"
            );
        }
    }
}

/// A record another process wrote may be a swap that is running: it is left alone for ten
/// minutes, and settled after them; a record this process is running is never touched; a record
/// that cannot be read stays; a name that is no stamp of this bridge's is not read at all.
#[test]
fn x_delta_apps3_14_a_record_of_another_process_waits_for_its_grace_and_a_running_swap_is_left_alone(
) {
    let drive = Killing::new(usize::MAX);
    let records = SwapRecords::in_memory();
    // A copy of src/ made beside dest/, as a swap that is cut after its first step leaves it.
    drive
        .inner
        .put("dest.azul-new-1000-4242-0/one.txt", b"one")
        .unwrap();
    let intent = SwapIntent {
        key: String::from("src"),
        target: String::from("dest"),
        beside: String::from("dest.azul-new-1000-4242-0"),
        aside: String::from("dest.azul-old-1000-4242-0"),
        folder: true,
        destination_is_folder: true,
        moving: false,
    };
    records
        .put("1000-4294967295-0", &serde_json::to_vec(&intent).unwrap())
        .unwrap();
    records.put("1000-4294967295-1", b"{ not an intent").unwrap();
    records.put("no-stamp", b"whatever").unwrap();
    let dav = dav_keeping(drive.clone(), records.clone());
    // Five minutes after it was written, by another process (4294967295 is no process of this computer): left alone, and
    // the look says when the grace is over.
    assert_eq!(
        dav.recover_swaps(1000 + 300).unwrap(),
        Recovery {
            settled: 0,
            wait_until: Some(1000 + SWAP_GRACE_SECS)
        }
    );
    assert!(drive.inner.get("dest.azul-new-1000-4242-0/one.txt").is_ok());
    // Past the grace: the stray copy goes, the destination was never touched.
    assert_eq!(
        dav.recover_swaps(1000 + 601).unwrap(),
        Recovery {
            settled: 1,
            wait_until: None
        }
    );
    assert!(
        drive.inner.keys().iter().all(|key| !key.contains(".azul-")),
        "{:?}",
        drive.inner.keys()
    );
    assert_eq!(drive.inner.get("dest/keep.txt").unwrap(), b"old");
    // The record that is no intent stays for a person to look at, and so does a name that is no stamp.
    assert_eq!(
        records.stamps().unwrap(),
        vec![String::from("1000-4294967295-1"), String::from("no-stamp")]
    );
    // This process's own running swap is not recovered under it.
    let stamp = format!("1-{}-0", std::process::id());
    dav.running().insert(stamp.clone());
    records
        .put(&stamp, &serde_json::to_vec(&intent).unwrap())
        .unwrap();
    drive
        .inner
        .put("dest.azul-new-1000-4242-0/one.txt", b"one")
        .unwrap();
    assert_eq!(
        dav.recover_swaps(u64::MAX / 2).unwrap().settled,
        0,
        "the running swap's record is left alone"
    );
    assert!(
        drive.inner.get("dest.azul-new-1000-4242-0/one.txt").is_ok(),
        "left alone"
    );
    assert!(records.get(&stamp).unwrap().is_some());
}

/// A record of another process that waited for its grace is looked at AGAIN once the grace is over - by
/// a later request of the same process, not only by the next start (a bridge that a login item starts
/// again a second after a crash found the record minutes young and left its stray for ever).
#[test]
fn x_delta_apps4_1_a_record_left_for_its_grace_is_settled_by_a_later_request() {
    let drive = Killing::new(usize::MAX);
    let records = SwapRecords::in_memory();
    drive
        .inner
        .put("dest.azul-new-1000-4242-0/one.txt", b"one")
        .unwrap();
    let intent = SwapIntent {
        key: String::from("src"),
        target: String::from("dest"),
        beside: String::from("dest.azul-new-1000-4242-0"),
        aside: String::from("dest.azul-old-1000-4242-0"),
        folder: true,
        destination_is_folder: true,
        moving: false,
    };
    records
        .put("1000-4294967295-0", &serde_json::to_vec(&intent).unwrap())
        .unwrap();
    let now = Arc::new(AtomicU64::new(1000 + 5));
    let mut dav = dav_keeping(drive.clone(), records.clone());
    let clock = now.clone();
    dav.clock = Some(Box::new(move || clock.load(Ordering::SeqCst)));
    let options = || dav.respond(&head("OPTIONS", "/", &[]), b"").status;
    // The first request, seconds after the record was made: left alone.
    assert_eq!(options(), Status::OK);
    assert!(drive.inner.get("dest.azul-new-1000-4242-0/one.txt").is_ok());
    // One second before the grace is over: not looked at, nothing changes.
    now.store(1000 + SWAP_GRACE_SECS - 1, Ordering::SeqCst);
    assert_eq!(options(), Status::OK);
    assert!(drive.inner.get("dest.azul-new-1000-4242-0/one.txt").is_ok());
    // The grace is over: the next request settles it, once.
    now.store(1000 + SWAP_GRACE_SECS, Ordering::SeqCst);
    assert_eq!(options(), Status::OK);
    assert!(
        drive.inner.keys().iter().all(|key| !key.contains(".azul-")),
        "{:?}",
        drive.inner.keys()
    );
    assert!(records.stamps().unwrap().is_empty());
    assert_eq!(drive.inner.get("dest/keep.txt").unwrap(), b"old");
    // Nothing waits any more: a later request does not look again (a record put now stays until the
    // next start - there is none that could be running but this process's own).
    records
        .put("1000-4294967295-9", &serde_json::to_vec(&intent).unwrap())
        .unwrap();
    now.store(1000 + 10 * SWAP_GRACE_SECS, Ordering::SeqCst);
    assert_eq!(options(), Status::OK);
    assert_eq!(records.stamps().unwrap(), vec![String::from("1000-4294967295-9")]);
}

/// A [`Flaky`] drive that, while `refuse` is set, refuses to delete what a swap staged (`*.azul-*`):
/// the drive is gone just when the cleanup of a failed copy runs.
struct Stubborn {
    inner: Arc<Flaky>,
    refuse: std::sync::atomic::AtomicBool,
}

impl Stubborn {
    fn new(inner: Arc<Flaky>) -> Arc<Stubborn> {
        Arc::new(Stubborn {
            inner,
            refuse: std::sync::atomic::AtomicBool::new(true),
        })
    }

    fn refusing(&self, key: &str) -> Result<(), azul_storage::DriveError> {
        if key.contains(".azul-") && self.refuse.load(Ordering::SeqCst) {
            return Err(azul_storage::DriveError::Transport(String::from(
                "the drive is gone",
            )));
        }
        Ok(())
    }
}

impl Drive for Stubborn {
    fn list(&self, request: &ListRequest) -> Result<ListPage, azul_storage::DriveError> {
        self.inner.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, azul_storage::DriveError> {
        self.inner.get(key)
    }
    fn get_range(
        &self,
        key: &str,
        range: azul_storage::ByteRange,
    ) -> Result<Vec<u8>, azul_storage::DriveError> {
        self.inner.get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), azul_storage::DriveError> {
        self.inner.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), azul_storage::DriveError> {
        self.refusing(key)?;
        self.inner.delete(key)
    }
    fn head(&self, key: &str) -> Result<azul_storage::ObjectInfo, azul_storage::DriveError> {
        self.inner.head(key)
    }
    fn copy(&self, from: &str, to: &str) -> Result<(), azul_storage::DriveError> {
        self.inner.copy(from, to)
    }
    fn rename(&self, from: &str, to: &str) -> Result<(), azul_storage::DriveError> {
        self.inner.rename(from, to)
    }
    fn delete_folder(&self, prefix: &str) -> Result<(), azul_storage::DriveError> {
        self.refusing(prefix)?;
        self.inner.delete_folder(prefix)
    }
}

/// X-DELTA-APPS3-14, the row the kill table does not reach: a step that FAILS leaves the destination
/// as it was and deletes what this call made - and when the drive refuses that delete too (it is gone
/// just then) the partial copy stays, and so must the intent that names it: the record went with the
/// "settled" swap and nothing swept the copy for ever. A swap is settled only when nothing it made is
/// left under a staging name; the next process's first request deletes the copy and then the record.
#[test]
fn x_delta_apps3_14_a_cleanup_the_drive_refuses_keeps_the_intent_for_the_next_start() {
    // A folder COPY whose second file cannot be copied, on a drive that refuses the cleanup too.
    let flaky = Flaky::new(1, &[]);
    let drive = Stubborn::new(flaky.clone());
    let records = SwapRecords::in_memory();
    let dav = dav_keeping(drive.clone(), records.clone());
    let answer = dav.respond(&head("COPY", "/src/", &dest("/dest/")), b"");
    assert_eq!(answer.status, Status::BAD_GATEWAY, "{}", text(&answer));
    let keys = flaky.inner.keys();
    assert!(
        keys.iter().any(|key| key.contains(".azul-new-")),
        "the partial copy is left: {keys:?}"
    );
    assert_eq!(
        records.stamps().unwrap().len(),
        1,
        "and so is the intent that names it: {keys:?}"
    );
    assert_eq!(flaky.inner.get("dest/keep.txt").unwrap(), b"old");
    // The drive answers again: the first request of the next process settles it.
    drive.refuse.store(false, Ordering::SeqCst);
    let next = dav_keeping(drive.clone(), records.clone());
    assert_eq!(
        next.respond(&head("OPTIONS", "/", &[]), b"").status,
        Status::OK
    );
    let keys = flaky.inner.keys();
    assert!(
        keys.iter().all(|key| !key.contains(".azul-")),
        "nothing under a staging name: {keys:?}"
    );
    assert!(records.stamps().unwrap().is_empty(), "no intent");
    assert_eq!(flaky.inner.get("dest/keep.txt").unwrap(), b"old");
    assert_eq!(flaky.inner.get("src/one.txt").unwrap(), b"one");
    assert_eq!(flaky.inner.get("src/two.txt").unwrap(), b"two");
}

// ==== X-DELTA-APPS4-1: the swap on the drives the bridge really serves ====
//
// Every swap test above runs on a `MemoryDrive`, which accepts every name. The intent record went to
// `.azlin/dav-swaps/`, a name the encrypted drive's index and a folder served by `--folder` both refuse
// as the drive's own bookkeeping, so on every real drive a replacing COPY or MOVE answered 409 and no
// swap was ever recorded, let alone settled. These run the same requests over a folder on disk
// (`serve --folder`: `LocalDrive`) and over the encrypted drive the bridge opens for an Azlin account
// (over a bucket in memory), with the records in a folder as `serve` keeps them.

/// A drive of a kind the bridge serves, as a test can make it without a server, and the records of
/// its swaps. Dropping it removes what it made.
struct Rig {
    /// Which kind, for the message of a failure.
    kind: &'static str,
    /// A new handle on the same storage: what a process that starts again opens.
    open: Box<dyn Fn() -> Arc<dyn Drive>>,
    /// The records, a folder below a state folder: the same value is the same records.
    records: SwapRecords,
    _keep: Vec<Box<dyn std::any::Any>>,
}

impl Rig {
    /// `serve --folder DIR`: a folder on disk.
    fn folder() -> Rig {
        let dir = azul_storage::testing::TempDir::new("bridge-dav-swap-folder");
        let root = dir.0.join("shared");
        std::fs::create_dir_all(&root).unwrap();
        let records = SwapRecords::of_drive(&dir.0.join("state"), Some("folder:shared"));
        Rig {
            kind: "a served folder",
            open: Box::new(move || {
                Arc::new(azul_storage::LocalDrive::without_manifest(root.clone())) as Arc<dyn Drive>
            }),
            records,
            _keep: vec![Box::new(dir)],
        }
    }

    /// An Azlin account's drive: encrypted, opened as `serve` opens it, over a bucket in memory; the
    /// index is the bucket's, so a handle opened again sees what the one before committed.
    #[cfg(feature = "encryption")]
    fn encrypted() -> Rig {
        use azmail_core::mail_drive;
        use azul_storage::{
            crypto::{device, keys::RecoveryKdf},
            keyring::{KeyringStore, MemoryKeyring},
            meta::MemoryBucket,
        };
        let keyring: Arc<dyn KeyringStore> = Arc::new(MemoryKeyring::new());
        let bucket = Arc::new(MemoryBucket::new());
        let cheap = RecoveryKdf::with_cost(64, 1, 1).unwrap();
        device::setup_new_drive(bucket.as_ref(), keyring.as_ref(), "d_swap", cheap).unwrap();
        let state = azul_storage::testing::TempDir::new("bridge-dav-swap-encrypted");
        let records = SwapRecords::of_drive(&state.0, Some("azlin:d_swap"));
        Rig {
            kind: "the encrypted Azlin drive",
            open: Box::new(move || -> Arc<dyn Drive> {
                mail_drive::wrap_auto(
                    bucket.clone(),
                    "d_swap",
                    keyring.clone(),
                    azul_storage::meta::RestoreSource::none("a test has no token server to ask"),
                )
            }),
            records,
            _keep: vec![Box::new(state)],
        }
    }
}

/// The drives of a swap test: every kind the bridge serves.
fn rigs() -> Vec<fn() -> Rig> {
    let folder: fn() -> Rig = Rig::folder;
    #[cfg(feature = "encryption")]
    let makers = vec![folder, Rig::encrypted as fn() -> Rig];
    #[cfg(not(feature = "encryption"))]
    let makers = vec![folder];
    makers
}

/// `src/` of two files, `dest/` of one, `docs/` of one and `top.txt`: what the swaps move around.
fn lay_out(drive: &dyn Drive) {
    for (key, bytes) in [
        ("src/one.txt", &b"one"[..]),
        ("src/two.txt", b"two"),
        ("dest/keep.txt", b"old"),
        ("docs/a.txt", b"hello"),
        ("top.txt", b"top"),
    ] {
        drive.put(key, bytes).unwrap();
    }
}

/// Every file of the drive with its bytes, by key (a folder's marker is no file).
fn files_of(drive: &dyn Drive) -> std::collections::BTreeMap<String, Vec<u8>> {
    azul_storage::ops::list_all(drive, "")
        .unwrap()
        .into_iter()
        .filter(|object| !object.key.ends_with('/'))
        .map(|object| {
            let bytes = drive.get(&object.key).unwrap();
            (object.key, bytes)
        })
        .collect()
}

/// Every key the listing of the drive shows, folders' markers included.
fn keys_of(drive: &dyn Drive) -> Vec<String> {
    azul_storage::ops::list_all(drive, "")
        .unwrap()
        .into_iter()
        .map(|object| object.key)
        .collect()
}

/// A drive that DIES - the process is gone: a panic nothing catches but the test - once `die_after` of
/// the writes a request makes (`put`, `copy`, `create_folder`, `rename`, `delete`, `delete_folder`)
/// are made: 0 is before the first, `n` right after the n-th one. The drive below is a real one and
/// every call goes to it as it is (what the swap does there, its own `rename` and `copy` included).
/// `hook` runs before each write: a request that comes while the swap is in the middle.
struct Dying {
    inner: Arc<dyn Drive>,
    done: AtomicUsize,
    die_after: usize,
    hook: std::sync::Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

impl Dying {
    fn new(inner: Arc<dyn Drive>, die_after: usize) -> Arc<Dying> {
        Arc::new(Dying {
            inner,
            done: AtomicUsize::new(0),
            die_after,
            hook: std::sync::Mutex::new(None),
        })
    }

    /// How many writes were made.
    fn writes(&self) -> usize {
        self.done.load(Ordering::SeqCst)
    }

    fn write<T>(
        &self,
        what: &str,
        call: impl FnOnce() -> Result<T, DriveError>,
    ) -> Result<T, DriveError> {
        // Taken out of the lock before it runs: a look that goes wrong and writes calls this again.
        let hook = self.hook.lock().unwrap().clone();
        if let Some(hook) = hook {
            hook();
        }
        if self.die_after == 0 && self.writes() == 0 {
            panic!("the process died before its first write ({what})");
        }
        let result = call();
        let made = self.done.fetch_add(1, Ordering::SeqCst) + 1;
        if made == self.die_after {
            panic!("the process died after write {made} ({what})");
        }
        result
    }
}

impl azul_storage::DriveDelegate for Dying {
    fn below(&self) -> &dyn Drive {
        &*self.inner
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.write(&format!("put {key}"), || self.inner.put(key, bytes))
    }
    fn copy(&self, from: &str, to: &str) -> Result<(), DriveError> {
        self.write(&format!("copy {from} {to}"), || self.inner.copy(from, to))
    }
    fn create_folder(&self, prefix: &str) -> Result<(), DriveError> {
        self.write(&format!("create_folder {prefix}"), || {
            self.inner.create_folder(prefix)
        })
    }
    fn rename(&self, from: &str, to: &str) -> Result<(), DriveError> {
        self.write(&format!("rename {from} {to}"), || {
            self.inner.rename(from, to)
        })
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.write(&format!("delete {key}"), || self.inner.delete(key))
    }
    fn delete_folder(&self, prefix: &str) -> Result<(), DriveError> {
        self.write(&format!("delete_folder {prefix}"), || {
            self.inner.delete_folder(prefix)
        })
    }
}

azul_storage::delegate_drive!(Dying);

/// RED on the code before: the record went to `.azlin/dav-swaps/`, a name both drives refuse, so the
/// first replacing COPY of every real drive answered 409 and nothing was replaced. Folder over folder
/// (by COPY and by MOVE), a file over a folder and a folder over a file all answer 204, put the new
/// content in place whole, keep the source of a COPY, and leave neither a staging name nor a record.
#[test]
fn x_delta_apps4_1_a_replacing_copy_or_move_works_on_the_drives_the_bridge_serves_and_leaves_no_record(
) {
    for rig in rigs().into_iter().map(|make| make()) {
        let drive = (rig.open)();
        lay_out(&*drive);
        let dav = dav_keeping(drive.clone(), rig.records.clone());
        let kind = rig.kind;
        let settled = |what: &str| {
            let keys = keys_of(&*drive);
            assert!(
                keys.iter().all(|key| !key.contains(".azul-")),
                "{kind}, {what}: a staging name is left: {keys:?}"
            );
            assert!(
                rig.records.stamps().unwrap().is_empty(),
                "{kind}, {what}: a record is left"
            );
        };
        // Folder over folder: COPY.
        let answer = dav.respond(&head("COPY", "/src/", &dest("/dest/")), b"");
        assert_eq!(
            answer.status,
            Status::NO_CONTENT,
            "{kind}: {}",
            text(&answer)
        );
        let files = files_of(&*drive);
        assert_eq!(
            files.get("dest/one.txt").map(Vec::as_slice),
            Some(&b"one"[..]),
            "{kind}: {files:?}"
        );
        assert_eq!(
            files.get("dest/two.txt").map(Vec::as_slice),
            Some(&b"two"[..]),
            "{kind}"
        );
        assert!(
            !files.contains_key("dest/keep.txt"),
            "{kind}: the old content is replaced: {files:?}"
        );
        assert!(
            files.contains_key("src/one.txt"),
            "{kind}: a COPY keeps the source: {files:?}"
        );
        settled("COPY folder over folder");
        // Folder over folder: MOVE.
        let answer = dav.respond(&head("MOVE", "/docs/", &dest("/src/")), b"");
        assert_eq!(
            answer.status,
            Status::NO_CONTENT,
            "{kind}: {}",
            text(&answer)
        );
        let files = files_of(&*drive);
        assert_eq!(
            files.get("src/a.txt").map(Vec::as_slice),
            Some(&b"hello"[..]),
            "{kind}: {files:?}"
        );
        assert!(!files.contains_key("src/one.txt"), "{kind}: {files:?}");
        assert!(
            !files.keys().any(|key| key.starts_with("docs/")),
            "{kind}: the source went: {files:?}"
        );
        settled("MOVE folder over folder");
        // A file over a folder.
        let answer = dav.respond(&head("COPY", "/top.txt", &dest("/dest")), b"");
        assert_eq!(
            answer.status,
            Status::NO_CONTENT,
            "{kind}: {}",
            text(&answer)
        );
        let files = files_of(&*drive);
        assert_eq!(
            files.get("dest").map(Vec::as_slice),
            Some(&b"top"[..]),
            "{kind}: {files:?}"
        );
        assert!(
            !files.keys().any(|key| key.starts_with("dest/")),
            "{kind}: {files:?}"
        );
        settled("COPY file over folder");
        // A folder over a file.
        let answer = dav.respond(&head("COPY", "/src/", &dest("/dest")), b"");
        assert_eq!(
            answer.status,
            Status::NO_CONTENT,
            "{kind}: {}",
            text(&answer)
        );
        let files = files_of(&*drive);
        assert_eq!(
            files.get("dest/a.txt").map(Vec::as_slice),
            Some(&b"hello"[..]),
            "{kind}: {files:?}"
        );
        assert!(
            !files.contains_key("dest"),
            "{kind}: the file is replaced by the folder: {files:?}"
        );
        settled("COPY folder over file");
        // And a folder over a folder by MOVE once more, now with the one the COPY made as the source.
        let answer = dav.respond(&head("MOVE", "/dest/", &dest("/src/")), b"");
        assert_eq!(
            answer.status,
            Status::NO_CONTENT,
            "{kind}: {}",
            text(&answer)
        );
        settled("MOVE again");
    }
}

/// What a kill inside a replacing COPY or MOVE leaves, stopped after EACH write the request makes
/// (`swap_in`'s table: the record, the new content beside, the old one aside, the new one in place, the
/// old one deleted, the record removed), on the drives the bridge serves, and what the NEXT process -
/// a new server over a handle opened again on the same storage and the same records - makes of it at
/// its first request: the destination is whole (the old content or the new, never both and never
/// neither), the source of a COPY is untouched and a MOVE's is either where it was (undone) or gone
/// with its content at the destination (finished), nothing is under a staging name, no record is left,
/// and the flip from undone to finished is where the table says: once the old one stepped aside.
#[test]
fn x_delta_apps4_1_a_kill_after_each_step_of_a_swap_is_settled_by_the_next_dav_on_the_real_drives()
{
    for make in rigs() {
        for method in ["COPY", "MOVE"] {
            // The writes of a swap that is not killed, by a run of its own.
            let total = {
                let rig = make();
                let base = (rig.open)();
                lay_out(&*base);
                let drive = Dying::new(base.clone(), usize::MAX);
                let dav = dav_keeping(drive.clone(), rig.records.clone());
                let answer = dav.respond(&head(method, "/src/", &dest("/dest/")), b"");
                assert_eq!(
                    answer.status,
                    Status::NO_CONTENT,
                    "{}: {method}: {}",
                    rig.kind,
                    text(&answer)
                );
                drive.writes()
            };
            assert!(total >= 4, "a swap writes at least four times: {total}");
            for die_after in 0..=total {
                let rig = make();
                let kind = rig.kind;
                let at = format!("{kind}: {method} killed after write {die_after} of {total}");
                let base = (rig.open)();
                lay_out(&*base);
                let dying = Dying::new(base.clone(), die_after);
                let dav = dav_keeping(dying.clone(), rig.records.clone());
                let died = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    dav.respond(&head(method, "/src/", &dest("/dest/")), b"")
                }));
                assert!(died.is_err(), "the process died: {at}");
                drop(dav);
                // The record is there for the next start from the moment the first write is made.
                assert_eq!(
                    rig.records.stamps().unwrap().len(),
                    1,
                    "the record outlives the process: {at}"
                );
                // The next process: its first request settles it.
                let reopened = (rig.open)();
                let next = dav_keeping(reopened.clone(), rig.records.clone());
                let answer = next.respond(&head("OPTIONS", "/", &[]), b"");
                assert_eq!(answer.status, Status::OK, "{at}");
                let files = files_of(&*reopened);
                let keys = keys_of(&*reopened);
                let at = format!("{at}: {files:?}");
                assert!(
                    keys.iter().all(|key| !key.contains(".azul-")),
                    "a staging name is left: {at}"
                );
                assert!(
                    rig.records.stamps().unwrap().is_empty(),
                    "a record is left: {at}"
                );
                let new_dest = files.get("dest/one.txt").map(Vec::as_slice) == Some(&b"one"[..])
                    && files.get("dest/two.txt").map(Vec::as_slice) == Some(&b"two"[..]);
                let old_dest = files.get("dest/keep.txt").map(Vec::as_slice) == Some(&b"old"[..]);
                assert!(
                    new_dest != old_dest,
                    "the destination is whole, old or new: {at}"
                );
                assert_eq!(
                    files.keys().filter(|key| key.starts_with("dest/")).count(),
                    if new_dest { 2 } else { 1 },
                    "nothing else is in it: {at}"
                );
                let source_whole = files.get("src/one.txt").map(Vec::as_slice) == Some(&b"one"[..])
                    && files.get("src/two.txt").map(Vec::as_slice) == Some(&b"two"[..]);
                if method == "COPY" || !new_dest {
                    assert!(source_whole, "the source is where it was: {at}");
                } else {
                    assert!(
                        !files.keys().any(|key| key.starts_with("src/")),
                        "a finished MOVE took the source: {at}"
                    );
                }
                assert_eq!(
                    files.get("top.txt").map(Vec::as_slice),
                    Some(&b"top"[..]),
                    "{at}"
                );
                assert_eq!(
                    files.get("docs/a.txt").map(Vec::as_slice),
                    Some(&b"hello"[..]),
                    "{at}"
                );
                // Undone before the old one stepped aside, finished from then on: the old one steps
                // aside by the third write from the end (aside, into place, deleted).
                assert_eq!(
                    new_dest,
                    die_after + 2 >= total,
                    "the flip is where the table says: {at}"
                );
                // Settled once: the next request finds nothing to do.
                assert_eq!(
                    next.recover_swaps(u64::MAX / 2).unwrap(),
                    Recovery::default(),
                    "{at}"
                );
            }
        }
    }
}

/// A kill leaves the record of ANOTHER process (here: the same record under a stamp of another process id,
/// as a bridge that a login item started a second after a crash finds the one before it): it is left
/// alone while it is younger than the grace - the stray beside the destination stays - and the first
/// request after the grace settles it, on the drives the bridge serves.
#[test]
fn x_delta_apps4_1_the_record_of_another_process_is_left_for_its_grace_and_settled_after_it_on_the_real_drives(
) {
    for make in rigs() {
        let rig = make();
        let kind = rig.kind;
        let base = (rig.open)();
        lay_out(&*base);
        // Cut after the first FILE of the new content is beside the destination (write 1 only made its
        // folder, and a folder on disk is no key of the listing: the stray would not show).
        let dying = Dying::new(base.clone(), 2);
        let dav = dav_keeping(dying, rig.records.clone());
        let died = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            dav.respond(&head("COPY", "/src/", &dest("/dest/")), b"")
        }));
        assert!(died.is_err(), "{kind}");
        drop(dav);
        let stamps = rig.records.stamps().unwrap();
        assert_eq!(stamps.len(), 1, "{kind}");
        let bytes = rig.records.get(&stamps[0]).unwrap().unwrap();
        let written_at: u64 = stamps[0].split('-').next().unwrap().parse().unwrap();
        rig.records.remove(&stamps[0]).unwrap();
        let foreign = format!("{written_at}-{}-0", std::process::id() + 1);
        rig.records.put(&foreign, &bytes).unwrap();
        // The next process, five minutes after the record was written.
        let now = Arc::new(AtomicU64::new(written_at + 300));
        let mut next = dav_keeping((rig.open)(), rig.records.clone());
        let clock = now.clone();
        next.clock = Some(Box::new(move || clock.load(Ordering::SeqCst)));
        let options = || next.respond(&head("OPTIONS", "/", &[]), b"").status;
        assert_eq!(options(), Status::OK, "{kind}");
        let after_first = keys_of(&*base);
        assert!(
            after_first.iter().any(|key| key.contains(".azul-new-")),
            "{kind}: the stray is left alone while the record is young: {after_first:?}"
        );
        assert_eq!(
            rig.records.stamps().unwrap(),
            vec![foreign.clone()],
            "{kind}"
        );
        // The grace is over: the next request settles it.
        now.store(written_at + SWAP_GRACE_SECS, Ordering::SeqCst);
        assert_eq!(options(), Status::OK, "{kind}");
        // Read through a handle opened AFTER the settle: the encrypted drive's handle of the killed
        // process keeps the index it had (it is polled, not pushed), and the settle was made through the
        // next process's own.
        let fresh = (rig.open)();
        let keys = keys_of(&*fresh);
        assert!(
            keys.iter().all(|key| !key.contains(".azul-")),
            "{kind}: {keys:?}"
        );
        assert!(rig.records.stamps().unwrap().is_empty(), "{kind}");
        let files = files_of(&*fresh);
        assert_eq!(
            files.get("dest/keep.txt").map(Vec::as_slice),
            Some(&b"old"[..]),
            "{kind}: {files:?}"
        );
        assert!(files.contains_key("src/one.txt"), "{kind}: {files:?}");
    }
}

/// A look at the records that comes while a swap is in the middle (a second connection's request, or the
/// first request of a process that serves a swap at once) leaves that swap's record alone, after each of
/// its writes: the stamp is running before its record is written.
#[test]
fn x_delta_apps4_1_a_look_at_the_records_during_a_swap_leaves_the_running_swap_alone() {
    for make in rigs() {
        let rig = make();
        let kind = rig.kind;
        let base = (rig.open)();
        lay_out(&*base);
        let dying = Dying::new(base.clone(), usize::MAX);
        let dav = Arc::new(dav_keeping(dying.clone(), rig.records.clone()));
        let looks = Arc::new(AtomicUsize::new(0));
        {
            let (dav, looks, records) = (dav.clone(), looks.clone(), rig.records.clone());
            *dying.hook.lock().unwrap() = Some(Arc::new(move || {
                // Far in the future: every grace is over, only the running swap protects its record.
                let found = dav.recover_swaps(u64::MAX / 2).unwrap();
                assert_eq!(
                    found,
                    Recovery::default(),
                    "{kind}: a running swap was settled under it"
                );
                assert_eq!(
                    records.stamps().unwrap().len(),
                    1,
                    "{kind}: its record is there"
                );
                looks.fetch_add(1, Ordering::SeqCst);
            }));
        }
        let answer = dav.respond(&head("MOVE", "/src/", &dest("/dest/")), b"");
        assert_eq!(
            answer.status,
            Status::NO_CONTENT,
            "{kind}: {}",
            text(&answer)
        );
        assert!(
            looks.load(Ordering::SeqCst) >= 4,
            "{kind}: the look was made at each write"
        );
        assert!(
            rig.records.stamps().unwrap().is_empty(),
            "{kind}: and the record went when the swap did"
        );
    }
}

/// A record that cannot be written is a swap that does not start: the answer says so (a 5xx, not a
/// 2xx and not the drive's 409), nothing of the drive is touched, and nothing is left. Here the folder
/// of the records cannot be made (a file is where the state folder should be).
#[test]
fn x_delta_apps4_1_a_record_that_cannot_be_written_starts_no_swap_on_the_real_drives() {
    for make in rigs() {
        let rig = make();
        let kind = rig.kind;
        let base = (rig.open)();
        lay_out(&*base);
        let blocker = azul_storage::testing::TempDir::new("bridge-dav-swap-blocked");
        let file = blocker.0.join("state");
        std::fs::write(&file, b"a file where the state folder should be").unwrap();
        let blocked = SwapRecords::of_drive(&file, Some("azlin:d_blocked"));
        let dav = dav_keeping(base.clone(), blocked.clone());
        let before = files_of(&*base);
        for (method, source) in [("COPY", "/src/"), ("MOVE", "/src/")] {
            let answer = dav.respond(&head(method, source, &dest("/dest/")), b"");
            assert_eq!(
                answer.status,
                Status::BAD_GATEWAY,
                "{kind}: {method}: {}",
                text(&answer)
            );
            assert!(
                text(&answer).contains("nothing was changed"),
                "{kind}: {method}: {}",
                text(&answer)
            );
            assert_eq!(
                files_of(&*base),
                before,
                "{kind}: {method} touched the drive"
            );
            assert!(
                keys_of(&*base).iter().all(|key| !key.contains(".azul-")),
                "{kind}: {method}: a staging name"
            );
        }
        assert!(
            dav.running().is_empty(),
            "{kind}: the swap is not left running"
        );
    }
}

/// `serve` keeps the records of each drive in a folder of the state folder, beside the UID maps - one
/// folder for each drive, so a record is never settled on another drive than the one it was made on -
/// and in memory for a drive that is gone with the process (`--memory`).
#[test]
fn x_delta_apps4_1_the_records_of_a_served_drive_are_files_in_the_state_folder_one_folder_for_each_drive(
) {
    let state = azul_storage::testing::TempDir::new("bridge-dav-swap-state");
    let documents = SwapRecords::of_drive(&state.0, Some("folder:/home/ada/Documents"));
    let account = SwapRecords::of_drive(&state.0, Some("azlin:d_1"));
    documents.put("1000-4294967295-0", b"{}").unwrap();
    assert_eq!(
        documents.stamps().unwrap(),
        vec![String::from("1000-4294967295-0")]
    );
    assert!(
        account.stamps().unwrap().is_empty(),
        "another drive's records are not this drive's"
    );
    // A file below `<state>/dav-swaps/<drive>/`, and nothing else in the state folder.
    let folders: Vec<_> = std::fs::read_dir(state.0.join(SWAPS_DIR))
        .unwrap()
        .flatten()
        .collect();
    assert_eq!(
        folders.len(),
        1,
        "one folder for the one drive that has a record"
    );
    assert!(folders[0].path().join("1000-4294967295-0.json").is_file());
    // A process that starts again finds it.
    let again = SwapRecords::of_drive(&state.0, Some("folder:/home/ada/Documents"));
    assert_eq!(
        again.get("1000-4294967295-0").unwrap().as_deref(),
        Some(&b"{}"[..])
    );
    again.remove("1000-4294967295-0").unwrap();
    assert!(documents.stamps().unwrap().is_empty());
    // A record that is gone is removed without a word.
    again.remove("1000-4294967295-0").unwrap();
    // What a write stages beside a record is no record.
    std::fs::write(
        folders[0]
            .path()
            .join(".1000-4242-1.json.azul-storage-1-0.tmp"),
        b"half",
    )
    .unwrap();
    assert!(documents.stamps().unwrap().is_empty());
    // `--memory`: nothing is written anywhere.
    let memory = SwapRecords::of_drive(&state.0, None);
    memory.put("1000-4242-2", b"{}").unwrap();
    assert_eq!(memory.stamps().unwrap(), vec![String::from("1000-4242-2")]);
    assert!(documents.stamps().unwrap().is_empty());
}

/// RED on the code before: PUT and DELETE ignored `If-Match`, so a save another client made after
/// this one's PROPFIND was overwritten or deleted without a word.
#[test]
fn x_delta_sync4_21_put_and_delete_ask_if_match_and_if_none_match_before_they_change_a_file() {
    let drive = drive();
    let dav = dav_on(drive.clone(), 0);
    let etag = dav
        .respond(&head("GET", "/docs/a.txt", &[]), b"")
        .header("ETag")
        .unwrap()
        .to_string();
    let stale = [("If-Match", "\"stale\"")];
    assert_eq!(
        dav.respond(&head("PUT", "/docs/a.txt", &stale), b"mine")
            .status,
        Status::PRECONDITION_FAILED
    );
    assert_eq!(
        drive.get("docs/a.txt").unwrap(),
        b"hello world",
        "not overwritten"
    );
    assert_eq!(
        dav.respond(&head("DELETE", "/docs/a.txt", &stale), b"")
            .status,
        Status::PRECONDITION_FAILED
    );
    assert!(drive.get("docs/a.txt").is_ok(), "not deleted");
    assert_eq!(
        dav.respond(
            &head("PUT", "/docs/a.txt", &[("If-None-Match", "*")]),
            b"mine"
        )
        .status,
        Status::PRECONDITION_FAILED,
        "the name is taken"
    );
    assert_eq!(
        dav.respond(
            &head("PUT", "/docs/new.txt", &[("If-None-Match", "*")]),
            b"new"
        )
        .status,
        Status::CREATED
    );
    assert_eq!(
        dav.respond(
            &head("PUT", "/docs/a.txt", &[("If-Match", etag.as_str())]),
            b"mine"
        )
        .status,
        Status::NO_CONTENT
    );
    assert_eq!(drive.get("docs/a.txt").unwrap(), b"mine");
    let etag = dav
        .respond(&head("GET", "/docs/a.txt", &[]), b"")
        .header("ETag")
        .unwrap()
        .to_string();
    assert_eq!(
        dav.respond(
            &head("DELETE", "/docs/a.txt", &[("If-Match", etag.as_str())]),
            b""
        )
        .status,
        Status::NO_CONTENT
    );
    assert!(drive.get("docs/a.txt").is_err());
}
