//! `font-stretch` (and its alias `font-width`) drives the `wdth` axis, `font-style: oblique
//! <angle>` the `slnt` axis (the other way round: CSS leans to the right, `slnt` counts
//! counter-clockwise) and `font-style: italic` the `ital` axis; an explicit
//! `font-variation-settings` value beats the property for the same axis.
//!
//! The fixture `azul-mock-variable.ttf` has one glyph for each axis whose advance is linear in
//! that axis only (see `tests/fonts/README.md`): `D` = 500 + 4 * (wdth - 100), `S` = 500 + 10 *
//! slnt, `I` = 500 + 100 * ital; the other glyphs are 500 wide whatever is asked.

use crate::variable_fonts::{
    advance_of, cache_with_file, drawn_bytes, mock_variable_path, red_hat_mono_path,
};

/// The advances of `D`, `S`, `I` of the face drawn for `css`.
fn advances(cache: &rust_fontconfig::FcFontCache, family: &str, css: &str) -> (u16, u16, u16) {
    let bytes = drawn_bytes(cache, family, css);
    (
        advance_of(&bytes, 'D'),
        advance_of(&bytes, 'S'),
        advance_of(&bytes, 'I'),
    )
}

#[test]
fn font_stretch_sets_the_width_axis() {
    let (cache, family, _source, _file) = cache_with_file(&mock_variable_path());
    assert_eq!(advances(&cache, &family, ""), (500, 500, 500), "the default instance");
    for (css, wdth_advance) in [
        ("font-stretch: 87.5%", 450),
        ("font-stretch: semi-condensed", 450),
        ("font-width: 87.5%", 450),
        ("font-stretch: condensed", 400),
        ("font-stretch: expanded", 600),
        ("font-stretch: 110%", 540),
        // the axis runs from 75 to 125: ultra-condensed (50%) and ultra-expanded (200%) clamp
        ("font-stretch: ultra-condensed", 400),
        ("font-stretch: ultra-expanded", 600),
        ("font-stretch: normal", 500),
    ] {
        assert_eq!(
            advances(&cache, &family, css),
            (wdth_advance, 500, 500),
            "`{css}`: only the width glyph moves"
        );
    }
}

#[test]
fn oblique_sets_the_slant_axis_the_other_way_round() {
    let (cache, family, _source, _file) = cache_with_file(&mock_variable_path());
    for (css, slnt_advance) in [
        ("font-style: oblique 10deg", 400),
        // a bare `oblique` is 14 degrees
        ("font-style: oblique", 360),
        // the axis runs from -15 to 0: 40 degrees clamps
        ("font-style: oblique 40deg", 350),
        ("font-style: oblique -10deg", 500),
        ("font-style: normal", 500),
    ] {
        assert_eq!(
            advances(&cache, &family, css),
            (500, slnt_advance, 500),
            "`{css}`: only the slant glyph moves"
        );
    }
}

#[test]
fn italic_sets_the_italic_axis_and_normal_resets_it() {
    let (cache, family, _source, _file) = cache_with_file(&mock_variable_path());
    assert_eq!(advances(&cache, &family, "font-style: italic"), (500, 500, 600));
    assert_eq!(advances(&cache, &family, "font-style: normal"), (500, 500, 500));
    assert_eq!(
        advances(&cache, &family, "font-style: normal; font-variation-settings: \"ital\" 1"),
        (500, 500, 600),
        "an explicit \"ital\" 1 beats `font-style: normal`"
    );
    assert_eq!(
        advances(&cache, &family, "font-style: italic; font-variation-settings: \"ital\" 0"),
        (500, 500, 500),
        "and \"ital\" 0 beats `font-style: italic`"
    );
}

#[test]
fn the_properties_combine_on_one_face() {
    let (cache, family, _source, _file) = cache_with_file(&mock_variable_path());
    assert_eq!(
        advances(
            &cache,
            &family,
            "font-stretch: 87.5%; font-style: oblique 10deg; font-weight: 650"
        ),
        (450, 400, 500)
    );
    let weight_glyph = |css: &str| advance_of(&drawn_bytes(&cache, &family, css), 'W');
    assert_eq!(weight_glyph("font-weight: 650"), 750);
    assert_eq!(weight_glyph("font-weight: 650; font-stretch: 87.5%"), 750);
    assert_eq!(weight_glyph("font-weight: 350"), 450);
    assert_eq!(weight_glyph("font-weight: 1000"), 800, "clamped to the 700 of the axis");
}

/// Red Hat Mono has only a `wght` axis: the other properties do not touch it.
#[test]
fn a_face_without_those_axes_ignores_them() {
    let (cache, family, _source, _file) = cache_with_file(&red_hat_mono_path());
    let plain = drawn_bytes(&cache, &family, "");
    for css in [
        "font-stretch: 87.5%",
        "font-style: oblique 10deg",
        "font-style: italic",
        "font-variation-settings: \"wdth\" 80, \"slnt\" -10",
    ] {
        assert!(
            drawn_bytes(&cache, &family, css) == plain,
            "`{css}` changed a face that has none of those axes"
        );
    }
}
