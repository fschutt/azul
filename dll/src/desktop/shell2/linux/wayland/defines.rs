//! C-style FFI definitions for Wayland protocols, EGL, and xkbcommon.
//!
//! Covers: `wl_*` (core), `xdg_shell`, `zwp_text_input_v3`, `org_kde_kwin_blur`.
//! XKB types and EGL constants are re-exported from `x11::defines` (shared across backends).
//! The `get_*_interface()` functions construct `wl_interface` descriptors at runtime
//! for protocols that lack a generated C header (text-input-v3).

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
pub struct wl_region {
    _private: [u8; 0],
}

// KDE Blur protocol (org_kde_kwin_blur)
#[repr(C)]
pub struct org_kde_kwin_blur_manager {
    _private: [u8; 0],
}
#[repr(C)]
pub struct org_kde_kwin_blur {
    _private: [u8; 0],
}

// xdg-decoration-unstable-v1 (server-side window decorations / titlebar).
#[repr(C)]
pub struct zxdg_decoration_manager_v1 {
    _private: [u8; 0],
}
#[repr(C)]
pub struct zxdg_toplevel_decoration_v1 {
    _private: [u8; 0],
}

/// Listener for `zxdg_toplevel_decoration_v1`. The single `configure` event
/// reports the decoration mode the compositor chose (1 = client_side,
/// 2 = server_side).
#[repr(C)]
pub struct zxdg_toplevel_decoration_v1_listener {
    pub configure:
        extern "C" fn(data: *mut core::ffi::c_void, deco: *mut zxdg_toplevel_decoration_v1, mode: u32),
}
#[repr(C)]
pub struct wl_surface {
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
pub struct wl_touch {
    _private: [u8; 0],
}
// wl_data_device family (core wayland.xml) — file drag-and-drop DESTINATION.
#[repr(C)]
pub struct wl_data_device_manager {
    _private: [u8; 0],
}
#[repr(C)]
pub struct wl_data_device {
    _private: [u8; 0],
}
#[repr(C)]
pub struct wl_data_offer {
    _private: [u8; 0],
}

/// Listener for `wl_data_offer` events. `offer` enumerates the MIME types the
/// source advertises; `source_actions`/`action` are v3+ DnD-action negotiation.
#[repr(C)]
pub struct wl_data_offer_listener {
    pub offer: extern "C" fn(data: *mut c_void, offer: *mut wl_data_offer, mime_type: *const c_char),
    pub source_actions:
        extern "C" fn(data: *mut c_void, offer: *mut wl_data_offer, source_actions: u32),
    pub action: extern "C" fn(data: *mut c_void, offer: *mut wl_data_offer, dnd_action: u32),
}

/// Listener for `wl_data_device` events (drag enter/motion/leave/drop +
/// data_offer announcement + clipboard selection). Coordinates in `enter`/
/// `motion` are `wl_fixed` (24.8 fixed point — divide by 256 for pixels).
#[repr(C)]
pub struct wl_data_device_listener {
    pub data_offer: extern "C" fn(data: *mut c_void, dev: *mut wl_data_device, id: *mut wl_data_offer),
    pub enter: extern "C" fn(
        data: *mut c_void,
        dev: *mut wl_data_device,
        serial: u32,
        surface: *mut wl_surface,
        x: i32,
        y: i32,
        id: *mut wl_data_offer,
    ),
    pub leave: extern "C" fn(data: *mut c_void, dev: *mut wl_data_device),
    pub motion:
        extern "C" fn(data: *mut c_void, dev: *mut wl_data_device, time: u32, x: i32, y: i32),
    pub drop: extern "C" fn(data: *mut c_void, dev: *mut wl_data_device),
    pub selection:
        extern "C" fn(data: *mut c_void, dev: *mut wl_data_device, id: *mut wl_data_offer),
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
#[repr(C)]
#[derive(Copy, Clone)]
pub struct wl_array {
    pub size: usize,
    pub alloc: usize,
    pub data: *mut c_void,
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
pub struct wl_subcompositor {
    _private: [u8; 0],
}
#[repr(C)]
pub struct wl_subsurface {
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

/// Thread-safe wrapper for a `&'static wl_interface` stored in a `OnceLock`.
///
/// `wl_interface` holds raw `*const` pointers (name / methods / events) which
/// aren't auto-`Sync`, but in this module every pointer targets immutable
/// `Box::leak`-ed data that outlives the process. That makes cross-thread
/// reads safe; this wrapper carries that promise so `OnceLock` can accept it.
struct SyncInterface(&'static wl_interface);
unsafe impl Send for SyncInterface {}
unsafe impl Sync for SyncInterface {}

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

// Re-export EGL types from X11 defines (they're the same across X11 and Wayland)
pub use super::super::x11::defines::{
    EGLConfig, EGLContext, EGLDisplay, EGLNativeDisplayType, EGLSurface,
};

// Listener structs (wayland-client protocol)

/// Listener for `wl_registry` global/global_remove events.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct wl_registry_listener {
    pub global: extern "C" fn(
        data: *mut c_void,
        registry: *mut wl_registry,
        name: u32,
        interface: *const c_char,
        version: u32,
    ),
    pub global_remove: extern "C" fn(data: *mut c_void, registry: *mut wl_registry, name: u32),
}

/// Listener for `wl_seat` capability/name events.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct wl_seat_listener {
    pub capabilities: extern "C" fn(data: *mut c_void, seat: *mut wl_seat, capabilities: u32),
    pub name: extern "C" fn(data: *mut c_void, seat: *mut wl_seat, name: *const c_char),
}

/// Listener for `wl_pointer` events (enter, leave, motion, button, axis).
#[repr(C)]
#[derive(Copy, Clone)]
pub struct wl_pointer_listener {
    pub enter: extern "C" fn(
        data: *mut c_void,
        pointer: *mut wl_pointer,
        serial: u32,
        surface: *mut wl_surface,
        surface_x: i32, // wl_fixed_t (24.8); convert /256.0 in the handler
        surface_y: i32,
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
        surface_x: i32, // wl_fixed_t (24.8); convert /256.0 in the handler
        surface_y: i32,
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
        value: i32, // wl_fixed_t
    ),
    pub frame: extern "C" fn(data: *mut c_void, pointer: *mut wl_pointer),
    pub axis_source: extern "C" fn(data: *mut c_void, pointer: *mut wl_pointer, axis_source: u32),
    pub axis_stop: extern "C" fn(data: *mut c_void, pointer: *mut wl_pointer, time: u32, axis: u32),
    pub axis_discrete:
        extern "C" fn(data: *mut c_void, pointer: *mut wl_pointer, axis: u32, discrete: i32),
}

/// Listener for `wl_touch` events. x/y are wl_fixed_t (i32, 24.8); /256.0 in the handler.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct wl_touch_listener {
    pub down: extern "C" fn(
        data: *mut c_void,
        touch: *mut wl_touch,
        serial: u32,
        time: u32,
        surface: *mut wl_surface,
        id: i32,
        x: i32,
        y: i32,
    ),
    pub up: extern "C" fn(data: *mut c_void, touch: *mut wl_touch, serial: u32, time: u32, id: i32),
    pub motion:
        extern "C" fn(data: *mut c_void, touch: *mut wl_touch, time: u32, id: i32, x: i32, y: i32),
    pub frame: extern "C" fn(data: *mut c_void, touch: *mut wl_touch),
    pub cancel: extern "C" fn(data: *mut c_void, touch: *mut wl_touch),
    pub shape:
        extern "C" fn(data: *mut c_void, touch: *mut wl_touch, id: i32, major: i32, minor: i32),
    pub orientation:
        extern "C" fn(data: *mut c_void, touch: *mut wl_touch, id: i32, orientation: i32),
}

/// Listener for `wl_keyboard` events (keymap, enter, leave, key, modifiers, repeat_info).
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

// Listener structs (xdg-shell protocol)

/// Listener for `xdg_wm_base` ping events.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct xdg_wm_base_listener {
    pub ping: extern "C" fn(data: *mut c_void, xdg_wm_base: *mut xdg_wm_base, serial: u32),
}

/// Listener for `xdg_surface` configure events.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct xdg_surface_listener {
    pub configure: extern "C" fn(data: *mut c_void, xdg_surface: *mut xdg_surface, serial: u32),
}

