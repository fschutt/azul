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
//!   device is checked and applied again on the newer state. The index keeps the history, so it
//!   releases no object (the old versions stay restorable).
//! - `.azlin` at the root (the drive's policy and keys) and the shards' names are no drive
//!   paths: hidden from reads, refused for changes.

use std::{
    cmp::Reverse,
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use super::{
    bucket::{Bucket, DriveBucket},
    hex,
    objects::{Commit, Mode, ObjectId},
    pack::PackWriter,
    pointer,
    repo::{remote_head, unpublished, Local, MetaRepo, RepoOptions, MAIN},
    seal::Sealer,
    shard::{self, SHARD_PREFIX},
    tree::{apply, entry_at, folder_at, Change},
    wal::{Packs, Publish, RefUpdate},
    MetaError,
};
use crate::{
    crypto::{DriveKey, ObjectId as DataId},
    encrypted::{EncryptedDrive, IndexChange, IndexEntry, IndexPage, IndexProvider, NameIndex},
    key::folder_of,
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

/// Whether `path` (a key or a folder prefix) is the drive's own: `.azlin` at
/// the root, or a segment of a shard's name.
fn reserved(path: &str) -> bool {
    let mut segments = path.trim_end_matches('/').split('/');
    segments.next() == Some(".azlin")
        || path.split('/').any(|segment| segment.starts_with(SHARD_PREFIX))
}

/// The drive's error for an index error.
fn to_drive(e: MetaError) -> DriveError {
    match e {
        MetaError::Conflict { key } => DriveError::Conflict { key },
        MetaError::Drive(e) => e,
        MetaError::Corrupt { key, reason } => DriveError::Corrupt { key, reason },
        MetaError::Sealed { key, reason } => DriveError::Corrupt {
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
            pointer::decode(blob)
                .map(Some)
                .map_err(|reason| MetaError::Corrupt {
                    key: path.to_string(),
                    reason,
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
    /// is (see the module docs); `Conflict` when an expectation does not hold.
    pub fn apply_index(&mut self, changes: &[IndexChange]) -> Result<(), MetaError> {
        for change in changes {
            if reserved(change_path(change)) {
                return Err(MetaError::Drive(DriveError::InvalidKey {
                    key: change_path(change).to_string(),
                    reason: "it is the drive's own bookkeeping (.azlin)",
                }));
            }
        }
        let signature = self.signature();
        let local = &mut self.local;
        let mut outcome: Option<(ObjectId, Vec<ObjectId>)> = None;
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
            outcome = Some((commit, new_objects));
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
        match (result, outcome) {
            (Some(published), Some((head, packed))) => {
                self.landed(Some(published), &packed)?;
                self.head = Some(head);
            }
            _ => {
                // Nothing to change: the copy is at the drive's head anyway.
                self.head = remote_head(self.store.state())?;
            }
        }
        self.save();
        Ok(())
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
}

/// The drive index as a [`NameIndex`]: an [`EncryptedDrive`] over it keeps its
/// names, folders, sizes and dates in the bucket's metadata repository and
/// browses without listing the bucket.
pub struct MetaIndex<B: Bucket, S: Sealer> {
    inner: Mutex<Inner<B, S>>,
    poll_every: u64,
}

impl<B: Bucket, S: Sealer> MetaIndex<B, S> {
    /// The index of `repo` (opened, pulled).
    #[must_use]
    pub fn new(repo: MetaRepo<B, S>) -> Self {
        MetaIndex {
            inner: Mutex::new(Inner {
                repo,
                last_poll: None,
            }),
            poll_every: DEFAULT_POLL_SECS,
        }
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

    /// Runs `f` with the device's copy (to commit, to restore, for maintenance).
    pub fn with_repo<T>(&self, f: impl FnOnce(&mut MetaRepo<B, S>) -> T) -> T {
        f(&mut self.lock().repo)
    }

    /// The copy, polled when the last poll is older than the poll time.
    fn fresh(&self) -> Result<MutexGuard<'_, Inner<B, S>>, DriveError> {
        let mut inner = self.lock();
        let now = inner.repo.store().now();
        let due = inner
            .last_poll
            .map_or(true, |last| now.saturating_sub(last) >= self.poll_every);
        if due {
            inner.repo.pull().map_err(to_drive)?;
            inner.last_poll = Some(now);
        }
        Ok(inner)
    }
}

impl<B: Bucket, S: Sealer> NameIndex for MetaIndex<B, S> {
    fn get(&self, path: &str) -> Result<Option<IndexEntry>, DriveError> {
        if path.is_empty() || reserved(path) {
            return Ok(None);
        }
        let mut inner = self.fresh()?;
        let repo = &mut inner.repo;
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

    fn list(&self, request: &ListRequest) -> Result<IndexPage, DriveError> {
        let prefix = request.prefix.as_str();
        if reserved(prefix) {
            return Ok(IndexPage::default());
        }
        let mut inner = self.fresh()?;
        let repo = &mut inner.repo;
        let Some(root) = repo.root().map_err(to_drive)? else {
            return Ok(IndexPage::default());
        };
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
                if dir.is_empty() && entry.name == ".azlin" {
                    continue;
                }
                let key = format!("{dir}{}", entry.name);
                match entry.mode {
                    Mode::File => {
                        if key.starts_with(prefix) {
                            let blob = repo.objects().blob(&entry.id).map_err(to_drive)?;
                            let pointer = pointer::decode(blob).map_err(|reason| {
                                DriveError::Corrupt {
                                    key: key.clone(),
                                    reason,
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
        Ok(page_of(found, request))
    }

    fn apply(&self, changes: Vec<IndexChange>) -> Result<Vec<DataId>, DriveError> {
        let mut inner = self.lock();
        inner.repo.apply_index(&changes).map_err(to_drive)?;
        let now = inner.repo.store().now();
        inner.last_poll = Some(now);
        Ok(Vec::new())
    }
}

/// One page of `found` for `request`, with S3's semantics as the memory index
/// has them: keys below the next delimiter after the prefix collapse into one
/// folder, sorted, after the continuation, `max_keys` at most.
fn page_of(found: Vec<(String, IndexEntry)>, request: &ListRequest) -> IndexPage {
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
    if let Some(after) = request.continuation.as_deref() {
        keys.retain(|(key, _)| key.as_str() > after);
    }
    let size = if request.max_keys == 0 {
        DEFAULT_PAGE_SIZE as usize
    } else {
        request.max_keys as usize
    };
    let next = (keys.len() > size).then(|| keys[size - 1].0.clone());
    keys.truncate(size);
    let mut page = IndexPage {
        next,
        ..IndexPage::default()
    };
    for (key, entry) in keys {
        match entry {
            Some(entry) => page.entries.push((key, entry)),
            None => page.folders.push(key),
        }
    }
    page
}

/// The drive index of the bucket `inner`: opened, or created for a bucket
/// without one (and opened when another device created it in the same moment).
fn open_or_create<D: Drive + ?Sized + 'static>(
    inner: &Arc<D>,
    drive_key: &DriveKey,
    device_id: &str,
    device_name: &str,
    options: &RepoOptions,
) -> Result<MetaRepo<DriveBucket<Arc<D>>, DriveKey>, DriveError> {
    let open = || {
        MetaRepo::open_with(
            DriveBucket::new(Arc::clone(inner)),
            drive_key.clone(),
            device_id,
            device_name,
            options,
        )
    };
    match open() {
        Ok(repo) => Ok(repo),
        Err(MetaError::NoRepository) => match MetaRepo::create_with(
            DriveBucket::new(Arc::clone(inner)),
            drive_key.clone(),
            device_id,
            device_name,
            options,
        ) {
            Ok(repo) => Ok(repo),
            Err(MetaError::RepositoryExists) => open().map_err(to_drive),
            Err(e) => Err(to_drive(e)),
        },
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
    let options = RepoOptions { cache_dir, lazy };
    let repo = open_or_create(&inner, &drive_key, device_id, device_name, &options)?;
    let index: Arc<dyn NameIndex> = Arc::new(MetaIndex::new(repo));
    Ok(EncryptedDrive::new(inner, drive_key, index))
}

/// The apps' [`IndexProvider`]: an encrypted drive's index is its bucket's
/// metadata repository, this device's copy of it kept in
/// `<cache root>/<drive>` (a hash of the drive's id) when there is a cache root.
pub struct MetaIndexProvider {
    device_name: String,
    cache_root: Option<PathBuf>,
    lazy: bool,
}

impl MetaIndexProvider {
    /// `device_name` names this device in conflict copies ("report (conflict,
    /// <device>).docx") and commits.
    #[must_use]
    pub fn new(device_name: &str) -> Self {
        MetaIndexProvider {
            device_name: device_name.to_string(),
            cache_root: None,
            lazy: false,
        }
    }

    /// Keeps the drives' copies under `root` between runs (`None`: in memory).
    #[must_use]
    pub fn with_cache_root(mut self, root: Option<PathBuf>) -> Self {
        self.cache_root = root;
        self
    }

    /// Reads the drives' packs lazily (C6).
    #[must_use]
    pub fn with_lazy(mut self, lazy: bool) -> Self {
        self.lazy = lazy;
        self
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
}

impl IndexProvider for MetaIndexProvider {
    fn open_index(
        &self,
        drive: &str,
        bucket: Arc<dyn Drive>,
        drive_key: &DriveKey,
    ) -> Result<Arc<dyn NameIndex>, DriveError> {
        let options = RepoOptions {
            cache_dir: self
                .cache_root
                .as_ref()
                .map(|root| root.join(hex(&Sha256::digest(drive.as_bytes())[..16]))),
            lazy: self.lazy,
        };
        let repo = open_or_create(&bucket, drive_key, &self.device_id(), &self.device_name, &options)?;
        Ok(Arc::new(MetaIndex::new(repo)))
    }
}
