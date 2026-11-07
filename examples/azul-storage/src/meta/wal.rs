//! The write-ahead log of the metadata repository in the bucket: walgit's
//! protocol, blocking, every object sealed.
//!
//! # Objects
//!
//! - **Manifest** (`.azlin/meta/manifest`): the head sequence number, the latest
//!   checkpoint, the log entries after it, the live packs, the objects waiting to be
//!   deleted, and a revision counter. Replaced only by a compare-and-swap on its version:
//!   the one place where devices agree.
//! - **Log entry** (`log/<seq>-<attempt>`): one per publish, immutable: the pack it added
//!   and the ref updates (Push), or the packs a compaction replaced (Compact). The random
//!   attempt id keeps two devices' attempts at the same `seq` apart, so a loser can
//!   remove its own entry and nothing else.
//! - **Pack** (`wal/<name>.pack` + `.idx`, see [`super::pack`]): immutable.
//! - **Checkpoint** (`checkpoints/<seq>-<attempt>`): the refs and the live packs at
//!   `seq`. A device that is behind it starts from it and replays the log after it.
//! - **Lease** (`leases/<purpose>`): who may compact and collect garbage, until when.
//!
//! # Publishing
//!
//! 1. Poll: a conditional GET of the manifest; read the new log entries.
//! 2. The caller builds the publish from the fresh state (and merges there).
//! 3. Write the pack (create-only; an existing one of the same name holds the same
//!    objects), then the log entry `head + 1` (create-only).
//! 4. Swap the manifest from the version polled in 1. Lost (412): remove the own log
//!    entry, poll again, rebuild, retry. Any other error is ambiguous (the swap may have
//!    landed): a fresh read decides by whether the manifest lists the own entry.
//!
//! A pack left behind by a lost swap is not removed: its name could be another
//! device's (the same objects), and a later attempt usually needs its objects
//! again. Orphans are for a sweep over the listing (later; maintenance only).
//!
//! # Sealing
//!
//! Every object is sealed by the [`Sealer`] with its own key in the bucket as the
//! context: the bucket cannot answer one object with another. It can answer with an
//! older manifest; the revision counter catches that against what this device has
//! seen ([`MetaError::Rollback`]).

use std::collections::{BTreeMap, HashSet};

use serde::{de::DeserializeOwned, Deserialize, Serialize};

use super::{
    bucket::{Bucket, Fetched, Version},
    certainty::Certainty,
    keys,
    objects::{ObjectId, Objects},
    pack::{PackIndex, PackWriter, SealedPack},
    MetaError, SealError, Sealer,
};

/// The manifest format this code writes and reads.
pub const FORMAT: u32 = 1;

/// The git object format of every repository this code writes.
pub const OBJECT_FORMAT: &str = "sha256";

/// Publish attempts before [`MetaError::Contended`].
pub const DEFAULT_ATTEMPTS: u32 = 16;

/// The name of the lease for compaction and garbage collection.
pub const MAINTENANCE: &str = "maintenance";

/// A pack the repository holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackRef {
    /// The keyed-hash name: the objects are `wal/<name>.pack` and `.idx`.
    pub name: String,
    pub objects: u32,
    pub pack_size: u64,
    pub idx_size: u64,
    /// The log entry that added it.
    pub seq: u64,
}

/// Where one log entry is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogRef {
    pub seq: u64,
    pub key: String,
}

/// Where the latest checkpoint is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckpointRef {
    pub seq: u64,
    pub key: String,
    /// Seconds since 1970.
    pub created_at: u64,
}

/// An object no longer referenced, deleted by garbage collection once it has
/// been retired long enough for every reader to be done with it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Retired {
    pub key: String,
    /// Seconds since 1970.
    pub at: u64,
}

/// How far the reclaimer has been ([`super::reclaim`]): every data object that a commit up to this
/// mark released has been dealt with - deleted, or found to be named again. A cursor, not a
/// ledger: the reclaimer derives what was released from the commits themselves, so a manifest
/// that lost the mark (an older client wrote it) only makes the next run start from the
/// beginning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReclaimMark {
    /// The newest commit dealt with: its effective time (seconds since 1970) ...
    pub time: u64,
    /// ... and its id (hex); the pair is the mark, in this order.
    pub commit: String,
    /// When the mark was set, by the device's clock.
    pub at: u64,
}

/// The manifest: the repository's current state, in one small object.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    pub format: u32,
    pub object_format: String,
    /// The last log entry's sequence number (0: nothing published yet).
    pub head_seq: u64,
    /// Counts the manifest's successful writes; never goes back.
    pub revision: u64,
    #[serde(default)]
    pub checkpoint: Option<CheckpointRef>,
    /// The log entries after the checkpoint, ascending and without a gap, up to `head_seq`.
    #[serde(default)]
    pub log: Vec<LogRef>,
    /// The live packs: everything a device needs.
    #[serde(default)]
    pub packs: Vec<PackRef>,
    #[serde(default)]
    pub retired: Vec<Retired>,
    /// How far the reclaimer got ([`ReclaimMark`]). Optional and unknown to older clients, which
    /// drop it when they write the manifest: `format` stays 1, and nothing depends on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reclaimed: Option<ReclaimMark>,
    /// The device that wrote this revision.
    pub writer: String,
    /// Seconds since 1970.
    pub updated_at: u64,
}

/// What a log entry did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EntryKind {
    /// Added a pack (or none) and moved refs.
    Push,
    /// Replaced packs by one with the same objects; refs unchanged.
    Compact,
}

/// One ref update of a publish. `None` = the ref is absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RefUpdate {
    /// `refs/heads/main`.
    pub name: String,
    pub old: Option<String>,
    pub new: Option<String>,
}

/// One entry of the log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogEntry {
    pub seq: u64,
    pub kind: EntryKind,
    #[serde(default)]
    pub pack: Option<PackRef>,
    #[serde(default)]
    pub updates: Vec<RefUpdate>,
    /// Compact: the names of the packs it replaced.
    #[serde(default)]
    pub supersedes: Vec<String>,
    pub writer: String,
    /// Seconds since 1970.
    pub created_at: u64,
    #[serde(default)]
    pub message: String,
}

/// The refs and the live packs at `seq`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub seq: u64,
    pub refs: BTreeMap<String, String>,
    pub packs: Vec<PackRef>,
    pub writer: String,
    pub created_at: u64,
}

/// Who holds a lease, and until when.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lease {
    pub holder: String,
    pub purpose: String,
    /// Seconds since 1970; 0 = released.
    pub expires_at: u64,
}

/// A lease this device holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeaseGuard {
    key: String,
    version: Option<Version>,
    pub purpose: String,
    pub expires_at: u64,
}

/// The repository as a device knows it: the refs after `head_seq`, and the live packs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepoState {
    pub head_seq: u64,
    pub revision: u64,
    pub refs: BTreeMap<String, String>,
    pub packs: Vec<PackRef>,
}

/// What a device keeps of its store between runs ([`MetaStore::snapshot`],
/// [`MetaStore::resume`]): the manifest it last saw and its version (so the
/// next poll is a conditional read), the state, and the highest revision seen
/// (so an older manifest is refused after a restart too).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoreSnapshot {
    #[serde(default)]
    pub manifest: Option<Manifest>,
    #[serde(default)]
    pub version: Option<Version>,
    pub state: RepoState,
    pub seen_revision: u64,
}

/// What a poll found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SyncReport {
    /// Whether the manifest changed.
    pub changed: bool,
    /// Whether the state was rebuilt from a checkpoint (the device was behind it).
    pub from_checkpoint: bool,
    /// The log entries applied, in order.
    pub entries: Vec<LogEntry>,
}

/// What a device wants to publish, built from the fresh [`RepoState`].
#[derive(Debug, Clone, Default)]
pub struct Publish {
    /// The new objects; `None` or empty for a ref-only change.
    pub pack: Option<PackWriter>,
    pub updates: Vec<RefUpdate>,
    pub message: String,
}

/// A publish that landed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Published {
    pub seq: u64,
    pub pack: Option<PackRef>,
    /// Attempts it took (1 = no other device came first).
    pub attempts: u32,
}

/// What a compaction did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Compacted {
    pub seq: u64,
    pub pack: PackRef,
    pub replaced: Vec<String>,
}

/// The most bytes of packs one compaction folds into one new pack (the sum of the packs it
/// replaces): three quarters of the largest index object a frozen drive takes in one PUT
/// ([`azlin_contract::INDEX_OBJECT_MAX`], 64 MiB), so that the compacted pack is an object the
/// drive's node still takes when the drive is unpaid, and a compaction never holds more than this
/// in memory (AZ-M5).
pub const COMPACT_PACK_MAX: u64 = azlin_contract::INDEX_OBJECT_MAX / 4 * 3;

