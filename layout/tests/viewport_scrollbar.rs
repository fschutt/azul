//! The VIEWPORT's scrollbar is one bar, whoever asks about it.
//!
//! CSS Overflow 3 §3.3 gives the root element's overflow to the viewport, so a
//! page taller than the window is scrolled by a bar that belongs to the WINDOW.
//! Four consumers compute that bar: the display list paints it, the scroll
//! manager hit-tests and drags it, the GPU value cache moves its thumb, and a
//! patched display-list build damages it. Each of them has to agree with the
//! others, and with a fresh window of the same size - or the bar is drawn where
//! the pointer cannot grab it, stays parked while the page scrolls, or leaves
//! stale pixels behind a resize (`real_ribbon_resize_sweep_matches_fresh_at_
//! every_step` in the dll: 6 px at width 705).

use azul_core::{
    dom::{Dom, DomId, NodeId, ScrollbarOrientation},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    hit_test::ScrollbarHitId,
    resources::RendererResources,
    styled_dom::StyledDom,
    task::Instant,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    solver3::display_list::{set_dl_patching_enabled, DisplayListItem, ScrollbarDrawInfo},
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// The root element of the root DOM: the node whose bar is the viewport's.
const ROOT: NodeId = NodeId::new(0);

/// `body` with the UA's 8px margins around one 600px block: 616px of page, so
/// a window 300px tall has 316px to scroll. No text, so no font moves a number.
fn long_page() -> StyledDom {
    StyledDom::create_from_dom(
        Dom::create_body()
            .with_css("margin: 8px;")
            .with_child(Dom::create_div().with_css("height: 600px;")),
    )
}

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

/// A new window, laid out once at `w`x`h`.
fn fresh_window(w: f32, h: f32) -> LayoutWindow {
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lay_out(&mut lw, long_page(), w, h);
    lw
}

/// The window's own DOM laid out again at `w`x`h`: the resize path, which
/// re-runs layout on the StyledDom it already has.
fn relayout(lw: &mut LayoutWindow, w: f32, h: f32) {
    let styled = lw
        .layout_results
        .remove(&DomId::ROOT_ID)
        .expect("laid out before")
        .styled_dom;
    lay_out(lw, styled, w, h);
}

/// The viewport's vertical bar, as the root DOM's display list paints it.
fn painted_bar(lw: &LayoutWindow) -> Option<ScrollbarDrawInfo> {
    let viewport_thumb = ScrollbarHitId::VerticalThumb(DomId::ROOT_ID, ROOT);
    lw.layout_results
        .get(&DomId::ROOT_ID)?
        .display_list
        .items
        .iter()
        .find_map(|item| match item {
            DisplayListItem::ScrollBarStyled { info } if info.hit_id == Some(viewport_thumb) => {
                Some((**info).clone())
            }
            _ => None,
        })
}

/// Everything about a bar that decides its pixels. The GPU keys it carries
/// are handles minted per window, not geometry.
fn geometry(bar: &ScrollbarDrawInfo) -> String {
    format!(
        "track {:?}, thumb {:?}, buttons {:?} {:?}, thumb offset {:?}",
        bar.track_bounds.0,
        bar.thumb_bounds.0,
        bar.button_decrement_bounds.map(|b| b.0),
        bar.button_increment_bounds.map(|b| b.0),
        bar.thumb_initial_transform.m[3],
    )
}

/// Every pixel of `rect` lies inside one of the `damage` rects.
fn covers(damage: &[LogicalRect], rect: LogicalRect) -> bool {
    let inside = |x: f32, y: f32| {
        damage.iter().any(|d| {
            x >= d.origin.x
                && x <= d.origin.x + d.size.width
                && y >= d.origin.y
                && y <= d.origin.y + d.size.height
        })
    };
    let right = rect.origin.x + rect.size.width;
    let bottom = rect.origin.y + rect.size.height;
    let mut y = rect.origin.y + 0.5;
    while y < bottom {
        let mut x = rect.origin.x + 0.5;
        while x < right {
            if !inside(x, y) {
                return false;
            }
            x += 1.0;
        }
        y += 1.0;
    }
    true
}

/// A relayout paints the same viewport bar a fresh window of the same size
/// paints.
///
/// The display list sized the thumb from the ScrollManager snapshot it is
/// built with, and that snapshot is taken BEFORE the pass: it holds what the
/// PREVIOUS layout published. After a resize from 500 to 420 wide the relayout
/// sized the thumb for the 500px pass's 616px margin box, while a fresh
/// window's first pass has no snapshot at all and fell back to the bare
/// 600px content. Two thumbs for one layout: in the dll's ribbon resize sweep
/// that was 6 px at the thumb's rounded end, at 705 - the first width after
/// the one at which the root was first published.
#[test]
fn a_relayout_paints_the_viewport_bar_a_fresh_window_paints() {
    let mut resized = fresh_window(500.0, 300.0);
    relayout(&mut resized, 420.0, 300.0);
    let after_resize =
        painted_bar(&resized).expect("the page overflows: the relayout paints the viewport's bar");
    let from_scratch = painted_bar(&fresh_window(420.0, 300.0))
        .expect("the page overflows: a fresh window paints the viewport's bar");
    assert_eq!(
        geometry(&after_resize),
        geometry(&from_scratch),
        "a relayout to 420x300 painted a different viewport bar than a fresh 420x300 window"
    );
}

/// The viewport's bar is painted where the pointer finds it: the painted track
/// and thumb are the ones the scroll manager hit-tests and drags.
///
/// The manager's scrollport for the root is the WINDOW (`register_scroll_nodes`
/// publishes it, and a thumb press is hit-tested against it). The painted bar
/// ran along the root's own box instead: the page's margins inside the
/// window's edge, and as tall as the page - 600px of track in a 300px window.
#[test]
fn the_viewport_bar_is_painted_where_the_pointer_finds_it() {
    let lw = fresh_window(400.0, 300.0);
    let bar = painted_bar(&lw).expect("the page overflows: the viewport's bar is painted");
    let hit = lw
        .scroll_manager
        .get_scrollbar_state(DomId::ROOT_ID, ROOT, ScrollbarOrientation::Vertical)
        .expect("the viewport's bar is hit-testable");
    assert_eq!(
        bar.track_bounds.0, hit.track_rect,
        "the viewport's track is painted away from where the pointer finds it"
    );
    let painted_thumb = (bar.thumb_bounds.0.origin.y, bar.thumb_bounds.0.size.height);
    let grabbed_thumb = (
        hit.track_rect.origin.y + hit.button_size + hit.thumb_offset,
        hit.thumb_length,
    );
    assert!(
        (painted_thumb.0 - grabbed_thumb.0).abs() < 0.5
            && (painted_thumb.1 - grabbed_thumb.1).abs() < 0.5,
        "the viewport's thumb is painted at (top, length) {painted_thumb:?} but grabbed at \
         {grabbed_thumb:?}"
    );
}

/// Scrolled to the bottom, the viewport's thumb is painted where the pointer
/// that dragged it there finds it.
///
/// A thumb's live position is the GPU value `update_scrollbar_transforms`
/// computes. Measured against the root's own box - which IS its content, so
/// it has nothing to scroll - that value stayed 0: the manager's offset went
/// to the bottom and the painted thumb stayed at the top.
#[test]
fn the_viewport_thumb_follows_the_page_to_the_bottom() {
    let mut lw = fresh_window(400.0, 300.0);
    let bottom = lw
        .scroll_manager
        .get_scroll_state(DomId::ROOT_ID, ROOT)
        .expect("the viewport is a scroll container")
        .max_scroll_offsets()
        .1;
    assert!(
        bottom > 300.0,
        "harness: 616px of page in a 300px window, got {bottom}px of travel"
    );
    lw.scroll_manager.set_scroll_position(
        DomId::ROOT_ID,
        ROOT,
        LogicalPosition::new(0.0, bottom),
        Instant::from(std::time::Instant::now()),
    );
    lw.scroll_manager.calculate_scrollbar_states();
    let _moved = lw.refresh_scrollbar_transforms();

    let bar = painted_bar(&lw).expect("the viewport's bar is painted");
    let live_offset = lw
        .gpu_state_manager
        .get_cache(DomId::ROOT_ID)
        .and_then(|cache| cache.current_transform_values.get(&ROOT))
        .map_or(0.0, |t| t.m[3][1]);
    let hit = lw
        .scroll_manager
        .get_scrollbar_state(DomId::ROOT_ID, ROOT, ScrollbarOrientation::Vertical)
        .expect("the viewport's bar is hit-testable");
    let painted_top = bar.thumb_bounds.0.origin.y + live_offset;
    let grabbed_top = hit.track_rect.origin.y + hit.button_size + hit.thumb_offset;
    assert!(
        (painted_top - grabbed_top).abs() < 1.0,
        "scrolled to the bottom, the viewport's thumb is painted at y={painted_top} but the \
         pointer finds it at y={grabbed_top}"
    );
}

/// Where the viewport's bar lies over another bar, the press goes to the
/// viewport's - the one painted on top.
///
/// The viewport's bar is painted last of all, after everything the root's
/// stacking context holds, so over a scroll box flush with the window's edge
/// it is the bar the user sees and grabs. The scroll manager tried the bars in
/// reverse key order, and the root element is node 0: the press went to the
/// scroll box underneath.
#[test]
fn a_press_on_the_viewport_bar_is_not_taken_by_a_bar_under_it() {
    // A scroll box as wide as the window, above 600px more of page: its bar
    // and the viewport's share the window's right edge for 200px.
    let page = Dom::create_body()
        .with_css("margin: 0;")
        .with_child(
            Dom::create_div()
                .with_css("height: 200px; overflow-y: scroll;")
                .with_child(Dom::create_div().with_css("height: 1000px;")),
        )
        .with_child(Dom::create_div().with_css("height: 600px;"));
    let mut lw = LayoutWindow::new(FcFontCache::default()).expect("a layout window");
    lay_out(&mut lw, StyledDom::create_from_dom(page), 400.0, 300.0);
    let scroll_box = NodeId::new(1);
    let press = LogicalPosition::new(399.0, 100.0);
    for (what, node) in [("the viewport", ROOT), ("the scroll box", scroll_box)] {
        let track = lw
            .scroll_manager
            .get_scrollbar_state(DomId::ROOT_ID, node, ScrollbarOrientation::Vertical)
            .unwrap_or_else(|| panic!("harness: {what} has a vertical bar"))
            .track_rect;
        assert!(
            track.contains(press),
            "harness: the press at {press:?} lies on {what}'s track {track:?}"
        );
    }
    let hit = lw
        .scroll_manager
        .hit_test_scrollbars(press)
        .expect("a press on two bars hits one of them");
    assert_eq!(
        (hit.dom_id, hit.node_id),
        (DomId::ROOT_ID, ROOT),
        "the press went to the bar painted UNDER the viewport's"
    );
}

/// A patched build damages a viewport bar it changed, even when no node's box
/// moved.
///
/// Bars are re-emitted by the stacking-context walk on every build and carry
/// no layout tag, so the patch's changed-node damage never saw one - and the
/// viewport's bar changes without its node: a taller window lengthens the
/// track while the root's box stays where it was. The patch's damage is what
/// a frame paints with ALONE when the item diff bails on a changed count.
#[test]
fn a_patched_pass_damages_the_viewport_bar_it_changed() {
    // The patch toggle is process-global, and `dl_patch_golden` switches it off
    // for a moment. A pass that ran in that moment is a wholesale build with
    // no patch damage to read, so run the sequence again.
    for _ in 0..3 {
        set_dl_patching_enabled(true);
        let mut lw = fresh_window(400.0, 300.0);
        let before = painted_bar(&lw).expect("the page overflows: the viewport's bar is painted");
        lw.layout_cache.resize_only_hint = true;
        relayout(&mut lw, 400.0, 340.0);
        if !lw.layout_cache.last_build_was_patched {
            continue;
        }
        let after = painted_bar(&lw).expect("the taller window still paints the viewport's bar");
        assert_ne!(
            geometry(&before),
            geometry(&after),
            "harness: the taller window must repaint the viewport's bar"
        );
        let damage = lw
            .layout_cache
            .last_patch_damage
            .clone()
            .unwrap_or_default();
        for (which, bar) in [("old", &before), ("new", &after)] {
            let rect = bar.bounds.0;
            assert!(
                covers(&damage, rect),
                "a patched pass repainted the viewport's bar, but its damage {damage:?} leaves \
                 the {which} bar {rect:?} stale"
            );
        }
        return;
    }
    panic!("harness: the resize-hinted pass never took the patched path");
}
