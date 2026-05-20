//! AzGamepad — a live game-controller readout, the P6 demo for gamepad input.
//!
//! Same shape as azul-spirit-level: a `create_callback` installs a Timer
//! that reads `get_primary_gamepad()` through the `TimerCallbackInfo`'s
//! wrapped `CallbackInfo` and stores the snapshot; `layout` renders it —
//! button chips light green when pressed, each stick drives a dot
//! (`transform: translate`), each trigger drives a bar. The Timer always
//! asks for a relayout so the dll's per-frame `gamepad::poll` keeps running
//! (that's what refreshes the state + detects hot-plug).
//!
//! The gilrs desktop backend runs on the host, so this is testable on the
//! dev box: plug in a controller and the panel goes live. With none
//! connected it shows a "connect a controller" prompt. Pure public `azul::`
//! surface (P6.gamepad end-to-end).

use azul::prelude::*;
use azul::misc::GamepadState;
use azul::option::OptionRefAny;
use azul::task::TerminateTimer;
use azul::widgets::GamepadButton;

/// The latest primary-controller snapshot, refreshed each Timer tick.
struct PadState {
    pad: Option<GamepadState>,
}

impl PadState {
    fn new() -> Self {
        Self { pad: None }
    }
}

// Button chips grouped into rows (avoids relying on flex-wrap).
const FACE: [(&str, GamepadButton); 4] = [
    ("A", GamepadButton::South),
    ("B", GamepadButton::East),
    ("X", GamepadButton::West),
    ("Y", GamepadButton::North),
];
const SHOULDER: [(&str, GamepadButton); 4] = [
    ("L1", GamepadButton::LeftBumper),
    ("R1", GamepadButton::RightBumper),
    ("L2", GamepadButton::LeftTrigger),
    ("R2", GamepadButton::RightTrigger),
];
const DPAD: [(&str, GamepadButton); 4] = [
    ("↑", GamepadButton::DPadUp),
    ("↓", GamepadButton::DPadDown),
    ("←", GamepadButton::DPadLeft),
    ("→", GamepadButton::DPadRight),
];
const CENTER: [(&str, GamepadButton); 5] = [
    ("Sel", GamepadButton::Select),
    ("Start", GamepadButton::Start),
    ("Mode", GamepadButton::Mode),
    ("L3", GamepadButton::LeftThumb),
    ("R3", GamepadButton::RightThumb),
];

const ROOT: &str = "display: flex; flex-direction: column; height: 100%; \
    align-items: center; justify-content: center; background: #0e0e14; \
    font-family: sans-serif;";
const TITLE: &str = "color: #e6e6f0; font-size: 24px; margin-bottom: 4px;";
const SUBTITLE: &str = "color: #9aa0b4; font-size: 14px; margin-bottom: 18px;";
const WAITING: &str = "color: #6a7080; font-size: 16px; margin-top: 20px;";
const PANEL: &str = "display: flex; flex-direction: column; align-items: center;";
const ROW: &str = "display: flex; flex-direction: row; margin: 4px 0px;";
const CHIP_OFF: &str = "min-width: 30px; padding: 8px 12px; margin: 0px 4px; \
    background: #20202e; color: #6a7080; border-radius: 6px; font-size: 14px; \
    text-align: center;";
const CHIP_ON: &str = "min-width: 30px; padding: 8px 12px; margin: 0px 4px; \
    background: #39d98a; color: #06200f; border-radius: 6px; font-size: 14px; \
    text-align: center;";
const STICKS_ROW: &str = "display: flex; flex-direction: row; margin-top: 14px;";
const STICK_AREA: &str = "width: 72px; height: 72px; border-radius: 36px; \
    border: 2px solid #2a2a3a; background: #16161f; margin: 0px 12px; \
    display: flex; align-items: center; justify-content: center; \
    overflow: visible;";
const TRIG_TRACK: &str = "width: 124px; height: 16px; border-radius: 8px; \
    background: #16161f; border: 1px solid #2a2a3a; margin: 18px 12px 0px 12px; \
    display: flex; flex-direction: row; align-items: center; overflow: hidden;";

