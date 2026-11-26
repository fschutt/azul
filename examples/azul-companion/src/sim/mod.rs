//! The macOS simulator of the phone: a window that shows exactly the phone's screens at the size of a phone (the same DOM,
//! the same callbacks, the same connection interface as on a device), in a bezel with the safe areas drawn, beside a panel
//! that plays the Mac and keeps a log of every command the phone sends. Everything here exists only for the simulator and
//! for the screenshots; the phone's own screens know nothing of it.
//!
//! Mouse to touch: a drag is a pan, a trackpad pinch or Ctrl and scroll zooms around the pen, two quick clicks are a double
//! tap, a right click is a two-finger tap (reset).

use std::sync::{Arc, Mutex};

use azul::prelude::*;
use azul_review_link::view::{FailureKind, Session};

use crate::env::{Env, Lang, DEVICES};
use crate::ui::kit::{column, div, px, row, text, Cx};
use crate::ui::{Action, EnvSource, Root};

pub mod check;
pub mod mac;
pub mod phone;
pub mod scenes;
pub mod shots;

use mac::SimMac;
use phone::{Commands, SimCommand, SIM_CODE};

/// The width of the panel that plays the Mac.
pub const PANEL_W: f32 = 340.0;
/// The bezel around the phone.
pub const BEZEL: f32 = 12.0;
/// The room around the bezel.
pub const STAGE_PAD: f32 = 24.0;

/// A session the panel can show at once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionScene {
    Unpaired,
    Searching,
    Found,
    Connected,
    Lost,
    NotFound,
    NoPermission,
    AddressChanged,
    UpdateCompanion,
    UpdateAzReview,
}

impl SessionScene {
    /// The session this scene shows, for a Mac called `mac`.
    #[must_use]
    pub fn session(self, mac: &str) -> Session {
        let name = mac.to_string();
        match self {
            SessionScene::Unpaired => Session::Unpaired,
            SessionScene::Searching => Session::Searching { attempt: 2 },
            SessionScene::Found => Session::Pairing { mac_name: name, code: SIM_CODE },
            SessionScene::Connected => Session::Connected { mac_name: name },
            SessionScene::Lost => Session::Lost { mac_name: name, attempt: 3 },
            SessionScene::NotFound => Session::Failed { kind: FailureKind::NotFound },
            SessionScene::NoPermission => Session::Failed { kind: FailureKind::NoPermission },
            SessionScene::AddressChanged => Session::Failed { kind: FailureKind::AddressChanged },
            SessionScene::UpdateCompanion => Session::Failed { kind: FailureKind::UpdateCompanion },
            SessionScene::UpdateAzReview => Session::Failed { kind: FailureKind::UpdateAzReview },
        }
    }

    fn label(self) -> &'static str {
        match self {
            SessionScene::Unpaired => "First start",
            SessionScene::Searching => "Searching",
            SessionScene::Found => "Found + code",
            SessionScene::Connected => "Connected",
            SessionScene::Lost => "Link lost",
            SessionScene::NotFound => "Mac not found",
            SessionScene::NoPermission => "No permission",
            SessionScene::AddressChanged => "Address changed",
            SessionScene::UpdateCompanion => "App too old",
            SessionScene::UpdateAzReview => "AzReview too old",
        }
    }
}

/// What the panel can do.
#[derive(Debug, Clone, PartialEq)]
pub enum SimAction {
    Device(usize),
    Landscape(bool),
    Safe(bool),
    Lang(Lang),
    Scene(SessionScene),
    MacConfirms,
    MacDeclines,
    PenWalk(bool),
    Region(bool),
    MacPage(i8),
    Latency(u64),
}

/// The simulator's state beside the phone's.
pub struct Extras {
    pub mac: Arc<Mutex<SimMac>>,
    pub commands: Commands,
    pub device: usize,
    pub landscape: bool,
    pub lang: Lang,
    pub safe: bool,
    pub latency: u64,
    /// The window should be this big (a different phone was chosen).
    pub resize_wanted: Option<(f32, f32)>,
    /// Show the bezel and the panel (a screenshot of the phone alone leaves them out).
    pub chrome: bool,
    /// A screenshot run in progress.
    pub player: Option<shots::Player>,
}

