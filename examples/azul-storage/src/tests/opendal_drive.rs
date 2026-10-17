//! A data source through Apache OpenDAL is a [`Drive`] like a bucket: OpenDAL's memory service
//! stands in for a real one (no network), and its HTTP services send through the app's
//! [`Transport`] - a fake here, azul's HTTP client in the apps.

use std::{collections::BTreeMap, sync::Mutex};

use crate::{
    config::SecretOptions, ByteRange, Drive, DriveError, HttpCall, HttpReply, ListRequest,
    Method, OpendalDrive, Transport,
};

struct NoNetwork;

impl Transport for NoNetwork {
    fn send(&self, _call: &HttpCall) -> Result<HttpReply, String> {
        Err("no network in this test".to_string())
    }
}

fn memory() -> OpendalDrive {
    OpendalDrive::open(
        "memory",
        &BTreeMap::new(),
        &SecretOptions::new(),
        Box::new(NoNetwork),
    )
    .expect("OpenDAL's memory service")
}

fn keys(page: &crate::ListPage) -> Vec<&str> {
    page.objects.iter().map(|o| o.key.as_str()).collect()
}

#[test]
fn a_memory_source_lists_its_folders_and_files_like_a_bucket() {
    let drive = memory();
    drive.put("docs/a.txt", b"alpha").unwrap();
    drive.put("docs/b.txt", b"beta!").unwrap();
    drive.put("top.txt", b"t").unwrap();

    let root = drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(root.folders, vec!["docs/".to_string()]);
    assert_eq!(keys(&root), vec!["top.txt"]);
    assert_eq!(root.next, None);

    let docs = drive.list(&ListRequest::folder("docs/")).unwrap();
    assert!(docs.folders.is_empty(), "{:?}", docs.folders);
    assert_eq!(keys(&docs), vec!["docs/a.txt", "docs/b.txt"]);
    assert_eq!(docs.objects[1].size, 5);

    let all = drive.list(&ListRequest::recursive("")).unwrap();
    assert_eq!(keys(&all), vec!["docs/a.txt", "docs/b.txt", "top.txt"]);
    assert!(all.folders.is_empty(), "a recursive listing has no folders");
}

#[test]
fn a_listing_pages_after_the_last_key_it_gave() {
    let drive = memory();
    for name in ["a", "b", "c", "d", "e"] {
        drive.put(&format!("{name}.txt"), name.as_bytes()).unwrap();
    }
    let first = drive.list(&ListRequest::folder("").with_max_keys(2)).unwrap();
    assert_eq!(keys(&first), vec!["a.txt", "b.txt"]);
    let token = first.next.clone().expect("more to come");
    let second = drive
        .list(&ListRequest::folder("").with_max_keys(2).with_continuation(token))
        .unwrap();
    assert_eq!(keys(&second), vec!["c.txt", "d.txt"]);
    let third = drive
        .list(
            &ListRequest::folder("")
                .with_max_keys(2)
                .with_continuation(second.next.clone().unwrap()),
        )
        .unwrap();
    assert_eq!(keys(&third), vec!["e.txt"]);
    assert_eq!(third.next, None);
}

#[test]
fn reading_part_of_an_object_and_its_head_work_through_opendal() {
    let drive = memory();
    drive.put("notes.txt", b"hello world").unwrap();
    assert_eq!(drive.get("notes.txt").unwrap(), b"hello world");
    assert_eq!(
        drive
            .get_range("notes.txt", ByteRange::new(6, Some(10)))
            .unwrap(),
        b"world"
    );
    assert_eq!(
        drive.get_range("notes.txt", ByteRange::new(6, None)).unwrap(),
        b"world"
    );
    let head = drive.head("notes.txt").unwrap();
    assert_eq!(head.key, "notes.txt");
    assert_eq!(head.size, 11);
}

#[test]
fn copy_rename_delete_and_new_folders_work_without_native_support() {
    let drive = memory();
    drive.put("a.txt", b"one").unwrap();
    drive.copy("a.txt", "b.txt").unwrap();
    assert_eq!(drive.get("b.txt").unwrap(), b"one");
    drive.rename("b.txt", "c.txt").unwrap();
    assert!(matches!(drive.get("b.txt"), Err(DriveError::NotFound { .. })));
    assert_eq!(drive.get("c.txt").unwrap(), b"one");
    drive.delete("c.txt").unwrap();
    drive.delete("c.txt").unwrap();
    assert!(matches!(drive.head("c.txt"), Err(DriveError::NotFound { .. })));
    drive.create_folder("new/").unwrap();
    let root = drive.list(&ListRequest::folder("")).unwrap();
    assert!(root.folders.contains(&"new/".to_string()), "{:?}", root);
}

#[test]
fn a_missing_object_is_not_found() {
    assert!(matches!(
        memory().get("nope.txt"),
        Err(DriveError::NotFound { key }) if key == "nope.txt"
    ));
}

