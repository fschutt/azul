//! AzSpirit — a spirit level (Wasserwaage), the P6 demo for motion sensors.
//!
//! On window create, a Timer is installed (`create_callback` →
//! `CallbackInfo::add_timer`). Each tick the Timer reads the accelerometer
//! through the `TimerCallbackInfo`'s wrapped `CallbackInfo`
//! (`get_sensor_reading(Accelerometer)`), low-pass-smooths the gravity
//! vector to tame jitter, and asks for a relayout. `layout` turns the
//! smoothed `(x, y)` gravity into a bubble offset (`transform: translate`)
//! inside a bullseye and reads off the tilt angle from horizontal — green
//! when level, like iOS's Level.
//!
//! The whole thing rides the public `azul::` api.json surface and the
//! P6.sensors pipeline (iOS CoreMotion / Android `SensorManager`). On a
//! device without an accelerometer (e.g. a desktop dev box) no reading ever
//! arrives, so the UI sits in a graceful "waiting" state.

use azul::prelude::*;
use azul::sensor::SensorKind;
use azul::option::OptionRefAny;
use azul::task::TerminateTimer;

/// Smoothed gravity vector (m/s²) — the only state the level needs.
struct LevelState {
    ax: f32,
    ay: f32,
    az: f32,
    /// `false` until the first accelerometer sample arrives (no backend /
    /// no hardware keeps it `false`, which drives the "waiting" UI).
    has_reading: bool,
}

impl LevelState {
    fn new() -> Self {
        Self {
            ax: 0.0,
            ay: 0.0,
            az: 0.0,
            has_reading: false,
        }
    }
}

/// Standard gravity (m/s²) — the magnitude of a resting accelerometer, used
/// to normalise the tilt into a bubble offset.
const G: f32 = 9.806_65;
/// Max bubble offset from centre (px). Kept inside the outer ring radius
/// (140) minus the bubble radius (32) so the bubble never fully escapes.
const MAX_OFFSET: f32 = 100.0;
/// Below this tilt (degrees) we call it level and turn the bubble green.
const LEVEL_DEG: f32 = 0.8;

const ROOT: &str = "display: flex; flex-direction: column; height: 100%; \
    align-items: center; justify-content: center; background: #0e0e14; \
    font-family: sans-serif;";
const TITLE: &str = "color: #e6e6f0; font-size: 22px; margin-bottom: 6px;";
const SUBTITLE: &str = "color: #6a7080; font-size: 13px; margin-bottom: 28px;";
// Outer reference ring (the level body).
const LEVEL_AREA: &str = "width: 280px; height: 280px; border-radius: 140px; \
    border: 2px solid #2a2a3a; background: #16161f; display: flex; \
    align-items: center; justify-content: center; overflow: visible;";
// Centre target zone — the bubble sits here when level.
const TARGET: &str = "width: 84px; height: 84px; border-radius: 42px; \
    border: 2px solid #3a4a6a; display: flex; align-items: center; \
    justify-content: center; overflow: visible;";
const READOUT: &str = "color: #ffffff; font-size: 40px; margin-top: 30px;";
const STATUS_LEVEL: &str = "color: #39d98a; font-size: 16px; margin-top: 4px; \
    letter-spacing: 2px;";
