use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use crate::{
    sigv4, ByteRange, Credentials, Drive, DriveError, HttpCall, HttpReply, ListRequest, Method,
    S3Config, S3Drive, Transport,
};

/// 2013-05-24T00:00:00Z, the date of the S3 reference examples.
const NOW: u64 = 1_369_353_600;

/// Records every call and answers from a script.
#[derive(Default)]
struct Script {
    calls: Mutex<Vec<HttpCall>>,
    replies: Mutex<VecDeque<Result<HttpReply, String>>>,
}

#[derive(Clone, Default)]
struct Fake(Arc<Script>);

impl Fake {
    fn answer(&self, status: u16, headers: &[(&str, &str)], body: &str) -> &Self {
        self.0.replies.lock().unwrap().push_back(Ok(HttpReply {
            status,
            headers: headers
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            body: body.as_bytes().to_vec(),
        }));
        self
    }

    fn fail(&self, message: &str) -> &Self {
        self.0
            .replies
            .lock()
            .unwrap()
            .push_back(Err(message.to_string()));
        self
    }

    fn calls(&self) -> Vec<HttpCall> {
        self.0.calls.lock().unwrap().clone()
    }

    fn last(&self) -> HttpCall {
        self.calls().pop().expect("a request was sent")
    }
}

impl Transport for Fake {
    fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
        self.0.calls.lock().unwrap().push(call.clone());
        self.0
            .replies
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Err("no scripted answer".to_string()))
    }
}

fn credentials() -> Credentials {
    Credentials::new("AKIDTEST", "test-secret")
}

fn config(endpoint: &str, path_style: bool) -> S3Config {
    S3Config {
        endpoint: endpoint.to_string(),
        region: "us-east-1".to_string(),
        bucket: "azdrive".to_string(),
        path_style,
    }
}

fn drive_at(endpoint: &str, path_style: bool, fake: &Fake) -> S3Drive {
    S3Drive::new(
        config(endpoint, path_style),
        credentials(),
        Box::new(fake.clone()),
    )
    .expect("a valid configuration")
    .with_clock(|| NOW)
}

fn local_drive(fake: &Fake) -> S3Drive {
    drive_at("http://127.0.0.1:9000", true, fake)
}

fn header<'a>(call: &'a HttpCall, name: &str) -> Option<&'a str> {
    call.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}

const EMPTY_LISTING: &str = "<ListBucketResult><IsTruncated>false</IsTruncated></ListBucketResult>";

#[test]
fn a_path_style_listing_asks_list_objects_v2_under_the_bucket_path() {
    let fake = Fake::default();
    fake.answer(200, &[], EMPTY_LISTING);
    local_drive(&fake)
        .list(&ListRequest::folder("mail/").with_max_keys(2))
        .unwrap();
    let call = fake.last();
    assert_eq!(call.method, Method::Get);
    assert_eq!(
        call.url,
        "http://127.0.0.1:9000/azdrive?delimiter=%2F&list-type=2&max-keys=2&prefix=mail%2F"
    );
    assert_eq!(header(&call, "x-amz-date"), Some("20130524T000000Z"));
    assert_eq!(
        header(&call, "x-amz-content-sha256"),
        Some(sigv4::EMPTY_SHA256)
    );
    assert!(call.body.is_empty());
}

#[test]
fn the_listing_signature_matches_an_independent_sigv4_computation() {
    // Computed with Python's hashlib / hmac over the canonical request of this call
    // (host 127.0.0.1:9000, secret "test-secret").
    let fake = Fake::default();
    fake.answer(200, &[], EMPTY_LISTING);
    local_drive(&fake)
        .list(&ListRequest::folder("mail/").with_max_keys(2))
        .unwrap();
    assert_eq!(
        header(&fake.last(), "authorization"),
        Some(
            "AWS4-HMAC-SHA256 Credential=AKIDTEST/20130524/us-east-1/s3/aws4_request, \
             SignedHeaders=host;x-amz-content-sha256;x-amz-date, \
             Signature=46202e814f6a973b3a27f9a18ab76be3fb33c5eb3e0fb108c9c61124694a6fad"
        )
    );
}

