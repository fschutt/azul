//! Whether two drives are the same place on this computer.
//!
//! The app makes more than one drive over one tree: Home is a drive on the home folder, a folder
//! added as a drive is another inside it, a synced listing is a drive over the synced folder
//! (`sync:<id>`), an upload's source is a drive over the file's folder (`os:<folder>`). Their ids
//! differ, so "the same drive" decided by ids says "no" for a file that is its own destination
//! and for a folder that is moved into itself - and the move's last step deletes the source.
//! These ask the disk instead: the paths the drives name, with links resolved.
//!
//! A quick answer for the transfers (AZ-S1/U4; AZ-U1 asks about folders); the transfer engine's
//! `Site` and `overlap` (ARCH28 AR5a-2), which also know case-folding volumes and bind mounts,
//! replace them.

use std::path::Path;

use crate::Drive;

/// Whether `a` and `b` are one file: the same path once their links are resolved, or (on Unix)
/// the same device and inode - a hard link, or a path spelt in another case on a disk that
/// ignores it.
fn one_file(a: &Path, b: &Path) -> bool {
    if let (Ok(x), Ok(y)) = (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        if x == y {
            return true;
        }
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let (Ok(x), Ok(y)) = (std::fs::metadata(a), std::fs::metadata(b)) {
            return x.dev() == y.dev() && x.ino() == y.ino();
        }
    }
    false
}

/// Whether the object `from` of `source` and the object `to` of `target` are one file on this
/// computer: both drives are folders on disk and the two paths name the same file - through a
/// link, a second drive over the same folder, or a hard link. `false` when either drive is not
/// on this disk or either file is not there (a file about to be made is nobody's yet).
#[must_use]
pub fn same_local_file(source: &dyn Drive, from: &str, target: &dyn Drive, to: &str) -> bool {
    match (source.local_path(from), target.local_path(to)) {
        (Some(a), Some(b)) => one_file(&a, &b),
        _ => false,
    }
}
