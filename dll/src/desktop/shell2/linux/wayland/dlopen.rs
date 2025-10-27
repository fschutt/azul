//! Dynamic loading for Wayland and related libraries.

use super::defines::*;
use crate::desktop::shell2::common::{dlopen::load_first_available, DlError, DynamicLibrary};
use std::rc::Rc;
use std::ffi::c_void;

macro_rules! load_symbol {
    ($lib:expr, $t:ty, $s:expr) => {
        match unsafe { $lib.get_symbol::<$t>($s) } {
            Ok(f) => f,
            Err(e) => return Err(e),
        }
    };
}

// Function pointer types
pub type wl_display_connect_t = unsafe extern "C" fn(name: *const i8) -> *mut wl_display;
pub type wl_display_disconnect_t = unsafe extern "C" fn(display: *mut wl_display);
pub type wl_display_get_registry_t = unsafe extern "C" fn(display: *mut wl_display) -> *mut wl_registry;
pub type wl_display_roundtrip_queue_t = unsafe extern "C" fn(display: *mut wl_display, queue: *mut wl_event_queue) -> i32;
pub type wl_display_create_event_queue_t = unsafe extern "C" fn(display: *mut wl_display) -> *mut wl_event_queue;
pub type wl_event_queue_destroy_t = unsafe extern "C" fn(queue: *mut wl_event_queue);
pub type wl_display_dispatch_queue_pending_t = unsafe extern "C" fn(display: *mut wl_display, queue: *mut wl_event_queue) -> i32;
pub type wl_display_flush_t = unsafe extern "C" fn(display: *mut wl_display) -> i32;

pub type wl_proxy_marshal_t = unsafe extern "C" fn(p: *mut wl_proxy, opcode: u32, ...);
pub type wl_proxy_set_queue_t = unsafe extern "C" fn(proxy: *mut wl_proxy, queue: *mut wl_event_queue);
pub type wl_proxy_destroy_t = unsafe extern "C" fn(proxy: *mut wl_proxy);
pub type wl_proxy_add_listener_t = unsafe extern "C" fn(proxy: *mut wl_proxy, implementation: *const c_void, data: *mut c_void) -> i32;

pub type wl_registry_add_listener_t = unsafe extern "C" fn(registry: *mut wl_registry, listener: *const wl_registry_listener, data: *mut c_void) -> i32;
pub type wl_compositor_create_surface_t = unsafe extern "C" fn(compositor: *mut wl_compositor) -> *mut wl_surface;
pub type wl_surface_destroy_t = unsafe extern "C" fn(surface: *mut wl_surface);
pub type wl_surface_commit_t = unsafe extern "C" fn(surface: *mut wl_surface);
pub type wl_surface_damage_t = unsafe extern "C" fn(surface: *mut wl_surface, x: i32, y: i32, width: i32, height: i32);

pub type wl_egl_window_create_t = unsafe extern "C" fn(surface: *mut wl_surface, width: i32, height: i32) -> *mut wl_egl_window;
pub type wl_egl_window_destroy_t = unsafe extern "C" fn(egl_window: *mut wl_egl_window);
pub type wl_egl_window_resize_t = unsafe extern "C" fn(egl_window: *mut wl_egl_window, width: i32, height: i32, dx: i32, dy: i32);

pub type xdg_wm_base_get_xdg_surface_t = unsafe extern "C" fn(wm_base: *mut xdg_wm_base, surface: *mut wl_surface) -> *mut xdg_surface;
pub type xdg_wm_base_destroy_t = unsafe extern "C" fn(wm_base: *mut xdg_wm_base);
pub type xdg_surface_add_listener_t = unsafe extern "C" fn(xdg_surface: *mut xdg_surface, listener: *const xdg_surface_listener, data: *mut c_void) -> i32;
pub type xdg_surface_get_toplevel_t = unsafe extern "C" fn(xdg_surface: *mut xdg_surface) -> *mut xdg_toplevel;
pub type xdg_surface_ack_configure_t = unsafe extern "C" fn(xdg_surface: *mut xdg_surface, serial: u32);
pub type xdg_surface_destroy_t = unsafe extern "C" fn(xdg_surface: *mut xdg_surface);
pub type xdg_toplevel_set_title_t = unsafe extern "C" fn(toplevel: *mut xdg_toplevel, title: *const i8);
pub type xdg_toplevel_destroy_t = unsafe extern "C" fn(toplevel: *mut xdg_toplevel);

