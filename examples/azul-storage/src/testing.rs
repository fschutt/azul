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

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
