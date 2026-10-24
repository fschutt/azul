//! Any azul-storage [`Drive`] as the sync's [`RemoteStore`]: how an app that holds its drives
//! as drives (AzDrive's Azlin and S3 drives - an Azlin drive refreshes its credentials under
//! the drive's keyring lock by itself) syncs a folder with the plain index and blobs.
//!
//! - A conditional GET is a HEAD first: the same entity tag answers "not modified" without the
//!   bytes; otherwise a GET, with the tag the HEAD saw (a write in between makes the next
//!   conditional write lose, and the run read again - never a lost update).
//! - A conditional PUT is the drive's [`Drive::put_if`] (S3 `If-Match` / `If-None-Match: *`);
//!   a drive that cannot ask (a folder on disk) cannot hold a synced index.

use std::{
    io::Read,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use azul_storage::{
    checked::{CheckedUpload, FileCheck},
    ops, transfer, ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo, Precondition,
};

use crate::{
    error::CloudResult,
    store::{Conditional, RemoteObject, RemoteStore},
};

/// A drive as the sync's store.
#[derive(Clone)]
pub struct DriveStore {
    drive: Arc<dyn Drive>,
}

impl DriveStore {
    #[must_use]
    pub fn new(drive: Arc<dyn Drive>) -> DriveStore {
        DriveStore { drive }
    }

    /// The drive.
    #[must_use]
    pub fn drive(&self) -> &Arc<dyn Drive> {
        &self.drive
    }
}

/// An entity tag as the drives compare it: without its quotes.
fn bare(etag: &str) -> &str {
    etag.trim_matches('"')
}

impl RemoteStore for DriveStore {
    fn get_unless(&self, key: &str, etag: Option<&str>) -> CloudResult<Conditional> {
        let info = match self.drive.head(key) {
            Ok(info) => info,
            Err(DriveError::NotFound { .. }) => return Ok(Conditional::NotFound),
            Err(e) => return Err(e.into()),
        };
        let now = info.etag.as_deref().map(bare).filter(|e| !e.is_empty());
        if let (Some(want), Some(now)) = (etag.map(bare), now) {
            if want == now {
                return Ok(Conditional::NotModified);
            }
        }
        match self.drive.get(key) {
            Ok(body) => Ok(Conditional::Found {
                body,
                etag: now.map(str::to_string),
            }),
            Err(DriveError::NotFound { .. }) => Ok(Conditional::NotFound),
            Err(e) => Err(e.into()),
        }
    }

    fn fetch(&self, key: &str, _size: u64) -> CloudResult<Option<Vec<u8>>> {
        match self.drive.get(key) {
            Ok(bytes) => Ok(Some(bytes)),
            Err(DriveError::NotFound { .. }) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn put(&self, key: &str, data: &[u8]) -> CloudResult<String> {
        self.drive.put(key, data)?;
        Ok(String::new())
    }

    fn put_if(
        &self,
        key: &str,
        data: &[u8],
        if_match: Option<&str>,
    ) -> CloudResult<Option<String>> {
        let condition = match if_match {
            Some(etag) => Precondition::Matches(bare(etag).to_string()),
            None => Precondition::Absent,
        };
        match self.drive.put_if(key, data, &condition) {
            Ok(Some(etag)) => Ok(Some(bare(&etag).to_string())),
            // The drive did not say the new version's tag: ask it (an empty tag makes the
            // next read a whole one).
            Ok(None) => Ok(Some(
                self.drive
                    .head(key)
                    .ok()
                    .and_then(|info| info.etag)
                    .map(|e| bare(&e).to_string())
                    .unwrap_or_default(),
            )),
            Err(DriveError::Conflict { .. }) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn head(&self, key: &str) -> CloudResult<Option<u64>> {
        match self.drive.head(key) {
            Ok(info) => Ok(Some(info.size)),
            Err(DriveError::NotFound { .. }) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    fn delete(&self, key: &str) -> CloudResult<()> {
        Ok(self.drive.delete(key)?)
    }

    fn put_from(&self, key: &str, body: &mut dyn Read, _size: u64) -> CloudResult<String> {
        self.drive.put_from(key, body)?;
        Ok(String::new())
    }

    fn put_file(&self, key: &str, path: &Path, check: &FileCheck) -> CloudResult<Option<u64>> {
        match self.drive.put_file_checked(key, path, check, &|_| {})? {
            CheckedUpload::Sent(written) => Ok(Some(written)),
            CheckedUpload::Changed => Ok(None),
        }
    }

    fn fetch_to_with(&self, key: &str, size: u64, dest: &Path, mid: &dyn Fn()) -> CloudResult<bool> {
        match download_told(self.drive.as_ref(), key, size, dest, mid) {
            Ok(_) => Ok(true),
            Err(DriveError::NotFound { .. }) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    fn fetch_to(&self, key: &str, size: u64, dest: &Path) -> CloudResult<bool> {
        match transfer::download_to_file(
            self.drive.as_ref(),
            key,
            Some(size),
            dest,
            transfer::CHUNK,
        ) {
            Ok(_) => Ok(true),
            Err(DriveError::NotFound { .. }) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    fn list(&self, prefix: &str) -> CloudResult<Vec<RemoteObject>> {
        Ok(ops::list_all(self.drive.as_ref(), prefix)?
            .into_iter()
            .map(|info| RemoteObject {
                key: info.key,
                size: info.size,
                modified: info.modified.and_then(|m| i64::try_from(m).ok()),
            })
            .collect())
    }
}

/// Downloads `key` (`size` bytes) into `dest` as [`transfer::download_to_file`] does (a big
/// object in ranged parts, a part left in the file for the next download to resume), calling
/// `mid` once when the first part of a download in parts is in the file.
///
/// `mid` is where a test harness crashes a pass (the `mid-download` point of the fault seam).
/// A crash unwinds out of it on this thread while the parts' threads are still fetching; a
/// process killed there would fetch nothing more, so from then on every request of this
/// download fails at once and the download ends with what is in the file.
pub(crate) fn download_told(
    drive: &dyn Drive,
    key: &str,
    size: u64,
    dest: &Path,
    mid: &dyn Fn(),
) -> Result<u64, DriveError> {
    let stopped = AtomicBool::new(false);
    let gated = Stoppable {
        inner: drive,
        stopped: &stopped,
    };
    let mut told = false;
    transfer::download_with_progress(&gated, key, Some(size), dest, transfer::CHUNK, &mut |n| {
        // The first call says what was in the file already (0 for a fresh download).
        if told || n == 0 || n >= size {
            return;
        }
        told = true;
        let stops = StopsOnUnwind(&stopped);
        mid();
        // `mid` returned: no crash, the download goes on.
        std::mem::forget(stops);
    })
}

/// Stops a download when the point it is dropped at unwinds.
struct StopsOnUnwind<'a>(&'a AtomicBool);

impl Drop for StopsOnUnwind<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

/// A drive whose requests fail once `stopped` is set (the download's process is gone).
struct Stoppable<'a> {
    inner: &'a dyn Drive,
    stopped: &'a AtomicBool,
}

impl Stoppable<'_> {
    fn alive(&self) -> Result<(), DriveError> {
        if self.stopped.load(Ordering::SeqCst) {
            return Err(DriveError::Transport(String::from(
                "the download stopped (its pass crashed)",
            )));
        }
        Ok(())
    }
}

impl Drive for Stoppable<'_> {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        self.alive()?;
        self.inner.list(request)
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.alive()?;
        self.inner.get(key)
    }

    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.alive()?;
        self.inner.get_range(key, range)
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.alive()?;
        self.inner.put(key, bytes)
    }

    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.alive()?;
        self.inner.delete(key)
    }

    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        self.alive()?;
        self.inner.head(key)
    }
}
