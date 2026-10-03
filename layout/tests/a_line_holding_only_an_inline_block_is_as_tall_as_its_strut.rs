//! A line holding only an inline-block is as tall as CSS 2.1 s10.8 makes it:
//! every line box starts with a STRUT - a zero-width inline box with the block
//! container's font and line-height - so a 10px box in a 16px Arial block
//! makes an 18px line, as in Chrome (user ruling 2026-10-03: "Chrome is the
//! reference").
//!
//! Three causes, all pinned here:
//! - text3 measured the inline formatting context by its ITEMS, and counted a
//!   line box only for a line holding no item with a height (a lone `<br>`):
//!   the line around a 10px box measured 10px;
//! - the strut's face was loaded only when some TEXT used the block's font, so
//!   a block holding only boxes took a 0.8em / 0.2em guess and `normal` = 1em;
//! - `vertical-align: middle` moved the box DOWN by half the x-height (it is
//!   raised: CSS 2.1 s10.8.1 "the baseline of the parent box plus half the
//!   x-height"), and the line box ignored every shifted box's extent (and the
//!   strut joined it only after the top / bottom pass).
//!
//! Every number is Chrome 154's (headless, `body { margin: 0 }`, Arial). The
//! ones that depend on the font's ascent and descent have a 1px tolerance
//! (Arial and Liberation Sans share their metrics); the ones the CSS fixes -
//! a line-height, a box taller than the strut - are exact.
//! Not compiled by the author (house rule); RED before the fix.

use crate::table_markup::{body, near, rect};

/// `<div id="p">` in 16px Arial (plus `css`) holding `content`, then a
/// `<div id="t">` with one line of text in the same font: the face the strut
/// takes is loaded, and `t` is a line of text to compare with.
fn with_text_line(css: &str, content: &str) -> azul_layout::window::LayoutWindow {
    body(&format!(
        "<div id=\"p\" style=\"font-family: Arial; font-size: 16px; {css}\">{content}</div>\
         <div id=\"t\" style=\"font-family: Arial; font-size: 16px; {css}\">x</div>"
    ))
}

/// A `size` px square inline-block with `css`.
fn square(id: &str, size: u32, css: &str) -> String {
    format!(
        "<span id=\"{id}\" style=\"display: inline-block; width: {size}px; height: {size}px; \
         {css}\"></span>"
    )
}

#[test]
fn a_line_holding_only_a_small_inline_block_is_as_tall_as_a_line_of_text() {
    let lw = with_text_line("", &square("b", 10, ""));
    let (p, t, b) = (rect(&lw, "p"), rect(&lw, "t"), rect(&lw, "b"));
    assert!(
        near(p.size.height, t.size.height, 0.5),
        "the line around the 10px box is as tall as a line of text in its font (Chrome: 18 and \
         18): {p:?} vs {t:?}"
    );
    assert!(near(p.size.height, 18.0, 1.0), "Chrome: 18px: {p:?}");
    assert!(
        near(b.origin.y - p.origin.y, 4.0, 1.0),
        "the box sits on the baseline, 4px down in Chrome: {b:?} in {p:?}"
    );
}

#[test]
fn a_line_of_only_inline_blocks_is_as_tall_as_its_line_height() {
    let lw = with_text_line(
        "line-height: 20px",
        &format!("{}{}", square("b", 10, ""), square("c", 10, "")),
    );
    let (p, b, c) = (rect(&lw, "p"), rect(&lw, "b"), rect(&lw, "c"));
    assert!(near(p.size.height, 20.0, 0.5), "Chrome: 20px: {p:?}");
    assert!(
        near(b.origin.y - p.origin.y, 5.0, 1.0),
        "Chrome: 5px down: {b:?} in {p:?}"
    );
    assert!(near(c.origin.y, b.origin.y, 0.1), "both on one baseline: {b:?} {c:?}");
}

#[test]
fn a_line_height_of_zero_makes_the_line_as_tall_as_its_box() {
    // The CSS an icon uses to keep the height of its box alone.
    let lw = with_text_line("line-height: 0", &square("b", 10, ""));
    let (p, b) = (rect(&lw, "p"), rect(&lw, "b"));
    assert!(near(p.size.height, 10.0, 0.5), "Chrome: 10px: {p:?}");
    assert!(near(b.origin.y, p.origin.y, 0.5), "Chrome: at the top: {b:?} in {p:?}");
}

#[test]
fn a_box_taller_than_the_strut_extends_the_line_by_the_struts_descent() {
    let lw = with_text_line("", &square("b", 40, ""));
    let (p, b) = (rect(&lw, "p"), rect(&lw, "b"));
    assert!(near(p.size.height, 44.0, 1.0), "Chrome: 44px: {p:?}");
    assert!(near(b.origin.y, p.origin.y, 0.5), "Chrome: at the top: {b:?} in {p:?}");
}

