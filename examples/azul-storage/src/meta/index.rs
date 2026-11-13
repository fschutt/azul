//! The drive index as the encrypted drive's [`NameIndex`] (feature `encryption`):
//! [`MetaIndex`], and [`open_encrypted_drive`], the drive it makes.
//!
//! - **A file** at `a/b.txt` is the pointer blob ([`super::pointer`]) at that path in the tree.
//! - **A folder marker** `a/` is the folder `a` itself: every folder has one (as if every folder
//!   had been made with `create_folder`), with no date. Removing a marker removes the folder
//!   only when nothing is left in it at the end of the change; deleting a folder's files is the
//!   drive's work (`delete_folder` removes every entry under it).
//! - **Reads** (`get`, `list`) answer from the device's copy, polled at most every few seconds
//!   (one conditional GET, a 304 when nothing changed), and never list the bucket: a folder is
//!   its tree ([`super::tree::folder_at`]). A lazy copy reads the folder's chunks first.
//! - **A change** (`apply`) is a compare-and-swap of the whole batch against the drive as it is:
//!   inside the publish, after the poll that brought the other devices' entries, every
//!   expectation is checked against their state, and the batch is committed on top of it. A
//!   failed expectation is [`DriveError::Conflict`] and nothing changes; a swap lost to another
//!   device is checked and applied again on the newer state. The index keeps the history, so what
//!   it releases is only HELD by it ([`Released::held_by_history`]): the old versions stay
//!   restorable, and no one deletes them but the reclaimer, past their retention.
//! - `.azlin` at the root (the drive's policy and keys) and the shards' names are no drive
//!   paths: hidden from reads, refused for changes. Only the sync's bookkeeping under it
//!   ([`SYNC_BOOKKEEPING`], the record of a folder synced at the root) is read and written by
//!   its key, sealed like every file and never listed.

