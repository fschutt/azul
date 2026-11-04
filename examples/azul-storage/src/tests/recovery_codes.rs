//! A drive's second recovery code (a second emergency kit, D51): sealed like the first in a
//! wrap of its own, either code opens the drive, removing one leaves the other.

use super::mem_bucket::MemBucket;
use crate::{
    crypto::{
        device::{
            add_recovery_code, enroll, other_devices, recover, remove_recovery_code,
            setup_new_drive, EXTRA_RECOVERY_PREFIX, MAX_EXTRA_RECOVERY_CODES,
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
