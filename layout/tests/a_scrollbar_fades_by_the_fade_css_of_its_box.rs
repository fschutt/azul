//! A scroll box fades its overlay scrollbar by the `-azul-scrollbar-fade-delay`
//! and `-azul-scrollbar-fade-duration` it declares (E2E-R9).
//!
//! Both properties were parsed, cascaded into the property cache and read by
//! `get_scrollbar_style` into `ComputedScrollbarStyle::fade_delay_ms` - which no
//! reader used. Every bar faded by the window's constants (500 ms hold, 200 ms
//! fade), so `-azul-scrollbar-fade-delay: 3000ms` and `: 50ms` fell at the same
//! moment (`gen-e2e` batch 1: the failing frame did not move between the two).
//!
//! The window's fade here is 500 / 200 TICKS (8.3 s / 3.3 s), far from what the
//! boxes declare in milliseconds (a tick is 16.67 ms, so 600 ms are 36 ticks), so
//! a bar that falls on the declared moment cannot be mistaken for one that falls
//! on the window's.

use std::cell::Cell;

use azul_core::{
    dom::{Dom, DomId, IdOrClass, NodeId},
    events::EasingFunction,
    geom::{LogicalPosition, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
    task::{Duration, GetSystemTimeCallback, Instant, SystemTick, SystemTickDiff},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const DOM: DomId = DomId::ROOT_ID;
/// Enough lines that the box overflows and shows a scrollbar.
const LINES: usize = 40;

/// The window's fade, in ticks: what a box that declares nothing falls by.
const WINDOW_DELAY: u64 = 500;
const WINDOW_DURATION: u64 = 200;

/// A scroll box whose rule carries `fade_css` (declarations, may be empty).
fn scroller(fade_css: &str) -> LayoutWindow {
    let css_src = format!(
        "* {{ margin: 0; padding: 0; }} body {{ font-size: 14px; width: 600px; }} .box {{ \
         display: block; width: 600px; height: 100px; overflow-y: scroll; {fade_css} }} .line {{ \
         display: block; }}"
    );
    let class: azul_core::dom::IdOrClassVec = vec![IdOrClass::Class("box".into())].into();
    let mut box_dom = Dom::create_div().with_ids_and_classes(class);
    for i in 0..LINES {
        let line_class: azul_core::dom::IdOrClassVec = vec![IdOrClass::Class("line".into())].into();
        box_dom = box_dom.with_child(
            Dom::create_div()
                .with_ids_and_classes(line_class)
                .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                    format!("line {i} with enough words to hit"),
                )),
        );
    }
    let mut dom = Dom::create_body().with_child(box_dom);
    let (css, _) = azul_css::parser2::new_from_str(&css_src);
    let styled_dom = StyledDom::create(&mut dom, css);

    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = window_state.clone();
    lw.layout_and_generate_display_list(
        styled_dom,
        &window_state,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut Some(Vec::new()),
    )
    .unwrap();
    lw
}

/// The scroll box is body's first child.
fn scroll_box() -> NodeId {
    NodeId::new(1)
}

// The sync reads "now" through the system callbacks, so the test owns that
// clock - one per thread, the tests of a folded binary run side by side.
thread_local! {
    static CLOCK: Cell<u64> = const { Cell::new(0) };
}

extern "C" fn tick_clock() -> Instant {
    Instant::Tick(SystemTick::new(CLOCK.with(Cell::get)))
}

fn at(t: u64) -> Instant {
    Instant::Tick(SystemTick::new(t))
}

fn ticks(d: u64) -> Duration {
    Duration::Tick(SystemTickDiff { tick_diff: d })
}

/// The per-frame opacity sync at tick `now`, and the vertical bar's opacity.
fn opacity_at(lw: &mut LayoutWindow, now: u64) -> f32 {
    CLOCK.with(|c| c.set(now));
    let cbs = ExternalSystemCallbacks {
        create_thread_fn: ExternalSystemCallbacks::rust_internal().create_thread_fn,
        get_system_time_fn: GetSystemTimeCallback { cb: tick_clock },
    };
    let LayoutWindow {
        ref layout_results,
        ref scroll_manager,
        ref mut gpu_state_manager,
        ..
    } = *lw;
    let result = &layout_results[&DOM];
    LayoutWindow::synchronize_scrollbar_opacity(
        gpu_state_manager,
        scroll_manager,
        DOM,
        &result.layout_tree,
        &result.styled_dom,
        &cbs,
        ticks(WINDOW_DELAY),
        ticks(WINDOW_DURATION),
    );
    gpu_state_manager.caches[&DOM].scrollbar_v_opacity_values[&(DOM, scroll_box())]
}

