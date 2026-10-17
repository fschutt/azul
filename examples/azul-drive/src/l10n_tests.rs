//! AzDrive's words in azul's localization: every key its source names is in resources/en.ftl
//! and resources/de.ftl and both parse; the engine gets them with appkit's and azcloud-kit's
//! error table; a language switch changes a label; a storage error is the table's message
//! with its error ID. No window.

use std::path::Path;

use azcloud_kit::user_errors::{Code, ID_LABEL_MESSAGE};
use azul_appkit::{
    l10n::{keep, named, set_locale, t, t_text},
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
        key: String::from("docs/a.txt"),
    };
    let said = drive_error_text(&missing);
    assert_eq!(said.keys(), vec!["azdrive-err-not-found"]);
    let name = said.phrase("azdrive-err-not-found").and_then(|p| p.get("name"));
    assert_eq!(name.map(ToString::to_string).as_deref(), Some("a.txt"));
}

/// An error the user caused is said in the window's language too: the name, the file
/// system's own words as they are.
#[test]
fn a_users_own_error_is_said_in_the_windows_language() {
    keep(&sources());
    set_locale("de-DE");
    let missing = DriveError::NotFound {
        key: String::from("docs/a.txt"),
    };
    assert_eq!(t_text(&drive_error_text(&missing)), "„a.txt“ existiert nicht.");
    let disk = DriveError::Io(String::from("No space left on device"));
    assert_eq!(
        t_text(&drive_error_text(&disk)),
        "Dateifehler: No space left on device"
    );
    let bad = DriveError::InvalidKey {
        key: String::from("a/../b"),
        reason: "it climbs out of its folder",
    };
    assert!(t_text(&drive_error_text(&bad)).starts_with("„a/../b“ ist kein gültiger Name"));
    set_locale("en-US");
    assert_eq!(t_text(&drive_error_text(&missing)), "\"a.txt\" does not exist.");
}

/// The reasons azul-storage and AzDrive give for a name that cannot be (`DriveError::InvalidKey`).
const REASONS: &[&str] = &[
    "something has this name already",
    "a folder is copied object by object",
    "a folder has this name",
    "a file has this name",
    "a folder cannot move into itself",
    "a file and a folder cannot trade places",
    "it names a folder, not a file",
    "it is a folder",
    "it is a file",
    "it has no usable file name",
    "it is not empty any more; delete it instead",
    "it is the drive's root",
    "it is the drive's own bookkeeping (.azlin)",
    "a folder name ends with /",
    "two shared files have this name",
    "a grant covers a folder, which ends with /",
    "the destination folder is inside the folder it would receive",
    "it is empty",
    "it contains a NUL character",
    "it contains a backslash",
    "it is an absolute path",
    "it has an empty path segment",
    "it has a \".\" segment",
    "it climbs out of its folder (\"..\")",
];

/// A storage reason is said in the window's language too - also when a worker thread made the
/// message, which has no language: "ein Ordner hat diesen Namen"; every reason has its German.
#[test]
fn a_storage_reason_is_said_in_the_windows_language_even_from_a_worker() {
    let bad = DriveError::InvalidKey {
        key: String::from("Fotos"),
        reason: "a folder has this name",
    };
    let said = std::thread::spawn(move || drive_error_text(&bad))
        .join()
        .unwrap();
    keep(&sources());
    set_locale("de-DE");
    assert_eq!(
        t_text(&said),
        "„Fotos“ ist kein gültiger Name: ein Ordner hat diesen Namen"
    );
    for reason in REASONS {
        let german = named("AzDrive", "reason", reason);
        assert_ne!(german, *reason, "{reason}: no German");
    }
    set_locale("en-US");
    assert_eq!(
        t_text(&said),
        "\"Fotos\" is not a valid name: a folder has this name"
    );
    for reason in REASONS {
        assert_eq!(named("AzDrive", "reason", reason), *reason);
    }
}
