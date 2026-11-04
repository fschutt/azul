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
        keys::{load_key_chain, KeyChain, RecoveryCode, RecoveryKdf, CHAIN_KEY_FILE},
        CryptoError, DriveKey, ObjectId,
    },
    encrypted::{open_encrypted, Expect, IndexChange, IndexProvider, MemoryIndex, NameIndex},
    keyring::{KeyringStore, MemoryKeyring},
    rotation::{pending, reencrypt_pass, resumable, rotate, Phase, ReencryptState, JOURNAL_FILE},
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

    // Another device cannot resume it: K2 is not there (and what it would start is a rotation
    // of its own, after its own lockdown).
    assert!(resumable(bucket.as_ref(), &s.other_device, DRIVE)
        .unwrap()
        .is_none());
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

// ==== K2: the journal is authenticated ====

/// The journal of a rotation nobody started, as anyone who can write to the bucket would plant
/// it: the ids are readable in a real journal (and the old key's id in every member file).
fn planted_journal(old: &DriveKey, phase: &str) -> Vec<u8> {
    format!(
        r#"{{"format":"azlin-drive-key","version":1,"kind":"rotation","old":"{}","new":"{}","phase":"{phase}"}}"#,
        old.id().to_hex(),
        "ab".repeat(16)
    )
    .into_bytes()
}

/// K2: a journal an attacker plants in the bucket is no rotation of this device. It does not
/// skip the account's lockdown (the apps ask `resumable`), and the rotation that starts replaces
/// it instead of failing on a key it cannot have.
#[test]
fn a_journal_planted_in_the_bucket_is_no_rotation_of_this_device() {
    let s = set_up();
    let bucket: Arc<dyn Drive> = s.bucket.clone();
    s.bucket
        .set(JOURNAL_FILE, planted_journal(&s.old_key, "members"));
    assert!(pending(bucket.as_ref()).unwrap().is_some(), "it is there");
    assert!(
        resumable(bucket.as_ref(), &s.keyring, DRIVE)
            .unwrap()
            .is_none(),
        "but no rotation of this device: the lockdown must not be skipped"
    );

    // The rotation starts anew: the planted journal is replaced and the drive rotated.
    let rotated = rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()).unwrap();
    assert_ne!(rotated.drive_key, s.old_key.id());
    assert!(pending(bucket.as_ref()).unwrap().is_none());
    let drive = open_encrypted(bucket.clone(), &s.keyring, DRIVE, &s.provider).unwrap();
    assert_eq!(drive.get("notes/plan.txt").unwrap(), b"the plan");
    assert!(wrapped_key_ids(&s.provider)
        .iter()
        .all(|id| *id == rotated.drive_key));

    // Garbage in the journal's place is no obstacle either.
    s.bucket.set(JOURNAL_FILE, b"{ not a journal".to_vec());
    assert!(resumable(bucket.as_ref(), &s.keyring, DRIVE)
        .unwrap()
        .is_none());
    rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()).unwrap();
    assert!(pending(bucket.as_ref()).unwrap().is_none());
}

/// K2 (C10): the journal of a rotation that really stopped resumes on the device that wrote it,
/// and only as written: another phase (the attacker reads the ids and moves the phase on, to
/// skip the re-keying and the re-wrapping), another device, or a journal of before the MAC does not.
#[test]
fn only_the_journal_this_device_wrote_resumes_and_as_it_wrote_it() {
    let s = set_up();
    let bucket: Arc<dyn Drive> = s.bucket.clone();
    assert!(
        resumable(bucket.as_ref(), &s.keyring, DRIVE)
            .unwrap()
            .is_none(),
        "none yet"
    );
    s.provider.rekey_failures.store(1, Ordering::SeqCst);
    assert!(rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()).is_err());

    let journal = resumable(bucket.as_ref(), &s.keyring, DRIVE)
        .unwrap()
        .expect("the journal this device wrote");
    assert_eq!(journal.phase, Phase::Started);
    assert!(
        resumable(bucket.as_ref(), &s.other_device, DRIVE)
            .unwrap()
            .is_none(),
        "another device does not have the new key"
    );

    // The phase moved on by hand.
    let mut moved = journal.clone();
    moved.phase = Phase::Members;
    s.bucket
        .set(JOURNAL_FILE, serde_json::to_vec(&moved).unwrap());
    assert!(pending(bucket.as_ref()).unwrap().is_some());
    assert!(resumable(bucket.as_ref(), &s.keyring, DRIVE)
        .unwrap()
        .is_none());

    // A journal of before the MAC.
    let mut old_format = journal.clone();
    old_format.mac = String::new();
    s.bucket
        .set(JOURNAL_FILE, serde_json::to_vec(&old_format).unwrap());
    assert!(resumable(bucket.as_ref(), &s.keyring, DRIVE)
        .unwrap()
        .is_none());

    // The genuine one resumes where it stopped.
    s.bucket
        .set(JOURNAL_FILE, serde_json::to_vec(&journal).unwrap());
    assert!(resumable(bucket.as_ref(), &s.keyring, DRIVE)
        .unwrap()
        .is_some());
    let rotated = rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()).unwrap();
    assert_eq!(rotated.drive_key.to_hex(), journal.new, "the same rotation");
    assert!(pending(bucket.as_ref()).unwrap().is_none());
}

