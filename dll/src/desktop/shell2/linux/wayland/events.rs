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
    window::{CursorPosition, VirtualKeyCode, WindowFrame},
};

use super::{defines::*, WaylandWindow};
use crate::desktop::shell2::common::window::PlatformWindow;

use crate::{log_debug, log_error, log_info, log_warn, log_trace};
use super::super::super::common::debug_server::LogCategory;

// -- State for input devices --

pub(super) struct WaylandKeyboardState {
    pub(super) context: *mut xkb_context,
    pub(super) keymap: *mut xkb_keymap,
    pub(super) state: *mut xkb_state,
}

impl WaylandKeyboardState {
    pub(super) fn new() -> Self {
        Self {
            context: std::ptr::null_mut(),
            keymap: std::ptr::null_mut(),
            state: std::ptr::null_mut(),
        }
    }
}

pub(super) struct PointerState {
    /// The wl_pointer object from Wayland
    pub(super) pointer: *mut super::defines::wl_pointer,
    /// The serial of the last pointer event, used for requests like popups or moves.
    pub(super) serial: u32,
    /// Tracks which button was pressed down to distinguish clicks from drags.
    pub(super) button_down: Option<MouseButton>,
    /// Current cursor theme (loaded once)
    pub(super) cursor_theme: *mut super::defines::wl_cursor_theme,
    /// Dedicated surface for cursor (reused instead of creating/destroying)
    pub(super) cursor_surface: *mut super::defines::wl_surface,
}

impl PointerState {
    pub(super) fn new() -> Self {
        Self {
            pointer: std::ptr::null_mut(),
            serial: 0,
            button_down: None,
            cursor_theme: std::ptr::null_mut(),
            cursor_surface: std::ptr::null_mut(),
        }
    }
}

// -- Listener Implementations --

// wl_output listener handlers
extern "C" fn wl_output_geometry_handler(
    data: *mut c_void,
    output: *mut wl_output,
    x: i32,
    y: i32,
    _physical_width: i32,
    _physical_height: i32,
    _subpixel: i32,
    make: *const i8,
    model: *const i8,
    _transform: i32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };

    // Find the MonitorState for this output
    if let Some(monitor) = window.known_outputs.iter_mut().find(|m| m.proxy == output) {
        monitor.x = x;
        monitor.y = y;

        if !make.is_null() {
            if let Ok(make_str) = unsafe { CStr::from_ptr(make).to_str() } {
                monitor.make = make_str.to_string();
            }
        }

        if !model.is_null() {
            if let Ok(model_str) = unsafe { CStr::from_ptr(model).to_str() } {
                monitor.model = model_str.to_string();
            }
        }
    }
}

extern "C" fn wl_output_mode_handler(
    data: *mut c_void,
    output: *mut wl_output,
    _flags: u32,
    width: i32,
    height: i32,
    _refresh: i32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };

    // Find the MonitorState for this output and update dimensions
    if let Some(monitor) = window.known_outputs.iter_mut().find(|m| m.proxy == output) {
        monitor.width = width;
        monitor.height = height;
    }
}

extern "C" fn wl_output_done_handler(_data: *mut c_void, _output: *mut wl_output) {
    // This event marks the end of a set of events for this output.
    // In our implementation, we update fields incrementally, so no action needed here.
}

extern "C" fn wl_output_scale_handler(data: *mut c_void, output: *mut wl_output, factor: i32) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };

    // Find the MonitorState for this output and update scale
    if let Some(monitor) = window.known_outputs.iter_mut().find(|m| m.proxy == output) {
        monitor.scale = factor;
    }
}

// wl_surface listener handlers
pub(super) extern "C" fn wl_surface_enter_handler(
    data: *mut c_void,
    _surface: *mut wl_surface,
    output: *mut wl_output,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };

    // Add this output to current_outputs if not already present
    if !window.current_outputs.contains(&output) {
        window.current_outputs.push(output);
    }

    // Check if scale factor changed (entered monitor with different DPI)
    let new_scale = window.calculate_current_scale_factor();
    let old_dpi = window.current_window_state.size.dpi;
    let new_dpi = (new_scale * 96.0) as u32;

    // Only regenerate if DPI changed significantly
    if (new_dpi as i32 - old_dpi as i32).abs() > 1 {
        log_info!(LogCategory::Window,
            "[Wayland DPI Change] {} -> {} (entered new monitor)",
            old_dpi, new_dpi
        );
        window.current_window_state.size.dpi = new_dpi;
        window.frame_needs_regeneration = true;
    }
}

