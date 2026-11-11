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
    encrypted::{open_encrypted, IndexEntry, IndexProvider, NameIndex},
    keyring::MemoryKeyring,
    meta::{
        keys, wal::narrow_writable_max, IndexProfile, MemoryBucket, MetaIndexProvider, MetaRepo,
    },
    rotation::{
        begun, lockdown_needed, pending, resumable, rewrap_batch, rotate, Phase, JOURNAL_FILE,
    },
    Drive, DriveError, ListRequest,
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
    let provider = MetaIndexProvider::new(IndexProfile::in_memory("Laptop"));
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
    let provider = MetaIndexProvider::new(IndexProfile::in_memory("Laptop"));
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

// ==== X-DELTA-KEYS2-2: the re-wrap of a drive over the writer's cap goes in batches ====

/// The writer's cap of these tests (bytes): a drive of 600 files weighs about 270 KB as one change
/// (a pointer is 370 bytes, a folder's tree 60 a file), so its one-commit re-wrap is far over it,
/// and a batch ([`rewrap_batch`]: 45 entries at this cap) is a quarter of it. It stands for the
/// 256 MiB of a drive of some 600,000 files.
const NARROW_CAP: u64 = 96 << 10;

/// Every entry of `index` that holds a file, with its path.
fn files_of(index: &dyn NameIndex) -> Vec<(String, IndexEntry)> {
    let mut found = Vec::new();
    let mut request = ListRequest::recursive("");
    loop {
        let page = index.list(&request).unwrap();
        found.extend(page.entries.into_iter().filter(|(_, e)| e.object.is_some()));
        match page.next {
            Some(token) => request = request.with_continuation(token),
            None => return found,
        }
    }
}

/// How many files of the drive's index (opened with `key`) have their file key wrapped by `wrapper`.
fn wrapped_by(w: &World, key: &DriveKey, wrapper: &DriveKey) -> (usize, usize) {
    let index = w.provider.open_index(DRIVE, w.plain.clone(), key).unwrap();
    let files = files_of(index.as_ref());
    let by = files
        .iter()
        .filter(|(_, e)| {
            e.object
                .as_ref()
                .is_some_and(|o| o.wrapped_key.key_id == wrapper.id())
        })
        .count();
    (by, files.len())
}

impl World {
    /// `folders` folders of `per` files each, on top of the two files of the world: every file is
    /// a put through the encrypted drive (a change of the index of its own).
    fn fill(&self, folders: usize, per: usize) {
        let drive = open_encrypted(self.plain.clone(), &self.keyring, DRIVE, &self.provider)
            .expect("the drive opens");
        for f in 0..folders {
            for n in 0..per {
                drive
                    .put(
                        &format!("f{f:02}/file{n}.txt"),
                        format!("file {f} {n}").as_bytes(),
                    )
                    .unwrap();
            }
        }
    }

    /// What the one-commit re-wrap of the drive's files would weigh at the least: the pointers.
    fn pointer_bytes(&self, key: &DriveKey) -> usize {
        let index = self
            .provider
            .open_index(DRIVE, self.plain.clone(), key)
            .unwrap();
        files_of(index.as_ref())
            .iter()
            .map(|(_, e)| crate::meta::pointer::encode(e).len())
            .sum()
    }

    fn all_read(&self, folders: usize, per: usize) {
        let drive = open_encrypted(self.plain.clone(), &self.keyring, DRIVE, &self.provider)
            .expect("the drive opens");
        assert_eq!(drive.get("notes/plan.txt").unwrap(), b"the plan");
        for f in (0..folders).step_by(7) {
            for n in (0..per).step_by(3) {
                assert_eq!(
                    drive.get(&format!("f{f:02}/file{n}.txt")).unwrap(),
                    format!("file {f} {n}").as_bytes()
                );
            }
        }
    }
}

/// X-DELTA-KEYS2-2 (the batch is sized by the writer's cap, never by the drive): one batch weighs
/// a quarter of the smaller of the cap and a compaction's biggest pack at the most, however many
/// files there are; never none.
#[test]
fn x_delta_keys2_2_a_batch_of_the_rewrap_is_sized_by_the_writers_cap() {
    use crate::meta::wal::{COMPACT_PACK_MAX, ENTRY_PACK_BYTES};
    let shipped = rewrap_batch();
    assert!(
        shipped > 1000,
        "a batch of the shipped cap is thousands of files: {shipped}"
    );
    assert!(
        shipped as u64 * ENTRY_PACK_BYTES <= COMPACT_PACK_MAX / 4,
        "a batch's pack is a quarter of a compaction's biggest at the most"
    );
    {
        let _cap = narrow_writable_max(NARROW_CAP);
        assert_eq!(rewrap_batch(), 45, "96 KiB / 4 / 540 bytes");
    }
    {
        let _cap = narrow_writable_max(1);
        assert_eq!(rewrap_batch(), 1, "never none");
    }
    assert_eq!(rewrap_batch(), shipped, "the guard gave the cap back");
}

