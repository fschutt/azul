//! Why a bucket takes no writes (or, once it is locked, nothing at all), and what a node answers
//! for each reason; what each state of a drive takes ([`DriveAccess::permits`], the one table
//! the nodes and the apps read); the service's own object metadata; the cluster map's epoch the
//! requests name.

use serde::{Deserialize, Serialize};

use crate::AzlinError;

/// Why a drive's bucket takes no writes. The bucket row keeps it beside its `read_only` flag;
/// a row from before the reason existed reads as [`ReadOnlyWhy::Unpaid`], the only reason
/// there was.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadOnlyWhy {
    /// The paid period ended, and its grace too.
    #[default]
    Unpaid,
    /// Someone used the recovery code: the drive is frozen until the lockdown's notice ends.
    LockdownPending,
    /// The operator froze it (`azctl bucket set`).
    Operator,
    /// Unpaid past its read-only time: the drive is locked. Nothing is read or written any
    /// more, its files are moved to offline storage, and only its owner who proves it and pays
    /// gets it back ([`crate::ArchiveWire`]).
    Archived,
    /// Its files are being moved (between blocks, to other disks): its owner's deletes go on,
    /// other writes wait until the move is done.
    Moving,
    /// A reason this build does not know: a newer service wrote it. Never written.
    #[serde(other)]
    Unknown,
}

impl ReadOnlyWhy {
    /// Every reason a service writes ([`ReadOnlyWhy::Unknown`] is not one).
    pub const ALL: [ReadOnlyWhy; 5] = [
        ReadOnlyWhy::Unpaid,
        ReadOnlyWhy::LockdownPending,
        ReadOnlyWhy::Operator,
        ReadOnlyWhy::Archived,
        ReadOnlyWhy::Moving,
    ];

    /// The reason as the wire and the bucket row write it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            ReadOnlyWhy::Unpaid => "unpaid",
            ReadOnlyWhy::LockdownPending => "lockdown_pending",
            ReadOnlyWhy::Operator => "operator",
            ReadOnlyWhy::Archived => "archived",
            ReadOnlyWhy::Moving => "moving",
            ReadOnlyWhy::Unknown => "unknown",
        }
    }

    /// The reason `text` names; `None` for one this build does not know.
    #[must_use]
    pub fn parse(text: &str) -> Option<ReadOnlyWhy> {
        let text = text.trim();
        ReadOnlyWhy::ALL.into_iter().find(|why| why.as_str() == text)
    }

    /// The reason of a received answer or row: [`ReadOnlyWhy::Unknown`] for one this build does
    /// not know.
    #[must_use]
    pub fn from_wire(text: &str) -> ReadOnlyWhy {
        ReadOnlyWhy::parse(text).unwrap_or(ReadOnlyWhy::Unknown)
    }

    /// The code a node refuses a write with: each reason its own words for the user. A reason
    /// this build does not know is "read-only for now", never a payment.
    #[must_use]
    pub fn code(self) -> AzlinError {
        match self {
            ReadOnlyWhy::Unpaid => AzlinError::ReadOnlyUnpaid,
            ReadOnlyWhy::LockdownPending => AzlinError::ReadOnlyLockdown,
            ReadOnlyWhy::Operator | ReadOnlyWhy::Unknown => AzlinError::ReadOnlyOperator,
            ReadOnlyWhy::Archived => AzlinError::DriveArchived,
            ReadOnlyWhy::Moving => AzlinError::ReadOnlyMoving,
        }
    }

    /// Whether reads stop too: a locked drive serves nothing to anyone but the operator who
    /// moves its files to offline storage and back.
    #[must_use]
    pub fn blocks_reads(self) -> bool {
        self == ReadOnlyWhy::Archived
    }
}

/// What a request does to a drive's bucket, as the states of a drive tell requests apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BucketOp {
    /// GetObject, HeadObject, a listing.
    Read,
    /// PutObject, CopyObject, a multipart upload's create, parts and complete.
    Write,
    /// The owner's DeleteObject, DeleteObjects and a multipart upload's abort.
    Delete,
    /// A PutObject of one of an encrypted drive's index objects ([`is_index_key`]): a commit of
    /// the drive's index, which a delete of the owner's files on an encrypted drive is. On a
    /// frozen drive a node takes it by [`DriveAccess::takes_index_commit`].
    IndexCommit,
    /// A presigned link that lets someone else read.
    Link,
}

/// Every key of an encrypted drive's index starts with this: the manifest, the log entries,
/// the packs and their indexes, the checkpoints and the leases (azul-storage's drive index).
pub const INDEX_PREFIX: &str = ".azlin/meta/";

/// Whether `key` is one of an encrypted drive's index objects: a key under [`INDEX_PREFIX`]
/// from the bucket's root.
#[must_use]
pub fn is_index_key(key: &str) -> bool {
    key.len() > INDEX_PREFIX.len() && key.starts_with(INDEX_PREFIX)
}

/// How far a frozen drive's stored bytes may go above what it stored when it froze, for its
/// owner's index commits: 64 MiB. The data objects a delete removes after its commit make room
/// again.
pub const INDEX_ALLOWANCE: u64 = 64 * 1024 * 1024;

/// The largest index object a frozen drive takes in one PutObject: 64 MiB.
pub const INDEX_OBJECT_MAX: u64 = 64 * 1024 * 1024;

