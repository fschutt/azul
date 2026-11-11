//! "I was hacked" with the real drive index: the key rotation runs through the metadata
//! repository's provider, and afterwards nothing of the index opens with the old key.

use std::sync::Arc;

use super::faulty::{Fail, Faulty};
use crate::{
    crypto::{
        device::{enroll, load_drive_key, load_key_at, rotation_key_entries, setup_new_drive},
        keys::{RecoveryKdf, CHAIN_KEY_FILE},
        DriveKey,
    },
    encrypted::open_encrypted,
    keyring::MemoryKeyring,
    meta::{keys, MemoryBucket, MetaIndexProvider, MetaRepo},
    rotation::{lockdown_needed, pending, resumable, rotate, JOURNAL_FILE},
    Drive, DriveError,
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

// ==== X-DELTA-AZDRIVE-KEYS-1: a rotation that stopped after the index was sealed under K2 ====

/// A drive with two files, over a bucket that fails where a test says.
struct World {
    mem: Arc<MemoryBucket>,
    faulty: Arc<Faulty>,
    plain: Arc<dyn Drive>,
    keyring: MemoryKeyring,
    provider: MetaIndexProvider,
    old_key: DriveKey,
}

fn world() -> World {
    let mem = Arc::new(MemoryBucket::new());
    let keyring = MemoryKeyring::new();
    let (old_key, _code) = setup_new_drive(mem.as_ref(), &keyring, DRIVE, cheap()).unwrap();
    let faulty = Arc::new(Faulty::new(mem.clone()));
    let plain: Arc<dyn Drive> = faulty.clone();
    let provider = MetaIndexProvider::new("Laptop");
    let drive = open_encrypted(plain.clone(), &keyring, DRIVE, &provider).unwrap();
    drive.put("notes/plan.txt", b"the plan").unwrap();
    drive.put("photos/a.jpg", b"a photo").unwrap();
    drop(drive);
    World {
        mem,
        faulty,
        plain,
        keyring,
        provider,
        old_key,
    }
}

impl World {
    /// A rotation that stops with the repository sealed under K2 and nothing else done: the
    /// network goes while K1 is put into the key chain. Returns K2.
    fn stopped_after_the_switch(&self) -> DriveKey {
        self.faulty.fail_write(CHAIN_KEY_FILE, Fail::Refused, 1);
        assert!(rotate(
            self.plain.clone(),
            &self.keyring,
            DRIVE,
            &self.provider,
            cheap()
        )
        .is_err());
        let (_, next) = rotation_key_entries(DRIVE);
        let k2 = load_key_at(&self.keyring, &next)
            .unwrap()
            .expect("K2 is kept in the keyring");
        assert!(
            repository_sealed_by(&self.mem, &k2),
            "the index is K2's now"
        );
        assert_eq!(
            load_drive_key(&self.keyring, DRIVE).unwrap(),
            Some(self.old_key.clone()),
            "the keyring's drive key is K1 until the Members step"
        );
        k2
    }

    fn files_read(&self) {
        let drive = open_encrypted(self.plain.clone(), &self.keyring, DRIVE, &self.provider)
            .expect("the drive opens");
        assert_eq!(drive.get("notes/plan.txt").unwrap(), b"the plan");
        assert_eq!(drive.get("photos/a.jpg").unwrap(), b"a photo");
    }
}

/// X-DELTA-AZDRIVE-KEYS-1: the journal of a rotation that had sealed the index under K2 is gone
/// (a bucket writer deleted `_rotation.key`). `rotate()` made a NEW K2 and overwrote the only
/// copy of the old one in the keyring: the index, sealed under the lost key, was unreadable for
/// good. Now the K2 that is there is the rotation's K2 again, and the drive reads on.
#[test]
fn x_delta_azdrive_keys_1_a_rotation_whose_journal_is_deleted_goes_on_with_the_key_it_made() {
    let w = world();
    let k2 = w.stopped_after_the_switch();
    w.mem.delete(JOURNAL_FILE).unwrap();
    assert!(resumable(w.plain.as_ref(), &w.keyring, DRIVE)
        .unwrap()
        .is_none());

    let rotated = rotate(w.plain.clone(), &w.keyring, DRIVE, &w.provider, cheap())
        .expect("the manifest is K2's: the rotation goes on with K2");
    assert_eq!(rotated.drive_key, k2.id(), "not a second key");
    assert_eq!(load_drive_key(&w.keyring, DRIVE).unwrap(), Some(k2.clone()));
    assert!(repository_sealed_by(&w.mem, &k2));
    w.files_read();
    assert!(pending(w.plain.as_ref()).unwrap().is_none());
}

/// X-DELTA-AZDRIVE-KEYS-1: the journal replaced by one that is not this device's (another
/// device's rotation, or junk) is the same case.
#[test]
fn x_delta_azdrive_keys_1_a_rotation_whose_journal_is_replaced_goes_on_with_the_key_it_made() {
    let w = world();
    let k2 = w.stopped_after_the_switch();
    w.mem
        .put(
            JOURNAL_FILE,
            format!(
                r#"{{"format":"azlin-drive-key","version":1,"kind":"rotation","old":"{}","new":"{}","phase":"members"}}"#,
                w.old_key.id().to_hex(),
                "ab".repeat(16)
            )
            .as_bytes(),
        )
        .unwrap();
    assert!(pending(w.plain.as_ref()).unwrap().is_some(), "it is there");
    assert!(
        resumable(w.plain.as_ref(), &w.keyring, DRIVE)
            .unwrap()
            .is_none(),
        "and it is none of this device's"
    );
    assert!(
        lockdown_needed(w.plain.as_ref(), &w.keyring, DRIVE).unwrap(),
        "so the lockdown runs once more, which is harmless"
    );

    let rotated = rotate(w.plain.clone(), &w.keyring, DRIVE, &w.provider, cheap()).unwrap();
    assert_eq!(rotated.drive_key, k2.id());
    w.files_read();
}

/// X-DELTA-AZDRIVE-KEYS-1: another device rotated the drive; this one still holds K1 and presses
/// "I was hacked". Nothing opens with its key: it is refused BEFORE the lockdown (which would
/// lock the other devices out of a drive whose key it cannot replace), and nothing is written -
/// no K2 in the keyring, no journal in the bucket.
#[test]
fn x_delta_azdrive_keys_1_a_device_whose_key_opens_no_index_is_refused_before_the_lockdown() {
    let w = world();
    // Device B has K1 and rotates the drive; the first keyring (device A) still holds K1.
    let b = MemoryKeyring::new();
    enroll(w.mem.as_ref(), &b, DRIVE, &w.old_key).unwrap();
    rotate(w.plain.clone(), &b, DRIVE, &w.provider, cheap()).unwrap();
    assert_eq!(
        load_drive_key(&w.keyring, DRIVE).unwrap(),
        Some(w.old_key.clone())
    );

    let refused = lockdown_needed(w.plain.as_ref(), &w.keyring, DRIVE);
    assert!(
        matches!(refused, Err(DriveError::Denied { .. })),
        "refused before the lockdown: {refused:?}"
    );
    let again = rotate(w.plain.clone(), &w.keyring, DRIVE, &w.provider, cheap());
    assert!(matches!(again, Err(DriveError::Denied { .. })), "{again:?}");
    // The sentence goes to the dialog as it is: one space between its words (it was once written
    // with a line continuation that left runs of fourteen spaces in it).
    if let Err(DriveError::Denied { message }) = &again {
        assert!(
            !message.contains("  "),
            "runs of spaces in the refusal: {message:?}"
        );
    }
    let (previous, next) = rotation_key_entries(DRIVE);
    assert!(
        load_key_at(&w.keyring, &next).unwrap().is_none(),
        "no K2 was made"
    );
    assert!(load_key_at(&w.keyring, &previous).unwrap().is_none());
    assert!(
        pending(w.plain.as_ref()).unwrap().is_none(),
        "no journal was written"
    );
}
