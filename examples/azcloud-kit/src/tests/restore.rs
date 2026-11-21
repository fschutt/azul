//! "Restore the drive as of <time>" (D42): an encrypted drive over its metadata repository -
//! the whole drive back as it was, as one more commit, its policy and keys left as they are -
//! and a plain drive's bucket by the token server and the drive's node.

use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use azul_storage::{
    meta::{merge::keep_both, tree::walk, Change, MemoryBucket, MetaRepo, Mode, TestSealer},
    testing::TempDir,
};

use super::{header, json, period::keyring_with_session, Fake, Shared, TOKEN};
use crate::{
    restore::{
        file_versions, restore_bucket_as_of, restore_drive_as_of, restore_file_version, state_at,
        BucketRestore,
    },
    token::TokenServer,
};

type Repo = MetaRepo<MemoryBucket, TestSealer>;

/// When the drive was made.
const T0: u64 = 1_791_000_000;

fn sealer() -> TestSealer {
    TestSealer::new([33; 32])
}

/// A device's copy of the drive in `bucket` (`create`: the first), on the shared `clock`.
fn device(bucket: &MemoryBucket, clock: &Arc<AtomicU64>, name: &str, create: bool) -> Repo {
    let repo = if create {
        Repo::create(bucket.clone(), sealer(), name, name)
    } else {
        Repo::open(bucket.clone(), sealer(), name, name)
    };
    let clock = Arc::clone(clock);
    repo.unwrap()
        .with_clock(move || clock.load(Ordering::SeqCst))
}

fn put(repo: &mut Repo, path: &str, content: &str) -> Change {
    Change::Put {
        path: path.to_string(),
        id: repo.write_blob(content.as_bytes()),
    }
}

/// Every file of the drive with its content.
fn files(repo: &Repo) -> BTreeMap<String, String> {
    let Some(root) = repo.root().unwrap() else {
        return BTreeMap::new();
    };
    walk(repo.objects(), &root)
        .unwrap()
        .into_iter()
        .filter(|(_, (mode, _))| *mode == Mode::File)
        .map(|(path, (_, id))| {
            let text = String::from_utf8(repo.objects().blob(&id).unwrap().to_vec()).unwrap();
            (path, text)
        })
        .collect()
}

fn expect(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(path, text)| ((*path).to_string(), (*text).to_string()))
        .collect()
}

/// A drive made at T0, then at T0 + 100 rewritten (a ransomware run): one file encrypted, one
/// deleted, one note left, the policy changed (a member removed).
fn drive_and_attack(bucket: &MemoryBucket, clock: &Arc<AtomicU64>) -> Repo {
    clock.store(T0, Ordering::SeqCst);
    let mut laptop = device(bucket, clock, "laptop", true);
    let changes = vec![
        put(&mut laptop, "a.txt", "a1"),
        put(&mut laptop, "docs/b.txt", "b1"),
        put(&mut laptop, ".azlin/policy.toml", "members = 2"),
    ];
    laptop.commit(&changes, "init", &mut keep_both).unwrap();
    clock.store(T0 + 100, Ordering::SeqCst);
    let changes = vec![
        put(&mut laptop, "a.txt", "encrypted"),
        Change::Delete {
            path: String::from("docs/b.txt"),
        },
        put(&mut laptop, "READ-ME-TO-DECRYPT.txt", "pay"),
        put(&mut laptop, ".azlin/policy.toml", "members = 1"),
    ];
    laptop.commit(&changes, "attack", &mut keep_both).unwrap();
    laptop
}

