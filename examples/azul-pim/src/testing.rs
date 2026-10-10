//! Tests only (this crate's, and the apps' through the `test-util` feature): a folder of its own
//! under the system's temporary folder, removed when dropped. The one copy lives in azul-storage
//! (`azul_storage::testing`, its `testing` feature); this is its re-export, so the apps that
//! depend on azul-pim's `test-util` keep their path. AzCalendar's `test_dir.rs` and AzTasks'
//! store tests each had a copy once (scripts/DEDUP_EDITORS_2026_10_02.md, B31).

pub use azul_storage::testing::TempDir;
