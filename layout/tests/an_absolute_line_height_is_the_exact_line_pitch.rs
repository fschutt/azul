//! A `line-height` is the line pitch, to the hundredth of a pixel.
//!
//! `line-height: 19px` on 11pt text laid the lines 19.55 px apart on paper.
//! A line box is the union of the text's own inline box and the strut (CSS
//! 2.2 §10.8.1), each put around the baseline by half-leading from ITS OWN
//! ascent and descent: A' = A + (line-height - A - D) / 2, D' likewise. The
//! text took A and D from the font it was shaped with; the strut took a
//! synthetic 0.8em / 0.2em split instead of its first available font's. For
//! any font whose split differs (Times: 0.891em / 0.216em) the two boxes sit
//! at different heights around the same baseline, and their union is taller
//! than the line-height by |(A - D) / 2 - 0.3em|: 0.55 px at 11pt.

use crate::table_markup::prose;
use crate::the_first_line_of_a_paragraph_starts_text_indent_further_in::paged_pens;

/// The distinct baselines of every glyph of `body` on paper, top to bottom.
pub(crate) fn baselines_of(body: &str) -> Vec<f32> {
    let mut ys: Vec<f32> = paged_pens("", body).into_iter().map(|(_, y)| y).collect();
    ys.sort_by(f32::total_cmp);
    ys.dedup_by(|a, b| (*a - *b).abs() < 1.0);
    ys
}

/// Every line of `body` lies `px` below the one before it, to the
/// hundredth of a pixel.
pub(crate) fn assert_line_pitch(body: &str, what: &str, px: f32) {
    let ys = baselines_of(body);
    assert!(
        ys.len() >= 3,
        "premise, the prose wraps into three lines or more ({what}): {ys:?}"
    );
    for pair in ys.windows(2) {
        let pitch = pair[1] - pair[0];
        assert!(
            (pitch - px).abs() < 0.02,
            "{what} puts the lines {px}px apart, not {pitch}px: {ys:?}"
        );
    }
}

/// A wrapped 11pt paragraph under `family` (empty: the UA default) and
/// `line_height` is pitched `px`.
fn assert_pitch(family: &str, line_height: &str, px: f32) {
    let family_css = if family.is_empty() {
        String::new()
    } else {
        format!(" font-family: {family};")
    };
    let body = format!(
        "<div style=\"width: 400px; font-size: 11pt; line-height: {line_height};{family_css}\">\
         <p>{}</p></div>",
        prose(60)
    );
    assert_line_pitch(
        &body,
        &format!("font-family {family:?}, line-height: {line_height}"),
        px,
    );
}

/// Every family the html2pdf / pdfocr markup names, and the UA default.
const FAMILIES: [&str; 3] = [
    "",
    "Helvetica, Arial, sans-serif",
    "'Times New Roman', serif",
];

#[test]
fn an_absolute_line_height_is_the_exact_line_pitch() {
    for family in FAMILIES {
        assert_pitch(family, "19px", 19.0);
        assert_pitch(family, "0.25in", 24.0);
    }
}

#[test]
fn a_line_height_number_is_the_exact_line_pitch() {
    // 1.5 x 11pt = 16.5pt = 22px.
    for family in FAMILIES {
        assert_pitch(family, "1.5", 22.0);
    }
}

#[test]
fn a_line_height_in_points_is_exact_to_the_hundredth() {
    // 14pt = 18.666..px: the compact style cache kept absolute line-heights
    // in tenths of a pixel (18.7px), the cascade the exact value.
    for family in FAMILIES {
        assert_pitch(family, "14pt", 14.0 * 96.0 / 72.0);
    }
}
