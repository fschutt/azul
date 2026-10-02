//! A multi-column block flows its children through its columns.
//!
//! `column-count` / `column-width` on a block whose children are blocks
//! (issue #481: `<div style="column-count: 2">` around four `<p>`s) did
//! nothing: the columns only ever reached text3 for an inline formatting
//! context ROOT (`translate_to_text3_constraints`), and `layout_bfc` had no
//! multi-column layout, so the paragraphs stacked in one full-width column.
//! They did NOT get columns of their own - `column-count` is not inherited,
//! and the issue's own numbers are those of single-column paragraphs (the
//! last test pins it).
//!
//! CSS Multi-column Layout 1: the container's content is laid out at the
//! column width and fills the columns in order, balanced (`column-fill:
//! balance`) so the columns come out as short and as equal as they can; a
//! paragraph continues at the top of the next column between two of its
//! lines, any other box moves to the next column whole; the columns sit
//! side by side `column-gap` apart, right to left under `direction: rtl`;
//! the container is as tall as its tallest column, and a container with a
//! height of its own keeps it, the content that does not fit running on in
//! further columns in the inline direction.
//!
//! Box heights are explicit wherever a number is asserted; text is only
//! used where the assertion is about lines landing in a column.
//!
//! Not compiled by the author (house rule); expected RED before the fix,
//! except the two pins (a lone inline formatting context keeps text3's own
//! split; a child sees no `column-count` of its own).

use azul_core::{dom::DomId, styled_dom::StyledNodeState};
use azul_layout::window::LayoutWindow;

use crate::table_markup::{glyph_runs, laid_out, near, node, prose, rect};

/// `body` laid out in an 800 x 600 window under `style`, every margin and
/// padding zeroed first (the container sits at the origin).
fn page(style: &str, body: &str) -> LayoutWindow {
    laid_out(
        &format!(
            "<html><head><style>* {{ margin: 0; padding: 0; }} {style}</style></head>\
             <body>{body}</body></html>"
        ),
        800.0,
        600.0,
    )
}

/// Four 40px paragraphs in the container `#c` (class `cols`).
const FOUR: &str = "<div id=\"c\" class=\"cols\"><p id=\"p1\">one</p><p id=\"p2\">two</p>\
                    <p id=\"p3\">three</p><p id=\"p4\">four</p></div>";

/// The element `id` has its border box's top-left corner at (`x`, `y`).
fn assert_at(lw: &LayoutWindow, id: &str, x: f32, y: f32) {
    let r = rect(lw, id);
    assert!(
        near(r.origin.x, x, 0.5) && near(r.origin.y, y, 0.5),
        "#{id} at ({}, {}), expected ({x}, {y})",
        r.origin.x,
        r.origin.y
    );
}

/// Every glyph pen `(x, y)` of the page.
fn pens(lw: &LayoutWindow) -> Vec<(f32, f32)> {
    glyph_runs(lw).into_iter().flatten().collect()
}

#[test]
fn four_paragraphs_fill_the_first_column_then_the_second() {
    let lw = page(
        ".cols { column-count: 2; column-gap: 20px; width: 420px } p { height: 40px }",
        FOUR,
    );
    // Two 200px columns 20px apart, balanced at 80px: two paragraphs each.
    assert_at(&lw, "p1", 0.0, 0.0);
    assert_at(&lw, "p2", 0.0, 40.0);
    assert_at(&lw, "p3", 220.0, 0.0);
    assert_at(&lw, "p4", 220.0, 40.0);
    for id in ["p1", "p2", "p3", "p4"] {
        let w = rect(&lw, id).size.width;
        assert!(near(w, 200.0, 0.5), "#{id} is a column wide: {w}");
    }
    let c = rect(&lw, "c");
    assert!(
        near(c.size.height, 80.0, 0.5),
        "the container is as tall as its tallest column: {}",
        c.size.height
    );
    assert!(
        near(c.size.width, 420.0, 0.5),
        "and keeps its width: {}",
        c.size.width
    );
}

#[test]
fn the_columns_are_the_column_gap_apart() {
    let lw = page(
        ".cols { column-count: 2; column-gap: 50px; width: 420px } p { height: 40px }",
        FOUR,
    );
    // (420 - 50) / 2 = 185px columns; the second starts at 185 + 50.
    assert_at(&lw, "p1", 0.0, 0.0);
    assert_at(&lw, "p3", 235.0, 0.0);
    let w = rect(&lw, "p1").size.width;
    assert!(near(w, 185.0, 0.5), "a 185px column: {w}");
}

#[test]
fn a_column_width_sets_how_many_columns_there_are() {
    let lw = page(
        ".cols { column-width: 100px; column-gap: 10px; width: 430px } p { height: 40px }",
        FOUR,
    );
    // floor((430 + 10) / (100 + 10)) = 4 columns of (440 / 4) - 10 = 100px:
    // one paragraph in each.
    for (k, id) in ["p1", "p2", "p3", "p4"].into_iter().enumerate() {
        assert_at(&lw, id, 110.0 * k as f32, 0.0);
        let w = rect(&lw, id).size.width;
        assert!(near(w, 100.0, 0.5), "#{id} is a 100px column wide: {w}");
    }
    let h = rect(&lw, "c").size.height;
    assert!(near(h, 40.0, 0.5), "one paragraph tall: {h}");
}

