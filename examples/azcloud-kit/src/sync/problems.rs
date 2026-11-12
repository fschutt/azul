//! The per-file problem list of a synced folder. Every file a pass skipped, refused, held,
//! froze or failed is listed with a code and a reason, and stays listed until a pass finds it
//! resolved: "up to date" means nothing is pending and nothing was skipped.
//!
//! A pass collects its [`Problem`]s ([`super::SyncReport::problems`], plus what
//! [`from_report`] reads out of the report's sentences), and [`ProblemList::publish`] turns
//! them into the folder's list, kept in the state folder (`problems.json`, 0600: it names
//! files). The codes are the sync test harness's vocabulary, so the same scenario checks the
//! engine and the reference.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    path::Path,
};

use serde::{Deserialize, Serialize};

use super::SyncReport;
use crate::{
    error::{CloudError, CloudResult},
    state::{read_json, write_json},
    user_errors::{Class, Code as UserCode, UserError},
};

/// The list's file in a pairing's state folder.
pub const PROBLEMS_FILE: &str = "problems.json";

/// The most problems of one code a list names one by one (X-DELTA-EDGE-3): a folder with a hundred
/// thousand names no drive takes, or a hostile index with that many bad keys, is no hundred thousand
/// rows in a scan, a report and `problems.json` - the first of them are listed and one row of the
/// code says how many more there are ([`Problem::more`], [`Tally`]).
pub const LISTED_PER_CODE: usize = 1_000;

/// What kind of problem a file has. Serialized in kebab case (`symlink-skipped`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Code {
    /// A symbolic link in the folder: never followed, not synced.
    SymlinkSkipped,
    /// A FIFO, socket or device file: never opened.
    SpecialFile,
    /// A local name that can be no key (invalid UTF-8, a control character).
    BadName,
    /// A name on the drive that can be no file anywhere (`..`, NUL, a drive letter).
    BadRemoteName,
    /// A name on the drive this system or disk cannot hold; it stays on the drive.
    InvalidHere,
    /// A name valid here that another system cannot hold (a warning).
    Portability,
    /// A name that can mislead (a right-to-left override, invisible characters).
    DisplayRisk,
    /// Two names this disk takes for one; the second stays on the drive.
    NameCollision,
    /// A file here where the drive has a folder, or the other way round.
    TypeConflict,
    /// A file of the drive under a local link or another disk: nothing is written through it.
    ThroughLink,
    /// A link from another device that is not created.
    LinkRefused,
    /// Another disk mounted inside the folder: never walked into.
    MountPoint,
    /// A hard link to a file outside the folder.
    Hardlink,
    /// A file or folder that cannot be read: unknown, never a delete.
    Unreadable,
    /// The file changed while the pass read it; the next pass takes it.
    ChangedDuringSync,
    /// The file was modified a moment ago and is taken once it is quiet.
    Settling,
    /// An app is saving the file (its old bytes sit in a temp file).
    SaveInProgress,
    /// Another app holds the file locked; retried with backoff.
    Locked,
    /// More bytes than this device or disk takes.
    TooLarge,
    /// Downloading it would eat into the disk's reserve.
    DiskReserve,
    /// The disk is full.
    DiskFull,
    /// The drive's quota is used up.
    QuotaExceeded,
    /// The drive failed for it (a 5xx, a dropped connection); retried with backoff.
    Transient,
    /// The drive asked to wait (429).
    RateLimited,
    /// The drive cannot be reached; nothing changed.
    RemoteOffline,
    /// The drive refused this device's credentials: sign in again (edits are kept).
    SignInNeeded,
    /// The drive's index uses a format this version does not know: read-only until updated.
    UpdateNeeded,
    /// A remote entry that cannot be read (it fails to decrypt, it names no content).
    RemoteEntryUnreadable,
    /// The drive's whole index cannot be read; nothing changed.
    RemoteIndexUnreadable,
    /// The drive answered an index older than one seen before; nothing changed.
    RemoteRollback,
    /// The drive was restored from its backup; newer files are sent again from here.
    ServerRestored,
    /// The drive's bytes do not match their hash; the old version stays.
    HashMismatch,
    /// A compressed file that would grow past its declared size.
    DecompressionBomb,
    /// A plan that deletes too much waits for the user.
    MassDeleteHeld,
    /// A file that keeps flipping between the same versions is frozen.
    LoopFrozen,
    /// A file deleted on the drive while this device was away is kept and asked about.
    DeletedElsewhereKept,
    /// The synced folder is gone (a disk not mounted?).
    RootMissing,
    /// The path is no longer the folder that was paired.
    RootReplaced,
    /// The base could not be read: the pass pairs afresh, deleting nothing.
    StateReset,
    /// Someone changed the shared ignore file: these files stopped syncing.
    IgnoreChange,
    /// A file renamed into a name another system cannot hold.
    RenamedToInvalid,
    /// Other devices kept committing; the next pass tries again.
    Busy,
    /// A local read or write failed.
    IoError,
    /// A conflict held for the user (keep mine, take theirs, keep both).
    ConflictHeld,
    /// Uploads paused by the burst guard until the user answers.
    UploadsPaused,
    /// A transfer held back by a limit (a metered network).
    HeldBack,
    /// The drive answered that it moved to another address (a 301 or 308): nothing changed,
    /// the files here and their queue stay until the drive answers at its new address.
    ServerMoved,
    /// The drive takes no new bytes (unpaid past its grace, a lockdown, an operator's hold, a
    /// ban): the files here stay, and go up once it takes writes again.
    ReadOnly,
    /// Items of the folder carry extended attributes, a resource fork or flags, which are not
    /// synced (their content is): a note, once per folder.
    XattrsNotSynced,
    /// Some folders are not watched (the system's limit of watches is used up, or a folder cannot
    /// be entered): a change in them wakes nothing, and a pass a minute later finds it.
    WatchLimited,
    /// A check this device could not make and went on without: the sentence says which (the disk's
    /// free space, on a share that does not say, so the reserve was not enforced).
    Unchecked,
    /// Anything else, with its sentence.
    Other,
}

