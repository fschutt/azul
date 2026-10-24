//! "I was hacked": the key rotation takes the old drive key out of every use, resumes after an
//! interruption, stops cleanly where the index cannot be rekeyed, and "re-encrypt everything"
//! leaves no object the old key opens.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use super::mem_bucket::MemBucket;
use crate::{
    crypto::{
        azl1::OpenObject,
        device::{self, enroll, recover, rotation_key_entries, seal_invite, setup_new_drive},
        drops::{enable_drop, load_previous_drop_key},
        keys::{RecoveryCode, RecoveryKdf},
        CryptoError, DriveKey, ObjectId,
    },
    encrypted::{open_encrypted, IndexProvider, MemoryIndex, NameIndex},
    keyring::{KeyringStore, MemoryKeyring},
    rotation::{pending, reencrypt_pass, rotate, Phase, ReencryptState, JOURNAL_FILE},
    sharing::{list_shares, share},
    time::now_unix,
    Drive, DriveError, ListRequest,
};

const DRIVE: &str = "d_rot";

fn cheap() -> RecoveryKdf {
    RecoveryKdf::with_cost(64, 1, 1).unwrap()
}

/// The drive's index in memory; `rekey` works (nothing of a memory index is sealed), fails
/// once on request, or is not there at all.
struct Provider {
    index: Arc<MemoryIndex>,
    rekey_failures: AtomicUsize,
    can_rekey: bool,
}

impl Provider {
    fn new() -> Provider {
        Provider {
            index: Arc::new(MemoryIndex::new()),
            rekey_failures: AtomicUsize::new(0),
            can_rekey: true,
        }
    }
}

impl IndexProvider for Provider {
    fn open_index(
        &self,
        _drive: &str,
        _bucket: Arc<dyn Drive>,
        _drive_key: &DriveKey,
    ) -> Result<Arc<dyn NameIndex>, DriveError> {
        Ok(self.index.clone())
    }

    fn rekey(
        &self,
        _drive: &str,
        _bucket: Arc<dyn Drive>,
        _old: &DriveKey,
        _new: &DriveKey,
    ) -> Result<(), DriveError> {
        if !self.can_rekey {
            return Err(DriveError::Unsupported(String::from("no rekey here")));
        }
        if self.rekey_failures.load(Ordering::SeqCst) > 0 {
            self.rekey_failures.fetch_sub(1, Ordering::SeqCst);
            return Err(DriveError::Io(String::from("the network went")));
        }
        Ok(())
    }
}

/// An encrypted drive with files, a second device, an open invite, incoming mail and a share.
struct Setup {
    bucket: Arc<MemBucket>,
    keyring: MemoryKeyring,
    other_device: MemoryKeyring,
    provider: Provider,
    old_key: DriveKey,
    old_code: RecoveryCode,
}

fn set_up() -> Setup {
    let bucket = Arc::new(MemBucket::new());
    let keyring = MemoryKeyring::new();
    let (old_key, old_code) = setup_new_drive(bucket.as_ref(), &keyring, DRIVE, cheap()).unwrap();
    let provider = Provider::new();
    let drive = open_encrypted(bucket.clone(), &keyring, DRIVE, &provider).unwrap();
    drive.put("notes/plan.txt", b"the plan").unwrap();
    drive.put("photos/a.jpg", b"a photo").unwrap();
    drive.copy("notes/plan.txt", "backup/plan.txt").unwrap();
    let other_device = MemoryKeyring::new();
    enroll(bucket.as_ref(), &other_device, DRIVE, &old_key).unwrap();
    seal_invite(bucket.as_ref(), DRIVE, &old_key).unwrap();
    enable_drop(bucket.as_ref(), &old_key, DRIVE).unwrap();
    share(&drive, &["notes/plan.txt"], None, None, None).unwrap();
    Setup {
        bucket,
        keyring,
        other_device,
        provider,
        old_key,
        old_code,
    }
}

fn wrapped_key_ids(provider: &Provider) -> Vec<crate::crypto::KeyId> {
    provider
        .index
        .list(&ListRequest::recursive(""))
        .unwrap()
        .entries
        .iter()
        .filter_map(|(_, e)| e.object.as_ref().map(|o| o.wrapped_key.key_id))
        .collect()
}

