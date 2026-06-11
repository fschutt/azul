//! Minimal timer-driven animation demo — verifies the `Timer -> RefreshDom ->
//! relayout` loop produces visible motion with NO external input (unlike the
//! spirit level, which needs an accelerometer).
//!
//! On window-create a `Timer` is installed via `CallbackInfo::add_timer`. Each
//! tick bumps a frame counter and asks for a relayout; `layout` maps the
//! counter to a smooth horizontal ping-pong (a sliding bubble) and also prints
//! the live frame count. So BOTH the moving box and the incrementing number
//! demonstrate the animation loop is actually running.

use azul::prelude::*;
use azul::option::OptionRefAny;
use azul::task::TerminateTimer;

/// The only state an animation needs here: how many timer ticks have elapsed.
struct AnimState {
    frame: u64,
}

const ROOT: &str = "display: flex; flex-direction: column; height: 100%; \
    align-items: center; justify-content: center; background: #0e0e14; \
    font-family: sans-serif;";
const TITLE: &str = "color: #e6e6f0; font-size: 22px; margin-bottom: 6px;";
const SUBTITLE: &str = "color: #6a7080; font-size: 13px; margin-bottom: 28px;";
// The rail the bubble slides along.
const TRACK: &str = "width: 320px; height: 72px; border-radius: 12px; \
    border: 2px solid #2a2a3a; background: #16161f; display: flex; \
    align-items: center; overflow: hidden;";
const COUNTER: &str = "color: #4a90e2; font-size: 18px; margin-top: 26px;";

/// Max horizontal travel (track width − bubble width − borders).
const AMPLITUDE: f32 = 248.0;
/// Radians of phase advanced per tick — small enough to look smooth.
const SPEED: f32 = 0.05;

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let frame = data.downcast_ref::<AnimState>().map(|s| s.frame).unwrap_or(0);

    // Eased ping-pong 0 -> 1 -> 0 derived purely from the frame counter.
    let phase = (frame as f32) * SPEED;
    let t = 0.5 - 0.5 * phase.cos();
    let x = (t * AMPLITUDE) as i32;

    let bubble_css = format!(
        "width: 64px; height: 64px; border-radius: 32px; background: #39d98a; \
         box-shadow: 0px 2px 10px rgba(0,0,0,0.55); margin-left: {}px;",
        x
    );

    Dom::create_body().with_child(
        Dom::create_div()
            .with_css(ROOT)
            .with_child(Dom::create_text("Timer Animation").with_css(TITLE))
            .with_child(
                Dom::create_text("box slides via add_timer → RefreshDom").with_css(SUBTITLE),
            )
            .with_child(
                Dom::create_div()
                    .with_css(TRACK)
                    .with_child(Dom::create_div().with_css(bubble_css.as_str())),
            )
            .with_child(
                Dom::create_text(format!("frame {}", frame).as_str()).with_css(COUNTER),
            ),
    )
}

/// Timer tick: advance the frame counter and request a relayout. Returning
/// `Update::RefreshDom` every tick is what makes the box move.
extern "C" fn tick(mut data: RefAny, _info: TimerCallbackInfo) -> TimerCallbackReturn {
    if let Some(mut s) = data.downcast_mut::<AnimState>() {
        s.frame = s.frame.wrapping_add(1);
    }
    TimerCallbackReturn {
        should_terminate: TerminateTimer::Continue,
        should_update: Update::RefreshDom,
    }
}

/// Window-create: install the per-frame Timer that drives the animation.
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
    let data = RefAny::new(AnimState { frame: 0 });
    let config = AppConfig::create();
    let app = App::create(data, config);
    let mut window = WindowCreateOptions::create(layout);
    window.create_callback = Some(Callback::create(startup)).into();
    app.run(window);
}