impl Code {
    /// The code as the list writes it (`symlink-skipped`).
    #[must_use]
    pub fn as_str(self) -> String {
        serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap_or_default()
    }

    /// Whether it blocks "up to date" (an error) or only informs (a warning).
    #[must_use]
    pub fn severity(self) -> Severity {
        match self {
            Code::Portability
            | Code::DisplayRisk
            | Code::Settling
            | Code::RemoteOffline
            | Code::Transient
            | Code::RateLimited
            | Code::ServerRestored
            | Code::StateReset
            | Code::Busy
            | Code::HeldBack
            | Code::XattrsNotSynced
            | Code::WatchLimited
            | Code::Unchecked
            | Code::ChangedDuringSync => Severity::Warning,
            _ => Severity::Error,
        }
    }

    /// Whether it keeps a change of this device from the drive while it lasts (the notes about
    /// names, links and passing moments do not).
    #[must_use]
    pub fn holds_back(self) -> bool {
        !matches!(
            self,
            Code::Portability
                | Code::DisplayRisk
                | Code::Hardlink
                | Code::Settling
                | Code::ChangedDuringSync
                | Code::ServerRestored
                | Code::StateReset
                | Code::WatchLimited
                | Code::Unchecked
        )
    }

    /// Whether the problem is about the whole pass (no file of its own): a pass that stopped
    /// early keeps the files' problems of the last pass that got that far.
    #[must_use]
    pub fn is_pass_wide(self) -> bool {
        matches!(
            self,
            Code::RemoteOffline
                | Code::SignInNeeded
                | Code::UpdateNeeded
                | Code::RemoteIndexUnreadable
                | Code::RemoteRollback
                | Code::ServerRestored
                | Code::MassDeleteHeld
                | Code::RootMissing
                | Code::RootReplaced
                | Code::StateReset
                | Code::Busy
                | Code::UploadsPaused
                | Code::ServerMoved
                | Code::WatchLimited
                | Code::Unchecked
        )
    }
}

/// Whether a problem stops "up to date".
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

