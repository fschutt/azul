//! "Restore as of..." of an Azlin drive: the time typed (a while ago, or a UTC time) within the
//! days the drive keeps, and what the window says after. No window.

use azul_appkit::l10n::Arg;
use azul_storage::time::iso8601;

use crate::restore::{not_restored_line, parse_as_of, restored_text, Restored, RESTORE_DAYS};
#[cfg(feature = "encryption")]
use crate::restore::{run, EncryptedRestore};

const NOW: u64 = 1_791_450_000;

#[test]
fn a_time_is_a_while_ago_or_a_utc_time_within_the_days_the_drive_keeps() {
    assert_eq!(parse_as_of("1 hour ago", NOW), Ok(NOW - 3_600));
    assert_eq!(parse_as_of(" 2 hours ago ", NOW), Ok(NOW - 7_200));
    assert_eq!(parse_as_of("30 minutes ago", NOW), Ok(NOW - 1_800));
    assert_eq!(parse_as_of("3 days ago", NOW), Ok(NOW - 3 * 86_400));
    let at = NOW - 5_000;
    assert_eq!(parse_as_of(&iso8601(at), NOW), Ok(at));
    // `YYYY-MM-DD HH:MM`, UTC.
    let minute = at - at % 60;
    let short = iso8601(minute)[..16].replace('T', " ");
    assert_eq!(parse_as_of(&short, NOW), Ok(minute), "{short}");
    for wrong in [
        "",
        "soon",
        "-1 hours ago",
        "2 weeks ago",
        "2026-13-40 99:99",
    ] {
        assert!(parse_as_of(wrong, NOW).is_err(), "{wrong:?}");
    }
    let later = parse_as_of(&iso8601(NOW + 60), NOW).unwrap_err();
    assert_eq!(later.keys(), ["azdrive-restore-in-future"], "{later}");
    let too_old = parse_as_of(&iso8601(NOW - RESTORE_DAYS * 86_400 - 1), NOW).unwrap_err();
    let days = too_old.phrase("azdrive-restore-too-old").and_then(|p| p.get("days"));
    assert_eq!(days, Some(&Arg::from(RESTORE_DAYS)), "{too_old}");
    let unknown = parse_as_of("soon", NOW).unwrap_err();
    let text = unknown.phrase("azdrive-restore-unknown-time").and_then(|p| p.get("text"));
    assert_eq!(text, Some(&Arg::from("soon")));
}

#[test]
fn what_a_restore_did_is_said_with_the_drive_and_the_time() {
    let at = NOW - 3_600;
    let said = restored_text("Work", at, &Restored::Files(4));
    assert_eq!(said.key, "azdrive-restored-files");
    assert_eq!(said.get("drive"), Some(&Arg::from("Work")));
    assert_eq!(said.get("at"), Some(&Arg::from(iso8601(at))));
    assert_eq!(said.get("count"), Some(&Arg::Int(4)));
    let said = restored_text("Work", at, &Restored::Objects(7));
    assert_eq!(said.key, "azdrive-restored-objects");
    assert_eq!(said.get("count"), Some(&Arg::Int(7)));
    let said = restored_text("Work", at, &Restored::Queued(String::from("r_1")));
    assert_eq!(said.key, "azdrive-restore-queued");
    assert_eq!(said.get("request"), Some(&Arg::from("r_1")));
}

/// A while ago typed in German ("vor 2 Stunden"): the dialog's words are the window's language,
/// and so is what the user types.
#[test]
fn a_while_ago_is_typed_in_german_too() {
    assert_eq!(parse_as_of("vor 1 Stunde", NOW), Ok(NOW - 3_600));
    assert_eq!(parse_as_of("vor 2 Stunden", NOW), Ok(NOW - 7_200));
    assert_eq!(parse_as_of("vor 30 Minuten", NOW), Ok(NOW - 1_800));
    assert_eq!(parse_as_of("vor 1 Minute", NOW), Ok(NOW - 60));
    assert_eq!(parse_as_of(" Vor 3 Tagen ", NOW), Ok(NOW - 3 * 86_400));
    assert_eq!(parse_as_of("vor 1 Tag", NOW), Ok(NOW - 86_400));
    for wrong in ["vor 2 Wochen", "vor Stunden", "2 Stunden"] {
        assert!(parse_as_of(wrong, NOW).is_err(), "{wrong:?}");
    }
}

