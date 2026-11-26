//! The phone's client: the real [`PhoneApi`] over a [`Link`]. A state machine without a thread, a socket or a clock of its own:
//! the app polls it with the time and it answers with what changed, the same way the host on the Mac is polled.
//!
//! WHAT IT DOES FOR THE SCREENS, so that they never see a frame:
//! - PAIRING. [`PhoneApi::begin_pairing`] takes the offer's text (the QR code, a pasted line), dials its ticket, proves the
//!   one-time secret, shows the six-digit code the Mac shows too and waits for the person at the Mac. The Mac is kept (its key, its
//!   name, the addresses it was reached at) in a [`PhoneStore`] and dialed again at every start.
//! - STAYING CONNECTED. A link that drops is dialed again after 0.5 s, then 1, 2, 4, 8 s, trying the saved addresses in turn. After
//!   six failures the phone says the Mac's address probably changed (the screen offers "Pair again") and goes on slowly, every 30 s;
//!   a Mac that announces itself on the local network ([`Discovery`]) is dialed at once. A Mac that does not know the phone any
//!   more, or declines it, or is too old or too new, is not dialed again: the screen says so, [`PhoneApi::retry`] tries once more.
//! - WHAT IT SENDS. A gesture's updates leave at most sixty times a second, newest wins, and its end always leaves; every other
//!   request is reliable, has an id, and is answered or marked failed after ten seconds. Everything is held to the caps the Mac
//!   enforces too (a gesture beyond them is cut with a diagnostic, a path outside the workspace is not sent).
//! - WHAT IT KEEPS FROM THE MAC. Frames go through [`PhoneView::apply_frame`]; the pen state of an old connection is dropped when a
//!   new one begins (the Mac counts from one again); a picture the pen names and that did not arrive is asked for.
//!
//! Nothing here panics on what the Mac (or whoever answers at its address) sends: frames are decoded totally, a frame the Mac may not
//! send is forgiven three times, and the key of whoever answers must be the key the phone was paired with.

use crate::discovery::{Discovery, NearbyEvent, SavedAddresses};
use crate::frame::{
    is_safe_relative_path, negotiate, ButtonSet, Focus, Frame, Hello, PairReject, Phase, Refusal, ReplyStatus, Role, ViewportCmd, ViewportOp,
};
use crate::limits::{
    cut_bytes, Capped, ADDRESS_CHANGED_AFTER, BUTTON_COUNT, CONFIRM_TIMEOUT_MS, DIAL_TIMEOUT_MS, HELLO_TIMEOUT_MS, MAX_GESTURE_PAN,
    MAX_GESTURE_ZOOM, MAX_NAME_BYTES, MIN_GESTURE_ZOOM, MIN_PROTOCOL_VERSION, PEER_TIMEOUT_MS, PING_INTERVAL_MS, PROTOCOL_VERSION,
    RECONNECT_FIRST_MS, RECONNECT_MAX_MS, RECONNECT_SLOW_MS, REQUEST_TIMEOUT_MS, THUMB_RETRY_MS, THUMB_WAIT_MS, VIEWPORT_MAX_HZ,
};
use crate::link::{Link, LinkEvent, Millis, PeerHandle, PERMISSION_PREFIX};
use crate::pace::Pacer;
use crate::pairing::{confirmation_code, is_valid_ticket, proof, OfferError, PairingOffer};
use crate::phone::PhoneApi;
use crate::store::{PhoneStore, SavedMac, Stored};
use crate::view::{FailureKind, PhoneView, Session};
use crate::wire::{decode, encode};

/// How long "Find my Mac" looks when the phone has no pairing code and no Mac saved, before it says it found nothing, in milliseconds.
pub const SEARCH_MS: Millis = 2_500;
/// How many frames that are no frames (or that the Mac may not send) are forgiven before the connection is dropped.
pub const GARBAGE_FORGIVEN: u32 = 3;
/// The newest diagnostics kept.
pub const MAX_DIAGNOSTICS: usize = 64;
/// How many page keys one command may ask for at most (the Mac holds a command to the same).
const MAX_PAGE_STEPS: i8 = 8;

/// How the phone introduces itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientConfig {
    /// The phone's name, for the Mac's screen.
    pub phone_name: String,
    pub app_version: String,
}

impl Default for ClientConfig {
    fn default() -> Self {
        ClientConfig { phone_name: "Phone".to_string(), app_version: "0.1.0".to_string() }
    }
}

/// What the phone is trying to do.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Goal {
    /// No Mac known: the pairing screen.
    Nobody,
    /// An offer was given: dial its ticket, prove the secret, wait for the person at the Mac.
    Pairing(PairingOffer),
    /// A paired Mac: dial it whenever the link is down.
    Saved,
}

/// The connection, as far as it goes.
enum Conn {
    /// No connection, and none being made.
    Down,
    /// A dial is out.
    Dialing { since: Millis },
    /// Connected at the transport.
    Up(Up),
}

/// Where a connection stands in the conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    /// Our hello is out; the Mac's has not come.
    AwaitHello,
    /// The proof is out; the person at the Mac has to confirm the code.
    Pairing { code: u32, since: Millis },
    /// The Mac serves the phone.
    Active,
}

