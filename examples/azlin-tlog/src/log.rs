//! The operator side of the log: append a release, sign a checkpoint, make the request a witness needs and the
//! proof bundle that travels with a release (planning/architecture.md section 2.20: "every signed release ...
//! is appended to Azlin's public log, a Merkle tree whose checkpoints independent witnesses cosign").
//!
//! A [`Log`] is a Merkle tree over a leaf store ([`azlin_merkle::Tree`]) and the record of every leaf in a
//! [`RecordStore`]. Both stores are seams (architecture 1.4 rule 1): production passes files or database
//! rows, the tests pass memory. The log only grows, and:
//!
//! - a release is logged once: the same record again returns its index and appends nothing; another record
//!   with the same (kind, name, version, arch) is an EQUIVOCATION, refused with the tree unchanged (T24,
//!   T25: one release with two digests is the tailored build the log exists to expose, architecture 3.9);
//! - the records are written BEFORE the leaf: a crash between the two writes leaves one record more than
//!   leaves, which [`Log::open`] completes (it appends the leaf of that one record); a leaf without its
//!   record, or any other disagreement, refuses to open (drift row X-DELTA-W1S45C-14);
//! - [`Log::open`] checks every stored record against its stored leaf hash and rebuilds the book of
//!   releases from the records (T24: the same stores give the same root).
//!
//! The operator's round after `append`: [`Log::checkpoint`] (the log's signature, Touch ID in AzCtl), one
//! [`Log::add_request`] per witness, the cosignatures added with [`Note::with_sig`], then [`Log::bundle`] for
//! the release. The memory of a log is the tree's: one hash per leaf and one per complete subtree, plus the
//! book of releases; nothing here promises less.
//!
//! Seam: first caller is AzCtl's release flow (S45-INT-1: `release sign|publish`).

use std::collections::BTreeMap;
use std::fmt;

use azlin_merkle::{
    leaf_hash, Hash, LeafIndex, LeafStore, StoreError, Tree, TreeError, TreeSize, MAX_LEAVES,
};

use crate::bundle::ProofBundle;
use crate::checkpoint::{sign_checkpoint, Checkpoint, CheckpointError};
use crate::names::Origin;
use crate::note::{Note, NoteError, NoteSigner};
use crate::record::{RecordError, ReleaseKey, ReleaseRecord};
use crate::witness::{AddRequest, RequestError};

/// Where the bytes of each leaf's record live, in the order of the leaves (the seam of 1.4 rule 1). The trait
/// has no way to change or remove a record: a log only grows.
///
/// Seam: first non-test implementors are the durable stores of AzCtl's release flow (S45-INT-1).
pub trait RecordStore {
    /// The number of records.
    fn len(&self) -> Result<TreeSize, StoreError>;
    /// Whether the store holds no record (provided: `len() == 0`).
    fn is_empty(&self) -> Result<bool, StoreError> {
        Ok(self.len()?.0 == 0)
    }
    /// The bytes of record `index`, or [`StoreError::IndexOutOfRange`] when `index >= len`.
    fn get(&self, index: LeafIndex) -> Result<Vec<u8>, StoreError>;
    /// Adds a record at the end; its index is the length before the call. A failed call has not appended.
    fn append(&mut self, bytes: Vec<u8>) -> Result<(), StoreError>;
}

/// A [`RecordStore`] in memory: the store of the tests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MemRecords {
    records: Vec<Vec<u8>>,
}

impl MemRecords {
    /// An empty store.
    pub fn new() -> MemRecords {
        MemRecords::default()
    }
}

impl RecordStore for MemRecords {
    fn len(&self) -> Result<TreeSize, StoreError> {
        Ok(TreeSize(self.records.len() as u64))
    }

    fn get(&self, index: LeafIndex) -> Result<Vec<u8>, StoreError> {
        let at = usize::try_from(index.0).map_err(|_| StoreError::IndexOutOfRange)?;
        self.records
            .get(at)
            .cloned()
            .ok_or(StoreError::IndexOutOfRange)
    }

    fn append(&mut self, bytes: Vec<u8>) -> Result<(), StoreError> {
        self.records.push(bytes);
        Ok(())
    }
}

/// Why a log call failed. Never a string.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum LogError {
    /// A store failed.
    Store(StoreError),
    /// The tree failed or refused.
    Tree(TreeError),
    /// A stored record is no valid record.
    BadRecord {
        /// The leaf whose record is refused.
        index: LeafIndex,
        /// Why.
        error: RecordError,
    },
    /// The record store and the leaf store do not agree in size (and the record store is not exactly one
    /// record ahead).
    RecordsMismatch {
        /// The number of leaves.
        leaves: TreeSize,
        /// The number of records.
        records: TreeSize,
    },
    /// The record stored for this leaf does not hash to the stored leaf hash.
    RecordMismatch {
        /// The leaf whose record differs.
        index: LeafIndex,
    },
    /// The same release (kind, name, version, arch) with other bytes is already logged at `first`: nothing was
    /// appended (T25).
    Equivocation {
        /// The leaf that holds the release.
        first: LeafIndex,
    },
    /// The stores hold the same release twice (an honest log appends it once).
    DuplicateRelease {
        /// The earlier leaf.
        first: LeafIndex,
        /// The later leaf.
        second: LeafIndex,
    },
    /// The note is no checkpoint.
    Checkpoint(CheckpointError),
    /// The note could not be made.
    Note(NoteError),
    /// The request could not be made.
    Request(RequestError),
    /// The checkpoint is not this log's: another origin, more leaves than the log has, or another root.
    NotOurs,
    /// The old size is above the checkpoint's size.
    OldAfterNew,
    /// The leaf index is not below the checkpoint's size.
    IndexOutOfRange,
}

