//! The engine around a pass: what makes [`super::sync_to`] a sync that survives crashes, stops
//! loops, logs what it destroys, retries in bounds and tells the user every file it could not
//! take. A pairing's [`Engine`] keeps its parts in the pairing's state folder:
//!
//! - the journal ([`super::journal`]), recovered before every pass scans;
//! - the operations log ([`super::oplog`]) and the loop detector ([`super::loops`]), fed by
//!   every local step the pass takes;
//! - the problem list ([`super::problems`]), published after every pass, failed or not;
//! - the pass backoff ([`super::retry::PassBackoff`]);
//! - quarantine marks ([`super::quarantine`]) on downloads from other people's devices.
//!
//! The pass reaches the engine through [`Observer`], one field of [`super::RunHooks`]: the
//! plan before anything moves, each local step before it starts and after it ended, and the
//! named points a test harness may stop or kill the process at.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{Mutex, PoisonError},
};

use serde::Serialize;

use super::{
    journal::{Intent, Journal, Recovery, Ticket},
    local::BaseEntry,
    loops::LoopDetector,
    oplog::{Op, OpKind, OpLog},
    problems::{self, Code, Problem, ProblemList},
    quarantine::{self, Writers},
    remote::RemoteIndex,
    retry::{random_jitter, PassBackoff},
    Action, RunHooks, SyncReport,
};
use crate::error::{CloudError, CloudResult};

/// One local step of a pass.
#[derive(Clone, Copy, Debug)]
pub enum Step<'a> {
    /// The drive's version of `key` (BLAKE3 `hash`, written by `writer`) comes down;
    /// `replaces`: a file of that name is here and is overwritten.
    Download {
        key: &'a str,
        hash: &'a str,
        replaces: bool,
        writer: &'a str,
    },
    /// `key` (with content `hash`) is deleted here: into the trash at `trash` (relative to the
    /// root), or for good when there is none.
    Delete {
        key: &'a str,
        hash: Option<&'a str>,
        trash: Option<&'a str>,
    },
    /// This side's version of `key` (BLAKE3 `hash`) is renamed to its conflict copy `copy`.
    Conflict {
        key: &'a str,
        copy: &'a str,
        hash: &'a str,
    },
    /// `from` is renamed to `to` (through `via`, a third name, for a case-only rename).
    Move {
        from: &'a str,
        to: &'a str,
        via: Option<&'a str>,
    },
}

impl Step<'_> {
    /// What the journal writes down for it, when it takes more than one change on disk.
    fn intent(&self) -> Option<Intent> {
        match *self {
            Step::Download { key, hash, .. } => Some(Intent::Download {
                key: key.to_string(),
                hash: hash.to_string(),
            }),
            Step::Delete {
                key,
                trash: Some(to),
                ..
            } => Some(Intent::Trash {
                key: key.to_string(),
                to: to.to_string(),
            }),
            Step::Delete { trash: None, .. } => None,
            Step::Conflict { key, copy, hash } => Some(Intent::Conflict {
                key: key.to_string(),
                copy: copy.to_string(),
                hash: hash.to_string(),
            }),
            Step::Move {
                from,
                to,
                via: Some(via),
            } => Some(Intent::CaseRename {
                from: from.to_string(),
                via: via.to_string(),
                to: to.to_string(),
            }),
            Step::Move { via: None, .. } => None,
        }
    }

    /// The line of the operations log, when the step destroyed something here.
    fn op(&self, t: f64) -> Option<Op> {
        let (op, key, to, hash) = match *self {
            Step::Download {
                key,
                replaces: true,
                ..
            } => (OpKind::Overwrite, key, None, None),
            Step::Download { .. } => return None,
            Step::Delete { key, hash, trash } => (
                if trash.is_some() {
                    OpKind::Trash
                } else {
                    OpKind::Delete
                },
                key,
                trash,
                hash,
            ),
            Step::Conflict { key, copy, hash } => (OpKind::Conflict, key, Some(copy), Some(hash)),
            Step::Move { from, to, .. } => (OpKind::Move, from, Some(to), None),
        };
        Some(Op {
            t,
            op,
            key: key.to_string(),
            to: to.map(str::to_string),
            hash: hash.map(str::to_string),
        })
    }
}

/// What a pass tells its engine. Every method has a default that does nothing, so a pass
/// without an engine (the plain command line) runs as before.
pub trait Observer: Sync {
    /// The plan of an attempt, before anything moves: actions may be taken out (a frozen
    /// loop), with their problems recorded.
    fn plan(
        &self,
        _actions: &mut Vec<Action>,
        _remote: &RemoteIndex,
        _base: &BTreeMap<String, BaseEntry>,
        _problems: &mut Vec<Problem>,
    ) {
    }

