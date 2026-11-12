//! "I was hacked": the key rotation takes the old drive key out of every use, resumes after an
//! interruption, stops cleanly where the index cannot be rekeyed, and "re-encrypt everything"
//! leaves no object the old key opens.

use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use super::{
    asked::{lockdown_needed, reencrypt_pass, rotate},
    faulty::{Fail, Faulty},
    mem_bucket::MemBucket,
};
use crate::{
    crypto::{
        azl1::OpenObject,
        device::{
            self, add_recovery_code, enroll, recover, rotation_key_entries, seal_invite,
            setup_new_drive,
        },
        drops::{
            enable_drop, load_drop_key, load_previous_drop_key, load_previous_drop_keys,
            DROP_KEY_FILE, PREVIOUS_DROP_KEY_FILE,
        },
        keys::{
            load_key_chain, KeyChain, RecoveryCode, RecoveryKdf, CHAIN_KEY_FILE, RECOVERY_KEY_FILE,
        },
        CryptoError, DriveKey, ObjectId,
    },
    encrypted::{open_encrypted, Expect, IndexChange, IndexProvider, MemoryIndex, NameIndex},
    keyring::{KeyringError, KeyringStore, MemoryKeyring},
    rotation::{pending, resumable, unfinished, Phase, ReencryptState, JOURNAL_FILE},
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
    let permit = device::drop_key_permit(bucket.as_ref(), &keyring, DRIVE, &old_key).unwrap();
    enable_drop(&permit, bucket.as_ref(), &old_key, DRIVE).unwrap();
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

/// X-DELTA-AZDRIVE-KEYS-27: a rotation deletes the further recovery codes' wraps (each sealed the
/// old key) and counted them among the members - "3 other computers removed" for two computers and
/// a kit in a drawer. Counted apart; the kit's paper opens nothing afterwards.
#[test]
fn x_delta_azdrive_keys_27_a_rotation_counts_the_further_recovery_codes_apart_from_the_members() {
    let s = set_up();
    let (kit_id, kit) = add_recovery_code(s.bucket.as_ref(), &s.keyring, DRIVE, cheap()).unwrap();
    let kit_file = s
        .bucket
        .keys()
        .into_iter()
        .find(|key| key.contains("recovery-"))
        .expect("the kit's wrap is in the bucket");
    assert!(kit_file.contains(&kit_id), "{kit_file}");
    let bucket: Arc<dyn Drive> = s.bucket.clone();

    let rotated = rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()).unwrap();
    assert_eq!(
        rotated.members_removed, 3,
        "the other device, the invite, this device's old wrap - and not the kit"
    );
    assert_eq!(rotated.codes_removed, 1, "the kit");
    assert!(s.bucket.object(&kit_file).is_none(), "its wrap is gone");
    // its paper opens nothing now
    let elsewhere = MemoryKeyring::new();
    assert!(recover(bucket.as_ref(), &elsewhere, DRIVE, &kit).is_err());
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
    assert!(reencrypt_pass(
        &s.keyring,
        DRIVE,
        &drive,
        &mut state,
        &mut |_| {
            saves += 1;
            Ok(())
        },
        &|| false
    )
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
    assert!(!reencrypt_pass(
        &s.keyring,
        DRIVE,
        &drive,
        &mut stopped,
        &mut |_| Ok(()),
        &stop_after_one
    )
    .unwrap());
    assert_eq!(
        (stopped.done, stopped.cursor.as_deref()),
        (1, Some("backup/plan.txt"))
    );
    assert!(reencrypt_pass(
        &s.keyring,
        DRIVE,
        &drive,
        &mut stopped,
        &mut |_| Ok(()),
        &|| false
    )
    .unwrap());
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

// ==== X-DELTA-AZDRIVE-KEYS: the delta review of the key lanes ====

/// X-DELTA-AZDRIVE-KEYS-6: a journal over the size a journal has (one byte past 64 KiB) used to
/// answer `Protocol` (a bucket that checks after the read) or a transport error (one that refuses
/// the answer on its way): the check "is a rotation of this device under way" ended in an error
/// and "I was hacked" never reached its lockdown. A device without the new key of a rotation has
/// no journal of its own; the bucket is not even asked.
#[test]
fn x_delta_azdrive_keys_6_a_planted_journal_over_the_limit_stops_neither_the_check_nor_the_rotation(
) {
    let s = set_up();
    let bucket: Arc<dyn Drive> = s.bucket.clone();
    s.bucket.set(JOURNAL_FILE, vec![b' '; 64 * 1024 + 1]);
    assert!(
        resumable(bucket.as_ref(), &s.keyring, DRIVE)
            .unwrap()
            .is_none(),
        "no K2 on this device: the planted file is none of its journals"
    );
    assert!(
        lockdown_needed(bucket.as_ref(), &s.keyring, DRIVE).unwrap(),
        "the account's lockdown runs"
    );

    // The same on a device whose own rotation stopped: its K2 is there, and the journal was
    // replaced by the big file. Not its journal; the rotation goes on with the K2 it has.
    s.provider.rekey_failures.store(1, Ordering::SeqCst);
    assert!(rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()).is_err());
    let (_, next) = rotation_key_entries(DRIVE);
    let k2 = device::load_key_at(&s.keyring, &next).unwrap().unwrap();
    s.bucket.set(JOURNAL_FILE, vec![b' '; 64 * 1024 + 1]);
    assert!(resumable(bucket.as_ref(), &s.keyring, DRIVE)
        .unwrap()
        .is_none());
    assert!(lockdown_needed(bucket.as_ref(), &s.keyring, DRIVE).unwrap());
    let rotated = rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()).unwrap();
    assert_eq!(
        rotated.drive_key,
        k2.id(),
        "the K2 it had, not a second one"
    );
    assert!(pending(bucket.as_ref()).unwrap().is_none());
}

