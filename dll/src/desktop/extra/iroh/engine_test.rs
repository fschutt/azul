//! The engine's local-network discovery: what the config chooses, what a refused start leaves, a
//! dial by endpoint id alone through a lookup that knows the address, and (by hand) the real mDNS.

use std::{
    sync::{atomic::AtomicU64, Arc, Mutex},
    time::{Duration, Instant},
};

use azul_css::{AzString, StringVec};
use iroh::{
    address_lookup::{AddressLookup, EndpointData, EndpointInfo, MemoryLookup, UserData},
    Endpoint, EndpointAddr, SecretKey, TransportAddr,
};

use super::*;

const ALPN: &str = "azul/iroh-discovery-test/1";

/// A config without relays, so nothing leaves this computer; discovery as the default has it.
fn config() -> IrohConfig {
    IrohConfig::create(AzString::from_const_str(ALPN)).with_relay_mode(IrohRelayMode::Disabled)
}

fn quiet() -> IrohConfig {
    config().with_local_discovery(false)
}

/// Records what the engine asked of discovery: one entry per start, true when it was to announce.
#[derive(Default)]
struct Recording {
    started: Mutex<Vec<bool>>,
}

impl LocalDiscovery for Recording {
    fn start(
        &self,
        _endpoint: &Endpoint,
        announce: bool,
        _ignored: &Arc<AtomicU64>,
        _nearby: &Arc<Nearby>,
    ) -> Result<Option<Held>, String> {
        self.started.lock().unwrap().push(announce);
        Ok(None)
    }
}

/// A network that refuses multicast.
struct Refusing;

impl LocalDiscovery for Refusing {
    fn start(
        &self,
        _endpoint: &Endpoint,
        _announce: bool,
        _ignored: &Arc<AtomicU64>,
        _nearby: &Arc<Nearby>,
    ) -> Result<Option<Held>, String> {
        Err("no multicast on this network".to_string())
    }
}

/// A lookup that knows where one peer is: what mDNS would find, without a network.
struct Knows(EndpointAddr);

impl LocalDiscovery for Knows {
    fn start(
        &self,
        endpoint: &Endpoint,
        _announce: bool,
        _ignored: &Arc<AtomicU64>,
        _nearby: &Arc<Nearby>,
    ) -> Result<Option<Held>, String> {
        let lookup = MemoryLookup::new();
        lookup.add_endpoint_info(self.0.clone());
        endpoint
            .address_lookup()
            .map_err(|e| e.to_string())?
            .add(lookup);
        Ok(None)
    }
}

fn bound(config: &IrohConfig, discovery: &dyn LocalDiscovery) -> Engine {
    Engine::bind_with(config, discovery).expect("the endpoint binds")
}

