//! A drive's second recovery code (a second emergency kit, D51): sealed like the first in a
//! wrap of its own, either code opens the drive, removing one leaves the other.

use super::mem_bucket::MemBucket;
use crate::{
    crypto::{
        device::{
            add_recovery_code, enroll, other_devices, recover, remove_recovery_code,
            setup_new_drive, EXTRA_RECOVERY_PREFIX,
        },
        keys::{RecoveryCode, RecoveryKdf, RECOVERY_KEY_FILE},
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
    assert!(matches!(
        add_recovery_code(&bucket, &MemoryKeyring::new(), "d_1", cheap()),
        Err(DriveError::Denied { .. })
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
