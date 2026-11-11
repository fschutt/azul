//! A drive's second recovery code (a second emergency kit, D51): sealed like the first in a
//! wrap of its own, either code opens the drive, removing one leaves the other.

use super::mem_bucket::MemBucket;
use crate::{
    crypto::{
        device::{
            add_recovery_code, add_recovery_code_for_key, enroll, other_devices,
            read_recovery_code, recover, recover_within, recovery_code_file_for,
            remove_recovery_code, restore_recovery_code, setup_new_drive, unclaimed_recovery_codes,
            EXTRA_RECOVERY_PREFIX, MAX_EXTRA_RECOVERY_CODES, RECOVER_WORK_BUDGET,
        },
        keys::{RecoveryCode, RecoveryKdf, RecoveryWrap, RECOVERY_KEY_FILE},
    },
    keyring::MemoryKeyring,
    Drive, DriveError, ListRequest,
};

fn cheap() -> RecoveryKdf {
    RecoveryKdf::with_cost(64, 1, 1).unwrap()
}

#[test]
fn a_second_recovery_code_opens_the_drive_and_so_does_the_first() {
    let bucket = MemBucket::new();
    let owner = MemoryKeyring::new();
    let (drive_key, first) = setup_new_drive(&bucket, &owner, "d_1", cheap()).unwrap();
    let (file, second) = add_recovery_code(&bucket, &owner, "d_1", cheap()).unwrap();
    assert!(
        file.starts_with(EXTRA_RECOVERY_PREFIX) && file.ends_with(".key"),
        "{file}"
    );
    assert_ne!(file, RECOVERY_KEY_FILE);
    assert_ne!(first.as_bytes(), second.as_bytes(), "a code of its own");
    assert!(bucket.get(&file).is_ok());
    // A computer without the key opens the drive with either code.
    for code in [&second, &first] {
        let other = MemoryKeyring::new();
        let opened = recover(&bucket, &other, "d_1", code).unwrap();
        assert_eq!(opened.id(), drive_key.id());
    }
    // Another code opens nothing.
    let stranger = RecoveryCode::from_bytes([0x11; 16]);
    assert!(matches!(
        recover(&bucket, &MemoryKeyring::new(), "d_1", &stranger),
        Err(DriveError::Denied { .. })
    ));
    // The extra wrap is no device.
    assert_eq!(
        other_devices(&bucket, &owner, "d_1").unwrap(),
        2,
        "the two recoveries"
    );
}

#[test]
fn a_removed_second_code_opens_nothing_and_the_first_stays() {
    let bucket = MemBucket::new();
    let owner = MemoryKeyring::new();
    let (_, first) = setup_new_drive(&bucket, &owner, "d_1", cheap()).unwrap();
    let (file, second) = add_recovery_code(&bucket, &owner, "d_1", cheap()).unwrap();
    remove_recovery_code(&bucket, &file).unwrap();
    remove_recovery_code(&bucket, &file).unwrap();
    assert!(matches!(
        recover(&bucket, &MemoryKeyring::new(), "d_1", &second),
        Err(DriveError::Denied { .. })
    ));
    assert!(recover(&bucket, &MemoryKeyring::new(), "d_1", &first).is_ok());
    // Only an extra code's file is removed this way, never the first code's or a member's.
    assert!(remove_recovery_code(&bucket, RECOVERY_KEY_FILE).is_err());
    assert!(remove_recovery_code(&bucket, ".azlin/keys/abc.key").is_err());
    assert!(bucket.get(RECOVERY_KEY_FILE).is_ok());
}

#[test]
fn a_second_code_needs_a_device_that_holds_the_drive_key() {
    let bucket = MemBucket::new();
    let owner = MemoryKeyring::new();
    let (drive_key, _) = setup_new_drive(&bucket, &owner, "d_1", cheap()).unwrap();
    // A device without the drive's key: the drive is locked on it.
    assert!(matches!(
        add_recovery_code(&bucket, &MemoryKeyring::new(), "d_1", cheap()),
        Err(DriveError::Locked { drive }) if drive == "d_1"
    ));
    // A second device with the key may add one too; the other devices are counted without it.
    let second_device = MemoryKeyring::new();
    enroll(&bucket, &second_device, "d_1", &drive_key).unwrap();
    add_recovery_code(&bucket, &second_device, "d_1", cheap()).unwrap();
    assert_eq!(other_devices(&bucket, &owner, "d_1").unwrap(), 1);
    let extras = bucket
        .list(&ListRequest::recursive(EXTRA_RECOVERY_PREFIX))
        .unwrap()
        .objects
        .len();
    assert_eq!(extras, 1);
}