/// X-DELTA-AZDRIVE-KEYS-6: a key chain file that is garbage, or too big to be one, used to stop
/// `open_encrypted` for every device (the doc said "fewer keys") and every rotation. Nothing it
/// holds can be opened or trusted: the drive opens without older keys, and a rotation starts the
/// chain anew with its own link.
#[test]
fn x_delta_azdrive_keys_6_a_chain_file_that_is_garbage_stops_no_open_and_no_rotation() {
    let s = set_up();
    let bucket: Arc<dyn Drive> = s.bucket.clone();
    for planted in [b"{ not a chain".to_vec(), vec![b' '; 300 * 1024]] {
        s.bucket.set(CHAIN_KEY_FILE, planted);
        let drive = open_encrypted(bucket.clone(), &s.keyring, DRIVE, &s.provider)
            .expect("a damaged chain gives no older keys, and the drive opens");
        assert_eq!(drive.get("notes/plan.txt").unwrap(), b"the plan");
    }
    let rotated = rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()).unwrap();
    let new_key = device::load_drive_key(&s.keyring, DRIVE).unwrap().unwrap();
    assert_eq!(new_key.id(), rotated.drive_key);
    let chain = load_key_chain(bucket.as_ref())
        .unwrap()
        .expect("a chain of its own");
    assert_eq!(chain.open(DRIVE, &new_key), vec![s.old_key.clone()]);
}

/// An index entry whose file key is wrapped by a key nobody here holds: what a restore as of a time before
/// an earlier rotation puts back at the head of a drive whose key chain is gone. The object and the entry
/// are the file's own; only the wrap of its file key is another key's (`k0`).
fn wrapped_by_a_key_nobody_holds(
    s: &Setup,
    path: &str,
) -> (DriveKey, crate::encrypted::IndexEntry) {
    let k0 = DriveKey::generate().unwrap();
    let entry = s
        .provider
        .index
        .get(path)
        .unwrap()
        .expect("the file is in the index");
    let object = entry.object.clone().expect("a file");
    let bound = object.bucket_object();
    let file_key = s
        .old_key
        .unwrap_file_key(&object.wrapped_key, &bound)
        .unwrap();
    let mut planted = entry.clone();
    planted.object.as_mut().unwrap().wrapped_key = k0.wrap_file_key(&file_key, &bound).unwrap();
    s.provider
        .index
        .apply(vec![IndexChange::Put {
            path: path.to_string(),
            entry: planted.clone(),
            expect: Expect::Object(object.id),
        }])
        .unwrap();
    (k0, planted)
}

