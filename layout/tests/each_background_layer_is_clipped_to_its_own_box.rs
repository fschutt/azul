//! Each background layer is painted within ITS OWN `background-clip` box
//! (CSS Backgrounds 3 s3.7: `background-clip` takes one box per layer, and
//! the `background` shorthand gives each layer its `<visual-box>`).
//!
//! That is how a metal edge is cut on the web: a transparent border, a
//! gradient on the border box UNDER a face on the padding box, so the
//! gradient shows only through the border - flora.css's selected tab
//! (`background: var(--fl-gem-sunken) padding-box, var(--fl-rolled-tab)
//! border-box`), its hero buttons and every leafed edge. azul had ONE clip
//! for every layer (a comma list kept its first value), and the shorthand
//! rejected a layer that named its box, so the whole declaration was
//! dropped and nothing was painted.
//! Not compiled by the author (house rule); expected RED. The face is listed
//! first and so is the top layer: this also needs the first layer painted on
//! top (`the_first_background_layer_is_painted_on_top`).

use crate::painted::{painted, Painted, BLUE, GREEN, WHITE};

/// A 30x30 content box with a 10px padding and a 10px transparent border:
/// border box 0..70, padding box 10..60, content box 20..50.
fn boxed(background: &str) -> Painted {
    painted(
        &format!(
            "<html><head><style>body {{ margin: 0; }}</style></head><body>\
             <div style=\"width: 30px; height: 30px; padding: 10px; \
             border: 10px solid transparent; {background}\"></div>\
             </body></html>"
        ),
        100,
        100,
    )
}

/// The ring under the border is the BOTTOM layer's, the padding and the
/// content the top layer's.
fn assert_metal_edge(p: &Painted, how: &str) {
    for (x, y) in [(5, 5), (35, 5), (65, 35), (35, 65)] {
        assert!(
            p.is(x, y, BLUE, 2),
            "{how}: the border-box layer shows through the border; ({x}, {y}) is {:?}",
            p.rgb(x, y)
        );
    }
    for (x, y) in [(15, 15), (35, 35), (55, 45)] {
        assert!(
            p.is(x, y, GREEN, 2),
            "{how}: the padding-box layer covers the padding box; ({x}, {y}) is {:?}",
            p.rgb(x, y)
        );
    }
    assert!(p.is(85, 85, WHITE, 2), "{how}: nothing outside the box");
}

#[test]
fn the_shorthand_gives_each_layer_its_own_box() {
    let p = boxed(
        "background: linear-gradient(green, green) padding-box, \
         linear-gradient(blue, blue) border-box;",
    );
    assert_metal_edge(&p, "background: <face> padding-box, <metal> border-box");
}

#[test]
fn the_clip_longhand_takes_one_box_per_layer() {
    let p = boxed(
        "background: linear-gradient(green, green), linear-gradient(blue, blue); \
         background-clip: padding-box, border-box;",
    );
    assert_metal_edge(&p, "background-clip: padding-box, border-box");
}

#[test]
fn a_short_clip_list_repeats_from_the_top_layer() {
    // Three layers, two clips: the third (bottom) layer takes the first clip
    // again. The bottom layer is red, clipped to the padding box, so it
    // shows nowhere - under the green top layer's padding box.
    let p = boxed(
        "background: linear-gradient(green, green), linear-gradient(blue, blue), \
         linear-gradient(red, red); background-clip: padding-box, border-box;",
    );
    assert_metal_edge(&p, "a two-box clip list over three layers");
}
