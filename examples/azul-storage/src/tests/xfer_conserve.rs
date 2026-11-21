//! The conservation matrix of a move (ARCH28 RF43 acceptance gate; AR5a-2): whichever call fails -
//! a read of the source, a write to the target, a delete of the source - with its effect lost
//! on the way back or not, and wherever a stop comes, every file of the move is at the source or at
//! the target with its own bytes, never in neither, and the target holds nothing that is not a
//! whole copy of one of them. The move is the one operation that deletes: its whole defence is the
//! order (a file lands, its receipt retires its source), and this is the test of the order.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use super::{xfer_local::Versioned, TempDir};
use crate::{
    ctl::{Ctl, Quiet},
    xfer::{plan as make_plan, run, Choice, Item, Report},
    ByteRange, Drive, DriveError, ListPage, ListRequest, LocalDrive, ObjectInfo,
};

/// What the move moves: `a.txt`, and the folder `docs/` with a file in it and one a level
/// deeper.
const FILES: &[(&str, &[u8])] = &[
    ("a.txt", b"alpha"),
    ("docs/b.txt", b"beta"),
    ("docs/sub/c.txt", b"gamma"),
];

fn file(key: &str) -> Item {
    Item {
        key: key.to_string(),
        is_folder: false,
        size: None,
    }
}

fn folder(key: &str) -> Item {
    Item {
        key: key.to_string(),
        is_folder: true,
        size: None,
    }
}

/// The kinds of call a fault is aimed at: everything a move calls on a drive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Call {
    Read,
    Write,
    Delete,
    Head,
    List,
    CreateFolder,
}

const CALLS: [Call; 6] = [
    Call::Read,
    Call::Write,
    Call::Delete,
    Call::Head,
    Call::List,
    Call::CreateFolder,
];

/// A drive in front of an in-memory bucket that fails ONE call: the `n`th of its kind (from 0).
/// With `lands`, the call takes effect and its answer is lost on the way back (a timeout after the
/// write, a connection cut after the delete): the worst kind of failure for a move, since the
/// caller is told "failed" about something that happened. Calls count - and fail - once the drive
/// is armed, after the plan is made: the plan's own looks are not the run's.
struct Faulty<'a> {
    inner: &'a Versioned,
    fail: Option<(Call, usize)>,
    lands: bool,
    seen: [AtomicUsize; 6],
    armed: AtomicBool,
    injected: AtomicBool,
}

impl<'a> Faulty<'a> {
    fn new(inner: &'a Versioned, fail: Option<(Call, usize)>, lands: bool) -> Faulty<'a> {
        Faulty {
            inner,
            fail,
            lands,
            seen: std::array::from_fn(|_| AtomicUsize::new(0)),
            armed: AtomicBool::new(false),
            injected: AtomicBool::new(false),
        }
    }

    fn arm(&self) {
        self.armed.store(true, Ordering::SeqCst);
    }

    /// Whether this call is the one that fails.
    fn fails(&self, call: Call) -> bool {
        if !self.armed.load(Ordering::SeqCst) {
            return false;
        }
        let n = self.seen[call as usize].fetch_add(1, Ordering::SeqCst);
        let hit = self.fail == Some((call, n));
        if hit {
            self.injected.store(true, Ordering::SeqCst);
        }
        hit
    }

    /// How many calls of each kind the run made.
    fn made(&self) -> [usize; 6] {
        std::array::from_fn(|i| self.seen[i].load(Ordering::SeqCst))
    }

    fn was_injected(&self) -> bool {
        self.injected.load(Ordering::SeqCst)
    }

    fn lost() -> DriveError {
        DriveError::Io(String::from("injected fault"))
    }
}

impl Drive for Faulty<'_> {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        if self.fails(Call::List) {
            return Err(Self::lost());
        }
        self.inner.list(request)
    }
    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        if self.fails(Call::Read) {
            return Err(Self::lost());
        }
        self.inner.get(key)
    }
    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        if self.fails(Call::Read) {
            return Err(Self::lost());
        }
        self.inner.get_range(key, range)
    }
    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        if self.fails(Call::Write) {
            if self.lands {
                self.inner.put(key, bytes)?;
            }
            return Err(Self::lost());
        }
        self.inner.put(key, bytes)
    }
    fn delete(&self, key: &str) -> Result<(), DriveError> {
        if self.fails(Call::Delete) {
            if self.lands {
                self.inner.delete(key)?;
            }
            return Err(Self::lost());
        }
        self.inner.delete(key)
    }
    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        if self.fails(Call::Head) {
            return Err(Self::lost());
        }
        self.inner.head(key)
    }
    fn create_folder(&self, prefix: &str) -> Result<(), DriveError> {
        if self.fails(Call::CreateFolder) {
            return Err(Self::lost());
        }
        self.inner.create_folder(prefix)
    }
}

