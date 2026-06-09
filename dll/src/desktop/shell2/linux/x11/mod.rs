//! X11 implementation for Linux using the shell2 architecture.
//!
//! The main type is [`X11Window`], which implements [`PlatformWindow`] and
//! manages the X11 display connection, event dispatch, rendering (GPU via
//! EGL/WebRender or CPU fallback), IME, tooltips, and native GNOME menus.
//!
//! Event loop entry points:
//! - [`X11Window::poll_event`] — non-blocking event poll used in the active
//!   rendering loop.
//! - [`X11Window::wait_for_events`] — blocking poll (via `poll(2)`) used when
//!   idle, also watches timerfd file descriptors.
//! - [`X11Window::render_and_present`] — full render cycle: layout
//!   regeneration, WebRender update, and buffer swap (GPU) or XPutImage (CPU).

use crate::impl_platform_window_getters;

pub mod accessibility;
pub mod clipboard;
pub mod defines;
pub mod dlopen;
pub mod events;
pub mod gl;
pub mod menu;
pub mod tooltip;

use std::{
    cell::RefCell,
    ffi::{c_void, CStr, CString},
    os::raw::{c_char, c_int},
    rc::Rc,
    sync::{Arc, Condvar, Mutex},
};

use azul_core::{
    callbacks::LayoutCallbackInfo,
    dom::DomId,
    events::{MouseButton, ProcessEventResult},
    geom::{LogicalSize, PhysicalSize},
    gl::{GlContextPtr, OptionGlContextPtr},
    hit_test::DocumentId,
    refany::RefAny,
    resources::{AppConfig, DpiScaleFactor, IdNamespace, ImageCache, RendererResources},
    window::{
        HwAcceleration, KeyboardState, Monitor, MouseCursorType, MouseState, RawWindowHandle,
        RendererType, WindowDecorations, XlibHandle,
    },
};
use azul_css::corety::OptionU32;
use azul_layout::{
    managers::hover::InputPointId,
    window::LayoutWindow,
    window_state::{FullWindowState, WindowCreateOptions},
    ScrollbarDragState,
};
use rust_fontconfig::FcFontCache;
use webrender::Renderer as WrRenderer;

use self::{
    defines::*,
    dlopen::{Egl, Gtk3Im, Library, Xkb, Xlib},
};
use super::common::gl::GlFunctions;
use crate::desktop::shell2::common::debug_server::LogCategory;
use crate::desktop::{
    shell2::common::{
        event::{self, HitTestNode, PlatformWindow},
        WindowError,
    },
    wr_translate2::{self, AsyncHitTester, Notifier, WrRenderApi},
};
use crate::{log_debug, log_error, log_info, log_trace, log_warn};

use super::super::common::CSS_BREAKPOINTS;

/// Fallback background color (blue) used when CPU rendering is not available.
const CPU_FALLBACK_BG_COLOR: std::os::raw::c_ulong = 0x0000FF;

/// X11 error handler to prevent application crashes
///
/// The default X11 error handler terminates the entire application.
/// This custom handler logs the error and allows the app to continue.
extern "C" fn x11_error_handler(_display: *mut Display, event: *mut XErrorEvent) -> c_int {
    let error = unsafe { *event };
    log_error!(
        LogCategory::Platform,
        "[X11 Error] Opcode: {}, Resource ID: {:#x}, Serial: {}, Error Code: {}",
        error.request_code,
        error.resourceid,
        error.serial,
        error.error_code
    );
    // Return 0 to indicate the error has been handled (don't terminate)
    0
}

/// Tracks the current rendering mode of the window.
enum RenderMode {
    Gpu(gl::GlContext, GlFunctions),
    Cpu(Option<GC>), // Option to hold the Graphics Context
    /// Transient teardown state: the GL context / graphics context has been
    /// dropped early (before the X window + display it depends on). Only ever
    /// set in the Drop impl; never rendered in this state.
    None,
}

/// Try to create an X11 window with a 32-bit ARGB visual for true background transparency.
///
/// This enables background-only transparency where the window background is transparent
/// (via glClearColor with alpha=0) but rendered content stays opaque.
///
/// Try to load XRandR and subscribe to screen change notifications.
/// Returns the XRandR event base if successful (screen change events = event_base + 0).
fn try_subscribe_xrandr(
    display: *mut Display,
    root: Window,
) -> Option<i32> {
    use crate::desktop::shell2::{
        common::{dlopen::load_first_available, DynamicLibrary},
        linux::x11::dlopen::Library,
    };

    type XRRQueryExtensionFn = unsafe extern "C" fn(
        *mut Display, *mut std::ffi::c_int, *mut std::ffi::c_int,
    ) -> std::ffi::c_int;
    type XRRSelectInputFn = unsafe extern "C" fn(
        *mut Display, Window, std::ffi::c_int,
    );

    const RR_SCREEN_CHANGE_NOTIFY_MASK: std::ffi::c_int = 1 << 0;

    unsafe {
        let xrandr_lib = load_first_available::<Library>(
            &["libXrandr.so.2", "libXrandr.so"],
        ).ok()?;

        let query_extension: XRRQueryExtensionFn =
            xrandr_lib.get_symbol("XRRQueryExtension").ok()?;
        let select_input: XRRSelectInputFn =
            xrandr_lib.get_symbol("XRRSelectInput").ok()?;

        let mut event_base: std::ffi::c_int = 0;
        let mut error_base: std::ffi::c_int = 0;
        if (query_extension)(display, &mut event_base, &mut error_base) == 0 {
            return None; // XRandR not available
        }

        // Subscribe to screen change events on the root window
        (select_input)(display, root, RR_SCREEN_CHANGE_NOTIFY_MASK);

        log_debug!(
            LogCategory::Platform,
            "[X11] XRandR screen change notifications enabled (event_base={})",
            event_base
        );

        // Keep the library loaded (leak it intentionally, we need the symbols alive)
        std::mem::forget(xrandr_lib);

        Some(event_base)
    }
}

/// See: https://stackoverflow.com/a/9215724 (inspired by datenwolf/FTB)
///
/// Returns: (window_handle, has_argb_visual, optional_colormap)
fn try_create_argb_window(
    xlib: &Rc<Xlib>,
    xrender: Option<&Rc<dlopen::Xrender>>,
    display: *mut Display,
    screen: std::ffi::c_int,
    root: Window,
    size: &azul_core::window::WindowSize,
    attributes: &mut XSetWindowAttributes,
    event_mask: std::ffi::c_long,
) -> Option<(Window, bool, Option<Colormap>)> {
    use defines::{
        AllocNone, CWBackPixmap, CWBorderPixel, CWColormap, CWEventMask, CWOverrideRedirect,
        InputOutput, TrueColor, XVisualInfo,
    };

    let xrender = xrender?;

    // Try to find a 32-bit TrueColor visual with alpha channel
    let mut visual_info: XVisualInfo = unsafe { std::mem::zeroed() };
    let found =
        unsafe { (xlib.XMatchVisualInfo)(display, screen, 32, TrueColor, &mut visual_info) };

    if found == 0 {
        log_debug!(
            LogCategory::Platform,
            "[X11] No 32-bit TrueColor visual found"
        );
        return None;
    }

    // Check with XRender if this visual has an alpha mask
    let pict_format = unsafe { (xrender.XRenderFindVisualFormat)(display, visual_info.visual) };

    if pict_format.is_null() {
        log_debug!(
            LogCategory::Platform,
            "[X11] XRenderFindVisualFormat returned null"
        );
        return None;
    }

    let alpha_mask = unsafe { (*pict_format).direct.alpha_mask };
    if alpha_mask == 0 {
        log_debug!(
            LogCategory::Platform,
            "[X11] Visual has no alpha mask (alphaMask=0)"
        );
        return None;
    }

    log_info!(
        LogCategory::Platform,
        "[X11] Found ARGB visual: depth={}, alphaMask={}",
        visual_info.depth,
        alpha_mask
    );

    // Create a colormap for this visual
    let colormap = unsafe { (xlib.XCreateColormap)(display, root, visual_info.visual, AllocNone) };

    if colormap == 0 {
        log_debug!(LogCategory::Platform, "[X11] XCreateColormap failed");
        return None;
    }

    // Set up window attributes for ARGB window
    attributes.colormap = colormap;
    attributes.background_pixmap = 0; // None
    attributes.border_pixel = 0;
    attributes.event_mask = event_mask;

    // CWOverrideRedirect is always in the mask so `attributes.override_redirect`
    // (0 for normal windows, 1 for menus/popups) is actually applied — without it
    // in the valuemask, XCreateWindow silently ignores the override_redirect field.
    let attr_mask =
        CWColormap | CWBackPixmap | CWBorderPixel | CWEventMask | CWOverrideRedirect;

    // Create window with ARGB visual
    let window = unsafe {
        (xlib.XCreateWindow)(
            display,
            root,
            0,
            0,
            size.dimensions.width as u32,
            size.dimensions.height as u32,
            0, // border_width
            visual_info.depth,
            InputOutput,
            visual_info.visual,
            attr_mask,
            attributes,
        )
    };

    if window == 0 {
        log_debug!(
            LogCategory::Platform,
            "[X11] XCreateWindow with ARGB visual failed"
        );
        // Clean up colormap
        unsafe { (xlib.XFreeColormap)(display, colormap) };
        return None;
    }

    log_info!(
        LogCategory::Platform,
        "[X11] Created window with ARGB visual for background transparency"
    );

    Some((window, true, Some(colormap)))
}

/// Initialise XInput2 for touch + pen/tablet. Best-effort: returns
/// `(None, 0, empty)` if libXi or the XInputExtension is unavailable (the shell
/// then falls back to core pointer events). Selects Button/Motion/Touch for all
/// master devices and maps each device's pressure/tilt valuator numbers via
/// their label atoms. ABI per scripts/WACOM_TOUCH_API_RESEARCH.md.
fn init_xinput2(
    xlib: &Rc<Xlib>,
    display: *mut Display,
    window: Window,
) -> (
    Option<Rc<dlopen::Xi>>,
    c_int,
    std::collections::HashMap<c_int, (i32, i32, i32, f64)>,
) {
    use std::collections::HashMap;
    unsafe {
        let mut opcode = 0i32;
        let (mut first_ev, mut first_err) = (0i32, 0i32);
        if (xlib.XQueryExtension)(
            display,
            b"XInputExtension\0".as_ptr() as *const _,
            &mut opcode,
            &mut first_ev,
            &mut first_err,
        ) == 0
        {
            return (None, 0, HashMap::new());
        }
        let xi = match dlopen::Xi::new() {
            Ok(x) => x,
            Err(_) => return (None, 0, HashMap::new()),
        };
        let (mut maj, mut min) = (2i32, 2i32);
        (xi.XIQueryVersion)(display, &mut maj, &mut min);

        // Select Button/Motion/Touch for all master devices.
        let mut mask = [0u8; 3];
        for e in [
            defines::XI_ButtonPress,
            defines::XI_ButtonRelease,
            defines::XI_Motion,
            defines::XI_TouchBegin,
            defines::XI_TouchUpdate,
            defines::XI_TouchEnd,
        ] {
            mask[(e >> 3) as usize] |= 1 << (e & 7);
        }
        let mut evmask = defines::XIEventMask {
            deviceid: defines::XIAllMasterDevices,
            mask_len: mask.len() as c_int,
            mask: mask.as_mut_ptr(),
        };
        (xi.XISelectEvents)(display, window, &mut evmask, 1);

        // Map deviceid -> (pressure#, tiltX#, tiltY#, pressure_max) via valuator labels.
        let p_atom = (xlib.XInternAtom)(display, b"Abs Pressure\0".as_ptr() as *const _, 0);
        let tx_atom = (xlib.XInternAtom)(display, b"Abs Tilt X\0".as_ptr() as *const _, 0);
        let ty_atom = (xlib.XInternAtom)(display, b"Abs Tilt Y\0".as_ptr() as *const _, 0);
        let mut map = HashMap::new();
        let mut ndev = 0i32;
        let devs = (xi.XIQueryDevice)(display, defines::XIAllDevices, &mut ndev);
        if !devs.is_null() {
            for i in 0..ndev as isize {
                let dev = &*devs.offset(i);
                let (mut p, mut tx, mut ty, mut pmax) = (-1i32, -1i32, -1i32, 1.0f64);
                for c in 0..dev.num_classes as isize {
                    let cls = *dev.classes.offset(c);
                    if cls.is_null() || (*cls).type_ != defines::XIValuatorClass {
                        continue;
                    }
                    let v = &*(cls as *const defines::XIValuatorClassInfo);
                    if v.label == p_atom {
                        p = v.number;
                        pmax = if v.max > 0.0 { v.max } else { 1.0 };
                    } else if v.label == tx_atom {
                        tx = v.number;
                    } else if v.label == ty_atom {
                        ty = v.number;
                    }
                }
                if p >= 0 || tx >= 0 || ty >= 0 {
                    map.insert(dev.deviceid, (p, tx, ty, pmax));
                }
            }
            (xi.XIFreeDeviceInfo)(devs);
        }
        (Some(xi), opcode, map)
    }
}

/// Decode an XI2 valuator value by valuator number. The `values` array is
/// packed (only set-mask valuators present, ascending), so the slot is the
/// count of set mask bits below `number`. See scripts/WACOM_TOUCH_API_RESEARCH.md.
unsafe fn decode_valuator(ev: &defines::XIDeviceEvent, number: i32) -> Option<f64> {
    if number < 0 {
        return None;
    }
    let vs = &ev.valuators;
    let byte = (number >> 3) as isize;
    if vs.mask.is_null() || byte >= vs.mask_len as isize {
        return None;
    }
    if *vs.mask.offset(byte) & (1 << (number & 7)) == 0 {
        return None;
    }
    let mut idx = 0isize;
    for k in 0..number {
        if *vs.mask.offset((k >> 3) as isize) & (1 << (k & 7)) != 0 {
            idx += 1;
        }
    }
    Some(*vs.values.offset(idx))
}

/// Human-readable name for an X11 core event type (for raw-event tracing).
/// Values are the stable X11 protocol numbers.
fn x11_event_name(t: i32) -> &'static str {
    match t {
        2 => "KeyPress",
        3 => "KeyRelease",
        4 => "ButtonPress",
        5 => "ButtonRelease",
        6 => "MotionNotify",
        7 => "EnterNotify",
        8 => "LeaveNotify",
        9 => "FocusIn",
        10 => "FocusOut",
        12 => "Expose",
        18 => "UnmapNotify",
        19 => "MapNotify",
        21 => "ReparentNotify",
        22 => "ConfigureNotify",
        28 => "PropertyNotify",
        33 => "ClientMessage",
        34 => "MappingNotify",
        35 => "GenericEvent",
        _ => "Other",
    }
}

/// Handle an XI2 GenericEvent.
///
/// IMPORTANT: this window `XISelectEvents`'d for XI_ButtonPress/Release/Motion
/// (see window creation), which makes the X server deliver pointer events ONLY
/// as XI2 GenericEvents and STOP sending the equivalent core ButtonPress /
/// MotionNotify to this client. So this function is the *sole* delivery path for
/// mouse button/motion — it translates them into the shared core handlers
/// (`handle_mouse_button` / `handle_mouse_move`). It also feeds pen
/// pressure/tilt and multi-touch (which core events can't express). Keyboard is
/// NOT XI-selected, so keys still arrive as core KeyPress events.
fn handle_xi_event(win: &mut X11Window, xev: &mut defines::XEvent) -> ProcessEventResult {
    let cookie = unsafe { &mut xev.xcookie };
    if cookie.extension != win.xi_opcode {
        return ProcessEventResult::DoNothing;
    }
    if win.xi.is_none() {
        return ProcessEventResult::DoNothing;
    }
    let mut result = ProcessEventResult::DoNothing;
    unsafe {
        if (win.xlib.XGetEventData)(win.display, cookie) == 0 {
            return ProcessEventResult::DoNothing;
        }
        let ev = &*(cookie.data as *const defines::XIDeviceEvent);
        let evtype = ev.evtype;
        let dpi = win
            .common
            .current_window_state
            .size
            .get_hidpi_factor()
            .inner
            .get();
        let pos = azul_core::geom::LogicalPosition::new(ev.event_x as f32 / dpi, ev.event_y as f32 / dpi);
        if evtype == defines::XI_TouchBegin
            || evtype == defines::XI_TouchUpdate
            || evtype == defines::XI_TouchEnd
        {
            // Touch: ev.detail = touch tracking id; merge into touch_state.
            let is_up = evtype == defines::XI_TouchEnd;
            let id = ev.detail as u64;
            use azul_core::window::{TouchPoint, TouchPointVec};
            let ts = &mut win.common.current_window_state.touch_state;
            let mut pts: Vec<TouchPoint> = ts.touch_points.clone().into_library_owned_vec();
            pts.retain(|p| p.id != id);
            if !is_up {
                pts.push(TouchPoint {
                    id,
                    position: pos,
                    force: 0.5,
                });
            }
            ts.touch_points = TouchPointVec::from_vec(pts);
            ts.num_touches = ts.touch_points.len();
        } else {
            // Pointer event from a mouse OR a pen/tablet. This window
            // XISelectEvents'd for XI_ButtonPress/Release/Motion at creation, so
            // the X server STOPS delivering the equivalent CORE ButtonPress /
            // MotionNotify to this client and routes them here as XI2
            // GenericEvents instead. The core dispatch arms in poll_event /
            // handle_event therefore never fire for the mouse — so we MUST
            // translate XI2 pointer events back into the shared core handlers,
            // otherwise every click / move / wheel is silently dropped (dead
            // caret, hover, drag-select, scroll).

            // Pen/tablet: additionally feed pressure/tilt for known pen devices.
            // Key the valuator map by sourceid (the originating SLAVE/physical
            // device), NOT deviceid: events selected on XIAllMasterDevices carry
            // the MASTER deviceid, but pressure/tilt valuators live on the slave
            // pen device — and pen_valuators is keyed by the slave's id from
            // XIQueryDevice. (X11 API audit, finding 9.)
            if let Some(&(p, tx, ty, pmax)) = win.pen_valuators.get(&ev.sourceid) {
                let pressure = decode_valuator(ev, p)
                    .map(|v| (v / pmax).clamp(0.0, 1.0) as f32)
                    .unwrap_or(0.0);
                let tilt_x = decode_valuator(ev, tx).unwrap_or(0.0) as f32;
                let tilt_y = decode_valuator(ev, ty).unwrap_or(0.0) as f32;
                let in_contact = pressure > 0.0;
                if let Some(lw) = win.common.layout_window.as_mut() {
                    lw.gesture_drag_manager.update_pen_state_full(
                        pos,
                        pressure,
                        (tilt_x, tilt_y),
                        in_contact,
                        false, // eraser: identified via tool/button labels, not valuators
                        false,
                        ev.deviceid as u64,
                        0.0,
                        0.0,
                        0,
                    );
                }
            }

            // Translate the XI2 pointer event into the equivalent core event and
            // run it through the same handler the core dispatch would have used.
            // Coordinates are passed as raw device pixels (as core events carry
            // them) so behaviour is identical to the core path.
            match evtype {
                defines::XI_ButtonPress | defines::XI_ButtonRelease => {
                    let btn = defines::XButtonEvent {
                        type_: if evtype == defines::XI_ButtonPress {
                            defines::ButtonPress
                        } else {
                            defines::ButtonRelease
                        },
                        serial: ev.serial,
                        send_event: ev.send_event,
                        display: ev.display,
                        window: ev.event,
                        root: ev.root,
                        subwindow: ev.child,
                        time: ev.time,
                        x: ev.event_x as c_int,
                        y: ev.event_y as c_int,
                        x_root: ev.root_x as c_int,
                        y_root: ev.root_y as c_int,
                        state: ev.mods.effective as u32,
                        button: ev.detail as u32,
                        same_screen: 1,
                    };
                    result = win.handle_mouse_button(&btn);
                }
                defines::XI_Motion => {
                    let mot = defines::XMotionEvent {
                        type_: defines::MotionNotify,
                        serial: ev.serial,
                        send_event: ev.send_event,
                        display: ev.display,
                        window: ev.event,
                        root: ev.root,
                        subwindow: ev.child,
                        time: ev.time,
                        x: ev.event_x as c_int,
                        y: ev.event_y as c_int,
                        x_root: ev.root_x as c_int,
                        y_root: ev.root_y as c_int,
                        state: ev.mods.effective as u32,
                        is_hint: 0,
                        same_screen: 1,
                    };
                    result = win.handle_mouse_move(&mot);
                }
                _ => {}
            }
        }
        (win.xlib.XFreeEventData)(win.display, cookie);
    }
    result
}