#[test]
fn the_whole_drive_comes_back_as_it_was_as_one_more_commit() {
    let bucket = MemoryBucket::new();
    let clock = Arc::new(AtomicU64::new(T0));
    let mut laptop = drive_and_attack(&bucket, &clock);
    clock.store(T0 + 200, Ordering::SeqCst);
    let as_of = (T0 + 50) as i64;
    let restore = restore_drive_as_of(&mut laptop, as_of, &mut keep_both)
        .unwrap()
        .expect("the drive had a state then");
    assert_eq!((restore.restored, restore.removed), (2, 1));
    assert!(restore.outcome.is_some(), "a commit");
    assert_eq!(
        files(&laptop),
        expect(&[
            (".azlin/policy.toml", "members = 1"),
            ("a.txt", "a1"),
            ("docs/b.txt", "b1"),
        ]),
        "the files as they were; the policy and keys as they are"
    );
    let history = laptop.history().unwrap();
    assert_eq!(history.len(), 3);
    assert_eq!(
        history[0].1.message,
        format!("Restore the drive as of {}", crate::rfc3339(as_of))
    );
    // Another device sees the restored drive.
    let mut phone = device(&bucket, &clock, "phone", false);
    phone.pull().unwrap();
    assert_eq!(files(&phone), files(&laptop));
    // Nothing is lost: the drive as it was before the restore comes back the same way.
    clock.store(T0 + 300, Ordering::SeqCst);
    restore_drive_as_of(&mut phone, (T0 + 150) as i64, &mut keep_both)
        .unwrap()
        .expect("a state then");
    assert_eq!(files(&phone)["a.txt"], "encrypted");
    assert_eq!(files(&phone)["READ-ME-TO-DECRYPT.txt"], "pay");
}

#[test]
fn a_drive_already_as_it_was_gets_no_commit_and_a_time_before_it_was_made_restores_nothing() {
    let bucket = MemoryBucket::new();
    let clock = Arc::new(AtomicU64::new(T0));
    let mut laptop = drive_and_attack(&bucket, &clock);
    let before = laptop.history().unwrap().len();
    let restore = restore_drive_as_of(&mut laptop, (T0 + 150) as i64, &mut keep_both)
        .unwrap()
        .expect("a state then");
    assert_eq!((restore.restored, restore.removed), (0, 0));
    assert!(restore.outcome.is_none());
    assert!(
        restore_drive_as_of(&mut laptop, (T0 - 1) as i64, &mut keep_both)
            .unwrap()
            .is_none()
    );
    assert_eq!(laptop.history().unwrap().len(), before);
}

#[test]
fn the_state_at_a_time_is_the_newest_commit_made_then_or_before() {
    let bucket = MemoryBucket::new();
    let clock = Arc::new(AtomicU64::new(T0));
    let laptop = drive_and_attack(&bucket, &clock);
    let history = laptop.history().unwrap();
    let (attack, init) = (history[0].0, history[1].0);
    assert_eq!(state_at(&history, T0 as i64), Some(init));
    assert_eq!(state_at(&history, (T0 + 99) as i64), Some(init));
    assert_eq!(state_at(&history, (T0 + 100) as i64), Some(attack));
    assert_eq!(state_at(&history, (T0 - 1) as i64), None);
}

