#![cfg(feature = "text_layout")]
//! What a real Arabic font does to its letters reaches the layout the way the
//! font means it: a dot that a one-to-many substitution splits off its letter
//! stays in that letter's cluster.
//!
//! The font is a small subset of Noto Sans Arabic (`tests/fonts/README.md`,
//! section 7). It has 1000 units per em, so at a font size of 1000px a pixel
//! is a font unit; the expected values are what HarfBuzz gives for the same
//! font and text.

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

/// The font size at which one pixel is one unit of the 1000-unit fonts here.
const ONE_PIXEL_PER_UNIT: f32 = 1000.0;

fn font(name: &str) -> ParsedFont {
    let path = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts")).join(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let shared = Arc::new(FontBytes::Owned(Arc::from(bytes.as_slice())));
    let mut warnings = Vec::new();
    ParsedFont::from_bytes(&bytes, 0, &mut warnings)
        .unwrap_or_else(|| panic!("parse {name}"))
        .with_source_bytes(shared)
}

fn noto() -> ParsedFont {
    font("NotoSansArabic-DotsAndMarks.ttf")
}

/// `text` shaped right to left as Arabic, one pixel per font unit.
fn shape_arabic(font: &ParsedFont, text: &str) -> Vec<Glyph> {
    let style = StyleProperties {
        font_size_px: ONE_PIXEL_PER_UNIT,
        ..StyleProperties::default()
    };
    shape_text_for_parsed_font(
        font,
        text,
        Script::Arabic,
        Language::EnglishUS,
        BidiDirection::Rtl,
        &style,
    )
    .expect("Arabic text shapes")
}

/// The shaped glyphs in one line each, for the assert messages.
fn describe(glyphs: &[Glyph]) -> String {
    glyphs
        .iter()
        .map(|g| {
            format!(
                "gid {} cluster {} bytes {}+{} {:?} advance {} offset ({}, {})",
                g.glyph_id,
                g.cluster,
                g.logical_byte_index,
                g.logical_byte_len,
                g.codepoint,
                g.advance,
                g.offset.x,
                g.offset.y
            )
        })
        .collect::<Vec<_>>()
        .join("\n  ")
}

/// The glyphs that stand for no text of their own: the dots the font split
/// off their letters.
fn split_off(glyphs: &[Glyph]) -> Vec<&Glyph> {
    glyphs.iter().filter(|g| g.logical_byte_len == 0).collect()
}

#[test]
fn a_dot_split_off_its_letter_keeps_the_letters_cluster() {
    // The font draws the initial beh of "بالعالم" as a dotless beh and a
    // separate dot below it. The dot belongs to the beh at byte 0, not to
    // the alef at byte 2 that comes after it.
    let glyphs = shape_arabic(&noto(), "بالعالم");
    let dots = split_off(&glyphs);
    assert_eq!(
        dots.len(),
        1,
        "the beh's dot should be the one glyph without text of its own:\n  {}",
        describe(&glyphs)
    );
    let dot = dots[0];
    assert_eq!(
        dot.codepoint,
        'ب',
        "the dot should stand for the beh it was split off:\n  {}",
        describe(&glyphs)
    );
    assert_eq!(
        (dot.cluster, dot.logical_byte_index),
        (0, 0),
        "the beh's dot landed in the alef's cluster instead of the beh's cluster 0:\n  {}",
        describe(&glyphs)
    );
}

#[test]
fn the_dots_of_the_last_letter_stay_inside_the_text() {
    // "تحدث" has a teh at byte 0 and a theh at byte 6, and the font splits
    // both into a dotless letter and their dots. Each letter's dots carry
    // that letter's cluster; the theh's must not point past the end of the
    // 8-byte text.
    let text = "تحدث";
    let glyphs = shape_arabic(&noto(), text);
    let dots: Vec<(char, u32)> = split_off(&glyphs)
        .iter()
        .map(|g| (g.codepoint, g.cluster))
        .collect();
    assert_eq!(
        dots,
        [('ت', 0), ('ث', 6)],
        "the dots should carry the clusters of the teh (0) and the theh (6) of a {}-byte \
         text, each dot got the cluster of whatever came after its letter:\n  {}",
        text.len(),
        describe(&glyphs)
    );
    let past_the_end: Vec<&Glyph> = glyphs
        .iter()
        .filter(|g| g.cluster as usize >= text.len())
        .collect();
    assert!(
        past_the_end.is_empty(),
        "no glyph should have a cluster past the end of the text:\n  {}",
        describe(&glyphs)
    );
}
