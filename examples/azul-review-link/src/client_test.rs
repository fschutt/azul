//! The phone's client: how it pairs, how it stays connected and finds the Mac again, what it sends and how often, what it does
//! with what the Mac sends. Two kinds of test. The `Rig` plays the Mac by hand over a scripted link (every frame the client writes
//! is seen, every answer is chosen): the protocol, the failures, the timings. The `Duo` puts the client and the real host on the
//! mock network: what the two of them make of each other, end to end.

use std::sync::{Arc, Mutex, MutexGuard};

use super::*;
use crate::access::ScreenId;
use crate::discovery::{MockDiscovery, Nearby};
use crate::frame::{Entry, EntryKind, FileState, FinishKind, HostSettings, PenState, QuickButton, Rows, Surface, ThumbFormat, Thumbnail};
use crate::host::{CompanionHost, HostConfig, HostEvent};
use crate::limits::{MAX_BUTTON_TEXT_CHARS, MAX_LABEL_CHARS, PAIRING_WINDOW_MS};
use crate::link::{Delivery, LinkStats};
use crate::mock::{Knobs, MockLink, MockNet};
use crate::pairing::SeededEntropy;
use crate::store::MemoryStore;

const MAC_ID: &str = "ab12ab12ab12ab12ab12ab12ab12ab12";
const PHONE_ID: &str = "cd34cd34cd34cd34cd34cd34cd34cd34";
const MAC_TICKET: &str = "mac-ticket-1";

// ---- a link played by hand

#[derive(Default)]
struct Script {
    connects: Vec<String>,
    connect_error: Option<String>,
    sent: Vec<(PeerHandle, Frame, Delivery)>,
    disconnected: Vec<PeerHandle>,
    inbox: Vec<LinkEvent>,
    closed: bool,
}

/// A link a test plays by hand: it records what the client does and says what the test makes it say. Clones share it.
#[derive(Clone, Default)]
struct ScriptLink(Arc<Mutex<Script>>);

impl ScriptLink {
    fn lock(&self) -> MutexGuard<'_, Script> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn say(&self, event: LinkEvent) {
        self.lock().inbox.push(event);
    }

    fn frame(&self, peer: PeerHandle, frame: &Frame) {
        let bytes = encode(frame).expect("a frame");
        self.say(LinkEvent::Received { peer, bytes, delivery: frame.delivery() });
    }

    fn noise(&self, peer: PeerHandle, bytes: &[u8]) {
        self.say(LinkEvent::Received { peer, bytes: bytes.to_vec(), delivery: Delivery::Reliable });
    }

    fn sent(&self) -> Vec<Frame> {
        self.lock().sent.iter().map(|(_, frame, _)| frame.clone()).collect()
    }

    fn take_sent(&self) -> Vec<Frame> {
        std::mem::take(&mut self.lock().sent).into_iter().map(|(_, frame, _)| frame).collect()
    }

    fn connects(&self) -> Vec<String> {
        self.lock().connects.clone()
    }

    fn disconnected(&self) -> Vec<PeerHandle> {
        self.lock().disconnected.clone()
    }
}

impl Link for ScriptLink {
    fn local_id(&self) -> String {
        PHONE_ID.to_string()
    }

    fn ticket(&self) -> String {
        String::new()
    }

    fn connect(&mut self, _now: Millis, ticket: &str) -> Result<(), String> {
        let mut script = self.lock();
        if let Some(error) = script.connect_error.take() {
            return Err(error);
        }
        script.connects.push(ticket.to_string());
        Ok(())
    }

    fn send(&mut self, _now: Millis, peer: PeerHandle, bytes: &[u8], delivery: Delivery) -> bool {
        let frame = decode(bytes).expect("the client writes frames that decode");
        self.lock().sent.push((peer, frame, delivery));
        true
    }

    fn disconnect(&mut self, peer: PeerHandle) {
        self.lock().disconnected.push(peer);
    }

    fn poll(&mut self, _now: Millis) -> Vec<LinkEvent> {
        std::mem::take(&mut self.lock().inbox)
    }

    fn stats(&self, _peer: PeerHandle) -> LinkStats {
        LinkStats::default()
    }

    fn close(&mut self) {
        self.lock().closed = true;
    }
}

/// A phone client over a scripted link, with a clock.
struct Rig {
    link: ScriptLink,
    store: MemoryStore,
    discovery: MockDiscovery,
    phone: PhoneClient<ScriptLink>,
    now: Millis,
}

fn config() -> ClientConfig {
    ClientConfig { phone_name: "Test phone".into(), app_version: "0.1.0".into() }
}

fn saved_mac() -> Stored {
    Stored { secret_key: Some([5; 32]), mac: Some(SavedMac { mac_id: MAC_ID.into(), name: "Studio Mac".into(), tickets: vec![MAC_TICKET.into()] }) }
}

fn mac_hello() -> Frame {
    Frame::Hello(Hello { role: Role::Host, protocol: 1, min_protocol: 1, app_version: "0.1.0".into(), name: "Studio Mac".into(), ticket: String::new() })
}

fn offer() -> PairingOffer {
    PairingOffer { ticket: MAC_TICKET.to_string(), secret: [9; 16] }
}

impl Rig {
    fn with_store(store: MemoryStore) -> Rig {
        let link = ScriptLink::default();
        let discovery = MockDiscovery::new();
        let phone = PhoneClient::new(link.clone(), config(), Box::new(store.clone())).with_discovery(Box::new(discovery.clone()));
        Rig { link, store, discovery, phone, now: 1_000 }
    }

    /// A phone that knows no Mac.
    fn unpaired() -> Rig {
        Rig::with_store(MemoryStore::new())
    }

    /// A phone that is paired with the Mac and has not dialed yet.
    fn saved() -> Rig {
        Rig::with_store(MemoryStore::with(saved_mac()))
    }

    fn poll(&mut self) -> bool {
        self.phone.poll(self.now)
    }

    fn advance(&mut self, ms: u64) {
        let end = self.now + ms;
        while self.now < end {
            self.now += 1;
            self.phone.poll(self.now);
        }
    }

    /// Time passes and the Mac is heard from every second (it pings), so the link is not taken for dead.
    fn advance_alive(&mut self, ms: u64) {
        for _ in 0..ms / 1_000 {
            self.link.frame(1, &Frame::Ping { nonce: 1 });
            self.advance(1_000);
        }
        self.advance(ms % 1_000);
    }

    fn session(&self) -> Session {
        self.phone.view().session.clone()
    }

    fn failed(&self) -> Option<FailureKind> {
        match &self.phone.view().session {
            Session::Failed { kind } => Some(kind.clone()),
            _ => None,
        }
    }

    /// The phone has dialed and the transport says it is connected to `id`.
    fn connect_as(&mut self, peer: PeerHandle, id: &str) {
        self.poll();
        self.link.say(LinkEvent::Connected { peer, id: id.to_string() });
        self.poll();
    }

    /// The Mac serves the phone (a paired phone: dial, connect, hello). Everything sent so far is forgotten.
    fn active() -> Rig {
        let mut rig = Rig::saved();
        rig.connect_as(1, MAC_ID);
        rig.link.frame(1, &mac_hello());
        rig.poll();
        assert!(rig.phone.is_active(), "{:?}", rig.session());
        rig.link.take_sent();
        rig
    }

    /// A phone that scanned the offer and is waiting for the person at the Mac: the proof is out.
    fn pairing() -> Rig {
        let mut rig = Rig::unpaired();
        rig.phone.begin_pairing(rig.now, &offer().to_text()).expect("an offer");
        rig.connect_as(1, MAC_ID);
        rig.link.frame(1, &mac_hello());
        rig.poll();
        assert!(matches!(rig.session(), Session::Pairing { .. }), "{:?}", rig.session());
        rig
    }

    fn pen(&self, seq: u32, thumb_id: u32) -> PenState {
        PenState { seq, thumb_id, ..PenState::empty() }
    }

    fn pairing_code(&self) -> u32 {
        confirmation_code(&offer().secret, MAC_ID, PHONE_ID)
    }
}

fn count(frames: &[Frame], name: &str) -> usize {
    frames.iter().filter(|f| f.name() == name).count()
}

// ---- the Mac and the phone on the mock network

struct Duo {
    net: MockNet,
    host: CompanionHost<MockLink>,
    phone: PhoneClient<MockLink>,
    store: MemoryStore,
    discovery: MockDiscovery,
    events: Vec<HostEvent>,
    now: Millis,
}

fn host_config() -> HostConfig {
    HostConfig { mac_name: "Studio Mac".into(), app_version: "0.1.0".into(), max_phones: 4 }
}

impl Duo {
    fn new() -> Duo {
        Duo::with_store(MemoryStore::new())
    }

    fn with_store(store: MemoryStore) -> Duo {
        let net = MockNet::with_knobs(Knobs { latency_ms: 1, ..Knobs::default() });
        let host = CompanionHost::new(net.endpoint("mac"), host_config(), Box::new(SeededEntropy(7)));
        let discovery = MockDiscovery::new();
        let phone = PhoneClient::new(net.endpoint("phone"), config(), Box::new(store.clone())).with_discovery(Box::new(discovery.clone()));
        let mut duo = Duo { net, host, phone, store, discovery, events: Vec::new(), now: 1_000 };
        duo.step(5);
        duo.events.clear();
        duo
    }

    fn step(&mut self, ms: u64) {
        for _ in 0..ms {
            self.now += 1;
            self.events.extend(self.host.poll(self.now));
            self.phone.poll(self.now);
        }
    }

    /// A long wait, in coarser steps.
    fn wait(&mut self, ms: u64) {
        for _ in 0..ms / 10 {
            self.now += 10;
            self.events.extend(self.host.poll(self.now));
            self.phone.poll(self.now);
        }
    }

