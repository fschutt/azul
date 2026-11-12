//! A reason at every verb that destroys (ARCH30-1, step 1 of the migration): what each witness
//! covers, what the three `*_for` calls refuse and do, through a folder on disk, a grant, a pointer and
//! the encrypted drive. The macro-made wrappers' own delete path is in `drive_coverage.rs`
//! (`arch30_1_a_macro_made_wrapper_composes_the_reasons_over_its_own_delete_path`).

use std::sync::{Arc, Mutex};

use super::TempDir;
use crate::{
    beneath::{Dir, Root},
    ctl::Quiet,
    durable::{publish, Expect, Mode, Publish},
    erase::{Chose, Target, Unlicensed, Why},
    meta::certainty::Certainty,
    BatchDelete, ByteRange, Drive, DriveError, ListPage, ListRequest, LocalDrive, ObjectInfo,
    ScopedDrive,
};

fn key(text: &str) -> Target {
    Target::Key(text.to_string())
}

fn keys(texts: &[&str]) -> Target {
    Target::Keys(texts.iter().map(|text| (*text).to_string()).collect())
}

fn folder(text: &str) -> Target {
    Target::Folder(text.to_string())
}

// ==== What a witness covers ====

/// A job that named one file covers that key and no other spelling of it, and no folder.
#[test]
fn a_job_that_named_one_file_covers_that_key_and_nothing_else() {
    let chose = Chose::job("delete the file", key("d/a.txt"));
    let why = Why::UserChose(&chose);
    assert!(why.covers("d/a.txt"));
    for other in [
        "d/b.txt",
        "d/a.txt/",
        "D/a.txt",
        "a.txt",
        "d/a.txt ",
        "",
        "/d/a.txt",
        "d/../d/a.txt",
        "d//a.txt",
        "d/./a.txt",
    ] {
        assert!(!why.covers(other), "{other:?}");
    }
    for folder in ["d/", "", "/", "d/a.txt/"] {
        assert!(
            !why.covers_folder(folder),
            "one file is no reason to remove a folder: {folder:?}"
        );
    }
    assert_eq!(why.reason(), "delete the file");
}

/// A job that named several files covers each of them and nothing between them.
#[test]
fn a_job_that_named_several_files_covers_each_and_no_other() {
    let chose = Chose::job("delete the picks", keys(&["a.txt", "d/b.txt"]));
    let why = Why::UserChose(&chose);
    assert!(why.covers("a.txt") && why.covers("d/b.txt"));
    assert!(!why.covers("d/c.txt") && !why.covers("d/") && !why.covers("b.txt"));
    assert!(!why.covers_folder("d/"));
}

/// A job that named a folder covers the keys under it, the folder's own marker and the folders under
/// it - not the same text without the boundary, not a way out of it, not the root.
#[test]
fn a_job_that_named_a_folder_covers_what_is_under_it_and_no_more() {
    let chose = Chose::job("delete the folder", folder("d/"));
    let why = Why::UserChose(&chose);
    for under in ["d/x.txt", "d/e/x.txt", "d/"] {
        assert!(why.covers(under), "{under:?}");
    }
    for outside in [
        "d",
        "dd/x.txt",
        "e/x.txt",
        "d/../e/x.txt",
        "/d/x.txt",
        "d//x.txt",
        "",
    ] {
        assert!(!why.covers(outside), "{outside:?}");
    }
    for under in ["d/", "d/e/"] {
        assert!(why.covers_folder(under), "{under:?}");
    }
    for outside in ["d", "dd/", "e/", "", "/", "d/../e/"] {
        assert!(!why.covers_folder(outside), "{outside:?}");
    }
}

/// A target no key can be under - the root, an empty name, a "folder" with no `/` - is a reason for
/// nothing: a job that names the whole drive is not a reason to delete a name.
#[test]
fn a_job_whose_folder_is_the_root_or_no_folder_covers_nothing() {
    for target in ["", "/", "d", "d//"] {
        let chose = Chose::job("delete", folder(target));
        let why = Why::UserChose(&chose);
        for probe in ["x.txt", "d/x.txt", "d/", "d", ""] {
            assert!(!why.covers(probe), "{target:?} covers {probe:?}");
            assert!(
                !why.covers_folder(probe),
                "{target:?} covers the folder {probe:?}"
            );
        }
    }
}

