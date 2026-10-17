//! A drive over the transport a run chose: HTTPS when asked, iroh when the dialed node answers
//! the probe, the fallback during a run, and the memory of a failure.

use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use azul_appkit::azlin_config::AzlinConfig;
use azul_storage::{testing::TempDir, HttpReply, Method, Transport};

use super::{bundle, fake_s3::FakeS3, Fake, Shared, TOKEN};
use crate::{
    account::{read_grant, store_grant, Account},
    drive::TransportFactory,
    settings::{Flags, OsDirs, Settings},
    state::{read_json, StateDir},
    transport::{CloudDrive, IrohDialer, IrohTarget, Lane, TransportMemory},
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

/// A device holding the drive of the tests' bundle (on the S3 service in memory), its HTTPS
/// requests sent through `https`.
fn device(https: &Arc<Fake>) -> (TempDir, Account) {
    let dir = TempDir::new("azcloud-lanes");
    let state = StateDir::open(dir.path()).unwrap();
    let answer: serde_json::Value =
        serde_json::from_str(&bundle("AKID1", "2026-10-08T21:15:00Z", "dt_f.0.aaa")).unwrap();
    store_grant(&state, &read_grant(&answer, TOKEN, "owner", 0).unwrap()).unwrap();
    let account = Account::open(&state, TOKEN, factory(https), None).unwrap();
    (dir, account)
}

fn no_file(_: &Path) -> (AzlinConfig, Option<String>) {
    (AzlinConfig::default(), None)
}

/// The settings of a run that asks for `transport`, with a pinned iroh node when `node`.
fn settings(transport: &str, node: bool) -> Settings {
    let flags = Flags {
        transport: Some(transport.to_string()),
        iroh_node: node.then(|| "ab".repeat(32)),
        iroh_addr: node.then(|| String::from("127.0.0.1:41000")),
        ..Flags::default()
    };
    let env = |name: &str| (name == "AZLIN_CONFIG").then(|| String::from("off"));
    Settings::resolve(&flags, &env, &OsDirs::default(), &no_file)
}

/// Dials by handing out its transport (a node over iroh), or fails.
struct Dialer {
    node: Option<Arc<Fake>>,
    dialed: Mutex<Vec<IrohTarget>>,
}

impl Dialer {
    fn to(node: &Arc<Fake>) -> Arc<Dialer> {
        Arc::new(Dialer {
            node: Some(node.clone()),
            dialed: Mutex::new(Vec::new()),
        })
    }

    fn failing() -> Arc<Dialer> {
        Arc::new(Dialer {
            node: None,
            dialed: Mutex::new(Vec::new()),
        })
    }
}

impl IrohDialer for Dialer {
    fn dial(
        &self,
        target: &IrohTarget,
        _relay: Option<&str>,
    ) -> Result<Box<dyn Transport>, String> {
        self.dialed.lock().unwrap().push(target.clone());
        match &self.node {
            Some(node) => Ok(Box::new(Shared(node.clone()))),
            None => Err(String::from("no route to the node")),
        }
    }
}

#[test]
fn https_asked_for_never_dials_and_every_request_goes_over_https() {
    let s3 = FakeS3::new();
    let (https, node) = (through(&s3), through(&s3));
    let dialer = Dialer::to(&node);
    let (_dir, account) = device(&https);
    let drive = CloudDrive::open(&account, &settings("https", true), Some(dialer.clone())).unwrap();
    assert_eq!(drive.lane(), Lane::Https);
    assert!(!drive.transport().probed);
    drive.put("a.txt", b"a").unwrap();
    assert_eq!(s3.read("a.txt").unwrap(), b"a");
    assert_eq!(https.calls().len(), 1);
    assert!(node.calls().is_empty());
    assert!(dialer.dialed.lock().unwrap().is_empty());
}

#[test]
fn auto_takes_iroh_when_the_node_answers_the_probe_and_remembers_it() {
    let s3 = FakeS3::new();
    let (https, node) = (through(&s3), through(&s3));
    let dialer = Dialer::to(&node);
    let (_dir, account) = device(&https);
    let drive = CloudDrive::open(&account, &settings("auto", true), Some(dialer.clone())).unwrap();
    assert_eq!(drive.lane(), Lane::Iroh);
    let report = drive.transport();
    assert!(
        report.probed && report.reason.contains("iroh answered"),
        "{report:?}"
    );
    assert_eq!(dialer.dialed.lock().unwrap()[0].id, "ab".repeat(32));
    drive.put("a.txt", b"a").unwrap();
    assert_eq!(s3.read("a.txt").unwrap(), b"a");
    assert!(https.calls().is_empty(), "nothing over https");
    assert!(node.calls().iter().any(|c| c.method == Method::Put));
    let memory: TransportMemory = read_json(&account.state().transport_file())
        .unwrap()
        .expect("transport.json");
    assert_eq!(memory.lane, Lane::Iroh);
    assert_eq!(memory.iroh_failed_at, None);
}

#[test]
fn a_node_that_does_not_answer_means_https_and_the_next_run_does_not_probe_again_soon() {
    let s3 = FakeS3::new();
    let https = through(&s3);
    let dead = Fake::new(|_, _| Err(String::from("connection refused")));
    let dialer = Dialer::to(&dead);
    let (_dir, account) = device(&https);
    let drive = CloudDrive::open(&account, &settings("auto", true), Some(dialer.clone())).unwrap();
    assert_eq!(drive.lane(), Lane::Https);
    let report = drive.transport();
    assert!(
        report.probed && report.reason.contains("iroh failed"),
        "{report:?}"
    );
    drive.close();
    let again = CloudDrive::open(&account, &settings("auto", true), Some(dialer.clone())).unwrap();
    assert_eq!(again.lane(), Lane::Https);
    assert!(!again.transport().probed, "the failure was remembered");
    assert_eq!(dead.calls().len(), 1, "one probe in all");
}

#[test]
fn iroh_only_fails_instead_of_passing_over_https() {
    let s3 = FakeS3::new();
    let https = through(&s3);
    let (_dir, account) = device(&https);
    let e = CloudDrive::open(&account, &settings("iroh", true), Some(Dialer::failing()))
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("iroh was asked for and failed") && e.contains("no route"),
        "{e}"
    );
    let e = CloudDrive::open(&account, &settings("iroh", true), None)
        .unwrap_err()
        .to_string();
    assert!(e.contains("without the iroh feature"), "{e}");
    let e = CloudDrive::open(&account, &settings("iroh", false), Some(Dialer::failing()))
        .unwrap_err()
        .to_string();
    assert!(e.contains("iroh id"), "{e}");
    assert!(https.calls().is_empty());
}

