//! The four failover layers of an Azlin drive's requests (D35) and their retries:
//!
//! 1. the block endpoint;
//! 2. the hint of a node that cannot serve (`503` + `x-azlin-alt-endpoints`): the SAME request
//!    goes there at once;
//! 3. the node list of the last credential refresh, ready nodes first - the one that answered
//!    stays first for ten minutes;
//! 4. a node whose name does not resolve is reached at its addresses, its name still the one the
//!    request (and TLS) goes to.
//!
//! What is tried again, and how, is the class of the answer in `user_errors` (no table of its
//! own): busy is asked again after a backoff, a pause a node asks for is kept (a long one is the
//! caller's to wait out), a drive that moved is followed, an answer the user must act on (full,
//! signed out, a conflict) is not repeated. An Azlin drive reaches the nodes its last refresh
//! listed, also after a restart.

use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use azul_storage::{
    config::keyring_key,
    keyring::{KeyringStore, MemoryKeyring},
    testing::TempDir,
    Credentials, Drive, DriveError, HttpCall, HttpReply, ListRequest, S3Config, Transport,
};

use super::{bundle, empty_listing, fake_s3::FakeS3, header, json, Fake, Shared, S3, TOKEN};
use crate::{
    bucket::Bucket,
    drive::{AzlinDrive, TransportFactory},
    failover::{Node, Retry, STICKY_SECS},
    lock::LockDir,
    shared::SharedKeyring,
    AzlinSession, DriveBundle,
};

const N1: &str = "http://127.0.0.1:19001";
const N2: &str = "http://127.0.0.1:19002";
const MOVED: &str = "http://127.0.0.1:19005";

/// The pauses a bucket took.
type Pauses = Arc<Mutex<Vec<Duration>>>;

fn factory(fake: &Arc<Fake>) -> TransportFactory {
    let fake = fake.clone();
    Arc::new(move || Box::new(Shared(fake.clone())) as Box<dyn Transport>)
}

fn config() -> S3Config {
    S3Config {
        endpoint: S3.to_string(),
        region: String::from("us-east-1"),
        bucket: String::from("d-1"),
        path_style: true,
    }
}

/// A bucket on `transports` whose pauses are recorded, not slept.
fn bucket_with(transports: TransportFactory, clock: Arc<AtomicU64>) -> (Bucket, Pauses) {
    let pauses: Pauses = Arc::new(Mutex::new(Vec::new()));
    let heard = pauses.clone();
    let bucket = Bucket::new(
        config(),
        Credentials::new("AKID1", "secret-of-AKID1"),
        transports,
    )
    .unwrap()
    .with_clock(move || clock.load(Ordering::SeqCst))
    .with_sleep(Arc::new(move |pause| heard.lock().unwrap().push(pause)));
    (bucket, pauses)
}

fn clock() -> Arc<AtomicU64> {
    Arc::new(AtomicU64::new(1_791_590_400))
}

fn reply(status: u16, headers: &[(&str, &str)], body: &str) -> HttpReply {
    HttpReply {
        status,
        headers: headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
        body: body.as_bytes().to_vec(),
    }
}

fn hosts(fake: &Fake) -> Vec<String> {
    fake.calls()
        .iter()
        .map(|c| {
            let rest = c.url.split_once("://").map_or("", |(_, r)| r);
            let authority = rest.split('/').next().unwrap_or("");
            format!("http://{authority}")
        })
        .collect()
}

fn node(name: &str, url: &str, ready: bool) -> Node {
    Node {
        name: name.to_string(),
        url: url.to_string(),
        addresses: Vec::new(),
        ready,
    }
}