/// One connection.
struct Up {
    peer: PeerHandle,
    /// The Mac's key as the transport authenticated it.
    host_key: String,
    stage: Stage,
    since: Millis,
    last_rx: Millis,
    next_ping_at: Millis,
    ping: Option<(u32, Millis)>,
    next_nonce: u32,
    garbage: u32,
    /// The dialing string the Mac's hello carried (its newest addresses).
    host_ticket: Option<String>,
}

/// One update of a gesture that waits for its turn.
#[derive(Debug, Clone, Copy)]
struct Step {
    id: u32,
    pan: (f32, f32),
    zoom: f32,
    focus: Focus,
}

/// The phone's client over a link.
pub struct PhoneClient<L: Link> {
    link: L,
    config: ClientConfig,
    store: Box<dyn PhoneStore>,
    /// What is kept (the phone's key stays as it was read).
    stored: Stored,
    addresses: SavedAddresses,
    discovery: Option<Box<dyn Discovery>>,
    view: PhoneView,
    changed: bool,
    diagnostics: Vec<String>,
    goal: Goal,
    /// The failure on the screen ends the trying until the user asks again.
    halted: bool,
    conn: Conn,
    /// Dials made since the link was last up (or since the user asked).
    attempt: u32,
    /// The link has been up in this run (a dial that follows is "lost", not "searching").
    was_up: bool,
    next_dial: Option<Millis>,
    /// "Find my Mac" without a pairing code ends then.
    search_until: Option<Millis>,
    /// The Mac announced itself on the local network: the next dial is by its key (the engine finds its address).
    dial_by_key: bool,
    next_request: u32,
    viewport_seq: u32,
    gestures: Pacer<Step>,
    /// The folder asked for must be answered by then.
    list_deadline: Option<Millis>,
    /// The request and the time each quick button's press must be answered by.
    pressed: [Option<(u32, Millis)>; BUTTON_COUNT],
    /// The request that saves the buttons, and its time.
    saving_buttons: Option<(u32, Millis)>,
    /// A picture the pen names that has not arrived: its id and since when.
    thumb_missing: Option<(u32, Millis)>,
    /// The picture asked for last: its id and when.
    thumb_asked: Option<(u32, Millis)>,
    /// What the pad's last gesture was held to (said once per gesture, not once per update).
    held_gesture: Option<u32>,
}

impl<L: Link> PhoneClient<L> {
    /// A client on `link`; what `store` has kept (the Mac, the phone's key) is read at once. A paired Mac is dialed at the first
    /// poll.
    pub fn new(link: L, config: ClientConfig, mut store: Box<dyn PhoneStore>) -> Self {
        let mut diagnostics = Vec::new();
        let stored = match store.load() {
            Ok(stored) => stored,
            Err(why) => {
                diagnostics.push(format!("what the phone kept could not be read, it starts empty: {why}"));
                Stored::default()
            }
        };
        let mut view = PhoneView::new();
        let (goal, addresses) = match &stored.mac {
            Some(mac) => {
                view.mac_name.clone_from(&mac.name);
                view.session = Session::Searching { attempt: 1 };
                (Goal::Saved, SavedAddresses::from_saved(&mac.tickets))
            }
            None => (Goal::Nobody, SavedAddresses::new()),
        };
        let next_dial = matches!(goal, Goal::Saved).then_some(0);
        PhoneClient {
            link,
            config,
            store,
            stored,
            addresses,
            discovery: None,
            view,
            changed: false,
            diagnostics,
            goal,
            halted: false,
            conn: Conn::Down,
            attempt: 0,
            was_up: false,
            next_dial,
            search_until: None,
            dial_by_key: false,
            next_request: 1,
            viewport_seq: 0,
            gestures: Pacer::new(VIEWPORT_MAX_HZ),
            list_deadline: None,
            pressed: [None; BUTTON_COUNT],
            saving_buttons: None,
            thumb_missing: None,
            thumb_asked: None,
            held_gesture: None,
        }
    }

    /// Lets a way of finding the Mac on the local network help: a Mac of the saved key that appears is dialed at once.
    #[must_use]
    pub fn with_discovery(mut self, discovery: Box<dyn Discovery>) -> Self {
        self.discovery = Some(discovery);
        self
    }

    /// What the phone noticed that a person debugging it may want to read (the newest [`MAX_DIAGNOSTICS`], oldest first).
    #[must_use]
    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    /// The diagnostics, and forgets them.
    pub fn take_diagnostics(&mut self) -> Vec<String> {
        std::mem::take(&mut self.diagnostics)
    }

    /// The addresses the phone would dial the Mac at, the newest first.
    #[must_use]
    pub fn addresses(&self) -> &[String] {
        self.addresses.candidates()
    }

    /// Whether the Mac serves the phone now.
    #[must_use]
    pub fn is_active(&self) -> bool {
        matches!(&self.conn, Conn::Up(up) if up.stage == Stage::Active)
    }

    /// Says goodbye and closes the link (the app is closing).
    pub fn close(&mut self, now: Millis) {
        if let Conn::Up(up) = &self.conn {
            let peer = up.peer;
            self.send_raw(now, peer, &Frame::Bye);
        }
        self.link.close();
        self.conn = Conn::Down;
    }

    // ---- the inside: the screen

