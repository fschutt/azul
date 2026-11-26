//! The phone's screens and what a tap does. The state is [`Root`] (the connection behind [`PhoneApi`], what the screens
//! remember in [`Ui`], the words); everything a tap can do is an [`Action`] and [`Root::perform`] does it, so the whole
//! behaviour of the app is testable without a window. The DOM is built from that state alone (one function per
//! screen), the same on a phone and in the simulator.

use std::time::Instant;

use azul_review_link::frame::{is_safe_relative_path, AgentFinished, ButtonSet, Focus, Haptic, Phase, QuickButton, Rect, ReplyStatus};
use azul_review_link::limits::Capped;
use azul_review_link::link::Millis;
use azul_review_link::phone::PhoneApi;
use azul_review_link::view::{PhoneView, Session};
use azul_review_link::viewport::{apply_gesture_in, resolve_focus, PanDirection, Space};

use crate::env::{Env, Lang};
use crate::l10n::Strings;
use crate::pad::{PadOut, PadScale, PadState};

pub mod chrome;
pub mod editor;
pub mod files;
pub mod glue;
pub mod kit;
pub mod pair;
pub mod remote;
pub mod settings;

/// The screens behind the tabs and the gear.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Remote,
    Files,
    /// The four quick buttons' editor.
    Editor,
    Settings,
}

/// Everything a tap can do.
#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Tab(Screen),
    OpenSettings,
    Back,
    Fit,
    ResetView,
    PageStep(i8),
    /// A tap on a quick button.
    Button(usize),
    /// A long press on a quick button (or the editor row in the settings).
    EditButtons(Option<usize>),
    OpenDir(String),
    OpenFile(String),
    /// A crumb of the breadcrumb: how many folders deep to go back to (0: the workspace).
    Crumb(usize),
    LoadMore,
    RetryFiles,
    EditorSave,
    EditorReset,
    ForgetAsk,
    ForgetYes,
    ForgetNo,
    PairFind,
    PairScan,
    PairCancel,
    PairRetry,
    PairStart,
    /// "Pair again": the pairing screen over a Mac that cannot be reached any more.
    PairAgain,
    /// Back from that screen.
    PairBack,
    OpenSystemSettings,
    /// The page strip: a tap or a swipe turned the page.
    Strip(i8),
    /// The first-use hint over the pad was dismissed.
    DismissHint,
    /// "Continue reviewing": back to the return point.
    ContinueReviewing,
    /// "Try it without a Mac": a pretend Mac answers.
    Demo,
    /// Back from the demo to the real connection.
    LeaveDemo,
    /// The simulator's panel.
    Sim(crate::sim::SimAction),
}

/// What the screens remember between frames.
#[derive(Debug, Clone)]
pub struct Ui {
    pub screen: Screen,
    /// The screens Back goes back through (the gear and the editor push the one they were opened from).
    pub history: Vec<Screen>,
    /// The editor's working copy of the four buttons (label, instruction).
    pub drafts: [(String, String); 4],
    /// What the editor cut from each field, to say so.
    pub draft_cut: [(Option<Capped>, Option<Capped>); 4],
    pub editor_focus: Option<usize>,
    /// The editor has changes that are not saved.
    pub editor_dirty: bool,
    /// The settings screen asks whether to forget the Mac.
    pub forget_ask: bool,
    pub pad: PadState,
    /// The view the Mac showed when the gesture in progress began, and the point its zoom is about.
    pub live_base: Option<LiveBase>,
    /// The pad's hint is shown until the first gesture.
    pub hint: bool,
    /// The "Connected" card after a pairing is shown until Start is tapped.
    pub pair_card: bool,
    /// The pairing screen is open over a Mac that cannot be reached ("Pair again").
    pub pair_again: bool,
    /// What the session was at the last look (a pairing that just went through shows the card).
    pub last_session: SessionKind,
    /// When each button's last status was first seen (the green tick goes away after a while).
    pub status_seen: [Option<(ReplyStatus, Millis)>; 4],
    /// A long press just opened the editor: the click that follows it is not a press.
    pub suppress_click_until: Millis,
    /// The file the phone just asked AzReview to open (marked in the list for a moment).
    pub opened: Option<(String, Millis)>,
    /// A line shown for a moment at the bottom ("Saved in AzReview").
    pub toast: Option<(String, Millis)>,
    /// How many agents that finished the phone has taken note of.
    pub finished_seen: u32,
    /// The agent that finished last, shown for a moment.
    pub finished_note: Option<(AgentFinished, Millis)>,
    /// The buzz the phone owes (the shell plays it on its next turn).
    pub haptic: Option<Haptic>,
}

