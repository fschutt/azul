//! Dynamic loading for Wayland and related libraries.
//!
//! Provides the [`Wayland`] struct which holds dynamically loaded function
//! pointers from `libwayland-client`, `libwayland-egl`, and optionally
//! `libwayland-cursor`. Re-exports [`Library`], [`Xkb`], and [`Gtk3Im`]
//! from the X11 dlopen module.

use std::{
    ffi::{c_char, c_void},
    rc::Rc,
};

// Re-using the Library loader from X11
pub use super::super::x11::dlopen::Library;
use super::defines::*;
use crate::desktop::shell2::common::{
    dlopen::load_first_available, DlError, DynamicLibrary as DynamicLibraryTrait,
};
use crate::load_symbol;

/// Dynamically loaded Wayland client, EGL, and cursor function pointers.
pub struct Wayland {
    _lib_client: Library,
    _lib_egl: Library,
    _lib_cursor: Option<Library>,

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
    pub wl_display_get_fd: unsafe extern "C" fn(display: *mut wl_display) -> i32,

    // Note: These are variadic C functions. Rust doesn't support variadic fn pointers,
    // so we store them as raw pointers and cast when calling.
    pub wl_proxy_marshal_constructor: *const c_void,
    pub wl_proxy_marshal: *const c_void,
    pub wl_proxy_marshal_constructor_versioned: *const c_void,
    pub wl_proxy_marshal_flags: *const c_void, // null if libwayland <1.20
    pub wl_proxy_get_version: unsafe extern "C" fn(*mut wl_proxy) -> u32,
    pub wl_proxy_add_listener:
        unsafe extern "C" fn(*mut wl_proxy, *const c_void, *mut c_void) -> i32,
    pub wl_proxy_destroy: unsafe extern "C" fn(proxy: *mut wl_proxy),
    pub wl_proxy_set_queue: unsafe extern "C" fn(proxy: *mut wl_proxy, queue: *mut wl_event_queue),

    // Protocol interfaces (needed for wl_registry_bind)
    pub wl_compositor_interface: wl_interface,
    pub wl_subcompositor_interface: wl_interface,
    pub wl_shm_interface: wl_interface,
    pub wl_seat_interface: wl_interface,
    pub wl_output_interface: wl_interface,
    pub xdg_wm_base_interface: wl_interface,
    // Interfaces of constructed objects (needed to marshal constructor requests).
    pub wl_surface_interface: wl_interface,
    pub wl_pointer_interface: wl_interface,
    pub wl_keyboard_interface: wl_interface,
    pub wl_touch_interface: wl_interface,
    pub wl_callback_interface: wl_interface,
    pub wl_region_interface: wl_interface,
    pub wl_shm_pool_interface: wl_interface,
    pub wl_buffer_interface: wl_interface,
    pub wl_subsurface_interface: wl_interface,
    pub xdg_surface_interface: wl_interface,
    pub xdg_toplevel_interface: wl_interface,
    pub xdg_popup_interface: wl_interface,
    pub xdg_positioner_interface: wl_interface,

