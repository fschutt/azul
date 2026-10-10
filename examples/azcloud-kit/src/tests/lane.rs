//! The iroh lane of a drive's requests, before the four HTTPS layers of the failover: every ready
//! node the node list names with an iroh id is dialed at its iroh sockets, in the list's order,
//! each probed once (one signed HEAD) before its first request; a node that fails rests for five
//! minutes - its own backoff, the other nodes are still asked - and the request goes on to the
//! next node, then to HTTPS. An answer iroh carried is the service's (a refusal is not repeated
//! over HTTPS); a busy one goes on. A drive whose node list names no iroh id (a plain S3 bucket
//! has no node list at all) never dials.

use std::sync::{
    atomic::{AtomicI64, Ordering},
    Arc, Mutex,
};

use azul_storage::{Credentials, HttpReply, Method, S3Config, Transport};

use super::{fake_s3::FakeS3, Fake, Shared, S3};
use crate::{
    bucket::Bucket,
    drive::TransportFactory,
    failover::Node,
    transport::{targets_from_nodes, IrohDialer, IrohLane, IrohTarget, Lane, IROH_RETRY_SECS},
};

fn factory(fake: &Arc<Fake>) -> TransportFactory {
    let fake = fake.clone();
    Arc::new(move || Box::new(Shared(fake.clone())) as Box<dyn Transport>)
}

/// A way to the S3 service in memory that records its calls.
fn through(s3: &Arc<FakeS3>) -> Arc<Fake> {
    let s3 = s3.clone();
    Fake::new(move |call, _| Ok(s3.answer(call)))
}

/// Dials the nodes it knows by their iroh id; any other id has no route.
struct Dialer {
    nodes: Vec<(String, Arc<Fake>)>,
    dialed: Mutex<Vec<IrohTarget>>,
}

impl Dialer {
    fn to(nodes: &[(&str, &Arc<Fake>)]) -> Arc<Dialer> {
        Arc::new(Dialer {
            nodes: nodes
                .iter()
                .map(|(id, fake)| (id.to_string(), (*fake).clone()))
                .collect(),
            dialed: Mutex::new(Vec::new()),
        })
    }

    fn ids(&self) -> Vec<String> {
        self.dialed
            .lock()
            .unwrap()
            .iter()
            .map(|t| t.id.clone())
            .collect()
    }
}

impl IrohDialer for Dialer {
    fn dial(
        &self,
        target: &IrohTarget,
        _relay: Option<&str>,
    ) -> Result<Box<dyn Transport>, String> {
        self.dialed.lock().unwrap().push(target.clone());
        self.nodes
            .iter()
            .find(|(id, _)| *id == target.id)
            .map(|(_, fake)| Box::new(Shared(fake.clone())) as Box<dyn Transport>)
            .ok_or_else(|| String::from("no route to the node"))
    }
}

fn iroh_node(name: &str, id: Option<&str>, iroh_addrs: &[&str], ready: bool) -> Node {
    Node {
        name: name.to_string(),
        url: format!("http://{name}.nodes.test:19001"),
        addresses: Vec::new(),
        ready,
        iroh_id: id.map(String::from),
        iroh_addrs: iroh_addrs.iter().map(|a| a.to_string()).collect(),
    }
}

/// A bucket whose HTTPS requests go to `https`, with `nodes` and an iroh lane over `dialer`.
fn lane_bucket(
    https: &Arc<Fake>,
    nodes: &[Node],
    dialer: &Arc<Dialer>,
    clock: &Arc<AtomicI64>,
) -> (Bucket, Arc<IrohLane>) {
    let bucket = Bucket::new(
        S3Config {
            endpoint: S3.to_string(),
            region: String::from("us-east-1"),
            bucket: String::from("d-1"),
            path_style: true,
        },
        Credentials::new("AKID1", "secret-of-AKID1"),
        factory(https),
    )
    .unwrap()
    .with_sleep(Arc::new(|_| {}))
    .with_nodes(nodes.to_vec());
    let now = clock.clone();
    let lane = Arc::new(
        IrohLane::new(dialer.clone(), None)
            .with_clock(Arc::new(move || now.load(Ordering::SeqCst))),
    );
    bucket.failover().set_lane(Some(lane.clone()));
    (bucket, lane)
}

fn clock() -> Arc<AtomicI64> {
    Arc::new(AtomicI64::new(1_791_590_400))
}

fn methods(fake: &Fake) -> Vec<Method> {
    fake.calls().iter().map(|c| c.method).collect()
}

