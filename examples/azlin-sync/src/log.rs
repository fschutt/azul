//! The server-sequenced append logs: `log_append` and `log_read` (planning/architecture.md section
//! 5.13 "Append logs (`log_append`, `log_read`): server-sequenced small sealed records kept in the
//! metadata group, compacted by clients into packs. They serve chat, audit segments and mail state
//! markers. One CAS head per busy channel would make every append race ... Every record carries
//! `author_device` and a signature", section 2.5 limits "`log_append` record | 64 KiB | `bad_request`",
//! section 2.16 "`logs`: the append logs' small records before compaction"; build unit S16, lane
//! W1-S16a).
//!
//! THE STATE MACHINE. An [`AppendLog`] is one log: records numbered 1, 2, 3, ... in the order the
//! leader applies them (the number is the server's, never the client's), a stamp from the leader, and a
//! prefix that clients have compacted into packs. It is deterministic (every voter applies the same
//! entry to the same state), reads no clock and does no I/O. Persistence is the `logs` table's
//! (SYNC-APPLY); [`AppendLog::resume`] rebuilds a log from its rows and refuses rows that do not hold
//! together.
//!
//! THE CHECKS OF AN APPEND, in order, each with its own refusal (a refused append changes nothing, not
//! the log and not the quota): the quota is the account's own; the record is within its limits; the
//! author may append to the space; the author's signature verifies; the sequence number exists; the
//! account's uncompacted bytes stay within the cap.
//!
//! WHAT THE AUTHOR SIGNS (DRIFT X-DELTA-W1S16A-7: the plan says only "every record carries
//! `author_device` and a signature"): the preimage is `signing_input(LOG_RECORD_V1, space || name ||
//! author || body)` ([`record_preimage`]), so a record cannot be moved into another log, another space
//! or under another author. The sequence number and the stamp are the server's and are not signed. The
//! byte-exact record is S5's (`log_record`); this layout is provisional and is replaced at INT-WIRE.
//! The record carries no nonce: a byte-identical record appended twice appears twice, and the sealed
//! body is expected to carry the message identity the readers deduplicate by (DRIFT X-DELTA-W1S16A-7).

use core::fmt;
use core::num::NonZeroU64;
use std::collections::BTreeMap;

use azlin_seams::{
    hash32, mutant, signing_input, AccountId, ByteCount, DeviceId, Domain, Hash32, SigAlg,
    SignatureVerifier, SpaceId, Stamp, VerifyError,
};

use crate::access::SpaceAccess;
use crate::quota::{AccountLogQuota, CompactNotice};

/// The domain of an append-log record's signature: `azlin-log-record-v1` (planning/architecture.md
/// section 2.5: signed structures each have their own domain). `azlin-keys` defines the same string
/// (`azlin_keys::sign::LOG_RECORD_V1`); this crate has no dependency on it, and a test pins the
/// spelling (DRIFT X-DELTA-W1S16A-7).
///
/// Implements planning/architecture.md section 2.5 (the domain of a signed structure) and section 5.13
/// (every record carries `author_device` and a signature).
///
/// Caller: `record_preimage`, hence `AppendLog::append`; the clients that sign records.
pub const LOG_RECORD_V1: Domain = Domain::new("azlin-log-record-v1");

/// The largest body of a record: 64 KiB (the limits table of planning/architecture.md section 2.5).
///
/// Implements planning/architecture.md section 2.5 (limits: `log_append` record) and section 1.4 rule
/// 4.
///
/// Caller: `AppendLog::append` (`AppendRefusal::TooLarge`), `AppendLog::resume`.
pub const RECORD_MAX: ByteCount = ByteCount::kib(64);

/// The largest signature a record may carry (an estimate: Ed25519's is 64 bytes; room is left for a
/// post-quantum scheme, whose signatures are a few kibibytes).
///
/// Implements planning/architecture.md section 1.4 rule 4 and section 5.13.
///
/// Caller: `AppendLog::append` (`AppendRefusal::TooLarge`), `AppendLog::resume`.
pub const SIG_MAX: ByteCount = ByteCount::kib(8);

