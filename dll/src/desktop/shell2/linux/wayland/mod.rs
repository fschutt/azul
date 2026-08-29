//! Wayland implementation for Linux.
//!
//! This module implements the PlatformWindow trait for Wayland.
//! It supports GPU-accelerated rendering via EGL and WebRender, with a
//! fallback to a CPU-rendered surface if GL context creation fails.
//!
//! Key subsystems:
//! - Dual render paths: GPU (EGL/WebRender) and CPU (wl_shm shared memory)
//! - Input handling: XKB keyboard translation, pointer events, scroll physics
//! - IME support: text-input v3 protocol with GTK IM context fallback
//! - Tooltips: wl_subsurface-based tooltip windows
//! - Popups: xdg_popup for context menus
//! - D-Bus screensaver inhibition (org.freedesktop.ScreenSaver)
//! - KDE blur protocol (org.kde.kwin.blur) for material effects
//!
//! Note: Uses dynamic loading (dlopen) to avoid linker errors
//! and ensure compatibility across Linux distributions.

use crate::impl_platform_window_getters;

// ============================================================================
// PLATFORM TRACE — deliberately NOT behind `feature = "logging"`
// ============================================================================
//
// Every `log_debug!` / `log_warn!` / `log_error!` in this backend expands to
// NOTHING unless azul-dll is built with the `logging` feature. That feature is
// in `default`, but an application that links azul-dll with
// `default-features = false` (miniword does, and so does anything following the
// lean `link-static` recipe) silently loses the entire platform log.
//
// That is not hypothetical: on 2026-08-07 a Wayland protocol error disconnected
// the client mid-run. `poll_event`'s dead-connection detector was present and
// correct, and printed nothing at all, because its `log_warn!` had been compiled
// out. The run produced `Error sending request: Broken pipe` from libwayland and
// no azul diagnosis whatsoever..
//
// So the OS-level trace uses `eprintln!` and a RUNTIME switch. A diagnostic that
// a build flag can silently delete is not a diagnostic.
//
//   AZ_WL_TRACE=1                    -> on
//   AZ_LOG=trace | debug | 1 | on    -> on  (AZ_LOG=off/0/none wins and forces off)
//
// Fatal conditions (lost connection, protocol error) print UNCONDITIONALLY and
// ignore both switches.

/// Runtime gate for [`wl_trace!`].
///
/// Delegates to [`azul_core::log_filter`] — the SAME gate every `log_*!` uses —
/// rather than parsing `AZ_LOG` a second time. The second parser is not a
/// hypothetical hazard: the first version of this function matched the whole
/// variable against `"debug"`, so `AZ_LOG=debug,-layout` (the invocation that
/// makes the log readable) silently produced no platform trace at all.
///
/// `AZ_WL_TRACE=1` forces it on regardless of level, for the case where you
/// want ONLY this trace and nothing else.
pub(super) fn wl_trace_enabled() -> bool {
    static FORCED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *FORCED.get_or_init(|| std::env::var_os("AZ_WL_TRACE").is_some()) {
        return true;
    }
    azul_core::log_filter::enabled(
        azul_core::log_filter::Category::Platform,
        azul_core::log_filter::Level::Debug,
    )
}

/// Wayland/OS-level trace line. Prefixed `[WL]` so a run log can be filtered
/// down to the platform layer with a single grep.
macro_rules! wl_trace {
    ($($arg:tt)*) => {
        if $crate::desktop::shell2::linux::wayland::wl_trace_enabled() {
            // Through the shared sink, so this trace gets the same timestamp
            // and the same `AZ_LOG_FILE` destination as everything else. It
                // used to `eprintln!` directly, which is why the mouse-resize
            // capture had 1 500 untimed `[WL]` lines in it.
            $crate::desktop::shell2::common::log_gate::emit(
                $crate::desktop::shell2::common::debug_server::LogLevel::Debug,
                $crate::desktop::shell2::common::debug_server::LogCategory::Platform,
                format!("[WL] {}", format_args!($($arg)*)),
            );
        }
    };
}
pub(super) use wl_trace;

/// Live/created/destroyed `CpuFallbackState` census.
///
/// §29 of the RSS map counted 1 279 `wl_shm_pool` proxies still attached at
/// teardown of an interactive session and asked for exactly this measurement:
/// "count pools created against configure events in the SAME run". These are
/// that count, and they are always maintained (the atomics are free) so the
/// numbers are available the moment a trace is switched on.
pub(super) static POOLS_CREATED: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);
pub(super) static POOLS_DESTROYED: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);
/// `xdg_toplevel.configure` events seen, and how many of those changed the size.
pub(super) static CONFIGURES_SEEN: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);
pub(super) static CONFIGURES_RESIZED: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);

/// Spell out the errno `wl_display_get_error` returns, because the two common
/// values mean opposite things about WHERE the fault is:
///
/// * `EPIPE` — the compositor had already closed the socket when we wrote. The
///   protocol violation happened BEFORE this and is not visible here; the
///   compositor's own log (`error in client communication (pid N)` from
///   libwayland-server) is the corroborating record.
/// * `EPROTO` — libwayland raised the error locally; `wl_display_get_protocol_error`
///   then names the interface, object id and error code.
pub(super) fn errno_name(e: i32) -> &'static str {
    match e {
        0 => "no error",
        libc::EPIPE => "EPIPE — compositor closed the socket before this write",
        libc::EPROTO => "EPROTO — protocol error",
        libc::ECONNRESET => "ECONNRESET",
        libc::EINVAL => "EINVAL",
        libc::ENOMEM => "ENOMEM",
        _ => "see errno(3)",
    }
}

/// Trim IME surrounding text to the protocol's size budget, keeping the
/// cursor (and, when it fits, the anchor) inside the returned window.
///
/// `zwp_text_input_v3.set_surrounding_text` is documented with a 4000-byte
/// budget, and libwayland enforces a HARD ~4096-byte cap on any single wire
/// message — a longer string does not get truncated, it makes the send fail
/// and the compositor CLOSE THE CONNECTION ("error in client communication",
/// then EPIPE client-side, then the window vanishes while the process lives).
/// miniword sent the focused node's ENTIRE text here, so clicking into any
/// paragraph over ~4 KB was fatal. This is the strongest candidate yet for
/// the long-standing "selecting text kills the window" crash — it survived
/// the data_offer fix because it is a different message on the same socket.
///
/// Returns `(byte_range_of_window, cursor_in_window, anchor_in_window)`;
/// offsets are rebased to the window and clamped to it (the spec wants both
/// inside the sent text; when the selection itself exceeds the budget the
/// anchor is clamped to the window edge — the compositor still gets valid,
/// self-consistent context, just less of it).
fn trim_surrounding_text(
    text: &str,
    cursor_byte: usize,
    anchor_byte: usize,
) -> (core::ops::Range<usize>, i32, i32) {
    /// Comfortably under both the 4000-byte protocol budget and libwayland's
    /// 4096-byte message cap (which must also fit the header + two ints).
    const BUDGET: usize = 3800;

    let len = text.len();
    // Defensive: offsets are computed from a parallel text extraction and may
    // disagree with `text` — clamp instead of trusting them.
    let cursor = cursor_byte.min(len);
    let anchor = anchor_byte.min(len);

    if len <= BUDGET {
        return (0..len, cursor as i32, anchor as i32);
    }

    let (lo, hi) = (cursor.min(anchor), cursor.max(anchor));
    // Centre the window on the selection when it fits, on the CURSOR when the
    // selection alone exceeds the budget.
    let (centre_lo, centre_hi) = if hi - lo <= BUDGET {
        (lo, hi)
    } else {
        (cursor, cursor)
    };
    let slack = BUDGET - (centre_hi - centre_lo);
    let mut start = centre_lo.saturating_sub(slack / 2);
    let mut end = (start + BUDGET).min(len);
    start = end.saturating_sub(BUDGET);

    // Snap INWARD to char boundaries — snapping outward could re-exceed the
    // budget, and a non-boundary split would send invalid UTF-8 (its own
    // protocol violation).
    while start < len && !text.is_char_boundary(start) {
        start += 1;
    }
    while end > start && !text.is_char_boundary(end) {
        end -= 1;
    }

    let rebase = |b: usize| -> i32 { (b.clamp(start, end) - start) as i32 };
    (start..end, rebase(cursor), rebase(anchor))
}

/// One-line pool census: created / destroyed / still live.
pub(super) fn pool_census() -> String {
    use core::sync::atomic::Ordering::Relaxed;
    let c = POOLS_CREATED.load(Relaxed);
    let d = POOLS_DESTROYED.load(Relaxed);
    format!(
        "pools created={c} destroyed={d} live={}",
        c.saturating_sub(d)
    )
}

pub mod clipboard;
mod defines;
mod dlopen;
mod events;
mod gl;
pub mod menu;
mod tooltip;

use std::{
    cell::RefCell,
    ffi::{c_void, CString},
    rc::Rc,
    sync::{Arc, Condvar, Mutex},
};

use azul_core::{
    callbacks::LayoutCallbackInfo,
    dom::DomId,
    events::{MouseButton, ProcessEventResult},
    geom::LogicalPosition,
    gl::{GlContextPtr, OptionGlContextPtr},
    hit_test::{DocumentId, FullHitTest},
    refany::RefAny,
    resources::{AppConfig, Au, DpiScaleFactor, IdNamespace, ImageCache, RendererResources},
    window::{
        CursorPosition, HwAcceleration, KeyboardState, Monitor, MouseCursorType, MouseState,
        RawWindowHandle, RendererType, WaylandHandle, WindowDecorations,
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
    dlopen::{Library, Wayland, Xkb},
};
use super::{
    common::{compose::ComposeAction, gl::GlFunctions},
    x11::{accessibility::LinuxAccessibilityAdapter, dlopen::Gtk3Im},
};
use crate::desktop::shell2::common::debug_server::LogCategory;
use crate::desktop::{
    shell2::common::{
        event::{
            self, HitTestNode, PlatformWindow, BUTTON_STATE_LEFT, BUTTON_STATE_MIDDLE,
            BUTTON_STATE_NONE, BUTTON_STATE_RIGHT,
        },
        WindowError,
    },
    wr_translate2::{self, AsyncHitTester, Notifier, WrRenderApi},
};
use crate::{log_debug, log_error, log_info, log_trace, log_warn};

/// Tracks the current rendering mode of the window.
enum RenderMode {
    Gpu(gl::GlContext, GlFunctions),
    /// CPU fallback - initialized lazily after receiving wl_shm from registry
    Cpu(Option<CpuFallbackState>),
}

/// State for CPU fallback rendering.
/// One of the two shm buffers backing a surface (double buffering).
struct ShmSlot {
    buffer: *mut defines::wl_buffer,
    /// Byte offset of this slot inside the shared pool.
    offset: usize,
    /// Heap flag flipped to `false` by the `wl_buffer.release` listener.
    /// While `true` the compositor may still be reading the buffer — writing
    /// to it is a protocol violation (visible as tearing).
    busy: *mut bool,
    /// Buffer-px regions updated in the OTHER slot since this slot was last
    /// presented — they must be copied forward before a partial update here.
    stale: Vec<(i32, i32, i32, i32)>,
    /// Too many stale rects accumulated → full copy on next use.
    stale_overflow: bool,
    /// This slot has held at least one COMPLETE frame (full render or full
    /// cross-slot copy). Until then partial catch-up is meaningless — the
    /// slot's other pixels are undefined (#27).
    valid: bool,
}

/// Set to `true` by the `wl_shm.format` listener when the compositor
/// advertises ABGR8888 (bytes R,G,B,A in LE memory = the CPU renderer's
/// output order). Compositor-global, hence process-global (#27).
pub(crate) static SHM_ABGR8888_ADVERTISED: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

// #27 master switch (shared across platform shells — see its doc there).
use crate::desktop::shell2::headless::native_backbuffer_enabled;

struct CpuFallbackState {
    wayland: Rc<Wayland>,
    pool: *mut defines::wl_shm_pool,
    /// Two buffers, alternated so the client never writes into a buffer the
    /// compositor still holds (the old single-buffer path violated the
    /// protocol on every frame after the first).
    slots: [ShmSlot; 2],
    /// Slot to draw into / attach next.
    active: usize,
    data: *mut u8,
    pool_size: usize,
    /// Buffer dimensions in PHYSICAL px (logical × scale).
    width: i32,
    height: i32,
    stride: i32,
    /// Integer buffer scale (`wl_surface.set_buffer_scale`); 1 on non-HiDPI.
    scale: i32,
    /// Pixel format the pool's buffers were created with:
    /// `WL_SHM_FORMAT_ABGR8888` (renderer byte order — presents copy rows
    /// verbatim and the renderer may target a slot DIRECTLY, #27) or the
    /// mandatory `WL_SHM_FORMAT_ARGB8888` (needs the R↔B swizzle at every
    /// copy). Fixed for the pool's lifetime.
    format: u32,
    fd: i32, // Keep fd open until drop
    /// Damage rects (x, y, w, h) of the last render pass, in BUFFER (physical)
    /// coordinates. Filled by the CPU present path from
    /// `CpuBackend::last_present_damage`; drained into per-rect
    /// `wl_surface_damage_buffer` (or scale-divided `wl_surface_damage`) at
    /// commit. Empty = nothing changed on screen.
    damage_rects: Vec<(i32, i32, i32, i32)>,
}

/// `wl_buffer.release`: compositor is done with the buffer — mark reusable.
/// `data` is the slot's heap `busy` flag; events are dispatched on the
/// window's own thread, so a plain bool is race-free.
extern "C" fn wl_buffer_release_handler(data: *mut c_void, _buffer: *mut defines::wl_buffer) {
    if !data.is_null() {
        unsafe {
            *(data as *mut bool) = false;
        }
    }
}

static WL_BUFFER_RELEASE_LISTENER: defines::wl_buffer_listener = defines::wl_buffer_listener {
    release: wl_buffer_release_handler,
};

/// Monitor state tracking for multi-monitor support
#[derive(Debug, Clone)]
pub struct MonitorState {
    pub proxy: *mut defines::wl_output,
    /// The `wl_registry` global id this output was advertised under. This is
    /// the ONLY handle `wl_registry.global_remove` gives us, so without it a
    /// monitor unplug cannot be matched to the entry it should drop.
    pub global_name: u32,
    pub name: String,
    pub scale: i32,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub make: String,  // Manufacturer (from wl_output.geometry)
    pub model: String, // Model (from wl_output.geometry)
}

impl MonitorState {
    /// Generate a stable MonitorId from this monitor's properties
    pub fn get_monitor_id(&self, index: usize) -> azul_core::window::MonitorId {
        use azul_css::props::basic::{LayoutPoint, LayoutSize};

        // Use make + model + name for more stable hash
        // This handles cases where position changes but physical monitor doesn't
        let stable_name = if !self.make.is_empty() && !self.model.is_empty() {
            format!("{}-{}-{}", self.make, self.model, self.name)
        } else {
            self.name.clone()
        };

        azul_core::window::MonitorId::from_properties(
            index,
            &stable_name,
            LayoutPoint::new(self.x as isize, self.y as isize),
            LayoutSize::new(self.width as isize, self.height as isize),
        )
    }
}

/// How long to wait for `wl_surface.frame`'s `done` before assuming it is never
/// coming and rendering anyway.
///
/// Generous on purpose: this is a deadlock escape, not a frame-pacing knob. At
/// 60 Hz a healthy compositor answers in ~16 ms, so half a second is ~30 missed
/// frames — long enough that it cannot fire on a merely slow machine, short
/// enough that a user who un-minimises a window does not sit looking at a frozen
/// one.
const FRAME_CALLBACK_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(500);

pub struct WaylandWindow {
    wayland: Rc<Wayland>,
    xkb: Rc<Xkb>,
    gtk_im: Option<Rc<Gtk3Im>>, // Optional GTK IM context for IME (fallback)
    gtk_im_context: Option<*mut dlopen::GtkIMContext>, // GTK IM context instance (fallback)
    text_input_manager: Option<*mut defines::zwp_text_input_manager_v3>, /* Wayland text-input
                                 * v3 manager */
    text_input: Option<*mut defines::zwp_text_input_v3>, // Wayland text-input v3 instance
    text_input_active: bool, // Whether compositor has activated text input for our surface
    text_input_enabled: bool, // Whether we've called enable() for current focus
    text_input_pending: events::TextInputPendingState, // Pending IME state between events
    pub display: *mut defines::wl_display,
    registry: *mut defines::wl_registry,
    compositor: *mut defines::wl_compositor,
    shm: *mut defines::wl_shm,
    seat: *mut defines::wl_seat,
    xdg_wm_base: *mut defines::xdg_wm_base,
    pub(crate) surface: *mut defines::wl_surface,
    xdg_surface: *mut defines::xdg_surface,
    xdg_toplevel: *mut defines::xdg_toplevel,
    event_queue: *mut defines::wl_event_queue,
    keyboard_state: events::WaylandKeyboardState,
    pointer_state: events::PointerState,
    // wl_keyboard / wl_touch proxies (created in seat_capabilities_handler). Stored so
    // rebind_listeners() can re-point their listener user-data to the stable boxed `self`.
    keyboard: *mut defines::wl_keyboard,
    touch: *mut defines::wl_touch,
    tablet_manager: *mut defines::zwp_tablet_manager_v2,
    tablet_seat: *mut defines::zwp_tablet_seat_v2,
    tablet_initialized: bool,
    // wl_data_device family (file drag-and-drop DESTINATION). Bound from the
    // registry; the data_device is created once both manager + seat are ready
    // (see events::try_init_data_device). `drag` holds the live transfer state.
    data_device_manager: *mut defines::wl_data_device_manager,
    data_device: *mut defines::wl_data_device,
    /// MWA-B3: current clipboard selection offer from the compositor
    /// (null = no selection). Set by events::data_device_selection.
    clipboard_offer: *mut defines::wl_data_offer,
    /// MWA-B3: our live outgoing clipboard source (null when another client
    /// owns the selection). Destroyed and replaced on every copy.
    clipboard_source: *mut defines::wl_data_source,
    /// MWA-B3: most recent input serial (pointer button OR key press) —
    /// wl_data_device.set_selection requires a real input serial.
    last_input_serial: u32,
    // zwp_primary_selection_v1 — the Wayland select-to-copy / middle-click
    // paste idiom. Null when the compositor does not implement the protocol,
    // in which case both halves simply stay inert.
    primary_selection_manager: *mut defines::zwp_primary_selection_device_manager_v1,
    primary_selection_device: *mut defines::zwp_primary_selection_device_v1,
    /// The offer the compositor last named as the primary selection
    /// (null = no selection).
    primary_selection_offer: *mut defines::zwp_primary_selection_offer_v1,
    /// Our live outgoing primary-selection source (null when another client
    /// owns it). Destroyed and replaced on every selection.
    primary_selection_source: *mut defines::zwp_primary_selection_source_v1,
    primary_selection_initialized: bool,
    /// MWA-C-scroll: wl_pointer.axis_source of the current pointer frame
    /// (0=wheel, 1=finger, 2=continuous, 3=wheel_tilt). Finger/continuous
    /// axis events classify as TrackpadContinuous so the physics timer
    /// applies deltas directly (OS/compositor momentum) instead of wheel
    /// impulses; axis_stop then emits TrackpadEnd for rubber-band
    /// spring-back.
    ///
    /// Reset to `wheel` by `handle_pointer_frame`. The protocol scopes
    /// axis_source to ONE frame ("carries the source information for all events
    /// within that frame"); when it was sticky instead, the first wheel tick
    /// after any trackpad scroll inherited TrackpadContinuous and scrolled with
    /// touchpad physics.
    current_axis_source: u32,
    /// Axis deltas accumulated within the current wl_pointer frame, in the
    /// engine's raw-delta convention (sign already flipped).
    ///
    /// A wl_pointer frame is the atomic unit: a diagonal trackpad scroll carries
    /// X and Y in ONE frame and must produce ONE scroll, not two full event
    /// passes with two synthetic Scroll dispatches.
    pending_axis_value: (f32, f32),
    /// `wl_pointer.axis_discrete` detents accumulated in the current frame, same
    /// sign convention as `pending_axis_value`. Non-zero only for ratcheting
    /// wheels; it is what lets the flush convert to the shared 20px-per-detent
    /// magnitude instead of the compositor's raw ~10-15px axis value.
    pending_axis_discrete: (f32, f32),
    /// Whether the current frame carries any axis input to flush.
    pending_axis: bool,
    /// Bound `wl_seat` version. `wl_pointer.frame` and `.axis_discrete` are v5+;
    /// on an older seat no frame event ever arrives, so the axis accumulator has
    /// to flush itself per event instead of waiting forever.
    seat_version: u32,
    data_device_version: u32,
    data_device_initialized: bool,
    drag: events::WaylandDragState,
    tablet_pen: events::TabletPenPending,
    tablet_pad: events::TabletPadPending,
    /// One-shot descriptive data per `zwp_tablet_tool_v2` (type / hardware
    /// serial), keyed by tool proxy address; applied to `tablet_pen` at
    /// `proximity_in`. See `events::TabletToolStatic` for why per-tool.
    tablet_tools: std::collections::HashMap<usize, events::TabletToolStatic>,
    /// Identity (name + USB ids) of the announced `zwp_tablet_v2` device.
    tablet_info: events::TabletStatic,
    /// Announced `zwp_tablet_pad_v2` (button count, ring/strip); `None` = no pad.
    tablet_pad_static: Option<events::TabletPadStatic>,
    // False until the first poll rebinds all proxy listeners to the stable boxed `self`.
    listeners_rebound: bool,
    /// EVERY proxy whose listener dereferences its user-data as `*mut WaylandWindow`,
    /// recorded by [`Self::track_listener`] AT THE MOMENT OF REGISTRATION.
    ///
    /// This is what makes `rebind_listeners` correct BY CONSTRUCTION. It used to
    /// re-point a hand-written array of named fields, which silently missed any
    /// proxy that had no field of its own or that a maintainer forgot to add —
    /// `xdg_wm_base` (bound inside `registry_global_handler`, so it has no
    /// registration site in `new()`) and every `wl_output` were both missing, and
    /// the first `xdg_wm_base.ping` the compositor sent after the window was boxed
    /// dereferenced the dead `new()` stack frame and SIGSEGV'd the process
    /// (`events.rs:xdg_wm_base_ping_handler`, `mov 0x180(%rax)` with `rax` read out
    /// of reclaimed stack). Registration now records; nothing can be forgotten.
    listener_proxies: Vec<*mut defines::wl_proxy>,
    is_open: bool,
    configured: bool,
    /// Set by `xdg_toplevel_configure_handler` when the configure batch being
    /// processed carries a SIZE change; read + reset by
    /// `xdg_surface_configure_handler` (which arrives at the end of that same
    /// batch) to decide how the mandatory commit-after-ack happens. A size
    /// change's commit comes from the real present with the new-size buffer; a
    /// state-only / repeated configure gets an immediate EMPTY commit — see
    /// the ack handler for why skipping that commit stalls the compositor.
    configure_size_changed: bool,

    // Wayland protocols
    subcompositor: Option<*mut defines::wl_subcompositor>, // For tooltips

    // KDE blur protocol (org.kde.kwin.blur)
    blur_manager: Option<*mut defines::org_kde_kwin_blur_manager>,
    current_blur: Option<*mut defines::org_kde_kwin_blur>,

    // xdg-decoration-unstable-v1 (server-side titlebar). Bound from the registry;
    // the per-toplevel decoration is created after the xdg_toplevel and asked for
    // server-side mode so the compositor draws move/close decorations.
    decoration_manager: Option<*mut defines::zxdg_decoration_manager_v1>,
    toplevel_decoration: Option<*mut defines::zxdg_toplevel_decoration_v1>,

    // wp-fractional-scale-v1 + wp-viewporter (fractional HiDPI). When the
    // compositor advertises both, `preferred_scale` (scale×120) drives
    // size.dpi, buffers are allocated at physical size WITHOUT
    // set_buffer_scale (must stay 1) and the viewport maps them to the
    // logical surface size via set_destination. When either protocol is
    // missing, the integer wl_output scale path below is used unchanged.
    fractional_scale_manager: Option<*mut defines::wp_fractional_scale_manager_v1>,
    viewporter: Option<*mut defines::wp_viewporter>,
    /// Per-surface wp_fractional_scale_v1 (delivers preferred_scale events).
    fractional_scale: Option<*mut defines::wp_fractional_scale_v1>,
    /// Per-surface wp_viewport for the main surface.
    viewport: Option<*mut defines::wp_viewport>,
    /// Last compositor-preferred scale ×120 (None until the first
    /// preferred_scale event = integer path active). Full precision lives
    /// here; size.dpi holds the rounded ×96 value.
    pub(crate) preferred_scale_120: Option<u32>,

    // Tooltip
    tooltip: Option<tooltip::TooltipWindow>,

    // Power management (D-Bus)
    screensaver_inhibit_cookie: Option<u32>,
    dbus_connection: Option<*mut super::dbus::DBusConnection>,

    // Shell2 state (common fields shared with all platforms)
    pub common: event::CommonWindowState,
    new_frame_ready: Arc<(Mutex<bool>, Condvar)>,

    render_mode: RenderMode,

    /// GPU damage rects from the last layout pass. Used to call
    /// wl_surface_damage per-rect instead of full surface in GPU mode,
    /// so the Wayland compositor can skip recompositing unchanged regions.
    gpu_damage_rects: Vec<azul_core::geom::LogicalRect>,

    /// Whether the last GPU render was actually presented (swapped). The
    /// Wayland GPU path skips the swap for 0-draw-call frames; WebRender's
    /// internal buffer-damage tracker still records such frames, so its
    /// frame counter and EGL's buffer-age counter drift apart. After a
    /// skipped present the next frame passes buffer_age=0 (= full render),
    /// resynchronizing conservatively.
    gpu_last_render_presented: bool,

    /// Shared CPU rendering backend (same as the headless + X11 paths): owns the
    /// retained pixmap, compositor, glyph cache, display-list damage diff AND the
    /// scroll-shift / eligibility / present-split machinery. Replaces the former
    /// per-backend glyph_cache / retained_pixmap / previous_display_list fields.
    #[cfg(feature = "cpurender")]
    cpu_backend: crate::desktop::shell2::headless::CpuBackend,

    /// The shm buffer's on-screen content is stale/undefined (first frame,
    /// buffer recreated on resize) — the next CPU present must copy + damage
    /// the FULL frame even if `render_frame` reports no damage. Consumed by
    /// the CPU present path in `generate_frame_if_needed`.
    os_present_requested: bool,

    /// Client-side key repeat. Wayland compositors do NOT repeat keys for
    /// clients (`wl_keyboard` delivers exactly one pressed/released pair) —
    /// without this timer, holding Backspace deletes ONE character.
    /// Interval in ms between repeats (0 = repeat disabled by compositor).
    key_repeat_rate_ms: u32,
    /// Delay in ms before the first repeat.
    key_repeat_delay_ms: u32,
    /// Dedicated timerfd driving the repeat (polled in wait_for_events).
    key_repeat_fd: i32,
    /// The evdev keycode currently held (armed for repeat).
    key_repeat_keycode: Option<u32>,

    /// evdev keycode → the `VirtualKeyCode` its PRESS put into
    /// `pressed_virtual_keycodes`.
    ///
    /// `xkb_state_key_get_one_sym` returns the keysym for the modifier state at
    /// that instant, so the press and the release of one physical key can resolve
    /// to different keysyms (release Shift/AltGr before the key and they always
    /// do). Translating the release keysym independently therefore removes the
    /// wrong code — or none — and leaves the pressed key latched forever, which
    /// the engine reads as a modifier that never came up. Releases consult this
    /// map instead. Only mapped keys get an entry; unmapped keysyms add nothing
    /// to either structure and so need nothing removed.
    pressed_key_vks: std::collections::BTreeMap<u32, azul_core::window::VirtualKeyCode>,

    // Monitor tracking for multi-monitor support
    pub known_outputs: Vec<MonitorState>,
    pub current_outputs: Vec<*mut defines::wl_output>,

    // V2 Event system state
    pub frame_callback_pending: bool, // Wayland frame callback synchronization
    /// When `frame_callback_pending` was armed. The latch has no other escape:
    /// it is cleared ONLY by `frame_done_callback`, and a compositor is entitled
    /// to never send `done` — wayland.xml says a server "should avoid signaling
    /// the frame callbacks if the surface is not visible in any way, e.g. the
    /// surface is off-screen, or completely obscured by other opaque surfaces",
    /// and Weston implements that literally. Without an expiry, minimising or
    /// occluding a window froze it permanently: Wayland has no Expose, and
    /// poll_event never calls generate_frame_if_needed, so nothing would ever
    /// retry. See FRAME_CALLBACK_TIMEOUT.
    pub frame_callback_armed_at: Option<std::time::Instant>,
    /// Raised when a visual update is needed but no layout regeneration is
    /// required. This happens when scroll offsets change (timer callbacks) or
    /// GPU values are updated. The next `generate_frame_if_needed()` sends a
    /// lightweight transaction.
    ///
    /// A latched request, not a bare bool, and PRIVATE to this module. It is
    /// raised from INSIDE `generate_frame_if_needed` — the CPU
    /// both-buffers-held retry, and the scrollbar-fade re-arm — so retiring it
    /// with a bare `= false` at the end of that function would eat exactly the
    /// frames those paths just asked for. Retire by epoch instead.
    needs_redraw: super::super::common::event::LatchedRequest,

    // Native timer support via timerfd (Linux-specific)
    // Maps TimerId -> (timerfd file descriptor)
    pub timer_fds: std::collections::BTreeMap<usize, i32>,

    // Accessibility
    #[cfg(feature = "a11y")]
    pub accessibility_adapter: LinuxAccessibilityAdapter,

    // Multi-window support
    /// Pending window creation requests (for popup menus, dialogs, etc.)
    /// Processed in Phase 3 of the event loop
    pub pending_window_creates: Vec<WindowCreateOptions>,

    /// Active menu popup, if any (xdg_popup nested under this parent surface).
    /// Wayland clients cannot position their own toplevels, so menus are
    /// anchored to the trigger rect on the parent via xdg_positioner and grab
    /// the seat for click-outside dismiss. Driven by `drive_active_popup()`.
    pub active_popup: Option<Box<WaylandPopup>>,

    /// Whether the most recent `wl_pointer.enter` targeted the active popup's
    /// surface (rather than this parent surface). The xdg_popup grab routes all
    /// pointer events through this parent's seat listeners, so we use this flag
    /// — set from the surface carried by `enter` — to forward subsequent
    /// motion/button events (which carry no surface) to the popup's layout.
    pub pointer_over_popup: bool,

    // GNOME native menu V2 with dlopen
    pub gnome_menu: Option<super::gnome_menu::GnomeMenuManager>,

    // Shared resources
    pub resources: Arc<super::AppResources>,
    /// Dynamic selector context for evaluating conditional CSS properties
    /// (viewport size, OS, theme, etc.) - updated on resize and theme change
    pub dynamic_selector_context: azul_css::dynamic_selector::DynamicSelectorContext,
}

#[derive(Debug, Clone, Copy)]
pub enum WaylandEvent {
    Redraw,
    Close,
    Other,
}

// Wayland Popup Window (for menus using xdg_popup)

/// Wayland popup window using xdg_popup protocol
///
/// This is used for menus and other transient popup surfaces. Unlike WaylandWindow
/// which uses xdg_toplevel, this uses xdg_popup which provides:
/// - Parent-relative positioning
/// - Compositor-managed stacking
/// - Automatic grab support
/// - Automatic dismissal on outside clicks
pub struct WaylandPopup {
    wayland: Rc<Wayland>,
    xkb: Rc<Xkb>,
    display: *mut defines::wl_display,
    parent_surface: *mut defines::wl_surface,
    surface: *mut defines::wl_surface,
    xdg_surface: *mut defines::xdg_surface,
    xdg_popup: *mut defines::xdg_popup,
    positioner: *mut defines::xdg_positioner,
    compositor: *mut defines::wl_compositor,
    seat: *mut defines::wl_seat,
    event_queue: *mut defines::wl_event_queue,
    keyboard_state: events::WaylandKeyboardState,
    pointer_state: events::PointerState,
    is_open: bool,
    configured: bool,

    // Listener context - must be freed on drop
    listener_context: *mut PopupListenerContext,

    /// The window state every `PlatformWindow` shares: layout window, current /
    /// previous window state, hit testers, regeneration flags. The popup IS a
    /// `PlatformWindow` — it runs the same event pipeline as a toplevel
    /// (hover, drag, pointer capture, text input, lifecycle events, the
    /// transient-window dismiss hooks), fed by the parent's seat listeners.
    pub common: event::CommonWindowState,
    /// Windows a callback inside the popup asked to create (a submenu, a
    /// nested popup); the parent drains these into its own queue.
    pub pending_window_creates: Vec<WindowCreateOptions>,
    /// Scancode → the VirtualKeyCode it mapped to on press, so the release
    /// removes the right one (mirrors `WaylandWindow::pressed_key_vks`).
    pressed_key_vks: std::collections::BTreeMap<u32, azul_core::window::VirtualKeyCode>,
    render_mode: RenderMode,

    // V2 Event system state
    pub frame_callback_pending: bool,

    // Shared resources
    pub resources: Arc<super::AppResources>,

    /// wl_shm handle (borrowed from the parent) for lazily creating the CPU buffer.
    shm: *mut defines::wl_shm,

    /// Snapshot of the parent window's `ImageCache` id map, taken at popup
    /// creation. The popup builds its own `LayoutWindow` lazily; without this
    /// seed, `url("...")` / css-id images in popup menus resolve to nothing
    /// (the popup's cache starts empty and nothing ever fills it).
    /// wp_viewporter (borrowed from the parent) + the parent's preferred
    /// fractional scale ×120. When both are present the popup buffer is
    /// allocated at the exact physical size and mapped to logical via a
    /// wp_viewport (buffer scale stays 1) instead of the integer
    /// set_buffer_scale path.
    viewporter: Option<*mut defines::wp_viewporter>,
    preferred_scale_120: Option<u32>,
    /// The popup surface's own wp_viewport (created lazily in
    /// `render_if_ready`, destroyed in `close`).
    viewport: Option<*mut defines::wp_viewport>,
    /// Whether the menu DOM has already been rendered into the buffer.
    rendered: bool,
    /// Set when the popup's content changed and it must paint again.
    ///
    /// `rendered` alone was a ONE-SHOT latch: `render_if_ready` returned early
    /// forever once the first frame was drawn, so a popup could never repaint.
    /// That is not academic — popups DO receive pointer input (see
    /// `pointer_over_popup`, and the hover resolve that maps popup-surface
    /// coordinates to a menu-item node), so the hover highlight was computed on
    /// every motion event and then never drawn. Same for a selected state, a
    /// scroll inside the popup, or a submenu opening.
    needs_repaint: bool,
    /// Shared CPU rendering backend (the menu is painted via the headless CPU
    /// path, same as the X11/Wayland CPU fallback — popups never use WebRender).
    #[cfg(feature = "cpurender")]
    cpu_backend: crate::desktop::shell2::headless::CpuBackend,
}

// Event Handler Types

/// `wl_pointer.axis` axis values.
const WL_POINTER_AXIS_VERTICAL_SCROLL: u32 = 0;
const WL_POINTER_AXIS_HORIZONTAL_SCROLL: u32 = 1;

/// `wl_pointer.axis_source` values.
const WL_AXIS_SOURCE_WHEEL: u32 = 0;
const WL_AXIS_SOURCE_FINGER: u32 = 1;
const WL_AXIS_SOURCE_CONTINUOUS: u32 = 2;

/// Pixels per discrete wheel detent — the shared cross-backend constant.
const WHEEL_TICK_PIXELS: f32 = crate::desktop::shell2::common::event::WHEEL_SCROLL_PIXELS_PER_LINE;

/// `wl_pointer.frame` and `.axis_discrete` were both added in wl_seat v5.
const WL_POINTER_FRAME_SINCE_VERSION: u32 = 5;

/// Write the down-flag of exactly ONE mouse button, leaving the others untouched.
///
/// Every button transition — press and release alike — must go through here.
/// Broadcasting `button == X` across all three flags is what turned "press Right
/// while Left is held" into a phantom LeftMouseUp in the state diff.
fn set_mouse_button_down(
    mouse_state: &mut azul_core::window::MouseState,
    button: MouseButton,
    down: bool,
) {
    match button {
        MouseButton::Left => mouse_state.left_down = down,
        MouseButton::Right => mouse_state.right_down = down,
        MouseButton::Middle => mouse_state.middle_down = down,
        MouseButton::Other(_) => {}
    }
}

/// Apply ONE `wl_keyboard.key` transition to the engine's keyboard state.
///
/// `current_virtual_keycode` MUST be cleared on release: the shared diff
/// derives VirtualKeyUp from `previous.is_some() && current.is_none()`, and a
/// leftover `Some(vk)` also swallows the next discrete press of the SAME key
/// (no `Some → Some` delta) — the "Backspace only works every other tap"
/// report.
///
/// An unmapped keysym (`virtual_keycode == None`) invents no code: nothing is
/// written to `current_virtual_keycode`, nothing is added to
/// `pressed_virtual_keycodes`, and so the release has nothing to fail to
/// remove. The scancode list is the PHYSICAL key list and is written either
/// way — `handle_key`'s repeat detection reads it.
///
/// The release removes the code the PRESS recorded for this physical key
/// (`pressed_key_vks`), never whatever the release keysym resolves to:
/// `xkb_state_key_get_one_sym` reports the EFFECTIVE keysym, so German AltGr+Q
/// is `XK_at` (→ Key2) on the way down and `XK_q` (→ Q) on the way up once
/// AltGr is released first.
fn apply_key_state_change(
    keyboard_state: &mut KeyboardState,
    pressed_key_vks: &mut std::collections::BTreeMap<u32, azul_core::window::VirtualKeyCode>,
    key: u32,
    virtual_keycode: Option<azul_core::window::VirtualKeyCode>,
    is_pressed: bool,
) {
    use azul_core::window::OptionVirtualKeyCode;

    if is_pressed {
        keyboard_state.current_virtual_keycode = virtual_keycode.into();
        if let Some(vk) = virtual_keycode {
            keyboard_state.pressed_virtual_keycodes.insert_hm_item(vk);
            pressed_key_vks.insert(key, vk);
        }
        keyboard_state.pressed_scancodes.insert_hm_item(key);
    } else {
        keyboard_state.current_virtual_keycode = OptionVirtualKeyCode::None;
        // `.or(virtual_keycode)` only covers a key whose press we never saw
        // (held across focus-in before the keymap arrived); the recorded code
        // wins whenever we have one.
        if let Some(vk) = pressed_key_vks.remove(&key).or(virtual_keycode) {
            keyboard_state.pressed_virtual_keycodes.remove_hm_item(&vk);
        }
        keyboard_state.pressed_scancodes.remove_hm_item(&key);
    }
}

/// Does this `wl_pointer.axis_source` describe a finger on a surface rather
/// than a ratcheting wheel?
///
/// Finger and continuous sources deliver POSITION deltas; treating them as
/// wheel ticks stacked velocity impulses and made touchpad scrolling fly.
/// Does this pointer-button event ask for a middle-click paste?
///
/// The RELEASE, not the press — the press is what moved the caret to the click
/// point, and pasting on it would insert at wherever the caret used to be.
/// Only when something editable has focus: the read waits on the selection
/// OWNER, so a middle click anywhere else must not pay for it (and
/// `record_text_input` would drop the text anyway).
pub(super) fn primary_paste_wanted(
    button: MouseButton,
    is_down: bool,
    has_active_editing: bool,
) -> bool {
    button == MouseButton::Middle && !is_down && has_active_editing
}

fn axis_source_is_trackpad(source: u32) -> bool {
    source == WL_AXIS_SOURCE_FINGER || source == WL_AXIS_SOURCE_CONTINUOUS
}

/// The scroll distance of ONE completed `wl_pointer.frame`.
///
/// A ratcheting wheel detent is worth [`WHEEL_TICK_PIXELS`] on every other
/// backend (X11 button 4/5 = ±1 × 20, Win32 = WHEEL_DELTA/120 × 20). The raw
/// `wl_pointer.axis` value for one detent is compositor-defined (~10-15 px), so
/// the same wheel scrolled a visibly shorter distance on Wayland.
/// `wl_pointer.axis_discrete` carries the detent count, which is the only
/// compositor-independent quantity available here; without it (a pre-v5
/// compositor, or a continuous source) fall back to the raw value.
///
/// Trackpad deltas are already pixel distances and pass through untouched.
fn axis_frame_delta(is_trackpad: bool, raw: (f32, f32), discrete: (f32, f32)) -> (f32, f32) {
    if !is_trackpad && (discrete.0 != 0.0 || discrete.1 != 0.0) {
        (
            discrete.0 * WHEEL_TICK_PIXELS,
            discrete.1 * WHEEL_TICK_PIXELS,
        )
    } else {
        raw
    }
}

// XKB Keyboard Translation
//
// There is NO Wayland-specific keysym table. Keysyms are an X11/xkb concept
// that both backends receive verbatim, so the single maintained mapping lives
// in `x11::events::keysym_to_virtual_keycode` and this backend reaches it
// through `events::keysym_to_virtual_keycode` — the one entry point. The
// hand-rolled table that used to live here returned a bare VirtualKeyCode with
// `_ => VirtualKeyCode::Escape` as its catch-all, so every key it did not know
// (Shift+digit, F13+, the whole keypad, every non-Latin letter) pressed AND
// released Escape: menus closed and Escape default-actions fired on innocent
// keystrokes. The shared table returns `Option` instead — an unmapped keysym
// is *no* virtual key, never a wrong one.

// Lifecycle methods (formerly on PlatformWindow V1 trait)

impl WaylandWindow {
    pub fn poll_event(&mut self) -> Option<WaylandEvent> {
        // First pump after the run loop boxed us: re-point all listeners to this stable
        // address (they were registered against the now-moved `new()` stack frame).
        self.ensure_listeners_rebound();

        // Check timers and threads before processing Wayland events
        self.check_timers_and_threads();

        // Drain the Wayland socket non-blockingly. The old code only called
        // wl_display_dispatch_queue_pending, which dispatches events ALREADY queued but
        // never READS the fd -- so the socket was only ever drained as a side effect of
        // eglSwapBuffers. An idle window (not rendering) therefore processed no events at
        // all, including xdg_toplevel.close, so it couldn't be closed from the taskbar
        // and ignored input until something forced a redraw. Use libwayland's canonical
        // race-free non-blocking read: prepare_read (retrying after draining if the queue
        // isn't empty), flush our requests, poll the fd with timeout 0, then read_events
        // if readable or cancel_read if not, and finally dispatch what we read.
        let mut hung_up = false;
        let dispatched = unsafe {
            while (self.wayland.wl_display_prepare_read_queue)(self.display, self.event_queue) != 0
            {
                // Queue not empty -> dispatch what's already there, then retry prepare.
                (self.wayland.wl_display_dispatch_queue_pending)(self.display, self.event_queue);
            }

            (self.wayland.wl_display_flush)(self.display);

            let fd = (self.wayland.wl_display_get_fd)(self.display);
            let mut pfd = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            let polled = libc::poll(&mut pfd, 1, 0);
            // POLLHUP/POLLERR are reported regardless of the events mask:
            // the compositor closed the socket.
            hung_up = polled > 0 && (pfd.revents & (libc::POLLHUP | libc::POLLERR)) != 0;
            let readable = polled > 0 && (pfd.revents & libc::POLLIN) != 0;

            if readable {
                (self.wayland.wl_display_read_events)(self.display);
            } else {
                (self.wayland.wl_display_cancel_read)(self.display);
            }

            (self.wayland.wl_display_dispatch_queue_pending)(self.display, self.event_queue)
        };

        // A DEAD CONNECTION MUST END THE WINDOW, not spin.
        //
        // libwayland latches a fatal error on the display (compositor gone,
        // protocol error, socket hangup); from then on every call fails and
        // `dispatch_queue_pending` just keeps returning -1. Without this
        // check the loop treated "nothing dispatched" as "idle" and ran
        // forever with no window on screen — the process outlived its own
        // surface as an orphan, invisible and unkillable from the taskbar.
        // (Observed when a slow frame made the client miss the
        // configure/ping handshake and KWin dropped the surface.)
        let display_error = unsafe { (self.wayland.wl_display_get_error)(self.display) };
        if hung_up || display_error != 0 || dispatched < 0 {
            // UNCONDITIONAL, and deliberately not a log macro.
            //
            // This detector was correct and silent on 2026-08-07: `log_warn!` in
            // a `debug-server` build routes into the debug server's in-memory
            // queue and never reaches stderr, so a compositor disconnect
            // produced libwayland's bare `Error sending request: Broken pipe`
            // and not one word from azul. Losing the display is fatal to the
            // window, so it prints no matter how logging is configured.
            let fatal = format!(
                "[WL] CONNECTION LOST — closing the window. hup={hung_up} \
                 errno={display_error} ({}) dispatched={dispatched}{} — \
                 configures={} {}",
                errno_name(display_error),
                self.describe_protocol_error(display_error),
                CONFIGURES_SEEN.load(core::sync::atomic::Ordering::Relaxed),
                pool_census(),
            );
            eprintln!("{fatal}");
            // Also into the log FILE. A fatal line that exists only on a
            // terminal is lost the moment the terminal scrolls, which is
            // exactly how the 2026-08-07 disconnect went undiagnosed.
            crate::desktop::shell2::common::log_gate::emit(
                crate::desktop::shell2::common::debug_server::LogLevel::Error,
                LogCategory::EventLoop,
                fatal,
            );
            log_warn!(
                LogCategory::EventLoop,
                "[Wayland] connection lost (hup={hung_up}, error={display_error}, \
                 dispatched={dispatched}){} - closing the window",
                self.describe_protocol_error(display_error),
            );
            self.is_open = false;
            // NONE, not Some(Close): the run loop drains with
            // `while poll_event().is_some()`, so a Close here would spin as
            // hard as the bug it fixes. Ending the drain lets the loop's
            // `!window.is_open()` pass unregister and drop the window.
            return None;
        }

        // Service any open menu popup: dispatching above may have delivered its
        // configure (so we can render+attach a buffer) or popup_done (dismiss).
        self.drive_active_popup();

        if dispatched > 0 {
            Some(WaylandEvent::Redraw) // Events were processed, a redraw might be needed.
        } else {
            None
        }
    }

    /// Name the object that killed the connection, for the "connection lost"
    /// log line.
    ///
    /// See also [`errno_name`]: `wl_display_get_error` returns a bare errno and
    /// the two that matter read very differently. EPIPE means WE wrote to a
    /// socket the compositor had already closed — the violation happened
    /// earlier and this is only the aftermath. EPROTO means libwayland itself
    /// rejected something, and then the protocol-error detail below names it.
    ///
    /// `wl_display_get_error` returns a bare errno, and EPROTO (71) covers
    /// every protocol violation there is — which is how a `wl_data_offer`
    /// id collision presented as an unexplained vanishing window. libwayland
    /// keeps the details, so ask for them:
    /// `wl_display_get_protocol_error` returns the interface-defined error
    /// CODE and fills in the interface and the offending object id.
    ///
    /// Empty string for anything that is not a protocol error (a hangup, a
    /// dead socket), so the caller can append it unconditionally.
    fn describe_protocol_error(&self, display_error: i32) -> String {
        if display_error != libc::EPROTO {
            return String::new();
        }
        let mut interface: *const defines::wl_interface = std::ptr::null();
        let mut id: u32 = 0;
        let code = unsafe {
            (self.wayland.wl_display_get_protocol_error)(self.display, &mut interface, &mut id)
        };
        let name = if interface.is_null() {
            "<unknown interface>".to_string()
        } else {
            unsafe {
                let n = (*interface).name;
                if n.is_null() {
                    "<unnamed>".to_string()
                } else {
                    std::ffi::CStr::from_ptr(n).to_string_lossy().into_owned()
                }
            }
        };
        format!(" [protocol error: {name}@{id} code {code}]")
    }

    // NOTE: `WaylandWindow::present` is deliberately GONE (with its sole
    // caller `LinuxWindow::present`). The real present path is
    // `generate_frame_if_needed` → `render_and_present`; the deleted body
    // attached buffers without a busy check, bypassed the frame-callback
    // latch, and hard-exited on AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER.

    pub fn is_open(&self) -> bool {
        self.is_open
    }
    pub fn close_requested(&self) -> bool {
        self.common.current_window_state().flags.close_requested
    }
    pub fn close(&mut self) {
        // WebRender's Renderer must be deinit()'d, not dropped — texture
        // deletion has to happen inside a frame. Never doing so crashed debug
        // builds on close and leaked GPU resources in release.
        self.common.deinit_renderer();
        if let Some(doc_id) = self.common.document_id {
            crate::desktop::gl_texture_integration::remove_document_textures(&doc_id);
        }
        self.is_open = false;
    }

    /// Re-point every proxy's listener user-data to this (now stable, boxed) `self`.
    ///
    /// All Wayland listeners are registered in `new()` against the stack-local
    /// `&mut window`, which the run loop then MOVES into a heap `Box`. libwayland stores
    /// the user-data *pointer* (not a copy) and hands that exact pointer to every event
    /// callback — so without this fixup every event (configure, close, pointer, keyboard,
    /// touch, IME, …) is delivered with a dangling `new()`-stack pointer. The most visible
    /// symptom: `xdg_toplevel.close` writes `is_open = false` into the dead stack copy, so
    /// the run loop (reading the live boxed copy) never sees it and the window won't close.
    /// Other state updates leak through shared heap pointers as use-after-free, producing
    /// erratic, focus-dependent input behaviour. Verified empirically: registration addr
    /// `0x7ffe…` (stack) vs live boxed addr `0x5bcb…` (heap).
    fn rebind_listeners(&mut self) {
        let set = self.wayland.wl_proxy_set_user_data;
        let me = self as *mut Self as *mut std::ffi::c_void;
        // `listener_proxies` is the COMPLETE set: every `*_add_listener` call that
        // can run before this rebind hands its proxy to `track_listener` first, so
        // the list is built by the registration itself and cannot drift out of sync
        // with it. Proxies created LATER (frame callbacks, per-drag wl_data_offers,
        // popups) are made by handlers that already hold the stable pointer, so they
        // inherit it automatically and are deliberately not recorded — that also
        // keeps this list bounded (one frame callback per frame would not be).
        for i in 0..self.listener_proxies.len() {
            let p = self.listener_proxies[i];
            if !p.is_null() {
                unsafe { set(p, me) };
            }
        }
    }

    /// Record a proxy whose listener dereferences its user-data as
    /// `*mut WaylandWindow`, so [`Self::rebind_listeners`] re-points it once the
    /// window reaches its final (heap) address.
    ///
    /// MUST be called next to every `*_add_listener` that passes a
    /// `*mut WaylandWindow` as user-data and can run before the rebind — i.e.
    /// everything registered from `new()` or from a handler dispatched by the
    /// initial `wl_display_roundtrip_queue`. Calling it after the rebind is a
    /// no-op: by then registration already uses the stable pointer.
    pub(super) fn track_listener<T>(&mut self, proxy: *mut T) {
        if self.listeners_rebound || proxy.is_null() {
            return;
        }
        self.listener_proxies.push(proxy.cast());
    }

    /// Rebind listeners to the stable `self` exactly once, on the first event pump after
    /// the window has been boxed by the run loop. Safe to call every poll.
    #[inline]
    fn ensure_listeners_rebound(&mut self) {
        if !self.listeners_rebound {
            self.rebind_listeners();
            self.listeners_rebound = true;
        }
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

        // Body shared with every other backend
        // (`PlatformWindow::dispatch_accessibility_actions`): apply each action,
        // mark the display list dirty for a non-empty affected set, dispatch the
        // callbacks it mapped to and honour the `Update` they return. This used
        // to be a hand-copy per backend, and a hand-copy is how the callback
        // dispatch went missing here in the first place.
        use crate::desktop::shell2::common::event::PlatformWindow as _;
        self.dispatch_accessibility_actions(actions);
        self.request_redraw();
    }

    pub fn request_redraw(&mut self) {
        self.needs_redraw.raise();
        if self.configured {
            self.generate_frame_if_needed();
        }
    }
}

// PlatformWindow Trait Implementation (Cross-platform V2 Event System)

/// `wl_surface.set_input_region` from the frame's alpha outline. Rects are
/// PHYSICAL pixels; a wl_region is in surface-local (logical) coordinates,
/// so they are divided by the buffer scale (outward-rounded, so an
/// anti-aliased edge stays clickable). The compositor does the visual part
/// on its own: an ARGB buffer's transparent pixels just are not drawn.
fn apply_input_region_from_shape(
    wayland: &Wayland,
    compositor: *mut defines::wl_compositor,
    surface: *mut defines::wl_surface,
    rects: &[azul_layout::cpurender::ShapeRect],
    scale: f32,
) {
    if compositor.is_null() || surface.is_null() || rects.is_empty() {
        return;
    }
    let scale = scale.max(0.01);
    unsafe {
        let region = (wayland.wl_compositor_create_region)(compositor);
        if region.is_null() {
            return;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)] // pixel coords
        for r in rects {
            let x0 = (r.x as f32 / scale).floor();
            let y0 = (r.y as f32 / scale).floor();
            let x1 = ((r.x + r.width) as f32 / scale).ceil();
            let y1 = ((r.y + r.height) as f32 / scale).ceil();
            (wayland.wl_region_add)(
                region,
                x0 as i32,
                y0 as i32,
                (x1 - x0) as i32,
                (y1 - y0) as i32,
            );
        }
        (wayland.wl_surface_set_input_region)(surface, region);
        (wayland.wl_region_destroy)(region);
    }
}

impl PlatformWindow for WaylandWindow {
    fn capture_screen_for_eyedropper(&mut self) -> Option<crate::desktop::eyedropper::Screenshot> {
        let scale = self
            .common
            .current_window_state()
            .size
            .get_hidpi_factor()
            .inner
            .get();
        crate::desktop::eyedropper::wayland::capture(scale)
    }

    fn apply_window_shape(&mut self, rects: &[azul_layout::cpurender::ShapeRect]) {
        let scale = self
            .common
            .current_window_state()
            .size
            .get_hidpi_factor()
            .inner
            .get();
        apply_input_region_from_shape(&self.wayland, self.compositor, self.surface, rects, scale);
    }

    /// Wayland owns its shm pool / EGL drawable, so an application-initiated
    /// resize must rebuild them here — the compositor will not send a
    /// configure for a size the client chose itself.
    fn resize_platform_surface(&mut self, width: i32, height: i32) {
        self.resize_surface(width, height);
    }

    fn regenerate_layout_once(
        &mut self,
    ) -> Result<crate::desktop::shell2::common::layout::LayoutRegenerateResult, String> {
        // The single pass. The bounded lifecycle loop lives in the trait
        // default `regenerate_layout`, which is what frame paths call.
        self.regenerate_layout_inner()
    }

    impl_platform_window_getters!(common);

    fn get_raw_window_handle(&self) -> RawWindowHandle {
        RawWindowHandle::Wayland(WaylandHandle {
            surface: self.surface as *mut c_void,
            display: self.display as *mut c_void,
        })
    }

    fn prepare_callback_invocation(&mut self) -> event::InvokeSingleCallbackBorrows {
        let borrows = self.common.layout_borrows();

        event::InvokeSingleCallbackBorrows {
            layout_window: borrows
                .layout_window
                .expect("Layout window must exist for callback invocation"),
            window_handle: RawWindowHandle::Wayland(WaylandHandle {
                surface: self.surface as *mut c_void,
                display: self.display as *mut c_void,
            }),
            gl_context_ptr: borrows.gl_context_ptr,
            fc_cache_clone: (**borrows.fc_cache).clone(),
            system_style: borrows.system_style.clone(),
            previous_window_state: borrows.previous_window_state,
            current_window_state: borrows.current_window_state,
            renderer_resources: borrows.renderer_resources,
        }
    }

    // Timer Management (Wayland Implementation - uses timerfd for native OS timer support)

    fn flush_a11y_tree_update(&mut self) {
        // MWA-A3e: push incremental a11y updates (text edits / caret moves)
        // parked in last_tree_update by the event pass; previously they only
        // reached AT-SPI on the next full relayout.
        #[cfg(feature = "a11y")]
        {
            let pending = self
                .common
                .layout_window
                .as_mut()
                .and_then(|lw| lw.a11y_manager.take_pending());
            if let Some(update) = pending {
                self.accessibility_adapter.update_tree(update);
            }
        }
    }

    fn start_timer(&mut self, timer_id: usize, timer: azul_layout::timer::Timer) {
        let interval_ms = timer.tick_millis();
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window
                .timers
                .insert(azul_core::task::TimerId { id: timer_id }, timer);
        }
        super::timer::start_timerfd(&mut self.timer_fds, timer_id, interval_ms, "Wayland");
    }

    fn stop_timer(&mut self, timer_id: usize) {
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window
                .timers
                .remove(&azul_core::task::TimerId { id: timer_id });
        }
        super::timer::stop_timerfd(&mut self.timer_fds, timer_id, "Wayland");
    }

    // Thread Management (Wayland Implementation - Stored in LayoutWindow)

    fn start_thread_poll_timer(&mut self) {
        // For Wayland, we don't need a separate timer - threads are checked
        // in the event loop when layout_window.threads is non-empty
        // Just mark for regeneration to start checking
        self.common
            .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
    }

    fn stop_thread_poll_timer(&mut self) {
        // No-op for Wayland - thread checking stops automatically when
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
        self.common
            .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
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

    fn request_regeneration_all_windows(&mut self) {
        for wid in super::registry::get_all_window_ids() {
            if wid == self.surface as u64 {
                continue;
            }
            if let Some(wptr) = unsafe { super::registry::get_window(wid) } {
                if let super::LinuxWindow::Wayland(w) = unsafe { &mut *wptr } {
                    w.common
                        .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
                    w.request_redraw();
                }
            }
        }
        // The nested xdg_popup is not a registered window; repaint it too.
        if let Some(p) = self.active_popup.as_mut() {
            p.request_repaint();
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
        anchor: Option<azul_core::geom::LogicalRect>,
    ) {
        // Check if native menus are enabled
        if self
            .common
            .current_window_state()
            .flags
            .use_native_context_menus
        {
            // TODO: Show native Wayland popup via xdg_popup protocol
            log_debug!(
                LogCategory::Platform,
                "[Wayland] Native xdg_popup menu at ({}, {}) - not yet implemented, using fallback",
                position.x,
                position.y
            );
            self.show_fallback_menu(menu, position, anchor);
        } else {
            // Show fallback DOM-based menu
            self.show_fallback_menu(menu, position, anchor);
        }
    }

    // Tooltip Methods (Wayland Implementation)

    fn show_tooltip_from_callback(
        &mut self,
        text: &str,
        position: azul_core::geom::LogicalPosition,
    ) {
        // Wayland tooltips use subsurfaces positioned relative to the parent
        // surface, so the logical position is passed through directly.
        self.show_tooltip(text, position);
    }

    fn hide_tooltip_from_callback(&mut self) {
        self.hide_tooltip();
    }

    fn handle_begin_interactive_move(&mut self) {
        // Wayland: use xdg_toplevel_move to let the compositor manage the window move.
        // This requires the toplevel handle, seat, and the serial from the last pointer event.
        let toplevel = self.xdg_toplevel;
        let seat = self.seat;
        let serial = self.pointer_state.serial;
        if !toplevel.is_null() && !seat.is_null() && serial != 0 {
            unsafe {
                (self.wayland.xdg_toplevel_move)(toplevel, seat, serial);
            }
        }
    }

    fn sync_window_state(&mut self) {
        WaylandWindow::sync_window_state(self);
    }
}

impl WaylandWindow {
    /// Show a fallback window-based menu at the given position.
    ///
    /// Wayland clients have no notion of absolute screen coordinates, so this
    /// path uses `menu::create_menu_popup_options` (parent-relative) instead of
    /// the absolute-coords `desktop::menu::show_menu` used on X11/Win/macOS.
    /// The trigger rectangle is collapsed to a zero-size rect anchored at the
    /// requested position; once xdg_popup wiring lands the positioner will
    /// anchor against this rect on the parent surface.
    fn show_fallback_menu(
        &mut self,
        menu: &azul_core::menu::Menu,
        position: azul_core::geom::LogicalPosition,
        anchor: Option<azul_core::geom::LogicalRect>,
    ) {
        // A real anchor rect is what an xdg_popup positioner wants: the
        // compositor flips and slides against the CONTROL, not against a
        // zero-sized point, and the menu can be sized to match it.
        let trigger_rect = anchor.unwrap_or_else(|| {
            azul_core::geom::LogicalRect::new(position, azul_core::geom::LogicalSize::zero())
        });
        let menu_size = self::menu::calculate_menu_size(menu, &self.common.system_style);

        let menu_options = self::menu::create_menu_popup_options(
            self,
            menu,
            &self.common.system_style,
            trigger_rect,
            menu_size,
        );

        log_debug!(
            LogCategory::Window,
            "[Wayland] Queuing fallback menu window at parent-relative ({}, {}) - will be created in event loop",
            position.x,
            position.y
        );

        self.pending_window_creates.push(menu_options);
    }

    /// Open a menu (`WindowType::Menu` create options) as a nested `xdg_popup`
    /// anchored to its trigger rect on this parent surface, instead of a
    /// mispositioned, event-capturing `xdg_toplevel`. Replaces any open menu.
    ///
    /// The trigger/anchor rect was stashed in the menu layout callback's RefAny
    /// (`menu::MenuLayoutData::trigger_rect`), in parent-surface-relative
    /// coordinates — Wayland clients cannot address absolute screen coordinates,
    /// so the compositor positions the popup from this rect.
    pub fn open_menu_popup(&mut self, options: WindowCreateOptions) -> Result<(), String> {
        use azul_core::geom::{LogicalRect, LogicalSize};

        // A new menu replaces any currently-open one.
        self.dismiss_active_popup();

        // The anchor: a menu stashes its trigger rect in `MenuLayoutData`; a
        // `<transient-window>` carries its placement in the shared mailbox
        // (`common::transient::TransientWindowData`) — anchor rect AND the edge
        // to open on. Both are parent-surface-relative, which is all a Wayland
        // client can say; the compositor's positioner does the placing.
        let (mailbox_anchor, edge) = match &options.window_state.layout_callback.ctx {
            azul_core::refany::OptionRefAny::Some(refany) => {
                let mut r = refany.clone();
                let menu = r
                    .downcast_ref::<self::menu::MenuLayoutData>()
                    .map(|d| d.trigger_rect);
                let mut r2 = refany.clone();
                let transient = r2
                    .downcast_ref::<crate::desktop::shell2::common::transient::TransientWindowData>(
                    )
                    .map(|d| (d.placement.anchor_rect, d.placement.anchor));
                match (menu, transient) {
                    (Some(rect), _) => (Some(rect), azul_core::transient::TransientAnchor::Cursor),
                    (None, Some((rect, edge))) => (Some(rect), edge),
                    (None, None) => (None, azul_core::transient::TransientAnchor::Cursor),
                }
            }
            azul_core::refany::OptionRefAny::None => {
                (None, azul_core::transient::TransientAnchor::Cursor)
            }
        };
        let mut anchor_rect = mailbox_anchor.unwrap_or_else(|| {
            LogicalRect::new(
                azul_core::geom::LogicalPosition::zero(),
                LogicalSize::zero(),
            )
        });

        // A zero-sized anchor rect is rejected by some compositors — clamp >= 1x1.
        anchor_rect.size.width = anchor_rect.size.width.max(1.0);
        anchor_rect.size.height = anchor_rect.size.height.max(1.0);

        let mut popup_size = options.window_state.size.dimensions;
        popup_size.width = popup_size.width.max(1.0);
        popup_size.height = popup_size.height.max(1.0);

        crate::plog_info!(
            "[wayland-popup] open_menu_popup: anchor=({:.0},{:.0} {:.0}x{:.0}) size={:.0}x{:.0}",
            anchor_rect.origin.x,
            anchor_rect.origin.y,
            anchor_rect.size.width,
            anchor_rect.size.height,
            popup_size.width,
            popup_size.height
        );
        let popup = WaylandPopup::new(self, anchor_rect, popup_size, edge, options)?;
        self.active_popup = Some(Box::new(popup));
        crate::plog_info!("[wayland-popup] xdg_popup created + grab requested, awaiting configure");

        // Flush the get_popup/grab/commit requests so the compositor configures
        // the popup before the next loop iteration renders into it.
        unsafe {
            (self.wayland.wl_display_flush)(self.display);
        }
        Ok(())
    }

    /// Dismiss (close + drop) the active menu popup, if any. Dropping the popup
    /// destroys its wl objects and releases the seat grab.
    pub fn dismiss_active_popup(&mut self) {
        if let Some(popup) = self.active_popup.take() {
            // A `<transient-window>` popup tells its parent — this window —
            // through the mailbox, so the engine's manager and the widget
            // learn it was dismissed (popup_done / click-outside) instead of
            // keeping a ghost "open" that the next swatch click only closes.
            use crate::desktop::shell2::common::transient::{
                poll_popup, post_dismissed, PopupAction,
            };
            let closed_by_parent =
                poll_popup(popup.common.current_window_state()) == PopupAction::Close;
            if !closed_by_parent && post_dismissed(popup.common.current_window_state()) {
                self.common
                    .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
                self.request_redraw();
            }
            drop(popup);
            unsafe {
                (self.wayland.wl_display_flush)(self.display);
            }
        }
    }

    /// Service the active popup each loop iteration: drop it if the compositor
    /// dismissed it (click-outside / popup_done), otherwise render it once the
    /// compositor has configured it.
    pub fn drive_active_popup(&mut self) {
        let dismissed = match self.active_popup.as_ref() {
            // The compositor dismissed it, it closed itself (Escape, focus
            // loss, a menu item's `close_requested`), or the parent's engine
            // told it to close through the mailbox (the app set `open=false`,
            // a press in the parent).
            Some(p) => {
                p.is_dismissed()
                    || !p.is_open
                    || p.close_requested()
                    || crate::desktop::shell2::common::transient::poll_popup(
                        p.common.current_window_state(),
                    ) == crate::desktop::shell2::common::transient::PopupAction::Close
            }
            None => return,
        };
        if dismissed {
            self.dismiss_active_popup();
            return;
        }
        // Windows a callback inside the popup asked for (a submenu, a nested
        // popup) are the parent's to create — they replace the active popup.
        let nested: Vec<WindowCreateOptions> = self
            .active_popup
            .as_mut()
            .map(|p| core::mem::take(&mut p.pending_window_creates))
            .unwrap_or_default();
        self.pending_window_creates.extend(nested);
        if let Some(popup) = self.active_popup.as_mut() {
            popup.render_if_ready();
        }
    }

    pub fn new(
        mut options: WindowCreateOptions,
        resources: Arc<super::AppResources>,
    ) -> Result<Self, WindowError> {
        // If background_color is None and no material effect, use system window background
        // Note: When a material is set, the renderer will use transparent clear color automatically
        if options.window_state.background_color.is_none() {
            use azul_core::window::WindowBackgroundMaterial;
            if matches!(
                options.window_state.flags.background_material,
                WindowBackgroundMaterial::Opaque
            ) {
                options.window_state.background_color =
                    resources.system_style.colors.window_background;
            }
            // For materials, leave background_color as None - renderer handles transparency
        }

        // Extract create_callback before consuming options
        let create_callback = options.create_callback.clone();

        let wayland = Wayland::new().map_err(|e| {
            WindowError::PlatformError(format!("Failed to load libwayland-client: {:?}", e))
        })?;
        let xkb = Xkb::new().map_err(|e| {
            WindowError::PlatformError(format!("Failed to load libxkbcommon: {:?}", e))
        })?;

        // Try to load GTK3 IM context for IME support (optional, fail silently)
        let (gtk_im, gtk_im_context) = match Gtk3Im::new() {
            Ok(gtk) => {
                log_debug!(
                    LogCategory::Platform,
                    "[Wayland] GTK3 IM context loaded for IME support"
                );
                let ctx = unsafe { (gtk.gtk_im_context_simple_new)() };
                if !ctx.is_null() {
                    (Some(gtk), Some(ctx))
                } else {
                    log_warn!(
                        LogCategory::Platform,
                        "[Wayland] Failed to create GTK IM context instance"
                    );
                    (None, None)
                }
            }
            Err(e) => {
                log_debug!(
                    LogCategory::Platform,
                    "[Wayland] GTK3 IM not available (IME positioning disabled): {:?}",
                    e
                );
                (None, None)
            }
        };

        let display = unsafe { (wayland.wl_display_connect)(std::ptr::null()) };
        if display.is_null() {
            return Err(WindowError::PlatformError(
                "Failed to connect to Wayland display".into(),
            ));
        }

        let event_queue = unsafe { (wayland.wl_display_create_queue)(display) };
        let registry = unsafe { (wayland.wl_display_get_registry)(display) };
        unsafe { (wayland.wl_proxy_set_queue)(registry as _, event_queue) };

        // Initialize LayoutWindow
        let mut layout_window =
            crate::desktop::shell2::common::layout::layout_window_sharing_fonts(
                resources.font_manager.as_ref(),
                &resources.fc_cache,
            )
            .map_err(|e| {
                WindowError::PlatformError(format!("LayoutWindow::new failed: {:?}", e))
            })?;
        layout_window.routes = resources.config.routes.clone();

        let mut common = event::CommonWindowState::new(
            FullWindowState {
                title: options.window_state.title.clone(),
                size: options.window_state.size,
                position: options.window_state.position,
                flags: options.window_state.flags,
                theme: options.window_state.theme,
                debug_state: options.window_state.debug_state,
                keyboard_state: options.window_state.keyboard_state.clone(),
                mouse_state: options.window_state.mouse_state.clone(),
                touch_state: options.window_state.touch_state.clone(),
                ime_position: options.window_state.ime_position,
                platform_specific_options: options.window_state.platform_specific_options.clone(),
                renderer_options: options.window_state.renderer_options,
                background_color: options.window_state.background_color,
                layout_callback: options.window_state.layout_callback.clone(),
                close_callback: options.window_state.close_callback.clone(),
                monitor_id: OptionU32::None,
                window_id: options.window_state.window_id.clone(),
                window_focused: false,
                active_route: azul_core::resources::OptionRouteMatch::None,
            },
            resources.fc_cache.clone(),
            resources.system_style.clone(),
            resources.app_data.clone(),
            resources.undo_manager.clone(),
        );
        common.layout_window = Some(layout_window);
        common.cpu_hit_tester = Some(azul_layout::headless::CpuHitTester::new());
        common.gl_context_ptr = None.into();
        common.regen = crate::desktop::shell2::common::event::RegenerationState::idle_initial();

        let mut window = Self {
            wayland: wayland.clone(),
            xkb,
            gtk_im,
            gtk_im_context,
            text_input_manager: None, // Will be populated if compositor supports text-input v3
            text_input: None,
            text_input_active: false,
            text_input_enabled: false,
            text_input_pending: events::TextInputPendingState::default(),
            display,
            event_queue,
            registry,
            compositor: std::ptr::null_mut(),
            shm: std::ptr::null_mut(),
            seat: std::ptr::null_mut(),
            xdg_wm_base: std::ptr::null_mut(),
            surface: std::ptr::null_mut(),
            xdg_surface: std::ptr::null_mut(),
            xdg_toplevel: std::ptr::null_mut(),
            is_open: true,
            configured: false,
            configure_size_changed: false,
            subcompositor: None,
            blur_manager: None,
            current_blur: None,
            decoration_manager: None,
            toplevel_decoration: None,
            fractional_scale_manager: None,
            viewporter: None,
            fractional_scale: None,
            viewport: None,
            preferred_scale_120: None,
            tooltip: None,
            screensaver_inhibit_cookie: None,
            dbus_connection: None,
            common,
            new_frame_ready: Arc::new((Mutex::new(false), Condvar::new())),
            keyboard_state: events::WaylandKeyboardState::new(),
            pointer_state: events::PointerState::new(),
            keyboard: std::ptr::null_mut(),
            touch: std::ptr::null_mut(),
            listeners_rebound: false,
            listener_proxies: Vec::new(),
            tablet_manager: std::ptr::null_mut(),
            tablet_seat: std::ptr::null_mut(),
            tablet_initialized: false,
            data_device_manager: std::ptr::null_mut(),
            data_device: std::ptr::null_mut(),
            clipboard_offer: std::ptr::null_mut(),
            clipboard_source: std::ptr::null_mut(),
            last_input_serial: 0,
            primary_selection_manager: std::ptr::null_mut(),
            primary_selection_device: std::ptr::null_mut(),
            primary_selection_offer: std::ptr::null_mut(),
            primary_selection_source: std::ptr::null_mut(),
            primary_selection_initialized: false,
            current_axis_source: 0,
            pending_axis_value: (0.0, 0.0),
            pending_axis_discrete: (0.0, 0.0),
            pending_axis: false,
            seat_version: 0,
            data_device_version: 0,
            data_device_initialized: false,
            drag: events::WaylandDragState::default(),
            tablet_pen: events::TabletPenPending::default(),
            tablet_pad: events::TabletPadPending::default(),
            tablet_tools: std::collections::HashMap::new(),
            tablet_info: events::TabletStatic::default(),
            tablet_pad_static: None,
            frame_callback_pending: false,
            frame_callback_armed_at: None,
            needs_redraw: super::super::common::event::LatchedRequest::default(),
            gpu_damage_rects: Vec::new(),
            gpu_last_render_presented: true,
            timer_fds: std::collections::BTreeMap::new(),
            #[cfg(feature = "a11y")]
            accessibility_adapter: LinuxAccessibilityAdapter::new(),
            // CPU rendering state will be initialized after receiving wl_shm from registry
            render_mode: RenderMode::Cpu(None),
            #[cfg(feature = "cpurender")]
            cpu_backend: crate::desktop::shell2::headless::CpuBackend::new(),
            os_present_requested: true, // first present must be full
            // 25 chars/s after 400ms — the common compositor default; a
            // wl_keyboard.repeat_info event (seat v4+) overrides both.
            key_repeat_rate_ms: 40,
            key_repeat_delay_ms: 400,
            key_repeat_fd: unsafe {
                libc::timerfd_create(
                    libc::CLOCK_MONOTONIC,
                    libc::TFD_NONBLOCK | libc::TFD_CLOEXEC,
                )
            },
            key_repeat_keycode: None,
            pressed_key_vks: std::collections::BTreeMap::new(),
            known_outputs: Vec::new(),
            current_outputs: Vec::new(),
            pending_window_creates: Vec::new(),
            active_popup: None,
            pointer_over_popup: false,
            gnome_menu: None, // Will be initialized if GNOME menus are enabled
            resources: resources.clone(),
            dynamic_selector_context: {
                let mut ctx = azul_css::dynamic_selector::DynamicSelectorContext::from_system_style(
                    &resources.system_style,
                );
                ctx.viewport_width = options.window_state.size.dimensions.width;
                ctx.viewport_height = options.window_state.size.dimensions.height;
                ctx.orientation = if ctx.viewport_width > ctx.viewport_height {
                    azul_css::dynamic_selector::OrientationType::Landscape
                } else {
                    azul_css::dynamic_selector::OrientationType::Portrait
                };
                ctx
            },
        };

        // Initialize the accessibility adapter (open the AT-SPI connection via
        // accesskit_unix). X11 does this at window creation (x11/mod.rs); Wayland
        // previously constructed the adapter but never initialized it, so
        // `update_tree()` silently no-op'd (inner Adapter stayed None) and NO
        // a11y tree was ever published on native Wayland. Mirror X11 here.
        #[cfg(feature = "a11y")]
        {
            let window_name = "Azul Window";
            window
                .accessibility_adapter
                .initialize(window_name)
                .map_err(|e| {
                    WindowError::PlatformError(format!("Accessibility init failed: {}", e))
                })?;
        }

        // Initialize monitor cache once at window creation
        if let Some(ref lw) = window.common.layout_window {
            if let Ok(mut guard) = lw.monitors.lock() {
                *guard = crate::desktop::display::get_monitors();
            }
        }

        // 'static: the proxy keeps the pointer (a stack-local would be a
        // use-after-free once globals arrive after this frame, e.g. hotplug).
        static REGISTRY_LISTENER: defines::wl_registry_listener = defines::wl_registry_listener {
            global: events::registry_global_handler,
            global_remove: events::registry_global_remove_handler,
        };
        unsafe {
            (window.wayland.wl_proxy_add_listener)(
                registry as _,
                &REGISTRY_LISTENER as *const _ as _,
                &mut window as *mut _ as *mut _,
            )
        };
        window.track_listener(registry);
        // The registry — and every object bound from it — lives on our custom
        // `event_queue`, so the initial global-binding roundtrip MUST dispatch
        // THAT queue. `wl_display_roundtrip()` pumps only the default queue,
        // leaving wl_compositor/xdg_wm_base unbound (null) → segfault below in
        // create_surface. Use the queue-aware roundtrip.
        unsafe { (window.wayland.wl_display_roundtrip_queue)(display, window.event_queue) };

        if window.compositor.is_null() || window.xdg_wm_base.is_null() {
            return Err(WindowError::PlatformError(
                "Wayland: required globals (wl_compositor / xdg_wm_base) not advertised by compositor".into(),
            ));
        }

        window.surface =
            unsafe { (window.wayland.wl_compositor_create_surface)(window.compositor) };

        // Add wl_surface listener to track which monitors the window is on
        static SURFACE_LISTENER: defines::wl_surface_listener = defines::wl_surface_listener {
            enter: events::wl_surface_enter_handler,
            leave: events::wl_surface_leave_handler,
        };
        unsafe {
            (window.wayland.wl_surface_add_listener)(
                window.surface,
                &SURFACE_LISTENER,
                &mut window as *mut _ as *mut _,
            )
        };
        window.track_listener(window.surface);

        // Fractional-scale support (wp-fractional-scale-v1 + wp-viewporter).
        // Both managers were bound in the registry roundtrip above (if the
        // compositor has them); create the per-surface objects now. The
        // wp_fractional_scale_v1.preferred_scale event then drives size.dpi
        // (see events::wp_fractional_scale_preferred_scale_handler); until it
        // arrives the integer wl_output scale path runs unchanged.
        if let Some(mgr) = window.fractional_scale_manager {
            unsafe {
                // get_fractional_scale: opcode 1, "no" (new_id, object<wl_surface>)
                // — same marshal_constructor pattern as get_toplevel_decoration.
                type GetFracCtor = unsafe extern "C" fn(
                    *mut defines::wl_proxy,
                    u32,
                    *const defines::wl_interface,
                    *mut c_void,
                    *mut defines::wl_surface,
                ) -> *mut defines::wl_proxy;
                let f: GetFracCtor =
                    std::mem::transmute(window.wayland.wl_proxy_marshal_constructor);
                let fs = f(
                    mgr as *mut defines::wl_proxy,
                    1, // opcode 1 = get_fractional_scale (opcode 0 is `destroy`!)
                    defines::get_wp_fractional_scale_v1_interface(),
                    std::ptr::null_mut(),
                    window.surface,
                );
                if !fs.is_null() {
                    static FRACTIONAL_SCALE_LISTENER: defines::wp_fractional_scale_v1_listener =
                        defines::wp_fractional_scale_v1_listener {
                            preferred_scale: events::wp_fractional_scale_preferred_scale_handler,
                        };
                    (window.wayland.wl_proxy_add_listener)(
                        fs,
                        &FRACTIONAL_SCALE_LISTENER as *const _ as *const _,
                        &mut window as *mut _ as *mut _,
                    );
                    window.track_listener(fs);
                    window.fractional_scale = Some(fs as *mut defines::wp_fractional_scale_v1);
                }
            }
        }
        if let Some(vpr) = window.viewporter {
            window.viewport =
                unsafe { wp_viewporter_get_viewport(&window.wayland, vpr, window.surface) };
            if window.fractional_scale.is_some() && window.viewport.is_some() {
                log_info!(
                    LogCategory::Platform,
                    "[Wayland] Fractional scaling enabled (wp_fractional_scale_v1 + wp_viewport)"
                );
            }
        }

        window.xdg_surface = unsafe {
            (window.wayland.xdg_wm_base_get_xdg_surface)(window.xdg_wm_base, window.surface)
        };

        // 'static: wl_proxy_add_listener stores the pointer, so the listener must
        // outlive the proxy (a stack-local here is a use-after-free that only
        // "works" until the stack frame is reused).
        static XDG_SURFACE_LISTENER: defines::xdg_surface_listener =
            defines::xdg_surface_listener {
                configure: events::xdg_surface_configure_handler,
            };
        unsafe {
            (window.wayland.xdg_surface_add_listener)(
                window.xdg_surface,
                &XDG_SURFACE_LISTENER,
                &mut window as *mut _ as *mut _,
            )
        };
        window.track_listener(window.xdg_surface);

        window.xdg_toplevel =
            unsafe { (window.wayland.xdg_surface_get_toplevel)(window.xdg_surface) };

        // Attach listener to receive configure and close events from compositor
        static XDG_TOPLEVEL_LISTENER: defines::xdg_toplevel_listener =
            defines::xdg_toplevel_listener {
                configure: events::xdg_toplevel_configure_handler,
                close: events::xdg_toplevel_close_handler,
                configure_bounds: events::xdg_toplevel_configure_bounds_handler,
                wm_capabilities: events::xdg_toplevel_wm_capabilities_handler,
            };
        unsafe {
            (window.wayland.xdg_toplevel_add_listener)(
                window.xdg_toplevel,
                &XDG_TOPLEVEL_LISTENER,
                &mut window as *mut _ as *mut _,
            )
        };
        window.track_listener(window.xdg_toplevel);

        // Request server-side decorations (xdg-decoration-unstable-v1) so the
        // compositor draws a titlebar (move / close), instead of relying on
        // client-side decorations azul doesn't render -> the window was an
        // immovable, uncloseable bare rectangle on Wayland. get_toplevel_decoration:
        // opcode 1, "no" (new_id<zxdg_toplevel_decoration_v1>, object<xdg_toplevel>),
        // then set_mode(server_side=2): opcode 1, "u". The compositor confirms via the
        // configure event (toplevel_decoration_configure_handler).
        if let Some(mgr) = window.decoration_manager {
            unsafe {
                let deco_iface = defines::get_zxdg_toplevel_decoration_v1_interface();
                // Use wl_proxy_marshal_constructor (proxy, opcode, new-interface,
                // NULL new_id placeholder, ...args) -- the same proven path as
                // xdg_surface_get_toplevel etc. (The wl_proxy_marshal_flags variant
                // returned NULL here.) get_toplevel_decoration: opcode 0, "no".
                type GetDecoCtor = unsafe extern "C" fn(
                    *mut defines::wl_proxy,
                    u32,
                    *const defines::wl_interface,
                    *mut std::ffi::c_void,
                    *mut defines::xdg_toplevel,
                ) -> *mut defines::wl_proxy;
                let f: GetDecoCtor =
                    std::mem::transmute(window.wayland.wl_proxy_marshal_constructor);
                // opcode 1 = get_toplevel_decoration (opcode 0 is `destroy`!).
                let deco = f(
                    mgr as *mut defines::wl_proxy,
                    1,
                    deco_iface,
                    std::ptr::null_mut(),
                    window.xdg_toplevel,
                );
                if !deco.is_null() {
                    static DECO_LISTENER: defines::zxdg_toplevel_decoration_v1_listener =
                        defines::zxdg_toplevel_decoration_v1_listener {
                            configure: events::toplevel_decoration_configure_handler,
                        };
                    (window.wayland.wl_proxy_add_listener)(
                        deco,
                        &DECO_LISTENER as *const _ as *const _,
                        // MWA-B6/MWA-D: same pattern as every other listener
                        // registered in this constructor — the pointer to
                        // the stack local is TEMPORARY by design; the first
                        // poll re-binds all proxy listeners to the stable
                        // boxed `self` (see `listeners_rebound`), and the
                        // configure handler (which acts on the window since
                        // MWA-B6) only runs during event dispatch, i.e.
                        // after that rebind.
                        &mut window as *mut WaylandWindow as *mut _,
                    );
                    window.track_listener(deco);
                    // MWA-B6: honor flags.decorations instead of forcing
                    // server-side. CSD-wanting and frameless windows request
                    // client_side (compositor draws nothing); everything
                    // else requests server_side. client_side=1, server_side=2.
                    let flags = &window.common.current_window_state().flags;
                    let wants_csd = crate::desktop::csd::should_inject_csd(
                        flags.has_decorations,
                        flags.decorations,
                    );
                    let frameless = flags.decorations == azul_core::window::WindowDecorations::None;
                    let mode: u32 = if wants_csd || frameless { 1 } else { 2 };
                    // set_mode: opcode 1, signature "u".
                    type SetModeFn = unsafe extern "C" fn(*mut defines::wl_proxy, u32, u32);
                    let set_mode_fn: SetModeFn =
                        std::mem::transmute(window.wayland.wl_proxy_marshal);
                    set_mode_fn(deco, 1, mode);
                    window.toplevel_decoration =
                        Some(deco as *mut defines::zxdg_toplevel_decoration_v1);
                    log_info!(
                        LogCategory::Platform,
                        "[Wayland] Requested {} decorations (xdg-decoration)",
                        if mode == 2 {
                            "server-side"
                        } else {
                            "client-side"
                        }
                    );
                }
            }
        } else {
            // MWA-B6: no xdg-decoration protocol (e.g. GNOME) — the
            // compositor will NEVER draw a frame. If the user asked for
            // normal decorations, flip this window to CSD so it isn't a
            // bare, immovable, uncloseable rectangle.
            if window.common.current_window_state().flags.decorations
                != azul_core::window::WindowDecorations::None
            {
                window
                    .common
                    .update_window_state(event::WindowStateSource::Os, |ws| {
                        ws.flags.decorations = azul_core::window::WindowDecorations::None;
                        ws.flags.has_decorations = true;
                    });
                log_info!(
                    LogCategory::Platform,
                    "[Wayland] No xdg-decoration protocol — falling back to CSD titlebar"
                );
            }
        }

        let title = CString::new(options.window_state.title.as_str()).unwrap();
        unsafe { (window.wayland.xdg_toplevel_set_title)(window.xdg_toplevel, title.as_ptr()) };

        let width = options.window_state.size.dimensions.width as i32;
        let height = options.window_state.size.dimensions.height as i32;

        // Backend selection.
        //  - AZ_BACKEND=cpu (or HwAcceleration::Disabled): NO GL trial at all —
        //    render purely on the CPU (wl_shm + cpurender, zero Mesa), leaving
        //    gl_context_ptr = None so image/canvas callbacks produce CPU pixmaps
        //    instead of GL textures.
        //  - AZ_BACKEND=gpu: force GL even if it turns out to be a software driver.
        //  - default (Auto): try GL, but if the driver is a software rasteriser
        //    (llvmpipe/swrast) drop it and render on the CPU — tiny-skia cpurender
        //    is faster than software GL and avoids desktop-GLSL shader issues.
        use crate::desktop::shell2::common::compositor::{AzBackend, GpuCheckResult};
        let backend = AzBackend::resolve(
            options
                .renderer
                .as_option()
                .map(|r| r.hw_accel)
                .or(Some(options.window_state.renderer_options.hw_accel)),
        );
        let force_cpu = matches!(backend, AzBackend::Cpu | AzBackend::Headless);
        let force_gpu = matches!(backend, AzBackend::Gpu);

        let render_mode = if force_cpu {
            log_info!(
                LogCategory::Rendering,
                "[Wayland] AZ_BACKEND=cpu -> CPU rendering (no GL context created)"
            );
            RenderMode::Cpu(Some(CpuFallbackState::new(
                &wayland, window.shm, width, height, 1,
            )?))
        } else {
            match gl::GlContext::new(&wayland, display, window.surface, width, height) {
                Ok(mut gl_context) => 'gpu: {
                    gl_context.configure_vsync(options.window_state.renderer_options.vsync);
                    // GL function loading must never dead-end to "no window".
                    let gl_functions = match gl_context
                        .egl
                        .as_ref()
                        .and_then(|egl| GlFunctions::initialize(egl).ok())
                    {
                        Some(f) => f,
                        None => {
                            log_warn!(
                                LogCategory::Rendering,
                                "[Wayland] GL function loading failed — falling back to CPU rendering"
                            );
                            drop(gl_context);
                            break 'gpu RenderMode::Cpu(Some(CpuFallbackState::new(
                                &wayland, window.shm, width, height, 1,
                            )?));
                        }
                    };
                    // Detect a software rasteriser; under Auto, prefer cpurender.
                    gl_context.make_current();
                    // Keep the blacklist detail — the X11 twin prints the
                    // renderer string, and "software GL" alone doesn't say
                    // WHICH driver was rejected or why.
                    let software_info =
                        match crate::desktop::shell2::common::compositor::query_gpu_info(
                            &gl_functions.functions,
                        ) {
                            GpuCheckResult::Blacklisted { info, reason } => Some((info, reason)),
                            _ => None,
                        };
                    let is_software = software_info.is_some();
                    if is_software && !force_gpu {
                        let (info, reason) = software_info.unwrap_or_default();
                        log_info!(
                            LogCategory::Rendering,
                            "[Wayland] software GL detected ({:?}: {}) -> CPU rendering \
                             (cpurender is faster; set AZ_BACKEND=gpu to override)",
                            info,
                            reason
                        );
                        drop(gl_context);
                        RenderMode::Cpu(Some(CpuFallbackState::new(
                            &wayland, window.shm, width, height, 1,
                        )?))
                    } else {
                        RenderMode::Gpu(gl_context, gl_functions)
                    }
                }
                Err(e) => {
                    log_warn!(
                        LogCategory::Rendering,
                        "[Wayland] GPU context failed: {:?}. Falling back to CPU.",
                        e
                    );
                    RenderMode::Cpu(Some(CpuFallbackState::new(
                        &wayland, window.shm, width, height, 1,
                    )?))
                }
            }
        };
        window.render_mode = render_mode;

        // Initialize WebRender on the GPU context; if it fails (e.g. shaders won't
        // compile on this driver) fall back to CPU rendering for this window rather
        // than failing window creation — "GPU init failed" must never mean "no window".
        let webrender_failed =
            if let RenderMode::Gpu(gl_context, gl_functions) = &mut window.render_mode {
                gl_context.make_current();
                // Borrow gl_functions separately to avoid double mutable borrow
                let gl_funcs_ref = gl_functions as *const GlFunctions;
                match window.initialize_webrender(&options, unsafe { &*gl_funcs_ref }) {
                    Ok(_) => false,
                    Err(e) => {
                        log_warn!(
                            LogCategory::Rendering,
                            "[Wayland] WebRender init failed: {:?} — falling back to CPU rendering",
                            e
                        );
                        true
                    }
                }
            } else {
                false
            };
        if webrender_failed {
            window.render_mode = RenderMode::Cpu(Some(CpuFallbackState::new(
                &wayland, window.shm, width, height, 1,
            )?));
        }

        unsafe { (window.wayland.wl_surface_commit)(window.surface) };
        unsafe { (window.wayland.wl_display_flush)(display) };

        // TODO: Window positioning on Wayland
        // Wayland does not support programmatic window positioning - the compositor
        // decides where windows are placed. The options.window_state.position and
        // options.window_state.monitor fields are hints that may be ignored.
        //
        // For feature parity with X11/Windows/macOS, we would position the window here,
        // but Wayland protocol intentionally does not provide this capability.
        // Applications should handle windows opening on unexpected monitors gracefully
        // by tracking actual monitor via wl_surface enter/leave events.
        //
        // See: https://wayland.freedesktop.org/docs/html/ch04.html#sect-Protocol-xdg_surface
        window.position_window_on_monitor(&options);

        // Initialize GNOME menu integration V2 (dlopen-based, no compile-time dependency)
        if options.window_state.flags.use_native_menus
            && super::gnome_menu::should_use_gnome_menus()
        {
            // Get shared DBus library instance (loaded once, shared across all windows)
            if let Some(dbus_lib) = super::gnome_menu::get_shared_dbus_lib() {
                let app_name = &options.window_state.title;

                match super::gnome_menu::GnomeMenuManager::new(app_name, dbus_lib) {
                    Ok(manager) => {
                        // Register window with GNOME Shell
                        // Note: We don't have direct access to wl_surface handle as XID,
                        // but GNOME Shell may be able to find the window via app ID
                        let app_id = None; // TODO: Extract from x11_wm_classes if needed

                        if let Err(e) = manager.set_window_properties_wayland(
                            window.surface as u64, // Use surface pointer as window ID
                            &app_id,
                        ) {
                            log_warn!(
                                LogCategory::Platform,
                                "[Wayland] Failed to set GNOME menu window properties: {}. \
                                 Falling back to client-side decorations.",
                                e
                            );
                        } else {
                            window.gnome_menu = Some(manager);
                            log_info!(
                                LogCategory::Platform,
                                "[Wayland] GNOME menu integration V2 initialized successfully"
                            );
                        }
                    }
                    Err(e) => {
                        log_warn!(
                            LogCategory::Platform,
                            "[Wayland] Failed to initialize GNOME menu integration V2: {}. \
                             Falling back to client-side decorations.",
                            e
                        );
                    }
                }
            }
        }

        // Invoke create_callback if provided (for GL resource upload, config loading, etc.)
        // This runs AFTER GL context is ready but BEFORE any layout is done
        if let Some(mut callback) = create_callback.into_option() {
            use azul_core::window::RawWindowHandle;

            let raw_handle = RawWindowHandle::Wayland(azul_core::window::WaylandHandle {
                surface: window.surface as *mut _,
                display: window.display as *mut _,
            });

            // Initialize LayoutWindow if not already done
            if window.common.layout_window.is_none() {
                let mut layout_window =
                    crate::desktop::shell2::common::layout::layout_window_sharing_fonts(
                        window.resources.font_manager.as_ref(),
                        &window.resources.fc_cache,
                    )
                    .map_err(|e| {
                        WindowError::PlatformError(format!(
                            "Failed to create LayoutWindow: {:?}",
                            e
                        ))
                    })?;

                if let Some(doc_id) = window.common.document_id {
                    layout_window.document_id = doc_id;
                }
                if let Some(ns_id) = window.common.id_namespace {
                    layout_window.id_namespace = ns_id;
                }
                layout_window.current_window_state = window.common.current_window_state().clone();
                layout_window.renderer_type = Some(azul_core::window::RendererType::Hardware);
                layout_window.routes = window.resources.config.routes.clone();
                // Initialize monitor cache once at window creation
                if let Ok(mut guard) = layout_window.monitors.lock() {
                    *guard = crate::desktop::display::refresh_monitors();
                }
                window.common.layout_window = Some(layout_window);
                // A fresh layout window starts with no tablet-device list;
                // re-publish what the descriptive listeners accumulated.
                window.sync_tablet_devices();
            }

            // Get mutable references needed for invoke_single_callback
            let borrows = window.common.layout_borrows();
            let layout_window = borrows
                .layout_window
                .expect("LayoutWindow should exist at this point");
            // Get app_data for callback
            let mut app_data_ref = window.resources.app_data.borrow_mut();

            let (changes, _update) = layout_window.invoke_single_callback(
                &mut callback,
                &mut *app_data_ref,
                &raw_handle,
                borrows.gl_context_ptr,
                window.resources.system_style.clone(),
                &azul_layout::callbacks::ExternalSystemCallbacks::rust_internal(),
                borrows.previous_window_state,
                borrows.current_window_state,
                borrows.renderer_resources,
            );

            drop(app_data_ref);
            use crate::desktop::shell2::common::event::PlatformWindow;
            for change in &changes {
                let r = window.apply_user_change(change);
                if r != azul_core::events::ProcessEventResult::DoNothing {
                    window
                        .common
                        .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
                }
            }
        }

        // Register debug timer if AZ_DEBUG is enabled
        #[cfg(feature = "std")]
        if crate::desktop::shell2::common::debug_server::is_debug_enabled() {
            // Initialize LayoutWindow if not already done
            if window.common.layout_window.is_none() {
                if let Ok(mut layout_window) =
                    crate::desktop::shell2::common::layout::layout_window_sharing_fonts(
                        window.resources.font_manager.as_ref(),
                        &window.resources.fc_cache,
                    )
                {
                    if let Some(doc_id) = window.common.document_id {
                        layout_window.document_id = doc_id;
                    }
                    if let Some(ns_id) = window.common.id_namespace {
                        layout_window.id_namespace = ns_id;
                    }
                    layout_window.current_window_state =
                        window.common.current_window_state().clone();
                    layout_window.renderer_type = Some(azul_core::window::RendererType::Hardware);
                    layout_window.routes = window.resources.config.routes.clone();
                    // Initialize monitor cache once at window creation
                    if let Ok(mut guard) = layout_window.monitors.lock() {
                        *guard = crate::desktop::display::refresh_monitors();
                    }
                    window.common.layout_window = Some(layout_window);
                    // Fresh layout window: re-publish the tablet-device list.
                    window.sync_tablet_devices();
                }
            }

            // Register debug timer is now done from run() with explicit channel + component map
        }

        // Apply initial background material if not Opaque
        {
            use azul_core::window::WindowBackgroundMaterial;
            let initial_material = window
                .common
                .current_window_state()
                .flags
                .background_material;
            if !matches!(initial_material, WindowBackgroundMaterial::Opaque) {
                log_trace!(
                    LogCategory::Window,
                    "[Wayland] Applying initial background material: {:?}",
                    initial_material
                );
                window.apply_background_material(initial_material);
            }
        }

        // Apply initial window state for fields not set during window creation
        window.apply_initial_window_state();

        Ok(window)
    }

    /// Position window on requested monitor (Wayland does not support this)
    fn position_window_on_monitor(&mut self, _options: &WindowCreateOptions) {
        // TODO: Wayland limitation
        // Unlike X11/Windows/macOS, Wayland does not allow applications to position
        // windows programmatically. The compositor controls all window placement.
        //
        // This function exists for API consistency across platforms, but is a no-op
        // on Wayland. Applications should:
        // 1. Use options.window_state.monitor as a hint (may be ignored by compositor)
        // 2. Track actual monitor via get_current_monitor_id() after mapping
        // 3. Handle windows opening on unexpected monitors gracefully
        //
        // Possible future improvements:
        // - Use xdg_toplevel_set_fullscreen(output) for fullscreen windows
        // - Use layer-shell protocol for positioned overlays (requires compositor support)
    }

    fn initialize_webrender(
        &mut self,
        options: &WindowCreateOptions,
        gl_functions: &GlFunctions,
    ) -> Result<(), WindowError> {
        let new_frame_ready = Arc::new((Mutex::new(false), Condvar::new()));
        let (mut renderer, sender) = webrender::create_webrender_instance(
            gl_functions.functions.clone(),
            Box::new(Notifier {
                new_frame_ready: new_frame_ready.clone(),
                // The Wayland loop consumes the flag in its render path; frame
                // callbacks provide the wake. (An eventfd wake like X11's can
                // be added if idle-frame latency shows up here.)
                wake: None,
            }),
            wr_translate2::default_renderer_options(
                options,
                wr_translate2::create_program_cache(&gl_functions.functions),
                // EGL backend: buffer-age partial present (WR accumulates
                // dirty regions over the back buffer's age and reports the
                // total through this cell for eglSwapBuffersWithDamage).
                match &self.render_mode {
                    RenderMode::Gpu(gl_context, _) => Some(gl_context.wr_damage.clone()),
                    _ => None,
                },
            ),
            None,
        )
        .map_err(|e| WindowError::PlatformError(format!("WebRender init failed: {:?}", e)))?;

        // External-image-backed content (the paint canvas, GL textures) needs an
        // ExternalImageHandler or WebRender panics ("Found external image, but no
        // handler set!"). macOS/Windows register this; Linux must too — without it,
        // azul-paint crashes the instant external-image content renders (#9).
        renderer.set_external_image_handler(Box::new(
            crate::desktop::wr_translate2::Compositor::default(),
        ));

        self.common.renderer = Some(renderer);
        self.common.render_api = Some(sender.create_api());

        let framebuffer_size = webrender::api::units::DeviceIntSize::new(
            self.common.current_window_state().size.dimensions.width as i32,
            self.common.current_window_state().size.dimensions.height as i32,
        );
        let render_api = self.common.render_api.as_mut().unwrap();
        let wr_doc_id = render_api.add_document(framebuffer_size);
        self.common.document_id = Some(wr_translate2::translate_document_id_wr(wr_doc_id));
        self.common.id_namespace = Some(wr_translate2::translate_id_namespace_wr(
            render_api.get_namespace_id(),
        ));
        let hit_tester_request = render_api.request_hit_tester(wr_doc_id);
        self.common.hit_tester = Some(AsyncHitTester::Requested(hit_tester_request));
        // R1: software GL (llvmpipe/swrast) can't compile desktop GLSL-150 SVG/FXAA
        // shaders — detect it and mark the GlContextPtr Software so they're skipped.
        let mut renderer_type = match crate::desktop::shell2::common::compositor::query_gpu_info(
            &gl_functions.functions,
        ) {
            crate::desktop::shell2::common::compositor::GpuCheckResult::Blacklisted {
                ref info,
                ref reason,
            } => {
                log_warn!(
                    LogCategory::Platform,
                    "[Wayland] software/blacklisted GL ({}): {} -- skipping GPU SVG/FXAA shaders",
                    info.renderer,
                    reason
                );
                RendererType::Software
            }
            _ => RendererType::Hardware,
        };
        // PROVE the context: a non-blacklisted driver can still reject our
        // SVG/brush shaders at every GLSL version. is_gl_usable() actually
        // compiles them; on failure downgrade to Software so the GPU SVG/FXAA/
        // brush shaders are skipped (WebRender, created above, keeps compositing).
        // This is the Wayland analogue of the X11 "context unusable -> CPU"
        // fallback -- here the already-committed WebRender renderer makes a
        // Software downgrade the safe equivalent of a full CPU switch.
        if matches!(renderer_type, RendererType::Hardware) {
            let probe = GlContextPtr::new(RendererType::Hardware, gl_functions.functions.clone());
            if !probe.is_gl_usable() {
                crate::plog_warn!(
                    "[Wayland] GL context unusable (shaders failed to compile at any GLSL \
                     version) -- skipping GPU SVG/FXAA/brush shaders"
                );
                renderer_type = RendererType::Software;
            }
        }
        self.common.gl_context_ptr = OptionGlContextPtr::Some(GlContextPtr::new(
            renderer_type,
            gl_functions.functions.clone(),
        ));
        self.new_frame_ready = new_frame_ready;

        Ok(())
    }

    pub fn wait_for_events(&mut self) -> Result<(), WindowError> {
        use super::super::common::event::PlatformWindow;

        // Re-point listeners to this stable address before the first dispatch (see
        // ensure_listeners_rebound / rebind_listeners).
        self.ensure_listeners_rebound();

        // First, dispatch any pending events without blocking
        let pending = unsafe {
            (self.wayland.wl_display_dispatch_queue_pending)(self.display, self.event_queue)
        };
        if pending > 0 {
            return Ok(()); // Events were processed
        }

        // Render anything still OWED before parking in poll().
        //
        // A frame request can be raised at a moment when it cannot be acted on,
        // and on Wayland the raise alone schedules nothing:
        //   * the CPU present found BOTH shm buffers still held by the
        //     compositor and skipped the attach. The wl_buffer.release that
        //     later frees a slot only flips the slot's `busy` flag — its
        //     listener user-data IS the bare bool, it cannot re-run the frame;
        //   * the first CPU frame ran before the lazy shm allocation existed;
        //   * a redraw was requested while the frame-callback latch was armed,
        //     and the `done` that cleared the latch was dispatched later in
        //     the same batch, after the request had already been swallowed.
        // In all of these the needs_redraw / regeneration request is still
        // raised, but nothing was committed and no frame callback was armed,
        // so a `-1` poll below would sleep ON TOP OF work it owes until the
        // user happens to supply another input event — the "type once, then
        // the screen only updates when you scroll" freeze. X11 has the same
        // guard at the top of its poll_event (regeneration / vview /
        // needs_redraw gate before render_and_present).
        //
        // generate_frame_if_needed() re-checks the frame-callback latch, so
        // this cannot over-render: with a fresh `done` outstanding it returns
        // immediately and the retry rides on frame_done_callback instead.
        let closing_now = !self.is_open || self.common.current_window_state().flags.close_requested;
        if self.configured && !closing_now {
            let vview_pending = self
                .common
                .layout_window
                .as_ref()
                .map(|lw| !lw.pending_virtual_view_updates.is_empty())
                .unwrap_or(false);
            if self.common.regeneration_pending()
                || self.common.relayout_only_pending()
                || self.common.resize_relayout_pending()
                || self.needs_redraw.pending()
                || vview_pending
            {
                self.generate_frame_if_needed();
            }
        }

        // Get the display fd
        let display_fd = unsafe { (self.wayland.wl_display_get_fd)(self.display) };

        unsafe {
            // Flush outgoing requests
            (self.wayland.wl_display_flush)(self.display);

            // Build pollfd array: Wayland connection + all timer fds
            let mut pollfds: Vec<libc::pollfd> = Vec::with_capacity(1 + self.timer_fds.len());

            // Add Wayland display fd
            pollfds.push(libc::pollfd {
                fd: display_fd,
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

            // Key-repeat timerfd (armed while a repeatable key is held)
            let key_repeat_idx = pollfds.len();
            if self.key_repeat_fd >= 0 {
                pollfds.push(libc::pollfd {
                    fd: self.key_repeat_fd,
                    events: libc::POLLIN,
                    revents: 0,
                });
            }

            // Background threads (e.g. MapWidget tile fetches) have NO fd in the
            // poll set, so their completion can't wake poll(). While any thread is
            // in flight, poll on a ~16ms tick and drain thread writebacks on every
            // wake; otherwise block indefinitely (timerfd's still wake us). Without
            // this the fetch workers finish but their writebacks never process
            // until some unrelated Wayland event happens to wake the loop.
            let has_threads = self
                .common
                .layout_window
                .as_ref()
                .map(|lw| !lw.threads.is_empty())
                .unwrap_or(false);
            // NEVER block indefinitely once this window is on its way out.
            //
            // `-1` sleeps until the compositor sends something. After the window
            // has been closed there is no reason for it to send anything ever
            // again, so the loop parked here and the process hung on exit —
            // until a user CLICKED the window, which produced an event, woke the
            // poll, and let teardown finish. That is the whole bug: exiting was
            // waiting on input that only arrives if someone happens to provide
            // it. It reproduced about 1 run in 2 locally, which is exactly what
            // "depends on whether a stray event turns up" looks like.
            //
            // While closing, poll with 0 so the iteration completes and the run
            // loop reaches its `get_all_window_ids()` check and unregisters.
            let closing = !self.is_open || self.common.current_window_state().flags.close_requested;
            // A live tray talks D-Bus, whose fd is not in this poll set — cap
            // the park so the run loop's tray pump answers the panel's
            // property reads (same reasoning as `has_threads`).
            let has_tray = crate::desktop::tray::has_live_tray();
            let timeout_ms: i32 = if closing {
                0
            } else if has_threads {
                16
            } else if has_tray {
                100
            } else {
                -1
            };

            let result = libc::poll(
                pollfds.as_mut_ptr(),
                pollfds.len() as libc::nfds_t,
                timeout_ms,
            );

            let mut any_timer_fired = false;
            if result > 0 {
                // Check Wayland display fd
                if pollfds[0].revents & libc::POLLIN != 0 {
                    // Drain the socket with the canonical NON-BLOCKING triple —
                    // prepare_read_queue (dispatching anything already queued),
                    // read_events, dispatch_queue_pending — the same sequence
                    // poll_event uses.
                    //
                    // NOT wl_display_dispatch_queue: that call returns only once
                    // an event lands on THIS queue, and POLLIN does not promise
                    // that. The readable bytes can belong entirely to a
                    // different queue — the display's own delete_id bookkeeping
                    // queue (libwayland ≥ 1.5), an open menu popup's queue, or
                    // (GPU mode) the EGL implementation's internal queues. In
                    // that case dispatch_queue consumes them and then goes back
                    // to sleep in ITS OWN internal poll on the display fd
                    // ALONE. Our timerfds, the key-repeat fd and the 16 ms
                    // thread tick are not in that poll, so the window froze
                    // whole — no repaint, no timers, no key repeat — until the
                    // compositor happened to send an event for this queue,
                    // i.e. until the user supplied more input. That is the
                    // reported "key press freezes the screen, scrolling
                    // revives it": a key press is the one input you give with
                    // the pointer parked, so nothing follows it to wake the
                    // stuck dispatch.
                    while (self.wayland.wl_display_prepare_read_queue)(
                        self.display,
                        self.event_queue,
                    ) != 0
                    {
                        (self.wayland.wl_display_dispatch_queue_pending)(
                            self.display,
                            self.event_queue,
                        );
                    }
                    (self.wayland.wl_display_read_events)(self.display);
                    (self.wayland.wl_display_dispatch_queue_pending)(
                        self.display,
                        self.event_queue,
                    );
                }

                // A dead compositor connection reports POLLERR/POLLHUP (or
                // POLLNVAL) and can never become POLLIN-readable again — but
                // poll() keeps returning immediately, so ignoring it turned a
                // compositor crash/logout into this loop spinning at 100% CPU
                // dispatching nothing, forever. Treat it as a close request:
                // the run loop honours the flag, unregisters the window and
                // tears it down.
                if pollfds[0].revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                    log_error!(
                        LogCategory::Platform,
                        "[Wayland] display connection lost (revents {:#x}) — closing window",
                        pollfds[0].revents,
                    );
                    // Tell the app before we go, the same way the titlebar X
                    // does — otherwise a compositor crash discarded unsaved
                    // work with no callback ever running. Unlike
                    // xdg_toplevel.close this is NOT vetoable: the connection
                    // is gone, so the flag goes back up whatever the callback
                    // did with it. The pass is also what consumes the delta.
                    self.snapshot_window_state_baseline("wayland.display_connection_lost");
                    self.common
                        .update_window_state(event::WindowStateSource::Os, |ws| {
                            ws.flags.close_requested = true;
                        });
                    let _ = self.process_window_events(0);
                    self.common
                        .update_window_state(event::WindowStateSource::Os, |ws| {
                            ws.flags.close_requested = true;
                        });
                    return Ok(());
                }

                // Check timerfd's - if any fired, invoke timer callbacks
                for (i, &timer_id) in timer_ids.iter().enumerate() {
                    let pollfd_idx = i + 1; // +1 because display fd is at index 0
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

                // Key repeat fired: replay the held key through the normal
                // key path (state 1 = pressed). One replay per wake is enough
                // (repeats coalesce under load instead of bursting).
                if self.key_repeat_fd >= 0
                    && key_repeat_idx < pollfds.len()
                    && pollfds[key_repeat_idx].revents & libc::POLLIN != 0
                {
                    let mut expirations: u64 = 0;
                    libc::read(
                        self.key_repeat_fd,
                        &mut expirations as *mut u64 as *mut libc::c_void,
                        8,
                    );
                    if let Some(keycode) = self.key_repeat_keycode {
                        self.handle_key(keycode, 1);
                    }
                }
            }

            // Invoke expired timer AND thread callbacks via the shared
            // check_timers_and_threads, which ALSO raises needs_redraw when a
            // callback produced a visual change. Run on every wake while threads
            // are active (the 16ms tick guarantees we get here) so tile-fetch
            // writebacks drain promptly.
            if any_timer_fired || has_threads {
                self.check_timers_and_threads();
            }
            // result == 0: timeout (shouldn't happen with -1)
            // result < 0: error or EINTR - ignore and continue
        }

        Ok(())
    }

    /// Export the application menu bar to GNOME Shell via DBus.
    ///
    /// When GNOME native menus are active the software menu bar is suppressed
    /// (`common::layout::inject_software_menubar` returns the DOM unchanged), so
    /// the menu must instead be exported over DBus. This extracts the `Menu`
    /// from the root DOM node — the same source the Windows `inject_menu_bar`
    /// path uses — and hands it to the manager, which converts + registers it
    /// (skipping the work when the menu is unchanged). No-op when GNOME menus
    /// are not in use or the root DOM declares no menu bar.
    fn update_gnome_menu(&self) {
        let manager = match self.gnome_menu.as_ref() {
            Some(m) => m,
            None => return,
        };

        let menu_opt: Option<azul_core::menu::Menu> =
            self.common.layout_window.as_ref().and_then(|lw| {
                lw.layout_results
                    .get(&azul_core::dom::DomId::ROOT_ID)
                    .and_then(|lr| {
                        lr.styled_dom
                            .node_data
                            .as_container()
                            .get(azul_core::dom::NodeId::ZERO)
                            .and_then(|n| n.get_menu_bar())
                            .map(|boxed_menu| boxed_menu.clone())
                    })
            });

        if let Some(menu) = menu_opt {
            if let Err(e) = manager.sync_menu(&menu) {
                super::gnome_menu::debug_log(&format!("Failed to sync GNOME menu: {}", e));
            }
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
                "[WaylandWindow] Processing menu callback for action: {}",
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
            let borrows = self.common.layout_borrows();
            let layout_window = match borrows.layout_window {
                Some(lw) => lw,
                None => {
                    log_warn!(
                        LogCategory::Callbacks,
                        "[WaylandWindow] No layout window available for menu callback"
                    );
                    continue;
                }
            };

            use azul_core::window::RawWindowHandle;

            // Use Wayland handle for menu callbacks
            let raw_handle = RawWindowHandle::Wayland(azul_core::window::WaylandHandle {
                display: self.display as *mut _,
                surface: self.surface as *mut _,
            });

            let (changes, update) = layout_window.invoke_single_callback(
                &mut menu_callback.callback,
                &mut menu_callback.refany,
                &raw_handle,
                borrows.gl_context_ptr,
                borrows.system_style.clone(),
                &azul_layout::callbacks::ExternalSystemCallbacks::rust_internal(),
                borrows.previous_window_state,
                borrows.current_window_state,
                borrows.renderer_resources,
            );

            use crate::desktop::shell2::common::event::PlatformWindow;
            let mut event_result = azul_core::events::ProcessEventResult::DoNothing;
            for change in &changes {
                event_result = event_result.max(self.apply_user_change(change));
            }
            use azul_core::callbacks::Update;
            match update {
                Update::RefreshDom | Update::RefreshDomAllWindows => {
                    event_result = event_result.max(
                        azul_core::events::ProcessEventResult::ShouldRegenerateDomCurrentWindow,
                    );
                }
                Update::DoNothing => {}
            }

            // Handle the event result
            use azul_core::events::ProcessEventResult;
            match event_result {
                ProcessEventResult::ShouldIncrementalRelayout => {
                    // Restyle / runtime edit (hover/focus CSS, set_css_property,
                    // set_node_text): re-run layout on the EXISTING StyledDom instead
                    // of a full regenerate_layout(). Mirrors the macOS arm.
                    // The relayout-only request then makes generate_frame_if_needed() skip
                    // regenerate_layout() and only rebuild + send the transaction.
                    let mut debug_messages = None;
                    if let Err(e) = self.incremental_relayout_dispatching(
                        crate::desktop::shell2::common::event::IncrementalRelayout::Restyle,
                        &mut debug_messages,
                    ) {
                        log_warn!(LogCategory::Layout, "Incremental relayout failed: {}", e);
                    }
                    self.common.request_relayout_only();
                    self.request_redraw();
                }
                ProcessEventResult::ShouldRegenerateDomCurrentWindow
                | ProcessEventResult::ShouldRegenerateDomAllWindows
                | ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                    self.common
                        .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
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

    /// Handle keyboard key event with full XKB translation
    pub fn handle_key(&mut self, key: u32, state: u32) {
        use azul_core::window::OptionVirtualKeyCode;

        // Only process key press events (state == 1)
        let is_pressed = state == 1;

        // Save previous state BEFORE making changes.
        // Detect key repeat: if the key is already in pressed_virtual_keycodes,
        // clear current_virtual_keycode in the snapshot for state-diff detection.
        let mut prev_snapshot = self.common.current_window_state().clone();
        if is_pressed {
            // We can't resolve the VK here yet (need XKB), but we can check
            // pressed_scancodes. The evdev keycode maps 1:1 with scan codes.
            let scan = key;
            let already_pressed = self
                .common
                .current_window_state()
                .keyboard_state
                .pressed_scancodes
                .as_ref()
                .iter()
                .any(|s| *s == scan);
            if already_pressed {
                prev_snapshot.keyboard_state.current_virtual_keycode =
                    azul_core::window::OptionVirtualKeyCode::None;
            }
        }
        self.set_previous_window_state(prev_snapshot);

        // Phase 2: OnFocus callback (delayed) - if we receive keyboard events, we must have focus
        // Wayland doesn't have explicit focus events like X11, so we detect focus from keyboard
        // activity
        if is_pressed && !self.common.current_window_state().window_focused {
            self.common
                .update_unsynced_state(|ws| ws.window_focused = true);
            self.dynamic_selector_context.window_focused = true;
            self.sync_ime_position_to_os();
        }

        // XKB uses keycode = evdev_keycode + 8
        let xkb_keycode = key + 8;

        // Get XKB state
        let xkb_state = self.keyboard_state.state;
        if xkb_state.is_null() {
            // XKB not initialized yet - V2 input system will handle text input
            self.common.keyboard_state_mut().current_virtual_keycode = OptionVirtualKeyCode::None;
            // SANCTIONED SWALLOW: no translation possible, no event owed.
            {
                use crate::desktop::shell2::common::event::PlatformWindow as _;
                self.discard_input_delta("wayland.handle_key.xkb_null");
            }
            return;
        }

        // Get keysym (symbolic key identifier)
        let keysym = unsafe { (self.xkb.xkb_state_key_get_one_sym)(xkb_state, xkb_keycode) };

        // Translate keysym to VirtualKeyCode through the SHARED xkb table
        // (`x11::events::keysym_to_virtual_keycode`). `None` means "this keysym
        // has no virtual key" — it must stay None all the way down: inventing a
        // code here is what made every unmapped key act like Escape. Character
        // production does NOT depend on this: it comes from
        // `xkb_state_key_get_utf8` further down, so an unmapped key still types.
        let virtual_keycode: Option<azul_core::window::VirtualKeyCode> =
            events::keysym_to_virtual_keycode(keysym);

        // Client-side key repeat: arm on press of a repeatable key, disarm
        // when THAT key is released. The keymap decides what repeats — modifiers,
        // Compose, dead keys and level-switch keys are all non-repeating there,
        // and only the keymap knows which of them a given layout defines.
        {
            // The shared `Xkb` binding (one dlopen path per library — the
            // codebase convention) resolves the symbol; the fallback below
            // only covers a still-missing keymap.
            let repeats = match (
                Some(self.xkb.xkb_keymap_key_repeats),
                self.keyboard_state.keymap.is_null(),
            ) {
                (Some(key_repeats), false) => unsafe {
                    key_repeats(self.keyboard_state.keymap, xkb_keycode) != 0
                },
                // No keymap / symbol unavailable: fall back to "everything except
                // the modifiers we can name repeats". An unmapped keysym (None)
                // is not a modifier we can name, and text keys are exactly what
                // needs to repeat, so it repeats.
                _ => {
                    use azul_core::window::VirtualKeyCode as VK;
                    !matches!(
                        virtual_keycode,
                        Some(
                            VK::LShift
                                | VK::RShift
                                | VK::LControl
                                | VK::RControl
                                | VK::LAlt
                                | VK::RAlt
                                | VK::LWin
                                | VK::RWin
                                | VK::Capital
                                | VK::Numlock
                                | VK::Scroll
                        )
                    )
                }
            };
            if is_pressed && repeats {
                self.arm_key_repeat(key);
            } else if !is_pressed && self.key_repeat_keycode == Some(key) {
                self.disarm_key_repeat();
            }
        }

        // While a popup is open it holds the keyboard (the xdg_popup grab):
        // every key goes to IT, through the same pipeline a toplevel runs —
        // Escape dismisses via the engine's transient hooks (a plain menu
        // closes on Escape the same way), Return activates whatever is
        // focused, typing lands in the popup's text fields. The typed text is
        // resolved here through the parent's xkb state, since the popup has
        // none of its own.
        if self.active_popup.is_some() {
            let text = if is_pressed {
                let mut buffer = [0i8; 32];
                let len = unsafe {
                    (self.xkb.xkb_state_key_get_utf8)(
                        xkb_state,
                        xkb_keycode,
                        buffer.as_mut_ptr(),
                        buffer.len(),
                    )
                };
                if len > 0 && len < buffer.len() as i32 {
                    let raw = unsafe {
                        std::slice::from_raw_parts(buffer.as_ptr() as *const u8, len as usize)
                    };
                    std::str::from_utf8(raw)
                        .ok()
                        .filter(|t| !t.chars().all(char::is_control))
                        .map(str::to_string)
                } else {
                    None
                }
            } else {
                None
            };
            if let Some(popup) = self.active_popup.as_mut() {
                popup.key_event(key, virtual_keycode, is_pressed, text.as_deref());
            }
            // The popup may have asked to close (Escape): service it now.
            self.drive_active_popup();
            // SANCTIONED SWALLOW: the popup consumed the key.
            {
                use crate::desktop::shell2::common::event::PlatformWindow as _;
                self.discard_input_delta("wayland.handle_key.popup_route");
            }
            return;
        }

        // Update current_virtual_keycode + the pressed_virtual_keycodes /
        // pressed_scancodes lists. current_virtual_keycode MUST be cleared on
        // release: the shared diff derives VirtualKeyUp from
        // `previous.is_some() && current.is_none()`, and a leftover Some(vk) also
        // swallows the next discrete press of the SAME key (no Some → Some delta),
        // which is why Backspace/Enter/arrows only registered every other tap.
        //
        // An unmapped keysym (`virtual_keycode == None`) leaves
        // current_virtual_keycode at None — no key code is invented, so
        // `determine_all_events` emits neither KeyDown nor KeyUp for it — and it
        // adds NOTHING to `pressed_virtual_keycodes`, so there is nothing for the
        // release to fail to remove.
        //
        // The release removes the code THE PRESS RECORDED for this physical key
        // (`pressed_key_vks`), not whatever the release keysym happens to resolve
        // to. `xkb_state_key_get_one_sym` reports the EFFECTIVE keysym, so the same
        // physical key yields different keysyms depending on the modifiers held at
        // that instant: press AltGr+Q on a German layout and the keysym is XK_at
        // (→ Key2), release AltGr first and the release keysym is XK_q (→ Q). The
        // shared table folds the common shifted forms onto one code, which covers
        // Shift+digit and the keypad, but it cannot cover the level-3 layouts —
        // remembering the press is what makes press/release symmetric for every key.
        apply_key_state_change(
            self.common.keyboard_state_mut(),
            &mut self.pressed_key_vks,
            key,
            virtual_keycode,
            is_pressed,
        );

        // Compose sequences (dead keys, the Compose key) come FIRST: they are
        // defined over keysyms, and `xkb_state_key_get_utf8` below knows
        // nothing about them — it hands back the dead key's own accent
        // character, which is how `´` + `e` typed `´e` instead of `é`.
        let compose = if is_pressed {
            match self.keyboard_state.compose.as_mut() {
                Some(sequencer) => sequencer.feed(keysym),
                None => ComposeAction::Pass,
            }
        } else {
            ComposeAction::Pass
        };
        match compose {
            ComposeAction::Commit(text) => {
                if let Some(ref mut layout_window) = self.common.layout_window {
                    layout_window.record_text_input(&text);
                }
                let result = self.process_window_events(0);
                self.handle_process_event_result(result);
                return;
            }
            ComposeAction::Composing | ComposeAction::Cancelled => {
                // The key belongs to the sequence, not to the document: no
                // text, and the pass still runs so the keydown itself reaches
                // the app (a shortcut must not be swallowed by a sequence that
                // is only half typed).
                let result = self.process_window_events(0);
                self.handle_process_event_result(result);
                return;
            }
            ComposeAction::Pass => {}
        }

        // Get UTF-8 character (if printable)
        if is_pressed {
            let mut buffer = [0i8; 32];
            let len = unsafe {
                (self.xkb.xkb_state_key_get_utf8)(
                    xkb_state,
                    xkb_keycode,
                    buffer.as_mut_ptr(),
                    buffer.len(),
                )
            };

            if len > 0 && len < buffer.len() as i32 {
                let raw_bytes = unsafe {
                    std::slice::from_raw_parts(buffer.as_ptr() as *const u8, len as usize)
                };

                // Use safe UTF-8 validation — XKB should always produce valid UTF-8,
                // but a corrupt keymap could cause UB with unchecked conversion.
                if let Ok(utf8_str) = std::str::from_utf8(raw_bytes) {
                    // Don't feed CONTROL characters into text input. xkb returns a byte
                    // for keys like Backspace (0x08), Tab (0x09), Enter (0x0d), Escape
                    // (0x1b) and Delete (0x7f); recording those inserts a glyphless
                    // "tofu" rect. The edit commands themselves (delete a char / newline
                    // / etc.) are driven by the VirtualKeyCode path in
                    // process_window_events below — only PRINTABLE text belongs here.
                    let is_control_only = utf8_str.chars().all(|c| c.is_control());
                    if !utf8_str.is_empty() && !is_control_only {
                        if let Some(ref mut layout_window) = self.common.layout_window {
                            layout_window.record_text_input(utf8_str);
                        }
                    }
                }
            }
        }

        // V2: Process events through the SHARED state-diffing handler — same as the
        // pointer/motion/touch paths. The old inline match here swallowed
        // ShouldUpdateDisplayList / ShouldIncrementalRelayout in `_ => {}` and never
        // requested a redraw after a DOM regen, so typed text only became visible on the
        // next event that happened to repaint (e.g. a mouse click).
        let result = self.process_window_events(0);
        self.handle_process_event_result(result);
    }

    fn handle_process_event_result(&mut self, result: ProcessEventResult) {
        match result {
            ProcessEventResult::ShouldIncrementalRelayout => {
                // Restyle / runtime edit: re-run layout on the EXISTING StyledDom
                // instead of a full regenerate_layout() (mirrors the macOS arm).
                // generate_frame_if_needed() then takes the relayout-only path
                // (relayout-only): skip regenerate_layout, but still rebuild the
                // CPU hit-tester + build & send the full WebRender transaction + present
                // (an incremental relayout does NOT send the transaction itself).
                let mut debug_messages = None;
                if let Err(e) = self.incremental_relayout_dispatching(
                    crate::desktop::shell2::common::event::IncrementalRelayout::Restyle,
                    &mut debug_messages,
                ) {
                    log_warn!(LogCategory::Layout, "Incremental relayout failed: {}", e);
                }
                self.common.request_relayout_only();
                self.request_redraw();
            }
            ProcessEventResult::ShouldRegenerateDomCurrentWindow
            | ProcessEventResult::ShouldRegenerateDomAllWindows
            | ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                // Layout/content changed → take the FULL rebuild path:
                // generate_frame_if_needed() runs regenerate_layout + rebuilds the CPU
                // hit-tester + builds & sends the WebRender transaction + presents, but
                // only when a regeneration is pending. Calling regenerate_layout()
                // directly here does NOT build/send the transaction on Wayland, so the
                // change never reached the screen until a later redraw — that was why
                // typed text (a content change) only appeared on the next mouse click.
                //
                // RefreshDomAllWindows: ALSO mark every other registered
                // Wayland window (mirrors the X11 fan-out). Without this, a
                // popup/second-window callback mutating shared app data (e.g.
                // app-global undo) refreshed only itself; other windows kept
                // rendering the stale DOM until they got their own input.
                if result == ProcessEventResult::ShouldRegenerateDomAllWindows {
                    for wid in super::registry::get_all_window_ids() {
                        if wid == self.surface as u64 {
                            continue;
                        }
                        if let Some(wptr) = unsafe { super::registry::get_window(wid) } {
                            if let super::LinuxWindow::Wayland(w) = unsafe { &mut *wptr } {
                                w.common.request_regeneration(
                                    azul_core::callbacks::RelayoutReason::RefreshDom,
                                );
                                w.request_redraw();
                            }
                        }
                    }
                }
                self.common
                    .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
                self.request_redraw();
            }
            ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            | ProcessEventResult::ShouldReRenderCurrentWindow => {
                self.request_redraw();
            }
            ProcessEventResult::DoNothing => {}
        }

        self.sync_ime_after_input();
    }

    /// Keep the IME in step with the FOCUS and the CARET, not with DOM
    /// regeneration.
    ///
    /// `sync_text_input_v3_focus_state` / `update_ime_position_from_cursor` used
    /// to run only from `regenerate_layout_inner`. Clicking into a
    /// contenteditable normally produces a re-render, not a regeneration, so
    /// `zwp_text_input_v3.enable()` was deferred until some unrelated DOM rebuild
    /// happened to occur — CJK composition had nothing to attach to — and the
    /// cursor rectangle went stale while typing. This runs at the end of every
    /// input pass instead, after any incremental relayout above has refreshed
    /// the caret geometry.
    ///
    /// The early-out keeps it off the mouse-motion hot path, and the rectangle
    /// only goes on the wire when it actually moved: `sync_ime_position_to_os`
    /// marshals + commits + flushes on every call.
    fn sync_ime_after_input(&mut self) {
        let editing = self
            .common
            .layout_window
            .as_ref()
            .map(|lw| lw.text_edit_manager.has_active_editing())
            .unwrap_or(false);
        if !editing && !self.text_input_enabled {
            return;
        }

        let was_enabled = self.text_input_enabled;
        let old_position = self.common.current_window_state().ime_position;

        self.update_ime_position_from_cursor();
        self.sync_text_input_v3_focus_state();

        if self.text_input_enabled
            && (!was_enabled || self.common.current_window_state().ime_position != old_position)
        {
            self.sync_ime_position_to_os();
        }
    }

    /// Handle pointer motion event
    /// Merge a touch point (down/motion) into touch_state by id, then process.
    /// `x`/`y` are surface-local logical coords (wl_fixed already /256.0).
    pub fn handle_touch_point(&mut self, id: i32, x: f64, y: f64) {
        use azul_core::window::{TouchPoint, TouchPointVec};
        let pos = LogicalPosition::new(x as f32, y as f32);
        self.snapshot_window_state_baseline("wayland.handle_touch_point");
        let ts = self.common.touch_state_mut();
        let mut pts: Vec<TouchPoint> = ts.touch_points.clone().into_library_owned_vec();
        let is_new = !pts.iter().any(|p| p.id == id as u64);
        if let Some(p) = pts.iter_mut().find(|p| p.id == id as u64) {
            p.position = pos;
        } else {
            pts.push(TouchPoint {
                id: id as u64,
                position: pos,
                force: 1.0,
            });
        }
        ts.touch_points = TouchPointVec::from_vec(pts);
        ts.num_touches = ts.touch_points.len();
        // MWA-B4: per-finger gesture sessions — without them, two-finger
        // pinch/rotate were structurally undetectable (touch only filled
        // touch_state). Screen position = surface-local estimate (the
        // compositor exposes no global coordinates on Wayland).
        {
            let now = azul_core::task::Instant::from(std::time::Instant::now());
            let window_position = self.common.current_window_state().position;
            if let Some(lw) = self.common.layout_window.as_mut() {
                if is_new {
                    lw.gesture_drag_manager
                        .touch_down(id as u64, pos, now, window_position, pos);
                } else {
                    lw.gesture_drag_manager.touch_move(id as u64, pos, now, pos);
                }
            }
        }
        let result = self.process_window_events(0);
        self.handle_process_event_result(result);
    }

    /// Remove a touch point (up) by id, then process.
    pub fn handle_touch_up(&mut self, id: i32) {
        use azul_core::window::{TouchPoint, TouchPointVec};
        self.snapshot_window_state_baseline("wayland.handle_touch_up");
        let ts = self.common.touch_state_mut();
        let mut pts: Vec<TouchPoint> = ts.touch_points.clone().into_library_owned_vec();
        let last_pos = pts.iter().find(|p| p.id == id as u64).map(|p| p.position);
        pts.retain(|p| p.id != id as u64);
        ts.touch_points = TouchPointVec::from_vec(pts);
        ts.num_touches = ts.touch_points.len();
        // MWA-B4: end this finger's gesture session.
        if let Some(pos) = last_pos {
            let now = azul_core::task::Instant::from(std::time::Instant::now());
            if let Some(lw) = self.common.layout_window.as_mut() {
                lw.gesture_drag_manager.touch_up(id as u64, pos, now, pos);
            }
        }
        let result = self.process_window_events(0);
        self.handle_process_event_result(result);
    }

    /// Clear all touch points (cancel — compositor took over the sequence).
    pub fn handle_touch_cancel(&mut self) {
        use azul_core::window::TouchPointVec;
        self.snapshot_window_state_baseline("wayland.handle_touch_cancel");
        let ts = self.common.touch_state_mut();
        ts.touch_points = TouchPointVec::from_vec(Vec::new());
        ts.num_touches = 0;
        // MWA-B4: end every gesture session for the cancelled sequence.
        if let Some(lw) = self.common.layout_window.as_mut() {
            lw.gesture_drag_manager.touch_cancel_all();
        }
        // Same contract as handle_touch_point / handle_touch_up: without the pass
        // the cancel's own touch_state delta was erased by the next handler's
        // snapshot, so a compositor-stolen gesture left the app mid-drag (and the
        // repaint that routing the result brings never happened either).
        let result = self.process_window_events(0);
        self.handle_process_event_result(result);
    }

    /// Feed the accumulated tablet PAD state (ExpressKeys + ring/strip).
    ///
    /// The pad is not a pointer: it never moves the cursor and has no surface
    /// coordinates, so it does not go through the pointer path at all. It is
    /// published straight into the gesture manager, where
    /// `CallbackInfo::get_wacom_pad` reads it — that accessor existed and
    /// returned `None` on every platform until this producer.
    /// (Re)build the published tablet-device list from the accumulated
    /// `zwp_tablet_v2` descriptive state and hand it to the gesture manager
    /// (`CallbackInfo::get_tablet_devices`). Called from the `done` /
    /// `removed` listeners and again after the layout window is created —
    /// the descriptive burst usually arrives during the initial roundtrips,
    /// before any layout window exists to hold the result.
    pub(super) fn sync_tablet_devices(&mut self) {
        use azul_layout::managers::gesture as gest;
        let mut devices = Vec::new();
        let composite_id =
            ((self.tablet_info.vendor_id as u64) << 32) | self.tablet_info.product_id as u64;
        for stat in self.tablet_tools.values() {
            devices.push(gest::TabletDeviceInfo {
                name: self.tablet_info.name.clone().into(),
                vendor_name: gest::tablet_usb_vendor_name(self.tablet_info.vendor_id).into(),
                vendor_id: self.tablet_info.vendor_id,
                product_id: self.tablet_info.product_id,
                kind: if stat.is_eraser {
                    gest::TabletToolKind::Eraser
                } else {
                    gest::TabletToolKind::Stylus
                },
                // Matches what the pen bridge reports as PenState.device_id.
                device_id: composite_id,
                capabilities: stat.capabilities,
                // zwp_tablet_tool_v2.pressure is always 0..=65535 on the wire.
                pressure_max: if stat.capabilities & gest::TABLET_CAP_PRESSURE != 0 {
                    65535.0
                } else {
                    0.0
                },
                // Wayland reports no physical size; 0 = unknown, not a guess.
                physical_width_mm: 0.0,
                physical_height_mm: 0.0,
                num_buttons: 0,
                path: self.tablet_info.path.clone().into(),
            });
        }
        if let Some(pad) = self.tablet_pad_static {
            devices.push(gest::TabletDeviceInfo {
                name: self.tablet_info.name.clone().into(),
                vendor_name: gest::tablet_usb_vendor_name(self.tablet_info.vendor_id).into(),
                vendor_id: self.tablet_info.vendor_id,
                product_id: self.tablet_info.product_id,
                kind: gest::TabletToolKind::Pad,
                device_id: composite_id,
                capabilities: if pad.has_ring || pad.has_strip {
                    gest::TABLET_CAP_TOUCHRING
                } else {
                    0
                },
                pressure_max: 0.0,
                physical_width_mm: 0.0,
                physical_height_mm: 0.0,
                num_buttons: pad.buttons,
                path: self.tablet_info.path.clone().into(),
            });
        }
        if let Some(lw) = self.common.layout_window.as_mut() {
            lw.gesture_drag_manager.set_tablet_devices(devices);
        }
    }

    pub fn handle_tablet_pad_frame(&mut self) {
        let pad = self.tablet_pad;
        let device_id =
            ((self.tablet_info.vendor_id as u64) << 32) | self.tablet_info.product_id as u64;
        self.snapshot_window_state_baseline("wayland.handle_tablet_pad_frame");
        if let Some(lw) = self.common.layout_window.as_mut() {
            lw.gesture_drag_manager.update_pad_state(
                azul_layout::managers::gesture::WacomPadState {
                    express_keys: pad.express_keys,
                    touch_ring: pad.touch_ring,
                    touch_ring_active: pad.touch_ring_active,
                    device_id,
                },
            );
        }
        let result = self.process_window_events(0);
        self.handle_process_event_result(result);
    }

    /// Feed the accumulated tablet pen state on the tool's `frame` event —
    /// and drive the POINTER pipeline from it.
    ///
    /// The second half is not optional: once a client binds tablet-v2, the
    /// compositor routes a tool in proximity through these events INSTEAD of
    /// `wl_pointer`. Without the bridge the cursor froze, nothing hit-tested
    /// and no Mouse* callback ever fired while drawing — the pen updated
    /// `get_pen_state()` and did nothing else. X11 never had the problem
    /// because the pen rides the master pointer there; this bridge is what
    /// makes the two backends agree.
    ///
    /// Mapping (W3C PointerEvent, pointerType "pen"): tip contact = LEFT
    /// button, either barrel button = RIGHT button, tool position = pointer
    /// position. Deliberate v1 limits: no scrollbar / CSD-resize-edge
    /// interaction and no popup routing from the pen — those stay
    /// pointer-only until a pen needs them.
    pub fn handle_tablet_frame(&mut self) {
        let p = self.tablet_pen;
        self.snapshot_window_state_baseline("wayland.handle_tablet_frame");

        if !p.in_proximity {
            // proximity_out: the tool is gone. Clear the engine pen state
            // (the Some→None diff fires PenLeave) and release the
            // synthesized buttons so nothing stays latched down.
            if let Some(lw) = self.common.layout_window.as_mut() {
                lw.gesture_drag_manager.clear_pen_state();
            }
            let ms = self.common.mouse_state_mut();
            ms.left_down = false;
            ms.right_down = false;
            let result = self.process_window_events(0);
            self.handle_process_event_result(result);
            return;
        }

        let (was_left, was_right) = {
            let ms = &self.common.current_window_state().mouse_state;
            (ms.left_down, ms.right_down)
        };
        let now_left = p.in_contact;
        let now_right = p.barrel_button;

        // 1) Full-fidelity pen state for CallbackInfo::get_pen_state().
        if let Some(lw) = self.common.layout_window.as_mut() {
            lw.gesture_drag_manager.update_pen_state_full(
                p.position,
                p.pressure,
                (p.tilt_x, p.tilt_y),
                p.in_contact,
                p.is_eraser,
                p.barrel_button,
                // Device identity = the TABLET's USB vid/pid; the per-tool
                // hardware serial goes in tool_id (truncated — Wintab tool
                // ids are 32-bit as well).
                ((self.tablet_info.vendor_id as u64) << 32)
                    | self.tablet_info.product_id as u64,
                p.tangential,
                p.rotation,
                p.tool_id as u32,
            );
        }

        // 2) The pointer bridge: position, hover hit-test, button edges.
        self.common.mouse_state_mut().cursor_position = CursorPosition::InWindow(p.position);
        self.common.mouse_state_mut().left_down = now_left;
        self.common.mouse_state_mut().right_down = now_right;
        self.update_hit_test(p.position);

        let button_state = if now_left {
            BUTTON_STATE_LEFT
        } else {
            BUTTON_STATE_NONE
        } | if now_right {
            BUTTON_STATE_RIGHT
        } else {
            BUTTON_STATE_NONE
        };
        let any_down_edge = (!was_left && now_left) || (!was_right && now_right);
        let any_up_edge = (was_left && !now_left) || (was_right && !now_right);
        self.record_input_sample(p.position, button_state, any_down_edge, any_up_edge, None);

        // Barrel press over a node with a context menu behaves like a right
        // click (parity with handle_pointer_button).
        if now_right && !was_right {
            if let Some(hit_node) = self.get_first_hovered_node() {
                if self.try_show_context_menu(hit_node, p.position) {
                    self.request_redraw();
                }
            }
        }

        let result = self.process_window_events(0);
        // The pen-up that ends a selection gesture claims PRIMARY, exactly
        // like a left-button release does on the pointer path.
        if was_left && !now_left {
            self.publish_primary_selection();
        }
        self.handle_process_event_result(result);
    }

    pub fn handle_pointer_motion(&mut self, x: f64, y: f64) {
        let logical_pos = LogicalPosition::new(x as f32, y as f32);

        // While the pointer is over an open menu popup, forward motion to the
        // popup (just tracks the popup-relative cursor for a later click/Return)
        // and don't touch the parent's hover/hit-test state.
        if self.pointer_over_popup && self.active_popup.is_some() {
            if let Some(popup) = self.active_popup.as_mut() {
                popup.pointer_motion(logical_pos);
            }
            return;
        }

        // Save previous state BEFORE making changes
        self.snapshot_window_state_baseline("wayland.handle_pointer_motion");

        self.common.mouse_state_mut().cursor_position = CursorPosition::InWindow(logical_pos);

        // Handle scrollbar dragging if active
        if self.common.scrollbar_drag_state.is_some() {
            use crate::desktop::shell2::common::event::PlatformWindow;
            let result = PlatformWindow::handle_scrollbar_drag(self, logical_pos);
            // Route like every other pointer path: a scroll callback can restyle
            // (ShouldIncrementalRelayout → incremental fast path) or rebuild the DOM
            // (ShouldRegenerateDom* → request_regeneration). DoNothing stays a
            // no-op and the redraw-only variants still request_redraw, so plain
            // scrollbar drags behave exactly as before.
            self.handle_process_event_result(result);
            // SANCTIONED SWALLOW: the thumb drag consumed this motion; the
            // cursor delta must not surface as a MouseMove event later.
            use crate::desktop::shell2::common::event::PlatformWindow as _;
            self.discard_input_delta("wayland.pointer_motion.scrollbar_drag");
            return;
        }

        // Record input sample for gesture detection (movement during button press)
        let button_state = if self.common.current_window_state().mouse_state.left_down {
            BUTTON_STATE_LEFT
        } else {
            BUTTON_STATE_NONE
        } | if self.common.current_window_state().mouse_state.right_down {
            BUTTON_STATE_RIGHT
        } else {
            BUTTON_STATE_NONE
        } | if self.common.current_window_state().mouse_state.middle_down {
            BUTTON_STATE_MIDDLE
        } else {
            BUTTON_STATE_NONE
        };
        self.record_input_sample(logical_pos, button_state, false, false, None);

        // Update hit test for hover effects
        self.update_hit_test(logical_pos);

        // Update cursor based on CSS cursor properties
        // This is done BEFORE callbacks so callbacks can override the cursor
        if let Some(layout_window) = self.common.layout_window.as_ref() {
            if let Some(hit_test) = layout_window
                .hover_manager
                .get_current(&azul_layout::managers::hover::InputPointId::Mouse)
            {
                let cursor_test = layout_window.compute_cursor_type_hit_test(hit_test);
                // Update the window state cursor type
                self.common.mouse_state_mut().mouse_cursor_type =
                    Some(cursor_test.cursor_icon).into();
                // Set the actual OS cursor
                self.set_cursor(cursor_test.cursor_icon);
            }
        }

        // V2: Process events through state-diffing system
        let result = self.process_window_events(0);
        self.handle_process_event_result(result);
    }

    /// Handle pointer button event
    pub fn handle_pointer_button(&mut self, serial: u32, button: u32, state: u32) {
        self.pointer_state.serial = serial;
        self.last_input_serial = serial; // MWA-B3: valid serial for set_selection

        // While the pointer is over an open menu popup, route the click to the
        // popup's layout (the xdg_popup grab delivers it through this parent's
        // seat). A left-press over a menu item fires its callback; the menu then
        // closes (menus dismiss on selection).
        if self.pointer_over_popup && self.active_popup.is_some() {
            crate::plog_info!(
                "[wayland-popup] pointer button (btn={:#x} state={}) -> routing to popup",
                button,
                state
            );
            if let Some(popup) = self.active_popup.as_mut() {
                popup.pointer_button(button, state);
            }
            return;
        }

        let mouse_button = match button {
            0x110 => MouseButton::Left,   // BTN_LEFT
            0x111 => MouseButton::Right,  // BTN_RIGHT
            0x112 => MouseButton::Middle, // BTN_MIDDLE
            _ => return,
        };

        let is_down = state == 1;
        let position = match self
            .common
            .current_window_state()
            .mouse_state
            .cursor_position
        {
            CursorPosition::InWindow(pos) => pos,
            _ => LogicalPosition::zero(),
        };

        // Save previous state BEFORE making changes
        self.snapshot_window_state_baseline("wayland.handle_pointer_button");

        // MWA-B11: CSD resize edges — frameless windows previously had NO
        // way to resize. A press in the border band hands the resize to the
        // compositor (xdg_toplevel.resize); edge codes per xdg-shell.
        if is_down
            && button == 0x110 // BTN_LEFT
            && self.common.current_window_state().flags.decorations
                == azul_core::window::WindowDecorations::None
        {
            use crate::desktop::shell2::common::event::{
                csd_resize_edge_at, CsdResizeEdge, CSD_RESIZE_BAND_PX,
            };
            let size = self.common.current_window_state().size.dimensions;
            if let Some(edge) = csd_resize_edge_at(position, size, CSD_RESIZE_BAND_PX) {
                let edges: u32 = match edge {
                    CsdResizeEdge::Top => 1,
                    CsdResizeEdge::Bottom => 2,
                    CsdResizeEdge::Left => 4,
                    CsdResizeEdge::TopLeft => 5,
                    CsdResizeEdge::BottomLeft => 6,
                    CsdResizeEdge::Right => 8,
                    CsdResizeEdge::TopRight => 9,
                    CsdResizeEdge::BottomRight => 10,
                };
                if !self.xdg_toplevel.is_null() && !self.seat.is_null() && serial != 0 {
                    unsafe {
                        (self.wayland.xdg_toplevel_resize)(
                            self.xdg_toplevel,
                            self.seat,
                            serial,
                            edges,
                        );
                    }
                    return;
                }
            }
        }

        // Check for scrollbar hit FIRST (before state changes)
        if is_down {
            use crate::desktop::shell2::common::event::PlatformWindow;
            if let Some(scrollbar_hit_id) =
                PlatformWindow::perform_scrollbar_hit_test(self, position)
            {
                let result =
                    PlatformWindow::handle_scrollbar_click(self, scrollbar_hit_id, position);
                // Route like every other pointer path (see handle_pointer_motion): a
                // scroll callback can restyle / rebuild the DOM. DoNothing stays a
                // no-op; the other variants still request_redraw.
                self.handle_process_event_result(result);
                return;
            }

            // Check for context menu (right-click).
            // MWA-C-hover: read the LIVE hover manager (X11 pattern) —
            // common.last_hovered_node has no writer anywhere, so hit-node
            // context menus never opened on Wayland.
            if mouse_button == MouseButton::Right {
                if let Some(hit_node) = self.get_first_hovered_node() {
                    if self.try_show_context_menu(hit_node, position) {
                        // Context menu was shown, consume the event
                        self.request_redraw();
                        return;
                    }
                }
            }
        } else {
            // End scrollbar drag if active
            if self.common.scrollbar_drag_state.is_some() {
                self.common.scrollbar_drag_state = None;
                self.request_redraw();
                return;
            }
        }

        // Only the button that actually changed may be written. Assigning all
        // three from `mouse_button == …` cleared the OTHER buttons, so pressing
        // Right while Left was held made the state diff synthesize a phantom
        // LeftMouseUp — drags and text selections died mid-gesture.
        set_mouse_button_down(self.common.mouse_state_mut(), mouse_button, is_down);
        self.pointer_state.button_down = if is_down { Some(mouse_button) } else { None };

        // Record input sample for gesture detection
        let button_state = match mouse_button {
            MouseButton::Left => BUTTON_STATE_LEFT,
            MouseButton::Right => BUTTON_STATE_RIGHT,
            MouseButton::Middle => BUTTON_STATE_MIDDLE,
            _ => BUTTON_STATE_NONE,
        };
        self.record_input_sample(position, button_state, is_down, !is_down, None);

        // Middle-click paste: the primary selection is inserted at the caret,
        // which the button-2 PRESS already moved to the click point. Recorded
        // BEFORE the pass so the changeset is applied by it, exactly like typed
        // text — the same shape as `x11/events.rs`, which has had this for
        // years while Wayland had neither half of the idiom.
        if primary_paste_wanted(
            mouse_button,
            is_down,
            self.common
                .layout_window
                .as_ref()
                .is_some_and(|lw| lw.text_edit_manager.has_active_editing()),
        ) {
            if let Some(text) = clipboard::get_primary_content() {
                if !text.is_empty() {
                    if let Some(ref mut layout_window) = self.common.layout_window {
                        layout_window.record_text_input(&text);
                    }
                }
            }
        }

        // V2: Process events through state-diffing system
        let result = self.process_window_events(0);

        // The release that ends a selection gesture claims the primary
        // selection (run AFTER the pass, which is what finalizes the
        // selection).
        if !is_down && mouse_button == MouseButton::Left {
            self.publish_primary_selection();
        }

        self.handle_process_event_result(result);
    }

    /// Claim the Wayland primary selection for the current text selection.
    ///
    /// On Wayland as on X11, *selecting* text is itself a primary-selection
    /// claim — no copy involved — and middle-click pastes it.
    fn publish_primary_selection(&mut self) {
        let text = {
            let Some(lw) = self.common.layout_window.as_ref() else {
                return;
            };
            if !lw.text_edit_manager.has_active_editing() {
                return;
            }
            let dom_id = lw
                .text_edit_manager
                .get_editing_dom_id()
                .unwrap_or(azul_core::dom::DomId { inner: 0 });
            match lw.get_selected_content_for_clipboard(&dom_id) {
                Some(content) => content.plain_text.as_str().to_string(),
                None => return,
            }
        };
        if text.is_empty() {
            return;
        }
        let _ = clipboard::write_to_primary(&text);
    }

    /// Accumulate one `wl_pointer.axis` event into the current pointer frame.
    ///
    /// Nothing is dispatched here — `handle_pointer_frame` flushes the frame as a
    /// single scroll. See [`Self::pending_axis_value`].
    pub fn handle_pointer_axis(&mut self, axis: u32, value: f64) {
        // MWA-B13: wl_pointer.axis is POSITIVE toward bottom/right (the
        // "natural" content direction), but azul's raw-delta chokepoint uses
        // the X11 convention (button 4 / up = +1, button 5 / down = −1) —
        // ScrollManager's scroll_sign() normalizes from THAT. Passing the wl
        // value through unsigned inverted every wheel/trackpad scroll on
        // Wayland. NEEDS-RUNTIME-VERIFY: direction on a real compositor
        // (with and without natural-scroll enabled).
        match axis {
            WL_POINTER_AXIS_HORIZONTAL_SCROLL => {
                self.pending_axis_value.0 -= value as f32;
            }
            WL_POINTER_AXIS_VERTICAL_SCROLL => {
                self.pending_axis_value.1 -= value as f32;
            }
            _ => return,
        }
        self.pending_axis = true;
        // No wl_pointer.frame on a pre-v5 seat — nothing would ever flush this.
        if self.seat_version < WL_POINTER_FRAME_SINCE_VERSION {
            self.flush_pending_axis();
        }
    }

    /// Accumulate one `wl_pointer.axis_discrete` (detent count) into the frame.
    pub fn handle_pointer_axis_discrete(&mut self, axis: u32, discrete: i32) {
        match axis {
            WL_POINTER_AXIS_HORIZONTAL_SCROLL => {
                self.pending_axis_discrete.0 -= discrete as f32;
            }
            WL_POINTER_AXIS_VERTICAL_SCROLL => {
                self.pending_axis_discrete.1 -= discrete as f32;
            }
            _ => return,
        }
        self.pending_axis = true;
    }

    /// `wl_pointer.frame` — the frame is complete. Flush its accumulated axis
    /// input and drop the frame-scoped axis source.
    pub fn handle_pointer_frame(&mut self) {
        self.flush_pending_axis();
        self.current_axis_source = WL_AXIS_SOURCE_WHEEL;
    }

    /// Dispatch the axis input accumulated in the current pointer frame as ONE
    /// scroll, then run a single event pass.
    fn flush_pending_axis(&mut self) {
        if !self.pending_axis {
            return;
        }
        self.pending_axis = false;
        let (raw_x, raw_y) = std::mem::replace(&mut self.pending_axis_value, (0.0, 0.0));
        let (disc_x, disc_y) = std::mem::replace(&mut self.pending_axis_discrete, (0.0, 0.0));

        let is_trackpad = axis_source_is_trackpad(self.current_axis_source);
        let (delta_x, delta_y) = axis_frame_delta(is_trackpad, (raw_x, raw_y), (disc_x, disc_y));

        if delta_x == 0.0 && delta_y == 0.0 {
            return;
        }

        // Save previous state BEFORE making changes
        self.snapshot_window_state_baseline("wayland.flush_pending_axis");

        // The scroll target is whatever is under the cursor NOW. Reusing the
        // hover manager's last hit test meant a stationary cursor over content
        // that had scrolled/relaid-out beneath it kept scrolling the node that
        // used to be there (X11 re-runs the hit test for exactly this reason).
        let hover_pos = match self
            .common
            .current_window_state()
            .mouse_state
            .cursor_position
        {
            CursorPosition::InWindow(pos) => Some(pos),
            _ => None,
        };
        if let Some(pos) = hover_pos {
            self.update_hit_test(pos);
        }

        // Queue scroll input for the physics timer instead of directly setting offsets.
        {
            let mut should_start_timer = false;
            let mut input_queue_clone = None;

            if let Some(ref mut layout_window) = self.common.layout_window {
                use azul_core::task::Instant;
                use azul_layout::managers::scroll_state::ScrollInputSource;

                let now = Instant::from(std::time::Instant::now());

                // MWA-C-scroll: classify by wl_pointer.axis_source — finger
                // (touchpad) and continuous (e.g. trackpoint with kinetic
                // scrolling) deltas are position deltas, not wheel ticks;
                // treating them as WheelDiscrete stacked velocity impulses
                // and made touchpad scrolling fly. axis_stop → TrackpadEnd.
                let (source, device) = if is_trackpad {
                    (
                        ScrollInputSource::TrackpadContinuous,
                        azul_layout::managers::scroll_state::ScrollInputDevice::Touchpad,
                    )
                } else {
                    (
                        ScrollInputSource::WheelDiscrete,
                        azul_layout::managers::scroll_state::ScrollInputDevice::MouseWheel,
                    )
                };

                azul_layout::scroll_timer::trace_scroll_input(
                    "wayland",
                    delta_x,
                    delta_y,
                    is_trackpad,
                    if is_trackpad {
                        "TrackpadContinuous"
                    } else {
                        "WheelDiscrete"
                    },
                    if is_trackpad {
                        "Touchpad"
                    } else {
                        "MouseWheel"
                    },
                );

                if let Some((_dom_id, _node_id, start_timer)) =
                    layout_window.scroll_manager.record_scroll_from_hit_test(
                        // Raw delta; sign applied centrally (natural-scroll flag).
                        delta_x,
                        delta_y,
                        source,
                        device,
                        &layout_window.hover_manager,
                        &InputPointId::Mouse,
                        now,
                    )
                {
                    // GUARD: `start_timer` only means "the input queue was drained
                    // when this event arrived", which the 16 ms physics tick
                    // makes true for almost every event of a gesture. Without
                    // also checking that the timer is not already registered,
                    // `start_timer` below REPLACED the live `ScrollPhysicsState`
                    // — throwing away velocity, animate targets and pending
                    // positions mid-gesture, and resetting the tick phase. The
                    // shared arming site in `common/event.rs` has always had
                    // this check.
                    should_start_timer = start_timer
                        && !layout_window
                            .timers
                            .contains_key(&azul_core::task::SCROLL_MOMENTUM_TIMER_ID);
                    if start_timer {
                        input_queue_clone = Some(layout_window.scroll_manager.get_input_queue());
                    }
                }
            }

            // Start the scroll momentum timer if this is the first input
            if should_start_timer {
                if let Some(queue) = input_queue_clone {
                    use azul_core::refany::RefAny;
                    use azul_core::task::Duration;
                    use azul_core::task::SCROLL_MOMENTUM_TIMER_ID;
                    use azul_layout::scroll_timer::{
                        scroll_physics_timer_callback, ScrollPhysicsState,
                    };
                    use azul_layout::timer::{Timer, TimerCallbackType};

                    let physics_state = ScrollPhysicsState::new(
                        queue,
                        self.common.system_style.scroll_physics.clone(),
                    );
                    let interval_ms = self.common.system_style.scroll_physics.timer_interval_ms;
                    let data = RefAny::new(physics_state);
                    let timer = Timer::create(
                        data,
                        scroll_physics_timer_callback as TimerCallbackType,
                        azul_layout::callbacks::ExternalSystemCallbacks::rust_internal()
                            .get_system_time_fn,
                    )
                    .with_interval(Duration::System(
                        azul_core::task::SystemTimeDiff::from_millis(interval_ms as u64),
                    ));

                    self.start_timer(SCROLL_MOMENTUM_TIMER_ID.id, timer);
                }
            }
        }

        // V2: Process events through state-diffing system
        let result = self.process_window_events(0);
        self.handle_process_event_result(result);
    }

    /// MWA-C-scroll: wl_pointer.axis_stop — fingers lifted from the
    /// touchpad. Emits a zero-delta `TrackpadEnd` at the current scroll
    /// target so the physics timer runs its rubber-band spring-back (the
    /// same signal macOS derives from NSEventPhase::Ended). Without it an
    /// overshot Wayland touchpad scroll stayed stuck past the boundary.
    pub fn handle_pointer_axis_stop(&mut self) {
        use azul_core::task::Instant;
        use azul_layout::managers::hover::InputPointId;
        use azul_layout::managers::scroll_state::ScrollInputSource;

        // axis_stop rides in the same frame as any axis events that preceded it,
        // and TrackpadEnd is only meaningful AFTER them.
        self.flush_pending_axis();

        if self.current_axis_source != WL_AXIS_SOURCE_FINGER
            && self.current_axis_source != WL_AXIS_SOURCE_CONTINUOUS
        {
            // Wheel sources get axis_stop too on some compositors; spring-back
            // only applies to trackpad-style rubber-banding.
            return;
        }

        if let Some(ref mut layout_window) = self.common.layout_window {
            let now = Instant::from(std::time::Instant::now());
            layout_window.scroll_manager.record_scroll_from_hit_test(
                0.0,
                0.0,
                ScrollInputSource::TrackpadEnd,
                azul_layout::managers::scroll_state::ScrollInputDevice::Touchpad,
                &layout_window.hover_manager,
                &InputPointId::Mouse,
                now,
            );
            // No timer start here: a TrackpadEnd only matters after
            // TrackpadContinuous inputs, which already started the physics
            // timer; if it somehow expired, there is no overshoot to spring
            // back from either.
        }
    }

    /// Handle pointer enter event.
    ///
    /// `over_popup` is `true` when the entered `wl_surface` (carried by
    /// `wl_pointer.enter`, compared against the popup's surface in the listener)
    /// is the active menu popup's surface. When a menu popup is open, the
    /// xdg_popup grab routes pointer events through this parent's seat listeners
    /// regardless of which surface they target; this flag tells us whether to
    /// forward this (and subsequent, surface-less motion/button) events to the
    /// popup's own layout instead of the parent's.
    pub fn handle_pointer_enter(&mut self, serial: u32, x: f64, y: f64, over_popup: bool) {
        self.pointer_state.serial = serial;
        let logical_pos = LogicalPosition::new(x as f32, y as f32);

        // Route to the active popup if the pointer entered ITS surface.
        let over_popup = over_popup && self.active_popup.is_some();
        self.pointer_over_popup = over_popup;
        if over_popup {
            crate::plog_info!(
                "[wayland-popup] pointer entered popup surface at ({:.1},{:.1})",
                x,
                y
            );
            if let Some(popup) = self.active_popup.as_mut() {
                popup.pointer_enter(logical_pos);
            }
            return;
        }

        // MWA-C-hover: save previous state + run the event pass so
        // MouseEnter callbacks and :hover styling fire on the entry itself
        // instead of on the first subsequent motion event.
        self.snapshot_window_state_baseline("wayland.handle_pointer_enter");
        self.common.mouse_state_mut().cursor_position = CursorPosition::InWindow(logical_pos);
        self.update_hit_test(logical_pos);
        let result = self.process_window_events(0);
        self.handle_process_event_result(result);
        self.request_redraw();
    }

    /// Handle keyboard leave event (window lost focus)
    pub fn handle_keyboard_leave(&mut self) {
        // Focus is gone — the compositor will not send the key release.
        self.disarm_key_repeat();
        self.snapshot_window_state_baseline("wayland.handle_keyboard_leave");
        self.common.update_unsynced_state(|ws| {
            ws.window_focused = false;
            // Release the mouse buttons: focus left while a button was down, so the
            // OS delivers the mouse-UP elsewhere and `left_down` would stay true
            // forever — every later move reads as a DRAG (text selects, buttons stop
            // clicking). Clearing the flags lets the normal state diff emit the
            // MouseUp that unwinds it. See macos::window_did_resign_key.
            ws.mouse_state.left_down = false;
            ws.mouse_state.right_down = false;
            ws.mouse_state.middle_down = false;
            // Drop every held KEY for the same reason as the buttons above: the
            // key-UP of whatever caused the focus change goes to the app that
            // took focus. On macOS that is Cmd of Cmd-Tab, which then stays
            // latched and turns every later keystroke into a shortcut. Windows
            // has done this since it was written; the other three never did.
            ws.keyboard_state.current_virtual_keycode =
                azul_core::window::OptionVirtualKeyCode::None;
            ws.keyboard_state.pressed_virtual_keycodes =
                azul_core::window::VirtualKeyCodeVec::from_vec(Vec::new());
            ws.keyboard_state.pressed_scancodes =
                azul_core::window::ScanCodeVec::from_vec(Vec::new());
        });
        self.dynamic_selector_context.window_focused = false;
        // Every held key is released somewhere we will never hear about, so drop
        // them all now. Engine modifiers are DERIVED from pressed_virtual_keycodes
        // (core/src/window.rs, KeyboardState::*_down) — leaving Ctrl in the list
        // made the whole app behave as if Ctrl were held forever after an
        // Alt-Tab away with a modifier down.
        self.common.keyboard_state_mut().pressed_virtual_keycodes =
            azul_core::window::VirtualKeyCodeVec::new();
        self.common.keyboard_state_mut().pressed_scancodes = azul_core::window::ScanCodeVec::new();
        self.common.keyboard_state_mut().current_virtual_keycode =
            azul_core::window::OptionVirtualKeyCode::None;
        // The press→code record must die with the list it mirrors, or a key held
        // across the focus change keeps an entry that a much later release of the
        // same physical key would use to remove a code nobody pressed.
        self.pressed_key_vks.clear();
        // A half-typed compose sequence dies with the focus too: leaving it
        // armed makes the FIRST keystroke after coming back complete a
        // sequence the user started in another window.
        if let Some(compose) = self.keyboard_state.compose.as_mut() {
            compose.reset();
        }
        // MWA-A3b: forward to AT-SPI — accesskit_unix never learns window
        // focus on its own (Orca got no focus events on Wayland).
        #[cfg(feature = "a11y")]
        self.accessibility_adapter.set_focus(false);
        // Run the state-diff pass so WindowFocusLost callbacks fire and
        // focus-conditional styling restyles (the bare snapshot alone was
        // overwritten by the next event's snapshot before anything diffed it).
        let result = self.process_window_events(0);
        self.handle_process_event_result(result);
        self.request_redraw();
    }

    /// Raw wayland connection fd (for the multi-window run-loop poll).
    #[must_use]
    pub fn display_fd(&self) -> i32 {
        unsafe { (self.wayland.wl_display_get_fd)(self.display) }
    }

    /// Integer buffer scale for this window (dpi is maintained by the
    /// wl_output enter/leave handlers; 96 → 1, 192 → 2, …).
    fn buffer_scale(&self) -> i32 {
        (self.common.current_window_state().size.dpi as f32 / 96.0)
            .round()
            .max(1.0) as i32
    }

    /// `true` when fractional viewport scaling drives presentation: the
    /// compositor sent a `preferred_scale` AND we have a wp_viewport to map
    /// the physical buffer onto the logical surface. In this mode
    /// `set_buffer_scale` must NOT be called (buffer scale stays 1) and the
    /// present path calls `wp_viewport.set_destination(logical_w, logical_h)`.
    fn fractional_scale_active(&self) -> bool {
        self.viewport.is_some() && self.preferred_scale_120.is_some()
    }

    /// (physical_width, physical_height, buffer_scale) for the CPU shm
    /// buffers at the given LOGICAL size.
    ///
    /// - Fractional path: physical = ceil(logical × dpi/96) — the exact size
    ///   `CpuBackend::render_frame` produces — with buffer scale 1 (the
    ///   viewport maps it back to logical).
    /// - Integer path: physical = logical × round(dpi/96), buffer scale =
    ///   round(dpi/96) (announced via set_buffer_scale at attach).
    fn cpu_buffer_spec(&self, logical_w: i32, logical_h: i32) -> (i32, i32, i32) {
        if self.fractional_scale_active() {
            let d = (self.common.current_window_state().size.dpi as f32 / 96.0).max(0.01);
            (
                ((logical_w.max(1) as f32) * d).ceil() as i32,
                ((logical_h.max(1) as f32) * d).ceil() as i32,
                1,
            )
        } else {
            let s = self.buffer_scale();
            (logical_w.max(1) * s, logical_h.max(1) * s, s)
        }
    }

    /// Arm the key-repeat timer for `keycode` (delay, then interval).
    fn arm_key_repeat(&mut self, keycode: u32) {
        if self.key_repeat_fd < 0 || self.key_repeat_rate_ms == 0 {
            return;
        }
        // Already armed for THIS key: leave the timer alone and let
        // it_interval drive. The repeat replay goes through handle_key(state=1),
        // which lands back here — re-arming would reset it_value (the initial
        // delay) on every replayed press, so the timer never got past its
        // first period and "repeat" fired at the DELAY cadence (~600 ms per
        // character on KDE defaults) instead of the advertised rate. A real
        // re-press of the same key always passes through a release first,
        // which disarms (key_repeat_keycode = None), so it still re-arms with
        // the full initial delay; a different key held re-arms below.
        if self.key_repeat_keycode == Some(keycode) {
            return;
        }
        self.key_repeat_keycode = Some(keycode);
        let delay = self.key_repeat_delay_ms.max(1) as i64;
        let interval = self.key_repeat_rate_ms.max(1) as i64;
        // tv_sec/tv_nsec are i32 on 32-bit targets (i686, armv7) — cast via
        // the libc typedefs; the ms-derived values always fit.
        let spec = libc::itimerspec {
            it_value: libc::timespec {
                tv_sec: (delay / 1000) as libc::time_t,
                tv_nsec: ((delay % 1000) * 1_000_000) as libc::c_long,
            },
            it_interval: libc::timespec {
                tv_sec: (interval / 1000) as libc::time_t,
                tv_nsec: ((interval % 1000) * 1_000_000) as libc::c_long,
            },
        };
        unsafe {
            libc::timerfd_settime(self.key_repeat_fd, 0, &spec, std::ptr::null_mut());
        }
    }

    /// Stop key repeat (key released / keyboard focus lost).
    fn disarm_key_repeat(&mut self) {
        self.key_repeat_keycode = None;
        if self.key_repeat_fd < 0 {
            return;
        }
        let spec: libc::itimerspec = unsafe { std::mem::zeroed() };
        unsafe {
            libc::timerfd_settime(self.key_repeat_fd, 0, &spec, std::ptr::null_mut());
        }
    }

    /// Handle `wl_keyboard.enter` — the compositor gave this surface keyboard
    /// focus. This was a stub: `window_focused` only ever became true after
    /// the first KEYPRESS (handle_key inferred it), so click-to-focus alone
    /// left the window styled/behaving as unfocused, and WindowFocusReceived
    /// callbacks never fired on Wayland.
    // --- Native Wayland clipboard (MWA-B3) ---

    /// Take clipboard ownership: create a `wl_data_source` offering the
    /// plain-text mime spellings and set it as the seat selection with the
    /// last input serial. Returns `false` when prerequisites are missing
    /// (no data device, no input serial yet) so the caller can fall back to
    /// the XWayland path. The text itself is parked in
    /// `clipboard::NATIVE_COPY`; the compositor pulls it through
    /// `events::data_source_send`.
    pub(super) fn wayland_set_selection(&mut self) -> bool {
        if self.data_device_manager.is_null() || self.data_device.is_null() {
            return false;
        }
        if self.last_input_serial == 0 {
            return false;
        }
        unsafe {
            // Destroy any previous outgoing source. destroy: opcode 1, "".
            if !self.clipboard_source.is_null() {
                // BOTH halves, like every destructor (see dlopen.rs
                // destroy_proxy_after_request): the request tells the server,
                // wl_proxy_destroy frees the client id. This site sent only
                // the request — one leaked proxy per clipboard copy.
                let destroy: unsafe extern "C" fn(*mut defines::wl_proxy, u32) =
                    std::mem::transmute(self.wayland.wl_proxy_marshal);
                destroy(self.clipboard_source as *mut defines::wl_proxy, 1);
                (self.wayland.wl_proxy_destroy)(self.clipboard_source as *mut defines::wl_proxy);
                self.clipboard_source = std::ptr::null_mut();
            }

            // create_data_source: opcode 0 on wl_data_device_manager, "n".
            type CreateSrcCtor = unsafe extern "C" fn(
                *mut defines::wl_proxy,
                u32,
                *const defines::wl_interface,
                *mut std::ffi::c_void,
            ) -> *mut defines::wl_proxy;
            let ctor: CreateSrcCtor =
                std::mem::transmute(self.wayland.wl_proxy_marshal_constructor);
            let src = ctor(
                self.data_device_manager as *mut defines::wl_proxy,
                0,
                defines::get_wl_data_source_interface(),
                std::ptr::null_mut(),
            );
            if src.is_null() {
                return false;
            }

            // offer(mime_type): opcode 0, "s" — one call per flavor the
            // parked payload carries, plus the pre-MIME plain-text spellings
            // so GTK/Qt/terminal clients still match. This is what makes a
            // Wayland copy a real fan-out: the peer picks which one it wants
            // and `data_source_send` names it back to us.
            let offer: unsafe extern "C" fn(
                *mut defines::wl_proxy,
                u32,
                *const std::os::raw::c_char,
            ) = std::mem::transmute(self.wayland.wl_proxy_marshal);
            let mimes = clipboard::native_copy_mimes();
            if mimes.is_empty() {
                // Nothing parked to serve. Taking the selection anyway would
                // make every paste from this app return an empty pipe.
                //
                // BOTH halves of the teardown, like every destructor here: the
                // request tells the server, `wl_proxy_destroy` frees the client
                // id. Skipping the request leaks the server-side object.
                let destroy: unsafe extern "C" fn(*mut defines::wl_proxy, u32) =
                    std::mem::transmute(self.wayland.wl_proxy_marshal);
                destroy(src, 1);
                (self.wayland.wl_proxy_destroy)(src);
                return false;
            }
            for mime in &mimes {
                // A mime with an interior NUL cannot be marshalled; skipping
                // it loses one flavor rather than the whole copy.
                let Ok(c) = std::ffi::CString::new(mime.as_str()) else {
                    continue;
                };
                offer(src, 0, c.as_ptr());
            }

            (self.wayland.wl_proxy_add_listener)(
                src,
                &events::WL_DATA_SOURCE_LISTENER as *const _ as *const _,
                self as *mut Self as *mut _,
            );

            // set_selection(source, serial): opcode 1 on wl_data_device.
            let set_selection: unsafe extern "C" fn(
                *mut defines::wl_proxy,
                u32,
                *mut defines::wl_proxy,
                u32,
            ) = std::mem::transmute(self.wayland.wl_proxy_marshal);
            set_selection(
                self.data_device as *mut defines::wl_proxy,
                1,
                src,
                self.last_input_serial,
            );
            (self.wayland.wl_display_flush)(self.display);

            self.clipboard_source = src as *mut defines::wl_data_source;
        }
        true
    }

    /// Read the current clipboard selection (another client's offer) as
    /// UTF-8 text via a pipe (same mechanism as the DnD uri-list receive).
    pub(super) fn read_wayland_selection(&mut self) -> Option<String> {
        if self.clipboard_offer.is_null() {
            return None;
        }
        let bytes = unsafe {
            events::receive_offer_bytes(self, self.clipboard_offer, "text/plain;charset=utf-8")
        };
        if bytes.is_empty() {
            return None;
        }
        Some(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Read EVERY flavor the current clipboard offer advertises that azul has
    /// a codec for.
    ///
    /// Driven by the offer's own advertised mime list (captured at
    /// `wl_data_device.selection`), not by guesswork: `wl_data_offer.receive`
    /// with a mime the source never offered is answered by a pipe the source
    /// is under no obligation to close, so each blind guess costs the full
    /// transfer deadline.
    pub(super) fn read_wayland_selection_payload(
        &mut self,
    ) -> Option<rich_clipboard::ClipboardPayload> {
        use rich_clipboard::{ClipboardItem, ClipboardPayload, Flavor, Platform};

        if self.clipboard_offer.is_null() {
            return None;
        }
        // Cloned because the receive borrows `self` mutably below.
        let offered: Vec<String> = self.drag.clipboard_mimes().to_vec();
        let offer = self.clipboard_offer;

        let mut payload = ClipboardPayload::new(Platform::Unix);
        // Borrows `offered`, so it must be declared after it (and is dropped
        // before it). `Flavor` is only `'static` when it came from a literal.
        let mut seen: Vec<Flavor<'_>> = Vec::new();
        for mime in &offered {
            let flavor = Flavor::from_mime(mime);
            // A flavor nothing here decodes is not worth a pipe transfer, and
            // two spellings of one flavor (`UTF8_STRING` next to
            // `text/plain;charset=utf-8`) are one transfer, not two.
            if matches!(flavor, Flavor::Other(_)) || seen.contains(&flavor) {
                continue;
            }
            let bytes = unsafe { events::receive_offer_bytes(self, offer, mime) };
            if bytes.is_empty() {
                continue;
            }
            seen.push(flavor);
            payload.push(ClipboardItem::new(mime.as_str(), bytes));
        }

        if payload.is_empty() {
            // No advertised mime answered — either the list never arrived
            // (an offer whose advertisements we missed) or every transfer
            // failed. Fall back to the single-flavor read, which asks for
            // plain text unconditionally.
            let text = self.read_wayland_selection()?;
            return rich_clipboard::encode(&rich_clipboard::RichItem::Text(text), Platform::Unix)
                .ok();
        }
        Some(payload)
    }

    // --- Primary selection (zwp_primary_selection_v1) ---

    /// Claim the primary selection for `text`.
    ///
    /// On Wayland as on X11, *selecting* text claims PRIMARY — no copy
    /// involved, and CLIPBOARD is untouched. Returns `false` when the
    /// compositor does not implement the protocol (GNOME did not until 42) or
    /// when there is no input serial yet, so the caller can stay silent rather
    /// than pretend.
    pub(super) fn wayland_set_primary_selection(&mut self) -> bool {
        if self.primary_selection_manager.is_null() || self.primary_selection_device.is_null() {
            return false;
        }
        if self.last_input_serial == 0 {
            return false;
        }
        unsafe {
            // Release any previous source. destroy: opcode 1, "". BOTH halves.
            if !self.primary_selection_source.is_null() {
                let destroy: unsafe extern "C" fn(*mut defines::wl_proxy, u32) =
                    std::mem::transmute(self.wayland.wl_proxy_marshal);
                destroy(self.primary_selection_source as *mut defines::wl_proxy, 1);
                (self.wayland.wl_proxy_destroy)(
                    self.primary_selection_source as *mut defines::wl_proxy,
                );
                self.primary_selection_source = std::ptr::null_mut();
            }

            // create_source: opcode 0 on the manager, "n".
            type CreateSrcCtor = unsafe extern "C" fn(
                *mut defines::wl_proxy,
                u32,
                *const defines::wl_interface,
                *mut std::ffi::c_void,
            ) -> *mut defines::wl_proxy;
            let ctor: CreateSrcCtor =
                std::mem::transmute(self.wayland.wl_proxy_marshal_constructor);
            let src = ctor(
                self.primary_selection_manager as *mut defines::wl_proxy,
                0,
                defines::get_primary_selection_source_v1_interface(),
                std::ptr::null_mut(),
            );
            if src.is_null() {
                return false;
            }

            // offer(mime_type): opcode 0, "s".
            let offer: unsafe extern "C" fn(
                *mut defines::wl_proxy,
                u32,
                *const std::os::raw::c_char,
            ) = std::mem::transmute(self.wayland.wl_proxy_marshal);
            for mime in ["text/plain;charset=utf-8", "UTF8_STRING", "text/plain"] {
                let Ok(c) = std::ffi::CString::new(mime) else {
                    continue;
                };
                offer(src, 0, c.as_ptr());
            }

            (self.wayland.wl_proxy_add_listener)(
                src,
                &events::PRIMARY_SELECTION_SOURCE_LISTENER as *const _ as *const _,
                self as *mut Self as *mut _,
            );

            // set_selection(source, serial): opcode 0 on the DEVICE — not the
            // 1 of wl_data_device.set_selection.
            let set_selection: unsafe extern "C" fn(
                *mut defines::wl_proxy,
                u32,
                *mut defines::wl_proxy,
                u32,
            ) = std::mem::transmute(self.wayland.wl_proxy_marshal);
            set_selection(
                self.primary_selection_device as *mut defines::wl_proxy,
                0,
                src,
                self.last_input_serial,
            );
            (self.wayland.wl_display_flush)(self.display);

            self.primary_selection_source = src as *mut defines::zwp_primary_selection_source_v1;
        }
        true
    }

    /// Read the current primary selection (another client's offer) as UTF-8.
    pub(super) fn read_wayland_primary_selection(&mut self) -> Option<String> {
        if self.primary_selection_offer.is_null() {
            return None;
        }
        let bytes = unsafe {
            events::receive_from_offer(
                self,
                self.primary_selection_offer as *mut defines::wl_proxy,
                events::PRIMARY_OFFER_RECEIVE_OPCODE,
                "text/plain;charset=utf-8",
            )
        };
        if bytes.is_empty() {
            return None;
        }
        Some(String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Handle keyboard enter event (window gained focus).
    ///
    /// `held_scancodes` are the evdev keycodes the compositor reports as already
    /// down at focus time (the `wl_keyboard.enter` keys array). They get no press
    /// events, so they must be seeded here or the engine's derived modifier set
    /// stays wrong until each of them is released.
    pub fn handle_keyboard_enter(&mut self, held_scancodes: &[u32]) {
        self.snapshot_window_state_baseline("wayland.handle_keyboard_enter");
        self.common
            .update_unsynced_state(|ws| ws.window_focused = true);
        self.dynamic_selector_context.window_focused = true;

        let xkb_state = self.keyboard_state.state;
        for &scancode in held_scancodes {
            self.common
                .keyboard_state_mut()
                .pressed_scancodes
                .insert_hm_item(scancode);
            if xkb_state.is_null() {
                continue;
            }
            // XKB keycode = evdev keycode + 8 (same offset as handle_key).
            let keysym = unsafe { (self.xkb.xkb_state_key_get_one_sym)(xkb_state, scancode + 8) };
            // Same shared table, same rule: a keysym with no virtual key seeds
            // nothing. Record what we DID seed so the eventual release removes
            // exactly that code (see the bookkeeping in `handle_key`).
            if let Some(vk) = events::keysym_to_virtual_keycode(keysym) {
                self.common
                    .keyboard_state_mut()
                    .pressed_virtual_keycodes
                    .insert_hm_item(vk);
                self.pressed_key_vks.insert(scancode, vk);
            }
        }
        // MWA-A3b: mirror of handle_keyboard_leave — forward focus to AT-SPI.
        #[cfg(feature = "a11y")]
        self.accessibility_adapter.set_focus(true);
        let result = self.process_window_events(0);
        self.handle_process_event_result(result);
        self.request_redraw();
    }

    /// Handle pointer leave event
    pub fn handle_pointer_leave(&mut self, _serial: u32) {
        // Pointer left the popup surface (e.g. moved back onto the parent):
        // tell the popup, clear the routing flag; the parent is not out-of-window.
        if self.pointer_over_popup {
            self.pointer_over_popup = false;
            if let Some(popup) = self.active_popup.as_mut() {
                popup.pointer_leave();
            }
            return;
        }

        // Get last known position before leaving
        let last_pos = match self
            .common
            .current_window_state()
            .mouse_state
            .cursor_position
        {
            CursorPosition::InWindow(pos) => pos,
            _ => LogicalPosition::zero(),
        };
        // MWA-C-hover: save the previous state and RUN the event pass —
        // previously this handler only pushed the empty hit test and
        // requested a redraw, so per-node MouseLeave callbacks, the tooltip
        // stop and the :hover restyle were all deferred to whatever event
        // happened to arrive next (macOS/X11/Windows all diff immediately).
        self.snapshot_window_state_baseline("wayland.handle_pointer_leave");
        self.common.mouse_state_mut().cursor_position = CursorPosition::OutOfWindow(last_pos);
        if let Some(ref mut layout_window) = self.common.layout_window {
            layout_window
                .hover_manager
                .push_hit_test(InputPointId::Mouse, FullHitTest::empty(None));
        }
        let result = self.process_window_events(0);
        self.handle_process_event_result(result);
        self.request_redraw();
    }

    /// Update hit test at current cursor position
    fn update_hit_test(&mut self, position: LogicalPosition) {
        // Delegate to the shared CommonWindowState::perform_hit_test, which resolves
        // the (now-refreshed, see generate_frame_if_needed) WebRender hit-tester in GPU
        // mode and falls back to the cpu_hit_tester in CPU mode. The previous inline
        // logic only acted `if let Resolved(..)`, but the hit-tester was left in the
        // `Requested` state forever -> it never ran -> no hover/click callbacks.
        let hit_test = self.common.perform_hit_test(position);
        if let Some(ref mut layout_window) = self.common.layout_window {
            layout_window
                .hover_manager
                .push_hit_test(InputPointId::Mouse, hit_test);
        }
    }

    /// wl_data_device drag entering / moving over the surface (emits
    /// `EventType::FileHover`). `position` is window-local; Wayland does not
    /// expose the file paths until the drop, so `paths` is a placeholder marker
    /// so the hover transition fires. Mirrors the X11/macOS handlers.
    pub fn handle_file_drag_entered(
        &mut self,
        position: LogicalPosition,
        paths: Vec<String>,
    ) -> ProcessEventResult {
        self.snapshot_window_state_baseline("wayland.handle_file_drag_entered");
        self.common.mouse_state_mut().cursor_position = CursorPosition::InWindow(position);
        if !paths.is_empty() {
            if let Some(layout_window) = self.common.layout_window.as_mut() {
                // MWA-B7: pass EVERY path — multi-file drops were silently
                // truncated to the first file at this ingress.
                layout_window
                    .file_drop_manager
                    .set_hovered_files(paths.iter().map(|p| p.clone().into()).collect());
            }
        }
        self.update_hit_test(position);
        self.process_window_events(0)
    }

    /// wl_data_device drag leaving without a drop (emits
    /// `EventType::FileHoverCancel`). Mirrors the X11/macOS handlers.
    pub fn handle_file_drag_exited(&mut self) -> ProcessEventResult {
        self.snapshot_window_state_baseline("wayland.handle_file_drag_exited");
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window.file_drop_manager.set_hovered_file(None);
        }
        let result = self.process_window_events(0);
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window.file_drop_manager.clear_hover_cancelled();
        }
        result
    }

    /// wl_data_device drop completed: the real file paths (parsed from
    /// `text/uri-list`) dropped at window-local `position` (emits
    /// `EventType::FileDrop`). Mirrors the X11/macOS handlers.
    pub fn handle_file_drop(
        &mut self,
        position: LogicalPosition,
        paths: Vec<String>,
    ) -> ProcessEventResult {
        self.snapshot_window_state_baseline("wayland.handle_file_drop");
        self.common.mouse_state_mut().cursor_position = CursorPosition::InWindow(position);
        if !paths.is_empty() {
            if let Some(layout_window) = self.common.layout_window.as_mut() {
                // MWA-B7: pass EVERY path — multi-file drops were silently
                // truncated to the first file at this ingress.
                layout_window
                    .file_drop_manager
                    .set_dropped_files(paths.iter().map(|p| p.clone().into()).collect());
            }
        }
        self.update_hit_test(position);
        let result = self.process_window_events(0);
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window.file_drop_manager.set_dropped_file(None);
        }
        result
    }

    /// MWA-C-hover: deepest hovered node from the LIVE hover manager (X11's
    /// get_first_hovered_node pattern) — used for right-click context menus;
    /// the old `common.last_hovered_node` field had no writer anywhere.
    fn get_first_hovered_node(&self) -> Option<HitTestNode> {
        self.common
            .layout_window
            .as_ref()?
            .hover_manager
            .get_current(&InputPointId::Mouse)?
            .hovered_nodes
            .iter()
            .flat_map(|(dom_id, ht)| {
                ht.regular_hit_test_nodes
                    .keys()
                    .next_back()
                    .map(|node_id| HitTestNode {
                        dom_id: dom_id.inner as u64,
                        node_id: node_id.index() as u64,
                    })
            })
            .next()
    }

    /// Try to show context menu for a node at the given position
    /// Returns true if a context menu was shown
    fn try_show_context_menu(
        &mut self,
        node: event::HitTestNode,
        position: LogicalPosition,
    ) -> bool {
        use azul_core::{dom::DomId, id::NodeId};

        let layout_window = match self.common.layout_window.as_ref() {
            Some(lw) => lw,
            None => return false,
        };

        let dom_id = DomId {
            inner: node.dom_id as usize,
        };

        // Get layout result for this DOM
        let layout_result = match layout_window.layout_results.get(&dom_id) {
            Some(lr) => lr,
            None => return false,
        };

        // Check if this node has a context menu
        let node_id = match NodeId::from_usize(node.node_id as usize) {
            Some(nid) => nid,
            None => return false,
        };

        let binding = layout_result.styled_dom.node_data.as_container();
        // A right-click on a CHILD of the node carrying the menu opens it too:
        // walk up to the first ancestor with a context menu (as X11/macOS do).
        let hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
        let mut cur = Some(node_id);
        let context_menu = loop {
            let nid = match cur {
                Some(n) => n,
                None => return false,
            };
            if let Some(menu) = binding.get(nid).and_then(|nd| nd.get_context_menu()) {
                break menu.clone();
            }
            cur = hierarchy.get(nid).and_then(|h| h.parent_id());
        };

        log_debug!(
            LogCategory::Input,
            "[Wayland Context Menu] Showing context menu at ({}, {}) for node {:?} with {} items",
            position.x,
            position.y,
            node,
            context_menu.items.as_slice().len()
        );

        // Queue the window creation instead of creating immediately
        self.show_window_based_context_menu(&context_menu, position);
        true
    }

    /// Queue a window-based context menu for creation in the event loop.
    ///
    /// This is part of the unified multi-window menu system (Shell2 V2).
    /// Wayland clients can't address absolute screen coordinates, so the
    /// popup is anchored relative to the parent surface via
    /// `menu::create_menu_popup_options`. The cursor position is recorded as
    /// a zero-sized trigger rect; the eventual xdg_popup positioner will
    /// anchor against it.
    fn show_window_based_context_menu(
        &mut self,
        menu: &azul_core::menu::Menu,
        position: LogicalPosition,
    ) {
        let trigger_rect =
            azul_core::geom::LogicalRect::new(position, azul_core::geom::LogicalSize::zero());
        let menu_size = self::menu::calculate_menu_size(menu, &self.common.system_style);

        let menu_options = self::menu::create_menu_popup_options(
            self,
            menu,
            &self.common.system_style,
            trigger_rect,
            menu_size,
        );

        log_debug!(
            LogCategory::Window,
            "[Wayland] Queuing window-based context menu at parent-relative ({}, {})",
            position.x,
            position.y
        );
        self.pending_window_creates.push(menu_options);
    }

    /// Regenerate layout after DOM changes
    ///
    /// Wayland-specific implementation with mandatory CSD injection.
    pub fn regenerate_layout_inner(
        &mut self,
    ) -> Result<crate::desktop::shell2::common::layout::LayoutRegenerateResult, String> {
        // Consume the reason tag BEFORE borrowing the layout window: this is
        // the regeneration this window asked for, and the tag travels with
        // the request (see CommonWindowState::request_regeneration).
        let relayout_reason = self.common.take_relayout_reason();

        let borrows = self.common.layout_borrows();
        let layout_window = borrows.layout_window.ok_or("No layout window")?;

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
            &self.resources.app_data,
            borrows.current_window_state,
            borrows.renderer_resources,
            borrows.gl_context_ptr,
            borrows.fc_cache,
            &self.resources.font_registry,
            borrows.system_style,
            &self.resources.icon_provider,
            &mut debug_messages,
            relayout_reason,
        )?;

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

        // NOTE: Do NOT request a regeneration here!
        // The caller (generate_frame_if_needed) manages this flag.
        // Setting it to true here would cause unnecessary re-layouts.

        // Update accessibility tree on Wayland
        #[cfg(feature = "a11y")]
        {
            // Scroll moved the content: throttled full rebuild into the slot
            // (bounds + scroll_x/y) before it is drained. Done on the local
            // directly — the `self`-taking trait helper cannot run while
            // `layout_window` is borrowed here.
            if layout_window.a11y_manager.scroll_rebuild_due(
                std::time::Instant::now(),
                std::time::Duration::from_millis(100),
            ) {
                layout_window.update_a11y_tree();
            }
            if let Some(tree_update) = layout_window.a11y_manager.take_pending() {
                self.accessibility_adapter.update_tree(tree_update);
            }
        }

        // Drain accessibility actions queued by the AT-SPI adapter (a screen
        // reader's 'click' etc.). The accesskit thread only parks them in
        // pending_actions; process_accessibility_actions() existed on every
        // backend but NOTHING ever called it — do_action() returned True at the
        // D-Bus level and the action was never dispatched.
        #[cfg(feature = "a11y")]
        self.process_accessibility_actions();

        // Drain lifecycle events (Mount / AfterMount / Unmount) produced by this
        // layout's reconciliation and dispatch them through the normal callback
        // pipeline — the SAME step headless + X11 run. Without this,
        // EventFilter::Component(AfterMount) callbacks never fire on Wayland, so
        // e.g. the MapWidget's first tile-fetch (kicked from AfterMount) never
        // starts. (The 16ms thread-poll tick below then drains the writebacks.)

        // Phase 2: Post-Layout callback - sync IME position after layout (MOST IMPORTANT)
        self.update_ime_position_from_cursor();
        self.sync_text_input_v3_focus_state();
        self.sync_ime_position_to_os();

        // Export the (possibly changed) application menu bar to GNOME Shell.
        // No-op unless GNOME native menus are active for this window.
        self.update_gnome_menu();

        Ok(result)
    }

    /// Update ime_position in window state from focused text cursor
    /// Called after layout to ensure IME window appears at correct position
    fn update_ime_position_from_cursor(&mut self) {
        use azul_core::window::ImePosition;

        if let Some(layout_window) = &self.common.layout_window {
            if let Some(cursor_rect) = layout_window.get_focused_cursor_rect_viewport() {
                // Successfully calculated cursor position from text layout
                self.common.update_unsynced_state(|ws| {
                    ws.ime_position = ImePosition::Initialized(cursor_rect);
                });
            }
        }
    }

    /// Apply initial window state at startup for fields not set during window creation.
    ///
    /// During new(), the following are already applied directly:
    /// - title (via xdg_toplevel_set_title)
    /// - size (via GL context / CPU buffer)
    /// - background_material (via apply_background_material)
    ///
    /// This method applies the remaining fields and seeds both baselines
    /// (event-diff and OS-sync) so that sync_window_state() works correctly for
    /// future changes.
    fn apply_initial_window_state(&mut self) {
        use azul_core::geom::OptionLogicalSize;
        use azul_core::window::WindowFrame;

        let mut needs_commit = false;

        // Window frame (Maximized, Minimized, Fullscreen)
        match self.common.current_window_state().flags.frame {
            WindowFrame::Maximized => {
                unsafe {
                    (self.wayland.xdg_toplevel_set_maximized)(self.xdg_toplevel);
                }
                needs_commit = true;
            }
            WindowFrame::Fullscreen => {
                unsafe {
                    (self.wayland.xdg_toplevel_set_fullscreen)(
                        self.xdg_toplevel,
                        std::ptr::null_mut(), // NULL = current output
                    );
                }
                needs_commit = true;
            }
            WindowFrame::Minimized => {
                unsafe {
                    (self.wayland.xdg_toplevel_set_minimized)(self.xdg_toplevel);
                }
                needs_commit = true;
            }
            WindowFrame::Normal => {} // Already in normal state
        }

        // Min dimensions
        if let OptionLogicalSize::Some(dims) =
            self.common.current_window_state().size.min_dimensions
        {
            unsafe {
                (self.wayland.xdg_toplevel_set_min_size)(
                    self.xdg_toplevel,
                    dims.width as i32,
                    dims.height as i32,
                );
            }
            needs_commit = true;
        }

        // Max dimensions
        if let OptionLogicalSize::Some(dims) =
            self.common.current_window_state().size.max_dimensions
        {
            unsafe {
                (self.wayland.xdg_toplevel_set_max_size)(
                    self.xdg_toplevel,
                    dims.width as i32,
                    dims.height as i32,
                );
            }
            needs_commit = true;
        }

        // is_top_level
        if self.common.current_window_state().flags.is_top_level {
            self.set_is_top_level(true);
        }

        // prevent_system_sleep
        if self
            .common
            .current_window_state()
            .flags
            .prevent_system_sleep
        {
            self.set_prevent_system_sleep(true);
        }

        // Commit changes if needed
        if needs_commit {
            unsafe {
                (self.wayland.wl_surface_commit)(self.surface);
            }
        }

        // Seed BOTH baselines: the event-diff one (so the first pass has
        // something to diff against) and the OS-sync one — everything above is
        // now applied on the toplevel, so sync_window_state() must not re-push
        // it. Until mark_os_synced() has run, take_os_sync_diff() answers None
        // and nothing is pushed at the compositor.
        self.seed_window_state_baseline("wayland.apply_initial_window_state");
        self.common.mark_os_synced();
    }

    /// Synchronize window state with Wayland compositor
    ///
    /// Wayland-specific state synchronization using Wayland protocols.
    pub fn sync_window_state(&mut self) {
        use azul_core::window::WindowFrame;

        // Diff against the OS-SYNC baseline, not the event baseline:
        // `previous_window_state` is advanced by every completed event pass and
        // is free to hold a live delta, so diffing it here would push (and echo)
        // state the compositor itself just reported in a configure.
        // `take_os_sync_diff` hands over (baseline, current) and advances the
        // baseline in the same call.
        let (previous, current) = match self.common.take_os_sync_diff() {
            Some(pair) => pair,
            None => return, // First frame, nothing to sync
        };

        // Note: Wayland state changes must be committed
        let mut needs_commit = false;

        // Sync title
        if previous.title != current.title {
            let c_title = match std::ffi::CString::new(current.title.as_str()) {
                Ok(s) => s,
                Err(_) => return,
            };
            unsafe {
                (self.wayland.xdg_toplevel_set_title)(self.xdg_toplevel, c_title.as_ptr());
            }
            needs_commit = true;
        }

        // Window frame state changed? (Minimize/Maximize/Normal/Fullscreen)
        if previous.flags.frame != current.flags.frame {
            match current.flags.frame {
                WindowFrame::Minimized => unsafe {
                    (self.wayland.xdg_toplevel_set_minimized)(self.xdg_toplevel);
                },
                WindowFrame::Maximized => {
                    // If previously fullscreen, unset fullscreen first
                    if previous.flags.frame == WindowFrame::Fullscreen {
                        unsafe {
                            (self.wayland.xdg_toplevel_unset_fullscreen)(self.xdg_toplevel);
                        }
                    }
                    unsafe {
                        (self.wayland.xdg_toplevel_set_maximized)(self.xdg_toplevel);
                    }
                }
                WindowFrame::Fullscreen => {
                    // If previously maximized, unset maximized first
                    if previous.flags.frame == WindowFrame::Maximized {
                        unsafe {
                            (self.wayland.xdg_toplevel_unset_maximized)(self.xdg_toplevel);
                        }
                    }
                    unsafe {
                        (self.wayland.xdg_toplevel_set_fullscreen)(
                            self.xdg_toplevel,
                            std::ptr::null_mut(), // NULL = current output
                        );
                    }
                }
                WindowFrame::Normal => {
                    if previous.flags.frame == WindowFrame::Maximized {
                        unsafe {
                            (self.wayland.xdg_toplevel_unset_maximized)(self.xdg_toplevel);
                        }
                    }
                    if previous.flags.frame == WindowFrame::Fullscreen {
                        unsafe {
                            (self.wayland.xdg_toplevel_unset_fullscreen)(self.xdg_toplevel);
                        }
                    }
                    // Note: Wayland has no explicit "unminimize" — the compositor handles it
                }
            }
            needs_commit = true;
        }

        // Min dimensions changed?
        if previous.size.min_dimensions != current.size.min_dimensions {
            use azul_core::geom::OptionLogicalSize;
            let (w, h) = match current.size.min_dimensions {
                OptionLogicalSize::Some(dims) => (dims.width as i32, dims.height as i32),
                OptionLogicalSize::None => (0, 0), // 0 = no minimum
            };
            unsafe {
                (self.wayland.xdg_toplevel_set_min_size)(self.xdg_toplevel, w, h);
            }
            needs_commit = true;
        }

        // Max dimensions changed?
        if previous.size.max_dimensions != current.size.max_dimensions {
            use azul_core::geom::OptionLogicalSize;
            let (w, h) = match current.size.max_dimensions {
                OptionLogicalSize::Some(dims) => (dims.width as i32, dims.height as i32),
                OptionLogicalSize::None => (0, 0), // 0 = no maximum
            };
            unsafe {
                (self.wayland.xdg_toplevel_set_max_size)(self.xdg_toplevel, w, h);
            }
            needs_commit = true;
        }

        // Check window flags for is_top_level
        if previous.flags.is_top_level != current.flags.is_top_level {
            self.set_is_top_level(current.flags.is_top_level);
        }

        // Check window flags for prevent_system_sleep
        if previous.flags.prevent_system_sleep != current.flags.prevent_system_sleep {
            self.set_prevent_system_sleep(current.flags.prevent_system_sleep);
        }

        // Background material changed? (transparency/blur effects)
        if previous.flags.background_material != current.flags.background_material {
            self.apply_background_material(current.flags.background_material);
            needs_commit = true;
        }

        // Note: Wayland doesn't support direct position control
        // The compositor decides window placement

        // Sync visibility
        // TODO: Wayland visibility control via xdg_toplevel methods

        // Commit changes if needed
        if needs_commit {
            unsafe {
                (self.wayland.wl_surface_commit)(self.surface);
            }
        }
    }

    /// Apply window background material for Wayland
    ///
    /// Wayland transparency handling:
    /// - Wayland compositors assume surfaces are opaque by default
    /// - To enable transparency: set opaque region to NULL
    /// - To optimize opaque windows: set opaque region covering entire surface
    /// - Blur effects (Mica, Acrylic) are compositor-specific:
    ///   - KDE Plasma: Uses `org.kde.kwin.blur` protocol
    ///   - GNOME: Does not support client-requested blur (window will be transparent only)
    ///   - Other compositors: Falls back to transparency without blur
    fn apply_background_material(&mut self, material: azul_core::window::WindowBackgroundMaterial) {
        use azul_core::window::WindowBackgroundMaterial;

        if self.surface.is_null() || self.compositor.is_null() {
            log_debug!(
                LogCategory::Platform,
                "[Wayland] Cannot apply background material - surface or compositor is null"
            );
            return;
        }

        // First, handle the opaque region based on material type
        let needs_transparency = !matches!(material, WindowBackgroundMaterial::Opaque);

        if needs_transparency {
            // Set opaque region to NULL to enable transparency
            // This tells the compositor the surface may have transparent areas
            unsafe {
                (self.wayland.wl_surface_set_opaque_region)(self.surface, std::ptr::null_mut());
            }
            log_debug!(
                LogCategory::Platform,
                "[Wayland] Set opaque region to NULL for transparency"
            );
        } else {
            // For opaque windows, set opaque region covering the entire surface
            // This optimizes compositing by telling the compositor it can skip blending
            let (width, height) = (
                self.common.current_window_state().size.dimensions.width as i32,
                self.common.current_window_state().size.dimensions.height as i32,
            );

            if width > 0 && height > 0 {
                unsafe {
                    let region = (self.wayland.wl_compositor_create_region)(self.compositor);
                    if !region.is_null() {
                        (self.wayland.wl_region_add)(region, 0, 0, width, height);
                        (self.wayland.wl_surface_set_opaque_region)(self.surface, region);
                        (self.wayland.wl_region_destroy)(region);
                        log_debug!(
                            LogCategory::Platform,
                            "[Wayland] Set opaque region to {}x{} for opaque window",
                            width,
                            height
                        );
                    }
                }
            }
        }

        // Handle blur effects for supported materials on KDE Plasma
        match material {
            WindowBackgroundMaterial::Opaque => {
                // Remove any existing blur effect
                self.remove_kde_blur();
            }
            WindowBackgroundMaterial::Transparent => {
                // Transparent but no blur - remove any existing blur
                self.remove_kde_blur();
            }
            WindowBackgroundMaterial::Sidebar
            | WindowBackgroundMaterial::Menu
            | WindowBackgroundMaterial::HUD
            | WindowBackgroundMaterial::Titlebar
            | WindowBackgroundMaterial::MicaAlt => {
                // These materials want blur effects
                // Try to apply KDE blur if blur_manager is available
                if self.blur_manager.is_some() {
                    self.apply_kde_blur();
                } else {
                    log_debug!(
                        LogCategory::Platform,
                        "[Wayland] Blur effects requested ({:?}) but no blur manager available - \
                         window will be transparent without blur (compositor may not support org.kde.kwin.blur)",
                        material
                    );
                }
            }
        }

        // Commit the surface to apply changes
        unsafe {
            (self.wayland.wl_surface_commit)(self.surface);
        }
    }

    /// Remove any existing KDE blur effect from the surface
    fn remove_kde_blur(&mut self) {
        if let Some(blur) = self.current_blur.take() {
            unsafe {
                // org_kde_kwin_blur.release: opcode 2 (destructor). Tell the server to
                // drop the blur, then free the client-side proxy.
                type ReleaseFn = unsafe extern "C" fn(*mut defines::wl_proxy, u32);
                let release_fn: ReleaseFn = std::mem::transmute(self.wayland.wl_proxy_marshal);
                release_fn(blur as *mut defines::wl_proxy, 2);
                (self.wayland.wl_proxy_destroy)(blur as *mut defines::wl_proxy);
            }
            log_debug!(
                LogCategory::Platform,
                "[Wayland] Removed KDE blur effect from surface"
            );
        }
    }

    /// Apply KDE blur effect to the surface
    ///
    /// Uses the org.kde.kwin.blur protocol available on KDE Plasma.
    /// The blur effect will cover the entire window.
    fn apply_kde_blur(&mut self) {
        let blur_manager = match self.blur_manager {
            Some(bm) => bm,
            None => return,
        };

        // Remove any existing blur first
        self.remove_kde_blur();

        // Create the per-surface blur object.
        // org_kde_kwin_blur_manager.create: opcode 0, signature "no"
        //   (new_id<org_kde_kwin_blur> id, object<wl_surface> surface).
        // A `new_id` REQUIRES a valid interface so libwayland can build the typed
        // proxy — the previous code passed a null interface (which libwayland
        // rejects: "null value passed for arg N"). We pass the hand-built
        // org_kde_kwin_blur interface and marshal via wl_proxy_marshal_flags
        // (with a wl_proxy_marshal_constructor fallback for libwayland < 1.20).
        unsafe {
            let blur_iface = defines::get_kde_blur_interface();
            let version =
                (self.wayland.wl_proxy_get_version)(blur_manager as *mut defines::wl_proxy);
            let blur = if !self.wayland.wl_proxy_marshal_flags.is_null() {
                type CreateFlags = unsafe extern "C" fn(
                    *mut defines::wl_proxy,
                    u32,
                    *const defines::wl_interface,
                    u32,
                    u32,
                    *mut std::ffi::c_void,
                    *mut defines::wl_surface,
                ) -> *mut defines::wl_proxy;
                let f: CreateFlags = std::mem::transmute(self.wayland.wl_proxy_marshal_flags);
                f(
                    blur_manager as *mut defines::wl_proxy,
                    0,
                    blur_iface,
                    version,
                    0,
                    std::ptr::null_mut(),
                    self.surface,
                )
            } else {
                type CreateCtor = unsafe extern "C" fn(
                    *mut defines::wl_proxy,
                    u32,
                    *const defines::wl_interface,
                    *mut std::ffi::c_void,
                    *mut defines::wl_surface,
                ) -> *mut defines::wl_proxy;
                let f: CreateCtor = std::mem::transmute(self.wayland.wl_proxy_marshal_constructor);
                f(
                    blur_manager as *mut defines::wl_proxy,
                    0,
                    blur_iface,
                    std::ptr::null_mut(),
                    self.surface,
                )
            };

            if blur.is_null() {
                log_debug!(
                    LogCategory::Platform,
                    "[Wayland] Failed to create KDE blur object"
                );
                return;
            }
            let blur = blur as *mut defines::org_kde_kwin_blur;

            // set_region(NULL) => blur the entire surface. opcode 1, signature "?o".
            type SetRegionFn =
                unsafe extern "C" fn(*mut defines::wl_proxy, u32, *const defines::wl_region);
            let set_region_fn: SetRegionFn = std::mem::transmute(self.wayland.wl_proxy_marshal);
            set_region_fn(
                blur as *mut defines::wl_proxy,
                1,
                std::ptr::null::<defines::wl_region>(),
            );

            // commit() => apply. opcode 0 (NOT 2 — opcode 2 is `release`, the
            // destructor; the old code committed with 2 and tore the blur down).
            type CommitFn = unsafe extern "C" fn(*mut defines::wl_proxy, u32);
            let commit_fn: CommitFn = std::mem::transmute(self.wayland.wl_proxy_marshal);
            commit_fn(blur as *mut defines::wl_proxy, 0);

            self.current_blur = Some(blur);

            log_debug!(
                LogCategory::Platform,
                "[Wayland] Applied KDE blur effect to surface"
            );
        }
    }

    /// Render a frame if needed, sending the appropriate WebRender transaction.
    ///
    /// Two paths:
    /// 1. **Full path** (a regeneration is pending): Regenerate layout, build full
    ///    transaction (fonts, images, display lists, scroll offsets, GPU values).
    /// 2. **Lightweight path** (only a redraw is pending, layout unchanged): Build lightweight
    ///    transaction (image callbacks, scroll offsets, GPU values only — skip scene builder).
    ///
    /// After sending the transaction, renders via WebRender and swaps buffers.
    /// Sets up Wayland frame callback for VSync.
    pub fn generate_frame_if_needed(&mut self) {
        // Queued VirtualView re-renders count as work owed. They are queued
        // OUT-OF-BAND by background writebacks (`trigger_all_virtual_view_
        // rerender` — e.g. the MapWidget's tile-fetch worker delivering a
        // tile) and are only ever drained inside this function; a gate that
        // ignores them leaves the queue sitting until some unrelated event
        // happens to repaint, and the async-loaded tiles never appear. X11's
        // poll_event gate names `vview_pending` for exactly this reason.
        let vview_pending = self
            .common
            .layout_window
            .as_ref()
            .map(|lw| !lw.pending_virtual_view_updates.is_empty())
            .unwrap_or(false);
        let needs_work = self.common.regeneration_pending()
            || self.common.relayout_only_pending()
            || self.common.resize_relayout_pending()
            || self.needs_redraw.pending()
            || vview_pending;
        if !needs_work {
            return;
        }
        if self.frame_callback_pending {
            // Expire a latch the compositor is never going to release.
            //
            // This flag is armed unconditionally after a commit and cleared ONLY
            // in frame_done_callback. It is also armed on paths that committed no
            // buffer at all (lazy CPU alloc, the GPU should_present skip, a
            // missing renderer), so there was nothing for the compositor to
            // answer. Combined with an occluded surface — where the protocol
            // explicitly permits withholding `done` — the window froze for good.
            let stale = self
                .frame_callback_armed_at
                .is_some_and(|t| t.elapsed() > FRAME_CALLBACK_TIMEOUT);
            if !stale {
                return;
            }
            log_warn!(
                LogCategory::Rendering,
                "[Wayland] frame callback not delivered within {:?} — proceeding anyway. The \
                 surface is likely occluded or minimised, where the compositor is entitled to \
                 withhold it.",
                FRAME_CALLBACK_TIMEOUT,
            );
            self.frame_callback_pending = false;
            self.frame_callback_armed_at = None;
        }

        // Did this frame actually put content on the surface? Set by the GPU swap
        // and by the CPU attach. Everything below keys off it: a frame that
        // committed nothing must NOT arm the frame-callback latch, because the
        // compositor has nothing to answer `done` for and the latch would then
        // block every future frame until the watchdog expires it.
        //
        // Reachable paths that commit nothing: the lazy CPU allocation
        // (RenderMode::Cpu(None) renders nothing on its first pass), the GPU
        // `should_present == false` skip, an empty CPU damage list, and a missing
        // renderer.
        // The frame clock for scope="present" (app_frame_seconds). X11 gets
        // this from the shared cpu_backend render fn; the Wayland present
        // path never passed through it, so Grafana under-counted Wayland
        // presents ~10x while the real event->render latency measured a
        // healthy 16ms p50 (wait_for_render probe, 2026-08-29). One pump per
        // generate_frame pass, ended on drop.
        let _frame_pump = azul_layout::telemetry::FramePump::begin("present");

        let mut surface_committed = false;

        // Did this frame RENDER to completion and simply find nothing changed
        // on screen (empty damage diff / zero GPU draw calls)? Such a frame has
        // fully satisfied the requests it saw even though it committed no
        // buffer, and they must be retired below exactly as X11 does at the end
        // of render_and_present. Leaving them raised looked harmless but
        // wasn't: the FIRST visually-inert regeneration (e.g. the AzMaps
        // startup frame re-rendering identical tile placeholders after
        // AfterMount re-raised the request) left `regeneration_pending` stuck,
        // so EVERY subsequent frame — every hover, every caret tick — took the
        // full regenerate_layout() path (rebuild the DOM, re-run layout)
        // instead of the lightweight one, forever. Stays `false` on the paths
        // that could NOT render/present (lazy shm alloc, both buffers held,
        // missing renderer), which genuinely still owe a frame.
        let mut frame_visually_complete = false;

        // CRITICAL: Make OpenGL context current BEFORE generate_frame
        // The image callbacks (RenderImageCallback) need the GL context to be current
        // to allocate textures and draw to them
        if let RenderMode::Gpu(ref gl_context, _) = self.render_mode {
            gl_context.make_current();
        }

        // RESIZE FAST PATH (coalesced). Any number of configures since the last
        // frame collapse into ONE incremental relayout of the EXISTING
        // StyledDom at the LATEST size — a drag delivers one configure per
        // pixel, and this consume-per-frame latch is what turns 75 configure/s
        // into (at most) frame-rate relayouts with zero layout() re-invocations.
        // A concurrent FULL regeneration request supersedes it: the full
        // rebuild lays out at the new size anyway, so the latch is consumed and
        // dropped rather than left to fire a redundant relayout afterwards.
        if self.common.take_resize_relayout() && !self.common.regeneration_pending() {
            let mut resize_relayout_failed = false;
            let mut debug_messages = None;
            let _span = crate::log_span!(LogCategory::Window, "resize_incremental_relayout");
            if let Err(e) = self.incremental_relayout_dispatching(
                crate::desktop::shell2::common::event::IncrementalRelayout::Resize,
                &mut debug_messages,
            ) {
                log_warn!(
                    LogCategory::Layout,
                    "[Wayland] resize fast-path relayout failed: {e} — falling back to a                          full regeneration"
                );
                resize_relayout_failed = true;
            }
            if resize_relayout_failed {
                self.common
                    .request_regeneration(azul_core::callbacks::RelayoutReason::Resize);
            } else {
                // Layout is now up to date on the existing StyledDom: take the
                // established relayout-only path below (skip regenerate_layout,
                // still rebuild the hit-tester + send the full transaction).
                self.common.request_relayout_only();
            }
        }

        // Captured BEFORE either path renders: a callback inside the render can
        // raise a new regeneration request, and only what we saw here may be
        // retired.
        let regen_epoch_seen = self.common.regen_epoch();
        // Same for the redraw request: the CPU "both buffers held" retry and the
        // scrollbar-fade re-arm BOTH raise it from inside this function.
        let redraw_epoch_seen = self.needs_redraw.epoch();
        let relayout_only = self.common.take_relayout_only();
        if self.common.regeneration_pending() || relayout_only {
            // FULL or RELAYOUT-ONLY PATH: both rebuild the CPU hit-tester + build &
            // send the full WebRender transaction below. Only the FULL path re-runs
            // regenerate_layout() (re-invokes the user's layout_callback + rebuilds the
            // StyledDom). The RELAYOUT-ONLY path's layout was already re-run by
            // incremental_relayout() in the ShouldIncrementalRelayout event arm
            // (relayout-only) — re-running regenerate_layout() here would discard
            // that work and re-invoke the layout_callback.
            if self.common.regeneration_pending() && !relayout_only {
                // FULL PATH: Regenerate layout
                if let Err(e) = self.regenerate_layout() {
                    log_error!(
                        LogCategory::Layout,
                        "[Wayland] Layout regeneration failed: {:?}",
                        e
                    );
                }
            }
            // The regeneration request is NOT retired here — it is retired at
            // the very end, together with the redraw request, on the only path
            // that actually commits a buffer. Retiring it here meant the three
            // early returns below (WebRender render error, eglSwapBuffers
            // failure, and the "nothing was committed" bail that the lazy CPU
            // buffer allocation takes on its first pass) dropped the rebuild on
            // the floor: no frame reached the screen and no request was left to
            // produce one.

            // Rebuild the CPU hit-tester from the fresh layout. CPU mode has no
            // WebRender hit-tester (render_api is None), and without this rebuild every
            // hit test returns nothing -> dead mouse hover / click / text selection /
            // focus. (GPU mode has cpu_hit_tester == None and uses the WebRender tester.)
            self.common.rebuild_cpu_hit_tester();

            // Send the full transaction (regenerate_layout only re-runs layout, doesn't
            // build/send the WebRender transaction on Wayland)
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
        } else {
            // LIGHTWEIGHT PATH: Scroll offsets + GPU values + image callbacks only
            if let (Some(ref mut layout_window), Some(ref mut render_api), Some(document_id)) = (
                self.common.layout_window.as_mut(),
                self.common.render_api.as_mut(),
                self.common.document_id,
            ) {
                // Advance easing-based scroll animations
                {
                    #[cfg(feature = "std")]
                    let now = azul_core::task::Instant::System(std::time::Instant::now().into());
                    #[cfg(not(feature = "std"))]
                    let now = azul_core::task::Instant::Tick(azul_core::task::SystemTick {
                        tick_counter: 0,
                    });
                    let tick_result = layout_window.scroll_manager.tick(now);
                    if tick_result.needs_repaint {
                        layout_window.scroll_manager.calculate_scrollbar_states();
                    }
                }

                // Process pending VirtualView updates (queued by ScrollTo → check_and_queue_virtual_view_reinvoke).
                // If present, we need a full display list rebuild rather than lightweight.
                let has_virtual_view_updates =
                    !layout_window.pending_virtual_view_updates.is_empty();
                if has_virtual_view_updates {
                    crate::desktop::shell2::common::layout::generate_frame(
                        layout_window,
                        render_api,
                        document_id,
                        &self.common.gl_context_ptr,
                    );
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
                            "[Wayland] Failed to build lightweight transaction: {}",
                            e
                        );
                    }

                    render_api.send_transaction(
                        crate::desktop::wr_translate2::wr_translate_document_id(document_id),
                        txn,
                    );
                }
            }
        }

        // Synchronously flush the scene builder so the transaction we just sent is
        // fully built before we render + present (this mirrors the working X11 path).
        // Without it, the GPU branch's old non-blocking readiness check bailed before
        // the very first swap, so the wl_surface never received a buffer and the
        // xdg_toplevel was never mapped -- the compositor showed only a taskbar icon.
        if let Some(ref mut render_api) = self.common.render_api {
            render_api.flush_scene_builder();
            // Refresh the WebRender hit-tester so it reflects the freshly-built
            // display list. AsyncHitTester::resolve() CACHES the resolved tester and
            // never re-resolves a newer scene, so a tester first resolved against the
            // initial (empty) display list stays stale forever -> hit tests return
            // nothing -> no hover/click callbacks fire (dead buttons). Re-requesting
            // after each flush (as macOS does) keeps it current. CPU mode has no
            // render_api, so it falls through to the cpu_hit_tester in perform_hit_test.
            if let Some(doc_id) = self.common.document_id {
                let req =
                    render_api.request_hit_tester(wr_translate2::wr_translate_document_id(doc_id));
                self.common.hit_tester = Some(AsyncHitTester::Requested(req));
            }
        }

        // NOTE: the redraw request is NOT retired here. It used to be, and every
        // early return below this line therefore destroyed it: the WebRender
        // render error, the eglSwapBuffers failure, and the "no buffer was
        // committed" bail all return with the request already gone and nothing
        // left to re-schedule them, so one transient GPU hiccup silently cost a
        // repaint nobody would ever ask for again. It is retired at the END, by
        // epoch, on the path that actually committed a frame — mirroring X11's
        // render_and_present.

        // Fractional-scale presentation state (computed before the
        // render_mode borrow): with a viewport + preferred_scale the buffer
        // scale stays 1 and set_destination maps the physical buffer to the
        // LOGICAL surface size.
        let fractional = self.fractional_scale_active();
        let (logical_w, logical_h) = {
            let d = &self.common.current_window_state().size.dimensions;
            (d.width as i32, d.height as i32)
        };
        // Read before `self.common.renderer` is borrowed mutably below.
        let physical_size = self.common.current_window_state().size.get_physical_size();

        match &mut self.render_mode {
            RenderMode::Gpu(gl_context, gl_functions) => {
                if let Some(renderer) = &mut self.common.renderer {
                    // Scene builder was flushed above -> the frame is ready. Clear the
                    // async readiness flag and render unconditionally; the previous
                    // `if !ready { return }` skipped the first present and left the
                    // window unmapped.
                    {
                        let (lock, _cvar) = &*self.new_frame_ready;
                        if let Ok(mut ready) = lock.lock() {
                            *ready = false;
                        }
                    }

                    // 1.5. Clear the EGL window backbuffer before WebRender draws.
                    // On this Wayland/EGL surface the default framebuffer comes back
                    // as uninitialized VRAM after each swap; WebRender's clear_color
                    // only clears its own offscreen render targets, so undrawn regions
                    // of the on-screen FBO showed stray pixels ("garbage dots"). Bind
                    // FBO 0, set the full viewport, and clear to the window background.
                    // (GenericGlContext is the same fn table used on macOS/X11.)
                    use azul_core::gl as gl_types;
                    gl_context.make_current();

                    // Back buffer age (EGL_EXT_buffer_age): lets WebRender
                    // render only the dirty regions accumulated over the last
                    // `age` frames. 0 = unsupported / undefined content ⇒
                    // full render (today's behavior). Pass 0 after a skipped
                    // present (see gpu_last_render_presented) — WR's damage
                    // tracker counts renders while EGL counts swaps.
                    let buffer_age = if self.gpu_last_render_presented {
                        gl_context.buffer_age()
                    } else {
                        0
                    };

                    gl_functions
                        .functions
                        .bind_framebuffer(gl_types::FRAMEBUFFER, 0);
                    gl_functions.functions.viewport(
                        0,
                        0,
                        physical_size.width as gl_types::GLint,
                        physical_size.height as gl_types::GLint,
                    );
                    // Clear the whole backbuffer ONLY when its content is
                    // undefined (age 0). With EGL_EXT_buffer_age reporting
                    // age >= 1 the buffer's previous content is guaranteed
                    // preserved — a full clear would wipe the regions
                    // WebRender is about to SKIP (partial render).
                    if buffer_age == 0 {
                        gl_functions.functions.clear_color(0.937, 0.941, 0.945, 1.0);
                        gl_functions
                            .functions
                            .clear(gl_types::COLOR_BUFFER_BIT | gl_types::DEPTH_BUFFER_BIT);
                    }

                    // 2. Update and render
                    renderer.update();
                    let device_size = webrender::api::units::DeviceIntSize::new(
                        physical_size.width as i32,
                        physical_size.height as i32,
                    );
                    // Present only when WebRender actually drew something. A no-op redraw
                    // (e.g. a lightweight frame, or a regen that rebuilds an unchanged
                    // scene after a duplicate compositor configure) renders 0 draw calls;
                    // since the EGL surface is multi-buffered, swapping that empty buffer
                    // would wipe the last good frame on the alternate buffer and blank the
                    // window. X11 only renders on real events so it never hit this; the
                    // Wayland frame-callback loop did. Gate strictly on draw calls.
                    let mut should_present = false;
                    match renderer.render(device_size, buffer_age) {
                        Ok(results) => {
                            if results.stats.total_draw_calls > 0 {
                                should_present = true;
                            }
                            // Store dirty rects for wl_surface_damage per-rect hints.
                            let dpi_scale =
                                self.common.current_window_state().size.dpi as f32 / 96.0;
                            self.gpu_damage_rects = results
                                .dirty_rects
                                .iter()
                                .map(|dr| azul_core::geom::LogicalRect {
                                    origin: azul_core::geom::LogicalPosition {
                                        x: dr.min.x as f32 / dpi_scale,
                                        y: dr.min.y as f32 / dpi_scale,
                                    },
                                    size: azul_core::geom::LogicalSize {
                                        width: dr.width() as f32 / dpi_scale,
                                        height: dr.height() as f32 / dpi_scale,
                                    },
                                })
                                .collect();
                        }
                        Err(e) => {
                            log_error!(
                                LogCategory::Rendering,
                                "[Wayland] WebRender render failed: {:?}",
                                e
                            );
                            return;
                        }
                    }

                    // 3. Present — but only if this frame actually drew content (see
                    // should_present above). Swapping an empty (0-draw-call) buffer would
                    // wipe the last good frame, since the EGL surface is multi-buffered.
                    if should_present {
                        // Buffer-age partial present: WebRender reported the
                        // TOTAL damage region (current frame ∪ previous
                        // `buffer_age - 1` frames) through the wr_damage cell.
                        // eglSwapBuffersWithDamage passes it to the compositor
                        // (and posts the wl_surface damage itself, so the
                        // manual wl_surface_damage hints below are skipped).
                        let fb_w = physical_size.width;
                        let fb_h = physical_size.height;
                        let present_rects = if gl_context.partial_present.swap_with_damage.is_some()
                        {
                            gl_context.wr_damage.take().map(|rects| {
                                wr_translate2::device_rects_to_present_rects(&rects, fb_w, fb_h)
                            })
                        } else {
                            let _ = gl_context.wr_damage.take();
                            None
                        };
                        let swap_result = match &present_rects {
                            // Empty rect list falls back to a full swap inside
                            // swap_buffers_with_damage (never silently ∅).
                            Some(rects) => gl_context.swap_buffers_with_damage(rects, fb_h),
                            None => gl_context.swap_buffers(),
                        };
                        if let Err(e) = swap_result {
                            log_error!(
                                LogCategory::Rendering,
                                "[Wayland] Swap buffers failed: {:?}",
                                e
                            );
                            return;
                        }
                        self.gpu_last_render_presented = true;
                        // eglSwapBuffers attaches, damages AND commits. Record it
                        // so step 4 does not commit a second time and so the
                        // frame-callback latch is only armed against a surface
                        // that actually has content pending.
                        surface_committed = true;
                        let swap_carried_damage =
                            matches!(&present_rects, Some(r) if !r.is_empty());

                        // 3.5. Inform Wayland compositor which regions changed (GPU damage
                        // hints). EGL handles buffer attachment via eglSwapBuffers, but
                        // explicit wl_surface_damage calls let the compositor skip
                        // recompositing unchanged regions. Skipped when the swap itself
                        // already carried the damage region.
                        if swap_carried_damage {
                            self.gpu_damage_rects.clear();
                        } else if !self.gpu_damage_rects.is_empty() {
                            for dr in &self.gpu_damage_rects {
                                unsafe {
                                    (self.wayland.wl_surface_damage)(
                                        self.surface,
                                        dr.origin.x as i32,
                                        dr.origin.y as i32,
                                        dr.size.width as i32,
                                        dr.size.height as i32,
                                    );
                                }
                            }
                            self.gpu_damage_rects.clear();
                        } else if self.common.display_list_initialized {
                            // No damage rects computed — full surface damage as fallback
                            let physical_size =
                                self.common.current_window_state().size.get_physical_size();
                            unsafe {
                                (self.wayland.wl_surface_damage)(
                                    self.surface,
                                    0,
                                    0,
                                    physical_size.width as i32,
                                    physical_size.height as i32,
                                );
                            }
                        }
                    } else {
                        // Rendered but NOT presented: WR's buffer-damage
                        // tracker recorded this frame while EGL's buffer age
                        // did not advance — force a full render next frame to
                        // resynchronize, and drop the stale damage region.
                        self.gpu_last_render_presented = false;
                        let _ = gl_context.wr_damage.take();
                        // Zero draw calls = the scene is visually unchanged;
                        // the requests this frame saw are satisfied.
                        frame_visually_complete = true;
                    }

                    self.common.display_list_initialized = true;

                    // Clean up old textures from previous epochs to prevent memory leak
                    if let Some(ref layout_window) = self.common.layout_window {
                        crate::desktop::gl_texture_integration::remove_old_gl_textures(
                            &layout_window.document_id,
                            layout_window.epoch,
                        );
                    }
                }
            }
            RenderMode::Cpu(Some(cpu_state)) => {
                // CPU rendering - render display list into shared memory buffer
                #[cfg(feature = "cpurender")]
                {
                    use azul_core::dom::DomId;

                    // Re-invoke any VirtualViews queued for in-place re-render
                    // (e.g. MapWidget tiles delivered by a background writeback
                    // that called trigger_all_virtual_view_rerender). The GPU
                    // path drains this inside generate_frame; the CPU path has
                    // no generate_frame, so without this the queue is never
                    // drained and async-loaded VirtualView content never
                    // appears (same fix as the X11 CPU branch). Must run
                    // BEFORE render_frame reads layout_results.
                    // One drain for every backend: it re-invokes in place AND rebuilds
                    // the CPU hit-tester (the rebuilt child DOMs carry fresh NodeIds).
                    self.common.drain_virtual_view_updates();

                    // Shared per-frame content preparation (journal clock, image
                    // callbacks through the content chokepoint, scrollbar cache).
                    // The logic lives in LayoutWindow so no backend can skip a piece.
                    if let Some(lw) = self.common.layout_window.as_mut() {
                        lw.prepare_frame_cpu();
                    }

                    // The both-buffers-held skip below re-raises needs_redraw
                    // deliberately; it must NOT count as "visually complete".
                    let mut present_skipped_buffers_held = false;
                    let rendered = if let Some(ref layout_window) = self.common.layout_window {
                        let dom_id = DomId { inner: 0 };
                        // render_frame looks up the layout result itself; we only
                        // need to know one exists before computing window dims.
                        if layout_window.layout_results.contains_key(&dom_id) {
                            let ws = &layout_window.current_window_state;
                            let width = ws.size.dimensions.width;
                            let height = ws.size.dimensions.height;
                            let dpi = ws.size.dpi as f32 / 96.0;

                            if width > 0.0 && height > 0.0 {
                                // #27 native backbuffer: with an ABGR8888 pool
                                // (renderer byte order) the CPU renderer draws
                                // DIRECTLY into the free shm slot — no owned
                                // intermediate frame, no swizzle copy. The
                                // slot is first caught up to the previous
                                // frame (cross-slot copy of the rects it
                                // missed) so the incremental model stays
                                // sound. A size mismatch (configure race)
                                // skips the arm; that frame takes the legacy
                                // owned+copy path below and the next re-arms.
                                let native_expected_w = (width * dpi).ceil() as u32;
                                let native_expected_h = (height * dpi).ceil() as u32;
                                let mut native_slot: Option<usize> = None;
                                let mut native_skip_render = false;
                                if (cpu_state.is_native() || cpu_state.needs_commit_swizzle())
                                    && cpu_state.width.max(0) as u32 == native_expected_w
                                    && cpu_state.height.max(0) as u32 == native_expected_h
                                {
                                    match cpu_state.acquire_slot() {
                                        Some(slot) => {
                                            cpu_state.catch_up_slot(slot);
                                            if !cpu_state.slots[slot].valid {
                                                // A never-filled slot (fresh
                                                // pool after a resize with no
                                                // valid sibling to copy from)
                                                // must receive a FULL render:
                                                // the incremental path rasters
                                                // only damage strips directly
                                                // into the slot and the rest
                                                // stays TRANSPARENT zeroed shm
                                                // - maximize let the window
                                                // below shine through (KDE
                                                // Wayland, 2026-08-29).
                                                // Dropping the previous DL
                                                // forces the diff onto the
                                                // full-repaint arm.
                                                self.cpu_backend.previous_display_list = None;
                                            }
                                            self.cpu_backend.native_target_pool_order =
                                                cpu_state.needs_commit_swizzle();
                                            self.cpu_backend.native_target = unsafe {
                                                azul_layout::cpurender::AzulPixmap::from_external(
                                                    cpu_state.slot_ptr(slot),
                                                    native_expected_w,
                                                    native_expected_h,
                                                )
                                            };
                                            native_slot = Some(slot);
                                        }
                                        None => {
                                            // Both buffers compositor-held:
                                            // nothing is rendered or consumed
                                            // this cycle, so the release-driven
                                            // retry re-enters as a clean first
                                            // attempt (unlike the legacy path,
                                            // no force-full is needed for
                                            // correctness — but os_present_
                                            // requested also wakes the retry).
                                            self.needs_redraw.raise();
                                            self.os_present_requested = true;
                                            present_skipped_buffers_held = true;
                                            native_skip_render = true;
                                        }
                                    }
                                }

                                // Shared CPU renderer (same path as headless + X11):
                                // damage diff + scroll-offset feed + thin-strip
                                // scroll-shift with eligibility + offset-aware render.
                                // Replaces the logic that used to live here and lacked
                                // all the scroll machinery (#13/#14).
                                if !native_skip_render {
                                    // Transparent material clears to alpha 0
                                    // (ARGB8888 carries it); shape if asked.
                                    self.cpu_backend
                                        .sync_window_flags(&layout_window.current_window_state);
                                    self.cpu_backend.render_frame(
                                        layout_window,
                                        &layout_window.renderer_resources,
                                        width,
                                        height,
                                        dpi,
                                    );
                                }
                                // Dangle guard: render_frame normally consumes
                                // the target, but its early returns (no layout
                                // result, zero size) must not leave a pointer
                                // into the pool armed across frames — the pool
                                // dies on resize.
                                self.cpu_backend.native_target = None;

                                if std::env::var("AZ_BB_DEBUG").is_ok() {
                                    eprintln!(
                                        "[bb] native={} slot={:?} frame_damage={:?} present_damage_rects={:?}",
                                        self.cpu_backend.rendered_native,
                                        native_slot,
                                        match self.cpu_backend.last_frame_damage {
                                            crate::desktop::shell2::headless::FrameDamage::Full => "FULL".to_string(),
                                            crate::desktop::shell2::headless::FrameDamage::None => "NONE".to_string(),
                                            crate::desktop::shell2::headless::FrameDamage::Rects(ref r) => format!("{} rects {:?}", r.len(), r.iter().take(4).collect::<Vec<_>>()),
                                        },
                                        self.cpu_backend.last_present_damage
                                            .to_present_rects_physical(dpi, native_expected_w, native_expected_h, false)
                                            .map(|r| r.len()),
                                    );
                                }
                                if self.cpu_backend.rendered_native {
                                    // #27: the frame was rasterised directly
                                    // into `native_slot`. Present = damage
                                    // bookkeeping only; the shared attach/
                                    // commit below picks the slot up via
                                    // `damage_rects`.
                                    let force_full = self.os_present_requested
                                        || !self.common.display_list_initialized;
                                    self.os_present_requested = false;
                                    let rects = self
                                        .cpu_backend
                                        .last_present_damage
                                        .to_present_rects_physical(
                                            dpi,
                                            native_expected_w,
                                            native_expected_h,
                                            force_full,
                                        );
                                    if let (Some(rects), Some(slot)) = (rects, native_slot) {
                                        let full_render = matches!(
                                            self.cpu_backend.last_frame_damage,
                                            crate::desktop::shell2::headless::FrameDamage::Full
                                        );
                                        if full_render {
                                            cpu_state.slots[slot].valid = true;
                                            cpu_state.slots[slot].stale.clear();
                                            cpu_state.slots[slot].stale_overflow = false;
                                        } else if !cpu_state.slots[slot].valid {
                                            // "The first render into a fresh
                                            // pool is a full repaint" broke:
                                            // an incremental frame landed on
                                            // undefined pixels.
                                            log_error!(
                                                LogCategory::Rendering,
                                                "[native-bb] INCREMENTAL render into \
                                                 never-filled slot {} — undefined pixels \
                                                 may be on screen",
                                                slot
                                            );
                                        }
                                        // The OTHER slot missed this frame's
                                        // changes.
                                        let other = 1 - slot;
                                        for (x, y, w, h) in &rects {
                                            cpu_state.slots[other]
                                                .stale
                                                .push((*x as i32, *y as i32, *w as i32, *h as i32));
                                        }
                                        if cpu_state.slots[other].stale.len() > 32 {
                                            cpu_state.slots[other].stale.clear();
                                            cpu_state.slots[other].stale_overflow = true;
                                        }
                                        cpu_state.damage_rects.extend(rects.iter().map(
                                            |(x, y, w, h)| {
                                                (*x as i32, *y as i32, *w as i32, *h as i32)
                                            },
                                        ));
                                        // #32: ARGB8888 pool — convert this
                                        // frame's written pixels from the
                                        // renderer's R,G,B,A to the pool's
                                        // B,G,R,A in place, exactly once,
                                        // before the shared attach/commit
                                        // below picks the slot up.
                                        //
                                        // The swizzle set is computed WITHOUT
                                        // force_full, independently of the
                                        // present set: presenting extra
                                        // damage is harmless over-coverage
                                        // for the compositor, but an in-place
                                        // byte swap of pixels nobody wrote
                                        // TOGGLES them R<->B on every
                                        // repetition. Swizzle exactly what
                                        // this frame wrote (paint rects ∪
                                        // shift-unswizzled clips), never the
                                        // whole buffer on someone's expose
                                        // request.
                                        if cpu_state.needs_commit_swizzle() {
                                            let stride = cpu_state.stride.max(0) as usize;
                                            let h = cpu_state.height.max(0) as usize;
                                            let swizzle_rects = if full_render {
                                                // A genuinely full render
                                                // wrote every pixel.
                                                Some(vec![(
                                                    0u32,
                                                    0u32,
                                                    native_expected_w,
                                                    native_expected_h,
                                                )])
                                            } else {
                                                self.cpu_backend
                                                    .last_present_damage
                                                    .to_present_rects_physical(
                                                        dpi,
                                                        native_expected_w,
                                                        native_expected_h,
                                                        false,
                                                    )
                                            };
                                            let int_rects: Vec<(i32, i32, i32, i32)> =
                                                swizzle_rects
                                                    .unwrap_or_default()
                                                    .iter()
                                                    .map(|(x, y, w, h)| {
                                                        (*x as i32, *y as i32, *w as i32, *h as i32)
                                                    })
                                                    .collect();
                                            if std::env::var("AZ_BB_DEBUG").is_ok() {
                                                eprintln!(
                                                    "[bb] SWIZZLE slot={slot} rects={int_rects:?}"
                                                );
                                            }
                                            crate::desktop::shell2::headless::swizzle_rb_in_rects(
                                                cpu_state.slot_buffer_mut(slot),
                                                stride,
                                                h,
                                                &int_rects,
                                            );
                                        }
                                    }
                                } else
                                // Blit the rendered pixmap into the Wayland shm
                                // buffer — PARTIALLY: only the present-damage
                                // rows are converted and copied, and the same
                                // rects are queued for per-rect
                                // wl_surface_damage at the commit below. The
                                // old code re-swizzled the WHOLE frame per
                                // present and posted full-surface damage, so
                                // the compositor recomposited everything on
                                // every hover/caret tick. FrameDamage::None →
                                // copy nothing, damage nothing (the retained
                                // single shm buffer already holds the frame).
                                if let Some(ref pixmap) = self.cpu_backend.last_frame {
                                    let force_full = self.os_present_requested
                                        || !self.common.display_list_initialized;
                                    self.os_present_requested = false;
                                    let src_w = pixmap.width();
                                    let src_h = pixmap.height();
                                    // Clamp the presentable area to BOTH the
                                    // pixmap and the shm buffer (they diverge
                                    // when a resize configure races a render).
                                    let clamp_w = src_w.min(cpu_state.width.max(0) as u32);
                                    let clamp_h = src_h.min(cpu_state.height.max(0) as u32);
                                    let rects = self
                                        .cpu_backend
                                        .last_present_damage
                                        .to_present_rects_physical(
                                            dpi, clamp_w, clamp_h, force_full,
                                        );
                                    if let Some(rects) = rects {
                                        // Double buffering: draw into a buffer
                                        // the compositor does NOT hold. If both
                                        // are held, skip this present and retry
                                        // after the next frame callback /
                                        // buffer release.
                                        if let Some(slot) = cpu_state.acquire_slot() {
                                            // Copy set = new damage ∪ what this
                                            // slot missed while the other one
                                            // was on screen.
                                            let full = (0u32, 0u32, clamp_w, clamp_h);
                                            let copy_rects: Vec<(u32, u32, u32, u32)> = if cpu_state
                                                .slots[slot]
                                                .stale_overflow
                                            {
                                                vec![full]
                                            } else {
                                                rects
                                                    .iter()
                                                    .copied()
                                                    .chain(cpu_state.slots[slot].stale.iter().map(
                                                        |&(x, y, w, h)| {
                                                            (
                                                                x.max(0) as u32,
                                                                y.max(0) as u32,
                                                                w.max(0) as u32,
                                                                h.max(0) as u32,
                                                            )
                                                        },
                                                    ))
                                                    .collect()
                                            };
                                            let dst_stride = (cpu_state.width.max(0) as usize) * 4;
                                            let src_stride = (src_w as usize) * 4;
                                            // #27: ABGR pool = renderer byte
                                            // order → rows copy verbatim (this
                                            // path is then only the configure-
                                            // race fallback); ARGB needs the
                                            // R↔B swizzle.
                                            let straight = cpu_state.is_native();
                                            let src = pixmap.data();
                                            let buf = cpu_state.slot_buffer_mut(slot);
                                            for (rx, ry, rw, rh) in &copy_rects {
                                                for row in 0..*rh as usize {
                                                    let y = *ry as usize + row;
                                                    let so = y * src_stride + (*rx as usize) * 4;
                                                    let doff = y * dst_stride + (*rx as usize) * 4;
                                                    let n = (*rw as usize) * 4;
                                                    if so + n > src.len() || doff + n > buf.len() {
                                                        continue;
                                                    }
                                                    if straight {
                                                        buf[doff..doff + n]
                                                            .copy_from_slice(&src[so..so + n]);
                                                        continue;
                                                    }
                                                    // RGBA → ARGB8888 (BGRA in LE memory)
                                                    for (s, d) in
                                                        src[so..so + n].chunks_exact(4).zip(
                                                            buf[doff..doff + n].chunks_exact_mut(4),
                                                        )
                                                    {
                                                        d[0] = s[2]; // B
                                                        d[1] = s[1]; // G
                                                        d[2] = s[0]; // R
                                                        d[3] = s[3]; // A
                                                    }
                                                }
                                            }
                                            // AZ_PRESENT_VERIFY=1: after the
                                            // partial copy the slot must equal
                                            // the pixmap EVERYWHERE (copied ∪
                                            // retained). Any mismatch is slot
                                            // staleness the catch-up missed —
                                            // the definitive live diagnostic
                                            // for "flapping" chrome (task #19).
                                            if std::env::var_os("AZ_PRESENT_VERIFY").is_some() {
                                                let mut bad = 0usize;
                                                let mut first: Option<(usize, usize)> = None;
                                                'rows: for y in 0..clamp_h as usize {
                                                    let so = y * src_stride;
                                                    let doff = y * dst_stride;
                                                    for x in 0..clamp_w as usize {
                                                        let s = &src[so + x * 4..so + x * 4 + 4];
                                                        let d =
                                                            &buf[doff + x * 4..doff + x * 4 + 4];
                                                        let differs = if straight {
                                                            d[0] != s[0]
                                                                || d[1] != s[1]
                                                                || d[2] != s[2]
                                                        } else {
                                                            d[0] != s[2]
                                                                || d[1] != s[1]
                                                                || d[2] != s[0]
                                                        };
                                                        if differs {
                                                            bad += 1;
                                                            if first.is_none() {
                                                                first = Some((x, y));
                                                            }
                                                            if bad > 4096 {
                                                                break 'rows;
                                                            }
                                                        }
                                                    }
                                                }
                                                if bad > 0 {
                                                    log_error!(
                                                        LogCategory::Rendering,
                                                        "[present-verify] slot {} STALE: {}+ px \
                                                         differ from the pixmap, first at {:?} \
                                                         ({} copy rects this present)",
                                                        slot,
                                                        bad,
                                                        first,
                                                        copy_rects.len()
                                                    );
                                                }
                                            }
                                            // Stale bookkeeping: this slot is
                                            // now current; the OTHER slot missed
                                            // this frame's rects.
                                            cpu_state.slots[slot].stale.clear();
                                            cpu_state.slots[slot].stale_overflow = false;
                                            // The copy set (rects ∪ stale, or
                                            // full) always leaves a legacy slot
                                            // complete (#27 validity model).
                                            cpu_state.slots[slot].valid = true;
                                            let other = 1 - slot;
                                            for (x, y, w, h) in &rects {
                                                cpu_state.slots[other].stale.push((
                                                    *x as i32, *y as i32, *w as i32, *h as i32,
                                                ));
                                            }
                                            if cpu_state.slots[other].stale.len() > 32 {
                                                cpu_state.slots[other].stale.clear();
                                                cpu_state.slots[other].stale_overflow = true;
                                            }
                                            // Damage = the NEW rects only, in
                                            // BUFFER coordinates.
                                            cpu_state.damage_rects.extend(rects.iter().map(
                                                |(x, y, w, h)| {
                                                    (*x as i32, *y as i32, *w as i32, *h as i32)
                                                },
                                            ));
                                        } else {
                                            // Both buffers held by the
                                            // compositor — retry next cycle.
                                            // The retry's render_frame will
                                            // diff as "unchanged", so force a
                                            // full copy+present then or this
                                            // frame would never reach screen.
                                            // (The wake for that retry is the
                                            // pre-park gate in
                                            // wait_for_events: the release
                                            // event itself can only flip the
                                            // slot's busy flag.)
                                            self.needs_redraw.raise();
                                            self.os_present_requested = true;
                                            present_skipped_buffers_held = true;
                                        }
                                    }
                                }
                                // (previous-display-list tracking now lives inside
                                // CpuBackend::render_frame.)
                                true
                            } else {
                                false
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    };

                    if !rendered {
                        if cpu_state.acquire_slot().is_some() {
                            cpu_state.draw_blue();
                            let (w, h) = (cpu_state.width, cpu_state.height);
                            cpu_state.damage_rects.push((0, 0, w, h));
                        } else {
                            self.needs_redraw.raise();
                        }
                    }

                    // Rendered to completion without a buffer-availability
                    // skip: even if the damage diff came back empty (nothing
                    // to attach below), this frame has DONE the work it saw.
                    frame_visually_complete = rendered && !present_skipped_buffers_held;
                }

                #[cfg(not(feature = "cpurender"))]
                {
                    if cpu_state.acquire_slot().is_some() {
                        cpu_state.draw_blue();
                        let (w, h) = (cpu_state.width, cpu_state.height);
                        cpu_state.damage_rects.push((0, 0, w, h));
                    } else {
                        self.needs_redraw.raise();
                    }
                }

                unsafe {
                    let surface_version =
                        (self.wayland.wl_proxy_get_version)(self.surface as *mut defines::wl_proxy);
                    // Attach only when something was drawn/damaged — attaching
                    // is what marks the buffer busy.
                    if !cpu_state.damage_rects.is_empty() {
                        (self.wayland.wl_surface_attach)(
                            self.surface,
                            cpu_state.active_buffer(),
                            0,
                            0,
                        );
                        *cpu_state.slots[cpu_state.active].busy = true;
                        surface_committed = true;
                        // The GPU branch sets this after its first present;
                        // the CPU branch NEVER did, so `force_full =
                        // .. || !display_list_initialized` stayed true for
                        // the window's whole life: every present claimed
                        // FULL-buffer damage (compositor recomposited the
                        // world per caret blink) and - far worse - the
                        // in-place commit-swizzle used the same rect set,
                        // TOGGLING every unwritten pixel R<->B per frame
                        // (AzWriter's blue/brown flip, 2026-08-29).
                        self.common.display_list_initialized = true;
                        if fractional {
                            // Fractional path: buffer scale stays 1 (reset a
                            // stale integer value if any); the viewport maps
                            // the physical buffer to the LOGICAL surface size.
                            if surface_version >= 3 {
                                (self.wayland.wl_surface_set_buffer_scale)(self.surface, 1);
                            }
                            if let Some(vp) = self.viewport {
                                wp_viewport_set_destination(
                                    &self.wayland,
                                    vp,
                                    logical_w,
                                    logical_h,
                                );
                            }
                        } else if surface_version >= 3 && cpu_state.scale > 1 {
                            // HiDPI: tell the compositor the buffer is scale×
                            // the surface size (v3+). Without this a
                            // physical-sized buffer displays scale× too large.
                            (self.wayland.wl_surface_set_buffer_scale)(
                                self.surface,
                                cpu_state.scale,
                            );
                        }
                    }
                    // Per-rect present damage (queued above; BUFFER px). Empty
                    // = frame unchanged → the compositor recomposites nothing.
                    // damage_buffer (v4+) takes buffer px directly; older
                    // surfaces get surface-local coords (buffer / scale,
                    // rounded OUTWARD).
                    let scale = cpu_state.scale.max(1);
                    for (dx, dy, dw, dh) in cpu_state.damage_rects.drain(..) {
                        if surface_version >= 4 {
                            (self.wayland.wl_surface_damage_buffer)(self.surface, dx, dy, dw, dh);
                        } else {
                            let x0 = dx.div_euclid(scale);
                            let y0 = dy.div_euclid(scale);
                            let x1 = (dx + dw + scale - 1).div_euclid(scale);
                            let y1 = (dy + dh + scale - 1).div_euclid(scale);
                            (self.wayland.wl_surface_damage)(
                                self.surface,
                                x0,
                                y0,
                                x1 - x0,
                                y1 - y0,
                            );
                        }
                    }
                }
            }
            RenderMode::Cpu(None) => {
                // CPU fallback not yet initialized - initialize it now if we have shm
                if !self.shm.is_null() {
                    let width = self.common.current_window_state().size.dimensions.width as i32;
                    let height = self.common.current_window_state().size.dimensions.height as i32;
                    let (buf_w, buf_h, scale) = self.cpu_buffer_spec(width, height);
                    match CpuFallbackState::new(&self.wayland, self.shm, buf_w, buf_h, scale) {
                        Ok(cpu_state) => {
                            self.render_mode = RenderMode::Cpu(Some(cpu_state));
                            self.os_present_requested = true; // fresh buffer
                            log_info!(
                                LogCategory::Rendering,
                                "[Wayland] CPU fallback initialized: {}x{}",
                                width,
                                height
                            );
                        }
                        Err(e) => {
                            log_error!(
                                LogCategory::Rendering,
                                "[Wayland] Failed to initialize CPU fallback: {:?}",
                                e
                            );
                        }
                    }
                }
            }
        }

        // 4. Set up frame callback for next frame (VSync).
        //
        // ONLY when the surface actually has content pending. Previously this ran
        // unconditionally, so a frame that committed nothing still armed
        // frame_callback_pending — and since the compositor had nothing to answer
        // for, no `done` ever arrived and the gate at the top of this function
        // blocked every subsequent frame.
        if !surface_committed {
            if frame_visually_complete {
                // The frame RENDERED and found nothing changed on screen: the
                // requests it observed are satisfied — retire them (by epoch,
                // so anything raised during the render survives), exactly as
                // the committing path below does. Skipping this looked safe
                // and was not: one visually-inert regeneration left
                // `regeneration_pending` raised forever, and every subsequent
                // frame took the full regenerate_layout() path.
                self.common
                    .clear_regeneration_unless_reraised(regen_epoch_seen);
                self.needs_redraw.retire_unless_reraised(redraw_epoch_seen);
                log_debug!(
                    LogCategory::Rendering,
                    "[Wayland] frame rendered with no visual change — nothing committed, \
                     requests retired, frame callback not armed",
                );
            } else {
                log_warn!(
                    LogCategory::Rendering,
                    "[Wayland] frame produced no buffer commit (lazy CPU alloc, both shm \
                     buffers held, or no renderer) — requests stay raised; not arming the \
                     frame callback, so the next frame is not blocked waiting for a `done` \
                     that cannot come",
                );
            }
            return;
        }

        // `AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER=1` exits 0 as soon as one frame has
        // genuinely reached the compositor. Placed HERE, after the
        // `surface_committed` guard, because that is the strongest evidence
        // Wayland offers: a buffer was attached, damaged and committed. Anything
        // earlier would report success for a frame that produced no buffer, which
        // is the exact false green this variable exists to remove.
        //
        // Until now only windows and headless honoured it — despite a comment in
        // headless/mod.rs claiming "windows, macos, x11 and wayland have honoured
        // this for a while", which was simply untrue. That gap is why there is no
        // non-interactive way to ask a real compositor "did it render?": the
        // process just keeps running, and a harness can only time out, which is
        // indistinguishable from a hang. (I hit exactly that while testing this
        // backend by hand.) With this in place a CI job can run weston/sway
        // headless and assert exit 0 — a real present-path test on the backend
        // where most of the redraw bugs lived. See #56.
        if std::env::var("AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER").is_ok() {
            log_info!(
                LogCategory::Rendering,
                "[Wayland] AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER: a buffer was committed to the \
                 compositor, requesting close",
            );
            // Request the close, do NOT call `self.close()` here.
            //
            // Closing mid-frame tears the GL device down while WebRender still
            // holds live resources, and webrender/core/src/device/gl.rs:1020
            // asserts `thread::panicking() || self.refcount == 0` — so the direct
            // call turned a successful render into SIGSEGV (verified: exit 139).
            // The run loop already honours this flag at run.rs:1388 and closes at
            // a point where teardown is safe.
            self.snapshot_window_state_baseline("wayland.exit_after_frame_render");
            self.common
                .update_window_state(event::WindowStateSource::App, |ws| {
                    ws.flags.close_requested = true;
                });
            let _ = self.process_window_events(0);
            // A synthetic shutdown, so it is announced but not refusable.
            self.common
                .update_window_state(event::WindowStateSource::App, |ws| {
                    ws.flags.close_requested = true;
                });
            return;
        }
        // KNOWN-SUBOPTIMAL, deliberately left visible here rather than only in a
        // tracker: this requests the frame callback AFTER the present and then
        // commits a SECOND time to schedule it. On the GPU path eglSwapBuffers has
        // already attached, damaged and committed, and Mesa's own guidance is that
        // "sending a wl_surface.commit request at all outside of eglSwapBuffers
        // will break frame throttling, and may result in discarded frames".
        //
        // The correct shape is to request wl_surface_frame BEFORE the present, so
        // the present's own commit carries it and no second commit exists. That
        // means threading the request through both the GPU and CPU branches, so it
        // is a focused change rather than a line edit — see #42.
        //
        // It is not currently harmful in the way Mesa warns about, because
        // configure_vsync now forces eglSwapInterval(0) (b94eeb146): Mesa arms no
        // throttle callback of its own, so ours is the only one and there is
        // nothing for the extra commit to desynchronise. It does cost one
        // redundant content update per frame.
        unsafe {
            let frame_callback = (self.wayland.wl_surface_frame)(self.surface);
            // The listener MUST outlive the proxy: wl_proxy_add_listener stores the
            // POINTER, not a copy. A stack-local listener here was a use-after-free —
            // when the compositor later sent `done`, libwayland dereferenced freed
            // stack and jumped to a garbage fn pointer (SIGSEGV in ffi_call). Use a
            // 'static listener, like every other listener in this file.
            static FRAME_CALLBACK_LISTENER: defines::wl_callback_listener =
                defines::wl_callback_listener {
                    done: frame_done_callback,
                };
            (self.wayland.wl_callback_add_listener)(
                frame_callback,
                &FRAME_CALLBACK_LISTENER,
                self as *mut _ as *mut _,
            );
        }
        // A shaped window: the input region rides on this commit.
        if let Some(rects) = self.cpu_backend.take_changed_shape() {
            self.apply_window_shape(&rects);
        }
        unsafe {
            (self.wayland.wl_surface_commit)(self.surface);
        }

        // Retire both requests HERE — the only point reached by a frame that
        // actually committed a buffer — and retire only what THIS frame saw. A
        // bare `= false` would erase a request raised while the frame was being
        // produced: `regenerate_layout` above runs user lifecycle callbacks, and
        // the CPU present retry raises the redraw request from inside this very
        // function, a few dozen lines up.
        self.common
            .clear_regeneration_unless_reraised(regen_epoch_seen);
        self.needs_redraw.retire_unless_reraised(redraw_epoch_seen);

        // ARM THE THROTTLE FIRST, then re-arm the fade. The order matters and the
        // old one recursed.
        //
        // On Wayland `request_redraw` is SYNCHRONOUS — it raises needs_redraw and
        // calls generate_frame_if_needed() directly (unlike X11, where it only
        // posts an Expose). With the fade re-arm running BEFORE
        // frame_callback_pending was set, the nested call found needs_work true
        // and the throttle still false, rendered a whole frame, reached this same
        // point, and re-armed again — an unbounded recursion / frame storm for as
        // long as a scrollbar was fading.
        //
        // Setting the latch first means the nested call hits the gate at the top
        // of this function and returns immediately, leaving the fade to advance
        // one frame per compositor callback, which is what it wanted.
        self.frame_callback_pending = true;
        self.frame_callback_armed_at = Some(std::time::Instant::now());

        // If any scrollbar is actively fading (0 < opacity < 1), or a layout
        // animation is still in flight, schedule another frame so the animation
        // runs to completion. Without this a DOM transition would advance only
        // on frames something ELSE happened to request, which looks like a
        // stutter rather than a slide.
        let needs_anim_frame = self
            .common
            .layout_window
            .as_ref()
            .map(|lw| lw.gpu_state_manager.scrollbar_fade_active || lw.needs_animation_frame())
            .unwrap_or(false);
        if needs_anim_frame {
            self.request_redraw();
        }
    }

    /// Set the mouse cursor for this window
    fn set_cursor(&mut self, cursor_type: azul_core::window::MouseCursorType) {
        // Only proceed if we have cursor functions loaded
        let cursor_theme_load = match self.wayland.wl_cursor_theme_load {
            Some(f) => f,
            None => return, // Cursor library not available
        };
        let cursor_theme_get = match self.wayland.wl_cursor_theme_get_cursor {
            Some(f) => f,
            None => return,
        };
        let cursor_image_get_buffer = match self.wayland.wl_cursor_image_get_buffer {
            Some(f) => f,
            None => return,
        };
        let pointer_set_cursor = match self.wayland.wl_pointer_set_cursor {
            Some(f) => f,
            None => return,
        };

        // Check if we have a pointer
        if self.pointer_state.pointer.is_null() {
            return;
        }

        // Load cursor theme once if not already loaded
        if self.pointer_state.cursor_theme.is_null() {
            self.pointer_state.cursor_theme = unsafe {
                cursor_theme_load(
                    std::ptr::null(), // Use default theme name
                    24,               // Cursor size
                    self.shm,         // Shared memory object
                )
            };
            if self.pointer_state.cursor_theme.is_null() {
                return; // Failed to load theme
            }
        }

        // Map MouseCursorType to Wayland cursor name
        let cursor_name = match cursor_type {
            azul_core::window::MouseCursorType::Default
            | azul_core::window::MouseCursorType::Arrow => "default",
            azul_core::window::MouseCursorType::Hand => "pointer",
            azul_core::window::MouseCursorType::Crosshair => "crosshair",
            azul_core::window::MouseCursorType::Text => "text",
            azul_core::window::MouseCursorType::Move => "move",
            azul_core::window::MouseCursorType::Wait => "wait",
            azul_core::window::MouseCursorType::Progress => "progress",
            azul_core::window::MouseCursorType::NotAllowed
            | azul_core::window::MouseCursorType::NoDrop => "not-allowed",
            azul_core::window::MouseCursorType::Help => "help",
            azul_core::window::MouseCursorType::ContextMenu => "context-menu",
            azul_core::window::MouseCursorType::Cell => "cell",
            azul_core::window::MouseCursorType::VerticalText => "vertical-text",
            azul_core::window::MouseCursorType::Alias => "alias",
            azul_core::window::MouseCursorType::Copy => "copy",
            azul_core::window::MouseCursorType::Grab => "grab",
            azul_core::window::MouseCursorType::Grabbing => "grabbing",
            azul_core::window::MouseCursorType::AllScroll => "all-scroll",
            azul_core::window::MouseCursorType::ZoomIn => "zoom-in",
            azul_core::window::MouseCursorType::ZoomOut => "zoom-out",
            azul_core::window::MouseCursorType::EResize => "e-resize",
            azul_core::window::MouseCursorType::NResize => "n-resize",
            azul_core::window::MouseCursorType::NeResize => "ne-resize",
            azul_core::window::MouseCursorType::NwResize => "nw-resize",
            azul_core::window::MouseCursorType::SResize => "s-resize",
            azul_core::window::MouseCursorType::SeResize => "se-resize",
            azul_core::window::MouseCursorType::SwResize => "sw-resize",
            azul_core::window::MouseCursorType::WResize => "w-resize",
            azul_core::window::MouseCursorType::EwResize => "ew-resize",
            azul_core::window::MouseCursorType::NsResize => "ns-resize",
            azul_core::window::MouseCursorType::NeswResize => "nesw-resize",
            azul_core::window::MouseCursorType::NwseResize => "nwse-resize",
            azul_core::window::MouseCursorType::ColResize => "col-resize",
            azul_core::window::MouseCursorType::RowResize => "row-resize",
        };

        // Get cursor from theme
        let cursor_name_cstr = match std::ffi::CString::new(cursor_name) {
            Ok(s) => s,
            Err(_) => return,
        };
        let cursor =
            unsafe { cursor_theme_get(self.pointer_state.cursor_theme, cursor_name_cstr.as_ptr()) };
        if cursor.is_null() {
            return; // Cursor not found in theme
        }

        // Get first image from cursor
        let cursor_struct = unsafe { &*cursor };
        if cursor_struct.image_count == 0 || cursor_struct.images.is_null() {
            return;
        }
        let image = unsafe { *cursor_struct.images };
        if image.is_null() {
            return;
        }

        // Get buffer from image
        let buffer = unsafe { cursor_image_get_buffer(image) };
        if buffer.is_null() {
            return;
        }

        // Create a dedicated surface for the cursor if we don't have one
        // This surface is reused across cursor changes for efficiency
        if self.pointer_state.cursor_surface.is_null() {
            self.pointer_state.cursor_surface =
                unsafe { (self.wayland.wl_compositor_create_surface)(self.compositor) };
            if self.pointer_state.cursor_surface.is_null() {
                return;
            }
        }

        // Attach buffer to cursor surface and commit
        unsafe {
            (self.wayland.wl_surface_attach)(self.pointer_state.cursor_surface, buffer, 0, 0);
            (self.wayland.wl_surface_damage)(
                self.pointer_state.cursor_surface,
                0,
                0,
                i32::MAX,
                i32::MAX,
            );
            (self.wayland.wl_surface_commit)(self.pointer_state.cursor_surface);
        }

        // Set cursor on pointer
        let image_struct = unsafe { &*image };
        unsafe {
            pointer_set_cursor(
                self.pointer_state.pointer,
                self.pointer_state.serial,
                self.pointer_state.cursor_surface,
                image_struct.hotspot_x as i32,
                image_struct.hotspot_y as i32,
            );
        }

        // No need to destroy cursor_surface - it's reused for the next cursor change
    }
}

/// Wayland frame callback - called when compositor is ready for next frame
extern "C" fn frame_done_callback(
    data: *mut std::ffi::c_void,
    callback: *mut defines::wl_callback,
    _callback_data: u32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.frame_callback_pending = false;
    window.frame_callback_armed_at = None;

    // The frame callback is one-shot: once the compositor delivers `done`, this
    // wl_callback proxy is dead. Destroy it here — otherwise EVERY frame leaks one
    // proxy. That is the "wl_callback@NNN still attached" flood seen on close (IDs
    // climbing into the hundreds), which culminates in libwayland's
    // "malloc(): mismatching next->prev_size" heap-corruption abort when the event
    // queue is torn down with all those dangling proxies still attached.
    if !callback.is_null() {
        unsafe {
            (window.wayland.wl_proxy_destroy)(callback as _);
        }
    }

    // If there are more changes pending, request another frame
    // The relayout-only request counts as work owed too. It used to be absent
    // from this gate, so a restyle queued by a timer callback (the only producer
    // that raised it alone) sat here until some unrelated event happened to
    // redraw the window. `request_relayout_only` now also raises the ordinary
    // regeneration request, and this gate names it explicitly so the intent
    // survives the next reader. Queued VirtualView re-renders (background
    // tile-fetch writebacks) are work owed for the same reason — they are
    // drained only inside generate_frame_if_needed.
    let vview_pending = window
        .common
        .layout_window
        .as_ref()
        .map(|lw| !lw.pending_virtual_view_updates.is_empty())
        .unwrap_or(false);
    if window.common.regeneration_pending()
        || window.common.relayout_only_pending()
        || window.common.resize_relayout_pending()
        || window.needs_redraw.pending()
        || vview_pending
    {
        window.generate_frame_if_needed();
    }
}

impl Drop for WaylandWindow {
    fn drop(&mut self) {
        // FIRST, before any wl_proxy_destroy below.
        //
        // `Drop::drop`'s BODY runs before the struct's FIELDS drop, so
        // `render_mode` — and everything hanging off it — used to be released
        // *after* this body had already destroyed `self.surface` and called
        // `wl_display_disconnect`. Both variants then touched freed objects:
        //
        //   * `RenderMode::Gpu` holds a `GlContext` whose Drop does
        //     eglDestroySurface / eglDestroyContext / eglTerminate and then
        //     `wl_egl_window_destroy`. The EGLSurface and the wl_egl_window
        //     both wrap `self.surface`, and eglTerminate wants the wl_display
        //     still connected. Under nvidia this faulted inside
        //     libnvidia-egl-wayland.so.1 — the reported teardown SIGSEGV.
        //
        //   * `RenderMode::Cpu` holds a `CpuFallbackState` whose Drop calls
        //     `wl_buffer_destroy` and `wl_shm_pool_destroy` on proxies of a
        //     display that has already been disconnected. Same defect, second
        //     code path, and it was never in the bug report because the crash
        //     only reproduced on the GPU backend.
        //
        // Replacing (rather than `ManuallyDrop`/`Option::take` gymnastics)
        // drops the old value at the end of this statement, while the surface
        // and display are both still alive. `Cpu(None)` owns nothing, so the
        // second drop at end of scope is a no-op.
        //
        // Do NOT try to solve this by walking `listener_proxies` here — that
        // Vec overlaps proxies the blocks below already destroy, and doing
        // both double-frees.
        // Order matters twice over, so do it explicitly:
        //
        //   1. `common.gl_context_ptr`'s Drop is not bookkeeping — it runs
        //      `glDeleteProgram` for the SVG, multicolor, FXAA and brush
        //      shaders (`azul_core::gl::GlContextPtrInner::drop`). Those are
        //      real GL calls and need a live, CURRENT context. It is a field
        //      of `self.common`, so it used to run after everything below,
        //      dispatching through function pointers into a library
        //      eglTerminate had already torn down — the crash landed in
        //      `?? ()` one frame under `delete_program`.
        //   2. Only then may the EGL context itself go.
        if let RenderMode::Gpu(ref gl_context, _) = self.render_mode {
            gl_context.make_current();
        }
        self.common.gl_context_ptr = OptionGlContextPtr::None;

        drop(std::mem::replace(
            &mut self.render_mode,
            RenderMode::Cpu(None),
        ));

        // Close all timerfd's
        for (_timer_id, fd) in std::mem::take(&mut self.timer_fds) {
            unsafe {
                libc::close(fd);
            }
        }
        if self.key_repeat_fd >= 0 {
            unsafe {
                libc::close(self.key_repeat_fd);
            }
        }

        unsafe {
            // The clipboard offer the compositor last handed us. Every
            // `wl_data_device.selection` releases its PREDECESSOR
            // (`events::destroy_data_offer`), so at teardown exactly one is
            // still held — the current one — and nothing else will ever
            // release it. It is what shows up in libwayland's
            // "queue destroyed while proxies still attached" report as
            // `wl_data_offer@4278190080`.
            //
            // Safe to do here, unlike the proxies the warning above is about:
            // offers are NEVER passed to `track_listener`, so they are not in
            // `listener_proxies` and cannot be double-freed by it. Released
            // FIRST, while the display is still connected.
            if !self.clipboard_offer.is_null() {
                events::destroy_data_offer_for_teardown(self, self.clipboard_offer);
                self.clipboard_offer = std::ptr::null_mut();
            }
            // Same for the primary-selection offer, for the same reason: each
            // `selection` event releases its PREDECESSOR, so exactly one is
            // still held at teardown and nothing else will ever release it.
            if !self.primary_selection_offer.is_null() {
                events::destroy_primary_offer(self, self.primary_selection_offer);
                self.primary_selection_offer = std::ptr::null_mut();
            }

            // Clean up text-input v3 resources
            if let Some(text_input) = self.text_input.take() {
                (self.wayland.wl_proxy_destroy)(text_input as _);
            }
            if let Some(manager) = self.text_input_manager.take() {
                (self.wayland.wl_proxy_destroy)(manager as _);
            }

            // Clean up KDE blur resources
            if let Some(blur) = self.current_blur.take() {
                (self.wayland.wl_proxy_destroy)(blur as _);
            }
            if let Some(blur_manager) = self.blur_manager.take() {
                (self.wayland.wl_proxy_destroy)(blur_manager as _);
            }

            // Clean up fractional-scale / viewporter resources
            if let Some(vp) = self.viewport.take() {
                wp_viewport_destroy(&self.wayland, vp);
            }
            if let Some(fs) = self.fractional_scale.take() {
                (self.wayland.wl_proxy_destroy)(fs as _);
            }
            if let Some(vpr) = self.viewporter.take() {
                (self.wayland.wl_proxy_destroy)(vpr as _);
            }
            if let Some(mgr) = self.fractional_scale_manager.take() {
                (self.wayland.wl_proxy_destroy)(mgr as _);
            }

            // Clean up xdg-decoration resources
            if let Some(deco) = self.toplevel_decoration.take() {
                (self.wayland.wl_proxy_destroy)(deco as _);
            }
            if let Some(deco_manager) = self.decoration_manager.take() {
                (self.wayland.wl_proxy_destroy)(deco_manager as _);
            }

            // Clean up cursor resources
            if !self.pointer_state.cursor_surface.is_null() {
                (self.wayland.wl_proxy_destroy)(self.pointer_state.cursor_surface as _);
                self.pointer_state.cursor_surface = std::ptr::null_mut();
            }
            if !self.pointer_state.cursor_theme.is_null() {
                if let Some(destroy_fn) = self.wayland.wl_cursor_theme_destroy {
                    destroy_fn(self.pointer_state.cursor_theme);
                }
                self.pointer_state.cursor_theme = std::ptr::null_mut();
            }

            // Clean up window surfaces
            if !self.xdg_toplevel.is_null() {
                (self.wayland.wl_proxy_destroy)(self.xdg_toplevel as _);
            }
            if !self.xdg_surface.is_null() {
                (self.wayland.wl_proxy_destroy)(self.xdg_surface as _);
            }
            if !self.surface.is_null() {
                (self.wayland.wl_proxy_destroy)(self.surface as _);
            }
            if !self.event_queue.is_null() {
                (self.wayland.wl_event_queue_destroy)(self.event_queue);
            }
            if !self.display.is_null() {
                (self.wayland.wl_display_disconnect)(self.display);
            }
        }
    }
}

// ── wp-viewporter marshal helpers (hand-rolled, like the xdg-decoration
//    requests: transmute wl_proxy_marshal[_constructor] with the interface
//    tables from defines.rs) ────────────────────────────────────────────────

/// `wp_viewporter.get_viewport` (opcode 1, "no"): one wp_viewport per
/// wl_surface. Returns None if the request failed.
unsafe fn wp_viewporter_get_viewport(
    wayland: &Wayland,
    viewporter: *mut defines::wp_viewporter,
    surface: *mut defines::wl_surface,
) -> Option<*mut defines::wp_viewport> {
    if viewporter.is_null() || surface.is_null() {
        return None;
    }
    type GetViewportCtor = unsafe extern "C" fn(
        *mut defines::wl_proxy,
        u32,
        *const defines::wl_interface,
        *mut c_void,
        *mut defines::wl_surface,
    ) -> *mut defines::wl_proxy;
    let f: GetViewportCtor = std::mem::transmute(wayland.wl_proxy_marshal_constructor);
    let vp = f(
        viewporter as *mut defines::wl_proxy,
        1, // opcode 1 = get_viewport (opcode 0 is `destroy`!)
        defines::get_wp_viewport_interface(),
        std::ptr::null_mut(), // NULL new_id placeholder ("n" arg)
        surface,
    );
    if vp.is_null() {
        None
    } else {
        Some(vp as *mut defines::wp_viewport)
    }
}

/// `wp_viewport.set_destination(width, height)` (opcode 2, "ii"): the surface
/// size in LOGICAL (surface-local) coordinates the buffer is scaled to.
/// Double-buffered state, applied on the next wl_surface.commit.
unsafe fn wp_viewport_set_destination(
    wayland: &Wayland,
    viewport: *mut defines::wp_viewport,
    logical_w: i32,
    logical_h: i32,
) {
    if viewport.is_null() || logical_w <= 0 || logical_h <= 0 {
        return; // 0/negative destination is a protocol error
    }
    type SetDestFn = unsafe extern "C" fn(*mut defines::wl_proxy, u32, i32, i32);
    let f: SetDestFn = std::mem::transmute(wayland.wl_proxy_marshal);
    f(viewport as *mut defines::wl_proxy, 2, logical_w, logical_h);
}

/// `wp_viewport.destroy` (opcode 0, "") + proxy teardown.
unsafe fn wp_viewport_destroy(wayland: &Wayland, viewport: *mut defines::wp_viewport) {
    if viewport.is_null() {
        return;
    }
    type DestroyFn = unsafe extern "C" fn(*mut defines::wl_proxy, u32);
    let f: DestroyFn = std::mem::transmute(wayland.wl_proxy_marshal);
    f(viewport as *mut defines::wl_proxy, 0);
    (wayland.wl_proxy_destroy)(viewport as *mut _);
}

impl CpuFallbackState {
    /// `physical_width`/`physical_height` are the BUFFER dimensions in device
    /// pixels (callers compute them via `cpu_buffer_spec` — logical × integer
    /// scale, or ceil(logical × fractional scale) with `scale` = 1 when
    /// viewport scaling is active). Buffers were once allocated at LOGICAL
    /// size while render_frame produced a physical-sized pixmap — on any
    /// scale>=2 output the linear copy sheared the image into garbage.
    /// `scale` is the integer value for `wl_surface.set_buffer_scale` (1 on
    /// non-HiDPI and ALWAYS 1 on the fractional/viewport path).
    fn new(
        wayland: &Rc<Wayland>,
        shm: *mut wl_shm,
        physical_width: i32,
        physical_height: i32,
        scale: i32,
    ) -> Result<Self, WindowError> {
        let scale = scale.max(1);
        let width = physical_width.max(1);
        let height = physical_height.max(1);
        let stride = width * 4;
        let size = stride * height * 2; // TWO buffers in one pool

        // Try memfd_create first (Linux 3.17+, glibc 2.27+)
        // Fall back to shm_open for older systems
        let fd = unsafe {
            #[cfg(target_os = "linux")]
            {
                // Try memfd_create via syscall if libc doesn't have it
                let result = libc::syscall(
                    libc::SYS_memfd_create,
                    CString::new("azul-fb").unwrap().as_ptr(),
                    1 as libc::c_int,
                ); // MFD_CLOEXEC = 1

                if result != -1 {
                    result as libc::c_int
                } else {
                    // Fallback to shm_open for older glibc
                    let name = CString::new(format!("/azul-fb-{}", std::process::id())).unwrap();
                    let fd = libc::shm_open(
                        name.as_ptr(),
                        libc::O_CREAT | libc::O_RDWR | libc::O_EXCL,
                        0o600,
                    );
                    if fd != -1 {
                        // Unlink immediately so it's cleaned up when closed
                        libc::shm_unlink(name.as_ptr());
                    }
                    fd
                }
            }
            #[cfg(not(target_os = "linux"))]
            {
                -1
            }
        };

        if fd == -1 {
            return Err(WindowError::PlatformError(
                "Failed to create shared memory".into(),
            ));
        }

        if unsafe { libc::ftruncate(fd, size as libc::off_t) } == -1 {
            unsafe { libc::close(fd) };
            return Err(WindowError::PlatformError("ftruncate failed".into()));
        }

        let data = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                size as usize,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                fd,
                0,
            )
        };

        if data == libc::MAP_FAILED {
            unsafe { libc::close(fd) };
            return Err(WindowError::PlatformError("mmap failed".into()));
        }

        // Create the pool BEFORE closing the fd - Wayland needs it open
        let pool = unsafe { (wayland.wl_shm_create_pool)(shm, fd, size) };
        // #27: prefer ABGR8888 (bytes R,G,B,A in LE memory = the CPU
        // renderer's output order) — presents become straight row copies and
        // the renderer can draw directly into a slot. ARGB8888 is the
        // mandatory fallback and keeps the R↔B swizzle.
        let format = if SHM_ABGR8888_ADVERTISED.load(core::sync::atomic::Ordering::Relaxed)
            && native_backbuffer_enabled()
        {
            WL_SHM_FORMAT_ABGR8888
        } else {
            WL_SHM_FORMAT_ARGB8888
        };
        let buf_bytes = (stride * height) as usize;
        let make_slot = |idx: usize| -> ShmSlot {
            let offset = idx * buf_bytes;
            let buffer = unsafe {
                (wayland.wl_shm_pool_create_buffer)(
                    pool,
                    offset as i32,
                    width,
                    height,
                    stride,
                    format,
                )
            };
            let busy = Box::into_raw(Box::new(false));
            unsafe {
                (wayland.wl_buffer_add_listener)(
                    buffer,
                    &WL_BUFFER_RELEASE_LISTENER,
                    busy as *mut c_void,
                );
            }
            ShmSlot {
                buffer,
                offset,
                busy,
                stale: Vec::new(),
                // A fresh slot has undefined content: full copy on first use.
                stale_overflow: true,
                valid: false,
            }
        };

        POOLS_CREATED.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        wl_trace!(
            "shm pool CREATE pool={pool:p} {width}x{height} stride={stride} scale={scale} \
             bytes={size} fd={fd} fmt={} — {}",
            if format == WL_SHM_FORMAT_ABGR8888 {
                "ABGR(native)"
            } else {
                "ARGB(legacy)"
            },
            pool_census()
        );
        log_debug!(
            LogCategory::Rendering,
            "[native-bb] shm pool {}x{}: {}",
            width,
            height,
            if format == WL_SHM_FORMAT_ABGR8888 {
                "ABGR8888 — renderer targets the slot directly"
            } else if !native_backbuffer_enabled() {
                "ARGB8888 (AZ_NATIVE_BACKBUFFER=0)"
            } else {
                "ARGB8888 + commit-swizzle — renderer targets the slot; damage rects \
                 converted in place (ABGR8888 not advertised at 8-bit)"
            }
        );

        Ok(Self {
            wayland: wayland.clone(),
            pool,
            slots: [make_slot(0), make_slot(1)],
            active: 0,
            data: data as *mut u8,
            pool_size: size as usize,
            width,
            height,
            stride,
            scale,
            format,
            fd, // Keep fd open - will be closed in Drop
            damage_rects: Vec::new(),
        })
    }

    /// Pick a buffer the compositor is NOT holding. Prefers the current
    /// `active` slot; returns None when both are busy (caller skips the
    /// attach this cycle and retries after the next frame callback/release).
    fn acquire_slot(&mut self) -> Option<usize> {
        let a = self.active;
        if unsafe { !*self.slots[a].busy } {
            return Some(a);
        }
        let b = 1 - a;
        if unsafe { !*self.slots[b].busy } {
            self.active = b;
            return Some(b);
        }
        None
    }

    /// The buffer that will be (or was last) attached.
    fn active_buffer(&self) -> *mut defines::wl_buffer {
        self.slots[self.active].buffer
    }

    /// Mutable pixels of `slot` (ARGB8888, physical px).
    fn slot_buffer_mut(&mut self, slot: usize) -> &mut [u8] {
        let buf_bytes = (self.stride * self.height) as usize;
        let off = self.slots[slot].offset;
        unsafe { std::slice::from_raw_parts_mut(self.data.add(off), buf_bytes) }
    }

    /// Get a mutable slice of the ACTIVE buffer as ARGB8888 pixels.
    fn pixel_buffer_mut(&mut self) -> &mut [u8] {
        self.slot_buffer_mut(self.active)
    }

    /// #27: the pool's byte order matches the renderer's — the renderer may
    /// draw directly into a slot, and copies (fallback frames) skip the
    /// swizzle.
    fn is_native(&self) -> bool {
        self.format == WL_SHM_FORMAT_ABGR8888
    }

    /// #32: ARGB8888 pool with native rendering — the renderer still writes
    /// R,G,B,A bytes directly into the slot; the commit converts ONLY this
    /// frame's damage rects in place (R↔B). Engages where the compositor
    /// never advertised ABGR8888 (KWin offers ABGR only at 10/16-bit
    /// depths). Slot bytes are ALWAYS in pool format after a commit,
    /// whichever path (native+swizzle or legacy copy) produced them.
    fn needs_commit_swizzle(&self) -> bool {
        self.format == WL_SHM_FORMAT_ARGB8888 && native_backbuffer_enabled()
    }

    /// Raw pointer to `slot`'s first pixel inside the pool mapping.
    fn slot_ptr(&mut self, slot: usize) -> *mut u8 {
        unsafe { self.data.add(self.slots[slot].offset) }
    }

    /// #27: bring `slot` up to the last-presented frame by copying the
    /// regions it missed from the OTHER slot. The renderer's incremental
    /// model requires its target to already hold the PREVIOUS frame — a slot
    /// that sat out a present only holds frame N−2. Reading the other slot
    /// while the compositor displays it is fine (only WRITES to an attached
    /// buffer violate the protocol), and the slots never overlap.
    fn catch_up_slot(&mut self, slot: usize) {
        let other = 1 - slot;
        if std::env::var("AZ_BB_DEBUG").is_ok() {
            eprintln!(
                "[bb] catch_up slot={} valid={} overflow={} stale={:?} other_valid={}",
                slot,
                self.slots[slot].valid,
                self.slots[slot].stale_overflow,
                self.slots[slot].stale.iter().take(4).collect::<Vec<_>>(),
                self.slots[other].valid,
            );
        }
        if self.slots[slot].stale_overflow {
            if self.slots[other].valid {
                let buf_bytes = (self.stride * self.height) as usize;
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        self.data.add(self.slots[other].offset),
                        self.data.add(self.slots[slot].offset),
                        buf_bytes,
                    );
                }
                self.slots[slot].valid = true;
                self.slots[slot].stale.clear();
                self.slots[slot].stale_overflow = false;
            }
            // No valid source (fresh pool): leave the debt marker set. The
            // first render after pool creation is a full repaint by
            // construction; the present path marks the slot valid then (or
            // trips the invalid-partial-present error if that law breaks).
            return;
        }
        let stride = self.stride.max(0) as usize;
        let (w, h) = (self.width.max(0), self.height.max(0));
        let stale = std::mem::take(&mut self.slots[slot].stale);
        for (x, y, rw, rh) in &stale {
            let x0 = (*x).clamp(0, w) as usize;
            let y0 = (*y).clamp(0, h) as usize;
            let x1 = x.saturating_add(*rw).clamp(0, w) as usize;
            let y1 = y.saturating_add(*rh).clamp(0, h) as usize;
            if x1 <= x0 || y1 <= y0 {
                continue;
            }
            let n = (x1 - x0) * 4;
            for row in y0..y1 {
                let off = row * stride + x0 * 4;
                unsafe {
                    std::ptr::copy_nonoverlapping(
                        self.data.add(self.slots[other].offset + off),
                        self.data.add(self.slots[slot].offset + off),
                        n,
                    );
                }
            }
        }
    }

    fn draw_blue(&mut self) {
        let native = self.is_native();
        let slice = self.pixel_buffer_mut();
        for chunk in slice.chunks_exact_mut(4) {
            if native {
                chunk[0] = 0x00; // R
                chunk[1] = 0x00; // G
                chunk[2] = 0xFF; // B
                chunk[3] = 0xFF; // A (ABGR pool: RGBA byte order)
            } else {
                chunk[0] = 0xFF; // Blue
                chunk[1] = 0x00; // Green
                chunk[2] = 0x00; // Red
                chunk[3] = 0xFF; // Alpha (ARGB format)
            }
        }
    }
}

impl Drop for CpuFallbackState {
    fn drop(&mut self) {
        POOLS_DESTROYED.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        wl_trace!(
            "shm pool DESTROY pool={:p} {}x{} bytes={} — {}",
            self.pool,
            self.width,
            self.height,
            self.pool_size,
            pool_census()
        );
        unsafe {
            for slot in &mut self.slots {
                if !slot.buffer.is_null() {
                    (self.wayland.wl_buffer_destroy)(slot.buffer);
                }
                if !slot.busy.is_null() {
                    // The proxy is destroyed above, so no release event can
                    // fire into this flag afterwards.
                    drop(Box::from_raw(slot.busy));
                    slot.busy = std::ptr::null_mut();
                }
            }
            if !self.pool.is_null() {
                (self.wayland.wl_shm_pool_destroy)(self.pool);
            }
            if !self.data.is_null() {
                libc::munmap(self.data as *mut _, self.pool_size);
            }
            // Close fd AFTER destroying pool - Wayland protocol requires it to stay open
            if self.fd != -1 {
                libc::close(self.fd);
            }
        }
    }
}

// Helper methods for WaylandWindow to get display information
impl WaylandWindow {
    /// Resize the rendering surface to match compositor's requested size
    pub(super) fn resize_surface(&mut self, width: i32, height: i32) {
        // Physical buffer size + integer buffer scale (fractional-aware);
        // computed before the render_mode borrow below.
        let (buf_w, buf_h, scale) = self.cpu_buffer_spec(width, height);
        // A mouse drag delivers one configure PER FRAME, and each one lands
        // here and rebuilds the whole shm pool: shm_open + ftruncate + mmap of
        // ~w*h*4*2 bytes, plus the munmap/close of the old one. That is the
        // cost this timing exists to expose — the E2E path takes it three
        // times, a drag takes it hundreds of times (RSS map §29 counted 1 279
        // pools in one interactive session).
        let t0 = std::time::Instant::now();
        match &mut self.render_mode {
            RenderMode::Gpu(gl_context, _gl_functions) => {
                gl_context.resize(&self.wayland, width, height);
            }
            RenderMode::Cpu(cpu_opt) => {
                if !self.shm.is_null() {
                    drop(cpu_opt.take());
                    match CpuFallbackState::new(&self.wayland, self.shm, buf_w, buf_h, scale) {
                        Ok(new_state) => {
                            *cpu_opt = Some(new_state);
                            // Fresh buffer = undefined content: the next
                            // present must copy + damage the full frame.
                            self.os_present_requested = true;
                        }
                        Err(e) => {
                            // NOT log_error!: in a `debug-server` build that
                            // macro only reaches the debug server's in-memory
                            // queue, so a failed buffer rebuild — which leaves
                            // the window with NO buffer at all — was invisible
                            // on stderr. This one always prints.
                            eprintln!("[WL] CPU buffer resize FAILED: {e:?}");
                            log_error!(
                                LogCategory::Rendering,
                                "[Wayland] CPU buffer resize failed: {:?}",
                                e
                            );
                        }
                    }
                }
            }
        }
        wl_trace!(
            "resize_surface logical={width}x{height} buffer={buf_w}x{buf_h} scale={scale} \
             took={:.2}ms — {}",
            t0.elapsed().as_secs_f64() * 1000.0,
            pool_census()
        );
    }

    /// Check timers and threads, trigger callbacks if needed.
    /// This is called on every poll_event() to simulate timer ticks.
    /// If any timer/thread callback requested a visual update, raise needs_redraw
    /// and attempt to render immediately (if no frame callback is pending).
    fn check_timers_and_threads(&mut self) {
        use super::super::common::event::PlatformWindow;
        if self.process_timers_and_threads() {
            self.needs_redraw.raise();
            self.generate_frame_if_needed();
        }

        // A runtime light/dark switch. There is NO Wayland protocol for this —
        // the xdg-desktop-portal `Settings` interface is the mechanism — so the
        // same watcher serves X11 and Wayland alike. `observed_system_theme` is
        // a relaxed atomic load; the blocking D-Bus round trip that feeds it
        // runs on a watcher thread, never on this one.
        if super::system_style::adopt_observed_theme(&mut self.common) {
            let _ = self.process_window_events(0);
            self.common
                .request_regeneration(azul_core::callbacks::RelayoutReason::ThemeChange);
            self.needs_redraw.raise();
            self.generate_frame_if_needed();
        }
    }

    /// Returns the logical size of the window's surface.
    pub fn get_window_size_logical(&self) -> (i32, i32) {
        let size = self.common.current_window_state().size.get_logical_size();
        (size.width as i32, size.height as i32)
    }

    /// Returns the physical size of the window by applying the scale factor.
    pub fn get_window_size_physical(&self) -> (i32, i32) {
        let size = self.common.current_window_state().size.get_physical_size();
        (size.width as i32, size.height as i32)
    }

    /// Returns the DPI scale factor for the window.
    pub fn get_scale_factor(&self) -> f32 {
        self.common
            .current_window_state()
            .size
            .get_hidpi_factor()
            .inner
            .get()
    }

    /// Calculate the current scale factor based on active outputs
    /// Returns the highest scale factor among all outputs the window is on
    pub fn calculate_current_scale_factor(&self) -> f32 {
        if self.current_outputs.is_empty() {
            return 1.0;
        }

        let mut max_scale = 1.0f32;
        for output_ptr in &self.current_outputs {
            if let Some(monitor_state) = self.known_outputs.iter().find(|m| m.proxy == *output_ptr)
            {
                max_scale = max_scale.max(monitor_state.scale as f32);
            }
        }

        max_scale
    }

    /// Get the current display/monitor the window is on
    ///
    /// Uses the CLI-detected monitors from display::get_displays() and matches them
    /// with the wl_output surfaces the window is currently on.
    ///
    /// Returns the first monitor if the window is on multiple monitors,
    /// or the primary monitor if tracking hasn't been initialized yet.
    pub fn get_current_monitor(&self) -> Option<crate::desktop::display::DisplayInfo> {
        let all_displays = crate::desktop::display::get_displays();

        if all_displays.is_empty() {
            return None;
        }

        // If we don't have any tracked outputs yet, return the primary display
        if self.current_outputs.is_empty() {
            return all_displays.into_iter().find(|d| d.is_primary);
        }

        // Try to match the first current output with our known outputs
        let current_output_ptr = self.current_outputs.first()?;

        // Find the index of this output in our known outputs list
        let output_index = self
            .known_outputs
            .iter()
            .position(|known| &known.proxy == current_output_ptr)?;

        // Return the display at that index, or the primary if out of range
        all_displays
            .get(output_index)
            .cloned()
            .or_else(|| all_displays.into_iter().find(|d| d.is_primary))
    }

    /// Get the monitor ID the window is currently on
    ///
    /// This returns a stable MonitorId based on monitor properties (name, position, size).
    /// The ID remains stable even if monitors are added/removed, as long as the physical
    /// monitor configuration doesn't change.
    pub fn get_current_monitor_id(&self) -> azul_core::window::MonitorId {
        if self.current_outputs.is_empty() {
            return azul_core::window::MonitorId::PRIMARY;
        }

        // Find the MonitorState for the first current output
        let current_output_ptr = self.current_outputs.first().copied();

        if let Some(ptr) = current_output_ptr {
            if let Some((index, monitor_state)) = self
                .known_outputs
                .iter()
                .enumerate()
                .find(|(_, m)| m.proxy == ptr)
            {
                return monitor_state.get_monitor_id(index);
            }
        }

        azul_core::window::MonitorId::PRIMARY
    }
}

// WaylandPopup Implementation

impl WaylandPopup {
    /// Create a new popup window using xdg_popup protocol
    ///
    /// This creates a popup surface that is properly managed by the Wayland compositor.
    /// The popup will be positioned relative to the parent window using xdg_positioner.
    ///
    /// # Arguments
    /// * `parent` - Parent WaylandWindow
    /// * `anchor_rect` - Rectangle on parent surface where popup is anchored (logical coords)
    /// * `popup_size` - Size of popup window (logical coords)
    /// * `options` - Window creation options (for rendering setup)
    ///
    /// # Returns
    /// * `Ok(WaylandPopup)` - Successfully created popup
    /// * `Err(String)` - Error message
    pub fn new(
        parent: &WaylandWindow,
        anchor_rect: azul_core::geom::LogicalRect,
        popup_size: azul_core::geom::LogicalSize,
        edge: azul_core::transient::TransientAnchor,
        options: WindowCreateOptions,
    ) -> Result<Self, String> {
        use crate::desktop::shell2::linux::wayland::defines::*;

        let wayland = parent.wayland.clone();
        let xkb = parent.xkb.clone();

        // 1. Create xdg_positioner
        let positioner = unsafe { (wayland.xdg_wm_base_create_positioner)(parent.xdg_wm_base) };

        if positioner.is_null() {
            return Err("Failed to create xdg_positioner".to_string());
        }

        // 2. Configure positioner
        unsafe {
            // Set popup size
            (wayland.xdg_positioner_set_size)(
                positioner,
                popup_size.width as i32,
                popup_size.height as i32,
            );

            // Set anchor rectangle (where popup is triggered from on parent surface)
            (wayland.xdg_positioner_set_anchor_rect)(
                positioner,
                anchor_rect.origin.x as i32,
                anchor_rect.origin.y as i32,
                anchor_rect.size.width as i32,
                anchor_rect.size.height as i32,
            );

            // Which corner/edge of the anchor rect the popup hangs off, and
            // which way it grows from there. A `<transient-window anchor=…>`
            // says so; a menu opens at its trigger's bottom-right like before.
            use azul_core::transient::TransientAnchor;
            let (anchor, gravity) = match edge {
                TransientAnchor::Bottom => (
                    XDG_POSITIONER_ANCHOR_BOTTOM_LEFT,
                    XDG_POSITIONER_GRAVITY_BOTTOM_RIGHT,
                ),
                TransientAnchor::Top => (
                    XDG_POSITIONER_ANCHOR_TOP_LEFT,
                    XDG_POSITIONER_GRAVITY_TOP_RIGHT,
                ),
                TransientAnchor::Left => (
                    XDG_POSITIONER_ANCHOR_TOP_LEFT,
                    XDG_POSITIONER_GRAVITY_BOTTOM_LEFT,
                ),
                TransientAnchor::Right => (
                    XDG_POSITIONER_ANCHOR_TOP_RIGHT,
                    XDG_POSITIONER_GRAVITY_BOTTOM_RIGHT,
                ),
                TransientAnchor::Cursor => (
                    XDG_POSITIONER_ANCHOR_BOTTOM_RIGHT,
                    XDG_POSITIONER_GRAVITY_BOTTOM_RIGHT,
                ),
            };
            (wayland.xdg_positioner_set_anchor)(positioner, anchor);
            (wayland.xdg_positioner_set_gravity)(positioner, gravity);

            // Allow compositor to flip/slide if popup would overflow screen
            (wayland.xdg_positioner_set_constraint_adjustment)(
                positioner,
                XDG_POSITIONER_CONSTRAINT_ADJUSTMENT_FLIP_X
                    | XDG_POSITIONER_CONSTRAINT_ADJUSTMENT_FLIP_Y
                    | XDG_POSITIONER_CONSTRAINT_ADJUSTMENT_SLIDE_X
                    | XDG_POSITIONER_CONSTRAINT_ADJUSTMENT_SLIDE_Y,
            );
        }

        // 3. Create wl_surface
        let surface = unsafe { (wayland.wl_compositor_create_surface)(parent.compositor) };

        if surface.is_null() {
            unsafe {
                (wayland.xdg_positioner_destroy)(positioner);
            }
            return Err("Failed to create wl_surface for popup".to_string());
        }

        // 4. Create xdg_surface
        let xdg_surface =
            unsafe { (wayland.xdg_wm_base_get_xdg_surface)(parent.xdg_wm_base, surface) };

        if xdg_surface.is_null() {
            unsafe {
                (wayland.wl_proxy_destroy)(surface as *mut _);
                (wayland.xdg_positioner_destroy)(positioner);
            }
            return Err("Failed to create xdg_surface for popup".to_string());
        }

        // 5. Get xdg_popup role
        let xdg_popup = unsafe {
            (wayland.xdg_surface_get_popup)(
                xdg_surface,
                parent.xdg_surface, // Parent xdg_surface
                positioner,
            )
        };

        if xdg_popup.is_null() {
            unsafe {
                (wayland.wl_proxy_destroy)(xdg_surface as *mut _);
                (wayland.wl_proxy_destroy)(surface as *mut _);
                (wayland.xdg_positioner_destroy)(positioner);
            }
            return Err("Failed to create xdg_popup".to_string());
        }

        // 6. Create listener context that will be passed to callbacks
        // This context must live as long as the listeners are active
        let listener_context = Box::new(PopupListenerContext {
            wayland: wayland.clone(),
            xdg_surface,
            xdg_popup,
            configured: std::cell::Cell::new(false),
            dismissed: std::cell::Cell::new(false),
        });
        let listener_context_ptr = Box::into_raw(listener_context);

        // 7. Add xdg_surface listener (configure events). 'static: the proxy stores
        // the pointer, so a stack-local would be a use-after-free.
        static POPUP_XDG_SURFACE_LISTENER: xdg_surface_listener = xdg_surface_listener {
            configure: popup_xdg_surface_configure,
        };

        unsafe {
            (wayland.xdg_surface_add_listener)(
                xdg_surface,
                &POPUP_XDG_SURFACE_LISTENER,
                listener_context_ptr as *mut _,
            );
        }

        // 8. Add xdg_popup listener
        static POPUP_LISTENER: xdg_popup_listener = xdg_popup_listener {
            configure: popup_configure,
            popup_done,
        };

        unsafe {
            (wayland.xdg_popup_add_listener)(
                xdg_popup,
                &POPUP_LISTENER,
                listener_context_ptr as *mut _,
            );
        }

        // 9. Grab pointer for exclusive input (using parent's last serial)
        unsafe {
            (wayland.xdg_popup_grab)(xdg_popup, parent.seat, parent.pointer_state.serial);
        }

        // 10. Commit surface to make popup visible
        unsafe {
            (wayland.wl_surface_commit)(surface);
        }

        // 11. Create window state — the popup's own `CommonWindowState`, so it
        // is a `PlatformWindow` like every toplevel.
        let current_window_state = FullWindowState {
            title: options.window_state.title.clone(),
            size: options.window_state.size,
            position: options.window_state.position,
            flags: options.window_state.flags,
            theme: parent.common.current_window_state().theme,
            debug_state: parent.common.current_window_state().debug_state,
            keyboard_state: azul_core::window::KeyboardState::default(),
            mouse_state: azul_core::window::MouseState::default(),
            touch_state: azul_core::window::TouchState::default(),
            ime_position: parent.common.current_window_state().ime_position,
            platform_specific_options: options.window_state.platform_specific_options.clone(),
            renderer_options: parent.common.current_window_state().renderer_options,
            background_color: options.window_state.background_color,
            layout_callback: options.window_state.layout_callback.clone(),
            close_callback: options.window_state.close_callback.clone(),
            monitor_id: parent.common.current_window_state().monitor_id,
            window_id: options.window_state.window_id.clone(),
            // The xdg_popup grab gives it the keyboard; report it focused so
            // the engine's focus-loss dismiss sees a true→false edge later.
            window_focused: true,
            active_route: azul_core::resources::OptionRouteMatch::None,
        };
        // A popup is a CHILD: share the PARENT's already-warmed manager, which
        // is both the warmest option and the one whose embedded (icon) faces the
        // popup is most likely to need. Falls back to the app-level manager.
        let mut layout_window = match parent.common.layout_window.as_ref() {
            Some(parent_lw) => {
                LayoutWindow::from_font_manager(parent_lw.font_manager.clone_shared())
            }
            None => crate::desktop::shell2::common::layout::layout_window_sharing_fonts(
                parent.resources.font_manager.as_ref(),
                &parent.resources.fc_cache,
            )
            .map_err(|e| format!("LayoutWindow::new failed: {e:?}"))?,
        };
        layout_window.routes = parent.resources.config.routes.clone();
        // Seed with the parent window's image map so css-id / url("...")
        // images inside the popup resolve (whole-map seed at creation).
        if let Some(parent_lw) = parent.common.layout_window.as_ref() {
            layout_window.seed_image_id_map(parent_lw.image_id_map_snapshot());
        }
        let mut common = event::CommonWindowState::new(
            current_window_state,
            parent.common.fc_cache.clone(),
            parent.resources.system_style.clone(),
            parent.common.app_data.clone(),
            parent.resources.undo_manager.clone(),
        );
        common.layout_window = Some(layout_window);
        common.cpu_hit_tester = Some(azul_layout::headless::CpuHitTester::new());
        common.gl_context_ptr = None.into();
        common.regen = crate::desktop::shell2::common::event::RegenerationState::idle_initial();
        Ok(Self {
            wayland,
            xkb,
            display: parent.display,
            parent_surface: parent.surface,
            surface,
            xdg_surface,
            xdg_popup,
            positioner,
            compositor: parent.compositor,
            seat: parent.seat,
            event_queue: parent.event_queue,
            keyboard_state: events::WaylandKeyboardState::new(),
            pointer_state: events::PointerState::new(),
            is_open: true,
            configured: false,
            listener_context: listener_context_ptr,
            common,
            pending_window_creates: Vec::new(),
            pressed_key_vks: std::collections::BTreeMap::new(),
            render_mode: RenderMode::Cpu(None),
            frame_callback_pending: false,
            resources: parent.resources.clone(),
            shm: parent.shm,
            viewporter: parent.viewporter,
            preferred_scale_120: parent.preferred_scale_120,
            viewport: None,
            rendered: false,
            needs_repaint: false,
            #[cfg(feature = "cpurender")]
            cpu_backend: crate::desktop::shell2::headless::CpuBackend::new(),
        })
    }

    /// Close the popup window
    pub fn close(&mut self) {
        if self.is_open {
            unsafe {
                // The viewport must go before its wl_surface (protocol).
                if let Some(vp) = self.viewport.take() {
                    wp_viewport_destroy(&self.wayland, vp);
                }

                if !self.xdg_popup.is_null() {
                    (self.wayland.xdg_popup_destroy)(self.xdg_popup);
                    self.xdg_popup = std::ptr::null_mut();
                }

                if !self.xdg_surface.is_null() {
                    (self.wayland.wl_proxy_destroy)(self.xdg_surface as *mut _);
                    self.xdg_surface = std::ptr::null_mut();
                }

                if !self.surface.is_null() {
                    (self.wayland.wl_proxy_destroy)(self.surface as *mut _);
                    self.surface = std::ptr::null_mut();
                }

                if !self.positioner.is_null() {
                    (self.wayland.xdg_positioner_destroy)(self.positioner);
                    self.positioner = std::ptr::null_mut();
                }
            }

            self.is_open = false;
        }
    }

    /// `true` once the compositor has sent the initial xdg_surface configure.
    fn is_configured(&self) -> bool {
        if self.listener_context.is_null() {
            return false;
        }
        unsafe { (*self.listener_context).configured.get() }
    }

    /// `true` once the compositor dismissed the popup (click-outside / popup_done).
    fn is_dismissed(&self) -> bool {
        if self.listener_context.is_null() {
            return false;
        }
        unsafe { (*self.listener_context).dismissed.get() }
    }

    /// Render the menu DOM into the popup's shm buffer and present it.
    ///
    /// Must run AFTER the compositor has configured the popup (`is_configured`),
    /// per xdg-shell (a buffer may only be attached once the surface is
    /// configured). Renders once; a popup menu's content is static.
    /// Mark the popup as needing another paint.
    ///
    /// Call this whenever anything the popup DRAWS changes — hover, selection,
    /// a scroll inside it, a submenu opening. Without a caller this flag is just
    /// a different kind of silence, so if you add popup state, add the call.
    pub fn request_repaint(&mut self) {
        self.needs_repaint = true;
    }

    fn render_if_ready(&mut self) {
        if !self.is_open || !self.is_configured() {
            return;
        }
        // Paint the first frame, and thereafter only when something asked.
        if self.rendered && !self.needs_repaint {
            return;
        }
        if self.surface.is_null() || self.shm.is_null() {
            return;
        }

        let logical_w = self
            .common
            .current_window_state()
            .size
            .dimensions
            .width
            .max(1.0);
        let logical_h = self
            .common
            .current_window_state()
            .size
            .dimensions
            .height
            .max(1.0);
        let dpi_factor = {
            let d = self.common.current_window_state().size.dpi as f32 / 96.0;
            if d <= 0.0 {
                1.0
            } else {
                d
            }
        };
        let buf_w = (logical_w * dpi_factor).ceil() as i32;
        let buf_h = (logical_h * dpi_factor).ceil() as i32;
        crate::plog_info!(
            "[wayland-popup] configured -> rendering menu: {:.0}x{:.0} logical, {}x{} px (dpi {:.2})",
            logical_w, logical_h, buf_w, buf_h, dpi_factor
        );

        // Fractional viewport scaling (inherited from the parent window):
        // buffer at exact physical size, buffer scale 1, wp_viewport maps it
        // to the logical popup size at attach.
        let fractional = self.viewporter.is_some() && self.preferred_scale_120.is_some();

        // Lazily create the CPU shm buffer (sized in physical pixels).
        if matches!(self.render_mode, RenderMode::Cpu(None)) {
            // Integer path: the popup renders at dpi_factor into a
            // physical-sized pixmap; give the buffer the matching integer
            // scale (rounded up to a multiple of it) so the compositor
            // doesn't display it dpi× oversized (set_buffer_scale at attach).
            let scale = if fractional {
                1
            } else {
                dpi_factor.round().max(1.0) as i32
            };
            let (phys_w, phys_h) = if fractional {
                (buf_w, buf_h)
            } else {
                (
                    ((buf_w + scale - 1) / scale) * scale,
                    ((buf_h + scale - 1) / scale) * scale,
                )
            };
            match CpuFallbackState::new(&self.wayland, self.shm, phys_w, phys_h, scale) {
                Ok(state) => self.render_mode = RenderMode::Cpu(Some(state)),
                Err(e) => {
                    log_error!(
                        LogCategory::Rendering,
                        "[Wayland popup] failed to create CPU buffer: {:?}",
                        e
                    );
                    return;
                }
            }
        }

        // Build + lay out the menu DOM (CPU path only — popups never use WebRender).
        #[cfg(feature = "cpurender")]
        let laid_out = self.ensure_layout();

        if let RenderMode::Cpu(Some(cpu_state)) = &mut self.render_mode {
            let mut painted = false;

            #[cfg(feature = "cpurender")]
            {
                if laid_out {
                    // Shared per-frame content preparation (journal clock, image
                    // callbacks through the content chokepoint, scrollbar cache).
                    if let Some(lw) = self.common.layout_window.as_mut() {
                        lw.prepare_frame_cpu();
                    }
                    if let Some(ref layout_window) = self.common.layout_window {
                        self.cpu_backend
                            .sync_window_flags(&layout_window.current_window_state);
                        self.cpu_backend.render_frame(
                            layout_window,
                            &layout_window.renderer_resources,
                            logical_w,
                            logical_h,
                            dpi_factor,
                        );
                        if let Some(ref pixmap) = self.cpu_backend.last_frame {
                            // #27: popups never arm the native target, but
                            // their pool shares the global format choice — an
                            // ABGR pool takes rows verbatim.
                            let straight = cpu_state.is_native();
                            let buf = cpu_state.pixel_buffer_mut();
                            let src = pixmap.data();
                            let copy_len = buf.len().min(src.len());
                            if straight {
                                buf[..copy_len].copy_from_slice(&src[..copy_len]);
                            } else {
                                // RGBA -> ARGB8888: swap R and B for Wayland.
                                let mut i = 0;
                                while i + 3 < copy_len {
                                    buf[i] = src[i + 2]; // B
                                    buf[i + 1] = src[i + 1]; // G
                                    buf[i + 2] = src[i]; // R
                                    buf[i + 3] = src[i + 3]; // A
                                    i += 4;
                                }
                            }
                            painted = true;
                        }
                    }
                }
            }

            if !painted {
                // Fallback so the popup still maps + grabs even if layout failed.
                cpu_state.draw_blue();
            }

            unsafe {
                (self.wayland.wl_surface_attach)(self.surface, cpu_state.active_buffer(), 0, 0);
                unsafe { *cpu_state.slots[cpu_state.active].busy = true };
                let surface_version =
                    (self.wayland.wl_proxy_get_version)(self.surface as *mut defines::wl_proxy);
                if fractional {
                    // Physical-sized buffer + wp_viewport → logical size.
                    // Buffer scale MUST stay 1 in this mode.
                    if self.viewport.is_none() {
                        if let Some(vpr) = self.viewporter {
                            self.viewport =
                                wp_viewporter_get_viewport(&self.wayland, vpr, self.surface);
                        }
                    }
                    if let Some(vp) = self.viewport {
                        wp_viewport_set_destination(
                            &self.wayland,
                            vp,
                            logical_w.ceil() as i32,
                            logical_h.ceil() as i32,
                        );
                    }
                } else if surface_version >= 3 && cpu_state.scale > 1 {
                    // Integer HiDPI: announce the buffer scale, or the
                    // physical-sized buffer displays scale× too large.
                    (self.wayland.wl_surface_set_buffer_scale)(self.surface, cpu_state.scale);
                }
                if surface_version >= 4 {
                    (self.wayland.wl_surface_damage_buffer)(
                        self.surface,
                        0,
                        0,
                        cpu_state.width,
                        cpu_state.height,
                    );
                } else {
                    (self.wayland.wl_surface_damage)(
                        self.surface,
                        0,
                        0,
                        cpu_state.width,
                        cpu_state.height,
                    );
                }
                (self.wayland.wl_surface_commit)(self.surface);
                (self.wayland.wl_display_flush)(self.display);
            }
        }

        self.rendered = true;
        self.needs_repaint = false;
    }

    /// Build the LayoutWindow (lazily) and run a layout pass for the menu DOM.
    /// Returns `true` if a layout result for the root DOM is available.
    /// Lay the popup out through the shared `PlatformWindow::regenerate_layout`
    /// (lifecycle events, the transient-window mailbox poll, dismissal) when
    /// nothing is laid out yet or a regeneration was requested. Returns whether
    /// a root layout exists afterwards.
    #[cfg(feature = "cpurender")]
    fn ensure_layout(&mut self) -> bool {
        use crate::desktop::shell2::common::event::PlatformWindow as _;
        use azul_core::dom::DomId;
        let has_root = self
            .common
            .layout_window
            .as_ref()
            .is_some_and(|lw| lw.layout_results.contains_key(&DomId::ROOT_ID));
        if !has_root || self.common.regeneration_pending() {
            if let Err(e) = self.regenerate_layout() {
                log_error!(
                    LogCategory::Layout,
                    "[Wayland popup] regenerate_layout failed: {}",
                    e
                );
                return false;
            }
        }
        self.common
            .layout_window
            .as_ref()
            .is_some_and(|lw| lw.layout_results.contains_key(&DomId::ROOT_ID))
    }

    /// Apply a `process_window_events` result the way a toplevel does: an
    /// incremental relayout right here, a regeneration request for the next
    /// `drive_active_popup`, and a repaint. `ShouldRegenerateDomAllWindows`
    /// (a callback mutated shared app state) also wakes every toplevel.
    fn apply_event_result(&mut self, result: ProcessEventResult) {
        use crate::desktop::shell2::common::event::PlatformWindow as _;
        match result {
            ProcessEventResult::ShouldIncrementalRelayout => {
                // The dispatching entry point: relayout, CPU hit-tester
                // rebuild (the popup has no WebRender one), NodeResized.
                let mut debug_messages = None;
                if let Err(e) = self.incremental_relayout_dispatching(
                    crate::desktop::shell2::common::event::IncrementalRelayout::Restyle,
                    &mut debug_messages,
                ) {
                    log_warn!(
                        LogCategory::Layout,
                        "[Wayland popup] incremental relayout failed: {}",
                        e
                    );
                }
                self.common.request_relayout_only();
                self.needs_repaint = true;
            }
            ProcessEventResult::ShouldRegenerateDomCurrentWindow
            | ProcessEventResult::ShouldRegenerateDomAllWindows
            | ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                if result == ProcessEventResult::ShouldRegenerateDomAllWindows {
                    self.request_regeneration_all_windows();
                }
                self.common
                    .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
                self.needs_repaint = true;
            }
            ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            | ProcessEventResult::ShouldReRenderCurrentWindow => {
                self.needs_repaint = true;
            }
            ProcessEventResult::DoNothing => {}
        }
    }

    /// The pointer entered the popup surface at `pos` (popup-local logical).
    pub fn pointer_enter(&mut self, pos: LogicalPosition) {
        use crate::desktop::shell2::common::event::PlatformWindow as _;
        self.snapshot_window_state_baseline("wayland.popup.pointer_enter");
        self.common.mouse_state_mut().cursor_position = CursorPosition::InWindow(pos);
        self.update_hit_test_at(pos);
        let r = self.process_window_events(0);
        self.apply_event_result(r);
    }

    /// The pointer moved over the popup surface (coords already popup-local;
    /// the xdg_popup grab delivers them that way).
    pub fn pointer_motion(&mut self, pos: LogicalPosition) {
        use crate::desktop::shell2::common::event::PlatformWindow as _;
        self.snapshot_window_state_baseline("wayland.popup.pointer_motion");
        self.common.mouse_state_mut().cursor_position = CursorPosition::InWindow(pos);
        self.update_hit_test_at(pos);
        // Gestures (a tear-off drag on the popup's grip) need the samples.
        let buttons = self.pressed_button_state();
        self.record_input_sample(pos, buttons, false, false, None);
        let r = self.process_window_events(0);
        self.apply_event_result(r);
    }

    /// The gesture manager's button bitfield from the popup's mouse state.
    fn pressed_button_state(&self) -> u8 {
        use crate::desktop::shell2::common::event::{
            BUTTON_STATE_LEFT, BUTTON_STATE_MIDDLE, BUTTON_STATE_NONE, BUTTON_STATE_RIGHT,
        };
        let ms = &self.common.current_window_state().mouse_state;
        let mut state = BUTTON_STATE_NONE;
        if ms.left_down {
            state |= BUTTON_STATE_LEFT;
        }
        if ms.right_down {
            state |= BUTTON_STATE_RIGHT;
        }
        if ms.middle_down {
            state |= BUTTON_STATE_MIDDLE;
        }
        state
    }

    /// A button went down/up over the popup. `button` is the evdev code
    /// (0x110 left, 0x111 right, 0x112 middle), `state` 1 = pressed.
    pub fn pointer_button(&mut self, button: u32, state: u32) {
        use crate::desktop::shell2::common::event::PlatformWindow as _;
        self.snapshot_window_state_baseline("wayland.popup.pointer_button");
        let down = state == 1;
        {
            let ms = self.common.mouse_state_mut();
            match button {
                0x110 => ms.left_down = down,
                0x111 => ms.right_down = down,
                0x112 => ms.middle_down = down,
                _ => {}
            }
        }
        if let CursorPosition::InWindow(pos) = self
            .common
            .current_window_state()
            .mouse_state
            .cursor_position
        {
            self.update_hit_test_at(pos);
            let buttons = self.pressed_button_state();
            self.record_input_sample(pos, buttons, down, !down, None);
        }
        let r = self.process_window_events(0);
        self.apply_event_result(r);
    }

    /// The pointer left the popup surface.
    pub fn pointer_leave(&mut self) {
        use crate::desktop::shell2::common::event::PlatformWindow as _;
        self.snapshot_window_state_baseline("wayland.popup.pointer_leave");
        let last = self
            .common
            .current_window_state()
            .mouse_state
            .cursor_position
            .get_position()
            .unwrap_or(LogicalPosition::zero());
        self.common.mouse_state_mut().cursor_position = CursorPosition::OutOfWindow(last);
        let r = self.process_window_events(0);
        self.apply_event_result(r);
    }

    /// A key event while the popup holds the keyboard (the grab). The parent
    /// resolved the virtual keycode and the typed text through its xkb state;
    /// the popup only applies them to ITS keyboard state and runs the pipeline,
    /// so Escape / typing into a field / shortcuts inside the popup all work.
    pub fn key_event(
        &mut self,
        key: u32,
        virtual_keycode: Option<azul_core::window::VirtualKeyCode>,
        is_pressed: bool,
        text: Option<&str>,
    ) {
        use crate::desktop::shell2::common::event::PlatformWindow as _;
        self.snapshot_window_state_baseline("wayland.popup.key");
        apply_key_state_change(
            self.common.keyboard_state_mut(),
            &mut self.pressed_key_vks,
            key,
            virtual_keycode,
            is_pressed,
        );
        if let (true, Some(t)) = (is_pressed, text) {
            if let Some(lw) = self.common.layout_window.as_mut() {
                lw.record_text_input(t);
            }
        }
        let r = self.process_window_events(0);
        self.apply_event_result(r);
    }

    /// The popup asked to close (Escape, focus loss, the parent's mailbox, a
    /// menu item's `close_requested`) — the parent's drive loop reads this.
    pub fn close_requested(&self) -> bool {
        self.common.current_window_state().flags.close_requested
    }
}

impl PlatformWindow for WaylandPopup {
    fn capture_screen_for_eyedropper(&mut self) -> Option<crate::desktop::eyedropper::Screenshot> {
        let scale = self
            .common
            .current_window_state()
            .size
            .get_hidpi_factor()
            .inner
            .get();
        crate::desktop::eyedropper::wayland::capture(scale)
    }

    fn apply_window_shape(&mut self, rects: &[azul_layout::cpurender::ShapeRect]) {
        let scale = self
            .common
            .current_window_state()
            .size
            .get_hidpi_factor()
            .inner
            .get();
        apply_input_region_from_shape(&self.wayland, self.compositor, self.surface, rects, scale);
    }

    fn regenerate_layout_once(
        &mut self,
    ) -> Result<crate::desktop::shell2::common::layout::LayoutRegenerateResult, String> {
        let relayout_reason = self.common.take_relayout_reason();
        let resources = self.resources.clone();
        let borrows = self.common.layout_borrows();
        let layout_window = borrows.layout_window.ok_or("No layout window")?;
        let mut debug_messages = None;
        let result = crate::desktop::shell2::common::layout::regenerate_layout(
            layout_window,
            borrows.app_data,
            borrows.current_window_state,
            borrows.renderer_resources,
            borrows.gl_context_ptr,
            borrows.fc_cache,
            &resources.font_registry,
            borrows.system_style,
            &resources.icon_provider,
            &mut debug_messages,
            relayout_reason,
        )?;
        // The popup has no WebRender hit-tester; the CPU one answers clicks.
        if let Some(ref mut cpu_ht) = self.common.cpu_hit_tester {
            if let Some(lw) = self.common.layout_window.as_ref() {
                cpu_ht
                    .rebuild_from_layout_with_gpu(&lw.layout_results, Some(&lw.gpu_state_manager));
            }
        }
        self.needs_repaint = true;
        Ok(result)
    }

    impl_platform_window_getters!(common);

    fn get_raw_window_handle(&self) -> RawWindowHandle {
        RawWindowHandle::Wayland(WaylandHandle {
            surface: self.surface as *mut c_void,
            display: self.display as *mut c_void,
        })
    }

    fn prepare_callback_invocation(&mut self) -> event::InvokeSingleCallbackBorrows<'_> {
        let window_handle = self.get_raw_window_handle();
        let borrows = self.common.layout_borrows();
        event::InvokeSingleCallbackBorrows {
            layout_window: borrows
                .layout_window
                .expect("Layout window must exist for callback invocation"),
            window_handle,
            gl_context_ptr: borrows.gl_context_ptr,
            fc_cache_clone: (**borrows.fc_cache).clone(),
            system_style: borrows.system_style.clone(),
            previous_window_state: borrows.previous_window_state,
            current_window_state: borrows.current_window_state,
            renderer_resources: borrows.renderer_resources,
        }
    }

    fn start_timer(&mut self, timer_id: usize, timer: azul_layout::timer::Timer) {
        if let Some(lw) = self.common.layout_window.as_mut() {
            lw.timers
                .insert(azul_core::task::TimerId { id: timer_id }, timer);
        }
    }

    fn stop_timer(&mut self, timer_id: usize) {
        if let Some(lw) = self.common.layout_window.as_mut() {
            lw.timers.remove(&azul_core::task::TimerId { id: timer_id });
        }
    }

    fn start_thread_poll_timer(&mut self) {}

    fn stop_thread_poll_timer(&mut self) {}

    fn add_threads(
        &mut self,
        threads: std::collections::BTreeMap<azul_core::task::ThreadId, azul_layout::thread::Thread>,
    ) {
        if let Some(lw) = self.common.layout_window.as_mut() {
            for (id, thread) in threads {
                lw.threads.insert(id, thread);
            }
        }
    }

    fn remove_threads(
        &mut self,
        thread_ids: &std::collections::BTreeSet<azul_core::task::ThreadId>,
    ) {
        if let Some(lw) = self.common.layout_window.as_mut() {
            for id in thread_ids {
                lw.threads.remove(id);
            }
        }
    }

    fn queue_window_create(&mut self, options: WindowCreateOptions) {
        // Drained by the parent into its own queue (a submenu replaces the
        // popup; a nested transient window becomes the active popup).
        self.pending_window_creates.push(options);
    }

    fn request_regeneration_all_windows(&mut self) {
        // The popup is not in the registry: every registered window is
        // "another" one — its parent included, which is the point.
        for wid in super::registry::get_all_window_ids() {
            if let Some(wptr) = unsafe { super::registry::get_window(wid) } {
                if let super::LinuxWindow::Wayland(w) = unsafe { &mut *wptr } {
                    w.common
                        .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
                    w.request_redraw();
                }
            }
        }
    }

    fn show_menu_from_callback(
        &mut self,
        menu: &azul_core::menu::Menu,
        position: LogicalPosition,
        anchor: Option<azul_core::geom::LogicalRect>,
    ) {
        // A context menu opened from inside a popup: build the menu window
        // like the parent does and let the parent drain it.
        let options = crate::desktop::menu::show_menu(
            menu.clone(),
            self.resources.system_style.clone(),
            LogicalPosition::zero(),
            anchor,
            Some(position),
            None,
        );
        self.pending_window_creates.push(options);
    }

    fn show_tooltip_from_callback(&mut self, _text: &str, _position: LogicalPosition) {}

    fn hide_tooltip_from_callback(&mut self) {}

    fn sync_window_state(&mut self) {}

    fn window_follows_position_changes(&self) -> bool {
        // `xdg_wm_base` is bound at v1: no `xdg_popup.reposition`, so a
        // popup cannot be moved once mapped. The tear-off drag still works
        // (the drop is computed arithmetically); the popup just does not
        // travel with the pointer until it becomes a toplevel.
        false
    }
}

impl Drop for WaylandPopup {
    fn drop(&mut self) {
        self.close();

        // Free the listener context if it was allocated
        if !self.listener_context.is_null() {
            unsafe {
                let _ = Box::from_raw(self.listener_context);
                self.listener_context = std::ptr::null_mut();
            }
        }
    }
}

// XDG Popup Listener Callbacks

/// Context passed to popup listener callbacks
struct PopupListenerContext {
    wayland: Rc<Wayland>,
    xdg_surface: *mut defines::xdg_surface,
    xdg_popup: *mut defines::xdg_popup,
    /// Set by the xdg_surface configure callback once the compositor has
    /// configured the popup, so the parent knows it may attach a buffer.
    configured: std::cell::Cell<bool>,
    /// Set by the xdg_popup popup_done callback (click-outside / compositor
    /// dismiss). The parent drops the popup on its next loop iteration.
    dismissed: std::cell::Cell<bool>,
}

/// xdg_surface configure callback for popup
extern "C" fn popup_xdg_surface_configure(
    data: *mut c_void,
    xdg_surface: *mut defines::xdg_surface,
    serial: u32,
) {
    if data.is_null() {
        log_error!(
            LogCategory::Platform,
            "[xdg_popup] configure: null data pointer!"
        );
        return;
    }

    unsafe {
        let ctx = &*(data as *const PopupListenerContext);
        // Acknowledge configure using the Wayland instance from context
        (ctx.wayland.xdg_surface_ack_configure)(xdg_surface, serial);
        // Signal the parent that the popup may now attach its first buffer.
        ctx.configured.set(true);
    }
}

// IME Position Management

impl WaylandWindow {
    /// Sync ime_position from window state to OS
    /// Sync IME position to OS (Wayland with text-input-v3 or GTK fallback)
    pub fn sync_ime_position_to_os(&self) {
        use azul_core::window::ImePosition;

        if let ImePosition::Initialized(rect) = self.common.current_window_state().ime_position {
            // Use text-input v3 protocol if available (native Wayland IME)
            if let Some(text_input) = self.text_input {
                if self.text_input_enabled {
                    // set_cursor_rectangle: opcode 6, args (x, y, width, height)
                    type MarshalFn = unsafe extern "C" fn(
                        *mut defines::wl_proxy,
                        u32, // opcode
                        i32,
                        i32,
                        i32,
                        i32,
                    );
                    let marshal: MarshalFn =
                        unsafe { std::mem::transmute(self.wayland.wl_proxy_marshal) };
                    unsafe {
                        marshal(
                            text_input as *mut defines::wl_proxy,
                            defines::ZWP_TEXT_INPUT_V3_SET_CURSOR_RECTANGLE,
                            rect.origin.x as i32,
                            rect.origin.y as i32,
                            rect.size.width.max(1.0) as i32,
                            rect.size.height.max(1.0) as i32,
                        );
                    }
                    // commit the pending state
                    type CommitFn = unsafe extern "C" fn(*mut defines::wl_proxy, u32);
                    let commit: CommitFn =
                        unsafe { std::mem::transmute(self.wayland.wl_proxy_marshal) };
                    unsafe {
                        commit(
                            text_input as *mut defines::wl_proxy,
                            defines::ZWP_TEXT_INPUT_V3_COMMIT,
                        );
                        (self.wayland.wl_display_flush)(self.display);
                    }
                }
            }

            // Fallback to GTK IM context (works across X11 and Wayland)
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

    /// Check if a contenteditable is focused and enable/disable text-input v3 accordingly.
    /// Called after every layout pass.
    fn sync_text_input_v3_focus_state(&mut self) {
        let has_contenteditable_focus = self
            .common
            .layout_window
            .as_ref()
            .map(|lw| lw.text_edit_manager.has_active_editing())
            .unwrap_or(false);

        if has_contenteditable_focus && !self.text_input_enabled {
            self.text_input_v3_enable(); // enable() calls send_surrounding_text() before commit
        } else if !has_contenteditable_focus && self.text_input_enabled {
            self.text_input_v3_disable();
        }
    }

    /// Send surrounding text context to IME so it can provide context-aware completions.
    fn send_surrounding_text(&self) {
        let text_input = match self.text_input {
            Some(ti) if self.text_input_enabled => ti,
            _ => return,
        };

        // Get the actual text content and cursor byte offset from the focused node
        let (text, cursor_byte, anchor_byte) = match self.common.layout_window.as_ref() {
            Some(lw) => {
                let mc = match lw.text_edit_manager.multi_cursor.as_ref() {
                    Some(mc) => mc,
                    None => return,
                };
                let node_id = match mc.node_id.node.into_crate_internal() {
                    Some(id) => id,
                    None => return,
                };
                let dom_id = mc.node_id.dom;

                // Get current text (checks dirty_text_nodes first)
                let content = lw.get_text_before_textinput(dom_id, node_id);
                let text_str = lw.extract_text_from_inline_content(&content);

                // Compute global byte offset: sum prior runs + offset in current run
                let (cursor_byte, anchor_byte) = match mc.get_primary() {
                    Some(identified) => {
                        let calc_global_offset =
                            |cursor: &azul_core::selection::TextCursor| -> i32 {
                                let run_idx = cursor.cluster_id.source_run as usize;
                                let byte_in_run = cursor.cluster_id.start_byte_in_run as usize;
                                let mut global = 0usize;
                                for (i, item) in content.iter().enumerate() {
                                    if i >= run_idx {
                                        break;
                                    }
                                    match item {
                                        azul_layout::text3::cache::InlineContent::Text(r) => {
                                            global += r.text.len()
                                        }
                                        azul_layout::text3::cache::InlineContent::Space(_) => {
                                            global += 1
                                        }
                                        azul_layout::text3::cache::InlineContent::LineBreak(_) => {
                                            global += 1
                                        }
                                        azul_layout::text3::cache::InlineContent::Tab {
                                            ..
                                        } => global += 1,
                                        _ => {}
                                    }
                                }
                                (global + byte_in_run) as i32
                            };
                        match &identified.selection {
                            azul_core::selection::Selection::Cursor(c) => {
                                let off = calc_global_offset(c);
                                (off, off)
                            }
                            azul_core::selection::Selection::Range(r) => {
                                (calc_global_offset(&r.start), calc_global_offset(&r.end))
                            }
                        }
                    }
                    None => (0, 0),
                };

                // Never hand the wire an oversized string — see
                // trim_surrounding_text: beyond ~4 KB the message is not
                // truncated, the compositor disconnects us.
                let (window, cursor_in_window, anchor_in_window) = trim_surrounding_text(
                    &text_str,
                    cursor_byte.max(0) as usize,
                    anchor_byte.max(0) as usize,
                );
                match std::ffi::CString::new(&text_str[window]) {
                    Ok(cstr) => (cstr, cursor_in_window, anchor_in_window),
                    Err(_) => (std::ffi::CString::new("").unwrap(), 0, 0),
                }
            }
            None => return,
        };

        // set_surrounding_text: opcode 3, args (text: string, cursor: int, anchor: int)
        type SurroundingFn =
            unsafe extern "C" fn(*mut defines::wl_proxy, u32, *const std::ffi::c_char, i32, i32);
        let set_surrounding: SurroundingFn =
            unsafe { std::mem::transmute(self.wayland.wl_proxy_marshal) };
        unsafe {
            set_surrounding(
                text_input as *mut defines::wl_proxy,
                defines::ZWP_TEXT_INPUT_V3_SET_SURROUNDING_TEXT,
                text.as_ptr(),
                cursor_byte,
                anchor_byte,
            );
        }
        // Note: commit is called by the caller (enable or sync_ime_position)
    }

    /// Enable text-input v3 for IME input (call when contenteditable gains focus)
    pub fn text_input_v3_enable(&mut self) {
        if let Some(text_input) = self.text_input {
            if self.text_input_enabled {
                return;
            }
            type MarshalFn = unsafe extern "C" fn(*mut defines::wl_proxy, u32);
            let marshal: MarshalFn = unsafe { std::mem::transmute(self.wayland.wl_proxy_marshal) };
            unsafe {
                // enable (opcode 1)
                marshal(
                    text_input as *mut defines::wl_proxy,
                    defines::ZWP_TEXT_INPUT_V3_ENABLE,
                );
                // set_content_type (opcode 5): hint=COMPLETION|SPELLCHECK, purpose=NORMAL
                type ContentTypeFn = unsafe extern "C" fn(*mut defines::wl_proxy, u32, u32, u32);
                let content_type: ContentTypeFn =
                    std::mem::transmute(self.wayland.wl_proxy_marshal);
                content_type(
                    text_input as *mut defines::wl_proxy,
                    defines::ZWP_TEXT_INPUT_V3_SET_CONTENT_TYPE,
                    defines::ZWP_TEXT_INPUT_V3_CONTENT_HINT_COMPLETION
                        | defines::ZWP_TEXT_INPUT_V3_CONTENT_HINT_SPELLCHECK,
                    defines::ZWP_TEXT_INPUT_V3_CONTENT_PURPOSE_NORMAL,
                );
            }
            self.text_input_enabled = true;
            // Send surrounding text BEFORE commit so IME gets context
            self.send_surrounding_text();
            unsafe {
                marshal(
                    text_input as *mut defines::wl_proxy,
                    defines::ZWP_TEXT_INPUT_V3_COMMIT,
                );
                (self.wayland.wl_display_flush)(self.display);
            }
            log_debug!(
                LogCategory::Platform,
                "[Wayland] text_input_v3: enabled for contenteditable focus"
            );
        }
    }

    /// Disable text-input v3 (call when contenteditable loses focus)
    pub fn text_input_v3_disable(&mut self) {
        if let Some(text_input) = self.text_input {
            if !self.text_input_enabled {
                return;
            }
            type MarshalFn = unsafe extern "C" fn(*mut defines::wl_proxy, u32);
            let marshal: MarshalFn = unsafe { std::mem::transmute(self.wayland.wl_proxy_marshal) };
            unsafe {
                // disable (opcode 2)
                marshal(
                    text_input as *mut defines::wl_proxy,
                    defines::ZWP_TEXT_INPUT_V3_DISABLE,
                );
                // commit (opcode 7)
                marshal(
                    text_input as *mut defines::wl_proxy,
                    defines::ZWP_TEXT_INPUT_V3_COMMIT,
                );
                (self.wayland.wl_display_flush)(self.display);
            }
            self.text_input_enabled = false;
            // Clear preedit state
            if let Some(ref mut lw) = self.common.layout_window {
                lw.text_edit_manager.clear_preedit();
            }
            log_debug!(
                LogCategory::Platform,
                "[Wayland] text_input_v3: disabled on blur"
            );
        }
    }

    /// Show a tooltip at the given position (Wayland implementation using subsurface)
    fn show_tooltip(&mut self, text: &str, position: azul_core::geom::LogicalPosition) {
        // Create tooltip if needed
        if self.tooltip.is_none() {
            let subcompositor = match self.subcompositor {
                Some(sc) => sc,
                None => {
                    log_warn!(
                        LogCategory::Platform,
                        "[Wayland] Subcompositor not available for tooltips"
                    );
                    return;
                }
            };

            match tooltip::TooltipWindow::new(
                self.wayland.clone(),
                self.display,
                self.surface,
                self.compositor,
                self.shm,
                subcompositor,
                self.viewporter,
                self.common.fc_cache.clone(),
            ) {
                Ok(tooltip_window) => {
                    self.tooltip = Some(tooltip_window);
                }
                Err(e) => {
                    log_error!(
                        LogCategory::Platform,
                        "[Wayland] Failed to create tooltip: {}",
                        e
                    );
                    return;
                }
            }
        }

        // Show tooltip
        let dpi = azul_core::resources::DpiScaleFactor::new(
            self.common.current_window_state().size.dpi as f32 / 96.0,
        );
        if let Some(tooltip) = self.tooltip.as_mut() {
            if let Err(e) = tooltip.show(text, position, dpi) {
                log_error!(
                    LogCategory::Platform,
                    "[Wayland] Failed to show tooltip: {}",
                    e
                );
            }
        }
    }

    /// Hide the tooltip (Wayland implementation)
    fn hide_tooltip(&mut self) {
        if let Some(tooltip) = self.tooltip.as_mut() {
            let _ = tooltip.hide();
        }
    }

    /// Set the window to always be on top (Wayland - not supported)
    ///
    /// Wayland does not provide a direct mechanism for applications to set themselves
    /// as "always on top". This is a deliberate design decision to prevent applications
    /// from interfering with the user's desktop environment.
    ///
    /// Workarounds using layer-shell (zwlr_layer_shell_v1) exist but require compositor
    /// support and are typically reserved for system components (panels, notifications, etc.).
    fn set_is_top_level(&mut self, _is_top_level: bool) {
        // Wayland does not support always-on-top for regular application windows
        // This would require zwlr_layer_shell_v1 which is compositor-specific
        log_debug!(
            LogCategory::Platform,
            "[Wayland] set_is_top_level not supported - Wayland does not allow applications to \
             force window stacking"
        );
    }

    /// Prevent the system from sleeping (Wayland implementation using D-Bus)
    ///
    /// Uses org.freedesktop.portal.Inhibit D-Bus API (XDG Desktop Portal).
    /// This is the standard way for Wayland applications to inhibit system sleep.
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
                        "[Wayland] Failed to load D-Bus library"
                    );
                    log_warn!(
                        LogCategory::Platform,
                        "[Wayland] System sleep prevention not available"
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
                            "[Wayland] Failed to connect to D-Bus session bus"
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
                // Create method call: org.freedesktop.ScreenSaver.Inhibit
                // (This works on both X11 and Wayland)
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
                        "[Wayland] Failed to create D-Bus method call"
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
                        "[Wayland] D-Bus ScreenSaver.Inhibit failed"
                    );
                    (dbus_lib.dbus_error_free)(&mut error);
                    return;
                }

                if reply.is_null() {
                    log_error!(
                        LogCategory::Platform,
                        "[Wayland] D-Bus ScreenSaver.Inhibit returned no reply"
                    );
                    return;
                }

                // Parse reply to get the cookie (uint32)
                let mut reply_iter: dbus::DBusMessageIter = std::mem::zeroed();
                if (dbus_lib.dbus_message_iter_init)(reply, &mut reply_iter) == 0 {
                    log_error!(
                        LogCategory::Platform,
                        "[Wayland] D-Bus reply has no arguments"
                    );
                    (dbus_lib.dbus_message_unref)(reply);
                    return;
                }

                let arg_type = (dbus_lib.dbus_message_iter_get_arg_type)(&mut reply_iter);
                if arg_type != dbus::DBUS_TYPE_UINT32 {
                    log_error!(
                        LogCategory::Platform,
                        "[Wayland] D-Bus reply has wrong type: expected uint32"
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
                    "[Wayland] System sleep prevented (cookie: {})",
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
                        "[Wayland] Failed to load D-Bus library"
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
                        "[Wayland] Failed to create D-Bus method call"
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
                        "[Wayland] D-Bus ScreenSaver.UnInhibit failed"
                    );
                    (dbus_lib.dbus_error_free)(&mut error);
                    return;
                }

                if !reply.is_null() {
                    (dbus_lib.dbus_message_unref)(reply);
                }

                log_info!(
                    LogCategory::Platform,
                    "[Wayland] System sleep allowed (cookie: {})",
                    cookie
                );
            }
        }
    }
}

/// xdg_popup configure callback
extern "C" fn popup_configure(
    data: *mut c_void,
    _xdg_popup: *mut defines::xdg_popup,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
) {
    if data.is_null() {
        log_error!(
            LogCategory::Platform,
            "[xdg_popup] configure: null data pointer!"
        );
        return;
    }

    log_debug!(
        LogCategory::Platform,
        "[xdg_popup] configure: x={}, y={}, width={}, height={}",
        x,
        y,
        width,
        height
    );
    // Compositor has positioned the popup
    // We could resize the popup here if needed
}

/// xdg_popup done callback - popup was dismissed by compositor
extern "C" fn popup_done(data: *mut c_void, _xdg_popup: *mut defines::xdg_popup) {
    if data.is_null() {
        log_error!(
            LogCategory::Platform,
            "[xdg_popup] popup_done: null data pointer!"
        );
        return;
    }

    log_debug!(
        LogCategory::Platform,
        "[xdg_popup] popup_done: compositor dismissed popup"
    );

    unsafe {
        let ctx = &*(data as *const PopupListenerContext);
        // Only SIGNAL dismissal. The parent WaylandWindow drops the popup on its
        // next loop iteration, and WaylandPopup::close() owns proxy destruction —
        // destroying the proxies here too would double-free them.
        ctx.dismissed.set(true);
    }
}

/// Tests for [`trim_surrounding_text`] — the guard between the IME context
/// and libwayland's hard per-message size cap. An oversized string here is a
/// CONNECTION-FATAL protocol failure, not a cosmetic truncation, so these pin
/// the budget, the UTF-8 boundary safety, and the offset rebasing.
#[cfg(test)]
mod primary_selection_tests {
    use azul_core::events::MouseButton;

    use super::primary_paste_wanted;

    /// Middle-click paste is the RELEASE, matching X11: the press is what
    /// moved the caret to the click point, so pasting on it would insert at
    /// wherever the caret happened to be before.
    ///
    /// NEGATIVE CONTROL: drop the `!is_down` term from `primary_paste_wanted`
    /// — the press case then wants a paste and this fails.
    #[test]
    fn middle_click_pastes_on_release_only() {
        assert!(primary_paste_wanted(MouseButton::Middle, false, true));
        assert!(!primary_paste_wanted(MouseButton::Middle, true, true));
    }

    /// The read waits on the selection OWNER, another process. A middle click
    /// with nothing editable focused must not pay for it — and
    /// `record_text_input` would drop the text anyway.
    #[test]
    fn a_middle_click_outside_an_editable_costs_nothing() {
        assert!(!primary_paste_wanted(MouseButton::Middle, false, false));
    }

    /// Only the middle button. Left ends a selection (which CLAIMS the primary
    /// selection) and right opens the context menu.
    #[test]
    fn the_other_buttons_never_paste() {
        assert!(!primary_paste_wanted(MouseButton::Left, false, true));
        assert!(!primary_paste_wanted(MouseButton::Right, false, true));
    }
}

#[cfg(test)]
mod trim_surrounding_text_tests {
    use super::trim_surrounding_text;

    const BUDGET: usize = 3800;

    #[test]
    fn small_text_passes_through_untouched() {
        let t = "hello world";
        let (r, c, a) = trim_surrounding_text(t, 5, 2);
        assert_eq!(r, 0..t.len());
        assert_eq!((c, a), (5, 2));
    }

    #[test]
    fn huge_text_is_bounded_and_offsets_stay_inside() {
        // The miniword crash case: a document paragraph far over the budget.
        let t = "x".repeat(60_000);
        let (r, c, a) = trim_surrounding_text(&t, 30_000, 29_990);
        assert!(
            r.end - r.start <= BUDGET,
            "window {} exceeds the budget",
            r.end - r.start
        );
        assert!(c >= 0 && (c as usize) <= r.end - r.start);
        assert!(a >= 0 && (a as usize) <= r.end - r.start);
        // The cursor's absolute position must be preserved relative to the window.
        assert_eq!(r.start + c as usize, 30_000);
        assert_eq!(r.start + a as usize, 29_990);
    }

    #[test]
    fn multibyte_boundaries_are_never_split() {
        // 4-byte scalars everywhere: any non-boundary cut would panic at the
        // slice (or send invalid UTF-8 — its own protocol violation).
        let t = "\u{1F600}".repeat(2_000); // 8000 bytes
        let (r, _c, _a) = trim_surrounding_text(&t, 4_000, 4_000);
        assert!(t.is_char_boundary(r.start) && t.is_char_boundary(r.end));
        assert!(r.end - r.start <= BUDGET);
        let _slice = &t[r.clone()]; // must not panic
    }

    #[test]
    fn selection_wider_than_the_budget_keeps_the_cursor() {
        let t = "y".repeat(20_000);
        let (r, c, a) = trim_surrounding_text(&t, 15_000, 1_000); // 14 KB selection
        assert!(r.end - r.start <= BUDGET);
        assert_eq!(r.start + c as usize, 15_000, "cursor must stay exact");
        // Anchor cannot fit — clamped to the window, still valid.
        assert!((a as usize) <= r.end - r.start);
    }

    #[test]
    fn out_of_range_offsets_are_clamped_not_fatal() {
        let t = "abc";
        let (r, c, a) = trim_surrounding_text(t, 999, 999);
        assert_eq!(r, 0..3);
        assert_eq!((c, a), (3, 3));
    }

    #[test]
    fn cursor_at_the_very_end_of_a_huge_text() {
        let t = "z".repeat(10_000);
        let (r, c, _a) = trim_surrounding_text(&t, 10_000, 10_000);
        assert!(r.end - r.start <= BUDGET);
        assert_eq!(
            r.end, 10_000,
            "window must reach the end to contain the cursor"
        );
        assert_eq!(r.start + c as usize, 10_000);
    }
}

#[cfg(test)]
mod wayland_input_state_tests {
    use azul_core::{
        events::MouseButton,
        window::{KeyboardState, MouseState, VirtualKeyCode},
    };

    use super::{
        apply_key_state_change, axis_frame_delta, axis_source_is_trackpad, set_mouse_button_down,
        WHEEL_TICK_PIXELS, WL_AXIS_SOURCE_CONTINUOUS, WL_AXIS_SOURCE_FINGER, WL_AXIS_SOURCE_WHEEL,
    };

    type PressedKeyVks = std::collections::BTreeMap<u32, VirtualKeyCode>;

    /// evdev keycodes (`wl_keyboard.key` reports these raw, xkb adds 8).
    const KEY_BACKSPACE: u32 = 14;
    const KEY_LEFTCTRL: u32 = 29;
    const KEY_Q: u32 = 16;

    fn press(
        state: &mut KeyboardState,
        map: &mut PressedKeyVks,
        key: u32,
        vk: Option<VirtualKeyCode>,
    ) {
        apply_key_state_change(state, map, key, vk, true);
    }

    fn release(
        state: &mut KeyboardState,
        map: &mut PressedKeyVks,
        key: u32,
        vk: Option<VirtualKeyCode>,
    ) {
        apply_key_state_change(state, map, key, vk, false);
    }

    /// `VirtualKeyUp` is derived from `previous.is_some() && current.is_none()`,
    /// so a release that leaves the code standing emits no KeyUp at all.
    ///
    /// NEGATIVE CONTROL: deleting the
    /// `keyboard_state.current_virtual_keycode = OptionVirtualKeyCode::None;`
    /// line from the release arm leaves `Some(Back)` and fails the second
    /// assertion.
    #[test]
    fn a_release_clears_the_current_keycode() {
        let mut state = KeyboardState::default();
        let mut map = PressedKeyVks::new();

        press(
            &mut state,
            &mut map,
            KEY_BACKSPACE,
            Some(VirtualKeyCode::Back),
        );
        assert_eq!(
            state.current_virtual_keycode.into_option(),
            Some(VirtualKeyCode::Back)
        );

        release(
            &mut state,
            &mut map,
            KEY_BACKSPACE,
            Some(VirtualKeyCode::Back),
        );
        assert_eq!(
            state.current_virtual_keycode.into_option(),
            None,
            "a stale Some(vk) makes VirtualKeyUp unreachable"
        );
        assert!(state.pressed_virtual_keycodes.as_ref().is_empty());
        assert!(state.pressed_scancodes.as_ref().is_empty());
        assert!(map.is_empty());
    }

    /// The other half of the same defect: with the code never cleared, the
    /// SECOND discrete press of the same key produces no `None → Some` delta
    /// and therefore no KeyDown — "backspace only works every other tap".
    ///
    /// NEGATIVE CONTROL: same deletion as above — the intermediate `None`
    /// assertion goes red.
    #[test]
    fn tapping_one_key_twice_produces_two_separate_downs() {
        let mut state = KeyboardState::default();
        let mut map = PressedKeyVks::new();

        for tap in 0..2 {
            press(
                &mut state,
                &mut map,
                KEY_BACKSPACE,
                Some(VirtualKeyCode::Back),
            );
            assert_eq!(
                state.current_virtual_keycode.into_option(),
                Some(VirtualKeyCode::Back),
                "tap {tap}"
            );
            release(
                &mut state,
                &mut map,
                KEY_BACKSPACE,
                Some(VirtualKeyCode::Back),
            );
            assert_eq!(
                state.current_virtual_keycode.into_option(),
                None,
                "tap {tap} must end at None so the next press is a fresh transition"
            );
        }
    }

    /// A keysym the shared table does not know is NO key, never a wrong one.
    /// The Wayland backend used to answer `VirtualKeyCode::Escape` for every
    /// unmapped keysym, so typing `ö` dismissed menus.
    ///
    /// NEGATIVE CONTROL: `keyboard_state.current_virtual_keycode =
    /// virtual_keycode.into();` changed to
    /// `= Some(VirtualKeyCode::Escape).into();` fails the first assertion.
    #[test]
    fn an_unmapped_keysym_invents_no_key() {
        let mut state = KeyboardState::default();
        let mut map = PressedKeyVks::new();

        press(&mut state, &mut map, 47, None);
        assert_eq!(state.current_virtual_keycode.into_option(), None);
        assert!(state.pressed_virtual_keycodes.as_ref().is_empty());
        assert!(map.is_empty(), "nothing to undo means nothing recorded");
        assert_eq!(
            state.pressed_scancodes.as_ref().len(),
            1,
            "the PHYSICAL key list is still written — key repeat reads it"
        );
        assert_eq!(state.pressed_scancodes.as_ref()[0], 47);

        release(&mut state, &mut map, 47, None);
        assert!(state.pressed_scancodes.as_ref().is_empty());
    }

    /// `xkb_state_key_get_one_sym` reports the EFFECTIVE keysym, so German
    /// AltGr+Q resolves to Key2 on the way down and Q on the way up when AltGr
    /// is released first. The release must undo the PRESS.
    ///
    /// NEGATIVE CONTROL: reduce `pressed_key_vks.remove(&key).or(virtual_keycode)`
    /// to `virtual_keycode` — Q is removed (it was never pressed) and Key2 stays
    /// latched for the life of the window.
    #[test]
    fn a_release_removes_the_code_the_press_recorded() {
        let mut state = KeyboardState::default();
        let mut map = PressedKeyVks::new();

        press(&mut state, &mut map, KEY_Q, Some(VirtualKeyCode::Key2));
        assert!(state.is_key_down(VirtualKeyCode::Key2));

        release(&mut state, &mut map, KEY_Q, Some(VirtualKeyCode::Q));
        assert!(
            state.pressed_virtual_keycodes.as_ref().is_empty(),
            "left over: {:?}",
            state.pressed_virtual_keycodes.as_ref()
        );
        assert!(map.is_empty());
    }

    /// Engine modifiers are DERIVED from `pressed_virtual_keycodes`, so a
    /// modifier that is never removed rewrites every later click.
    ///
    /// NEGATIVE CONTROL: the same reduction to `virtual_keycode` — the release
    /// resolves to no code, removes nothing, and `ctrl_down()` stays true.
    #[test]
    fn a_release_that_resolves_to_nothing_still_lifts_the_modifier() {
        let mut state = KeyboardState::default();
        let mut map = PressedKeyVks::new();

        press(
            &mut state,
            &mut map,
            KEY_LEFTCTRL,
            Some(VirtualKeyCode::LControl),
        );
        assert!(state.ctrl_down());

        release(&mut state, &mut map, KEY_LEFTCTRL, None);
        assert!(!state.ctrl_down());
    }

    /// Pressing a second button must not clear the first. Broadcasting
    /// `button == X` across all three flags turned "press Right while dragging
    /// with Left" into a phantom LeftMouseUp, which dropped the drag and the
    /// text selection with it.
    ///
    /// NEGATIVE CONTROL: `MouseButton::Right => mouse_state.right_down = down,`
    /// changed to `MouseButton::Right => { mouse_state.right_down = down;
    /// mouse_state.left_down = false; }` (the pre-fix broadcast) fails the
    /// `left_down` assertion.
    #[test]
    fn a_press_writes_exactly_one_button_flag() {
        let mut mouse = MouseState::default();

        set_mouse_button_down(&mut mouse, MouseButton::Left, true);
        assert!(mouse.left_down);

        set_mouse_button_down(&mut mouse, MouseButton::Right, true);
        assert!(
            mouse.left_down,
            "the drag in progress must survive a second button"
        );
        assert!(mouse.right_down);
        assert!(!mouse.middle_down);

        set_mouse_button_down(&mut mouse, MouseButton::Right, false);
        assert!(mouse.left_down);
        assert!(!mouse.right_down);

        set_mouse_button_down(&mut mouse, MouseButton::Middle, true);
        assert!(mouse.left_down);
        assert!(mouse.middle_down);
    }

    /// A button with no flag of its own must leave all three alone rather than
    /// fall into a catch-all that clears them.
    #[test]
    fn an_extra_button_touches_nothing() {
        let mut mouse = MouseState::default();
        mouse.left_down = true;
        mouse.middle_down = true;

        set_mouse_button_down(&mut mouse, MouseButton::Other(9), true);
        assert!(mouse.left_down);
        assert!(mouse.middle_down);
        assert!(!mouse.right_down);
    }

    /// One wheel detent must move the same distance it moves on X11 and Win32.
    /// The raw `wl_pointer.axis` value for a detent is compositor-defined
    /// (~10-15 px), which made Wayland scroll 25-33 % short of every other
    /// backend; `axis_discrete` carries the compositor-independent detent count.
    ///
    /// NEGATIVE CONTROL: return `raw` from the discrete arm (i.e. delete the
    /// `!is_trackpad && (discrete...)` branch) — the frame scrolls 12 px.
    #[test]
    fn a_wheel_detent_is_worth_the_shared_tick_distance() {
        assert_eq!(
            axis_frame_delta(false, (0.0, -12.0), (0.0, -1.0)),
            (0.0, -WHEEL_TICK_PIXELS)
        );
        assert_eq!(
            axis_frame_delta(false, (0.0, -36.0), (0.0, -3.0)),
            (0.0, -3.0 * WHEEL_TICK_PIXELS)
        );
        assert_eq!(WHEEL_TICK_PIXELS, 20.0, "the cross-backend detent distance");
    }

    /// A trackpad frame already carries pixel distances — multiplying its
    /// (rounded) detent count by a tick would quantize smooth scrolling back
    /// into jumps.
    ///
    /// NEGATIVE CONTROL: drop the `!is_trackpad &&` guard — the frame scrolls
    /// 20 px instead of 7.5.
    #[test]
    fn a_trackpad_frame_keeps_its_pixel_deltas() {
        assert_eq!(
            axis_frame_delta(true, (0.0, -7.5), (0.0, -1.0)),
            (0.0, -7.5)
        );
    }

    /// A compositor too old for `axis_discrete` sends no detent count; the raw
    /// value is all there is.
    #[test]
    fn a_frame_without_a_detent_count_falls_back_to_the_raw_value() {
        assert_eq!(
            axis_frame_delta(false, (0.0, -10.0), (0.0, 0.0)),
            (0.0, -10.0)
        );
    }

    /// Both axes of one frame are one scroll, so both must survive the flush.
    #[test]
    fn a_diagonal_frame_keeps_both_axes() {
        assert_eq!(
            axis_frame_delta(false, (-5.0, -10.0), (-1.0, -2.0)),
            (-WHEEL_TICK_PIXELS, -2.0 * WHEEL_TICK_PIXELS)
        );
    }

    /// Finger and continuous sources are position deltas; wheel is detents.
    /// Misclassifying a wheel tick as a trackpad gesture stacked velocity
    /// impulses into the scroll physics.
    ///
    /// NEGATIVE CONTROL: `source == WL_AXIS_SOURCE_FINGER` alone (dropping the
    /// continuous arm) fails the CONTINUOUS assertion.
    #[test]
    fn only_finger_and_continuous_sources_are_trackpads() {
        assert!(!axis_source_is_trackpad(WL_AXIS_SOURCE_WHEEL));
        assert!(axis_source_is_trackpad(WL_AXIS_SOURCE_FINGER));
        assert!(axis_source_is_trackpad(WL_AXIS_SOURCE_CONTINUOUS));
    }
}
