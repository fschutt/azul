//! `font-feature-settings`, `font-variant-caps` and `font-variant-numeric` ask the font for
//! OpenType features: a text set under them draws the glyphs the features reach.
//!
//! The font is `azul-mock-features.ttf` (see `tests/fonts/README.md`): one feature for each
//! property value, each changing one letter to a glyph of its own, so the glyph ids painted say
//! WHICH feature the CSS asked for. The glyph id of an ASCII character is `code point - 0x1F`.

use crate::text_rig::{block, ids_of, lay_out, painted, span, window, FEATURES};

fn shaped(css: &str, input: &str) -> Vec<u32> {
    ids_of(FEATURES, css, input)
}

// Letters of the font.
const A: u32 = 66; // a
const B_CAPITAL: u32 = 35; // B
const C_CAPITAL: u32 = 36; // C
const S: u32 = 84; // s
const X: u32 = 89; // x
const ZERO: u32 = 17; // 0
// What the caps features reach: a.smcp, B.c2sc, b.pcap, C.c2pc, u.unic, t.titl.
const A_SMCP: u32 = 105;
const B_C2SC: u32 = 106;
const B_PCAP: u32 = 107;
const C_C2PC: u32 = 108;
const U_UNIC: u32 = 109;
const T_TITL: u32 = 110;
// What the numeric features reach: 1 -> lnum, 2 -> onum, 3 -> pnum, 4 -> tnum, 5 -> frac,
// 6 -> afrc, 7 -> ordn, 0 -> zero.
const ONE_LNUM: u32 = 111;
const TWO_ONUM: u32 = 112;
const THREE_PNUM: u32 = 113;
const FOUR_TNUM: u32 = 114;
const FIVE_FRAC: u32 = 115;
const SIX_AFRC: u32 = 116;
const SEVEN_ORDN: u32 = 117;
const ZERO_ZERO: u32 = 118;
// What the other features reach.
const K: u32 = 76; // k
const K_CALT: u32 = 121;
const X_SS01: u32 = 123;
const Y_SS02: u32 = 124;
const Z_CV01: u32 = 125;
const V_CV02: u32 = 126;
const S_ALT1: u32 = 131;
const S_ALT2: u32 = 132;
const S_ALT3: u32 = 133;
const W_SWSH2: u32 = 135;

// ---------------------------------------------------------------------------------------------
// font-feature-settings
// ---------------------------------------------------------------------------------------------

#[test]
fn a_feature_that_is_asked_for_changes_its_letter() {
    assert_eq!(shaped("font-feature-settings: \"ss01\"", "x"), [X_SS01]);
    assert_eq!(shaped("font-feature-settings: \"ss01\" 1", "x"), [X_SS01]);
    assert_eq!(shaped("font-feature-settings: \"ss01\" on", "x"), [X_SS01]);
    assert_eq!(shaped("font-feature-settings: \"ss01\", \"ss02\"", "xy"), [X_SS01, Y_SS02]);
    assert_eq!(shaped("font-feature-settings: \"cv01\", \"cv02\"", "zv"), [Z_CV01, V_CV02]);
    assert_eq!(shaped("", "x"), [X], "premise: an unasked feature changes nothing");
}

#[test]
fn a_feature_that_is_turned_off_or_not_asked_for_changes_nothing() {
    assert_eq!(shaped("font-feature-settings: \"ss01\" 0", "x"), shaped("", "x"));
    assert_eq!(shaped("font-feature-settings: \"ss01\" off", "x"), shaped("", "x"));
    assert_eq!(shaped("font-feature-settings: normal", "x"), shaped("", "x"));
    assert_eq!(shaped("font-feature-settings: \"ss02\"", "x"), shaped("", "x"), "another letter's feature");
}

/// A feature a Latin script turns on by itself is turned off with a 0.
#[test]
fn a_feature_the_script_has_on_is_turned_off_with_a_zero() {
    assert_eq!(shaped("", "k"), [K_CALT]);
    assert_eq!(shaped("font-feature-settings: \"calt\" 0", "k"), [K]);
    assert_eq!(shaped("font-feature-settings: \"calt\" 1", "k"), [K_CALT]);
}

/// A value above 1 picks the alternate of that number, counted from 1: `"salt" 2` is the second
/// glyph of the set the font offers.
#[test]
fn a_feature_value_picks_its_alternate_counted_from_one() {
    assert_eq!(shaped("font-feature-settings: \"salt\"", "s"), [S_ALT1]);
    assert_eq!(shaped("font-feature-settings: \"salt\" 1", "s"), [S_ALT1]);
    assert_eq!(shaped("font-feature-settings: \"salt\" 2", "s"), [S_ALT2]);
    assert_eq!(shaped("font-feature-settings: \"salt\" 3", "s"), [S_ALT3]);
    assert_eq!(shaped("font-feature-settings: \"salt\" 4", "s"), [S], "the set has three");
    assert_eq!(shaped("font-feature-settings: \"swsh\" 2", "w"), [W_SWSH2]);
}

