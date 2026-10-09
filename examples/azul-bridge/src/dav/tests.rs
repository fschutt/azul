//! WebDAV requests against a drive in memory: what Finder, Explorer and the file managers
//! send, what the bridge answers, what lands in the drive.

use std::{
    io::{Read, Write},
    net::TcpStream,
    sync::Arc,
    time::Duration,
};

use azul_storage::Drive;

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
