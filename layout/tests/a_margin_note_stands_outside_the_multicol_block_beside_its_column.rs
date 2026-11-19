//! A margin note stands outside a multi-column container, on the side of
//! the column its anchor is in.
//!
//! An extension of this engine, no CSS: an absolutely positioned box with
//! all insets `auto` and `--azul-margin-note: outside` whose static
//! position is inside a multi-column block container. Its right edge is
//! `--azul-margin-note-gap` (12px by default) left of the container for an
//! anchor in the left column, its left edge that far right of the container
//! for one in the right column; it stays on the line of its anchor; a later
//! note on the same side stands below an earlier one it would cover.
//!
//! The container is 420px wide, 100px from the left: two 200px columns
//! 20px apart. Its paragraph has four lines of one fixed-size inline block
//! each: two a column.

use crate::table_markup::{laid_out, near, rect};

fn word(id: &str) -> String {
    format!("<i id=\"{id}\" style=\"display: inline-block; width: 150px; height: 10px\"></i>")
}

/// The paragraph `lines` (each ended by a `<br/>` but the last) in the
/// container, the note class `.mn` styled by `note_css`.
fn page(note_css: &str, lines: &[String]) -> azul_layout::window::LayoutWindow {
    laid_out(
        &format!(
            "<html><head><style>* {{ margin: 0; padding: 0; }} \
             .cols {{ column-count: 2; column-gap: 20px; width: 420px; margin-left: 100px; \
             line-height: 20px }} \
             .mn {{ position: absolute; width: 50px; height: 10px; {note_css} }}</style></head>\
             <body><div id=\"c\" class=\"cols\"><p>{}</p></div></body></html>",
            lines.join("<br/>")
        ),
        800.0,
        600.0,
    )
}

const OUTSIDE: &str = "--azul-margin-note: outside;";

fn note(id: &str) -> String {
    format!("<span id=\"{id}\" class=\"mn\"></span>")
}

#[test]
fn a_note_anchored_in_the_left_column_stands_left_of_the_container() {
    let lw = page(
        OUTSIDE,
        &[word("w1"), word("w2") + &note("a"), word("w3"), word("w4")],
    );
    let (c, w2, a) = (rect(&lw, "c"), rect(&lw, "w2"), rect(&lw, "a"));
    assert!(near(c.origin.x, 100.0, 0.5) && near(w2.origin.x, 100.0, 0.5), "{c:?} {w2:?}");
    assert!(
        near(a.origin.x + a.size.width, 88.0, 0.5),
        "the note's right edge is 12px left of the container: {a:?}"
    );
    assert!(near(a.origin.y, w2.origin.y, 0.5), "on the line of its anchor {w2:?}: {a:?}");
}

#[test]
fn a_note_anchored_in_the_right_column_stands_right_of_the_container() {
    let lw = page(
        OUTSIDE,
        &[word("w1"), word("w2"), word("w3"), word("w4") + &note("a")],
    );
    let (w4, a) = (rect(&lw, "w4"), rect(&lw, "a"));
    assert!(near(w4.origin.x, 320.0, 0.5), "the fourth line is in the right column: {w4:?}");
    assert!(
        near(a.origin.x, 532.0, 0.5),
        "the note's left edge is 12px right of the container (520): {a:?}"
    );
    assert!(near(a.origin.y, w4.origin.y, 0.5), "on the line of its anchor {w4:?}: {a:?}");
}

#[test]
fn the_gap_is_what_the_property_says() {
    let lw = page(
        "--azul-margin-note: outside; --azul-margin-note-gap: 30px;",
        &[word("w1") + &note("a"), word("w2"), word("w3"), word("w4")],
    );
    let a = rect(&lw, "a");
    assert!(near(a.origin.x + a.size.width, 70.0, 0.5), "{a:?}");
}

