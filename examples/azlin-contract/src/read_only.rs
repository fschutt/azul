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
    /// A PutObject of one of an encrypted drive's index objects ([`is_index_key`]) no larger
    /// than the object it replaces ([`index_put_shrinks`]): a delete of the owner's files on a
    /// drive whose index the backend cannot read. Entries can be removed, stored data cannot
    /// grow.
    IndexShrink,
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

/// Whether a PutObject of `new_len` bytes at an index key shrinks the index: an object is there
/// (`current_len`, its size) and the new one is not larger. A new index object adds to the
/// index, so it is no shrink.
#[must_use]
pub const fn index_put_shrinks(current_len: Option<u64>, new_len: u64) -> bool {
    match current_len {
        Some(current) => new_len <= current,
        None => false,
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
    /// deletes (an unpaid drive, the operator's hold, a move), a pending lockdown takes no
    /// change, and a reason this build does not know is taken as reads only.
    #[must_use]
    pub const fn permits(self, op: BucketOp) -> bool {
        // (reads, writes, deletes, index shrinks, links)
        let (read, write, delete, shrink, link) = match self {
            DriveAccess::Writable => (true, true, true, true, true),
            DriveAccess::ReadOnly(
                ReadOnlyWhy::Unpaid | ReadOnlyWhy::Operator | ReadOnlyWhy::Moving,
            ) => (true, false, true, true, true),
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
            BucketOp::IndexShrink => shrink,
            BucketOp::Link => link,
        }
    }

    /// Whether a drive in this state takes a PutObject of `new_len` bytes at `key`, where
    /// `current_len` is the size of the object it replaces (`None`: a new key): any put while it
    /// takes writes, else only an index shrink ([`BucketOp::IndexShrink`]).
    #[must_use]
    pub fn takes_put(self, key: &str, current_len: Option<u64>, new_len: u64) -> bool {
        self.permits(BucketOp::Write)
            || (self.permits(BucketOp::IndexShrink)
                && is_index_key(key)
                && index_put_shrinks(current_len, new_len))
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
