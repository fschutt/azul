//! The Mac's side of the conversation: AzReview's companion host. A pure state machine over a [`Link`]: no UI, no app state, no
//! global state. AzReview polls it with a clock, feeds it what the Mac knows (the pen, the picture of the surface, the rows of a
//! folder, the buttons, the settings, an agent that finished) and gets [`HostEvent`]s back: a viewport command, a file to open,
//! a folder to list, a quick button pressed (with its text), a peer asking to be paired (with the code to show), a peer gone,
//! "Continue reviewing".
//!
//! What it takes care of, so AzReview does not have to:
//! - WHO MAY TALK. A key that is not paired is refused before it can say anything, unless a pairing window is open, and then it
//!   may only pair. The keys come from the transport, which has authenticated them. What a paired peer may command is
//!   [`may_command`](crate::access::may_command) (a screen it does not control is refused with a diagnostic).
//! - PAIRING. An offer (the ticket and a one-time secret), the proof the peer must give, the six-digit code both screens show,
//!   the person's confirmation, the secret spent, at most four peers, a wrong proof three times closes the window.
//! - WHAT A PEER SENDS. Every frame is decoded totally, checked for direction, and each request is held to its caps (paths that
//!   stay inside the workspace, button presses at most eight a second, text cut to its length, a peer that keeps sending what
//!   is no frame is dropped).
//! - GESTURES. The peer's drag and pinch come as cumulative totals in two kinds of frame; the host puts them in order (a stale
//!   one is dropped, an update that comes before its begin gets one, a gesture that goes quiet is ended) and AzReview sees a
//!   clean Begin / Update / End per gesture with totals it can apply to the view the gesture began on.
//! - WHAT GOES OUT. The pen state is sent at most thirty times a second, newest wins, only when it changed; the picture of the
//!   surface once per peer and again when asked; replies, rows and notices to the peer they are for; the settings and the
//!   buttons whenever they change.

use crate::access::{may_command, ScreenId, ScreenInfo, TrustRecord};
use crate::frame::{
    is_safe_relative_path, negotiate, AgentFinished, ButtonSet, Frame, Hello, HostSettings, Notice, PairReject, PenState, Phase,
    Refusal, Reply, ReplyStatus, Role, Rows, Thumbnail, ViewportCmd, ViewportOp,
};
use crate::limits::{
    cut_bytes, Capped, BUTTONS_PER_SECOND, CONFIRM_TIMEOUT_MS, HELLO_TIMEOUT_MS, MAX_GESTURE_PAN, MAX_GESTURE_ZOOM, MAX_NAME_BYTES,
    MAX_NOTICE_BYTES, MAX_PAIRED_PHONES, MAX_PATH_BYTES, MIN_GESTURE_ZOOM, MIN_PROTOCOL_VERSION, PAIRING_MAX_WRONG_PROOFS,
    PAIRING_WINDOW_MS, PEER_TIMEOUT_MS, PEN_STATE_MAX_HZ, PING_INTERVAL_MS, PROTOCOL_VERSION,
};
use crate::link::{Delivery, Link, LinkEvent, Millis, PeerHandle};
use crate::pace::Pacer;
use crate::pairing::{confirmation_code, new_secret, verify_proof, Entropy, PairingOffer};
use crate::wire::{decode, encode};

/// How long a gesture may stay silent before the host ends it at its last totals, in milliseconds.
pub const GESTURE_QUIET_MS: Millis = 500;
/// How many frames that are no frames (or that this peer may not send) are forgiven before the connection is dropped.
pub const GARBAGE_FORGIVEN: u32 = 3;
/// How many page keys one command may ask for at most.
pub const MAX_PAGE_STEPS: i8 = 8;
/// How many requests of a peer the host remembers, so that a repeated one is the same one.
const REMEMBERED_REQUESTS: usize = 32;

/// How the host introduces itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostConfig {
    /// The Mac's name, for the phone's screen.
    pub mac_name: String,
    pub app_version: String,
    /// Peers that may be paired.
    pub max_phones: usize,
}

impl Default for HostConfig {
    fn default() -> Self {
        HostConfig { mac_name: "Mac".to_string(), app_version: "0.1.0".to_string(), max_phones: MAX_PAIRED_PHONES }
    }
}

/// A paired peer and whether it is here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhoneInfo {
    pub id: String,
    pub name: String,
    pub connected: bool,
    pub rtt_ms: Option<u32>,
}

