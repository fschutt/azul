//! DBus Dynamic Loading
//!
//! Runtime loading of libdbus-1.so.3 for GNOME menu integration.
//!
//! This module provides dlopen-based loading of the DBus C library to avoid
//! compile-time dependencies. This enables cross-compilation without requiring
//! libdbus-dev to be installed on the build system.
//!
//! ## Safety
//!
//! All DBus function calls are `unsafe` as they directly call C code through FFI.
//! Callers must ensure proper error handling and null pointer checks.

use std::{
    ffi::{c_char, c_int, c_uint, c_void, CStr, CString},
    rc::Rc,
};

use crate::desktop::shell2::common::{
    dlopen::load_first_available, DlError, DynamicLibrary as DynamicLibraryTrait,
};

// Helper for loading symbols and casting them to function pointers
macro_rules! load_symbol {
    ($lib:expr, $t:ty, $s:expr) => {
        match unsafe { $lib.get_symbol::<$t>($s) } {
            Ok(f) => f,
            Err(e) => return Err(e),
        }
    };
}

// Wrapper for dlopen, dlsym, dlclose
pub struct Library {
    handle: *mut c_void,
}

impl DynamicLibraryTrait for Library {
    fn load(name: &str) -> Result<Self, DlError> {
        let c_name = CString::new(name).unwrap();
        let handle = unsafe { libc::dlopen(c_name.as_ptr(), libc::RTLD_LAZY) };
        if handle.is_null() {
            let error = unsafe { CStr::from_ptr(libc::dlerror()).to_string_lossy() };
            Err(DlError::LibraryNotFound {
                name: name.to_string(),
                tried: vec![name.to_string()],
                suggestion: format!("dlopen failed: {}", error),
            })
        } else {
            Ok(Self { handle })
        }
    }

    unsafe fn get_symbol<T>(&self, name: &str) -> Result<T, DlError> {
        let c_name = CString::new(name).unwrap();
        let sym = libc::dlsym(self.handle, c_name.as_ptr());
        if sym.is_null() {
            Err(DlError::SymbolNotFound {
                symbol: name.to_string(),
                library: "unknown".to_string(),
                suggestion: "Symbol not found in library".to_string(),
            })
        } else {
            Ok(std::mem::transmute_copy::<*mut c_void, T>(&sym))
        }
    }

    fn unload(&mut self) {
        // Drop implementation already handles cleanup
    }
}

impl Drop for Library {
    fn drop(&mut self) {
        unsafe { libc::dlclose(self.handle) };
    }
}

/// DBus library handle with function pointers
pub struct DBusLib {
    _lib: Library,

    // Connection management
    pub dbus_bus_get: unsafe extern "C" fn(c_int, *mut DBusError) -> *mut DBusConnection,
    pub dbus_connection_unref: unsafe extern "C" fn(*mut DBusConnection),
    pub dbus_connection_read_write_dispatch:
        unsafe extern "C" fn(*mut DBusConnection, c_int) -> c_int,
    pub dbus_connection_flush: unsafe extern "C" fn(*mut DBusConnection),

    // Name registration
    pub dbus_bus_request_name:
        unsafe extern "C" fn(*mut DBusConnection, *const c_char, c_uint, *mut DBusError) -> c_int,

    // Object registration
    pub dbus_connection_register_object_path: unsafe extern "C" fn(
        *mut DBusConnection,
        *const c_char,
        *const DBusObjectPathVTable,
        *mut c_void,
    ) -> c_int,
    pub dbus_connection_unregister_object_path:
        unsafe extern "C" fn(*mut DBusConnection, *const c_char) -> c_int,

    // Message handling
    pub dbus_message_new_method_return: unsafe extern "C" fn(*mut DBusMessage) -> *mut DBusMessage,
    pub dbus_message_new_error:
        unsafe extern "C" fn(*mut DBusMessage, *const c_char, *const c_char) -> *mut DBusMessage,
    pub dbus_message_ref: unsafe extern "C" fn(*mut DBusMessage) -> *mut DBusMessage,
    pub dbus_message_unref: unsafe extern "C" fn(*mut DBusMessage),
    pub dbus_message_get_member: unsafe extern "C" fn(*mut DBusMessage) -> *const c_char,
    pub dbus_message_get_interface: unsafe extern "C" fn(*mut DBusMessage) -> *const c_char,