/// A gesture in progress, from the Mac's view at its start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiveBase {
    pub id: u32,
    pub base: Rect,
    pub about: (f32, f32),
}

/// The kinds of session, for noticing a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionKind {
    Unpaired,
    Searching,
    Pairing,
    Connected,
    Lost,
    Failed,
}

impl SessionKind {
    /// The kind of `session`.
    #[must_use]
    pub fn of(session: &Session) -> SessionKind {
        match session {
            Session::Unpaired => SessionKind::Unpaired,
            Session::Searching { .. } => SessionKind::Searching,
            Session::Pairing { .. } => SessionKind::Pairing,
            Session::Connected { .. } => SessionKind::Connected,
            Session::Lost { .. } => SessionKind::Lost,
            Session::Failed { .. } => SessionKind::Failed,
        }
    }
}

impl Ui {
    /// The screens as the phone opens them.
    #[must_use]
    pub fn new() -> Ui {
        let defaults = ButtonSet::defaults();
        Ui {
            screen: Screen::Remote,
            history: Vec::new(),
            drafts: std::array::from_fn(|i| (defaults.buttons[i].label.clone(), defaults.buttons[i].text.clone())),
            draft_cut: Default::default(),
            editor_focus: None,
            editor_dirty: false,
            forget_ask: false,
            pad: PadState::new(),
            live_base: None,
            hint: true,
            pair_card: false,
            pair_again: false,
            last_session: SessionKind::Unpaired,
            status_seen: [None; 4],
            suppress_click_until: 0,
            opened: None,
            toast: None,
            finished_seen: 0,
            finished_note: None,
            haptic: None,
        }
    }
}

impl Default for Ui {
    fn default() -> Self {
        Ui::new()
    }
}

/// Where the environment of the screens comes from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum EnvSource {
    /// The window and the system tell (a phone).
    Window,
    /// The simulator says which phone it is.
    Fixed(Env),
}

/// The whole app state.
pub struct Root {
    pub api: Box<dyn PhoneApi>,
    pub ui: Ui,
    pub strings: Strings,
    pub env: EnvSource,
    pub started: Instant,
    /// The simulator's extras (the fake Mac's panel); `None` on a phone.
    pub sim: Option<crate::sim::Extras>,
    /// The demo is on: a pretend Mac answers (the real connection waits in `stashed`).
    pub demo: bool,
    stashed: Option<Box<dyn PhoneApi>>,
}

impl Root {
    /// An app with this connection.
    #[must_use]
    pub fn new(api: Box<dyn PhoneApi>, env: EnvSource) -> Root {
        Root { api, ui: Ui::new(), strings: Strings::builtin(), env, started: Instant::now(), sim: None, demo: false, stashed: None }
    }

    /// Milliseconds since the app started.
    #[must_use]
    pub fn now(&self) -> Millis {
        self.started.elapsed().as_millis() as Millis
    }

    /// The words of `key` in `lang`.
    #[must_use]
    pub fn t(&self, lang: Lang, key: &str) -> String {
        self.strings.t(lang, key)
    }

    /// The phone's view of the Mac.
    #[must_use]
    pub fn view(&self) -> &PhoneView {
        self.api.view()
    }

    /// The part of the page the pad shows as the viewport: while a gesture is in progress what it has made of the view
    /// it began on (so the rectangle follows the finger at once), else what the Mac last said.
    #[must_use]
    pub fn shown_view(&self) -> Option<Rect> {
        let pen = self.api.view().pen.as_ref()?;
        match (self.ui.pad.live(), self.ui.live_base) {
            (Some(live), Some(base)) if live.id == base.id => Some(apply_gesture_in(base.base, live.pan, live.zoom, base.about, space_of(&pen.surface))),
            _ => Some(pen.view),
        }
    }

