//! Typing and Backspace in an editable list item act on the item's text.
//!
//! A list item's inline layout starts with its `::marker`: `solver3::fc`
//! pushes the marker into the item's content first, so the text is run 1 and
//! every caret the layout mints there (a click, an arrow key) says run 1. The
//! edit model - `get_text_before_textinput`, the content overlay - is the
//! DOM's text alone, where the same text is run 0. `insert_text` found no
//! run 1 and did nothing: the keystroke vanished (and a debug build tripped
//! the "insert missed every selection" assertion in
//! `apply_one_text_changeset`). Backspace likewise.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, IdOrClass, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::RendererResources,
    selection::{CursorAffinity, GraphemeClusterId, TextCursor},
    styled_dom::{NodeHierarchyItemId, StyledDom},
};
use azul_layout::{
    callbacks::ExternalSystemCallbacks, text3::cache::ShapedItem, window::LayoutWindow,
    window_state::FullWindowState,
};
use rust_fontconfig::FcFontCache;

/// `body(0) > div.host[contenteditable](1) > div.li(2) > text(3) "alpha"`
const HOST: usize = 1;
const ITEM: usize = 2;

fn list_item_editor() -> LayoutWindow {
    const CSS: &str = r#"
        * { margin: 0; padding: 0; }
        body { font-size: 14px; width: 600px; }
        .host { display: block; }
        .li { display: list-item; }
    "#;
    let class =
        |name: &str| -> azul_core::dom::IdOrClassVec { vec![IdOrClass::Class(name.into())].into() };
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(class("host"))
            .with_contenteditable(true)
            .with_child(
                Dom::create_div()
                    .with_ids_and_classes(class("li"))
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                        "alpha",
                    )),
            ),
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

fn rect_of(lw: &LayoutWindow, n: usize) -> LogicalRect {
    lw.get_node_layout_rect(dnid(n))
        .unwrap_or_else(|| panic!("node {n} is laid out"))
}

fn text_of(lw: &LayoutWindow, n: usize) -> String {
    let content = lw.get_text_before_textinput(DomId::ROOT_ID, NodeId::new(n));
    lw.extract_text_from_inline_content(&content)
}

/// The item's layout still draws its marker: a shaped cluster that belongs
/// to no DOM text node (generated content).
fn marker_is_shaped(lw: &LayoutWindow) -> bool {
    lw.materialized_inline_layout_for_node(DomId::ROOT_ID, NodeId::new(ITEM))
        .is_some_and(|layout| {
            layout.items.iter().any(|item| {
                matches!(&item.item, ShapedItem::Cluster(c) if c.source_node_id.is_none())
            })
        })
}

/// A click inside the item, to the right of "alpha": the click path puts the
/// session on the item with the layout's caret after the last 'a'.
fn click_after_the_text(lw: &mut LayoutWindow) {
    let item = rect_of(lw, ITEM);
    let point = LogicalPosition::new(
        item.origin.x + 200.0,
        item.origin.y + item.size.height * 0.5,
    );
    lw.process_mouse_click_for_selection(point, 0)
        .expect("the click lands in the list item");
    lw.focus_manager.set_focused_node(Some(dnid(HOST)));
    assert_eq!(
        lw.text_edit_manager.get_editing_node_id(),
        Some(NodeId::new(ITEM)),
        "premise: the click opened the session on the list item"
    );
    let caret = lw
        .text_edit_manager
        .get_primary_cursor()
        .expect("premise: the click placed a caret");
    assert_eq!(
        caret.cluster_id.source_run, 1,
        "premise: the marker is run 0 of the item's layout, so its text is run 1"
    );
}

fn type_text(lw: &mut LayoutWindow, text: &str) {
    let _ = lw.record_text_input(text);
    let _ = lw.apply_text_changeset();
}

#[test]
fn typing_after_a_click_in_a_list_item_lands_in_its_text() {
    let mut lw = list_item_editor();
    assert!(
        marker_is_shaped(&lw),
        "premise: the marker is part of the item's inline layout"
    );
    click_after_the_text(&mut lw);

    type_text(&mut lw, "x");
    assert_eq!(text_of(&lw, ITEM), "alphax");
    type_text(&mut lw, "y");
    assert_eq!(
        text_of(&lw, ITEM),
        "alphaxy",
        "the caret followed the first keystroke"
    );
    assert!(
        marker_is_shaped(&lw),
        "the edited item is still drawn with its marker"
    );
}

#[test]
fn backspace_after_a_click_in_a_list_item_deletes_from_its_text() {
    let mut lw = list_item_editor();
    click_after_the_text(&mut lw);

    lw.delete_selection(dnid(HOST), false)
        .expect("backspace after 'alpha' deletes its last 'a'");
    assert_eq!(text_of(&lw, ITEM), "alph");
    assert!(
        marker_is_shaped(&lw),
        "the edited item is still drawn with its marker"
    );
}

/// Backspace before the item's first character has no character of the
/// item's own to delete. The marker is generated content, not text: it must
/// not be what goes. (Passes before the fix too - nothing is edited there at
/// all - and pins what the fix must not break.)
#[test]
fn backspace_before_the_first_character_leaves_the_marker_alone() {
    let mut lw = list_item_editor();
    click_after_the_text(&mut lw);
    lw.text_edit_manager
        .multi_cursor
        .as_mut()
        .expect("the click opened a session")
        .set_single_cursor(TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 1,
                start_byte_in_run: 0,
            },
            affinity: CursorAffinity::Leading,
        });

    let _ = lw.delete_selection(dnid(HOST), false);
    assert_eq!(text_of(&lw, ITEM), "alpha");
    assert!(marker_is_shaped(&lw), "the marker survives");
}
