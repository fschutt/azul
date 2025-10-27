//! Wayland event handling and IME support.

use std::{
    ffi::{c_void, CStr},
    os::unix::io::FromRawFd,
};

use azul_core::{
    dom::DomId,
    events::{EventFilter, MouseButton, ProcessEventResult},
    geom::LogicalPosition,
    hit_test::FullHitTest,
    window::{CursorPosition, VirtualKeyCode},
};

use super::{defines::*, WaylandWindow};
use crate::desktop::shell2::common::window::PlatformWindow;

// -- State for input devices --

pub(super) struct KeyboardState {
    pub(super) context: *mut xkb_context,
    pub(super) keymap: *mut xkb_keymap,
    pub(super) state: *mut xkb_state,
}

impl KeyboardState {
    pub(super) fn new() -> Self {
        Self {
            context: std::ptr::null_mut(),
            keymap: std::ptr::null_mut(),
            state: std::ptr::null_mut(),
        }
    }
}

pub(super) struct PointerState {
    /// The serial of the last pointer event, used for requests like popups or moves.
    pub(super) serial: u32,
    /// Tracks which button was pressed down to distinguish clicks from drags.
    pub(super) button_down: Option<MouseButton>,
}

impl PointerState {
    pub(super) fn new() -> Self {
        Self {
            serial: 0,
            button_down: None,
        }
    }
}

// -- Listener Implementations --

extern "C" fn xdg_wm_base_ping_handler(data: *mut c_void, shell: *mut xdg_wm_base, serial: u32) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    unsafe { (window.wayland.xdg_wm_base_pong)(shell, serial) };
}

// wl_registry listener
pub(super) extern "C" fn registry_global_handler(
    data: *mut c_void,
    registry: *mut wl_registry,
    name: u32,
    interface: *const i8,
    version: u32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    let interface_str = unsafe { CStr::from_ptr(interface).to_str().unwrap() };

    match interface_str {
        "wl_compositor" => {
            window.compositor = unsafe {
                (window.wayland.wl_registry_bind)(
                    registry,
                    name,
                    &window.wayland.wl_compositor_interface,
                    version.min(4),
                ) as *mut _
            };
        }
        "wl_shm" => {
            window.shm = unsafe {
                (window.wayland.wl_registry_bind)(
                    registry,
                    name,
                    &window.wayland.wl_shm_interface,
                    1,
                ) as *mut _
            };
        }
        "xdg_wm_base" => {
            window.xdg_wm_base = unsafe {
                (window.wayland.wl_registry_bind)(
                    registry,
                    name,
                    &window.wayland.xdg_wm_base_interface,
                    1,
                ) as *mut _
            };
            let listener = xdg_wm_base_listener {
                ping: xdg_wm_base_ping_handler,
            };
            unsafe {
                (window.wayland.xdg_wm_base_add_listener)(window.xdg_wm_base, &listener, data)
            };
        }
        "wl_seat" => {
            let seat = unsafe {
                (window.wayland.wl_registry_bind)(
                    registry,
                    name,
                    &window.wayland.wl_seat_interface,
                    version.min(7),
                ) as *mut wl_seat
            };
            window.seat = seat;
            let listener = wl_seat_listener {
                capabilities: seat_capabilities_handler,
                name: seat_name_handler,
            };
            unsafe { (window.wayland.wl_seat_add_listener)(seat, &listener, data) };
        }
        _ => {}
    }
}

pub(super) extern "C" fn registry_global_remove_handler(
    _data: *mut c_void,
    _registry: *mut wl_registry,
    _name: u32,
) {
}

// wl_seat listener
pub(super) extern "C" fn seat_capabilities_handler(
    data: *mut c_void,
    seat: *mut wl_seat,
    capabilities: u32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };

    if capabilities & WL_SEAT_CAPABILITY_POINTER != 0 {
        let pointer = unsafe { (window.wayland.wl_seat_get_pointer)(seat) };
        let listener = wl_pointer_listener {
            enter: pointer_enter_handler,
            leave: pointer_leave_handler,
            motion: pointer_motion_handler,
            button: pointer_button_handler,
            axis: pointer_axis_handler,
            frame: pointer_frame_handler,
            axis_source: pointer_axis_source_handler,
            axis_stop: pointer_axis_stop_handler,
            axis_discrete: pointer_axis_discrete_handler,
        };
        unsafe { (window.wayland.wl_pointer_add_listener)(pointer, &listener, data) };
    }

    if capabilities & WL_SEAT_CAPABILITY_KEYBOARD != 0 {
        let keyboard = unsafe { (window.wayland.wl_seat_get_keyboard)(seat) };
        let listener = wl_keyboard_listener {
            keymap: keyboard_keymap_handler,
            enter: keyboard_enter_handler,
            leave: keyboard_leave_handler,
            key: keyboard_key_handler,
            modifiers: keyboard_modifiers_handler,
            repeat_info: keyboard_repeat_info_handler,
        };
        unsafe { (window.wayland.wl_keyboard_add_listener)(keyboard, &listener, data) };
    }
}

pub(super) extern "C" fn seat_name_handler(
    _data: *mut c_void,
    _seat: *mut wl_seat,
    _name: *const i8,
) {
}

