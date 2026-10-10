//! The drive index (`crate::meta`): its buckets, its sealing, its WAL, its git
//! objects and its merges.

mod bucket;
mod cache;
/// The encrypted drive over the drive index (feature `encryption`).
#[cfg(feature = "encryption")]
mod index;
mod merge;
mod objects;
mod pack;
mod repo;
mod seal;
mod shard;
mod tree;
mod wal;

use super::TempDir;
use crate::meta::{Bucket, DriveBucket, FolderBucket, MemoryBucket};

/// One kind of bucket under test, with what keeps it alive.
pub(crate) struct TestBucket {
    pub name: &'static str,
    pub bucket: Box<dyn Bucket>,
    /// The folder of a [`FolderBucket`], removed when dropped.
    pub _dir: Option<TempDir>,
}

/// Every bucket kind: in memory, a folder on disk, a drive with conditional writes.
pub(crate) fn every_bucket() -> Vec<TestBucket> {
    let dir = TempDir::new("meta-bucket");
    vec![
        TestBucket {
            name: "memory",
            bucket: Box::new(MemoryBucket::new()),
            _dir: None,
        },
        TestBucket {
            name: "folder",
            bucket: Box::new(FolderBucket::new(dir.path())),
            _dir: Some(dir),
        },
        TestBucket {
            name: "drive",
            bucket: Box::new(DriveBucket::new(MemoryBucket::new())),
            _dir: None,
        },
    ]
}
