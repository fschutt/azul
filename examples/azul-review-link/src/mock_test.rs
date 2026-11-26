//! The in-process network: dialing by ticket, latency, order, newest-wins frames, loss, a slow link, an address that
//! changed, a link that is down. The protocol tests stand on it, so it is held to account first.

use super::*;

fn two() -> (MockNet, MockLink, MockLink) {
    let net = MockNet::new();
    let mac = net.endpoint("mac");
    let phone = net.endpoint("phone");
    (net, mac, phone)
}

fn events(link: &mut MockLink, from: Millis, to: Millis) -> Vec<(Millis, LinkEvent)> {
    let mut out = Vec::new();
    for ms in from..=to {
        for e in link.poll(ms) {
            out.push((ms, e));
        }
    }
    out
}

/// The phone dials the Mac and both are connected: (the Mac's handle of it, the phone's handle of it).
fn connected(mac: &mut MockLink, phone: &mut MockLink, now: Millis) -> (PeerHandle, PeerHandle) {
    phone.connect(now, &mac.ticket()).unwrap();
    let theirs = events(mac, now, now + 50).into_iter().find_map(|(_, e)| if let LinkEvent::Connected { peer, .. } = e { Some(peer) } else { None }).expect("the Mac was connected");
    let mine = events(phone, now, now + 50).into_iter().find_map(|(_, e)| if let LinkEvent::Connected { peer, .. } = e { Some(peer) } else { None }).expect("the phone was connected");
    (theirs, mine)
}

fn received(list: &[(Millis, LinkEvent)]) -> Vec<(Millis, Vec<u8>)> {
    list.iter().filter_map(|(at, e)| if let LinkEvent::Received { bytes, .. } = e { Some((*at, bytes.clone())) } else { None }).collect()
}

// ---- keys and tickets

