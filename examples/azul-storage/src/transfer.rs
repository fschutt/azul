//! Moving ONE object between a drive and a local file: what "Download", "Open"
//! and "Upload" do in AzDrive, and what an export does in AzMail.

use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
};

use crate::{
    key::safe_file_name,
    local::{temp_sibling, write_atomically},
    ByteRange, Drive, DriveError,
};

/// Bytes per ranged GET when an object is bigger than this.
pub const CHUNK: u64 = 8 * 1024 * 1024;

/// A reader that tells `progress` the bytes read so far after every read: a stream's upload
/// progress ([`Drive::put_file`]'s default).
pub struct ProgressReader<'a> {
    inner: &'a mut dyn Read,
    read: u64,
    progress: &'a (dyn Fn(u64) + Sync),
}

impl<'a> ProgressReader<'a> {
    pub fn new(inner: &'a mut dyn Read, progress: &'a (dyn Fn(u64) + Sync)) -> Self {
        ProgressReader {
            inner,
            read: 0,
            progress,
        }
    }
}

impl Read for ProgressReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n > 0 {
            self.read += n as u64;
            (self.progress)(self.read);
        }
        Ok(n)
    }
}

/// Writes the ranged chunks of `key` into `file`; returns the bytes written.
/// `progress` hears the bytes written so far after every chunk.
fn copy_ranges(
    drive: &dyn Drive,
    key: &str,
    size: u64,
    chunk: u64,
    file: &mut File,
    progress: &mut dyn FnMut(u64),
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
        progress(offset);
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
    download_with_progress(drive, key, size, dest, chunk, &mut |_| {})
}

/// [`download_to_file`] that tells `progress` the bytes written so far after
/// every chunk (once, at the end, for an object that fits in one GET).
pub fn download_with_progress(
    drive: &dyn Drive,
    key: &str,
    size: Option<u64>,
    dest: &Path,
    chunk: u64,
    progress: &mut dyn FnMut(u64),
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
        progress(bytes.len() as u64);
        return Ok(bytes.len() as u64);
    }
    let tmp = temp_sibling(dest);
    let copied = File::create(&tmp)
        .map_err(DriveError::from)
        .and_then(|mut file| copy_ranges(drive, key, size, chunk, &mut file, progress));
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

/// Bytes per read when a file is copied on disk.
const DISK_CHUNK: usize = 1024 * 1024;

/// Copies the file `source` to `dest` through a temporary file next to it,
/// telling `progress` the bytes copied so far after every megabyte.
fn copy_file(source: &Path, dest: &Path, progress: &mut dyn FnMut(u64)) -> Result<u64, DriveError> {
    if let Some(parent) = dest.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let tmp = temp_sibling(dest);
    let copied = (|| -> Result<u64, DriveError> {
        let mut from = File::open(source)?;
        let mut to = File::create(&tmp)?;
        let mut buffer = vec![0u8; DISK_CHUNK];
        let mut total = 0u64;
        loop {
            let read = from.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            to.write_all(&buffer[..read])?;
            total += read as u64;
            progress(total);
        }
        to.flush()?;
        drop(to);
        fs::rename(&tmp, dest)?;
        Ok(total)
    })();
    if copied.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    copied
}

/// Copies ONE object from `source` to `target` (the same drive or another):
/// a file copy on disk when both are folders on this computer, ranged GETs
/// of [`CHUNK`] bytes into the target's file when only the target is, one
/// read and one PUT otherwise (a bucket takes an object in one request; the
/// whole object passes through memory). `size` comes from the listing;
/// `None` asks with a HEAD when it matters. `progress` hears the bytes
/// copied so far. Returns the bytes copied. Overwrites `target_key`:
/// conflicts are the caller's.
pub fn copy_object(
    source: &dyn Drive,
    key: &str,
    size: Option<u64>,
    target: &dyn Drive,
    target_key: &str,
    progress: &mut dyn FnMut(u64),
) -> Result<u64, DriveError> {
    match (source.local_path(key), target.local_path(target_key)) {
        (Some(from), Some(to)) => {
            if !from.is_file() {
                return Err(DriveError::NotFound {
                    key: key.to_string(),
                });
            }
            copy_file(&from, &to, progress)
        }
        (None, Some(to)) => download_with_progress(source, key, size, &to, CHUNK, progress),
        (from, None) => {
            let bytes = match from {
                Some(path) => fs::read(&path).map_err(|e| {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        DriveError::NotFound {
                            key: key.to_string(),
                        }
                    } else {
                        DriveError::from(e)
                    }
                })?,
                None => source.get(key)?,
            };
            target.put(target_key, &bytes)?;
            progress(bytes.len() as u64);
            Ok(bytes.len() as u64)
        }
    }
}