/// The receipt of a publish is for the name the new version landed at, from the root the folder
/// was walked from: the same file name in two folders is two names.
#[test]
fn a_receipt_covers_the_name_the_version_landed_at_and_no_other() {
    let tmp = TempDir::new("erase-landed");
    std::fs::create_dir_all(tmp.path().join("a/b")).unwrap();
    let root = Root::open(tmp.path()).unwrap();
    let top = root.top().unwrap();
    let nested = root.dir_existing("a/b").unwrap();
    let publish_as = |dir: &Dir, name: &str| {
        publish(
            dir,
            name,
            &mut &b"v"[..],
            Publish::new(Expect::Any, Mode::Shared, &Quiet),
        )
        .unwrap()
    };
    let at_top = publish_as(&top, "c.txt");
    let deep = publish_as(&nested, "c.txt");
    assert_eq!(at_top.at(), "c.txt");
    assert_eq!(deep.at(), "a/b/c.txt");
    assert!(Why::Superseded(&at_top).covers("c.txt"));
    assert!(!Why::Superseded(&at_top).covers("a/b/c.txt"));
    assert!(Why::Superseded(&deep).covers("a/b/c.txt"));
    assert!(!Why::Superseded(&deep).covers("c.txt"));
    assert!(
        !Why::Superseded(&deep).covers_folder("a/b/"),
        "a receipt is for a name"
    );
}

// ==== What the three calls do ====

/// `delete_for` of a folder on disk, through the trait's default: the reason is for the key or
/// nothing is deleted, and the refusal is typed (`Unlicensed`, then `Denied`, a refusal that changed
/// nothing).
#[test]
fn a_delete_for_removes_what_the_reason_covers_and_refuses_the_rest_with_nothing_deleted() {
    let dir = TempDir::new("erase-local");
    let drive = LocalDrive::new(dir.path().to_path_buf());
    drive.put("a.txt", b"one").unwrap();
    drive.put("b.txt", b"two").unwrap();
    let for_b = Chose::job("delete b.txt", key("b.txt"));
    assert_eq!(
        Why::UserChose(&for_b).check("a.txt"),
        Err(Unlicensed {
            target: String::from("a.txt"),
            offered: "delete b.txt",
            folder: false,
        })
    );
    let refused = drive
        .delete_for("a.txt", Why::UserChose(&for_b))
        .unwrap_err();
    assert!(
        matches!(&refused, DriveError::Denied { message }
            if message.contains("\"a.txt\"") && message.contains("delete b.txt")),
        "{refused:?}"
    );
    assert_eq!(refused.certainty(), Certainty::Refused);
    assert_eq!(drive.get("a.txt").unwrap(), b"one", "nothing was deleted");
    drive.delete_for("b.txt", Why::UserChose(&for_b)).unwrap();
    assert!(matches!(
        drive.head("b.txt"),
        Err(DriveError::NotFound { .. })
    ));
    assert_eq!(drive.get("a.txt").unwrap(), b"one");
}

/// A batch is refused whole when one of its keys is not the reason's (one result per item, none
/// deleted), and deleted as `delete_many` does when every key is.
#[test]
fn a_batch_is_refused_whole_for_one_stray_key() {
    let dir = TempDir::new("erase-batch");
    let drive = LocalDrive::new(dir.path().to_path_buf());
    for name in ["a.txt", "b.txt", "c.txt"] {
        drive.put(name, b"x").unwrap();
    }
    let chose = Chose::job("delete a and b", keys(&["a.txt", "b.txt"]));
    let stray = [BatchDelete::new("a.txt"), BatchDelete::new("c.txt")];
    let answers = drive.delete_many_for(&stray, Why::UserChose(&chose));
    assert_eq!(answers.len(), 2, "one result per item");
    assert!(answers
        .iter()
        .all(|a| matches!(a, Err(DriveError::Denied { .. }))));
    assert!(
        drive.head("a.txt").is_ok(),
        "nothing of a refused batch is deleted"
    );
    let both = [BatchDelete::new("a.txt"), BatchDelete::new("b.txt")];
    let answers = drive.delete_many_for(&both, Why::UserChose(&chose));
    assert!(answers.iter().all(Result::is_ok), "{answers:?}");
    assert!(drive.head("a.txt").is_err() && drive.head("b.txt").is_err());
    assert!(drive.head("c.txt").is_ok());
    assert!(
        drive
            .delete_many_for(&[], Why::UserChose(&chose))
            .is_empty(),
        "an empty batch is an empty answer"
    );
}