#[test]
fn a_scheme_opendal_does_not_know_is_a_sentence_not_a_panic() {
    match OpendalDrive::open(
        "no-such-service",
        &BTreeMap::new(),
        &SecretOptions::new(),
        Box::new(NoNetwork),
    ) {
        Err(DriveError::InvalidConfig(why) | DriveError::Unsupported(why)) => {
            assert!(why.contains("no-such-service"), "{why}");
        }
        Err(other) => panic!("another error: {other}"),
        Ok(_) => panic!("opened an unknown service"),
    }
}

/// A web server in memory: GET (with Range) and HEAD of the files it holds, 404 otherwise.
struct FakeWeb {
    files: BTreeMap<String, Vec<u8>>,
    calls: Mutex<Vec<HttpCall>>,
}

impl FakeWeb {
    fn new(files: &[(&str, &[u8])]) -> Self {
        FakeWeb {
            files: files
                .iter()
                .map(|(url, body)| (url.to_string(), body.to_vec()))
                .collect(),
            calls: Mutex::new(Vec::new()),
        }
    }
}

impl Transport for FakeWeb {
    fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
        self.calls.lock().unwrap().push(call.clone());
        let Some(body) = self.files.get(&call.url) else {
            return Ok(HttpReply {
                status: 404,
                headers: vec![("content-type".to_string(), "text/plain".to_string())],
                body: b"not found".to_vec(),
            });
        };
        let length = body.len();
        let range = call
            .headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case("range"))
            .and_then(|(_, v)| v.strip_prefix("bytes="))
            .and_then(|v| v.split_once('-'))
            .map(|(a, b)| {
                let start: usize = a.parse().unwrap_or(0);
                let end: usize = b.parse().unwrap_or(length.saturating_sub(1));
                (start, end.min(length.saturating_sub(1)))
            });
        let mut headers = vec![
            ("content-type".to_string(), "text/plain".to_string()),
            (
                "last-modified".to_string(),
                "Thu, 08 Oct 2026 09:15:00 GMT".to_string(),
            ),
        ];
        let (status, part) = match range {
            Some((start, end)) if start <= end && end < length => {
                headers.push((
                    "content-range".to_string(),
                    format!("bytes {start}-{end}/{length}"),
                ));
                (206, body[start..=end].to_vec())
            }
            _ => (200, body.clone()),
        };
        headers.push(("content-length".to_string(), part.len().to_string()));
        Ok(HttpReply {
            status,
            headers,
            body: if call.method == Method::Head {
                Vec::new()
            } else {
                part
            },
        })
    }
}

#[test]
fn opendals_http_goes_through_the_apps_transport() {
    let web = std::sync::Arc::new(FakeWeb::new(&[(
        "http://files.example/readme.txt",
        b"hello web",
    )]));
    struct Shared(std::sync::Arc<FakeWeb>);
    impl Transport for Shared {
        fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
            self.0.send(call)
        }
    }
    let mut options = BTreeMap::new();
    options.insert("endpoint".to_string(), "http://files.example".to_string());
    let drive = OpendalDrive::open(
        "http",
        &options,
        &SecretOptions::new(),
        Box::new(Shared(web.clone())),
    )
    .unwrap();
    assert_eq!(drive.get("readme.txt").unwrap(), b"hello web");
    let calls = web.calls.lock().unwrap().clone();
    assert!(
        calls
            .iter()
            .any(|c| c.method == Method::Get && c.url == "http://files.example/readme.txt"),
        "{calls:?}"
    );
    for call in &calls {
        for (name, _) in &call.headers {
            assert!(
                !name.eq_ignore_ascii_case("host") && !name.eq_ignore_ascii_case("content-length"),
                "the HTTP client sets {name} itself: {call:?}"
            );
        }
    }
    assert!(matches!(
        drive.get("missing.txt"),
        Err(DriveError::NotFound { .. })
    ));
}

#[test]
fn a_request_without_an_answer_is_a_transport_error() {
    let mut options = BTreeMap::new();
    options.insert("endpoint".to_string(), "http://files.example".to_string());
    let drive =
        OpendalDrive::open("http", &options, &SecretOptions::new(), Box::new(NoNetwork)).unwrap();
    match drive.get("readme.txt") {
        Err(DriveError::Transport(why)) => assert!(why.contains("no network"), "{why}"),
        other => panic!("not a transport error: {other:?}"),
    }
}

#[test]
fn a_sources_secrets_reach_opendal_but_never_its_debug_output() {
    let mut options = BTreeMap::new();
    options.insert("endpoint".to_string(), "http://files.example".to_string());
    options.insert("username".to_string(), "ann".to_string());
    let mut secrets = SecretOptions::new();
    secrets.insert("password", "sesame-42");
    let drive = OpendalDrive::open("http", &options, &secrets, Box::new(NoNetwork)).unwrap();
    let text = format!("{drive:?}");
    assert!(!text.contains("sesame-42"), "{text}");
    assert!(text.contains("http"), "{text}");
    assert_eq!(drive.scheme(), "http");
}