#[test]
fn a_rotation_takes_the_old_key_out_of_every_use() {
    let s = set_up();
    let bucket: Arc<dyn Drive> = s.bucket.clone();
    let old_drop = device::load_drive_key(&s.keyring, DRIVE)
        .unwrap()
        .and_then(|k| crate::crypto::drops::load_drop_key(bucket.as_ref(), &k, DRIVE).unwrap())
        .unwrap()
        .public();

    let rotated = rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()).unwrap();
    assert_ne!(rotated.drive_key, s.old_key.id());
    assert_eq!(rotated.rewrapped, 3, "three entries, the copy among them");
    assert_eq!(rotated.members_removed, 3, "the other device, the invite, this device's old wrap");
    assert_eq!(rotated.shares_revoked, 1);
    let new_drop = rotated.drop_key.expect("incoming mail was on");
    assert_ne!(new_drop, old_drop);

    // The keyring's drive key is K2; the rotation's entries and journal are gone.
    let new_key = device::load_drive_key(&s.keyring, DRIVE).unwrap().unwrap();
    assert_eq!(new_key.id(), rotated.drive_key);
    let (previous, next) = rotation_key_entries(DRIVE);
    assert_eq!(s.keyring.get(&previous).unwrap(), None);
    assert_eq!(s.keyring.get(&next).unwrap(), None);
    assert!(pending(bucket.as_ref()).unwrap().is_none());
    assert!(s.bucket.object(JOURNAL_FILE).is_none());

    // Every entry is K2's; the files read the same; K1 opens none of them.
    assert!(wrapped_key_ids(&s.provider).iter().all(|id| *id == new_key.id()));
    let drive = open_encrypted(bucket.clone(), &s.keyring, DRIVE, &s.provider).unwrap();
    assert_eq!(drive.get("notes/plan.txt").unwrap(), b"the plan");
    assert_eq!(drive.get("backup/plan.txt").unwrap(), b"the plan");
    assert_eq!(drive.get("photos/a.jpg").unwrap(), b"a photo");
    let entry = drive.entry("photos/a.jpg").unwrap().object.unwrap();
    assert!(matches!(
        s.old_key.unwrap_file_key(&entry.wrapped_key, &entry.id),
        Err(CryptoError::WrongKey)
    ));

    // The other device: its wrap is gone, and the K1 it keeps opens nothing.
    let on_other = open_encrypted(bucket.clone(), &s.other_device, DRIVE, &s.provider).unwrap();
    assert!(matches!(
        on_other.get("notes/plan.txt"),
        Err(DriveError::Denied { .. })
    ));
    let fresh = MemoryKeyring::new();
    assert!(device::unlock(bucket.as_ref(), &fresh, DRIVE).unwrap().is_none());
    // This device's own (new) wrap opens K2.
    s.keyring.delete(&device::drive_key_entry(DRIVE)).unwrap();
    assert_eq!(
        device::unlock(bucket.as_ref(), &s.keyring, DRIVE).unwrap().map(|k| k.id()),
        Some(new_key.id())
    );

    // The old recovery code opens nothing; the new one opens K2.
    assert!(matches!(
        recover(bucket.as_ref(), &MemoryKeyring::new(), DRIVE, &s.old_code),
        Err(DriveError::Denied { .. })
    ));
    assert_eq!(
        recover(bucket.as_ref(), &MemoryKeyring::new(), DRIVE, &rotated.recovery_code)
            .unwrap()
            .id(),
        new_key.id()
    );
    // Drops the Worker sealed before it got the new key still open; no share is left.
    assert_eq!(
        load_previous_drop_key(bucket.as_ref(), &new_key, DRIVE)
            .unwrap()
            .map(|k| k.public()),
        Some(old_drop)
    );
    assert!(list_shares(bucket.as_ref()).unwrap().is_empty());
}

#[test]
fn an_interrupted_rotation_resumes_on_the_device_that_started_it() {
    let s = set_up();
    let bucket: Arc<dyn Drive> = s.bucket.clone();
    s.provider.rekey_failures.store(1, Ordering::SeqCst);
    assert!(matches!(
        rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()),
        Err(DriveError::Io(_))
    ));
    let journal = pending(bucket.as_ref()).unwrap().unwrap();
    assert_eq!(journal.phase, Phase::Started);
    assert_eq!(journal.old, s.old_key.id().to_hex());
    let journal_bytes = s.bucket.object(JOURNAL_FILE).unwrap();
    let new_key = device::load_key_at(&s.keyring, &rotation_key_entries(DRIVE).1)
        .unwrap()
        .unwrap();
    assert!(
        !String::from_utf8(journal_bytes).unwrap().contains(&crate::crypto::to_hex(new_key.as_bytes())),
        "the journal names the keys, never holds one"
    );

    // Another device cannot finish it: K2 is not there.
    assert!(matches!(
        rotate(bucket.clone(), &s.other_device, DRIVE, &s.provider, cheap()),
        Err(DriveError::Denied { .. })
    ));
    // The device that started it does.
    let rotated = rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()).unwrap();
    assert_eq!(rotated.drive_key, new_key.id());
    assert!(pending(bucket.as_ref()).unwrap().is_none());
    let drive = open_encrypted(bucket, &s.keyring, DRIVE, &s.provider).unwrap();
    assert_eq!(drive.get("notes/plan.txt").unwrap(), b"the plan");
}

