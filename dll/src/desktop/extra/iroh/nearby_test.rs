//! The list of the endpoints on the local network: what an announcement is cut down to, what the
//! list holds, and which changes the app is told of (once each, however often they came).

use std::net::SocketAddr;

use iroh::{
    address_lookup::{EndpointData, EndpointInfo, UserData},
    SecretKey, TransportAddr,
};

use super::*;

fn id(seed: u8) -> EndpointId {
    SecretKey::from_bytes(&[seed; 32]).public()
}

fn ip(text: &str) -> SocketAddr {
    text.parse().expect("a socket address")
}

/// What endpoint `seed` announces: the text it advertises (if any) and its addresses.
fn info(seed: u8, advertised: Option<&str>, addresses: &[&str]) -> EndpointInfo {
    let addrs: Vec<TransportAddr> = addresses
        .iter()
        .map(|address| TransportAddr::Ip(ip(address)))
        .collect();
    let mut data = EndpointData::new(addrs);
    if let Some(text) = advertised {
        data = data.with_user_data(text.parse::<UserData>().expect("a text iroh holds"));
    }
    EndpointInfo::from_parts(id(seed), data)
}

fn own() -> EndpointId {
    id(1)
}

#[test]
fn what_is_advertised_is_cleaned_and_cut_at_what_an_announcement_holds() {
    assert_eq!(MAX_ADVERTISED_BYTES, UserData::MAX_LENGTH);

    let plain = advertised_text("azreview/1;p=ab12;n=Studio Mac");
    assert_eq!(plain.text, "azreview/1;p=ab12;n=Studio Mac");
    assert_eq!(plain.cut_from, None);

    // control characters (a line break in a name, an escape sequence) are not a name
    assert_eq!(
        advertised_text("a\u{0}b\nc\td\u{7f}e\u{85}f").text,
        "abcdef"
    );
    // a bidirectional override makes the text around a name read backwards: a spoof
    assert_eq!(
        advertised_text("\u{202e}evil\u{2066}!\u{2069}").text,
        "evil!"
    );
    // white space around the text is not part of it
    assert_eq!(advertised_text("  Studio Mac \n").text, "Studio Mac");
    assert_eq!(advertised_text("").text, "");
    assert_eq!(advertised_text("\u{0}\u{1}").text, "");

    // exactly what fits is kept, a byte more is cut
    let fits = "x".repeat(MAX_ADVERTISED_BYTES);
    assert_eq!(
        advertised_text(&fits),
        Advertised {
            text: fits.clone(),
            cut_from: None
        }
    );
    let over = "x".repeat(MAX_ADVERTISED_BYTES + 1);
    assert_eq!(
        advertised_text(&over),
        Advertised {
            text: fits,
            cut_from: Some(MAX_ADVERTISED_BYTES + 1)
        }
    );
    // a cut never splits a character: 300 two-byte letters keep 122 of them (244 bytes)
    let wide = "\u{e9}".repeat(300);
    let cut = advertised_text(&wide);
    assert_eq!(cut.text, "\u{e9}".repeat(122));
    assert_eq!(cut.cut_from, Some(600));
}

#[test]
fn a_nearby_peer_is_what_its_announcement_says_cut_down() {
    // loopback, unspecified, multicast, port 0 and an IPv6 link-local address cannot be dialed
    let many: Vec<String> = (1..=20).map(|n| format!("192.168.1.{n}:4000")).collect();
    let mut addresses: Vec<&str> = vec![
        "127.0.0.1:4000",
        "0.0.0.0:4000",
        "224.0.0.251:5353",
        "192.168.9.9:0",
        "[fe80::1]:4000",
    ];
    addresses.extend(many.iter().map(String::as_str));
    let peer = nearby_of(&info(7, Some("\u{202e}azreview/1;n=Anna"), &addresses));
    assert_eq!(peer.id, id(7));
    assert_eq!(peer.advertised, "azreview/1;n=Anna");
    assert_eq!(peer.addresses.len(), 16, "{:?}", peer.addresses);
    assert!(peer
        .addresses
        .iter()
        .all(|a| a.ip().to_string().starts_with("192.168.1.")));

    // no text, no address, a relay and nothing else
    let relay = TransportAddr::Relay("https://relay.example".parse().expect("a relay url"));
    let data = EndpointData::new(vec![relay]);
    let bare = nearby_of(&EndpointInfo::from_parts(id(8), data));
    assert_eq!(bare.advertised, "");
    assert!(bare.addresses.is_empty());
}

#[test]
fn the_list_holds_what_was_heard_by_endpoint_id_and_forgets_what_is_gone() {
    let nearby = Nearby::new(own());
    assert!(nearby.list().is_empty());

    nearby.seen(&info(3, Some("three"), &["192.168.1.3:4003"]));
    nearby.seen(&info(2, None, &["192.168.1.2:4002"]));
    // the endpoint itself is never listed, whatever the network says
    nearby.seen(&info(1, Some("me"), &["192.168.1.1:4001"]));
    let listed = nearby.list();
    assert_eq!(listed.len(), 2, "{listed:?}");
    assert!(listed[0].id < listed[1].id, "by endpoint id");
    let three = listed
        .iter()
        .find(|peer| peer.id == id(3))
        .expect("endpoint 3");
    assert_eq!(three.advertised, "three");
    assert_eq!(three.addresses, vec![ip("192.168.1.3:4003")]);

    // a newer announcement replaces the older, a repeat changes nothing
    nearby.seen(&info(3, Some("three, renamed"), &["192.168.1.30:4003"]));
    nearby.seen(&info(3, Some("three, renamed"), &["192.168.1.30:4003"]));
    assert_eq!(nearby.list().len(), 2);
    let three = nearby
        .list()
        .into_iter()
        .find(|peer| peer.id == id(3))
        .expect("endpoint 3");
    assert_eq!(three.advertised, "three, renamed");
    assert_eq!(three.addresses, vec![ip("192.168.1.30:4003")]);

    nearby.gone(&id(3));
    nearby.gone(&id(3));
    nearby.gone(&id(99));
    let left = nearby.list();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].id, id(2));
}

