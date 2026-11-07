//! What a drive leaves on a device (ARCH28 RF53 / AR5b-4): the names of its keyring entries, the
//! removal that tries every store and takes the drives-file entry last, the evidence a key needs
//! before it is erased or a wrap published, and the plain file that takes no key.

use std::sync::atomic::{AtomicBool, Ordering};

use super::TempDir;
use crate::{
    config::{DriveEntry, DriveLocation, DrivesFile},
    footprint::{
        contact_key_entry, forget_drive, DrivesListing, Footprint, KeptBecause, KeyEntry,
        KeyringFootprint, Paths, RemovalProof, StoreOutcome,
    },
    keyring::{KeyringError, KeyringStore, MemoryKeyring},
};

const DRIVE: &str = "d_k3f9";

/// Every kind of entry a drive has, filled for `drive`.
fn filled(keyring: &MemoryKeyring, drive: &str) {
    for entry in KeyEntry::ALL {
        keyring.set(&entry.name(drive), "a secret text").unwrap();
    }
}

fn local_entry(id: &str) -> DriveEntry {
    DriveEntry {
        id: id.to_string(),
        name: "A drive".to_string(),
        location: DriveLocation::Local {
            root: "/tmp/a-drive".to_string(),
        },
    }
}

/// A keyring that refuses to delete one entry until it is told it may.
struct Stubborn {
    inner: MemoryKeyring,
    refuses: String,
    allowed: AtomicBool,
}

impl Stubborn {
    fn refusing(entry: String) -> Stubborn {
        Stubborn {
            inner: MemoryKeyring::new(),
            refuses: entry,
            allowed: AtomicBool::new(false),
        }
    }
}

impl KeyringStore for Stubborn {
    fn get(&self, key: &str) -> Result<Option<String>, KeyringError> {
        self.inner.get(key)
    }
    fn set(&self, key: &str, secret: &str) -> Result<(), KeyringError> {
        self.inner.set(key, secret)
    }
    fn delete(&self, key: &str) -> Result<(), KeyringError> {
        if key == self.refuses && !self.allowed.load(Ordering::SeqCst) {
            return Err(KeyringError::Denied);
        }
        self.inner.delete(key)
    }
}

/// LANE-AZ-2 (K6, X-AR5b-4): the names of a drive's keyring entries are written in ONE place and
/// pinned here, because users' keys are in them. Renaming one strands every key already kept.
#[test]
fn lane_az_2_the_keyring_entry_names_of_a_drive_are_pinned() {
    let names: Vec<String> = KeyEntry::ALL.iter().map(|e| e.name("d_1")).collect();
    assert_eq!(
        names,
        [
            "azul-storage/s3/d_1",
            "azul-storage/azlin/d_1",
            "azul-storage/drive-key/d_1",
            "azul-storage/drive-key-previous/d_1",
            "azul-storage/drive-key-next/d_1",
            "azul-storage/drive-key-planned/d_1",
            "azul-storage/member-key/d_1",
            "azul-storage/recovery-request/d_1",
        ]
    );
    assert_eq!(contact_key_entry("ab12"), "azul-storage/contact-key/ab12");
    // The names the rest of the crate and the apps use are these.
    assert_eq!(crate::config::keyring_key("d_1"), names[0]);
    // Every kind is listed once, and a name reads back to its kind.
    for entry in KeyEntry::ALL {
        assert_eq!(
            KeyEntry::ALL.iter().filter(|e| **e == entry).count(),
            1,
            "{entry:?}"
        );
        assert_eq!(KeyEntry::of_name(&entry.name("d_1"), "d_1"), Some(entry));
        assert_eq!(KeyEntry::of_name(&entry.name("d_1"), "d_2"), None);
    }
    // Only the session and the drive token are credentials; the rest is key material.
    let credentials: Vec<KeyEntry> = KeyEntry::ALL
        .into_iter()
        .filter(|e| !e.is_key_material())
        .collect();
    assert_eq!(credentials, [KeyEntry::Session, KeyEntry::AzlinToken]);
}