    fn host_key() -> String {
        MockNet::key("mac")
    }

    fn phone_key() -> String {
        MockNet::key("phone")
    }

    /// The whole pairing, the way a person does it: the Mac shows the offer, the phone takes it, the person says yes.
    fn pair(&mut self) {
        let offer = self.host.begin_pairing(self.now).expect("the host knows its address");
        self.phone.begin_pairing(self.now, &offer.to_text()).expect("an offer");
        self.step(40);
        let peer = self
            .events
            .iter()
            .find_map(|e| if let HostEvent::PairingRequested { peer, .. } = e { Some(peer.clone()) } else { None })
            .expect("the phone asked to be paired");
        self.host.accept_pairing(self.now, &peer).expect("confirmed");
        self.step(40);
        assert!(self.phone.is_active(), "{:?}", self.phone.view().session);
    }

    fn paired() -> Duo {
        let mut duo = Duo::new();
        duo.pair();
        duo.events.clear();
        duo
    }

    fn take_events(&mut self) -> Vec<HostEvent> {
        std::mem::take(&mut self.events)
    }

    fn session(&self) -> Session {
        self.phone.view().session.clone()
    }

    /// The app is closed and opened again: a new client on the same store and the same key.
    fn restart_phone(&mut self) {
        self.phone.close(self.now);
        self.phone = PhoneClient::new(self.net.endpoint("phone"), config(), Box::new(self.store.clone())).with_discovery(Box::new(self.discovery.clone()));
    }
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

fn diagnostics(events: &[HostEvent]) -> Vec<String> {
    events.iter().filter_map(|e| if let HostEvent::Diagnostic(d) = e { Some(d.clone()) } else { None }).collect()
}

// ======================================================================================================================
// pairing

#[test]
fn a_phone_pairs_from_the_offer_the_mac_made_and_keeps_the_mac() {
    let mut duo = Duo::new();
    let offer = duo.host.begin_pairing(duo.now).expect("the host knows its address");
    assert_eq!(duo.phone.view().session, Session::Unpaired);
    duo.phone.begin_pairing(duo.now, &offer.to_text()).expect("an offer");
    assert_eq!(duo.phone.view().session, Session::Searching { attempt: 1 });
    duo.step(40);
    // the Mac shows a code, and so does the phone: the same
    let (peer, code) = duo
        .events
        .iter()
        .find_map(|e| if let HostEvent::PairingRequested { peer, code, .. } = e { Some((peer.clone(), *code)) } else { None })
        .expect("the phone proved the secret");
    assert_eq!(code, confirmation_code(&offer.secret, &Duo::host_key(), &Duo::phone_key()));
    assert_eq!(duo.phone.view().session, Session::Pairing { mac_name: "Studio Mac".into(), code });
    assert!(duo.store.peek().mac.is_none(), "nothing is kept before the person says yes");
    // the person says yes
    duo.host.accept_pairing(duo.now, &peer).expect("confirmed");
    duo.step(40);
    assert_eq!(duo.phone.view().session, Session::Connected { mac_name: "Studio Mac".into() });
    assert!(duo.phone.is_active());
    assert_eq!(
        duo.store.peek().mac,
        Some(SavedMac { mac_id: Duo::host_key(), name: "Studio Mac".into(), tickets: vec![offer.ticket.clone()] }),
        "the Mac, its name and the address it was reached at are kept"
    );
    assert!(duo.events.iter().any(|e| matches!(e, HostEvent::Paired { record } if record.peer == Duo::phone_key() && record.name == "Test phone")));
}

#[test]
fn what_the_mac_settled_reaches_the_phone_with_the_pairing() {
    let mut duo = Duo::new();
    duo.host.set_settings(duo.now, HostSettings { reverse_pan: true, snap_to_page_top: true, ..HostSettings::default() });
    let mut buttons = ButtonSet::defaults();
    buttons.buttons[0].label = "Mine".into();
    duo.host.set_buttons(duo.now, &buttons);
    duo.pair();
    assert!(duo.phone.view().settings.reverse_pan && duo.phone.view().settings.snap_to_page_top);
    assert_eq!(duo.phone.view().buttons.buttons[0].label, "Mine");
}

#[test]
fn a_wrong_secret_is_a_wrong_code_and_nothing_is_kept() {
    let mut duo = Duo::new();
    duo.host.begin_pairing(duo.now).expect("an offer");
    let forged = PairingOffer { ticket: duo.host.ticket().to_string(), secret: [1; 16] };
    duo.phone.begin_pairing(duo.now, &forged.to_text()).expect("an offer");
    duo.step(40);
    assert_eq!(duo.phone.view().session, Session::Failed { kind: FailureKind::WrongCode });
    assert_eq!(duo.store.peek().mac, None);
    assert!(!duo.phone.is_active());
}

#[test]
fn the_person_at_the_mac_saying_no_is_declined() {
    let mut duo = Duo::new();
    let offer = duo.host.begin_pairing(duo.now).expect("an offer");
    duo.phone.begin_pairing(duo.now, &offer.to_text()).expect("an offer");
    duo.step(40);
    duo.host.reject_pairing(duo.now, &Duo::phone_key());
    duo.step(20);
    assert_eq!(duo.phone.view().session, Session::Failed { kind: FailureKind::Declined });
    assert_eq!(duo.store.peek().mac, None);
}

#[test]
fn a_mac_with_no_pairing_open_turns_the_phone_away_and_it_says_the_code_is_no_good_any_more() {
    let mut duo = Duo::new();
    let stale = PairingOffer { ticket: duo.host.ticket().to_string(), secret: [1; 16] };
    duo.phone.begin_pairing(duo.now, &stale.to_text()).expect("an offer");
    duo.step(40);
    assert_eq!(duo.phone.view().session, Session::Failed { kind: FailureKind::Expired });
    assert!(!duo.phone.is_active());
}

#[test]
fn an_offer_that_ran_out_of_time_is_expired() {
    let mut duo = Duo::new();
    let offer = duo.host.begin_pairing(duo.now).expect("an offer");
    duo.wait(PAIRING_WINDOW_MS + 1_000);
    duo.phone.begin_pairing(duo.now, &offer.to_text()).expect("an offer");
    duo.step(40);
    assert_eq!(duo.phone.view().session, Session::Failed { kind: FailureKind::Expired });
}

#[test]
fn a_text_that_is_no_offer_changes_nothing() {
    let mut rig = Rig::unpaired();
    assert_eq!(rig.phone.begin_pairing(rig.now, "hello there"), Err(OfferError::NotAnOffer));
    assert_eq!(rig.phone.begin_pairing(rig.now, "azrv9:00:00:x"), Err(OfferError::WrongVersion));
    assert_eq!(rig.session(), Session::Unpaired);
    rig.advance(1_000);
    assert!(rig.link.connects().is_empty(), "nothing was dialed");
}

#[test]
fn the_proof_goes_out_after_the_hello_and_is_the_one_the_mac_checks() {
    let rig = Rig::pairing();
    let sent = rig.link.sent();
    assert_eq!(sent.len(), 2, "{sent:?}");
    assert!(matches!(&sent[0], Frame::Hello(h) if h.role == Role::Phone && h.protocol == PROTOCOL_VERSION && h.name == "Test phone" && h.ticket.is_empty()));
    match &sent[1] {
        Frame::PairRequest { proof: got, phone_name } => {
            assert_eq!(*got, proof(&offer().secret, MAC_ID, PHONE_ID));
            assert_eq!(phone_name, "Test phone");
        }
        other => panic!("a pair request was expected, got {other:?}"),
    }
    assert_eq!(rig.link.connects(), [MAC_TICKET]);
    assert_eq!(rig.session(), Session::Pairing { mac_name: "Studio Mac".into(), code: rig.pairing_code() });
}

#[test]
fn a_code_the_mac_confirms_that_is_not_the_phones_is_not_trusted() {
    let mut rig = Rig::pairing();
    let other = (rig.pairing_code() + 1) % 1_000_000;
    rig.link.frame(1, &Frame::PairAccepted { code: other });
    rig.poll();
    assert_eq!(rig.failed(), Some(FailureKind::WrongCode));
    assert_eq!(rig.store.peek().mac, None, "nothing is kept");
    assert!(rig.phone.diagnostics().iter().any(|d| d.contains("not the phone's")), "{:?}", rig.phone.diagnostics());
    assert_eq!(rig.link.disconnected(), [1]);
}

#[test]
fn the_mac_serving_a_phone_it_knew_already_is_taken_as_paired() {
    // the Mac trusts this key already: it skips the confirmation and serves the phone at once
    let mut rig = Rig::pairing();
    rig.link.frame(1, &Frame::Settings(HostSettings::default()));
    rig.poll();
    assert_eq!(rig.session(), Session::Connected { mac_name: "Studio Mac".into() });
    assert_eq!(rig.store.peek().mac.map(|m| m.mac_id), Some(MAC_ID.to_string()));
}

#[test]
fn every_reason_the_mac_gives_for_not_pairing_has_its_own_failure() {
    for (reject, kind) in [
        (PairReject::WrongSecret, FailureKind::WrongCode),
        (PairReject::Expired, FailureKind::Expired),
        (PairReject::NoPairingOpen, FailureKind::Expired),
        (PairReject::Busy, FailureKind::Busy),
        (PairReject::TooManyPhones, FailureKind::Busy),
        (PairReject::Declined, FailureKind::Declined),
    ] {
        let mut rig = Rig::pairing();
        rig.link.frame(1, &Frame::PairRejected(reject));
        rig.poll();
        assert_eq!(rig.failed(), Some(kind), "{reject:?}");
        assert_eq!(rig.store.peek().mac, None);
    }
}

#[test]
fn a_rejection_that_comes_when_nothing_was_asked_is_ignored() {
    let mut rig = Rig::active();
    rig.link.frame(1, &Frame::PairRejected(PairReject::Declined));
    rig.poll();
    assert!(rig.phone.is_active(), "a connected phone is not unpaired by a stray frame");
}

#[test]
fn a_pairing_that_nobody_confirms_ends_with_a_timeout() {
    let mut rig = Rig::pairing();
    // the Mac pings a waiting phone, and the phone is not taken for gone while the person decides
    rig.advance_alive(CONFIRM_TIMEOUT_MS - 2_000);
    assert!(matches!(rig.session(), Session::Pairing { .. }), "{:?}", rig.session());
    // the person has had the confirmation window and the hello's grace on top of it
    rig.advance_alive(HELLO_TIMEOUT_MS + 2_100);
    assert_eq!(rig.failed(), Some(FailureKind::Timeout), "{:?}", rig.session());
    assert!(rig.link.sent().contains(&Frame::Bye));
}

#[test]
fn a_pairing_that_the_mac_never_answers_times_out_when_the_mac_goes_silent() {
    let mut rig = Rig::pairing();
    rig.advance(PEER_TIMEOUT_MS + 500);
    assert_eq!(rig.failed(), Some(FailureKind::Timeout));
}

#[test]
fn cancelling_a_pairing_with_no_mac_kept_is_an_unpaired_phone_and_the_mac_is_told() {
    let mut rig = Rig::pairing();
    rig.link.take_sent();
    rig.phone.cancel(rig.now);
    assert_eq!(rig.session(), Session::Unpaired);
    assert!(rig.link.sent().contains(&Frame::Bye), "the Mac stops waiting for the person");
    assert_eq!(rig.link.disconnected(), [1]);
    rig.advance(60_000);
    assert_eq!(rig.link.connects().len(), 1, "no more dials");
}

#[test]
fn cancelling_a_new_pairing_leaves_the_old_mac_and_looks_for_it_again() {
    let mut rig = Rig::saved();
    let other = PairingOffer { ticket: "another-macs-ticket".into(), secret: [4; 16] };
    rig.phone.begin_pairing(rig.now, &other.to_text()).expect("an offer");
    rig.poll();
    assert_eq!(rig.link.connects(), ["another-macs-ticket"], "the offer's address is dialed");
    rig.phone.cancel(rig.now);
    assert_eq!(rig.session(), Session::Searching { attempt: 1 });
    assert!(rig.store.peek().mac.is_some(), "the Mac the phone was paired with is kept");
    rig.poll();
    assert_eq!(rig.link.connects(), ["another-macs-ticket", MAC_TICKET], "and the saved Mac is dialed again");
}

#[test]
fn cancelling_the_search_for_the_kept_mac_stops_it_and_says_it_was_not_found() {
    let mut rig = Rig::saved();
    rig.poll();
    rig.phone.cancel(rig.now);
    assert_eq!(rig.failed(), Some(FailureKind::NotFound));
    rig.advance(120_000);
    assert_eq!(rig.link.connects().len(), 1, "it does not dial again by itself");
    assert!(rig.store.peek().mac.is_some(), "and the Mac is still the phone's");
    rig.phone.retry(rig.now);
    rig.poll();
    assert_eq!(rig.link.connects().len(), 2, "retry dials once more");
}

// ======================================================================================================================
// what the Mac says first

#[test]
fn a_paired_phone_dials_its_mac_at_the_first_poll_and_is_served_after_the_hellos() {
    let mut rig = Rig::saved();
    assert_eq!(rig.session(), Session::Searching { attempt: 1 });
    assert_eq!(rig.phone.view().mac_name, "Studio Mac", "the Mac's name is known before it answers");
    rig.poll();
    assert_eq!(rig.link.connects(), [MAC_TICKET]);
    assert!(rig.link.sent().is_empty(), "nothing is said before the connection is there");
    rig.link.say(LinkEvent::Connected { peer: 3, id: MAC_ID.into() });
    rig.poll();
    assert_eq!(count(&rig.link.sent(), "hello"), 1);
    assert!(matches!(rig.session(), Session::Searching { .. }), "not served before the Mac says hello");
    rig.link.frame(3, &mac_hello());
    assert!(rig.poll());
    assert_eq!(rig.session(), Session::Connected { mac_name: "Studio Mac".into() });
}

#[test]
fn a_connection_to_another_key_than_the_pairing_made_is_not_served() {
    let mut rig = Rig::saved();
    rig.connect_as(1, &"ee".repeat(16));
    assert_eq!(rig.link.disconnected(), [1], "the stranger is let go");
    assert!(rig.link.sent().is_empty(), "and told nothing");
    assert!(!rig.phone.is_active());
    assert!(rig.phone.diagnostics().iter().any(|d| d.contains("another endpoint")), "{:?}", rig.phone.diagnostics());
}

#[test]
fn a_connection_nobody_asked_for_is_let_go() {
    let mut rig = Rig::unpaired();
    rig.connect_as(4, MAC_ID);
    assert_eq!(rig.link.disconnected(), [4]);
    assert_eq!(rig.session(), Session::Unpaired);
}

#[test]
fn a_hello_of_a_protocol_the_phone_is_too_old_for_tells_it_to_update_the_companion() {
    let mut rig = Rig::saved();
    rig.connect_as(1, MAC_ID);
    rig.link.frame(1, &Frame::Hello(Hello { role: Role::Host, protocol: 5, min_protocol: 4, app_version: "9.0.0".into(), name: "Studio Mac".into(), ticket: String::new() }));
    rig.poll();
    assert_eq!(rig.failed(), Some(FailureKind::UpdateCompanion));
    assert!(rig.link.sent().contains(&Frame::Bye));
    rig.advance(120_000);
    assert_eq!(rig.link.connects().len(), 1, "an update is needed, so it does not keep dialing");
}

#[test]
fn a_hello_of_a_mac_that_is_too_old_tells_the_user_to_update_azreview() {
    let mut rig = Rig::saved();
    rig.connect_as(1, MAC_ID);
    rig.link.frame(1, &Frame::Hello(Hello { role: Role::Host, protocol: 0, min_protocol: 0, app_version: "0.0.1".into(), name: "Studio Mac".into(), ticket: String::new() }));
    rig.poll();
    assert_eq!(rig.failed(), Some(FailureKind::UpdateAzReview));
}

#[test]
fn what_the_mac_refuses_with_is_the_failure_the_screen_shows_and_the_phone_stops_trying() {
    for (refusal, kind) in [
        (Refusal::NotPaired, FailureKind::Forgotten),
        (Refusal::UpdateCompanion, FailureKind::UpdateCompanion),
        (Refusal::UpdateAzReview, FailureKind::UpdateAzReview),
        (Refusal::UnsupportedRole, FailureKind::NotFound),
    ] {
        let mut rig = Rig::saved();
        rig.connect_as(1, MAC_ID);
        rig.link.frame(1, &Frame::Refused(refusal));
        rig.poll();
        assert_eq!(rig.failed(), Some(kind), "{refusal:?}");
        rig.advance(120_000);
        assert_eq!(rig.link.connects().len(), 1, "{refusal:?}: the phone does not dial again by itself");
        rig.phone.retry(rig.now);
        rig.poll();
        assert_eq!(rig.link.connects().len(), 2, "{refusal:?}: retry dials once more");
    }
}

#[test]
fn something_that_is_not_a_host_answering_at_the_address_is_not_found() {
    let mut rig = Rig::saved();
    rig.connect_as(1, MAC_ID);
    rig.link.frame(1, &Frame::Hello(Hello { role: Role::Review, protocol: 1, min_protocol: 1, app_version: "0.1.0".into(), name: "A review".into(), ticket: String::new() }));
    rig.poll();
    assert_eq!(rig.failed(), Some(FailureKind::NotFound));
}

#[test]
fn the_name_and_the_newest_address_in_the_macs_hello_are_kept_once() {
    let mut rig = Rig::saved();
    rig.connect_as(1, MAC_ID);
    let hello = |name: &str, ticket: &str| Frame::Hello(Hello { role: Role::Host, protocol: 1, min_protocol: 1, app_version: "0.1.0".into(), name: name.into(), ticket: ticket.into() });
    rig.link.frame(1, &hello("Studio Mac (renamed)", "new-ticket-2"));
    rig.poll();
    assert_eq!(rig.phone.addresses(), ["new-ticket-2", MAC_TICKET], "the newest address first, the old one after");
    let kept = rig.store.peek().mac.expect("a Mac");
    assert_eq!((kept.name.as_str(), kept.tickets.clone()), ("Studio Mac (renamed)", vec!["new-ticket-2".to_string(), MAC_TICKET.to_string()]));
    assert_eq!(rig.phone.view().mac_name, "Studio Mac (renamed)");
    let saves = rig.store.saves();
    // the same hello on the next connection changes nothing and writes nothing
    rig.link.say(LinkEvent::Disconnected { peer: 1, reason: "gone".into() });
    rig.poll();
    rig.advance(RECONNECT_FIRST_MS + 5);
    rig.connect_as(2, MAC_ID);
    rig.link.frame(2, &hello("Studio Mac (renamed)", "new-ticket-2"));
    rig.poll();
    assert!(rig.phone.is_active());
    assert_eq!(rig.store.saves(), saves, "nothing new to keep");
}

#[test]
fn a_ticket_with_characters_a_ticket_does_not_have_is_not_kept() {
    let mut rig = Rig::saved();
    rig.connect_as(1, MAC_ID);
    rig.link.frame(1, &Frame::Hello(Hello { role: Role::Host, protocol: 1, min_protocol: 1, app_version: "0.1.0".into(), name: "Studio Mac".into(), ticket: "bad ticket\nwith lines".into() }));
    rig.poll();
    assert_eq!(rig.phone.addresses(), [MAC_TICKET]);
    assert!(rig.phone.is_active(), "the rest of the hello is good");
}

#[test]
fn a_frame_before_the_hello_is_not_taken() {
    let mut rig = Rig::saved();
    rig.connect_as(1, MAC_ID);
    rig.link.frame(1, &Frame::Settings(HostSettings { reverse_pan: true, ..HostSettings::default() }));
    rig.poll();
    assert!(!rig.phone.view().settings.reverse_pan);
    assert!(matches!(rig.session(), Session::Searching { .. }));
}

#[test]
fn a_mac_that_never_says_hello_is_given_up_on_and_dialed_again() {
    let mut rig = Rig::saved();
    rig.connect_as(1, MAC_ID);
    rig.advance(HELLO_TIMEOUT_MS + 100);
    assert!(rig.link.sent().contains(&Frame::Bye));
    assert_eq!(rig.link.disconnected(), [1]);
    rig.advance(RECONNECT_FIRST_MS + 100);
    assert_eq!(rig.link.connects().len(), 2, "a second dial");
}

// ======================================================================================================================
// staying connected

#[test]
fn a_dial_that_gets_no_answer_in_time_is_given_up_and_the_next_address_is_tried() {
    let mut store = saved_mac();
    store.mac.as_mut().expect("a Mac").tickets = vec!["first-ticket".into(), "second-ticket".into()];
    let mut rig = Rig::with_store(MemoryStore::with(store));
    rig.poll();
    assert_eq!(rig.link.connects(), ["first-ticket"]);
    rig.advance(DIAL_TIMEOUT_MS - 100);
    assert_eq!(rig.session(), Session::Searching { attempt: 1 });
    rig.advance(200);
    // the dial was given up: an answer that comes now is to nobody, and is let go
    rig.link.say(LinkEvent::Connected { peer: 9, id: MAC_ID.into() });
    rig.poll();
    assert_eq!(rig.link.disconnected(), [9]);
    assert!(!rig.phone.is_active());
    rig.advance(RECONNECT_FIRST_MS + 50);
    assert_eq!(rig.link.connects(), ["first-ticket", "second-ticket"], "the addresses are tried in turn");
}

#[test]
fn a_dial_that_fails_is_tried_again_with_waits_that_double_up_to_eight_seconds_and_then_the_address_is_said_to_have_changed() {
    let mut rig = Rig::saved();
    let mut dials: Vec<Millis> = Vec::new();
    let mut seen = 0;
    let mut address_changed_at = None;
    // every dial fails at once, as a refused connection does: the failure is told in the millisecond of the dial
    for _ in 0..400_000 {
        rig.now += 1;
        let before = rig.link.connects().len();
        rig.phone.poll(rig.now);
        if rig.link.connects().len() > before {
            dials.push(rig.now);
            rig.link.say(LinkEvent::Failed { text: "connection refused".into() });
            rig.phone.poll(rig.now);
        }
        if seen == 0 && rig.failed() == Some(FailureKind::AddressChanged) {
            seen = dials.len();
            address_changed_at = Some(rig.now);
        }
        if dials.len() >= 9 {
            break;
        }
    }
    let gaps: Vec<Millis> = dials.windows(2).map(|w| w[1] - w[0]).collect();
    assert_eq!(&gaps[..5], &[500, 1_000, 2_000, 4_000, 8_000], "0.5 s, then twice as long each time, up to 8 s: {gaps:?}");
    assert_eq!(seen, ADDRESS_CHANGED_AFTER as usize, "after six failed dials the screen says the address changed");
    assert!(address_changed_at.is_some());
    assert_eq!(&gaps[5..], &[RECONNECT_SLOW_MS, RECONNECT_SLOW_MS, RECONNECT_SLOW_MS], "then slowly, every 30 s: {gaps:?}");
    assert_eq!(rig.failed(), Some(FailureKind::AddressChanged), "and the screen stays on it while the phone keeps trying");
}

#[test]
fn a_dial_that_fails_before_the_link_was_ever_up_is_searching_and_after_it_was_up_is_lost() {
    let mut rig = Rig::saved();
    rig.poll();
    rig.link.say(LinkEvent::Failed { text: "no answer".into() });
    rig.poll();
    assert_eq!(rig.session(), Session::Searching { attempt: 1 });
    let mut up = Rig::active();
    up.link.say(LinkEvent::Disconnected { peer: 1, reason: "gone".into() });
    up.poll();
    assert_eq!(up.session(), Session::Lost { mac_name: "Studio Mac".into(), attempt: 1 });
}

#[test]
fn a_link_that_ends_is_dialed_again_after_half_a_second_and_serves_again() {
    let mut rig = Rig::active();
    rig.link.say(LinkEvent::Disconnected { peer: 1, reason: "the connection was cut".into() });
    rig.poll();
    assert!(!rig.phone.is_active());
    assert_eq!(rig.phone.view().latency_ms, None);
    rig.advance(RECONNECT_FIRST_MS - 50);
    assert_eq!(rig.link.connects().len(), 1, "not yet");
    rig.advance(100);
    assert_eq!(rig.link.connects().len(), 2, "after half a second");
    rig.connect_as(2, MAC_ID);
    rig.link.frame(2, &mac_hello());
    rig.poll();
    assert_eq!(rig.session(), Session::Connected { mac_name: "Studio Mac".into() });
}

#[test]
fn a_mac_that_says_goodbye_is_lost_and_dialed_again() {
    let mut rig = Rig::active();
    rig.link.frame(1, &Frame::Bye);
    rig.poll();
    assert_eq!(rig.session(), Session::Lost { mac_name: "Studio Mac".into(), attempt: 1 });
    assert_eq!(rig.link.disconnected(), [1]);
}

#[test]
fn silence_for_eight_seconds_ends_the_connection() {
    let mut rig = Rig::active();
    rig.advance(PEER_TIMEOUT_MS - 200);
    assert!(rig.phone.is_active());
    rig.advance(400);
    assert!(!rig.phone.is_active());
    assert!(matches!(rig.session(), Session::Lost { .. }), "{:?}", rig.session());
}

#[test]
fn anything_the_mac_sends_counts_as_a_sign_of_life() {
    let mut rig = Rig::active();
    for _ in 0..4 {
        rig.advance(PEER_TIMEOUT_MS - 1_000);
        rig.link.frame(1, &Frame::Notice(crate::frame::Notice { level: crate::frame::NoticeLevel::Info, text: "still here".into() }));
        rig.poll();
    }
    assert!(rig.phone.is_active());
}

#[test]
fn the_phone_pings_the_mac_and_shows_the_round_trip_and_answers_the_macs_pings() {
    let mut rig = Rig::saved();
    rig.connect_as(1, MAC_ID);
    rig.link.frame(1, &mac_hello());
    rig.poll();
    let pings: Vec<u32> = rig.link.sent().iter().filter_map(|f| if let Frame::Ping { nonce } = f { Some(*nonce) } else { None }).collect();
    assert_eq!(pings.len(), 1, "a ping as soon as the Mac serves the phone");
    rig.advance(20);
    // a pong that is not the one asked for says nothing
    rig.link.frame(1, &Frame::Pong { nonce: pings[0].wrapping_add(1000) });
    assert!(!rig.poll());
    assert_eq!(rig.phone.view().latency_ms, None);
    rig.advance(17);
    rig.link.frame(1, &Frame::Pong { nonce: pings[0] });
    assert!(rig.poll());
    assert_eq!(rig.phone.view().latency_ms, Some(37));
    // and the Mac's ping is answered
    rig.link.take_sent();
    rig.link.frame(1, &Frame::Ping { nonce: 77 });
    rig.poll();
    assert!(rig.link.sent().contains(&Frame::Pong { nonce: 77 }));
}

#[test]
fn pings_go_out_every_two_seconds() {
    let mut rig = Rig::active();
    rig.advance_alive(PING_INTERVAL_MS * 3 + 100);
    // the first went out when the Mac began to serve the phone (the rig forgot it); then one every two seconds
    assert_eq!(count(&rig.link.sent(), "ping"), 3);
}

#[test]
fn noise_from_the_mac_is_forgiven_three_times_and_the_fourth_ends_the_connection() {
    let mut rig = Rig::active();
    for round in 1..=GARBAGE_FORGIVEN {
        rig.link.noise(1, &[1, 2, 3]);
        rig.poll();
        assert!(rig.phone.is_active(), "forgiven {round}");
    }
    rig.link.noise(1, &[0xA7, 99, 99]);
    rig.poll();
    assert!(!rig.phone.is_active());
    assert!(rig.link.sent().contains(&Frame::Bye));
    assert!(rig.phone.diagnostics().iter().any(|d| d.contains("was not understood")), "{:?}", rig.phone.diagnostics());
}

#[test]
fn a_frame_only_a_phone_may_send_is_not_taken_from_the_mac() {
    let mut rig = Rig::active();
    rig.link.frame(1, &Frame::Viewport(ViewportCmd { screen: ScreenId::MAIN, seq: 1, op: ViewportOp::Fit }));
    rig.poll();
    assert!(rig.phone.is_active());
    assert!(rig.phone.diagnostics().iter().any(|d| d.contains("only a phone may send")), "{:?}", rig.phone.diagnostics());
}

#[test]
fn a_dial_that_cannot_even_start_is_a_failed_dial() {
    let mut rig = Rig::saved();
    rig.link.lock().connect_error = Some("not an endpoint ticket".into());
    rig.poll();
    assert!(rig.link.connects().is_empty());
    assert!(rig.phone.diagnostics().iter().any(|d| d.contains("not an endpoint ticket")));
    rig.advance(RECONNECT_FIRST_MS + 20);
    assert_eq!(rig.link.connects().len(), 1, "and tried again");
}

#[test]
fn a_paired_phone_with_no_address_at_all_says_the_address_changed_and_does_not_dial() {
    let mut rig = Rig::with_store(MemoryStore::with(Stored { secret_key: None, mac: Some(SavedMac { mac_id: MAC_ID.into(), name: "Studio Mac".into(), tickets: vec![] }) }));
    rig.advance(20_000);
    assert!(rig.link.connects().is_empty());
    assert_eq!(rig.failed(), Some(FailureKind::AddressChanged));
    assert!(rig.phone.diagnostics().iter().any(|d| d.contains("no address")));
}

#[test]
fn a_failure_the_system_caused_by_not_allowing_the_local_network_is_a_permission_failure() {
    let mut pairing = Rig::unpaired();
    pairing.phone.begin_pairing(pairing.now, &offer().to_text()).expect("an offer");
    pairing.poll();
    pairing.link.say(LinkEvent::Failed { text: format!("{PERMISSION_PREFIX} the local network is off for this app") });
    pairing.poll();
    assert_eq!(pairing.failed(), Some(FailureKind::NoPermission));
    // a phone that is paired keeps trying, slowly, and shows the same
    let mut saved = Rig::saved();
    saved.poll();
    saved.link.say(LinkEvent::Failed { text: format!("{PERMISSION_PREFIX} off") });
    saved.poll();
    assert_eq!(saved.failed(), Some(FailureKind::NoPermission));
    saved.advance(RECONNECT_SLOW_MS + 50);
    assert_eq!(saved.link.connects().len(), 2, "and tries again once the user may have switched it on");
}

#[test]
fn a_failure_while_connected_is_only_written_down() {
    let mut rig = Rig::active();
    rig.link.say(LinkEvent::Failed { text: "a path went away".into() });
    rig.poll();
    assert!(rig.phone.is_active());
    assert!(rig.phone.diagnostics().iter().any(|d| d.contains("a path went away")));
}

#[test]
fn the_failure_of_a_dial_for_a_pairing_is_not_found_and_retry_dials_the_offer_again() {
    let mut rig = Rig::unpaired();
    rig.phone.begin_pairing(rig.now, &offer().to_text()).expect("an offer");
    rig.poll();
    rig.link.say(LinkEvent::Failed { text: "no answer".into() });
    rig.poll();
    assert_eq!(rig.failed(), Some(FailureKind::NotFound));
    rig.advance(60_000);
    assert_eq!(rig.link.connects().len(), 1, "a first pairing does not retry by itself");
    rig.phone.retry(rig.now);
    assert_eq!(rig.session(), Session::Searching { attempt: 1 });
    rig.poll();
    assert_eq!(rig.link.connects(), [MAC_TICKET, MAC_TICKET]);
}

#[test]
fn a_pairing_dial_that_gets_no_answer_in_time_is_not_found() {
    let mut rig = Rig::unpaired();
    rig.phone.begin_pairing(rig.now, &offer().to_text()).expect("an offer");
    rig.advance(DIAL_TIMEOUT_MS + 100);
    assert_eq!(rig.failed(), Some(FailureKind::NotFound));
}

#[test]
fn find_my_mac_with_no_code_and_no_mac_looks_briefly_and_says_it_found_nothing() {
    let mut rig = Rig::unpaired();
    rig.phone.retry(rig.now);
    assert_eq!(rig.session(), Session::Searching { attempt: 1 });
    rig.advance(SEARCH_MS - 100);
    assert_eq!(rig.session(), Session::Searching { attempt: 1 });
    rig.advance(200);
    assert_eq!(rig.failed(), Some(FailureKind::NotFound));
    assert!(rig.link.connects().is_empty(), "there is nothing to dial");
}

// ---- end to end

#[test]
fn a_cut_link_is_dialed_again_and_serves_again_and_the_new_connections_pen_is_shown() {
    let mut duo = Duo::paired();
    let host_key = Duo::host_key();
    for seq in 1..=5 {
        duo.host.publish_pen(PenState { folder: format!("a{seq}"), ..PenState::empty() });
        duo.step(40);
    }
    let old_seq = duo.phone.view().pen.as_ref().expect("a pen").seq;
    assert!(old_seq >= 5, "{old_seq}");
    duo.net.kill_connections(duo.now, "mac");
    duo.step(10);
    assert!(matches!(duo.session(), Session::Lost { .. }), "{:?}", duo.session());
    duo.step(RECONNECT_FIRST_MS + 100);
    assert!(duo.phone.is_active(), "{:?}", duo.session());
    // the Mac counts its pen states from one again on a new connection: the phone shows them all the same
    duo.host.publish_pen(PenState { folder: "after".into(), ..PenState::empty() });
    duo.step(80);
    let pen = duo.phone.view().pen.clone().expect("a pen");
    assert_eq!(pen.folder, "after");
    assert!(pen.seq < old_seq, "the new connection counts from one again ({} vs {old_seq})", pen.seq);
    assert_eq!(duo.store.peek().mac.map(|m| m.mac_id), Some(host_key));
}

#[test]
fn a_mac_that_is_down_is_tried_again_and_found_when_it_comes_back() {
    let mut duo = Duo::paired();
    duo.net.set_knobs(Knobs { latency_ms: 1, down: true, ..Knobs::default() });
    duo.net.kill_connections(duo.now, "mac");
    duo.wait(20_000);
    assert!(!duo.phone.is_active());
    duo.net.set_knobs(Knobs { latency_ms: 1, ..Knobs::default() });
    // the next dial is at most a slow wait away
    duo.wait(RECONNECT_SLOW_MS + 5_000);
    assert!(duo.phone.is_active(), "{:?}", duo.session());
}

#[test]
fn a_mac_whose_address_changed_is_found_through_discovery_without_waiting() {
    let mut duo = Duo::paired();
    duo.net.change_address("mac");
    duo.net.kill_connections(duo.now, "mac");
    // the old address leads nowhere: six failed dials and the phone says so
    for _ in 0..400 {
        duo.wait(1_000);
        if duo.session() == (Session::Failed { kind: FailureKind::AddressChanged }) {
            break;
        }
    }
    assert_eq!(duo.session(), Session::Failed { kind: FailureKind::AddressChanged });
    // another Mac announces itself: not ours, nothing happens
    duo.discovery.appear(Nearby::from_announcement(&MockNet::key("stranger"), "", vec![format!("mt-{}-1", MockNet::key("stranger"))]));
    duo.step(50);
    assert_eq!(duo.session(), Session::Failed { kind: FailureKind::AddressChanged });
    // ours announces itself at its new address: dialed at once
    duo.discovery.appear(Nearby::from_announcement(&Duo::host_key(), "", vec![format!("mt-{}-2", Duo::host_key())]));
    duo.step(60);
    assert!(duo.phone.is_active(), "{:?}", duo.session());
}

#[test]
fn a_mac_that_forgot_the_phone_says_forgotten_once_and_the_phone_stops_dialing() {
    let mut duo = Duo::paired();
    duo.host.forget(duo.now, &Duo::phone_key());
    duo.step(10);
    duo.take_events();
    duo.wait(60_000);
    assert_eq!(duo.session(), Session::Failed { kind: FailureKind::Forgotten });
    let refused = diagnostics(&duo.take_events()).iter().filter(|d| d.contains("not paired tried to connect")).count();
    assert_eq!(refused, 1, "the Mac was asked once");
}

#[test]
fn an_app_that_is_opened_again_dials_its_mac_and_is_served_without_pairing_again() {
    let mut duo = Duo::paired();
    duo.restart_phone();
    assert_eq!(duo.session(), Session::Searching { attempt: 1 });
    assert_eq!(duo.phone.view().mac_name, "Studio Mac");
    duo.step(60);
    assert_eq!(duo.session(), Session::Connected { mac_name: "Studio Mac".into() });
}

#[test]
fn forgetting_the_mac_says_goodbye_keeps_the_phones_key_and_is_unpaired() {
    let mut duo = Duo::with_store(MemoryStore::with(Stored { secret_key: Some([3; 32]), mac: None }));
    duo.pair();
    assert!(duo.store.peek().mac.is_some());
    duo.phone.forget_mac(duo.now);
    assert_eq!(duo.phone.view(), &PhoneView::new());
    duo.step(20);
    assert!(duo.take_events().iter().any(|e| matches!(e, HostEvent::PeerGone { reason, .. } if reason.contains("goodbye"))));
    assert_eq!(duo.store.peek(), Stored { secret_key: Some([3; 32]), mac: None }, "the Mac is forgotten, the phone's own key is not");
    assert!(duo.phone.addresses().is_empty());
    duo.wait(60_000);
    assert_eq!(duo.session(), Session::Unpaired, "it does not dial a Mac it forgot");
}

#[test]
fn an_unreadable_store_is_said_and_the_phone_starts_empty_and_an_unwritable_one_does_not_stop_a_pairing() {
    struct Broken;
    impl PhoneStore for Broken {
        fn load(&mut self) -> Result<Stored, String> {
            Err("the disk said no".into())
        }
        fn save(&mut self, _: &Stored) -> Result<(), String> {
            Err("the disk is full".into())
        }
    }
    let net = MockNet::with_knobs(Knobs { latency_ms: 1, ..Knobs::default() });
    let mut host = CompanionHost::new(net.endpoint("mac"), host_config(), Box::new(SeededEntropy(7)));
    let mut phone = PhoneClient::new(net.endpoint("phone"), config(), Box::new(Broken));
    assert_eq!(phone.view().session, Session::Unpaired);
    assert!(phone.diagnostics().iter().any(|d| d.contains("the disk said no")));
    let mut now = 1_000;
    let mut events = Vec::new();
    fn run(ms: u64, host: &mut CompanionHost<MockLink>, phone: &mut PhoneClient<MockLink>, now: &mut Millis, events: &mut Vec<HostEvent>) {
        for _ in 0..ms {
            *now += 1;
            events.extend(host.poll(*now));
            phone.poll(*now);
        }
    }
    run(5, &mut host, &mut phone, &mut now, &mut events);
    let offer = host.begin_pairing(now).expect("an offer");
    phone.begin_pairing(now, &offer.to_text()).expect("an offer");
    run(40, &mut host, &mut phone, &mut now, &mut events);
    let peer = events.iter().find_map(|e| if let HostEvent::PairingRequested { peer, .. } = e { Some(peer.clone()) } else { None }).expect("asked");
    host.accept_pairing(now, &peer).expect("confirmed");
    run(40, &mut host, &mut phone, &mut now, &mut events);
    assert!(phone.is_active(), "paired for this run");
    assert!(phone.diagnostics().iter().any(|d| d.contains("the disk is full")), "{:?}", phone.diagnostics());
}

#[test]
fn diagnostics_keep_the_newest_sixty_four() {
    let mut rig = Rig::active();
    for i in 0..(MAX_DIAGNOSTICS + 20) {
        rig.link.say(LinkEvent::Failed { text: format!("trouble {i}") });
    }
    rig.poll();
    let diags = rig.phone.diagnostics();
    assert_eq!(diags.len(), MAX_DIAGNOSTICS);
    assert!(diags.last().expect("a last").contains(&format!("trouble {}", MAX_DIAGNOSTICS + 19)));
    assert_eq!(rig.phone.take_diagnostics().len(), MAX_DIAGNOSTICS);
    assert!(rig.phone.diagnostics().is_empty());
}

#[test]
fn closing_says_goodbye_and_closes_the_link() {
    let mut rig = Rig::active();
    rig.phone.close(rig.now);
    assert!(rig.link.sent().contains(&Frame::Bye));
    assert!(rig.link.lock().closed);
    assert!(!rig.phone.is_active());
}

// ======================================================================================================================
// what goes out

#[test]
fn nothing_is_sent_while_the_mac_does_not_serve_the_phone() {
    let mut rig = Rig::unpaired();
    let now = rig.now;
    rig.phone.gesture(now, 1, Phase::Begin, (0.0, 0.0), 1.0, Focus::Pen);
    rig.phone.gesture(now, 1, Phase::Update, (0.1, 0.0), 1.0, Focus::Pen);
    rig.phone.fit(now);
    rig.phone.page_step(now, 1);
    rig.phone.reset_view(now);
    rig.phone.continue_reviewing(now);
    rig.phone.open_file(now, "src/lib.rs", None);
    rig.phone.press_button(now, 0);
    assert_eq!(rig.phone.set_buttons(now, &ButtonSet::defaults()), Vec::<Capped>::new());
    rig.poll();
    assert!(rig.link.sent().is_empty(), "{:?}", rig.link.sent());
    assert_eq!(rig.phone.view().button_status, [None; BUTTON_COUNT]);
    // and a phone that is still waiting for the Mac's hello sends nothing either
    let mut waiting = Rig::saved();
    waiting.connect_as(1, MAC_ID);
    waiting.link.take_sent();
    waiting.phone.fit(waiting.now);
    assert!(waiting.link.sent().is_empty());
}

#[test]
fn a_drag_is_a_begin_updates_and_an_end_in_order_with_the_totals_the_pad_gave() {
    let mut duo = Duo::paired();
    let now = duo.now;
    duo.phone.gesture(now, 7, Phase::Begin, (0.0, 0.0), 1.0, Focus::Pen);
    duo.step(30);
    duo.phone.gesture(duo.now, 7, Phase::Update, (0.10, 0.20), 1.5, Focus::Pen);
    duo.step(30);
    duo.phone.gesture(duo.now, 7, Phase::Update, (0.11, 0.25), 1.6, Focus::Pen);
    duo.step(30);
    duo.phone.gesture(duo.now, 7, Phase::End, (0.12, 0.30), 1.7, Focus::Pen);
    duo.step(30);
    assert_eq!(
        gestures(&duo.events),
        [
            (7, Phase::Begin, 0.0, 0.0, 1.0),
            (7, Phase::Update, 0.10, 0.20, 1.5),
            (7, Phase::Update, 0.11, 0.25, 1.6),
            (7, Phase::End, 0.12, 0.30, 1.7),
        ]
    );
}

#[test]
fn updates_leave_at_most_sixty_a_second_newest_wins_and_the_end_always_leaves() {
    let mut duo = Duo::paired();
    duo.phone.gesture(duo.now, 3, Phase::Begin, (0.0, 0.0), 1.0, Focus::Pen);
    // a pad that reports every 5 ms for a second
    for i in 1..=200 {
        duo.phone.gesture(duo.now, 3, Phase::Update, (i as f32 / 1000.0, 0.0), 1.0, Focus::Pen);
        duo.step(5);
    }
    duo.phone.gesture(duo.now, 3, Phase::End, (0.2, 0.0), 1.0, Focus::Pen);
    duo.step(30);
    let all = gestures(&duo.events);
    let updates: Vec<_> = all.iter().filter(|g| g.1 == Phase::Update).collect();
    assert!((40..=60).contains(&updates.len()), "{} updates in a second", updates.len());
    assert!(updates.windows(2).all(|w| w[0].2 < w[1].2), "newer and newer");
    assert_eq!(all.last().map(|g| (g.1, g.2)), Some((Phase::End, 0.2)), "the end carries the last totals");
    assert_eq!(all.first().map(|g| g.1), Some(Phase::Begin));
}

#[test]
fn an_update_that_comes_before_its_turn_waits_and_a_newer_one_takes_its_place() {
    let mut rig = Rig::active();
    let t = rig.now;
    rig.phone.gesture(t, 1, Phase::Begin, (0.0, 0.0), 1.0, Focus::Pen);
    rig.phone.gesture(t, 1, Phase::Update, (0.1, 0.0), 1.0, Focus::Pen);
    rig.phone.gesture(t, 1, Phase::Update, (0.2, 0.0), 1.0, Focus::Pen);
    rig.phone.gesture(t, 1, Phase::Update, (0.3, 0.0), 1.0, Focus::Pen);
    let sent_now: Vec<f32> = rig.link.sent().iter().filter_map(|f| if let Frame::Viewport(ViewportCmd { op: ViewportOp::Gesture { phase: Phase::Update, pan_x, .. }, .. }) = f { Some(*pan_x) } else { None }).collect();
    assert_eq!(sent_now, [0.1], "the first update goes at once, the others wait");
    rig.advance(30);
    let sent_later: Vec<f32> = rig.link.sent().iter().filter_map(|f| if let Frame::Viewport(ViewportCmd { op: ViewportOp::Gesture { phase: Phase::Update, pan_x, .. }, .. }) = f { Some(*pan_x) } else { None }).collect();
    assert_eq!(sent_later, [0.1, 0.3], "and the newest is the one that goes: the 0.2 was replaced");
}

#[test]
fn a_gesture_beyond_the_limits_is_held_to_them_and_says_so_once() {
    let mut duo = Duo::paired();
    duo.phone.take_diagnostics();
    duo.phone.gesture(duo.now, 4, Phase::Begin, (0.0, 0.0), 1.0, Focus::Pen);
    duo.step(25);
    duo.phone.gesture(duo.now, 4, Phase::Update, (100.0, -100.0), 100.0, Focus::Pen);
    duo.step(25);
    duo.phone.gesture(duo.now, 4, Phase::Update, (200.0, -100.0), 0.0, Focus::Pen);
    duo.step(25);
    duo.phone.gesture(duo.now, 4, Phase::End, (f32::NAN, f32::INFINITY), f32::NAN, Focus::Pen);
    duo.step(25);
    let got = gestures(&duo.events);
    assert_eq!(got[1], (4, Phase::Update, MAX_GESTURE_PAN, -MAX_GESTURE_PAN, MAX_GESTURE_ZOOM));
    assert_eq!(got[2], (4, Phase::Update, MAX_GESTURE_PAN, -MAX_GESTURE_PAN, MIN_GESTURE_ZOOM));
    assert_eq!(got[3], (4, Phase::End, 0.0, MAX_GESTURE_PAN, 1.0), "what is not a number is a pan of nothing and no zoom, an infinity is the limit");
    assert_eq!(duo.phone.diagnostics().iter().filter(|d| d.contains("beyond the limits")).count(), 1, "said once for the gesture, not for every update");
    assert!(diagnostics(&duo.events).iter().all(|d| !d.contains("held to the limits")), "the Mac had nothing to hold: {:?}", diagnostics(&duo.events));
}

#[test]
fn a_fit_a_page_step_a_reset_and_continue_reviewing_reach_the_mac_in_order() {
    let mut duo = Duo::paired();
    duo.phone.fit(duo.now);
    duo.phone.page_step(duo.now, 1);
    duo.phone.page_step(duo.now, -1);
    duo.phone.page_step(duo.now, 100);
    duo.phone.reset_view(duo.now);
    duo.phone.continue_reviewing(duo.now);
    duo.step(40);
    let ops: Vec<ViewportOp> = duo.events.iter().filter_map(|e| if let HostEvent::Viewport { op, .. } = e { Some(*op) } else { None }).collect();
    assert_eq!(ops, [ViewportOp::Fit, ViewportOp::PageStep(1), ViewportOp::PageStep(-1), ViewportOp::PageStep(8), ViewportOp::Reset]);
    assert!(duo.events.iter().any(|e| matches!(e, HostEvent::ContinueReviewing { screen, .. } if *screen == ScreenId::MAIN)));
}

#[test]
fn every_viewport_command_is_numbered_higher_than_the_one_before_and_names_the_screen_the_pen_is_on() {
    let mut rig = Rig::active();
    rig.link.frame(1, &Frame::PenState(PenState { seq: 1, screen: ScreenId(2), ..PenState::empty() }));
    rig.poll();
    rig.phone.fit(rig.now);
    rig.phone.reset_view(rig.now);
    rig.phone.page_step(rig.now, 1);
    let cmds: Vec<ViewportCmd> = rig.link.sent().iter().filter_map(|f| if let Frame::Viewport(c) = f { Some(*c) } else { None }).collect();
    assert_eq!(cmds.len(), 3);
    assert!(cmds.windows(2).all(|w| w[1].seq == w[0].seq.wrapping_add(1)), "{cmds:?}");
    assert!(cmds.iter().all(|c| c.screen == ScreenId(2)));
    rig.phone.continue_reviewing(rig.now);
    assert!(rig.link.sent().contains(&Frame::ContinueReviewing { screen: ScreenId(2) }));
}

#[test]
fn a_gesture_update_travels_on_its_own_newest_wins_track_and_a_fit_is_reliable() {
    let mut rig = Rig::active();
    rig.phone.gesture(rig.now, 1, Phase::Begin, (0.0, 0.0), 1.0, Focus::Pen);
    rig.phone.gesture(rig.now, 1, Phase::Update, (0.1, 0.0), 1.0, Focus::Pen);
    rig.phone.fit(rig.now);
    let deliveries: Vec<Delivery> = rig.link.lock().sent.iter().map(|(_, _, d)| *d).collect();
    assert_eq!(deliveries, [Delivery::Reliable, Delivery::Latest(crate::frame::TRACK_VIEWPORT), Delivery::Reliable]);
}

#[test]
fn a_folder_asked_for_is_listed_when_the_rows_come() {
    let mut duo = Duo::paired();
    duo.phone.list_dir(duo.now, "src", 0);
    assert_eq!(duo.phone.view().files.requested.as_deref(), Some("src"));
    assert!(duo.phone.view().files.loading());
    duo.step(30);
    let (peer, request_id) = duo
        .events
        .iter()
        .find_map(|e| if let HostEvent::ListDir { peer, request_id, path, offset } = e { assert_eq!((path.as_str(), *offset), ("src", 0)); Some((peer.clone(), *request_id)) } else { None })
        .expect("the Mac was asked");
    let rows = Rows {
        request_id,
        dir: "src".into(),
        offset: 0,
        entries: vec![Entry { name: "lib.rs".into(), kind: EntryKind::File, state: FileState::Clean }],
        more: false,
    };
    duo.host.publish_rows(duo.now, &peer, rows);
    duo.step(30);
    let files = &duo.phone.view().files;
    assert_eq!((files.dir.as_str(), files.entries.len(), files.loading(), files.error.clone()), ("src", 1, false, None));
}

#[test]
fn a_folder_outside_the_workspace_is_not_asked_for() {
    let mut rig = Rig::active();
    for path in ["../secrets", "/etc", "a/../../b", "C:\\x"] {
        rig.phone.list_dir(rig.now, path, 0);
        assert_eq!(rig.phone.view().files.error.as_deref(), Some("outside-workspace"), "{path}");
        assert!(!rig.phone.view().files.loading());
    }
    assert!(rig.link.sent().is_empty());
    rig.phone.open_file(rig.now, "../x", None);
    rig.phone.open_file(rig.now, "", None);
    assert!(rig.link.sent().is_empty());
    assert!(rig.phone.diagnostics().iter().any(|d| d.contains("not inside the workspace")));
}

#[test]
fn a_folder_the_mac_does_not_answer_is_an_error_after_ten_seconds_and_the_old_rows_stay() {
    let mut rig = Rig::active();
    rig.link.frame(1, &Frame::Rows(Rows { request_id: 1, dir: String::new(), offset: 0, entries: vec![Entry { name: "a".into(), kind: EntryKind::Dir, state: FileState::None }], more: false }));
    rig.poll();
    rig.phone.list_dir(rig.now, "a", 0);
    rig.advance_alive(REQUEST_TIMEOUT_MS - 100);
    assert!(rig.phone.view().files.loading());
    rig.advance_alive(200);
    assert_eq!(rig.phone.view().files.error.as_deref(), Some("no-answer"));
    assert_eq!(rig.phone.view().files.entries.len(), 1, "the rows on screen stay");
}

#[test]
fn a_folder_asked_for_while_not_connected_is_an_error_at_once() {
    let mut rig = Rig::unpaired();
    rig.phone.list_dir(rig.now, "src", 0);
    assert_eq!(rig.phone.view().files.error.as_deref(), Some("not-connected"));
    assert!(!rig.phone.view().files.loading());
}

#[test]
fn a_folder_asked_for_when_the_link_is_lost_fails_with_the_connection() {
    let mut rig = Rig::active();
    rig.phone.list_dir(rig.now, "src", 0);
    rig.link.say(LinkEvent::Disconnected { peer: 1, reason: "gone".into() });
    rig.poll();
    assert_eq!(rig.phone.view().files.error.as_deref(), Some("connection-lost"));
}

#[test]
fn a_file_is_opened_on_the_screen_the_pen_is_on() {
    let mut duo = Duo::paired();
    duo.phone.open_file(duo.now, "src/lib.rs", Some(42));
    duo.step(30);
    assert!(duo.events.iter().any(|e| matches!(e, HostEvent::OpenFile { path, line: Some(42), screen, .. } if path == "src/lib.rs" && *screen == ScreenId::MAIN)));
}

#[test]
fn a_quick_button_press_reaches_the_mac_with_its_text_and_the_status_comes_back() {
    let mut duo = Duo::paired();
    duo.phone.press_button(duo.now, 1);
    assert_eq!(duo.phone.view().button_status[1], None, "pressed, not answered yet");
    duo.step(30);
    let (peer, request_id, label, text) = duo
        .events
        .iter()
        .find_map(|e| if let HostEvent::ButtonPressed { peer, request_id, label, text, index, .. } = e { assert_eq!(*index, 1); Some((peer.clone(), *request_id, label.clone(), text.clone())) } else { None })
        .expect("the Mac saw the press");
    assert_eq!((label.as_str(), text.as_str()), ("Explain", "Explain what this does, briefly."));
    assert_eq!(duo.phone.view().button_request[1], request_id);
    assert_eq!(duo.phone.view().button_status[1], Some(ReplyStatus::Received), "the host acknowledged it");
    duo.host.reply(duo.now, &peer, request_id, ReplyStatus::Working);
    duo.step(10);
    assert_eq!(duo.phone.view().button_status[1], Some(ReplyStatus::Working));
    duo.host.reply(duo.now, &peer, request_id, ReplyStatus::Done);
    duo.step(10);
    assert_eq!(duo.phone.view().button_status[1], Some(ReplyStatus::Done));
}

#[test]
fn a_press_the_mac_does_not_answer_is_marked_failed_after_ten_seconds() {
    let mut rig = Rig::active();
    rig.phone.press_button(rig.now, 2);
    assert_eq!(count(&rig.link.sent(), "button"), 1);
    rig.advance_alive(REQUEST_TIMEOUT_MS - 200);
    assert_eq!(rig.phone.view().button_status[2], None);
    rig.advance_alive(400);
    assert_eq!(rig.phone.view().button_status[2], Some(ReplyStatus::Failed));
    assert!(rig.phone.diagnostics().iter().any(|d| d.contains("button 3")));
}

#[test]
fn a_press_that_was_answered_is_not_marked_failed_later() {
    let mut rig = Rig::active();
    rig.phone.press_button(rig.now, 0);
    let request_id = rig.phone.view().button_request[0];
    rig.link.frame(1, &Frame::Reply(crate::frame::Reply { request_id, status: ReplyStatus::Done }));
    rig.advance_alive(REQUEST_TIMEOUT_MS + 500);
    assert_eq!(rig.phone.view().button_status[0], Some(ReplyStatus::Done));
}

#[test]
fn a_button_that_does_not_exist_is_not_pressed() {
    let mut rig = Rig::active();
    rig.phone.press_button(rig.now, BUTTON_COUNT);
    rig.phone.press_button(rig.now, usize::MAX);
    assert!(rig.link.sent().is_empty());
}

#[test]
fn the_requests_are_numbered_and_the_numbers_never_pass_through_zero() {
    let mut rig = Rig::active();
    rig.phone.press_button(rig.now, 0);
    rig.phone.press_button(rig.now, 1);
    rig.phone.open_file(rig.now, "a", None);
    let ids: Vec<u32> = rig
        .link
        .sent()
        .iter()
        .filter_map(|f| match f {
            Frame::Button { request_id, .. } | Frame::OpenFile { request_id, .. } => Some(*request_id),
            _ => None,
        })
        .collect();
    assert_eq!(ids, [1, 2, 3]);
    // after the largest number comes 1: 0 means "no request" to the screens
    rig.phone.next_request = u32::MAX;
    assert_eq!(rig.phone.request_id(), u32::MAX);
    assert_eq!(rig.phone.request_id(), 1);
}

#[test]
fn the_buttons_are_saved_cut_to_their_caps_shown_at_once_and_confirmed_by_the_mac() {
    let mut duo = Duo::paired();
    let mut set = ButtonSet::defaults();
    set.buttons[0] = QuickButton { label: "A label that is far too long".into(), text: "x".repeat(MAX_BUTTON_TEXT_CHARS + 20) };
    let caps = duo.phone.set_buttons(duo.now, &set);
    assert_eq!(caps.len(), 2, "the label and the text were cut: {caps:?}");
    assert_eq!(duo.phone.view().buttons.buttons[0].label.chars().count(), MAX_LABEL_CHARS, "shown at once");
    duo.step(30);
    let changed = duo.events.iter().find_map(|e| if let HostEvent::ButtonsChanged { buttons } = e { Some(buttons.clone()) } else { None }).expect("the Mac saved them");
    assert_eq!(changed, duo.phone.view().buttons);
    assert_eq!(duo.host.buttons(), &changed);
    assert_eq!(duo.phone.view().buttons, changed, "and what the Mac sent back is the same");
}

#[test]
fn the_picture_the_pen_names_is_asked_for_once_when_it_does_not_come_and_again_later() {
    let mut rig = Rig::active();
    rig.link.frame(1, &rig_pen(&rig, 1, 5));
    rig.poll();
    rig.advance_alive(THUMB_WAIT_MS - 100);
    assert_eq!(count(&rig.link.sent(), "need thumbnail"), 0, "it may be on its way");
    rig.advance_alive(200);
    assert_eq!(rig.link.sent().iter().filter(|f| **f == Frame::NeedThumbnail { id: 5 }).count(), 1);
    rig.advance_alive(THUMB_RETRY_MS - 500);
    assert_eq!(count(&rig.link.sent(), "need thumbnail"), 1, "not again at once");
    rig.advance_alive(1_000);
    assert_eq!(count(&rig.link.sent(), "need thumbnail"), 2, "again after a while");
}

fn rig_pen(rig: &Rig, seq: u32, thumb_id: u32) -> Frame {
    Frame::PenState(rig.pen(seq, thumb_id))
}

#[test]
fn a_picture_that_arrives_is_not_asked_for() {
    let mut rig = Rig::active();
    rig.link.frame(1, &rig_pen(&rig, 1, 5));
    rig.link.frame(1, &Frame::Thumbnail(Thumbnail { id: 5, width: 1, height: 1, format: ThumbFormat::Rgba8, bytes: vec![0; 4] }));
    rig.poll();
    rig.advance_alive(THUMB_WAIT_MS + THUMB_RETRY_MS + 100);
    assert_eq!(count(&rig.link.sent(), "need thumbnail"), 0);
    // a page with no picture is not waited for either
    rig.link.frame(1, &rig_pen(&rig, 2, 0));
    rig.poll();
    rig.advance_alive(THUMB_WAIT_MS + 100);
    assert_eq!(count(&rig.link.sent(), "need thumbnail"), 0);
}

#[test]
fn what_the_mac_tells_goes_through_to_the_screens_and_says_it_changed() {
    let mut rig = Rig::active();
    rig.poll();
    let pen = PenState { seq: 1, surface: Surface::Page(crate::frame::PageSurface { file: "fc.rs".into(), page_count: 9, ..crate::frame::PageSurface::empty() }), ..PenState::empty() };
    rig.link.frame(1, &Frame::PenState(pen));
    rig.link.frame(1, &Frame::AgentFinished(crate::frame::AgentFinished { conversation_id: 3, file: "fc.rs".into(), line: 10, kind: FinishKind::Done }));
    assert!(rig.poll(), "a poll that took frames says the view changed");
    assert_eq!(rig.phone.view().pen.as_ref().map(PenState::file), Some("fc.rs"));
    assert_eq!(rig.phone.view().finished_count, 1);
    assert!(!rig.poll(), "and the next one, with nothing new, says it did not");
}

#[test]
fn a_client_started_on_a_store_with_a_key_and_no_mac_is_unpaired_and_dials_nothing() {
    let mut rig = Rig::with_store(MemoryStore::with(Stored { secret_key: Some([1; 32]), mac: None }));
    rig.advance(30_000);
    assert_eq!(rig.session(), Session::Unpaired);
    assert!(rig.link.connects().is_empty());
}

#[test]
fn the_wait_between_dials_doubles_and_never_passes_the_ceiling() {
    let waits: Vec<Millis> = (1..=12).map(PhoneClient::<ScriptLink>::backoff).collect();
    assert_eq!(&waits[..5], &[500, 1_000, 2_000, 4_000, 8_000]);
    assert!(waits[5..].iter().all(|w| *w == RECONNECT_MAX_MS), "{waits:?}");
    assert_eq!(PhoneClient::<ScriptLink>::backoff(0), RECONNECT_FIRST_MS, "a count of none is the first wait");
    assert_eq!(PhoneClient::<ScriptLink>::backoff(u32::MAX), RECONNECT_MAX_MS);
}

fn sent_phases(link: &ScriptLink) -> Vec<Phase> {
    link.sent().iter().filter_map(|f| if let Frame::Viewport(ViewportCmd { op: ViewportOp::Gesture { phase, .. }, .. }) = f { Some(*phase) } else { None }).collect()
}

#[test]
fn an_update_that_was_waiting_does_not_come_after_the_end() {
    let mut rig = Rig::active();
    let t = rig.now;
    rig.phone.gesture(t, 1, Phase::Begin, (0.0, 0.0), 1.0, Focus::Pen);
    rig.phone.gesture(t, 1, Phase::Update, (0.1, 0.0), 1.0, Focus::Pen);
    rig.phone.gesture(t, 1, Phase::Update, (0.2, 0.0), 1.0, Focus::Pen);
    rig.phone.gesture(t, 1, Phase::End, (0.3, 0.0), 1.0, Focus::Pen);
    rig.advance(100);
    assert_eq!(sent_phases(&rig.link), [Phase::Begin, Phase::Update, Phase::End], "the end has the totals: the waiting update is older news");
}

#[test]
fn a_page_step_is_held_to_eight_before_it_leaves() {
    let mut rig = Rig::active();
    rig.phone.page_step(rig.now, 100);
    rig.phone.page_step(rig.now, -100);
    rig.phone.page_step(rig.now, 3);
    let steps: Vec<i8> = rig.link.sent().iter().filter_map(|f| if let Frame::Viewport(ViewportCmd { op: ViewportOp::PageStep(n), .. }) = f { Some(*n) } else { None }).collect();
    assert_eq!(steps, [8, -8, 3]);
}

#[test]
fn a_button_that_does_not_exist_is_not_pressed_even_when_the_mac_serves_the_phone() {
    let mut rig = Rig::active();
    rig.phone.press_button(rig.now, BUTTON_COUNT);
    rig.phone.press_button(rig.now, 7);
    assert!(rig.link.sent().is_empty(), "{:?}", rig.link.sent());
    assert_eq!(rig.phone.view().button_status, [None; BUTTON_COUNT]);
    assert!(rig.phone.diagnostics().is_empty(), "it is not even tried (the wire would refuse it): {:?}", rig.phone.diagnostics());
    rig.phone.press_button(rig.now, BUTTON_COUNT - 1);
    assert_eq!(count(&rig.link.sent(), "button"), 1, "the last one that exists is pressed");
}

#[test]
fn a_new_pairing_after_a_connection_is_searching_and_not_lost() {
    let mut rig = Rig::active();
    let other = PairingOffer { ticket: "another-macs-ticket".into(), secret: [4; 16] };
    rig.phone.begin_pairing(rig.now, &other.to_text()).expect("an offer");
    rig.poll();
    assert_eq!(rig.session(), Session::Searching { attempt: 1 }, "it is not the old Mac that is lost: a new one is looked for");
    assert_eq!(rig.link.connects().last().map(String::as_str), Some("another-macs-ticket"));
}

#[test]
fn a_mac_that_announces_itself_does_not_wake_a_phone_that_waits_for_the_user() {
    // the Mac forgot the phone: it can announce itself as often as it likes
    let mut forgotten = Rig::saved();
    forgotten.connect_as(1, MAC_ID);
    forgotten.link.frame(1, &Frame::Refused(Refusal::NotPaired));
    forgotten.poll();
    assert_eq!(forgotten.failed(), Some(FailureKind::Forgotten));
    forgotten.discovery.appear(Nearby::from_announcement(MAC_ID, "", vec!["mac-ticket-2".into()]));
    forgotten.advance(2_000);
    assert_eq!(forgotten.link.connects().len(), 1, "no dial: the Mac said it does not know the phone");
    // the user gave up on the search: the same
    let mut cancelled = Rig::saved();
    cancelled.poll();
    cancelled.phone.cancel(cancelled.now);
    cancelled.discovery.appear(Nearby::from_announcement(MAC_ID, "", vec!["mac-ticket-2".into()]));
    cancelled.advance(2_000);
    assert_eq!(cancelled.link.connects().len(), 1, "no dial: the user stopped the search");
    cancelled.phone.retry(cancelled.now);
    cancelled.poll();
    assert_eq!(cancelled.link.connects().len(), 2, "until the user asks again");
}

#[test]
fn a_mac_that_announces_itself_while_a_dial_is_out_or_the_link_is_up_changes_nothing() {
    let mut rig = Rig::active();
    rig.discovery.appear(Nearby::from_announcement(MAC_ID, "", vec!["mac-ticket-2".into()]));
    rig.advance(1_000);
    assert!(rig.phone.is_active());
    assert_eq!(rig.link.connects().len(), 1, "the link is up: no second dial");
    assert_eq!(rig.phone.addresses(), [MAC_TICKET], "and the address of an announcement is not taken into the saved ones while it is not needed");
}
