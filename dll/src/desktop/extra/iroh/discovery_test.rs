use std::{
    net::SocketAddr,
    sync::{atomic::AtomicU64, atomic::Ordering, Arc, Mutex},
};

use azul_css::{AzString, StringVec};
use iroh::{address_lookup::UserData, SecretKey};
use n0_future::StreamExt;

use super::*;
use crate::desktop::extra::iroh::types::IrohRelayMode;

fn config() -> IrohConfig {
    IrohConfig::create(AzString::from_const_str("azul/iroh-test/1"))
}

fn id(seed: u8) -> EndpointId {
    SecretKey::from_bytes(&[seed; 32]).public()
}

fn ip(text: &str) -> SocketAddr {
    text.parse().expect("a socket address")
}

fn relay() -> TransportAddr {
    TransportAddr::Relay("https://relay.example".parse().expect("a relay url"))
}

fn item_with(addrs: Vec<TransportAddr>) -> Item {
    let data = EndpointData::new(addrs).with_user_data("user data".parse::<UserData>().unwrap());
    Item::new(EndpointInfo::from_parts(id(7), data), "mdns", Some(11))
}

/// A lookup that answers every question with one fixed item, and keeps what it is told to publish.
#[derive(Debug)]
struct Fixed {
    item: Item,
    published: Mutex<Vec<EndpointData>>,
}

impl AddressLookup for Fixed {
    fn publish(&self, data: &EndpointData) {
        self.published.lock().unwrap().push(data.clone());
    }

    fn resolve(&self, _endpoint_id: EndpointId) -> Option<BoxStream<Result<Item, LookupError>>> {
        Some(Box::pin(n0_future::stream::iter([Ok(self.item.clone())])))
    }
}

#[test]
fn a_config_asks_for_discovery_unless_it_is_switched_off_or_has_no_udp_socket() {
    assert_eq!(wanted(&config()), Wanted::LookAndAnnounce);
    assert_eq!(wanted(&config().with_local_discovery(false)), Wanted::Off);
    // relay_only binds no UDP socket: nothing on the local network to find or to announce.
    let relay_only = config()
        .with_relay_url(AzString::from_const_str("http://127.0.0.1:9"))
        .with_relay_only(true);
    assert_eq!(wanted(&relay_only), Wanted::Off);
}

#[test]
fn an_endpoint_that_accepts_nobody_looks_and_does_not_announce_itself() {
    let nobody = config().with_allowed_peers(StringVec::from_vec(Vec::new()));
    assert_eq!(wanted(&nobody), Wanted::Look);
    let somebody = config().with_allowed_peers(StringVec::from_vec(vec![AzString::from(
        id(3).to_string(),
    )]));
    assert_eq!(wanted(&somebody), Wanted::LookAndAnnounce);
    // off stays off whatever the list says
    assert_eq!(wanted(&nobody.with_local_discovery(false)), Wanted::Off);
    // a relay mode of its own does not change the question
    assert_eq!(
        wanted(&config().with_relay_mode(IrohRelayMode::Disabled)),
        Wanted::LookAndAnnounce
    );
}

#[test]
fn only_an_address_a_dial_can_use_is_believed() {
    for good in [
        "192.168.1.8:4000",
        "10.0.0.2:1",
        "169.254.3.4:9",
        "203.0.113.9:443",
        "[2001:db8::1]:5",
        "[fd00::1]:7",
        "[::ffff:192.168.0.5]:8",
    ] {
        assert!(usable_peer_addr(&ip(good)), "{good} should be usable");
    }
    for bad in [
        "192.168.1.8:0",
        "0.0.0.0:5",
        "127.0.0.1:5",
        "127.255.255.254:5",
        "224.0.0.251:5353",
        "239.255.255.250:1900",
        "255.255.255.255:9",
        "[::]:5",
        "[::1]:5",
        "[ff02::fb]:5353",
        "[fe80::1]:5",
        "[febf::1]:5",
        "[::ffff:127.0.0.1]:5",
        "[::ffff:0.0.0.0]:5",
        "[::ffff:224.0.0.251]:5353",
    ] {
        assert!(!usable_peer_addr(&ip(bad)), "{bad} should not be usable");
    }
}

#[test]
fn the_local_link_is_told_the_addresses_a_neighbour_can_use_and_never_the_relay() {
    let addrs = vec![
        TransportAddr::Ip(ip("192.168.1.8:4000")),
        TransportAddr::Ip(ip("169.254.9.9:4000")),
        TransportAddr::Ip(ip("[2001:db8::8]:4001")),
        // what a probe of the router reported: a neighbour cannot use it
        TransportAddr::Ip(ip("203.0.113.9:4000")),
        TransportAddr::Ip(ip("127.0.0.1:4000")),
        TransportAddr::Ip(ip("[::1]:4001")),
        TransportAddr::Ip(ip("[fe80::1]:4001")),
        TransportAddr::Ip(ip("0.0.0.0:4000")),
        relay(),
    ];
    let told = announce_filter().apply(&addrs);
    assert_eq!(
        told.as_slice(),
        &[
            TransportAddr::Ip(ip("192.168.1.8:4000")),
            TransportAddr::Ip(ip("169.254.9.9:4000")),
            TransportAddr::Ip(ip("[2001:db8::8]:4001")),
        ]
    );
}

