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
        reclaim::{analyse, reclaim, reclaim_judged},
        shadow::Ledger,
        Caps, Change, DriveBucket, Hold, IndexProfile, Maintenance, MemoryBucket, MetaError,
        MetaIndex, MetaIndexProvider, MetaRepo, NotYet, ReclaimMode, ReclaimOptions, RepoOptions,
        ServerClock, RETENTION,
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

    let report = reclaim(&mut lab.maintainer(), &options(ReclaimMode::Shadow)).unwrap();
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
    let report = reclaim(&mut lab.maintainer(), &options(ReclaimMode::Delete)).unwrap();
    assert_eq!(
        (report.candidates, report.deleted),
        (0, 0),
        "{}",
        report.line()
    );
    assert_eq!(lab.data_objects().len(), 2);

    // A hundred days on, nothing can ask for it any more: it goes, and the quota frees.
    lab.at(T0 + 100 * DAY);
    let report = reclaim(&mut lab.maintainer(), &options(ReclaimMode::Delete)).unwrap();
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
    let again = reclaim(&mut lab.maintainer(), &options(ReclaimMode::Delete)).unwrap();
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
    let report = reclaim(&mut lab.maintainer(), &options(ReclaimMode::Delete)).unwrap();
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
    let report = reclaim(&mut lab.maintainer(), &options(ReclaimMode::Delete)).unwrap();
    assert_eq!(
        (report.candidates, report.deleted),
        (0, 0),
        "{}",
        report.line()
    );
    assert_eq!(lab.drive.get("p2.txt").unwrap(), b"second small file");

    // The last file leaves; its commit is inside the window: the history still names the pack.
    lab.drive.delete("p2.txt").unwrap();
    let report = reclaim(&mut lab.maintainer(), &options(ReclaimMode::Delete)).unwrap();
    assert_eq!(
        (report.candidates, report.deleted),
        (0, 0),
        "{}",
        report.line()
    );
    assert_eq!(lab.data_objects().len(), 1);

    // Past the retention nothing names it: the pack goes.
    lab.at(T0 + 300 * DAY);
    let report = reclaim(&mut lab.maintainer(), &options(ReclaimMode::Delete)).unwrap();
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
    let report = reclaim(&mut lab.maintainer(), &options(ReclaimMode::Delete)).unwrap();
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
    let report = reclaim(&mut repo, &options(ReclaimMode::Delete)).unwrap();
    assert_eq!(report.stopped, Some(NotYet::Moved), "{}", report.line());
    assert_eq!(report.deleted, 0);
    assert_eq!(lab.data_objects().len(), 2);
    // The next run, from fresh reads, goes through.
    let again = reclaim(&mut lab.maintainer(), &options(ReclaimMode::Delete)).unwrap();
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
    let report = reclaim(&mut lab.maintainer(), &required).unwrap();
    assert!(
        matches!(report.stopped, Some(NotYet::Clock { server: None, .. })),
        "{}",
        report.line()
    );
    assert_eq!(lab.data_objects().len(), 2);

    // The storage's clock is a week off this device's.
    lab.bucket
        .set_server_clock(move || T0 + 100 * DAY + 7 * DAY);
    let report = reclaim(&mut lab.maintainer(), &required).unwrap();
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
    let report = reclaim(&mut lab.maintainer(), &required).unwrap();
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
    let report = reclaim(&mut lab.maintainer(), &options(ReclaimMode::Delete)).unwrap();
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
    let again = reclaim(&mut lab.maintainer(), &options(ReclaimMode::Delete)).unwrap();
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
    let report = reclaim(&mut lab.maintainer(), &capped).unwrap();
    assert!(report.capped, "{}", report.line());
    assert_eq!(report.scanned, 2);
    let off = reclaim(&mut lab.maintainer(), &options(ReclaimMode::Off)).unwrap();
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
    let report = reclaim(&mut lab.maintainer(), &options(ReclaimMode::Delete)).unwrap();
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
    let bucket: Arc<dyn Drive> = Arc::new(MemoryBucket::new());
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

/// X-DELTA-AZDRIVE-SYNC-5 (d). The bucket's clock is compared with the device's clock of the moment
/// the lease was taken, not with the one the analysis began at: a big drive's analysis takes
/// longer than the skew, and the check could never pass.
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
        // The analysis took two hours (the bucket's clock and the device's both went on).
        slow.fetch_add(2 * 3_600, Ordering::SeqCst);
        ReclaimMode::Delete
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
    let report = reclaim(&mut lab.maintainer(), &options(ReclaimMode::Delete)).unwrap();
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
    let report = reclaim(&mut lab.maintainer(), &options(ReclaimMode::Shadow)).unwrap();
    assert_eq!(report.devices, 2, "the laptop and the maintenance device");
    assert_eq!(report.candidates, 0, "everything is inside the window");
    assert_eq!(report.releasing, None);

    lab.at(T0 + 100 * DAY);
    let one = reclaim(&mut lab.maintainer(), &options(ReclaimMode::Shadow)).unwrap();
    let two = reclaim(&mut lab.maintainer(), &options(ReclaimMode::Shadow)).unwrap();
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
    let three = reclaim(&mut lab.maintainer(), &options(ReclaimMode::Shadow)).unwrap();
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
    let whole = reclaim(&mut lab.maintainer(), &options(ReclaimMode::Shadow)).unwrap();
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
    let capped = reclaim(&mut lab.maintainer(), &small).unwrap();
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

