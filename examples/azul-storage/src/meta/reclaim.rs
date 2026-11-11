//! The lifecycle of a drive's data objects (feature `encryption`; ARCH28 RF44 / AR5a-5): what may
//! take a data object out of the bucket, and on what proof.
//!
//! A data object (`data/xx/<id>`: a file's version, or a pack of small files) used to be deleted
//! at a dozen places on a dozen proofs, each proof a guess ("the index refused it", "the head does
//! not name it", "thirty days passed"). The rules of this module are few:
//!
//! - **A new object that no change named** is taken back out only by [`drop_new_object`], and
//!   only on a [`Refusal`]: the proof that the change was refused for good or never made
//!   ([`Certainty`]). A change whose outcome is not known - no answer, a service error - may have
//!   landed; the object it uploaded stays (garbage at worst, never a name without its object).
//! - **Nothing else** deletes a data object of a drive that keeps its history (the drive index is
//!   a repository: every state of the drive is a commit). Deleting is a decision about history,
//!   and only the reclaimer makes it.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fmt,
};

use super::{
    bucket::Bucket,
    certainty::Certainty,
    objects::{Mode, ObjectId as GitId, Objects, TreeEntry},
    pointer,
    repo::MetaRepo,
    retention::Retention,
    seal::Sealer,
    shard,
    wal::{LeaseGuard, ReclaimMark, MAINTENANCE},
    MetaError,
};
use crate::{crypto::ObjectId, BatchDelete, Drive, DriveError};

/// The proof that the change which was to name a new object did not happen. Made from an error
/// whose certainty is [`Certainty::Refused`] or [`Certainty::NotAttempted`], and from nothing else;
/// it cannot be copied, defaulted or read from a file.
#[derive(Debug)]
pub(crate) struct Refusal {
    _sealed: (),
}

impl Refusal {
    /// The refusal `error` is, if it is one; `None` for a change that may have landed.
    pub(crate) fn of(error: &DriveError) -> Option<Refusal> {
        match error.certainty() {
            Certainty::Unknown => None,
            Certainty::Refused | Certainty::NotAttempted => Some(Refusal { _sealed: () }),
        }
    }
}

/// Takes the new object `id` back out of `drive`'s bucket: the one compensation of a change that
/// did not name it (`_refusal` is the proof). A delete the bucket refuses leaves the object where
/// it is - named by nothing, and the caller says so.
///
/// # Errors
///
/// The bucket's error.
pub(crate) fn drop_new_object<D: Drive + ?Sized>(
    drive: &D,
    id: &ObjectId,
    _refusal: &Refusal,
) -> Result<(), DriveError> {
    drive.delete(&id.bucket_key())
}

/// What an index releases when a change leaves objects that no entry of its head names any more.
/// Two obligations that used to share one return value: an object nobody can ask for again is
/// deleted at once; one the history still names is not deleted by whoever released it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Released {
    /// Objects no state of the drive names any more - an index that keeps no history
    /// ([`crate::encrypted::MemoryIndex`]): they may go from the bucket now.
    pub delete_now: Vec<ObjectId>,
    /// Objects the head no longer names but the history does (a repository: every state of the
    /// drive is a commit). They stay in the bucket until the reclaimer has proved them past their
    /// retention ([`super::retention::RETENTION`]). A hint: it tells a caller not to delete, and
    /// wakes the reclaimer; nothing depends on it, because the reclaimer derives what was released
    /// from the commits themselves.
    pub held_by_history: Vec<ObjectId>,
}

impl Released {
    /// Nothing released.
    #[must_use]
    pub fn none() -> Released {
        Released::default()
    }

    /// `ids` can go from the bucket now.
    #[must_use]
    pub fn delete_now(ids: Vec<ObjectId>) -> Released {
        Released {
            delete_now: ids,
            held_by_history: Vec::new(),
        }
    }

    /// `ids` are held by the history.
    #[must_use]
    pub fn held(ids: Vec<ObjectId>) -> Released {
        Released {
            delete_now: Vec::new(),
            held_by_history: ids,
        }
    }

    /// Whether nothing was released.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.delete_now.is_empty() && self.held_by_history.is_empty()
    }

    /// What `other` released as well.
    pub fn extend(&mut self, other: Released) {
        self.delete_now.extend(other.delete_now);
        self.held_by_history.extend(other.held_by_history);
    }
}

/// Deletes the objects an index without history released ([`Released::delete_now`]). The objects the
/// bucket would not delete come back with its answer: they are named by nothing and count against
/// the quota, so the caller keeps them to try again and says so (AZ-U22) - they used to be dropped
/// on the floor.
pub(crate) fn delete_released<D: Drive + ?Sized>(
    drive: &D,
    released: &Released,
) -> Vec<(ObjectId, DriveError)> {
    if released.delete_now.is_empty() {
        return Vec::new();
    }
    let keys: Vec<String> = released
        .delete_now
        .iter()
        .map(ObjectId::bucket_key)
        .collect();
    let items: Vec<BatchDelete<'_>> = keys.iter().map(|key| BatchDelete::new(key)).collect();
    let mut answers = drive.delete_many(&items).into_iter();
    let mut failed = Vec::new();
    for id in &released.delete_now {
        // A drive that answers for fewer items than it was asked is not taken to have deleted
        // the rest.
        let answer = answers.next().unwrap_or_else(|| {
            Err(DriveError::Protocol(String::from(
                "the storage answered for fewer deletes than were asked",
            )))
        });
        if let Err(e) = answer {
            failed.push((*id, e));
        }
    }
    failed
}

