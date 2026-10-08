//! Which transport a drive takes: the decision before any request, the node to dial.

use serde_json::json;

use crate::transport::{
    decide, target_from_nodes, Decision, IrohTarget, Lane, TransportMemory, TransportPref,
    IROH_RETRY_SECS,
};

fn target() -> IrohTarget {
    IrohTarget {
        id: "ab".repeat(32),
        addr: Some(String::from("127.0.0.1:41000")),
        source: String::from("environment AZLIN_IROH_NODE"),
    }
}

#[test]
fn https_is_taken_when_asked_for_without_iroh_or_without_a_node_to_dial() {
    let t = target();
    assert!(matches!(
        decide(TransportPref::Https, true, Some(&t), None, 0),
        Ok(Decision::Https(_))
    ));
    assert!(matches!(
        decide(TransportPref::Auto, false, Some(&t), None, 0),
        Ok(Decision::Https(_))
    ));
    match decide(TransportPref::Auto, true, None, None, 0) {
        Ok(Decision::Https(reason)) => assert!(reason.contains("iroh id"), "{reason}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn auto_probes_iroh_and_after_a_failure_keeps_to_https_for_five_minutes() {
    let t = target();
    assert_eq!(
        decide(TransportPref::Auto, true, Some(&t), None, 1000),
        Ok(Decision::Probe)
    );
    let failed = TransportMemory {
        lane: Lane::Https,
        reason: String::from("iroh failed"),
        decided_at: 1000,
        iroh_failed_at: Some(1000),
        iroh_error: Some(String::from("no answer within 10 s")),
    };
    match decide(TransportPref::Auto, true, Some(&t), Some(&failed), 1100) {
        Ok(Decision::Https(reason)) => {
            assert!(reason.contains("no answer within 10 s"), "{reason}")
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        decide(
            TransportPref::Auto,
            true,
            Some(&t),
            Some(&failed),
            1000 + IROH_RETRY_SECS
        ),
        Ok(Decision::Probe),
        "five minutes later iroh is tried again"
    );
    let worked = TransportMemory {
        lane: Lane::Iroh,
        iroh_failed_at: None,
        iroh_error: None,
        ..failed
    };
    assert_eq!(
        decide(TransportPref::Auto, true, Some(&t), Some(&worked), 1100),
        Ok(Decision::Probe)
    );
}

#[test]
fn iroh_only_is_an_error_when_it_cannot_be_had_and_never_a_silent_https() {
    let t = target();
    assert!(decide(TransportPref::Iroh, false, Some(&t), None, 0).is_err());
    assert!(decide(TransportPref::Iroh, true, None, None, 0).is_err());
    let failed = TransportMemory {
        lane: Lane::Https,
        reason: String::new(),
        decided_at: 0,
        iroh_failed_at: Some(0),
        iroh_error: None,
    };
    assert_eq!(
        decide(TransportPref::Iroh, true, Some(&t), Some(&failed), 1),
        Ok(Decision::Probe),
        "the memory of a failure does not stop an iroh-only run"
    );
}

#[test]
fn a_node_list_that_names_iroh_ids_gives_the_first_ready_node_with_its_ipv4_socket() {
    let nodes = vec![
        json!({"name": "n1", "url": "http://127.0.0.1:9001", "ready": false, "iroh_id": "aa"}),
        json!({"name": "n2", "url": "http://127.0.0.1:9002", "ready": true}),
        json!({"name": "n3", "ready": true, "sign_pubkey": "cc",
               "iroh_addrs": ["[::1]:4433", "127.0.0.1:4433"]}),
    ];
    let t = target_from_nodes(&nodes).expect("n3");
    assert_eq!(t.id, "cc");
    assert_eq!(t.addr.as_deref(), Some("127.0.0.1:4433"));
    assert!(t.source.contains("n3"));
    assert_eq!(
        target_from_nodes(&[json!({"name": "n1", "url": "http://x"})]),
        None,
        "a node list without iroh ids names no node to dial"
    );
}

#[test]
fn the_transport_names_parse_in_any_case() {
    assert_eq!(TransportPref::parse(" HTTPS "), Some(TransportPref::Https));
    assert_eq!(TransportPref::parse("http"), Some(TransportPref::Https));
    assert_eq!(TransportPref::parse("quic"), Some(TransportPref::Iroh));
    assert_eq!(TransportPref::parse("auto"), Some(TransportPref::Auto));
    assert_eq!(TransportPref::parse("tcp"), None);
    assert_eq!(Lane::Iroh.name(), "iroh");
    assert_eq!(
        serde_json::to_value(Lane::Https).unwrap(),
        json!(Lane::Https.name())
    );
}