/// Listener for `xdg_toplevel` events (configure, close, configure_bounds, wm_capabilities).
#[repr(C)]
#[derive(Copy, Clone)]
pub struct xdg_toplevel_listener {
    pub configure: extern "C" fn(
        data: *mut c_void,
        xdg_toplevel: *mut xdg_toplevel,
        width: i32,
        height: i32,
        states: *mut wl_array,
    ),
    pub close: extern "C" fn(data: *mut c_void, xdg_toplevel: *mut xdg_toplevel),
    pub configure_bounds:
        extern "C" fn(data: *mut c_void, xdg_toplevel: *mut xdg_toplevel, width: i32, height: i32),
    pub wm_capabilities: extern "C" fn(
        data: *mut c_void,
        xdg_toplevel: *mut xdg_toplevel,
        capabilities: *mut wl_array,
    ),
}

/// Listener for `xdg_popup` events (configure, popup_done).
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

/// Listener for `wl_callback` done events (used for frame callbacks).
#[repr(C)]
#[derive(Copy, Clone)]
pub struct wl_callback_listener {
    pub done: extern "C" fn(data: *mut c_void, callback: *mut wl_callback, callback_data: u32),
}

/// Listener for `wl_buffer.release` — the compositor is done reading the
/// buffer and the client may write to it again. Required for correct
/// double-buffering (writing to an attached buffer before release is a
/// protocol violation that shows as tearing).
#[repr(C)]
#[derive(Copy, Clone)]
pub struct wl_buffer_listener {
    pub release: extern "C" fn(data: *mut c_void, buffer: *mut wl_buffer),
}

/// Listener for `wl_output` events (geometry, mode, done, scale).
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

/// Listener for `wl_surface` enter/leave events (output tracking).
#[repr(C)]
#[derive(Copy, Clone)]
pub struct wl_surface_listener {
    pub enter:
        extern "C" fn(data: *mut c_void, wl_surface: *mut wl_surface, output: *mut wl_output),
    pub leave:
        extern "C" fn(data: *mut c_void, wl_surface: *mut wl_surface, output: *mut wl_output),
}

// XDG Positioner constants (xdg-shell protocol, version 3+)
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

// Wayland core protocol constants
pub const WL_SEAT_CAPABILITY_POINTER: u32 = 1;
pub const WL_SEAT_CAPABILITY_KEYBOARD: u32 = 2;
pub const WL_SEAT_CAPABILITY_TOUCH: u32 = 4;

pub const WL_KEYBOARD_KEYMAP_FORMAT_XKB_V1: u32 = 1;
pub const WL_KEYBOARD_KEY_STATE_RELEASED: u32 = 0;
pub const WL_KEYBOARD_KEY_STATE_PRESSED: u32 = 1;

pub const WL_POINTER_BUTTON_STATE_RELEASED: u32 = 0;
pub const WL_POINTER_BUTTON_STATE_PRESSED: u32 = 1;
pub const WL_POINTER_AXIS_VERTICAL_SCROLL: u32 = 0;
pub const WL_POINTER_AXIS_HORIZONTAL_SCROLL: u32 = 1;

pub const WL_SHM_FORMAT_ARGB8888: u32 = 0;

// Text input protocol v3 (zwp_text_input_v3)
#[repr(C)]
pub struct zwp_text_input_manager_v3 {
    _private: [u8; 0],
}

#[repr(C)]
pub struct zwp_text_input_v3 {
    _private: [u8; 0],
}

// zwp_text_input_v3 event listener
// Events: enter, leave, preedit_string, commit_string, delete_surrounding_text, done
#[repr(C)]
#[derive(Copy, Clone)]
pub struct zwp_text_input_v3_listener {
    pub enter: extern "C" fn(
        data: *mut c_void,
        text_input: *mut zwp_text_input_v3,
        surface: *mut wl_surface,
    ),
    pub leave: extern "C" fn(
        data: *mut c_void,
        text_input: *mut zwp_text_input_v3,
        surface: *mut wl_surface,
    ),
    pub preedit_string: extern "C" fn(
        data: *mut c_void,
        text_input: *mut zwp_text_input_v3,
        text: *const c_char,
        cursor_begin: i32,
        cursor_end: i32,
    ),
    pub commit_string: extern "C" fn(
        data: *mut c_void,
        text_input: *mut zwp_text_input_v3,
        text: *const c_char,
    ),
    pub delete_surrounding_text: extern "C" fn(
        data: *mut c_void,
        text_input: *mut zwp_text_input_v3,
        before_length: u32,
        after_length: u32,
    ),
    pub done: extern "C" fn(
        data: *mut c_void,
        text_input: *mut zwp_text_input_v3,
        serial: u32,
    ),
}

// zwp_text_input_v3 content type hint flags
pub const ZWP_TEXT_INPUT_V3_CONTENT_HINT_NONE: u32 = 0x0;
pub const ZWP_TEXT_INPUT_V3_CONTENT_HINT_COMPLETION: u32 = 0x1;
pub const ZWP_TEXT_INPUT_V3_CONTENT_HINT_SPELLCHECK: u32 = 0x2;
pub const ZWP_TEXT_INPUT_V3_CONTENT_HINT_AUTO_CAPITALIZATION: u32 = 0x4;
pub const ZWP_TEXT_INPUT_V3_CONTENT_HINT_LOWERCASE: u32 = 0x8;
pub const ZWP_TEXT_INPUT_V3_CONTENT_HINT_UPPERCASE: u32 = 0x10;
pub const ZWP_TEXT_INPUT_V3_CONTENT_HINT_TITLECASE: u32 = 0x20;
pub const ZWP_TEXT_INPUT_V3_CONTENT_HINT_HIDDEN_TEXT: u32 = 0x40;
pub const ZWP_TEXT_INPUT_V3_CONTENT_HINT_SENSITIVE_DATA: u32 = 0x80;
pub const ZWP_TEXT_INPUT_V3_CONTENT_HINT_LATIN: u32 = 0x100;
pub const ZWP_TEXT_INPUT_V3_CONTENT_HINT_MULTILINE: u32 = 0x200;

