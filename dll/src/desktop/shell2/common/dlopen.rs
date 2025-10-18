//! Dynamic library loading abstraction.

use core::ffi::c_void;

use super::error::DlError;

/// Platform-specific dynamic library handle.
#[cfg(target_os = "linux")]
pub type LibraryHandle = *mut c_void;

#[cfg(target_os = "windows")]
pub type LibraryHandle = *mut c_void; // HMODULE

#[cfg(target_os = "macos")]
pub type LibraryHandle = *mut c_void; // Not used - static linking

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
        name: names[0].to_string(),
        tried: names.iter().map(|s| s.to_string()).collect(),
        suggestion: format!("Install the required system libraries. Tried: {:?}", names),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_library_handle_size() {
        // Ensure LibraryHandle is pointer-sized
        assert_eq!(
            core::mem::size_of::<LibraryHandle>(),
            core::mem::size_of::<usize>()
        );
    }
}
