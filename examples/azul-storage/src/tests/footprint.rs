//! What a drive leaves on a device (ARCH28 RF53 / AR5b-4): the names of its keyring entries, the
//! removal that tries every store and takes the drives-file entry last, the evidence a key needs
//! before it is erased or a wrap published, and the plain file that takes no key.

#[cfg(feature = "encryption")]
use std::sync::atomic::{AtomicBool, Ordering};

use super::TempDir;
use crate::{
    config::{DriveEntry, DriveLocation, DrivesFile},
    footprint::{
        contact_key_entry, forget_drive, DrivesListing, Footprint, KeptBecause, KeyEntry,
        KeyringFootprint, Paths, StoreOutcome,
    },
    keyring::{KeyringError, KeyringStore, MemoryKeyring},
};
#[cfg(feature = "encryption")]
use crate::footprint::RemovalProof;

const DRIVE: &str = "d_k3f9";

/// A proof as the apps make one: the key permit's Permit of a removal of `drive`, asked on a keyring that
/// holds the drive's finished setup, with a recovery code typed back right and known to the token server
/// and no vault of its own to lose months from. It is not made from a name: the judge makes it, or nobody.
#[cfg(feature = "encryption")]
fn proof_for(drive: &str) -> RemovalProof {
    use crate::crypto::{
        device,
        keys::RecoveryKdf,
        keystate::{ask, Evidence, Op, Read, RecoveryRecord, TokenFacts, VaultFacts, Verdict},
    };

    struct Checked;
    impl RecoveryRecord for Checked {
        fn code_checked(&self) -> Read<u64> {
            Read::Found(1_000)
        }
        fn code_registered(&self) -> Read<bool> {
            Read::Found(true)
        }
    }

    let keyring = MemoryKeyring::new();
    let bucket = super::mem_bucket::MemBucket::new();
    let kdf = RecoveryKdf::with_cost(64, 1, 1).unwrap();
    device::setup_new_drive(&bucket, &keyring, drive, kdf).unwrap();
    let mut evidence = Evidence::new(drive, 2_000);
    evidence.keyring = device::key_facts(&keyring, drive);
    evidence.token = TokenFacts::of(&Checked);
    evidence.vault = VaultFacts::no_vault();
    match ask(Op::Remove, &evidence) {
        Verdict::Permit(permit) => {
            RemovalProof::from_permit(permit).expect("the permit of a removal")
        }
        Verdict::Refuse(reason) => panic!("{reason}"),
    }
}

/// Every kind of entry a drive has, filled for `drive`.
#[cfg(feature = "encryption")]
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
#[cfg(feature = "encryption")]
struct Stubborn {
    inner: MemoryKeyring,
    refuses: String,
    allowed: AtomicBool,
}

#[cfg(feature = "encryption")]
impl Stubborn {
    fn refusing(entry: String) -> Stubborn {
        Stubborn {
            inner: MemoryKeyring::new(),
            refuses: entry,
            allowed: AtomicBool::new(false),
        }
    }
}

#[cfg(feature = "encryption")]
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
            "azul-storage/s3/d_1/device-key",
            "azul-storage/drive-key/d_1",
            "azul-storage/drive-key-previous/d_1",
            "azul-storage/drive-key-next/d_1",
            "azul-storage/drive-key-planned/d_1",
            "azul-storage/member-key/d_1",
            "azul-storage/recovery-request/d_1",
            "azul-storage/vault-key/d_1",
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
    // Only the session, the drive token, the device key and the vault's key are credentials; the
    // rest is key material.
    let credentials: Vec<KeyEntry> = KeyEntry::ALL
        .into_iter()
        .filter(|e| !e.is_key_material())
        .collect();
    assert_eq!(
        credentials,
        [
            KeyEntry::Session,
            KeyEntry::AzlinToken,
            KeyEntry::DeviceKey,
            KeyEntry::VaultKey
        ]
    );
}