/// The time field opens with "1 hour ago" in the window's language, and that parses.
#[test]
fn the_time_fields_first_words_parse_in_english_and_german() {
    use azul_appkit::l10n::{keep, set_locale, t};
    keep(&crate::l10n::sources());
    for locale in ["en-US", "de-DE"] {
        set_locale(locale);
        let first = t("azdrive-restore-default-as-of");
        assert_eq!(parse_as_of(&first, NOW), Ok(NOW - 3_600), "{locale}: {first}");
    }
    set_locale("en-US");
}

/// A restore refused (the time typed is no time) or failed says so on stdout for scripts, on
/// one line: the drive and why, as its keys and arguments (the same in every language).
#[test]
fn a_restore_not_done_names_the_drive_and_why_on_one_line() {
    use azul_appkit::l10n::{Phrase, Text};
    let why = Text::from(
        Phrase::new("azdrive-restore-unknown-time").arg("text", "1 hour ago\n2026-10-10T08:00Z"),
    );
    assert_eq!(
        not_restored_line("AZDRIVE_RESTORE_REFUSED", "d1", &why),
        "AZDRIVE_RESTORE_REFUSED d1 azdrive-restore-unknown-time(text=1 hour ago 2026-10-10T08:00Z)"
    );
    let failed = Text::key("azdrive-restore-node-failed").then(" ").then("disk\r\nfull");
    assert_eq!(
        not_restored_line("AZDRIVE_RESTORE_FAILED", "d1", &failed),
        "AZDRIVE_RESTORE_FAILED d1 azdrive-restore-node-failed disk full"
    );
}

/// X-AR5b-2: an encrypted drive's restore is the answer of the job: its files, or why not. The
/// job has no way back to the bucket's restore at the token server - which rolls every object of
/// the bucket back - whatever the encrypted restore says (the closure used to answer `None`,
/// "it is plain", for a drive it could not decide). No connection is made for either answer.
#[cfg(feature = "encryption")]
#[test]
fn x_ar5b_2_an_encrypted_restore_that_failed_is_the_end_of_the_job_not_a_bucket_restore() {
    use std::sync::Arc;

    use azcloud_kit::SharedKeyring;
    use azul_storage::{
        keyring::{KeyringNamespace, MemoryKeyring},
        testing::TempDir,
    };

    let dir = TempDir::new("restore-job");
    let keyring = SharedKeyring::with_lock_root(
        Arc::new(MemoryKeyring::new()),
        KeyringNamespace::azlin_shared(),
        &dir.path().join("locks"),
    );
    // A token server nobody listens at: reaching it would be an error of a different kind.
    let token_url = "http://127.0.0.1:9";
    let open_first: EncryptedRestore = Box::new(|_| Err(String::from("azdrive-restore-open-first")));
    let refused = run("d_1", NOW, token_url, &keyring, Some(open_first)).unwrap_err();
    assert_eq!(refused.keys(), ["azdrive-restore-open-first"], "{refused}");

    let two_files: EncryptedRestore = Box::new(|as_of| {
        assert_eq!(as_of, i64::try_from(NOW).unwrap());
        Ok(2)
    });
    assert_eq!(
        run("d_1", NOW, token_url, &keyring, Some(two_files)).unwrap(),
        Restored::Files(2)
    );
}

