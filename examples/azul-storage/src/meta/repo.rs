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

use std::collections::HashSet;

use super::{
    bucket::Bucket,
    merge::{merge_base, merge_trees, Conflict, Resolution, Resolved},
    objects::{Commit, Kind, Mode, ObjectId, Objects, Signature},
    pack::PackWriter,
    seal::Sealer,
    tree::{apply, entry_at, Change},
    wal::{MetaStore, Packs, Publish, Published, RefUpdate, RepoState},
    MetaError,
};

/// The branch that is the drive's history.
pub const MAIN: &str = "refs/heads/main";

/// What a commit became.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitOutcome {
    /// The drive's new head: this device's commit, or the merge of it.
    pub head: ObjectId,
    pub published: Published,
    /// The conflicts the merge met, with what became of them.
    pub conflicts: Vec<Resolved>,
}

/// One device's copy of the drive index.
pub struct MetaRepo<B: Bucket, S: Sealer> {
    store: MetaStore<B, S>,
    objects: Objects,
    /// The packs read into `objects`.
    fetched: HashSet<String>,
    /// The objects some pack holds: what a publish never packs again.
    published: HashSet<ObjectId>,
    head: Option<ObjectId>,
    /// Shown in conflict copies and commits ("Laptop").
    device_name: String,
}

/// Fetches the packs of `state` not read yet.
fn fetch_new<B: Bucket, S: Sealer>(
    state: &RepoState,
    packs: &Packs<'_, B, S>,
    objects: &mut Objects,
    fetched: &mut HashSet<String>,
    published: &mut HashSet<ObjectId>,
) -> Result<(), MetaError> {
    for pack in &state.packs {
        if !fetched.contains(&pack.name) {
            published.extend(packs.fetch(pack, objects)?);
            fetched.insert(pack.name.clone());
        }
    }
    Ok(())
}

/// The drive's head as the state names it.
fn remote_head(state: &RepoState) -> Result<Option<ObjectId>, MetaError> {
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
fn unpublished(
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

impl<B: Bucket, S: Sealer> MetaRepo<B, S> {
    fn with_store(store: MetaStore<B, S>, device_name: &str) -> Self {
        MetaRepo {
            store,
            objects: Objects::new(),
            fetched: HashSet::new(),
            published: HashSet::new(),
            head: None,
            device_name: device_name.to_string(),
        }
    }

    /// Creates the drive index in `bucket`. `device_id` names this device in
    /// the log, `device_name` in commits and conflict copies.
    pub fn create(bucket: B, sealer: S, device_id: &str, device_name: &str) -> Result<Self, MetaError> {
        let store = MetaStore::create(bucket, sealer, device_id)?;
        Ok(MetaRepo::with_store(store, device_name))
    }

    /// Opens the drive index in `bucket` and reads it up to its head.
    pub fn open(bucket: B, sealer: S, device_id: &str, device_name: &str) -> Result<Self, MetaError> {
        let store = MetaStore::open(bucket, sealer, device_id)?;
        let mut repo = MetaRepo::with_store(store, device_name);
        repo.pull()?;
        Ok(repo)
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

    #[must_use]
    pub fn objects(&self) -> &Objects {
        &self.objects
    }

    /// This device's head commit; `None` before anything was committed.
    #[must_use]
    pub fn head(&self) -> Option<ObjectId> {
        self.head
    }

    /// The root tree of the head.
    pub fn root(&self) -> Result<Option<ObjectId>, MetaError> {
        self.head
            .map(|head| self.objects.commit(&head).map(|c| c.tree))
            .transpose()
    }

    /// Stores a file's bytes (a pointer file, the policy) as a blob; its id for
    /// [`Change::Put`].
    pub fn write_blob(&mut self, bytes: &[u8]) -> ObjectId {
        self.objects.write_blob(bytes)
    }

    /// Polls the drive: reads the new packs and moves to the drive's head.
    /// Whether anything changed.
    pub fn pull(&mut self) -> Result<bool, MetaError> {
        let report = self.store.sync()?;
        let packs = self.store.packs();
        fetch_new(
            self.store.state(),
            &packs,
            &mut self.objects,
            &mut self.fetched,
            &mut self.published,
        )?;
        let remote = remote_head(self.store.state())?;
        let moved = remote != self.head;
        // Every commit of this device is published when `commit` returns, so the
        // drive's head is at or after it.
        self.head = remote;
        Ok(report.changed || moved)
    }

    fn signature(&self) -> Signature {
        let now = i64::try_from(crate::time::now_unix()).unwrap_or(0);
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
        let root = self.root()?;
        let tree = apply(&mut self.objects, root.as_ref(), changes)?;
        let signature = self.signature();
        let mine = self.objects.write_commit(&Commit {
            tree,
            parents: self.head.into_iter().collect(),
            author: signature.clone(),
            committer: signature.clone(),
            message: message.to_string(),
        });

        let device_name = self.device_name.clone();
        let objects = &mut self.objects;
        let fetched = &mut self.fetched;
        let published_ids = &mut self.published;
        let mut outcome: Option<(ObjectId, Vec<Resolved>, Vec<ObjectId>)> = None;
        let result = self.store.publish_with(|state, packs| {
            fetch_new(state, packs, objects, fetched, published_ids)?;
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
            let new_objects = unpublished(objects, &head, published_ids)?;
            let mut writer = PackWriter::new();
            for id in &new_objects {
                writer.add_from(objects, id)?;
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
        let landed = match result {
            Some(landed) => landed,
            None => Published {
                seq: self.store.state().head_seq,
                pack: None,
                attempts: 1,
            },
        };
        if let Some(pack) = &landed.pack {
            self.fetched.insert(pack.name.clone());
        }
        self.published.extend(packed);
        self.head = Some(head);
        Ok(CommitOutcome {
            head,
            published: landed,
            conflicts,
        })
    }

    /// Whether the entry at `path` differs between the commit `base` (the
    /// version a local copy is based on) and the head: the check before an
    /// upload (D52). Pull first to compare with the drive as it is now.
    pub fn file_changed_since(&self, path: &str, base: &ObjectId) -> Result<bool, MetaError> {
        let then = entry_at(&self.objects, &self.objects.commit(base)?.tree, path)?;
        let now = match self.root()? {
            Some(root) => entry_at(&self.objects, &root, path)?,
            None => None,
        };
        Ok(then != now)
    }

    /// The commits of the head back to the first, following first parents
    /// (the drive's own history), newest first.
    pub fn history(&self) -> Result<Vec<(ObjectId, Commit)>, MetaError> {
        let mut out = Vec::new();
        let mut next = self.head;
        while let Some(id) = next {
            let commit = self.objects.commit(&id)?;
            next = commit.parents.first().copied();
            out.push((id, commit));
        }
        Ok(out)
    }

    /// The file at `path` as it was in the commit `at`; `None` when there was none.
    pub fn file_at(&self, at: &ObjectId, path: &str) -> Result<Option<ObjectId>, MetaError> {
        let tree = self.objects.commit(at)?.tree;
        Ok(entry_at(&self.objects, &tree, path)?
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