    /// Time passes: the connection works; the screens notice what changed. True when the DOM must be built again.
    pub fn tick(&mut self, now: Millis) -> bool {
        let mut changed = self.api.poll(now);
        let kind = SessionKind::of(&self.api.view().session);
        if kind != self.ui.last_session {
            if kind == SessionKind::Connected && self.ui.last_session == SessionKind::Pairing {
                self.ui.pair_card = true;
            }
            if kind == SessionKind::Connected {
                self.ui.pair_again = false;
            }
            if kind != SessionKind::Connected && kind != SessionKind::Lost {
                self.ui.screen = Screen::Remote;
                self.ui.history.clear();
                self.ui.forget_ask = false;
            }
            self.ui.last_session = kind;
            changed = true;
        }
        for (i, seen) in self.ui.status_seen.iter_mut().enumerate() {
            let status = self.api.view().button_status[i];
            let now_seen = status.map(|s| (s, now));
            match (seen.as_ref(), now_seen) {
                (Some((old, _)), Some((new, _))) if *old == new => {}
                (None, None) => {}
                _ => {
                    *seen = now_seen;
                    changed = true;
                }
            }
        }
        let (count, finished, settings) = {
            let v = self.api.view();
            (v.finished_count, v.finished.clone(), v.settings)
        };
        if count != self.ui.finished_seen {
            // news: an agent finished (a count that went down is a fresh connection, nothing to announce)
            if count.wrapping_sub(self.ui.finished_seen) as i32 > 0 {
                if let Some(done) = finished {
                    self.ui.haptic = settings.haptic_for(done.kind);
                    self.ui.finished_note = Some((done, now));
                }
            }
            self.ui.finished_seen = count;
            changed = true;
        }
        if let Some((_, at)) = &self.ui.finished_note {
            if now.saturating_sub(*at) > FINISHED_SHOWN_MS {
                self.ui.finished_note = None;
                changed = true;
            }
        }
        if let Some((_, at)) = self.ui.toast {
            if now.saturating_sub(at) > 2500 {
                self.ui.toast = None;
                changed = true;
            }
        }
        if let Some((_, at)) = &self.ui.opened {
            if now.saturating_sub(*at) > 1800 {
                self.ui.opened = None;
                changed = true;
            }
        }
        // a finished conversation's tick goes away after a few seconds
        for seen in &self.ui.status_seen {
            if let Some((ReplyStatus::Done, at)) = seen {
                if now.saturating_sub(*at) > DONE_SHOWN_MS && now.saturating_sub(*at) < DONE_SHOWN_MS + 80 {
                    changed = true;
                }
            }
        }
        changed
    }

