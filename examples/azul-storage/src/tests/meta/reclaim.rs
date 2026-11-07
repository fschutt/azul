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
    meta::{
        keys,
        merge::keep_both,
        pointer,
        reclaim::{analyse, reclaim},
        Caps, Change, Maintenance, MemoryBucket, MetaError, MetaIndex, MetaIndexProvider, MetaRepo,
        NotYet, ReclaimMode, ReclaimOptions, RepoOptions, ServerClock, RETENTION,
    },
    packing::PackOptions,
    BatchPut, Drive,
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
        let clock = Arc::new(AtomicU64::new(T0));
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
        ..ReclaimOptions::default()
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
        ..ReclaimOptions::default()
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
    let provider = MetaIndexProvider::new("Laptop");
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
    let off = MetaIndexProvider::new("Laptop").with_reclaim(ReclaimOptions {
        mode: ReclaimMode::Off,
        ..ReclaimOptions::default()
    });
    off.maintain(Arc::clone(&bucket), &key, &Maintenance::default())
        .unwrap();
    assert!(off.last_reclaim().is_none());
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

/// A drive that packs, deletes by its upkeep, and has retired the pack of two small files that were
/// deleted: the pack is named by the first commit only (and the grace of a retired pack is
/// `grace`).
fn lab_with_a_retired_pack(grace: u64) -> (Lab, String) {
    let mut options = PackOptions::default();
    options.retired_grace_secs = grace;
    let lab = Lab::with(|drive| {
        drive
            .with_packing(options)
            .with_reclaim(ReclaimMode::Delete)
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
    let lab = Lab::with(|drive| drive.with_reclaim(ReclaimMode::Delete));
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
    let lab = Lab::with(|drive| drive.with_reclaim(ReclaimMode::Delete));
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
    use crate::rotation::{reencrypt_pass, ReencryptState};

    let bucket = Arc::new(MemoryBucket::new());
    bucket.set_server_clock(crate::time::now_unix);
    let key = DriveKey::generate().unwrap();
    let provider = MetaIndexProvider::new("Laptop");
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
    assert!(reencrypt_pass(&drive, &mut state, &mut |_| Ok(()), &|| false).unwrap());
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
            batch: 1_000,
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