/// A stop the test sets from the run's own progress: the bytes of ALL the files so far
/// (`Progress::bytes_done`), not the count of the file being written - the per-file count of the
/// first version of this matrix never passed the largest file (5 bytes), and seven of its eleven
/// stops never came.
struct StopFlag<'a>(&'a AtomicBool);

impl Ctl for StopFlag<'_> {
    fn bytes(&self, _done: u64) {}

    fn cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Every file is at the source or at the target with its own bytes, and everything at the target
/// is a whole copy of one of them (or the marker of a folder).
fn assert_conserved(case: &str, source: &Versioned, target: &Versioned, prefix: &str) {
    assert_conserved_in(case, source, target, prefix, false);
}

/// [`assert_conserved`]; `same`: source and target are one store, so the keys outside the target's
/// folder are the files still at their places.
fn assert_conserved_in(
    case: &str,
    source: &Versioned,
    target: &Versioned,
    prefix: &str,
    same: bool,
) {
    for (key, bytes) in FILES {
        let at_source = source.get(key).ok();
        let at_target = target.get(&format!("{prefix}{key}")).ok();
        assert!(
            at_source.as_deref() == Some(*bytes) || at_target.as_deref() == Some(*bytes),
            "{case}: {key} is in neither place (source {at_source:?}, target {at_target:?})"
        );
    }
    for key in target.keys() {
        if key.ends_with('/') {
            assert_eq!(
                target.get(&key).unwrap(),
                b"",
                "{case}: the marker {key} holds something"
            );
            continue;
        }
        if same && !key.starts_with(prefix) {
            continue;
        }
        let original = key
            .strip_prefix(prefix)
            .and_then(|rest| FILES.iter().find(|(known, _)| *known == rest));
        let Some((_, bytes)) = original else {
            panic!("{case}: {key} is at the target and is nobody's copy");
        };
        assert_eq!(
            target.get(&key).unwrap(),
            *bytes,
            "{case}: {key} is not whole"
        );
    }
}

/// What a move of `a.txt` and `docs/` into `dest/` made of the calls of each drive, and how it ended.
struct Moved {
    report: Report,
    source_made: [usize; 6],
    target_made: [usize; 6],
    source_injected: bool,
    target_injected: bool,
}

/// A move of `a.txt` and `docs/` into `dest/` where ONE call fails - on the source's side or the
/// target's - with its effect kept or lost.
fn moved(
    source_store: &Versioned,
    target_store: &Versioned,
    source_fault: Option<(Call, usize)>,
    target_fault: Option<(Call, usize)>,
    lands: bool,
) -> Moved {
    let source = Faulty::new(source_store, source_fault, lands);
    let target = Faulty::new(target_store, target_fault, lands);
    let plan = make_plan(
        &source,
        &[file("a.txt"), folder("docs/")],
        &target,
        "dest/",
        true,
        "Copy",
    )
    .unwrap();
    source.arm();
    target.arm();
    let report = run(&plan, &source, &target, true, &Quiet, &mut |_| {});
    Moved {
        report,
        source_made: source.made(),
        target_made: target.made(),
        source_injected: source.was_injected(),
        target_injected: target.was_injected(),
    }
}

