//! Wayland event handling and IME support.

use std::{
    ffi::{c_char, c_void, CStr},
    os::unix::io::FromRawFd,
};

use azul_core::{
    events::MouseButton,
    window::{VirtualKeyCode, WindowFrame},
};

use super::{defines, defines::*, WaylandWindow};

use super::super::super::common::debug_server::LogCategory;
use super::super::super::common::event::PlatformWindow;
use crate::{log_debug, log_error, log_info, log_trace, log_warn};

// -- State for input devices --

/// XKB keyboard state for translating Wayland key events into keysyms.
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

/// Tracks Wayland pointer (mouse) state including cursor theme and current button.
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

/// Per-frame accumulator for the tablet tool (pen); fed on the tool `frame` event.
#[derive(Default, Clone, Copy)]
pub struct TabletPenPending {
    pub position: azul_core::geom::LogicalPosition,
    pub pressure: f32,
    pub tilt_x: f32,
    pub tilt_y: f32,
    pub rotation: f32,
    pub in_contact: bool,
    pub is_eraser: bool,
    pub tool_id: u64,
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

// -- Static listener tables --
// These must be `static` because wl_proxy_add_listener stores the pointer
// without copying. A stack-local struct would become a dangling pointer.

static XDG_WM_BASE_LISTENER: xdg_wm_base_listener = xdg_wm_base_listener {
    ping: xdg_wm_base_ping_handler,
};

static WL_SEAT_LISTENER: wl_seat_listener = wl_seat_listener {
    capabilities: seat_capabilities_handler,
    name: seat_name_handler,
};

static WL_OUTPUT_LISTENER: wl_output_listener = wl_output_listener {
    geometry: wl_output_geometry_handler,
    mode: wl_output_mode_handler,
    done: wl_output_done_handler,
    scale: wl_output_scale_handler,
};

static WL_POINTER_LISTENER: wl_pointer_listener = wl_pointer_listener {
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

static WL_KEYBOARD_LISTENER: wl_keyboard_listener = wl_keyboard_listener {
    keymap: keyboard_keymap_handler,
    enter: keyboard_enter_handler,
    leave: keyboard_leave_handler,
    key: keyboard_key_handler,
    modifiers: keyboard_modifiers_handler,
    repeat_info: keyboard_repeat_info_handler,
};

static ZWP_TEXT_INPUT_V3_LISTENER: defines::zwp_text_input_v3_listener =
    defines::zwp_text_input_v3_listener {
        enter: text_input_enter_handler,
        leave: text_input_leave_handler,
        preedit_string: text_input_preedit_string_handler,
        commit_string: text_input_commit_string_handler,
        delete_surrounding_text: text_input_delete_surrounding_text_handler,
        done: text_input_done_handler,
    };

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
    make: *const c_char,
    model: *const c_char,
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
    let old_dpi = window.common.current_window_state.size.dpi;
    let new_dpi = (new_scale * 96.0) as u32;

    // Only regenerate if DPI changed significantly
    if (new_dpi as i32 - old_dpi as i32).abs() > 1 {
        log_info!(
            LogCategory::Window,
            "[Wayland DPI Change] {} -> {} (entered new monitor)",
            old_dpi,
            new_dpi
        );
        window.common.current_window_state.size.dpi = new_dpi;
        window.common.frame_needs_regeneration = true;
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
    let old_dpi = window.common.current_window_state.size.dpi;
    let new_dpi = (new_scale * 96.0) as u32;

    // Only regenerate if DPI changed significantly
    if (new_dpi as i32 - old_dpi as i32).abs() > 1 {
        log_info!(
            LogCategory::Window,
            "[Wayland DPI Change] {} -> {} (left monitor)",
            old_dpi,
            new_dpi
        );
        window.common.current_window_state.size.dpi = new_dpi;
        window.common.frame_needs_regeneration = true;
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
    interface: *const c_char,
    version: u32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    let interface_str = unsafe { CStr::from_ptr(interface).to_str().unwrap_or_default() };

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
            unsafe {
                (window.wayland.xdg_wm_base_add_listener)(
                    window.xdg_wm_base,
                    &XDG_WM_BASE_LISTENER,
                    data,
                )
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
            unsafe { (window.wayland.wl_seat_add_listener)(seat, &WL_SEAT_LISTENER, data) };
            unsafe { try_init_tablet(window, data) };
        }
        "zwp_tablet_manager_v2" => {
            window.tablet_manager = unsafe {
                (window.wayland.wl_registry_bind)(
                    registry,
                    name,
                    get_tablet_manager_v2_interface(),
                    version.min(2),
                ) as *mut _
            };
            unsafe { try_init_tablet(window, data) };
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

            unsafe {
                (window.wayland.wl_output_add_listener)(output, &WL_OUTPUT_LISTENER, data)
            };
        }
        "zwp_text_input_manager_v3" => {
            let manager_interface = defines::get_text_input_manager_v3_interface();
            let text_input_interface = defines::get_text_input_v3_interface();

            // Bind via the normal registry path. The previous code transmuted
            // wl_proxy_marshal_constructor and passed `name` as the OPCODE (the
            // registry only has opcode 0 = bind) while omitting the bind-specific
            // string/version arguments -> a malformed `wl_registry.bind`. Use
            // `wl_registry_bind` which marshals the special "usun" bind signature
            // correctly (same fix as the KDE blur-manager bind).
            let manager = unsafe {
                (window.wayland.wl_registry_bind)(
                    registry,
                    name,
                    manager_interface,
                    version.min(1),
                ) as *mut zwp_text_input_manager_v3
            };

            if !manager.is_null() {
                window.text_input_manager = Some(manager);

                // Create text_input instance via get_text_input(seat)
                // Opcode 1 = get_text_input, args: new_id + seat
                if !window.seat.is_null() {
                    // get_text_input(id: new_id<zwp_text_input_v3>, seat: object<wl_seat>),
                    // signature "no". A new_id request needs the NULL new_id placeholder
                    // BEFORE the object in the marshalled varargs (libwayland's own
                    // wrapper passes `NULL, seat`); the previous code omitted it, so the
                    // compositor rejected the request ("invalid arguments ... get_text_input"
                    // -> fatal wl_display error). Marshal via wl_proxy_marshal_flags with
                    // the interface + NULL new_id + seat (fallback to marshal_constructor).
                    let text_input = unsafe {
                        let version = (window.wayland.wl_proxy_get_version)(manager as *mut wl_proxy);
                        if !window.wayland.wl_proxy_marshal_flags.is_null() {
                            type GetFlags = unsafe extern "C" fn(
                                *mut wl_proxy, u32, *const wl_interface, u32, u32,
                                *mut std::ffi::c_void, *mut wl_seat,
                            ) -> *mut wl_proxy;
                            let f: GetFlags = std::mem::transmute(window.wayland.wl_proxy_marshal_flags);
                            f(manager as *mut wl_proxy,
                              defines::ZWP_TEXT_INPUT_MANAGER_V3_GET_TEXT_INPUT,
                              text_input_interface, version, 0,
                              std::ptr::null_mut(), window.seat) as *mut zwp_text_input_v3
                        } else {
                            type GetCtor = unsafe extern "C" fn(
                                *mut wl_proxy, u32, *const wl_interface,
                                *mut std::ffi::c_void, *mut wl_seat,
                            ) -> *mut wl_proxy;
                            let f: GetCtor = std::mem::transmute(window.wayland.wl_proxy_marshal_constructor);
                            f(manager as *mut wl_proxy,
                              defines::ZWP_TEXT_INPUT_MANAGER_V3_GET_TEXT_INPUT,
                              text_input_interface, std::ptr::null_mut(), window.seat) as *mut zwp_text_input_v3
                        }
                    };

                    if !text_input.is_null() {
                        // Register event listener for text-input events
                        unsafe {
                            (window.wayland.wl_proxy_add_listener)(
                                text_input as *mut wl_proxy,
                                &ZWP_TEXT_INPUT_V3_LISTENER as *const _ as *const c_void,
                                data,
                            )
                        };

                        window.text_input = Some(text_input);
                        crate::log_debug!(
                            LogCategory::Platform,
                            "[Wayland] Bound zwp_text_input_v3 - native IME available"
                        );
                    }
                }
            }
        }
        "org_kde_kwin_blur_manager" => {
            // KDE Plasma blur protocol - allows client-requested blur effects. Not in
            // the core protocol, so libwayland doesn't export its wl_interface; bind it
            // through the normal `wl_registry_bind` (marshal_flags) with a hand-built
            // minimal interface. Binding with a NULL interface (the old code) made
            // libwayland reject the request -- a new-id bind REQUIRES a valid interface
            // to create the typed proxy ("null value passed for arg 3").
            let blur_manager = unsafe {
                (window.wayland.wl_registry_bind)(
                    registry,
                    name,
                    super::defines::get_kde_blur_manager_interface(),
                    version.min(1),
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
        "zxdg_decoration_manager_v1" => {
            // xdg-decoration-unstable-v1: lets the client request server-side
            // decorations (compositor-drawn titlebar). Unstable protocol, not
            // exported by libwayland -> bind with a hand-built interface (same as
            // the blur manager). The per-toplevel decoration object is created after
            // the xdg_toplevel exists (see WaylandWindow::new).
            let mgr = unsafe {
                (window.wayland.wl_registry_bind)(
                    registry,
                    name,
                    super::defines::get_zxdg_decoration_manager_v1_interface(),
                    version.min(1),
                ) as *mut zxdg_decoration_manager_v1
            };
            if !mgr.is_null() {
                window.decoration_manager = Some(mgr);
                crate::log_debug!(
                    LogCategory::Platform,
                    "[Wayland] Bound zxdg_decoration_manager_v1 - server-side decorations available"
                );
            }
        }
        _ => {}
    }
}

/// `zxdg_toplevel_decoration_v1.configure` — the compositor tells us which
/// decoration mode it will use (1 = client_side, 2 = server_side). Informational;
/// we requested server-side, so this confirms whether the compositor honored it.
pub(super) extern "C" fn toplevel_decoration_configure_handler(
    _data: *mut c_void,
    _deco: *mut zxdg_toplevel_decoration_v1,
    mode: u32,
) {
    // Informational: we requested server-side (2); this reports what the compositor
    // chose. A listener must exist for libwayland to dispatch the event, but we don't
    // need to act on it (the compositor draws the decorations either way).
    let _ = mode;
}

pub(super) extern "C" fn registry_global_remove_handler(
    _data: *mut c_void,
    _registry: *mut wl_registry,
    _name: u32,
) {
}

// wl_seat listener
// wl_touch listeners -> touch_state (x/y are wl_fixed_t, /256.0 to logical).
pub(super) extern "C" fn touch_down_handler(
    data: *mut c_void,
    _touch: *mut wl_touch,
    _serial: u32,
    _time: u32,
    _surface: *mut wl_surface,
    id: i32,
    x: i32,
    y: i32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.handle_touch_point(id, x as f64 / 256.0, y as f64 / 256.0);
}
pub(super) extern "C" fn touch_up_handler(
    data: *mut c_void,
    _touch: *mut wl_touch,
    _serial: u32,
    _time: u32,
    id: i32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.handle_touch_up(id);
}
pub(super) extern "C" fn touch_motion_handler(
    data: *mut c_void,
    _touch: *mut wl_touch,
    _time: u32,
    id: i32,
    x: i32,
    y: i32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.handle_touch_point(id, x as f64 / 256.0, y as f64 / 256.0);
}
extern "C" fn touch_frame_handler(_data: *mut c_void, _touch: *mut wl_touch) {}
pub(super) extern "C" fn touch_cancel_handler(data: *mut c_void, _touch: *mut wl_touch) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.handle_touch_cancel();
}
extern "C" fn touch_shape_handler(
    _data: *mut c_void,
    _touch: *mut wl_touch,
    _id: i32,
    _major: i32,
    _minor: i32,
) {
}
extern "C" fn touch_orientation_handler(
    _data: *mut c_void,
    _touch: *mut wl_touch,
    _id: i32,
    _orientation: i32,
) {
}

static WL_TOUCH_LISTENER: wl_touch_listener = wl_touch_listener {
    down: touch_down_handler,
    up: touch_up_handler,
    motion: touch_motion_handler,
    frame: touch_frame_handler,
    cancel: touch_cancel_handler,
    shape: touch_shape_handler,
    orientation: touch_orientation_handler,
};

// ===== Tablet (zwp_tablet_v2): pen feed into gesture pen-state; pad parse-and-drop =====
/// Once both the tablet manager + the seat are bound, get the tablet seat and
/// start listening. Idempotent; called from both registry arms (any order).
pub(super) unsafe fn try_init_tablet(window: &mut WaylandWindow, data: *mut c_void) {
    if window.tablet_initialized || window.tablet_manager.is_null() || window.seat.is_null() {
        return;
    }
    let seat =
        (window.wayland.zwp_tablet_manager_v2_get_tablet_seat)(window.tablet_manager, window.seat);
    window.tablet_seat = seat;
    (window.wayland.zwp_tablet_seat_v2_add_listener)(seat, &ZWP_TABLET_SEAT_LISTENER, data);
    window.tablet_initialized = true;
}

extern "C" fn tablet_seat_tablet_added(
    data: *mut c_void,
    _seat: *mut zwp_tablet_seat_v2,
    id: *mut zwp_tablet_v2,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    unsafe { (window.wayland.zwp_tablet_v2_add_listener)(id, &ZWP_TABLET_V2_LISTENER, data) };
}
extern "C" fn tablet_seat_tool_added(
    data: *mut c_void,
    _seat: *mut zwp_tablet_seat_v2,
    id: *mut zwp_tablet_tool_v2,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    unsafe { (window.wayland.zwp_tablet_tool_v2_add_listener)(id, &ZWP_TABLET_TOOL_LISTENER, data) };
}
extern "C" fn tablet_seat_pad_added(
    _data: *mut c_void,
    _seat: *mut zwp_tablet_seat_v2,
    _id: *mut zwp_tablet_pad_v2,
) {
    // Pad proxy is created by libwayland (the descriptors parse its events); no listener.
}
static ZWP_TABLET_SEAT_LISTENER: zwp_tablet_seat_v2_listener = zwp_tablet_seat_v2_listener {
    tablet_added: tablet_seat_tablet_added,
    tool_added: tablet_seat_tool_added,
    pad_added: tablet_seat_pad_added,
};

// zwp_tablet_v2 descriptive events — ignored (the pen comes via the tool).
extern "C" fn tablet_noop_name(_d: *mut c_void, _t: *mut zwp_tablet_v2, _n: *const c_char) {}
extern "C" fn tablet_noop_id(_d: *mut c_void, _t: *mut zwp_tablet_v2, _v: u32, _p: u32) {}
extern "C" fn tablet_noop_path(_d: *mut c_void, _t: *mut zwp_tablet_v2, _p: *const c_char) {}
extern "C" fn tablet_noop_done(_d: *mut c_void, _t: *mut zwp_tablet_v2) {}
extern "C" fn tablet_noop_removed(_d: *mut c_void, _t: *mut zwp_tablet_v2) {}
extern "C" fn tablet_noop_bustype(_d: *mut c_void, _t: *mut zwp_tablet_v2, _b: u32) {}
static ZWP_TABLET_V2_LISTENER: zwp_tablet_v2_listener = zwp_tablet_v2_listener {
    name: tablet_noop_name,
    id: tablet_noop_id,
    path: tablet_noop_path,
    done: tablet_noop_done,
    removed: tablet_noop_removed,
    bustype: tablet_noop_bustype,
};

// zwp_tablet_tool_v2 — the pen. Accumulate into window.tablet_pen; feed on `frame`.
extern "C" fn tool_type(data: *mut c_void, _t: *mut zwp_tablet_tool_v2, tool_type: u32) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.tablet_pen.is_eraser = tool_type == 0x141; // eraser
}
extern "C" fn tool_noop_uu(_d: *mut c_void, _t: *mut zwp_tablet_tool_v2, _a: u32, _b: u32) {}
extern "C" fn tool_noop_u(_d: *mut c_void, _t: *mut zwp_tablet_tool_v2, _a: u32) {}
extern "C" fn tool_noop(_d: *mut c_void, _t: *mut zwp_tablet_tool_v2) {}
extern "C" fn tool_proximity_in(
    _d: *mut c_void,
    _t: *mut zwp_tablet_tool_v2,
    _serial: u32,
    _tablet: *mut zwp_tablet_v2,
    _surface: *mut wl_surface,
) {
}
extern "C" fn tool_proximity_out(data: *mut c_void, _t: *mut zwp_tablet_tool_v2) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.tablet_pen.in_contact = false;
    window.tablet_pen.pressure = 0.0;
}
extern "C" fn tool_down(data: *mut c_void, _t: *mut zwp_tablet_tool_v2, _serial: u32) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.tablet_pen.in_contact = true;
}
extern "C" fn tool_up(data: *mut c_void, _t: *mut zwp_tablet_tool_v2) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.tablet_pen.in_contact = false;
}
extern "C" fn tool_motion(data: *mut c_void, _t: *mut zwp_tablet_tool_v2, x: i32, y: i32) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.tablet_pen.position =
        azul_core::geom::LogicalPosition::new(x as f32 / 256.0, y as f32 / 256.0);
}
extern "C" fn tool_pressure(data: *mut c_void, _t: *mut zwp_tablet_tool_v2, pressure: u32) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.tablet_pen.pressure = pressure as f32 / 65535.0;
}
extern "C" fn tool_distance(_d: *mut c_void, _t: *mut zwp_tablet_tool_v2, _distance: u32) {}
extern "C" fn tool_tilt(data: *mut c_void, _t: *mut zwp_tablet_tool_v2, tx: i32, ty: i32) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.tablet_pen.tilt_x = tx as f32 / 256.0;
    window.tablet_pen.tilt_y = ty as f32 / 256.0;
}
extern "C" fn tool_rotation(data: *mut c_void, _t: *mut zwp_tablet_tool_v2, degrees: i32) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.tablet_pen.rotation = (degrees as f32 / 256.0) * core::f32::consts::PI / 180.0;
}
extern "C" fn tool_slider(_d: *mut c_void, _t: *mut zwp_tablet_tool_v2, _position: i32) {}
extern "C" fn tool_wheel(_d: *mut c_void, _t: *mut zwp_tablet_tool_v2, _degrees: i32, _clicks: i32) {
}
extern "C" fn tool_button(
    _d: *mut c_void,
    _t: *mut zwp_tablet_tool_v2,
    _serial: u32,
    _button: u32,
    _state: u32,
) {
}
extern "C" fn tool_frame(data: *mut c_void, _t: *mut zwp_tablet_tool_v2, _time: u32) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.handle_tablet_frame();
}
static ZWP_TABLET_TOOL_LISTENER: zwp_tablet_tool_v2_listener = zwp_tablet_tool_v2_listener {
    type_: tool_type,
    hardware_serial: tool_noop_uu,
    hardware_id_wacom: tool_noop_uu,
    capability: tool_noop_u,
    done: tool_noop,
    removed: tool_noop,
    proximity_in: tool_proximity_in,
    proximity_out: tool_proximity_out,
    down: tool_down,
    up: tool_up,
    motion: tool_motion,
    pressure: tool_pressure,
    distance: tool_distance,
    tilt: tool_tilt,
    rotation: tool_rotation,
    slider: tool_slider,
    wheel: tool_wheel,
    button: tool_button,
    frame: tool_frame,
};