pub(super) extern "C" fn wl_surface_leave_handler(
    data: *mut c_void,
    _surface: *mut wl_surface,
    output: *mut wl_output,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };

    // Remove this output from current_outputs
    window.current_outputs.retain(|&o| o != output);

    // Check if scale factor changed (left monitor, now on different monitor)
    let new_scale = window.calculate_current_scale_factor();
    let old_dpi = window.current_window_state.size.dpi;
    let new_dpi = (new_scale * 96.0) as u32;

    // Only regenerate if DPI changed significantly
    if (new_dpi as i32 - old_dpi as i32).abs() > 1 {
        log_info!(LogCategory::Window,
            "[Wayland DPI Change] {} -> {} (left monitor)",
            old_dpi, new_dpi
        );
        window.current_window_state.size.dpi = new_dpi;
        window.frame_needs_regeneration = true;
    }
}

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
        "wl_subcompositor" => {
            let subcompositor = unsafe {
                (window.wayland.wl_registry_bind)(
                    registry,
                    name,
                    &window.wayland.wl_subcompositor_interface,
                    1,
                ) as *mut _
            };
            window.subcompositor = Some(subcompositor);
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
        "wl_output" => {
            let output = unsafe {
                (window.wayland.wl_registry_bind)(
                    registry,
                    name,
                    &window.wayland.wl_output_interface,
                    version.min(3),
                ) as *mut wl_output
            };

            // Add a new MonitorState entry
            use super::MonitorState;
            window.known_outputs.push(MonitorState {
                proxy: output,
                name: format!("output-{}", name),
                scale: 1,
                x: 0,
                y: 0,
                width: 0,
                height: 0,
                make: String::new(),
                model: String::new(),
            });

            let listener = wl_output_listener {
                geometry: wl_output_geometry_handler,
                mode: wl_output_mode_handler,
                done: wl_output_done_handler,
                scale: wl_output_scale_handler,
            };
            unsafe { (window.wayland.wl_output_add_listener)(output, &listener, data) };
        }
        "org_kde_kwin_blur_manager" => {
            // KDE Plasma blur protocol - allows client-requested blur effects
            // We use wl_registry_bind (which is wl_proxy_marshal_constructor transmuted)
            // with a null interface since org_kde_kwin_blur_manager is not in the core protocol
            let blur_manager = unsafe {
                // wl_registry.bind signature: new_id<interface>(name: uint, interface: string, version: uint)
                type WlRegistryBindFn = unsafe extern "C" fn(
                    *mut wl_proxy, u32, *const wl_interface, u32, *const i8, u32, *const std::ffi::c_void
                ) -> *mut wl_proxy;
                let bind_fn: WlRegistryBindFn = std::mem::transmute(window.wayland.wl_proxy_marshal_constructor);
                bind_fn(
                    registry as *mut wl_proxy,
                    0, // wl_registry.bind opcode
                    std::ptr::null(), // No interface definition for KDE extension
                    name,
                    b"org_kde_kwin_blur_manager\0".as_ptr() as *const i8,
                    1u32, // version
                    std::ptr::null::<std::ffi::c_void>(),
                ) as *mut org_kde_kwin_blur_manager
            };
            if !blur_manager.is_null() {
                window.blur_manager = Some(blur_manager);
                crate::log_debug!(
                    LogCategory::Platform,
                    "[Wayland] Bound org_kde_kwin_blur_manager - blur effects available"
                );
            }
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
        window.pointer_state.pointer = pointer;
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

// xdg_toplevel listener handlers
pub(super) extern "C" fn xdg_toplevel_configure_handler(
    data: *mut c_void,
    _xdg_toplevel: *mut xdg_toplevel,
    width: i32,
    height: i32,
    states: *mut wl_array,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };

    // Parse states array to determine window state (maximized, fullscreen, etc.)
    if !states.is_null() {
        let array = unsafe { &*states };
        let states_data = array.data as *const u32;
        let states_count = array.size / std::mem::size_of::<u32>();

        let mut is_maximized = false;
        let mut is_fullscreen = false;
        let mut is_activated = false;

        for i in 0..states_count {
            let state = unsafe { *states_data.add(i) };
            // XDG toplevel states: 1=maximized, 2=fullscreen, 3=resizing, 4=activated
            match state {
                1 => is_maximized = true,
                2 => is_fullscreen = true,
                4 => is_activated = true,
                _ => {}
            }
        }

        window.current_window_state.flags.frame = if is_fullscreen {
            WindowFrame::Fullscreen
        } else if is_maximized {
            WindowFrame::Maximized
        } else {
            WindowFrame::Normal
        };
        let _ = is_activated; // Can be used for focus indication if needed
    }

    // If width/height are non-zero, the compositor is requesting a specific size
    if width > 0 && height > 0 {
        let current_width = window.current_window_state.size.dimensions.width as i32;
        let current_height = window.current_window_state.size.dimensions.height as i32;

        if width != current_width || height != current_height {
            window.current_window_state.size.dimensions.width = width as f32;
            window.current_window_state.size.dimensions.height = height as f32;
            window.frame_needs_regeneration = true;

            // Resize the rendering surface
            window.resize_surface(width, height);
        }
    }
}

pub(super) extern "C" fn xdg_toplevel_close_handler(
    data: *mut c_void,
    _xdg_toplevel: *mut xdg_toplevel,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.is_open = false;
}

pub(super) extern "C" fn xdg_toplevel_configure_bounds_handler(
    _data: *mut c_void,
    _xdg_toplevel: *mut xdg_toplevel,
    _width: i32,
    _height: i32,
) {
    // Optional: could store bounds for future reference
    // This event provides hints about maximum window size
}

pub(super) extern "C" fn xdg_toplevel_wm_capabilities_handler(
    _data: *mut c_void,
    _xdg_toplevel: *mut xdg_toplevel,
    _capabilities: *mut wl_array,
) {
    // Optional: could parse capabilities to know what the compositor supports
    // (e.g., maximize, minimize, fullscreen, window menu)
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