/// Every call a move makes - a read, a write, a delete, a `head`, a `list`, a `create_folder` - on
/// either drive, the `n`th of its kind for every `n` the move reaches (counted on a run with no
/// fault: the first version injected a fault in 22 of its 72 cases and ran the rest with none),
/// with its effect kept or lost. After it, nothing is lost; and when the run says nothing failed,
/// everything moved.
#[test]
fn a_move_loses_no_file_whichever_call_fails() {
    let base = moved(
        &Versioned::with(FILES),
        &Versioned::default(),
        None,
        None,
        false,
    );
    assert!(base.report.failed.is_empty(), "{:?}", base.report);
    let mut cases = 0;
    for on_source in [true, false] {
        let made = if on_source {
            base.source_made
        } else {
            base.target_made
        };
        for call in CALLS {
            for n in 0..made[call as usize] {
                // Only a write or a delete can take effect and be lost on the way back.
                let landings: &[bool] = if matches!(call, Call::Write | Call::Delete) {
                    &[false, true]
                } else {
                    &[false]
                };
                for &lands in landings {
                    let source_store = Versioned::with(FILES);
                    let target_store = Versioned::default();
                    let fault = Some((call, n));
                    let out = moved(
                        &source_store,
                        &target_store,
                        if on_source { fault } else { None },
                        if on_source { None } else { fault },
                        lands,
                    );
                    let case = format!(
                        "{} {call:?} #{n} lands={lands}: {:?}",
                        if on_source { "source" } else { "target" },
                        out.report
                    );
                    assert!(
                        if on_source {
                            out.source_injected
                        } else {
                            out.target_injected
                        },
                        "{case}: the fault was never reached"
                    );
                    assert_conserved(&case, &source_store, &target_store, "dest/");
                    if out.report.failed.is_empty() && !out.report.cancelled {
                        for (key, bytes) in FILES {
                            assert!(
                                source_store.get(key).is_err(),
                                "{case}: {key} is still at the source"
                            );
                            assert_eq!(
                                target_store.get(&format!("dest/{key}")).unwrap(),
                                *bytes,
                                "{case}"
                            );
                        }
                    }
                    cases += 1;
                }
            }
        }
    }
    // A move reads, writes and deletes at least once per file, and looks at the folders.
    assert!(cases >= 24, "only {cases} cases ran");
    for call in [Call::Read, Call::Delete] {
        assert!(base.source_made[call as usize] > 0, "{call:?}");
    }
    assert!(base.target_made[Call::Write as usize] > 0);
}

/// The same move stopped after `after` bytes IN ALL, for every `after` from before the first byte to
/// after the last (the files hold 14): a stop is no failure and no loss.
#[test]
fn a_move_that_is_stopped_anywhere_loses_no_file() {
    for after in [0u64, 1, 4, 5, 6, 9, 10, 13, 14, 15, 100] {
        let source_store = Versioned::with(FILES);
        let target_store = Versioned::default();
        let source = Faulty::new(&source_store, None, false);
        let target = Faulty::new(&target_store, None, false);
        let plan = make_plan(
            &source,
            &[file("a.txt"), folder("docs/")],
            &target,
            "dest/",
            true,
            "Copy",
        )
        .unwrap();
        let flag = AtomicBool::new(after == 0);
        let stop = StopFlag(&flag);
        let report = run(&plan, &source, &target, true, &stop, &mut |progress| {
            if progress.bytes_done >= after {
                flag.store(true, Ordering::SeqCst);
            }
        });
        let case = format!("stop after {after} bytes: {report:?}");
        assert_conserved(&case, &source_store, &target_store, "dest/");
        assert!(report.failed.is_empty(), "{case}: a stop is no failure");
        if after <= 5 {
            assert!(report.cancelled, "{case}: the stop never came");
        }
        if after >= 15 {
            assert!(!report.cancelled, "{case}: a stop that never comes");
        }
    }
}

/// A copy (not a move) never touches its source, whatever fails.
#[test]
fn a_copy_leaves_every_source_file_where_it_is_whichever_call_fails() {
    for call in CALLS {
        for n in 0..6 {
            let source_store = Versioned::with(FILES);
            let target_store = Versioned::default();
            let source = Faulty::new(&source_store, None, false);
            let target = Faulty::new(&target_store, Some((call, n)), true);
            let plan = make_plan(
                &source,
                &[file("a.txt"), folder("docs/")],
                &target,
                "dest/",
                false,
                "Copy",
            )
            .unwrap();
            source.arm();
            target.arm();
            let report = run(&plan, &source, &target, false, &Quiet, &mut |_| {});
            for (key, bytes) in FILES {
                assert_eq!(
                    source_store.get(key).unwrap(),
                    *bytes,
                    "{call:?} #{n}: {key} was touched by a copy: {report:?}"
                );
            }
        }
    }
}

/// The real code of a folder on disk (`durable`: a staged file, synced and renamed in) as the
/// target: whichever call of the source fails, every file is at the source or at the target.
#[test]
fn a_move_to_a_folder_on_disk_loses_no_file_whichever_source_call_fails() {
    let run_once = |fault: Option<(Call, usize)>, lands: bool| {
        let tmp = TempDir::new("conserve-disk");
        let target = LocalDrive::without_manifest(tmp.path());
        let source_store = Versioned::with(FILES);
        let source = Faulty::new(&source_store, fault, lands);
        let plan = make_plan(
            &source,
            &[file("a.txt"), folder("docs/")],
            &target,
            "dest/",
            true,
            "Copy",
        )
        .unwrap();
        source.arm();
        let report = run(&plan, &source, &target, true, &Quiet, &mut |_| {});
        for (key, bytes) in FILES {
            let at_source = source_store.get(key).ok();
            let at_target = target.get(&format!("dest/{key}")).ok();
            assert!(
                at_source.as_deref() == Some(*bytes) || at_target.as_deref() == Some(*bytes),
                "{fault:?} lands={lands}: {key} is in neither place: {report:?}"
            );
        }
        (source.made(), source.was_injected(), report, source_store)
    };
    let (made, _, base, _) = run_once(None, false);
    assert!(base.failed.is_empty(), "{base:?}");
    let mut cases = 0;
    for call in CALLS {
        for n in 0..made[call as usize] {
            for lands in [false, true] {
                if lands && call != Call::Delete {
                    continue;
                }
                let (_, injected, report, _) = run_once(Some((call, n)), lands);
                assert!(
                    injected,
                    "{call:?} #{n}: the fault was never reached: {report:?}"
                );
                cases += 1;
            }
        }
    }
    assert!(cases >= 6, "only {cases} cases ran");
}

