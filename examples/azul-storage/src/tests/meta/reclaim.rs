//! The reclaimer (ARCH28 RF44 / AR5a-5; AZ-M2, X-AR5a-14, X-AR5a-21): the data objects of a drive
//! whose index keeps its history are deleted when - and only when - nothing that can still be
//! asked for names them. These are the tests that try to make it delete something still needed.

use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

use crate::{
    crypto::DriveKey,
    encrypted::{EncryptedDrive, IndexEntry, IndexProvider, NameIndex},
    limits::Limit,
    meta::{
        index::{listed_objects, rederive},
        keys,
        merge::keep_both,
        pointer,
        reclaim::{
            analyse, reclaim, reclaim_judged, DeleteLicence, GO_CLEAN_ROUNDS, GO_DAYS, GO_DEVICES,
        },
        shadow::Ledger,
        Caps, Change, DriveBucket, GoEvidence, Hold, IndexProfile, Maintenance, MemoryBucket,
        MetaError, MetaIndex, MetaIndexProvider, MetaRepo, NotYet, ReclaimMode, ReclaimOptions,
        ReclaimReport, RepoOptions, Retention, ServerClock, Standing, RETENTION,
    },
    packing::PackOptions,
    BatchPut, ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo, Precondition,
};

const T0: u64 = 1_800_000_000;
const DAY: u64 = 86_400;

type Index = MetaIndex<MemoryBucket, DriveKey>;
type Repo = MetaRepo<MemoryBucket, DriveKey>;

fn clock_reader(clock: &Arc<AtomicU64>) -> impl Fn() -> u64 + Send + Sync + 'static {
    let clock = Arc::clone(clock);
    move || clock.load(Ordering::SeqCst)
}

/// An encrypted drive over a repository, with a clock the test sets.
struct Lab {
    bucket: MemoryBucket,
    key: DriveKey,
    clock: Arc<AtomicU64>,
    drive: EncryptedDrive<MemoryBucket>,
}

impl Lab {
    fn new() -> Lab {
        Lab::with(|drive| drive)
    }

    /// A lab whose drive is shaped by `shape` (packing, for one).
    fn with(
        shape: impl FnOnce(EncryptedDrive<MemoryBucket>) -> EncryptedDrive<MemoryBucket>,
    ) -> Lab {
        Lab::starting_at(T0, shape)
    }

    /// [`Lab::with`] whose clock starts at `base`.
    fn starting_at(
        base: u64,
        shape: impl FnOnce(EncryptedDrive<MemoryBucket>) -> EncryptedDrive<MemoryBucket>,
    ) -> Lab {
        let clock = Arc::new(AtomicU64::new(base));
        let key = DriveKey::generate().unwrap();
        let bucket = MemoryBucket::new();
        let repo = Repo::create_with(
            bucket.clone(),
            key.clone(),
            "laptop",
            "Laptop",
            &RepoOptions::default(),
        )
        .unwrap()
        .with_clock(clock_reader(&clock));
        let index: Arc<dyn NameIndex> = Arc::new(Index::new(repo).with_poll_every(0));
        let drive = shape(EncryptedDrive::new(bucket.clone(), key.clone(), index));
        Lab {
            bucket,
            key,
            clock,
            drive,
        }
    }

    /// The date the next commits get.
    fn at(&self, secs: u64) {
        self.clock.store(secs, Ordering::SeqCst);
    }

    /// A copy of the repository of its own, as the maintenance round opens one, judging time by
    /// the lab's clock.
    fn maintainer(&self) -> Repo {
        Repo::open_with(
            self.bucket.clone(),
            self.key.clone(),
            "maintenance",
            "Maintenance",
            &RepoOptions::default(),
        )
        .unwrap()
        .with_clock(clock_reader(&self.clock))
    }

    fn data_objects(&self) -> Vec<String> {
        self.bucket
            .objects()
            .into_iter()
            .map(|(key, _)| key)
            .filter(|key| key.starts_with("data/"))
            .collect()
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

fn options(mode: ReclaimMode) -> ReclaimOptions {
    ReclaimOptions {
        mode,
        server_clock: ServerClock::Unchecked,
        ..ReclaimOptions::shadow()
    }
}

#[test]
fn az_m2_the_reclaimer_in_shadow_mode_says_what_it_would_delete_and_deletes_nothing() {
    let lab = Lab::new();
    lab.drive.put("a.txt", b"version one").unwrap();
    let stored = lab
        .drive
        .entry("a.txt")
        .unwrap()
        .object
        .unwrap()
        .stored_size;
    lab.at(T0 + DAY);
    lab.drive.put("a.txt", b"version two").unwrap();
    lab.at(T0 + 100 * DAY);

    let report = run(&mut lab.maintainer(), &options(ReclaimMode::Shadow)).unwrap();
    assert_eq!(
        (report.candidates, report.deleted),
        (1, 0),
        "{}",
        report.line()
    );
    assert_eq!(report.candidate_bytes, stored);
    assert!(
        report
            .line()
            .starts_with("reclaim (shadow): would delete 1 objects"),
        "{}",
        report.line()
    );
    assert_eq!(lab.data_objects().len(), 2, "shadow mode deletes nothing");
    let repo = lab.maintainer();
    assert!(
        repo.store().manifest().unwrap().reclaimed.is_none(),
        "and writes nothing: the manifest has no mark"
    );
}

#[test]
fn az_m2_a_version_inside_the_window_stays_and_one_past_it_goes_and_the_drive_still_reads() {
    let lab = Lab::new();
    lab.drive.put("a.txt", b"version one").unwrap();
    lab.at(T0 + DAY);
    lab.drive.put("a.txt", b"version two").unwrap();

    // Fifty days on, version one can still be restored: both commits are inside the window.
    lab.at(T0 + 50 * DAY);
    let report = run(&mut lab.maintainer(), &options(ReclaimMode::Delete)).unwrap();
    assert_eq!(
        (report.candidates, report.deleted),
        (0, 0),
        "{}",
        report.line()
    );
    assert_eq!(lab.data_objects().len(), 2);

    // A hundred days on, nothing can ask for it any more: it goes, and the quota frees.
    lab.at(T0 + 100 * DAY);
    let report = run(&mut lab.maintainer(), &options(ReclaimMode::Delete)).unwrap();
    assert_eq!((report.deleted, report.failed), (1, 0), "{}", report.line());
    assert_eq!(lab.data_objects().len(), 1);
    assert_eq!(lab.drive.get("a.txt").unwrap(), b"version two");
    assert!(
        lab.maintainer()
            .store()
            .manifest()
            .unwrap()
            .reclaimed
            .is_some(),
        "the run recorded how far it got"
    );

    // The next run starts after what was dealt with.
    let again = run(&mut lab.maintainer(), &options(ReclaimMode::Delete)).unwrap();
    assert_eq!((again.candidates, again.scanned, again.deleted), (0, 0, 0));
}

#[test]
fn az_m2_an_object_the_head_still_names_under_another_path_is_never_deleted() {
    let lab = Lab::new();
    lab.drive.put("a.txt", b"shared bytes").unwrap();
    lab.drive.copy("a.txt", "b.txt").unwrap();
    lab.at(T0 + DAY);
    lab.drive.put("a.txt", b"replaced").unwrap();
    lab.at(T0 + 100 * DAY);
    let report = run(&mut lab.maintainer(), &options(ReclaimMode::Delete)).unwrap();
    assert_eq!(
        (report.candidates, report.deleted),
        (0, 0),
        "{}",
        report.line()
    );
    assert_eq!(lab.drive.get("b.txt").unwrap(), b"shared bytes");
}

#[test]
fn x_ar5a_14_a_pack_lives_while_any_file_of_it_is_named_and_goes_when_none_is() {
    let lab = Lab::with(|drive| drive.with_packing(PackOptions::default()));
    let small: [(&str, &[u8]); 2] = [
        ("p1.txt", b"first small file"),
        ("p2.txt", b"second small file"),
    ];
    let items: Vec<BatchPut<'_>> = small.iter().map(|(k, b)| BatchPut::new(k, b)).collect();
    for result in lab.drive.put_many(&items) {
        result.unwrap();
    }
    assert_eq!(lab.data_objects().len(), 1, "one pack for both");
    lab.at(T0 + DAY);
    lab.drive.delete("p1.txt").unwrap();

    // The head still names the pack through p2: it stays however old the commits are.
    lab.at(T0 + 100 * DAY);
    let report = run(&mut lab.maintainer(), &options(ReclaimMode::Delete)).unwrap();
    assert_eq!(
        (report.candidates, report.deleted),
        (0, 0),
        "{}",
        report.line()
    );
    assert_eq!(lab.drive.get("p2.txt").unwrap(), b"second small file");

    // The last file leaves; its commit is inside the window: the history still names the pack.
    lab.drive.delete("p2.txt").unwrap();
    let report = run(&mut lab.maintainer(), &options(ReclaimMode::Delete)).unwrap();
    assert_eq!(
        (report.candidates, report.deleted),
        (0, 0),
        "{}",
        report.line()
    );
    assert_eq!(lab.data_objects().len(), 1);

    // Past the retention nothing names it: the pack goes.
    lab.at(T0 + 300 * DAY);
    let report = run(&mut lab.maintainer(), &options(ReclaimMode::Delete)).unwrap();
    assert_eq!(report.deleted, 1, "{}", report.line());
    assert_eq!(lab.data_objects().len(), 0);
}

#[test]
fn az_m2_a_pointer_the_reclaimer_cannot_read_stops_it_and_nothing_is_deleted() {
    let lab = Lab::new();
    lab.drive.put("a.txt", b"version one").unwrap();
    lab.at(T0 + DAY);
    lab.drive.put("a.txt", b"version two").unwrap();
    // A newer app wrote a pointer of a format this one does not know: what it names is unknown.
    let mut repo = lab.maintainer();
    let blob = repo.write_blob(b"azlin-pointer 9\nsomething newer\n");
    repo.commit(
        &[Change::Put {
            path: String::from("future.txt"),
            id: blob,
        }],
        "a newer app",
        &mut keep_both,
    )
    .unwrap();
    lab.at(T0 + 100 * DAY);
    let report = run(&mut lab.maintainer(), &options(ReclaimMode::Delete)).unwrap();
    assert!(
        matches!(report.stopped, Some(NotYet::Unreadable { .. })),
        "{}",
        report.line()
    );
    assert_eq!(report.deleted, 0);
    assert_eq!(
        lab.data_objects().len(),
        2,
        "nothing was decided from what cannot be read"
    );
}

#[test]
fn az_m2_a_drive_that_moved_between_the_read_and_the_fence_loses_nothing() {
    let lab = Lab::new();
    lab.drive.put("a.txt", b"version one").unwrap();
    lab.at(T0 + DAY);
    lab.drive.put("a.txt", b"version two").unwrap();
    lab.at(T0 + 100 * DAY);
    let mut repo = lab.maintainer();
    // Another device publishes just as the reclaimer is about to fence: its reads are old.
    let mut phone = Repo::open_with(
        lab.bucket.clone(),
        lab.key.clone(),
        "phone",
        "Phone",
        &RepoOptions::default(),
    )
    .unwrap()
    .with_clock(clock_reader(&lab.clock));
    // A file that names no data object: a pointer all the same.
    let blob = phone.write_blob(&pointer::encode(&IndexEntry {
        size: 0,
        modified: None,
        object: None,
    }));
    lab.bucket.before_next_replace(keys::MANIFEST, move || {
        phone
            .commit(
                &[Change::Put {
                    path: String::from("note.txt"),
                    id: blob,
                }],
                "phone",
                &mut keep_both,
            )
            .unwrap();
    });
    let report = run(&mut repo, &options(ReclaimMode::Delete)).unwrap();
    assert_eq!(report.stopped, Some(NotYet::Moved), "{}", report.line());
    assert_eq!(report.deleted, 0);
    assert_eq!(lab.data_objects().len(), 2);
    // The next run, from fresh reads, goes through.
    let again = run(&mut lab.maintainer(), &options(ReclaimMode::Delete)).unwrap();
    assert_eq!(again.deleted, 1, "{}", again.line());
}

#[test]
fn az_m2_a_clock_the_storage_does_not_agree_with_deletes_nothing() {
    let lab = Lab::new();
    lab.drive.put("a.txt", b"version one").unwrap();
    lab.at(T0 + DAY);
    lab.drive.put("a.txt", b"version two").unwrap();
    lab.at(T0 + 100 * DAY);
    let required = ReclaimOptions {
        mode: ReclaimMode::Delete,
        ..ReclaimOptions::shadow()
    };

    // The storage does not tell the time: the device's word alone is not believed.
    let report = run(&mut lab.maintainer(), &required).unwrap();
    assert!(
        matches!(report.stopped, Some(NotYet::Clock { server: None, .. })),
        "{}",
        report.line()
    );
    assert_eq!(lab.data_objects().len(), 2);

    // The storage's clock is a week off this device's.
    lab.bucket
        .set_server_clock(move || T0 + 100 * DAY + 7 * DAY);
    let report = run(&mut lab.maintainer(), &required).unwrap();
    assert!(
        matches!(
            report.stopped,
            Some(NotYet::Clock {
                server: Some(_),
                ..
            })
        ),
        "{}",
        report.line()
    );
    assert_eq!(lab.data_objects().len(), 2);

    // The two agree: it goes.
    lab.bucket.set_server_clock(move || T0 + 100 * DAY + 5);
    let report = run(&mut lab.maintainer(), &required).unwrap();
    assert_eq!(report.deleted, 1, "{}", report.line());
}

#[test]
fn az_u22_a_delete_the_bucket_refuses_stops_the_mark_and_the_next_run_deletes_it() {
    let lab = Lab::new();
    lab.drive.put("a.txt", b"version one").unwrap();
    let doomed = lab.object_of("a.txt");
    lab.at(T0 + DAY);
    lab.drive.put("a.txt", b"version two").unwrap();
    lab.at(T0 + 100 * DAY);
    lab.bucket.fail_next_write(
        &doomed,
        MetaError::Drive(crate::DriveError::Transport(String::from("reset"))),
    );
    let report = run(&mut lab.maintainer(), &options(ReclaimMode::Delete)).unwrap();
    assert_eq!((report.deleted, report.failed), (0, 1), "{}", report.line());
    assert!(
        lab.maintainer()
            .store()
            .manifest()
            .unwrap()
            .reclaimed
            .is_none(),
        "the mark did not move past an object that is still there"
    );
    let again = run(&mut lab.maintainer(), &options(ReclaimMode::Delete)).unwrap();
    assert_eq!((again.deleted, again.failed), (1, 0), "{}", again.line());
    assert_eq!(lab.data_objects().len(), 1);
}

#[test]
fn az_m2_the_caps_end_a_run_and_the_kill_switch_stops_it() {
    let lab = Lab::new();
    for day in 0..4 {
        lab.at(T0 + day * DAY);
        lab.drive
            .put("a.txt", format!("version {day}").as_bytes())
            .unwrap();
    }
    lab.at(T0 + 100 * DAY);
    let capped = ReclaimOptions {
        caps: Caps {
            commits: 2,
            ..Caps::default()
        },
        ..options(ReclaimMode::Shadow)
    };
    let report = run(&mut lab.maintainer(), &capped).unwrap();
    assert!(report.capped, "{}", report.line());
    assert_eq!(report.scanned, 2);
    let off = run(&mut lab.maintainer(), &options(ReclaimMode::Off)).unwrap();
    assert_eq!(off.stopped, Some(NotYet::Off));
    assert_eq!(off.line(), "reclaim: switched off");
}

#[test]
fn az_m2_a_commit_stamped_in_the_future_does_not_make_a_history_look_older() {
    let lab = Lab::new();
    lab.drive.put("a.txt", b"version one").unwrap();
    // A device whose clock is a year ahead replaces the file; the next commit, on a right clock,
    // is dated before it.
    lab.at(T0 + 365 * DAY);
    lab.drive.put("a.txt", b"version two").unwrap();
    lab.at(T0 + 2 * DAY);
    lab.drive.put("a.txt", b"version three").unwrap();
    lab.at(T0 + 100 * DAY);
    let report = run(&mut lab.maintainer(), &options(ReclaimMode::Delete)).unwrap();
    // Version two's commit counts as dated a day ahead of now, and version three's is never
    // earlier than its parent's: both are inside the window, so version one (the state the window
    // began with) stays too.
    assert_eq!(
        (report.candidates, report.deleted),
        (0, 0),
        "{}",
        report.line()
    );
    assert_eq!(lab.data_objects().len(), 3);
}

#[test]
fn x_ar5a_14_the_index_names_what_the_head_and_the_window_name_and_proves_only_the_rest() {
    let lab = Lab::new();
    lab.drive.put("a.txt", b"version one").unwrap();
    let first = lab.drive.entry("a.txt").unwrap().object.unwrap().id;
    lab.at(T0 + DAY);
    lab.drive.put("a.txt", b"version two").unwrap();
    let second = lab.drive.entry("a.txt").unwrap().object.unwrap().id;
    let stranger = crate::crypto::ObjectId::generate().unwrap();

    lab.at(T0 + 50 * DAY);
    let index = lab.drive.index();
    let live = index
        .live_objects(T0 + 50 * DAY)
        .unwrap()
        .expect("a repository keeps history");
    assert!(
        live.names(&first) && live.names(&second),
        "both versions are inside the window"
    );
    assert!(live.prove_unnamed(&first).is_err());

    lab.at(T0 + 100 * DAY);
    let live = index.live_objects(T0 + 100 * DAY).unwrap().unwrap();
    assert!(live.names(&second));
    assert!(!live.names(&first));
    assert!(live.prove_unnamed(&first).is_ok(), "past the window");
    assert!(live.prove_unnamed(&second).is_err(), "the head names it");
    assert!(
        live.prove_unnamed(&stranger).is_ok(),
        "an upload nothing ever named"
    );
}

#[test]
fn az_m2_the_analysis_reads_a_synthetic_history_and_agrees_with_the_run() {
    // The pure core, on objects built by hand: what the run does is this function's answer.
    let lab = Lab::new();
    lab.drive.put("a.txt", b"version one").unwrap();
    lab.at(T0 + DAY);
    lab.drive.put("a.txt", b"version two").unwrap();
    lab.at(T0 + 100 * DAY);
    let mut repo = lab.maintainer();
    repo.pull().unwrap();
    repo.fetch_all().unwrap();
    let head = repo.head().unwrap();
    let analysis = analyse(
        repo.objects(),
        &head,
        T0 + 100 * DAY,
        &RETENTION,
        None,
        &Caps::default(),
        false,
    )
    .unwrap();
    assert_eq!((analysis.commits, analysis.window_commits), (2, 0));
    assert_eq!((analysis.live.len(), analysis.candidates.len()), (1, 1));
    assert_eq!(
        analysis.window_start,
        T0 + 100 * DAY - RETENTION.versions.as_secs()
    );
}

#[test]
fn az_m2_the_providers_maintenance_round_ends_with_a_shadow_round_and_tells_what_it_found() {
    let bucket: Arc<dyn Drive> = Arc::new(MemoryBucket::new());
    let key = DriveKey::generate().unwrap();
    let provider = MetaIndexProvider::new(IndexProfile::in_memory("Laptop"));
    assert!(provider.last_reclaim().is_none(), "no round yet");
    let index = provider
        .open_index("drive-1", Arc::clone(&bucket), &key)
        .unwrap();
    let drive = EncryptedDrive::new(Arc::clone(&bucket), key.clone(), index);
    drive.put("a.txt", b"version one").unwrap();
    drive.put("a.txt", b"version two").unwrap();
    provider
        .maintain(Arc::clone(&bucket), &key, &Maintenance::default())
        .unwrap()
        .expect("no other device holds the lease");
    let report = provider
        .last_reclaim()
        .expect("the maintenance round ended with a reclaimer round")
        .expect("it read the drive");
    assert_eq!(
        report.mode,
        ReclaimMode::Shadow,
        "the first release ships in shadow mode"
    );
    assert_eq!((report.commits, report.deleted), (2, 0));
    assert_eq!(drive.get("a.txt").unwrap(), b"version two");

    // The kill switch: no round at all.
    let off = MetaIndexProvider::new(IndexProfile {
        reclaim: ReclaimOptions {
            mode: ReclaimMode::Off,
            ..ReclaimOptions::shadow()
        },
        ..IndexProfile::in_memory("Laptop")
    });
    off.maintain(Arc::clone(&bucket), &key, &Maintenance::default())
        .unwrap();
    assert!(off.last_reclaim().is_none());
}

/// A drive that keeps the key of every read of it.
struct GetLog {
    inner: MemoryBucket,
    gets: std::sync::Mutex<Vec<String>>,
}

impl GetLog {
    fn over(inner: MemoryBucket) -> Arc<GetLog> {
        Arc::new(GetLog {
            inner,
            gets: std::sync::Mutex::new(Vec::new()),
        })
    }

    fn note(&self, key: &str) {
        self.gets.lock().unwrap().push(key.to_string());
    }

    fn forget(&self) {
        self.gets.lock().unwrap().clear();
    }

    fn gets(&self) -> Vec<String> {
        self.gets.lock().unwrap().clone()
    }
}

impl Drive for GetLog {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.inner.list(request)
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.note(key);
        Drive::get(&self.inner, key)
    }

    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.note(key);
        Drive::get_range(&self.inner, key, range)
    }

