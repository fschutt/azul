//! Uploads of a local file that must hold the bytes a scan named: a sync names a blob by the
//! BLAKE3 of its file, so an upload of a file that changed since - before it starts, while its
//! parts go up, or between a dropped connection and the resumed upload - must never complete
//! ([`crate::Drive::put_file_checked`]).

use std::{
    fs::{File, Metadata},
    io::{self, Read},
    time::UNIX_EPOCH,
};

/// What a local file must still be when its upload completes: what a scan found of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileCheck {
    /// Its bytes.
    pub size: u64,
    /// Its modification time, in nanoseconds since 1970 (0 when the system does not say).
    pub modified_ns: u64,
    /// The BLAKE3 of its bytes, lowercase hex: what the upload is named by.
    pub blake3: String,
}

impl FileCheck {
    /// Whether `meta` is still the file the scan found: its size and its time. (The bytes are
    /// hashed again where an upload completes.)
    #[must_use]
    pub fn matches(&self, meta: &Metadata) -> bool {
        meta.len() == self.size && modified_ns(meta) == self.modified_ns
    }

    /// Whether `bytes` are the named ones.
    #[must_use]
    pub fn names(&self, bytes: &[u8]) -> bool {
        blake3::hash(bytes).to_hex().as_str() == self.blake3
    }
}

/// How a checked upload ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckedUpload {
    /// The object holds the file's bytes: how many.
    Sent(u64),
    /// The file is not the one the check describes (it changed): nothing was completed, and an
    /// upload under way was aborted.
    Changed,
}

/// The modification time of `meta` in nanoseconds since 1970 (0 when the system does not say).
#[must_use]
pub fn modified_ns(meta: &Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
}

/// The BLAKE3 of `file`, read in pieces from where it stands (its start, freshly opened).
///
/// # Errors
///
/// When it cannot be read.
pub fn hash_file(mut file: File) -> io::Result<String> {
    let mut hasher = blake3::Hasher::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(hasher.finalize().to_hex().to_string());
        }
        hasher.update(&buf[..n]);
    }
}

/// A file read for a streamed upload, hashed as it is read: at its end a content that is not
/// the named one fails the read, so the upload stops before it completes, and
/// [`HashCheck::changed`] says why.
pub struct HashCheck<R> {
    inner: R,
    hasher: blake3::Hasher,
    expected: String,
    read: u64,
    ended: bool,
    changed: bool,
}

impl<R: Read> HashCheck<R> {
    pub fn new(inner: R, expected: &str) -> HashCheck<R> {
        HashCheck {
            inner,
            hasher: blake3::Hasher::new(),
            expected: expected.to_string(),
            read: 0,
            ended: false,
            changed: false,
        }
    }

    /// The content was not the named one.
    #[must_use]
    pub fn changed(&self) -> bool {
        self.changed
    }

    /// How many bytes were read so far. (Not `bytes`: that name is
    /// `Read::bytes`, which a call on an owned reader resolves to first.)
    #[must_use]
    pub fn bytes_read(&self) -> u64 {
        self.read
    }
}

impl<R: Read> Read for HashCheck<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let changed = || {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "the file changed while it was sent",
            )
        };
        if self.changed {
            return Err(changed());
        }
        if self.ended {
            return Ok(0);
        }
        let n = self.inner.read(buf)?;
        if n > 0 {
            self.hasher.update(&buf[..n]);
            self.read += n as u64;
            return Ok(n);
        }
        self.ended = true;
        if self.hasher.finalize().to_hex().as_str() != self.expected {
            self.changed = true;
            return Err(changed());
        }
        Ok(0)
    }
}
