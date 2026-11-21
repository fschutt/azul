//! The monitor: anyone can watch the log from outside (planning/architecture.md section 2.20 point 3: "Azlin
//! publishes a small monitor that anyone can run"; point 7: "a hash the log does not hold (a build made for
//! some users) pages at once").
//!
//! A [`Monitor`] holds a [`Policy`], the last checkpoint it verified and an index of the releases the log
//! holds, by digest. It is pure: notes, proofs and records in, decisions out; it fetches nothing and keeps no
//! clock (the caller says what time it is).
//!
//! - [`Monitor::observe`] takes a checkpoint note. It must pass the policy (`Alarm::NotWitnessed`). A first
//!   checkpoint is remembered. Another of the same size must have the same root: two validly signed and
//!   cosigned checkpoints of one size with two roots are a split log, and the alarm carries BOTH notes as the
//!   evidence (`Alarm::Fork`, T30). A smaller size is `Alarm::Shrunk` (T31). A larger size needs a consistency
//!   proof from the size the monitor holds (`Observed::NeedsProof` without one); a proof that does not
//!   verify is `Alarm::BadConsistency` (T31).
//! - [`Monitor::ingest`] takes the records of the whole tree up to the last checkpoint, which must hash to its
//!   root; only then are they read, so a tampered list accuses nobody. A record that is no release record is
//!   `Alarm::BadRecord`; one release (kind, name, version, arch) twice is `Alarm::Equivocation` (a build
//!   made for some users alone is exactly this); more than the index limit is `Alarm::TooMany`, never an
//!   eviction (T32, T35).
//! - [`Monitor::knows`] answers whether the log holds a digest. The index covers the records of the LAST
//!   ingest: [`Monitor::indexed`] says up to which size; a watcher that sees the checkpoint grow ingests
//!   again before it says that a digest is unknown.
//!
//! Seam: first callers are the leader's watcher rule "a reported hash the log does not hold" (S45-INT-4,
//! S13-INT) and the phone apps' start check (2.20 point 6, brief drift 7; this crate provides the index, not
//! a lookup service).

use std::collections::BTreeMap;
use std::fmt;

use azlin_merkle::{
    leaf_hash, verify_consistency, ConsistencyProof, LeafIndex, MemStore, Tree, TreeError, TreeSize,
};

use crate::checkpoint::Checkpoint;
use crate::note::Note;
use crate::policy::{Policy, PolicyError};
use crate::record::{Digest, RecordError, ReleaseKey, ReleaseRecord};
use crate::time::UnixTime;

/// The most records the monitor's index holds (brief section 5): a larger log is refused, never evicted.
pub const MAX_INDEX_RECORDS: TreeSize = TreeSize(1_000_000);

/// What a checkpoint note told the monitor, when it raised no alarm.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Observed {
    /// The first checkpoint the monitor saw: it is remembered.
    First {
        /// The size of the tree.
        size: TreeSize,
    },
    /// The same size and the same root as the checkpoint held.
    Unchanged,
    /// A larger tree, proven consistent with the one held: it is remembered.
    Grew {
        /// The size held before.
        from: TreeSize,
        /// The size now.
        to: TreeSize,
    },
    /// A larger tree and no consistency proof: nothing changed; fetch the proof from `have` and ask again.
    NeedsProof {
        /// The size the monitor holds.
        have: TreeSize,
    },
}

