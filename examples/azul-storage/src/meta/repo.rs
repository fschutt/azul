//! A device's copy of the drive index: the objects of the repository's packs,
//! this device's head commit, and the store it syncs with.
//!
//! - [`MetaRepo::pull`]: poll; read the packs that are new; move to the drive's head.
//! - [`MetaRepo::commit`]: apply changes to the head's tree, commit, publish. When another
//!   device published in the meantime, the publish merges its state in (three-way, against
//!   the merge base) and publishes the merge; conflicts go to the caller's resolver.
//! - [`MetaRepo::file_changed_since`]: the check before an upload (D52): did the file move
//!   on the drive since the version this device's copy is based on?
//! - [`MetaRepo::restore`]: puts a file back as it was in an older commit.
//!
//! The drive's history is the branch `refs/heads/main`. A merge commit's first
//! parent is the state the other devices published, the second this device's.
//!
//! # Keeping the copy ([`RepoOptions`])
//!
//! - **On disk** (`cache_dir`): the packs and their indexes exactly as the bucket holds them
//!   (sealed: the cache is as encrypted as the bucket), the chunks a lazy copy read, and the
//!   state - the manifest seen, its version, the highest revision seen, the head - sealed with
//!   the drive key. A copy opened from it polls with one conditional read and downloads only
//!   what is new; a damaged or foreign cache is ignored and the copy is read anew.
//! - **Lazily** (`lazy`, C6): a pull reads the indexes only; an object is read when it is
//!   needed, with one ranged GET of the ~64 KiB chunk that holds it (and its neighbours). A
//!   new device browses before the packs are downloaded. A commit (which may merge) reads
//!   every pack whole first.

use std::{
    collections::{BTreeSet, HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use super::{
    bucket::Bucket,
    merge::{merge_base, merge_trees, Conflict, Resolution, Resolved},
    objects::{Commit, Kind, Mode, ObjectId, Objects, Signature},
    pack::{PackIndex, PackWriter},
    seal::Sealer,
    shard,
    tree::{apply, entry_at, segments, Change},
    wal::{MetaStore, PackRef, Packs, Publish, Published, RefUpdate, RepoState, StoreSnapshot},
    MetaError,
};
use crate::local::write_atomically;

/// The branch that is the drive's history.
pub const MAIN: &str = "refs/heads/main";

/// The context the cached state is sealed with (it is no bucket object).
const STATE_CONTEXT: &[u8] = b"azul-storage meta: the cached state of a device";

/// What a commit became.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitOutcome {
    /// The drive's new head: this device's commit, or the merge of it.
    pub head: ObjectId,
    pub published: Published,
    /// The conflicts the merge met, with what became of them.
    pub conflicts: Vec<Resolved>,
}

/// How a device keeps its copy.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepoOptions {
    /// A folder for the copy between runs; `None`: in memory only.
    pub cache_dir: Option<PathBuf>,
    /// Read packs chunk by chunk when objects are needed, not whole on a pull (C6).
    pub lazy: bool,
}

/// What the device keeps of its state in the cache.
#[derive(Serialize, Deserialize)]
struct Saved {
    store: StoreSnapshot,
    #[serde(default)]
    head: Option<String>,
}

/// Reads a cache file; `None` when there is none (or no cache).
fn read_cached(path: Option<&Path>) -> Option<Vec<u8>> {
    fs::read(path?).ok()
}

/// Writes a cache file. A cache that cannot be written (a full disk) only
/// costs downloads later: the error is dropped.
fn write_cached(path: Option<&Path>, bytes: &[u8]) {
    if let Some(path) = path {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = write_atomically(path, bytes);
    }
}

/// The objects a device holds, and where the others are.
pub(super) struct Local {
    pub(super) objects: Objects,
    /// The live packs.
    refs: HashMap<String, PackRef>,
    /// The opened index of every live pack.
    indexes: HashMap<String, PackIndex>,
    /// The packs read whole.
    fetched: HashSet<String>,
    /// The chunks read (pack name, chunk).
    chunks: HashSet<(String, u32)>,
    /// Every id some pack holds, read or not: what a publish never packs again.
    pub(super) published: HashSet<ObjectId>,
    cache: Option<PathBuf>,
    pub(super) lazy: bool,
}