use std::{
    cmp::Reverse,
    collections::{BTreeMap, HashSet},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use super::{
    bucket::{Bucket, DriveBucket},
    hex, keys,
    objects::{Commit, Mode, ObjectId},
    pack::PackWriter,
    pointer,
    repo::{
        remote_head, unpublished, unreachable, Local, MetaRepo, RepoOptions, RestoreSource, MAIN,
    },
    retention::RETENTION,
    seal::{KeyWindow, Sealer},
    shadow::{Footing, Ledger, Round, ANALYSIS_RULES, UNVERIFIED},
    shard::{self, SHARD_PREFIX},
    tree::{apply, entry_at, folder_at, Change},
    wal::{reseal, Maintained, Maintenance, MetaStore, Packs, Publish, RefUpdate},
    MetaError,
};
use crate::{
    crypto::{DriveKey, ObjectId as DataId},
    encrypted::{EncryptedDrive, IndexChange, IndexEntry, IndexPage, IndexProvider, NameIndex},
    key::folder_of,
    meta::reclaim::{
        analyse, ask_retired_delete, manifest_pack_bytes, pack_bytes_of, purge_options, reclaim,
        reclaim_judged, stamp_agrees, Analysis, Caps, DeleteLicence, GoEvidence, History, Hold,
        LiveObjects, NotYet, PurgeConfirmation, ReclaimMode, ReclaimOptions, ReclaimReport,
        Released, ServerClock, Standing,
    },
    Drive, DriveError, ListRequest, DEFAULT_PAGE_SIZE,
};
use sha2::{Digest, Sha256};

/// Reads poll the bucket at most this often by default (seconds).
pub const DEFAULT_POLL_SECS: u64 = 5;

/// The marker entry of a folder.
fn marker() -> IndexEntry {
    IndexEntry {
        size: 0,
        modified: None,
        object: None,
    }
}

/// The sync's bookkeeping in the drive's own folder: the record of a folder synced at the root,
/// a file of the drive (sealed, kept in the history) that no listing shows.
pub const SYNC_BOOKKEEPING: &str = ".azlin/sync/";

/// Whether `path` (a key or a folder prefix) is the drive's own: `.azlin` at
/// the root (but the sync's bookkeeping in it), or a segment of a shard's name.
fn reserved(path: &str) -> bool {
    let mut segments = path.trim_end_matches('/').split('/');
    (segments
        .next()
        .is_some_and(crate::names::Protected::is_bookkeeping)
        && !path.starts_with(SYNC_BOOKKEEPING))
        || path.split('/').any(|segment| segment.starts_with(SHARD_PREFIX))
}

/// The drive's error for an index error: the drive's own index and its upkeep. An object sealed under another
/// key than the one this device holds is the key to ask for (`KeyRefused`; the layer that knows the drive names
/// it), not a damaged file (X-TESTEDGE-A-1); a changed byte stays `Corrupt`.
pub(super) fn to_drive(e: MetaError) -> DriveError {
    match e {
        MetaError::Sealed {
            reason,
            other_key: true,
            ..
        } => DriveError::KeyRefused {
            drive: String::new(),
            reason: format!(
                "its files are sealed under another key than the one this device holds ({reason})"
            ),
        },
        other => to_drive_untyped(other),
    }
}

/// [`to_drive`] where the repository is opened with a CANDIDATE key - the member record (`MemberRecord`): "it
/// does not open with this key" stays `Corrupt` there, as it was, and what it means is the caller's to say
/// (`device::catch_up` and the rotation decide on the key they hold: a device whose own key change stopped
/// halfway is told by the kind of this error to look at its next key, and a `KeyRefused` is the answer of a
/// wrap that names another key).
pub(super) fn to_drive_untyped(e: MetaError) -> DriveError {
    match e {
        MetaError::Conflict { key } => DriveError::Conflict { key },
        MetaError::Drive(e) => e,
        MetaError::Corrupt { key, reason } => DriveError::Corrupt { key, reason },
        MetaError::Unsupported(why) => DriveError::Unsupported(why),
        // A change whose landing is not known is a request that got no answer: a caller never
        // takes it for a refusal (`Conflict`), which lets it undo what the change may have named.
        e @ MetaError::OutcomeUnknown { .. } => DriveError::Transport(e.to_string()),
        MetaError::Sealed { key, reason, .. } => DriveError::Corrupt {
            key,
            reason: format!("it does not open with this drive's key: {reason}"),
        },
        other => DriveError::Protocol(other.to_string()),
    }
}

/// The pointer at `path` of the (local) tree `root`, or the marker of the
/// folder `path` (ending in `/`).
fn entry_of(
    local: &Local,
    root: &ObjectId,
    path: &str,
) -> Result<Option<IndexEntry>, MetaError> {
    if let Some(folder) = path.strip_suffix('/') {
        return Ok(folder_at(&local.objects, root, folder)?.map(|_| marker()));
    }
    match entry_at(&local.objects, root, path)? {
        Some(entry) if entry.mode == Mode::File => {
            let blob = local.objects.blob(&entry.id)?;
            pointer::decode(blob).map(Some).map_err(|reason| {
                if pointer::newer_format(blob).is_some() {
                    MetaError::Unsupported(format!("\"{path}\": {reason}"))
                } else {
                    MetaError::Corrupt {
                        key: path.to_string(),
                        reason,
                    }
                }
            })
        }
        _ => Ok(None),
    }
}

/// Makes what a change of `path` reads and rewrites local: every folder on
/// the way whole (a change rewrites them), and the entry itself.
fn ensure_for_change<B: Bucket, S: Sealer>(
    local: &mut Local,
    root: &ObjectId,
    path: &str,
    packs: &Packs<'_, B, S>,
) -> Result<(), MetaError> {
    let trimmed = path.trim_end_matches('/');
    let mut folder = String::new();
    local.ensure_folder(root, "", false, packs)?;
    let parts: Vec<&str> = trimmed.split('/').collect();
    let last = parts.len().saturating_sub(1);
    for (i, part) in parts.iter().enumerate() {
        // A file's own path is no folder; a folder marker's is.
        if i == last && !path.ends_with('/') {
            break;
        }
        if !folder.is_empty() {
            folder.push('/');
        }
        folder.push_str(part);
        if local.ensure_folder(root, &folder, false, packs)?.is_none() {
            return Ok(());
        }
    }
    if !path.ends_with('/') {
        local.ensure_entry(root, trimmed, packs)?;
    }
    Ok(())
}

impl<B: Bucket, S: Sealer> MetaRepo<B, S> {
    /// Applies an index batch as one compare-and-swap against the drive as it
    /// is (see the module docs); `Conflict` when an expectation does not hold. The data objects
    /// the batch left unnamed in the head - they stay in the history, which still names them.
    pub fn apply_index(&mut self, changes: &[IndexChange]) -> Result<Vec<DataId>, MetaError> {
        for change in changes {
            if reserved(change_path(change)) {
                return Err(MetaError::Drive(DriveError::InvalidKey {
                    key: change_path(change).to_string(),
                    reason: "it is the drive's own bookkeeping (.azlin)",
                }));
            }
        }
        // A restore announced since the last poll: the copy is dropped first, the publish below
        // reads the restored drive anew.
        self.take_restore();
        let signature = self.signature();
        let local = &mut self.local;
        let mut outcome: Option<(ObjectId, Vec<ObjectId>, Vec<DataId>)> = None;
        let result = self.store.publish_with(|state, packs| {
            local.refresh(state, packs)?;
            let remote = remote_head(state)?;
            let base = match remote {
                Some(head) => {
                    local.ensure(&[head], packs)?;
                    Some(local.objects.commit(&head)?.tree)
                }
                None => None,
            };
            if let Some(root) = &base {
                for change in changes {
                    ensure_for_change(local, root, change_path(change), packs)?;
                }
            }
            // What each path holds as the batch goes.
            let mut staged: BTreeMap<String, Option<IndexEntry>> = BTreeMap::new();
            let mut tree_changes: Vec<Change> = Vec::new();
            let mut removed_folders: Vec<String> = Vec::new();
            // The objects the batch leaves unnamed in the head.
            let mut held: Vec<DataId> = Vec::new();
            for change in changes {
                let path = change_path(change);
                let current = match staged.get(path) {
                    Some(entry) => entry.clone(),
                    None => match &base {
                        Some(root) => entry_of(local, root, path)?,
                        None => None,
                    },
                };
                let expect = match change {
                    IndexChange::Put { expect, .. } | IndexChange::Remove { expect, .. } => expect,
                };
                if !expect.holds(current.as_ref()) {
                    return Err(MetaError::Conflict {
                        key: path.to_string(),
                    });
                }
                match change {
                    IndexChange::Put { path, entry, .. } => {
                        if let Some(folder) = path.strip_suffix('/') {
                            removed_folders.retain(|f| f != folder);
                            tree_changes.push(Change::Folder {
                                path: folder.to_string(),
                            });
                            staged.insert(path.clone(), Some(marker()));
                        } else {
                            if let Some(old) = current.as_ref().and_then(IndexEntry::bucket_object)
                            {
                                if entry.bucket_object() != Some(old) {
                                    held.push(old);
                                }
                            }
                            let id = local.objects.write_blob(&pointer::encode(entry));
                            tree_changes.push(Change::Put {
                                path: path.clone(),
                                id,
                            });
                            staged.insert(path.clone(), Some(entry.clone()));
                        }
                    }
                    IndexChange::Remove { path, .. } => {
                        if let Some(folder) = path.strip_suffix('/') {
                            removed_folders.push(folder.to_string());
                        } else if current.is_some() {
                            held.extend(current.as_ref().and_then(IndexEntry::bucket_object));
                            tree_changes.push(Change::Delete { path: path.clone() });
                        }
                        staged.insert(path.clone(), None);
                    }
                }
            }
            if tree_changes.is_empty() && removed_folders.is_empty() {
                return Ok(None);
            }
            let mut root = apply(&mut local.objects, base.as_ref(), &tree_changes)?;
            // A removed marker takes its folder only when nothing is left in it;
            // the deepest first, so emptied parents go too.
            removed_folders.sort_by_key(|folder| Reverse(folder.matches('/').count()));
            for folder in removed_folders {
                let empty = folder_at(&local.objects, &root, &folder)?.is_some_and(|f| f.is_empty());
                if empty {
                    root = apply(&mut local.objects, Some(&root), &[Change::Delete { path: folder }])?;
                }
            }
            if base == Some(root) {
                return Ok(None);
            }
            let commit = local.objects.write_commit(&Commit {
                tree: root,
                parents: remote.into_iter().collect(),
                author: signature.clone(),
                committer: signature.clone(),
                message: "Drive change\n".to_string(),
            });
            let new_objects = unpublished(&local.objects, &commit, &local.published)?;
            let mut writer = PackWriter::new();
            for id in &new_objects {
                writer.add_from(&local.objects, id)?;
            }
            held.sort();
            held.dedup();
            outcome = Some((commit, new_objects, held));
            Ok(Some(Publish {
                pack: Some(writer),
                updates: vec![RefUpdate {
                    name: MAIN.to_string(),
                    old: remote.map(|r| r.to_hex()),
                    new: Some(commit.to_hex()),
                }],
                message: "Drive change".to_string(),
            }))
        });
        let result = match result {
            Ok(result) => result,
            Err(e) => {
                // A refused batch still brought the drive's head (and its commit) to
                // this device: reads show what the expectation was checked against.
                if let Ok(Some(head)) = remote_head(self.store.state()) {
                    if self.local.objects.contains(&head) {
                        self.head = Some(head);
                    }
                }
                return Err(e);
            }
        };
        let held = match (result, outcome) {
            (Some(published), Some((head, packed, held))) => {
                self.landed(Some(published), &packed)?;
                self.head = Some(head);
                held
            }
            _ => {
                // Nothing to change: the copy is at the drive's head anyway.
                self.head = remote_head(self.store.state())?;
                Vec::new()
            }
        };
        self.save();
        Ok(held)
    }
}

fn change_path(change: &IndexChange) -> &str {
    match change {
        IndexChange::Put { path, .. } | IndexChange::Remove { path, .. } => path,
    }
}

/// A device's copy and when it last polled.
struct Inner<B: Bucket, S: Sealer> {
    repo: MetaRepo<B, S>,
    last_poll: Option<u64>,
    /// The last listing's entries, folded by its delimiter and sorted: its next page at the same
    /// tree is cut from them instead of walking the tree and reading every pointer again (a
    /// listing of N entries in pages of P would cost N * N / P). Any change of the tree makes a
    /// new root, and the next listing walks again.
    listed: Option<Listing>,
    /// How many times a listing walked the tree and read every pointer: the paging test counts them
    /// (X-SYNC4L-1: a paged listing walks the tree once, not once per page).
    #[cfg(test)]
    walks: usize,
}

/// One listing of [`Inner::listed`]: the tree, the request it answers, its keys.
struct Listing {
    root: ObjectId,
    prefix: String,
    delimiter: Option<String>,
    keys: Vec<(String, Option<IndexEntry>)>,
}

/// The drive index as a [`NameIndex`]: an [`EncryptedDrive`] over it keeps its
/// names, folders, sizes and dates in the bucket's metadata repository and
/// browses without listing the bucket.
pub struct MetaIndex<B: Bucket, S: Sealer> {
    inner: Mutex<Inner<B, S>>,
    poll_every: u64,
    /// What [`NameIndex::live_objects`] may read: `Some(budget)` the whole history of the drive,
    /// when its packs are at most `budget` bytes (the pack upkeep of a drive whose reclaimer may
    /// delete asks it); `None` the head alone (a drive whose reclaimer deletes nothing: the upkeep
    /// then never deletes either, and what the history names is not its business).
    history_budget: Option<u64>,
}

impl<B: Bucket, S: Sealer> MetaIndex<B, S> {
    /// The index of `repo` (opened, pulled).
    #[must_use]
    pub fn new(repo: MetaRepo<B, S>) -> Self {
        MetaIndex {
            inner: Mutex::new(Inner {
                repo,
                last_poll: None,
                listed: None,
                #[cfg(test)]
                walks: 0,
            }),
            poll_every: DEFAULT_POLL_SECS,
            history_budget: Some(Caps::default().pack_bytes),
        }
    }

    /// What the upkeep may ask of the history ([`MetaIndex::history_budget`]): `None` the head
    /// alone, `Some(bytes)` the whole history of a drive whose packs are at most that big.
    #[must_use]
    pub fn with_history_budget(mut self, budget: Option<u64>) -> Self {
        self.history_budget = budget;
        self
    }

    /// Reads poll the bucket at most every `secs` seconds (0: before every read).
    #[must_use]
    pub fn with_poll_every(mut self, secs: u64) -> Self {
        self.poll_every = secs;
        self
    }

    fn lock(&self) -> MutexGuard<'_, Inner<B, S>> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// How many times a listing walked the tree and read every pointer (a page cut from the listing
    /// before is no walk): the paging test's measure, where a wall clock was its bound.
    #[cfg(test)]
    pub(crate) fn listing_walks(&self) -> usize {
        self.lock().walks
    }

    /// Runs `f` with the device's copy (to commit, to restore, for maintenance).
    pub fn with_repo<T>(&self, f: impl FnOnce(&mut MetaRepo<B, S>) -> T) -> T {
        f(&mut self.lock().repo)
    }

    /// The copy, polled when the last poll is older than the poll time - or at once when `asked`
    /// (a user's refresh, [`ListRequest::refresh`]): then a bucket that refuses says so; one
    /// that cannot be reached still leaves the copy to answer.
    fn fresh(&self, asked: bool) -> Result<MutexGuard<'_, Inner<B, S>>, DriveError> {
        let mut inner = self.lock();
        let now = inner.repo.store().now();
        let due = asked
            || inner
                .last_poll
                .map_or(true, |last| now.saturating_sub(last) >= self.poll_every);
        if due {
            match inner.repo.pull() {
                Ok(_) => {}
                // No bucket: reads go on from the copy (changes need the bucket).
                Err(e) if unreachable(&e) && inner.repo.head().is_some() => {}
                Err(e) => return Err(to_drive(e)),
            }
            inner.last_poll = Some(now);
        }
        Ok(inner)
    }
}

