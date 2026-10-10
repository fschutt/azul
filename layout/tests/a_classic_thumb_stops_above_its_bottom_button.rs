//! A classic scrollbar's thumb stops above its bottom arrow button.
//!
//! The thumb is painted once, just below the top button, and moved by a live
//! GPU transform that `GpuStateManager::update_scrollbar_transforms` computes
//! from the same geometry the bar is painted with - including its arrow
//! buttons. That updater decided the VERTICAL bar's buttons from
//! `scrollbar_height`, the reservation of the HORIZONTAL bar. A vertical-only
//! classic bar reserves its width and no height, so it was measured as an
//! overlay without buttons: the live offset ran the thumb along the whole
//! track, and at the bottom it overshot into (and past) the bottom button by
//! 2 x button x (1 - viewport / content).

use azul_core::{
    dom::{Dom, DomId, NodeId},
    geom::{LogicalPosition, LogicalSize},
    hit_test::ScrollbarHitId,
    resources::RendererResources,
    styled_dom::StyledDom,
    task::Instant,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    solver3::display_list::{DisplayListItem, ScrollbarDrawInfo},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The scroll box: `body(0) > box(1) > content(2)`.
const BOX: NodeId = NodeId::new(1);

/// The box's vertical bar as the root display list paints it.
fn painted_bar(lw: &LayoutWindow) -> ScrollbarDrawInfo {
    let thumb = ScrollbarHitId::VerticalThumb(DomId::ROOT_ID, BOX);
    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("the page is laid out")
        .display_list
        .items
        .iter()
        .find_map(|item| match item {
            DisplayListItem::ScrollBarStyled { info } if info.hit_id == Some(thumb) => {
                Some((**info).clone())
            }
            _ => None,
        })
        .expect("harness: the scroll box paints its vertical bar")
}

/// A 200px box over 800px of content, scrolled to the bottom: the thumb
/// shows a quarter of the track and has travelled all of it. Classic 12px
/// bar (the UA fallback without a system style): 12px arrow buttons, a
/// 176px usable track, a 44px thumb that stops at y=188 - where the bottom
/// button starts.
#[test]
fn a_classic_thumb_at_the_bottom_stops_above_its_bottom_button() {
    let styled = StyledDom::create_from_dom(
        Dom::create_body().with_css("margin: 0;").with_child(
            Dom::create_div()
                .with_css("height: 200px; overflow-y: scroll;")
                .with_child(Dom::create_div().with_css("height: 800px;")),
        ),
    );
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
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

    let bottom = lw
        .scroll_manager
        .get_scroll_state(DomId::ROOT_ID, BOX)
        .expect("harness: the box is a scroll container")
        .max_scroll_offsets()
        .1;
    lw.scroll_manager.set_scroll_position(
        DomId::ROOT_ID,
        BOX,
        LogicalPosition::new(0.0, bottom),
        Instant::from(std::time::Instant::now()),
    );
    lw.scroll_manager.calculate_scrollbar_states();
    let _moved = lw.refresh_scrollbar_transforms();

    let bar = painted_bar(&lw);
    let bottom_button_top = bar
        .button_increment_bounds
        .map(|b| b.0.origin.y)
        .expect("harness: a classic bar has arrow buttons");
    let live_offset = lw
        .gpu_state_manager
        .get_cache(DomId::ROOT_ID)
        .and_then(|cache| cache.current_transform_values.get(&BOX))
        .map_or(0.0, |t| t.m[3][1]);
    let thumb = bar.thumb_bounds.0;
    let thumb_bottom = thumb.origin.y + live_offset + thumb.size.height;
    assert!(
        thumb_bottom <= bottom_button_top + 0.5,
        "scrolled to the bottom, the thumb (painted at y={}, {}px long, live offset {live_offset}) \
         ends at y={thumb_bottom}, past the bottom button that starts at y={bottom_button_top}",
        thumb.origin.y,
        thumb.size.height,
    );
}
