//! `font-variation-settings` sets the axes of a variable face by their four-letter tags, and its
//! values win over every value the other font properties derive for the same axis (CSS Fonts 4,
//! 7.2: `font-weight` and `font-stretch` are applied first, the settings last). Values are
//! clamped to the axis of the face, a tag given twice keeps its last value, a tag the face does
//! not have changes nothing, and a face without the axis draws as it did.
//!
//! Red Hat Mono has one axis, `wght` (300 to 700, default 400).

use crate::variable_fonts::{
    cache_with_file, drawn_bytes, instance_bytes, page, chain_of, first_face, red_hat_mono_path,
    resolve, static_face_path,
};

/// The face drawn for the text set with `css`, inside a block that declares `outer_css`.
fn drawn_inside(cache: &rust_fontconfig::FcFontCache, family: &str, outer_css: &str, css: &str) -> Vec<u8> {
    let styled = page(family, outer_css, css, "W");
    let id = first_face(&chain_of(&resolve(&styled, cache), family));
    cache
        .get_font_bytes(&id)
        .expect("the face drawn has bytes")
        .as_slice()
        .to_vec()
}

#[test]
fn the_settings_set_an_axis_and_win_over_font_weight() {
    let (cache, family, _source, file) = cache_with_file(&red_hat_mono_path());
    let at_650 = instance_bytes(&file, &[(*b"wght", 650.0)]);
    assert!(
        drawn_bytes(&cache, &family, "font-variation-settings: \"wght\" 650") == at_650,
        "\"wght\" 650 alone draws the file at 650"
    );
    assert!(
        drawn_bytes(
            &cache,
            &family,
            "font-weight: 300; font-variation-settings: \"wght\" 650"
        ) == at_650,
        "\"wght\" 650 wins over `font-weight: 300`, whatever the order of the declarations"
    );
    assert!(
        drawn_bytes(
            &cache,
            &family,
            "font-variation-settings: \"wght\" 650; font-weight: 300"
        ) == at_650,
        "\"wght\" 650 wins over a `font-weight` declared after it"
    );
}

#[test]
fn the_settings_are_inherited_and_normal_resets_them() {
    let (cache, family, _source, file) = cache_with_file(&red_hat_mono_path());
    let at_650 = instance_bytes(&file, &[(*b"wght", 650.0)]);
    let at_default = instance_bytes(&file, &[]);
    assert!(
        drawn_inside(&cache, &family, "font-variation-settings: \"wght\" 650", "") == at_650,
        "the text inherits the settings of the block around it"
    );
    assert!(
        drawn_inside(
            &cache,
            &family,
            "font-variation-settings: \"wght\" 650",
            "font-variation-settings: normal"
        ) == at_default,
        "`normal` resets the inherited settings: the default instance"
    );
}

#[test]
fn a_value_is_clamped_to_the_axis_and_the_last_of_a_tag_wins() {
    let (cache, family, _source, file) = cache_with_file(&red_hat_mono_path());
    assert!(
        drawn_bytes(&cache, &family, "font-variation-settings: \"wght\" 900")
            == instance_bytes(&file, &[(*b"wght", 700.0)]),
        "900 is clamped to the 700 of the axis"
    );
    assert!(
        drawn_bytes(&cache, &family, "font-variation-settings: \"wght\" 100")
            == instance_bytes(&file, &[(*b"wght", 300.0)]),
        "100 is clamped to the 300 of the axis"
    );
    assert!(
        drawn_bytes(
            &cache,
            &family,
            "font-variation-settings: \"wght\" 300, \"wght\" 650"
        ) == instance_bytes(&file, &[(*b"wght", 650.0)]),
        "a tag given twice keeps its last value"
    );
}

#[test]
fn a_tag_the_face_does_not_have_changes_nothing() {
    let (cache, family, _source, file) = cache_with_file(&red_hat_mono_path());
    let at_default = instance_bytes(&file, &[]);
    assert!(
        drawn_bytes(&cache, &family, "font-variation-settings: \"abcd\" 5") == at_default,
        "an unknown tag draws the default instance"
    );
    assert!(
        drawn_bytes(
            &cache,
            &family,
            "font-variation-settings: \"abcd\" 5, \"wght\" 650"
        ) == instance_bytes(&file, &[(*b"wght", 650.0)]),
        "the tags the face has are still applied"
    );
}

#[test]
fn a_face_without_the_axis_is_drawn_as_it_was() {
    let (cache, family, _source, file) = cache_with_file(&static_face_path());
    let plain = drawn_bytes(&cache, &family, "");
    assert!(plain == file, "premise: a static face is drawn as the file");
    assert!(
        drawn_bytes(&cache, &family, "font-variation-settings: \"wght\" 650") == plain,
        "\"wght\" 650 on a face without a `wght` axis changes nothing"
    );
}