/// When a maintenance round ([`MetaStore::maintain`]) does what.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Maintenance {
    /// How long the round may hold the lease (seconds). The round extends it before the steps
    /// that need it, so a long round keeps it.
    pub lease_secs: u64,
    /// Fold the live packs from this many on.
    pub compact_at_packs: usize,
    /// The most bytes of packs a compaction folds into one pack ([`COMPACT_PACK_MAX`]): a drive
    /// whose index is bigger keeps several packs, none bigger than this.
    pub compact_max_bytes: u64,
    /// Write a checkpoint from this many log entries after the last one on.
    pub checkpoint_at_entries: usize,
    /// How old an object no manifest names must be before the sweep retires it (seconds).
    pub orphan_age: u64,
    /// How long a retired object waits before it is deleted (seconds).
    pub grace: u64,
}

impl Default for Maintenance {
    /// Ten minutes of lease; 16 packs of at most [`COMPACT_PACK_MAX`] folded; 64 log entries; a
    /// day for orphans and for the grace.
    fn default() -> Self {
        Maintenance {
            lease_secs: 600,
            compact_at_packs: 16,
            compact_max_bytes: COMPACT_PACK_MAX,
            checkpoint_at_entries: 64,
            orphan_age: 86_400,
            grace: 86_400,
        }
    }
}

/// What a maintenance round did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Maintained {
    /// The objects no manifest named, retired.
    pub swept: usize,
    pub compacted: bool,
    pub checkpointed: bool,
    /// The retired objects deleted.
    pub collected: usize,
}

/// Whether the steps of a maintenance round that are left are worth trying after a step failed
/// with `e`: the storage did not answer, or another device was quicker. A lease that was lost, a
/// repository that does not open or read right, or one that went back in time end the round.
fn round_goes_on(e: &MetaError) -> bool {
    matches!(
        e,
        MetaError::Drive(_)
            | MetaError::Raced { .. }
            | MetaError::Conflict { .. }
            | MetaError::OutcomeUnknown { .. }
            | MetaError::Contended { .. }
    )
}

fn sealed_error(key: &str, e: &SealError) -> MetaError {
    MetaError::Sealed {
        key: key.to_string(),
        reason: e.reason.clone(),
    }
}

fn corrupt(key: &str, reason: impl Into<String>) -> MetaError {
    MetaError::Corrupt {
        key: key.to_string(),
        reason: reason.into(),
    }
}

/// `value` as JSON, sealed by `sealer` with the bucket key `key` as context.
fn seal_json<T: Serialize, S: Sealer + ?Sized>(
    sealer: &S,
    key: &str,
    value: &T,
) -> Result<Vec<u8>, MetaError> {
    let plain = serde_json::to_vec(value).map_err(|e| corrupt(key, e.to_string()))?;
    sealer
        .seal(key.as_bytes(), &plain)
        .map_err(|e| sealed_error(key, &e))
}

/// The JSON value `sealed` holds, opened by `sealer` with the bucket key `key`.
fn open_json<T: DeserializeOwned, S: Sealer + ?Sized>(
    sealer: &S,
    key: &str,
    sealed: &[u8],
) -> Result<T, MetaError> {
    let plain = sealer
        .open(key.as_bytes(), sealed)
        .map_err(|e| sealed_error(key, &e))?;
    serde_json::from_slice(&plain).map_err(|e| corrupt(key, e.to_string()))
}

/// Whether a sealed pack of `pack` bytes with an index of `idx` bytes is one a reader takes: neither
/// is more than an index object may weigh ([`crate::limits::named::INDEX_OBJECT`]).
pub(crate) fn check_writable(pack: usize, idx: usize) -> Result<(), MetaError> {
    let limit = crate::limits::named::INDEX_OBJECT;
    for (what, len) in [("pack", pack), ("index", idx)] {
        if len as u64 > limit.max {
            return Err(MetaError::Unsupported(format!(
                "a {what} of the drive index of {len} bytes is more than {} may be ({} bytes): the \
                 change names too many files at once",
                limit.name, limit.max
            )));
        }
    }
    Ok(())
}

/// Writes a sealed pack and its index (create-only; one of the same name holds
/// the same objects) and says what a manifest holds of it. A pack or an index that is more than
/// an index object may weigh ([`crate::limits::named::INDEX_OBJECT`]) is not written: no reader
/// would take it (the check of the read side refuses it), so a drive that stored it would not
/// open - the write fails instead, and says why.
fn put_sealed_pack<B: Bucket + ?Sized>(
    bucket: &B,
    sealed: SealedPack,
    seq: u64,
) -> Result<PackRef, MetaError> {
    check_writable(sealed.pack.len(), sealed.idx.len())?;
    for (key, bytes) in [
        (keys::pack(&sealed.name), &sealed.pack),
        (keys::idx(&sealed.name), &sealed.idx),
    ] {
        match bucket.create(&key, bytes) {
            Ok(_) | Err(MetaError::Conflict { .. }) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(PackRef {
        name: sealed.name,
        objects: sealed.objects,
        pack_size: sealed.pack.len() as u64,
        idx_size: sealed.idx.len() as u64,
        seq,
    })
}

/// Waits a little before the next attempt after a 409 (another conditional
/// write of the object was in progress): 1 to 2^attempt ms, at most ~64 ms,
/// at random, so racing devices do not meet again.
fn back_off(attempt: u32) {
    let ceiling = 1u64 << attempt.min(6);
    let millis = 1 + crate::ids::random_seed() % ceiling;
    std::thread::sleep(std::time::Duration::from_millis(millis));
}

/// 128 random bits as 32 hex digits: an attempt id.
fn attempt_id() -> String {
    format!(
        "{:016x}{:016x}",
        crate::ids::random_seed(),
        crate::ids::random_seed()
    )
}

/// Reads a repository's packs (also while a publish is being built).
pub struct Packs<'a, B: Bucket, S: Sealer> {
    bucket: &'a B,
    sealer: &'a S,
}

impl<B: Bucket, S: Sealer> Packs<'_, B, S> {
    /// Opens the index of `pack`.
    pub fn index(&self, pack: &PackRef) -> Result<PackIndex, MetaError> {
        PackIndex::open(self.sealer, &pack.name, &self.idx_bytes(pack)?)
    }

    /// Reads every object of `pack` into `objects`, each checked against its
    /// id; the ids the pack holds.
    pub fn fetch(&self, pack: &PackRef, objects: &mut Objects) -> Result<Vec<ObjectId>, MetaError> {
        let (idx, bytes) = self.download(pack)?;
        load_pack(self.sealer, &pack.name, &idx, &bytes, objects)
    }

    /// The sealed `.idx` and `.pack` of `pack`, as the bucket holds them (a
    /// device's cache keeps them so, encrypted).
    pub fn download(&self, pack: &PackRef) -> Result<(Vec<u8>, Vec<u8>), MetaError> {
        Ok((self.idx_bytes(pack)?, self.pack_bytes(pack)?))
    }

    /// The sealed `.idx` of `pack`.
    pub fn idx_bytes(&self, pack: &PackRef) -> Result<Vec<u8>, MetaError> {
        self.object(&keys::idx(&pack.name))
    }

    /// The sealed `.pack` of `pack`.
    pub fn pack_bytes(&self, pack: &PackRef) -> Result<Vec<u8>, MetaError> {
        self.object(&keys::pack(&pack.name))
    }

    fn object(&self, key: &str) -> Result<Vec<u8>, MetaError> {
        self.bucket
            .read(key)?
            .map(|(bytes, _)| bytes)
            .ok_or_else(|| corrupt(key, "the manifest names it, but it is missing"))
    }

    /// The sealer that opens the packs.
    #[must_use]
    pub fn sealer(&self) -> &S {
        self.sealer
    }

    /// The sealed bytes of chunk `chunk` of `pack`: one ranged GET (C6).
    pub fn read_chunk(
        &self,
        pack: &PackRef,
        index: &PackIndex,
        chunk: u32,
    ) -> Result<Vec<u8>, MetaError> {
        let key = keys::pack(&pack.name);
        let range = index
            .chunk_range(chunk)
            .ok_or_else(|| corrupt(&key, "a chunk the pack does not have"))?;
        self.bucket.read_range(&key, range)
    }
}

/// Opens the sealed `idx` and reads every object of the sealed `pack` of the
/// pack called `name` into `objects`, each checked; the ids it holds.
pub fn load_pack(
    sealer: &dyn Sealer,
    name: &str,
    idx: &[u8],
    pack: &[u8],
    objects: &mut Objects,
) -> Result<Vec<ObjectId>, MetaError> {
    let index = PackIndex::open(sealer, name, idx)?;
    index.read_into(sealer, pack, objects)?;
    Ok(index.entries().iter().map(|e| e.id).collect())
}

/// The manifest as this device last read or wrote it.
struct Synced {
    manifest: Manifest,
    /// `None` when the bucket did not tell the version of what was written: the next
    /// poll reads the manifest whole.
    version: Option<Version>,
}

