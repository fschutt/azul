#![allow(non_camel_case_types, clippy::missing_safety_doc)]
//! dlopen-based drop-in fork of `libudev-sys` 0.1.4.
//!
//! API-identical to upstream `libudev-sys` (same opaque structs + the same
//! `udev_*` functions), but instead of *link-binding* `libudev` at build time
//! (upstream's `build.rs` runs `pkg_config::find_library("libudev").unwrap()`,
//! which fails when cross-compiling to a host that has no libudev), every
//! function loads `libudev.so` lazily at *runtime* via `libloading` and
//! dispatches through a function pointer.
//!
//! Consequences:
//! - The crate has **no build-time native dependency**, so anything depending
//!   on it (gilrs-core -> gilrs) cross-compiles to any target with no libudev
//!   present (e.g. `cargo build --target x86_64-unknown-linux-gnu` from macOS).
//! - If `libudev.so` is not loadable at runtime, every function returns a
//!   zeroed value (null pointer / 0), so the caller (gilrs) simply observes "no
//!   udev / no devices" and gracefully reports no gamepads - rather than the
//!   process failing to start.
//!
//! Used via `[patch.crates-io] libudev-sys = { path = "forks/libudev-sys" }`
//! in the workspace; gilrs itself is unmodified.

use libc::{c_char, c_int, c_ulonglong, c_void, dev_t, size_t};

#[repr(C)]
pub struct udev {
    __private: c_void,
}
#[repr(C)]
pub struct udev_list_entry {
    __private: c_void,
}
#[repr(C)]
pub struct udev_device {
    __private: c_void,
}
#[repr(C)]
pub struct udev_monitor {
    __private: c_void,
}
#[repr(C)]
pub struct udev_enumerate {
    __private: c_void,
}
#[repr(C)]
pub struct udev_queue {
    __private: c_void,
}

/// Generate, for each declared function:
/// - a field in `LibudevFns` holding the dlsym'd function pointer,
/// - a public `extern "C"` wrapper that dispatches to it (or returns zeroed if
///   libudev isn't loadable).
macro_rules! libudev_dlopen {
    ($( fn $name:ident( $($arg:ident : $aty:ty),* $(,)? ) $(-> $ret:ty)? ; )*) => {
        struct LibudevFns {
            $( $name: unsafe extern "C" fn($($aty),*) $(-> $ret)?, )*
        }

        unsafe fn load() -> Option<(libloading::Library, LibudevFns)> {
            let lib = libloading::Library::new("libudev.so.1")
                .or_else(|_| libloading::Library::new("libudev.so.0"))
                .or_else(|_| libloading::Library::new("libudev.so"))
                .ok()?;
            let fns = LibudevFns {
                $( $name: {
                    let sym: libloading::Symbol<unsafe extern "C" fn($($aty),*) $(-> $ret)?> =
                        lib.get(concat!(stringify!($name), "\0").as_bytes()).ok()?;
                    *sym
                }, )*
            };
            Some((lib, fns))
        }

        // Loaded once; the `Library` is kept alive in the tuple so the function
        // pointers stay valid for the process lifetime.
        static LIBUDEV: std::sync::OnceLock<Option<(libloading::Library, LibudevFns)>> =
            std::sync::OnceLock::new();

        #[inline]
        fn fns() -> Option<&'static LibudevFns> {
            LIBUDEV.get_or_init(|| unsafe { load() }).as_ref().map(|(_, f)| f)
        }

        $(
            pub unsafe extern "C" fn $name( $($arg : $aty),* ) $(-> $ret)? {
                match fns() {
                    Some(f) => (f.$name)($($arg),*),
                    // libudev not loadable: null / 0 (POD return types only).
                    None => ::core::mem::zeroed(),
                }
            }
        )*
    };
}

