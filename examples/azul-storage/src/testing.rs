//! Tests only (this crate's, and the apps' through the `testing` feature): a folder of its own
//! under the system's temporary folder, removed when dropped. The one copy: azul-pim
//! re-exports it, and AzMail, AzDrive, AzReader and azul-appkit had their own
//! (scripts/waves/wave9/SMALL_FIXES.md 5.6).

use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU32, Ordering},
};

/// A folder of its own under the system's temporary folder, removed when dropped.
pub struct TempDir(pub PathBuf);

impl TempDir {
    /// A new empty folder (named after the process and a counter: two parallel test runs
    /// never share one).
    ///
    /// # Panics
    /// When the folder cannot be made.
    #[must_use]
    pub fn create() -> Self {
        Self::new("test")
    }

    /// A new empty folder whose name says what it is for (`what`, as in
    /// `azul-test-<what>-<process>-<counter>`).
    ///
    /// # Panics
    /// When the folder cannot be made.
    #[must_use]
    pub fn new(what: &str) -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "azul-test-{what}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        // A folder of an earlier run with the same process id.
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a temporary folder");
        TempDir(path)
    }

    /// The folder.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.0
    }
}

/// Binds a Unix socket named `name` in the folder `dir`, however long the folder's path is: a
/// socket's address holds 104 bytes on macOS, and the folder of a test under `$TMPDIR` is longer.
/// The bind goes through a short link to the folder in `/tmp`, removed at once (the socket stays
/// where it was made).
///
/// # Errors
///
/// When the link or the socket cannot be made.
#[cfg(unix)]
pub fn bind_unix_socket(
    dir: &Path,
    name: &str,
) -> std::io::Result<std::os::unix::net::UnixListener> {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let link = Path::new("/tmp").join(format!(
        "azs-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::os::unix::fs::symlink(dir, &link)?;
    let bound = std::os::unix::net::UnixListener::bind(link.join(name));
    // The link was only the short way to the folder.
    std::fs::remove_file(&link)?;
    bound
}

/// Makes the chain `levels` deep of folders called `name` under `top` with `openat` (the path of the
/// deep ones is past what a path-based call takes), calling `each(level, folder)` on the way down: a
/// test's tree for the walks that must not recurse. The folders are made by the walk that makes (the
/// crate's own `Dir::make_child`), which no code outside this crate and its tests can reach.
///
/// # Panics
///
/// When `top` cannot be opened or a folder of the chain cannot be made.
#[cfg(unix)]
pub fn make_folder_chain(
    top: &Path,
    name: &str,
    levels: usize,
    mut each: impl FnMut(usize, &crate::beneath::Dir),
) {
    let tree = crate::beneath::Root::open(top).expect("the top of the chain opens");
    let mut at = tree.top().expect("the root's own folder");
    for level in 1..=levels {
        at = at.make_child(name).expect("a folder of the chain");
        each(level, &at);
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
