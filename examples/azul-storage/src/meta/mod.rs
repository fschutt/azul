//! The drive index: an encrypted metadata git repository in the bucket.
//!
//! A drive is a bucket: the file bytes as encrypted data objects with random
//! names, and ONE metadata repository that knows what they are. The repository
//! holds a pointer file per drive file (at the file's path: the data object's
//! key, the plaintext size, the mtime, the BLAKE3 of the plaintext and the file
//! key wrapped by the drive key), the folders as git trees, the drive's policy
//! (`.azlin/policy.toml`) and the members' key wraps. Devices browse the
//! repository, never the bucket: no S3 List.
//!
//! # Layout in the bucket
//!
//! The repository is stored as a write-ahead log, in the layout of walgit
//! (<https://github.com/tobi/walgit>, MIT), which implements the design of
//! Cursor's "Git at any scale". Every object is sealed with the drive key:
//!
//! ```text
//! .azlin/meta/manifest                 the linearization point: replaced only by a
//!                                      compare-and-swap (If-Match on its version)
//! .azlin/meta/log/<seq>-<attempt>      one immutable log entry per publish
//! .azlin/meta/wal/<name>.pack + .idx   immutable packs of git objects; <name> is a
//!                                      keyed hash, never the git hash
//! .azlin/meta/checkpoints/<seq>        the refs and the live packs at <seq>
//! .azlin/meta/leases/<purpose>         a lease with an expiry (compaction)
//! ```
//!
//! - **Polling** is one conditional GET of the manifest; "not modified" means nothing
//!   changed.
//! - **A publish** writes the pack, then the log entry, then swaps the manifest. When the
//!   swap loses (another device was first), the device reads the new entries, merges,
//!   and tries again.
//! - **A new device** reads the latest checkpoint plus the log entries after it.
//!
//! # Encryption
//!
//! The bucket sees that a repository exists, its size and its write times;
//! names, the tree's shape and git's hashes stay inside the ciphertext. What
//! seals is a [`Sealer`] (the drive key's; an authenticated cipher with
//! associated data). The associated data of every sealed object is its key in
//! the bucket, so the bucket cannot answer one object with another.
//!
//! # Why our own git objects (the dependency evaluation, 2026-10-10)
//!
//! The plan was to use the `gix` crates for objects, trees and packs, and to
//! vendor walgit's `walgit-wal` and `walgit-store` behind an encrypting store.
//!
//! - **walgit** is not published on crates.io. `walgit-wal` is a server: async (tokio),
//!   protobuf (its build script needs `protoc`), a bare repository on disk managed through
//!   the `git` binary, and it takes `walgit-store` with the AWS and Google Cloud SDKs. Its
//!   dependency closure is 514 crates, 154 of them crates this workspace has never had.
//! - **gix** for objects, hashes and pack writing is 25 new crates (one build script) on a
//!   release train with a breaking release every month. A tree merge through gix-merge is
//!   42 new crates. git's pack format (zlib, deltas, one trailing hash) cannot be read in
//!   ranges once it is sealed.
//!
//! What a drive index needs is small: git objects byte-identical to git's
//! (so `git-remote-azlin` can hand them to plain git), SHA-256 (sha2 is a
//! dependency already), a pack whose sealed chunks can be read in ranges, and
//! a three-way merge of trees whose conflicts are files to keep or rename,
//! never text to merge. That is this module, with no new crate.
//!
//! # Modules
//!
//! - [`bucket`]: whole objects with versions and conditional reads and writes ([`Bucket`]),
//!   over a [`crate::Drive`] ([`DriveBucket`]), a folder ([`FolderBucket`]) or memory
//!   ([`MemoryBucket`]).
//! - [`seal`]: the [`Sealer`] (the drive key with the `encryption` feature; [`TestSealer`]).
//! - [`objects`], [`pack`]: git objects and sealed, range-readable packs.
//! - [`wal`]: the log in the bucket ([`MetaStore`]): poll, publish, checkpoint, lease,
//!   compaction, garbage collection.
//! - [`tree`], [`merge`]: folders as trees, changes, the three-way merge (D52 conflicts);
//!   [`shard`]: huge folders in hidden fan-out subtrees.
//! - [`repo`]: one device's drive index ([`MetaRepo`]): commit, pull, merge, restore; kept
//!   on disk and read lazily ([`RepoOptions`]).
//! - `index`, `pointer` (feature `encryption`): the encrypted drive's `NameIndex` over the
//!   repository (`MetaIndex`, `open_encrypted_drive`) and its pointer files.
//! - `cache` (feature `index-cache`): the local SQLite query cache (search, largest,
//!   recent, totals), rebuilt from the tree.

