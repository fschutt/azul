//! Whether a window that asks to be kept out of screen captures really is, on this machine: a window whose whole
//! face is one colour that is nowhere else on the desktop (magenta) floats over the others, a display capture of
//! the display it is on runs for ten seconds with the app's own windows IN the picture (so that what is tested
//! is the window's flag and not the capture's own-window filter), and every frame is looked at in memory for
//! that colour. Nothing is saved and nothing is sent anywhere; what it prints is one line, `RECPROBE
//! preview_visible=yes|no`, and the numbers behind it.
//!
//! It needs the Screen Recording permission (the first run asks for it: for a program started from a terminal
//! the system names the terminal), so it is run by hand, and twice: with the flag on (`--exclude on`, the
//! default: the window should be gone from the picture) and off (`--exclude off`: the control, the window must
//! be in the picture or the check could not have seen it).
//!
//! `recorder_probe [--exclude on|off] [--seconds N]`. stdout: `RECPROBE_START exclude=<on|off> display=<id>`,
//! `RECPROBE_NO_FRAMES <why>`, `RECPROBE preview_visible=<yes|no> frames=<n> with_patch=<n> exclude=<on|off>`.

use std::time::{Duration as StdDuration, Instant};

use azul::{
    callbacks::LayoutCallbackType,
    image::RawImageFormat,
    prelude::*,
    screen::{ScreenCaptureConfig, ScreenCaptureSource},
    str::String as AzString,
    task::TerminateTimer,
    time::SystemTimeDiff,
    widgets::{ConsumerFrame, FrameConsumer, ScreenCaptureWidget},
};

use crate::{
    ids,
    recorder_host::consumer_size,
    recorder_patch::{patch_pixels, window_seen, SEEN_PIXELS},
};

/// The size of the picture the check looks at (and the id of its consumer).
const LOOK_WIDTH: u32 = 960;
const PROBE_CONSUMER: u32 = 21;

/// How long after the window is up the first picture may take (the system's question about the permission is
/// answered by a person), seconds, besides the length of the check.
const GRACE_SECONDS: u64 = 60;

/// What the check keeps.
pub struct Probe {
    exclude: bool,
    seconds: u64,
    display: u32,
    size: (u32, u32),
    frames: u32,
    with_patch: u32,
    /// When the window was up.
    armed: Option<Instant>,
    /// When the first picture came.
    started: Option<Instant>,
    done: bool,
}

fn with_probe(data: &mut RefAny, f: &mut dyn FnMut(&mut Probe)) {
    if let Some(mut p) = data.downcast_mut::<Probe>() {
        f(&mut p);
    }
}

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let mut capture = None;
    with_probe(&mut data, &mut |p| capture = Some((p.display, p.size)));
    let mut body = Dom::create_body().with_css("margin: 0px; background: #ff00ff;");
    if let Some((display, (w, h))) = capture {
        if w > 0 {
            body.add_child(
                ScreenCaptureWidget::create(ScreenCaptureConfig {
                    source: ScreenCaptureSource::DisplayId(display),
                    fps: 10,
                    output_format: RawImageFormat::RGBA8,
                    show_cursor: false,
                    include_own_windows: true,
                })
                .with_consumer(FrameConsumer::create(PROBE_CONSUMER, w, h))
                .with_on_consumer_frame(data.clone(), on_frame)
                .dom()
                .with_id(ids::REC_SCREEN)
                .with_css("width: 1px; height: 1px; overflow: hidden;"),
            );
        }
    }
    body
}

extern "C" fn on_frame(mut data: RefAny, _info: CallbackInfo, frame: ConsumerFrame) -> Update {
    if frame.consumer.id != PROBE_CONSUMER || !matches!(frame.frame.format, RawImageFormat::RGBA8) {
        return Update::DoNothing;
    }
    let found = patch_pixels(frame.frame.bytes.as_ref());
    with_probe(&mut data, &mut |p| {
        p.started.get_or_insert_with(Instant::now);
        p.frames += 1;
        if found >= SEEN_PIXELS {
            p.with_patch += 1;
        }
    });
    Update::DoNothing
}