pub struct X11Window {
    pub xlib: Rc<Xlib>,
    pub egl: Rc<Egl>,
    pub xkb: Rc<Xkb>,
    pub xrender: Option<Rc<dlopen::Xrender>>, // Optional XRender for ARGB visual detection
    pub gtk_im: Option<Rc<Gtk3Im>>,           // Optional GTK IM context for IME
    pub gtk_im_context: Option<*mut dlopen::GtkIMContext>, // GTK IM context instance
    pub display: *mut Display,
    /// True if THIS window opened `display` (and must XCloseDisplay it on Drop +
    /// drain/dispatch its event queue). False for CHILD windows (menus/dialogs)
    /// that reuse a parent's display (resolved from `parent_window_id`): the
    /// OWNER drains the shared connection and dispatches each event to the
    /// target window by `XAnyEvent.window`, so children neither drain nor close it.
    pub owns_display: bool,
    pub window: Window,
    pub is_open: bool,
    wm_delete_window_atom: Atom,
    ime_manager: Option<events::ImeManager>,
    render_mode: RenderMode,
    tooltip: Option<tooltip::TooltipWindow>,
    screensaver_inhibit_cookie: Option<u32>, // D-Bus cookie for ScreenSaver.Inhibit
    dbus_connection: Option<*mut super::dbus::DBusConnection>, // D-Bus session connection

    // ARGB visual support for true background transparency
    // See: https://stackoverflow.com/a/9215724 (inspired by datenwolf/FTB)
    pub has_argb_visual: bool, // True if window was created with 32-bit ARGB visual
    pub argb_colormap: Option<Colormap>, // Custom colormap for ARGB visual (needs cleanup)

    // XInput2 touch+pen feed (None if libXi/the extension is unavailable).
    xi: Option<Rc<dlopen::Xi>>,
    xi_opcode: c_int,
    /// deviceid -> (pressure#, tiltX#, tiltY#, pressure_max); -1 = absent valuator.
    pen_valuators: std::collections::HashMap<c_int, (i32, i32, i32, f64)>,

    // Shell2 state (common fields shared with all platforms)
    pub common: event::CommonWindowState,
    new_frame_ready: Arc<(Mutex<bool>, Condvar)>,
    /// XRandR event base (if available). Screen change events have type xrandr_event_base + 0.
    pub xrandr_event_base: Option<i32>,

    // Native timer support via timerfd (Linux-specific)
    // Maps TimerId -> (timerfd file descriptor)
    // When timerfd becomes readable, the timer has fired
    pub timer_fds: std::collections::BTreeMap<usize, i32>,

    // Multi-window support
    /// Pending window creation requests (for popup menus, dialogs, etc.)
    /// Processed in Phase 3 of the event loop
    pub pending_window_creates: Vec<WindowCreateOptions>,

    // GNOME native menu V2 with dlopen (no compile-time dependency)
    pub gnome_menu: Option<super::gnome_menu::GnomeMenuManager>,

    // Shared resources
    pub resources: Arc<super::AppResources>,

    /// Dynamic selector context for evaluating conditional CSS properties
    /// (viewport size, OS, theme, etc.) - updated on resize and theme change
    pub dynamic_selector_context: azul_css::dynamic_selector::DynamicSelectorContext,

    /// Shared CPU rendering backend (same as the headless path): owns the
    /// retained pixmap, compositor, glyph cache, display-list damage diff AND the
    /// scroll-shift / eligibility / present-split machinery. Replaces the former
    /// per-backend `glyph_cache` / `retained_pixmap` / `previous_display_list`
    /// fields so X11 gets scroll fast-path + correct incremental scroll for free.
    #[cfg(feature = "cpurender")]
    cpu_backend: crate::desktop::shell2::headless::CpuBackend,
    /// Cached BGRA conversion buffer reused across CPU frames
    #[cfg(feature = "cpurender")]
    bgra_buffer: Vec<u8>,

    /// Damage rects for incremental rendering (CPU and GPU)
    gpu_damage_rects: Vec<azul_core::geom::LogicalRect>,

    /// Render-intent flag: the authoritative "a repaint is needed" signal.
    /// Set by request_redraw()/Expose/events/timers; CONSUMED by
    /// render_and_present (which renders whenever it is set). This replaces the
    /// old GPU skip-heuristic that guessed "did anything change?" from only
    /// scroll/scrollbar-fade/virtual-view and so dropped resize, caret move/
    /// blink, physics-scroll and a11y repaints.
    needs_redraw: bool,

    /// Set when this window was created with `size_to_content` (e.g. menus).
    /// The window is created UNMAPPED; the first `poll_event` lays the DOM out
    /// at a tiny viewport so the content overflows to its natural extent,
    /// measures that (`LayoutTree::get_content_size(0)` — the overflow size),
    /// resizes the window to it, then maps — so the popup appears exactly
    /// content-sized with no tiny-window flash. Cleared after the one-time pass.
    size_to_content_pending: bool,

    // Accessibility
    /// Linux accessibility adapter
    #[cfg(feature = "a11y")]
    pub accessibility_adapter: accessibility::LinuxAccessibilityAdapter,
}

#[derive(Debug, Clone, Copy)]
pub enum X11Event {
    Redraw,
    Close,
    Other,
}

// Lifecycle methods (formerly on PlatformWindow V1 trait)
impl X11Window {
    /// Poll for the next X11 event without blocking.
    ///
    /// Checks timers/threads first, then drains the X11 event queue.
    /// Returns `Some(X11Event::Close)` if the window was closed, otherwise `None`.
    pub fn poll_event(&mut self) -> Option<X11Event> {
        // Check timers and threads before processing X11 events
        self.check_timers_and_threads();

        // Process GNOME menu DBus messages (non-blocking)
        if let Some(ref manager) = self.gnome_menu {
            manager.process_messages();
        }
        self.process_pending_menu_callbacks();

        // Force a render whenever a (re)layout is pending. X11 rendering is otherwise
        // purely Expose-driven, but a compositing WM (e.g. xfwm4, which Mint/XFCE runs by
        // default) does NOT send an Expose when the window is first mapped — so without
        // this the very first frame is never drawn and the window stays blank/black.
        // `frame_needs_regeneration` starts true and is cleared inside render_and_present,
        // so this fires once for the initial frame (and again only when a real relayout is
        // queued, e.g. resize) — it does not busy-loop.
        //
        // One-time size_to_content pass (menus/tooltips, created UNMAPPED): measure
        // the content's natural size, resize the window to it, and map — before the
        // first present, so the popup appears exactly content-sized with no flash.
        if self.size_to_content_pending {
            self.apply_size_to_content();
        }
        // Render when a relayout is pending OR a VirtualView re-render was queued
        // out-of-band (e.g. a background tile-fetch writeback called
        // trigger_all_virtual_view_rerender). X11 is otherwise Expose-driven, so
        // without the vview check the re-render would sit in the queue until some
        // unrelated event happened to repaint — the async-loaded tiles would only
        // appear on the next mouse move.
        let vview_pending = self
            .common
            .layout_window
            .as_ref()
            .map(|lw| !lw.pending_virtual_view_updates.is_empty())
            .unwrap_or(false);
        if self.common.frame_needs_regeneration || vview_pending {
            if let Err(e) = self.render_and_present() {
                log_error!(
                    LogCategory::Rendering,
                    "[X11] forced initial render_and_present failed: {:?}",
                    e
                );
            }
        }

        // Only the display OWNER drains the connection. Child windows (menus,
        // dropdowns, dialogs) share the owner's display (owns_display == false);
        // the owner drains the single shared queue and dispatches each event to
        // its target window by XAnyEvent.window. Children therefore skip draining
        // — otherwise owner and children would race on one queue and misroute
        // each other's events. (option-(b) shared-display pump.)
        if self.owns_display {
            while unsafe { (self.xlib.XPending)(self.display) } > 0 {
                let mut event: XEvent = unsafe { std::mem::zeroed() };
                unsafe { (self.xlib.XNextEvent)(self.display, &mut event) };

                let target = unsafe { event.any.window } as u64;
                if target == self.window {
                    self.handle_event(&mut event);
                } else if let Some(wptr) = unsafe { super::registry::get_window(target) } {
                    // Dispatch to the child window that owns this X window.
                    match unsafe { &mut *wptr } {
                        super::LinuxWindow::X11(child) => child.handle_event(&mut event),
                        _ => self.handle_event(&mut event),
                    };
                } else {
                    // Unknown/just-closed target — handle on self so it isn't lost.
                    self.handle_event(&mut event);
                }
            }
        }

        None
    }

    /// Swap buffers (GPU) or flush (CPU) to present the current frame.
    pub fn present(&mut self) -> Result<(), WindowError> {
        match &self.render_mode {
            RenderMode::Gpu(gl_context, _) => gl_context.swap_buffers(),
            RenderMode::Cpu(_gc) => {
                // CPU rendering is handled by render_and_present(); just flush here
                unsafe { (self.xlib.XFlush)(self.display) };
                Ok(())
            }
            // Only set transiently during Drop; never present in this state.
            RenderMode::None => Ok(()),
        }?;

        // CI testing: Exit successfully after first frame render if env var is set
        if std::env::var("AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER").is_ok() {
            log_info!(
                LogCategory::General,
                "[CI] AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER set - exiting with success"
            );
            std::process::exit(0);
        }

        Ok(())
    }

    /// Process pending accessibility actions from assistive technology (e.g. Orca)
    #[cfg(feature = "a11y")]
    pub fn process_accessibility_actions(&mut self) {
        let mut actions = Vec::new();
        while let Some(action) = self.accessibility_adapter.poll_action() {
            actions.push(action);
        }
        if actions.is_empty() {
            return;
        }

        let now = std::time::Instant::now();
        for (dom_id, node_id, action) in actions {
            if let Some(lw) = self.common.layout_window.as_mut() {
                let affected = lw.process_accessibility_action(dom_id, node_id, action, now);
                if !affected.is_empty() {
                    self.common.display_list_dirty = true;
                }
            }
        }

        self.common.a11y_dirty = true;
        self.request_redraw();
    }

    /// Request a window redraw by sending an Expose event.
    ///
    /// If GPU damage rects are available, sends per-rect Expose events
    /// for incremental invalidation; otherwise sends a full-surface Expose.
    pub fn request_redraw(&mut self) {
        // Record render intent (the authoritative signal render_and_present
        // consumes) AND post a synthetic Expose to wake the blocking event
        // loop so the repaint happens promptly. The flag is what guarantees
        // the frame actually paints; the Expose is just the wake-up.
        self.needs_redraw = true;
        // Use per-rect Expose events when damage rects available
        if !self.gpu_damage_rects.is_empty() {
            let rects: Vec<_> = self.gpu_damage_rects.drain(..).collect();
            for dr in &rects {
                let mut event: XEvent = unsafe { std::mem::zeroed() };
                let expose = unsafe { &mut event.expose };
                expose.type_ = Expose;
                expose.display = self.display;
                expose.window = self.window;
                expose.x = dr.origin.x as i32;
                expose.y = dr.origin.y as i32;
                expose.width = dr.size.width as i32 + 1;
                expose.height = dr.size.height as i32 + 1;
                unsafe {
                    (self.xlib.XSendEvent)(self.display, self.window, 0, ExposureMask, &mut event);
                }
            }
            unsafe { (self.xlib.XFlush)(self.display) };
            return;
        }

        // Full-surface redraw fallback
        let mut event: XEvent = unsafe { std::mem::zeroed() };
        let expose = unsafe { &mut event.expose };
        expose.type_ = Expose;
        expose.display = self.display;
        expose.window = self.window;
        unsafe {
            (self.xlib.XSendEvent)(self.display, self.window, 0, ExposureMask, &mut event);
            (self.xlib.XFlush)(self.display);
        }
    }

    /// Synchronize the clipboard state with the system clipboard.
    pub fn sync_clipboard(
        &mut self,
        clipboard_manager: &mut azul_layout::managers::clipboard::ClipboardManager,
    ) {
        clipboard::sync_clipboard(clipboard_manager);
    }

    /// Destroy the X11 window, free the ARGB colormap, and close the display.
    pub fn close(&mut self) {
        if self.is_open {
            self.is_open = false;
            if let Some(doc_id) = self.common.document_id {
                crate::desktop::gl_texture_integration::remove_document_textures(&doc_id);
            }
            unsafe {
                // Release a menu/popup's pointer grab on close (no-op otherwise).
                if self.common.current_window_state.flags.window_type
                    == azul_core::window::WindowType::Menu
                {
                    (self.xlib.XUngrabPointer)(self.display, defines::CurrentTime);
                }
                (self.xlib.XDestroyWindow)(self.display, self.window);
                // Free the ARGB colormap if we created one
                if let Some(colormap) = self.argb_colormap.take() {
                    (self.xlib.XFreeColormap)(self.display, colormap);
                }
                // Only close a display we OWN; child windows share a parent's.
                if self.owns_display {
                    (self.xlib.XCloseDisplay)(self.display);
                }
            }
        }
    }

    /// Returns `true` if the window has not been closed.
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    /// Returns `true` if a callback set `flags.close_requested` (e.g. a menu item
    /// click closing the menu, or a CSD close button). The run loop turns this into
    /// a `close()` — X11 has no native close-flag path.
    pub fn close_requested(&self) -> bool {
        self.common.current_window_state.flags.close_requested
    }
}

impl X11Window {
    fn ensure_layout_window_initialized(&mut self) -> Result<(), WindowError> {
        if self.common.layout_window.is_some() {
            return Ok(());
        }
        let mut layout_window =
            azul_layout::window::LayoutWindow::new((*self.resources.fc_cache).clone())
                .map_err(|e| {
                    WindowError::PlatformError(format!(
                        "Failed to create LayoutWindow: {:?}",
                        e
                    ))
                })?;

        if let Some(doc_id) = self.common.document_id {
            layout_window.document_id = doc_id;
        }
        if let Some(ns_id) = self.common.id_namespace {
            layout_window.id_namespace = ns_id;
        }
        layout_window.current_window_state = self.common.current_window_state.clone();
        layout_window.renderer_type = Some(azul_core::window::RendererType::Hardware);
        layout_window.routes = self.resources.config.routes.clone();
        if let Ok(mut guard) = layout_window.monitors.lock() {
            *guard = crate::desktop::display::get_monitors();
        }
        self.common.layout_window = Some(layout_window);
        Ok(())
    }