/// What the host tells AzReview.
#[derive(Debug, Clone, PartialEq)]
pub enum HostEvent {
    /// The endpoint knows its address: its ticket (a QR code made before this has a stale one).
    Ready { ticket: String },
    /// A peer that is not paired proved the secret: show `code`, ask the person, then `accept_pairing` or `reject_pairing`.
    PairingRequested { peer: String, name: String, code: u32 },
    /// A peer was paired (the person said yes): keep `paired()` in the settings.
    Paired { record: TrustRecord },
    /// A paired peer is here and has said hello.
    PeerConnected { peer: String, name: String, role: Role },
    PeerGone { peer: String, reason: String },
    /// A gesture's step, or a fit, a page step, a reset, for a screen.
    Viewport { peer: String, screen: ScreenId, op: ViewportOp },
    ListDir { peer: String, request_id: u32, path: String, offset: u32 },
    OpenFile { peer: String, screen: ScreenId, request_id: u32, path: String, line: Option<u32> },
    /// A quick button was pressed: its label and instruction as the host holds them now. The host has answered `Received`.
    ButtonPressed { peer: String, screen: ScreenId, index: usize, label: String, text: String, request_id: u32 },
    /// A peer saved the buttons (already cut to their caps and kept here; sent on to the other peers).
    ButtonsChanged { buttons: ButtonSet },
    /// "Continue reviewing": go back to the return point of the screen.
    ContinueReviewing { peer: String, screen: ScreenId },
    /// Something the user may want to see: a cap that was hit, a refused connection, a frame that was not understood.
    Diagnostic(String),
}

/// Why a confirmation did not pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmError {
    /// No peer is waiting for it (it left, or the time ran out).
    NobodyWaiting,
    /// The peer waiting is not the one named.
    NotThatPhone,
    /// Four peers are paired already.
    TooManyPhones,
}

/// Where a connection stands.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Phase_ {
    /// The key is paired (or a pairing window is open): the peer has to say hello.
    AwaitHello { since: Millis },
    /// A key that is not paired, in a pairing window: it may only pair.
    Pairing,
    /// Hello said, versions agreed: the peer is served.
    Active,
}

/// A gesture in progress, as far as the host has seen it.
#[derive(Debug, Clone, Copy)]
struct Gesture {
    id: u32,
    pan: (f32, f32),
    zoom: f32,
    focus: crate::frame::Focus,
    last_at: Millis,
}

/// One connection.
struct Peer {
    handle: PeerHandle,
    id: String,
    name: String,
    role: Role,
    phase: Phase_,
    last_rx: Millis,
    /// The host has said hello to it.
    greeted: bool,
    garbage: u32,
    /// The last viewport command number taken (the order of the peer's commands).
    last_seq: Option<u32>,
    gesture: Option<Gesture>,
    last_ended: Option<u32>,
    ping: Option<(u32, Millis)>,
    next_ping_at: Millis,
    next_nonce: u32,
    rtt_ms: Option<u32>,
    button_times: Vec<Millis>,
    requests: Vec<u32>,
    pen_pacer: Pacer<PenState>,
    pen_seq: u32,
    last_pen: Option<PenState>,
    thumb_sent: u32,
}

/// The pairing window: one secret, open for a while.
struct Window {
    secret: [u8; crate::limits::PAIRING_SECRET_BYTES],
    opened_at: Millis,
    wrong: u32,
    waiting: Option<Waiting>,
}

/// A peer that proved the secret and waits for the person at the Mac.
struct Waiting {
    peer: String,
    name: String,
    code: u32,
    since: Millis,
}

/// The Mac's companion host over a link.
pub struct CompanionHost<L: Link> {
    link: L,
    config: HostConfig,
    entropy: Box<dyn Entropy>,
    node_id: String,
    ticket: String,
    ready_said: bool,
    trust: Vec<TrustRecord>,
    peers: Vec<Peer>,
    window: Option<Window>,
    buttons: ButtonSet,
    settings: HostSettings,
    pen: Option<PenState>,
    thumb: Option<Thumbnail>,
    screens: Vec<ScreenInfo>,
    events: Vec<HostEvent>,
    /// The clock at the last poll (what the calls that carry no time of their own use).
    last_poll: Millis,
}

impl<L: Link> CompanionHost<L> {
    /// A host on `link`; `entropy` makes the one-time secrets.
    pub fn new(link: L, config: HostConfig, entropy: Box<dyn Entropy>) -> Self {
        let node_id = link.local_id();
        let main = ScreenInfo { id: ScreenId::MAIN, owner: node_id.clone(), controller: node_id.clone(), role: Role::Review };
        CompanionHost {
            link,
            config,
            entropy,
            node_id,
            ticket: String::new(),
            ready_said: false,
            trust: Vec::new(),
            peers: Vec::new(),
            window: None,
            buttons: ButtonSet::defaults(),
            settings: HostSettings::default(),
            pen: None,
            thumb: None,
            screens: vec![main],
            events: Vec::new(),
            last_poll: 0,
        }
    }

    /// The host's own key (the node's identity).
    #[must_use]
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// How a peer dials the host now (empty until the endpoint knows its addresses).
    #[must_use]
    pub fn ticket(&self) -> &str {
        &self.ticket
    }

    /// The screens AzReview has (the main review screen until it says more).
    pub fn set_screens(&mut self, screens: Vec<ScreenInfo>) {
        self.screens = screens;
    }

    /// The peers AzReview kept from the last run (at most the cap; the rest is left out and said).
    pub fn load_paired(&mut self, records: Vec<TrustRecord>) {
        let cap = self.config.max_phones;
        if records.len() > cap {
            self.events.push(HostEvent::Diagnostic(format!("{} peers were paired and only {cap} are kept", records.len())));
        }
        self.trust = records.into_iter().take(cap).collect();
    }

