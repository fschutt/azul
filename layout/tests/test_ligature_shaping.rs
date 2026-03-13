//! Unit tests for OpenType ligature shaping via allsorts.
//!
//! Documents that ligature substitution depends on the font's GSUB table
//! having a `liga` feature under the relevant script. Fonts like Times New
//! Roman do NOT have `liga` under `latn` — they only have it for Arabic.
//! This matches HarfBuzz behavior (confirmed via `hb-shape`).

use azul_layout::font::parsed::ParsedFont;
use azul_layout::text3::cache::{BidiDirection, StyleProperties};
use azul_layout::text3::default::shape_text_for_parsed_font;
use azul_layout::text3::script::Script;
use hyphenation::Language;

fn load_times_new_roman() -> Option<ParsedFont> {
    let font_path = "/System/Library/Fonts/Supplemental/Times New Roman.ttf";
    let font_bytes = std::fs::read(font_path).ok()?;
    let mut warnings = Vec::new();
    ParsedFont::from_bytes(&font_bytes, 0, &mut warnings)
}

/// Times New Roman has NO `liga` feature under `latn` in its GSUB table.
/// Therefore fi ligature substitution does NOT occur — this matches HarfBuzz:
///   $ hb-shape --font-file="Times New Roman.ttf" --text="filled" --script=latn
///   [f=0+682|i=1+569|l=2+569|l=3+569|e=4+909|d=5+1024]
///
/// The fi ligature glyph exists at U+FB01 in cmap, but GSUB doesn't reference
/// it for Latin. Platform engines (CoreText, DirectWrite) may substitute it
/// outside the OpenType pipeline, but that's not allsorts' responsibility.
#[test]
fn test_times_new_roman_no_fi_ligature() {
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

    let glyphs = shape_text_for_parsed_font(
        &font,
        "filled",
        Script::Latin,
        Language::EnglishUS,
        BidiDirection::Ltr,
        &style,
    )
    .expect("shaping should succeed");

    // No GSUB liga for latn → 6 separate glyphs, no ligature
    assert_eq!(
        glyphs.len(),
        6,
        "Times New Roman has no GSUB liga for Latin; 'filled' should produce 6 glyphs, got {}",
        glyphs.len()
    );
}

/// Verify shaping produces correct glyph count for text without ligature opportunities.
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
