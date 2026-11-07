//! The run of a plan: every file is put in place before its source is touched (ARCH28 RF43 /
//! AR5a-2).
//!
//! For each file: the version of the source is noted, the file is written beside its name and put
//! in place in the one step that checks the name still holds what the plan saw - free, or the
//! version the user chose to replace (a file made since is a new conflict, never replaced without
//! asking) - and a [`Receipt`] says it is there. A move retires a source only by that receipt, and
//! only the version it noted: a file written to meanwhile stays. The folders a move empties are
//! removed only if nothing is in them ([`Drive::remove_empty_folder`]); a file that was made
//! there since the plan, a name no listing shows, stays with its folder. A stop is looked for per
//! chunk of the file being copied, not between files: a stopped copy leaves the target as it was.

use super::{
    plan::{Choice, Plan, PlannedFile},
    site::{overlap, Overlap},
};
use crate::{
    ctl::Ctl, durable::Expect, local, ops, quarantine, same_etag, transfer, BatchDelete, BatchMove,
    BatchPut, Drive, DriveError, ObjectInfo, Precondition,
};

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

/// How a run ended.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Report {
    /// Files copied and items renamed.
    pub done: usize,
    pub skipped: usize,
    /// What failed, and why.
    pub failed: Vec<(String, DriveError)>,
    pub cancelled: bool,
    /// The items renamed in place, `(from, to)`: what an Undo puts back - and nothing it did not
    /// do.
    pub moved: Vec<(String, String)>,
    /// The folders of a move that were not removed because something is still in them: the files
    /// that were skipped or failed, or were made there since the plan.
    pub left: Vec<String>,
}

/// The proof that a file is in place at the target. Made by [`run`] when a file landed, never
/// otherwise: the one thing [`retire_sources`] requires.
#[derive(Debug)]
pub struct Receipt {
    len: u64,
}

impl Receipt {
    fn landed(len: u64) -> Receipt {
        Receipt { len }
    }

    /// Bytes that landed.
    #[must_use]
    pub fn len(&self) -> u64 {
        self.len
    }

    /// Whether the file that landed is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

/// The version of a source file a move noted before it copied the file: the one version it may
/// delete afterwards, and no other (X-DELTA-AZDRIVE-FILES-4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceVersion {
    /// The drive's entity tag: the delete names it ([`BatchDelete::of_version`]).
    Tag(String),
    /// A drive that gives no tag: the size and the date it gave. They are looked at again right
    /// before the delete, and a source that differs is kept (an edit saved while the file was
    /// copied changes at least one of them).
    Stamp { size: u64, modified: Option<u64> },
}

impl SourceVersion {
    /// The version `info` names: its tag, else (a drive that gives none) its size and date; `None`
    /// when the drive gives nothing a later look could compare (no tag and no date).
    #[must_use]
    pub fn of(info: &ObjectInfo) -> Option<SourceVersion> {
        match (&info.etag, info.modified) {
            (Some(tag), _) => Some(SourceVersion::Tag(tag.clone())),
            (None, Some(_)) => Some(SourceVersion::Stamp {
                size: info.size,
                modified: info.modified,
            }),
            (None, None) => None,
        }
    }
}

/// One source to retire: the file, the version that was copied and the receipt of its copy.
pub struct Retire<'a> {
    pub key: &'a str,
    pub seen: &'a SourceVersion,
    pub proof: &'a Receipt,
}