    /// The paired peers, for the settings.
    #[must_use]
    pub fn paired(&self) -> &[TrustRecord] {
        &self.trust
    }

    /// The paired peers and whether each is here.
    #[must_use]
    pub fn phones(&self) -> Vec<PhoneInfo> {
        self.trust
            .iter()
            .map(|record| {
                let live = self.peers.iter().find(|p| p.id == record.peer && p.phase == Phase_::Active);
                PhoneInfo { id: record.peer.clone(), name: record.name.clone(), connected: live.is_some(), rtt_ms: live.and_then(|p| p.rtt_ms) }
            })
            .collect()
    }

    /// The four buttons as the host holds them.
    #[must_use]
    pub fn buttons(&self) -> &ButtonSet {
        &self.buttons
    }

    /// The Mac's own change of the buttons (the settings): cut to the caps, kept, sent to every peer here. What was cut.
    pub fn set_buttons(&mut self, now: Millis, buttons: &ButtonSet) -> Vec<Capped> {
        let (clean, caps) = buttons.sanitized();
        self.buttons = clean;
        let frame = Frame::Buttons(self.buttons.clone());
        self.send_to_active(now, &frame);
        caps
    }

    /// What the user settled on the Mac that the phone obeys.
    #[must_use]
    pub fn settings(&self) -> HostSettings {
        self.settings
    }

    /// A change of the settings: kept and sent to every peer here.
    pub fn set_settings(&mut self, now: Millis, settings: HostSettings) {
        if self.settings != settings {
            self.settings = settings;
            self.send_to_active(now, &Frame::Settings(settings));
        }
    }

    /// Opens a pairing window and returns the offer to show (a QR code): the ticket and a new one-time secret. `None` until the
    /// endpoint knows its address. A window that was open is replaced: its secret no longer works.
    pub fn begin_pairing(&mut self, now: Millis) -> Option<PairingOffer> {
        if self.ticket.is_empty() {
            return None;
        }
        self.drop_waiting(now, PairReject::Expired);
        let secret = new_secret(self.entropy.as_mut());
        self.window = Some(Window { secret, opened_at: now, wrong: 0, waiting: None });
        Some(PairingOffer { ticket: self.ticket.clone(), secret })
    }

    /// Whether a pairing window is open.
    #[must_use]
    pub fn pairing_open(&self, now: Millis) -> bool {
        self.window.as_ref().is_some_and(|w| now.saturating_sub(w.opened_at) <= PAIRING_WINDOW_MS)
    }

    /// Closes the pairing window.
    pub fn cancel_pairing(&mut self) {
        let now = self.last_now();
        self.drop_waiting(now, PairReject::NoPairingOpen);
        self.window = None;
    }

    /// The person confirmed the code of `peer`: it is paired and told so.
    ///
    /// # Errors
    /// Nobody waits, another peer waits, or four are paired.
    pub fn accept_pairing(&mut self, now: Millis, peer: &str) -> Result<(), ConfirmError> {
        let Some(window) = self.window.as_mut() else {
            return Err(ConfirmError::NobodyWaiting);
        };
        let Some(waiting) = window.waiting.as_ref() else {
            return Err(ConfirmError::NobodyWaiting);
        };
        if now.saturating_sub(waiting.since) > CONFIRM_TIMEOUT_MS {
            return Err(ConfirmError::NobodyWaiting);
        }
        if waiting.peer != peer {
            return Err(ConfirmError::NotThatPhone);
        }
        if self.trust.len() >= self.config.max_phones {
            return Err(ConfirmError::TooManyPhones);
        }
        let (name, code) = (waiting.name.clone(), waiting.code);
        // the secret is spent
        self.window = None;
        let record = TrustRecord::phone(peer, &name);
        self.trust.push(record.clone());
        self.events.push(HostEvent::Paired { record });
        if let Some(index) = self.peers.iter().position(|p| p.id == peer) {
            let handle = self.peers[index].handle;
            self.send_frame(now, handle, &Frame::PairAccepted { code });
            self.peers[index].phase = Phase_::AwaitHello { since: now };
            // it said hello already (that is how it came to pair): it is served from now on
            self.activate(now, index);
        }
        Ok(())
    }

    /// The person said no to the code of `peer`: it is told, and nothing is paired.
    pub fn reject_pairing(&mut self, now: Millis, peer: &str) {
        let waits_for_it = self.window.as_ref().and_then(|w| w.waiting.as_ref()).is_some_and(|w| w.peer == peer);
        if waits_for_it {
            if let Some(window) = self.window.as_mut() {
                window.waiting = None;
            }
            if let Some(handle) = self.peers.iter().find(|p| p.id == peer).map(|p| p.handle) {
                self.send_frame(now, handle, &Frame::PairRejected(PairReject::Declined));
            }
        }
    }

    /// Forgets a paired peer: it is disconnected and is refused next time.
    pub fn forget(&mut self, now: Millis, peer: &str) {
        self.trust.retain(|r| r.peer != peer);
        let handles: Vec<PeerHandle> = self.peers.iter().filter(|p| p.id == peer).map(|p| p.handle).collect();
        for handle in handles {
            self.drop_peer(now, handle, "forgotten", true);
        }
    }

