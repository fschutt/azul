//! `font-variant-position: sub | super` asks the font for its `subs` and `sups` features, the
//! glyphs it draws a subscript or a superscript with; `normal` asks for neither, and
//! `font-feature-settings` still has the last word.
//!
//! The face is `azul-mock-features.ttf`: `p` has a `subs` glyph (119) and `q` a `sups` glyph
//! (120); the glyph id of an ASCII character is `code point - 0x1F` (`tests/fonts/README.md`).

use crate::text_rig::{block, ids, ids_of, lay_out, span, window, FEATURES};

const P: u32 = 81;
const Q: u32 = 82;
const P_SUBS: u32 = 119;
const Q_SUPS: u32 = 120;

fn shaped(css: &str) -> Vec<u32> {
    ids_of(FEATURES, css, "pq")
}

#[test]
fn normal_and_nothing_ask_for_neither_form() {
    assert_eq!(shaped(""), [P, Q]);
    assert_eq!(shaped("font-variant-position: normal"), [P, Q]);
}

#[test]
fn sub_asks_for_the_subscript_glyphs() {
    assert_eq!(shaped("font-variant-position: sub"), [P_SUBS, Q]);
}

#[test]
fn super_asks_for_the_superscript_glyphs() {
    assert_eq!(shaped("font-variant-position: super"), [P, Q_SUPS]);
}

#[test]
fn font_feature_settings_has_the_last_word() {
    assert_eq!(shaped("font-variant-position: sub; font-feature-settings: \"subs\" 0"), [P, Q]);
    assert_eq!(shaped("font-feature-settings: \"sups\" 1"), [P, Q_SUPS]);
    assert_eq!(
        shaped("font-variant-position: sub; font-feature-settings: \"sups\" 1"),
        [P_SUBS, Q_SUPS],
        "the two are different features: both are on"
    );
}

#[test]
fn the_value_is_inherited_and_a_child_can_end_it() {
    let mut lw = window();
    let dom = block(
        FEATURES,
        "font-variant-position: sub",
        vec![span("", "p"), span("font-variant-position: normal", "p"), span("font-variant-position: super", "q")],
    );
    let dl = lay_out(&mut lw, dom, 800.0);
    assert_eq!(ids(&dl), [P_SUBS, P, Q_SUPS]);
}

#[test]
fn an_unknown_value_is_dropped_and_the_text_is_set_as_it_was() {
    assert_eq!(shaped("font-variant-position: superscript"), [P, Q]);
    assert_eq!(shaped("font-variant-position: sub super"), [P, Q]);
}