/// LANE-AZ-2: a removal without the evidence that the recovery code was checked takes the
/// credentials and leaves the keys, and says so; with it, every entry of the drive goes and no
/// entry of another drive does.
#[cfg(feature = "encryption")]
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
    assert_eq!(
        done.kept.len(),
        8,
        "the keys, the vault's key and the device key stay: {done:?}"
    );
    assert!(done.kept.iter().all(|k| k.because == KeptBecause::NoProof));
    assert_eq!(store.residue(DRIVE).unwrap().len(), 8);

    let proof = proof_for(DRIVE);
    let done = store.forget(DRIVE, Some(&proof)).unwrap();
    assert!(done.kept.is_empty(), "{done:?}");
    assert!(store.residue(DRIVE).unwrap().is_empty());
    // Every entry of the other drive is still there, its credentials too: nothing of it was asked.
    let mut other: Vec<String> = KeyEntry::ALL.iter().map(|e| e.name("d_other")).collect();
    other.sort();
    let mut left = store.residue("d_other").unwrap();
    left.sort();
    assert_eq!(left, other, "another drive's entries stay");
    // Nothing there: nothing to refuse.
    store.forget(DRIVE, Some(&proof)).unwrap();
}

/// K6, on the removal's own path (`device::forget_keys`, which took every entry and asked no permit, is gone):
/// one entry the keyring refuses to delete does not keep the others - every one is tried, and the refusal is
/// the answer, with the entry's name; the drive stays listed for another try.
#[cfg(feature = "encryption")]
#[test]
fn k6_a_removal_tries_every_entry_and_reports_the_refusal() {
    struct RefusesTheMemberKey(MemoryKeyring);
    impl KeyringStore for RefusesTheMemberKey {
        fn get(&self, key: &str) -> Result<Option<String>, KeyringError> {
            self.0.get(key)
        }
        fn set(&self, key: &str, secret: &str) -> Result<(), KeyringError> {
            self.0.set(key, secret)
        }
        fn delete(&self, key: &str) -> Result<(), KeyringError> {
            if key == KeyEntry::MemberKey.name(DRIVE) {
                return Err(KeyringError::Denied);
            }
            self.0.delete(key)
        }
    }
    let keyring = RefusesTheMemberKey(MemoryKeyring::new());
    filled(&keyring.0, DRIVE);
    let store = KeyringFootprint::new(&keyring);
    let proof = proof_for(DRIVE);

    let failed = store.forget(DRIVE, Some(&proof)).unwrap_err();
    assert!(
        failed
            .to_string()
            .contains(&KeyEntry::MemberKey.name(DRIVE)),
        "{failed}"
    );
    assert_eq!(
        store.residue(DRIVE).unwrap(),
        [KeyEntry::MemberKey.name(DRIVE)],
        "every other entry went; the refused one is the residue"
    );
}

/// X-DELTA-STRUCT-KEYSTATE-7: the removal's keys go through the registry's forget, which is for the permit of
/// a removal of ONE drive. A proof for another drive handed straight to the keyring's store - not through
/// `forget_drive`, which drops it - erases no key of this one: the credentials go (they need no permit), every
/// key entry is refused and named, and stays.
#[cfg(feature = "encryption")]
#[test]
fn x_delta_struct_keystate_7_a_proof_for_another_drive_handed_to_the_keyring_store_erases_no_key() {
    let keyring = MemoryKeyring::new();
    filled(&keyring, DRIVE);
    let store = KeyringFootprint::new(&keyring);
    let proof = proof_for("d_other");

    let failed = store.forget(DRIVE, Some(&proof)).unwrap_err();
    let mut keys: Vec<String> = KeyEntry::ALL
        .iter()
        .filter(|entry| entry.goes_with_proof())
        .map(|entry| entry.name(DRIVE))
        .collect();
    keys.sort();
    for key in &keys {
        assert!(failed.to_string().contains(key), "{key} is named: {failed}");
    }
    let mut left = store.residue(DRIVE).unwrap();
    left.sort();
    assert_eq!(left, keys, "the credentials went, every key stayed");
}