    /// What the pen is looking at now (sent at most 30 times a second to every peer here, newest wins, only when it changed).
    pub fn publish_pen(&mut self, pen: PenState) {
        for peer in &mut self.peers {
            if peer.phase == Phase_::Active {
                peer.pen_pacer.offer(pen.clone());
            }
        }
        self.pen = Some(pen);
    }

    /// The picture of the surface on screen (sent once to each peer, and again when it asks).
    pub fn publish_thumbnail(&mut self, thumbnail: Thumbnail) {
        let now = self.last_now();
        let frame = Frame::Thumbnail(thumbnail.clone());
        let id = thumbnail.id;
        self.thumb = Some(thumbnail);
        let targets: Vec<(usize, PeerHandle)> =
            self.peers.iter().enumerate().filter(|(_, p)| p.phase == Phase_::Active && p.thumb_sent != id).map(|(i, p)| (i, p.handle)).collect();
        for (index, handle) in targets {
            if self.send_frame(now, handle, &frame) {
                self.peers[index].thumb_sent = id;
            }
        }
    }

    /// The rows of a folder, for the peer that asked.
    pub fn publish_rows(&mut self, now: Millis, peer: &str, rows: Rows) {
        self.send_to(now, peer, &Frame::Rows(rows));
    }

    /// How far the conversation of a button press, or any request, has got.
    pub fn reply(&mut self, now: Millis, peer: &str, request_id: u32, status: ReplyStatus) {
        self.send_to(now, peer, &Frame::Reply(Reply { request_id, status }));
    }

    /// A line for every peer that is here.
    pub fn notice(&mut self, now: Millis, notice: Notice) {
        let (text, _) = cut_bytes(&notice.text, MAX_NOTICE_BYTES, "notice");
        self.send_to_active(now, &Frame::Notice(Notice { text, ..notice }));
    }

    /// An agent finished answering: every peer here is told (the phone buzzes by the settings and shows where).
    pub fn agent_finished(&mut self, now: Millis, finished: AgentFinished) {
        let (file, _) = cut_bytes(&finished.file, MAX_PATH_BYTES, "file");
        self.send_to_active(now, &Frame::AgentFinished(AgentFinished { file, ..finished }));
    }

    /// Lets the host work: receive, send, time out. What happened.
    pub fn poll(&mut self, now: Millis) -> Vec<HostEvent> {
        self.last_poll = now;
        for event in self.link.poll(now) {
            match event {
                LinkEvent::Ready { ticket } => {
                    self.ticket = ticket;
                    if !self.ready_said {
                        self.ready_said = true;
                    }
                    self.events.push(HostEvent::Ready { ticket: self.ticket.clone() });
                }
                LinkEvent::Connected { peer, id } => self.on_connected(now, peer, id),
                LinkEvent::Disconnected { peer, reason } => self.on_disconnected(now, peer, &reason),
                LinkEvent::Received { peer, bytes, .. } => self.on_received(now, peer, &bytes),
                LinkEvent::Failed { text } => self.events.push(HostEvent::Diagnostic(format!("the network said: {text}"))),
            }
        }
        self.tick(now);
        std::mem::take(&mut self.events)
    }

    /// Ends the host: the link is closed.
    pub fn close(&mut self) {
        self.link.close();
        self.peers.clear();
    }
}

// ---- the inside

impl<L: Link> CompanionHost<L> {
    fn last_now(&self) -> Millis {
        self.last_poll
    }

    fn hello(&self) -> Frame {
        Frame::Hello(Hello {
            role: Role::Host,
            protocol: PROTOCOL_VERSION,
            min_protocol: MIN_PROTOCOL_VERSION,
            app_version: self.config.app_version.clone(),
            name: self.config.mac_name.clone(),
            ticket: self.ticket.clone(),
        })
    }

    /// Writes `frame` to a connection. False when it could not be written or sent.
    fn send_frame(&mut self, now: Millis, handle: PeerHandle, frame: &Frame) -> bool {
        match encode(frame) {
            Ok(bytes) => self.link.send(now, handle, &bytes, frame.delivery()),
            Err(error) => {
                self.events.push(HostEvent::Diagnostic(format!("a {} could not be sent: {error}", frame.name())));
                false
            }
        }
    }

    fn send_to(&mut self, now: Millis, peer: &str, frame: &Frame) {
        if let Some(handle) = self.peers.iter().find(|p| p.id == peer && p.phase == Phase_::Active).map(|p| p.handle) {
            self.send_frame(now, handle, frame);
        }
    }

    fn send_to_active(&mut self, now: Millis, frame: &Frame) {
        let handles: Vec<PeerHandle> = self.peers.iter().filter(|p| p.phase == Phase_::Active).map(|p| p.handle).collect();
        for handle in handles {
            self.send_frame(now, handle, frame);
        }
    }