#[test]
fn a_box_aligned_top_sits_at_the_top_of_a_strut_tall_line() {
    let lw = with_text_line("", &square("b", 10, "vertical-align: top"));
    let (p, t, b) = (rect(&lw, "p"), rect(&lw, "t"), rect(&lw, "b"));
    assert!(near(p.size.height, t.size.height, 0.5), "Chrome: 18 and 18: {p:?} {t:?}");
    assert!(near(b.origin.y, p.origin.y, 0.5), "Chrome: at the top: {b:?} in {p:?}");
}

#[test]
fn a_box_aligned_middle_is_raised_to_the_middle_of_the_x_height() {
    // 10px box: its middle at the baseline (14) minus half Arial's x-height
    // (4.15): top 4.84 in Chrome. Moving it DOWN by the half x-height put it
    // at 11.8, half outside its own 10px line.
    let lw = with_text_line("", &square("b", 10, "vertical-align: middle"));
    let (p, b) = (rect(&lw, "p"), rect(&lw, "b"));
    assert!(near(p.size.height, 18.0, 1.0), "Chrome: 18px: {p:?}");
    assert!(
        near(b.origin.y - p.origin.y, 4.84, 1.0),
        "Chrome: 4.84px down: {b:?} in {p:?}"
    );
}

#[test]
fn an_icon_aligned_middle_and_taller_than_the_strut_makes_the_line_its_height() {
    // The icon case: a 24px box, `vertical-align: middle`, in 16px text -
    // the line is exactly the icon (Chrome: 24, the icon at its top). The
    // line box must hold the box where the alignment PUTS it.
    let lw = with_text_line("", &square("b", 24, "vertical-align: middle"));
    let (p, b) = (rect(&lw, "p"), rect(&lw, "b"));
    assert!(near(p.size.height, 24.0, 0.5), "Chrome: 24px: {p:?}");
    assert!(near(b.origin.y, p.origin.y, 0.5), "Chrome: at the top: {b:?} in {p:?}");

    let lw = with_text_line("", &format!("x{}", square("b", 24, "vertical-align: middle")));
    let (p, b) = (rect(&lw, "p"), rect(&lw, "b"));
    assert!(
        near(p.size.height, 24.0, 0.5),
        "beside text too (Chrome: 24): {p:?}"
    );
    assert!(near(b.origin.y, p.origin.y, 0.5), "Chrome: at the top: {b:?} in {p:?}");
}

#[test]
fn a_box_aligned_bottom_shares_the_line_with_the_strut() {
    // Top / bottom boxes are aligned AFTER the line box holds everything
    // else - the strut included: a 24px box is taller than the 18px strut,
    // so the line is 24 and the box fills it.
    let lw = with_text_line("", &square("b", 24, "vertical-align: bottom"));
    let (p, b) = (rect(&lw, "p"), rect(&lw, "b"));
    assert!(near(p.size.height, 24.0, 0.5), "Chrome: 24px: {p:?}");
    assert!(near(b.origin.y, p.origin.y, 0.5), "Chrome: at the top: {b:?} in {p:?}");
}

#[test]
fn a_box_lowered_by_a_length_deepens_the_line() {
    // `vertical-align: -5px`: the box's bottom 5px below the baseline (14):
    // the line is 19px, the box at 9 in Chrome.
    let lw = with_text_line("", &square("b", 10, "vertical-align: -5px"));
    let (p, b) = (rect(&lw, "p"), rect(&lw, "b"));
    assert!(near(p.size.height, 19.0, 1.0), "Chrome: 19px: {p:?}");
    assert!(
        near(b.origin.y - p.origin.y, 9.0, 1.0),
        "Chrome: 9px down: {b:?} in {p:?}"
    );
}

#[test]
fn the_strut_takes_its_fonts_metrics_when_no_text_uses_that_font() {
    // No text anywhere in the document: the block's own font must still be
    // loaded for its strut (CSS 2.1 s10.8.1: the strut has the A and D of the
    // block's first available font) - not a 0.8em / 0.2em guess with
    // `normal` taken as 1em (16px).
    let lw = body(&format!(
        "<div id=\"p\" style=\"font-family: Arial; font-size: 16px\">{}</div>",
        square("b", 10, "")
    ));
    let (p, b) = (rect(&lw, "p"), rect(&lw, "b"));
    assert!(near(p.size.height, 18.0, 1.0), "Chrome: 18px: {p:?}");
    assert!(
        near(b.origin.y - p.origin.y, 4.0, 1.0),
        "Chrome: 4px down: {b:?} in {p:?}"
    );
}