/// X-DELTA-AZDRIVE-KEYS-32 (a): the device key's entry (azcloud-kit built its name by suffixing
/// the session's) was in no list of a drive's entries, so a removal never found it. It is a kind of
/// `KeyEntry`, named below the session's, and it goes with the proof like the keys: it is what
/// gets a locked drive back to its owner.
#[cfg(feature = "encryption")]
#[test]
fn x_delta_azdrive_keys_32_the_device_key_is_an_entry_of_the_drive_and_goes_with_its_proof() {
    let keyring = MemoryKeyring::new();
    let device = KeyEntry::DeviceKey.name(DRIVE);
    assert_eq!(
        device,
        format!("{}/device-key", KeyEntry::Session.name(DRIVE)),
        "the name users' keyrings already hold"
    );
    assert_eq!(KeyEntry::of_name(&device, DRIVE), Some(KeyEntry::DeviceKey));
    assert_eq!(
        KeyEntry::of_name(&KeyEntry::Session.name(DRIVE), DRIVE),
        Some(KeyEntry::Session),
        "the session is not mistaken for it"
    );
    keyring.set(&device, "{\"seed\":\"c2VlZA\"}").unwrap();
    keyring
        .set(&KeyEntry::Session.name(DRIVE), "a session")
        .unwrap();
    keyring
        .set(&KeyEntry::DeviceKey.name("d_other"), "another")
        .unwrap();
    let store = KeyringFootprint::new(&keyring);

    // without the proof: the credentials go (the session's entry and the drive token's, said
    // whether or not they were there: `Forgotten::removed`), the device key stays and is said to
    let done = store.forget(DRIVE, None).unwrap();
    assert_eq!(
        done.removed,
        [
            KeyEntry::Session.name(DRIVE),
            KeyEntry::AzlinToken.name(DRIVE)
        ]
    );
    let kept: Vec<&str> = done.kept.iter().map(|k| k.what.as_str()).collect();
    assert_eq!(kept, [device.as_str()], "{done:?}");
    assert_eq!(store.residue(DRIVE).unwrap(), [device.clone()]);

    // with it: gone, and another drive's is not touched
    let proof = proof_for(DRIVE);
    store.forget(DRIVE, Some(&proof)).unwrap();
    assert!(keyring.get(&device).unwrap().is_none());
    assert!(store.residue(DRIVE).unwrap().is_empty());
    assert!(keyring
        .get(&KeyEntry::DeviceKey.name("d_other"))
        .unwrap()
        .is_some());
}

/// X-DELTA-SYNC4-25: the legacy folder an older program handed copies out of (no record of what it wrote) is a store of
/// the removal: with no file in it, it goes; with a file in it - maybe what the user changed, which nothing synced - it
/// is made private and stays, said (`UsersWork`), and the removal is still complete.
#[cfg(unix)]
#[test]
fn x_delta_sync4_25_a_legacy_copies_folder_goes_when_it_holds_no_file_and_stays_when_it_holds_one() {
    use std::os::unix::fs::PermissionsExt;

    use crate::footprint::LegacyCopiesFootprint;

    let dir = TempDir::new("footprint-legacy-copies");
    let empty = dir.path().join("picked").join("d_1");
    std::fs::create_dir_all(empty.join("folder-only")).unwrap();
    let held = dir.path().join("picked").join("d_2");
    std::fs::create_dir_all(held.join("docs")).unwrap();
    std::fs::write(held.join("docs").join("a.txt"), b"what the user saved here").unwrap();
    std::fs::set_permissions(&held, std::fs::Permissions::from_mode(0o755)).unwrap();
    let store = LegacyCopiesFootprint::new(
        "picked copies",
        vec![empty.clone(), held.clone(), dir.path().join("picked").join("d_3")],
    );
    assert_eq!(store.residue(DRIVE).unwrap().len(), 2, "d_3 is not there");
    let done = store.forget(DRIVE, None).unwrap();
    assert!(!empty.exists(), "a folder with no file goes");
    assert_eq!(
        std::fs::read(held.join("docs").join("a.txt")).unwrap(),
        b"what the user saved here",
        "the user's copy is where it was"
    );
    assert_eq!(
        std::fs::metadata(&held).unwrap().permissions().mode() & 0o777,
        0o700,
        "closed to other users"
    );
    assert_eq!(
        done.kept,
        vec![crate::footprint::Kept {
            what: held.display().to_string(),
            because: KeptBecause::UsersWork
        }]
    );
    assert_eq!(done.removed.len(), 2, "the empty one and the one that was not there: {done:?}");
}