    fn diag(&mut self, text: String) {
        self.diagnostics.push(text);
        if self.diagnostics.len() > MAX_DIAGNOSTICS {
            self.diagnostics.remove(0);
        }
    }

    fn show(&mut self, session: Session) {
        if self.view.session != session {
            self.view.session = session;
            self.changed = true;
        }
    }

    /// The session while the phone is looking for the Mac it is paired with.
    fn show_trying(&mut self) {
        if matches!(self.view.session, Session::Failed { kind: FailureKind::AddressChanged | FailureKind::NoPermission }) {
            return; // the screen stays on the failure while the phone keeps trying slowly
        }
        let attempt = self.attempt.max(1);
        let mac_name = self.view.mac_name.clone();
        self.show(if self.was_up && self.goal == Goal::Saved { Session::Lost { mac_name, attempt } } else { Session::Searching { attempt } });
    }

    fn persist(&mut self) {
        if let Err(why) = self.store.save(&self.stored) {
            self.diag(format!("what the phone keeps could not be saved: {why}"));
        }
    }

    // ---- the inside: the connection

    /// Ends the trying: the screen says why, `retry` starts again.
    fn halt(&mut self, kind: FailureKind) {
        self.halted = true;
        self.next_dial = None;
        self.drop_connection();
        self.show(Session::Failed { kind });
    }

    /// Closes what is open (without a word to the Mac).
    fn drop_connection(&mut self) {
        if let Conn::Up(up) = &self.conn {
            self.link.disconnect(up.peer);
        }
        self.conn = Conn::Down;
        self.gestures.clear();
        self.thumb_missing = None;
        if self.view.latency_ms.take().is_some() {
            self.changed = true;
        }
    }

    fn backoff(attempt: u32) -> Millis {
        (RECONNECT_FIRST_MS << attempt.saturating_sub(1).min(8)).min(RECONNECT_MAX_MS)
    }

    /// What can be dialed to reach the Mac: the addresses it was reached at (the newest first), then its key: the engine finds a peer of
    /// the local network by its key alone, also after its address changed, so the key is the way back when every address is stale.
    fn dial_candidates(&self) -> Vec<String> {
        let mut candidates = self.addresses.candidates().to_vec();
        if let Some(mac) = &self.stored.mac {
            if !mac.mac_id.is_empty() && !candidates.contains(&mac.mac_id) {
                candidates.push(mac.mac_id.clone());
            }
        }
        candidates
    }

    /// Dials the Mac (the offer's ticket, or the next saved address).
    fn dial(&mut self, now: Millis) {
        self.next_dial = None;
        let ticket = match &self.goal {
            Goal::Pairing(offer) => offer.ticket.clone(),
            Goal::Saved => {
                let candidates = self.dial_candidates();
                let Some(last) = candidates.len().checked_sub(1) else {
                    // no address and no key: only a new pairing helps
                    self.diag("the phone has no address of the Mac".to_string());
                    self.show(Session::Failed { kind: FailureKind::AddressChanged });
                    self.next_dial = Some(now + RECONNECT_SLOW_MS);
                    return;
                };
                // the Mac announced itself: its key is the way (the last candidate); else the addresses are tried in turn
                let index = if std::mem::take(&mut self.dial_by_key) { last } else { self.attempt as usize % candidates.len() };
                candidates[index].clone()
            }
            Goal::Nobody => return,
        };
        self.attempt = self.attempt.saturating_add(1);
        self.show_trying();
        match self.link.connect(now, &ticket) {
            Ok(()) => self.conn = Conn::Dialing { since: now },
            Err(why) => self.dial_failed(now, &why),
        }
    }

    /// A dial did not get through.
    fn dial_failed(&mut self, now: Millis, why: &str) {
        self.conn = Conn::Down;
        self.diag(format!("dialing the Mac failed: {why}"));
        let permission = why.starts_with(PERMISSION_PREFIX);
        match self.goal {
            Goal::Pairing(_) => self.halt(if permission { FailureKind::NoPermission } else { FailureKind::NotFound }),
            Goal::Saved => {
                if permission {
                    self.show(Session::Failed { kind: FailureKind::NoPermission });
                    self.next_dial = Some(now + RECONNECT_SLOW_MS);
                } else if self.attempt >= ADDRESS_CHANGED_AFTER {
                    self.show(Session::Failed { kind: FailureKind::AddressChanged });
                    self.next_dial = Some(now + RECONNECT_SLOW_MS);
                } else {
                    self.next_dial = Some(now + Self::backoff(self.attempt));
                    self.show_trying();
                }
            }
            Goal::Nobody => {}
        }
    }

    /// The link that was up is gone.
    fn connection_lost(&mut self, now: Millis, why: &str) {
        let stage = match &self.conn {
            Conn::Up(up) => up.stage,
            _ => return,
        };
        self.drop_connection();
        self.diag(format!("the connection ended: {why}"));
        if self.goal == Goal::Saved {
            if stage == Stage::Active {
                self.was_up = true;
                self.attempt = 0;
                self.fail_open_requests();
                self.next_dial = Some(now + RECONNECT_FIRST_MS);
                self.show_trying();
            } else {
                self.dial_failed(now, why);
            }
        } else if matches!(self.goal, Goal::Pairing(_)) {
            self.halt(FailureKind::Timeout);
        }
    }

