//! The head register: one space's compare-and-swap register and its history (planning/
//! architecture.md section 5.13 "A head never goes backwards", section 2.16 "`heads`: each space's CAS
//! register and its history", section 6.10 A7 "the head by `head_cas` with the expected version";
//! build unit S16, lane W1-S16a).
//!
//! THE STATE MACHINE. A [`HeadRegister`] is deterministic: every voter of the metadata group applies
//! the same entry to the same state and gets the same answer (planning/architecture.md section 2.4),
//! so nothing here reads a clock (the leader's [`Stamp`] is a parameter), draws a random number or
//! does I/O. Every answer is one value of an enum that carries its class (planning/architecture.md
//! section 2.5: `conflict` is retry, `bad_request` is fatal, `retry_after` is after). The `heads`
//! table of `meta.db` persists the records; SYNC-APPLY calls this state machine inside `MetaDb::apply`.
//!
//! THE RULES OF A WRITE (`cas`): the value is at most `HEAD_MAX`; `expected` names the current
//! version (`None` means "create": the register must be empty); the new version is the current one
//! plus one; the new record keeps the restore epoch of the head it replaces; a lock stamp is stored as
//! given. A refused write changes nothing at all: the register is the same, byte for byte.

use core::fmt;
use std::collections::BTreeMap;

use azlin_seams::{hash32, mutant, ByteCount, DeviceId, Hash32, RestoreEpoch, SpaceId, Stamp};

use crate::head::{
    record_header, HeadLimits, HeadRecord, HeadValue, LockTerm, Writer, HEAD_MAX, HEAD_STATE_DOMAIN,
};
use crate::version::Version;

/// What a `head_cas` came to. Every refusal changes nothing.
///
/// Implements planning/architecture.md section 5.13 (`head_cas`), section 2.5 (the statuses and their
/// classes: `conflict` is retry, `bad_request` fatal, `retry_after` after) and section 6.10 A7 (a
/// lost race reads the head again, merges again and retries).
///
/// Caller: `HeadRegister::cas`; S33 derives `head_changed` from `Ok` only (a lost CAS emits no event:
/// DRIFT X-DELTA-W1S16A-6), SYNC-APPLY maps the answer to a status.
#[derive(Clone, Debug, PartialEq, Eq)]
#[must_use]
pub enum CasOutcome {
    /// The write landed: this is the new head (status `ok`).
    Ok {
        /// The record the write created.
        record: HeadRecord,
    },
    /// The expected version is not the current one (status `conflict`, class retry: read the head,
    /// merge, try again). `current` is the head the register holds now, `None` when it is empty.
    Conflict {
        /// The head now.
        current: Option<HeadRecord>,
    },
    /// The value is over `HEAD_MAX` (status `bad_request`, fatal).
    TooLarge {
        /// The size of the value that was sent.
        size: ByteCount,
    },
    /// The register keeps as many versions or bytes as its limits allow (status `retry_after`: the
    /// leader's prune frees room as versions pass their retention).
    HistoryFull,
    /// The register has used every version number (status `unavailable`; it cannot happen in
    /// practice, and a counter never wraps).
    VersionExhausted,
}

/// Why a register cannot be rebuilt from stored records.
///
/// Implements planning/architecture.md section 2.16 (`heads` is persisted by the node) and section
/// 1.4 rule 11 (a broken invariant is found, not written over).
///
/// Caller: `HeadRegister::resume` (SYNC-APPLY, when it loads the rows of a space or installs a
/// snapshot).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ResumeRefusal {
    /// The versions are not strictly ascending: this version is not above the one before it.
    #[error("version {version} is not above the version before it")]
    NotAscending {
        /// The offending version.
        version: Version,
    },
    /// A record's restore epoch is below the epoch of the record before it.
    #[error("the restore epoch of version {version} is below the one before it")]
    EpochGoesDown {
        /// The offending version.
        version: Version,
    },
    /// A value is over `HEAD_MAX`.
    #[error("the value of version {version} is over the limit of a head value")]
    TooLarge {
        /// The offending version.
        version: Version,
    },
    /// A restore record names a version that is not below its own.
    #[error("version {version} claims to restore a version that is not below it")]
    BadRestoredFrom {
        /// The offending version.
        version: Version,
    },
    /// The records are more than the limits allow.
    #[error("the records are more than the limits of the register allow")]
    OverLimit,
}

