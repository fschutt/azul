//! The renderer: which backend ran, on which driver, and how long a frame takes.
//!
//! The row draws about three seconds of a per-frame animation (the runner ticks at 16 ms and refreshes the DOM), so that frames
//! come one after another, then reads the engine's own lines about its renderer (`[render] backend=...`, at `AZ_LOG=info`; on
//! Android an app may read its OWN lines of logcat), and adds what the app measured itself (`get_frame_timings`). PASS when a
//! backend line exists (renderer, what was asked for, driver, frame times in the text); a GPU that was asked for and a CPU that
//! ran is a FAIL with the engine's reason; no line at all says how to switch the logging on. The same row answers the user's
//! question on a phone with a real GPU: `adb shell setprop debug.az.env 'AZ_LOG=info;AZ_BACKEND=gpu'`, a cold start, this row.

use azul::dom::Dom;

use super::{Ctx, ProbeDef, Step};
use crate::{
    render_lines::{self, Judged},
    report::Verdict,
};

/// How long the animation runs before the lines are read.
const ANIMATE_MS: u64 = 3_000;

pub static PROBES: &[ProbeDef] = &[ProbeDef {
    id: "render.renderer",
    group: "render",
    title: "Renderer: backend, driver, ms per frame",
    human: None,
    note: "",
    widget: Some(animation),
    step: renderer,
}];

/// A bar that moves every frame: the engine has something new to draw on every tick.
fn animation(elapsed_ms: u64) -> Dom {
    let width = 40 + (elapsed_ms / 6) % 280;
    Dom::create_div().with_css(
        format!("height: 14px; width: {width}px; margin: 6px 12px; background-color: #2563eb; border-radius: 7px;").as_str(),
    )
}

fn renderer(ctx: &mut Ctx<'_>) -> Step {
    ctx.want_widget = true;
    ctx.want_refresh = true;
    let timings = ctx.info.get_frame_timings();
    if timings.frame_interval_us > 0 {
        ctx.scratch.push(timings.frame_interval_us);
    }
    if ctx.elapsed_ms < ANIMATE_MS {
        return Step::Wait;
    }
    ctx.want_widget = false;
    ctx.want_refresh = false;
    let mut app_side = match render_lines::spread_ms(ctx.scratch) {
        Some((median, p95, max)) => format!(
            "app side: frame interval median {median:.1} / p95 {p95:.1} / max {max:.1} ms over {} frames, last frame work {:.1} ms",
            ctx.scratch.len(),
            timings.frame_work_us as f64 / 1000.0
        ),
        None => "app side: no frame timing".to_string(),
    };
    if timings.gpu_draw_calls > 0 {
        app_side.push_str(&format!(", {} GPU draw calls in the last frame", timings.gpu_draw_calls));
    }
    let Some(lines) = own_render_lines() else {
        return Step::Done(Verdict::Unavailable(format!(
            "the engine's [render] lines are read from the Android log; on this platform read them from the console. {app_side}"
        )));
    };
    Step::Done(match render_lines::judge(&lines, &app_side) {
        Judged::Pass(text) => Verdict::Pass(text),
        Judged::Fail(text) => Verdict::Fail(text),
        Judged::NoLines => Verdict::NeedsHuman(format!(
            "the engine logs its renderer at AZ_LOG=info: adb shell setprop debug.az.env 'AZ_LOG=info' (add ;AZ_BACKEND=gpu to ask for the GPU), start the app cold, run this row again. {app_side}"
        )),
    })
}

/// The `[render]` lines of this process's own log, in order (Android: `logcat --pid`, an app may read its own lines).
#[cfg(target_os = "android")]
fn own_render_lines() -> Option<Vec<String>> {
    let out = std::process::Command::new("logcat")
        .args(["-d", "-v", "raw", &format!("--pid={}", std::process::id()), "-s", "RustStdoutStderr:I"])
        .output()
        .ok()?;
    Some(
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter(|line| line.contains("[render]"))
            .map(str::to_string)
            .collect(),
    )
}

#[cfg(not(target_os = "android"))]
fn own_render_lines() -> Option<Vec<String>> {
    None
}