/// X-DELTA-AZDRIVE-KEYS-40 (d): the test above hands the job a closure made by hand; the job of
/// "Restore as of..." is given the one `encryption::restore_with` builds from the open drive. Through
/// THAT closure, a drive the keyring would not open and a drive that is plain end the job with the
/// words that say what to do first - and no connection is made (the token server is an address
/// nobody listens at, so the restore of the bucket, which rolls every object back, would have come
/// back as a connection error and none of the sentences below).
#[cfg(feature = "encryption")]
#[test]
fn x_delta_azdrive_keys_40_the_restore_the_job_is_given_ends_it_for_a_drive_it_cannot_decide_or_a_plain_one(
) {
    use std::sync::Arc;

    use azcloud_kit::SharedKeyring;
    use azul_storage::{
        encrypted::AutoEncrypted,
        keyring::{KeyringError, KeyringNamespace, KeyringStore, MemoryKeyring},
        local::LocalDrive,
        testing::TempDir,
        Drive,
    };

    /// A keyring whose prompt the user declined.
    struct Declined;

    impl KeyringStore for Declined {
        fn get(&self, _key: &str) -> Result<Option<String>, KeyringError> {
            Err(KeyringError::Denied)
        }
        fn set(&self, _key: &str, _secret: &str) -> Result<(), KeyringError> {
            Err(KeyringError::Denied)
        }
        fn delete(&self, _key: &str) -> Result<(), KeyringError> {
            Err(KeyringError::Denied)
        }
    }

    let dir = TempDir::new("restore-job-closure");
    let bucket_dir = dir.path().join("bucket");
    std::fs::create_dir_all(&bucket_dir).unwrap();
    std::fs::write(bucket_dir.join("a.txt"), b"plain").unwrap();
    let bucket: Arc<dyn Drive> = Arc::new(LocalDrive::new(bucket_dir.clone()));
    let keyring = SharedKeyring::with_lock_root(
        Arc::new(MemoryKeyring::new()),
        KeyringNamespace::azlin_shared(),
        &dir.path().join("locks"),
    );
    let token_url = "http://127.0.0.1:9";

    let declined = Arc::new(AutoEncrypted::new(
        Arc::clone(&bucket),
        "d_1",
        Arc::new(Declined),
        None,
    ));
    let refused = run(
        "d_1",
        NOW,
        token_url,
        &keyring,
        Some(crate::encryption::restore_with(declined)),
    )
    .unwrap_err();
    assert!(
        refused.to_string().contains("keyring"),
        "the keyring's refusal is what the job says: {refused}"
    );

    // A bucket with a file and no key anywhere: not encrypted, so nothing to restore its files from.
    let plain = Arc::new(AutoEncrypted::new(
        Arc::clone(&bucket),
        "d_2",
        Arc::new(MemoryKeyring::new()),
        None,
    ));
    let refused = run(
        "d_2",
        NOW,
        token_url,
        &keyring,
        Some(crate::encryption::restore_with(plain)),
    )
    .unwrap_err();
    assert_eq!(
        refused.keys(),
        ["azdrive-restore-encrypt-first"],
        "{refused}"
    );
    assert!(
        bucket_dir.join("a.txt").exists(),
        "and nothing of the drive was touched"
    );
}

/// X-AR5a-21. The window of "Restore as of..." is the span the drive keeps what it released - the
/// versions of an encrypted drive's files, ninety days, which the drive's reclaimer honours; the
/// trash of a bucket the node restores itself, fourteen - read from the one `Retention`, not a
/// number of the dialog's own (it was fourteen, shorter than the versions' ninety).
#[test]
fn x_ar5a_21_the_restore_window_is_the_retention_of_the_kind_of_drive() {
    let retention = azul_storage::meta::RETENTION;
    #[cfg(feature = "encryption")]
    assert_eq!(RESTORE_DAYS, retention.versions_days());
    #[cfg(not(feature = "encryption"))]
    assert_eq!(RESTORE_DAYS, retention.node_days());
    let inside = NOW - (RESTORE_DAYS - 1) * 86_400;
    assert_eq!(parse_as_of(&iso8601(inside), NOW), Ok(inside));
}