/// X-DELTA-KEYS3-7: a drive rotated before whose head holds a pointer wrapped by an older key (a restore as
/// of a time before the earlier rotation puts such pointers back) and whose key chain is gone - nobody can
/// open that pointer. The re-wrap returned an error for it, AFTER the index was sealed under the new key,
/// and so did every retry: DK = K1 on this device, the index K2's, the other devices locked out. An entry
/// nobody can unwrap is unreadable either way: it is left exactly as it is, counted, said in `left_over`, and
/// the rotation goes on and ends.
#[test]
fn x_delta_keys3_7_an_entry_no_key_opens_is_left_as_it_is_and_counted_and_the_rotation_ends() {
    let s = set_up();
    let bucket: Arc<dyn Drive> = s.bucket.clone();
    let (_k0, planted) = wrapped_by_a_key_nobody_holds(&s, "photos/a.jpg");
    assert!(
        s.bucket.object(CHAIN_KEY_FILE).is_none(),
        "premise: no key chain"
    );

    let rotated = rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap())
        .expect("one entry nobody can read does not stop the key change");
    assert_eq!(rotated.unwrapped, 1);
    assert_eq!(rotated.rewrapped, 2, "the other two entries");
    assert!(
        rotated
            .left_over
            .iter()
            .any(|left| left.contains("1 file(s)") && left.contains("stay as they were")),
        "{:?}",
        rotated.left_over
    );
    // Never deleted, never rewritten: the entry is the one the planting left, byte for byte.
    assert_eq!(s.provider.index.get("photos/a.jpg").unwrap(), Some(planted));
    // The key change is over: the keyring holds the new key, and the other files read with it.
    let new_key = device::load_drive_key(&s.keyring, DRIVE).unwrap().unwrap();
    assert_eq!(new_key.id(), rotated.drive_key);
    assert!(
        pending(bucket.as_ref()).unwrap().is_none(),
        "the journal is gone"
    );
    let drive = open_encrypted(bucket, &s.keyring, DRIVE, &s.provider).unwrap();
    assert_eq!(drive.get("notes/plan.txt").unwrap(), b"the plan");
    assert!(
        drive.get("photos/a.jpg").is_err(),
        "and the one nobody can open stays unreadable"
    );
}

/// X-DELTA-KEYS3-7: the same on a resume - the rotation that stopped after the index was sealed under the new
/// key (the journal's `Rekeyed` write refused) goes on past the entry, counts it, and ends. On the old code
/// this is the stuck state of the review: the index is K2's, DK is K1, and every retry stops at the entry.
#[test]
fn x_delta_keys3_7_a_stopped_rotation_resumes_past_an_entry_no_key_opens() {
    let s = set_up();
    wrapped_by_a_key_nobody_holds(&s, "backup/plan.txt");
    let faulty = Arc::new(Faulty::new(s.bucket.clone()));
    let bucket: Arc<dyn Drive> = faulty.clone();
    // The journal is written twice before the re-wrap (Started, then Rekeyed): the second is refused.
    faulty.fail_nth_write_ending("_rotation.key", 2, Fail::Refused);
    assert!(rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()).is_err());
    assert_eq!(
        pending(bucket.as_ref()).unwrap().unwrap().phase,
        Phase::Started
    );
    assert!(
        s.bucket.object(CHAIN_KEY_FILE).is_some(),
        "premise: the index is the new key's and K1 is in the chain"
    );

    let rotated = rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap())
        .expect("the resume goes on past the entry nobody can open");
    assert_eq!(rotated.unwrapped, 1);
    assert_eq!(rotated.rewrapped, 2);
    let new_key = device::load_drive_key(&s.keyring, DRIVE).unwrap().unwrap();
    assert_eq!(new_key.id(), rotated.drive_key);
    assert!(pending(bucket.as_ref()).unwrap().is_none());
}

/// X-DELTA-KEYS3-7: the helper itself, on the index: every entry the re-wrap could not open is counted, and
/// the ones it could are done.
#[test]
fn x_delta_keys3_7_the_rewrap_counts_what_it_could_not_unwrap_and_changes_nothing_of_it() {
    let s = set_up();
    let (_k0, planted) = wrapped_by_a_key_nobody_holds(&s, "photos/a.jpg");
    let new_key = DriveKey::generate().unwrap();
    let done =
        crate::rotation::rewrap_index(s.provider.index.as_ref(), &s.old_key, &new_key).unwrap();
    assert_eq!(
        done,
        crate::rotation::Rewrapped {
            done: 2,
            unwrapped: 1
        }
    );
    assert_eq!(s.provider.index.get("photos/a.jpg").unwrap(), Some(planted));
    // Again: nothing left to do, the same one counted.
    let again =
        crate::rotation::rewrap_index(s.provider.index.as_ref(), &s.old_key, &new_key).unwrap();
    assert_eq!(
        again,
        crate::rotation::Rewrapped {
            done: 0,
            unwrapped: 1
        }
    );
}