    fn get_limited(&self, key: &str, limit: Limit) -> Result<Vec<u8>, DriveError> {
        self.note(key);
        Drive::get_limited(&self.inner, key, limit)
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        Drive::put(&self.inner, key, bytes)
    }

    fn delete(&self, key: &str) -> Result<(), DriveError> {
        Drive::delete(&self.inner, key)
    }

    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        Drive::head(&self.inner, key)
    }

    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        Drive::put_if(&self.inner, key, bytes, condition)
    }
}

/// X-DELTA-AZDRIVE-SYNC-5. The maintenance round of every device ran a reclaimer round that read the
/// whole history of the drive, every six hours, on every computer. Now one round per drive per day,
/// from whichever device is first: the second maintenance round reads none of it; a day on, the
/// ledger says a round is due again. The first round is on the ledger as clean: the head listed by
/// the app's own listing agreed with the live set.
#[test]
fn x_delta_azdrive_sync_5_a_maintenance_round_runs_the_reclaimer_once_a_day_per_drive() {
    // A bucket that tells the time, as every bucket the apps open does: the provider's clock policy
    // wants the bucket's word before a round counts (X-DELTA-SYNC2-2).
    let memory = MemoryBucket::new();
    memory.set_server_clock(crate::time::now_unix);
    let bucket: Arc<dyn Drive> = Arc::new(memory);
    let key = DriveKey::generate().unwrap();
    let provider = MetaIndexProvider::new(IndexProfile::in_memory("Laptop"));
    let index = provider
        .open_index("drive-1", Arc::clone(&bucket), &key)
        .unwrap();
    let drive = EncryptedDrive::new(Arc::clone(&bucket), key.clone(), index);
    drive.put("a.txt", b"version one").unwrap();
    drive.put("a.txt", b"version two").unwrap();
    let dbucket = DriveBucket::new(Arc::clone(&bucket));
    let ledger = || Ledger::read(&dbucket, &key).unwrap().0;
    let rules = Maintenance::default();

    provider
        .maintain(Arc::clone(&bucket), &key, &rules)
        .unwrap()
        .expect("no other device holds the lease");
    let report = provider
        .last_reclaim()
        .expect("the first round is due")
        .expect("it read the drive");
    assert_eq!(
        report.rederived,
        Some(true),
        "the head listed again agrees with the live set: {}",
        report.line()
    );
    assert!(report.line().contains("agrees"), "{}", report.line());
    assert_eq!(ledger().rounds.len(), 1, "and the round is written down");
    assert!(ledger().rounds[0].clean, "{:?}", ledger().rounds[0]);
    // X-DELTA-SYNC4-7: it says which analysis made it.
    assert_eq!(
        ledger().rounds[0].rules,
        crate::meta::shadow::ANALYSIS_RULES,
        "{:?}",
        ledger().rounds[0]
    );

    // Minutes later: the next maintenance round - of this device or another - runs none.
    provider
        .maintain(Arc::clone(&bucket), &key, &rules)
        .unwrap()
        .expect("no other device holds the lease");
    assert!(
        provider.last_reclaim().is_none(),
        "no round was due: it used to read the whole history again at every maintenance round"
    );
    assert_eq!(ledger().rounds.len(), 1);

    // The ledger says the round was two days ago: due again.
    let (mut old, version) = Ledger::read(&dbucket, &key).unwrap();
    for round in &mut old.rounds {
        round.at -= 2 * DAY;
    }
    assert!(old.write(&dbucket, &key, version.as_deref()).unwrap());
    // Another run of the app: a provider that has run no round. This process's own memory of its
    // last round is a second limit beside the ledger, for a bucket that cannot keep one.
    let later = MetaIndexProvider::new(IndexProfile::in_memory("Phone"));
    later
        .maintain(Arc::clone(&bucket), &key, &rules)
        .unwrap()
        .expect("no other device holds the lease");
    assert!(later.last_reclaim().is_some(), "a day on, a round is due");
    assert_eq!(ledger().rounds.len(), 2);

    // The same process asked again at once runs none, even when the ledger says a round is due.
    let (mut old, version) = Ledger::read(&dbucket, &key).unwrap();
    for round in &mut old.rounds {
        round.at -= 2 * DAY;
    }
    assert!(old.write(&dbucket, &key, version.as_deref()).unwrap());
    later
        .maintain(Arc::clone(&bucket), &key, &rules)
        .unwrap()
        .expect("no other device holds the lease");
    assert!(
        later.last_reclaim().is_none(),
        "this process ran a round of the drive a moment ago: a ledger that is old (or that the \
         bucket cannot keep) does not make it read the history at every maintenance round"
    );
    assert_eq!(ledger().rounds.len(), 2, "and wrote none");
}

/// X-DELTA-AZDRIVE-SYNC-5. A drive whose index is over the budget of a run is "too big to judge": the
/// manifest says the packs' bytes, the round reads none of them (no pack of the repository was
/// asked for), nothing is decided, a run configured to delete says why it did not, and the round is
/// on the ledger as one that was not clean.
#[test]
fn x_delta_azdrive_sync_5_a_drive_over_the_budget_is_too_big_to_judge_and_no_pack_is_fetched() {
    let lab = Lab::with(|drive| drive.with_packing(PackOptions::default()));
    for i in 0..4 {
        lab.drive
            .put(&format!("f{i}.txt"), format!("file {i}").as_bytes())
            .unwrap();
    }
    let spy = GetLog::over(lab.bucket.clone());
    let bucket: Arc<dyn Drive> = spy.clone();
    for mode in [ReclaimMode::Shadow, ReclaimMode::Delete] {
        let provider = MetaIndexProvider::new(IndexProfile {
            reclaim: ReclaimOptions {
                mode,
                caps: Caps {
                    pack_bytes: 1,
                    ..Caps::default()
                },
                ..ReclaimOptions::shadow()
            },
            ..IndexProfile::in_memory("Laptop")
        });
        spy.forget();
        let report = provider.reclaim(Arc::clone(&bucket), &lab.key).unwrap();
        assert!(
            matches!(report.stopped, Some(NotYet::TooBig { .. })),
            "{}",
            report.line()
        );
        assert!(
            report.line().contains("too big to judge"),
            "{}",
            report.line()
        );
        assert_eq!(
            (report.commits, report.candidates, report.deleted),
            (0, 0, 0),
            "nothing was read, nothing decided"
        );
        let packs: Vec<String> = spy
            .gets()
            .into_iter()
            .filter(|key| key.starts_with(keys::WAL_DIR))
            .collect();
        assert!(packs.is_empty(), "no pack was fetched: {packs:?}");
        assert_eq!(
            report.held,
            (mode == ReclaimMode::Delete).then_some(Hold::TooBig),
            "a run configured to delete says why it did not"
        );
    }
    let (ledger, _) = Ledger::read(&DriveBucket::new(Arc::clone(&bucket)), &lab.key).unwrap();
    assert_eq!(ledger.rounds.len(), 2);
    assert!(
        ledger
            .rounds
            .iter()
            .all(|round| !round.clean && round.unclean.contains("too big to judge")),
        "{:?}",
        ledger.rounds
    );
}

/// X-DELTA-AZDRIVE-SYNC-6. A run configured to delete, on a drive that has history old enough to
/// reclaim and that no shadow round has watched, deletes nothing and says what it lacks. The old
/// code deleted at the first round: the mode was the only switch.
#[test]
fn x_delta_azdrive_sync_6_a_run_configured_to_delete_waits_for_the_evidence_of_its_shadow_rounds() {
    let base = crate::time::now_unix() - 200 * DAY;
    let lab = Lab::starting_at(base, |drive| drive);
    lab.drive.put("a.txt", b"version one").unwrap();
    let first = lab.object_of("a.txt");
    lab.at(base + DAY);
    lab.drive.put("a.txt", b"version two").unwrap();

    let provider = MetaIndexProvider::new(IndexProfile {
        reclaim: ReclaimOptions {
            mode: ReclaimMode::Delete,
            server_clock: ServerClock::Unchecked,
            ..ReclaimOptions::shadow()
        },
        ..IndexProfile::in_memory("Laptop")
    });
    let bucket: Arc<dyn Drive> = Arc::new(lab.bucket.clone());
    let report = provider.reclaim(Arc::clone(&bucket), &lab.key).unwrap();
    assert_eq!(
        report.candidates,
        1,
        "the first version is older than the window and the head does not name it: {}",
        report.line()
    );
    assert_eq!(report.deleted, 0, "{}", report.line());
    assert_eq!(
        report.mode,
        ReclaimMode::Shadow,
        "it went on as a shadow run"
    );
    assert_eq!(
        report.held,
        Some(Hold::NoRestoreSource),
        "an app with no restore announcement is the first thing lacking: {}",
        report.line()
    );
    assert!(report.line().contains("held back"), "{}", report.line());
    assert!(
        lab.data_objects().contains(&first),
        "the first version's object is still in the bucket"
    );
}

