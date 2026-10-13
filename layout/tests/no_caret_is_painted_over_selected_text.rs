//! An editable shows its caret only while nothing is selected.
//!
//! Chrome hides the caret of an editing host as soon as the selection is a
//! range, and so does every native text field. A drag selection in an
//! editable is a range whose focus end follows the pointer, and a drag that
//! leaves its paragraph for the next one is a selection across two blocks;
//! neither may paint a caret. The caret the press placed must not stay
//! behind either: across two paragraphs it stood in the middle of the
//! highlight.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    geom::{LogicalPosition, LogicalSize},
    resources::RendererResources,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    transform::ComputedTransform3D,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    headless::{convert_cpu_hit_test_to_full, CpuHitTester},
    managers::hover::InputPointId,
    solver3::display_list::DisplayListItem,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = "* { margin: 0; padding: 0; } body { font-size: 16px; width: 500px; } \
                   p { display: block; }";

/// `body(0) > div[contenteditable](1) > [p(2) > text(3), p(4) > text(5)]`.
const HOST: usize = 1;
const FIRST: usize = 2;
const SECOND: usize = 4;

fn dnid(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

/// The editable laid out and focused, as a click into it leaves it.
fn editable() -> LayoutWindow {
    let mut host = Dom::create_div()
        .with_child(Dom::create_p().with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper("First paragraph of words"),
        ))
        .with_child(Dom::create_p().with_child(
            Dom::create_text_do_not_use_without_block_level_wrapper("Second paragraph of words"),
        ));
    host.set_contenteditable(true);
    let mut dom = Dom::create_body().with_child(host);
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    let styled = StyledDom::create(&mut dom, css);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(600.0, 400.0);
    lw.current_window_state = ws.clone();
    let mut dbg = None;
    lw.layout_and_generate_display_list(
        styled,
        &ws,
        &RendererResources::default(),
        &ExternalSystemCallbacks::rust_internal(),
        &mut dbg,
    )
    .unwrap();
    lw.focus_manager.set_focused_node(Some(dnid(HOST)));
    lw
}

/// The pointer at `at`, as the shells hand it to the window.
fn point_at(lw: &mut LayoutWindow, at: LogicalPosition) {
    let mut tester = CpuHitTester::new();
    tester.rebuild_from_layout(&lw.layout_results);
    let no_scroll = |_: DomId, _: NodeId| -> Option<LogicalPosition> { None };
    let no_transform = |_: DomId, _: NodeId| -> Option<ComputedTransform3D> { None };
    let hits = tester.hit_test_scrolled(at, &no_scroll, &no_transform);
    let full = convert_cpu_hit_test_to_full(
        &tester,
        &hits,
        None,
        &lw.layout_results,
        at,
        &no_scroll,
        &no_transform,
    );
    lw.hover_manager.push_hit_test(InputPointId::Mouse, full);
}

/// A point `fraction` of the way across paragraph `node`, on its line.
fn across(lw: &LayoutWindow, node: usize, fraction: f32) -> LogicalPosition {
    let r = lw
        .get_node_layout_rect(dnid(node))
        .expect("the paragraph is laid out");
    LogicalPosition::new(
        r.origin.x + r.size.width * fraction,
        r.origin.y + r.size.height * 0.5,
    )
}

fn press(lw: &mut LayoutWindow, at: LogicalPosition) {
    point_at(lw, at);
    assert!(
        lw.process_mouse_click_for_selection(at, 0).is_some(),
        "premise: the press at {at:?} lands in the editable's text"
    );
}

fn drag(lw: &mut LayoutWindow, from: LogicalPosition, to: LogicalPosition) {
    point_at(lw, to);
    assert!(
        lw.process_mouse_drag_for_selection(from, to).is_some(),
        "premise: the drag to {to:?} extends the selection"
    );
}

/// The carets the display list paints, visible or in their blink-off phase.
fn carets(lw: &mut LayoutWindow) -> usize {
    lw.regenerate_display_list_for_dom(DomId::ROOT_ID);
    lw.get_layout_result(&DomId::ROOT_ID)
        .expect("laid out")
        .display_list
        .items
        .iter()
        .filter(|item| matches!(item, DisplayListItem::CursorRect { .. }))
        .count()
}

#[test]
fn a_press_in_an_editable_paints_its_caret() {
    let mut lw = editable();
    let at = across(&lw, FIRST, 0.05);
    press(&mut lw, at);

    assert_eq!(carets(&mut lw), 1, "a collapsed selection is a caret");
}

#[test]
fn a_drag_selection_in_an_editable_paints_no_caret() {
    let mut lw = editable();
    let from = across(&lw, FIRST, 0.05);
    press(&mut lw, from);
    let to = across(&lw, FIRST, 0.6);
    drag(&mut lw, from, to);

    assert_eq!(
        carets(&mut lw),
        0,
        "a selected range hides the caret, as in Chrome and every native text field"
    );
}

#[test]
fn a_drag_selection_across_two_paragraphs_paints_no_caret_inside_the_highlight() {
    let mut lw = editable();
    let from = across(&lw, FIRST, 0.05);
    press(&mut lw, from);
    let inside = across(&lw, FIRST, 0.5);
    drag(&mut lw, from, inside);
    let next = across(&lw, SECOND, 0.5);
    drag(&mut lw, inside, next);
    assert!(
        lw.text_edit_manager.get_cross_block_selection().is_some(),
        "premise: the drag selects across both paragraphs"
    );

    assert_eq!(
        carets(&mut lw),
        0,
        "the caret the drag left in the first paragraph stands in the middle of the highlight"
    );
}

#[test]
fn a_drag_back_to_the_press_point_paints_the_caret_again() {
    let mut lw = editable();
    let from = across(&lw, FIRST, 0.05);
    press(&mut lw, from);
    let to = across(&lw, FIRST, 0.6);
    drag(&mut lw, from, to);
    drag(&mut lw, to, from);

    assert_eq!(
        carets(&mut lw),
        1,
        "a selection collapsed back to the press point is a caret again"
    );
}
