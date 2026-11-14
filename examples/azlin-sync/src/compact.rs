//! Compaction of an append log: clients write the records into a pack, and the leader drops them from the
//! log in the same batch (planning/architecture.md section 5.13 "server-sequenced small sealed records
//! kept in the metadata group, compacted by clients into packs", section 2.16 "`logs`: the append logs'
//! small records before compaction", section 2.5 limits "uncompacted append-log bytes per account";
//! build unit S16, lane W1-S16a).
//!
//! NO VERB COMPACTS A LOG (DRIFT X-DELTA-W1S16A-9, the brief's drift 2): the verb table of section 2.5
//! has `log_append` and `log_read` only. The smallest answer is that `pack_commit` carries
//! `covers_log: (log id, through seq)` and the leader calls [`AppendLog::compact_through`] in the same
//! batch; the verb table's change is S5's (X-D-03). The leader makes a [`PackCovers`] from the
//! `pack_commit` it has applied, and compaction needs it: records are dropped only up to what a
//! committed pack covers, never beyond, and only from the log the pack names.
//!
//! WHAT THE PROOF IS AND IS NOT. The leader cannot read a pack (it is sealed), so a [`PackCovers`] is
//! the claim of the device that committed the pack, checked for authority by the node (the device may
//! append to the space) and for existence by the node (the pack was committed): a device that may
//! append can claim a pack covers records it does not hold. That is the trust the plan gives a writer
//! of the space, and the sequence numbers and the signatures let the readers find what is missing.

use azlin_seams::{mutant, ByteCount};

use crate::log::{AppendLog, LogId, Seq};
use crate::quota::AccountLogQuota;

/// The proof that a committed pack covers a log up to a sequence number: the leader makes it from a
/// `pack_commit` it has applied. It has private fields and no `Clone`, `Default` or `From`: the only
/// maker is [`PackCovers::from_commit`], and `AppendLog::compact_through` consumes no more than it
/// covers.
///
/// ```compile_fail,E0451
/// // A pack proof cannot be built by hand.
/// use azlin_seams::SpaceId;
/// use azlin_sync::{LogId, PackCovers, Seq};
/// let log = LogId { space: SpaceId::from_bytes([1; 16]), name: [2; 16] };
/// let _forged = PackCovers { log, through: Seq::FIRST };
/// ```
///
/// Implements planning/architecture.md section 5.13 (records are compacted by clients into packs) and
/// section 2.5 (`pack_commit`; DRIFT X-DELTA-W1S16A-9: it carries `covers_log`).
///
/// Caller: `AppendLog::compact_through`; made by SYNC-APPLY in the batch of a `pack_commit` whose
/// `covers_log` names this log.
#[derive(Debug)]
pub struct PackCovers {
    log: LogId,
    through: Seq,
}

impl PackCovers {
    /// The proof for a `pack_commit` the leader has applied, whose `covers_log` says that the pack
    /// holds the records of `log` through `through`. Call it only for such a commit.
    pub fn from_commit(log: LogId, through: Seq) -> PackCovers {
        PackCovers { log, through }
    }

    /// The log the pack covers.
    pub fn log(&self) -> LogId {
        self.log
    }

    /// The last sequence number the pack covers.
    pub fn through(&self) -> Seq {
        self.through
    }
}

/// What a compaction freed.
///
/// Implements planning/architecture.md section 5.13 (uncompacted bytes count against the account's
/// quota: compaction frees them).
///
/// Caller: `AppendLog::compact_through`; SYNC-APPLY deletes the rows that were dropped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Freed {
    /// How many records were dropped.
    pub records: u64,
    /// The bytes they cost, released from the account's quota.
    pub bytes: ByteCount,
}

/// Why a compaction is refused. A refusal changes nothing.
///
/// Implements planning/architecture.md section 5.13 and section 2.5 (the statuses: all are
/// `bad_request` but `QuotaUnderflow`, a fault of the node).
///
/// Caller: `AppendLog::compact_through`; SYNC-APPLY maps it to a status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CompactRefusal {
    /// The proof is for another log.
    #[error("the pack proof is for another log")]
    WrongLog,
    /// The compaction asks for more than the pack covers.
    #[error("the compaction goes beyond what the pack covers")]
    BeyondProof,
    /// The compaction asks for records the log has not got.
    #[error("the compaction goes beyond the end of the log")]
    BeyondEnd,
    /// The quota is another account's: the node wired the log to the wrong quota.
    #[error("the quota belongs to another account")]
    WrongQuota,
    /// The quota holds fewer bytes than the compaction would free: the quota and the log do not belong
    /// together.
    #[error("the quota holds fewer bytes than the records to drop")]
    QuotaUnderflow,
}

impl AppendLog {
    /// Drops the records with a sequence number up to and including `through` (they are in a pack now)
    /// and releases their bytes from `q`. The proof must be for this log and cover `through`; the log
    /// must hold `through`. A `through` at or below what is already compacted frees nothing and is
    /// not an error (a repeated batch changes nothing). A refusal changes nothing.
    pub fn compact_through(
        &mut self,
        through: Seq,
        proof: &PackCovers,
        q: &mut AccountLogQuota,
    ) -> Result<Freed, CompactRefusal> {
        if proof.log != self.id && !mutant!("sync_compact_wrong_log_allowed") {
            return Err(CompactRefusal::WrongLog);
        }
        if through > proof.through && !mutant!("sync_compact_beyond_proof_allowed") {
            return Err(CompactRefusal::BeyondProof);
        }
        if q.account() != self.account {
            return Err(CompactRefusal::WrongQuota);
        }
        let end = match self.last {
            Some(last) => last,
            None => return Err(CompactRefusal::BeyondEnd),
        };
        if through > end {
            return Err(CompactRefusal::BeyondEnd);
        }
        if let Some(done) = self.compacted {
            if through <= done {
                return Ok(Freed {
                    records: 0,
                    bytes: ByteCount::ZERO,
                });
            }
        }
        // What would go, counted before anything changes.
        let mut doomed: Vec<Seq> = Vec::new();
        let mut bytes = ByteCount::ZERO;
        for (seq, logged) in self.records.range(..=through) {
            doomed.push(*seq);
            bytes = bytes.saturating_add(logged.record.size());
        }
        if !q.can_release(bytes) {
            return Err(CompactRefusal::QuotaUnderflow);
        }
        // The quota first: it cannot refuse (checked above), so the log is changed only after it.
        if !mutant!("sync_compact_quota_not_released") && q.release(bytes).is_err() {
            return Err(CompactRefusal::QuotaUnderflow);
        }
        for seq in &doomed {
            self.records.remove(seq);
        }
        self.bytes = self.bytes.saturating_sub(bytes);
        self.compacted = Some(through);
        Ok(Freed {
            // `usize` fits `u64` on every supported target (the assertion in azlin-seams).
            records: doomed.len() as u64,
            bytes,
        })
    }
}