    // Convenience wrappers for common operations
    pub wl_registry_bind:
        unsafe extern "C" fn(*mut wl_registry, u32, *const wl_interface, u32) -> *mut c_void,
    pub wl_compositor_create_surface: unsafe extern "C" fn(*mut wl_compositor) -> *mut wl_surface,
    pub wl_subcompositor_get_subsurface: unsafe extern "C" fn(
        *mut wl_subcompositor,
        *mut wl_surface,
        *mut wl_surface,
    ) -> *mut wl_subsurface,
    pub wl_subsurface_set_position: unsafe extern "C" fn(*mut wl_subsurface, i32, i32),
    pub wl_subsurface_set_desync: unsafe extern "C" fn(*mut wl_subsurface),
    pub wl_subsurface_destroy: unsafe extern "C" fn(*mut wl_subsurface),
    pub wl_surface_destroy: unsafe extern "C" fn(*mut wl_surface),
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
    pub xdg_toplevel_set_title: unsafe extern "C" fn(*mut xdg_toplevel, *const c_char),
    pub xdg_toplevel_set_minimized: unsafe extern "C" fn(*mut xdg_toplevel),
    pub xdg_toplevel_set_maximized: unsafe extern "C" fn(*mut xdg_toplevel),
    pub xdg_toplevel_unset_maximized: unsafe extern "C" fn(*mut xdg_toplevel),
    pub xdg_toplevel_set_fullscreen:
        unsafe extern "C" fn(*mut xdg_toplevel, *mut wl_output),
    pub xdg_toplevel_unset_fullscreen: unsafe extern "C" fn(*mut xdg_toplevel),
    pub xdg_toplevel_set_min_size: unsafe extern "C" fn(*mut xdg_toplevel, i32, i32),
    pub xdg_toplevel_set_max_size: unsafe extern "C" fn(*mut xdg_toplevel, i32, i32),
    pub xdg_toplevel_move: unsafe extern "C" fn(*mut xdg_toplevel, *mut wl_seat, u32),
    pub xdg_wm_base_add_listener:
        unsafe extern "C" fn(*mut xdg_wm_base, *const xdg_wm_base_listener, *mut c_void) -> i32,
    pub xdg_surface_add_listener:
        unsafe extern "C" fn(*mut xdg_surface, *const xdg_surface_listener, *mut c_void) -> i32,
    pub xdg_toplevel_add_listener:
        unsafe extern "C" fn(*mut xdg_toplevel, *const xdg_toplevel_listener, *mut c_void) -> i32,

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
    pub wl_seat_get_touch: unsafe extern "C" fn(*mut wl_seat) -> *mut wl_touch,
    pub wl_touch_add_listener:
        unsafe extern "C" fn(*mut wl_touch, *const wl_touch_listener, *mut c_void) -> i32,
    pub zwp_tablet_manager_v2_get_tablet_seat:
        unsafe extern "C" fn(*mut zwp_tablet_manager_v2, *mut wl_seat) -> *mut zwp_tablet_seat_v2,
    pub zwp_tablet_seat_v2_add_listener: unsafe extern "C" fn(
        *mut zwp_tablet_seat_v2,
        *const zwp_tablet_seat_v2_listener,
        *mut c_void,
    ) -> i32,
    pub zwp_tablet_v2_add_listener:
        unsafe extern "C" fn(*mut zwp_tablet_v2, *const zwp_tablet_v2_listener, *mut c_void) -> i32,
    pub zwp_tablet_tool_v2_add_listener: unsafe extern "C" fn(
        *mut zwp_tablet_tool_v2,
        *const zwp_tablet_tool_v2_listener,
        *mut c_void,
    ) -> i32,

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

    // wl_region functions (for transparency)
    pub wl_compositor_create_region: unsafe extern "C" fn(*mut wl_compositor) -> *mut wl_region,
    pub wl_region_destroy: unsafe extern "C" fn(*mut wl_region),
    pub wl_region_add: unsafe extern "C" fn(*mut wl_region, i32, i32, i32, i32),
    pub wl_surface_set_opaque_region: unsafe extern "C" fn(*mut wl_surface, *mut wl_region),

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

    // wayland-cursor functions (optional - may not be available)
    pub wl_cursor_theme_load: Option<
        unsafe extern "C" fn(
            name: *const c_char,
            size: i32,
            shm: *mut wl_shm,
        ) -> *mut wl_cursor_theme,
    >,
    pub wl_cursor_theme_destroy: Option<unsafe extern "C" fn(theme: *mut wl_cursor_theme)>,
    pub wl_cursor_theme_get_cursor: Option<
        unsafe extern "C" fn(theme: *mut wl_cursor_theme, name: *const c_char) -> *mut wl_cursor,
    >,
    pub wl_cursor_image_get_buffer:
        Option<unsafe extern "C" fn(image: *mut wl_cursor_image) -> *mut wl_buffer>,
    pub wl_pointer_set_cursor: Option<
        unsafe extern "C" fn(
            pointer: *mut wl_pointer,
            serial: u32,
            surface: *mut wl_surface,
            hotspot_x: i32,
            hotspot_y: i32,
        ),
    >,
}