/// One space's head register: the CAS register and its history, oldest first, the last record being
/// the current head. The state machine of the `heads` table.
///
/// Implements planning/architecture.md section 5.13 (`head_get`, `head_cas`, `head_history`,
/// `restore_as_of`) and section 2.16 (`heads`).
///
/// Caller: SYNC-APPLY (inside the Raft apply path of the node); S33 (events from `CasOutcome::Ok`);
/// the tests of S47c (the restore cases).
#[derive(Clone, PartialEq, Eq)]
pub struct HeadRegister {
    pub(crate) space: SpaceId,
    pub(crate) records: BTreeMap<Version, HeadRecord>,
    pub(crate) bytes: ByteCount,
    pub(crate) limits: HeadLimits,
    pub(crate) legal_hold: bool,
}

impl fmt::Debug for HeadRegister {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "HeadRegister(space {}, {} versions, {}, head {:?})",
            self.space,
            self.records.len(),
            self.bytes,
            self.get().map(|record| record.version)
        )
    }
}

impl HeadRegister {
    /// An empty register of `space` with the standard limits.
    pub fn new(space: SpaceId) -> HeadRegister {
        HeadRegister::with_limits(space, HeadLimits::standard())
    }

    /// An empty register of `space` with these limits.
    pub fn with_limits(space: SpaceId, limits: HeadLimits) -> HeadRegister {
        HeadRegister {
            space,
            records: BTreeMap::new(),
            bytes: ByteCount::ZERO,
            limits,
            legal_hold: false,
        }
    }

    /// Rebuilds a register from stored records (oldest first): the rows of `heads`, or a snapshot a
    /// new voter installs. The records are checked, never trusted: strictly ascending versions,
    /// non-decreasing restore epochs, values within `HEAD_MAX`, restore records that name an older
    /// version, and the whole within `limits`.
    pub fn resume(
        space: SpaceId,
        limits: HeadLimits,
        legal_hold: bool,
        records: Vec<HeadRecord>,
    ) -> Result<HeadRegister, ResumeRefusal> {
        let mut map = BTreeMap::new();
        let mut bytes = ByteCount::ZERO;
        let mut previous: Option<(Version, RestoreEpoch)> = None;
        for record in records {
            if let Some((version, epoch)) = previous {
                if record.version <= version {
                    return Err(ResumeRefusal::NotAscending {
                        version: record.version,
                    });
                }
                if record.restore_epoch < epoch {
                    return Err(ResumeRefusal::EpochGoesDown {
                        version: record.version,
                    });
                }
            }
            if record.value.size() > HEAD_MAX {
                return Err(ResumeRefusal::TooLarge {
                    version: record.version,
                });
            }
            if let Some(from) = record.restored_from {
                if from >= record.version {
                    return Err(ResumeRefusal::BadRestoredFrom {
                        version: record.version,
                    });
                }
            }
            bytes = match bytes.checked_add(record.size()) {
                Some(total) => total,
                None => return Err(ResumeRefusal::OverLimit),
            };
            previous = Some((record.version, record.restore_epoch));
            map.insert(record.version, record);
        }
        if map.len() > limits.versions() || bytes > limits.bytes() {
            return Err(ResumeRefusal::OverLimit);
        }
        Ok(HeadRegister {
            space,
            records: map,
            bytes,
            limits,
            legal_hold,
        })
    }

    /// The space this register belongs to.
    pub fn space(&self) -> SpaceId {
        self.space
    }

    /// The current head: the newest record; `None` for a register nobody has written.
    pub fn get(&self) -> Option<&HeadRecord> {
        self.records.values().next_back()
    }

    /// The record of one version, if the register still holds it.
    pub fn get_version(&self, version: Version) -> Option<&HeadRecord> {
        self.records.get(&version)
    }

    /// The records the register holds, oldest first (for the rows of `heads`).
    pub fn records(&self) -> impl Iterator<Item = &HeadRecord> {
        self.records.values()
    }

    /// How many versions the register holds.
    pub fn versions_held(&self) -> usize {
        self.records.len()
    }

    /// How many bytes of history the register holds.
    pub fn bytes_held(&self) -> ByteCount {
        self.bytes
    }

