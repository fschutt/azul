#![cfg(feature = "text_layout")]
//! EXACT, adversarial GSUB/GPOS shaping tests for the text3 engine, driven by
//! the on-disk stress fonts (`tests/fonts/azul-mock-{liga,kern,arabic}.ttf`)
//! whose metrics are hand-chosen round numbers (see `tests/fonts/README.md`).
//!
//! Everything here is exact arithmetic. upem 1000, so at font-size 20px the
//! scale is 0.02: a 500-unit advance is 10px, 700u is 14px, 900u is 18px, and
//! a -200u GPOS kern is -4px. The point of these tests is to PROVE that
//! ligature substitution and pair kerning actually fire — if "fi" stays two
//! glyphs or "AV" is unkerned, that is a MAJOR engine bug and the test FAILS.

use std::path::PathBuf;
use std::sync::Arc;

use azul_layout::font::parsed::ParsedFont;
use azul_layout::text3::cache::{BidiDirection, Glyph, StyleProperties};
use azul_layout::text3::default::shape_text_for_parsed_font;
use azul_layout::text3::script::{Language, Script};
use rust_fontconfig::FontBytes;

const FONT_SIZE: f32 = 20.0;
// scale = FONT_SIZE / upem(1000) = 0.02

fn font_path(name: &str) -> PathBuf {
    PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts")).join(name)
}

/// Parse a stress font, retaining source bytes so hmtx advances are readable.
fn parsed(name: &str) -> ParsedFont {
    let bytes = std::fs::read(font_path(name))
        .unwrap_or_else(|e| panic!("read stress font {name}: {e}"));
    let arc = Arc::new(FontBytes::Owned(Arc::from(bytes.as_slice())));
    let mut warnings = Vec::new();
    ParsedFont::from_bytes(&bytes, 0, &mut warnings)
        .unwrap_or_else(|| panic!("parse stress font {name}"))
        .with_source_bytes(arc)
}

fn style() -> StyleProperties {
    StyleProperties {
        font_size_px: FONT_SIZE,
        ..StyleProperties::default()
    }
}

fn shape(font: &ParsedFont, text: &str, script: Script, dir: BidiDirection) -> Vec<Glyph> {
    shape_text_for_parsed_font(font, text, script, Language::EnglishUS, dir, &style())
        .expect("shaping must succeed")
}

fn shape_latin(font: &ParsedFont, text: &str) -> Vec<Glyph> {
    shape(font, text, Script::Latin, BidiDirection::Ltr)
}

fn ids(glyphs: &[Glyph]) -> Vec<u16> {
    glyphs.iter().map(|g| g.glyph_id).collect()
}

/// Total pen advance in px = sum(advance + kerning) for every glyph.
fn total_advance(glyphs: &[Glyph]) -> f32 {
    glyphs.iter().map(|g| g.advance + g.kerning).sum()
}

fn assert_px(actual: f32, expected: f32, what: &str) {
    assert!(
        (actual - expected).abs() <= 0.05,
        "{what}: expected {expected:.4}px, got {actual:.4}px"
    );
}

// ===========================================================================
// A1. GSUB ligatures (Azul Mock Liga)
// ===========================================================================

#[test]
fn liga_fi_collapses_to_one_glyph_id96_adv14() {
    // f(71)+i(74) -> f_i(96), advance 700u => 14px. NOT two 500u glyphs.
    let font = parsed("azul-mock-liga.ttf");
    let g = shape_latin(&font, "fi");
    assert_eq!(g.len(), 1, "'fi' must ligate to exactly 1 glyph, got {:?}", ids(&g));
    assert_eq!(g[0].glyph_id, 96, "'fi' ligature glyph id must be 96");
    assert_px(g[0].advance, 14.0, "fi ligature advance");
}

#[test]
fn liga_ff_collapses_to_id97() {
    // f+f -> f_f(97), advance 700u => 14px.
    let font = parsed("azul-mock-liga.ttf");
    let g = shape_latin(&font, "ff");
    assert_eq!(g.len(), 1, "'ff' must ligate to 1 glyph, got {:?}", ids(&g));
    assert_eq!(g[0].glyph_id, 97, "'ff' ligature glyph id must be 97");
    assert_px(g[0].advance, 14.0, "ff ligature advance");
}

#[test]
fn liga_ffi_collapses_to_id98_adv18() {
    // f+f+i -> f_f_i(98), advance 900u => 18px. The 3-component ligature wins.
    let font = parsed("azul-mock-liga.ttf");
    let g = shape_latin(&font, "ffi");
    assert_eq!(g.len(), 1, "'ffi' must ligate to 1 glyph, got {:?}", ids(&g));
    assert_eq!(g[0].glyph_id, 98, "'ffi' ligature glyph id must be 98");
    assert_px(g[0].advance, 18.0, "ffi ligature advance");
}

#[test]
fn liga_office_collapses_ffi() {
    // o f f i c e -> o, f_f_i(98), c, e = 4 glyphs.
    // advances: 10 + 18 + 10 + 10 = 48px.
    let font = parsed("azul-mock-liga.ttf");
    let g = shape_latin(&font, "office");
    assert_eq!(g.len(), 4, "'office' must be 4 glyphs (ffi ligated), got {:?}", ids(&g));
    assert!(g.iter().any(|gl| gl.glyph_id == 98), "office must contain the ffi ligature (98)");
    assert_px(total_advance(&g), 48.0, "office total advance");
}

