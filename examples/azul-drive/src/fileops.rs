//! AzDrive's file operations on any drive, as Explorer does them: copy and
//! move (within a drive or across two), with a plan made first (every file
//! under a folder, every name that is taken at the target) so the user
//! decides each conflict - replace, skip, keep both - before a byte moves;
//! delete to a trash folder (local drives) or for good; the names Explorer
//! makes ("a - Copy.txt", "a (2).txt", "New folder (2)"). Everything here
//! blocks: the app runs it on an azul `Thread`. No azul types.

use std::{
    collections::HashSet,
    sync::atomic::{AtomicBool, Ordering},
};

use azul_storage::{key, ops as storage_ops, transfer, Drive, DriveError};

use crate::browse;

/// The trash folder at the root of a local drive: a deleted item goes to
/// `.azdrive-trash/<stamp>/<its key>`, so it can come back where it was.
/// A dot name: hidden unless "Hidden items" is on.
pub const TRASH_FOLDER: &str = ".azdrive-trash/";

// ==== Names ====

/// `archive.tar.gz` -> (`archive.tar`, `.gz`); `.env` and `README` have no
/// extension.
#[must_use]
pub fn split_extension(name: &str) -> (&str, &str) {
    match browse::extension_of(name) {
        Some(ext) => {
            let cut = name.len() - ext.len() - 1;
            (&name[..cut], &name[cut..])
        }
        None => (name, ""),
    }
}

/// `stem<suffix>ext` for the first suffix of `suffixes` whose name is free.
fn first_free(
    stem: &str,
    ext: &str,
    suffixes: impl Iterator<Item = String>,
    taken: &dyn Fn(&str) -> bool,
) -> String {
    let mut last = format!("{stem}{ext}");
    for suffix in suffixes {
        let candidate = format!("{stem}{suffix}{ext}");
        if !taken(&candidate) {
            return candidate;
        }
        last = candidate;
    }
    last
}

/// The name Explorer gives a copy pasted into the folder it came from:
/// `a - Copy.txt`, then `a - Copy (2).txt`, ...
#[must_use]
pub fn copy_name(name: &str, taken: &dyn Fn(&str) -> bool) -> String {
    let (stem, ext) = split_extension(name);
    first_free(
        stem,
        ext,
        std::iter::once(String::from(" - Copy"))
            .chain((2..10_000).map(|n| format!(" - Copy ({n})"))),
        taken,
    )
}

/// [`copy_name`] for a folder (a dot in its name is not an extension).
#[must_use]
pub fn copy_folder_name(name: &str, taken: &dyn Fn(&str) -> bool) -> String {
    first_free(
        name,
        "",
        std::iter::once(String::from(" - Copy"))
            .chain((2..10_000).map(|n| format!(" - Copy ({n})"))),
        taken,
    )
}

/// The name "Keep both files" gives the newcomer: `a (2).txt`, `a (3).txt`.
#[must_use]
pub fn keep_both_name(name: &str, taken: &dyn Fn(&str) -> bool) -> String {
    let (stem, ext) = split_extension(name);
    first_free(stem, ext, (2..10_000).map(|n| format!(" ({n})")), taken)
}

/// A new item's name: `name` when free, else `name (2)`, `name (3)`, ...
/// ("New folder", "New Text Document.txt").
#[must_use]
pub fn new_name(name: &str, taken: &dyn Fn(&str) -> bool) -> String {
    if taken(name) {
        keep_both_name(name, taken)
    } else {
        name.to_string()
    }
}

/// The characters Explorer refuses in a name.
pub const FORBIDDEN_CHARS: &[char] = &['\\', '/', ':', '*', '?', '"', '<', '>', '|'];

/// Whether `name` can name a file or folder, or why not (a sentence).
pub fn check_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(String::from("A name cannot be empty."));
    }
    if trimmed == "." || trimmed == ".." {
        return Err(format!("\"{trimmed}\" is reserved."));
    }
    if name.chars().any(|c| FORBIDDEN_CHARS.contains(&c)) {
        return Err(String::from(
            "A name cannot contain any of these characters: \\ / : * ? \" < > |",
        ));
    }
    if name.chars().any(char::is_control) {
        return Err(String::from("A name cannot contain control characters."));
    }
    Ok(())
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

/// What the user decided for a name that is taken at the target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConflictChoice {
    /// "Replace the file in the destination".
    Replace,
    /// "Skip this file".
    Skip,
    /// "Keep both files": the newcomer gets ` (2)`.
    KeepBoth,
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

