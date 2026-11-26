//! The phone companion in the review window: the host a phone talks to (`azul-review-link`), when it listens, what is kept of it, and
//! its page of the settings.
//!
//! OFF BY DEFAULT. Nothing listens, nothing is announced on the network and no key is made until the user turns the companion on
//! (Settings > Phone companion, or the key on the first screen that leads there). When it is on it is bound at every start, after the
//! first frame (a timer that fires once the window has been up a moment), and polled by a timer of the window while it lives. Turning
//! it off closes the endpoint at once; what was paired stays, so that turning it on again finds the same phones.
//!
//! What is kept (`companion_store`: the switch, this computer's key, the paired phones) is a file of its own that belongs to this
//! computer and is written for the user only (`companion_io`), on Threads, never on the window's own thread; nothing is written before
//! the file was read, or the user's phones would be replaced by the default.
//!
//! The state is [`Glue`], held by the app's state in a `RefCell` like the whiteboard's; every access from a callback goes through
//! [`with_companion`], which never panics when it is reached while the glue is held already: it does nothing then. All the rules
//! (what an event does, when the poll is slow and when it is fast, what is written) are methods of the glue over a link that is
//! handed in, so that a test drives them over the link crate's in-process network and no socket is opened.

use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Instant,
};

use azul::{
    callbacks::{SegmentedOnChangeCallbackType, WriteBackCallbackType},
    prelude::*,
    task::TerminateTimer,
    time::SystemTimeDiff,
    widgets::{Segmented, SegmentedState},
};
use azul_appkit::{
    l10n::{label, labels, t, t_args, Arg},
    qr::QrCode,
    ui as kit,
};
use azul_review_link::{
    frame::{AgentFinished, ButtonSet, FinishKind, HostSettings, ReplyStatus, Rows, ViewportOp},
    host::{CompanionHost, ConfirmError, HostConfig, HostEvent, PhoneInfo},
    iroh_link::{IrohLink, IrohOptions},
    limits::{CONFIRM_TIMEOUT_MS, MAX_PAIRED_PHONES},
    link::{Link, Millis},
    pairing::{Entropy, OsEntropy},
};

use crate::{
    agent_window::{Events, Finish, Finished},
    companion_io::{self, Loaded},
    companion_store::{self, Stored},
    ids,
    page_view::Held,
    AppState,
};

/// The host as the app holds it: over whatever link was bound (the iroh endpoint; the link crate's in-process network in a test).
pub type Station = CompanionHost<Box<dyn Link>>;

/// How often the host is looked at while a phone is here or a pairing is open, ms (about 60 times a second: a gesture's updates come
/// that often and are applied at that rate).
pub const POLL_ACTIVE_MS: u64 = 16;
/// How often it is looked at while it waits for a phone, ms.
pub const POLL_IDLE_MS: u64 = 100;
/// How long the window has been up when the companion is bound at a start, ms: the first frame is behind it.
pub const FIRST_FRAME_MS: u64 = 750;
/// How many lines of what the host said are kept (the oldest go first).
pub const MAX_LOG: usize = 40;
/// How many bytes of a line are kept.
pub const MAX_LOG_BYTES: usize = 300;

/// What the companion is doing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    /// Not listening: the user has not turned it on (or turned it off).
    Off,
    /// On, and not bound yet: the window has not been up long enough, or the file has not been read.
    Waiting,
    /// Bound: phones may connect (the ones that are paired).
    On,
    /// On, and the endpoint could not be bound: why.
    Failed(String),
}

/// What the settings page and a test see of it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Off,
    Starting,
    Listening { paired: usize, here: usize },
    Failed(String),
}

/// A pairing code that is open: the text a phone pastes (or scans from the picture) and when it was made. The text holds the one-time
/// secret: it is shown to the user and put on the clipboard as a secret, and is never logged.
pub struct Offer {
    pub text: String,
    /// The picture of it, when it is short enough for one (a ticket with many addresses is longer than a QR code of the size AzReview makes).
    pub qr: Option<QrCode>,
    pub opened_at: Millis,
    /// The user copied it (the window says so).
    pub copied: bool,
}

/// A phone that proved the one-time secret and waits for the person at this computer to say that the six digits are the same on both
/// screens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Asked {
    /// The phone's key.
    pub peer: String,
    /// What it calls itself (cleaned and cut).
    pub name: String,
    /// The six digits both screens show.
    pub code: u32,
    pub since: Millis,
}

/// Why no pairing code was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairError {
    /// The companion is not on, or not bound.
    NotListening,
    /// The endpoint does not know its address yet (a moment after it was bound).
    NoAddress,
    /// Four phones are paired: one is forgotten first.
    TooManyPhones,
}

/// What a phone asks of the window that the glue cannot answer by itself, because it needs the window's state.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// A step of a drag or a pinch, a fit, a page key or a reset: the window's view of the pages moves (`companion_cmd::viewport`).
    Viewport(ViewportOp),
    /// A quick button was pressed: its instruction goes to an agent with the context at the pen (`companion_cmd::button`).
    Button { peer: String, request_id: u32, text: String },
}