/// A folder goes only for a job that named a folder: not for a job that named files, not for a
/// receipt, and not for a folder the job's folder does not hold.
#[test]
fn a_folder_goes_only_for_a_job_that_named_that_folder() {
    let dir = TempDir::new("erase-folder");
    let drive = LocalDrive::new(dir.path().to_path_buf());
    for name in ["old/a.txt", "old/sub/b.txt", "new/c.txt"] {
        drive.put(name, b"x").unwrap();
    }
    let files = Chose::job("delete the files", keys(&["old/a.txt", "old/sub/b.txt"]));
    let whole = Chose::job("delete old/", folder("old/"));
    let refused = drive.delete_folder_for("old/", Why::UserChose(&files));
    assert!(
        matches!(refused, Err(DriveError::Denied { .. })),
        "{refused:?}"
    );
    let elsewhere = drive.delete_folder_for("new/", Why::UserChose(&whole));
    assert!(
        matches!(elsewhere, Err(DriveError::Denied { .. })),
        "{elsewhere:?}"
    );
    assert!(drive.head("old/a.txt").is_ok() && drive.head("new/c.txt").is_ok());
    drive
        .delete_folder_for("old/", Why::UserChose(&whole))
        .unwrap();
    assert!(drive.head("old/a.txt").is_err() && drive.head("old/sub/b.txt").is_err());
    assert!(drive.head("new/c.txt").is_ok());
}

/// A grant keeps its own checks - the reason does not make a read-only grant writable - and the
/// reason is for the key the CALLER names, the one the job chose, not for the key with the grant's
/// prefix in front of it.
#[test]
fn a_grant_keeps_its_own_checks_and_the_reason_is_for_the_key_the_caller_names() {
    let dir = TempDir::new("erase-scoped");
    let local = Arc::new(LocalDrive::new(dir.path().to_path_buf()));
    local.put("shared/a.txt", b"x").unwrap();
    let for_name = Chose::job("delete a.txt", key("a.txt"));
    let for_inner = Chose::job("delete shared/a.txt", key("shared/a.txt"));

    let read_only = ScopedDrive::new(local.clone(), "shared/", false).unwrap();
    let refused = read_only.delete_for("a.txt", Why::UserChose(&for_name));
    assert!(
        matches!(&refused, Err(DriveError::Denied { message }) if message.contains("read-only")),
        "the grant's own refusal: {refused:?}"
    );
    assert!(local.head("shared/a.txt").is_ok());

    let writable = ScopedDrive::new(local.clone(), "shared/", true).unwrap();
    let rewritten = writable.delete_for("a.txt", Why::UserChose(&for_inner));
    assert!(
        matches!(&rewritten, Err(DriveError::Denied { message }) if message.contains("delete shared/a.txt")),
        "a reason for the grant's inner key is not one for the caller's: {rewritten:?}"
    );
    assert!(local.head("shared/a.txt").is_ok());
    writable
        .delete_for("a.txt", Why::UserChose(&for_name))
        .unwrap();
    assert!(
        local.head("shared/a.txt").is_err(),
        "the key was rewritten by the grant's own delete"
    );
}

// ==== A pointer to a drive is that drive ====

/// A drive that records what it is asked: its three `*_for` calls are its own (a leaf that must
/// answer for itself, the later step of the migration), so a pointer that did not forward them would
/// show as `delete` instead.
#[derive(Default)]
struct Recording {
    calls: Mutex<Vec<String>>,
}