impl From<StoreError> for LogError {
    fn from(error: StoreError) -> LogError {
        LogError::Store(error)
    }
}

impl From<TreeError> for LogError {
    fn from(error: TreeError) -> LogError {
        LogError::Tree(error)
    }
}

impl fmt::Display for LogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LogError::Store(error) => write!(f, "a store failed: {error}"),
            LogError::Tree(error) => write!(f, "the tree failed: {error}"),
            LogError::BadRecord { index, error } => {
                write!(f, "the record of leaf {} is refused: {error}", index.0)
            }
            LogError::RecordsMismatch { leaves, records } => {
                write!(f, "{} leaves and {} records", leaves.0, records.0)
            }
            LogError::RecordMismatch { index } => {
                write!(
                    f,
                    "the record of leaf {} does not hash to the leaf",
                    index.0
                )
            }
            LogError::Equivocation { first } => {
                write!(
                    f,
                    "the release is logged at leaf {} with other bytes",
                    first.0
                )
            }
            LogError::DuplicateRelease { first, second } => {
                write!(
                    f,
                    "the release is logged at leaves {} and {}",
                    first.0, second.0
                )
            }
            LogError::Checkpoint(error) => write!(f, "the note is no checkpoint: {error}"),
            LogError::Note(error) => write!(f, "the note could not be made: {error}"),
            LogError::Request(error) => write!(f, "the request could not be made: {error}"),
            LogError::NotOurs => f.write_str("the checkpoint is not of this log"),
            LogError::OldAfterNew => f.write_str("the old size is above the checkpoint's size"),
            LogError::IndexOutOfRange => f.write_str("the leaf is not below the checkpoint's size"),
        }
    }
}

impl std::error::Error for LogError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LogError::Store(error) => Some(error),
            LogError::Tree(error) => Some(error),
            LogError::BadRecord { error, .. } => Some(error),
            LogError::Checkpoint(error) => Some(error),
            LogError::Note(error) => Some(error),
            LogError::Request(error) => Some(error),
            _ => None,
        }
    }
}

/// The operator's log: a tree, the records of its leaves and the book of releases (see the module
/// documentation).
///
/// Seam: first caller is AzCtl's release flow (S45-INT-1).
pub struct Log<S: LeafStore, R: RecordStore> {
    origin: Origin,
    tree: Tree<S>,
    records: R,
    book: BTreeMap<ReleaseKey, (LeafIndex, Hash)>,
}

impl<S: LeafStore, R: RecordStore> Log<S, R> {
    /// Opens the log over its stores: every record is checked against its leaf hash and the book of releases
    /// is rebuilt from them. A record store that is exactly one record ahead of the leaf store (a crash
    /// between the two writes of an `append`) is completed; every other disagreement is refused.
    pub fn open(origin: Origin, store: S, records: R) -> Result<Log<S, R>, LogError> {
        let leaves = store.size()?;
        if leaves.0 > MAX_LEAVES.0 {
            return Err(LogError::Tree(TreeError::TooManyLeaves));
        }
        let held = records.len()?;
        if held.0 < leaves.0 || held.0 > leaves.0 + 1 {
            return Err(LogError::RecordsMismatch {
                leaves,
                records: held,
            });
        }
        let mut book: BTreeMap<ReleaseKey, (LeafIndex, Hash)> = BTreeMap::new();
        for position in 0..leaves.0 {
            let index = LeafIndex(position);
            let bytes = records.get(index)?;
            let record = ReleaseRecord::parse(&bytes)
                .map_err(|error| LogError::BadRecord { index, error })?;
            let leaf = leaf_hash(&bytes);
            if store.leaf(index)? != leaf {
                return Err(LogError::RecordMismatch { index });
            }
            if let Some((first, _)) = book.insert(record.key(), (index, leaf)) {
                return Err(LogError::DuplicateRelease {
                    first,
                    second: index,
                });
            }
        }
        let mut tree = Tree::new(store)?;
        if held.0 == leaves.0 + 1 {
            let index = LeafIndex(leaves.0);
            let bytes = records.get(index)?;
            let record = ReleaseRecord::parse(&bytes)
                .map_err(|error| LogError::BadRecord { index, error })?;
            if let Some((first, _)) = book.get(&record.key()) {
                return Err(LogError::DuplicateRelease {
                    first: *first,
                    second: index,
                });
            }
            let leaf = leaf_hash(&bytes);
            if tree.append_hash(leaf)? != index {
                return Err(LogError::Tree(TreeError::Inconsistent));
            }
            book.insert(record.key(), (index, leaf));
        }
        Ok(Log {
            origin,
            tree,
            records,
            book,
        })
    }