/// C6: the recovery tries each wrap's Argon2id in turn, so a drive holds a bounded number of
/// further codes (a second kit, a third), and a bucket that lists more is read as far as that.
#[test]
fn a_drive_holds_at_most_eight_further_recovery_codes() {
    let bucket = MemBucket::new();
    let owner = MemoryKeyring::new();
    setup_new_drive(&bucket, &owner, "d_1", cheap()).unwrap();
    for _ in 0..MAX_EXTRA_RECOVERY_CODES {
        add_recovery_code(&bucket, &owner, "d_1", cheap()).unwrap();
    }
    assert!(matches!(
        add_recovery_code(&bucket, &owner, "d_1", cheap()),
        Err(DriveError::Unsupported(_))
    ));
    // Remove one, and there is room again.
    let first = bucket
        .list(&ListRequest::recursive(EXTRA_RECOVERY_PREFIX))
        .unwrap()
        .objects[0]
        .key
        .clone();
    remove_recovery_code(&bucket, &first).unwrap();
    add_recovery_code(&bucket, &owner, "d_1", cheap()).unwrap();
}

/// K10 with C6: a first wrap that is missing or damaged (a hostile or buggy bucket) no longer keeps
/// a second kit's code from opening the drive, and a further wrap that is damaged is left out
/// instead of ending the recovery. What was wrong with the first wrap is said when no code opens.
#[test]
fn a_spoiled_wrap_does_not_keep_another_codes_wrap_from_opening_the_drive() {
    let bucket = MemBucket::new();
    let owner = MemoryKeyring::new();
    let (drive_key, first) = setup_new_drive(&bucket, &owner, "d_1", cheap()).unwrap();
    let (_, second) = add_recovery_code(&bucket, &owner, "d_1", cheap()).unwrap();
    // A further file that is no wrap, named so that it is read first.
    bucket.set(
        &format!("{EXTRA_RECOVERY_PREFIX}0000000000000000.key"),
        b"not a wrap".to_vec(),
    );

    // The first wrap damaged: the second kit still opens the drive; the first code's wrap is gone.
    bucket.set(RECOVERY_KEY_FILE, b"damaged".to_vec());
    let opened = recover(&bucket, &MemoryKeyring::new(), "d_1", &second).unwrap();
    assert_eq!(opened.id(), drive_key.id());
    // No code opens it: the damage is the answer, not "the code is wrong".
    let stranger = RecoveryCode::from_bytes([0x22; 16]);
    assert!(matches!(
        recover(&bucket, &MemoryKeyring::new(), "d_1", &stranger),
        Err(DriveError::Corrupt { .. })
    ));
    // The first wrap missing altogether.
    bucket.delete(RECOVERY_KEY_FILE).unwrap();
    assert!(recover(&bucket, &MemoryKeyring::new(), "d_1", &second).is_ok());
    assert!(matches!(
        recover(&bucket, &MemoryKeyring::new(), "d_1", &first),
        Err(DriveError::NotFound { .. })
    ));
}

/// A bucket that lists more further wraps than a drive holds is read as far as it holds: the wraps
/// after the eighth by name are not tried.
#[test]
fn a_bucket_listing_more_further_wraps_than_a_drive_holds_is_read_as_far_as_that() {
    let bucket = MemBucket::new();
    let owner = MemoryKeyring::new();
    let (drive_key, _) = setup_new_drive(&bucket, &owner, "d_1", cheap()).unwrap();
    let code = RecoveryCode::generate().unwrap();
    let wrap = RecoveryWrap::seal(&drive_key, "d_1", &code, cheap()).unwrap();
    // Eight wraps of other codes sort before this one.
    for n in 0..MAX_EXTRA_RECOVERY_CODES as u64 {
        let other = RecoveryWrap::seal(
            &drive_key,
            "d_1",
            &RecoveryCode::generate().unwrap(),
            cheap(),
        )
        .unwrap();
        bucket.set(
            &format!("{EXTRA_RECOVERY_PREFIX}{n:016x}.key"),
            other.to_bytes(),
        );
    }
    bucket.set(
        &format!("{EXTRA_RECOVERY_PREFIX}ffffffffffffffff.key"),
        wrap.to_bytes(),
    );
    assert!(matches!(
        recover(&bucket, &MemoryKeyring::new(), "d_1", &code),
        Err(DriveError::Denied { .. })
    ));
}

