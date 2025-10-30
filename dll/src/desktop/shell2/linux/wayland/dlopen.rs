//! Dynamic loading for Wayland and related libraries.

use std::{
    ffi::{c_void, CStr, CString},
    rc::Rc,
};

// Re-using the Library loader from X11
pub use super::super::x11::dlopen::Library;
use super::defines::*;
use crate::desktop::shell2::common::{
    dlopen::load_first_available, DlError, DynamicLibrary as DynamicLibraryTrait,
};

macro_rules! load_symbol {
    ($lib:expr, $t:ty, $s:expr) => {
        match unsafe { $lib.get_symbol::<$t>($s) } {
            Ok(f) => f,
            Err(e) => return Err(e),
        }
    };
}

// Dynamically loaded Wayland client and EGL functions
pub struct Wayland {
    _lib_client: Library,
    _lib_egl: Library,

    // wayland-client functions
    pub wl_display_connect: unsafe extern "C" fn(name: *const i8) -> *mut wl_display,
    pub wl_display_disconnect: unsafe extern "C" fn(display: *mut wl_display),
    pub wl_display_get_registry: unsafe extern "C" fn(display: *mut wl_display) -> *mut wl_registry,
    pub wl_display_roundtrip: unsafe extern "C" fn(display: *mut wl_display) -> i32,
    pub wl_display_dispatch_queue:
        unsafe extern "C" fn(display: *mut wl_display, queue: *mut wl_event_queue) -> i32,
    pub wl_display_dispatch_queue_pending:
        unsafe extern "C" fn(display: *mut wl_display, queue: *mut wl_event_queue) -> i32,
    pub wl_display_create_event_queue:
        unsafe extern "C" fn(display: *mut wl_display) -> *mut wl_event_queue,
    pub wl_event_queue_destroy: unsafe extern "C" fn(queue: *mut wl_event_queue),
    pub wl_display_flush: unsafe extern "C" fn(display: *mut wl_display) -> i32,

    pub wl_proxy_marshal_constructor:
        unsafe extern "C" fn(*mut wl_proxy, u32, *const wl_interface, ...) -> *mut wl_proxy,
    pub wl_proxy_add_listener:
        unsafe extern "C" fn(*mut wl_proxy, *const c_void, *mut c_void) -> i32,
    pub wl_proxy_destroy: unsafe extern "C" fn(proxy: *mut wl_proxy),
    pub wl_proxy_set_queue: unsafe extern "C" fn(proxy: *mut wl_proxy, queue: *mut wl_event_queue),

    // Protocol interfaces (needed for wl_registry_bind)
    pub wl_compositor_interface: wl_interface,
    pub wl_shm_interface: wl_interface,
    pub wl_seat_interface: wl_interface,
    pub wl_output_interface: wl_interface,
    pub xdg_wm_base_interface: wl_interface,

    // Convenience wrappers for common operations
    pub wl_registry_bind:
        unsafe extern "C" fn(*mut wl_registry, u32, *const wl_interface, u32) -> *mut c_void,
    pub wl_compositor_create_surface: unsafe extern "C" fn(*mut wl_compositor) -> *mut wl_surface,
    pub wl_surface_commit: unsafe extern "C" fn(surface: *mut wl_surface),
    pub wl_surface_attach:
        unsafe extern "C" fn(surface: *mut wl_surface, buffer: *mut wl_buffer, i32, i32),
    pub wl_surface_damage: unsafe extern "C" fn(surface: *mut wl_surface, i32, i32, i32, i32),
    pub wl_surface_frame: unsafe extern "C" fn(surface: *mut wl_surface) -> *mut wl_callback,

    pub wl_callback_add_listener:
        unsafe extern "C" fn(*mut wl_callback, *const wl_callback_listener, *mut c_void) -> i32,