#[test]
fn a_node_that_cannot_serve_hands_the_same_request_to_the_node_it_names() {
    let s3 = FakeS3::new();
    s3.write("a.txt", b"alpha".to_vec());
    let service = s3.clone();
    let fake = Fake::new(move |call, _| {
        if call.url.starts_with(S3) {
            return Ok(reply(
                503,
                &[
                    ("x-azlin-error", "unavailable"),
                    ("x-azlin-alt-endpoints", N2),
                    ("Retry-After", "0"),
                ],
                "<Error><Code>ServiceUnavailable</Code><Message>draining</Message></Error>",
            ));
        }
        Ok(service.answer(call))
    });
    let (bucket, pauses) = bucket_with(factory(&fake), clock());
    let bucket = bucket.with_nodes(vec![node("n1", N1, true)]);
    assert_eq!(bucket.get("a.txt").unwrap().unwrap(), b"alpha");
    assert_eq!(
        hosts(&fake),
        vec![S3.to_string(), N2.to_string()],
        "the hint comes before the node list"
    );
    assert!(
        pauses.lock().unwrap().is_empty(),
        "Retry-After: 0 - at once"
    );
}

#[test]
fn without_a_hint_the_node_list_is_asked_ready_nodes_first() {
    let s3 = FakeS3::new();
    s3.write("a.txt", b"alpha".to_vec());
    let service = s3.clone();
    let fake = Fake::new(move |call, _| {
        if call.url.starts_with(S3) {
            return Err(String::from("connection refused"));
        }
        Ok(service.answer(call))
    });
    let (bucket, _) = bucket_with(factory(&fake), clock());
    let bucket = bucket.with_nodes(Node::list(&[
        serde_json::json!({"name": "n1", "url": N1, "ready": false}),
        serde_json::json!({"name": "n2", "url": N2, "ready": true, "ipv4": "127.0.0.1"}),
    ]));
    assert_eq!(bucket.get("a.txt").unwrap().unwrap(), b"alpha");
    assert_eq!(hosts(&fake), vec![S3.to_string(), N2.to_string()]);
}

#[test]
fn the_node_list_reads_the_token_servers_names_urls_and_addresses() {
    let nodes = Node::list(&[
        serde_json::json!({"name": "n1", "url": N1, "host": "127.0.0.1", "ipv4": "192.0.2.1",
                           "ipv6": "2001:db8::1", "ready": true}),
        serde_json::json!({"name": "n2", "public_url": N2, "ipv4": null, "ready": true}),
        serde_json::json!({"name": "no url"}),
    ]);
    assert_eq!(nodes.len(), 2, "{nodes:?}");
    assert_eq!(nodes[0].url, N1);
    assert_eq!(nodes[0].addresses, vec!["192.0.2.1", "2001:db8::1"]);
    assert_eq!(nodes[1].url, N2);
    assert!(nodes[1].addresses.is_empty());
}

/// Answers through the fake service, but a host that is no IP address resolves only once the
/// transport was told its addresses.
struct Resolving {
    s3: Arc<FakeS3>,
    told: Mutex<Vec<(String, Vec<String>)>>,
    calls: Mutex<Vec<HttpCall>>,
}

impl Transport for Resolving {
    fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
        self.calls.lock().unwrap().push(call.clone());
        let rest = call.url.split_once("://").map_or("", |(_, r)| r);
        let host = rest
            .split('/')
            .next()
            .unwrap_or("")
            .rsplit_once(':')
            .map_or(rest, |(h, _)| h)
            .to_string();
        let known = host.parse::<std::net::IpAddr>().is_ok()
            || self.told.lock().unwrap().iter().any(|(h, _)| *h == host);
        if !known {
            return Err(format!("DNS resolution failed for {}", call.url));
        }
        Ok(self.s3.answer(call))
    }

    fn fallback_addresses(&self, host: &str, addresses: &[String]) -> bool {
        self.told
            .lock()
            .unwrap()
            .push((host.to_string(), addresses.to_vec()));
        true
    }
}

struct SharedResolving(Arc<Resolving>);

impl Transport for SharedResolving {
    fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
        self.0.send(call)
    }
    fn fallback_addresses(&self, host: &str, addresses: &[String]) -> bool {
        self.0.fallback_addresses(host, addresses)
    }
}