#[test]
fn the_app_is_told_of_every_change_once_however_often_it_came() {
    let nearby = Nearby::new(own());
    assert!(nearby.take_event().is_none());

    nearby.seen(&info(2, Some("two"), &["192.168.1.2:4002"]));
    let appeared = nearby.take_event().expect("a new endpoint is reported");
    assert_eq!(appeared.kind, IrohEventKind::NearbyAppeared);
    assert_eq!(appeared.text.as_str(), id(2).to_string());
    assert_eq!(appeared.data.as_ref(), b"two");
    assert_eq!(appeared.peer, 0, "it is no connection");
    assert!(nearby.take_event().is_none(), "once");

    // the same announcement again: nothing to tell
    nearby.seen(&info(2, Some("two"), &["192.168.1.2:4002"]));
    assert!(nearby.take_event().is_none());

    // two changes before the app asks: one report, the latest state
    nearby.seen(&info(2, Some("two, renamed"), &["192.168.1.2:4002"]));
    nearby.seen(&info(2, Some("two, renamed twice"), &["192.168.1.2:4002"]));
    let changed = nearby.take_event().expect("a change is reported");
    assert_eq!(changed.kind, IrohEventKind::NearbyAppeared);
    assert_eq!(changed.data.as_ref(), b"two, renamed twice");
    assert!(nearby.take_event().is_none());

    // a change of the addresses alone is a change too
    nearby.seen(&info(2, Some("two, renamed twice"), &["192.168.1.22:4002"]));
    assert_eq!(
        nearby.take_event().expect("new address").kind,
        IrohEventKind::NearbyAppeared
    );

    nearby.gone(&id(2));
    let gone = nearby
        .take_event()
        .expect("an endpoint the app knows vanished");
    assert_eq!(gone.kind, IrohEventKind::NearbyGone);
    assert_eq!(gone.text.as_str(), id(2).to_string());
    assert!(nearby.take_event().is_none());
}

#[test]
fn what_the_app_never_heard_of_is_not_reported_gone() {
    let nearby = Nearby::new(own());
    // heard and gone between two questions: the app never knew of it
    nearby.seen(&info(2, Some("two"), &["192.168.1.2:4002"]));
    nearby.gone(&id(2));
    assert!(nearby.take_event().is_none());

    // gone and back, unchanged, between two questions: nothing happened
    nearby.seen(&info(3, Some("three"), &["192.168.1.3:4003"]));
    assert!(nearby.take_event().is_some());
    nearby.gone(&id(3));
    nearby.seen(&info(3, Some("three"), &["192.168.1.3:4003"]));
    assert!(nearby.take_event().is_none());
    assert_eq!(nearby.list().len(), 1);
}

#[test]
fn the_list_is_capped_and_the_announcements_it_refused_are_counted() {
    let nearby = Nearby::new(own());
    for seed in 10..(10 + MAX_NEARBY_PEERS as u8 + 6) {
        nearby.seen(&info(seed, None, &["192.168.1.9:4000"]));
    }
    assert_eq!(nearby.list().len(), MAX_NEARBY_PEERS);
    assert_eq!(nearby.refused(), 6);

    // a listed endpoint can still change when the list is full
    nearby.seen(&info(10, Some("renamed"), &["192.168.1.9:4000"]));
    let first = nearby
        .list()
        .into_iter()
        .find(|peer| peer.id == id(10))
        .expect("endpoint 10");
    assert_eq!(first.advertised, "renamed");
    assert_eq!(nearby.refused(), 6);

    // and a place that is freed is taken by the next one that announces itself
    nearby.gone(&id(10));
    nearby.seen(&info(200, None, &["192.168.1.9:4000"]));
    assert_eq!(nearby.list().len(), MAX_NEARBY_PEERS);
    assert!(nearby.list().iter().any(|peer| peer.id == id(200)));

    // what is reported is bounded by the list: a flood is 64 reports at most
    let mut told = 0;
    while nearby.take_event().is_some() {
        told += 1;
        assert!(
            told <= 2 * MAX_NEARBY_PEERS,
            "the reports of a flood are not bounded"
        );
    }
    assert_eq!(told, MAX_NEARBY_PEERS);
}

#[test]
fn a_failure_of_the_lookup_is_kept_for_the_status_and_the_first_reason_stays() {
    let nearby = Nearby::new(own());
    assert_eq!(nearby.trouble(), None);
    nearby.set_trouble("Local Network access is denied".to_string());
    nearby.set_trouble("a second thing".to_string());
    assert_eq!(
        nearby.trouble().as_deref(),
        Some("Local Network access is denied")
    );
}