    pub xdg_wm_base_pong: unsafe extern "C" fn(*mut xdg_wm_base, u32),
    pub xdg_wm_base_get_xdg_surface:
        unsafe extern "C" fn(*mut xdg_wm_base, *mut wl_surface) -> *mut xdg_surface,
    pub xdg_surface_get_toplevel: unsafe extern "C" fn(*mut xdg_surface) -> *mut xdg_toplevel,
    pub xdg_surface_ack_configure: unsafe extern "C" fn(*mut xdg_surface, u32),
    pub xdg_toplevel_set_title: unsafe extern "C" fn(*mut xdg_toplevel, *const i8),
    pub xdg_toplevel_set_minimized: unsafe extern "C" fn(*mut xdg_toplevel),
    pub xdg_toplevel_set_maximized: unsafe extern "C" fn(*mut xdg_toplevel),
    pub xdg_toplevel_unset_maximized: unsafe extern "C" fn(*mut xdg_toplevel),
    pub xdg_wm_base_add_listener:
        unsafe extern "C" fn(*mut xdg_wm_base, *const xdg_wm_base_listener, *mut c_void) -> i32,
    pub xdg_surface_add_listener:
        unsafe extern "C" fn(*mut xdg_surface, *const xdg_surface_listener, *mut c_void) -> i32,

    // xdg_popup and xdg_positioner functions
    pub xdg_wm_base_create_positioner:
        unsafe extern "C" fn(*mut xdg_wm_base) -> *mut xdg_positioner,
    pub xdg_positioner_set_size: unsafe extern "C" fn(*mut xdg_positioner, i32, i32),
    pub xdg_positioner_set_anchor_rect:
        unsafe extern "C" fn(*mut xdg_positioner, i32, i32, i32, i32),
    pub xdg_positioner_set_anchor: unsafe extern "C" fn(*mut xdg_positioner, u32),
    pub xdg_positioner_set_gravity: unsafe extern "C" fn(*mut xdg_positioner, u32),
    pub xdg_positioner_set_constraint_adjustment: unsafe extern "C" fn(*mut xdg_positioner, u32),
    pub xdg_positioner_destroy: unsafe extern "C" fn(*mut xdg_positioner),
    pub xdg_surface_get_popup: unsafe extern "C" fn(
        *mut xdg_surface,
        *mut xdg_surface,
        *mut xdg_positioner,
    ) -> *mut xdg_popup,
    pub xdg_popup_add_listener:
        unsafe extern "C" fn(*mut xdg_popup, *const xdg_popup_listener, *mut c_void) -> i32,
    pub xdg_popup_grab: unsafe extern "C" fn(*mut xdg_popup, *mut wl_seat, u32),
    pub xdg_popup_destroy: unsafe extern "C" fn(*mut xdg_popup),

    pub wl_seat_get_pointer: unsafe extern "C" fn(*mut wl_seat) -> *mut wl_pointer,
    pub wl_seat_get_keyboard: unsafe extern "C" fn(*mut wl_seat) -> *mut wl_keyboard,
    pub wl_seat_add_listener:
        unsafe extern "C" fn(*mut wl_seat, *const wl_seat_listener, *mut c_void) -> i32,
    pub wl_pointer_add_listener:
        unsafe extern "C" fn(*mut wl_pointer, *const wl_pointer_listener, *mut c_void) -> i32,
    pub wl_keyboard_add_listener:
        unsafe extern "C" fn(*mut wl_keyboard, *const wl_keyboard_listener, *mut c_void) -> i32,

    pub wl_shm_create_pool: unsafe extern "C" fn(*mut wl_shm, i32, i32) -> *mut wl_shm_pool,
    pub wl_shm_pool_create_buffer:
        unsafe extern "C" fn(*mut wl_shm_pool, i32, i32, i32, i32, u32) -> *mut wl_buffer,
    pub wl_buffer_destroy: unsafe extern "C" fn(*mut wl_buffer),
    pub wl_shm_pool_destroy: unsafe extern "C" fn(*mut wl_shm_pool),

    // wl_output functions
    pub wl_output_add_listener:
        unsafe extern "C" fn(*mut wl_output, *const wl_output_listener, *mut c_void) -> i32,
    
    // wl_surface listener functions
    pub wl_surface_add_listener:
        unsafe extern "C" fn(*mut wl_surface, *const wl_surface_listener, *mut c_void) -> i32,