/// A quick button of a phone that is under way: the conversation it started, and who is told how it goes.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingButton {
    note: u64,
    peer: String,
    request_id: u32,
}

/// How many finished conversations wait to be told to the phones (the oldest give way).
const MAX_FINISHED: usize = 64;

/// What the agents' window tells when a conversation finishes, put in a queue the glue takes from at its next look at the host (the call
/// is on the window's thread and must not block; the glue is not for another thread).
struct FinishedQueue(Arc<Mutex<Vec<Finished>>>);

impl Events for FinishedQueue {
    fn finished(&self, event: &Finished) {
        if let Ok(mut queue) = self.0.lock() {
            if queue.len() >= MAX_FINISHED {
                queue.remove(0);
            }
            queue.push(event.clone());
        }
    }
}

/// The phone's word for how a conversation ended.
#[must_use]
pub fn kind_of(finish: Finish) -> FinishKind {
    match finish {
        Finish::Done => FinishKind::Done,
        Finish::Suggested(_) => FinishKind::Suggestion,
        Finish::Quiet => FinishKind::NoAction,
        Finish::Failed => FinishKind::Failed,
    }
}

/// How a quick button's request stands when its conversation ended as `finish`.
#[must_use]
pub fn status_of(finish: Finish) -> ReplyStatus {
    match finish {
        Finish::Done | Finish::Quiet => ReplyStatus::Done,
        Finish::Suggested(_) => ReplyStatus::Suggested,
        Finish::Failed => ReplyStatus::Failed,
    }
}

/// What a look at the host found, for the window to act on.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Polled {
    /// What the phones asked, in the order they asked it.
    pub commands: Vec<Command>,
    /// A phone asks to be paired: the window shows it whatever page it is on.
    pub attention: bool,
    /// What the settings page shows may have changed (a phone came or went, the address is known).
    pub changed: bool,
    /// What is kept changed: it is to be written.
    pub save: bool,
    /// The poll is to be at another pace from now on: the interval, ms.
    pub rate: Option<u64>,
}

/// The companion of the app: the file it keeps, the host, the timer that polls it.
pub struct Glue {
    path: PathBuf,
    /// What is kept: the switch, the key, the paired phones.
    pub stored: Stored,
    started: bool,
    loaded: bool,
    state: State,
    host: Option<Station>,
    timer: Option<TimerId>,
    interval_ms: u64,
    clock: Instant,
    ticket_known: bool,
    offer: Option<Offer>,
    asked: Option<Asked>,
    /// The gesture of the phone that is under way (a drag or a pinch) with the view it began on.
    pub held: Held,
    /// Where the strip is to be scrolled once the pages were laid out at their new zoom, and whether the timer that does it is under way.
    pub(crate) pending_scroll: Option<(f32, f32)>,
    pub(crate) settling: bool,
    /// Something the phone is told of moved: its state is made and sent again at the next look at the host.
    pen_dirty: bool,
    /// The buttons as the fields of the settings hold them (what was typed, not cut yet): what is kept and sent is this cut to its caps.
    draft: ButtonSet,
    /// The quick buttons that are being answered, and the conversations that finished (told by the agents' window, taken at a look).
    pending: Vec<PendingButton>,
    finished: Arc<Mutex<Vec<Finished>>>,
    saving: bool,
    save_pending: bool,
    log: Vec<String>,
}

