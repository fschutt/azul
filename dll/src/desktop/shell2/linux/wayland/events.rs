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

use super::super::super::common::clipboard::MAX_FLAVOR_BYTES;
use super::super::super::common::debug_server::LogCategory;
use super::super::super::common::event::PlatformWindow;
use super::super::common::compose::{ComposeAction, ComposeSequencer};
use crate::{log_debug, log_error, log_info, log_trace, log_warn};

// -- State for input devices --

/// XKB keyboard state for translating Wayland key events into keysyms.
pub(super) struct WaylandKeyboardState {
    pub(super) context: *mut xkb_context,
    pub(super) keymap: *mut xkb_keymap,
    pub(super) state: *mut xkb_state,
    /// Dead keys / the Compose key. `None` when the locale has no Compose
    /// file or libxkbcommon predates the compose API; the key path then falls
    /// back to the raw keysym, which is what it did everywhere before.
    pub(super) compose: Option<ComposeSequencer>,
}

impl WaylandKeyboardState {
    pub(super) fn new() -> Self {
        Self {
            context: std::ptr::null_mut(),
            keymap: std::ptr::null_mut(),
            state: std::ptr::null_mut(),
            compose: None,
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

/// Pad state accumulated between `frame` events, mirroring `TabletPenPending`.
#[derive(Debug, Clone, Copy, Default)]
pub struct TabletPadPending {
    pub express_keys: u32,
    pub touch_ring: f32,
    pub touch_ring_active: bool,
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

/// `wl_shm.format`: records ABGR8888 support (#27 native backbuffer — that
/// format's memory byte order matches the CPU renderer's RGBA output).
/// Formats are a property of the compositor, not of a window, so a
/// process-global flag is correct even with multiple windows.
extern "C" fn wl_shm_format_handler(_data: *mut c_void, _shm: *mut defines::wl_shm, format: u32) {
    // Live-run 2026-08-12: the ABGR flag never flipped on KWin — log every
    // received format so "listener never fires" and "fires but AB24 absent"
    // are distinguishable in one run.
    crate::log_debug!(
        super::super::super::common::debug_server::LogCategory::Platform,
        "[native-bb] wl_shm.format advertised: {:#010x}",
        format
    );
    if format == defines::WL_SHM_FORMAT_ABGR8888 {
        super::SHM_ABGR8888_ADVERTISED.store(true, core::sync::atomic::Ordering::Relaxed);
    }
}

static WL_SHM_LISTENER: wl_shm_listener = wl_shm_listener {
    format: wl_shm_format_handler,
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

    // Fractional-scale protocol active? Then it — not the integer wl_output
    // scale — owns size.dpi (the compositor sends preferred_scale on monitor
    // changes too). Keep the output bookkeeping above, skip the dpi update.
    if window.preferred_scale_120.is_some() {
        return;
    }

    // Check if scale factor changed (entered monitor with different DPI)
    let new_scale = window.calculate_current_scale_factor();
    let old_dpi = window.common.current_window_state().size.dpi;
    let new_dpi = (new_scale * 96.0) as u32;

    // Only regenerate if DPI changed significantly
    if (new_dpi as i32 - old_dpi as i32).abs() > 1 {
        log_info!(
            LogCategory::Window,
            "[Wayland DPI Change] {} -> {} (entered new monitor)",
            old_dpi,
            new_dpi
        );
        apply_os_dpi_change(window, new_dpi);
    }
}

/// Publish an OS-driven DPI change: snapshot the diff baseline, write
/// `size.dpi`, recreate the shm buffers at the new physical size, schedule the
/// frame and run the shared pass.
///
/// The pass is not optional. `WindowDpiChanged` is derived from the `size.dpi`
/// delta between previous and current, so writing `current` alone left the next
/// handler's snapshot to erase the change and the app never heard about it.
fn apply_os_dpi_change(window: &mut WaylandWindow, new_dpi: u32) {
    window.snapshot_window_state_baseline("wayland.apply_os_dpi_change");
    // Source = Os: the compositor already applied the scale, so the write lands
    // in `current` AND the OS-sync baseline, and never in `previous_window_state`
    // (the event delta the pass at the bottom consumes).
    window.common.update_window_state(
        crate::desktop::shell2::common::event::WindowStateSource::Os,
        |ws| {
            ws.size.dpi = new_dpi;
        },
    );
    window
        .common
        .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
    // Recreate the shm buffers at the new scale (physical = logical × scale) —
    // the old buffers are sized for the previous scale and the copy clamp would
    // truncate every frame.
    let (w, h) = {
        let d = &window.common.current_window_state().size.dimensions;
        (d.width as i32, d.height as i32)
    };
    window.resize_surface(w, h);
    // Schedule the frame NOW. Setting the flag alone renders nothing: Wayland
    // gets no spurious expose/configure events, so an idle window dragged to
    // another monitor kept its old-DPI frame until the next input event.
    window.request_redraw();

    let result = window.process_window_events(0);
    window.handle_process_event_result(result);
}

pub(super) extern "C" fn wl_surface_leave_handler(
    data: *mut c_void,
    _surface: *mut wl_surface,
    output: *mut wl_output,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };

    // Remove this output from current_outputs
    window.current_outputs.retain(|&o| o != output);

    // Fractional-scale protocol owns size.dpi when active (see enter handler).
    if window.preferred_scale_120.is_some() {
        return;
    }

    // Check if scale factor changed (left monitor, now on different monitor)
    let new_scale = window.calculate_current_scale_factor();
    let old_dpi = window.common.current_window_state().size.dpi;
    let new_dpi = (new_scale * 96.0) as u32;

    // Only regenerate if DPI changed significantly
    if (new_dpi as i32 - old_dpi as i32).abs() > 1 {
        log_info!(
            LogCategory::Window,
            "[Wayland DPI Change] {} -> {} (left monitor)",
            old_dpi,
            new_dpi
        );
        apply_os_dpi_change(window, new_dpi);
    }
}

/// `wp_fractional_scale_v1.preferred_scale` — the compositor's preferred scale
/// for our surface, delivered as scale×120 (120 = 1.0, 144 = 1.2, 180 = 1.5).
/// Takes over DPI ownership from the integer wl_output path: updates size.dpi
/// (= scale × 96), recreates the shm buffers at the new physical size,
/// relayouts and schedules a full repaint. `WindowSize.dimensions` stays
/// LOGICAL (that contract is scale-independent).
pub(super) extern "C" fn wp_fractional_scale_preferred_scale_handler(
    data: *mut c_void,
    _fractional_scale: *mut wp_fractional_scale_v1,
    scale_120: u32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    if scale_120 == 0 || window.preferred_scale_120 == Some(scale_120) {
        return;
    }
    let old_dpi = window.common.current_window_state().size.dpi;
    // dpi = scale × 96 = scale_120 × 96 / 120, rounded to the nearest integer
    // (size.dpi is u32; the exact ×120 value stays in preferred_scale_120).
    let new_dpi = (scale_120 * 96 + 60) / 120;
    window.preferred_scale_120 = Some(scale_120);

    if new_dpi == old_dpi {
        return; // e.g. the initial preferred_scale(120) on a 1.0 output
    }

    log_info!(
        LogCategory::Window,
        "[Wayland DPI Change] {} -> {} (wp_fractional_scale preferred_scale = {}/120)",
        old_dpi,
        new_dpi,
        scale_120
    );
    apply_os_dpi_change(window, new_dpi);
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
            // #27 native backbuffer: learn which pixel formats the compositor
            // accepts. The format events arrive on the next dispatch, i.e.
            // during window setup's later roundtrips — before the first shm
            // pool is created. If they never arrive the flag simply stays
            // false and pools stay ARGB8888 (legacy swizzle path).
            if !window.shm.is_null() {
                unsafe {
                    let rc = (window.wayland.wl_proxy_add_listener)(
                        window.shm as *mut wl_proxy,
                        &WL_SHM_LISTENER as *const _ as *const c_void,
                        std::ptr::null_mut(),
                    );
                    // Live-run 2026-08-12: formats never arrived — a failed
                    // attach (rc != 0: proxy already has a listener, or
                    // events already dispatched) must announce itself.
                    if rc != 0 {
                        crate::log_warn!(
                            super::super::super::common::debug_server::LogCategory::Platform,
                            "[native-bb] wl_shm listener attach FAILED (rc={rc}) — \
                             format detection dead, pools stay ARGB8888"
                        );
                    }
                }
            }
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
            // THE shipped-crash site: this listener is registered while `data`
            // still points at the `WaylandWindow::new()` STACK local, and
            // `xdg_wm_base` has no entry in the old hand-written rebind array, so
            // the first compositor ping after the window was boxed dereferenced a
            // dead stack frame. Registration now records the proxy itself.
            window.track_listener(window.xdg_wm_base);
        }
        "wl_seat" => {
            let seat_version = version.min(7);
            let seat = unsafe {
                (window.wayland.wl_registry_bind)(
                    registry,
                    name,
                    &window.wayland.wl_seat_interface,
                    seat_version,
                ) as *mut wl_seat
            };
            window.seat = seat;
            window.seat_version = seat_version;
            unsafe { (window.wayland.wl_seat_add_listener)(seat, &WL_SEAT_LISTENER, data) };
            window.track_listener(seat);
            unsafe { try_init_tablet(window, data) };
            unsafe { try_init_data_device(window, data) };
            unsafe { try_init_primary_selection(window, data) };
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
        "wl_data_device_manager" => {
            // Bind at version.min(3): v3 adds the DnD-action negotiation
            // (set_actions/finish/source_actions/action) required by modern
            // compositors. Lower versions skip those (version-gated below).
            let v = version.min(3);
            window.data_device_version = v;
            window.data_device_manager = unsafe {
                (window.wayland.wl_registry_bind)(
                    registry,
                    name,
                    &window.wayland.wl_data_device_manager_interface,
                    v,
                ) as *mut _
            };
            unsafe { try_init_data_device(window, data) };
        }
        "zwp_primary_selection_device_manager_v1" => {
            // The Wayland PRIMARY selection (select-to-copy / middle-click
            // paste). Unstable-v1 has exactly one version.
            window.primary_selection_manager = unsafe {
                (window.wayland.wl_registry_bind)(
                    registry,
                    name,
                    get_primary_selection_device_manager_v1_interface(),
                    version.min(1),
                ) as *mut _
            };
            unsafe { try_init_primary_selection(window, data) };
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
                global_name: name,
                name: format!("output-{}", name),
                scale: 1,
                x: 0,
                y: 0,
                width: 0,
                height: 0,
                make: String::new(),
                model: String::new(),
            });

            unsafe { (window.wayland.wl_output_add_listener)(output, &WL_OUTPUT_LISTENER, data) };
            // Same defect class as `xdg_wm_base` above: outputs bound during the
            // initial roundtrip carry the stack pointer, and the old rebind array
            // had no entry for them — a monitor scale/geometry change would have
            // faulted in `wl_output_scale_handler` exactly like the ping did.
            window.track_listener(output);
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
                (window.wayland.wl_registry_bind)(registry, name, manager_interface, version.min(1))
                    as *mut zwp_text_input_manager_v3
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
                        let version =
                            (window.wayland.wl_proxy_get_version)(manager as *mut wl_proxy);
                        if !window.wayland.wl_proxy_marshal_flags.is_null() {
                            type GetFlags = unsafe extern "C" fn(
                                *mut wl_proxy,
                                u32,
                                *const wl_interface,
                                u32,
                                u32,
                                *mut std::ffi::c_void,
                                *mut wl_seat,
                            )
                                -> *mut wl_proxy;
                            let f: GetFlags =
                                std::mem::transmute(window.wayland.wl_proxy_marshal_flags);
                            f(
                                manager as *mut wl_proxy,
                                defines::ZWP_TEXT_INPUT_MANAGER_V3_GET_TEXT_INPUT,
                                text_input_interface,
                                version,
                                0,
                                std::ptr::null_mut(),
                                window.seat,
                            ) as *mut zwp_text_input_v3
                        } else {
                            type GetCtor = unsafe extern "C" fn(
                                *mut wl_proxy,
                                u32,
                                *const wl_interface,
                                *mut std::ffi::c_void,
                                *mut wl_seat,
                            )
                                -> *mut wl_proxy;
                            let f: GetCtor =
                                std::mem::transmute(window.wayland.wl_proxy_marshal_constructor);
                            f(
                                manager as *mut wl_proxy,
                                defines::ZWP_TEXT_INPUT_MANAGER_V3_GET_TEXT_INPUT,
                                text_input_interface,
                                std::ptr::null_mut(),
                                window.seat,
                            ) as *mut zwp_text_input_v3
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
                        window.track_listener(text_input);

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
        "wp_fractional_scale_manager_v1" => {
            // fractional-scale-v1: the compositor tells us the preferred
            // per-surface scale as scale×120 (144 = 1.2). Staging protocol, not
            // exported by libwayland -> hand-built interface (same as the blur
            // manager). The per-surface wp_fractional_scale_v1 object is
            // created after the wl_surface exists (see WaylandWindow::new).
            let mgr = unsafe {
                (window.wayland.wl_registry_bind)(
                    registry,
                    name,
                    super::defines::get_wp_fractional_scale_manager_v1_interface(),
                    version.min(1),
                ) as *mut wp_fractional_scale_manager_v1
            };
            if !mgr.is_null() {
                window.fractional_scale_manager = Some(mgr);
                crate::log_debug!(
                    LogCategory::Platform,
                    "[Wayland] Bound wp_fractional_scale_manager_v1 - fractional scaling available"
                );
            }
        }
        "wp_viewporter" => {
            // viewporter (stable): maps a physical-sized buffer onto the
            // logical surface size (wp_viewport.set_destination) — required to
            // present fractional-scale buffers, since set_buffer_scale is
            // integer-only. Per-surface viewports are created after the
            // wl_surface exists (see WaylandWindow::new).
            let vpr = unsafe {
                (window.wayland.wl_registry_bind)(
                    registry,
                    name,
                    super::defines::get_wp_viewporter_interface(),
                    version.min(1),
                ) as *mut wp_viewporter
            };
            if !vpr.is_null() {
                window.viewporter = Some(vpr);
                crate::log_debug!(
                    LogCategory::Platform,
                    "[Wayland] Bound wp_viewporter - viewport scaling available"
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
    data: *mut c_void,
    _deco: *mut zxdg_toplevel_decoration_v1,
    mode: u32,
) {
    // MWA-B6: the compositor reports the mode it WILL use (1 = client_side,
    // 2 = server_side) — it may refuse our request. If it will NOT draw
    // server decorations while the window still expects them, flip to CSD
    // (frameless + azul titlebar) and regenerate. The old handler discarded
    // this, leaving a bare uncloseable rectangle on refusing compositors.
    if data.is_null() {
        return;
    }
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    const CLIENT_SIDE: u32 = 1;
    let refuses_ssd = mode == CLIENT_SIDE
        && window.common.current_window_state().flags.decorations
            != azul_core::window::WindowDecorations::None;
    if refuses_ssd {
        window.common.update_window_state(
            crate::desktop::shell2::common::event::WindowStateSource::Os,
            |ws| {
                ws.flags.decorations = azul_core::window::WindowDecorations::None;
                ws.flags.has_decorations = true;
            },
        );
        window
            .common
            .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
        window.request_redraw();
    }
}

/// A global went away. In practice: a monitor was unplugged, powered off over
/// DisplayPort (which the link treats as an unplug), or the compositor dropped
/// a virtual output.
///
/// This used to be an empty stub, which leaked in three separate ways:
///   * `known_outputs` grew monotonically across replug cycles, so every index
///     after the removed one shifted — and `get_current_monitor()` correlates
///     `known_outputs` against `display::get_displays()` BY INDEX.
///   * the `wl_output` proxy was never destroyed.
///   * the tracked-listener entry kept pointing at a dead proxy, so a later
///     event on a recycled id could dispatch into freed state.
pub(super) extern "C" fn registry_global_remove_handler(
    data: *mut c_void,
    _registry: *mut wl_registry,
    name: u32,
) {
    let window = unsafe { &mut *(data as *mut super::WaylandWindow) };

    let Some(idx) = window
        .known_outputs
        .iter()
        .position(|m| m.global_name == name)
    else {
        // Not an output — seats, shells and every other global share this
        // event, and we only track outputs here.
        return;
    };

    let removed = window.known_outputs.remove(idx);

    // The surface may still list this output as one it had entered; drop it,
    // or `calculate_current_scale_factor()` keeps folding a dead output's
    // scale into the max forever.
    window.current_outputs.retain(|p| *p != removed.proxy);

    // Drop the rebind entry before destroying the proxy, or the next
    // `rebind_listeners()` re-registers a listener on freed memory.
    let dead = removed.proxy.cast::<super::defines::wl_proxy>();
    window.listener_proxies.retain(|p| *p != dead);
    unsafe { (window.wayland.wl_proxy_destroy)(removed.proxy.cast()) };

    log_info!(
        LogCategory::Window,
        "[Wayland] output {} ({}) removed; {} left",
        name,
        removed.name,
        window.known_outputs.len()
    );

    // The topology changed, so the memoised display list is now wrong.
    if let Some(ref lw) = window.common.layout_window {
        if let Ok(mut guard) = lw.monitors.lock() {
            *guard = crate::desktop::display::refresh_monitors();
        }
    }

    // Losing the output the window was on changes its effective scale — same
    // recompute as the `leave` handler. The fractional-scale protocol owns
    // size.dpi outright when active, so leave it alone there.
    if window.preferred_scale_120.is_some() {
        return;
    }
    let new_dpi = (window.calculate_current_scale_factor() * 96.0) as u32;
    let old_dpi = window.common.current_window_state().size.dpi;
    if new_dpi > 0 && (new_dpi as i32 - old_dpi as i32).abs() > 1 {
        apply_os_dpi_change(window, new_dpi);
    }
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
    window.track_listener(seat);
    window.tablet_initialized = true;
}

extern "C" fn tablet_seat_tablet_added(
    data: *mut c_void,
    _seat: *mut zwp_tablet_seat_v2,
    id: *mut zwp_tablet_v2,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    unsafe { (window.wayland.zwp_tablet_v2_add_listener)(id, &ZWP_TABLET_V2_LISTENER, data) };
    window.track_listener(id);
}
extern "C" fn tablet_seat_tool_added(
    data: *mut c_void,
    _seat: *mut zwp_tablet_seat_v2,
    id: *mut zwp_tablet_tool_v2,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    unsafe {
        (window.wayland.zwp_tablet_tool_v2_add_listener)(id, &ZWP_TABLET_TOOL_LISTENER, data)
    };
    window.track_listener(id);
}
extern "C" fn tablet_seat_pad_added(
    data: *mut c_void,
    _seat: *mut zwp_tablet_seat_v2,
    id: *mut zwp_tablet_pad_v2,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    unsafe { (window.wayland.zwp_tablet_pad_v2_add_listener)(id, &ZWP_TABLET_PAD_LISTENER, data) };
    window.track_listener(id);
}

// ===== zwp_tablet_pad_v2 — ExpressKeys, ring and strip =====
//
// The pad is a separate device from the pen and moves no cursor, so none of
// this goes through the pointer path. Button state accumulates into a bitset
// and the ring/strip into a single normalised position; both are pushed to the
// gesture manager as one `WacomPadState`.
//
// Note the two different unit conventions sitting next to each other: the ring
// reports wl_fixed DEGREES (so /256.0 then /360.0) while the strip reports a
// plain integer 0..=65535.

extern "C" fn pad_group(
    data: *mut c_void,
    _pad: *mut zwp_tablet_pad_v2,
    id: *mut zwp_tablet_pad_group_v2,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    unsafe {
        (window.wayland.zwp_tablet_pad_group_v2_add_listener)(
            id,
            &ZWP_TABLET_PAD_GROUP_LISTENER,
            data,
        )
    };
    window.track_listener(id);
}
extern "C" fn pad_path(_d: *mut c_void, _p: *mut zwp_tablet_pad_v2, _path: *const c_char) {}
extern "C" fn pad_buttons(_d: *mut c_void, _p: *mut zwp_tablet_pad_v2, _n: u32) {}
extern "C" fn pad_done(_d: *mut c_void, _p: *mut zwp_tablet_pad_v2) {}
extern "C" fn pad_button(
    data: *mut c_void,
    _p: *mut zwp_tablet_pad_v2,
    _time: u32,
    button: u32,
    state: u32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    // `WacomPadState::express_keys` is a u32 bitset, so buttons past 31 have
    // nowhere to go. Real pads top out well below that; drop rather than wrap,
    // which would report the wrong key as held.
    if button < 32 {
        let bit = 1u32 << button;
        if state == 1 {
            window.tablet_pad.express_keys |= bit;
        } else {
            window.tablet_pad.express_keys &= !bit;
        }
    }
    window.handle_tablet_pad_frame();
}
extern "C" fn pad_enter(
    _d: *mut c_void,
    _p: *mut zwp_tablet_pad_v2,
    _serial: u32,
    _tablet: *mut zwp_tablet_v2,
    _surface: *mut wl_surface,
) {
}
extern "C" fn pad_leave(
    data: *mut c_void,
    _p: *mut zwp_tablet_pad_v2,
    _serial: u32,
    _surface: *mut wl_surface,
) {
    // Focus left the surface: the compositor stops sending button releases, so
    // holding state here would latch a key down forever.
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.tablet_pad = TabletPadPending::default();
    window.handle_tablet_pad_frame();
}
extern "C" fn pad_removed(data: *mut c_void, _p: *mut zwp_tablet_pad_v2) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.tablet_pad = TabletPadPending::default();
    window.handle_tablet_pad_frame();
}
static ZWP_TABLET_PAD_LISTENER: zwp_tablet_pad_v2_listener = zwp_tablet_pad_v2_listener {
    group: pad_group,
    path: pad_path,
    buttons: pad_buttons,
    done: pad_done,
    button: pad_button,
    enter: pad_enter,
    leave: pad_leave,
    removed: pad_removed,
};

extern "C" fn pad_group_buttons(
    _d: *mut c_void,
    _g: *mut zwp_tablet_pad_group_v2,
    _b: *mut wl_array,
) {
}
extern "C" fn pad_group_ring(
    data: *mut c_void,
    _g: *mut zwp_tablet_pad_group_v2,
    id: *mut zwp_tablet_pad_ring_v2,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    unsafe {
        (window.wayland.zwp_tablet_pad_ring_v2_add_listener)(
            id,
            &ZWP_TABLET_PAD_RING_LISTENER,
            data,
        )
    };
    window.track_listener(id);
}
extern "C" fn pad_group_strip(
    data: *mut c_void,
    _g: *mut zwp_tablet_pad_group_v2,
    id: *mut zwp_tablet_pad_strip_v2,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    unsafe {
        (window.wayland.zwp_tablet_pad_strip_v2_add_listener)(
            id,
            &ZWP_TABLET_PAD_STRIP_LISTENER,
            data,
        )
    };
    window.track_listener(id);
}
extern "C" fn pad_group_modes(_d: *mut c_void, _g: *mut zwp_tablet_pad_group_v2, _m: u32) {}
extern "C" fn pad_group_done(_d: *mut c_void, _g: *mut zwp_tablet_pad_group_v2) {}
extern "C" fn pad_group_mode_switch(
    _d: *mut c_void,
    _g: *mut zwp_tablet_pad_group_v2,
    _time: u32,
    _serial: u32,
    _mode: u32,
) {
}
static ZWP_TABLET_PAD_GROUP_LISTENER: zwp_tablet_pad_group_v2_listener =
    zwp_tablet_pad_group_v2_listener {
        buttons: pad_group_buttons,
        ring: pad_group_ring,
        strip: pad_group_strip,
        modes: pad_group_modes,
        done: pad_group_done,
        mode_switch: pad_group_mode_switch,
    };

extern "C" fn pad_ring_source(_d: *mut c_void, _r: *mut zwp_tablet_pad_ring_v2, _s: u32) {}
extern "C" fn pad_ring_angle(data: *mut c_void, _r: *mut zwp_tablet_pad_ring_v2, degrees: i32) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    // wl_fixed (24.8) degrees -> 0.0..=1.0 around the ring.
    let deg = degrees as f32 / 256.0;
    window.tablet_pad.touch_ring = (deg / 360.0).clamp(0.0, 1.0);
    window.tablet_pad.touch_ring_active = true;
}
extern "C" fn pad_ring_stop(data: *mut c_void, _r: *mut zwp_tablet_pad_ring_v2) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.tablet_pad.touch_ring_active = false;
}
extern "C" fn pad_ring_frame(data: *mut c_void, _r: *mut zwp_tablet_pad_ring_v2, _time: u32) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.handle_tablet_pad_frame();
}
static ZWP_TABLET_PAD_RING_LISTENER: zwp_tablet_pad_ring_v2_listener =
    zwp_tablet_pad_ring_v2_listener {
        source: pad_ring_source,
        angle: pad_ring_angle,
        stop: pad_ring_stop,
        frame: pad_ring_frame,
    };

extern "C" fn pad_strip_source(_d: *mut c_void, _s: *mut zwp_tablet_pad_strip_v2, _src: u32) {}
extern "C" fn pad_strip_position(
    data: *mut c_void,
    _s: *mut zwp_tablet_pad_strip_v2,
    position: u32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    // Plain integer 0..=65535 here, NOT wl_fixed like the ring above.
    window.tablet_pad.touch_ring = (position as f32 / 65535.0).clamp(0.0, 1.0);
    window.tablet_pad.touch_ring_active = true;
}
extern "C" fn pad_strip_stop(data: *mut c_void, _s: *mut zwp_tablet_pad_strip_v2) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.tablet_pad.touch_ring_active = false;
}
extern "C" fn pad_strip_frame(data: *mut c_void, _s: *mut zwp_tablet_pad_strip_v2, _time: u32) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.handle_tablet_pad_frame();
}
static ZWP_TABLET_PAD_STRIP_LISTENER: zwp_tablet_pad_strip_v2_listener =
    zwp_tablet_pad_strip_v2_listener {
        source: pad_strip_source,
        position: pad_strip_position,
        stop: pad_strip_stop,
        frame: pad_strip_frame,
    };
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
extern "C" fn tool_wheel(
    _d: *mut c_void,
    _t: *mut zwp_tablet_tool_v2,
    _degrees: i32,
    _clicks: i32,
) {
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

// ===== File drag-and-drop DESTINATION (wl_data_device) =====

/// DnD MIME type we accept as a file drop target.
const URI_LIST_MIME: &str = "text/uri-list";
/// wl_data_device_manager.dnd_action: copy (bit 1).
const WL_DATA_DEVICE_MANAGER_DND_ACTION_COPY: u32 = 1;

/// Live state for an in-progress drag over our surface (one drag at a time).
#[derive(Default)]
pub struct WaylandDragState {
    /// The current incoming `wl_data_offer` (set by `data_offer`, consumed/
    /// destroyed on leave or drop).
    pub offer: *mut wl_data_offer,
    /// Serial from the most recent `enter` — required to `accept` the offer.
    pub enter_serial: u32,
    /// Whether the current DRAG offer advertised `text/uri-list` (i.e. droppable
    /// files). Only `data_device.enter` may set this, from
    /// [`Self::pending_has_uri_list`].
    pub has_uri_list: bool,
    /// Last drag position (window-local pixels), updated on enter/motion.
    pub position: azul_core::geom::LogicalPosition,
    /// The offer whose `wl_data_offer.offer` mime advertisements are currently
    /// arriving. An offer's mime list is announced BEFORE the `enter` or
    /// `selection` that reveals what the offer is FOR, so the advertisements
    /// have to be accumulated against the offer itself.
    pub pending_offer: *mut wl_data_offer,
    /// Whether [`Self::pending_offer`] advertised `text/uri-list`.
    pub pending_has_uri_list: bool,
    /// EVERY mime type [`Self::pending_offer`] advertised, in the order the
    /// source listed them.
    ///
    /// A clipboard read has to know this: `wl_data_offer.receive` with a mime
    /// the source never offered is answered by a pipe that only closes when
    /// the source feels like it, so probing blind costs the full transfer
    /// deadline per guess. With the list, a payload read asks for exactly the
    /// flavors that are actually there.
    pub pending_mimes: Vec<String>,
    /// The advertised mime list of the offer that turned out to be the
    /// CLIPBOARD selection, promoted from [`Self::pending_mimes`].
    ///
    /// Separate from `pending_mimes` for the same reason `has_uri_list` is
    /// separate from `pending_has_uri_list`: offers arrive for every clipboard
    /// change in every other application, so the list has to be captured when
    /// `selection` names the offer, not whenever the last advertisement
    /// happened to land.
    pub clipboard_mimes: Vec<String>,
}

impl WaylandDragState {
    /// `wl_data_device.data_offer`: a new offer exists. It may turn out to be a
    /// drag or a clipboard selection; nothing says which yet.
    ///
    /// This event fires for EVERY incoming offer, including every clipboard
    /// change in every OTHER application. Resetting `has_uri_list` here is what
    /// made a mid-drag clipboard change stop `data_device_motion` from
    /// accepting, so the drop was refused.
    pub(super) fn begin_offer(&mut self, id: *mut wl_data_offer) {
        self.pending_offer = id;
        self.pending_has_uri_list = false;
        self.pending_mimes.clear();
    }

    /// `wl_data_device.selection`: THIS offer is the clipboard. Promote its
    /// advertised mime list, the same way `begin_drag` promotes
    /// `pending_has_uri_list`.
    ///
    /// A null offer means the selection was cleared — nothing is on offer, so
    /// the list empties rather than going stale.
    pub(super) fn begin_selection(&mut self, id: *mut wl_data_offer) {
        self.clipboard_mimes = if !id.is_null() && id == self.pending_offer {
            self.pending_mimes.clone()
        } else {
            Vec::new()
        };
    }

    /// The mime types the current clipboard offer advertised.
    pub(super) fn clipboard_mimes(&self) -> &[String] {
        &self.clipboard_mimes
    }

    /// `wl_data_offer.offer`: one advertised mime type of `offer`. An offer's
    /// mime list arrives BEFORE the `enter`/`selection` that reveals what the
    /// offer is for, so it is accumulated against the offer itself.
    pub(super) fn note_offered_mime(&mut self, offer: *mut wl_data_offer, mime: &str) {
        if offer != self.pending_offer {
            return;
        }
        if mime == URI_LIST_MIME {
            self.pending_has_uri_list = true;
        }
        // Bounded: a source is free to advertise as many types as it likes,
        // and this list is held for as long as the offer is. A real clipboard
        // offers a handful — Safari's eleven is the most anything sane does.
        const MAX_ADVERTISED_MIMES: usize = 64;
        if self.pending_mimes.len() < MAX_ADVERTISED_MIMES
            && !self.pending_mimes.iter().any(|m| m == mime)
        {
            self.pending_mimes.push(mime.to_owned());
        }
    }

    /// `wl_data_device.enter`: THIS offer is a drag. The only writer of
    /// `has_uri_list` — and it promotes the advertisement of this offer, never
    /// of whatever clipboard offer happened to arrive last.
    pub(super) fn begin_drag(&mut self, id: *mut wl_data_offer) {
        self.has_uri_list = !id.is_null() && id == self.pending_offer && self.pending_has_uri_list;
    }
}

/// Create the wl_data_device once both the manager and the seat are bound
/// (idempotent; called from both registry arms in any order — mirrors
/// `try_init_tablet`).
pub(super) unsafe fn try_init_data_device(window: &mut WaylandWindow, data: *mut c_void) {
    if window.data_device_initialized
        || window.data_device_manager.is_null()
        || window.seat.is_null()
    {
        return;
    }
    let dev = (window.wayland.wl_data_device_manager_get_data_device)(
        window.data_device_manager,
        window.seat,
    );
    window.data_device = dev;
    (window.wayland.wl_data_device_add_listener)(dev, &WL_DATA_DEVICE_LISTENER, data);
    window.track_listener(dev);
    window.data_device_initialized = true;
}

// --- wl_data_offer events ---
extern "C" fn data_offer_offer(
    data: *mut c_void,
    offer: *mut wl_data_offer,
    mime_type: *const c_char,
) {
    if mime_type.is_null() {
        return;
    }
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    let mime = unsafe { CStr::from_ptr(mime_type).to_str().unwrap_or_default() };
    // Record against the OFFER, not the drag. Whether this offer is a drag or a
    // clipboard selection is not known until `enter` / `selection` arrives.
    window.drag.note_offered_mime(offer, mime);
}
extern "C" fn data_offer_source_actions(
    _data: *mut c_void,
    _offer: *mut wl_data_offer,
    _source_actions: u32,
) {
}
extern "C" fn data_offer_action(_data: *mut c_void, _offer: *mut wl_data_offer, _dnd_action: u32) {}
static WL_DATA_OFFER_LISTENER: wl_data_offer_listener = wl_data_offer_listener {
    offer: data_offer_offer,
    source_actions: data_offer_source_actions,
    action: data_offer_action,
};

// --- wl_data_device events ---
/// A new data offer is incoming — attach the offer listener so its advertised
/// MIME types arrive (via `offer`) before the `enter`/`selection` that uses it.
extern "C" fn data_device_data_offer(
    data: *mut c_void,
    _dev: *mut wl_data_device,
    id: *mut wl_data_offer,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    // Start accumulating THIS offer's mime advertisements; they follow
    // immediately. `drag.has_uri_list` is deliberately untouched: every clipboard
    // change in any other app also delivers an offer here, and resetting the drag
    // flag from it made a mid-drag clipboard change stop `data_device_motion`
    // from accepting — the drop was then refused.
    window.drag.begin_offer(id);
    unsafe { (window.wayland.wl_data_offer_add_listener)(id, &WL_DATA_OFFER_LISTENER, data) };
}

/// Dispose of a `wl_data_offer`: send the destroy REQUEST, then free the
/// local PROXY. Both halves, always, in that order.
///
/// This is what libwayland's own generated `wl_data_offer_destroy` does, and
/// getting only half of it wrong disconnects the client:
///
/// * Skipping `wl_proxy_destroy` leaves the id in the CLIENT's object map.
///   Offer ids are SERVER-allocated (they start at 0xFF000000), and the
///   server reuses them; the next `wl_data_device.data_offer` carrying a
///   recycled id makes libwayland's demarshaller find the id already
///   occupied and raise
///   `not a valid new object id (4278190080), message data_offer(n)`.
///   That is a protocol error, so the compositor drops the connection: the
///   window VANISHES while the process keeps running. Selecting text was
///   enough to hit it, because every clipboard change delivers a fresh
///   offer through `selection`.
/// * Skipping the destroy request leaks the object SERVER-side instead.
///
/// The three call sites (`selection`, `leave`, `drop`) each used to open-code
/// this and each got a different half of it right. Routing them through one
/// function is the fix; `destroys_the_request_and_the_proxy_in_that_order`
/// pins the pair.
unsafe fn destroy_data_offer(window: &WaylandWindow, offer: *mut wl_data_offer) {
    destroy_data_offer_raw(
        std::mem::transmute(window.wayland.wl_proxy_marshal),
        window.wayland.wl_proxy_destroy,
        offer,
    );
}

/// [`destroy_data_offer`], reachable from `WaylandWindow::drop`.
pub(super) unsafe fn destroy_data_offer_for_teardown(
    window: &WaylandWindow,
    offer: *mut wl_data_offer,
) {
    destroy_data_offer(window, offer);
}

/// [`destroy_data_offer`] against explicit libwayland entry points, so the
/// pair can be exercised without a compositor (see the tests below).
unsafe fn destroy_data_offer_raw(
    marshal: unsafe extern "C" fn(*mut wl_proxy, u32),
    proxy_destroy: unsafe extern "C" fn(*mut wl_proxy),
    offer: *mut wl_data_offer,
) {
    if offer.is_null() {
        return;
    }
    // wl_data_offer.destroy: opcode 2, signature "".
    marshal(offer as *mut wl_proxy, 2);
    proxy_destroy(offer as *mut wl_proxy);
}

/// Marshal `wl_data_offer.accept(serial, mime_type)` — opcode 0, signature "u?s".
unsafe fn data_offer_accept(window: &WaylandWindow, offer: *mut wl_data_offer, serial: u32) {
    let mime = std::ffi::CString::new(URI_LIST_MIME).unwrap();
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, u32, *const c_char) =
        std::mem::transmute(window.wayland.wl_proxy_marshal);
    f(offer as *mut wl_proxy, 0, serial, mime.as_ptr());
}

