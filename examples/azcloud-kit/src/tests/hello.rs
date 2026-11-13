//! Every request to the token server and the nodes says which app and contract sent it.

use std::sync::Arc;

use azlin_contract::{ClientHello, CLIENT_HEADER, CONTRACT_VERSION};
use azul_storage::{Credentials, S3Config, Transport};

use super::{fake_s3::FakeS3, header, json, Fake, Shared, S3, TOKEN};
use crate::{bucket::Bucket, drive::TransportFactory, hello, TokenServer};

/// The hello this process sends (the kit's own unless a test announced one).
fn sent() -> String {
    hello::current().expect("the kit has a hello").header_value()
}

#[test]
fn a_token_server_call_carries_the_hello() {
    let fake = Fake::new(|_, _| Ok(json(200, r#"{"tiers": [], "methods": []}"#)));
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let _ = server.tiers();
    let calls = fake.calls();
    assert_eq!(calls.len(), 1);
    let value = header(&calls[0], CLIENT_HEADER).expect("the hello is sent");
    assert_eq!(value, sent());
    let read = ClientHello::parse(value).expect("it reads as a hello");
    assert_eq!(read.contract, CONTRACT_VERSION);
    let named = calls[0]
        .headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case(CLIENT_HEADER))
        .count();
    assert_eq!(named, 1, "once");
}

#[test]
fn a_nodes_call_and_the_bucket_probe_carry_the_hello() {
    let s3 = FakeS3::new();
    s3.write("a.txt", b"alpha".to_vec());
    let service = s3.clone();
    let fake = Fake::new(move |call, _| Ok(service.answer(call)));
    let shared = fake.clone();
    let transports: TransportFactory =
        Arc::new(move || Box::new(Shared(shared.clone())) as Box<dyn Transport>);
    let config = S3Config {
        endpoint: S3.to_string(),
        region: String::from("us-east-1"),
        bucket: String::from("d-1"),
        path_style: true,
    };
    let bucket = Bucket::new(config, Credentials::new("AKID1", "secret-of-AKID1"), transports)
        .unwrap();
    assert_eq!(bucket.get("a.txt").unwrap().unwrap(), b"alpha");
    let _ = bucket.probe("probe-key-that-is-not-there");
    let calls = fake.calls();
    assert!(calls.len() >= 2, "the read and the probe");
    for call in &calls {
        assert_eq!(header(call, CLIENT_HEADER), Some(sent().as_str()), "{}", call.url);
    }
}

#[test]
fn the_kits_own_hello_names_the_kit_and_an_app_names_itself() {
    let kit = hello::of_app("azcloud-kit", env!("CARGO_PKG_VERSION")).expect("the kit's hello");
    assert_eq!(kit.app.as_str(), "azcloud-kit");
    assert_eq!(kit.contract, CONTRACT_VERSION);
    let app = hello::of_app("AzDrive", "0.3.0").expect("an app's hello");
    assert_eq!(app.header_value(), format!("azdrive/0.3.0; contract={CONTRACT_VERSION}"));
    assert!(hello::of_app("Az Drive", "0.3.0").is_none());
    assert!(hello::of_app("azdrive", "0.3").is_none());
}

#[test]
fn stamping_twice_leaves_one_hello() {
    let mut headers = vec![
        (String::from("X-Azlin-Client"), String::from("stale/0.0.1; contract=1")),
        (String::from("accept"), String::from("application/json")),
    ];
    hello::stamp_headers(&mut headers);
    hello::stamp_headers(&mut headers);
    let hellos: Vec<&str> = headers
        .iter()
        .filter(|(name, _)| name.eq_ignore_ascii_case(CLIENT_HEADER))
        .map(|(_, value)| value.as_str())
        .collect();
    assert_eq!(hellos, vec![sent().as_str()]);
    assert!(headers.iter().any(|(name, _)| name == "accept"));
}
