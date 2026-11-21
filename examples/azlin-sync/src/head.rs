//! The vocabulary of a head register: the value, the record, who wrote it, the lock term of a write
//! and the limits of a register (planning/architecture.md section 5.13 "A head never goes backwards",
//! section 2.16 "`heads`: each space's CAS register and its history", section 5.8 "The CAS register
//! keeps every version under a lock", section 1.4 rule 4 "Everything has a limit, stated up front";
//! build unit S16, lane W1-S16a).
//!
//! The value of a head is an opaque small blob that the repository defines (a reference to its newest
//! log entry): this crate never parses it. It is at most [`HEAD_MAX`] bytes (DRIFT X-DELTA-W1S16A-4:
//! the plan's limits table gives the head value no size; 4 KiB is the brief's smallest answer).

use core::fmt;

use azlin_seams::{hash32, ByteCount, DeviceId, Hash32, RestoreEpoch, Stamp};

use crate::version::Version;

/// The largest value of a head: 4 KiB (DRIFT X-DELTA-W1S16A-4: the limits table of section 2.5 does
/// not list the head value; this is the smallest answer and an estimate).
///
/// Implements planning/architecture.md section 2.5 (limits: everything has one) and section 1.4 rule
/// 4.
///
/// Caller: `HeadRegister::cas` (`CasOutcome::TooLarge`), `HeadLimits::new`, `HeadRegister::resume`.
pub const HEAD_MAX: ByteCount = ByteCount::kib(4);

/// What a stored head record costs besides its value: the version, the stamp, the writer, the epoch,
/// the lock stamp and the restored-from version of a row (an estimate of the row `heads` keeps).
///
/// Implements planning/architecture.md section 2.16 (`heads`) and section 1.4 rule 4.
///
/// Caller: `HeadRecord::size`, the history bounds of `HeadRegister`.
pub const HEAD_RECORD_OVERHEAD: ByteCount = ByteCount::new(64);

/// The most versions one register keeps unless it is given another bound (an estimate: a space
/// commits a few hundred times a day, and the longest retention is 365 days).
///
/// Implements planning/architecture.md section 1.4 rule 4 (a table never grows without bound) and
/// section 5.8 (retention 14, 60, 90 or 365 days).
///
/// Caller: `HeadLimits::standard`.
pub const MAX_VERSIONS: usize = 100_000;

/// The most bytes of history one register keeps unless it is given another bound (an estimate).
///
/// Implements planning/architecture.md section 1.4 rule 4 and section 5.8.
///
/// Caller: `HeadLimits::standard`.
pub const MAX_HISTORY_BYTES: ByteCount = ByteCount::mib(64);

/// The hash domain of a head value's hash (a hash domain, not a signature domain: it is registered
/// for uniqueness only, DRIFT X-DELTA-W1S16A-8).
pub(crate) const HEAD_VALUE_DOMAIN: &str = "azlin-sync-head-value-v1";

/// The hash domain of a register's state hash.
pub(crate) const HEAD_STATE_DOMAIN: &str = "azlin-sync-head-state-v1";

/// The value of a head: at most [`HEAD_MAX`] bytes of opaque data that the repository defines.
///
/// The type does not check the size by itself: a value comes off the wire, and the register that
/// receives it answers `CasOutcome::TooLarge` (one decision, one place). `Debug` prints the size and
/// the hash, never the bytes.
///
/// Implements planning/architecture.md section 5.13 (the head register) and section 6.10 A7 (the
/// head is committed by `head_cas`).
///
/// Caller: `HeadRegister::cas`, `resolve_lost_cas` (the value a device sent), `Served::of`, the
/// repository's commit path (SYNC-CLIENT).
#[derive(Clone, PartialEq, Eq)]
pub struct HeadValue(Vec<u8>);

impl HeadValue {
    /// A value of these bytes.
    pub fn new(bytes: Vec<u8>) -> HeadValue {
        HeadValue(bytes)
    }

    /// The bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// The size of the value.
    pub fn size(&self) -> ByteCount {
        ByteCount::of_len(self.0.len())
    }

    /// The hash of the value (`hash32` under the hash domain `azlin-sync-head-value-v1`): what a
    /// device remembers of the value it saw at a version, to tell a server that serves two values at
    /// one version (`Standing::Equivocation`).
    pub fn hash(&self) -> Hash32 {
        hash32(HEAD_VALUE_DOMAIN, &[self.0.as_slice()])
    }
}

impl fmt::Debug for HeadValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "HeadValue({}, {})", self.size(), self.hash())
    }
}

/// Who wrote a head record: a device (a `head_cas`, or a restore an owner asked for through that
/// device) or the operator (a restore the operator asked for through `ctl`, which has no device).
///
/// Implements planning/architecture.md section 5.13 (`head_cas` is by a device) and section 2.17
/// (a restore is asked for by the operator or an owner).
///
/// Caller: `HeadRecord::writer`, `RestoreAuth::requester`, `resolve_lost_cas`; the `heads` table's
/// writer column (SYNC-APPLY).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Writer {
    /// A device.
    Device(DeviceId),
    /// The operator, through `ctl`.
    Operator,
}