/// Marshal `wl_data_offer.set_actions(dnd_actions, preferred)` — opcode 4 (v3+).
unsafe fn data_offer_set_actions(window: &WaylandWindow, offer: *mut wl_data_offer) {
    if window.data_device_version < 3 {
        return;
    }
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, u32, u32) =
        std::mem::transmute(window.wayland.wl_proxy_marshal);
    f(
        offer as *mut wl_proxy,
        4,
        WL_DATA_DEVICE_MANAGER_DND_ACTION_COPY,
        WL_DATA_DEVICE_MANAGER_DND_ACTION_COPY,
    );
}

extern "C" fn data_device_enter(
    data: *mut c_void,
    _dev: *mut wl_data_device,
    serial: u32,
    _surface: *mut wl_surface,
    x: i32,
    y: i32,
    id: *mut wl_data_offer,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.drag.offer = id;
    window.drag.enter_serial = serial;
    // This is the event that says "that offer is a DRAG" — promote its
    // accumulated mime advertisement now.
    window.drag.begin_drag(id);
    // wl_fixed (24.8) -> logical pixels.
    let pos = azul_core::geom::LogicalPosition::new(x as f32 / 256.0, y as f32 / 256.0);
    window.drag.position = pos;
    if id.is_null() {
        return;
    }
    // MUST accept the offer (and set DnD actions on v3+) or the compositor
    // rejects the drop. Only accept if the source actually offered files.
    if window.drag.has_uri_list {
        unsafe {
            data_offer_accept(window, id, serial);
            data_offer_set_actions(window, id);
        }
        // MWA-C-file_drop: file identity is unknown until drop on Wayland —
        // reading the offer (wl_data_offer.receive) is a blocking pipe
        // round-trip the source app may not answer before the user releases,
        // so hover carries a "<file>" placeholder (same convention as X11
        // before its speculative fetch returns) and the real paths arrive
        // with the drop in data_device_drop.
        let r = window.handle_file_drag_entered(pos, vec!["<file>".to_string()]);
        window.handle_process_event_result(r);
    } else {
        // Decline: accept(serial, NULL) clears the selection.
        unsafe {
            let f: unsafe extern "C" fn(*mut wl_proxy, u32, u32, *const c_char) =
                std::mem::transmute(window.wayland.wl_proxy_marshal);
            f(id as *mut wl_proxy, 0, serial, std::ptr::null());
        }
    }
}

