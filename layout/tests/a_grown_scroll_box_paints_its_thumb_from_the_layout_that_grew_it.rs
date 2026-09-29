//! A scroll box whose content grew paints its thumb from the layout that
//! grew it.
//!
//! `paint_scrollbars` sized a scroll box's thumb from `scroll_offsets`: the
//! scroll manager's snapshot taken BEFORE the pass, which holds the extent
//! the PREVIOUS layout published. `register_scroll_nodes` publishes this
//! pass's extent only after the display list is built. So the pass that
//! grows a box's content painted the thumb of the old content, while the
//! press router (`hit_test_scrollbars`) already measured the new one: the
//! thumb was drawn one layout late, and pressed somewhere else than drawn,
//! until the next pass. (The viewport's bar got its own fix,
//! 9a0f13b4c; this is every other scroll box.)
//!
//! The page: a 200x200 `overflow-y: scroll` box over 400px of content,
//! laid out again over 800px - tall enough that both thumbs (~88px and
//! ~44px on a 12px classic bar) are longer than the minimum thumb (twice the
//! bar's thickness), which would make them equal.

use azul_core::{
    dom::{Dom, DomId, NodeId, ScrollbarOrientation},
    geom::{LogicalRect, LogicalSize},
    hit_test::ScrollbarHitId,
    resources::RendererResources,
    styled_dom::StyledDom,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// body(0) > box(1) > content(2).
const BOX: NodeId = NodeId::new(1);

fn page(content_height: f32) -> StyledDom {
    StyledDom::create_from_dom(
        Dom::create_body().with_css("margin: 0;").with_child(
            Dom::create_div()
                .with_css("width: 200px; height: 200px; overflow-y: scroll;")
                .with_child(Dom::create_div().with_css(&format!("height: {content_height}px;"))),
        ),
    )
}

/// One layout pass of `styled` in a 400x300 window, the way the shells run
/// one: the window state first, then the funnel, which publishes the scroll
/// state.
fn lay_out(lw: &mut LayoutWindow, styled: StyledDom) {
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(400.0, 300.0);
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

/// The box's thumb as this pass's display list paints it.
fn painted_thumb(lw: &LayoutWindow) -> LogicalRect {
    let lr = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("the page is laid out");
    lr.display_list
        .items
        .iter()
        .find_map(|item| match item {
            DisplayListItem::ScrollBarStyled { info }
                if info.hit_id == Some(ScrollbarHitId::VerticalThumb(DomId::ROOT_ID, BOX)) =>
            {
                Some(*info.thumb_bounds.inner())
            }
            _ => None,
        })
        .expect("the scroll box paints a vertical bar")
}

/// How long the thumb is where the press router measures it.
fn pressed_thumb_length(lw: &LayoutWindow) -> f32 {
    lw.scroll_manager
        .get_scrollbar_state(DomId::ROOT_ID, BOX, ScrollbarOrientation::Vertical)
        .expect("the scroll box has a bar to press")
        .thumb_length
}

#[test]
fn a_grown_scroll_box_paints_its_thumb_from_the_layout_that_grew_it() {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lay_out(&mut lw, page(400.0));
    let first = painted_thumb(&lw).size.height;
    assert!(
        (first - pressed_thumb_length(&lw)).abs() < 0.5,
        "harness: the first pass paints the thumb it measures ({first} vs {})",
        pressed_thumb_length(&lw)
    );

    lay_out(&mut lw, page(800.0));
    let pressed = pressed_thumb_length(&lw);
    assert!(
        pressed < first - 1.0,
        "harness: twice the content makes a shorter thumb ({pressed} vs {first})"
    );
    let painted = painted_thumb(&lw).size.height;
    assert!(
        (painted - pressed).abs() < 0.5,
        "the pass that doubled the box's content must paint the thumb of that content: painted \
         {painted}px long, measured {pressed}px (the previous layout's thumb was {first}px)"
    );
}
