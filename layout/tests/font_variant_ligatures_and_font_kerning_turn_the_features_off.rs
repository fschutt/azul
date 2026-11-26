//! `font-variant-ligatures` and `font-kerning` reach the shaper: the groups of ligatures a text
//! turns off lose their OpenType features (also the ones a Latin script has on by default, `liga`,
//! `clig` and `calt`), the groups it turns on get theirs (`dlig`, `hlig`), and kerning is a switch
//! of its own that `font-feature-settings` still has the last word over.
//!
//! The two properties had no source: the shaper read a field of the style that nothing set from
//! CSS, so `font-variant-ligatures: none` joined the letters all the same and `font-kerning: none`
//! kerned them.
//!
//! The fonts are `azul-mock-features.ttf` (the ligatures `fi` = `liga`, `cl` = `clig`, `dl` =
//! `dlig`, `hl` = `hlig`, and `k` = `calt`, each one glyph of its own) and `azul-mock-kern.ttf`
//! (`A V` kerned by -200 units); see `tests/fonts/README.md`. The glyph id of an ASCII character in
//! both is `code point - 0x1F`.

use crate::text_rig::{block, ids_of, laid_out, painted, span, text, window, KERN, FEATURES};

// The glyphs of the letters.
const F: u32 = 71;
const I: u32 = 74;
const C: u32 = 68;
const L: u32 = 77;
const D: u32 = 69;
const H: u32 = 73;
const K: u32 = 76;
// The glyphs the features reach: f+i (`liga`), c+l (`clig`), d+l (`dlig`), h+l (`hlig`), k (`calt`).
const F_I: u32 = 127;
const C_L: u32 = 128;
const D_L: u32 = 129;
const H_L: u32 = 130;
const K_CALT: u32 = 121;

fn shaped(css: &str, input: &str) -> Vec<u32> {
    ids_of(FEATURES, css, input)
}

/// The premise: with nothing said a Latin script turns on `liga`, `clig` and `calt`, and `dlig` and
/// `hlig` stay off.
#[test]
fn the_default_joins_the_common_ligatures_and_picks_the_contextual_alternate() {
    assert_eq!(shaped("", "fi"), [F_I]);
    assert_eq!(shaped("", "cl"), [C_L]);
    assert_eq!(shaped("", "k"), [K_CALT]);
    assert_eq!(shaped("", "dl"), [D, L], "discretionary ligatures are off by default");
    assert_eq!(shaped("", "hl"), [H, L], "historical ligatures are off by default");
    assert_eq!(shaped("font-variant-ligatures: normal", "fi"), [F_I]);
}

#[test]
fn none_turns_off_every_group() {
    for css in ["font-variant-ligatures: none", "font-variant: none"] {
        assert_eq!(shaped(css, "fi"), [F, I], "`{css}`: liga");
        assert_eq!(shaped(css, "cl"), [C, L], "`{css}`: clig");
        assert_eq!(shaped(css, "k"), [K], "`{css}`: calt");
        assert_eq!(shaped(css, "dl"), [D, L], "`{css}`: dlig stays off");
    }
}

#[test]
fn no_common_ligatures_turns_off_liga_and_clig_and_nothing_else() {
    let css = "font-variant-ligatures: no-common-ligatures";
    assert_eq!(shaped(css, "fi"), [F, I]);
    assert_eq!(shaped(css, "cl"), [C, L]);
    assert_eq!(shaped(css, "k"), [K_CALT], "the contextual group is another group");
}

#[test]
fn no_contextual_turns_off_calt_and_nothing_else() {
    let css = "font-variant-ligatures: no-contextual";
    assert_eq!(shaped(css, "k"), [K]);
    assert_eq!(shaped(css, "fi"), [F_I]);
    assert_eq!(shaped("font-variant-ligatures: contextual", "k"), [K_CALT]);
}

#[test]
fn discretionary_and_historical_ligatures_turn_dlig_and_hlig_on() {
    let css = "font-variant-ligatures: discretionary-ligatures";
    assert_eq!(shaped(css, "dl"), [D_L]);
    assert_eq!(shaped(css, "hl"), [H, L], "the other group stays off");
    assert_eq!(shaped(css, "fi"), [F_I], "and the common ones stay on");

    let css = "font-variant-ligatures: historical-ligatures";
    assert_eq!(shaped(css, "hl"), [H_L]);
    assert_eq!(shaped(css, "dl"), [D, L]);

    let css = "font-variant-ligatures: discretionary-ligatures historical-ligatures no-common-ligatures";
    assert_eq!(shaped(css, "dlhlfi"), [D_L, H_L, F, I], "a value of three groups asks for each");

    assert_eq!(shaped("font-variant-ligatures: common-ligatures", "fi"), [F_I]);
}