    /// Closes a connection; `tell` sends it a `Bye` first. The event says why.
    fn drop_peer(&mut self, now: Millis, handle: PeerHandle, reason: &str, tell: bool) {
        let Some(index) = self.peers.iter().position(|p| p.handle == handle) else {
            return;
        };
        let peer = self.peers.remove(index);
        if tell {
            if let Ok(bytes) = encode(&Frame::Bye) {
                self.link.send(now, handle, &bytes, Delivery::Reliable);
            }
        }
        self.link.disconnect(handle);
        // a peer that was waiting for the person's yes is not waiting any more
        if let Some(window) = self.window.as_mut() {
            if window.waiting.as_ref().is_some_and(|w| w.peer == peer.id) {
                window.waiting = None;
            }
        }
        self.events.push(HostEvent::PeerGone { peer: peer.id, reason: reason.to_string() });
    }

    /// The waiting peer (if any) is told it will not be paired, and does not wait.
    fn drop_waiting(&mut self, now: Millis, why: PairReject) {
        let waiting = self.window.as_mut().and_then(|w| w.waiting.take());
        if let Some(waiting) = waiting {
            if let Some(handle) = self.peers.iter().find(|p| p.id == waiting.peer).map(|p| p.handle) {
                self.send_frame(now, handle, &Frame::PairRejected(why));
            }
        }
    }

    fn on_connected(&mut self, now: Millis, handle: PeerHandle, id: String) {
        let paired = self.trust.iter().any(|r| r.peer == id);
        if !paired && !self.pairing_open(now) {
            self.events.push(HostEvent::Diagnostic(format!("a phone that is not paired tried to connect ({}…)", &id[..id.len().min(8)])));
            self.send_frame(now, handle, &Frame::Refused(Refusal::NotPaired));
            self.link.disconnect(handle);
            return;
        }
        // one connection a peer: a new one replaces the old
        let old: Vec<PeerHandle> = self.peers.iter().filter(|p| p.id == id).map(|p| p.handle).collect();
        for old_handle in old {
            self.drop_peer(now, old_handle, "connected again", true);
        }
        self.peers.push(Peer {
            handle,
            id,
            name: String::new(),
            role: Role::Phone,
            phase: if paired { Phase_::AwaitHello { since: now } } else { Phase_::Pairing },
            last_rx: now,
            greeted: false,
            garbage: 0,
            last_seq: None,
            gesture: None,
            last_ended: None,
            ping: None,
            next_ping_at: now + PING_INTERVAL_MS,
            next_nonce: 1,
            rtt_ms: None,
            button_times: Vec::new(),
            requests: Vec::new(),
            pen_pacer: Pacer::new(PEN_STATE_MAX_HZ),
            pen_seq: 0,
            last_pen: None,
            thumb_sent: 0,
        });
        let hello = self.hello();
        if self.send_frame(now, handle, &hello) {
            if let Some(peer) = self.peers.last_mut() {
                peer.greeted = true;
            }
        }
    }

    fn on_disconnected(&mut self, now: Millis, handle: PeerHandle, reason: &str) {
        if self.peers.iter().any(|p| p.handle == handle) {
            self.drop_peer(now, handle, &format!("the connection ended: {reason}"), false);
        }
    }

    fn on_received(&mut self, now: Millis, handle: PeerHandle, bytes: &[u8]) {
        let Some(index) = self.peers.iter().position(|p| p.handle == handle) else {
            return;
        };
        self.peers[index].last_rx = now;
        let frame = match decode(bytes) {
            Ok(frame) => frame,
            Err(error) => {
                let name = self.display_name(index);
                self.events.push(HostEvent::Diagnostic(format!("a frame from {name} was not understood ({error}, {} bytes)", bytes.len())));
                self.garbage(now, index);
                return;
            }
        };
        if !frame.is_phone_to_host() && !matches!(frame, Frame::Hello(_) | Frame::Ping { .. } | Frame::Pong { .. } | Frame::Bye) {
            let name = self.display_name(index);
            self.events.push(HostEvent::Diagnostic(format!("{name} sent a {}, which only the Mac may send: ignored", frame.name())));
            self.garbage(now, index);
            return;
        }
        match frame {
            Frame::Bye => self.drop_peer(now, handle, "said goodbye", false),
            Frame::Ping { nonce } => {
                self.send_frame(now, handle, &Frame::Pong { nonce });
            }
            Frame::Pong { nonce } => {
                if let Some((sent_nonce, sent_at)) = self.peers[index].ping {
                    if sent_nonce == nonce {
                        self.peers[index].rtt_ms = Some(now.saturating_sub(sent_at).min(u64::from(u32::MAX)) as u32);
                        self.peers[index].ping = None;
                    }
                }
            }
            Frame::Hello(hello) => self.on_hello(now, index, &hello),
            Frame::PairRequest { proof, phone_name } => self.on_pair_request(now, index, &proof, &phone_name),
            other => {
                if self.peers[index].phase == Phase_::Active {
                    self.on_command(now, index, other);
                }
                // before the hello (or before pairing) nothing a peer asks is taken
            }
        }
    }

    fn display_name(&self, index: usize) -> String {
        let peer = &self.peers[index];
        if peer.name.is_empty() {
            format!("a peer ({}…)", &peer.id[..peer.id.len().min(8)])
        } else {
            peer.name.clone()
        }
    }