pub mod bucket;
#[cfg(feature = "index-cache")]
pub mod cache;
#[cfg(feature = "encryption")]
pub mod index;
pub mod merge;
pub mod objects;
pub mod pack;
#[cfg(feature = "encryption")]
pub mod pointer;
pub mod repo;
pub mod seal;
pub mod shard;
pub mod tree;
pub mod wal;

use std::fmt;

pub use bucket::{
    Bucket, DriveBucket, Fetched, FolderBucket, Listed, MemoryBucket, RequestCounts, Version,
};
pub use merge::{Conflict, ConflictKind, Merged, Resolution, Resolved};
pub use objects::{Commit, Kind, Mode, ObjectId, Objects, Signature, Tree, TreeEntry};
pub use pack::{PackIndex, PackWriter, SealedPack};
#[cfg(feature = "encryption")]
pub use index::{open_encrypted_drive, MetaIndex};
pub use repo::{CommitOutcome, MetaRepo, RepoOptions};
pub use seal::{SealError, Sealer, TestSealer};
pub use tree::Change;
pub use wal::{
    LeaseGuard, LogEntry, Manifest, MetaStore, PackRef, Packs, Publish, Published, RefUpdate,
    RepoState, StoreSnapshot, SyncReport,
};

use crate::DriveError;

/// The repository's keys in the bucket. Nothing in them comes from a name or
/// a path of the drive: sequence numbers, random attempt ids, keyed hashes.
pub mod keys {
    /// Every key of the repository starts with this.
    pub const ROOT: &str = ".azlin/meta/";
    /// The manifest: replaced only by a compare-and-swap.
    pub const MANIFEST: &str = ".azlin/meta/manifest";
    pub const LOG_DIR: &str = ".azlin/meta/log/";
    pub const WAL_DIR: &str = ".azlin/meta/wal/";
    pub const CHECKPOINT_DIR: &str = ".azlin/meta/checkpoints/";
    pub const LEASE_DIR: &str = ".azlin/meta/leases/";

    /// The log entry `seq` of one publish attempt (16 hex digits sort as numbers).
    #[must_use]
    pub fn log(seq: u64, attempt: &str) -> String {
        format!("{LOG_DIR}{seq:016x}-{attempt}")
    }

    /// The pack called `name` (a keyed hash).
    #[must_use]
    pub fn pack(name: &str) -> String {
        format!("{WAL_DIR}{name}.pack")
    }

    /// The index of the pack called `name`.
    #[must_use]
    pub fn idx(name: &str) -> String {
        format!("{WAL_DIR}{name}.idx")
    }

    /// The checkpoint at `seq` of one attempt.
    #[must_use]
    pub fn checkpoint(seq: u64, attempt: &str) -> String {
        format!("{CHECKPOINT_DIR}{seq:016x}-{attempt}")
    }

    /// The lease for `purpose` (`maintenance`).
    #[must_use]
    pub fn lease(purpose: &str) -> String {
        format!("{LEASE_DIR}{purpose}")
    }
}

/// Lowercase hex digits of `bytes`.
#[must_use]
pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(char::from(DIGITS[usize::from(b >> 4)]));
        out.push(char::from(DIGITS[usize::from(b & 0x0f)]));
    }
    out
}

