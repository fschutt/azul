//! The Mac's host against a phone made of a bare endpoint on the mock network: who may talk, how a phone is paired, what each frame
//! of a paired phone turns into, what goes out and how often, how gestures come out in order, what the caps do to a hostile peer.

use super::*;
use crate::access::ScreenId;
use crate::access::{PeerKind, ScreenInfo, TrustGroup, TrustRecord};
use crate::frame::{
    AgentFinished, Context, Entry, EntryKind, FileState, FinishKind, Focus, NoticeLevel, PageSurface, Phase, QuickButton, Rect, Surface, ThumbFormat,
};
use crate::limits::{BUTTON_COUNT, MAX_PATH_BYTES, PAIRING_MAX_WRONG_PROOFS, PAIRING_WINDOW_MS, PEER_TIMEOUT_MS, PING_INTERVAL_MS};
use crate::link::{Delivery, Link, LinkEvent, PeerHandle};
use crate::mock::{Knobs, MockLink, MockNet};
use crate::pairing::{confirmation_code, proof, SeededEntropy};
use crate::wire::{decode, encode};

/// A host and a phone that is only an endpoint, with a clock.
struct World {
    net: MockNet,
    host: CompanionHost<MockLink>,
    phone: MockLink,
    phone_id: String,
    host_id: String,
    peer: Option<PeerHandle>,
    now: Millis,
    events: Vec<HostEvent>,
    frames: Vec<Frame>,
    refused_bytes: Vec<Vec<u8>>,
    disconnected: bool,
    /// The fake phone answers the host's pings, as a real phone does.
    auto_pong: bool,
}

fn config() -> HostConfig {
    HostConfig { mac_name: "Studio Mac".into(), app_version: "0.1.0".into(), max_phones: MAX_PAIRED_PHONES }
}

impl World {
    fn new() -> World {
        let net = MockNet::with_knobs(Knobs { latency_ms: 1, ..Knobs::default() });
        let host_link = net.endpoint("mac");
        let host_id = host_link.local_id();
        let phone = net.endpoint("phone");
        let phone_id = phone.local_id();
        let host = CompanionHost::new(host_link, config(), Box::new(SeededEntropy(7)));
        let mut w = World { net, host, phone, phone_id, host_id, peer: None, now: 1_000, events: Vec::new(), frames: Vec::new(), refused_bytes: Vec::new(), disconnected: false, auto_pong: false };
        w.step(5);
        w.events.clear();
        w
    }

    fn step(&mut self, ms: u64) {
        for _ in 0..ms {
            self.now += 1;
            self.events.extend(self.host.poll(self.now));
            for event in self.phone.poll(self.now) {
                match event {
                    LinkEvent::Connected { peer, .. } => self.peer = Some(peer),
                    LinkEvent::Received { bytes, .. } => match decode(&bytes) {
                        Ok(frame) => {
                            if let (Frame::Ping { nonce }, true, Some(peer)) = (&frame, self.auto_pong, self.peer) {
                                let pong = encode(&Frame::Pong { nonce: *nonce }).expect("a pong");
                                self.phone.send(self.now, peer, &pong, Delivery::Reliable);
                            }
                            self.frames.push(frame);
                        }
                        Err(_) => self.refused_bytes.push(bytes),
                    },
                    LinkEvent::Disconnected { .. } => self.disconnected = true,
                    _ => {}
                }
            }
        }
    }

    fn dial(&mut self) {
        self.disconnected = false;
        let ticket = self.host.ticket().to_string();
        self.phone.connect(self.now, &ticket).expect("the host's ticket can be dialed");
        self.step(8);
    }

    fn send(&mut self, frame: &Frame) {
        let bytes = encode(frame).expect("a frame to send");
        self.send_bytes(&bytes, frame.delivery());
    }

    fn send_bytes(&mut self, bytes: &[u8], delivery: Delivery) {
        let peer = self.peer.expect("dial first");
        self.phone.send(self.now, peer, bytes, delivery);
        self.step(4);
    }

    fn hello(&mut self) {
        self.send(&Frame::Hello(Hello {
            role: Role::Phone,
            protocol: 1,
            min_protocol: 1,
            app_version: "0.1.0".into(),
            name: "Test phone".into(),
            ticket: String::new(),
        }));
    }

    fn pair_in_settings(&mut self) {
        self.host.load_paired(vec![TrustRecord::phone(&self.phone_id, "Test phone")]);
    }

    /// A paired phone, dialed and said hello.
    fn active() -> World {
        let mut w = World::new();
        w.pair_in_settings();
        w.dial();
        w.hello();
        w.events.clear();
        w.frames.clear();
        w
    }

    fn take_events(&mut self) -> Vec<HostEvent> {
        std::mem::take(&mut self.events)
    }

    fn take_frames(&mut self) -> Vec<Frame> {
        std::mem::take(&mut self.frames)
    }

    fn secret_proof(&self, offer: &PairingOffer) -> [u8; 32] {
        proof(&offer.secret, &self.host_id, &self.phone_id)
    }
}

fn frames_named(frames: &[Frame], name: &str) -> usize {
    frames.iter().filter(|f| f.name() == name).count()
}

fn gesture(seq: u32, id: u32, phase: Phase, pan_x: f32, pan_y: f32, zoom: f32) -> Frame {
    Frame::Viewport(ViewportCmd { screen: ScreenId::MAIN, seq, op: ViewportOp::Gesture { id, phase, pan_x, pan_y, zoom, focus: Focus::Pen } })
}

fn gestures(events: &[HostEvent]) -> Vec<(u32, Phase, f32, f32, f32)> {
    events
        .iter()
        .filter_map(|e| match e {
            HostEvent::Viewport { op: ViewportOp::Gesture { id, phase, pan_x, pan_y, zoom, .. }, .. } => Some((*id, *phase, *pan_x, *pan_y, *zoom)),
            _ => None,
        })
        .collect()
}

fn pen(seq_hint: u32, line: u32) -> PenState {
    PenState {
        seq: seq_hint,
        folder: "azul".into(),
        surface: Surface::Page(PageSurface { file: "src/lib.rs".into(), page_index: 1, page_count: 4, line, line_count: 500, ..PageSurface::empty() }),
        thumb_id: 5,
        ..PenState::empty()
    }
}

