# DBus dlopen Design for GNOME Menus

## Problem

The current GNOME menu implementation uses the `dbus` Rust crate, which:
- Links directly to `libdbus-1.so` at compile time
- Requires `libdbus-dev` package installed
- Breaks cross-compilation to Linux from other platforms
- Is behind a feature flag but still causes build issues

## Solution

Use **dlopen** (runtime dynamic loading) instead of compile-time linking:
- Load `libdbus-1.so.3` at runtime via `libloading`
- No compile-time dependency on libdbus
- Cross-compilation works without libdbus-dev
- Feature flag still controls code inclusion
- Graceful fallback if libdbus not available at runtime

## Implementation Strategy

### Phase 1: DBus Function Research

Identify minimal set of libdbus-1 C API functions needed for GNOME menus:

**Connection Management:**
```c
DBusConnection* dbus_bus_get(DBusBusType type, DBusError *error);
void dbus_connection_unref(DBusConnection *connection);
dbus_bool_t dbus_connection_read_write_dispatch(DBusConnection *connection, int timeout_milliseconds);
```

**Name Registration:**
```c
int dbus_bus_request_name(DBusConnection *connection, const char *name, unsigned int flags, DBusError *error);
#define DBUS_NAME_FLAG_DO_NOT_QUEUE 0x04
```

**Object Registration (using low-level API):**
```c
dbus_bool_t dbus_connection_register_object_path(
    DBusConnection *connection,
    const char *path,
    const DBusObjectPathVTable *vtable,
    void *user_data
);
```

**Message Handling:**
```c
DBusMessage* dbus_message_new_method_return(DBusMessage *method_call);
void dbus_message_unref(DBusMessage *message);
dbus_bool_t dbus_message_append_args(DBusMessage *message, int first_arg_type, ...);
dbus_bool_t dbus_connection_send(DBusConnection *connection, DBusMessage *message, dbus_uint32_t *serial);
```

**Error Handling:**
```c
void dbus_error_init(DBusError *error);
dbus_bool_t dbus_error_is_set(const DBusError *error);
void dbus_error_free(DBusError *error);
```

### Phase 2: Create dbus/dlopen.rs

Similar to existing `x11/dlopen.rs`, create:

```rust
// dll/src/desktop/shell2/linux/dbus/dlopen.rs

use libloading::{Library, Symbol};
use std::os::raw::{c_char, c_int, c_uint, c_void};
use std::rc::Rc;

pub struct DBusLib {
    _lib: Library,
    
    // Connection management
    pub dbus_bus_get: Symbol<'static, unsafe extern "C" fn(c_int, *mut DBusError) -> *mut DBusConnection>,
    pub dbus_connection_unref: Symbol<'static, unsafe extern "C" fn(*mut DBusConnection)>,
    pub dbus_connection_read_write_dispatch: Symbol<'static, unsafe extern "C" fn(*mut DBusConnection, c_int) -> c_int>,
    
    // Name registration
    pub dbus_bus_request_name: Symbol<'static, unsafe extern "C" fn(*mut DBusConnection, *const c_char, c_uint, *mut DBusError) -> c_int>,
    
    // Object registration
    pub dbus_connection_register_object_path: Symbol<'static, unsafe extern "C" fn(*mut DBusConnection, *const c_char, *const DBusObjectPathVTable, *mut c_void) -> c_int>,
    
    // Message handling
    pub dbus_message_new_method_return: Symbol<'static, unsafe extern "C" fn(*mut DBusMessage) -> *mut DBusMessage>,
    pub dbus_message_unref: Symbol<'static, unsafe extern "C" fn(*mut DBusMessage)>,
    pub dbus_message_append_args: Symbol<'static, unsafe extern "C" fn(*mut DBusMessage, c_int, ...) -> c_int>,
    pub dbus_connection_send: Symbol<'static, unsafe extern "C" fn(*mut DBusConnection, *mut DBusMessage, *mut c_uint) -> c_int>,
    
    // Error handling
    pub dbus_error_init: Symbol<'static, unsafe extern "C" fn(*mut DBusError)>,
    pub dbus_error_is_set: Symbol<'static, unsafe extern "C" fn(*const DBusError) -> c_int>,
    pub dbus_error_free: Symbol<'static, unsafe extern "C" fn(*mut DBusError)>,
}

#[repr(C)]
pub struct DBusConnection {
    _private: [u8; 0],
}

#[repr(C)]
pub struct DBusMessage {
    _private: [u8; 0],
}

#[repr(C)]
pub struct DBusError {
    pub name: *const c_char,
    pub message: *const c_char,
    dummy: c_int,
    padding: [*mut c_void; 8],
}

#[repr(C)]
pub struct DBusObjectPathVTable {
    pub unregister_function: Option<extern "C" fn(*mut DBusConnection, *mut c_void)>,
    pub message_function: Option<extern "C" fn(*mut DBusConnection, *mut DBusMessage, *mut c_void) -> c_int>,
}

pub const DBUS_BUS_SESSION: c_int = 0;
pub const DBUS_NAME_FLAG_DO_NOT_QUEUE: c_uint = 0x04;

impl DBusLib {
    pub fn new() -> Result<Rc<Self>, libloading::Error> {
        unsafe {
            let lib = Library::new("libdbus-1.so.3")?;
            
            Ok(Rc::new(Self {
                dbus_bus_get: std::mem::transmute(lib.get(b"dbus_bus_get\0")?.into_raw()),
                dbus_connection_unref: std::mem::transmute(lib.get(b"dbus_connection_unref\0")?.into_raw()),
                dbus_connection_read_write_dispatch: std::mem::transmute(lib.get(b"dbus_connection_read_write_dispatch\0")?.into_raw()),
                dbus_bus_request_name: std::mem::transmute(lib.get(b"dbus_bus_request_name\0")?.into_raw()),
                dbus_connection_register_object_path: std::mem::transmute(lib.get(b"dbus_connection_register_object_path\0")?.into_raw()),
                dbus_message_new_method_return: std::mem::transmute(lib.get(b"dbus_message_new_method_return\0")?.into_raw()),
                dbus_message_unref: std::mem::transmute(lib.get(b"dbus_message_unref\0")?.into_raw()),
                dbus_message_append_args: std::mem::transmute(lib.get(b"dbus_message_append_args\0")?.into_raw()),
                dbus_connection_send: std::mem::transmute(lib.get(b"dbus_connection_send\0")?.into_raw()),
                dbus_error_init: std::mem::transmute(lib.get(b"dbus_error_init\0")?.into_raw()),
                dbus_error_is_set: std::mem::transmute(lib.get(b"dbus_error_is_set\0")?.into_raw()),
                dbus_error_free: std::mem::transmute(lib.get(b"dbus_error_free\0")?.into_raw()),
                _lib: lib,
            }))
        }
    }
}
```

