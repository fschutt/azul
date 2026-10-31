//! AzMeet's words in azul's localization: every key its source names is in resources/en.ftl
//! and resources/de.ftl and both parse; the engine gets them with appkit's; a language switch
//! changes a label. (The Language row is appkit's settings page's.) No window.

use std::path::Path;

use azul_appkit::{
    l10n::{set_locale, t},
    l10n_check::check,
};

use crate::l10n::{sources, DE, EN};

#[test]
fn every_key_of_azmeet_source_is_in_english_and_german_and_both_parse() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let problems = check(&manifest.join("src"), &["azmeet"], EN, DE);
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_engine_gets_azmeet_words_and_appkits_in_english_and_german() {
    let sources = sources();
    let tags: Vec<&str> = sources.iter().map(|(tag, _)| tag.as_str()).collect();
    assert_eq!(tags, vec!["en", "de"]);
    for (tag, source) in &sources {
        for key in ["kit-general-language", "azmeet-new-meeting"] {
            assert!(source.contains(&format!("{key} = ")), "{tag} lacks {key}");
        }
    }
}

#[test]
fn switching_the_language_changes_a_label_of_azmeet() {
    crate::l10n::in_english();
    assert_eq!(t("azmeet-new-meeting"), "New meeting");
    set_locale("de-DE");
    assert_eq!(t("azmeet-new-meeting"), "Neue Besprechung");
    set_locale("en-US");
}

/// rooms.rs is compiled into azul-calendar-core too, which has neither the kit nor AzMeet's
/// modules (no libazul): AzMeet says a meeting's times elsewhere.
#[test]
fn the_rooms_file_names_neither_the_kit_nor_azmeets_words_so_the_calendar_core_compiles_it() {
    let source = include_str!("rooms.rs");
    for name in ["azul_appkit", "crate::l10n", "azul::"] {
        assert!(!source.contains(name), "rooms.rs names {name}");
    }
}
