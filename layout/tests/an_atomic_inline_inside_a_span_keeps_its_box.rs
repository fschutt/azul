//! An inline-flex, inline-grid or inline-table box, or an `<img>`, inside an
//! inline span is laid out as the atomic inline it is (MAILENG6 item 6,
//! TEXTENG's "left for wave 6").
//!
//! An inline box is a transparent wrapper (CSS Inline 3 §2): what sits in
//! `<span>` sits in the line exactly as it would without the span. The IFC
//! root's own collection already measures every non-`inline` child as an
//! atomic inline and an `<img>` as a replaced box, but
//! `fc::collect_inline_span_recursive` only knew `inline-block`: an
//! inline-flex / inline-grid / inline-table in a span was "inlinified" (its
//! children poured into the line, the box itself never laid out), and an
//! `<img>` in a span was an empty inline span - its picture's box lost (mail
//! HTML wraps nearly every picture in `<a>` or `<span>`).
//!
//! Fixed sizes; `font-size: 0` takes the strut out of the line heights.
//! Not compiled by the author (house rule); RED before the fix.

use crate::table_markup::{body, near, rect};

fn in_a_span(display: &str) -> String {
    format!(
        "<div id=\"d\" style=\"font-size: 0\"><span><span id=\"x\" style=\"display: {display}; \
         width: 60px; height: 20px; background: rgb(0, 128, 0)\"></span></span></div>"
    )
}

/// The line of `#d` (no strut: `font-size: 0`) holds the box: it is at
/// least `height` tall.
fn line_holds(lw: &azul_layout::window::LayoutWindow, height: f32) {
    let d = rect(lw, "d");
    assert!(
        d.size.height >= height - 0.5,
        "the box takes part in its line, which is at least {height}px tall: {d:?}"
    );
}

#[test]
fn an_inline_flex_box_in_a_span_is_sized_by_its_own_css() {
    let lw = body(&in_a_span("inline-flex"));
    let x = rect(&lw, "x");
    assert!(
        near(x.size.width, 60.0, 0.5) && near(x.size.height, 20.0, 0.5),
        "the inline-flex box keeps its 60 x 20 box inside the span: {x:?}"
    );
    line_holds(&lw, 20.0);
}

#[test]
fn an_inline_grid_box_in_a_span_is_sized_by_its_own_css() {
    let lw = body(&in_a_span("inline-grid"));
    let x = rect(&lw, "x");
    assert!(
        near(x.size.width, 60.0, 0.5) && near(x.size.height, 20.0, 0.5),
        "the inline-grid box keeps its 60 x 20 box inside the span: {x:?}"
    );
    line_holds(&lw, 20.0);
}

#[test]
fn an_inline_table_in_a_span_is_one_box() {
    let lw = body(
        "<div id=\"d\" style=\"font-size: 0\"><span><table id=\"x\" style=\"display: inline-table; \
         border-spacing: 0\"><tr><td style=\"padding: 0\"><span style=\"display: \
         inline-block; width: 60px; height: 20px\"></span></td></tr></table></span></div>",
    );
    let x = rect(&lw, "x");
    assert!(
        near(x.size.width, 60.0, 0.5) && near(x.size.height, 20.0, 0.5),
        "the inline table is one 60 x 20 box inside the span: {x:?}"
    );
    line_holds(&lw, 20.0);
}

#[test]
fn an_img_in_a_link_keeps_its_size() {
    let lw = body(
        "<div id=\"d\" style=\"font-size: 0\"><a href=\"#\"><img id=\"x\" \
         src=\"https://example.org/logo.png\" width=\"50\" height=\"30\"/></a></div>",
    );
    let x = rect(&lw, "x");
    assert!(
        near(x.size.width, 50.0, 0.5) && near(x.size.height, 30.0, 0.5),
        "the picture's box is its width / height attributes inside the link: {x:?}"
    );
    line_holds(&lw, 30.0);
}
