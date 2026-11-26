//! The iroh link. What the engine's events become is plain data and is checked wherever libazul is linked. The transport itself needs the
//! engine in the library: two endpoints on this machine (no relay) connect by ticket, exchange both kinds of frames, and
//! the Mac's host and the phone's client pair and talk over them. A library without the engine (a stubbed one binds nothing) skips
//! those tests and says so, unless AZ_REQUIRE_IROH_ENGINE is set, as it is on the runner: then a missing engine is a failure.

use std::time::{Duration, Instant};

use azul::iroh::{IrohEvent, IrohEventKind, IrohNearbyPeer, IrohPeerStats, IrohRelayMode};
use azul::str::String as AzString;
use azul::vec::{StringVec, U8Vec};

use super::*;
use crate::client::{ClientConfig, PhoneClient};
use crate::frame::{ButtonSet, Entry, EntryKind, FileState, Focus, PenState, Phase, ReplyStatus, Rows, ViewportOp};
use crate::host::{CompanionHost, HostConfig, HostEvent};
use crate::pairing::SeededEntropy;
use crate::phone::PhoneApi;
use crate::store::MemoryStore;
use crate::view::Session;

fn event(kind: IrohEventKind, peer: u64, text: &str, data: &[u8], track: u32) -> IrohEvent {
    IrohEvent { peer, sequence: 0, text: AzString::from(text), data: U8Vec::from(data.to_vec()), kind, track }
}

// ---- what the engine's events become

#[test]
fn a_ready_event_is_the_ticket() {
    assert_eq!(link_event(&event(IrohEventKind::Ready, 0, "the-ticket", &[], 0)), Some(LinkEvent::Ready { ticket: "the-ticket".into() }));
}

#[test]
fn a_connection_is_a_peer_handle_and_the_remote_key() {
    assert_eq!(link_event(&event(IrohEventKind::PeerConnected, 3, "abcd", &[], 0)), Some(LinkEvent::Connected { peer: 3, id: "abcd".into() }));
    assert_eq!(link_event(&event(IrohEventKind::PeerDisconnected, 3, "closed", &[], 0)), Some(LinkEvent::Disconnected { peer: 3, reason: "closed".into() }));
}

#[test]
fn a_message_is_reliable_and_a_frame_is_newest_wins_on_its_track() {
    assert_eq!(link_event(&event(IrohEventKind::Message, 2, "", &[1, 2, 3], 0)), Some(LinkEvent::Received { peer: 2, bytes: vec![1, 2, 3], delivery: Delivery::Reliable }));
    assert_eq!(link_event(&event(IrohEventKind::Frame, 2, "", &[9], 2)), Some(LinkEvent::Received { peer: 2, bytes: vec![9], delivery: Delivery::Latest(2) }));
    assert_eq!(
        link_event(&event(IrohEventKind::Frame, 2, "", &[9], 70_000)),
        Some(LinkEvent::Received { peer: 2, bytes: vec![9], delivery: Delivery::Latest(u8::MAX) }),
        "a track the protocol does not have is the last of them, whatever the peer says"
    );
}

#[test]
fn an_error_is_a_failure_and_one_the_system_caused_says_so() {
    assert_eq!(link_event(&event(IrohEventKind::Error, 0, "no answer from abcd", &[], 0)), Some(LinkEvent::Failed { text: "no answer from abcd".into() }));
    for text in [
        "Operation not permitted (os error 1)",
        "bind: Permission denied",
        "the Local Network switch is off",
        "the lookup of the Mac failed: kDNSServiceErr_PolicyDenied",
    ] {
        let Some(LinkEvent::Failed { text: said }) = link_event(&event(IrohEventKind::Error, 0, text, &[], 0)) else {
            panic!("a failure was expected");
        };
        assert!(said.starts_with(PERMISSION_PREFIX), "{said}");
        assert!(said.ends_with(text), "the engine's words stay: {said}");
    }
}

fn advert_text() -> String {
    crate::advert::encode(&crate::advert::Advert { project: crate::advert::project_hash("the repo"), name: "Studio Mac".into(), user: String::new(), role: Some(crate::frame::Role::Host) })
}

