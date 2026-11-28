//! An idle window asks for no frame, and a window with a caret only for the frames of its blinks.
//!
//! The user's flicker and jitter audit (2026-11-28) looks for frames nobody asked for: a page nothing changes in
//! must not be redrawn at all, and a page with a caret in it redraws exactly where the caret blinks, once per
//! half period of its blink, not at the refresh rate. This is the same law as `idle_cpu.rs`, over ten seconds
//! of the window's own clock (a frame asked for by a timer that never ends adds up over that, one more than a
//! hundred milliseconds do), on the desktop host: what a window asks for through `process_timers_and_threads`
//! is what its shell turns into a redraw.

use super::*;

/// One period of the window's driver, in test-clock ms (the refresh of a 60 Hz panel).
const FRAME_MS: u64 = 16;

/// Ten seconds of driver periods.
const TEN_SECONDS: usize = 625;

/// `body(0) > div[contenteditable, autofocus](1) > "Dear"`: a page that opens with a caret.
extern "C" fn caret_page_layout(_data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    use azul_core::dom::AttributeType;
    Dom::create_body().with_child(
        Dom::create_div()
            .with_contenteditable(true)
            .with_attribute(AttributeType::Autofocus)
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                "Dear",
            )),
    )
}

/// The window's timers, by id.
fn timer_ids(window: &HeadlessWindow) -> Vec<usize> {
    window
        .common
        .layout_window
        .as_ref()
        .expect("layout window")
        .timers
        .keys()
        .map(|t| t.id)
        .collect()
}

/// The blink period of the caret, in ms.
fn blink_period_ms(window: &HeadlessWindow) -> u64 {
    let lw = window.common.layout_window.as_ref().expect("layout window");
    match lw.text_edit_manager.blink.blink_interval {
        azul_core::task::Duration::System(d) => d.secs * 1000 + u64::from(d.nanos / 1_000_000),
        azul_core::task::Duration::Tick(_) => panic!("the caret blinks by a system interval"),
    }
}

/// Run `periods` driver periods on the frozen test clock; how many of them asked for a frame.
fn frames_asked_for_over(window: &mut HeadlessWindow, periods: usize) -> usize {
    (0..periods)
        .filter(|_| {
            let _ = azul_core::task::advance_test_clock_ms(FRAME_MS);
            window.process_timers_and_threads()
        })
        .count()
}

/// A window with a page nothing changes in, past its first two layouts: ten seconds of driver periods ask for
/// no frame and register no timer.
///
/// Mutation: any timer left running after the first frames (a driver that never ends, a tween that is never
/// told it is done): the frames it asks for show here, one per period.
#[test]
fn a_static_page_asks_for_no_frame_in_ten_seconds() {
    azul_core::task::reset_test_clock();
    azul_core::task::freeze_test_clock();
    let state = Arc::new(RefCell::new(RefAny::new(UiState {
        label: "nothing moves here".to_string(),
    })));
    let mut window = make_harness_window(&state);
    window.regenerate_layout().expect("the first layout");
    window.regenerate_layout().expect("the second layout");
    let _ = window.common.take_regeneration();
    window.arm_animation_drivers_if_needed();

    let timers = timer_ids(&window);
    let asked = frames_asked_for_over(&mut window, TEN_SECONDS);
    azul_core::task::reset_test_clock();

    assert!(
        timers.is_empty(),
        "an idle window registers no timer, it registered {timers:x?}"
    );
    assert_eq!(
        asked, 0,
        "an idle window asked for {asked} frames in ten seconds"
    );
}

/// A window with a caret in it, past its first two layouts: the frames it asks for in ten seconds are the
/// caret's blinks, one per half period, and no others (not one per period of the driver, which would be 625).
///
/// Mutation: a blink timer that asks for a frame on every tick, or a caret tween that keeps ticking after the
/// caret is at rest: far more than the blinks.
#[test]
fn a_page_with_a_blinking_caret_asks_for_one_frame_per_blink_and_no_other() {
    azul_core::task::reset_test_clock();
    azul_core::task::freeze_test_clock();
    let state = Arc::new(RefCell::new(RefAny::new(())));
    let mut window = make_window_with(&state, caret_page_layout);
    window.regenerate_layout().expect("the first layout");
    window.regenerate_layout().expect("the second layout");
    let _ = window.common.take_regeneration();
    window.arm_animation_drivers_if_needed();

    let blinking = timer_ids(&window).contains(&azul_core::task::CURSOR_BLINK_TIMER_ID.id);
    let period = blink_period_ms(&window);
    let asked = frames_asked_for_over(&mut window, TEN_SECONDS);
    let timers_after = timer_ids(&window);
    azul_core::task::reset_test_clock();

    assert!(blinking, "harness: the caret of the page blinks");
    let blinks = (TEN_SECONDS as u64 * FRAME_MS / period) as usize;
    assert!(
        asked >= blinks.saturating_sub(1) && asked <= blinks + 1,
        "a caret that blinks every {period} ms asks for {blinks} frames in ten seconds (one more or less \
         where a period of the driver falls), it asked for {asked}; timers left: {timers_after:x?}"
    );
}