/// Whose credential signs a request, as the nodes tell them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CredentialKind {
    /// A member's read-write credential: one of the owner's devices.
    MemberReadWrite,
    /// A member's read-only credential: one of the owner's devices, its credential minted while
    /// the drive took no writes.
    MemberReadOnly,
    /// A share's credential: someone a link or a shared folder lets in.
    Share,
    /// A kind this build does not know: a newer service wrote it. Never written.
    #[serde(other)]
    Unknown,
}

impl CredentialKind {
    /// Whether it is the owner's: a member's credential (read-write at most), never a share's.
    #[must_use]
    pub const fn is_owner(self) -> bool {
        matches!(self, CredentialKind::MemberReadWrite | CredentialKind::MemberReadOnly)
    }
}

/// A drive's state as its bucket sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DriveAccess {
    /// Paid and in good standing.
    Writable,
    /// It takes no writes, for this reason.
    ReadOnly(ReadOnlyWhy),
    /// Banned, within its grace: its files can be read and copied, nothing else.
    Banned,
    /// Banned past its grace: nothing at all.
    Closed,
}

impl DriveAccess {
    /// Whether a drive in this state takes `op`. This is the one table of it: the nodes refuse
    /// by it, and the apps' windows and syncs offer and send by it. A freeze keeps the owner's
    /// deletes (an unpaid drive, the operator's hold, a move); an encrypted drive's index commits
    /// go on while it is unpaid or held by the operator ([`Self::takes_index_commit`] adds the
    /// byte budget), not during a move; a pending lockdown takes no change, and a reason this
    /// build does not know is taken as reads only.
    #[must_use]
    pub const fn permits(self, op: BucketOp) -> bool {
        // (reads, writes, deletes, index commits, links)
        let (read, write, delete, commit, link) = match self {
            DriveAccess::Writable => (true, true, true, true, true),
            DriveAccess::ReadOnly(ReadOnlyWhy::Unpaid | ReadOnlyWhy::Operator) => {
                (true, false, true, true, true)
            }
            DriveAccess::ReadOnly(ReadOnlyWhy::Moving) => (true, false, true, false, true),
            DriveAccess::ReadOnly(ReadOnlyWhy::LockdownPending | ReadOnlyWhy::Unknown) => {
                (true, false, false, false, true)
            }
            DriveAccess::ReadOnly(ReadOnlyWhy::Archived) | DriveAccess::Closed => {
                (false, false, false, false, false)
            }
            DriveAccess::Banned => (true, false, false, false, false),
        };
        match op {
            BucketOp::Read => read,
            BucketOp::Write => write,
            BucketOp::Delete => delete,
            BucketOp::IndexCommit => commit,
            BucketOp::Link => link,
        }
    }

    /// Whether a frozen drive in this state takes its owner's index commit: a PutObject of
    /// `new_len` bytes at `key` in place of an object of `replaced_len` bytes (0 for a new key),
    /// signed with a `credential`, while the bucket stores `stored_now` bytes and stored
    /// `frozen_at` when it became read-only. It takes it only when all hold: the key is an index
    /// key ([`is_index_key`]); the drive is unpaid or held by the operator (not being moved, not
    /// in a lockdown's pause, not locked, not banned); the credential is a member's
    /// ([`CredentialKind::is_owner`]); the bucket stays within `frozen_at` plus
    /// [`INDEX_ALLOWANCE`] after the put; the object is at most [`INDEX_OBJECT_MAX`]. The figures
    /// are stored bytes as the nodes count them; no sum underflows or overflows. A writable
    /// drive's puts are writes ([`BucketOp::Write`]), not this exception.
    #[must_use]
    pub fn takes_index_commit(
        self,
        credential: CredentialKind,
        key: &str,
        stored_now: u64,
        frozen_at: u64,
        replaced_len: u64,
        new_len: u64,
    ) -> bool {
        let frozen = matches!(
            self,
            DriveAccess::ReadOnly(ReadOnlyWhy::Unpaid | ReadOnlyWhy::Operator)
        );
        let after = stored_now.saturating_sub(replaced_len).saturating_add(new_len);
        frozen
            && credential.is_owner()
            && is_index_key(key)
            && new_len <= INDEX_OBJECT_MAX
            && after <= frozen_at.saturating_add(INDEX_ALLOWANCE)
    }
}

/// The user metadata names the service keeps for itself (`x-amz-meta-azlin-...`): never a
/// file's own metadata, never shown or synced as one.
pub const SERVICE_METADATA_PREFIX: &str = "x-amz-meta-azlin-";

/// The marker a node puts on an object it moved, on GET and HEAD.
pub const MOVED_HEADER: &str = "x-amz-meta-azlin-moved";

/// Whether the header `name` is the service's own metadata ([`SERVICE_METADATA_PREFIX`]), in
/// any letter case.
#[must_use]
pub fn is_service_metadata(name: &str) -> bool {
    name.len() >= SERVICE_METADATA_PREFIX.len()
        && name
            .get(..SERVICE_METADATA_PREFIX.len())
            .is_some_and(|start| start.eq_ignore_ascii_case(SERVICE_METADATA_PREFIX))
}

/// The header a client names the epoch of the cluster map it knows in (`x-azlin-map: 7`), and
/// a node with a newer map answers its own epoch and hash in (`x-azlin-map: 8 <hex>`, with a
/// 409 `map_stale`).
pub const MAP_HEADER: &str = "x-azlin-map";

/// The epoch an `x-azlin-map` value names: its first word.
#[must_use]
pub fn map_epoch(header: &str) -> Option<u64> {
    header.split_whitespace().next()?.parse().ok()
}