fn thumb(id: u32) -> Thumbnail {
    Thumbnail { id, width: 2, height: 2, format: ThumbFormat::Rgba8, bytes: vec![255; 16] }
}

// ---- who may talk

#[test]
fn the_host_tells_its_ticket_when_the_endpoint_knows_its_address() {
    let net = MockNet::new();
    let mut host = CompanionHost::new(net.endpoint("mac"), config(), Box::new(SeededEntropy(1)));
    assert_eq!(host.ticket(), "");
    let events = host.poll(0);
    assert_eq!(events, vec![HostEvent::Ready { ticket: host.ticket().to_string() }]);
    assert!(host.ticket().starts_with("mt-"));
}

#[test]
fn a_phone_that_is_not_paired_is_refused_before_it_can_say_anything_when_no_pairing_is_open() {
    let mut w = World::new();
    w.dial();
    assert!(w.disconnected, "the host closed the connection");
    assert!(w.frames.iter().any(|f| matches!(f, Frame::Refused(Refusal::NotPaired))), "{:?}", w.frames);
    let diagnostics: Vec<_> = w.take_events().into_iter().filter(|e| matches!(e, HostEvent::Diagnostic(_))).collect();
    assert_eq!(diagnostics.len(), 1, "one line for the user: a phone that is not paired tried to connect");
    assert!(w.host.paired().is_empty());
}

#[test]
fn a_paired_phone_gets_the_hosts_hello_at_once_and_the_state_after_its_own() {
    let mut w = World::new();
    w.pair_in_settings();
    w.host.publish_pen(pen(0, 7));
    w.host.publish_thumbnail(thumb(5));
    w.dial();
    assert!(matches!(w.frames.first(), Some(Frame::Hello(h)) if h.role == Role::Host && h.name == "Studio Mac" && h.ticket == w.host.ticket()), "{:?}", w.frames);
    assert_eq!(frames_named(&w.frames, "buttons"), 0, "nothing before the phone has said hello");
    w.hello();
    w.step(40);
    let names: Vec<&str> = w.frames.iter().map(Frame::name).collect();
    assert!(names.contains(&"buttons") && names.contains(&"thumbnail") && names.contains(&"pen state"), "{names:?}");
    let events = w.take_events();
    assert!(events.contains(&HostEvent::PeerConnected { peer: w.phone_id.clone(), name: "Test phone".into(), role: Role::Phone }), "{events:?}");
}

#[test]
fn a_phone_too_old_is_told_to_update_the_companion_and_one_that_wants_more_is_told_to_update_azreview() {
    for (hello_protocol, min, expected) in [(0u16, 0u16, Refusal::UpdateCompanion), (9, 5, Refusal::UpdateAzReview)] {
        let mut w = World::new();
        w.pair_in_settings();
        w.dial();
        w.send(&Frame::Hello(Hello { role: Role::Phone, protocol: hello_protocol, min_protocol: min, app_version: "x".into(), name: "Old".into(), ticket: String::new() }));
        w.step(10);
        assert!(w.frames.contains(&Frame::Refused(expected)), "{expected:?}: {:?}", w.frames);
        assert!(w.disconnected);
    }
}

#[test]
fn a_connection_that_says_no_hello_is_dropped_after_the_timeout() {
    let mut w = World::new();
    w.pair_in_settings();
    w.dial();
    assert!(!w.disconnected);
    w.step(crate::limits::HELLO_TIMEOUT_MS + 50);
    assert!(w.disconnected);
}

#[test]
fn a_frame_before_the_hello_is_ignored_and_does_not_reach_azreview() {
    let mut w = World::new();
    w.pair_in_settings();
    w.dial();
    w.send(&Frame::Button { screen: ScreenId::MAIN, request_id: 1, index: 0 });
    w.send(&Frame::ListDir { request_id: 2, path: "src".into(), offset: 0 });
    assert!(!w.events.iter().any(|e| matches!(e, HostEvent::ButtonPressed { .. } | HostEvent::ListDir { .. })), "{:?}", w.events);
}

#[test]
fn bytes_that_are_no_frame_never_crash_and_a_peer_that_keeps_sending_them_is_dropped() {
    let mut w = World::active();
    for _ in 0..3 {
        w.send_bytes(&[0xFF, 0x00, 0x13], Delivery::Reliable);
    }
    assert!(!w.disconnected, "three are forgiven");
    for _ in 0..4 {
        w.send_bytes(&[0xA7; 40], Delivery::Reliable);
    }
    assert!(w.disconnected, "a peer that keeps sending garbage is gone");
    assert!(w.events.iter().any(|e| matches!(e, HostEvent::Diagnostic(d) if d.contains("not understood") || d.contains("bytes"))), "{:?}", w.events);
}

#[test]
fn a_frame_only_the_mac_may_send_is_refused_from_a_phone() {
    let mut w = World::active();
    w.send(&Frame::PenState(pen(1, 1)));
    w.send(&Frame::Notice(Notice { level: NoticeLevel::Info, text: "hi".into() }));
    let diagnostics = w.events.iter().filter(|e| matches!(e, HostEvent::Diagnostic(_))).count();
    assert!(diagnostics >= 2, "{:?}", w.events);
}

// ---- pairing

#[test]
fn an_offer_needs_the_endpoint_to_know_its_address_and_a_second_offer_replaces_the_first() {
    let net = MockNet::new();
    let mut host = CompanionHost::new(net.endpoint("mac"), config(), Box::new(SeededEntropy(3)));
    assert!(host.begin_pairing(0).is_none(), "no ticket yet");
    host.poll(0);
    let first = host.begin_pairing(10).expect("an offer");
    assert_eq!(first.ticket, host.ticket());
    assert!(host.pairing_open(11));
    let second = host.begin_pairing(20).expect("another offer");
    assert_ne!(first.secret, second.secret);
    assert!(PairingOffer::parse(&second.to_text()).is_ok(), "the offer can be shown as a code");
    host.cancel_pairing();
    assert!(!host.pairing_open(21));
}

