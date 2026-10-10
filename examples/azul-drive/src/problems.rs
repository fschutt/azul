//! What AzDrive shows of a drive's errors (C11, D33): azcloud-kit's table
//! ([`azcloud_kit::user_errors`]) in the user's language - from the environment's locale -
//! with the request ID as the error ID. A storage error reads as the table's text wherever
//! AzDrive words it (a listing, a transfer); the drive in view keeps its problem in the status
//! line (a transient one only after [`TRANSIENT_QUIET_SECS`]) until the drive answers again,
//! and one the user must act on (sign in again, a full or unpaid drive) is a system
//! notification at once, a transient one after [`TRANSIENT_NOTIFY_SECS`] (the period timer
//! asks), each once for as long as it lasts and at most once in [`NOTIFY_EVERY_SECS`] per
//! drive.
//!
//! On stdout: `AZDRIVE_PROBLEM <drive id> <code> <request id or ->` when a drive's problem is
//! recorded, `AZDRIVE_PROBLEM_NOTIFIED <drive id>` with its notification,
//! `AZDRIVE_PROBLEM_GONE <drive id>` when it answers again.

use std::{collections::HashMap, sync::OnceLock};

use azcloud_kit::{
    user_errors::{Class, Lang},
    UserError,
};
use azul::{notification::Notification, prelude::*};
use azul_storage::DriveError;

use crate::{actions::now_secs, DriveState};

/// A transient problem stays out of sight this long.
pub(crate) const TRANSIENT_QUIET_SECS: u64 = 120;
/// One notification per drive in this time at most.
pub(crate) const NOTIFY_EVERY_SECS: u64 = 3_600;
/// A transient problem that lasts this long notifies (once).
pub(crate) const TRANSIENT_NOTIFY_SECS: u64 = 1_800;

/// The language of the environment's locale: `LC_ALL`, `LC_MESSAGES`, `LANG` - the first one
/// set and not empty ([`Lang::from_locale`]).
#[must_use]
pub(crate) fn lang_from(var: impl Fn(&str) -> Option<String>) -> Lang {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|name| var(name))
        .find(|value| !value.trim().is_empty())
        .map_or(Lang::En, |locale| Lang::from_locale(&locale))
}

/// This run's language (read once).
#[must_use]
pub(crate) fn lang() -> Lang {
    static LANG: OnceLock<Lang> = OnceLock::new();
    *LANG.get_or_init(|| lang_from(|name| std::env::var(name).ok()))
}

/// The text of `e` in `lang`: a storage or token server error as the table words it, with its
/// error ID; one the user caused (no such file, a name that cannot be one) as before.
#[must_use]
pub(crate) fn describe_in(e: &DriveError, lang: Lang) -> String {
    UserError::from_drive_error(e).map_or_else(|| e.to_string(), |user| user.message(lang))
}

/// [`describe_in`] this run's language.
#[must_use]
pub(crate) fn describe(e: &DriveError) -> String {
    describe_in(e, lang())
}

/// A drive's problem and since when it lasts.
#[derive(Debug, Clone)]
struct Lasting {
    problem: UserError,
    since: u64,
    /// It notified already (once for as long as it lasts).
    notified: bool,
}

impl Lasting {
    /// Whether it asks for its notification at `now`: one the user must act on at once, a
    /// transient one after [`TRANSIENT_NOTIFY_SECS`] - each once.
    fn wants_notice(&self, now: u64) -> bool {
        !self.notified
            && (self.problem.notifies()
                || (self.problem.class() == Class::Retry
                    && now >= self.since.saturating_add(TRANSIENT_NOTIFY_SECS)))
    }
}

/// The drives' problems as the window keeps them.
#[derive(Debug, Default)]
pub(crate) struct Problems {
    by_drive: HashMap<String, Lasting>,
    /// When each drive last notified.
    notified: HashMap<String, u64>,
}

impl Problems {
    /// `problem` of `drive_id` at `now` (the same code again keeps its start); whether it asks
    /// for a notification now - one the user must act on, or a transient one that lasted
    /// [`TRANSIENT_NOTIFY_SECS`], once each, the drive's first in an hour.
    pub(crate) fn record(&mut self, drive_id: &str, problem: UserError, now: u64) -> bool {
        let (since, notified) = match self.by_drive.get(drive_id) {
            Some(lasting) if lasting.problem.code == problem.code => {
                (lasting.since, lasting.notified)
            }
            _ => (now, false),
        };
        let mut lasting = Lasting {
            problem,
            since,
            notified,
        };
        let notify = lasting.wants_notice(now) && self.may_notify(drive_id, now);
        if notify {
            lasting.notified = true;
            self.notified.insert(drive_id.to_string(), now);
        }
        self.by_drive.insert(drive_id.to_string(), lasting);
        notify
    }

