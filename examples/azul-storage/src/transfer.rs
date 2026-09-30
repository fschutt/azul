//! Moving ONE object between a drive and a local file: what "Download", "Open"
//! and "Upload" do in AzDrive, and what an export does in AzMail.

use std::{
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use crate::{
    key::safe_file_name,
    local::{temp_sibling, write_atomically},
    ByteRange, Drive, DriveError,
};

/// Bytes per ranged GET when an object is bigger than this.
pub const CHUNK: u64 = 8 * 1024 * 1024;

/// Writes the ranged chunks of `key` into `file`; returns the bytes written.
fn copy_ranges(
    drive: &dyn Drive,
    key: &str,
    size: u64,
    chunk: u64,
    file: &mut File,
) -> Result<u64, DriveError> {
    let mut offset = 0u64;
    while offset < size {
        let end = (offset + chunk).min(size) - 1;
        let bytes = drive.get_range(key, ByteRange::new(offset, Some(end)))?;
        if bytes.is_empty() {
            return Err(DriveError::Protocol(format!(
                "\"{key}\" ended at byte {offset} of {size}"
            )));
        }
        file.write_all(&bytes)?;
        offset += bytes.len() as u64;
    }
    file.flush()?;
    Ok(offset)
}

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
    let chunk = chunk.max(1);
    let size = match size {
        Some(size) => size,
        None => drive.head(key)?.size,
    };
    if let Some(parent) = dest.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    if size <= chunk {
        let bytes = drive.get(key)?;
        write_atomically(dest, &bytes)?;
        return Ok(bytes.len() as u64);
    }
    let tmp = temp_sibling(dest);
    let copied = File::create(&tmp)
        .map_err(DriveError::from)
        .and_then(|mut file| copy_ranges(drive, key, size, chunk, &mut file));
    let renamed = copied.and_then(|written| {
        fs::rename(&tmp, dest)?;
        Ok(written)
    });
    if renamed.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    renamed
}

/// Where a download of `key` goes in `folder`: the key's file name made safe
/// ([`crate::key::safe_file_name`]), with ` (1)`, ` (2)`, ... before the
/// extension when that name is taken. `None` when the key has no usable name.
#[must_use]
pub fn download_path(folder: &Path, key: &str) -> Option<PathBuf> {
    let name = safe_file_name(key)?;
    let first = folder.join(&name);
    if !first.exists() {
        return Some(first);
    }
    let (stem, extension) = match name.rfind('.') {
        Some(i) if i > 0 => (&name[..i], &name[i..]),
        _ => (name.as_str(), ""),
    };
    (1..10_000)
        .map(|n| folder.join(format!("{stem} ({n}){extension}")))
        .find(|candidate| !candidate.exists())
}

/// Puts the local file `path` as `key`. Returns the bytes sent.
pub fn upload_file(drive: &dyn Drive, path: &Path, key: &str) -> Result<u64, DriveError> {
    let bytes = fs::read(path)?;
    drive.put(key, &bytes)?;
    Ok(bytes.len() as u64)
}