    /// The limits of the register.
    pub fn limits(&self) -> HeadLimits {
        self.limits
    }

    /// Whether the space is under a legal hold: `prune` drops nothing while it is (planning/
    /// architecture.md section 5.8: nodes refuse deletes and overwrites under a hold).
    pub fn legal_hold(&self) -> bool {
        self.legal_hold
    }

    /// Sets or lifts the legal hold of the space (the `legal_hold` verb applies it: it is part of the
    /// replicated state, so it is part of the state hash).
    pub fn set_legal_hold(&mut self, held: bool) {
        self.legal_hold = held;
    }

    /// Compare-and-swap: writes `new` as the next version when `expected` is the current version
    /// (`None` creates the register's first version). `at` is the leader's stamp; `lock` is the lock
    /// term of the new version. A refused write changes nothing.
    ///
    /// The order of the decisions is fixed: the size of the value, then the expected version, then the
    /// version counter, then the limits of the register.
    pub fn cas(
        &mut self,
        expected: Option<Version>,
        new: HeadValue,
        writer: DeviceId,
        at: Stamp,
        lock: LockTerm,
    ) -> CasOutcome {
        if new.size() > HEAD_MAX && !mutant!("sync_cas_size_unchecked") {
            return CasOutcome::TooLarge { size: new.size() };
        }
        let current = self.records.values().next_back();
        let current_version = current.map(|record| record.version);
        let agrees = expected == current_version
            || (expected.is_none() && mutant!("sync_cas_create_overwrites"));
        if !agrees && !mutant!("sync_cas_expected_unchecked") {
            return CasOutcome::Conflict {
                current: current.cloned(),
            };
        }
        let version = match current_version {
            None => Version::FIRST,
            Some(version) => match version.next() {
                Some(next) if mutant!("sync_cas_version_skips") => match next.next() {
                    Some(skipped) => skipped,
                    None => return CasOutcome::VersionExhausted,
                },
                Some(next) => next,
                None => return CasOutcome::VersionExhausted,
            },
        };
        let restore_epoch = match current {
            Some(record) => record.restore_epoch,
            None => RestoreEpoch::ZERO,
        };
        let record = HeadRecord {
            version,
            value: new,
            at,
            writer: Writer::Device(writer),
            restore_epoch,
            retain_until: match lock {
                LockTerm::Unlocked => None,
                LockTerm::Until(until) => Some(until),
            },
            restored_from: None,
        };
        if !self.has_room_for(&record) && !mutant!("sync_cas_limits_unchecked") {
            return CasOutcome::HistoryFull;
        }
        self.insert(record.clone());
        CasOutcome::Ok { record }
    }

    /// Whether one more record fits the limits of the register.
    pub(crate) fn has_room_for(&self, record: &HeadRecord) -> bool {
        if self.records.len() >= self.limits.versions() {
            return false;
        }
        match self.bytes.checked_add(record.size()) {
            Some(total) => total <= self.limits.bytes(),
            None => false,
        }
    }

    /// Stores a record that the caller has checked: the new head.
    pub(crate) fn insert(&mut self, record: HeadRecord) {
        self.bytes = self.bytes.saturating_add(record.size());
        self.records.insert(record.version, record);
    }

    /// A hash of the whole state: the space, the legal hold and every record, in order. Two replicas
    /// that applied the same entries have the same hash; a replica that diverged in any field of any
    /// record has another one. It reads the whole history: it is for snapshots and checks, not for
    /// the request path.
    pub fn state_hash(&self) -> Hash32 {
        let headers: Vec<Vec<u8>> = self.records.values().map(record_header).collect();
        let space: &[u8] = self.space.as_bytes();
        let hold: [u8; 1] = [u8::from(self.legal_hold)];
        // `usize` fits `u64` on every supported target (the assertion in azlin-seams).
        let count = (self.records.len() as u64).to_be_bytes();
        let mut parts: Vec<&[u8]> = Vec::with_capacity(3 + 2 * headers.len());
        parts.push(space);
        parts.push(hold.as_slice());
        parts.push(count.as_slice());
        for (header, record) in headers.iter().zip(self.records.values()) {
            parts.push(header.as_slice());
            parts.push(record.value.as_bytes());
        }
        hash32(HEAD_STATE_DOMAIN, &parts)
    }
}