// zwp_text_input_v3 content purpose
pub const ZWP_TEXT_INPUT_V3_CONTENT_PURPOSE_NORMAL: u32 = 0;
pub const ZWP_TEXT_INPUT_V3_CONTENT_PURPOSE_ALPHA: u32 = 1;
pub const ZWP_TEXT_INPUT_V3_CONTENT_PURPOSE_DIGITS: u32 = 2;
pub const ZWP_TEXT_INPUT_V3_CONTENT_PURPOSE_NUMBER: u32 = 3;
pub const ZWP_TEXT_INPUT_V3_CONTENT_PURPOSE_PHONE: u32 = 4;
pub const ZWP_TEXT_INPUT_V3_CONTENT_PURPOSE_URL: u32 = 5;
pub const ZWP_TEXT_INPUT_V3_CONTENT_PURPOSE_EMAIL: u32 = 6;
pub const ZWP_TEXT_INPUT_V3_CONTENT_PURPOSE_NAME: u32 = 7;
pub const ZWP_TEXT_INPUT_V3_CONTENT_PURPOSE_PASSWORD: u32 = 8;
pub const ZWP_TEXT_INPUT_V3_CONTENT_PURPOSE_PIN: u32 = 9;
pub const ZWP_TEXT_INPUT_V3_CONTENT_PURPOSE_DATE: u32 = 10;
pub const ZWP_TEXT_INPUT_V3_CONTENT_PURPOSE_TIME: u32 = 11;
pub const ZWP_TEXT_INPUT_V3_CONTENT_PURPOSE_DATETIME: u32 = 12;
pub const ZWP_TEXT_INPUT_V3_CONTENT_PURPOSE_TERMINAL: u32 = 13;

// zwp_text_input_v3 change cause
pub const ZWP_TEXT_INPUT_V3_CHANGE_CAUSE_INPUT_METHOD: u32 = 0;
pub const ZWP_TEXT_INPUT_V3_CHANGE_CAUSE_OTHER: u32 = 1;

// zwp_text_input_v3 protocol opcodes
pub const ZWP_TEXT_INPUT_V3_DESTROY: u32 = 0;
pub const ZWP_TEXT_INPUT_V3_ENABLE: u32 = 1;
pub const ZWP_TEXT_INPUT_V3_DISABLE: u32 = 2;
pub const ZWP_TEXT_INPUT_V3_SET_SURROUNDING_TEXT: u32 = 3;
pub const ZWP_TEXT_INPUT_V3_SET_TEXT_CHANGE_CAUSE: u32 = 4;
pub const ZWP_TEXT_INPUT_V3_SET_CONTENT_TYPE: u32 = 5;
pub const ZWP_TEXT_INPUT_V3_SET_CURSOR_RECTANGLE: u32 = 6;
pub const ZWP_TEXT_INPUT_V3_COMMIT: u32 = 7;

// zwp_text_input_manager_v3 protocol opcodes
pub const ZWP_TEXT_INPUT_MANAGER_V3_DESTROY: u32 = 0;
pub const ZWP_TEXT_INPUT_MANAGER_V3_GET_TEXT_INPUT: u32 = 1;

// ── xdg-shell interface descriptors (hand-built) ─────────────────────────
//
// libwayland-client.so does NOT export the `xdg_*_interface` symbols — the
// xdg-shell protocol is `wayland-scanner`-generated and normally compiled
// into the client, not part of libwayland. We `dlopen` libwayland and provide
// these interface tables ourselves so proxy marshalling works (libwayland
// looks up `proxy.interface.methods[opcode].signature` to serialize a
// request, and `…events[opcode]` to dispatch an event). Opcodes, message
// order and wire signatures match the stable xdg-shell protocol; `types` are
// all-null (the constructed interface is passed explicitly to
// `wl_proxy_marshal_constructor`, and object args need no client-side type
// table on the classic path) — exactly like `get_text_input_v3_interface`.
// All objects are bound at version 1 (see events.rs registry handler), so the
// v4/v5 xdg_toplevel events are present but never dispatched.

/// Minimal `org_kde_kwin_blur_manager` interface (KDE Plasma blur protocol, not in
/// the core protocol so not exported by libwayland). A `wl_registry.bind` creates a
/// typed new-id proxy and REQUIRES a valid `wl_interface` — binding with a null
/// interface makes libwayland reject the request ("null value passed for arg 3").
/// v1 requests: create(new_id<org_kde_kwin_blur>, object<wl_surface>) = "no",
/// unset(object<wl_surface>) = "o".
pub fn get_kde_blur_manager_interface() -> &'static wl_interface {
    use std::sync::OnceLock;
    static INTERFACE: OnceLock<SyncInterface> = OnceLock::new();
    INTERFACE.get_or_init(|| SyncInterface({
        let nt: &'static [*const wl_interface; 4] = Box::leak(Box::new([
            std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(),
        ]));
        let requests: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"create\0".as_ptr() as _, signature: b"no\0".as_ptr() as _, types: nt.as_ptr() },
            wl_message { name: b"unset\0".as_ptr() as _,  signature: b"o\0".as_ptr() as _,  types: nt.as_ptr() },
        ]));
        Box::leak(Box::new(wl_interface {
            name: b"org_kde_kwin_blur_manager\0".as_ptr() as _, version: 1,
            method_count: 2, methods: requests.as_ptr(),
            event_count: 0, events: std::ptr::null(),
        }))
    })).0
}

/// Minimal `org_kde_kwin_blur` interface (the per-surface blur object returned by
/// `org_kde_kwin_blur_manager.create`). Needed so `create`'s `new_id` can build a
/// typed proxy. v1 requests, in opcode order: commit() = "", set_region(object?<wl_region>)
/// = "?o", release() [destructor] = "".
pub fn get_kde_blur_interface() -> &'static wl_interface {
    use std::sync::OnceLock;
    static INTERFACE: OnceLock<SyncInterface> = OnceLock::new();
    INTERFACE.get_or_init(|| SyncInterface({
        let nt: &'static [*const wl_interface; 4] = Box::leak(Box::new([
            std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(),
        ]));
        let requests: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"commit\0".as_ptr() as _,     signature: b"\0".as_ptr() as _,   types: nt.as_ptr() },
            wl_message { name: b"set_region\0".as_ptr() as _, signature: b"?o\0".as_ptr() as _, types: nt.as_ptr() },
            wl_message { name: b"release\0".as_ptr() as _,    signature: b"\0".as_ptr() as _,   types: nt.as_ptr() },
        ]));
        Box::leak(Box::new(wl_interface {
            name: b"org_kde_kwin_blur\0".as_ptr() as _, version: 1,
            method_count: 3, methods: requests.as_ptr(),
            event_count: 0, events: std::ptr::null(),
        }))
    })).0
}

