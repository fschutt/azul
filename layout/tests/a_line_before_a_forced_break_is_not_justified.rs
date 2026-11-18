//! Regression: `text-align: justify` spread a line that ends with `<br>`.
//!
//! CSS Text 3 §6.3: the last line of a block and a line right before a forced
//! break are aligned by `text-align-last`, whose `auto` turns `justify` into
//! `start`. The line breaker resolved that into each line's own alignment, but
//! the spacing for justification was decided from the paragraph's `text-align`
//! and from "is this the paragraph's last line" alone. So in a verse written as
//! lines with `<br>` between them, every line but the last was stretched to the
//! full width of its box, two words standing at its two ends.

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

const WIDTH: usize = 400;
const LINE: usize = 30;

fn font() -> FontRef {
    let parsed = azul_layout::font::parsed::ParsedFont::from_bytes(KOHO_LIGHT, 0, &mut Vec::new())
        .expect("the bundled test face must parse");
    azul_layout::parsed_font_to_font_ref(parsed)
}

fn text(content: &str) -> Dom {
    let mut text = Dom::create_text_do_not_use_without_block_level_wrapper(content);
    text.root
        .set_css_props(CssPropertyWithConditionsVec::from_vec(vec![
            CssPropertyWithConditions::simple(CssProperty::font_family(
                StyleFontFamilyVec::from_vec(vec![StyleFontFamily::Ref(font())]),
            )),
        ]));
    text
}

/// The rightmost inked column of each line of a justified paragraph: "on on",
/// a forced break, then a text long enough to wrap, with `align_last` as its
/// `text-align-last` if given.
fn right_edges(align_last: Option<&str>) -> Vec<usize> {
    let long = "on on on on on on on on on on on on on on on on on on on on on on on on on on";
    let paragraph = Dom::create_div()
        .with_ids_and_classes(vec![IdOrClass::Class("p".to_string().into())].into())
        .with_child(text("on on"))
        .with_child(Dom::create_br())
        .with_child(text(long));
    let last = align_last.map_or(String::new(), |a| format!("text-align-last: {a};"));
    let (css, _) = azul_css::parser2::new_from_str(&format!(
        ".p {{ width: {WIDTH}px; font-size: 20px; line-height: {LINE}px; color: #000000; \
               background: #ffffff; text-align: justify; {last} }}"
    ));
    let png = render_dom_to_image(paragraph, css, WIDTH as f32, 300.0, 1.0).expect("render_dom_to_image");
    let picture = AzulPixmap::decode_png(&png).expect("decode png");
    let width = picture.width as usize;
    let rows: Vec<&[u8]> = picture.data().chunks_exact(width * 4).collect();
    rows.chunks(LINE)
        .map(|line| {
            (0..width)
                .rev()
                .find(|&x| line.iter().any(|row| row[x * 4] < 128))
                .unwrap_or(0)
        })
        .take_while(|&edge| edge > 0)
        .collect()
}

#[test]
fn the_line_a_break_ends_keeps_its_words_together() {
    let edges = right_edges(None);
    assert!(edges.len() >= 3, "a short line, and a long text over at least two: {edges:?}");
    // "on on" is some 50px wide. Justified, its second word stood at the right edge.
    assert!(
        edges[0] < WIDTH / 2,
        "the line before the break is not spread to the box's width ({WIDTH}px): {edges:?}"
    );
    // The lines of the wrapped text are justified: they reach the right edge.
    assert!(
        edges[1] > WIDTH - 12,
        "a line the text wraps at is spread to the width: {edges:?}"
    );
    // And the paragraph's last line is not.
    let last = *edges.last().expect("lines");
    assert!(last < WIDTH - 12, "the last line stays unjustified: {edges:?}");
}

#[test]
fn text_align_last_justify_spreads_them_after_all() {
    let edges = right_edges(Some("justify"));
    assert!(
        edges[0] > WIDTH - 12,
        "with text-align-last: justify the line before the break is spread: {edges:?}"
    );
    let last = *edges.last().expect("lines");
    assert!(last > WIDTH - 12, "and so is the last line: {edges:?}");
}