#[test]
fn an_announcement_of_the_local_network_is_a_neighbour_with_its_text_and_the_addresses_the_engine_lists() {
    let announced = event(IrohEventKind::NearbyAppeared, 0, "abcd", advert_text().as_bytes(), 0);
    let Some(NearbyEvent::Appeared(near)) = nearby_event(&announced, vec!["192.168.1.5:4000".into()]) else {
        panic!("an announcement was expected");
    };
    assert_eq!(near.endpoint_id, "abcd");
    assert_eq!(near.addresses, ["192.168.1.5:4000"]);
    assert_eq!(near.advert.map(|a| (a.name, a.role)), Some(("Studio Mac".to_string(), Some(crate::frame::Role::Host))));
}

#[test]
fn what_a_stranger_announces_is_a_neighbour_without_an_advert_and_never_a_crash() {
    for bytes in [&b"some other app's text"[..], &[0xff, 0xfe, 0x00][..], &[][..]] {
        let Some(NearbyEvent::Appeared(near)) = nearby_event(&event(IrohEventKind::NearbyAppeared, 0, "abcd", bytes, 0), Vec::new()) else {
            panic!("an announcement was expected");
        };
        assert_eq!((near.endpoint_id.as_str(), near.advert), ("abcd", None));
    }
}

#[test]
fn a_neighbour_that_goes_is_named_by_its_key_and_nothing_else_is_news() {
    assert_eq!(nearby_event(&event(IrohEventKind::NearbyGone, 0, "abcd", &[], 0), Vec::new()), Some(NearbyEvent::Gone("abcd".into())));
    for kind in [IrohEventKind::Ready, IrohEventKind::PeerConnected, IrohEventKind::Frame, IrohEventKind::Message, IrohEventKind::Error] {
        assert_eq!(nearby_event(&event(kind, 1, "x", b"y", 1), Vec::new()), None, "{kind:?}");
    }
    // and an announcement is not a connection
    assert_eq!(link_event(&event(IrohEventKind::NearbyAppeared, 0, "abcd", b"text", 0)), None);
    assert_eq!(link_event(&event(IrohEventKind::NearbyGone, 0, "abcd", &[], 0)), None);
}

#[test]
fn a_neighbour_the_engine_lists_is_read_the_same_way() {
    let listed = IrohNearbyPeer { endpoint_id: AzString::from("abcd"), advertised: AzString::from(advert_text().as_str()), addresses: StringVec::from_vec(vec![AzString::from("10.0.0.2:5000"), AzString::from("[fe80::1]:5000")]) };
    let near = neighbour(&listed);
    assert_eq!(near.endpoint_id, "abcd");
    assert_eq!(near.addresses, ["10.0.0.2:5000", "[fe80::1]:5000"]);
    assert_eq!(near.advert.map(|a| a.name), Some("Studio Mac".to_string()));
}

#[test]
fn an_advertised_text_is_part_of_what_is_bound_and_a_link_without_neighbours_has_none() {
    let Ok(link) = IrohLink::bind(&IrohOptions { advertised: Some(advert_text()), ..IrohOptions::default() }) else {
        return;
    };
    let mut discovery = link.discovery();
    // (other endpoints of this machine's other tests may announce themselves: the list is whoever is there, never this endpoint)
    assert!(discovery.nearby().iter().all(|near| near.endpoint_id != link.local_id()));
    let _news = discovery.poll(0);
    assert!(!link.discovery_status().is_empty(), "the engine says what it does for the local network");
}

#[test]
fn an_app_can_extend_the_config_and_bind_it() {
    // the same key through `config` and `bind_config` is the same endpoint as through `bind`
    let key = [9u8; 32];
    let Ok(first) = IrohLink::bind(&IrohOptions { secret_key: Some(key), ..IrohOptions::default() }) else {
        return;
    };
    let id = first.local_id();
    drop(first);
    let config = IrohLink::config(&IrohOptions { secret_key: Some(key), ..IrohOptions::default() }).with_max_frame_bytes(1_000_000);
    let again = IrohLink::bind_config(config).expect("a config made from the options binds");
    assert_eq!(again.local_id(), id);
}

