//! AzDrive's file operations on any drive, as Explorer does them: copy and
//! move (within a drive or across two), with a plan made first (every file
//! under a folder, every name that is taken at the target) so the user
//! decides each conflict - replace, skip, keep both - before a byte moves;
//! delete to a trash folder (local drives) or for good; the names Explorer
//! makes ("a - Copy.txt", "a (2).txt", "New folder (2)"). Everything here
//! blocks: the app runs it on an azul `Thread`. No azul types.

use std::sync::atomic::{AtomicBool, Ordering};

use azul_appkit::l10n::{Phrase, Text};
use azul_storage::{
    key::System,
    names::{Policy, Purpose, Volume},
    xfer, BatchDelete, BatchMove, Drive, DriveError,
};

use crate::browse;

// The plan of a transfer is the storage crate's (`xfer::plan`): AzDrive says it in words and
// queues it.
pub use azul_storage::xfer::{parent_of, Choice as ConflictChoice, Plan, PlannedFile};

/// The trash folder at the root of a local drive: a deleted item goes to
/// `.azdrive-trash/<stamp>/<its key>`, so it can come back where it was.
/// A dot name: hidden unless "Hidden items" is on.
pub const TRASH_FOLDER: &str = ".azdrive-trash/";

// ==== Names ====

/// `archive.tar.gz` -> (`archive.tar`, `.gz`); `.env` and `README` have no
/// extension.
#[must_use]
pub fn split_extension(name: &str) -> (&str, &str) {
    xfer::names::split_extension(name)
}

/// The name Explorer gives a copy pasted into the folder it came from: `a - Copy.txt`, then
/// `a - Copy (2).txt`, ... - `copy` the window's language's word ("Kopie").
#[must_use]
pub fn copy_name(name: &str, copy: &str, taken: &dyn Fn(&str) -> bool) -> String {
    xfer::names::copy_name(name, copy, &mut |n| taken(n))
}

/// [`copy_name`] for a folder (a dot in its name is not an extension).
#[must_use]
pub fn copy_folder_name(name: &str, copy: &str, taken: &dyn Fn(&str) -> bool) -> String {
    xfer::names::copy_folder_name(name, copy, &mut |n| taken(n))
}

/// The name "Keep both files" gives the newcomer: `a (2).txt`, `a (3).txt`.
#[must_use]
pub fn keep_both_name(name: &str, taken: &dyn Fn(&str) -> bool) -> String {
    xfer::names::keep_both_name(name, &mut |n| taken(n))
}

/// A new item's name: `name` when free, else `name (2)`, `name (3)`, ...
/// ("New folder", "New Text Document.txt").
#[must_use]
pub fn new_name(name: &str, taken: &dyn Fn(&str) -> bool) -> String {
    xfer::names::new_name(name, &mut |n| taken(n))
}

/// The characters Explorer refuses in a name.
pub const FORBIDDEN_CHARS: &[char] = &['\\', '/', ':', '*', '?', '"', '<', '>', '|'];

/// Whether `name` can name a file or folder, or why not (a sentence): the characters Explorer
/// refuses, then the one name table's rows for this computer ([`azul_storage::names::Policy`]).
///
/// # Errors
///
/// The sentence, as a message.
pub fn check_name(name: &str) -> Result<(), Text> {
    check_name_on(name, System::here())
}

/// [`check_name`] for a computer of `system` (a test asks for Windows' rows on any computer).
///
/// # Errors
///
/// The sentence, as a message.
pub fn check_name_on(name: &str, system: System) -> Result<(), Text> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(Text::key("azdrive-name-empty"));
    }
    if trimmed == "." || trimmed == ".." {
        return Err(Phrase::new("azdrive-name-reserved")
            .arg("name", trimmed)
            .into());
    }
    if name.chars().any(|c| FORBIDDEN_CHARS.contains(&c)) {
        return Err(Text::key("azdrive-name-forbidden-chars"));
    }
    if name.chars().any(char::is_control) {
        return Err(Text::key("azdrive-name-control-chars"));
    }
    // What this system reads as something else - a device name, a trailing dot or space -
    // comes from the same table every other path check of the app asks (X-AR5a-17).
    let policy = Policy {
        system,
        volume: Volume::default(),
    };
    match policy.admit(name, Purpose::UserTyped) {
        Ok(_) => Ok(()),
        Err(refusal) => Err(Phrase::new("azdrive-name-unusable")
            .arg("why", refusal.why.as_str())
            .into()),
    }
}

// ==== Plans ====

/// What a transfer does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransferKind {
    /// Paste after Copy, "Copy to", Ctrl+drag.
    Copy,
    /// Paste after Cut, "Move to", a drag.
    Move,
    /// A file of this computer to a drive.
    Upload,
    /// A drive's file to this computer.
    Download,
}

impl TransferKind {
    /// The kind as a message's argument (`azdrive-transfer-label`'s `$kind`).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            TransferKind::Copy => "copy",
            TransferKind::Move => "move",
            TransferKind::Upload => "upload",
            TransferKind::Download => "download",
        }
    }

    /// "Copying", "Moving", ...
    #[must_use]
    pub fn verb(self) -> &'static str {
        match self {
            TransferKind::Copy => "Copying",
            TransferKind::Move => "Moving",
            TransferKind::Upload => "Uploading",
            TransferKind::Download => "Downloading",
        }
    }

    /// A move takes the items away from where they were.
    #[must_use]
    pub fn removes_source(self) -> bool {
        self == TransferKind::Move
    }
}

/// One item the user picked: a file, or a folder (`key` ending in `/`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceItem {
    pub key: String,
    pub is_folder: bool,
    /// From the listing; `None` asks the drive when it matters.
    pub size: Option<u64>,
}

impl SourceItem {
    /// The item of a listed entry.
    #[must_use]
    pub fn of(entry: &browse::Entry) -> Self {
        SourceItem {
            key: entry.key.clone(),
            is_folder: entry.is_folder,
            size: entry.size,
        }
    }
}