// ==== The reclaimer ====
//
// What a repository keeps is every state of the drive. A data object that the head no longer names
// is still named by the commits that did - and a user can ask for those states back for
// [`Retention::versions`]. When the last commit that names an object is older than that, nobody
// can ask for it any more, and the object only costs quota: it is reclaimable. The reclaimer
// derives this from the commits themselves; it records nothing a lost write could falsify, and
// nothing an older client does can make it delete more.

/// Whether and how the reclaimer runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReclaimMode {
    /// It does not run: the kill switch.
    Off,
    /// It runs, says "would delete N objects, M bytes" and deletes nothing: how it ships first.
    Shadow,
    /// It deletes what it proved.
    Delete,
}

/// The mode a drive's reclaimer starts in. The first release ships it in [`ReclaimMode::Shadow`]
/// (ARCH28 Q30: the most dangerous code of the client runs a release without a delete); the
/// next release changes this one line.
pub const DEFAULT_MODE: ReclaimMode = ReclaimMode::Shadow;

/// How much one run may do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Caps {
    /// The commits older than the window whose trees a run reads.
    pub commits: usize,
    /// The objects a run decides on.
    pub objects: usize,
    /// The objects deleted between two fences.
    pub batch: usize,
}

impl Default for Caps {
    fn default() -> Self {
        Caps {
            commits: 250_000,
            objects: 50_000,
            batch: 1_000,
        }
    }
}

/// Whether the bucket's clock must agree with this device's before anything is deleted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServerClock {
    /// The bucket says when the lease the run took was written, and that time and this device's
    /// agree within [`SKEW_SECS`]; a bucket that does not say, or a device whose clock is off,
    /// deletes nothing. The age of a commit is the device's word otherwise.
    Required,
    /// The device's clock alone (a folder on disk, the tests).
    Unchecked,
}

/// What a reclaimer run is told.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReclaimOptions {
    pub mode: ReclaimMode,
    pub caps: Caps,
    pub retention: Retention,
    pub server_clock: ServerClock,
    /// Whether the run goes on from the manifest's mark and moves it. A purge ([`purge_options`])
    /// reads the whole history and leaves the mark where it is.
    pub use_mark: bool,
}

impl ReclaimOptions {
    /// The reclaimer as the first release ships it (ARCH28 Q30): it runs, says what it would
    /// delete and deletes nothing. There is no `Default`: an app that builds an index provider says
    /// which it wants ([`ReclaimOptions::shadow`] or [`ReclaimOptions::off`]), and a reader of its
    /// code sees it (X-DELTA-AZDRIVE-SYNC-3).
    #[must_use]
    pub fn shadow() -> ReclaimOptions {
        ReclaimOptions {
            mode: DEFAULT_MODE,
            caps: Caps::default(),
            retention: super::retention::RETENTION,
            server_clock: ServerClock::Required,
            use_mark: true,
        }
    }

    /// The kill switch: the reclaimer does not run.
    #[must_use]
    pub fn off() -> ReclaimOptions {
        ReclaimOptions {
            mode: ReclaimMode::Off,
            ..ReclaimOptions::shadow()
        }
    }
}

/// What the user confirmed when they asked for the old versions of a drive to be erased: after a
/// key was changed ("I was hacked": re-encrypting every file leaves the old objects, which the old
/// key still opens, in the history) the history is restorable until the retention is over -
/// unless they say it is not. Made from the confirmation a person gave and from nothing else.
// The private field and not `#[non_exhaustive]`: only this module can make one, where
// `non_exhaustive` would let the whole crate.
#[allow(clippy::manual_non_exhaustive)]
#[derive(Debug)]
pub struct PurgeConfirmation {
    /// When they confirmed (seconds since 1970).
    pub at: u64,
    _sealed: (),
}

impl PurgeConfirmation {
    /// The user confirmed at `at` that every old version of every file is erased now, for good:
    /// the drive's history before this moment cannot be restored afterwards. Call this only after
    /// a person said so.
    #[must_use]
    pub fn confirmed_by_the_user(at: u64) -> PurgeConfirmation {
        PurgeConfirmation { at, _sealed: () }
    }
}

/// The options of a purge: every object the head does not name, whatever its age, deleted; the
/// whole history read; no mark kept. The bucket's clock must still agree with the device's.
#[must_use]
pub fn purge_options(_confirmation: &PurgeConfirmation) -> ReclaimOptions {
    ReclaimOptions {
        mode: ReclaimMode::Delete,
        caps: Caps {
            commits: usize::MAX,
            objects: usize::MAX,
            batch: Caps::default().batch,
        },
        retention: Retention {
            trash: std::time::Duration::ZERO,
            versions: std::time::Duration::ZERO,
        },
        server_clock: ServerClock::Required,
        use_mark: false,
    }
}

/// How far this device's clock and the bucket's may differ before the age of a commit is not
/// believed (seconds).
pub const SKEW_SECS: u64 = 600;