#[test]
fn a_node_whose_name_does_not_resolve_is_reached_at_its_address_under_its_name() {
    let s3 = FakeS3::new();
    s3.write("a.txt", b"alpha".to_vec());
    let resolving = Arc::new(Resolving {
        s3: s3.clone(),
        told: Mutex::new(Vec::new()),
        calls: Mutex::new(Vec::new()),
    });
    let shared = resolving.clone();
    let transports: TransportFactory =
        Arc::new(move || Box::new(SharedResolving(shared.clone())) as Box<dyn Transport>);
    let pauses: Pauses = Arc::new(Mutex::new(Vec::new()));
    let heard = pauses.clone();
    let bucket = Bucket::new(
        S3Config {
            endpoint: String::from("http://block.nodes.test:19000"),
            ..config()
        },
        Credentials::new("AKID1", "secret-of-AKID1"),
        transports,
    )
    .unwrap()
    .with_sleep(Arc::new(move |pause| heard.lock().unwrap().push(pause)))
    .with_nodes(vec![Node {
        name: String::from("n2"),
        url: String::from("http://n2.nodes.test:19002"),
        addresses: vec![String::from("127.0.0.1")],
        ready: true,
    }]);
    assert_eq!(bucket.get("a.txt").unwrap().unwrap(), b"alpha");
    assert_eq!(
        *resolving.told.lock().unwrap(),
        vec![(
            String::from("n2.nodes.test"),
            vec![String::from("127.0.0.1")]
        )],
        "only the node with addresses, only after its name failed"
    );
    let last = resolving.calls.lock().unwrap().last().cloned().unwrap();
    assert!(
        last.url.starts_with("http://n2.nodes.test:19002/"),
        "the request still names the node (TLS verifies that name): {}",
        last.url
    );
}

#[test]
fn the_node_that_answered_stays_first_for_ten_minutes() {
    let s3 = FakeS3::new();
    s3.write("a.txt", b"alpha".to_vec());
    let service = s3.clone();
    let block_down = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let down = block_down.clone();
    let fake = Fake::new(move |call, _| {
        if call.url.starts_with(S3) && down.load(Ordering::SeqCst) {
            return Err(String::from("connection refused"));
        }
        Ok(service.answer(call))
    });
    let now = clock();
    let (bucket, _) = bucket_with(factory(&fake), now.clone());
    let bucket = bucket.with_nodes(vec![node("n2", N2, true)]);
    bucket.get("a.txt").unwrap();
    block_down.store(false, Ordering::SeqCst);
    bucket.get("a.txt").unwrap();
    assert_eq!(
        hosts(&fake),
        vec![S3.to_string(), N2.to_string(), N2.to_string()],
        "the second request goes to the node that answered"
    );
    now.fetch_add(STICKY_SECS + 1, Ordering::SeqCst);
    bucket.get("a.txt").unwrap();
    assert_eq!(
        hosts(&fake).last().unwrap(),
        S3,
        "ten minutes on: the block endpoint"
    );
}

#[test]
fn a_busy_service_is_asked_again_after_a_growing_backoff() {
    let s3 = FakeS3::new();
    s3.write("a.txt", b"alpha".to_vec());
    let service = s3.clone();
    let fake = Fake::new(move |call, n| {
        if n < 2 {
            return Ok(reply(
                503,
                &[],
                "<Error><Code>SlowDown</Code><Message>later</Message></Error>",
            ));
        }
        Ok(service.answer(call))
    });
    let (bucket, pauses) = bucket_with(factory(&fake), clock());
    assert_eq!(bucket.get("a.txt").unwrap().unwrap(), b"alpha");
    assert_eq!(fake.calls().len(), 3);
    let pauses = pauses.lock().unwrap().clone();
    assert_eq!(pauses.len(), 2, "{pauses:?}");
    assert!(pauses[1] > pauses[0], "the backoff grows: {pauses:?}");
}