#[test]
fn an_endpoint_has_a_key_that_looks_like_the_transports_and_a_ticket_that_names_it() {
    let (_, mac, phone) = two();
    let key = mac.local_id();
    assert_eq!(key.len(), 64);
    assert!(key.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    assert_ne!(key, phone.local_id());
    assert_eq!(key, MockNet::key("mac"), "the same name is the same key");
    assert!(mac.ticket().contains(&key));
    assert!(mac.ticket().chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'), "a pairing offer can carry it");
}

#[test]
fn the_first_poll_of_an_endpoint_says_it_is_ready_once() {
    let (_, mut mac, _) = two();
    let first = mac.poll(0);
    assert_eq!(first, vec![LinkEvent::Ready { ticket: mac.ticket() }]);
    assert!(mac.poll(1).is_empty());
}

// ---- dialing

#[test]
fn a_dial_by_ticket_connects_both_ends_after_the_latency_and_each_learns_the_others_key() {
    let (_, mut mac, mut phone) = two();
    mac.poll(0);
    phone.poll(0);
    phone.connect(0, &mac.ticket()).unwrap();
    assert!(mac.poll(4).is_empty() && phone.poll(4).is_empty(), "the latency is 5 ms");
    let on_mac = mac.poll(5);
    let on_phone = phone.poll(5);
    assert!(matches!(&on_mac[..], [LinkEvent::Connected { id, .. }] if *id == phone.local_id()), "{on_mac:?}");
    assert!(matches!(&on_phone[..], [LinkEvent::Connected { id, .. }] if *id == mac.local_id()), "{on_phone:?}");
}

#[test]
fn a_ticket_that_is_garbled_or_the_endpoints_own_cannot_be_dialed_at_all() {
    let (_, mac, mut phone) = two();
    for bad in ["", "garbage", "mt-", "mt-abc", "mt-abc-x", "endpointabc"] {
        assert!(phone.connect(0, bad).is_err(), "{bad:?}");
    }
    assert!(phone.connect(0, &phone.ticket()).is_err());
    assert!(mac.ticket().starts_with("mt-"));
}

#[test]
fn a_bare_key_is_found_only_where_the_local_network_has_discovery_and_at_the_address_the_endpoint_has_now() {
    let (net, mut mac, mut phone) = two();
    let key = mac.local_id();
    // no discovery: a key is no ticket
    assert!(phone.connect(0, &key).is_err());
    // discovery: the key reaches the endpoint, also after its address changed
    net.set_knobs(Knobs { discovery: true, ..Knobs::default() });
    net.change_address("mac");
    phone.poll(0);
    phone.connect(0, &key).expect("a key is dialed on a network with discovery");
    let on_phone = events(&mut phone, 0, 100);
    assert!(on_phone.iter().any(|(_, e)| matches!(e, LinkEvent::Connected { id, .. } if *id == key)), "{on_phone:?}");
    let on_mac = events(&mut mac, 0, 100);
    assert!(on_mac.iter().any(|(_, e)| matches!(e, LinkEvent::Connected { id, .. } if *id == phone.local_id())), "{on_mac:?}");
    // a key nobody has is no ticket either
    assert!(phone.connect(200, &MockNet::key("nobody")).is_err());
}

#[test]
fn a_ticket_for_an_endpoint_that_is_not_there_fails_later_with_a_word() {
    let (net, _, mut phone) = two();
    let _ = net;
    let nowhere = format!("mt-{}-1", MockNet::key("nobody"));
    phone.poll(0);
    phone.connect(0, &nowhere).unwrap();
    let list = events(&mut phone, 0, 1_000);
    assert!(matches!(list.as_slice(), [(_, LinkEvent::Failed { text })] if text.contains("could not connect")), "{list:?}");
}

#[test]
fn after_an_address_change_the_old_ticket_leads_nowhere_and_the_new_one_works() {
    let (net, mut mac, mut phone) = two();
    let old = mac.ticket();
    net.change_address("mac");
    assert_ne!(old, mac.ticket());
    phone.connect(0, &old).unwrap();
    let list = events(&mut phone, 0, 1_000);
    assert!(list.iter().any(|(_, e)| matches!(e, LinkEvent::Failed { .. })), "{list:?}");
    let (host_side, phone_side) = connected(&mut mac, &mut phone, 2_000);
    let _ = (host_side, phone_side);
}

#[test]
fn when_the_network_is_down_a_dial_fails_and_what_was_sent_is_dropped_without_a_word() {
    let (net, mut mac, mut phone) = two();
    let (host_side, phone_side) = connected(&mut mac, &mut phone, 0);
    net.set_knobs(Knobs { down: true, ..Knobs::default() });
    assert!(phone.send(100, phone_side, b"into the void", Delivery::Reliable), "the sender is not told");
    assert!(received(&events(&mut mac, 100, 500)).is_empty());
    let _ = host_side;
    let mut other = net.endpoint("other");
    other.connect(600, &mac.ticket()).unwrap();
    assert!(events(&mut other, 600, 1_500).iter().any(|(_, e)| matches!(e, LinkEvent::Failed { .. })));
}

// ---- frames

#[test]
fn a_reliable_frame_arrives_after_the_latency_and_not_before() {
    let (_, mut mac, mut phone) = two();
    let (host_side, phone_side) = connected(&mut mac, &mut phone, 0);
    assert!(phone.send(100, phone_side, b"hello", Delivery::Reliable));
    assert!(mac.poll(104).iter().all(|e| !matches!(e, LinkEvent::Received { .. })));
    let list = mac.poll(105);
    assert_eq!(list, vec![LinkEvent::Received { peer: host_side, bytes: b"hello".to_vec(), delivery: Delivery::Reliable }]);
    assert!(mac.poll(500).is_empty(), "a frame arrives once");
}

#[test]
fn reliable_frames_arrive_in_the_order_they_were_sent_whatever_the_jitter() {
    let net = MockNet::with_knobs(Knobs { jitter_ms: 40, ..Knobs::default() });
    let (mut mac, mut phone) = (net.endpoint("mac"), net.endpoint("phone"));
    let (_, phone_side) = connected(&mut mac, &mut phone, 0);
    for i in 0..50u8 {
        phone.send(1_000, phone_side, &[i], Delivery::Reliable);
    }
    let got = received(&events(&mut mac, 1_000, 1_500));
    let order: Vec<u8> = got.iter().map(|(_, b)| b[0]).collect();
    assert_eq!(order, (0..50).collect::<Vec<u8>>());
}

#[test]
fn a_newer_frame_of_a_track_replaces_one_on_its_way_and_the_replaced_one_is_counted() {
    let (_, mut mac, mut phone) = two();
    let (_, phone_side) = connected(&mut mac, &mut phone, 0);
    phone.send(100, phone_side, b"one", Delivery::Latest(1));
    phone.send(101, phone_side, b"two", Delivery::Latest(1));
    phone.send(102, phone_side, b"three", Delivery::Latest(1));
    let got = received(&events(&mut mac, 100, 200));
    assert_eq!(got.iter().map(|(_, b)| b.clone()).collect::<Vec<_>>(), [b"three".to_vec()]);
    assert_eq!(phone.stats(phone_side).skipped, 2);
}

#[test]
fn tracks_do_not_replace_each_other_nor_reliable_frames() {
    let (_, mut mac, mut phone) = two();
    let (_, phone_side) = connected(&mut mac, &mut phone, 0);
    phone.send(100, phone_side, b"pen", Delivery::Latest(1));
    phone.send(100, phone_side, b"view", Delivery::Latest(2));
    phone.send(100, phone_side, b"reliable", Delivery::Reliable);
    let mut got: Vec<Vec<u8>> = received(&events(&mut mac, 100, 200)).into_iter().map(|(_, b)| b).collect();
    got.sort();
    assert_eq!(got, [b"pen".to_vec(), b"reliable".to_vec(), b"view".to_vec()]);
}

#[test]
fn a_frame_that_has_arrived_is_not_replaced_by_the_next() {
    let (_, mut mac, mut phone) = two();
    let (_, phone_side) = connected(&mut mac, &mut phone, 0);
    phone.send(100, phone_side, b"one", Delivery::Latest(1));
    let first = received(&events(&mut mac, 100, 110));
    phone.send(120, phone_side, b"two", Delivery::Latest(1));
    let second = received(&events(&mut mac, 120, 140));
    assert_eq!((first.len(), second.len()), (1, 1));
    assert_eq!(phone.stats(phone_side).skipped, 0);
}

#[test]
fn loss_drops_some_newest_wins_frames_and_never_a_reliable_one() {
    let net = MockNet::with_knobs(Knobs { loss_percent: 30, seed: 7, ..Knobs::default() });
    let (mut mac, mut phone) = (net.endpoint("mac"), net.endpoint("phone"));
    let (_, phone_side) = connected(&mut mac, &mut phone, 0);
    for i in 0..1_000u64 {
        phone.send(1_000 + i * 20, phone_side, b"x", Delivery::Latest(1));
        phone.send(1_000 + i * 20, phone_side, b"r", Delivery::Reliable);
    }
    let got = received(&events(&mut mac, 1_000, 22_000));
    let latest = got.iter().filter(|(_, b)| b == b"x").count();
    let reliable = got.iter().filter(|(_, b)| b == b"r").count();
    assert_eq!(reliable, 1_000, "reliable frames are never lost");
    assert!((600..=800).contains(&latest), "{latest} of 1000 newest-wins frames arrived at 30 % loss");
}

#[test]
fn reordering_lets_a_newest_wins_frame_lag_behind_a_reliable_one_sent_after_it() {
    let net = MockNet::with_knobs(Knobs { reorder_percent: 100, ..Knobs::default() });
    let (mut mac, mut phone) = (net.endpoint("mac"), net.endpoint("phone"));
    let (_, phone_side) = connected(&mut mac, &mut phone, 0);
    phone.send(100, phone_side, b"late", Delivery::Latest(1));
    phone.send(101, phone_side, b"first", Delivery::Reliable);
    let got = received(&events(&mut mac, 100, 300));
    assert_eq!(got.iter().map(|(_, b)| b.clone()).collect::<Vec<_>>(), [b"first".to_vec(), b"late".to_vec()]);
}

#[test]
fn a_slow_link_takes_time_to_carry_each_frame_and_reports_the_backlog() {
    let net = MockNet::with_knobs(Knobs { bytes_per_second: Some(10_000), ..Knobs::default() });
    let (mut mac, mut phone) = (net.endpoint("mac"), net.endpoint("phone"));
    let (_, phone_side) = connected(&mut mac, &mut phone, 0);
    for i in 0..5u8 {
        phone.send(1_000, phone_side, &vec![i; 4_000], Delivery::Reliable);
    }
    assert_eq!(phone.stats(phone_side).queued_bytes, 20_000);
    let got = received(&events(&mut mac, 1_000, 4_000));
    let times: Vec<Millis> = got.iter().map(|(at, _)| *at).collect();
    assert_eq!(times.len(), 5);
    assert!(times[0] >= 1_000 + 400 && times[0] < 1_000 + 420, "{times:?}");
    assert!(times[4] >= 1_000 + 2_000 && times[4] < 1_000 + 2_030, "{times:?}");
    assert_eq!(phone.stats(phone_side).queued_bytes, 0);
}

// ---- closing

#[test]
fn disconnecting_tells_both_ends_and_a_closed_connection_takes_no_more() {
    let (_, mut mac, mut phone) = two();
    let (host_side, phone_side) = connected(&mut mac, &mut phone, 0);
    phone.disconnect(phone_side);
    let on_mac = events(&mut mac, 10, 100);
    let on_phone = events(&mut phone, 10, 100);
    assert!(on_mac.iter().any(|(_, e)| matches!(e, LinkEvent::Disconnected { peer, .. } if *peer == host_side)), "{on_mac:?}");
    assert!(on_phone.iter().any(|(_, e)| matches!(e, LinkEvent::Disconnected { peer, .. } if *peer == phone_side)), "{on_phone:?}");
    assert!(!phone.send(200, phone_side, b"x", Delivery::Reliable));
    assert!(!mac.send(200, host_side, b"x", Delivery::Reliable));
    assert!(!phone.stats(phone_side).connected);
}

#[test]
fn a_peer_that_does_not_exist_is_refused() {
    let (_, mut mac, _) = two();
    assert!(!mac.send(0, 99, b"x", Delivery::Reliable));
    assert_eq!(mac.stats(99), LinkStats::default());
    mac.disconnect(99);
}

#[test]
fn killing_the_connections_of_an_endpoint_tells_the_other_end_after_the_latency() {
    let (net, mut mac, mut phone) = two();
    let (_, phone_side) = connected(&mut mac, &mut phone, 0);
    net.kill_connections(500, "mac");
    assert_eq!(net.open_connections(), 0);
    let on_phone = events(&mut phone, 500, 600);
    assert!(on_phone.iter().any(|(at, e)| *at >= 505 && matches!(e, LinkEvent::Disconnected { peer, .. } if *peer == phone_side)), "{on_phone:?}");
}

#[test]
fn closing_an_endpoint_closes_what_it_had_open() {
    let (net, mut mac, mut phone) = two();
    connected(&mut mac, &mut phone, 0);
    assert_eq!(net.open_connections(), 1);
    mac.close();
    assert_eq!(net.open_connections(), 0);
    assert!(events(&mut phone, 10, 50).iter().any(|(_, e)| matches!(e, LinkEvent::Disconnected { .. })));
}

// ---- the run is the same every time

#[test]
fn the_same_seed_gives_the_same_run() {
    let run = || {
        let net = MockNet::with_knobs(Knobs { loss_percent: 40, jitter_ms: 20, reorder_percent: 30, seed: 99, ..Knobs::default() });
        let (mut mac, mut phone) = (net.endpoint("mac"), net.endpoint("phone"));
        let (_, phone_side) = connected(&mut mac, &mut phone, 0);
        for i in 0..200u64 {
            phone.send(1_000 + i * 5, phone_side, &[(i % 251) as u8], Delivery::Latest((i % 3) as u8));
        }
        received(&events(&mut mac, 1_000, 3_000))
    };
    assert_eq!(run(), run());
}