/// One file's problem.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Problem {
    pub code: Code,
    /// The file's key as the folder or the drive has it (empty for the whole pass). Shown
    /// through [`escaped`], never as it is.
    pub key: String,
    /// The reason, as a sentence.
    pub detail: String,
    pub severity: Severity,
    /// When a pass first listed it (seconds), carried while it stays listed; 0 until published.
    #[serde(default)]
    pub since: i64,
    /// More than 0: this row names no file (its `key` is empty) and stands for that many other
    /// problems of its code that are not listed one by one ([`LISTED_PER_CODE`]).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub more: u64,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

impl Problem {
    /// The problem `code` of `key`, with its sentence and the code's severity.
    #[must_use]
    pub fn new(code: Code, key: &str, detail: impl Into<String>) -> Problem {
        Problem {
            code,
            key: key.to_string(),
            detail: detail.into(),
            severity: code.severity(),
            since: 0,
            more: 0,
        }
    }

    /// The row that says `more` other problems of `code` are not listed one by one.
    #[must_use]
    pub fn left_out(code: Code, more: u64) -> Problem {
        Problem {
            more,
            ..Problem::new(code, "", more_sentence(more))
        }
    }

    /// The problem a failed transfer or write of `key` is: by what the error says, not by its
    /// words (a busy node is `transient`, a 429 `rate-limited`, a full disk `disk-full`, a
    /// refused device `sign-in-needed`).
    #[must_use]
    pub fn of_error(key: &str, error: &CloudError) -> Problem {
        Problem::new(code_of_error(error), key, error.to_string())
    }

    /// The key as people should see it ([`escaped`]).
    #[must_use]
    pub fn shown_key(&self) -> String {
        escaped(&self.key)
    }
}

/// What the row of `more` problems left out says.
fn more_sentence(more: u64) -> String {
    format!(
        "and {more} more of this kind: only the first {LISTED_PER_CODE} of them are listed one by one"
    )
}

/// How many problems of each code a producer has told - a scan, a pass's report - so that its list
/// keeps the first [`LISTED_PER_CODE`] of a code and only counts the rest (X-DELTA-EDGE-3). The
/// producer asks [`Tally::admit`] before it keeps a problem, and [`Tally::close`]s its list at the
/// end with one row per code that overflowed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    /// Problems told per code, kept or not.
    told: BTreeMap<Code, u64>,
    /// Problems another list already left out ([`Problem::more`]), per code.
    left: BTreeMap<Code, u64>,
}

impl Tally {
    /// Counts one more problem of `code`; whether it is kept (the code has room left).
    pub fn admit(&mut self, code: Code) -> bool {
        let told = self.told.entry(code).or_default();
        *told += 1;
        *told <= LISTED_PER_CODE as u64
    }

    /// Counts `more` problems of `code` that were left out before they came here.
    pub fn leave_out(&mut self, code: Code, more: u64) {
        *self.left.entry(code).or_default() += more;
    }

    /// Counts everything `other` told as told here too (a scan's problems in its pass's).
    pub fn absorb(&mut self, other: &Tally) {
        for (code, told) in &other.told {
            *self.told.entry(*code).or_default() += told;
        }
        for (code, more) in &other.left {
            self.leave_out(*code, *more);
        }
    }

    /// How many problems of `code` are not kept.
    #[must_use]
    pub fn left_out(&self, code: Code) -> u64 {
        let over = self
            .told
            .get(&code)
            .copied()
            .unwrap_or(0)
            .saturating_sub(LISTED_PER_CODE as u64);
        over + self.left.get(&code).copied().unwrap_or(0)
    }

    /// Puts into `list` one row per code that left problems out: its [`Problem::left_out`] row,
    /// set to the whole count when `list` has one already (so closing twice says the same).
    pub fn close(&self, list: &mut Vec<Problem>) {
        let codes: BTreeSet<Code> = self.told.keys().chain(self.left.keys()).copied().collect();
        for code in codes {
            let more = self.left_out(code);
            if more == 0 {
                continue;
            }
            match list.iter_mut().find(|p| p.code == code && p.more > 0) {
                Some(row) => {
                    row.more = more;
                    row.detail = more_sentence(more);
                }
                None => list.push(Problem::left_out(code, more)),
            }
        }
    }
}