impl Extras {
    /// The simulator around `mac`, an iPhone in portrait, in English.
    #[must_use]
    pub fn new(mac: Arc<Mutex<SimMac>>, commands: Commands) -> Extras {
        Extras { mac, commands, device: 0, landscape: false, lang: Lang::En, safe: true, latency: 12, resize_wanted: None, chrome: true, player: None }
    }

    /// What the chosen phone reports.
    #[must_use]
    pub fn env(&self) -> Env {
        DEVICES[self.device.min(DEVICES.len() - 1)].env(self.landscape, self.lang, self.safe)
    }

    /// The window that holds the phone in its bezel and the panel.
    #[must_use]
    pub fn window_size(&self) -> (f32, f32) {
        let env = self.env();
        if !self.chrome {
            return (env.width, env.height);
        }
        let stage_w = env.width + 2.0 * BEZEL + 2.0 * STAGE_PAD;
        let stage_h = env.height + 2.0 * BEZEL + 2.0 * STAGE_PAD;
        (stage_w + PANEL_W, stage_h.max(760.0))
    }

    /// Does what the panel asked. True when the screens must be built again.
    pub fn perform(&mut self, root: &mut Root, now: u64, action: SimAction) -> bool {
        let _ = now;
        let name = self.mac.lock().map(|m| m.name.clone()).unwrap_or_default();
        let push = |c: SimCommand| {
            if let Ok(mut commands) = self.commands.lock() {
                commands.push(c);
            }
        };
        match action {
            SimAction::Device(i) if i < DEVICES.len() => self.device = i,
            SimAction::Device(_) => return false,
            SimAction::Landscape(on) => self.landscape = on,
            SimAction::Safe(on) => self.safe = on,
            SimAction::Lang(lang) => self.lang = lang,
            SimAction::Scene(scene) => {
                push(SimCommand::Session(scene.session(&name)));
                return true;
            }
            SimAction::MacConfirms => {
                push(SimCommand::MacConfirms);
                return true;
            }
            SimAction::MacDeclines => {
                push(SimCommand::MacDeclines);
                return true;
            }
            SimAction::PenWalk(on) => {
                if let Ok(mut mac) = self.mac.lock() {
                    mac.pen_walk = on;
                }
                return true;
            }
            SimAction::Region(on) => {
                if let Ok(mut mac) = self.mac.lock() {
                    mac.armed_region = on;
                }
                return true;
            }
            SimAction::MacPage(step) => {
                if let Ok(mut mac) = self.mac.lock() {
                    mac.apply(now, azul_review_link::frame::ViewportOp::PageStep(step));
                }
                return true;
            }
            SimAction::Latency(ms) => {
                self.latency = ms;
                push(SimCommand::Latency(ms));
                return true;
            }
        }
        root.env = EnvSource::Fixed(self.env());
        self.resize_wanted = Some(self.window_size());
        true
    }

    /// Time passes (the window is resized when a different phone was chosen). True when the screens must be built again.
    pub fn tick(&mut self, root: &mut Root, now: u64, info: &mut CallbackInfo) -> bool {
        if let Some(mut player) = self.player.take() {
            match player.step(root, &*self, now, info) {
                shots::Step::Working(refresh) => {
                    self.player = Some(player);
                    return refresh;
                }
                shots::Step::Finished(code) => std::process::exit(code),
            }
        }
        if let Some((w, h)) = self.resize_wanted.take() {
            let mut state = info.get_current_window_state();
            state.size.dimensions = LogicalSize::create(w, h);
            info.modify_window_state(state);
            return true;
        }
        false
    }
}

// ---- the picture: the phone in its bezel, the panel

/// A pill that stands for the notch, the camera or the home indicator.
fn pill(css: &str) -> Dom {
    div(&format!("position: absolute; {css}"))
}