#[test]
fn a_phone_that_proves_the_secret_is_asked_about_with_the_code_the_phone_shows_too() {
    let mut w = World::new();
    let offer = w.host.begin_pairing(w.now).unwrap();
    w.dial();
    w.hello();
    let phone_proof = w.secret_proof(&offer);
    w.send(&Frame::PairRequest { proof: phone_proof, phone_name: "Anna's phone".into() });
    let expected = confirmation_code(&offer.secret, &w.host_id, &w.phone_id);
    let events = w.take_events();
    assert!(events.contains(&HostEvent::PairingRequested { peer: w.phone_id.clone(), name: "Anna's phone".into(), code: expected }), "{events:?}");
    assert!(w.host.paired().is_empty(), "nothing is paired until the person says so");
}

#[test]
fn confirming_pairs_the_phone_spends_the_secret_and_tells_the_phone_the_code() {
    let mut w = World::new();
    let offer = w.host.begin_pairing(w.now).unwrap();
    w.dial();
    w.hello();
    w.send(&Frame::PairRequest { proof: w.secret_proof(&offer), phone_name: "Anna's phone".into() });
    w.take_events();
    w.host.accept_pairing(w.now, &w.phone_id.clone()).expect("confirmed");
    w.step(10);
    let code = confirmation_code(&offer.secret, &w.host_id, &w.phone_id);
    assert!(w.frames.contains(&Frame::PairAccepted { code }), "{:?}", w.frames);
    assert_eq!(w.host.paired(), [TrustRecord::phone(&w.phone_id, "Anna's phone")]);
    assert!(w.take_events().contains(&HostEvent::Paired { record: TrustRecord::phone(&w.phone_id, "Anna's phone") }));
    assert!(!w.host.pairing_open(w.now), "the secret is spent");
    assert!(w.frames.iter().any(|f| matches!(f, Frame::Buttons(_))), "the phone gets its state once it is paired: {:?}", w.frames);
}

#[test]
fn a_wrong_proof_is_rejected_and_the_third_closes_the_window() {
    let mut w = World::new();
    let offer = w.host.begin_pairing(w.now).unwrap();
    for attempt in 0..PAIRING_MAX_WRONG_PROOFS {
        w.dial();
        w.hello();
        w.send(&Frame::PairRequest { proof: [attempt as u8; 32], phone_name: "x".into() });
        w.step(5);
        assert!(w.frames.contains(&Frame::PairRejected(PairReject::WrongSecret)), "attempt {attempt}: {:?}", w.frames);
        w.take_frames();
        assert!(w.host.paired().is_empty());
    }
    assert!(!w.host.pairing_open(w.now), "the window closed after {PAIRING_MAX_WRONG_PROOFS} wrong proofs");
    // the right proof is no good any more
    w.dial();
    w.hello();
    w.send(&Frame::PairRequest { proof: w.secret_proof(&offer), phone_name: "x".into() });
    assert!(w.host.paired().is_empty());
}

#[test]
fn a_proof_made_for_another_phone_is_wrong() {
    let mut w = World::new();
    let offer = w.host.begin_pairing(w.now).unwrap();
    w.dial();
    w.hello();
    let someone_elses = proof(&offer.secret, &w.host_id, &"cd".repeat(32));
    w.send(&Frame::PairRequest { proof: someone_elses, phone_name: "thief".into() });
    assert!(w.frames.contains(&Frame::PairRejected(PairReject::WrongSecret)));
}

#[test]
fn a_pairing_window_that_is_over_closes_and_a_phone_that_asks_late_is_told_it_expired() {
    let mut w = World::new();
    let offer = w.host.begin_pairing(w.now).unwrap();
    w.dial();
    w.hello();
    w.now += PAIRING_WINDOW_MS + 10;
    w.send(&Frame::PairRequest { proof: w.secret_proof(&offer), phone_name: "late".into() });
    w.step(5);
    assert!(w.frames.iter().any(|f| matches!(f, Frame::PairRejected(PairReject::Expired | PairReject::NoPairingOpen))), "{:?}", w.frames);
    assert!(w.host.paired().is_empty());
}

#[test]
fn declining_tells_the_phone_and_pairs_nothing() {
    let mut w = World::new();
    let offer = w.host.begin_pairing(w.now).unwrap();
    w.dial();
    w.hello();
    w.send(&Frame::PairRequest { proof: w.secret_proof(&offer), phone_name: "x".into() });
    w.host.reject_pairing(w.now, &w.phone_id.clone());
    w.step(10);
    assert!(w.frames.contains(&Frame::PairRejected(PairReject::Declined)));
    assert!(w.host.paired().is_empty());
    assert_eq!(w.host.accept_pairing(w.now, &w.phone_id.clone()), Err(ConfirmError::NobodyWaiting));
}

#[test]
fn a_phone_that_waits_too_long_for_the_person_is_told_it_expired() {
    let mut w = World::new();
    w.auto_pong = true; // a phone that waits keeps answering
    let offer = w.host.begin_pairing(w.now).unwrap();
    w.dial();
    w.hello();
    w.send(&Frame::PairRequest { proof: w.secret_proof(&offer), phone_name: "x".into() });
    w.step(crate::limits::CONFIRM_TIMEOUT_MS + 50);
    assert!(w.frames.iter().any(|f| matches!(f, Frame::PairRejected(PairReject::Expired))), "{:?}", w.frames);
    assert_eq!(w.host.accept_pairing(w.now, &w.phone_id.clone()), Err(ConfirmError::NobodyWaiting));
}

