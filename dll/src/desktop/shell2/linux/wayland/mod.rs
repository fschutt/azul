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
    common::gl::GlFunctions,
    x11::{accessibility::LinuxAccessibilityAdapter, dlopen::Gtk3Im},
};
use crate::desktop::shell2::common::debug_server::LogCategory;
use crate::desktop::{
    shell2::common::{
        event::{self, HitTestNode, PlatformWindow, BUTTON_STATE_LEFT, BUTTON_STATE_RIGHT, BUTTON_STATE_MIDDLE, BUTTON_STATE_NONE},
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
}

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
    data_device_version: u32,
    data_device_initialized: bool,
    drag: events::WaylandDragState,
    tablet_pen: events::TabletPenPending,
    // False until the first poll rebinds all proxy listeners to the stable boxed `self`.
    listeners_rebound: bool,
    is_open: bool,
    configured: bool,

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

    // Monitor tracking for multi-monitor support
    pub known_outputs: Vec<MonitorState>,
    pub current_outputs: Vec<*mut defines::wl_output>,

    // V2 Event system state
    pub frame_callback_pending: bool, // Wayland frame callback synchronization
    /// Set to true when a visual update is needed but no layout regeneration is required.
    /// This happens when scroll offsets change (timer callbacks) or GPU values are updated.
    /// The next `render_frame_if_ready()` will send a lightweight transaction.
    pub needs_redraw: bool,

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

    // Shell2 state (same as WaylandWindow)
    pub layout_window: Option<LayoutWindow>,
    pub current_window_state: FullWindowState,
    pub previous_window_state: Option<FullWindowState>,
    pub render_api: Option<webrender::RenderApi>,
    pub renderer: Option<WrRenderer>,
    pub hit_tester: Option<AsyncHitTester>,
    pub document_id: Option<DocumentId>,
    pub image_cache: ImageCache,
    pub renderer_resources: RendererResources,
    gl_context_ptr: OptionGlContextPtr,
    new_frame_ready: Arc<(Mutex<bool>, Condvar)>,
    id_namespace: Option<IdNamespace>,
    render_mode: RenderMode,

    // V2 Event system state
    pub scrollbar_drag_state: Option<ScrollbarDragState>,
    pub last_hovered_node: Option<event::HitTestNode>,
    pub frame_needs_regeneration: bool,
    pub frame_callback_pending: bool,

    // Shared resources
    pub resources: Arc<super::AppResources>,
    fc_cache: Arc<FcFontCache>,
    app_data: Arc<RefCell<RefAny>>,

    /// wl_shm handle (borrowed from the parent) for lazily creating the CPU buffer.
    shm: *mut defines::wl_shm,
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
    /// Shared CPU rendering backend (the menu is painted via the headless CPU
    /// path, same as the X11/Wayland CPU fallback — popups never use WebRender).
    #[cfg(feature = "cpurender")]
    cpu_backend: crate::desktop::shell2::headless::CpuBackend,
    /// CPU hit-tester rebuilt from the popup's own layout (in `ensure_menu_layout`).
    /// Resolves popup-surface-relative pointer coords to the menu-item node so a
    /// click can fire its callback — the popup has no WebRender hit-tester.
    cpu_hit_tester: azul_layout::headless::CpuHitTester,
}

// Event Handler Types

// XKB Keyboard Translation

