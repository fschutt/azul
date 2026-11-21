//! A CSS-wide keyword (`initial`, `inherit`, `unset`, `revert`) gives a property the value the
//! keyword names, on every property: `inherit` the parent's value, `initial` the property's
//! initial value, `unset` either of them by whether the property inherits, `revert` the user
//! agent's value.
//!
//! The cascade kept the keywords as written. Readers took a keyword for "no value", so
//! `width: inherit` was `auto`, `font-size: initial` was the parent's size, and the layout cache
//! kept whatever value came before the keyword (the user agent's centred `th`, a class's
//! padding). Every number here is headless Chrome's for the same page, with fixed-size
//! inline-blocks (`i`, 10 x 10) so no font is measured.

use crate::table_markup::{laid_out_page, near, page, rect, rects_of_color};

const STYLE: &str = "i { display: inline-block; width: 10px; height: 10px } \
                     body { margin: 0 }";

fn laid_out(style: &str, body: &str) -> azul_layout::window::LayoutWindow {
    laid_out_page(&page(&format!("{STYLE} {style}"), body))
}

#[test]
fn display_initial_makes_a_div_inline() {
    let lw = laid_out(
        ".di { display: initial }",
        "<div class=\"di\"><i id=\"d1\"></i></div><div class=\"di\"><i id=\"d2\"></i></div>",
    );
    let (d1, d2) = (rect(&lw, "d1"), rect(&lw, "d2"));
    assert!(
        near(d2.origin.y, d1.origin.y, 0.5) && near(d2.origin.x, 10.0, 0.5),
        "both divs are inline, so both boxes sit on one line (Chrome: x 0 and 10): {d1:?} {d2:?}"
    );
}

#[test]
fn width_inherit_takes_the_parents_width() {
    let lw = laid_out(
        "",
        "<div style=\"width: 300px\"><div id=\"w\" style=\"display: inline-block; width: \
         inherit; height: 10px\"></div></div>",
    );
    let w = rect(&lw, "w");
    assert!(near(w.size.width, 300.0, 0.5), "Chrome 300: {w:?}");
}

#[test]
fn margin_inherit_takes_the_parents_margin() {
    let lw = laid_out(
        "",
        "<div style=\"margin-left: 40px\"><div id=\"m\" style=\"margin-left: inherit; height: \
         10px\"></div></div>",
    );
    let m = rect(&lw, "m");
    assert!(near(m.origin.x, 80.0, 0.5), "40 + 40 (Chrome 80): {m:?}");
}

#[test]
fn an_inherit_under_an_inherit_takes_the_grandparents_value() {
    let lw = laid_out(
        "",
        "<div style=\"margin-left: 20px\"><div style=\"margin-left: inherit\"><div id=\"mm\" \
         style=\"margin-left: inherit; height: 10px\"></div></div></div>",
    );
    let mm = rect(&lw, "mm");
    assert!(near(mm.origin.x, 60.0, 0.5), "20 + 20 + 20 (Chrome 60): {mm:?}");
}

#[test]
fn padding_unset_is_its_initial_zero_over_a_class_rule() {
    let lw = laid_out(
        ".p { padding-left: 30px } #p2 { padding-left: unset }",
        "<div class=\"p\" id=\"p2\"><i id=\"p\"></i></div>",
    );
    let p = rect(&lw, "p");
    assert!(near(p.origin.x, 0.0, 0.5), "Chrome 0: {p:?}");
}

#[test]
fn font_size_initial_is_medium_under_a_larger_parent() {
    let lw = laid_out(
        "",
        "<div style=\"font-size: 30px\"><div id=\"f\" style=\"font-size: initial; width: 10em; \
         height: 10px\"></div></div>",
    );
    let f = rect(&lw, "f");
    assert!(near(f.size.width, 160.0, 0.5), "10em of 16px (Chrome 160): {f:?}");
}

#[test]
fn font_size_inherit_wins_over_a_class_rule() {
    let lw = laid_out(
        ".big { font-size: 40px }",
        "<div style=\"font-size: 20px\"><div class=\"big\" id=\"f2\" style=\"font-size: \
         inherit; width: 10em; height: 10px\"></div></div>",
    );
    let f2 = rect(&lw, "f2");
    assert!(near(f2.size.width, 200.0, 0.5), "10em of 20px (Chrome 200): {f2:?}");
}

#[test]
fn a_header_cell_with_text_align_inherit_aligns_as_its_table() {
    let lw = laid_out(
        "",
        "<table style=\"text-align: right; width: 300px; border-spacing: 0\"><tr><th id=\"th\" \
         style=\"text-align: inherit; padding: 0\"><i id=\"t\"></i></th></tr></table>",
    );
    let t = rect(&lw, "t");
    assert!(
        near(t.origin.x, 290.0, 0.5),
        "right-aligned like the table, not centred like a th (Chrome 290): {t:?}"
    );
}

#[test]
fn display_revert_is_the_user_agents_block_over_an_author_rule() {
    let lw = laid_out(
        ".r { display: inline } .r { display: revert }",
        "<div class=\"r\"><i id=\"r1\"></i></div><div class=\"r\"><i id=\"r2\"></i></div>",
    );
    let (r1, r2) = (rect(&lw, "r1"), rect(&lw, "r2"));
    assert!(
        near(r2.origin.x, 0.0, 0.5) && r2.origin.y > r1.origin.y + 9.5,
        "the divs are blocks again, one under the other (Chrome y 116 and 134): {r1:?} {r2:?}"
    );
}

#[test]
fn background_color_inherit_paints_the_parents_colour() {
    let lw = laid_out(
        "",
        "<div style=\"background-color: rgb(1, 2, 3); height: 40px; width: 300px\"><div \
         id=\"bg\" style=\"background-color: inherit; width: 100px; height: 10px\"></div></div>",
    );
    let painted = rects_of_color(&lw, (1, 2, 3));
    assert!(
        painted
            .iter()
            .any(|r| near(r.size.width, 100.0, 0.5) && near(r.size.height, 10.0, 0.5)),
        "the child paints rgb(1, 2, 3) too (Chrome): {painted:?}"
    );
}