/// Minimal `zxdg_decoration_manager_v1` interface (xdg-decoration-unstable-v1).
/// Not exported by libwayland (it's an unstable protocol extension), so we build
/// it by hand like the xdg_shell / blur tables. v1 requests, in opcode order:
/// destroy() = "", get_toplevel_decoration(new_id<zxdg_toplevel_decoration_v1>,
/// object<xdg_toplevel>) = "no". No events.
pub fn get_zxdg_decoration_manager_v1_interface() -> &'static wl_interface {
    use std::sync::OnceLock;
    static INTERFACE: OnceLock<SyncInterface> = OnceLock::new();
    INTERFACE.get_or_init(|| SyncInterface({
        let nt: &'static [*const wl_interface; 4] = Box::leak(Box::new([
            std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(),
        ]));
        let requests: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"destroy\0".as_ptr() as _,                 signature: b"\0".as_ptr() as _,  types: nt.as_ptr() },
            wl_message { name: b"get_toplevel_decoration\0".as_ptr() as _,  signature: b"no\0".as_ptr() as _, types: nt.as_ptr() },
        ]));
        Box::leak(Box::new(wl_interface {
            name: b"zxdg_decoration_manager_v1\0".as_ptr() as _, version: 1,
            method_count: 2, methods: requests.as_ptr(),
            event_count: 0, events: std::ptr::null(),
        }))
    })).0
}

/// Minimal `zxdg_toplevel_decoration_v1` interface (the per-toplevel decoration
/// object returned by `get_toplevel_decoration`). v1 requests, in opcode order:
/// destroy() = "", set_mode(uint mode) = "u", unset_mode() = "". One event:
/// configure(uint mode) = "u" (mode: 1 = client_side, 2 = server_side).
pub fn get_zxdg_toplevel_decoration_v1_interface() -> &'static wl_interface {
    use std::sync::OnceLock;
    static INTERFACE: OnceLock<SyncInterface> = OnceLock::new();
    INTERFACE.get_or_init(|| SyncInterface({
        let nt: &'static [*const wl_interface; 4] = Box::leak(Box::new([
            std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(),
        ]));
        let requests: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"destroy\0".as_ptr() as _,    signature: b"\0".as_ptr() as _,  types: nt.as_ptr() },
            wl_message { name: b"set_mode\0".as_ptr() as _,   signature: b"u\0".as_ptr() as _, types: nt.as_ptr() },
            wl_message { name: b"unset_mode\0".as_ptr() as _, signature: b"\0".as_ptr() as _,  types: nt.as_ptr() },
        ]));
        let events: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"configure\0".as_ptr() as _, signature: b"u\0".as_ptr() as _, types: nt.as_ptr() },
        ]));
        Box::leak(Box::new(wl_interface {
            name: b"zxdg_toplevel_decoration_v1\0".as_ptr() as _, version: 1,
            method_count: 3, methods: requests.as_ptr(),
            event_count: 1, events: events.as_ptr(),
        }))
    })).0
}

pub fn get_xdg_wm_base_interface() -> &'static wl_interface {
    use std::sync::OnceLock;
    static INTERFACE: OnceLock<SyncInterface> = OnceLock::new();
    INTERFACE.get_or_init(|| SyncInterface({
        let nt: &'static [*const wl_interface; 4] = Box::leak(Box::new([
            std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(),
        ]));
        let events: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"ping\0".as_ptr() as _, signature: b"u\0".as_ptr() as _, types: nt.as_ptr() },
        ]));
        let requests: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"destroy\0".as_ptr() as _,           signature: b"\0".as_ptr() as _,   types: nt.as_ptr() },
            wl_message { name: b"create_positioner\0".as_ptr() as _, signature: b"n\0".as_ptr() as _,  types: nt.as_ptr() },
            wl_message { name: b"get_xdg_surface\0".as_ptr() as _,   signature: b"no\0".as_ptr() as _, types: nt.as_ptr() },
            wl_message { name: b"pong\0".as_ptr() as _,              signature: b"u\0".as_ptr() as _,  types: nt.as_ptr() },
        ]));
        Box::leak(Box::new(wl_interface {
            name: b"xdg_wm_base\0".as_ptr() as _, version: 1,
            method_count: 4, methods: requests.as_ptr(),
            event_count: 1, events: events.as_ptr(),
        }))
    })).0
}

pub fn get_xdg_positioner_interface() -> &'static wl_interface {
    use std::sync::OnceLock;
    static INTERFACE: OnceLock<SyncInterface> = OnceLock::new();
    INTERFACE.get_or_init(|| SyncInterface({
        let nt: &'static [*const wl_interface; 4] = Box::leak(Box::new([
            std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(),
        ]));
        let requests: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"destroy\0".as_ptr() as _,                   signature: b"\0".as_ptr() as _,    types: nt.as_ptr() },
            wl_message { name: b"set_size\0".as_ptr() as _,                  signature: b"ii\0".as_ptr() as _,  types: nt.as_ptr() },
            wl_message { name: b"set_anchor_rect\0".as_ptr() as _,           signature: b"iiii\0".as_ptr() as _, types: nt.as_ptr() },
            wl_message { name: b"set_anchor\0".as_ptr() as _,                signature: b"u\0".as_ptr() as _,   types: nt.as_ptr() },
            wl_message { name: b"set_gravity\0".as_ptr() as _,               signature: b"u\0".as_ptr() as _,   types: nt.as_ptr() },
            wl_message { name: b"set_constraint_adjustment\0".as_ptr() as _, signature: b"u\0".as_ptr() as _,   types: nt.as_ptr() },
        ]));
        Box::leak(Box::new(wl_interface {
            name: b"xdg_positioner\0".as_ptr() as _, version: 1,
            method_count: 6, methods: requests.as_ptr(),
            event_count: 0, events: std::ptr::null(),
        }))
    })).0
}

pub fn get_xdg_surface_interface() -> &'static wl_interface {
    use std::sync::OnceLock;
    static INTERFACE: OnceLock<SyncInterface> = OnceLock::new();
    INTERFACE.get_or_init(|| SyncInterface({
        let nt: &'static [*const wl_interface; 4] = Box::leak(Box::new([
            std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(),
        ]));
        let events: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"configure\0".as_ptr() as _, signature: b"u\0".as_ptr() as _, types: nt.as_ptr() },
        ]));
        let requests: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"destroy\0".as_ptr() as _,             signature: b"\0".as_ptr() as _,    types: nt.as_ptr() },
            wl_message { name: b"get_toplevel\0".as_ptr() as _,        signature: b"n\0".as_ptr() as _,   types: nt.as_ptr() },
            wl_message { name: b"get_popup\0".as_ptr() as _,           signature: b"n?oo\0".as_ptr() as _, types: nt.as_ptr() },
            wl_message { name: b"set_window_geometry\0".as_ptr() as _, signature: b"iiii\0".as_ptr() as _, types: nt.as_ptr() },
            wl_message { name: b"ack_configure\0".as_ptr() as _,       signature: b"u\0".as_ptr() as _,   types: nt.as_ptr() },
        ]));
        Box::leak(Box::new(wl_interface {
            name: b"xdg_surface\0".as_ptr() as _, version: 1,
            method_count: 5, methods: requests.as_ptr(),
            event_count: 1, events: events.as_ptr(),
        }))
    })).0
}

