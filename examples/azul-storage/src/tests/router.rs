//! An S3 drive's requests through a [`Router`]: the seam a drive whose bucket answers at several
//! endpoints (an Azlin drive's nodes) fails over with. Every attempt is signed anew for the
//! endpoint it goes to (SigV4 signs the host), the router sees which object a request is about,
//! and every request of an upload - each part - goes through it. A transport says whether it can
//! connect to a host at known addresses when the name does not resolve; by default it cannot.

use std::sync::{Arc, Mutex};

use super::fake_bucket::{FakeBucket, BUCKET};
use crate::{
    s3::{Routed, Router},
    Credentials, Drive, DriveError, HttpCall, HttpReply, Method, S3Config, S3Drive, Transport,
};

const HOME: &str = "http://127.0.0.1:9000";
const OTHER: &str = "http://127.0.0.1:9002";

/// The fake service behind every host but the dead ones; records every call.
struct Hosts {
    dead: Vec<&'static str>,
    s3: Arc<FakeBucket>,
    calls: Arc<Mutex<Vec<HttpCall>>>,
}

impl Transport for Hosts {
    fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
        self.calls.lock().unwrap().push(call.clone());
        if self.dead.iter().any(|dead| call.url.starts_with(dead)) {
            return Err(String::from("connection refused"));
        }
        self.s3.answer(call)
    }
}

/// Tries its endpoints in order until one answers; remembers what it was asked.
struct InOrder {
    endpoints: Vec<&'static str>,
    seen: Mutex<Vec<(Method, Option<String>, String)>>,
}

impl Router for InOrder {
    fn send(
        &self,
        request: &Routed<'_>,
        transport: &dyn Transport,
    ) -> Result<HttpReply, DriveError> {
        self.seen.lock().unwrap().push((
            request.method(),
            request.key().map(str::to_string),
            request.endpoint().to_string(),
        ));
        let mut last = None;
        for endpoint in &self.endpoints {
            let call = request.signed_for(endpoint)?;
            match transport.send(&call) {
                Ok(reply) => return Ok(reply),
                Err(why) => last = Some(why),
            }
        }
        Err(DriveError::Transport(last.unwrap_or_default()))
    }
}

fn routed(
    dead: Vec<&'static str>,
) -> (
    S3Drive,
    Arc<InOrder>,
    Arc<FakeBucket>,
    Arc<Mutex<Vec<HttpCall>>>,
) {
    let s3 = FakeBucket::new();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let router = Arc::new(InOrder {
        endpoints: vec![HOME, OTHER],
        seen: Mutex::new(Vec::new()),
    });
    let drive = S3Drive::new(
        S3Config {
            endpoint: HOME.to_string(),
            region: String::from("us-east-1"),
            bucket: BUCKET.to_string(),
            path_style: true,
        },
        Credentials::new("AKIDTEST", "test-secret"),
        Box::new(Hosts {
            dead,
            s3: s3.clone(),
            calls: calls.clone(),
        }),
    )
    .unwrap()
    .with_clock(|| 1_791_590_400)
    .with_router(router.clone());
    (drive, router, s3, calls)
}

fn authorization(call: &HttpCall) -> String {
    call.headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("authorization"))
        .map(|(_, v)| v.clone())
        .unwrap_or_default()
}

#[test]
fn a_routed_request_is_signed_anew_for_the_endpoint_it_goes_to() {
    let (drive, router, s3, calls) = routed(vec![HOME]);
    s3.write("a.txt", b"alpha");
    assert_eq!(drive.get("a.txt").unwrap(), b"alpha");
    let calls = calls.lock().unwrap().clone();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(calls[0].url.starts_with(&format!("{HOME}/{BUCKET}/a.txt")));
    assert!(calls[1].url.starts_with(&format!("{OTHER}/{BUCKET}/a.txt")));
    assert_ne!(
        authorization(&calls[0]),
        authorization(&calls[1]),
        "SigV4 signs the host: each endpoint its own signature"
    );
    assert_eq!(
        *router.seen.lock().unwrap(),
        vec![(Method::Get, Some(String::from("a.txt")), HOME.to_string())],
        "one request, about the object, from the drive's own endpoint"
    );
}

#[test]
fn a_listing_is_routed_as_a_request_on_the_bucket() {
    let (drive, router, _s3, _calls) = routed(vec![]);
    let _ = drive.list(&crate::ListRequest::folder(""));
    let seen = router.seen.lock().unwrap().clone();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].1, None, "no object: the bucket");
}

#[test]
fn every_part_of_an_upload_goes_through_the_router() {
    let (drive, router, s3, _calls) = routed(vec![HOME]);
    let drive = drive.with_part_size(1024);
    let body = vec![5u8; 3 * 1024 + 1];
    drive.put_from("big.bin", &mut &body[..]).unwrap();
    assert_eq!(
        s3.object("big.bin").unwrap(),
        body,
        "through the other endpoint"
    );
    let seen = router.seen.lock().unwrap().clone();
    let methods: Vec<Method> = seen.iter().map(|(m, _, _)| *m).collect();
    assert_eq!(
        methods,
        vec![
            Method::Post,
            Method::Put,
            Method::Put,
            Method::Put,
            Method::Put,
            Method::Post
        ],
        "start, four parts, complete"
    );
}

#[test]
fn a_transport_cannot_connect_by_address_unless_it_says_so() {
    let transport = Hosts {
        dead: Vec::new(),
        s3: FakeBucket::new(),
        calls: Arc::new(Mutex::new(Vec::new())),
    };
    assert!(!transport.fallback_addresses("n2.example.test", &[String::from("192.0.2.7")]));
}
