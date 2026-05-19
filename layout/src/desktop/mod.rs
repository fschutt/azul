//! Desktop-specific APIs (dialogs, file operations)

// Native modal dialogs are desktop-only — Android/iOS have no equivalent
// from a pure-Rust crate, and tfd 0.1.0 does not cross-compile for them.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod dialogs;
pub mod extra;
pub mod file;
