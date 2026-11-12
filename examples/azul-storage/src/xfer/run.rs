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
    ctl::Ctl, durable::Expect, local, ops, s3::VERSIONED_GROUP, same_etag, transfer, BatchDelete,
    BatchMove, BatchPut, Drive, DriveError, ObjectInfo, Precondition,
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
/// The looks and the deletes go in groups of [`VERSIONED_GROUP`] - each group is looked at and
/// deleted before the next one is looked at, as a bucket's versioned delete does: with ten thousand
/// files on a server at 50 ms a look, the first file's look had been eight minutes old when the
/// first delete came, and an edit saved in them was deleted with it (X-DELTA-SYNC2-7).
/// The only delete of a source in a transfer. One result per item, in order.
#[must_use]
pub fn retire_sources(source: &dyn Drive, items: &[Retire<'_>]) -> Vec<Result<(), DriveError>> {
    let mut results: Vec<Option<Result<(), DriveError>>> = vec![None; items.len()];
    for (group, chunk) in items.chunks(VERSIONED_GROUP).enumerate() {
        let first = group * VERSIONED_GROUP;
        let mut batch: Vec<BatchDelete<'_>> = Vec::new();
        let mut at: Vec<usize> = Vec::new();
        for (j, item) in chunk.iter().enumerate() {
            let i = first + j;
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
    /// The file the user chose to replace, for a target that tells no tag (SFTP, FTP, a service
    /// without ETags): the size and the date it had at the plan. A file another device saved since
    /// has another date or another size, and is not replaced without asking (X-DELTA-SYNC4-11). What
    /// cannot be told apart - a save that keeps both - is the stamp's own limit; the write has no
    /// condition to carry it.
    Stamp { size: u64, modified: Option<u64> },
}

impl Guard {
    fn of(file: &PlannedFile) -> Guard {
        if !file.conflict {
            return Guard::Absent;
        }
        match (file.choice, &file.target_etag) {
            (Some(Choice::Replace), Some(tag)) => Guard::Version(tag.clone()),
            // A target that tells no tag but its size and date at the plan: those hold the replace to
            // the file the user was asked about.
            (Some(Choice::Replace), None) => match file.target_size {
                Some(size) => Guard::Stamp {
                    size,
                    modified: file.target_modified,
                },
                // A target that told nothing at all (it cannot look at a file): what the user was
                // asked about is replaced as it stands.
                None => Guard::Any,
            },
            // Keep both: the newcomer's own name, which was free.
            _ => Guard::Absent,
        }
    }

    /// The same as a condition of a bucket's write.
    fn condition(&self) -> Option<Precondition> {
        match self {
            // No condition a write can carry: the look before it holds the guard.
            Guard::Any | Guard::Stamp { .. } => None,
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
        Guard::Stamp { size, modified } => match target.head(key) {
            Ok(info) if info.size == *size && info.modified == *modified => Ok(()),
            // Another size or date, or none: the file the user chose to replace is not there.
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
        // The look is all a stamp has: the rename then goes on without a check of its own.
        Guard::Stamp { .. } => {
            precheck(target, key, guard)?;
            Ok(Expect::Any)
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
    // A write that asks its guard itself - the rename of a folder whose file system gives files an
    // identity, the request that completes an upload to a drive that writes conditionally - needs no
    // more. Any other (a drive that cannot, a folder whose `expect` is Any for want of an identity)
    // looked at the name when the copy began and looks once more right before it writes: what is left
    // of the window is the write's own step, not the copy (X-DELTA-SYNC2-9).
    let asks_itself = if on_disk {
        !matches!(expect, Expect::Any)
    } else {
        // A drive that writes conditionally asks only a guard that has a condition to send.
        condition.is_some() && target.conditional_writes()
    };
    let look = || precheck(target, &file.target_key, guard);
    let last_look: Option<transfer::Look<'_>> =
        (!asks_itself && !matches!(guard, Guard::Any)).then_some(&look);
    let len = transfer::copy_object_relayed(
        source,
        &file.source_key,
        file.size,
        target,
        &file.target_key,
        transfer::When {
            expect,
            condition: condition.as_ref(),
            last_look,
        },
        stop,
        progress,
    )?;
    // A file of a drive that lands on this disk carries the system's "downloaded from elsewhere"
    // mark (AZ-W20): `copy_object_when` puts it on, for every caller of a copy.
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
        // A guard that has no condition to send - a stamp - is looked at right before the batch is
        // written: the file another device saved since the plan is a conflict, not replaced.
        let mut asked = Vec::with_capacity(files.len());
        for file in files {
            if let Guard::Stamp { .. } = file.guard {
                if let Err(e) = precheck(target, &file.target_key, &file.guard) {
                    progress.bytes_done += file.bytes.len() as u64;
                    progress.files_done += 1;
                    out.failed.push((file.source_key, e));
                    continue;
                }
            }
            asked.push(file);
        }
        let files = asked;
        if files.is_empty() {
            report(progress);
            return;
        }
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
///
/// What a copy guarantees about the NAME it writes - a name free when the plan was made stays
/// unwritten if another device made it since; a version the user chose to replace is replaced only
/// while it is still that version (the plan's guard) - and what it leaves:
///
/// - a folder on this computer whose file system gives files an identity: the rename that puts the
///   file in place asks ([`Expect`]); nothing can come between;
/// - a drive that [writes conditionally](Drive::conditional_writes) (a bucket, an encrypted drive):
///   the request that completes the object asks (`If-None-Match`, `If-Match`); a big file that
///   REPLACES a version inside its own bucket goes through that copy and not the bucket's own;
/// - any other target (a drive that cannot write conditionally; a folder whose file system gave the
///   file no identity): the name is looked at when the copy begins and once more, right before the
///   write, when the last byte has been read ([`transfer::When::last_look`]). What is left is the write's
///   own step - one PUT, one rename - and not the minutes the copy took;
/// - a server-side copy inside one bucket (a small file that replaces, a file to a free name, a drive
///   whose own copy is one change of its index): the name is looked at right before the service is
///   asked, and the service's CopyObject asks no condition of the name it writes (S3 has none for it):
///   a name another device saves WHILE the service copies is written over. For a small object that is
///   milliseconds; for a big one to a free name it is the service's time, which the bucket's shortcut
///   buys (streaming it through this computer would cost the egress of the whole file).
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
    // Names a drive gave that this computer's disk would read as something else (`.GIT/config` is
    // `.git/config` on APFS and NTFS) are not written: each is a failure of its own, and a move
    // leaves it where it is (X-DELTA-AZDRIVE-FILES-32).
    for (source_key, _why) in &plan.refused {
        out.failed.push((
            source_key.clone(),
            DriveError::InvalidKey {
                key: source_key.clone(),
                reason: "it is not a name this computer can hold as it is meant",
            },
        ));
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
        // A bucket copies inside itself (CopyObject: no byte passes through here) - but its copy asks
        // no condition of the name it writes, and a big object takes the service long enough for
        // another device to save that name meanwhile. A big file that REPLACES the version the plan
        // saw therefore goes the long way, through the streamed copy whose last request carries the
        // condition (X-DELTA-SYNC2-9); a small one, a free name and a drive whose own copy is one
        // change of its index keep the shortcut.
        let replaces_big = target_site.is_bucket()
            && matches!(guard, Guard::Version(_))
            && file.size.is_none_or(|size| size > transfer::CHUNK);
        let server_side =
            plan.same_drive && source.local_path(&file.source_key).is_none() && !replaces_big;
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
                // A copy between two folders of this computer carries what the file carries
                // besides its bytes (tags, labels, the resource fork); what the disk of the copy
                // would not take decides what happens to the source (X-DELTA-AZDRIVE-FILES-16).
                let lost = lost_attributes(source, target, file);
                match attrs_outcome(&lost, removes_source, &file.source_key) {
                    AttrsOutcome::KeepSource(why) => {
                        out.failed
                            .push((file.source_key.clone(), DriveError::Unsupported(why)));
                    }
                    outcome => {
                        if let AttrsOutcome::Note(note) = outcome {
                            crate::cap::note_unchecked(&note);
                        }
                        landed.push(Landing {
                            key: file.source_key.clone(),
                            seen,
                            proof: Receipt::landed(len),
                        });
                    }
                }
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

/// What the attributes of a file that was copied between two folders of this computer mean for the
/// run.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum AttrsOutcome {
    /// The copy has everything the original carries (or the original carries nothing).
    Whole,
    /// A Move: the copy lacks attributes of the original, which stays where it is - the sentence.
    KeepSource(String),
    /// A Copy: the copy lacks attributes of the original, and the report says so - the sentence.
    Note(String),
}

/// What the attributes `lost` (the names a disk would not take) mean for the copy of `key`: a Move
/// keeps its source, a Copy says it once.
pub(crate) fn attrs_outcome(lost: &[String], removes_source: bool, key: &str) -> AttrsOutcome {
    if lost.is_empty() {
        return AttrsOutcome::Whole;
    }
    let names = lost.join(", ");
    if removes_source {
        AttrsOutcome::KeepSource(format!(
            "\"{key}\" was copied and is kept where it was: the disk of the copy would not take \
             its attributes ({names})"
        ))
    } else {
        AttrsOutcome::Note(format!(
            "\"{key}\": the copy lacks attributes of the original that its disk would not take \
             ({names})"
        ))
    }
}

/// Carries the attributes of `file`'s source to its copy when both are files of this computer;
/// the names the copy lacks.
fn lost_attributes(source: &dyn Drive, target: &dyn Drive, file: &PlannedFile) -> Vec<String> {
    match (
        source.local_path(&file.source_key),
        target.local_path(&file.target_key),
    ) {
        (Some(from), Some(to)) => crate::xattrs::carry(&from, &to).lost,
        _ => Vec::new(),
    }
}

/// The last name of a key, for the progress line.
fn key_name(key: &str) -> &str {
    crate::key::last_segment(key)
}