impl<B: Bucket, S: Sealer> MetaIndex<B, S> {
    /// The entry of `path` in `repo`'s copy, its objects read first.
    fn lookup(repo: &mut MetaRepo<B, S>, path: &str) -> Result<Option<IndexEntry>, DriveError> {
        let Some(root) = repo.root().map_err(to_drive)? else {
            return Ok(None);
        };
        match path.strip_suffix('/') {
            Some(folder) => repo.ensure_folder(folder, false).map_err(to_drive)?,
            None => {
                repo.ensure_entry(path).map_err(to_drive)?;
                None
            }
        };
        entry_of(&repo.local, &root, path).map_err(to_drive)
    }
}

impl<B: Bucket, S: Sealer> NameIndex for MetaIndex<B, S> {
    fn get(&self, path: &str) -> Result<Option<IndexEntry>, DriveError> {
        if path.is_empty() || reserved(path) {
            return Ok(None);
        }
        let mut inner = self.fresh(false)?;
        Self::lookup(&mut inner.repo, path)
    }

    /// The head and the window, read from the drive now: the copy is polled (an error when the
    /// drive cannot be reached) and read whole, then every state inside the retention is walked.
    /// Holds this device's copy for the walk: a call for the rare round of upkeep, not for a
    /// listing - and only for a drive whose index is within the budget of a run
    /// ([`Caps::pack_bytes`]; a bigger one is "too big to judge", an error, and nothing is
    /// deleted). A drive whose reclaimer deletes nothing ([`MetaIndex::with_history_budget`] of
    /// `None`) answers with the head alone, from the copy's own listing: the upkeep deletes nothing
    /// there, so it needs only what IS named, and the history is not read (it was read whole, into
    /// the app's live copy, at every maintenance round that found a pack due - in shadow mode
    /// forever, since nothing it found due could be deleted).
    fn live_objects(&self, now: u64) -> Result<Option<LiveObjects>, DriveError> {
        let Some(budget) = self.history_budget else {
            return Ok(Some(LiveObjects::of(listed_objects(self)?.0, now)));
        };
        let mut inner = self.lock();
        inner.repo.pull().map_err(to_drive)?;
        let bytes = pack_bytes_of(&inner.repo);
        if bytes > budget {
            return Err(DriveError::Protocol(
                NotYet::TooBig { bytes, budget }.to_string(),
            ));
        }
        inner.repo.fetch_all().map_err(to_drive)?;
        let Some(head) = inner.repo.head() else {
            return Ok(Some(LiveObjects::empty(now)));
        };
        let analysis = analyse(
            inner.repo.objects(),
            &head,
            now,
            &RETENTION,
            None,
            &Caps::default(),
            true,
        )
        .map_err(|why| DriveError::Protocol(why.to_string()))?;
        Ok(Some(LiveObjects::from_analysis(&analysis)))
    }

    /// The drive is polled now; a drive that cannot be reached is an error, never the copy's
    /// answer.
    fn get_fresh(&self, path: &str) -> Result<Option<IndexEntry>, DriveError> {
        if path.is_empty() || reserved(path) {
            return Ok(None);
        }
        let mut inner = self.lock();
        inner.repo.pull().map_err(to_drive)?;
        let now = inner.repo.store().now();
        inner.last_poll = Some(now);
        Self::lookup(&mut inner.repo, path)
    }

    fn list(&self, request: &ListRequest) -> Result<IndexPage, DriveError> {
        let prefix = request.prefix.as_str();
        if reserved(prefix) {
            return Ok(IndexPage::default());
        }
        let mut guard = self.fresh(request.refresh)?;
        let inner = &mut *guard;
        let repo = &mut inner.repo;
        let Some(root) = repo.root().map_err(to_drive)? else {
            return Ok(IndexPage::default());
        };
        if let Some(listing) = inner.listed.as_ref().filter(|l| {
            l.root == root && l.prefix == request.prefix && l.delimiter == request.delimiter
        }) {
            return Ok(page_from(&listing.keys, request));
        }
        let folder_part = folder_of(prefix);
        let one_level = request.delimiter.as_deref() == Some("/");
        // (key, entry) of everything that may be listed; folders as their markers.
        let mut found: Vec<(String, IndexEntry)> = Vec::new();
        let mut pending: Vec<String> = vec![folder_part.trim_end_matches('/').to_string()];
        while let Some(folder) = pending.pop() {
            let Some(folder_id) = repo.ensure_folder(&folder, true).map_err(to_drive)? else {
                continue;
            };
            let dir = if folder.is_empty() {
                String::new()
            } else {
                format!("{folder}/")
            };
            if !dir.is_empty() && dir.starts_with(prefix) {
                found.push((dir.clone(), marker()));
            }
            let entries = shard::read_folder(repo.objects(), &folder_id).map_err(to_drive)?;
            for entry in entries.entries() {
                if dir.is_empty() && crate::names::Protected::is_bookkeeping(&entry.name) {
                    continue;
                }
                let key = format!("{dir}{}", entry.name);
                match entry.mode {
                    Mode::File => {
                        if key.starts_with(prefix) {
                            let blob = repo.objects().blob(&entry.id).map_err(to_drive)?;
                            let pointer = pointer::decode(blob).map_err(|reason| {
                                if pointer::newer_format(blob).is_some() {
                                    DriveError::Unsupported(format!("\"{key}\": {reason}"))
                                } else {
                                    DriveError::Corrupt {
                                        key: key.clone(),
                                        reason,
                                    }
                                }
                            })?;
                            found.push((key, pointer));
                        }
                    }
                    Mode::Tree => {
                        let marker_key = format!("{key}/");
                        if one_level {
                            if marker_key.starts_with(prefix) {
                                found.push((marker_key, marker()));
                            }
                        } else if marker_key.starts_with(prefix) || prefix.starts_with(&marker_key) {
                            pending.push(key);
                        }
                    }
                }
            }
        }
        let keys = sorted_keys(found, request);
        let page = page_from(&keys, request);
        #[cfg(test)]
        {
            inner.walks += 1;
        }
        inner.listed = Some(Listing {
            root,
            prefix: request.prefix.clone(),
            delimiter: request.delimiter.clone(),
            keys,
        });
        Ok(page)
    }

