//! The phone's connection in the simulator: the real [`PhoneApi`] the screens talk to, answered by a [`SimMac`] instead
//! of a network. Frames the Mac sends travel with a latency (so the screens meet a link that is not instant), and the
//! simulator's panel can set the session (searching, a pairing code, the link lost, the Mac not found) with
//! [`SimCommand`]s. The real client over the real link replaces this in the phone app; the screens do not know.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use azul_review_link::frame::{ButtonSet, Focus, Frame, Phase, ViewportOp};
use azul_review_link::limits::Capped;
use azul_review_link::link::Millis;
use azul_review_link::pairing::{OfferError, PairingOffer};
use azul_review_link::phone::PhoneApi;
use azul_review_link::view::{FailureKind, PhoneView, Session};

use super::mac::SimMac;

/// The code both screens show in the simulator's pairing.
pub const SIM_CODE: u32 = 482_913;

/// What the panel asks the simulated connection to do.
#[derive(Debug, Clone, PartialEq)]
pub enum SimCommand {
    /// Show this session at once.
    Session(Session),
    /// The person at the Mac confirms the pairing code.
    MacConfirms,
    /// The person at the Mac declines it.
    MacDeclines,
    /// The link takes this long one way, in milliseconds.
    Latency(u64),
    /// Forget everything the phone was told: a phone that was just started (the session stays connected).
    Fresh,
    /// The link goes down this long from now (the phone has been told what the Mac said so far).
    LinkLostAfter(u64),
}

/// The commands the panel has given and the connection has not taken yet.
pub type Commands = Arc<Mutex<Vec<SimCommand>>>;

/// The simulated connection.
pub struct SimPhone {
    view: PhoneView,
    mac: Arc<Mutex<SimMac>>,
    commands: Commands,
    inflight: VecDeque<(Millis, Frame)>,
    latency: u64,
    next_request: u32,
    /// A session change that happens at a time (the search finds the Mac after a moment).
    scheduled: Option<(Millis, Session)>,
}

impl SimPhone {
    /// A phone that is connected to `mac`.
    #[must_use]
    pub fn new(mac: Arc<Mutex<SimMac>>, commands: Commands) -> SimPhone {
        let name = mac.lock().map(|m| m.name.clone()).unwrap_or_default();
        let mut view = PhoneView::new();
        view.session = Session::Connected { mac_name: name.clone() };
        view.mac_name = name;
        view.latency_ms = Some(24);
        SimPhone { view, mac, commands, inflight: VecDeque::new(), latency: 12, next_request: 1, scheduled: None }
    }

    /// Takes the panel's commands, in the order they were given. True when any was.
    fn take_commands(&mut self, now: Millis) -> bool {
        let mut changed = false;
        let commands: Vec<SimCommand> = self.commands.lock().map(|mut c| std::mem::take(&mut *c)).unwrap_or_default();
        for command in commands {
            changed = true;
            match command {
                SimCommand::Session(session) => {
                    self.scheduled = None;
                    if matches!(session, Session::Connected { .. }) {
                        // a link that comes back says everything again
                        if let Ok(mut mac) = self.mac.lock() {
                            mac.reset_sent();
                        }
                    }
                    self.show(session);
                }
                SimCommand::MacConfirms => {
                    if matches!(self.view.session, Session::Pairing { .. }) {
                        let name = self.mac_name();
                        if let Ok(mut mac) = self.mac.lock() {
                            mac.reset_sent();
                        }
                        self.show(Session::Connected { mac_name: name });
                    }
                }
                SimCommand::MacDeclines => {
                    if matches!(self.view.session, Session::Pairing { .. }) {
                        self.view.session = Session::Failed { kind: FailureKind::Declined };
                    }
                }
                SimCommand::Latency(ms) => self.latency = ms,
                SimCommand::Fresh => {
                    let name = self.mac_name();
                    self.view = PhoneView::new();
                    self.view.session = Session::Connected { mac_name: name.clone() };
                    self.view.mac_name = name;
                    self.view.latency_ms = Some((2 * self.latency) as u32);
                    self.inflight.clear();
                    self.scheduled = None;
                    if let Ok(mut mac) = self.mac.lock() {
                        mac.reset_sent();
                    }
                }
                SimCommand::LinkLostAfter(ms) => {
                    self.scheduled = Some((now + ms, Session::Lost { mac_name: self.mac_name(), attempt: 3 }));
                }
            }
        }
        changed
    }

    fn request(&mut self) -> u32 {
        let id = self.next_request;
        self.next_request += 1;
        id
    }

    fn connected(&self) -> bool {
        self.view.is_connected()
    }

    fn send_later(&mut self, now: Millis, frame: Frame) {
        self.inflight.push_back((now + 2 * self.latency, frame));
    }

    fn show(&mut self, session: Session) {
        if let Session::Connected { mac_name } | Session::Lost { mac_name, .. } | Session::Pairing { mac_name, .. } = &session {
            self.view.mac_name = mac_name.clone();
        }
        self.view.session = session;
    }