#[test]
fn a_second_phone_while_one_waits_for_the_person_is_told_the_host_is_busy() {
    let mut w = World::new();
    let offer = w.host.begin_pairing(w.now).unwrap();
    w.dial();
    w.hello();
    w.send(&Frame::PairRequest { proof: w.secret_proof(&offer), phone_name: "first".into() });
    // a second phone, another key
    let mut second = w.net.endpoint("second phone");
    second.poll(w.now);
    let ticket = w.host.ticket().to_string();
    second.connect(w.now, &ticket).unwrap();
    let mut peer = None;
    let mut told = Vec::new();
    for _ in 0..12 {
        w.step(1);
        for e in second.poll(w.now) {
            if let LinkEvent::Connected { peer: p, .. } = e {
                peer = Some(p);
            }
            if let LinkEvent::Received { bytes, .. } = e {
                told.extend(decode(&bytes));
            }
        }
    }
    let peer = peer.expect("the second phone connected");
    let hello = encode(&Frame::Hello(Hello { role: Role::Phone, protocol: 1, min_protocol: 1, app_version: "x".into(), name: "second".into(), ticket: String::new() })).unwrap();
    second.send(w.now, peer, &hello, Delivery::Reliable);
    let request = encode(&Frame::PairRequest { proof: proof(&offer.secret, &w.host_id, &second.local_id()), phone_name: "second".into() }).unwrap();
    second.send(w.now, peer, &request, Delivery::Reliable);
    for _ in 0..12 {
        w.step(1);
        for e in second.poll(w.now) {
            if let LinkEvent::Received { bytes, .. } = e {
                told.extend(decode(&bytes));
            }
        }
    }
    assert!(told.contains(&Frame::PairRejected(PairReject::Busy)), "{told:?}");
}

#[test]
fn four_phones_are_the_most_and_a_fifth_is_told_so() {
    let mut w = World::new();
    let others: Vec<TrustRecord> = (0..MAX_PAIRED_PHONES).map(|i| TrustRecord::phone(&format!("{i:064x}"), &format!("phone {i}"))).collect();
    w.host.load_paired(others);
    let offer = w.host.begin_pairing(w.now).unwrap();
    w.dial();
    w.hello();
    w.send(&Frame::PairRequest { proof: w.secret_proof(&offer), phone_name: "fifth".into() });
    w.step(5);
    assert!(w.frames.contains(&Frame::PairRejected(PairReject::TooManyPhones)), "{:?}", w.frames);
    assert_eq!(w.host.paired().len(), MAX_PAIRED_PHONES);
}

#[test]
fn loading_more_phones_than_the_cap_keeps_the_first_four_and_says_so() {
    let net = MockNet::new();
    let mut host = CompanionHost::new(net.endpoint("mac"), config(), Box::new(SeededEntropy(1)));
    host.load_paired((0..9).map(|i| TrustRecord::phone(&format!("{i:064x}"), &format!("p{i}"))).collect());
    assert_eq!(host.paired().len(), MAX_PAIRED_PHONES);
    let events = host.poll(0);
    assert!(events.iter().any(|e| matches!(e, HostEvent::Diagnostic(d) if d.contains("paired"))), "{events:?}");
}

#[test]
fn the_secret_never_appears_in_an_event_or_a_diagnostic() {
    let mut w = World::new();
    let offer = w.host.begin_pairing(w.now).unwrap();
    w.dial();
    w.hello();
    w.send(&Frame::PairRequest { proof: [9; 32], phone_name: "x".into() });
    w.send(&Frame::PairRequest { proof: w.secret_proof(&offer), phone_name: "x".into() });
    let hex: String = offer.secret.iter().map(|b| format!("{b:02x}")).collect();
    let said = format!("{:?}", w.events);
    assert!(!said.contains(&hex), "{said}");
}

// ---- forgetting and reconnecting

#[test]
fn forgetting_a_phone_disconnects_it_and_it_is_refused_next_time() {
    let mut w = World::active();
    let id = w.phone_id.clone();
    w.host.forget(w.now, &id);
    w.step(10);
    assert!(w.disconnected);
    assert!(w.host.paired().is_empty());
    w.take_frames();
    w.dial();
    assert!(w.frames.iter().any(|f| matches!(f, Frame::Refused(Refusal::NotPaired))));
}

#[test]
fn a_phone_that_connects_again_replaces_its_old_connection() {
    let mut w = World::active();
    let before = w.host.phones().iter().filter(|p| p.connected).count();
    assert_eq!(before, 1);
    w.dial();
    w.hello();
    assert_eq!(w.host.phones().iter().filter(|p| p.connected).count(), 1, "one connection per phone");
}

#[test]
fn the_list_of_phones_says_who_is_here_and_how_slow_the_link_is() {
    let mut w = World::active();
    w.step(PING_INTERVAL_MS + 50);
    let pong = Frame::Pong { nonce: w.frames.iter().find_map(|f| if let Frame::Ping { nonce } = f { Some(*nonce) } else { None }).expect("a ping") };
    w.send(&pong);
    let phones = w.host.phones();
    assert_eq!(phones.len(), 1);
    assert!(phones[0].connected && phones[0].name == "Test phone");
    assert!(phones[0].rtt_ms.is_some(), "{phones:?}");
}

#[test]
fn a_phone_that_says_nothing_for_eight_seconds_is_gone_and_a_bye_ends_it_at_once() {
    let mut w = World::active();
    w.step(PEER_TIMEOUT_MS + 3_000);
    assert!(w.disconnected);
    assert!(w.take_events().iter().any(|e| matches!(e, HostEvent::PeerGone { reason, .. } if reason.contains("no answer") || reason.contains("silent"))));
    let mut w = World::active();
    w.send(&Frame::Bye);
    w.step(10);
    assert!(w.take_events().iter().any(|e| matches!(e, HostEvent::PeerGone { .. })));
}

// ---- what a paired phone asks

#[test]
fn a_gesture_comes_out_in_order_with_its_totals_and_the_same_numbers_again_are_dropped() {
    let mut w = World::active();
    w.send(&gesture(1, 7, Phase::Begin, 0.0, 0.0, 1.0));
    w.send(&gesture(2, 7, Phase::Update, 0.05, 0.1, 1.2));
    w.send(&gesture(3, 7, Phase::Update, 0.10, 0.1, 1.5));
    w.send(&gesture(4, 7, Phase::End, 0.12, 0.1, 1.5));
    let got = gestures(&w.events);
    let phases: Vec<Phase> = got.iter().map(|g| g.1).collect();
    assert_eq!(phases, [Phase::Begin, Phase::Update, Phase::Update, Phase::End]);
    assert_eq!(got[3], (7, Phase::End, 0.12, 0.1, 1.5));
    w.take_events();
    w.send(&gesture(3, 7, Phase::Update, 0.10, 0.1, 1.5));
    assert!(gestures(&w.events).is_empty(), "an older frame that came late");
}