/// The marks of the system's bars drawn over the phone, where the safe area is.
fn system_marks(cx: &Cx, extras: &Extras) -> Vec<Dom> {
    let env = cx.env;
    let mut marks = Vec::new();
    if !extras.safe {
        return marks;
    }
    let device = &DEVICES[extras.device.min(DEVICES.len() - 1)];
    let ios = device.platform == crate::env::Platform::Ios;
    if ios {
        if env.landscape() {
            marks.push(pill("left: 11px; top: 50%; margin-top: -62px; width: 34px; height: 124px; border-radius: 17px; background: #000000;"));
        } else {
            marks.push(pill("top: 11px; left: 50%; margin-left: -62px; width: 124px; height: 34px; border-radius: 17px; background: #000000;"));
        }
        marks.push(pill("bottom: 8px; left: 50%; margin-left: -67px; width: 134px; height: 5px; border-radius: 3px; background: rgba(255, 255, 255, 0.55);"));
    } else {
        marks.push(pill("top: 8px; left: 50%; margin-left: -7px; width: 14px; height: 14px; border-radius: 7px; background: #000000;"));
        marks.push(pill("bottom: 8px; left: 50%; margin-left: -54px; width: 108px; height: 4px; border-radius: 2px; background: rgba(255, 255, 255, 0.5);"));
    }
    marks
}

fn panel_key(label: &str, on: bool, cx: &Cx, action: SimAction) -> Dom {
    let (bg, fg) = if on { (crate::theme::ACCENT, "#ffffff") } else { ("#2a2a2e", "#d8d8de") };
    Dom::create_node(NodeType::Button)
        .with_css(
            format!(
                "box-sizing: border-box; height: 30px; padding: 0px 10px; border-radius: 8px; border: 0px solid transparent; margin: 0px; \
                 background: {bg}; color: {fg}; font-size: 12px; display: flex; align-items: center; justify-content: center;"
            )
            .as_str(),
        )
        .with_accessibility_name(label)
        .with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            RefAny::new(crate::ui::glue::ActCtx { root: cx.data.clone(), action: Action::Sim(action) }),
            crate::ui::glue::on_act,
        )
        .with_child(Dom::create_div_with_text(label).with_css("font-size: 12px;"))
}

fn heading(words: &str) -> Dom {
    text(words, "font-size: 11px; font-weight: 700; color: #8a8a94; letter-spacing: 1px; padding-top: 6px;")
}

fn keys() -> Dom {
    row("flex-wrap: wrap; gap: 6px; width: 100%;")
}