fn chip(label: &str, pressed: bool) -> Dom {
    Dom::create_div()
        .with_css(if pressed { CHIP_ON } else { CHIP_OFF })
        .with_child(Dom::create_text(label))
}

fn button_row(pad: &GamepadState, group: &[(&str, GamepadButton)]) -> Dom {
    let mut row = Dom::create_div().with_css(ROW);
    for (label, btn) in group {
        row = row.with_child(chip(label, pad.is_pressed(*btn)));
    }
    row
}

fn stick(x: f32, y: f32) -> Dom {
    // Up on the stick is +Y, but screen Y grows down → negate so the dot
    // rises when you push up. Clamp to the ring radius (≈ area/2 − dot/2).
    let dot = format!(
        "width: 16px; height: 16px; border-radius: 8px; background: #4a90e2; \
         transform: translate({:.1}px, {:.1}px);",
        (x * 24.0).clamp(-24.0, 24.0),
        (-y * 24.0).clamp(-24.0, 24.0),
    );
    Dom::create_div()
        .with_css(STICK_AREA)
        .with_child(Dom::create_div().with_css(dot.as_str()))
}

fn trigger_bar(value: f32) -> Dom {
    let fill = format!(
        "width: {:.1}px; height: 100%; background: #e74c3c;",
        value.clamp(0.0, 1.0) * 124.0,
    );
    Dom::create_div()
        .with_css(TRIG_TRACK)
        .with_child(Dom::create_div().with_css(fill.as_str()))
}

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let pad = data.downcast_ref::<PadState>().and_then(|s| s.pad);

    let body = match pad {
        None => Dom::create_div()
            .with_css(WAITING)
            .with_child(Dom::create_text("No controller connected — plug one in.")),
        Some(p) => Dom::create_div()
            .with_css(PANEL)
            .with_child(
                Dom::create_text(format!("Controller #{}", p.id.id).as_str()).with_css(SUBTITLE),
            )
            .with_child(button_row(&p, &FACE))
            .with_child(button_row(&p, &SHOULDER))
            .with_child(button_row(&p, &DPAD))
            .with_child(button_row(&p, &CENTER))
            .with_child(
                Dom::create_div()
                    .with_css(STICKS_ROW)
                    .with_child(stick(p.left_stick_x, p.left_stick_y))
                    .with_child(stick(p.right_stick_x, p.right_stick_y)),
            )
            .with_child(
                Dom::create_div()
                    .with_css(STICKS_ROW)
                    .with_child(trigger_bar(p.left_z))
                    .with_child(trigger_bar(p.right_z)),
            ),
    };

    Dom::create_body().with_child(
        Dom::create_div()
            .with_css(ROOT)
            .with_child(Dom::create_text("🎮 Gamepad").with_css(TITLE))
            .with_child(body),
    )
}

/// Timer tick: snapshot the primary controller, then always relayout so the
/// dll's per-frame `gamepad::poll` keeps refreshing the state (and picks up
/// hot-plugged controllers).
extern "C" fn tick(mut data: RefAny, info: TimerCallbackInfo) -> TimerCallbackReturn {
    let pad = info.callback_info.get_primary_gamepad().into_option();
    if let Some(mut s) = data.downcast_mut::<PadState>() {
        s.pad = pad;
    }
    TimerCallbackReturn {
        should_terminate: TerminateTimer::Continue,
        should_update: Update::RefreshDom,
    }
}

/// Window-create callback: install the per-frame Timer that polls the pad.
extern "C" fn startup(data: RefAny, mut info: CallbackInfo) -> Update {
    info.add_timer(
        TimerId::unique(),
        Timer::create(
            data.clone(),
            TimerCallback {
                cb: tick,
                ctx: OptionRefAny::None,
            },
            info.get_system_time_fn(),
        ),
    );
    Update::DoNothing
}

fn main() {
    let data = RefAny::new(PadState::new());
    let config = AppConfig::create();
    let app = App::create(data, config);
    let mut window = WindowCreateOptions::create(layout);
    window.create_callback = Some(Callback::create(startup)).into();
    app.run(window);
}