/// A commit stamped further ahead of the device's clock than this counts as stamped then: one
/// device with a wrong clock must not make a whole history look younger than it is.
const AHEAD_SECS: u64 = 86_400;

/// How long a run keeps the maintenance lease between two renewals (seconds).
const LEASE_SECS: u64 = 900;

/// Why a decision was not made: the reclaimer says "not yet" and deletes nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotYet {
    /// The kill switch is on.
    Off,
    /// A tree or a pointer could not be read: nothing is decided from what cannot be read.
    Unreadable { what: String, reason: String },
    /// The head or a commit of the window names the object.
    Named,
    /// The history does not show the object released by a commit past its retention.
    NotReleased,
    /// The drive moved since it was read: the reads are old.
    Moved,
    /// This device's clock and the bucket's disagree (or the bucket does not say).
    Clock { device: u64, server: Option<u64> },
    /// Another device holds the maintenance lease, or this one lost it.
    Lease,
}

impl fmt::Display for NotYet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NotYet::Off => f.write_str("the reclaimer is switched off"),
            NotYet::Unreadable { what, reason } => {
                write!(
                    f,
                    "\"{what}\" cannot be read ({reason}): nothing is decided"
                )
            }
            NotYet::Named => f.write_str("the head or a version inside the window names it"),
            NotYet::NotReleased => f.write_str("no commit past the retention released it"),
            NotYet::Moved => f.write_str("the drive changed since it was read"),
            NotYet::Clock { device, server } => match server {
                Some(server) => write!(
                    f,
                    "this device's clock ({device}) and the storage's ({server}) disagree"
                ),
                None => write!(
                    f,
                    "the storage does not tell the time, and this device's clock ({device}) is \
                     not believed alone"
                ),
            },
            NotYet::Lease => f.write_str("another device is tidying the drive"),
        }
    }
}

impl NotYet {
    fn unreadable(what: impl Into<String>, error: impl fmt::Display) -> NotYet {
        NotYet::Unreadable {
            what: what.into(),
            reason: error.to_string(),
        }
    }
}

/// The proof that a data object may leave the bucket: the history released it, the last commit
/// that named it is past the retention, neither the head nor a commit of the window names it, and
/// it was read fresh and whole. Made by the reclaimer's analysis and by nothing else; it cannot
/// be copied, defaulted or read from a file.
#[derive(Debug)]
pub struct Reclaimable {
    id: ObjectId,
    _sealed: (),
}

impl Reclaimable {
    /// The object.
    #[must_use]
    pub fn id(&self) -> ObjectId {
        self.id
    }
}

/// What became of a [`Reclaimable`] handed to the deleter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deleted {
    /// The object is gone from the bucket.
    Gone,
    /// Shadow mode (or off): it would have been, and nothing was deleted.
    Would,
}

/// The ONE delete of a data object that is not the compensation of a refused change: `proof` says
/// the history released it and its retention is over. In [`ReclaimMode::Shadow`] nothing is
/// deleted.
///
/// # Errors
///
/// The bucket's.
pub(crate) fn delete_reclaimable<D: Drive + ?Sized>(
    drive: &D,
    mode: ReclaimMode,
    proof: Reclaimable,
) -> Result<Deleted, DriveError> {
    match mode {
        ReclaimMode::Delete => drive.delete(&proof.id.bucket_key()).map(|()| Deleted::Gone),
        ReclaimMode::Shadow | ReclaimMode::Off => Ok(Deleted::Would),
    }
}

/// [`delete_reclaimable`] for the repository's own bucket.
fn remove_reclaimable<B: Bucket>(
    bucket: &B,
    mode: ReclaimMode,
    proof: Reclaimable,
) -> Result<Deleted, MetaError> {
    match mode {
        ReclaimMode::Delete => bucket
            .remove(&proof.id.bucket_key())
            .map(|()| Deleted::Gone),
        ReclaimMode::Shadow | ReclaimMode::Off => Ok(Deleted::Would),
    }
}

/// How far the analysis got through the commits older than the window: they are processed in the
/// order of this pair (effective time, then id).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Mark {
    pub(crate) time: u64,
    pub(crate) id: GitId,
}

impl Mark {
    fn from_wal(mark: &ReclaimMark) -> Option<Mark> {
        Some(Mark {
            time: mark.time,
            id: GitId::from_hex(&mark.commit)?,
        })
    }

    fn to_wal(self, at: u64) -> ReclaimMark {
        ReclaimMark {
            time: self.time,
            commit: self.id.to_hex(),
            at,
        }
    }
}

/// An object an old commit released - it was in the tree of a parent and is not in its own - and
/// nothing in the window names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Candidate {
    /// Its bytes in the bucket (a file's object, or a whole pack).
    pub(crate) bytes: u64,
    /// The first old commit that released it.
    pub(crate) by: GitId,
}