/// K10 (the KDF part): the cost of a recovery wrap is the bucket's word, paid before the code is
/// known to be right, once per file. One recovery spends one budget of work on all its wraps
/// together; the wraps after it are not tried, and what stopped the search is said.
#[test]
fn az_k10_the_wraps_of_a_bucket_are_tried_within_one_work_budget() {
    let bucket = MemBucket::new();
    let owner = MemoryKeyring::new();
    let (drive_key, _first) = setup_new_drive(&bucket, &owner, "d_1", cheap()).unwrap();
    let code = RecoveryCode::generate().unwrap();
    let target = RecoveryWrap::seal(&drive_key, "d_1", &code, cheap()).unwrap();
    // Three wraps of other codes sort before the target: it is the fifth wrap tried.
    for n in 0..3u64 {
        let other = RecoveryWrap::seal(
            &drive_key,
            "d_1",
            &RecoveryCode::generate().unwrap(),
            cheap(),
        )
        .unwrap();
        bucket.set(
            &format!("{EXTRA_RECOVERY_PREFIX}{n:016x}.key"),
            other.to_bytes(),
        );
    }
    bucket.set(
        &format!("{EXTRA_RECOVERY_PREFIX}ffffffffffffffff.key"),
        target.to_bytes(),
    );
    let work = cheap().work();
    let short = recover_within(&bucket, &MemoryKeyring::new(), "d_1", &code, 4 * work);
    assert!(
        matches!(&short, Err(DriveError::Unsupported(why)) if why.contains("more work")),
        "the fifth wrap is past a budget of four: {short:?}"
    );
    let enough = recover_within(&bucket, &MemoryKeyring::new(), "d_1", &code, 5 * work).unwrap();
    assert_eq!(enough.id(), drive_key.id());

    // The budget of recover() holds the first wrap and eight further ones at the cost this
    // software writes, and no more than three of the worst a file may ask for.
    const _: () = assert!(
        RECOVER_WORK_BUDGET >= (1 + MAX_EXTRA_RECOVERY_CODES as u64) * RecoveryKdf::DEFAULT_WORK
    );
    const _: () = assert!(RECOVER_WORK_BUDGET < 4 * RecoveryKdf::MAX_WORK);
}

/// A key file bigger than any key file is (a bucket that streams on) is left out of a recovery like a
/// damaged one: the other codes' wraps are still tried.
#[test]
fn az_k10_a_key_file_over_the_limit_is_left_out_not_the_end_of_the_recovery() {
    let bucket = MemBucket::new();
    let owner = MemoryKeyring::new();
    let (drive_key, _first) = setup_new_drive(&bucket, &owner, "d_1", cheap()).unwrap();
    let (_, second) = add_recovery_code(&bucket, &owner, "d_1", cheap()).unwrap();
    bucket.set(
        &format!("{EXTRA_RECOVERY_PREFIX}0000000000000000.key"),
        vec![b'x'; (crate::limits::named::KEY_FILE.max + 1) as usize],
    );
    let opened = recover(&bucket, &MemoryKeyring::new(), "d_1", &second).unwrap();
    assert_eq!(opened.id(), drive_key.id());
}

