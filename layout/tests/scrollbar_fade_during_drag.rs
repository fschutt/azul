//! A scrollbar thumb the user is holding must not fade out.
//!
//! The fade is driven by the scroll manager's per-node `last_activity` stamp,
//! which only a scroll-position change refreshes. A thumb drag refreshes it
//! on every pointer motion — and not at all while the pointer rests. Holding
//! the thumb still for longer than the fade delay therefore faded the bar out
//! under the user's finger, because the drag itself was recorded in the
//! shell's window state where the fade could not see it.

use std::sync::atomic::{AtomicU64, Ordering};

use azul_core::{
    dom::{Dom, DomId, IdOrClass, NodeId, ScrollbarOrientation},
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

fn scroller() -> LayoutWindow {
    let css_src = "* { margin: 0; padding: 0; } body { font-size: 14px; width: 600px; } .box { \
                   display: block; width: 600px; height: 100px; overflow-y: scroll; } .line { \
                   display: block; }";
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
    let (css, _) = azul_css::parser2::new_from_str(css_src);
    let styled_dom = StyledDom::create(&mut dom, css);

    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut window_state = FullWindowState::default();
    window_state.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = window_state.clone();
    let renderer_resources = RendererResources::default();
    let system_callbacks = ExternalSystemCallbacks::rust_internal();
    let mut debug_messages = Some(Vec::new());
    lw.layout_and_generate_display_list(
        styled_dom,
        &window_state,
        &renderer_resources,
        &system_callbacks,
        &mut debug_messages,
    )
    .unwrap();
    lw
}

/// The scroll box is body's first child.
fn scroll_box() -> NodeId {
    NodeId::new(1)
}

/// Deterministic tick clock: the sync reads "now" through the system
/// callbacks, so the test owns that clock.
static CLOCK: AtomicU64 = AtomicU64::new(0);

extern "C" fn tick_clock() -> Instant {
    Instant::Tick(SystemTick::new(CLOCK.load(Ordering::SeqCst)))
}

fn at(t: u64) -> Instant {
    Instant::Tick(SystemTick::new(t))
}

fn ticks(d: u64) -> Duration {
    Duration::Tick(SystemTickDiff { tick_diff: d })
}

const FADE_DELAY: u64 = 500;
const FADE_DURATION: u64 = 200;

/// Run the per-frame opacity sync at tick `now` and return the vertical
/// scrollbar's opacity for the scroll box.
fn opacity_at(lw: &mut LayoutWindow, now: u64) -> f32 {
    CLOCK.store(now, Ordering::SeqCst);
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
    let layout_tree = &layout_results[&DOM].layout_tree;
    LayoutWindow::synchronize_scrollbar_opacity(
        gpu_state_manager,
        scroll_manager,
        DOM,
        layout_tree,
        &cbs,
        ticks(FADE_DELAY),
        ticks(FADE_DURATION),
    );
    gpu_state_manager.caches[&DOM].scrollbar_v_opacity_values[&(DOM, scroll_box())]
}

/// A one-off scroll at tick 0: this is the last activity the fade sees.
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

/// Control: with no drag the bar has fully faded once delay + duration have
/// passed. This is what the fixture must be able to detect.
#[test]
fn a_resting_scrollbar_fades_out() {
    let mut lw = scroller();
    scroll_once_at_zero(&mut lw);
    assert_eq!(opacity_at(&mut lw, 100), 1.0, "inside the delay the bar is opaque");
    assert_eq!(
        opacity_at(&mut lw, FADE_DELAY + FADE_DURATION + 1000),
        0.0,
        "long after the last scroll the bar has faded"
    );
}

/// The bug: the thumb is pressed at tick 0 and held still. Long after the
/// fade delay the bar must still be fully opaque — the user is holding it.
#[test]
fn a_dragged_thumb_never_fades() {
    let mut lw = scroller();
    scroll_once_at_zero(&mut lw);
    lw.scroll_manager
        .begin_thumb_drag(DOM, scroll_box(), ScrollbarOrientation::Vertical, at(0));
    assert_eq!(
        opacity_at(&mut lw, FADE_DELAY + FADE_DURATION + 1000),
        1.0,
        "a thumb the user is holding must not fade"
    );
}

/// Releasing the thumb restarts the fade from the release, not from the last
/// scroll: the bar stays for the full delay and then fades as usual.
#[test]
fn releasing_the_thumb_restarts_the_fade() {
    let mut lw = scroller();
    scroll_once_at_zero(&mut lw);
    lw.scroll_manager
        .begin_thumb_drag(DOM, scroll_box(), ScrollbarOrientation::Vertical, at(0));
    let release = 3000;
    lw.scroll_manager.end_thumb_drag(at(release));
    assert_eq!(opacity_at(&mut lw, release + 100), 1.0, "still inside the delay after release");
    let mid = opacity_at(&mut lw, release + FADE_DELAY + FADE_DURATION / 2);
    assert!(mid > 0.0 && mid < 1.0, "half-way through the fade: {mid}");
    assert_eq!(
        opacity_at(&mut lw, release + FADE_DELAY + FADE_DURATION + 1000),
        0.0,
        "fully faded after delay + duration"
    );
}

/// Only the bar being dragged is pinned; a horizontal bar on the same node
/// (there is none here) or any other node follows the normal fade.
#[test]
fn the_pin_is_per_bar() {
    let mut lw = scroller();
    scroll_once_at_zero(&mut lw);
    lw.scroll_manager
        .begin_thumb_drag(DOM, scroll_box(), ScrollbarOrientation::Horizontal, at(0));
    assert_eq!(
        opacity_at(&mut lw, FADE_DELAY + FADE_DURATION + 1000),
        0.0,
        "dragging the horizontal thumb does not pin the vertical bar"
    );
}