/// What the caller must see: evidence of a misbehaving log, or a refusal. Never a string.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Alarm {
    /// The note does not pass the policy.
    NotWitnessed(PolicyError),
    /// Two checkpoints of one size with two roots, both validly signed and cosigned: the log is split.
    Fork {
        /// The checkpoint the monitor held.
        a: Note,
        /// The checkpoint that disagrees with it.
        b: Note,
    },
    /// A checkpoint smaller than the one held: the log rolled back, or the monitor is shown an old copy.
    Shrunk {
        /// The size held.
        have: TreeSize,
        /// The smaller size seen.
        seen: TreeSize,
    },
    /// The consistency proof does not prove that the held checkpoint is a prefix of the new one.
    BadConsistency,
    /// `ingest` before any checkpoint was observed.
    NoCheckpoint,
    /// The log has more records than the monitor's index limit.
    TooMany,
    /// The number of records is not the checkpoint's tree size.
    RecordCount {
        /// The tree size of the last checkpoint.
        want: TreeSize,
        /// The number of records given.
        have: usize,
    },
    /// The records do not hash to the root of the last checkpoint.
    RecordsMismatch,
    /// A record of the log (whose root matches) is no release record.
    BadRecord {
        /// The leaf.
        index: LeafIndex,
        /// Why.
        error: RecordError,
    },
    /// One release is in the log twice: a build for some users alone, or a log that breaks its own rule.
    Equivocation {
        /// The earlier leaf.
        first: LeafIndex,
        /// The later leaf.
        second: LeafIndex,
    },
    /// The tree over the records failed (not expected for a list in memory).
    Tree(TreeError),
}

impl fmt::Display for Alarm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Alarm::NotWitnessed(error) => write!(f, "the checkpoint is not witnessed: {error}"),
            Alarm::Fork { .. } => {
                f.write_str("two checkpoints of one size have two roots: the log is split")
            }
            Alarm::Shrunk { have, seen } => {
                write!(f, "the log shrank from {} to {} leaves", have.0, seen.0)
            }
            Alarm::BadConsistency => f.write_str("the consistency proof does not verify"),
            Alarm::NoCheckpoint => f.write_str("no checkpoint was observed yet"),
            Alarm::TooMany => f.write_str("the log has more records than the monitor's limit"),
            Alarm::RecordCount { want, have } => {
                write!(f, "{have} records for a tree of {} leaves", want.0)
            }
            Alarm::RecordsMismatch => {
                f.write_str("the records do not hash to the checkpoint's root")
            }
            Alarm::BadRecord { index, error } => {
                write!(f, "the record of leaf {} is refused: {error}", index.0)
            }
            Alarm::Equivocation { first, second } => {
                write!(
                    f,
                    "one release is in the log at leaves {} and {}",
                    first.0, second.0
                )
            }
            Alarm::Tree(error) => write!(f, "the tree over the records failed: {error}"),
        }
    }
}

impl std::error::Error for Alarm {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Alarm::NotWitnessed(error) => Some(error),
            Alarm::BadRecord { error, .. } => Some(error),
            Alarm::Tree(error) => Some(error),
            _ => None,
        }
    }
}

/// The last checkpoint the monitor verified, with its note (the evidence of a later fork).
struct Seen {
    note: Note,
    checkpoint: Checkpoint,
}

/// Watches one log (see the module documentation).
///
/// Seam: first callers are S45-INT-4 (the leader's watcher) and the phone apps' start check.
pub struct Monitor {
    policy: Policy,
    limit: TreeSize,
    last: Option<Seen>,
    index: BTreeMap<Digest, ReleaseRecord>,
    indexed: Option<TreeSize>,
}

impl fmt::Debug for Monitor {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Monitor")
            .field(
                "checkpoint",
                &self.last.as_ref().map(|seen| seen.checkpoint.size()),
            )
            .field("indexed", &self.indexed)
            .field("records", &self.index.len())
            .finish_non_exhaustive()
    }
}

impl Monitor {
    /// A monitor for the log of `policy`, with the index limit [`MAX_INDEX_RECORDS`].
    pub fn new(policy: Policy) -> Monitor {
        Monitor::with_limit(policy, MAX_INDEX_RECORDS)
    }

    /// A monitor with a lower (or higher) index limit: for a small client, and for the tests of the limit.
    pub fn with_limit(policy: Policy, limit: TreeSize) -> Monitor {
        Monitor {
            policy,
            limit,
            last: None,
            index: BTreeMap::new(),
            indexed: None,
        }
    }