    /// A frame that is no frame, or one this peer may not send: forgiven a few times, then the connection ends.
    fn garbage(&mut self, now: Millis, index: usize) {
        self.peers[index].garbage += 1;
        if self.peers[index].garbage > GARBAGE_FORGIVEN {
            let handle = self.peers[index].handle;
            self.drop_peer(now, handle, "it keeps sending what is no frame", true);
        }
    }

    fn on_hello(&mut self, now: Millis, index: usize, hello: &Hello) {
        let handle = self.peers[index].handle;
        if !matches!(self.peers[index].phase, Phase_::AwaitHello { .. } | Phase_::Pairing) || !self.peers[index].name.is_empty() {
            return; // a second hello changes nothing
        }
        if hello.role != Role::Phone {
            self.events.push(HostEvent::Diagnostic(format!("a {:?} tried to connect; the host only serves phones: refused", hello.role)));
            self.send_frame(now, handle, &Frame::Refused(Refusal::UnsupportedRole));
            self.drop_peer(now, handle, "an unsupported role", false);
            return;
        }
        if let Err(refusal) = negotiate(Role::Host, (PROTOCOL_VERSION, MIN_PROTOCOL_VERSION), hello) {
            self.send_frame(now, handle, &Frame::Refused(refusal));
            self.drop_peer(now, handle, &format!("its protocol does not fit: {refusal:?}"), false);
            return;
        }
        let (name, _) = cut_bytes(hello.name.trim(), MAX_NAME_BYTES, "name");
        self.peers[index].name = if name.is_empty() { "phone".to_string() } else { name };
        self.peers[index].role = hello.role;
        if self.peers[index].phase == Phase_::Pairing {
            return; // it may only pair; the proof comes next
        }
        self.activate(now, index);
    }

    /// The peer is served from now on: it is told what the host knows.
    fn activate(&mut self, now: Millis, index: usize) {
        self.peers[index].phase = Phase_::Active;
        let handle = self.peers[index].handle;
        let (id, name, role) = (self.peers[index].id.clone(), self.peers[index].name.clone(), self.peers[index].role);
        self.events.push(HostEvent::PeerConnected { peer: id, name, role });
        self.send_frame(now, handle, &Frame::Settings(self.settings));
        self.send_frame(now, handle, &Frame::Buttons(self.buttons.clone()));
        if let Some(thumb) = self.thumb.clone() {
            let id = thumb.id;
            if self.send_frame(now, handle, &Frame::Thumbnail(thumb)) {
                self.peers[index].thumb_sent = id;
            }
        }
        if let Some(pen) = self.pen.clone() {
            self.peers[index].pen_pacer.offer(pen);
        }
    }

    fn on_pair_request(&mut self, now: Millis, index: usize, proof: &[u8; 32], phone_name: &str) {
        let handle = self.peers[index].handle;
        if self.peers[index].phase != Phase_::Pairing {
            return;
        }
        let reject = |host: &mut Self, why: PairReject| {
            host.send_frame(now, handle, &Frame::PairRejected(why));
        };
        let Some(window) = self.window.as_mut() else {
            reject(self, PairReject::NoPairingOpen);
            return;
        };
        if now.saturating_sub(window.opened_at) > PAIRING_WINDOW_MS {
            self.window = None;
            reject(self, PairReject::Expired);
            return;
        }
        let phone_key = self.peers[index].id.clone();
        if !verify_proof(&window.secret, &self.node_id, &phone_key, proof) {
            window.wrong += 1;
            let closed = window.wrong >= PAIRING_MAX_WRONG_PROOFS;
            if closed {
                self.window = None;
                self.events.push(HostEvent::Diagnostic("a pairing window was closed: three wrong proofs".to_string()));
            }
            reject(self, PairReject::WrongSecret);
            return;
        }
        if self.trust.len() >= self.config.max_phones {
            reject(self, PairReject::TooManyPhones);
            return;
        }
        if window.waiting.as_ref().is_some_and(|w| w.peer != phone_key) {
            reject(self, PairReject::Busy);
            return;
        }
        let code = confirmation_code(&window.secret, &self.node_id, &phone_key);
        let (name, _) = cut_bytes(phone_name.trim(), MAX_NAME_BYTES, "name");
        let name = if name.is_empty() { "phone".to_string() } else { name };
        window.waiting = Some(Waiting { peer: phone_key.clone(), name: name.clone(), code, since: now });
        self.peers[index].name = name.clone();
        self.events.push(HostEvent::PairingRequested { peer: phone_key, name, code });
    }

    /// Whether `peer` may command `screen`; a diagnostic says why not.
    fn allowed(&mut self, index: usize, screen: ScreenId) -> bool {
        let record = self.trust.iter().find(|r| r.peer == self.peers[index].id);
        let info = self.screens.iter().find(|s| s.id == screen);
        match may_command(record, info, &self.node_id) {
            Ok(()) => true,
            Err(why) => {
                let name = self.display_name(index);
                self.events.push(HostEvent::Diagnostic(format!("{name} may not command {screen}: {why}")));
                false
            }
        }
    }

