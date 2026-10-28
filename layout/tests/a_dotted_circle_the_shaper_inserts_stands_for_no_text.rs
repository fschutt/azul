#![cfg(feature = "text_layout")]
//! A vowel sign with no consonant before it is a broken syllable, and the
//! shaper draws it on a dotted circle (U+25CC) it inserts. That glyph stands
//! for no text: it must not take the source bytes of the characters after
//! it, which pushed the following glyphs' clusters past the end of the text
//! and dropped the consonant after the broken syllable from the line.
//!
//! The font is a small subset of Noto Sans Devanagari
//! (`tests/fonts/README.md`, section 9); the expected clusters are what
//! `HarfBuzz` gives for the same font and text.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use azul_css::props::basic::FontRef;
use azul_layout::{
    font::parsed::ParsedFont,
    parsed_font_to_font_ref,
    text3::{
        cache::{
            create_logical_items, perform_fragment_layout, reorder_logical_items,
            shape_visual_items, AvailableSpace, BidiDirection, BreakCursor, FontChainKey,
            FontStack, Glyph, InlineContent, LoadedFonts, StyleProperties, StyledRun,
            UnicodeBidi, UnifiedConstraints,
        },
        default::shape_text_for_parsed_font,
        glyphs::get_glyph_positions,
        script::{Language, Script},
    },
};
use rust_fontconfig::{FcFontCache, FontBytes, FontFallbackChain, FontId};

fn devanagari() -> ParsedFont {
    let path = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts"))
        .join("NotoSansDevanagari-BrokenSyllables.ttf");
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let shared = Arc::new(FontBytes::Owned(Arc::from(bytes.as_slice())));
    let mut warnings = Vec::new();
    ParsedFont::from_bytes(&bytes, 0, &mut warnings)
        .expect("the Devanagari subset parses")
        .with_source_bytes(shared)
}

/// `text` shaped as Devanagari.
fn shape(text: &str) -> Vec<Glyph> {
    let style = StyleProperties {
        font_size_px: 100.0,
        ..StyleProperties::default()
    };
    shape_text_for_parsed_font(
        &devanagari(),
        text,
        Script::Devanagari,
        Language::EnglishUS,
        BidiDirection::Ltr,
        &style,
    )
    .expect("Devanagari text shapes")
}

/// Each glyph as (the character it was shaped from, cluster, byte length).
fn spans(glyphs: &[Glyph]) -> Vec<(char, u32, usize)> {
    glyphs
        .iter()
        .map(|g| (g.codepoint, g.cluster, g.logical_byte_len))
        .collect()
}

#[test]
fn a_dotted_circle_before_a_lone_vowel_sign_stands_for_no_text() {
    // "ाक": the aa sign (bytes 0..3) has no consonant, so the shaper puts a
    // dotted circle in front of it; ka follows at byte 3.
    let glyphs = shape("ाक");
    assert_eq!(
        spans(&glyphs),
        [('◌', 0, 0), ('ा', 0, 3), ('क', 3, 3)],
        "the inserted dotted circle should take no bytes and stand in the aa sign's \
         cluster, so ka keeps its cluster 3 inside the 6-byte text"
    );
}

#[test]
fn a_dotted_circle_after_a_moved_vowel_sign_belongs_to_it() {
    // "िक": the i sign is drawn before its base, so the shaper moves it in
    // front of the dotted circle it inserts; the circle still belongs to the
    // i sign, and ka keeps its own cluster.
    let glyphs = shape("िक");
    assert_eq!(
        spans(&glyphs),
        [('ि', 0, 3), ('◌', 0, 0), ('क', 3, 3)],
        "the dotted circle should take no bytes and stay in the i sign's cluster"
    );
}

#[test]
fn a_dotted_circle_of_the_text_is_still_text() {
    // "◌िक" writes the i sign on a dotted circle of its own, so nothing is
    // inserted; the shaper still draws the i sign first. Every byte of the
    // text belongs to a glyph, the dotted circle's three included, and ka
    // keeps its cluster 6.
    let text = "◌िक";
    let glyphs = shape(text);
    let bytes: usize = glyphs.iter().map(|g| g.logical_byte_len).sum();
    assert_eq!(
        bytes,
        text.len(),
        "the glyphs should stand for all {} bytes of the text: {:?}",
        text.len(),
        spans(&glyphs)
    );
    assert!(
        spans(&glyphs).contains(&('◌', 3, 3)) && spans(&glyphs).contains(&('क', 6, 3)),
        "the text's dotted circle should take its 3 bytes and ka its cluster 6: {:?}",
        spans(&glyphs)
    );
}

#[test]
fn the_consonant_after_a_broken_syllable_is_drawn() {
    // Laid out as a line, a cluster past the end of the text was dropped:
    // "ाक" lost its ka.
    let parsed = devanagari();
    let ka = parsed.lookup_glyph_index(u32::from('क')).expect("the subset maps ka");
    let font_ref = parsed_font_to_font_ref(parsed);
    let style = Arc::new(StyleProperties {
        font_stack: FontStack::Ref(font_ref.clone()),
        font_size_px: 100.0,
        ..StyleProperties::default()
    });
    let content = vec![InlineContent::Text(StyledRun {
        text: Arc::from("ाक"),
        style,
        logical_start_byte: 0,
        source_node_id: None,
    })];
    let mut loaded: LoadedFonts<FontRef> = LoadedFonts::new();
    loaded.insert(FontId::new(), font_ref);
    let logical = create_logical_items(&content, &[], &mut None);
    let visual =
        reorder_logical_items(&logical, BidiDirection::Ltr, UnicodeBidi::Normal, &mut None)
            .expect("bidi reorder");
    let chains: HashMap<FontChainKey, FontFallbackChain> = HashMap::new();
    let shaped = shape_visual_items(&visual, &chains, &FcFontCache::default(), &loaded, &mut None)
        .expect("shaping");
    let mut cursor = BreakCursor::new(&shaped);
    let constraints = UnifiedConstraints {
        available_width: AvailableSpace::MaxContent,
        ..UnifiedConstraints::default()
    };
    let layout = perform_fragment_layout(&mut cursor, &logical, &constraints, &mut None, &loaded)
        .expect("line layout");
    let drawn: Vec<u16> = get_glyph_positions(&layout)
        .iter()
        .map(|p| p.glyph_id)
        .collect();
    assert!(
        drawn.contains(&ka),
        "ka (glyph {ka}) should be drawn after the broken syllable, the line drew {drawn:?}"
    );
}