/// What the history says of the data objects of one drive at one moment.
#[derive(Debug)]
pub(crate) struct Analysis {
    /// The manifest revision the reads were made at (set by whoever read).
    pub(crate) revision: u64,
    /// The time of day the ages were judged by.
    pub(crate) now: u64,
    /// The commits before this time are old (seconds since 1970).
    pub(crate) window_start: u64,
    /// The commits reachable from the head.
    pub(crate) commits: usize,
    /// Those inside the window.
    pub(crate) window_commits: usize,
    /// Every data object the head, a commit of the window or the state the window began with
    /// names, with its bytes.
    pub(crate) live: HashMap<ObjectId, u64>,
    /// The ones the head names alone: what a restore of an old version has already brought back.
    pub(crate) head: HashSet<ObjectId>,
    /// Objects old commits released and nothing live names.
    pub(crate) candidates: BTreeMap<ObjectId, Candidate>,
    /// The old commits whose trees were read this time.
    pub(crate) scanned: usize,
    /// The newest old commit dealt with.
    pub(crate) cursor: Option<Mark>,
    /// A cap ended the scan before the old commits did.
    pub(crate) stopped_by_cap: bool,
}

/// One commit of the graph.
struct Node {
    tree: GitId,
    parents: Vec<GitId>,
    /// The effective time: never earlier than a parent's, never further ahead than the device's
    /// clock by more than [`AHEAD_SECS`].
    eff: u64,
}

/// The commits reachable from `head`, parents before children, with their effective times.
fn load_graph(
    objects: &Objects,
    head: &GitId,
    now: u64,
) -> Result<(HashMap<GitId, Node>, Vec<GitId>), NotYet> {
    let ceiling = now.saturating_add(AHEAD_SECS);
    let mut nodes: HashMap<GitId, Node> = HashMap::new();
    let mut order: Vec<GitId> = Vec::new();
    // The commits read and waiting for their parents: (tree, parents, own time).
    let mut pending: HashMap<GitId, (GitId, Vec<GitId>, u64)> = HashMap::new();
    let mut stack: Vec<(GitId, bool)> = vec![(*head, false)];
    while let Some((id, expanded)) = stack.pop() {
        if nodes.contains_key(&id) {
            continue;
        }
        if !expanded {
            if pending.contains_key(&id) {
                continue;
            }
            let commit = objects
                .commit(&id)
                .map_err(|e| NotYet::unreadable(format!("commit {}", id.to_hex()), e))?;
            let own = u64::try_from(commit.committer.time)
                .unwrap_or(0)
                .min(ceiling);
            stack.push((id, true));
            for parent in &commit.parents {
                if !nodes.contains_key(parent) {
                    stack.push((*parent, false));
                }
            }
            pending.insert(id, (commit.tree, commit.parents, own));
            continue;
        }
        let Some((tree, parents, own)) = pending.remove(&id) else {
            continue;
        };
        let mut eff = own;
        for parent in &parents {
            // A parent is finished before its child; one that is not is a loop, which a hash
            // cannot make.
            let Some(done) = nodes.get(parent) else {
                return Err(NotYet::unreadable(
                    format!("commit {}", id.to_hex()),
                    "its history loops back on itself",
                ));
            };
            eff = eff.max(done.eff);
        }
        nodes.insert(id, Node { tree, parents, eff });
        order.push(id);
    }
    Ok((nodes, order))
}

/// Reads the data objects the trees name, each tree and each pointer once however many commits
/// share them.
struct Walk<'a> {
    objects: &'a Objects,
    seen_trees: HashSet<(GitId, bool)>,
    seen_blobs: HashSet<GitId>,
}

