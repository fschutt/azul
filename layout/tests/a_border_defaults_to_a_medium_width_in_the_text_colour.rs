//! A border defaults to a medium width in the text colour (CSS Backgrounds 3
//! s4.1-4.3: `border-*-color` is initially `currentcolor`, `border-*-width`
//! initially `medium` = 3px, and computes to 0 when the style is `none` or
//! `hidden`).
//!
//! WPT sweep (scripts/REFCI_2026_09_30.md, E-BG "border-width keywords":
//! css-backgrounds/border-top-width-medium / -thin / -thick). The keywords
//! themselves parse; probed on the 2026-10-03 prebuilt dylib, what failed is
//! around them:
//!
//! - `border-top-style: solid; border-top-width: medium` drew NOTHING: a side
//!   without a declared colour painted transparent instead of the text colour;
//! - `border-top-style: solid` alone had no border at all (width 0, not 3px);
//! - `border: 2px solid` (no colour) drew black, not the text colour.
//!
//! An explicit `transparent` must stay transparent (the compact cache stores
//! "no colour" and "transparent" alike).
//! Not compiled by the author (house rule); expected RED.

use crate::painted::{painted, Painted, BLUE, RED, WHITE};

fn page(divs: &str) -> Painted {
    painted(
        &format!(
            "<html><head><style>body {{ margin: 0; }} div {{ width: 60px; height: 10px; }}\
             </style></head><body>{divs}</body></html>"
        ),
        100,
        100,
    )
}

/// How many rows of column x=30, from the top, are blue.
fn blue_rows_from_the_top(p: &Painted) -> usize {
    (0..p.height).take_while(|&y| p.is(30, y, BLUE, 10)).count()
}

#[test]
fn a_border_with_a_style_and_a_width_but_no_colour_is_the_text_colour() {
    let p = page(
        "<div style=\"color: blue; border-top-style: solid; border-top-width: medium\"></div>",
    );
    assert_eq!(
        blue_rows_from_the_top(&p),
        3,
        "a medium (3px) border-top in the text colour, blue"
    );
}

#[test]
fn a_border_style_alone_draws_a_medium_border() {
    let p = page("<div style=\"color: blue; border-top-style: solid\"></div>");
    assert_eq!(
        blue_rows_from_the_top(&p),
        3,
        "the initial border-top-width is medium (3px)"
    );
}

#[test]
fn a_border_shorthand_without_a_colour_takes_the_text_colour() {
    let p = page("<div style=\"color: blue; border: 2px solid\"></div>");
    assert_eq!(
        blue_rows_from_the_top(&p),
        2,
        "`border: 2px solid` is drawn in the text colour"
    );
}

#[test]
fn an_explicitly_transparent_border_stays_transparent() {
    let p = page("<div style=\"color: blue; border: 4px solid transparent\"></div>");
    assert!(
        p.is(30, 1, WHITE, 2),
        "a transparent border paints nothing; (30, 1) is {:?}",
        p.rgb(30, 1)
    );
}

#[test]
fn a_border_width_without_a_style_is_no_border() {
    let p = page(
        "<div style=\"border-top-width: 20px\"></div>\
         <div style=\"background: red\"></div>",
    );
    let (_, y0, _, _) = p.bounds_of(RED, 10).expect("the red box is painted");
    assert_eq!(
        y0, 10,
        "a border without a style computes to 0: the red box follows the 10px div directly"
    );
}
