//! A drive seen through a grant: the seam for RBAC and access links.
//!
//! The database's access links say who may view or edit which path. Resolved,
//! a link is a drive (the bucket), a key prefix (`meetings/<uuid>/`) and
//! whether writing is allowed; [`ScopedDrive`] enforces the last two whatever
//! the backend does.

use std::{io::Read, path::PathBuf};

use crate::{
    key::{check_key, check_prefix},
    ops::check_folder,
    ByteRange, Ctl, Drive, DriveError, Limit, ListPage, ListRequest, ObjectInfo, Precondition,
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
            check_prefix(prefix)?;
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
        check_key(key)?;
        Ok(format!("{}{key}", self.prefix))
    }

    /// A key or a folder name (ending in `/`) of the inner drive.
    fn full_name(&self, name: &str) -> Result<String, DriveError> {
        if name.ends_with('/') {
            check_folder(name)?;
            check_prefix(name)?;
            Ok(format!("{}{name}", self.prefix))
        } else {
            self.full_key(name)
        }
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
        check_prefix(&request.prefix)?;
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

    fn get_range_if(
        &self,
        key: &str,
        range: ByteRange,
        condition: &Precondition,
    ) -> Result<Vec<u8>, DriveError> {
        self.inner
            .get_range_if(&self.full_key(key)?, range, condition)
    }

    fn get_limited(&self, key: &str, limit: Limit) -> Result<Vec<u8>, DriveError> {
        self.inner.get_limited(&self.full_key(key)?, limit)
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

    fn put_from(&self, key: &str, body: &mut dyn Read) -> Result<u64, DriveError> {
        self.check_writable()?;
        self.inner.put_from(&self.full_key(key)?, body)
    }

    fn put_if(
        &self,
        key: &str,
        bytes: &[u8],
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.check_writable()?;
        self.inner.put_if(&self.full_key(key)?, bytes, condition)
    }

    fn copy(&self, from: &str, to: &str) -> Result<(), DriveError> {
        self.check_writable()?;
        self.inner.copy(&self.full_key(from)?, &self.full_key(to)?)
    }

    fn create_folder(&self, prefix: &str) -> Result<(), DriveError> {
        self.check_writable()?;
        self.inner.create_folder(&self.full_name(prefix)?)
    }

    fn rename(&self, from: &str, to: &str) -> Result<(), DriveError> {
        self.check_writable()?;
        self.inner
            .rename(&self.full_name(from)?, &self.full_name(to)?)
    }

    fn delete_folder(&self, prefix: &str) -> Result<(), DriveError> {
        self.check_writable()?;
        self.inner.delete_folder(&self.full_name(prefix)?)
    }

    fn remove_empty_folder(&self, prefix: &str) -> Result<bool, DriveError> {
        self.check_writable()?;
        self.inner.remove_empty_folder(&self.full_name(prefix)?)
    }

    // inherits: put_many, delete_many, move_many, put_from_many - the trait's batches call this
    // drive's own put, delete, copy and put_from, which check the grant and the key of every item

    fn local_path(&self, key: &str) -> Option<PathBuf> {
        self.inner.local_path(&self.full_name(key).ok()?)
    }

    fn site(&self) -> crate::xfer::Site {
        self.inner.site().under(&self.prefix)
    }

    fn resume_folder(&self) -> Option<PathBuf> {
        self.inner.resume_folder()
    }

    fn metadata(&self, key: &str) -> Result<Vec<(String, String)>, DriveError> {
        self.inner.metadata(&self.full_key(key)?)
    }

    fn put_file(
        &self,
        key: &str,
        path: &std::path::Path,
        ctl: &dyn Ctl,
    ) -> Result<u64, DriveError> {
        self.check_writable()?;
        self.inner.put_file(&self.full_key(key)?, path, ctl)
    }

    fn put_file_checked(
        &self,
        key: &str,
        path: &std::path::Path,
        check: &crate::checked::FileCheck,
        ctl: &dyn Ctl,
    ) -> Result<crate::checked::CheckedUpload, DriveError> {
        self.check_writable()?;
        self.inner
            .put_file_checked(&self.full_key(key)?, path, check, ctl)
    }

    fn put_from_if(
        &self,
        key: &str,
        body: &mut dyn Read,
        condition: &Precondition,
    ) -> Result<Option<String>, DriveError> {
        self.check_writable()?;
        self.inner
            .put_from_if(&self.full_key(key)?, body, condition)
    }

    fn conditional_writes(&self) -> bool {
        self.inner.conditional_writes()
    }

    fn put_file_if(
        &self,
        key: &str,
        path: &std::path::Path,
        condition: &Precondition,
        ctl: &dyn Ctl,
    ) -> Result<Option<String>, DriveError> {
        self.check_writable()?;
        self.inner
            .put_file_if(&self.full_key(key)?, path, condition, ctl)
    }
}