/// The panel that plays the Mac.
fn panel(cx: &Cx, extras: &Extras, height: f32) -> Dom {
    let mac = extras.mac.lock();
    let (name, pen_walk, region, log) = match &mac {
        Ok(m) => (m.name.clone(), m.pen_walk, m.armed_region, m.log.iter().rev().take(13).map(|l| (l.at, l.text.clone())).collect::<Vec<_>>()),
        Err(_) => (String::new(), false, false, Vec::new()),
    };
    let mut devices = keys();
    for (i, d) in DEVICES.iter().enumerate() {
        devices.add_child(panel_key(d.name, extras.device == i, cx, SimAction::Device(i)));
    }
    let orientation = keys()
        .with_child(panel_key("Portrait", !extras.landscape, cx, SimAction::Landscape(false)))
        .with_child(panel_key("Landscape", extras.landscape, cx, SimAction::Landscape(true)))
        .with_child(panel_key("Safe areas", extras.safe, cx, SimAction::Safe(!extras.safe)))
        .with_child(panel_key("EN", extras.lang == Lang::En, cx, SimAction::Lang(Lang::En)))
        .with_child(panel_key("DE", extras.lang == Lang::De, cx, SimAction::Lang(Lang::De)));
    let mut scenes = keys();
    for scene in [
        SessionScene::Unpaired,
        SessionScene::Searching,
        SessionScene::Found,
        SessionScene::Connected,
        SessionScene::Lost,
        SessionScene::NotFound,
        SessionScene::NoPermission,
        SessionScene::AddressChanged,
        SessionScene::UpdateCompanion,
        SessionScene::UpdateAzReview,
    ] {
        scenes.add_child(panel_key(scene.label(), false, cx, SimAction::Scene(scene)));
    }
    let mac_keys = keys()
        .with_child(panel_key("Mac confirms code", false, cx, SimAction::MacConfirms))
        .with_child(panel_key("Mac declines", false, cx, SimAction::MacDeclines))
        .with_child(panel_key("Pen walks", pen_walk, cx, SimAction::PenWalk(!pen_walk)))
        .with_child(panel_key("Region marked", region, cx, SimAction::Region(!region)))
        .with_child(panel_key("Page -", false, cx, SimAction::MacPage(-1)))
        .with_child(panel_key("Page +", false, cx, SimAction::MacPage(1)));
    let mut link = keys();
    for ms in [0_u64, 12, 80, 250] {
        link.add_child(panel_key(&format!("{} ms", 2 * ms), extras.latency == ms, cx, SimAction::Latency(ms)));
    }
    let mut log_box = column("width: 100%; gap: 2px; padding: 8px; background: #0e0e10; border-radius: 8px; font-family: system:monospace, monospace;");
    if log.is_empty() {
        log_box.add_child(text("nothing yet", "font-size: 11px; color: #6f6f78;"));
    }
    for (at, line) in log {
        log_box.add_child(text(&format!("{:>6.1}s {line}", at as f32 / 1000.0), "font-size: 11px; color: #b8b8c2;"));
    }
    column(&format!(
        "width: {}; height: {}; flex-shrink: 0; padding: 14px; gap: 8px; overflow-y: auto; background: #151517; border-left: 1px solid #2a2a2e; color: #d8d8de;",
        px(PANEL_W),
        px(height)
    ))
    .with_child(text("Simulated Mac", "font-size: 17px; font-weight: 700;"))
    .with_child(text(&format!("{name}  ·  AzReview is running"), "font-size: 12px; color: #8a8a94;"))
    .with_child(heading("PHONE"))
    .with_child(devices)
    .with_child(orientation)
    .with_child(heading("SESSION"))
    .with_child(scenes)
    .with_child(heading("THE MAC"))
    .with_child(mac_keys)
    .with_child(heading("LINK (ROUND TRIP)"))
    .with_child(link)
    .with_child(heading("MOUSE AS TOUCH"))
    .with_child(text(
        "Drag = pan. Trackpad pinch, or Ctrl + scroll = zoom around the pen. Two quick clicks = double tap (fit width). Right click = two-finger tap (reset). Hold a quick button = edit it.",
        "font-size: 12px; color: #a3a3ad; line-height: 1.35;",
    ))
    .with_child(heading("WHAT THE PHONE SENT"))
    .with_child(log_box)
}

/// The simulator's window: the phone in its bezel and the panel beside it (or the phone alone, for a screenshot).
#[must_use]
pub fn wrap(cx: &Cx, extras: &Extras, phone: Dom) -> Dom {
    let body = Dom::create_body().with_css("margin: 0px; padding: 0px; background: #0e0e10; font-family: system:ui, sans-serif;");
    if !extras.chrome {
        return body.with_child(phone);
    }
    let env = cx.env;
    let (window_w, window_h) = extras.window_size();
    let mut screen = div(&format!(
        "position: relative; width: {}; height: {}; border-radius: 44px; overflow: hidden; flex-shrink: 0;",
        px(env.width),
        px(env.height)
    ))
    .with_child(phone);
    for mark in system_marks(cx, extras) {
        screen.add_child(mark);
    }
    let bezel = div(&format!(
        "padding: {}; background: #050506; border-radius: {}; border: 1px solid #2a2a2e; flex-shrink: 0;",
        px(BEZEL),
        px(44.0 + BEZEL)
    ))
    .with_child(screen);
    let stage = row(&format!(
        "width: {}; height: {}; justify-content: center; flex-grow: 1; background: #0e0e10;",
        px(window_w - PANEL_W),
        px(window_h)
    ))
    .with_child(bezel);
    body.with_child(row(&format!("width: {}; height: {}; align-items: stretch;", px(window_w), px(window_h))).with_child(stage).with_child(panel(cx, extras, window_h)))
}

#[cfg(test)]
#[path = "mod_test.rs"]
mod mod_test;