/// Translate XKB keysym to Azul VirtualKeyCode
///
/// XKB keysyms are defined in <xkbcommon/xkbcommon-keysyms.h>
/// This function maps common keysyms to VirtualKeyCode variants.
fn translate_keysym_to_virtual_keycode(keysym: u32) -> azul_core::window::VirtualKeyCode {
    use azul_core::window::VirtualKeyCode;

    // XKB keysym constants (from xkbcommon-keysyms.h)
    const XKB_KEY_Escape: u32 = 0xff1b;
    const XKB_KEY_Return: u32 = 0xff0d;
    const XKB_KEY_Tab: u32 = 0xff09;
    const XKB_KEY_BackSpace: u32 = 0xff08;
    const XKB_KEY_Delete: u32 = 0xffff;
    const XKB_KEY_Insert: u32 = 0xff63;
    const XKB_KEY_Home: u32 = 0xff50;
    const XKB_KEY_End: u32 = 0xff57;
    const XKB_KEY_Page_Up: u32 = 0xff55;
    const XKB_KEY_Page_Down: u32 = 0xff56;

    const XKB_KEY_Left: u32 = 0xff51;
    const XKB_KEY_Up: u32 = 0xff52;
    const XKB_KEY_Right: u32 = 0xff53;
    const XKB_KEY_Down: u32 = 0xff54;

    const XKB_KEY_F1: u32 = 0xffbe;
    const XKB_KEY_F2: u32 = 0xffbf;
    const XKB_KEY_F3: u32 = 0xffc0;
    const XKB_KEY_F4: u32 = 0xffc1;
    const XKB_KEY_F5: u32 = 0xffc2;
    const XKB_KEY_F6: u32 = 0xffc3;
    const XKB_KEY_F7: u32 = 0xffc4;
    const XKB_KEY_F8: u32 = 0xffc5;
    const XKB_KEY_F9: u32 = 0xffc6;
    const XKB_KEY_F10: u32 = 0xffc7;
    const XKB_KEY_F11: u32 = 0xffc8;
    const XKB_KEY_F12: u32 = 0xffc9;

    const XKB_KEY_Shift_L: u32 = 0xffe1;
    const XKB_KEY_Shift_R: u32 = 0xffe2;
    const XKB_KEY_Control_L: u32 = 0xffe3;
    const XKB_KEY_Control_R: u32 = 0xffe4;
    const XKB_KEY_Alt_L: u32 = 0xffe9;
    const XKB_KEY_Alt_R: u32 = 0xffea;
    const XKB_KEY_Super_L: u32 = 0xffeb;
    const XKB_KEY_Super_R: u32 = 0xffec;

    const XKB_KEY_space: u32 = 0x0020;
    const XKB_KEY_comma: u32 = 0x002c;
    const XKB_KEY_period: u32 = 0x002e;
    const XKB_KEY_slash: u32 = 0x002f;
    const XKB_KEY_semicolon: u32 = 0x003b;
    const XKB_KEY_apostrophe: u32 = 0x0027;
    const XKB_KEY_bracketleft: u32 = 0x005b;
    const XKB_KEY_bracketright: u32 = 0x005d;
    const XKB_KEY_backslash: u32 = 0x005c;
    const XKB_KEY_minus: u32 = 0x002d;
    const XKB_KEY_equal: u32 = 0x003d;
    const XKB_KEY_grave: u32 = 0x0060;

    match keysym {
        // Special keys
        XKB_KEY_Escape => VirtualKeyCode::Escape,
        XKB_KEY_Return => VirtualKeyCode::Return,
        XKB_KEY_Tab => VirtualKeyCode::Tab,
        XKB_KEY_BackSpace => VirtualKeyCode::Back,
        XKB_KEY_Delete => VirtualKeyCode::Delete,
        XKB_KEY_Insert => VirtualKeyCode::Insert,
        XKB_KEY_Home => VirtualKeyCode::Home,
        XKB_KEY_End => VirtualKeyCode::End,
        XKB_KEY_Page_Up => VirtualKeyCode::PageUp,
        XKB_KEY_Page_Down => VirtualKeyCode::PageDown,

        // Arrow keys
        XKB_KEY_Left => VirtualKeyCode::Left,
        XKB_KEY_Up => VirtualKeyCode::Up,
        XKB_KEY_Right => VirtualKeyCode::Right,
        XKB_KEY_Down => VirtualKeyCode::Down,

        // Function keys
        XKB_KEY_F1 => VirtualKeyCode::F1,
        XKB_KEY_F2 => VirtualKeyCode::F2,
        XKB_KEY_F3 => VirtualKeyCode::F3,
        XKB_KEY_F4 => VirtualKeyCode::F4,
        XKB_KEY_F5 => VirtualKeyCode::F5,
        XKB_KEY_F6 => VirtualKeyCode::F6,
        XKB_KEY_F7 => VirtualKeyCode::F7,
        XKB_KEY_F8 => VirtualKeyCode::F8,
        XKB_KEY_F9 => VirtualKeyCode::F9,
        XKB_KEY_F10 => VirtualKeyCode::F10,
        XKB_KEY_F11 => VirtualKeyCode::F11,
        XKB_KEY_F12 => VirtualKeyCode::F12,

        // Modifier keys
        XKB_KEY_Shift_L => VirtualKeyCode::LShift,
        XKB_KEY_Shift_R => VirtualKeyCode::RShift,
        XKB_KEY_Control_L => VirtualKeyCode::LControl,
        XKB_KEY_Control_R => VirtualKeyCode::RControl,
        XKB_KEY_Alt_L => VirtualKeyCode::LAlt,
        XKB_KEY_Alt_R => VirtualKeyCode::RAlt,
        XKB_KEY_Super_L => VirtualKeyCode::LWin,
        XKB_KEY_Super_R => VirtualKeyCode::RWin,

        // Punctuation
        XKB_KEY_space => VirtualKeyCode::Space,
        XKB_KEY_comma => VirtualKeyCode::Comma,
        XKB_KEY_period => VirtualKeyCode::Period,
        XKB_KEY_slash => VirtualKeyCode::Slash,
        XKB_KEY_semicolon => VirtualKeyCode::Semicolon,
        XKB_KEY_apostrophe => VirtualKeyCode::Apostrophe,
        XKB_KEY_bracketleft => VirtualKeyCode::LBracket,
        XKB_KEY_bracketright => VirtualKeyCode::RBracket,
        XKB_KEY_backslash => VirtualKeyCode::Backslash,
        XKB_KEY_minus => VirtualKeyCode::Minus,
        XKB_KEY_equal => VirtualKeyCode::Equals,
        XKB_KEY_grave => VirtualKeyCode::Grave,

        // Letters a-z (lowercase keysyms 0x0061-0x007a)
        0x0061 => VirtualKeyCode::A,
        0x0062 => VirtualKeyCode::B,
        0x0063 => VirtualKeyCode::C,
        0x0064 => VirtualKeyCode::D,
        0x0065 => VirtualKeyCode::E,
        0x0066 => VirtualKeyCode::F,
        0x0067 => VirtualKeyCode::G,
        0x0068 => VirtualKeyCode::H,
        0x0069 => VirtualKeyCode::I,
        0x006a => VirtualKeyCode::J,
        0x006b => VirtualKeyCode::K,
        0x006c => VirtualKeyCode::L,
        0x006d => VirtualKeyCode::M,
        0x006e => VirtualKeyCode::N,
        0x006f => VirtualKeyCode::O,
        0x0070 => VirtualKeyCode::P,
        0x0071 => VirtualKeyCode::Q,
        0x0072 => VirtualKeyCode::R,
        0x0073 => VirtualKeyCode::S,
        0x0074 => VirtualKeyCode::T,
        0x0075 => VirtualKeyCode::U,
        0x0076 => VirtualKeyCode::V,
        0x0077 => VirtualKeyCode::W,
        0x0078 => VirtualKeyCode::X,
        0x0079 => VirtualKeyCode::Y,
        0x007a => VirtualKeyCode::Z,

        // Letters A-Z (uppercase keysyms 0x0041-0x005a)
        0x0041 => VirtualKeyCode::A,
        0x0042 => VirtualKeyCode::B,
        0x0043 => VirtualKeyCode::C,
        0x0044 => VirtualKeyCode::D,
        0x0045 => VirtualKeyCode::E,
        0x0046 => VirtualKeyCode::F,
        0x0047 => VirtualKeyCode::G,
        0x0048 => VirtualKeyCode::H,
        0x0049 => VirtualKeyCode::I,
        0x004a => VirtualKeyCode::J,
        0x004b => VirtualKeyCode::K,
        0x004c => VirtualKeyCode::L,
        0x004d => VirtualKeyCode::M,
        0x004e => VirtualKeyCode::N,
        0x004f => VirtualKeyCode::O,
        0x0050 => VirtualKeyCode::P,
        0x0051 => VirtualKeyCode::Q,
        0x0052 => VirtualKeyCode::R,
        0x0053 => VirtualKeyCode::S,
        0x0054 => VirtualKeyCode::T,
        0x0055 => VirtualKeyCode::U,
        0x0056 => VirtualKeyCode::V,
        0x0057 => VirtualKeyCode::W,
        0x0058 => VirtualKeyCode::X,
        0x0059 => VirtualKeyCode::Y,
        0x005a => VirtualKeyCode::Z,

        // Numbers 0-9 (keysyms 0x0030-0x0039)
        0x0030 => VirtualKeyCode::Key0,
        0x0031 => VirtualKeyCode::Key1,
        0x0032 => VirtualKeyCode::Key2,
        0x0033 => VirtualKeyCode::Key3,
        0x0034 => VirtualKeyCode::Key4,
        0x0035 => VirtualKeyCode::Key5,
        0x0036 => VirtualKeyCode::Key6,
        0x0037 => VirtualKeyCode::Key7,
        0x0038 => VirtualKeyCode::Key8,
        0x0039 => VirtualKeyCode::Key9,

        // Unknown key - default to Escape
        _ => VirtualKeyCode::Escape,
    }
}

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
        let dispatched = unsafe {
            while (self.wayland.wl_display_prepare_read_queue)(self.display, self.event_queue) != 0 {
                // Queue not empty -> dispatch what's already there, then retry prepare.
                (self.wayland.wl_display_dispatch_queue_pending)(self.display, self.event_queue);
            }

            (self.wayland.wl_display_flush)(self.display);

            let fd = (self.wayland.wl_display_get_fd)(self.display);
            let mut pfd = libc::pollfd { fd, events: libc::POLLIN, revents: 0 };
            let readable =
                libc::poll(&mut pfd, 1, 0) > 0 && (pfd.revents & libc::POLLIN) != 0;

            if readable {
                (self.wayland.wl_display_read_events)(self.display);
            } else {
                (self.wayland.wl_display_cancel_read)(self.display);
            }

            (self.wayland.wl_display_dispatch_queue_pending)(self.display, self.event_queue)
        };

        // Service any open menu popup: dispatching above may have delivered its
        // configure (so we can render+attach a buffer) or popup_done (dismiss).
        self.drive_active_popup();

        if dispatched > 0 {
            Some(WaylandEvent::Redraw) // Events were processed, a redraw might be needed.
        } else {
            None
        }
    }

    pub fn present(&mut self) -> Result<(), WindowError> {
        let fractional = self.fractional_scale_active();
        let (logical_w, logical_h) = {
            let d = &self.common.current_window_state.size.dimensions;
            (d.width as i32, d.height as i32)
        };
        let result = match &mut self.render_mode {
            RenderMode::Gpu(gl_context, _) => gl_context.swap_buffers(),
            RenderMode::Cpu(Some(cpu_state)) => {
                // Buffer already rendered by render_frame_if_ready — just submit
                unsafe {
                    (self.wayland.wl_surface_attach)(self.surface, cpu_state.active_buffer(), 0, 0);
                    *cpu_state.slots[cpu_state.active].busy = true;
                    // Per-rect damage from the last render pass (buffer px);
                    // empty = the frame is unchanged (nothing to recomposite).
                    let surface_version =
                        (self.wayland.wl_proxy_get_version)(self.surface as *mut defines::wl_proxy);
                    let scale = cpu_state.scale.max(1);
                    if fractional {
                        // Viewport fractional scaling: buffer scale MUST be 1
                        // (reset any stale integer value) and the viewport
                        // maps the physical buffer to the logical size.
                        if surface_version >= 3 {
                            (self.wayland.wl_surface_set_buffer_scale)(self.surface, 1);
                        }
                        if let Some(vp) = self.viewport {
                            wp_viewport_set_destination(
                                &self.wayland, vp, logical_w, logical_h,
                            );
                        }
                    } else if surface_version >= 3 && scale > 1 {
                        (self.wayland.wl_surface_set_buffer_scale)(self.surface, scale);
                    }
                    for (dx, dy, dw, dh) in cpu_state.damage_rects.drain(..) {
                        if surface_version >= 4 {
                            (self.wayland.wl_surface_damage_buffer)(self.surface, dx, dy, dw, dh);
                        } else {
                            let x0 = dx.div_euclid(scale);
                            let y0 = dy.div_euclid(scale);
                            let x1 = (dx + dw + scale - 1).div_euclid(scale);
                            let y1 = (dy + dh + scale - 1).div_euclid(scale);
                            (self.wayland.wl_surface_damage)(self.surface, x0, y0, x1 - x0, y1 - y0);
                        }
                    }
                    (self.wayland.wl_surface_commit)(self.surface);
                }
                Ok(())
            }
            RenderMode::Cpu(None) => {
                // CPU fallback not yet initialized - wait for wl_shm from registry
                Ok(())
            }
        };

        // Clean up old textures from previous epochs to prevent memory leak
        // This must happen AFTER render() and buffer swap when WebRender no longer needs the textures
        if let Some(ref layout_window) = self.common.layout_window {
            crate::desktop::gl_texture_integration::remove_old_gl_textures(
                &layout_window.document_id,
                layout_window.epoch,
            );
        }

        // CI-only escape hatch: exit after first successful frame render.
        // Intentionally uses process::exit() to skip Drop impls for fast CI shutdown.
        if result.is_ok() && std::env::var("AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER").is_ok() {
            log_info!(
                LogCategory::Platform,
                "[CI] AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER set - exiting with success"
            );
            std::process::exit(0);
        }

        result
    }

    pub fn is_open(&self) -> bool {
        self.is_open
    }
    pub fn close_requested(&self) -> bool {
        self.common.current_window_state.flags.close_requested
    }
    pub fn close(&mut self) {
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
        // Snapshot every proxy that carries a listener which dereferences `data as
        // *mut WaylandWindow`, BEFORE taking the raw self-pointer. Proxies created later
        // (frame callbacks, tablet tools) are made by handlers that — after this rebind —
        // already hold the stable pointer, so they inherit it automatically.
        let proxies: [*mut std::ffi::c_void; 10] = [
            self.registry as _,
            self.surface as _,
            self.xdg_surface as _,
            self.xdg_toplevel as _,
            self.seat as _,
            self.pointer_state.pointer as _,
            self.keyboard as _,
            self.touch as _,
            self.tablet_manager as _,
            self.tablet_seat as _,
        ];
        let opt_proxies: [*mut std::ffi::c_void; 4] = [
            self.text_input.map_or(std::ptr::null_mut(), |p| p as _),
            self.toplevel_decoration.map_or(std::ptr::null_mut(), |p| p as _),
            // wp_fractional_scale_v1 (preferred_scale events dereference `data`).
            self.fractional_scale.map_or(std::ptr::null_mut(), |p| p as _),
            // wl_data_device (file DnD). The per-drag wl_data_offer proxies are
            // created by handlers that already hold the stable pointer, so they
            // inherit it automatically.
            self.data_device as _,
        ];
        let me = self as *mut Self as *mut std::ffi::c_void;
        for p in proxies.iter().chain(opt_proxies.iter()) {
            if !p.is_null() {
                unsafe { set(*p as *mut defines::wl_proxy, me) };
            }
        }
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

        let now = std::time::Instant::now();
        for (dom_id, node_id, action) in actions {
            if let Some(lw) = self.common.layout_window.as_mut() {
                let affected = lw.process_accessibility_action(dom_id, node_id, action, now);
                if !affected.is_empty() {
                    self.common.display_list_dirty = true;
                    // Invoke the callbacks the action mapped to (synthetic
                    // MouseUp for the Default/click action, etc.) — previously
                    // this map was dropped and screen-reader activation did
                    // nothing.
                    use crate::desktop::shell2::common::event::PlatformWindow as _;
                    let update = self.dispatch_accessibility_events(&affected);
                    if !matches!(update, azul_core::callbacks::Update::DoNothing) {
                        // The callback asked for a refresh (e.g. RefreshDom
                        // from a zoom button) — regenerate on the next frame,
                        // exactly like pointer-event dispatch does.
                        self.common.frame_needs_regeneration = true;
                    }
                }
            }
        }

        self.common.a11y_dirty = true;
        self.request_redraw();
    }

    pub fn request_redraw(&mut self) {
        self.needs_redraw = true;
        if self.configured {
            self.generate_frame_if_needed();
        }
    }

    pub fn sync_clipboard(
        &mut self,
        clipboard_manager: &mut azul_layout::managers::clipboard::ClipboardManager,
    ) {
        clipboard::sync_clipboard(clipboard_manager);
    }
}

// PlatformWindow Trait Implementation (Cross-platform V2 Event System)

impl PlatformWindow for WaylandWindow {

    impl_platform_window_getters!(common);