    /// The origin of this log.
    pub fn origin(&self) -> &Origin {
        &self.origin
    }

    /// The number of leaves.
    pub fn size(&self) -> TreeSize {
        self.tree.size()
    }

    /// The root over all leaves.
    pub fn root(&self) -> Result<Hash, LogError> {
        Ok(self.tree.root()?)
    }

    /// The leaf that holds the release `key`, if it is logged.
    pub fn find(&self, key: &ReleaseKey) -> Option<LeafIndex> {
        self.book.get(key).map(|(index, _)| *index)
    }

    /// Gives the stores back (to reopen the log after a failed `append`, which leaves the two stores one
    /// record apart). Seam: first caller is S45-INT-1 (AzCtl reopens its stores after a failed append).
    pub fn into_parts(self) -> (S, R) {
        (self.tree.into_store(), self.records)
    }

    /// Appends the record of a release and returns its leaf index. The same record again (the same bytes)
    /// returns the index it has and changes nothing; the same (kind, name, version, arch) with other bytes
    /// is an [`LogError::Equivocation`] and nothing is appended (T24, T25).
    pub fn append(&mut self, record: &ReleaseRecord) -> Result<LeafIndex, LogError> {
        let bytes = record.to_bytes();
        let leaf = leaf_hash(&bytes);
        let key = record.key();
        if let Some((index, held)) = self.book.get(&key) {
            return if *held == leaf {
                Ok(*index)
            } else {
                Err(LogError::Equivocation { first: *index })
            };
        }
        let size = self.tree.size();
        if size.0 >= MAX_LEAVES.0 {
            return Err(LogError::Tree(TreeError::TooManyLeaves));
        }
        let held = self.records.len()?;
        if held != size {
            return Err(LogError::RecordsMismatch {
                leaves: size,
                records: held,
            });
        }
        // The record first: if the leaf write fails or the process stops here, `open` completes it.
        self.records.append(bytes)?;
        let index = self.tree.append_hash(leaf)?;
        self.book.insert(key, (index, leaf));
        Ok(index)
    }

    /// The signed checkpoint of the log as it is now: the log's signature (type `0x01`) over its origin, size
    /// and root, with the extension lines `extensions`.
    pub fn checkpoint(
        &self,
        signer: &dyn NoteSigner,
        extensions: Vec<String>,
    ) -> Result<Note, LogError> {
        let root = self.tree.root()?;
        let checkpoint = Checkpoint::new(self.origin.clone(), self.tree.size(), root, extensions)
            .map_err(LogError::Checkpoint)?;
        sign_checkpoint(&checkpoint, signer).map_err(LogError::Note)
    }

    /// The request that asks a witness, which holds the checkpoint of size `old` (zero if it holds none), to
    /// cosign `note`: the consistency proof from `old` to the size of the checkpoint in `note`, which must
    /// be a checkpoint of this log.
    pub fn add_request(&self, old: TreeSize, note: &Note) -> Result<AddRequest, LogError> {
        let checkpoint = self.own_checkpoint(note)?;
        if old > checkpoint.size() {
            return Err(LogError::OldAfterNew);
        }
        let proof = self.tree.consistency_proof(old, checkpoint.size())?;
        AddRequest::new(old, proof.path, note.clone()).map_err(LogError::Request)
    }

    /// The proof bundle of leaf `index` under the cosigned checkpoint `cosigned` (a checkpoint of this log):
    /// the record, the inclusion proof for the checkpoint's size and the whole note with its cosignatures.
    /// Any checkpoint that covers the leaf will do; the bundle of an old checkpoint stays valid as the log
    /// grows (T29).
    pub fn bundle(&self, index: LeafIndex, cosigned: &Note) -> Result<ProofBundle, LogError> {
        let checkpoint = self.own_checkpoint(cosigned)?;
        if index.0 >= checkpoint.size().0 {
            return Err(LogError::IndexOutOfRange);
        }
        let proof = self.tree.inclusion_proof(index, checkpoint.size())?;
        let record = self.records.get(index)?;
        Ok(ProofBundle {
            record,
            proof,
            checkpoint: cosigned.to_bytes(),
        })
    }

    /// The checkpoint in `note`, if it is a checkpoint of this log: its origin, a size the log has reached
    /// and the root the log had at that size.
    fn own_checkpoint(&self, note: &Note) -> Result<Checkpoint, LogError> {
        let checkpoint = Checkpoint::parse_text(note.text()).map_err(LogError::Checkpoint)?;
        if checkpoint.origin() != &self.origin || checkpoint.size() > self.tree.size() {
            return Err(LogError::NotOurs);
        }
        if self.tree.root_at(checkpoint.size())? != checkpoint.root() {
            return Err(LogError::NotOurs);
        }
        Ok(checkpoint)
    }
}
