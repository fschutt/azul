//! Moving ONE object between a drive and a local file: what "Download", "Open"
//! and "Upload" do in AzDrive, and what an export does in AzMail.

use std::path::{Path, PathBuf};

use crate::{Drive, DriveError};

/// Bytes per ranged GET when an object is bigger than this.
pub const CHUNK: u64 = 8 * 1024 * 1024;

/// Downloads `key` to `dest`: one GET when it fits in `chunk` bytes, ranged GETs
/// of `chunk` bytes otherwise, so a big object never sits in memory whole.
/// `size` comes from the listing; `None` asks with a HEAD first. The bytes go to
/// a temporary file next to `dest`, renamed when complete, so a failed download
/// leaves nothing behind. Returns the bytes written.
pub fn download_to_file(
    drive: &dyn Drive,
    key: &str,
    size: Option<u64>,
    dest: &Path,
    chunk: u64,
) -> Result<u64, DriveError> {
    let _ = (drive, key, size, dest, chunk);
    todo!("RED")
}

/// Where a download of `key` goes in `folder`: the key's file name made safe
/// ([`crate::key::safe_file_name`]), with ` (1)`, ` (2)`, ... before the
/// extension when that name is taken. `None` when the key has no usable name.
#[must_use]
pub fn download_path(folder: &Path, key: &str) -> Option<PathBuf> {
    let _ = (folder, key);
    todo!("RED")
}

/// Puts the local file `path` as `key`. Returns the bytes sent.
pub fn upload_file(drive: &dyn Drive, path: &Path, key: &str) -> Result<u64, DriveError> {
    let _ = (drive, path, key);
    todo!("RED")
}
