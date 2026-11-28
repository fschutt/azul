//! What the user's contrast and transparency settings do to a colour that is about to be painted.
//!
//! Pure functions: a colour and the surface it is painted on go in, a colour comes out. The layout
//! (`solver3::presentation`) knows which colour is the ink, the fill or the edge of which node and
//! which surface lies behind it; the numbers are here, tested on every host.
//!
//! * Reduced transparency: [`opaque_over`] - a translucent colour becomes the opaque colour it looks
//!   like on its surface.
//! * Increased contrast: a text is read at [`MORE_CONTRAST_TEXT`] (7:1, WCAG 2.2 AAA) on its surface
//!   ([`more_contrast_ink`]); a surface is one that an extreme ink reaches that on
//!   ([`more_contrast_surface`]); a rule, the edge of a control and a focus ring are seen at
//!   [`MORE_CONTRAST_EDGE`] (4.5:1; WCAG 1.4.11 asks 3:1) ([`more_contrast_edge`]).
//!
//! A colour that already has the contrast comes back as it is, so a page that was readable does not
//! move; one that has not is moved toward black or white, keeping its tint, no further than it
//! must.

use crate::props::basic::color::ColorU;

/// The contrast ratio a text has on its surface under increased contrast: WCAG 2.2 success criterion
/// 1.4.6 (AAA), 7:1.
pub const MORE_CONTRAST_TEXT: f32 = 7.0;

/// The contrast ratio of a rule, a control's edge or a ring against its surface under increased
/// contrast: stronger than the 3:1 of WCAG 2.2 success criterion 1.4.11, as strong as the AA ratio of
/// a normal text.
pub const MORE_CONTRAST_EDGE: f32 = 4.5;

/// `colour` as it looks on `surface`, fully opaque: reduced transparency paints nothing see-through.
/// An opaque colour comes back as it is. `surface` is taken as opaque.
#[must_use]
pub fn opaque_over(colour: ColorU, surface: ColorU) -> ColorU {
    if colour.a == u8::MAX {
        return colour;
    }
    let share = f32::from(colour.a) / 255.0;
    let mut seen = surface.mix(&colour, share);
    seen.a = u8::MAX;
    seen
}

/// The extreme (black or white) that contrasts most with `surface`.
fn extreme_against(surface: &ColorU) -> ColorU {
    if surface.contrast_ratio(&ColorU::WHITE) >= surface.contrast_ratio(&ColorU::BLACK) {
        ColorU::WHITE
    } else {
        ColorU::BLACK
    }
}

/// `colour` moved away from `surface` until it has `ratio` against it, and at most to the extreme
/// that contrasts most; the colour itself when it has the ratio already.
fn at_least(colour: ColorU, surface: ColorU, ratio: f32) -> ColorU {
    let colour = opaque_over(colour, surface);
    if colour.contrast_ratio(&surface) >= ratio {
        return colour;
    }
    let moved = colour.ensure_contrast(&surface, ratio);
    if moved.contrast_ratio(&surface) >= ratio {
        return moved;
    }
    // no tint of this colour reaches it on this surface (a mid-tone one): the extreme does the most
    extreme_against(&surface)
}

/// A fill an increased-contrast text can be read on. A fill that an extreme ink (black or white)
/// reaches [`MORE_CONTRAST_TEXT`] on stays; a mid-tone one - neither black nor white reaches it - is
/// moved to the side it is nearer to, keeping its tint. `fill` is opaque (see [`opaque_over`]).
#[must_use]
pub fn more_contrast_surface(fill: ColorU) -> ColorU {
    let against_white = fill.contrast_ratio(&ColorU::WHITE);
    let against_black = fill.contrast_ratio(&ColorU::BLACK);
    if against_white >= MORE_CONTRAST_TEXT || against_black >= MORE_CONTRAST_TEXT {
        return fill;
    }
    let ink = if against_white >= against_black { ColorU::WHITE } else { ColorU::BLACK };
    let mut moved = fill.ensure_contrast(&ink, MORE_CONTRAST_TEXT);
    moved.a = fill.a;
    moved
}

/// A text colour on `surface` under increased contrast: [`MORE_CONTRAST_TEXT`] against it. Opaque.
#[must_use]
pub fn more_contrast_ink(ink: ColorU, surface: ColorU) -> ColorU {
    at_least(ink, surface, MORE_CONTRAST_TEXT)
}

/// A rule, the edge of a control or a ring on `surface` under increased contrast:
/// [`MORE_CONTRAST_EDGE`] against it. Opaque.
#[must_use]
pub fn more_contrast_edge(edge: ColorU, surface: ColorU) -> ColorU {
    at_least(edge, surface, MORE_CONTRAST_EDGE)
}

#[cfg(test)]
#[path = "presentation_test.rs"]
mod presentation_test;