    // wayland-egl functions
    pub wl_egl_window_create: unsafe extern "C" fn(
        surface: *mut wl_surface,
        width: i32,
        height: i32,
    ) -> *mut wl_egl_window,
    pub wl_egl_window_destroy: unsafe extern "C" fn(egl_window: *mut wl_egl_window),
    pub wl_egl_window_resize: unsafe extern "C" fn(
        egl_window: *mut wl_egl_window,
        width: i32,
        height: i32,
        dx: i32,
        dy: i32,
    ),
}

impl Wayland {
    pub fn new() -> Result<Rc<Self>, DlError> {
        let lib_client = load_first_available::<Library>(&["libwayland-client.so.0"])?;
        let lib_egl = load_first_available::<Library>(&["libwayland-egl.so.1"])?;

        // Wayland uses a proxy-based system where most functions are actually `wl_proxy_marshal`.
        // The type-safe wrappers are often macros in C, so we load the core marshalling function
        // and then cast function pointers to the specific signatures we need.
        let wl_proxy_marshal_constructor =
            load_symbol!(lib_client, _, "wl_proxy_marshal_constructor");

        // Load wl_proxy_marshal and wl_proxy_add_listener once
        let wl_proxy_marshal_ptr = unsafe {
            lib_client
                .get_symbol::<*const c_void>("wl_proxy_marshal")
                .expect("wl_proxy_marshal not found")
        };
        let wl_proxy_add_listener_ptr = unsafe {
            lib_client
                .get_symbol::<*const c_void>("wl_proxy_add_listener")
                .expect("wl_proxy_add_listener not found")
        };

        Ok(Rc::new(Self {
            wl_display_connect: load_symbol!(lib_client, _, "wl_display_connect"),
            wl_display_disconnect: load_symbol!(lib_client, _, "wl_display_disconnect"),
            wl_display_get_registry: load_symbol!(lib_client, _, "wl_display_get_registry"),
            wl_display_roundtrip: load_symbol!(lib_client, _, "wl_display_roundtrip"),
            wl_display_dispatch_queue: load_symbol!(lib_client, _, "wl_display_dispatch_queue"),
            wl_display_dispatch_queue_pending: load_symbol!(
                lib_client,
                _,
                "wl_display_dispatch_queue_pending"
            ),
            wl_display_create_event_queue: load_symbol!(
                lib_client,
                _,
                "wl_display_create_event_queue"
            ),
            wl_event_queue_destroy: load_symbol!(lib_client, _, "wl_event_queue_destroy"),
            wl_display_flush: load_symbol!(lib_client, _, "wl_display_flush"),

            wl_proxy_marshal_constructor,
            wl_proxy_add_listener: load_symbol!(lib_client, _, "wl_proxy_add_listener"),
            wl_proxy_destroy: load_symbol!(lib_client, _, "wl_proxy_destroy"),
            wl_proxy_set_queue: load_symbol!(lib_client, _, "wl_proxy_set_queue"),

            wl_compositor_interface: unsafe {
                *load_symbol!(lib_client, *const wl_interface, "wl_compositor_interface")
            },
            wl_shm_interface: unsafe {
                *load_symbol!(lib_client, *const wl_interface, "wl_shm_interface")
            },
            wl_seat_interface: unsafe {
                *load_symbol!(lib_client, *const wl_interface, "wl_seat_interface")
            },
            wl_output_interface: unsafe {
                *load_symbol!(lib_client, *const wl_interface, "wl_output_interface")
            },
            xdg_wm_base_interface: unsafe {
                *load_symbol!(lib_client, *const wl_interface, "xdg_wm_base_interface")
            },

            wl_registry_bind: unsafe { std::mem::transmute(wl_proxy_marshal_constructor) },
            wl_compositor_create_surface: unsafe {
                std::mem::transmute(wl_proxy_marshal_constructor)
            },
            wl_surface_commit: unsafe { std::mem::transmute(wl_proxy_marshal_ptr) },
            wl_surface_attach: unsafe { std::mem::transmute(wl_proxy_marshal_ptr) },
            wl_surface_damage: unsafe { std::mem::transmute(wl_proxy_marshal_ptr) },
            wl_surface_frame: unsafe { std::mem::transmute(wl_proxy_marshal_constructor) },

            wl_callback_add_listener: unsafe { std::mem::transmute(wl_proxy_add_listener_ptr) },

            xdg_wm_base_pong: unsafe { std::mem::transmute(wl_proxy_marshal_ptr) },
            xdg_wm_base_get_xdg_surface: unsafe {
                std::mem::transmute(wl_proxy_marshal_constructor)
            },
            xdg_surface_get_toplevel: unsafe { std::mem::transmute(wl_proxy_marshal_constructor) },
            xdg_surface_ack_configure: unsafe { std::mem::transmute(wl_proxy_marshal_ptr) },
            xdg_toplevel_set_title: unsafe { std::mem::transmute(wl_proxy_marshal_ptr) },
            xdg_toplevel_set_minimized: unsafe { std::mem::transmute(wl_proxy_marshal_ptr) },
            xdg_toplevel_set_maximized: unsafe { std::mem::transmute(wl_proxy_marshal_ptr) },
            xdg_toplevel_unset_maximized: unsafe { std::mem::transmute(wl_proxy_marshal_ptr) },
            xdg_wm_base_add_listener: unsafe { std::mem::transmute(wl_proxy_add_listener_ptr) },
            xdg_surface_add_listener: unsafe { std::mem::transmute(wl_proxy_add_listener_ptr) },

            // xdg_popup and xdg_positioner
            xdg_wm_base_create_positioner: unsafe {
                std::mem::transmute(wl_proxy_marshal_constructor)
            },
            xdg_positioner_set_size: unsafe { std::mem::transmute(wl_proxy_marshal_ptr) },
            xdg_positioner_set_anchor_rect: unsafe { std::mem::transmute(wl_proxy_marshal_ptr) },
            xdg_positioner_set_anchor: unsafe { std::mem::transmute(wl_proxy_marshal_ptr) },
            xdg_positioner_set_gravity: unsafe { std::mem::transmute(wl_proxy_marshal_ptr) },
            xdg_positioner_set_constraint_adjustment: unsafe {
                std::mem::transmute(wl_proxy_marshal_ptr)
            },
            xdg_positioner_destroy: unsafe { std::mem::transmute(wl_proxy_marshal_ptr) },
            xdg_surface_get_popup: unsafe { std::mem::transmute(wl_proxy_marshal_constructor) },
            xdg_popup_add_listener: unsafe { std::mem::transmute(wl_proxy_add_listener_ptr) },
            xdg_popup_grab: unsafe { std::mem::transmute(wl_proxy_marshal_ptr) },
            xdg_popup_destroy: unsafe { std::mem::transmute(wl_proxy_marshal_ptr) },

            wl_seat_get_pointer: unsafe { std::mem::transmute(wl_proxy_marshal_constructor) },
            wl_seat_get_keyboard: unsafe { std::mem::transmute(wl_proxy_marshal_constructor) },
            wl_seat_add_listener: unsafe { std::mem::transmute(wl_proxy_add_listener_ptr) },
            wl_pointer_add_listener: unsafe { std::mem::transmute(wl_proxy_add_listener_ptr) },
            wl_keyboard_add_listener: unsafe { std::mem::transmute(wl_proxy_add_listener_ptr) },

            wl_shm_create_pool: unsafe { std::mem::transmute(wl_proxy_marshal_constructor) },
            wl_shm_pool_create_buffer: unsafe { std::mem::transmute(wl_proxy_marshal_constructor) },
            wl_buffer_destroy: unsafe { std::mem::transmute(wl_proxy_marshal_ptr) },
            wl_shm_pool_destroy: unsafe { std::mem::transmute(wl_proxy_marshal_ptr) },

            wl_output_add_listener: unsafe { std::mem::transmute(wl_proxy_add_listener_ptr) },
            wl_surface_add_listener: unsafe { std::mem::transmute(wl_proxy_add_listener_ptr) },

            wl_egl_window_create: load_symbol!(lib_egl, _, "wl_egl_window_create"),
            wl_egl_window_destroy: load_symbol!(lib_egl, _, "wl_egl_window_destroy"),
            wl_egl_window_resize: load_symbol!(lib_egl, _, "wl_egl_window_resize"),

            _lib_client: lib_client,
            _lib_egl: lib_egl,
        }))
    }
}

// Re-export Xkb from X11's dlopen module
pub use super::super::x11::dlopen::Xkb;