#[test]
fn a_request_that_fails_over_iroh_later_falls_back_to_https_for_the_rest_of_the_run() {
    let s3 = FakeS3::new();
    let https = through(&s3);
    let service = s3.clone();
    // The node answers the probe, then its streams break.
    let flaky = Fake::new(move |call, n| {
        if n == 0 {
            Ok(service.answer(call))
        } else {
            Err(String::from("the stream was reset"))
        }
    });
    let dialer = Dialer::to(&flaky);
    let (_dir, account) = device(&https);
    let drive = CloudDrive::open(&account, &settings("auto", true), Some(dialer.clone())).unwrap();
    assert_eq!(drive.lane(), Lane::Iroh);
    drive.put("b.txt", b"b").unwrap();
    assert_eq!(drive.lane(), Lane::Https);
    assert_eq!(s3.read("b.txt").unwrap(), b"b");
    let report = drive.transport();
    assert!(
        report
            .fell_back
            .as_deref()
            .is_some_and(|why| why.contains("put failed over iroh")),
        "{report:?}"
    );
    drive.put("c.txt", b"c").unwrap();
    assert_eq!(https.calls().len(), 2, "the rest of the run is https");
    let memory: TransportMemory = read_json(&account.state().transport_file())
        .unwrap()
        .expect("transport.json");
    assert!(memory.iroh_failed_at.is_some());
}

#[test]
fn an_error_iroh_carried_fine_is_the_services_and_is_not_repeated_over_https() {
    let s3 = FakeS3::new();
    let https = through(&s3);
    let service = s3.clone();
    let node = Fake::new(move |call, _| {
        if call.method == Method::Get && call.url.ends_with("/secret.txt") {
            return Ok(HttpReply {
                status: 403,
                headers: Vec::new(),
                body: b"<Error><Code>AccessDenied</Code><Message>no</Message></Error>".to_vec(),
            });
        }
        Ok(service.answer(call))
    });
    let dialer = Dialer::to(&node);
    let (_dir, account) = device(&https);
    let drive = CloudDrive::open(&account, &settings("auto", true), Some(dialer.clone())).unwrap();
    assert!(drive.get("secret.txt").is_err());
    assert_eq!(drive.lane(), Lane::Iroh, "iroh still works");
    assert!(https.calls().is_empty(), "https would only repeat the answer");
}