#[test]
fn a_node_that_names_its_iroh_id_is_asked_over_iroh_first_and_https_never() {
    let s3 = FakeS3::new();
    let (https, n1) = (through(&s3), through(&s3));
    let dialer = Dialer::to(&[("aa", &n1)]);
    let nodes = vec![iroh_node(
        "n1",
        Some("aa"),
        &["192.0.2.1:7001", "[2001:db8::1]:7001"],
        true,
    )];
    let (bucket, lane) = lane_bucket(&https, &nodes, &dialer, &clock());
    bucket.put("a.txt", b"a").unwrap();
    assert_eq!(s3.read("a.txt").unwrap(), b"a");
    assert!(https.calls().is_empty(), "nothing over https");
    assert_eq!(
        methods(&n1),
        vec![Method::Head, Method::Put],
        "a probe, then the request"
    );
    assert!(n1.calls()[0].url.ends_with("/.azlin/probe"));
    let dialed = dialer.dialed.lock().unwrap().clone();
    assert_eq!(dialed[0].id, "aa");
    assert_eq!(
        dialed[0].addrs,
        vec!["192.0.2.1:7001", "[2001:db8::1]:7001"],
        "dialed at every socket the node list names"
    );
    assert_eq!(lane.report().lane, Some(Lane::Iroh));
    assert_eq!(lane.lane_now(&nodes), Lane::Iroh);
    bucket.put("b.txt", b"b").unwrap();
    assert_eq!(
        methods(&n1),
        vec![Method::Head, Method::Put, Method::Put],
        "probed once"
    );
    assert_eq!(dialer.ids(), vec!["aa"], "dialed once");
}

#[test]
fn when_iroh_fails_the_same_request_goes_over_https_and_that_node_rests_for_five_minutes() {
    let s3 = FakeS3::new();
    let https = through(&s3);
    let dead = Fake::new(|_, _| Err(String::from("connection refused")));
    let dialer = Dialer::to(&[("aa", &dead)]);
    let nodes = vec![iroh_node("n1", Some("aa"), &["192.0.2.1:7001"], true)];
    let now = clock();
    let (bucket, lane) = lane_bucket(&https, &nodes, &dialer, &now);
    bucket.put("a.txt", b"a").unwrap();
    assert_eq!(s3.read("a.txt").unwrap(), b"a", "over https");
    assert_eq!(
        dead.calls().len(),
        1,
        "the probe failed: the request never went over iroh"
    );
    assert_eq!(lane.lane_now(&nodes), Lane::Https);
    let report = lane.report();
    assert_eq!(report.lane, Some(Lane::Https));
    assert!(
        report.reason.contains("iroh failed") && report.reason.contains("connection refused"),
        "{report:?}"
    );
    bucket.put("b.txt", b"b").unwrap();
    assert_eq!(dead.calls().len(), 1, "the node rests");
    assert_eq!(methods(&https), vec![Method::Put, Method::Put]);
    now.fetch_add(IROH_RETRY_SECS, Ordering::SeqCst);
    bucket.put("c.txt", b"c").unwrap();
    assert_eq!(
        dead.calls().len(),
        2,
        "five minutes later iroh is tried again"
    );
    assert_eq!(s3.read("c.txt").unwrap(), b"c");
}

#[test]
fn every_ready_node_with_an_iroh_id_is_dialed_in_turn_until_one_answers() {
    let s3 = FakeS3::new();
    let (https, n2, n3) = (through(&s3), through(&s3), through(&s3));
    // n1 has no route; n3 is not ready.
    let dialer = Dialer::to(&[("bb", &n2), ("cc", &n3)]);
    let nodes = vec![
        iroh_node("n1", Some("aa"), &["192.0.2.1:7001"], true),
        iroh_node("n2", Some("bb"), &["192.0.2.2:7001"], true),
        iroh_node("n3", Some("cc"), &["192.0.2.3:7001"], false),
    ];
    let (bucket, lane) = lane_bucket(&https, &nodes, &dialer, &clock());
    bucket.put("a.txt", b"a").unwrap();
    assert_eq!(s3.read("a.txt").unwrap(), b"a");
    assert_eq!(dialer.ids(), vec!["aa", "bb"]);
    assert_eq!(methods(&n2), vec![Method::Head, Method::Put]);
    assert!(
        n3.calls().is_empty(),
        "a node that is not ready is not dialed"
    );
    assert!(https.calls().is_empty());
    assert_eq!(
        lane.report().target.map(|t| t.id),
        Some(String::from("bb")),
        "the node that answered"
    );
    bucket.put("b.txt", b"b").unwrap();
    assert_eq!(
        dialer.ids(),
        vec!["aa", "bb"],
        "the node without a route rests; the one that answered is kept"
    );
}

#[test]
fn a_node_list_without_iroh_ids_never_dials() {
    let s3 = FakeS3::new();
    let https = through(&s3);
    let dialer = Dialer::to(&[]);
    let nodes = vec![iroh_node("n1", None, &[], true)];
    let (bucket, lane) = lane_bucket(&https, &nodes, &dialer, &clock());
    bucket.put("a.txt", b"a").unwrap();
    assert_eq!(methods(&https), vec![Method::Put]);
    assert!(dialer.ids().is_empty());
    assert_eq!(lane.lane_now(&nodes), Lane::Https);
}