    /// What was asked and cannot be answered now is marked failed.
    fn fail_open_requests(&mut self) {
        if self.view.files.loading() {
            self.view.files_failed("connection-lost");
            self.changed = true;
        }
        self.list_deadline = None;
        for index in 0..BUTTON_COUNT {
            if self.pressed[index].take().is_some() {
                self.view.button_status[index] = Some(ReplyStatus::Failed);
                self.changed = true;
            }
        }
        self.saving_buttons = None;
    }

    fn on_event(&mut self, now: Millis, event: LinkEvent) {
        match event {
            LinkEvent::Ready { .. } => {}
            LinkEvent::Connected { peer, id } => self.on_connected(now, peer, id),
            LinkEvent::Disconnected { peer, reason } => {
                if matches!(&self.conn, Conn::Up(up) if up.peer == peer) {
                    self.connection_lost(now, &reason);
                }
            }
            LinkEvent::Received { peer, bytes, .. } => self.on_received(now, peer, &bytes),
            LinkEvent::Failed { text } => {
                if matches!(self.conn, Conn::Dialing { .. }) {
                    self.dial_failed(now, &text);
                } else {
                    self.diag(format!("the network said: {text}"));
                }
            }
        }
    }

    fn on_connected(&mut self, now: Millis, peer: PeerHandle, id: String) {
        if !matches!(self.conn, Conn::Dialing { .. }) {
            // nobody was dialed (a late answer to a dial that was given up, or a stranger): not served
            self.link.disconnect(peer);
            return;
        }
        if self.goal == Goal::Saved {
            let known = self.stored.mac.as_ref().map(|m| m.mac_id.clone());
            if known.as_deref() != Some(id.as_str()) {
                self.link.disconnect(peer);
                self.dial_failed(now, "another endpoint answered at the address of the Mac");
                return;
            }
        }
        self.conn = Conn::Up(Up {
            peer,
            host_key: id,
            stage: Stage::AwaitHello,
            since: now,
            last_rx: now,
            next_ping_at: now + PING_INTERVAL_MS,
            ping: None,
            next_nonce: 1,
            garbage: 0,
            host_ticket: None,
        });
        let (name, _) = cut_bytes(self.config.phone_name.trim(), MAX_NAME_BYTES, "name");
        let hello = Frame::Hello(Hello {
            role: Role::Phone,
            protocol: PROTOCOL_VERSION,
            min_protocol: MIN_PROTOCOL_VERSION,
            app_version: self.config.app_version.clone(),
            name: if name.is_empty() { "phone".to_string() } else { name },
            ticket: String::new(),
        });
        self.send_raw(now, peer, &hello);
    }

    fn on_received(&mut self, now: Millis, peer: PeerHandle, bytes: &[u8]) {
        let Conn::Up(up) = &mut self.conn else {
            return;
        };
        if up.peer != peer {
            return;
        }
        up.last_rx = now;
        let frame = match decode(bytes) {
            Ok(frame) => frame,
            Err(error) => {
                self.diag(format!("a frame from the Mac was not understood ({error}, {} bytes)", bytes.len()));
                self.garbage(now);
                return;
            }
        };
        if frame.is_phone_to_host() {
            self.diag(format!("the Mac sent a {}, which only a phone may send: ignored", frame.name()));
            self.garbage(now);
            return;
        }
        self.on_frame(now, frame);
    }

    /// A frame that is no frame, or one the Mac may not send: forgiven a few times, then the connection ends.
    fn garbage(&mut self, now: Millis) {
        let too_many = match &mut self.conn {
            Conn::Up(up) => {
                up.garbage += 1;
                up.garbage > GARBAGE_FORGIVEN
            }
            _ => false,
        };
        if too_many {
            if let Conn::Up(up) = &self.conn {
                let peer = up.peer;
                self.send_raw(now, peer, &Frame::Bye);
            }
            self.connection_lost(now, "it keeps sending what is no frame");
        }
    }

    fn on_frame(&mut self, now: Millis, frame: Frame) {
        let Conn::Up(up) = &mut self.conn else {
            return;
        };
        let (peer, stage) = (up.peer, up.stage);
        match frame {
            Frame::Bye => {
                self.connection_lost(now, "the Mac said goodbye");
            }
            Frame::Ping { nonce } => {
                self.send_raw(now, peer, &Frame::Pong { nonce });
            }
            Frame::Pong { nonce } => {
                if let Some((sent_nonce, sent_at)) = up.ping {
                    if sent_nonce == nonce {
                        up.ping = None;
                        let rtt = u32::try_from(now.saturating_sub(sent_at)).unwrap_or(u32::MAX);
                        if self.view.latency_ms != Some(rtt) {
                            self.view.latency_ms = Some(rtt);
                            self.changed = true;
                        }
                    }
                }
            }
            Frame::Refused(why) => self.on_refused(why),
            Frame::Hello(hello) if stage == Stage::AwaitHello => self.on_hello(now, &hello),
            Frame::PairAccepted { code } => {
                if let Stage::Pairing { code: ours, .. } = stage {
                    if code == ours {
                        self.finish_pairing(now);
                    } else {
                        self.diag(format!("the Mac's pairing code ({code}) is not the phone's ({ours}): not paired"));
                        self.halt(FailureKind::WrongCode);
                    }
                }
            }
            Frame::PairRejected(why) => {
                if matches!(stage, Stage::Pairing { .. }) {
                    self.halt(match why {
                        PairReject::WrongSecret => FailureKind::WrongCode,
                        PairReject::Expired | PairReject::NoPairingOpen => FailureKind::Expired,
                        PairReject::Busy | PairReject::TooManyPhones => FailureKind::Busy,
                        PairReject::Declined => FailureKind::Declined,
                    });
                }
            }
            Frame::Hello(_) => {} // a second hello changes nothing
            other => match stage {
                Stage::Active => {
                    self.on_data(&other);
                }
                Stage::Pairing { .. } if matches!(other, Frame::Settings(_)) => {
                    // the Mac knew this phone already and served it without a confirmation: it is paired
                    self.finish_pairing(now);
                    self.on_data(&other);
                }
                _ => {
                    self.diag(format!("the Mac sent a {} before it served the phone: ignored", other.name()));
                    self.garbage(now);
                }
            },
        }
    }