#[test]
fn an_update_before_its_begin_gets_a_begin_made_for_it_and_an_update_after_the_end_is_ignored() {
    let mut w = World::active();
    w.send(&gesture(5, 9, Phase::Update, 0.1, 0.0, 1.0));
    let phases: Vec<Phase> = gestures(&w.events).iter().map(|g| g.1).collect();
    assert_eq!(phases, [Phase::Begin, Phase::Update], "the first sight of a gesture starts it");
    w.send(&gesture(6, 9, Phase::End, 0.2, 0.0, 1.0));
    w.take_events();
    w.send(&gesture(7, 9, Phase::Update, 0.3, 0.0, 1.0));
    assert!(gestures(&w.events).is_empty(), "the gesture is over");
}

#[test]
fn when_the_last_update_is_dropped_the_end_still_carries_the_totals() {
    let mut w = World::active();
    w.send(&gesture(1, 1, Phase::Begin, 0.0, 0.0, 1.0));
    // updates 2 and 3 never arrive; the end says where it got to
    w.send(&gesture(4, 1, Phase::End, 0.4, -0.2, 2.0));
    let last = gestures(&w.events).pop().expect("an end");
    assert_eq!(last, (1, Phase::End, 0.4, -0.2, 2.0));
}

#[test]
fn a_new_gesture_ends_the_one_before_it_at_its_last_totals() {
    let mut w = World::active();
    w.send(&gesture(1, 1, Phase::Begin, 0.0, 0.0, 1.0));
    w.send(&gesture(2, 1, Phase::Update, 0.2, 0.0, 1.0));
    w.take_events();
    w.send(&gesture(3, 2, Phase::Begin, 0.0, 0.0, 1.0));
    let got = gestures(&w.events);
    assert_eq!(got[0], (1, Phase::End, 0.2, 0.0, 1.0), "{got:?}");
    assert_eq!(got[1].1, Phase::Begin);
    assert_eq!(got[1].0, 2);
}

#[test]
fn a_gesture_that_goes_quiet_is_ended_by_the_host() {
    let mut w = World::active();
    w.send(&gesture(1, 3, Phase::Begin, 0.0, 0.0, 1.0));
    w.send(&gesture(2, 3, Phase::Update, 0.3, 0.1, 1.4));
    w.take_events();
    w.step(GESTURE_QUIET_MS + 100);
    let got = gestures(&w.events);
    assert_eq!(got, [(3, Phase::End, 0.3, 0.1, 1.4)], "{got:?}");
}

#[test]
fn totals_over_the_caps_are_held_to_them_and_said() {
    let mut w = World::active();
    w.send(&gesture(1, 1, Phase::Begin, 0.0, 0.0, 1.0));
    w.send(&gesture(2, 1, Phase::Update, 500.0, -500.0, 900.0));
    let last = gestures(&w.events).pop().unwrap();
    assert_eq!((last.2, last.3, last.4), (crate::limits::MAX_GESTURE_PAN, -crate::limits::MAX_GESTURE_PAN, crate::limits::MAX_GESTURE_ZOOM));
    assert!(w.events.iter().any(|e| matches!(e, HostEvent::Diagnostic(_))), "{:?}", w.events);
}

#[test]
fn fit_reset_and_the_pages_come_through_and_a_page_step_is_held_to_a_few_pages() {
    let mut w = World::active();
    for (seq, op) in [(1, ViewportOp::Fit), (2, ViewportOp::Reset), (3, ViewportOp::PageStep(1)), (4, ViewportOp::PageStep(-1)), (5, ViewportOp::PageStep(120))] {
        w.send(&Frame::Viewport(ViewportCmd { screen: ScreenId::MAIN, seq, op }));
    }
    let ops: Vec<ViewportOp> = w.events.iter().filter_map(|e| if let HostEvent::Viewport { op, .. } = e { Some(*op) } else { None }).collect();
    assert_eq!(ops, [ViewportOp::Fit, ViewportOp::Reset, ViewportOp::PageStep(1), ViewportOp::PageStep(-1), ViewportOp::PageStep(8)]);
}

#[test]
fn a_command_with_an_older_number_than_one_already_taken_is_dropped_so_a_page_is_not_turned_twice() {
    let mut w = World::active();
    w.send(&Frame::Viewport(ViewportCmd { screen: ScreenId::MAIN, seq: 10, op: ViewportOp::PageStep(1) }));
    w.send(&Frame::Viewport(ViewportCmd { screen: ScreenId::MAIN, seq: 9, op: ViewportOp::PageStep(1) }));
    w.send(&Frame::Viewport(ViewportCmd { screen: ScreenId::MAIN, seq: 10, op: ViewportOp::PageStep(1) }));
    let steps = w.events.iter().filter(|e| matches!(e, HostEvent::Viewport { op: ViewportOp::PageStep(_), .. })).count();
    assert_eq!(steps, 1);
}

#[test]
fn the_numbers_of_a_phone_that_wrap_round_are_still_in_order() {
    let mut w = World::active();
    w.send(&Frame::Viewport(ViewportCmd { screen: ScreenId::MAIN, seq: u32::MAX, op: ViewportOp::Fit }));
    w.send(&Frame::Viewport(ViewportCmd { screen: ScreenId::MAIN, seq: 0, op: ViewportOp::Reset }));
    let ops: Vec<ViewportOp> = w.events.iter().filter_map(|e| if let HostEvent::Viewport { op, .. } = e { Some(*op) } else { None }).collect();
    assert_eq!(ops, [ViewportOp::Fit, ViewportOp::Reset]);
}

