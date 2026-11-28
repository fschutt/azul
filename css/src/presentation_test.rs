//! The colours of increased contrast and reduced transparency, over a sweep of colours and surfaces.

use crate::{
    presentation::{
        more_contrast_edge, more_contrast_ink, more_contrast_surface, opaque_over, MORE_CONTRAST_EDGE,
        MORE_CONTRAST_TEXT,
    },
    props::basic::color::ColorU,
};

/// 6 levels per channel: 216 colours, from black to white through every tint.
fn palette() -> Vec<ColorU> {
    const LEVELS: [u8; 6] = [0, 51, 102, 153, 204, 255];
    let mut colours = Vec::new();
    for r in LEVELS {
        for g in LEVELS {
            for b in LEVELS {
                colours.push(ColorU { r, g, b, a: 255 });
            }
        }
    }
    colours
}

#[test]
fn an_opaque_colour_is_not_changed_and_a_translucent_one_is_what_it_looks_like() {
    let ink = ColorU { r: 10, g: 20, b: 30, a: 255 };
    let white = ColorU::WHITE;
    assert_eq!(opaque_over(ink, white), ink);
    // fully transparent: the surface itself
    let nothing = ColorU { r: 200, g: 0, b: 0, a: 0 };
    assert_eq!(opaque_over(nothing, white), white);
    // half: halfway between (rounded), and opaque
    let half = ColorU { r: 0, g: 0, b: 0, a: 128 };
    let seen = opaque_over(half, white);
    assert_eq!(seen.a, 255);
    assert!((i32::from(seen.r) - 127).abs() <= 1 && seen.r == seen.g && seen.g == seen.b, "{seen:?}");
}

#[test]
fn a_text_has_seven_to_one_on_a_surface_an_extreme_ink_reaches_it_on_and_a_readable_one_stays() {
    let mut moved = 0;
    for surface in palette() {
        let surface = more_contrast_surface(surface);
        for ink in palette() {
            let read = more_contrast_ink(ink, surface);
            assert_eq!(read.a, 255);
            let ratio = read.contrast_ratio(&surface);
            assert!(
                ratio >= MORE_CONTRAST_TEXT,
                "{ink:?} on {surface:?} reads {ratio:.2}:1 as {read:?}"
            );
            if ink.contrast_ratio(&surface) >= MORE_CONTRAST_TEXT {
                assert_eq!(read, ink, "a text that was readable does not move");
            } else {
                moved += 1;
            }
        }
    }
    assert!(moved > 1000, "the sweep must contain many unreadable pairs to mean something: {moved}");
}

#[test]
fn a_mid_tone_fill_moves_to_a_side_and_an_extreme_fill_stays() {
    // black, white, the dark and the light of a theme: an extreme ink reaches 7:1 on each
    for fill in [ColorU::BLACK, ColorU::WHITE, ColorU { r: 24, g: 24, b: 28, a: 255 }, ColorU { r: 250, g: 248, b: 240, a: 255 }] {
        assert_eq!(more_contrast_surface(fill), fill);
    }
    // a mid grey and an accent blue (white on it is 5.6:1, black 3.5:1): neither reaches it
    let mid = ColorU { r: 120, g: 120, b: 120, a: 255 };
    let blue = ColorU { r: 32, g: 96, b: 192, a: 255 };
    for fill in [mid, blue] {
        let moved = more_contrast_surface(fill);
        assert_ne!(moved, fill);
        let best = moved
            .contrast_ratio(&ColorU::WHITE)
            .max(moved.contrast_ratio(&ColorU::BLACK));
        assert!(best >= MORE_CONTRAST_TEXT, "{fill:?} became {moved:?}: {best:.2}:1");
    }
    // the blue keeps its tint: it stays a blue (blue channel the largest)
    let moved = more_contrast_surface(blue);
    assert!(moved.b >= moved.r && moved.b >= moved.g, "{moved:?}");
}

#[test]
fn an_edge_has_four_and_a_half_to_one_on_its_surface_and_is_opaque() {
    for surface in palette() {
        for edge in palette().iter().step_by(7) {
            let seen = more_contrast_edge(*edge, surface);
            assert_eq!(seen.a, 255);
            let ratio = seen.contrast_ratio(&surface);
            // a mid-tone surface may only reach the best the extremes give (a text never sits on one:
            // `more_contrast_surface` moves it); every other one reaches the edge ratio
            let best = surface
                .contrast_ratio(&ColorU::WHITE)
                .max(surface.contrast_ratio(&ColorU::BLACK));
            assert!(
                ratio >= MORE_CONTRAST_EDGE.min(best) - 0.01,
                "{edge:?} on {surface:?}: {ratio:.2}:1 as {seen:?}"
            );
        }
    }
}

#[test]
fn a_translucent_ink_is_taken_for_what_it_looks_like_on_its_surface() {
    // a 30 % black on white is a light grey: not readable; it comes out as an opaque dark one
    let faint = ColorU { r: 0, g: 0, b: 0, a: 76 };
    let read = more_contrast_ink(faint, ColorU::WHITE);
    assert_eq!(read.a, 255);
    assert!(read.contrast_ratio(&ColorU::WHITE) >= MORE_CONTRAST_TEXT);
}