pub struct Wayland {
    _lib: Box<dyn DynamicLibrary>,
    _lib_egl: Box<dyn DynamicLibrary>,
    pub wl_display_connect: wl_display_connect_t,
    pub wl_display_disconnect: wl_display_disconnect_t,
    pub wl_display_get_registry: wl_display_get_registry_t,
    pub wl_display_roundtrip_queue: wl_display_roundtrip_queue_t,
    pub wl_display_create_event_queue: wl_display_create_event_queue_t,
    pub wl_event_queue_destroy: wl_event_queue_destroy_t,
    pub wl_display_dispatch_queue_pending: wl_display_dispatch_queue_pending_t,
    pub wl_display_flush: wl_display_flush_t,
    pub wl_proxy_marshal: wl_proxy_marshal_t,
    pub wl_proxy_set_queue: wl_proxy_set_queue_t,
    pub wl_proxy_destroy: wl_proxy_destroy_t,
    pub wl_proxy_add_listener: wl_proxy_add_listener_t,
    pub wl_registry_add_listener: wl_registry_add_listener_t,
    pub wl_compositor_create_surface: wl_compositor_create_surface_t,
    pub wl_surface_destroy: wl_surface_destroy_t,
    pub wl_surface_commit: wl_surface_commit_t,
    pub wl_surface_damage: wl_surface_damage_t,
    pub wl_egl_window_create: wl_egl_window_create_t,
    pub wl_egl_window_destroy: wl_egl_window_destroy_t,
    pub wl_egl_window_resize: wl_egl_window_resize_t,
    pub xdg_wm_base_get_xdg_surface: xdg_wm_base_get_xdg_surface_t,
    pub xdg_wm_base_destroy: xdg_wm_base_destroy_t,
    pub xdg_surface_add_listener: xdg_surface_add_listener_t,
    pub xdg_surface_get_toplevel: xdg_surface_get_toplevel_t,
    pub xdg_surface_ack_configure: xdg_surface_ack_configure_t,
    pub xdg_surface_destroy: xdg_surface_destroy_t,
    pub xdg_toplevel_set_title: xdg_toplevel_set_title_t,
    pub xdg_toplevel_destroy: xdg_toplevel_destroy_t,
}

impl Wayland {
    pub fn new() -> Result<Rc<Self>, DlError> {
        let lib = load_first_available(&["libwayland-client.so.0"])?;
        let lib_egl = load_first_available(&["libwayland-egl.so.1"])?;
        Ok(Rc::new(Self {
            wl_display_connect: load_symbol!(lib, _, "wl_display_connect"),
            wl_display_disconnect: load_symbol!(lib, _, "wl_display_disconnect"),
            wl_display_get_registry: load_symbol!(lib, _, "wl_display_get_registry"),
            wl_display_roundtrip_queue: load_symbol!(lib, _, "wl_display_roundtrip_queue"),
            wl_display_create_event_queue: load_symbol!(lib, _, "wl_display_create_event_queue"),
            wl_event_queue_destroy: load_symbol!(lib, _, "wl_event_queue_destroy"),
            wl_display_dispatch_queue_pending: load_symbol!(lib, _, "wl_display_dispatch_queue_pending"),
            wl_display_flush: load_symbol!(lib, _, "wl_display_flush"),
            wl_proxy_marshal: load_symbol!(lib, _, "wl_proxy_marshal"),
            wl_proxy_set_queue: load_symbol!(lib, _, "wl_proxy_set_queue"),
            wl_proxy_destroy: load_symbol!(lib, _, "wl_proxy_destroy"),
            wl_proxy_add_listener: load_symbol!(lib, _, "wl_proxy_add_listener"),
            wl_registry_add_listener: load_symbol!(lib, _, "wl_registry_add_listener"),
            wl_compositor_create_surface: load_symbol!(lib, _, "wl_compositor_create_surface"),
            wl_surface_destroy: load_symbol!(lib, _, "wl_surface_destroy"),
            wl_surface_commit: load_symbol!(lib, _, "wl_surface_commit"),
            wl_surface_damage: load_symbol!(lib, _, "wl_surface_damage"),
            wl_egl_window_create: load_symbol!(lib_egl, _, "wl_egl_window_create"),
            wl_egl_window_destroy: load_symbol!(lib_egl, _, "wl_egl_window_destroy"),
            wl_egl_window_resize: load_symbol!(lib_egl, _, "wl_egl_window_resize"),
            xdg_wm_base_get_xdg_surface: load_symbol!(lib, _, "xdg_wm_base_get_xdg_surface"),
            xdg_wm_base_destroy: load_symbol!(lib, _, "xdg_wm_base_destroy"),
            xdg_surface_add_listener: load_symbol!(lib, _, "xdg_surface_add_listener"),
            xdg_surface_get_toplevel: load_symbol!(lib, _, "xdg_surface_get_toplevel"),
            xdg_surface_ack_configure: load_symbol!(lib, _, "xdg_surface_ack_configure"),
            xdg_surface_destroy: load_symbol!(lib, _, "xdg_surface_destroy"),
            xdg_toplevel_set_title: load_symbol!(lib, _, "xdg_toplevel_set_title"),
            xdg_toplevel_destroy: load_symbol!(lib, _, "xdg_toplevel_destroy"),
            _lib: Box::from(lib),
            _lib_egl: Box::from(lib_egl),
        }))
    }
}

