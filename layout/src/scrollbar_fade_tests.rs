//! A scrolled container goes idle once its scrollbar has faded.
//!
//! `GpuStateManager::scrollbar_fade_left` - how long the overlay bars still have
//! to be on screen, which `wait_settled` of the E2E runner asks - was the
//! milliseconds left AS OF THE LAST FRAME. Only a frame refreshes it, and a window
//! that has stopped drawing (an idle one: the state a settle wait waits for)
//! held its last answer for ever: a page whose sidebar scrolls (a long list of
//! repaint layers: its bar is stamped as active at every layout) never settled -
//! "a scrollbar waiting to fade out" for more than 15 s of engine time - and the
//! editor's idle assertions had to wait a fixed 1.5 s instead. The answer now
//! counts down by the clock to the deadline the last frame set.

use std::time::Duration;

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::{LogicalPosition, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
    task::Instant,
};
use rust_fontconfig::FcFontCache;

use crate::{callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState};

/// A 300 x 100 box that scrolls vertically over 400 px of content.
fn window() -> (LayoutWindow, NodeId) {
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_class("box".into())
            .with_css("width: 300px; height: 100px; overflow-y: scroll;")
            .with_child(Dom::create_div().with_css("height: 400px; width: 280px; background: #dddddd;")),
    );
    let styled_dom = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
    lw.current_window_state = ws.clone();
    lw.layout_and_generate_display_list(
        styled_dom,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )
    .expect("the page lays out");
    let sd = &lw.layout_results[&DomId::ROOT_ID].styled_dom;
    let nodes = sd.node_data.as_container();
    let scroller = (0..nodes.len())
        .map(NodeId::new)
        .find(|n| nodes[*n].has_class("box"))
        .expect("the box");
    (lw, scroller)
}

#[test]
fn a_scrolled_container_goes_idle_after_its_scrollbar_has_faded() {
    let (mut lw, scroller) = window();
    lw.scroll_manager.set_scroll_position(
        DomId::ROOT_ID,
        scroller,
        LogicalPosition::new(0.0, 60.0),
        Instant::now(),
    );

    // A frame, as the shells draw one: the bar is on screen, with its hold
    // (500 ms) and its fade (200 ms) still to come.
    let _ = lw.refresh_scrollbar_gpu_cache_for_cpu_frame();
    let left = lw.gpu_state_manager.scrollbar_fade_left();
    assert!(
        (1..=700).contains(&left),
        "just scrolled: the bar has up to 700 ms to go, got {left}"
    );

    // The window now draws nothing: no frame refreshes anything. The clock runs.
    std::thread::sleep(Duration::from_millis(900));
    assert_eq!(
        lw.gpu_state_manager.scrollbar_fade_left(),
        0,
        "no frame was drawn since, yet the bar is gone by the clock: the window is idle"
    );

    // And a frame drawn now agrees: nothing is fading, nothing is left.
    let _ = lw.refresh_scrollbar_gpu_cache_for_cpu_frame();
    assert_eq!(lw.gpu_state_manager.scrollbar_fade_left(), 0);
    assert!(!lw.gpu_state_manager.scrollbar_fade_active);
}

/// The countdown is by the clock between frames, not a number that waits for
/// the next one.
#[test]
fn the_time_left_shrinks_between_frames() {
    let (mut lw, scroller) = window();
    lw.scroll_manager.set_scroll_position(
        DomId::ROOT_ID,
        scroller,
        LogicalPosition::new(0.0, 30.0),
        Instant::now(),
    );
    let _ = lw.refresh_scrollbar_gpu_cache_for_cpu_frame();
    let first = lw.gpu_state_manager.scrollbar_fade_left();
    std::thread::sleep(Duration::from_millis(250));
    let second = lw.gpu_state_manager.scrollbar_fade_left();
    assert!(
        second + 150 <= first,
        "250 ms later, with no frame in between: {first} ms -> {second} ms"
    );
}