/// The next event of `kind` within `limit`; an `Error` event on the way fails the test.
fn next_of(engine: &Engine, kind: IrohEventKind, limit: Duration) -> IrohEvent {
    let deadline = Instant::now() + limit;
    loop {
        match engine.recv() {
            Some(event) if event.kind == kind => return event,
            Some(event) => {
                assert_ne!(event.kind, IrohEventKind::Error, "{}", event.text.as_str())
            }
            None => {
                assert!(
                    Instant::now() < deadline,
                    "no {kind:?} event within {limit:?}"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    }
}

const WAIT: Duration = Duration::from_secs(20);

#[test]
fn the_config_chooses_whether_discovery_starts_and_whether_it_announces() {
    // on by default: started once, to announce
    let recording = Recording::default();
    let engine = bound(&config(), &recording);
    assert_eq!(*recording.started.lock().unwrap(), vec![true]);
    assert_eq!(engine.local_discovery_status(), "on");

    // switched off: never started
    let recording = Recording::default();
    let engine = bound(&quiet(), &recording);
    assert!(recording.started.lock().unwrap().is_empty());
    assert_eq!(engine.local_discovery_status(), "off");

    // an endpoint that accepts nobody is a client: started, not announcing
    let recording = Recording::default();
    let client = config().with_allowed_peers(StringVec::from_vec(Vec::new()));
    let engine = bound(&client, &recording);
    assert_eq!(*recording.started.lock().unwrap(), vec![false]);
    assert!(
        engine
            .local_discovery_status()
            .starts_with("on, looking only"),
        "{}",
        engine.local_discovery_status()
    );

    // no UDP socket, nothing to find: never started, even though the flag is on
    let recording = Recording::default();
    let relay_only = IrohConfig::create(AzString::from_const_str(ALPN))
        .with_relay_url(AzString::from_const_str("http://127.0.0.1:9"))
        .with_relay_only(true);
    assert!(relay_only.local_discovery);
    let engine = bound(&relay_only, &recording);
    assert!(recording.started.lock().unwrap().is_empty());
    assert_eq!(engine.local_discovery_status(), "off");
}

#[test]
fn a_network_that_refuses_discovery_leaves_the_endpoint_bound_and_says_why() {
    let engine = bound(&config(), &Refusing);
    assert!(engine.is_bound());
    assert_eq!(
        engine.local_discovery_status(),
        "unavailable: no multicast on this network"
    );
    // no Error event for an app to treat as fatal: the first thing it hears is the ticket
    let ready = next_of(&engine, IrohEventKind::Ready, WAIT);
    assert!(!ready.text.as_str().is_empty());
}

#[test]
fn an_endpoint_dialled_by_its_id_alone_connects_when_a_lookup_knows_its_address() {
    let host = bound(&quiet(), &Recording::default());
    next_of(&host, IrohEventKind::Ready, WAIT);
    let host_id = host.endpoint_id();

    // the control: without a lookup a bare id names no address, and the dial fails
    let lonely = bound(&quiet(), &Recording::default());
    lonely.connect(&host_id).expect("an endpoint id parses");
    let deadline = Instant::now() + WAIT;
    let failure = loop {
        match lonely.recv() {
            Some(event) if event.kind == IrohEventKind::Error => break event,
            Some(_) => {}
            None => {
                assert!(
                    Instant::now() < deadline,
                    "the dial by id neither failed nor connected"
                );
                std::thread::sleep(Duration::from_millis(5));
            }
        }
    };
    assert!(
        failure.text.as_str().contains("could not connect"),
        "{}",
        failure.text.as_str()
    );

    // with a lookup that knows the address (what mDNS finds on a network) the same dial connects
    let guest = bound(&config(), &Knows(host.endpoint.addr()));
    assert_eq!(guest.local_discovery_status(), "on");
    guest.connect(&host_id).expect("an endpoint id parses");
    assert_eq!(
        next_of(&guest, IrohEventKind::PeerConnected, WAIT)
            .text
            .as_str(),
        host_id
    );
    assert_eq!(
        next_of(&host, IrohEventKind::PeerConnected, WAIT)
            .text
            .as_str(),
        guest.endpoint_id()
    );
}

#[test]
fn an_endpoint_that_accepted_only_listed_peers_accepts_everybody_again_once_cleared() {
    let nobody = quiet().with_allowed_peers(StringVec::from_vec(Vec::new()));
    let host = bound(&nobody, &Recording::default());
    let ticket = next_of(&host, IrohEventKind::Ready, WAIT).text;
    let stranger = bound(&quiet(), &Recording::default());

    stranger
        .connect(ticket.as_str())
        .expect("the ticket parses");
    let deadline = Instant::now() + WAIT;
    let refusal = loop {
        if let Some(event) = host.recv().filter(|e| e.kind == IrohEventKind::Error) {
            break event;
        }
        assert!(Instant::now() < deadline, "the stranger was not refused");
        std::thread::sleep(Duration::from_millis(5));
    };
    assert!(
        refusal.text.as_str().contains("not an allowed peer"),
        "{}",
        refusal.text.as_str()
    );
    assert_eq!(host.peer_count(), 0);

    host.set_allowed_peers(None).expect("clearing never fails");
    stranger
        .connect(ticket.as_str())
        .expect("the ticket parses");
    assert_eq!(
        next_of(&host, IrohEventKind::PeerConnected, WAIT)
            .text
            .as_str(),
        stranger.endpoint_id()
    );
}

// ----- the endpoints of the local network, and what an endpoint says about itself -----

fn neighbour_id(seed: u8) -> iroh::EndpointId {
    SecretKey::from_bytes(&[seed; 32]).public()
}

/// What the neighbour `seed` announces on the network: its text (if any) and one address.
fn neighbour(seed: u8, advertised: Option<&str>, address: &str) -> EndpointInfo {
    let mut data = EndpointData::new(vec![TransportAddr::Ip(
        address.parse().expect("an address"),
    )]);
    if let Some(text) = advertised {
        data = data.with_user_data(text.parse::<UserData>().expect("a text iroh holds"));
    }
    EndpointInfo::from_parts(neighbour_id(seed), data)
}

/// A network with neighbours on it: each is told to the engine's list as soon as discovery starts.
struct Neighbours(Vec<EndpointInfo>);

impl LocalDiscovery for Neighbours {
    fn start(
        &self,
        _endpoint: &Endpoint,
        _announce: bool,
        _ignored: &Arc<AtomicU64>,
        nearby: &Arc<Nearby>,
    ) -> Result<Option<Held>, String> {
        for info in &self.0 {
            nearby.seen(info);
        }
        Ok(None)
    }
}

/// A network the test changes while the engine runs: it keeps the list it was given.
#[derive(Default)]
struct Changing(Mutex<Option<Arc<Nearby>>>);

impl Changing {
    fn network(&self) -> Arc<Nearby> {
        self.0
            .lock()
            .unwrap()
            .clone()
            .expect("discovery was started")
    }
}

impl LocalDiscovery for Changing {
    fn start(
        &self,
        _endpoint: &Endpoint,
        _announce: bool,
        _ignored: &Arc<AtomicU64>,
        nearby: &Arc<Nearby>,
    ) -> Result<Option<Held>, String> {
        *self.0.lock().unwrap() = Some(nearby.clone());
        Ok(None)
    }
}

/// The next `kind` event, or `None` once the engine has no more events for the moment.
fn events_of(engine: &Engine, kind: IrohEventKind) -> Vec<IrohEvent> {
    let mut found = Vec::new();
    while let Some(event) = engine.recv() {
        if event.kind == kind {
            found.push(event);
        }
    }
    found
}

#[test]
fn the_neighbours_of_the_network_are_listed_and_reported_and_never_dialled() {
    let neighbours = Neighbours(vec![
        neighbour(31, Some("azreview/1;n=Studio Mac"), "192.168.1.31:4031"),
        neighbour(32, None, "192.168.1.32:4032"),
    ]);
    let engine = bound(&config(), &neighbours);

    let listed = engine.nearby_peers();
    assert_eq!(listed.len(), 2, "{listed:?}");
    let mac = listed
        .iter()
        .find(|peer| peer.id == neighbour_id(31))
        .expect("the Mac is listed");
    assert_eq!(mac.advertised, "azreview/1;n=Studio Mac");
    assert_eq!(mac.addresses, vec!["192.168.1.31:4031".parse().unwrap()]);

    // each is reported once, with its endpoint id and its text
    let mut told = std::collections::BTreeMap::new();
    for _ in 0..2 {
        let event = next_of(&engine, IrohEventKind::NearbyAppeared, WAIT);
        told.insert(
            event.text.as_str().to_string(),
            String::from_utf8(event.data.as_ref().to_vec()).expect("UTF-8"),
        );
    }
    assert_eq!(told.len(), 2, "{told:?}");
    assert_eq!(
        told[&neighbour_id(31).to_string()],
        "azreview/1;n=Studio Mac"
    );
    assert_eq!(told[&neighbour_id(32).to_string()], "");
    assert!(
        events_of(&engine, IrohEventKind::NearbyAppeared).is_empty(),
        "once"
    );

    // listing is not dialling
    assert_eq!(engine.peer_count(), 0);
}

#[test]
fn a_neighbour_that_stops_announcing_is_reported_gone_and_leaves_the_list() {
    let network = Changing::default();
    let engine = bound(&config(), &network);
    assert!(engine.nearby_peers().is_empty());
    assert!(events_of(&engine, IrohEventKind::NearbyAppeared).is_empty());

    network
        .network()
        .seen(&neighbour(41, Some("first"), "192.168.1.41:4041"));
    let appeared = next_of(&engine, IrohEventKind::NearbyAppeared, WAIT);
    assert_eq!(appeared.text.as_str(), neighbour_id(41).to_string());
    assert_eq!(appeared.data.as_ref(), b"first");

    // it renamed itself: reported again, with the new text
    network
        .network()
        .seen(&neighbour(41, Some("second"), "192.168.1.41:4041"));
    let renamed = next_of(&engine, IrohEventKind::NearbyAppeared, WAIT);
    assert_eq!(renamed.data.as_ref(), b"second");

    network.network().gone(&neighbour_id(41));
    let gone = next_of(&engine, IrohEventKind::NearbyGone, WAIT);
    assert_eq!(gone.text.as_str(), neighbour_id(41).to_string());
    assert!(engine.nearby_peers().is_empty());
}

/// What the endpoint publishes to a lookup of its own, as the local-network lookups are given it.
#[derive(Debug, Default)]
struct Hears {
    published: Arc<Mutex<Vec<EndpointData>>>,
}

impl AddressLookup for Hears {
    fn publish(&self, data: &EndpointData) {
        self.published.lock().unwrap().push(data.clone());
    }
}

/// Discovery that is only an address lookup that listens to what the endpoint publishes.
struct Publishing(Arc<Mutex<Vec<EndpointData>>>);

impl LocalDiscovery for Publishing {
    fn start(
        &self,
        endpoint: &Endpoint,
        _announce: bool,
        _ignored: &Arc<AtomicU64>,
        _nearby: &Arc<Nearby>,
    ) -> Result<Option<Held>, String> {
        endpoint
            .address_lookup()
            .map_err(|e| e.to_string())?
            .add(Hears {
                published: self.0.clone(),
            });
        Ok(None)
    }
}

/// The text of the newest publication, `None` when it carries none.
fn published_text(published: &Arc<Mutex<Vec<EndpointData>>>) -> Option<String> {
    published
        .lock()
        .unwrap()
        .last()
        .and_then(|data| data.user_data().map(ToString::to_string))
}

fn wait_for_text(published: &Arc<Mutex<Vec<EndpointData>>>, wanted: Option<&str>) {
    let deadline = Instant::now() + WAIT;
    while published_text(published).as_deref() != wanted {
        assert!(
            Instant::now() < deadline,
            "the endpoint publishes {:?}, not {wanted:?}",
            published_text(published)
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn the_advertised_text_is_what_the_local_lookups_are_given_and_it_can_change() {
    let published = Arc::new(Mutex::new(Vec::new()));
    let text = "azreview/1;p=ab12;n=Studio Mac";
    let config = config().with_advertised(AzString::from_const_str(text));
    let engine = bound(&config, &Publishing(published.clone()));
    wait_for_text(&published, Some(text));

    engine.set_advertised("azreview/1;p=ab12;n=Linux box");
    wait_for_text(&published, Some("azreview/1;p=ab12;n=Linux box"));

    // an empty text advertises nothing again
    engine.set_advertised("");
    wait_for_text(&published, None);

    // what is announced is cleaned, and cut at what an announcement holds, and the status says so
    engine.set_advertised("\u{202e}Studio\nMac");
    wait_for_text(&published, Some("StudioMac"));
    engine.set_advertised(&"x".repeat(300));
    wait_for_text(&published, Some(&"x".repeat(245)));
    let status = engine.local_discovery_status();
    assert!(status.contains("cut from 300 to 245 bytes"), "{status}");
    engine.set_advertised("short");
    wait_for_text(&published, Some("short"));
    let status = engine.local_discovery_status();
    assert!(!status.contains("cut from"), "{status}");
}

#[test]
fn nothing_is_advertised_unless_the_app_set_a_text() {
    let published = Arc::new(Mutex::new(Vec::new()));
    let _engine = bound(&config(), &Publishing(published.clone()));
    // the endpoint publishes its addresses (and has no text to add to them)
    let deadline = Instant::now() + WAIT;
    while published.lock().unwrap().is_empty() {
        assert!(Instant::now() < deadline, "the endpoint published nothing");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(published
        .lock()
        .unwrap()
        .iter()
        .all(|data| data.user_data().is_none()));
}

/// Garbage a hostile neighbour can multicast to the mDNS group of every computer on the link. None
/// of it may take an endpoint down, and none of it is an announcement.
fn hostile_datagrams() -> usize {
    let socket = std::net::UdpSocket::bind("0.0.0.0:0").expect("a socket");
    let mut packets: Vec<Vec<u8>> = vec![
        vec![],
        vec![0],
        vec![0xff; 11],
        vec![0xff; 12],
        vec![0; 1472],
        (0..1472u32).map(|n| (n * 7 + 3) as u8).collect(),
        // a header that claims 65535 of everything and carries nothing
        [
            &[
                0u8, 0, 0x84, 0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            ][..],
            &[0u8; 20],
        ]
        .concat(),
        // an answer whose name, and whose data, are a compression pointer to itself
        vec![
            0, 0, 0x84, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0xc0, 0x0c, 0, 12, 0, 1, 0, 0, 0, 0, 0, 2, 0xc0,
            0x0c,
        ],
        // a label that is longer than the packet
        [
            &[0u8, 0, 0x84, 0, 0, 0, 0, 1, 0, 0, 0, 0, 63][..],
            &[b'a'; 10],
        ]
        .concat(),
    ];
    // and noise of every length
    for length in [1usize, 2, 12, 13, 64, 255, 256, 511, 1200, 1471] {
        packets.push(
            (0..length)
                .map(|n| (n as u32 * 31 + length as u32) as u8)
                .collect(),
        );
    }
    let mut sent = 0;
    for packet in &packets {
        if socket.send_to(packet, ("224.0.0.251", 5353)).is_ok() {
            sent += 1;
        }
    }
    sent
}

/// BY HAND: two endpoints of this process find each other by id over the real mDNS of this
/// computer's network, with garbage on the multicast group meanwhile. A CI runner's network does
/// not carry multicast between the endpoints of one process reliably, so no job runs it.
#[test]
#[ignore = "X-EDGE-C-2: needs a network that carries multicast (a CI runner's does not): run by hand with cargo test -p azul-dll --lib --features iroh -- --ignored --nocapture two_endpoints_find_each_other_by_id_over_real_mdns"]
fn two_endpoints_find_each_other_by_id_over_real_mdns() {
    let host = Engine::bind(&config()).expect("the host binds");
    let guest = Engine::bind(&config()).expect("the guest binds");
    eprintln!(
        "local discovery: host {}, guest {}",
        host.local_discovery_status(),
        guest.local_discovery_status()
    );
    assert!(
        host.local_discovery_status().starts_with("on"),
        "{}",
        host.local_discovery_status()
    );
    assert!(
        guest.local_discovery_status().starts_with("on"),
        "{}",
        guest.local_discovery_status()
    );
    next_of(&host, IrohEventKind::Ready, WAIT);
    next_of(&guest, IrohEventKind::Ready, WAIT);

    eprintln!("hostile datagrams sent: {}", hostile_datagrams());

    let started = Instant::now();
    guest
        .connect(&host.endpoint_id())
        .expect("an endpoint id parses");
    let connected = next_of(
        &guest,
        IrohEventKind::PeerConnected,
        Duration::from_secs(90),
    );
    eprintln!("found by id and connected in {:?}", started.elapsed());
    assert_eq!(connected.text.as_str(), host.endpoint_id());
    next_of(&host, IrohEventKind::PeerConnected, WAIT);

    // the garbage did not take anything down
    hostile_datagrams();
    assert!(host.is_bound() && guest.is_bound());
    assert!(
        host.local_discovery_status().starts_with("on"),
        "{}",
        host.local_discovery_status()
    );
}

/// BY HAND: two endpoints of this process list each other over the real mDNS of this computer's
/// network with the text each advertises, hear a change of it, and hear one of them go; nothing is
/// dialed.
#[test]
#[ignore = "X-EDGE-C-2: needs a network that carries multicast (a CI runner's does not): run by hand with cargo test -p azul-dll --lib --features iroh -- --ignored --nocapture two_endpoints_list_each_other_over_real_mdns"]
fn two_endpoints_list_each_other_over_real_mdns() {
    let host = Engine::bind(
        &config().with_advertised(AzString::from_const_str("azreview/1;p=ab12;n=Studio Mac")),
    )
    .expect("the host binds");
    let guest = Engine::bind(
        &config().with_advertised(AzString::from_const_str("azreview/1;p=ab12;n=Linux box")),
    )
    .expect("the guest binds");
    let host_id = host.endpoint_id();

    let listed_with = |text: &str| {
        let deadline = Instant::now() + Duration::from_secs(90);
        loop {
            if guest
                .nearby_peers()
                .iter()
                .any(|peer| peer.id.to_string() == host_id && peer.advertised == text)
            {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "the guest does not list the host as {text:?}"
            );
            std::thread::sleep(Duration::from_millis(100));
        }
    };
    let started = Instant::now();
    listed_with("azreview/1;p=ab12;n=Studio Mac");
    eprintln!("listed in {:?}", started.elapsed());
    let appeared = next_of(
        &guest,
        IrohEventKind::NearbyAppeared,
        Duration::from_secs(30),
    );
    assert_eq!(appeared.text.as_str(), host_id);
    assert_eq!(guest.peer_count(), 0, "listing is not dialling");

    host.set_advertised("azreview/1;p=ab12;n=Studio Mac (renamed)");
    listed_with("azreview/1;p=ab12;n=Studio Mac (renamed)");

    // an announcement that stops is forgotten once its time to live has passed
    drop(host);
    let gone = next_of(&guest, IrohEventKind::NearbyGone, Duration::from_secs(200));
    assert_eq!(gone.text.as_str(), host_id);
    assert!(guest
        .nearby_peers()
        .iter()
        .all(|peer| peer.id.to_string() != host_id));
}