#[test]
fn a_folder_and_a_file_come_through_when_the_path_is_safe_and_are_refused_when_it_is_not() {
    let mut w = World::active();
    w.send(&Frame::ListDir { request_id: 4, path: "layout/src".into(), offset: 500 });
    w.send(&Frame::OpenFile { screen: ScreenId::MAIN, request_id: 5, path: "layout/src/window.rs".into(), line: Some(42) });
    let events = w.take_events();
    assert!(events.contains(&HostEvent::ListDir { peer: w.phone_id.clone(), request_id: 4, path: "layout/src".into(), offset: 500 }), "{events:?}");
    assert!(events.contains(&HostEvent::OpenFile { peer: w.phone_id.clone(), screen: ScreenId::MAIN, request_id: 5, path: "layout/src/window.rs".into(), line: Some(42) }));
    for bad in ["../secrets", "/etc/passwd", "a/../../b", "a\\b"] {
        w.send(&Frame::ListDir { request_id: 6, path: bad.into(), offset: 0 });
        w.send(&Frame::OpenFile { screen: ScreenId::MAIN, request_id: 7, path: bad.into(), line: None });
    }
    let events = w.take_events();
    assert!(!events.iter().any(|e| matches!(e, HostEvent::ListDir { .. } | HostEvent::OpenFile { .. })), "{events:?}");
    assert!(events.iter().filter(|e| matches!(e, HostEvent::Diagnostic(_))).count() >= 4, "each refusal is said");
}

#[test]
fn rows_go_to_the_phone_that_asked() {
    let mut w = World::active();
    w.host.publish_rows(w.now, &w.phone_id.clone(), Rows { request_id: 4, dir: "src".into(), offset: 0, entries: vec![Entry { name: "lib.rs".into(), kind: EntryKind::File, state: FileState::Clean }], more: false });
    w.step(10);
    let rows = w.frames.iter().find_map(|f| if let Frame::Rows(r) = f { Some(r.clone()) } else { None }).expect("rows");
    assert_eq!((rows.request_id, rows.dir.as_str(), rows.entries.len()), (4, "src", 1));
}

#[test]
fn rows_for_a_phone_that_is_not_here_go_nowhere_and_never_crash() {
    let mut w = World::new();
    w.host.publish_rows(w.now, &"ef".repeat(32), Rows { request_id: 1, dir: String::new(), offset: 0, entries: Vec::new(), more: false });
    w.step(10);
    assert!(w.frames.is_empty());
}

// ---- the quick buttons

#[test]
fn a_button_press_gives_azreview_the_text_and_the_phone_a_received_reply() {
    let mut w = World::active();
    w.send(&Frame::Button { screen: ScreenId::MAIN, request_id: 21, index: 2 });
    let events = w.take_events();
    let defaults = ButtonSet::defaults();
    assert!(
        events.contains(&HostEvent::ButtonPressed {
            peer: w.phone_id.clone(),
            screen: ScreenId::MAIN,
            index: 2,
            label: defaults.buttons[2].label.clone(),
            text: defaults.buttons[2].text.clone(),
            request_id: 21
        }),
        "{events:?}"
    );
    w.step(10);
    assert!(w.frames.contains(&Frame::Reply(Reply { request_id: 21, status: ReplyStatus::Received })), "{:?}", w.frames);
}

#[test]
fn a_phone_cannot_press_more_than_eight_buttons_a_second_and_a_repeated_request_is_taken_once() {
    let mut w = World::active();
    for i in 0..30u32 {
        w.send(&Frame::Button { screen: ScreenId::MAIN, request_id: 100 + i, index: (i % 4) as u8 });
    }
    let pressed = w.events.iter().filter(|e| matches!(e, HostEvent::ButtonPressed { .. })).count();
    assert!(pressed <= crate::limits::BUTTONS_PER_SECOND + 1, "{pressed}");
    assert!(w.events.iter().any(|e| matches!(e, HostEvent::Diagnostic(d) if d.contains("button"))), "the user is told");
    let mut w = World::active();
    w.send(&Frame::Button { screen: ScreenId::MAIN, request_id: 5, index: 0 });
    w.step(300);
    w.send(&Frame::Button { screen: ScreenId::MAIN, request_id: 5, index: 0 });
    let pressed = w.events.iter().filter(|e| matches!(e, HostEvent::ButtonPressed { .. })).count();
    assert_eq!(pressed, 1, "the same request is the same press");
}

#[test]
fn buttons_a_phone_saves_are_cut_kept_sent_to_every_phone_and_answered_done() {
    let mut w = World::active();
    let mut set = ButtonSet::defaults();
    set.buttons[0].label = "A label that is far too long".into();
    set.buttons[1].text = "x".repeat(MAX_BUTTON_TEXT_CHARS_FOR_TEST);
    w.send(&Frame::SetButtons { request_id: 31, buttons: ButtonSet { buttons: [QuickButton { label: "Short".into(), text: set.buttons[0].text.clone() }, set.buttons[1].clone(), set.buttons[2].clone(), set.buttons[3].clone()] } });
    let events = w.take_events();
    let changed = events.iter().find_map(|e| if let HostEvent::ButtonsChanged { buttons } = e { Some(buttons.clone()) } else { None }).expect("changed");
    assert_eq!(changed.buttons[0].label, "Short");
    assert_eq!(w.host.buttons(), &changed);
    w.step(10);
    assert!(w.frames.iter().any(|f| matches!(f, Frame::Buttons(b) if *b == changed)), "sent to the phone that is here");
    assert!(w.frames.contains(&Frame::Reply(Reply { request_id: 31, status: ReplyStatus::Done })));
}

const MAX_BUTTON_TEXT_CHARS_FOR_TEST: usize = 280;

#[test]
fn the_macs_own_change_of_the_buttons_is_cut_to_the_caps_and_reaches_every_phone() {
    let mut w = World::active();
    let mut set = ButtonSet::defaults();
    set.buttons[3].label = "Thisiswaytoolong".into();
    let cut = w.host.set_buttons(w.now, &set);
    assert_eq!(cut.len(), 1);
    assert_eq!(w.host.buttons().buttons[3].label.chars().count(), crate::limits::MAX_LABEL_CHARS);
    w.step(10);
    assert!(w.frames.iter().any(|f| matches!(f, Frame::Buttons(_))));
    let _ = BUTTON_COUNT;
}

// ---- what goes out