impl Glue {
    /// The companion of an app whose file is `path` (not read yet: off, and nothing written until it is read).
    #[must_use]
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            stored: Stored::default(),
            started: false,
            loaded: false,
            state: State::Off,
            host: None,
            timer: None,
            interval_ms: POLL_IDLE_MS,
            clock: Instant::now(),
            ticket_known: false,
            offer: None,
            asked: None,
            held: Held::default(),
            pending_scroll: None,
            settling: false,
            pen_dirty: false,
            draft: ButtonSet::defaults(),
            pending: Vec::new(),
            finished: Arc::new(Mutex::new(Vec::new())),
            saving: false,
            save_pending: false,
            log: Vec::new(),
        }
    }

    /// Something the phone is told of moved (the file, the page, the zoom, the pen): its state is sent again at the next look.
    pub fn touch(&mut self) {
        self.pen_dirty = true;
    }

    /// Whether the phone's state is to be made and sent now: something moved, and a phone is here to be told (the host keeps the newest for
    /// one that comes). Taking the answer clears it.
    pub fn take_dirty(&mut self) -> bool {
        let here = self.host.as_ref().is_some_and(|h| h.phones().iter().any(|p| p.connected));
        std::mem::take(&mut self.pen_dirty) && here
    }

    /// The milliseconds since this glue was made: the monotonic clock the host is polled with.
    #[must_use]
    pub fn now(&self) -> Millis {
        u64::try_from(self.clock.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// What it is doing, for the page.
    #[must_use]
    pub fn status(&self) -> Status {
        match &self.state {
            State::Off => Status::Off,
            State::Waiting => Status::Starting,
            State::Failed(why) => Status::Failed(why.clone()),
            State::On => {
                let phones = self.host.as_ref().map(Station::phones).unwrap_or_default();
                Status::Listening { paired: phones.len(), here: phones.iter().filter(|p| p.connected).count() }
            }
        }
    }

    /// Whether the host is bound (listening).
    #[must_use]
    pub fn is_listening(&self) -> bool {
        self.host.is_some()
    }

    /// What the host and the glue said that the user may want to read, the newest last.
    #[must_use]
    pub fn log(&self) -> &[String] {
        &self.log
    }

    /// The host, for the pairing and the commands (`None` while nothing is bound).
    pub fn host_mut(&mut self) -> Option<&mut Station> {
        self.host.as_mut()
    }

    /// What the user settled on this computer that the phones obey (the defaults while nothing is bound).
    #[must_use]
    pub fn host_settings(&self) -> HostSettings {
        self.host.as_ref().map_or(self.stored.prefs, |h| h.settings())
    }

    /// Whether the endpoint's address is known (a code for a phone can be made from it).
    #[must_use]
    pub fn has_address(&self) -> bool {
        self.ticket_known
    }

    fn note(&mut self, text: &str) {
        // (a line is one line whatever a phone put in the name it said: no control character gets into the log or the lines a script reads)
        let plain: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect();
        let (line, _cut) = azul_review_link::limits::cut_bytes(plain.trim(), MAX_LOG_BYTES, "companion log line");
        println!("AZREVIEW_COMPANION {line}");
        if self.log.len() >= MAX_LOG {
            self.log.remove(0);
        }
        self.log.push(line);
    }

    // ---- what the file says

    /// The file was read: what it holds is what is kept now (the default when it held nothing or was damaged: the problems are said).
    /// `true` when the user had turned the companion on: it is bound after the first frame.
    pub fn apply_loaded(&mut self, loaded: Loaded) -> bool {
        if let Some(problem) = &loaded.problem {
            self.note(problem);
        }
        let (stored, problems) = Stored::parse(loaded.text.as_deref().unwrap_or(""));
        for problem in &problems {
            self.note(problem);
        }
        self.draft = stored.buttons.clone();
        self.stored = stored;
        self.loaded = true;
        let on = self.stored.enabled;
        // a switch that was turned on in the meantime stays
        if on && self.state == State::Off {
            self.state = State::Waiting;
        }
        on
    }

    /// Whether the file was read.
    #[must_use]
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// The text to write now, when there is anything to write (never before the file was read: what is there is not ours to replace
    /// by the default).
    #[must_use]
    pub fn to_write(&self) -> Option<(PathBuf, Vec<u8>)> {
        self.loaded.then(|| (self.path.clone(), self.stored.to_json().into_bytes()))
    }

    // ---- on and off

    /// The user turned the companion on or off. `true` when that changed anything; on, the caller then binds (`bind_real`, or at the
    /// next start) and writes; off, it is closed already.
    pub fn set_enabled(&mut self, on: bool) -> bool {
        if !self.loaded || self.stored.enabled == on {
            return false;
        }
        self.stored.enabled = on;
        if on {
            self.state = State::Waiting;
        } else {
            self.close();
        }
        self.note(if on { "turned on" } else { "turned off" });
        true
    }

    fn close(&mut self) {
        if let Some(mut host) = self.host.take() {
            host.close();
        }
        self.state = State::Off;
        self.ticket_known = false;
        self.offer = None;
        self.asked = None;
    }

    /// The name this computer goes by to the phones.
    #[must_use]
    pub fn name(&self) -> String {
        companion_store::DEFAULT_NAME.to_string()
    }

    /// Binds the iroh endpoint with this computer's key (a first start makes one, and it is kept) and the text the computer announces
    /// on the local network (its name, no secret), and starts the host over it.
    ///
    /// # Errors
    /// Why the engine could not bind (no engine in the library, no socket): it is the status then, and it is tried again at the next start.
    pub fn bind_real(&mut self) -> Result<(), String> {
        if self.host.is_some() {
            return Ok(());
        }
        let options = IrohOptions {
            secret_key: self.stored.key(),
            advertised: Some(companion_store::advertised(&self.name())),
            ..IrohOptions::default()
        };
        match IrohLink::bind(&options) {
            Ok(link) => {
                if self.stored.key().is_none() {
                    if let Some(key) = link.secret_key() {
                        self.stored.set_key(key);
                    }
                }
                self.bind_link(Box::new(link), Box::new(OsEntropy));
                Ok(())
            }
            Err(why) => Err(self.bind_failed(&why)),
        }
    }

    /// The endpoint could not be bound: that is the status now (the reason cut to what a line holds), and it is said. The text.
    fn bind_failed(&mut self, why: &str) -> String {
        let (why, _cut) = azul_review_link::limits::cut_bytes(why, MAX_LOG_BYTES, "bind error");
        self.note(&format!("could not start: {why}"));
        self.state = State::Failed(why.clone());
        why
    }

    /// Starts the host over `link`: the paired phones that were kept are the only ones it lets in. `entropy` makes the one-time
    /// secrets of a pairing.
    pub fn bind_link(&mut self, link: Box<dyn Link>, entropy: Box<dyn Entropy>) {
        let now = self.now();
        let config = HostConfig { mac_name: self.name(), app_version: env!("CARGO_PKG_VERSION").to_string(), max_phones: MAX_PAIRED_PHONES };
        let mut host = CompanionHost::new(link, config, entropy);
        host.load_paired(self.stored.phones.clone());
        host.set_settings(now, self.stored.prefs);
        let _cut = host.set_buttons(now, &self.stored.buttons);
        if let Ok(mut queue) = self.finished.lock() {
            queue.clear();
        }
        self.pending.clear();
        self.host = Some(host);
        self.state = State::On;
        self.ticket_known = false;
        self.offer = None;
        self.asked = None;
        self.interval_ms = POLL_IDLE_MS;
        self.note("listening");
    }

    // ---- the poll

    /// How often the host is to be looked at now: fast while a phone is here or a pairing is open, slow while it waits.
    #[must_use]
    pub fn wanted_interval_ms(&self) -> u64 {
        self.wanted_interval_at(self.now())
    }

    /// [`Self::wanted_interval_ms`] at `now` ms of the clock the host is polled with.
    #[must_use]
    pub fn wanted_interval_at(&self, now: Millis) -> u64 {
        let busy = self.host.as_ref().is_some_and(|h| h.pairing_open(now) || h.phones().iter().any(|p| p.connected));
        if busy {
            POLL_ACTIVE_MS
        } else {
            POLL_IDLE_MS
        }
    }

    /// One look at the host at the clock's now.
    pub fn poll(&mut self) -> Polled {
        let now = self.now();
        self.poll_at(now)
    }

    /// One look at the host at `now` ms: what it received is handled, and what is to be shown or written is said.
    pub fn poll_at(&mut self, now: Millis) -> Polled {
        let mut out = Polled::default();
        let Some(mut host) = self.host.take() else {
            return out;
        };
        for event in host.poll(now) {
            self.handle(&mut host, now, event, &mut out);
        }
        self.tell_finished(&mut host, now);
        // a code is good for as long as the host's window is open, and a phone waits for the person for two minutes
        if self.offer.is_some() && !host.pairing_open(now) {
            self.offer = None;
            out.changed = true;
        }
        if self.asked.as_ref().is_some_and(|a| now.saturating_sub(a.since) > CONFIRM_TIMEOUT_MS) {
            self.asked = None;
            out.changed = true;
        }
        self.host = Some(host);
        let wanted = self.wanted_interval_at(now);
        if wanted != self.interval_ms {
            self.interval_ms = wanted;
            out.rate = Some(wanted);
        }
        out
    }

    /// What one event of the host asks of the app. The commands of a phone (the viewport, the files, the buttons) are answered as
    /// "cannot" until they are wired; nothing a phone says is acted on without being understood.
    fn handle(&mut self, host: &mut Station, now: Millis, event: HostEvent, out: &mut Polled) {
        match event {
            HostEvent::Ready { .. } => {
                self.ticket_known = true;
                out.changed = true;
            }
            HostEvent::PairingRequested { peer, name, code } => {
                // the person at this computer is asked (a bar over the window): the host gives them two minutes
                self.asked = Some(Asked { peer, name: companion_store::clean_name(&name), code, since: now });
                self.note("a phone asks to be paired");
                out.changed = true;
                out.attention = true;
            }
            HostEvent::Paired { .. } => {
                let before = self.stored.phones.clone();
                self.stored.set_phones(host.paired());
                out.save = self.stored.phones != before;
                out.changed = true;
                self.asked = None;
                self.offer = None;
            }
            HostEvent::PeerConnected { .. } => {
                // the host sends a phone that comes the newest state it has: the window makes a new one
                self.pen_dirty = true;
                out.changed = true;
            }
            HostEvent::PeerGone { peer, .. } => {
                // a phone that went while it was asked has nobody to be answered
                if self.asked.as_ref().is_some_and(|a| a.peer == peer) {
                    self.asked = None;
                }
                out.changed = true;
            }
            HostEvent::ListDir { peer, request_id, path, offset } => {
                host.publish_rows(now, &peer, Rows { request_id, dir: path, offset, entries: Vec::new(), more: false });
            }
            HostEvent::OpenFile { peer, request_id, .. } => {
                host.reply(now, &peer, request_id, ReplyStatus::Failed);
            }
            HostEvent::Viewport { op, .. } => out.commands.push(Command::Viewport(op)),
            HostEvent::ButtonPressed { peer, request_id, text, .. } => out.commands.push(Command::Button { peer, request_id, text }),
            HostEvent::ButtonsChanged { buttons } => {
                // a phone saved the buttons (the host cut them to their caps and sent them on to the others): they are kept
                if self.stored.buttons != buttons {
                    self.draft = buttons.clone();
                    self.stored.buttons = buttons;
                    out.save = true;
                    out.changed = true;
                }
            }
            HostEvent::ContinueReviewing { .. } => {}
            HostEvent::Diagnostic(text) => self.note(&text),
        }
    }

    // ---- the agents and the quick buttons

    /// The listener the agents' window is given: it queues the conversations that finish, and this glue tells the phones at its next look.
    #[must_use]
    pub fn events(&self) -> Arc<dyn Events> {
        Arc::new(FinishedQueue(self.finished.clone()))
    }

    /// The conversations that finished since the last look are told to the phones (a buzz, where and how), and the quick button that
    /// started one is told how it ended.
    fn tell_finished(&mut self, host: &mut Station, now: Millis) {
        let finished: Vec<Finished> = match self.finished.lock() {
            Ok(mut queue) => std::mem::take(&mut *queue),
            Err(_) => Vec::new(),
        };
        for event in finished {
            host.agent_finished(
                now,
                AgentFinished { conversation_id: u32::try_from(event.conversation).unwrap_or(u32::MAX), file: event.file.clone(), line: event.line, kind: kind_of(event.finish) },
            );
            if let Some(at) = self.pending.iter().position(|p| p.note == event.conversation) {
                let button = self.pending.remove(at);
                host.reply(now, &button.peer, button.request_id, status_of(event.finish));
            }
        }
    }

    /// A quick button started conversation `note`: the phone is told an agent is at work, and how it ends when it does.
    pub fn button_started(&mut self, note: u64, peer: &str, request_id: u32) {
        let now = self.now();
        if let Some(host) = self.host.as_mut() {
            host.reply(now, peer, request_id, ReplyStatus::Working);
        }
        if self.pending.len() >= MAX_FINISHED {
            self.pending.remove(0);
        }
        self.pending.push(PendingButton { note, peer: peer.to_string(), request_id });
    }

    /// A quick button could not start anything: the phone is told so.
    pub fn button_refused(&mut self, peer: &str, request_id: u32) {
        let now = self.now();
        if let Some(host) = self.host.as_mut() {
            host.reply(now, peer, request_id, ReplyStatus::Failed);
        }
    }

    /// The settings the phones obey were changed on this computer: they are kept (the caller writes them) and sent to the phones that are here.
    pub fn set_prefs(&mut self, prefs: HostSettings) {
        let now = self.now();
        self.stored.prefs = prefs;
        if let Some(host) = self.host.as_mut() {
            host.set_settings(now, prefs);
        }
    }

    /// The buttons were changed on this computer: cut to their caps (what was cut is said), kept (the caller writes them) and sent to the phones.
    pub fn set_buttons(&mut self, buttons: &ButtonSet) {
        let now = self.now();
        let (clean, caps) = buttons.sanitized();
        for cap in &caps {
            self.note(&cap.to_string());
        }
        self.stored.buttons = clean;
        if let Some(host) = self.host.as_mut() {
            let _cut = host.set_buttons(now, &self.stored.buttons);
        }
    }

    /// The buttons as the fields of the settings hold them.
    #[must_use]
    pub fn draft_buttons(&self) -> &ButtonSet {
        &self.draft
    }

    /// The fields of the settings were typed in: `fields` are what they hold (kept as typed, for the fields), and the buttons are that cut to
    /// their caps, kept and sent to the phones (the caller writes them). `true` when the buttons changed.
    pub fn edit_buttons(&mut self, fields: &ButtonSet) -> bool {
        self.draft = fields.clone();
        let before = self.stored.buttons.clone();
        self.set_buttons(fields);
        self.stored.buttons != before
    }

    // ---- the pairing

    /// Opens a pairing window and makes the code a phone takes (pasted, or scanned from its picture). It holds the one-time secret and is
    /// good for five minutes; a window that was open is replaced (its code no longer works).
    ///
    /// # Errors
    /// Why there is no code: not listening, no address yet, four phones paired.
    pub fn begin_pairing(&mut self, now: Millis) -> Result<(), PairError> {
        let Some(host) = self.host.as_mut() else {
            return Err(PairError::NotListening);
        };
        if host.paired().len() >= MAX_PAIRED_PHONES {
            return Err(PairError::TooManyPhones);
        }
        let Some(offer) = host.begin_pairing(now) else {
            return Err(PairError::NoAddress);
        };
        let text = offer.to_text();
        let qr = QrCode::encode(text.as_bytes()).ok();
        self.offer = Some(Offer { text, qr, opened_at: now, copied: false });
        self.asked = None;
        self.note("a pairing code was made");
        Ok(())
    }

    /// Closes the pairing window: the code no longer works, a phone that waits is told no.
    pub fn cancel_pairing(&mut self) {
        if let Some(host) = self.host.as_mut() {
            host.cancel_pairing();
        }
        self.offer = None;
        self.asked = None;
    }

    /// The code that is open.
    #[must_use]
    pub fn offer(&self) -> Option<&Offer> {
        self.offer.as_ref()
    }

    /// The user copied the code: it is marked so (the window says "Copied"), and its text is handed back to be put on the clipboard.
    pub fn mark_copied(&mut self) -> Option<String> {
        let offer = self.offer.as_mut()?;
        offer.copied = true;
        Some(offer.text.clone())
    }

    /// The phone that waits for the person to say that the six digits are the same.
    #[must_use]
    pub fn asked(&self) -> Option<&Asked> {
        self.asked.as_ref()
    }

    /// The person said that the six digits are the same: the phone is paired (kept at once: the caller writes it) and told.
    ///
    /// # Errors
    /// Nobody waits any more, or four phones are paired.
    pub fn accept(&mut self, now: Millis) -> Result<(), ConfirmError> {
        let Some(asked) = self.asked.clone() else {
            return Err(ConfirmError::NobodyWaiting);
        };
        let result = match self.host.as_mut() {
            Some(host) => {
                let result = host.accept_pairing(now, &asked.peer);
                if result.is_ok() {
                    self.stored.set_phones(host.paired());
                }
                result
            }
            None => Err(ConfirmError::NobodyWaiting),
        };
        if result.is_ok() {
            self.offer = None;
            self.note("a phone was paired");
        }
        self.asked = None;
        result
    }

    /// The person said that the six digits are not the same: the phone is told no and nothing is paired.
    pub fn decline(&mut self, now: Millis) {
        let Some(asked) = self.asked.take() else {
            return;
        };
        if let Some(host) = self.host.as_mut() {
            host.reject_pairing(now, &asked.peer);
        }
        self.note("a phone was declined");
    }

    /// Forgets a paired phone: it is disconnected and refused from now on (kept at once: the caller writes it). `true` when it was paired.
    pub fn forget(&mut self, now: Millis, peer: &str) -> bool {
        let known = self.stored.phones.iter().any(|p| p.peer == peer);
        if let Some(host) = self.host.as_mut() {
            host.forget(now, peer);
            self.stored.set_phones(host.paired());
        } else {
            self.stored.phones.retain(|p| p.peer != peer);
        }
        if known {
            self.note("a phone was forgotten");
        }
        known
    }

    /// The paired phones and whether each is here (those that were kept, when nothing is bound).
    #[must_use]
    pub fn phones(&self) -> Vec<PhoneInfo> {
        match &self.host {
            Some(host) => host.phones(),
            None => self.stored.phones.iter().map(|p| PhoneInfo { id: p.peer.clone(), name: p.name.clone(), connected: false, rtt_ms: None }).collect(),
        }
    }

    // ---- the writes

    /// A write is wanted: `true` when it can start now (none is in flight); else it is remembered and starts when the one in flight ends.
    fn want_write(&mut self) -> bool {
        if !self.loaded {
            return false;
        }
        if self.saving {
            self.save_pending = true;
            return false;
        }
        self.saving = true;
        true
    }

    /// A write ended: `true` when another was asked for meanwhile and starts now.
    fn write_ended(&mut self, error: Option<&str>) -> bool {
        self.saving = false;
        if let Some(error) = error {
            self.note(&format!("could not keep the companion's file: {error}"));
        }
        std::mem::take(&mut self.save_pending) && self.want_write()
    }

    /// What the state dump says of the companion (never the key).
    #[must_use]
    pub fn json(&self) -> serde_json::Value {
        let (status, paired, here, failed) = match self.status() {
            Status::Off => ("off", 0, 0, None),
            Status::Starting => ("starting", 0, 0, None),
            Status::Listening { paired, here } => ("listening", paired, here, None),
            Status::Failed(why) => ("failed", 0, 0, Some(why)),
        };
        serde_json::json!({
            "enabled": self.stored.enabled,
            "status": status,
            "loaded": self.loaded,
            "has_key": self.stored.key().is_some(),
            "address": self.ticket_known,
            "paired": paired,
            "connected": here,
            "kept_phones": self.stored.phones.len(),
            "phones": self.phones().iter().map(|p| serde_json::json!({"name": p.name, "connected": p.connected})).collect::<Vec<_>>(),
            "pairing_open": self.offer.is_some(),
            "asked": self.asked.as_ref().map(|a| a.name.clone()),
            "failed": failed,
            "interval_ms": self.interval_ms,
            "log": self.log,
        })
    }
}

// Closing the host when the app goes tells the phones; a process that ends closes the endpoint anyway.
impl Drop for Glue {
    fn drop(&mut self) {
        self.close();
    }
}

/// Runs `f` on the companion and the app's state, when the app has them and the glue is not held already.
pub fn with_companion<R>(app: &RefAny, f: impl FnOnce(&mut Glue, &AppState) -> R) -> Option<R> {
    let mut handle = app.clone();
    let state = handle.downcast_ref::<AppState>()?;
    let mut glue = state.companion.try_borrow_mut().ok()?;
    Some(f(&mut glue, &state))
}

// ==== the start ====

/// The window exists: the file of the companion is read (on a Thread). What it says is applied by [`on_loaded`]; when it says the
/// companion is on, it is bound after the first frame.
pub fn start(app: &RefAny, info: &mut CallbackInfo) {
    // (once: a second look at the file would replace what the user changed meanwhile)
    let path = with_companion(app, |g, _| (!std::mem::replace(&mut g.started, true)).then(|| g.path.clone())).flatten();
    if let Some(path) = path {
        companion_io::spawn_load(info, app, path, on_loaded as WriteBackCallbackType);
    }
}

extern "C" fn on_loaded(data: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(loaded) = companion_io::take_loaded(&mut msg) else {
        return Update::DoNothing;
    };
    let bind = with_companion(&data, |g, _| g.apply_loaded(loaded)).unwrap_or(false);
    if bind {
        schedule_bind(&data, &mut info);
    }
    Update::DoNothing
}

/// A timer that fires once, after the first frame, and binds the companion.
fn schedule_bind(app: &RefAny, info: &mut CallbackInfo) {
    let after = || Duration::System(SystemTimeDiff::from_millis(FIRST_FRAME_MS));
    let timer = Timer::create(app.clone(), on_bind, info.get_system_time_fn()).with_delay(after()).with_interval(after());
    info.add_timer(TimerId::unique(), timer);
}

extern "C" fn on_bind(data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    bind_now(&data, &mut info.callback_info);
    TimerCallbackReturn { should_update: Update::RefreshDom, should_terminate: TerminateTimer::Terminate }
}

/// Binds the endpoint and starts polling it, when the companion is on and not bound yet; the status of it is the page's to show.
fn bind_now(app: &RefAny, info: &mut CallbackInfo) {
    let bound = with_companion(app, |g, _| {
        if !g.stored.enabled || g.is_listening() {
            return false;
        }
        let bound = g.bind_real().is_ok();
        println!("AZREVIEW_COMPANION bind {}", if bound { "ok" } else { "failed" });
        bound
    })
    .unwrap_or(false);
    if bound {
        // the key a first start made is kept
        save(app, info);
        ensure_poll_timer(app, info);
    }
}

// ==== the poll timer ====

fn poll_timer(app: &RefAny, info: &CallbackInfo, ms: u64) -> Timer {
    Timer::create(app.clone(), on_poll, info.get_system_time_fn()).with_interval(Duration::System(SystemTimeDiff::from_millis(ms)))
}

/// Starts the timer that polls the host, when there is a host and no such timer.
fn ensure_poll_timer(app: &RefAny, info: &mut CallbackInfo) {
    let started = with_companion(app, |g, _| {
        if !g.is_listening() || g.timer.is_some() {
            return None;
        }
        let id = TimerId::unique();
        g.timer = Some(id);
        g.interval_ms = g.wanted_interval_ms();
        Some((id, g.interval_ms))
    })
    .flatten();
    if let Some((id, ms)) = started {
        let timer = poll_timer(app, info, ms);
        info.add_timer(id, timer);
    }
}

extern "C" fn on_poll(data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let polled = with_companion(&data, |g, s| (g.poll(), g.is_listening(), kit::settings_open(&s.kit)));
    let Some((mut polled, alive, settings_open)) = polled else {
        return TimerCallbackReturn::continue_unchanged();
    };
    if !alive {
        // turned off while the timer ran: it ends with the host
        with_companion(&data, |g, _| g.timer = None);
        return TimerCallbackReturn { should_update: Update::DoNothing, should_terminate: TerminateTimer::Terminate };
    }
    if polled.save {
        save(&data, &mut info.callback_info);
    }
    // what the phones asked of the window
    let mut moved = false;
    for command in std::mem::take(&mut polled.commands) {
        moved |= match command {
            Command::Viewport(op) => crate::companion_cmd::viewport(&data, &mut info.callback_info, op),
            Command::Button { peer, request_id, text } => crate::companion_cmd::button(&data, &mut info.callback_info, &peer, request_id, &text),
        };
    }
    // what the phones are told: the window as it is now
    if with_companion(&data, |g, _| g.take_dirty()).unwrap_or(false) {
        crate::companion_cmd::publish(&data, &info.callback_info);
    }
    let update = if polled.attention || moved || (polled.changed && settings_open) { Update::RefreshDom } else { Update::DoNothing };
    if let Some(ms) = polled.rate {
        // the same look at the clock at another pace: a new timer takes over from this one
        let id = TimerId::unique();
        with_companion(&data, |g, _| {
            g.timer = Some(id);
            g.interval_ms = ms;
        });
        let timer = poll_timer(&data, &info.callback_info, ms);
        info.callback_info.add_timer(id, timer);
        return TimerCallbackReturn { should_update: update, should_terminate: TerminateTimer::Terminate };
    }
    TimerCallbackReturn { should_update: update, should_terminate: TerminateTimer::Continue }
}

// ==== the file ====

/// Writes what is kept, on a Thread; one write is in flight at a time and another asked for meanwhile follows it.
pub fn save(app: &RefAny, info: &mut CallbackInfo) {
    let job = with_companion(app, |g, _| if g.want_write() { g.to_write() } else { None }).flatten();
    if let Some((path, bytes)) = job {
        companion_io::spawn_save(info, app, path, bytes, on_saved as WriteBackCallbackType);
    }
}

extern "C" fn on_saved(data: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(saved) = companion_io::take_saved(&mut msg) else {
        return Update::DoNothing;
    };
    let again = with_companion(&data, |g, _| g.write_ended(saved.error.as_deref())).unwrap_or(false);
    if again {
        // what changed while the write was in flight is written now (from what is kept now)
        let job = with_companion(&data, |g, _| g.to_write()).flatten();
        if let Some((path, bytes)) = job {
            companion_io::spawn_save(&mut info, &data, path, bytes, on_saved as WriteBackCallbackType);
        }
    }
    Update::DoNothing
}

// ==== the settings page ====

/// The labels of the switch, in the order of its values (Off, On).
static SWITCH_LABELS: [&str; 2] = ["azreview-companion-off", "azreview-companion-on"];

/// What the line under the switch says.
#[must_use]
pub fn status_text(status: &Status) -> String {
    match status {
        Status::Off => t("azreview-companion-status-off"),
        Status::Starting => t("azreview-companion-status-starting"),
        Status::Listening { paired, here } => t_args(
            "azreview-companion-status-on",
            &[("paired", Arg::from(*paired)), ("max", Arg::from(MAX_PAIRED_PHONES)), ("here", Arg::from(*here))],
        ),
        Status::Failed(why) => t_args("azreview-companion-status-failed", &[("why", Arg::from(why.clone()))]),
    }
}

/// The phone companion's section of the settings page: the switch (off until the user turns it on), what it is doing, and what it is.
#[must_use]
pub fn settings_section(s: &AppState, app: &RefAny) -> kit::AppSection {
    let (on, status) = s.companion.try_borrow().map_or((false, Status::Off), |g| (g.stored.enabled, g.status()));
    let content = Dom::create_div()
        .with_css("display: flex; flex-direction: column;")
        .with_child(kit::row(
            "azreview-companion-when",
            Segmented::create(labels(&SWITCH_LABELS))
                .with_selected_index(usize::from(on))
                .with_on_change(app.clone(), on_switch as SegmentedOnChangeCallbackType)
                .dom()
                .with_id(ids::SETTING_COMPANION),
        ))
        .with_child(
            Dom::create_span_with_text(status_text(&status).as_str())
                .with_id(ids::COMPANION_STATUS)
                .with_css("padding: 8px 0px 0px 0px; font-size: 11px; color: system:secondary-text;"),
        )
        .with_child(kit::note("azreview-companion-note").with_id(ids::COMPANION_NOTE));
    let mut content = content;
    for block in [crate::companion_ui::phones_block(s, app), crate::companion_ui::prefs_block(s, app), crate::companion_ui::buttons_block(s, app)].into_iter().flatten() {
        content.add_child(block);
    }
    kit::AppSection { category: 1, title: t("azreview-category-phone"), content }
}

/// The switch was moved: on binds the endpoint now (the window is up) and starts polling it, off closes it; either is kept.
extern "C" fn on_switch(data: RefAny, mut info: CallbackInfo, state: SegmentedState) -> Update {
    let on = state.selected_index == 1;
    let changed = with_companion(&data, |g, _| g.set_enabled(on)).unwrap_or(false);
    if !changed {
        return Update::DoNothing;
    }
    if on {
        bind_now(&data, &mut info);
    }
    save(&data, &mut info);
    Update::RefreshDom
}

// ==== the first screen ====

/// Whether the key on the first screen is shown: not once the companion is on (it has nothing more to offer then).
#[must_use]
pub fn first_run_shown(s: &AppState) -> bool {
    s.companion.try_borrow().map_or(true, |g| !g.stored.enabled)
}

/// The key on the first screen (no folder yet), next to Open folder: "Have a phone? Connect the companion app". It leads to the
/// companion's settings and asks nothing else of the user: the screen is not blocked by it.
#[must_use]
pub fn first_run_key(data: &RefAny) -> Dom {
    let name = label("azreview-companion-first-run");
    Dom::create_node(NodeType::Button)
        .with_css(
            "display: flex; flex-direction: row; align-items: center; align-self: center; gap: 8px; height: 36px; \
             padding: 0px 14px; font-size: 13px; border-radius: 8px; border: 1px solid system:separator; \
             background: transparent; color: system:secondary-text; cursor: pointer; margin-top: 2px;",
        )
        .with_accessibility_name(name.clone())
        .with_callback(EventFilter::Hover(HoverEventFilter::Click), data.clone(), on_first_run)
        .with_id(ids::COMPANION_FIRST_RUN)
        .with_child(Dom::create_icon("smartphone").with_css("font-size: 18px;"))
        .with_child(Dom::create_span_with_text(name.as_str()))
}

/// The key on the first screen was pressed: the settings page opens at the phone companion.
extern "C" fn on_first_run(mut data: RefAny, _info: CallbackInfo) -> Update {
    let kit_ref = data.downcast_ref::<AppState>().map(|s| s.kit.clone());
    if let Some(kit_ref) = kit_ref {
        kit::open_settings(&kit_ref, Some("phone"));
    }
    Update::RefreshDom
}

#[cfg(test)]
#[path = "companion_plug_test.rs"]
mod companion_plug_test;
