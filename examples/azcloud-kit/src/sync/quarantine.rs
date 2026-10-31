//! Quarantine marks for other people's files. A file that came down from a share participant
//! (not from one of the user's own devices) gets the system's "downloaded from elsewhere"
//! mark, so the system checks it before it runs: `com.apple.quarantine` on macOS (Gatekeeper),
//! the `Zone.Identifier` stream ("Mark of the Web") on Windows (SmartScreen). Linux has no such
//! check, so nothing is set there.
//!
//! The mark goes on after the download is renamed in and before the pass reports it. A file
//! that cannot be marked (a disk without extended attributes) is still synced; the pass lists
//! that once as a warning.

use std::path::Path;

/// The agent named in the mark (macOS shows it: "downloaded by AzDrive").
pub const AGENT: &str = "AzDrive";

/// Whether this system has a mark at all.
#[must_use]
pub fn supported() -> bool {
    cfg!(any(target_os = "macos", windows))
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

/// Who wrote a file, as the quarantine rule sees it: the user's own devices need no mark.
pub trait Writers: Sync {
    /// Whether `device` (a remote entry's writer) is one of the user's own devices.
    fn is_own(&self, device: &str) -> bool;
}

/// The user's own devices by name (the command line's `--own-devices`; an app passes its
/// account's devices).
#[derive(Clone, Debug, Default)]
pub struct OwnDevices(pub Vec<String>);

impl Writers for OwnDevices {
    fn is_own(&self, device: &str) -> bool {
        self.0.iter().any(|d| d == device)
    }
}