/// What a stored record costs besides its body and signature: the author, the sequence number and the
/// stamp of a row (an estimate of the row `logs` keeps). A record is charged to the quota at this
/// plus its body and signature.
///
/// Implements planning/architecture.md section 2.16 (`logs`) and section 5.13 (uncompacted bytes count).
///
/// Caller: `Record::size`.
pub const RECORD_OVERHEAD: ByteCount = ByteCount::new(32);

/// The most records of one `log_read` page.
///
/// Implements planning/architecture.md section 1.4 rule 4 and section 5.13 (`log_read`); an estimate
/// (DRIFT X-DELTA-W1S16A-5).
///
/// Caller: `AppendLog::read`.
pub const READ_PAGE_MAX: u16 = 256;

/// The most bytes of one `log_read` page (the records' sizes, `Record::size`), whatever the reader
/// asked for.
///
/// Implements planning/architecture.md section 1.4 rule 4 and section 5.13 (`log_read`); an estimate
/// (DRIFT X-DELTA-W1S16A-5).
///
/// Caller: `AppendLog::read`.
pub const READ_PAGE_BYTES: ByteCount = ByteCount::mib(1);

/// The hash domain of a log's state hash.
pub(crate) const LOG_STATE_DOMAIN: &str = "azlin-sync-log-state-v1";

/// The sequence number of a record in its log: positive, assigned by the server, one above the record
/// before it (the first record of a log is 1).
///
/// Implements planning/architecture.md section 5.13 (server-sequenced records) and section 2.17 (a
/// restore re-anchors the append-log sequence numbers, so chat never reuses one).
///
/// Caller: `AppendLog` (every record), `ReadPage`, `PackCovers`; SYNC-CLIENT (the next position to
/// read).
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Seq(NonZeroU64);

impl Seq {
    /// The sequence number of the first record of a log.
    pub const FIRST: Seq = Seq(NonZeroU64::MIN);

    /// The sequence number with this number; `None` for 0 (there is no record 0).
    #[allow(clippy::manual_map)] // `Option::map` is not a `const fn`
    pub const fn new(number: u64) -> Option<Seq> {
        match NonZeroU64::new(number) {
            Some(number) => Some(Seq(number)),
            None => None,
        }
    }

    /// The number.
    pub const fn get(self) -> u64 {
        self.0.get()
    }

    /// The sequence number after this one; `None` at the largest number (a counter never wraps).
    pub fn next(self) -> Option<Seq> {
        self.0.get().checked_add(1).and_then(Seq::new)
    }
}

/// Decimal.
impl fmt::Display for Seq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl fmt::Debug for Seq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Seq({})", self.0)
    }
}

/// Which log: a space and a 16-byte name (DRIFT X-DELTA-W1S16A-3: a space can hold several logs, a
/// page's, a link's comments, the `system_audit` space's; the brief's smallest answer is a name of 16
/// bytes).
///
/// Implements planning/architecture.md section 5.13 (append logs) and section 5.12 (a link's comment
/// log), section 5.14 (the `system_audit` space).
///
/// Caller: `AppendLog`, `record_preimage`, `PackCovers`; the `log_append` and `log_read` verbs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LogId {
    /// The space the log belongs to.
    pub space: SpaceId,
    /// The name of the log inside the space.
    pub name: [u8; 16],
}

/// One record as a device sends it: the author, the sealed body (at most [`RECORD_MAX`] bytes) and the
/// author's signature over [`record_preimage`].
///
/// Implements planning/architecture.md section 5.13 (every record carries `author_device` and a
/// signature) and section 2.5 (the limit of a record).
///
/// Caller: `AppendLog::append`; the clients that append (SYNC-CLIENT, AzMail, ShareDot chat).
#[derive(Clone, PartialEq, Eq)]
pub struct Record {
    /// The device that wrote the record.
    pub author: DeviceId,
    /// The sealed body: opaque here.
    pub body: Vec<u8>,
    /// The author's signature over the record's preimage.
    pub sig: Vec<u8>,
}

