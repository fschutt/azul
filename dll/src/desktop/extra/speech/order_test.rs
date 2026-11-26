//! Which recognizer is asked first: by the language, by what the vocabulary holds, by what the app
//! asked for. The rule is a pure function of four facts, so the numbers behind it are tested here
//! and the chain only does what it says.

use super::{choose, Preference};
use super::super::recognizer::Kind;

#[test]
fn english_goes_to_the_built_in_model_when_it_is_ready() {
    for language in ["en", "en-US", "en_GB", "EN-us", " en "] {
        let order = choose(Preference::Automatic, language, false, true);
        assert_eq!(order.first, Kind::Builtin, "{language}");
        assert!(order.why.contains("English"), "{}", order.why);
    }
}

#[test]
fn german_other_languages_and_the_systems_own_language_go_to_the_system_recognizer() {
    for language in ["de", "de-DE", "fr-FR", "ja-JP", "", "  ", "-"] {
        let order = choose(Preference::Automatic, language, false, true);
        assert_eq!(order.first, Kind::Native, "{language:?}");
    }
    assert!(choose(Preference::Automatic, "de-DE", false, true).why.contains("German"));
    assert!(!choose(Preference::Automatic, "ja-JP", false, true).why.is_empty());
}

#[test]
fn an_identifier_in_the_vocabulary_puts_the_system_recognizer_first_even_in_english() {
    let order = choose(Preference::Automatic, "en-US", true, true);
    assert_eq!(order.first, Kind::Native);
    assert!(order.why.contains("identifier"), "{}", order.why);
}

#[test]
fn a_built_in_model_that_is_not_ready_is_not_put_first_on_its_own() {
    let order = choose(Preference::Automatic, "en-US", false, false);
    assert_eq!(order.first, Kind::Native);
    assert!(order.why.contains("not set up"), "{}", order.why);
}

#[test]
fn what_the_app_asked_for_wins_over_every_fact() {
    for (language, identifiers, ready) in [("en", false, true), ("de", true, false), ("", true, true), ("en", true, false)] {
        let native = choose(Preference::NativeFirst, language, identifiers, ready);
        assert_eq!((native.first, native.why), (Kind::Native, "as the app asked"));
        let builtin = choose(Preference::BuiltinFirst, language, identifiers, ready);
        assert_eq!((builtin.first, builtin.why), (Kind::Builtin, "as the app asked"));
    }
}

#[test]
fn every_clause_is_short_enough_for_a_status_line() {
    for preference in [Preference::Automatic, Preference::NativeFirst, Preference::BuiltinFirst] {
        for language in ["en", "de", "", "ja"] {
            for identifiers in [false, true] {
                for ready in [false, true] {
                    let why = choose(preference, language, identifiers, ready).why;
                    assert!(!why.is_empty() && why.len() <= 90, "{why:?}");
                }
            }
        }
    }
}

#[test]
fn the_default_asks_nothing_special() {
    assert_eq!(Preference::default(), Preference::Automatic);
}
