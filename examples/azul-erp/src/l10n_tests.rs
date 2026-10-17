//! AzERP's words in azul's localization: every key its source names is in resources/en.ftl and
//! resources/de.ftl and both parse; the engine gets them with appkit's; a language switch
//! changes a label. (The Language row is appkit's settings page's.) No window.

use std::path::Path;

use azul_appkit::{
    l10n::{set_locale, t},
    l10n_check::check,
};

use crate::l10n::{sources, DE, EN};

#[test]
fn every_key_of_azerp_source_is_in_english_and_german_and_both_parse() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let problems = check(&manifest.join("src"), &["azerp"], EN, DE);
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_engine_gets_azerp_words_and_appkits_in_english_and_german() {
    let sources = sources();
    let tags: Vec<&str> = sources.iter().map(|(tag, _)| tag.as_str()).collect();
    assert_eq!(tags, vec!["en", "de"]);
    for (tag, source) in &sources {
        for key in ["kit-general-language", "azerp-status-checked-out"] {
            assert!(source.contains(&format!("{key} = ")), "{tag} lacks {key}");
        }
    }
}

/// The views' labels are keyed by the ERP's own keys (`fields.book_value.label`), which no key in
/// the source names: every one of the English table has its message in both files.
#[test]
fn every_label_of_the_views_has_its_words_in_english_and_german() {
    let table: std::collections::BTreeMap<String, String> =
        serde_json::from_str(crate::views::LABELS_EN).expect("en.json");
    for key in table.keys() {
        let message = crate::l10n::view_label_key(key);
        for (tag, source) in [("en", EN), ("de", DE)] {
            assert!(
                source.contains(&format!("\n{message} = ")),
                "{tag} lacks {message} ({key})"
            );
        }
    }
    crate::l10n::in_english();
    let labels = crate::views::Labels::en();
    assert_eq!(labels.get("fields.book_value.label"), "Book value");
    set_locale("de-DE");
    assert_eq!(labels.get("fields.book_value.label"), "Buchwert");
    set_locale("en-US");
}

#[test]
fn switching_the_language_changes_a_label_of_azerp() {
    crate::l10n::in_english();
    assert_eq!(t("azerp-status-checked-out"), "Checked out");
    set_locale("de-DE");
    assert_eq!(t("azerp-status-checked-out"), "Ausgeliehen");
    set_locale("en-US");
}
