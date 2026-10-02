//! Tests only (this crate's, and the apps' through the `test-util` feature): a folder of its own
//! under the system's temporary folder, removed when dropped. AzCalendar's `test_dir.rs` and
//! AzTasks' store tests each had this copy (scripts/DEDUP_EDITORS_2026_10_02.md, B31).

use std::{
    path::PathBuf,
    sync::atomic::{AtomicU32, Ordering},
};

/// A folder of its own under the system's temporary folder, removed when dropped.
pub struct TempDir(pub PathBuf);

impl TempDir {
    /// A new empty folder, named after the process and a counter (two parallel test runs never
    /// share one).
    ///
    /// # Panics
    /// When the folder cannot be made.
    #[must_use]
    pub fn create() -> Self {
        static NEXT: AtomicU32 = AtomicU32::new(0);
        let path = std::env::temp_dir().join(format!(
            "azul-pim-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("a temporary folder");
        TempDir(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
