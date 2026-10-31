//! A hyphenated word's min-content is its widest fragment with its hyphen.
//!
//! With `hyphens: auto` the min-content takes every break of a word, the
//! dictionary's too (as Chrome measures `width: min-content`): its lines are
//! the word's fragments between its dictionary breaks, each but the last
//! ending in a hyphen, and the intrinsic min-content width is the widest of
//! those lines. The rest of a word carried to the next line keeps the
//! word's own breaks: "incomprehensible" is "in-com-pre-hen-si-ble",
//! though the dictionary breaks "sible" on its own nowhere.

use std::{collections::HashMap, sync::Arc};

use azul_css::props::basic::FontRef;
use azul_layout::text3::cache::{
    AvailableSpace, FontChainKey, FontStack, Hyphens, InlineContent, LoadedFonts,
    StyleProperties, StyledRun, TextShapingCache, UnifiedConstraints,
};
use hyphenation::{Hyphenator, Language, Load, Standard};
use rust_fontconfig::{FcFontCache, FontFallbackChain, FontId};

use crate::calmet_lines::{calmet_ui, lay_out, lines, FONT_SIZE};

/// A long word the en-US dictionary breaks five times.
const WORD: &str = "incomprehensible";

/// The fragments of [`WORD`] between its en-US dictionary breaks.
fn fragments() -> Vec<String> {
    let dictionary = Standard::from_embedded(Language::EnglishUS).expect("en-US patterns");
    let breaks = dictionary.hyphenate(WORD).breaks;
    let mut fragments = Vec::new();
    let mut start = 0;
    for byte in breaks.into_iter().chain(std::iter::once(WORD.len())) {
        fragments.push(WORD[start..byte].to_string());
        start = byte;
    }
    assert!(
        fragments.len() > 2,
        "harness: the dictionary breaks {WORD:?} more than once: {fragments:?}"
    );
    fragments
}

/// Hyphenates the paragraph in American English.
fn hyphenate_in_english(paragraph: &mut UnifiedConstraints) {
    paragraph.hyphenation = Hyphens::Auto;
    paragraph.hyphenation_language = Some(Language::EnglishUS);
}

/// The texts and the ends of the lines of [`WORD`] laid out at its
/// min-content.
fn min_content_lines(font: &FontRef) -> Vec<(String, f32)> {
    let layout = lay_out(font, WORD, 0.0, |_| {}, |paragraph| {
        hyphenate_in_english(paragraph);
        paragraph.available_width = AvailableSpace::MinContent;
    });
    let laid_out = lines(&layout);
    laid_out
        .iter()
        .map(|line| (line.text(), line.end))
        .collect()
}

/// The intrinsic min-content width the solver sizes a box around [`WORD`]
/// with.
fn intrinsic_min_content(font: &FontRef) -> f32 {
    let content = vec![InlineContent::Text(StyledRun {
        text: Arc::from(WORD),
        style: Arc::new(StyleProperties {
            font_stack: FontStack::Ref(font.clone()),
            font_size_px: FONT_SIZE,
            ..StyleProperties::default()
        }),
        logical_start_byte: 0,
        source_node_id: None,
    })];
    let mut constraints = UnifiedConstraints::default();
    hyphenate_in_english(&mut constraints);
    let mut fonts: LoadedFonts<FontRef> = LoadedFonts::new();
    fonts.insert(FontId::new(), font.clone());
    let chains: HashMap<FontChainKey, FontFallbackChain> = HashMap::new();
    let fc_cache = FcFontCache::default();
    let mut cache = TextShapingCache::new();
    let sizes = cache
        .measure_intrinsic_widths(
            &content,
            &[],
            &constraints,
            &chains,
            &fc_cache,
            &fonts,
            &mut None,
        )
        .expect("the word is measured");
    sizes.min_content_width
}

#[test]
fn a_hyphenated_words_min_content_lines_are_its_dictionary_fragments() {
    let fragments = fragments();
    let last = fragments.len() - 1;
    let expected: Vec<String> = fragments
        .iter()
        .enumerate()
        .map(|(index, fragment)| {
            if index == last {
                fragment.clone()
            } else {
                format!("{fragment}-")
            }
        })
        .collect();
    let got: Vec<String> = min_content_lines(&calmet_ui())
        .into_iter()
        .map(|(text, _)| text)
        .collect();
    assert_eq!(
        got, expected,
        "at its min-content {WORD:?} is laid out in the lines {got:?}, not in its dictionary \
         fragments, each but the last with its hyphen"
    );
}

#[test]
fn a_hyphenated_words_intrinsic_min_content_is_its_widest_fragment_line() {
    let font = calmet_ui();
    let fragment_lines = min_content_lines(&font);
    let widest = fragment_lines
        .iter()
        .map(|(_, end)| *end)
        .fold(0.0_f32, f32::max);
    let whole = lay_out(&font, WORD, 10_000.0, |_| {}, hyphenate_in_english);
    let whole_width = lines(&whole).first().map_or(0.0, |line| line.end);
    assert!(
        widest < whole_width,
        "harness: the widest fragment line ({widest:.2}) is narrower than the word \
         ({whole_width:.2}): {fragment_lines:?}"
    );
    let measured = intrinsic_min_content(&font);
    assert!(
        (measured - widest).abs() <= 0.05,
        "the intrinsic min-content of {WORD:?} is {measured:.2}, the widest line its min-content \
         layout holds {widest:.2} ({fragment_lines:?}): the measurement and the line breaker do \
         not agree where the word breaks"
    );
}