/// X-DELTA-KEYS3-7: the key chain file of a rotation that is committed is never started over. The rotation
/// stopped after its Members step and a chain file that is no chain was planted: its last chain step finds a
/// file it cannot read, says so in words that tell what to do, and writes nothing over it; with the file
/// removed the same call goes on and ends. (Its FIRST chain step still starts a chain over a damaged file:
/// `x_delta_azdrive_keys_6_...`, which a planted file must not be able to stop.)
#[test]
fn x_delta_keys3_7_a_chain_file_damaged_after_the_members_step_is_not_written_over() {
    let s = set_up();
    let faulty = Arc::new(Faulty::new(s.bucket.clone()));
    let bucket: Arc<dyn Drive> = faulty.clone();
    // The drop key step stops the rotation after its first chain step and the Members step.
    faulty.fail_write(DROP_KEY_FILE, Fail::Refused, 1);
    assert!(rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()).is_err());
    assert_eq!(
        pending(bucket.as_ref()).unwrap().unwrap().phase,
        Phase::Members
    );
    let planted = b"{ a chain that is none".to_vec();
    s.bucket.set(CHAIN_KEY_FILE, planted.clone());

    let refused = rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap());
    match &refused {
        Err(DriveError::Corrupt { key, reason }) => {
            assert_eq!(key, CHAIN_KEY_FILE);
            assert!(reason.contains("remove the file"), "{reason}");
            assert!(reason.contains("nothing of it was changed"), "{reason}");
        }
        other => {
            panic!("a chain file nobody can read is refused after the Members step: {other:?}")
        }
    }
    assert_eq!(
        s.bucket.object(CHAIN_KEY_FILE).unwrap(),
        planted,
        "byte-identical"
    );
    assert!(s.bucket.object(RECOVERY_KEY_FILE).is_some());

    // Removed, the file is no chain: the rotation starts one with its link and ends.
    s.bucket.delete(CHAIN_KEY_FILE).unwrap();
    let rotated = rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()).unwrap();
    let new_key = device::load_drive_key(&s.keyring, DRIVE).unwrap().unwrap();
    assert_eq!(new_key.id(), rotated.drive_key);
    let chain = load_key_chain(bucket.as_ref()).unwrap().expect("a chain");
    assert_eq!(chain.open(DRIVE, &new_key), vec![s.old_key.clone()]);
}

/// X-DELTA-AZDRIVE-KEYS-6: the chain takes the first link of a key it finds; a link somebody
/// planted for the same key, ahead of the genuine one, ended the walk and hid K1.
#[test]
fn x_delta_azdrive_keys_6_a_planted_link_does_not_hide_the_genuine_one() {
    let (a, b) = (DriveKey::generate().unwrap(), DriveKey::generate().unwrap());
    let mut chain = KeyChain::new();
    chain.add(DRIVE, &a, &b).unwrap();
    let mut json: serde_json::Value = serde_json::from_slice(&chain.to_bytes()).unwrap();
    let bogus = serde_json::json!({
        "older": "ab".repeat(16),
        "newer": b.id().to_hex(),
        "nonce": "00".repeat(24),
        "sealed": "00".repeat(48),
    });
    json["links"].as_array_mut().unwrap().insert(0, bogus);
    let planted = KeyChain::parse(&serde_json::to_vec(&json).unwrap()).unwrap();
    assert_eq!(planted.len(), 2);
    assert_eq!(planted.open(DRIVE, &b), vec![a], "the genuine link opens");
}

/// X-DELTA-AZDRIVE-KEYS-6: the rotation reads the chain back and does not let go of K1 unless K2
/// opens it from there: a chain write that did not keep it stops the rotation with the old key
/// still on this device.
#[test]
fn x_delta_azdrive_keys_6_the_old_key_stays_unless_the_chain_holds_it() {
    let s = set_up();
    // The chain put is answered Ok, and the file is not what was written (a writer replaced it).
    struct Replaced(Arc<MemBucket>);
    impl Drive for Replaced {
        fn list(&self, r: &ListRequest) -> Result<crate::ListPage, DriveError> {
            self.0.list(r)
        }
        fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
            self.0.get(key)
        }
        fn get_range(&self, key: &str, range: crate::ByteRange) -> Result<Vec<u8>, DriveError> {
            self.0.get_range(key, range)
        }
        fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
            if key == CHAIN_KEY_FILE {
                return self.0.put(key, b"{ planted over it");
            }
            self.0.put(key, bytes)
        }
        fn delete(&self, key: &str) -> Result<(), DriveError> {
            self.0.delete(key)
        }
        fn head(&self, key: &str) -> Result<crate::ObjectInfo, DriveError> {
            self.0.head(key)
        }
    }
    let replaced: Arc<dyn Drive> = Arc::new(Replaced(s.bucket.clone()));
    let err = rotate(replaced, &s.keyring, DRIVE, &s.provider, cheap()).unwrap_err();
    assert!(
        matches!(err, DriveError::Io(ref why) if why.contains("key chain")),
        "{err:?}"
    );
    let (previous, _) = rotation_key_entries(DRIVE);
    assert!(
        device::load_key_at(&s.keyring, &previous)
            .unwrap()
            .is_some(),
        "K1 is still in the keyring: nothing else holds it"
    );
}

