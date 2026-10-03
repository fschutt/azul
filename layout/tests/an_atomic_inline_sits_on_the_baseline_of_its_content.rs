//! An atomic inline sits on the line by the baseline of its CONTENT, so a
//! button alone on a line makes the line exactly as tall as the button.
//!
//! The line box holds the strut (CSS 2.1 s10.8, see
//! `a_line_holding_only_an_inline_block_is_as_tall_as_its_strut`); where the
//! box itself sits on the baseline decides whether it also reaches below the
//! strut. Its baseline:
//! - an `inline-flex` / `inline-grid` box: its FIRST item's (CSS Flexbox 8.5,
//!   Grid 10.6) - `overflow` does not change it;
//! - an `inline-block` holding blocks: its LAST line box's, at any depth
//!   (CSS 2.1 s10.8.1), or its bottom margin edge when it holds no line box or
//!   its `overflow` is not `visible`;
//! - a box with no baseline at all (an empty flex box): its bottom margin
//!   edge, so the strut's descent hangs below it.
//!
//! The flex / grid layout reported NO baseline and a block container's
//! layout neither, so every such box sat on the baseline with its bottom
//! edge: a 32px button beside text made a 36px line (Chrome 32), and alone
//! on a line - now that the line holds the strut - it would too.
//!
//! Chrome 154's numbers (headless, `body { margin: 0 }`, 16px Arial, `line-height:
//! normal`). Not compiled by the author (house rule); RED before the fix.
//!
//! Two fixes make these green: the flex / grid ones by RULINGS8
//! (`fc::layout_flex_grid` reports its first item's baseline), the
//! inline-block ones by MAILREF8 (`fc::inline_block_baseline`,
//! `UnifiedLayout::last_line_baseline`, wave 8 - merged at the integration,
//! not in RULINGS8's own base: on the RULINGS8 branch alone the inline-block
//! tests stay red).

use crate::table_markup::{body, near, rect};

/// `<div id="p">` in 16px Arial holding `content`.
fn line(content: &str) -> azul_layout::window::LayoutWindow {
    body(&format!(
        "<div id=\"p\" style=\"font-family: Arial; font-size: 16px\">{content}</div>"
    ))
}

/// The line's height, and the box `b`'s top relative to it.
fn heights(content: &str) -> (f32, f32) {
    let lw = line(content);
    let (p, b) = (rect(&lw, "p"), rect(&lw, "b"));
    (p.size.height, b.origin.y - p.origin.y)
}

const BUTTON: &str = "<span id=\"b\" style=\"display: inline-flex; padding: 6px 10px; border: 1px \
                      solid black\"><span>Button</span></span>";

#[test]
fn an_inline_flex_button_alone_on_a_line_makes_the_line_its_height() {
    let (p, b) = heights(BUTTON);
    assert!(near(p, 32.0, 1.0), "Chrome: a 32px line around the 32px button, got {p}");
    assert!(near(b, 0.0, 0.5), "Chrome: the button at the top, got {b}");
}

#[test]
fn an_inline_flex_button_beside_text_sits_on_its_labels_baseline() {
    let (p, b) = heights(&format!("x{BUTTON}"));
    assert!(
        near(p, 32.0, 1.0),
        "Chrome: 32 - the label's baseline on the text's, not the button's bottom (36), got {p}"
    );
    assert!(near(b, 0.0, 0.5), "Chrome: the button at the top, got {b}");
}

#[test]
fn an_inline_flex_box_takes_the_baseline_of_its_first_item_with_one() {
    // A 20px icon block (no baseline) then a label, centred: the label's.
    let (p, _) = heights(
        "<span id=\"b\" style=\"display: inline-flex; align-items: center; padding: 4px\">\
         <span style=\"display: block; width: 20px; height: 20px\"></span><span>Label</span></span>",
    );
    assert!(near(p, 28.0, 1.0), "Chrome: 28px, got {p}");

    let (p, _) = heights(
        "<span id=\"b\" style=\"display: inline-flex; flex-direction: column\">\
         <span>one</span><span>two</span></span>",
    );
    assert!(near(p, 36.0, 1.0), "Chrome: a column's first item: 36px, got {p}");
}