/// Plans copying or moving `items` of `source` into the folder `target_prefix` of `target`: the
/// storage crate's plan ([`xfer::plan`]) of the items, with the kind's one difference - only a
/// move takes the items away. Whether the two are one drive is the drives' own answer
/// ([`Drive::site`]), not the caller's.
///
/// # Errors
///
/// As [`xfer::plan`].
pub fn plan_transfer(
    source: &dyn Drive,
    items: &[SourceItem],
    target: &dyn Drive,
    target_prefix: &str,
    kind: TransferKind,
    copy: &str,
) -> Result<Plan, DriveError> {
    let items: Vec<xfer::Item> = items
        .iter()
        .map(|item| xfer::Item {
            key: item.key.clone(),
            is_folder: item.is_folder,
            size: item.size,
        })
        .collect();
    xfer::plan(
        source,
        &items,
        target,
        target_prefix,
        kind.removes_source(),
        copy,
    )
}

// ==== Running ====

// How far a running transfer is: the storage crate's, queued and shown here.
pub use azul_storage::xfer::Progress;

/// How a transfer ended.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TransferReport {
    /// Files copied and items renamed.
    pub done: usize,
    pub skipped: usize,
    /// What failed, and why.
    pub failed: Vec<(String, azul_appkit::l10n::Text)>,
    pub cancelled: bool,
    /// The items renamed in place, `(from, to)`: what an Undo puts back, and nothing it did not
    /// do (AZ-U17).
    pub moved: Vec<(String, String)>,
    /// The folders of a move that stay because something is still in them.
    pub left: Vec<String>,
}

/// The stop of a transfer: a flag the window sets. It hears no bytes (the queue's progress comes
/// from the run's own reports).
struct Stop<'a>(&'a AtomicBool);

impl azul_storage::Ctl for Stop<'_> {
    fn bytes(&self, _done: u64) {}

    fn cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Runs `plan` ([`xfer::run`]): renames the whole items, creates the folders, copies every file by
/// its choice - an undecided conflict is skipped, never overwritten, and a name made at the target
/// since the plan is a new conflict, not replaced - and for a move retires what landed and removes
/// the folders it emptied, if nothing else is in them. `cancel` is looked at per chunk of the
/// file being copied, not between files; `report` hears the progress after every chunk, every
/// file and every batch. A failure is noted and the rest goes on.
pub fn run_transfer(
    plan: &Plan,
    source: &dyn Drive,
    target: &dyn Drive,
    kind: TransferKind,
    cancel: &AtomicBool,
    report: &mut dyn FnMut(&Progress),
) -> TransferReport {
    let ran = xfer::run(
        plan,
        source,
        target,
        kind.removes_source(),
        &Stop(cancel),
        report,
    );
    TransferReport {
        done: ran.done,
        skipped: ran.skipped,
        failed: ran
            .failed
            .into_iter()
            .map(|(what, why)| (what, crate::l10n::drive_error_text(&why)))
            .collect(),
        cancelled: ran.cancelled,
        moved: ran.moved,
        left: ran.left,
    }
}

// ==== Delete, trash, undo ====

/// A stamp naming one delete in the trash: `20261001-031500-1`.
#[must_use]
pub fn trash_stamp(unix_secs: u64, serial: u32) -> String {
    let time = i64::try_from(unix_secs)
        .ok()
        .and_then(|secs| chrono::DateTime::from_timestamp(secs, 0))
        .map(|t| t.format("%Y%m%d-%H%M%S").to_string())
        .unwrap_or_else(|| String::from("00000000-000000"));
    format!("{time}-{serial}")
}

/// Where `key` goes in the trash of the delete `stamp`.
#[must_use]
pub fn trash_key(key: &str, stamp: &str) -> String {
    format!("{TRASH_FOLDER}{stamp}/{key}")
}

/// Whether `key` is in the trash folder (deleting it there is for good).
#[must_use]
pub fn is_in_trash(key: &str) -> bool {
    key.starts_with(TRASH_FOLDER)
}

/// The folder of one delete in the trash: `.azdrive-trash/<stamp>/`.
#[must_use]
pub fn trash_folder_of(trash_key: &str) -> Option<String> {
    let rest = trash_key.strip_prefix(TRASH_FOLDER)?;
    let stamp = rest.split('/').next().filter(|s| !s.is_empty())?;
    Some(format!("{TRASH_FOLDER}{stamp}/"))
}

/// What a delete did: the items that went, and the ones that did not. A batch that fails for
/// one item still did the others - and says which (AZ-U17: the first failure used to end the
/// loop, dropping the Undo of the 39 that went).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Deleted {
    /// `(key, where it went)` for every item that went (an empty place for good): what
    /// [`restore_items`] undoes.
    pub gone: Vec<(String, String)>,
    /// The items that stayed, and why.
    pub failed: Vec<(String, DriveError)>,
}

/// Deletes `items`: into the trash of the delete `stamp` (each moved to
/// [`trash_key`]), or for good with `None`. A folder goes as a whole; the files go in one
/// batch each way (an encrypted drive changes its index once, a bucket deletes a thousand
/// keys a request). Every item is tried: what went is in [`Deleted::gone`], what did not in
/// [`Deleted::failed`].
#[must_use]
pub fn delete_items(drive: &dyn Drive, items: &[SourceItem], stamp: Option<&str>) -> Deleted {
    let mut out = Deleted::default();
    let mut into_trash: Vec<(String, String)> = Vec::new();
    let mut for_good: Vec<String> = Vec::new();
    for item in items {
        match stamp {
            Some(stamp) if !is_in_trash(&item.key) => {
                let to = trash_key(&item.key, stamp);
                if item.is_folder {
                    match drive.rename(&item.key, &to) {
                        Ok(()) => out.gone.push((item.key.clone(), to)),
                        Err(e) => out.failed.push((item.key.clone(), e)),
                    }
                } else {
                    into_trash.push((item.key.clone(), to));
                }
            }
            _ => {
                if item.is_folder {
                    // The user's own "delete for good": the one place a folder is removed with
                    // what is in it.
                    match drive.delete_folder(&item.key) {
                        Ok(()) => out.gone.push((item.key.clone(), String::new())),
                        Err(e) => out.failed.push((item.key.clone(), e)),
                    }
                } else {
                    for_good.push(item.key.clone());
                }
            }
        }
    }
    if !into_trash.is_empty() {
        let batch: Vec<BatchMove<'_>> = into_trash
            .iter()
            .map(|(from, to)| BatchMove::new(from, to))
            .collect();
        for ((from, to), result) in into_trash.iter().zip(drive.move_many(&batch)) {
            match result {
                Ok(()) => out.gone.push((from.clone(), to.clone())),
                Err(e) => out.failed.push((from.clone(), e)),
            }
        }
    }
    if !for_good.is_empty() {
        let batch: Vec<BatchDelete<'_>> =
            for_good.iter().map(|key| BatchDelete::new(key)).collect();
        for (key, result) in for_good.iter().zip(drive.delete_many(&batch)) {
            match result {
                Ok(()) => out.gone.push((key.clone(), String::new())),
                Err(e) => out.failed.push((key.clone(), e)),
            }
        }
    }
    out
}

