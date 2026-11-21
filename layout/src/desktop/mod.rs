//! Desktop-specific APIs (native dialogs). File operations live in
//! `crate::file` (`FilePath`), which is the one file API on every target.

pub mod dialogs;
/// The file picker a `FileDialog` request prefers (a registered provider,
/// AzDrive's, else the system's dialog).
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod file_picker;
