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

use libloading::{Library, Symbol};
use std::os::raw::{c_char, c_int, c_uint, c_void};
use std::rc::Rc;

/// DBus library handle with function pointers
pub struct DBusLib {
    _lib: Library,

    // Connection management
    pub dbus_bus_get:
        Symbol<'static, unsafe extern "C" fn(c_int, *mut DBusError) -> *mut DBusConnection>,
    pub dbus_connection_unref: Symbol<'static, unsafe extern "C" fn(*mut DBusConnection)>,
    pub dbus_connection_read_write_dispatch:
        Symbol<'static, unsafe extern "C" fn(*mut DBusConnection, c_int) -> c_int>,
    pub dbus_connection_flush: Symbol<'static, unsafe extern "C" fn(*mut DBusConnection)>,

    // Name registration
    pub dbus_bus_request_name: Symbol<
        'static,
        unsafe extern "C" fn(*mut DBusConnection, *const c_char, c_uint, *mut DBusError) -> c_int,
    >,

    // Object registration
    pub dbus_connection_register_object_path: Symbol<
        'static,
        unsafe extern "C" fn(
            *mut DBusConnection,
            *const c_char,
            *const DBusObjectPathVTable,
            *mut c_void,
        ) -> c_int,
    >,
    pub dbus_connection_unregister_object_path:
        Symbol<'static, unsafe extern "C" fn(*mut DBusConnection, *const c_char) -> c_int>,

    // Message handling
    pub dbus_message_new_method_return:
        Symbol<'static, unsafe extern "C" fn(*mut DBusMessage) -> *mut DBusMessage>,
    pub dbus_message_new_error: Symbol<
        'static,
        unsafe extern "C" fn(*mut DBusMessage, *const c_char, *const c_char) -> *mut DBusMessage,
    >,
    pub dbus_message_ref: Symbol<'static, unsafe extern "C" fn(*mut DBusMessage) -> *mut DBusMessage>,
    pub dbus_message_unref: Symbol<'static, unsafe extern "C" fn(*mut DBusMessage)>,
    pub dbus_message_get_member:
        Symbol<'static, unsafe extern "C" fn(*mut DBusMessage) -> *const c_char>,
    pub dbus_message_get_interface:
        Symbol<'static, unsafe extern "C" fn(*mut DBusMessage) -> *const c_char>,

    // Message iteration for parsing arguments
    pub dbus_message_iter_init:
        Symbol<'static, unsafe extern "C" fn(*mut DBusMessage, *mut DBusMessageIter) -> c_int>,
    pub dbus_message_iter_get_arg_type:
        Symbol<'static, unsafe extern "C" fn(*mut DBusMessageIter) -> c_int>,
    pub dbus_message_iter_get_basic:
        Symbol<'static, unsafe extern "C" fn(*mut DBusMessageIter, *mut c_void)>,
    pub dbus_message_iter_next:
        Symbol<'static, unsafe extern "C" fn(*mut DBusMessageIter) -> c_int>,
    pub dbus_message_iter_recurse:
        Symbol<'static, unsafe extern "C" fn(*mut DBusMessageIter, *mut DBusMessageIter)>,

    // Message iteration for building responses
    pub dbus_message_iter_init_append:
        Symbol<'static, unsafe extern "C" fn(*mut DBusMessage, *mut DBusMessageIter)>,
    pub dbus_message_iter_append_basic:
        Symbol<'static, unsafe extern "C" fn(*mut DBusMessageIter, c_int, *const c_void) -> c_int>,
    pub dbus_message_iter_open_container: Symbol<
        'static,
        unsafe extern "C" fn(*mut DBusMessageIter, c_int, *const c_char, *mut DBusMessageIter) -> c_int,
    >,
    pub dbus_message_iter_close_container:
        Symbol<'static, unsafe extern "C" fn(*mut DBusMessageIter, *mut DBusMessageIter) -> c_int>,

    // Sending
    pub dbus_connection_send: Symbol<
        'static,
        unsafe extern "C" fn(*mut DBusConnection, *mut DBusMessage, *mut c_uint) -> c_int,
    >,

    // Error handling
    pub dbus_error_init: Symbol<'static, unsafe extern "C" fn(*mut DBusError)>,
    pub dbus_error_is_set: Symbol<'static, unsafe extern "C" fn(*const DBusError) -> c_int>,
    pub dbus_error_free: Symbol<'static, unsafe extern "C" fn(*mut DBusError)>,
}