/// LANE-AZ-2: a removal without the evidence that the recovery code was checked takes the
/// credentials and leaves the keys, and says so; with it, every entry of the drive goes and no
/// entry of another drive does.
#[test]
fn lane_az_2_a_removal_takes_the_credentials_and_keeps_the_keys_without_a_proof() {
    let keyring = MemoryKeyring::new();
    filled(&keyring, DRIVE);
    filled(&keyring, "d_other");
    let store = KeyringFootprint::new(&keyring);

    let done = store.forget(DRIVE, None).unwrap();
    assert_eq!(
        done.removed.len(),
        2,
        "the session and the drive token: {done:?}"
    );
    assert_eq!(done.kept.len(), 6, "the keys stay: {done:?}");
    assert!(done.kept.iter().all(|k| k.because == KeptBecause::NoProof));
    assert_eq!(store.residue(DRIVE).unwrap().len(), 6);

    let proof = RemovalProof::from_checked_code(DRIVE);
    let done = store.forget(DRIVE, Some(&proof)).unwrap();
    assert!(done.kept.is_empty(), "{done:?}");
    assert!(store.residue(DRIVE).unwrap().is_empty());
    assert_eq!(
        store.residue("d_other").unwrap().len(),
        8,
        "another drive's entries stay"
    );
    // Nothing there: nothing to refuse.
    store.forget(DRIVE, Some(&proof)).unwrap();
}

/// A keyring that cannot be looked into: every read of an entry is refused.
struct Unreadable(MemoryKeyring);

impl KeyringStore for Unreadable {
    fn get(&self, _key: &str) -> Result<Option<String>, KeyringError> {
        Err(KeyringError::Denied)
    }
    fn set(&self, key: &str, secret: &str) -> Result<(), KeyringError> {
        self.0.set(key, secret)
    }
    fn delete(&self, key: &str) -> Result<(), KeyringError> {
        self.0.delete(key)
    }
}

/// LANE-AZ-2: "the keys stay" is said of keys that are there. A drive of a bucket has a session
/// and no key, and its removal keeps nothing for want of a proof (the old answer listed all six
/// key entries as kept whether or not they existed, so every removal of a drive without a checked
/// code said that its keys stay).
#[test]
fn lane_az_2_a_drive_that_never_had_keys_keeps_none_for_want_of_a_proof() {
    let keyring = MemoryKeyring::new();
    keyring
        .set(&KeyEntry::Session.name(DRIVE), "a session")
        .unwrap();
    let store = KeyringFootprint::new(&keyring);
    let done = store.forget(DRIVE, None).unwrap();
    assert!(done.kept.is_empty(), "{done:?}");
    assert!(store.residue(DRIVE).unwrap().is_empty(), "the session went");

    // One key entry is there: that one is kept, and only that one.
    keyring
        .set(&KeyEntry::DriveKey.name(DRIVE), "a key")
        .unwrap();
    let done = store.forget(DRIVE, None).unwrap();
    assert_eq!(done.kept.len(), 1, "{done:?}");
    assert_eq!(done.kept[0].what, KeyEntry::DriveKey.name(DRIVE));
}

/// LANE-AZ-2: a key entry that cannot be looked at is neither kept nor gone: the store fails, so
/// the drive stays on the list for another try (an unreadable entry is not an absent one).
#[test]
fn lane_az_2_a_key_entry_that_cannot_be_read_fails_the_store_and_the_drive_stays_listed() {
    let keyring = Unreadable(MemoryKeyring::new());
    let store = KeyringFootprint::new(&keyring);
    let failed = store.forget(DRIVE, None).unwrap_err();
    assert!(
        failed.to_string().contains(&KeyEntry::DriveKey.name(DRIVE)),
        "{failed}"
    );

    let dir = TempDir::new("footprint-unreadable");
    let drives = dir.path().join("drives.json");
    DrivesFile::update(&drives, |file| file.add(local_entry(DRIVE))).unwrap();
    let listing = DrivesListing::new(&drives);
    let report = forget_drive(&[&listing, &store], DRIVE, None);
    assert!(!report.complete());
    assert!(report
        .stores
        .iter()
        .any(|s| s.store == "keyring" && matches!(s.outcome, StoreOutcome::Failed(_))));
    assert_eq!(
        report.stores.last().map(|s| &s.outcome),
        Some(&StoreOutcome::Skipped)
    );
    assert!(DrivesFile::load(&drives).unwrap().get(DRIVE).is_some());
}