extern "C" fn data_device_motion(
    data: *mut c_void,
    _dev: *mut wl_data_device,
    _time: u32,
    x: i32,
    y: i32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    let pos = azul_core::geom::LogicalPosition::new(x as f32 / 256.0, y as f32 / 256.0);
    window.drag.position = pos;
    if window.drag.has_uri_list && !window.drag.offer.is_null() {
        // Re-accept with the saved enter serial (compositors expect a response
        // on each motion to keep the drag alive). "<file>" placeholder: see
        // data_device_enter — paths are only readable at drop.
        unsafe { data_offer_accept(window, window.drag.offer, window.drag.enter_serial) };
        let r = window.handle_file_drag_entered(pos, vec!["<file>".to_string()]);
        window.handle_process_event_result(r);
    }
}

extern "C" fn data_device_leave(data: *mut c_void, _dev: *mut wl_data_device) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    unsafe { destroy_data_offer(window, window.drag.offer) };
    window.drag.offer = std::ptr::null_mut();
    window.drag.has_uri_list = false;
    let r = window.handle_file_drag_exited();
    window.handle_process_event_result(r);
}

extern "C" fn data_device_drop(data: *mut c_void, _dev: *mut wl_data_device) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    let offer = window.drag.offer;
    let pos = window.drag.position;
    if offer.is_null() || !window.drag.has_uri_list {
        return;
    }

    // Receive the text/uri-list payload through a pipe: receive(mime, write_fd)
    // [opcode 1, "sh"], flush, close write end, read read end to EOF.
    let paths = unsafe { receive_uri_list(window, offer) };

    // v3+: finish() [opcode 3] before the destroy pair.
    unsafe {
        if window.data_device_version >= 3 {
            let f: unsafe extern "C" fn(*mut wl_proxy, u32) =
                std::mem::transmute(window.wayland.wl_proxy_marshal);
            f(offer as *mut wl_proxy, 3);
        }
        destroy_data_offer(window, offer);
    }
    window.drag.offer = std::ptr::null_mut();
    window.drag.has_uri_list = false;

    if !paths.is_empty() {
        let r = window.handle_file_drop(pos, paths);
        window.handle_process_event_result(r);
    }
}

