//! Tests of the hyphenation dictionaries `hyphens: auto` reads (`hyphenator_for`,
//! `get_hyphenator`): which languages a build carries, and what a language without a
//! dictionary does.
//!
//! The default `text_layout_hyphenation` embeds the English (US) patterns only, 91 KB; the
//! feature `text_layout_hyphenation_all` embeds the 82 dictionaries of the `hyphenation` crate
//! (2.9 MB). A language without an embedded dictionary is not hyphenated: no error reaches the
//! layout and no word breaks.

use super::*;

/// The constraints of a paragraph `hyphens: auto` of `language` sets.
fn auto(language: HyphenationLanguage) -> UnifiedConstraints {
    UnifiedConstraints {
        hyphenation: Hyphens::Auto,
        hyphenation_language: Some(language),
        ..UnifiedConstraints::default()
    }
}

/// The byte offsets `word` breaks at in `language`; empty when no dictionary is embedded.
fn breaks_of(language: HyphenationLanguage, word: &str) -> Vec<usize> {
    hyphenator_for(&auto(language))
        .map(|dictionary| dictionary.hyphenate(word).breaks)
        .unwrap_or_default()
}

#[test]
fn hyphens_auto_hyphenates_english_in_every_build_with_hyphenation() {
    let breaks = breaks_of(HyphenationLanguage::EnglishUS, "extraordinarily");
    assert!(
        !breaks.is_empty(),
        "the English patterns are embedded by the default feature and break this word"
    );
}

#[test]
fn hyphens_manual_and_none_read_no_dictionary_even_for_english() {
    for hyphens in [Hyphens::Manual, Hyphens::None] {
        let constraints = UnifiedConstraints {
            hyphenation: hyphens,
            ..auto(HyphenationLanguage::EnglishUS)
        };
        assert!(hyphenator_for(&constraints).is_none(), "{hyphens:?}");
    }
}

#[cfg(not(feature = "text_layout_hyphenation_all"))]
#[test]
fn hyphenation_of_a_language_without_an_embedded_dictionary_is_none() {
    for language in [
        HyphenationLanguage::German1996,
        HyphenationLanguage::French,
        HyphenationLanguage::Hungarian,
    ] {
        assert!(
            hyphenator_for(&auto(language)).is_none(),
            "{language:?} has no dictionary in the default build: no hyphenation"
        );
        assert!(
            matches!(
                get_hyphenator(language),
                Err(LayoutError::HyphenationError(_))
            ),
            "{language:?}: an error the callers read as no hyphenation, never a panic"
        );
        // asked again, the answer is the same
        assert!(hyphenator_for(&auto(language)).is_none());
    }
}

#[cfg(feature = "text_layout_hyphenation_all")]
#[test]
fn hyphenation_of_german_works_with_the_all_languages_feature() {
    let breaks = breaks_of(HyphenationLanguage::German1996, "Donaudampfschifffahrt");
    assert!(
        breaks.len() >= 2,
        "the German patterns are embedded by text_layout_hyphenation_all: {breaks:?}"
    );
    assert!(hyphenator_for(&auto(HyphenationLanguage::French)).is_some());
    assert!(hyphenator_for(&auto(HyphenationLanguage::Hungarian)).is_some());
}