    /// Create a new X11 window with shared resources
    ///
    /// This is the preferred way to create X11 windows, as it allows
    /// sharing font cache, app data, and system styling across windows.
    pub fn new_with_resources(
        mut options: WindowCreateOptions,
        resources: Arc<super::AppResources>,
    ) -> Result<Self, WindowError> {
        // If background_color is None and no material effect, use system window background
        // Note: When a material is set, the renderer will use transparent clear color automatically
        if options.window_state.background_color.is_none() {
            use azul_core::window::WindowBackgroundMaterial;
            if matches!(options.window_state.flags.background_material, WindowBackgroundMaterial::Opaque) {
                options.window_state.background_color = resources.system_style.colors.window_background;
            }
            // For materials, leave background_color as None - renderer handles transparency
        }
        
        // Extract create_callback before consuming options
        let create_callback = options.create_callback.clone();

        let xlib = Xlib::new()
            .map_err(|e| WindowError::PlatformError(format!("Failed to load libX11: {:?}", e)))?;
        let egl = Egl::new()
            .map_err(|e| WindowError::PlatformError(format!("Failed to load libEGL: {:?}", e)))?;
        let xkb = Xkb::new().map_err(|e| {
            WindowError::PlatformError(format!("Failed to load libxkbcommon: {:?}", e))
        })?;

        // Set custom X11 error handler to prevent application crashes
        // The default handler terminates the app on any X protocol error
        unsafe {
            (xlib.XSetErrorHandler)(Some(x11_error_handler));
        }

        // Try to load GTK3 IM context for IME support (optional, fail silently)
        let (gtk_im, gtk_im_context) = match Gtk3Im::new() {
            Ok(gtk) => {
                log_info!(
                    LogCategory::Platform,
                    "[X11] GTK3 IM context loaded for IME support"
                );
                let ctx = unsafe { (gtk.gtk_im_context_simple_new)() };
                if !ctx.is_null() {
                    (Some(gtk), Some(ctx))
                } else {
                    log_warn!(
                        LogCategory::Platform,
                        "[X11] Failed to create GTK IM context instance"
                    );
                    (None, None)
                }
            }
            Err(e) => {
                log_debug!(
                    LogCategory::Platform,
                    "[X11] GTK3 IM not available (IME positioning disabled): {:?}",
                    e
                );
                (None, None)
            }
        };

        // Establish the C library's LC_CTYPE locale from the environment
        // BEFORE XOpenDisplay / XOpenIM. Without this the process stays in the
        // default "C" (ASCII) locale, so XmbLookupString and the whole XIM
        // machinery can neither compose dead-keys (´ + e → é) nor produce CJK
        // text — keyboard input silently degrades to ASCII regardless of the
        // user's layout/IME. Xlib reads LC_CTYPE specifically (XSupportsLocale,
        // the Xmb*/Xutf8* codeset), so only LC_CTYPE is set — LC_NUMERIC stays
        // "C" so a locale with a comma decimal separator can't break float
        // parsing in C dependencies. Process-wide, runs once.
        {
            use std::sync::Once;
            static SET_LOCALE: Once = Once::new();
            SET_LOCALE.call_once(|| unsafe {
                libc::setlocale(libc::LC_CTYPE, b"\0".as_ptr() as *const libc::c_char);
            });
        }

        // Child windows (menus/dropdowns/dialogs) REUSE the parent's display so a
        // single event pump (the owner's poll_event) drains one connection and
        // dispatches by XAnyEvent.window. The parent is identified by
        // `parent_window_id` (the window-registry key) — resolve it to the
        // parent's live display. Fall back to opening our own if there is no
        // parent or the parent isn't an X11 window.
        let shared_parent_display: Option<*mut Display> = if options.parent_window_id != 0 {
            unsafe {
                super::registry::get_window(options.parent_window_id).and_then(|wptr| {
                    match &*wptr {
                        super::LinuxWindow::X11(parent) => Some(parent.display),
                        _ => None,
                    }
                })
            }
        } else {
            None
        };
        let (display, owns_display) = match shared_parent_display {
            Some(d) => (d, false),
            None => (unsafe { (xlib.XOpenDisplay)(std::ptr::null()) }, true),
        };
        if display.is_null() {
            return Err(WindowError::PlatformError(
                "Failed to open X display".into(),
            ));
        }

        let screen = unsafe { (xlib.XDefaultScreen)(display) };
        let root = unsafe { (xlib.XRootWindow)(display, screen) };

        // Try to load XRender for ARGB visual detection (optional)
        // See: https://stackoverflow.com/a/9215724 (inspired by datenwolf/FTB)
        let xrender = dlopen::Xrender::new().ok();

        let mut attributes: XSetWindowAttributes = unsafe { std::mem::zeroed() };
        let event_mask = ExposureMask
            | KeyPressMask
            | KeyReleaseMask
            | ButtonPressMask
            | ButtonReleaseMask
            | PointerMotionMask
            | StructureNotifyMask
            | EnterWindowMask
            | LeaveWindowMask
            | FocusChangeMask;

        // Override-redirect (a borderless, WM-unmanaged popup such as a menu/tooltip)
        // is driven by the explicit window option, NOT by decorations==None: a CSD
        // main window is borderless yet must stay WM-managed (movable, in the taskbar),
        // whereas menus/popups must be override-redirect so the WM never reparents or
        // decorates them. Set declaratively via WindowCreateOptions (e.g. show_menu).
        if options
            .window_state
            .platform_specific_options
            .linux_options
            .x11_override_redirect
        {
            attributes.override_redirect = 1;
        }
        attributes.event_mask = event_mask;

        let size = options.window_state.size;
        let position = options.window_state.position;
        // Monitor ID is now stored in FullWindowState.monitor_id, not in WindowState
        // For now, we default to monitor 0
        let monitor_id = 0; // TODO: Get from options or detect primary monitor

        // Try to create window with ARGB visual for true background transparency
        // This allows background-only transparency where the background is transparent
        // but rendered content stays opaque (using glClearColor with alpha=0)
        let (window_handle, has_argb_visual, argb_colormap) = try_create_argb_window(
            &xlib,
            xrender.as_ref(),
            display,
            screen,
            root,
            &size,
            &mut attributes,
            event_mask,
        )
        .unwrap_or_else(|| {
            // Fallback to simple window without ARGB visual
            let window = unsafe {
                (xlib.XCreateSimpleWindow)(
                    display,
                    root,
                    0,
                    0,
                    size.dimensions.width as u32,
                    size.dimensions.height as u32,
                    1,
                    0,
                    0,
                )
            };
            (window, false, None)
        });

        unsafe { (xlib.XSelectInput)(display, window_handle, event_mask) };

        // XInput2: select touch + pen/tablet events (best-effort; core events otherwise).
        let (xi, xi_opcode, pen_valuators) = init_xinput2(&xlib, display, window_handle);

        let wm_delete_window_atom =
            unsafe { (xlib.XInternAtom)(display, b"WM_DELETE_WINDOW\0".as_ptr() as _, 0) };
        unsafe {
            (xlib.XSetWMProtocols)(
                display,
                window_handle,
                [wm_delete_window_atom].as_mut_ptr(),
                1,
            )
        };

        // EWMH _NET_WM_WINDOW_TYPE hint: tells the WM/compositor what kind of
        // window this is (menu/tooltip/dialog). Menus/tooltips are also
        // override_redirect (set above) so the WM doesn't manage their geometry,
        // but compositors still read this atom to apply the right effects
        // (menu drop-shadows, fade-in) and stacking. Without it a popup menu
        // is just an anonymous frameless window the compositor can't classify.
        {
            use azul_core::window::WindowType;
            let type_atom_name: Option<&[u8]> = match options.window_state.flags.window_type {
                WindowType::Menu => Some(b"_NET_WM_WINDOW_TYPE_POPUP_MENU\0".as_slice()),
                WindowType::Tooltip => Some(b"_NET_WM_WINDOW_TYPE_TOOLTIP\0".as_slice()),
                WindowType::Dialog => Some(b"_NET_WM_WINDOW_TYPE_DIALOG\0".as_slice()),
                WindowType::Normal => None,
            };
            if let Some(name) = type_atom_name {
                unsafe {
                    let net_wm_window_type = (xlib.XInternAtom)(
                        display,
                        b"_NET_WM_WINDOW_TYPE\0".as_ptr() as *const c_char,
                        0,
                    );
                    let type_atom = (xlib.XInternAtom)(
                        display,
                        name.as_ptr() as *const c_char,
                        0,
                    );
                    // format=32 properties are arrays of C `long` on the wire side
                    // of Xlib (it packs to 32-bit); pass a c_long, not a u32.
                    let type_atom_long: std::os::raw::c_long = type_atom as std::os::raw::c_long;
                    (xlib.XChangeProperty)(
                        display,
                        window_handle,
                        net_wm_window_type,
                        defines::XA_ATOM,
                        32,
                        defines::PropModeReplace,
                        &type_atom_long as *const std::os::raw::c_long as *const u8,
                        1,
                    );
                }
            }
        }

        // WM_CLASS hint (instance + class) from the window options, so the WM / taskbar
        // can identify + group the window. Property format: two NUL-terminated 8-bit
        // strings ("instance\0class\0") on the WM_CLASS atom (type STRING). A no-op for
        // override-redirect popups (the WM ignores them) but correct for normal windows.
        {
            let classes = &options
                .window_state
                .platform_specific_options
                .linux_options
                .x11_wm_classes;
            if let Some(pair) = classes.as_ref().first() {
                let mut data: Vec<u8> = Vec::new();
                data.extend_from_slice(pair.key.as_str().as_bytes());
                data.push(0);
                data.extend_from_slice(pair.value.as_str().as_bytes());
                data.push(0);
                unsafe {
                    let wm_class_atom =
                        (xlib.XInternAtom)(display, b"WM_CLASS\0".as_ptr() as *const c_char, 0);
                    let string_atom =
                        (xlib.XInternAtom)(display, b"STRING\0".as_ptr() as *const c_char, 0);
                    (xlib.XChangeProperty)(
                        display,
                        window_handle,
                        wm_class_atom,
                        string_atom,
                        8,
                        defines::PropModeReplace,
                        data.as_ptr(),
                        data.len() as i32,
                    );
                }
            }
        }

        let ime_manager = events::ImeManager::new(&xlib, display, window_handle);

        // Honor an explicit CPU-render request (AZ_BACKEND=cpu / HwAcceleration::Disabled):
        // skip the GL context and use the cpurender (XPutImage) path. Previously X11 always
        // tried GL and only fell back to CPU when GL *failed*, so AZ_BACKEND=cpu was ignored.
        // (Mirrors the Wayland force-CPU path; also avoids the very slow WebRender shader
        // compilation on software / weak GPUs such as nouveau.)
        let force_cpu = matches!(
            crate::desktop::shell2::common::compositor::AzBackend::resolve(
                options.renderer.as_option().map(|r| r.hw_accel)
            ),
            crate::desktop::shell2::common::compositor::AzBackend::Cpu
        );
        let (
            render_mode,
            renderer,
            render_api,
            hit_tester,
            document_id,
            id_namespace,
            gl_context_ptr,
        ) = if force_cpu {
            log_debug!(
                LogCategory::Platform,
                "[X11] CPU rendering requested (AZ_BACKEND=cpu) — skipping GL context (cpurender)"
            );
            let gc = unsafe { (xlib.XCreateGC)(display, window_handle, 0, std::ptr::null_mut()) };
            (RenderMode::Cpu(Some(gc)), None, None, None, None, None, None.into())
        } else {
            match gl::GlContext::new(&xlib, &egl, display, window_handle) {
            Ok(gl_context) => 'gpu: {
                gl_context.make_current();
                gl_context.configure_vsync(options.window_state.renderer_options.vsync);
                // ANY failure past this point falls back to CPU rendering in THIS
                // window — "GPU init failed" must never mean "no window".
                let gl_functions = match GlFunctions::initialize(&egl) {
                    Ok(f) => f,
                    Err(e) => {
                        crate::plog_warn!(
                            "[X11] GL function loading failed: {:?} — falling back to CPU rendering",
                            e
                        );
                        let gc = unsafe {
                            (xlib.XCreateGC)(display, window_handle, 0, std::ptr::null_mut())
                        };
                        break 'gpu (RenderMode::Cpu(Some(gc)), None, None, None, None, None, None.into());
                    }
                };

                let new_frame_ready = Arc::new((Mutex::new(false), Condvar::new()));
                let (renderer, sender) = match webrender::create_webrender_instance(
                    gl_functions.functions.clone(),
                    Box::new(Notifier {
                        new_frame_ready: new_frame_ready.clone(),
                    }),
                    wr_translate2::default_renderer_options(
                        &options,
                        wr_translate2::create_program_cache(&gl_functions.functions),
                    ),
                    None,
                ) {
                    Ok(rs) => rs,
                    Err(e) => {
                        crate::plog_warn!(
                            "[X11] WebRender init failed: {:?} — falling back to CPU rendering",
                            e
                        );
                        let gc = unsafe {
                            (xlib.XCreateGC)(display, window_handle, 0, std::ptr::null_mut())
                        };
                        break 'gpu (RenderMode::Cpu(Some(gc)), None, None, None, None, None, None.into());
                    }
                };

                let render_api = sender.create_api();
                let framebuffer_size = webrender::api::units::DeviceIntSize::new(
                    size.dimensions.width as i32,
                    size.dimensions.height as i32,
                );
                let wr_doc_id = render_api.add_document(framebuffer_size);
                let document_id = wr_translate2::translate_document_id_wr(wr_doc_id);
                let id_namespace =
                    wr_translate2::translate_id_namespace_wr(render_api.get_namespace_id());
                let hit_tester_request = render_api.request_hit_tester(wr_doc_id);
                // R1: a software GL stack (llvmpipe/swrast) presents as a real GL
                // context but can't compile the desktop GLSL-150 SVG/FXAA shaders.
                // Detect it and mark the GlContextPtr Software so those shaders are
                // skipped (see GlContextPtr::new). WebRender compositing is left as-is.
                let renderer_type = match crate::desktop::shell2::common::compositor::query_gpu_info(
                    &gl_functions.functions,
                ) {
                    crate::desktop::shell2::common::compositor::GpuCheckResult::Blacklisted {
                        ref info,
                        ref reason,
                    } => {
                        log_warn!(
                            LogCategory::Platform,
                            "[X11] software/blacklisted GL ({}): {} — skipping GPU SVG/FXAA shaders",
                            info.renderer,
                            reason
                        );
                        RendererType::Software
                    }
                    _ => RendererType::Hardware,
                };
                let gl_ptr = GlContextPtr::new(renderer_type, gl_functions.functions.clone());
                // PROVE the context: if the shaders didn't compile (broken
                // driver), is_gl_usable() is false — fall back to CPU rendering
                // for this window instead of presenting a black/garbled GPU
                // surface. (The probe lives in azul_core::gl::GlContextPtr::new.)
                let gl_usable = gl_ptr.is_gl_usable();
                if matches!(renderer_type, RendererType::Hardware) && !gl_usable {
                    crate::plog_warn!(
                        "[X11] GL context unusable (shaders failed to compile at any GLSL version) \
                         — falling back to CPU rendering for this window"
                    );
                    drop(gl_ptr);
                    let gc = unsafe {
                        (xlib.XCreateGC)(display, window_handle, 0, std::ptr::null_mut())
                    };
                    (RenderMode::Cpu(Some(gc)), None, None, None, None, None, None.into())
                } else {
                    let gl_context_ptr = OptionGlContextPtr::Some(gl_ptr);
                    log_debug!(
                        LogCategory::Platform,
                        "[X11] GPU rendering initialized ({}x{})",
                        framebuffer_size.width,
                        framebuffer_size.height
                    );
                    (
                        RenderMode::Gpu(gl_context, gl_functions),
                        Some(renderer),
                        Some(render_api),
                        Some(AsyncHitTester::Requested(hit_tester_request)),
                        Some(document_id),
                        Some(id_namespace),
                        gl_context_ptr,
                    )
                }
            }
            Err(e) => {
                log_warn!(
                    LogCategory::Platform,
                    "[X11] GL context creation failed: {:?}, falling back to CPU rendering",
                    e
                );
                let gc =
                    unsafe { (xlib.XCreateGC)(display, window_handle, 0, std::ptr::null_mut()) };
                (
                    RenderMode::Cpu(Some(gc)),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None.into(),
                )
            }
            }
        };

        let is_cpu_mode = matches!(render_mode, RenderMode::Cpu(_));
        let mut window = Self {
            xlib,
            egl,
            xkb,
            xrender,
            gtk_im,
            gtk_im_context,
            display,
            owns_display,
            window: window_handle,
            is_open: true,
            wm_delete_window_atom,
            ime_manager,
            render_mode,
            tooltip: None,
            screensaver_inhibit_cookie: None,
            dbus_connection: None,
            has_argb_visual,
            argb_colormap,
            xi,
            xi_opcode,
            pen_valuators,
            common: event::CommonWindowState {
                layout_window: None,
                current_window_state: FullWindowState {
                    title: options.window_state.title.clone(),
                    size: options.window_state.size,
                    position: options.window_state.position,
                    flags: options.window_state.flags,
                    theme: options.window_state.theme,
                    debug_state: options.window_state.debug_state,
                    keyboard_state: Default::default(),
                    mouse_state: Default::default(),
                    touch_state: Default::default(),
                    ime_position: options.window_state.ime_position,
                    platform_specific_options: options.window_state.platform_specific_options.clone(),
                    renderer_options: options.window_state.renderer_options,
                    background_color: options.window_state.background_color,
                    layout_callback: options.window_state.layout_callback,
                    close_callback: options.window_state.close_callback.clone(),
                    monitor_id: OptionU32::None,
                    window_id: options.window_state.window_id.clone(),
                    window_focused: true,
                    active_route: azul_core::resources::OptionRouteMatch::None,
                },
                previous_window_state: None,
                renderer,
                render_api,
                hit_tester,
                cpu_hit_tester: if is_cpu_mode {
                    Some(azul_layout::headless::CpuHitTester::new())
                } else {
                    None
                },
                document_id,
                id_namespace,
                image_cache: ImageCache::default(),
                renderer_resources: RendererResources::default(),
                gl_context_ptr,
                fc_cache: resources.fc_cache.clone(),
                system_style: resources.system_style.clone(),
                app_data: resources.app_data.clone(),
                scrollbar_drag_state: None,
                last_hovered_node: None,
                frame_needs_regeneration: true,
                next_relayout_reason: azul_core::callbacks::RelayoutReason::Initial,
                display_list_initialized: false,
                display_list_dirty: false,
                a11y_dirty: true,
            },
            new_frame_ready: Arc::new((Mutex::new(false), Condvar::new())),
            xrandr_event_base: None,
            timer_fds: std::collections::BTreeMap::new(),
            pending_window_creates: Vec::new(),
            gnome_menu: None, // New dlopen-based implementation
            resources: resources.clone(),
            dynamic_selector_context: {
                let mut ctx =
                    azul_css::dynamic_selector::DynamicSelectorContext::from_system_style(&resources.system_style);
                ctx.viewport_width = options.window_state.size.dimensions.width;
                ctx.viewport_height = options.window_state.size.dimensions.height;
                ctx.orientation = if ctx.viewport_width > ctx.viewport_height {
                    azul_css::dynamic_selector::OrientationType::Landscape
                } else {
                    azul_css::dynamic_selector::OrientationType::Portrait
                };
                ctx
            },
            #[cfg(feature = "cpurender")]
            cpu_backend: crate::desktop::shell2::headless::CpuBackend::new(),
            #[cfg(feature = "cpurender")]
            bgra_buffer: Vec::new(),
            gpu_damage_rects: Vec::new(),
            needs_redraw: true,
            size_to_content_pending: options.size_to_content,
            #[cfg(feature = "a11y")]
            accessibility_adapter: accessibility::LinuxAccessibilityAdapter::new(),
        };

