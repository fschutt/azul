//! A zoomed box paints its border, its corners and its shadow zoomed (wave 9
//! PKG 2 item 2.1; LAYOUT7 left, MAIL6): CSS `zoom` (Chrome's model) scales
//! every absolute length of the subtree, the painted ones too. LAYOUT7 zoomed
//! the box model (the border's SPACE grew), but the painter read the declared
//! lengths: a `3px` border under `zoom: 2` painted 3px inside its 6px slot, a
//! `10px` radius stayed 10px and a shadow kept its unzoomed offset.
//!
//! What Chrome paints (each length x 2): the boxes are font-independent
//! coloured rectangles. Not compiled by the author (house rule); RED before
//! the fix.

use crate::painted::{painted, Painted, WHITE};

const BLACK: (u8, u8, u8) = (0, 0, 0);
const YELLOW: (u8, u8, u8) = (255, 255, 0);

fn page(body: &str) -> Painted {
    painted(
        &format!(
            "<html><head><style>body {{ margin: 0; }}</style></head><body>{body}</body></html>"
        ),
        120,
        120,
    )
}

#[test]
fn a_zoomed_border_is_painted_at_its_zoomed_width() {
    // 20 x 20 content, 3px border, zoom 2: a 52 x 52 box with a 6px border.
    let p = page(
        "<div style=\"zoom: 2; width: 20px; height: 20px; border: 3px solid black; \
         background: yellow\"></div>",
    );
    assert!(
        p.is(4, 26, BLACK, 10),
        "the border is 6px wide, (4, 26) lies in it; it is {:?}",
        p.rgb(4, 26)
    );
    assert!(
        p.is(8, 26, YELLOW, 10),
        "the background inside the 6px border; (8, 26) is {:?}",
        p.rgb(8, 26)
    );
}

#[test]
fn a_zoomed_border_rounds_its_corners_at_the_zoomed_radius() {
    // 40 x 40 content, 5px border, 10px radius, zoom 2: a 100 x 100 box with a
    // 10px border and 20px corners. (4, 4) lies outside a 20px corner's curve
    // (centre (20, 20), 21.9px away) but inside a 10px one's border band.
    let p = page(
        "<div style=\"zoom: 2; width: 40px; height: 40px; border: 5px solid black; \
         border-radius: 10px\"></div>",
    );
    assert!(
        p.is(4, 4, WHITE, 10),
        "nothing outside the 20px corner; (4, 4) is {:?}",
        p.rgb(4, 4)
    );
    assert!(
        p.is(4, 50, BLACK, 10),
        "the straight 10px border on the left; (4, 50) is {:?}",
        p.rgb(4, 50)
    );
}

#[test]
fn a_zoomed_box_casts_its_shadow_at_the_zoomed_offset() {
    // A 20 x 20 box with a 10px 10px shadow, zoom 2: a 40 x 40 box whose
    // shadow is moved 20px right and down (it covers 20..60).
    let p = page(
        "<div style=\"zoom: 2; width: 20px; height: 20px; \
         box-shadow: black 10px 10px\"></div>",
    );
    assert!(
        p.is(55, 55, BLACK, 10),
        "the shadow reaches 60px; (55, 55) is {:?}",
        p.rgb(55, 55)
    );
    assert!(
        p.is(30, 30, WHITE, 2),
        "no shadow inside the transparent 40px box; (30, 30) is {:?}",
        p.rgb(30, 30)
    );
}