/// X-DELTA-AZDRIVE-KEYS-7: the recovery wrap was replaced before the drop key and the shares; a
/// failure after it left a wrap sealed under a code nobody saw while the old sheet's code passed
/// every drill. The wrap is the last step that can fail: before it, the old wrap and its code
/// stand as they were, and the retry makes the new code.
#[test]
fn x_delta_azdrive_keys_7_a_failure_before_the_recovery_wrap_leaves_the_old_wrap_and_its_code() {
    let s = set_up();
    let faulty = Arc::new(Faulty::new(s.bucket.clone()));
    let bucket: Arc<dyn Drive> = faulty.clone();
    let wrap_before = s.bucket.object(RECOVERY_KEY_FILE).unwrap();
    faulty.fail_write(DROP_KEY_FILE, Fail::Refused, 1);
    assert!(rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()).is_err());
    assert_eq!(
        s.bucket.object(RECOVERY_KEY_FILE).unwrap(),
        wrap_before,
        "the wrap of the old sheet is the wrap still"
    );

    // The journal says Members: the same call goes on and ends with the code the wrap holds.
    let rotated = rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()).unwrap();
    assert_ne!(s.bucket.object(RECOVERY_KEY_FILE).unwrap(), wrap_before);
    let new_key = device::load_drive_key(&s.keyring, DRIVE).unwrap().unwrap();
    assert_eq!(
        recover(
            bucket.as_ref(),
            &MemoryKeyring::new(),
            DRIVE,
            &rotated.recovery_code
        )
        .unwrap()
        .id(),
        new_key.id()
    );
    assert!(matches!(
        recover(bucket.as_ref(), &MemoryKeyring::new(), DRIVE, &s.old_code),
        Err(DriveError::Denied { .. })
    ));
}

/// X-DELTA-AZDRIVE-KEYS-7: a drop key file somebody planted (junk that opens with nothing) made
/// `rotate_drop_key` fail on every retry. It is no drop key: the rotation does not stop on it.
#[test]
fn x_delta_azdrive_keys_7_a_planted_drop_key_file_stops_no_rotation() {
    let s = set_up();
    let bucket: Arc<dyn Drive> = s.bucket.clone();
    s.bucket
        .set(DROP_KEY_FILE, b"junk, not a drop key".to_vec());
    let rotated = rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()).unwrap();
    assert!(
        rotated.drop_key.is_none(),
        "no drop key to carry over: incoming mail is off until it is turned on again"
    );
    let new_key = device::load_drive_key(&s.keyring, DRIVE).unwrap().unwrap();
    assert_eq!(
        recover(
            bucket.as_ref(),
            &MemoryKeyring::new(),
            DRIVE,
            &rotated.recovery_code
        )
        .unwrap()
        .id(),
        new_key.id()
    );
}

/// X-DELTA-AZDRIVE-KEYS-7: from the recovery wrap on nothing returns an error - the code is the
/// drive's and has to reach the user. A journal that cannot be deleted, a keyring entry that is
/// refused: said in `left_over`, the keys are changed.
#[test]
fn x_delta_azdrive_keys_7_what_cannot_be_cleaned_up_after_the_new_wrap_is_said_not_raised() {
    let s = set_up();
    let faulty = Arc::new(Faulty::new(s.bucket.clone()));
    let bucket: Arc<dyn Drive> = faulty.clone();
    faulty.fail_delete(JOURNAL_FILE, 1);
    let rotated = rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap())
        .expect("the keys are changed: the code reaches the user");
    assert!(
        rotated
            .left_over
            .iter()
            .any(|left| left.contains(JOURNAL_FILE)),
        "{:?}",
        rotated.left_over
    );
    let (previous, next) = rotation_key_entries(DRIVE);
    assert_eq!(s.keyring.get(&previous).unwrap(), None);
    assert_eq!(s.keyring.get(&next).unwrap(), None);
    // The journal that stayed is inert: nothing of this device vouches for it, so it skips no
    // lockdown of a later "I was hacked".
    assert!(s.bucket.object(JOURNAL_FILE).is_some());
    assert!(resumable(bucket.as_ref(), &s.keyring, DRIVE)
        .unwrap()
        .is_none());
    assert!(lockdown_needed(bucket.as_ref(), &s.keyring, DRIVE).unwrap());
    let new_key = device::load_drive_key(&s.keyring, DRIVE).unwrap().unwrap();
    assert_eq!(
        recover(
            bucket.as_ref(),
            &MemoryKeyring::new(),
            DRIVE,
            &rotated.recovery_code
        )
        .unwrap()
        .id(),
        new_key.id()
    );
}