    /// Whether a request id of a peer is new (it is remembered, so that a repeated one is the same one).
    fn fresh_request(&mut self, index: usize, request_id: u32) -> bool {
        let seen = &mut self.peers[index].requests;
        if seen.contains(&request_id) {
            return false;
        }
        seen.push(request_id);
        if seen.len() > REMEMBERED_REQUESTS {
            seen.remove(0);
        }
        true
    }

    fn on_command(&mut self, now: Millis, index: usize, frame: Frame) {
        let handle = self.peers[index].handle;
        let peer = self.peers[index].id.clone();
        match frame {
            Frame::Viewport(cmd) => self.on_viewport(now, index, cmd),
            Frame::ListDir { request_id, path, offset } => {
                if !is_safe_relative_path(&path) {
                    let name = self.display_name(index);
                    self.events.push(HostEvent::Diagnostic(format!("{name} asked for a folder that is not inside the workspace: refused")));
                } else {
                    // a folder asked for twice is listed twice: a listing is the same however often it is asked
                    self.events.push(HostEvent::ListDir { peer, request_id, path, offset });
                }
            }
            Frame::OpenFile { screen, request_id, path, line } => {
                if !is_safe_relative_path(&path) || path.is_empty() {
                    let name = self.display_name(index);
                    self.events.push(HostEvent::Diagnostic(format!("{name} asked for a file that is not inside the workspace: refused")));
                } else if self.allowed(index, screen) {
                    self.events.push(HostEvent::OpenFile { peer, screen, request_id, path, line });
                }
            }
            Frame::Button { screen, request_id, index: button } => {
                if !self.allowed(index, screen) || !self.fresh_request(index, request_id) {
                    return;
                }
                let recent = &mut self.peers[index].button_times;
                recent.retain(|t| now.saturating_sub(*t) < 1_000);
                if recent.len() >= BUTTONS_PER_SECOND {
                    let name = self.display_name(index);
                    self.events.push(HostEvent::Diagnostic(format!("button presses from {name} are too fast: ignored")));
                    return;
                }
                recent.push(now);
                let Some(key) = self.buttons.buttons.get(usize::from(button)) else {
                    return;
                };
                let (label, text) = (key.label.clone(), key.text.clone());
                self.events.push(HostEvent::ButtonPressed { peer: peer.clone(), screen, index: usize::from(button), label, text, request_id });
                self.send_frame(now, handle, &Frame::Reply(Reply { request_id, status: ReplyStatus::Received }));
            }
            Frame::SetButtons { request_id, buttons } => {
                let own = self.trust.iter().any(|r| r.peer == peer && r.group == crate::access::TrustGroup::MyDevices);
                if !own {
                    let name = self.display_name(index);
                    self.events.push(HostEvent::Diagnostic(format!("{name} may not change the buttons: refused")));
                    return;
                }
                let (clean, caps) = buttons.sanitized();
                for cap in &caps {
                    self.events.push(HostEvent::Diagnostic(format!("the buttons were cut: {cap}")));
                }
                self.buttons = clean.clone();
                self.events.push(HostEvent::ButtonsChanged { buttons: clean.clone() });
                self.send_to_active(now, &Frame::Buttons(clean));
                self.send_frame(now, handle, &Frame::Reply(Reply { request_id, status: ReplyStatus::Done }));
            }
            Frame::NeedThumbnail { id } => {
                if let Some(thumb) = self.thumb.clone().filter(|t| t.id == id) {
                    self.send_frame(now, handle, &Frame::Thumbnail(thumb));
                }
            }
            Frame::ContinueReviewing { screen } => {
                if self.allowed(index, screen) {
                    self.events.push(HostEvent::ContinueReviewing { peer, screen });
                }
            }
            _ => {}
        }
    }