#[test]
fn an_inline_flex_box_keeps_its_items_baseline_with_overflow_hidden() {
    let (p, _) = heights(
        "<span id=\"b\" style=\"display: inline-flex; overflow: hidden; padding: 6px\">\
         <span>Clip</span></span>",
    );
    assert!(near(p, 30.0, 1.0), "Chrome: 30px, got {p}");
}

#[test]
fn an_inline_grid_box_takes_its_first_items_baseline() {
    let (p, _) = heights(
        "<span id=\"b\" style=\"display: inline-grid; padding: 5px\"><span>cell</span></span>",
    );
    assert!(near(p, 28.0, 1.0), "Chrome: 28px, got {p}");
}

#[test]
fn an_inline_block_of_blocks_takes_its_last_line_boxs_baseline() {
    let (p, _) = heights(
        "<span id=\"b\" style=\"display: inline-block; padding: 6px\"><div>text</div></span>",
    );
    assert!(near(p, 30.0, 1.0), "Chrome: 30px, got {p}");

    let (p, b) = heights(
        "<span id=\"b\" style=\"display: inline-block\"><div>a</div><div>b</div></span>",
    );
    assert!(
        near(p, 36.0, 1.0),
        "Chrome: the LAST line's baseline - two 18px lines make a 36px line, got {p}"
    );
    assert!(near(b, 0.0, 0.5), "Chrome: at the top, got {b}");
}

#[test]
fn a_box_with_no_baseline_hangs_the_struts_descent_below_it() {
    let (p, _) =
        heights("<span id=\"b\" style=\"display: inline-flex; width: 30px; height: 30px\"></span>");
    assert!(near(p, 34.0, 1.0), "Chrome: an empty flex box: 30 + 4, got {p}");

    let (p, _) = heights(
        "<span id=\"b\" style=\"display: inline-block\"><div style=\"width: 30px; height: \
         30px\"></div></span>",
    );
    assert!(near(p, 34.0, 1.0), "Chrome: no line box inside: 30 + 4, got {p}");
}

#[test]
fn an_inline_block_of_text_takes_the_baseline_of_its_last_line_where_it_is_laid_out() {
    // The inline formatting context reported the last item's OWN baseline
    // offset (a glyph's ascent, a box's distance from its bottom), not where
    // its last line's baseline IS: a two-line inline-block beside text sat
    // on its FIRST line (40, Chrome 36), a `line-height: 30px` one 4px low
    // (34, Chrome 30).
    let (p, b) = heights("x<span id=\"b\" style=\"display: inline-block\">a<br/>b</span>");
    assert!(near(p, 36.0, 1.0), "Chrome: 36px, got {p}");
    assert!(near(b, 0.0, 0.5), "Chrome: at the top, got {b}");

    let (p, _) =
        heights("x<span id=\"b\" style=\"display: inline-block; line-height: 30px\">a</span>");
    assert!(near(p, 30.0, 1.0), "Chrome: 30px, got {p}");
}

#[test]
fn an_inline_block_holding_a_button_sits_on_the_buttons_label() {
    // The dialog invoker: an inline-block whose one line holds an inline-flex
    // button - its baseline is the button's (its label's), 21px down, not the
    // button's distance from its bottom read as a distance from the top.
    let (p, b) = heights(&format!(
        "<span id=\"b\" style=\"display: inline-block\">{}</span>",
        BUTTON.replace("id=\"b\"", "id=\"w\"")
    ));
    assert!(near(p, 32.0, 1.0), "Chrome: 32px, got {p}");
    assert!(near(b, 0.0, 0.5), "Chrome: at the top, got {b}");

    // ...and one holding a 10px square holds the strut around it (Chrome 18).
    let (p, _) = heights(
        "<span id=\"b\" style=\"display: inline-block\"><span style=\"display: inline-block; \
         width: 10px; height: 10px\"></span></span>",
    );
    assert!(near(p, 18.0, 1.0), "Chrome: 18px, got {p}");
}