/// One file the transfer copies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFile {
    pub source_key: String,
    pub target_key: String,
    pub size: Option<u64>,
    /// Something has `target_key` already.
    pub conflict: bool,
    /// What the user chose for a conflict (`None`: not asked yet).
    pub choice: Option<ConflictChoice>,
}

/// Everything a transfer will do, decided before it starts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plan {
    /// The files to copy (under the folders too), in order.
    pub files: Vec<PlannedFile>,
    /// The folders to create at the target, parents first (empty ones too).
    pub folders: Vec<String>,
    /// Whole items renamed in place: a move within one drive with nothing
    /// in the way (one `rename`, not a copy).
    pub moves: Vec<(String, String)>,
    /// The folders a move empties, removed after their files went.
    pub source_folders: Vec<String>,
    /// The items the transfer makes at the target (what to select after).
    pub tops: Vec<String>,
    /// Source and target are one drive. In a bucket a file then goes by the
    /// drive's own copy (CopyObject, no byte through AzDrive); on disk by the
    /// chunked file copy, which reports its progress megabyte by megabyte.
    pub same_drive: bool,
    /// The target keys that are taken (for "Keep both" names).
    taken: HashSet<String>,
}

impl Plan {
    /// The files whose names are taken at the target, by index.
    #[must_use]
    pub fn conflicts(&self) -> Vec<usize> {
        self.files
            .iter()
            .enumerate()
            .filter(|(_, f)| f.conflict)
            .map(|(i, _)| i)
            .collect()
    }

    /// The first conflict nobody decided yet.
    #[must_use]
    pub fn unresolved(&self) -> Option<usize> {
        self.files
            .iter()
            .position(|f| f.conflict && f.choice.is_none())
    }

    /// The unresolved conflicts from `from` on (for "Do this for all").
    #[must_use]
    pub fn unresolved_count(&self) -> usize {
        self.files
            .iter()
            .filter(|f| f.conflict && f.choice.is_none())
            .count()
    }

    /// Decides the conflict of file `index`; "Keep both" renames the
    /// newcomer to the first free ` (n)` name.
    pub fn choose(&mut self, index: usize, choice: ConflictChoice) {
        let Some(file) = self.files.get(index) else {
            return;
        };
        let mut target_key = file.target_key.clone();
        if choice == ConflictChoice::KeepBoth {
            let folder = key::folder_of(&target_key).to_string();
            let name = key::last_segment(&target_key).to_string();
            let taken = &self.taken;
            let free = keep_both_name(&name, &|n: &str| taken.contains(&format!("{folder}{n}")));
            target_key = format!("{folder}{free}");
            self.taken.insert(target_key.clone());
        }
        if let Some(file) = self.files.get_mut(index) {
            file.target_key = target_key;
            file.choice = Some(choice);
        }
    }

    /// The bytes the files hold (as far as known).
    #[must_use]
    pub fn total_bytes(&self) -> u64 {
        self.files.iter().filter_map(|f| f.size).sum()
    }

    /// Nothing to do.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.folders.is_empty() && self.moves.is_empty()
    }
}

/// The folder an item sits in: `docs/a.txt` -> `docs/`, `docs/sub/` -> `docs/`.
#[must_use]
pub fn parent_of(item_key: &str) -> String {
    if item_key.ends_with('/') {
        key::parent_prefix(item_key)
    } else {
        key::folder_of(item_key).to_string()
    }
}

