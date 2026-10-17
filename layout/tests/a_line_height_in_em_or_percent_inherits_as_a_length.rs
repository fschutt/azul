//! `line-height` keeps its unit until it is computed (CSS 2.2 §10.8.1, CSS
//! Inline 3 §4.2): a `<number>` is inherited as the number - every
//! descendant multiplies its OWN font size - but a `<length>` or a
//! `<percentage>` computes to an absolute length, and THAT is what the
//! descendants inherit. `em` is a length.
//!
//! `StyleLineHeight` kept every value in one `PercentageValue` (positive: a
//! factor of the font size, negative: absolute px), so `2em` and `200%` were
//! stored as the number 2 and a child with a bigger font got a bigger line
//! pitch, and `rem` and the viewport units could not be stored at all: the
//! declaration was dropped and the lines came out at `normal`.

use crate::an_absolute_line_height_is_the_exact_line_pitch::assert_line_pitch;
use crate::table_markup::prose;

/// A 400px wide container declaring `line_height` at a 10px font size,
/// around a paragraph at 20px that declares none.
fn nested(line_height: &str) -> String {
    format!(
        "<div style=\"width: 400px; font-size: 10px; line-height: {line_height}\">\
         <p style=\"font-size: 20px\">{}</p></div>",
        prose(60)
    )
}

/// An 11pt paragraph in a 400px wide container declaring `line_height`.
fn plain(line_height: &str) -> String {
    format!(
        "<div style=\"width: 400px; font-size: 11pt; line-height: {line_height}\">\
         <p>{}</p></div>",
        prose(60)
    )
}

#[test]
fn a_line_height_in_em_inherits_as_the_length_it_computes_to() {
    // 2em of the container's 10px is 20px, whatever the paragraph's font size.
    assert_line_pitch(
        &nested("2em"),
        "line-height: 2em at 10px, inherited by a 20px paragraph",
        20.0,
    );
}

#[test]
fn a_line_height_in_percent_inherits_as_the_length_it_computes_to() {
    assert_line_pitch(
        &nested("200%"),
        "line-height: 200% at 10px, inherited by a 20px paragraph",
        20.0,
    );
}

#[test]
fn a_line_height_number_inherits_as_the_number() {
    // 2 x the paragraph's own 20px.
    assert_line_pitch(
        &nested("2"),
        "line-height: 2 at 10px, inherited by a 20px paragraph",
        40.0,
    );
}

#[test]
fn a_line_height_in_em_on_the_element_itself_uses_its_own_font_size() {
    let body = format!(
        "<div style=\"width: 400px\"><p style=\"font-size: 20px; line-height: 1.5em\">{}</p></div>",
        prose(60)
    );
    assert_line_pitch(&body, "line-height: 1.5em at 20px", 30.0);
}

#[test]
fn a_line_height_in_rem_sets_the_line_pitch() {
    // The root font size is the UA's 16px.
    assert_line_pitch(&plain("2rem"), "line-height: 2rem", 32.0);
}

#[test]
fn a_line_height_in_viewport_units_sets_the_line_pitch() {
    // The page is 800 x 600.
    for (value, px) in [("5vh", 30.0), ("4vw", 32.0), ("5vmin", 30.0), ("4vmax", 32.0)] {
        assert_line_pitch(&plain(value), &format!("line-height: {value}"), px);
    }
}
