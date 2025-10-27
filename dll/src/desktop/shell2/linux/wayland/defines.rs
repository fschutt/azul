//! C-style definitions for Wayland, EGL, and xkbcommon.

#![allow(non_camel_case_types, non_snake_case)]

use std::ffi::c_void;

// Opaque structs from wayland-client.h
#[repr(C)] pub struct wl_display { _private: [u8; 0] }
#[repr(C)] pub struct wl_registry { _private: [u8; 0] }
#[repr(C)] pub struct wl_compositor { _private: [u8; 0] }
#[repr(C)] pub struct wl_surface { _private: [u8; 0] }
#[repr(C)] pub struct wl_shell_surface { _private: [u8; 0] }
#[repr(C)] pub struct wl_seat { _private: [u8; 0] }
#[repr(C)] pub struct wl_pointer { _private: [u8; 0] }
#[repr(C)] pub struct wl_keyboard { _private: [u8; 0] }
#[repr(C)] pub struct wl_shm { _private: [u8; 0] }
#[repr(C)] pub struct wl_proxy { _private: [u8; 0] }
#[repr(C)] pub struct wl_event_queue { _private: [u8; 0] }

// Opaque structs from wayland-egl.h
#[repr(C)] pub struct wl_egl_window { _private: [u8; 0] }

// Opaque structs from xdg-shell.h
#[repr(C)] pub struct xdg_wm_base { _private: [u8; 0] }
#[repr(C)] pub struct xdg_surface { _private: [u8; 0] }
#[repr(C)] pub struct xdg_toplevel { _private: [u8; 0] }

// Opaque structs from xkbcommon
#[repr(C)] pub struct xkb_context { _private: [u8; 0] }
#[repr(C)] pub struct xkb_keymap { _private: [u8; 0] }
#[repr(C)] pub struct xkb_state { _private: [u8; 0] }
pub type xkb_keycode_t = u32;
pub type xkb_keysym_t = u32;

// EGL types
pub type EGLDisplay = *mut c_void;
pub type EGLConfig = *mut c_void;
pub type EGLContext = *mut c_void;
pub type EGLSurface = *mut c_void;
pub type EGLNativeDisplayType = *mut c_void;

// Listener structs
#[repr(C)]
pub struct wl_registry_listener {
    pub global: extern "C" fn(data: *mut c_void, registry: *mut wl_registry, name: u32, interface: *const i8, version: u32),
    pub global_remove: extern "C" fn(data: *mut c_void, registry: *mut wl_registry, name: u32),
}

#[repr(C)]
pub struct wl_seat_listener {
    pub capabilities: extern "C" fn(data: *mut c_void, seat: *mut wl_seat, capabilities: u32),
    pub name: extern "C" fn(data: *mut c_void, seat: *mut wl_seat, name: *const i8),
}

#[repr(C)]
pub struct wl_pointer_listener {
    pub enter: extern "C" fn(data: *mut c_void, pointer: *mut wl_pointer, serial: u32, surface: *mut wl_surface, surface_x: f64, surface_y: f64),
    pub leave: extern "C" fn(data: *mut c_void, pointer: *mut wl_pointer, serial: u32, surface: *mut wl_surface),
    pub motion: extern "C" fn(data: *mut c_void, pointer: *mut wl_pointer, time: u32, surface_x: f64, surface_y: f64),
    pub button: extern "C" fn(data: *mut c_void, pointer: *mut wl_pointer, serial: u32, time: u32, button: u32, state: u32),
    pub axis: extern "C" fn(data: *mut c_void, pointer: *mut wl_pointer, time: u32, axis: u32, value: f64),
}

#[repr(C)]
pub struct wl_keyboard_listener {
    pub keymap: extern "C" fn(data: *mut c_void, keyboard: *mut wl_keyboard, format: u32, fd: i32, size: u32),
    pub enter: extern "C" fn(data: *mut c_void, keyboard: *mut wl_keyboard, serial: u32, surface: *mut wl_surface, keys: *mut c_void),
    pub leave: extern "C" fn(data: *mut c_void, keyboard: *mut wl_keyboard, serial: u32, surface: *mut wl_surface),
    pub key: extern "C" fn(data: *mut c_void, keyboard: *mut wl_keyboard, serial: u32, time: u32, key: u32, state: u32),
    pub modifiers: extern "C" fn(data: *mut c_void, keyboard: *mut wl_keyboard, serial: u32, mods_depressed: u32, mods_latched: u32, mods_locked: u32, group: u32),
}

#[repr(C)]
pub struct xdg_wm_base_listener {
    pub ping: extern "C" fn(data: *mut c_void, xdg_wm_base: *mut xdg_wm_base, serial: u32),
}

#[repr(C)]
pub struct xdg_surface_listener {
    pub configure: extern "C" fn(data: *mut c_void, xdg_surface: *mut xdg_surface, serial: u32),
}