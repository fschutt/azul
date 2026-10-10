use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

use crate::{
    sigv4, ByteRange, Credentials, Drive, DriveError, HttpCall, HttpReply, ListRequest, Method,
    Precondition, S3Config, S3Drive, Transport,
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
pub(super) struct Fake(Arc<Script>);

impl Fake {
    pub(super) fn answer(&self, status: u16, headers: &[(&str, &str)], body: &str) -> &Self {
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

    pub(super) fn fail(&self, message: &str) -> &Self {
        self.0
            .replies
            .lock()
            .unwrap()
            .push_back(Err(message.to_string()));
        self
    }

    pub(super) fn calls(&self) -> Vec<HttpCall> {
        self.0.calls.lock().unwrap().clone()
    }

    pub(super) fn last(&self) -> HttpCall {
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
fn an_azlin_error_answer_keeps_its_code_its_retry_after_and_its_request_id() {
    // An Azlin node's answer (D33): the S3 error, its own code in `x-azlin-error`, how long to
    // wait in `Retry-After`, the request ID in `x-amz-request-id` (also in the body).
    let fake = Fake::default();
    fake.answer(
        503,
        &[
            ("x-azlin-error", "maintenance"),
            ("Retry-After", "30"),
            ("x-amz-request-id", "n2-81723"),
        ],
        "<Error><Code>ServiceUnavailable</Code><Message>upgrading</Message></Error>",
    );
    // A body without a RequestId: the header names it.
    match local_drive(&fake).list(&ListRequest::folder("")) {
        Err(DriveError::Service(e)) => {
            assert_eq!((e.status, e.code.as_str()), (503, "ServiceUnavailable"));
            assert_eq!(e.azlin_error.as_deref(), Some("maintenance"));
            assert_eq!(e.retry_after, Some(30));
            assert_eq!(e.request_id.as_deref(), Some("n2-81723"));
            let text = DriveError::Service(e).to_string();
            assert!(text.contains("x-azlin-error maintenance"), "{text}");
        }
        other => panic!("expected a service error, got {other:?}"),
    }
    // Another service: none of it, and a Retry-After that is a date is no number of seconds.
    fake.answer(
        503,
        &[("Retry-After", "Wed, 21 Oct 2026 07:28:00 GMT")],
        "<Error><Code>SlowDown</Code><Message>slow down</Message>\
         <RequestId>4442587FB7D0A2F9</RequestId></Error>",
    );
    match local_drive(&fake).list(&ListRequest::folder("")) {
        Err(DriveError::Service(e)) => {
            assert_eq!(e.azlin_error, None);
            assert_eq!(e.retry_after, None);
            assert_eq!(e.request_id.as_deref(), Some("4442587FB7D0A2F9"));
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

#[test]
fn a_copy_within_the_bucket_is_one_copy_object_request() {
    let fake = Fake::default();
    fake.answer(
        200,
        &[],
        "<CopyObjectResult><ETag>\"abc\"</ETag></CopyObjectResult>",
    );
    local_drive(&fake)
        .copy("photos/a b.png", "backup/a b.png")
        .unwrap();
    let calls = fake.calls();
    assert_eq!(calls.len(), 1, "no GET, no upload: the service copies");
    let call = &calls[0];
    assert_eq!(call.method, Method::Put);
    assert_eq!(call.url, "http://127.0.0.1:9000/azdrive/backup/a%20b.png");
    assert_eq!(
        header(call, "x-amz-copy-source"),
        Some("/azdrive/photos/a%20b.png")
    );
    assert!(call.body.is_empty());
}

#[test]
fn a_copy_error_inside_a_200_answer_fails() {
    let fake = Fake::default();
    fake.answer(
        200,
        &[],
        "<Error><Code>InternalError</Code><Message>try again</Message></Error>",
    );
    assert!(local_drive(&fake).copy("a.txt", "b.txt").is_err());
    let fake = Fake::default();
    fake.answer(404, &[], "<Error><Code>NoSuchKey</Code></Error>");
    assert!(matches!(
        local_drive(&fake).copy("a.txt", "b.txt"),
        Err(DriveError::NotFound { .. })
    ));
}

/// AWS's reference example of a presigned URL ("Authenticating Requests: Using Query
/// Parameters"): GET examplebucket's test.txt, valid 24 hours from 2013-05-24, signed with the
/// documentation's example keys - the signature is the documented one, and nothing is sent.
#[test]
fn a_presigned_link_carries_the_documented_signature_and_sends_nothing() {
    let fake = Fake::default();
    let drive = S3Drive::new(
        S3Config {
            endpoint: "https://s3.amazonaws.com".to_string(),
            region: "us-east-1".to_string(),
            bucket: "examplebucket".to_string(),
            path_style: false,
        },
        Credentials::new(
            "AKIAIOSFODNN7EXAMPLE",
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY",
        ),
        Box::new(fake.clone()),
    )
    .expect("the reference configuration")
    .with_clock(|| NOW);
    let url = drive.presigned_get_url("test.txt", 86_400).expect("a link");
    assert!(
        url.starts_with("https://examplebucket.s3.amazonaws.com/test.txt?"),
        "{url}"
    );
    for part in [
        "X-Amz-Algorithm=AWS4-HMAC-SHA256",
        "X-Amz-Credential=AKIAIOSFODNN7EXAMPLE%2F20130524%2Fus-east-1%2Fs3%2Faws4_request",
        "X-Amz-Date=20130524T000000Z",
        "X-Amz-Expires=86400",
        "X-Amz-SignedHeaders=host",
        "X-Amz-Signature=aeeed9bbccd4d02ee5c0109b86d86835f995330da4c265957d157751f604d404",
    ] {
        assert!(url.contains(part), "{part} in {url}");
    }
    assert!(fake.calls().is_empty(), "a link is made, not fetched");
    // S3 takes seven days at most; a longer wish is cut to it.
    let week = drive
        .presigned_get_url("test.txt", 30 * 86_400)
        .expect("a link");
    assert!(week.contains("X-Amz-Expires=604800"), "{week}");
    assert!(drive.presigned_get_url("", 60).is_err(), "no key, no link");
}

#[test]
fn a_copy_the_service_does_not_confirm_fails() {
    // A 200 without a CopyObjectResult is no copy (a server that ignored
    // `x-amz-copy-source` wrote an empty object).
    let fake = Fake::default();
    fake.answer(200, &[], "");
    assert!(matches!(
        local_drive(&fake).copy("a.txt", "b.txt"),
        Err(DriveError::Protocol(_))
    ));
}

#[test]
fn a_raw_get_signs_exactly_like_the_drives_own_get() {
    // The same request as the_get_object_signature_matches_an_independent_sigv4_computation.
    let fake = Fake::default();
    fake.answer(200, &[], "hello");
    let reply = local_drive(&fake)
        .send_raw(
            Method::Get,
            Some("mail/inbox/0001.eml"),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            "",
        )
        .unwrap();
    assert_eq!(reply.body, b"hello");
    let call = fake.last();
    assert_eq!(call.url, "http://127.0.0.1:9000/azdrive/mail/inbox/0001.eml");
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
fn a_raw_request_signs_its_extra_headers_and_hands_back_any_status() {
    let fake = Fake::default();
    fake.answer(
        412,
        &[],
        "<Error><Code>PreconditionFailed</Code><Message>At least one of the pre-conditions \
         you specified did not hold</Message></Error>",
    );
    let reply = local_drive(&fake)
        .send_raw(
            Method::Put,
            Some("sync/.azlin/index.json"),
            Vec::new(),
            vec![(String::from("if-match"), String::from("\"v1\""))],
            b"{}".to_vec(),
            "application/json",
        )
        .expect("a 412 is an answer, not a failure to send");
    assert_eq!(reply.status, 412);
    let call = fake.last();
    assert_eq!(call.method, Method::Put);
    assert_eq!(
        call.url,
        "http://127.0.0.1:9000/azdrive/sync/.azlin/index.json"
    );
    assert_eq!(header(&call, "if-match"), Some("\"v1\""));
    assert_eq!(call.content_type, "application/json");
    assert_eq!(
        header(&call, "x-amz-content-sha256"),
        Some(sigv4::sha256_hex(b"{}").as_str())
    );
    assert!(header(&call, "authorization")
        .unwrap()
        .contains("SignedHeaders=host;if-match;x-amz-content-sha256;x-amz-date,"));
    match S3Drive::failure_of(&reply, Some("sync/.azlin/index.json")) {
        DriveError::Service(e) => {
            assert_eq!(e.status, 412);
            assert_eq!(e.code, "PreconditionFailed");
        }
        other => panic!("not the service's refusal: {other:?}"),
    }
}

#[test]
fn a_raw_request_carries_its_query_and_reaches_the_bucket_without_a_key() {
    let fake = Fake::default();
    fake.answer(200, &[], "<InitiateMultipartUploadResult/>");
    fake.answer(200, &[], EMPTY_LISTING);
    let drive = local_drive(&fake);
    drive
        .send_raw(
            Method::Post,
            Some("big.bin"),
            vec![(String::from("uploads"), String::new())],
            Vec::new(),
            Vec::new(),
            "",
        )
        .unwrap();
    assert_eq!(fake.last().url, "http://127.0.0.1:9000/azdrive/big.bin?uploads=");
    let reply = drive
        .send_raw(
            Method::Get,
            None,
            vec![
                (String::from("list-type"), String::from("2")),
                (String::from("prefix"), String::from("a/")),
            ],
            Vec::new(),
            Vec::new(),
            "",
        )
        .unwrap();
    assert_eq!(
        fake.last().url,
        "http://127.0.0.1:9000/azdrive?list-type=2&prefix=a%2F"
    );
    let page = crate::s3::parse_listing(std::str::from_utf8(&reply.body).unwrap()).unwrap();
    assert!(page.objects.is_empty() && page.next.is_none());
    assert!(
        drive
            .send_raw(Method::Get, Some(""), Vec::new(), Vec::new(), Vec::new(), "")
            .is_err(),
        "an empty key is refused before anything is sent"
    );
    assert_eq!(fake.calls().len(), 2);
}

#[test]
fn a_failed_raw_answer_reads_as_the_drives_own_error() {
    let missing = HttpReply {
        status: 404,
        headers: Vec::new(),
        body: b"<Error><Code>NoSuchKey</Code><Message>gone</Message></Error>".to_vec(),
    };
    assert_eq!(
        S3Drive::failure_of(&missing, Some("a.txt")),
        DriveError::NotFound {
            key: String::from("a.txt")
        }
    );
    let busy = HttpReply {
        status: 503,
        headers: Vec::new(),
        body: Vec::new(),
    };
    assert!(matches!(
        S3Drive::failure_of(&busy, None),
        DriveError::Service(e) if e.status == 503 && e.code == "ServiceUnavailable"
    ));
}

#[test]
fn a_conditional_put_asks_if_none_match_or_if_match_and_reads_a_412_as_a_conflict() {
    let fake = Fake::default();
    fake.answer(200, &[("ETag", "\"e1\"")], "");
    fake.answer(
        412,
        &[],
        "<Error><Code>PreconditionFailed</Code><Message>At least one of the pre-conditions \
         you specified did not hold</Message></Error>",
    );
    fake.answer(200, &[("ETag", "\"e2\"")], "");
    let drive = local_drive(&fake);

    let created = drive.put_if("data/ab/obj", b"one", &Precondition::Absent);
    assert_eq!(created, Ok(Some(String::from("e1"))));
    let call = fake.last();
    assert_eq!(call.method, Method::Put);
    assert_eq!(header(&call, "if-none-match"), Some("*"));
    assert!(header(&call, "authorization")
        .unwrap()
        .contains("SignedHeaders=host;if-none-match;x-amz-content-sha256;x-amz-date,"));

    assert_eq!(
        drive.put_if("data/ab/obj", b"two", &Precondition::Absent),
        Err(DriveError::Conflict {
            key: String::from("data/ab/obj")
        })
    );

    let replaced = drive.put_if(
        "data/ab/obj",
        b"three",
        &Precondition::Matches(String::from("e1")),
    );
    assert_eq!(replaced, Ok(Some(String::from("e2"))));
    let call = fake.last();
    assert_eq!(header(&call, "if-match"), Some("\"e1\""), "the tag goes in quotes");
    assert_eq!(header(&call, "if-none-match"), None);
    assert_eq!(call.body, b"three");
    assert_eq!(fake.calls().len(), 3);
}

/// S3's answer to a conditional write that met another one in flight: nothing was written.
const IN_FLIGHT: &str = "<Error><Code>ConditionalRequestConflict</Code><Message>A conflicting \
     conditional operation is currently in progress against this resource. Please try \
     again.</Message></Error>";

#[test]
fn a_conditional_put_that_meets_another_in_flight_is_sent_again() {
    let fake = Fake::default();
    fake.answer(409, &[], IN_FLIGHT);
    fake.answer(200, &[("ETag", "\"e1\"")], "");
    let drive = local_drive(&fake);
    assert_eq!(
        drive.put_if("data/ab/obj", b"one", &Precondition::Absent),
        Ok(Some(String::from("e1")))
    );
    assert_eq!(fake.calls().len(), 2);
    assert_eq!(header(&fake.last(), "if-none-match"), Some("*"));
}

#[test]
fn a_conditional_put_that_keeps_meeting_others_says_so_after_a_few_tries() {
    let fake = Fake::default();
    for _ in 0..8 {
        fake.answer(409, &[], IN_FLIGHT);
    }
    let drive = local_drive(&fake);
    let result = drive.put_if("data/ab/obj", b"one", &Precondition::Matches(String::from("e1")));
    assert!(
        matches!(&result, Err(DriveError::Service(e)) if e.status == 409),
        "{result:?}"
    );
    let sent = fake.calls().len();
    assert!(sent > 1 && sent <= 5, "{sent} tries");
}

/// 16 MiB: the part size of a streamed upload.
const PART: usize = 16 * 1024 * 1024;

/// The drive of [`local_drive`] sending one part at a time: the scripted answers come in order.
fn one_part_at_a_time(fake: &Fake) -> S3Drive {
    local_drive(fake).with_parallel(1)
}

#[test]
fn a_streamed_body_above_one_part_goes_up_as_a_multipart_upload() {
    let fake = Fake::default();
    fake.answer(
        200,
        &[],
        "<InitiateMultipartUploadResult><Bucket>azdrive</Bucket><Key>big.bin</Key>\
         <UploadId>up-1</UploadId></InitiateMultipartUploadResult>",
    );
    fake.answer(200, &[("ETag", "\"p1\"")], "");
    fake.answer(200, &[("ETag", "\"p2\"")], "");
    fake.answer(
        200,
        &[],
        "<CompleteMultipartUploadResult><ETag>\"whole-2\"</ETag></CompleteMultipartUploadResult>",
    );
    let body: Vec<u8> = (0..PART + 5).map(|i| (i % 251) as u8).collect();
    let written = one_part_at_a_time(&fake)
        .put_from("big.bin", &mut &body[..])
        .unwrap();
    assert_eq!(written, body.len() as u64);

    let calls = fake.calls();
    assert_eq!(calls.len(), 4, "start, two parts, complete: {calls:?}");
    assert_eq!(calls[0].method, Method::Post);
    assert_eq!(calls[0].url, "http://127.0.0.1:9000/azdrive/big.bin?uploads=");
    assert_eq!(calls[1].method, Method::Put);
    assert_eq!(
        calls[1].url,
        "http://127.0.0.1:9000/azdrive/big.bin?partNumber=1&uploadId=up-1"
    );
    assert_eq!(calls[1].body, &body[..PART]);
    assert_eq!(
        calls[2].url,
        "http://127.0.0.1:9000/azdrive/big.bin?partNumber=2&uploadId=up-1"
    );
    assert_eq!(calls[2].body, &body[PART..]);
    assert_eq!(calls[3].method, Method::Post);
    assert_eq!(
        calls[3].url,
        "http://127.0.0.1:9000/azdrive/big.bin?uploadId=up-1"
    );
    let xml = String::from_utf8(calls[3].body.clone()).unwrap();
    assert_eq!(
        xml,
        "<CompleteMultipartUpload>\
         <Part><PartNumber>1</PartNumber><ETag>\"p1\"</ETag></Part>\
         <Part><PartNumber>2</PartNumber><ETag>\"p2\"</ETag></Part>\
         </CompleteMultipartUpload>"
    );
}

#[test]
fn a_multipart_upload_that_fails_is_aborted() {
    let fake = Fake::default();
    fake.answer(
        200,
        &[],
        "<InitiateMultipartUploadResult><UploadId>up-2</UploadId></InitiateMultipartUploadResult>",
    );
    fake.answer(200, &[("ETag", "\"p1\"")], "");
    fake.answer(
        500,
        &[],
        "<Error><Code>InternalError</Code><Message>try again</Message></Error>",
    );
    fake.answer(204, &[], "");
    let body = vec![7u8; PART + 1];
    let error = one_part_at_a_time(&fake)
        .put_from("big.bin", &mut &body[..])
        .unwrap_err();
    assert!(matches!(error, DriveError::Service(ref e) if e.status == 500), "{error:?}");
    let last = fake.last();
    assert_eq!(last.method, Method::Delete, "the parts sent are not left behind");
    assert_eq!(
        last.url,
        "http://127.0.0.1:9000/azdrive/big.bin?uploadId=up-2"
    );
    assert_eq!(fake.calls().len(), 4);
}

#[test]
fn a_streamed_body_of_one_part_or_less_goes_up_in_one_put() {
    let fake = Fake::default();
    fake.answer(200, &[("ETag", "\"one\"")], "");
    fake.answer(200, &[("ETag", "\"two\"")], "");
    let drive = local_drive(&fake);
    assert_eq!(drive.put_from("small.txt", &mut &b"hello"[..]).unwrap(), 5);
    assert_eq!(fake.last().method, Method::Put);
    assert_eq!(fake.last().body, b"hello");
    let exactly_one_part = vec![1u8; PART];
    assert_eq!(
        drive
            .put_from("part.bin", &mut &exactly_one_part[..])
            .unwrap(),
        PART as u64
    );
    assert_eq!(fake.calls().len(), 2, "one PUT each");
    assert_eq!(fake.last().body.len(), PART);
}