/// Ask the source to write `text/uri-list` into a pipe, read it fully, and parse
/// it into local file paths. Returns empty on any failure.
unsafe fn receive_uri_list(window: &WaylandWindow, offer: *mut wl_data_offer) -> Vec<String> {
    let bytes = receive_offer_bytes(window, offer, URI_LIST_MIME);
    let text = String::from_utf8_lossy(&bytes);
    parse_uri_list(&text)
}

/// Deadline for a `wl_data_offer` pipe transfer, ON THE WORKER THREAD. The
/// peer is another process and may never write or close.
const OFFER_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

/// How long the UI THREAD waits for a transfer before giving up on it.
///
/// The transfer itself is a pipe fed by a FOREIGN process, and when that
/// process is wedged or gone it never writes and never closes. The three-second
/// deadline above is the right budget for the transfer; it is a catastrophic
/// one for the event loop, and until this existed an ordinary Ctrl+V spent it
/// there — caret blink, tweens and rendering stalled with it. Long enough for
/// any peer that is actually alive (a real transfer is a few milliseconds),
/// short enough that a dead one costs a hitch instead of a freeze. Same budget,
/// same reasoning, as `x11/clipboard.rs::PASTE_UI_DEADLINE`.
pub(super) const PASTE_UI_DEADLINE: std::time::Duration = std::time::Duration::from_millis(400);

/// `wl_data_offer.receive` — opcode 1, signature "sh".
const WL_DATA_OFFER_RECEIVE_OPCODE: u32 = 1;

/// MWA-B3: receive an arbitrary mime payload from a `wl_data_offer` through a
/// pipe — the generalization of the DnD uri-list receive, shared with the
/// clipboard paste path (`read_wayland_selection`).
pub(super) unsafe fn receive_offer_bytes(
    window: &WaylandWindow,
    offer: *mut wl_data_offer,
    mime_type: &str,
) -> Vec<u8> {
    receive_from_offer(
        window,
        offer as *mut wl_proxy,
        WL_DATA_OFFER_RECEIVE_OPCODE,
        mime_type,
    )
}

/// [`receive_offer_bytes`] for ANY offer object. The primary-selection offer
/// has the same `receive(mime, fd)` shape but a different opcode (0, not 1),
/// which is the only thing that differs between the two protocols here.
///
/// The libwayland half — allocate the pipe, marshal `receive`, flush — stays on
/// the UI thread, because a proxy call from another thread would race the
/// single-threaded event loop. Only the DRAIN, which is the part that can block
/// for seconds, is handed off.
pub(super) unsafe fn receive_from_offer(
    window: &WaylandWindow,
    offer: *mut wl_proxy,
    receive_opcode: u32,
    mime_type: &str,
) -> Vec<u8> {
    let mut fds = [0i32; 2];
    if libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) != 0 {
        return Vec::new();
    }
    let (read_fd, write_fd) = (fds[0], fds[1]);

    let Ok(mime) = std::ffi::CString::new(mime_type) else {
        libc::close(read_fd);
        libc::close(write_fd);
        return Vec::new();
    };
    let f: unsafe extern "C" fn(*mut wl_proxy, u32, *const c_char, i32) =
        std::mem::transmute(window.wayland.wl_proxy_marshal);
    f(offer, receive_opcode, mime.as_ptr(), write_fd);

    // Flush BEFORE closing the write fd, otherwise the request never reaches the
    // server and the read end blocks forever (deadlock).
    (window.wayland.wl_display_flush)(window.display);
    libc::close(write_fd);

    drain_offer_pipe_off_thread(read_fd, OFFER_READ_TIMEOUT, PASTE_UI_DEADLINE, mime_type)
}

/// Work handed to the Wayland transfer worker: drain this pipe and answer.
struct TransferJob {
    read_fd: i32,
    timeout: std::time::Duration,
    mime_type: String,
    reply: std::sync::mpsc::SyncSender<Vec<u8>>,
}

/// Handle to the (lazily spawned) transfer worker.
///
/// One long-lived thread rather than one per paste: a wedged peer must not be
/// able to pile up threads, and serializing the transfers means an abandoned
/// one is still draining while the next is queued rather than racing it.
fn transfer_worker() -> Option<std::sync::MutexGuard<'static, std::sync::mpsc::Sender<TransferJob>>>
{
    use std::sync::{mpsc, Mutex, OnceLock};
    static WORKER: OnceLock<Mutex<mpsc::Sender<TransferJob>>> = OnceLock::new();
    let m = WORKER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<TransferJob>();
        let _ = std::thread::Builder::new()
            .name("azul-wayland-transfer".to_string())
            .spawn(move || {
                while let Ok(job) = rx.recv() {
                    // `drain_offer_pipe` closes the fd on every exit path, so
                    // abandoning the reply never leaks it.
                    let bytes =
                        unsafe { drain_offer_pipe(job.read_fd, job.timeout, &job.mime_type) };
                    let _ = job.reply.try_send(bytes);
                }
            });
        Mutex::new(tx)
    });
    m.lock().ok()
}

/// Drain `read_fd` on the transfer worker and wait at most `ui_deadline` for
/// the result.
///
/// Giving up costs a paste; not giving up costs the frame loop. The worker
/// keeps draining (and closing) the abandoned fd on its own.
///
/// # Safety
/// `read_fd` must be an owned, open fd; ownership moves to the worker.
pub(super) unsafe fn drain_offer_pipe_off_thread(
    read_fd: i32,
    timeout: std::time::Duration,
    ui_deadline: std::time::Duration,
    mime_type: &str,
) -> Vec<u8> {
    let (reply, answer) = std::sync::mpsc::sync_channel(1);
    let queued = match transfer_worker() {
        Some(sender) => sender
            .send(TransferJob {
                read_fd,
                timeout,
                mime_type: mime_type.to_owned(),
                reply,
            })
            .is_ok(),
        None => false,
    };
    if !queued {
        // No worker: fall back to draining here rather than losing the paste
        // entirely. Still bounded, just by the transfer deadline.
        return drain_offer_pipe(read_fd, timeout, mime_type);
    }
    await_transfer(&answer, ui_deadline, mime_type)
}

/// Wait for the worker's answer, but never longer than `deadline`.
fn await_transfer(
    answer: &std::sync::mpsc::Receiver<Vec<u8>>,
    deadline: std::time::Duration,
    mime_type: &str,
) -> Vec<u8> {
    match answer.recv_timeout(deadline) {
        Ok(bytes) => bytes,
        Err(_) => {
            log_warn!(
                LogCategory::Platform,
                "[Wayland] '{}' source did not answer within {:?} — abandoning the transfer \
                 rather than blocking the UI thread",
                mime_type,
                deadline
            );
            Vec::new()
        }
    }
}

/// Read a `wl_data_offer` pipe to EOF, giving up after `timeout`. Closes
/// `read_fd` on every exit path.
///
/// The fd on the other end belongs to a FOREIGN process. A source that hangs,
/// is stopped, or dies without closing its write end used to freeze the whole
/// UI thread in `read()` forever — blocking calls on the UI thread are
/// forbidden, and this one was reachable from an ordinary Ctrl+V. So: a
/// non-blocking read end (the write end keeps its blocking semantics — it is
/// the source's fd and `O_NONBLOCK` there would truncate large payloads) plus a
/// poll deadline, matching the XWayland fallback's `CLIPBOARD_READ_TIMEOUT`. A
/// timeout returns whatever arrived rather than never returning.
///
/// The deadline is not the only bound that is needed. **Wayland is the one
/// platform where the payload size is unknowable in advance** — the protocol
/// hands over a pipe and never states a length, so there is no `GlobalSize` to
/// ask and no `INCR` lower bound to read. A peer that streams fast enough can
/// therefore push hundreds of megabytes into this `Vec` inside the timeout,
/// and counting the bytes as they arrive is the only defence there is. Past
/// [`MAX_FLAVOR_BYTES`] the transfer is abandoned and what arrived is
/// discarded: a truncated flavor is worse than no flavor, because the decode
/// would succeed on the prefix and paste half a document.
///
/// # Safety
/// `read_fd` must be an owned, open fd; this function closes it.
pub(super) unsafe fn drain_offer_pipe(
    read_fd: i32,
    timeout: std::time::Duration,
    mime_type: &str,
) -> Vec<u8> {
    let flags = libc::fcntl(read_fd, libc::F_GETFL, 0);
    if flags >= 0 {
        libc::fcntl(read_fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
    }

    let deadline = std::time::Instant::now() + timeout;
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    'transfer: loop {
        if buf.len() as u64 > MAX_FLAVOR_BYTES {
            log_warn!(
                LogCategory::Platform,
                "[Wayland] '{}' transfer exceeded the {}-byte cap — abandoning it. The protocol \
                 declares no length, so counting is the only bound there is.",
                mime_type,
                MAX_FLAVOR_BYTES
            );
            libc::close(read_fd);
            // Discarded, not truncated: half a document that decodes cleanly
            // is worse than nothing.
            return Vec::new();
        }
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        if remaining.is_zero() {
            log_warn!(
                LogCategory::Platform,
                "[Wayland] '{}' transfer timed out after {:?} ({} bytes read) — abandoning the \
                 pipe rather than blocking the UI thread",
                mime_type,
                timeout,
                buf.len()
            );
            break;
        }

        let mut pfd = libc::pollfd {
            fd: read_fd,
            events: libc::POLLIN,
            revents: 0,
        };
        // .max(1): a sub-millisecond remainder must still WAIT, not spin.
        let timeout_ms = remaining.as_millis().clamp(1, i32::MAX as u128) as i32;
        let ready = libc::poll(&mut pfd, 1, timeout_ms);
        if ready < 0 {
            if *libc::__errno_location() == libc::EINTR {
                continue;
            }
            break;
        }
        if ready == 0 {
            continue; // deadline check above turns this into the timeout branch
        }

        // POLLHUP with no data pending still needs the read() to observe EOF.
        loop {
            let n = libc::read(read_fd, chunk.as_mut_ptr() as *mut c_void, chunk.len());
            if n > 0 {
                buf.extend_from_slice(&chunk[..n as usize]);
            } else if n == 0 {
                break 'transfer;
            } else {
                let err = *libc::__errno_location();
                if err == libc::EINTR {
                    continue;
                }
                if err == libc::EAGAIN || err == libc::EWOULDBLOCK {
                    continue 'transfer; // drained for now — wait for more
                }
                break 'transfer;
            }
        }
    }
    libc::close(read_fd);

    buf
}

/// Parse a `text/uri-list` payload (RFC 2483) into local filesystem paths:
/// CRLF/`\n`-separated, `#` comments skipped, `file://[host]/path` stripped to
/// path + percent-decoded. Mirrors the X11 XDND parser.
fn parse_uri_list(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for raw in text.split("\r\n").flat_map(|l| l.split('\n')) {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let path = if let Some(rest) = line.strip_prefix("file://") {
            match rest.find('/') {
                Some(idx) => &rest[idx..],
                None => continue,
            }
        } else if line.starts_with('/') {
            line
        } else {
            continue;
        };
        out.push(percent_decode(path));
    }
    out
}

/// Minimal `%XX` percent-decoder; invalid escapes pass through unchanged.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

