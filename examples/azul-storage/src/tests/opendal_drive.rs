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

/// A transport with no answer that says which URL it tried, as an HTTP client's error does
/// ("Connection failed: {url}").
struct NamesTheUrl;

impl Transport for NamesTheUrl {
    fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
        Err(format!("Connection failed: {}", call.url))
    }
}

/// A server that answers every request with a 500.
struct AlwaysBroken;

impl Transport for AlwaysBroken {
    fn send(&self, _call: &HttpCall) -> Result<HttpReply, String> {
        Ok(HttpReply {
            status: 500,
            headers: vec![("content-type".to_string(), "text/plain".to_string())],
            body: b"broken".to_vec(),
        })
    }
}

fn pcloud(transport: Box<dyn Transport>) -> OpendalDrive {
    let mut options = BTreeMap::new();
    options.insert("endpoint".to_string(), "https://api.pcloud.com".to_string());
    options.insert("username".to_string(), "ann".to_string());
    let mut secrets = SecretOptions::new();
    secrets.insert("password", "hunter2");
    OpendalDrive::open("pcloud", &options, &secrets, transport).expect("the pCloud source")
}

fn webhdfs(transport: Box<dyn Transport>) -> OpendalDrive {
    let mut options = BTreeMap::new();
    options.insert("endpoint".to_string(), "http://namenode:9870".to_string());
    options.insert("user_name".to_string(), "ann".to_string());
    let mut secrets = SecretOptions::new();
    secrets.insert("delegation", "HAAAABBCCDD-token");
    OpendalDrive::open("webhdfs", &options, &secrets, transport).expect("the WebHDFS source")
}

/// AZ-CN3: pCloud sends the user name and the PASSWORD in the query of every request, WebHDFS its
/// delegation token; a request that got no answer said "Connection failed: {url}", and that text
/// was the red line under "Test connection" (the review's screen share). No error of the drive
/// names the secret, shown or in its Debug output, and the host is still there.
#[test]
fn az_cn3_a_source_that_puts_its_credentials_in_the_url_names_none_in_a_failure() {
    for (drive, secret, host) in [
        (pcloud(Box::new(NamesTheUrl)), "hunter2", "api.pcloud.com"),
        (
            webhdfs(Box::new(NamesTheUrl)),
            "HAAAABBCCDD",
            "namenode:9870",
        ),
    ] {
        let error = drive.list(&ListRequest::folder("")).unwrap_err();
        let (shown, debug) = (error.to_string(), format!("{error:?}"));
        for text in [&shown, &debug] {
            assert!(
                !text.contains(secret),
                "the failure names the secret: {text}"
            );
            assert!(
                text.contains(host),
                "the host is what finds the server: {text}"
            );
        }
    }
}

/// The same for a server that answered: OpenDAL's error carries the request's URI as context.
#[test]
fn az_cn3_an_error_answer_of_such_a_source_names_no_credential_either() {
    for (drive, secret) in [
        (pcloud(Box::new(AlwaysBroken)), "hunter2"),
        (webhdfs(Box::new(AlwaysBroken)), "HAAAABBCCDD"),
    ] {
        let error = drive.list(&ListRequest::folder("")).unwrap_err();
        let (shown, debug) = (error.to_string(), format!("{error:?}"));
        for text in [&shown, &debug] {
            assert!(
                !text.contains(secret),
                "the failure names the secret: {text}"
            );
        }
    }
}

/// AZ-CN5: the first request of a WebDAV source is a PROPFIND (a listing, a stat), and it reaches
/// the transport as one. (The transport of the apps refused it: its own test pins the mapping.)
#[test]
fn az_cn5_a_webdav_source_sends_propfind_through_the_transport() {
    struct RecordsVerbs(std::sync::Arc<Mutex<Vec<(Method, String)>>>);
    impl Transport for RecordsVerbs {
        fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
            self.0.lock().unwrap().push((call.method, call.url.clone()));
            Err("no network in this test".to_string())
        }
    }
    let seen = std::sync::Arc::new(Mutex::new(Vec::new()));
    let mut options = BTreeMap::new();
    options.insert(
        "endpoint".to_string(),
        "https://cloud.example/remote.php/dav/files/ann".to_string(),
    );
    options.insert("username".to_string(), "ann".to_string());
    let mut secrets = SecretOptions::new();
    secrets.insert("password", "sesame-42");
    let drive = OpendalDrive::open(
        "webdav",
        &options,
        &secrets,
        Box::new(RecordsVerbs(seen.clone())),
    )
    .unwrap();
    let _ = drive.list(&ListRequest::folder(""));
    let seen = seen.lock().unwrap().clone();
    assert!(
        seen.iter().any(|(method, _)| *method == Method::Propfind),
        "a WebDAV listing is a PROPFIND: {seen:?}"
    );
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

/// A transport whose app pinned or distrusted roots: its TLS trusts less than the public set.
struct NarrowedTrust;

impl Transport for NarrowedTrust {
    fn send(&self, _call: &HttpCall) -> Result<HttpReply, String> {
        Err("no network in this test".to_string())
    }

    fn trusts_public_roots(&self) -> bool {
        false
    }
}

