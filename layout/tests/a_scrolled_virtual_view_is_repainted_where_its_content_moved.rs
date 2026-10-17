//! A `VirtualView` scrolled on the lightweight path is repainted where its
//! content moved.
//!
//! A `VirtualView` has no scroll frame: its scroll is the `content_offset`
//! of its display-list item (`materialized origin - scroll offset`), and the
//! lightweight scroll path only re-points that field
//! (`LayoutWindow::patch_virtual_view_content_offset`). The CPU backends
//! then present the frame from the display-list DIFF. `is_visually_equal`
//! compared two `VirtualView` items by child, bounds and clip alone, so a
//! view that had scrolled compared EQUAL, the diff produced no damage, and
//! the content stayed where it was while the bar moved - AzReview's sheet
//! strip on the Mac: "only the bottom 8px (the scrollbar) still update".
//!
//! Each test builds the frame the backends present for a scroll from one
//! offset to the next (`headless/mod.rs render_frame` and its e2e twin: the
//! parent item diff plus the child diff, then a damage-only repaint), many
//! steps in a row, and asserts each frame equals a full render at the new
//! offset.

use std::{collections::BTreeMap, sync::Arc};

use azul_core::{
    dom::DomId,
    geom::{LogicalPosition, LogicalRect},
};
use azul_layout::{
    cpurender::{self, CpuRenderState, ScrollOffsetMap},
    solver3::display_list::{DisplayList, DisplayListItem, WindowLogicalRect},
};

use super::a_scroll_box_keeps_its_blit_on_a_scrolled_page::{
    fill, first_difference, rect, rgb, Raster,
};

const CHILD: DomId = DomId { inner: 1 };

/// The box the view shows its content through.
fn view_box() -> LogicalRect {
    rect(20.0, 20.0, 120.0, 60.0)
}

/// The view's item, scrolled by `x`: the materialized window starts at
/// origin zero, so `content_offset` is `-x`.
fn view_item(x: f32) -> DisplayListItem {
    DisplayListItem::VirtualView {
        child_dom_id: CHILD,
        bounds: WindowLogicalRect(view_box()),
        clip_rect: WindowLogicalRect(view_box()),
        content_offset: LogicalPosition::new(-x, 0.0),
    }
}

/// The parent list: a grey window and the view scrolled by `x`.
fn parent(x: f32) -> DisplayList {
    DisplayList {
        items: vec![
            fill(rect(0.0, 0.0, 200.0, 200.0), rgb(128, 128, 128)),
            view_item(x),
        ],
        ..Default::default()
    }
}

/// The child list, 0-relative: six 40px stripes of six colours, 240px of
/// content for a 120px box, so no offset up to 120 looks like another.
fn child() -> Arc<DisplayList> {
    let colours = [
        rgb(220, 40, 40),
        rgb(40, 160, 60),
        rgb(40, 80, 220),
        rgb(230, 200, 30),
        rgb(180, 40, 200),
        rgb(30, 200, 210),
    ];
    let items = colours
        .iter()
        .enumerate()
        .map(|(k, c)| fill(rect(40.0 * k as f32, 0.0, 40.0, 60.0), *c))
        .collect();
    Arc::new(DisplayList {
        items,
        ..Default::default()
    })
}

fn state(children: &BTreeMap<DomId, Arc<DisplayList>>) -> CpuRenderState {
    CpuRenderState::new(ScrollOffsetMap::new()).with_virtual_view_display_lists(children.clone())
}

#[test]
fn a_virtual_view_item_whose_content_offset_moved_is_not_visually_equal() {
    assert!(
        view_item(10.0).is_visually_equal(&view_item(10.0)),
        "the same view at the same scroll"
    );
    assert!(
        !view_item(10.0).is_visually_equal(&view_item(50.0)),
        "the same view scrolled by 40px more paints different pixels, so the item diff must \
         damage it"
    );
}

#[test]
fn a_virtual_view_scrolled_on_the_lightweight_path_is_repainted_where_its_content_moved() {
    let children: BTreeMap<DomId, Arc<DisplayList>> = [(CHILD, child())].into_iter().collect();
    let empty = ScrollOffsetMap::new();
    let mut raster = Raster::new();
    // Forward, back, a fractional step, the end: every step is one frame.
    let steps = [
        0.0f32, 8.0, 24.0, 40.0, 57.5, 80.0, 120.0, 96.0, 30.0, 0.0, 119.0,
    ];

    let mut previous = parent(steps[0]);
    let mut frame = raster.full_state(&previous, &state(&children));
    for (i, x) in steps.iter().enumerate().skip(1) {
        let current = parent(*x);
        // The backends' recipe: the parent item diff and the child diff,
        // then a damage-only repaint of the frame that is on screen.
        let mut damage =
            cpurender::compute_display_list_damage(&previous, &current, &empty, &empty)
                .expect("the two lists hold the same items");
        damage.extend(cpurender::compute_virtual_view_damage(
            &current, &children, &children, &empty,
        ));
        raster.repaint_state(&current, &mut frame, &state(&children), &damage);

        let expected = raster.full_state(&current, &state(&children));
        assert_eq!(
            first_difference(&frame, &expected),
            None,
            "step {i}: the view scrolled to x={x}, so the presented frame must equal a full \
             render there - the content moved with the scroll (damage {damage:?})"
        );
        previous = current;
    }
}