/// One device's handle on the repository in a bucket. Every call blocks.
pub struct MetaStore<B: Bucket, S: Sealer> {
    bucket: B,
    sealer: S,
    device: String,
    clock: Box<dyn Fn() -> u64 + Send + Sync>,
    attempts: u32,
    synced: Option<Synced>,
    state: RepoState,
    /// The highest manifest revision this device has seen.
    seen_revision: u64,
}

impl<B: Bucket, S: Sealer> MetaStore<B, S> {
    fn new(bucket: B, sealer: S, device: &str) -> Self {
        MetaStore {
            bucket,
            sealer,
            device: device.to_string(),
            clock: Box::new(crate::time::now_unix),
            attempts: DEFAULT_ATTEMPTS,
            synced: None,
            state: RepoState::default(),
            seen_revision: 0,
        }
    }

    /// Creates the repository in `bucket`; [`MetaError::RepositoryExists`] when
    /// there is one. `device` names this device in what it writes.
    pub fn create(bucket: B, sealer: S, device: &str) -> Result<Self, MetaError> {
        let mut store = MetaStore::new(bucket, sealer, device);
        store.init()?;
        Ok(store)
    }

    /// Writes the first manifest (create-only).
    fn init(&mut self) -> Result<(), MetaError> {
        let manifest = Manifest {
            format: FORMAT,
            object_format: OBJECT_FORMAT.to_string(),
            head_seq: 0,
            revision: 1,
            checkpoint: None,
            log: Vec::new(),
            packs: Vec::new(),
            retired: Vec::new(),
            reclaimed: None,
            writer: self.device.clone(),
            updated_at: self.now(),
        };
        let sealed = self.seal(keys::MANIFEST, &manifest)?;
        let version = match self.bucket.create(keys::MANIFEST, &sealed) {
            Ok(version) => version,
            Err(MetaError::Conflict { .. }) => return Err(MetaError::RepositoryExists),
            Err(e) => return Err(e),
        };
        self.seen_revision = manifest.revision;
        self.state.revision = manifest.revision;
        self.synced = Some(Synced { manifest, version });
        Ok(())
    }

    /// Opens the repository in `bucket` and reads it up to its head;
    /// [`MetaError::NoRepository`] when there is none.
    pub fn open(bucket: B, sealer: S, device: &str) -> Result<Self, MetaError> {
        let mut store = MetaStore::new(bucket, sealer, device);
        store.sync()?;
        Ok(store)
    }

    /// [`MetaStore::open`] for a device that knows the repository was at revision `floor`
    /// already (its copy of it, or a record of it): a manifest older than that is
    /// [`MetaError::Rollback`], as for a device that has been in the repository all along. A
    /// device that remembers nothing accepts whatever the bucket serves (`floor` 0).
    pub fn open_above(bucket: B, sealer: S, device: &str, floor: u64) -> Result<Self, MetaError> {
        let mut store = MetaStore::new(bucket, sealer, device);
        store.seen_revision = floor;
        store.sync()?;
        Ok(store)
    }

    /// The highest manifest revision this device has seen: what a bucket must not serve less than.
    #[must_use]
    pub fn seen_revision(&self) -> u64 {
        self.seen_revision
    }

    /// Forgets everything this device knows of the repository - the manifest and its version, the
    /// state and the highest revision seen: the next poll reads the manifest whole and takes the
    /// one the bucket serves, as a device new to the drive does. Only for a restore of the drive
    /// that the token server announced ([`super::repo::MetaRepo::pull`]); a bucket that serves an
    /// older manifest nobody announced stays [`MetaError::Rollback`].
    pub(super) fn forget_state(&mut self) {
        self.synced = None;
        self.state = RepoState::default();
        self.seen_revision = 0;
    }

    /// Opens the repository, or creates it when the bucket has none (and opens
    /// the one another device created in the same moment).
    pub fn open_or_create(bucket: B, sealer: S, device: &str) -> Result<Self, MetaError> {
        let mut store = MetaStore::new(bucket, sealer, device);
        match store.sync() {
            Ok(_) => {}
            Err(MetaError::NoRepository) => match store.init() {
                Ok(()) => {}
                Err(MetaError::RepositoryExists) => {
                    store.sync()?;
                }
                Err(e) => return Err(e),
            },
            Err(e) => return Err(e),
        }
        Ok(store)
    }

    /// What this device keeps of the store between runs.
    #[must_use]
    pub fn snapshot(&self) -> StoreSnapshot {
        StoreSnapshot {
            manifest: self.synced.as_ref().map(|s| s.manifest.clone()),
            version: self.synced.as_ref().and_then(|s| s.version.clone()),
            state: self.state.clone(),
            seen_revision: self.seen_revision,
        }
    }

    /// The store as [`MetaStore::snapshot`] left it, without a request: the
    /// next [`MetaStore::sync`] is one conditional read.
    pub fn resume(bucket: B, sealer: S, device: &str, snapshot: StoreSnapshot) -> Self {
        let mut store = MetaStore::new(bucket, sealer, device);
        store.synced = snapshot.manifest.map(|manifest| Synced {
            manifest,
            version: snapshot.version,
        });
        store.state = snapshot.state;
        store.seen_revision = snapshot.seen_revision;
        store
    }

    /// Seconds since 1970 by this store's clock.
    #[must_use]
    pub fn now(&self) -> u64 {
        (self.clock)()
    }

    /// Sets the clock (seconds since 1970): the tests' time.
    #[must_use]
    pub fn with_clock(mut self, clock: impl Fn() -> u64 + Send + Sync + 'static) -> Self {
        self.clock = Box::new(clock);
        self
    }

    /// Sets how many publish attempts are made before giving up.
    #[must_use]
    pub fn with_attempts(mut self, attempts: u32) -> Self {
        self.attempts = attempts.max(1);
        self
    }

    #[must_use]
    pub fn bucket(&self) -> &B {
        &self.bucket
    }

    #[must_use]
    pub fn sealer(&self) -> &S {
        &self.sealer
    }

    #[must_use]
    pub fn device(&self) -> &str {
        &self.device
    }

    /// The state after the last poll or publish.
    #[must_use]
    pub fn state(&self) -> &RepoState {
        &self.state
    }

    /// The manifest after the last poll or publish.
    #[must_use]
    pub fn manifest(&self) -> Option<&Manifest> {
        self.synced.as_ref().map(|s| &s.manifest)
    }

    // ---- sealing ----

    fn seal<T: Serialize>(&self, key: &str, value: &T) -> Result<Vec<u8>, MetaError> {
        seal_json(&self.sealer, key, value)
    }

    fn open_sealed<T: DeserializeOwned>(&self, key: &str, sealed: &[u8]) -> Result<T, MetaError> {
        open_json(&self.sealer, key, sealed)
    }

    fn read_sealed<T: DeserializeOwned>(&self, key: &str) -> Result<T, MetaError> {
        match self.bucket.read(key)? {
            Some((bytes, _)) => self.open_sealed(key, &bytes),
            None => Err(corrupt(key, "the manifest names it, but it is missing")),
        }
    }

    // ---- polling ----

    /// Brings the state up to the bucket's manifest: one conditional GET when
    /// nothing changed; else the new log entries (or the checkpoint and the log
    /// after it, for a device behind the checkpoint).
    pub fn sync(&mut self) -> Result<SyncReport, MetaError> {
        let known = self.synced.as_ref().and_then(|s| s.version.clone());
        let (bytes, version) = match known {
            Some(known) => match self.bucket.read_if_changed(keys::MANIFEST, &known)? {
                Fetched::NotModified => return Ok(SyncReport::default()),
                Fetched::Missing => return Err(MetaError::NoRepository),
                Fetched::Changed { bytes, version } => (bytes, Some(version)),
            },
            None => match self.bucket.read(keys::MANIFEST)? {
                None => return Err(MetaError::NoRepository),
                Some((bytes, version)) => (bytes, Some(version)),
            },
        };
        let manifest: Manifest = self.open_sealed(keys::MANIFEST, &bytes)?;
        self.adopt(manifest, version)
    }