/// Plans copying or moving `items` of `source` into the folder
/// `target_prefix` of `target` (`same_drive`: they are one drive). Lists
/// every folder (one recursive listing each) and every taken name at the
/// target (one listing per folder that is there, one HEAD per file).
/// Explorer's rules: a copy into the folder it came from is named
/// "a - Copy"; a move into it does nothing; a folder never goes into
/// itself. A move within one drive with nothing in the way is one rename.
pub fn plan_transfer(
    source: &dyn Drive,
    items: &[SourceItem],
    target: &dyn Drive,
    target_prefix: &str,
    same_drive: bool,
    kind: TransferKind,
) -> Result<Plan, DriveError> {
    let mut plan = Plan {
        same_drive,
        ..Plan::default()
    };
    for item in items {
        let name = key::last_segment(&item.key).to_string();
        let parent = parent_of(&item.key);
        if item.is_folder && same_drive && target_prefix.starts_with(item.key.as_str()) {
            return Err(DriveError::InvalidKey {
                key: target_prefix.to_string(),
                reason: "the destination folder is inside the folder it would receive",
            });
        }
        let same_place = same_drive && parent == target_prefix;
        if same_place && kind == TransferKind::Move {
            continue;
        }
        let target_name = if same_place {
            if item.is_folder {
                copy_folder_name(&name, &|n: &str| {
                    storage_ops::folder_exists(target, &format!("{target_prefix}{n}/"))
                        .unwrap_or(true)
                })
            } else {
                copy_name(&name, &|n: &str| {
                    storage_ops::exists(target, &format!("{target_prefix}{n}")).unwrap_or(true)
                })
            }
        } else {
            name.clone()
        };
        if item.is_folder {
            let target_key = format!("{target_prefix}{target_name}/");
            plan.tops.push(target_key.clone());
            let target_exists = storage_ops::folder_exists(target, &target_key)?;
            if same_drive && kind == TransferKind::Move && !target_exists {
                plan.moves.push((item.key.clone(), target_key));
                continue;
            }
            let existing: HashSet<String> = if target_exists {
                storage_ops::list_all(target, &target_key)?
                    .into_iter()
                    .map(|o| o.key)
                    .collect()
            } else {
                HashSet::new()
            };
            plan.folders.push(target_key.clone());
            for object in storage_ops::list_all(source, &item.key)? {
                let rest = &object.key[item.key.len()..];
                if rest.is_empty() {
                    continue; // the folder's own marker
                }
                let to = format!("{target_key}{rest}");
                if rest.ends_with('/') {
                    plan.folders.push(to);
                    continue;
                }
                let conflict = existing.contains(&to);
                if conflict {
                    plan.taken.insert(to.clone());
                }
                plan.files.push(PlannedFile {
                    source_key: object.key.clone(),
                    target_key: to,
                    size: Some(object.size),
                    conflict,
                    choice: None,
                });
            }
            plan.taken.extend(existing);
            if kind.removes_source() {
                plan.source_folders.push(item.key.clone());
            }
        } else {
            let target_key = format!("{target_prefix}{target_name}");
            plan.tops.push(target_key.clone());
            let taken = storage_ops::exists(target, &target_key)?;
            if same_drive && kind == TransferKind::Move && !taken {
                plan.moves.push((item.key.clone(), target_key));
                continue;
            }
            let size = match item.size {
                Some(size) => Some(size),
                None => source.head(&item.key).ok().map(|info| info.size),
            };
            if taken {
                plan.taken.insert(target_key.clone());
            }
            plan.files.push(PlannedFile {
                source_key: item.key.clone(),
                target_key,
                size,
                conflict: taken,
                choice: None,
            });
        }
    }
    Ok(plan)
}

// ==== Running ====

/// Where a running transfer is.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Progress {
    /// Files done (copied, skipped or failed) and items renamed.
    pub files_done: usize,
    pub files_total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
    /// The name being copied.
    pub current: String,
}

impl Progress {
    /// Done so far, 0..=100: by bytes when they are known, else by files.
    #[must_use]
    pub fn percent(&self) -> f32 {
        if self.bytes_total > 0 {
            (self.bytes_done as f64 / self.bytes_total as f64 * 100.0).min(100.0) as f32
        } else if self.files_total > 0 {
            (self.files_done as f64 / self.files_total as f64 * 100.0).min(100.0) as f32
        } else {
            0.0
        }
    }
}

/// How a transfer ended.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TransferReport {
    /// Files copied and items renamed.
    pub done: usize,
    pub skipped: usize,
    /// What failed, and why.
    pub failed: Vec<(String, String)>,
    pub cancelled: bool,
}

