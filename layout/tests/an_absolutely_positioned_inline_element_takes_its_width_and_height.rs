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

use crate::table_markup::{laid_out, rect};

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