#[test]
fn the_get_object_signature_matches_an_independent_sigv4_computation() {
    let fake = Fake::default();
    fake.answer(200, &[], "hello");
    let bytes = local_drive(&fake).get("mail/inbox/0001.eml").unwrap();
    assert_eq!(bytes, b"hello");
    let call = fake.last();
    assert_eq!(
        call.url,
        "http://127.0.0.1:9000/azdrive/mail/inbox/0001.eml"
    );
    assert_eq!(
        header(&call, "authorization"),
        Some(
            "AWS4-HMAC-SHA256 Credential=AKIDTEST/20130524/us-east-1/s3/aws4_request, \
             SignedHeaders=host;x-amz-content-sha256;x-amz-date, \
             Signature=cfb20a8140b1d84d3d5018d5cccad359a0bcb9a8ef6764732b51341545fe5f6d"
        )
    );
}

#[test]
fn a_virtual_host_listing_puts_the_bucket_in_the_host() {
    let fake = Fake::default();
    fake.answer(200, &[], EMPTY_LISTING);
    drive_at("https://s3.eu-central-1.amazonaws.com", false, &fake)
        .list(&ListRequest::folder(""))
        .unwrap();
    assert_eq!(
        fake.last().url,
        "https://azdrive.s3.eu-central-1.amazonaws.com/?delimiter=%2F&list-type=2&max-keys=1000&prefix="
    );
}

#[test]
fn the_continuation_token_goes_back_encoded() {
    let fake = Fake::default();
    fake.answer(200, &[], EMPTY_LISTING);
    local_drive(&fake)
        .list(&ListRequest::folder("").with_continuation("a+b/c=".to_string()))
        .unwrap();
    assert!(fake.last().url.contains("continuation-token=a%2Bb%2Fc%3D"));
}

#[test]
fn a_listing_answer_becomes_a_page() {
    let fake = Fake::default();
    fake.answer(
        200,
        &[],
        "<ListBucketResult><IsTruncated>true</IsTruncated>\
         <Contents><Key>mail/x.eml</Key><Size>3</Size>\
         <LastModified>2013-05-24T00:00:00.000Z</LastModified><ETag>\"e\"</ETag></Contents>\
         <CommonPrefixes><Prefix>mail/inbox/</Prefix></CommonPrefixes>\
         <NextContinuationToken>tok</NextContinuationToken></ListBucketResult>",
    );
    let page = local_drive(&fake)
        .list(&ListRequest::folder("mail/"))
        .unwrap();
    assert_eq!(page.folders, vec!["mail/inbox/".to_string()]);
    assert_eq!(page.objects[0].key, "mail/x.eml");
    assert_eq!(page.objects[0].modified, Some(NOW));
    assert_eq!(page.next.as_deref(), Some("tok"));
}

#[test]
fn get_object_encodes_the_key_but_keeps_its_slashes() {
    let fake = Fake::default();
    fake.answer(200, &[], "");
    local_drive(&fake).get("mail/inbox/a b+c.eml").unwrap();
    assert_eq!(
        fake.last().url,
        "http://127.0.0.1:9000/azdrive/mail/inbox/a%20b%2Bc.eml"
    );
}

#[test]
fn get_range_sends_a_signed_range_header() {
    let fake = Fake::default();
    fake.answer(206, &[("Content-Range", "bytes 0-9/100")], "0123456789");
    let bytes = local_drive(&fake)
        .get_range("big.bin", ByteRange::new(0, Some(9)))
        .unwrap();
    assert_eq!(bytes, b"0123456789");
    let call = fake.last();
    assert_eq!(header(&call, "range"), Some("bytes=0-9"));
    assert!(header(&call, "authorization")
        .unwrap()
        .contains("SignedHeaders=host;range;x-amz-content-sha256;x-amz-date,"));
}

#[test]
fn a_server_that_ignores_the_range_still_yields_just_the_range() {
    let fake = Fake::default();
    fake.answer(200, &[], "0123456789");
    let bytes = local_drive(&fake)
        .get_range("big.bin", ByteRange::new(3, Some(5)))
        .unwrap();
    assert_eq!(bytes, b"345");
}

#[test]
fn put_object_sends_the_body_and_signs_its_hash() {
    let fake = Fake::default();
    fake.answer(200, &[("ETag", "\"abc\"")], "");
    local_drive(&fake)
        .put("mail/out.eml", b"Subject: hi")
        .unwrap();
    let call = fake.last();
    assert_eq!(call.method, Method::Put);
    assert_eq!(call.url, "http://127.0.0.1:9000/azdrive/mail/out.eml");
    assert_eq!(call.body, b"Subject: hi");
    assert_eq!(
        header(&call, "x-amz-content-sha256"),
        Some(sigv4::sha256_hex(b"Subject: hi").as_str())
    );
}