        // Initialize accessibility adapter
        #[cfg(feature = "a11y")]
        {
            let window_name = format!("Azul Window ({})", window.window);
            window
                .accessibility_adapter
                .initialize(&window_name)
                .map_err(|e| {
                    WindowError::PlatformError(format!("Accessibility init failed: {}", e))
                })?;
        }

        // Defer mapping for size_to_content windows (menus): they are mapped in
        // apply_size_to_content() once measured + resized, so they never appear
        // at the wrong (pre-measure) size. All other windows map now.
        if !options.size_to_content {
            unsafe { (window.xlib.XMapWindow)(display, window.window) };
            unsafe { (window.xlib.XFlush)(display) };
        }

        // Try to subscribe to XRandR screen change notifications (optional)
        window.xrandr_event_base = try_subscribe_xrandr(display, root);

        // Position window on requested monitor (or center on primary)
        // Convert u32 to MonitorId
        let monitor_id_typed = azul_core::window::MonitorId {
            index: monitor_id as usize,
            hash: 0,
        };
        window.position_window_on_monitor(monitor_id_typed, position, size, options.parent_window_id);

        // Initialize GNOME native menus V2 (dlopen-based)
        // Only attempt if use_native_menus is true and GNOME is available
        if options.window_state.flags.use_native_menus
            && super::gnome_menu::should_use_gnome_menus()
        {
            // Get shared DBus library (loaded once, shared across all windows)
            if let Some(dbus_lib) = super::gnome_menu::get_shared_dbus_lib() {
                let app_name = &options.window_state.title;

                match super::gnome_menu::GnomeMenuManager::new(app_name, dbus_lib) {
                    Ok(menu_manager) => {
                        // Try to set window properties for GNOME Shell integration
                        match menu_manager
                            .set_window_properties(window.window as u64, display as *mut _)
                        {
                            Ok(_) => {
                                super::gnome_menu::debug_log(&format!(
                                    "GNOME menu V2 integration enabled for window: {}",
                                    app_name
                                ));
                                window.gnome_menu = Some(menu_manager);
                            }
                            Err(e) => {
                                super::gnome_menu::debug_log(&format!(
                                    "Failed to set GNOME V2 window properties: {} - falling back \
                                     to CSD menus",
                                    e
                                ));
                                // Continue without GNOME menus - will use CSD fallback
                            }
                        }
                    }
                    Err(e) => {
                        super::gnome_menu::debug_log(&format!(
                            "Failed to create GNOME menu V2 manager: {} - using CSD fallback",
                            e
                        ));
                        // Continue without GNOME menus - will use CSD fallback
                    }
                }
            } else {
                super::gnome_menu::debug_log("DBus library not available - using CSD fallback");
            }
        }

        // Invoke create_callback if provided (for GL resource upload, config loading, etc.)
        // This runs AFTER GL context is ready but BEFORE any layout is done
        if let Some(mut callback) = create_callback.into_option() {
            use azul_core::window::RawWindowHandle;

            let raw_handle = RawWindowHandle::Xlib(azul_core::window::XlibHandle {
                window: window.window as u64,
                display: window.display as *mut _,
            });

            window.ensure_layout_window_initialized()?;

            // Get mutable references needed for invoke_single_callback
            let layout_window = window
                .common.layout_window
                .as_mut()
                .expect("LayoutWindow should exist at this point");
            // Get app_data for callback
            let mut app_data_ref = window.resources.app_data.borrow_mut();

            let (changes, _update) = layout_window.invoke_single_callback(
                &mut callback,
                &mut *app_data_ref,
                &raw_handle,
                &window.common.gl_context_ptr,
                window.resources.system_style.clone(),
                &azul_layout::callbacks::ExternalSystemCallbacks::rust_internal(),
                &window.common.previous_window_state,
                &window.common.current_window_state,
                &window.common.renderer_resources,
            );

            drop(app_data_ref);
            use crate::desktop::shell2::common::event::PlatformWindow;
            for change in &changes {
                let r = window.apply_user_change(change);
                if r != azul_core::events::ProcessEventResult::DoNothing {
                    window.common.frame_needs_regeneration = true;
                }
            }
        }

        // CRITICAL: Always initialize LayoutWindow if not already done
        // This is needed for rendering even without callbacks or debug mode
        window.ensure_layout_window_initialized()?;

        // Register debug timer is now done from run() with explicit channel + component map

        // Apply initial background material if not Opaque
        {
            use azul_core::window::WindowBackgroundMaterial;
            let initial_material = window.common.current_window_state.flags.background_material;
            if !matches!(initial_material, WindowBackgroundMaterial::Opaque) {
                log_trace!(
                    LogCategory::Window,
                    "[X11] Applying initial background material: {:?}",
                    initial_material
                );
                window.apply_background_material(initial_material);
            }
        }

        // Apply initial window state for fields not set during window creation
        window.apply_initial_window_state();