extern "C" fn data_device_selection(
    data: *mut c_void,
    _dev: *mut wl_data_device,
    id: *mut wl_data_offer,
) {
    // MWA-B3: the compositor announces the current clipboard selection
    // owner's offer here. Stash it so read_wayland_selection() can
    // receive() from it; destroy the previous offer (each selection event
    // hands over a fresh one). id == NULL means the selection was cleared.
    // This was an empty stub — pure-Wayland paste was impossible.
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    let old = window.clipboard_offer;
    if !old.is_null() && old != id {
        unsafe { destroy_data_offer(window, old) };
    }
    window.clipboard_offer = id;
    // Promote THIS offer's advertised mime list, so a payload read knows which
    // flavors are actually on offer rather than probing blind (each blind
    // guess costs the full transfer deadline).
    window.drag.begin_selection(id);
}

// --- Primary selection (zwp_primary_selection_v1) ---
//
// The Wayland spelling of the X11 select-to-copy / middle-click-to-paste
// idiom. X11 has had both halves for years (`x11/clipboard.rs::write_to_primary`
// on every selection-ending release, `get_primary_content` on middle click);
// Wayland had NEITHER, so middle-click paste under a Wayland session did
// nothing at all and a selection was invisible to every other application.

/// `zwp_primary_selection_offer_v1.destroy` — opcode **1**.
///
/// NOT the 2 of `wl_data_offer.destroy`: the primary-selection offer has no
/// `accept`/`finish` requests in front of it. Sending the wrong opcode here is
/// a protocol error that disconnects the client and makes the window vanish.
const PRIMARY_OFFER_DESTROY_OPCODE: u32 = 1;
/// `zwp_primary_selection_offer_v1.receive` — opcode **0** (`wl_data_offer`'s
/// is 1, for the same reason).
pub(super) const PRIMARY_OFFER_RECEIVE_OPCODE: u32 = 0;
/// `zwp_primary_selection_source_v1.destroy` — opcode 1.
const PRIMARY_SOURCE_DESTROY_OPCODE: u32 = 1;

static PRIMARY_SELECTION_DEVICE_LISTENER: zwp_primary_selection_device_v1_listener =
    zwp_primary_selection_device_v1_listener {
        data_offer: primary_selection_data_offer,
        selection: primary_selection_selection,
    };

static PRIMARY_SELECTION_OFFER_LISTENER: zwp_primary_selection_offer_v1_listener =
    zwp_primary_selection_offer_v1_listener {
        offer: primary_selection_offer_mime,
    };

pub(super) static PRIMARY_SELECTION_SOURCE_LISTENER: zwp_primary_selection_source_v1_listener =
    zwp_primary_selection_source_v1_listener {
        send: primary_selection_source_send,
        cancelled: primary_selection_source_cancelled,
    };

/// Create the primary-selection device once both the manager and the seat are
/// bound (idempotent; called from both registry arms in either order — mirrors
/// `try_init_data_device`).
pub(super) unsafe fn try_init_primary_selection(window: &mut WaylandWindow, data: *mut c_void) {
    if window.primary_selection_initialized
        || window.primary_selection_manager.is_null()
        || window.seat.is_null()
    {
        return;
    }
    // get_device(new_id<device>, seat): opcode 1, signature "no".
    type GetDeviceCtor = unsafe extern "C" fn(
        *mut wl_proxy,
        u32,
        *const wl_interface,
        *mut c_void,
        *mut wl_seat,
    ) -> *mut wl_proxy;
    let ctor: GetDeviceCtor = std::mem::transmute(window.wayland.wl_proxy_marshal_constructor);
    let dev = ctor(
        window.primary_selection_manager as *mut wl_proxy,
        1,
        get_primary_selection_device_v1_interface(),
        std::ptr::null_mut(),
        window.seat,
    );
    if dev.is_null() {
        return;
    }
    (window.wayland.wl_proxy_add_listener)(
        dev,
        &PRIMARY_SELECTION_DEVICE_LISTENER as *const _ as *const _,
        data,
    );
    window.primary_selection_device = dev as *mut zwp_primary_selection_device_v1;
    window.track_listener(dev);
    window.primary_selection_initialized = true;
    log_debug!(
        LogCategory::Platform,
        "[Wayland] primary selection bound — middle-click paste is live"
    );
}

/// A fresh offer was announced. Listen on it so the compositor's `offer`
/// events have somewhere to go; `selection` below decides whether we keep it.
extern "C" fn primary_selection_data_offer(
    data: *mut c_void,
    _dev: *mut zwp_primary_selection_device_v1,
    offer: *mut zwp_primary_selection_offer_v1,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    unsafe {
        (window.wayland.wl_proxy_add_listener)(
            offer as *mut wl_proxy,
            &PRIMARY_SELECTION_OFFER_LISTENER as *const _ as *const _,
            data,
        );
    }
}

/// One `offer` event per advertised mime type. Nothing to record: we ask for
/// the canonical UTF-8 plain-text spelling when reading, and a source that
/// does not have it simply answers with an empty pipe.
extern "C" fn primary_selection_offer_mime(
    _data: *mut c_void,
    _offer: *mut zwp_primary_selection_offer_v1,
    _mime: *const c_char,
) {
}

/// The current primary selection changed hands. `id == NULL` clears it.
extern "C" fn primary_selection_selection(
    data: *mut c_void,
    _dev: *mut zwp_primary_selection_device_v1,
    id: *mut zwp_primary_selection_offer_v1,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    let old = window.primary_selection_offer;
    if !old.is_null() && old != id {
        unsafe { destroy_primary_offer(window, old) };
    }
    window.primary_selection_offer = id;
}

/// Dispose of a primary-selection offer: the destroy REQUEST for the server
/// AND `wl_proxy_destroy` for the client id. Both halves, for the reason
/// spelled out on `destroy_data_offer_raw` — offer ids are server-allocated
/// and RECYCLED, and a leaked one makes the next offer a protocol error.
pub(super) unsafe fn destroy_primary_offer(
    window: &WaylandWindow,
    offer: *mut zwp_primary_selection_offer_v1,
) {
    destroy_primary_offer_raw(
        std::mem::transmute(window.wayland.wl_proxy_marshal),
        window.wayland.wl_proxy_destroy,
        offer,
    );
}

/// [`destroy_primary_offer`] against explicit libwayland entry points, so the
/// pair can be exercised without a compositor (see the tests below).
unsafe fn destroy_primary_offer_raw(
    marshal: unsafe extern "C" fn(*mut wl_proxy, u32),
    proxy_destroy: unsafe extern "C" fn(*mut wl_proxy),
    offer: *mut zwp_primary_selection_offer_v1,
) {
    if offer.is_null() {
        return;
    }
    marshal(offer as *mut wl_proxy, PRIMARY_OFFER_DESTROY_OPCODE);
    proxy_destroy(offer as *mut wl_proxy);
}

/// The compositor (on behalf of the pasting client) pulls our selected text.
extern "C" fn primary_selection_source_send(
    _data: *mut c_void,
    _source: *mut zwp_primary_selection_source_v1,
    _mime: *const c_char,
    fd: i32,
) {
    let text = super::clipboard::native_primary_text().unwrap_or_default();
    write_all_then_close(fd, text.as_bytes());
}

/// Another client claimed the primary selection: stop serving, and release our
/// source (both halves, like every destructor here).
extern "C" fn primary_selection_source_cancelled(
    data: *mut c_void,
    source: *mut zwp_primary_selection_source_v1,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    super::clipboard::clear_native_primary();
    if window.primary_selection_source == source {
        window.primary_selection_source = std::ptr::null_mut();
    }
    unsafe {
        let destroy: unsafe extern "C" fn(*mut wl_proxy, u32) =
            std::mem::transmute(window.wayland.wl_proxy_marshal);
        destroy(source as *mut wl_proxy, PRIMARY_SOURCE_DESTROY_OPCODE);
        (window.wayland.wl_proxy_destroy)(source as *mut wl_proxy);
    }
}

// --- wl_data_source events (MWA-B3: outgoing clipboard) ---

extern "C" fn data_source_target(
    _data: *mut c_void,
    _source: *mut wl_data_source,
    _mime: *const c_char,
) {
}

/// The compositor (on behalf of the pasting client) pulls our copy text.
extern "C" fn data_source_send(
    _data: *mut c_void,
    _source: *mut wl_data_source,
    mime: *const c_char,
    fd: i32,
) {
    // Serve the representation the peer ASKED for. The offered mimes are no
    // longer all spellings of one plain-text blob: a styled copy offers RTF,
    // HTML and plain text at once, and answering all three with the same
    // bytes would paste RTF source into a plain-text field.
    //
    // The fd must be closed on every path, including the ones that serve
    // nothing — an fd left open leaves the pasting client blocked until its
    // own deadline.
    let requested = if mime.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(mime).to_str().unwrap_or_default().to_owned() }
    };
    let bytes = super::clipboard::native_copy_bytes(&requested).unwrap_or_default();
    write_all_then_close(fd, &bytes);
}

/// Serve a selection: write every byte to the compositor's fd, then close it.
/// Closing is what tells the pasting client the transfer is over — an fd left
/// open leaves it blocked until its own deadline.
fn write_all_then_close(fd: i32, bytes: &[u8]) {
    let mut off = 0usize;
    while off < bytes.len() {
        let n = unsafe {
            libc::write(
                fd,
                bytes[off..].as_ptr() as *const c_void,
                bytes.len() - off,
            )
        };
        if n > 0 {
            off += n as usize;
        } else {
            let err = unsafe { *libc::__errno_location() };
            if err == libc::EINTR {
                continue;
            }
            break;
        }
    }
    unsafe { libc::close(fd) };
}

/// Another client took the selection — we no longer own the clipboard.
extern "C" fn data_source_cancelled(data: *mut c_void, source: *mut wl_data_source) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    super::clipboard::clear_native_copy();
    if window.clipboard_source == source {
        window.clipboard_source = std::ptr::null_mut();
    }
    // wl_data_source.destroy: opcode 1, signature "". BOTH halves — the
    // request for the server, wl_proxy_destroy for the client id (this site
    // leaked one proxy per cancelled clipboard source).
    unsafe {
        let destroy: unsafe extern "C" fn(*mut wl_proxy, u32) =
            std::mem::transmute(window.wayland.wl_proxy_marshal);
        destroy(source as *mut wl_proxy, 1);
        (window.wayland.wl_proxy_destroy)(source as *mut wl_proxy);
    }
}

extern "C" fn data_source_dnd_drop_performed(_data: *mut c_void, _source: *mut wl_data_source) {}
extern "C" fn data_source_dnd_finished(_data: *mut c_void, _source: *mut wl_data_source) {}
extern "C" fn data_source_action(_data: *mut c_void, _source: *mut wl_data_source, _action: u32) {}

pub(super) static WL_DATA_SOURCE_LISTENER: wl_data_source_listener = wl_data_source_listener {
    target: data_source_target,
    send: data_source_send,
    cancelled: data_source_cancelled,
    dnd_drop_performed: data_source_dnd_drop_performed,
    dnd_finished: data_source_dnd_finished,
    action: data_source_action,
};

static WL_DATA_DEVICE_LISTENER: wl_data_device_listener = wl_data_device_listener {
    data_offer: data_device_data_offer,
    enter: data_device_enter,
    leave: data_device_leave,
    motion: data_device_motion,
    drop: data_device_drop,
    selection: data_device_selection,
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
        unsafe { (window.wayland.wl_pointer_add_listener)(pointer, &WL_POINTER_LISTENER, data) };
        window.track_listener(pointer);
    }

    if capabilities & WL_SEAT_CAPABILITY_KEYBOARD != 0 {
        let keyboard = unsafe { (window.wayland.wl_seat_get_keyboard)(seat) };
        window.keyboard = keyboard;
        unsafe { (window.wayland.wl_keyboard_add_listener)(keyboard, &WL_KEYBOARD_LISTENER, data) };
        window.track_listener(keyboard);
    }

    if capabilities & WL_SEAT_CAPABILITY_TOUCH != 0 {
        let touch = unsafe { (window.wayland.wl_seat_get_touch)(seat) };
        window.touch = touch;
        unsafe { (window.wayland.wl_touch_add_listener)(touch, &WL_TOUCH_LISTENER, data) };
        window.track_listener(touch);
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
    // Compose sequences live on the LOCALE, not on the compositor's keymap, so
    // the sequencer owns its own xkb_context and survives the layout switches
    // that replace the one above. Built once: a second keymap event must not
    // throw away a sequence in flight.
    if window.keyboard_state.compose.is_none() {
        window.keyboard_state.compose = window.xkb.compose_fns().and_then(ComposeSequencer::new);
    }
}