/// The lock term of one write: the stamp until which the version it creates must be kept
/// (planning/architecture.md section 5.8: `lock_mode` and `lock_days` give each new object
/// `retain_until`; the CAS register keeps every version under a lock). The caller adds the space's
/// `lock_days` to the leader's stamp (`Stamp::checked_add(Span::days(..))`): this crate does no
/// clock arithmetic on a lock.
///
/// Implements planning/architecture.md section 5.8 (retention and locks).
///
/// Caller: `HeadRegister::cas`; the space's `lock_mode`/`lock_days` come from the registry
/// (SYNC-APPLY).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LockTerm {
    /// No lock: the version is kept for the space's retention and then pruned.
    Unlocked,
    /// The version is kept at least until this stamp, whatever the retention says.
    Until(Stamp),
}

/// One version of a head: what `head_get` answers and what `head_history` lists (the row of the
/// `heads` table, planning/architecture.md section 2.16).
///
/// `restore_epoch` is the restore epoch the register stood in when the record was written: 0 until the
/// first restore; a restore writes a record whose epoch is the restore's epoch (above every epoch the
/// register had), and every later write carries it on (DRIFT X-DELTA-W1S16A-1). `restored_from` names
/// the version whose value a restore record carries: `Some` marks a restore record, so a device never
/// mistakes it for its own write.
///
/// Implements planning/architecture.md section 5.13 (`head_get`, `head_cas`, `head_history`,
/// `restore_as_of`) and section 6.10 V3 (a restore epoch the metadata group signs).
///
/// Caller: `HeadRegister` (every operation), `resolve_lost_cas`, `Served::of`; the wire format is
/// S5's `head_record` (INT-WIRE).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeadRecord {
    /// The version of this record.
    pub version: Version,
    /// The value.
    pub value: HeadValue,
    /// The leader's stamp of the write.
    pub at: Stamp,
    /// Who wrote it.
    pub writer: Writer,
    /// The restore epoch the register stood in when this record was written.
    pub restore_epoch: RestoreEpoch,
    /// The stamp until which this version must be kept (a lock); `None` when it is not locked.
    pub retain_until: Option<Stamp>,
    /// The version whose value this record carries, for a restore record; `None` for a write.
    pub restored_from: Option<Version>,
}

impl HeadRecord {
    /// What the record costs to keep: its value and [`HEAD_RECORD_OVERHEAD`].
    pub fn size(&self) -> ByteCount {
        HEAD_RECORD_OVERHEAD.saturating_add(self.value.size())
    }

    /// Whether this record is a write by `device` (a restore record is nobody's write, even when the
    /// device asked for the restore).
    pub fn is_write_by(&self, device: DeviceId) -> bool {
        self.restored_from.is_none() && self.writer == Writer::Device(device)
    }

    /// Whether a lock still holds this version at `now`: its `retain_until` is after `now`.
    pub fn is_locked(&self, now: Stamp) -> bool {
        matches!(self.retain_until, Some(until) if until > now)
    }
}

/// The bounds of one register: how many versions and how many bytes of history it keeps. At a bound
/// the register refuses the write (`CasOutcome::HistoryFull`) instead of growing (planning/
/// architecture.md section 1.4 rule 4); `prune` frees room as versions pass their retention.
///
/// Implements planning/architecture.md section 1.4 rule 4 and section 5.8.
///
/// Caller: `HeadRegister::with_limits`, `HeadRegister::resume`; the values come from the signed
/// `node_config` (SYNC-APPLY).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HeadLimits {
    versions: usize,
    bytes: ByteCount,
}

impl HeadLimits {
    /// The standard bounds: [`MAX_VERSIONS`] versions and [`MAX_HISTORY_BYTES`] bytes.
    pub const fn standard() -> HeadLimits {
        HeadLimits {
            versions: MAX_VERSIONS,
            bytes: MAX_HISTORY_BYTES,
        }
    }

    /// The bounds of at most `versions` versions and `bytes` bytes; `None` when they cannot hold even
    /// one record of the largest value (no version at all, or fewer bytes than the largest record).
    pub fn new(versions: usize, bytes: ByteCount) -> Option<HeadLimits> {
        let largest = HEAD_MAX.saturating_add(HEAD_RECORD_OVERHEAD);
        if versions == 0 || bytes < largest {
            return None;
        }
        Some(HeadLimits { versions, bytes })
    }

    /// The most versions.
    pub fn versions(&self) -> usize {
        self.versions
    }

    /// The most bytes of history.
    pub fn bytes(&self) -> ByteCount {
        self.bytes
    }
}

/// The fixed-width bytes of a record's fields (everything but the value), for the state hash: the
/// version, the stamp, the writer, the epoch, the lock and the restored-from version, each with a tag
/// where it is optional.
pub(crate) fn record_header(record: &HeadRecord) -> Vec<u8> {
    let mut out = Vec::with_capacity(59);
    out.extend_from_slice(&record.version.get().to_be_bytes());
    out.extend_from_slice(&record.at.as_secs().to_be_bytes());
    match record.writer {
        Writer::Device(device) => {
            out.push(1);
            out.extend_from_slice(device.as_bytes());
        }
        Writer::Operator => {
            out.push(2);
            out.extend_from_slice(&[0u8; 16]);
        }
    }
    out.extend_from_slice(&record.restore_epoch.get().to_be_bytes());
    match record.retain_until {
        Some(until) => {
            out.push(1);
            out.extend_from_slice(&until.as_secs().to_be_bytes());
        }
        None => {
            out.push(0);
            out.extend_from_slice(&[0u8; 8]);
        }
    }
    match record.restored_from {
        Some(version) => {
            out.push(1);
            out.extend_from_slice(&version.get().to_be_bytes());
        }
        None => {
            out.push(0);
            out.extend_from_slice(&[0u8; 8]);
        }
    }
    out
}