    /// Does what a tap asks. True when the DOM must be built again.
    pub fn perform(&mut self, now: Millis, action: Action) -> bool {
        match action {
            Action::Tab(screen @ (Screen::Remote | Screen::Files)) => {
                if self.ui.screen == screen {
                    return false;
                }
                self.ui.screen = screen;
                self.ui.history.clear();
                let files = &self.api.view().files;
                if screen == Screen::Files && files.entries.is_empty() && !files.loading() {
                    self.api.list_dir(now, "", 0);
                }
                true
            }
            Action::Tab(_) => false,
            Action::OpenSettings => {
                if self.ui.screen != Screen::Settings {
                    self.ui.history.push(self.ui.screen);
                    self.ui.screen = Screen::Settings;
                    self.ui.forget_ask = false;
                }
                true
            }
            Action::Back => match self.ui.screen {
                Screen::Settings | Screen::Editor => {
                    self.ui.screen = self.ui.history.pop().unwrap_or(Screen::Remote);
                    self.ui.forget_ask = false;
                    true
                }
                _ => false,
            },
            Action::Fit => {
                self.api.fit(now);
                false
            }
            Action::ResetView => {
                self.api.reset_view(now);
                false
            }
            Action::PageStep(step) | Action::Strip(step) => {
                self.api.page_step(now, step);
                false
            }
            Action::Button(index) => {
                if index >= azul_review_link::limits::BUTTON_COUNT || now < self.ui.suppress_click_until {
                    return false;
                }
                self.api.press_button(now, index);
                true
            }
            Action::EditButtons(focus) => {
                let set = self.api.view().buttons.clone();
                self.ui.drafts = std::array::from_fn(|i| (set.buttons[i].label.clone(), set.buttons[i].text.clone()));
                self.ui.draft_cut = Default::default();
                self.ui.editor_focus = focus.filter(|i| *i < azul_review_link::limits::BUTTON_COUNT);
                self.ui.editor_dirty = false;
                if self.ui.screen != Screen::Editor {
                    self.ui.history.push(self.ui.screen);
                }
                self.ui.screen = Screen::Editor;
                self.ui.suppress_click_until = now + LONG_PRESS_RELEASE_MS;
                true
            }
            Action::EditorSave => {
                let set = ButtonSet {
                    buttons: std::array::from_fn(|i| QuickButton { label: self.ui.drafts[i].0.clone(), text: self.ui.drafts[i].1.clone() }),
                };
                let cut = self.api.set_buttons(now, &set);
                self.ui.toast = Some((String::from(if cut.is_empty() { "editor-saved" } else { "editor-capped" }), now));
                self.ui.editor_dirty = false;
                self.ui.screen = self.ui.history.pop().unwrap_or(Screen::Remote);
                true
            }
            Action::EditorReset => {
                let defaults = ButtonSet::defaults();
                self.ui.drafts = std::array::from_fn(|i| (defaults.buttons[i].label.clone(), defaults.buttons[i].text.clone()));
                self.ui.draft_cut = Default::default();
                self.ui.editor_dirty = true;
                true
            }
            Action::OpenDir(path) => {
                if !is_safe_relative_path(&path) {
                    return false;
                }
                self.api.list_dir(now, &path, 0);
                true
            }
            Action::OpenFile(path) => {
                if !is_safe_relative_path(&path) || path.is_empty() {
                    return false;
                }
                self.api.open_file(now, &path, None);
                self.ui.opened = Some((path, now));
                self.ui.toast = Some((String::from("files-opened"), now));
                true
            }
            Action::Crumb(depth) => {
                let dir = self.api.view().files.dir.clone();
                let target: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).take(depth).collect();
                self.api.list_dir(now, &target.join("/"), 0);
                true
            }
            Action::LoadMore => {
                let files = &self.api.view().files;
                let (dir, have) = (files.dir.clone(), files.entries.len() as u32);
                self.api.list_dir(now, &dir, have);
                true
            }
            Action::RetryFiles => {
                let dir = self.api.view().files.dir.clone();
                self.api.list_dir(now, &dir, 0);
                true
            }
            Action::ForgetAsk => {
                self.ui.forget_ask = true;
                true
            }
            Action::ForgetNo => {
                self.ui.forget_ask = false;
                true
            }
            Action::ForgetYes => {
                self.api.forget_mac(now);
                self.ui.forget_ask = false;
                self.ui.screen = Screen::Remote;
                self.ui.history.clear();
                true
            }
            Action::PairFind | Action::PairRetry => {
                self.api.retry(now);
                true
            }
            Action::PairScan => {
                self.ui.toast = Some((String::from("pair-scan-soon"), now));
                true
            }
            Action::PairCancel => {
                self.api.forget_mac(now);
                true
            }
            Action::PairStart => {
                self.ui.pair_card = false;
                true
            }
            Action::PairAgain => {
                self.ui.pair_again = true;
                true
            }
            Action::PairBack => {
                self.ui.pair_again = false;
                true
            }
            Action::OpenSystemSettings => false,
            Action::DismissHint => {
                self.ui.hint = false;
                true
            }
            Action::ContinueReviewing => {
                self.api.continue_reviewing(now);
                true
            }
            Action::Demo => {
                self.start_demo();
                true
            }
            Action::LeaveDemo => {
                self.leave_demo();
                true
            }
            Action::Sim(action) => match self.sim.take() {
                Some(mut sim) => {
                    let changed = sim.perform(self, now, action);
                    self.sim = Some(sim);
                    changed
                }
                None => false,
            },
        }
    }

    /// The demo: the screens talk to a pretend Mac inside the app (its pen walks on its own, a button press starts a conversation
    /// that ends in a suggestion and a result, each of which announces itself) and nothing is sent anywhere.
    pub fn start_demo(&mut self) {
        if self.demo {
            return;
        }
        let mut mac = crate::sim::mac::SimMac::new();
        mac.name = "Demo Mac".to_string();
        mac.pen_walk = true;
        mac.announce_finished = true;
        let phone = crate::sim::phone::SimPhone::new(std::sync::Arc::new(std::sync::Mutex::new(mac)), std::sync::Arc::new(std::sync::Mutex::new(Vec::new())));
        self.stashed = Some(std::mem::replace(&mut self.api, Box::new(phone)));
        self.demo = true;
        self.ui = Ui::new();
    }

    /// Leaves the demo: the real connection is back, as it was.
    pub fn leave_demo(&mut self) {
        if let Some(real) = self.stashed.take() {
            self.api = real;
        }
        self.demo = false;
        self.ui = Ui::new();
    }

    /// The buzz the phone owes, once (the shell plays it).
    pub fn take_haptic(&mut self) -> Option<Haptic> {
        self.ui.haptic.take()
    }

    /// What the pad's pointer and pinch events become. `pad` is the pad's size in logical pixels.
    pub fn pad_event(&mut self, now: Millis, event: PadEvent, pad: (f32, f32)) -> bool {
        let view = self.api.view();
        let pen = view.pen.clone();
        let direction = PanDirection::from_reverse(view.settings.reverse_pan);
        let outs = match (event, &pen) {
            (PadEvent::Cancel, _) => self.ui.pad.cancel(),
            (_, None) => return false,
            (PadEvent::Secondary, _) => self.ui.pad.secondary_tap(),
            (PadEvent::Down { x, y }, _) => self.ui.pad.pointer_down(now, x, y),
            (PadEvent::Up { x, y }, _) => self.ui.pad.pointer_up(now, x, y),
            (PadEvent::Pinch { scale, began, ended }, _) => self.ui.pad.pinch(now, scale, began, ended),
            (PadEvent::Move { x, y }, Some(pen)) => {
                // the scale of the view the gesture began on: the sheets under the finger keep their size while it drags
                let base = self.ui.live_base.map_or(pen.view, |b| b.base);
                let px_per_unit = crate::padscene::px_per_unit(pad, pen.page().is_some(), base);
                self.ui.pad.pointer_move(now, x, y, &PadScale { px_per_unit, direction })
            }
        };
        let moved = !outs.is_empty() || self.ui.pad.live().is_some();
        for out in outs {
            match out {
                PadOut::Gesture { id, phase, pan, zoom } => {
                    if phase == Phase::Begin {
                        if let Some(pen) = &pen {
                            let about = resolve_focus(Focus::Pen, pen.view, pen.pen.map(|p| (p.x, p.y)));
                            self.ui.live_base = Some(LiveBase { id, base: pen.view, about });
                        }
                        self.ui.hint = false;
                    }
                    self.api.gesture(now, id, phase, pan, zoom, Focus::Pen);
                    if phase == Phase::End {
                        self.ui.live_base = None;
                    }
                }
                PadOut::Fit => self.api.fit(now),
                PadOut::Reset => self.api.reset_view(now),
            }
        }
        moved
    }
}

/// Where the view may go on a surface.
#[must_use]
pub fn space_of(surface: &azul_review_link::frame::Surface) -> Space {
    match surface {
        azul_review_link::frame::Surface::Page(page) => Space::of_pages(page),
        azul_review_link::frame::Surface::Whiteboard(board) => Space::of_board(board.aspect),
    }
}

/// How long after a long press a click is taken for its release, not a press.
pub const LONG_PRESS_RELEASE_MS: Millis = 600;

/// How long the line about an agent that finished stays at the bottom.
pub const FINISHED_SHOWN_MS: Millis = 5_000;

/// How long a "Done" tick stays on a quick button.
pub const DONE_SHOWN_MS: Millis = 6_000;

/// What happens on the pad, in the pad's own pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PadEvent {
    Down { x: f32, y: f32 },
    Move { x: f32, y: f32 },
    Up { x: f32, y: f32 },
    /// The pointer left the pad or the system took the touch.
    Cancel,
    Pinch { scale: f32, began: bool, ended: bool },
    /// A right click or a two-finger tap.
    Secondary,
}

#[cfg(test)]
#[path = "mod_test.rs"]
mod mod_test;