#[test]
fn the_relay_is_off_unless_asked_for() {
    assert_eq!(relay_mode(false), IrohRelayMode::Disabled);
    assert_eq!(relay_mode(true), IrohRelayMode::Default);
}

#[test]
fn the_engines_numbers_become_the_links() {
    let engine = IrohPeerStats { connected: true, direct: true, rtt_us: 2_500, frames_skipped: 7, message_bytes_queued: 4_096, ..IrohPeerStats::default() };
    assert_eq!(link_stats(&engine), LinkStats { connected: true, direct: true, rtt_ms: 2, skipped: 7, queued_bytes: 4_096 });
    assert_eq!(link_stats(&IrohPeerStats::default()), LinkStats::default(), "a connection that is gone is all zero");
    assert_eq!(link_stats(&IrohPeerStats { rtt_us: u64::MAX, ..IrohPeerStats::default() }).rtt_ms, u32::MAX, "a round trip beyond a u32 of milliseconds is held");
}

#[test]
fn the_defaults_are_no_relay_and_a_new_key() {
    assert_eq!(IrohOptions::default(), IrohOptions { secret_key: None, relay: false, advertised: None, local_discovery: true });
}

// ---- the transport (needs the engine)

fn quiet() -> IrohOptions {
    // no company: the transport tests dial by ticket, and the engine's announcements of the other tests' endpoints are not their business
    IrohOptions { local_discovery: false, ..IrohOptions::default() }
}

/// A bound link, or `None` when this libazul has no iroh engine (the test is skipped, or fails when the run says the engine must be there).
fn bind(options: &IrohOptions) -> Option<IrohLink> {
    match IrohLink::bind(options) {
        Ok(link) => Some(link),
        Err(why) => {
            assert!(std::env::var_os("AZ_REQUIRE_IROH_ENGINE").is_none(), "this library has no iroh engine and the run says it must have: {why}");
            eprintln!("iroh_link_test: no iroh engine in this libazul ({why}): the transport test is skipped");
            None
        }
    }
}

struct Clock(Instant);

impl Clock {
    fn new() -> Clock {
        Clock(Instant::now())
    }