    /// The problems that ask for their notification at `now` without a new failure (a
    /// transient one nobody asked about again, its half hour up), marked as notified - for the
    /// timer.
    pub(crate) fn due(&mut self, now: u64) -> Vec<(String, UserError)> {
        let mut drives: Vec<String> = self.by_drive.keys().cloned().collect();
        drives.sort();
        let mut due = Vec::new();
        for drive_id in drives {
            let wants = self
                .by_drive
                .get(&drive_id)
                .is_some_and(|lasting| lasting.wants_notice(now));
            if !wants || !self.may_notify(&drive_id, now) {
                continue;
            }
            if let Some(lasting) = self.by_drive.get_mut(&drive_id) {
                lasting.notified = true;
                due.push((drive_id.clone(), lasting.problem.clone()));
            }
            self.notified.insert(drive_id, now);
        }
        due
    }

    /// Whether `drive_id` may notify at `now`: its last notification is an hour old.
    fn may_notify(&self, drive_id: &str, now: u64) -> bool {
        self.notified
            .get(drive_id)
            .is_none_or(|at| now >= at.saturating_add(NOTIFY_EVERY_SECS))
    }

    /// `drive_id` answers again; whether it had a problem.
    pub(crate) fn clear(&mut self, drive_id: &str) -> bool {
        self.by_drive.remove(drive_id).is_some()
    }

    /// The problem of `drive_id` to show at `now`: a transient one only after
    /// [`TRANSIENT_QUIET_SECS`].
    #[must_use]
    pub(crate) fn shown(&self, drive_id: &str, now: u64) -> Option<&UserError> {
        let lasting = self.by_drive.get(drive_id)?;
        let quiet = lasting.problem.class() == Class::Retry
            && now < lasting.since.saturating_add(TRANSIENT_QUIET_SECS);
        (!quiet).then_some(&lasting.problem)
    }
}

/// A listing of the drive in view (`serial`) met `problem`: kept for the status line, said
/// on stdout, and a notification when it asks for one.
pub(crate) fn drive_problem(
    info: &mut CallbackInfo,
    s: &mut DriveState,
    serial: u64,
    problem: UserError,
) {
    if serial != s.list_serial {
        return;
    }
    let Some(drive_id) = s.current_drive_id() else {
        return;
    };
    let id = problem.request_id.clone().unwrap_or_else(|| String::from("-"));
    println!("AZDRIVE_PROBLEM {drive_id} {} {id}", problem.code.as_str());
    let text = problem.message(lang());
    if s.problems.record(&drive_id, problem, now_secs()) {
        notify(info, s, &drive_id, &text);
    }
}

/// The notifications the timer finds due: a transient problem nobody asked about again whose
/// half hour is up.
pub(crate) fn notify_due(info: &mut CallbackInfo, s: &mut DriveState) {
    for (drive_id, problem) in s.problems.due(now_secs()) {
        notify(info, s, &drive_id, &problem.message(lang()));
    }
}

/// The system notification of `drive_id`'s problem `text` (`AZDRIVE_PROBLEM_NOTIFIED
/// <drive>` on stdout).
fn notify(info: &mut CallbackInfo, s: &DriveState, drive_id: &str, text: &str) {
    let name = s.drive_name(&crate::browse::Place::folder(drive_id, ""));
    println!("AZDRIVE_PROBLEM_NOTIFIED {drive_id}");
    info.post_notification(
        Notification::create(format!("azdrive-problem-{drive_id}"), "AzDrive")
            .with_body(format!("{name}: {text}")),
    );
}

/// The drive in view answered a listing: its problem is gone.
pub(crate) fn drive_answered(s: &mut DriveState) {
    if let Some(drive_id) = s.current_drive_id() {
        if s.problems.clear(&drive_id) {
            println!("AZDRIVE_PROBLEM_GONE {drive_id}");
        }
    }
}

/// The status line's words for the drive in view, when it has a problem to show.
#[must_use]
pub(crate) fn status_of(s: &DriveState) -> Option<String> {
    let drive_id = s.current_drive_id()?;
    s.problems
        .shown(&drive_id, now_secs())
        .map(|problem| problem.message(lang()))
}