/// X-DELTA-AZDRIVE-SYNC-5 (d). The bucket's clock is compared with the time of day the analysis began
/// at, carried forward by the monotonic clock (X-DELTA-SYNC4-4), not with the one that began it as it
/// stood: a big drive's analysis takes longer than the skew, and the check could never pass.
#[test]
fn x_delta_azdrive_sync_5_the_clock_check_is_made_against_the_clock_of_the_moment_not_of_the_analysis(
) {
    let lab = Lab::new();
    lab.drive.put("a.txt", b"version one").unwrap();
    lab.at(T0 + DAY);
    lab.drive.put("a.txt", b"version two").unwrap();
    lab.at(T0 + 100 * DAY);
    let server = Arc::clone(&lab.clock);
    lab.bucket
        .set_server_clock(move || server.load(Ordering::SeqCst));
    let options = ReclaimOptions {
        mode: ReclaimMode::Delete,
        server_clock: ServerClock::Required,
        ..ReclaimOptions::shadow()
    };
    let slow = Arc::clone(&lab.clock);
    let report = reclaim_judged(&mut lab.maintainer(), &options, &mut |_, _| {
        // The analysis took two hours (the bucket's clock, the device's and the monotonic one all
        // went on: a clock that went on alone is a step, X-DELTA-SYNC4-4).
        slow.fetch_add(2 * 3_600, Ordering::SeqCst);
        crate::time::advance_monotonic(2 * 3_600);
        Ok(DeleteLicence::lab())
    })
    .unwrap();
    assert_eq!(
        report.stopped,
        None,
        "the lease was written just now by both clocks: {}",
        report.line()
    );
    assert_eq!(report.deleted, 1, "{}", report.line());
}

/// X-DELTA-AZDRIVE-SYNC-6. The reclaimer reads a pointer strictly: a line it does not know could name
/// another object, and an object nothing is seen to name is deleted. The old reader skipped the line,
/// found the first version released, and deleted it.
#[test]
fn x_delta_azdrive_sync_6_a_pointer_with_a_line_the_reclaimer_does_not_know_stops_the_run() {
    let lab = Lab::new();
    lab.drive.put("a.txt", b"version one").unwrap();
    lab.at(T0 + DAY);
    lab.drive.put("a.txt", b"version two").unwrap();
    // A later format of the app adds a line to a pointer that is otherwise this one's.
    let mut repo = lab.maintainer();
    let head = repo.head().unwrap();
    let existing = repo.file_at(&head, "a.txt").unwrap().unwrap();
    let mut bytes = repo.objects().blob(&existing).unwrap().to_vec();
    bytes.extend_from_slice(b"also-names 0123456789abcdef\n");
    let blob = repo.write_blob(&bytes);
    repo.commit(
        &[Change::Put {
            path: String::from("later.txt"),
            id: blob,
        }],
        "a later format of the app",
        &mut keep_both,
    )
    .unwrap();
    lab.at(T0 + 100 * DAY);
    let report = run(&mut lab.maintainer(), &options(ReclaimMode::Delete)).unwrap();
    assert!(
        matches!(&report.stopped, Some(NotYet::Unreadable { reason, .. }) if reason.contains("does not know")),
        "{}",
        report.line()
    );
    assert_eq!(report.deleted, 0);
    assert_eq!(lab.data_objects().len(), 2, "nothing was decided");
    // The app's own reader still reads the pointer (it skips what it does not know).
    assert!(pointer::decode(&bytes).is_ok());
    assert!(pointer::decode_strict(&bytes).is_err());
}

/// X-DELTA-AZDRIVE-SYNC-6. A report carries what the owner reads: the digest of the candidates (the
/// same for two runs over one history, another when a candidate is added), the times of the commits
/// that released them, and the devices that committed inside the window.
#[test]
fn x_delta_azdrive_sync_6_a_report_carries_the_digest_the_releasing_commits_and_the_devices() {
    let lab = Lab::new();
    lab.drive.put("a.txt", b"version one").unwrap();
    lab.drive.put("c.txt", b"another file").unwrap();
    lab.at(T0 + DAY);
    lab.drive.put("a.txt", b"version two").unwrap();
    // A second device commits inside the window (a copy of the pointer of c.txt).
    lab.at(T0 + 2 * DAY);
    let mut other = lab.maintainer();
    let head = other.head().unwrap();
    let pointer_blob = other.file_at(&head, "c.txt").unwrap().unwrap();
    other
        .commit(
            &[Change::Put {
                path: String::from("b.txt"),
                id: pointer_blob,
            }],
            "the maintenance device",
            &mut keep_both,
        )
        .unwrap();
    let report = run(&mut lab.maintainer(), &options(ReclaimMode::Shadow)).unwrap();
    assert_eq!(report.devices, 2, "the laptop and the maintenance device");
    assert_eq!(report.candidates, 0, "everything is inside the window");
    assert_eq!(report.releasing, None);

    lab.at(T0 + 100 * DAY);
    let one = run(&mut lab.maintainer(), &options(ReclaimMode::Shadow)).unwrap();
    let two = run(&mut lab.maintainer(), &options(ReclaimMode::Shadow)).unwrap();
    assert_eq!(one.candidates, 1, "{}", one.line());
    assert_eq!(
        one.digest, two.digest,
        "the same history says the same digest"
    );
    assert_eq!(one.digest.len(), 64);
    assert_eq!(
        one.releasing,
        Some((T0 + DAY, T0 + DAY)),
        "released by the commit of the second version"
    );
    assert!(
        one.line().contains(&one.digest[..16]),
        "the line carries the digest: {}",
        one.line()
    );

    // Another version leaves the window: the candidates are not the same set, and say so.
    lab.at(T0 + 101 * DAY);
    lab.drive.put("a.txt", b"version three").unwrap();
    lab.at(T0 + 300 * DAY);
    let three = run(&mut lab.maintainer(), &options(ReclaimMode::Shadow)).unwrap();
    assert_eq!(three.candidates, 2, "{}", three.line());
    assert_ne!(one.digest, three.digest);
}

/// X-DELTA-AZDRIVE-SYNC-6. The independent listing of the head is the check the shadow rounds are
/// accepted by: it agrees with the live set the analysis made, and a listing that names an object
/// the live set lacks - the analysis missed a file the head names - does not, so the round is not
/// clean. A listing made at another revision than the analysis proves nothing.
#[test]
fn x_delta_azdrive_sync_6_the_listing_of_the_head_can_disagree_with_the_live_set() {
    use std::collections::HashSet;
    let lab = Lab::new();
    lab.drive.put("a.txt", b"version one").unwrap();
    lab.at(T0 + DAY);
    lab.drive.put("a.txt", b"version two").unwrap();
    lab.at(T0 + 100 * DAY);

    // The app's own listing of a copy, and the analysis of the same copy at the same revision.
    let mut repo = lab.maintainer();
    repo.pull().unwrap();
    repo.fetch_all().unwrap();
    let revision = repo.store().state().revision;
    let head = repo.head().unwrap();
    let mut analysis = analyse(
        repo.objects(),
        &head,
        T0 + 100 * DAY,
        &RETENTION,
        None,
        &Caps::default(),
        false,
    )
    .unwrap();
    analysis.revision = revision;
    let index = MetaIndex::new(repo).with_poll_every(u64::MAX);
    let listed = listed_objects(&index).unwrap();
    assert_eq!(listed.1, revision);
    assert_eq!(listed.0.len(), 1, "the head names one object");
    assert_eq!(
        rederive(Some(&listed), &analysis),
        Some(true),
        "the two derivations agree"
    );

    // An object the head names and the live set lacks (here: a name no history knows).
    let stranger = crate::crypto::ObjectId::from_hex(&"ab".repeat(16)).unwrap();
    let mut wrong: HashSet<crate::crypto::ObjectId> = listed.0.clone();
    wrong.insert(stranger);
    assert_eq!(
        rederive(Some(&(wrong, revision)), &analysis),
        Some(false),
        "the analysis would have deleted a file the head names"
    );
    // Another revision, or no listing at all: not shown either way.
    assert_eq!(
        rederive(Some(&(listed.0.clone(), revision + 1)), &analysis),
        None
    );
    assert_eq!(rederive(None, &analysis), None);
}

/// X-DELTA-AZDRIVE-SYNC-6. The first round that deletes took at most 50,000 objects of any size off a
/// drive. Now the scan also ends at the commit that passes a byte budget, and says it stopped at a
/// cap.
#[test]
fn x_delta_azdrive_sync_6_a_run_stops_at_the_commit_that_passes_its_byte_budget() {
    let lab = Lab::new();
    lab.drive.put("a.txt", &vec![1u8; 4_000]).unwrap();
    lab.at(T0 + DAY);
    lab.drive.put("a.txt", &vec![2u8; 4_000]).unwrap();
    lab.at(T0 + 2 * DAY);
    lab.drive.put("a.txt", &vec![3u8; 4_000]).unwrap();
    lab.at(T0 + 300 * DAY);
    let whole = run(&mut lab.maintainer(), &options(ReclaimMode::Shadow)).unwrap();
    assert_eq!(
        (whole.candidates, whole.capped),
        (2, false),
        "{}",
        whole.line()
    );

    let small = ReclaimOptions {
        caps: Caps {
            candidate_bytes: 1,
            ..Caps::default()
        },
        ..options(ReclaimMode::Shadow)
    };
    let capped = run(&mut lab.maintainer(), &small).unwrap();
    assert_eq!(
        (capped.candidates, capped.capped),
        (1, true),
        "the first version's release passed the budget: {}",
        capped.line()
    );
    assert!(capped.line().contains("stopped at a cap"));
}

/// X-DELTA-AZDRIVE-SYNC-5 (b). In shadow mode the pack upkeep of a drive that keeps its history
/// deletes nothing, so what it asks the drive is what the head names (the provider hands its index
/// no history budget in shadow mode): a retired pack stays retired and in the bucket, as before.
#[test]
fn x_delta_azdrive_sync_5_the_upkeep_of_a_shadow_drive_asks_for_the_head_and_deletes_nothing() {
    let options = PackOptions {
        retired_grace_secs: 0,
        ..PackOptions::default()
    };
    let key = DriveKey::generate().unwrap();
    let bucket: Arc<dyn Drive> = Arc::new(MemoryBucket::new());
    let provider = MetaIndexProvider::new(IndexProfile::in_memory("Laptop"));
    let index = provider
        .open_index("drive-1", Arc::clone(&bucket), &key)
        .unwrap();
    let drive = EncryptedDrive::new(Arc::clone(&bucket), key.clone(), index)
        .with_packing(options)
        .with_reclaim(ReclaimMode::Shadow);
    let items = [
        BatchPut::new("p1.txt", b"first small file"),
        BatchPut::new("p2.txt", b"second small file"),
    ];
    for result in drive.put_many(&items) {
        result.unwrap();
    }
    drive.delete("p1.txt").unwrap();
    drive.delete("p2.txt").unwrap();
    let now = crate::time::now_unix();
    drive.repack(now).unwrap();
    assert_eq!(
        drive.retired_packs().len(),
        1,
        "the emptied pack is retired"
    );
    assert_eq!(
        drive.collect_retired(now + 1).unwrap(),
        0,
        "shadow: nothing goes"
    );
    assert_eq!(drive.retired_packs().len(), 1, "and it stays retired");
}

/// X-DELTA-AZDRIVE-SYNC-5 (b). What the upkeep of a drive asks of the history is bounded and
/// private to what it needs: a drive whose reclaimer may delete asks for the whole history within the
/// budget of a run (the old trees come into the copy), one over the budget is an error that says "too
/// big to judge" (nothing is read), and a drive whose reclaimer deletes nothing asks for the head alone
/// - the app's lazy copy of it did not become a full copy of the history for it.
#[test]
fn x_delta_azdrive_sync_5_the_history_the_upkeep_asks_for_is_read_within_the_budget_or_not_at_all()
{
    let lab = Lab::new();
    lab.drive.put("a.txt", b"version one").unwrap();
    lab.at(T0 + DAY);
    lab.drive.put("a.txt", b"version two").unwrap();
    // The tree of the state before the second version: no longer the head's.
    let full = lab.maintainer();
    let history = full.history().unwrap();
    let old_tree = full.objects().commit(&history[1].0).unwrap().tree;
    assert_ne!(Some(old_tree), full.root().unwrap(), "an old state");

    let lazy = |budget: Option<u64>| {
        let repo = Repo::open_with(
            lab.bucket.clone(),
            lab.key.clone(),
            "reader",
            "Reader",
            &RepoOptions {
                lazy: true,
                ..RepoOptions::default()
            },
        )
        .unwrap()
        .with_clock(clock_reader(&lab.clock));
        MetaIndex::new(repo)
            .with_poll_every(0)
            .with_history_budget(budget)
    };
    let knows = |index: &Index| index.with_repo(|repo| repo.objects().contains(&old_tree));

    // The head alone: the old state is not read into the copy.
    let head = lazy(None);
    let live = head.live_objects(T0 + 100 * DAY).unwrap().unwrap();
    let second = lab.drive.entry("a.txt").unwrap().bucket_object().unwrap();
    assert!(live.names(&second) && live.names_in_head(&second));
    assert!(
        !knows(&head),
        "the history was not read into the app's lazy copy (it was, whole, for every round)"
    );

    // Over the budget: refused before the history is read.
    let tight = lazy(Some(1));
    let refused = tight.live_objects(T0 + 100 * DAY);
    assert!(
        matches!(&refused, Err(DriveError::Protocol(why)) if why.contains("too big to judge")),
        "{refused:?}"
    );
    assert!(!knows(&tight));

    // Within it: the whole history.
    let whole = lazy(Some(u64::MAX));
    assert!(whole.live_objects(T0 + 100 * DAY).unwrap().is_some());
    assert!(knows(&whole), "the old state is in the copy now");
}

/// The bucket key of the pack that holds a small file.
fn pack_of(lab: &Lab, path: &str) -> String {
    lab.object_of(path)
}