/// X-DELTA-KEYS2-2: a rotation of a drive whose one-commit re-wrap is over the writer's cap. The
/// old code applied every entry in ONE index change: `put_sealed_pack` refused that pack AFTER the
/// index was sealed under K2 (`rotate` returned "the change names too many files at once") and
/// every retry stopped there. In batches it goes through, and no pack is over the cap.
#[test]
fn x_delta_keys2_2_a_rotation_of_a_drive_over_the_writers_cap_goes_through_in_batches() {
    let w = world();
    w.fill(60, 10);
    let pointers = w.pointer_bytes(&w.old_key);
    assert!(
        pointers as u64 > NARROW_CAP * 2,
        "premise: the pointers alone ({pointers} bytes) are over the cap, so one change of every file is"
    );

    let rotated = {
        let _cap = narrow_writable_max(NARROW_CAP);
        rotate(w.plain.clone(), &w.keyring, DRIVE, &w.provider, cheap())
            .expect("batches of the cap's size go through")
    };
    assert_eq!(rotated.rewrapped, 602, "every file of the drive");
    let new_key = load_drive_key(&w.keyring, DRIVE).unwrap().unwrap();
    assert_eq!(new_key.id(), rotated.drive_key);
    assert!(repository_sealed_by(&w.mem, &new_key));
    assert_eq!(
        wrapped_by(&w, &new_key, &new_key),
        (602, 602),
        "nothing is left on K1"
    );
    let repo = MetaRepo::open(w.mem.clone(), new_key.clone(), "checker", "Checker").unwrap();
    let biggest = repo
        .store()
        .state()
        .packs
        .iter()
        .map(|pack| pack.pack_size.max(pack.idx_size))
        .max()
        .unwrap();
    assert!(biggest <= NARROW_CAP, "no pack is over the cap: {biggest}");
    w.all_read(60, 10);
}

/// X-DELTA-KEYS2-2: the network goes between two batches. The index is sealed under K2 and half of
/// its entries are wrapped with K2 (it reads through the key chain); the same call goes on and
/// wraps only what is left.
#[test]
fn x_delta_keys2_2_a_rotation_stopped_between_two_batches_goes_on_with_the_rest() {
    let w = world();
    w.fill(60, 10);
    let k2 = w.stopped_after_the_switch();
    let _cap = narrow_writable_max(NARROW_CAP);

    // The third batch's pack is refused: two batches (90 files) landed.
    w.faulty.fail_nth_write_ending(".pack", 3, Fail::Refused);
    let stopped = rotate(w.plain.clone(), &w.keyring, DRIVE, &w.provider, cheap());
    assert!(stopped.is_err(), "the third batch failed: {stopped:?}");
    let (done, total) = wrapped_by(&w, &k2, &k2);
    assert_eq!(total, 602);
    assert_eq!(done, 90, "the two batches that landed stay");
    assert_eq!(
        pending(w.plain.as_ref()).unwrap().unwrap().phase,
        Phase::Rekeyed
    );
    assert!(
        !lockdown_needed(w.plain.as_ref(), &w.keyring, DRIVE).unwrap(),
        "a rotation of this device that stopped has had its lockdown"
    );

    let rotated = rotate(w.plain.clone(), &w.keyring, DRIVE, &w.provider, cheap())
        .expect("goes on where it stopped");
    assert_eq!(rotated.rewrapped, 602 - done, "only what was left");
    assert_eq!(wrapped_by(&w, &k2, &k2), (602, 602));
    assert_eq!(load_drive_key(&w.keyring, DRIVE).unwrap(), Some(k2.clone()));
    assert!(
        pending(w.plain.as_ref()).unwrap().is_none(),
        "the journal is gone"
    );
    w.all_read(60, 10);
}

