//! AzCode: a code editor on the public azul API.
//!
//! The window is the S8 `DeveloperShell` (the app-drawn `Titlebar` under
//! `WindowDecorations::NoTitle`; the activity bar, the explorer, document
//! tabs over azul's `CodeView`, the status bar) inside a `ShellThemeScope`,
//! behind a `CloseGuard` that asks "Save changes?". It follows the app theme
//! (flat / flora) and the OS mode.
//!
//! - [`buffer`]: the text of an open file, a piece table (plain Rust).
//! - [`highlight`]: syntect, incremental by line, checkpoints, a background
//!   job for far jumps (plain Rust).
//!
//! On stdout, for scripts (`scripts/azcode_e2e.py`): `AZCODE_READY`,
//! `AZCODE_LISTED <folder> <n>`, `AZCODE_OPENED <path> <lines>`,
//! `AZCODE_SAVED <path>`, `AZCODE_FOUND <n>`, `AZCODE_REPLACED <n>`.

pub mod buffer;
pub mod highlight;
pub mod search;
pub mod workspace;

/// Starts the app (filled in by the UI half of this crate).
pub fn start() {}
