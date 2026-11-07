//! Extended attributes carried with a copy on this computer (X-DELTA-AZDRIVE-FILES-16).
//!
//! A file manager's copy keeps what a file carries besides its bytes: on macOS the Finder tags and
//! labels, the comments, "where from" and the resource fork (they are extended attributes); on
//! Linux the `user.*` attributes and the ACLs. The copy of this engine had the bytes, the mode and
//! the time (AZ-U9), nothing else - and a Move across disks deleted the source, with no message.
//!
//! [`carry`] puts the attributes of the source on the copy after it landed. What the disk of the
//! copy would not take comes back in [`Carried::lost`]: a Copy says it once, a Move keeps its
//! source (`xfer::run`). Attributes the system manages itself (macOS' provenance and access list,
//! Linux' security labels, which the target's policy gives) are not carried and are never "lost".
//!
//! Only macOS and Linux have extended attributes here; Windows' alternate data streams are not
//! carried, and nothing there says so.

use std::path::Path;

/// What carrying the attributes of a file did.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Carried {
    /// Attributes now on the copy.
    pub carried: usize,
    /// The names the copy does not have: the disk refused them, or the source's value could not
    /// be read. Empty: the copy has everything the source carries (or the source carries nothing).
    pub lost: Vec<String>,
}

/// Whether the system manages the attribute `name` itself: it is set by the kernel, a daemon or
/// the target's policy, and a copy cannot (or must not) carry it.
#[must_use]
pub fn is_managed(name: &[u8]) -> bool {
    name.starts_with(b"com.apple.provenance")
        || name == b"com.apple.macl"
        || name == b"com.apple.rootless"
        || name.starts_with(b"security.")
}

/// Puts the extended attributes of the file `from` on the file `to`, which landed as its copy.
#[must_use]
pub fn carry(from: &Path, to: &Path) -> Carried {
    carry_with(from, to, &imp::set)
}

/// [`carry`] with the setter a test replaces (a disk that refuses).
#[must_use]
pub(crate) fn carry_with(
    from: &Path,
    to: &Path,
    set: &dyn Fn(&Path, &[u8], &[u8]) -> std::io::Result<()>,
) -> Carried {
    let mut out = Carried::default();
    // A source whose attributes cannot be listed (a disk without any, a file gone) has none to
    // carry: there is nothing to say.
    let Ok(names) = imp::names(from) else {
        return out;
    };
    for name in names {
        if is_managed(&name) {
            continue;
        }
        let carried = imp::value(from, &name).and_then(|value| set(to, &name, &value));
        match carried {
            Ok(()) => out.carried += 1,
            Err(_) => out.lost.push(String::from_utf8_lossy(&name).into_owned()),
        }
    }
    out
}

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "android"))]
mod imp {
    use std::{
        ffi::{c_char, c_void, CString},
        io,
        os::unix::ffi::OsStrExt,
        path::Path,
    };