pub(super) extern "C" fn keyboard_key_handler(
    data: *mut c_void,
    _keyboard: *mut wl_keyboard,
    serial: u32,
    _time: u32,
    key: u32,
    state: u32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    // MWA-B3: key serials are valid input serials for set_selection — store
    // BEFORE the keymap guard so Ctrl+C works even without prior clicks.
    window.last_input_serial = serial;
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
    let first_configure = !window.configured;
    window.configured = true;

    // A configure is only COMPLETE once a commit follows the ack — the ack
    // alone is half the handshake. For a SIZE-changing batch that commit is
    // the real present (new-size buffer; `resize_surface` set
    // `os_present_requested`, the frame path attaches + commits). For a
    // state-only or repeated configure there may be NOTHING to redraw: the
    // frame path's damage machinery correctly computes "no damage" and
    // commits nothing — leaving the configure PERMANENTLY un-completed.
    // Measured consequence (2026-08-08, the first fast-path field test): KWin
    // re-sent the same 1920x1008 configure at ~243/s (401 in 1.65 s), refused
    // to start interactive edge-resizes, and the run ended in EPIPE. The old
    // code was accidentally immune because it forced a FULL DOM regeneration
    // per ack, whose present always committed. The empty commit below is the
    // protocol-correct completion: no attach, no damage — it applies pending
    // state and answers the configure, nothing else.
    let size_changed_batch = std::mem::take(&mut window.configure_size_changed);
    if !first_configure && !size_changed_batch {
        unsafe { (window.wayland.wl_surface_commit)(window.surface) };
        super::wl_trace!(
            "xdg_surface.ack_configure serial={serial}: state-only — completed with an              empty commit"
        );
    }
    // ONLY the FIRST configure (the initial map) forces a full regeneration:
    // at that point WebRender has no display list for the surface, and the
    // lightweight image-only path would render an uncleared backbuffer
    // (garbage). This mirrors the X11 ConfigureNotify path.
    //
    // It used to fire on EVERY configure — and a drag-resize acks one
    // configure per pixel of movement, so this line alone forced a full DOM
    // rebuild 75 times per second REGARDLESS of what the toplevel-configure
    // handler decided, defeating the resize fast path from a second entry
    // point. Post-map, size handling belongs to xdg_toplevel_configure_handler
    // (which chooses fast/full), and non-size configures (activation, tiling
    // states) change no layout input at all.
    if first_configure {
        window
            .common
            .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
    }
    // request_redraw() raises needs_redraw so the frame that applies whatever
    // was decided actually happens.
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
    let new_frame = if states.is_null() {
        None
    } else {
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

        let _ = is_activated; // Can be used for focus indication if needed
        Some(if is_fullscreen {
            WindowFrame::Fullscreen
        } else if is_maximized {
            WindowFrame::Maximized
        } else {
            WindowFrame::Normal
        })
    };

    // A configure carries OS-driven window-state changes (size, maximize,
    // fullscreen) and every one of them has to reach the app through the shared
    // pass: snapshot the diff baseline BEFORE mutating, run the pass after.
    // Writing `current` with no snapshot and no pass is what made native
    // drag-resize / maximize fire no WindowResize at all — the NEXT handler's
    // snapshot re-based `previous` onto the already-changed state and the delta
    // was gone.
    //
    // Both conditions are decided before anything is written so the repeated
    // no-op configures (a drag-resize delivers one PER PIXEL, and the compositor
    // re-sends the current size on every state-only change) cost neither a full
    // window-state clone nor an event pass.
    let frame_changed =
        new_frame.is_some_and(|f| window.common.current_window_state().flags.frame != f);
    let size_changed = width > 0
        && height > 0
        && (width != window.common.current_window_state().size.dimensions.width as i32
            || height != window.common.current_window_state().size.dimensions.height as i32);

    if frame_changed || size_changed {
        window.snapshot_window_state_baseline("wayland.xdg_toplevel_configure_handler");
    }
    if let Some(frame) = new_frame {
        // Source = Os: the compositor has already applied the frame state, so
        // the OS-sync baseline advances with it and the next sync_window_state()
        // does not send xdg_toplevel_set_maximized straight back.
        window.common.update_window_state(
            crate::desktop::shell2::common::event::WindowStateSource::Os,
            |ws| {
                ws.flags.frame = frame;
            },
        );
    }

    // Configure census. A mouse drag-resize delivers one of these PER FRAME;
    // the E2E path delivers three per run. Comparing `configures` against
    // `pools created` is the measurement RSS map §29 asked for and never took:
    // it separates "a Drop that is not running" from "a creation path nobody
    // is tracking", without any new plumbing.
    super::CONFIGURES_SEEN.fetch_add(1, core::sync::atomic::Ordering::Relaxed);

    // If width/height are non-zero, the compositor is requesting a specific size
    if width > 0 && height > 0 {
        let current_width = window.common.current_window_state().size.dimensions.width as i32;
        let current_height = window.common.current_window_state().size.dimensions.height as i32;

        super::wl_trace!(
            "xdg_toplevel.configure {}x{} (current {}x{}) changed={} — configures={} {}",
            width,
            height,
            current_width,
            current_height,
            width != current_width || height != current_height,
            super::CONFIGURES_SEEN.load(core::sync::atomic::Ordering::Relaxed),
            super::pool_census(),
        );

        if width != current_width || height != current_height {
            super::CONFIGURES_RESIZED.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            // This configure batch changes the size: the commit that completes
            // it must carry the NEW-size buffer, so the ack handler below must
            // NOT send its empty commit (an old-size buffer surviving an empty
            // commit violates the size rules in maximized/tiled states).
            window.configure_size_changed = true;
            // Store old context for breakpoint detection
            let old_context = window.dynamic_selector_context.clone();
            let old_logical =
                azul_core::geom::LogicalSize::new(current_width as f32, current_height as f32);

            window.common.update_window_state(
                crate::desktop::shell2::common::event::WindowStateSource::Os,
                |ws| {
                    ws.size.dimensions.width = width as f32;
                    ws.size.dimensions.height = height as f32;
                },
            );
            // RESIZE POLICY (user ruling 2026-08-08): a drag delivers one
            // configure PER PIXEL (373 measured in a 5 s drag), and each full
            // regeneration costs 654-942 ms — so a resize NEVER re-invokes the
            // app's layout() unless it could observably change its result
            // (a recorded window-size query answer flips, a CSS breakpoint /
            // orientation crossing, or there is no previous layout). The fast
            // path latches ONE coalesced relayout-at-new-size per frame.
            let full = window.common.request_regeneration_for_resize(
                old_logical,
                azul_core::geom::LogicalSize::new(width as f32, height as f32),
            );
            super::wl_trace!(
                "xdg_toplevel.configure resize {}x{} -> {}x{}: {}",
                current_width,
                current_height,
                width,
                height,
                if full {
                    "FULL regeneration (boundary crossed)"
                } else {
                    "fast relayout"
                },
            );

            // Update dynamic selector context with new viewport dimensions
            window.dynamic_selector_context.viewport_width = width as f32;
            window.dynamic_selector_context.viewport_height = height as f32;
            window.dynamic_selector_context.orientation = if width > height {
                azul_css::dynamic_selector::OrientationType::Landscape
            } else {
                azul_css::dynamic_selector::OrientationType::Portrait
            };

            // Check if any CSS breakpoints were crossed
            if old_context.viewport_breakpoint_changed(
                &window.dynamic_selector_context,
                super::super::super::common::CSS_BREAKPOINTS,
            ) {
                log_debug!(
                    LogCategory::Layout,
                    "[Wayland Resize] Breakpoint crossed: {}x{} -> {}x{}",
                    old_context.viewport_width,
                    old_context.viewport_height,
                    window.dynamic_selector_context.viewport_width,
                    window.dynamic_selector_context.viewport_height
                );
            }

            // Resize the rendering surface
            window.resize_surface(width, height);
        }
    }

    // Relayout is already scheduled above (request_regeneration_for_resize keeps
    // the per-pixel drag off the full-regeneration path); this pass exists purely
    // so the app's WindowResize / frame-change callbacks actually run.
    if frame_changed || size_changed {
        let result = window.process_window_events(0);
        window.handle_process_event_result(result);
    }
}

pub(super) extern "C" fn xdg_toplevel_close_handler(
    data: *mut c_void,
    _xdg_toplevel: *mut xdg_toplevel,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };

    // xdg_toplevel.close is a REQUEST, not an order. Clearing `is_open` here made
    // the run loop drop the window without the app's close callback ever running,
    // so Alt+F4 / the titlebar X discarded unsaved work silently and no callback
    // could veto it. Same protocol as Win32 WM_CLOSE: flip `close_requested`
    // false -> true and run a pass so EventType::WindowClose fires; a callback
    // that clears the flag cancels the close.
    let outcome = window.request_window_close("wayland.xdg_toplevel_close_handler");
    window.handle_process_event_result(outcome.result);

    if outcome.confirmed {
        window.is_open = false;
    } else {
        log_debug!(
            LogCategory::Window,
            "[Wayland] xdg_toplevel.close cancelled by callback"
        );
    }
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

/// `wl_pointer.frame` closes an atomic group of pointer events. The axis events
/// of a frame are accumulated, not dispatched, so this is where a scroll
/// actually happens — one dispatch for a diagonal scroll instead of two — and
/// where the frame-scoped `axis_source` is dropped.
extern "C" fn pointer_frame_handler(data: *mut c_void, _pointer: *mut wl_pointer) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.handle_pointer_frame();
}
// MWA-C-scroll: axis_source/axis_stop were empty stubs, so every Wayland
// scroll was WheelDiscrete (touchpad deltas became velocity impulses) and
// rubber-band spring-back never triggered. axis_source arrives before the
// axis events of its frame — store it; axis_stop = fingers lifted.
extern "C" fn pointer_axis_source_handler(
    data: *mut c_void,
    _pointer: *mut wl_pointer,
    axis_source: u32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.current_axis_source = axis_source;
}
extern "C" fn pointer_axis_stop_handler(
    data: *mut c_void,
    _pointer: *mut wl_pointer,
    _time: u32,
    _axis: u32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.handle_pointer_axis_stop();
}
/// `wl_pointer.axis_discrete` — the detent count behind the frame's axis value.
/// It is the only compositor-independent scroll quantity available here, so the
/// frame flush uses it to hit the same per-notch distance as X11 / Win32.
extern "C" fn pointer_axis_discrete_handler(
    data: *mut c_void,
    _pointer: *mut wl_pointer,
    axis: u32,
    discrete: i32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.handle_pointer_axis_discrete(axis, discrete);
}

extern "C" fn keyboard_enter_handler(
    data: *mut c_void,
    _keyboard: *mut wl_keyboard,
    _serial: u32,
    _surface: *mut wl_surface,
    keys: *mut c_void,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };

    let held = unsafe { held_keycodes_from_wl_array(keys) };
    window.handle_keyboard_enter(&held);
}

/// Decode the `keys` argument of `wl_keyboard.enter` — the evdev keycodes held
/// down AT THE MOMENT focus arrives.
///
/// The compositor never replays the presses of those keys, so discarding the
/// array left a key held across the focus change (Ctrl during Alt-Tab)
/// invisible until its release, which then removed something we never added.
/// The listener types the argument `*mut c_void`; the protocol type is
/// `wl_array`, whose `size` is in BYTES.
///
/// # Safety
/// `keys` must be null or a live `wl_array` of `u32`.
pub(super) unsafe fn held_keycodes_from_wl_array(keys: *mut c_void) -> Vec<u32> {
    if keys.is_null() {
        return Vec::new();
    }
    let array = unsafe { &*(keys as *const wl_array) };
    if array.data.is_null() {
        return Vec::new();
    }
    let count = array.size / std::mem::size_of::<u32>();
    let keycodes = array.data as *const u32;
    (0..count).map(|i| unsafe { *keycodes.add(i) }).collect()
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
    data: *mut c_void,
    _keyboard: *mut wl_keyboard,
    rate: i32,
    delay: i32,
) {
    // rate = characters per second (0 = repeat disabled), delay = ms before
    // the first repeat. Was an empty stub → no key repeat at all on Wayland.
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.key_repeat_rate_ms = if rate > 0 {
        (1000 / rate.max(1)) as u32
    } else {
        0
    };
    window.key_repeat_delay_ms = delay.max(0) as u32;
}

/// Keycode translation from XKB keysym to Azul `VirtualKeyCode` — the ONLY
/// keysym→keycode entry point on this backend.
///
/// Keysyms are an X11/xkb concept that Wayland hands us verbatim, so there is one
/// table for both backends: `x11::events::keysym_to_virtual_keycode`. It is the
/// maintained one (punctuation, the full keypad, AltGr/`XK_ISO_Level3_Shift`,
/// Num/Caps Lock, Menu, Print, and the shifted digit forms folded onto their
/// unshifted codes so a press and its release resolve to the same code).
///
/// The result is deliberately an `Option`: a keysym this table does not know has
/// NO virtual key. Callers must propagate that `None` — the hand-rolled Wayland
/// table this replaced ended in `_ => VirtualKeyCode::Escape`, so every unknown
/// key pressed and released Escape, dismissing menus and firing Escape default
/// actions. Text still reaches the app for those keys: characters come from
/// `xkb_state_key_get_utf8`, which never consults this table.
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
        unsafe { CStr::from_ptr(text) }
            .to_str()
            .ok()
            .map(|s| s.to_string())
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
        unsafe { CStr::from_ptr(text) }
            .to_str()
            .ok()
            .map(|s| s.to_string())
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
        // Route through the SHARED result handler (same as the pointer / keyboard
        // paths) so ShouldIncrementalRelayout and ShouldUpdateDisplayList aren't
        // swallowed by a `_ => {}` arm, and a redraw is always requested after a DOM
        // regen. The old inline match called regenerate_layout() directly, which on
        // Wayland does NOT build/send the WebRender transaction — so committed IME
        // text only became visible on the next event (e.g. a mouse click).
        //
        // The IME commit is `EventType::Input` from the TextInputManager provider,
        // not a previous→current state diff, so a fresh baseline suppresses nothing
        // this pass owes. What it DOES suppress is somebody else's: without it this
        // was the one pass in the backend running on whatever baseline the last
        // handler left, resurrecting that handler's delta and reporting it here.
        window.snapshot_window_state_baseline("wayland.text_input_done_handler");
        let result = window.process_window_events(0);
        window.handle_process_event_result(result);
    }

    // Step 3: Update preedit display + request redraw
    if let Some(ref mut lw) = window.common.layout_window {
        if let Some(ref preedit) = preedit_text {
            lw.text_edit_manager
                .set_preedit(preedit.clone(), preedit_begin, preedit_end);
        } else {
            lw.text_edit_manager.clear_preedit();
        }
        // MWA-C-text_input: splice/restore the composition glyphs in the
        // text cache (macOS-only before) — Wayland CJK composition showed
        // only an approximate-width underline with no visible text.
        if let Some((dom_id, node_id)) = lw
            .text_edit_manager
            .get_editing_dom_id()
            .zip(lw.text_edit_manager.get_editing_node_id())
        {
            lw.apply_preedit_to_text_cache(dom_id, node_id);
        }
    }
    // Preedit changes (set or clear) need a redraw
    window.request_redraw();
}