/// Runs `plan`: renames the whole items, creates the folders, copies every
/// file (`transfer::copy_object`: a file copy on disk, ranged GETs into a
/// file, one PUT into a bucket) by its choice - an undecided conflict is
/// skipped, never overwritten - and for a move deletes what it copied and
/// the folders it emptied. `cancel` is read before every file; `report`
/// hears the progress after every chunk and every file. A failure is noted
/// and the rest goes on.
pub fn run_transfer(
    plan: &Plan,
    source: &dyn Drive,
    target: &dyn Drive,
    kind: TransferKind,
    cancel: &AtomicBool,
    report: &mut dyn FnMut(&Progress),
) -> TransferReport {
    let mut out = TransferReport::default();
    let mut progress = Progress {
        files_total: plan.files.len() + plan.moves.len(),
        bytes_total: plan.total_bytes(),
        ..Progress::default()
    };
    if cancel.load(Ordering::SeqCst) {
        out.cancelled = true;
        return out;
    }
    for (from, to) in &plan.moves {
        if cancel.load(Ordering::SeqCst) {
            out.cancelled = true;
            return out;
        }
        progress.current = key::last_segment(from).to_string();
        match source.rename(from, to) {
            Ok(()) => out.done += 1,
            Err(e) => out.failed.push((from.clone(), crate::problems::describe(&e))),
        }
        progress.files_done += 1;
        report(&progress);
    }
    for folder in &plan.folders {
        if let Err(e) = target.create_folder(folder) {
            out.failed.push((folder.clone(), crate::problems::describe(&e)));
        }
    }
    let mut copied = Vec::new();
    for file in &plan.files {
        if cancel.load(Ordering::SeqCst) {
            out.cancelled = true;
            break;
        }
        progress.current = key::last_segment(&file.source_key).to_string();
        let base = progress.bytes_done;
        let go = !file.conflict
            || matches!(
                file.choice,
                Some(ConflictChoice::Replace) | Some(ConflictChoice::KeepBoth)
            );
        if go {
            if file.conflict && file.choice == Some(ConflictChoice::Replace) {
                // Out of the way first: a rename over a file fails on Windows.
                let _ = target.delete(&file.target_key);
            }
            let server_side = plan.same_drive && source.local_path(&file.source_key).is_none();
            let result = if server_side {
                // The bucket copies (CopyObject): no byte passes through here.
                source
                    .copy(&file.source_key, &file.target_key)
                    .map(|()| file.size.unwrap_or(0))
            } else {
                let mut on_bytes = |bytes: u64| {
                    progress.bytes_done = base + bytes;
                    report(&progress);
                };
                transfer::copy_object(
                    source,
                    &file.source_key,
                    file.size,
                    target,
                    &file.target_key,
                    &mut on_bytes,
                )
            };
            match result {
                Ok(_) => {
                    out.done += 1;
                    copied.push(file.source_key.clone());
                }
                Err(e) => out.failed.push((file.source_key.clone(), crate::problems::describe(&e))),
            }
        } else {
            out.skipped += 1;
        }
        progress.bytes_done = base + file.size.unwrap_or(0);
        progress.files_done += 1;
        report(&progress);
    }
    if kind.removes_source() && !out.cancelled {
        for key in &copied {
            if let Err(e) = source.delete(key) {
                out.failed.push((key.clone(), crate::problems::describe(&e)));
            }
        }
        // A folder goes only when everything in it went.
        if out.failed.is_empty() && out.skipped == 0 {
            for folder in &plan.source_folders {
                if let Err(e) = source.delete_folder(folder) {
                    out.failed.push((folder.clone(), crate::problems::describe(&e)));
                }
            }
        }
    }
    out
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

/// Deletes `items`: into the trash of the delete `stamp` (each renamed to
/// [`trash_key`]), or for good with `None`. Returns `(key, where it went)`
/// for every item (an empty place for good), what [`restore_items`] undoes.
pub fn delete_items(
    drive: &dyn Drive,
    items: &[SourceItem],
    stamp: Option<&str>,
) -> Result<Vec<(String, String)>, DriveError> {
    let mut gone = Vec::new();
    for item in items {
        match stamp {
            Some(stamp) if !is_in_trash(&item.key) => {
                let to = trash_key(&item.key, stamp);
                drive.rename(&item.key, &to)?;
                gone.push((item.key.clone(), to));
            }
            _ => {
                if item.is_folder {
                    drive.delete_folder(&item.key)?;
                } else {
                    drive.delete(&item.key)?;
                }
                gone.push((item.key.clone(), String::new()));
            }
        }
    }
    Ok(gone)
}

/// Undoes a delete into the trash: every item renamed back, the delete's
/// folder in the trash removed when it is empty.
pub fn restore_items(drive: &dyn Drive, gone: &[(String, String)]) -> Result<(), DriveError> {
    let mut folders = Vec::new();
    for (key, trashed) in gone {
        if trashed.is_empty() {
            continue;
        }
        drive.rename(trashed, key)?;
        if let Some(folder) = trash_folder_of(trashed) {
            if !folders.contains(&folder) {
                folders.push(folder);
            }
        }
    }
    for folder in folders {
        if storage_ops::list_all(drive, &folder)
            .map(|objects| objects.iter().all(|o| o.key.ends_with('/')))
            .unwrap_or(false)
        {
            let _ = drive.delete_folder(&folder);
        }
    }
    Ok(())
}

// ==== The transfer queue ====

/// Where a queued transfer is.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobState {
    Waiting,
    Running,
    Done,
    Failed(String),
    Cancelled,
}