// ==== K1: the key chain keeps the history readable ====

/// The index entry of `path` as it is now.
fn entry_of(provider: &Provider, path: &str) -> crate::encrypted::IndexEntry {
    provider.index.get(path).unwrap().unwrap()
}

/// K1: a rotation used to delete the old key, and every pointer from before it (the history, the
/// trash, the version a restore puts back) was wrapped by it: unreadable for ever. Now the old
/// key is in the drive's key chain, sealed under the new one: the new key opens it, and with it
/// the old wraps.
#[test]
fn the_old_wraps_of_the_history_open_through_the_key_chain_after_a_rotation() {
    let s = set_up();
    let bucket: Arc<dyn Drive> = s.bucket.clone();
    // What the history holds: the entry as it was before the rotation, wrapped by K1.
    let before = entry_of(&s.provider, "photos/a.jpg");
    assert_eq!(
        before.object.as_ref().unwrap().wrapped_key.key_id,
        s.old_key.id()
    );

    let rotated = rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()).unwrap();
    let new_key = device::load_drive_key(&s.keyring, DRIVE).unwrap().unwrap();
    assert_eq!(new_key.id(), rotated.drive_key);

    // The old key is in the bucket, sealed: the new key opens it, the old one cannot open the new.
    let chain = load_key_chain(bucket.as_ref()).unwrap().expect("a chain");
    assert_eq!(chain.older_ids(), vec![s.old_key.id()]);
    let opened = chain.open(DRIVE, &new_key);
    assert_eq!(opened.len(), 1);
    assert_eq!(opened[0], s.old_key);
    assert!(chain.open(DRIVE, &s.old_key).is_empty(), "K1 opens no K2");
    assert!(
        chain.open("d_other", &new_key).is_empty(),
        "a chain is bound to its drive"
    );
    let bytes = s.bucket.object(CHAIN_KEY_FILE).unwrap();
    assert!(
        !String::from_utf8(bytes)
            .unwrap()
            .contains(&crate::crypto::to_hex(s.old_key.as_bytes())),
        "the chain holds no key in the clear"
    );

    // A restore puts the old pointer back at the head: the drive opens it.
    let drive = open_encrypted(bucket.clone(), &s.keyring, DRIVE, &s.provider).unwrap();
    s.provider
        .index
        .apply(vec![IndexChange::Put {
            path: String::from("photos/a.jpg"),
            entry: before,
            expect: Expect::Any,
        }])
        .unwrap();
    assert_eq!(drive.get("photos/a.jpg").unwrap(), b"a photo");
    let mut reader = drive.open_reader("photos/a.jpg").unwrap();
    let mut text = Vec::new();
    std::io::Read::read_to_end(&mut reader, &mut text).unwrap();
    assert_eq!(text, b"a photo");

    // A device that holds nothing but the recovery code reaches the history as well.
    let fresh = MemoryKeyring::new();
    recover(bucket.as_ref(), &fresh, DRIVE, &rotated.recovery_code).unwrap();
    let on_fresh = open_encrypted(bucket.clone(), &fresh, DRIVE, &s.provider).unwrap();
    assert_eq!(on_fresh.get("photos/a.jpg").unwrap(), b"a photo");
}

