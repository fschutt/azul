//! AzSheets' words in azul's localization: every key its source names is in resources/en.ftl
//! and resources/de.ftl and both parse; the engine gets them with appkit's; a language switch
//! changes a label. (The Language row is appkit's settings page's.) No window.

use std::path::Path;

use azul_appkit::{
    l10n::{set_locale, t},
    l10n_check::check,
};

use crate::l10n::{sources, DE, EN};

#[test]
fn every_key_of_azsheets_source_is_in_english_and_german_and_both_parse() {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let problems = check(&manifest.join("src"), &["azsheets"], EN, DE);
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_engine_gets_azsheets_words_and_appkits_in_english_and_german() {
    let sources = sources();
    let tags: Vec<&str> = sources.iter().map(|(tag, _)| tag.as_str()).collect();
    assert_eq!(tags, vec!["en", "de"]);
    for (tag, source) in &sources {
        for key in ["kit-general-language", "azsheets-tab-home"] {
            assert!(source.contains(&format!("{key} = ")), "{tag} lacks {key}");
        }
    }
}

#[test]
fn switching_the_language_changes_a_label_of_azsheets() {
    crate::l10n::in_english();
    assert_eq!(t("azsheets-tab-home"), "HOME");
    set_locale("de-DE");
    assert_eq!(t("azsheets-tab-home"), "START");
    set_locale("en-US");
}

#[test]
fn an_engine_error_of_azsheets_is_said_and_ironcalcs_own_words_stay() {
    use crate::engine::{error_text, keyed_error};
    crate::l10n::in_english();
    let said = |e: &str| azul_appkit::l10n::t_text(&error_text(e));
    assert_eq!(
        said(&keyed_error("azsheets-open-failed", "bad zip")),
        "Could not open the workbook: bad zip"
    );
    assert_eq!(said(&keyed_error("azsheets-no-such-sheet", "")), "There is no such sheet.");
    assert_eq!(said("Invalid sheet name"), "Invalid sheet name", "IronCalc's words as they are");
    set_locale("de-DE");
    assert_eq!(said(&keyed_error("azsheets-no-such-sheet", "")), "Dieses Blatt gibt es nicht.");
    set_locale("en-US");
}
