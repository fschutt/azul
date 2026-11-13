//! CSS `zoom` scales the lengths of its subtree (LAYOUT7 item 6; MAIL6:
//! AzMail's reading-pane zoom could only scale text that inherits the pane's
//! font - a mail's own px sizes stayed, so its tables did not grow).
//!
//! The `zoom` property as Chrome implements it (and CSS Viewport 1 now
//! specifies): `<number> | <percentage> | normal`, not inherited, but its
//! EFFECT multiplies down the tree - the effective zoom of a box is the
//! product of the `zoom` of the box and of every ancestor. Every absolute
//! length (px, pt, in, cm, mm) and rem of a zoomed box is multiplied by its
//! effective zoom; its font size is, so em lengths follow; percentages are
//! not (they resolve against an already zoomed containing block).
//!
//! Measured in headless Chrome 154; every box is font-independent
//! (`line-height` set, fixed sizes).
//! Not compiled by the author (house rule); RED before the fix (`zoom` did
//! not parse: every box laid out unzoomed).

use crate::table_markup::{body, near, rect};

#[test]
fn a_zoomed_box_scales_its_width_padding_and_line_height() {
    let lw = body(
        "<div id=\"z\" style=\"zoom: 2; width: 100px; padding: 10px; font-size: 10px; \
         line-height: 20px\">x</div><div id=\"after\" style=\"height: 10px\"></div>",
    );
    let z = rect(&lw, "z");
    assert!(
        near(z.size.width, 240.0, 0.5) && near(z.size.height, 80.0, 0.5),
        "(100 + 2 x 10) x 2 wide, (20 + 2 x 10) x 2 tall (Chrome 240 x 80): {z:?}"
    );
    let after = rect(&lw, "after");
    assert!(
        near(after.origin.y, 80.0, 0.5),
        "the flow moves on by the zoomed box: {after:?}"
    );
}

#[test]
fn a_percentage_inside_a_zoomed_box_is_not_zoomed_again() {
    let lw = body(
        "<div style=\"width: 400px\"><div id=\"z\" style=\"zoom: 2\"><div id=\"c\" \
         style=\"width: 50%; height: 10px\"></div></div></div>",
    );
    let z = rect(&lw, "z");
    assert!(
        near(z.size.width, 400.0, 0.5),
        "an auto width still fills: {z:?}"
    );
    let c = rect(&lw, "c");
    assert!(
        near(c.size.width, 200.0, 0.5) && near(c.size.height, 20.0, 0.5),
        "50% of the zoomed box, its 10px height zoomed (Chrome 200 x 20): {c:?}"
    );
}

#[test]
fn nested_zooms_multiply() {
    let lw = body(
        "<div style=\"zoom: 1.5\"><div style=\"zoom: 2\"><div id=\"c\" style=\"width: 10px; \
         height: 10px\"></div></div></div>",
    );
    let c = rect(&lw, "c");
    assert!(
        near(c.size.width, 30.0, 0.5) && near(c.size.height, 30.0, 0.5),
        "1.5 x 2 = 3 (Chrome 30 x 30): {c:?}"
    );
}

#[test]
fn em_lengths_follow_the_zoomed_font_size() {
    let lw = body(
        "<div id=\"z\" style=\"zoom: 2; font-size: 10px; padding: 1em; width: 50px; height: \
         10px\"></div>",
    );
    let z = rect(&lw, "z");
    assert!(
        near(z.size.width, 140.0, 0.5) && near(z.size.height, 60.0, 0.5),
        "1em is 20px at zoom 2 (Chrome 140 x 60): {z:?}"
    );
}

#[test]
fn a_percentage_zoom_parses_and_normal_is_one() {
    let lw = body(
        "<div id=\"z\" style=\"zoom: 150%; width: 100px; height: 10px\"></div><div id=\"n\" \
         style=\"zoom: normal; width: 100px; height: 10px\"></div>",
    );
    let z = rect(&lw, "z");
    assert!(
        near(z.size.width, 150.0, 0.5) && near(z.size.height, 15.0, 0.5),
        "150% is 1.5 (Chrome 150 x 15): {z:?}"
    );
    let n = rect(&lw, "n");
    assert!(
        near(n.size.width, 100.0, 0.5) && near(n.origin.y, 15.0, 0.5),
        "normal is 1: {n:?}"
    );
}

#[test]
fn a_zoomed_box_scales_its_margin_and_border() {
    let lw = body(
        "<div id=\"z\" style=\"zoom: 2; margin: 5px; border: 3px solid red; width: 10px; \
         height: 10px\"></div><div id=\"after\" style=\"height: 10px\"></div>",
    );
    let z = rect(&lw, "z");
    assert!(
        near(z.origin.x, 10.0, 0.5)
            && near(z.origin.y, 10.0, 0.5)
            && near(z.size.width, 32.0, 0.5)
            && near(z.size.height, 32.0, 0.5),
        "the margin 10, the border box (10 + 2 x 3) x 2 (Chrome 10, 10, 32 x 32): {z:?}"
    );
    let after = rect(&lw, "after");
    assert!(
        near(after.origin.y, 52.0, 0.5),
        "10 + 32 + 10 (Chrome 52): {after:?}"
    );
}

#[test]
fn a_zoomed_positioned_box_scales_its_offsets() {
    let lw = body(
        "<div style=\"position: relative; width: 300px; height: 300px\"><div id=\"z\" \
         style=\"zoom: 2; position: absolute; left: 10px; top: 10px; width: 10px; height: \
         10px\"></div></div>",
    );
    let z = rect(&lw, "z");
    assert!(
        near(z.origin.x, 20.0, 0.5) && near(z.origin.y, 20.0, 0.5) && near(z.size.width, 20.0, 0.5),
        "left / top 10px are 20px at zoom 2 (Chrome 20, 20, 20 x 20): {z:?}"
    );
}
