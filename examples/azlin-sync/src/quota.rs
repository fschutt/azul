//! The account's quota for uncompacted append-log bytes (planning/architecture.md section 5.13
//! "The voters' SSD must not fill when no device compacts (a bot appending all day, a business with
//! no admin device online for months), so uncompacted bytes count against the account's quota and are
//! capped per account (2.5)", section 2.5 limits "uncompacted append-log bytes per account (they count
//! against its quota) | 1 GiB | `quota_exceeded`; the account's devices get a notice to compact";
//! build unit S16, lane W1-S16a).
//!
//! ONE quota per ACCOUNT, shared by every log of the account: a bot that appends to a hundred logs
//! fills one gibibyte, not a hundred. Every append charges the bytes it stores, every compaction
//! releases the bytes it dropped, and a charge that would pass the cap changes nothing and returns the
//! notice that tells the account's devices to compact. The bytes also count against the account's
//! plan quota (the registry adds [`AccountLogQuota::used`] to its usage).

use azlin_seams::{hash32, mutant, AccountId, ByteCount, Hash32};

/// The default cap on the uncompacted append-log bytes of one account: 1 GiB (the limits table of
/// planning/architecture.md section 2.5; an estimate).
///
/// Implements planning/architecture.md section 2.5 (limits) and section 5.13 (append logs).
///
/// Caller: `AccountLogQuota::new`.
pub const LOG_CAP: ByteCount = ByteCount::gib(1);

/// The hash domain of a quota's state hash.
pub(crate) const QUOTA_STATE_DOMAIN: &str = "azlin-sync-quota-state-v1";

/// The notice the account's devices get when their append logs are full: compact (planning/
/// architecture.md section 5.13). It names the account and the figures at the moment of the
/// refusal; delivery (an inbox item, an event) is the node's.
///
/// Implements planning/architecture.md section 2.5 (`quota_exceeded`; the account's devices get a
/// notice to compact) and section 5.13.
///
/// Caller: `AppendRefusal::QuotaExceeded`, `AccountLogQuota::notice`; S33 / SYNC-APPLY deliver it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CompactNotice {
    /// The account whose logs are full.
    pub account: AccountId,
    /// The uncompacted bytes the account's logs hold.
    pub used: ByteCount,
    /// The cap on them.
    pub cap: ByteCount,
}

/// The uncompacted bytes of ALL the account's append logs, and the cap on them.
///
/// Implements planning/architecture.md section 5.13 (capped per account) and section 2.5 (limits).
///
/// Caller: `AppendLog::append` and `AppendLog::compact_through` (SYNC-APPLY holds one per account
/// and passes it to every log of the account).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountLogQuota {
    account: AccountId,
    used: ByteCount,
    cap: ByteCount,
}

/// A release of more bytes than the quota holds: the log and the quota do not belong together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Underflow;

impl AccountLogQuota {
    /// An empty quota of `account` with the default cap ([`LOG_CAP`]).
    pub fn new(account: AccountId) -> AccountLogQuota {
        AccountLogQuota::with_cap(account, LOG_CAP)
    }

    /// An empty quota of `account` with this cap (from the signed `node_config`).
    pub fn with_cap(account: AccountId, cap: ByteCount) -> AccountLogQuota {
        AccountLogQuota {
            account,
            used: ByteCount::ZERO,
            cap,
        }
    }

    /// A quota rebuilt from stored state: the account, its cap and the bytes in use. A `used` above
    /// the cap (the cap was lowered) is allowed: appends are refused until compaction brings it
    /// down.
    pub fn resume(account: AccountId, cap: ByteCount, used: ByteCount) -> AccountLogQuota {
        AccountLogQuota { account, used, cap }
    }

    /// The account the quota belongs to.
    pub fn account(&self) -> AccountId {
        self.account
    }

    /// The uncompacted bytes in use.
    pub fn used(&self) -> ByteCount {
        self.used
    }

    /// The cap.
    pub fn cap(&self) -> ByteCount {
        self.cap
    }

    /// The notice for the figures as they are now.
    pub fn notice(&self) -> CompactNotice {
        CompactNotice {
            account: self.account,
            used: self.used,
            cap: self.cap,
        }
    }

    /// Charges `bytes` against the cap: `Ok` and the bytes are counted, or the notice and nothing is
    /// counted.
    pub(crate) fn charge(&mut self, bytes: ByteCount) -> Result<(), CompactNotice> {
        self.charge_counting(bytes, self.used)
    }

    /// Charges `bytes`, comparing `counted + bytes` (rather than the quota's own `used`) with the cap:
    /// the normal charge passes its own `used`; the mutant of a PER-LOG cap passes the log's.
    pub(crate) fn charge_counting(
        &mut self,
        bytes: ByteCount,
        counted: ByteCount,
    ) -> Result<(), CompactNotice> {
        let Some(after) = self.used.checked_add(bytes) else {
            return Err(self.notice());
        };
        let against_cap = match counted.checked_add(bytes) {
            Some(total) => total,
            None => return Err(self.notice()),
        };
        if against_cap > self.cap && !mutant!("sync_log_quota_unchecked") {
            return Err(self.notice());
        }
        self.used = after;
        Ok(())
    }

    /// Whether `bytes` can be released: the quota holds at least that many.
    pub(crate) fn can_release(&self, bytes: ByteCount) -> bool {
        self.used >= bytes
    }

    /// Releases `bytes` (a compaction dropped them); a release of more than the quota holds changes
    /// nothing and is an error.
    pub(crate) fn release(&mut self, bytes: ByteCount) -> Result<(), Underflow> {
        match self.used.checked_sub(bytes) {
            Some(rest) => {
                self.used = rest;
                Ok(())
            }
            None => Err(Underflow),
        }
    }

    /// A hash of the quota's state (account, cap, used): equal on replicas that applied the same
    /// entries.
    pub fn state_hash(&self) -> Hash32 {
        let used = self.used.get().to_be_bytes();
        let cap = self.cap.get().to_be_bytes();
        let account: &[u8] = self.account.as_bytes();
        hash32(
            QUOTA_STATE_DOMAIN,
            &[account, used.as_slice(), cap.as_slice()],
        )
    }
}
