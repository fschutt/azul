//! The first layer of a `background` list is painted on TOP (CSS Backgrounds
//! 3 s2.2: "The first image in the list is the layer closest to the user,
//! the next one is painted behind the first, and so on. The background color,
//! if present, is painted below all of the other layers").
//!
//! azul stores a node's layers in PAINT order - the bottom layer first, which
//! is how every theme builds them (`themes::decl::layers`: "the base colour
//! goes FIRST - the reverse of a CSS comma list") - but the CSS parser kept
//! the comma list as written, so CSS text came out upside down:
//! `background: linear-gradient(green, green), red` painted the red colour
//! over the green layer. flora.css builds every metal edge that way - the face
//! first, the metal under it (`background: var(--fl-gem-sunken) padding-box,
//! var(--fl-rolled-tab) border-box`) - and the guide's own example
//! (`background: builtin(vellum-overlay), #f2f1ed`) buried its grain under
//! the colour.
//! Not compiled by the author (house rule); expected RED.

use crate::painted::{painted, Painted, GREEN};

/// A 40x40 box carrying `background`.
fn boxed(background: &str) -> Painted {
    painted(
        &format!(
            "<html><head><style>body {{ margin: 0; }}</style></head><body>\
             <div style=\"width: 40px; height: 40px; background: {background};\"></div>\
             </body></html>"
        ),
        60,
        60,
    )
}

#[test]
fn a_gradient_listed_before_the_colour_covers_it() {
    let p = boxed("linear-gradient(green, green), red");
    assert!(
        p.is(20, 20, GREEN, 2),
        "the first layer is the top one; (20, 20) is {:?}",
        p.rgb(20, 20)
    );
}

#[test]
fn of_two_gradients_the_first_one_shows() {
    let p = boxed("linear-gradient(green, green), linear-gradient(red, red)");
    assert!(
        p.is(20, 20, GREEN, 2),
        "the first layer is the top one; (20, 20) is {:?}",
        p.rgb(20, 20)
    );
}

#[test]
fn a_translucent_first_layer_lets_the_one_under_it_show_through() {
    // Half-transparent green over opaque red: (128, 64, 0). Upside down, the
    // opaque red would cover the green and the pixel would be pure red.
    let p = boxed("linear-gradient(rgba(0, 128, 0, 0.5), rgba(0, 128, 0, 0.5)), red");
    assert!(
        p.is(20, 20, (128, 64, 0), 3),
        "half green over red; (20, 20) is {:?}",
        p.rgb(20, 20)
    );
}
