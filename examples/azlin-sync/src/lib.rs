//! Azlin sync: the state machines of space heads, append logs and the device's head guard
//! (planning/architecture.md section 5.13 "Sync: events, watch, push, append logs, peers", section 2.16
//! "`heads` and `logs` in meta.db", section 2.5 "the verbs and their limits", section 6.10 S15 and V3,
//! section 12 E4; build unit S16, lane W1-S16a).
//!
//! Three things the metadata group keeps and the devices check, as pure logic:
//!
//! | what | module | the plan |
//! |---|---|---|
//! | a space's compare-and-swap register: [`HeadRegister`], [`CasOutcome`], [`HeadRecord`] | [`cas`], [`head`], [`version`] | 5.13, 2.16 |
//! | its history and the retention prune: [`HistoryPage`], [`Pruned`] | [`history`] | 5.13, 5.8 |
//! | restoring a head as of a moment: [`RestoreAuth`], [`RestoredHead`] | [`restore`] | 5.13, 2.17, 6.10 V3 |
//! | after a lost reply: did my write land? [`resolve_lost_cas`], [`LostCas`] | [`lost_reply`] | 2.5, 6.10 A7 |
//! | the server-sequenced append logs: [`AppendLog`], [`Record`], [`ReadPage`] | [`log`] | 5.13, 2.5 |
//! | the account's cap on uncompacted log bytes: [`AccountLogQuota`] | [`quota`] | 5.13, 2.5 |
//! | compaction of a log into a pack: [`PackCovers`] | [`compact`] | 5.13 |
//! | the device's rule "a head never goes backwards": [`judge`], [`Standing`], [`Seen`] | [`guard`] | 5.13, 6.10 S15 V3, 12 E4 |
//! | the validation of a `watch` subscription: [`validate_subscription`] | [`watch`] | 5.13, 2.5 |
//! | what the registry is asked: [`SpaceAccess`] | [`access`] | 5.13, 5.11 |
//!
//! THE SERVER HALF (`HeadRegister`, `AppendLog`, `AccountLogQuota`) is deterministic and is called inside
//! the Raft apply path: every voter applies the same entry to the same state and must get the same answer
//! and the same state (`state_hash` says so). It reads no clock (the leader's [`Stamp`] is a parameter of
//! every operation that needs a time), draws no random number and does no I/O. The `heads` and `logs`
//! tables of `meta.db` persist what it holds; the node's glue (SYNC-APPLY) loads and saves the rows and
//! maps each refusal to its status (planning/architecture.md section 2.5). THE DEVICE HALF (`judge`,
//! `resolve_lost_cas`) runs in the client and is pure too.
//!
//! THE RULES, in one place. A write to a head names the version it expects, and exactly one of the
//! writers that expect the same version wins. A head never goes backwards: the version only grows, a
//! point-in-time restore writes a NEW, higher version with the old value and a higher restore epoch,
//! and a device refuses a head older than the newest it has seen unless a restore the metadata group
//! signed (the map key's signature, E4) raised the epoch and covers the space. A log is numbered by the
//! server, each record carries its author and a signature, and the account's uncompacted bytes are capped
//! so that no device that compacts nothing can fill the voters' disks. Everything has a limit stated up
//! front (planning/architecture.md section 1.4 rule 4): the size of a head value, the versions and bytes
//! a register keeps, the size of a record, the bytes of an account's logs, the size of a page, the spaces
//! of a subscription.
//!
//! Mutation guards (`azlin_seams::mutant!`, the feature `azlin-seams/mutants`) sit at each of these
//! decisions; the mutant hold of this crate turns each one off and watches a test fail (the table is in
//! `tests/hygiene.rs`).
//!
//! DRIFT (the register is `delta/W1S16A.tsv`, ids `X-DELTA-W1S16A-<n>`): the epoch of a head and of a
//! restore (-1), the rows of the device guard for heads (-2), log names (-3), the head value size (-4),
//! paging (-5), events from the entry and its outcome (-6), what a record signs (-7), hash domains (-8),
//! compaction (-9), `baseline` and what "covers" means (-10), the key of a record's author (-11), the
//! items beyond the brief (-12), the restore authorization (-13), the golden fixtures (-14), the first
//! look of a pairing (-15).
//!
//! ```
//! use azlin_seams::{DeviceId, SpaceId, UnixTime};
//! use azlin_sync::{CasOutcome, HeadRegister, HeadValue, LockTerm, Version};
//!
//! let mut register = HeadRegister::new(SpaceId::from_bytes([1; 16]));
//! let device = DeviceId::from_bytes([2; 16]);
//! let at = UnixTime::from_secs(100);
//! // The first write expects nothing and becomes version 1.
//! let first = register.cas(None, HeadValue::new(b"a".to_vec()), device, at, LockTerm::Unlocked);
//! assert!(matches!(first, CasOutcome::Ok { ref record } if record.version == Version::FIRST));
//! // A write that expects a version that is not the head's loses, and learns the head.
//! let late = register.cas(None, HeadValue::new(b"b".to_vec()), device, at, LockTerm::Unlocked);
//! assert!(matches!(late, CasOutcome::Conflict { current: Some(_) }));
//! ```

#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented,
        clippy::indexing_slicing
    )
)]
#![warn(missing_docs)]

// A `usize` fits a `u64` on every target this engine builds for: the widening casts of the counts rest
// on it, so a target where it fails does not compile (the same assertion as azlin-seams).
const _: () = assert!(usize::BITS <= u64::BITS);

pub mod access;
pub mod cas;
pub mod compact;
pub mod guard;
pub mod head;
pub mod history;
pub mod log;
pub mod lost_reply;
pub mod quota;
pub mod restore;
pub mod version;
pub mod watch;

pub use access::SpaceAccess;
pub use cas::{CasOutcome, HeadRegister, ResumeRefusal};
pub use compact::{CompactRefusal, Freed, PackCovers};
pub use guard::{baseline, judge, Seen, Served, Standing};
pub use head::{
    HeadLimits, HeadRecord, HeadValue, LockTerm, Writer, HEAD_MAX, HEAD_RECORD_OVERHEAD,
    MAX_HISTORY_BYTES, MAX_VERSIONS,
};
pub use history::{HistoryPage, Pruned, HISTORY_PAGE_BYTES, HISTORY_PAGE_MAX};
pub use log::{
    record_preimage, AppendLog, AppendRefusal, Appended, LogId, LogResumeRefusal, LoggedRecord,
    ReadPage, Record, Seq, LOG_RECORD_V1, READ_PAGE_BYTES, READ_PAGE_MAX, RECORD_MAX,
    RECORD_OVERHEAD, SIG_MAX,
};
pub use lost_reply::{resolve_lost_cas, LostCas};
pub use quota::{AccountLogQuota, CompactNotice, LOG_CAP};
pub use restore::{
    Requester, RestoreAsOfRefusal, RestoreAuth, RestoreRequest, RestoreRequestVerifier,
    RestoredHead,
};
pub use version::Version;
pub use watch::{validate_subscription, Accepted, Level, WatchRefusal, MAX_SPACES};

// The types of the vocabulary that appear in the public API, from the one crate that names them.
pub use azlin_seams::Stamp;
