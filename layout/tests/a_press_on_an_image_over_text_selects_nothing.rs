//! A press starts a text selection only in the box painted on top under the
//! pointer, as a browser's does: a drawing surface laid over text (AzReview's
//! ink over the code of a page) takes the press, and the text under it stays
//! unselected.
//!
//! The press walked EVERY node under the pointer, deepest first, and selected
//! the first text it met, the covered text included; when that walk found
//! nothing, a geometric search over every text block selected it anyway. A
//! pen stroke over a page of code dragged a selection through the code, and
//! every move of the stroke extended it.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::{LogicalPosition, LogicalSize},
    resources::RendererResources,
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

const CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { font-size: 14px; width: 600px; }
    .page { display: block; position: relative; width: 400px; height: 120px; }
    .code { display: block; }
    .ink { position: absolute; top: 0px; left: 0px; width: 400px; height: 120px; }
"#;

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

fn code() -> Dom {
    Dom::create_div()
        .with_ids_and_classes(class("code"))
        .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
            "pub fn main() { println!(\"hello\"); }",
        ))
}

/// `body(0) > div.page(1) > [div.code(2) > text(3), div.ink(4)]`, or
/// without the ink `body(0) > div.page(1) > div.code(2) > text(3)`.
fn page(with_ink: bool) -> LayoutWindow {
    let mut page = Dom::create_div()
        .with_ids_and_classes(class("page"))
        .with_child(code());
    if with_ink {
        page = page.with_child(Dom::create_div().with_ids_and_classes(class("ink")));
    }
    let mut dom = Dom::create_body().with_child(page);
    let (css, _) = azul_css::parser2::new_from_str(CSS);
    let styled_dom = StyledDom::create(&mut dom, css);
    let mut lw = LayoutWindow::new(FcFontCache::build()).unwrap();
    let mut ws = FullWindowState::default();
    ws.size.dimensions = LogicalSize::new(800.0, 600.0);
    lw.current_window_state = ws.clone();
    let rr = RendererResources::default();
    let sc = ExternalSystemCallbacks::rust_internal();
    let mut dbg = Some(Vec::new());
    lw.layout_and_generate_display_list(styled_dom, &ws, &rr, &sc, &mut dbg)
        .unwrap();
    lw
}

fn dnid(n: usize) -> DomNodeId {
    DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(n))),
    }
}

/// A point on the code's first characters.
fn on_the_code(lw: &LayoutWindow) -> LogicalPosition {
    let r = lw
        .get_node_layout_rect(dnid(2))
        .expect("the code is laid out");
    LogicalPosition::new(r.origin.x + 4.0, r.origin.y + r.size.height * 0.5)
}

/// The pointer at `at`, as the shells hand it to the window: the hit test
/// the hover manager keeps for the mouse. Returns the nodes under it, the
/// one painted on top first.
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

#[test]
fn a_press_on_text_nothing_covers_places_the_caret_in_it() {
    let mut lw = page(false);
    let at = on_the_code(&lw);
    point_at(&mut lw, at);

    assert!(
        lw.process_mouse_click_for_selection(at, 0).is_some(),
        "a press on uncovered text starts a selection in it"
    );
}

#[test]
fn a_press_on_a_surface_laid_over_text_selects_nothing_under_it() {
    let mut lw = page(true);
    let at = on_the_code(&lw);
    let under = point_at(&mut lw, at);
    assert_eq!(
        under.first(),
        Some(&dnid(4)),
        "premise: the ink is painted on top of the code at {at:?}: {under:?}"
    );

    let placed = lw.process_mouse_click_for_selection(at, 0);

    assert!(
        placed.is_none(),
        "the press belongs to the ink, not to the code under it: {placed:?}"
    );
    assert!(
        lw.text_edit_manager.get_cross_block_selection().is_none(),
        "no selection runs through the covered code"
    );
}