#[test]
fn an_index_that_cannot_be_rekeyed_stops_the_rotation_before_anything_changed() {
    let mut s = set_up();
    s.provider.can_rekey = false;
    let bucket: Arc<dyn Drive> = s.bucket.clone();
    let keys_before: Vec<String> = s.bucket.keys();
    assert!(matches!(
        rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()),
        Err(DriveError::Unsupported(_))
    ));
    assert!(pending(bucket.as_ref()).unwrap().is_none());
    assert_eq!(s.bucket.keys(), keys_before, "the bucket is as it was");
    assert!(wrapped_key_ids(&s.provider).iter().all(|id| *id == s.old_key.id()));
    let (previous, next) = rotation_key_entries(DRIVE);
    assert_eq!(s.keyring.get(&previous).unwrap(), None);
    assert_eq!(s.keyring.get(&next).unwrap(), None);
    let drive = open_encrypted(bucket, &s.keyring, DRIVE, &s.provider).unwrap();
    assert_eq!(drive.get("photos/a.jpg").unwrap(), b"a photo");
}

#[test]
fn re_encrypting_everything_leaves_no_object_the_old_key_opens() {
    let s = set_up();
    let bucket: Arc<dyn Drive> = s.bucket.clone();
    rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()).unwrap();
    let new_key = device::load_drive_key(&s.keyring, DRIVE).unwrap().unwrap();
    let drive = open_encrypted(bucket.clone(), &s.keyring, DRIVE, &s.provider).unwrap();
    let dates_before: Vec<Option<u64>> = ["notes/plan.txt", "photos/a.jpg"]
        .iter()
        .map(|p| drive.entry(p).unwrap().modified)
        .collect();

    let mut state = ReencryptState::new(now_unix() + 1);
    let mut saves = 0;
    assert!(reencrypt_pass(&drive, &mut state, &mut |_| {
        saves += 1;
        Ok(())
    }, &|| false)
    .unwrap());
    assert_eq!(state.done, 3);
    assert_eq!(saves, 3);
    assert_eq!(ReencryptState::from_json(&state.to_json()).unwrap(), state);

    for key in s.bucket.keys() {
        let Some(id) = ObjectId::from_bucket_key(&key) else {
            continue;
        };
        let bytes = s.bucket.object(&key).unwrap();
        assert!(
            OpenObject::open_with_drive_key(&bytes[..], &id, &s.old_key).is_err(),
            "{key} still opens with the old key"
        );
        assert!(OpenObject::open_with_drive_key(&bytes[..], &id, &new_key).is_ok());
    }
    assert_eq!(drive.get("notes/plan.txt").unwrap(), b"the plan");
    assert_eq!(drive.get("backup/plan.txt").unwrap(), b"the plan");
    let dates_after: Vec<Option<u64>> = ["notes/plan.txt", "photos/a.jpg"]
        .iter()
        .map(|p| drive.entry(p).unwrap().modified)
        .collect();
    assert_eq!(dates_after, dates_before, "re-encryption is no edit");

    // A stopped pass goes on after the last file it did.
    let mut stopped = ReencryptState::new(now_unix() + 1);
    let asked = AtomicUsize::new(0);
    let stop_after_one = || asked.fetch_add(1, Ordering::SeqCst) >= 1;
    assert!(!reencrypt_pass(&drive, &mut stopped, &mut |_| Ok(()), &stop_after_one).unwrap());
    assert_eq!((stopped.done, stopped.cursor.as_deref()), (1, Some("backup/plan.txt")));
    assert!(reencrypt_pass(&drive, &mut stopped, &mut |_| Ok(()), &|| false).unwrap());
    assert_eq!(stopped.done, 3);
}