#[test]
fn the_pen_state_goes_out_at_most_thirty_times_a_second_the_newest_wins_and_it_counts_up() {
    let mut w = World::active();
    w.host.publish_pen(pen(0, 1));
    w.step(2);
    for line in 2..=200u32 {
        w.host.publish_pen(pen(0, line));
        w.step(1);
    }
    w.step(60);
    let states: Vec<PenState> = w.frames.iter().filter_map(|f| if let Frame::PenState(p) = f { Some(p.clone()) } else { None }).collect();
    assert!((5..=9).contains(&states.len()), "{} states in about 260 ms at 30 a second", states.len());
    assert_eq!(states.last().unwrap().page().map(|p| p.line), Some(200), "the newest wins");
    assert!(states.windows(2).all(|p| p[1].seq.wrapping_sub(p[0].seq) as i32 > 0), "the number counts up");
}

#[test]
fn a_pen_state_that_did_not_change_is_not_sent_again() {
    let mut w = World::active();
    w.host.publish_pen(pen(0, 5));
    w.step(50);
    let first = frames_named(&w.frames, "pen state");
    assert_eq!(first, 1);
    for _ in 0..10 {
        w.host.publish_pen(pen(0, 5));
        w.step(40);
    }
    assert_eq!(frames_named(&w.frames, "pen state"), 1, "the same state ten times is one state");
}

#[test]
fn the_picture_goes_once_to_each_phone_and_again_when_it_asks() {
    let mut w = World::active();
    w.host.publish_thumbnail(thumb(5));
    w.step(10);
    assert_eq!(frames_named(&w.frames, "thumbnail"), 1);
    w.host.publish_thumbnail(thumb(5));
    w.step(10);
    assert_eq!(frames_named(&w.frames, "thumbnail"), 1, "the same picture is not sent again");
    w.send(&Frame::NeedThumbnail { id: 5 });
    w.step(10);
    assert_eq!(frames_named(&w.frames, "thumbnail"), 2, "it asked");
    w.host.publish_thumbnail(thumb(6));
    w.step(10);
    assert_eq!(frames_named(&w.frames, "thumbnail"), 3, "a new picture");
}

#[test]
fn a_reply_goes_to_the_phone_named_and_a_notice_to_every_phone_cut_to_its_cap() {
    let mut w = World::active();
    w.host.reply(w.now, &w.phone_id.clone(), 21, ReplyStatus::Working);
    w.host.reply(w.now, &"ab".repeat(32), 21, ReplyStatus::Done);
    w.host.notice(w.now, Notice { level: NoticeLevel::Warning, text: "n".repeat(crate::limits::MAX_NOTICE_BYTES + 100) });
    w.step(10);
    assert_eq!(w.frames.iter().filter(|f| matches!(f, Frame::Reply(_))).count(), 1, "only the phone named hears the reply");
    let notice = w.frames.iter().find_map(|f| if let Frame::Notice(n) = f { Some(n.clone()) } else { None }).expect("a notice");
    assert_eq!(notice.text.len(), crate::limits::MAX_NOTICE_BYTES);
}

#[test]
fn nothing_is_sent_to_a_phone_that_has_not_said_hello() {
    let mut w = World::new();
    w.pair_in_settings();
    w.host.publish_pen(pen(0, 3));
    w.host.publish_thumbnail(thumb(5));
    w.dial();
    w.step(100);
    assert_eq!(frames_named(&w.frames, "pen state") + frames_named(&w.frames, "thumbnail") + frames_named(&w.frames, "buttons"), 0, "{:?}", w.frames);
}

#[test]
fn a_slow_link_drops_pen_states_and_keeps_the_newest() {
    let mut w = World::active();
    w.net.set_knobs(Knobs { latency_ms: 1, bytes_per_second: Some(2_000), ..Knobs::default() });
    for line in 1..=100u32 {
        w.host.publish_pen(pen(0, line));
        w.step(34);
    }
    w.step(2_000);
    let states: Vec<PenState> = w.frames.iter().filter_map(|f| if let Frame::PenState(p) = f { Some(p.clone()) } else { None }).collect();
    assert!(states.len() < 100, "a slow link drops what is in between: {}", states.len());
    assert_eq!(states.last().unwrap().page().map(|p| p.line), Some(100), "and ends on the newest");
}

#[test]
fn the_context_of_the_pen_state_is_sent_as_it_is() {
    let mut w = World::active();
    let mut state = pen(0, 9);
    state.context = Context::Region;
    state.view = Rect { x: 0.1, y: 0.2, w: 0.3, h: 0.4 };
    w.host.publish_pen(state);
    w.step(50);
    let got = w.frames.iter().find_map(|f| if let Frame::PenState(p) = f { Some(p.clone()) } else { None }).expect("a state");
    assert_eq!(got.context, Context::Region);
    assert_eq!(got.view, Rect { x: 0.1, y: 0.2, w: 0.3, h: 0.4 });
}

// ---- peers, screens and what a screen may be commanded by

fn collaborator_world() -> World {
    let mut w = World::new();
    w.host.load_paired(vec![TrustRecord { peer: w.phone_id.clone(), name: "Anna".into(), kind: PeerKind::Person, group: TrustGroup::Collaborator }]);
    w.dial();
    w.hello();
    w.events.clear();
    w.frames.clear();
    w
}

#[test]
fn a_collaborator_may_look_but_not_command_the_main_screen_the_buttons_or_the_return_point() {
    let mut w = collaborator_world();
    w.send(&gesture(1, 1, Phase::Begin, 0.0, 0.0, 1.0));
    w.send(&Frame::Viewport(ViewportCmd { screen: ScreenId::MAIN, seq: 2, op: ViewportOp::Fit }));
    w.send(&Frame::Button { screen: ScreenId::MAIN, request_id: 5, index: 0 });
    w.send(&Frame::OpenFile { screen: ScreenId::MAIN, request_id: 6, path: "a.rs".into(), line: None });
    w.send(&Frame::ContinueReviewing { screen: ScreenId::MAIN });
    w.send(&Frame::SetButtons { request_id: 7, buttons: ButtonSet::defaults() });
    let events = w.take_events();
    assert!(
        !events.iter().any(|e| matches!(
            e,
            HostEvent::Viewport { .. } | HostEvent::ButtonPressed { .. } | HostEvent::OpenFile { .. } | HostEvent::ContinueReviewing { .. } | HostEvent::ButtonsChanged { .. }
        )),
        "{events:?}"
    );
    let said = events.iter().filter(|e| matches!(e, HostEvent::Diagnostic(d) if d.contains("may not"))).count();
    assert!(said >= 5, "each refusal is said: {events:?}");
    // it may still ask for a folder: the workspace's names are not a command
    w.send(&Frame::ListDir { request_id: 8, path: String::new(), offset: 0 });
    assert!(w.take_events().iter().any(|e| matches!(e, HostEvent::ListDir { .. })));
}