pub type xkb_context_new_t = unsafe extern "C" fn(flags: u32) -> *mut xkb_context;
pub type xkb_context_unref_t = unsafe extern "C" fn(context: *mut xkb_context);
pub type xkb_keymap_new_from_string_t = unsafe extern "C" fn(context: *mut xkb_context, string: *const i8, format: u32, flags: u32) -> *mut xkb_keymap;
pub type xkb_keymap_unref_t = unsafe extern "C" fn(keymap: *mut xkb_keymap);
pub type xkb_state_new_t = unsafe extern "C" fn(keymap: *mut xkb_keymap) -> *mut xkb_state;
pub type xkb_state_unref_t = unsafe extern "C" fn(state: *mut xkb_state);
pub type xkb_state_update_mask_t = unsafe extern "C" fn(state: *mut xkb_state, depressed_mods: u32, latched_mods: u32, locked_mods: u32, depressed_layout: u32, latched_layout: u32, locked_layout: u32) -> u32;
pub type xkb_state_key_get_one_sym_t = unsafe extern "C" fn(state: *mut xkb_state, key: xkb_keycode_t) -> xkb_keysym_t;
pub type xkb_state_key_get_utf8_t = unsafe extern "C" fn(state: *mut xkb_state, key: xkb_keycode_t, buffer: *mut i8, size: usize) -> i32;

pub struct Xkb {
    _lib: Box<dyn DynamicLibrary>,
    pub xkb_context_new: xkb_context_new_t,
    pub xkb_context_unref: xkb_context_unref_t,
    pub xkb_keymap_new_from_string: xkb_keymap_new_from_string_t,
    pub xkb_keymap_unref: xkb_keymap_unref_t,
    pub xkb_state_new: xkb_state_new_t,
    pub xkb_state_unref: xkb_state_unref_t,
    pub xkb_state_update_mask: xkb_state_update_mask_t,
    pub xkb_state_key_get_one_sym: xkb_state_key_get_one_sym_t,
    pub xkb_state_key_get_utf8: xkb_state_key_get_utf8_t,
}

impl Xkb {
    pub fn new() -> Result<Rc<Self>, DlError> {
        let lib = load_first_available(&["libxkbcommon.so.0"])?;
        Ok(Rc::new(Self {
            xkb_context_new: load_symbol!(lib, _, "xkb_context_new"),
            xkb_context_unref: load_symbol!(lib, _, "xkb_context_unref"),
            xkb_keymap_new_from_string: load_symbol!(lib, _, "xkb_keymap_new_from_string"),
            xkb_keymap_unref: load_symbol!(lib, _, "xkb_keymap_unref"),
            xkb_state_new: load_symbol!(lib, _, "xkb_state_new"),
            xkb_state_unref: load_symbol!(lib, _, "xkb_state_unref"),
            xkb_state_update_mask: load_symbol!(lib, _, "xkb_state_update_mask"),
            xkb_state_key_get_one_sym: load_symbol!(lib, _, "xkb_state_key_get_one_sym"),
            xkb_state_key_get_utf8: load_symbol!(lib, _, "xkb_state_key_get_utf8"),
            _lib: Box::from(lib),
        }))
    }
}