    /// The last checkpoint that passed.
    pub fn checkpoint(&self) -> Option<&Checkpoint> {
        self.last.as_ref().map(|seen| &seen.checkpoint)
    }

    /// The tree size the index covers: the size of the checkpoint at the last successful `ingest`.
    pub fn indexed(&self) -> Option<TreeSize> {
        self.indexed
    }

    /// Takes a checkpoint note (see the module documentation).
    pub fn observe(
        &mut self,
        note: &Note,
        now: UnixTime,
        consistency: Option<&ConsistencyProof>,
    ) -> Result<Observed, Alarm> {
        let witnessed = self.policy.check(note, now).map_err(Alarm::NotWitnessed)?;
        let checkpoint = witnessed.checkpoint().clone();
        let held = self.last.as_ref().map(|seen| {
            (
                seen.checkpoint.size(),
                seen.checkpoint.root(),
                seen.note.clone(),
            )
        });
        let Some((have, root, held_note)) = held else {
            let size = checkpoint.size();
            self.last = Some(Seen {
                note: note.clone(),
                checkpoint,
            });
            return Ok(Observed::First { size });
        };
        if checkpoint.size() < have {
            return Err(Alarm::Shrunk {
                have,
                seen: checkpoint.size(),
            });
        }
        if checkpoint.size() == have {
            return if checkpoint.root() == root {
                Ok(Observed::Unchanged)
            } else {
                Err(Alarm::Fork {
                    a: held_note,
                    b: note.clone(),
                })
            };
        }
        let Some(proof) = consistency else {
            return Ok(Observed::NeedsProof { have });
        };
        if proof.old != have || proof.new != checkpoint.size() {
            return Err(Alarm::BadConsistency);
        }
        verify_consistency(proof, &root, &checkpoint.root()).map_err(|_| Alarm::BadConsistency)?;
        let to = checkpoint.size();
        self.last = Some(Seen {
            note: note.clone(),
            checkpoint,
        });
        Ok(Observed::Grew { from: have, to })
    }

    /// Takes the records of the whole tree, in the order of the leaves (see the module documentation). On
    /// an alarm the index is as it was.
    pub fn ingest(&mut self, records: &[Vec<u8>]) -> Result<(), Alarm> {
        let seen = self.last.as_ref().ok_or(Alarm::NoCheckpoint)?;
        let size = seen.checkpoint.size();
        if size > self.limit {
            return Err(Alarm::TooMany);
        }
        if records.len() as u64 != size.0 {
            return Err(Alarm::RecordCount {
                want: size,
                have: records.len(),
            });
        }
        // Authenticate the list first: only a list that hashes to the checkpoint's root can accuse the log.
        let leaves = records.iter().map(|bytes| leaf_hash(bytes)).collect();
        let root = Tree::new(MemStore::from_leaves(leaves))
            .and_then(|tree| tree.root())
            .map_err(Alarm::Tree)?;
        if root != seen.checkpoint.root() {
            return Err(Alarm::RecordsMismatch);
        }
        let mut index: BTreeMap<Digest, ReleaseRecord> = BTreeMap::new();
        let mut releases: BTreeMap<ReleaseKey, LeafIndex> = BTreeMap::new();
        for (position, bytes) in records.iter().enumerate() {
            let at = LeafIndex(position as u64);
            let record = ReleaseRecord::parse(bytes)
                .map_err(|error| Alarm::BadRecord { index: at, error })?;
            if let Some(first) = releases.insert(record.key(), at) {
                return Err(Alarm::Equivocation { first, second: at });
            }
            // One digest under two names keeps the earlier record: the question is only "does the log hold
            // this digest".
            index.entry(record.digest).or_insert(record);
        }
        self.index = index;
        self.indexed = Some(size);
        Ok(())
    }

    /// The record of the log that has this digest (the earliest, if several do), among the records of the
    /// last ingest.
    pub fn knows(&self, digest: &Digest) -> Option<&ReleaseRecord> {
        self.index.get(digest)
    }
}
