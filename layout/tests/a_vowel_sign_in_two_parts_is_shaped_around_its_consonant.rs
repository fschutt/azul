#![cfg(feature = "text_layout")]
//! A Bengali o sign (U+09CB) is drawn in two parts: an e sign before its
//! consonant and an aa sign after it. allsorts shapes it so only once the text
//! is preprocessed (`allsorts::scripts::preprocess_text`, which decomposes the
//! sign); shaped as it stands, the sign reached the Indic shaper whole and
//! allsorts-azul 0.17.2 failed an assertion (`indic.rs:1414`, `left != right`)
//! on "যশোর" (Jessore, a city on many maps of Bangladesh).
//!
//! The font is a small subset of Noto Sans Bengali Bold
//! (`tests/fonts/README.md`, section 10); the expected glyphs are what
//! `HarfBuzz` gives for the same font and text.

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

fn bengali() -> ParsedFont {
    let path = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts"))
        .join("NotoSansBengali-Jessore.ttf");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let shared = Arc::new(FontBytes::Owned(Arc::from(bytes.as_slice())));
    let mut warnings = Vec::new();
    ParsedFont::from_bytes(&bytes, 0, &mut warnings)
        .expect("the Bengali subset parses")
        .with_source_bytes(shared)
}

fn shape(text: &str) -> Vec<Glyph> {
    let style = StyleProperties {
        font_size_px: 100.0,
        ..StyleProperties::default()
    };
    shape_text_for_parsed_font(
        &bengali(),
        text,
        Script::Bengali,
        Language::EnglishUS,
        BidiDirection::Ltr,
        &style,
    )
    .expect("Bengali text shapes")
}

#[test]
fn jessore_is_shaped_with_the_o_sign_around_its_consonant() {
    // ya, sha, o sign, ra
    let text = "\u{09AF}\u{09B6}\u{09CB}\u{09B0}";
    let glyphs = shape(text);
    let ids: Vec<u16> = glyphs.iter().map(|g| g.glyph_id).collect();
    assert_eq!(
        ids,
        [4, 11, 6, 8, 5],
        "ya, the e sign (long) before sha, sha, the aa sign after it, ra - as HarfBuzz draws it, \
         not the o sign whole after sha"
    );
    // The glyphs' spans stay inside the 12-byte text and cover it once, in
    // order.
    let mut end = 0;
    for g in &glyphs {
        let (start, len) = (g.logical_byte_index, g.logical_byte_len);
        assert!(start + len <= text.len(), "{start}+{len} past the text");
        assert!(start >= end || len == 0, "spans overlap at {start}");
        if len > 0 {
            end = start + len;
        }
    }
    let covered: usize = glyphs.iter().map(|g| g.logical_byte_len).sum();
    assert_eq!(
        covered,
        text.len(),
        "every byte of the text belongs to one glyph"
    );
}

#[test]
fn the_decomposed_spelling_shapes_alike() {
    // The same word with the sign written in its two parts (U+09C7 U+09BE).
    let glyphs = shape("\u{09AF}\u{09B6}\u{09C7}\u{09BE}\u{09B0}");
    let ids: Vec<u16> = glyphs.iter().map(|g| g.glyph_id).collect();
    assert_eq!(ids, [4, 11, 6, 8, 5]);
}
