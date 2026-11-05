//! The conservation matrix of a move (ARCH28 RF43 acceptance gate; AR5a-2): whichever call fails -
//! a read of the source, a write to the target, a delete of the source - with its effect lost
//! on the way back or not, and wherever a stop comes, every file of the move is at the source or at
//! the target with its own bytes, never in neither, and the target holds nothing that is not a
//! whole copy of one of them. The move is the one operation that deletes: its whole defence is the
//! order (a file lands, its receipt retires its source), and this is the test of the order.

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

use super::xfer_local::Versioned;
use crate::{
    ctl::{Ctl, Quiet},
    xfer::{plan as make_plan, run, Item},
    ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo,
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

/// The kinds of call a fault is aimed at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Call {
    Read,
    Write,
    Delete,
}

/// A drive in front of an in-memory bucket that fails ONE call: the `n`th of its kind (from 0).
/// With `lands`, the call takes effect and its answer is lost on the way back (a timeout after the
/// write, a connection cut after the delete): the worst kind of failure for a move, since the
/// caller is told "failed" about something that happened.
struct Faulty<'a> {
    inner: &'a Versioned,
    fail: Option<(Call, usize)>,
    lands: bool,
    seen: [AtomicUsize; 3],
}

impl<'a> Faulty<'a> {
    fn new(inner: &'a Versioned, fail: Option<(Call, usize)>, lands: bool) -> Faulty<'a> {
        Faulty {
            inner,
            fail,
            lands,
            seen: [
                AtomicUsize::new(0),
                AtomicUsize::new(0),
                AtomicUsize::new(0),
            ],
        }
    }

    /// Whether this call is the one that fails.
    fn fails(&self, call: Call) -> bool {
        let n = self.seen[call as usize].fetch_add(1, Ordering::SeqCst);
        self.fail == Some((call, n))
    }

    fn lost() -> DriveError {
        DriveError::Io(String::from("injected fault"))
    }
}

impl Drive for Faulty<'_> {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
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
        self.inner.head(key)
    }
}

/// A stop that comes once `after` bytes of a file are written.
struct Stop {
    after: u64,
    heard: AtomicU64,
}

impl Ctl for Stop {
    fn bytes(&self, done: u64) {
        self.heard.fetch_max(done, Ordering::SeqCst);
    }

    fn cancelled(&self) -> bool {
        self.heard.load(Ordering::SeqCst) >= self.after
    }
}

/// Every file is at the source or at the target with its own bytes, and everything at the target
/// is a whole copy of one of them (or the marker of a folder).
fn assert_conserved(case: &str, source: &Versioned, target: &Versioned, prefix: &str) {
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

/// A move of `a.txt` and `docs/` into `dest/` of another drive where ONE call fails - on the
/// source's side or the target's, any kind of call, the first six of its kind, with its effect
/// kept or lost. After it, nothing is lost; and when the run says nothing failed, everything moved.
#[test]
fn a_move_loses_no_file_whichever_call_fails() {
    let mut cases = 0;
    for on_source in [true, false] {
        for call in [Call::Read, Call::Write, Call::Delete] {
            for n in 0..6 {
                for lands in [false, true] {
                    let source_store = Versioned::with(FILES);
                    let target_store = Versioned::default();
                    let fault = Some((call, n));
                    let source =
                        Faulty::new(&source_store, if on_source { fault } else { None }, lands);
                    let target =
                        Faulty::new(&target_store, if on_source { None } else { fault }, lands);
                    let plan = make_plan(
                        &source,
                        &[file("a.txt"), folder("docs/")],
                        &target,
                        "dest/",
                        true,
                        "Copy",
                    )
                    .unwrap();
                    let report = run(&plan, &source, &target, true, &Quiet, &mut |_| {});
                    let case = format!(
                        "{} {call:?} #{n} lands={lands}: {report:?}",
                        if on_source { "source" } else { "target" }
                    );
                    assert_conserved(&case, &source_store, &target_store, "dest/");
                    if report.failed.is_empty() && !report.cancelled {
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
    assert_eq!(cases, 72);
}

/// The same move stopped after `n` bytes, for every `n` from before the first byte to after the
/// last: a stop is no failure and no loss.
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
        let stop = Stop {
            after,
            heard: AtomicU64::new(0),
        };
        let report = run(&plan, &source, &target, true, &stop, &mut |_| {});
        let case = format!("stop after {after} bytes: {report:?}");
        assert_conserved(&case, &source_store, &target_store, "dest/");
        assert!(report.failed.is_empty(), "{case}: a stop is no failure");
    }
}

/// A copy (not a move) never touches its source, whatever fails.
#[test]
fn a_copy_leaves_every_source_file_where_it_is_whichever_call_fails() {
    for call in [Call::Read, Call::Write, Call::Delete] {
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