    /// A frame of the Mac's that tells the phone something.
    fn on_data(&mut self, frame: &Frame) {
        if let Frame::Reply(reply) = frame {
            for slot in &mut self.pressed {
                if slot.is_some_and(|(id, _)| id == reply.request_id) {
                    *slot = None;
                }
            }
            if self.saving_buttons.is_some_and(|(id, _)| id == reply.request_id) {
                self.saving_buttons = None;
            }
        }
        if self.view.apply_frame(frame) {
            self.changed = true;
        }
    }

    fn on_refused(&mut self, why: Refusal) {
        let kind = match why {
            Refusal::NotPaired => match self.goal {
                Goal::Pairing(_) => FailureKind::Expired,
                _ => FailureKind::Forgotten,
            },
            Refusal::UpdateCompanion => FailureKind::UpdateCompanion,
            Refusal::UpdateAzReview => FailureKind::UpdateAzReview,
            Refusal::UnsupportedRole => FailureKind::NotFound,
        };
        self.halt(kind);
    }

    fn on_hello(&mut self, now: Millis, hello: &Hello) {
        let Conn::Up(up) = &mut self.conn else {
            return;
        };
        if hello.role != Role::Host {
            self.diag(format!("{:?} answered at the address of the Mac, not AzReview's host", hello.role));
            self.halt(FailureKind::NotFound);
            return;
        }
        if let Err(refusal) = negotiate(Role::Phone, (PROTOCOL_VERSION, MIN_PROTOCOL_VERSION), hello) {
            let peer = up.peer;
            self.send_raw(now, peer, &Frame::Bye);
            self.on_refused(refusal);
            return;
        }
        let (name, _) = cut_bytes(hello.name.trim(), MAX_NAME_BYTES, "name");
        if !name.is_empty() && name != self.view.mac_name {
            self.view.mac_name = name;
            self.changed = true;
        }
        let ticket = Some(hello.ticket.clone()).filter(|t| is_valid_ticket(t));
        let (peer, host_key) = (up.peer, up.host_key.clone());
        up.host_ticket.clone_from(&ticket);
        match self.goal.clone() {
            Goal::Pairing(offer) => {
                let phone_key = self.link.local_id();
                let code = confirmation_code(&offer.secret, &host_key, &phone_key);
                let request = Frame::PairRequest { proof: proof(&offer.secret, &host_key, &phone_key), phone_name: self.phone_name() };
                if self.send_raw(now, peer, &request) {
                    if let Conn::Up(up) = &mut self.conn {
                        up.stage = Stage::Pairing { code, since: now };
                    }
                    let mac_name = self.view.mac_name.clone();
                    self.show(Session::Pairing { mac_name, code });
                }
            }
            Goal::Saved => {
                self.remember_mac(&host_key, ticket.as_deref());
                self.enter_active(now);
            }
            Goal::Nobody => {}
        }
    }

    fn phone_name(&self) -> String {
        let (name, _) = cut_bytes(self.config.phone_name.trim(), MAX_NAME_BYTES, "name");
        if name.is_empty() {
            "phone".to_string()
        } else {
            name
        }
    }

    /// Keeps what the Mac's hello said about itself: its name, and its newest address.
    fn remember_mac(&mut self, host_key: &str, ticket: Option<&str>) {
        let name = self.view.mac_name.clone();
        let mut dirty = false;
        if let Some(ticket) = ticket {
            if self.addresses.candidates().first().map(String::as_str) != Some(ticket) {
                self.addresses.remember(ticket);
                dirty = true;
            }
        }
        let tickets = self.addresses.candidates().to_vec();
        match self.stored.mac.as_mut() {
            Some(mac) if mac.mac_id == host_key => {
                if mac.name != name || mac.tickets != tickets {
                    mac.name = name;
                    mac.tickets = tickets;
                    dirty = true;
                }
            }
            _ => {
                self.stored.mac = Some(SavedMac { mac_id: host_key.to_string(), name, tickets });
                dirty = true;
            }
        }
        if dirty {
            self.persist();
        }
    }

