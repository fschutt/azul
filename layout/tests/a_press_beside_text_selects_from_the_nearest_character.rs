//! A press beside text starts a selection at the character nearest to it.
//!
//! Chrome resolves a press to a text position through the box that was hit:
//! a press in a container's padding, in the gap below the last line, or
//! anywhere else in a box with no text right under the pointer finds the
//! nearest character of the text inside that box (`PositionForPoint`), and
//! the selection starts there. A press only started a selection when it
//! landed on a paragraph's own box, so a drag begun in the margin beside a
//! paragraph selected nothing. Text outside the box that was hit stays out
//! of reach: a press on an empty toolbar does not select the page under it.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::{LogicalPosition, LogicalSize},
    resources::RendererResources,
    selection::{CursorAffinity, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
    transform::ComputedTransform3D,
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks,
    headless::{convert_cpu_hit_test_to_full, CpuHitTester},
    managers::hover::InputPointId,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = r"
    * { margin: 0; padding: 0; }
    body { font-size: 16px; width: 600px; }
    .toolbar { display: block; height: 40px; }
    .box { display: block; padding: 40px; height: 200px; }
    p { display: block; }
";

/// The words of the paragraph: one line, far narrower than its box.
const WORDS: &str = "Some words";

/// `body(0) > [div.toolbar(1), div.box(2) > p(3) > text(4)]`.
const TOOLBAR: usize = 1;
const BOX: usize = 2;
const PARAGRAPH: usize = 3;

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

fn dnid(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

fn page() -> LayoutWindow {
    let mut dom = Dom::create_body()
        .with_child(Dom::create_div().with_ids_and_classes(class("toolbar")))
        .with_child(
            Dom::create_div()
                .with_ids_and_classes(class("box"))
                .with_child(Dom::create_p().with_child(
                    Dom::create_text_do_not_use_without_block_level_wrapper(WORDS),
                )),
        );
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    let styled = StyledDom::create(&mut dom, css);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
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
    lw
}

/// The pointer at `at`, as the shells hand it to the window. Returns the
/// nodes under it, the one painted on top first.
fn point_at(lw: &mut LayoutWindow, at: LogicalPosition) -> Vec<DomNodeId> {
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
    let under = full.hovered_node_ids();
    lw.hover_manager.push_hit_test(InputPointId::Mouse, full);
    under
}

fn rect(lw: &LayoutWindow, node: usize) -> azul_core::geom::LogicalRect {
    lw.get_node_layout_rect(dnid(node))
        .expect("the node is laid out")
}

/// Press at `at` and return the caret the press placed and the element of
/// the block it sits in.
fn press(lw: &mut LayoutWindow, at: LogicalPosition) -> Option<(DomNodeId, TextCursor)> {
    let under = point_at(lw, at);
    assert!(
        !under.contains(&dnid(PARAGRAPH)),
        "premise: the press at {at:?} is beside the paragraph, not on it: {under:?}"
    );
    lw.process_mouse_click_for_selection(at, 0)?;
    let block = lw.text_edit_manager.multi_cursor.as_ref()?.block;
    Some((
        block.container_dom_node(),
        lw.text_edit_manager.get_primary_cursor()?,
    ))
}

#[test]
fn a_press_in_the_padding_left_of_a_line_starts_the_selection_at_its_first_character() {
    let mut lw = page();
    let line = rect(&lw, PARAGRAPH);
    let left_of_it = LogicalPosition::new(rect(&lw, BOX).origin.x + 10.0, line.origin.y + 4.0);

    let (element, caret) = press(&mut lw, left_of_it).expect("the press starts a selection");

    assert_eq!(element, dnid(PARAGRAPH), "in the paragraph beside the press");
    assert_eq!(caret.cluster_id.start_byte_in_run, 0, "at its first character");
    assert_eq!(caret.affinity, CursorAffinity::Leading, "before it");
}

#[test]
fn a_press_below_the_last_line_past_its_end_starts_the_selection_after_its_last_character() {
    let mut lw = page();
    let below_the_end = LogicalPosition::new(
        rect(&lw, PARAGRAPH).origin.x + 400.0,
        rect(&lw, BOX).origin.y + 200.0,
    );

    let (element, caret) = press(&mut lw, below_the_end).expect("the press starts a selection");

    assert_eq!(element, dnid(PARAGRAPH), "in the paragraph above the press");
    assert_eq!(
        caret.cluster_id.start_byte_in_run,
        (WORDS.len() - 1) as u32,
        "on its last character"
    );
    assert_eq!(caret.affinity, CursorAffinity::Trailing, "after it");
}

#[test]
fn a_press_on_a_box_without_text_selects_nothing_outside_it() {
    let mut lw = page();
    let toolbar = rect(&lw, TOOLBAR);
    let on_it = LogicalPosition::new(toolbar.origin.x + 20.0, toolbar.origin.y + 20.0);

    assert!(
        press(&mut lw, on_it).is_none(),
        "the toolbar holds no text, and the paragraph below it is not inside it"
    );
}
