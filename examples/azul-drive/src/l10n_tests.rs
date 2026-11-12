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
        azlin_error: Some(azul_storage::AzlinError::ReadOnlyUnpaid),
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
    assert!(t(&Code::ReadOnlyUnpaid.message_id()).starts_with("Dein bezahlter Zeitraum"));
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

/// X-DELTA-AZDRIVE-KEYS-32: the confirm dialog of "Remove drive" promised that the keys go before
/// anyone knew whether there was a proof. It has two texts - the keys go (and say what else loses
/// them: AzMail, the bridge), the keys stay (the recovery code was never checked) - in both
/// languages, and they are not the same words.
#[test]
fn x_delta_azdrive_keys_32_the_remove_dialog_says_what_becomes_of_the_keys_in_both_languages() {
    keep(&sources());
    for (locale, goes, stays) in [
        ("en-US", "AzMail", "never checked"),
        ("de-DE", "AzMail", "nie geprüft"),
    ] {
        set_locale(locale);
        let go = t("azdrive-forget-what");
        let stay = t("azdrive-forget-what-keys-stay");
        assert_ne!(go, stay, "{locale}");
        assert!(go.contains(goes), "{locale}: {go}");
        assert!(stay.contains(stays), "{locale}: {stay}");
        assert!(!go.contains(stays), "{locale}: {go}");
    }
    set_locale("en-US");
}

/// X-DELTA-AZDRIVE-KEYS-31: "nothing in the drive opens with the old key any more" was said after
/// re-encrypting every file, and on a drive that keeps its history (since a1c5e2a77) the older
/// versions and the trash still do. The sentence says what is true.
#[test]
fn x_delta_azdrive_keys_31_the_re_encrypted_message_does_not_claim_that_nothing_opens_with_the_old_key(
) {
    keep(&sources());
    for (locale, old_claim, true_words) in [
        (
            "en-US",
            "nothing in the drive opens with the old key",
            "older versions",
        ),
        (
            "de-DE",
            "Nichts im Laufwerk öffnet sich mehr mit dem alten Schlüssel",
            "älteren Versionen",
        ),
    ] {
        set_locale(locale);
        let said = t("azdrive-enc-reencrypted");
        assert!(!said.contains(old_claim), "{locale}: {said}");
        assert!(said.contains(true_words), "{locale}: {said}");
    }
    set_locale("en-US");
}

/// X-DELTA-KEYS2-6: a key change that never finished (a new key kept, no journal written) keeps the drive's keys
/// when it is removed for good, and the removal cannot tell it is dead without reading the bucket: only a new key
/// change ends it. The dialog that says the keys stay names that button by its own label, in both languages - a
/// label that is renamed breaks this, and the dialog is not left pointing at a button that is not there.
#[test]
fn x_delta_keys2_6_the_dialog_of_kept_keys_names_the_button_of_a_new_key_change() {
    for (lang, ftl) in [("en", EN), ("de", DE)] {
        let line = |key: &str| {
            ftl.lines()
                .find_map(|l| l.strip_prefix(&format!("{key} = ")))
                .unwrap_or_else(|| panic!("{lang}: {key} is missing"))
        };
        let label = line("azdrive-side-rotate").trim_end_matches('…');
        assert!(
            line("azdrive-forget-keys-kept").contains(label),
            "{lang}: the dialog of kept keys does not name {label:?}"
        );
    }
}