impl Wayland {
    /// Loads `libwayland-client`, `libwayland-egl`, and optionally
    /// `libwayland-cursor`, resolving all required symbols.
    pub fn new() -> Result<Rc<Self>, DlError> {
        let lib_client = load_first_available::<Library>(&["libwayland-client.so.0"])?;
        let lib_egl = load_first_available::<Library>(&["libwayland-egl.so.1"])?;

        // Wayland uses a proxy-based system where most functions are actually `wl_proxy_marshal`.
        // The type-safe wrappers are often macros in C, so we load the core marshalling function
        // and then cast function pointers to the specific signatures we need.
        // Note: These are variadic C functions. Rust doesn't support variadic fn pointers,
        // so we load them as raw pointers and transmute when calling.
        let wl_proxy_marshal_constructor_ptr = unsafe {
            lib_client
                .get_symbol::<*const c_void>("wl_proxy_marshal_constructor")?
        };

        // Load wl_proxy_marshal and wl_proxy_add_listener once
        let wl_proxy_marshal_ptr = unsafe {
            lib_client
                .get_symbol::<*const c_void>("wl_proxy_marshal")?
        };
        let wl_proxy_add_listener_ptr = unsafe {
            lib_client
                .get_symbol::<*const c_void>("wl_proxy_add_listener")?
        };
        // marshal_flags is libwayland >=1.20; null -> impl fns fall back to the constructor fns.
        let wl_proxy_marshal_flags_ptr = unsafe {
            lib_client
                .get_symbol::<*const c_void>("wl_proxy_marshal_flags")
                .unwrap_or(std::ptr::null())
        };
        let wl_proxy_marshal_constructor_versioned_ptr = unsafe {
            lib_client
                .get_symbol::<*const c_void>("wl_proxy_marshal_constructor_versioned")
                .unwrap_or(std::ptr::null())
        };

        let lib_cursor = load_first_available::<Library>(&[
            "libwayland-cursor.so.0",
            "libwayland-cursor.so",
        ]).ok();

        let wl = Rc::new(Self {
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
            wl_display_get_fd: load_symbol!(lib_client, _, "wl_display_get_fd"),

            wl_proxy_marshal_constructor: wl_proxy_marshal_constructor_ptr,
            wl_proxy_marshal: wl_proxy_marshal_ptr,
            wl_proxy_marshal_constructor_versioned: wl_proxy_marshal_constructor_versioned_ptr,
            wl_proxy_marshal_flags: wl_proxy_marshal_flags_ptr,
            wl_proxy_get_version: load_symbol!(lib_client, _, "wl_proxy_get_version"),
            wl_proxy_add_listener: load_symbol!(lib_client, _, "wl_proxy_add_listener"),
            wl_proxy_destroy: load_symbol!(lib_client, _, "wl_proxy_destroy"),
            wl_proxy_set_queue: load_symbol!(lib_client, _, "wl_proxy_set_queue"),

            wl_compositor_interface: unsafe {
                *load_symbol!(lib_client, *const wl_interface, "wl_compositor_interface")
            },
            wl_subcompositor_interface: unsafe {
                *load_symbol!(
                    lib_client,
                    *const wl_interface,
                    "wl_subcompositor_interface"
                )
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
            wl_surface_interface: unsafe { *load_symbol!(lib_client, *const wl_interface, "wl_surface_interface") },
            wl_pointer_interface: unsafe { *load_symbol!(lib_client, *const wl_interface, "wl_pointer_interface") },
            wl_keyboard_interface: unsafe { *load_symbol!(lib_client, *const wl_interface, "wl_keyboard_interface") },
            wl_touch_interface: unsafe { *load_symbol!(lib_client, *const wl_interface, "wl_touch_interface") },
            wl_callback_interface: unsafe { *load_symbol!(lib_client, *const wl_interface, "wl_callback_interface") },
            wl_region_interface: unsafe { *load_symbol!(lib_client, *const wl_interface, "wl_region_interface") },
            wl_shm_pool_interface: unsafe { *load_symbol!(lib_client, *const wl_interface, "wl_shm_pool_interface") },
            wl_buffer_interface: unsafe { *load_symbol!(lib_client, *const wl_interface, "wl_buffer_interface") },
            wl_subsurface_interface: unsafe { *load_symbol!(lib_client, *const wl_interface, "wl_subsurface_interface") },
            xdg_surface_interface: unsafe { *load_symbol!(lib_client, *const wl_interface, "xdg_surface_interface") },
            xdg_toplevel_interface: unsafe { *load_symbol!(lib_client, *const wl_interface, "xdg_toplevel_interface") },
            xdg_popup_interface: unsafe { *load_symbol!(lib_client, *const wl_interface, "xdg_popup_interface") },
            xdg_positioner_interface: unsafe { *load_symbol!(lib_client, *const wl_interface, "xdg_positioner_interface") },

            wl_registry_bind: wl_registry_bind_impl,
            wl_compositor_create_surface: wl_compositor_create_surface_impl,
            wl_subcompositor_get_subsurface: wl_subcompositor_get_subsurface_impl,
            wl_subsurface_set_position: wl_subsurface_set_position_impl,
            wl_subsurface_set_desync: wl_subsurface_set_desync_impl,
            wl_subsurface_destroy: wl_subsurface_destroy_impl,
            wl_surface_destroy: wl_surface_destroy_impl,
            wl_surface_commit: wl_surface_commit_impl,
            wl_surface_attach: wl_surface_attach_impl,
            wl_surface_damage: wl_surface_damage_impl,
            wl_surface_frame: wl_surface_frame_impl,

            wl_callback_add_listener: unsafe { std::mem::transmute(wl_proxy_add_listener_ptr) },

            xdg_wm_base_pong: xdg_wm_base_pong_impl,
            xdg_wm_base_get_xdg_surface: xdg_wm_base_get_xdg_surface_impl,
            xdg_surface_get_toplevel: xdg_surface_get_toplevel_impl,
            xdg_surface_ack_configure: xdg_surface_ack_configure_impl,
            xdg_toplevel_set_title: xdg_toplevel_set_title_impl,
            xdg_toplevel_set_minimized: xdg_toplevel_set_minimized_impl,
            xdg_toplevel_set_maximized: xdg_toplevel_set_maximized_impl,
            xdg_toplevel_unset_maximized: xdg_toplevel_unset_maximized_impl,
            xdg_toplevel_set_fullscreen: xdg_toplevel_set_fullscreen_impl,
            xdg_toplevel_unset_fullscreen: xdg_toplevel_unset_fullscreen_impl,
            xdg_toplevel_set_min_size: xdg_toplevel_set_min_size_impl,
            xdg_toplevel_set_max_size: xdg_toplevel_set_max_size_impl,
            xdg_toplevel_move: xdg_toplevel_move_impl,
            xdg_wm_base_add_listener: unsafe { std::mem::transmute(wl_proxy_add_listener_ptr) },
            xdg_surface_add_listener: unsafe { std::mem::transmute(wl_proxy_add_listener_ptr) },
            xdg_toplevel_add_listener: unsafe { std::mem::transmute(wl_proxy_add_listener_ptr) },

            // xdg_popup and xdg_positioner
            xdg_wm_base_create_positioner: xdg_wm_base_create_positioner_impl,
            xdg_positioner_set_size: xdg_positioner_set_size_impl,
            xdg_positioner_set_anchor_rect: xdg_positioner_set_anchor_rect_impl,
            xdg_positioner_set_anchor: xdg_positioner_set_anchor_impl,
            xdg_positioner_set_gravity: xdg_positioner_set_gravity_impl,
            xdg_positioner_set_constraint_adjustment: xdg_positioner_set_constraint_adjustment_impl,
            xdg_positioner_destroy: xdg_positioner_destroy_impl,
            xdg_surface_get_popup: xdg_surface_get_popup_impl,
            xdg_popup_add_listener: unsafe { std::mem::transmute(wl_proxy_add_listener_ptr) },
            xdg_popup_grab: xdg_popup_grab_impl,
            xdg_popup_destroy: xdg_popup_destroy_impl,

            wl_seat_get_pointer: wl_seat_get_pointer_impl,
            wl_seat_get_keyboard: wl_seat_get_keyboard_impl,
            wl_seat_add_listener: unsafe { std::mem::transmute(wl_proxy_add_listener_ptr) },
            wl_pointer_add_listener: unsafe { std::mem::transmute(wl_proxy_add_listener_ptr) },
            wl_keyboard_add_listener: unsafe { std::mem::transmute(wl_proxy_add_listener_ptr) },
            wl_seat_get_touch: wl_seat_get_touch_impl,
            wl_touch_add_listener: unsafe { std::mem::transmute(wl_proxy_add_listener_ptr) },
            zwp_tablet_manager_v2_get_tablet_seat: zwp_tablet_manager_v2_get_tablet_seat_impl,
            zwp_tablet_seat_v2_add_listener: unsafe {
                std::mem::transmute(wl_proxy_add_listener_ptr)
            },
            zwp_tablet_v2_add_listener: unsafe { std::mem::transmute(wl_proxy_add_listener_ptr) },
            zwp_tablet_tool_v2_add_listener: unsafe {
                std::mem::transmute(wl_proxy_add_listener_ptr)
            },

            wl_shm_create_pool: wl_shm_create_pool_impl,
            wl_shm_pool_create_buffer: wl_shm_pool_create_buffer_impl,
            wl_buffer_destroy: wl_buffer_destroy_impl,
            wl_shm_pool_destroy: wl_shm_pool_destroy_impl,

            wl_output_add_listener: unsafe { std::mem::transmute(wl_proxy_add_listener_ptr) },
            wl_surface_add_listener: unsafe { std::mem::transmute(wl_proxy_add_listener_ptr) },

            // wl_region functions for transparency
            wl_compositor_create_region: wl_compositor_create_region_impl,
            wl_region_destroy: wl_region_destroy_impl,
            wl_region_add: wl_region_add_impl,
            wl_surface_set_opaque_region: wl_surface_set_opaque_region_impl,

            wl_egl_window_create: load_symbol!(lib_egl, _, "wl_egl_window_create"),
            wl_egl_window_destroy: load_symbol!(lib_egl, _, "wl_egl_window_destroy"),
            wl_egl_window_resize: load_symbol!(lib_egl, _, "wl_egl_window_resize"),

            // wl_pointer_set_cursor is a core Wayland protocol request (via wl_proxy_marshal),
            // not a cursor-library function
            wl_pointer_set_cursor: Some(wl_pointer_set_cursor_impl),

            // Try to load wayland-cursor library once (optional)
            wl_cursor_theme_load: lib_cursor.as_ref().and_then(|lc| unsafe {
                lc.get_symbol("wl_cursor_theme_load").ok()
            }),
            wl_cursor_theme_destroy: lib_cursor.as_ref().and_then(|lc| unsafe {
                lc.get_symbol("wl_cursor_theme_destroy").ok()
            }),
            wl_cursor_theme_get_cursor: lib_cursor.as_ref().and_then(|lc| unsafe {
                lc.get_symbol("wl_cursor_theme_get_cursor").ok()
            }),
            wl_cursor_image_get_buffer: lib_cursor.as_ref().and_then(|lc| unsafe {
                lc.get_symbol("wl_cursor_image_get_buffer").ok()
            }),

            _lib_cursor: lib_cursor,
            _lib_client: lib_client,
            _lib_egl: lib_egl,
        });
        init_marshal_ctx(&wl);
        Ok(wl)
    }
}

// ===== Wayland request marshalling (fixes the broken transmute-wrappers) =====
// libwayland's request wrappers are inline in C; we reimplement them via the
// real exported wl_proxy_marshal* with the correct opcode + interface (the old
// fields dropped both -> the backend was dead at startup). A global ctx holds
// the marshaller + interface pointers (valid for the app-lifetime Wayland Rc,
// which owns _lib_client). See scripts/WACOM_TOUCH_API_RESEARCH.md.
struct WlMarshalCtx {
    marshal: *const c_void,
    marshal_constructor: *const c_void,
    marshal_constructor_versioned: *const c_void,
    wl_surface: *const wl_interface,
    wl_pointer: *const wl_interface,
    wl_keyboard: *const wl_interface,
    wl_touch: *const wl_interface,
    wl_callback: *const wl_interface,
    wl_region: *const wl_interface,
    wl_shm_pool: *const wl_interface,
    wl_buffer: *const wl_interface,
    wl_subsurface: *const wl_interface,
    xdg_surface: *const wl_interface,
    xdg_toplevel: *const wl_interface,
    xdg_popup: *const wl_interface,
    xdg_positioner: *const wl_interface,
}
unsafe impl Send for WlMarshalCtx {}
unsafe impl Sync for WlMarshalCtx {}
static WL_CTX: std::sync::OnceLock<WlMarshalCtx> = std::sync::OnceLock::new();
fn ctx() -> &'static WlMarshalCtx {
    WL_CTX.get().expect("wayland marshal ctx not initialised")
}
fn init_marshal_ctx(w: &Wayland) {
    let _ = WL_CTX.set(WlMarshalCtx {
        marshal: w.wl_proxy_marshal,
        marshal_constructor: w.wl_proxy_marshal_constructor,
        marshal_constructor_versioned: w.wl_proxy_marshal_constructor_versioned,
        wl_surface: &w.wl_surface_interface,
        wl_pointer: &w.wl_pointer_interface,
        wl_keyboard: &w.wl_keyboard_interface,
        wl_touch: &w.wl_touch_interface,
        wl_callback: &w.wl_callback_interface,
        wl_region: &w.wl_region_interface,
        wl_shm_pool: &w.wl_shm_pool_interface,
        wl_buffer: &w.wl_buffer_interface,
        wl_subsurface: &w.wl_subsurface_interface,
        xdg_surface: &w.xdg_surface_interface,
        xdg_toplevel: &w.xdg_toplevel_interface,
        xdg_popup: &w.xdg_popup_interface,
        xdg_positioner: &w.xdg_positioner_interface,
    });
}