/// One transfer of the queue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedJob {
    pub id: u64,
    /// "Copying 3 items to docs".
    pub label: String,
    pub state: JobState,
    pub progress: Progress,
    /// When it started running (milliseconds since 1970; 0 while it waits).
    pub started_ms: u64,
    /// The progress dialog was shown for it (once closed it stays closed).
    pub dialog_shown: bool,
}

/// The transfers, one running at a time (the source list's activity area shows it).
#[derive(Debug, Clone, Default)]
pub struct TransferQueue {
    jobs: Vec<QueuedJob>,
    next_id: u64,
}

impl TransferQueue {
    /// Queues a transfer; returns its id.
    pub fn push(&mut self, label: String) -> u64 {
        self.next_id += 1;
        self.jobs.push(QueuedJob {
            id: self.next_id,
            label,
            state: JobState::Waiting,
            progress: Progress::default(),
            started_ms: 0,
            dialog_shown: false,
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
    pub fn finish(&mut self, id: u64, error: Option<String>) {
        if let Some(job) = self.job_mut(id) {
            job.state = match error {
                Some(e) => JobState::Failed(e),
                None => JobState::Done,
            };
        }
    }

    /// A waiting transfer will not start; a running one is marked (its
    /// thread stops at the next file).
    pub fn cancel(&mut self, id: u64) {
        if let Some(job) = self.job_mut(id) {
            if matches!(job.state, JobState::Waiting | JobState::Running) {
                job.state = JobState::Cancelled;
            }
        }
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
    pub fn status_text(&self) -> String {
        let Some(job) = self.running() else {
            return match self.waiting() {
                0 => String::new(),
                n => format!("{n} waiting"),
            };
        };
        let p = &job.progress;
        let mut text = format!(
            "{} - {} of {} ({:.0}%)",
            job.label,
            p.files_done,
            p.files_total,
            p.percent()
        );
        let waiting = self.waiting();
        if waiting > 0 {
            text.push_str(&format!(", {waiting} waiting"));
        }
        text
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
        assert_eq!(copy_name("a.txt", &taken), "a - Copy (2).txt");
        assert_eq!(copy_name("c.txt", &taken), "c - Copy.txt");
        assert_eq!(copy_name("dir", &|_| false), "dir - Copy");
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
        assert!(reason.contains('|'), "{reason}");
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
            false,
            TransferKind::Copy,
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
            true,
            TransferKind::Copy,
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
            true,
            TransferKind::Move,
        )
        .unwrap();
        assert!(noop.files.is_empty() && noop.folders.is_empty());
        assert!(plan_transfer(
            &home,
            &[item("docs/")],
            &home,
            "docs/sub/",
            true,
            TransferKind::Move
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
            false,
            TransferKind::Copy,
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
            false,
            TransferKind::Move,
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
            true,
            TransferKind::Move,
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
            true,
            TransferKind::Copy,
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
        let moved =
            delete_items(&home, &[item("docs/"), item("readme.txt")], Some(&stamp)).unwrap();
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
        delete_items(&home, &[item("docs/")], None).unwrap();
        assert!(home.local_path("docs/").is_none_or(|p| !p.exists()));
    }

    #[test]
    fn a_transfer_running_two_seconds_asks_for_the_progress_dialog_once() {
        let mut queue = TransferQueue::default();
        let a = queue.push("Copying 1 item to docs".to_string());
        assert_eq!(queue.wants_progress_dialog(5_000, 2_000), None, "nothing runs");
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
        let b = queue.push("Copying 2 items to docs".to_string());
        queue.start(b, 10_000);
        assert_eq!(queue.wants_progress_dialog(12_000, 2_000), Some(b), "the next one asks again");
    }

    #[test]
    fn a_transfer_queue_runs_one_job_at_a_time_and_sums_the_progress() {
        let mut queue = TransferQueue::default();
        let a = queue.push("Copying 3 items".to_string());
        let b = queue.push("Uploading photo.jpg".to_string());
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
        assert!(
            queue.status_text().contains("Copying 3 items"),
            "{}",
            queue.status_text()
        );
        queue.finish(a, None);
        assert_eq!(queue.next_to_start(), Some(b));
        queue.start(b, 2_000);
        queue.finish(b, Some("no answer".to_string()));
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
            true,
            TransferKind::Copy,
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
            true,
            TransferKind::Copy,
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
}
