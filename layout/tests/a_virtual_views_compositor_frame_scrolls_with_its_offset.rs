//! A `VirtualView`'s child dom scrolls on the GPU the way it does on the CPU.
//!
//! The CPU raster draws the child dom at its host item's
//! `bounds.origin + content_offset`. WebRender draws it as an iframe
//! pipeline at the item's bounds and never read `content_offset`, so in GPU
//! mode the rows only moved when the view rendered a new window of them:
//! the bar glided and the content jumped from one window to the next
//! (AzDrive's file lists). The compositor scrolls the child pipeline's root
//! frame instead, by `LayoutWindow::virtual_view_frame_scroll_offsets`,
//! every frame, like any other scroll frame.

use azul_core::{dom::DomId, geom::LogicalPosition, task::Instant};

use super::a_dragged_thumb_scrolls_a_virtual_views_content::{host, item_content_offset, window};

/// The offset the compositor scrolls `child`'s frame by.
fn frame_offset_of(
    lw: &azul_layout::window::LayoutWindow,
    child: DomId,
) -> Option<LogicalPosition> {
    lw.virtual_view_frame_scroll_offsets()
        .into_iter()
        .find_map(|(dom, offset)| (dom == child).then_some(offset))
}

#[test]
fn a_virtual_views_frame_is_scrolled_by_minus_its_content_offset() {
    let mut lw = window();
    let (node, child) = host(&lw);
    // Inside the rows the view rendered: no new window, only a glide.
    for y in [0.0f32, 7.5, 63.25, 140.0] {
        lw.apply_scroll_offset(
            DomId::ROOT_ID,
            node,
            LogicalPosition::new(0.0, y),
            false,
            Instant::now(),
        );
        let content = item_content_offset(&lw, child);
        assert_eq!(
            frame_offset_of(&lw, child),
            Some(LogicalPosition::new(-content.x, -content.y)),
            "at y={y} the CPU draws the rows {content:?} from the box: the GPU frame must scroll \
             them by the same amount, every frame, not when the view renders new rows"
        );
    }
}

#[test]
fn a_virtual_views_frame_offset_follows_the_scroll_to_the_pixel() {
    let mut lw = window();
    let (node, child) = host(&lw);
    lw.apply_scroll_offset(
        DomId::ROOT_ID,
        node,
        LogicalPosition::new(0.0, 10.0),
        false,
        Instant::now(),
    );
    let at_ten = frame_offset_of(&lw, child).expect("the view's frame is scrolled");
    lw.apply_scroll_offset(
        DomId::ROOT_ID,
        node,
        LogicalPosition::new(0.0, 13.0),
        false,
        Instant::now(),
    );
    let at_thirteen = frame_offset_of(&lw, child).expect("the view's frame is scrolled");
    assert!(
        (at_thirteen.y - at_ten.y - 3.0).abs() < 1e-3,
        "three pixels of scroll move the rows three pixels, not a row: {at_ten:?} -> \
         {at_thirteen:?}"
    );
}

#[test]
fn a_new_materialized_origin_moves_the_rows_on_the_cpu_as_on_the_gpu() {
    let mut lw = window();
    let (node, child) = host(&lw);
    let before = item_content_offset(&lw, child);
    // `SetVirtualViewGeometry` with a window that starts 40 px further down:
    // the same rows, re-placed without invoking the callback.
    let mut moved = lw
        .virtual_view_manager
        .materialized_window_origin(DomId::ROOT_ID, node)
        .unwrap_or_default();
    moved.y += 40.0;
    let size = lw
        .virtual_view_manager
        .get_declared_sizes(DomId::ROOT_ID, node)
        .0
        .expect("harness: the view declared its window");
    lw.apply_virtual_view_geometry(
        DomId::ROOT_ID,
        node,
        Some(azul_core::geom::LogicalRect::new(moved, size)),
        None,
    );
    let gpu = frame_offset_of(&lw, child).expect("the view's frame is scrolled");
    let cpu = item_content_offset(&lw, child);
    assert_eq!(
        cpu,
        LogicalPosition::new(-gpu.x, -gpu.y),
        "the GPU frame moved the rows to {gpu:?}: the CPU raster, which reads the item, must draw \
         them there too (it showed them at {before:?} until the next scroll)"
    );
}