    /// Checks `manifest` and moves the state to it.
    fn adopt(&mut self, manifest: Manifest, version: Option<Version>) -> Result<SyncReport, MetaError> {
        check_manifest(&manifest)?;
        if manifest.revision < self.seen_revision {
            return Err(MetaError::Rollback {
                seen: self.seen_revision,
                served: manifest.revision,
            });
        }
        let unchanged = self
            .synced
            .as_ref()
            .is_some_and(|s| s.manifest == manifest);
        let mut report = SyncReport {
            changed: !unchanged,
            ..SyncReport::default()
        };
        if manifest.head_seq < self.state.head_seq {
            return Err(corrupt(
                keys::MANIFEST,
                format!(
                    "head {} is behind this device's {}",
                    manifest.head_seq, self.state.head_seq
                ),
            ));
        }
        let first_in_log = manifest.checkpoint.as_ref().map_or(0, |c| c.seq) + 1;
        let mut state = self.state.clone();
        if manifest.head_seq > state.head_seq && state.head_seq + 1 < first_in_log {
            // Behind the checkpoint: start from it.
            let cp_ref = manifest
                .checkpoint
                .clone()
                .ok_or_else(|| corrupt(keys::MANIFEST, "a log that does not start at 1"))?;
            let checkpoint: Checkpoint = self.read_sealed(&cp_ref.key)?;
            if checkpoint.seq != cp_ref.seq {
                return Err(corrupt(&cp_ref.key, "a checkpoint of another sequence number"));
            }
            state.head_seq = checkpoint.seq;
            state.refs = checkpoint.refs;
            report.from_checkpoint = true;
        }
        let from = state.head_seq;
        for log_ref in manifest.log.iter().filter(|r| r.seq > from) {
            let entry: LogEntry = self.read_sealed(&log_ref.key)?;
            if entry.seq != log_ref.seq {
                return Err(corrupt(&log_ref.key, "a log entry of another sequence number"));
            }
            apply(&mut state, &entry, &log_ref.key)?;
            report.entries.push(entry);
        }
        state.packs = manifest.packs.clone();
        state.revision = manifest.revision;
        self.seen_revision = manifest.revision;
        self.state = state;
        self.synced = Some(Synced { manifest, version });
        Ok(report)
    }

    // ---- packs ----

    /// Writes the sealed pack and its index (create-only; one of the same name
    /// is the same objects) and says what the manifest will hold of it. The ordinary name of a
    /// pack is the keyed hash of its objects, so the same objects meet the same keys: when one of
    /// them is listed as retired - a sweep took the pack for an orphan, and a garbage collection
    /// is deleting it, or will - the writer does not take the object for its own, which would let
    /// the collection delete a pack the manifest names the moment the writer's swap lands: it gives
    /// the pack another name and uploads it anew (AZ-M9).
    fn put_pack(&self, writer: &PackWriter, seq: u64) -> Result<PackRef, MetaError> {
        let ordinary = writer.name(&self.sealer);
        let burned = self.synced.as_ref().is_some_and(|synced| {
            synced
                .manifest
                .retired
                .iter()
                .any(|r| r.key == keys::pack(&ordinary) || r.key == keys::idx(&ordinary))
        });
        let name = if burned {
            writer.salted_name(&self.sealer, attempt_id().as_bytes())
        } else {
            ordinary
        };
        put_sealed_pack(&self.bucket, writer.seal_as(&self.sealer, &name)?, seq)
    }

    /// Opens the index of `pack`.
    pub fn pack_index(&self, pack: &PackRef) -> Result<PackIndex, MetaError> {
        self.packs().index(pack)
    }

    /// Reads every object of `pack` into `objects`, each checked against its id.
    pub fn fetch_pack(&self, pack: &PackRef, objects: &mut Objects) -> Result<(), MetaError> {
        self.packs().fetch(pack, objects).map(|_| ())
    }