pub fn get_xdg_toplevel_interface() -> &'static wl_interface {
    use std::sync::OnceLock;
    static INTERFACE: OnceLock<SyncInterface> = OnceLock::new();
    INTERFACE.get_or_init(|| SyncInterface({
        let nt: &'static [*const wl_interface; 4] = Box::leak(Box::new([
            std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(),
        ]));
        let events: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"configure\0".as_ptr() as _,        signature: b"iia\0".as_ptr() as _, types: nt.as_ptr() },
            wl_message { name: b"close\0".as_ptr() as _,            signature: b"\0".as_ptr() as _,    types: nt.as_ptr() },
            wl_message { name: b"configure_bounds\0".as_ptr() as _, signature: b"ii\0".as_ptr() as _,  types: nt.as_ptr() },
            wl_message { name: b"wm_capabilities\0".as_ptr() as _,  signature: b"a\0".as_ptr() as _,   types: nt.as_ptr() },
        ]));
        let requests: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"destroy\0".as_ptr() as _,          signature: b"\0".as_ptr() as _,    types: nt.as_ptr() },
            wl_message { name: b"set_parent\0".as_ptr() as _,       signature: b"?o\0".as_ptr() as _,  types: nt.as_ptr() },
            wl_message { name: b"set_title\0".as_ptr() as _,        signature: b"s\0".as_ptr() as _,   types: nt.as_ptr() },
            wl_message { name: b"set_app_id\0".as_ptr() as _,       signature: b"s\0".as_ptr() as _,   types: nt.as_ptr() },
            wl_message { name: b"show_window_menu\0".as_ptr() as _, signature: b"ouii\0".as_ptr() as _, types: nt.as_ptr() },
            wl_message { name: b"move\0".as_ptr() as _,             signature: b"ou\0".as_ptr() as _,  types: nt.as_ptr() },
            wl_message { name: b"resize\0".as_ptr() as _,           signature: b"ouu\0".as_ptr() as _, types: nt.as_ptr() },
            wl_message { name: b"set_max_size\0".as_ptr() as _,     signature: b"ii\0".as_ptr() as _,  types: nt.as_ptr() },
            wl_message { name: b"set_min_size\0".as_ptr() as _,     signature: b"ii\0".as_ptr() as _,  types: nt.as_ptr() },
            wl_message { name: b"set_maximized\0".as_ptr() as _,    signature: b"\0".as_ptr() as _,    types: nt.as_ptr() },
            wl_message { name: b"unset_maximized\0".as_ptr() as _,  signature: b"\0".as_ptr() as _,    types: nt.as_ptr() },
            wl_message { name: b"set_fullscreen\0".as_ptr() as _,   signature: b"?o\0".as_ptr() as _,  types: nt.as_ptr() },
            wl_message { name: b"unset_fullscreen\0".as_ptr() as _, signature: b"\0".as_ptr() as _,    types: nt.as_ptr() },
            wl_message { name: b"set_minimized\0".as_ptr() as _,    signature: b"\0".as_ptr() as _,    types: nt.as_ptr() },
        ]));
        Box::leak(Box::new(wl_interface {
            name: b"xdg_toplevel\0".as_ptr() as _, version: 1,
            method_count: 14, methods: requests.as_ptr(),
            event_count: 4, events: events.as_ptr(),
        }))
    })).0
}

pub fn get_xdg_popup_interface() -> &'static wl_interface {
    use std::sync::OnceLock;
    static INTERFACE: OnceLock<SyncInterface> = OnceLock::new();
    INTERFACE.get_or_init(|| SyncInterface({
        let nt: &'static [*const wl_interface; 4] = Box::leak(Box::new([
            std::ptr::null(), std::ptr::null(), std::ptr::null(), std::ptr::null(),
        ]));
        let events: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"configure\0".as_ptr() as _,  signature: b"iiii\0".as_ptr() as _, types: nt.as_ptr() },
            wl_message { name: b"popup_done\0".as_ptr() as _, signature: b"\0".as_ptr() as _,     types: nt.as_ptr() },
        ]));
        let requests: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"destroy\0".as_ptr() as _, signature: b"\0".as_ptr() as _,   types: nt.as_ptr() },
            wl_message { name: b"grab\0".as_ptr() as _,    signature: b"ou\0".as_ptr() as _, types: nt.as_ptr() },
        ]));
        Box::leak(Box::new(wl_interface {
            name: b"xdg_popup\0".as_ptr() as _, version: 1,
            method_count: 2, methods: requests.as_ptr(),
            event_count: 2, events: events.as_ptr(),
        }))
    })).0
}

/// Create the wl_interface for zwp_text_input_v3 at runtime.
///
/// The interface is leaked (Box::leak) because Wayland stores the pointer
/// on the proxy and expects it to live as long as the proxy. This is a
/// one-time ~300 byte allocation per process.
pub fn get_text_input_v3_interface() -> &'static wl_interface {
    use std::sync::OnceLock;
    static INTERFACE: OnceLock<SyncInterface> = OnceLock::new();

    INTERFACE.get_or_init(|| SyncInterface({
        let null_types: &'static [*const wl_interface; 4] = Box::leak(Box::new([
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
            std::ptr::null(),
        ]));

        let events: &'static [wl_message] = Box::leak(Box::new([
            wl_message {
                name: b"enter\0".as_ptr() as _,
                signature: b"o\0".as_ptr() as _,
                types: null_types.as_ptr(),
            },
            wl_message {
                name: b"leave\0".as_ptr() as _,
                signature: b"o\0".as_ptr() as _,
                types: null_types.as_ptr(),
            },
            wl_message {
                name: b"preedit_string\0".as_ptr() as _,
                signature: b"?sii\0".as_ptr() as _,
                types: null_types.as_ptr(),
            },
            wl_message {
                name: b"commit_string\0".as_ptr() as _,
                signature: b"?s\0".as_ptr() as _,
                types: null_types.as_ptr(),
            },
            wl_message {
                name: b"delete_surrounding_text\0".as_ptr() as _,
                signature: b"uu\0".as_ptr() as _,
                types: null_types.as_ptr(),
            },
            wl_message {
                name: b"done\0".as_ptr() as _,
                signature: b"u\0".as_ptr() as _,
                types: null_types.as_ptr(),
            },
        ]));

        let requests: &'static [wl_message] = Box::leak(Box::new([
            wl_message {
                name: b"destroy\0".as_ptr() as _,
                signature: b"\0".as_ptr() as _,
                types: null_types.as_ptr(),
            },
            wl_message {
                name: b"enable\0".as_ptr() as _,
                signature: b"\0".as_ptr() as _,
                types: null_types.as_ptr(),
            },
            wl_message {
                name: b"disable\0".as_ptr() as _,
                signature: b"\0".as_ptr() as _,
                types: null_types.as_ptr(),
            },
            wl_message {
                name: b"set_surrounding_text\0".as_ptr() as _,
                signature: b"sii\0".as_ptr() as _,
                types: null_types.as_ptr(),
            },
            wl_message {
                name: b"set_text_change_cause\0".as_ptr() as _,
                signature: b"u\0".as_ptr() as _,
                types: null_types.as_ptr(),
            },
            wl_message {
                name: b"set_content_type\0".as_ptr() as _,
                signature: b"uu\0".as_ptr() as _,
                types: null_types.as_ptr(),
            },
            wl_message {
                name: b"set_cursor_rectangle\0".as_ptr() as _,
                signature: b"iiii\0".as_ptr() as _,
                types: null_types.as_ptr(),
            },
            wl_message {
                name: b"commit\0".as_ptr() as _,
                signature: b"\0".as_ptr() as _,
                types: null_types.as_ptr(),
            },
        ]));

        Box::leak(Box::new(wl_interface {
            name: b"zwp_text_input_v3\0".as_ptr() as _,
            version: 1,
            method_count: 8,
            methods: requests.as_ptr(),
            event_count: 6,
            events: events.as_ptr(),
        }))
    })).0
}