    /// A local step is about to start. An error means it must not start (its journal line
    /// could not be written).
    ///
    /// # Errors
    ///
    /// When the step cannot be recorded first.
    fn begin(&self, _step: &Step<'_>) -> CloudResult<Option<Ticket>> {
        Ok(None)
    }

    /// The step ended (`ok`: it happened); `path` is the file it left, for a download.
    fn finished(&self, _step: &Step<'_>, _ticket: Option<Ticket>, _ok: bool, _path: Option<&Path>) {
    }

    /// The pass reached the point `name` (`start`, `before-upload`, `after-commit`, ...).
    fn point(&self, _name: &str) {}
}

impl RunHooks<'_> {
    /// [`Observer::plan`], when the run has an observer.
    pub(crate) fn observe_plan(
        &self,
        actions: &mut Vec<Action>,
        remote: &RemoteIndex,
        base: &BTreeMap<String, BaseEntry>,
        problems: &mut Vec<Problem>,
    ) {
        if let Some(observer) = self.observer {
            observer.plan(actions, remote, base, problems);
        }
    }

    /// [`Observer::begin`].
    pub(crate) fn begin(&self, step: &Step<'_>) -> CloudResult<Option<Ticket>> {
        self.observer.map_or(Ok(None), |o| o.begin(step))
    }

    /// [`Observer::finished`].
    pub(crate) fn finished(
        &self,
        step: &Step<'_>,
        ticket: Option<Ticket>,
        ok: bool,
        path: Option<&Path>,
    ) {
        if let Some(observer) = self.observer {
            observer.finished(step, ticket, ok, path);
        }
    }

    /// [`Observer::point`].
    pub(crate) fn point(&self, name: &str) {
        if let Some(observer) = self.observer {
            observer.point(name);
        }
    }
}

/// The engine's clock: the system's, moved by `offset` seconds (a test harness gives a device
/// a clock that is off; nothing decides by it, it only stamps logs and windows).
#[derive(Clone, Copy, Debug, Default)]
pub struct Clock {
    pub offset: f64,
}

impl Clock {
    /// Now, in seconds since 1970 with fractions.
    #[must_use]
    pub fn now(&self) -> f64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0.0, |d| d.as_secs_f64())
            + self.offset
    }
}

/// A pairing's engine.
pub struct Engine {
    /// The pairing's state folder: every part's file is in it, never in the root.
    pub state_dir: PathBuf,
    /// The synced folder.
    pub root: PathBuf,
    pub clock: Clock,
    journal: Journal,
    oplog: OpLog,
    loops: Mutex<LoopDetector>,
    /// The user's own devices: a download written by any other device gets a quarantine mark.
    writers: Box<dyn Writers>,
    /// Problems the engine's own parts found during the pass (a log or a mark that could not
    /// be written).
    found: Mutex<Vec<Problem>>,
    /// The test harness's stops and kills at named points (the command line sets it).
    points: Option<Box<dyn Fn(&str) + Sync>>,
}

/// What one pass came to, as the command line and the apps show it.
#[derive(Clone, Debug, Serialize)]
pub struct PassSummary {
    /// 0: done (problems may be listed); 1: failed, try again; 2: refused, nothing changed
    /// (a root that is not the paired folder, a rolled-back or unreadable index, a newer
    /// format); 3: held for the user (a mass delete).
    pub exit: i32,
    /// Why the run failed, when it did.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub recovery: Recovery,
    pub problems: ProblemList,
    /// Seconds before the next pass should run (the backoff, a Retry-After).
    pub wait_secs: f64,
}

impl Engine {
    /// The engine of the folder `root`, whose state is in `state_dir`; `writers` says which
    /// devices are the user's own.
    #[must_use]
    pub fn new(state_dir: &Path, root: &Path, writers: Box<dyn Writers>) -> Engine {
        Engine {
            state_dir: state_dir.to_path_buf(),
            root: root.to_path_buf(),
            clock: Clock::default(),
            journal: Journal::new(state_dir),
            oplog: OpLog::new(state_dir),
            loops: Mutex::new(LoopDetector::load(state_dir)),
            writers,
            found: Mutex::new(Vec::new()),
            points: None,
        }
    }

    /// The same engine with a clock moved by `offset` seconds.
    #[must_use]
    pub fn with_clock(mut self, offset: f64) -> Engine {
        self.clock = Clock { offset };
        self
    }