    /// The person at the Mac said yes: the Mac is kept.
    fn finish_pairing(&mut self, now: Millis) {
        let (host_key, host_ticket) = match &self.conn {
            Conn::Up(up) => (up.host_key.clone(), up.host_ticket.clone()),
            _ => return,
        };
        let Goal::Pairing(offer) = self.goal.clone() else {
            return;
        };
        let mut addresses = SavedAddresses::new();
        addresses.remember(&offer.ticket);
        if let Some(ticket) = &host_ticket {
            addresses.remember(ticket);
        }
        self.addresses = addresses;
        self.stored.mac = Some(SavedMac { mac_id: host_key, name: self.view.mac_name.clone(), tickets: self.addresses.candidates().to_vec() });
        self.persist();
        self.goal = Goal::Saved;
        self.enter_active(now);
    }

    /// The Mac serves the phone: it is told what the phone looks at, and the screens go to Connected.
    fn enter_active(&mut self, now: Millis) {
        if let Conn::Up(up) = &mut self.conn {
            up.stage = Stage::Active;
            up.next_ping_at = now;
        }
        self.halted = false;
        self.attempt = 0;
        self.was_up = true;
        self.next_dial = None;
        // the Mac counts its pen states from one again on a new connection: the old ones would be taken for newer
        self.view.pen = None;
        let mac_name = self.view.mac_name.clone();
        self.show(Session::Connected { mac_name });
        self.changed = true;
    }

    // ---- the inside: what goes out

    /// Writes a frame to a connection. False when it could not be written or sent.
    fn send_raw(&mut self, now: Millis, peer: PeerHandle, frame: &Frame) -> bool {
        match encode(frame) {
            Ok(bytes) => self.link.send(now, peer, &bytes, frame.delivery()),
            Err(error) => {
                self.diag(format!("a {} could not be sent: {error}", frame.name()));
                false
            }
        }
    }

    /// Sends to the Mac, when it serves the phone.
    fn send_active(&mut self, now: Millis, frame: &Frame) -> bool {
        match &self.conn {
            Conn::Up(up) if up.stage == Stage::Active => {
                let peer = up.peer;
                self.send_raw(now, peer, frame)
            }
            _ => false,
        }
    }

    fn request_id(&mut self) -> u32 {
        let id = self.next_request;
        self.next_request = if id == u32::MAX { 1 } else { id + 1 };
        id
    }

    fn viewport(&mut self, now: Millis, op: ViewportOp) {
        self.viewport_seq = self.viewport_seq.wrapping_add(1);
        let cmd = ViewportCmd { screen: self.view.screen(), seq: self.viewport_seq, op };
        self.send_active(now, &Frame::Viewport(cmd));
    }

    fn send_step(&mut self, now: Millis, id: u32, phase: Phase, pan: (f32, f32), zoom: f32, focus: Focus) {
        self.viewport(now, ViewportOp::Gesture { id, phase, pan_x: pan.0, pan_y: pan.1, zoom, focus });
    }

    /// The update that waited for its turn goes if its turn has come.
    fn send_due_step(&mut self, now: Millis) {
        if let Some(step) = self.gestures.take(now) {
            self.send_step(now, step.id, Phase::Update, step.pan, step.zoom, step.focus);
        }
    }

    // ---- the inside: time

    fn discovery_news(&mut self, now: Millis) {
        let Some(discovery) = self.discovery.as_mut() else {
            return;
        };
        let news = discovery.poll(now);
        let known = self.stored.mac.as_ref().map(|m| m.mac_id.clone());
        for event in news {
            let NearbyEvent::Appeared(near) = event else {
                continue;
            };
            if self.goal != Goal::Saved || self.halted || Some(&near.endpoint_id) != known.as_ref() || !matches!(self.conn, Conn::Down) {
                continue;
            }
            // the Mac announced itself: the wait is over, and its key is the way (the engine knows its address now)
            self.dial_by_key = true;
            self.attempt = 0;
            self.next_dial = Some(now);
        }
    }

    fn tick(&mut self, now: Millis) {
        if self.search_until.is_some_and(|until| now >= until) {
            self.search_until = None;
            self.show(Session::Failed { kind: FailureKind::NotFound });
        }
        if self.next_dial.is_some_and(|at| now >= at) && matches!(self.conn, Conn::Down) && !self.halted {
            self.dial(now);
        }
        if let Conn::Dialing { since } = self.conn {
            if now.saturating_sub(since) > DIAL_TIMEOUT_MS {
                self.dial_failed(now, "no answer in time");
            }
        }
        let up = if let Conn::Up(up) = &self.conn { Some((up.stage, up.since, up.last_rx, up.next_ping_at, up.peer)) } else { None };
        if let Some((stage, since, last_rx, next_ping_at, peer)) = up {
            self.tick_up(now, stage, since, last_rx, next_ping_at, peer);
        }
        if self.is_active() {
            self.tick_active(now);
        }
    }

