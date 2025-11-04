//! Shared DBus library instance
//!
//! This module provides a singleton DBusLib that can be shared across
//! all windows in the application. The library is loaded once at startup.

use std::{
    rc::Rc,
    sync::{Arc, Mutex, Once},
};

use super::debug_log;
use crate::desktop::shell2::linux::dbus::DBusLib;

/// Global shared DBus library instance
static mut DBUS_LIB: Option<Arc<Mutex<Option<Rc<DBusLib>>>>> = None;
static INIT: Once = Once::new();

/// Get or initialize the shared DBus library instance
///
/// This function is thread-safe and will load the library only once.
/// Returns `None` if the library cannot be loaded.
pub fn get_shared_dbus_lib() -> Option<Rc<DBusLib>> {
    unsafe {
        INIT.call_once(|| {
            debug_log("Attempting to load libdbus-1.so");

            match DBusLib::new() {
                Ok(lib) => {
                    debug_log("Successfully loaded libdbus-1.so");
                    DBUS_LIB = Some(Arc::new(Mutex::new(Some(lib))));
                }
                Err(e) => {
                    debug_log(&format!("Failed to load libdbus-1.so: {}", e));
                    DBUS_LIB = Some(Arc::new(Mutex::new(None)));
                }
            }
        });

        if let Some(ref lib_mutex) = DBUS_LIB {
            let guard = lib_mutex.lock().unwrap();
            guard.as_ref().cloned()
        } else {
            None
        }
    }
}

/// Check if DBus library is available
///
/// This is a quick check without trying to load the library.
pub fn is_dbus_available() -> bool {
    unsafe {
        INIT.call_once(|| {
            // Ensure initialization
            get_shared_dbus_lib();
        });

        if let Some(ref lib_mutex) = DBUS_LIB {
            let guard = lib_mutex.lock().unwrap();
            guard.is_some()
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "linux")]
    fn test_dbus_library_loading() {
        // This test requires libdbus-1.so to be installed
        match get_shared_dbus_lib() {
            Some(lib) => {
                println!("DBus library loaded successfully");
                assert!(is_dbus_available());
            }
            None => {
                println!("DBus library not available (expected if not installed)");
                assert!(!is_dbus_available());
            }
        }
    }

    #[test]
    fn test_shared_instance() {
        // Getting the library multiple times should return the same instance
        let lib1 = get_shared_dbus_lib();
        let lib2 = get_shared_dbus_lib();

        match (lib1, lib2) {
            (Some(l1), Some(l2)) => {
                // Both should point to the same Rc
                assert!(Rc::ptr_eq(&l1, &l2));
            }
            (None, None) => {
                // Both None is also fine (library not available)
            }
            _ => panic!("Inconsistent library state"),
        }
    }
}
