//! The system's "downloaded from elsewhere" mark on a file (AZ-W20).
//!
//! A file that came down from a drive - another person's, or the user's own on a computer that is
//! not this one - gets the mark the system keeps for that, so the system checks it before it runs:
//! `com.apple.quarantine` on macOS (Gatekeeper), the `Zone.Identifier` stream ("Mark of the Web") on
//! Windows (SmartScreen). Linux has no such check, so nothing is set there.
//!
//! Two callers: the folder sync marks the files other people's devices wrote (azcloud-kit,
//! `sync::quarantine`), and AzDrive's own Open and Download mark what they bring to this disk
//! ([`mark_downloaded`], called by [`crate::transfer::download_guarded`]) - an `.exe`, a `.js` or a
//! `.lnk` of a shared drive opened from AzDrive was run without the system's second look.
//!
//! The mark goes on after the file is in place. A file that cannot be marked (a disk without
//! extended attributes or streams) is still there; the sync lists that once as a warning.

use std::path::Path;

/// The agent named in the mark (macOS shows it: "downloaded by AzDrive").
pub const AGENT: &str = "AzDrive";

/// Whether this system has a mark at all.
#[must_use]
pub fn supported() -> bool {
    cfg!(target_os = "macos") || crate::key::System::here() == crate::key::System::Windows
}

/// Marks the file at `path` as downloaded from someone else, at `now` (seconds). `Ok(false)`
/// on a system without marks.
///
/// # Errors
///
/// When the system has marks and this one could not be set.
pub fn mark(path: &Path, now: i64) -> std::io::Result<bool> {
    imp::mark(path, now)
}

/// Whether the file at `path` carries the mark (`false` on a system without marks).
#[must_use]
pub fn is_marked(path: &Path) -> bool {
    imp::is_marked(path)
}

/// The value of macOS's attribute: flags (0x0081: downloaded, by an app that is not the
/// system's own), the time in hex seconds, the agent, an event id.
#[must_use]
pub fn macos_value(now: i64, event: &str) -> String {
    format!("0081;{:08x};{AGENT};{event}", now.max(0))
}

/// The text of Windows's `Zone.Identifier` stream: zone 3 is "the Internet".
pub const ZONE_IDENTIFIER: &str = "[ZoneTransfer]\r\nZoneId=3\r\n";

#[cfg(target_os = "macos")]
mod imp {
    use std::{ffi::CString, os::unix::ffi::OsStrExt, path::Path};

    const NAME: &[u8] = b"com.apple.quarantine\0";

    fn c_path(path: &Path) -> std::io::Result<CString> {
        CString::new(path.as_os_str().as_bytes())
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "a NUL in the path"))
    }

    pub fn mark(path: &Path, now: i64) -> std::io::Result<bool> {
        let c = c_path(path)?;
        let event = blake3::hash(path.as_os_str().as_bytes()).to_hex();
        let value = super::macos_value(now, &event.as_str()[..32]);
        // SAFETY: both strings are NUL-terminated and live across the call; the value is
        // `value.len()` bytes. XATTR_NOFOLLOW: a link swapped in is never marked through.
        let rc = unsafe {
            libc::setxattr(
                c.as_ptr(),
                NAME.as_ptr().cast(),
                value.as_ptr().cast(),
                value.len(),
                0,
                libc::XATTR_NOFOLLOW,
            )
        };
        if rc == 0 {
            Ok(true)
        } else {
            Err(std::io::Error::last_os_error())
        }
    }

    pub fn is_marked(path: &Path) -> bool {
        let Ok(c) = c_path(path) else {
            return false;
        };
        // SAFETY: a size query (null buffer, size 0) of a NUL-terminated name and path.
        let n = unsafe {
            libc::getxattr(
                c.as_ptr(),
                NAME.as_ptr().cast(),
                std::ptr::null_mut(),
                0,
                0,
                libc::XATTR_NOFOLLOW,
            )
        };
        n > 0
    }
}

#[cfg(windows)]
mod imp {
    use std::path::Path;

    fn stream(path: &Path) -> std::path::PathBuf {
        let mut s = path.as_os_str().to_os_string();
        s.push(":Zone.Identifier");
        s.into()
    }

    pub fn mark(path: &Path, _now: i64) -> std::io::Result<bool> {
        std::fs::write(stream(path), super::ZONE_IDENTIFIER)?;
        Ok(true)
    }

    pub fn is_marked(path: &Path) -> bool {
        std::fs::read_to_string(stream(path)).is_ok_and(|t| t.contains("ZoneId=3"))
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
mod imp {
    use std::path::Path;

    pub fn mark(_path: &Path, _now: i64) -> std::io::Result<bool> {
        Ok(false)
    }

    pub fn is_marked(_path: &Path) -> bool {
        false
    }
}

/// Marks a file this program has just brought from a drive to this disk, at the time of now. A disk
/// that cannot hold the mark still has the file: the mark is a look the system takes before it runs
/// the file, and a disk without marks (FAT, a share without streams) has always had files without
/// one.
pub fn mark_downloaded(path: &Path) {
    // collapse-ok: the download landed; what a disk that keeps no marks refuses is the check the
    // system would have made, which that disk never made for anything.
    let _ = mark(
        path,
        i64::try_from(crate::time::now_unix()).unwrap_or(i64::MAX),
    );
}