        Ok(window)
    }

    /// Resolve a parent window's absolute top-left (root coords) from the window
    /// registry, for `WindowPosition::RelativeToParentWindow`. Returns `None` if
    /// there is no parent, it isn't an X11 window, or it has no concrete position
    /// yet (in which case the caller treats the offset as monitor-relative).
    fn resolve_parent_origin(parent_window_id: u64) -> Option<(i32, i32)> {
        if parent_window_id == 0 {
            return None;
        }
        unsafe {
            let wptr = super::registry::get_window(parent_window_id)?;
            match &*wptr {
                super::LinuxWindow::X11(parent) => {
                    match parent.common.current_window_state.position {
                        azul_core::window::WindowPosition::Initialized(pos) => Some((pos.x, pos.y)),
                        _ => None,
                    }
                }
                _ => None,
            }
        }
    }

    /// Position window on requested monitor, or center on primary monitor
    fn position_window_on_monitor(
        &mut self,
        monitor_id: azul_core::window::MonitorId,
        position: azul_core::window::WindowPosition,
        size: azul_core::window::WindowSize,
        parent_window_id: u64,
    ) {
        use azul_core::window::WindowPosition;

        use crate::desktop::display::get_monitors;

        // Get all available monitors
        let monitors = get_monitors();
        if monitors.len() == 0 {
            return; // No monitors available, let window manager decide
        }

        // Determine target monitor
        let target_monitor = monitors
            .as_slice()
            .iter()
            .find(|m| m.monitor_id.index == monitor_id.index)
            .or_else(|| {
                monitors
                    .as_slice()
                    .iter()
                    .find(|m| m.monitor_id.hash == monitor_id.hash && monitor_id.hash != 0)
            })
            .unwrap_or(&monitors.as_slice()[0]); // Fallback to primary

        // Calculate window position
        let (x, y) = match position {
            WindowPosition::Initialized(pos) => {
                // Explicit position requested - use it relative to monitor
                (
                    (target_monitor.position.x + pos.x as isize) as i32,
                    (target_monitor.position.y + pos.y as isize) as i32,
                )
            }
            WindowPosition::Uninitialized => {
                // No explicit position - center on target monitor
                let window_width = size.dimensions.width as isize;
                let window_height = size.dimensions.height as isize;

                let center_x =
                    target_monitor.position.x + (target_monitor.size.width - window_width) / 2;
                let center_y =
                    target_monitor.position.y + (target_monitor.size.height - window_height) / 2;

                (center_x as i32, center_y as i32)
            }
            WindowPosition::RelativeToParentWindow(offset) => {
                // Child window (menu/dropdown/popup): place at parent_top_left +
                // offset. Resolve the parent's absolute origin from the registry;
                // if it is unknown, fall back to treating the offset as
                // monitor-relative (so the popup still appears on-screen).
                match Self::resolve_parent_origin(parent_window_id) {
                    Some((px, py)) => (px + offset.x, py + offset.y),
                    None => (
                        (target_monitor.position.x + offset.x as isize) as i32,
                        (target_monitor.position.y + offset.y as isize) as i32,
                    ),
                }
            }
        };

        // Move window to calculated position
        unsafe {
            (self.xlib.XMoveWindow)(self.display, self.window, x, y);
            (self.xlib.XFlush)(self.display);
        }

        // For a relative (child) window, record the resolved absolute position so
        // it can itself act as a parent for nested popups and so later position
        // queries are consistent. A subsequent ConfigureNotify refines it.
        if matches!(position, WindowPosition::RelativeToParentWindow(_)) {
            self.common.current_window_state.position =
                WindowPosition::Initialized(azul_core::geom::PhysicalPositionI32::new(x, y));
        }
    }

    /// Process pending menu callbacks from GNOME DBus.
    ///
    /// When a menu item is clicked in GNOME Shell, the DBus handler queues
    /// the callback data. This function drains the queue and invokes each
    /// callback with proper CallbackInfo context.
    fn process_pending_menu_callbacks(&mut self) {
        use super::gnome_menu::drain_pending_menu_callbacks;

        let pending_callbacks = drain_pending_menu_callbacks();
        if pending_callbacks.is_empty() {
            return;
        }

        for pending in pending_callbacks {
            log_debug!(
                LogCategory::Callbacks,
                "[X11Window] Processing menu callback for action: {}",
                pending.action_name
            );

            // Convert CoreMenuCallback to layout MenuCallback
            use azul_layout::callbacks::{Callback, MenuCallback};

            let layout_callback = Callback::from_core(pending.menu_callback.callback);
            let mut menu_callback = MenuCallback {
                callback: layout_callback,
                refany: pending.menu_callback.refany,
            };

            // Get layout window
            let layout_window = match self.common.layout_window.as_mut() {
                Some(lw) => lw,
                None => {
                    log_warn!(
                        LogCategory::Callbacks,
                        "[X11Window] No layout window available for menu callback"
                    );
                    continue;
                }
            };

            use azul_core::window::RawWindowHandle;

            let raw_handle = RawWindowHandle::Xlib(azul_core::window::XlibHandle {
                display: self.display as *mut _,
                window: self.window as u64,
            });

            let (changes, update) = layout_window.invoke_single_callback(
                &mut menu_callback.callback,
                &mut menu_callback.refany,
                &raw_handle,
                &self.common.gl_context_ptr,
                self.common.system_style.clone(),
                &azul_layout::callbacks::ExternalSystemCallbacks::rust_internal(),
                &self.common.previous_window_state,
                &self.common.current_window_state,
                &self.common.renderer_resources,
            );

            use crate::desktop::shell2::common::event::PlatformWindow;
            let mut event_result = azul_core::events::ProcessEventResult::DoNothing;
            for change in &changes {
                event_result = event_result.max(self.apply_user_change(change));
            }
            use azul_core::callbacks::Update;
            match update {
                Update::RefreshDom => {
                    event_result = event_result.max(azul_core::events::ProcessEventResult::ShouldRegenerateDomCurrentWindow);
                }
                Update::RefreshDomAllWindows => {
                    event_result = event_result.max(azul_core::events::ProcessEventResult::ShouldRegenerateDomAllWindows);
                }
                Update::DoNothing => {}
            }

            // Handle the event result
            use azul_core::events::ProcessEventResult;
            match event_result {
                ProcessEventResult::ShouldRegenerateDomAllWindows => {
                    // Refresh EVERY registered window, not just this one
                    // (Update::RefreshDomAllWindows previously behaved like
                    // RefreshDom). Skip self in the loop to avoid aliasing the
                    // &mut self borrow; handle it explicitly afterwards.
                    for wid in super::registry::get_all_window_ids() {
                        if wid == self.window as u64 {
                            continue;
                        }
                        if let Some(wptr) = unsafe { super::registry::get_window(wid) } {
                            if let super::LinuxWindow::X11(w) = unsafe { &mut *wptr } {
                                w.common.frame_needs_regeneration = true;
                                w.request_redraw();
                            }
                        }
                    }
                    self.common.frame_needs_regeneration = true;
                    self.request_redraw();
                }
                ProcessEventResult::ShouldRegenerateDomCurrentWindow
                | ProcessEventResult::ShouldIncrementalRelayout
                | ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                    self.common.frame_needs_regeneration = true;
                    self.request_redraw();
                }
                // ShouldUpdateDisplayListCurrentWindow: pending VirtualView updates are
                // queued in layout_window.pending_virtual_view_updates and will be processed
                // in the render path — no full layout regeneration needed.
                ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
                | ProcessEventResult::ShouldReRenderCurrentWindow => {
                    self.request_redraw();
                }
                ProcessEventResult::DoNothing => {
                    // No action needed
                }
            }
        }
    }

    /// Block until an X11 event arrives or a timerfd fires.
    ///
    /// Uses `poll(2)` to wait on both the X11 connection fd and any active
    /// timerfd file descriptors simultaneously.
    pub fn wait_for_events(&mut self) -> Result<(), WindowError> {
        use super::super::common::event::PlatformWindow;
        use std::mem;

        let connection_fd = unsafe { (self.xlib.XConnectionNumber)(self.display) };

        unsafe {
            // Flush pending requests first
            (self.xlib.XFlush)(self.display);

            // Drain ALL already-queued events. Xlib buffers events client-side,
            // so the connection fd can be NOT readable even while events sit in
            // Xlib's internal queue — dequeue via XPending rather than one event
            // per poll() wake (which leaves the rest queued until unrelated fd
            // activity). (X11 API audit, finding 10.)
            if (self.xlib.XPending)(self.display) > 0 {
                while (self.xlib.XPending)(self.display) > 0 {
                    let mut event: XEvent = mem::zeroed();
                    (self.xlib.XNextEvent)(self.display, &mut event);
                    self.handle_event(&mut event);
                }
                return Ok(());
            }

            // Build pollfd array: X11 connection + all timer fds
            let mut pollfds: Vec<libc::pollfd> = Vec::with_capacity(1 + self.timer_fds.len());

            // Add X11 connection fd
            pollfds.push(libc::pollfd {
                fd: connection_fd,
                events: libc::POLLIN,
                revents: 0,
            });

            // Add all timerfd's
            let timer_ids: Vec<usize> = self.timer_fds.keys().copied().collect();
            for &timer_id in &timer_ids {
                if let Some(&fd) = self.timer_fds.get(&timer_id) {
                    pollfds.push(libc::pollfd {
                        fd,
                        events: libc::POLLIN,
                        revents: 0,
                    });
                }
            }

            // Background threads (e.g. MapWidget tile fetches) have NO fd in the
            // poll set — their completion can't wake poll(). So while any thread
            // is in flight, poll on a ~16ms tick and drain thread writebacks on
            // every wake; otherwise block indefinitely (timerfd's still wake us).
            // Without this the fetch workers finish but their writebacks are never
            // processed until some unrelated X11 event happens to wake the loop.
            let has_threads = self
                .common
                .layout_window
                .as_ref()
                .map(|lw| !lw.threads.is_empty())
                .unwrap_or(false);
            let timeout_ms: i32 = if has_threads { 16 } else { -1 };

            let result = libc::poll(
                pollfds.as_mut_ptr(),
                pollfds.len() as libc::nfds_t,
                timeout_ms,
            );

            let mut any_timer_fired = false;
            if result > 0 {
                // Check X11 connection
                if pollfds[0].revents & libc::POLLIN != 0 {
                    // Drain fully — a single poll() wake can carry many queued
                    // events; processing one would leave the rest stuck.
                    while (self.xlib.XPending)(self.display) > 0 {
                        let mut event: XEvent = mem::zeroed();
                        (self.xlib.XNextEvent)(self.display, &mut event);
                        self.handle_event(&mut event);
                    }
                }

                // Check timerfd's - if any fired, invoke timer callbacks
                for (i, &timer_id) in timer_ids.iter().enumerate() {
                    let pollfd_idx = i + 1; // +1 because X11 fd is at index 0
                    if pollfd_idx < pollfds.len() && pollfds[pollfd_idx].revents & libc::POLLIN != 0
                    {
                        // Read from timerfd to acknowledge the timer
                        if let Some(&fd) = self.timer_fds.get(&timer_id) {
                            let mut expirations: u64 = 0;
                            libc::read(fd, &mut expirations as *mut u64 as *mut libc::c_void, 8);
                            any_timer_fired = true;
                        }
                    }
                }
            }
            // result == 0: timeout (the 16ms thread tick, or spurious)
            // result < 0: error or EINTR - ignore and continue

            // Invoke expired timer AND thread callbacks via the shared
            // check_timers_and_threads, which ALSO requests a redraw when a
            // callback produced a visual change. Calling the bare
            // process_timers_and_threads here (the previous behaviour)
            // advanced timer state — e.g. the scroll-physics momentum
            // offset — but discarded the redraw signal, so real mouse-wheel
            // scrolling updated the offset invisibly and the window
            // appeared frozen until some unrelated event forced a repaint.
            // Run it on every wake while threads are active (the 16ms tick
            // guarantees we get here) so tile-fetch writebacks drain promptly.
            if any_timer_fired || has_threads {
                self.check_timers_and_threads();
            }
        }

        Ok(())
    }

    fn handle_event(&mut self, event: &mut XEvent) {
        if let Some(ime) = &self.ime_manager {
            let consumed = ime.filter_event(event);
            if let Some((preedit, caret)) = ime.drain_preedit() {
                if let Some(ref mut lw) = self.common.layout_window {
                    match preedit {
                        Some(t) if !t.is_empty() => {
                            lw.text_edit_manager.set_preedit(t, caret, caret);
                        }
                        _ => lw.text_edit_manager.clear_preedit(),
                    }
                }
            }
            if consumed {
                return;
            }
        }

        // Raw-event trace (see the main event loop).
        crate::plog_trace!(
            "[x11 ev] raw {} (#{})",
            x11_event_name(unsafe { event.type_ }),
            unsafe { event.type_ }
        );

        // Process event with V2 handlers
        let result = match unsafe { event.type_ } {
            defines::Expose => {
                // A real (WM) or synthetic Expose means "repaint now". Render
                // directly — the previous code re-posted ANOTHER Expose here
                // (request_redraw), so in the blocking idle path the repaint
                // request ping-ponged and the frame never actually painted
                // (resize/timer/caret repaints appeared frozen). This now
                // matches poll_event's Expose arm.
                self.needs_redraw = true;
                if let Err(e) = self.render_and_present() {
                    log_warn!(LogCategory::Rendering, "[X11] handle_event Expose render failed: {:?}", e);
                }
                ProcessEventResult::DoNothing
            }
            defines::ClientMessage => {
                let msg_atom = unsafe { event.client_message.data.l[0] } as Atom;
                log_debug!(
                    LogCategory::Window,
                    "[X11] handle_event: ClientMessage msg_atom={}, wm_delete_atom={}",
                    msg_atom,
                    self.wm_delete_window_atom
                );
                if msg_atom == self.wm_delete_window_atom {
                    log_info!(
                        LogCategory::Window,
                        "[X11] handle_event: WM_DELETE_WINDOW - closing window"
                    );
                    self.is_open = false;
                }
                ProcessEventResult::DoNothing
            }
            defines::ButtonPress | defines::ButtonRelease => {
                self.handle_mouse_button(unsafe { &event.button })
            }
            defines::MotionNotify => self.handle_mouse_move(unsafe { &event.motion }),
            defines::GenericEvent => handle_xi_event(self, event),
            defines::KeyPress | defines::KeyRelease => {
                self.handle_keyboard(unsafe { &mut event.key })
            }
            defines::EnterNotify | defines::LeaveNotify => {
                self.handle_mouse_crossing(unsafe { &event.crossing })
            }
            defines::FocusIn => {
                self.common.current_window_state.window_focused = true;
                self.dynamic_selector_context.window_focused = true;
                self.sync_ime_position_to_os();
                ProcessEventResult::DoNothing
            }
            defines::FocusOut => {
                self.common.current_window_state.window_focused = false;
                self.dynamic_selector_context.window_focused = false;
                ProcessEventResult::DoNothing
            }
            defines::MapNotify => {
                self.needs_redraw = true;
                ProcessEventResult::DoNothing
            }
            defines::ConfigureNotify => {
                let ev = unsafe { &event.configure };
                let (new_width, new_height) = (ev.width as u32, ev.height as u32);

                let old_context = self.dynamic_selector_context.clone();
                let size_changed = self.common.current_window_state.size.get_physical_size()
                    != PhysicalSize::new(new_width, new_height);
                let position_changed = match self.common.current_window_state.position {
                    azul_core::window::WindowPosition::Initialized(pos) => {
                        pos.x != ev.x || pos.y != ev.y
                    }
                    _ => true,
                };

                if size_changed {
                    self.common.current_window_state.size.dimensions =
                        LogicalSize::new(new_width as f32, new_height as f32);
                    self.dynamic_selector_context.viewport_width = new_width as f32;
                    self.dynamic_selector_context.viewport_height = new_height as f32;
                    self.dynamic_selector_context.orientation = if new_width > new_height {
                        azul_css::dynamic_selector::OrientationType::Landscape
                    } else {
                        azul_css::dynamic_selector::OrientationType::Portrait
                    };
                    if old_context.viewport_breakpoint_changed(
                        &self.dynamic_selector_context,
                        CSS_BREAKPOINTS,
                    ) {
                        log_debug!(
                            LogCategory::Layout,
                            "[X11 Resize] Breakpoint crossed: {}x{} -> {}x{}",
                            old_context.viewport_width,
                            old_context.viewport_height,
                            self.dynamic_selector_context.viewport_width,
                            self.dynamic_selector_context.viewport_height
                        );
                    }
                    self.common.next_relayout_reason =
                        azul_core::callbacks::RelayoutReason::Resize;
                    self.regenerate_layout().ok();

                    // XWayland / some compositors don't send an Expose after
                    // ConfigureNotify, so request a repaint explicitly (self-Expose,
                    // coalesced) — otherwise resize relayouts but never presents.
                    crate::plog_info!(
                        "[X11] ConfigureNotify resize -> {}x{}: relayout + request_redraw",
                        new_width,
                        new_height
                    );
                    self.request_redraw();
                }

                crate::plog_trace!(
                    "[x11 ev] ConfigureNotify x={} y={} w={} h={} send_event={}",
                    ev.x, ev.y, new_width, new_height, ev.send_event
                );

                // F4: OS-reported geometry (source = Os) — acknowledge into both
                // current and the sync baseline so it is not echoed back. See the
                // main-loop handler + CommonWindowState::update_window_state.
                let new_pos = azul_core::window::WindowPosition::Initialized(
                    azul_core::geom::PhysicalPositionI32::new(ev.x, ev.y),
                );
                let new_dims = LogicalSize::new(new_width as f32, new_height as f32);
                self.common.update_window_state(
                    crate::desktop::shell2::common::event::WindowStateSource::Os,
                    |ws| {
                        ws.position = new_pos;
                        if size_changed {
                            ws.size.dimensions = new_dims;
                        }
                    },
                );

                if position_changed && !size_changed {
                    use azul_core::geom::LogicalPosition;
                    let window_center = LogicalPosition::new(
                        ev.x as f32 + new_width as f32 / 2.0,
                        ev.y as f32 + new_height as f32 / 2.0,
                    );
                    if let Some(display) =
                        crate::desktop::display::get_display_at_point(window_center)
                    {
                        let new_dpi = (display.scale_factor * 96.0) as u32;
                        let old_dpi = self.common.current_window_state.size.dpi;
                        if (new_dpi as i32 - old_dpi as i32).abs() > 1 {
                            log_debug!(
                                LogCategory::Window,
                                "[X11 DPI Change] {} -> {} (moved to different monitor)",
                                old_dpi,
                                new_dpi
                            );
                            self.common.current_window_state.size.dpi = new_dpi;
                            self.regenerate_layout().ok();
                        }
                    }
                }

                ProcessEventResult::DoNothing
            }
            other => {
                // Check for XRandR screen change event (dynamic event type)
                if let Some(event_base) = self.xrandr_event_base {
                    if other == event_base {
                        log_debug!(
                            LogCategory::Platform,
                            "[X11] XRandR screen change detected (handle_event), refreshing monitor cache"
                        );
                        if let Some(ref lw) = self.common.layout_window {
                            if let Ok(mut guard) = lw.monitors.lock() {
                                *guard = crate::desktop::display::get_monitors();
                            }
                        }
                    }
                }
                ProcessEventResult::DoNothing
            }
        };

        // Fan out a cross-window refresh. process_window_events already marked
        // SELF via mark_frame_needs_regeneration for RefreshDom/RefreshDomAllWindows;
        // for RefreshDomAllWindows we must ALSO mark every OTHER registered window so
        // a child popup's callback (e.g. a context-menu item mutating shared app data)
        // re-lays-out its parent. Previously this result was discarded here except for
        // a self request_redraw, so the software-menu/DOM path never refreshed the
        // parent (the native gnome-menu path handled it in process_pending_menu_callbacks).
        if result == ProcessEventResult::ShouldRegenerateDomAllWindows {
            for wid in super::registry::get_all_window_ids() {
                if wid == self.window as u64 {
                    continue;
                }
                if let Some(wptr) = unsafe { super::registry::get_window(wid) } {
                    if let super::LinuxWindow::X11(w) = unsafe { &mut *wptr } {
                        w.common.frame_needs_regeneration = true;
                        w.request_redraw();
                    }
                }
            }
        }

        // Request redraw if needed
        if result != ProcessEventResult::DoNothing {
            self.request_redraw();
        }
    }

    /// One-time `size_to_content`: lay the DOM out at a tiny viewport so the
    /// content overflows to its natural size, read that overflow extent
    /// (`LayoutTree::get_content_size(0)`), resize the window to it, then map.
    /// Called once from poll_event before the first render for windows created
    /// with `size_to_content` (menus/tooltips). Mirrors the user-specified
    /// algorithm: size→0, read overflow, resize, relayout.
    fn apply_size_to_content(&mut self) {
        // LogicalSize is already in scope (top-of-file `geom::{LogicalSize, ...}`).
        self.size_to_content_pending = false;

        // 1. Lay out at a tiny viewport (narrower than any real menu) so block
        //    children shrink to min/content width and the content overflows.
        let orig = self.common.current_window_state.size;
        let mut tiny = orig;
        tiny.dimensions = LogicalSize::new(16.0, 16.0);
        self.common.current_window_state.size = tiny;
        let _ = self.regenerate_layout();

        // 2. Read the natural content size (overflow extent of the root node 0).
        let natural = self
            .common
            .layout_window
            .as_ref()
            .and_then(|lw| lw.layout_results.get(&azul_core::dom::DomId { inner: 0 }))
            .map(|lr| lr.layout_tree.get_content_size(0));

        // 3. Resize to it (≥1px). DPI handled by WindowSize's logical→physical.
        let mut final_size = orig;
        if let Some(sz) = natural {
            final_size.dimensions = LogicalSize::new(sz.width.max(1.0), sz.height.max(1.0));
            log_debug!(
                LogCategory::Window,
                "[X11] size_to_content: measured natural size {}x{}",
                sz.width,
                sz.height
            );
        }
        self.common.current_window_state.size = final_size;

        // Re-clamp a menu's position to the monitor work-area now that its TRUE
        // size is known (show_menu positioned it from a size ESTIMATE). Keeps a
        // menu whose real content exceeds the estimate from spilling off-screen
        // right/bottom. (DPI=1 assumption: position is physical, work_area logical.)
        if self.common.current_window_state.flags.window_type
            == azul_core::window::WindowType::Menu
        {
            use azul_core::window::WindowPosition;
            if let WindowPosition::Initialized(pos) = self.common.current_window_state.position {
                let posf = azul_core::geom::LogicalPosition::new(pos.x as f32, pos.y as f32);
                if let Some(display) = crate::desktop::display::get_display_at_point(posf) {
                    let wa = display.work_area;
                    // Height-clamp: a menu taller than the monitor work-area is
                    // capped to it so it never runs off the bottom of the screen.
                    // The menu DOM scrolls within the capped height (overflow-y:auto
                    // on the menu container in menu_renderer).
                    if final_size.dimensions.height > wa.size.height {
                        final_size.dimensions.height = wa.size.height;
                        self.common.current_window_state.size = final_size;
                    }
                    let w = final_size.dimensions.width;
                    let h = final_size.dimensions.height;
                    let nx = posf.x.min(wa.origin.x + wa.size.width - w).max(wa.origin.x);
                    let ny = posf.y.min(wa.origin.y + wa.size.height - h).max(wa.origin.y);
                    if (nx - posf.x).abs() > 0.5 || (ny - posf.y).abs() > 0.5 {
                        self.common.current_window_state.position = WindowPosition::Initialized(
                            azul_core::geom::PhysicalPositionI32::new(nx as i32, ny as i32),
                        );
                        unsafe {
                            (self.xlib.XMoveWindow)(self.display, self.window, nx as i32, ny as i32);
                        }
                    }
                }
            }
        }

        let phys = final_size.get_physical_size();
        unsafe {
            (self.xlib.XResizeWindow)(
                self.display,
                self.window,
                (phys.width as u32).max(1),
                (phys.height as u32).max(1),
            );
            // 4. Map now (deferred from creation). The render_and_present that
            //    follows in this same poll_event iteration lays out + presents at
            //    final_size, so the popup never appears at the tiny measure size.
            (self.xlib.XMapWindow)(self.display, self.window);
            // XSync (not XFlush): block until the server has PROCESSED the map so the
            // window is viewable before we grab. XGrabPointer on a not-yet-viewable
            // window fails with GrabNotViewable — and then the menu never receives the
            // click-outside (or item-click) that dismisses it, so it stays stuck open.
            (self.xlib.XSync)(self.display, 0);
            // Menu/popup windows grab the pointer so a click ANYWHERE outside the
            // menu (another window, the root, …) is delivered here for dismissal.
            // owner_events=False routes every pointer event to the menu;
            // handle_mouse_button's bounds-check decides item-click vs click-outside.
            if self.common.current_window_state.flags.window_type
                == azul_core::window::WindowType::Menu
            {
                // Retry until the grab succeeds (GrabSuccess == 0): even after XSync an
                // override-redirect popup can momentarily be un-grabbable, and a
                // silently-failed grab leaves the menu impossible to dismiss.
                for _ in 0..20 {
                    let r = (self.xlib.XGrabPointer)(
                        self.display,
                        self.window,
                        0, // owner_events = False
                        (defines::ButtonPressMask
                            | defines::ButtonReleaseMask
                            | defines::PointerMotionMask) as u32,
                        defines::GrabModeAsync,
                        defines::GrabModeAsync,
                        0, // confine_to = None
                        0, // cursor = None
                        defines::CurrentTime,
                    );
                    if r == 0 {
                        break;
                    }
                    (self.xlib.XSync)(self.display, 0);
                }
            }
        }

        // 5. Re-layout at the final size (drops the scrollbars the tiny pass added).
        self.common.frame_needs_regeneration = true;
    }

    pub fn regenerate_layout(&mut self) -> Result<crate::desktop::shell2::common::layout::LayoutRegenerateResult, String> {
        let layout_window = self.common.layout_window.as_mut().ok_or("No layout window")?;

        // Collect debug messages if debug server is enabled
        let debug_enabled = crate::desktop::shell2::common::debug_server::is_debug_enabled();
        let mut debug_messages = if debug_enabled {
            Some(Vec::new())
        } else {
            None
        };

        // Call unified regenerate_layout from common module
        let result = crate::desktop::shell2::common::layout::regenerate_layout(
            layout_window,
            &self.common.app_data,
            &self.common.current_window_state,
            &mut self.common.renderer_resources,
            &self.common.image_cache,
            &self.common.gl_context_ptr,
            &self.common.fc_cache,
            &self.resources.font_registry,
            &self.common.system_style,
            &self.resources.icon_provider,
            &mut debug_messages,
        
            self.common.next_relayout_reason,
        )?;
        // Consumed; reset so an untagged regen sees the implicit RefreshDom.
        self.common.next_relayout_reason =
            azul_core::callbacks::RelayoutReason::RefreshDom;

        // Forward layout debug messages to the debug server's log queue
        if let Some(msgs) = debug_messages {
            for msg in msgs {
                crate::desktop::shell2::common::debug_server::log(
                    crate::desktop::shell2::common::debug_server::LogLevel::Debug,
                    crate::desktop::shell2::common::debug_server::LogCategory::Layout,
                    msg.message.as_str().to_string(),
                    None,
                );
            }
        }

        // Update accessibility tree after layout
        #[cfg(feature = "a11y")]
        if let Some(layout_window) = self.common.layout_window.as_ref() {
            if let Some(tree_update) = layout_window.a11y_manager.last_tree_update.clone() {
                self.accessibility_adapter.update_tree(tree_update);
            }
        }

        // Send frame to WebRender (GPU mode only - CPU mode reads display list directly)
        if let RenderMode::Gpu(ref gl_context, _) = self.render_mode {
            gl_context.make_current();

            if let (Some(layout_window), Some(render_api), Some(document_id)) = (
                self.common.layout_window.as_mut(),
                self.common.render_api.as_mut(),
                self.common.document_id,
            ) {
                crate::desktop::shell2::common::layout::generate_frame(
                    layout_window,
                    render_api,
                    document_id,
                    &self.common.gl_context_ptr,
                );
                render_api.flush_scene_builder();
            }
        }

        // Rebuild the CPU hit-tester from the fresh layout. CPU mode has no
        // WebRender hit-tester (render_api is None), so without this rebuild
        // every hit test returns 0 hits -> dead mouse hover / click / caret /
        // drag-select / wheel-scroll / focus (#46). GPU mode has
        // cpu_hit_tester == None and uses the WebRender tester instead, so the
        // is_some() guard naturally restricts this to the CPU path.
        if let (Some(cpu_ht), Some(lw)) = (
            self.common.cpu_hit_tester.as_mut(),
            self.common.layout_window.as_ref(),
        ) {
            cpu_ht.rebuild_from_layout(&lw.layout_results);
        }

        // Drain lifecycle events (Mount / AfterMount / Unmount / Resize) produced
        // by this layout's DOM reconciliation and dispatch them through the normal
        // callback pipeline — the SAME step the headless backend runs inside its
        // own regenerate_layout. Without this, `EventFilter::Component(AfterMount)`
        // callbacks NEVER fire on X11, so e.g. the MapWidget's first tile-fetch
        // (kicked from AfterMount) never starts. The VirtualView render above has
        // already inserted the Pending tiles, so the AfterMount handler sees them.
        let _ = self.dispatch_pending_lifecycle_events();

        // Phase 2: Post-Layout callback - sync IME position after layout (MOST IMPORTANT)
        self.update_ime_position_from_cursor();
        self.sync_ime_position_to_os();

        Ok(result)
    }

    /// Update ime_position in window state from focused text cursor
    /// Called after layout to ensure IME window appears at correct position
    fn update_ime_position_from_cursor(&mut self) {
        use azul_core::window::ImePosition;

        if let Some(layout_window) = &self.common.layout_window {
            if let Some(cursor_rect) = layout_window.get_focused_cursor_rect_viewport() {
                // Successfully calculated cursor position from text layout
                self.common.current_window_state.ime_position = ImePosition::Initialized(cursor_rect);
            }
        }
    }

    /// Generate frame if needed and reset flag
    pub fn generate_frame_if_needed(&mut self) {
        if !self.common.frame_needs_regeneration {
            return;
        }

        // CRITICAL: Make OpenGL context current BEFORE generate_frame
        // The image callbacks (RenderImageCallback) need the GL context to be current
        if let RenderMode::Gpu(ref gl_context, _) = self.render_mode {
            gl_context.make_current();
        }

        if let (Some(ref mut layout_window), Some(ref mut render_api), Some(document_id)) = (
            self.common.layout_window.as_mut(),
            self.common.render_api.as_mut(),
            self.common.document_id,
        ) {
            crate::desktop::shell2::common::layout::generate_frame(
                layout_window,
                render_api,
                document_id,
                &self.common.gl_context_ptr,
            );
        }

        self.common.frame_needs_regeneration = false;
    }

    /// Render and present a frame using WebRender
    ///
    /// This is called on Expose events to actually draw content to the window.
    /// The flow is:
    /// 1. Regenerate layout if needed
    /// 2. Build and send WebRender transaction
    /// 3. Call renderer.update() and renderer.render()
    /// 4. Swap buffers to show the rendered frame
    pub fn render_and_present(&mut self) -> Result<(), WindowError> {
        // Consume the render-intent flag: this call satisfies it. The GPU
        // skip-heuristic below honours `want_redraw`, so any explicitly
        // requested repaint (resize, caret move/blink, physics-scroll, a11y,
        // real Expose) is never skipped — only a truly idle speculative call is.
        let want_redraw = self.needs_redraw;
        self.needs_redraw = false;

        // Skip rendering a degenerate (0-size) window. A reparenting/compositing
        // WM delivers a 0-size ConfigureNotify on iconify (minimize) and during
        // some maximize transitions; laying out / rendering at 0 crashes the GPU
        // path (the CPU path already guarded width/height > 0). Gating on the
        // actual size is reliable and WM-independent — unlike tracking
        // MapNotify/UnmapNotify ordering, which varies between window managers.
        let phys = self.common.current_window_state.size.get_physical_size();
        if phys.width == 0 || phys.height == 0 {
            return Ok(());
        }

        // Step 1: Regenerate layout if needed, otherwise send lightweight transaction
        let layout_was_regenerated = if self.common.frame_needs_regeneration {
            if let Err(e) = self.regenerate_layout() {
                return Err(WindowError::PlatformError(format!("Layout failed: {}", e)));
            }
            self.common.frame_needs_regeneration = false;
            true
        } else {
            false
        };

        // CPU rendering path: skip WebRender steps, render directly via cpurender
        if let RenderMode::Cpu(gc) = &self.render_mode {
            if let Some(gc) = gc {
                #[cfg(feature = "cpurender")]
                {
                    use azul_core::dom::DomId;
                    use std::ffi::{c_char, c_uint};

                    let mut rendered = false;

                    // Synchronize window state to layout_window before rendering
                    if let Some(ref mut layout_window) = self.common.layout_window {
                        layout_window.current_window_state =
                            self.common.current_window_state.clone();

                        // Advance easing-based scroll animations
                        {
                            #[cfg(feature = "std")]
                            let now = azul_core::task::Instant::System(
                                std::time::Instant::now().into(),
                            );
                            #[cfg(not(feature = "std"))]
                            let now = azul_core::task::Instant::Tick(
                                azul_core::task::SystemTick { tick_counter: 0 },
                            );
                            let tick_result = layout_window.scroll_manager.tick(now);
                            if tick_result.needs_repaint {
                                layout_window.scroll_manager.calculate_scrollbar_states();
                            }
                        }
                    }

                    // Re-invoke any VirtualViews queued for in-place re-render
                    // (e.g. MapWidget tiles delivered by a background writeback
                    // that called trigger_all_virtual_view_rerender). The GPU
                    // path does this inside generate_frame; the CPU path has no
                    // generate_frame, so without this the re-render queue is
                    // never drained and async-loaded VirtualView content never
                    // appears on the CPU backend. Must run BEFORE render_frame
                    // reads layout_results.
                    if let Some(lw) = self.common.layout_window.as_mut() {
                        if !lw.pending_virtual_view_updates.is_empty() {
                            let system_callbacks =
                                azul_layout::callbacks::ExternalSystemCallbacks::rust_internal();
                            let current_window_state = lw.current_window_state.clone();
                            let renderer_resources =
                                std::mem::take(&mut lw.renderer_resources);
                            lw.process_pending_virtual_view_updates(
                                &current_window_state,
                                &renderer_resources,
                                &system_callbacks,
                            );
                            lw.renderer_resources = renderer_resources;
                        }
                    }

                    if let Some(ref layout_window) = self.common.layout_window {
                        let dom_id = DomId { inner: 0 };
                        // render_frame looks up the layout result itself; we only
                        // need to know one exists before computing window dims.
                        if layout_window.layout_results.contains_key(&dom_id) {
                            let ws = &layout_window.current_window_state;
                            let width = ws.size.dimensions.width;
                            let height = ws.size.dimensions.height;
                            let dpi = ws.size.dpi as f32 / 96.0;

                            if width > 0.0 && height > 0.0 {
                                // Shared CPU renderer (the SAME path as headless):
                                // display-list damage diff + scroll-offset feed +
                                // thin-strip scroll-shift with fast-path eligibility +
                                // offset-aware incremental/full render, all inside
                                // render_frame. Replaces the logic that used to live
                                // here and lacked ALL the scroll machinery (its
                                // incremental path only damaged the scrollbar, so
                                // scrolling left the content frozen — #13/#14).
                                self.cpu_backend.render_frame(
                                    layout_window,
                                    &layout_window.renderer_resources,
                                    width,
                                    height,
                                    dpi,
                                );

                                // Blit the rendered pixmap to the X11 window.
                                if let Some(ref pixmap) = self.cpu_backend.last_frame {
                                    let pw = pixmap.width() as c_uint;
                                    let ph = pixmap.height() as c_uint;
                                    let data = pixmap.data();

                                    // R2: if the blitted pixmap is smaller than the
                                    // window, the uncovered window area shows the X11
                                    // window background (black) — the reported "bg goes
                                    // black when window < content". Surface it so the
                                    // per-OS run can confirm this is the flicker source.
                                    let phys = self.common.current_window_state.size.get_physical_size();
                                    if pw != phys.width || ph != phys.height {
                                        crate::plog_warn!(
                                            "[x11 cpu] pixmap {}x{} != window {}x{} — uncovered area will show black (R2)",
                                            pw, ph, phys.width, phys.height
                                        );
                                    }

                                    // Reuse BGRA conversion buffer
                                    self.bgra_buffer.resize(data.len(), 0);
                                    for (src, dst) in data.chunks_exact(4).zip(self.bgra_buffer.chunks_exact_mut(4)) {
                                        dst[0] = src[2]; // B
                                        dst[1] = src[1]; // G
                                        dst[2] = src[0]; // R
                                        dst[3] = src[3]; // A
                                    }

                                    unsafe {
                                        let screen = (self.xlib.XDefaultScreen)(self.display);
                                        // XCreateImage needs the visual and depth to be
                                        // consistent. The window uses a 32-bit ARGB visual;
                                        // pairing depth=32 (below) with the 24-bit DEFAULT
                                        // visual makes XCreateImage return NULL → the blit is
                                        // skipped → the window stays black. Re-find the
                                        // matching 32-bit visual (XMatchVisualInfo is
                                        // client-side — no server round-trip).
                                        let visual = if self.has_argb_visual {
                                            let mut vinfo: defines::XVisualInfo =
                                                std::mem::zeroed();
                                            if (self.xlib.XMatchVisualInfo)(
                                                self.display,
                                                screen,
                                                32,
                                                defines::TrueColor,
                                                &mut vinfo,
                                            ) != 0
                                            {
                                                vinfo.visual
                                            } else {
                                                (self.xlib.XDefaultVisual)(self.display, screen)
                                            }
                                        } else {
                                            (self.xlib.XDefaultVisual)(self.display, screen)
                                        };
                                        // XPutImage requires the image depth to EQUAL the
                                        // drawable (window) depth. The window uses a 32-bit
                                        // ARGB visual (for transparency), not the screen
                                        // default (usually 24) — so using XDefaultDepth here
                                        // made XPutImage fail with BadMatch (opcode 72 /
                                        // error code 8) and the window stayed black. Match
                                        // the window's actual depth.
                                        let depth: c_uint = if self.has_argb_visual {
                                            32
                                        } else {
                                            (self.xlib.XDefaultDepth)(self.display, screen) as c_uint
                                        };

                                        let ximage = (self.xlib.XCreateImage)(
                                            self.display,
                                            visual as *mut c_void,
                                            depth as c_uint,
                                            2, // ZPixmap
                                            0,
                                            self.bgra_buffer.as_mut_ptr() as *mut c_char,
                                            pw, ph,
                                            32, // bitmap_pad
                                            0,  // bytes_per_line (0 = auto)
                                        );

                                        if !ximage.is_null() {
                                            (self.xlib.XPutImage)(
                                                self.display, self.window, *gc, ximage,
                                                0, 0, 0, 0, pw, ph,
                                            );
                                            (*ximage).data = std::ptr::null_mut();
                                            (self.xlib.XDestroyImage)(ximage);
                                        }
                                    }
                                    rendered = true;
                                }
                                // (previous-display-list tracking now lives inside
                                // CpuBackend::render_frame.)
                            }
                        }
                    }

                    if !rendered {
                        // R2: no pixmap was produced this frame (e.g. unchanged
                        // display list but the retained pixmap was dropped) — we
                        // paint a solid fallback colour instead of the content,
                        // which flips with real frames as the black/white flicker.
                        crate::plog_warn!(
                            "[x11 cpu] no rendered pixmap — painting solid fallback bg instead of content (R2 flicker suspect)"
                        );
                        // Fallback to solid rectangle if CPU rendering not yet available
                        unsafe {
                            (self.xlib.XSetForeground)(self.display, *gc, CPU_FALLBACK_BG_COLOR);
                            let physical_size =
                                self.common.current_window_state.size.get_physical_size();
                            (self.xlib.XFillRectangle)(
                                self.display,
                                self.window,
                                *gc,
                                0,
                                0,
                                physical_size.width,
                                physical_size.height,
                            );
                        }
                    }
                }

                #[cfg(not(feature = "cpurender"))]
                unsafe {
                    let physical_size =
                        self.common.current_window_state.size.get_physical_size();
                    (self.xlib.XSetForeground)(self.display, *gc, CPU_FALLBACK_BG_COLOR);
                    (self.xlib.XFillRectangle)(
                        self.display,
                        self.window,
                        *gc,
                        0,
                        0,
                        physical_size.width,
                        physical_size.height,
                    );
                }
            }
            unsafe { (self.xlib.XFlush)(self.display) };

            self.common.display_list_initialized = true;

            // CI testing: Exit successfully after first frame render if env var is set
            if std::env::var("AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER").is_ok() {
                log_debug!(
                    LogCategory::General,
                    "[CI] AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER set - exiting"
                );
                std::process::exit(0);
            }

            return Ok(());
        }

        // GPU rendering path: Steps 2-6 use WebRender

        // Step 2: Make sure we have required components
        let renderer = match self.common.renderer.as_mut() {
            Some(r) => r,
            None => {
                return Err(WindowError::PlatformError("No renderer available".into()));
            }
        };

        // Step 3: Make GL context current (for GPU rendering)
        if let RenderMode::Gpu(ref gl_context, _) = self.render_mode {
            gl_context.make_current();
        }

        // Step 3.5: If layout was NOT regenerated, send lightweight transaction
        // for scroll offsets + GPU values + image callback updates.
        // When layout WAS regenerated, regenerate_layout() already sent the full
        // transaction via common::layout::generate_frame().
        if !layout_was_regenerated {
            // Early-return optimization: if the display list is already initialized
            // and layout wasn't regenerated, check if there's any visual change at all.
            // If not, skip the entire WebRender render cycle to save GPU work.
            if self.common.display_list_initialized {
                let scroll_active = self.common.layout_window.as_ref()
                    .map(|lw| lw.scroll_manager.has_active_animations())
                    .unwrap_or(false);
                let scrollbar_fade = self.common.layout_window.as_ref()
                    .map(|lw| lw.gpu_state_manager.scrollbar_fade_active)
                    .unwrap_or(false);
                let virtual_view_pending = self.common.layout_window.as_ref()
                    .map(|lw| !lw.pending_virtual_view_updates.is_empty())
                    .unwrap_or(false);
                if !want_redraw && !scroll_active && !scrollbar_fade && !virtual_view_pending {
                    log_trace!(
                        LogCategory::Rendering,
                        "[X11] No redraw requested and no animation active — skipping GPU render"
                    );
                    return Ok(());
                }
            }

            if let (Some(layout_window), Some(render_api)) = (
                self.common.layout_window.as_mut(),
                self.common.render_api.as_mut(),
            ) {
                // Advance easing-based scroll animations
                {
                    #[cfg(feature = "std")]
                    let now = azul_core::task::Instant::System(std::time::Instant::now().into());
                    #[cfg(not(feature = "std"))]
                    let now = azul_core::task::Instant::Tick(azul_core::task::SystemTick { tick_counter: 0 });
                    let tick_result = layout_window.scroll_manager.tick(now);
                    if tick_result.needs_repaint {
                        layout_window.scroll_manager.calculate_scrollbar_states();
                    }
                }

                // Process pending VirtualView updates (queued by ScrollTo -> check_and_queue_virtual_view_reinvoke).
                // If present, we need a full display list rebuild rather than lightweight.
                let has_virtual_view_updates = !layout_window.pending_virtual_view_updates.is_empty();
                if has_virtual_view_updates {
                    if let Some(document_id) = self.common.document_id {
                        crate::desktop::shell2::common::layout::generate_frame(
                            layout_window,
                            render_api,
                            document_id,
                            &self.common.gl_context_ptr,
                        );
                        render_api.flush_scene_builder();
                    }
                } else {
                    let mut txn = crate::desktop::wr_translate2::WrTransaction::new();
                    if let Err(e) = crate::desktop::wr_translate2::build_image_only_transaction(
                        &mut txn,
                        layout_window,
                        render_api,
                        &self.common.gl_context_ptr,
                    ) {
                        log_error!(
                            LogCategory::Rendering,
                            "[X11] Failed to build lightweight transaction: {}",
                            e
                        );
                    }

                    if let Some(document_id) = self.common.document_id {
                        render_api.send_transaction(
                            crate::desktop::wr_translate2::wr_translate_document_id(document_id),
                            txn,
                        );
                        render_api.flush_scene_builder();
                    }
                }
            }
        }

        // Step 4: Update WebRender (re-borrow renderer after layout_window borrow)
        let renderer = match self.common.renderer.as_mut() {
            Some(r) => r,
            None => {
                return Err(WindowError::PlatformError("No renderer available".into()));
            }
        };
        renderer.update();

        // Step 5: Render frame
        let physical_size = self.common.current_window_state.size.get_physical_size();
        // Clamp to >= 1x1: a reparenting/compositing WM delivers a 0-size
        // ConfigureNotify on iconify (minimize) and during some maximize
        // transitions; feeding 0 into WebRender render()/glViewport crashes
        // (the CPU path already guards width/height > 0; the GPU path did not).
        let framebuffer_size = webrender::api::units::DeviceIntSize::new(
            (physical_size.width as i32).max(1),
            (physical_size.height as i32).max(1),
        );

        match renderer.render(framebuffer_size, 0) {
            Ok(results) => {
                // Store WebRender's dirty rects for per-rect Expose invalidation.
                let dpi_scale = self.common.current_window_state.size.dpi as f32 / 96.0;
                self.gpu_damage_rects = results.dirty_rects.iter().map(|dr| {
                    azul_core::geom::LogicalRect {
                        origin: azul_core::geom::LogicalPosition {
                            x: dr.min.x as f32 / dpi_scale,
                            y: dr.min.y as f32 / dpi_scale,
                        },
                        size: azul_core::geom::LogicalSize {
                            width: dr.width() as f32 / dpi_scale,
                            height: dr.height() as f32 / dpi_scale,
                        },
                    }
                }).collect();
            }
            Err(errors) => {
                log_warn!(LogCategory::Rendering, "[X11] Render errors: {:?}", errors);
                return Err(WindowError::PlatformError(format!(
                    "Render failed: {:?}",
                    errors
                )));
            }
        }

        self.common.display_list_initialized = true;

        // Step 6: Swap buffers
        if let RenderMode::Gpu(gl_context, _) = &self.render_mode {
            if let Err(e) = gl_context.swap_buffers() {
                return Err(e);
            }
        }

        // Clean up old textures from previous epochs to prevent memory leak
        // This must happen AFTER render() and buffer swap when WebRender no longer needs the textures
        if let Some(ref layout_window) = self.common.layout_window {
            crate::desktop::gl_texture_integration::remove_old_gl_textures(
                &layout_window.document_id,
                layout_window.epoch,
            );
        }

        // If any scrollbar is actively fading (0 < opacity < 1), schedule
        // another frame so the fade-out animation runs to completion.
        let needs_fade_frame = self.common.layout_window.as_ref()
            .map(|lw| lw.gpu_state_manager.scrollbar_fade_active)
            .unwrap_or(false);
        if needs_fade_frame {
            self.request_redraw();
        }

        // CI testing: Exit successfully after first frame render if env var is set
        if std::env::var("AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER").is_ok() {
            log_debug!(
                LogCategory::General,
                "[CI] AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER set - exiting"
            );
            std::process::exit(0);
        }

        Ok(())
    }

    /// Apply initial window state at startup for fields not set during window creation.
    ///
    /// During new(), the following are already applied directly:
    /// - size (via XCreateWindow)
    /// - position (via position_window_on_monitor)
    /// - decorations (override_redirect for None only)
    /// - background_material (via apply_background_material)
    ///
    /// This method applies the remaining fields and sets previous_window_state
    /// so that sync_window_state() works correctly for future changes.
    fn apply_initial_window_state(&mut self) {
        use azul_core::window::WindowFrame;
        use std::ffi::CString;

        // Title — XStoreName is NOT called in new(), so we must apply it here
        {
            let c_title = CString::new(self.common.current_window_state.title.as_str()).unwrap();
            unsafe {
                (self.xlib.XStoreName)(self.display, self.window, c_title.as_ptr());
            }
        }

        // Window frame (Maximized, Minimized, Fullscreen)
        // Must be done AFTER XMapWindow since _NET_WM_STATE messages go to the root window
        match self.common.current_window_state.flags.frame {
            WindowFrame::Maximized => unsafe {
                self.send_wm_state_change(
                    1,
                    b"_NET_WM_STATE_MAXIMIZED_VERT\0",
                    Some(b"_NET_WM_STATE_MAXIMIZED_HORZ\0"),
                );
            },
            WindowFrame::Fullscreen => unsafe {
                self.send_wm_state_change(1, b"_NET_WM_STATE_FULLSCREEN\0", None);
            },
            WindowFrame::Minimized => unsafe {
                (self.xlib.XUnmapWindow)(self.display, self.window);
            },
            WindowFrame::Normal => {} // Already in normal state
        }

        // Always-on-top
        if self.common.current_window_state.flags.is_always_on_top {
            unsafe {
                self.send_wm_state_change(1, b"_NET_WM_STATE_ABOVE\0", None);
            }
        }

        // is_top_level
        if self.common.current_window_state.flags.is_top_level {
            self.set_is_top_level(true);
        }

        // prevent_system_sleep
        if self.common.current_window_state.flags.prevent_system_sleep {
            self.set_prevent_system_sleep(true);
        }

        // Flush all X11 commands
        unsafe {
            (self.xlib.XFlush)(self.display);
        }

        // CRITICAL: Set previous_window_state so sync_window_state() works for future changes
        self.common.previous_window_state = Some(self.common.current_window_state.clone());
    }

    /// Synchronize X11 window properties with current_window_state
    fn sync_window_state(&mut self) {
        use std::ffi::CString;

        // Get copies of previous and current state to avoid borrow checker issues
        let (previous, current) = match &self.common.previous_window_state {
            Some(prev) => (prev.clone(), self.common.current_window_state.clone()),
            None => return, // First frame, nothing to sync
        };

        // Title changed?
        if previous.title != current.title {
            let c_title = CString::new(current.title.as_str()).unwrap();
            unsafe {
                (self.xlib.XStoreName)(self.display, self.window, c_title.as_ptr());
            }
        }

        // Size changed?
        if previous.size.dimensions != current.size.dimensions {
            let width = current.size.dimensions.width as u32;
            let height = current.size.dimensions.height as u32;
            unsafe {
                (self.xlib.XResizeWindow)(self.display, self.window, width, height);
            }
        }

        // Position changed?
        if previous.position != current.position {
            match current.position {
                azul_core::window::WindowPosition::Initialized(pos) => unsafe {
                    (self.xlib.XMoveWindow)(self.display, self.window, pos.x, pos.y);
                },
                // Relative (child) windows are positioned once at creation and not
                // re-synced at runtime; Uninitialized lets the WM decide.
                azul_core::window::WindowPosition::Uninitialized
                | azul_core::window::WindowPosition::RelativeToParentWindow(_) => {}
            }
        }

        // Visibility changed?
        if previous.flags.is_visible != current.flags.is_visible {
            unsafe {
                if current.flags.is_visible {
                    (self.xlib.XMapWindow)(self.display, self.window);
                } else {
                    (self.xlib.XUnmapWindow)(self.display, self.window);
                }
            }
        }

        // Window frame state changed? (Minimize/Maximize/Normal)
        if previous.flags.frame != current.flags.frame {
            use azul_core::window::WindowFrame;
            match current.flags.frame {
                WindowFrame::Minimized => unsafe {
                    (self.xlib.XUnmapWindow)(self.display, self.window);
                },
                WindowFrame::Maximized => unsafe {
                    self.send_wm_state_change(
                        1,
                        b"_NET_WM_STATE_MAXIMIZED_VERT\0",
                        Some(b"_NET_WM_STATE_MAXIMIZED_HORZ\0"),
                    );
                },
                WindowFrame::Normal => {
                    unsafe {
                        if previous.flags.frame == WindowFrame::Maximized {
                            self.send_wm_state_change(
                                0,
                                b"_NET_WM_STATE_MAXIMIZED_VERT\0",
                                Some(b"_NET_WM_STATE_MAXIMIZED_HORZ\0"),
                            );
                        }
                        if previous.flags.frame == WindowFrame::Fullscreen {
                            self.send_wm_state_change(
                                0,
                                b"_NET_WM_STATE_FULLSCREEN\0",
                                None,
                            );
                        }
                        if previous.flags.frame == WindowFrame::Minimized {
                            (self.xlib.XMapWindow)(self.display, self.window);
                        }
                    }
                }
                WindowFrame::Fullscreen => unsafe {
                    if previous.flags.frame == WindowFrame::Maximized {
                        self.send_wm_state_change(
                            0,
                            b"_NET_WM_STATE_MAXIMIZED_VERT\0",
                            Some(b"_NET_WM_STATE_MAXIMIZED_HORZ\0"),
                        );
                    }
                    self.send_wm_state_change(1, b"_NET_WM_STATE_FULLSCREEN\0", None);
                },
            }
        }

        // Always-on-top changed?
        if previous.flags.is_always_on_top != current.flags.is_always_on_top {
            let action = if current.flags.is_always_on_top { 1 } else { 0 };
            unsafe {
                self.send_wm_state_change(action, b"_NET_WM_STATE_ABOVE\0", None);
            }
        }

        // Background material changed? (transparency/blur effects)
        if previous.flags.background_material != current.flags.background_material {
            self.apply_background_material(current.flags.background_material);
        }

        // Check window flags for is_top_level
        if previous.flags.is_top_level != current.flags.is_top_level {
            self.set_is_top_level(current.flags.is_top_level);
        }

        // Check window flags for prevent_system_sleep
        if previous.flags.prevent_system_sleep != current.flags.prevent_system_sleep {
            self.set_prevent_system_sleep(current.flags.prevent_system_sleep);
        }

        // Flush X11 commands
        unsafe {
            (self.xlib.XFlush)(self.display);
        }
    }

    /// Apply window background material for X11
    ///
    /// This function supports two different transparency modes:
    ///
    /// 1. **Background-only transparency** (if `has_argb_visual` is true):
    ///    The window was created with a 32-bit ARGB visual. Background transparency
    ///    is achieved by calling `glClearColor(r, g, b, 0)` - the background becomes
    ///    transparent but rendered content stays fully opaque.
    ///    See: https://stackoverflow.com/a/9215724 (inspired by datenwolf/FTB)
    ///
    /// 2. **Whole-window transparency** (if `has_argb_visual` is false):
    ///    Uses `_NET_WM_WINDOW_OPACITY` which affects the entire window including
    ///    rendered content. Useful for effects like fading tooltips in/out.
    ///
    /// For blur effects: X11 has no standard blur protocol - depends on compositor
    /// (picom, compton, etc.). We use ~88% opacity as a fallback hint.
    fn apply_background_material(&mut self, material: azul_core::window::WindowBackgroundMaterial) {
        use azul_core::window::WindowBackgroundMaterial;

        if self.has_argb_visual {
            // ARGB visual mode: background transparency is handled by the OpenGL clear color
            // The actual transparency is achieved when rendering clears with alpha=0
            // Here we just log the state - the actual glClearColor call happens in the renderer
            match material {
                WindowBackgroundMaterial::Opaque => {
                    log_debug!(
                        LogCategory::Platform,
                        "[X11/ARGB] Background material: Opaque (renderer should use alpha=1.0)"
                    );
                }
                WindowBackgroundMaterial::Transparent => {
                    log_debug!(LogCategory::Platform,
                        "[X11/ARGB] Background material: Transparent (renderer should use alpha=0.0)");
                }
                _ => {
                    // For blur types, we could potentially set a hint opacity too
                    log_debug!(LogCategory::Platform,
                        "[X11/ARGB] Background material: {:?} (semi-transparent, renderer uses alpha<1.0)", material);
                }
            }
            // Note: We don't set _NET_WM_WINDOW_OPACITY here because that would
            // make the entire window (including content) transparent, which defeats
            // the purpose of ARGB visual transparency.
            return;
        }

        // Non-ARGB mode: Use _NET_WM_WINDOW_OPACITY for whole-window transparency
        // Map material to opacity value (32-bit cardinal, 0xFFFFFFFF = fully opaque)
        let opacity: u32 = match material {
            WindowBackgroundMaterial::Opaque => 0xFFFFFFFF,
            WindowBackgroundMaterial::Transparent => 0x00000000,
            // ~88% opaque for blur/translucent types (compositor-dependent blur)
            WindowBackgroundMaterial::Sidebar
            | WindowBackgroundMaterial::Menu
            | WindowBackgroundMaterial::HUD
            | WindowBackgroundMaterial::Titlebar
            | WindowBackgroundMaterial::MicaAlt => 0xE0000000,
        };

        unsafe {
            // Get the _NET_WM_WINDOW_OPACITY atom
            let opacity_atom = (self.xlib.XInternAtom)(
                self.display,
                b"_NET_WM_WINDOW_OPACITY\0".as_ptr() as *const c_char,
                0, // create if doesn't exist
            );

            let cardinal_atom =
                (self.xlib.XInternAtom)(self.display, b"CARDINAL\0".as_ptr() as *const c_char, 0);

            // Set the opacity property
            (self.xlib.XChangeProperty)(
                self.display,
                self.window,
                opacity_atom,
                cardinal_atom,
                32, // format (32-bit)
                0,  // PropModeReplace
                &opacity as *const u32 as *const u8,
                1, // nelements
            );

            log_debug!(
                LogCategory::Platform,
                "[X11/Opacity] Applied background material {:?} (whole-window opacity: 0x{:08X})",
                material,
                opacity
            );
        }
    }

    /// Set the mouse cursor for this window
    fn set_cursor(&mut self, cursor_type: azul_core::window::MouseCursorType) {
        use defines::*;

        // Map MouseCursorType to X11 cursor constants
        let cursor_id = match cursor_type {
            azul_core::window::MouseCursorType::Default
            | azul_core::window::MouseCursorType::Arrow => XC_left_ptr,
            azul_core::window::MouseCursorType::Crosshair => XC_crosshair,
            azul_core::window::MouseCursorType::Hand => XC_hand2,
            azul_core::window::MouseCursorType::Move => XC_fleur,
            azul_core::window::MouseCursorType::Text => XC_xterm,
            azul_core::window::MouseCursorType::Wait => XC_watch,
            azul_core::window::MouseCursorType::Progress => XC_watch,
            azul_core::window::MouseCursorType::NotAllowed => XC_X_cursor,
            azul_core::window::MouseCursorType::EResize => XC_right_side,
            azul_core::window::MouseCursorType::NResize => XC_top_side,
            azul_core::window::MouseCursorType::NeResize => XC_top_right_corner,
            azul_core::window::MouseCursorType::NwResize => XC_top_left_corner,
            azul_core::window::MouseCursorType::SResize => XC_bottom_side,
            azul_core::window::MouseCursorType::SeResize => XC_bottom_right_corner,
            azul_core::window::MouseCursorType::SwResize => XC_bottom_left_corner,
            azul_core::window::MouseCursorType::WResize => XC_left_side,
            azul_core::window::MouseCursorType::EwResize => XC_sb_h_double_arrow,
            azul_core::window::MouseCursorType::NsResize => XC_sb_v_double_arrow,
            azul_core::window::MouseCursorType::NeswResize => XC_sizing,
            azul_core::window::MouseCursorType::NwseResize => XC_sizing,
            azul_core::window::MouseCursorType::ColResize => XC_sb_h_double_arrow,
            azul_core::window::MouseCursorType::RowResize => XC_sb_v_double_arrow,
            // Additional cursor types that may not have exact X11 equivalents
            azul_core::window::MouseCursorType::Help => XC_left_ptr, // No help cursor in X11
            azul_core::window::MouseCursorType::ContextMenu => XC_left_ptr,
            azul_core::window::MouseCursorType::Cell => XC_crosshair,
            azul_core::window::MouseCursorType::VerticalText => XC_xterm,
            azul_core::window::MouseCursorType::Alias => XC_hand2,
            azul_core::window::MouseCursorType::Copy => XC_hand2,
            azul_core::window::MouseCursorType::NoDrop => XC_X_cursor,
            azul_core::window::MouseCursorType::Grab => XC_hand2,
            azul_core::window::MouseCursorType::Grabbing => XC_fleur,
            azul_core::window::MouseCursorType::AllScroll => XC_fleur,
            azul_core::window::MouseCursorType::ZoomIn => XC_left_ptr,
            azul_core::window::MouseCursorType::ZoomOut => XC_left_ptr,
        };

        unsafe {
            let cursor = (self.xlib.XCreateFontCursor)(self.display, cursor_id);
            (self.xlib.XDefineCursor)(self.display, self.window, cursor);
            (self.xlib.XFreeCursor)(self.display, cursor);
        }
    }

    /// Get display information for the screen this window is on
    pub fn get_window_display_info(&self) -> Option<crate::desktop::display::DisplayInfo> {
        use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};

        unsafe {
            let screen = (self.xlib.XDefaultScreen)(self.display);

            // Get screen dimensions in pixels
            let width_px = (self.xlib.XDisplayWidth)(self.display, screen);
            let height_px = (self.xlib.XDisplayHeight)(self.display, screen);

            // Get screen dimensions in millimeters for DPI calculation
            let width_mm = (self.xlib.XDisplayWidthMM)(self.display, screen);
            let height_mm = (self.xlib.XDisplayHeightMM)(self.display, screen);

            // Calculate DPI
            let dpi_x = if width_mm > 0 {
                (width_px as f32 / width_mm as f32) * 25.4
            } else {
                96.0
            };

            let dpi_y = if height_mm > 0 {
                (height_px as f32 / height_mm as f32) * 25.4
            } else {
                96.0
            };

            // Use average DPI for scale factor
            let avg_dpi = (dpi_x + dpi_y) / 2.0;
            let scale_factor = avg_dpi / 96.0;

            let bounds = LogicalRect::new(
                LogicalPosition::zero(),
                LogicalSize::new(width_px as f32, height_px as f32),
            );

            // Approximate work area by subtracting common panel height
            let work_area = LogicalRect::new(
                LogicalPosition::zero(),
                LogicalSize::new(width_px as f32, (height_px - 24).max(0) as f32),
            );

            Some(crate::desktop::display::DisplayInfo {
                name: format!(":0.{}", screen),
                bounds,
                work_area,
                scale_factor,
                is_primary: true,
                video_modes: vec![azul_core::window::VideoMode {
                    size: azul_css::props::basic::LayoutSize::new(
                        width_px as isize,
                        height_px as isize,
                    ),
                    bit_depth: 32,
                    refresh_rate: 60,
                }],
            })
        }
    }
}

