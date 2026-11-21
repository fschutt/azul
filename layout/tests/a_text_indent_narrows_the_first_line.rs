//! `text-indent` narrows the first line: it is a margin on the line box's start edge, so the
//! line holds `width - indent` of text (CSS Text 3 §8.1).
//!
//! Reported 2026-10-02 by the PDF OCR work: the greedy breaker fills the first line to the
//! full width and the indent then shifts it, so the line ends `indent` past the right edge
//! (text off the page with `text-align: justify`). Only the Knuth-Plass path
//! (`text-wrap: balance`) takes the indent off the first line's width.

use azul_layout::window::LayoutWindow;

use crate::table_markup::{glyph_runs, laid_out, prose, rect, right};

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

/// The leftmost and rightmost glyph pen of each line, top to bottom.
fn line_extents(lw: &LayoutWindow) -> Vec<(f32, f32)> {
    let mut lines: Vec<(f32, f32, f32)> = Vec::new(); // (y, min x, max x)
    for (x, y) in glyph_runs(lw).into_iter().flatten() {
        match lines.iter_mut().find(|(ly, ..)| (*ly - y).abs() < 2.0) {
            Some(line) => {
                line.1 = line.1.min(x);
                line.2 = line.2.max(x);
            }
            None => lines.push((y, x, x)),
        }
    }
    lines.sort_by(|a, b| a.0.total_cmp(&b.0));
    lines.into_iter().map(|(_, a, b)| (a, b)).collect()
}

fn assert_first_line_fits(align: &str, indent: &str, indent_px: f32) {
    let lw = page(
        &format!("p {{ width: 600px; font-size: 16px; text-align: {align}; text-indent: {indent} }}"),
        &format!("<p>{}</p>", prose(80)),
    );
    let lines = line_extents(&lw);
    assert!(lines.len() >= 3, "{align}: premise, the prose wraps: {lines:?}");
    assert!((lines[0].0 - indent_px).abs() <= 1.0, "{align}: premise, the first line is indented: {lines:?}");
    // the last glyph's pen, so its right edge is a glyph further: < 600 is lenient
    assert!(
        lines[0].1 < 600.0,
        "{align}, text-indent {indent}: the first line's last glyph starts at {:.1}px, past the 600px \
         paragraph (by about the indent): {lines:?}",
        lines[0].1
    );
}

#[test]
fn a_left_aligned_first_line_ends_inside_the_paragraph() {
    assert_first_line_fits("left", "120px", 120.0);
}

#[test]
fn a_justified_first_line_ends_inside_the_paragraph() {
    assert_first_line_fits("justify", "120px", 120.0);
}

#[test]
fn an_indent_in_points_narrows_the_first_line_too() {
    // 126.8pt, as on the OCR pages
    assert_first_line_fits("justify", "126.8pt", 126.8 * 4.0 / 3.0);
}

// ==== TEXT7: the rest of CSS Text 3 s8.1 ====
//
// The indent is a margin on the START edge of the line box: the line box is narrower
// (wider, for a negative indent) by the indent, so the breaker fills and `justify`
// spreads over what is left; only the lines `text-indent` picks get it (the first
// formatted line; with `each-line` every line after a forced break too; `hanging`
// inverts the choice); a right-to-left line's start edge is its right one; and a
// shrink-to-fit box makes room for it (it counts in the intrinsic sizes).

/// A glyph's advance at 16px is below this: the slack between the last pens of two
/// lines that both end at the same edge.
const ONE_GLYPH: f32 = 14.0;

/// The lines of a 600px, 16px paragraph styled `p_style` and holding `body`.
fn paragraph(p_style: &str, body: &str) -> Vec<(f32, f32)> {
    line_extents(&page(
        &format!("p {{ width: 600px; font-size: 16px; {p_style} }}"),
        &format!("<p>{body}</p>"),
    ))
}

#[test]
fn a_justified_first_line_ends_flush_with_the_lines_below() {
    let lines = paragraph("text-align: justify; text-indent: 120px", &prose(80));
    assert!(lines.len() >= 3, "premise, the prose wraps: {lines:?}");
    assert!(
        (lines[0].1 - lines[1].1).abs() <= ONE_GLYPH,
        "justify spreads the first line over the 480px the indent leaves, so it ends where \
         the second line ends: {lines:?}"
    );
}

#[test]
fn a_negative_indent_widens_the_first_line() {
    // The first line starts 120px left of the others, in the paragraph's margin, and is
    // 120px wider: justified, it ends where they end.
    let lines = paragraph(
        "margin-left: 120px; text-align: justify; text-indent: -120px",
        &prose(80),
    );
    assert!(lines.len() >= 3, "premise, the prose wraps: {lines:?}");
    assert!(lines[0].0.abs() <= 1.0, "premise, the first line hangs into the margin: {lines:?}");
    assert!(
        lines[1..].iter().all(|l| (l.0 - 120.0).abs() <= 1.0),
        "premise, the other lines start at the paragraph's edge: {lines:?}"
    );
    assert!(
        (lines[0].1 - lines[1].1).abs() <= ONE_GLYPH,
        "the 720px first line ends where the 600px lines end: {lines:?}"
    );
}