/// X-DELTA-KEYS2-2: the state the old code left a drive in at the cap - journal `Rekeyed`, the
/// index sealed under K2, `drive-key` = `drive-key-previous` = K1, `drive-key-next` = K2, not one
/// file re-wrapped - and the drive opened on no device. A build that batches resumes it.
#[test]
fn x_delta_keys2_2_a_drive_stuck_at_the_rewrap_is_resumed_with_batches() {
    let w = world();
    w.fill(60, 10);
    let k2 = w.stopped_after_the_switch();
    let _cap = narrow_writable_max(NARROW_CAP);
    w.faulty.fail_nth_write_ending(".pack", 1, Fail::Refused);
    assert!(rotate(w.plain.clone(), &w.keyring, DRIVE, &w.provider, cheap()).is_err());

    // The stuck state, as the review wrote it down.
    let (previous, next) = rotation_key_entries(DRIVE);
    assert_eq!(
        pending(w.plain.as_ref()).unwrap().unwrap().phase,
        Phase::Rekeyed
    );
    assert!(repository_sealed_by(&w.mem, &k2), "IDX(K2)");
    assert_eq!(
        load_drive_key(&w.keyring, DRIVE).unwrap(),
        Some(w.old_key.clone()),
        "DK = K1"
    );
    assert_eq!(
        load_key_at(&w.keyring, &previous).unwrap(),
        Some(w.old_key.clone()),
        "P = K1"
    );
    assert_eq!(
        load_key_at(&w.keyring, &next).unwrap(),
        Some(k2.clone()),
        "N = K2"
    );
    assert_eq!(wrapped_by(&w, &k2, &k2).0, 0, "no file re-wrapped");
    assert!(
        MetaRepo::open(w.mem.clone(), w.old_key.clone(), "checker", "Checker").is_err(),
        "the index opens with the key this device holds for the drive: on no device"
    );

    let rotated = rotate(w.plain.clone(), &w.keyring, DRIVE, &w.provider, cheap())
        .expect("resumes with the batched re-wrap");
    assert_eq!(rotated.rewrapped, 602);
    assert_eq!(load_drive_key(&w.keyring, DRIVE).unwrap(), Some(k2));
    w.all_read(60, 10);
}

/// X-DELTA-KEYS2-2 (the pre-flight): a drive whose index holds a pack the writer would refuse to
/// seal again (written before the writer had a cap) is refused BEFORE the lockdown, in words, with
/// nothing changed. The old code said yes to the lockdown and stopped at R4, after the other
/// devices were locked out of a drive whose key was not replaced.
#[test]
fn x_delta_keys2_2_a_pack_the_writer_refuses_is_refused_before_the_lockdown() {
    let w = world();
    assert!(
        lockdown_needed(w.plain.as_ref(), &w.keyring, DRIVE).unwrap(),
        "control: at the shipped cap the drive rotates"
    );
    let cap = narrow_writable_max(64);
    let refused = lockdown_needed(w.plain.as_ref(), &w.keyring, DRIVE);
    let Err(DriveError::Unsupported(why)) = &refused else {
        panic!("refused before the lockdown: {refused:?}");
    };
    assert!(why.contains("Nothing was changed"), "{why}");
    assert!(
        why.contains("more than an index object may be (64 bytes)"),
        "{why}"
    );
    let again = rotate(w.plain.clone(), &w.keyring, DRIVE, &w.provider, cheap());
    assert!(
        matches!(again, Err(DriveError::Unsupported(_))),
        "{again:?}"
    );

    let (previous, next) = rotation_key_entries(DRIVE);
    assert!(
        load_key_at(&w.keyring, &next).unwrap().is_none(),
        "no K2 was made"
    );
    assert!(load_key_at(&w.keyring, &previous).unwrap().is_none());
    assert!(pending(w.plain.as_ref()).unwrap().is_none(), "no journal");
    assert!(
        repository_sealed_by(&w.mem, &w.old_key),
        "the index is as it was"
    );
    drop(cap);
    w.files_read();
}

/// X-DELTA-KEYS2-1: whether a rotation of this device got as far as its keyring (the previous or the
/// next key is there). AzDrive asks it of a rotation that failed: before it, nothing of the drive
/// changed and the code typed back right stays checked; from it on the wrap may have changed.
#[test]
fn x_delta_keys2_1_a_rotation_has_begun_once_its_keys_are_in_the_keyring() {
    let w = world();
    assert!(!begun(&w.keyring, DRIVE).unwrap(), "no rotation yet");
    // A refusal before anything was written: the key permit says no (a keyring with no key).
    let empty = MemoryKeyring::new();
    assert!(rotate(w.plain.clone(), &empty, DRIVE, &w.provider, cheap()).is_err());
    assert!(
        !begun(&empty, DRIVE).unwrap(),
        "refused before its first write"
    );

    w.stopped_after_the_switch();
    assert!(
        begun(&w.keyring, DRIVE).unwrap(),
        "the keys of the try are there"
    );
    rotate(w.plain.clone(), &w.keyring, DRIVE, &w.provider, cheap()).unwrap();
    assert!(
        !begun(&w.keyring, DRIVE).unwrap(),
        "a finished rotation took them away"
    );
}
