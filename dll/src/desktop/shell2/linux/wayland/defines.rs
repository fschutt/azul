//! C-style definitions for Wayland, EGL, and xkbcommon.

#![allow(non_camel_case_types, non_snake_case)]

use std::ffi::{c_char, c_int, c_void};

// Opaque structs from wayland-client.h
#[repr(C)]
#[derive(Copy, Clone)]
pub struct wl_display {
    _private: [u8; 0],
}
#[repr(C)]
pub struct wl_registry {
    _private: [u8; 0],
}
#[repr(C)]
pub struct wl_compositor {
    _private: [u8; 0],
}
#[repr(C)]
pub struct wl_surface {
    _private: [u8; 0],
}
#[repr(C)]
pub struct wl_shell_surface {
    _private: [u8; 0],
}
#[repr(C)]
pub struct wl_seat {
    _private: [u8; 0],
}
#[repr(C)]
pub struct wl_pointer {
    _private: [u8; 0],
}
#[repr(C)]
pub struct wl_keyboard {
    _private: [u8; 0],
}
#[repr(C)]
pub struct wl_shm {
    _private: [u8; 0],
}
#[repr(C)]
pub struct wl_shm_pool {
    _private: [u8; 0],
}
#[repr(C)]
pub struct wl_buffer {
    _private: [u8; 0],
}

// wl_cursor types (from wayland-cursor.h)
#[repr(C)]
pub struct wl_cursor_theme {
    _private: [u8; 0],
}

#[repr(C)]
pub struct wl_cursor {
    pub image_count: u32,
    pub images: *mut *mut wl_cursor_image,
    pub name: *mut c_char,
}

#[repr(C)]
pub struct wl_cursor_image {
    pub width: u32,
    pub height: u32,
    pub hotspot_x: u32,
    pub hotspot_y: u32,
    pub delay: u32,
}

