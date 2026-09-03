//! Draggable selection handles (U2-a): the teardrops under each end of a
//! touch selection, painted and dragged by the ENGINE where the platform
//! lends a custom view none of its own (Android).
//!
//! Everything here runs on the host: the geometry, the hit test and the drag
//! are `LayoutWindow` methods with no platform in them. What no host test can
//! show is a finger on a device.

use azul_core::dom::{Dom, IdOrClass};
use azul_core::geom::{LogicalPosition, LogicalSize};
use azul_core::resources::RendererResources;
use azul_core::selection::Selection;
use azul_core::styled_dom::StyledDom;
use azul_layout::managers::text_edit::{
    SelectionHandleEnd, SELECTION_HANDLE_RADIUS, SELECTION_HANDLE_SLOP,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// One paragraph of selectable text, laid out at 14px in an 800x600 window.
fn paragraph() -> LayoutWindow {
    let css_src = "* { margin: 0; padding: 0; } \
                   body { font-size: 14px; width: 600px; } \
                   .p { display: block; width: 600px; }";
    let class: azul_core::dom::IdOrClassVec = vec![IdOrClass::Class("p".into())].into();
    let p = Dom::create_div().with_ids_and_classes(class).with_child(
        Dom::create_text_do_not_use_without_block_level_wrapper(
            "the quick brown fox jumps over the lazy dog and keeps on running",
        ),
    );
    let mut dom = Dom::create_body().with_child(p);
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

/// A paragraph with the range from x=20 to x=120 on the first line selected,
/// the way a press-and-drag makes one, and the engine's handles enabled.
fn with_range() -> LayoutWindow {
    let mut lw = paragraph();
    lw.text_edit_manager.selection_handles = true;
    lw.process_mouse_click_for_selection(LogicalPosition::new(20.0, 8.0), 0);
    lw.process_mouse_drag_for_selection(
        LogicalPosition::new(20.0, 8.0),
        LogicalPosition::new(120.0, 8.0),
    );
    lw
}

fn primary_range(lw: &LayoutWindow) -> Option<(u32, u32)> {
    let mc = lw.text_edit_manager.multi_cursor.as_ref()?;
    match mc.get_primary()?.selection {
        Selection::Range(r) => Some((
            r.start.cluster_id.start_byte_in_run,
            r.end.cluster_id.start_byte_in_run,
        )),
        Selection::Cursor(_) => None,
    }
}

#[test]
fn a_caret_has_no_handles_and_a_range_has_two() {
    let mut lw = paragraph();
    lw.text_edit_manager.selection_handles = true;
    lw.process_mouse_click_for_selection(LogicalPosition::new(20.0, 8.0), 0);
    assert!(
        lw.selection_handle_geometry().is_none(),
        "a collapsed caret has nothing to drag"
    );

    let lw = with_range();
    let (start_b, end_b) = primary_range(&lw).expect("the drag made a range");
    assert!(start_b < end_b, "forward range {start_b}..{end_b}");
    let [start, end] = lw.selection_handle_geometry().expect("two handles");
    assert_eq!(start.end, SelectionHandleEnd::Start);
    assert_eq!(end.end, SelectionHandleEnd::End);
    assert!(
        start.center.x < end.center.x,
        "start handle left of end handle: {start:?} {end:?}"
    );
    assert!(
        (start.center.y - end.center.y).abs() < 0.5,
        "both under the same line"
    );
    // Hanging UNDER the line: the circle's top touches the line's bottom.
    let line_bottom = start.center.y - SELECTION_HANDLE_RADIUS;
    assert!(line_bottom > 8.0, "below the text the drag ran through");
    // The hit box is the circle plus slop on every side.
    let reach = SELECTION_HANDLE_RADIUS + SELECTION_HANDLE_SLOP;
    assert!((start.hit.size.width - 2.0 * reach).abs() < 0.01);
    assert!(start.contains(start.center));
    assert!(!start.contains(LogicalPosition::new(
        start.center.x,
        start.center.y + reach + 1.0
    )));
}

#[test]
fn a_press_on_the_end_handle_drags_that_end_and_keeps_the_start() {
    let mut lw = with_range();
    let (start_b, end_b) = primary_range(&lw).unwrap();
    let [_, end] = lw.selection_handle_geometry().unwrap();

    assert!(lw.begin_selection_handle_drag(end.center), "the press is on the handle");
    assert!(lw.selection_handle_drag_active());
    // A handle drag is NOT a click: the range survived the press.
    assert_eq!(primary_range(&lw), Some((start_b, end_b)));

    // Drag further right along the line.
    assert!(lw.process_selection_handle_drag(LogicalPosition::new(220.0, 8.0)));
    let (s2, e2) = primary_range(&lw).expect("still a range");
    assert_eq!(s2, start_b, "the start is the anchor and stayed");
    assert!(e2 > end_b, "the end followed the finger: {end_b} -> {e2}");

    // The handles moved with it.
    let [_, end_after] = lw.selection_handle_geometry().unwrap();
    assert!(end_after.center.x > end.center.x);

    assert!(lw.end_selection_handle_drag());
    assert!(!lw.selection_handle_drag_active());
    assert!(!lw.end_selection_handle_drag(), "released twice is a no-op");
}

#[test]
fn a_press_on_the_start_handle_keeps_the_end() {
    let mut lw = with_range();
    let (start_b, end_b) = primary_range(&lw).unwrap();
    let [start, _] = lw.selection_handle_geometry().unwrap();

    assert!(lw.begin_selection_handle_drag(start.center));
    assert!(lw.process_selection_handle_drag(LogicalPosition::new(60.0, 8.0)));
    let (s2, e2) = primary_range(&lw).unwrap();
    // The anchor is the END now, so the stored range runs backward from it:
    // start == old end, end == the new start. Document order is what the
    // handles are labelled by.
    assert_eq!(s2, end_b, "the end is the anchor and stayed");
    assert!(e2 > start_b && e2 < end_b, "the start moved inward: {start_b} -> {e2}");
    let [start_after, end_after] = lw.selection_handle_geometry().unwrap();
    assert!(start_after.center.x > start.center.x);
    assert!(start_after.center.x < end_after.center.x, "still labelled by document order");
}

#[test]
fn dragging_a_handle_onto_the_anchor_does_not_collapse_the_selection() {
    let mut lw = with_range();
    let [start, end] = lw.selection_handle_geometry().unwrap();
    assert!(lw.begin_selection_handle_drag(end.center));
    // Straight onto the other end: ignored, the range and its handles stay.
    let moved = lw.process_selection_handle_drag(LogicalPosition::new(start.center.x, 8.0));
    assert!(!moved);
    assert!(primary_range(&lw).is_some(), "still a range");
    assert!(lw.selection_handle_geometry().is_some(), "still two handles");
}

#[test]
fn a_press_off_the_handles_is_not_a_handle_drag() {
    let mut lw = with_range();
    let [start, end] = lw.selection_handle_geometry().unwrap();
    // Inside the selected text, between the handles' x but on the line, not
    // under it.
    let on_text = LogicalPosition::new((start.center.x + end.center.x) / 2.0, 8.0);
    assert!(lw.selection_handle_at(on_text).is_none());
    assert!(!lw.begin_selection_handle_drag(on_text));
    assert!(!lw.selection_handle_drag_active());
    assert!(!lw.process_selection_handle_drag(LogicalPosition::new(300.0, 8.0)));
}

#[test]
fn handles_off_means_nothing_to_grab() {
    // iOS and desktop: the geometry is still answerable, but nothing arms.
    let mut lw = with_range();
    lw.text_edit_manager.selection_handles = false;
    let [_, end] = lw.selection_handle_geometry().expect("geometry is platform-free");
    assert!(lw.selection_handle_at(end.center).is_none());
    assert!(!lw.begin_selection_handle_drag(end.center));
}

#[test]
fn the_display_list_paints_two_handles_only_when_enabled() {
    use azul_layout::solver3::display_list::DisplayListItem as I;
    let count_selection_rects = |lw: &mut LayoutWindow| -> usize {
        let dom_id = azul_core::dom::DomId::ROOT_ID;
        lw.regenerate_display_list_for_dom(dom_id);
        lw.layout_results
            .get(&dom_id)
            .expect("the root DOM is laid out")
            .display_list
            .items
            .iter()
            .filter(|i| matches!(i, I::SelectionRect { .. }))
            .count()
    };
    let mut lw = with_range();
    lw.text_edit_manager.selection_handles = false;
    let without = count_selection_rects(&mut lw);
    lw.text_edit_manager.selection_handles = true;
    let with = count_selection_rects(&mut lw);
    assert_eq!(
        with,
        without + 2,
        "the highlight rects plus one circle per handle ({without} -> {with})"
    );
    // A collapsed caret paints none either way.
    lw.process_mouse_click_for_selection(LogicalPosition::new(20.0, 8.0), 0);
    assert_eq!(count_selection_rects(&mut lw), 0);
}