/// LANE-AZ-2: a proof is for one drive. One for another drive is none.
#[test]
fn lane_az_2_a_proof_for_another_drive_is_no_proof() {
    let keyring = MemoryKeyring::new();
    filled(&keyring, DRIVE);
    let store = KeyringFootprint::new(&keyring);
    let other = RemovalProof::from_checked_code("d_other");
    let report = forget_drive(&[&store], DRIVE, Some(&other));
    assert!(report.complete(), "{report:?}");
    assert_eq!(report.kept().len(), 6, "the keys stay: {report:?}");
    assert_eq!(store.residue(DRIVE).unwrap().len(), 6);
}

/// LANE-AZ-2 / K6: the removal tries EVERY store whatever one of them did, and takes the drive off
/// the list LAST and only when no store failed - given first, the listing still goes last. A delete
/// the keyring refused leaves the drive on the list, its entries findable by its id, and the same
/// call finishes the job once the keyring lets go.
#[test]
fn lane_az_2_a_refused_delete_leaves_the_drive_on_the_list_and_every_other_store_is_tried() {
    let dir = TempDir::new("footprint");
    let keyring = Stubborn::refusing(KeyEntry::DriveKey.name(DRIVE));
    for entry in KeyEntry::ALL {
        keyring.set(&entry.name(DRIVE), "a secret text").unwrap();
    }
    // The cache holds the plain text of the drive's files.
    let cache = dir.0.join("cache");
    std::fs::create_dir_all(cache.join("search")).unwrap();
    std::fs::write(
        cache.join("search").join("index"),
        "the plain text of a file",
    )
    .unwrap();
    let drives = dir.0.join("drives.json");
    DrivesFile::update(&drives, |file| file.add(local_entry(DRIVE))).unwrap();

    let keys = KeyringFootprint::new(&keyring);
    let caches = Paths::new("cache", {
        let cache = cache.clone();
        move |_| vec![cache.clone()]
    });
    let listing = DrivesListing::new(&drives);
    let proof = RemovalProof::from_checked_code(DRIVE);

    let report = forget_drive(&[&listing, &keys, &caches], DRIVE, Some(&proof));
    assert!(!report.complete(), "{report:?}");
    let failures = report.failures();
    assert_eq!(failures.len(), 1, "{report:?}");
    assert_eq!(failures[0].0, "keyring");
    assert!(
        failures[0]
            .1
            .to_string()
            .contains(&KeyEntry::DriveKey.name(DRIVE)),
        "the failure names the entry: {}",
        failures[0].1
    );
    assert!(!cache.exists(), "the other stores were tried all the same");
    assert_eq!(
        report.stores.last().map(|s| &s.outcome),
        Some(&StoreOutcome::Skipped),
        "the listing is last, and left alone"
    );
    assert!(
        DrivesFile::load(&drives).unwrap().get(DRIVE).is_some(),
        "the drive stays on the list: its entries can be found and removed again"
    );
    assert!(!report.listing_removed(DrivesListing::STORE));

    // The keyring lets go: the same call finishes.
    keyring.allowed.store(true, Ordering::SeqCst);
    let report = forget_drive(&[&listing, &keys, &caches], DRIVE, Some(&proof));
    assert!(report.complete(), "{report:?}");
    assert!(report.listing_removed(DrivesListing::STORE));
    assert!(DrivesFile::load(&drives).unwrap().get(DRIVE).is_none());
    assert!(keys.residue(DRIVE).unwrap().is_empty());
    assert!(caches.residue(DRIVE).unwrap().is_empty());
    assert!(listing.residue(DRIVE).unwrap().is_empty());
}

