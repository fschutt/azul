//! Wayland event handling and IME support.

use super::{defines::*, WaylandWindow};
use azul_core::{
    dom::DomId,
    events::{EventFilter, MouseButton, ProcessEventResult},
    geom::LogicalPosition,
    hit_test::FullHitTest,
    window::{CursorPosition, VirtualKeyCode},
};
use std::ffi::{c_void, CStr};
use std::os::unix::io::FromRawFd;

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
                    &wl_compositor_interface,
                    version.min(4),
                ) as *mut _
            };
        }
        "wl_shm" => {
             window.shm = unsafe {
                (window.wayland.wl_registry_bind)(
                    registry,
                    name,
                    &wl_shm_interface,
                    1,
                ) as *mut _
            };
        }
        "xdg_wm_base" => {
             window.xdg_wm_base = unsafe {
                (window.wayland.wl_registry_bind)(
                    registry,
                    name,
                    &xdg_wm_base_interface,
                    1,
                ) as *mut _
            };
             let listener = xdg_wm_base_listener { ping: |data, shell, serial| {
                let window = unsafe { &mut *(data as *mut WaylandWindow) };
                unsafe { (window.wayland.xdg_wm_base_pong)(shell, serial) };
             }};
             unsafe { (window.wayland.xdg_wm_base_add_listener)(window.xdg_wm_base, &listener, data) };
        }
        "wl_seat" => {
            let seat = unsafe {
                (window.wayland.wl_registry_bind)(
                    registry,
                    name,
                    &wl_seat_interface,
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

pub(super) extern "C" fn registry_global_remove_handler(_data: *mut c_void, _registry: *mut wl_registry, _name: u32) {}

// wl_seat listener
pub(super) extern "C" fn seat_capabilities_handler(data: *mut c_void, seat: *mut wl_seat, capabilities: u32) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };

    if capabilities & WL_SEAT_CAPABILITY_POINTER != 0 {
        let pointer = unsafe { (window.wayland.wl_seat_get_pointer)(seat) };
        let listener = wl_pointer_listener {
            enter: pointer_enter_handler,
            leave: pointer_leave_handler,
            motion: pointer_motion_handler,
            button: pointer_button_handler,
            axis: pointer_axis_handler,
            frame: |_, _| {},
            axis_source: |_, _| {},
            axis_stop: |_, _, _| {},
            axis_discrete: |_, _, _| {},
        };
        unsafe { (window.wayland.wl_pointer_add_listener)(pointer, &listener, data) };
    }

    if capabilities & WL_SEAT_CAPABILITY_KEYBOARD != 0 {
        let keyboard = unsafe { (window.wayland.wl_seat_get_keyboard)(seat) };
        let listener = wl_keyboard_listener {
            keymap: keyboard_keymap_handler,
            enter: |_, _, _, _, _| {},
            leave: |_, _, _, _| {},
            key: keyboard_key_handler,
            modifiers: keyboard_modifiers_handler,
            repeat_info: |_, _, _, _| {},
        };
        unsafe { (window.wayland.wl_keyboard_add_listener)(keyboard, &listener, data) };
    }
}

pub(super) extern "C" fn seat_name_handler(_data: *mut c_void, _seat: *mut wl_seat, _name: *const i8) {}

// wl_keyboard listener
pub(super) extern "C" fn keyboard_keymap_handler(data: *mut c_void, _keyboard: *mut wl_keyboard, format: u32, fd: i32, _size: u32) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    if format != WL_KEYBOARD_KEYMAP_FORMAT_XKB_V1 {
        unsafe { libc::close(fd) };
        return;
    }

    let mut file = unsafe { std::fs::File::from_raw_fd(fd) };
    let mut string = String::new();
    use std::io::Read;
    if file.read_to_string(&mut string).is_err() { return; }

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
    window.keyboard_state.state = unsafe { (window.xkb.xkb_state_new)(window.keyboard_state.keymap) };
}