    fn apply(&self, changes: Vec<IndexChange>) -> Result<Released, DriveError> {
        let mut inner = self.lock();
        let held = inner.repo.apply_index(&changes).map_err(to_drive)?;
        let now = inner.repo.store().now();
        inner.last_poll = Some(now);
        // The index keeps the history: nothing it released may be deleted now.
        Ok(Released::held(held))
    }
}

/// The keys of `found` for `request`, with S3's semantics as the memory index has them: keys
/// below the next delimiter after the prefix collapse into one folder (`None`), sorted, each
/// once.
fn sorted_keys(
    found: Vec<(String, IndexEntry)>,
    request: &ListRequest,
) -> Vec<(String, Option<IndexEntry>)> {
    let mut keys: Vec<(String, Option<IndexEntry>)> = Vec::with_capacity(found.len());
    for (key, entry) in found {
        let rest = &key[request.prefix.len()..];
        let folder = request
            .delimiter
            .as_deref()
            .filter(|d| !d.is_empty())
            .and_then(|d| rest.find(d).map(|i| &rest[..i + d.len()]));
        match folder {
            Some(folder) => keys.push((format!("{}{folder}", request.prefix), None)),
            None => keys.push((key, Some(entry))),
        }
    }
    keys.sort_by(|a, b| a.0.cmp(&b.0));
    keys.dedup_by(|a, b| a.0 == b.0);
    keys
}

/// One page of the sorted `keys` for `request`: after the continuation, `max_keys` at most.
fn page_from(keys: &[(String, Option<IndexEntry>)], request: &ListRequest) -> IndexPage {
    let start = match request.continuation.as_deref() {
        Some(after) => keys.partition_point(|(key, _)| key.as_str() <= after),
        None => 0,
    };
    let rest = &keys[start..];
    let size = if request.max_keys == 0 {
        DEFAULT_PAGE_SIZE as usize
    } else {
        request.max_keys as usize
    };
    let next = (rest.len() > size).then(|| rest[size - 1].0.clone());
    let mut page = IndexPage {
        next,
        ..IndexPage::default()
    };
    for (key, entry) in rest.iter().take(size) {
        match entry {
            Some(entry) => page.entries.push((key.clone(), entry.clone())),
            None => page.folders.push(key.clone()),
        }
    }
    page
}

/// The drive index of the bucket `inner` under `sealer` (the drive key, or a rotation's
/// window): opened, or created for a bucket without one (and opened when another device
/// created it in the same moment).
fn open_or_create<D: Drive + ?Sized + 'static, S: Sealer + Clone>(
    inner: &Arc<D>,
    sealer: &S,
    device_id: &str,
    device_name: &str,
    options: &RepoOptions,
) -> Result<MetaRepo<DriveBucket<Arc<D>>, S>, DriveError> {
    let open = || {
        MetaRepo::open_with(
            DriveBucket::new(Arc::clone(inner)),
            sealer.clone(),
            device_id,
            device_name,
            options,
        )
    };
    match open() {
        Ok(repo) => Ok(repo),
        Err(MetaError::NoRepository) => {
            // "No manifest" is not "no repository": a drive that lost only its manifest keeps its
            // log, its packs - and its files. A new, empty index created over them is the winner of
            // the next sweep, and everything the old one knew (names, pointers, wrapped keys) is
            // gone two days later (AZ-M11). A repository is created only for a drive that holds
            // none of it.
            // The listing is the proof: a drive that cannot be listed is not called empty.
            if holds_remnants(inner.as_ref())? {
                // Another device may have created the repository in this very moment: look again
                // before calling it lost.
                return match open() {
                    Ok(repo) => Ok(repo),
                    Err(MetaError::NoRepository) => Err(DriveError::Corrupt {
                        key: super::keys::MANIFEST.to_string(),
                        reason: String::from(
                            "the drive's index has lost its manifest but not its files: a new \
                             index is not created over them",
                        ),
                    }),
                    Err(e) => Err(to_drive(e)),
                };
            }
            create_index(inner, sealer, device_id, device_name, options, open)
        }
        Err(e) => Err(to_drive(e)),
    }
}

/// Whether the drive in `bucket` holds what an index leaves behind or what it names: an object
/// under the repository's own folder, or a data object (`data/<two hex digits>/...`).
fn holds_remnants<D: Drive + ?Sized>(bucket: &D) -> Result<bool, DriveError> {
    let mut request = ListRequest::recursive(super::keys::ROOT);
    request.max_keys = 1;
    if !bucket.list(&request)?.objects.is_empty() {
        return Ok(true);
    }
    // A user's own folder `data/` is no data object; only the fan-out folders are.
    let mut request = ListRequest::folder("data/");
    for _ in 0..8 {
        let page = bucket.list(&request)?;
        let fan_out = |folder: &String| {
            folder
                .strip_prefix("data/")
                .and_then(|rest| rest.strip_suffix('/'))
                .is_some_and(|digits| {
                    digits.len() == 2
                        && digits
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                })
        };
        if page.folders.iter().any(fan_out) {
            return Ok(true);
        }
        match page.next {
            Some(token) => request = request.with_continuation(token),
            None => break,
        }
    }
    Ok(false)
}

/// The index of a drive that holds none: created, or - when another device created it in the
/// same moment - opened.
fn create_index<D: Drive + ?Sized + 'static, S: Sealer + Clone>(
    inner: &Arc<D>,
    sealer: &S,
    device_id: &str,
    device_name: &str,
    options: &RepoOptions,
    open: impl Fn() -> Result<MetaRepo<DriveBucket<Arc<D>>, S>, MetaError>,
) -> Result<MetaRepo<DriveBucket<Arc<D>>, S>, DriveError> {
    match MetaRepo::create_with(
        DriveBucket::new(Arc::clone(inner)),
        sealer.clone(),
        device_id,
        device_name,
        options,
    ) {
        Ok(repo) => Ok(repo),
        Err(MetaError::RepositoryExists) => open().map_err(to_drive),
        Err(e) => Err(to_drive(e)),
    }
}

/// An encrypted drive over `inner` whose index is the bucket's metadata
/// repository: opened (or created, for a bucket without one) with the drive
/// key, kept in `cache_dir` between runs, read lazily when `lazy`.
/// `device_id` names this device in the log, `device_name` in conflict copies.
pub fn open_encrypted_drive<D: Drive + 'static>(
    inner: Arc<D>,
    drive_key: DriveKey,
    device_id: &str,
    device_name: &str,
    cache_dir: Option<PathBuf>,
    lazy: bool,
) -> Result<EncryptedDrive<Arc<D>>, DriveError> {
    let options = RepoOptions {
        cache_dir,
        lazy,
        // gate:restore-none allow: a drive opened bare, for tools and tests, has no token server to ask [X-DELTA-AZDRIVE-SYNC-3]
        restore: RestoreSource::none("a bare drive has no token server to ask"),
    };
    let repo = open_or_create(&inner, &drive_key, device_id, device_name, &options)?;
    let index: Arc<dyn NameIndex> = Arc::new(MetaIndex::new(repo));
    Ok(EncryptedDrive::new(inner, drive_key, index))
}

