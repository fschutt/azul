//! An absolutely positioned child does not split its parent's line, and a
//! list item's marker rides the item's first line (LAYOUT7 item 1; WRITER6
//! "Seen broken" N1: every check item of AzNotes / AzWriter / AzMail compose
//! drawn on two lines).
//!
//! CSS 2.2 s9.2.1.1: only IN-FLOW block-level boxes make a block container
//! wrap its inline content in anonymous blocks; an absolutely positioned box
//! is out of flow (s9.7 blockifies it, it does not join the flow). CSS Lists 3
//! s3.1: the `::marker` sits on the list item's first line box, and a marker
//! with no content (`list-style-type: none`, no image) generates no box.
//!
//! Measured in headless Chrome 154 against the prebuilt engine at 2e55eef06
//! (16px Arial, `line-height: 20px`; Chrome / azul):
//! - `<div>one<div abs/> two</div>`: 20 / 40 tall;
//! - the check item (`li`, list-style none, position relative, text then an
//!   abspos box): 20 / 40;
//! - `<li>Item<div>block</div></li>`: 40 / 60 - the marker took a line of its
//!   own, laid out as an inline formatting context of the `li`'s DOM node,
//!   which collected the item's loose text a second time;
//! - `<li><p>a</p><p>b</p></li>`: 40 / 60; `<li><div><p>a</p></div></li>`:
//!   20 / 40.
//!
//! Not compiled by the author (house rule); RED before the fix.

use azul_core::dom::DomId;
use azul_layout::solver3::layout_tree::PseudoElement;

use crate::table_markup::{glyph_runs, laid_out, near, node, rect};

fn page(body: &str) -> azul_layout::window::LayoutWindow {
    laid_out(
        &format!(
            "<html><head></head><body style=\"margin: 0; font-size: 16px; line-height: \
             20px\">{body}</body></html>"
        ),
        800.0,
        600.0,
    )
}

#[test]
fn an_absolutely_positioned_child_does_not_split_its_parents_line() {
    let lw = page(
        "<div id=\"line\" style=\"position: relative\">one<div id=\"abs\" style=\"position: \
         absolute; left: 0px; top: 0px; width: 10px; height: 10px\"></div> two</div>\
         <p id=\"after\" style=\"margin: 0\">x</p>",
    );
    let line = rect(&lw, "line");
    assert!(
        near(line.size.height, 20.0, 0.5),
        "\"one\" and \"two\" share one line box: the out-of-flow box between them is no block \
         of the flow (Chrome 20px): {line:?}"
    );
    let after = rect(&lw, "after");
    assert!(
        near(after.origin.y, 20.0, 0.5),
        "the next block follows one line down: {after:?}"
    );
    let abs = rect(&lw, "abs");
    assert!(
        near(abs.origin.x, 0.0, 0.5) && near(abs.origin.y, 0.0, 0.5),
        "the positioned box still sits at its offsets: {abs:?}"
    );
}

#[test]
fn a_check_item_with_an_absolutely_positioned_box_is_one_line_tall() {
    // The RichTextEditor's check item (layout/src/widgets/rich_text_editor.rs).
    let lw = page(
        "<div style=\"width: 300px\"><li id=\"item\" style=\"display: list-item; \
         list-style-type: none; position: relative; padding-left: 28px\">text<div id=\"box\" \
         style=\"position: absolute; left: 2px; top: 0px; width: 20px; height: \
         20px\"></div></li><p id=\"after\" style=\"margin: 0\">x</p></div>",
    );
    let item = rect(&lw, "item");
    assert!(
        near(item.size.height, 20.0, 0.5),
        "the check item is one line tall (Chrome 20px), not a marker line plus a text line: \
         {item:?}"
    );
    let check_box = rect(&lw, "box");
    assert!(
        near(check_box.origin.x, 2.0, 0.5) && near(check_box.origin.y, 0.0, 0.5),
        "the box sits at its offsets in the item: {check_box:?}"
    );
    let after = rect(&lw, "after");
    assert!(
        near(after.origin.y, 20.0, 0.5),
        "the next block follows one line down: {after:?}"
    );
}