impl Local {
    fn new(options: &RepoOptions) -> Self {
        Local {
            objects: Objects::new(),
            refs: HashMap::new(),
            indexes: HashMap::new(),
            fetched: HashSet::new(),
            chunks: HashSet::new(),
            published: HashSet::new(),
            cache: options.cache_dir.clone(),
            lazy: options.lazy,
        }
    }

    fn file(&self, name: &str) -> Option<PathBuf> {
        self.cache.as_ref().map(|dir| dir.join("packs").join(name))
    }

    /// The opened index of the pack `name`, from the cache or the bucket.
    fn open_index<B: Bucket, S: Sealer>(
        &self,
        pack: &PackRef,
        packs: &Packs<'_, B, S>,
    ) -> Result<PackIndex, MetaError> {
        let path = self.file(&format!("{}.idx", pack.name));
        if let Some(bytes) = read_cached(path.as_deref()) {
            if let Ok(index) = PackIndex::open(packs.sealer(), &pack.name, &bytes) {
                return Ok(index);
            }
        }
        let bytes = packs.idx_bytes(pack)?;
        let index = PackIndex::open(packs.sealer(), &pack.name, &bytes)?;
        write_cached(path.as_deref(), &bytes);
        Ok(index)
    }

    /// Brings the indexes (and, unless lazy, the objects) up to the live packs
    /// of `state`; forgets the packs that are no longer live and their cache files.
    pub(super) fn refresh<B: Bucket, S: Sealer>(
        &mut self,
        state: &RepoState,
        packs: &Packs<'_, B, S>,
    ) -> Result<(), MetaError> {
        for pack in &state.packs {
            if !self.indexes.contains_key(&pack.name) {
                let index = self.open_index(pack, packs)?;
                self.published.extend(index.entries().iter().map(|e| e.id));
                self.indexes.insert(pack.name.clone(), index);
                self.refs.insert(pack.name.clone(), pack.clone());
            }
            if !self.lazy && !self.fetched.contains(&pack.name) {
                self.fetch_whole(&pack.name, packs)?;
            }
        }
        let live: HashSet<&str> = state.packs.iter().map(|p| p.name.as_str()).collect();
        let gone: Vec<String> = self
            .indexes
            .keys()
            .filter(|name| !live.contains(name.as_str()))
            .cloned()
            .collect();
        for name in gone {
            self.indexes.remove(&name);
            self.refs.remove(&name);
            self.fetched.remove(&name);
            self.chunks.retain(|(pack, _)| *pack != name);
            if let Some(dir) = self.cache.as_ref().map(|d| d.join("packs")) {
                if let Ok(files) = fs::read_dir(&dir) {
                    let prefix = format!("{name}.");
                    for file in files.flatten() {
                        if file.file_name().to_string_lossy().starts_with(&prefix) {
                            let _ = fs::remove_file(file.path());
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Reads the live pack `name` whole, from the cache or the bucket.
    fn fetch_whole<B: Bucket, S: Sealer>(
        &mut self,
        name: &str,
        packs: &Packs<'_, B, S>,
    ) -> Result<(), MetaError> {
        let pack = self.refs.get(name).cloned().ok_or_else(|| MetaError::Corrupt {
            key: name.to_string(),
            reason: "a pack the device does not know".to_string(),
        })?;
        let path = self.file(&format!("{name}.pack"));
        let index = self.indexes.get(name).ok_or_else(|| MetaError::Corrupt {
            key: name.to_string(),
            reason: "a pack without its index".to_string(),
        })?;
        // The cached copy, unless it is damaged: then the bucket's.
        let cached = read_cached(path.as_deref())
            .filter(|bytes| bytes.len() as u64 == pack.pack_size)
            .is_some_and(|bytes| {
                index
                    .read_into(packs.sealer(), &bytes, &mut self.objects)
                    .is_ok()
            });
        if !cached {
            let bytes = packs.pack_bytes(&pack)?;
            index.read_into(packs.sealer(), &bytes, &mut self.objects)?;
            write_cached(path.as_deref(), &bytes);
        }
        self.fetched.insert(name.to_string());
        Ok(())
    }

    /// Reads every live pack whole (a lazy copy before it merges).
    pub(super) fn fetch_all<B: Bucket, S: Sealer>(
        &mut self,
        packs: &Packs<'_, B, S>,
    ) -> Result<(), MetaError> {
        let names: Vec<String> = self
            .refs
            .keys()
            .filter(|name| !self.fetched.contains(*name))
            .cloned()
            .collect();
        for name in names {
            self.fetch_whole(&name, packs)?;
        }
        Ok(())
    }

    /// Makes the objects `ids` local. In a lazy copy: the chunks that hold them,
    /// one ranged read each (several wanted objects in one chunk cost one read).
    /// An id no live pack holds is [`MetaError::MissingObject`].
    pub(super) fn ensure<B: Bucket, S: Sealer>(
        &mut self,
        ids: &[ObjectId],
        packs: &Packs<'_, B, S>,
    ) -> Result<(), MetaError> {
        let mut wanted: BTreeSet<(String, u32)> = BTreeSet::new();
        for id in ids {
            if self.objects.contains(id) {
                continue;
            }
            let found = self
                .indexes
                .iter()
                .find_map(|(name, index)| index.find(id).map(|e| (name.clone(), e.chunk)));
            match found {
                Some(chunk) => {
                    wanted.insert(chunk);
                }
                None => return Err(MetaError::MissingObject { id: id.to_hex() }),
            }
        }
        for (name, chunk) in wanted {
            let path = self.file(&format!("{name}.{chunk}.chunk"));
            let pack = self.refs.get(&name).cloned().ok_or_else(|| MetaError::Corrupt {
                key: name.clone(),
                reason: "a pack the device does not know".to_string(),
            })?;
            let index = self.indexes.get(&name).ok_or_else(|| MetaError::Corrupt {
                key: name.clone(),
                reason: "a pack without its index".to_string(),
            })?;
            // The cached chunk, unless it is damaged: then the bucket's.
            let opened = read_cached(path.as_deref())
                .and_then(|sealed| index.open_chunk(packs.sealer(), chunk, &sealed).ok());
            let plain = match opened {
                Some(plain) => plain,
                None => {
                    let sealed = packs.read_chunk(&pack, index, chunk)?;
                    let plain = index.open_chunk(packs.sealer(), chunk, &sealed)?;
                    write_cached(path.as_deref(), &sealed);
                    plain
                }
            };
            for entry in index.entries().iter().filter(|e| e.chunk == chunk) {
                let body = index.object_in_chunk(entry, &plain)?;
                self.objects.insert_checked(entry.id, entry.kind, body)?;
            }
            self.chunks.insert((name, chunk));
        }
        Ok(())
    }

    /// Makes the objects to read the folder `path` (`""`: the root) of the tree
    /// `root` local: the trees on the way (one shard of a sharded folder), the
    /// folder whole and, with `files`, the blobs of its files. The folder's
    /// tree id, or `None` when there is no folder there.
    pub(super) fn ensure_folder<B: Bucket, S: Sealer>(
        &mut self,
        root: &ObjectId,
        path: &str,
        files: bool,
        packs: &Packs<'_, B, S>,
    ) -> Result<Option<ObjectId>, MetaError> {
        self.ensure(&[*root], packs)?;
        let mut folder = *root;
        if !path.is_empty() {
            for part in segments(path)? {
                let Some(entry) = self.ensure_lookup(&folder, part, packs)? else {
                    return Ok(None);
                };
                if entry.0 != Mode::Tree {
                    return Ok(None);
                }
                self.ensure(&[entry.1], packs)?;
                folder = entry.1;
            }
        }
        let shard_ids: Vec<ObjectId> = shard::shards(&self.objects.tree(&folder)?)
            .iter()
            .map(|e| e.id)
            .collect();
        self.ensure(&shard_ids, packs)?;
        if files {
            let blobs: Vec<ObjectId> = shard::read_folder(&self.objects, &folder)?
                .entries()
                .iter()
                .filter(|e| e.mode == Mode::File)
                .map(|e| e.id)
                .collect();
            self.ensure(&blobs, packs)?;
        }
        Ok(Some(folder))
    }

    /// The entry `name` of the (local) folder tree `folder`, reading the one
    /// shard it is in when the folder is sharded.
    fn ensure_lookup<B: Bucket, S: Sealer>(
        &mut self,
        folder: &ObjectId,
        name: &str,
        packs: &Packs<'_, B, S>,
    ) -> Result<Option<(Mode, ObjectId)>, MetaError> {
        let raw = self.objects.tree(folder)?;
        if shard::is_sharded(&raw) {
            if let Some(held) = raw.get(&shard::shard_of(name)) {
                self.ensure(&[held.id], packs)?;
            }
        }
        Ok(shard::lookup(&self.objects, folder, name)?.map(|e| (e.mode, e.id)))
    }

    /// Makes the objects to read the entry at `path` of the tree `root` local:
    /// the trees on the way and, for a file, its blob.
    pub(super) fn ensure_entry<B: Bucket, S: Sealer>(
        &mut self,
        root: &ObjectId,
        path: &str,
        packs: &Packs<'_, B, S>,
    ) -> Result<(), MetaError> {
        let parts = segments(path)?;
        let (name, folders) = parts.split_last().ok_or_else(|| MetaError::Corrupt {
            key: path.to_string(),
            reason: "an empty path".to_string(),
        })?;
        let Some(folder) = self.ensure_folder(root, &folders.join("/"), false, packs)? else {
            return Ok(());
        };
        if let Some((Mode::File, blob)) = self.ensure_lookup(&folder, name, packs)? {
            self.ensure(&[blob], packs)?;
        }
        Ok(())
    }

    /// Notes a pack this device wrote and published: its objects are local; the
    /// cache keeps a sealed copy (the same name; the bucket's has other nonces).
    fn add_own<S: Sealer>(
        &mut self,
        pack: &PackRef,
        ids: &[ObjectId],
        sealer: &S,
    ) -> Result<(), MetaError> {
        let mut writer = PackWriter::new();
        for id in ids {
            writer.add_from(&self.objects, id)?;
        }
        let sealed = writer.seal(sealer)?;
        let index = PackIndex::open(sealer, &sealed.name, &sealed.idx)?;
        write_cached(self.file(&format!("{}.idx", pack.name)).as_deref(), &sealed.idx);
        write_cached(self.file(&format!("{}.pack", pack.name)).as_deref(), &sealed.pack);
        self.published.extend(ids.iter().copied());
        self.indexes.insert(pack.name.clone(), index);
        self.refs.insert(pack.name.clone(), pack.clone());
        self.fetched.insert(pack.name.clone());
        Ok(())
    }
}

/// The drive's head as the state names it.
pub(super) fn remote_head(state: &RepoState) -> Result<Option<ObjectId>, MetaError> {
    match state.refs.get(MAIN) {
        None => Ok(None),
        Some(hex) => ObjectId::from_hex(hex).map(Some).ok_or_else(|| MetaError::Corrupt {
            key: MAIN.to_string(),
            reason: "not an object id".to_string(),
        }),
    }
}

/// Every object reachable from `head` that no pack holds yet. A published
/// object's whole closure is published (every pack carries closures), so the
/// walk stops there.
pub(super) fn unpublished(
    objects: &Objects,
    head: &ObjectId,
    published: &HashSet<ObjectId>,
) -> Result<Vec<ObjectId>, MetaError> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    let mut stack = vec![*head];
    while let Some(id) = stack.pop() {
        if published.contains(&id) || !seen.insert(id) {
            continue;
        }
        let (kind, _) = objects
            .get(&id)
            .ok_or_else(|| MetaError::MissingObject { id: id.to_hex() })?;
        match kind {
            Kind::Commit => {
                let commit = objects.commit(&id)?;
                stack.push(commit.tree);
                stack.extend(commit.parents);
            }
            Kind::Tree => stack.extend(objects.tree(&id)?.entries().iter().map(|e| e.id)),
            Kind::Blob => {}
        }
        out.push(id);
    }
    Ok(out)
}

/// Whether `ancestor` is `id` or comes before it.
fn is_ancestor(objects: &Objects, ancestor: &ObjectId, id: &ObjectId) -> Result<bool, MetaError> {
    let mut seen = HashSet::new();
    let mut stack = vec![*id];
    while let Some(next) = stack.pop() {
        if next == *ancestor {
            return Ok(true);
        }
        if seen.insert(next) {
            stack.extend(objects.commit(&next)?.parents);
        }
    }
    Ok(false)
}

/// One device's copy of the drive index.
pub struct MetaRepo<B: Bucket, S: Sealer> {
    pub(super) store: MetaStore<B, S>,
    pub(super) local: Local,
    pub(super) head: Option<ObjectId>,
    /// Shown in conflict copies and commits ("Laptop").
    pub(super) device_name: String,
}

impl<B: Bucket, S: Sealer> MetaRepo<B, S> {
    fn with_store(store: MetaStore<B, S>, device_name: &str, options: &RepoOptions) -> Self {
        MetaRepo {
            store,
            local: Local::new(options),
            head: None,
            device_name: device_name.to_string(),
        }
    }

    /// Creates the drive index in `bucket`. `device_id` names this device in
    /// the log, `device_name` in commits and conflict copies.
    pub fn create(bucket: B, sealer: S, device_id: &str, device_name: &str) -> Result<Self, MetaError> {
        MetaRepo::create_with(bucket, sealer, device_id, device_name, &RepoOptions::default())
    }

    /// [`MetaRepo::create`] with a cache folder or a lazy copy.
    pub fn create_with(
        bucket: B,
        sealer: S,
        device_id: &str,
        device_name: &str,
        options: &RepoOptions,
    ) -> Result<Self, MetaError> {
        let store = MetaStore::create(bucket, sealer, device_id)?;
        let repo = MetaRepo::with_store(store, device_name, options);
        repo.save();
        Ok(repo)
    }

    /// Opens the drive index in `bucket` and reads it up to its head.
    pub fn open(bucket: B, sealer: S, device_id: &str, device_name: &str) -> Result<Self, MetaError> {
        MetaRepo::open_with(bucket, sealer, device_id, device_name, &RepoOptions::default())
    }

    /// [`MetaRepo::open`] from a cache folder (only what is new is downloaded),
    /// or as a lazy copy.
    pub fn open_with(
        bucket: B,
        sealer: S,
        device_id: &str,
        device_name: &str,
        options: &RepoOptions,
    ) -> Result<Self, MetaError> {
        let saved = options
            .cache_dir
            .as_deref()
            .and_then(|dir| read_cached(Some(dir.join("state").as_path())))
            .and_then(|sealed| sealer.open(STATE_CONTEXT, &sealed).ok())
            .and_then(|plain| serde_json::from_slice::<Saved>(&plain).ok());
        let store = match saved {
            Some(saved) => MetaStore::resume(bucket, sealer, device_id, saved.store),
            None => MetaStore::open(bucket, sealer, device_id)?,
        };
        let mut repo = MetaRepo::with_store(store, device_name, options);
        repo.pull()?;
        Ok(repo)
    }

    /// Writes the state to the cache folder (when there is one).
    pub(super) fn save(&self) {
        let Some(dir) = &self.local.cache else {
            return;
        };
        let saved = Saved {
            store: self.store.snapshot(),
            head: self.head.map(|h| h.to_hex()),
        };
        let Ok(plain) = serde_json::to_vec(&saved) else {
            return;
        };
        if let Ok(sealed) = self.store.sealer().seal(STATE_CONTEXT, &plain) {
            write_cached(Some(dir.join("state").as_path()), &sealed);
        }
    }

    /// Whether the copy was opened from the cache without reaching the bucket.
    #[must_use]
    pub fn is_offline(&self) -> bool {
        false
    }

    /// Sets the store's clock (seconds since 1970): the tests' time.
    #[must_use]
    pub fn with_clock(mut self, clock: impl Fn() -> u64 + Send + Sync + 'static) -> Self {
        self.store = self.store.with_clock(clock);
        self
    }

    #[must_use]
    pub fn store(&self) -> &MetaStore<B, S> {
        &self.store
    }

    /// The store, for maintenance (checkpoints, leases, compaction).
    pub fn store_mut(&mut self) -> &mut MetaStore<B, S> {
        &mut self.store
    }

    /// The objects this device holds (in a lazy copy: the ones read so far).
    #[must_use]
    pub fn objects(&self) -> &Objects {
        &self.local.objects
    }

    /// This device's head commit; `None` before anything was committed.
    #[must_use]
    pub fn head(&self) -> Option<ObjectId> {
        self.head
    }

    /// The root tree of the head.
    pub fn root(&self) -> Result<Option<ObjectId>, MetaError> {
        self.head
            .map(|head| self.local.objects.commit(&head).map(|c| c.tree))
            .transpose()
    }

    /// Stores a file's bytes (a pointer file, the policy) as a blob; its id for
    /// [`Change::Put`].
    pub fn write_blob(&mut self, bytes: &[u8]) -> ObjectId {
        self.local.objects.write_blob(bytes)
    }

    /// Polls the drive: reads the new packs (in a lazy copy: their indexes and
    /// the head's commit and root folder) and moves to the drive's head.
    /// Whether anything changed.
    pub fn pull(&mut self) -> Result<bool, MetaError> {
        let report = self.store.sync()?;
        let packs = self.store.packs();
        self.local.refresh(self.store.state(), &packs)?;
        let remote = remote_head(self.store.state())?;
        if let Some(head) = remote {
            self.local.ensure(&[head], &packs)?;
            let root = self.local.objects.commit(&head)?.tree;
            self.local.ensure_folder(&root, "", false, &packs)?;
        }
        let moved = remote != self.head;
        // Every commit of this device is published when `commit` returns, so the
        // drive's head is at or after it.
        self.head = remote;
        self.save();
        Ok(report.changed || moved)
    }

    /// Makes what reading the entry at `path` needs local: the trees on the
    /// way and, for a file, its blob. In a full copy nothing is read.
    pub fn ensure_entry(&mut self, path: &str) -> Result<(), MetaError> {
        let Some(root) = self.root()? else {
            return Ok(());
        };
        let packs = self.store.packs();
        self.local.ensure_entry(&root, path, &packs)
    }

    /// Reads every pack whole (a lazy copy becomes a full one).
    pub fn fetch_all(&mut self) -> Result<(), MetaError> {
        let packs = self.store.packs();
        self.local.fetch_all(&packs)
    }

    /// Makes what the folder `path` (`""`: the root) shows local - its trees
    /// and, with `files`, its files' blobs - and returns its tree; `None` when
    /// there is no such folder. In a full copy nothing is read.
    pub fn ensure_folder(&mut self, path: &str, files: bool) -> Result<Option<ObjectId>, MetaError> {
        let Some(root) = self.root()? else {
            return Ok(None);
        };
        let packs = self.store.packs();
        self.local.ensure_folder(&root, path, files, &packs)
    }

    pub(super) fn signature(&self) -> Signature {
        let now = i64::try_from(self.store.now()).unwrap_or(0);
        Signature {
            name: self.device_name.clone(),
            email: self.store.device().to_string(),
            time: now,
            offset_minutes: 0,
        }
    }

    /// Applies `changes` to the head's tree, commits and publishes. When the
    /// drive moved on, merges with it (`resolve` decides each conflict; an
    /// unattended sync passes [`super::merge::keep_both`]).
    pub fn commit(
        &mut self,
        changes: &[Change],
        message: &str,
        resolve: &mut dyn FnMut(&Conflict) -> Resolution,
    ) -> Result<CommitOutcome, MetaError> {
        if self.local.lazy {
            self.fetch_all()?;
        }
        let root = self.root()?;
        let tree = apply(&mut self.local.objects, root.as_ref(), changes)?;
        let signature = self.signature();
        let mine = self.local.objects.write_commit(&Commit {
            tree,
            parents: self.head.into_iter().collect(),
            author: signature.clone(),
            committer: signature.clone(),
            message: message.to_string(),
        });

        let device_name = self.device_name.clone();
        let local = &mut self.local;
        let mut outcome: Option<(ObjectId, Vec<Resolved>, Vec<ObjectId>)> = None;
        let result = self.store.publish_with(|state, packs| {
            local.refresh(state, packs)?;
            if local.lazy {
                local.fetch_all(packs)?;
            }
            let objects = &mut local.objects;
            let remote = remote_head(state)?;
            let (head, conflicts) = match remote {
                None => (mine, Vec::new()),
                Some(remote) if is_ancestor(objects, &remote, &mine)? => (mine, Vec::new()),
                Some(remote) if is_ancestor(objects, &mine, &remote)? => {
                    // Already on the drive: nothing to publish.
                    outcome = Some((remote, Vec::new(), Vec::new()));
                    return Ok(None);
                }
                Some(remote) => {
                    let base = merge_base(objects, &mine, &remote)?;
                    let base_tree = match base {
                        Some(base) => Some(objects.commit(&base)?.tree),
                        None => None,
                    };
                    let theirs_tree = objects.commit(&remote)?.tree;
                    let merged = merge_trees(
                        objects,
                        base_tree.as_ref(),
                        &tree,
                        &theirs_tree,
                        &device_name,
                        &mut *resolve,
                    )?;
                    let merge = objects.write_commit(&Commit {
                        tree: merged.tree,
                        parents: vec![remote, mine],
                        author: signature.clone(),
                        committer: signature.clone(),
                        message: format!("Merge: {message}"),
                    });
                    (merge, merged.conflicts)
                }
            };
            let new_objects = unpublished(objects, &head, &local.published)?;
            let mut writer = PackWriter::new();
            for id in &new_objects {
                writer.add_from(&local.objects, id)?;
            }
            outcome = Some((head, conflicts, new_objects));
            Ok(Some(Publish {
                pack: Some(writer),
                updates: vec![RefUpdate {
                    name: MAIN.to_string(),
                    old: remote.map(|r| r.to_hex()),
                    new: Some(head.to_hex()),
                }],
                message: message.to_string(),
            }))
        })?;
        let (head, conflicts, packed) = outcome.ok_or_else(|| MetaError::Corrupt {
            key: MAIN.to_string(),
            reason: "a publish without a head".to_string(),
        })?;
        let landed = self.landed(result, &packed)?;
        self.head = Some(head);
        self.save();
        Ok(CommitOutcome {
            head,
            published: landed,
            conflicts,
        })
    }

    /// Notes what a publish wrote (its pack is this device's) and what it
    /// became; for a publish of nothing, the head it found.
    pub(super) fn landed(
        &mut self,
        result: Option<Published>,
        packed: &[ObjectId],
    ) -> Result<Published, MetaError> {
        let landed = match result {
            Some(landed) => landed,
            None => Published {
                seq: self.store.state().head_seq,
                pack: None,
                attempts: 1,
            },
        };
        if let Some(pack) = &landed.pack {
            self.local.add_own(pack, packed, self.store.sealer())?;
        }
        Ok(landed)
    }

    /// Whether the entry at `path` differs between the commit `base` (the
    /// version a local copy is based on) and the head: the check before an
    /// upload (D52). Pull first to compare with the drive as it is now.
    pub fn file_changed_since(&self, path: &str, base: &ObjectId) -> Result<bool, MetaError> {
        let objects = &self.local.objects;
        let then = entry_at(objects, &objects.commit(base)?.tree, path)?;
        let now = match self.root()? {
            Some(root) => entry_at(objects, &root, path)?,
            None => None,
        };
        Ok(then != now)
    }

    /// The commits of the head back to the first, following first parents
    /// (the drive's own history), newest first. A lazy copy reads every pack
    /// first ([`MetaRepo::fetch_all`]).
    pub fn history(&self) -> Result<Vec<(ObjectId, Commit)>, MetaError> {
        let mut out = Vec::new();
        let mut next = self.head;
        while let Some(id) = next {
            let commit = self.local.objects.commit(&id)?;
            next = commit.parents.first().copied();
            out.push((id, commit));
        }
        Ok(out)
    }

    /// The file at `path` as it was in the commit `at`; `None` when there was none.
    pub fn file_at(&self, at: &ObjectId, path: &str) -> Result<Option<ObjectId>, MetaError> {
        let objects = &self.local.objects;
        let tree = objects.commit(at)?.tree;
        Ok(entry_at(objects, &tree, path)?
            .filter(|e| e.mode == Mode::File)
            .map(|e| e.id))
    }

    /// Puts the file at `path` back as it was in the commit `at` (deletes it
    /// when it was not there), as a new commit.
    pub fn restore(
        &mut self,
        path: &str,
        at: &ObjectId,
        resolve: &mut dyn FnMut(&Conflict) -> Resolution,
    ) -> Result<CommitOutcome, MetaError> {
        let change = match self.file_at(at, path)? {
            Some(id) => Change::Put {
                path: path.to_string(),
                id,
            },
            None => Change::Delete {
                path: path.to_string(),
            },
        };
        self.commit(&[change], &format!("Restore {path}"), resolve)
    }
}
