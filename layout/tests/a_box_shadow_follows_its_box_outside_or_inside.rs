//! A box shadow follows its box, outside or inside (CSS Backgrounds 3 s7.1-
//! 7.2): an OUTER shadow is painted outside the border box only (clipped
//! inside it - a transparent box shows no shadow through itself); an INNER
//! (`inset`) shadow is painted inside the padding box only, above the
//! background, as if everything outside the padding edge were opaque - its
//! hole shrunk by the spread and moved by the offset.
//!
//! WPT sweep (scripts/REFCI_2026_09_30.md, E-BG, css-backgrounds/
//! box-shadow-inset-without-border-radius, box-shadow-outset-without-border-
//! radius-001), probed on the 2026-10-03 prebuilt dylib:
//!
//! - an inline-block's shadow was never painted (the inline painter drew the
//!   background and the border only) - both WPT pages are inline-blocks;
//! - an inset shadow was drawn as an outer one, BELOW the background;
//! - an outer shadow kept a 1px sliver under the border box.
//! Not compiled by the author (house rule); expected RED.

use crate::painted::{painted, Painted, WHITE};

const BLACK: (u8, u8, u8) = (0, 0, 0);
const YELLOW: (u8, u8, u8) = (255, 255, 0);

fn page(body: &str) -> Painted {
    painted(
        &format!(
            "<html><head><style>body {{ margin: 0; }}</style></head><body>{body}</body></html>"
        ),
        100,
        100,
    )
}

#[test]
fn an_inline_blocks_shadow_is_painted() {
    let p = page(
        "<div style=\"display: inline-block; width: 20px; height: 20px; \
         box-shadow: black 10px 10px 0px 0px\"></div>",
    );
    assert!(
        p.is(25, 25, BLACK, 10),
        "the shadow of the 20px inline-block, moved 10px right and down; (25, 25) is {:?}",
        p.rgb(25, 25)
    );
}

#[test]
fn an_outer_shadow_is_not_painted_inside_a_transparent_box() {
    let p = page("<div style=\"width: 20px; height: 20px; box-shadow: black 10px 10px\"></div>");
    assert!(
        p.is(25, 25, BLACK, 10),
        "the shadow outside the box; (25, 25) is {:?}",
        p.rgb(25, 25)
    );
    for (x, y) in [(15, 15), (19, 15), (15, 19), (19, 19)] {
        assert!(
            p.is(x, y, WHITE, 2),
            "no shadow inside the border box, up to its edge; ({x}, {y}) is {:?}",
            p.rgb(x, y)
        );
    }
}

#[test]
fn an_inset_shadow_is_painted_inside_the_box_above_its_background() {
    // The hole is the padding box moved 10px right and down: the shadow is
    // the 10px strips along the top and the left edge.
    let p = page(
        "<div style=\"width: 40px; height: 40px; background: yellow; \
         box-shadow: inset 10px 10px 0 0 black\"></div>",
    );
    for (x, y) in [(5, 20), (20, 5), (5, 5)] {
        assert!(
            p.is(x, y, BLACK, 10),
            "the inset shadow along the top and left edges; ({x}, {y}) is {:?}",
            p.rgb(x, y)
        );
    }
    assert!(
        p.is(30, 30, YELLOW, 10),
        "the background where the hole is; (30, 30) is {:?}",
        p.rgb(30, 30)
    );
    assert!(
        p.is(45, 45, WHITE, 2),
        "nothing of an inset shadow outside the box; (45, 45) is {:?}",
        p.rgb(45, 45)
    );
}

#[test]
fn an_inset_shadows_spread_shrinks_its_hole() {
    let p = page(
        "<div style=\"width: 40px; height: 40px; background: yellow; \
         box-shadow: inset 0 0 0 10px black\"></div>",
    );
    for (x, y) in [(5, 20), (35, 20), (20, 5), (20, 35)] {
        assert!(
            p.is(x, y, BLACK, 10),
            "a 10px frame inside the box; ({x}, {y}) is {:?}",
            p.rgb(x, y)
        );
    }
    assert!(
        p.is(20, 20, YELLOW, 10),
        "the background in the middle; (20, 20) is {:?}",
        p.rgb(20, 20)
    );
}
