//! A folder on disk as a drive. The Home drive of AzDrive, and a local export
//! target for AzMail.

use std::path::{Path, PathBuf};

use crate::{ByteRange, Drive, DriveError, ListPage, ListRequest, ObjectInfo};

/// A folder on disk: the key `a/b.txt` is the file `<root>/a/b.txt`. Keys that
/// would leave the root (`..`, absolute paths) are refused before any file is
/// touched. Symbolic links inside the root are followed.
#[derive(Debug, Clone)]
pub struct LocalDrive {
    root: PathBuf,
}

impl LocalDrive {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        LocalDrive { root: root.into() }
    }

    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }
}

impl Drive for LocalDrive {
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
