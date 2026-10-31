//! What a file carries besides its bytes, its time and its executable bit: extended attributes
//! (a Finder comment, a resource fork, a tag, a PDF reader's notes) and BSD flags (hidden,
//! immutable, append-only). The sync does not carry them to other devices (rule P6); a pass
//! says so once per folder that holds any ([`super::problems::Code::XattrsNotSynced`]), so they
//! are never lost without a word.
//!
//! Attributes the system sets on its own are left out (a download's quarantine mark - the
//! sync sets it too -, provenance, the last time a file was opened, sandbox grants, SELinux
//! labels): every file would carry one, and none of them is the user's.
//!
//! The setters are here for test harnesses, which play a user's attributes and flags.

use std::path::Path;

/// Whether an attribute named `name` is one the system sets by itself, not the user's.
#[must_use]
pub fn is_system_attribute(name: &str) -> bool {
    const SYSTEM: &[&str] = &[
        "com.apple.quarantine",
        "com.apple.provenance",
        "com.apple.macl",
        "com.apple.lastuseddate#PS",
        "com.apple.metadata:kMDItemWhereFroms",
        "com.apple.metadata:kMDItemDownloadedDate",
        "com.apple.rootless",
        "com.apple.decmpfs",
    ];
    SYSTEM.contains(&name)
        || name.starts_with("security.")
        || name.starts_with("system.")
        || name.starts_with("trusted.")
}

/// The BSD flags a user sets and the sync does not carry (macOS's `chflags` names).
pub const FLAGS: &[(&str, u32)] = &[
    ("nodump", 0x0000_0001),
    ("uchg", 0x0000_0002),
    ("uappnd", 0x0000_0004),
    ("opaque", 0x0000_0008),
    ("hidden", 0x0000_8000),
    ("arch", 0x0001_0000),
    ("schg", 0x0002_0000),
    ("sappnd", 0x0004_0000),
];

/// The flags of [`FLAGS`] a pass tells about (the archive bit is no user's choice).
const NOTED_FLAGS: u32 = 0x0000_0001 | 0x0000_0002 | 0x0000_0004 | 0x0000_8000 | 0x0002_0000 | 0x0004_0000;

/// `current` with the flags of `spec` (a comma list of [`FLAGS`] names, each with `no` before
/// it to clear it; `""` clears every flag of [`FLAGS`]) applied, as `chflags` does.
///
/// # Errors
///
/// A name that is none of [`FLAGS`].
pub fn apply_flags(current: u32, spec: &str) -> Result<u32, String> {
    let all: u32 = FLAGS.iter().map(|(_, bit)| bit).fold(0, |a, b| a | b);
    if spec.trim().is_empty() {
        return Ok(current & !all);
    }
    let mut out = current;
    for word in spec.split(',').map(str::trim).filter(|w| !w.is_empty()) {
        let (clear, name) = match word.strip_prefix("no") {
            // "nodump" is a flag of its own; "nonodump" clears it.
            Some(rest) if word != "nodump" => (true, rest),
            _ => (false, word),
        };
        let bit = FLAGS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, bit)| *bit)
            .ok_or_else(|| format!("'{word}' is no flag ({})", names_of_flags()))?;
        if clear {
            out &= !bit;
        } else {
            out |= bit;
        }
    }
    Ok(out)
}

fn names_of_flags() -> String {
    FLAGS.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")
}

/// Whether the item at `path` (not followed when it is a link) carries a user's extended
/// attribute or flag. `false` where it cannot be read or the system has none.
#[must_use]
pub fn has_user_attributes(path: &Path) -> bool {
    imp::names(path).is_ok_and(|names| names.iter().any(|n| !is_system_attribute(n)))
        || imp::flags(path).is_ok_and(|flags| flags & NOTED_FLAGS != 0)
}

/// The names of the extended attributes of `path` (not followed).
///
/// # Errors
///
/// The system's error, `Unsupported` where there are none.
pub fn attribute_names(path: &Path) -> std::io::Result<Vec<String>> {
    imp::names(path)
}

/// Sets the extended attribute `name` of `path` to `value`.
///
/// # Errors
///
/// The system's error, `Unsupported` where there are none.
pub fn set_attribute(path: &Path, name: &str, value: &[u8]) -> std::io::Result<()> {
    imp::set(path, name, value)
}

/// The BSD flags of `path` (not followed; 0 where the system has none).
///
/// # Errors
///
/// When `path` cannot be read.
pub fn flags(path: &Path) -> std::io::Result<u32> {
    imp::flags(path)
}

/// Sets the BSD flags of `path`.
///
/// # Errors
///
/// The system's error, `Unsupported` where flags cannot be set this way.
pub fn set_flags(path: &Path, flags: u32) -> std::io::Result<()> {
    imp::set_flags(path, flags)
}

#[cfg(unix)]
fn c_string(text: &[u8]) -> std::io::Result<std::ffi::CString> {
    std::ffi::CString::new(text)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidInput, "a NUL in the name"))
}