impl Record {
    /// What the record costs to keep: [`RECORD_OVERHEAD`] plus its body and its signature.
    pub fn size(&self) -> ByteCount {
        RECORD_OVERHEAD
            .saturating_add(ByteCount::of_len(self.body.len()))
            .saturating_add(ByteCount::of_len(self.sig.len()))
    }
}

impl fmt::Debug for Record {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Record(author {}, body {} B, sig {} B)",
            self.author,
            self.body.len(),
            self.sig.len()
        )
    }
}

/// The bytes a record's author signs: `signing_input(LOG_RECORD_V1, space || name || author || body)`.
/// The body is last, so the fixed 48-byte prefix makes the layout unambiguous.
///
/// Implements planning/architecture.md section 2.5 (the one signing rule: `domain || 0x00 || bytes`)
/// and section 5.13 (every record carries `author_device` and a signature).
///
/// Caller: `AppendLog::append` (the check); the clients that sign records (SYNC-CLIENT).
pub fn record_preimage(log: &LogId, author: DeviceId, body: &[u8]) -> Vec<u8> {
    let mut signed = Vec::with_capacity(48usize.saturating_add(body.len()));
    signed.extend_from_slice(log.space.as_bytes());
    signed.extend_from_slice(&log.name);
    signed.extend_from_slice(author.as_bytes());
    signed.extend_from_slice(body);
    signing_input(&LOG_RECORD_V1, &signed)
}

/// A record as the log keeps and `log_read` returns it: the record, the sequence number the server gave
/// it and the leader's stamp.
///
/// Implements planning/architecture.md section 5.13 (server-sequenced records) and section 2.16
/// (`logs`).
///
/// Caller: `ReadPage`, `AppendLog::records`, `AppendLog::resume`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoggedRecord {
    /// The sequence number.
    pub seq: Seq,
    /// The leader's stamp of the append.
    pub at: Stamp,
    /// The record.
    pub record: Record,
}

/// What an append came to: the sequence number the record was given.
///
/// Implements planning/architecture.md section 5.13 (server-sequenced records).
///
/// Caller: `AppendLog::append`; S33 derives `log_appended` from it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Appended {
    /// The sequence number of the record.
    pub seq: Seq,
}

/// Why an append is refused. A refusal changes nothing. The statuses of planning/architecture.md
/// section 2.5: `TooLarge` and `BadSignature` are `bad_request`, `QuotaExceeded` is `quota_exceeded`
/// (and the account's devices get the notice), `AuthorNotAllowed` is `forbidden`, the last two are a
/// fault of the node (`unavailable`).
///
/// Implements planning/architecture.md section 2.5 (limits and statuses) and section 5.13.
///
/// Caller: `AppendLog::append`; SYNC-APPLY maps it to a status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum AppendRefusal {
    /// The body or the signature is over its limit.
    #[error("the record is over its limit ({size})")]
    TooLarge {
        /// The size of the part that is over.
        size: ByteCount,
    },
    /// The account's uncompacted bytes would pass the cap: the notice tells its devices to compact.
    #[error("the account's append logs are full: compact")]
    QuotaExceeded {
        /// The notice for the account's devices.
        notice: CompactNotice,
    },
    /// The author's signature does not verify.
    #[error("the author's signature does not verify: {0}")]
    BadSignature(VerifyError),
    /// The author may not append to this space, or is not known (the two are one answer).
    #[error("the author may not append to this space")]
    AuthorNotAllowed,
    /// The quota is another account's: the node wired the log to the wrong quota.
    #[error("the quota belongs to another account")]
    WrongQuota,
    /// The log has used every sequence number.
    #[error("the log has used every sequence number")]
    SeqExhausted,
}