// PlatformWindow Trait Implementation

impl PlatformWindow for X11Window {

    impl_platform_window_getters!(common);

    fn get_raw_window_handle(&self) -> RawWindowHandle {
        RawWindowHandle::Xlib(XlibHandle {
            window: self.window as u64,
            display: self.display as *mut c_void,
        })
    }

    fn prepare_callback_invocation(&mut self) -> event::InvokeSingleCallbackBorrows {
        let layout_window = self.common
            .layout_window
            .as_mut()
            .expect("Layout window must exist for callback invocation");

        event::InvokeSingleCallbackBorrows {
            layout_window,
            window_handle: RawWindowHandle::Xlib(XlibHandle {
                window: self.window as u64,
                display: self.display as *mut c_void,
            }),
            gl_context_ptr: &self.common.gl_context_ptr,
            image_cache: &mut self.common.image_cache,
            fc_cache_clone: (*self.common.fc_cache).clone(),
            system_style: self.common.system_style.clone(),
            previous_window_state: &self.common.previous_window_state,
            current_window_state: &self.common.current_window_state,
            renderer_resources: &mut self.common.renderer_resources,
        }
    }

    // Timer Management (X11 Implementation - uses timerfd for native OS timer support)

    fn start_timer(&mut self, timer_id: usize, timer: azul_layout::timer::Timer) {
        let interval_ms = timer.tick_millis();
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window
                .timers
                .insert(azul_core::task::TimerId { id: timer_id }, timer);
        }
        super::timer::start_timerfd(&mut self.timer_fds, timer_id, interval_ms, "X11");
    }

    fn stop_timer(&mut self, timer_id: usize) {
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window
                .timers
                .remove(&azul_core::task::TimerId { id: timer_id });
        }
        super::timer::stop_timerfd(&mut self.timer_fds, timer_id, "X11");
    }

    // Thread Management (X11 Implementation - Stored in LayoutWindow)

    fn start_thread_poll_timer(&mut self) {
        // For X11, we don't need a separate timer - threads are checked
        // in the event loop when layout_window.threads is non-empty
        // Just mark for regeneration to start checking
        self.common.frame_needs_regeneration = true;
    }

    fn stop_thread_poll_timer(&mut self) {
        // No-op for X11 - thread checking stops automatically when
        // layout_window.threads becomes empty
    }

    fn add_threads(
        &mut self,
        threads: std::collections::BTreeMap<azul_core::task::ThreadId, azul_layout::thread::Thread>,
    ) {
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            for (thread_id, thread) in threads {
                layout_window.threads.insert(thread_id, thread);
            }
        }

        // Mark for regeneration to start thread polling
        self.common.frame_needs_regeneration = true;
    }

    fn remove_threads(
        &mut self,
        thread_ids: &std::collections::BTreeSet<azul_core::task::ThreadId>,
    ) {
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            for thread_id in thread_ids {
                layout_window.threads.remove(thread_id);
            }
        }
    }

    fn queue_window_create(&mut self, options: azul_layout::window_state::WindowCreateOptions) {
        self.pending_window_creates.push(options);
    }

    // REQUIRED: Menu Display

    fn show_menu_from_callback(
        &mut self,
        menu: &azul_core::menu::Menu,
        position: azul_core::geom::LogicalPosition,
    ) {
        // Check if native menus are enabled (GNOME menus on Linux)
        if self.common.current_window_state.flags.use_native_context_menus {
            // TODO: Show GNOME native menu via DBus
            log_debug!(
                LogCategory::Window,
                "[X11] Native GNOME menu at ({}, {}) - not yet implemented, using fallback",
                position.x,
                position.y
            );
            self.show_fallback_menu(menu, position);
        } else {
            // Show fallback DOM-based menu
            self.show_fallback_menu(menu, position);
        }
    }

    // Tooltip Methods (X11 Implementation)

    fn show_tooltip_from_callback(
        &mut self,
        text: &str,
        position: azul_core::geom::LogicalPosition,
    ) {
        // Convert logical position to screen coordinates
        let window_pos = match self.common.current_window_state.position {
            azul_core::window::WindowPosition::Initialized(pos) => (pos.x, pos.y),
            _ => (0, 0),
        };

        let screen_x = window_pos.0 + position.x as i32;
        let screen_y = window_pos.1 + position.y as i32;

        self.show_tooltip(text.to_string(), screen_x, screen_y);
    }

    fn hide_tooltip_from_callback(&mut self) {
        self.hide_tooltip();
    }

    fn sync_window_state(&mut self) {
        X11Window::sync_window_state(self);
    }
}