#[test]
fn a_paragraph_continues_at_the_top_of_the_next_column() {
    let lw = page(
        ".cols { column-count: 2; column-gap: 20px; width: 420px; line-height: 20px } \
         #a { height: 40px }",
        "<div id=\"c\" class=\"cols\"><div id=\"a\"></div>\
         <p id=\"b\">ab<br/>cd<br/>ef<br/>gh<br/>ij<br/>kl</p></div>",
    );
    // 40px + six 20px lines = 160px, balanced at 80px: the paragraph starts
    // under #a and its third line opens the second column.
    assert_at(&lw, "b", 0.0, 40.0);
    let pens = pens(&lw);
    let second: Vec<_> = pens.iter().filter(|(x, _)| *x >= 219.5).collect();
    assert!(
        !second.is_empty(),
        "the paragraph's later lines are in the second column: {pens:?}"
    );
    let top = second.iter().map(|(_, y)| *y).fold(f32::MAX, f32::min);
    assert!(
        top < 20.0,
        "the second column starts with a line at its top: first pen at y = {top}"
    );
    let first_col_top = pens
        .iter()
        .filter(|(x, _)| *x < 200.0)
        .map(|(_, y)| *y)
        .fold(f32::MAX, f32::min);
    assert!(
        first_col_top > 40.0,
        "the first column's lines sit below #a: first pen at y = {first_col_top}"
    );
    let bottom = pens.iter().map(|(_, y)| *y).fold(f32::MIN, f32::max);
    assert!(
        bottom < 80.0,
        "no line runs below the balanced 80px columns: last pen at y = {bottom}"
    );
    let h = rect(&lw, "c").size.height;
    assert!(h < 100.0, "the container is about one column tall: {h}");
}

#[test]
fn right_to_left_columns_start_at_the_right() {
    let lw = page(
        ".cols { column-count: 2; column-gap: 20px; width: 420px; direction: rtl } \
         p { height: 40px }",
        FOUR,
    );
    assert_at(&lw, "p1", 220.0, 0.0);
    assert_at(&lw, "p2", 220.0, 40.0);
    assert_at(&lw, "p3", 0.0, 0.0);
    assert_at(&lw, "p4", 0.0, 40.0);
}

#[test]
fn a_container_of_fixed_height_runs_on_into_further_columns() {
    let lw = page(
        ".cols { column-count: 2; column-gap: 20px; width: 420px; height: 60px } \
         p { height: 40px }",
        FOUR,
    );
    // 60px columns hold one 40px paragraph each; the content does not fit
    // two columns, so it continues in overflow columns to the right.
    assert_at(&lw, "p1", 0.0, 0.0);
    assert_at(&lw, "p2", 220.0, 0.0);
    assert_at(&lw, "p3", 440.0, 0.0);
    assert_at(&lw, "p4", 660.0, 0.0);
    let h = rect(&lw, "c").size.height;
    assert!(
        near(h, 60.0, 0.5),
        "the container keeps its own height: {h}"
    );
}

#[test]
fn a_lone_inline_context_keeps_splitting_its_lines_across_the_columns() {
    // The container holds text directly: it is the inline formatting
    // context root, and text3 splits its lines over the columns (pin).
    let lw = page(
        "",
        &format!(
            "<div id=\"c\" style=\"column-count: 2; column-gap: 20px; width: 420px\">{}</div>",
            prose(150)
        ),
    );
    let pens = pens(&lw);
    assert!(
        pens.iter().any(|(x, _)| *x >= 219.5),
        "lines in the second column: {pens:?}"
    );
    assert!(
        pens.iter().all(|(x, _)| *x < 200.5 || *x >= 219.5),
        "nothing in the gap: {pens:?}"
    );
}

#[test]
fn the_child_of_a_multicol_block_gets_no_columns_of_its_own() {
    let lw = page(
        ".cols { column-count: 2; column-gap: 20px; width: 420px }",
        &format!(
            "<div id=\"c\" class=\"cols\"><p id=\"p\">{}</p></div>",
            prose(80)
        ),
    );
    // `column-count` is not inherited: the paragraph resolves none (pin).
    let p = node(&lw, "p")
        .node
        .into_crate_internal()
        .expect("#p is a node");
    let sd = &lw
        .get_layout_result(&DomId::ROOT_ID)
        .expect("laid out")
        .styled_dom;
    assert_eq!(
        azul_layout::solver3::getters::get_column_count(sd, p, &StyledNodeState::default()),
        None,
        "the paragraph has no column-count of its own"
    );
    // It is one column wide, and its lines run the column's whole width:
    // two columns of its own would leave the band at 90..110px empty.
    let w = rect(&lw, "p").size.width;
    assert!(near(w, 200.0, 0.5), "#p is a column wide: {w}");
    let pens = pens(&lw);
    assert!(
        pens.iter().any(|(x, _)| *x > 90.0 && *x < 110.0),
        "the lines cross the middle of the column: {pens:?}"
    );
}

#[test]
fn loose_text_beside_a_block_gets_no_columns_of_its_own() {
    // Text directly in the container next to a block goes into an
    // ANONYMOUS block box - whose inline context used to read its style off
    // the container, columns included (the box has no element of its own).
    // Its lines must run the column's whole width like a paragraph's.
    let lw = page(
        ".cols { column-count: 2; column-gap: 20px; width: 420px } #a { height: 20px }",
        &format!(
            "<div id=\"c\" class=\"cols\"><div id=\"a\"></div>{}</div>",
            prose(80)
        ),
    );
    let pens = pens(&lw);
    assert!(
        pens.iter().any(|(x, _)| *x > 90.0 && *x < 110.0),
        "the loose text's lines cross the middle of the column: {pens:?}"
    );
    assert!(
        pens.iter().all(|(x, _)| *x < 200.5 || *x >= 219.5),
        "and stay out of the gap between the columns: {pens:?}"
    );
}