/// The value is inherited, and a box that says otherwise turns its own text back on.
#[test]
fn the_value_is_inherited_and_a_child_can_say_otherwise() {
    let mut lw = window();
    let dom = block(
        FEATURES,
        "font-variant-ligatures: none",
        vec![
            text("fi"),
            span("font-variant-ligatures: common-ligatures", "fi"),
            span("", "fi"),
        ],
    );
    let dl = crate::text_rig::lay_out(&mut lw, dom, 800.0);
    let ids: Vec<u32> = painted(&dl).iter().map(|g| g.id).collect();
    assert_eq!(
        ids,
        [F, I, F_I, F, I],
        "the text and the plain span inherit `none`, the span that asks for the common ligatures has them"
    );
}

/// `font-feature-settings` is applied after `font-variant-ligatures`: it has the last word.
#[test]
fn font_feature_settings_has_the_last_word() {
    assert_eq!(
        shaped("font-variant-ligatures: none; font-feature-settings: \"liga\" 1", "fi"),
        [F_I]
    );
    assert_eq!(
        shaped("font-variant-ligatures: discretionary-ligatures; font-feature-settings: \"dlig\" 0", "dl"),
        [D, L]
    );
    assert_eq!(shaped("font-feature-settings: \"liga\" 0", "fi"), [F, I]);
}

/// A tracked text is not joined (CSS Text 3 s8.2), unless the ligatures are asked for.
#[test]
fn letter_spacing_keeps_the_common_ligatures_off_unless_they_are_asked_for() {
    assert_eq!(shaped("letter-spacing: 2px", "fi"), [F, I]);
    assert_eq!(
        shaped("letter-spacing: 2px; font-variant-ligatures: common-ligatures", "fi"),
        [F_I]
    );
}

// ---------------------------------------------------------------------------------------------
// font-kerning
// ---------------------------------------------------------------------------------------------

/// The distance from the pen of `A` to the pen of `V` in "AV" set in the kern font under `css`: an
/// advance of 500 units is 10 px at 20 px, the pair is kerned by -200 units = -4 px.
fn pair(css: &str) -> f32 {
    let (_lw, dl) = laid_out(KERN, css, "AV", 800.0);
    let glyphs = painted(&dl);
    assert_eq!(glyphs.len(), 2, "`{css}`: the pair is two glyphs");
    glyphs[1].x - glyphs[0].x
}

#[test]
fn a_pair_is_kerned_by_default_and_font_kerning_none_turns_it_off() {
    assert!((pair("") - 6.0).abs() < 0.01, "kerned by default: {}", pair(""));
    assert!((pair("font-kerning: auto") - 6.0).abs() < 0.01);
    assert!((pair("font-kerning: normal") - 6.0).abs() < 0.01, "not kerned twice: {}", pair("font-kerning: normal"));
    assert!((pair("font-kerning: none") - 10.0).abs() < 0.01, "{}", pair("font-kerning: none"));
}

/// `font-feature-settings` keeps the last word over `font-kerning`, and a `"kern"` that asks for
/// the kerning the face has on already applies the pair adjustments once.
#[test]
fn font_feature_settings_kern_has_the_last_word_and_kerns_once() {
    assert!((pair("font-kerning: none; font-feature-settings: \"kern\" 1") - 6.0).abs() < 0.01);
    assert!((pair("font-kerning: normal; font-feature-settings: \"kern\" 0") - 10.0).abs() < 0.01);
    assert!((pair("font-feature-settings: \"kern\" 0") - 10.0).abs() < 0.01);
    assert!(
        (pair("font-feature-settings: \"kern\" 1") - 6.0).abs() < 0.01,
        "the pair is kerned once, not twice: {}",
        pair("font-feature-settings: \"kern\" 1")
    );
}

#[test]
fn font_kerning_is_inherited() {
    let mut lw = window();
    let dom = block(KERN, "font-kerning: none", vec![span("", "AV")]);
    let dl = crate::text_rig::lay_out(&mut lw, dom, 800.0);
    let glyphs = painted(&dl);
    assert_eq!(glyphs.len(), 2);
    assert!((glyphs[1].x - glyphs[0].x - 10.0).abs() < 0.01, "the span inherits `none`");
}