impl X11Window {
    /// Show a fallback window-based menu at the given position
    fn show_fallback_menu(
        &mut self,
        menu: &azul_core::menu::Menu,
        position: azul_core::geom::LogicalPosition,
    ) {
        // Get parent window position
        let parent_pos = match self.common.current_window_state.position {
            azul_core::window::WindowPosition::Initialized(pos) => {
                azul_core::geom::LogicalPosition::new(pos.x as f32, pos.y as f32)
            }
            _ => azul_core::geom::LogicalPosition::new(0.0, 0.0),
        };

        // Create menu window options
        let mut menu_options = crate::desktop::menu::show_menu(
            menu.clone(),
            self.common.system_style.clone(),
            parent_pos,
            None,           // No trigger rect
            Some(position), // Position for menu
            None,           // No parent menu
        );
        // Parent the menu to THIS window so it reuses our X display (single
        // shared event pump) and is positioned relative to us.
        menu_options.parent_window_id = self.window as u64;

        // Queue window creation request
        log_debug!(
            LogCategory::Window,
            "[X11] Queuing fallback menu window at ({}, {}) - will be created in event loop",
            position.x,
            position.y
        );

        self.pending_window_creates.push(menu_options);
    }
}

// Private helper methods for X11Window
impl X11Window {
    /// Show a tooltip at the given position (X11 implementation)
    fn show_tooltip(&mut self, text: String, x: i32, y: i32) {
        // Create tooltip window if needed
        if self.tooltip.is_none() {
            match tooltip::TooltipWindow::new(self.xlib.clone(), self.display) {
                Ok(tooltip_window) => {
                    self.tooltip = Some(tooltip_window);
                }
                Err(e) => {
                    log_error!(
                        LogCategory::Window,
                        "[X11] Failed to create tooltip window: {}",
                        e
                    );
                    return;
                }
            }
        }

        // Show tooltip
        if let Some(tooltip) = self.tooltip.as_mut() {
            use azul_core::{geom::LogicalPosition, resources::DpiScaleFactor};

            let position = LogicalPosition::new(x as f32, y as f32);
            let dpi = DpiScaleFactor::new(self.common.current_window_state.size.dpi as f32 / 96.0);

            if let Err(e) = tooltip.show(&text, position, dpi) {
                log_error!(LogCategory::Window, "[X11] Failed to show tooltip: {}", e);
            }
        }
    }