// Constructor requests: marshal_constructor(proxy, opcode, ret_interface, NULL new_id, ...args).
unsafe extern "C" fn wl_registry_bind_impl(
    registry: *mut wl_registry,
    name: u32,
    interface: *const wl_interface,
    version: u32,
) -> *mut c_void {
    let c = ctx();
    // wl_registry.bind op 0; varargs: name, interface->name(string), version, NULL new_id.
    let f: unsafe extern "C" fn(
        *mut wl_proxy,
        u32,
        *const wl_interface,
        u32,
        u32,
        *const c_char,
        u32,
        *mut c_void,
    ) -> *mut wl_proxy = std::mem::transmute(c.marshal_constructor_versioned);
    f(
        registry as *mut wl_proxy,
        0,
        interface,
        version,
        name,
        (*interface).name,
        version,
        std::ptr::null_mut(),
    ) as *mut c_void
}
unsafe extern "C" fn wl_compositor_create_surface_impl(
    compositor: *mut wl_compositor,
) -> *mut wl_surface {
    let c = ctx();
    let f: unsafe extern "C" fn(
        *mut wl_proxy,
        u32,
        *const wl_interface,
        *mut c_void,
    ) -> *mut wl_proxy = std::mem::transmute(c.marshal_constructor);
    f(compositor as *mut wl_proxy, 0, c.wl_surface, std::ptr::null_mut()) as *mut wl_surface
}
unsafe extern "C" fn wl_seat_get_pointer_impl(seat: *mut wl_seat) -> *mut wl_pointer {
    let c = ctx();
    let f: unsafe extern "C" fn(
        *mut wl_proxy,
        u32,
        *const wl_interface,
        *mut c_void,
    ) -> *mut wl_proxy = std::mem::transmute(c.marshal_constructor);
    f(seat as *mut wl_proxy, 0, c.wl_pointer, std::ptr::null_mut()) as *mut wl_pointer
}
// Plain requests: marshal(proxy, opcode, ...args).
unsafe extern "C" fn wl_surface_commit_impl(surface: *mut wl_surface) {
    let c = ctx();
    let f: unsafe extern "C" fn(*mut wl_proxy, u32) = std::mem::transmute(c.marshal);
    f(surface as *mut wl_proxy, 6);
}