#[test]
fn a_session_token_is_sent_and_signed() {
    let fake = Fake::default();
    fake.answer(200, &[], "");
    let drive = S3Drive::new(
        config("http://127.0.0.1:9000", true),
        credentials().with_session_token("session-token"),
        Box::new(fake.clone()),
    )
    .unwrap()
    .with_clock(|| NOW);
    drive.get("a.txt").unwrap();
    let call = fake.last();
    assert_eq!(header(&call, "x-amz-security-token"), Some("session-token"));
    assert!(header(&call, "authorization")
        .unwrap()
        .contains("SignedHeaders=host;x-amz-content-sha256;x-amz-date;x-amz-security-token,"));
}

#[test]
fn head_object_reads_the_size_etag_and_last_modified_from_the_headers() {
    let fake = Fake::default();
    fake.answer(
        200,
        &[
            ("Content-Length", "434234"),
            ("ETag", "\"fba9dede\""),
            ("Last-Modified", "Mon, 12 Oct 2009 17:50:30 GMT"),
        ],
        "",
    );
    let info = local_drive(&fake).head("mail/a.eml").unwrap();
    assert_eq!(fake.last().method, Method::Head);
    assert_eq!(info.key, "mail/a.eml");
    assert_eq!(info.size, 434_234);
    assert_eq!(info.etag.as_deref(), Some("fba9dede"));
    assert_eq!(info.modified, Some(1_255_369_830));
}

#[test]
fn delete_sends_delete_and_accepts_204() {
    let fake = Fake::default();
    fake.answer(204, &[], "");
    local_drive(&fake).delete("mail/old.eml").unwrap();
    let call = fake.last();
    assert_eq!(call.method, Method::Delete);
    assert_eq!(call.url, "http://127.0.0.1:9000/azdrive/mail/old.eml");
}

#[test]
fn a_missing_object_is_not_found() {
    let fake = Fake::default();
    fake.answer(
        404,
        &[],
        "<Error><Code>NoSuchKey</Code><Message>The specified key does not exist.</Message>\
         <Key>mail/nope.eml</Key></Error>",
    );
    assert!(matches!(
        local_drive(&fake).get("mail/nope.eml"),
        Err(DriveError::NotFound { .. })
    ));
}

#[test]
fn head_of_a_missing_object_without_a_body_is_not_found() {
    let fake = Fake::default();
    fake.answer(404, &[], "");
    assert!(matches!(
        local_drive(&fake).head("mail/nope.eml"),
        Err(DriveError::NotFound { .. })
    ));
}

#[test]
fn an_s3_error_answer_is_a_readable_service_error() {
    let fake = Fake::default();
    fake.answer(
        403,
        &[],
        "<Error><Code>InvalidAccessKeyId</Code><Message>The AWS Access Key Id you provided \
         does not exist in our records.</Message></Error>",
    );
    match local_drive(&fake).list(&ListRequest::folder("")) {
        Err(DriveError::Service(e)) => {
            assert_eq!(e.status, 403);
            assert_eq!(e.code, "InvalidAccessKeyId");
            let text = DriveError::Service(e).to_string();
            assert!(text.contains("does not exist in our records"), "{text}");
        }
        other => panic!("expected a service error, got {other:?}"),
    }
}

#[test]
fn a_transport_failure_is_a_transport_error() {
    let fake = Fake::default();
    fake.fail("Connection failed: http://127.0.0.1:9000");
    match local_drive(&fake).get("a.txt") {
        Err(DriveError::Transport(message)) => assert!(message.contains("Connection failed")),
        other => panic!("expected a transport error, got {other:?}"),
    }
}

#[test]
fn an_endpoint_with_a_default_port_or_a_trailing_slash_is_normalised() {
    let fake = Fake::default();
    fake.answer(200, &[], "");
    drive_at("https://S3.Example.COM:443/", true, &fake)
        .get("a.txt")
        .unwrap();
    assert_eq!(fake.last().url, "https://s3.example.com/azdrive/a.txt");
}

#[test]
fn an_endpoint_with_a_base_path_keeps_it_in_front_of_the_bucket() {
    let fake = Fake::default();
    fake.answer(200, &[], "");
    drive_at("http://localhost:8080/storage", true, &fake)
        .get("a.txt")
        .unwrap();
    assert_eq!(
        fake.last().url,
        "http://localhost:8080/storage/azdrive/a.txt"
    );
}