/// A keyring that refuses to delete one entry (the user said "No" to the prompt).
struct RefusesToDelete {
    inner: MemoryKeyring,
    entry: String,
}

impl KeyringStore for RefusesToDelete {
    fn get(&self, key: &str) -> Result<Option<String>, KeyringError> {
        self.inner.get(key)
    }
    fn set(&self, key: &str, secret: &str) -> Result<(), KeyringError> {
        self.inner.set(key, secret)
    }
    fn delete(&self, key: &str) -> Result<(), KeyringError> {
        if key == self.entry {
            return Err(KeyringError::Denied);
        }
        self.inner.delete(key)
    }
}

/// X-DELTA-AZDRIVE-KEYS-7: the keyring's last deletes ran after the journal was gone and, refused,
/// ended the rotation with an error after the wrap was replaced. Now the code comes back, with the
/// entry named in `left_over`.
#[test]
fn x_delta_azdrive_keys_7_a_keyring_that_refuses_the_last_delete_still_returns_the_code() {
    let s = set_up();
    let bucket: Arc<dyn Drive> = s.bucket.clone();
    let (previous, _) = rotation_key_entries(DRIVE);
    let keyring = RefusesToDelete {
        inner: MemoryKeyring::new(),
        entry: previous.clone(),
    };
    // This device's keys: the drive key and its member key, as set_up made them in `s.keyring`.
    for entry in [
        device::drive_key_entry(DRIVE),
        device::member_key_entry(DRIVE),
    ] {
        keyring
            .inner
            .set(&entry, &s.keyring.get(&entry).unwrap().unwrap())
            .unwrap();
    }
    let rotated = rotate(bucket.clone(), &keyring, DRIVE, &s.provider, cheap())
        .expect("the code is the drive's: it comes back");
    assert!(
        rotated
            .left_over
            .iter()
            .any(|left| left.contains(&previous)),
        "{:?}",
        rotated.left_over
    );
    assert!(
        pending(bucket.as_ref()).unwrap().is_none(),
        "the journal is gone"
    );
}

/// X-DELTA-KEYS3-4: a rotation that ended with its journal and the new key's own entry left behind (the bucket
/// refused the journal's delete and the keyring the entry's: `left_over`), and the previous key gone - the keys
/// are DK = N = K2 and an authentic journal. The key permit said `Active` (a finished rotation), the guard said
/// `Resume` (an own journal, whatever the keyring holds) and never read the index, so `rotation::ask` refused
/// every "I was hacked" with "whether the drive's index opens ... was not looked at" - for ever, and a resume
/// would fail at the key it replaces. The guard and the judge agree: a new rotation starts, the index is read.
#[test]
fn x_delta_keys3_4_the_leftover_journal_of_a_finished_rotation_does_not_stop_the_next_one() {
    use crate::rotation::{ask, start, Start};
    let s = set_up();
    let faulty = Arc::new(Faulty::new(s.bucket.clone()));
    let bucket: Arc<dyn Drive> = faulty.clone();
    let (_, next) = rotation_key_entries(DRIVE);
    let keyring = RefusesToDelete {
        inner: MemoryKeyring::new(),
        entry: next.clone(),
    };
    for entry in [
        device::drive_key_entry(DRIVE),
        device::member_key_entry(DRIVE),
    ] {
        keyring
            .inner
            .set(&entry, &s.keyring.get(&entry).unwrap().unwrap())
            .unwrap();
    }
    faulty.fail_delete(JOURNAL_FILE, 1);
    let rotated = rotate(bucket.clone(), &keyring, DRIVE, &s.provider, cheap())
        .expect("the keys are changed: what stays is said");
    assert!(
        rotated
            .left_over
            .iter()
            .any(|left| left.contains(JOURNAL_FILE))
            && rotated.left_over.iter().any(|left| left.contains(&next)),
        "{:?}",
        rotated.left_over
    );
    let k2 = device::load_drive_key(&keyring, DRIVE).unwrap().unwrap();
    assert_eq!(
        device::load_key_at(&keyring, &next).unwrap(),
        Some(k2.clone())
    );
    assert!(
        !unfinished(&keyring, DRIVE).unwrap(),
        "the keys say a finished rotation"
    );
    assert!(
        resumable(bucket.as_ref(), &keyring, DRIVE)
            .unwrap()
            .is_some(),
        "premise: the journal that stayed is this device's own (it verifies under K2)"
    );

    // The guard answers what the judge does: a new rotation, not a resume of a finished one.
    assert!(
        matches!(
            start(bucket.as_ref(), &keyring, DRIVE),
            Ok(Start::Fresh { .. })
        ),
        "a finished rotation's journal is no rotation to resume"
    );
    let rotation = ask(bucket.as_ref(), &keyring, DRIVE).expect(
        "the key change is asked for, not refused in a sentence about a fact nobody looked at",
    );
    assert!(
        rotation.lockdown().is_some(),
        "a new rotation runs the account's lockdown"
    );
    let again = rotate(bucket.clone(), &keyring, DRIVE, &s.provider, cheap()).unwrap();
    assert_ne!(
        again.drive_key,
        k2.id(),
        "a second new key: the drive's keys changed again"
    );
}