#[test]
fn the_pause_a_node_asks_for_is_kept_and_a_long_one_is_the_callers_to_wait_out() {
    let s3 = FakeS3::new();
    s3.write("a.txt", b"alpha".to_vec());
    let service = s3.clone();
    let fake = Fake::new(move |call, n| {
        if n == 0 {
            return Ok(reply(
                503,
                &[("x-azlin-error", "maintenance"), ("Retry-After", "2")],
                "",
            ));
        }
        Ok(service.answer(call))
    });
    let (bucket, pauses) = bucket_with(factory(&fake), clock());
    bucket.get("a.txt").unwrap();
    assert_eq!(*pauses.lock().unwrap(), vec![Duration::from_secs(2)]);

    let long = Fake::new(|_, _| {
        Ok(reply(
            503,
            &[("x-azlin-error", "maintenance"), ("Retry-After", "3600")],
            "",
        ))
    });
    let (bucket, pauses) = bucket_with(factory(&long), clock());
    match bucket.get("a.txt") {
        Err(crate::CloudError::Drive(DriveError::Service(e))) => {
            assert_eq!(e.azlin_error.as_deref(), Some("maintenance"));
            assert_eq!(e.retry_after, Some(3600));
        }
        other => panic!("not the node's answer: {other:?}"),
    }
    assert_eq!(long.calls().len(), 1, "not asked again within the call");
    assert!(pauses.lock().unwrap().is_empty());
}

#[test]
fn an_answer_the_user_must_act_on_is_not_asked_again() {
    for (status, code) in [
        (403, Some("quota_exceeded")),
        (403, Some("credentials_revoked")),
        (412, None),
        (404, None),
    ] {
        let fake = Fake::new(move |_, _| {
            let headers: Vec<(&str, &str)> =
                code.map(|c| ("x-azlin-error", c)).into_iter().collect();
            Ok(reply(
                status,
                &headers,
                "<Error><Code>Refused</Code><Message>no</Message></Error>",
            ))
        });
        let (bucket, pauses) = bucket_with(factory(&fake), clock());
        let bucket = bucket.with_nodes(vec![node("n2", N2, true)]);
        let _ = bucket.put_if("a.txt", b"x", None);
        assert_eq!(
            fake.calls().len(),
            1,
            "{status} {code:?}: {:?}",
            hosts(&fake)
        );
        assert!(pauses.lock().unwrap().is_empty());
    }
}

#[test]
fn a_drive_that_moved_is_followed_to_the_endpoint_the_answer_names() {
    let s3 = FakeS3::new();
    s3.write("a.txt", b"alpha".to_vec());
    let service = s3.clone();
    let fake = Fake::new(move |call, _| {
        if call.url.starts_with(S3) {
            return Ok(reply(
                301,
                &[("x-azlin-error", "wrong_block")],
                &format!(
                    "<Error><Code>PermanentRedirect</Code><Message>moved</Message>\
                     <Endpoint>{MOVED}</Endpoint></Error>"
                ),
            ));
        }
        Ok(service.answer(call))
    });
    let (bucket, _) = bucket_with(factory(&fake), clock());
    assert_eq!(bucket.get("a.txt").unwrap().unwrap(), b"alpha");
    assert_eq!(hosts(&fake), vec![S3.to_string(), MOVED.to_string()]);
}

#[test]
fn each_part_of_a_big_upload_fails_over_on_its_own() {
    let s3 = FakeS3::new();
    let service = s3.clone();
    let fake = Fake::new(move |call, _| {
        if call.url.starts_with(S3) && call.url.contains("partNumber=2&") {
            return Err(String::from("connection reset"));
        }
        Ok(service.answer(call))
    });
    let (mut bucket, _) = bucket_with(factory(&fake), clock());
    bucket.set_part_size(4);
    let bucket = bucket.with_nodes(vec![node("n2", N2, true)]);
    let data: Vec<u8> = (0..23u8).collect();
    bucket.put("big.bin", &data).unwrap();
    assert_eq!(s3.read("big.bin").unwrap(), data, "the upload is whole");
    let calls = fake.calls();
    assert!(
        calls
            .iter()
            .any(|c| c.url.starts_with(N2) && c.url.contains("partNumber=2&")),
        "the part the block endpoint dropped went to the node: {:?}",
        hosts(&fake)
    );
    assert!(
        calls
            .iter()
            .any(|c| c.url.starts_with(S3) && c.url.contains("partNumber=2&")),
        "it was the block endpoint that dropped it: {:?}",
        hosts(&fake)
    );
}

