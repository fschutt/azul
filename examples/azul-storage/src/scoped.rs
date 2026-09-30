//! A drive seen through a grant: the seam for RBAC and access links.
//!
//! The database's access links say who may view or edit which path. Resolved,
//! a link is a drive (the bucket), a key prefix (`meetings/<uuid>/`) and
//! whether writing is allowed; [`ScopedDrive`] enforces the last two whatever
//! the backend does.

use crate::{
    key::{check_path_key, check_path_prefix},
    ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo,
};

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
        if !prefix.is_empty() {
            if !prefix.ends_with('/') {
                return Err(DriveError::InvalidKey {
                    key: prefix.to_string(),
                    reason: "a grant covers a folder, which ends with /",
                });
            }
            check_path_prefix(prefix)?;
        }
        Ok(ScopedDrive {
            inner,
            prefix: prefix.to_string(),
            can_write,
        })
    }

    #[must_use]
    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    #[must_use]
    pub fn can_write(&self) -> bool {
        self.can_write
    }

    fn full_key(&self, key: &str) -> Result<String, DriveError> {
        check_path_key(key)?;
        Ok(format!("{}{key}", self.prefix))
    }

    fn check_writable(&self) -> Result<(), DriveError> {
        if self.can_write {
            return Ok(());
        }
        let scope = if self.prefix.is_empty() {
            "this drive"
        } else {
            self.prefix.as_str()
        };
        Err(DriveError::Denied {
            message: format!("the grant for {scope} is read-only"),
        })
    }
}

impl<D: Drive> Drive for ScopedDrive<D> {
    fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
        check_path_prefix(&request.prefix)?;
        let mut inner_request = request.clone();
        inner_request.prefix = format!("{}{}", self.prefix, request.prefix);
        let page = self.inner.list(&inner_request)?;
        let strip = |key: &str| key.strip_prefix(self.prefix.as_str()).map(str::to_string);
        Ok(ListPage {
            folders: page
                .folders
                .iter()
                .filter_map(|f| strip(f.as_str()))
                .collect(),
            objects: page
                .objects
                .into_iter()
                .filter_map(|mut info| {
                    info.key = strip(info.key.as_str())?;
                    Some(info)
                })
                .collect(),
            next: page.next,
        })
    }

    fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
        self.inner.get(&self.full_key(key)?)
    }

    fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
        self.inner.get_range(&self.full_key(key)?, range)
    }

    fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
        self.check_writable()?;
        self.inner.put(&self.full_key(key)?, bytes)
    }

    fn delete(&self, key: &str) -> Result<(), DriveError> {
        self.check_writable()?;
        self.inner.delete(&self.full_key(key)?)
    }

    fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
        let mut info = self.inner.head(&self.full_key(key)?)?;
        info.key = key.to_string();
        Ok(info)
    }
}