impl<'a> Walk<'a> {
    fn new(objects: &'a Objects) -> Walk<'a> {
        Walk {
            objects,
            seen_trees: HashSet::new(),
            seen_blobs: HashSet::new(),
        }
    }

    /// Adds to `into` the data objects named by the pointer files below the root tree `root` that
    /// no earlier walk of this value has read. The drive's own `.azlin` folder holds no files of
    /// the drive (the policy, the members' keys) but its `sync` bookkeeping, which is sealed like
    /// every file.
    fn refs(&mut self, root: &GitId, into: &mut HashMap<ObjectId, u64>) -> Result<(), NotYet> {
        self.walk_from(vec![(*root, true)], into)
    }

    /// [`Walk::refs`] below a folder that is no root.
    fn subtree_refs(
        &mut self,
        tree: &GitId,
        into: &mut HashMap<ObjectId, u64>,
    ) -> Result<(), NotYet> {
        self.walk_from(vec![(*tree, false)], into)
    }

    fn walk_from(
        &mut self,
        mut stack: Vec<(GitId, bool)>,
        into: &mut HashMap<ObjectId, u64>,
    ) -> Result<(), NotYet> {
        while let Some((tree, is_root)) = stack.pop() {
            if !self.seen_trees.insert((tree, is_root)) {
                continue;
            }
            let folder = shard::read_folder(self.objects, &tree)
                .map_err(|e| NotYet::unreadable(format!("folder {}", tree.to_hex()), e))?;
            for entry in folder.entries() {
                match entry.mode {
                    Mode::Tree
                        if is_root && crate::names::Protected::is_bookkeeping(&entry.name) =>
                    {
                        self.bookkeeping(&entry.id, &mut stack)?;
                    }
                    Mode::Tree => stack.push((entry.id, false)),
                    Mode::File => self.pointer(&entry.id, &entry.name, into)?,
                }
            }
        }
        Ok(())
    }

    /// Adds to `into` the data objects the root tree `from` names that the root tree `to` does not
    /// name at the same place: what the change from one to the other dropped or replaced. Only
    /// the folders that differ are read, so a commit that changes one file costs one path.
    fn dropped(
        &mut self,
        from: &GitId,
        to: &GitId,
        into: &mut HashMap<ObjectId, u64>,
    ) -> Result<(), NotYet> {
        let mut stack: Vec<(GitId, GitId, bool)> = vec![(*from, *to, true)];
        while let Some((before, after, is_root)) = stack.pop() {
            if before == after {
                continue;
            }
            let was = shard::read_folder(self.objects, &before)
                .map_err(|e| NotYet::unreadable(format!("folder {}", before.to_hex()), e))?;
            let now = shard::read_folder(self.objects, &after)
                .map_err(|e| NotYet::unreadable(format!("folder {}", after.to_hex()), e))?;
            let now_by_name: HashMap<&str, &TreeEntry> = now
                .entries()
                .iter()
                .map(|entry| (entry.name.as_str(), entry))
                .collect();
            for entry in was.entries() {
                let same_place = now_by_name.get(entry.name.as_str()).copied();
                let bookkeeping = is_root && crate::names::Protected::is_bookkeeping(&entry.name);
                match (entry.mode, same_place) {
                    (mode, Some(other)) if other.mode == mode && other.id == entry.id => {}
                    (Mode::Tree, Some(other)) if other.mode == Mode::Tree && bookkeeping => {
                        self.sync_pair(&entry.id, &other.id, &mut stack, into)?;
                    }
                    (Mode::Tree, Some(other)) if other.mode == Mode::Tree => {
                        stack.push((entry.id, other.id, false));
                    }
                    (Mode::Tree, _) if bookkeeping => {
                        let mut below = Vec::new();
                        self.bookkeeping(&entry.id, &mut below)?;
                        self.walk_from(below, into)?;
                    }
                    (Mode::Tree, _) => self.subtree_refs(&entry.id, into)?,
                    (Mode::File, _) => self.pointer(&entry.id, &entry.name, into)?,
                }
            }
        }
        Ok(())
    }

    /// The `sync` folders of two versions of `.azlin`: compared like any folders.
    fn sync_pair(
        &mut self,
        before: &GitId,
        after: &GitId,
        stack: &mut Vec<(GitId, GitId, bool)>,
        into: &mut HashMap<ObjectId, u64>,
    ) -> Result<(), NotYet> {
        let find = |walk: &Walk<'_>, azlin: &GitId| -> Result<Option<GitId>, NotYet> {
            let folder = shard::read_folder(walk.objects, azlin)
                .map_err(|e| NotYet::unreadable(crate::names::BOOKKEEPING, e))?;
            Ok(folder
                .entries()
                .iter()
                .find(|e| e.mode == Mode::Tree && e.name == "sync")
                .map(|e| e.id))
        };
        match (find(self, before)?, find(self, after)?) {
            (Some(was), Some(now)) => stack.push((was, now, false)),
            (Some(was), None) => self.subtree_refs(&was, into)?,
            (None, _) => {}
        }
        Ok(())
    }

    /// The `sync` folder of the drive's `.azlin` folder, if it has one.
    fn bookkeeping(&self, azlin: &GitId, stack: &mut Vec<(GitId, bool)>) -> Result<(), NotYet> {
        let folder = shard::read_folder(self.objects, azlin)
            .map_err(|e| NotYet::unreadable(crate::names::BOOKKEEPING, e))?;
        if let Some(sync) = folder
            .entries()
            .iter()
            .find(|e| e.mode == Mode::Tree && e.name == "sync")
        {
            stack.push((sync.id, false));
        }
        Ok(())
    }

    /// The data object a pointer file names. A pointer that does not read - a newer format, a
    /// damaged one - is not "names nothing": it stops the decision.
    fn pointer(
        &mut self,
        blob: &GitId,
        name: &str,
        into: &mut HashMap<ObjectId, u64>,
    ) -> Result<(), NotYet> {
        if !self.seen_blobs.insert(*blob) {
            return Ok(());
        }
        let bytes = self
            .objects
            .blob(blob)
            .map_err(|e| NotYet::unreadable(name, e))?;
        let entry = pointer::decode(bytes).map_err(|reason| NotYet::unreadable(name, reason))?;
        if let Some(object) = entry.object {
            let stored = object
                .packed
                .as_ref()
                .map_or(object.stored_size, |packed| packed.pack.object_len);
            into.entry(object.bucket_object()).or_insert(stored);
        }
        Ok(())
    }
}