/// What `log_read` answers.
///
/// Implements planning/architecture.md section 5.13 (`log_read`; records are compacted by clients into
/// packs).
///
/// Caller: `AppendLog::read`; SYNC-CLIENT reads a log with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadPage {
    /// Records from the position asked for, oldest first; `next` is where to continue, `None` when the
    /// page reaches the end of the log (a page with no records is a log with nothing past the
    /// position).
    Records {
        /// The records.
        records: Vec<LoggedRecord>,
        /// The position to continue from.
        next: Option<Seq>,
    },
    /// The position asked for is inside the compacted prefix: those records are in packs now. Read the
    /// packs, then continue from the position after `through`.
    Compacted {
        /// The last compacted sequence number.
        through: Seq,
    },
}

/// Why a log cannot be rebuilt from stored rows.
///
/// Implements planning/architecture.md section 2.16 (`logs` is persisted by the node) and section 1.4
/// rule 11.
///
/// Caller: `AppendLog::resume` (SYNC-APPLY, when it loads the rows of a log or installs a snapshot).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LogResumeRefusal {
    /// The sequence numbers are not consecutive from the first uncompacted one: this record is not the
    /// one that should be next.
    #[error("record {seq} is not the next sequence number")]
    NotConsecutive {
        /// The offending sequence number.
        seq: Seq,
    },
    /// A body or a signature is over its limit.
    #[error("record {seq} is over its limit")]
    TooLarge {
        /// The offending sequence number.
        seq: Seq,
    },
}

/// One append log: its uncompacted records, the sequence numbers and the compacted prefix.
///
/// Implements planning/architecture.md section 5.13 (`log_append`, `log_read`) and section 2.16
/// (`logs`).
///
/// Caller: SYNC-APPLY (inside the Raft apply path of the node); S33 (events); the chat, audit and
/// mail apps through the verbs.
#[derive(Clone, PartialEq, Eq)]
pub struct AppendLog {
    pub(crate) id: LogId,
    pub(crate) account: AccountId,
    pub(crate) records: BTreeMap<Seq, LoggedRecord>,
    pub(crate) bytes: ByteCount,
    pub(crate) last: Option<Seq>,
    pub(crate) compacted: Option<Seq>,
}

impl fmt::Debug for AppendLog {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "AppendLog(space {}, {} records, {}, last {:?}, compacted {:?})",
            self.id.space,
            self.records.len(),
            self.bytes,
            self.last,
            self.compacted
        )
    }
}

impl AppendLog {
    /// An empty log `id` of `account`.
    pub fn new(id: LogId, account: AccountId) -> AppendLog {
        AppendLog {
            id,
            account,
            records: BTreeMap::new(),
            bytes: ByteCount::ZERO,
            last: None,
            compacted: None,
        }
    }

    /// Rebuilds a log from its stored rows (oldest first): the rows of `logs`, or a snapshot a new
    /// voter installs. `compacted_through` is the last sequence number clients compacted (`None`: no
    /// compaction yet). The rows are checked, never trusted: consecutive sequence numbers from the
    /// first uncompacted one, and every body and signature within its limit.
    pub fn resume(
        id: LogId,
        account: AccountId,
        compacted_through: Option<Seq>,
        rows: Vec<LoggedRecord>,
    ) -> Result<AppendLog, LogResumeRefusal> {
        let mut expected = match compacted_through {
            None => Some(Seq::FIRST),
            Some(through) => through.next(),
        };
        let mut records = BTreeMap::new();
        let mut bytes = ByteCount::ZERO;
        let mut last = compacted_through;
        for row in rows {
            if Some(row.seq) != expected {
                return Err(LogResumeRefusal::NotConsecutive { seq: row.seq });
            }
            if ByteCount::of_len(row.record.body.len()) > RECORD_MAX
                || ByteCount::of_len(row.record.sig.len()) > SIG_MAX
            {
                return Err(LogResumeRefusal::TooLarge { seq: row.seq });
            }
            bytes = bytes.saturating_add(row.record.size());
            expected = row.seq.next();
            last = Some(row.seq);
            records.insert(row.seq, row);
        }
        Ok(AppendLog {
            id,
            account,
            records,
            bytes,
            last,
            compacted: compacted_through,
        })
    }