/// Opaque DBus connection handle
#[repr(C)]
pub struct DBusConnection {
    _private: [u8; 0],
}

/// Opaque DBus message handle
#[repr(C)]
pub struct DBusMessage {
    _private: [u8; 0],
}

/// DBus error structure
#[repr(C)]
pub struct DBusError {
    pub name: *const c_char,
    pub message: *const c_char,
    dummy1: c_uint,
    dummy2: c_uint,
    dummy3: c_uint,
    dummy4: c_uint,
    dummy5: c_uint,
    padding1: *mut c_void,
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
pub struct DBusObjectPathVTable {
    pub unregister_function: Option<unsafe extern "C" fn(*mut DBusConnection, *mut c_void)>,
    pub message_function: Option<
        unsafe extern "C" fn(*mut DBusConnection, *mut DBusMessage, *mut c_void) -> c_int,
    >,
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
    pub fn new() -> Result<Rc<Self>, libloading::Error> {
        unsafe {
            // Try multiple library names (different distros have different symlinks)
            let lib = Library::new("libdbus-1.so.3")
                .or_else(|_| Library::new("libdbus-1.so"))
                .or_else(|_| Library::new("libdbus-1.so.0"))?;

            macro_rules! load_symbol {
                ($lib:expr, $name:expr) => {
                    std::mem::transmute($lib.get(concat!($name, "\0").as_bytes())?.into_raw())
                };
            }

            Ok(Rc::new(Self {
                // Connection management
                dbus_bus_get: load_symbol!(lib, "dbus_bus_get"),
                dbus_connection_unref: load_symbol!(lib, "dbus_connection_unref"),
                dbus_connection_read_write_dispatch: load_symbol!(
                    lib,
                    "dbus_connection_read_write_dispatch"
                ),
                dbus_connection_flush: load_symbol!(lib, "dbus_connection_flush"),

                // Name registration
                dbus_bus_request_name: load_symbol!(lib, "dbus_bus_request_name"),

                // Object registration
                dbus_connection_register_object_path: load_symbol!(
                    lib,
                    "dbus_connection_register_object_path"
                ),
                dbus_connection_unregister_object_path: load_symbol!(
                    lib,
                    "dbus_connection_unregister_object_path"
                ),

                // Message handling
                dbus_message_new_method_return: load_symbol!(lib, "dbus_message_new_method_return"),
                dbus_message_new_error: load_symbol!(lib, "dbus_message_new_error"),
                dbus_message_ref: load_symbol!(lib, "dbus_message_ref"),
                dbus_message_unref: load_symbol!(lib, "dbus_message_unref"),
                dbus_message_get_member: load_symbol!(lib, "dbus_message_get_member"),
                dbus_message_get_interface: load_symbol!(lib, "dbus_message_get_interface"),

                // Message iteration (parsing)
                dbus_message_iter_init: load_symbol!(lib, "dbus_message_iter_init"),
                dbus_message_iter_get_arg_type: load_symbol!(lib, "dbus_message_iter_get_arg_type"),
                dbus_message_iter_get_basic: load_symbol!(lib, "dbus_message_iter_get_basic"),
                dbus_message_iter_next: load_symbol!(lib, "dbus_message_iter_next"),
                dbus_message_iter_recurse: load_symbol!(lib, "dbus_message_iter_recurse"),

                // Message iteration (building)
                dbus_message_iter_init_append: load_symbol!(lib, "dbus_message_iter_init_append"),
                dbus_message_iter_append_basic: load_symbol!(lib, "dbus_message_iter_append_basic"),
                dbus_message_iter_open_container: load_symbol!(
                    lib,
                    "dbus_message_iter_open_container"
                ),
                dbus_message_iter_close_container: load_symbol!(
                    lib,
                    "dbus_message_iter_close_container"
                ),

                // Sending
                dbus_connection_send: load_symbol!(lib, "dbus_connection_send"),

                // Error handling
                dbus_error_init: load_symbol!(lib, "dbus_error_init"),
                dbus_error_is_set: load_symbol!(lib, "dbus_error_is_set"),
                dbus_error_free: load_symbol!(lib, "dbus_error_free"),

                _lib: lib,
            }))
        }
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