/// The bytes of hex digits (either case); `None` for an odd count or another
/// character.
#[must_use]
pub(crate) fn from_hex(text: &str) -> Option<Vec<u8>> {
    fn digit(c: u8) -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            b'A'..=b'F' => Some(c - b'A' + 10),
            _ => None,
        }
    }
    let bytes = text.as_bytes();
    if bytes.len() % 2 != 0 {
        return None;
    }
    bytes
        .chunks(2)
        .map(|pair| Some(digit(pair[0])? << 4 | digit(pair[1])?))
        .collect()
}

/// Why a call of the metadata repository failed. Every variant reads as a
/// sentence; none carries plaintext of the repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MetaError {
    /// A conditional write lost: the object was there already, or it changed
    /// since it was read (S3: 412 Precondition Failed). Nothing was written.
    Conflict { key: String },
    /// Another conditional write of the object was in progress (S3: 409
    /// `ConditionalRequestConflict`): nothing was written, try again. Never
    /// "the object is there".
    Raced { key: String },
    /// The bucket's own error.
    Drive(DriveError),
    /// The object does not open with this drive key (another key, or a changed byte).
    Sealed { key: String, reason: String },
    /// The object opened but makes no sense.
    Corrupt { key: String, reason: String },
    /// The bucket cannot do what the repository needs (conditional writes, versions).
    Unsupported(String),
    /// The bucket holds no metadata repository.
    NoRepository,
    /// The bucket holds a metadata repository already.
    RepositoryExists,
    /// Other devices kept publishing first; gave up after this many attempts.
    Contended { attempts: u32 },
    /// A ref was not at the value the update expected (`None` = absent).
    RefConflict {
        name: String,
        expected: Option<String>,
        actual: Option<String>,
    },
    /// The bucket served an older manifest than one this device has seen: a
    /// replay, or a bucket that lost writes.
    Rollback { seen: u64, served: u64 },
    /// Another device holds the lease until `expires_at` (seconds since 1970).
    LeaseHeld { holder: String, expires_at: u64 },
    /// A git object the repository needs is in none of its packs.
    MissingObject { id: String },
}

impl fmt::Display for MetaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MetaError::Conflict { key } => {
                write!(f, "\"{key}\" was written by another device in the meantime")
            }
            MetaError::Raced { key } => {
                write!(f, "\"{key}\" was being written by another device; try again")
            }
            MetaError::Drive(e) => write!(f, "{e}"),
            MetaError::Sealed { key, reason } => {
                write!(f, "\"{key}\" does not open with this drive's key: {reason}")
            }
            MetaError::Corrupt { key, reason } => write!(f, "\"{key}\" is damaged: {reason}"),
            MetaError::Unsupported(message) => {
                write!(f, "this storage cannot hold a drive index: {message}")
            }
            MetaError::NoRepository => f.write_str("this drive has no index yet"),
            MetaError::RepositoryExists => f.write_str("this drive has an index already"),
            MetaError::Contended { attempts } => write!(
                f,
                "other devices kept changing the drive; gave up after {attempts} attempts"
            ),
            MetaError::RefConflict {
                name,
                expected,
                actual,
            } => write!(
                f,
                "{name} is at {} instead of {}",
                actual.as_deref().unwrap_or("nothing"),
                expected.as_deref().unwrap_or("nothing")
            ),
            MetaError::Rollback { seen, served } => write!(
                f,
                "the storage answered with an older index (revision {served}) than this \
                 device has seen (revision {seen})"
            ),
            MetaError::LeaseHeld { holder, expires_at } => write!(
                f,
                "the device {holder} is tidying the index (until {expires_at})"
            ),
            MetaError::MissingObject { id } => write!(f, "the index lacks the object {id}"),
        }
    }
}

impl std::error::Error for MetaError {}

impl From<DriveError> for MetaError {
    /// A lost conditional write of the drive is the repository's [`MetaError::Conflict`].
    fn from(e: DriveError) -> Self {
        match e {
            DriveError::Conflict { key } => MetaError::Conflict { key },
            other => MetaError::Drive(other),
        }
    }
}