/// The token server answers a refresh with a bundle whose node list names `N2`; the block
/// endpoint gives no answer, the node answers a listing.
fn cloud_with_nodes() -> Arc<Fake> {
    Fake::new(move |call, _| {
        if call.url.starts_with(TOKEN) {
            return Ok(match header(call, "authorization") {
                Some("Bearer dt_f.0.aaa") => json(
                    200,
                    &bundle("AKID2", "2026-10-09T09:15:00Z", "dt_f.1.bbb").replace(
                        r#""nodes": []"#,
                        &format!(r#""nodes": [{{"name": "n2", "url": "{N2}", "ready": true}}]"#),
                    ),
                ),
                _ => json(401, r#"{"error": "token_reuse", "message": "reused"}"#),
            });
        }
        if call.url.starts_with(S3) {
            return Err(String::from("connection refused"));
        }
        Ok(empty_listing())
    })
}

fn azlin_drive(
    fake: &Arc<Fake>,
    dir: &TempDir,
    now: u64,
    nodes_file: &std::path::Path,
) -> AzlinDrive {
    let first = DriveBundle::parse(&bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa")).unwrap();
    let keyring = Arc::new(MemoryKeyring::new());
    keyring
        .set(&keyring_key("d_1"), &first.session().to_keyring_secret())
        .unwrap();
    let shared = SharedKeyring::new(keyring, LockDir::new(dir.path()));
    AzlinDrive::new(
        &first.entry_named("Cloud", TOKEN),
        first.session(),
        TOKEN,
        shared,
        factory(fake),
        Box::new(|_: &AzlinSession, _: Result<(), String>| {}),
    )
    .unwrap()
    .with_clock(move || now)
    .with_nodes_file(nodes_file)
}

#[test]
fn an_azlin_drive_reaches_the_nodes_its_last_refresh_listed_also_after_a_restart() {
    let dir = TempDir::new("azcloud-failover");
    let nodes_file = dir.path().join("nodes").join("d_1.json");
    let fake = cloud_with_nodes();
    // 2026-10-08T21:05:00Z: the first credentials run out - the refresh lists the nodes.
    let drive = azlin_drive(&fake, &dir, 1_791_493_500, &nodes_file);
    drive.failover().set_retry(Retry::once());
    drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(drive.nodes(), vec![node("n2", N2, true)]);
    assert!(
        fake.calls().iter().any(|c| c.url.starts_with(N2)),
        "the listing went to the node"
    );
    assert!(
        nodes_file.is_file(),
        "the node list is kept for the next start"
    );

    // The app starts again, with credentials that need no refresh: the kept list is used.
    let again = Fake::new(|call, _| {
        if call.url.starts_with(S3) {
            return Err(String::from("connection refused"));
        }
        Ok(empty_listing())
    });
    let drive = azlin_drive(&again, &dir, 1_791_450_900, &nodes_file);
    drive.failover().set_retry(Retry::once());
    drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(hosts(&again), vec![S3.to_string(), N2.to_string()]);
}

/// A node of the list at `addresses`.
fn node_at(name: &str, url: &str, addresses: &[&str], ready: bool) -> Node {
    Node {
        addresses: addresses.iter().map(|a| a.to_string()).collect(),
        ..node(name, url, ready)
    }
}

#[test]
fn with_dns_down_from_the_start_the_block_endpoint_answers_at_the_nodes_addresses() {
    let s3 = FakeS3::new();
    s3.write("a.txt", b"alpha".to_vec());
    let resolving = Arc::new(Resolving {
        s3: s3.clone(),
        told: Mutex::new(Vec::new()),
        calls: Mutex::new(Vec::new()),
    });
    let shared = resolving.clone();
    let transports: TransportFactory =
        Arc::new(move || Box::new(SharedResolving(shared.clone())) as Box<dyn Transport>);
    let bucket = Bucket::new(
        S3Config {
            endpoint: String::from("http://block.nodes.test:19000"),
            ..config()
        },
        Credentials::new("AKID1", "secret-of-AKID1"),
        transports,
    )
    .unwrap()
    .with_sleep(Arc::new(|_| {}))
    .with_nodes(vec![
        node_at("n2", "http://n2.nodes.test:19002", &["127.0.0.2"], false),
        node_at(
            "n1",
            "http://n1.nodes.test:19001",
            &["127.0.0.1", "::1"],
            true,
        ),
    ]);
    assert_eq!(bucket.get("a.txt").unwrap().unwrap(), b"alpha");
    let calls = resolving.calls.lock().unwrap().clone();
    assert_eq!(
        calls.len(),
        1,
        "the addresses were known before the first request: no request failed on a name"
    );
    assert!(
        calls[0].url.starts_with("http://block.nodes.test:19000/"),
        "still the block endpoint, under its name (its certificate covers it): {}",
        calls[0].url
    );
    let told = resolving.told.lock().unwrap().clone();
    let of = |host: &str| -> Vec<String> {
        told.iter()
            .find(|(h, _)| h == host)
            .map(|(_, a)| a.clone())
            .unwrap_or_default()
    };
    assert_eq!(
        of("block.nodes.test"),
        vec!["127.0.0.1", "::1", "127.0.0.2"],
        "the block host: every node's addresses, the ready nodes' first"
    );
    assert_eq!(of("n1.nodes.test"), vec!["127.0.0.1", "::1"]);
    assert_eq!(of("n2.nodes.test"), vec!["127.0.0.2"]);
}

#[test]
fn the_node_list_reads_each_nodes_iroh_id_and_iroh_sockets() {
    let nodes = Node::list(&[
        serde_json::json!({"name": "n1", "url": N1, "ready": true, "iroh_id": "aa",
                           "iroh_addrs": ["192.0.2.1:7001", "[2001:db8::1]:7001"]}),
        serde_json::json!({"name": "n2", "url": N2, "ready": true, "sign_pubkey": "bb"}),
        serde_json::json!({"name": "n3", "ready": true, "iroh_id": "cc",
                           "iroh_addrs": ["192.0.2.3:7001"]}),
        serde_json::json!({"name": "n4", "ready": true}),
    ]);
    assert_eq!(
        nodes.len(),
        3,
        "a node without a URL is kept when it can be dialed: {nodes:?}"
    );
    assert_eq!(nodes[0].iroh_id.as_deref(), Some("aa"));
    assert_eq!(
        nodes[0].iroh_addrs,
        vec!["192.0.2.1:7001", "[2001:db8::1]:7001"]
    );
    assert_eq!(
        nodes[1].iroh_id.as_deref(),
        Some("bb"),
        "sign_pubkey is the iroh id"
    );
    assert!(nodes[1].iroh_addrs.is_empty());
    assert_eq!(nodes[2].name, "n3");
    assert!(nodes[2].url.is_empty());
    let kept: Vec<Node> = serde_json::from_str(&serde_json::to_string(&nodes).unwrap()).unwrap();
    assert_eq!(kept, nodes, "the nodes file keeps them");
    let older: Vec<Node> =
        serde_json::from_str(r#"[{"name": "n1", "url": "http://n1", "ready": true}]"#).unwrap();
    assert_eq!(older[0].iroh_id, None, "a nodes file of before reads");
}