/// X-DELTA-AZDRIVE-KEYS-32 (c): the decrypted copies of a drive's files handed to apps lived in one
/// scratch purpose for every drive and a removal could not tell them: they waited for the sweep of
/// a day. A purpose of the drive's own is a store of the removal: the copies the program made go,
/// a folder that holds the user's own work (a copy edited and saved beside it) stays - said - and
/// another drive's copies are not touched.
#[cfg(unix)]
#[test]
fn x_delta_azdrive_keys_32_the_copies_of_a_removed_drive_go_and_the_users_own_work_stays() {
    // Here and not at the top of the file: only this test (unix only) uses them, and an import no
    // test uses is a warning on Windows, where the CI fails on any warning of the tree.
    use crate::{footprint::ScratchFootprint, scratch::Scratch};

    let dir = TempDir::new("footprint-open-copies");
    let mine = Scratch::tagged_under(dir.path(), "open", "d_1");
    let other = Scratch::tagged_under(dir.path(), "open", "d_2");
    assert_ne!(mine.dir(), other.dir());
    let make = |scratch: &Scratch, tag: &str, name: &str| {
        let folder = scratch.keep(tag).unwrap();
        std::fs::write(folder.join(name), b"the plain text of a file").unwrap();
        Scratch::note_made(&folder, name).unwrap();
        folder
    };
    let copy = make(&mine, "docs/a.txt\0v1\05", "a.txt");
    let edited = make(&mine, "docs/b.txt\0v1\05", "b.txt");
    std::fs::write(edited.join("b (edited).txt"), b"the user's words").unwrap();
    let theirs = make(&other, "docs/c.txt\0v1\05", "c.txt");

    let store = ScratchFootprint::new("open copies", mine.clone());
    assert_eq!(store.residue("d_1").unwrap().len(), 2);
    let done = store.forget("d_1", None).unwrap();
    assert!(done.removed.is_empty(), "something stays: {done:?}");
    assert_eq!(done.kept.len(), 1, "{done:?}");
    assert_eq!(done.kept[0].because, KeptBecause::UsersWork);
    assert!(!copy.exists(), "the program's own copy is gone");
    assert_eq!(
        std::fs::read(edited.join("b (edited).txt")).unwrap(),
        b"the user's words",
        "the user's work is not the program's to delete"
    );
    assert!(theirs.join("c.txt").exists(), "another drive's copies stay");

    // Once the user's work is gone too, the drive's purpose folder goes with the rest.
    std::fs::remove_dir_all(&edited).unwrap();
    let done = store.forget("d_1", None).unwrap();
    assert_eq!(done.removed.len(), 1, "{done:?}");
    assert!(done.kept.is_empty());
    assert!(!mine.dir().exists());
    assert!(store.residue("d_1").unwrap().is_empty());
    // and a removal of a drive that opened nothing is nothing to do
    assert!(store.forget("d_1", None).unwrap().kept.is_empty());
}

/// The report's "keys kept" is about the proof only: a user's file that stayed is not a key.
#[test]
fn x_delta_azdrive_keys_32_a_users_file_that_stayed_is_not_keys_kept() {
    use crate::footprint::{ForgetReport, Forgotten, Kept, StoreOutcome, StoreReport};
    let report = |because: KeptBecause| ForgetReport {
        stores: vec![StoreReport {
            store: "x",
            outcome: StoreOutcome::Done(Forgotten {
                removed: Vec::new(),
                kept: vec![Kept {
                    what: String::from("a"),
                    because,
                }],
            }),
        }],
    };
    assert!(report(KeptBecause::NoProof).keys_kept());
    assert!(!report(KeptBecause::UsersWork).keys_kept());
    assert!(!ForgetReport::default().keys_kept());
}