/// The code of `error` for one file.
#[must_use]
pub fn code_of_error(error: &CloudError) -> Code {
    if let CloudError::Drive(azul_storage::DriveError::Service(service)) = error.root() {
        // The service's own code says more than the status: a read-only drive answers 403
        // like revoked keys do, but signing in again changes nothing there.
        match &service.azlin_error {
            Some(
                azul_storage::AzlinError::ReadOnlyUnpaid
                | azul_storage::AzlinError::ReadOnlyLockdown
                | azul_storage::AzlinError::ReadOnlyOperator
                | azul_storage::AzlinError::ReadOnlyMoving
                | azul_storage::AzlinError::DriveBanned
                | azul_storage::AzlinError::DriveArchived,
            ) => return Code::ReadOnly,
            Some(azul_storage::AzlinError::ObjectTooLarge) => return Code::TooLarge,
            Some(azul_storage::AzlinError::QuotaExceeded) => return Code::QuotaExceeded,
            _ => {}
        }
        match service.status {
            301 | 308 => return Code::ServerMoved,
            413 => return Code::TooLarge,
            429 => return Code::RateLimited,
            507 => return Code::QuotaExceeded,
            401 | 403 => return Code::SignInNeeded,
            500..=599 => return Code::Transient,
            _ => {}
        }
    }
    if let CloudError::Drive(azul_storage::DriveError::Corrupt { .. }) = error.root() {
        return Code::HashMismatch;
    }
    // An encrypted drive this device holds no key for (not joined yet, a forgotten passphrase,
    // removed by a key rotation): nothing here can be read or written until it joins again
    // with a code or unlocks with the recovery code - for the user that is signing in again.
    if let CloudError::Drive(azul_storage::DriveError::Locked { .. }) = error.root() {
        return Code::SignInNeeded;
    }
    // A drive key this device refused (the bucket offered another than its own): nothing is
    // read or written until the recovery code or a join code gives the drive's key back.
    if let CloudError::Drive(azul_storage::DriveError::KeyRefused { .. }) = error.root() {
        return Code::SignInNeeded;
    }
    if let CloudError::Drive(azul_storage::DriveError::Transport(_)) = error.root() {
        return Code::RemoteOffline;
    }
    if let CloudError::Io(text) | CloudError::Drive(azul_storage::DriveError::Io(text)) =
        error.root()
    {
        return code_of_io_text(text);
    }
    match UserError::from_cloud_error(error) {
        Some(user) if user.code == UserCode::CredentialsRevoked => Code::SignInNeeded,
        Some(user) if user.class() == Class::Retry => Code::Transient,
        _ => code_of_sentence(&error.to_string()),
    }
}

/// The code of an I/O error's sentence (the kit keeps I/O errors as text).
fn code_of_io_text(text: &str) -> Code {
    let lower = text.to_ascii_lowercase();
    if lower.contains("no space left") || lower.contains("disk full") || lower.contains("enospc")
    {
        Code::DiskFull
    } else if lower.contains("being used by another process")
        || lower.contains("resource busy")
        || lower.contains("locked")
    {
        Code::Locked
    } else if lower.contains("operation not permitted") {
        // EPERM on a write or a rename here: the file or its folder is made immutable (an app
        // holding it, chflags uchg, chattr +i), the way Windows locks an open file.
        Code::Locked
    } else if lower.contains("permission denied") {
        Code::Unreadable
    } else {
        Code::IoError
    }
}