    /// The repository's packs, to read.
    #[must_use]
    pub fn packs(&self) -> Packs<'_, B, S> {
        Packs {
            bucket: &self.bucket,
            sealer: &self.sealer,
        }
    }

    // ---- publishing ----

    /// Publishes what `build` makes of the fresh state: a pack, ref updates.
    /// `build` runs once per attempt, after the poll that brought the other
    /// devices' entries (this is where the caller merges); `Ok(None)` from it
    /// publishes nothing. On a lost swap the attempt's log entry is removed and
    /// the next attempt starts with a poll.
    pub fn publish<F>(&mut self, mut build: F) -> Result<Option<Published>, MetaError>
    where
        F: FnMut(&RepoState) -> Result<Option<Publish>, MetaError>,
    {
        self.publish_with(|state, _| build(state))
    }

    /// [`MetaStore::publish`] whose `build` can also read the packs the fresh
    /// state names (the other devices' objects, to merge with).
    pub fn publish_with<F>(&mut self, mut build: F) -> Result<Option<Published>, MetaError>
    where
        F: FnMut(&RepoState, &Packs<'_, B, S>) -> Result<Option<Publish>, MetaError>,
    {
        for attempt in 1..=self.attempts {
            self.sync()?;
            let packs = Packs {
                bucket: &self.bucket,
                sealer: &self.sealer,
            };
            let Some(publish) = build(&self.state, &packs)? else {
                return Ok(None);
            };
            for update in &publish.updates {
                let actual = self.state.refs.get(&update.name).cloned();
                if actual != update.old {
                    return Err(MetaError::RefConflict {
                        name: update.name.clone(),
                        expected: update.old.clone(),
                        actual,
                    });
                }
            }
            let seq = self.state.head_seq + 1;
            let pack = match &publish.pack {
                Some(writer) if !writer.is_empty() => match self.put_pack(writer, seq) {
                    Ok(pack) => Some(pack),
                    Err(MetaError::Raced { .. }) => {
                        back_off(attempt);
                        continue;
                    }
                    Err(e) => return Err(e),
                },
                _ => None,
            };
            let entry = LogEntry {
                seq,
                kind: EntryKind::Push,
                pack: pack.clone(),
                updates: publish.updates.clone(),
                supersedes: Vec::new(),
                writer: self.device.clone(),
                created_at: self.now(),
                message: publish.message.clone(),
            };
            let committed = self.commit_entry(&entry, |manifest| {
                if let Some(pack) = &pack {
                    manifest.packs.push(pack.clone());
                }
            });
            match committed {
                Ok(true) => {
                    return Ok(Some(Published {
                        seq,
                        pack,
                        attempts: attempt,
                    }))
                }
                Ok(false) => {}
                Err(MetaError::Raced { .. }) => back_off(attempt),
                Err(e) => return Err(e),
            }
        }
        Err(MetaError::Contended {
            attempts: self.attempts,
        })
    }

    /// Writes `entry` at `head + 1` and swaps the manifest to include it (and
    /// what `change` does to it). `Ok(false)` when another device swapped first
    /// (the entry is removed again).
    fn commit_entry(
        &mut self,
        entry: &LogEntry,
        change: impl FnOnce(&mut Manifest),
    ) -> Result<bool, MetaError> {
        let synced = self
            .synced
            .as_ref()
            .ok_or(MetaError::NoRepository)?;
        let known = synced.version.clone();
        let log_key = keys::log(entry.seq, &attempt_id());
        let sealed_entry = self.seal(&log_key, entry)?;
        match self.bucket.create(&log_key, &sealed_entry) {
            // The key holds this attempt's random id, so nobody else writes it: a conflict is
            // this very create, sent again after its answer was lost.
            Ok(_) | Err(MetaError::Conflict { .. }) => {}
            Err(e) => return Err(e),
        }

        let mut next = synced.manifest.clone();
        next.head_seq = entry.seq;
        next.revision += 1;
        next.log.push(LogRef {
            seq: entry.seq,
            key: log_key.clone(),
        });
        next.writer = self.device.clone();
        next.updated_at = self.now();
        change(&mut next);
        match self.swap_manifest(&next, known.as_deref()) {
            Ok(version) => {
                let mut state = self.state.clone();
                apply(&mut state, entry, &log_key)?;
                state.packs = next.packs.clone();
                state.revision = next.revision;
                self.state = state;
                self.seen_revision = next.revision;
                self.synced = Some(Synced {
                    manifest: next,
                    version,
                });
                Ok(true)
            }
            Err(lost @ (MetaError::Conflict { .. } | MetaError::Raced { .. })) => {
                // Another device swapped first - or this very swap landed, its answer got lost
                // and the request, sent again, met the version it had made. A fresh read
                // decides.
                let names_entry =
                    |manifest: &Manifest| manifest.log.iter().any(|r| r.key == log_key);
                match self.swap_landed(names_entry)? {
                    Some(true) => Ok(true),
                    // A lost race: nothing names the entry, and nobody else writes its key.
                    Some(false) if matches!(lost, MetaError::Conflict { .. }) => {
                        let _ = self.bucket.remove(&log_key);
                        Ok(false)
                    }
                    // A race (409) may be this swap itself, still under way: the entry stays
                    // (one nothing names is only space; a named one removed would break the
                    // repository) and the publish is tried again.
                    Some(false) => Ok(false),
                    // The bucket cannot be read: the swap may have landed (its repeat met its
                    // own version) or lost. The conflict is no answer to hand on - a caller
                    // that took it for "refused for good" would delete the data object the
                    // manifest may now name (AZ-M1). The entry stays too.
                    None => Err(MetaError::OutcomeUnknown {
                        key: keys::MANIFEST.to_string(),
                    }),
                }
            }
            Err(e) => {
                // The swap may have landed and only the answer got lost.
                let names_entry =
                    |manifest: &Manifest| manifest.log.iter().any(|r| r.key == log_key);
                match self.swap_landed(names_entry)? {
                    Some(true) => Ok(true),
                    // Refused for good - a drive that is frozen, over its quota - and named by no
                    // manifest: the entry goes again. Every try of a drive that refuses its index
                    // changes used to leave one sealed entry behind, counted in the budget the
                    // drive has for them (AZ-M6).
                    Some(false) if refused_for_good(&e) => {
                        // collapse-ok: an entry nothing names is only space; a delete the
                        // storage also refuses leaves it for the orphan sweep.
                        let _ = self.bucket.remove(&log_key);
                        Err(e)
                    }
                    _ => Err(e),
                }
            }
        }
    }

    /// Whether the manifest in the bucket now is one this device's swap made (`mine` says so);
    /// it is adopted then. `None` when the bucket cannot be read now. After a swap whose
    /// outcome is unknown this decides, never the swap's own error.
    fn swap_landed(
        &mut self,
        mine: impl Fn(&Manifest) -> bool,
    ) -> Result<Option<bool>, MetaError> {
        let (bytes, version) = match self.bucket.read(keys::MANIFEST) {
            Ok(Some(found)) => found,
            Ok(None) => return Ok(Some(false)),
            Err(_) => return Ok(None),
        };
        let Ok(manifest) = self.open_sealed::<Manifest>(keys::MANIFEST, &bytes) else {
            return Ok(None);
        };
        if !mine(&manifest) {
            return Ok(Some(false));
        }
        self.adopt(manifest, Some(version))?;
        Ok(Some(true))
    }

    /// Replaces the manifest from `known` (or creates it when this device has
    /// never seen a version: only right after a create whose version was not told).
    fn swap_manifest(
        &self,
        next: &Manifest,
        known: Option<&str>,
    ) -> Result<Option<Version>, MetaError> {
        let sealed = self.seal(keys::MANIFEST, next)?;
        match known {
            Some(known) => self.bucket.replace(keys::MANIFEST, &sealed, known),
            None => Err(MetaError::Conflict {
                key: keys::MANIFEST.to_string(),
            }),
        }
    }

    // ---- checkpoints ----

    /// Writes a checkpoint at the head and points the manifest at it; the log
    /// entries and the checkpoint it replaces are retired. A device behind it
    /// starts from it.
    pub fn checkpoint(&mut self) -> Result<CheckpointRef, MetaError> {
        for attempt in 1..=self.attempts {
            self.sync()?;
            let synced = self.synced.as_ref().ok_or(MetaError::NoRepository)?;
            if synced
                .manifest
                .checkpoint
                .as_ref()
                .is_some_and(|c| c.seq == self.state.head_seq)
            {
                if let Some(existing) = synced.manifest.checkpoint.clone() {
                    return Ok(existing);
                }
            }
            let now = self.now();
            let checkpoint = Checkpoint {
                seq: self.state.head_seq,
                refs: self.state.refs.clone(),
                packs: self.state.packs.clone(),
                writer: self.device.clone(),
                created_at: now,
            };
            let key = keys::checkpoint(checkpoint.seq, &attempt_id());
            let sealed = self.seal(&key, &checkpoint)?;
            match self.bucket.create(&key, &sealed) {
                // A random attempt id: a conflict is this create, sent again.
                Ok(_) | Err(MetaError::Conflict { .. }) => {}
                Err(MetaError::Raced { .. }) => {
                    back_off(attempt);
                    continue;
                }
                Err(e) => return Err(e),
            }
            let cp_ref = CheckpointRef {
                seq: checkpoint.seq,
                key: key.clone(),
                created_at: now,
            };
            let mut next = synced.manifest.clone();
            let known = synced.version.clone();
            for old in std::mem::take(&mut next.log) {
                next.retired.push(Retired { key: old.key, at: now });
            }
            if let Some(old) = next.checkpoint.replace(cp_ref.clone()) {
                next.retired.push(Retired { key: old.key, at: now });
            }
            next.revision += 1;
            next.writer = self.device.clone();
            next.updated_at = now;
            match self.swap_manifest(&next, known.as_deref()) {
                Ok(version) => {
                    self.seen_revision = next.revision;
                    self.state.revision = next.revision;
                    self.synced = Some(Synced {
                        manifest: next,
                        version,
                    });
                    return Ok(cp_ref);
                }
                Err(lost) => {
                    // As in a publish: the swap may have landed with its answer lost; only a
                    // manifest that does not name the checkpoint lets the race be lost.
                    let names = |manifest: &Manifest| {
                        manifest.checkpoint.as_ref().is_some_and(|c| c.key == key)
                    };
                    match (self.swap_landed(names)?, &lost) {
                        (Some(true), _) => return Ok(cp_ref),
                        (Some(false), MetaError::Conflict { .. }) => {
                            let _ = self.bucket.remove(&key);
                        }
                        (Some(false), MetaError::Raced { .. }) => back_off(attempt),
                        // Unreadable now: the swap may have landed or lost - not a conflict.
                        (None, MetaError::Conflict { .. } | MetaError::Raced { .. }) => {
                            return Err(MetaError::OutcomeUnknown {
                                key: keys::MANIFEST.to_string(),
                            })
                        }
                        _ => return Err(lost),
                    }
                }
            }
        }
        Err(MetaError::Contended {
            attempts: self.attempts,
        })
    }

    // ---- leases ----

    /// Takes the lease for `purpose` for `ttl` seconds: when nobody holds it,
    /// when it expired, or when this device holds it already.
    pub fn acquire_lease(&mut self, purpose: &str, ttl: u64) -> Result<LeaseGuard, MetaError> {
        let key = keys::lease(purpose);
        let now = self.now();
        let lease = Lease {
            holder: self.device.clone(),
            purpose: purpose.to_string(),
            expires_at: now + ttl,
        };
        let sealed = self.seal(&key, &lease)?;
        let written = match self.bucket.read(&key)? {
            None => self.bucket.create(&key, &sealed),
            Some((bytes, version)) => {
                let current: Lease = self.open_sealed(&key, &bytes)?;
                if current.expires_at > now && current.holder != self.device {
                    return Err(MetaError::LeaseHeld {
                        holder: current.holder,
                        expires_at: current.expires_at,
                    });
                }
                self.bucket.replace(&key, &sealed, &version)
            }
        };
        match written {
            Ok(version) => Ok(LeaseGuard {
                key,
                version,
                purpose: purpose.to_string(),
                expires_at: lease.expires_at,
            }),
            Err(MetaError::Conflict { .. }) => {
                // Another device took it in between.
                let holder = match self.bucket.read(&key)? {
                    Some((bytes, _)) => self.open_sealed::<Lease>(&key, &bytes)?,
                    None => lease,
                };
                Err(MetaError::LeaseHeld {
                    holder: holder.holder,
                    expires_at: holder.expires_at,
                })
            }
            Err(e) => Err(e),
        }
    }

    /// The version of this device's own lease in the bucket: the guard's, or
    /// (when the bucket did not tell it) the one read now, if the lease there is
    /// still this device's.
    fn own_lease_version(&self, guard: &LeaseGuard) -> Result<Version, MetaError> {
        if let Some(version) = &guard.version {
            return Ok(version.clone());
        }
        let lost = |holder: String, expires_at: u64| MetaError::LeaseHeld { holder, expires_at };
        match self.bucket.read(&guard.key)? {
            Some((bytes, version)) => {
                let current: Lease = self.open_sealed(&guard.key, &bytes)?;
                if current.holder == self.device && current.expires_at == guard.expires_at {
                    Ok(version)
                } else {
                    Err(lost(current.holder, current.expires_at))
                }
            }
            None => Err(lost(String::new(), 0)),
        }
    }

    /// Extends the lease to `ttl` seconds from now; [`MetaError::LeaseHeld`]
    /// when it was lost (it expired and another device took it).
    pub fn renew_lease(&mut self, guard: &mut LeaseGuard, ttl: u64) -> Result<(), MetaError> {
        let version = self.own_lease_version(guard)?;
        let lease = Lease {
            holder: self.device.clone(),
            purpose: guard.purpose.clone(),
            expires_at: self.now() + ttl,
        };
        let sealed = self.seal(&guard.key, &lease)?;
        match self.bucket.replace(&guard.key, &sealed, &version) {
            Ok(version) => {
                guard.version = version;
                guard.expires_at = lease.expires_at;
                Ok(())
            }
            Err(MetaError::Conflict { .. }) => Err(MetaError::LeaseHeld {
                holder: String::new(),
                expires_at: 0,
            }),
            Err(e) => Err(e),
        }
    }

    /// Gives the lease back (it expires now). Losing it in the meantime is fine.
    pub fn release_lease(&mut self, guard: LeaseGuard) -> Result<(), MetaError> {
        let Ok(version) = self.own_lease_version(&guard) else {
            return Ok(());
        };
        let lease = Lease {
            holder: self.device.clone(),
            purpose: guard.purpose.clone(),
            expires_at: 0,
        };
        let sealed = self.seal(&guard.key, &lease)?;
        match self.bucket.replace(&guard.key, &sealed, &version) {
            Ok(_) | Err(MetaError::Conflict { .. }) => Ok(()),
            Err(e) => Err(e),
        }
    }

    // ---- maintenance (under the lease) ----

    /// Folds every live pack into one (under `guard`): the objects stay, the old
    /// packs are retired. `Ok(None)` when there is at most one pack.
    pub fn compact(&mut self, guard: &LeaseGuard) -> Result<Option<Compacted>, MetaError> {
        self.compact_within(guard, u64::MAX)
    }

    /// [`MetaStore::compact`] of the smallest packs whose bytes add up to `max_bytes` at most: the
    /// pack it makes is never bigger than that (and holds that much in memory at most). A pack
    /// that is bigger than `max_bytes` itself is left as it is; `Ok(None)` when fewer than two
    /// packs fit together (AZ-M5: a repository of a few hundred MiB was folded into one pack a
    /// frozen drive's node refuses, and read whole by every device that opened it).
    pub fn compact_within(
        &mut self,
        guard: &LeaseGuard,
        max_bytes: u64,
    ) -> Result<Option<Compacted>, MetaError> {
        self.check_guard(guard)?;
        for attempt in 1..=self.attempts {
            self.sync()?;
            let mut by_size = self.state.packs.clone();
            by_size.sort_by_key(|pack| (pack.pack_size, pack.seq));
            let mut total = 0u64;
            let mut old = Vec::new();
            for pack in by_size {
                match total.checked_add(pack.pack_size) {
                    Some(sum) if sum <= max_bytes => {
                        total = sum;
                        old.push(pack);
                    }
                    _ => break,
                }
            }
            if old.len() < 2 {
                return Ok(None);
            }
            let mut objects = Objects::new();
            for pack in &old {
                self.fetch_pack(pack, &mut objects)?;
            }
            let mut writer = PackWriter::new();
            let ids: Vec<_> = objects.ids().copied().collect();
            for id in &ids {
                writer.add_from(&objects, id)?;
            }
            let seq = self.state.head_seq + 1;
            let pack = match self.put_pack(&writer, seq) {
                Ok(pack) => pack,
                Err(MetaError::Raced { .. }) => {
                    back_off(attempt);
                    continue;
                }
                Err(e) => return Err(e),
            };
            let replaced: Vec<String> = old.iter().map(|p| p.name.clone()).collect();
            let entry = LogEntry {
                seq,
                kind: EntryKind::Compact,
                pack: Some(pack.clone()),
                updates: Vec::new(),
                supersedes: replaced.clone(),
                writer: self.device.clone(),
                created_at: self.now(),
                message: String::new(),
            };
            let now = self.now();
            let new_pack = pack.clone();
            let gone = replaced.clone();
            let committed = self.commit_entry(&entry, move |manifest| {
                manifest.packs.retain(|p| !gone.contains(&p.name));
                // The same objects can come out under a name the manifest has.
                if !manifest.packs.iter().any(|p| p.name == new_pack.name) {
                    manifest.packs.push(new_pack);
                }
                for name in &gone {
                    manifest.retired.push(Retired {
                        key: keys::pack(name),
                        at: now,
                    });
                    manifest.retired.push(Retired {
                        key: keys::idx(name),
                        at: now,
                    });
                }
            });
            let committed = match committed {
                Ok(committed) => committed,
                Err(MetaError::Raced { .. }) => {
                    back_off(attempt);
                    continue;
                }
                Err(e) => return Err(e),
            };
            if committed {
                return Ok(Some(Compacted {
                    seq,
                    pack,
                    replaced,
                }));
            }
        }
        Err(MetaError::Contended {
            attempts: self.attempts,
        })
    }

    /// Deletes what was retired at least `grace` seconds ago (under `guard`):
    /// every device that read the older manifest has had time to finish.
    /// Returns how many objects were due.
    ///
    /// In three steps. A swap that fences (a collection that read an older manifest cannot go on)
    /// and takes back from `retired` what a manifest names again (the pack of a slow writer
    /// that a sweep took for an orphan). Then the deletes, while the due objects are STILL listed
    /// as retired: a writer that meets one of their names takes the object for one on its way out
    /// and uploads its own under another name ([`MetaStore::put_pack`]). Then a swap that drops
    /// what was deleted from the list; one that is lost leaves the names listed for the next
    /// round, and an object the storage would not delete stays listed for it too.
    pub fn collect_garbage(&mut self, guard: &LeaseGuard, grace: u64) -> Result<usize, MetaError> {
        self.check_guard(guard)?;
        for attempt in 1..=self.attempts {
            self.sync()?;
            let synced = self.synced.as_ref().ok_or(MetaError::NoRepository)?;
            let now = self.now();
            // A retired object the manifest names again (the pack of a slow writer
            // that a sweep took for an orphan) is kept and no longer retired.
            let live = live_keys(&synced.manifest);
            let (named, retired): (Vec<Retired>, Vec<Retired>) = synced
                .manifest
                .retired
                .iter()
                .cloned()
                .partition(|r| live.contains(&r.key));
            let (due, keep): (Vec<Retired>, Vec<Retired>) =
                retired.into_iter().partition(|r| r.at + grace <= now);
            if due.is_empty() && named.is_empty() {
                return Ok(0);
            }
            let mut next = synced.manifest.clone();
            let known = synced.version.clone();
            next.retired = keep.iter().chain(due.iter()).cloned().collect();
            next.revision += 1;
            next.writer = self.device.clone();
            next.updated_at = now;
            match self.swap_manifest(&next, known.as_deref()) {
                Ok(version) => {
                    self.seen_revision = next.revision;
                    self.state.revision = next.revision;
                    self.synced = Some(Synced {
                        manifest: next,
                        version,
                    });
                    // No manifest names them any more: deleting them is safe, and
                    // one that stays behind is only space.
                    let mut gone: Vec<String> = Vec::new();
                    for retired in &due {
                        if self.bucket.remove(&retired.key).is_ok() {
                            gone.push(retired.key.clone());
                        }
                    }
                    self.forget_retired(&gone);
                    return Ok(due.len());
                }
                Err(MetaError::Conflict { .. }) => {}
                Err(MetaError::Raced { .. }) => back_off(attempt),
                Err(e) => return Err(e),
            }
        }
        Err(MetaError::Contended {
            attempts: self.attempts,
        })
    }

    /// Drops `gone` - objects that were deleted - from the list of retired objects. Best effort: a
    /// swap that is lost leaves them listed, and the next collection lists them again (deleting an
    /// object that is not there is nothing).
    fn forget_retired(&mut self, gone: &[String]) {
        if gone.is_empty() {
            return;
        }
        for attempt in 1..=self.attempts {
            if self.sync().is_err() {
                return;
            }
            let Some(synced) = self.synced.as_ref() else {
                return;
            };
            if !synced
                .manifest
                .retired
                .iter()
                .any(|r| gone.contains(&r.key))
            {
                return;
            }
            let mut next = synced.manifest.clone();
            let known = synced.version.clone();
            next.retired.retain(|r| !gone.contains(&r.key));
            next.revision += 1;
            next.writer = self.device.clone();
            next.updated_at = self.now();
            match self.swap_manifest(&next, known.as_deref()) {
                Ok(version) => {
                    self.seen_revision = next.revision;
                    self.state.revision = next.revision;
                    self.synced = Some(Synced {
                        manifest: next,
                        version,
                    });
                    return;
                }
                Err(MetaError::Conflict { .. }) => {}
                Err(MetaError::Raced { .. }) => back_off(attempt),
                Err(_) => return,
            }
        }
    }

    /// Under `guard`: swaps the manifest for the same one a revision higher - with the reclaimer's
    /// `mark` when there is one - but only while it is still the revision `expected`, the one the
    /// reclaimer read the drive at. That is the fence of a reclaimer: when this returns, nobody
    /// published since it read, and whoever publishes from now on meets a manifest that moved.
    /// [`MetaError::Conflict`] when the manifest is not at `expected` (the reads are old: nothing
    /// may be deleted from them); [`MetaError::OutcomeUnknown`] when the swap's landing cannot be
    /// told. Returns the revision the manifest is at now.
    pub fn stamp(
        &mut self,
        guard: &LeaseGuard,
        expected: u64,
        mark: Option<ReclaimMark>,
    ) -> Result<u64, MetaError> {
        self.check_guard(guard)?;
        self.sync()?;
        let synced = self.synced.as_ref().ok_or(MetaError::NoRepository)?;
        if synced.manifest.revision != expected {
            return Err(MetaError::Conflict {
                key: keys::MANIFEST.to_string(),
            });
        }
        let known = synced.version.clone();
        let mut next = synced.manifest.clone();
        next.revision += 1;
        next.writer = self.device.clone();
        next.updated_at = self.now();
        if mark.is_some() {
            next.reclaimed = mark;
        }
        match self.swap_manifest(&next, known.as_deref()) {
            Ok(version) => {
                self.seen_revision = next.revision;
                self.state.revision = next.revision;
                let revision = next.revision;
                self.synced = Some(Synced {
                    manifest: next,
                    version,
                });
                Ok(revision)
            }
            Err(lost) => {
                // As in a publish: the swap may have landed with its answer lost.
                let revision = next.revision;
                let device = self.device.clone();
                let mine = |manifest: &Manifest| {
                    manifest.revision == revision
                        && manifest.writer == device
                        && manifest.reclaimed == next.reclaimed
                };
                match self.swap_landed(mine)? {
                    Some(true) => Ok(revision),
                    Some(false) => Err(match lost {
                        MetaError::Raced { .. } => MetaError::Conflict {
                            key: keys::MANIFEST.to_string(),
                        },
                        other => other,
                    }),
                    None => Err(MetaError::OutcomeUnknown {
                        key: keys::MANIFEST.to_string(),
                    }),
                }
            }
        }
    }

    /// The lease is still this device's: not expired by this device's clock,
    /// and the bucket's lease is still the one the guard took (not released,
    /// not taken over). The lease only saves double work: every change of the
    /// manifest is a swap anyway.
    /// One maintenance round under the lease - `None` when another device holds it (the work
    /// is that device's): the orphan sweep (where the bucket can list), compaction from
    /// `compact_at_packs` live packs on, a checkpoint from `checkpoint_at_entries` log entries
    /// on, the garbage collection; then the lease goes back. A device runs it now and then,
    /// idle and on mains power.
    pub fn maintain(&mut self, rules: &Maintenance) -> Result<Option<Maintained>, MetaError> {
        let mut guard = match self.acquire_lease(MAINTENANCE, rules.lease_secs) {
            Ok(guard) => guard,
            Err(MetaError::LeaseHeld { .. }) => return Ok(None),
            Err(e) => return Err(e),
        };
        let done = self.maintain_under(&mut guard, rules);
        let released = self.release_lease(guard);
        let done = done?;
        released?;
        Ok(Some(done))
    }

    /// The steps of a round, each when it is due. They do not hang on each other: a compaction
    /// that fails (the storage did not answer, another device was quicker) does not keep the
    /// checkpoint and the collection from running, and the first failure is the round's answer
    /// when they are done. The lease is extended before the steps that hold it, so a round that
    /// takes longer than the lease still ends under it; a lease that is lost ends the round.
    fn maintain_under(
        &mut self,
        guard: &mut LeaseGuard,
        rules: &Maintenance,
    ) -> Result<Maintained, MetaError> {
        let swept = match self.sweep_orphans(guard, rules.orphan_age) {
            Ok(swept) => swept,
            Err(MetaError::Unsupported(_)) => 0,
            Err(e) => return Err(e),
        };
        self.sync()?;
        let mut failed: Option<MetaError> = None;
        let mut compacted = false;
        if self.state.packs.len() >= rules.compact_at_packs.max(2) {
            self.renew_lease(guard, rules.lease_secs)?;
            match self.compact_within(guard, rules.compact_max_bytes) {
                Ok(done) => compacted = done.is_some(),
                Err(e) if round_goes_on(&e) => failed = Some(e),
                Err(e) => return Err(e),
            }
        }
        let entries = self.synced.as_ref().map_or(0, |s| s.manifest.log.len());
        let mut checkpointed = false;
        if entries >= rules.checkpoint_at_entries.max(1) {
            match self.checkpoint() {
                Ok(_) => checkpointed = true,
                Err(e) if round_goes_on(&e) => {
                    failed.get_or_insert(e);
                }
                Err(e) => return Err(e),
            }
        }
        self.renew_lease(guard, rules.lease_secs)?;
        let collected = match self.collect_garbage(guard, rules.grace) {
            Ok(collected) => collected,
            Err(e) if round_goes_on(&e) => {
                failed.get_or_insert(e);
                0
            }
            Err(e) => return Err(e),
        };
        match failed {
            Some(e) => Err(e),
            None => Ok(Maintained {
                swept,
                compacted,
                checkpointed,
                collected,
            }),
        }
    }

    /// Finds what no manifest names - the packs of lost swaps, the log entries
    /// and checkpoints of writers that crashed between their write and their
    /// swap - and retires it (under `guard`): [`MetaStore::collect_garbage`]
    /// deletes it once its grace time has passed, unless a manifest names it by
    /// then. Only objects at least `min_age` seconds old count (by the bucket's
    /// dates, where it has them): a writer may be between its pack and its swap
    /// right now. The repository's one listing of its own folders, for
    /// maintenance; the number of objects retired.
    pub fn sweep_orphans(&mut self, guard: &LeaseGuard, min_age: u64) -> Result<usize, MetaError> {
        self.check_guard(guard)?;
        for attempt in 1..=self.attempts {
            self.sync()?;
            let now = self.now();
            let mut listed = Vec::new();
            for dir in [keys::WAL_DIR, keys::LOG_DIR, keys::CHECKPOINT_DIR] {
                listed.extend(self.bucket.list_keys(dir)?);
            }
            let synced = self.synced.as_ref().ok_or(MetaError::NoRepository)?;
            let mut known = live_keys(&synced.manifest);
            known.extend(synced.manifest.retired.iter().map(|r| r.key.clone()));
            let orphans: Vec<String> = listed
                .into_iter()
                .filter(|listed| !known.contains(&listed.key))
                .filter(|listed| listed.modified.map_or(true, |at| at + min_age <= now))
                .map(|listed| listed.key)
                .collect();
            if orphans.is_empty() {
                return Ok(0);
            }
            let mut next = synced.manifest.clone();
            let version = synced.version.clone();
            next.retired
                .extend(orphans.iter().map(|key| Retired { key: key.clone(), at: now }));
            next.revision += 1;
            next.writer = self.device.clone();
            next.updated_at = now;
            match self.swap_manifest(&next, version.as_deref()) {
                Ok(version) => {
                    self.seen_revision = next.revision;
                    self.state.revision = next.revision;
                    self.synced = Some(Synced {
                        manifest: next,
                        version,
                    });
                    return Ok(orphans.len());
                }
                Err(MetaError::Conflict { .. }) => {}
                Err(MetaError::Raced { .. }) => back_off(attempt),
                Err(e) => return Err(e),
            }
        }
        Err(MetaError::Contended {
            attempts: self.attempts,
        })
    }

    fn check_guard(&self, guard: &LeaseGuard) -> Result<(), MetaError> {
        if guard.expires_at <= self.now() {
            return Err(MetaError::LeaseHeld {
                holder: String::new(),
                expires_at: guard.expires_at,
            });
        }
        match self.bucket.read(&guard.key)? {
            Some((bytes, _)) => {
                let current: Lease = self.open_sealed(&guard.key, &bytes)?;
                if current.holder == self.device && current.expires_at == guard.expires_at {
                    Ok(())
                } else {
                    Err(MetaError::LeaseHeld {
                        holder: current.holder,
                        expires_at: current.expires_at,
                    })
                }
            }
            None => Err(MetaError::LeaseHeld {
                holder: String::new(),
                expires_at: 0,
            }),
        }
    }
}

