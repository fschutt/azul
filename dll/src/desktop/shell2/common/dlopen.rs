//! Dynamic library loading abstraction.
//!
//! Provides the [`DynamicLibrary`] trait for platform backends (X11, Wayland,
//! dbus, Win32) and the [`load_first_available`] helper for trying multiple
//! library names in order (e.g. `libX11.so.6` then `libX11.so`).

use super::error::DlError;

#[macro_export]
macro_rules! load_symbol {
    ($lib:expr, $t:ty, $s:expr) => {
        match unsafe { $lib.get_symbol::<$t>($s) } {
            Ok(f) => f,
            Err(e) => return Err(e),
        }
    };
}

/// Dynamic library loading trait.
pub trait DynamicLibrary {
    /// Load a system library by name.
    ///
    /// # Arguments
    /// * `name` - Library name (e.g., "libX11.so.6" or "user32.dll")
    fn load(name: &str) -> Result<Self, DlError>
    where
        Self: Sized;

    /// Get function pointer by symbol name.
    ///
    /// # Safety
    /// The returned function pointer must be called with correct arguments.
    unsafe fn get_symbol<T>(&self, name: &str) -> Result<T, DlError>;

    /// Unload library (called automatically on Drop).
    fn unload(&mut self);
}

/// Helper to try loading a library with multiple names.
///
/// Useful for different library versions across distributions.
/// Example: ["libX11.so.6", "libX11.so"]
#[cfg(any(target_os = "linux", target_os = "windows"))]
pub fn load_first_available<L: DynamicLibrary>(names: &[&str]) -> Result<L, DlError> {
    let mut errors = Vec::new();

    for name in names {
        match L::load(name) {
            Ok(lib) => return Ok(lib),
            Err(e) => errors.push(format!("{}: {}", name, e)),
        }
    }

    Err(DlError::LibraryNotFound {
        name: names.first().unwrap_or(&"<unknown>").to_string(),
        tried: names.iter().map(|s| s.to_string()).collect(),
        suggestion: format!("Install the required system libraries. Tried: {:?}", names),
    })
}