/// What the history of `head` says of the drive's data objects at `now` (the time of day by the
/// caller's clock): the objects the head and the window name, and - unless `live_only` - the
/// objects the old commits (after `after`, at most `caps.commits` of them) named and nothing live
/// does. Pure: it reads `objects` and nothing else.
pub(crate) fn analyse(
    objects: &Objects,
    head: &GitId,
    now: u64,
    retention: &Retention,
    after: Option<&Mark>,
    caps: &Caps,
    live_only: bool,
) -> Result<Analysis, NotYet> {
    let (nodes, order) = load_graph(objects, head, now)?;
    // A retention of nothing keeps no state but the head: no commit is inside the window.
    let window_start = if retention.versions.is_zero() {
        u64::MAX
    } else {
        now.saturating_sub(retention.versions.as_secs())
    };
    let in_window = |id: &GitId| nodes.get(id).is_some_and(|node| node.eff >= window_start);

    // The window's commits, and the old commits it grew out of: the state the window began with.
    let mut window: Vec<GitId> = Vec::new();
    let mut frontier: HashSet<GitId> = HashSet::new();
    let mut old: Vec<GitId> = Vec::new();
    for id in &order {
        if in_window(id) {
            window.push(*id);
            if let Some(node) = nodes.get(id) {
                frontier.extend(node.parents.iter().filter(|parent| !in_window(parent)));
            }
        } else {
            old.push(*id);
        }
    }

    let mut walk = Walk::new(objects);
    let mut live: HashMap<ObjectId, u64> = HashMap::new();
    let head_node = nodes
        .get(head)
        .ok_or_else(|| NotYet::unreadable("the head", "it is not in the history"))?;
    walk.refs(&head_node.tree, &mut live)?;
    let head_names: HashSet<ObjectId> = live.keys().copied().collect();
    for id in window.iter().chain(frontier.iter()) {
        if let Some(node) = nodes.get(id) {
            walk.refs(&node.tree, &mut live)?;
        }
    }

    let mut analysis = Analysis {
        revision: 0,
        now,
        window_start,
        commits: order.len(),
        window_commits: window.len(),
        live,
        head: head_names,
        candidates: BTreeMap::new(),
        scanned: 0,
        cursor: after.copied(),
        stopped_by_cap: false,
    };
    if live_only {
        return Ok(analysis);
    }

    // The old commits, oldest first, from where the last run stopped.
    let mut todo: Vec<Mark> = old
        .iter()
        .filter_map(|id| {
            nodes.get(id).map(|node| Mark {
                time: node.eff,
                id: *id,
            })
        })
        .filter(|mark| after.is_none_or(|done| mark > done))
        .collect();
    todo.sort();
    for mark in todo {
        if analysis.scanned >= caps.commits || analysis.candidates.len() >= caps.objects {
            analysis.stopped_by_cap = true;
            break;
        }
        let Some(node) = nodes.get(&mark.id) else {
            continue;
        };
        // What this commit released: what its parents named and it does not.
        let mut named: HashMap<ObjectId, u64> = HashMap::new();
        for parent in &node.parents {
            if let Some(parent) = nodes.get(parent) {
                walk.dropped(&parent.tree, &node.tree, &mut named)?;
            }
        }
        for (id, bytes) in named {
            if !analysis.live.contains_key(&id) {
                analysis
                    .candidates
                    .entry(id)
                    .or_insert(Candidate { bytes, by: mark.id });
            }
        }
        analysis.scanned += 1;
        analysis.cursor = Some(mark);
    }
    Ok(analysis)
}

impl Analysis {
    /// The proof that `id` may leave the bucket, if the analysis shows it released by an old
    /// commit and named by nothing live.
    pub(crate) fn prove(&self, id: &ObjectId) -> Result<Reclaimable, NotYet> {
        if self.live.contains_key(id) {
            return Err(NotYet::Named);
        }
        if !self.candidates.contains_key(id) {
            return Err(NotYet::NotReleased);
        }
        Ok(Reclaimable {
            id: *id,
            _sealed: (),
        })
    }
}

/// The data objects the head and every state inside the retention window name, read fresh and
/// whole from the drive (an index in memory has none: its head is all there is).
#[derive(Debug)]
pub struct LiveObjects {
    live: HashSet<ObjectId>,
    /// The ones the head alone names (all of `live` for an index that keeps no history).
    head: HashSet<ObjectId>,
    now: u64,
}

impl LiveObjects {
    /// The objects `named` - a head of an index that keeps no history, which is everything there
    /// is to be named.
    pub(crate) fn of(named: HashSet<ObjectId>, now: u64) -> LiveObjects {
        LiveObjects {
            head: named.clone(),
            live: named,
            now,
        }
    }

    /// A drive with no commit yet names nothing.
    pub(crate) fn empty(now: u64) -> LiveObjects {
        LiveObjects {
            live: HashSet::new(),
            head: HashSet::new(),
            now,
        }
    }

    pub(crate) fn from_analysis(analysis: &Analysis) -> LiveObjects {
        LiveObjects {
            live: analysis.live.keys().copied().collect(),
            head: analysis.head.clone(),
            now: analysis.now,
        }
    }

    /// The time of day the window was judged by.
    #[must_use]
    pub fn now(&self) -> u64 {
        self.now
    }

    /// Whether the head or the window names `id`.
    #[must_use]
    pub fn names(&self, id: &ObjectId) -> bool {
        self.live.contains(id)
    }

    /// Whether the head itself names `id`: a version a restore brought back. What only the window
    /// names is held by the history, and is still released as far as the head goes.
    #[must_use]
    pub fn names_in_head(&self, id: &ObjectId) -> bool {
        self.head.contains(id)
    }