#[test]
fn a_list_item_with_list_style_type_none_has_no_marker_box() {
    let lw = page(
        "<ul style=\"margin: 0; padding: 0\"><li id=\"none\" style=\"list-style-type: \
         none\">text</li><li id=\"disc\">text</li></ul>",
    );
    let result = lw.get_layout_result(&DomId::ROOT_ID).expect("laid out");
    let tree = &result.layout_tree;
    let markers_of = |id: &str| {
        let dom = node(&lw, id)
            .node
            .into_crate_internal()
            .expect("a DOM node");
        tree.dom_to_layout
            .get(&dom)
            .map(|boxes| {
                boxes
                    .iter()
                    .filter(|&&b| {
                        tree.warm(b)
                            .is_some_and(|w| w.pseudo_element == Some(PseudoElement::Marker))
                    })
                    .count()
            })
            .unwrap_or(0)
    };
    assert_eq!(
        markers_of("none"),
        0,
        "list-style-type: none leaves the marker without content: no ::marker box (CSS Lists \
         3 s3.1)"
    );
    assert_eq!(markers_of("disc"), 1, "a disc item keeps its marker");
}

#[test]
fn a_list_items_marker_rides_its_first_line_not_a_line_of_its_own() {
    let lw = page(
        "<ul style=\"margin: 0; padding-left: 40px\"><li id=\"mixed\">Item<div \
         id=\"block\">block</div></li></ul>\
         <ul style=\"margin: 0; padding-left: 40px\"><li id=\"paras\"><p id=\"p1\" \
         style=\"margin: 0\">a</p><p id=\"p2\" style=\"margin: 0\">b</p></li></ul>\
         <ul style=\"margin: 0; padding-left: 40px\"><li id=\"nested\"><div><p id=\"deep\" \
         style=\"margin: 0\">a</p></div></li></ul>",
    );
    let mixed = rect(&lw, "mixed");
    assert!(
        near(mixed.size.height, 40.0, 0.5),
        "loose text then a block: two lines (Chrome 40px), the marker on the first: {mixed:?}"
    );
    let block = rect(&lw, "block");
    assert!(
        near(block.origin.y - mixed.origin.y, 20.0, 0.5),
        "the block is the second line: {block:?}"
    );
    let paras = rect(&lw, "paras");
    assert!(
        near(paras.size.height, 40.0, 0.5),
        "two paragraphs: two lines (Chrome 40px), one marker: {paras:?}"
    );
    let p1 = rect(&lw, "p1");
    assert!(
        near(p1.origin.y, paras.origin.y, 0.5),
        "the first paragraph starts the item: {p1:?} in {paras:?}"
    );
    let nested = rect(&lw, "nested");
    assert!(
        near(nested.size.height, 20.0, 0.5),
        "the marker rides the first line of a nested block too (Chrome 20px): {nested:?}"
    );
    let deep = rect(&lw, "deep");
    assert!(
        near(deep.origin.y, nested.origin.y, 0.5),
        "the paragraph starts the item: {deep:?} in {nested:?}"
    );
}

#[test]
fn a_list_items_text_is_painted_once() {
    // No marker glyphs (list-style-type: none): every glyph is the text's.
    let lw = page(
        "<ul style=\"margin: 0; padding: 0; list-style-type: none\"><li>Item<div>block</div>\
         </li></ul>",
    );
    let glyphs: usize = glyph_runs(&lw).iter().map(Vec::len).sum();
    assert_eq!(
        glyphs, 9,
        "\"Item\" and \"block\" are painted once each (4 + 5 glyphs), not \"Item\" again on a \
         marker line"
    );
}

#[test]
fn the_marker_sits_on_the_first_lines_baseline() {
    let lw = page("<ul style=\"margin: 0; padding-left: 40px\"><li>Item<div>block</div></li></ul>");
    let runs = glyph_runs(&lw);
    let marker_y: Vec<f32> = runs
        .iter()
        .flatten()
        .filter(|(x, _)| *x < 39.5)
        .map(|(_, y)| *y)
        .collect();
    assert!(
        !marker_y.is_empty(),
        "the outside marker hangs left of the item's content edge: {runs:?}"
    );
    let first_baseline = runs
        .iter()
        .flatten()
        .filter(|(x, _)| *x >= 39.5)
        .map(|(_, y)| *y)
        .fold(f32::MAX, f32::min);
    for y in marker_y {
        assert!(
            near(y, first_baseline, 0.5),
            "the marker is on the baseline of the item's first line ({first_baseline}), not on \
             a line of its own: {y} ({runs:?})"
        );
    }
}
