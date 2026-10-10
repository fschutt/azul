//! AzDrive's words in azul's localization: every key its source names is in resources/en.ftl
//! and resources/de.ftl and both parse; the engine gets them with appkit's and azcloud-kit's
//! error table; a language switch changes a label; a storage error is the table's message
//! with its error ID. No window.

use std::path::Path;

use azcloud_kit::user_errors::{Code, ID_LABEL_MESSAGE};
use azul_appkit::{
    l10n::{keep, set_locale, t, Text},
    l10n_check::check,
};
use azul_storage::{DriveError, ServiceError};

use crate::l10n::{drive_error_text, sources, DE, EN};

fn unpaid() -> DriveError {
    DriveError::Service(ServiceError {
        status: 403,
        code: String::from("AccessDenied"),
        message: String::from("read-only"),
        request_id: Some(String::from("n2-81723")),
        azlin_error: Some(String::from("read_only_unpaid")),
        ..ServiceError::default()
    })
}

#[test]
fn every_key_of_azdrives_source_is_in_english_and_german_and_both_parse() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let problems = check(&dir, &["azdrive"], EN, DE);
    assert!(problems.is_empty(), "{problems:#?}");
}

#[test]
fn the_engine_gets_azdrives_words_appkits_and_the_kits_error_table_in_english_and_german() {
    let sources = sources();
    let tags: Vec<&str> = sources.iter().map(|(tag, _)| tag.as_str()).collect();
    assert_eq!(tags, vec!["en", "de"]);
    let unpaid = Code::ReadOnlyUnpaid.message_id();
    for (tag, source) in &sources {
        for key in [
            "kit-general-language",
            "azdrive-tab-home",
            unpaid.as_str(),
            ID_LABEL_MESSAGE,
        ] {
            assert!(source.contains(&format!("{key} = ")), "{tag} lacks {key}");
        }
    }
}

#[test]
fn switching_the_language_changes_a_label_of_azdrive() {
    keep(&sources());
    set_locale("en-US");
    assert_eq!(t("azdrive-tab-home"), "Home");
    set_locale("de-DE");
    assert_eq!(t("azdrive-tab-home"), "Start");
    assert!(t(&Code::ReadOnlyUnpaid.message_id()).starts_with("Deine letzte Zahlung"));
    set_locale("en-US");
}

#[test]
fn a_storage_error_is_the_tables_message_with_its_error_id_and_a_local_one_its_own_words() {
    let text = drive_error_text(&unpaid());
    assert_eq!(
        text.keys(),
        vec!["azlin-error-read-only-unpaid", ID_LABEL_MESSAGE]
    );
    let id = text
        .phrase(ID_LABEL_MESSAGE)
        .and_then(|phrase| phrase.get("id"))
        .map(ToString::to_string);
    assert_eq!(id.as_deref(), Some("n2-81723"));
    let missing = DriveError::NotFound {
        key: String::from("a.txt"),
    };
    assert_eq!(drive_error_text(&missing), Text::plain(missing.to_string()));
}