    /// Hide the tooltip (X11 implementation)
    fn hide_tooltip(&mut self) {
        if let Some(tooltip) = self.tooltip.as_mut() {
            let _ = tooltip.hide();
        }
    }
}

impl Drop for X11Window {
    fn drop(&mut self) {
        // Close all timerfd's
        for (_timer_id, fd) in std::mem::take(&mut self.timer_fds) {
            unsafe {
                libc::close(fd);
            }
        }

        // Unregister from global registry before closing
        super::registry::unregister_window(self.window as u64);

        // Tear down the GL context / graphics context BEFORE the X resources it
        // depends on. close() destroys the X window and (for a display owner)
        // closes the X display; the EGL context / GC live in `render_mode`, a
        // struct field that would otherwise drop AFTER close() returns — i.e.
        // eglDestroyContext / XFreeGC would run against a destroyed window /
        // closed display, crashing on exit (the exit-time GL crash). Assigning
        // None drops the old RenderMode (running its GL teardown) while the
        // window + display are still alive, fixing the teardown ordering.
        self.render_mode = RenderMode::None;
        self.close();
    }
}

// IME Position Management

impl X11Window {
    /// Sync ime_position from window state to OS
    /// Sync IME position to OS (X11 with XIM)
    pub fn sync_ime_position_to_os(&self) {
        use azul_core::window::ImePosition;
        use defines::XPoint;

        if let ImePosition::Initialized(rect) = self.common.current_window_state.ime_position {
            // Use XIM if available (preferred over GTK)
            if let Some(ref ime_mgr) = self.ime_manager {
                let spot = XPoint {
                    x: rect.origin.x as i16,
                    y: rect.origin.y as i16,
                };

                // XNSpotLocation must be wrapped in an XNPreeditAttributes
                // nested list per the XIM spec. The IM only consults it when
                // the negotiated input style is XIMPreeditPosition, but it
                // costs almost nothing to push for other styles too.
                unsafe {
                    let xic = ime_mgr.get_xic();
                    let nested = (self.xlib.XVaCreateNestedList)(
                        0,
                        defines::XN_SPOT_LOCATION.as_ptr() as *const c_char,
                        &spot as *const XPoint,
                        std::ptr::null::<i8>(),
                    );
                    if !nested.is_null() {
                        (self.xlib.XSetICValues)(
                            xic,
                            defines::XN_PREEDIT_ATTRIBUTES.as_ptr() as *const c_char,
                            nested,
                            std::ptr::null::<i8>(),
                        );
                        (self.xlib.XFree)(nested);
                    }
                }
                return;
            }

            // Fallback to GTK IM context if XIM not available
            if let (Some(ref gtk_im), Some(ctx)) = (&self.gtk_im, self.gtk_im_context) {
                let gdk_rect = dlopen::GdkRectangle {
                    x: rect.origin.x as i32,
                    y: rect.origin.y as i32,
                    width: rect.size.width as i32,
                    height: rect.size.height as i32,
                };

                unsafe {
                    (gtk_im.gtk_im_context_set_cursor_location)(ctx, &gdk_rect);
                }
            }
        }
    }
}

impl X11Window {
    /// Send a `_NET_WM_STATE` client message to add or remove a window manager state atom.
    ///
    /// `action`: 0 = remove, 1 = add.
    /// `atom1_name`: null-terminated atom name (e.g. `b"_NET_WM_STATE_ABOVE\0"`).
    /// `atom2_name`: optional second atom (used for maximize vert+horz).
    unsafe fn send_wm_state_change(
        &self,
        action: std::os::raw::c_long,
        atom1_name: &[u8],
        atom2_name: Option<&[u8]>,
    ) {
        let screen = (self.xlib.XDefaultScreen)(self.display);
        let root = (self.xlib.XRootWindow)(self.display, screen);

        let mut event: defines::XClientMessageEvent = std::mem::zeroed();
        event.type_ = defines::ClientMessage;
        event.window = self.window;
        event.message_type = (self.xlib.XInternAtom)(
            self.display,
            b"_NET_WM_STATE\0".as_ptr() as *const c_char,
            0,
        );
        event.format = 32;
        event.data.l[0] = action;
        event.data.l[1] =
            (self.xlib.XInternAtom)(self.display, atom1_name.as_ptr() as *const c_char, 0)
                as std::os::raw::c_long;
        if let Some(a2) = atom2_name {
            event.data.l[2] =
                (self.xlib.XInternAtom)(self.display, a2.as_ptr() as *const c_char, 0)
                    as std::os::raw::c_long;
        }
        event.data.l[3] = 1;

        (self.xlib.XSendEvent)(
            self.display,
            root,
            0,
            defines::SubstructureNotifyMask | defines::SubstructureRedirectMask,
            &mut event as *mut _ as *mut defines::XEvent,
        );
    }

    /// Check timers and threads, trigger callbacks if needed.
    /// This is called on every poll_event() to simulate timer ticks.
    /// If any timer/thread callback requested a visual update, trigger a redraw
    /// so that scroll offsets / GPU values are sent to WebRender.
    fn check_timers_and_threads(&mut self) {
        use super::super::common::event::PlatformWindow;
        if self.process_timers_and_threads() {
            self.request_redraw();
        }
    }

    /// Set the window to always be on top (X11 implementation using _NET_WM_STATE_ABOVE)
    fn set_is_top_level(&mut self, is_top_level: bool) {
        unsafe {
            // Get _NET_WM_STATE atom
            let net_wm_state =
                (self.xlib.XInternAtom)(self.display, b"_NET_WM_STATE\0".as_ptr() as *const c_char, 0);

            // Get _NET_WM_STATE_ABOVE atom
            let net_wm_state_above = (self.xlib.XInternAtom)(
                self.display,
                b"_NET_WM_STATE_ABOVE\0".as_ptr() as *const c_char,
                0,
            );

            if is_top_level {
                // Add _NET_WM_STATE_ABOVE to window properties
                // Convert to u32 for X11 protocol compliance (format=32 means 32-bit values)
                let atom_u32 = net_wm_state_above as u32;
                (self.xlib.XChangeProperty)(
                    self.display,
                    self.window,
                    net_wm_state,
                    defines::XA_ATOM,
                    32,
                    defines::PropModeAppend,
                    &atom_u32 as *const _ as *const u8,
                    1,
                );
            } else {
                // Remove _NET_WM_STATE_ABOVE from window properties
                // First, get current state
                let mut actual_type: Atom = 0;
                let mut actual_format: i32 = 0;
                let mut nitems: std::os::raw::c_ulong = 0;
                let mut bytes_after: std::os::raw::c_ulong = 0;
                let mut prop: *mut u8 = std::ptr::null_mut();

                let result = (self.xlib.XGetWindowProperty)(
                    self.display,
                    self.window,
                    net_wm_state,
                    0,
                    1024,
                    0,
                    defines::XA_ATOM,
                    &mut actual_type,
                    &mut actual_format,
                    &mut nitems,
                    &mut bytes_after,
                    &mut prop,
                );

                if result == 0
                    && !prop.is_null()
                    && actual_type == defines::XA_ATOM
                    && actual_format == 32
                {
                    // Read atoms as u32 (protocol uses 32-bit values even on 64-bit systems)
                    let atoms = std::slice::from_raw_parts(prop as *const u32, nitems as usize);
                    let net_wm_state_above_u32 = net_wm_state_above as u32;

                    let mut new_atoms: Vec<u32> = atoms
                        .iter()
                        .filter(|&&atom| atom != net_wm_state_above_u32)
                        .copied()
                        .collect();

                    // Replace property with filtered list
                    (self.xlib.XChangeProperty)(
                        self.display,
                        self.window,
                        net_wm_state,
                        defines::XA_ATOM,
                        32,
                        defines::PropModeReplace,
                        new_atoms.as_mut_ptr() as *const u8,
                        new_atoms.len() as i32,
                    );

                    (self.xlib.XFree)(prop as *mut c_void);
                }
            }

            (self.xlib.XFlush)(self.display);
        }
    }

    /// Prevent the system from sleeping (X11 implementation using D-Bus ScreenSaver inhibit)
    fn set_prevent_system_sleep(&mut self, prevent: bool) {
        use std::ffi::CString;

        use super::dbus;

        if prevent {
            // Already inhibited?
            if self.screensaver_inhibit_cookie.is_some() {
                return;
            }

            // Get shared D-Bus library (loaded once, shared across all windows)
            let dbus_lib = match super::gnome_menu::get_shared_dbus_lib() {
                Some(lib) => lib,
                None => {
                    log_warn!(
                        LogCategory::Platform,
                        "[X11] Failed to load D-Bus library"
                    );
                    log_warn!(
                        LogCategory::Platform,
                        "[X11] System sleep prevention not available"
                    );
                    return;
                }
            };

            // Connect to session bus if not already connected
            if self.dbus_connection.is_none() {
                unsafe {
                    let mut error: dbus::DBusError = std::mem::zeroed();
                    (dbus_lib.dbus_error_init)(&mut error);

                    let conn = (dbus_lib.dbus_bus_get)(dbus::DBUS_BUS_SESSION, &mut error);
                    if (dbus_lib.dbus_error_is_set)(&error) != 0 {
                        log_error!(
                            LogCategory::Platform,
                            "[X11] Failed to connect to D-Bus session bus"
                        );
                        (dbus_lib.dbus_error_free)(&mut error);
                        return;
                    }

                    self.dbus_connection = Some(conn);
                }
            }

            let conn = match self.dbus_connection {
                Some(c) => c,
                None => return,
            };

            unsafe {
                // Create method call: org.freedesktop.ScreenSaver.Inhibit(app_name, reason)
                let destination = CString::new("org.freedesktop.ScreenSaver").unwrap();
                let path = CString::new("/org/freedesktop/ScreenSaver").unwrap();
                let interface = CString::new("org.freedesktop.ScreenSaver").unwrap();
                let method = CString::new("Inhibit").unwrap();

                let msg = (dbus_lib.dbus_message_new_method_call)(
                    destination.as_ptr(),
                    path.as_ptr(),
                    interface.as_ptr(),
                    method.as_ptr(),
                );

                if msg.is_null() {
                    log_error!(
                        LogCategory::Platform,
                        "[X11] Failed to create D-Bus method call"
                    );
                    return;
                }

                // Append arguments: app_name (string), reason (string)
                let app_name = CString::new("Azul GUI Application").unwrap();
                let reason = CString::new("Video playback or presentation mode").unwrap();

                let mut iter: dbus::DBusMessageIter = std::mem::zeroed();
                (dbus_lib.dbus_message_iter_init_append)(msg, &mut iter);

                let app_name_ptr = app_name.as_ptr();
                (dbus_lib.dbus_message_iter_append_basic)(
                    &mut iter,
                    dbus::DBUS_TYPE_STRING,
                    &app_name_ptr as *const _ as *const c_void,
                );

                let reason_ptr = reason.as_ptr();
                (dbus_lib.dbus_message_iter_append_basic)(
                    &mut iter,
                    dbus::DBUS_TYPE_STRING,
                    &reason_ptr as *const _ as *const c_void,
                );

                // Send with reply and wait for cookie
                let mut error: dbus::DBusError = std::mem::zeroed();
                (dbus_lib.dbus_error_init)(&mut error);

                let reply = (dbus_lib.dbus_connection_send_with_reply_and_block)(
                    conn, msg, -1, // default timeout
                    &mut error,
                );

                (dbus_lib.dbus_message_unref)(msg);

                if (dbus_lib.dbus_error_is_set)(&error) != 0 {
                    log_error!(
                        LogCategory::Platform,
                        "[X11] D-Bus ScreenSaver.Inhibit failed"
                    );
                    (dbus_lib.dbus_error_free)(&mut error);
                    return;
                }

                if reply.is_null() {
                    log_error!(
                        LogCategory::Platform,
                        "[X11] D-Bus ScreenSaver.Inhibit returned no reply"
                    );
                    return;
                }

                // Parse reply to get the cookie (uint32)
                let mut reply_iter: dbus::DBusMessageIter = std::mem::zeroed();
                if (dbus_lib.dbus_message_iter_init)(reply, &mut reply_iter) == 0 {
                    log_error!(LogCategory::Platform, "[X11] D-Bus reply has no arguments");
                    (dbus_lib.dbus_message_unref)(reply);
                    return;
                }

                let arg_type = (dbus_lib.dbus_message_iter_get_arg_type)(&mut reply_iter);
                if arg_type != dbus::DBUS_TYPE_UINT32 {
                    log_error!(
                        LogCategory::Platform,
                        "[X11] D-Bus reply has wrong type: expected uint32"
                    );
                    (dbus_lib.dbus_message_unref)(reply);
                    return;
                }

                let mut cookie: u32 = 0;
                (dbus_lib.dbus_message_iter_get_basic)(
                    &mut reply_iter,
                    &mut cookie as *mut _ as *mut c_void,
                );

                self.screensaver_inhibit_cookie = Some(cookie);
                (dbus_lib.dbus_message_unref)(reply);

                log_info!(
                    LogCategory::Platform,
                    "[X11] System sleep prevented (cookie: {})",
                    cookie
                );
            }
        } else {
            // Remove inhibit
            let cookie = match self.screensaver_inhibit_cookie.take() {
                Some(c) => c,
                None => return, // Not inhibited
            };

            let conn = match self.dbus_connection {
                Some(c) => c,
                None => return,
            };

            // Get shared D-Bus library
            let dbus_lib = match super::gnome_menu::get_shared_dbus_lib() {
                Some(lib) => lib,
                None => {
                    log_warn!(
                        LogCategory::Platform,
                        "[X11] Failed to load D-Bus library"
                    );
                    return;
                }
            };

            unsafe {
                // Create method call: org.freedesktop.ScreenSaver.UnInhibit(cookie)
                let destination = CString::new("org.freedesktop.ScreenSaver").unwrap();
                let path = CString::new("/org/freedesktop/ScreenSaver").unwrap();
                let interface = CString::new("org.freedesktop.ScreenSaver").unwrap();
                let method = CString::new("UnInhibit").unwrap();

                let msg = (dbus_lib.dbus_message_new_method_call)(
                    destination.as_ptr(),
                    path.as_ptr(),
                    interface.as_ptr(),
                    method.as_ptr(),
                );

                if msg.is_null() {
                    log_error!(
                        LogCategory::Platform,
                        "[X11] Failed to create D-Bus method call"
                    );
                    return;
                }

                // Append argument: cookie (uint32)
                let mut iter: dbus::DBusMessageIter = std::mem::zeroed();
                (dbus_lib.dbus_message_iter_init_append)(msg, &mut iter);
                (dbus_lib.dbus_message_iter_append_basic)(
                    &mut iter,
                    dbus::DBUS_TYPE_UINT32,
                    &cookie as *const _ as *const c_void,
                );

                // Send (no reply needed)
                let mut error: dbus::DBusError = std::mem::zeroed();
                (dbus_lib.dbus_error_init)(&mut error);

                let reply = (dbus_lib.dbus_connection_send_with_reply_and_block)(
                    conn, msg, -1, // default timeout
                    &mut error,
                );

                (dbus_lib.dbus_message_unref)(msg);

                if (dbus_lib.dbus_error_is_set)(&error) != 0 {
                    log_error!(
                        LogCategory::Platform,
                        "[X11] D-Bus ScreenSaver.UnInhibit failed"
                    );
                    (dbus_lib.dbus_error_free)(&mut error);
                    return;
                }

                if !reply.is_null() {
                    (dbus_lib.dbus_message_unref)(reply);
                }

                log_info!(
                    LogCategory::Platform,
                    "[X11] System sleep allowed (cookie: {})",
                    cookie
                );
            }
        }
    }
}