#[test]
fn x_ar5a_14_re_encrypting_the_packs_of_a_drive_that_keeps_its_history_deletes_none_of_them() {
    let lab = Lab::with(|drive| drive.with_packing(PackOptions::default()));
    let small: [(&str, &[u8]); 2] = [
        ("p1.txt", b"first small file"),
        ("p2.txt", b"second small file"),
    ];
    let items: Vec<BatchPut<'_>> = small.iter().map(|(k, b)| BatchPut::new(k, b)).collect();
    for result in lab.drive.put_many(&items) {
        result.unwrap();
    }
    let old_pack = pack_of(&lab, "p1.txt");
    let now = crate::time::now_unix();

    // Every pack is written anew; the old one is the history of both files.
    let report = lab.drive.reencrypt_packs(now + 10, now).unwrap();
    assert_eq!(report.files_moved, 2, "{report:?}");
    assert_ne!(
        pack_of(&lab, "p1.txt"),
        old_pack,
        "the files moved to a new pack"
    );
    assert!(
        lab.data_objects().contains(&old_pack),
        "the old pack stays: a version restored from the history still finds its bytes"
    );
    assert_eq!(lab.drive.get("p2.txt").unwrap(), b"second small file");
    assert_eq!(
        lab.drive.retired_packs().len(),
        1,
        "it waits retired for its retention"
    );
}

/// The standing of a drive whose shadow rounds showed everything the switch to deleting asks for,
/// judged at the very time the upkeep asks: what the provider's own answer
/// ([`crate::meta::MetaIndexProvider`]: the evidence of its last judged round) is, stood in for. A
/// drive configured to delete deletes only when the one gate says so of one (X-DELTA-SYNC2-2).
fn proven() -> crate::encrypted::ReclaimStanding {
    Arc::new(|now| {
        Some(Standing {
            at: now,
            evidence: GoEvidence {
                clean_rounds: GO_CLEAN_ROUNDS,
                days: GO_DAYS,
                devices: GO_DEVICES,
                drive_devices: GO_DEVICES,
                rederived_live_equal: Some(true),
                window_rederived: Some(true),
                clock_agrees: Some(true),
                byte_budget_ok: true,
                floor_known: true,
                restore_source: true,
            },
        })
    })
}

/// A run of the reclaimer as the machinery's own tests make it - the fences, the batches, the lease,
/// the caps: one configured to delete is handed a licence of the test's own, one that is not is
/// handed what holds it back. WHEN a licence is made is the gate's tests' (`ask_retired_delete`).
fn run(repo: &mut Repo, options: &ReclaimOptions) -> Result<ReclaimReport, MetaError> {
    let gate = if options.mode == ReclaimMode::Delete {
        Ok(DeleteLicence::lab())
    } else {
        Err(Hold::Configured)
    };
    reclaim(repo, options, gate)
}

/// A drive that packs, deletes by its upkeep, and has retired the pack of two small files that were
/// deleted: the pack is named by the first commit only (and the grace of a retired pack is
/// `grace`).
fn lab_with_a_retired_pack(grace: u64) -> (Lab, String) {
    let options = PackOptions {
        retired_grace_secs: grace,
        ..PackOptions::default()
    };
    let lab = Lab::with(|drive| {
        drive
            .with_packing(options)
            .with_reclaim(ReclaimMode::Delete)
            .with_reclaim_standing(proven())
    });
    let small: [(&str, &[u8]); 2] = [
        ("p1.txt", b"first small file"),
        ("p2.txt", b"second small file"),
    ];
    let items: Vec<BatchPut<'_>> = small.iter().map(|(k, b)| BatchPut::new(k, b)).collect();
    for result in lab.drive.put_many(&items) {
        result.unwrap();
    }
    let pack = pack_of(&lab, "p1.txt");
    lab.at(T0 + DAY);
    lab.drive.delete("p1.txt").unwrap();
    lab.drive.delete("p2.txt").unwrap();
    // The pack is emptied: the upkeep retires it.
    lab.drive.repack(T0 + 2 * DAY).unwrap();
    assert_eq!(lab.drive.retired_packs().len(), 1);
    (lab, pack)
}

#[test]
fn x_ar5a_14_a_retired_pack_waits_as_long_as_a_version_can_be_restored() {
    let (lab, pack) = lab_with_a_retired_pack(RETENTION.versions.as_secs());
    // A month on (the grace used to be thirty days): a version of p1 can still be restored from
    // the history, so the pack is still there.
    assert_eq!(lab.drive.collect_retired(T0 + 33 * DAY).unwrap(), 0);
    assert!(lab.data_objects().contains(&pack));
    // Three months on nothing can ask for it: it goes.
    assert_eq!(lab.drive.collect_retired(T0 + 93 * DAY).unwrap(), 1);
    assert!(!lab.data_objects().contains(&pack));
}

#[test]
fn az_m10_a_retired_pack_stays_while_the_history_inside_the_window_names_it() {
    // The grace is over at once; the drive's history still names the pack. The head alone,
    // from a copy, used to decide: nothing in the head names it, and it went.
    let (lab, pack) = lab_with_a_retired_pack(0);
    assert_eq!(lab.drive.collect_retired(T0 + 50 * DAY).unwrap(), 0);
    assert!(lab.data_objects().contains(&pack));
    assert_eq!(lab.drive.retired_packs().len(), 1, "and it stays retired");
    // Past the retention it goes.
    assert_eq!(lab.drive.collect_retired(T0 + 200 * DAY).unwrap(), 1);
    assert!(!lab.data_objects().contains(&pack));
}

/// X-DELTA-AZDRIVE-SYNC-7. Pack A is retired on day 2 and pack B on day 12, the grace is ninety
/// days. On day 93 A is due and the window (the last ninety days) does not name it; B is not due
/// and the window does name it. The collection used to leave B out of the list it writes back
/// (`continue` without `keep.push`) because "the drive names it": the first pack that was due took
/// the bookkeeping of every pack the window still names with it.
#[test]
fn x_delta_azdrive_sync_7_the_first_pack_due_does_not_take_the_list_of_the_later_ones_with_it() {
    let options = PackOptions {
        retired_grace_secs: 90 * DAY,
        ..PackOptions::default()
    };
    let lab = Lab::with(|drive| {
        drive
            .with_packing(options)
            .with_reclaim(ReclaimMode::Delete)
            .with_reclaim_standing(proven())
    });
    let put_pair = |a: (&str, &[u8]), b: (&str, &[u8])| {
        let items = [BatchPut::new(a.0, a.1), BatchPut::new(b.0, b.1)];
        for result in lab.drive.put_many(&items) {
            result.unwrap();
        }
    };
    put_pair(
        ("a1.txt", b"first small file of A"),
        ("a2.txt", b"second small file of A"),
    );
    let pack_a = pack_of(&lab, "a1.txt");
    lab.at(T0 + DAY);
    lab.drive.delete("a1.txt").unwrap();
    lab.drive.delete("a2.txt").unwrap();
    lab.drive.repack(T0 + 2 * DAY).unwrap();

    lab.at(T0 + 10 * DAY);
    put_pair(
        ("b1.txt", b"first small file of B"),
        ("b2.txt", b"second small file of B"),
    );
    let pack_b = pack_of(&lab, "b1.txt");
    lab.at(T0 + 11 * DAY);
    lab.drive.delete("b1.txt").unwrap();
    lab.drive.delete("b2.txt").unwrap();
    lab.drive.repack(T0 + 12 * DAY).unwrap();
    assert_eq!(lab.drive.retired_packs().len(), 2, "both packs are retired");

    assert_eq!(
        lab.drive.collect_retired(T0 + 93 * DAY).unwrap(),
        1,
        "A's grace is over and no state inside the window names it"
    );
    assert!(!lab.data_objects().contains(&pack_a), "A went");
    assert!(
        lab.data_objects().contains(&pack_b),
        "B is not due and the window names it"
    );
    assert_eq!(
        lab.drive
            .retired_packs()
            .iter()
            .map(|(id, _)| id.bucket_key())
            .collect::<Vec<_>>(),
        vec![pack_b.clone()],
        "B is still on the list of the retired packs (it was dropped with the first one due)"
    );
    // The window has left it by day 200, and then it goes.
    assert_eq!(lab.drive.collect_retired(T0 + 200 * DAY).unwrap(), 1);
    assert!(!lab.data_objects().contains(&pack_b));
    assert!(lab.drive.retired_packs().is_empty());
}

/// X-DELTA-AZDRIVE-SYNC-7 (the other meaning of "the drive names it", which the fix keeps). A pack
/// the HEAD names again - a restore brought one of its files back - is no longer retired and is
/// not deleted, whatever its grace.
#[test]
fn x_delta_azdrive_sync_7_a_retired_pack_the_head_names_again_is_no_longer_retired() {
    let (lab, pack) = lab_with_a_retired_pack(0);
    let mut repo = lab.maintainer();
    let history = repo.history().unwrap();
    // The oldest state that has the file: its pointer names the pack.
    let first = history
        .iter()
        .rev()
        .map(|(id, _)| *id)
        .find(|id| repo.file_at(id, "p1.txt").unwrap().is_some())
        .expect("a state that has p1.txt");
    lab.at(T0 + 50 * DAY);
    repo.restore("p1.txt", &first, &mut keep_both).unwrap();

    assert_eq!(lab.drive.collect_retired(T0 + 200 * DAY).unwrap(), 0);
    assert!(
        lab.data_objects().contains(&pack),
        "the head names it again: it stays"
    );
    assert!(
        lab.drive.retired_packs().is_empty(),
        "and it is no longer retired"
    );
}

#[test]
fn x_ar5a_14_a_drive_in_shadow_mode_keeps_what_its_upkeep_would_delete() {
    let lab = Lab::with(|drive| drive.with_packing(PackOptions::default()));
    let small: [(&str, &[u8]); 1] = [("p1.txt", b"first small file")];
    let items: Vec<BatchPut<'_>> = small.iter().map(|(k, b)| BatchPut::new(k, b)).collect();
    lab.drive.put_many(&items).pop().unwrap().unwrap();
    let pack = pack_of(&lab, "p1.txt");
    lab.at(T0 + DAY);
    lab.drive.delete("p1.txt").unwrap();
    lab.drive.repack(T0 + 2 * DAY).unwrap();
    lab.at(T0 + 200 * DAY);
    assert_eq!(
        lab.drive.collect_retired(T0 + 200 * DAY).unwrap(),
        0,
        "shadow: nothing goes"
    );
    assert!(lab.data_objects().contains(&pack));
    assert_eq!(lab.drive.retired_packs().len(), 1, "and it stays retired");
}

#[test]
fn az_m10_an_object_whose_change_may_have_landed_is_cleared_up_from_the_drive_not_from_a_copy() {
    // The change fails with an error that says nothing about whether it landed: the object stays
    // and is written down.
    let lab = Lab::with(|drive| {
        drive
            .with_reclaim(ReclaimMode::Delete)
            .with_reclaim_standing(proven())
    });
    lab.bucket.fail_next_write(
        keys::MANIFEST,
        MetaError::Drive(crate::DriveError::Transport(String::from("reset"))),
    );
    let put = lab.drive.put("a.txt", b"the file");
    assert!(
        matches!(put, Err(crate::DriveError::Transport(_))),
        "{put:?}"
    );
    assert_eq!(
        lab.data_objects().len(),
        1,
        "the object stays: it may be named"
    );

    // Not stale yet: left alone, its writer may still commit it.
    let now = crate::time::now_unix();
    assert_eq!(lab.drive.recover_packs(now).unwrap(), 0);
    assert_eq!(lab.data_objects().len(), 1);
    // Later the drive, read fresh and whole, names nothing of it: garbage that costs quota.
    assert_eq!(lab.drive.recover_packs(now + 3_600).unwrap(), 1);
    assert_eq!(lab.data_objects().len(), 0);
}

#[test]
fn az_m10_an_object_the_drive_names_after_all_is_never_deleted_by_the_upkeep() {
    // Another device's index change landed with its answer lost: the drive names the object.
    let lab = Lab::with(|drive| {
        drive
            .with_reclaim(ReclaimMode::Delete)
            .with_reclaim_standing(proven())
    });
    lab.drive.put("a.txt", b"the file").unwrap();
    let object = lab.drive.entry("a.txt").unwrap().object.unwrap().id;
    // The journal has it as in flight (this is what a write that could not tell leaves).
    lab.drive.note_in_flight(&object);
    let now = crate::time::now_unix();
    assert_eq!(
        lab.drive.recover_packs(now + 3_600).unwrap(),
        1,
        "cleared from the journal"
    );
    assert_eq!(
        lab.data_objects().len(),
        1,
        "and the file it is of is still there"
    );
    assert_eq!(lab.drive.get("a.txt").unwrap(), b"the file");
}

#[test]
fn az_m2_a_drive_in_shadow_mode_keeps_an_object_nothing_names_and_its_journal_entry() {
    let lab = Lab::new();
    lab.bucket.fail_next_write(
        keys::MANIFEST,
        MetaError::Drive(crate::DriveError::Transport(String::from("reset"))),
    );
    let _ = lab.drive.put("a.txt", b"the file");
    let now = crate::time::now_unix();
    assert_eq!(
        lab.drive.recover_packs(now + 3_600).unwrap(),
        0,
        "shadow: nothing is cleared"
    );
    assert_eq!(lab.data_objects().len(), 1);
    // It is still written down: a drive switched to deleting clears it up.
    assert_eq!(lab.drive.recover_packs(now + 7_200).unwrap(), 0);
}

#[test]
fn az_m2_re_encrypting_a_drive_with_history_keeps_its_old_versions_until_the_user_confirms_a_purge()
{
    use crate::rotation::ReencryptState;
    use crate::tests::asked::reencrypt_pass_of_parts;

    let bucket = Arc::new(MemoryBucket::new());
    bucket.set_server_clock(crate::time::now_unix);
    let key = DriveKey::generate().unwrap();
    let provider = MetaIndexProvider::new(IndexProfile::in_memory("Laptop"));
    let raw: Arc<dyn Drive> = bucket.clone();
    let index = provider
        .open_index("drive-1", Arc::clone(&raw), &key)
        .unwrap();
    let drive = EncryptedDrive::new(Arc::clone(&raw), key.clone(), index);
    drive.put("a.txt", b"version one").unwrap();
    drive.put("a.txt", b"version two").unwrap();
    let objects = |bucket: &MemoryBucket| -> usize {
        bucket
            .objects()
            .into_iter()
            .filter(|(key, _)| key.starts_with("data/"))
            .count()
    };
    assert_eq!(objects(&bucket), 2);

    // Every file is written into a new object; the old ones are the history, and stay.
    let mut state = ReencryptState::new(crate::time::now_unix() + 10);
    assert!(reencrypt_pass_of_parts(&key, &drive, &mut state, &mut |_| Ok(()), &|| false).unwrap());
    assert_eq!(state.done, 1);
    assert_eq!(
        objects(&bucket),
        3,
        "version one, version two and the re-encrypted file"
    );
    assert!(
        state.purge_confirmation().is_none(),
        "nobody asked for an erasure"
    );
    assert_eq!(drive.get("a.txt").unwrap(), b"version two");

    // The user confirmed that the old versions are erased too: they go, the file stays.
    state.confirm_purge(crate::time::now_unix());
    assert_eq!(
        ReencryptState::from_json(&state.to_json()).unwrap(),
        state,
        "the confirmation survives a restart"
    );
    let confirmation = state.purge_confirmation().expect("they confirmed");
    let report = provider.purge("drive-1", raw, &key, &confirmation).unwrap();
    assert_eq!((report.deleted, report.failed), (2, 0), "{}", report.line());
    assert_eq!(objects(&bucket), 1);
    assert_eq!(drive.get("a.txt").unwrap(), b"version two");
}