/// What [`reseal`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Resealed {
    /// Whether this call made the switch (`false`: there was no repository, or it was
    /// switched already by a call that stopped later).
    pub switched: bool,
    /// The objects sealed with the old key that were deleted.
    pub removed: usize,
}

/// Seals the repository in `bucket` under `new` from now on, in place of `old` (a key
/// rotation; `device` writes, `now` dates).
///
/// 1. Every live object is read with `old` and written again with `new`: the objects in one
///    pack (its name is new too - a pack's name is a keyed hash), the refs in a checkpoint at
///    the head.
/// 2. **The switch:** the manifest is swapped (compare-and-swap) for one sealed with `new` that
///    names only those, everything the old one named listed as retired. Before it the
///    repository opens with `old`, after it with `new`; never with neither. Losing the swap to
///    another device starts again from 1.
/// 3. The objects sealed with `old` are deleted - what the old manifest named and what it had
///    retired - and the lease; then the manifest stops listing them.
///
/// Run again after a stop at any point, it finishes: a manifest that opens with `new` is past
/// the switch, and 3 runs (again). Objects a crashed writer left behind are the orphan sweep's.
/// A bucket without a repository has nothing to re-seal.
pub fn reseal<B: Bucket + ?Sized, O: Sealer, N: Sealer>(
    bucket: &B,
    old: &O,
    new: &N,
    device: &str,
    now: u64,
) -> Result<Resealed, MetaError> {
    'attempts: for attempt in 1..=DEFAULT_ATTEMPTS {
        let Some((bytes, version)) = bucket.read(keys::MANIFEST)? else {
            return Ok(Resealed::default());
        };
        if let Ok(manifest) = open_json::<Manifest, N>(new, keys::MANIFEST, &bytes) {
            let removed = remove_old(bucket, new, manifest, Some(version), device, now)?;
            return Ok(Resealed {
                switched: false,
                removed,
            });
        }
        let manifest: Manifest = open_json(old, keys::MANIFEST, &bytes)?;
        check_manifest(&manifest)?;
        // The repository as the old key reads it: the refs at the head, every object.
        let store = MetaStore::open(bucket, old, device)?;
        if store.state().revision != manifest.revision {
            continue;
        }
        let head = store.state().head_seq;
        // Pack by pack: each live pack is read, sealed again under `new` as ONE pack of its own and
        // dropped from memory. The objects of all of them in one new pack held the whole index in
        // memory, made a pack of the size of the index (a drive of some 600,000 files: more than
        // an index object may weigh, which no reader takes, and more than a frozen drive's node
        // takes in one PUT), and a repository the compaction keeps in several packs came out as
        // one (X-DELTA-AZDRIVE-KEYS-13).
        let mut packs: Vec<PackRef> = Vec::new();
        for old_pack in &store.state().packs {
            let mut objects = Objects::new();
            store.fetch_pack(old_pack, &mut objects)?;
            if objects.is_empty() {
                continue;
            }
            let mut writer = PackWriter::new();
            let ids: Vec<ObjectId> = objects.ids().copied().collect();
            for id in &ids {
                writer.add_from(&objects, id)?;
            }
            match put_sealed_pack(bucket, writer.seal(new)?, head) {
                // Two packs of the same objects are one pack under the new key.
                Ok(pack) => {
                    if !packs.iter().any(|kept| kept.name == pack.name) {
                        packs.push(pack);
                    }
                }
                Err(MetaError::Raced { .. }) => {
                    back_off(attempt);
                    continue 'attempts;
                }
                Err(e) => return Err(e),
            }
        }
        let checkpoint = Checkpoint {
            seq: head,
            refs: store.state().refs.clone(),
            packs: packs.clone(),
            writer: device.to_string(),
            created_at: now,
        };
        let checkpoint_key = keys::checkpoint(head, &attempt_id());
        match bucket.create(&checkpoint_key, &seal_json(new, &checkpoint_key, &checkpoint)?) {
            Ok(_) => {}
            Err(MetaError::Raced { .. }) => {
                back_off(attempt);
                continue;
            }
            Err(e) => return Err(e),
        }
        let mut retired = manifest.retired.clone();
        for key in live_keys(&manifest) {
            if key != keys::MANIFEST {
                retired.push(Retired { key, at: now });
            }
        }
        let next = Manifest {
            format: FORMAT,
            object_format: OBJECT_FORMAT.to_string(),
            head_seq: head,
            revision: manifest.revision + 1,
            checkpoint: Some(CheckpointRef {
                seq: head,
                key: checkpoint_key.clone(),
                created_at: now,
            }),
            log: Vec::new(),
            packs,
            retired,
            reclaimed: manifest.reclaimed.clone(),
            writer: device.to_string(),
            updated_at: now,
        };
        let sealed = seal_json(new, keys::MANIFEST, &next)?;
        match bucket.replace(keys::MANIFEST, &sealed, &version) {
            Ok(version) => {
                let removed = remove_old(bucket, new, next, version, device, now)?;
                return Ok(Resealed {
                    switched: true,
                    removed,
                });
            }
            Err(MetaError::Conflict { .. }) => {
                let _ = bucket.remove(&checkpoint_key);
            }
            Err(MetaError::Raced { .. }) => {
                let _ = bucket.remove(&checkpoint_key);
                back_off(attempt);
            }
            Err(e) => {
                // The swap may have landed and only its answer got lost: a fresh read decides.
                match bucket.read(keys::MANIFEST) {
                    Ok(Some((bytes, version))) => {
                        if let Ok(landed) = open_json::<Manifest, N>(new, keys::MANIFEST, &bytes) {
                            let removed =
                                remove_old(bucket, new, landed, Some(version), device, now)?;
                            return Ok(Resealed {
                                switched: true,
                                removed,
                            });
                        }
                        // Not switched: the new checkpoint is nobody's.
                        let _ = bucket.remove(&checkpoint_key);
                    }
                    Ok(None) | Err(_) => {}
                }
                return Err(e);
            }
        }
    }
    Err(MetaError::Contended {
        attempts: DEFAULT_ATTEMPTS,
    })
}