#[test]
fn a_node_that_answers_busy_over_iroh_hands_the_request_on_to_https() {
    let s3 = FakeS3::new();
    let https = through(&s3);
    let service = s3.clone();
    let busy = Fake::new(move |call, _| {
        if call.method == Method::Put {
            return Ok(HttpReply {
                status: 503,
                headers: vec![(String::from("x-azlin-error"), String::from("unavailable"))],
                body: b"<Error><Code>ServiceUnavailable</Code><Message>draining</Message></Error>"
                    .to_vec(),
            });
        }
        Ok(service.answer(call))
    });
    let dialer = Dialer::to(&[("aa", &busy)]);
    let nodes = vec![iroh_node("n1", Some("aa"), &["192.0.2.1:7001"], true)];
    let (bucket, _lane) = lane_bucket(&https, &nodes, &dialer, &clock());
    bucket.put("a.txt", b"a").unwrap();
    assert_eq!(s3.read("a.txt").unwrap(), b"a");
    assert_eq!(methods(&https), vec![Method::Put]);
}

#[test]
fn the_nodes_to_dial_are_every_ready_node_with_an_iroh_id_at_all_its_sockets() {
    let nodes = Node::list(&[
        serde_json::json!({"name": "n1", "url": "http://n1.test", "ready": false,
                           "iroh_id": "aa"}),
        serde_json::json!({"name": "n2", "url": "http://n2.test", "ready": true}),
        serde_json::json!({"name": "n3", "ready": true, "sign_pubkey": "cc",
                           "iroh_addrs": ["[::1]:4433", "127.0.0.1:4433"]}),
        serde_json::json!({"name": "n4", "url": "http://n4.test", "ready": true,
                           "iroh_id": "dd", "iroh_addrs": ["192.0.2.4:7001"]}),
    ]);
    let targets = targets_from_nodes(&nodes);
    let ids: Vec<&str> = targets.iter().map(|t| t.id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["cc", "dd"],
        "not n1 (not ready), not n2 (no iroh id)"
    );
    assert_eq!(targets[0].addrs, vec!["[::1]:4433", "127.0.0.1:4433"]);
    assert!(targets[0].source.contains("n3"));
    assert_eq!(targets[1].addrs, vec!["192.0.2.4:7001"]);
}

/// The token server answers the refresh with a node list whose node names its iroh id; the
/// block endpoint gives no answer.
fn cloud_with_an_iroh_node() -> Arc<Fake> {
    Fake::new(move |call, _| {
        if call.url.starts_with(super::TOKEN) {
            return Ok(super::json(
                200,
                &super::bundle("AKID2", "2026-10-09T09:15:00Z", "dt_f.1.bbb").replace(
                    r#""nodes": []"#,
                    r#""nodes": [{"name": "n1", "url": "http://n1.nodes.test:19001",
                                  "ready": true, "iroh_id": "aa",
                                  "iroh_addrs": ["192.0.2.1:7001"]}]"#,
                ),
            ));
        }
        Err(String::from("connection refused"))
    })
}

#[test]
fn an_azlin_drive_with_an_iroh_dialer_goes_over_iroh_to_the_nodes_its_refresh_names() {
    use azul_storage::{
        config::keyring_key,
        keyring::{KeyringStore, MemoryKeyring},
        testing::TempDir,
        Drive, ListRequest,
    };

    use crate::{
        drive::AzlinDrive, lock::LockDir, shared::SharedKeyring, AzlinSession, DriveBundle,
    };

    let dir = TempDir::new("azcloud-lane");
    let cloud = cloud_with_an_iroh_node();
    let node = Fake::new(|_, _| Ok(super::empty_listing()));
    let dialer = Dialer::to(&[("aa", &node)]);
    let first = DriveBundle::parse(&super::bundle(
        "AKID1",
        "2026-10-08T21:15:00Z",
        "dt_f.0.aaa",
    ))
    .unwrap();
    let keyring = Arc::new(MemoryKeyring::new());
    keyring
        .set(&keyring_key("d_1"), &first.session().to_keyring_secret())
        .unwrap();
    let drive = AzlinDrive::new(
        &first.entry_named("Cloud", super::TOKEN),
        first.session(),
        super::TOKEN,
        SharedKeyring::new(keyring, LockDir::new(dir.path())),
        factory(&cloud),
        Box::new(|_: &AzlinSession, _: Result<(), String>| {}),
    )
    .unwrap()
    // 2026-10-08T21:05:00Z: the credentials run out, the refresh lists the node.
    .with_clock(|| 1_791_493_500)
    .with_iroh(dialer.clone(), None);
    drive.list(&ListRequest::folder("")).unwrap();
    assert_eq!(dialer.ids(), vec!["aa"]);
    assert_eq!(
        methods(&node),
        vec![Method::Head, Method::Get],
        "a probe, then the listing"
    );
    assert!(
        cloud
            .calls()
            .iter()
            .all(|c| c.url.starts_with(super::TOKEN)),
        "only the refresh went over https"
    );
}