pub(super) extern "C" fn seat_capabilities_handler(
    data: *mut c_void,
    seat: *mut wl_seat,
    capabilities: u32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };

    if capabilities & WL_SEAT_CAPABILITY_POINTER != 0 {
        let pointer = unsafe { (window.wayland.wl_seat_get_pointer)(seat) };
        window.pointer_state.pointer = pointer;
        unsafe {
            (window.wayland.wl_pointer_add_listener)(pointer, &WL_POINTER_LISTENER, data)
        };
    }

    if capabilities & WL_SEAT_CAPABILITY_KEYBOARD != 0 {
        let keyboard = unsafe { (window.wayland.wl_seat_get_keyboard)(seat) };
        window.keyboard = keyboard; // stored so rebind_listeners() can re-point it
        unsafe {
            (window.wayland.wl_keyboard_add_listener)(keyboard, &WL_KEYBOARD_LISTENER, data)
        };
    }

    if capabilities & WL_SEAT_CAPABILITY_TOUCH != 0 {
        let touch = unsafe { (window.wayland.wl_seat_get_touch)(seat) };
        window.touch = touch; // stored so rebind_listeners() can re-point it
        unsafe { (window.wayland.wl_touch_add_listener)(touch, &WL_TOUCH_LISTENER, data) };
    }
}