#[test]
fn two_notes_on_one_side_do_not_overlap() {
    // Lines 20px apart, notes 30px tall: the second would cover the first.
    let lw = page(
        "--azul-margin-note: outside; height: 30px;",
        &[
            word("w1") + &note("a"),
            word("w2") + &note("b"),
            word("w3") + &note("r"),
            word("w4"),
        ],
    );
    let (w1, w2, w3) = (rect(&lw, "w1"), rect(&lw, "w2"), rect(&lw, "w3"));
    let (a, b, r) = (rect(&lw, "a"), rect(&lw, "b"), rect(&lw, "r"));
    assert!(near(a.origin.y, w1.origin.y, 0.5), "{a:?}");
    assert!(w2.origin.y < a.origin.y + 30.0, "the second anchor is beside the first note");
    assert!(
        near(b.origin.y, a.origin.y + 30.0, 0.5),
        "the second note stands below the first: {a:?} {b:?}"
    );
    // The right column's note is on the other side: on its own line.
    assert!(near(r.origin.y, w3.origin.y, 0.5) && r.origin.x > 500.0, "{r:?}");
}

#[test]
fn without_the_property_an_absolute_box_stays_at_its_static_position() {
    let lw = page("", &[word("w1"), word("w2") + &note("a"), word("w3"), word("w4")]);
    let (w2, a) = (rect(&lw, "w2"), rect(&lw, "a"));
    assert!(
        near(a.origin.x, w2.origin.x + 150.0, 0.5) && near(a.origin.y, w2.origin.y, 0.5),
        "the box stands after its anchor {w2:?}: {a:?}"
    );
}

#[test]
fn outside_a_multicol_block_the_property_does_nothing() {
    let lw = laid_out(
        &format!(
            "<html><head><style>* {{ margin: 0; padding: 0; }} \
             p {{ margin-left: 100px; line-height: 20px }} \
             .mn {{ position: absolute; width: 50px; height: 10px; {OUTSIDE} }}</style></head>\
             <body><p>{}{}</p></body></html>",
            word("w1"),
            note("a")
        ),
        800.0,
        600.0,
    );
    let (w1, a) = (rect(&lw, "w1"), rect(&lw, "a"));
    assert!(near(a.origin.x, w1.origin.x + 150.0, 0.5), "{w1:?} {a:?}");
}

#[test]
fn on_a_page_a_margin_note_is_drawn_beside_the_container_at_its_anchor() {
    // The page slicer cuts the canvas in the block direction only: a note
    // left of a container at the page's left edge is drawn at a negative x,
    // in the page's margin.
    use crate::a_footnote_is_drawn_at_the_bottom_of_the_page_of_its_anchor::{pages, rects};
    let (result, _, _) = pages(&format!(
        "<html><head><style>* {{ margin: 0; padding: 0; }} \
         .cols {{ column-count: 2; column-gap: 20px; width: 420px; line-height: 20px }} \
         .mn {{ position: absolute; width: 50px; height: 10px; background: #ff0000; {OUTSIDE} }}\
         </style></head><body><div class=\"cols\"><p>{}<br/>{}{}<br/>{}<br/>{}</p></div>\
         </body></html>",
        word("w1"),
        word("w2"),
        note("a"),
        word("w3"),
        word("w4")
    ));
    assert_eq!(result.pages.len(), 1);
    let drawn = rects(&result.pages[0], (255, 0, 0));
    assert_eq!(drawn.len(), 1, "{drawn:?}");
    let (top, bottom, left, width) = drawn[0];
    assert_eq!((left, width), (-62.0, 50.0), "12px left of the container at x 0");
    assert!((20.0..40.0).contains(&top) && bottom - top == 10.0, "on the second line: {drawn:?}");
}

#[test]
fn a_note_anchored_inside_a_span_stands_beside_the_column_of_its_line() {
    // The anchor is written inside an inline box, in the fourth line (the
    // right column): its static position is on that line all the same.
    let lw = page(
        OUTSIDE,
        &[
            word("w1"),
            word("w2"),
            word("w3"),
            format!("<span>{}{}</span>", word("w4"), note("a")),
        ],
    );
    let (w4, a) = (rect(&lw, "w4"), rect(&lw, "a"));
    assert!(near(w4.origin.x, 320.0, 0.5), "the fourth line is in the right column: {w4:?}");
    assert!(near(a.origin.x, 532.0, 0.5), "right of the container: {a:?}");
    assert!(near(a.origin.y, w4.origin.y, 0.5), "on the line of its anchor {w4:?}: {a:?}");
}