// --- remaining constructor requests (marshal_constructor: op, ret-interface, NULL new_id, args) ---
unsafe extern "C" fn wl_compositor_create_region_impl(comp: *mut wl_compositor) -> *mut wl_region {
    let c = ctx();
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, *const wl_interface, *mut c_void) -> *mut wl_proxy =
        std::mem::transmute(c.marshal_constructor);
    f(comp as *mut wl_proxy, 1, c.wl_region, std::ptr::null_mut()) as *mut wl_region
}
unsafe extern "C" fn wl_subcompositor_get_subsurface_impl(
    sc: *mut wl_subcompositor,
    surface: *mut wl_surface,
    parent: *mut wl_surface,
) -> *mut wl_subsurface {
    let c = ctx();
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, *const wl_interface, *mut c_void, *mut wl_surface, *mut wl_surface) -> *mut wl_proxy =
        std::mem::transmute(c.marshal_constructor);
    f(sc as *mut wl_proxy, 1, c.wl_subsurface, std::ptr::null_mut(), surface, parent) as *mut wl_subsurface
}
unsafe extern "C" fn wl_surface_frame_impl(s: *mut wl_surface) -> *mut wl_callback {
    let c = ctx();
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, *const wl_interface, *mut c_void) -> *mut wl_proxy =
        std::mem::transmute(c.marshal_constructor);
    f(s as *mut wl_proxy, 3, c.wl_callback, std::ptr::null_mut()) as *mut wl_callback
}
unsafe extern "C" fn xdg_wm_base_get_xdg_surface_impl(
    wm: *mut xdg_wm_base,
    surface: *mut wl_surface,
) -> *mut xdg_surface {
    let c = ctx();
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, *const wl_interface, *mut c_void, *mut wl_surface) -> *mut wl_proxy =
        std::mem::transmute(c.marshal_constructor);
    f(wm as *mut wl_proxy, 2, c.xdg_surface, std::ptr::null_mut(), surface) as *mut xdg_surface
}
unsafe extern "C" fn xdg_wm_base_create_positioner_impl(wm: *mut xdg_wm_base) -> *mut xdg_positioner {
    let c = ctx();
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, *const wl_interface, *mut c_void) -> *mut wl_proxy =
        std::mem::transmute(c.marshal_constructor);
    f(wm as *mut wl_proxy, 1, c.xdg_positioner, std::ptr::null_mut()) as *mut xdg_positioner
}
unsafe extern "C" fn xdg_surface_get_toplevel_impl(xs: *mut xdg_surface) -> *mut xdg_toplevel {
    let c = ctx();
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, *const wl_interface, *mut c_void) -> *mut wl_proxy =
        std::mem::transmute(c.marshal_constructor);
    f(xs as *mut wl_proxy, 1, c.xdg_toplevel, std::ptr::null_mut()) as *mut xdg_toplevel
}
unsafe extern "C" fn xdg_surface_get_popup_impl(
    xs: *mut xdg_surface,
    parent: *mut xdg_surface,
    positioner: *mut xdg_positioner,
) -> *mut xdg_popup {
    let c = ctx();
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, *const wl_interface, *mut c_void, *mut xdg_surface, *mut xdg_positioner) -> *mut wl_proxy =
        std::mem::transmute(c.marshal_constructor);
    f(xs as *mut wl_proxy, 3, c.xdg_popup, std::ptr::null_mut(), parent, positioner) as *mut xdg_popup
}
unsafe extern "C" fn wl_seat_get_keyboard_impl(seat: *mut wl_seat) -> *mut wl_keyboard {
    let c = ctx();
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, *const wl_interface, *mut c_void) -> *mut wl_proxy =
        std::mem::transmute(c.marshal_constructor);
    f(seat as *mut wl_proxy, 1, c.wl_keyboard, std::ptr::null_mut()) as *mut wl_keyboard
}
unsafe extern "C" fn wl_seat_get_touch_impl(seat: *mut wl_seat) -> *mut wl_touch {
    let c = ctx();
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, *const wl_interface, *mut c_void) -> *mut wl_proxy =
        std::mem::transmute(c.marshal_constructor);
    f(seat as *mut wl_proxy, 2, c.wl_touch, std::ptr::null_mut()) as *mut wl_touch
}
unsafe extern "C" fn zwp_tablet_manager_v2_get_tablet_seat_impl(
    mgr: *mut zwp_tablet_manager_v2,
    seat: *mut wl_seat,
) -> *mut zwp_tablet_seat_v2 {
    let c = ctx();
    // get_tablet_seat op0; the seat interface is the hand-rolled descriptor.
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, *const wl_interface, *mut c_void, *mut wl_seat) -> *mut wl_proxy =
        std::mem::transmute(c.marshal_constructor);
    f(mgr as *mut wl_proxy, 0, get_tablet_seat_v2_interface() as *const _, std::ptr::null_mut(), seat)
        as *mut zwp_tablet_seat_v2
}
unsafe extern "C" fn wl_shm_create_pool_impl(shm: *mut wl_shm, fd: i32, size: i32) -> *mut wl_shm_pool {
    let c = ctx();
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, *const wl_interface, *mut c_void, i32, i32) -> *mut wl_proxy =
        std::mem::transmute(c.marshal_constructor);
    f(shm as *mut wl_proxy, 0, c.wl_shm_pool, std::ptr::null_mut(), fd, size) as *mut wl_shm_pool
}
unsafe extern "C" fn wl_shm_pool_create_buffer_impl(
    pool: *mut wl_shm_pool,
    offset: i32,
    w: i32,
    h: i32,
    stride: i32,
    format: u32,
) -> *mut wl_buffer {
    let c = ctx();
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, *const wl_interface, *mut c_void, i32, i32, i32, i32, u32) -> *mut wl_proxy =
        std::mem::transmute(c.marshal_constructor);
    f(pool as *mut wl_proxy, 0, c.wl_buffer, std::ptr::null_mut(), offset, w, h, stride, format) as *mut wl_buffer
}