pub(super) extern "C" fn seat_name_handler(
    _data: *mut c_void,
    _seat: *mut wl_seat,
    _name: *const c_char,
) {
}

// wl_keyboard listener
pub(super) extern "C" fn keyboard_keymap_handler(
    data: *mut c_void,
    _keyboard: *mut wl_keyboard,
    format: u32,
    fd: i32,
    size: u32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    if format != WL_KEYBOARD_KEYMAP_FORMAT_XKB_V1 || size == 0 {
        unsafe { libc::close(fd) };
        return;
    }

    // The keymap is delivered as a (read-only, NUL-terminated) shared-memory fd of
    // `size` bytes; the canonical way to read it is mmap, NOT read()/read_to_string
    // (which is unreliable on a sealed shm fd and keeps the trailing/padding NULs).
    // We mmap, take the bytes up to the first NUL, build a C string, and compile it.
    // Every failure path degrades gracefully (no panic, no NULL xkb_state deref).
    let map = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            size as usize,
            libc::PROT_READ,
            libc::MAP_PRIVATE,
            fd,
            0,
        )
    };
    if map == libc::MAP_FAILED {
        unsafe { libc::close(fd) };
        return;
    }
    let bytes = unsafe { std::slice::from_raw_parts(map as *const u8, size as usize) };
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    let c_string = std::ffi::CString::new(&bytes[..end]).ok();
    unsafe {
        libc::munmap(map, size as usize);
        libc::close(fd);
    }
    let c_string = match c_string {
        Some(c) => c,
        None => return,
    };

    let context = unsafe { (window.xkb.xkb_context_new)(XKB_CONTEXT_NO_FLAGS) };
    if context.is_null() {
        return;
    }
    let keymap = unsafe {
        (window.xkb.xkb_keymap_new_from_string)(
            context,
            c_string.as_ptr(),
            XKB_KEYMAP_FORMAT_TEXT_V1,
            XKB_KEYMAP_COMPILE_NO_FLAGS,
        )
    };
    if keymap.is_null() {
        // Keymap failed to compile (e.g. a layout xkbcommon can't parse). Keep any
        // previous working keymap/state rather than installing a NULL one (a NULL
        // xkb_state would segfault in the key/modifier handlers).
        crate::log_warn!(
            LogCategory::Platform,
            "[Wayland] xkb_keymap_new_from_string failed to parse the keymap; keyboard input disabled"
        );
        return;
    }
    let state = unsafe { (window.xkb.xkb_state_new)(keymap) };
    if state.is_null() {
        return;
    }
    window.keyboard_state.context = context;
    window.keyboard_state.keymap = keymap;
    window.keyboard_state.state = state;
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
    // No usable keymap/state (compositor sent an unparseable keymap) -> skip rather
    // than deref a NULL xkb_state in the translation path.
    if window.keyboard_state.state.is_null() {
        return;
    }
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
    if window.keyboard_state.state.is_null() {
        return;
    }
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
    // A configure is the initial map AND every resize. The frame that follows must
    // be a FULL regeneration (relayout + rebuild + send the display-list transaction),
    // not the lightweight image-only path — otherwise WebRender has no display list
    // for this surface and renders an uncleared backbuffer (garbage). This mirrors
    // the X11 ConfigureNotify path. request_redraw() additionally sets needs_redraw.
    window.common.frame_needs_regeneration = true;
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

        window.common.current_window_state.flags.frame = if is_fullscreen {
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
        let current_width = window.common.current_window_state.size.dimensions.width as i32;
        let current_height = window.common.current_window_state.size.dimensions.height as i32;

        if width != current_width || height != current_height {
            // Store old context for breakpoint detection
            let old_context = window.dynamic_selector_context.clone();

            window.common.current_window_state.size.dimensions.width = width as f32;
            window.common.current_window_state.size.dimensions.height = height as f32;
            window.common.frame_needs_regeneration = true;

            // Update dynamic selector context with new viewport dimensions
            window.dynamic_selector_context.viewport_width = width as f32;
            window.dynamic_selector_context.viewport_height = height as f32;
            window.dynamic_selector_context.orientation = if width > height {
                azul_css::dynamic_selector::OrientationType::Landscape
            } else {
                azul_css::dynamic_selector::OrientationType::Portrait
            };

            // Check if any CSS breakpoints were crossed
            if old_context
                .viewport_breakpoint_changed(
                    &window.dynamic_selector_context,
                    super::super::super::common::CSS_BREAKPOINTS,
                )
            {
                log_debug!(
                    LogCategory::Layout,
                    "[Wayland Resize] Breakpoint crossed: {}x{} -> {}x{}",
                    old_context.viewport_width,
                    old_context.viewport_height,
                    window.dynamic_selector_context.viewport_width,
                    window.dynamic_selector_context.viewport_height
                );
            }

            // Tag the next regen as a resize so the user's layout()
            // callback can detect it via `info.relayout_reason()`.
            window.common.next_relayout_reason =
                azul_core::callbacks::RelayoutReason::Resize;

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
    surface: *mut wl_surface,
    surface_x: i32,
    surface_y: i32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    // wl_fixed_t (24.8 fixed-point) -> logical f64.
    let (x, y) = (surface_x as f64 / 256.0, surface_y as f64 / 256.0);
    // Resolve whether the pointer entered an open menu popup's surface (vs. the
    // parent) here — comparing the raw `wl_surface` — and pass a bool, so the
    // public `handle_pointer_enter` signature stays free of FFI pointer types.
    // (This child module can read the popup's private `surface` field.)
    let over_popup = window
        .active_popup
        .as_ref()
        .map_or(false, |p| !surface.is_null() && p.surface == surface);
    window.handle_pointer_enter(serial, x, y, over_popup);
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
    surface_x: i32,
    surface_y: i32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    let (x, y) = (surface_x as f64 / 256.0, surface_y as f64 / 256.0);
    window.handle_pointer_motion(x, y);
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
    value: i32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.handle_pointer_axis(axis, value as f64 / 256.0);
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
    data: *mut c_void,
    _keyboard: *mut wl_keyboard,
    _serial: u32,
    _surface: *mut wl_surface,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.handle_keyboard_leave();
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
    x11_map(keysym as super::super::x11::defines::KeySym)
}

// ============================================================
// zwp_text_input_v3 event handlers
// ============================================================

/// Pending text-input state accumulated between preedit_string/commit_string and done events.
/// The text-input v3 protocol batches: preedit_string and/or commit_string arrive first,
/// then `done` signals that the batch is complete and should be applied.
pub(super) struct TextInputPendingState {
    pub preedit_text: Option<String>,
    pub preedit_cursor_begin: i32,
    pub preedit_cursor_end: i32,
    pub commit_text: Option<String>,
    /// Number of UTF-8 bytes to delete before cursor
    pub delete_before: u32,
    /// Number of UTF-8 bytes to delete after cursor
    pub delete_after: u32,
}

impl Default for TextInputPendingState {
    fn default() -> Self {
        Self {
            preedit_text: None,
            preedit_cursor_begin: -1,
            preedit_cursor_end: -1,
            commit_text: None,
            delete_before: 0,
            delete_after: 0,
        }
    }
}

pub(super) extern "C" fn text_input_enter_handler(
    data: *mut c_void,
    _text_input: *mut zwp_text_input_v3,
    _surface: *mut wl_surface,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    log_debug!(
        LogCategory::Platform,
        "[Wayland] text_input_v3: enter - IME activated for surface"
    );
    // The compositor tells us IME is available for this surface.
    // We'll call enable() when a contenteditable gains focus.
    window.text_input_active = true;
}

pub(super) extern "C" fn text_input_leave_handler(
    data: *mut c_void,
    _text_input: *mut zwp_text_input_v3,
    _surface: *mut wl_surface,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    log_debug!(
        LogCategory::Platform,
        "[Wayland] text_input_v3: leave - IME deactivated"
    );
    window.text_input_active = false;
    // Clear any pending preedit
    if let Some(ref mut lw) = window.common.layout_window {
        lw.text_edit_manager.clear_preedit();
    }
}

pub(super) extern "C" fn text_input_preedit_string_handler(
    data: *mut c_void,
    _text_input: *mut zwp_text_input_v3,
    text: *const std::ffi::c_char,
    cursor_begin: i32,
    cursor_end: i32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    let preedit = if text.is_null() {
        None
    } else {
        unsafe { CStr::from_ptr(text) }.to_str().ok().map(|s| s.to_string())
    };
    log_debug!(
        LogCategory::Platform,
        "[Wayland] text_input_v3: preedit_string text={:?} cursor={}..{}",
        preedit,
        cursor_begin,
        cursor_end
    );
    window.text_input_pending.preedit_text = preedit;
    window.text_input_pending.preedit_cursor_begin = cursor_begin;
    window.text_input_pending.preedit_cursor_end = cursor_end;
}

pub(super) extern "C" fn text_input_commit_string_handler(
    data: *mut c_void,
    _text_input: *mut zwp_text_input_v3,
    text: *const std::ffi::c_char,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    let commit = if text.is_null() {
        None
    } else {
        unsafe { CStr::from_ptr(text) }.to_str().ok().map(|s| s.to_string())
    };
    log_debug!(
        LogCategory::Platform,
        "[Wayland] text_input_v3: commit_string text={:?}",
        commit
    );
    window.text_input_pending.commit_text = commit;
}

pub(super) extern "C" fn text_input_delete_surrounding_text_handler(
    data: *mut c_void,
    _text_input: *mut zwp_text_input_v3,
    before_length: u32,
    after_length: u32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    log_debug!(
        LogCategory::Platform,
        "[Wayland] text_input_v3: delete_surrounding_text before={} after={}",
        before_length,
        after_length
    );
    window.text_input_pending.delete_before = before_length;
    window.text_input_pending.delete_after = after_length;
}

pub(super) extern "C" fn text_input_done_handler(
    data: *mut c_void,
    _text_input: *mut zwp_text_input_v3,
    serial: u32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    log_debug!(
        LogCategory::Platform,
        "[Wayland] text_input_v3: done serial={}",
        serial
    );

    // Extract all pending state at once
    let commit_text = window.text_input_pending.commit_text.take();
    let preedit_text = window.text_input_pending.preedit_text.take();
    let preedit_begin = window.text_input_pending.preedit_cursor_begin;
    let preedit_end = window.text_input_pending.preedit_cursor_end;
    let delete_before = window.text_input_pending.delete_before;
    let delete_after = window.text_input_pending.delete_after;

    // Reset pending state
    window.text_input_pending = TextInputPendingState::default();

    let mut needs_process = false;

    // Step 1: Apply surrounding text deletions
    // The IME sends byte counts, but delete_selection operates on grapheme clusters.
    // Approximate: each deletion request removes one grapheme cluster.
    if delete_before > 0 || delete_after > 0 {
        if let Some(ref mut lw) = window.common.layout_window {
            if let Some(focused) = lw.focus_manager.get_focused_node().copied() {
                // Delete before cursor (backspace direction)
                for _ in 0..delete_before {
                    lw.delete_selection(focused, false);
                }
                // Delete after cursor (forward/delete direction)
                for _ in 0..delete_after {
                    lw.delete_selection(focused, true);
                }
                needs_process = true;
            }
        }
    }

    // Step 2: Commit confirmed text
    if let Some(text) = commit_text {
        if !text.is_empty() {
            if let Some(ref mut lw) = window.common.layout_window {
                lw.text_edit_manager.clear_preedit();
                let _ = lw.record_text_input(&text);
            }
            needs_process = true;
        }
    }

    if needs_process {
        use azul_core::events::ProcessEventResult;
        match window.process_window_events(0) {
            ProcessEventResult::ShouldRegenerateDomCurrentWindow => {
                if let Err(e) = window.regenerate_layout() {
                    log_error!(
                        LogCategory::Layout,
                        "[Wayland] IME layout regeneration error: {}",
                        e
                    );
                }
            }
            ProcessEventResult::ShouldReRenderCurrentWindow => {
                window.request_redraw();
            }
            _ => {}
        }
    }

    // Step 3: Update preedit display + request redraw
    if let Some(ref mut lw) = window.common.layout_window {
        if let Some(ref preedit) = preedit_text {
            lw.text_edit_manager.set_preedit(preedit.clone(), preedit_begin, preedit_end);
        } else {
            lw.text_edit_manager.clear_preedit();
        }
    }
    // Preedit changes (set or clear) need a redraw
    window.request_redraw();
}