/// The code of one of the sync's sentences about a file (its `skipped` and `file_errors`
/// reasons). The structured problems a pass records come first; this reads the rest.
#[must_use]
pub fn code_of_sentence(why: &str) -> Code {
    const WORDS: &[(&str, Code)] = &[
        ("symbolic link", Code::SymlinkSkipped),
        ("not a regular file", Code::SpecialFile),
        ("cannot be a key", Code::BadName),
        ("name is not utf-8", Code::BadName),
        ("same name to this disk", Code::NameCollision),
        ("another disk", Code::MountPoint),
        ("changed while it was synced", Code::ChangedDuringSync),
        ("changed during the run", Code::ChangedDuringSync),
        ("cannot be read", Code::Unreadable),
        ("is damaged", Code::HashMismatch),
        // The drive names a file whose content it no longer holds: what it says of the file
        // cannot be read.
        ("missing from the drive", Code::RemoteEntryUnreadable),
        ("blake3 is not", Code::HashMismatch),
        ("larger than", Code::TooLarge),
        ("too big", Code::TooLarge),
        ("no space left", Code::DiskFull),
        ("quota", Code::QuotaExceeded),
        ("drive is full", Code::QuotaExceeded),
        ("sign in", Code::SignInNeeded),
        // DriveError::Locked's sentence, when only the text came through.
        ("has no key for", Code::SignInNeeded),
        ("cannot be reached", Code::RemoteOffline),
        ("timed out", Code::Transient),
        ("connection", Code::Transient),
        ("busy", Code::Transient),
    ];
    let lower = why.to_lowercase();
    WORDS
        .iter()
        .find(|(word, _)| lower.contains(word))
        .map_or(Code::Other, |(_, code)| *code)
}

/// The problems of a finished pass: its structured ones first, then what its sentences say
/// (skipped files, failed files, files that changed during it, a held mass delete, paused
/// uploads, a newer format). A key the structured problems name already is not read twice.
#[must_use]
pub fn from_report(report: &SyncReport) -> Vec<Problem> {
    let mut out: Vec<Problem> = report.problems.clone();
    // The keys the problems name, asked for once each: a pass with a hundred thousand skipped or
    // failed files asked the list so far for every one (X-DELTA-EDGE-3).
    let mut named: HashSet<String> = out
        .iter()
        .filter(|p| !p.key.is_empty())
        .map(|p| p.key.clone())
        .collect();
    // A whole-pass row (no key) is never "named"; any other key is told once.
    let mut first = |key: &str| key.is_empty() || named.insert(key.to_string());
    for (key, why) in report.skipped.iter().chain(report.file_errors.iter()) {
        if first(key) {
            out.push(Problem::new(code_of_sentence(why), key, why.clone()));
        }
    }
    for line in &report.changed_during_sync {
        // `key: why` - the reason has no ": " of its own, the key may.
        let key = line.rfind(": ").map_or(line.as_str(), |i| &line[..i]);
        if first(key) {
            out.push(Problem::new(Code::ChangedDuringSync, key, line.clone()));
        }
    }
    if let Some(asked) = &report.mass_delete {
        out.push(Problem::new(
            Code::MassDeleteHeld,
            "",
            format!(
                "{} of {} files would be deleted {}; nothing changed until you confirm",
                asked.count,
                asked.of,
                if asked.here { "here" } else { "on the drive" }
            ),
        ));
    }
    if let Some(pause) = &report.paused {
        out.push(Problem::new(Code::UploadsPaused, "", pause.describe()));
    }
    if !report.newer_format.is_empty() {
        out.push(Problem::new(
            Code::UpdateNeeded,
            "",
            format!(
                "the drive uses what this version does not know ({}); it is left as it is \
                 until the app is updated",
                report.newer_format.join(", ")
            ),
        ));
    }
    for held in &report.held {
        out.push(Problem::new(
            Code::ConflictHeld,
            &held.key,
            format!("changed here and by {} on the drive", held.there_device),
        ));
    }
    for key in &report.held_back {
        out.push(Problem::new(
            Code::HeldBack,
            key,
            "bigger than the transfer limit; it moves on a pass without the limit",
        ));
    }
    out
}

/// The problem a whole pass failed with (it changed nothing): the drive offline, the
/// credentials refused, the index unreadable, or the error's own code.
#[must_use]
pub fn of_failed_pass(error: &CloudError) -> Problem {
    let code = match code_of_error(error) {
        _ if error.to_string().contains("index cannot be read") => Code::RemoteIndexUnreadable,
        Code::Other if error.to_string().contains("--allow-mass-delete") => Code::MassDeleteHeld,
        code => code,
    };
    Problem::new(code, "", error.to_string())
}

