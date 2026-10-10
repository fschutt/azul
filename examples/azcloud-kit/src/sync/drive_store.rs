//! Any azul-storage [`Drive`] as the sync's [`RemoteStore`]: how an app that holds its drives
//! as drives (AzDrive's Azlin and S3 drives - an Azlin drive refreshes its credentials under
//! the drive's keyring lock by itself) syncs a folder with the plain index and blobs.
//!
//! - A conditional GET is a HEAD first: the same entity tag answers "not modified" without the
//!   bytes; otherwise a GET, with the tag the HEAD saw (a write in between makes the next
//!   conditional write lose, and the run read again - never a lost update).
//! - A conditional PUT is the drive's [`Drive::put_if`] (S3 `If-Match` / `If-None-Match: *`);
//!   a drive that cannot ask (a folder on disk) cannot hold a synced index.

use std::sync::Arc;

use azul_storage::{ops, Drive, DriveError, Precondition};

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