/// LANE-AZ-2: a store of paths removes a file and a folder with everything in it, and a path that
/// is not there is no error; `residue` lists what is left, by path.
#[test]
fn lane_az_2_a_store_of_paths_removes_files_and_folders_and_reports_what_is_left() {
    let dir = TempDir::new("footprint-paths");
    let file = dir.0.join("state.json");
    let folder = dir.0.join("copies");
    std::fs::write(&file, "{}").unwrap();
    std::fs::create_dir_all(folder.join("deep")).unwrap();
    std::fs::write(folder.join("deep").join("object"), "ciphertext").unwrap();
    let missing = dir.0.join("never-there");
    let paths = Paths::new("state", {
        let places = vec![file.clone(), folder.clone(), missing];
        move |_| places.clone()
    });
    assert_eq!(paths.residue(DRIVE).unwrap().len(), 2);
    let done = paths.forget(DRIVE, None).unwrap();
    assert_eq!(done.removed.len(), 3);
    assert!(!file.exists() && !folder.exists());
    assert!(paths.residue(DRIVE).unwrap().is_empty());
}

#[cfg(feature = "encryption")]
mod keys {
    use super::*;
    use crate::{
        crypto::{
            device::{
                drive_key_entry, held, planned_key_entry, publish_recovery_wrap, setup_new_drive,
                stash, Publish,
            },
            keys::{RecoveryCode, RecoveryKdf, RecoveryWrap},
            DriveKey,
        },
        footprint::{Allow, Protection},
        tests::mem_bucket::MemBucket,
        DriveError,
    };

    fn cheap() -> RecoveryKdf {
        RecoveryKdf::with_cost(64, 1, 1).unwrap()
    }

    /// A keyring that is a plain file, and may be allowed to hold key material.
    struct PlainFile {
        inner: MemoryKeyring,
        allow: Allow,
    }

    impl KeyringStore for PlainFile {
        fn get(&self, key: &str) -> Result<Option<String>, KeyringError> {
            self.inner.get(key)
        }
        fn set(&self, key: &str, secret: &str) -> Result<(), KeyringError> {
            self.inner.set(key, secret)
        }
        fn delete(&self, key: &str) -> Result<(), KeyringError> {
            self.inner.delete(key)
        }
        fn protection(&self) -> Protection {
            Protection::PlainFile { allow: self.allow }
        }
    }

    /// A keyring that takes everything and gives nothing back.
    struct Forgetful;

    impl KeyringStore for Forgetful {
        fn get(&self, _key: &str) -> Result<Option<String>, KeyringError> {
            Ok(None)
        }
        fn set(&self, _key: &str, _secret: &str) -> Result<(), KeyringError> {
            Ok(())
        }
        fn delete(&self, _key: &str) -> Result<(), KeyringError> {
            Ok(())
        }
    }

    /// The names the key flows of `crypto::device` use are the entries' names.
    #[test]
    fn lane_az_2_the_key_flows_name_their_entries_through_the_one_enum() {
        use crate::crypto::device::{
            key_entries, member_key_entry, request_key_entry, rotation_key_entries,
        };
        assert_eq!(drive_key_entry("d_1"), KeyEntry::DriveKey.name("d_1"));
        assert_eq!(member_key_entry("d_1"), KeyEntry::MemberKey.name("d_1"));
        assert_eq!(planned_key_entry("d_1"), KeyEntry::PlannedKey.name("d_1"));
        assert_eq!(request_key_entry("d_1"), KeyEntry::RequestKey.name("d_1"));
        let (previous, next) = rotation_key_entries("d_1");
        assert_eq!(previous, KeyEntry::RotationPrevious.name("d_1"));
        assert_eq!(next, KeyEntry::RotationNext.name("d_1"));
        let all: Vec<String> = KeyEntry::ALL.iter().map(|e| e.name("d_1")).collect();
        assert_eq!(key_entries("d_1"), all);
    }

