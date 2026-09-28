//! A scroll box has a scrollbar exactly where its style draws one.
//!
//! "Is there a bar here" had three answers: the painter read the style
//! (`scrollbar-width: none` - no bar), layout's `needs_*` flag meant
//! "this axis scrolls", and the scroll manager - which every shell asks
//! BEFORE the content, on every press - built a bar for any axis whose content
//! was larger than its box, 16px thick when the style said zero. A bar only the
//! scroll manager believed in took presses nobody could see it take: the
//! TextInput whose text could not be selected once it overflowed
//! (`a_press_on_an_overflowing_field_selects.rs`), and the phantom vertical bar
//! down an `overflow-y: hidden` box whose content ran past its bottom.
//!
//! These drive the production path - layout, registration, the scroll
//! manager's bars - on real boxes.

use azul_core::{
    dom::{Dom, DomId, NodeId, ScrollbarOrientation},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The scroll box of every fixture here: `body(0) > box(1) > content(2)`.
const BOX: NodeId = NodeId::new(1);

/// One layout pass of `styled` at `w`x`h`, the way the shells run one: the
/// window state first, then the funnel, which publishes the scroll state.
fn lay_out(lw: &mut LayoutWindow, styled: StyledDom, w: f32, h: f32) {
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(w, h);
    lw.current_window_state = ws.clone();
    let mut debug = None;
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut debug,
    )
    .expect("the page lays out");
}

/// `body` without margins holding one box styled `box_css` around one block
/// styled `content_css`.
fn page(box_css: &str, content_css: &str) -> StyledDom {
    StyledDom::create_from_dom(
        Dom::create_body().with_css("margin: 0;").with_child(
            Dom::create_div()
                .with_css(box_css)
                .with_child(Dom::create_div().with_css(content_css)),
        ),
    )
}

/// The border box of `node` as laid out.
fn border_box(lw: &LayoutWindow, node: NodeId) -> LogicalRect {
    let lr = lw
        .layout_results
        .get(&DomId::ROOT_ID)
        .expect("the page is laid out");
    let idx = *lr
        .layout_tree
        .dom_to_layout
        .get(&node)
        .and_then(|indices| indices.first())
        .expect("the node has a layout box");
    let origin = lr
        .calculated_positions
        .get(idx.index())
        .copied()
        .expect("the node has a position");
    let size = lr
        .layout_tree
        .get(idx)
        .and_then(|n| n.used_size)
        .expect("the node has a size");
    LogicalRect::new(origin, size)
}

/// M1c. `overflow-y: hidden` gives the vertical axis no bar, however far the
/// content reaches past the bottom - only `scroll` and `auto` carry one. The
/// horizontal `auto` axis overflows too, so the box IS a registered scroll
/// container with a horizontal bar; the vertical one must not come with it.
#[test]
fn a_hidden_axis_that_overflows_has_no_scrollbar() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lay_out(
        &mut lw,
        page(
            "width: 200px; height: 40px; overflow-x: auto; overflow-y: hidden;",
            "width: 400px; height: 120px;",
        ),
        400.0,
        300.0,
    );

    assert!(
        lw.scroll_manager
            .get_scroll_state(DomId::ROOT_ID, BOX)
            .is_some(),
        "harness: the box overflows its auto axis, so it is a registered scroll container"
    );
    assert!(
        lw.scroll_manager
            .get_scrollbar_state(DomId::ROOT_ID, BOX, ScrollbarOrientation::Horizontal)
            .is_some(),
        "harness: the overflowing auto axis has its bar"
    );

    assert!(
        lw.scroll_manager
            .get_scrollbar_state(DomId::ROOT_ID, BOX, ScrollbarOrientation::Vertical)
            .is_none(),
        "the hidden vertical axis has no bar, although 120px of content overflow 40px"
    );
    let b = border_box(&lw, BOX);
    let top_right = LogicalPosition::new(b.origin.x + b.size.width - 3.0, b.origin.y + 5.0);
    let press = lw.scroll_manager.hit_test_scrollbars(top_right);
    assert!(
        press.is_none(),
        "a press at the box's top-right corner {top_right:?} is the content's, got {press:?}"
    );
}