/// A number from the environment, `default` without it.
fn env_number(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|text| text.replace('_', "").parse().ok())
        .unwrap_or(default)
}

/// The pointer file of a new data object, as the repository keeps one (nothing else is written).
fn pointer_blob(objects: &mut crate::meta::Objects, serial: u64) -> crate::meta::ObjectId {
    let mut raw = [0u8; crate::crypto::OBJECT_ID_LEN];
    raw[..8].copy_from_slice(&serial.to_le_bytes());
    let entry = IndexEntry {
        size: 100,
        modified: Some(serial),
        object: Some(crate::encrypted::StoredObject {
            id: crate::crypto::ObjectId(raw),
            stored_size: 100,
            blake3: [0; 32],
            wrapped_key: crate::crypto::WrappedKey::from_bytes(
                &[0; crate::crypto::WrappedKey::LEN],
            )
            .unwrap(),
            compressed: false,
            packed: None,
        }),
    };
    objects.write_blob(&pointer::encode(&entry))
}

/// E8 (ARCH28 section 5.4): how big a history the reclaimer's walk can take. A synthetic
/// repository - `E8_FILES` files in folders, `E8_COMMITS` commits over 100 days that each replace
/// `E8_CHANGES` files - built as objects in memory, then the walk the reclaimer makes at its first
/// run: the commit graph, the state of the window, the trees of every old commit. The owner runs
/// it on the 8 GB machine before the deletion of the second release is switched on:
///
/// ```text
/// E8_FILES=100000 E8_COMMITS=1000000 E8_CHANGES=3 cargo test -p azul-storage \
///     --features encryption --release -- --ignored --nocapture e8_history_size_for_reclaim
/// ```
///
/// The defaults (10 000 files, 20 000 commits) are a smoke run of a few seconds. It prints the
/// time of each phase and the sizes; nothing is asserted but that the walk finishes and sees what
/// was built.
#[test]
#[ignore = "X-EDGE-C-2: E8: the owner's machine; the command is in the doc comment"]
fn e8_history_size_for_reclaim() {
    use std::time::Instant;

    use crate::meta::{
        objects::{Commit, Signature},
        tree::{apply, Change},
        Objects,
    };

    let files = env_number("E8_FILES", 10_000);
    let commits = env_number("E8_COMMITS", 20_000);
    let changes = env_number("E8_CHANGES", 3).max(1);
    let span = 100 * DAY;
    let started = Instant::now();
    let mut objects = Objects::new();
    let mut serial = 0u64;
    let folders = (files as f64).sqrt().ceil().max(1.0) as usize;
    let path_of = |i: usize| format!("d{:05}/f{:07}", i % folders, i);
    let mut initial = Vec::with_capacity(files);
    for i in 0..files {
        serial += 1;
        initial.push(Change::Put {
            path: path_of(i),
            id: pointer_blob(&mut objects, serial),
        });
    }
    let mut root = apply(&mut objects, None, &initial).unwrap();
    let signature = |time: u64| Signature {
        name: String::from("e8"),
        email: String::from("e8"),
        time: i64::try_from(time).unwrap_or(0),
        offset_minutes: 0,
    };
    let mut head = objects.write_commit(&Commit {
        tree: root,
        parents: Vec::new(),
        author: signature(T0),
        committer: signature(T0),
        message: String::from("e8"),
    });
    for n in 0..commits {
        let time = T0 + (n as u64 + 1) * span / (commits as u64 + 1);
        let mut edits = Vec::with_capacity(changes);
        for k in 0..changes {
            serial += 1;
            edits.push(Change::Put {
                path: path_of((n * 7_919 + k * 104_729) % files.max(1)),
                id: pointer_blob(&mut objects, serial),
            });
        }
        root = apply(&mut objects, Some(&root), &edits).unwrap();
        head = objects.write_commit(&Commit {
            tree: root,
            parents: vec![head],
            author: signature(time),
            committer: signature(time),
            message: String::from("e8"),
        });
    }
    println!(
        "E8 built: {files} files, {} commits, {} git objects, {:?}",
        commits + 1,
        objects.len(),
        started.elapsed()
    );

    // What `MetaRepo::history` costs: the first-parent chain, every commit decoded.
    let walk = Instant::now();
    let mut chain = 0usize;
    let mut next = Some(head);
    while let Some(id) = next {
        let commit = objects.commit(&id).unwrap();
        next = commit.parents.first().copied();
        chain += 1;
    }
    println!("E8 history: {chain} commits in {:?}", walk.elapsed());

    let now = T0 + span + DAY;
    let run = Instant::now();
    let analysis = analyse(
        &objects,
        &head,
        now,
        &RETENTION,
        None,
        &Caps {
            commits: usize::MAX,
            objects: usize::MAX,
            candidate_bytes: u64::MAX,
            batch: 1_000,
            pack_bytes: u64::MAX,
        },
        false,
    )
    .unwrap();
    println!(
        "E8 analysis: {} commits ({} in the window, {} read), {} live objects, {} to reclaim, {:?}",
        analysis.commits,
        analysis.window_commits,
        analysis.scanned,
        analysis.live.len(),
        analysis.candidates.len(),
        run.elapsed()
    );
    assert_eq!(analysis.commits, commits + 1);
    assert!(
        analysis.live.len() >= files,
        "every file of the head is live"
    );
}

// ==== X-DELTA-SYNC2-2 (a): the evidence gate stops every deleter, not only the reclaimer's own round ====

/// A drive opened the way an app opens one - `open_encrypted` with a provider configured to delete,
/// packing small files or not. Returns the provider (its proof is what the drive asks), the drive and
/// the bucket.
fn configured_to_delete(
    name: &str,
    packing: bool,
) -> (
    MetaIndexProvider,
    EncryptedDrive<Arc<dyn Drive>>,
    Arc<MemoryBucket>,
) {
    use crate::{
        crypto::{device::setup_new_drive, keys::RecoveryKdf},
        encrypted::open_encrypted,
        keyring::MemoryKeyring,
    };
    let bucket = Arc::new(MemoryBucket::new());
    let keyring = MemoryKeyring::new();
    setup_new_drive(
        bucket.as_ref(),
        &keyring,
        name,
        RecoveryKdf::with_cost(64, 1, 1).unwrap(),
    )
    .unwrap();
    let provider = MetaIndexProvider::new(IndexProfile {
        packing,
        reclaim: ReclaimOptions {
            mode: ReclaimMode::Delete,
            ..ReclaimOptions::shadow()
        },
        ..IndexProfile::in_memory("Laptop")
    });
    let plain: Arc<dyn Drive> = bucket.clone();
    let drive = open_encrypted(plain, &keyring, name, &provider).unwrap();
    (provider, drive, bucket)
}

/// The data objects the bucket holds.
fn data_objects_of(bucket: &MemoryBucket) -> Vec<String> {
    bucket
        .objects()
        .into_iter()
        .map(|(key, _)| key)
        .filter(|key| key.starts_with("data/"))
        .collect()
}

/// X-DELTA-SYNC2-2 (a). `go()` gated only the reclaimer's own round: the pack upkeep
/// (`collect_retired`, `recover_packs`) deleted in the mode the provider was CONFIGURED with, so a
/// provider switched to deleting made every drive delete its retired packs at the next maintenance
/// round with none of the rounds, days, devices or footing the gate promises. A drive configured to
/// delete deletes now only while the provider's proof - the verdict of its last judged round - says
/// the evidence is there. RED on the old code: the retired pack is deleted by `collect_retired`.
#[test]
fn x_delta_sync2_2_a_provider_configured_to_delete_keeps_every_due_pack_without_the_evidence() {
    let (provider, drive, bucket) = configured_to_delete("d_sync2_2", true);
    // Two small files in one pack, both deleted: the emptied pack is retired.
    let items = [
        BatchPut::new("p1.txt", b"first small file"),
        BatchPut::new("p2.txt", b"second small file"),
    ];
    for result in drive.put_many(&items) {
        result.unwrap();
    }
    drive.delete("p1.txt").unwrap();
    drive.delete("p2.txt").unwrap();
    drive.repack(crate::time::now_unix()).unwrap();
    let retired = drive.retired_packs();
    assert_eq!(retired.len(), 1, "the emptied pack is retired");
    let pack = retired[0].0.bucket_key();
    let far = crate::time::now_unix() + 200 * DAY;
    assert!(
        provider
            .reclaim_standing("d_sync2_2")
            .expect("the provider offers its standings")(far)
        .is_none(),
        "no round has judged the drive: no standing to ask the gate with"
    );
    assert_eq!(
        drive.collect_retired(far).unwrap(),
        0,
        "the pack is due and no state names it, but nothing was proven: it stays"
    );
    assert_eq!(drive.retired_packs().len(), 1, "and it stays retired");
    assert!(bucket.head(&pack).is_ok(), "in the bucket");
}

/// The other deleter of the upkeep: an object whose writer stopped between the upload and the
/// journal's end, named by nothing, is cleared up from the bucket only with the same proof.
#[test]
fn x_delta_sync2_2_a_provider_configured_to_delete_clears_no_object_in_flight_without_the_evidence()
{
    let (_provider, drive, bucket) = configured_to_delete("d_sync2_2b", false);
    // The change that names the file fails with an answer that says nothing of whether it landed:
    // the object stays in the bucket, written down as in flight, named by nothing.
    bucket.fail_next_write(
        keys::MANIFEST,
        MetaError::Drive(DriveError::Transport(String::from("reset"))),
    );
    let put = drive.put("a.txt", b"the file");
    assert!(matches!(put, Err(DriveError::Transport(_))), "{put:?}");
    assert_eq!(data_objects_of(&bucket).len(), 1, "the object stays");
    // Stale, and the drive read whole names nothing of it: garbage - but no round proved the
    // history that could still name it, so it stays in the bucket and in the journal.
    let far = crate::time::now_unix() + 3_600;
    assert_eq!(drive.recover_packs(far).unwrap(), 0, "nothing was proven");
    assert_eq!(
        data_objects_of(&bucket).len(),
        1,
        "the object is still in the bucket"
    );
}

/// X-DELTA-SYNC2-2. THE gate every deleter of data the history may still name asks: a table of its
/// answers. An index with no history has no window; a shadow run and the kill switch never delete; a
/// run configured to delete needs a standing - none is "not judged", one older than the rounds'
/// schedule or stamped in the future is "stale" - and then the evidence's own table (`go`): the first
/// thing it lacks holds the delete back, and only evidence in which nothing is lacking gives the
/// licence. The reclaimer's own run and the pack upkeep both end here.
#[test]
fn x_delta_sync2_2_the_one_gate_answers_every_deleter_from_the_standing_and_the_evidence_table() {
    use crate::meta::reclaim::{ask_retired_delete, History, STANDING_FRESH_SECS};
    let at = T0;
    let standing = |change: &dyn Fn(&mut GoEvidence)| {
        let mut evidence = (proven())(at).expect("a standing").evidence;
        change(&mut evidence);
        Standing { at, evidence }
    };
    let ask = |mode: ReclaimMode, standing: Option<&Standing>, now: u64| {
        ask_retired_delete(
            History::Kept {
                mode,
                standing,
                switch: crate::meta::Switch::Clear,
            },
            now,
        )
    };
    let full = standing(&|_| {});
    // An index that keeps no history: nothing is retired, the head's word is all there is.
    assert!(ask_retired_delete(History::None, at).is_ok());
    // The kill switch and the shadow run never delete, whatever stands behind them.
    assert_eq!(
        ask(ReclaimMode::Off, Some(&full), at).unwrap_err(),
        Hold::Off
    );
    assert_eq!(
        ask(ReclaimMode::Shadow, Some(&full), at).unwrap_err(),
        Hold::Configured
    );
    // A run configured to delete, with evidence in which nothing is lacking: the licence.
    assert!(ask(ReclaimMode::Delete, Some(&full), at).is_ok());
    assert!(ask(ReclaimMode::Delete, Some(&full), at + STANDING_FRESH_SECS).is_ok());
    // None, old, future: the rounds are not there to be believed.
    assert_eq!(
        ask(ReclaimMode::Delete, None, at).unwrap_err(),
        Hold::NotJudged
    );
    assert_eq!(
        ask(
            ReclaimMode::Delete,
            Some(&full),
            at + STANDING_FRESH_SECS + 1
        )
        .unwrap_err(),
        Hold::Stale
    );
    assert_eq!(
        ask(ReclaimMode::Delete, Some(&full), at - 1).unwrap_err(),
        Hold::Stale,
        "a standing stamped in the future is stale, not fresh"
    );
    // The evidence's own table, through the gate: one thing lacking, that thing is said.
    let rows: Vec<(&str, Standing, Hold)> = vec![
        (
            "one clean round short",
            standing(&|e| e.clean_rounds = GO_CLEAN_ROUNDS - 1),
            Hold::Rounds {
                have: GO_CLEAN_ROUNDS - 1,
                need: GO_CLEAN_ROUNDS,
            },
        ),
        (
            "one day short",
            standing(&|e| e.days = GO_DAYS - 1),
            Hold::Days {
                have: GO_DAYS - 1,
                need: GO_DAYS,
            },
        ),
        (
            "two of three devices",
            standing(&|e| e.devices = GO_DEVICES - 1),
            Hold::Devices {
                have: GO_DEVICES - 1,
                need: GO_DEVICES,
            },
        ),
        (
            "the head was not listed again",
            standing(&|e| e.rederived_live_equal = None),
            Hold::NotRederived,
        ),
        (
            "the head's listing names what the live set lacks",
            standing(&|e| e.rederived_live_equal = Some(false)),
            Hold::Differs,
        ),
        (
            "no restore source",
            standing(&|e| e.restore_source = false),
            Hold::NoRestoreSource,
        ),
        (
            "no floor",
            standing(&|e| e.floor_known = false),
            Hold::NoFloor,
        ),
        (
            "over the budget",
            standing(&|e| e.byte_budget_ok = false),
            Hold::TooBig,
        ),
    ];
    for (what, lacking, hold) in &rows {
        assert_eq!(
            ask(ReclaimMode::Delete, Some(lacking), at).unwrap_err(),
            *hold,
            "{what}"
        );
    }
}

