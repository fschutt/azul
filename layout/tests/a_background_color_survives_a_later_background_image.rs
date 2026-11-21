//! `background-color` and `background-image` are two properties (CSS
//! Backgrounds 3 s3.2, s3.3): a later `background-image` - `none` too - keeps
//! the colour, the colour paints under the image layers, and the
//! `background` shorthand sets both (its colour, or transparent).
//!
//! Both used to be one property, so the later declaration won: `background-
//! image: none` after `background-color: green` painted no colour at all.

use crate::painted::{painted, Painted, BLUE, GREEN, WHITE};

/// A 40x40 box with `style`, alone in a 60x60 window.
fn boxed(style: &str) -> Painted {
    painted(
        &format!(
            "<html><head><style>body {{ margin: 0; }} div {{ width: 40px; height: 40px; }} \
             {style}</style></head><body><div class=\"b\"></div></body></html>"
        ),
        60,
        60,
    )
}

fn assert_at(p: &Painted, (x, y): (usize, usize), colour: (u8, u8, u8), what: &str) {
    assert!(p.is(x, y, colour, 2), "{what}: ({x}, {y}) is {:?}", p.rgb(x, y));
}

#[test]
fn background_image_none_keeps_the_background_colour() {
    let p = boxed(".b { background-color: green; background-image: none; }");
    assert_at(&p, (20, 20), GREEN, "the colour stays");
    assert_at(&p, (50, 50), WHITE, "outside the box");
}

#[test]
fn a_background_image_paints_over_the_background_colour() {
    // The gradient covers the top half; the colour shows below it.
    let p = boxed(
        ".b { background-color: green; \
         background-image: linear-gradient(blue, blue); background-size: 40px 20px; \
         background-repeat: no-repeat; }",
    );
    assert_at(&p, (20, 10), BLUE, "the image layer");
    assert_at(&p, (20, 30), GREEN, "the colour under it");
}

#[test]
fn a_background_colour_after_the_shorthand_replaces_its_colour() {
    let p = boxed(".b { background: blue; background-color: green; }");
    assert_at(&p, (20, 20), GREEN, "the later colour wins");
}

#[test]
fn the_shorthand_resets_an_earlier_background_colour() {
    // `background: linear-gradient(..) no-repeat` sets the colour to
    // transparent: the red declared before it is gone.
    let p = boxed(
        ".b { background-color: red; \
         background: linear-gradient(green, green) 0px 0px / 40px 20px no-repeat; }",
    );
    assert_at(&p, (20, 10), GREEN, "the shorthand's image");
    assert_at(&p, (20, 30), WHITE, "no colour under it");
}

#[test]
fn two_rules_set_the_colour_and_the_image_apart() {
    // Two rules, one per property: the image rule must not take the colour
    // the other rule set.
    let p = boxed(".b { background-color: green; } div.b { background-image: none; }");
    assert_at(&p, (20, 20), GREEN, "the colour of the other rule");
    let p = boxed(".b { background-image: none; } .b { background-color: green; }");
    assert_at(&p, (20, 20), GREEN, "in either order");
}