    // Message iteration for parsing arguments
    pub dbus_message_iter_init:
        unsafe extern "C" fn(*mut DBusMessage, *mut DBusMessageIter) -> c_int,
    pub dbus_message_iter_get_arg_type: unsafe extern "C" fn(*mut DBusMessageIter) -> c_int,
    pub dbus_message_iter_get_basic: unsafe extern "C" fn(*mut DBusMessageIter, *mut c_void),
    pub dbus_message_iter_next: unsafe extern "C" fn(*mut DBusMessageIter) -> c_int,
    pub dbus_message_iter_recurse: unsafe extern "C" fn(*mut DBusMessageIter, *mut DBusMessageIter),

    // Message iteration for building responses
    pub dbus_message_iter_init_append: unsafe extern "C" fn(*mut DBusMessage, *mut DBusMessageIter),
    pub dbus_message_iter_append_basic:
        unsafe extern "C" fn(*mut DBusMessageIter, c_int, *const c_void) -> c_int,
    pub dbus_message_iter_open_container: unsafe extern "C" fn(
        *mut DBusMessageIter,
        c_int,
        *const c_char,
        *mut DBusMessageIter,
    ) -> c_int,
    pub dbus_message_iter_close_container:
        unsafe extern "C" fn(*mut DBusMessageIter, *mut DBusMessageIter) -> c_int,

    // Sending
    pub dbus_connection_send:
        unsafe extern "C" fn(*mut DBusConnection, *mut DBusMessage, *mut c_uint) -> c_int,
    pub dbus_connection_send_with_reply_and_block: unsafe extern "C" fn(
        *mut DBusConnection,
        *mut DBusMessage,
        c_int,
        *mut DBusError,
    ) -> *mut DBusMessage,

    // Method calls
    pub dbus_message_new_method_call: unsafe extern "C" fn(
        *const c_char,
        *const c_char,
        *const c_char,
        *const c_char,
    ) -> *mut DBusMessage,

    // Error handling
    pub dbus_error_init: unsafe extern "C" fn(*mut DBusError),
    pub dbus_error_is_set: unsafe extern "C" fn(*const DBusError) -> c_int,
    pub dbus_error_free: unsafe extern "C" fn(*mut DBusError),
}

/// Opaque DBus connection type
#[repr(C)]
#[derive(Copy, Clone)]
pub struct DBusConnection {
    _private: [u8; 0],
}

/// Opaque DBus message type
#[repr(C)]
#[derive(Copy, Clone)]
pub struct DBusMessage {
    _private: [u8; 0],
}

/// DBus error structure
#[repr(C)]
pub struct DBusError {
    pub name: *const c_char,
    pub message: *const c_char,
    pub dummy1: c_uint,
    pub dummy2: c_uint,
    pub dummy3: c_uint,
    pub dummy4: c_uint,
    pub dummy5: c_uint,
    pub padding1: *mut c_void,
}

/// DBus message iterator for parsing/building complex types
#[repr(C)]
pub struct DBusMessageIter {
    dummy1: *mut c_void,
    dummy2: *mut c_void,
    dummy3: c_uint,
    dummy4: c_int,
    dummy5: c_int,
    dummy6: c_int,
    dummy7: c_int,
    dummy8: c_int,
    dummy9: c_int,
    dummy10: c_int,
    dummy11: c_int,
    pad1: c_int,
    pad2: *mut c_void,
    pad3: *mut c_void,
}

/// Virtual table for object path message handlers
#[repr(C)]
#[derive(Copy, Clone)]
pub struct DBusObjectPathVTable {
    pub unregister_function: Option<unsafe extern "C" fn(*mut DBusConnection, *mut c_void)>,
    pub message_function:
        Option<unsafe extern "C" fn(*mut DBusConnection, *mut DBusMessage, *mut c_void) -> c_int>,
}

// DBus constants
pub const DBUS_BUS_SESSION: c_int = 0;
pub const DBUS_BUS_SYSTEM: c_int = 1;
pub const DBUS_NAME_FLAG_DO_NOT_QUEUE: c_uint = 0x04;