    /// The same engine calling `points` at every named point of a pass.
    #[must_use]
    pub fn with_points(mut self, points: Box<dyn Fn(&str) + Sync>) -> Engine {
        self.points = Some(points);
        self
    }

    /// The operations log.
    #[must_use]
    pub fn oplog(&self) -> &OpLog {
        &self.oplog
    }

    /// How long the next pass should wait (a backoff after failures, a Retry-After).
    #[must_use]
    pub fn wait(&self) -> std::time::Duration {
        PassBackoff::load(&self.state_dir).wait(self.clock.now())
    }

    /// The user chose what `key` should be: its loop is let go.
    ///
    /// # Errors
    ///
    /// When the detector's memory cannot be written.
    pub fn release_loop(&self, key: &str) -> CloudResult<()> {
        let mut loops = self.loops.lock().unwrap_or_else(PoisonError::into_inner);
        loops.release(key);
        loops.save(&self.state_dir)
    }

    /// One pass: recovers the journal, runs `run` with `hooks` plus this engine as their
    /// observer, then publishes the problem list, saves the loop memory and the backoff. The
    /// run's own result, and what the pass came to.
    pub fn pass(
        &self,
        hooks: RunHooks<'_>,
        run: impl FnOnce(&RunHooks<'_>) -> CloudResult<SyncReport>,
    ) -> (CloudResult<SyncReport>, PassSummary) {
        self.point("start");
        let mut problems_now: Vec<Problem> = Vec::new();
        let recovery = match self.journal.recover(&self.root) {
            Ok(recovery) => recovery,
            Err(e) => {
                problems_now.push(Problem::new(Code::IoError, "", e.to_string()));
                Recovery::default()
            }
        };
        let hooks = RunHooks {
            observer: Some(self as &dyn Observer),
            ..hooks
        };
        let outcome = run(&hooks);
        let now = self.clock.now();
        let mut backoff = PassBackoff::load(&self.state_dir);
        let (exit, error, complete) = match &outcome {
            Ok(report) => {
                problems_now.extend(problems::from_report(report));
                let refused = problems_now.iter().any(|p| refuses(p.code));
                let held = report.mass_delete.is_some();
                let exit = if held {
                    3
                } else if refused || !report.newer_format.is_empty() {
                    2
                } else {
                    0
                };
                backoff.succeeded();
                let complete = exit == 0 && !report.cancelled;
                (exit, None, complete)
            }
            Err(e) => {
                let problem = problems::of_failed_pass(e);
                let exit = exit_of_failure(problem.code);
                problems_now.push(problem);
                backoff.failed(e, now, random_jitter());
                (exit, Some(e.to_string()), false)
            }
        };
        problems_now.extend(
            self.found
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .drain(..),
        );
        let mut list = ProblemList::load(&self.state_dir).unwrap_or_default();
        list.publish(problems_now, complete, now as i64);
        if let Err(e) = list.save(&self.state_dir) {
            list.problems
                .push(Problem::new(Code::IoError, "", e.to_string()));
        }
        {
            let mut loops = self.loops.lock().unwrap_or_else(PoisonError::into_inner);
            loops.forget_old(now);
            let _ = loops.save(&self.state_dir);
        }
        let _ = backoff.save(&self.state_dir);
        let summary = PassSummary {
            exit,
            error,
            recovery,
            problems: list,
            wait_secs: backoff.wait(now).as_secs_f64(),
        };
        (outcome, summary)
    }

    /// Runs passes until `stop` is set: one at start, one soon after the watcher hears a
    /// change (gathered for `quiet`, so a save is one pass), a full one after the computer
    /// slept and every 15 minutes, none while the backoff or a Retry-After says to wait.
    /// `pass` runs one pass and is told why (`start`, `changed`, `wake`, `interval`,
    /// `rescan`); a watcher that cannot start leaves the clock alone in charge.
    pub fn watch(
        &self,
        stop: &std::sync::atomic::AtomicBool,
        quiet: std::time::Duration,
        mut pass: impl FnMut(&str) -> PassSummary,
    ) {
        use std::{
            sync::atomic::Ordering,
            time::{Duration, Instant, SystemTime},
        };
        let watcher = super::watcher::Watcher::start(&self.root).ok();
        let mut clock = super::watcher::RescanClock::default();
        let mut reason: Option<String> = None;
        while !stop.load(Ordering::SeqCst) {
            let due = clock.due(SystemTime::now(), Instant::now());
            if let Some(why) = reason.take().or_else(|| due.map(str::to_string)) {
                let wait = self.wait();
                if !wait.is_zero() {
                    // Asked to wait: the reason stays for when the wait is over.
                    reason = Some(why);
                    std::thread::sleep(wait.min(Duration::from_secs(1)));
                    continue;
                }
                pass(&why);
                clock.ran(Instant::now());
            }
            let heard = match &watcher {
                Some(w) => w.wait(Duration::from_secs(1), quiet),
                None => {
                    std::thread::sleep(Duration::from_secs(1));
                    None
                }
            };
            reason = match heard {
                Some(super::watcher::Wake::Changed(_)) => Some(String::from("changed")),
                Some(super::watcher::Wake::Rescan(_)) => Some(String::from("rescan")),
                None => None,
            };
        }
    }

    fn note(&self, problem: Problem) {
        self.found
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(problem);
    }
}

/// Whether a pass-wide problem means the pass refused to change anything.
fn refuses(code: Code) -> bool {
    matches!(
        code,
        Code::RootMissing
            | Code::RootReplaced
            | Code::RemoteRollback
            | Code::RemoteIndexUnreadable
            | Code::UpdateNeeded
    )
}

/// The exit of a pass that failed with a problem of `code`: the drive out of reach, the
/// credentials refused or a moment's failure are listed and the pass is done (the next one
/// tries again); a refusal is 2; anything else failed (1).
fn exit_of_failure(code: Code) -> i32 {
    match code {
        Code::RemoteOffline | Code::SignInNeeded | Code::Transient | Code::RateLimited => 0,
        Code::MassDeleteHeld => 3,
        code if refuses(code) => 2,
        _ => 1,
    }
}

/// The state a local step leaves a file in, for the loop detector: its content, or gone.
fn loop_state(step: &Step<'_>) -> Option<(String, Option<String>)> {
    match *step {
        Step::Download { key, hash, .. } => Some((key.to_string(), Some(hash.to_string()))),
        Step::Delete { key, .. } => Some((key.to_string(), None)),
        Step::Conflict { .. } | Step::Move { .. } => None,
    }
}

impl Observer for Engine {
    fn plan(
        &self,
        actions: &mut Vec<Action>,
        remote: &RemoteIndex,
        _base: &BTreeMap<String, BaseEntry>,
        problems: &mut Vec<Problem>,
    ) {
        let loops = self.loops.lock().unwrap_or_else(PoisonError::into_inner);
        let now = self.clock.now();
        actions.retain(|action| {
            let (key, next) = match action {
                Action::Download { key } | Action::Conflict { key, .. } => {
                    (key, remote.files.get(key).map(|f| f.hash.clone()))
                }
                Action::DeleteLocal { key } => (key, None),
                _ => return true,
            };
            if !loops.frozen(key, &next, now) {
                return true;
            }
            problems.push(Problem::new(
                Code::LoopFrozen,
                key,
                "it keeps changing back and forth between the same versions; it is left as \
                 it is until you choose",
            ));
            false
        });
    }

