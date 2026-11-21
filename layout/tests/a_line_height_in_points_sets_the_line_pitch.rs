//! A `line-height` in any absolute unit sets the line pitch.
//!
//! `parse_style_line_height` took numbers, percentages and `px` only: the
//! `line-height: 14pt` pdfocr's html2pdf writes on every text region failed
//! to parse, the declaration was dropped and the lines came out at `normal`
//! (16.9px for 11pt Helvetica instead of 18.67px).

use crate::table_markup::prose;
use crate::the_first_line_of_a_paragraph_starts_text_indent_further_in::paged_pens;

/// The distance between the first two lines' pens, on paper.
fn pitch(line_height: &str) -> f32 {
    let body = format!(
        "<div style=\"width: 400px; font-size: 11pt; line-height: {line_height}\"><p>{}</p></div>",
        prose(60)
    );
    let mut ys: Vec<f32> = paged_pens("", &body).into_iter().map(|(_, y)| y).collect();
    ys.sort_by(f32::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < 1.0);
    assert!(ys.len() >= 2, "premise, the prose wraps: {ys:?}");
    ys[1] - ys[0]
}

#[test]
fn a_line_height_in_points_sets_the_line_pitch() {
    for (value, px) in [("14pt", 14.0 * 96.0 / 72.0), ("0.25in", 24.0), ("6mm", 6.0 * 96.0 / 25.4)] {
        let got = pitch(value);
        assert!((got - px).abs() <= 1.0, "line-height: {value} is {px}px apart: {got}px");
    }
}
