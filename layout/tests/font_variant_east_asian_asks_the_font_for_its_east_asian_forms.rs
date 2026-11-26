//! `font-variant-east-asian` reaches the shaper: `jis78`, `jis83`, `jis90`, `jis04`, `simplified`
//! and `traditional` ask the font for `jp78`, `jp83`, `jp90`, `jp04`, `smpl` and `trad`,
//! `full-width` and `proportional-width` for `fwid` and `pwid`, `ruby` for `ruby`, and a value
//! combines one of each group.
//!
//! The property parsed and was inherited, and then nothing read it: the style the shaper is given
//! had no field for it (the old one was a single keyword, which could not hold `jis78 full-width
//! ruby` either).
//!
//! The font is `azul-mock-features.ttf` (see `tests/fonts/README.md`): the six forms of `A`, the
//! two widths of `E` and the ruby form of `R` are glyphs of their own. The glyph id of an ASCII
//! character is `code point - 0x1F`.

use crate::text_rig::{block, ids_of, lay_out, painted, span, window, FEATURES};

const A: u32 = 34;
const E: u32 = 38;
const R: u32 = 51;
// The glyphs the features reach: A.jp78, A.jp83, A.jp90, A.jp04, A.smpl, A.trad, E.fwid, E.pwid, R.ruby.
const JP78: u32 = 96;
const JP83: u32 = 97;
const JP90: u32 = 98;
const JP04: u32 = 99;
const SMPL: u32 = 100;
const TRAD: u32 = 101;
const FWID: u32 = 102;
const PWID: u32 = 103;
const RUBY: u32 = 104;

/// `A E R` set under `css`: one letter of each group.
fn shaped(css: &str) -> Vec<u32> {
    ids_of(FEATURES, css, "AER")
}

#[test]
fn the_default_asks_for_no_east_asian_form() {
    assert_eq!(shaped(""), [A, E, R]);
    assert_eq!(shaped("font-variant-east-asian: normal"), [A, E, R]);
}

#[test]
fn each_jis_and_script_form_asks_for_its_feature() {
    for (css, glyph) in [
        ("jis78", JP78),
        ("jis83", JP83),
        ("jis90", JP90),
        ("jis04", JP04),
        ("simplified", SMPL),
        ("traditional", TRAD),
    ] {
        assert_eq!(
            shaped(&format!("font-variant-east-asian: {css}")),
            [glyph, E, R],
            "`{css}` changes the one letter of its group"
        );
    }
}

#[test]
fn the_widths_and_ruby_ask_for_their_features() {
    assert_eq!(shaped("font-variant-east-asian: full-width"), [A, FWID, R]);
    assert_eq!(shaped("font-variant-east-asian: proportional-width"), [A, PWID, R]);
    assert_eq!(shaped("font-variant-east-asian: ruby"), [A, E, RUBY]);
}

/// A value combines one keyword of each group; every one of them is asked for.
#[test]
fn a_value_of_three_groups_asks_for_each() {
    assert_eq!(shaped("font-variant-east-asian: jis78 full-width ruby"), [JP78, FWID, RUBY]);
    assert_eq!(shaped("font-variant-east-asian: ruby traditional proportional-width"), [TRAD, PWID, RUBY]);
    assert_eq!(shaped("font-variant-east-asian: jis90 ruby"), [JP90, E, RUBY]);
}

/// A value the grammar does not allow is dropped whole, as every invalid declaration is.
#[test]
fn an_invalid_value_leaves_the_text_as_it_was() {
    assert_eq!(shaped("font-variant-east-asian: jis78 jis83"), [A, E, R], "two of one group");
    assert_eq!(shaped("font-variant-east-asian: full-width proportional-width"), [A, E, R]);
    assert_eq!(shaped("font-variant-east-asian: sideways"), [A, E, R]);
}

/// The `font-variant` shorthand sets the property too.
#[test]
fn the_font_variant_shorthand_sets_it() {
    assert_eq!(shaped("font-variant: jis83 full-width"), [JP83, FWID, R]);
    assert_eq!(
        shaped("font-variant-east-asian: jis78; font-variant: normal"),
        [A, E, R],
        "`font-variant: normal` resets every longhand it sets"
    );
}

/// `font-feature-settings` is applied after `font-variant-east-asian`: it has the last word.
#[test]
fn font_feature_settings_has_the_last_word() {
    assert_eq!(
        shaped("font-variant-east-asian: ruby; font-feature-settings: \"ruby\" 0"),
        [A, E, R]
    );
    assert_eq!(shaped("font-feature-settings: \"jp83\" 1"), [JP83, E, R]);
}

/// The value is inherited.
#[test]
fn the_value_is_inherited_and_a_child_can_reset_it() {
    let mut lw = window();
    let dom = block(
        FEATURES,
        "font-variant-east-asian: jis78",
        vec![span("", "A"), span("font-variant-east-asian: normal", "A"), span("font-variant-east-asian: jis04", "A")],
    );
    let dl = lay_out(&mut lw, dom, 800.0);
    let ids: Vec<u32> = painted(&dl).iter().map(|g| g.id).collect();
    assert_eq!(ids, [JP78, A, JP04]);
}

/// The widths change the advance: `fwid` is a whole em (20 px at 20 px), `pwid` 400 units (8 px).
#[test]
fn the_width_forms_have_their_own_advances() {
    let pen_of_a_after_e = |css: &str| {
        let (_lw, dl) = crate::text_rig::laid_out(FEATURES, css, "EA", 800.0);
        let glyphs = painted(&dl);
        assert_eq!(glyphs.len(), 2, "`{css}`");
        glyphs[1].x - glyphs[0].x
    };
    assert!((pen_of_a_after_e("") - 10.0).abs() < 0.01);
    assert!((pen_of_a_after_e("font-variant-east-asian: full-width") - 20.0).abs() < 0.01);
    assert!((pen_of_a_after_e("font-variant-east-asian: proportional-width") - 8.0).abs() < 0.01);
}