/// The names of a `listxattr` buffer: NUL-terminated, one after the other.
#[cfg(unix)]
fn split_names(buffer: &[u8]) -> Vec<String> {
    buffer
        .split(|b| *b == 0)
        .filter(|n| !n.is_empty())
        .map(|n| String::from_utf8_lossy(n).into_owned())
        .collect()
}

#[cfg(target_os = "macos")]
mod imp {
    use std::{os::unix::ffi::OsStrExt, path::Path};

    pub fn names(path: &Path) -> std::io::Result<Vec<String>> {
        let c = super::c_string(path.as_os_str().as_bytes())?;
        // SAFETY: a size query (null buffer, size 0) of a NUL-terminated path.
        let n = unsafe { libc::listxattr(c.as_ptr(), std::ptr::null_mut(), 0, libc::XATTR_NOFOLLOW) };
        if n < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if n == 0 {
            return Ok(Vec::new());
        }
        let mut buffer = vec![0u8; n as usize];
        // SAFETY: the buffer holds `buffer.len()` bytes; the call writes at most that many.
        let n = unsafe {
            libc::listxattr(
                c.as_ptr(),
                buffer.as_mut_ptr().cast(),
                buffer.len(),
                libc::XATTR_NOFOLLOW,
            )
        };
        if n < 0 {
            return Err(std::io::Error::last_os_error());
        }
        buffer.truncate(n as usize);
        Ok(super::split_names(&buffer))
    }

    pub fn set(path: &Path, name: &str, value: &[u8]) -> std::io::Result<()> {
        let c = super::c_string(path.as_os_str().as_bytes())?;
        let n = super::c_string(name.as_bytes())?;
        // SAFETY: NUL-terminated path and name; the value is `value.len()` bytes.
        let rc = unsafe {
            libc::setxattr(
                c.as_ptr(),
                n.as_ptr(),
                value.as_ptr().cast(),
                value.len(),
                0,
                libc::XATTR_NOFOLLOW,
            )
        };
        if rc == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }

    pub fn flags(path: &Path) -> std::io::Result<u32> {
        let c = super::c_string(path.as_os_str().as_bytes())?;
        // SAFETY: `st` is written by lstat before it is read; the path is NUL-terminated.
        let mut st: libc::stat = unsafe { std::mem::zeroed() };
        let rc = unsafe { libc::lstat(c.as_ptr(), &mut st) };
        if rc != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(st.st_flags)
    }

    pub fn set_flags(path: &Path, flags: u32) -> std::io::Result<()> {
        let c = super::c_string(path.as_os_str().as_bytes())?;
        // SAFETY: a NUL-terminated path.
        let rc = unsafe { libc::chflags(c.as_ptr(), flags.into()) };
        if rc == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use std::{os::unix::ffi::OsStrExt, path::Path};

    pub fn names(path: &Path) -> std::io::Result<Vec<String>> {
        let c = super::c_string(path.as_os_str().as_bytes())?;
        // SAFETY: a size query (null buffer, size 0) of a NUL-terminated path.
        let n = unsafe { libc::llistxattr(c.as_ptr(), std::ptr::null_mut(), 0) };
        if n < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if n == 0 {
            return Ok(Vec::new());
        }
        let mut buffer = vec![0u8; n as usize];
        // SAFETY: the buffer holds `buffer.len()` bytes; the call writes at most that many.
        let n = unsafe { libc::llistxattr(c.as_ptr(), buffer.as_mut_ptr().cast(), buffer.len()) };
        if n < 0 {
            return Err(std::io::Error::last_os_error());
        }
        buffer.truncate(n as usize);
        Ok(super::split_names(&buffer))
    }

    pub fn set(path: &Path, name: &str, value: &[u8]) -> std::io::Result<()> {
        let c = super::c_string(path.as_os_str().as_bytes())?;
        let n = super::c_string(name.as_bytes())?;
        // SAFETY: NUL-terminated path and name; the value is `value.len()` bytes.
        let rc = unsafe { libc::lsetxattr(c.as_ptr(), n.as_ptr(), value.as_ptr().cast(), value.len(), 0) };
        if rc == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }

    /// Linux keeps its file flags (chattr) behind an ioctl the sync never reads: none here.
    pub fn flags(path: &Path) -> std::io::Result<u32> {
        std::fs::symlink_metadata(path).map(|_| 0)
    }

    pub fn set_flags(_path: &Path, _flags: u32) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "BSD file flags are macOS's (Linux has chattr)",
        ))
    }
}

#[cfg(not(unix))]
mod imp {
    use std::path::Path;

    fn unsupported() -> std::io::Error {
        std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "this system keeps no extended attributes or BSD flags",
        )
    }

    pub fn names(_path: &Path) -> std::io::Result<Vec<String>> {
        Err(unsupported())
    }

    pub fn set(_path: &Path, _name: &str, _value: &[u8]) -> std::io::Result<()> {
        Err(unsupported())
    }

    pub fn flags(path: &Path) -> std::io::Result<u32> {
        std::fs::symlink_metadata(path).map(|_| 0)
    }

    pub fn set_flags(_path: &Path, _flags: u32) -> std::io::Result<()> {
        Err(unsupported())
    }
}
