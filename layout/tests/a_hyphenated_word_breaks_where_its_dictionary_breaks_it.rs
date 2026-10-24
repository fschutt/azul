//! A hyphenated word breaks where its dictionary breaks it, and the part
//! left on the line fits the line.
//!
//! The hyphenation dictionary names a word's breaks as byte offsets into the
//! word. Bytes, characters and clusters only agree for ASCII text with one
//! character per cluster, so a break after a two-byte letter has to be found
//! in the word's clusters by its byte, and a break inside a ligature (between
//! the f and the i of an "fi" glyph) is no place to split at all.
//!
//! Calmet's Dictionary (pdfocr engine issue 6) showed both ways it goes wrong
//! when the offsets are read as letters: "Præc-eperat" where the dictionary
//! says "Præ-ceperat", and a justified line ending in "Instrum-" 27 pt past
//! its column, because after the long-s-t ligature the line took one cluster
//! more than it had measured.
//!
//! Each word is laid out after "xxxxxx " in boxes from 60 px to 300 px wide,
//! without and with letter-spacing (the pen adds it after every character,
//! the hyphen included): every line that ends in a hyphen holds the word up
//! to one of the dictionary's breaks, and fits its box.

use azul_css::props::basic::FontRef;
use azul_layout::text3::cache::{Hyphens, ShapedCluster, Spacing};
use hyphenation::{Hyphenator, Language, Load, Standard};

use crate::calmet_lines::{calmet_ui, is_inserted_hyphen, lay_out, lines};

/// The ASCII word in front of the word under test, so the word starts mid-line.
const LEAD: &str = "xxxxxx ";

/// Words with a two-byte letter or a ligature before a break.
const WORDS: [&str; 4] = ["Præceperat", "quæstionibus", "difficulties", "significant"];

/// The dictionary's breaks of `word`, as the number of letters before each.
fn dictionary_breaks(word: &str) -> Vec<usize> {
    let dictionary = Standard::from_embedded(Language::EnglishUS).expect("en-US patterns");
    let breaks: Vec<usize> = dictionary
        .hyphenate(word)
        .breaks
        .iter()
        .map(|&byte| word[..byte].chars().count())
        .collect();
    assert!(!breaks.is_empty(), "harness: the dictionary breaks {word}");
    breaks
}

/// Every hyphenated line of `word` laid out after [`LEAD`] in boxes from 60
/// px to 300 px wide: (box width, letters of the word up to the line's end,
/// the line's end with the letter-spacing after its hyphen).
fn hyphenated_lines(font: &FontRef, word: &str, letter_spacing: f32) -> Vec<(f32, usize, f32)> {
    let lead_letters = LEAD.chars().filter(|c| !c.is_whitespace()).count();
    let mut found = Vec::new();
    for step in 0..=40 {
        let width = 60.0 + 6.0 * step as f32;
        let layout = lay_out(
            font,
            &format!("{LEAD}{word}"),
            width,
            |style| style.letter_spacing = Spacing::PxF(letter_spacing),
            |paragraph| {
                paragraph.hyphenation = Hyphens::Auto;
                paragraph.hyphenation_language = Some(Language::EnglishUS);
            },
        );
        let mut letters = 0;
        for line in lines(&layout) {
            for c in &line.clusters {
                if !is_inserted_hyphen(c) {
                    letters += c.text().chars().filter(|c| !c.is_whitespace()).count();
                }
            }
            if line.clusters.last().is_some_and(|c| is_inserted_hyphen(c)) {
                found.push((
                    width,
                    letters.saturating_sub(lead_letters),
                    line.end + letter_spacing,
                ));
            }
        }
    }
    found
}

#[test]
fn calmet_ui_ligates_fi_and_ffi_into_one_cluster() {
    // The harness's premise: without the ligatures the words below test
    // nothing about clusters of several letters.
    let layout = lay_out(
        &calmet_ui(),
        "difficulties significant",
        1000.0,
        |_| {},
        |_| {},
    );
    let texts: Vec<&str> = layout
        .items
        .iter()
        .filter_map(|item| item.item.as_cluster().map(ShapedCluster::text))
        .collect();
    assert!(
        texts.contains(&"ffi") && texts.contains(&"fi"),
        "Calmet UI's liga makes one cluster of ffi and of fi: {texts:?}"
    );
}

#[test]
fn a_hyphenated_line_ends_at_one_of_the_dictionarys_breaks() {
    let font = calmet_ui();
    for word in WORDS {
        let breaks = dictionary_breaks(word);
        for letter_spacing in [0.0, 2.0] {
            let found = hyphenated_lines(&font, word, letter_spacing);
            assert!(
                !found.is_empty(),
                "{word} (letter-spacing {letter_spacing}px) is hyphenated in some box from 60 \
                 to 300 px"
            );
            for (width, letters, _) in found {
                let part: String = word.chars().take(letters).collect();
                assert!(
                    breaks.contains(&letters),
                    "in a {width}px box (letter-spacing {letter_spacing}px) {word} breaks after \
                     \"{part}-\", where its dictionary does not break it (its breaks: after \
                     {breaks:?} letters)"
                );
            }
        }
    }
}

#[test]
fn a_hyphenated_line_fits_its_box() {
    let font = calmet_ui();
    for word in WORDS {
        for letter_spacing in [0.0, 2.0] {
            for (width, letters, end) in hyphenated_lines(&font, word, letter_spacing) {
                let part: String = word.chars().take(letters).collect();
                assert!(
                    end <= width + 0.5,
                    "the hyphenated line \"{LEAD}{part}-\" (letter-spacing {letter_spacing}px) \
                     fits its {width}px box: it ends at {end:.2}"
                );
            }
        }
    }
}