/// A Replace of a name the target holds: whichever call of the target fails, the name holds the old
/// file or the new one - never nothing, never a part - and the source is only retired for the new.
#[test]
fn a_replace_that_fails_leaves_the_old_file_or_the_new_one_at_the_name() {
    let once = |fault: Option<(Call, usize)>, lands: bool| {
        let source_store = Versioned::with(FILES);
        let target_store = Versioned::default();
        target_store.write("dest/a.txt", b"old");
        let source = Faulty::new(&source_store, None, false);
        let target = Faulty::new(&target_store, fault, lands);
        let mut plan =
            make_plan(&source, &[file("a.txt")], &target, "dest/", true, "Copy").unwrap();
        assert_eq!(plan.conflicts().len(), 1, "{plan:?}");
        plan.choose(0, Choice::Replace);
        source.arm();
        target.arm();
        let report = run(&plan, &source, &target, true, &Quiet, &mut |_| {});
        let name = target_store.get("dest/a.txt").ok();
        assert!(
            name.as_deref() == Some(&b"old"[..]) || name.as_deref() == Some(&b"alpha"[..]),
            "{fault:?} lands={lands}: the name holds {name:?}: {report:?}"
        );
        if source_store.get("a.txt").is_err() {
            assert_eq!(
                name.as_deref(),
                Some(&b"alpha"[..]),
                "{fault:?} lands={lands}: the source went and the new file is not at the name"
            );
        }
        (target.made(), target.was_injected(), report)
    };
    let (made, _, base) = once(None, false);
    assert!(base.failed.is_empty(), "{base:?}");
    let mut cases = 0;
    for call in CALLS {
        for n in 0..made[call as usize] {
            for lands in [false, true] {
                if lands && !matches!(call, Call::Write | Call::Delete) {
                    continue;
                }
                let (_, injected, report) = once(Some((call, n)), lands);
                assert!(
                    injected,
                    "{call:?} #{n}: the fault was never reached: {report:?}"
                );
                cases += 1;
            }
        }
    }
    assert!(cases >= 2, "only {cases} cases ran");
}

/// A move within one drive is a rename (a copy and a delete where the drive has no rename): whichever
/// call fails, every file is at its place or at the new one.
#[test]
fn a_move_within_one_drive_loses_no_file_whichever_call_fails() {
    let once = |fault: Option<(Call, usize)>, lands: bool| {
        let store = Versioned::with(FILES);
        let drive = Faulty::new(&store, fault, lands);
        let plan = make_plan(
            &drive,
            &[file("a.txt"), folder("docs/")],
            &drive,
            "dest/",
            true,
            "Copy",
        )
        .unwrap();
        drive.arm();
        let report = run(&plan, &drive, &drive, true, &Quiet, &mut |_| {});
        let case = format!("{fault:?} lands={lands}: {report:?}");
        for (key, bytes) in FILES {
            let at_place = store.get(key).ok();
            let moved = store.get(&format!("dest/{key}")).ok();
            assert!(
                at_place.as_deref() == Some(*bytes) || moved.as_deref() == Some(*bytes),
                "{case}: {key} is in neither place"
            );
        }
        assert_conserved_in(&case, &store, &store, "dest/", true);
        (drive.made(), drive.was_injected(), report)
    };
    let (made, _, base) = once(None, false);
    assert!(base.failed.is_empty(), "{base:?}");
    let mut cases = 0;
    for call in CALLS {
        for n in 0..made[call as usize] {
            for lands in [false, true] {
                if lands && !matches!(call, Call::Write | Call::Delete) {
                    continue;
                }
                let (_, injected, report) = once(Some((call, n)), lands);
                assert!(
                    injected,
                    "{call:?} #{n}: the fault was never reached: {report:?}"
                );
                cases += 1;
            }
        }
    }
    assert!(cases >= 2, "only {cases} cases ran");
}
