#![cfg(feature = "text_layout")]
//! Arabic shaped the way HarfBuzz and allsorts' own `Font::map_glyphs` shape
//! it: the letters in their contextual forms, and the marks reordered before
//! the font sees them (`allsorts::scripts::preprocess_text`), so a vowel mark
//! written before a shadda stacks ON the shadda. azul mapped the text as it
//! stood: "اللَّهِ" (fatha, then shadda) drew the fatha on the lam and the
//! shadda over it - in AzNotes, a PDF, and the printpdf web demo alike.
//!
//! The font is a small subset of Noto Sans Arabic (`tests/fonts/README.md`,
//! section 11). It has 1000 units per em, so at a font size of 1000px a pixel
//! is a font unit; the expected glyphs and offsets are what `HarfBuzz` 15.0.0
//! gives for the same font and text, in the text's (logical) order.

use std::{path::PathBuf, sync::Arc};

use azul_layout::{
    font::parsed::ParsedFont,
    text3::{
        cache::{BidiDirection, Glyph, StyleProperties},
        default::shape_text_for_parsed_font,
        script::{Language, Script},
    },
};
use rust_fontconfig::FontBytes;

fn noto() -> ParsedFont {
    let path = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts"))
        .join("NotoSansArabic-Sample.ttf");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let shared = Arc::new(FontBytes::Owned(Arc::from(bytes.as_slice())));
    let mut warnings = Vec::new();
    ParsedFont::from_bytes(&bytes, 0, &mut warnings)
        .expect("the Arabic subset parses")
        .with_source_bytes(shared)
}

fn shape(text: &str) -> Vec<Glyph> {
    let style = StyleProperties {
        font_size_px: 1000.0,
        ..StyleProperties::default()
    };
    shape_text_for_parsed_font(
        &noto(),
        text,
        Script::Arabic,
        Language::EnglishUS,
        BidiDirection::Rtl,
        &style,
    )
    .expect("Arabic text shapes")
}

fn ids(glyphs: &[Glyph]) -> Vec<u16> {
    glyphs.iter().map(|g| g.glyph_id).collect()
}

/// "Hello world" and Cairo: every letter in its contextual (joining) form,
/// the dots the font splits off beh and qaf, teh marbuta drawn as heh goal
/// with its two dots.
#[test]
fn letters_take_their_contextual_forms() {
    assert_eq!(
        ids(&shape("مرحبا بالعالم")),
        [37, 17, 15, 8, 48, 3, 1, 10, 48, 3, 30, 20, 3, 30, 35],
        "meem.init reh.fina hah.init beh.medi (+dot) alef.fina, beh.init (+dot) alef.fina \
         lam.init ain.medi alef.fina lam.init meem.fina"
    );
    assert_eq!(
        ids(&shape("القاهرة")),
        [2, 30, 22, 47, 3, 41, 17, 42, 47],
        "alef lam.init qaf.medi (dotless + two dots) alef.fina heh.init reh.fina teh-marbuta"
    );
}

/// "اللَّهِ" spells the second lam's marks fatha, shadda (U+064E U+0651, the
/// canonical order). The shadda goes first: on the lam, the fatha on it.
#[test]
fn a_fatha_written_before_a_shadda_stacks_on_it() {
    let glyphs = shape("\u{0627}\u{0644}\u{0644}\u{064E}\u{0651}\u{0647}\u{0650}");
    assert_eq!(
        ids(&glyphs),
        [2, 30, 28, 50, 52, 39, 53],
        "alef, lam.init, lam.medi, SHADDA, FATHA, heh.fina, kasra"
    );
    let at = |gid: u16| {
        let g = glyphs
            .iter()
            .find(|g| g.glyph_id == gid)
            .expect("the mark is drawn");
        (g.offset.x, g.offset.y)
    };
    let near = |a: (f32, f32), b: (f32, f32)| (a.0 - b.0).abs() <= 1.0 && (a.1 - b.1).abs() <= 1.0;
    assert!(
        near(at(50), (-27.0, 256.0)),
        "the shadda on the lam: {:?}",
        at(50)
    );
    assert!(
        near(at(52), (-35.0, 456.0)),
        "the fatha above the shadda: {:?}",
        at(52)
    );
}