/// Step 3 of [`reseal`]: deletes what `manifest` (sealed with `new`, past the switch) lists
/// as retired - the objects sealed with the old key - and a lease the new key does not open,
/// then swaps the manifest for one that lists none of them (a lost swap leaves them listed;
/// garbage collection drops the names of objects that are gone). An object that cannot be
/// deleted stops it: run again, it goes on.
fn remove_old<B: Bucket + ?Sized, N: Sealer>(
    bucket: &B,
    new: &N,
    manifest: Manifest,
    version: Option<Version>,
    device: &str,
    now: u64,
) -> Result<usize, MetaError> {
    let mut removed = 0;
    for retired in &manifest.retired {
        bucket.remove(&retired.key)?;
        removed += 1;
    }
    let lease = keys::lease(MAINTENANCE);
    if let Some((bytes, _)) = bucket.read(&lease)? {
        if open_json::<Lease, N>(new, &lease, &bytes).is_err() {
            bucket.remove(&lease)?;
            removed += 1;
        }
    }
    if let (false, Some(version)) = (manifest.retired.is_empty(), version) {
        let mut next = manifest;
        next.retired.clear();
        next.revision += 1;
        next.writer = device.to_string();
        next.updated_at = now;
        let sealed = seal_json(new, keys::MANIFEST, &next)?;
        match bucket.replace(keys::MANIFEST, &sealed, &version) {
            Ok(_) | Err(MetaError::Conflict { .. } | MetaError::Raced { .. }) => {}
            Err(e) => return Err(e),
        }
    }
    Ok(removed)
}