#[test]
fn an_endpoint_that_is_not_http_is_refused() {
    for endpoint in [
        "",
        "ftp://example.com",
        "example.com",
        "http://",
        "http://a b",
    ] {
        assert!(
            S3Drive::new(
                config(endpoint, true),
                credentials(),
                Box::new(Fake::default())
            )
            .is_err(),
            "{endpoint:?}"
        );
    }
}

#[test]
fn a_bucket_name_that_cannot_be_in_a_url_is_refused() {
    for bucket in ["", "a/b", "has space"] {
        let mut c = config("http://127.0.0.1:9000", true);
        c.bucket = bucket.to_string();
        assert!(
            S3Drive::new(c, credentials(), Box::new(Fake::default())).is_err(),
            "{bucket:?}"
        );
    }
    // Virtual-host style puts the bucket in a host name: DNS rules apply.
    let mut c = config("https://s3.amazonaws.com", false);
    c.bucket = "Upper_Case".to_string();
    assert!(S3Drive::new(c, credentials(), Box::new(Fake::default())).is_err());
}

#[test]
fn an_empty_key_is_refused_before_anything_is_sent() {
    let fake = Fake::default();
    assert!(matches!(
        local_drive(&fake).get(""),
        Err(DriveError::InvalidKey { .. })
    ));
    assert!(fake.calls().is_empty());
}

#[test]
fn credentials_never_show_the_secret_in_debug_output() {
    let creds = credentials().with_session_token("session-token-value");
    let text = format!("{creds:?}");
    assert!(!text.contains("test-secret"), "{text}");
    assert!(!text.contains("session-token-value"), "{text}");
    assert!(!text.contains("AKIDTEST"), "{text}");

    let fake = Fake::default();
    fake.answer(200, &[], "");
    let drive = S3Drive::new(
        config("http://127.0.0.1:9000", true),
        creds,
        Box::new(fake.clone()),
    )
    .unwrap()
    .with_clock(|| NOW);
    let text = format!("{drive:?}");
    assert!(
        !text.contains("test-secret") && !text.contains("session-token-value"),
        "{text}"
    );

    drive.get("a.txt").unwrap();
    let call = format!("{:?}", fake.last());
    assert!(!call.contains("Signature="), "{call}");
    assert!(!call.contains("session-token-value"), "{call}");
}

#[test]
fn credentials_round_trip_through_the_keyring_secret() {
    let creds = credentials().with_session_token("tok");
    let secret = creds.to_keyring_secret();
    let back = Credentials::from_keyring_secret(&secret).unwrap();
    assert_eq!(back, creds);
    assert!(Credentials::from_keyring_secret("not json").is_err());
    assert!(Credentials::from_keyring_secret("{}").is_err());
}

#[test]
fn the_metadata_of_an_object_lists_its_type_storage_class_and_user_metadata() {
    let fake = Fake::default();
    fake.answer(
        200,
        &[
            ("Content-Type", "image/png"),
            ("Content-Length", "1234"),
            ("ETag", "\"abc\""),
            ("x-amz-storage-class", "STANDARD_IA"),
            ("x-amz-meta-author", "Felix"),
            ("x-amz-server-side-encryption", "AES256"),
            ("x-amz-request-id", "R1"),
        ],
        "",
    );
    let pairs = local_drive(&fake).metadata("photos/a.png").unwrap();
    let call = fake.last();
    assert_eq!(call.method, Method::Head);
    let get = |name: &str| {
        pairs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    };
    assert_eq!(get("Content-Type"), Some("image/png"));
    assert_eq!(get("Storage class"), Some("STANDARD_IA"));
    assert_eq!(get("Encryption"), Some("AES256"));
    assert_eq!(get("ETag"), Some("abc"));
    assert_eq!(
        get("author"),
        Some("Felix"),
        "user metadata by its own name: {pairs:?}"
    );
    assert_eq!(
        get("x-amz-request-id"),
        None,
        "request bookkeeping is not metadata"
    );
}

#[test]
fn the_metadata_of_a_missing_object_is_not_found() {
    let fake = Fake::default();
    fake.answer(404, &[], "");
    assert!(matches!(
        local_drive(&fake).metadata("nope.png"),
        Err(DriveError::NotFound { .. })
    ));
}
