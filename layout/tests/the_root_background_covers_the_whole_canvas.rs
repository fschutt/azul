//! The root background covers the whole canvas (CSS 2.2 s14.2, CSS
//! Backgrounds 3 s2.11.1 / s2.11.2).
//!
//! WPT sweep (scripts/REFCI_2026_09_30.md, gap E-BG-CANVAS): every
//! `<body bgcolor>` mail and three WPT pages stay white around their body:
//!
//! - `background-color-body-propagation-001`: `html { background-color:
//!   transparent }` (an EXPLICIT transparent) and `body { background: green }` -
//!   the viewport must be green. azul took the root's transparent colour
//!   for "the root has a background", so nothing was propagated and only the
//!   body's own box turned green;
//! - `background-color-body-propagation-002`: the root's own background wins
//!   over the body's;
//! - `background-margin-root`: a gradient on a root with a margin covers the
//!   whole canvas, its tile anchored at the root's box and repeated
//!   (`background-repeat: repeat`, the initial value). azul painted the
//!   gradient on the root's box only and left the margins white.
//!
//! And one more the spec spells out: a propagated background is painted ONCE,
//! on the canvas - the body's "used values ... are their initial values"
//! (s2.11.2) and the root's background is the canvas's - so a translucent
//! body colour does not darken where the root's and the body's boxes are.
//!
//! The reftests these should flip: css/css-backgrounds/
//! background-color-body-propagation-001, background-margin-root.
//! Not compiled by the author (house rule); expected RED.

use crate::painted::{close, painted, GREEN, WHITE};

#[test]
fn an_explicitly_transparent_root_takes_the_bodys_background_for_the_canvas() {
    let page = painted(
        "<html><head><style>\
         html { background-color: transparent; background-image: none; }\
         body { background-color: green; margin: 0; }\
         div { height: 20px; }\
         </style></head><body><div></div></body></html>",
        200,
        100,
    );
    for (x, y) in [(5, 5), (190, 90), (100, 60)] {
        assert!(
            page.is(x, y, GREEN, 2),
            "the whole viewport is green, the body is only 20px tall; ({x}, {y}) is {:?}",
            page.rgb(x, y)
        );
    }
}

#[test]
fn the_roots_own_background_wins_over_the_bodys() {
    let page = painted(
        "<html><head><style>\
         html { background-color: green; }\
         body { background-color: red; margin: 0; }\
         div { height: 20px; background: green; }\
         </style></head><body><div></div></body></html>",
        200,
        100,
    );
    assert!(
        page.is(190, 90, GREEN, 2),
        "the canvas takes the root's green, not the body's red; (190, 90) is {:?}",
        page.rgb(190, 90)
    );
}

#[test]
fn a_translucent_body_background_is_painted_once() {
    // rgba(0, 0, 255, 0.5) over the white the window starts from: one layer
    // is (127, 127, 255). The body's background moved to the canvas, so the
    // html box and the body box paint nothing more over it.
    let page = painted(
        "<html><head><style>\
         body { background: rgba(0, 0, 255, 0.5); margin: 0; height: 40px; }\
         </style></head><body></body></html>",
        200,
        100,
    );
    let once = (127, 127, 255);
    for (x, y) in [(100, 20), (100, 80)] {
        assert!(
            close(page.rgb(x, y), once, 6),
            "one layer of 50% blue over white, inside the body ({x}, {y}) as outside it; got {:?}",
            page.rgb(x, y)
        );
    }
}

#[test]
fn a_root_gradient_repeats_over_the_whole_canvas_from_the_roots_box() {
    // The root is 160 x 100 at (20, 20): the gradient's tile is that box, and
    // it repeats in both directions over the whole 200 x 300 canvas -
    // red at the top of each tile, blue at its bottom.
    let page = painted(
        "<html><head><style>\
         html { background: linear-gradient(red, blue); height: 100px; margin: 20px; }\
         body { margin: 0; }\
         </style></head><body></body></html>",
        200,
        300,
    );
    let reddish = |(r, _, b): (u8, u8, u8)| r > 200 && b < 60;
    let bluish = |(r, _, b): (u8, u8, u8)| b > 200 && r < 60;
    for (x, y) in [(100, 22), (5, 22), (195, 22), (100, 122), (100, 222)] {
        assert!(
            reddish(page.rgb(x, y)),
            "the top of a tile is red at ({x}, {y}); got {:?}",
            page.rgb(x, y)
        );
    }
    for (x, y) in [(100, 117), (5, 117), (100, 217), (100, 17)] {
        assert!(
            bluish(page.rgb(x, y)),
            "the bottom of a tile is blue at ({x}, {y}); got {:?}",
            page.rgb(x, y)
        );
    }
    assert!(
        !page.is(100, 280, WHITE, 10),
        "nothing of the canvas stays white below the root"
    );
}
