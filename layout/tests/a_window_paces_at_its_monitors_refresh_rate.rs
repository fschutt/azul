//! USER RULING 2026-09-30: "audit for '16ms' or '60fps', we need to make this
//! configurable and wire in the monitor, so that we're ready for 120fps
//! mobile".
//!
//! A window's frame interval has ONE source: the refresh rate of the
//! monitor the window is on (`LayoutWindow::frame_interval_nanos`), slowed to
//! `RendererOptions::max_frame_rate` when that is lower. Every frame-paced
//! driver reads it: the CSS animation driver, the caret tween, the first
//! step of the animation clock.

use azul_core::window::{frame_interval_nanos, Monitor, MonitorId, MonitorVec, VideoMode};
use azul_css::{props::basic::LayoutSize, OptionU32};
use azul_layout::window::LayoutWindow;
use rust_fontconfig::FcFontCache;

fn monitor(index: usize, hz: u16, primary: bool) -> Monitor {
    Monitor {
        monitor_id: MonitorId {
            index,
            hash: index as u64 + 1,
        },
        video_modes: vec![VideoMode {
            size: LayoutSize::new(1920, 1080),
            bit_depth: 32,
            refresh_rate: hz,
        }]
        .into(),
        is_primary_monitor: primary,
        ..Monitor::default()
    }
}

/// A window on monitor `on` of `monitors`.
fn window_on(monitors: Vec<Monitor>, on: u32) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("layout window");
    *lw.monitors.lock().expect("monitor list") = MonitorVec::from_vec(monitors);
    lw.current_window_state.monitor_id = OptionU32::Some(on);
    lw
}

fn ns_of(d: azul_core::task::Duration) -> u128 {
    d.as_nanos()
}

fn near(ns: u128, want_ns: u128) -> bool {
    ns.abs_diff(want_ns) <= 1_000
}

#[test]
fn a_window_on_a_120_hz_monitor_paces_at_8_33_ms() {
    let lw = window_on(vec![monitor(0, 120, true)], 0);

    assert!(
        near(u128::from(lw.frame_interval_nanos()), 8_333_333),
        "a 120 Hz monitor's frame is 8.33 ms, the window reports {} ns",
        lw.frame_interval_nanos()
    );
    let tween = lw
        .create_caret_tween_timer()
        .interval
        .into_option()
        .map(ns_of);
    assert!(
        tween.is_some_and(|ns| near(ns, 8_333_333)),
        "the caret tween steps once per frame: {tween:?} ns"
    );
    let driver = lw
        .create_css_animation_timer()
        .interval
        .into_option()
        .map(ns_of);
    assert!(
        driver.is_some_and(|ns| near(ns, 8_333_333)),
        "the CSS animation driver fires once per frame: {driver:?} ns"
    );
    let first_step = lw.animation_step_at(&azul_core::task::Instant::now());
    assert!(
        (first_step - 1.0 / 120.0).abs() < 1e-5,
        "the first animation step after an idle period is one 120 Hz frame, not {first_step}"
    );
}

#[test]
fn the_window_paces_at_the_monitor_it_is_on_not_the_primary() {
    let lw = window_on(vec![monitor(0, 60, true), monitor(1, 144, false)], 1);
    assert!(
        near(u128::from(lw.frame_interval_nanos()), 6_944_444),
        "on the 144 Hz secondary monitor a frame is 6.94 ms, the window reports {} ns",
        lw.frame_interval_nanos()
    );
}

#[test]
fn a_max_frame_rate_of_30_paces_at_33_ms() {
    let mut lw = window_on(vec![monitor(0, 120, true)], 0);
    lw.current_window_state.renderer_options.max_frame_rate = OptionU32::Some(30);
    assert!(
        near(u128::from(lw.frame_interval_nanos()), 33_333_333),
        "a 30 fps cap paces at 33.3 ms on any monitor, the window reports {} ns",
        lw.frame_interval_nanos()
    );
    // A cap ABOVE the monitor's rate changes nothing.
    lw.current_window_state.renderer_options.max_frame_rate = OptionU32::Some(240);
    assert!(near(u128::from(lw.frame_interval_nanos()), 8_333_333));
}

#[test]
fn a_monitor_that_reports_no_rate_paces_at_60_hz() {
    let lw = window_on(vec![monitor(0, 0, true)], 0);
    assert!(near(u128::from(lw.frame_interval_nanos()), 16_666_666));
    let no_monitor = LayoutWindow::new(FcFontCache::default()).expect("layout window");
    assert!(near(
        u128::from(no_monitor.frame_interval_nanos()),
        16_666_666
    ));
}

#[test]
fn the_frame_interval_formula() {
    assert!(near(
        u128::from(frame_interval_nanos(Some(60), None)),
        16_666_666
    ));
    assert!(near(
        u128::from(frame_interval_nanos(Some(120), None)),
        8_333_333
    ));
    assert!(near(
        u128::from(frame_interval_nanos(None, None)),
        16_666_666
    ));
    assert!(near(
        u128::from(frame_interval_nanos(Some(120), Some(30))),
        33_333_333
    ));
    assert!(near(
        u128::from(frame_interval_nanos(Some(60), Some(120))),
        16_666_666
    ));
    // Implausible readings fall back rather than stall or spin a pacer.
    assert!(near(
        u128::from(frame_interval_nanos(Some(0), None)),
        16_666_666
    ));
    assert!(near(
        u128::from(frame_interval_nanos(Some(100_000), None)),
        16_666_666
    ));
}
