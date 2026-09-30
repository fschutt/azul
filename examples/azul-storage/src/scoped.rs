//! A drive seen through a grant: the seam for RBAC and access links.
//!
//! The database's access links say who may view or edit which path. Resolved,
//! a link is a drive (the bucket), a key prefix (`meetings/<uuid>/`) and
//! whether writing is allowed; [`ScopedDrive`] enforces the last two whatever
//! the backend does.

use crate::{ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo};

/// `inner` limited to the keys under `prefix`, which it shows without the
/// prefix; `put` and `delete` refused unless `can_write`.
#[derive(Debug, Clone)]
pub struct ScopedDrive<D: Drive> {
    inner: D,
    prefix: String,
    can_write: bool,
}

impl<D: Drive> ScopedDrive<D> {
    /// `prefix` is empty (the whole drive) or a folder ending in `/`.
    pub fn new(inner: D, prefix: &str, can_write: bool) -> Result<Self, DriveError> {
        let _ = (&inner, prefix, can_write);
        todo!("RED")
    }

    #[must_use]
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    #[must_use]
    pub fn can_write(&self) -> bool {
        self.can_write
    }
}

impl<D: Drive> Drive for ScopedDrive<D> {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        let _ = request;
        todo!("RED")
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        let _ = key;
        todo!("RED")
    }

    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        let _ = (key, range);
        todo!("RED")
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        let _ = (key, bytes);
        todo!("RED")
    }

    fn delete(&self, key: &str) -> Result<(), DriveError> {
        let _ = key;
        todo!("RED")
    }

    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        let _ = key;
        todo!("RED")
    }
}
