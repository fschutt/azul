//! An absolutely positioned inline element is a block: its `width` and
//! `height` apply.
//!
//! CSS 2.1 9.7 ("Relationships between 'display', 'position', and
//! 'float'"): an absolutely positioned element's `display` is set by the
//! table there, and `inline` becomes `block`. The size of such a box was
//! worked out from the `display` it was specified with, and "the width
//! property does not apply to inline non-replaced elements" (10.3.1) took
//! both of its dimensions away: a `position: absolute` `<span>` came out
//! 0px tall whatever its `height` said.

use crate::table_markup::{laid_out, rect, rects_of_color};

#[test]
fn an_absolute_span_is_as_wide_and_as_tall_as_its_css_says() {
    let lw = laid_out(
        "<html><head><style>* { margin: 0; padding: 0; } \
         #a { position: absolute; width: 30px; height: 12px }</style></head>\
         <body><p>text<span id=\"a\"></span></p></body></html>",
        800.0,
        600.0,
    );
    let a = rect(&lw, "a");
    assert_eq!((a.size.width, a.size.height), (30.0, 12.0));
}

#[test]
fn an_absolute_span_paints_its_background_once() {
    // The box paints the background; its text must not paint it again. It
    // did when the span had a `display: none` child beside its text.
    let lw = laid_out(
        "<html><head><style>* { margin: 0; padding: 0; } \
         #a { position: absolute; width: 60px; height: 30px; background: #ff0000 }</style></head>\
         <body><p>text<span id=\"a\"><b style=\"display: none\">hidden</b>note</span></p></body>\
         </html>",
        800.0,
        600.0,
    );
    let red = rects_of_color(&lw, (255, 0, 0));
    assert_eq!(red.len(), 1, "{red:?}");
    assert_eq!((red[0].size.width, red[0].size.height), (60.0, 30.0));
}

#[test]
fn a_hidden_child_of_an_inline_box_shows_nothing() {
    // CSS 2.1 9.2.4: a `display: none` element "and its content generates
    // no boxes at all" - also inside an inline box, where its text was
    // collected with the rest of the line.
    let lw = laid_out(
        "<html><head><style>* { margin: 0; padding: 0; }</style></head>\
         <body><p>a<span>b<b style=\"display: none\">hidden</b>c</span></p></body></html>",
        800.0,
        600.0,
    );
    let glyphs: usize = crate::table_markup::items(&lw)
        .iter()
        .filter_map(|item| match item {
            azul_layout::solver3::display_list::DisplayListItem::Text { glyphs, .. } => {
                Some(glyphs.len())
            }
            _ => None,
        })
        .sum();
    assert_eq!(glyphs, 3, "`abc`");
}
