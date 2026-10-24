//! "I was hacked" with the real drive index: the key rotation runs through the metadata
//! repository's provider, and afterwards nothing of the index opens with the old key.

use std::sync::Arc;

use crate::{
    crypto::{device::setup_new_drive, keys::RecoveryKdf, DriveKey},
    encrypted::open_encrypted,
    keyring::MemoryKeyring,
    meta::{keys, MemoryBucket, MetaIndexProvider, MetaRepo},
    rotation::rotate,
    Drive,
};

const DRIVE: &str = "d_rot_meta";

fn cheap() -> RecoveryKdf {
    RecoveryKdf::with_cost(64, 1, 1).unwrap()
}

/// Whether every object of the repository is sealed with `key` (`"AZM1" | key id | ...`).
fn repository_sealed_by(bucket: &MemoryBucket, key: &DriveKey) -> bool {
    let objects: Vec<(String, Vec<u8>)> = bucket
        .objects()
        .into_iter()
        .filter(|(name, _)| name.starts_with(keys::ROOT))
        .collect();
    !objects.is_empty()
        && objects
            .iter()
            .all(|(_, bytes)| bytes.len() >= 20 && bytes[..4] == *b"AZM1" && bytes[4..20] == key.id().0)
}

#[test]
fn a_rotation_through_the_drive_index_seals_the_index_with_the_new_key() {
    let bucket = Arc::new(MemoryBucket::new());
    let keyring = MemoryKeyring::new();
    let (old_key, _code) = setup_new_drive(bucket.as_ref(), &keyring, DRIVE, cheap()).unwrap();
    let provider = MetaIndexProvider::new("Laptop");
    let plain: Arc<dyn Drive> = bucket.clone();
    let drive = open_encrypted(plain.clone(), &keyring, DRIVE, &provider).unwrap();
    drive.put("notes/plan.txt", b"the plan").unwrap();
    drive.put("photos/a.jpg", b"a photo").unwrap();
    drop(drive);
    assert!(repository_sealed_by(&bucket, &old_key));

    let rotated = rotate(plain.clone(), &keyring, DRIVE, &provider, cheap()).unwrap();
    assert_eq!(rotated.rewrapped, 2);
    let new_key = crate::crypto::device::load_drive_key(&keyring, DRIVE)
        .unwrap()
        .unwrap();
    assert_eq!(new_key.id(), rotated.drive_key);
    assert!(repository_sealed_by(&bucket, &new_key));
    assert!(MetaRepo::open(bucket.clone(), old_key.clone(), "checker", "Checker").is_err());

    let drive = open_encrypted(plain, &keyring, DRIVE, &provider).unwrap();
    assert_eq!(drive.get("notes/plan.txt").unwrap(), b"the plan");
    assert_eq!(drive.get("photos/a.jpg").unwrap(), b"a photo");
}