    /// AZ-K7: a plain file holds no key of a drive unless the user said it may. The refusal comes
    /// before anything reaches the bucket - not a drive with a recovery wrap and no key anywhere.
    #[test]
    fn az_k7_a_drives_key_is_not_kept_in_a_plain_file_unless_the_user_allowed_it() {
        let bucket = MemBucket::new();
        let refusing = PlainFile {
            inner: MemoryKeyring::new(),
            allow: Allow::Nothing,
        };
        let err = setup_new_drive(&bucket, &refusing, DRIVE, cheap()).unwrap_err();
        assert!(
            matches!(&err, DriveError::InvalidConfig(why) if why.contains("plain file")),
            "{err}"
        );
        assert!(
            bucket.keys().is_empty(),
            "nothing reached the bucket: {:?}",
            bucket.keys()
        );
        assert!(refusing
            .inner
            .get(&drive_key_entry(DRIVE))
            .unwrap()
            .is_none());
        assert!(refusing
            .inner
            .get(&planned_key_entry(DRIVE))
            .unwrap()
            .is_none());

        let allowed = PlainFile {
            inner: MemoryKeyring::new(),
            allow: Allow::PlainSecrets,
        };
        setup_new_drive(&bucket, &allowed, DRIVE, cheap()).unwrap();
        assert!(allowed
            .inner
            .get(&drive_key_entry(DRIVE))
            .unwrap()
            .is_some());

        // The keyring of the system and a map in memory take it without being told.
        setup_new_drive(&MemBucket::new(), &MemoryKeyring::new(), DRIVE, cheap()).unwrap();
        assert!(Protection::Os.keeps_key_material());
        assert!(Protection::Memory.keeps_key_material());
        assert!(!Protection::PlainFile {
            allow: Allow::Nothing
        }
        .keeps_key_material());
        assert!(Protection::PlainFile {
            allow: Allow::PlainSecrets
        }
        .keeps_key_material());
    }

    /// AZ-K3 / RF53: a recovery wrap goes into the bucket for a key the keyring was seen to hold -
    /// stored and read back, or read - and for no other: a wrap of another key writes nothing.
    #[test]
    fn az_k3_a_recovery_wrap_is_published_only_for_the_key_the_keyring_holds() {
        let bucket = MemBucket::new();
        let keyring = MemoryKeyring::new();
        let key = DriveKey::generate().unwrap();
        let other = DriveKey::generate().unwrap();
        let entry = planned_key_entry(DRIVE);

        let stashed = stash(&keyring, &entry, &key).unwrap();
        assert_eq!(stashed.key_id(), key.id());
        assert_eq!(stashed.entry(), entry);
        let (read, held_again) = held(&keyring, &entry).unwrap().unwrap();
        assert_eq!(read, key);
        assert_eq!(held_again.key_id(), key.id());
        assert!(held(&keyring, &drive_key_entry(DRIVE)).unwrap().is_none());

        let code = RecoveryCode::generate().unwrap();
        let of_other = RecoveryWrap::seal(&other, DRIVE, &code, cheap()).unwrap();
        let err =
            publish_recovery_wrap(&bucket, &stashed, &of_other, Publish::FirstNew).unwrap_err();
        assert!(matches!(err, DriveError::Io(_)), "{err}");
        assert!(
            bucket.keys().is_empty(),
            "a wrap of another key wrote nothing"
        );

        let wrap = RecoveryWrap::seal(&key, DRIVE, &code, cheap()).unwrap();
        publish_recovery_wrap(&bucket, &stashed, &wrap, Publish::FirstNew).unwrap();
        assert_eq!(bucket.keys().len(), 1);
        publish_recovery_wrap(&bucket, &stashed, &wrap, Publish::FirstReplace).unwrap();
        assert_eq!(bucket.keys().len(), 1);
    }

    /// AZ-K3 / RF53: a keyring that does not give a key back has not stashed it; no evidence is
    /// made, so no wrap is published for it.
    #[test]
    fn az_k3_a_keyring_that_does_not_give_the_key_back_stashes_nothing() {
        let key = DriveKey::generate().unwrap();
        let err = stash(&Forgetful, &planned_key_entry(DRIVE), &key).unwrap_err();
        assert!(
            matches!(&err, DriveError::Io(why) if why.contains("did not keep")),
            "{err}"
        );
        // And a setup over it publishes nothing.
        let bucket = MemBucket::new();
        assert!(setup_new_drive(&bucket, &Forgetful, DRIVE, cheap()).is_err());
        assert!(bucket.keys().is_empty(), "{:?}", bucket.keys());
    }
}