/// How an app's device keeps the drives' indexes: ONE value with no `Default`, so that every field
/// is said where a provider is made (ARCH29 C6b, X-DELTA-AZDRIVE-SYNC-3). The provider used to be
/// `new(name)` and five optional builders (now gone), and the app that left `with_restore` out kept a
/// persistent copy and refused a restored drive for good (AzMail, the bridge and the command line
/// did; AzDrive alone was wired). A struct literal cannot leave `restore` or `reclaim` out.
#[derive(Clone, Debug)]
pub struct IndexProfile {
    /// Names this device in conflict copies ("report (conflict, <device>).docx") and in the
    /// commits of the drives' logs.
    pub device_name: String,
    /// Where the copies of the drives' indexes are kept between runs (`<root>/<hash of the drive's
    /// id>`); `None`: in memory, read anew on every start.
    pub cache_root: Option<PathBuf>,
    /// Whether the copies read their packs lazily (C6): a folder's objects arrive in ranged reads
    /// when it is browsed.
    pub lazy: bool,
    /// Whether the drives pack small files together ([`crate::packing`]), their journal in each
    /// drive's cache folder.
    pub packing: bool,
    /// Where a restore of the drives is announced: the token server's word as the app last read it
    /// ([`RestoreSource::new`]), or - for a drive that is not the token server's, a provider that
    /// opens no copy of an index, a test - [`RestoreSource::none`] with its reason. Without a source
    /// a restored drive that serves an older state than this device saw is a rollback, for good.
    pub restore: RestoreSource,
    /// How the reclaimer of the drives it maintains runs: [`ReclaimOptions::shadow`] (it reports
    /// what it would delete and deletes nothing) or [`ReclaimOptions::off`] (the kill switch).
    pub reclaim: ReclaimOptions,
}

impl IndexProfile {
    /// A provider for a test: in memory, eager, unpacked, no restore source and the reclaimer in
    /// shadow mode.
    #[cfg(any(test, feature = "testing"))]
    #[must_use]
    pub fn in_memory(device_name: &str) -> IndexProfile {
        IndexProfile {
            device_name: device_name.to_string(),
            cache_root: None,
            lazy: false,
            packing: false,
            // gate:restore-none allow: a test's provider has no token server to ask [X-DELTA-AZDRIVE-SYNC-3]
            restore: RestoreSource::none("a test's provider has no token server to ask"),
            reclaim: ReclaimOptions::shadow(),
        }
    }
}

/// The apps' [`IndexProvider`]: an encrypted drive's index is its bucket's
/// metadata repository, this device's copy of it kept in
/// `<cache root>/<drive>` (a hash of the drive's id) when there is a cache root.
pub struct MetaIndexProvider {
    device_name: String,
    cache_root: Option<PathBuf>,
    lazy: bool,
    /// Whether the drives it opens pack small files ([`crate::packing`]), their journal in
    /// each drive's cache folder.
    packing: bool,
    /// Where a restore of the drives it opens is announced (the token server's word).
    restore: RestoreSource,
    /// How the reclaimer of the drives it maintains runs: shadow by default.
    reclaim: ReclaimOptions,
    /// What the reclaimer round at the end of the last [`MetaIndexProvider::maintain`] came to.
    last_reclaim: Mutex<Option<Result<ReclaimReport, String>>>,
    /// When this process last ran a reclaimer round of each drive (seconds since 1970; the key of
    /// a round with no drive id is empty): a second limit beside the ledger's, for a bucket that
    /// cannot keep the ledger (a refused write, an unreadable object) - without it such a drive
    /// would be read whole at every maintenance round again.
    rounds_at: Mutex<BTreeMap<String, u64>>,
    /// What the last judged round of each drive came to (the key of a round with no drive id is
    /// empty): the standing the drives' pack upkeep asks [`ask_retired_delete`] with - the one gate
    /// stops every deleter, not only the reclaimer's own round (X-DELTA-SYNC2-2).
    standings: Arc<Mutex<BTreeMap<String, Standing>>>,
}