#[test]
fn an_announcement_is_cut_down_before_a_dial_sees_it() {
    let mut addrs = vec![
        TransportAddr::Ip(ip("192.168.1.8:4000")),
        TransportAddr::Ip(ip("127.0.0.1:4000")),
        TransportAddr::Ip(ip("10.0.0.2:0")),
        TransportAddr::Ip(ip("224.0.0.251:5353")),
        TransportAddr::Ip(ip("[fe80::1]:4001")),
        TransportAddr::Ip(ip("[2001:db8::1]:4001")),
        relay(),
    ];
    let ignored = AtomicU64::new(0);
    let item = sanitised(item_with(addrs.clone()), &ignored);
    let kept: Vec<SocketAddr> = item.ip_addrs().copied().collect();
    assert_eq!(kept, vec![ip("192.168.1.8:4000"), ip("[2001:db8::1]:4001")]);
    assert_eq!(ignored.load(Ordering::Relaxed), 4, "the four unusable addresses are counted");
    assert_eq!(item.relay_urls().count(), 0, "a neighbour does not choose the relay of a dial");
    assert!(item.user_data().is_none(), "nor does it hand the dial user data");
    assert_eq!(item.endpoint_id(), id(7));
    assert_eq!(item.provenance(), "mdns");
    assert_eq!(item.last_updated(), Some(11));

    // a flood: ten thousand usable addresses are cut to the limit and counted
    addrs.clear();
    addrs.extend((0..10_000u32).map(|n| {
        TransportAddr::Ip(SocketAddr::from(([10, (n >> 8) as u8, n as u8, 1], 1000 + (n % 5000) as u16)))
    }));
    let ignored = AtomicU64::new(0);
    let item = sanitised(item_with(addrs), &ignored);
    assert_eq!(item.ip_addrs().count(), MAX_ADDRESSES_PER_PEER);
    assert_eq!(
        ignored.load(Ordering::Relaxed),
        10_000 - MAX_ADDRESSES_PER_PEER as u64
    );

    // nothing usable: an item without addresses, not a panic
    let ignored = AtomicU64::new(0);
    let item = sanitised(item_with(vec![TransportAddr::Ip(ip("127.0.0.1:1"))]), &ignored);
    assert_eq!(item.ip_addrs().count(), 0);
    assert_eq!(ignored.load(Ordering::Relaxed), 1);
}

#[test]
fn the_lookup_hands_the_endpoint_what_its_inner_lookup_found_cut_down() {
    let item = item_with(vec![
        TransportAddr::Ip(ip("192.168.1.8:4000")),
        TransportAddr::Ip(ip("127.0.0.1:4000")),
        relay(),
    ]);
    let ignored = Arc::new(AtomicU64::new(0));
    let lookup = LocalLookup::new(
        Fixed {
            item,
            published: Mutex::default(),
        },
        ignored.clone(),
    );

    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("a runtime");
    let found = runtime.block_on(async {
        let mut stream = lookup.resolve(id(7)).expect("the inner lookup answers");
        let first = stream.next().await.expect("one item").expect("not an error");
        assert!(stream.next().await.is_none(), "the stream ends with the inner one");
        first
    });
    assert_eq!(
        found.ip_addrs().copied().collect::<Vec<_>>(),
        vec![ip("192.168.1.8:4000")]
    );
    assert_eq!(found.relay_urls().count(), 0);
    assert_eq!(ignored.load(Ordering::Relaxed), 1);

    // what the endpoint publishes goes through to the inner lookup unchanged
    let data = EndpointData::new(vec![TransportAddr::Ip(ip("192.168.1.8:4000")), relay()]);
    lookup.publish(&data);
    assert_eq!(lookup.inner.published.lock().unwrap().as_slice(), &[data]);
}

#[test]
fn the_status_says_what_discovery_is_doing() {
    assert_eq!(LocalDiscoveryState::Off.status(0), "off");
    assert_eq!(LocalDiscoveryState::On { announcing: true }.status(0), "on");
    assert_eq!(
        LocalDiscoveryState::On { announcing: true }.status(3),
        "on; 3 announced addresses were not used"
    );
    let looking = LocalDiscoveryState::On { announcing: false }.status(0);
    assert!(looking.starts_with("on, looking only"), "{looking}");
    assert_eq!(
        LocalDiscoveryState::Unavailable("no multicast".to_string()).status(9),
        "unavailable: no multicast"
    );
}