/// The folder's list as the last passes left it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProblemList {
    pub problems: Vec<Problem>,
    /// When it was last published (seconds).
    #[serde(default)]
    pub updated_at: i64,
}

impl ProblemList {
    /// The list kept in `state_dir` (empty when there is none).
    ///
    /// # Errors
    ///
    /// When the file is there and cannot be read.
    pub fn load(state_dir: &Path) -> CloudResult<ProblemList> {
        Ok(read_json(&state_dir.join(PROBLEMS_FILE))?.unwrap_or_default())
    }

    /// Writes the list into `state_dir` (0600).
    ///
    /// # Errors
    ///
    /// When it cannot be written.
    pub fn save(&self, state_dir: &Path) -> CloudResult<()> {
        write_json(&state_dir.join(PROBLEMS_FILE), self, true)
    }

    /// Makes the problems of a pass the list. A problem listed before keeps its `since`. A
    /// pass that did not get to its files (`complete` false: offline, refused, held) keeps
    /// the files' problems of the last one that did, so a skipped file never drops out of
    /// the list just because the drive could not be reached.
    ///
    /// One row per (code, key), the first told winning; at most [`LISTED_PER_CODE`] rows of a
    /// code - the first by key - and one row of it that says how many more there were
    /// ([`Problem::more`]): sorted and deduplicated, never a scan of the list so far for every
    /// problem (X-DELTA-EDGE-3: 200,000 problems took minutes).
    pub fn publish(&mut self, pass: Vec<Problem>, complete: bool, now: i64) {
        let next = {
            let first_listed: HashMap<(Code, &str), i64> = self
                .problems
                .iter()
                .map(|p| ((p.code, p.key.as_str()), p.since))
                .collect();
            let mut rows: Vec<Problem> = Vec::new();
            // Problems a row of "and N more" stands for, per code: the first count told of a code
            // is the one kept (a pass that could not look again keeps the last pass's row).
            let mut left: BTreeMap<Code, u64> = BTreeMap::new();
            let mut kept_row: BTreeSet<Code> = BTreeSet::new();
            if !complete {
                for p in self.problems.iter().filter(|p| !p.code.is_pass_wide()) {
                    if p.more > 0 {
                        left.insert(p.code, p.more);
                        kept_row.insert(p.code);
                    } else {
                        rows.push(p.clone());
                    }
                }
            }
            for mut p in pass {
                if p.more > 0 {
                    if !kept_row.contains(&p.code) {
                        *left.entry(p.code).or_default() += p.more;
                    }
                    continue;
                }
                p.since = first_listed
                    .get(&(p.code, p.key.as_str()))
                    .copied()
                    .unwrap_or(now);
                rows.push(p);
            }
            // By key, then code; the sort is stable, so of two rows for one (code, key) the one
            // told first (a kept one, then the pass's first) stays.
            rows.sort_by(|a, b| (a.key.as_str(), a.code).cmp(&(b.key.as_str(), b.code)));
            rows.dedup_by(|later, first| later.code == first.code && later.key == first.key);
            let mut tally = Tally::default();
            let mut next: Vec<Problem> = rows.into_iter().filter(|p| tally.admit(p.code)).collect();
            for (code, more) in left {
                tally.leave_out(code, more);
            }
            tally.close(&mut next);
            for row in next.iter_mut().filter(|p| p.more > 0) {
                row.since = first_listed
                    .get(&(row.code, row.key.as_str()))
                    .copied()
                    .unwrap_or(now);
            }
            // The rows that stand for others come last.
            next.sort_by(|a, b| {
                (a.more > 0, a.key.as_str(), a.code).cmp(&(b.more > 0, b.key.as_str(), b.code))
            });
            next
        };
        self.problems = next;
        self.updated_at = now;
    }