impl MetaIndexProvider {
    /// The provider `profile` describes: the one way to make one (the old `new(name)` and its five
    /// optional builders are gone; see [`IndexProfile`]).
    #[must_use]
    pub fn new(profile: IndexProfile) -> Self {
        MetaIndexProvider {
            device_name: profile.device_name,
            cache_root: profile.cache_root,
            lazy: profile.lazy,
            packing: profile.packing,
            restore: profile.restore,
            reclaim: profile.reclaim,
            last_reclaim: Mutex::new(None),
            rounds_at: Mutex::new(BTreeMap::new()),
            standings: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    /// Notes what the round of `drive` came to: its standing, for the pack upkeep and the next
    /// question of the gate.
    pub(crate) fn note_standing(&self, drive: Option<&str>, standing: Standing) {
        self.standings
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(drive.unwrap_or_default().to_string(), standing);
    }

    /// A round that proved nothing (it stopped before it judged anything): no standing - the
    /// upkeep then asks with none, and the gate says [`Hold::NotJudged`].
    pub(crate) fn forget_standing(&self, drive: Option<&str>) {
        self.standings
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(drive.unwrap_or_default());
    }

    /// What the reclaimer round at the end of the last [`MetaIndexProvider::maintain`] came to:
    /// the report (its [`ReclaimReport::line`] is what the log says), or why the round could not
    /// read the drive. `None` before any round.
    #[must_use]
    pub fn last_reclaim(&self) -> Option<Result<ReclaimReport, String>> {
        self.last_reclaim
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// One reclaimer round on the drive in `bucket` now, whatever the schedule
    /// ([`MetaIndexProvider::judged_round`]): on a copy of the repository of its own, read whole
    /// within the budget of a run ([`Caps::pack_bytes`]), so the device's open copies stay as
    /// they are. In shadow mode it deletes nothing; `Ok` with the report even when the round
    /// stopped on a decision (`stopped`), an error only when the drive could not be read.
    ///
    /// # Errors
    ///
    /// The drive's error.
    pub fn reclaim(
        &self,
        bucket: Arc<dyn Drive>,
        drive_key: &DriveKey,
    ) -> Result<ReclaimReport, DriveError> {
        if self.reclaim.mode == ReclaimMode::Off {
            return Ok(ReclaimReport::none(ReclaimMode::Off));
        }
        self.judged_round(None, bucket, drive_key)
    }

    /// Erases every old version of every file of the drive `drive` in `bucket` that the user
    /// confirmed: each data object the head does not name that a commit made at or before the
    /// confirmation named, whatever its age - the history before that moment cannot be restored
    /// afterwards; what a commit made after it first names stays (what the user never agreed to
    /// lose: [`purge_options`]). What the user asked for after changing the drive's keys (the old
    /// objects are the ones the old key still opens, and a re-encryption pass replaces them one by
    /// one in the hours after the click); it needs their confirmation ([`PurgeConfirmation`]) and
    /// is not switched off by the reclaimer's mode, nor held back by the evidence of the shadow
    /// rounds ([`go`]): the user said so. The bucket's clock must still agree with this device's,
    /// and the floor of this device's copy of the drive refuses a bucket that serves an older
    /// manifest than the copy has seen, as the maintenance round does (a rolled-back head would make
    /// an object a later commit names again a candidate, and the purge deletes candidates). A device
    /// that has no copy of the drive knows no floor: the purge is run by the one that re-encrypted.
    ///
    /// # Errors
    ///
    /// The drive's error; [`DriveError::Protocol`] for a bucket that serves an older index than this
    /// device's copy has seen.
    pub fn purge(
        &self,
        drive: &str,
        bucket: Arc<dyn Drive>,
        drive_key: &DriveKey,
        confirmation: &PurgeConfirmation,
    ) -> Result<ReclaimReport, DriveError> {
        let options = purge_options(confirmation);
        let device = self.device_id();
        let floor = self
            .drive_cache_dir(drive)
            .map_or(0, |dir| super::repo::cached_floor(&dir, drive_key));
        match MetaStore::open_above(
            DriveBucket::new(Arc::clone(&bucket)),
            drive_key.clone(),
            &device,
            floor,
        ) {
            Ok(_) => {}
            Err(MetaError::NoRepository) => return Ok(ReclaimReport::none(options.mode)),
            Err(e) => return Err(to_drive(e)),
        }
        let mut repo = match MetaRepo::open_with(
            DriveBucket::new(bucket),
            drive_key.clone(),
            &device,
            "Maintenance",
            &RepoOptions {
                restore: self.restore.clone(),
                ..RepoOptions::default()
            },
        ) {
            Ok(repo) => repo,
            Err(MetaError::NoRepository) => return Ok(ReclaimReport::none(options.mode)),
            Err(e) => return Err(to_drive(e)),
        };
        // The user confirmed: the shadow rounds' evidence is not asked (see `purge`'s doc).
        reclaim(
            &mut repo,
            &options,
            Ok(DeleteLicence::confirmed(confirmation)),
        )
        .map_err(to_drive)
    }

    /// The automatic round after a maintenance round of the drive `drive`: when one is due - the
    /// ledger of the drive's shadow rounds ([`Ledger`]) shows none in the last day, from any
    /// device - [`MetaIndexProvider::judged_round`]; `None` when it is not.
    fn due_round(
        &self,
        drive: Option<&str>,
        bucket: &Arc<dyn Drive>,
        drive_key: &DriveKey,
    ) -> Result<Option<ReclaimReport>, DriveError> {
        let now = crate::time::now_unix();
        let id = drive.unwrap_or_default().to_string();
        let ran = self
            .rounds_at
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&id)
            .copied();
        // This process ran one lately (a stamp ahead of the clock is stale, not fresh).
        if ran.is_some_and(|at| at <= now && now - at < super::shadow::EVERY_SECS) {
            return Ok(None);
        }
        let (ledger, _) =
            Ledger::read(&DriveBucket::new(Arc::clone(bucket)), drive_key).map_err(to_drive)?;
        if !ledger.due(now) {
            return Ok(None);
        }
        self.rounds_at
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(id, now);
        self.judged_round(drive, Arc::clone(bucket), drive_key)
            .map(Some)
    }

    /// One round of the reclaimer over the drive `drive` (none: no copy of this device's is known):
    ///
    /// 1. the size first: the manifest says the packs' bytes before one is fetched, and a drive over
    ///    the budget is "too big to judge" - recorded as a round that was not clean, nothing read
    ///    (the maintenance round used to read the whole history of every drive into memory on every
    ///    computer every six hours); the floor of this device's copy refuses a rolled-back bucket
    ///    here as it does for the maintenance round before;
    /// 2. the copy of its own, with this device's cache for it (only what is new is fetched next
    ///    time) and the app's restore source;
    /// 3. the head listed by the app's own listing, an independent derivation of what it names;
    /// 4. the analysis ([`reclaim_judged`]), and the round written into the ledger with the digest of
    ///    its candidates and whether the live set agreed with the listing;
    /// 5. what the ledger then shows decides the mode ([`go`]): a run configured to delete deletes
    ///    only when the evidence is there.
    fn judged_round(
        &self,
        drive: Option<&str>,
        bucket: Arc<dyn Drive>,
        drive_key: &DriveKey,
    ) -> Result<ReclaimReport, DriveError> {
        let mode = self.reclaim.mode;
        let caps = self.reclaim.caps;
        let device = self.device_id();
        let now = crate::time::now_unix();
        // The stamp of the round and the window the analysis cuts are made at this time of day; the
        // bucket vouches for it at the write, carried forward by the monotonic clock (SYNC4-4).
        let started = crate::time::Stopwatch::start();
        let dbucket = DriveBucket::new(Arc::clone(&bucket));
        let (before, _) = Ledger::read(&dbucket, drive_key).map_err(to_drive)?;
        let cache = drive.and_then(|drive| self.drive_cache_dir(drive));
        let floor = cache
            .as_deref()
            .map_or(0, |dir| super::repo::cached_floor(dir, drive_key));
        let footing = |drive_devices: u32,
                       rederived: Option<bool>,
                       window: Option<bool>,
                       byte_budget_ok: bool| Footing {
            drive_devices,
            rederived_live_equal: rederived,
            window_rederived: window,
            // Asked of the bucket when the round is written ([`MetaIndexProvider::record_round`]).
            clock_agrees: None,
            byte_budget_ok,
            floor_known: floor > 0,
            restore_source: self.restore.is_set(),
        };

        let sized = match MetaStore::open_above(
            DriveBucket::new(Arc::clone(&bucket)),
            drive_key.clone(),
            &device,
            floor,
        ) {
            Ok(store) => store,
            Err(MetaError::NoRepository) => return Ok(ReclaimReport::none(mode)),
            Err(e) => return Err(to_drive(e)),
        };
        let bytes = sized
            .manifest()
            .map_or(0, |manifest| manifest_pack_bytes(&manifest.packs));
        if bytes > caps.pack_bytes {
            let why = NotYet::TooBig {
                bytes,
                budget: caps.pack_bytes,
            };
            let mut report = ReclaimReport::too_big(mode, bytes, caps.pack_bytes);
            let round = Round {
                at: now,
                device,
                revision: sized.state().revision,
                digest: String::new(),
                candidates: 0,
                candidate_bytes: 0,
                live: 0,
                rules: ANALYSIS_RULES,
                clean: false,
                unclean: why.to_string(),
            };
            let evidence = self.record_round(
                &dbucket,
                drive_key,
                &before,
                round,
                now,
                started,
                footing(1, None, None, false),
            );
            report.evidence = Some(evidence);
            report.held = (mode == ReclaimMode::Delete).then_some(Hold::TooBig);
            // The standing of a drive too big to judge: its evidence says so, and the gate holds.
            self.note_standing(drive, Standing { at: now, evidence });
            return Ok(report);
        }
        drop(sized);

        let options = RepoOptions {
            cache_dir: cache.map(|dir| dir.join(RECLAIM_COPY)),
            lazy: false,
            restore: self.restore.clone(),
        };
        let repo = match MetaRepo::open_with(
            DriveBucket::new(Arc::clone(&bucket)),
            drive_key.clone(),
            &device,
            "Maintenance",
            &options,
        ) {
            Ok(repo) => repo,
            Err(MetaError::NoRepository) => return Ok(ReclaimReport::none(mode)),
            Err(e) => return Err(to_drive(e)),
        };
        let index = MetaIndex::new(repo).with_poll_every(u64::MAX);
        // collapse-ok: a head that cannot be listed is a round that was not checked against it, and
        // such a round is not clean (`rederived` is `None`): the failure holds the switch back.
        let listed = listed_objects(&index).ok();

        let mut judged = false;
        let mut report = index
            .with_repo(|repo| {
                reclaim_judged(repo, &self.reclaim, &mut |analysis, report| {
                    judged = true;
                    let rederived = rederive(listed.as_ref(), analysis);
                    report.rederived = rederived;
                    // The head listed again AND what the restore dialog still offers derived its
                    // own way: the first can fail only on a defect of the tree walk, the second on
                    // a window that is not the one the user was promised (X-DELTA-SYNC2-2).
                    let window = analysis.window_agrees;
                    let clean = rederived == Some(true) && window == Some(true);
                    let round = Round {
                        at: now,
                        device: device.clone(),
                        revision: analysis.revision,
                        digest: report.digest.clone(),
                        candidates: u64::try_from(report.candidates).unwrap_or(u64::MAX),
                        candidate_bytes: report.candidate_bytes,
                        live: u64::try_from(report.live_objects).unwrap_or(u64::MAX),
                        rules: ANALYSIS_RULES,
                        clean,
                        unclean: if clean {
                            String::new()
                        } else {
                            unclean_why(rederived, window)
                        },
                    };
                    let evidence = self.record_round(
                        &dbucket,
                        drive_key,
                        &before,
                        round,
                        now,
                        started,
                        footing(
                            u32::try_from(analysis.devices).unwrap_or(u32::MAX),
                            rederived,
                            window,
                            true,
                        ),
                    );
                    report.evidence = Some(evidence);
                    // The round's standing is what the pack upkeep asks the same gate with; the
                    // run asks it now, with the standing it has just made.
                    let standing = Standing { at: now, evidence };
                    self.note_standing(drive, standing);
                    let gate = ask_retired_delete(
                        History::Kept {
                            mode,
                            standing: Some(&standing),
                        },
                        now,
                    );
                    if let (Err(hold), ReclaimMode::Delete) = (&gate, mode) {
                        report.held = Some(*hold);
                    }
                    gate
                })
            })
            .map_err(to_drive)?;
        if !judged {
            // Nothing was proven: the pack upkeep waits for a round that is.
            self.forget_standing(drive);
            // The run stopped before there was anything to judge: a tree or a pointer that does not
            // read is a round that was not clean, and counts against the streak; a drive with no
            // commit yet has nothing to record.
            if let Some(why) = report.stopped.as_ref().map(ToString::to_string) {
                let round = Round {
                    at: now,
                    device,
                    revision: 0,
                    digest: String::new(),
                    candidates: 0,
                    candidate_bytes: 0,
                    live: 0,
                    rules: ANALYSIS_RULES,
                    clean: false,
                    unclean: why,
                };
                let evidence = self.record_round(
                    &dbucket,
                    drive_key,
                    &before,
                    round,
                    now,
                    started,
                    footing(1, None, None, true),
                );
                report.evidence = Some(evidence);
                let standing = Standing { at: now, evidence };
                report.held = (mode == ReclaimMode::Delete).then(|| {
                    match ask_retired_delete(
                        History::Kept {
                            mode,
                            standing: Some(&standing),
                        },
                        now,
                    ) {
                        Err(hold) => hold,
                        Ok(_) => Hold::NotRederived,
                    }
                });
            }
        }
        Ok(report)
    }

    /// Writes `round` into the drive's ledger and returns what the record shows with it. A round
    /// that could not be recorded - the bucket did not answer, another device wrote three times
    /// first - counts as one that was not clean, so the evidence of a run never rests on what is not
    /// written down.
    fn record_round(
        &self,
        bucket: &DriveBucket<Arc<dyn Drive>>,
        drive_key: &DriveKey,
        before: &Ledger,
        mut round: Round,
        now: u64,
        started: crate::time::Stopwatch,
        mut footing: Footing,
    ) -> GoEvidence {
        // The stamp of a round is this device's clock. When the bucket must vouch for it (the run's
        // clock policy), a round the analysis called clean is written as NOT clean first and made
        // clean only after the bucket's own time has agreed with the stamp: a stop in between, a lost
        // write, a bucket that does not say the time or a device whose clock is off leave a round
        // that does not count - a past-dated clean round used to stretch the days (X-DELTA-SYNC2-2).
        let vouching = round.clean && self.reclaim.server_clock == ServerClock::Required;
        let mut written = round.clone();
        if vouching {
            written.clean = false;
            written.unclean = String::from(UNVERIFIED);
        }
        let mut ledger = match Ledger::append(bucket, drive_key, &written) {
            Ok(Some(ledger)) => ledger,
            Ok(None) | Err(_) => {
                round.clean = false;
                round.unclean = String::from("it could not be recorded");
                let mut ledger = before.clone();
                ledger.record(round);
                // Nothing was written: no time was vouched for.
                return ledger.evidence(now, &footing);
            }
        };
        footing.clock_agrees = self.bucket_clock_agrees(bucket, now, started);
        if vouching && footing.clock_agrees == Some(true) {
            // collapse-ok: a round that could not be made clean stays what it was written as, not
            // clean: the failure only holds the switch back.
            if let Ok(Some(confirmed)) = Ledger::confirm(bucket, drive_key, &written) {
                ledger = confirmed;
            }
        }
        ledger.evidence(now, &footing)
    }

    /// Whether the bucket's own time agrees with the time of day the round's stamp and window were
    /// made at, when the round has just been written (the ledger's object carries the bucket's time
    /// of that write): `Some(true)` when they are within [`SKEW_SECS`] - or when the run's policy takes
    /// the device's clock alone ([`ServerClock::Unchecked`]: a folder on disk, the tests) -,
    /// `Some(false)` when they are not, `None` when the bucket does not say. The stamp carried forward
    /// by the monotonic clock ([`stamp_agrees`]): a big drive's analysis takes longer than the skew,
    /// and the device's clock of the moment of the write is not the one the window was cut by.
    fn bucket_clock_agrees(
        &self,
        bucket: &DriveBucket<Arc<dyn Drive>>,
        stamp: u64,
        started: crate::time::Stopwatch,
    ) -> Option<bool> {
        match self.reclaim.server_clock {
            ServerClock::Unchecked => Some(true),
            ServerClock::Required => match bucket.modified_of(keys::SHADOW) {
                Ok(Some(server)) => Some(stamp_agrees(server, stamp, started.secs())),
                // collapse-ok: a bucket that cannot say the time is not believed: `None` holds the
                // switch back, as it does for the run that deletes.
                Ok(None) | Err(_) => None,
            },
        }
    }

    /// The index over `repo`, with what the upkeep of the drive may ask of its history: the whole of
    /// it within the budget of a run when the provider's reclaimer may delete, the head alone when
    /// it deletes nothing (shadow mode, the first release's).
    fn index_of<B: Bucket, S: Sealer>(&self, repo: MetaRepo<B, S>) -> MetaIndex<B, S> {
        MetaIndex::new(repo).with_history_budget(
            (self.reclaim.mode == ReclaimMode::Delete).then_some(self.reclaim.caps.pack_bytes),
        )
    }

    /// How this device keeps its copy of the drive `drive`.
    fn options(&self, drive: &str) -> RepoOptions {
        RepoOptions {
            cache_dir: self.drive_cache_dir(drive),
            lazy: self.lazy,
            restore: self.restore.clone(),
        }
    }

    /// The folder of this device's copy of the drive `drive` (named by a hash of its id): the
    /// drive's own cache folder - an app keeps what it derives from the drive's plain contents
    /// there (a search index), and it goes with the copy when the drive's key is rotated.
    #[must_use]
    pub fn drive_cache_dir(&self, drive: &str) -> Option<PathBuf> {
        MetaIndexProvider::drive_cache_dir_in(self.cache_root.as_deref(), drive)
    }

    /// [`MetaIndexProvider::drive_cache_dir`] for a caller that only needs the path (the removal of
    /// a drive's copies, a search index beside the copy) and has no provider: the folder of the
    /// drive `drive` under the cache root `root`; `None` without a root.
    #[must_use]
    pub fn drive_cache_dir_in(root: Option<&Path>, drive: &str) -> Option<PathBuf> {
        root.map(|root| root.join(hex(&Sha256::digest(drive.as_bytes())[..16])))
    }

    /// This device's id in the drives' logs and leases: kept in `<cache
    /// root>/device-id` (made once); a new one per run without a cache root.
    fn device_id(&self) -> String {
        let Some(root) = &self.cache_root else {
            return crate::ids::new_uuid();
        };
        let file = root.join("device-id");
        if let Ok(text) = std::fs::read_to_string(&file) {
            let id = text.trim();
            if crate::ids::is_uuid(id) {
                return id.to_string();
            }
        }
        let id = crate::ids::new_uuid();
        let _ = std::fs::create_dir_all(root);
        let _ = crate::local::write_atomically(&file, id.as_bytes());
        id
    }

    /// One maintenance round of the index in `bucket` ([`MetaStore::maintain`]). The round
    /// runs under the bucket's maintenance lease and does four things: it sweeps orphans,
    /// folds the live packs into packs of at most [`Maintenance::compact_max_bytes`], writes a
    /// checkpoint, and deletes what was retired long enough ago. It is meant for an app's idle
    /// time, because compaction reads every live pack it folds. It answers `None` when the bucket has no index yet or another device holds the
    /// lease. It opens the store on its own, so this device's copies stay as they are, and
    /// their next pull reads the new pack.
    pub fn maintain(
        &self,
        bucket: Arc<dyn Drive>,
        drive_key: &DriveKey,
        rules: &Maintenance,
    ) -> Result<Option<Maintained>, DriveError> {
        self.maintain_above(None, 0, bucket, drive_key, rules)
    }

    /// [`MetaIndexProvider::maintain`] for the drive `drive`: the round refuses a manifest older
    /// than the highest one this computer's copy of the drive's index has seen
    /// ([`MetaError::Rollback`], as for the copy), where `maintain` - which does not know the
    /// copy - takes whatever the bucket serves. The copy takes an operator's restore of the drive
    /// (announced by the token server) at its next poll; a round that met the restored drive
    /// before that stops, and the next one (hours later) runs.
    pub fn maintain_drive(
        &self,
        drive: &str,
        bucket: Arc<dyn Drive>,
        drive_key: &DriveKey,
        rules: &Maintenance,
    ) -> Result<Option<Maintained>, DriveError> {
        let floor = self
            .drive_cache_dir(drive)
            .map_or(0, |dir| super::repo::cached_floor(&dir, drive_key));
        self.maintain_above(Some(drive), floor, bucket, drive_key, rules)
    }

    fn maintain_above(
        &self,
        drive: Option<&str>,
        floor: u64,
        bucket: Arc<dyn Drive>,
        drive_key: &DriveKey,
        rules: &Maintenance,
    ) -> Result<Option<Maintained>, DriveError> {
        let raw = Arc::clone(&bucket);
        let bucket = DriveBucket::new(bucket);
        let mut store =
            match MetaStore::open_above(bucket, drive_key.clone(), &self.device_id(), floor) {
                Ok(store) => store,
                Err(MetaError::NoRepository) => return Ok(None),
                Err(e) => return Err(to_drive(e)),
            };
        let done = store.maintain(rules).map_err(to_drive)?;
        // After the round the index is in its smallest shape: the reclaimer looks at the data
        // objects its history released - when a round is due (one a day per drive, not one per
        // maintenance round of every device), within the budget of a run. A round that cannot read
        // the drive does not fail the maintenance, and is told in `last_reclaim`; one that was not
        // due leaves nothing there.
        let outcome = if done.is_some() && self.reclaim.mode != ReclaimMode::Off {
            self.due_round(drive, &raw, drive_key)
                .map_err(|e| e.to_string())
                .transpose()
        } else {
            None
        };
        *self
            .last_reclaim
            .lock()
            .unwrap_or_else(PoisonError::into_inner) = outcome;
        Ok(done)
    }
}

/// The folder of the reclaimer's copy of a drive's index below the drive's cache folder: a copy of
/// its own, so the open copy of the app is never made a full one for the reclaimer's sake.
const RECLAIM_COPY: &str = "reclaim";

/// What the app's own listing says the head of `index` names, and the revision of the manifest it
/// read it at: the derivation the reclaimer's walk is checked against (it is the code every read
/// of the drive goes through every day, not the reclaimer's).
pub(crate) fn listed_objects<B: Bucket, S: Sealer>(
    index: &MetaIndex<B, S>,
) -> Result<(HashSet<DataId>, u64), DriveError> {
    let mut named = HashSet::new();
    let mut request = ListRequest::recursive("");
    loop {
        let page = index.list(&request)?;
        for (_, entry) in &page.entries {
            if let Some(object) = entry.bucket_object() {
                named.insert(object);
            }
        }
        match page.next {
            Some(token) => request = request.with_continuation(token),
            None => break,
        }
    }
    let revision = index.with_repo(|repo| repo.store().state().revision);
    Ok((named, revision))
}

/// Whether the live set of `analysis` holds every object the listing named: `Some(true)` it does,
/// `Some(false)` an object the head names is missing from it (deleting by this analysis would lose
/// a file), `None` there was no listing, or it was made at another revision than the analysis. The
/// live set may hold more (the window, the drive's own bookkeeping): that is only more care.
pub(crate) fn rederive(
    listed: Option<&(HashSet<DataId>, u64)>,
    analysis: &Analysis,
) -> Option<bool> {
    let (named, revision) = listed?;
    if *revision != analysis.revision {
        return None;
    }
    Some(named.iter().all(|id| analysis.live.contains_key(id)))
}

/// Why a round whose re-derivations were `rederived` (the head listed again) and `window` (what the
/// restore dialog still offers) was not clean: the first thing that did not agree.
fn unclean_why(rederived: Option<bool>, window: Option<bool>) -> String {
    match (rederived, window) {
        (Some(true), Some(true)) => String::new(),
        (Some(false), _) => NotYet::Unreadable {
            what: String::from("the live set"),
            reason: String::from("the head's listing names an object it lacks"),
        }
        .to_string(),
        (None, _) => String::from("the head was not listed at the revision the analysis read"),
        (Some(true), Some(false)) => NotYet::Unreadable {
            what: String::from("the live set"),
            reason: String::from(
                "the restore dialog still offers a version the window of the run does not name",
            ),
        }
        .to_string(),
        (Some(true), None) => {
            String::from("what the restore dialog still offers could not be derived again")
        }
    }
}

impl IndexProvider for MetaIndexProvider {
    fn reclaim_mode(&self, _drive: &str) -> ReclaimMode {
        self.reclaim.mode
    }

    /// What the last round this provider judged for `drive` came to: the standing the pack upkeep of
    /// the drive asks [`ask_retired_delete`] with - the same gate as the reclaimer's own run, with
    /// the evidence the round recorded. No round, and the upkeep has none to ask with (X-DELTA-SYNC2-2).
    fn reclaim_standing(&self, drive: &str) -> Option<crate::encrypted::ReclaimStanding> {
        let standings = Arc::clone(&self.standings);
        let drive = drive.to_string();
        Some(Arc::new(move |_now| {
            standings
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .get(&drive)
                .copied()
        }))
    }

    fn purge_confirmed(
        &self,
        drive: &str,
        bucket: Arc<dyn Drive>,
        drive_key: &DriveKey,
        confirmation: &PurgeConfirmation,
    ) -> Result<ReclaimReport, DriveError> {
        self.purge(drive, bucket, drive_key, confirmation)
    }

    fn packing(&self, drive: &str) -> Option<crate::packing::PackOptions> {
        if !self.packing {
            return None;
        }
        let options = crate::packing::PackOptions::default();
        Some(match self.drive_cache_dir(drive) {
            Some(dir) => options.with_journal(dir.join("packs")),
            None => options,
        })
    }

    fn open_index(
        &self,
        drive: &str,
        bucket: Arc<dyn Drive>,
        drive_key: &DriveKey,
    ) -> Result<Arc<dyn NameIndex>, DriveError> {
        let options = self.options(drive);
        let repo = open_or_create(
            &bucket,
            drive_key,
            &self.device_id(),
            &self.device_name,
            &options,
        )?;
        Ok(Arc::new(self.index_of(repo)))
    }

    /// This device's copy, sealed before the rotation under `previous`, opens through the
    /// rotation's window ([`KeyWindow`]), offline too. From then on the copy is sealed under
    /// `drive_key`: its state when it is saved again, its packs as the drive's new ones
    /// arrive (a re-sealed repository has new pack names, so the old ones leave the copy).
    fn open_index_in_window(
        &self,
        drive: &str,
        bucket: Arc<dyn Drive>,
        drive_key: &DriveKey,
        previous: &DriveKey,
    ) -> Result<Arc<dyn NameIndex>, DriveError> {
        let window = KeyWindow::new(drive_key.clone(), Some(previous.clone()));
        let options = self.options(drive);
        let repo = open_or_create(
            &bucket,
            &window,
            &self.device_id(),
            &self.device_name,
            &options,
        )?;
        Ok(Arc::new(self.index_of(repo)))
    }

    /// The repository re-sealed under `new` ([`reseal`]: crash-safe, run again after a stop
    /// and it finishes), then this device's copy - sealed with `old` - deleted; the next open
    /// reads the repository anew.
    fn rekey(
        &self,
        drive: &str,
        bucket: Arc<dyn Drive>,
        old: &DriveKey,
        new: &DriveKey,
    ) -> Result<(), DriveError> {
        let bucket = DriveBucket::new(bucket);
        reseal(&bucket, old, new, &self.device_id(), crate::time::now_unix()).map_err(to_drive)?;
        if let Some(dir) = self.drive_cache_dir(drive) {
            match std::fs::remove_dir_all(&dir) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(DriveError::Io(format!("{}: {e}", dir.display()))),
            }
        }
        Ok(())
    }
}
