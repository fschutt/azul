//! The system's Bonjour lookup: what its discovery says it does, and (by hand, on a Mac) that it finds
//! what the mDNS lookup announces. The parsing and assembling it relies on is `dns_wire_test.rs`.

use azul_css::AzString;

use super::*;
use crate::desktop::extra::iroh::{
    engine::Engine,
    types::{IrohConfig, IrohRelayMode},
};

fn config() -> IrohConfig {
    IrohConfig::create(AzString::from_const_str("azul/iroh-bonjour-test/1"))
        .with_relay_mode(IrohRelayMode::Disabled)
}

#[test]
fn an_iphone_looks_for_the_mac_and_announces_nothing() {
    assert!(!BonjourDiscovery.can_announce());
    let engine = Engine::bind_with(&config(), &BonjourDiscovery).expect("the endpoint binds");
    let status = engine.local_discovery_status();
    assert!(status.starts_with("on, looking only"), "{status}");
}

/// BY HAND, on a Mac (the engine announces through mDNS there): the lookup the iPhone uses finds
/// the endpoint by its id through the system's Bonjour, and the addresses are the ones the
/// endpoint is bound to. This is what proves that what swarm-discovery announces is what Apple's
/// browse and resolve calls read. The first run shows the Local Network prompt for the terminal.
#[cfg(target_os = "macos")]
#[test]
#[ignore = "X-EDGE-C-2: needs a Mac on a network that carries multicast and the Local Network permission: run by hand with cargo test -p azul-dll --lib --features iroh -- --ignored --nocapture bonjour_finds_what_the_mdns_lookup_announces"]
fn bonjour_finds_what_the_mdns_lookup_announces() {
    use n0_future::StreamExt;

    let host = Engine::bind(&config()).expect("the host binds");
    eprintln!("host local discovery: {}", host.local_discovery_status());
    assert!(host.local_discovery_status().starts_with("on"));
    let host_id = host.endpoint_id();
    let id: EndpointId = host_id.parse().expect("an endpoint id");

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("a runtime");
    let started = Instant::now();
    let item = runtime
        .block_on(async {
            let mut stream = BonjourLookup.resolve(id).expect("a worker thread starts");
            tokio::time::timeout(Duration::from_secs(40), stream.next()).await
        })
        .expect("Bonjour answered within 40 s")
        .expect("the stream gave an item")
        .expect("the item is not an error");
    eprintln!("found in {:?}: {:?}", started.elapsed(), item.ip_addrs().collect::<Vec<_>>());
    assert_eq!(item.endpoint_id(), id);
    let ports: Vec<u16> = host.bound_sockets().iter().map(|s| s.port()).collect();
    assert!(
        item.ip_addrs().any(|addr| ports.contains(&addr.port())),
        "no found address has one of the host's ports {ports:?}: {:?}",
        item.ip_addrs().collect::<Vec<_>>()
    );
}