#[test]
fn a_collaborator_that_was_given_a_screen_may_command_that_one() {
    let mut w = collaborator_world();
    let given = ScreenInfo { id: ScreenId(3), owner: w.host_id.clone(), controller: w.phone_id.clone(), role: Role::ReviewMirror };
    let main = ScreenInfo { id: ScreenId::MAIN, owner: w.host_id.clone(), controller: w.host_id.clone(), role: Role::Review };
    w.host.set_screens(vec![main, given]);
    w.send(&Frame::Viewport(ViewportCmd { screen: ScreenId(3), seq: 1, op: ViewportOp::Fit }));
    w.send(&Frame::Viewport(ViewportCmd { screen: ScreenId::MAIN, seq: 2, op: ViewportOp::Fit }));
    let screens: Vec<ScreenId> = w.events.iter().filter_map(|e| if let HostEvent::Viewport { screen, .. } = e { Some(*screen) } else { None }).collect();
    assert_eq!(screens, [ScreenId(3)], "the screen it controls, not the other");
}

#[test]
fn a_command_for_a_screen_that_does_not_exist_is_refused_and_said() {
    let mut w = World::active();
    w.send(&Frame::Viewport(ViewportCmd { screen: ScreenId(9), seq: 1, op: ViewportOp::Fit }));
    w.send(&Frame::ContinueReviewing { screen: ScreenId(9) });
    let events = w.take_events();
    assert!(!events.iter().any(|e| matches!(e, HostEvent::Viewport { .. } | HostEvent::ContinueReviewing { .. })), "{events:?}");
    assert!(events.iter().any(|e| matches!(e, HostEvent::Diagnostic(d) if d.contains("no such screen"))), "{events:?}");
}

#[test]
fn a_device_of_the_users_own_group_commands_every_screen_of_the_node() {
    let mut w = World::active();
    let extra = ScreenInfo { id: ScreenId(2), owner: w.host_id.clone(), controller: "someone-else".into(), role: Role::Whiteboard };
    let main = ScreenInfo { id: ScreenId::MAIN, owner: w.host_id.clone(), controller: w.host_id.clone(), role: Role::Review };
    w.host.set_screens(vec![main, extra]);
    w.send(&Frame::ContinueReviewing { screen: ScreenId(2) });
    w.send(&Frame::ContinueReviewing { screen: ScreenId::MAIN });
    let events = w.take_events();
    assert!(events.contains(&HostEvent::ContinueReviewing { peer: w.phone_id.clone(), screen: ScreenId(2) }), "{events:?}");
    assert!(events.contains(&HostEvent::ContinueReviewing { peer: w.phone_id.clone(), screen: ScreenId::MAIN }));
}

#[test]
fn a_peer_that_says_it_is_something_else_than_a_phone_is_refused_for_its_role() {
    let mut w = World::new();
    w.pair_in_settings();
    w.dial();
    w.send(&Frame::Hello(Hello { role: Role::Review, protocol: 1, min_protocol: 1, app_version: "x".into(), name: "A window".into(), ticket: String::new() }));
    w.step(10);
    assert!(w.frames.contains(&Frame::Refused(Refusal::UnsupportedRole)), "{:?}", w.frames);
    assert!(w.disconnected);
    assert!(!w.events.iter().any(|e| matches!(e, HostEvent::PeerConnected { .. })));
}

// ---- what the Mac settles and what finishes

#[test]
fn the_settings_reach_a_phone_when_it_is_served_and_every_phone_when_they_change() {
    let mut w = World::new();
    w.host.set_settings(w.now, HostSettings { reverse_pan: true, ..HostSettings::default() });
    w.pair_in_settings();
    w.dial();
    w.hello();
    w.step(20);
    let first = w.frames.iter().find_map(|f| if let Frame::Settings(s) = f { Some(*s) } else { None }).expect("the settings come with the state");
    assert!(first.reverse_pan);
    w.frames.clear();
    let changed = HostSettings { reverse_pan: false, snap_to_page_top: true, ..HostSettings::default() };
    w.host.set_settings(w.now, changed);
    w.host.set_settings(w.now, changed); // the same again is not news
    w.step(10);
    assert_eq!(w.frames.iter().filter(|f| matches!(f, Frame::Settings(_))).count(), 1);
    assert_eq!(w.host.settings(), changed);
}

#[test]
fn an_agent_that_finished_is_told_to_every_phone_with_its_file_cut_to_the_cap() {
    let mut w = World::active();
    w.host.agent_finished(w.now, AgentFinished { conversation_id: 4, file: "f".repeat(MAX_PATH_BYTES + 50), line: 12, kind: FinishKind::Failed });
    w.step(10);
    let done = w.frames.iter().find_map(|f| if let Frame::AgentFinished(a) = f { Some(a.clone()) } else { None }).expect("told");
    assert_eq!((done.conversation_id, done.line, done.kind), (4, 12, FinishKind::Failed));
    assert_eq!(done.file.len(), MAX_PATH_BYTES);
}

#[test]
fn nothing_is_told_to_a_phone_that_is_not_served_yet() {
    let mut w = World::new();
    w.pair_in_settings();
    w.dial(); // connected, no hello yet
    w.frames.clear();
    w.host.agent_finished(w.now, AgentFinished { conversation_id: 1, file: "a".into(), line: 1, kind: FinishKind::Done });
    w.host.set_settings(w.now, HostSettings { reverse_pan: true, ..HostSettings::default() });
    w.host.notice(w.now, Notice { level: NoticeLevel::Info, text: "x".into() });
    w.step(20);
    assert!(w.frames.is_empty(), "{:?}", w.frames);
}