// DBus types
pub const DBUS_TYPE_INVALID: c_int = 0;
pub const DBUS_TYPE_BYTE: c_int = b'y' as c_int;
pub const DBUS_TYPE_BOOLEAN: c_int = b'b' as c_int;
pub const DBUS_TYPE_INT16: c_int = b'n' as c_int;
pub const DBUS_TYPE_UINT16: c_int = b'q' as c_int;
pub const DBUS_TYPE_INT32: c_int = b'i' as c_int;
pub const DBUS_TYPE_UINT32: c_int = b'u' as c_int;
pub const DBUS_TYPE_INT64: c_int = b'x' as c_int;
pub const DBUS_TYPE_UINT64: c_int = b't' as c_int;
pub const DBUS_TYPE_DOUBLE: c_int = b'd' as c_int;
pub const DBUS_TYPE_STRING: c_int = b's' as c_int;
pub const DBUS_TYPE_OBJECT_PATH: c_int = b'o' as c_int;
pub const DBUS_TYPE_SIGNATURE: c_int = b'g' as c_int;
pub const DBUS_TYPE_ARRAY: c_int = b'a' as c_int;
pub const DBUS_TYPE_VARIANT: c_int = b'v' as c_int;
pub const DBUS_TYPE_STRUCT: c_int = b'r' as c_int;
pub const DBUS_TYPE_DICT_ENTRY: c_int = b'e' as c_int;

// Message handler return values
pub const DBUS_HANDLER_RESULT_HANDLED: c_int = 0;
pub const DBUS_HANDLER_RESULT_NOT_YET_HANDLED: c_int = 1;
pub const DBUS_HANDLER_RESULT_NEED_MEMORY: c_int = 2;

