//! Wayland implementation for Linux.
//!
//! This module implements the PlatformWindow trait for Wayland.
//! It supports GPU-accelerated rendering via EGL and WebRender, with a
//! fallback to a CPU-rendered surface if GL context creation fails.
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
        event::{self, PlatformWindow},
        WindowError,
    },
    wr_translate2::{self, AsyncHitTester, Notifier},
};
use crate::{log_debug, log_error, log_info, log_trace, log_warn};

/// Tracks the current rendering mode of the window.
enum RenderMode {
    Gpu(gl::GlContext, GlFunctions),
    /// CPU fallback - initialized lazily after receiving wl_shm from registry
    Cpu(Option<CpuFallbackState>),
}

/// State for CPU fallback rendering.
struct CpuFallbackState {
    wayland: Rc<Wayland>,
    pool: *mut defines::wl_shm_pool,
    buffer: *mut defines::wl_buffer,
    data: *mut u8,
    width: i32,
    height: i32,
    stride: i32,
    fd: i32, // Keep fd open until drop
}

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
    pub display: *mut defines::wl_display,
    registry: *mut defines::wl_registry,
    compositor: *mut defines::wl_compositor,
    shm: *mut defines::wl_shm,
    seat: *mut defines::wl_seat,
    xdg_wm_base: *mut defines::xdg_wm_base,
    surface: *mut defines::wl_surface,
    xdg_surface: *mut defines::xdg_surface,
    xdg_toplevel: *mut defines::xdg_toplevel,
    event_queue: *mut defines::wl_event_queue,
    keyboard_state: events::WaylandKeyboardState,
    pointer_state: events::PointerState,
    is_open: bool,
    configured: bool,

    // Wayland protocols
    subcompositor: Option<*mut defines::wl_subcompositor>, // For tooltips

    // KDE blur protocol (org.kde.kwin.blur)
    blur_manager: Option<*mut defines::org_kde_kwin_blur_manager>,
    current_blur: Option<*mut defines::org_kde_kwin_blur>,

    // Tooltip
    tooltip: Option<tooltip::TooltipWindow>,

    // Power management (D-Bus)
    screensaver_inhibit_cookie: Option<u32>,
    dbus_connection: Option<*mut super::dbus::DBusConnection>,

    // Shell2 state (common fields shared with all platforms)
    pub common: event::CommonWindowState,
    new_frame_ready: Arc<(Mutex<bool>, Condvar)>,

    render_mode: RenderMode,

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
}

// Event Handler Types

