//! The vocabulary: what is kept of the words an app hands in, which words of an answer count as
//! favoured, and which entries are identifiers.

use super::{key, looks_like_identifier, Vocabulary};
use super::super::recognizer::Word;

fn words(list: &[&str]) -> Vec<String> {
    list.iter().map(|w| (*w).to_string()).collect()
}

#[test]
fn a_word_is_compared_by_its_letters_and_digits_not_by_case_or_punctuation() {
    assert_eq!(key("Enum,"), "enum");
    assert_eq!(key("AzReview"), "azreview");
    assert_eq!(key("(CSS)."), "css");
    assert_eq!(key("don't"), "dont");
    assert_eq!(key("unwrap_or_else"), "unwrap_or_else", "an underscore belongs to the identifier");
    assert_eq!(key("Größe"), "größe", "letters of any alphabet stay");
    assert_eq!(key("..."), "");
    assert_eq!(key("  "), "");
}

#[test]
fn what_is_handed_in_is_cleaned_and_kept_to_the_limits() {
    let (vocabulary, note) = Vocabulary::from_words(
        &words(&["AzReview", "  Neckarbrücke ", "", "   ", "line\nbreak"]),
        200,
        64,
    );
    assert_eq!(note, None);
    assert_eq!(vocabulary.entries(), ["AzReview", "Neckarbrücke", "linebreak"]);
    assert_eq!(vocabulary.joined(), "AzReview\nNeckarbrücke\nlinebreak");
    assert!(!vocabulary.is_empty());

    let many: Vec<String> = (0..500).map(|i| format!("word{i}")).collect();
    let (vocabulary, note) = Vocabulary::from_words(&many, 200, 64);
    assert_eq!(vocabulary.entries().len(), 200);
    assert_eq!(vocabulary.entries()[199], "word199", "the first ones are kept");
    let note = note.expect("something was cut");
    assert!(note.contains("cut to 200 words of at most 64 characters"), "{note}");

    let (vocabulary, note) = Vocabulary::from_words(&["x".repeat(500)], 200, 64);
    assert_eq!(vocabulary.entries()[0].chars().count(), 64);
    assert!(note.is_some(), "a word that was shortened is said so");

    let (nothing, note) = Vocabulary::from_words(&[], 200, 64);
    assert!(nothing.is_empty());
    assert_eq!(nothing.joined(), "");
    assert_eq!(note, None);
}

#[test]
fn the_first_entries_are_what_a_recognizer_that_takes_fewer_gets() {
    let many: Vec<String> = (0..300).map(|i| format!("word{i}")).collect();
    let (vocabulary, _) = Vocabulary::from_words(&many, 200, 64);
    let (fewer, left_out) = vocabulary.first(100);
    assert_eq!(fewer.entries().len(), 100);
    assert_eq!(fewer.entries()[99], "word99");
    assert!(left_out);
    assert!(fewer.marks("word99") && !fewer.marks("word100"), "the words of the kept entries only");
    // What fits is returned whole, with no word about it.
    let (same, left_out) = vocabulary.first(500);
    assert_eq!(same.entries().len(), 200);
    assert!(!left_out);
}

fn marked(vocabulary: &Vocabulary, text: &[&str]) -> Vec<(String, bool)> {
    let mut list: Vec<Word> = text.iter().map(|t| Word::new(t, 0, 100, 0.9)).collect();
    vocabulary.mark(&mut list);
    list.into_iter().map(|w| (w.text, w.biased)).collect()
}

#[test]
fn the_words_of_the_vocabulary_are_marked_in_an_answer() {
    let (vocabulary, _) = Vocabulary::from_words(&words(&["ENUM", "azreview", "speech session"]), 200, 64);
    assert_eq!(
        marked(&vocabulary, &["Rename", "the", "enum,", "AzReview", "and", "the", "speech", "session."]),
        [
            ("Rename", false),
            ("the", false),
            ("enum,", true),
            ("AzReview", true),
            ("and", false),
            ("the", false),
            ("speech", true),
            ("session.", true),
        ]
        .map(|(t, b)| (t.to_string(), b))
    );
}

#[test]
fn nothing_is_marked_without_a_vocabulary_or_by_a_word_that_is_only_punctuation() {
    let (nothing, _) = Vocabulary::from_words(&[], 200, 64);
    assert!(marked(&nothing, &["enum", "AzReview"]).iter().all(|(_, b)| !b));
    // An entry that is only punctuation has no letters to compare: it marks no word, not every "word" of punctuation.
    let (odd, _) = Vocabulary::from_words(&words(&["...", "-"]), 200, 64);
    assert!(marked(&odd, &["...", "-", "hello"]).iter().all(|(_, b)| !b));
}

#[test]
fn an_entry_that_looks_like_an_identifier_is_found() {
    for identifier in [
        "CssError",
        "SpeechSession",
        "AzReview",
        "DataTableCell",
        "ScrollManager",
        "HttpRequest",
        "HTTPRequest",
        "unwrap_or_else",
        "snake_case",
        "cssError",
        "iPhone",
        "x86_64",
    ] {
        assert!(looks_like_identifier(identifier), "{identifier}");
    }
    for plain in ["CSS", "API", "JSON", "enum", "struct", "Rust", "Tübingen", "Neckarbrücke", "css", "A1", "", "_"] {
        assert!(!looks_like_identifier(plain), "{plain}");
    }
    let (with, _) = Vocabulary::from_words(&words(&["enum", "CssError"]), 200, 64);
    assert!(with.has_identifier());
    let (without, _) = Vocabulary::from_words(&words(&["enum", "CSS", "speech session"]), 200, 64);
    assert!(!without.has_identifier());
    // A phrase is an identifier-like entry when one of its words is.
    let (phrase, _) = Vocabulary::from_words(&words(&["open the AzReview"]), 200, 64);
    assert!(phrase.has_identifier());
    assert!(!Vocabulary::default().has_identifier());
}
