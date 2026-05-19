//! Desktop-specific APIs (dialogs, file operations)

// The dialogs module exposes the public type surface unconditionally so
// consumers (`pub use azul_layout::desktop::dialogs::*` in azul-dll) keep
// resolving. On mobile, every method is a no-op that returns the safest
// default — there is no equivalent of `tfd` on Android/iOS.
#[cfg(not(any(target_os = "android", target_os = "ios")))]
pub mod dialogs;

#[cfg(any(target_os = "android", target_os = "ios"))]
#[path = "dialogs_stub.rs"]
pub mod dialogs;

pub mod extra;
pub mod file;
