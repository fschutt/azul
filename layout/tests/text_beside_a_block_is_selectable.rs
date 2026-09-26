//! Text that shares its container with a block - "Item" in
//! `li > ["Item", ul > li("sub")]` - can be clicked, dragged into, painted
//! and copied like any other text.
//!
//! CSS wraps such a run of inline content in an ANONYMOUS block box, and that
//! box owns the run's inline layout. It has no DOM node of its own, and every
//! selection path used to demand one: the click's fallback scan and the drag
//! target search skipped every IFC root without a `dom_node_id`, the
//! document-order walk dropped it, and the painter returned before painting
//! it. So a click on "Item" placed no caret, a drag could not end in it, and a
//! selection that ran across it neither highlighted nor copied it.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
    selection::{CursorAffinity, GraphemeClusterId, TextBlock, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, solver3::display_list::DisplayListItem,
    window::LayoutWindow, window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

const CSS: &str = r#"
    * { margin: 0; padding: 0; }
    body { font-size: 14px; width: 600px; }
    .block { display: block; }
"#;

/// `body(0) > [div.block(1) > text(2) "one",
///             div.block(3) > [text(4) "Item", div.block(5) > text(6) "sub"]]`
const ONE: usize = 2;
const ITEM_CONTAINER: usize = 3;
const ITEM: usize = 4;
const SUB: usize = 6;

fn class(name: &str) -> azul_core::dom::IdOrClassVec {
    vec![IdOrClass::Class(name.into())].into()
}

fn text(s: &str) -> Dom {
    Dom::create_text_do_not_use_without_block_level_wrapper(s)
}

fn block() -> Dom {
    Dom::create_div().with_ids_and_classes(class("block"))
}

fn document() -> LayoutWindow {
    let mut dom = Dom::create_body()
        .with_child(block().with_child(text("one")))
        .with_child(
            block()
                .with_child(text("Item"))
                .with_child(block().with_child(text("sub"))),
        );
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

fn block_of(lw: &LayoutWindow, n: usize) -> TextBlock {
    lw.text_block_of(dnid(n))
        .unwrap_or_else(|| panic!("node {n} is in a text block"))
}

fn rect_of(lw: &LayoutWindow, n: usize) -> LogicalRect {
    lw.get_node_layout_rect(dnid(n))
        .unwrap_or_else(|| panic!("node {n} is laid out"))
}

/// A point on "Item": the first line of its container, whose box starts where
/// the anonymous block does.
fn on_item(lw: &LayoutWindow) -> LogicalPosition {
    let r = rect_of(lw, ITEM_CONTAINER);
    LogicalPosition::new(r.origin.x + 4.0, r.origin.y + 6.0)
}

fn start() -> TextCursor {
    TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: 0,
        },
        affinity: CursorAffinity::Leading,
    }
}

/// The distinct vertical bands the selection highlight covers.
fn highlighted_lines(lw: &LayoutWindow) -> usize {
    let mut ys: Vec<f32> = lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("layout result")
        .display_list
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayListItem::SelectionRect { bounds, .. } => Some(bounds.origin().y),
            _ => None,
        })
        .collect();
    ys.sort_by(f32::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < 2.0);
    ys.len()
}

#[test]
fn premise_item_is_an_anonymous_block() {
    let lw = document();
    assert!(
        block_of(&lw, ITEM).is_anonymous(),
        "\"Item\" shares its container with a block: CSS wraps it in an anonymous block"
    );
}

#[test]
fn a_click_on_text_beside_a_block_places_a_caret_in_it() {
    let mut lw = document();
    let point = on_item(&lw);

    lw.process_mouse_click_for_selection(point, 0)
        .expect("the click lands on \"Item\"");

    assert_eq!(
        lw.text_edit_manager.get_editing_block(),
        Some(block_of(&lw, ITEM)),
        "the caret is in the anonymous block around \"Item\""
    );
}

#[test]
fn a_drag_can_end_in_text_beside_a_block() {
    let mut lw = document();
    let one = rect_of(&lw, 1);
    let from = LogicalPosition::new(one.origin.x + 1.0, one.origin.y + 6.0);
    lw.process_mouse_click_for_selection(from, 0)
        .expect("premise: the press lands on \"one\"");

    lw.process_mouse_drag_for_selection(from, on_item(&lw))
        .expect("the drag is handled");

    let focus = lw
        .text_edit_manager
        .get_cross_block_selection()
        .map(|sel| sel.focus.block);
    assert_eq!(
        focus,
        Some(block_of(&lw, ITEM)),
        "the drag's far end is in \"Item\""
    );
}

#[test]
fn a_selection_across_text_beside_a_block_paints_and_copies_it() {
    let mut lw = document();
    let one = block_of(&lw, ONE);
    let sub = block_of(&lw, SUB);
    let end_of_sub = lw
        .text_target(sub)
        .and_then(|t| t.last_caret())
        .expect("premise: \"sub\" has a last caret");
    assert!(lw.set_cross_block_selection(one, start(), sub, end_of_sub));
    lw.regenerate_display_list_for_dom(DomId::ROOT_ID);

    let copied = lw
        .get_selected_content_for_clipboard(&DomId::ROOT_ID)
        .map(|c| c.plain_text.as_str().to_string());
    assert_eq!(copied.as_deref(), Some("one\nItem\nsub"));
    assert_eq!(
        highlighted_lines(&lw),
        3,
        "each of the three lines is highlighted"
    );
}