// --- plain requests (marshal: op, args) ---
unsafe extern "C" fn wl_subsurface_set_position_impl(p: *mut wl_subsurface, x: i32, y: i32) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, i32, i32) = std::mem::transmute(ctx().marshal);
    f(p as *mut wl_proxy, 1, x, y);
}
unsafe extern "C" fn wl_subsurface_set_desync_impl(p: *mut wl_subsurface) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32) = std::mem::transmute(ctx().marshal);
    f(p as *mut wl_proxy, 5);
}
unsafe extern "C" fn wl_subsurface_destroy_impl(p: *mut wl_subsurface) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32) = std::mem::transmute(ctx().marshal);
    f(p as *mut wl_proxy, 0);
}
unsafe extern "C" fn wl_surface_destroy_impl(s: *mut wl_surface) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32) = std::mem::transmute(ctx().marshal);
    f(s as *mut wl_proxy, 0);
}
unsafe extern "C" fn wl_surface_attach_impl(s: *mut wl_surface, buffer: *mut wl_buffer, x: i32, y: i32) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, *mut wl_buffer, i32, i32) = std::mem::transmute(ctx().marshal);
    f(s as *mut wl_proxy, 1, buffer, x, y);
}
unsafe extern "C" fn wl_surface_damage_impl(s: *mut wl_surface, x: i32, y: i32, w: i32, h: i32) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, i32, i32, i32, i32) = std::mem::transmute(ctx().marshal);
    f(s as *mut wl_proxy, 2, x, y, w, h);
}
unsafe extern "C" fn wl_surface_set_opaque_region_impl(s: *mut wl_surface, region: *mut wl_region) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, *mut wl_region) = std::mem::transmute(ctx().marshal);
    f(s as *mut wl_proxy, 4, region);
}
unsafe extern "C" fn xdg_wm_base_pong_impl(wm: *mut xdg_wm_base, serial: u32) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, u32) = std::mem::transmute(ctx().marshal);
    f(wm as *mut wl_proxy, 3, serial);
}
unsafe extern "C" fn xdg_surface_ack_configure_impl(xs: *mut xdg_surface, serial: u32) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, u32) = std::mem::transmute(ctx().marshal);
    f(xs as *mut wl_proxy, 4, serial);
}
unsafe extern "C" fn xdg_toplevel_set_title_impl(t: *mut xdg_toplevel, title: *const c_char) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, *const c_char) =
        std::mem::transmute(ctx().marshal);
    f(t as *mut wl_proxy, 2, title);
}
unsafe extern "C" fn xdg_toplevel_set_minimized_impl(t: *mut xdg_toplevel) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32) = std::mem::transmute(ctx().marshal);
    f(t as *mut wl_proxy, 12);
}
unsafe extern "C" fn xdg_toplevel_set_maximized_impl(t: *mut xdg_toplevel) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32) = std::mem::transmute(ctx().marshal);
    f(t as *mut wl_proxy, 10);
}
unsafe extern "C" fn xdg_toplevel_unset_maximized_impl(t: *mut xdg_toplevel) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32) = std::mem::transmute(ctx().marshal);
    f(t as *mut wl_proxy, 11);
}
unsafe extern "C" fn xdg_toplevel_set_fullscreen_impl(t: *mut xdg_toplevel, output: *mut wl_output) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, *mut wl_output) = std::mem::transmute(ctx().marshal);
    f(t as *mut wl_proxy, 13, output);
}
unsafe extern "C" fn xdg_toplevel_unset_fullscreen_impl(t: *mut xdg_toplevel) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32) = std::mem::transmute(ctx().marshal);
    f(t as *mut wl_proxy, 14);
}
unsafe extern "C" fn xdg_toplevel_set_min_size_impl(t: *mut xdg_toplevel, w: i32, h: i32) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, i32, i32) = std::mem::transmute(ctx().marshal);
    f(t as *mut wl_proxy, 8, w, h);
}
unsafe extern "C" fn xdg_toplevel_set_max_size_impl(t: *mut xdg_toplevel, w: i32, h: i32) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, i32, i32) = std::mem::transmute(ctx().marshal);
    f(t as *mut wl_proxy, 7, w, h);
}
unsafe extern "C" fn xdg_toplevel_move_impl(t: *mut xdg_toplevel, seat: *mut wl_seat, serial: u32) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, *mut wl_seat, u32) = std::mem::transmute(ctx().marshal);
    f(t as *mut wl_proxy, 5, seat, serial);
}
unsafe extern "C" fn xdg_positioner_set_size_impl(p: *mut xdg_positioner, w: i32, h: i32) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, i32, i32) = std::mem::transmute(ctx().marshal);
    f(p as *mut wl_proxy, 1, w, h);
}
unsafe extern "C" fn xdg_positioner_set_anchor_rect_impl(p: *mut xdg_positioner, x: i32, y: i32, w: i32, h: i32) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, i32, i32, i32, i32) = std::mem::transmute(ctx().marshal);
    f(p as *mut wl_proxy, 2, x, y, w, h);
}
unsafe extern "C" fn xdg_positioner_set_anchor_impl(p: *mut xdg_positioner, anchor: u32) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, u32) = std::mem::transmute(ctx().marshal);
    f(p as *mut wl_proxy, 4, anchor);
}
unsafe extern "C" fn xdg_positioner_set_gravity_impl(p: *mut xdg_positioner, gravity: u32) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, u32) = std::mem::transmute(ctx().marshal);
    f(p as *mut wl_proxy, 5, gravity);
}
unsafe extern "C" fn xdg_positioner_set_constraint_adjustment_impl(p: *mut xdg_positioner, adj: u32) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, u32) = std::mem::transmute(ctx().marshal);
    f(p as *mut wl_proxy, 6, adj);
}
unsafe extern "C" fn xdg_positioner_destroy_impl(p: *mut xdg_positioner) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32) = std::mem::transmute(ctx().marshal);
    f(p as *mut wl_proxy, 0);
}
unsafe extern "C" fn xdg_popup_grab_impl(popup: *mut xdg_popup, seat: *mut wl_seat, serial: u32) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, *mut wl_seat, u32) = std::mem::transmute(ctx().marshal);
    f(popup as *mut wl_proxy, 1, seat, serial);
}
unsafe extern "C" fn xdg_popup_destroy_impl(popup: *mut xdg_popup) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32) = std::mem::transmute(ctx().marshal);
    f(popup as *mut wl_proxy, 0);
}
unsafe extern "C" fn wl_buffer_destroy_impl(b: *mut wl_buffer) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32) = std::mem::transmute(ctx().marshal);
    f(b as *mut wl_proxy, 0);
}
unsafe extern "C" fn wl_shm_pool_destroy_impl(pool: *mut wl_shm_pool) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32) = std::mem::transmute(ctx().marshal);
    f(pool as *mut wl_proxy, 1);
}
unsafe extern "C" fn wl_region_destroy_impl(r: *mut wl_region) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32) = std::mem::transmute(ctx().marshal);
    f(r as *mut wl_proxy, 0);
}
unsafe extern "C" fn wl_region_add_impl(r: *mut wl_region, x: i32, y: i32, w: i32, h: i32) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, i32, i32, i32, i32) = std::mem::transmute(ctx().marshal);
    f(r as *mut wl_proxy, 1, x, y, w, h);
}
unsafe extern "C" fn wl_pointer_set_cursor_impl(
    pointer: *mut wl_pointer,
    serial: u32,
    surface: *mut wl_surface,
    hx: i32,
    hy: i32,
) {
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, u32, *mut wl_surface, i32, i32) = std::mem::transmute(ctx().marshal);
    f(pointer as *mut wl_proxy, 0, serial, surface, hx, hy);
}

// Re-export Xkb and GTK IM from X11's dlopen module
pub use super::super::x11::dlopen::{GdkRectangle, Gtk3Im, GtkIMContext, Xkb};
