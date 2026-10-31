//! When AzDrive reads a drive's keys from the keychain: the start, a timer or a poll read them
//! only quietly (no read of theirs can show a prompt), all the drives they want in one round; a
//! drive whose keys only a prompt hands over waits for the user's Unlock; "Unlock drives" asks
//! in one round, which the user's first "No" ends. A drive's keys are kept for the session and
//! wiped when it is locked or the window closes.

use azul_storage::{
    config::{DriveAuth, DriveEntry, DriveLocation},
    keyring::{Asking, KeyringError},
};

use crate::unlock::{Read, Unlocks};

fn ids(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_string()).collect()
}

fn s3_slot(id: &str) -> crate::Slot {
    crate::Slot::new(DriveEntry {
        id: id.to_string(),
        name: String::from("Archive"),
        location: DriveLocation::S3 {
            endpoint: String::from("https://s3.example"),
            region: String::from("eu-central-1"),
            bucket: String::from("archive"),
            path_style: false,
            auth: DriveAuth::Keyring,
        },
    })
}

/// The window's start (and a timer's poll) asks for several drives at once: their reads are
/// gathered while it runs and read in one quiet round when it ends - never one read each.
#[test]
fn the_start_reads_every_locked_drive_in_one_quiet_round() {
    let mut unlocks = Unlocks::default();
    unlocks.gathering = true;
    unlocks.want("a");
    unlocks.want("b");
    unlocks.want("a");
    assert_eq!(unlocks.next_round(), None, "nothing is read while the start gathers");
    unlocks.gathering = false;
    assert_eq!(unlocks.next_round(), Some(ids(&["a", "b"])));
    assert!(unlocks.is_reading("a") && unlocks.is_reading("b"));
    assert_eq!(unlocks.next_round(), None, "one round at a time");
}

/// A poll that asks again for a drive being read adds nothing; another drive waits for the
/// next round.
#[test]
fn a_drive_wanted_while_a_round_runs_is_read_by_the_next_one() {
    let mut unlocks = Unlocks::default();
    unlocks.want("a");
    assert_eq!(unlocks.next_round(), Some(ids(&["a"])));
    unlocks.want("a");
    unlocks.want("b");
    assert_eq!(unlocks.next_round(), None);
    unlocks.round_ended();
    assert_eq!(unlocks.next_round(), Some(ids(&["b"])));
}

/// A drive whose keys the keychain gives only after asking waits for the user: no start, timer
/// or poll reads it again, so no prompt ever comes from a timer.
#[test]
fn a_drive_whose_keys_need_the_user_is_not_read_again_by_a_timer() {
    let mut unlocks = Unlocks::default();
    unlocks.want("a");
    assert_eq!(unlocks.next_round(), Some(ids(&["a"])));
    unlocks.round_ended();
    unlocks.needs_user("a");
    assert!(unlocks.waits_for_user("a"));
    for _ in 0..10 {
        unlocks.want("a");
    }
    assert_eq!(unlocks.next_round(), None, "no read on a timer, so no prompt");
    assert_eq!(unlocks.waiting(), ids(&["a"]));
}

/// "Unlock drives": every drive waiting for the user in one round that may ask; pressed again
/// while it runs, still one round. A drive that opened waits no more.
#[test]
fn unlock_drives_reads_every_waiting_drive_in_one_round() {
    let mut unlocks = Unlocks::default();
    unlocks.needs_user("b");
    unlocks.needs_user("a");
    assert_eq!(unlocks.user_round(), Some(ids(&["a", "b"])));
    assert_eq!(unlocks.user_round(), None, "pressed again while it runs");
    unlocks.round_ended();
    unlocks.unlocked("a");
    assert!(!unlocks.waits_for_user("a"));
    assert!(unlocks.waits_for_user("b"));
    assert_eq!(unlocks.user_round(), Some(ids(&["b"])));
}

/// The user's Lock: the drive waits for Unlock, and no quiet round reads it back behind their
/// back.
#[test]
fn a_locked_drive_waits_for_the_user_until_unlocked() {
    let mut unlocks = Unlocks::default();
    unlocks.lock("a");
    assert!(unlocks.waits_for_user("a"));
    unlocks.want("a");
    assert_eq!(unlocks.next_round(), None);
}

/// A quiet read that met a prompt waits for the user; the user's own "No" is a refusal; an
/// entry that is not there, or no keyring, says so.
#[test]
fn a_reads_answer_says_whether_the_drive_opens_waits_or_cannot_be_opened() {
    match Read::of_answer(Ok(Some(String::from("keys"))), Asking::Never) {
        Read::Secret(secret) => assert_eq!(secret.as_str(), "keys"),
        _ => panic!("the keys were read"),
    }
    assert!(matches!(
        Read::of_answer(Err(KeyringError::Denied), Asking::Never),
        Read::NeedsUser
    ));
    assert!(matches!(
        Read::of_answer(Err(KeyringError::Denied), Asking::User),
        Read::Refused
    ));
    assert!(matches!(Read::of_answer(Ok(None), Asking::User), Read::Missing));
    assert!(matches!(
        Read::of_answer(Err(KeyringError::Unavailable), Asking::Never),
        Read::Unavailable
    ));
    assert!(matches!(
        Read::of_answer(Err(KeyringError::Failed(String::from("locked"))), Asking::Never),
        Read::Failed
    ));
    assert_eq!(Read::Missing.problem(), "azdrive-keyring-not-found");
    assert_eq!(Read::Refused.problem(), "azdrive-keyring-denied");
}

/// The keys of a drive stay for the session; a lock or the window's close wipes them (the
/// text is zeroed when it is dropped), and the drive is locked again.
#[test]
fn a_drives_keys_are_wiped_when_it_is_locked() {
    let mut slot = s3_slot("s3-1");
    assert!(slot.locked());
    slot.secret = Some(azcloud_kit::Zeroizing::new(String::from("keys")));
    assert!(!slot.locked());
    crate::unlock::wipe(&mut slot);
    assert!(slot.secret.is_none());
    assert!(slot.drive.is_none());
    assert!(slot.locked());
}