/// A token server whose restore of `d_1` is `r_1`, its progress `states` one after the other (the
/// last one from then on); every call with the newest drive token.
fn restoring_server(states: &'static [&'static str]) -> Arc<Fake> {
    Fake::new(move |call, n| {
        assert_eq!(header(call, "authorization"), Some("Bearer dt_f.4.newest"));
        if n == 0 {
            assert_eq!(call.url, format!("{TOKEN}/v1/drives/d_1/restore"));
            let body: serde_json::Value = serde_json::from_slice(&call.body).unwrap();
            assert_eq!(body["prefix"], "", "the whole bucket");
            assert_eq!(body["as_of"], "2026-10-10T08:00:00Z");
            return Ok(json(202, r#"{"request_id": "r_1", "status": "queued"}"#));
        }
        assert_eq!(call.url, format!("{TOKEN}/v1/drives/d_1/restore/r_1"));
        Ok(json(200, states[(n - 1).min(states.len() - 1)]))
    })
}

#[test]
fn a_plain_drives_bucket_is_restored_by_the_token_server_and_its_progress_followed() {
    let dir = TempDir::new("azcloud-restore");
    let shared = keyring_with_session(&dir, "dt_f.4.newest");
    let as_of = crate::parse_rfc3339("2026-10-10T08:00:00Z").unwrap();
    let transport = Shared(restoring_server(&[
        r#"{"request_id": "r_1", "status": "queued"}"#,
        r#"{"request_id": "r_1", "status": "done", "objects": 3}"#,
    ]));
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let wait = Duration::from_secs(5);
    assert_eq!(
        restore_bucket_as_of(&server, &shared, "d_1", as_of, Duration::ZERO, wait).unwrap(),
        BucketRestore::Done {
            request: String::from("r_1"),
            objects: 3
        }
    );
    // Refused by the node: its word.
    let transport = Shared(restoring_server(&[
        r#"{"request_id": "r_1", "status": "failed", "error": "version vanished"}"#,
    ]));
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert_eq!(
        restore_bucket_as_of(&server, &shared, "d_1", as_of, Duration::ZERO, wait).unwrap(),
        BucketRestore::Failed {
            request: String::from("r_1"),
            error: String::from("version vanished")
        }
    );
    // Still queued when the wait is over: the request, to ask about later.
    let transport = Shared(restoring_server(&[
        r#"{"request_id": "r_1", "status": "queued"}"#,
    ]));
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert_eq!(
        restore_bucket_as_of(
            &server,
            &shared,
            "d_1",
            as_of,
            Duration::ZERO,
            Duration::ZERO
        )
        .unwrap(),
        BucketRestore::Queued {
            request: String::from("r_1")
        }
    );
}

#[test]
fn a_files_versions_are_listed_once_each_and_an_old_or_deleted_one_comes_back() {
    let bucket = MemoryBucket::new();
    let clock = Arc::new(AtomicU64::new(T0));
    let mut laptop = device(&bucket, &clock, "laptop", true);
    let v1 = put(&mut laptop, "report.txt", "version one");
    let kept = put(&mut laptop, "gone.txt", "kept");
    laptop.commit(&[v1, kept], "one", &mut keep_both).unwrap();
    clock.store(T0 + 10, Ordering::SeqCst);
    let v2 = put(&mut laptop, "report.txt", "version two");
    laptop.commit(&[v2], "two", &mut keep_both).unwrap();
    // A commit that leaves the file as it is adds no version.
    clock.store(T0 + 20, Ordering::SeqCst);
    let other = put(&mut laptop, "other.txt", "x");
    let deleted = Change::Delete { path: String::from("gone.txt") };
    laptop.commit(&[other, deleted], "other", &mut keep_both).unwrap();
    let versions = file_versions(&mut laptop, "report.txt").unwrap();
    assert_eq!(versions.len(), 2, "{versions:?}");
    assert_eq!((versions[0].at, versions[1].at), (T0 as i64, (T0 + 10) as i64));
    // Version one back, as one more commit another device sees.
    clock.store(T0 + 30, Ordering::SeqCst);
    restore_file_version(&mut laptop, "report.txt", versions[0].id, &mut keep_both).unwrap();
    let gone = file_versions(&mut laptop, "gone.txt").unwrap();
    assert_eq!(gone.len(), 1);
    restore_file_version(&mut laptop, "gone.txt", gone[0].id, &mut keep_both).unwrap();
    let mut phone = device(&bucket, &clock, "phone", false);
    phone.pull().unwrap();
    assert_eq!(files(&phone)["report.txt"], "version one");
    assert_eq!(files(&phone)["gone.txt"], "kept");
    assert_eq!(file_versions(&mut phone, "report.txt").unwrap().len(), 3);
    assert!(restore_file_version(&mut phone, ".azlin/policy.toml", versions[0].id, &mut keep_both).is_err());
}

// ==== K1: restoring from before a key rotation ====

/// A drive's history holds the file keys its old drive key wrapped. The rotation used to delete
/// that key, and every pointer a restore put back from before it read as "this drive's key does
/// not open it" for ever - the clean versions of the hacked flow, unreadable exactly when it
/// needs them. The key chain keeps the old key (sealed under the new one), so they open.
#[cfg(feature = "encryption")]
mod after_a_key_rotation {
    use azul_storage::{
        crypto::{
            device::{self, setup_new_drive},
            keys::RecoveryKdf,
        },
        encrypted::open_encrypted,
        keyring::MemoryKeyring,
        meta::{
            merge::keep_both, DriveBucket, IndexProfile, MemoryBucket, MetaIndexProvider, MetaRepo,
        },
        rotation::{self, Rotated},
        Drive, DriveError,
    };

    use super::*;

    const DRIVE: &str = "d_rot";

    /// A key change as the apps run it: asked (the key permit), then done.
    fn rotate(
        bucket: Arc<dyn Drive>,
        keyring: &MemoryKeyring,
        drive: &str,
        provider: &MetaIndexProvider,
        kdf: RecoveryKdf,
    ) -> Result<Rotated, DriveError> {
        let rotation = rotation::ask(bucket.as_ref(), keyring, drive)?;
        rotation::rotate(rotation.permit(), bucket, keyring, drive, provider, kdf)
    }

    /// An encrypted drive with `notes/plan.txt` written twice (two versions) and `other.txt`,
    /// on a bucket in memory; `rotate`d when asked.
    fn drive_with_history() -> (
        Arc<MemoryBucket>,
        MemoryKeyring,
        MetaIndexProvider,
        RecoveryKdf,
    ) {
        let bucket = Arc::new(MemoryBucket::new());
        let keyring = MemoryKeyring::new();
        let kdf = RecoveryKdf::with_cost(64, 1, 1).unwrap();
        let plain: Arc<dyn Drive> = bucket.clone();
        setup_new_drive(plain.as_ref(), &keyring, DRIVE, kdf.clone()).unwrap();
        let provider = MetaIndexProvider::new(IndexProfile::in_memory("Laptop"));
        let drive = open_encrypted(plain, &keyring, DRIVE, &provider).unwrap();
        drive
            .put("notes/plan.txt", b"the plan, version one")
            .unwrap();
        drive
            .put("notes/plan.txt", b"the plan, version two")
            .unwrap();
        drive.put("other.txt", b"other").unwrap();
        (bucket, keyring, provider, kdf)
    }

    fn repo_of(
        bucket: &Arc<MemoryBucket>,
        keyring: &MemoryKeyring,
    ) -> MetaRepo<DriveBucket<Arc<dyn Drive>>, azul_storage::crypto::DriveKey> {
        let key = device::load_drive_key(keyring, DRIVE).unwrap().unwrap();
        let plain: Arc<dyn Drive> = bucket.clone();
        MetaRepo::open(DriveBucket::new(plain), key, "checker", "Checker").unwrap()
    }

    #[test]
    fn a_version_restored_from_before_the_rotation_opens() {
        let (bucket, keyring, provider, kdf) = drive_with_history();
        let plain: Arc<dyn Drive> = bucket.clone();
        rotate(plain.clone(), &keyring, DRIVE, &provider, kdf).unwrap();

        // The history holds both versions as the old key wrapped them, and the head's copy as the
        // new key does.
        let mut repo = repo_of(&bucket, &keyring);
        let versions = file_versions(&mut repo, "notes/plan.txt").unwrap();
        assert_eq!(versions.len(), 3, "{versions:?}");
        restore_file_version(&mut repo, "notes/plan.txt", versions[0].id, &mut keep_both).unwrap();

        let drive = open_encrypted(plain.clone(), &keyring, DRIVE, &provider).unwrap();
        assert_eq!(
            drive.get("notes/plan.txt").unwrap(),
            b"the plan, version one",
            "the restored pointer is wrapped by the old key: the key chain opens it"
        );
        // Every way of reading it.
        let mut text = Vec::new();
        std::io::Read::read_to_end(&mut drive.open_reader("notes/plan.txt").unwrap(), &mut text)
            .unwrap();
        assert_eq!(text, b"the plan, version one");
    }

    #[test]
    fn the_drive_restored_as_of_before_the_rotation_reads_as_it_was() {
        let (bucket, keyring, provider, kdf) = drive_with_history();
        let plain: Arc<dyn Drive> = bucket.clone();
        let as_of = azul_storage::time::now_unix() as i64;
        // The rotation's commits are a later second than the writes (history is by the second).
        std::thread::sleep(Duration::from_millis(1_100));
        rotate(plain.clone(), &keyring, DRIVE, &provider, kdf).unwrap();
        let drive = open_encrypted(plain.clone(), &keyring, DRIVE, &provider).unwrap();
        drive
            .put("plan-after.txt", b"written after the rotation")
            .unwrap();
        drive
            .put("notes/plan.txt", b"changed after the rotation")
            .unwrap();
        drop(drive);

        let mut repo = repo_of(&bucket, &keyring);
        let restore = restore_drive_as_of(&mut repo, as_of, &mut keep_both)
            .unwrap()
            .expect("the drive had a state then");
        assert!(restore.restored >= 1, "{restore:?}");

        let drive = open_encrypted(plain, &keyring, DRIVE, &provider).unwrap();
        assert_eq!(
            drive.get("notes/plan.txt").unwrap(),
            b"the plan, version two",
            "the state before the rotation, its pointers wrapped by the old key"
        );
        assert_eq!(drive.get("other.txt").unwrap(), b"other");
        assert!(
            drive.get("plan-after.txt").is_err(),
            "made after: gone again"
        );
    }
}

// ==== AZ-U24: a restore never names bytes that are gone ====

/// The restore of an encrypted drive after the retention removed some of its objects: the files
/// it can bring back come back, the ones whose bytes are gone stay as they are and are said.
#[cfg(feature = "encryption")]
mod bytes_gone {
    use azul_storage::{
        crypto::DriveKey,
        encrypted::{EncryptedDrive, NameIndex},
        meta::{merge::keep_both, MemoryBucket, MetaError, MetaIndex, MetaRepo, RepoOptions},
        Drive,
    };

    use super::*;

    const DAY: u64 = 86_400;

    /// An encrypted drive over a repository, with a clock the test sets.
    struct Lab {
        bucket: MemoryBucket,
        key: DriveKey,
        clock: Arc<AtomicU64>,
        drive: EncryptedDrive<MemoryBucket>,
    }

    fn reader(clock: &Arc<AtomicU64>) -> impl Fn() -> u64 + Send + Sync + 'static {
        let clock = Arc::clone(clock);
        move || clock.load(Ordering::SeqCst)
    }

    impl Lab {
        fn new() -> Lab {
            let clock = Arc::new(AtomicU64::new(T0));
            let key = DriveKey::generate().unwrap();
            let bucket = MemoryBucket::new();
            let repo = MetaRepo::create_with(
                bucket.clone(),
                key.clone(),
                "laptop",
                "Laptop",
                &RepoOptions::default(),
            )
            .unwrap()
            .with_clock(reader(&clock));
            let index: Arc<dyn NameIndex> = Arc::new(MetaIndex::new(repo).with_poll_every(0));
            let drive = EncryptedDrive::new(bucket.clone(), key.clone(), index);
            Lab {
                bucket,
                key,
                clock,
                drive,
            }
        }

        fn at(&self, secs: u64) {
            self.clock.store(secs, Ordering::SeqCst);
        }

        /// A copy of the repository of its own that judges time by `now`.
        fn repo(&self, now: u64) -> MetaRepo<MemoryBucket, DriveKey> {
            MetaRepo::open_with(
                self.bucket.clone(),
                self.key.clone(),
                "restorer",
                "Restorer",
                &RepoOptions::default(),
            )
            .unwrap()
            .with_clock(move || now)
        }

        /// The bucket key of the object `path` names now.
        fn object_of(&self, path: &str) -> String {
            self.drive
                .entry(path)
                .unwrap()
                .bucket_object()
                .unwrap()
                .bucket_key()
        }
    }

    #[test]
    fn az_u24_restoring_a_drive_does_not_put_back_a_file_whose_bytes_are_gone() {
        let lab = Lab::new();
        lab.drive.put("keep.txt", b"keep one").unwrap();
        lab.drive.put("lost.txt", b"lost one").unwrap();
        let gone = lab.object_of("lost.txt");
        lab.at(T0 + 100);
        lab.drive.put("keep.txt", b"keep two").unwrap();
        lab.drive.put("lost.txt", b"lost two").unwrap();
        // The retention removed the first version of lost.txt.
        lab.drive.inner().delete(&gone).unwrap();

        let mut repo = lab.repo(T0 + 200);
        let restore = restore_drive_as_of(&mut repo, (T0 + 50) as i64, &mut keep_both)
            .unwrap()
            .expect("the drive had a state then");
        assert_eq!(restore.missing, vec![String::from("lost.txt")]);
        assert_eq!((restore.restored, restore.removed), (1, 0));
        assert_eq!(
            lab.drive.get("keep.txt").unwrap(),
            b"keep one",
            "what can come back does"
        );
        assert_eq!(
            lab.drive.get("lost.txt").unwrap(),
            b"lost two",
            "and a file whose old bytes are gone stays the good file it is"
        );
    }

    #[test]
    fn az_u24_a_version_whose_bytes_are_gone_is_not_offered_and_not_restored() {
        let lab = Lab::new();
        lab.drive.put("a.txt", b"version one").unwrap();
        let gone = lab.object_of("a.txt");
        lab.at(T0 + DAY);
        lab.drive.put("a.txt", b"version two").unwrap();
        lab.at(T0 + 2 * DAY);
        lab.drive.put("a.txt", b"version three").unwrap();
        let now = T0 + 3 * DAY;
        let before = file_versions(&mut lab.repo(now), "a.txt").unwrap();
        assert_eq!(before.len(), 3);
        let first = before[0].id;

        lab.drive.inner().delete(&gone).unwrap();
        let mut repo = lab.repo(now);
        let after = file_versions(&mut repo, "a.txt").unwrap();
        assert_eq!(after.len(), 2, "version one is not offered: {after:?}");
        let refused = restore_file_version(&mut repo, "a.txt", first, &mut keep_both);
        assert!(
            matches!(refused, Err(MetaError::Expired { .. })),
            "{refused:?}"
        );
        assert_eq!(
            lab.drive.get("a.txt").unwrap(),
            b"version three",
            "nothing changed"
        );
    }

    /// X-DELTA-SYNC4-8. What a restore can still bring back was derived twice: by the restore dialog
    /// (`file_versions`, per file, along the drive's first parents) and by the reclaimer to check its own
    /// window (`restorable`, the objects of the whole drive), each with its own subtraction and comparison.
    /// They ask `Retention::offers` now, and this runs both over ONE repository - a file replaced exactly at the
    /// window's edge, one replaced before it, one deleted inside it, a commit dated BEFORE the commit it follows
    /// (a device whose clock lags) and a restore - at a time when part of the history is outside the window:
    /// every version the dialog offers is named by the reclaimer's window (a delete from a window that names
    /// less would take a version the dialog still offers), and the version replaced before the window is in
    /// neither.
    #[test]
    fn x_delta_sync4_8_what_the_dialog_offers_the_reclaimers_window_names() {
        use azul_storage::{
            crypto::ObjectId as DataId,
            meta::{pointer, reclaim::restorable},
        };

        let data_of = |repo: &MetaRepo<MemoryBucket, DriveKey>,
                       blob: &azul_storage::meta::ObjectId|
         -> DataId {
            let bytes = repo
                .objects()
                .blob(blob)
                .expect("the pointer blob is there");
            pointer::decode(bytes)
                .expect("a pointer")
                .bucket_object()
                .expect("a file with an object")
        };

        let lab = Lab::new();
        lab.drive.put("a.txt", b"a, version one").unwrap();
        lab.drive.put("b.txt", b"b, version one").unwrap();
        lab.drive.put("e.txt", b"e, version one").unwrap();
        // b's first version is replaced 95 days before the check: outside the window.
        lab.at(T0 + 5 * DAY);
        lab.drive.put("b.txt", b"b, version two").unwrap();
        // a's and e's first versions are replaced exactly 90 days before it: the edge, and offered.
        lab.at(T0 + 10 * DAY);
        lab.drive.put("a.txt", b"a, version two").unwrap();
        lab.drive.put("e.txt", b"e, version two").unwrap();
        // A device whose clock lags: this commit is dated BEFORE the one it follows.
        lab.at(T0 + 8 * DAY);
        lab.drive
            .put("c.txt", b"c, written by a clock that lags")
            .unwrap();
        lab.at(T0 + 70 * DAY);
        lab.drive.delete("b.txt").unwrap();
        // A restore: a's first version is put back as the drive's.
        let a_one = file_versions(&mut lab.repo(T0 + 75 * DAY), "a.txt").unwrap()[0].id;
        restore_file_version(&mut lab.repo(T0 + 80 * DAY), "a.txt", a_one, &mut keep_both).unwrap();

        let now = T0 + 100 * DAY;
        let mut repo = lab.repo(now);
        let mut offered: Vec<(String, DataId)> = Vec::new();
        for path in ["a.txt", "b.txt", "c.txt", "e.txt"] {
            for version in file_versions(&mut repo, path).unwrap() {
                offered.push((path.to_string(), data_of(&repo, &version.id)));
            }
        }
        // The reclaimer's window, as it checks itself, over the same repository.
        let head = repo.head().expect("a head");
        let named = restorable(repo.objects(), &head, now).expect("the window can be derived");
        for (path, object) in &offered {
            assert!(
                named.contains_key(object),
                "{path}: the dialog offers a version the reclaimer's window does not name"
            );
        }
        let b_one = {
            let blob = file_versions(&mut lab.repo(T0 + 6 * DAY), "b.txt").unwrap()[0].id;
            data_of(&repo, &blob)
        };
        assert!(
            offered.iter().all(|(_, object)| *object != b_one),
            "b's first version was replaced before the window: the dialog does not offer it"
        );
        assert!(
            !named.contains_key(&b_one),
            "and the window does not name it"
        );
        // The edge is really there: e's first version, replaced exactly at the window's start and never put back,
        // is offered by the dialog AND named by the reclaimer's window. (a's first version is put back by the
        // restore above and so is offered as the head whatever the edge does: e is the one that isolates it.)
        let e_one = {
            let blob = file_versions(&mut lab.repo(T0 + 11 * DAY), "e.txt").unwrap()[0].id;
            data_of(&repo, &blob)
        };
        assert!(
            offered
                .iter()
                .any(|(path, object)| path == "e.txt" && *object == e_one),
            "the version replaced exactly at the window's edge is offered by the dialog"
        );
        assert!(
            named.contains_key(&e_one),
            "and named by the reclaimer's window"
        );
        let a_edge = data_of(&repo, &a_one);
        assert!(offered.iter().any(|(_, object)| *object == a_edge));
        assert!(named.contains_key(&a_edge));
    }

    #[test]
    fn x_ar5a_21_only_versions_inside_the_retention_are_offered() {
        let lab = Lab::new();
        lab.drive.put("a.txt", b"version one").unwrap();
        lab.at(T0 + DAY);
        lab.drive.put("a.txt", b"version two").unwrap();
        lab.at(T0 + 200 * DAY);
        lab.drive.put("a.txt", b"version three").unwrap();
        // 210 days on: version one was replaced 209 days ago, version two 10 days ago.
        let versions = file_versions(&mut lab.repo(T0 + 210 * DAY), "a.txt").unwrap();
        assert_eq!(versions.len(), 2, "{versions:?}");
        assert_eq!(
            versions[0].at,
            (T0 + DAY) as i64,
            "version two, replaced inside the window"
        );
    }
}