/// X-DELTA-AZDRIVE-KEYS-8: the period vault's FILES go only with a proof (the months paid for the
/// drive may be only there); the key that opens them went without one, and the files that stayed
/// were ciphertext nobody could read. The key stays with the files.
#[cfg(feature = "encryption")]
#[test]
fn x_delta_azdrive_keys_8_the_key_of_the_period_vault_goes_only_with_the_proof_its_files_wait_for()
{
    let keyring = MemoryKeyring::new();
    keyring
        .set(&KeyEntry::VaultKey.name(DRIVE), "the vault's key")
        .unwrap();
    keyring
        .set(&KeyEntry::Session.name(DRIVE), "a session")
        .unwrap();
    let store = KeyringFootprint::new(&keyring);

    let done = store.forget(DRIVE, None).unwrap();
    let kept: Vec<&str> = done.kept.iter().map(|k| k.what.as_str()).collect();
    assert_eq!(kept, [KeyEntry::VaultKey.name(DRIVE).as_str()], "{done:?}");
    assert!(keyring
        .get(&KeyEntry::VaultKey.name(DRIVE))
        .unwrap()
        .is_some());
    assert!(
        keyring
            .get(&KeyEntry::Session.name(DRIVE))
            .unwrap()
            .is_none(),
        "a credential goes without a proof"
    );

    let proof = proof_for(DRIVE);
    store.forget(DRIVE, Some(&proof)).unwrap();
    assert!(keyring
        .get(&KeyEntry::VaultKey.name(DRIVE))
        .unwrap()
        .is_none());

    // The rule in one place: key material and the vault's key wait for a proof, the credentials
    // of the session do not.
    for entry in KeyEntry::ALL {
        assert_eq!(
            entry.goes_with_proof(),
            entry.is_key_material() || matches!(entry, KeyEntry::VaultKey | KeyEntry::DeviceKey),
            "{entry:?}"
        );
    }
    assert!(!KeyEntry::Session.goes_with_proof() && !KeyEntry::AzlinToken.goes_with_proof());
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
#[cfg(feature = "encryption")]
#[test]
fn lane_az_2_a_proof_for_another_drive_is_no_proof() {
    let keyring = MemoryKeyring::new();
    filled(&keyring, DRIVE);
    let store = KeyringFootprint::new(&keyring);
    let other = proof_for("d_other");
    let report = forget_drive(&[&store], DRIVE, Some(&other));
    assert!(report.complete(), "{report:?}");
    // What stays is every entry that goes only with a proof - the keys, the vault's key (KEYS-8)
    // and the device key (KEYS-32) - and what goes is the two credentials, by name.
    let mut stays: Vec<String> = [
        KeyEntry::DriveKey,
        KeyEntry::RotationPrevious,
        KeyEntry::RotationNext,
        KeyEntry::PlannedKey,
        KeyEntry::MemberKey,
        KeyEntry::RequestKey,
        KeyEntry::VaultKey,
        KeyEntry::DeviceKey,
    ]
    .iter()
    .map(|e| e.name(DRIVE))
    .collect();
    stays.sort();
    let mut kept: Vec<String> = report.kept().iter().map(|k| k.what.clone()).collect();
    kept.sort();
    assert_eq!(kept, stays, "the keys stay: {report:?}");
    let mut residue = store.residue(DRIVE).unwrap();
    residue.sort();
    assert_eq!(residue, stays, "and nothing else of the drive is left");
}

/// LANE-AZ-2 / K6: the removal tries EVERY store whatever one of them did, and takes the drive off
/// the list LAST and only when no store failed - given first, the listing still goes last. A delete
/// the keyring refused leaves the drive on the list, its entries findable by its id, and the same
/// call finishes the job once the keyring lets go.
#[cfg(feature = "encryption")]
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
    let proof = proof_for(DRIVE);

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
        // The key permit's `Setup` of the drive whose key is planned: the ask of the flow that publishes the wrap.
        let setup = {
            use crate::crypto::{
                device::{keeps_keys_fact, key_facts},
                keystate::{ask, Evidence, Op, Read, Verdict},
            };
            let mut evidence = Evidence::new(DRIVE, 1);
            evidence.keyring = key_facts(&keyring, DRIVE);
            evidence.keyring.keeps_keys = keeps_keys_fact(&keyring);
            evidence.bucket.encrypted = Read::Found(false);
            match ask(Op::Setup, &evidence) {
                Verdict::Permit(permit) => permit,
                Verdict::Refuse(reason) => panic!("{reason}"),
            }
        };
        let (read, held_again) = held(&keyring, &entry).unwrap().unwrap();
        assert_eq!(read, key);
        assert_eq!(held_again.key_id(), key.id());
        assert!(held(&keyring, &drive_key_entry(DRIVE)).unwrap().is_none());

        let code = RecoveryCode::generate().unwrap();
        let of_other = RecoveryWrap::seal(&other, DRIVE, &code, cheap()).unwrap();
        let err = publish_recovery_wrap(&setup, &bucket, &stashed, &of_other, Publish::FirstNew)
            .unwrap_err();
        assert!(matches!(err, DriveError::Io(_)), "{err}");
        assert!(
            bucket.keys().is_empty(),
            "a wrap of another key wrote nothing"
        );

        let wrap = RecoveryWrap::seal(&key, DRIVE, &code, cheap()).unwrap();
        publish_recovery_wrap(&setup, &bucket, &stashed, &wrap, Publish::FirstNew).unwrap();
        assert_eq!(bucket.keys().len(), 1);
        publish_recovery_wrap(&setup, &bucket, &stashed, &wrap, Publish::FirstReplace).unwrap();
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
