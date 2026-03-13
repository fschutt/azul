//! Unit test for OpenType ligature shaping via allsorts.
//!
//! Verifies that the text shaping pipeline correctly substitutes
//! "fi"/"fl" sequences with ligature glyphs when using a real font
//! (Times New Roman) that has Unicode ligature codepoints (U+FB01, U+FB02).

use azul_layout::font::parsed::ParsedFont;
use azul_layout::text3::cache::{BidiDirection, StyleProperties};
use azul_layout::text3::default::shape_text_for_parsed_font;
use azul_layout::text3::script::Script;
use hyphenation::Language;

/// Load Times New Roman from the system (macOS path).
/// Returns None if the font file is not found (e.g., CI without fonts).
fn load_times_new_roman() -> Option<ParsedFont> {
    let font_path = "/System/Library/Fonts/Supplemental/Times New Roman.ttf";
    let font_bytes = std::fs::read(font_path).ok()?;
    let mut warnings = Vec::new();
    ParsedFont::from_bytes(&font_bytes, 0, &mut warnings)
}

#[test]
fn test_fi_ligature_is_substituted() {
    let font = match load_times_new_roman() {
        Some(f) => f,
        None => {
            eprintln!("Skipping: Times New Roman not found");
            return;
        }
    };

    let style = StyleProperties {
        font_size_px: 16.0,
        ..StyleProperties::default()
    };

    // Shape "filled" - should produce a ligature for "fi"
    let glyphs = shape_text_for_parsed_font(
        &font,
        "filled",
        Script::Latin,
        Language::EnglishUS,
        BidiDirection::Ltr,
        &style,
    )
    .expect("shaping should succeed");

    // Without ligatures: f, i, l, l, e, d = 6 glyphs
    // With fi ligature:  fi, l, l, e, d = 5 glyphs
    assert!(
        glyphs.len() <= 5,
        "Expected fi ligature to reduce glyph count from 6 to 5, got {} glyphs",
        glyphs.len()
    );

    // Verify the ligature glyph ID differs from standalone 'f'
    let f_only = shape_text_for_parsed_font(
        &font,
        "f",
        Script::Latin,
        Language::EnglishUS,
        BidiDirection::Ltr,
        &style,
    )
    .expect("shaping 'f' should succeed");
    assert_ne!(
        glyphs[0].glyph_id, f_only[0].glyph_id,
        "Ligature glyph ID should differ from standalone 'f' glyph ID"
    );
}

#[test]
fn test_fl_ligature_is_substituted() {
    let font = match load_times_new_roman() {
        Some(f) => f,
        None => {
            eprintln!("Skipping: Times New Roman not found");
            return;
        }
    };

    let style = StyleProperties {
        font_size_px: 16.0,
        ..StyleProperties::default()
    };

    // "flow" should produce an fl ligature: fl, o, w = 3 glyphs
    let glyphs = shape_text_for_parsed_font(
        &font,
        "flow",
        Script::Latin,
        Language::EnglishUS,
        BidiDirection::Ltr,
        &style,
    )
    .expect("shaping should succeed");

    assert_eq!(
        glyphs.len(),
        3,
        "Expected fl ligature to reduce 'flow' from 4 to 3 glyphs, got {}",
        glyphs.len()
    );
}

#[test]
fn test_no_ligature_without_fi() {
    let font = match load_times_new_roman() {
        Some(f) => f,
        None => {
            eprintln!("Skipping: Times New Roman not found");
            return;
        }
    };

    let style = StyleProperties {
        font_size_px: 16.0,
        ..StyleProperties::default()
    };

    // "hello" has no ligature opportunities - should stay at 5 glyphs
    let glyphs = shape_text_for_parsed_font(
        &font,
        "hello",
        Script::Latin,
        Language::EnglishUS,
        BidiDirection::Ltr,
        &style,
    )
    .expect("shaping should succeed");

    assert_eq!(glyphs.len(), 5, "Expected 5 glyphs for 'hello', got {}", glyphs.len());
}
