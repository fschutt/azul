//! A line break ends a line in the max-content width.
//!
//! CSS Sizing 3 5.1: the max-content inline size of an inline formatting
//! context is its widest line with soft wraps suppressed - a forced break
//! (`<br>`) still ends a line. The layout's inline collection emits a hard
//! break for a `<br>` (`fc::collect_and_measure_inline_content`); the
//! intrinsic-size collection (`sizing::process_layout_children`) went into
//! the `<br>` as an empty inline and emitted nothing, so the lines before
//! and after it were measured as ONE line.
//!
//! The exploration receipt (tests/mail_corpus/exploration/04_receipt.html)
//! has `<td><span>Cloud storage - 200 GB<br/><span>Monthly ...</span></span>`:
//! its column was measured at both lines side by side and took 439 of the
//! 554px, Chrome gives it 366 (mail_boxes, 2026-10-02).
//!
//! Chrome 154's numbers (probe; azul at 2e92c759b: 180, 295.1 vs 151.5, 180).

use crate::table_markup::{body, near, rect};

fn span(w: u32) -> String {
    format!("<span style=\"display: inline-block; width: {w}px; height: 10px\"></span>")
}

#[test]
fn a_cell_of_two_lines_in_a_span_is_as_wide_as_its_wider_line() {
    let lw = body(&format!(
        "<table id=\"t\" style=\"border-spacing: 0\"><tr><td style=\"padding: 0\">\
         <span>{}<br/>{}</span></td></tr></table>",
        span(100),
        span(80)
    ));
    let t = rect(&lw, "t");
    assert!(
        near(t.size.width, 100.0, 0.5),
        "max(100, 80), not 100 + 80: {}",
        t.size.width
    );
}

#[test]
fn a_cell_of_two_lines_is_as_wide_as_its_wider_line() {
    let lw = body(&format!(
        "<table id=\"t\" style=\"border-spacing: 0\"><tr><td style=\"padding: 0\">\
         {}<br/>{}</td></tr></table>",
        span(100),
        span(80)
    ));
    let t = rect(&lw, "t");
    assert!(
        near(t.size.width, 100.0, 0.5),
        "max(100, 80), not 100 + 80: {}",
        t.size.width
    );
}

#[test]
fn a_receipt_line_and_its_note_are_measured_as_two_lines() {
    let lw = body(
        "<table id=\"t\" style=\"border-spacing: 0\"><tr><td style=\"padding: 0\">\
         <span>Cloud storage 200 GB<br/><span>Monthly renews 30 Oct</span></span>\
         </td></tr></table>\
         <table id=\"l1\" style=\"border-spacing: 0\"><tr><td style=\"padding: 0\">\
         <span>Cloud storage 200 GB</span></td></tr></table>\
         <table id=\"l2\" style=\"border-spacing: 0\"><tr><td style=\"padding: 0\">\
         <span><span>Monthly renews 30 Oct</span></span></td></tr></table>",
    );
    let t = rect(&lw, "t").size.width;
    let wider = rect(&lw, "l1").size.width.max(rect(&lw, "l2").size.width);
    assert!(
        near(t, wider, 0.5),
        "the two-line cell is as wide as its wider line alone: {t} vs {wider}"
    );
}
