//! Regression: the CPU renderer cut glyph ink at the text's line box.
//!
//! A text item's `clip_rect` was the content box of its inline formatting
//! context, and on an axis whose overflow is `visible` it was widened for ink
//! horizontally only - its vertical extent doubled as the "lines" that
//! pagination must not split (azul#478), so it had to stay tight. The renderer
//! clips each run to that rect, which meant that wherever the ink is taller
//! than the line, the ink was sliced off: the descenders of `g j p q y` under
//! `line-height: 1` (the descent of most faces lies below a 1em line), and
//! most of the text under a smaller line height. A PDF of the same display
//! list showed the whole glyphs, because that writer ignores the rect.
//!
//! The item now has two rectangles, `line_box` for pagination and `clip_rect`
//! for painting, and only an axis that clips cuts ink.

#![cfg(all(
    feature = "cpurender",
    feature = "text_layout",
    feature = "font_loading"
))]

use azul_core::dom::{CssPropertyWithConditions, CssPropertyWithConditionsVec, Dom, IdOrClass};
use azul_css::props::{
    basic::{FontRef, StyleFontFamily, StyleFontFamilyVec},
    property::CssProperty,
};
use azul_layout::cpurender::{render_dom_to_image, AzulPixmap};

const KOHO_LIGHT: &[u8] = include_bytes!("../../examples/assets/fonts/KoHo-Light.ttf");

/// Where the text's box starts. The text is 48px on a line half as high, so
/// that its capitals stand out of the line above it.
const TOP: usize = 60;
const FONT_SIZE: usize = 48;

fn font() -> FontRef {
    let parsed = azul_layout::font::parsed::ParsedFont::from_bytes(KOHO_LIGHT, 0, &mut Vec::new())
        .expect("the bundled test face must parse");
    azul_layout::parsed_font_to_font_ref(parsed)
}

/// The rows of a 400 x 200 picture of "HELLO" that have ink in them.
fn inked_rows(overflow: &str) -> Vec<usize> {
    let mut text = Dom::create_text_do_not_use_without_block_level_wrapper("HELLO");
    text.root
        .set_css_props(CssPropertyWithConditionsVec::from_vec(vec![
            CssPropertyWithConditions::simple(CssProperty::font_family(
                StyleFontFamilyVec::from_vec(vec![StyleFontFamily::Ref(font())]),
            )),
        ]));
    let line = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("t".to_string().into())].into())
        .with_child(text);
    let dom = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("page".to_string().into())].into())
        .with_child(line);

    let (css, _) = azul_css::parser2::new_from_str(&format!(
        ".page {{ padding-top: {TOP}px; background: #ffffff; }} \
         .t {{ font-size: {FONT_SIZE}px; line-height: 0.5; color: #000000; overflow: {overflow}; }}"
    ));

    let png = render_dom_to_image(dom, css, 400.0, 200.0, 1.0).expect("render_dom_to_image");
    let picture = AzulPixmap::decode_png(&png).expect("decode png");
    let width = picture.width as usize;
    picture
        .data()
        .chunks_exact(width * 4)
        .enumerate()
        .filter(|(_, row)| {
            row.chunks_exact(4)
                .any(|p| p[0] < 128 && p[1] < 128 && p[2] < 128)
        })
        .map(|(y, _)| y)
        .collect()
}

#[test]
fn ink_taller_than_its_line_is_painted_whole_when_overflow_is_visible() {
    let rows = inked_rows("visible");
    let first = *rows.first().expect("the text is painted");
    // The line starts at TOP; a capital on a half-height line begins above it.
    // Cut at the line box, the first inked row was TOP itself.
    assert!(
        first < TOP,
        "the capitals reach above their line (which starts at row {TOP}): rows {rows:?}"
    );
}

#[test]
fn a_box_that_clips_still_cuts_the_ink_at_its_edge() {
    let visible = inked_rows("visible");
    let hidden = inked_rows("hidden");
    let first = *hidden.first().expect("the text is painted");
    assert!(
        first >= TOP,
        "overflow: hidden lets nothing out above the box at row {TOP}: rows {hidden:?}"
    );
    assert!(
        hidden.len() < visible.len(),
        "and so shows less of the text than overflow: visible does: {} rows against {}",
        hidden.len(),
        visible.len()
    );
}