    /// The proof that `id` - an object the caller knows was uploaded and may be named by nothing
    /// (an upload whose commit never came, a pack its files all left) - may leave the bucket:
    /// the head and the window do not name it.
    ///
    /// # Errors
    ///
    /// [`NotYet::Named`] when they do.
    pub fn prove_unnamed(&self, id: &ObjectId) -> Result<Reclaimable, NotYet> {
        if self.live.contains(id) {
            return Err(NotYet::Named);
        }
        Ok(Reclaimable {
            id: *id,
            _sealed: (),
        })
    }
}

/// Of the pointer files `blobs` (ids in `repo`'s copy, which must hold them), the ones whose data
/// object is no longer in the bucket: a restore that commits such a pointer names an object that
/// is gone, and the file it replaces - a good one - would read "its object is missing" from then
/// on (AZ-U24: the retention had removed the object). A blob that is no pointer, or one of a
/// newer format, names nothing this can judge and is not reported; a bucket that cannot be asked
/// is an error, never "it is there". Each object is asked once.
///
/// # Errors
///
/// The repository's (a blob the copy does not hold) or the bucket's.
pub fn gone_objects<B: Bucket, S: Sealer>(
    repo: &MetaRepo<B, S>,
    blobs: &[GitId],
) -> Result<Vec<GitId>, MetaError> {
    let mut asked: HashMap<ObjectId, bool> = HashMap::new();
    let mut gone = Vec::new();
    for blob in blobs {
        let bytes = repo.objects().blob(blob)?;
        let Ok(entry) = pointer::decode(bytes) else {
            continue;
        };
        let Some(object) = entry.bucket_object() else {
            continue;
        };
        let there = match asked.get(&object) {
            Some(there) => *there,
            None => {
                let there = repo.store().bucket().exists(&object.bucket_key())?;
                asked.insert(object, there);
                there
            }
        };
        if !there {
            gone.push(*blob);
        }
    }
    Ok(gone)
}

/// What a reclaimer run found and did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReclaimReport {
    pub mode: ReclaimMode,
    /// The commits before this time (seconds since 1970) were older than the retention.
    pub window_start: u64,
    /// The commits reachable from the head, and those inside the window.
    pub commits: usize,
    pub window_commits: usize,
    /// The old commits whose trees were read.
    pub scanned: usize,
    /// The data objects the head and the window name.
    pub live_objects: usize,
    /// The objects the history shows released past the retention, and their bytes in the bucket:
    /// what a run in [`ReclaimMode::Delete`] deletes - in shadow mode, "would delete N objects,
    /// M bytes".
    pub candidates: usize,
    pub candidate_bytes: u64,
    pub deleted: usize,
    pub deleted_bytes: u64,
    /// Deletes the bucket refused (tried again at the next run).
    pub failed: usize,
    /// A cap ended the scan before the old commits did.
    pub capped: bool,
    /// Why the run did not go through, if it did not.
    pub stopped: Option<NotYet>,
}

impl ReclaimReport {
    /// The report of a round that had nothing to read (a drive without an index yet).
    #[must_use]
    pub fn none(mode: ReclaimMode) -> ReclaimReport {
        ReclaimReport::new(mode)
    }

    fn new(mode: ReclaimMode) -> ReclaimReport {
        ReclaimReport {
            mode,
            window_start: 0,
            commits: 0,
            window_commits: 0,
            scanned: 0,
            live_objects: 0,
            candidates: 0,
            candidate_bytes: 0,
            deleted: 0,
            deleted_bytes: 0,
            failed: 0,
            capped: false,
            stopped: None,
        }
    }

    fn take(&mut self, analysis: &Analysis) {
        self.window_start = analysis.window_start;
        self.commits = analysis.commits;
        self.window_commits = analysis.window_commits;
        self.scanned = analysis.scanned;
        self.live_objects = analysis.live.len();
        self.candidates = analysis.candidates.len();
        self.candidate_bytes = analysis.candidates.values().map(|c| c.bytes).sum();
        self.capped = analysis.stopped_by_cap;
    }

    /// The report as one line: what the log says.
    #[must_use]
    pub fn line(&self) -> String {
        let how = match self.mode {
            ReclaimMode::Off => return String::from("reclaim: switched off"),
            ReclaimMode::Shadow => "reclaim (shadow): would delete",
            ReclaimMode::Delete => "reclaim: deleted",
        };
        let (count, bytes) = if self.mode == ReclaimMode::Delete {
            (self.deleted, self.deleted_bytes)
        } else {
            (self.candidates, self.candidate_bytes)
        };
        let mut line = format!(
            "{how} {count} objects, {bytes} bytes ({} commits, {} in the window, {} read, {} \
             objects live)",
            self.commits, self.window_commits, self.scanned, self.live_objects
        );
        if self.failed > 0 {
            line.push_str(&format!(", {} not deleted", self.failed));
        }
        if self.capped {
            line.push_str(", stopped at a cap");
        }
        if let Some(why) = &self.stopped {
            line.push_str(&format!(", stopped: {why}"));
        }
        line
    }
}