/// X-DELTA-SYNC2-2. The provider keeps the standing of the last round it judged of each drive and
/// hands it out per drive; a round that proved nothing leaves none. The gate - not the provider -
/// decides what a standing is worth.
#[test]
fn x_delta_sync2_2_the_provider_holds_one_standing_per_drive_and_forgets_it_with_a_round_that_proved_nothing(
) {
    let provider = MetaIndexProvider::new(IndexProfile {
        reclaim: ReclaimOptions {
            mode: ReclaimMode::Delete,
            ..ReclaimOptions::shadow()
        },
        ..IndexProfile::in_memory("Laptop")
    });
    let standing_of = |drive: &str| provider.reclaim_standing(drive).expect("offered")(T0);
    assert!(standing_of("d_a").is_none(), "no round yet");
    let said = (proven())(T0).expect("a standing");
    provider.note_standing(Some("d_a"), said);
    assert_eq!(standing_of("d_a"), Some(said));
    assert!(
        standing_of("d_b").is_none(),
        "another drive's round is not this one's"
    );
    assert!(
        standing_of("").is_none(),
        "the round of no drive is no drive's"
    );
    provider.forget_standing(Some("d_a"));
    assert!(
        standing_of("d_a").is_none(),
        "a round that proved nothing leaves none"
    );
}

/// X-DELTA-SYNC2-2. The boundary the brief names, through the drive's own upkeep and the gate it
/// asks: with the evidence one clean round short (6 of 7) a drive configured to delete deletes
/// nothing - its due pack stays retired and in the bucket - and with the 7th it deletes. Nothing but
/// the count of clean rounds changes between the two.
#[test]
fn x_delta_sync2_2_the_pack_upkeep_deletes_with_seven_clean_rounds_and_nothing_with_six() {
    let options = PackOptions {
        retired_grace_secs: 0,
        ..PackOptions::default()
    };
    let rounds = Arc::new(AtomicU64::new(u64::from(GO_CLEAN_ROUNDS) - 1));
    let counted = Arc::clone(&rounds);
    let lab = Lab::with(|drive| {
        drive
            .with_packing(options)
            .with_reclaim(ReclaimMode::Delete)
            .with_reclaim_standing(Arc::new(move |now| {
                let mut standing = (proven())(now)?;
                standing.evidence.clean_rounds =
                    u32::try_from(counted.load(Ordering::SeqCst)).unwrap_or(u32::MAX);
                Some(standing)
            }))
    });
    let items = [
        BatchPut::new("p1.txt", b"first small file"),
        BatchPut::new("p2.txt", b"second small file"),
    ];
    for result in lab.drive.put_many(&items) {
        result.unwrap();
    }
    let pack = pack_of(&lab, "p1.txt");
    lab.at(T0 + DAY);
    lab.drive.delete("p1.txt").unwrap();
    lab.drive.delete("p2.txt").unwrap();
    lab.drive.repack(T0 + 2 * DAY).unwrap();
    let far = T0 + 200 * DAY;
    assert_eq!(
        lab.drive.collect_retired(far).unwrap(),
        0,
        "six clean rounds are one short: the due pack stays"
    );
    assert!(lab.data_objects().contains(&pack));
    assert_eq!(lab.drive.retired_packs().len(), 1, "and it stays retired");
    rounds.store(u64::from(GO_CLEAN_ROUNDS), Ordering::SeqCst);
    assert_eq!(
        lab.drive.collect_retired(far).unwrap(),
        1,
        "the seventh clean round: the same drive, the same pack, and it goes"
    );
    assert!(!lab.data_objects().contains(&pack));
}

/// The other deleter of the upkeep, at the same boundary: an object in flight is cleared up from the
/// bucket only with the evidence the gate wants.
#[test]
fn x_delta_sync2_2_an_object_in_flight_is_cleared_up_with_seven_clean_rounds_and_not_with_six() {
    let rounds = Arc::new(AtomicU64::new(u64::from(GO_CLEAN_ROUNDS) - 1));
    let counted = Arc::clone(&rounds);
    let lab = Lab::with(|drive| {
        drive
            .with_packing(PackOptions::default())
            .with_reclaim(ReclaimMode::Delete)
            .with_reclaim_standing(Arc::new(move |now| {
                let mut standing = (proven())(now)?;
                standing.evidence.clean_rounds =
                    u32::try_from(counted.load(Ordering::SeqCst)).unwrap_or(u32::MAX);
                Some(standing)
            }))
    });
    // An object whose writer stopped between the upload and the journal's end: written down as in
    // flight, named by no state.
    let id = crate::crypto::ObjectId::generate().unwrap();
    lab.bucket.put(&id.bucket_key(), b"an orphan").unwrap();
    lab.drive.note_in_flight(&id);
    let far = crate::time::now_unix() + 3_600;
    assert_eq!(
        lab.drive.recover_packs(far).unwrap(),
        0,
        "six clean rounds: it stays"
    );
    assert!(lab.data_objects().contains(&id.bucket_key()));
    rounds.store(u64::from(GO_CLEAN_ROUNDS), Ordering::SeqCst);
    assert_eq!(lab.drive.recover_packs(far).unwrap(), 1, "seven: it goes");
    assert!(!lab.data_objects().contains(&id.bucket_key()));
}

// ==== X-DELTA-SYNC2-2: the evidence that can see the failures that matter ====

/// A provider whose reclaimer window is `versions` days and whose clock policy is `server_clock`,
/// configured to delete (what a release after the first would ship): the lab's drives on a bucket the
/// test sets the clock of.
fn provider_with(versions_days: u64, server_clock: ServerClock) -> MetaIndexProvider {
    MetaIndexProvider::new(IndexProfile {
        reclaim: ReclaimOptions {
            mode: ReclaimMode::Delete,
            server_clock,
            retention: Retention {
                versions: std::time::Duration::from_secs(versions_days * DAY),
                ..RETENTION
            },
            ..ReclaimOptions::shadow()
        },
        ..IndexProfile::in_memory("Laptop")
    })
}

/// a.txt: version one made 40 days ago, replaced by version two 39 days ago. The restore dialog
/// (RETENTION: 90 days) still offers version one; a reclaimer whose window is 30 days calls it released
/// past its retention.
fn a_file_replaced_forty_days_ago() -> (Lab, String) {
    let now = crate::time::now_unix();
    let lab = Lab::starting_at(now - 40 * DAY, |drive| drive);
    lab.drive.put("a.txt", b"version one").unwrap();
    let first = lab.object_of("a.txt");
    lab.at(now - 39 * DAY);
    lab.drive.put("a.txt", b"version two").unwrap();
    (lab, first)
}

/// X-DELTA-SYNC2-2 (b). A clean round proved only that the app's own listing of the HEAD names nothing
/// the live set lacks - which `live` contains by construction: it could fail on a defect of the tree
/// walk and never on a mistake of the WINDOW, the one that deletes a version the user can still ask for.
/// The round now also derives what the restore dialog still offers its own way (the first parents,
/// the time each commit says, the dialog's own retention) and a live set that lacks any of it is a
/// round that was not clean. RED on the old code: the window of 30 days (not the 90 the dialog
/// promises) released version one, the head listing agreed, and the round was clean - one more toward
/// the switch to deleting.
#[test]
fn x_delta_sync2_2_a_window_shorter_than_the_restore_dialogs_is_not_a_clean_round() {
    let (lab, first) = a_file_replaced_forty_days_ago();
    let provider = provider_with(30, ServerClock::Unchecked);
    let bucket: Arc<dyn Drive> = Arc::new(lab.bucket.clone());
    let report = provider.reclaim(Arc::clone(&bucket), &lab.key).unwrap();
    assert_eq!(
        report.candidates,
        1,
        "by a window of 30 days version one is released past its retention: {}",
        report.line()
    );
    assert_eq!(
        report.rederived,
        Some(true),
        "the head listing cannot see it"
    );
    assert_eq!(
        report.window_rederived,
        Some(false),
        "the restore dialog still offers version one: {}",
        report.line()
    );
    assert!(report.line().contains("DISAGREES"), "{}", report.line());
    let (ledger, _) = Ledger::read(&DriveBucket::new(Arc::clone(&bucket)), &lab.key).unwrap();
    let round = ledger.rounds.last().expect("the round is written down");
    assert!(
        !round.clean && round.unclean.contains("restore dialog"),
        "{round:?}"
    );
    let evidence = report.evidence.expect("the evidence");
    assert_eq!(
        evidence.clean_rounds, 0,
        "it does not count toward the switch"
    );
    assert_eq!(evidence.window_rederived, Some(false));
    assert_eq!(report.deleted, 0);
    assert!(lab.data_objects().contains(&first), "version one stays");
}

/// The same history with the window the dialog promises: the restore dialog's own derivation agrees,
/// the round is clean, and version one is live (the window began with the state that names it).
#[test]
fn x_delta_sync2_2_a_window_that_agrees_with_the_restore_dialog_is_a_clean_round() {
    let (lab, first) = a_file_replaced_forty_days_ago();
    let provider = provider_with(90, ServerClock::Unchecked);
    let bucket: Arc<dyn Drive> = Arc::new(lab.bucket.clone());
    let report = provider.reclaim(Arc::clone(&bucket), &lab.key).unwrap();
    assert_eq!(report.candidates, 0, "{}", report.line());
    assert_eq!(report.window_rederived, Some(true), "{}", report.line());
    let (ledger, _) = Ledger::read(&DriveBucket::new(Arc::clone(&bucket)), &lab.key).unwrap();
    assert!(ledger.rounds.last().expect("written").clean);
    assert!(lab.data_objects().contains(&first));
}

/// What a restore can still bring back, derived its own way, is what the dialog offers: a version is
/// offered while the commit that REPLACED it is inside the dialog's retention, the head's objects
/// always, and a version replaced longer ago than that is not - it is what the reclaimer may take.
#[test]
fn x_delta_sync2_2_what_a_restore_can_bring_back_is_the_versions_replaced_inside_the_window() {
    use crate::meta::reclaim::restorable;
    let now = crate::time::now_unix();
    let lab = Lab::starting_at(now - 200 * DAY, |drive| drive);
    let mut objects = Vec::new();
    for (days_ago, text) in [
        (200, "version one"),
        (100, "version two"),
        (50, "version three"),
        (10, "version four"),
    ] {
        lab.at(now - days_ago * DAY);
        lab.drive.put("a.txt", text.as_bytes()).unwrap();
        objects.push(lab.object_of("a.txt"));
    }
    // The run is judged by the lab's clock: the present, as the dialog's derivation is.
    lab.at(now);
    let repo = lab.maintainer();
    let head = repo.head().expect("a head");
    let offered: std::collections::BTreeSet<String> = restorable(repo.objects(), &head, now)
        .unwrap()
        .keys()
        .map(|id| id.bucket_key())
        .collect();
    let expected: std::collections::BTreeSet<String> = objects[1..].iter().cloned().collect();
    assert_eq!(
        offered, expected,
        "versions two (replaced 50 days ago), three (10 days ago) and the head's four; not version one"
    );
    // The reclaimer, with the window the dialog promises, takes exactly the one the dialog does not
    // offer.
    let report = run(&mut lab.maintainer(), &options(ReclaimMode::Shadow)).unwrap();
    assert_eq!(report.candidates, 1, "{}", report.line());
    assert_eq!(report.window_rederived, Some(true));
}

/// X-DELTA-SYNC2-2 (b). The stamp of a round is the writing device's clock, and a past-dated clean
/// round stretched `days`. When the bucket must vouch for the time ([`ServerClock::Required`]) a round
/// the analysis called clean is written NOT clean first and is made clean only after the bucket's own
/// time agreed with the stamp: a bucket whose clock is days off the device's leaves a round that does not
/// count (and the evidence says the clocks differ). RED on the old code: the round was clean.
#[test]
fn x_delta_sync2_2_a_round_whose_stamp_the_buckets_clock_does_not_agree_with_does_not_count() {
    use crate::meta::shadow::UNVERIFIED;
    let lab = Lab::new();
    lab.drive.put("a.txt", b"version one").unwrap();
    let now = crate::time::now_unix();
    lab.bucket.set_server_clock(move || now - 3 * DAY);
    let provider = provider_with(90, ServerClock::Required);
    let bucket: Arc<dyn Drive> = Arc::new(lab.bucket.clone());
    let report = provider.reclaim(Arc::clone(&bucket), &lab.key).unwrap();
    let evidence = report.evidence.expect("the evidence");
    assert_eq!(evidence.clock_agrees, Some(false), "{}", report.line());
    assert_eq!(evidence.clean_rounds, 0);
    let (ledger, _) = Ledger::read(&DriveBucket::new(Arc::clone(&bucket)), &lab.key).unwrap();
    let round = ledger.rounds.last().expect("written");
    assert!(!round.clean && round.unclean == UNVERIFIED, "{round:?}");
    // And the standing the upkeep asks the gate with says so: a drive whose clock is off deletes nothing.
    let standing = provider.reclaim_standing("").expect("offered")(now).expect("a standing");
    assert_eq!(
        crate::meta::go(ReclaimMode::Delete, &standing.evidence),
        crate::meta::Go::Shadow(Hold::NoRestoreSource),
        "(the first thing lacking in a test's provider)"
    );
    assert_eq!(standing.evidence.clock_agrees, Some(false));
}

/// A bucket that agrees with the device's clock vouches for the round: it is made clean (the second
/// write), the evidence says the clocks agree and the round counts.
#[test]
fn x_delta_sync2_2_a_round_the_buckets_clock_agrees_with_is_made_clean() {
    let lab = Lab::new();
    lab.drive.put("a.txt", b"version one").unwrap();
    let now = crate::time::now_unix();
    lab.bucket.set_server_clock(move || now + 30);
    let provider = provider_with(90, ServerClock::Required);
    let bucket: Arc<dyn Drive> = Arc::new(lab.bucket.clone());
    let report = provider.reclaim(Arc::clone(&bucket), &lab.key).unwrap();
    let evidence = report.evidence.expect("the evidence");
    assert_eq!(evidence.clock_agrees, Some(true), "{}", report.line());
    assert_eq!(evidence.clean_rounds, 1, "{}", report.line());
    let (ledger, _) = Ledger::read(&DriveBucket::new(Arc::clone(&bucket)), &lab.key).unwrap();
    assert_eq!(ledger.rounds.len(), 1, "one round, made clean in place");
    assert!(ledger.rounds[0].clean && ledger.rounds[0].unclean.is_empty());
}