impl Recording {
    fn note(&self, call: String) {
        self.calls.lock().unwrap().push(call);
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl Drive for Recording {
    fn list(&self, _request: &ListRequest) -> Result<ListPage, DriveError> {
        Ok(ListPage::default())
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        Err(DriveError::NotFound {
            key: key.to_string(),
        })
    }
    fn get_range(&self, key: &str, _range: ByteRange) -> Result<Vec<u8>, DriveError> {
        Err(DriveError::NotFound {
            key: key.to_string(),
        })
    }
    fn put(&self, _key: &str, _bytes: &[u8]) -> Result<(), DriveError> {
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.note(format!("delete:{key}"));
        Ok(())
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        Err(DriveError::NotFound {
            key: key.to_string(),
        })
    }
    fn delete_for(&self, key: &str, _why: Why<'_>) -> Result<(), DriveError> {
        self.note(format!("for:{key}"));
        Ok(())
    }
    fn delete_many_for(
        &self,
        items: &[BatchDelete<'_>],
        _why: Why<'_>,
    ) -> Vec<Result<(), DriveError>> {
        self.note(format!("many_for:{}", items.len()));
        items.iter().map(|_| Ok(())).collect()
    }
    fn delete_folder_for(&self, prefix: &str, _why: Why<'_>) -> Result<(), DriveError> {
        self.note(format!("folder_for:{prefix}"));
        Ok(())
    }
}

/// A drive that overrides none of them: the trait's default checks the reason and calls its own
/// `delete`, `delete_many` or `delete_folder` - once, with the key.
#[derive(Default)]
struct Plain {
    calls: Mutex<Vec<String>>,
}

impl Drive for Plain {
    fn list(&self, _request: &ListRequest) -> Result<ListPage, DriveError> {
        Ok(ListPage::default())
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        Err(DriveError::NotFound {
            key: key.to_string(),
        })
    }
    fn get_range(&self, key: &str, _range: ByteRange) -> Result<Vec<u8>, DriveError> {
        Err(DriveError::NotFound {
            key: key.to_string(),
        })
    }
    fn put(&self, _key: &str, _bytes: &[u8]) -> Result<(), DriveError> {
        Ok(())
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.calls.lock().unwrap().push(format!("delete:{key}"));
        Ok(())
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        Err(DriveError::NotFound {
            key: key.to_string(),
        })
    }
    fn delete_folder(&self, prefix: &str) -> Result<(), DriveError> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("delete_folder:{prefix}"));
        Ok(())
    }
}

/// X-DELTA-SYNC2-3's lesson for the new calls: a provided method that a pointer does not forward is
/// answered by the trait's default and the drive's own method is never asked. A `Box`, an `Arc` and
/// a reference hand the three calls to the drive they point at.
#[test]
fn a_box_an_arc_and_a_reference_hand_the_reasons_to_the_drive_they_point_at() {
    let recording = Arc::new(Recording::default());
    let chose = Chose::job("delete", key("k"));
    let folder_chose = Chose::job("delete the folder", folder("f/"));
    let item = [BatchDelete::new("k")];
    let ask = |drive: &dyn Drive| {
        drive.delete_for("k", Why::UserChose(&chose)).unwrap();
        let answers = drive.delete_many_for(&item, Why::UserChose(&chose));
        assert!(answers.iter().all(Result::is_ok));
        drive
            .delete_folder_for("f/", Why::UserChose(&folder_chose))
            .unwrap();
    };
    let as_arc: Arc<dyn Drive> = recording.clone();
    ask(&as_arc);
    let as_box: Box<dyn Drive> = Box::new(recording.clone());
    ask(&as_box);
    let by_reference: &dyn Drive = &*recording;
    ask(&by_reference);
    let expected = ["for:k", "many_for:1", "folder_for:f/"];
    let calls = recording.calls();
    assert_eq!(calls.len(), 9, "{calls:?}");
    for round in calls.chunks(3) {
        assert_eq!(
            round, expected,
            "the drive's own methods, not the default's"
        );
    }
}

/// The default composition over a drive that overrides nothing: its own `delete` is asked once, with
/// the key the caller named, and a reason that is for another key never gets that far.
#[test]
fn the_default_checks_the_reason_and_then_asks_the_drives_own_delete_once() {
    let plain = Plain::default();
    let chose = Chose::job("delete k", key("k"));
    plain.delete_for("k", Why::UserChose(&chose)).unwrap();
    assert!(plain.delete_for("other", Why::UserChose(&chose)).is_err());
    let batch = [BatchDelete::new("k")];
    assert!(plain
        .delete_many_for(&batch, Why::UserChose(&chose))
        .iter()
        .all(Result::is_ok));
    let whole = Chose::job("delete f/", folder("f/"));
    plain
        .delete_folder_for("f/", Why::UserChose(&whole))
        .unwrap();
    assert!(plain
        .delete_folder_for("g/", Why::UserChose(&whole))
        .is_err());
    assert_eq!(
        plain.calls.lock().unwrap().clone(),
        ["delete:k", "delete:k", "delete_folder:f/"],
        "the refused ones asked nothing"
    );
}

// ==== The witnesses of the encrypted drive's history, and its drives ====

#[cfg(feature = "encryption")]
mod encrypted {
    use std::sync::Arc;