    fn mac_name(&self) -> String {
        self.mac.lock().map(|m| m.name.clone()).unwrap_or_default()
    }

    fn start_search(&mut self, now: Millis, attempt: u32) {
        let online = self.mac.lock().map(|m| m.online).unwrap_or(false);
        self.view.session = Session::Searching { attempt };
        self.scheduled = Some(if online {
            (now + 1_800, Session::Pairing { mac_name: self.mac_name(), code: SIM_CODE })
        } else {
            (now + 3_200, Session::Failed { kind: FailureKind::NotFound })
        });
    }
}

impl PhoneApi for SimPhone {
    fn view(&self) -> &PhoneView {
        &self.view
    }

    fn poll(&mut self, now: Millis) -> bool {
        let mut changed = self.take_commands(now);
        if let Some((at, session)) = self.scheduled.clone() {
            if now >= at {
                self.scheduled = None;
                self.view.session = session;
                changed = true;
            }
        }
        if self.connected() {
            let frames = self.mac.lock().map(|mut m| m.tick(now)).unwrap_or_default();
            for frame in frames {
                self.inflight.push_back((now + self.latency, frame));
            }
            let ms = (2 * self.latency) as u32;
            if self.view.latency_ms != Some(ms) {
                self.view.latency_ms = Some(ms);
                changed = true;
            }
        } else {
            self.inflight.clear();
        }
        while self.inflight.front().is_some_and(|(due, _)| *due <= now) {
            if let Some((_, frame)) = self.inflight.pop_front() {
                changed |= self.view.apply_frame(&frame);
            }
        }
        changed
    }

    fn gesture(&mut self, now: Millis, id: u32, phase: Phase, pan: (f32, f32), zoom: f32, focus: Focus) {
        self.take_commands(now);
        if self.connected() {
            if let Ok(mut mac) = self.mac.lock() {
                mac.apply(now, ViewportOp::Gesture { id, phase, pan_x: pan.0, pan_y: pan.1, zoom, focus });
            }
        }
    }

    fn fit(&mut self, now: Millis) {
        self.take_commands(now);
        if self.connected() {
            if let Ok(mut mac) = self.mac.lock() {
                mac.apply(now, ViewportOp::Fit);
            }
        }
    }

    fn page_step(&mut self, now: Millis, step: i8) {
        self.take_commands(now);
        if self.connected() {
            if let Ok(mut mac) = self.mac.lock() {
                mac.apply(now, ViewportOp::PageStep(step));
            }
        }
    }

    fn reset_view(&mut self, now: Millis) {
        self.take_commands(now);
        if self.connected() {
            if let Ok(mut mac) = self.mac.lock() {
                mac.apply(now, ViewportOp::Reset);
            }
        }
    }

    fn list_dir(&mut self, now: Millis, path: &str, offset: u32) {
        self.take_commands(now);
        self.view.begin_listing(path);
        if !self.connected() {
            self.view.files_failed("not connected");
            return;
        }
        let id = self.request();
        let rows = self.mac.lock().map(|m| m.list(id, path, offset)).ok();
        if let Some(rows) = rows {
            self.send_later(now, Frame::Rows(rows));
        }
    }

    fn open_file(&mut self, now: Millis, path: &str, line: Option<u32>) {
        self.take_commands(now);
        if self.connected() {
            if let Ok(mut mac) = self.mac.lock() {
                mac.open_file(now, path, line);
            }
        }
    }

    fn press_button(&mut self, now: Millis, index: usize) {
        self.take_commands(now);
        if !self.connected() || index >= self.view.buttons.buttons.len() {
            return;
        }
        let id = self.request();
        self.view.button_pressed(index, id);
        if let Ok(mut mac) = self.mac.lock() {
            mac.press_button(now, index, id);
        }
    }

    fn set_buttons(&mut self, now: Millis, buttons: &ButtonSet) -> Vec<Capped> {
        self.take_commands(now);
        let (caps, kept) = match self.mac.lock() {
            Ok(mut mac) => (mac.set_buttons(now, buttons), mac.buttons.clone()),
            Err(_) => (Vec::new(), buttons.clone()),
        };
        self.send_later(now, Frame::Buttons(kept));
        caps
    }

    fn begin_pairing(&mut self, now: Millis, offer: &str) -> Result<(), OfferError> {
        self.take_commands(now);
        PairingOffer::parse(offer)?;
        self.start_search(now, 1);
        Ok(())
    }

    fn retry(&mut self, now: Millis) {
        self.take_commands(now);
        let attempt = match &self.view.session {
            Session::Searching { attempt } | Session::Lost { attempt, .. } => attempt + 1,
            _ => 1,
        };
        self.start_search(now, attempt);
    }

    fn forget_mac(&mut self, _now: Millis) {
        self.scheduled = None;
        self.view.session = Session::Unpaired;
        self.view.mac_name.clear();
        self.view.latency_ms = None;
    }
}

#[cfg(test)]
#[path = "phone_test.rs"]
mod phone_test;