const STATUS_TILT: &str = "color: #6a7080; font-size: 16px; margin-top: 4px;";

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let (has, ax, ay, az) = match data.downcast_ref::<LevelState>() {
        Some(s) => (s.has_reading, s.ax, s.ay, s.az),
        None => (false, 0.0, 0.0, 0.0),
    };

    // Tilt from horizontal: 0° flat (gravity straight down → az dominates),
    // 90° on edge (gravity in the x/y plane).
    let horiz = (ax * ax + ay * ay).sqrt();
    let angle = horiz.atan2(az.abs()).to_degrees();
    let level = has && angle < LEVEL_DEG;

    // Bubble offset ∝ the horizontal gravity component; centred when flat.
    // (Per-axis sign conventions differ iOS vs Android — centring + the
    // angle readout are convention-independent; axis-sign calibration is a
    // device-side refinement.)
    let (ox, oy) = if has {
        (
            (ax / G * MAX_OFFSET).clamp(-MAX_OFFSET, MAX_OFFSET),
            (ay / G * MAX_OFFSET).clamp(-MAX_OFFSET, MAX_OFFSET),
        )
    } else {
        (0.0, 0.0)
    };

    let bubble_color = if level {
        "#39d98a"
    } else if has {
        "#4a90e2"
    } else {
        "#3a3a4a"
    };
    let bubble_css = format!(
        "width: 64px; height: 64px; border-radius: 32px; background: {}; \
         box-shadow: 0px 2px 10px rgba(0,0,0,0.55); \
         transform: translate({}px, {}px);",
        bubble_color, ox, oy
    );

    let readout = if has {
        format!("{:.1}°", angle)
    } else {
        "—".to_string()
    };
    let (status, status_css) = if !has {
        ("Waiting for accelerometer…", STATUS_TILT)
    } else if level {
        ("LEVEL", STATUS_LEVEL)
    } else {
        ("Tilt to level", STATUS_TILT)
    };

    Dom::create_body().with_child(
        Dom::create_div()
            .with_css(ROOT)
            .with_child(Dom::create_text("Spirit Level").with_css(TITLE))
            .with_child(Dom::create_text("Wasserwaage · accelerometer").with_css(SUBTITLE))
            .with_child(
                Dom::create_div().with_css(LEVEL_AREA).with_child(
                    Dom::create_div()
                        .with_css(TARGET)
                        .with_child(Dom::create_div().with_css(bubble_css.as_str())),
                ),
            )
            .with_child(Dom::create_text(readout.as_str()).with_css(READOUT))
            .with_child(Dom::create_text(status).with_css(status_css)),
    )
}

/// Timer tick: pull the latest accelerometer sample through the wrapped
/// `CallbackInfo` and fold it into the smoothed state, then relayout.
extern "C" fn tick(mut data: RefAny, info: TimerCallbackInfo) -> TimerCallbackReturn {
    let should_update = match info
        .callback_info
        .get_sensor_reading(SensorKind::Accelerometer)
        .into_option()
    {
        Some(r) => {
            if let Some(mut s) = data.downcast_mut::<LevelState>() {
                if s.has_reading {
                    // Exponential low-pass — the bubble glides instead of
                    // twitching on raw sensor noise.
                    s.ax = s.ax * 0.85 + r.x * 0.15;
                    s.ay = s.ay * 0.85 + r.y * 0.15;
                    s.az = s.az * 0.85 + r.z * 0.15;
                } else {
                    s.ax = r.x;
                    s.ay = r.y;
                    s.az = r.z;
                    s.has_reading = true;
                }
            }
            Update::RefreshDom
        }
        None => Update::DoNothing,
    };
    TimerCallbackReturn {
        should_terminate: TerminateTimer::Continue,
        should_update,
    }
}

/// Window-create callback: install the per-frame Timer that drives the
/// sensor poll + relayout. (Sensors are read from a `CallbackInfo`, not the
/// layout callback, so the Timer is what makes the level *live*.)
extern "C" fn startup(data: RefAny, mut info: CallbackInfo) -> Update {
    info.add_timer(
        TimerId::unique(),
        Timer::create(
            data.clone(),
            // AzTimerCallback is a `{ cb, ctx }` struct (no `create` ctor,
            // unlike the regular Callback); the data rides in via the
            // refany arg above, so the callback's own ctx is None.
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
    let data = RefAny::new(LevelState::new());
    let config = AppConfig::create();
    let app = App::create(data, config);
    let mut window = WindowCreateOptions::create(layout);
    window.create_callback = Some(Callback::create(startup)).into();
    app.run(window);
}