/// A bucket that says no time cannot vouch for one: under the policy that wants the bucket's word the
/// round stays unverified, and the evidence says the clock was not checked; a run whose policy takes the
/// device's clock alone ([`ServerClock::Unchecked`]: a folder, a test) is as it was - one write, clean.
#[test]
fn x_delta_sync2_2_a_bucket_that_says_no_time_vouches_for_nothing_and_an_unchecked_policy_needs_no_word(
) {
    let lab = Lab::new();
    lab.drive.put("a.txt", b"version one").unwrap();
    let bucket: Arc<dyn Drive> = Arc::new(lab.bucket.clone());
    let required = provider_with(90, ServerClock::Required);
    let report = required.reclaim(Arc::clone(&bucket), &lab.key).unwrap();
    let evidence = report.evidence.expect("the evidence");
    assert_eq!(evidence.clock_agrees, None, "{}", report.line());
    assert_eq!(evidence.clean_rounds, 0, "the round stays unverified");
    let unchecked = provider_with(90, ServerClock::Unchecked);
    let report = unchecked.reclaim(Arc::clone(&bucket), &lab.key).unwrap();
    let evidence = report.evidence.expect("the evidence");
    assert_eq!(evidence.clock_agrees, Some(true), "by policy");
    assert_eq!(
        evidence.clean_rounds, 1,
        "the first (unverified) round ended no streak, this one is clean"
    );
}

/// X-DELTA-SYNC4-4, the pure rule. A stamp is carried forward by the monotonic clock: a slow analysis
/// (an hour on a big drive) agrees with a bucket that is an hour on, a stamp 40 days off does not,
/// whatever the device's clock says at the moment the bucket is asked.
#[test]
fn x_delta_sync4_4_a_stamp_agrees_with_the_bucket_by_the_time_carried_forward_not_by_the_clock_of_the_moment(
) {
    use crate::meta::reclaim::{stamp_agrees, SKEW_SECS};
    let stamp = T0;
    // The analysis ran an hour: the bucket says an hour later.
    assert!(stamp_agrees(stamp + 3_600, stamp, 3_600));
    assert!(stamp_agrees(stamp + 3_600 + SKEW_SECS, stamp, 3_600));
    assert!(!stamp_agrees(stamp + 3_600 + SKEW_SECS + 1, stamp, 3_600));
    // The same bucket time against a stamp that was forty days ahead when the analysis began.
    assert!(!stamp_agrees(stamp + 3_600, stamp + 40 * DAY, 3_600));
    assert!(!stamp_agrees(stamp + 3_600, stamp - 40 * DAY, 3_600));
}

/// X-DELTA-SYNC4-4, the round. The device's clock is 40 days ahead when the round begins (typed by
/// hand, a wrong virtual machine host) and time sync corrects it during the analysis of a big drive:
/// the window was cut 40 days too late, the stamp is 40 days ahead, and at the write the corrected
/// clock agrees with the bucket. The bucket vouches for the stamp AS CARRIED FORWARD by the monotonic
/// clock, so the round stays unverified. RED on the old code: it compared the bucket with the clock
/// of the moment of the write, the round was made clean and counted.
#[test]
fn x_delta_sync4_4_a_clock_corrected_during_the_analysis_does_not_vouch_for_the_round() {
    use crate::meta::shadow::UNVERIFIED;
    let lab = Lab::new();
    lab.drive.put("a.txt", b"version one").unwrap();
    // The bucket's own time is the system's; asking for it is the moment the clock is corrected.
    lab.bucket.set_server_clock(|| {
        crate::time::step_clock(0);
        crate::time::system_unix()
    });
    crate::time::step_clock(i64::try_from(40 * DAY).unwrap());
    let provider = provider_with(90, ServerClock::Required);
    let bucket: Arc<dyn Drive> = Arc::new(lab.bucket.clone());
    let report = provider.reclaim(Arc::clone(&bucket), &lab.key);
    crate::time::step_clock(0);
    let report = report.unwrap();
    let evidence = report.evidence.expect("the evidence");
    assert_eq!(evidence.clock_agrees, Some(false), "{}", report.line());
    assert_eq!(evidence.clean_rounds, 0, "{}", report.line());
    let (ledger, _) = Ledger::read(&DriveBucket::new(Arc::clone(&bucket)), &lab.key).unwrap();
    let round = ledger.rounds.last().expect("written");
    assert!(!round.clean && round.unclean == UNVERIFIED, "{round:?}");
}

/// X-DELTA-SYNC4-4, the delete run. The lease the run wrote is compared with the time of day the
/// analysis was made at, carried forward by the monotonic clock, not with the device's clock of the
/// moment: here the clock reads 40 days ahead during the analysis and is corrected (to the bucket's
/// time) before the run deletes. RED on the old code: the corrected clock agreed with the bucket and the
/// objects released in the 40 days at the window's edge were deleted.
#[test]
fn x_delta_sync4_4_a_clock_corrected_between_the_analysis_and_the_delete_stops_the_run() {
    let lab = Lab::new();
    lab.drive.put("a.txt", b"version one").unwrap();
    lab.at(T0 + DAY);
    lab.drive.put("a.txt", b"version two").unwrap();
    // The bucket says what the corrected clock says; the analysis ran 40 days ahead of it.
    let true_time = T0 + 100 * DAY;
    lab.bucket.set_server_clock(move || true_time + 5);
    lab.at(true_time + 40 * DAY);
    let required = ReclaimOptions {
        mode: ReclaimMode::Delete,
        ..ReclaimOptions::shadow()
    };
    let mut repo = lab.maintainer();
    let report = reclaim_judged(&mut repo, &required, &mut |_, _| {
        // The analysis is done; the clock is corrected before anything is deleted.
        lab.at(true_time);
        Ok(DeleteLicence::lab())
    })
    .unwrap();
    assert_eq!(report.deleted, 0, "{}", report.line());
    assert!(
        matches!(report.stopped, Some(NotYet::Clock { .. })),
        "{}",
        report.line()
    );
    assert_eq!(lab.data_objects().len(), 2, "nothing was deleted");
}

/// X-DELTA-SYNC4-10, as X-DELTA-SYNC6-1 corrected it. The user-confirmed purge erases every version
/// there was when they said so - the version that was current at the click included, however late
/// the save that replaced it - and keeps what a commit made after the confirmation FIRST names: a
/// version they never agreed to lose. The first version of this test kept version two, "released by
/// the save after the click"; that was the state the click found, the very thing the purge is for.
/// RED on the old code: the window of the purge began at the click, so the click-time state was live
/// and version two stayed.
#[test]
fn x_delta_sync4_10_the_purge_keeps_the_versions_first_made_after_the_confirmation_and_erases_every_other_one(
) {
    let now = crate::time::now_unix();
    let lab = Lab::starting_at(now - 30 * DAY, |drive| drive);
    lab.bucket.set_server_clock(crate::time::now_unix);
    lab.drive.put("a.txt", b"version one").unwrap();
    let one = lab.object_of("a.txt");
    lab.at(now - 20 * DAY);
    lab.drive.put("a.txt", b"version two").unwrap();
    let two = lab.object_of("a.txt");
    // The user confirmed ten days ago; the file was saved twice after it.
    let confirmation = crate::meta::reclaim::PurgeConfirmation::lab(now - 10 * DAY);
    lab.at(now - 5 * DAY);
    lab.drive.put("a.txt", b"version three").unwrap();
    let three = lab.object_of("a.txt");
    lab.at(now - 4 * DAY);
    lab.drive.put("a.txt", b"version four").unwrap();
    let four = lab.object_of("a.txt");
    assert_eq!(lab.data_objects().len(), 4, "four versions in the bucket");
    let provider = provider_with(90, ServerClock::Required);
    let bucket: Arc<dyn Drive> = Arc::new(lab.bucket.clone());
    let report = provider
        .purge("drive-1", bucket, &lab.key, &confirmation)
        .unwrap();
    let left = lab.data_objects();
    assert!(
        !left.contains(&one) && !left.contains(&two),
        "versions one and two were there when the user confirmed: erased ({left:?}): {}",
        report.line()
    );
    assert!(
        left.contains(&three) && left.contains(&four),
        "version three was first made after the confirmation and version four is the head: both stay \
         ({left:?}): {}",
        report.line()
    );
    assert_eq!((report.deleted, report.failed), (2, 0), "{}", report.line());
    assert_eq!(lab.drive.get("a.txt").unwrap(), b"version four");
}

/// X-DELTA-SYNC6-1, the order the docs describe: the user confirms, the re-encryption pass rewrites
/// every file in the hours after the click (each rewrite is a commit made AFTER the confirmation and
/// releases the click-time object, the one the old key opens), the purge runs when the pass is over.
/// The click-time objects are gone; what the pass wrote, and a version the user saved after the
/// click, stay. RED on the old code: the window began at the click, so the state the click found was
/// live and the releases of the pass's own commits were never scanned - the purge erased the history
/// before the click and none of the objects the user wanted gone (one of three here).
#[test]
fn x_delta_sync6_1_a_purge_confirmed_before_the_re_encryption_erases_the_objects_the_pass_replaced()
{
    use crate::{rotation::ReencryptState, tests::asked::reencrypt_pass_of_parts};

    let now = crate::time::now_unix();
    let lab = Lab::starting_at(now - 30 * DAY, |drive| drive);
    lab.bucket.set_server_clock(crate::time::now_unix);
    lab.drive.put("a.txt", b"a, version one").unwrap();
    let a_one = lab.object_of("a.txt");
    lab.at(now - 20 * DAY);
    lab.drive.put("a.txt", b"a, version two").unwrap();
    let a_two = lab.object_of("a.txt");
    lab.drive.put("b.txt", b"b, the only version").unwrap();
    let b_one = lab.object_of("b.txt");

    // Ten days ago the user chose "re-encrypt everything and erase the old versions": the answer is
    // in the state the pass saves.
    let mut state = ReencryptState::new(now + 10);
    state.confirm_purge(now - 10 * DAY);
    // The pass takes hours: its commits come after the confirmation.
    lab.at(now - 10 * DAY + 3_600);
    assert!(
        reencrypt_pass_of_parts(&lab.key, &lab.drive, &mut state, &mut |_| Ok(()), &|| false)
            .unwrap()
    );
    assert_eq!(state.done, 2);
    let a_new = lab.object_of("a.txt");
    let b_new = lab.object_of("b.txt");
    // A save of the user's after the pass: a version first made after the confirmation.
    lab.at(now - 9 * DAY);
    lab.drive.put("b.txt", b"b, edited after the pass").unwrap();
    let b_edit = lab.object_of("b.txt");
    assert_eq!(lab.data_objects().len(), 6);

    let provider = provider_with(90, ServerClock::Required);
    let bucket: Arc<dyn Drive> = Arc::new(lab.bucket.clone());
    let confirmation = state.purge_confirmation().expect("they confirmed");
    let report = provider
        .purge("drive-1", bucket, &lab.key, &confirmation)
        .unwrap();
    let left = lab.data_objects();
    for (name, gone) in [("a one", &a_one), ("a two", &a_two), ("b one", &b_one)] {
        assert!(
            !left.contains(gone),
            "{name} was there when the user confirmed: erased ({left:?}): {}",
            report.line()
        );
    }
    for (name, kept) in [
        ("a, re-encrypted", &a_new),
        ("b, re-encrypted", &b_new),
        ("b, edited", &b_edit),
    ] {
        assert!(
            left.contains(kept),
            "{name} was first made after the confirmation: kept ({left:?}): {}",
            report.line()
        );
    }
    assert_eq!((report.deleted, report.failed), (3, 0), "{}", report.line());
    assert_eq!(lab.drive.get("a.txt").unwrap(), b"a, version two");
    assert_eq!(lab.drive.get("b.txt").unwrap(), b"b, edited after the pass");
}

/// X-DELTA-SYNC7-2: a pass that left a file whose head it could not read is no licence to erase. The
/// older versions of that file are the only readable copies of it: the confirmed purge ran after such a
/// pass (`state.packs_done` was all the erase asked) and erased exactly them, and the command said "1
/// damaged ones left as they were" afterwards. RED on the old code: the state of a pass with a damaged
/// head gave the confirmation (`erase_licence` did not exist; `purge_confirmation()` answered `Some`
/// and the kit erased on it), and a damaged member of a pack was not even counted.
#[test]
fn x_delta_sync7_2_a_pass_that_left_a_damaged_head_gives_no_licence_to_erase() {
    use crate::{
        rotation::{EraseRefusal, ReencryptState},
        tests::asked::reencrypt_pass_of_parts,
    };

    let now = crate::time::now_unix();
    let lab = Lab::starting_at(now - 30 * DAY, |drive| drive);
    lab.bucket.set_server_clock(crate::time::now_unix);
    lab.drive.put("a.txt", b"a, version one").unwrap();
    let a_one = lab.object_of("a.txt");
    lab.at(now - 20 * DAY);
    lab.drive.put("a.txt", b"a, version two").unwrap();
    let a_two = lab.object_of("a.txt");
    lab.drive.put("b.txt", b"b, the only version").unwrap();
    // The head of a.txt rots in the bucket: the pass cannot read it.
    let sound = lab.bucket.get(&a_two).unwrap();
    lab.bucket.put(&a_two, b"rotten").unwrap();

    let mut state = ReencryptState::new(now + 10);
    state.confirm_purge(now - 10 * DAY);
    lab.at(now - 10 * DAY + 3_600);
    assert!(
        reencrypt_pass_of_parts(&lab.key, &lab.drive, &mut state, &mut |_| Ok(()), &|| false)
            .unwrap(),
        "the pass is over: it counts the file it could not read and goes on"
    );
    assert_eq!((state.done, state.failed), (1, 1), "{state:?}");
    assert!(state.packs_done);
    assert!(state.purge_confirmation().is_some(), "they did confirm it");
    let refused = state
        .erase_licence()
        .err()
        .expect("no licence after damage");
    assert_eq!(refused, EraseRefusal::Damaged { files: 1 });
    assert!(
        refused.to_string().contains("could not be read")
            && refused.to_string().contains("only readable copies"),
        "{refused}"
    );
    // Nothing could have been erased: the older version of the damaged file is in the bucket, and the
    // window of the restore dialog still names it - it restores.
    assert!(
        lab.data_objects().contains(&a_one),
        "{:?}",
        lab.data_objects()
    );
    let mut repo = lab.maintainer();
    repo.pull().unwrap();
    repo.fetch_all().unwrap();
    let offered =
        crate::meta::reclaim::restorable(repo.objects(), &repo.head().unwrap(), now).unwrap();
    assert!(
        offered.keys().any(|id| id.bucket_key() == a_one),
        "the older version of the damaged file is still offered by a restore"
    );

    // The control: the rot is repaired, the pass finds nothing it cannot read - and the licence is given.
    lab.bucket.put(&a_two, &sound).unwrap();
    let mut again = ReencryptState::new(now + 10);
    again.confirm_purge(now - 10 * DAY);
    assert!(
        reencrypt_pass_of_parts(&lab.key, &lab.drive, &mut again, &mut |_| Ok(()), &|| false)
            .unwrap()
    );
    assert_eq!((again.done, again.failed), (2, 0), "{again:?}");
    assert_eq!(
        again.erase_licence().map(|c| c.at).ok(),
        Some(now - 10 * DAY),
        "an undamaged pass erases on the confirmation as before"
    );
    // And the other two refusals the licence has always carried.
    let mut unconfirmed = ReencryptState::new(now);
    unconfirmed.packs_done = true;
    assert_eq!(
        unconfirmed.erase_licence().err(),
        Some(EraseRefusal::NotConfirmed)
    );
    let mut early = ReencryptState::new(now);
    early.confirm_purge(now);
    assert_eq!(early.erase_licence().err(), Some(EraseRefusal::NotOver));
}

