//! What a delete says (AZ-U12). A cloud delete asked "for good? This cannot be undone." for every
//! drive, also for an encrypted Azlin drive, which keeps its last days ("Restore as of…"), and said
//! "Deleted … for good" after it. The words follow what is true of the drive, by the policy the
//! restore itself asks.

use azul_appkit::l10n::{t, t_args, t_phrase, Arg, Phrase};

use crate::restore::RESTORE_DAYS;

/// Only a drive "Restore as of…" works for keeps what is deleted from it; a drive without an
/// encryption seam, one not decided yet and one that is not encrypted do not promise it.
#[cfg(feature = "encryption")]
#[test]
fn az_u12_only_a_drive_the_restore_works_for_is_said_to_keep_what_is_deleted() {
    use azul_storage::trust::{EncState, Via, Why};

    use crate::actions::history_kept;

    assert!(
        !history_kept(None),
        "a drive without an encryption seam: for good"
    );
    for via in [Via::KeyInKeyring, Via::Record, Via::KeyFilesListed] {
        let encrypted = EncState::Encrypted { key_id: None, via };
        assert!(history_kept(Some(encrypted)), "{encrypted:?}");
    }
    for state in [
        EncState::Undecided,
        EncState::SetupPending,
        EncState::Unencrypted(Why::New),
        EncState::Unencrypted(Why::HiddenOrAbsent),
    ] {
        assert!(!history_kept(Some(state)), "{state:?}: no promise");
    }
}

/// The question's last line and the notice after the delete, for a drive with history: the days
/// and the way back, and neither "cannot be undone" nor "for good".
#[test]
fn az_u12_the_words_of_a_delete_from_a_drive_with_history_promise_the_way_back_only() {
    crate::l10n::in_english();
    let cannot = t("azdrive-delete-cannot-undo");
    assert!(cannot.contains("cannot be undone"), "{cannot}");

    // The days the words promise are the days the drive keeps what it released: the versions of an
    // encrypted drive's files, ninety (the owner's decision of 2026-10-24), and - in a build
    // without encrypted drives - the fourteen its node keeps (D38). The one `Retention` owns the
    // span since X-AR5a-21 (1a9e92feb; the dialog's own number was fourteen, shorter than the
    // ninety the drive keeps), so the literal here is that decided span, and a window that moves
    // fails this line and has the words read again.
    let days: u64 = if cfg!(feature = "encryption") { 90 } else { 14 };
    assert_eq!(RESTORE_DAYS, days, "the span the way back is promised for");

    let can = t_args(
        "azdrive-delete-can-restore",
        &[("days", Arg::from(RESTORE_DAYS))],
    );
    assert!(can.contains(&format!("last {days} days")), "{can}");
    assert!(can.contains("Restore as of"), "{can}");
    assert!(!can.contains("cannot be undone"), "{can}");

    let one = t_phrase(
        &Phrase::new("azdrive-deleted-with-history")
            .arg("count", 1usize)
            .arg("days", RESTORE_DAYS),
    );
    assert!(one.starts_with("Deleted one item."), "{one}");
    assert!(
        one.contains(&format!("brings it back for {days} days")),
        "{one}"
    );
    assert!(!one.contains("for good"), "{one}");
    let three = t_phrase(
        &Phrase::new("azdrive-deleted-with-history")
            .arg("count", 3usize)
            .arg("days", RESTORE_DAYS),
    );
    assert!(three.starts_with("Deleted 3 items."), "{three}");
    assert!(
        three.contains(&format!("brings them back for {days} days")),
        "{three}"
    );

    // The older notice stays for a drive that keeps nothing.
    let for_good = t_phrase(&Phrase::new("azdrive-deleted-for-good").arg("count", 2usize));
    assert!(for_good.contains("for good"), "{for_good}");
}

/// The words a screen reader hears on the Delete button say what it deletes and from where - not
/// "cannot be undone", which a drive with history would make false.
#[test]
fn az_u12_the_delete_button_says_what_it_deletes_and_from_where() {
    crate::l10n::in_english();
    let one = t_args(
        "azdrive-delete-warning",
        &[("count", Arg::from(1usize)), ("drive", Arg::from("Photos"))],
    );
    assert_eq!(one, "Deletes one item from \"Photos\".");
    let many = t_args(
        "azdrive-delete-warning",
        &[("count", Arg::from(4usize)), ("drive", Arg::from("Photos"))],
    );
    assert_eq!(many, "Deletes 4 items from \"Photos\".");
    assert!(!many.contains("cannot be undone"));
    assert!(t("azdrive-delete-folders").contains("everything in it"));
}
