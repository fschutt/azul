//! Shared DBus library instance
//!
//! This module provides a singleton DBusLib that can be shared across
//! all windows in the application. The library is loaded once at startup.

use std::sync::{Arc, OnceLock};

use super::debug_log;
use crate::desktop::shell2::linux::dbus::DBusLib;

/// Global shared DBus library instance, initialized once
static DBUS_LIB: OnceLock<Option<Arc<DBusLib>>> = OnceLock::new();

/// Get or initialize the shared DBus library instance
///
/// This function is thread-safe and will load the library only once.
/// Returns `None` if the library cannot be loaded.
pub fn get_shared_dbus_lib() -> Option<Arc<DBusLib>> {
    DBUS_LIB
        .get_or_init(|| {
            debug_log("Attempting to load libdbus-1.so");

            match DBusLib::new() {
                Ok(lib) => {
                    debug_log("Successfully loaded libdbus-1.so");
                    Some(lib)
                }
                Err(e) => {
                    debug_log(&format!("Failed to load libdbus-1.so: {}", e));
                    None
                }
            }
        })
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "linux")]
    #[cfg_attr(miri, ignore)] // Miri doesn't support dlopen
    fn test_dbus_library_loading() {
        // This test requires libdbus-1.so to be installed
        match get_shared_dbus_lib() {
            Some(_lib) => {
                println!("DBus library loaded successfully");
            }
            None => {
                println!("DBus library not available (expected if not installed)");
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore)] // Miri doesn't support dlopen
    fn test_shared_instance() {
        let lib1 = get_shared_dbus_lib();
        let lib2 = get_shared_dbus_lib();

        match (lib1, lib2) {
            (Some(l1), Some(l2)) => {
                assert!(Arc::ptr_eq(&l1, &l2));
            }
            (None, None) => {
                // Both None is also fine (library not available)
            }
            _ => panic!("Inconsistent library state"),
        }
    }
}