/// X-DELTA-AZDRIVE-KEYS-30: the previous drop key was one deep; a second rotation before the
/// Worker had the key of the first put the first key out, and the mail sealed to it could never
/// be opened. Now the keys before the current one are a list, newest first.
#[test]
fn x_delta_azdrive_keys_30_the_mail_sealed_to_the_first_drop_key_still_opens_after_a_second_rotation(
) {
    let s = set_up();
    let bucket: Arc<dyn Drive> = s.bucket.clone();
    let k1 = device::load_drive_key(&s.keyring, DRIVE).unwrap().unwrap();
    let first = load_drop_key(bucket.as_ref(), &k1, DRIVE)
        .unwrap()
        .unwrap()
        .public();

    rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()).unwrap();
    let k2 = device::load_drive_key(&s.keyring, DRIVE).unwrap().unwrap();
    let second = load_drop_key(bucket.as_ref(), &k2, DRIVE)
        .unwrap()
        .unwrap()
        .public();
    assert_ne!(first, second);

    rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()).unwrap();
    let k3 = device::load_drive_key(&s.keyring, DRIVE).unwrap().unwrap();
    let third = load_drop_key(bucket.as_ref(), &k3, DRIVE)
        .unwrap()
        .unwrap()
        .public();
    assert!(third != first && third != second);
    let kept: Vec<_> = load_previous_drop_keys(bucket.as_ref(), &k3, DRIVE)
        .unwrap()
        .iter()
        .map(|key| key.public())
        .collect();
    assert_eq!(
        kept,
        vec![second, first],
        "newest first; the first key is not lost"
    );
    assert_eq!(
        load_previous_drop_key(bucket.as_ref(), &k3, DRIVE)
            .unwrap()
            .map(|key| key.public()),
        Some(second),
        "the single-key reader still gives the newest"
    );
}

/// A second process on the same keyring - `azcloud mail-drop` - and a device that holds only the new key:
/// what asks to turn incoming mail on while a rotation stands between its Members step and its drop key
/// step. Each must find the drop key file as the rotation left it, and leave it so (X-DELTA-KEYS3-2).
fn turning_mail_on_is_refused_and_leaves_the_file(
    s: &Setup,
    bucket: &Arc<dyn Drive>,
    new_key: &DriveKey,
) {
    let file = s.bucket.object(DROP_KEY_FILE).expect("incoming mail is on");
    // On the keyring of the rotation (drive key = new key, previous key kept): the key permit refuses
    // a key change that is not over, before the bucket is read for it.
    let refused = device::drop_key_permit(bucket.as_ref(), &s.keyring, DRIVE, new_key);
    assert!(
        matches!(&refused, Err(DriveError::Denied { message }) if message.contains("not finished")),
        "{refused:?}"
    );
    // On a device that holds the new key and knows nothing of the key change: its key permit is asked
    // on the file, which the K1 secret still seals - not a key this device reads, not one it writes over.
    let elsewhere = MemoryKeyring::new();
    device::store_drive_key(&elsewhere, DRIVE, new_key).unwrap();
    let refused = device::drop_key_permit(bucket.as_ref(), &elsewhere, DRIVE, new_key);
    assert!(
        matches!(refused, Err(DriveError::KeyRefused { .. })),
        "{refused:?}"
    );
    let read = load_drop_key(bucket.as_ref(), new_key, DRIVE);
    assert!(
        matches!(read, Err(DriveError::KeyRefused { .. })),
        "{read:?}"
    );
    assert_eq!(
        s.bucket.object(DROP_KEY_FILE).unwrap(),
        file,
        "_drop.key is byte-identical"
    );
}