    fn c_path(path: &Path) -> io::Result<CString> {
        CString::new(path.as_os_str().as_bytes())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "a NUL in the path"))
    }

    fn c_name(name: &[u8]) -> io::Result<CString> {
        CString::new(name)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "a NUL in the name"))
    }

    /// How often a size that changed between the question and the answer is asked again.
    const TRIES: usize = 4;

    // The link itself is the subject of each call: a link swapped in for the file is never followed.
    #[cfg(target_os = "macos")]
    fn list(path: *const c_char, buf: *mut c_char, size: usize) -> isize {
        // SAFETY: a NUL-terminated path, and a buffer of `size` bytes (or null with size 0).
        unsafe { libc::listxattr(path, buf, size, libc::XATTR_NOFOLLOW) }
    }

    #[cfg(not(target_os = "macos"))]
    fn list(path: *const c_char, buf: *mut c_char, size: usize) -> isize {
        // SAFETY: a NUL-terminated path, and a buffer of `size` bytes (or null with size 0).
        unsafe { libc::llistxattr(path, buf, size) }
    }

    #[cfg(target_os = "macos")]
    fn get(path: *const c_char, name: *const c_char, buf: *mut c_void, size: usize) -> isize {
        // SAFETY: a NUL-terminated path and name, and a buffer of `size` bytes (or null, size 0).
        unsafe { libc::getxattr(path, name, buf, size, 0, libc::XATTR_NOFOLLOW) }
    }

    #[cfg(not(target_os = "macos"))]
    fn get(path: *const c_char, name: *const c_char, buf: *mut c_void, size: usize) -> isize {
        // SAFETY: a NUL-terminated path and name, and a buffer of `size` bytes (or null, size 0).
        unsafe { libc::lgetxattr(path, name, buf, size) }
    }

    #[cfg(target_os = "macos")]
    fn put(path: *const c_char, name: *const c_char, value: *const c_void, size: usize) -> i32 {
        // SAFETY: a NUL-terminated path and name, and `size` bytes of value.
        unsafe { libc::setxattr(path, name, value, size, 0, libc::XATTR_NOFOLLOW) }
    }

    #[cfg(not(target_os = "macos"))]
    fn put(path: *const c_char, name: *const c_char, value: *const c_void, size: usize) -> i32 {
        // SAFETY: a NUL-terminated path and name, and `size` bytes of value.
        unsafe { libc::lsetxattr(path, name, value, size, 0) }
    }

    /// The names of the attributes of the file at `path`.
    pub fn names(path: &Path) -> io::Result<Vec<Vec<u8>>> {
        let c = c_path(path)?;
        for _ in 0..TRIES {
            let size = list(c.as_ptr(), std::ptr::null_mut(), 0);
            if size < 0 {
                return Err(io::Error::last_os_error());
            }
            if size == 0 {
                return Ok(Vec::new());
            }
            let mut buf = vec![0u8; usize::try_from(size).unwrap_or(0)];
            let got = list(c.as_ptr(), buf.as_mut_ptr().cast(), buf.len());
            if got < 0 {
                let e = io::Error::last_os_error();
                if e.raw_os_error() == Some(libc::ERANGE) {
                    continue;
                }
                return Err(e);
            }
            buf.truncate(usize::try_from(got).unwrap_or(0));
            return Ok(buf
                .split(|b| *b == 0)
                .filter(|name| !name.is_empty())
                .map(<[u8]>::to_vec)
                .collect());
        }
        Err(io::Error::other("the attributes kept changing"))
    }

    /// The value of the attribute `name` of the file at `path`.
    pub fn value(path: &Path, name: &[u8]) -> io::Result<Vec<u8>> {
        let (c, n) = (c_path(path)?, c_name(name)?);
        for _ in 0..TRIES {
            let size = get(c.as_ptr(), n.as_ptr(), std::ptr::null_mut(), 0);
            if size < 0 {
                return Err(io::Error::last_os_error());
            }
            let mut buf = vec![0u8; usize::try_from(size).unwrap_or(0)];
            if buf.is_empty() {
                return Ok(buf);
            }
            let got = get(c.as_ptr(), n.as_ptr(), buf.as_mut_ptr().cast(), buf.len());
            if got < 0 {
                let e = io::Error::last_os_error();
                if e.raw_os_error() == Some(libc::ERANGE) {
                    continue;
                }
                return Err(e);
            }
            buf.truncate(usize::try_from(got).unwrap_or(0));
            return Ok(buf);
        }
        Err(io::Error::other("the attribute kept changing"))
    }

    /// Sets the attribute `name` of the file at `path` to `value`.
    pub fn set(path: &Path, name: &[u8], value: &[u8]) -> io::Result<()> {
        let (c, n) = (c_path(path)?, c_name(name)?);
        let rc = put(c.as_ptr(), n.as_ptr(), value.as_ptr().cast(), value.len());
        if rc == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "android")))]
mod imp {
    use std::{io, path::Path};

    pub fn names(_path: &Path) -> io::Result<Vec<Vec<u8>>> {
        Ok(Vec::new())
    }

    pub fn value(_path: &Path, _name: &[u8]) -> io::Result<Vec<u8>> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }

    pub fn set(_path: &Path, _name: &[u8], _value: &[u8]) -> io::Result<()> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }
}

/// Sets one attribute (a test makes a source that carries something).
#[cfg(test)]
pub(crate) fn set_for_test(path: &Path, name: &str, value: &[u8]) -> std::io::Result<()> {
    imp::set(path, name.as_bytes(), value)
}

/// Reads one attribute (a test looks at what the copy has).
#[cfg(test)]
pub(crate) fn value_for_test(path: &Path, name: &str) -> std::io::Result<Vec<u8>> {
    imp::value(path, name.as_bytes())
}
