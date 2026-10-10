//! A layout blit repaints the scrollbar it dragged.
//!
//! When a patched layout pass moves a block of boxes by one delta, the CPU
//! backends present it by moving the block's pixels
//! (`cpurender::execute_translate_blit`, one memmove per mover rect over its
//! old and new place) and repainting only what the move could not produce.
//! `compute_patch_move_summary` leaves the movers' ANCESTORS out of that
//! repaint: an ancestor paints below its descendants, under their opaque
//! backgrounds. Its SCROLLBAR does not - a scroll container paints its bar
//! after its content, on top of it, and the viewport's bar tops the page.
//! Where a bar crosses a mover, the memmove dragged the bar's pixels along
//! with the mover's, and an unchanged bar is in no diff's damage: the old
//! thumb stayed painted where the move put it.
//!
//! The frame the backends present - the old frame, the blit, a damage-only
//! repaint of the new list - must look like a full repaint of the new list.

use azul_core::{
    dom::{DomId, NodeId, ScrollbarOrientation},
    hit_test::ScrollbarHitId,
    transform::ComputedTransform3D,
};
use azul_css::props::style::scrollbar::ScrollbarVisibilityMode;
use azul_layout::{
    cpurender::{self, ScrollOffsetMap, TranslateHint},
    solver3::display_list::{
        BorderRadius, DisplayList, DisplayListItem, ScrollbarDrawInfo, WindowLogicalRect,
    },
};

use super::a_scroll_box_keeps_its_blit_on_a_scrolled_page::{
    fill, first_difference, rect, rgb, Raster,
};

/// A classic vertical bar down the right edge of a scroll box, its thumb
/// 30px from the top: painted after the box's content, over it.
fn bar() -> DisplayListItem {
    let track = rect(180.0, 0.0, 12.0, 200.0);
    DisplayListItem::ScrollBarStyled {
        info: Box::new(ScrollbarDrawInfo {
            bounds: WindowLogicalRect(track),
            orientation: ScrollbarOrientation::Vertical,
            track_bounds: WindowLogicalRect(track),
            track_color: rgb(235, 235, 235),
            thumb_bounds: WindowLogicalRect(rect(180.0, 30.0, 12.0, 30.0)),
            thumb_color: rgb(60, 60, 60),
            thumb_border_radius: BorderRadius::default(),
            button_decrement_bounds: None,
            button_increment_bounds: None,
            button_color: rgb(200, 200, 200),
            opacity_key: None,
            thumb_transform_key: None,
            thumb_initial_transform: ComputedTransform3D::IDENTITY,
            hit_id: Some(ScrollbarHitId::VerticalThumb(DomId::ROOT_ID, NodeId::new(1))),
            clip_to_container_border: false,
            container_border_radius: BorderRadius::default(),
            visibility: ScrollbarVisibilityMode::Always,
        }),
    }
}

/// The scroll box's content: one opaque card at `y`, under the bar's right
/// edge, then the box's bar.
fn list_with_the_card_at(y: f32) -> DisplayList {
    DisplayList {
        items: vec![
            fill(rect(0.0, 0.0, 200.0, 200.0), rgb(255, 255, 255)),
            fill(rect(10.0, y, 180.0, 60.0), rgb(30, 60, 200)),
            bar(),
        ],
        ..Default::default()
    }
}

/// The card moves 20px down (a box above it grew); the bar does not move.
#[test]
fn a_layout_blit_repaints_the_scrollbar_it_dragged() {
    let old = list_with_the_card_at(20.0);
    let new = list_with_the_card_at(40.0);
    let at_rest = ScrollOffsetMap::new();
    let mover = rect(10.0, 20.0, 180.0, 60.0);
    let hint = TranslateHint {
        delta: (0.0, 20.0),
        region_old: mover,
    };

    let mut raster = Raster::new();
    let mut presented = raster.full(&old, &at_rest);
    let blit = cpurender::execute_translate_blit(&mut presented, &hint, &[], &[mover], &new, 1.0);
    raster.repaint(&new, &mut presented, &at_rest, &blit.damage);
    let expected = raster.full(&new, &at_rest);

    assert_eq!(
        first_difference(&presented, &expected),
        None,
        "the blit moved the card and the bar's pixels over it; the bar did not move and must be \
         repainted where it is and where its pixels were dragged to (damage {:?})",
        blit.damage
    );
}