#[test]
fn a_hanging_indent_narrows_every_line_but_the_first() {
    let lines = paragraph("text-indent: 120px hanging", &prose(80));
    assert!(lines.len() >= 3, "premise, the prose wraps: {lines:?}");
    assert!(lines[0].0.abs() <= 1.0, "the first line starts at the edge: {lines:?}");
    assert!(
        lines[1..].iter().all(|l| (l.0 - 120.0).abs() <= 1.0),
        "every other line starts 120px in: {lines:?}"
    );
    assert!(
        lines.iter().all(|l| l.1 < 600.0),
        "every line ends inside the paragraph: {lines:?}"
    );
}

#[test]
fn only_the_first_line_is_indented_after_a_forced_break() {
    let lines = paragraph("text-indent: 120px", &format!("{}<br/>{}", prose(40), prose(40)));
    assert!(lines.len() >= 4, "premise, both halves wrap: {lines:?}");
    assert!((lines[0].0 - 120.0).abs() <= 1.0, "the first line is indented: {lines:?}");
    assert_eq!(
        lines.iter().filter(|l| (l.0 - 120.0).abs() <= 1.0).count(),
        1,
        "the line after the <br/> is not: {lines:?}"
    );
    assert!(
        lines.iter().all(|l| l.1 < 600.0),
        "every line ends inside the paragraph: {lines:?}"
    );
}

#[test]
fn each_line_indents_and_narrows_the_line_after_a_forced_break_too() {
    let lines = paragraph(
        "text-indent: 120px each-line",
        &format!("{}<br/>{}", prose(40), prose(40)),
    );
    assert!(lines.len() >= 4, "premise, both halves wrap: {lines:?}");
    assert!((lines[0].0 - 120.0).abs() <= 1.0, "the first line is indented: {lines:?}");
    assert_eq!(
        lines.iter().filter(|l| (l.0 - 120.0).abs() <= 1.0).count(),
        2,
        "so is the line after the <br/>: {lines:?}"
    );
    assert!(
        lines.iter().all(|l| l.1 < 600.0),
        "every line ends inside the paragraph: {lines:?}"
    );
}

#[test]
fn a_right_to_left_first_line_is_indented_from_its_right_edge() {
    let lines = paragraph(
        "direction: rtl; text-align: right; text-indent: 120px",
        &prose(80),
    );
    assert!(lines.len() >= 3, "premise, the prose wraps: {lines:?}");
    assert!(lines[1].1 > 480.0, "premise, the other lines reach the right edge: {lines:?}");
    // the last glyph's pen: its right edge is a glyph further, so < 480 is lenient
    assert!(
        lines[0].1 < 480.0,
        "the first line ends 120px in from the right (start) edge: {lines:?}"
    );
    assert!(lines[0].0 >= -1.0, "and starts inside the paragraph: {lines:?}");
}

#[test]
fn a_shrink_to_fit_box_makes_room_for_its_indent() {
    let lw = page(
        "#f { float: left; font-size: 16px; text-indent: 50px }",
        "<div id=\"f\">lorem ipsum dolor</div>",
    );
    let lines = line_extents(&lw);
    let f = rect(&lw, "f");
    assert_eq!(lines.len(), 1, "the text stays on one line: {lines:?}");
    assert!((lines[0].0 - 50.0).abs() <= 1.0, "premise, it is indented: {lines:?}");
    assert!(
        lines[0].1 < right(&f),
        "the float's max-content counts the indent: its last glyph starts at {:.1}px, the \
         float ends at {:.1}px",
        lines[0].1,
        right(&f)
    );
}

#[test]
fn a_min_content_box_makes_room_for_its_indent_on_the_first_word() {
    let lw = page(
        "#m { width: min-content; font-size: 16px; text-indent: 100px }",
        "<div id=\"m\">lorem ipsum</div>",
    );
    let lines = line_extents(&lw);
    let m = rect(&lw, "m");
    assert_eq!(lines.len(), 2, "one word per line: {lines:?}");
    assert!((lines[0].0 - 100.0).abs() <= 1.0, "premise, the first word is indented: {lines:?}");
    assert!(
        lines[0].1 < right(&m),
        "the min-content is the indent plus the first word: its last glyph starts at {:.1}px, \
         the box ends at {:.1}px",
        lines[0].1,
        right(&m)
    );
}