/// Deletes the sources whose copies landed - the version that was copied, no other: one that
/// changed since is [`DriveError::Conflict`] and stays. A tagged source goes by its tag; a
/// source that has only a size and a date is looked at again first, and kept when either differs.
/// The only delete of a source in a transfer. One result per item, in order.
#[must_use]
pub fn retire_sources(source: &dyn Drive, items: &[Retire<'_>]) -> Vec<Result<(), DriveError>> {
    let mut results: Vec<Option<Result<(), DriveError>>> = vec![None; items.len()];
    let mut batch: Vec<BatchDelete<'_>> = Vec::new();
    let mut at: Vec<usize> = Vec::new();
    for (i, item) in items.iter().enumerate() {
        match item.seen {
            SourceVersion::Tag(tag) => {
                batch.push(BatchDelete::new(item.key).of_version(tag));
                at.push(i);
            }
            SourceVersion::Stamp { .. } => match source.head(item.key) {
                Ok(info) if SourceVersion::of(&info).as_ref() == Some(item.seen) => {
                    batch.push(BatchDelete::new(item.key));
                    at.push(i);
                }
                // Another size or date: an edit saved while the file was copied. It stays.
                Ok(_) => {
                    results[i] = Some(Err(DriveError::Conflict {
                        key: item.key.to_string(),
                    }));
                }
                // Gone already: nothing is left to retire.
                Err(DriveError::NotFound { .. }) => results[i] = Some(Ok(())),
                Err(e) => results[i] = Some(Err(e)),
            },
        }
    }
    if !batch.is_empty() {
        for (i, answer) in at.into_iter().zip(source.delete_many(&batch)) {
            results[i] = Some(answer);
        }
    }
    results
        .into_iter()
        .map(|answer| {
            answer.unwrap_or_else(|| {
                Err(DriveError::Transport(String::from(
                    "the batch gave no answer",
                )))
            })
        })
        .collect()
}

/// What the name must hold when a file is put in place there.
#[derive(Clone, Debug)]
enum Guard {
    /// Whatever is there.
    Any,
    /// Nothing: the name was free when the plan was made.
    Absent,
    /// The version the user chose to replace (the target's tag at the plan).
    Version(String),
}

impl Guard {
    fn of(file: &PlannedFile) -> Guard {
        if !file.conflict {
            return Guard::Absent;
        }
        match (file.choice, &file.target_etag) {
            (Some(Choice::Replace), Some(tag)) => Guard::Version(tag.clone()),
            // A target that tells no versions: what the user was asked about is replaced.
            (Some(Choice::Replace), None) => Guard::Any,
            // Keep both: the newcomer's own name, which was free.
            _ => Guard::Absent,
        }
    }

    /// The same as a condition of a bucket's write.
    fn condition(&self) -> Option<Precondition> {
        match self {
            Guard::Any => None,
            Guard::Absent => Some(Precondition::Absent),
            Guard::Version(tag) => Some(Precondition::Matches(tag.clone())),
        }
    }
}

/// The look at the target right before a write that cannot ask for the guard in the step that
/// puts the file in place: [`DriveError::Conflict`] when the name no longer holds what it said.
fn precheck(target: &dyn Drive, key: &str, guard: &Guard) -> Result<(), DriveError> {
    let changed = || DriveError::Conflict {
        key: key.to_string(),
    };
    match guard {
        Guard::Any => Ok(()),
        Guard::Absent => {
            if ops::exists(target, key)? {
                Err(changed())
            } else {
                Ok(())
            }
        }
        Guard::Version(tag) => match target.head(key) {
            Ok(info) if same_etag(info.etag.as_deref(), tag) => Ok(()),
            // Another version, or none: the file the user chose to replace is not there.
            Ok(_) | Err(DriveError::NotFound { .. }) => Err(changed()),
            Err(e) => Err(e),
        },
    }
}

/// What a disk target's name must hold, asked of the step that renames the file in.
fn expect_on_disk(target: &dyn Drive, key: &str, guard: &Guard) -> Result<Expect, DriveError> {
    match guard {
        Guard::Any => Ok(Expect::Any),
        Guard::Absent => Ok(Expect::Absent),
        Guard::Version(tag) => {
            // Looked at first: a system that gives files no identity compares tag, size and time
            // here, and the rename then goes on without a check of its own.
            precheck(target, key, guard)?;
            Ok(local::expect_of_tag(tag).unwrap_or(Expect::Any))
        }
    }
}

/// Copies one file of the plan; the bytes of this file come to `progress` as they go.
fn copy_one(
    source: &dyn Drive,
    target: &dyn Drive,
    file: &PlannedFile,
    guard: &Guard,
    stop: &dyn Ctl,
    progress: &mut dyn FnMut(u64),
) -> Result<u64, DriveError> {
    let on_disk = target.local_path(&file.target_key).is_some();
    let expect = if on_disk {
        expect_on_disk(target, &file.target_key, guard)?
    } else {
        precheck(target, &file.target_key, guard)?;
        Expect::Any
    };
    // The look above is an early refusal, nothing more: a big file takes an hour, and a name made
    // in that hour must not be written over. A drive that can ask asks again in the step that
    // completes the upload (X-DELTA-AZDRIVE-FILES-3).
    let condition = if on_disk { None } else { guard.condition() };
    let len = transfer::copy_object_relayed(
        source,
        &file.source_key,
        file.size,
        target,
        &file.target_key,
        transfer::When {
            expect,
            condition: condition.as_ref(),
        },
        stop,
        progress,
    )?;
    // A file of a drive that lands on this disk carries the system's "downloaded from elsewhere"
    // mark, so the system looks at an `.exe`, a script or a shortcut of a shared drive before it runs
    // it (AZ-W20); a copy from one folder of this disk to another is no download.
    if source.local_path(&file.source_key).is_none() {
        if let Some(path) = target.local_path(&file.target_key) {
            quarantine::mark_downloaded(&path);
        }
    }
    Ok(len)
}

/// The bytes of `file` when it is a small file on this computer going into a bucket: what a paste
/// sends in batches. `None` when it goes its own way (a folder on both sides, a large or unknown
/// size), `Some(Err)` when it can't be read.
fn small_local_file(
    source: &dyn Drive,
    target: &dyn Drive,
    file: &PlannedFile,
) -> Option<Result<Vec<u8>, DriveError>> {
    if target.local_path(&file.target_key).is_some() {
        return None;
    }
    let path = source.local_path(&file.source_key)?;
    let size = match file.size {
        Some(size) => size,
        None => std::fs::metadata(&path).ok()?.len(),
    };
    if size >= crate::SMALL_FILE_BYTES || !path.is_file() {
        return None;
    }
    Some(std::fs::read(&path).map_err(DriveError::from))
}

/// A file that landed: its source, the version of it that was copied, the receipt.
struct Landing {
    key: String,
    /// `None`: the drive names no version of it (the file is copied, and kept).
    seen: Option<SourceVersion>,
    proof: Receipt,
}

/// A small file read and waiting for one `put_many`.
struct Pending {
    source_key: String,
    target_key: String,
    bytes: Vec<u8>,
    guard: Guard,
    seen: Option<SourceVersion>,
}

/// Small files from this computer into a bucket go in batches (`put_many`): an encrypted drive
/// packs a batch's files together and names them in one index change, where one file at a time
/// would make each its own pack; a bucket gets fewer requests too. Each file asks its guard as a
/// condition of its own write.
#[derive(Default)]
struct SmallBatch {
    files: Vec<Pending>,
    /// Their bytes together: the batch is sent once it holds [`crate::SMALL_BATCH_BYTES`].
    bytes: u64,
}

impl SmallBatch {
    fn push(&mut self, pending: Pending) {
        self.bytes += pending.bytes.len() as u64;
        self.files.push(pending);
    }

    /// Writes the batch in one `put_many` and tells each file's outcome as a single copy does.
    fn send(
        &mut self,
        target: &dyn Drive,
        out: &mut Report,
        landed: &mut Vec<Landing>,
        progress: &mut Progress,
        report: &mut dyn FnMut(&Progress),
    ) {
        if self.files.is_empty() {
            return;
        }
        let files = std::mem::take(&mut self.files);
        self.bytes = 0;
        let conditions: Vec<Option<Precondition>> =
            files.iter().map(|file| file.guard.condition()).collect();
        let items: Vec<BatchPut<'_>> = files
            .iter()
            .zip(&conditions)
            .map(|(file, condition)| {
                let put = BatchPut::new(&file.target_key, &file.bytes);
                match condition {
                    Some(condition) => put.when(condition),
                    None => put,
                }
            })
            .collect();
        let results = target.put_many(&items);
        for ((file, condition), result) in files.into_iter().zip(&conditions).zip(results) {
            let result = match (result, condition) {
                // A drive that cannot ask: the name is looked at, then written.
                (Err(DriveError::Unsupported(_)), Some(_)) => {
                    precheck(target, &file.target_key, &file.guard)
                        .and_then(|()| target.put(&file.target_key, &file.bytes))
                }
                (other, _) => other.map(|_| ()),
            };
            match result {
                Ok(()) => {
                    out.done += 1;
                    landed.push(Landing {
                        key: file.source_key,
                        seen: file.seen,
                        proof: Receipt::landed(file.bytes.len() as u64),
                    });
                }
                Err(e) => out.failed.push((file.source_key, e)),
            }
            progress.bytes_done += file.bytes.len() as u64;
            progress.files_done += 1;
        }
        report(progress);
    }
}

/// Runs `plan`: renames the whole items, creates the folders, copies every file by its choice - an
/// undecided conflict is skipped, never overwritten - and for a move retires what landed and
/// removes the folders it emptied, if nothing else is in them. `ctl` hears the bytes of the file
/// being copied (not their sum) and is asked, per chunk, whether to stop; `report` hears the
/// progress after every chunk, every file and every batch. A failure is noted and the rest goes
/// on. A stopped run leaves the files that landed (a move retires them: each file is at the
/// source or at the target, never in neither).
pub fn run(
    plan: &Plan,
    source: &dyn Drive,
    target: &dyn Drive,
    removes_source: bool,
    ctl: &dyn Ctl,
    report: &mut dyn FnMut(&Progress),
) -> Report {
    let mut out = Report::default();
    let mut progress = Progress {
        files_total: plan.files.len() + plan.moves.len(),
        bytes_total: plan.total_bytes(),
        ..Progress::default()
    };
    if ctl.cancelled() {
        out.cancelled = true;
        return out;
    }
    // A folder is renamed as a whole; the files go in one batch (an encrypted drive changes its
    // index once for all of them, a bucket copies them side by side).
    let (folder_moves, file_moves): (Vec<&(String, String)>, Vec<&(String, String)>) =
        plan.moves.iter().partition(|(from, _)| from.ends_with('/'));
    for (from, to) in folder_moves {
        if ctl.cancelled() {
            out.cancelled = true;
            return out;
        }
        progress.current = key_name(from).to_string();
        match source.rename(from, to) {
            Ok(()) => {
                out.done += 1;
                out.moved.push((from.clone(), to.clone()));
            }
            Err(e) => out.failed.push((from.clone(), e)),
        }
        progress.files_done += 1;
        report(&progress);
    }
    if !file_moves.is_empty() {
        if ctl.cancelled() {
            out.cancelled = true;
            return out;
        }
        let batch: Vec<BatchMove<'_>> = file_moves
            .iter()
            .map(|(from, to)| BatchMove::new(from, to))
            .collect();
        for ((from, to), result) in file_moves.iter().zip(source.move_many(&batch)) {
            match result {
                Ok(()) => {
                    out.done += 1;
                    out.moved.push((from.clone(), to.clone()));
                }
                Err(e) => out.failed.push((from.clone(), e)),
            }
            progress.files_done += 1;
        }
        report(&progress);
    }
    // A folder the target would not make is a folder the move must not remove from the source: an
    // empty folder is in no file's way, and its removal would end it everywhere (X-DELTA-AZDRIVE-
    // FILES-15).
    let mut folders_made = true;
    for folder in &plan.folders {
        if let Err(e) = target.create_folder(folder) {
            folders_made = false;
            out.failed.push((folder.clone(), e));
        }
    }
    let (source_site, target_site) = (source.site(), target.site());
    let mut landed: Vec<Landing> = Vec::new();
    let mut batch = SmallBatch::default();
    for file in &plan.files {
        if ctl.cancelled() {
            // A batch not sent yet is not written: its files count as not done.
            out.cancelled = true;
            break;
        }
        progress.current = key_name(&file.source_key).to_string();
        let base = progress.bytes_done;
        let go =
            !file.conflict || matches!(file.choice, Some(Choice::Replace) | Some(Choice::KeepBoth));
        if !go {
            out.skipped += 1;
            progress.bytes_done = base + file.size.unwrap_or(0);
            progress.files_done += 1;
            report(&progress);
            continue;
        }
        // The file is its own destination (a second drive over its folder): nothing is replaced
        // by itself, and a move does not delete it as a done source.
        if overlap(
            &source_site,
            &file.source_key,
            &target_site,
            &file.target_key,
        ) == Overlap::Same
        {
            out.failed.push((
                file.source_key.clone(),
                DriveError::InvalidKey {
                    key: file.target_key.clone(),
                    reason: "the destination is the very file being copied",
                },
            ));
            progress.bytes_done = base + file.size.unwrap_or(0);
            progress.files_done += 1;
            report(&progress);
            continue;
        }
        let guard = Guard::of(file);
        // The version of the source that is copied: what a move retires and nothing else. A
        // bucket's listing says; a folder on disk is looked at now, before its bytes are read. A
        // look that FAILS is no "unknown version": the file is not copied (and not retired), the
        // failure is the file's; a drive that names no version at all copies the file and keeps it
        // (X-DELTA-AZDRIVE-FILES-4).
        let seen = if removes_source {
            match &file.source_etag {
                Some(tag) => Some(SourceVersion::Tag(tag.clone())),
                None => match source.head(&file.source_key) {
                    Ok(info) => SourceVersion::of(&info),
                    Err(e) => {
                        out.failed.push((file.source_key.clone(), e));
                        progress.bytes_done = base + file.size.unwrap_or(0);
                        progress.files_done += 1;
                        report(&progress);
                        continue;
                    }
                },
            }
        } else {
            None
        };
        let server_side = plan.same_drive && source.local_path(&file.source_key).is_none();
        if !server_side {
            if let Some(read) = small_local_file(source, target, file) {
                match read {
                    Ok(bytes) => batch.push(Pending {
                        source_key: file.source_key.clone(),
                        target_key: file.target_key.clone(),
                        bytes,
                        guard,
                        seen,
                    }),
                    Err(e) => {
                        out.failed.push((file.source_key.clone(), e));
                        progress.files_done += 1;
                        report(&progress);
                    }
                }
                if batch.bytes >= crate::SMALL_BATCH_BYTES {
                    batch.send(target, &mut out, &mut landed, &mut progress, report);
                }
                continue;
            }
        }
        let result = if server_side {
            // The bucket copies (CopyObject): no byte passes through here.
            precheck(target, &file.target_key, &guard)
                .and_then(|()| source.copy(&file.source_key, &file.target_key))
                .map(|()| file.size.unwrap_or(0))
        } else {
            let mut on_bytes = |bytes: u64| {
                progress.bytes_done = base + bytes;
                report(&progress);
            };
            copy_one(source, target, file, &guard, ctl, &mut on_bytes)
        };
        match result {
            Ok(len) => {
                out.done += 1;
                landed.push(Landing {
                    key: file.source_key.clone(),
                    seen,
                    proof: Receipt::landed(len),
                });
            }
            Err(DriveError::Cancelled) => {
                out.cancelled = true;
                break;
            }
            Err(e) => out.failed.push((file.source_key.clone(), e)),
        }
        progress.bytes_done = base + file.size.unwrap_or(0);
        progress.files_done += 1;
        report(&progress);
    }
    if !out.cancelled {
        batch.send(target, &mut out, &mut landed, &mut progress, report);
    }
    if removes_source && !landed.is_empty() {
        // A file whose version is unknown landed and stays where it is: its source is not
        // deleted on a guess, and the user is told it was copied, not moved.
        let (known, unknown): (Vec<&Landing>, Vec<&Landing>) =
            landed.iter().partition(|landing| landing.seen.is_some());
        for landing in unknown {
            out.failed.push((
                landing.key.clone(),
                DriveError::Unsupported(format!(
                    "\"{}\" was copied and is kept where it was: the drive names no version of \
                     it, so an edit made while it was copied could not be told from the file that \
                     was copied",
                    landing.key
                )),
            ));
        }
        let items: Vec<Retire<'_>> = known
            .iter()
            .filter_map(|landing| {
                Some(Retire {
                    key: &landing.key,
                    seen: landing.seen.as_ref()?,
                    proof: &landing.proof,
                })
            })
            .collect();
        for (landing, result) in known.iter().zip(retire_sources(source, &items)) {
            if let Err(e) = result {
                out.failed.push((landing.key.clone(), e));
            }
        }
    }
    if removes_source && !out.cancelled {
        // The folders the move emptied, the deepest first; one with anything left in it stays.
        let mut folders: Vec<&String> = plan.source_folders.iter().collect();
        folders.sort_by_key(|folder| std::cmp::Reverse(folder.len()));
        for folder in folders {
            if !folders_made {
                out.left.push(folder.clone());
                continue;
            }
            match source.remove_empty_folder(folder) {
                Ok(true) => {}
                Ok(false) => out.left.push(folder.clone()),
                Err(e) => out.failed.push((folder.clone(), e)),
            }
        }
    }
    out
}

/// The last name of a key, for the progress line.
fn key_name(key: &str) -> &str {
    crate::key::last_segment(key)
}
