//! X11 implementation for Linux using the shell2 architecture.

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
    os::raw::c_int,
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
        event::{self, PlatformWindow},
        WindowError,
    },
    wr_translate2::{self, AsyncHitTester, Notifier},
};
use crate::{log_debug, log_error, log_info, log_trace, log_warn};

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
        AllocNone, CWBackPixmap, CWBorderPixel, CWColormap, CWEventMask, InputOutput, TrueColor,
        XVisualInfo,
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

    let attr_mask = CWColormap | CWBackPixmap | CWBorderPixel | CWEventMask;

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

pub struct X11Window {
    pub xlib: Rc<Xlib>,
    pub egl: Rc<Egl>,
    pub xkb: Rc<Xkb>,
    pub xrender: Option<Rc<dlopen::Xrender>>, // Optional XRender for ARGB visual detection
    pub gtk_im: Option<Rc<Gtk3Im>>,           // Optional GTK IM context for IME
    pub gtk_im_context: Option<*mut dlopen::GtkIMContext>, // GTK IM context instance
    pub display: *mut Display,
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
    pub fn poll_event(&mut self) -> Option<X11Event> {
        // Check timers and threads before processing X11 events
        self.check_timers_and_threads();

        while unsafe { (self.xlib.XPending)(self.display) } > 0 {
            let mut event: XEvent = unsafe { std::mem::zeroed() };
            unsafe { (self.xlib.XNextEvent)(self.display, &mut event) };

            if let Some(ime) = &self.ime_manager {
                if ime.filter_event(&mut event) {
                    continue; // Event was consumed by IME
                }
            }

            // Process event with V2 handlers
            let result = match unsafe { event.type_ } {
                defines::Expose => {
                    // Actually render and present the frame instead of just requesting another redraw
                    if let Err(e) = self.render_and_present() {
                        log_warn!(
                            LogCategory::Rendering,
                            "[X11] render_and_present failed: {:?}",
                            e
                        );
                    }
                    ProcessEventResult::DoNothing
                }
                defines::FocusIn => {
                    // Window gained focus
                    self.common.current_window_state.window_focused = true;
                    self.dynamic_selector_context.window_focused = true;

                    // Phase 2: OnFocus callback - sync IME position after focus
                    self.sync_ime_position_to_os();

                    ProcessEventResult::DoNothing
                }
                defines::FocusOut => {
                    // Window lost focus
                    self.common.current_window_state.window_focused = false;
                    self.dynamic_selector_context.window_focused = false;
                    ProcessEventResult::DoNothing
                }
                defines::ConfigureNotify => {
                    let ev = unsafe { &event.configure };
                    let (new_width, new_height) = (ev.width as u32, ev.height as u32);

                    // Store old context for breakpoint detection
                    let old_context = self.dynamic_selector_context.clone();

                    // Check if size changed
                    let size_changed = self.common.current_window_state.size.get_physical_size()
                        != PhysicalSize::new(new_width, new_height);

                    // Check if position changed (might have moved to different monitor with
                    // different DPI)
                    let position_changed = match self.common.current_window_state.position {
                        azul_core::window::WindowPosition::Initialized(pos) => {
                            pos.x != ev.x || pos.y != ev.y
                        }
                        _ => true,
                    };

                    if size_changed {
                        self.common.current_window_state.size.dimensions =
                            LogicalSize::new(new_width as f32, new_height as f32);

                        // Update dynamic selector context with new viewport dimensions
                        self.dynamic_selector_context.viewport_width = new_width as f32;
                        self.dynamic_selector_context.viewport_height = new_height as f32;
                        self.dynamic_selector_context.orientation = if new_width > new_height {
                            azul_css::dynamic_selector::OrientationType::Landscape
                        } else {
                            azul_css::dynamic_selector::OrientationType::Portrait
                        };

                        // Check if any CSS breakpoints were crossed
                        let breakpoints =
                            [320.0, 480.0, 640.0, 768.0, 1024.0, 1280.0, 1440.0, 1920.0];
                        if old_context.viewport_breakpoint_changed(
                            &self.dynamic_selector_context,
                            &breakpoints,
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

                        self.regenerate_layout().ok();
                    }

                    // Update position
                    self.common.current_window_state.position =
                        azul_core::window::WindowPosition::Initialized(
                            azul_core::geom::PhysicalPositionI32::new(ev.x, ev.y),
                        );

                    // If position changed, check for DPI change (moved to different monitor)
                    if position_changed && !size_changed {
                        // Get current display DPI at new position
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

                            // Only regenerate if DPI changed significantly (avoid rounding errors)
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
                defines::ClientMessage => {
                    let msg_atom = unsafe { event.client_message.data.l[0] } as Atom;
                    log_debug!(
                        LogCategory::Window,
                        "[X11] ClientMessage received: msg_atom={}, wm_delete_atom={}",
                        msg_atom,
                        self.wm_delete_window_atom
                    );
                    if msg_atom == self.wm_delete_window_atom {
                        log_info!(
                            LogCategory::Window,
                            "[X11] WM_DELETE_WINDOW matched - closing window"
                        );
                        self.is_open = false;
                        return Some(X11Event::Close);
                    }
                    ProcessEventResult::DoNothing
                }
                defines::ButtonPress | defines::ButtonRelease => {
                    self.handle_mouse_button(unsafe { &event.button })
                }
                defines::MotionNotify => self.handle_mouse_move(unsafe { &event.motion }),
                defines::KeyPress | defines::KeyRelease => {
                    self.handle_keyboard(unsafe { &mut event.key })
                }
                defines::EnterNotify | defines::LeaveNotify => {
                    self.handle_mouse_crossing(unsafe { &event.crossing })
                }
                other => {
                    // Check for XRandR screen change event (dynamic event type)
                    if let Some(event_base) = self.xrandr_event_base {
                        if other == event_base {
                            // RRScreenChangeNotify — monitor topology changed
                            log_debug!(
                                LogCategory::Platform,
                                "[X11] XRandR screen change detected, refreshing monitor cache"
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

            // Request redraw if needed (but not for Expose which already called render_and_present)
            if result != ProcessEventResult::DoNothing {
                self.request_redraw();
            }
        }

        None
    }

    pub fn present(&mut self) -> Result<(), WindowError> {
        match &self.render_mode {
            RenderMode::Gpu(gl_context, _) => gl_context.swap_buffers(),
            RenderMode::Cpu(gc) => {
                if let Some(gc) = gc {
                    // Simple blue fallback for CPU rendering
                    unsafe {
                        (self.xlib.XSetForeground)(self.display, *gc, 0x0000FF);
                        (self.xlib.XFillRectangle)(
                            self.display,
                            self.window,
                            *gc,
                            0,
                            0,
                            self.common.current_window_state.size.dimensions.width as u32,
                            self.common.current_window_state.size.dimensions.height as u32,
                        );
                    }
                }
                unsafe { (self.xlib.XFlush)(self.display) };
                Ok(())
            }
        }?;

        // CI testing: Exit successfully after first frame render if env var is set
        if std::env::var("AZUL_EXIT_SUCCESS_AFTER_FRAME_RENDER").is_ok() {
            log_info!(
                LogCategory::General,
                "[CI] AZUL_EXIT_SUCCESS_AFTER_FRAME_RENDER set - exiting with success"
            );
            std::process::exit(0);
        }

        Ok(())
    }

    pub fn request_redraw(&mut self) {
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

    pub fn sync_clipboard(
        &mut self,
        clipboard_manager: &mut azul_layout::managers::clipboard::ClipboardManager,
    ) {
        clipboard::sync_clipboard(clipboard_manager);
    }

    pub fn close(&mut self) {
        if self.is_open {
            self.is_open = false;
            unsafe {
                (self.xlib.XDestroyWindow)(self.display, self.window);
                // Free the ARGB colormap if we created one
                if let Some(colormap) = self.argb_colormap.take() {
                    (self.xlib.XFreeColormap)(self.display, colormap);
                }
                (self.xlib.XCloseDisplay)(self.display);
            }
        }
    }

    pub fn is_open(&self) -> bool {
        self.is_open
    }
}

impl X11Window {
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

        let display = unsafe { (xlib.XOpenDisplay)(std::ptr::null()) };
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

        let use_csd = options.window_state.flags.decorations == WindowDecorations::None;
        if use_csd {
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

        let ime_manager = events::ImeManager::new(&xlib, display, window_handle);

        let (
            render_mode,
            renderer,
            render_api,
            hit_tester,
            document_id,
            id_namespace,
            gl_context_ptr,
        ) = match gl::GlContext::new(&xlib, &egl, display, window_handle) {
            Ok(gl_context) => {
                gl_context.make_current();
                gl_context.configure_vsync(options.window_state.renderer_options.vsync);
                let gl_functions = GlFunctions::initialize(&egl).unwrap();

                let new_frame_ready = Arc::new((Mutex::new(false), Condvar::new()));
                let (renderer, sender) = webrender::create_webrender_instance(
                    gl_functions.functions.clone(),
                    Box::new(Notifier {
                        new_frame_ready: new_frame_ready.clone(),
                    }),
                    wr_translate2::default_renderer_options(
                        &options,
                        wr_translate2::create_program_cache(&gl_functions.functions),
                    ),
                    None,
                )
                .map_err(|e| {
                    WindowError::PlatformError(format!("WebRender init failed: {:?}", e))
                })?;

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
                let gl_context_ptr = OptionGlContextPtr::Some(GlContextPtr::new(
                    RendererType::Hardware,
                    gl_functions.functions.clone(),
                ));
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
        };

        let mut window = Self {
            xlib,
            egl,
            xkb,
            xrender,
            gtk_im,
            gtk_im_context,
            display,
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
                },
                previous_window_state: None,
                renderer,
                render_api,
                hit_tester,
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
                display_list_initialized: false,
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

        unsafe { (window.xlib.XMapWindow)(display, window.window) };
        unsafe { (window.xlib.XFlush)(display) };

        // Try to subscribe to XRandR screen change notifications (optional)
        window.xrandr_event_base = try_subscribe_xrandr(display, root);

        // Position window on requested monitor (or center on primary)
        // Convert u32 to MonitorId
        let monitor_id_typed = azul_core::window::MonitorId {
            index: monitor_id as usize,
            hash: 0,
        };
        window.position_window_on_monitor(monitor_id_typed, position, size);

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
                            .set_window_properties(window.window, display as *mut _)
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

        // Register window in global registry for multi-window support
        unsafe {
            super::registry::register_x11_window(window.window, &mut window as *mut _);
        }

        // Invoke create_callback if provided (for GL resource upload, config loading, etc.)
        // This runs AFTER GL context is ready but BEFORE any layout is done
        if let Some(mut callback) = create_callback.into_option() {
            use azul_core::window::RawWindowHandle;

            let raw_handle = RawWindowHandle::Xlib(azul_core::window::XlibHandle {
                window: window.window as u64,
                display: window.display as *mut _,
            });

            // Initialize LayoutWindow if not already done
            if window.layout_window.is_none() {
                let mut layout_window =
                    azul_layout::window::LayoutWindow::new((*window.resources.fc_cache).clone())
                        .map_err(|e| {
                            WindowError::PlatformError(format!(
                                "Failed to create LayoutWindow: {:?}",
                                e
                            ))
                        })?;

                if let Some(doc_id) = window.document_id {
                    layout_window.document_id = doc_id;
                }
                if let Some(ns_id) = window.id_namespace {
                    layout_window.id_namespace = ns_id;
                }
                layout_window.current_window_state = window.current_window_state.clone();
                layout_window.renderer_type = Some(azul_core::window::RendererType::Hardware);
                // Initialize monitor cache once at window creation
                if let Ok(mut guard) = layout_window.monitors.lock() {
                    *guard = crate::desktop::display::get_monitors();
                }
                window.layout_window = Some(layout_window);
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
                &window.gl_context_ptr,
                window.resources.system_style.clone(),
                &azul_layout::callbacks::ExternalSystemCallbacks::rust_internal(),
                &window.previous_window_state,
                &window.current_window_state,
                &window.renderer_resources,
            );

            drop(app_data_ref);
            use crate::desktop::shell2::common::event::PlatformWindow;
            for change in &changes {
                let r = window.apply_user_change(change);
                if r != azul_core::events::ProcessEventResult::DoNothing {
                    window.frame_needs_regeneration = true;
                }
            }
        }

        // CRITICAL: Always initialize LayoutWindow if not already done
        // This is needed for rendering even without callbacks or debug mode
        if window.layout_window.is_none() {
            let mut layout_window =
                azul_layout::window::LayoutWindow::new((*window.resources.fc_cache).clone())
                    .map_err(|e| {
                        WindowError::PlatformError(format!(
                            "Failed to create LayoutWindow: {:?}",
                            e
                        ))
                    })?;

            if let Some(doc_id) = window.document_id {
                layout_window.document_id = doc_id;
            }
            if let Some(ns_id) = window.id_namespace {
                layout_window.id_namespace = ns_id;
            }
            layout_window.current_window_state = window.current_window_state.clone();
            layout_window.renderer_type = Some(azul_core::window::RendererType::Hardware);
            // Initialize monitor cache once at window creation
            if let Ok(mut guard) = layout_window.monitors.lock() {
                *guard = crate::desktop::display::get_monitors();
            }
            window.layout_window = Some(layout_window);
        }

        // Register debug timer is now done from run() with explicit channel + component map

        // Apply initial background material if not Opaque
        {
            use azul_core::window::WindowBackgroundMaterial;
            let initial_material = window.current_window_state.flags.background_material;
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

    /// Position window on requested monitor, or center on primary monitor
    fn position_window_on_monitor(
        &mut self,
        monitor_id: azul_core::window::MonitorId,
        position: azul_core::window::WindowPosition,
        size: azul_core::window::WindowSize,
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
        };

        // Move window to calculated position
        unsafe {
            (self.xlib.XMoveWindow)(self.display, self.window, x, y);
            (self.xlib.XFlush)(self.display);
        }
    }

    fn process_events(&mut self) {
        // Process GNOME menu DBus messages (non-blocking)
        if let Some(ref manager) = self.gnome_menu {
            manager.process_messages();
        }

        // Process any pending menu callbacks from DBus
        self.process_pending_menu_callbacks();

        let result = self.process_window_events(0);
        if result != ProcessEventResult::DoNothing {
            self.request_redraw();
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
                | ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                    self.common.frame_needs_regeneration = true;
                    self.request_redraw();
                }
                // ShouldUpdateDisplayListCurrentWindow: pending IFrame updates are
                // queued in layout_window.pending_iframe_updates and will be processed
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

    pub fn wait_for_events(&mut self) -> Result<(), WindowError> {
        use super::super::common::event::PlatformWindow;
        use std::mem;

        let connection_fd = unsafe { (self.xlib.XConnectionNumber)(self.display) };

        unsafe {
            // Flush pending requests first
            (self.xlib.XFlush)(self.display);

            // Check if there are already pending events
            if (self.xlib.XPending)(self.display) > 0 {
                let mut event: XEvent = mem::zeroed();
                (self.xlib.XNextEvent)(self.display, &mut event);
                self.handle_event(&mut event);
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

            // If no timers, use -1 (block indefinitely), otherwise block until something fires
            let timeout_ms = if self.timer_fds.is_empty() { -1 } else { -1 };

            let result = libc::poll(
                pollfds.as_mut_ptr(),
                pollfds.len() as libc::nfds_t,
                timeout_ms,
            );

            if result > 0 {
                // Check X11 connection
                if pollfds[0].revents & libc::POLLIN != 0 {
                    if (self.xlib.XPending)(self.display) > 0 {
                        let mut event: XEvent = mem::zeroed();
                        (self.xlib.XNextEvent)(self.display, &mut event);
                        self.handle_event(&mut event);
                    }
                }

                // Check timerfd's - if any fired, invoke timer callbacks
                let mut any_timer_fired = false;
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

    fn handle_event(&mut self, event: &mut XEvent) {
        if let Some(ime) = &self.ime_manager {
            if ime.filter_event(event) {
                return;
            }
        }

        // Process event with V2 handlers
        let result = match unsafe { event.type_ } {
            defines::Expose => {
                self.request_redraw();
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
            defines::KeyPress | defines::KeyRelease => {
                self.handle_keyboard(unsafe { &mut event.key })
            }
            defines::EnterNotify | defines::LeaveNotify => {
                self.handle_mouse_crossing(unsafe { &event.crossing })
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

        // Request redraw if needed
        if result != ProcessEventResult::DoNothing {
            self.request_redraw();
        }
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

        // Update accessibility tree after layout
        #[cfg(feature = "a11y")]
        if let Some(layout_window) = self.common.layout_window.as_ref() {
            if let Some(tree_update) = layout_window.a11y_manager.last_tree_update.clone() {
                self.accessibility_adapter.update_tree(tree_update);
            }
        }

        // CRITICAL: Make OpenGL context current BEFORE generate_frame
        // The image callbacks (RenderImageCallback) need the GL context to be current
        // to allocate textures and draw to them
        if let RenderMode::Gpu(ref gl_context, _) = self.render_mode {
            gl_context.make_current();
        }

        // Send frame immediately (like Windows - ensures WebRender transaction is sent)
        let layout_window = self.common.layout_window.as_mut().ok_or("No layout window")?;
        crate::desktop::shell2::common::layout::generate_frame(
            layout_window,
            self.common.render_api.as_mut().ok_or("No render API")?,
            self.common.document_id.ok_or("No document ID")?,
            &self.common.gl_context_ptr,
        );
        if let Some(render_api) = self.common.render_api.as_mut() {
            render_api.flush_scene_builder();
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
                    let _tick_result = layout_window.scroll_manager.tick(now);
                }

                // Process pending IFrame updates (queued by ScrollTo → check_and_queue_iframe_reinvoke).
                // If present, we need a full display list rebuild rather than lightweight.
                let has_iframe_updates = !layout_window.pending_iframe_updates.is_empty();
                if has_iframe_updates {
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
        let framebuffer_size = webrender::api::units::DeviceIntSize::new(
            physical_size.width as i32,
            physical_size.height as i32,
        );

        match renderer.render(framebuffer_size, 0) {
            Ok(_results) => {}
            Err(errors) => {
                log_warn!(LogCategory::Rendering, "[X11] Render errors: {:?}", errors);
                return Err(WindowError::PlatformError(format!(
                    "Render failed: {:?}",
                    errors
                )));
            }
        }

        // Step 6: Swap buffers
        match &self.render_mode {
            RenderMode::Gpu(gl_context, _) => {
                if let Err(e) = gl_context.swap_buffers() {
                    return Err(e);
                }
            }
            RenderMode::Cpu(gc) => {
                if let Some(gc) = gc {
                    unsafe {
                        (self.xlib.XSetForeground)(self.display, *gc, 0x0000FF);
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

        // If any scrollbar is still visible (opacity > 0), schedule another
        // frame so that synchronize_scrollbar_opacity can continue driving
        // the fade-out animation.
        let needs_fade_frame = self.common.layout_window.as_ref()
            .map(|lw| lw.gpu_state_manager.scrollbar_fade_active)
            .unwrap_or(false);
        if needs_fade_frame {
            self.request_redraw();
        }

        // CI testing: Exit successfully after first frame render if env var is set
        if std::env::var("AZUL_EXIT_SUCCESS_AFTER_FRAME_RENDER").is_ok() {
            log_debug!(
                LogCategory::General,
                "[CI] AZUL_EXIT_SUCCESS_AFTER_FRAME_RENDER set - exiting"
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
                let screen = (self.xlib.XDefaultScreen)(self.display);
                let root = (self.xlib.XRootWindow)(self.display, screen);

                let mut event: defines::XClientMessageEvent = std::mem::zeroed();
                event.type_ = defines::ClientMessage;
                event.window = self.window;
                event.message_type = (self.xlib.XInternAtom)(
                    self.display,
                    b"_NET_WM_STATE\0".as_ptr() as *const i8,
                    0,
                );
                event.format = 32;
                event.data.l[0] = 1; // _NET_WM_STATE_ADD
                event.data.l[1] = (self.xlib.XInternAtom)(
                    self.display,
                    b"_NET_WM_STATE_MAXIMIZED_VERT\0".as_ptr() as *const i8,
                    0,
                ) as i64;
                event.data.l[2] = (self.xlib.XInternAtom)(
                    self.display,
                    b"_NET_WM_STATE_MAXIMIZED_HORZ\0".as_ptr() as *const i8,
                    0,
                ) as i64;
                event.data.l[3] = 1;

                (self.xlib.XSendEvent)(
                    self.display,
                    root,
                    0,
                    defines::SubstructureNotifyMask | defines::SubstructureRedirectMask,
                    &mut event as *mut _ as *mut defines::XEvent,
                );
            },
            WindowFrame::Fullscreen => unsafe {
                let screen = (self.xlib.XDefaultScreen)(self.display);
                let root = (self.xlib.XRootWindow)(self.display, screen);

                let mut event: defines::XClientMessageEvent = std::mem::zeroed();
                event.type_ = defines::ClientMessage;
                event.window = self.window;
                event.message_type = (self.xlib.XInternAtom)(
                    self.display,
                    b"_NET_WM_STATE\0".as_ptr() as *const i8,
                    0,
                );
                event.format = 32;
                event.data.l[0] = 1; // _NET_WM_STATE_ADD
                event.data.l[1] = (self.xlib.XInternAtom)(
                    self.display,
                    b"_NET_WM_STATE_FULLSCREEN\0".as_ptr() as *const i8,
                    0,
                ) as i64;
                event.data.l[3] = 1;

                (self.xlib.XSendEvent)(
                    self.display,
                    root,
                    0,
                    defines::SubstructureNotifyMask | defines::SubstructureRedirectMask,
                    &mut event as *mut _ as *mut defines::XEvent,
                );
            },
            WindowFrame::Minimized => unsafe {
                (self.xlib.XUnmapWindow)(self.display, self.window);
            },
            WindowFrame::Normal => {} // Already in normal state
        }

        // Always-on-top
        if self.common.current_window_state.flags.is_always_on_top {
            unsafe {
                let screen = (self.xlib.XDefaultScreen)(self.display);
                let root = (self.xlib.XRootWindow)(self.display, screen);

                let mut event: defines::XClientMessageEvent = std::mem::zeroed();
                event.type_ = defines::ClientMessage;
                event.window = self.window;
                event.message_type = (self.xlib.XInternAtom)(
                    self.display,
                    b"_NET_WM_STATE\0".as_ptr() as *const i8,
                    0,
                );
                event.format = 32;
                event.data.l[0] = 1; // _NET_WM_STATE_ADD
                event.data.l[1] = (self.xlib.XInternAtom)(
                    self.display,
                    b"_NET_WM_STATE_ABOVE\0".as_ptr() as *const i8,
                    0,
                ) as i64;
                event.data.l[3] = 1;

                (self.xlib.XSendEvent)(
                    self.display,
                    root,
                    0,
                    defines::SubstructureNotifyMask | defines::SubstructureRedirectMask,
                    &mut event as *mut _ as *mut defines::XEvent,
                );
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
                azul_core::window::WindowPosition::Uninitialized => {}
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
                WindowFrame::Maximized => {
                    // Maximize via _NET_WM_STATE
                    unsafe {
                        let screen = (self.xlib.XDefaultScreen)(self.display);
                        let root = (self.xlib.XRootWindow)(self.display, screen);

                        let mut event: defines::XClientMessageEvent = std::mem::zeroed();
                        event.type_ = defines::ClientMessage;
                        event.window = self.window;
                        event.message_type = (self.xlib.XInternAtom)(
                            self.display,
                            b"_NET_WM_STATE\0".as_ptr() as *const i8,
                            0,
                        );
                        event.format = 32;
                        event.data.l[0] = 1; // _NET_WM_STATE_ADD
                        event.data.l[1] = (self.xlib.XInternAtom)(
                            self.display,
                            b"_NET_WM_STATE_MAXIMIZED_VERT\0".as_ptr() as *const i8,
                            0,
                        ) as i64;
                        event.data.l[2] = (self.xlib.XInternAtom)(
                            self.display,
                            b"_NET_WM_STATE_MAXIMIZED_HORZ\0".as_ptr() as *const i8,
                            0,
                        ) as i64;
                        event.data.l[3] = 1; // Source indication

                        (self.xlib.XSendEvent)(
                            self.display,
                            root,
                            0,
                            defines::SubstructureNotifyMask | defines::SubstructureRedirectMask,
                            &mut event as *mut _ as *mut defines::XEvent,
                        );
                    }
                }
                WindowFrame::Normal => {
                    // Restore to normal - remove maximize and fullscreen states
                    unsafe {
                        let screen = (self.xlib.XDefaultScreen)(self.display);
                        let root = (self.xlib.XRootWindow)(self.display, screen);

                        if previous.flags.frame == WindowFrame::Maximized {
                            let mut event: defines::XClientMessageEvent = std::mem::zeroed();
                            event.type_ = defines::ClientMessage;
                            event.window = self.window;
                            event.message_type = (self.xlib.XInternAtom)(
                                self.display,
                                b"_NET_WM_STATE\0".as_ptr() as *const i8,
                                0,
                            );
                            event.format = 32;
                            event.data.l[0] = 0; // _NET_WM_STATE_REMOVE
                            event.data.l[1] = (self.xlib.XInternAtom)(
                                self.display,
                                b"_NET_WM_STATE_MAXIMIZED_VERT\0".as_ptr() as *const i8,
                                0,
                            ) as i64;
                            event.data.l[2] = (self.xlib.XInternAtom)(
                                self.display,
                                b"_NET_WM_STATE_MAXIMIZED_HORZ\0".as_ptr() as *const i8,
                                0,
                            ) as i64;
                            event.data.l[3] = 1; // Source indication

                            (self.xlib.XSendEvent)(
                                self.display,
                                root,
                                0,
                                defines::SubstructureNotifyMask | defines::SubstructureRedirectMask,
                                &mut event as *mut _ as *mut defines::XEvent,
                            );
                        }

                        if previous.flags.frame == WindowFrame::Fullscreen {
                            let mut event: defines::XClientMessageEvent = std::mem::zeroed();
                            event.type_ = defines::ClientMessage;
                            event.window = self.window;
                            event.message_type = (self.xlib.XInternAtom)(
                                self.display,
                                b"_NET_WM_STATE\0".as_ptr() as *const i8,
                                0,
                            );
                            event.format = 32;
                            event.data.l[0] = 0; // _NET_WM_STATE_REMOVE
                            event.data.l[1] = (self.xlib.XInternAtom)(
                                self.display,
                                b"_NET_WM_STATE_FULLSCREEN\0".as_ptr() as *const i8,
                                0,
                            ) as i64;
                            event.data.l[3] = 1; // Source indication

                            (self.xlib.XSendEvent)(
                                self.display,
                                root,
                                0,
                                defines::SubstructureNotifyMask | defines::SubstructureRedirectMask,
                                &mut event as *mut _ as *mut defines::XEvent,
                            );
                        }

                        if previous.flags.frame == WindowFrame::Minimized {
                            (self.xlib.XMapWindow)(self.display, self.window);
                        }
                    }
                }
                WindowFrame::Fullscreen => {
                    // Set fullscreen via _NET_WM_STATE
                    unsafe {
                        let screen = (self.xlib.XDefaultScreen)(self.display);
                        let root = (self.xlib.XRootWindow)(self.display, screen);

                        // If previously maximized, remove maximize first
                        if previous.flags.frame == WindowFrame::Maximized {
                            let mut event: defines::XClientMessageEvent = std::mem::zeroed();
                            event.type_ = defines::ClientMessage;
                            event.window = self.window;
                            event.message_type = (self.xlib.XInternAtom)(
                                self.display,
                                b"_NET_WM_STATE\0".as_ptr() as *const i8,
                                0,
                            );
                            event.format = 32;
                            event.data.l[0] = 0; // _NET_WM_STATE_REMOVE
                            event.data.l[1] = (self.xlib.XInternAtom)(
                                self.display,
                                b"_NET_WM_STATE_MAXIMIZED_VERT\0".as_ptr() as *const i8,
                                0,
                            ) as i64;
                            event.data.l[2] = (self.xlib.XInternAtom)(
                                self.display,
                                b"_NET_WM_STATE_MAXIMIZED_HORZ\0".as_ptr() as *const i8,
                                0,
                            ) as i64;
                            event.data.l[3] = 1;

                            (self.xlib.XSendEvent)(
                                self.display,
                                root,
                                0,
                                defines::SubstructureNotifyMask | defines::SubstructureRedirectMask,
                                &mut event as *mut _ as *mut defines::XEvent,
                            );
                        }

                        let mut event: defines::XClientMessageEvent = std::mem::zeroed();
                        event.type_ = defines::ClientMessage;
                        event.window = self.window;
                        event.message_type = (self.xlib.XInternAtom)(
                            self.display,
                            b"_NET_WM_STATE\0".as_ptr() as *const i8,
                            0,
                        );
                        event.format = 32;
                        event.data.l[0] = 1; // _NET_WM_STATE_ADD
                        event.data.l[1] = (self.xlib.XInternAtom)(
                            self.display,
                            b"_NET_WM_STATE_FULLSCREEN\0".as_ptr() as *const i8,
                            0,
                        ) as i64;
                        event.data.l[3] = 1; // Source indication

                        (self.xlib.XSendEvent)(
                            self.display,
                            root,
                            0,
                            defines::SubstructureNotifyMask | defines::SubstructureRedirectMask,
                            &mut event as *mut _ as *mut defines::XEvent,
                        );
                    }
                }
            }
        }

        // Always-on-top changed?
        if previous.flags.is_always_on_top != current.flags.is_always_on_top {
            unsafe {
                let screen = (self.xlib.XDefaultScreen)(self.display);
                let root = (self.xlib.XRootWindow)(self.display, screen);

                let mut event: defines::XClientMessageEvent = std::mem::zeroed();
                event.type_ = defines::ClientMessage;
                event.window = self.window;
                event.message_type = (self.xlib.XInternAtom)(
                    self.display,
                    b"_NET_WM_STATE\0".as_ptr() as *const i8,
                    0,
                );
                event.format = 32;
                event.data.l[0] = if current.flags.is_always_on_top { 1 } else { 0 };
                event.data.l[1] = (self.xlib.XInternAtom)(
                    self.display,
                    b"_NET_WM_STATE_ABOVE\0".as_ptr() as *const i8,
                    0,
                ) as i64;
                event.data.l[3] = 1; // Source indication

                (self.xlib.XSendEvent)(
                    self.display,
                    root,
                    0,
                    defines::SubstructureNotifyMask | defines::SubstructureRedirectMask,
                    &mut event as *mut _ as *mut defines::XEvent,
                );
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
                b"_NET_WM_WINDOW_OPACITY\0".as_ptr() as *const i8,
                0, // create if doesn't exist
            );

            let cardinal_atom =
                (self.xlib.XInternAtom)(self.display, b"CARDINAL\0".as_ptr() as *const i8, 0);

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

    /// Calculates the DPI of the screen the window is on.
    pub fn get_screen_dpi(&self) -> Option<f32> {
        Some(96.0)
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
            window: self.window,
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
                window: self.window,
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
            match tooltip::TooltipWindow::new(self.xlib.clone(), self.display, self.window) {
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
            // Use default DPI factor - tooltips don't need precise scaling
            let dpi = DpiScaleFactor::new(1.0);

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
        super::registry::unregister_x11_window(self.window);
        self.close();
    }
}

// IME Position Management

impl X11Window {
    /// Sync ime_position from window state to OS
    /// Sync IME position to OS (X11 with XIM)
    pub fn sync_ime_position_to_os(&self) {
        use std::ffi::CString;

        use azul_core::window::ImePosition;
        use defines::{XPoint, XRectangle};

        if let ImePosition::Initialized(rect) = self.common.current_window_state.ime_position {
            // Use XIM if available (preferred over GTK)
            if let Some(ref ime_mgr) = self.ime_manager {
                let spot = XPoint {
                    x: rect.origin.x as i16,
                    y: rect.origin.y as i16,
                };

                let area = XRectangle {
                    x: rect.origin.x as i16,
                    y: rect.origin.y as i16,
                    width: rect.size.width as u16,
                    height: rect.size.height as u16,
                };

                unsafe {
                    let spot_location = CString::new("spotLocation").unwrap();
                    let preedit_attr = CString::new("preeditAttributes").unwrap();

                    // Set spot location (cursor position) for preedit window
                    let xic = ime_mgr.get_xic();
                    (self.xlib.XSetICValues)(
                        xic,
                        preedit_attr.as_ptr(),
                        spot_location.as_ptr(),
                        &spot as *const XPoint,
                        std::ptr::null::<i8>(),
                    );
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
                (self.xlib.XInternAtom)(self.display, b"_NET_WM_STATE\0".as_ptr() as *const i8, 0);

            // Get _NET_WM_STATE_ABOVE atom
            let net_wm_state_above = (self.xlib.XInternAtom)(
                self.display,
                b"_NET_WM_STATE_ABOVE\0".as_ptr() as *const i8,
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
                let mut nitems: u64 = 0;
                let mut bytes_after: u64 = 0;
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

            // Try to load D-Bus library
            let dbus_lib = match dbus::DBusLib::new() {
                Ok(lib) => lib,
                Err(e) => {
                    log_warn!(
                        LogCategory::Platform,
                        "[X11] Failed to load D-Bus library: {}",
                        e
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

            // Try to load D-Bus library
            let dbus_lib = match dbus::DBusLib::new() {
                Ok(lib) => lib,
                Err(e) => {
                    log_warn!(
                        LogCategory::Platform,
                        "[X11] Failed to load D-Bus library: {}",
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
