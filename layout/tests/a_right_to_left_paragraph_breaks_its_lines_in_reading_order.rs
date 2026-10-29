#![cfg(feature = "text_layout")]
//! A right-to-left paragraph that mixes in left-to-right words is broken into
//! lines in reading (logical) order and only then put in visual order, line by
//! line (UAX 9 rules L1 and L2 apply per line). The line breaker used to see
//! the paragraph's runs already in visual order: the run read first stood
//! last, so its space before "Hello" looked like the line's trailing space
//! and was trimmed, and a wrapped paragraph started with its last words.
//!
//! In "مرحبا Hello بالعالم 123" under `direction: rtl` the bidi algorithm puts
//! every space at the paragraph's level (rule N2 between Arabic and Latin, N1
//! between Arabic and the number), so a browser draws, left to right,
//! "123 بالعالم Hello مرحبا" with all three spaces.
//!
//! Fonts: the Noto Sans Arabic subset of `tests/fonts/README.md` (section 7)
//! for the Arabic runs and the built-in Azul Mock Mono for the Latin ones.

use std::{collections::HashMap, path::PathBuf, sync::Arc};

use azul_css::props::basic::FontRef;
use azul_layout::{
    font::parsed::ParsedFont,
    parsed_font_to_font_ref,
    text3::{
        cache::{
            create_logical_items, perform_fragment_layout, reorder_logical_items,
            shape_visual_items, AvailableSpace, BidiDirection, BreakCursor, FontChainKey,
            FontStack, InlineContent, LoadedFonts, ShapedItem, StyleProperties, StyledRun,
            UnicodeBidi, UnifiedConstraints, UnifiedLayout,
        },
        mock_fonts::MOCK_MONO_TTF,
    },
};
use rust_fontconfig::{FcFontCache, FontBytes, FontFallbackChain, FontId};

fn font_ref(bytes: &[u8]) -> FontRef {
    let shared = Arc::new(FontBytes::Owned(Arc::from(bytes)));
    let mut warnings = Vec::new();
    let parsed = ParsedFont::from_bytes(bytes, 0, &mut warnings)
        .expect("the test font parses")
        .with_source_bytes(shared);
    parsed_font_to_font_ref(parsed)
}

/// "مرحبا Hello بالعالم 123" in a `direction: rtl` paragraph `width` wide,
/// the Arabic in Noto Sans Arabic and the Latin in Azul Mock Mono, at 20px.
fn lay_out(width: AvailableSpace) -> UnifiedLayout {
    let path = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fonts"))
        .join("NotoSansArabic-DotsAndMarks.ttf");
    let arabic = font_ref(&std::fs::read(&path).expect("read the Arabic test font"));
    let latin = font_ref(MOCK_MONO_TTF);
    let run = |text: &str, font: &FontRef| {
        InlineContent::Text(StyledRun {
            text: Arc::from(text),
            style: Arc::new(StyleProperties {
                font_stack: FontStack::Ref(font.clone()),
                font_size_px: 20.0,
                ..StyleProperties::default()
            }),
            logical_start_byte: 0,
            source_node_id: None,
        })
    };
    let content = vec![
        run("مرحبا ", &arabic),
        run("Hello", &latin),
        run(" بالعالم ", &arabic),
        run("123", &latin),
    ];
    let mut loaded: LoadedFonts<FontRef> = LoadedFonts::new();
    loaded.insert(FontId::new(), arabic);
    loaded.insert(FontId::new(), latin);
    let logical = create_logical_items(&content, &[], &mut None);
    let runs = reorder_logical_items(&logical, BidiDirection::Rtl, UnicodeBidi::Normal, &mut None)
        .expect("bidi analysis");
    let chains: HashMap<FontChainKey, FontFallbackChain> = HashMap::new();
    let shaped = shape_visual_items(&runs, &chains, &FcFontCache::default(), &loaded, &mut None)
        .expect("shaping");
    let mut cursor = BreakCursor::new(&shaped);
    let constraints = UnifiedConstraints {
        available_width: width,
        direction: Some(BidiDirection::Rtl),
        ..UnifiedConstraints::default()
    };
    perform_fragment_layout(&mut cursor, &logical, &constraints, &mut None, &loaded)
        .expect("line layout")
}

/// Each cluster of `layout` as (line, text, x, advance), lines top to bottom
/// and each line left to right.
fn clusters(layout: &UnifiedLayout) -> Vec<(usize, String, f32, f32)> {
    let mut out: Vec<(usize, String, f32, f32)> = layout
        .items
        .iter()
        .filter_map(|p| match &p.item {
            ShapedItem::Cluster(c) => {
                Some((p.line_index, c.text().to_string(), p.position.x, c.advance))
            }
            _ => None,
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0).then(a.2.total_cmp(&b.2)));
    out
}

#[test]
fn the_space_before_a_latin_word_in_a_right_to_left_line_is_kept() {
    let drawn = clusters(&lay_out(AvailableSpace::MaxContent));
    let texts: Vec<&str> = drawn.iter().map(|c| c.1.as_str()).collect();
    let spaces: Vec<&(usize, String, f32, f32)> = drawn.iter().filter(|c| c.1 == " ").collect();
    assert_eq!(
        spaces.len(),
        3,
        "all three spaces should be laid out, left to right the line reads {texts:?}"
    );
    assert!(
        spaces.iter().all(|s| s.3 > 0.0),
        "every space should have its advance: {drawn:?}"
    );
    // Left to right: ... "Hello" " " "مرحبا", whose last letter, the alef,
    // stands leftmost.
    let o = texts
        .iter()
        .rposition(|t| *t == "o")
        .unwrap_or_else(|| panic!("Hello is laid out: {texts:?}"));
    assert_eq!(
        (texts.get(o + 1).copied(), texts.get(o + 2).copied()),
        (Some(" "), Some("ا")),
        "a space should stand between Hello and مرحبا, left to right the line reads {texts:?}"
    );
    let (space, alef) = (&drawn[o + 1], &drawn[o + 2]);
    assert!(
        drawn[o].2 + drawn[o].3 <= space.2 + 0.01 && space.2 + space.3 <= alef.2 + 0.01,
        "the space should sit between Hello and مرحبا: {drawn:?}"
    );
}

#[test]
fn a_wrapped_right_to_left_paragraph_starts_with_its_first_word() {
    // A line too narrow for any two words puts each word on a line of its
    // own, in reading order: مرحبا first, 123 last. The hah is only in
    // مرحبا and the ain only in بالعالم.
    let drawn = clusters(&lay_out(AvailableSpace::Definite(1.0)));
    let line_of = |letter: &str| {
        drawn
            .iter()
            .find(|c| c.1 == letter)
            .map(|c| c.0)
            .unwrap_or_else(|| panic!("{letter} is laid out: {drawn:?}"))
    };
    assert_eq!(
        [line_of("ح"), line_of("H"), line_of("ع"), line_of("1")],
        [0, 1, 2, 3],
        "the words should fill the lines in reading order, the lines hold {drawn:?}"
    );
}
