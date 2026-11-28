//! The folder the working copies of what the system opened the app with live in, and the limits of making them.
//!
//! A file another app shares to this one is a `content://` URI (Android) or a security-scoped URL (iOS): neither opens with
//! `std::fs`, and the permission the sender gave may end with the activity. So the engine makes a WORKING COPY in the app's own
//! folder - `<files>/azul-inbox/<token>/<name>` - on a worker thread, and the app opens that. The copy is the app's: it may
//! edit it, keep it or delete it; the engine only removes folders of copies that nobody has touched for a month, so that an
//! app that opens a hundred attachments does not fill the phone.
//!
//! Every limit is a cap with the reason in the answer, never a crash: a file over [`OpenLimits::max_file_bytes`] is not copied
//! (the item says so, with its size and the limit), the copies of one request stop at [`OpenLimits::max_request_bytes`], a name
//! is made safe and unique.

use alloc::{string::String, vec::Vec};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use crate::managers::{
    share::{sanitize_display_name, unique_names},
    share_area::new_token,
};

/// The limits of the copies of one request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OpenLimits {
    /// One file, in bytes.
    pub max_file_bytes: u64,
    /// All the copies of one request together, in bytes.
    pub max_request_bytes: u64,
    /// A file name, in bytes of UTF-8.
    pub max_name_bytes: usize,
}

impl Default for OpenLimits {
    fn default() -> Self {
        Self {
            max_file_bytes: 256 * 1024 * 1024,
            max_request_bytes: 1024 * 1024 * 1024,
            max_name_bytes: 128,
        }
    }
}

/// How long a request's folder is kept when nothing in it was touched.
pub const KEEP_FOR: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// The folders of all requests of an app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inbox {
    root: PathBuf,
}

/// The folder of ONE request and what has been copied into it so far.
#[derive(Debug)]
pub struct InboxRequest {
    dir: PathBuf,
    names: Vec<String>,
    total: u64,
    limits: OpenLimits,
}

/// A working copy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copied {
    /// Where it is.
    pub path: PathBuf,
    /// The name it has (the wanted one, made safe and unique inside the request).
    pub name: String,
    /// Bytes.
    pub size: u64,
}

impl Inbox {
    /// The inbox at `root` (the folder is made at the first request).
    #[must_use]
    pub const fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// The folder of the inbox.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Start a request: a folder of its own under a fresh token. Folders nobody touched for [`KEEP_FOR`] are removed first.
    ///
    /// # Errors
    ///
    /// The reason the folder could not be made.
    pub fn begin(&self, limits: OpenLimits) -> Result<InboxRequest, String> {
        fs::create_dir_all(&self.root).map_err(|e| format!("cannot make the folder of the opened files: {e}"))?;
        let _removed = self.cleanup(SystemTime::now(), KEEP_FOR);
        let dir = self.root.join(new_token());
        fs::create_dir(&dir).map_err(|e| format!("cannot make the folder of this request: {e}"))?;
        Ok(InboxRequest {
            dir,
            names: Vec::new(),
            total: 0,
            limits,
        })
    }

    /// Remove the request folders last written longer than `keep` before `now`; how many went.
    pub fn cleanup(&self, now: SystemTime, keep: Duration) -> usize {
        let Ok(entries) = fs::read_dir(&self.root) else {
            return 0;
        };
        let mut removed = 0;
        for entry in entries.filter_map(Result::ok) {
            let is_request = entry.file_name().to_str().is_some_and(crate::managers::share_area::is_token);
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            if !is_request || !meta.is_dir() {
                continue;
            }
            let newest = newest_write(&entry.path()).unwrap_or_else(|| meta.modified().unwrap_or(SystemTime::UNIX_EPOCH));
            if now.duration_since(newest).unwrap_or(Duration::ZERO) > keep && fs::remove_dir_all(entry.path()).is_ok() {
                removed += 1;
            }
        }
        removed
    }
}

/// The latest modification time of the folder and the files directly in it.
fn newest_write(dir: &Path) -> Option<SystemTime> {
    let mut newest = fs::metadata(dir).ok()?.modified().ok()?;
    for entry in fs::read_dir(dir).ok()?.filter_map(Result::ok) {
        if let Ok(modified) = entry.metadata().and_then(|m| m.modified()) {
            newest = newest.max(modified);
        }
    }
    Some(newest)
}

impl InboxRequest {
    /// The folder of this request.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// How many bytes this request has copied so far.
    #[must_use]
    pub const fn total(&self) -> u64 {
        self.total
    }