/// Undoes a delete into the trash: every item renamed back - one that cannot be does not stop
/// the others - and the delete's folder in the trash removed when nothing is left in it (never
/// with what is in it: [`Drive::remove_empty_folder`]). The first failure, if there was one.
///
/// # Errors
///
/// The first item that could not be put back.
pub fn restore_items(drive: &dyn Drive, gone: &[(String, String)]) -> Result<(), DriveError> {
    let mut first_error: Option<DriveError> = None;
    let mut folders = Vec::new();
    let mut files: Vec<(&str, &str)> = Vec::new();
    for (key, trashed) in gone {
        if trashed.is_empty() {
            continue;
        }
        if key.ends_with('/') {
            if let Err(e) = drive.rename(trashed, key) {
                first_error.get_or_insert(e);
            }
        } else {
            files.push((trashed.as_str(), key.as_str()));
        }
        if let Some(folder) = trash_folder_of(trashed) {
            if !folders.contains(&folder) {
                folders.push(folder);
            }
        }
    }
    if !files.is_empty() {
        let batch: Vec<BatchMove<'_>> = files
            .iter()
            .map(|(from, to)| BatchMove::new(from, to))
            .collect();
        for result in drive.move_many(&batch) {
            if let Err(e) = result {
                first_error.get_or_insert(e);
            }
        }
    }
    for folder in folders {
        // Gone when empty; a file that could not be put back keeps its folder.
        let _kept_or_gone = drive.remove_empty_folder(&folder);
    }
    first_error.map_or(Ok(()), Err)
}

// ==== The transfer queue ====

/// Where a queued transfer is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobState {
    Waiting,
    Running,
    Done,
    Failed(Text),
    Cancelled,
}

/// One transfer of the queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedJob {
    pub id: u64,
    /// "Copying 3 items to docs".
    pub label: Text,
    pub state: JobState,
    pub progress: Progress,
    /// When it started running (milliseconds since 1970; 0 while it waits).
    pub started_ms: u64,
    /// The progress dialog was shown for it (once closed it stays closed).
    pub dialog_shown: bool,
    /// It was told to stop and has not said it did: it still runs, and the next transfer does
    /// not start beside it (AZ-U10).
    pub cancelling: bool,
}

/// The transfers, one running at a time (the source list's activity area shows it).
#[derive(Debug, Clone, Default)]
pub struct TransferQueue {
    jobs: Vec<QueuedJob>,
    next_id: u64,
}

impl TransferQueue {
    /// Queues a transfer; returns its id.
    pub fn push(&mut self, label: Text) -> u64 {
        self.next_id += 1;
        self.jobs.push(QueuedJob {
            id: self.next_id,
            label,
            state: JobState::Waiting,
            progress: Progress::default(),
            started_ms: 0,
            dialog_shown: false,
            cancelling: false,
        });
        self.next_id
    }

    fn job_mut(&mut self, id: u64) -> Option<&mut QueuedJob> {
        self.jobs.iter_mut().find(|j| j.id == id)
    }

    /// The transfer to start now: the first waiting one, when none runs.
    #[must_use]
    pub fn next_to_start(&self) -> Option<u64> {
        if self.running().is_some() {
            return None;
        }
        self.jobs
            .iter()
            .find(|j| j.state == JobState::Waiting)
            .map(|j| j.id)
    }

    /// Transfer `id` runs from `now_ms` (milliseconds since 1970) on.
    pub fn start(&mut self, id: u64, now_ms: u64) {
        if let Some(job) = self.job_mut(id) {
            job.started_ms = now_ms;
            job.state = JobState::Running;
        }
    }

    pub fn progress(&mut self, id: u64, progress: &Progress) {
        if let Some(job) = self.job_mut(id) {
            job.progress = progress.clone();
        }
    }

    /// The transfer ended: with an error, or without.
    pub fn finish(&mut self, id: u64, error: Option<Text>) {
        if let Some(job) = self.job_mut(id) {
            job.state = match error {
                Some(e) => JobState::Failed(e),
                None => JobState::Done,
            };
        }
    }

    /// A waiting transfer will not start. A running one is told to stop (the caller sets its
    /// flag) and stays running until its thread says it stopped ([`TransferQueue::stopped`]):
    /// marked cancelled at once, it let the next transfer start beside the thread that was still
    /// sending parts (AZ-U10).
    pub fn cancel(&mut self, id: u64) {
        if let Some(job) = self.job_mut(id) {
            match job.state {
                JobState::Waiting => job.state = JobState::Cancelled,
                JobState::Running => job.cancelling = true,
                _ => {}
            }
        }
    }

    /// The thread of transfer `id` stopped because it was told to: it is cancelled now.
    pub fn stopped(&mut self, id: u64) {
        if let Some(job) = self.job_mut(id) {
            job.state = JobState::Cancelled;
            job.cancelling = false;
        }
    }

    /// Whether transfer `id` was cancelled, or was told to stop and has not yet.
    #[must_use]
    pub fn is_cancelled(&self, id: u64) -> bool {
        self.jobs
            .iter()
            .any(|j| j.id == id && (j.state == JobState::Cancelled || j.cancelling))
    }

    /// The running transfer.
    #[must_use]
    pub fn running(&self) -> Option<&QueuedJob> {
        self.jobs.iter().find(|j| j.state == JobState::Running)
    }