#[test]
fn liga_if_does_not_ligate() {
    // 'i' then 'f' — no ligature keyed by 'i'; stays 2 glyphs, 20px.
    let font = parsed("azul-mock-liga.ttf");
    let g = shape_latin(&font, "if");
    assert_eq!(g.len(), 2, "'if' must NOT ligate, got {:?}", ids(&g));
    assert_px(total_advance(&g), 20.0, "if total advance (2x10px)");
}

// ===========================================================================
// A2. GPOS pair kerning (Azul Mock Kern)
// ===========================================================================

#[test]
fn kern_av_pair_is_16px() {
    // A(34)+V(55): GPOS XAdvance -200u => -4px. 10 + 10 - 4 = 16px. Count stays 2.
    let font = parsed("azul-mock-kern.ttf");
    let g = shape_latin(&font, "AV");
    assert_eq!(g.len(), 2, "'AV' must stay 2 glyphs (kern is positioning, not substitution)");
    assert_px(total_advance(&g), 16.0, "AV kerned total advance");
}

#[test]
fn kern_va_is_not_kerned_20px() {
    // Only the (A,V) pair is defined; (V,A) is not => 10 + 10 = 20px.
    let font = parsed("azul-mock-kern.ttf");
    let g = shape_latin(&font, "VA");
    assert_eq!(g.len(), 2);
    assert_px(total_advance(&g), 20.0, "VA unkerned total advance");
}

#[test]
fn kern_ava_kerns_only_first_pair_26px() {
    // A V A: (A,V) kerns -4px, (V,A) does not. 10 + 10 + 10 - 4 = 26px.
    let font = parsed("azul-mock-kern.ttf");
    let g = shape_latin(&font, "AVA");
    assert_eq!(g.len(), 3);
    assert_px(total_advance(&g), 26.0, "AVA total advance (one kern pair)");
}

#[test]
fn kern_aa_is_not_kerned_20px() {
    // (A,A) has no kern pair: proves kerning is pair-specific.
    let font = parsed("azul-mock-kern.ttf");
    let g = shape_latin(&font, "AA");
    assert_px(total_advance(&g), 20.0, "AA unkerned total advance");
}

// ===========================================================================
// A3. Arabic joining + required ligature (Azul Mock Arabic, RTL)
// ===========================================================================

fn shape_arabic(font: &ParsedFont, text: &str) -> Vec<Glyph> {
    shape(font, text, Script::Arabic, BidiDirection::Rtl)
}

#[test]
fn arabic_beh_isolated() {
    // Single beh -> isolated form gid 7.
    let font = parsed("azul-mock-arabic.ttf");
    let g = shape_arabic(&font, "\u{0628}");
    assert_eq!(g.len(), 1, "single beh => 1 glyph");
    assert_eq!(g[0].glyph_id, 7, "isolated beh must be gid 7, got {:?}", ids(&g));
}

#[test]
fn arabic_beh_teh_init_fina() {
    // beh teh -> [beh.init(8), teh.fina(14)]. Assert both forms present.
    let font = parsed("azul-mock-arabic.ttf");
    let g = shape_arabic(&font, "\u{0628}\u{062A}");
    assert_eq!(g.len(), 2, "beh teh => 2 glyphs, got {:?}", ids(&g));
    let set: Vec<u16> = ids(&g);
    assert!(set.contains(&8), "must contain beh.init (8): {set:?}");
    assert!(set.contains(&14), "must contain teh.fina (14): {set:?}");
}

#[test]
fn arabic_beh_teh_meem_init_medi_fina() {
    // beh teh meem -> [beh.init(8), teh.medi(13), meem.fina(22)].
    let font = parsed("azul-mock-arabic.ttf");
    let g = shape_arabic(&font, "\u{0628}\u{062A}\u{0645}");
    assert_eq!(g.len(), 3, "beh teh meem => 3 glyphs, got {:?}", ids(&g));
    let set: Vec<u16> = ids(&g);
    assert!(set.contains(&8), "beh.init (8) missing: {set:?}");
    assert!(set.contains(&13), "teh.medi (13) missing: {set:?}");
    assert!(set.contains(&22), "meem.fina (22) missing: {set:?}");
}

#[test]
fn arabic_lam_alef_required_ligature() {
    // lam alef -> single lam_alef glyph gid 25 (count 2 -> 1).
    let font = parsed("azul-mock-arabic.ttf");
    let g = shape_arabic(&font, "\u{0644}\u{0627}");
    assert_eq!(g.len(), 1, "lam+alef must collapse to 1 glyph, got {:?}", ids(&g));
    assert_eq!(g[0].glyph_id, 25, "lam_alef ligature must be gid 25");
}

#[test]
fn arabic_beh_lam_alef() {
    // beh lam alef -> [beh.init(8), lam_alef(25)].
    let font = parsed("azul-mock-arabic.ttf");
    let g = shape_arabic(&font, "\u{0628}\u{0644}\u{0627}");
    assert_eq!(g.len(), 2, "beh + lam_alef => 2 glyphs, got {:?}", ids(&g));
    let set: Vec<u16> = ids(&g);
    assert!(set.contains(&8), "beh.init (8) missing: {set:?}");
    assert!(set.contains(&25), "lam_alef (25) missing: {set:?}");
}