#[test]
fn a_tag_given_twice_takes_its_last_value() {
    assert_eq!(shaped("font-feature-settings: \"salt\" 2, \"salt\" 3", "s"), [S_ALT3]);
    assert_eq!(shaped("font-feature-settings: \"ss01\" 0, \"ss01\" 1", "x"), [X_SS01]);
}

#[test]
fn the_settings_are_inherited_and_a_child_can_reset_them() {
    let mut lw = window();
    let dom = block(
        FEATURES,
        "font-feature-settings: \"ss01\"",
        vec![span("", "x"), span("font-feature-settings: normal", "x"), span("font-feature-settings: \"ss02\"", "xy")],
    );
    let dl = lay_out(&mut lw, dom, 800.0);
    let ids: Vec<u32> = painted(&dl).iter().map(|g| g.id).collect();
    assert_eq!(ids, [X_SS01, X, X, Y_SS02]);
}

// ---------------------------------------------------------------------------------------------
// font-variant-caps
// ---------------------------------------------------------------------------------------------

#[test]
fn small_caps_asks_for_smcp_and_all_small_caps_for_c2sc_as_well() {
    assert_eq!(shaped("", "aB"), [A, B_CAPITAL]);
    assert_eq!(shaped("font-variant-caps: normal", "aB"), [A, B_CAPITAL]);
    assert_eq!(shaped("font-variant-caps: small-caps", "aB"), [A_SMCP, B_CAPITAL], "lowercase letters only");
    assert_eq!(shaped("font-variant-caps: all-small-caps", "aB"), [A_SMCP, B_C2SC]);
}

#[test]
fn petite_caps_ask_for_pcap_and_all_petite_caps_for_c2pc_as_well() {
    assert_eq!(shaped("font-variant-caps: petite-caps", "bC"), [B_PCAP, C_CAPITAL]);
    assert_eq!(shaped("font-variant-caps: all-petite-caps", "bC"), [B_PCAP, C_C2PC]);
}

#[test]
fn unicase_and_titling_caps_ask_for_unic_and_titl() {
    assert_eq!(shaped("font-variant-caps: unicase", "u"), [U_UNIC]);
    assert_eq!(shaped("font-variant-caps: titling-caps", "t"), [T_TITL]);
}

#[test]
fn the_caps_are_inherited_and_a_child_can_reset_them() {
    let mut lw = window();
    let dom = block(
        FEATURES,
        "font-variant-caps: small-caps",
        vec![span("", "a"), span("font-variant-caps: normal", "a")],
    );
    let dl = lay_out(&mut lw, dom, 800.0);
    let ids: Vec<u32> = painted(&dl).iter().map(|g| g.id).collect();
    assert_eq!(ids, [A_SMCP, A]);
}

// ---------------------------------------------------------------------------------------------
// font-variant-numeric
// ---------------------------------------------------------------------------------------------

#[test]
fn each_numeric_keyword_asks_for_its_feature() {
    for (css, digit, glyph) in [
        ("lining-nums", "1", ONE_LNUM),
        ("oldstyle-nums", "2", TWO_ONUM),
        ("proportional-nums", "3", THREE_PNUM),
        ("tabular-nums", "4", FOUR_TNUM),
        ("diagonal-fractions", "5", FIVE_FRAC),
        ("stacked-fractions", "6", SIX_AFRC),
        ("ordinal", "7", SEVEN_ORDN),
        ("slashed-zero", "0", ZERO_ZERO),
    ] {
        assert_eq!(
            shaped(&format!("font-variant-numeric: {css}"), digit),
            [glyph],
            "`{css}` changes {digit}"
        );
        assert_ne!(shaped("", digit), [glyph], "premise: {digit} is another glyph without it");
    }
}

#[test]
fn a_value_of_several_groups_asks_for_each() {
    assert_eq!(shaped("font-variant-numeric: tabular-nums slashed-zero", "04"), [ZERO_ZERO, FOUR_TNUM]);
    assert_eq!(shaped("font-variant-numeric: normal", "0"), [ZERO]);
}

/// A tabular figure is as wide as the font says (600 units = 12 px at 20 px): the pen of the next
/// glyph moves with it.
#[test]
fn a_feature_glyph_brings_its_own_advance() {
    let pen_after = |css: &str| {
        let (_lw, dl) = crate::text_rig::laid_out(FEATURES, css, "4a", 800.0);
        let glyphs = painted(&dl);
        assert_eq!(glyphs.len(), 2, "`{css}`");
        glyphs[1].x - glyphs[0].x
    };
    assert!((pen_after("") - 10.0).abs() < 0.01);
    assert!((pen_after("font-variant-numeric: tabular-nums") - 12.0).abs() < 0.01);
}