    use super::{folder, key, Chose, Recording, Why};
    use crate::{
        crypto::{device::setup_new_drive, keys::RecoveryKdf, DriveKey, ObjectId, OBJECT_ID_LEN},
        encrypted::{AutoEncrypted, EncryptedDrive, IndexProvider, MemoryIndex, NameIndex},
        keyring::MemoryKeyring,
        meta::reclaim::{
            drop_new_object_after, DeleteLicence, PurgeConfirmation, Reclaimable, Refusal,
        },
        rotation::ReencryptState,
        tests::mem_bucket::MemBucket,
        Drive, DriveError,
    };

    fn object(seed: u8) -> ObjectId {
        ObjectId([seed; OBJECT_ID_LEN])
    }

    /// The reasons of the history are for the object they carry and no other, and for no NAME.
    #[test]
    fn the_reasons_of_the_history_are_for_the_one_object_they_carry() {
        let (a, b) = (object(1), object(2));
        let licence = DeleteLicence::lab();
        let proof = Reclaimable::lab(a);
        let confirmation = PurgeConfirmation::lab(7);
        let refusal = Refusal::lab(a);
        let reasons = [
            Why::Retired(&licence, &proof),
            Why::Confirmed(&confirmation, &proof),
            Why::Refused(&refusal),
        ];
        for why in &reasons {
            assert!(why.covers(&a.bucket_key()), "{}", why.reason());
            assert!(!why.covers(&b.bucket_key()), "{}", why.reason());
            assert!(!why.covers("a.txt"), "{}: no name", why.reason());
            assert!(
                !why.covers(&a.to_hex()),
                "{}: the hex is no key",
                why.reason()
            );
            assert!(!why.covers_folder("data/"), "{}: no folder", why.reason());
            assert!(why.check(&b.bucket_key()).is_err());
        }
    }

    /// The compensation of a change that did not name a new object is the one door a refusal is made
    /// at: a change that may have landed makes none and deletes nothing, one that was refused for good
    /// or never made deletes THAT object - and no other.
    #[test]
    fn a_new_object_is_taken_back_only_after_a_change_that_did_not_happen() {
        let recording = Recording::default();
        let (a, b) = (object(3), object(4));
        let lost = DriveError::Transport(String::from("no answer"));
        assert!(drop_new_object_after(&recording, &a, &lost).is_none());
        assert!(
            recording.calls().is_empty(),
            "it may have landed: nothing is deleted"
        );
        let refused = DriveError::Conflict {
            key: String::from("k"),
        };
        assert_eq!(
            drop_new_object_after(&recording, &a, &refused),
            Some(Ok(()))
        );
        assert_eq!(
            drop_new_object_after(&recording, &b, &DriveError::Cancelled),
            Some(Ok(())),
            "a change that was never made"
        );
        assert_eq!(
            recording.calls(),
            [
                format!("delete:{}", a.bucket_key()),
                format!("delete:{}", b.bucket_key())
            ],
            "each object by its own key"
        );
    }

    /// The confirmation exists only for an answer a re-encryption pass recorded.
    #[test]
    fn a_confirmation_is_made_only_of_the_answer_a_pass_recorded() {
        let mut state = ReencryptState::new(0);
        assert!(PurgeConfirmation::recorded_in(&state).is_none());
        assert!(state.purge_confirmation().is_none());
        state.confirm_purge(1234);
        let confirmation = PurgeConfirmation::recorded_in(&state).expect("recorded");
        assert_eq!(confirmation.at, 1234);
        assert_eq!(state.purge_confirmation().map(|c| c.at), Some(1234));
    }

    fn new_drive() -> EncryptedDrive<Arc<MemBucket>> {
        EncryptedDrive::new(
            Arc::new(MemBucket::new()),
            DriveKey::generate().unwrap(),
            Arc::new(MemoryIndex::new()),
        )
    }