pub(super) extern "C" fn keyboard_key_handler(data: *mut c_void, _keyboard: *mut wl_keyboard, _serial: u32, _time: u32, key: u32, state: u32) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.handle_key(key, state);
}

pub(super) extern "C" fn keyboard_modifiers_handler(data: *mut c_void, _keyboard: *mut wl_keyboard, _serial: u32, mods_depressed: u32, mods_latched: u32, mods_locked: u32, group: u32) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    unsafe { (window.xkb.xkb_state_update_mask)(window.keyboard_state.state, mods_depressed, mods_latched, mods_locked, 0, 0, group) };
}

// xdg_surface listener
pub(super) extern "C" fn xdg_surface_configure_handler(data: *mut c_void, xdg_surface: *mut xdg_surface, serial: u32) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    unsafe { (window.wayland.xdg_surface_ack_configure)(xdg_surface, serial) };
    window.request_redraw();
}

// wl_pointer listeners
pub(super) extern "C" fn pointer_enter_handler(data: *mut c_void, _pointer: *mut wl_pointer, serial: u32, _surface: *mut wl_surface, surface_x: f64, surface_y: f64) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.handle_pointer_enter(serial, surface_x, surface_y);
}

pub(super) extern "C" fn pointer_leave_handler(data: *mut c_void, _pointer: *mut wl_pointer, serial: u32, _surface: *mut wl_surface) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.handle_pointer_leave(serial);
}

pub(super) extern "C" fn pointer_motion_handler(data: *mut c_void, _pointer: *mut wl_pointer, _time: u32, surface_x: f64, surface_y: f64) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.handle_pointer_motion(surface_x, surface_y);
}

pub(super) extern "C" fn pointer_button_handler(data: *mut c_void, _pointer: *mut wl_pointer, serial: u32, _time: u32, button: u32, state: u32) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.handle_pointer_button(serial, button, state);
}

pub(super) extern "C" fn pointer_axis_handler(data: *mut c_void, _pointer: *mut wl_pointer, _time: u32, axis: u32, value: f64) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.handle_pointer_axis(axis, value);
}

/// Keycode translation from XKB keysym to Azul VirtualKeyCode
pub(super) fn keysym_to_virtual_keycode(keysym: xkb_keysym_t) -> Option<VirtualKeyCode> {
    use azul_core::window::VirtualKeyCode::*;
    match keysym {
        0xff08 => Some(Back), 0xff09 => Some(Tab), 0xff0d => Some(Return),
        0xff13 => Some(Pause), 0xff14 => Some(Scroll), 0xff1b => Some(Escape),
        0xff50 => Some(Home), 0xff51 => Some(Left), 0xff52 => Some(Up),
        0xff53 => Some(Right), 0xff54 => Some(Down), 0xff55 => Some(PageUp),
        0xff56 => Some(PageDown), 0xff57 => Some(End), 0xff63 => Some(Insert),
        0xffff => Some(Delete), 0x0020 => Some(Space),
        0x0030..=0x0039 => Some(unsafe { std::mem::transmute((keysym - 0x0030) as u8 + Key0 as u8) }),
        0x0061..=0x007a => Some(unsafe { std::mem::transmute((keysym - 0x0061) as u8 + A as u8) }),
        0x0041..=0x005a => Some(unsafe { std::mem::transmute((keysym - 0x0041) as u8 + A as u8) }),
        0xffbe..=0xffcf => Some(unsafe { std::mem::transmute((keysym - 0xffbe) as u8 + F1 as u8) }),
        0xffe1 => Some(LShift), 0xffe2 => Some(RShift),
        0xffe3 => Some(LControl), 0xffe4 => Some(RControl),
        0xffe9 => Some(LAlt), 0xffea => Some(RAlt),
        0xffeb => Some(LWin), 0xffec => Some(RWin),
        _ => None,
    }
}