/// One run of the reclaimer over `repo`'s drive. The copy is read fresh and whole first. In
/// [`ReclaimMode::Shadow`] that is all: the report says what would go. In [`ReclaimMode::Delete`]
/// the run takes the maintenance lease, checks the bucket's clock against this device's, and for
/// each batch of proven objects (1) fences - the manifest is swapped, and only while it is still
/// the revision the reads were made at, so nobody published since -, (2) deletes, and when every
/// delete went through (3) records how far it got. A delete the bucket refuses stops the mark: the
/// next run finds the object again and tries again. Nothing is deleted from reads that are old.
///
/// # Errors
///
/// The repository's or the bucket's, for a run that could not even read the drive; a run that
/// stops on a decision (a clock, a lease, a drive that moved) is a report with `stopped`.
pub fn reclaim<B: Bucket, S: Sealer>(
    repo: &mut MetaRepo<B, S>,
    options: &ReclaimOptions,
) -> Result<ReclaimReport, MetaError> {
    let mut report = ReclaimReport::new(options.mode);
    if options.mode == ReclaimMode::Off {
        report.stopped = Some(NotYet::Off);
        return Ok(report);
    }
    repo.pull()?;
    repo.fetch_all()?;
    let Some(head) = repo.head() else {
        return Ok(report);
    };
    let now = repo.store().now();
    let revision = repo.store().state().revision;
    let after = repo
        .store()
        .manifest()
        .and_then(|manifest| manifest.reclaimed.as_ref())
        .and_then(Mark::from_wal)
        .filter(|_| options.use_mark);
    let mut analysis = match analyse(
        repo.objects(),
        &head,
        now,
        &options.retention,
        after.as_ref(),
        &options.caps,
        false,
    ) {
        Ok(analysis) => analysis,
        Err(why) => {
            report.stopped = Some(why);
            return Ok(report);
        }
    };
    analysis.revision = revision;
    report.take(&analysis);
    if options.mode == ReclaimMode::Shadow {
        return Ok(report);
    }
    let mut guard = match repo.store_mut().acquire_lease(MAINTENANCE, LEASE_SECS) {
        Ok(guard) => guard,
        Err(MetaError::LeaseHeld { .. }) => {
            report.stopped = Some(NotYet::Lease);
            return Ok(report);
        }
        Err(e) => return Err(e),
    };
    let done = delete_under(
        repo,
        &mut guard,
        &analysis,
        after.as_ref(),
        options,
        &mut report,
    );
    let released = repo.store_mut().release_lease(guard);
    done?;
    released?;
    Ok(report)
}

/// The deleting half of [`reclaim`], under the lease.
fn delete_under<B: Bucket, S: Sealer>(
    repo: &mut MetaRepo<B, S>,
    guard: &mut LeaseGuard,
    analysis: &Analysis,
    after: Option<&Mark>,
    options: &ReclaimOptions,
    report: &mut ReclaimReport,
) -> Result<(), MetaError> {
    if let Err(why) = check_clock(repo, guard, analysis.now, options.server_clock) {
        report.stopped = Some(why);
        return Ok(());
    }
    let ids: Vec<ObjectId> = analysis.candidates.keys().copied().collect();
    let mut revision = analysis.revision;
    for batch in ids.chunks(options.caps.batch.max(1)) {
        repo.store_mut().renew_lease(guard, LEASE_SECS)?;
        // The fence: the manifest is still the revision the reads were made at.
        revision = match repo.store_mut().stamp(guard, revision, None) {
            Ok(revision) => revision,
            Err(MetaError::Conflict { .. } | MetaError::OutcomeUnknown { .. }) => {
                report.stopped = Some(NotYet::Moved);
                return Ok(());
            }
            Err(e) => return Err(e),
        };
        for id in batch {
            let proof = match analysis.prove(id) {
                Ok(proof) => proof,
                Err(why) => {
                    report.stopped = Some(why);
                    return Ok(());
                }
            };
            match remove_reclaimable(repo.store().bucket(), ReclaimMode::Delete, proof) {
                Ok(_) => {
                    report.deleted += 1;
                    report.deleted_bytes += analysis.candidates.get(id).map_or(0, |c| c.bytes);
                }
                Err(_) => report.failed += 1,
            }
        }
    }
    // Every delete went through: the mark moves, so the next run starts after these commits.
    if report.failed == 0 && options.use_mark {
        if let Some(cursor) = analysis.cursor.filter(|cursor| after != Some(cursor)) {
            let mark = cursor.to_wal(repo.store().now());
            match repo.store_mut().stamp(guard, revision, Some(mark)) {
                Ok(_) => {}
                Err(MetaError::Conflict { .. } | MetaError::OutcomeUnknown { .. }) => {
                    report.stopped = Some(NotYet::Moved);
                }
                Err(e) => return Err(e),
            }
        }
    }
    Ok(())
}

/// The bucket's clock and this device's agree: the lease this run took was written just now, and
/// the bucket says when.
fn check_clock<B: Bucket, S: Sealer>(
    repo: &MetaRepo<B, S>,
    guard: &LeaseGuard,
    now: u64,
    policy: ServerClock,
) -> Result<(), NotYet> {
    if policy == ServerClock::Unchecked {
        return Ok(());
    }
    let key = super::keys::lease(&guard.purpose);
    let server = repo.store().bucket().modified_of(&key).ok().flatten();
    match server {
        Some(server) if server.abs_diff(now) <= SKEW_SECS => Ok(()),
        _ => Err(NotYet::Clock {
            device: now,
            server,
        }),
    }
}