    fn now(&self) -> Millis {
        u64::try_from(self.0.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

/// Runs `step` every few milliseconds until it says it is done; fails after twenty seconds.
fn pump(clock: &Clock, what: &str, mut step: impl FnMut(Millis) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if step(clock.now()) {
            return;
        }
        assert!(Instant::now() < deadline, "{what}: not within 20 s");
        std::thread::sleep(Duration::from_millis(4));
    }
}

/// Two bound links, connected: `phone` dialed `host`.
struct Pair {
    host: IrohLink,
    phone: IrohLink,
    host_peer: PeerHandle,
    phone_peer: PeerHandle,
    clock: Clock,
}

fn connected_pair() -> Option<Pair> {
    let (mut host, mut phone) = (bind(&quiet())?, bind(&quiet())?);
    let clock = Clock::new();
    let mut ticket = String::new();
    pump(&clock, "the host knows its address", |now| {
        for event in host.poll(now) {
            if let LinkEvent::Ready { ticket: found } = event {
                ticket = found;
            }
        }
        !ticket.is_empty()
    });
    assert!(!host.ticket().is_empty(), "the host's ticket is there to read");
    phone.connect(clock.now(), &ticket).expect("the ticket can be dialed");
    let (mut host_sees, mut phone_sees) = (None, None);
    let mut failures = Vec::new();
    pump(&clock, "both ends see the connection", |now| {
        for event in host.poll(now) {
            match event {
                LinkEvent::Connected { peer, id } => host_sees = Some((peer, id)),
                LinkEvent::Failed { text } => failures.push(text),
                _ => {}
            }
        }
        for event in phone.poll(now) {
            match event {
                LinkEvent::Connected { peer, id } => phone_sees = Some((peer, id)),
                LinkEvent::Failed { text } => failures.push(text),
                _ => {}
            }
        }
        host_sees.is_some() && phone_sees.is_some()
    });
    assert!(failures.is_empty(), "{failures:?}");
    let ((host_peer, phone_key), (phone_peer, host_key)) = (host_sees?, phone_sees?);
    assert_eq!(phone_key, phone.local_id(), "the host sees the phone's key as the transport authenticated it");
    assert_eq!(host_key, host.local_id(), "and the phone the host's");
    Some(Pair { host, phone, host_peer, phone_peer, clock })
}

#[test]
fn two_endpoints_connect_by_ticket_and_each_sees_the_others_key() {
    let Some(pair) = connected_pair() else {
        return;
    };
    assert_ne!(pair.host.local_id(), pair.phone.local_id());
    let stats = pair.host.stats(pair.host_peer);
    assert!(stats.connected, "{stats:?}");
}

#[test]
fn messages_arrive_in_order_and_the_newest_frame_of_a_track_always_arrives() {
    let Some(mut pair) = connected_pair() else {
        return;
    };
    for i in 0..20 {
        assert!(pair.phone.send(pair.clock.now(), pair.phone_peer, format!("message {i}").as_bytes(), Delivery::Reliable));
    }
    for i in 0..100 {
        assert!(pair.phone.send(pair.clock.now(), pair.phone_peer, format!("frame {i}").as_bytes(), Delivery::Latest(1)));
    }
    let (mut messages, mut frames) = (Vec::new(), Vec::new());
    let Pair { host, clock, .. } = &mut pair;
    pump(clock, "the messages and the newest frame arrive", |now| {
        for event in host.poll(now) {
            if let LinkEvent::Received { bytes, delivery, .. } = event {
                let text = String::from_utf8_lossy(&bytes).to_string();
                match delivery {
                    Delivery::Reliable => messages.push(text),
                    Delivery::Latest(track) => {
                        assert_eq!(track, 1);
                        frames.push(text);
                    }
                }
            }
        }
        messages.len() == 20 && frames.last().is_some_and(|f| f == "frame 99")
    });
    let expected: Vec<String> = (0..20).map(|i| format!("message {i}")).collect();
    assert_eq!(messages, expected, "reliable messages arrive in the order they were sent");
    assert!(frames.len() <= 100 && frames.windows(2).all(|w| w[0] != w[1]), "newest wins: the frames in between may be skipped, never repeated");
}

#[test]
fn a_peer_that_disconnects_is_told_to_the_other_end() {
    let Some(mut pair) = connected_pair() else {
        return;
    };
    pair.phone.disconnect(pair.phone_peer);
    let Pair { host, clock, host_peer, .. } = &mut pair;
    let mut gone = None;
    pump(clock, "the host is told", |now| {
        for event in host.poll(now) {
            if let LinkEvent::Disconnected { peer, reason } = event {
                gone = Some((peer, reason));
            }
        }
        gone.is_some()
    });
    assert_eq!(gone.map(|g| g.0), Some(*host_peer));
}

#[test]
fn a_message_over_the_protocols_cap_does_not_arrive() {
    let Some(mut pair) = connected_pair() else {
        return;
    };
    let big = vec![7u8; MAX_FRAME_BYTES + 1_000];
    pair.phone.send(pair.clock.now(), pair.phone_peer, &big, Delivery::Reliable);
    let Pair { host, clock, .. } = &mut pair;
    let started = Instant::now();
    let mut arrived_whole = false;
    while started.elapsed() < Duration::from_millis(1_500) {
        for event in host.poll(clock.now()) {
            if let LinkEvent::Received { bytes, .. } = event {
                arrived_whole |= bytes.len() == MAX_FRAME_BYTES + 1_000;
            }
        }
        std::thread::sleep(Duration::from_millis(4));
    }
    assert!(!arrived_whole, "the endpoint takes frames of the protocol's cap at most");
}

#[test]
fn a_ticket_that_is_no_ticket_is_refused_at_once() {
    let Some(mut link) = bind(&quiet()) else {
        return;
    };
    assert_eq!(link.connect(0, ""), Err("there is no address to dial".to_string()));
    assert!(link.connect(0, "this is not a ticket!").is_err());
}

#[test]
fn the_same_key_is_the_same_endpoint_after_a_restart() {
    let Some(first) = bind(&quiet()) else {
        return;
    };
    let key = first.secret_key().expect("a bound endpoint has a key");
    let id = first.local_id();
    drop(first);
    let again = bind(&IrohOptions { secret_key: Some(key), ..quiet() }).expect("it binds again");
    assert_eq!(again.local_id(), id, "the key is the identity");
    assert_eq!(again.secret_key(), Some(key));
    let other = bind(&quiet()).expect("a new one binds");
    assert_ne!(other.local_id(), id, "no key given is a new identity");
}

#[test]
fn the_events_of_a_flood_are_taken_a_poll_at_a_time() {
    let Some(mut pair) = connected_pair() else {
        return;
    };
    for _ in 0..(MAX_EVENTS_PER_POLL + 100) {
        pair.phone.send(pair.clock.now(), pair.phone_peer, b"x", Delivery::Reliable);
    }
    // let them all arrive before the host looks: the first poll then has more than it may take
    std::thread::sleep(Duration::from_millis(800));
    let Pair { host, clock, .. } = &mut pair;
    let (mut taken, mut biggest) = (0, 0);
    pump(clock, "the whole flood arrives", |now| {
        let events = host.poll(now);
        biggest = biggest.max(events.len());
        taken += events.iter().filter(|e| matches!(e, LinkEvent::Received { .. })).count();
        taken == MAX_EVENTS_PER_POLL + 100
    });
    assert!(biggest <= MAX_EVENTS_PER_POLL, "{biggest}");
    assert!(biggest > MAX_EVENTS_PER_POLL / 2, "the flood did queue up: {biggest}");
}

// ---- the host and the phone's client over it

fn host_config() -> HostConfig {
    HostConfig { mac_name: "Studio Mac".into(), app_version: "0.1.0".into(), max_phones: 4 }
}

fn client_config() -> ClientConfig {
    ClientConfig { phone_name: "Test phone".into(), app_version: "0.1.0".into() }
}

#[test]
fn the_host_and_the_phone_client_pair_and_talk_over_the_real_transport() {
    let (Some(host_link), Some(phone_link)) = (bind(&quiet()), bind(&quiet())) else {
        return;
    };
    let phone_key = phone_link.secret_key().expect("a key");
    let mut host = CompanionHost::new(host_link, host_config(), Box::new(SeededEntropy(7)));
    let store = MemoryStore::new();
    let mut phone = PhoneClient::new(phone_link, client_config(), Box::new(store.clone()));
    let clock = Clock::new();
    let mut events: Vec<HostEvent> = Vec::new();
    let work = |phone: &mut PhoneClient<IrohLink>, host: &mut CompanionHost<IrohLink>, events: &mut Vec<HostEvent>, now: Millis| {
        events.extend(host.poll(now));
        phone.poll(now);
    };

    pump(&clock, "the host knows its ticket", |now| {
        work(&mut phone, &mut host, &mut events, now);
        !host.ticket().is_empty()
    });
    // the pairing: the Mac shows an offer, the phone takes it, both show the code, the person at the Mac says yes
    let offer = host.begin_pairing(clock.now()).expect("an offer");
    phone.begin_pairing(clock.now(), &offer.to_text()).expect("an offer the phone takes");
    pump(&clock, "the host is asked to pair", |now| {
        work(&mut phone, &mut host, &mut events, now);
        events.iter().any(|e| matches!(e, HostEvent::PairingRequested { .. }))
    });
    let (peer, code) = events.iter().find_map(|e| if let HostEvent::PairingRequested { peer, code, .. } = e { Some((peer.clone(), *code)) } else { None }).expect("asked");
    assert_eq!(phone.view().session, Session::Pairing { mac_name: "Studio Mac".into(), code }, "the phone shows the code the Mac shows");
    host.accept_pairing(clock.now(), &peer).expect("confirmed");
    pump(&clock, "the phone is served", |now| {
        work(&mut phone, &mut host, &mut events, now);
        phone.is_active()
    });
    assert_eq!(store.peek().mac.map(|m| m.name), Some("Studio Mac".to_string()), "the Mac is kept");

    // the pen goes to the phone
    host.publish_pen(PenState { folder: "azul".into(), ..PenState::empty() });
    pump(&clock, "the pen state arrives", |now| {
        work(&mut phone, &mut host, &mut events, now);
        phone.view().pen.as_ref().is_some_and(|p| p.folder == "azul")
    });

    // a drag goes to the Mac: a begin, updates, an end with the last totals
    events.clear();
    phone.gesture(clock.now(), 5, Phase::Begin, (0.0, 0.0), 1.0, Focus::Pen);
    for i in 1..=30 {
        phone.gesture(clock.now(), 5, Phase::Update, (i as f32 / 100.0, 0.0), 1.0, Focus::Pen);
        std::thread::sleep(Duration::from_millis(5));
        work(&mut phone, &mut host, &mut events, clock.now());
    }
    phone.gesture(clock.now(), 5, Phase::End, (0.3, 0.0), 1.0, Focus::Pen);
    let ended = |events: &[HostEvent]| events.iter().any(|e| matches!(e, HostEvent::Viewport { op: ViewportOp::Gesture { phase: Phase::End, .. }, .. }));
    pump(&clock, "the end of the gesture arrives", |now| {
        work(&mut phone, &mut host, &mut events, now);
        ended(&events)
    });
    let phases: Vec<Phase> = events.iter().filter_map(|e| if let HostEvent::Viewport { op: ViewportOp::Gesture { phase, .. }, .. } = e { Some(*phase) } else { None }).collect();
    assert_eq!(phases.first(), Some(&Phase::Begin));
    assert_eq!(phases.last(), Some(&Phase::End));

    // a folder is listed
    phone.list_dir(clock.now(), "src", 0);
    pump(&clock, "the host is asked for the folder", |now| {
        work(&mut phone, &mut host, &mut events, now);
        events.iter().any(|e| matches!(e, HostEvent::ListDir { .. }))
    });
    let (peer, request_id) = events.iter().find_map(|e| if let HostEvent::ListDir { peer, request_id, .. } = e { Some((peer.clone(), *request_id)) } else { None }).expect("asked");
    host.publish_rows(clock.now(), &peer, Rows { request_id, dir: "src".into(), offset: 0, entries: vec![Entry { name: "lib.rs".into(), kind: EntryKind::File, state: FileState::Clean }], more: false });
    pump(&clock, "the rows arrive", |now| {
        work(&mut phone, &mut host, &mut events, now);
        phone.view().files.entries.len() == 1
    });

    // a quick button: the Mac gets the press with its text, the phone the status
    phone.press_button(clock.now(), 0);
    pump(&clock, "the press is acknowledged", |now| {
        work(&mut phone, &mut host, &mut events, now);
        phone.view().button_status[0] == Some(ReplyStatus::Received)
    });
    assert!(events.iter().any(|e| matches!(e, HostEvent::ButtonPressed { index: 0, label, .. } if label == "Tone")));

    // the buttons are saved and come back
    let mut set = ButtonSet::defaults();
    set.buttons[1].label = "Mine".into();
    assert!(phone.set_buttons(clock.now(), &set).is_empty());
    pump(&clock, "the Mac has the buttons", |now| {
        work(&mut phone, &mut host, &mut events, now);
        host.buttons().buttons[1].label == "Mine"
    });

    // the phone's app is closed and opened again: the same key, the Mac in the store: served again, no pairing
    phone.close(clock.now());
    drop(phone);
    let again = bind(&IrohOptions { secret_key: Some(phone_key), ..quiet() }).expect("the phone's endpoint binds again");
    let mut phone = PhoneClient::new(again, client_config(), Box::new(store.clone()));
    assert!(matches!(phone.view().session, Session::Searching { .. }), "{:?}", phone.view().session);
    // (the Mac tells the buttons when it serves the phone: a moment after the phone knows it is served)
    pump(&clock, "the phone is served again without pairing, and has the Mac's buttons", |now| {
        work(&mut phone, &mut host, &mut events, now);
        phone.is_active() && phone.view().buttons.buttons[1].label == "Mine"
    });
}