#[cfg(test)]
mod tests {
    use super::*;

    // Recorded libwayland calls. A `wl_data_offer` must be disposed of with
    // BOTH the destroy request and `wl_proxy_destroy`; the bug this pins
    // sent only one of them, and which one differed per call site.
    static MARSHAL_CALLS: std::sync::Mutex<Vec<(usize, u32)>> = std::sync::Mutex::new(Vec::new());
    static DESTROY_CALLS: std::sync::Mutex<Vec<usize>> = std::sync::Mutex::new(Vec::new());
    /// Interleaved order: ("marshal"|"destroy", proxy).
    static ORDER: std::sync::Mutex<Vec<&'static str>> = std::sync::Mutex::new(Vec::new());

    unsafe extern "C" fn stub_marshal(p: *mut wl_proxy, opcode: u32) {
        MARSHAL_CALLS.lock().unwrap().push((p as usize, opcode));
        ORDER.lock().unwrap().push("marshal");
    }
    unsafe extern "C" fn stub_proxy_destroy(p: *mut wl_proxy) {
        DESTROY_CALLS.lock().unwrap().push(p as usize);
        ORDER.lock().unwrap().push("destroy");
    }

    /// The recorders above are process-global and `cargo test` runs tests in
    /// PARALLEL, so every test that drives the stubs has to hold this for its
    /// whole body — otherwise one test's `reset()` erases another's recording
    /// and the failure looks like a bug in the code under test.
    static RECORDER: std::sync::Mutex<()> = std::sync::Mutex::new(());