    fn tick_up(&mut self, now: Millis, stage: Stage, since: Millis, last_rx: Millis, next_ping_at: Millis, peer: PeerHandle) {
        if stage == Stage::AwaitHello && now.saturating_sub(since) > HELLO_TIMEOUT_MS {
            self.send_raw(now, peer, &Frame::Bye);
            self.connection_lost(now, "the Mac said no hello");
            return;
        }
        if let Stage::Pairing { since, .. } = stage {
            if now.saturating_sub(since) > CONFIRM_TIMEOUT_MS + HELLO_TIMEOUT_MS {
                self.send_raw(now, peer, &Frame::Bye);
                self.halt(FailureKind::Timeout);
                return;
            }
        }
        if now.saturating_sub(last_rx) > PEER_TIMEOUT_MS {
            self.connection_lost(now, "no answer for 8 s");
            return;
        }
        if stage != Stage::AwaitHello && now >= next_ping_at {
            let nonce = match &mut self.conn {
                Conn::Up(up) => {
                    let nonce = up.next_nonce;
                    up.next_nonce = nonce.wrapping_add(1);
                    up.ping = Some((nonce, now));
                    up.next_ping_at = now + PING_INTERVAL_MS;
                    nonce
                }
                _ => return,
            };
            self.send_raw(now, peer, &Frame::Ping { nonce });
        }
    }

    fn tick_active(&mut self, now: Millis) {
        self.send_due_step(now);
        if self.list_deadline.is_some_and(|at| now >= at) {
            self.list_deadline = None;
            if self.view.files.loading() {
                self.view.files_failed("no-answer");
                self.changed = true;
            }
        }
        for index in 0..BUTTON_COUNT {
            if self.pressed[index].is_some_and(|(_, at)| now >= at) {
                self.pressed[index] = None;
                self.view.button_status[index] = Some(ReplyStatus::Failed);
                self.changed = true;
                self.diag(format!("the Mac did not answer the press of button {}", index + 1));
            }
        }
        if self.saving_buttons.is_some_and(|(_, at)| now >= at) {
            self.saving_buttons = None;
            self.diag("the Mac did not confirm that it saved the buttons".to_string());
        }
        self.ask_for_picture(now);
    }

    /// The pen names a picture that did not come with it: it is asked for once it is clear that it is not on its way.
    fn ask_for_picture(&mut self, now: Millis) {
        let wanted = self.view.pen.as_ref().filter(|pen| pen.thumb_id != 0 && !self.view.thumb_is_current()).map(|pen| pen.thumb_id);
        let Some(id) = wanted else {
            self.thumb_missing = None;
            return;
        };
        let since = match self.thumb_missing {
            Some((missing, since)) if missing == id => since,
            _ => {
                self.thumb_missing = Some((id, now));
                now
            }
        };
        let asked_lately = self.thumb_asked.is_some_and(|(asked, at)| asked == id && now.saturating_sub(at) < THUMB_RETRY_MS);
        if now.saturating_sub(since) >= THUMB_WAIT_MS && !asked_lately {
            self.thumb_asked = Some((id, now));
            self.send_active(now, &Frame::NeedThumbnail { id });
        }
    }

    /// The pad's numbers held to the limits the Mac holds them to (a gesture beyond them says so once).
    fn held(&mut self, id: u32, pan: (f32, f32), zoom: f32) -> ((f32, f32), f32) {
        // a number that is no number is nothing (no pan, no zoom); an infinity is the limit
        let hold = |value: f32, fallback: f32, least: f32, most: f32| if value.is_nan() { fallback } else { value.clamp(least, most) };
        let held = (
            (hold(pan.0, 0.0, -MAX_GESTURE_PAN, MAX_GESTURE_PAN), hold(pan.1, 0.0, -MAX_GESTURE_PAN, MAX_GESTURE_PAN)),
            hold(zoom, 1.0, MIN_GESTURE_ZOOM, MAX_GESTURE_ZOOM),
        );
        // (a number that was none differs from itself too, so it is said as well)
        if held != (pan, zoom) {
            self.say_held(id);
        }
        held
    }

    fn say_held(&mut self, id: u32) {
        if self.held_gesture != Some(id) {
            self.held_gesture = Some(id);
            self.diag("a gesture went beyond the limits and was held to them".to_string());
        }
    }
}

impl<L: Link> PhoneApi for PhoneClient<L> {
    fn view(&self) -> &PhoneView {
        &self.view
    }

    fn poll(&mut self, now: Millis) -> bool {
        for event in self.link.poll(now) {
            self.on_event(now, event);
        }
        self.discovery_news(now);
        self.tick(now);
        std::mem::take(&mut self.changed)
    }

    fn gesture(&mut self, now: Millis, id: u32, phase: Phase, pan: (f32, f32), zoom: f32, focus: Focus) {
        if !self.is_active() {
            return;
        }
        let (pan, zoom) = self.held(id, pan, zoom);
        match phase {
            Phase::Begin => {
                self.gestures.clear();
                self.send_step(now, id, Phase::Begin, pan, zoom, focus);
            }
            Phase::Update => {
                self.gestures.offer(Step { id, pan, zoom, focus });
                self.send_due_step(now);
            }
            Phase::End => {
                // the end carries the totals: a waiting update would only be older news
                self.gestures.clear();
                self.send_step(now, id, Phase::End, pan, zoom, focus);
            }
        }
    }

    fn fit(&mut self, now: Millis) {
        self.viewport(now, ViewportOp::Fit);
    }

    fn page_step(&mut self, now: Millis, step: i8) {
        self.viewport(now, ViewportOp::PageStep(step.clamp(-MAX_PAGE_STEPS, MAX_PAGE_STEPS)));
    }

    fn reset_view(&mut self, now: Millis) {
        self.viewport(now, ViewportOp::Reset);
    }

