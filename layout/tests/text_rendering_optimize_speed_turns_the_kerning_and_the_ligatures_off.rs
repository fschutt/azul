//! `text-rendering: optimizeSpeed` turns the pair adjustments and the common ligatures off where
//! `font-kerning` and `font-variant-ligatures` say nothing (a browser does the same); the other
//! values leave the shaper as it is, and an explicit `font-kerning` or `font-variant-ligatures`
//! has the last word.
//!
//! The fonts are `azul-mock-kern.ttf` (`A V` kerned by -200 units: 6 px apart instead of 10 at
//! 20 px) and `azul-mock-features.ttf` (`fi` = `liga`, `k` = `calt`); see `tests/fonts/README.md`.

use crate::text_rig::{block, ids, ids_of, lay_out, laid_out, painted, span, window, FEATURES, KERN};

const F: u32 = 71;
const I: u32 = 74;
const K: u32 = 76;
const F_I: u32 = 127;
const K_CALT: u32 = 121;

/// The distance from the pen of `A` to the pen of `V` in "AV".
fn pair(css: &str) -> f32 {
    let (_lw, dl) = laid_out(KERN, css, "AV", 800.0);
    let glyphs = painted(&dl);
    assert_eq!(glyphs.len(), 2, "`{css}`");
    glyphs[1].x - glyphs[0].x
}

fn shaped(css: &str, input: &str) -> Vec<u32> {
    ids_of(FEATURES, css, input)
}

#[test]
fn optimize_speed_turns_the_pair_adjustments_off() {
    assert!((pair("") - 6.0).abs() < 0.01);
    assert!((pair("text-rendering: optimizeSpeed") - 10.0).abs() < 0.01, "{}", pair("text-rendering: optimizeSpeed"));
}

#[test]
fn the_other_values_leave_the_kerning_on() {
    for value in ["auto", "optimizeLegibility", "geometricPrecision"] {
        let css = format!("text-rendering: {value}");
        assert!((pair(&css) - 6.0).abs() < 0.01, "`{css}`: {}", pair(&css));
    }
}

#[test]
fn an_explicit_font_kerning_has_the_last_word() {
    assert!((pair("text-rendering: optimizeSpeed; font-kerning: normal") - 6.0).abs() < 0.01);
    assert!((pair("text-rendering: optimizeLegibility; font-kerning: none") - 10.0).abs() < 0.01);
    assert!((pair("text-rendering: optimizeSpeed; font-feature-settings: \"kern\" 1") - 6.0).abs() < 0.01);
}

#[test]
fn optimize_speed_turns_the_ligatures_off() {
    assert_eq!(shaped("", "fi"), [F_I]);
    assert_eq!(shaped("text-rendering: optimizeSpeed", "fi"), [F, I]);
    assert_eq!(shaped("text-rendering: optimizeSpeed", "k"), [K], "the contextual alternate as well");
    for value in ["auto", "optimizeLegibility", "geometricPrecision"] {
        assert_eq!(shaped(&format!("text-rendering: {value}"), "fi"), [F_I], "{value}");
        assert_eq!(shaped(&format!("text-rendering: {value}"), "k"), [K_CALT], "{value}");
    }
}

#[test]
fn an_explicit_font_variant_ligatures_has_the_last_word() {
    assert_eq!(shaped("text-rendering: optimizeSpeed; font-variant-ligatures: common-ligatures", "fi"), [F_I]);
    assert_eq!(shaped("text-rendering: optimizeSpeed; font-feature-settings: \"liga\" 1", "fi"), [F_I]);
    assert_eq!(shaped("text-rendering: optimizeSpeed; font-variant-ligatures: normal", "fi"), [F, I]);
}

#[test]
fn the_value_is_inherited_and_auto_ends_it() {
    let mut lw = window();
    let dom = block(
        FEATURES,
        "text-rendering: optimizeSpeed",
        vec![span("", "fi"), span("text-rendering: auto", "fi"), span("text-rendering: optimizeLegibility", "fi")],
    );
    let dl = lay_out(&mut lw, dom, 800.0);
    assert_eq!(ids(&dl), [F, I, F_I, F_I]);
}