/// FTP over TLS and Redis over TLS bring their own fixed root store (the OS's), which cannot
/// follow an app that pinned or distrusted roots: such a source is refused when it opens,
/// before anything connects; the same sources without TLS, or under the public roots, open.
#[test]
fn a_source_with_its_own_tls_roots_is_refused_when_the_app_narrowed_its_trusted_roots() {
    let open = |scheme: &str, endpoint: &str, transport: Box<dyn Transport>| {
        let mut options = BTreeMap::new();
        options.insert("endpoint".to_string(), endpoint.to_string());
        OpendalDrive::open(scheme, &options, &SecretOptions::new(), transport)
    };
    for (scheme, endpoint) in [
        ("ftp", "ftps://nas.local:21"),
        ("ftp", "nas.local:21"),
        ("redis", "rediss://cache.local:6380"),
    ] {
        match open(scheme, endpoint, Box::new(NarrowedTrust)) {
            Err(DriveError::InvalidConfig(why)) => {
                assert!(why.contains("trusted roots"), "{scheme} {endpoint}: {why}");
            }
            other => panic!("{scheme} {endpoint}: {:?}", other.map(|_| ())),
        }
        assert!(open(scheme, endpoint, Box::new(NoNetwork)).is_ok(), "{scheme} {endpoint}");
    }
    assert!(open("ftp", "ftp://nas.local:21", Box::new(NarrowedTrust)).is_ok());
    assert!(open("redis", "tcp://cache.local:6379", Box::new(NarrowedTrust)).is_ok());
}

// ==== AZ-CN8 / AZ-CN9: what the source is told ====

fn settings_for(
    scheme: &str,
    options: &[(&str, &str)],
    secrets: &[(&str, &str)],
) -> Vec<(String, String)> {
    let options: BTreeMap<String, String> = options
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    let mut keyring = SecretOptions::new();
    for (key, value) in secrets {
        keyring.insert(key, value);
    }
    let mut settings = crate::opendal_drive::settings_of(scheme, &options, &keyring);
    settings.sort();
    settings
}

fn has(settings: &[(String, String)], key: &str, value: &str) -> bool {
    settings.iter().any(|(k, v)| k == key && v == value)
}

/// A Google source whose form chose a credential does not look for another on this computer: a key
/// that is mis-pasted fails instead of being replaced by the developer's gcloud login. The machine's
/// login is a choice of its own, and a source saved before the form asked is as it was.
#[test]
fn az_cn9_a_google_source_with_a_key_does_not_look_for_the_machines_login() {
    let key = [("credential", "a2V5")];
    let with_key = settings_for("gcs", &[("bucket", "b")], &key);
    assert!(
        has(&with_key, "disable_config_load", "true"),
        "{with_key:?}"
    );
    assert!(
        has(&with_key, "disable_vm_metadata", "true"),
        "{with_key:?}"
    );
    assert!(has(&with_key, "credential", "a2V5"));
    let public = settings_for("gcs", &[("bucket", "b"), ("anonymous", "true")], &[]);
    assert!(has(&public, "skip_signature", "true"), "{public:?}");
    assert!(has(&public, "disable_config_load", "true"), "{public:?}");
    // The machine's login, on purpose.
    let machine = settings_for("gcs", &[("bucket", "b"), ("machine_identity", "true")], &[]);
    assert!(
        !machine.iter().any(|(k, _)| k.starts_with("disable_")),
        "{machine:?}"
    );
    let both = settings_for(
        "gcs",
        &[("bucket", "b"), ("machine_identity", "true")],
        &key,
    );
    assert!(
        !both.iter().any(|(k, _)| k.starts_with("disable_")),
        "{both:?}"
    );
    // Saved before the form asked for a credential: unchanged.
    let old = settings_for("gcs", &[("bucket", "b")], &[]);
    assert!(
        !old.iter().any(|(k, _)| k.starts_with("disable_")),
        "{old:?}"
    );
    // The form's two switches are no OpenDAL options.
    for settings in [&with_key, &public, &machine, &old] {
        assert!(
            !settings
                .iter()
                .any(|(k, _)| k == "anonymous" || k == "machine_identity"),
            "{settings:?}"
        );
    }
    // Tencent's lookup is off once its key is given; the others have no switch.
    let cos = settings_for("cos", &[("bucket", "b")], &[("secret_key", "s")]);
    assert!(has(&cos, "disable_config_load", "true"), "{cos:?}");
    let webdav = settings_for(
        "webdav",
        &[("endpoint", "https://h/dav")],
        &[("password", "p")],
    );
    assert!(
        !webdav.iter().any(|(k, _)| k.starts_with("disable_")),
        "{webdav:?}"
    );
}

/// A source already saved with a user name and a password inside its address is not opened with
/// them: drives.json keeps them in plain text and every window showed them. The reason does not
/// repeat the password.
#[test]
fn az_cn8_a_source_whose_address_holds_a_login_is_refused_without_repeating_it() {
    let mut options = BTreeMap::new();
    options.insert(
        "endpoint".to_string(),
        "https://ann:hunter2@cloud.example/dav".to_string(),
    );
    match OpendalDrive::open(
        "webdav",
        &options,
        &SecretOptions::new(),
        Box::new(NoNetwork),
    ) {
        Err(DriveError::InvalidConfig(why)) => {
            assert!(
                why.contains("endpoint") && why.contains("user name"),
                "{why}"
            );
            assert!(!why.contains("hunter2"), "{why}");
        }
        other => panic!("{:?}", other.map(|_| ())),
    }
    // Without the login it opens.
    options.insert(
        "endpoint".to_string(),
        "https://cloud.example/dav".to_string(),
    );
    assert!(OpendalDrive::open(
        "webdav",
        &options,
        &SecretOptions::new(),
        Box::new(NoNetwork)
    )
    .is_ok());
}