### Phase 3: Refactor gnome_menu modules

**dbus_connection.rs:**
- Replace `dbus::blocking::Connection` with `*mut DBusConnection`
- Use dlopen DBusLib instead of dbus crate
- Manual name registration with `dbus_bus_request_name`

**menu_protocol.rs and actions_protocol.rs:**
- Remove `dbus::tree` usage
- Use low-level `dbus_connection_register_object_path`
- Manual message parsing and response construction
- Implement `DBusObjectPathVTable` callbacks

### Phase 4: Remove dbus crate dependency

```toml
# dll/Cargo.toml

# Remove:
# dbus = { version = "0.9", optional = true }

# Keep:
gnome-menus = []  # Feature flag remains, but no dependencies
```

### Benefits

1. **Cross-compilation:** Works from any platform to Linux
2. **No build deps:** Doesn't require libdbus-dev at build time
3. **Runtime graceful:** Falls back if libdbus-1.so.3 not present
4. **Consistent:** Same pattern as Xlib, Wayland, other platform APIs
5. **Smaller binary:** No static linking to dbus

### Drawbacks

1. **More code:** Manual FFI instead of safe Rust wrapper
2. **More unsafe:** Direct C API usage
3. **Testing:** Requires actual DBus session bus (can't mock easily)

### Alternative Considered

**zbus crate:** Pure Rust DBus implementation
- Pro: No C dependencies, safe Rust
- Con: Larger binary size, more complex
- Con: Still needs feature flag for optional inclusion
- Decision: dlopen is simpler and consistent with existing patterns

## Implementation Order

1. ✅ Create this design document
2. ⬜ Create `dbus/dlopen.rs` with minimal function set
3. ⬜ Test dlopen loading on Linux system
4. ⬜ Refactor `dbus_connection.rs` to use dlopen
5. ⬜ Refactor `menu_protocol.rs` to use low-level API
6. ⬜ Refactor `actions_protocol.rs` to use low-level API
7. ⬜ Remove dbus crate from Cargo.toml
8. ⬜ Test GNOME menus still work
9. ⬜ Test cross-compilation from macOS/Windows
10. ⬜ Update documentation

## Testing Strategy

**Unit tests:**
- dlopen loading (mock library path)
- Function pointer validation
- Error handling

**Integration tests:**
- Actual DBus connection on Linux with DBus session
- Menu registration and method calls
- Graceful fallback when DBus unavailable

**Cross-compilation tests:**
- Build from macOS to x86_64-unknown-linux-gnu
- Build from Windows to x86_64-unknown-linux-gnu
- Verify no libdbus link errors