/// X-DELTA-SYNC6-3: what the account and the command line call is the provider's trait method, not
/// the concrete provider's `purge`: through `&dyn IndexProvider` the same confirmed purge erases the
/// same objects, and the confirmation is the one a pass recorded. RED on the old code: the trait had
/// no such method - `purge` had no caller outside tests.
#[test]
fn x_delta_sync6_3_the_provider_trait_runs_the_confirmed_purge() {
    use crate::{encrypted::IndexProvider, rotation::ReencryptState};

    let now = crate::time::now_unix();
    let lab = Lab::starting_at(now - 30 * DAY, |drive| drive);
    lab.bucket.set_server_clock(crate::time::now_unix);
    lab.drive.put("a.txt", b"version one").unwrap();
    let one = lab.object_of("a.txt");
    lab.at(now - 20 * DAY);
    lab.drive.put("a.txt", b"version two").unwrap();
    let two = lab.object_of("a.txt");
    let mut state = ReencryptState::new(now);
    state.confirm_purge(now - 10 * DAY);
    let confirmation = state.purge_confirmation().expect("they confirmed");
    let concrete = provider_with(90, ServerClock::Required);
    let provider: &dyn IndexProvider = &concrete;
    let bucket: Arc<dyn Drive> = Arc::new(lab.bucket.clone());
    let report = provider
        .purge_confirmed("drive-1", bucket, &lab.key, &confirmation)
        .unwrap();
    let left = lab.data_objects();
    assert!(!left.contains(&one), "erased ({left:?}): {}", report.line());
    assert!(left.contains(&two), "the head stays ({left:?})");
    assert_eq!((report.deleted, report.failed), (1, 0), "{}", report.line());
}

/// X-DELTA-SYNC6-3: a provider that keeps no history has nothing to erase, and says so - it never
/// answers as if it had erased (the default of the trait method). Nothing is touched.
#[test]
fn x_delta_sync6_3_a_provider_without_history_refuses_the_erase() {
    use crate::{
        encrypted::{IndexProvider, NameIndex},
        rotation::ReencryptState,
    };
    struct NoHistory;
    impl IndexProvider for NoHistory {
        fn open_index(
            &self,
            _drive: &str,
            _bucket: Arc<dyn Drive>,
            _drive_key: &crate::crypto::DriveKey,
        ) -> Result<Arc<dyn NameIndex>, crate::DriveError> {
            Err(crate::DriveError::Unsupported(String::from("not opened here")))
        }
    }
    let lab = Lab::starting_at(crate::time::now_unix() - DAY, |drive| drive);
    lab.drive.put("a.txt", b"version one").unwrap();
    let mut state = ReencryptState::new(crate::time::now_unix());
    state.confirm_purge(crate::time::now_unix());
    let confirmation = state.purge_confirmation().expect("they confirmed");
    let bucket: Arc<dyn Drive> = Arc::new(lab.bucket.clone());
    let refused = NoHistory
        .purge_confirmed("drive-1", bucket, &lab.key, &confirmation)
        .unwrap_err();
    assert!(
        matches!(refused, crate::DriveError::Unsupported(_)),
        "{refused:?}"
    );
    assert_eq!(lab.drive.get("a.txt").unwrap(), b"version one");
}

/// X-DELTA-SYNC6-1: the purge's window is the confirmation's own time, carried as it is - not a
/// span measured from a clock read before the repository was opened (that one started late by the
/// time the open takes, and was zero for a confirmation dated ahead of the purge's clock: the
/// no-window behaviour, every released version gone).
#[test]
fn x_delta_sync6_1_the_purge_options_carry_the_confirmations_own_time() {
    let confirmation = crate::meta::reclaim::PurgeConfirmation::lab(1_234_567);
    assert_eq!(
        crate::meta::reclaim::purge_options(&confirmation).purge_after,
        Some(1_234_567)
    );
    assert_eq!(
        ReclaimOptions::shadow().purge_after,
        None,
        "the reclaimer's own run has none"
    );
}

/// X-DELTA-SYNC6-1: a confirmation dated after this device's clock covers what cannot be told (a
/// clock set back since): nothing is erased. RED on the old code: `since` came out zero, the window
/// became "no window" and every released version went, versions saved after the click included.
#[test]
fn x_delta_sync6_1_a_confirmation_dated_after_this_devices_clock_erases_nothing() {
    let now = crate::time::now_unix();
    let lab = Lab::starting_at(now - 30 * DAY, |drive| drive);
    lab.bucket.set_server_clock(crate::time::now_unix);
    lab.drive.put("a.txt", b"version one").unwrap();
    lab.at(now - 20 * DAY);
    lab.drive.put("a.txt", b"version two").unwrap();
    let confirmation = crate::meta::reclaim::PurgeConfirmation::lab(now + 3 * DAY);
    let provider = provider_with(90, ServerClock::Required);
    let bucket: Arc<dyn Drive> = Arc::new(lab.bucket.clone());
    let report = provider
        .purge("drive-1", bucket, &lab.key, &confirmation)
        .unwrap();
    assert_eq!(report.deleted, 0, "{}", report.line());
    assert!(
        matches!(report.stopped, Some(NotYet::Confirmation { .. })),
        "{}",
        report.line()
    );
    assert_eq!(lab.data_objects().len(), 2, "nothing was erased");
}


// ==== X-DELTA-AZDRIVE-SYNC-6: the kill switch of the deleting ====

/// THE gate: a hold on the deleting, or a switch nobody could read, is no delete - whatever evidence stands
/// behind it - and a drive that is not configured to delete says what it said before. RED on the old code: there
/// was no switch; a drive with the evidence deleted whatever the owner wanted (the only switch was the mode the
/// drive was configured with, which the app sets at its own start).
#[test]
fn x_delta_azdrive_sync_6_the_one_gate_holds_on_a_kill_switch_that_is_set_and_on_one_that_cannot_be_read() {
    use crate::meta::reclaim::{ask_retired_delete, History};
    use crate::meta::Switch;

    let at = T0;
    let full = (proven())(at).expect("a standing");
    let ask = |mode: ReclaimMode, switch: Switch| {
        ask_retired_delete(
            History::Kept {
                mode,
                standing: Some(&full),
                switch,
            },
            at,
        )
    };
    assert!(ask(ReclaimMode::Delete, Switch::Clear).is_ok(), "clear: the evidence decides");
    assert_eq!(
        ask(ReclaimMode::Delete, Switch::Held).unwrap_err(),
        Hold::KillSwitch,
        "a hold outranks the evidence"
    );
    assert_eq!(
        ask(ReclaimMode::Delete, Switch::Unread).unwrap_err(),
        Hold::SwitchNotRead,
        "a deleter that cannot read the switch does not delete"
    );
    // A drive that deletes nothing says what it said.
    assert_eq!(ask(ReclaimMode::Shadow, Switch::Held).unwrap_err(), Hold::Configured);
    assert_eq!(ask(ReclaimMode::Off, Switch::Unread).unwrap_err(), Hold::Off);
    // What a look at the object says: not there, there, could not be asked.
    let lab = Lab::new();
    assert_eq!(Switch::read_drive(&lab.bucket), Switch::Clear);
    assert_eq!(Switch::read_bucket(&lab.bucket), Switch::Clear);
    lab.bucket.put(keys::RECLAIM_HOLD, b"held").unwrap();
    assert_eq!(Switch::read_drive(&lab.bucket), Switch::Held);
    assert_eq!(Switch::read_bucket(&lab.bucket), Switch::Held);
    assert!(!Hold::KillSwitch.to_string().is_empty() && !Hold::SwitchNotRead.to_string().is_empty());
}

/// The reclaimer's own delete run looks at the switch before every batch: a hold set before the run stops it
/// before anything is deleted (the run was handed a licence of the test's own: the switch is not the licence's
/// business), and the hold's release lets the same run delete. The hold is set the way an operator sets it,
/// through the provider.
#[test]
fn x_delta_azdrive_sync_6_the_reclaimers_run_stops_before_a_batch_while_the_kill_switch_is_set() {
    use crate::meta::{NotYet, Switch};

    let lab = Lab::new();
    lab.drive.put("a.txt", b"version one").unwrap();
    lab.at(T0 + DAY);
    lab.drive.put("a.txt", b"version two").unwrap();
    lab.at(T0 + 100 * DAY);
    let operator = MetaIndexProvider::new(IndexProfile::in_memory("Operator"));
    let bucket: Arc<dyn Drive> = Arc::new(lab.bucket.clone());
    operator
        .hold_deleting(Arc::clone(&bucket), &lab.key, "a defect was found in the reclaimer")
        .unwrap();
    assert_eq!(Switch::read_drive(bucket.as_ref()), Switch::Held);
    operator
        .hold_deleting(Arc::clone(&bucket), &lab.key, "set twice")
        .expect("setting it twice is setting it");

    let report = run(&mut lab.maintainer(), &options(ReclaimMode::Delete)).unwrap();
    assert_eq!(report.stopped, Some(NotYet::Switch(Switch::Held)), "{}", report.line());
    assert_eq!(report.deleted, 0);
    assert_eq!(lab.data_objects().len(), 2, "nothing was deleted");

    operator.release_deleting_hold(Arc::clone(&bucket)).unwrap();
    operator
        .release_deleting_hold(Arc::clone(&bucket))
        .expect("releasing it twice is releasing it");
    let report = run(&mut lab.maintainer(), &options(ReclaimMode::Delete)).unwrap();
    assert_eq!((report.deleted, report.failed), (1, 0), "{}", report.line());
    assert_eq!(lab.data_objects().len(), 1);
    assert_eq!(lab.drive.get("a.txt").unwrap(), b"version two");
}

/// The pack upkeep - the other deleter of what the history may still name - reads the switch at every delete:
/// the same drive with the same evidence (a standing in which nothing is lacking) keeps its due pack while the
/// hold is there and deletes it once it is released. RED on the old code: the pack goes with the hold set.
#[test]
fn x_delta_azdrive_sync_6_the_pack_upkeep_keeps_a_due_pack_while_the_kill_switch_is_set() {
    let (lab, pack) = lab_with_a_retired_pack(0);
    let far = T0 + 200 * DAY;
    lab.bucket.put(keys::RECLAIM_HOLD, b"held").unwrap();
    assert_eq!(
        lab.drive.collect_retired(far).unwrap(),
        0,
        "the pack is due, the evidence is complete, and the kill switch is set: it stays"
    );
    assert!(lab.data_objects().contains(&pack));
    assert_eq!(lab.drive.retired_packs().len(), 1, "and it stays retired");
    lab.bucket.delete(keys::RECLAIM_HOLD).unwrap();
    assert_eq!(
        lab.drive.collect_retired(far).unwrap(),
        1,
        "the hold is released: the same drive, the same pack, and it goes"
    );
    assert!(!lab.data_objects().contains(&pack));
}

// ==== X-DELTA-SYNC2-2: two rounds a day apart agree about what they share ====

/// A round is clean only if the candidates it names that were released before the window of the round BEFORE it
/// began are exactly the candidates that round named (its digest): two rounds over one history that disagree
/// about what an old commit released are not evidence for the switch to deleting - a defect of the analysis, a
/// window that drifted, an object that was named again. RED on the old code: nothing compared a round with the
/// one before it, and the planted disagreement was one more clean round toward the switch.
#[test]
fn x_delta_sync2_2_a_round_whose_old_candidates_are_not_the_ones_the_round_before_named_is_not_clean() {
    let now = crate::time::now_unix();
    let lab = Lab::starting_at(now - 130 * DAY, |drive| drive);
    lab.drive.put("a.txt", b"version one").unwrap();
    lab.at(now - 120 * DAY);
    // Version one is released 120 days ago: past the 90 days, a candidate of every round.
    lab.drive.put("a.txt", b"version two").unwrap();
    let provider = provider_with(90, ServerClock::Unchecked);
    let bucket: Arc<dyn Drive> = Arc::new(lab.bucket.clone());
    let dbucket = DriveBucket::new(Arc::clone(&bucket));

    // The control: two real rounds over the same history agree, and both are clean.
    let first = provider.reclaim(Arc::clone(&bucket), &lab.key).unwrap();
    assert_eq!(first.candidates, 1, "{}", first.line());
    let second = provider.reclaim(Arc::clone(&bucket), &lab.key).unwrap();
    let (ledger, _) = Ledger::read(&dbucket, &lab.key).unwrap();
    assert_eq!(ledger.rounds.len(), 2);
    assert!(
        ledger.rounds.iter().all(|round| round.clean && round.window_start > 0),
        "{:?}",
        ledger.rounds
    );
    assert_eq!(second.evidence.expect("the evidence").clean_rounds, 2);

    // The round before names other candidates than the history shows for the same span: planted.
    let (mut planted, version) = Ledger::read(&dbucket, &lab.key).unwrap();
    planted.rounds.last_mut().expect("a round").digest =
        String::from("the digest of some other candidates");
    assert!(planted.write(&dbucket, &lab.key, version.as_deref()).unwrap());
    let third = provider.reclaim(Arc::clone(&bucket), &lab.key).unwrap();
    let (ledger, _) = Ledger::read(&dbucket, &lab.key).unwrap();
    let round = ledger.rounds.last().expect("written down");
    assert!(
        !round.clean && round.unclean.contains("round before"),
        "{round:?}"
    );
    assert_eq!(
        third.evidence.expect("the evidence").clean_rounds,
        0,
        "it ends the streak toward the switch"
    );
    // Nothing was deleted by any of this: the evidence is a few rounds, not the seven the switch to deleting waits for.
    assert!(lab.data_objects().len() >= 2);
}