libudev_dlopen! {
    // udev
    fn udev_new() -> *mut udev;
    fn udev_ref(udev: *mut udev) -> *mut udev;
    fn udev_unref(udev: *mut udev) -> *mut udev;
    fn udev_set_userdata(udev: *mut udev, userdata: *mut c_void);
    fn udev_get_userdata(udev: *mut udev) -> *mut c_void;

    // udev_list
    fn udev_list_entry_get_next(list_entry: *mut udev_list_entry) -> *mut udev_list_entry;
    fn udev_list_entry_get_by_name(list_entry: *mut udev_list_entry, name: *const c_char) -> *mut udev_list_entry;
    fn udev_list_entry_get_name(list_entry: *mut udev_list_entry) -> *const c_char;
    fn udev_list_entry_get_value(list_entry: *mut udev_list_entry) -> *const c_char;

    // udev_device
    fn udev_device_ref(udev_device: *mut udev_device) -> *mut udev_device;
    fn udev_device_unref(udev_device: *mut udev_device) -> *mut udev_device;
    fn udev_device_get_udev(udev_device: *mut udev_device) -> *mut udev;
    fn udev_device_new_from_syspath(udev: *mut udev, syspath: *const c_char) -> *mut udev_device;
    fn udev_device_new_from_devnum(udev: *mut udev, dev_type: c_char, devnum: dev_t) -> *mut udev_device;
    fn udev_device_new_from_subsystem_sysname(udev: *mut udev, subsystem: *const c_char, sysname: *const c_char) -> *mut udev_device;
    fn udev_device_new_from_device_id(udev: *mut udev, id: *const c_char) -> *mut udev_device;
    fn udev_device_new_from_environment(udev: *mut udev) -> *mut udev_device;
    fn udev_device_get_parent(udev_device: *mut udev_device) -> *mut udev_device;
    fn udev_device_get_parent_with_subsystem_devtype(udev_device: *mut udev_device, subsystem: *const c_char, devtype: *const c_char) -> *mut udev_device;
    fn udev_device_get_devpath(udev_device: *mut udev_device) -> *const c_char;
    fn udev_device_get_subsystem(udev_device: *mut udev_device) -> *const c_char;
    fn udev_device_get_devtype(udev_device: *mut udev_device) -> *const c_char;
    fn udev_device_get_syspath(udev_device: *mut udev_device) -> *const c_char;
    fn udev_device_get_sysname(udev_device: *mut udev_device) -> *const c_char;
    fn udev_device_get_sysnum(udev_device: *mut udev_device) -> *const c_char;
    fn udev_device_get_devnode(udev_device: *mut udev_device) -> *const c_char;
    fn udev_device_get_is_initialized(udev_device: *mut udev_device) -> c_int;
    fn udev_device_get_devlinks_list_entry(udev_device: *mut udev_device) -> *mut udev_list_entry;
    fn udev_device_get_properties_list_entry(udev_device: *mut udev_device) -> *mut udev_list_entry;
    fn udev_device_get_tags_list_entry(udev_device: *mut udev_device) -> *mut udev_list_entry;
    fn udev_device_get_property_value(udev_device: *mut udev_device, key: *const c_char) -> *const c_char;
    fn udev_device_get_driver(udev_device: *mut udev_device) -> *const c_char;
    fn udev_device_get_devnum(udev_device: *mut udev_device) -> dev_t;
    fn udev_device_get_action(udev_device: *mut udev_device) -> *const c_char;
    fn udev_device_get_sysattr_value(udev_device: *mut udev_device, sysattr: *const c_char) -> *const c_char;
    fn udev_device_set_sysattr_value(udev_device: *mut udev_device, sysattr: *const c_char, value: *mut c_char) -> c_int;
    fn udev_device_get_sysattr_list_entry(udev_device: *mut udev_device) -> *mut udev_list_entry;
    fn udev_device_get_seqnum(udev_device: *mut udev_device) -> c_ulonglong;
    fn udev_device_get_usec_since_initialized(udev_device: *mut udev_device) -> c_ulonglong;
    fn udev_device_has_tag(udev_device: *mut udev_device, tag: *const c_char) -> c_int;

    // udev_monitor
    fn udev_monitor_ref(udev_monitor: *mut udev_monitor) -> *mut udev_monitor;
    fn udev_monitor_unref(udev_monitor: *mut udev_monitor) -> *mut udev_monitor;
    fn udev_monitor_get_udev(udev_monitor: *mut udev_monitor) -> *mut udev;
    fn udev_monitor_new_from_netlink(udev: *mut udev, name: *const c_char) -> *mut udev_monitor;
    fn udev_monitor_enable_receiving(udev_monitor: *mut udev_monitor) -> c_int;
    fn udev_monitor_set_receive_buffer_size(udev_monitor: *mut udev_monitor, size: c_int) -> c_int;
    fn udev_monitor_get_fd(udev_monitor: *mut udev_monitor) -> c_int;
    fn udev_monitor_receive_device(udev_monitor: *mut udev_monitor) -> *mut udev_device;
    fn udev_monitor_filter_add_match_subsystem_devtype(udev_monitor: *mut udev_monitor, subsystem: *const c_char, devtype: *const c_char) -> c_int;
    fn udev_monitor_filter_add_match_tag(udev_monitor: *mut udev_monitor, tag: *const c_char) -> c_int;
    fn udev_monitor_filter_update(udev_monitor: *mut udev_monitor) -> c_int;
    fn udev_monitor_filter_remove(udev_monitor: *mut udev_monitor) -> c_int;

    // udev_enumerate
    fn udev_enumerate_ref(udev_enumerate: *mut udev_enumerate) -> *mut udev_enumerate;
    fn udev_enumerate_unref(udev_enumerate: *mut udev_enumerate) -> *mut udev_enumerate;
    fn udev_enumerate_get_udev(udev_enumerate: *mut udev_enumerate) -> *mut udev;
    fn udev_enumerate_new(udev: *mut udev) -> *mut udev_enumerate;
    fn udev_enumerate_add_match_subsystem(udev_enumerate: *mut udev_enumerate, subsystem: *const c_char) -> c_int;
    fn udev_enumerate_add_nomatch_subsystem(udev_enumerate: *mut udev_enumerate, subsystem: *const c_char) -> c_int;
    fn udev_enumerate_add_match_sysattr(udev_enumerate: *mut udev_enumerate, sysattr: *const c_char, value: *const c_char) -> c_int;
    fn udev_enumerate_add_nomatch_sysattr(udev_enumerate: *mut udev_enumerate, sysattr: *const c_char, value: *const c_char) -> c_int;
    fn udev_enumerate_add_match_property(udev_enumerate: *mut udev_enumerate, property: *const c_char, value: *const c_char) -> c_int;
    fn udev_enumerate_add_match_tag(udev_enumerate: *mut udev_enumerate, tag: *const c_char) -> c_int;
    fn udev_enumerate_add_match_parent(udev_enumerate: *mut udev_enumerate, parent: *mut udev_device) -> c_int;
    fn udev_enumerate_add_match_is_initialized(udev_enumerate: *mut udev_enumerate) -> c_int;
    fn udev_enumerate_add_match_sysname(udev_enumerate: *mut udev_enumerate, sysname: *const c_char) -> c_int;
    fn udev_enumerate_add_syspath(udev_enumerate: *mut udev_enumerate, syspath: *const c_char) -> c_int;
    fn udev_enumerate_scan_devices(udev_enumerate: *mut udev_enumerate) -> c_int;
    fn udev_enumerate_scan_subsystems(udev_enumerate: *mut udev_enumerate) -> c_int;
    fn udev_enumerate_get_list_entry(udev_enumerate: *mut udev_enumerate) -> *mut udev_list_entry;

    // udev_queue
    fn udev_queue_ref(udev_queue: *mut udev_queue) -> *mut udev_queue;
    fn udev_queue_unref(udev_queue: *mut udev_queue) -> *mut udev_queue;
    fn udev_queue_get_udev(udev_queue: *mut udev_queue) -> *mut udev;
    fn udev_queue_new(udev: *mut udev) -> *mut udev_queue;
    fn udev_queue_get_udev_is_active(udev_queue: *mut udev_queue) -> c_int;
    fn udev_queue_get_queue_is_empty(udev_queue: *mut udev_queue) -> c_int;
    fn udev_queue_get_fd(udev_queue: *mut udev_queue) -> c_int;
    fn udev_queue_flush(udev_queue: *mut udev_queue) -> c_int;

    // udev_util
    fn udev_util_encode_string(s: *const c_char, str_enc: *mut c_char, len: size_t) -> c_int;
}