/// X-DELTA-KEYS3-2, the window of the review's second scenario: DK = K2 (the Members step) and `_drop.key`
/// still sealed under K1 until the drop key step. A second `enable_drop` on the same keyring read the K1 file
/// as "no key", wrote a new key sealed under K2 over it, and the step then answered "done already" without
/// ever having saved the secret the Worker's mail was sealed to. Cell R13a: the step stops before its first
/// write. The window is closed to `enable_drop`, and the resumed step saves the previous key.
#[test]
fn x_delta_keys3_2_r13a_a_second_enable_in_the_window_cannot_overwrite_and_the_resume_saves_the_previous_key(
) {
    let s = set_up();
    let faulty = Arc::new(Faulty::new(s.bucket.clone()));
    let bucket: Arc<dyn Drive> = faulty.clone();
    let k1 = s.old_key.clone();
    let before = load_drop_key(bucket.as_ref(), &k1, DRIVE)
        .unwrap()
        .unwrap()
        .public();
    faulty.fail_write(PREVIOUS_DROP_KEY_FILE, Fail::Refused, 1);
    assert!(rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()).is_err());
    let k2 = device::load_drive_key(&s.keyring, DRIVE).unwrap().unwrap();
    assert_ne!(
        k2.id(),
        k1.id(),
        "premise: the Members step made the new key the drive key"
    );
    assert!(
        s.bucket.object(PREVIOUS_DROP_KEY_FILE).is_none(),
        "premise: no write of the step landed"
    );

    turning_mail_on_is_refused_and_leaves_the_file(&s, &bucket, &k2);

    let rotated = rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap())
        .expect("the resume finishes the drop key step");
    let current = load_drop_key(bucket.as_ref(), &k2, DRIVE)
        .unwrap()
        .unwrap()
        .public();
    assert_eq!(rotated.drop_key, Some(current));
    assert_ne!(current, before);
    assert_eq!(
        load_previous_drop_key(bucket.as_ref(), &k2, DRIVE)
            .unwrap()
            .map(|key| key.public()),
        Some(before),
        "the secret the Worker's mail was sealed to is saved, sealed under the new key"
    );
}

/// X-DELTA-KEYS3-2, cell R13b: the previous key is written, the step stops before it replaces `_drop.key`:
/// the file is still K1's and the previous file already holds its secret. The same refusal, and the resume
/// writes the same previous key again and replaces the file.
#[test]
fn x_delta_keys3_2_r13b_a_stop_after_the_previous_key_leaves_the_window_closed_and_resumes() {
    let s = set_up();
    let faulty = Arc::new(Faulty::new(s.bucket.clone()));
    let bucket: Arc<dyn Drive> = faulty.clone();
    let k1 = s.old_key.clone();
    let before = load_drop_key(bucket.as_ref(), &k1, DRIVE)
        .unwrap()
        .unwrap()
        .public();
    faulty.fail_write(DROP_KEY_FILE, Fail::Refused, 1);
    assert!(rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()).is_err());
    let k2 = device::load_drive_key(&s.keyring, DRIVE).unwrap().unwrap();
    assert!(
        s.bucket.object(PREVIOUS_DROP_KEY_FILE).is_some(),
        "premise: the previous key was written"
    );

    turning_mail_on_is_refused_and_leaves_the_file(&s, &bucket, &k2);

    let rotated = rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()).unwrap();
    assert_ne!(rotated.drop_key.unwrap(), before);
    assert_eq!(
        load_previous_drop_key(bucket.as_ref(), &k2, DRIVE)
            .unwrap()
            .map(|key| key.public()),
        Some(before)
    );
}

/// X-DELTA-AZDRIVE-KEYS-2: a key change that stopped is "unfinished" for as long as the keyring
/// holds a new key that is not the drive key: the question a removal asks the keyring before it
/// deletes keys.
#[test]
fn x_delta_azdrive_keys_2_a_rotation_that_stopped_is_unfinished_until_it_ends() {
    let s = set_up();
    let bucket: Arc<dyn Drive> = s.bucket.clone();
    assert!(
        !unfinished(&s.keyring, DRIVE).unwrap(),
        "no rotation under way"
    );
    s.provider.rekey_failures.store(1, Ordering::SeqCst);
    assert!(rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()).is_err());
    assert!(
        unfinished(&s.keyring, DRIVE).unwrap(),
        "K2 is kept and is not the drive key"
    );
    rotate(bucket.clone(), &s.keyring, DRIVE, &s.provider, cheap()).unwrap();
    assert!(
        !unfinished(&s.keyring, DRIVE).unwrap(),
        "finished: the entries are gone"
    );
    // Another device has no new key: nothing of its own is unfinished.
    assert!(!unfinished(&s.other_device, DRIVE).unwrap());
}