    /// A viewport command: in the peer's order, held to the caps, a gesture made whole.
    fn on_viewport(&mut self, now: Millis, index: usize, cmd: ViewportCmd) {
        if !self.allowed(index, cmd.screen) {
            return;
        }
        if let Some(last) = self.peers[index].last_seq {
            if (cmd.seq.wrapping_sub(last) as i32) <= 0 {
                return; // older, or the same one again
            }
        }
        self.peers[index].last_seq = Some(cmd.seq);
        let peer = self.peers[index].id.clone();
        let screen = cmd.screen;
        let emit = |host: &mut Self, op: ViewportOp| host.events.push(HostEvent::Viewport { peer: peer.clone(), screen, op });
        match cmd.op {
            ViewportOp::Gesture { id, phase, pan_x, pan_y, zoom, focus } => {
                let (px, py, z) = (pan_x.clamp(-MAX_GESTURE_PAN, MAX_GESTURE_PAN), pan_y.clamp(-MAX_GESTURE_PAN, MAX_GESTURE_PAN), zoom.clamp(MIN_GESTURE_ZOOM, MAX_GESTURE_ZOOM));
                if (px, py, z) != (pan_x, pan_y, zoom) {
                    let name = self.display_name(index);
                    self.events.push(HostEvent::Diagnostic(format!("a gesture of {name} was held to the limits (pan {pan_x:.1} {pan_y:.1}, zoom {zoom:.1})")));
                }
                if self.peers[index].last_ended == Some(id) {
                    return; // a late frame of a gesture that is over
                }
                // another gesture takes over: the one before ends where it was
                if let Some(old) = self.peers[index].gesture {
                    if old.id != id {
                        emit(self, ViewportOp::Gesture { id: old.id, phase: Phase::End, pan_x: old.pan.0, pan_y: old.pan.1, zoom: old.zoom, focus: old.focus });
                        self.peers[index].last_ended = Some(old.id);
                        self.peers[index].gesture = None;
                    }
                }
                let begun = self.peers[index].gesture.is_some_and(|g| g.id == id);
                if !begun {
                    // the first sight of a gesture is its start, whatever frame it came in
                    emit(self, ViewportOp::Gesture { id, phase: Phase::Begin, pan_x: 0.0, pan_y: 0.0, zoom: 1.0, focus });
                    if phase == Phase::Begin {
                        self.peers[index].gesture = Some(Gesture { id, pan: (0.0, 0.0), zoom: 1.0, focus, last_at: now });
                        return;
                    }
                }
                match phase {
                    Phase::Begin => {}
                    Phase::Update => {
                        emit(self, ViewportOp::Gesture { id, phase: Phase::Update, pan_x: px, pan_y: py, zoom: z, focus });
                        self.peers[index].gesture = Some(Gesture { id, pan: (px, py), zoom: z, focus, last_at: now });
                    }
                    Phase::End => {
                        emit(self, ViewportOp::Gesture { id, phase: Phase::End, pan_x: px, pan_y: py, zoom: z, focus });
                        self.peers[index].gesture = None;
                        self.peers[index].last_ended = Some(id);
                    }
                }
            }
            ViewportOp::PageStep(step) => emit(self, ViewportOp::PageStep(step.clamp(-MAX_PAGE_STEPS, MAX_PAGE_STEPS))),
            op => emit(self, op),
        }
    }

    /// What time does: hello and confirmation timeouts, the window, silence, pings, the end of a quiet gesture, the pen.
    fn tick(&mut self, now: Millis) {
        // the pairing window and the person's yes
        if self.window.as_ref().is_some_and(|w| now.saturating_sub(w.opened_at) > PAIRING_WINDOW_MS) {
            self.drop_waiting(now, PairReject::Expired);
            self.window = None;
        }
        if self.window.as_ref().and_then(|w| w.waiting.as_ref()).is_some_and(|w| now.saturating_sub(w.since) > CONFIRM_TIMEOUT_MS) {
            self.drop_waiting(now, PairReject::Expired);
        }
        let handles: Vec<PeerHandle> = self.peers.iter().map(|p| p.handle).collect();
        for handle in handles {
            let Some(index) = self.peers.iter().position(|p| p.handle == handle) else {
                continue;
            };
            // a connection that says no hello
            if let Phase_::AwaitHello { since } = self.peers[index].phase {
                if now.saturating_sub(since) > HELLO_TIMEOUT_MS {
                    self.drop_peer(now, handle, "it said no hello", true);
                    continue;
                }
            }
            // silence
            if now.saturating_sub(self.peers[index].last_rx) > PEER_TIMEOUT_MS {
                self.drop_peer(now, handle, "no answer for 8 s", true);
                continue;
            }
            // pings: a peer that waits for the person's yes is pinged too (it is silent otherwise, and must not be taken for gone)
            let said_hello = !self.peers[index].name.is_empty();
            if (self.peers[index].phase == Phase_::Active || (self.peers[index].phase == Phase_::Pairing && said_hello)) && now >= self.peers[index].next_ping_at {
                let nonce = self.peers[index].next_nonce;
                self.peers[index].next_nonce = nonce.wrapping_add(1);
                self.peers[index].ping = Some((nonce, now));
                self.peers[index].next_ping_at = now + PING_INTERVAL_MS;
                self.send_frame(now, handle, &Frame::Ping { nonce });
            }
            if self.peers[index].phase != Phase_::Active {
                continue;
            }
            // a gesture that went quiet ends where it got to
            if let Some(g) = self.peers[index].gesture {
                if now.saturating_sub(g.last_at) > GESTURE_QUIET_MS {
                    let peer = self.peers[index].id.clone();
                    self.events.push(HostEvent::Viewport {
                        peer,
                        screen: ScreenId::MAIN,
                        op: ViewportOp::Gesture { id: g.id, phase: Phase::End, pan_x: g.pan.0, pan_y: g.pan.1, zoom: g.zoom, focus: g.focus },
                    });
                    self.peers[index].gesture = None;
                    self.peers[index].last_ended = Some(g.id);
                }
            }
            // the pen: at most thirty a second, only when it changed, counting up
            if let Some(mut pen) = self.peers[index].pen_pacer.take(now) {
                let changed = self.peers[index].last_pen.as_ref().is_none_or(|last| {
                    let mut same = last.clone();
                    same.seq = pen.seq;
                    same != pen
                });
                if changed {
                    self.peers[index].pen_seq = self.peers[index].pen_seq.wrapping_add(1);
                    pen.seq = self.peers[index].pen_seq;
                    self.peers[index].last_pen = Some(pen.clone());
                    self.send_frame(now, handle, &Frame::PenState(pen));
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "host_test.rs"]
mod host_test;