// wl_keyboard listener
pub(super) extern "C" fn keyboard_keymap_handler(
    data: *mut c_void,
    _keyboard: *mut wl_keyboard,
    format: u32,
    fd: i32,
    _size: u32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    if format != WL_KEYBOARD_KEYMAP_FORMAT_XKB_V1 {
        unsafe { libc::close(fd) };
        return;
    }

    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    let mut string = String::new();
    use std::io::Read;
    if file.read_to_string(&mut string).is_err() {
        return;
    }

    window.keyboard_state.context = unsafe { (window.xkb.xkb_context_new)(XKB_CONTEXT_NO_FLAGS) };
    let c_string = std::ffi::CString::new(string).unwrap();

    window.keyboard_state.keymap = unsafe {
        (window.xkb.xkb_keymap_new_from_string)(
            window.keyboard_state.context,
            c_string.as_ptr(),
            XKB_KEYMAP_FORMAT_TEXT_V1,
            XKB_KEYMAP_COMPILE_NO_FLAGS,
        )
    };
    window.keyboard_state.state =
        unsafe { (window.xkb.xkb_state_new)(window.keyboard_state.keymap) };
}

pub(super) extern "C" fn keyboard_key_handler(
    data: *mut c_void,
    _keyboard: *mut wl_keyboard,
    _serial: u32,
    _time: u32,
    key: u32,
    state: u32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.handle_key(key, state);
}

pub(super) extern "C" fn keyboard_modifiers_handler(
    data: *mut c_void,
    _keyboard: *mut wl_keyboard,
    _serial: u32,
    mods_depressed: u32,
    mods_latched: u32,
    mods_locked: u32,
    group: u32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    unsafe {
        (window.xkb.xkb_state_update_mask)(
            window.keyboard_state.state,
            mods_depressed,
            mods_latched,
            mods_locked,
            0,
            0,
            group,
        )
    };
}

// xdg_surface listener
pub(super) extern "C" fn xdg_surface_configure_handler(
    data: *mut c_void,
    xdg_surface: *mut xdg_surface,
    serial: u32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    unsafe { (window.wayland.xdg_surface_ack_configure)(xdg_surface, serial) };
    window.configured = true;
    window.request_redraw();
}

// wl_pointer listeners
pub(super) extern "C" fn pointer_enter_handler(
    data: *mut c_void,
    _pointer: *mut wl_pointer,
    serial: u32,
    _surface: *mut wl_surface,
    surface_x: f64,
    surface_y: f64,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.handle_pointer_enter(serial, surface_x, surface_y);
}

pub(super) extern "C" fn pointer_leave_handler(
    data: *mut c_void,
    _pointer: *mut wl_pointer,
    serial: u32,
    _surface: *mut wl_surface,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.handle_pointer_leave(serial);
}

pub(super) extern "C" fn pointer_motion_handler(
    data: *mut c_void,
    _pointer: *mut wl_pointer,
    _time: u32,
    surface_x: f64,
    surface_y: f64,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.handle_pointer_motion(surface_x, surface_y);
}

pub(super) extern "C" fn pointer_button_handler(
    data: *mut c_void,
    _pointer: *mut wl_pointer,
    serial: u32,
    _time: u32,
    button: u32,
    state: u32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.handle_pointer_button(serial, button, state);
}

pub(super) extern "C" fn pointer_axis_handler(
    data: *mut c_void,
    _pointer: *mut wl_pointer,
    _time: u32,
    axis: u32,
    value: f64,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.handle_pointer_axis(axis, value);
}

// Stub handlers for unused pointer events
extern "C" fn pointer_frame_handler(_data: *mut c_void, _pointer: *mut wl_pointer) {}
extern "C" fn pointer_axis_source_handler(
    _data: *mut c_void,
    _pointer: *mut wl_pointer,
    _axis_source: u32,
) {
}
extern "C" fn pointer_axis_stop_handler(
    _data: *mut c_void,
    _pointer: *mut wl_pointer,
    _time: u32,
    _axis: u32,
) {
}
extern "C" fn pointer_axis_discrete_handler(
    _data: *mut c_void,
    _pointer: *mut wl_pointer,
    _axis: u32,
    _discrete: i32,
) {
}

// Stub handlers for unused keyboard events
extern "C" fn keyboard_enter_handler(
    _data: *mut c_void,
    _keyboard: *mut wl_keyboard,
    _serial: u32,
    _surface: *mut wl_surface,
    _keys: *mut c_void,
) {
}
extern "C" fn keyboard_leave_handler(
    _data: *mut c_void,
    _keyboard: *mut wl_keyboard,
    _serial: u32,
    _surface: *mut wl_surface,
) {
}
extern "C" fn keyboard_repeat_info_handler(
    _data: *mut c_void,
    _keyboard: *mut wl_keyboard,
    _rate: i32,
    _delay: i32,
) {
}

/// Keycode translation from XKB keysym to Azul VirtualKeyCode
pub(super) fn keysym_to_virtual_keycode(keysym: xkb_keysym_t) -> Option<VirtualKeyCode> {
    // Re-use the X11 keysym mapping as they are identical
    use super::super::x11::events::keysym_to_virtual_keycode as x11_map;
    x11_map(keysym as u64)
}
