//! An edit in a vertical paragraph lays it out as a fresh layout of the
//! edited text does.
//!
//! A vertical paragraph (`writing-mode: vertical-rl`) runs its letters down
//! its lines: before its lines are broken, each glyph's advance is turned
//! to run along the line, by its vertical advance. A keystroke lays the
//! edited paragraph out again, and its letters have to stand where a fresh
//! window puts the edited text: on the same lines, at the same places.
//!
//! The page and the edit are those of
//! `an_edit_re_wraps_its_paragraph_as_a_fresh_layout_does`.

use azul_core::dom::{DomId, NodeId};
use azul_layout::window::LayoutWindow;

use crate::an_edit_re_wraps_its_paragraph_as_a_fresh_layout_does::{
    page, text_of, type_in_front, window, PARAGRAPH,
};

/// The paragraph's text items: (line, text, x, y), in layout order.
pub(crate) fn items_of(lw: &LayoutWindow) -> Vec<(usize, String, f32, f32)> {
    let tree = &lw.layout_results[&DomId::ROOT_ID].layout_tree;
    let layout = (0..tree.nodes.len())
        .filter(|&index| tree.nodes[index].dom_node_id == Some(NodeId::new(PARAGRAPH)))
        .find_map(|index| tree.materialized_inline_layout_for_node(index))
        .expect("the paragraph has an inline layout");
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

/// Whether two layouts put the same text on the same lines at the same
/// places.
pub(crate) fn same_layout(
    a: &[(usize, String, f32, f32)],
    b: &[(usize, String, f32, f32)],
) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            x.0 == y.0 && x.1 == y.1 && (x.2 - y.2).abs() < 0.01 && (x.3 - y.3).abs() < 0.01
        })
}

/// Asserts that typing a letter in front of `text` in a vertical paragraph
/// styled `css` lays it out as a fresh window lays out the edited text.
fn assert_an_edit_lays_out_as_a_fresh_layout(text: &str, css: &str) {
    let edited = format!("b{text}");
    let fresh = items_of(&window(page(&edited, css)));
    let horizontal = items_of(&window(page(&edited, "width: 100px; font-size: 20px;")));
    assert!(
        !same_layout(&fresh, &horizontal),
        "harness: the vertical paragraph is laid out apart from a horizontal one: {fresh:?}"
    );

    let mut lw = window(page(text, css));
    type_in_front(&mut lw, "b");
    assert_eq!(text_of(&lw), edited, "harness: the letter is typed in");
    let got = items_of(&lw);
    assert!(
        same_layout(&got, &fresh),
        "{css}: after an edit the paragraph's letters stand at {got:?}, a fresh layout of the \
         same text puts them at {fresh:?}: the edit laid the paragraph out without turning its \
         glyphs for its writing mode"
    );
}

#[test]
fn an_edit_in_an_upright_vertical_paragraph_lays_out_as_a_fresh_layout_does() {
    assert_an_edit_lays_out_as_a_fresh_layout(
        "abcdefghijkl",
        "width: 100px; font-size: 20px; writing-mode: vertical-rl; text-orientation: upright;",
    );
}

#[test]
fn an_edit_in_a_mixed_vertical_paragraph_lays_out_as_a_fresh_layout_does() {
    assert_an_edit_lays_out_as_a_fresh_layout(
        "abcdefghijkl",
        "width: 100px; font-size: 20px; writing-mode: vertical-rl; text-orientation: mixed;",
    );
}
