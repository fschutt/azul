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


// ─── Cross-block selections (U2-a-i) ────────────────────────────────────

/// Three paragraphs, 14px, in an 800x600 window. Node layout: body=0,
/// div1=1, text=2, div2=3, text=4, div3=5, text=6 - the `cross_block_selection`
/// fixture, so the two suites agree on what a block is.
fn three_paragraphs() -> LayoutWindow {
    let css_src = "* { margin: 0; padding: 0; } \
                   body { font-size: 14px; width: 600px; } \
                   .p { display: block; }";
    let class: azul_core::dom::IdOrClassVec = vec![IdOrClass::Class("p".into())].into();
    let para = |text: &str| {
        Dom::create_div()
            .with_ids_and_classes(class.clone())
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(text))
    };
    let mut dom = Dom::create_body()
        .with_child(para("first paragraph"))
        .with_child(para("second paragraph"))
        .with_child(para("third paragraph"));
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
    lw.text_edit_manager.selection_handles = true;
    lw
}

const P1: usize = 1;
const P2: usize = 3;
const P3: usize = 5;

fn text_cursor(byte: u32) -> azul_core::selection::TextCursor {
    azul_core::selection::TextCursor {
        cluster_id: azul_core::selection::GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: byte,
        },
        affinity: azul_core::selection::CursorAffinity::Leading,
    }
}

/// "first |paragraph" .. "third| paragraph", as a drag from P1 makes it.
fn spanning_p1_to_p3() -> LayoutWindow {
    let mut lw = three_paragraphs();
    // The session sits in P1 the way a press there would leave it.
    lw.process_mouse_click_for_selection(LogicalPosition::new(2.0, 6.0), 0);
    assert!(lw.set_cross_block_selection(
        azul_core::dom::DomId::ROOT_ID,
        azul_core::dom::NodeId::new(P1),
        text_cursor(6),
        azul_core::dom::NodeId::new(P3),
        text_cursor(5),
    ));
    lw
}

fn spanned_blocks(lw: &LayoutWindow) -> Vec<usize> {
    lw.text_edit_manager
        .cross_block
        .as_ref()
        .map(|s| s.affected_nodes.keys().map(|n| n.index()).collect())
        .unwrap_or_default()
}

#[test]
fn a_cross_block_selection_has_a_handle_under_each_blocks_end() {
    let lw = spanning_p1_to_p3();
    let [start, end] = lw
        .selection_handle_geometry()
        .expect("handles for a selection that spans blocks");
    assert_eq!(start.end, SelectionHandleEnd::Start);
    assert_eq!(end.end, SelectionHandleEnd::End);
    // Two lines apart: the start hangs under P1's line, the end under P3's.
    assert!(
        end.center.y - start.center.y > 20.0,
        "ends on different lines: {start:?} {end:?}"
    );
    // The start handle sits at "first |paragraph", well right of x=0; the end
    // at "third| paragraph".
    assert!(start.center.x > 20.0, "{start:?}");
    assert!(end.center.x > 20.0, "{end:?}");
}

#[test]
fn dragging_the_end_handle_of_a_cross_block_selection_moves_the_far_end() {
    let mut lw = spanning_p1_to_p3();
    let [start, end] = lw.selection_handle_geometry().unwrap();
    assert!(lw.begin_selection_handle_drag(end.center));
    assert!(lw.selection_handle_drag_active());
    // The painted selection survives the press (no first move yet).
    assert_eq!(spanned_blocks(&lw), vec![P1, P2, P3]);

    // Into the MIDDLE paragraph: the line right under P1's.
    let p2_middle = LogicalPosition::new(30.0, start.center.y - SELECTION_HANDLE_RADIUS + 8.0);
    assert!(lw.process_selection_handle_drag(p2_middle));
    assert_eq!(spanned_blocks(&lw), vec![P1, P2], "the far end moved from P3 to P2");
    let sel = lw.text_edit_manager.cross_block.as_ref().unwrap();
    assert_eq!(sel.anchor.ifc_root_node_id.index(), P1, "the start is the anchor and stayed");
    assert_eq!(sel.anchor.cursor.cluster_id.start_byte_in_run, 6);
    assert!(sel.is_forward);
    // The handles followed.
    let [_, end_after] = lw.selection_handle_geometry().unwrap();
    assert!(end_after.center.y < end.center.y, "the end handle rose to P2's line");
    assert!(lw.end_selection_handle_drag());
}

#[test]
fn dragging_the_start_handle_of_a_cross_block_selection_re_anchors_at_the_end() {
    // THE HARD HALF: the session lives in P1 (where the press that made the
    // selection was), but the START handle keeps the END fixed - so the
    // session has to move to P3 first, or the mouse-drag machinery would
    // extend from P1 and the end handle would drop.
    let mut lw = spanning_p1_to_p3();
    let [start, end] = lw.selection_handle_geometry().unwrap();
    assert!(lw.begin_selection_handle_drag(start.center));
    assert_eq!(
        lw.text_edit_manager.get_editing_node_id().map(|n| n.index()),
        Some(P3),
        "the session re-anchored at the end's block"
    );
    assert_eq!(spanned_blocks(&lw), vec![P1, P2, P3], "still painted until the first move");

    let p2_middle = LogicalPosition::new(30.0, end.center.y - SELECTION_HANDLE_RADIUS - 8.0 - 17.0);
    assert!(lw.process_selection_handle_drag(p2_middle));
    assert_eq!(spanned_blocks(&lw), vec![P2, P3], "the near end moved from P1 to P2");
    let sel = lw.text_edit_manager.cross_block.as_ref().unwrap();
    assert_eq!(sel.anchor.ifc_root_node_id.index(), P3, "the end is the anchor now");
    assert_eq!(sel.anchor.cursor.cluster_id.start_byte_in_run, 5, "and kept its place");
    assert!(!sel.is_forward, "anchor after focus in document order");
    let [start_after, end_after] = lw.selection_handle_geometry().unwrap();
    assert!(start_after.center.y > start.center.y, "the start handle dropped to P2's line");
    assert!((end_after.center.y - end.center.y).abs() < 0.5, "the end handle did not move");
}

#[test]
fn a_cross_block_handle_dragged_back_into_the_anchor_block_becomes_a_single_block_range() {
    let mut lw = spanning_p1_to_p3();
    let [start, end] = lw.selection_handle_geometry().unwrap();
    assert!(lw.begin_selection_handle_drag(end.center));
    // Back into P1, right of the start.
    let p1_line = LogicalPosition::new(start.center.x + 30.0, 6.0);
    assert!(lw.process_selection_handle_drag(p1_line));
    assert!(lw.text_edit_manager.cross_block.is_none(), "collapsed to one block");
    let (s, e) = primary_range(&lw).expect("a single-block range");
    assert_eq!(s, 6, "anchor kept");
    assert!(e > 6);
    assert!(lw.selection_handle_geometry().is_some(), "and it still has handles");
}