/// Hit test node structure for event routing.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct HitTestNode {
    dom_id: u64,
    node_id: u64,
}

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
        XKB_KEY_Shift_L | XKB_KEY_Shift_R => VirtualKeyCode::LShift,
        XKB_KEY_Control_L | XKB_KEY_Control_R => VirtualKeyCode::LControl,
        XKB_KEY_Alt_L | XKB_KEY_Alt_R => VirtualKeyCode::LAlt,
        XKB_KEY_Super_L | XKB_KEY_Super_R => VirtualKeyCode::LWin,

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
        // Check timers and threads before processing Wayland events
        self.check_timers_and_threads();

        if unsafe {
            (self.wayland.wl_display_dispatch_queue_pending)(self.display, self.event_queue)
        } > 0
        {
            Some(WaylandEvent::Redraw) // Events were processed, a redraw might be needed.
        } else {
            None
        }
    }

    pub fn present(&mut self) -> Result<(), WindowError> {
        let result = match &self.render_mode {
            RenderMode::Gpu(gl_context, _) => gl_context.swap_buffers(),
            RenderMode::Cpu(Some(cpu_state)) => {
                cpu_state.draw_blue();
                unsafe {
                    (self.wayland.wl_surface_attach)(self.surface, cpu_state.buffer, 0, 0);
                    (self.wayland.wl_surface_damage)(
                        self.surface,
                        0,
                        0,
                        cpu_state.width,
                        cpu_state.height,
                    );
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

        // CI testing: Exit successfully after first frame render if env var is set
        if result.is_ok() && std::env::var("AZUL_EXIT_SUCCESS_AFTER_FRAME_RENDER").is_ok() {
            log_info!(
                LogCategory::Platform,
                "[CI] AZUL_EXIT_SUCCESS_AFTER_FRAME_RENDER set - exiting with success"
            );
            std::process::exit(0);
        }

        result
    }

    pub fn is_open(&self) -> bool {
        self.is_open
    }
    pub fn close(&mut self) {
        self.is_open = false;
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
        // Convert logical position to surface-relative coordinates
        // (Wayland tooltips use subsurfaces positioned relative to parent)
        let x = position.x as i32;
        let y = position.y as i32;

        self.show_tooltip(text.to_string(), x, y);
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
        let menu_options = crate::desktop::menu::show_menu(
            menu.clone(),
            self.common.system_style.clone(),
            parent_pos,
            None,           // No trigger rect
            Some(position), // Position for menu
            None,           // No parent menu
        );

        // Queue window creation request
        log_debug!(
            LogCategory::Window,
            "[Wayland] Queuing fallback menu window at ({}, {}) - will be created in event loop",
            position.x,
            position.y
        );

        self.pending_window_creates.push(menu_options);
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

        let event_queue = unsafe { (wayland.wl_display_create_event_queue)(display) };
        let registry = unsafe { (wayland.wl_display_get_registry)(display) };
        unsafe { (wayland.wl_proxy_set_queue)(registry as _, event_queue) };

        // Initialize LayoutWindow
        let layout_window = LayoutWindow::new((*resources.fc_cache).clone()).map_err(|e| {
            WindowError::PlatformError(format!("LayoutWindow::new failed: {:?}", e))
        })?;

        let mut window = Self {
            wayland: wayland.clone(),
            xkb,
            gtk_im,
            gtk_im_context,
            text_input_manager: None, // Will be populated if compositor supports text-input v3
            text_input: None,
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
                },
                previous_window_state: None,
                layout_window: Some(layout_window),
                render_api: None,
                renderer: None,
                hit_tester: None,
                document_id: None,
                image_cache: ImageCache::default(),
                renderer_resources: RendererResources::default(),
                gl_context_ptr: None.into(),
                id_namespace: None,
                fc_cache: resources.fc_cache.clone(),
                system_style: resources.system_style.clone(),
                app_data: resources.app_data.clone(),
                scrollbar_drag_state: None,
                last_hovered_node: None,
                frame_needs_regeneration: false,
            },
            new_frame_ready: Arc::new((Mutex::new(false), Condvar::new())),
            keyboard_state: events::WaylandKeyboardState::new(),
            pointer_state: events::PointerState::new(),
            frame_callback_pending: false,
            needs_redraw: false,
            timer_fds: std::collections::BTreeMap::new(),
            #[cfg(feature = "a11y")]
            accessibility_adapter: LinuxAccessibilityAdapter::new(),
            // CPU rendering state will be initialized after receiving wl_shm from registry
            render_mode: RenderMode::Cpu(None),
            known_outputs: Vec::new(),
            current_outputs: Vec::new(),
            pending_window_creates: Vec::new(),
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

        // Initialize monitor cache once at window creation
        if let Some(ref lw) = window.common.layout_window {
            if let Ok(mut guard) = lw.monitors.lock() {
                *guard = crate::desktop::display::get_monitors();
            }
        }

        let listener = defines::wl_registry_listener {
            global: events::registry_global_handler,
            global_remove: events::registry_global_remove_handler,
        };
        unsafe {
            (window.wayland.wl_proxy_add_listener)(
                registry as _,
                &listener as *const _ as _,
                &mut window as *mut _ as *mut _,
            )
        };
        unsafe { (window.wayland.wl_display_roundtrip)(display) };

        window.surface =
            unsafe { (window.wayland.wl_compositor_create_surface)(window.compositor) };

        // Add wl_surface listener to track which monitors the window is on
        let surface_listener = defines::wl_surface_listener {
            enter: events::wl_surface_enter_handler,
            leave: events::wl_surface_leave_handler,
        };
        unsafe {
            (window.wayland.wl_surface_add_listener)(
                window.surface,
                &surface_listener,
                &mut window as *mut _ as *mut _,
            )
        };

        window.xdg_surface = unsafe {
            (window.wayland.xdg_wm_base_get_xdg_surface)(window.xdg_wm_base, window.surface)
        };

        let xdg_surface_listener = defines::xdg_surface_listener {
            configure: events::xdg_surface_configure_handler,
        };
        unsafe {
            (window.wayland.xdg_surface_add_listener)(
                window.xdg_surface,
                &xdg_surface_listener,
                &mut window as *mut _ as *mut _,
            )
        };

        window.xdg_toplevel =
            unsafe { (window.wayland.xdg_surface_get_toplevel)(window.xdg_surface) };

        // Attach listener to receive configure and close events from compositor
        let xdg_toplevel_listener = defines::xdg_toplevel_listener {
            configure: events::xdg_toplevel_configure_handler,
            close: events::xdg_toplevel_close_handler,
            configure_bounds: events::xdg_toplevel_configure_bounds_handler,
            wm_capabilities: events::xdg_toplevel_wm_capabilities_handler,
        };
        unsafe {
            (window.wayland.xdg_toplevel_add_listener)(
                window.xdg_toplevel,
                &xdg_toplevel_listener,
                &mut window as *mut _ as *mut _,
            )
        };

        let title = CString::new(options.window_state.title.as_str()).unwrap();
        unsafe { (window.wayland.xdg_toplevel_set_title)(window.xdg_toplevel, title.as_ptr()) };

        let width = options.window_state.size.dimensions.width as i32;
        let height = options.window_state.size.dimensions.height as i32;

        let render_mode = match gl::GlContext::new(&wayland, display, window.surface, width, height)
        {
            Ok(mut gl_context) => {
                gl_context.configure_vsync(options.window_state.renderer_options.vsync);
                let gl_functions =
                    GlFunctions::initialize(gl_context.egl.as_ref().unwrap()).unwrap();
                RenderMode::Gpu(gl_context, gl_functions)
            }
            Err(e) => {
                log_warn!(
                    LogCategory::Rendering,
                    "[Wayland] GPU context failed: {:?}. Falling back to CPU.",
                    e
                );
                RenderMode::Cpu(Some(CpuFallbackState::new(
                    &wayland, window.shm, width, height,
                )?))
            }
        };
        window.render_mode = render_mode;

        if let RenderMode::Gpu(gl_context, gl_functions) = &mut window.render_mode {
            gl_context.make_current();
            // Borrow gl_functions separately to avoid double mutable borrow
            let gl_funcs_ref = gl_functions as *const GlFunctions;
            window.initialize_webrender(&options, unsafe { &*gl_funcs_ref })?;
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
                            window.surface as u32, // Use surface pointer as window ID
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
                // Initialize monitor cache once at window creation
                if let Ok(mut guard) = layout_window.monitors.lock() {
                    *guard = crate::desktop::display::get_monitors();
                }
                window.common.layout_window = Some(layout_window);
            }

            // Get mutable references needed for invoke_single_callback
            let layout_window = window
                .layout_window
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

        // Register debug timer if AZUL_DEBUG is enabled
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
        let (renderer, sender) = webrender::create_webrender_instance(
            gl_functions.functions.clone(),
            Box::new(Notifier {
                new_frame_ready: new_frame_ready.clone(),
            }),
            wr_translate2::default_renderer_options(
                options,
                wr_translate2::create_program_cache(&gl_functions.functions),
            ),
            None,
        )
        .map_err(|e| WindowError::PlatformError(format!("WebRender init failed: {:?}", e)))?;

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
        self.common.gl_context_ptr = OptionGlContextPtr::Some(GlContextPtr::new(
            RendererType::Hardware,
            gl_functions.functions.clone(),
        ));
        self.new_frame_ready = new_frame_ready;

        Ok(())
    }

    pub fn wait_for_events(&mut self) -> Result<(), WindowError> {
        use super::super::common::event::PlatformWindow;

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

            // If no timers, block indefinitely (-1), otherwise block until something fires
            let timeout_ms = if self.timer_fds.is_empty() { -1 } else { -1 };

            let result = libc::poll(
                pollfds.as_mut_ptr(),
                pollfds.len() as libc::nfds_t,
                timeout_ms,
            );

            if result > 0 {
                // Check Wayland display fd
                if pollfds[0].revents & libc::POLLIN != 0 {
                    (self.wayland.wl_display_dispatch_queue)(self.display, self.event_queue);
                }

                // Check timerfd's - if any fired, invoke timer callbacks
                let mut any_timer_fired = false;
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

                // Invoke expired timer and thread callbacks
                if any_timer_fired {
                    self.process_timers_and_threads();
                }
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
                ProcessEventResult::ShouldRegenerateDomCurrentWindow
                | ProcessEventResult::ShouldRegenerateDomAllWindows
                | ProcessEventResult::ShouldIncrementalRelayout
                | ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
                | ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                    self.common.frame_needs_regeneration = true;
                    self.request_redraw();
                }
                ProcessEventResult::ShouldReRenderCurrentWindow => {
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

        // Save previous state BEFORE making changes
        self.common.previous_window_state = Some(self.common.current_window_state.clone());

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
                let utf8_str = unsafe {
                    std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                        buffer.as_ptr() as *const u8,
                        len as usize,
                    ))
                };

                // Record text input in TextInputManager
                if !utf8_str.is_empty() {
                    if let Some(ref mut layout_window) = self.common.layout_window {
                        layout_window.record_text_input(utf8_str);
                    }
                }
            }
        }

        // V2: Process events through state-diffing system
        let result = self.process_window_events(0);

        // Process the result
        match result {
            ProcessEventResult::ShouldRegenerateDomCurrentWindow => {
                if let Err(e) = self.regenerate_layout() {
                    log_error!(
                        LogCategory::Layout,
                        "[Wayland] Layout regeneration error: {}",
                        e
                    );
                }
            }
            ProcessEventResult::ShouldReRenderCurrentWindow => {
                self.request_redraw();
            }
            _ => {}
        }
    }

    /// Handle pointer motion event
    pub fn handle_pointer_motion(&mut self, x: f64, y: f64) {
        let logical_pos = LogicalPosition::new(x as f32, y as f32);

        // Save previous state BEFORE making changes
        self.common.previous_window_state = Some(self.common.current_window_state.clone());

        self.common.current_window_state.mouse_state.cursor_position =
            CursorPosition::InWindow(logical_pos);

        // Handle scrollbar dragging if active
        if self.common.scrollbar_drag_state.is_some() {
            use crate::desktop::shell2::common::event::PlatformWindow;
            let result = PlatformWindow::handle_scrollbar_drag(self, logical_pos);
            if !matches!(result, ProcessEventResult::DoNothing) {
                self.request_redraw();
            }
            return;
        }

        // Record input sample for gesture detection (movement during button press)
        let button_state = if self.common.current_window_state.mouse_state.left_down {
            0x01
        } else {
            0x00
        } | if self.common.current_window_state.mouse_state.right_down {
            0x02
        } else {
            0x00
        } | if self.common.current_window_state.mouse_state.middle_down {
            0x04
        } else {
            0x00
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

        // Process the result
        match result {
            ProcessEventResult::ShouldRegenerateDomCurrentWindow
            | ProcessEventResult::ShouldRegenerateDomAllWindows => {
                if let Err(e) = self.regenerate_layout() {
                    log_error!(
                        LogCategory::Layout,
                        "[Wayland] Layout regeneration error: {}",
                        e
                    );
                }
            }
            ProcessEventResult::ShouldIncrementalRelayout
            | ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            | ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                self.common.frame_needs_regeneration = true;
                self.request_redraw();
            }
            ProcessEventResult::ShouldReRenderCurrentWindow => {
                self.request_redraw();
            }
            ProcessEventResult::DoNothing => {}
        }
    }

    /// Handle pointer button event
    pub fn handle_pointer_button(&mut self, serial: u32, button: u32, state: u32) {
        self.pointer_state.serial = serial;

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
                if !matches!(result, ProcessEventResult::DoNothing) {
                    self.request_redraw();
                }
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
            // Button released
            self.common.current_window_state.mouse_state.left_down = false;
            self.common.current_window_state.mouse_state.right_down = false;
            self.common.current_window_state.mouse_state.middle_down = false;
            self.pointer_state.button_down = None;
        }

        // Record input sample for gesture detection
        let button_state = match mouse_button {
            MouseButton::Left => 0x01,
            MouseButton::Right => 0x02,
            MouseButton::Middle => 0x04,
            _ => 0x00,
        };
        self.record_input_sample(position, button_state, is_down, !is_down, None);

        // V2: Process events through state-diffing system
        let result = self.process_window_events(0);

        // Process the result
        match result {
            ProcessEventResult::ShouldRegenerateDomCurrentWindow
            | ProcessEventResult::ShouldRegenerateDomAllWindows => {
                if let Err(e) = self.regenerate_layout() {
                    log_error!(
                        LogCategory::Layout,
                        "[Wayland] Layout regeneration error: {}",
                        e
                    );
                }
            }
            ProcessEventResult::ShouldIncrementalRelayout
            | ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            | ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                self.common.frame_needs_regeneration = true;
                self.request_redraw();
            }
            ProcessEventResult::ShouldReRenderCurrentWindow => {
                self.request_redraw();
            }
            ProcessEventResult::DoNothing => {}
        }
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
                        -delta_x,
                        -delta_y,
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

        // Process the result
        match result {
            ProcessEventResult::ShouldRegenerateDomCurrentWindow
            | ProcessEventResult::ShouldRegenerateDomAllWindows => {
                if let Err(e) = self.regenerate_layout() {
                    log_error!(
                        LogCategory::Layout,
                        "[Wayland] Layout regeneration error: {}",
                        e
                    );
                }
            }
            ProcessEventResult::ShouldIncrementalRelayout
            | ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            | ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                self.common.frame_needs_regeneration = true;
                self.request_redraw();
            }
            ProcessEventResult::ShouldReRenderCurrentWindow => {
                self.request_redraw();
            }
            ProcessEventResult::DoNothing => {}
        }
    }

    /// Handle pointer enter event
    pub fn handle_pointer_enter(&mut self, serial: u32, x: f64, y: f64) {
        self.pointer_state.serial = serial;
        let logical_pos = LogicalPosition::new(x as f32, y as f32);
        self.common.current_window_state.mouse_state.cursor_position =
            CursorPosition::InWindow(logical_pos);
        self.update_hit_test(logical_pos);
        self.request_redraw();
    }

    /// Handle keyboard leave event (window lost focus)
    pub fn handle_keyboard_leave(&mut self) {
        self.common.previous_window_state = Some(self.common.current_window_state.clone());
        self.common.current_window_state.window_focused = false;
        self.dynamic_selector_context.window_focused = false;
        self.request_redraw();
    }

    /// Handle pointer leave event
    pub fn handle_pointer_leave(&mut self, _serial: u32) {
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
        use azul_core::geom::PhysicalPosition;

        if let Some(AsyncHitTester::Resolved(ref hit_tester)) = self.common.hit_tester {
            let physical_pos_u32 = position.to_physical(
                self.common.current_window_state
                    .size
                    .get_hidpi_factor()
                    .inner
                    .get(),
            );
            let physical_pos =
                PhysicalPosition::new(physical_pos_u32.x as f32, physical_pos_u32.y as f32);

            let hit_test_result =
                hit_tester.hit_test(wr_translate2::translate_world_point(physical_pos));
            // Get focused node from FocusManager
            let focused_node = self
                .layout_window
                .as_ref()
                .and_then(|lw| lw.focus_manager.get_focused_node().copied());
            let hit_test = wr_translate2::translate_hit_test_result(hit_test_result, focused_node);
            if let Some(ref mut layout_window) = self.common.layout_window {
                layout_window
                    .hover_manager
                    .push_hit_test(InputPointId::Mouse, hit_test);
            }
        }
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
            Some(menu) => (**menu).clone(),
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

    /// Queue a window-based context menu for creation in the event loop
    /// This is part of the unified multi-window menu system (Shell2 V2)
    fn show_window_based_context_menu(
        &mut self,
        menu: &azul_core::menu::Menu,
        position: LogicalPosition,
    ) {
        // Get parent window position (Wayland doesn't expose absolute positions)
        let parent_pos = match self.common.current_window_state.position {
            azul_core::window::WindowPosition::Initialized(pos) => {
                azul_core::geom::LogicalPosition::new(pos.x as f32, pos.y as f32)
            }
            _ => azul_core::geom::LogicalPosition::new(0.0, 0.0),
        };

        // Create menu window options using unified menu system
        let menu_options = crate::desktop::menu::show_menu(
            menu.clone(),
            self.common.system_style.clone(),
            parent_pos,
            None,           // No trigger rect for context menus
            Some(position), // Cursor position
            None,           // No parent menu
        );

        log_debug!(
            LogCategory::Window,
            "[Wayland] Queuing window-based context menu at screen ({}, {})",
            position.x,
            position.y
        );
        self.pending_window_creates.push(menu_options);
    }

    /// Process window events (V2 wrapper for external use)
    pub fn process_window_events_v2(&mut self) -> ProcessEventResult {
        self.process_events();
        ProcessEventResult::DoNothing
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

        // Mark that frame needs regeneration (will be called once at event processing end)
        self.common.frame_needs_regeneration = true;

        // Update accessibility tree on Wayland
        #[cfg(feature = "a11y")]
        {
            if let Some(tree_update) = layout_window.a11y_manager.last_tree_update.take() {
                self.accessibility_adapter.update_tree(tree_update);
            }
        }

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
            if let Some(ref blur_manager) = self.blur_manager {
                unsafe {
                    // org_kde_kwin_blur_release is opcode 0
                    // We use wl_proxy_destroy since we don't have a dedicated release function
                    (self.wayland.wl_proxy_destroy)(blur as *mut defines::wl_proxy);
                }
                log_debug!(
                    LogCategory::Platform,
                    "[Wayland] Removed KDE blur effect from surface"
                );
            }
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

        // Create new blur object for this surface
        // org_kde_kwin_blur_manager.create opcode is 0
        // Signature: create(id: new_id<org_kde_kwin_blur>, surface: object<wl_surface>)
        unsafe {
            // Transmute to specific signature for this call
            type CreateBlurFn = unsafe extern "C" fn(
                *mut defines::wl_proxy,
                u32,
                *const defines::wl_interface,
                *const std::ffi::c_void,
                *mut defines::wl_surface,
            ) -> *mut defines::wl_proxy;
            let create_fn: CreateBlurFn =
                std::mem::transmute(self.wayland.wl_proxy_marshal_constructor);

            let blur = create_fn(
                blur_manager as *mut defines::wl_proxy,
                0,                                    // create opcode
                std::ptr::null(), // org_kde_kwin_blur has no interface constant, use null
                std::ptr::null::<std::ffi::c_void>(), // new_id placeholder
                self.surface,
            );

            if blur.is_null() {
                log_debug!(
                    LogCategory::Platform,
                    "[Wayland] Failed to create KDE blur object"
                );
                return;
            }

            let blur = blur as *mut defines::org_kde_kwin_blur;

            // Set blur region to cover entire surface (NULL = entire surface)
            // org_kde_kwin_blur.set_region opcode is 1
            // Signature: set_region(region: object<wl_region>)
            type SetRegionFn =
                unsafe extern "C" fn(*mut defines::wl_proxy, u32, *const defines::wl_region);
            let set_region_fn: SetRegionFn = std::mem::transmute(self.wayland.wl_proxy_marshal);
            set_region_fn(
                blur as *mut defines::wl_proxy,
                1,                                      // set_region opcode
                std::ptr::null::<defines::wl_region>(), // NULL = entire surface
            );

            // Commit the blur
            // org_kde_kwin_blur.commit opcode is 2
            type CommitFn = unsafe extern "C" fn(*mut defines::wl_proxy, u32);
            let commit_fn: CommitFn = std::mem::transmute(self.wayland.wl_proxy_marshal);
            commit_fn(
                blur as *mut defines::wl_proxy,
                2, // commit opcode
            );

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
        let needs_work = self.common.frame_needs_regeneration || self.needs_redraw;
        if !needs_work || self.frame_callback_pending {
            return;
        }

        // CRITICAL: Make OpenGL context current BEFORE generate_frame
        // The image callbacks (RenderImageCallback) need the GL context to be current
        // to allocate textures and draw to them
        if let RenderMode::Gpu(ref gl_context, _) = self.render_mode {
            gl_context.make_current();
        }

        if self.common.frame_needs_regeneration {
            // FULL PATH: Regenerate layout + build full transaction
            if let Err(e) = self.regenerate_layout() {
                log_error!(
                    LogCategory::Layout,
                    "[Wayland] Layout regeneration failed: {:?}",
                    e
                );
            }
            self.common.frame_needs_regeneration = false;

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
                    let _tick_result = layout_window.scroll_manager.tick(now);
                }

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

        self.needs_redraw = false;

        match &mut self.render_mode {
            RenderMode::Gpu(gl_context, _) => {
                // 1. Wait for WebRender to be ready
                if let Some(renderer) = &mut self.common.renderer {
                    let (lock, cvar) = &*self.new_frame_ready;
                    let mut ready = lock.lock().unwrap();

                    // Non-blocking check - don't wait if not ready
                    if !*ready {
                        return;
                    }
                    *ready = false;
                    drop(ready); // Release lock before rendering

                    // 2. Update and render
                    renderer.update();
                    let physical_size = self.common.current_window_state.size.get_physical_size();
                    let device_size = webrender::api::units::DeviceIntSize::new(
                        physical_size.width as i32,
                        physical_size.height as i32,
                    );
                    if let Err(e) = renderer.render(device_size, 0) {
                        log_error!(
                            LogCategory::Rendering,
                            "[Wayland] WebRender render failed: {:?}",
                            e
                        );
                        return;
                    }

                    // 3. Swap buffers
                    if let Err(e) = gl_context.swap_buffers() {
                        log_error!(
                            LogCategory::Rendering,
                            "[Wayland] Swap buffers failed: {:?}",
                            e
                        );
                        return;
                    }

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
                // CPU rendering - draw to shared memory buffer
                cpu_state.draw_blue();
                unsafe {
                    (self.wayland.wl_surface_attach)(self.surface, cpu_state.buffer, 0, 0);
                    (self.wayland.wl_surface_damage)(
                        self.surface,
                        0,
                        0,
                        cpu_state.width,
                        cpu_state.height,
                    );
                }
            }
            RenderMode::Cpu(None) => {
                // CPU fallback not yet initialized - initialize it now if we have shm
                if !self.shm.is_null() {
                    let width = self.common.current_window_state.size.dimensions.width as i32;
                    let height = self.common.current_window_state.size.dimensions.height as i32;
                    match CpuFallbackState::new(&self.wayland, self.shm, width, height) {
                        Ok(cpu_state) => {
                            self.render_mode = RenderMode::Cpu(Some(cpu_state));
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
            let listener = defines::wl_callback_listener {
                done: frame_done_callback,
            };
            (self.wayland.wl_callback_add_listener)(
                frame_callback,
                &listener,
                self as *mut _ as *mut _,
            );
            (self.wayland.wl_surface_commit)(self.surface);
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
    _callback: *mut defines::wl_callback,
    _callback_data: u32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    window.frame_callback_pending = false;

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

        unsafe {
            // Clean up KDE blur resources
            if let Some(blur) = self.current_blur.take() {
                (self.wayland.wl_proxy_destroy)(blur as _);
            }
            if let Some(blur_manager) = self.blur_manager.take() {
                (self.wayland.wl_proxy_destroy)(blur_manager as _);
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

impl CpuFallbackState {
    fn new(
        wayland: &Rc<Wayland>,
        shm: *mut wl_shm,
        width: i32,
        height: i32,
    ) -> Result<Self, WindowError> {
        let stride = width * 4;
        let size = stride * height;

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
        let buffer = unsafe {
            (wayland.wl_shm_pool_create_buffer)(
                pool,
                0,
                width,
                height,
                stride,
                WL_SHM_FORMAT_ARGB8888,
            )
        };

        Ok(Self {
            wayland: wayland.clone(),
            pool,
            buffer,
            data: data as *mut u8,
            width,
            height,
            stride,
            fd, // Keep fd open - will be closed in Drop
        })
    }

    fn draw_blue(&self) {
        let size = (self.stride * self.height) as usize;
        let slice = unsafe { std::slice::from_raw_parts_mut(self.data, size) };
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
            if !self.buffer.is_null() {
                (self.wayland.wl_buffer_destroy)(self.buffer);
            }
            if !self.pool.is_null() {
                (self.wayland.wl_shm_pool_destroy)(self.pool);
            }
            if !self.data.is_null() {
                libc::munmap(self.data as *mut _, (self.stride * self.height) as usize);
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
        match &mut self.render_mode {
            RenderMode::Gpu(gl_context, _gl_functions) => {
                gl_context.resize(&self.wayland, width, height);
            }
            RenderMode::Cpu(Some(cpu_state)) => {
                // For CPU rendering, we need to recreate the buffer with new size
                // This is a simplified approach - in practice, you might want to
                // recreate the entire CpuFallbackState
                let _ = (cpu_state, width, height);
                // TODO: Implement CPU buffer resizing if needed
            }
            RenderMode::Cpu(None) => {}
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

    /// Get display information for Wayland
    ///
    /// Note: Wayland doesn't expose absolute positioning information to clients.
    /// This returns an approximation based on the window's size and scale.
    pub fn get_window_display_info(&self) -> Option<crate::desktop::display::DisplayInfo> {
        use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};

        let scale_factor = self.get_scale_factor();

        // Use actual window size if available, otherwise reasonable defaults
        let (width, height) = if self.common.current_window_state.size.dimensions.width > 0.0
            && self.common.current_window_state.size.dimensions.height > 0.0
        {
            // Use window dimensions as a proxy for display size
            // This is not accurate for multi-monitor setups, but Wayland doesn't
            // provide absolute display enumeration to clients
            (
                self.common.current_window_state.size.dimensions.width as i32,
                self.common.current_window_state.size.dimensions.height as i32,
            )
        } else {
            // Fallback to environment variables or defaults
            let width = std::env::var("WAYLAND_DISPLAY_WIDTH")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1920);
            let height = std::env::var("WAYLAND_DISPLAY_HEIGHT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1080);
            (width, height)
        };

        let bounds = LogicalRect::new(
            LogicalPosition::zero(),
            LogicalSize::new(width as f32, height as f32),
        );

        let work_area = LogicalRect::new(
            LogicalPosition::zero(),
            LogicalSize::new(width as f32, (height as i32 - 24).max(0) as f32),
        );

        Some(crate::desktop::display::DisplayInfo {
            name: "wayland-0".to_string(),
            bounds,
            work_area,
            scale_factor,
            is_primary: true,
            video_modes: vec![azul_core::window::VideoMode {
                size: azul_css::props::basic::LayoutSize::new(width as isize, height as isize),
                bit_depth: 32,
                refresh_rate: 60,
            }],
        })
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
        });
        let listener_context_ptr = Box::into_raw(listener_context);

        // 7. Add xdg_surface listener (configure events)
        let xdg_surface_listener = xdg_surface_listener {
            configure: popup_xdg_surface_configure,
        };

        unsafe {
            (wayland.xdg_surface_add_listener)(
                xdg_surface,
                &xdg_surface_listener,
                listener_context_ptr as *mut _,
            );
        }

        // 8. Add xdg_popup listener
        let popup_listener = xdg_popup_listener {
            configure: popup_configure,
            popup_done,
        };

        unsafe {
            (wayland.xdg_popup_add_listener)(
                xdg_popup,
                &popup_listener,
                listener_context_ptr as *mut _,
            );
        }

        // 9. Grab pointer for exclusive input (using parent's last serial)
        unsafe {
            (wayland.xdg_popup_grab)(xdg_popup, parent.seat, parent.pointer_state.serial);
        }

        // 9. Commit surface to make popup visible
        unsafe {
            (wayland.wl_surface_commit)(surface);
        }

        // 10. Create window state
        let current_window_state = FullWindowState {
            title: "Popup".to_string().into(),
            size: options.window_state.size,
            position: parent.current_window_state.position,
            flags: parent.current_window_state.flags,
            theme: parent.current_window_state.theme,
            debug_state: parent.current_window_state.debug_state,
            keyboard_state: parent.current_window_state.keyboard_state.clone(),
            mouse_state: parent.current_window_state.mouse_state.clone(),
            touch_state: parent.current_window_state.touch_state.clone(),
            ime_position: parent.current_window_state.ime_position,
            platform_specific_options: parent
                .current_window_state
                .platform_specific_options
                .clone(),
            renderer_options: parent.current_window_state.renderer_options,
            background_color: parent.current_window_state.background_color,
            layout_callback: options.window_state.layout_callback.clone(),
            close_callback: options.window_state.close_callback.clone(),
            monitor_id: parent.current_window_state.monitor_id,
            window_id: options.window_state.window_id.clone(),
            window_focused: false,
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
            fc_cache: parent.fc_cache.clone(),
            app_data: parent.app_data.clone(),
        })
    }

    /// Close the popup window
    pub fn close(&mut self) {
        if self.is_open {
            unsafe {
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
    }
}

// IME Position Management

impl WaylandWindow {
    /// Sync ime_position from window state to OS
    /// Sync IME position to OS (Wayland with text-input-v3 or GTK fallback)
    pub fn sync_ime_position_to_os(&self) {
        use azul_core::window::ImePosition;

        if let ImePosition::Initialized(rect) = self.common.current_window_state.ime_position {
            // Try text-input v3 protocol first (preferred, but requires compositor support)
            if let Some(text_input) = self.text_input {
                // zwp_text_input_v3_set_cursor_rectangle would be called here
                // However, this requires proper protocol bindings which are complex
                // For now, we note that this is where native Wayland IME would go
                log_debug!(
                    LogCategory::Platform,
                    "[Wayland] text-input v3 available but not yet implemented"
                );

                // The proper implementation would be:
                // zwp_text_input_v3_set_cursor_rectangle(
                //     text_input,
                //     rect.origin.x as i32,
                //     rect.origin.y as i32,
                //     rect.size.width as i32,
                //     rect.size.height as i32,
                // );
                // wl_display_flush(self.display);
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

    /// Show a tooltip at the given position (Wayland implementation using subsurface)
    fn show_tooltip(&mut self, text: String, x: i32, y: i32) {
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
        if let Some(tooltip) = self.tooltip.as_mut() {
            tooltip.show(text, x, y);
        }
    }

    /// Hide the tooltip (Wayland implementation)
    fn hide_tooltip(&mut self) {
        if let Some(tooltip) = self.tooltip.as_mut() {
            tooltip.hide();
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

            // Try to load D-Bus library
            let dbus_lib = match dbus::DBusLib::new() {
                Ok(lib) => lib,
                Err(e) => {
                    log_warn!(
                        LogCategory::Platform,
                        "[Wayland] Failed to load D-Bus library: {}",
                        e
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

            // Try to load D-Bus library
            let dbus_lib = match dbus::DBusLib::new() {
                Ok(lib) => lib,
                Err(e) => {
                    log_warn!(
                        LogCategory::Platform,
                        "[Wayland] Failed to load D-Bus library: {}",
                        e
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
extern "C" fn popup_done(data: *mut c_void, xdg_popup: *mut defines::xdg_popup) {
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
        // Destroy the popup when compositor dismisses it
        (ctx.wayland.xdg_popup_destroy)(xdg_popup);
        (ctx.wayland.wl_proxy_destroy)(ctx.xdg_surface as *mut _);
    }

    // TODO: Signal to application that popup was dismissed
    // This would require storing a callback or channel in PopupListenerContext
}
