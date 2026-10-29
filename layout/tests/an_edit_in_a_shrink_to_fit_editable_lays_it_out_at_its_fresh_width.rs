//! An edit in a shrink-to-fit editable lays its text out at the width a
//! fresh layout gives the box.
//!
//! An inline-block or an absolutely positioned box with an `auto` width is
//! as wide as its content (shrink-to-fit, CSS 2.2 s10.3.5 / s10.3.7). A
//! word typed into such an editable widens its content, and a fresh layout
//! of the edited text widens the box with it: the edit has to lay the text
//! out at that width, not wrap it in the box's old one.

use azul_core::{
    dom::{Dom, DomId, DomNodeId, NodeId},
    selection::{CursorAffinity, GraphemeClusterId, TextCursor},
    styled_dom::NodeHierarchyItemId,
};
use azul_layout::window::LayoutWindow;

use crate::{
    an_edit_in_a_vertical_paragraph_lays_out_as_a_fresh_layout_does::same_layout,
    an_edit_re_wraps_its_paragraph_as_a_fresh_layout_does::window,
};

/// The editable: body(0) > div(1) > text(2), editable(3) > text(4).
const EDITABLE: usize = 3;

/// The page with `text` in an editable styled `css`, behind a label in a
/// 300 px wide paragraph that is the containing block of what it holds.
fn page(text: &str, css: &str) -> Dom {
    Dom::create_body().with_css("margin: 0px;").with_child(
        Dom::create_div()
            .with_css("width: 300px; font-size: 20px; position: relative;")
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(
                "Name: ",
            ))
            .with_child(
                Dom::create_span()
                    .with_contenteditable(true)
                    .with_css(css)
                    .with_child(Dom::create_text_do_not_use_without_block_level_wrapper(text)),
            ),
    )
}

/// The editable's letters: line, text and where its inline layout puts them.
fn items_of(lw: &LayoutWindow) -> Vec<(usize, String, f32, f32)> {
    let tree = &lw.layout_results[&DomId::ROOT_ID].layout_tree;
    let layout = (0..tree.nodes.len())
        .filter(|&index| tree.nodes[index].dom_node_id == Some(NodeId::new(EDITABLE)))
        .find_map(|index| tree.materialized_inline_layout_for_node(index))
        .expect("the editable has an inline layout");
    layout
        .items
        .iter()
        .filter_map(|item| {
            let cluster = item.item.as_cluster()?;
            Some((
                item.line_index,
                cluster.text().to_string(),
                item.position.x,
                item.position.y,
            ))
        })
        .collect()
}

/// Types `text` in front of the editable's text.
fn type_in_front(lw: &mut LayoutWindow, text: &str) {
    lw.focus_manager.set_focused_node(Some(DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(EDITABLE))),
    }));
    let at_start = TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: 0,
        },
        affinity: CursorAffinity::Leading,
    };
    lw.start_editing_at(at_start, DomId::ROOT_ID, NodeId::new(EDITABLE), 0);
    let _ = lw.record_text_input(text);
    let _ = lw.apply_text_changeset();
}

/// Asserts that typing a word in front of "Bob" in an editable styled
/// `css` lays it out on the one line a fresh layout puts it on.
fn assert_a_typed_word_lays_out_as_a_fresh_layout(css: &str) {
    let fresh = items_of(&window(page("Al Bob", css)));
    assert!(
        !fresh.is_empty() && fresh.iter().all(|item| item.0 == 0),
        "harness: the box grows to hold the edited text on one line: {fresh:?}"
    );

    let mut lw = window(page("Bob", css));
    type_in_front(&mut lw, "Al ");
    let got = items_of(&lw);
    assert!(
        same_layout(&got, &fresh),
        "{css}: after typing a word the editable's letters stand at {got:?}, a fresh layout of \
         the same text puts them at {fresh:?}: the edit wrapped the text in the box's old width"
    );
}

#[test]
fn a_word_typed_into_an_inline_block_editable_does_not_wrap() {
    assert_a_typed_word_lays_out_as_a_fresh_layout("display: inline-block;");
}

#[test]
fn a_word_typed_into_an_absolutely_positioned_editable_does_not_wrap() {
    assert_a_typed_word_lays_out_as_a_fresh_layout(
        "display: block; position: absolute; left: 0px; top: 40px;",
    );
}