    /// The transfers that wait.
    #[must_use]
    pub fn waiting(&self) -> usize {
        self.jobs
            .iter()
            .filter(|j| j.state == JobState::Waiting)
            .count()
    }

    /// Nothing runs and nothing waits.
    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.running().is_none() && self.waiting() == 0
    }

    /// The running transfer's progress, 0..=100.
    #[must_use]
    pub fn percent(&self) -> Option<f32> {
        self.running().map(|j| j.progress.percent())
    }

    /// The running transfer's line: "Copying 3 items - 1 of 3 (25%), 2 waiting".
    #[must_use]
    pub fn status_text(&self) -> Text {
        let waiting = Phrase::new("azdrive-queue-waiting").arg("count", self.waiting());
        let Some(job) = self.running() else {
            return match self.waiting() {
                0 => Text::default(),
                _ => waiting.into(),
            };
        };
        let p = &job.progress;
        let text = job.label.clone().then(" - ").then(
            Phrase::new("azdrive-queue-progress")
                .arg("done", p.files_done)
                .arg("total", p.files_total)
                .arg("percent", format!("{:.0}", p.percent())),
        );
        if self.waiting() > 0 {
            text.then(", ").then(waiting)
        } else {
            text
        }
    }

    /// The transfers that failed.
    #[must_use]
    pub fn failed(&self) -> Vec<&QueuedJob> {
        self.jobs
            .iter()
            .filter(|j| matches!(j.state, JobState::Failed(_)))
            .collect()
    }

    /// Forgets the transfers that ended.
    pub fn clear_finished(&mut self) {
        self.jobs
            .retain(|j| matches!(j.state, JobState::Waiting | JobState::Running));
    }

    #[must_use]
    pub fn jobs(&self) -> &[QueuedJob] {
        &self.jobs
    }

    /// The running transfer that has taken `after_ms` or longer by `now_ms` and has not had
    /// its progress dialog yet: Explorer shows the dialog for a long copy, not for a quick one.
    #[must_use]
    pub fn wants_progress_dialog(&self, now_ms: u64, after_ms: u64) -> Option<u64> {
        let job = self.running()?;
        (!job.dialog_shown && now_ms.saturating_sub(job.started_ms) >= after_ms).then_some(job.id)
    }

    /// The progress dialog of transfer `id` was shown.
    pub fn mark_dialog_shown(&mut self, id: u64) {
        if let Some(job) = self.job_mut(id) {
            job.dialog_shown = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        path::PathBuf,
        sync::atomic::{AtomicBool, AtomicU32, Ordering},
    };

    use azul_appkit::l10n::Arg;
    use azul_storage::{testing::TempDir, Drive, LocalDrive};

    use super::*;

    fn seeded(tmp: &TempDir) -> LocalDrive {
        let drive = LocalDrive::new(tmp.path().join("home"));
        drive.put("docs/a.txt", b"alpha").unwrap();
        drive.put("docs/sub/b.txt", b"beta").unwrap();
        drive.put("readme.txt", b"hello").unwrap();
        drive.create_folder("empty/").unwrap();
        drive
    }

    fn item(key: &str) -> SourceItem {
        SourceItem {
            key: key.to_string(),
            is_folder: key.ends_with('/'),
            size: None,
        }
    }

    fn never_cancel() -> AtomicBool {
        AtomicBool::new(false)
    }

    #[test]
    fn explorer_names_a_copy_in_the_same_folder_and_a_kept_duplicate() {
        let taken = |n: &str| ["a.txt", "a - Copy.txt", "b (2).txt", "New folder"].contains(&n);
        assert_eq!(copy_name("a.txt", "Copy", &taken), "a - Copy (2).txt");
        assert_eq!(copy_name("c.txt", "Copy", &taken), "c - Copy.txt");
        assert_eq!(copy_name("dir", "Copy", &|_| false), "dir - Copy");
        assert_eq!(copy_name("c.txt", "Kopie", &taken), "c - Kopie.txt");
        assert_eq!(keep_both_name("b.txt", &taken), "b (3).txt");
        assert_eq!(keep_both_name("a.txt", &taken), "a (2).txt");
        assert_eq!(new_name("New folder", &taken), "New folder (2)");
        assert_eq!(
            new_name("New Text Document.txt", &taken),
            "New Text Document.txt"
        );
        assert_eq!(split_extension("archive.tar.gz"), ("archive.tar", ".gz"));
        assert_eq!(split_extension(".env"), (".env", ""));
        assert_eq!(split_extension("noext"), ("noext", ""));
    }

    /// X-AR5a-17: the name check had a character list of its own and knew no device names and no
    /// trailing dots; the one name table's rows of the computer decide the rest.
    #[test]
    fn x_ar5a_17_a_name_windows_reads_as_something_else_is_refused_there() {
        for name in ["CON", "nul.txt", "COM1", "report.", "report ", "LPT9.dat"] {
            assert!(
                check_name_on(name, System::Windows).is_err(),
                "{name:?} on Windows"
            );
            assert!(
                check_name_on(name, System::Linux).is_ok(),
                "{name:?} on Linux is a name"
            );
        }
        let why = check_name_on("CON", System::Windows).unwrap_err();
        assert!(why.keys().contains(&"azdrive-name-unusable"), "{why}");
        assert!(check_name_on("notes.txt", System::Windows).is_ok());
        // The characters of Explorer stay refused on every system (a name is portable).
        assert!(check_name_on("a:b", System::Linux).is_err());
    }

    #[test]
    fn a_name_explorer_refuses_is_refused_with_the_reason() {
        assert!(check_name("notes.txt").is_ok());
        assert!(check_name("").is_err());
        assert!(check_name("   ").is_err());
        assert!(check_name("a/b").is_err());
        assert!(check_name("a:b").is_err());
        assert!(check_name("what?").is_err());
        assert!(check_name("..").is_err());
        let reason = check_name("a|b").unwrap_err();
        assert_eq!(reason.keys(), ["azdrive-name-forbidden-chars"], "{reason}");
    }

    #[test]
    fn a_plan_lists_every_file_under_a_folder_and_finds_the_taken_names() {
        let tmp = TempDir::new("plan");
        let home = seeded(&tmp);
        let target = LocalDrive::new(tmp.path().join("other"));
        target.put("in/docs/a.txt", b"old").unwrap();
        let plan = plan_transfer(
            &home,
            &[item("docs/"), item("readme.txt"), item("empty/")],
            &target,
            "in/",
            TransferKind::Copy,
            "Copy",
        )
        .unwrap();
        let targets: Vec<&str> = plan.files.iter().map(|f| f.target_key.as_str()).collect();
        assert_eq!(
            targets,
            vec!["in/docs/a.txt", "in/docs/sub/b.txt", "in/readme.txt"]
        );
        assert!(plan.folders.contains(&"in/docs/".to_string()));
        assert!(
            plan.folders.contains(&"in/empty/".to_string()),
            "{:?}",
            plan.folders
        );
        assert_eq!(plan.conflicts(), vec![0], "in/docs/a.txt is there");
        assert_eq!(plan.total_bytes(), 5 + 4 + 5);
    }

    #[test]
    fn a_copy_into_the_same_folder_is_named_like_explorers_copy() {
        let tmp = TempDir::new("same");
        let home = seeded(&tmp);
        let plan = plan_transfer(
            &home,
            &[item("readme.txt")],
            &home,
            "",
            TransferKind::Copy,
            "Copy",
        )
        .unwrap();
        assert_eq!(plan.files[0].target_key, "readme - Copy.txt");
        assert!(plan.conflicts().is_empty());
        // Moving into the folder it is in does nothing; a folder never goes into itself.
        let noop = plan_transfer(
            &home,
            &[item("readme.txt")],
            &home,
            "",
            TransferKind::Move,
            "Copy",
        )
        .unwrap();
        assert!(noop.files.is_empty() && noop.folders.is_empty());
        assert!(plan_transfer(
            &home,
            &[item("docs/")],
            &home,
            "docs/sub/",
            TransferKind::Move,
            "Copy",
        )
        .is_err());
    }

    #[test]
    fn running_a_copy_with_choices_replaces_skips_or_keeps_both() {
        let tmp = TempDir::new("run");
        let home = seeded(&tmp);
        let target = LocalDrive::new(tmp.path().join("other"));
        target.put("docs/a.txt", b"old").unwrap();
        target.put("readme.txt", b"old").unwrap();
        let mut plan = plan_transfer(
            &home,
            &[item("docs/"), item("readme.txt")],
            &target,
            "",
            TransferKind::Copy,
            "Copy",
        )
        .unwrap();
        assert_eq!(plan.conflicts().len(), 2);
        let first = plan.conflicts()[0];
        let second = plan.conflicts()[1];
        plan.choose(first, ConflictChoice::KeepBoth);
        plan.choose(second, ConflictChoice::Skip);
        assert!(plan.unresolved().is_none());
        let mut seen = Vec::new();
        let report = run_transfer(
            &plan,
            &home,
            &target,
            TransferKind::Copy,
            &never_cancel(),
            &mut |p| seen.push(p.clone()),
        );
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert_eq!(
            report.done, 2,
            "docs/a.txt kept both, docs/sub/b.txt copied"
        );
        assert_eq!(report.skipped, 1);
        assert_eq!(target.get("docs/a.txt").unwrap(), b"old");
        assert_eq!(target.get("docs/a (2).txt").unwrap(), b"alpha");
        assert_eq!(target.get("docs/sub/b.txt").unwrap(), b"beta");
        assert_eq!(target.get("readme.txt").unwrap(), b"old", "skipped");
        let last = seen.last().expect("progress was reported");
        assert_eq!(last.files_done, 3);
        assert_eq!(
            home.get("docs/a.txt").unwrap(),
            b"alpha",
            "a copy keeps the source"
        );
    }

    #[test]
    fn a_move_removes_the_sources_and_replace_overwrites() {
        let tmp = TempDir::new("move");
        let home = seeded(&tmp);
        let target = LocalDrive::new(tmp.path().join("other"));
        target.put("readme.txt", b"old").unwrap();
        let mut plan = plan_transfer(
            &home,
            &[item("docs/"), item("readme.txt")],
            &target,
            "",
            TransferKind::Move,
            "Copy",
        )
        .unwrap();
        for i in plan.conflicts() {
            plan.choose(i, ConflictChoice::Replace);
        }
        let report = run_transfer(
            &plan,
            &home,
            &target,
            TransferKind::Move,
            &never_cancel(),
            &mut |_| {},
        );
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert_eq!(target.get("readme.txt").unwrap(), b"hello");
        assert_eq!(target.get("docs/sub/b.txt").unwrap(), b"beta");
        assert!(home.get("readme.txt").is_err());
        assert!(
            home.local_path("docs/").is_none_or(|p| !p.exists()),
            "the folder moved"
        );
    }

    #[test]
    fn a_move_within_one_drive_renames_and_a_cancel_stops_before_the_next_file() {
        let tmp = TempDir::new("rename");
        let home = seeded(&tmp);
        home.create_folder("archive/").unwrap();
        let plan = plan_transfer(
            &home,
            &[item("docs/")],
            &home,
            "archive/",
            TransferKind::Move,
            "Copy",
        )
        .unwrap();
        let report = run_transfer(
            &plan,
            &home,
            &home,
            TransferKind::Move,
            &never_cancel(),
            &mut |_| {},
        );
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert_eq!(home.get("archive/docs/sub/b.txt").unwrap(), b"beta");

        let plan = plan_transfer(
            &home,
            &[item("archive/")],
            &home,
            "",
            TransferKind::Copy,
            "Copy",
        )
        .unwrap();
        let cancelled = AtomicBool::new(true);
        let report = run_transfer(
            &plan,
            &home,
            &home,
            TransferKind::Copy,
            &cancelled,
            &mut |_| {},
        );
        assert!(report.cancelled);
        assert_eq!(report.done, 0);
    }

    #[test]
    fn delete_goes_to_the_trash_folder_and_comes_back_with_undo() {
        let tmp = TempDir::new("trash");
        let home = seeded(&tmp);
        let stamp = trash_stamp(1_759_300_000, 1);
        let deleted = delete_items(&home, &[item("docs/"), item("readme.txt")], Some(&stamp));
        assert!(deleted.failed.is_empty(), "{:?}", deleted.failed);
        let moved = deleted.gone;
        assert_eq!(moved.len(), 2);
        assert!(home.get("readme.txt").is_err());
        assert_eq!(moved[1].1, trash_key("readme.txt", &stamp));
        assert!(moved[1].1.starts_with(TRASH_FOLDER));
        assert_eq!(
            home.get(&trash_key("docs/sub/b.txt", &stamp)).unwrap(),
            b"beta"
        );
        assert!(is_in_trash(&moved[0].1));
        // Undo: every item back where it was.
        restore_items(&home, &moved).unwrap();
        assert_eq!(home.get("readme.txt").unwrap(), b"hello");
        assert_eq!(home.get("docs/sub/b.txt").unwrap(), b"beta");
        // For good: nothing left anywhere.
        let for_good = delete_items(&home, &[item("docs/")], None);
        assert!(for_good.failed.is_empty(), "{:?}", for_good.failed);
        assert!(home.local_path("docs/").is_none_or(|p| !p.exists()));
    }

    #[test]
    fn a_transfer_running_two_seconds_asks_for_the_progress_dialog_once() {
        let mut queue = TransferQueue::default();
        let a = queue.push(Text::plain("Copying 1 item to docs"));
        assert_eq!(
            queue.wants_progress_dialog(5_000, 2_000),
            None,
            "nothing runs"
        );
        queue.start(a, 1_000);
        assert_eq!(
            queue.wants_progress_dialog(2_500, 2_000),
            None,
            "a quick copy shows no dialog"
        );
        assert_eq!(queue.wants_progress_dialog(3_000, 2_000), Some(a));
        queue.mark_dialog_shown(a);
        assert_eq!(
            queue.wants_progress_dialog(9_000, 2_000),
            None,
            "a dialog the user closed stays closed"
        );
        queue.finish(a, None);
        let b = queue.push(Text::plain("Copying 2 items to docs"));
        queue.start(b, 10_000);
        assert_eq!(
            queue.wants_progress_dialog(12_000, 2_000),
            Some(b),
            "the next one asks again"
        );
    }

    #[test]
    fn a_transfer_queue_runs_one_job_at_a_time_and_sums_the_progress() {
        let mut queue = TransferQueue::default();
        let a = queue.push(Text::plain("Copying 3 items"));
        let b = queue.push(Text::plain("Uploading photo.jpg"));
        assert_eq!(queue.next_to_start(), Some(a));
        queue.start(a, 1_000);
        assert_eq!(queue.next_to_start(), None, "one at a time");
        queue.progress(
            a,
            &Progress {
                files_done: 1,
                files_total: 3,
                bytes_done: 50,
                bytes_total: 200,
                current: "a.txt".to_string(),
            },
        );
        assert_eq!(queue.percent(), Some(25.0));
        let status = queue.status_text();
        assert!(
            status.to_string().starts_with("Copying 3 items - "),
            "{status}"
        );
        let progress = status
            .phrase("azdrive-queue-progress")
            .expect("the progress");
        assert_eq!(progress.get("done"), Some(&Arg::Int(1)));
        assert_eq!(progress.get("total"), Some(&Arg::Int(3)));
        assert_eq!(progress.get("percent"), Some(&Arg::from("25")));
        assert_eq!(
            status
                .phrase("azdrive-queue-waiting")
                .and_then(|w| w.get("count")),
            Some(&Arg::Int(1))
        );
        queue.finish(a, None);
        assert_eq!(queue.next_to_start(), Some(b));
        queue.start(b, 2_000);
        queue.finish(b, Some(Text::plain("no answer")));
        assert!(queue.is_idle());
        assert_eq!(queue.failed().len(), 1);
        queue.clear_finished();
        assert!(queue.jobs().is_empty());
    }

    /// A local drive that counts its gets and its own copies.
    struct Counting {
        inner: LocalDrive,
        gets: AtomicU32,
        copies: AtomicU32,
        /// Whether it says where its files are (a folder on disk) or not (as
        /// a bucket).
        on_disk: bool,
    }

    impl Counting {
        fn new(tmp: &TempDir, on_disk: bool) -> Self {
            Self {
                inner: LocalDrive::new(tmp.path().join("home")),
                gets: AtomicU32::new(0),
                copies: AtomicU32::new(0),
                on_disk,
            }
        }
    }

    impl Drive for Counting {
        fn list(
            &self,
            request: &azul_storage::ListRequest,
        ) -> Result<azul_storage::ListPage, DriveError> {
            self.inner.list(request)
        }
        fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
            self.gets.fetch_add(1, Ordering::SeqCst);
            self.inner.get(key)
        }
        fn get_range(
            &self,
            key: &str,
            range: azul_storage::ByteRange,
        ) -> Result<Vec<u8>, DriveError> {
            self.gets.fetch_add(1, Ordering::SeqCst);
            self.inner.get_range(key, range)
        }
        fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
            self.inner.put(key, bytes)
        }
        fn delete(&self, key: &str) -> Result<(), DriveError> {
            self.inner.delete(key)
        }
        fn head(&self, key: &str) -> Result<azul_storage::ObjectInfo, DriveError> {
            self.inner.head(key)
        }
        fn copy(&self, from: &str, to: &str) -> Result<(), DriveError> {
            self.copies.fetch_add(1, Ordering::SeqCst);
            self.inner.copy(from, to)
        }
        fn create_folder(&self, prefix: &str) -> Result<(), DriveError> {
            self.inner.create_folder(prefix)
        }
        fn local_path(&self, key: &str) -> Option<PathBuf> {
            self.inner.local_path(key).filter(|_| self.on_disk)
        }
    }

    /// Within one bucket a copy is the bucket's own copy (CopyObject): the
    /// bytes never pass through AzDrive.
    #[test]
    fn a_copy_within_one_bucket_is_the_buckets_own_copy() {
        let tmp = TempDir::new("own-copy");
        seeded(&tmp);
        // A drive with no local path, as a bucket.
        let drive = Counting::new(&tmp, false);
        drive.create_folder("backup/").unwrap();
        let plan = plan_transfer(
            &drive,
            &[item("docs/"), item("readme.txt")],
            &drive,
            "backup/",
            TransferKind::Copy,
            "Copy",
        )
        .unwrap();
        assert!(plan.same_drive);
        let report = run_transfer(
            &plan,
            &drive,
            &drive,
            TransferKind::Copy,
            &never_cancel(),
            &mut |_| {},
        );
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert_eq!(report.done, 3);
        assert_eq!(drive.copies.load(Ordering::SeqCst), 3);
        assert_eq!(
            drive.gets.load(Ordering::SeqCst),
            0,
            "no byte through the app"
        );
        assert_eq!(drive.get("backup/docs/sub/b.txt").unwrap(), b"beta");
    }

    /// On disk a copy within the drive keeps Explorer's progress: the file
    /// goes megabyte by megabyte, and the status bar hears each one.
    #[test]
    fn a_copy_on_disk_reports_its_progress_megabyte_by_megabyte() {
        let tmp = TempDir::new("disk-progress");
        let drive = Counting::new(&tmp, true);
        let big = vec![7u8; 3 * 1024 * 1024 + 5];
        drive.put("big.bin", &big).unwrap();
        drive.create_folder("backup/").unwrap();
        let plan = plan_transfer(
            &drive,
            &[SourceItem {
                key: String::from("big.bin"),
                is_folder: false,
                size: Some(big.len() as u64),
            }],
            &drive,
            "backup/",
            TransferKind::Copy,
            "Copy",
        )
        .unwrap();
        let mut seen = Vec::new();
        let report = run_transfer(
            &plan,
            &drive,
            &drive,
            TransferKind::Copy,
            &never_cancel(),
            &mut |p| {
                seen.push(p.bytes_done);
            },
        );
        assert!(report.failed.is_empty(), "{:?}", report.failed);
        assert_eq!(drive.get("backup/big.bin").unwrap().len(), big.len());
        let distinct: std::collections::BTreeSet<u64> = seen.iter().copied().collect();
        assert!(
            distinct.len() >= 4,
            "progress after every megabyte: {seen:?}"
        );
        assert_eq!(
            drive.copies.load(Ordering::SeqCst),
            0,
            "the chunked copy, not fs::copy"
        );
    }

    /// A drive on the far side of a connection that drops: it lists and answers HEAD, and every
    /// read of a file's bytes fails.
    struct Dropping(LocalDrive);

    impl Drive for Dropping {
        fn list(
            &self,
            request: &azul_storage::ListRequest,
        ) -> Result<azul_storage::ListPage, DriveError> {
            self.0.list(request)
        }
        fn get(&self, _key: &str) -> Result<Vec<u8>, DriveError> {
            Err(DriveError::Transport(String::from("connection reset")))
        }
        fn get_range(
            &self,
            _key: &str,
            _range: azul_storage::ByteRange,
        ) -> Result<Vec<u8>, DriveError> {
            Err(DriveError::Transport(String::from("connection reset")))
        }
        fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
            self.0.put(key, bytes)
        }
        fn delete(&self, key: &str) -> Result<(), DriveError> {
            self.0.delete(key)
        }
        fn head(&self, key: &str) -> Result<azul_storage::ObjectInfo, DriveError> {
            self.0.head(key)
        }
    }

    /// AZ-U4/AZ-S1: "Replace" used to delete the target first. A copy that fails after it left
    /// neither file; a file replaced by itself (a second drive over the same folder) left none.
    #[test]
    fn az_u4_s1_a_replace_whose_copy_fails_keeps_the_old_file() {
        let tmp = TempDir::new("replace-fails");
        let target = seeded(&tmp);
        let far = Dropping(LocalDrive::new(tmp.path().join("far")));
        far.put("readme.txt", b"the new text").unwrap();
        let mut plan = plan_transfer(
            &far,
            &[item("readme.txt")],
            &target,
            "",
            TransferKind::Copy,
            "Copy",
        )
        .unwrap();
        assert_eq!(plan.conflicts().len(), 1, "readme.txt is there already");
        plan.choose(0, ConflictChoice::Replace);
        let report = run_transfer(
            &plan,
            &far,
            &target,
            TransferKind::Copy,
            &never_cancel(),
            &mut |_| {},
        );
        assert_eq!(report.failed.len(), 1, "the read dropped: {report:?}");
        assert_eq!(
            target.get("readme.txt").unwrap(),
            b"hello",
            "the old file is still there: nothing was deleted before the new one was whole"
        );
    }

    #[test]
    fn az_u4_s1_a_file_is_never_replaced_by_itself_through_a_second_drive_over_its_folder() {
        for kind in [TransferKind::Move, TransferKind::Copy] {
            let tmp = TempDir::new("replace-self");
            let home = seeded(&tmp);
            // The second drive over the same tree the app builds itself (`os:<folder>`,
            // `sync:<drive>`).
            let os = LocalDrive::new(tmp.path().join("home"));
            let plan = plan_transfer(&os, &[item("readme.txt")], &home, "", kind, "Copy").unwrap();
            assert_eq!(
                plan.conflicts().len(),
                0,
                "{kind:?}: the file is where it would land - not 'there already' as another file"
            );
            match kind {
                TransferKind::Move => assert!(plan.is_empty(), "a move onto itself does nothing"),
                _ => assert_eq!(plan.files[0].target_key, "readme - Copy.txt"),
            }
            let report = run_transfer(&plan, &os, &home, kind, &never_cancel(), &mut |_| {});
            assert!(report.failed.is_empty(), "{kind:?}: {report:?}");
            assert_eq!(
                home.get("readme.txt").unwrap(),
                b"hello",
                "{kind:?}: the only copy is still there"
            );
        }
    }

    /// A plan that names a file as its own destination (made some other way than by the planner)
    /// is refused at the run: the file is not replaced by itself, and a Move does not delete it
    /// as a done source.
    #[test]
    fn az_u4_s1_the_run_refuses_a_file_that_is_its_own_destination() {
        for kind in [TransferKind::Move, TransferKind::Copy] {
            let tmp = TempDir::new("replace-self-run");
            let home = seeded(&tmp);
            let os = LocalDrive::new(tmp.path().join("home"));
            let mut plan = Plan::default();
            plan.files.push(PlannedFile {
                source_key: String::from("readme.txt"),
                target_key: String::from("readme.txt"),
                size: Some(5),
                conflict: true,
                choice: Some(ConflictChoice::Replace),
                source_etag: None,
                target_etag: None,
                target_size: None,
                keep_as: None,
            });
            let report = run_transfer(&plan, &os, &home, kind, &never_cancel(), &mut |_| {});
            assert_eq!(home.get("readme.txt").unwrap(), b"hello", "{kind:?}");
            assert_eq!(
                report.failed.len(),
                1,
                "{kind:?}: refused, not done: {report:?}"
            );
            assert_eq!(report.done, 0, "{kind:?}");
        }
    }

    /// AZ-U17: the first failed item ended the delete's loops, dropping the Undo of the 39 that
    /// went. Every item is tried, and the result says which went and which did not.
    #[test]
    fn az_u17_a_partly_failed_delete_reports_what_went_and_what_did_not() {
        let tmp = TempDir::new("u17-delete");
        let home = seeded(&tmp);
        home.put("c.txt", b"c").unwrap();
        let stamp = trash_stamp(1_759_300_000, 1);
        // The middle one is not there (somebody removed it meanwhile).
        let deleted = delete_items(
            &home,
            &[item("readme.txt"), item("gone.txt"), item("c.txt")],
            Some(&stamp),
        );
        let went: Vec<&str> = deleted.gone.iter().map(|(key, _)| key.as_str()).collect();
        assert_eq!(went, vec!["readme.txt", "c.txt"], "{deleted:?}");
        assert_eq!(deleted.failed.len(), 1, "{deleted:?}");
        assert_eq!(deleted.failed[0].0, "gone.txt");
        assert!(matches!(deleted.failed[0].1, DriveError::NotFound { .. }));
        assert!(home.get("readme.txt").is_err() && home.get("c.txt").is_err());
        // For good: the same.
        let for_good = delete_items(&home, &[item("missing.txt"), item("docs/")], None);
        assert_eq!(for_good.gone, vec![(String::from("docs/"), String::new())]);
        assert_eq!(
            for_good.failed.len(),
            0,
            "a missing file is deleted already: {for_good:?}"
        );
    }

    /// An Undo that meets an item it cannot put back goes on with the others.
    #[test]
    fn az_u17_an_undo_puts_back_what_it_can_and_names_what_it_could_not() {
        let tmp = TempDir::new("u17-undo");
        let home = seeded(&tmp);
        let stamp = trash_stamp(1_759_300_000, 1);
        let deleted = delete_items(&home, &[item("readme.txt"), item("docs/")], Some(&stamp));
        assert!(deleted.failed.is_empty());
        // The trashed copy of the folder went in the meantime.
        home.delete_folder(&trash_key("docs/", &stamp)).unwrap();
        let undone = restore_items(&home, &deleted.gone);
        assert!(undone.is_err(), "the folder could not come back");
        assert_eq!(
            home.get("readme.txt").unwrap(),
            b"hello",
            "the file came back all the same"
        );
    }

    /// AZ X-AR5a-10: Undo removed the delete's folder in the trash with `delete_folder` after a
    /// listing that cannot see a socket: it was removed with the folder.
    #[cfg(unix)]
    #[test]
    fn x_ar5a_10_an_undo_keeps_the_trash_folder_that_holds_something_it_did_not_put_back() {
        use std::os::unix::net::UnixListener;
        let tmp = TempDir::new("x10-undo");
        let home = seeded(&tmp);
        let stamp = trash_stamp(1_759_300_000, 1);
        let deleted = delete_items(&home, &[item("readme.txt")], Some(&stamp));
        assert!(deleted.failed.is_empty());
        let folder = tmp.path().join("home").join(TRASH_FOLDER).join(&stamp);
        let socket = folder.join("s");
        let _listener = UnixListener::bind(&socket).unwrap();
        restore_items(&home, &deleted.gone).unwrap();
        assert_eq!(home.get("readme.txt").unwrap(), b"hello");
        assert!(
            socket.exists(),
            "the trash folder was not removed with what was in it"
        );
    }

    /// AZ-U1: the guard against a folder going into itself compared drive ids. A second drive
    /// over the same tree, or over a folder inside it, has another id.
    #[test]
    fn az_u1_a_folder_never_goes_into_itself_or_below_itself_through_another_drive() {
        let tmp = TempDir::new("into-itself");
        let home = seeded(&tmp);
        // The same folder again, and a drive on a folder inside it (a drive added on
        // ~/Projects, the synced listing's folder).
        let os = LocalDrive::new(tmp.path().join("home"));
        let inner = LocalDrive::without_manifest(tmp.path().join("home").join("docs"));
        let plan = |source: &LocalDrive, key: &str, target: &LocalDrive, prefix: &str| {
            plan_transfer(
                source,
                &[item(key)],
                target,
                prefix,
                TransferKind::Move,
                "Copy",
            )
        };
        // Below itself: docs/ into docs/sub/ as the other drive sees it ...
        assert!(plan(&home, "docs/", &os, "docs/sub/").is_err());
        // ... and as the inner drive sees it (its "sub/" is docs/sub/).
        assert!(plan(&home, "docs/", &inner, "sub/").is_err());
        // Into the inner drive's root: it would land as docs/docs/.
        assert!(plan(&home, "docs/", &inner, "").is_err());
        // Onto its own place: the root of the same tree, as docs/ it already is - nothing to do.
        assert!(plan(&home, "docs/", &os, "").unwrap().is_empty());
        // The error is the guard's, not a listing failure.
        assert!(matches!(
            plan(&home, "docs/", &os, "docs/sub/"),
            Err(DriveError::InvalidKey { .. })
        ));
        // What is not inside it is fine: up a level, or into a sibling.
        assert!(plan(&home, "docs/sub/", &os, "").is_ok());
        assert!(plan(&home, "docs/sub/", &os, "empty/").is_ok());
        // Nothing moved by planning.
        assert_eq!(home.get("docs/sub/b.txt").unwrap(), b"beta");
    }
}