extern "C" fn on_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let mut line = None;
    with_probe(&mut data, &mut |p| {
        let Some(armed) = p.armed else { return };
        let seen_for = p.started.map(|s| s.elapsed());
        let gave_up = seen_for.is_none() && armed.elapsed() >= StdDuration::from_secs(p.seconds + GRACE_SECONDS);
        let enough = seen_for.is_some_and(|s| s >= StdDuration::from_secs(p.seconds));
        if enough || gave_up {
            p.done = true;
            line = Some(if p.frames == 0 {
                String::from("RECPROBE_NO_FRAMES the capture gave no picture: is Screen Recording allowed for this program?")
            } else {
                format!("RECPROBE preview_visible={} frames={} with_patch={} exclude={}", if window_seen(p.frames, p.with_patch) { "yes" } else { "no" }, p.frames, p.with_patch, if p.exclude { "on" } else { "off" })
            });
        }
    });
    match line {
        Some(line) => {
            println!("{line}");
            info.callback_info.close_window();
            TimerCallbackReturn { should_update: Update::DoNothing, should_terminate: TerminateTimer::Terminate }
        }
        None => TimerCallbackReturn { should_update: Update::DoNothing, should_terminate: TerminateTimer::Continue },
    }
}

extern "C" fn on_created(mut data: RefAny, mut info: CallbackInfo) -> Update {
    // The display the window is on and its size in pixels, as the capture will name them.
    let monitor = info.get_current_monitor().into_option().or_else(|| info.get_monitors().as_ref().iter().find(|m| m.is_primary_monitor).cloned());
    if let Some(m) = monitor {
        let scale = m.scale_factor.max(1.0);
        let (w, h) = consumer_size((m.size.width as f64 * scale).round() as u32, (m.size.height as f64 * scale).round() as u32, LOOK_WIDTH);
        with_probe(&mut data, &mut |p| {
            p.display = m.native_id;
            p.size = (w, h);
        });
        let (display, exclude) = {
            let mut out = (0, false);
            with_probe(&mut data, &mut |p| out = (p.display, p.exclude));
            out
        };
        println!("RECPROBE_START exclude={} display={display}", if exclude { "on" } else { "off" });
    }
    with_probe(&mut data, &mut |p| p.armed = Some(Instant::now()));
    let timer = Timer::create(data.clone(), on_tick, info.get_system_time_fn()).with_interval(Duration::System(SystemTimeDiff::from_millis(250)));
    info.add_timer(TimerId::unique(), timer);
    Update::RefreshDom
}

/// Starts the probe window.
pub fn run() {
    let mut exclude = true;
    let mut seconds = 10_u64;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--exclude" => exclude = args.next().as_deref() != Some("off"),
            "--seconds" => seconds = args.next().and_then(|n| n.parse().ok()).unwrap_or(10).clamp(2, 60),
            other => {
                eprintln!("recorder_probe [--exclude on|off] [--seconds N]: not understood: {other}");
                std::process::exit(2);
            }
        }
    }
    let probe = Probe { exclude, seconds, display: 0, size: (0, 0), frames: 0, with_patch: 0, armed: None, started: None, done: false };
    let app = App::create(RefAny::new(probe), AppConfig::create());
    let layout: LayoutCallbackType = layout;
    let mut window = WindowCreateOptions::create(layout);
    window.window_state.title = AzString::from("recorder probe");
    window.window_state.size.dimensions = LogicalSize::create(300.0, 200.0);
    window.window_state.flags.is_top_level = true;
    window.window_state.flags.exclude_from_capture = exclude;
    window.create_callback = Some(Callback::create(on_created)).into();
    app.run(window);
}