    fn get_raw_window_handle(&self) -> RawWindowHandle {
        RawWindowHandle::Wayland(WaylandHandle {
            surface: self.surface as *mut c_void,
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
            window_handle: RawWindowHandle::Wayland(WaylandHandle {
                surface: self.surface as *mut c_void,
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
                .and_then(|lw| lw.a11y_manager.last_tree_update.take());
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
        self.common.frame_needs_regeneration = true;
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
        // Check if native menus are enabled
        if self.common.current_window_state.flags.use_native_context_menus {
            // TODO: Show native Wayland popup via xdg_popup protocol
            log_debug!(
                LogCategory::Platform,
                "[Wayland] Native xdg_popup menu at ({}, {}) - not yet implemented, using fallback",
                position.x,
                position.y
            );
            self.show_fallback_menu(menu, position);
        } else {
            // Show fallback DOM-based menu
            self.show_fallback_menu(menu, position);
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
    ) {
        let trigger_rect = azul_core::geom::LogicalRect::new(
            position,
            azul_core::geom::LogicalSize::zero(),
        );
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

        let mut anchor_rect = match &options.window_state.layout_callback.ctx {
            azul_core::refany::OptionRefAny::Some(refany) => {
                let mut refany = refany.clone();
                refany
                    .downcast_ref::<self::menu::MenuLayoutData>()
                    .map(|d| d.trigger_rect)
            }
            azul_core::refany::OptionRefAny::None => None,
        }
        .unwrap_or_else(|| LogicalRect::new(azul_core::geom::LogicalPosition::zero(), LogicalSize::zero()));

        // A zero-sized anchor rect is rejected by some compositors — clamp >= 1x1.
        anchor_rect.size.width = anchor_rect.size.width.max(1.0);
        anchor_rect.size.height = anchor_rect.size.height.max(1.0);

        let mut popup_size = options.window_state.size.dimensions;
        popup_size.width = popup_size.width.max(1.0);
        popup_size.height = popup_size.height.max(1.0);

        crate::plog_info!(
            "[wayland-popup] open_menu_popup: anchor=({:.0},{:.0} {:.0}x{:.0}) size={:.0}x{:.0}",
            anchor_rect.origin.x, anchor_rect.origin.y,
            anchor_rect.size.width, anchor_rect.size.height,
            popup_size.width, popup_size.height
        );
        let popup = WaylandPopup::new(self, anchor_rect, popup_size, options)?;
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
        if self.active_popup.take().is_some() {
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
            Some(p) => p.is_dismissed() || !p.is_open,
            None => return,
        };
        if dismissed {
            self.dismiss_active_popup();
            return;
        }
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
            if matches!(options.window_state.flags.background_material, WindowBackgroundMaterial::Opaque) {
                options.window_state.background_color = resources.system_style.colors.window_background;
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
        let mut layout_window = LayoutWindow::new((*resources.fc_cache).clone()).map_err(|e| {
            WindowError::PlatformError(format!("LayoutWindow::new failed: {:?}", e))
        })?;
        layout_window.routes = resources.config.routes.clone();

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
            common: event::CommonWindowState {
                current_window_state: FullWindowState {
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
                previous_window_state: None,
                layout_window: Some(layout_window),
                render_api: None,
                renderer: None,
                hit_tester: None,
                cpu_hit_tester: Some(azul_layout::headless::CpuHitTester::new()),
                document_id: None,
                image_cache: ImageCache::default(),
                renderer_resources: RendererResources::default(),
                gl_context_ptr: None.into(),
                id_namespace: None,
                fc_cache: resources.fc_cache.clone(),
                system_style: resources.system_style.clone(),
                app_data: resources.app_data.clone(),
                undo_manager: resources.undo_manager.clone(),
                scrollbar_drag_state: None,
                last_hovered_node: None,
                frame_needs_regeneration: false,
                frame_relayout_only: false,
                next_relayout_reason: azul_core::callbacks::RelayoutReason::Initial,
                display_list_initialized: false,
                display_list_dirty: false,
                a11y_dirty: true,
            },
            new_frame_ready: Arc::new((Mutex::new(false), Condvar::new())),
            keyboard_state: events::WaylandKeyboardState::new(),
            pointer_state: events::PointerState::new(),
            keyboard: std::ptr::null_mut(),
            touch: std::ptr::null_mut(),
            listeners_rebound: false,
            tablet_manager: std::ptr::null_mut(),
            tablet_seat: std::ptr::null_mut(),
            tablet_initialized: false,
            data_device_manager: std::ptr::null_mut(),
            data_device: std::ptr::null_mut(),
            clipboard_offer: std::ptr::null_mut(),
            clipboard_source: std::ptr::null_mut(),
            last_input_serial: 0,
            data_device_version: 0,
            data_device_initialized: false,
            drag: events::WaylandDragState::default(),
            tablet_pen: events::TabletPenPending::default(),
            frame_callback_pending: false,
            needs_redraw: false,
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
            known_outputs: Vec::new(),
            current_outputs: Vec::new(),
            pending_window_creates: Vec::new(),
            active_popup: None,
            pointer_over_popup: false,
            gnome_menu: None, // Will be initialized if GNOME menus are enabled
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
                    *mut defines::wl_proxy, u32, *const defines::wl_interface,
                    *mut c_void, *mut defines::wl_surface,
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
        static XDG_SURFACE_LISTENER: defines::xdg_surface_listener = defines::xdg_surface_listener {
            configure: events::xdg_surface_configure_handler,
        };
        unsafe {
            (window.wayland.xdg_surface_add_listener)(
                window.xdg_surface,
                &XDG_SURFACE_LISTENER,
                &mut window as *mut _ as *mut _,
            )
        };

        window.xdg_toplevel =
            unsafe { (window.wayland.xdg_surface_get_toplevel)(window.xdg_surface) };

        // Attach listener to receive configure and close events from compositor
        static XDG_TOPLEVEL_LISTENER: defines::xdg_toplevel_listener = defines::xdg_toplevel_listener {
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
                    *mut defines::wl_proxy, u32, *const defines::wl_interface,
                    *mut std::ffi::c_void, *mut defines::xdg_toplevel,
                ) -> *mut defines::wl_proxy;
                let f: GetDecoCtor =
                    std::mem::transmute(window.wayland.wl_proxy_marshal_constructor);
                // opcode 1 = get_toplevel_decoration (opcode 0 is `destroy`!).
                let deco = f(mgr as *mut defines::wl_proxy, 1, deco_iface,
                             std::ptr::null_mut(), window.xdg_toplevel);
                if !deco.is_null() {
                    static DECO_LISTENER: defines::zxdg_toplevel_decoration_v1_listener =
                        defines::zxdg_toplevel_decoration_v1_listener {
                            configure: events::toplevel_decoration_configure_handler,
                        };
                    (window.wayland.wl_proxy_add_listener)(
                        deco,
                        &DECO_LISTENER as *const _ as *const _,
                        &mut window as *mut _ as *mut _,
                    );
                    // set_mode(server_side = 2): opcode 1, signature "u".
                    type SetModeFn = unsafe extern "C" fn(*mut defines::wl_proxy, u32, u32);
                    let set_mode_fn: SetModeFn =
                        std::mem::transmute(window.wayland.wl_proxy_marshal);
                    set_mode_fn(deco, 1, 2);
                    window.toplevel_decoration =
                        Some(deco as *mut defines::zxdg_toplevel_decoration_v1);
                    log_info!(
                        LogCategory::Platform,
                        "[Wayland] Requested server-side decorations (xdg-decoration)"
                    );
                }
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
        let backend = AzBackend::resolve(options.renderer.as_option().map(|r| r.hw_accel));
        let force_cpu = matches!(backend, AzBackend::Cpu);
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
                    let is_software = matches!(
                        crate::desktop::shell2::common::compositor::query_gpu_info(
                            &gl_functions.functions,
                        ),
                        GpuCheckResult::Blacklisted { .. }
                    );
                    if is_software && !force_gpu {
                        log_info!(
                            LogCategory::Rendering,
                            "[Wayland] software GL (llvmpipe/swrast) detected -> CPU rendering \
                             (cpurender is faster; set AZ_BACKEND=gpu to override)"
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
        let webrender_failed = if let RenderMode::Gpu(gl_context, gl_functions) =
            &mut window.render_mode
        {
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
                    azul_layout::window::LayoutWindow::new((*window.resources.fc_cache).clone())
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
                layout_window.current_window_state = window.common.current_window_state.clone();
                layout_window.renderer_type = Some(azul_core::window::RendererType::Hardware);
                layout_window.routes = window.resources.config.routes.clone();
                // Initialize monitor cache once at window creation
                if let Ok(mut guard) = layout_window.monitors.lock() {
                    *guard = crate::desktop::display::get_monitors();
                }
                window.common.layout_window = Some(layout_window);
            }

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

        // Register debug timer if AZ_DEBUG is enabled
        #[cfg(feature = "std")]
        if crate::desktop::shell2::common::debug_server::is_debug_enabled() {
            // Initialize LayoutWindow if not already done
            if window.common.layout_window.is_none() {
                if let Ok(mut layout_window) =
                    azul_layout::window::LayoutWindow::new((*window.resources.fc_cache).clone())
                {
                    if let Some(doc_id) = window.common.document_id {
                        layout_window.document_id = doc_id;
                    }
                    if let Some(ns_id) = window.common.id_namespace {
                        layout_window.id_namespace = ns_id;
                    }
                    layout_window.current_window_state = window.common.current_window_state.clone();
                    layout_window.renderer_type = Some(azul_core::window::RendererType::Hardware);
                    layout_window.routes = window.resources.config.routes.clone();
                    // Initialize monitor cache once at window creation
                    if let Ok(mut guard) = layout_window.monitors.lock() {
                        *guard = crate::desktop::display::get_monitors();
                    }
                    window.common.layout_window = Some(layout_window);
                }
            }

            // Register debug timer is now done from run() with explicit channel + component map
        }

        // Apply initial background material if not Opaque
        {
            use azul_core::window::WindowBackgroundMaterial;
            let initial_material = window.common.current_window_state.flags.background_material;
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
        let render_api = self.common.render_api.as_mut().unwrap();

        let framebuffer_size = webrender::api::units::DeviceIntSize::new(
            self.common.current_window_state.size.dimensions.width as i32,
            self.common.current_window_state.size.dimensions.height as i32,
        );
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
            let timeout_ms: i32 = if has_threads { 16 } else { -1 };

            let result = libc::poll(
                pollfds.as_mut_ptr(),
                pollfds.len() as libc::nfds_t,
                timeout_ms,
            );

            let mut any_timer_fired = false;
            if result > 0 {
                // Check Wayland display fd
                if pollfds[0].revents & libc::POLLIN != 0 {
                    (self.wayland.wl_display_dispatch_queue)(self.display, self.event_queue);
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
            // check_timers_and_threads, which ALSO marks needs_redraw when a
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

    /// Process events using state-diffing architecture.
    /// V2: Uses cross-platform dispatch system with recursive callback handling.
    pub fn process_events(&mut self) -> ProcessEventResult {
        // Process GNOME menu DBus messages (non-blocking)
        if let Some(ref manager) = self.gnome_menu {
            manager.process_messages();
        }

        // Process any pending menu callbacks from DBus
        self.process_pending_menu_callbacks();

        self.process_window_events(0)
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
            let layout_window = match self.common.layout_window.as_mut() {
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
                Update::RefreshDom | Update::RefreshDomAllWindows => {
                    event_result = event_result.max(azul_core::events::ProcessEventResult::ShouldRegenerateDomCurrentWindow);
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
                    // frame_relayout_only then makes generate_frame_if_needed() skip
                    // regenerate_layout() and only rebuild + send the transaction.
                    if let Some(layout_window) = self.common.layout_window.as_mut() {
                        let mut debug_messages = None;
                        if let Err(e) = crate::desktop::shell2::common::layout::incremental_relayout(
                            layout_window,
                            &self.common.current_window_state,
                            &mut self.common.renderer_resources,
                            &mut debug_messages,
                        ) {
                            log_warn!(LogCategory::Layout, "Incremental relayout failed: {}", e);
                        }
                    }
                    self.common.frame_relayout_only = true;
                    self.common.frame_needs_regeneration = true;
                    self.request_redraw();
                }
                ProcessEventResult::ShouldRegenerateDomCurrentWindow
                | ProcessEventResult::ShouldRegenerateDomAllWindows
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

    /// Handle keyboard key event with full XKB translation
    pub fn handle_key(&mut self, key: u32, state: u32) {
        use azul_core::window::{OptionChar, OptionVirtualKeyCode};

        // Only process key press events (state == 1)
        let is_pressed = state == 1;

        // Save previous state BEFORE making changes.
        // Detect key repeat: if the key is already in pressed_virtual_keycodes,
        // clear current_virtual_keycode in the snapshot for state-diff detection.
        let mut prev_snapshot = self.common.current_window_state.clone();
        if is_pressed {
            // We can't resolve the VK here yet (need XKB), but we can check
            // pressed_scancodes. The evdev keycode maps 1:1 with scan codes.
            let scan = key;
            let already_pressed = self.common.current_window_state.keyboard_state
                .pressed_scancodes.as_ref().iter().any(|s| *s == scan);
            if already_pressed {
                prev_snapshot.keyboard_state.current_virtual_keycode =
                    azul_core::window::OptionVirtualKeyCode::None;
            }
        }
        self.common.previous_window_state = Some(prev_snapshot);

        // Phase 2: OnFocus callback (delayed) - if we receive keyboard events, we must have focus
        // Wayland doesn't have explicit focus events like X11, so we detect focus from keyboard
        // activity
        if is_pressed && !self.common.current_window_state.window_focused {
            self.common.current_window_state.window_focused = true;
            self.dynamic_selector_context.window_focused = true;
            self.sync_ime_position_to_os();
        }

        // XKB uses keycode = evdev_keycode + 8
        let xkb_keycode = key + 8;

        // Get XKB state
        let xkb_state = self.keyboard_state.state;
        if xkb_state.is_null() {
            // XKB not initialized yet - V2 input system will handle text input
            self.common.current_window_state
                .keyboard_state
                .current_virtual_keycode = OptionVirtualKeyCode::None;
            return;
        }

        // Get keysym (symbolic key identifier)
        let keysym = unsafe { (self.xkb.xkb_state_key_get_one_sym)(xkb_state, xkb_keycode) };

        // Translate keysym to VirtualKeyCode
        let virtual_keycode = translate_keysym_to_virtual_keycode(keysym);

        // Client-side key repeat: arm on press of a repeatable key, disarm
        // when THAT key is released. Modifiers don't repeat.
        {
            use azul_core::window::VirtualKeyCode as VK;
            let is_modifier = matches!(
                virtual_keycode,
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
            );
            if is_pressed && !is_modifier {
                self.arm_key_repeat(key);
            } else if !is_pressed && self.key_repeat_keycode == Some(key) {
                self.disarm_key_repeat();
            }
        }

        // While a menu popup is open, Escape closes it (consumed, not forwarded
        // to the app), matching the click-outside dismiss behaviour.
        if is_pressed
            && virtual_keycode == azul_core::window::VirtualKeyCode::Escape
            && self.active_popup.is_some()
        {
            self.dismiss_active_popup();
            return;
        }

        // While a menu popup is open, route all other keys to it (consumed — not
        // forwarded to the app behind the menu). Return/Enter activates the item
        // currently under the popup cursor; the menu then closes.
        if self.active_popup.is_some() {
            if is_pressed {
                crate::plog_info!(
                    "[wayland-popup] routing key to popup: {:?}",
                    virtual_keycode
                );
                if virtual_keycode == azul_core::window::VirtualKeyCode::Return
                    || virtual_keycode == azul_core::window::VirtualKeyCode::NumpadEnter
                {
                    let activated = self
                        .active_popup
                        .as_mut()
                        .map_or(false, |p| p.activate_hovered());
                    if activated {
                        self.dismiss_active_popup();
                        self.common.frame_needs_regeneration = true;
                        self.request_redraw();
                    }
                }
            }
            return;
        }

        self.common.current_window_state
            .keyboard_state
            .current_virtual_keycode = OptionVirtualKeyCode::Some(virtual_keycode);

        // Update pressed_virtual_keycodes and pressed_scancodes lists
        if is_pressed {
            // Add key to pressed lists
            self.common.current_window_state
                .keyboard_state
                .pressed_virtual_keycodes
                .insert_hm_item(virtual_keycode);
            self.common.current_window_state
                .keyboard_state
                .pressed_scancodes
                .insert_hm_item(key);
        } else {
            // Remove key from pressed lists
            self.common.current_window_state
                .keyboard_state
                .pressed_virtual_keycodes
                .remove_hm_item(&virtual_keycode);
            self.common.current_window_state
                .keyboard_state
                .pressed_scancodes
                .remove_hm_item(&key);
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
                // (frame_relayout_only): skip regenerate_layout, but still rebuild the
                // CPU hit-tester + build & send the full WebRender transaction + present
                // (an incremental relayout does NOT send the transaction itself).
                if let Some(layout_window) = self.common.layout_window.as_mut() {
                    let mut debug_messages = None;
                    if let Err(e) = crate::desktop::shell2::common::layout::incremental_relayout(
                        layout_window,
                        &self.common.current_window_state,
                        &mut self.common.renderer_resources,
                        &mut debug_messages,
                    ) {
                        log_warn!(LogCategory::Layout, "Incremental relayout failed: {}", e);
                    }
                }
                self.common.frame_relayout_only = true;
                self.common.frame_needs_regeneration = true;
                self.request_redraw();
            }
            ProcessEventResult::ShouldRegenerateDomCurrentWindow
            | ProcessEventResult::ShouldRegenerateDomAllWindows
            | ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                // Layout/content changed → take the FULL rebuild path:
                // generate_frame_if_needed() runs regenerate_layout + rebuilds the CPU
                // hit-tester + builds & sends the WebRender transaction + presents, but
                // only when frame_needs_regeneration is set. Calling regenerate_layout()
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
                                w.common.frame_needs_regeneration = true;
                                w.request_redraw();
                            }
                        }
                    }
                }
                self.common.frame_needs_regeneration = true;
                self.request_redraw();
            }
            ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            | ProcessEventResult::ShouldReRenderCurrentWindow => {
                self.request_redraw();
            }
            ProcessEventResult::DoNothing => {}
        }
    }

    /// Handle pointer motion event
    /// Merge a touch point (down/motion) into touch_state by id, then process.
    /// `x`/`y` are surface-local logical coords (wl_fixed already /256.0).
    pub fn handle_touch_point(&mut self, id: i32, x: f64, y: f64) {
        use azul_core::window::{TouchPoint, TouchPointVec};
        let pos = LogicalPosition::new(x as f32, y as f32);
        self.common.previous_window_state = Some(self.common.current_window_state.clone());
        let ts = &mut self.common.current_window_state.touch_state;
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
            let window_position = self.common.current_window_state.position;
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
        self.common.previous_window_state = Some(self.common.current_window_state.clone());
        let ts = &mut self.common.current_window_state.touch_state;
        let mut pts: Vec<TouchPoint> = ts.touch_points.clone().into_library_owned_vec();
        let last_pos = pts
            .iter()
            .find(|p| p.id == id as u64)
            .map(|p| p.position);
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
        let ts = &mut self.common.current_window_state.touch_state;
        ts.touch_points = TouchPointVec::from_vec(Vec::new());
        ts.num_touches = 0;
        // MWA-B4: end every gesture session for the cancelled sequence.
        if let Some(lw) = self.common.layout_window.as_mut() {
            lw.gesture_drag_manager.touch_cancel_all();
        }
    }

    /// Feed the accumulated tablet pen state on the tool's `frame` event.
    pub fn handle_tablet_frame(&mut self) {
        let p = self.tablet_pen;
        self.common.previous_window_state = Some(self.common.current_window_state.clone());
        if let Some(lw) = self.common.layout_window.as_mut() {
            lw.gesture_drag_manager.update_pen_state_full(
                p.position,
                p.pressure,
                (p.tilt_x, p.tilt_y),
                p.in_contact,
                p.is_eraser,
                false,
                p.tool_id,
                0.0,
                p.rotation,
                0,
            );
        }
        let result = self.process_window_events(0);
        self.handle_process_event_result(result);
    }

    pub fn handle_pointer_motion(&mut self, x: f64, y: f64) {
        let logical_pos = LogicalPosition::new(x as f32, y as f32);

        // While the pointer is over an open menu popup, forward motion to the
        // popup (just tracks the popup-relative cursor for a later click/Return)
        // and don't touch the parent's hover/hit-test state.
        if self.pointer_over_popup && self.active_popup.is_some() {
            if let Some(popup) = self.active_popup.as_mut() {
                popup.set_cursor_pos(logical_pos);
            }
            return;
        }

        // Save previous state BEFORE making changes
        self.common.previous_window_state = Some(self.common.current_window_state.clone());

        self.common.current_window_state.mouse_state.cursor_position =
            CursorPosition::InWindow(logical_pos);

        // Handle scrollbar dragging if active
        if self.common.scrollbar_drag_state.is_some() {
            use crate::desktop::shell2::common::event::PlatformWindow;
            let result = PlatformWindow::handle_scrollbar_drag(self, logical_pos);
            // Route like every other pointer path: a scroll callback can restyle
            // (ShouldIncrementalRelayout → incremental fast path) or rebuild the DOM
            // (ShouldRegenerateDom* → frame_needs_regeneration). DoNothing stays a
            // no-op and the redraw-only variants still request_redraw, so plain
            // scrollbar drags behave exactly as before.
            self.handle_process_event_result(result);
            return;
        }

        // Record input sample for gesture detection (movement during button press)
        let button_state = if self.common.current_window_state.mouse_state.left_down {
            BUTTON_STATE_LEFT
        } else {
            BUTTON_STATE_NONE
        } | if self.common.current_window_state.mouse_state.right_down {
            BUTTON_STATE_RIGHT
        } else {
            BUTTON_STATE_NONE
        } | if self.common.current_window_state.mouse_state.middle_down {
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
                self.common.current_window_state.mouse_state.mouse_cursor_type =
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
                button, state
            );
            let activated = self
                .active_popup
                .as_mut()
                .map_or(false, |p| p.dispatch_button(button, state));
            if activated {
                self.dismiss_active_popup();
                // The menu callback likely mutated shared app state — regenerate
                // and repaint the parent so the selection's effect is visible.
                self.common.frame_needs_regeneration = true;
                self.request_redraw();
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
        let position = match self.common.current_window_state.mouse_state.cursor_position {
            CursorPosition::InWindow(pos) => pos,
            _ => LogicalPosition::zero(),
        };

        // Save previous state BEFORE making changes
        self.common.previous_window_state = Some(self.common.current_window_state.clone());

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

            // Check for context menu (right-click)
            if mouse_button == MouseButton::Right {
                if let Some(hit_node) = self.common.last_hovered_node {
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

        if is_down {
            // Button pressed
            self.common.current_window_state.mouse_state.left_down = mouse_button == MouseButton::Left;
            self.common.current_window_state.mouse_state.right_down = mouse_button == MouseButton::Right;
            self.common.current_window_state.mouse_state.middle_down = mouse_button == MouseButton::Middle;
            self.pointer_state.button_down = Some(mouse_button);
        } else {
            // Button released — only clear the button that was actually released
            match mouse_button {
                MouseButton::Left => self.common.current_window_state.mouse_state.left_down = false,
                MouseButton::Right => self.common.current_window_state.mouse_state.right_down = false,
                MouseButton::Middle => self.common.current_window_state.mouse_state.middle_down = false,
                _ => {}
            }
            self.pointer_state.button_down = None;
        }

        // Record input sample for gesture detection
        let button_state = match mouse_button {
            MouseButton::Left => BUTTON_STATE_LEFT,
            MouseButton::Right => BUTTON_STATE_RIGHT,
            MouseButton::Middle => BUTTON_STATE_MIDDLE,
            _ => BUTTON_STATE_NONE,
        };
        self.record_input_sample(position, button_state, is_down, !is_down, None);

        // V2: Process events through state-diffing system
        let result = self.process_window_events(0);
        self.handle_process_event_result(result);
    }

    /// Handle pointer axis (scroll) event
    pub fn handle_pointer_axis(&mut self, axis: u32, value: f64) {
        use azul_css::OptionF32;

        const WL_POINTER_AXIS_VERTICAL_SCROLL: u32 = 0;
        const WL_POINTER_AXIS_HORIZONTAL_SCROLL: u32 = 1;

        // Save previous state BEFORE making changes
        self.common.previous_window_state = Some(self.common.current_window_state.clone());

        // Determine scroll delta based on axis
        let (delta_x, delta_y) = match axis {
            WL_POINTER_AXIS_HORIZONTAL_SCROLL => (value as f32, 0.0),
            WL_POINTER_AXIS_VERTICAL_SCROLL => (0.0, value as f32),
            _ => (0.0, 0.0),
        };

        // Queue scroll input for the physics timer instead of directly setting offsets.
        {
            let mut should_start_timer = false;
            let mut input_queue_clone = None;

            if let Some(ref mut layout_window) = self.common.layout_window {
                use azul_core::task::Instant;
                use azul_layout::managers::scroll_state::ScrollInputSource;

                let now = Instant::from(std::time::Instant::now());

                if let Some((_dom_id, _node_id, start_timer)) =
                    layout_window.scroll_manager.record_scroll_from_hit_test(
                        // Raw delta; sign applied centrally (natural-scroll flag).
                        delta_x,
                        delta_y,
                        ScrollInputSource::WheelDiscrete,
                        &layout_window.hover_manager,
                        &InputPointId::Mouse,
                        now,
                    )
                {
                    should_start_timer = start_timer;
                    if start_timer {
                        input_queue_clone = Some(
                            layout_window.scroll_manager.get_input_queue()
                        );
                    }
                }
            }

            // Start the scroll momentum timer if this is the first input
            if should_start_timer {
                if let Some(queue) = input_queue_clone {
                    use azul_core::task::SCROLL_MOMENTUM_TIMER_ID;
                    use azul_layout::scroll_timer::{ScrollPhysicsState, scroll_physics_timer_callback};
                    use azul_layout::timer::{Timer, TimerCallbackType};
                    use azul_core::refany::RefAny;
                    use azul_core::task::Duration;

                    let physics_state = ScrollPhysicsState::new(queue, self.common.system_style.scroll_physics.clone());
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
                x, y
            );
            if let Some(popup) = self.active_popup.as_mut() {
                popup.set_cursor_pos(logical_pos);
            }
            return;
        }

        self.common.current_window_state.mouse_state.cursor_position =
            CursorPosition::InWindow(logical_pos);
        self.update_hit_test(logical_pos);
        self.request_redraw();
    }

    /// Handle keyboard leave event (window lost focus)
    pub fn handle_keyboard_leave(&mut self) {
        // Focus is gone — the compositor will not send the key release.
        self.disarm_key_repeat();
        self.common.previous_window_state = Some(self.common.current_window_state.clone());
        self.common.current_window_state.window_focused = false;
        self.dynamic_selector_context.window_focused = false;
        // MWA-A3b: forward to AT-SPI — accesskit_unix never learns window
        // focus on its own (Orca got no focus events on Wayland).
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
        (self.common.current_window_state.size.dpi as f32 / 96.0)
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
            let d = (self.common.current_window_state.size.dpi as f32 / 96.0).max(0.01);
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
        self.key_repeat_keycode = Some(keycode);
        let delay = self.key_repeat_delay_ms.max(1) as i64;
        let interval = self.key_repeat_rate_ms.max(1) as i64;
        let spec = libc::itimerspec {
            it_value: libc::timespec {
                tv_sec: delay / 1000,
                tv_nsec: (delay % 1000) * 1_000_000,
            },
            it_interval: libc::timespec {
                tv_sec: interval / 1000,
                tv_nsec: (interval % 1000) * 1_000_000,
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
                let destroy: unsafe extern "C" fn(*mut defines::wl_proxy, u32) =
                    std::mem::transmute(self.wayland.wl_proxy_marshal);
                destroy(self.clipboard_source as *mut defines::wl_proxy, 1);
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

            // offer(mime_type): opcode 0, "s" — advertise the common
            // plain-text spellings so GTK/Qt/terminal clients all match.
            let offer: unsafe extern "C" fn(
                *mut defines::wl_proxy,
                u32,
                *const std::os::raw::c_char,
            ) = std::mem::transmute(self.wayland.wl_proxy_marshal);
            for mime in ["text/plain;charset=utf-8", "UTF8_STRING", "text/plain"] {
                let c = std::ffi::CString::new(mime).unwrap();
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

    pub fn handle_keyboard_enter(&mut self) {
        self.common.previous_window_state = Some(self.common.current_window_state.clone());
        self.common.current_window_state.window_focused = true;
        self.dynamic_selector_context.window_focused = true;
        // MWA-A3b: mirror of handle_keyboard_leave — forward focus to AT-SPI.
        self.accessibility_adapter.set_focus(true);
        let result = self.process_window_events(0);
        self.handle_process_event_result(result);
        self.request_redraw();
    }

    /// Handle pointer leave event
    pub fn handle_pointer_leave(&mut self, _serial: u32) {
        // Pointer left the popup surface (e.g. moved back onto the parent):
        // just clear the routing flag; don't mark the parent out-of-window.
        if self.pointer_over_popup {
            self.pointer_over_popup = false;
            return;
        }

        // Get last known position before leaving
        let last_pos = match self.common.current_window_state.mouse_state.cursor_position {
            CursorPosition::InWindow(pos) => pos,
            _ => LogicalPosition::zero(),
        };
        self.common.current_window_state.mouse_state.cursor_position =
            CursorPosition::OutOfWindow(last_pos);
        if let Some(ref mut layout_window) = self.common.layout_window {
            layout_window
                .hover_manager
                .push_hit_test(InputPointId::Mouse, FullHitTest::empty(None));
        }
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
        self.common.previous_window_state = Some(self.common.current_window_state.clone());
        self.common.current_window_state.mouse_state.cursor_position =
            CursorPosition::InWindow(position);
        if let Some(first_path) = paths.first() {
            if let Some(layout_window) = self.common.layout_window.as_mut() {
                layout_window
                    .file_drop_manager
                    .set_hovered_file(Some(first_path.clone().into()));
            }
        }
        self.update_hit_test(position);
        self.process_window_events(0)
    }

    /// wl_data_device drag leaving without a drop (emits
    /// `EventType::FileHoverCancel`). Mirrors the X11/macOS handlers.
    pub fn handle_file_drag_exited(&mut self) -> ProcessEventResult {
        self.common.previous_window_state = Some(self.common.current_window_state.clone());
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
        self.common.previous_window_state = Some(self.common.current_window_state.clone());
        self.common.current_window_state.mouse_state.cursor_position =
            CursorPosition::InWindow(position);
        if let Some(first_path) = paths.first() {
            if let Some(layout_window) = self.common.layout_window.as_mut() {
                layout_window
                    .file_drop_manager
                    .set_dropped_file(Some(first_path.clone().into()));
            }
        }
        self.update_hit_test(position);
        let result = self.process_window_events(0);
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window.file_drop_manager.set_dropped_file(None);
        }
        result
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
        let node_data = match binding.get(node_id) {
            Some(nd) => nd,
            None => return false,
        };

        // Context menus are stored directly on NodeData
        // Clone to avoid borrow conflict (same pattern as macOS/X11)
        let context_menu = match node_data.get_context_menu() {
            Some(menu) => menu.clone(),
            None => return false,
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
        let trigger_rect = azul_core::geom::LogicalRect::new(
            position,
            azul_core::geom::LogicalSize::zero(),
        );
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
            &self.resources.app_data,
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

        // NOTE: Do NOT set frame_needs_regeneration here!
        // The caller (generate_frame_if_needed) manages this flag.
        // Setting it to true here would cause unnecessary re-layouts.

        // Update accessibility tree on Wayland
        #[cfg(feature = "a11y")]
        {
            if let Some(tree_update) = layout_window.a11y_manager.last_tree_update.take() {
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
        let _ = self.dispatch_pending_lifecycle_events();

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
                self.common.current_window_state.ime_position = ImePosition::Initialized(cursor_rect);
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
    /// This method applies the remaining fields and sets previous_window_state
    /// so that sync_window_state() works correctly for future changes.
    fn apply_initial_window_state(&mut self) {
        use azul_core::geom::OptionLogicalSize;
        use azul_core::window::WindowFrame;

        let mut needs_commit = false;

        // Window frame (Maximized, Minimized, Fullscreen)
        match self.common.current_window_state.flags.frame {
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
        if let OptionLogicalSize::Some(dims) = self.common.current_window_state.size.min_dimensions {
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
        if let OptionLogicalSize::Some(dims) = self.common.current_window_state.size.max_dimensions {
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
        if self.common.current_window_state.flags.is_top_level {
            self.set_is_top_level(true);
        }

        // prevent_system_sleep
        if self.common.current_window_state.flags.prevent_system_sleep {
            self.set_prevent_system_sleep(true);
        }

        // Commit changes if needed
        if needs_commit {
            unsafe {
                (self.wayland.wl_surface_commit)(self.surface);
            }
        }

        // CRITICAL: Set previous_window_state so sync_window_state() works for future changes
        self.common.previous_window_state = Some(self.common.current_window_state.clone());
    }

    /// Synchronize window state with Wayland compositor
    ///
    /// Wayland-specific state synchronization using Wayland protocols.
    pub fn sync_window_state(&mut self) {
        use azul_core::window::WindowFrame;

        // Note: Wayland state changes must be committed
        let mut needs_commit = false;

        // Sync title
        if let Some(prev) = &self.common.previous_window_state {
            if prev.title != self.common.current_window_state.title {
                let c_title = match std::ffi::CString::new(self.common.current_window_state.title.as_str())
                {
                    Ok(s) => s,
                    Err(_) => return,
                };
                unsafe {
                    (self.wayland.xdg_toplevel_set_title)(self.xdg_toplevel, c_title.as_ptr());
                }
                needs_commit = true;
            }

            // Window frame state changed? (Minimize/Maximize/Normal/Fullscreen)
            if prev.flags.frame != self.common.current_window_state.flags.frame {
                match self.common.current_window_state.flags.frame {
                    WindowFrame::Minimized => {
                        unsafe {
                            (self.wayland.xdg_toplevel_set_minimized)(self.xdg_toplevel);
                        }
                    }
                    WindowFrame::Maximized => {
                        // If previously fullscreen, unset fullscreen first
                        if prev.flags.frame == WindowFrame::Fullscreen {
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
                        if prev.flags.frame == WindowFrame::Maximized {
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
                        if prev.flags.frame == WindowFrame::Maximized {
                            unsafe {
                                (self.wayland.xdg_toplevel_unset_maximized)(self.xdg_toplevel);
                            }
                        }
                        if prev.flags.frame == WindowFrame::Fullscreen {
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
            if prev.size.min_dimensions != self.common.current_window_state.size.min_dimensions {
                use azul_core::geom::OptionLogicalSize;
                let (w, h) = match self.common.current_window_state.size.min_dimensions {
                    OptionLogicalSize::Some(dims) => (dims.width as i32, dims.height as i32),
                    OptionLogicalSize::None => (0, 0), // 0 = no minimum
                };
                unsafe {
                    (self.wayland.xdg_toplevel_set_min_size)(self.xdg_toplevel, w, h);
                }
                needs_commit = true;
            }

            // Max dimensions changed?
            if prev.size.max_dimensions != self.common.current_window_state.size.max_dimensions {
                use azul_core::geom::OptionLogicalSize;
                let (w, h) = match self.common.current_window_state.size.max_dimensions {
                    OptionLogicalSize::Some(dims) => (dims.width as i32, dims.height as i32),
                    OptionLogicalSize::None => (0, 0), // 0 = no maximum
                };
                unsafe {
                    (self.wayland.xdg_toplevel_set_max_size)(self.xdg_toplevel, w, h);
                }
                needs_commit = true;
            }
        }

        // Check window flags for is_top_level and other changes
        // We extract all values first to avoid borrow conflicts
        let flag_changes = self.common.previous_window_state.as_ref().map(|prev| {
            let is_top_level_changed =
                prev.flags.is_top_level != self.common.current_window_state.flags.is_top_level;
            let prevent_sleep_changed = prev.flags.prevent_system_sleep
                != self.common.current_window_state.flags.prevent_system_sleep;
            let background_material_changed = prev.flags.background_material
                != self.common.current_window_state.flags.background_material;
            let new_is_top_level = self.common.current_window_state.flags.is_top_level;
            let new_prevent_sleep = self.common.current_window_state.flags.prevent_system_sleep;
            let new_background_material = self.common.current_window_state.flags.background_material;

            (
                is_top_level_changed,
                new_is_top_level,
                prevent_sleep_changed,
                new_prevent_sleep,
                background_material_changed,
                new_background_material,
            )
        });

        if let Some((
            is_top_level_changed,
            new_is_top_level,
            prevent_sleep_changed,
            new_prevent_sleep,
            background_material_changed,
            new_background_material,
        )) = flag_changes
        {
            if is_top_level_changed {
                self.set_is_top_level(new_is_top_level);
            }

            // Check window flags for prevent_system_sleep
            if prevent_sleep_changed {
                self.set_prevent_system_sleep(new_prevent_sleep);
            }

            // Background material changed? (transparency/blur effects)
            if background_material_changed {
                self.apply_background_material(new_background_material);
                needs_commit = true;
            }
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
                self.common.current_window_state.size.dimensions.width as i32,
                self.common.current_window_state.size.dimensions.height as i32,
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
            let version = (self.wayland.wl_proxy_get_version)(blur_manager as *mut defines::wl_proxy);
            let blur = if !self.wayland.wl_proxy_marshal_flags.is_null() {
                type CreateFlags = unsafe extern "C" fn(
                    *mut defines::wl_proxy, u32, *const defines::wl_interface, u32, u32,
                    *mut std::ffi::c_void, *mut defines::wl_surface,
                ) -> *mut defines::wl_proxy;
                let f: CreateFlags = std::mem::transmute(self.wayland.wl_proxy_marshal_flags);
                f(blur_manager as *mut defines::wl_proxy, 0, blur_iface, version, 0,
                  std::ptr::null_mut(), self.surface)
            } else {
                type CreateCtor = unsafe extern "C" fn(
                    *mut defines::wl_proxy, u32, *const defines::wl_interface,
                    *mut std::ffi::c_void, *mut defines::wl_surface,
                ) -> *mut defines::wl_proxy;
                let f: CreateCtor = std::mem::transmute(self.wayland.wl_proxy_marshal_constructor);
                f(blur_manager as *mut defines::wl_proxy, 0, blur_iface,
                  std::ptr::null_mut(), self.surface)
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
            set_region_fn(blur as *mut defines::wl_proxy, 1, std::ptr::null::<defines::wl_region>());

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
    /// 1. **Full path** (`frame_needs_regeneration = true`): Regenerate layout, build full
    ///    transaction (fonts, images, display lists, scroll offsets, GPU values).
    /// 2. **Lightweight path** (`needs_redraw = true`, layout unchanged): Build lightweight
    ///    transaction (image callbacks, scroll offsets, GPU values only — skip scene builder).
    ///
    /// After sending the transaction, renders via WebRender and swaps buffers.
    /// Sets up Wayland frame callback for VSync.
    pub fn generate_frame_if_needed(&mut self) {
        let needs_work = self.common.frame_needs_regeneration
            || self.common.frame_relayout_only
            || self.needs_redraw;
        if !needs_work || self.frame_callback_pending {
            return;
        }

        // CRITICAL: Make OpenGL context current BEFORE generate_frame
        // The image callbacks (RenderImageCallback) need the GL context to be current
        // to allocate textures and draw to them
        if let RenderMode::Gpu(ref gl_context, _) = self.render_mode {
            gl_context.make_current();
        }

        if self.common.frame_needs_regeneration || self.common.frame_relayout_only {
            // FULL or RELAYOUT-ONLY PATH: both rebuild the CPU hit-tester + build &
            // send the full WebRender transaction below. Only the FULL path re-runs
            // regenerate_layout() (re-invokes the user's layout_callback + rebuilds the
            // StyledDom). The RELAYOUT-ONLY path's layout was already re-run by
            // incremental_relayout() in the ShouldIncrementalRelayout event arm
            // (frame_relayout_only) — re-running regenerate_layout() here would discard
            // that work and re-invoke the layout_callback.
            if self.common.frame_needs_regeneration && !self.common.frame_relayout_only {
                // FULL PATH: Regenerate layout
                if let Err(e) = self.regenerate_layout() {
                    log_error!(
                        LogCategory::Layout,
                        "[Wayland] Layout regeneration failed: {:?}",
                        e
                    );
                }
            }
            self.common.frame_needs_regeneration = false;
            self.common.frame_relayout_only = false;

            // Rebuild the CPU hit-tester from the fresh layout. CPU mode has no
            // WebRender hit-tester (render_api is None), and without this rebuild every
            // hit test returns nothing -> dead mouse hover / click / text selection /
            // focus. (GPU mode has cpu_hit_tester == None and uses the WebRender tester.)
            if let (Some(cpu_ht), Some(lw)) = (
                self.common.cpu_hit_tester.as_mut(),
                self.common.layout_window.as_ref(),
            ) {
                cpu_ht.rebuild_from_layout(&lw.layout_results);
            }

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
                    let now = azul_core::task::Instant::Tick(azul_core::task::SystemTick { tick_counter: 0 });
                    let tick_result = layout_window.scroll_manager.tick(now);
                    if tick_result.needs_repaint {
                        layout_window.scroll_manager.calculate_scrollbar_states();
                    }
                }

                // Process pending VirtualView updates (queued by ScrollTo → check_and_queue_virtual_view_reinvoke).
                // If present, we need a full display list rebuild rather than lightweight.
                let has_virtual_view_updates = !layout_window.pending_virtual_view_updates.is_empty();
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
                let req = render_api
                    .request_hit_tester(wr_translate2::wr_translate_document_id(doc_id));
                self.common.hit_tester = Some(AsyncHitTester::Requested(req));
            }
        }

        self.needs_redraw = false;

        // Fractional-scale presentation state (computed before the
        // render_mode borrow): with a viewport + preferred_scale the buffer
        // scale stays 1 and set_destination maps the physical buffer to the
        // LOGICAL surface size.
        let fractional = self.fractional_scale_active();
        let (logical_w, logical_h) = {
            let d = &self.common.current_window_state.size.dimensions;
            (d.width as i32, d.height as i32)
        };

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
                    let physical_size = self.common.current_window_state.size.get_physical_size();
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
                            let physical_size = self.common.current_window_state.size.get_physical_size();
                            unsafe {
                                (self.wayland.wl_surface_damage)(
                                    self.surface,
                                    0, 0,
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
                    let mut vviews_rebuilt = false;
                    if let Some(lw) = self.common.layout_window.as_mut() {
                        if !lw.pending_virtual_view_updates.is_empty() {
                            let system_callbacks =
                                azul_layout::callbacks::ExternalSystemCallbacks::rust_internal();
                            let current_window_state = lw.current_window_state.clone();
                            let renderer_resources =
                                std::mem::take(&mut lw.renderer_resources);
                            let updated = lw.process_pending_virtual_view_updates(
                                &current_window_state,
                                &renderer_resources,
                                &system_callbacks,
                            );
                            lw.renderer_resources = renderer_resources;
                            vviews_rebuilt = !updated.is_empty();
                        }
                    }
                    // The drain REBUILT VirtualView child DOMs (fresh NodeIds).
                    // The CPU hit-tester still indexes the previous generation's
                    // rects — rebuild it now, or the next pointer move hit-tests
                    // stale NodeIds (cursor panic / events on the wrong node).
                    if vviews_rebuilt {
                        if let (Some(cpu_ht), Some(lw)) = (
                            self.common.cpu_hit_tester.as_mut(),
                            self.common.layout_window.as_ref(),
                        ) {
                            cpu_ht.rebuild_from_layout(&lw.layout_results);
                        }
                    }

                    // Resolve RenderImageCallback <img> nodes (e.g. the AzulPaint
                    // canvas) into CPU images — the renderer can't invoke callbacks
                    // itself. gl=None forces the callback's CPU branch. Cheap when
                    // content is unchanged (callbacks cache by their own revision).
                    if let Some(lw) = self.common.layout_window.as_mut() {
                        lw.invoke_cpu_image_callbacks(&azul_core::gl::OptionGlContextPtr::None);
                    }

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
                                // Shared CPU renderer (same path as headless + X11):
                                // damage diff + scroll-offset feed + thin-strip
                                // scroll-shift with eligibility + offset-aware render.
                                // Replaces the logic that used to live here and lacked
                                // all the scroll machinery (#13/#14).
                                self.cpu_backend.render_frame(
                                    layout_window,
                                    &layout_window.renderer_resources,
                                    width,
                                    height,
                                    dpi,
                                );

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
                                            let copy_rects: Vec<(u32, u32, u32, u32)> =
                                                if cpu_state.slots[slot].stale_overflow {
                                                    vec![full]
                                                } else {
                                                    rects
                                                        .iter()
                                                        .copied()
                                                        .chain(
                                                            cpu_state.slots[slot]
                                                                .stale
                                                                .iter()
                                                                .map(|&(x, y, w, h)| {
                                                                    (
                                                                        x.max(0) as u32,
                                                                        y.max(0) as u32,
                                                                        w.max(0) as u32,
                                                                        h.max(0) as u32,
                                                                    )
                                                                }),
                                                        )
                                                        .collect()
                                                };
                                            let dst_stride =
                                                (cpu_state.width.max(0) as usize) * 4;
                                            let src_stride = (src_w as usize) * 4;
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
                                                    // RGBA → ARGB8888 (BGRA in LE memory)
                                                    for (s, d) in src[so..so + n]
                                                        .chunks_exact(4)
                                                        .zip(
                                                            buf[doff..doff + n]
                                                                .chunks_exact_mut(4),
                                                        )
                                                    {
                                                        d[0] = s[2]; // B
                                                        d[1] = s[1]; // G
                                                        d[2] = s[0]; // R
                                                        d[3] = s[3]; // A
                                                    }
                                                }
                                            }
                                            // Stale bookkeeping: this slot is
                                            // now current; the OTHER slot missed
                                            // this frame's rects.
                                            cpu_state.slots[slot].stale.clear();
                                            cpu_state.slots[slot].stale_overflow = false;
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
                                            self.needs_redraw = true;
                                            self.os_present_requested = true;
                                        }
                                    }
                                }
                                // (previous-display-list tracking now lives inside
                                // CpuBackend::render_frame.)
                                true
                            } else { false }
                        } else { false }
                    } else { false };

                    if !rendered {
                        if cpu_state.acquire_slot().is_some() {
                            cpu_state.draw_blue();
                            let (w, h) = (cpu_state.width, cpu_state.height);
                            cpu_state.damage_rects.push((0, 0, w, h));
                        } else {
                            self.needs_redraw = true;
                        }
                    }
                }

                #[cfg(not(feature = "cpurender"))]
                {
                    if cpu_state.acquire_slot().is_some() {
                        cpu_state.draw_blue();
                        let (w, h) = (cpu_state.width, cpu_state.height);
                        cpu_state.damage_rects.push((0, 0, w, h));
                    } else {
                        self.needs_redraw = true;
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
                        if fractional {
                            // Fractional path: buffer scale stays 1 (reset a
                            // stale integer value if any); the viewport maps
                            // the physical buffer to the LOGICAL surface size.
                            if surface_version >= 3 {
                                (self.wayland.wl_surface_set_buffer_scale)(self.surface, 1);
                            }
                            if let Some(vp) = self.viewport {
                                wp_viewport_set_destination(
                                    &self.wayland, vp, logical_w, logical_h,
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
                            (self.wayland.wl_surface_damage_buffer)(
                                self.surface, dx, dy, dw, dh,
                            );
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
                    let width = self.common.current_window_state.size.dimensions.width as i32;
                    let height = self.common.current_window_state.size.dimensions.height as i32;
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

        // 4. Set up frame callback for next frame (VSync)
        unsafe {
            let frame_callback = (self.wayland.wl_surface_frame)(self.surface);
            // The listener MUST outlive the proxy: wl_proxy_add_listener stores the
            // POINTER, not a copy. A stack-local listener here was a use-after-free —
            // when the compositor later sent `done`, libwayland dereferenced freed
            // stack and jumped to a garbage fn pointer (SIGSEGV in ffi_call). Use a
            // 'static listener, like every other listener in this file.
            static FRAME_CALLBACK_LISTENER: defines::wl_callback_listener =
                defines::wl_callback_listener { done: frame_done_callback };
            (self.wayland.wl_callback_add_listener)(
                frame_callback,
                &FRAME_CALLBACK_LISTENER,
                self as *mut _ as *mut _,
            );
            (self.wayland.wl_surface_commit)(self.surface);
        }

        // If any scrollbar is actively fading (0 < opacity < 1), schedule
        // another frame so the fade-out animation runs to completion.
        let needs_fade_frame = self.common.layout_window.as_ref()
            .map(|lw| lw.gpu_state_manager.scrollbar_fade_active)
            .unwrap_or(false);
        if needs_fade_frame {
            self.request_redraw();
        }

        self.common.frame_needs_regeneration = false;
        self.frame_callback_pending = true;
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

    // The frame callback is one-shot: once the compositor delivers `done`, this
    // wl_callback proxy is dead. Destroy it here — otherwise EVERY frame leaks one
    // proxy. That is the "wl_callback@NNN still attached" flood seen on close (IDs
    // climbing into the hundreds), which culminates in libwayland's
    // "malloc(): mismatching next->prev_size" heap-corruption abort when the event
    // queue is torn down with all those dangling proxies still attached.
    if !callback.is_null() {
        unsafe { (window.wayland.wl_proxy_destroy)(callback as _); }
    }

    // If there are more changes pending, request another frame
    if window.common.frame_needs_regeneration || window.needs_redraw {
        window.generate_frame_if_needed();
    }
}

impl Drop for WaylandWindow {
    fn drop(&mut self) {
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
                    WL_SHM_FORMAT_ARGB8888,
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
            }
        };

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

    fn draw_blue(&mut self) {
        let slice = self.pixel_buffer_mut();
        for chunk in slice.chunks_exact_mut(4) {
            chunk[0] = 0xFF; // Blue
            chunk[1] = 0x00; // Green
            chunk[2] = 0x00; // Red
            chunk[3] = 0xFF; // Alpha (ARGB format)
        }
    }
}

impl Drop for CpuFallbackState {
    fn drop(&mut self) {
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
    }

    /// Check timers and threads, trigger callbacks if needed.
    /// This is called on every poll_event() to simulate timer ticks.
    /// If any timer/thread callback requested a visual update, mark needs_redraw
    /// and attempt to render immediately (if no frame callback is pending).
    fn check_timers_and_threads(&mut self) {
        use super::super::common::event::PlatformWindow;
        if self.process_timers_and_threads() {
            self.needs_redraw = true;
            self.generate_frame_if_needed();
        }
    }

    /// Returns the logical size of the window's surface.
    pub fn get_window_size_logical(&self) -> (i32, i32) {
        let size = self.common.current_window_state.size.get_logical_size();
        (size.width as i32, size.height as i32)
    }

    /// Returns the physical size of the window by applying the scale factor.
    pub fn get_window_size_physical(&self) -> (i32, i32) {
        let size = self.common.current_window_state.size.get_physical_size();
        (size.width as i32, size.height as i32)
    }

    /// Returns the DPI scale factor for the window.
    pub fn get_scale_factor(&self) -> f32 {
        self.common.current_window_state
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

            // Anchor to bottom-right corner of anchor rect
            (wayland.xdg_positioner_set_anchor)(positioner, XDG_POSITIONER_ANCHOR_BOTTOM_RIGHT);

            // Popup grows down and right from anchor point
            (wayland.xdg_positioner_set_gravity)(positioner, XDG_POSITIONER_GRAVITY_BOTTOM_RIGHT);

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

        // 11. Create window state
        let current_window_state = FullWindowState {
            title: "Popup".to_string().into(),
            size: options.window_state.size,
            position: parent.common.current_window_state.position,
            flags: parent.common.current_window_state.flags,
            theme: parent.common.current_window_state.theme,
            debug_state: parent.common.current_window_state.debug_state,
            keyboard_state: parent.common.current_window_state.keyboard_state.clone(),
            mouse_state: parent.common.current_window_state.mouse_state.clone(),
            touch_state: parent.common.current_window_state.touch_state.clone(),
            ime_position: parent.common.current_window_state.ime_position,
            platform_specific_options: parent
                .common.current_window_state
                .platform_specific_options
                .clone(),
            renderer_options: parent.common.current_window_state.renderer_options,
            background_color: parent.common.current_window_state.background_color,
            layout_callback: options.window_state.layout_callback.clone(),
            close_callback: options.window_state.close_callback.clone(),
            monitor_id: parent.common.current_window_state.monitor_id,
            window_id: options.window_state.window_id.clone(),
            window_focused: false,
            active_route: azul_core::resources::OptionRouteMatch::None,
        };

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

            layout_window: None,
            current_window_state,
            previous_window_state: None,
            render_api: None,
            renderer: None,
            hit_tester: None,
            document_id: None,
            image_cache: ImageCache::default(),
            renderer_resources: RendererResources::default(),
            gl_context_ptr: OptionGlContextPtr::None,
            new_frame_ready: Arc::new((Mutex::new(false), Condvar::new())),
            id_namespace: None,
            render_mode: RenderMode::Cpu(None),

            scrollbar_drag_state: None,
            last_hovered_node: None,
            frame_needs_regeneration: true,
            frame_callback_pending: false,

            resources: parent.resources.clone(),
            fc_cache: parent.common.fc_cache.clone(),
            app_data: parent.common.app_data.clone(),

            shm: parent.shm,
            viewporter: parent.viewporter,
            preferred_scale_120: parent.preferred_scale_120,
            viewport: None,
            rendered: false,
            #[cfg(feature = "cpurender")]
            cpu_backend: crate::desktop::shell2::headless::CpuBackend::new(),
            cpu_hit_tester: azul_layout::headless::CpuHitTester::new(),
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
    fn render_if_ready(&mut self) {
        if !self.is_open || self.rendered || !self.is_configured() {
            return;
        }
        if self.surface.is_null() || self.shm.is_null() {
            return;
        }

        let logical_w = self.current_window_state.size.dimensions.width.max(1.0);
        let logical_h = self.current_window_state.size.dimensions.height.max(1.0);
        let dpi_factor = {
            let d = self.current_window_state.size.dpi as f32 / 96.0;
            if d <= 0.0 { 1.0 } else { d }
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
            let scale = if fractional { 1 } else { dpi_factor.round().max(1.0) as i32 };
            let (phys_w, phys_h) = if fractional {
                (buf_w, buf_h)
            } else {
                (
                    ((buf_w + scale - 1) / scale) * scale,
                    ((buf_h + scale - 1) / scale) * scale,
                )
            };
            match CpuFallbackState::new(
                &self.wayland,
                self.shm,
                phys_w,
                phys_h,
                scale,
            ) {
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
        let laid_out = self.ensure_menu_layout();

        if let RenderMode::Cpu(Some(cpu_state)) = &mut self.render_mode {
            let mut painted = false;

            #[cfg(feature = "cpurender")]
            {
                if laid_out {
                    if let Some(ref layout_window) = self.layout_window {
                        self.cpu_backend.render_frame(
                            layout_window,
                            &layout_window.renderer_resources,
                            logical_w,
                            logical_h,
                            dpi_factor,
                        );
                        if let Some(ref pixmap) = self.cpu_backend.last_frame {
                            let buf = cpu_state.pixel_buffer_mut();
                            let src = pixmap.data();
                            let copy_len = buf.len().min(src.len());
                            // RGBA -> ARGB8888: swap R and B for Wayland.
                            let mut i = 0;
                            while i + 3 < copy_len {
                                buf[i] = src[i + 2];     // B
                                buf[i + 1] = src[i + 1]; // G
                                buf[i + 2] = src[i];     // R
                                buf[i + 3] = src[i + 3]; // A
                                i += 4;
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
    }

    /// Build the LayoutWindow (lazily) and run a layout pass for the menu DOM.
    /// Returns `true` if a layout result for the root DOM is available.
    #[cfg(feature = "cpurender")]
    fn ensure_menu_layout(&mut self) -> bool {
        use azul_core::dom::DomId;

        if self.layout_window.is_none() {
            match LayoutWindow::new((*self.fc_cache).clone()) {
                Ok(mut lw) => {
                    lw.routes = self.resources.config.routes.clone();
                    self.layout_window = Some(lw);
                }
                Err(e) => {
                    log_error!(
                        LogCategory::Layout,
                        "[Wayland popup] LayoutWindow::new failed: {:?}",
                        e
                    );
                    return false;
                }
            }
        }

        let resources = self.resources.clone();
        let mut debug_messages = None;

        let layout_window = match self.layout_window.as_mut() {
            Some(lw) => lw,
            None => return false,
        };

        let result = crate::desktop::shell2::common::layout::regenerate_layout(
            layout_window,
            &resources.app_data,
            &self.current_window_state,
            &mut self.renderer_resources,
            &self.image_cache,
            &self.gl_context_ptr,
            &self.fc_cache,
            &resources.font_registry,
            &resources.system_style,
            &resources.icon_provider,
            &mut debug_messages,
            azul_core::callbacks::RelayoutReason::Initial,
        );

        match result {
            Ok(_) => {
                // Rebuild the popup's CPU hit-tester from the fresh layout so a
                // pointer click can be resolved to a menu-item node + its
                // callback (mirrors the parent window's post-regenerate_layout
                // rebuild at the CPU path). The popup has no WebRender hit-tester.
                if let Some(lw) = self.layout_window.as_ref() {
                    self.cpu_hit_tester.rebuild_from_layout(&lw.layout_results);
                }
                self.layout_window
                    .as_ref()
                    .map(|lw| lw.layout_results.contains_key(&DomId { inner: 0 }))
                    .unwrap_or(false)
            }
            Err(e) => {
                log_error!(
                    LogCategory::Layout,
                    "[Wayland popup] regenerate_layout failed: {}",
                    e
                );
                false
            }
        }
    }

    /// Update the popup-relative cursor position from a pointer enter/motion.
    /// The xdg_popup grab delivers enter/motion coords already relative to the
    /// popup surface, so no translation is needed — they map straight onto the
    /// popup's logical layout. Stored so a later button/Return can hit-test here.
    fn set_cursor_pos(&mut self, pos: LogicalPosition) {
        self.current_window_state.mouse_state.cursor_position =
            CursorPosition::InWindow(pos);
    }

    /// Handle a pointer button delivered to the popup. Menu items register their
    /// activation callback on `Hover(MouseDown)`, so fire on a LEFT button press.
    /// Returns `true` if a menu item was activated (caller then dismisses).
    fn dispatch_button(&mut self, button: u32, state: u32) -> bool {
        // 0x110 == BTN_LEFT; state 1 == pressed.
        if state != 1 || button != 0x110 {
            return false;
        }
        let pos = match self.current_window_state.mouse_state.cursor_position {
            CursorPosition::InWindow(p) => p,
            _ => return false,
        };
        self.activate_item_at(pos)
    }

    /// Activate the menu item currently under the popup cursor (used by Return).
    fn activate_hovered(&mut self) -> bool {
        let pos = match self.current_window_state.mouse_state.cursor_position {
            CursorPosition::InWindow(p) => p,
            _ => return false,
        };
        self.activate_item_at(pos)
    }

    /// Hit-test the popup's own layout at `pos` (popup-surface-relative logical
    /// coords) and, if a menu-item node carries a `Hover(MouseDown)` callback,
    /// invoke it — the same machinery the parent uses in
    /// `dispatch_events_propagated` (find the node's `CoreCallbackData`, convert
    /// via `Callback::from_core`, run `invoke_single_callback_at`). The CPU
    /// hit-tester returns every node geometrically containing the point (incl.
    /// ancestors), so a click on a child label still finds the item div's
    /// callback. Returns `true` if a callback fired.
    fn activate_item_at(&mut self, pos: LogicalPosition) -> bool {
        use azul_core::dom::{DomNodeId, EventFilter, HoverEventFilter};
        use azul_core::styled_dom::NodeHierarchyItemId;

        // Phase 1 (read-only): find the topmost hit node with a MouseDown callback.
        let target = {
            let hits = self.cpu_hit_tester.hit_test(pos);
            let lw = match self.layout_window.as_ref() {
                Some(lw) => lw,
                None => return false,
            };
            let mut found = None;
            'outer: for (dom_id, node_id) in &hits {
                if let Some(lr) = lw.layout_results.get(dom_id) {
                    let ndc = lr.styled_dom.node_data.as_container();
                    if let Some(nd) = ndc.get(*node_id) {
                        for cb in nd.get_callbacks().as_ref().iter() {
                            if cb.event == EventFilter::Hover(HoverEventFilter::MouseDown) {
                                found = Some((*dom_id, *node_id, cb.clone()));
                                break 'outer;
                            }
                        }
                    }
                }
            }
            found
        };

        let (dom_id, node_id, cb_data) = match target {
            Some(t) => t,
            None => {
                crate::plog_info!(
                    "[wayland-popup] pointer over popup at ({:.1},{:.1}) -> no actionable menu item",
                    pos.x, pos.y
                );
                return false;
            }
        };

        crate::plog_info!(
            "[wayland-popup] pointer over popup at ({:.1},{:.1}) -> node {}",
            pos.x, pos.y, node_id.index()
        );

        // Phase 2 (mutable): invoke the menu item's callback.
        let mut callback = azul_layout::callbacks::Callback::from_core(cb_data.callback);
        let mut refany = cb_data.refany.clone();
        let hit_node = DomNodeId {
            dom: dom_id,
            node: NodeHierarchyItemId::from_crate_internal(Some(node_id)),
        };
        let raw_handle = RawWindowHandle::Wayland(WaylandHandle {
            display: self.display as *mut _,
            surface: self.surface as *mut _,
        });
        let system_style = self.resources.system_style.clone();
        match self.layout_window.as_mut() {
            Some(lw) => {
                let _ = lw.invoke_single_callback_at(
                    hit_node,
                    &mut callback,
                    &mut refany,
                    &raw_handle,
                    &self.gl_context_ptr,
                    system_style,
                    &azul_layout::callbacks::ExternalSystemCallbacks::rust_internal(),
                    &self.previous_window_state,
                    &self.current_window_state,
                    &self.renderer_resources,
                );
            }
            None => return false,
        }
        crate::plog_info!(
            "[wayland-popup] menu item activated -> node {} callback fired",
            node_id.index()
        );
        true
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

        if let ImePosition::Initialized(rect) = self.common.current_window_state.ime_position {
            // Use text-input v3 protocol if available (native Wayland IME)
            if let Some(text_input) = self.text_input {
                if self.text_input_enabled {
                    // set_cursor_rectangle: opcode 6, args (x, y, width, height)
                    type MarshalFn = unsafe extern "C" fn(
                        *mut defines::wl_proxy,
                        u32, // opcode
                        i32, i32, i32, i32,
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
        let has_contenteditable_focus = self.common.layout_window.as_ref()
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
                        let calc_global_offset = |cursor: &azul_core::selection::TextCursor| -> i32 {
                            let run_idx = cursor.cluster_id.source_run as usize;
                            let byte_in_run = cursor.cluster_id.start_byte_in_run as usize;
                            let mut global = 0usize;
                            for (i, item) in content.iter().enumerate() {
                                if i >= run_idx { break; }
                                match item {
                                    azul_layout::text3::cache::InlineContent::Text(r) => global += r.text.len(),
                                    azul_layout::text3::cache::InlineContent::Space(_) => global += 1,
                                    azul_layout::text3::cache::InlineContent::LineBreak(_) => global += 1,
                                    azul_layout::text3::cache::InlineContent::Tab { .. } => global += 1,
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

                match std::ffi::CString::new(text_str) {
                    Ok(cstr) => (cstr, cursor_byte, anchor_byte),
                    Err(_) => (std::ffi::CString::new("").unwrap(), 0, 0),
                }
            }
            None => return,
        };

        // set_surrounding_text: opcode 3, args (text: string, cursor: int, anchor: int)
        type SurroundingFn = unsafe extern "C" fn(
            *mut defines::wl_proxy, u32,
            *const std::ffi::c_char, i32, i32,
        );
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
            let marshal: MarshalFn =
                unsafe { std::mem::transmute(self.wayland.wl_proxy_marshal) };
            unsafe {
                // enable (opcode 1)
                marshal(
                    text_input as *mut defines::wl_proxy,
                    defines::ZWP_TEXT_INPUT_V3_ENABLE,
                );
                // set_content_type (opcode 5): hint=COMPLETION|SPELLCHECK, purpose=NORMAL
                type ContentTypeFn =
                    unsafe extern "C" fn(*mut defines::wl_proxy, u32, u32, u32);
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
            let marshal: MarshalFn =
                unsafe { std::mem::transmute(self.wayland.wl_proxy_marshal) };
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
            self.common.current_window_state.size.dpi as f32 / 96.0,
        );
        if let Some(tooltip) = self.tooltip.as_mut() {
            if let Err(e) = tooltip.show(text, position, dpi) {
                log_error!(LogCategory::Platform, "[Wayland] Failed to show tooltip: {}", e);
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