    fn continue_reviewing(&mut self, now: Millis) {
        let screen = self.view.screen();
        self.send_active(now, &Frame::ContinueReviewing { screen });
    }

    fn list_dir(&mut self, now: Millis, path: &str, offset: u32) {
        self.view.begin_listing(path);
        self.changed = true;
        if !is_safe_relative_path(path) {
            self.view.files_failed("outside-workspace");
            return;
        }
        let request_id = self.request_id();
        if self.send_active(now, &Frame::ListDir { request_id, path: path.to_string(), offset }) {
            self.list_deadline = Some(now + REQUEST_TIMEOUT_MS);
        } else {
            self.view.files_failed("not-connected");
        }
    }

    fn open_file(&mut self, now: Millis, path: &str, line: Option<u32>) {
        if path.is_empty() || !is_safe_relative_path(path) {
            self.diag(format!("a file that is not inside the workspace was not asked for: {path:?}"));
            return;
        }
        let request_id = self.request_id();
        let screen = self.view.screen();
        self.send_active(now, &Frame::OpenFile { screen, request_id, path: path.to_string(), line });
    }

    fn press_button(&mut self, now: Millis, index: usize) {
        if index >= BUTTON_COUNT || !self.is_active() {
            return;
        }
        let request_id = self.request_id();
        let screen = self.view.screen();
        // the index is below BUTTON_COUNT (4): it fits a byte
        if self.send_active(now, &Frame::Button { screen, request_id, index: index as u8 }) {
            self.view.button_pressed(index, request_id);
            self.pressed[index] = Some((request_id, now + REQUEST_TIMEOUT_MS));
            self.changed = true;
        }
    }

    fn set_buttons(&mut self, now: Millis, buttons: &ButtonSet) -> Vec<Capped> {
        let (clean, caps) = buttons.sanitized();
        let request_id = self.request_id();
        if self.send_active(now, &Frame::SetButtons { request_id, buttons: clean.clone() }) {
            self.saving_buttons = Some((request_id, now + REQUEST_TIMEOUT_MS));
            // what was saved is what the buttons are, before the Mac says so too
            if self.view.buttons != clean {
                self.view.buttons = clean;
                self.changed = true;
            }
        } else {
            self.diag("the buttons were not saved: the phone is not connected to the Mac".to_string());
        }
        caps
    }

    fn begin_pairing(&mut self, now: Millis, offer: &str) -> Result<(), OfferError> {
        let offer = PairingOffer::parse(offer)?;
        self.drop_connection();
        self.goal = Goal::Pairing(offer);
        self.halted = false;
        self.attempt = 0;
        self.search_until = None;
        self.dial_by_key = false;
        self.next_dial = Some(now);
        self.show(Session::Searching { attempt: 1 });
        Ok(())
    }

    fn retry(&mut self, now: Millis) {
        self.halted = false;
        self.drop_connection();
        match self.goal {
            Goal::Nobody => {
                // "Find my Mac" with no code and no Mac saved: nothing can be paired from here, and it says so after a look
                self.search_until = Some(now + SEARCH_MS);
                self.show(Session::Searching { attempt: 1 });
            }
            Goal::Pairing(_) | Goal::Saved => {
                self.attempt = 0;
                self.next_dial = Some(now);
                self.show(Session::Searching { attempt: 1 });
            }
        }
    }

    fn cancel(&mut self, now: Millis) {
        if let Conn::Up(up) = &self.conn {
            let peer = up.peer;
            self.send_raw(now, peer, &Frame::Bye);
        }
        self.drop_connection();
        self.search_until = None;
        self.next_dial = None;
        self.attempt = 0;
        match self.goal {
            // a pairing that is given up leaves the Mac the phone was paired with (if any) as it was, and looks for it again
            Goal::Pairing(_) if self.stored.mac.is_some() => {
                self.goal = Goal::Saved;
                self.halted = false;
                self.next_dial = Some(now);
                // the name of the Mac the aborted pairing met is not the name of the one the phone is paired with
                if let Some(mac) = &self.stored.mac {
                    self.view.mac_name.clone_from(&mac.name);
                }
                self.show(Session::Searching { attempt: 1 });
            }
            Goal::Pairing(_) | Goal::Nobody => {
                self.goal = Goal::Nobody;
                self.halted = false;
                self.show(Session::Unpaired);
            }
            // the search for the paired Mac stops where it is: the screen says it was not found, "try again" starts it
            Goal::Saved => {
                self.halted = true;
                self.show(Session::Failed { kind: FailureKind::NotFound });
            }
        }
    }

    fn forget_mac(&mut self, now: Millis) {
        if let Conn::Up(up) = &self.conn {
            let peer = up.peer;
            self.send_raw(now, peer, &Frame::Bye);
        }
        self.drop_connection();
        self.goal = Goal::Nobody;
        self.halted = false;
        self.attempt = 0;
        self.dial_by_key = false;
        self.was_up = false;
        self.next_dial = None;
        self.search_until = None;
        self.stored.mac = None;
        self.addresses.clear();
        self.persist();
        self.pressed = [None; BUTTON_COUNT];
        self.saving_buttons = None;
        self.list_deadline = None;
        self.view = PhoneView::new();
        self.changed = true;
    }
}

#[cfg(test)]
#[path = "client_test.rs"]
mod client_test;