    /// The log's id.
    pub fn id(&self) -> LogId {
        self.id
    }

    /// The account the log belongs to.
    pub fn account(&self) -> AccountId {
        self.account
    }

    /// The sequence number of the newest record ever appended (compacted or not); `None` for a log
    /// nobody has appended to.
    pub fn last_seq(&self) -> Option<Seq> {
        self.last
    }

    /// The last compacted sequence number; `None` when nothing is compacted.
    pub fn compacted_through(&self) -> Option<Seq> {
        self.compacted
    }

    /// How many uncompacted records the log holds.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Whether the log holds no uncompacted record.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// The bytes the uncompacted records cost (what this log holds against the account's quota).
    pub fn uncompacted_bytes(&self) -> ByteCount {
        self.bytes
    }

    /// The uncompacted records, oldest first (for the rows of `logs`).
    pub fn records(&self) -> impl Iterator<Item = &LoggedRecord> {
        self.records.values()
    }

    /// Appends a record: the next sequence number is its, `at` the leader's stamp. `q` is the
    /// account's quota (shared by all its logs), `who` the registry's view, `v` the signature
    /// scheme. See the module for the checks and their order. A refused append changes nothing.
    pub fn append(
        &mut self,
        r: Record,
        at: Stamp,
        q: &mut AccountLogQuota,
        who: &dyn SpaceAccess,
        v: &dyn SignatureVerifier,
    ) -> Result<Appended, AppendRefusal> {
        if q.account() != self.account {
            return Err(AppendRefusal::WrongQuota);
        }
        let body = ByteCount::of_len(r.body.len());
        let sig = ByteCount::of_len(r.sig.len());
        if (body > RECORD_MAX || sig > SIG_MAX) && !mutant!("sync_log_size_unchecked") {
            return Err(AppendRefusal::TooLarge {
                size: if body > RECORD_MAX { body } else { sig },
            });
        }
        if !who.may_append(r.author, self.id.space) && !mutant!("sync_log_access_unchecked") {
            return Err(AppendRefusal::AuthorNotAllowed);
        }
        let Some(key) = who.author_key(r.author) else {
            return Err(AppendRefusal::AuthorNotAllowed);
        };
        if !mutant!("sync_log_signature_unchecked") {
            let preimage = record_preimage(&self.id, r.author, &r.body);
            if let Err(error) = v.verify(SigAlg::Ed25519, key.as_bytes(), &preimage, &r.sig) {
                return Err(AppendRefusal::BadSignature(error));
            }
        }
        let seq = match self.last {
            None => Seq::FIRST,
            Some(last) => match last.next() {
                Some(next) if mutant!("sync_log_seq_skips") => match next.next() {
                    Some(skipped) => skipped,
                    None => return Err(AppendRefusal::SeqExhausted),
                },
                Some(next) => next,
                None => return Err(AppendRefusal::SeqExhausted),
            },
        };
        let size = r.size();
        // The last step that can refuse: after it nothing fails, so a refusal changed nothing.
        let charged = if mutant!("sync_log_quota_per_log") {
            q.charge_counting(size, self.bytes)
        } else {
            q.charge(size)
        };
        if let Err(notice) = charged {
            return Err(AppendRefusal::QuotaExceeded { notice });
        }
        self.bytes = self.bytes.saturating_add(size);
        self.last = Some(seq);
        self.records
            .insert(seq, LoggedRecord { seq, at, record: r });
        Ok(Appended { seq })
    }