/// Whether `e` says the storage refused the change for good (or it was never made): nothing
/// landed, and nothing will.
fn refused_for_good(e: &MetaError) -> bool {
    matches!(e, MetaError::Drive(d) if d.certainty() != Certainty::Unknown)
}

/// Every key the manifest names: itself, the live packs and their indexes, the
/// log entries, the checkpoint.
fn live_keys(manifest: &Manifest) -> HashSet<String> {
    let mut live = HashSet::new();
    live.insert(keys::MANIFEST.to_string());
    for pack in &manifest.packs {
        live.insert(keys::pack(&pack.name));
        live.insert(keys::idx(&pack.name));
    }
    live.extend(manifest.log.iter().map(|entry| entry.key.clone()));
    if let Some(checkpoint) = &manifest.checkpoint {
        live.insert(checkpoint.key.clone());
    }
    live
}

/// A manifest this code can work with: its format, and a log without gaps from
/// the checkpoint to the head.
fn check_manifest(manifest: &Manifest) -> Result<(), MetaError> {
    let key = keys::MANIFEST;
    if manifest.format != FORMAT {
        return Err(corrupt(key, format!("manifest format {}", manifest.format)));
    }
    if manifest.object_format != OBJECT_FORMAT {
        return Err(corrupt(
            key,
            format!("the object format {}", manifest.object_format),
        ));
    }
    let base = manifest.checkpoint.as_ref().map_or(0, |c| c.seq);
    if base > manifest.head_seq {
        return Err(corrupt(key, "a checkpoint after the head"));
    }
    let mut expected = base + 1;
    for entry in &manifest.log {
        if entry.seq != expected || !entry.key.starts_with(keys::LOG_DIR) {
            return Err(corrupt(key, "a log with a gap"));
        }
        expected += 1;
    }
    if expected != manifest.head_seq + 1 {
        return Err(corrupt(key, "a log that does not reach the head"));
    }
    Ok(())
}

/// Applies one entry to the state. A Push whose old values do not match is a
/// damaged log: its writer checked them against this very state.
fn apply(state: &mut RepoState, entry: &LogEntry, key: &str) -> Result<(), MetaError> {
    if entry.seq != state.head_seq + 1 {
        return Err(corrupt(key, "a log entry out of sequence"));
    }
    if entry.kind == EntryKind::Push {
        for update in &entry.updates {
            if state.refs.get(&update.name) != update.old.as_ref() {
                return Err(corrupt(key, format!("an update of {} from another value", update.name)));
            }
            match &update.new {
                Some(new) => {
                    state.refs.insert(update.name.clone(), new.clone());
                }
                None => {
                    state.refs.remove(&update.name);
                }
            }
        }
    }
    state.head_seq = entry.seq;
    Ok(())
}