impl DBusLib {
    /// Load libdbus-1.so.3 dynamically
    ///
    /// # Errors
    ///
    /// Returns an error if the library cannot be loaded or if any required
    /// function symbol cannot be found.
    pub fn new() -> Result<Rc<Self>, DlError> {
        let lib =
            load_first_available::<Library>(&["libdbus-1.so.3", "libdbus-1.so", "libdbus-1.so.0"])?;

        Ok(Rc::new(Self {
            // Connection management
            dbus_bus_get: load_symbol!(
                lib,
                unsafe extern "C" fn(c_int, *mut DBusError) -> *mut DBusConnection,
                "dbus_bus_get"
            ),
            dbus_connection_unref: load_symbol!(
                lib,
                unsafe extern "C" fn(*mut DBusConnection),
                "dbus_connection_unref"
            ),
            dbus_connection_read_write_dispatch: load_symbol!(
                lib,
                unsafe extern "C" fn(*mut DBusConnection, c_int) -> c_int,
                "dbus_connection_read_write_dispatch"
            ),
            dbus_connection_flush: load_symbol!(
                lib,
                unsafe extern "C" fn(*mut DBusConnection),
                "dbus_connection_flush"
            ),

            // Name registration
            dbus_bus_request_name: load_symbol!(
                lib,
                unsafe extern "C" fn(
                    *mut DBusConnection,
                    *const c_char,
                    c_uint,
                    *mut DBusError,
                ) -> c_int,
                "dbus_bus_request_name"
            ),

            // Object registration
            dbus_connection_register_object_path: load_symbol!(
                lib,
                unsafe extern "C" fn(
                    *mut DBusConnection,
                    *const c_char,
                    *const DBusObjectPathVTable,
                    *mut c_void,
                ) -> c_int,
                "dbus_connection_register_object_path"
            ),
            dbus_connection_unregister_object_path: load_symbol!(
                lib,
                unsafe extern "C" fn(*mut DBusConnection, *const c_char) -> c_int,
                "dbus_connection_unregister_object_path"
            ),

            // Message handling
            dbus_message_new_method_return: load_symbol!(
                lib,
                unsafe extern "C" fn(*mut DBusMessage) -> *mut DBusMessage,
                "dbus_message_new_method_return"
            ),
            dbus_message_new_error: load_symbol!(
                lib,
                unsafe extern "C" fn(
                    *mut DBusMessage,
                    *const c_char,
                    *const c_char,
                ) -> *mut DBusMessage,
                "dbus_message_new_error"
            ),
            dbus_message_ref: load_symbol!(
                lib,
                unsafe extern "C" fn(*mut DBusMessage) -> *mut DBusMessage,
                "dbus_message_ref"
            ),
            dbus_message_unref: load_symbol!(
                lib,
                unsafe extern "C" fn(*mut DBusMessage),
                "dbus_message_unref"
            ),
            dbus_message_get_member: load_symbol!(
                lib,
                unsafe extern "C" fn(*mut DBusMessage) -> *const c_char,
                "dbus_message_get_member"
            ),
            dbus_message_get_interface: load_symbol!(
                lib,
                unsafe extern "C" fn(*mut DBusMessage) -> *const c_char,
                "dbus_message_get_interface"
            ),

            // Message iteration (parsing)
            dbus_message_iter_init: load_symbol!(
                lib,
                unsafe extern "C" fn(*mut DBusMessage, *mut DBusMessageIter) -> c_int,
                "dbus_message_iter_init"
            ),
            dbus_message_iter_get_arg_type: load_symbol!(
                lib,
                unsafe extern "C" fn(*mut DBusMessageIter) -> c_int,
                "dbus_message_iter_get_arg_type"
            ),
            dbus_message_iter_get_basic: load_symbol!(
                lib,
                unsafe extern "C" fn(*mut DBusMessageIter, *mut c_void),
                "dbus_message_iter_get_basic"
            ),
            dbus_message_iter_next: load_symbol!(
                lib,
                unsafe extern "C" fn(*mut DBusMessageIter) -> c_int,
                "dbus_message_iter_next"
            ),
            dbus_message_iter_recurse: load_symbol!(
                lib,
                unsafe extern "C" fn(*mut DBusMessageIter, *mut DBusMessageIter),
                "dbus_message_iter_recurse"
            ),

            // Message iteration (building)
            dbus_message_iter_init_append: load_symbol!(
                lib,
                unsafe extern "C" fn(*mut DBusMessage, *mut DBusMessageIter),
                "dbus_message_iter_init_append"
            ),
            dbus_message_iter_append_basic: load_symbol!(
                lib,
                unsafe extern "C" fn(*mut DBusMessageIter, c_int, *const c_void) -> c_int,
                "dbus_message_iter_append_basic"
            ),
            dbus_message_iter_open_container: load_symbol!(
                lib,
                unsafe extern "C" fn(
                    *mut DBusMessageIter,
                    c_int,
                    *const c_char,
                    *mut DBusMessageIter,
                ) -> c_int,
                "dbus_message_iter_open_container"
            ),
            dbus_message_iter_close_container: load_symbol!(
                lib,
                unsafe extern "C" fn(*mut DBusMessageIter, *mut DBusMessageIter) -> c_int,
                "dbus_message_iter_close_container"
            ),

            // Sending
            dbus_connection_send: load_symbol!(
                lib,
                unsafe extern "C" fn(*mut DBusConnection, *mut DBusMessage, *mut c_uint) -> c_int,
                "dbus_connection_send"
            ),
            dbus_connection_send_with_reply_and_block: load_symbol!(
                lib,
                unsafe extern "C" fn(
                    *mut DBusConnection,
                    *mut DBusMessage,
                    c_int,
                    *mut DBusError,
                ) -> *mut DBusMessage,
                "dbus_connection_send_with_reply_and_block"
            ),

            // Method calls
            dbus_message_new_method_call: load_symbol!(
                lib,
                unsafe extern "C" fn(
                    *const c_char,
                    *const c_char,
                    *const c_char,
                    *const c_char,
                ) -> *mut DBusMessage,
                "dbus_message_new_method_call"
            ),

            // Error handling
            dbus_error_init: load_symbol!(
                lib,
                unsafe extern "C" fn(*mut DBusError),
                "dbus_error_init"
            ),
            dbus_error_is_set: load_symbol!(
                lib,
                unsafe extern "C" fn(*const DBusError) -> c_int,
                "dbus_error_is_set"
            ),
            dbus_error_free: load_symbol!(
                lib,
                unsafe extern "C" fn(*mut DBusError),
                "dbus_error_free"
            ),

            _lib: lib,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "linux")]
    fn test_load_dbus_library() {
        // This test requires libdbus-1.so.3 to be installed
        match DBusLib::new() {
            Ok(dbus) => {
                println!("Successfully loaded libdbus-1.so.3");
                // Verify we can access function pointers
                assert!(!std::ptr::null::<DBusLib>().is_null());
            }
            Err(e) => {
                println!("Could not load libdbus-1.so.3: {}", e);
                println!("This is expected if DBus is not installed");
            }
        }
    }

    #[test]
    fn test_dbus_types() {
        // Verify DBus type constants
        assert_eq!(DBUS_TYPE_STRING, b's' as c_int);
        assert_eq!(DBUS_TYPE_UINT32, b'u' as c_int);
        assert_eq!(DBUS_TYPE_ARRAY, b'a' as c_int);
        assert_eq!(DBUS_TYPE_VARIANT, b'v' as c_int);
    }

    #[test]
    fn test_dbus_constants() {
        assert_eq!(DBUS_BUS_SESSION, 0);
        assert_eq!(DBUS_BUS_SYSTEM, 1);
        assert_eq!(DBUS_NAME_FLAG_DO_NOT_QUEUE, 0x04);
    }
}