    /// Copy what `reader` yields into a new file called (about) `wanted` and answer where it is, or WHY not: too large (before a
    /// byte is read when the size is known, else when the cap is crossed: the partial file is removed), the request's budget
    /// is used up, the reader failed, the file could not be written. A name is made safe and unique inside this request.
    ///
    /// # Errors
    ///
    /// The reason, for the item's `error`.
    pub fn copy_in(&mut self, wanted: &str, reader: &mut dyn Read, size_hint: Option<u64>) -> Result<Copied, String> {
        let limits = self.limits;
        let name = {
            let mut names = self.names.clone();
            names.push(sanitize_display_name(wanted, limits.max_name_bytes));
            unique_names(names).pop().unwrap_or_else(|| "file".to_string())
        };
        if let Some(size) = size_hint {
            if size > limits.max_file_bytes {
                return Err(too_large(&name, size, limits.max_file_bytes));
            }
            if self.total.saturating_add(size) > limits.max_request_bytes {
                return Err(request_full(self.total, size, limits.max_request_bytes));
            }
        }
        let path = self.dir.join(&name);
        let mut out = fs::File::create(&path).map_err(|e| format!("{name} cannot be copied into the app's folder: {e}"))?;
        let budget = limits
            .max_file_bytes
            .min(limits.max_request_bytes.saturating_sub(self.total));
        let mut copied: u64 = 0;
        let mut buf = vec![0_u8; 64 * 1024];
        let outcome = loop {
            let n = match reader.read(&mut buf) {
                Ok(0) => break Ok(()),
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => break Err(format!("{name} could not be read from where it came from: {e}")),
            };
            copied += n as u64;
            if copied > budget {
                break Err(if budget < limits.max_file_bytes {
                    request_full(self.total, copied, limits.max_request_bytes)
                } else {
                    too_large(&name, copied, limits.max_file_bytes)
                });
            }
            if let Err(e) = out.write_all(&buf[..n]) {
                break Err(format!("{name} cannot be written into the app's folder: {e}"));
            }
        };
        match outcome {
            Ok(()) => {
                self.total += copied;
                self.names.push(name.clone());
                Ok(Copied { path, name, size: copied })
            }
            Err(reason) => {
                drop(out);
                drop(fs::remove_file(&path));
                Err(reason)
            }
        }
    }
}

/// Where a working copy came from: what a save writes BACK to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Origin {
    /// The locator `Documents` takes (a `content://` document URI on Android).
    pub uri: String,
    /// The sender or the picker granted writing to it.
    pub writable: bool,
}

/// The file that records the origin of the working copy `copy`: `<folder>/.<name>.origin`. A name that starts with a dot is never made
/// by [`InboxRequest::copy_in`] (it strips them), so the record cannot be mistaken for a copy.
#[must_use]
pub fn origin_file(copy: &Path) -> Option<PathBuf> {
    let name = copy.file_name()?.to_str()?;
    Some(copy.with_file_name(format!(".{name}.origin")))
}

/// Record where `copy` came from (next to it, so that the record is removed with the folder and survives a restart).
///
/// # Errors
///
/// What writing the record said.
pub fn write_origin(copy: &Path, origin: &Origin) -> std::io::Result<()> {
    let record = origin_file(copy).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "a copy with no file name"))?;
    fs::write(record, format!("{}\n{}\n", origin.uri, if origin.writable { "writable" } else { "read-only" }))
}

/// Where `copy` came from, or `None` for a file that has no record (it was not opened from somewhere else, or it is not a working
/// copy at all).
#[must_use]
pub fn read_origin(copy: &Path) -> Option<Origin> {
    let text = fs::read_to_string(origin_file(copy)?).ok()?;
    let mut lines = text.lines();
    let uri = lines.next().filter(|u| !u.is_empty())?.to_string();
    let writable = lines.next() == Some("writable");
    Some(Origin { uri, writable })
}

fn too_large(name: &str, size: u64, limit: u64) -> String {
    format!(
        "{name} is too large to copy into the app ({} MiB; the limit is {} MiB)",
        size.div_ceil(1024 * 1024),
        limit / (1024 * 1024)
    )
}

fn request_full(used: u64, wanted: u64, limit: u64) -> String {
    format!(
        "no room left in this request for another file ({} MiB copied, {} MiB more wanted; the limit is {} MiB in all)",
        used.div_ceil(1024 * 1024),
        wanted.div_ceil(1024 * 1024),
        limit / (1024 * 1024)
    )
}

#[cfg(test)]
#[path = "open_inbox_test.rs"]
mod open_inbox_test;
