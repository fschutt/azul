//! The objects of a bucket kept on this device: a [`Drive`] in front of a bucket that answers a
//! read of an object it keeps from its folder, and passes every other call through.
//!
//! An encrypted drive's file versions are immutable AZL1 objects under random keys, so a copy
//! never goes stale: kept below the encryption (`AutoEncrypted` / `EncryptedDrive` over this
//! over the bucket), they are §13.7's "local copies: encrypted, decrypt when opened" - this
//! device holds ciphertext only, and opening a file decrypts it from here without a download.
//! Only what [`ObjectCache::fill`] took is kept (a sync session decides what); a read alone
//! keeps nothing, and a delete through the cache drops its copy too.

use std::{
    fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    sync::Arc,
};

use azul_storage::{
    ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo, Precondition,
};

use crate::state::write_atomic;

/// A bucket's objects kept on this device.
pub struct ObjectCache {
    inner: Arc<dyn Drive>,
    dir: PathBuf,
}

impl ObjectCache {
    /// The bucket `inner`, its kept objects in `dir`.
    #[must_use]
    pub fn new(inner: Arc<dyn Drive>, dir: PathBuf) -> ObjectCache {
        ObjectCache { inner, dir }
    }

    /// The drive below.
    #[must_use]
    pub fn inner(&self) -> &Arc<dyn Drive> {
        &self.inner
    }

    /// Where the copy of `key` lives.
    #[must_use]
    pub fn path_of(&self, key: &str) -> PathBuf {
        super::local::path_of(&self.dir, key)
    }

    /// Whether this device keeps `key`.
    #[must_use]
    pub fn has(&self, key: &str) -> bool {
        self.path_of(key).is_file()
    }

    /// Keeps `key` on this device (read from the bucket once); its bytes.
    ///
    /// # Errors
    ///
    /// When the bucket cannot give it or the copy cannot be written.
    pub fn fill(&self, key: &str) -> Result<u64, DriveError> {
        let path = self.path_of(key);
        if let Ok(meta) = fs::metadata(&path) {
            return Ok(meta.len());
        }
        let bytes = self.inner.get(key)?;
        write_atomic(&path, &bytes, true).map_err(|e| DriveError::Io(format!("{key}: {e}")))?;
        Ok(bytes.len() as u64)
    }

    /// Drops this device's copy of `key` (the bucket keeps it).
    ///
    /// # Errors
    ///
    /// When the copy cannot be removed.
    pub fn evict(&self, key: &str) -> Result<(), DriveError> {
        match fs::remove_file(self.path_of(key)) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == ErrorKind::NotFound => Ok(()),
            Err(e) => Err(DriveError::Io(format!("{key}: {e}"))),
        }
    }

    /// The keys kept, in order.
    #[must_use]
    pub fn cached(&self) -> Vec<String> {
        let mut out = Vec::new();
        walk(&self.dir, "", &mut out);
        out.sort();
        out
    }

    /// The bytes kept.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.cached()
            .iter()
            .filter_map(|key| fs::metadata(self.path_of(key)).ok())
            .map(|meta| meta.len())
            .sum()
    }

    fn kept(&self, key: &str) -> Option<Vec<u8>> {
        fs::read(self.path_of(key)).ok()
    }
}

/// Every file under `dir` as a key (temporary files of a write left out).
fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let key = format!("{prefix}{name}");
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => walk(&entry.path(), &format!("{key}/"), out),
            Ok(kind) if kind.is_file() => out.push(key),
            _ => {}
        }
    }
}

impl Drive for ObjectCache {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.inner.list(request)
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        match self.kept(key) {
            Some(bytes) => Ok(bytes),
            None => self.inner.get(key),
        }
    }

    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        let Some(bytes) = self.kept(key) else {
            return self.inner.get_range(key, range);
        };
        let len = bytes.len() as u64;
        if range.start >= len {
            return Err(DriveError::InvalidRange {
                key: key.to_string(),
            });
        }
        let end = range.end.map_or(len - 1, |end| end.min(len - 1));
        if end < range.start {
            return Err(DriveError::InvalidRange {
                key: key.to_string(),
            });
        }
        // Both ends are inside `bytes`: they fit a usize.
        let (start, end) = (range.start as usize, end as usize);
        Ok(bytes[start..=end].to_vec())
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.evict(key)?;
        self.inner.put(key, bytes)
    }

    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.evict(key)?;
        self.inner.delete(key)
    }

    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.inner.head(key)
    }

    fn put_from(&self, key: &str, body: &mut dyn std::io::Read) -> Result<u64, DriveError> {
        self.evict(key)?;
        self.inner.put_from(key, body)
    }

    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        let written = self.inner.put_if(key, bytes, condition)?;
        self.evict(key)?;
        Ok(written)
    }

    fn copy(&self, from: &str, to: &str) -> Result<(), DriveError> {
        self.evict(to)?;
        self.inner.copy(from, to)
    }

    fn create_folder(&self, prefix: &str) -> Result<(), DriveError> {
        self.inner.create_folder(prefix)
    }

    fn rename(&self, from: &str, to: &str) -> Result<(), DriveError> {
        self.evict(from)?;
        self.evict(to)?;
        self.inner.rename(from, to)
    }

    fn delete_folder(&self, prefix: &str) -> Result<(), DriveError> {
        self.inner.delete_folder(prefix)
    }

    fn local_path(&self, key: &str) -> Option<PathBuf> {
        self.inner.local_path(key)
    }

    fn metadata(&self, key: &str) -> Result<Vec<(String, String)>, DriveError> {
        self.inner.metadata(key)
    }

    fn put_file(
        &self,
        key: &str,
        path: &std::path::Path,
        progress: &(dyn Fn(u64) + Sync),
    ) -> Result<u64, DriveError> {
        self.evict(key)?;
        self.inner.put_file(key, path, progress)
    }

    fn put_from_if(
        &self,
        key: &str,
        body: &mut dyn std::io::Read,
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        let written = self.inner.put_from_if(key, body, condition)?;
        self.evict(key)?;
        Ok(written)
    }
}