    /// Adds `problem` to the list at `now`, outside a pass (a check found it): one row per (code,
    /// key), and past [`LISTED_PER_CODE`] rows of its code the problem is counted in the code's
    /// "and N more" row instead - a read-back that finds a hundred thousand damaged files adds one
    /// row to the list for each of the first thousand only. (A problem past the cap has no row to
    /// be told twice by: every call of it counts.)
    pub fn note(&mut self, mut problem: Problem, now: i64) {
        let named = |p: &Problem| p.code == problem.code && p.key == problem.key;
        if self.problems.iter().any(named) {
            return;
        }
        let rows = self
            .problems
            .iter()
            .filter(|p| p.code == problem.code && p.more == 0)
            .count();
        self.updated_at = now;
        if rows < LISTED_PER_CODE {
            problem.since = now;
            self.problems.push(problem);
            return;
        }
        match self
            .problems
            .iter_mut()
            .find(|p| p.code == problem.code && p.more > 0)
        {
            Some(row) => {
                row.more += 1;
                row.detail = more_sentence(row.more);
            }
            None => {
                let mut row = Problem::left_out(problem.code, 1);
                row.since = now;
                self.problems.push(row);
            }
        }
    }

    /// How many problems the list holds, those its "and N more" rows stand for included.
    #[must_use]
    pub fn total(&self) -> u64 {
        self.problems.iter().map(|p| p.more.max(1)).sum()
    }

    /// Whether nothing blocks "up to date" (warnings only, or nothing).
    #[must_use]
    pub fn up_to_date(&self) -> bool {
        self.problems.iter().all(|p| p.severity == Severity::Warning)
    }

    /// Since when (seconds) something here has not reached the drive: the oldest problem the
    /// list still holds that keeps a change back - a file that did not go up, a pass that
    /// could not reach the drive (offline for a month is the case that matters), a question
    /// waiting - for the "not backed up since" warning. Notes about names and moments (a file
    /// still being written) keep nothing back. `None`: nothing waits.
    #[must_use]
    pub fn unsynced_since(&self) -> Option<i64> {
        self.problems
            .iter()
            .filter(|p| p.since > 0 && p.code.holds_back())
            .map(|p| p.since)
            .min()
    }

    /// The problems of `key`.
    pub fn of_key<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a Problem> + 'a {
        self.problems.iter().filter(move |p| p.key == key)
    }
}

/// `key` as people should see it: every character that can hide what a name is (a bidi
/// control, an invisible one, a look-alike of a dot or a slash, a control character, the
/// replacement for bytes that were no UTF-8) is written as its code point, `<U+202E>`, so a
/// reversed extension cannot pass for another.
#[must_use]
pub fn escaped(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    for c in key.chars() {
        if is_risky(c) {
            out.push_str(&format!("<U+{:04X}>", u32::from(c)));
        } else {
            out.push(c);
        }
    }
    out
}

/// Whether `c` can make a name look like another.
#[must_use]
pub fn is_risky(c: char) -> bool {
    let o = u32::from(c);
    let bidi = matches!(o, 0x061C | 0x200E | 0x200F | 0x202A..=0x202E | 0x2066..=0x2069);
    let invisible = matches!(
        o,
        0x00AD | 0x034F | 0x180E | 0x200B | 0x200C | 0x200D | 0x2060 | 0xFEFF
    );
    let lookalike = matches!(o, 0x2024 | 0x2215 | 0x2044 | 0xFF0F | 0x29F8 | 0xFF0E | 0x3002);
    let control = o < 0x20 || o == 0x7F || (0x80..=0x9F).contains(&o);
    bidi || invisible || lookalike || control || c == char::REPLACEMENT_CHARACTER
}

/// The list as the sync test harness reads it: `[{code, key, detail, severity}]`, keys
/// escaped.
#[must_use]
pub fn harness_json(list: &ProblemList) -> serde_json::Value {
    serde_json::Value::Array(
        list.problems
            .iter()
            .map(|p| {
                let mut row = serde_json::json!({
                    "code": p.code.as_str(),
                    "key": p.shown_key(),
                    "detail": p.detail,
                    "severity": p.severity,
                });
                // A row that stands for others says how many (X-DELTA-EDGE-3).
                if p.more > 0 {
                    row["more"] = serde_json::json!(p.more);
                }
                row
            })
            .collect(),
    )
}