/// Create the wl_interface for zwp_text_input_manager_v3 at runtime.
pub fn get_text_input_manager_v3_interface() -> &'static wl_interface {
    use std::sync::OnceLock;
    static INTERFACE: OnceLock<SyncInterface> = OnceLock::new();

    INTERFACE.get_or_init(|| SyncInterface({
        let null_types: &'static [*const wl_interface; 2] = Box::leak(Box::new([
            std::ptr::null(),
            std::ptr::null(),
        ]));

        let requests: &'static [wl_message] = Box::leak(Box::new([
            wl_message {
                name: b"destroy\0".as_ptr() as _,
                signature: b"\0".as_ptr() as _,
                types: null_types.as_ptr(),
            },
            wl_message {
                name: b"get_text_input\0".as_ptr() as _,
                signature: b"no\0".as_ptr() as _,
                types: null_types.as_ptr(),
            },
        ]));

        Box::leak(Box::new(wl_interface {
            name: b"zwp_text_input_manager_v3\0".as_ptr() as _,
            version: 1,
            method_count: 2,
            methods: requests.as_ptr(),
            event_count: 0,
            events: std::ptr::null(),
        }))
    })).0
}

// ===== Tablet protocol (zwp_tablet_v2) — opaque objects + hand-rolled wl_interface =====
// libwayland does NOT export the tablet interfaces, so we build them at runtime
// (Box::leak, like the text-input builders). All v2. Only new_id ('n') EVENT
// types are non-NULL (libwayland creates the proxy); 'o'/primitive -> NULL.
// Data per scripts/WACOM_TOUCH_API_RESEARCH.md. See WACOM research for opcodes.
#[repr(C)]
pub struct zwp_tablet_manager_v2 {
    _private: [u8; 0],
}
#[repr(C)]
pub struct zwp_tablet_seat_v2 {
    _private: [u8; 0],
}
#[repr(C)]
pub struct zwp_tablet_v2 {
    _private: [u8; 0],
}
#[repr(C)]
pub struct zwp_tablet_tool_v2 {
    _private: [u8; 0],
}
#[repr(C)]
pub struct zwp_tablet_pad_v2 {
    _private: [u8; 0],
}
#[repr(C)]
pub struct zwp_tablet_pad_group_v2 {
    _private: [u8; 0],
}
#[repr(C)]
pub struct zwp_tablet_pad_ring_v2 {
    _private: [u8; 0],
}
#[repr(C)]
pub struct zwp_tablet_pad_strip_v2 {
    _private: [u8; 0],
}
#[repr(C)]
pub struct zwp_tablet_pad_dial_v2 {
    _private: [u8; 0],
}

/// Listener for `zwp_tablet_seat_v2` (device hotplug). new_ids are server-created.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct zwp_tablet_seat_v2_listener {
    pub tablet_added:
        extern "C" fn(data: *mut c_void, seat: *mut zwp_tablet_seat_v2, id: *mut zwp_tablet_v2),
    pub tool_added: extern "C" fn(
        data: *mut c_void,
        seat: *mut zwp_tablet_seat_v2,
        id: *mut zwp_tablet_tool_v2,
    ),
    pub pad_added:
        extern "C" fn(data: *mut c_void, seat: *mut zwp_tablet_seat_v2, id: *mut zwp_tablet_pad_v2),
}

/// Listener for `zwp_tablet_v2` (physical tablet descriptive events).
#[repr(C)]
#[derive(Copy, Clone)]
pub struct zwp_tablet_v2_listener {
    pub name: extern "C" fn(data: *mut c_void, tablet: *mut zwp_tablet_v2, name: *const c_char),
    pub id: extern "C" fn(data: *mut c_void, tablet: *mut zwp_tablet_v2, vid: u32, pid: u32),
    pub path: extern "C" fn(data: *mut c_void, tablet: *mut zwp_tablet_v2, path: *const c_char),
    pub done: extern "C" fn(data: *mut c_void, tablet: *mut zwp_tablet_v2),
    pub removed: extern "C" fn(data: *mut c_void, tablet: *mut zwp_tablet_v2),
    pub bustype: extern "C" fn(data: *mut c_void, tablet: *mut zwp_tablet_v2, bustype: u32),
}

/// Listener for `zwp_tablet_tool_v2` (the pen). Coordinate args are wl_fixed_t
/// (i32; /256.0); pressure/distance are uint 0..65535.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct zwp_tablet_tool_v2_listener {
    pub type_: extern "C" fn(data: *mut c_void, tool: *mut zwp_tablet_tool_v2, tool_type: u32),
    pub hardware_serial:
        extern "C" fn(data: *mut c_void, tool: *mut zwp_tablet_tool_v2, hi: u32, lo: u32),
    pub hardware_id_wacom:
        extern "C" fn(data: *mut c_void, tool: *mut zwp_tablet_tool_v2, hi: u32, lo: u32),
    pub capability: extern "C" fn(data: *mut c_void, tool: *mut zwp_tablet_tool_v2, capability: u32),
    pub done: extern "C" fn(data: *mut c_void, tool: *mut zwp_tablet_tool_v2),
    pub removed: extern "C" fn(data: *mut c_void, tool: *mut zwp_tablet_tool_v2),
    pub proximity_in: extern "C" fn(
        data: *mut c_void,
        tool: *mut zwp_tablet_tool_v2,
        serial: u32,
        tablet: *mut zwp_tablet_v2,
        surface: *mut wl_surface,
    ),
    pub proximity_out: extern "C" fn(data: *mut c_void, tool: *mut zwp_tablet_tool_v2),
    pub down: extern "C" fn(data: *mut c_void, tool: *mut zwp_tablet_tool_v2, serial: u32),
    pub up: extern "C" fn(data: *mut c_void, tool: *mut zwp_tablet_tool_v2),
    pub motion: extern "C" fn(data: *mut c_void, tool: *mut zwp_tablet_tool_v2, x: i32, y: i32),
    pub pressure: extern "C" fn(data: *mut c_void, tool: *mut zwp_tablet_tool_v2, pressure: u32),
    pub distance: extern "C" fn(data: *mut c_void, tool: *mut zwp_tablet_tool_v2, distance: u32),
    pub tilt:
        extern "C" fn(data: *mut c_void, tool: *mut zwp_tablet_tool_v2, tilt_x: i32, tilt_y: i32),
    pub rotation: extern "C" fn(data: *mut c_void, tool: *mut zwp_tablet_tool_v2, degrees: i32),
    pub slider: extern "C" fn(data: *mut c_void, tool: *mut zwp_tablet_tool_v2, position: i32),
    pub wheel:
        extern "C" fn(data: *mut c_void, tool: *mut zwp_tablet_tool_v2, degrees: i32, clicks: i32),
    pub button: extern "C" fn(
        data: *mut c_void,
        tool: *mut zwp_tablet_tool_v2,
        serial: u32,
        button: u32,
        state: u32,
    ),
    pub frame: extern "C" fn(data: *mut c_void, tool: *mut zwp_tablet_tool_v2, time: u32),
}