    /// Reads from `from` (inclusive): up to `limit` records (never more than [`READ_PAGE_MAX`]; a
    /// `limit` of 0 counts as 1) within `max_bytes` (never more than [`READ_PAGE_BYTES`]), oldest first,
    /// and always at least one record when there is one, so that a reader that follows `next` always
    /// advances. A `from` inside the compacted prefix is answered `Compacted`.
    pub fn read(&self, from: Seq, limit: u16, max_bytes: ByteCount) -> ReadPage {
        if let Some(through) = self.compacted {
            if from <= through && !mutant!("sync_read_compacted_as_empty") {
                return ReadPage::Compacted { through };
            }
        }
        let limit = if mutant!("sync_read_count_unbounded") {
            usize::from(limit)
        } else {
            usize::from(limit.clamp(1, READ_PAGE_MAX))
        };
        let budget = if max_bytes > READ_PAGE_BYTES {
            READ_PAGE_BYTES
        } else {
            max_bytes
        };
        let mut records: Vec<LoggedRecord> = Vec::new();
        let mut bytes = ByteCount::ZERO;
        let mut next = None;
        for (seq, logged) in self.records.range(from..) {
            let size = logged.record.size();
            let over_bytes = !records.is_empty()
                && bytes.saturating_add(size) > budget
                && !mutant!("sync_read_bytes_unbounded");
            if records.len() >= limit || over_bytes {
                next = Some(*seq);
                break;
            }
            bytes = bytes.saturating_add(size);
            records.push(logged.clone());
        }
        ReadPage::Records { records, next }
    }

    /// A hash of the whole state: the log's id and account, the sequence numbers and every
    /// uncompacted record. Two replicas that applied the same entries have the same hash. It reads
    /// every record: it is for snapshots and checks, not for the request path.
    pub fn state_hash(&self) -> Hash32 {
        let last = tagged(self.last.map(Seq::get));
        let compacted = tagged(self.compacted.map(Seq::get));
        // `usize` fits `u64` on every supported target (the assertion in azlin-seams).
        let count = (self.records.len() as u64).to_be_bytes();
        // The sequence number and the stamp of each record, fixed width.
        let heads: Vec<Vec<u8>> = self
            .records
            .values()
            .map(|logged| {
                let mut head = Vec::with_capacity(16);
                head.extend_from_slice(&logged.seq.get().to_be_bytes());
                head.extend_from_slice(&logged.at.as_secs().to_be_bytes());
                head
            })
            .collect();
        let space: &[u8] = self.id.space.as_bytes();
        let account: &[u8] = self.account.as_bytes();
        let mut parts: Vec<&[u8]> = Vec::with_capacity(6 + 4 * heads.len());
        parts.push(space);
        parts.push(self.id.name.as_slice());
        parts.push(account);
        parts.push(last.as_slice());
        parts.push(compacted.as_slice());
        parts.push(count.as_slice());
        for (head, logged) in heads.iter().zip(self.records.values()) {
            parts.push(head.as_slice());
            parts.push(logged.record.author.as_bytes().as_slice());
            parts.push(logged.record.body.as_slice());
            parts.push(logged.record.sig.as_slice());
        }
        hash32(LOG_STATE_DOMAIN, &parts)
    }
}

/// An optional number as a tag byte (1 when there is one, 0 when there is none) and 8 bytes: the
/// absence has its own spelling, so that "no number" and "the number 0" never hash alike.
fn tagged(number: Option<u64>) -> Vec<u8> {
    let mut out = Vec::with_capacity(9);
    match number {
        Some(number) => {
            out.push(1);
            out.extend_from_slice(&number.to_be_bytes());
        }
        None => {
            out.push(0);
            out.extend_from_slice(&[0u8; 8]);
        }
    }
    out
}