#[repr(C)]
pub struct wl_proxy {
    _private: [u8; 0],
}
#[repr(C)]
pub struct wl_event_queue {
    _private: [u8; 0],
}
#[repr(C)]
pub struct wl_callback {
    _private: [u8; 0],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct wl_output {
    _private: [u8; 0],
}
#[repr(C)]
#[derive(Clone, Copy)]
pub struct wl_interface {
    pub name: *const c_char,
    pub version: c_int,
    pub method_count: c_int,
    pub methods: *const wl_message,
    pub event_count: c_int,
    pub events: *const wl_message,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct wl_message {
    pub name: *const c_char,
    pub signature: *const c_char,
    pub types: *const *const wl_interface,
}

// Opaque structs from wayland-egl.h
#[repr(C)]
pub struct wl_egl_window {
    _private: [u8; 0],
}

// Opaque structs from xdg-shell.h
#[repr(C)]
pub struct xdg_wm_base {
    _private: [u8; 0],
}
#[repr(C)]
pub struct xdg_surface {
    _private: [u8; 0],
}
#[repr(C)]
pub struct xdg_toplevel {
    _private: [u8; 0],
}
#[repr(C)]
pub struct xdg_popup {
    _private: [u8; 0],
}
#[repr(C)]
pub struct xdg_positioner {
    _private: [u8; 0],
}

// Re-export XKB types from X11 defines (they're the same across X11 and Wayland)
pub use super::super::x11::defines::{
    xkb_context, xkb_keycode_t, xkb_keymap, xkb_keysym_t, xkb_state,
};

// EGL types
pub type EGLDisplay = *mut c_void;
pub type EGLConfig = *mut c_void;
pub type EGLContext = *mut c_void;
pub type EGLSurface = *mut c_void;
pub type EGLNativeDisplayType = *mut c_void;

// Listener structs
#[repr(C)]
#[derive(Copy, Clone)]
pub struct wl_registry_listener {
    pub global: extern "C" fn(
        data: *mut c_void,
        registry: *mut wl_registry,
        name: u32,
        interface: *const i8,
        version: u32,
    ),
    pub global_remove: extern "C" fn(data: *mut c_void, registry: *mut wl_registry, name: u32),
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct wl_seat_listener {
    pub capabilities: extern "C" fn(data: *mut c_void, seat: *mut wl_seat, capabilities: u32),
    pub name: extern "C" fn(data: *mut c_void, seat: *mut wl_seat, name: *const i8),
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct wl_pointer_listener {
    pub enter: extern "C" fn(
        data: *mut c_void,
        pointer: *mut wl_pointer,
        serial: u32,
        surface: *mut wl_surface,
        surface_x: f64,
        surface_y: f64,
    ),
    pub leave: extern "C" fn(
        data: *mut c_void,
        pointer: *mut wl_pointer,
        serial: u32,
        surface: *mut wl_surface,
    ),
    pub motion: extern "C" fn(
        data: *mut c_void,
        pointer: *mut wl_pointer,
        time: u32,
        surface_x: f64,
        surface_y: f64,
    ),
    pub button: extern "C" fn(
        data: *mut c_void,
        pointer: *mut wl_pointer,
        serial: u32,
        time: u32,
        button: u32,
        state: u32,
    ),
    pub axis: extern "C" fn(
        data: *mut c_void,
        pointer: *mut wl_pointer,
        time: u32,
        axis: u32,
        value: f64,
    ),
    pub frame: extern "C" fn(data: *mut c_void, pointer: *mut wl_pointer),
    pub axis_source: extern "C" fn(data: *mut c_void, pointer: *mut wl_pointer, axis_source: u32),
    pub axis_stop: extern "C" fn(data: *mut c_void, pointer: *mut wl_pointer, time: u32, axis: u32),
    pub axis_discrete:
        extern "C" fn(data: *mut c_void, pointer: *mut wl_pointer, axis: u32, discrete: i32),
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct wl_keyboard_listener {
    pub keymap: extern "C" fn(
        data: *mut c_void,
        keyboard: *mut wl_keyboard,
        format: u32,
        fd: i32,
        size: u32,
    ),
    pub enter: extern "C" fn(
        data: *mut c_void,
        keyboard: *mut wl_keyboard,
        serial: u32,
        surface: *mut wl_surface,
        keys: *mut c_void,
    ),
    pub leave: extern "C" fn(
        data: *mut c_void,
        keyboard: *mut wl_keyboard,
        serial: u32,
        surface: *mut wl_surface,
    ),
    pub key: extern "C" fn(
        data: *mut c_void,
        keyboard: *mut wl_keyboard,
        serial: u32,
        time: u32,
        key: u32,
        state: u32,
    ),
    pub modifiers: extern "C" fn(
        data: *mut c_void,
        keyboard: *mut wl_keyboard,
        serial: u32,
        mods_depressed: u32,
        mods_latched: u32,
        mods_locked: u32,
        group: u32,
    ),
    pub repeat_info:
        extern "C" fn(data: *mut c_void, wl_keyboard: *mut wl_keyboard, rate: i32, delay: i32),
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct xdg_wm_base_listener {
    pub ping: extern "C" fn(data: *mut c_void, xdg_wm_base: *mut xdg_wm_base, serial: u32),
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct xdg_surface_listener {
    pub configure: extern "C" fn(data: *mut c_void, xdg_surface: *mut xdg_surface, serial: u32),
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct xdg_popup_listener {
    pub configure: extern "C" fn(
        data: *mut c_void,
        xdg_popup: *mut xdg_popup,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    ),
    pub popup_done: extern "C" fn(data: *mut c_void, xdg_popup: *mut xdg_popup),
}

#[repr(C)]
#[derive(Copy, Clone)]
pub struct wl_callback_listener {
    pub done: extern "C" fn(data: *mut c_void, callback: *mut wl_callback, callback_data: u32),
}

// wl_output listener for monitor information
#[repr(C)]
#[derive(Copy, Clone)]
pub struct wl_output_listener {
    pub geometry: extern "C" fn(
        data: *mut c_void,
        wl_output: *mut wl_output,
        x: i32,
        y: i32,
        physical_width: i32,
        physical_height: i32,
        subpixel: i32,
        make: *const c_char,
        model: *const c_char,
        transform: i32,
    ),
    pub mode: extern "C" fn(
        data: *mut c_void,
        wl_output: *mut wl_output,
        flags: u32,
        width: i32,
        height: i32,
        refresh: i32,
    ),
    pub done: extern "C" fn(data: *mut c_void, wl_output: *mut wl_output),
    pub scale: extern "C" fn(data: *mut c_void, wl_output: *mut wl_output, factor: i32),
}

// wl_surface listener for enter/leave events
#[repr(C)]
#[derive(Copy, Clone)]
pub struct wl_surface_listener {
    pub enter:
        extern "C" fn(data: *mut c_void, wl_surface: *mut wl_surface, output: *mut wl_output),
    pub leave:
        extern "C" fn(data: *mut c_void, wl_surface: *mut wl_surface, output: *mut wl_output),
}

// XDG Positioner Enums (from xdg-shell protocol)
pub const XDG_POSITIONER_ANCHOR_NONE: u32 = 0;
pub const XDG_POSITIONER_ANCHOR_TOP: u32 = 1;
pub const XDG_POSITIONER_ANCHOR_BOTTOM: u32 = 2;
pub const XDG_POSITIONER_ANCHOR_LEFT: u32 = 3;
pub const XDG_POSITIONER_ANCHOR_RIGHT: u32 = 4;
pub const XDG_POSITIONER_ANCHOR_TOP_LEFT: u32 = 5;
pub const XDG_POSITIONER_ANCHOR_BOTTOM_LEFT: u32 = 6;
pub const XDG_POSITIONER_ANCHOR_TOP_RIGHT: u32 = 7;
pub const XDG_POSITIONER_ANCHOR_BOTTOM_RIGHT: u32 = 8;

pub const XDG_POSITIONER_GRAVITY_NONE: u32 = 0;
pub const XDG_POSITIONER_GRAVITY_TOP: u32 = 1;
pub const XDG_POSITIONER_GRAVITY_BOTTOM: u32 = 2;
pub const XDG_POSITIONER_GRAVITY_LEFT: u32 = 3;
pub const XDG_POSITIONER_GRAVITY_RIGHT: u32 = 4;
pub const XDG_POSITIONER_GRAVITY_TOP_LEFT: u32 = 5;
pub const XDG_POSITIONER_GRAVITY_BOTTOM_LEFT: u32 = 6;
pub const XDG_POSITIONER_GRAVITY_TOP_RIGHT: u32 = 7;
pub const XDG_POSITIONER_GRAVITY_BOTTOM_RIGHT: u32 = 8;

pub const XDG_POSITIONER_CONSTRAINT_ADJUSTMENT_NONE: u32 = 0;
pub const XDG_POSITIONER_CONSTRAINT_ADJUSTMENT_SLIDE_X: u32 = 1;
pub const XDG_POSITIONER_CONSTRAINT_ADJUSTMENT_SLIDE_Y: u32 = 2;
pub const XDG_POSITIONER_CONSTRAINT_ADJUSTMENT_FLIP_X: u32 = 4;
pub const XDG_POSITIONER_CONSTRAINT_ADJUSTMENT_FLIP_Y: u32 = 8;
pub const XDG_POSITIONER_CONSTRAINT_ADJUSTMENT_RESIZE_X: u32 = 16;
pub const XDG_POSITIONER_CONSTRAINT_ADJUSTMENT_RESIZE_Y: u32 = 32;

// Wayland Constants
pub const WL_SEAT_CAPABILITY_POINTER: u32 = 1;
pub const WL_SEAT_CAPABILITY_KEYBOARD: u32 = 2;

pub const WL_KEYBOARD_KEYMAP_FORMAT_XKB_V1: u32 = 1;
pub const WL_KEYBOARD_KEY_STATE_RELEASED: u32 = 0;
pub const WL_KEYBOARD_KEY_STATE_PRESSED: u32 = 1;

pub const WL_POINTER_BUTTON_STATE_RELEASED: u32 = 0;
pub const WL_POINTER_BUTTON_STATE_PRESSED: u32 = 1;
pub const WL_POINTER_AXIS_VERTICAL_SCROLL: u32 = 0;
pub const WL_POINTER_AXIS_HORIZONTAL_SCROLL: u32 = 1;

pub const WL_SHM_FORMAT_ARGB8888: u32 = 0;

// XKB Constants
pub const XKB_CONTEXT_NO_FLAGS: u32 = 0;
pub const XKB_KEYMAP_FORMAT_TEXT_V1: u32 = 1;
pub const XKB_KEYMAP_COMPILE_NO_FLAGS: u32 = 0;

// EGL Constants (from x11/defines.rs, as they are the same)
pub use super::super::x11::defines::{
    EGL_ALPHA_SIZE, EGL_BLUE_SIZE, EGL_CONTEXT_MAJOR_VERSION, EGL_CONTEXT_MINOR_VERSION,
    EGL_CONTEXT_OPENGL_CORE_PROFILE_BIT, EGL_CONTEXT_OPENGL_PROFILE_MASK, EGL_DEPTH_SIZE,
    EGL_GREEN_SIZE, EGL_NONE, EGL_OPENGL_API, EGL_OPENGL_BIT, EGL_RED_SIZE, EGL_RENDERABLE_TYPE,
    EGL_STENCIL_SIZE, EGL_SURFACE_TYPE, EGL_WINDOW_BIT,
};