/// X-DELTA-AZDRIVE-KEYS-28: a further code's wrap had a random name only the computer that made
/// the kit knew, so removing the kit's key from another computer left the wrap, which still opened
/// the drive key. Named by the code's key at the token server, it is found by anyone who sees that
/// key; the file reads back and goes back byte for byte when a removal is refused.
#[test]
fn x_delta_azdrive_keys_28_a_kit_named_by_its_key_is_found_from_another_computer_and_goes_back() {
    // A bucket that writes conditionally, as every real one does (S3's `If-None-Match`, a folder's
    // rename): the put-back asks for a free name. Where a bucket answers `Unsupported`, `put_new`
    // writes the key file without the question (it is "conditional where the bucket can"), so the
    // refusal below is a bucket's to give.
    let bucket = MemBucket::with_conditional_writes();
    let owner = MemoryKeyring::new();
    let (drive_key, _) = setup_new_drive(&bucket, &owner, "d_1", cheap()).unwrap();
    let (file, kit) =
        add_recovery_code_for_key(&bucket, &owner, "d_1", cheap(), |_| String::from("KITKEY="))
            .unwrap();
    // the name is the key's, the same everywhere; another key is another file
    assert_eq!(file, recovery_code_file_for("KITKEY="));
    assert_eq!(
        file,
        recovery_code_file_for(" KITKEY=\n"),
        "spaces around the text are none"
    );
    assert_ne!(file, recovery_code_file_for("OTHERKEY="));
    assert!(
        file.starts_with(EXTRA_RECOVERY_PREFIX) && file.ends_with(".key"),
        "{file}"
    );
    // a computer that holds nothing of the kit can name the file from the server's key alone and
    // takes the wrap out; the kit then opens nothing
    let wrap = read_recovery_code(&bucket, &recovery_code_file_for("KITKEY="))
        .unwrap()
        .expect("the wrap is there");
    remove_recovery_code(&bucket, &recovery_code_file_for("KITKEY=")).unwrap();
    assert!(recover(&bucket, &MemoryKeyring::new(), "d_1", &kit).is_err());
    assert_eq!(
        read_recovery_code(&bucket, &file).unwrap(),
        None,
        "a file that is gone reads as none"
    );
    // the server refused: it goes back, byte for byte, and the kit opens the drive again
    restore_recovery_code(&bucket, &file, &wrap).unwrap();
    assert_eq!(bucket.get(&file).unwrap(), wrap);
    let opened = recover(&bucket, &MemoryKeyring::new(), "d_1", &kit).unwrap();
    assert_eq!(opened.id(), drive_key.id());
    // nothing is put over a file that is there
    assert!(matches!(
        restore_recovery_code(&bucket, &file, b"another"),
        Err(DriveError::Conflict { .. })
    ));
    assert_eq!(bucket.get(&file).unwrap(), wrap);
    // and only a further recovery code's file is read, removed or put back
    for named in [
        RECOVERY_KEY_FILE,
        ".azlin/keys/_chain.key",
        ".azlin/keys/recovery-xyz.key",
    ] {
        assert!(read_recovery_code(&bucket, named).is_err(), "{named}");
        assert!(
            restore_recovery_code(&bucket, named, b"x").is_err(),
            "{named}"
        );
    }
}

/// X-DELTA-AZDRIVE-KEYS-28: a kit made before a copy was named by its key has a name no key maps
/// to, and the copy of a key that was removed earlier stays in the bucket; each still opens the
/// drive key for its code. The copies that no claimed name covers are found - the first code's
/// `recovery.key` is not a further code's - and a claim takes its copy out of the answer.
#[test]
fn x_delta_azdrive_keys_28_the_copies_no_key_claims_are_found() {
    let bucket = MemBucket::new();
    let owner = MemoryKeyring::new();
    setup_new_drive(&bucket, &owner, "d_1", cheap()).unwrap();
    let (named, _) =
        add_recovery_code_for_key(&bucket, &owner, "d_1", cheap(), |_| String::from("KITKEY="))
            .unwrap();
    let (before, _) = add_recovery_code(&bucket, &owner, "d_1", cheap()).unwrap();
    assert_eq!(
        unclaimed_recovery_codes(&bucket, &[]).unwrap().len(),
        2,
        "nothing claimed: both copies are found, and not the first code's"
    );
    let left = unclaimed_recovery_codes(&bucket, &[recovery_code_file_for("KITKEY=")]).unwrap();
    assert_eq!(
        left,
        vec![before.clone()],
        "the named kit is claimed by its key"
    );
    assert!(!left.contains(&named));
    let none = unclaimed_recovery_codes(&bucket, &[named, before]).unwrap();
    assert!(none.is_empty(), "every copy has its key: {none:?}");
    // a storage that cannot list is an error, never "none left"
    struct Unlistable;
    impl Drive for Unlistable {
        fn list(&self, _: &ListRequest) -> Result<crate::ListPage, DriveError> {
            Err(DriveError::Transport(String::from("cut")))
        }
        fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
            Err(DriveError::NotFound {
                key: key.to_string(),
            })
        }
        fn get_range(&self, key: &str, _: crate::ByteRange) -> Result<Vec<u8>, DriveError> {
            Err(DriveError::NotFound {
                key: key.to_string(),
            })
        }
        fn put(&self, _: &str, _: &[u8]) -> Result<(), DriveError> {
            Ok(())
        }
        fn delete(&self, _: &str) -> Result<(), DriveError> {
            Ok(())
        }
        fn head(&self, key: &str) -> Result<crate::ObjectInfo, DriveError> {
            Err(DriveError::NotFound {
                key: key.to_string(),
            })
        }
    }
    assert!(unclaimed_recovery_codes(&Unlistable, &[]).is_err());
}