/// The proof of a drive whose shadow rounds showed everything the switch to deleting asks for: what
/// the provider's own answer ([`crate::meta::MetaIndexProvider`]: its last judged round's verdict)
/// is, stood in for. A drive configured to delete deletes only with one (X-DELTA-SYNC2-2).
fn proven() -> crate::encrypted::ReclaimProof {
    Arc::new(|_| true)
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
            .with_reclaim_proof(proven())
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
            .with_reclaim_proof(proven())
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
            .with_reclaim_proof(proven())
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
            .with_reclaim_proof(proven())
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
    let report = provider.purge(raw, &key, &confirmation).unwrap();
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
#[ignore = "E8: the owner's machine; the command is in the doc comment"]
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
        !provider
            .reclaim_proof("d_sync2_2")
            .expect("the provider offers a proof")(far),
        "no round has judged the drive: no proof"
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

/// X-DELTA-SYNC2-2 (a). The proof is the verdict of the provider's last judged round: a round that
/// held back, no round, another drive's round and a verdict older than the rounds' schedule allows
/// prove nothing; one that said Delete proves it for [`VERDICT_FRESH_SECS`].
#[test]
fn x_delta_sync2_2_the_providers_proof_is_its_last_judged_rounds_verdict_and_it_goes_stale() {
    use crate::meta::index::VERDICT_FRESH_SECS;
    let provider = MetaIndexProvider::new(IndexProfile {
        reclaim: ReclaimOptions {
            mode: ReclaimMode::Delete,
            ..ReclaimOptions::shadow()
        },
        ..IndexProfile::in_memory("Laptop")
    });
    let proof = |drive: &str, now: u64| provider.reclaim_proof(drive).expect("a proof")(now);
    let at = T0;
    assert!(!proof("d_a", at), "no round yet");
    provider.note_verdict(Some("d_a"), at, true);
    assert!(proof("d_a", at));
    assert!(proof("d_a", at + VERDICT_FRESH_SECS));
    assert!(
        !proof("d_a", at + VERDICT_FRESH_SECS + 1),
        "a drive nobody judged for two days has no proof"
    );
    assert!(
        !proof("d_a", at - 1),
        "a verdict stamped in the future is stale, not fresh"
    );
    assert!(
        !proof("d_b", at),
        "another drive's round proves nothing here"
    );
    assert!(
        !provider.reclaim_proof("").expect("a proof")(at),
        "the round of no drive proves no drive"
    );
    // The next round held back: the proof is gone at once.
    provider.note_verdict(Some("d_a"), at + 100, false);
    assert!(!proof("d_a", at + 100));
}

/// The drive configured to delete AND given the proof deletes (the pack lifecycle the other tests
/// rely on), and one whose proof stops saying yes keeps the pack again.
#[test]
fn x_delta_sync2_2_a_drive_deletes_while_its_proof_says_so_and_not_after() {
    let options = PackOptions {
        retired_grace_secs: 0,
        ..PackOptions::default()
    };
    let (open_until, calls) = {
        let calls = Arc::new(AtomicU64::new(0));
        let seen = Arc::clone(&calls);
        let until = Arc::new(AtomicU64::new(u64::MAX));
        let limit = Arc::clone(&until);
        (
            until,
            (calls, move |now: u64| {
                seen.fetch_add(1, Ordering::SeqCst);
                now <= limit.load(Ordering::SeqCst)
            }),
        )
    };
    let (calls, proof) = calls;
    let lab = Lab::with(|drive| {
        drive
            .with_packing(options)
            .with_reclaim(ReclaimMode::Delete)
            .with_reclaim_proof(Arc::new(proof))
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
    // The proof is withdrawn: the pack, due and named by no state in the window, stays.
    open_until.store(0, Ordering::SeqCst);
    assert_eq!(lab.drive.collect_retired(T0 + 200 * DAY).unwrap(), 0);
    assert!(lab.data_objects().contains(&pack));
    assert!(
        calls.load(Ordering::SeqCst) >= 1,
        "the drive asked its proof"
    );
    // Given again, it goes.
    open_until.store(u64::MAX, Ordering::SeqCst);
    assert_eq!(lab.drive.collect_retired(T0 + 200 * DAY).unwrap(), 1);
    assert!(!lab.data_objects().contains(&pack));
}