/// K1: rotations chain: K3 opens K2 opens K1, and a second rotation re-wraps what a restore put
/// back from before the first one.
#[test]
fn two_rotations_chain_all_the_way_back() {
    let s = set_up();
    let bucket: Arc<dyn Drive> = s.bucket.clone();
    let from_k1 = entry_of(&s.provider, "photos/a.jpg");
    rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()).unwrap();
    let k2 = device::load_drive_key(&s.keyring, DRIVE).unwrap().unwrap();
    let from_k2 = entry_of(&s.provider, "photos/a.jpg");

    // A restore brings the pointer from before the first rotation back, then the drive rotates again.
    s.provider
        .index
        .apply(vec![IndexChange::Put {
            path: String::from("photos/a.jpg"),
            entry: from_k1.clone(),
            expect: Expect::Any,
        }])
        .unwrap();
    let rotated = rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()).unwrap();
    let k3 = device::load_drive_key(&s.keyring, DRIVE).unwrap().unwrap();
    assert_eq!(k3.id(), rotated.drive_key);
    assert!(
        wrapped_key_ids(&s.provider).iter().all(|id| *id == k3.id()),
        "the restored pointer was re-wrapped"
    );

    let chain = load_key_chain(bucket.as_ref()).unwrap().unwrap();
    assert_eq!(chain.len(), 2);
    let keys = chain.open(DRIVE, &k3);
    assert_eq!(
        keys.iter().map(|k| k.id()).collect::<Vec<_>>(),
        vec![k2.id(), s.old_key.id()]
    );

    // Both generations of history still open: a pointer wrapped by the first key (two links
    // back) and one wrapped by the second.
    let drive = open_encrypted(bucket.clone(), &s.keyring, DRIVE, &s.provider).unwrap();
    for entry in [from_k1, from_k2] {
        s.provider
            .index
            .apply(vec![IndexChange::Put {
                path: String::from("photos/a.jpg"),
                entry,
                expect: Expect::Any,
            }])
            .unwrap();
        assert_eq!(drive.get("photos/a.jpg").unwrap(), b"a photo");
    }
}

#[test]
fn a_key_chain_file_is_read_with_bounds_and_the_other_devices_count_leaves_it_out() {
    let mut chain = KeyChain::new();
    let (a, b) = (DriveKey::generate().unwrap(), DriveKey::generate().unwrap());
    chain.add(DRIVE, &a, &b).unwrap();
    chain.add(DRIVE, &a, &b).unwrap();
    assert_eq!(chain.len(), 1, "a link for the same key is replaced");
    assert!(chain.add(DRIVE, &a, &a).is_err(), "no key under itself");
    let back = KeyChain::parse(&chain.to_bytes()).unwrap();
    assert_eq!(back, chain);
    assert_eq!(back.open(DRIVE, &b)[0], a);
    // Too many links, an unreasonable size, another kind of file.
    let mut links = String::new();
    for _ in 0..300 {
        links.push_str(&format!(
            r#"{{"older":"{0}","newer":"{0}","nonce":"{1}","sealed":"{2}"}},"#,
            "00".repeat(16),
            "00".repeat(24),
            "00".repeat(48)
        ));
    }
    let many = format!(
        r#"{{"format":"azlin-drive-key","version":1,"kind":"chain","links":[{}]}}"#,
        links.trim_end_matches(',')
    );
    assert!(KeyChain::parse(many.as_bytes()).is_err());
    assert!(KeyChain::parse(&vec![b' '; 300 * 1024]).is_err());
    assert!(KeyChain::parse(
        br#"{"format":"azlin-drive-key","version":1,"kind":"recovery","links":[]}"#
    )
    .is_err());
    assert!(KeyChain::parse(b"nonsense").is_err());

    // The journal and the chain are no devices.
    let s = set_up();
    let bucket: Arc<dyn Drive> = s.bucket.clone();
    let before = device::other_devices(bucket.as_ref(), &s.keyring, DRIVE).unwrap();
    rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()).unwrap();
    assert!(s.bucket.object(CHAIN_KEY_FILE).is_some());
    s.bucket
        .set(JOURNAL_FILE, planted_journal(&s.old_key, "started"));
    let after = device::other_devices(bucket.as_ref(), &s.keyring, DRIVE).unwrap();
    assert_eq!(
        (before, after),
        (1, 0),
        "the other device went; the chain and the journal are none"
    );
}