fn leak_null_types() -> *const *const wl_interface {
    let a: &'static [*const wl_interface; 8] = Box::leak(Box::new([std::ptr::null(); 8]));
    a.as_ptr()
}
fn leak_one_type(i: &'static wl_interface) -> *const *const wl_interface {
    let a: &'static [*const wl_interface; 1] = Box::leak(Box::new([i as *const _]));
    a.as_ptr()
}

pub fn get_tablet_pad_ring_v2_interface() -> &'static wl_interface {
    use std::sync::OnceLock;
    static I: OnceLock<SyncInterface> = OnceLock::new();
    I.get_or_init(|| SyncInterface({
        let n = leak_null_types();
        let requests: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"set_feedback\0".as_ptr() as _, signature: b"su\0".as_ptr() as _, types: n },
            wl_message { name: b"destroy\0".as_ptr() as _, signature: b"\0".as_ptr() as _, types: n },
        ]));
        let events: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"source\0".as_ptr() as _, signature: b"u\0".as_ptr() as _, types: n },
            wl_message { name: b"angle\0".as_ptr() as _, signature: b"f\0".as_ptr() as _, types: n },
            wl_message { name: b"stop\0".as_ptr() as _, signature: b"\0".as_ptr() as _, types: n },
            wl_message { name: b"frame\0".as_ptr() as _, signature: b"u\0".as_ptr() as _, types: n },
        ]));
        Box::leak(Box::new(wl_interface { name: b"zwp_tablet_pad_ring_v2\0".as_ptr() as _, version: 2, method_count: 2, methods: requests.as_ptr(), event_count: 4, events: events.as_ptr() }))
    })).0
}
pub fn get_tablet_pad_strip_v2_interface() -> &'static wl_interface {
    use std::sync::OnceLock;
    static I: OnceLock<SyncInterface> = OnceLock::new();
    I.get_or_init(|| SyncInterface({
        let n = leak_null_types();
        let requests: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"set_feedback\0".as_ptr() as _, signature: b"su\0".as_ptr() as _, types: n },
            wl_message { name: b"destroy\0".as_ptr() as _, signature: b"\0".as_ptr() as _, types: n },
        ]));
        let events: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"source\0".as_ptr() as _, signature: b"u\0".as_ptr() as _, types: n },
            wl_message { name: b"position\0".as_ptr() as _, signature: b"u\0".as_ptr() as _, types: n },
            wl_message { name: b"stop\0".as_ptr() as _, signature: b"\0".as_ptr() as _, types: n },
            wl_message { name: b"frame\0".as_ptr() as _, signature: b"u\0".as_ptr() as _, types: n },
        ]));
        Box::leak(Box::new(wl_interface { name: b"zwp_tablet_pad_strip_v2\0".as_ptr() as _, version: 2, method_count: 2, methods: requests.as_ptr(), event_count: 4, events: events.as_ptr() }))
    })).0
}
pub fn get_tablet_pad_dial_v2_interface() -> &'static wl_interface {
    use std::sync::OnceLock;
    static I: OnceLock<SyncInterface> = OnceLock::new();
    I.get_or_init(|| SyncInterface({
        let n = leak_null_types();
        let requests: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"set_feedback\0".as_ptr() as _, signature: b"su\0".as_ptr() as _, types: n },
            wl_message { name: b"destroy\0".as_ptr() as _, signature: b"\0".as_ptr() as _, types: n },
        ]));
        let events: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"delta\0".as_ptr() as _, signature: b"i\0".as_ptr() as _, types: n },
            wl_message { name: b"frame\0".as_ptr() as _, signature: b"u\0".as_ptr() as _, types: n },
        ]));
        Box::leak(Box::new(wl_interface { name: b"zwp_tablet_pad_dial_v2\0".as_ptr() as _, version: 2, method_count: 2, methods: requests.as_ptr(), event_count: 2, events: events.as_ptr() }))
    })).0
}
pub fn get_tablet_pad_group_v2_interface() -> &'static wl_interface {
    use std::sync::OnceLock;
    static I: OnceLock<SyncInterface> = OnceLock::new();
    I.get_or_init(|| SyncInterface({
        let n = leak_null_types();
        let requests: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"destroy\0".as_ptr() as _, signature: b"\0".as_ptr() as _, types: n },
        ]));
        let events: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"buttons\0".as_ptr() as _, signature: b"a\0".as_ptr() as _, types: n },
            wl_message { name: b"ring\0".as_ptr() as _, signature: b"n\0".as_ptr() as _, types: leak_one_type(get_tablet_pad_ring_v2_interface()) },
            wl_message { name: b"strip\0".as_ptr() as _, signature: b"n\0".as_ptr() as _, types: leak_one_type(get_tablet_pad_strip_v2_interface()) },
            wl_message { name: b"modes\0".as_ptr() as _, signature: b"u\0".as_ptr() as _, types: n },
            wl_message { name: b"done\0".as_ptr() as _, signature: b"\0".as_ptr() as _, types: n },
            wl_message { name: b"mode_switch\0".as_ptr() as _, signature: b"uuu\0".as_ptr() as _, types: n },
            wl_message { name: b"dial\0".as_ptr() as _, signature: b"2n\0".as_ptr() as _, types: leak_one_type(get_tablet_pad_dial_v2_interface()) },
        ]));
        Box::leak(Box::new(wl_interface { name: b"zwp_tablet_pad_group_v2\0".as_ptr() as _, version: 2, method_count: 1, methods: requests.as_ptr(), event_count: 7, events: events.as_ptr() }))
    })).0
}
pub fn get_tablet_pad_v2_interface() -> &'static wl_interface {
    use std::sync::OnceLock;
    static I: OnceLock<SyncInterface> = OnceLock::new();
    I.get_or_init(|| SyncInterface({
        let n = leak_null_types();
        let requests: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"set_feedback\0".as_ptr() as _, signature: b"usu\0".as_ptr() as _, types: n },
            wl_message { name: b"destroy\0".as_ptr() as _, signature: b"\0".as_ptr() as _, types: n },
        ]));
        let events: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"group\0".as_ptr() as _, signature: b"n\0".as_ptr() as _, types: leak_one_type(get_tablet_pad_group_v2_interface()) },
            wl_message { name: b"path\0".as_ptr() as _, signature: b"s\0".as_ptr() as _, types: n },
            wl_message { name: b"buttons\0".as_ptr() as _, signature: b"u\0".as_ptr() as _, types: n },
            wl_message { name: b"done\0".as_ptr() as _, signature: b"\0".as_ptr() as _, types: n },
            wl_message { name: b"button\0".as_ptr() as _, signature: b"uuu\0".as_ptr() as _, types: n },
            wl_message { name: b"enter\0".as_ptr() as _, signature: b"uoo\0".as_ptr() as _, types: n },
            wl_message { name: b"leave\0".as_ptr() as _, signature: b"uo\0".as_ptr() as _, types: n },
            wl_message { name: b"removed\0".as_ptr() as _, signature: b"\0".as_ptr() as _, types: n },
        ]));
        Box::leak(Box::new(wl_interface { name: b"zwp_tablet_pad_v2\0".as_ptr() as _, version: 2, method_count: 2, methods: requests.as_ptr(), event_count: 8, events: events.as_ptr() }))
    })).0
}
pub fn get_tablet_v2_interface() -> &'static wl_interface {
    use std::sync::OnceLock;
    static I: OnceLock<SyncInterface> = OnceLock::new();
    I.get_or_init(|| SyncInterface({
        let n = leak_null_types();
        let requests: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"destroy\0".as_ptr() as _, signature: b"\0".as_ptr() as _, types: n },
        ]));
        let events: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"name\0".as_ptr() as _, signature: b"s\0".as_ptr() as _, types: n },
            wl_message { name: b"id\0".as_ptr() as _, signature: b"uu\0".as_ptr() as _, types: n },
            wl_message { name: b"path\0".as_ptr() as _, signature: b"s\0".as_ptr() as _, types: n },
            wl_message { name: b"done\0".as_ptr() as _, signature: b"\0".as_ptr() as _, types: n },
            wl_message { name: b"removed\0".as_ptr() as _, signature: b"\0".as_ptr() as _, types: n },
            wl_message { name: b"bustype\0".as_ptr() as _, signature: b"2u\0".as_ptr() as _, types: n },
        ]));
        Box::leak(Box::new(wl_interface { name: b"zwp_tablet_v2\0".as_ptr() as _, version: 2, method_count: 1, methods: requests.as_ptr(), event_count: 6, events: events.as_ptr() }))
    })).0
}
pub fn get_tablet_tool_v2_interface() -> &'static wl_interface {
    use std::sync::OnceLock;
    static I: OnceLock<SyncInterface> = OnceLock::new();
    I.get_or_init(|| SyncInterface({
        let n = leak_null_types();
        let requests: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"set_cursor\0".as_ptr() as _, signature: b"u?oii\0".as_ptr() as _, types: n },
            wl_message { name: b"destroy\0".as_ptr() as _, signature: b"\0".as_ptr() as _, types: n },
        ]));
        let events: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"type\0".as_ptr() as _, signature: b"u\0".as_ptr() as _, types: n },
            wl_message { name: b"hardware_serial\0".as_ptr() as _, signature: b"uu\0".as_ptr() as _, types: n },
            wl_message { name: b"hardware_id_wacom\0".as_ptr() as _, signature: b"uu\0".as_ptr() as _, types: n },
            wl_message { name: b"capability\0".as_ptr() as _, signature: b"u\0".as_ptr() as _, types: n },
            wl_message { name: b"done\0".as_ptr() as _, signature: b"\0".as_ptr() as _, types: n },
            wl_message { name: b"removed\0".as_ptr() as _, signature: b"\0".as_ptr() as _, types: n },
            wl_message { name: b"proximity_in\0".as_ptr() as _, signature: b"uoo\0".as_ptr() as _, types: n },
            wl_message { name: b"proximity_out\0".as_ptr() as _, signature: b"\0".as_ptr() as _, types: n },
            wl_message { name: b"down\0".as_ptr() as _, signature: b"u\0".as_ptr() as _, types: n },
            wl_message { name: b"up\0".as_ptr() as _, signature: b"\0".as_ptr() as _, types: n },
            wl_message { name: b"motion\0".as_ptr() as _, signature: b"ff\0".as_ptr() as _, types: n },
            wl_message { name: b"pressure\0".as_ptr() as _, signature: b"u\0".as_ptr() as _, types: n },
            wl_message { name: b"distance\0".as_ptr() as _, signature: b"u\0".as_ptr() as _, types: n },
            wl_message { name: b"tilt\0".as_ptr() as _, signature: b"ff\0".as_ptr() as _, types: n },
            wl_message { name: b"rotation\0".as_ptr() as _, signature: b"f\0".as_ptr() as _, types: n },
            wl_message { name: b"slider\0".as_ptr() as _, signature: b"i\0".as_ptr() as _, types: n },
            wl_message { name: b"wheel\0".as_ptr() as _, signature: b"fi\0".as_ptr() as _, types: n },
            wl_message { name: b"button\0".as_ptr() as _, signature: b"uuu\0".as_ptr() as _, types: n },
            wl_message { name: b"frame\0".as_ptr() as _, signature: b"u\0".as_ptr() as _, types: n },
        ]));
        Box::leak(Box::new(wl_interface { name: b"zwp_tablet_tool_v2\0".as_ptr() as _, version: 2, method_count: 2, methods: requests.as_ptr(), event_count: 19, events: events.as_ptr() }))
    })).0
}
pub fn get_tablet_seat_v2_interface() -> &'static wl_interface {
    use std::sync::OnceLock;
    static I: OnceLock<SyncInterface> = OnceLock::new();
    I.get_or_init(|| SyncInterface({
        let n = leak_null_types();
        let requests: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"destroy\0".as_ptr() as _, signature: b"\0".as_ptr() as _, types: n },
        ]));
        let events: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"tablet_added\0".as_ptr() as _, signature: b"n\0".as_ptr() as _, types: leak_one_type(get_tablet_v2_interface()) },
            wl_message { name: b"tool_added\0".as_ptr() as _, signature: b"n\0".as_ptr() as _, types: leak_one_type(get_tablet_tool_v2_interface()) },
            wl_message { name: b"pad_added\0".as_ptr() as _, signature: b"n\0".as_ptr() as _, types: leak_one_type(get_tablet_pad_v2_interface()) },
        ]));
        Box::leak(Box::new(wl_interface { name: b"zwp_tablet_seat_v2\0".as_ptr() as _, version: 2, method_count: 1, methods: requests.as_ptr(), event_count: 3, events: events.as_ptr() }))
    })).0
}
pub fn get_tablet_manager_v2_interface() -> &'static wl_interface {
    use std::sync::OnceLock;
    static I: OnceLock<SyncInterface> = OnceLock::new();
    I.get_or_init(|| SyncInterface({
        let n = leak_null_types();
        let requests: &'static [wl_message] = Box::leak(Box::new([
            wl_message { name: b"get_tablet_seat\0".as_ptr() as _, signature: b"no\0".as_ptr() as _, types: n },
            wl_message { name: b"destroy\0".as_ptr() as _, signature: b"\0".as_ptr() as _, types: n },
        ]));
        Box::leak(Box::new(wl_interface { name: b"zwp_tablet_manager_v2\0".as_ptr() as _, version: 2, method_count: 2, methods: requests.as_ptr(), event_count: 0, events: std::ptr::null() }))
    })).0
}

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