    /// Clear the recorders and take the lock that keeps them ours.
    ///
    /// Poison-tolerant throughout: an assertion in one of these tests unwinds
    /// while holding a recorder's guard, which POISONS it, and a bare
    /// `.unwrap()` here would turn one real failure into three — the next two
    /// tests failing on the poison instead of on anything they tested.
    #[must_use]
    fn reset() -> std::sync::MutexGuard<'static, ()> {
        let guard = RECORDER.lock().unwrap_or_else(|e| e.into_inner());
        MARSHAL_CALLS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        DESTROY_CALLS
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        ORDER.lock().unwrap_or_else(|e| e.into_inner()).clear();
        guard
    }

    /// Split this file into top-level item bodies, keyed by the item's first
    /// line. Everything in here is at column 0, so a line that starts with a
    /// non-space, non-`}` character opens the next item.
    fn top_level_items(source: &str) -> Vec<(String, String)> {
        let mut items: Vec<(String, String)> = Vec::new();
        let mut header: Option<String> = None;
        let mut body = String::new();
        for line in source.lines() {
            let opens_item = !line.is_empty()
                && !line.starts_with(char::is_whitespace)
                && !line.starts_with('}')
                && !line.starts_with("//")
                && !line.starts_with('#');
            if opens_item {
                if let Some(h) = header.take() {
                    items.push((h, std::mem::take(&mut body)));
                }
                header = Some(line.to_string());
            }
            if header.is_some() {
                body.push_str(line);
                body.push('\n');
            }
        }
        if let Some(h) = header {
            items.push((h, body));
        }
        items
    }

    /// EVERY event pass in this file must open on a fresh event-diff baseline.
    ///
    /// `process_window_events` derives its events from previous -> current. A
    /// pass that runs on whatever baseline the LAST handler happened to leave
    /// behind resurrects that handler's unconsumed delta and reports it as if
    /// it had just happened here — a resize attributed to an IME commit, a
    /// cursor move attributed to a configure. `text_input_done_handler` was
    /// the one pass in the backend doing exactly that.
    ///
    /// The sanctioned openers are the named `PlatformWindow` helpers, which
    /// carry a site string and route through `check_input_delta_consumed`.
    ///
    /// NEGATIVE CONTROL: delete the
    /// `snapshot_window_state_baseline("wayland.text_input_done_handler")`
    /// line and this reports that handler.
    #[test]
    fn every_event_pass_in_this_file_opens_on_a_fresh_baseline() {
        const OPENERS: [&str; 3] = [
            "snapshot_window_state_baseline",
            "seed_window_state_baseline",
            "discard_input_delta",
        ];

        let mut offenders = Vec::new();
        for (header, body) in top_level_items(include_str!("events.rs")) {
            if header.starts_with("mod tests") {
                continue;
            }
            let Some(pass_at) = body.find("process_window_events(") else {
                continue;
            };
            let before_pass = &body[..pass_at];
            if !OPENERS.iter().any(|opener| before_pass.contains(opener)) {
                offenders.push(header);
            }
        }

        assert!(
            offenders.is_empty(),
            "these handlers run an event pass on a stale event-diff baseline: {offenders:#?}"
        );
    }

    // --- primary selection (zwp_primary_selection_v1) ---

    /// Decode a hand-rolled `wl_interface` back into what libwayland will read
    /// out of it.
    unsafe fn describe(
        i: &wl_interface,
    ) -> (String, i32, Vec<(String, String)>, Vec<(String, String)>) {
        let name = CStr::from_ptr(i.name).to_string_lossy().into_owned();
        let read = |ptr: *const wl_message, count: i32| -> Vec<(String, String)> {
            (0..count)
                .map(|k| {
                    let m = &*ptr.offset(k as isize);
                    (
                        CStr::from_ptr(m.name).to_string_lossy().into_owned(),
                        CStr::from_ptr(m.signature).to_string_lossy().into_owned(),
                    )
                })
                .collect()
        };
        let methods = read(i.methods, i.method_count);
        let events = if i.event_count > 0 {
            read(i.events, i.event_count)
        } else {
            Vec::new()
        };
        (name, i.version, methods, events)
    }

    /// The hand-rolled primary-selection interfaces, against
    /// `primary-selection-unstable-v1.xml`.
    ///
    /// libwayland indexes requests and events BY POSITION, so a reordering or
    /// a wrong signature is not a subtle bug: it marshals the wrong opcode,
    /// the compositor raises a protocol error, and the client is disconnected —
    /// the window simply vanishes. There is no compositor in CI to catch that,
    /// so the table is checked here instead.
    ///
    /// NEGATIVE CONTROL: swap `receive` and `destroy` in
    /// `get_primary_selection_offer_v1_interface` — the offer assertion fails.
    #[test]
    fn the_primary_selection_interfaces_match_the_protocol() {
        unsafe {
            let (name, version, methods, events) =
                describe(get_primary_selection_device_manager_v1_interface());
            assert_eq!(name, "zwp_primary_selection_device_manager_v1");
            assert_eq!(version, 1);
            assert_eq!(
                methods,
                vec![
                    ("create_source".to_string(), "n".to_string()),
                    ("get_device".to_string(), "no".to_string()),
                    ("destroy".to_string(), String::new()),
                ]
            );
            assert!(events.is_empty());

            let (name, version, methods, events) =
                describe(get_primary_selection_device_v1_interface());
            assert_eq!(name, "zwp_primary_selection_device_v1");
            assert_eq!(version, 1);
            assert_eq!(
                methods,
                vec![
                    ("set_selection".to_string(), "?ou".to_string()),
                    ("destroy".to_string(), String::new()),
                ]
            );
            assert_eq!(
                events,
                vec![
                    ("data_offer".to_string(), "n".to_string()),
                    ("selection".to_string(), "?o".to_string()),
                ]
            );

            let (name, version, methods, events) =
                describe(get_primary_selection_offer_v1_interface());
            assert_eq!(name, "zwp_primary_selection_offer_v1");
            assert_eq!(version, 1);
            assert_eq!(
                methods,
                vec![
                    ("receive".to_string(), "sh".to_string()),
                    ("destroy".to_string(), String::new()),
                ]
            );
            assert_eq!(events, vec![("offer".to_string(), "s".to_string())]);

            let (name, version, methods, events) =
                describe(get_primary_selection_source_v1_interface());
            assert_eq!(name, "zwp_primary_selection_source_v1");
            assert_eq!(version, 1);
            assert_eq!(
                methods,
                vec![
                    ("offer".to_string(), "s".to_string()),
                    ("destroy".to_string(), String::new()),
                ]
            );
            assert_eq!(
                events,
                vec![
                    ("send".to_string(), "sh".to_string()),
                    ("cancelled".to_string(), String::new()),
                ]
            );
        }
    }

    /// The `data_offer` event carries a `new_id`: libwayland allocates the
    /// proxy itself and needs the interface pointer to do it. A NULL there is
    /// a segfault inside libwayland the first time anything is copied.
    ///
    /// NEGATIVE CONTROL: change that message's `types` to `n` (the null table)
    /// in `get_primary_selection_device_v1_interface`.
    #[test]
    fn the_new_id_event_carries_its_interface() {
        unsafe {
            let device = get_primary_selection_device_v1_interface();
            let data_offer = &*device.methods.offset(0);
            // Requests first: set_selection takes an OBJECT, not a new_id, so
            // its type slot is legitimately null.
            assert_eq!(
                CStr::from_ptr(data_offer.name).to_string_lossy(),
                "set_selection"
            );

            let event = &*device.events.offset(0);
            assert_eq!(CStr::from_ptr(event.name).to_string_lossy(), "data_offer");
            assert!(!event.types.is_null());
            let referenced = *event.types;
            assert!(
                !referenced.is_null(),
                "data_offer's new_id has no interface"
            );
            assert_eq!(
                CStr::from_ptr((*referenced).name).to_string_lossy(),
                "zwp_primary_selection_offer_v1"
            );
        }
    }

    /// A primary-selection offer is disposed of with the destroy REQUEST and
    /// the local PROXY, in that order — and with opcode **1**, not the 2 of
    /// `wl_data_offer.destroy`. Sending the wrong opcode is a protocol error.
    ///
    /// NEGATIVE CONTROL: change `PRIMARY_OFFER_DESTROY_OPCODE` to 2, or drop
    /// either call from `destroy_primary_offer_raw`.
    #[test]
    fn a_primary_offer_is_destroyed_with_its_own_opcode_and_both_halves() {
        let _recorder = reset();
        let offer = 0x5150 as *mut zwp_primary_selection_offer_v1;
        unsafe { destroy_primary_offer_raw(stub_marshal, stub_proxy_destroy, offer) };

        // The opcode is spelled out, NOT read back from the constant: a test
        // that compares the constant with itself would pass whatever it was
        // changed to.
        assert_eq!(*MARSHAL_CALLS.lock().unwrap(), vec![(0x5150usize, 1u32)]);
        assert_eq!(*DESTROY_CALLS.lock().unwrap(), vec![0x5150usize]);
        assert_eq!(*ORDER.lock().unwrap(), vec!["marshal", "destroy"]);
    }

    /// The opcodes the marshalling uses ARE the positions in the interface
    /// table. Nothing enforces that at compile time — the constants are typed
    /// `u32` and the table is a slice — so a table edit that moves a request
    /// silently starts marshalling the wrong one.
    ///
    /// NEGATIVE CONTROL: set `PRIMARY_OFFER_RECEIVE_OPCODE` to 1.
    #[test]
    fn the_marshalled_opcodes_are_the_table_positions() {
        let (_, _, methods, _) = unsafe { describe(get_primary_selection_offer_v1_interface()) };
        let position = |name: &str| {
            methods
                .iter()
                .position(|(n, _)| n == name)
                .unwrap_or_else(|| panic!("{name} is not in the offer interface"))
                as u32
        };
        assert_eq!(PRIMARY_OFFER_RECEIVE_OPCODE, position("receive"));
        assert_eq!(PRIMARY_OFFER_DESTROY_OPCODE, position("destroy"));

        let (_, _, methods, _) = unsafe { describe(get_primary_selection_source_v1_interface()) };
        assert_eq!(
            PRIMARY_SOURCE_DESTROY_OPCODE as usize,
            methods.iter().position(|(n, _)| n == "destroy").unwrap()
        );
    }

    /// A null offer calls nothing.
    #[test]
    fn a_null_primary_offer_calls_nothing() {
        let _recorder = reset();
        unsafe {
            destroy_primary_offer_raw(stub_marshal, stub_proxy_destroy, std::ptr::null_mut())
        };
        assert!(MARSHAL_CALLS.lock().unwrap().is_empty());
        assert!(DESTROY_CALLS.lock().unwrap().is_empty());
    }

    // --- clipboard read off the UI thread (C1) ---

    /// A selection source that never writes and never closes — routine the
    /// moment the application that owns it is stopped or gone — must cost a
    /// paste, not the frame loop. The transfer deadline is three seconds and
    /// it used to be spent right here, on the UI thread, on an ordinary
    /// Ctrl+V.
    ///
    /// NEGATIVE CONTROL: call `drain_offer_pipe(read_fd, timeout, mime)`
    /// directly in place of the off-thread wait — the elapsed assertion below
    /// fails after the full three seconds.
    #[test]
    fn a_silent_wayland_source_costs_only_the_ui_deadline() {
        let mut fds = [0i32; 2];
        assert_eq!(unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) }, 0);
        let (read_fd, write_fd) = (fds[0], fds[1]);

        let started = std::time::Instant::now();
        let bytes = unsafe {
            drain_offer_pipe_off_thread(
                read_fd,
                std::time::Duration::from_secs(3),
                std::time::Duration::from_millis(80),
                "text/plain;charset=utf-8",
            )
        };
        let elapsed = started.elapsed();

        assert!(bytes.is_empty());
        assert!(
            elapsed < std::time::Duration::from_secs(1),
            "the UI thread waited {elapsed:?} on a silent source"
        );
        // Only now let the abandoned worker finish and close the read end.
        unsafe { libc::close(write_fd) };
    }

    /// A source that DOES answer is read to EOF through the worker, unchanged.
    #[test]
    fn a_wayland_source_that_answers_is_read_to_eof() {
        let mut fds = [0i32; 2];
        assert_eq!(unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) }, 0);
        let (read_fd, write_fd) = (fds[0], fds[1]);

        let payload = b"pasted text";
        unsafe {
            libc::write(write_fd, payload.as_ptr() as *const c_void, payload.len());
            libc::close(write_fd);
        }

        let bytes = unsafe {
            drain_offer_pipe_off_thread(
                read_fd,
                std::time::Duration::from_secs(3),
                std::time::Duration::from_secs(5),
                "text/plain;charset=utf-8",
            )
        };
        assert_eq!(bytes, payload.to_vec());
    }

    /// The budget the UI thread is allowed to spend on a paste. Anything
    /// approaching a second is a visible stall, not a hitch — and the transfer
    /// deadline it replaces is three seconds.
    ///
    /// NEGATIVE CONTROL: set `PASTE_UI_DEADLINE` to `OFFER_READ_TIMEOUT`.
    #[test]
    fn the_wayland_paste_budget_stays_sub_second() {
        assert!(
            PASTE_UI_DEADLINE <= std::time::Duration::from_millis(500),
            "PASTE_UI_DEADLINE = {PASTE_UI_DEADLINE:?}"
        );
        assert!(PASTE_UI_DEADLINE < OFFER_READ_TIMEOUT);
    }

    /// Negative control for the audit above: it must actually be looking at
    /// bodies that contain a pass, or an empty offender list means nothing.
    #[test]
    fn the_baseline_audit_actually_finds_the_passes() {
        let passes = top_level_items(include_str!("events.rs"))
            .into_iter()
            .filter(|(header, body)| {
                !header.starts_with("mod tests") && body.contains("process_window_events(")
            })
            .count();
        assert!(
            passes >= 3,
            "the audit only found {passes} event passes in wayland/events.rs — the item \
             splitter stopped matching this file"
        );
    }

    /// Disposing of an offer must send the destroy REQUEST and free the local
    /// PROXY, in that order.
    ///
    /// Dropping `wl_proxy_destroy` leaves the id in the client's object map.
    /// Offer ids are server-allocated and RECYCLED, so the next `data_offer`
    /// carrying a reused id makes libwayland raise "not a valid new object
    /// id" — a protocol error that disconnects the client and makes the
    /// window vanish. Dropping the request instead leaks the object
    /// server-side. Neither half is optional.
    ///
    /// NEGATIVE CONTROL: removing either call from `destroy_data_offer_raw`
    /// fails this — run and seen for both.
    #[test]
    fn destroys_the_request_and_the_proxy_in_that_order() {
        let _recorder = reset();
        let offer = 0xDEAD_BEEF_usize as *mut wl_data_offer;
        unsafe { destroy_data_offer_raw(stub_marshal, stub_proxy_destroy, offer) };

        assert_eq!(
            *MARSHAL_CALLS.lock().unwrap(),
            vec![(offer as usize, 2)],
            "the wl_data_offer.destroy request (opcode 2) must be sent to the \
             server exactly once, for this offer"
        );
        assert_eq!(
            *DESTROY_CALLS.lock().unwrap(),
            vec![offer as usize],
            "wl_proxy_destroy must free the local proxy exactly once — without \
             it the server-allocated id stays in the client's object map and \
             its next reuse is a fatal protocol error"
        );
        assert_eq!(
            *ORDER.lock().unwrap(),
            vec!["marshal", "destroy"],
            "the request has to go out BEFORE the proxy is freed; the other \
             order marshals through a dead proxy"
        );
    }

    /// A null offer is the normal "nothing to release" case (no drag in
    /// progress, selection cleared) and must not reach libwayland.
    #[test]
    fn a_null_offer_calls_nothing() {
        let _recorder = reset();
        unsafe { destroy_data_offer_raw(stub_marshal, stub_proxy_destroy, std::ptr::null_mut()) };
        assert!(MARSHAL_CALLS.lock().unwrap().is_empty());
        assert!(DESTROY_CALLS.lock().unwrap().is_empty());
    }

    fn offer(n: usize) -> *mut wl_data_offer {
        n as *mut wl_data_offer
    }

    /// `wl_data_device.data_offer` fires for EVERY incoming offer, including
    /// every clipboard change in every other running application. It must not
    /// touch the in-progress drag: doing so cleared `has_uri_list`,
    /// `data_device_motion` stopped accepting, and the drop was refused —
    /// "dragging a file in stops working if anything copies text meanwhile".
    ///
    /// NEGATIVE CONTROL: add `self.has_uri_list = false;` to `begin_offer` (the
    /// pre-fix line) — the assertion after the clipboard offer goes red.
    #[test]
    fn a_clipboard_offer_mid_drag_does_not_cancel_the_drop() {
        let mut drag = WaylandDragState::default();
        let dragged = offer(0x1001);
        let clipboard = offer(0x2002);

        drag.begin_offer(dragged);
        drag.note_offered_mime(dragged, URI_LIST_MIME);
        drag.begin_drag(dragged);
        assert!(drag.has_uri_list);

        drag.begin_offer(clipboard);
        drag.note_offered_mime(clipboard, "text/plain");
        assert!(
            drag.has_uri_list,
            "another app's clipboard change is not this drag's business"
        );
    }

    /// The mime advertisement belongs to the offer that advertised it. A
    /// clipboard offer carrying `text/uri-list` (a file manager copying a file)
    /// must not make the NEXT drag droppable when that drag offers nothing.
    ///
    /// NEGATIVE CONTROL: reduce `begin_drag` to
    /// `self.has_uri_list = self.pending_has_uri_list;` — dropping the identity
    /// check makes the second `begin_drag` inherit the first offer's flag.
    #[test]
    fn a_drag_only_inherits_its_own_mime_advertisement() {
        let mut drag = WaylandDragState::default();
        let with_files = offer(0x1001);
        let without = offer(0x2002);

        drag.begin_offer(with_files);
        drag.note_offered_mime(with_files, URI_LIST_MIME);
        drag.begin_offer(without);
        drag.begin_drag(without);
        assert!(!drag.has_uri_list);

        // ... and a mime advertised against a DIFFERENT offer than the pending
        // one is never recorded at all.
        drag.note_offered_mime(with_files, URI_LIST_MIME);
        assert!(!drag.pending_has_uri_list);
    }

    /// A drag with no offer at all (the compositor signalling "nothing here")
    /// is not droppable.
    #[test]
    fn a_null_drag_offer_is_not_droppable() {
        let mut drag = WaylandDragState::default();
        drag.begin_offer(std::ptr::null_mut());
        drag.note_offered_mime(std::ptr::null_mut(), URI_LIST_MIME);
        drag.begin_drag(std::ptr::null_mut());
        assert!(!drag.has_uri_list);
    }

    /// `wl_keyboard.enter` carries the keys already held when focus arrives;
    /// the compositor never replays their presses. Discarding the array left
    /// Ctrl-held-through-Alt-Tab invisible until a release that then removed a
    /// code nothing had added.
    ///
    /// NEGATIVE CONTROL: `return Vec::new();` as the first line of
    /// `held_keycodes_from_wl_array` — the held-key assertion goes red.
    #[test]
    fn the_enter_array_yields_every_held_keycode() {
        let mut held: Vec<u32> = vec![29, 42, 16];
        let mut array = wl_array {
            size: held.len() * std::mem::size_of::<u32>(),
            alloc: held.len() * std::mem::size_of::<u32>(),
            data: held.as_mut_ptr() as *mut c_void,
        };

        let decoded =
            unsafe { held_keycodes_from_wl_array(&mut array as *mut wl_array as *mut c_void) };
        assert_eq!(decoded, vec![29, 42, 16]);
    }

    /// `wl_array.size` is in BYTES; reading it as an element count would run
    /// four times past the end of the allocation.
    ///
    /// NEGATIVE CONTROL: `let count = array.size;` (dropping the
    /// `/ size_of::<u32>()`) returns 4 entries for a 1-key array.
    #[test]
    fn the_enter_array_size_is_bytes_not_elements() {
        let mut held: Vec<u32> = vec![58];
        let mut array = wl_array {
            size: std::mem::size_of::<u32>(),
            alloc: std::mem::size_of::<u32>(),
            data: held.as_mut_ptr() as *mut c_void,
        };
        let decoded =
            unsafe { held_keycodes_from_wl_array(&mut array as *mut wl_array as *mut c_void) };
        assert_eq!(decoded, vec![58]);
    }

    /// Focus arriving with nothing held, and the degenerate arrays a
    /// compositor may send.
    #[test]
    fn an_empty_or_null_enter_array_holds_nothing() {
        assert!(unsafe { held_keycodes_from_wl_array(std::ptr::null_mut()) }.is_empty());

        let mut empty = wl_array {
            size: 0,
            alloc: 0,
            data: std::ptr::null_mut(),
        };
        assert!(
            unsafe { held_keycodes_from_wl_array(&mut empty as *mut wl_array as *mut c_void) }
                .is_empty()
        );
    }

    /// A `wl_data_offer` pipe whose FOREIGN source never writes and never
    /// closes must not hold the UI thread. Before the deadline this was a
    /// `read()` to EOF on the main thread, reachable from an ordinary Ctrl+V.
    ///
    /// The drain runs on a worker so a regression fails this test instead of
    /// hanging the suite.
    ///
    /// NEGATIVE CONTROL: delete the `if remaining.is_zero() { … break; }`
    /// guard, or drop the `timeout_ms` argument to `poll` in favour of `-1` —
    /// the receiver times out and the test panics.
    #[test]
    fn a_silent_pipe_source_cannot_hold_the_ui_thread() {
        let mut fds = [0i32; 2];
        assert_eq!(unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) }, 0);
        let (read_fd, write_fd) = (fds[0], fds[1]);

        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let started = std::time::Instant::now();
            let got = unsafe {
                drain_offer_pipe(read_fd, std::time::Duration::from_millis(120), "text/plain")
            };
            let _ = tx.send((got, started.elapsed()));
        });

        let (got, elapsed) = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the drain must give up on its own deadline, not block forever");
        assert!(got.is_empty());
        assert!(
            elapsed >= std::time::Duration::from_millis(100),
            "it must WAIT for the deadline, not spin: {elapsed:?}"
        );
        unsafe { libc::close(write_fd) };
    }

    /// The deadline must not truncate a source that does answer: everything
    /// written before EOF has to arrive.
    ///
    /// NEGATIVE CONTROL: `break 'transfer;` in place of the
    /// `EAGAIN`/`EWOULDBLOCK` `continue 'transfer` — a payload that arrives in
    /// more than one chunk comes back short.
    #[test]
    fn a_pipe_that_answers_is_read_to_eof() {
        let mut fds = [0i32; 2];
        assert_eq!(unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) }, 0);
        let (read_fd, write_fd) = (fds[0], fds[1]);

        let payload = vec![b'z'; 200_000];
        let expected = payload.len();
        let writer = std::thread::spawn(move || {
            let mut sent = 0usize;
            while sent < payload.len() {
                let n = unsafe {
                    libc::write(
                        write_fd,
                        payload[sent..].as_ptr() as *const c_void,
                        payload.len() - sent,
                    )
                };
                if n <= 0 {
                    break;
                }
                sent += n as usize;
            }
            unsafe { libc::close(write_fd) };
        });

        let got =
            unsafe { drain_offer_pipe(read_fd, std::time::Duration::from_secs(5), "text/plain") };
        writer.join().unwrap();
        assert_eq!(got.len(), expected);
        assert!(got.iter().all(|b| *b == b'z'));
    }
}
