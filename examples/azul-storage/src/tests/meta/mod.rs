//! The drive index (`crate::meta`): its buckets, its sealing, its WAL, its git
//! objects and its merges.

mod bucket;
mod cache;
mod git;
/// The encrypted drive over the drive index (feature `encryption`).
#[cfg(feature = "encryption")]
mod index;
mod merge;
mod objects;
mod pack;
/// The local query cache (feature `index-cache`).
#[cfg(feature = "index-cache")]
mod query_cache;
mod race;
/// The drive index under a new drive key (feature `encryption`).
#[cfg(feature = "encryption")]
mod rekey;
mod repo;
mod seal;
mod shard;
mod sweep;
mod tree;
mod wal;

use std::sync::atomic::{AtomicBool, Ordering};

use super::TempDir;
use crate::{
    meta::{Bucket, DriveBucket, Fetched, FolderBucket, Listed, MemoryBucket, MetaError, Version},
    ByteRange, DriveError,
};

/// A bucket that can be cut off: while it is down, every call fails as a lost
/// connection does.
#[derive(Default)]
pub(crate) struct Unplugged {
    pub inner: MemoryBucket,
    down: AtomicBool,
}

impl Unplugged {
    pub(crate) fn set_down(&self, down: bool) {
        self.down.store(down, Ordering::SeqCst);
    }

    fn up(&self) -> Result<(), MetaError> {
        if self.down.load(Ordering::SeqCst) {
            return Err(MetaError::Drive(DriveError::Transport(
                "no connection to the storage".to_string(),
            )));
        }
        Ok(())
    }
}

impl Bucket for Unplugged {
    fn read(&self, key: &str) -> Result<Option<(Vec<u8>, Version)>, MetaError> {
        self.up()?;
        self.inner.read(key)
    }
    fn read_if_changed(&self, key: &str, known: &str) -> Result<Fetched, MetaError> {
        self.up()?;
        self.inner.read_if_changed(key, known)
    }
    fn read_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, MetaError> {
        self.up()?;
        Bucket::read_range(&self.inner, key, range)
    }
    fn create(&self, key: &str, bytes: &[u8]) -> Result<Option<Version>, MetaError> {
        self.up()?;
        self.inner.create(key, bytes)
    }
    fn replace(&self, key: &str, bytes: &[u8], known: &str) -> Result<Option<Version>, MetaError> {
        self.up()?;
        self.inner.replace(key, bytes, known)
    }
    fn remove(&self, key: &str) -> Result<(), MetaError> {
        self.up()?;
        self.inner.remove(key)
    }
    fn list_keys(&self, prefix: &str) -> Result<Vec<Listed>, MetaError> {
        self.up()?;
        self.inner.list_keys(prefix)
    }
}

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