    /// The encrypted drive: a reason is for a NAME of the drive's own key space, checked against the
    /// name the caller gives; the data object stays for the history; a reason for a bucket key is no
    /// reason to delete a name.
    #[test]
    fn an_encrypted_drive_deletes_a_name_for_a_reason_that_is_for_that_name() {
        let drive = new_drive();
        drive.put("x.txt", b"one").unwrap();
        drive.put("y.txt", b"two").unwrap();
        let (licence, proof) = (DeleteLicence::lab(), Reclaimable::lab(object(9)));
        let retired = drive.delete_for("y.txt", Why::Retired(&licence, &proof));
        assert!(
            matches!(retired, Err(DriveError::Denied { .. })),
            "{retired:?}"
        );
        let for_x = Chose::job("delete x.txt", key("x.txt"));
        let wrong_name = drive.delete_for("y.txt", Why::UserChose(&for_x));
        assert!(
            matches!(wrong_name, Err(DriveError::Denied { .. })),
            "{wrong_name:?}"
        );
        assert_eq!(drive.get("y.txt").unwrap(), b"two", "nothing was deleted");
        drive.delete_for("x.txt", Why::UserChose(&for_x)).unwrap();
        assert!(matches!(
            drive.get("x.txt"),
            Err(DriveError::NotFound { .. })
        ));
        assert_eq!(drive.get("y.txt").unwrap(), b"two");
        let whole = Chose::job("delete the folder", folder("d/"));
        drive.put("d/z.txt", b"three").unwrap();
        drive
            .delete_folder_for("d/", Why::UserChose(&whole))
            .unwrap();
        assert!(matches!(
            drive.get("d/z.txt"),
            Err(DriveError::NotFound { .. })
        ));
    }

    /// The tests' index provider: one index in memory that every device shares.
    struct Shared(Arc<MemoryIndex>);

    impl IndexProvider for Shared {
        fn open_index(
            &self,
            _drive: &str,
            _bucket: Arc<dyn Drive>,
            _drive_key: &DriveKey,
        ) -> Result<Arc<dyn NameIndex>, DriveError> {
            Ok(self.0.clone())
        }
    }

    /// The drive the apps open: the reasons go through the gate that every delete goes through. A
    /// drive that has no key yet answers as its delete does (the gate's refusal, not the reason's),
    /// and one that is set up deletes for a reason that is for the key and refuses any other.
    #[test]
    fn an_auto_encrypted_drive_sends_the_reasons_through_its_gate() {
        let mem = Arc::new(MemBucket::new());
        let keyring = Arc::new(MemoryKeyring::new());
        let provider: Option<Arc<dyn IndexProvider>> =
            Some(Arc::new(Shared(Arc::new(MemoryIndex::new()))));
        let drive = AutoEncrypted::new(mem.clone(), "d_erase", keyring.clone(), provider);
        let chose = Chose::job("delete a.txt", key("a.txt"));
        let plain_delete = drive.delete("a.txt");
        let for_delete = drive.delete_for("a.txt", Why::UserChose(&chose));
        assert_eq!(
            for_delete.is_err(),
            plain_delete.is_err(),
            "the gate answers a reason as it answers a delete: {for_delete:?} / {plain_delete:?}"
        );
        let kdf = RecoveryKdf::with_cost(64, 1, 1).unwrap();
        setup_new_drive(drive.bucket().as_ref(), keyring.as_ref(), "d_erase", kdf).unwrap();
        drive.reopen();
        drive.put("a.txt", b"sealed").unwrap();
        drive.put("b.txt", b"sealed too").unwrap();
        let refused = drive.delete_for("b.txt", Why::UserChose(&chose));
        assert!(
            matches!(refused, Err(DriveError::Denied { .. })),
            "{refused:?}"
        );
        assert_eq!(drive.get("b.txt").unwrap(), b"sealed too");
        drive.delete_for("a.txt", Why::UserChose(&chose)).unwrap();
        assert!(matches!(
            drive.get("a.txt"),
            Err(DriveError::NotFound { .. })
        ));
        let both = [crate::BatchDelete::new("b.txt")];
        let batch = drive.delete_many_for(&both, Why::UserChose(&chose));
        assert!(
            matches!(batch[0], Err(DriveError::Denied { .. })),
            "{batch:?}"
        );
        assert_eq!(drive.get("b.txt").unwrap(), b"sealed too");
    }
}