/// A one-off scroll at tick 0: the last activity the fade sees.
fn scroll_once_at_zero(lw: &mut LayoutWindow) {
    lw.scroll_manager.scroll_by(
        DOM,
        scroll_box(),
        LogicalPosition::new(0.0, 10.0),
        ticks(0),
        EasingFunction::Linear,
        at(0),
    );
}

/// 600 ms hold (36 ticks) and 300 ms fade (18 ticks): the box fades at
/// tick 36..54, the window at tick 500..700.
const BOTH: &str = "-azul-scrollbar-fade-delay: 600ms; -azul-scrollbar-fade-duration: 300ms;";

/// The control: a box that declares nothing falls by the window's fade, so the
/// tests below can tell the two apart (and the fixture can see a fade at all).
#[test]
fn e2e_r9_a_box_that_declares_nothing_fades_by_the_windows_fade() {
    let mut lw = scroller("");
    scroll_once_at_zero(&mut lw);
    assert_eq!(
        opacity_at(&mut lw, 45),
        1.0,
        "tick 45 is inside the window's 500-tick hold"
    );
    assert_eq!(opacity_at(&mut lw, 80), 1.0, "tick 80 too");
    assert_eq!(
        opacity_at(&mut lw, WINDOW_DELAY + WINDOW_DURATION + 1000),
        0.0,
        "long after the window's delay + duration the bar is gone"
    );
}

/// E2E-R9: the declared delay and duration are the box's fade. At tick 30
/// (500 ms) the bar is inside its 600 ms hold; at tick 45 (750 ms) it is half
/// way through the 300 ms fade; at tick 80 it is gone - all of it long before
/// the window's fade would start.
#[test]
fn e2e_r9_a_box_fades_by_its_declared_delay_and_duration() {
    let mut lw = scroller(BOTH);
    scroll_once_at_zero(&mut lw);
    assert_eq!(
        opacity_at(&mut lw, 30),
        1.0,
        "inside the declared 600 ms hold"
    );
    let mid = opacity_at(&mut lw, 45);
    assert!(
        (0.4..0.6).contains(&mid),
        "tick 45 is 150 ms into a 300 ms fade, half way: {mid} (the window's fade reads 1.0)"
    );
    assert_eq!(
        opacity_at(&mut lw, 80),
        0.0,
        "past 600 + 300 ms the bar is gone"
    );
}

/// E2E-R9: the two properties are independent. A box that sets only the delay
/// keeps the window's (3.3 s) duration: at tick 45 it has just begun to fade, and
/// at tick 250 (4.2 s, past 600 ms + 3.3 s) it is gone.
#[test]
fn e2e_r9_a_box_that_declares_only_the_delay_keeps_the_windows_duration() {
    let mut lw = scroller("-azul-scrollbar-fade-delay: 600ms;");
    scroll_once_at_zero(&mut lw);
    let early = opacity_at(&mut lw, 45);
    assert!(
        (0.9..0.99).contains(&early),
        "150 ms into the window's 3.3 s fade the bar is barely fainter: {early}"
    );
    assert_eq!(opacity_at(&mut lw, 250), 0.0, "past 600 ms + 3.3 s");
}

/// E2E-R9: a delay of 0 is the documented "never fades (always visible)" - the
/// bar is there after any amount of time, and before anything scrolled.
#[test]
fn e2e_r9_a_delay_of_zero_never_fades() {
    let mut lw = scroller("-azul-scrollbar-fade-delay: 0;");
    assert_eq!(
        opacity_at(&mut lw, 10),
        1.0,
        "visible before anything scrolled"
    );
    scroll_once_at_zero(&mut lw);
    assert_eq!(
        opacity_at(&mut lw, 100_000),
        1.0,
        "and an age after the last scroll"
    );
}

/// E2E-R9: a duration of 0 is "instant disappearance": the bar is opaque for the
/// delay and gone the moment it is up - no fade, and no NaN from the ratio.
#[test]
fn e2e_r9_a_duration_of_zero_ends_the_bar_at_once() {
    let mut lw = scroller("-azul-scrollbar-fade-delay: 600ms; -azul-scrollbar-fade-duration: 0;");
    scroll_once_at_zero(&mut lw);
    assert_eq!(opacity_at(&mut lw, 30), 1.0, "inside the 600 ms hold");
    assert_eq!(
        opacity_at(&mut lw, 37),
        0.0,
        "616 ms: the hold is over and there is no fade"
    );
}
