//! A background is painted within its `background-clip` box (CSS Backgrounds
//! 3 s3.7: `border-box` - the initial value -, `padding-box` or
//! `content-box`).
//!
//! WPT sweep (scripts/REFCI_2026_09_30.md, E-BG: css-backgrounds/
//! background-clip-003..007, css3-background-clip-content-box,
//! background-clip-color): azul had no `background-clip` property at all -
//! the declaration was dropped as unknown and every background covered the
//! border box.
//! Not compiled by the author (house rule); expected RED.

use crate::painted::{painted, Painted, GREEN, WHITE};

/// A 30x30 content box with a 10px padding and a 10px transparent border:
/// border box 0..70, padding box 10..60, content box 20..50.
fn boxed(clip: &str) -> Painted {
    painted(
        &format!(
            "<html><head><style>body {{ margin: 0; }}</style></head><body>\
             <div style=\"width: 30px; height: 30px; padding: 10px; \
             border: 10px solid transparent; background: green; {clip}\"></div>\
             </body></html>"
        ),
        100,
        100,
    )
}

#[test]
fn a_background_covers_the_border_box_by_default() {
    let p = boxed("");
    assert!(
        p.is(5, 5, GREEN, 2),
        "under the transparent border; (5, 5) is {:?}",
        p.rgb(5, 5)
    );
}

#[test]
fn a_padding_box_clip_leaves_the_border_area_bare() {
    let p = boxed("background-clip: padding-box;");
    assert!(
        p.is(5, 5, WHITE, 2),
        "nothing under the border; (5, 5) is {:?}",
        p.rgb(5, 5)
    );
    assert!(
        p.is(15, 15, GREEN, 2),
        "the padding is painted; (15, 15) is {:?}",
        p.rgb(15, 15)
    );
}

#[test]
fn a_content_box_clip_leaves_the_padding_bare_too() {
    let p = boxed("background-clip: content-box;");
    for (x, y) in [(5, 5), (15, 15), (55, 35)] {
        assert!(
            p.is(x, y, WHITE, 2),
            "nothing outside the content box; ({x}, {y}) is {:?}",
            p.rgb(x, y)
        );
    }
    assert!(
        p.is(35, 35, GREEN, 2),
        "the content box is painted; (35, 35) is {:?}",
        p.rgb(35, 35)
    );
}