    fn begin(&self, step: &Step<'_>) -> CloudResult<Option<Ticket>> {
        match step.intent() {
            Some(intent) => self.journal.begin(intent).map(Some),
            None => Ok(None),
        }
    }

    fn finished(&self, step: &Step<'_>, ticket: Option<Ticket>, ok: bool, path: Option<&Path>) {
        if let Some(ticket) = ticket {
            if let Err(e) = self.journal.done(ticket) {
                self.note(Problem::new(Code::IoError, "", e.to_string()));
            }
        }
        if !ok {
            return;
        }
        let now = self.clock.now();
        if let Some(op) = step.op(now) {
            if let Err(e) = self.oplog.record(&op) {
                self.note(Problem::new(Code::IoError, &op.key, e.to_string()));
            }
        }
        if let Some((key, state)) = loop_state(step) {
            self.loops
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .applied(&key, state, now);
        }
        if let (Step::Download { key, writer, .. }, Some(path)) = (step, path) {
            if quarantine::supported() && !self.writers.is_own(writer) {
                if let Err(e) = quarantine::mark(path, now as i64) {
                    self.note(Problem::new(
                        Code::IoError,
                        key,
                        format!("it came from {writer} and could not be marked as downloaded: {e}"),
                    ));
                }
            }
        }
    }

    fn point(&self, name: &str) {
        if let Some(points) = &self.points {
            points(name);
        }
    }
}

/// [`CloudError`] of a pass the engine refused before it started (a pairing that is not
/// there): the command line says it and exits 2.
#[must_use]
pub fn not_paired(state_dir: &Path) -> CloudError {
    CloudError::failed(format!("{} holds no pairing", state_dir.display()))
}
