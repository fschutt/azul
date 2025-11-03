//! X11 implementation for Linux using the shell2 architecture.

pub mod accessibility;
pub mod defines;
pub mod dlopen;
pub mod events;
pub mod gl;
pub mod menu;

use std::{
    cell::RefCell,
    ffi::{c_void, CStr, CString},
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
use azul_layout::{
    managers::hover::InputPointId,
    window::LayoutWindow,
    window_state::{FullWindowState, WindowCreateOptions, WindowState},
    ScrollbarDragState,
};
use rust_fontconfig::FcFontCache;
use webrender::Renderer as WrRenderer;

use self::{
    defines::*,
    dlopen::{Egl, Library, Xkb, Xlib},
};
use super::common::gl::GlFunctions;
use crate::desktop::{
    shell2::common::{
        event_v2::{self, PlatformWindowV2},
        PlatformWindow, RenderContext, WindowError, WindowProperties,
    },
    wr_translate2::{self, AsyncHitTester, Notifier},
};

/// Tracks the current rendering mode of the window.
enum RenderMode {
    Gpu(gl::GlContext, GlFunctions),
    Cpu(Option<GC>), // Option to hold the Graphics Context
}

pub struct X11Window {
    pub xlib: Rc<Xlib>,
    pub egl: Rc<Egl>,
    pub xkb: Rc<Xkb>,
    pub display: *mut Display,
    pub window: Window,
    pub is_open: bool,
    wm_delete_window_atom: Atom,
    ime_manager: Option<events::ImeManager>,
    render_mode: RenderMode,

    // Shell2 state
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

    // V2 Event system state
    pub scrollbar_drag_state: Option<ScrollbarDragState>,
    pub last_hovered_node: Option<event_v2::HitTestNode>,
    pub frame_needs_regeneration: bool,

    // Multi-window support
    /// Pending window creation requests (for popup menus, dialogs, etc.)
    /// Processed in Phase 3 of the event loop
    pub pending_window_creates: Vec<WindowCreateOptions>,

    // GNOME native menu integration (optional)
    #[cfg(feature = "gnome-menus")]
    pub gnome_menu: Option<super::gnome_menu::GnomeMenuManager>,

    // Shared resources
    pub resources: Arc<super::AppResources>,

    // Accessibility
    /// Linux accessibility adapter
    #[cfg(feature = "accessibility")]
    pub accessibility_adapter: accessibility::LinuxAccessibilityAdapter,
}

#[derive(Debug, Clone, Copy)]
pub enum X11Event {
    Redraw,
    Close,
    Other,
}

impl PlatformWindow for X11Window {
    type EventType = X11Event;

    fn new(options: WindowCreateOptions) -> Result<Self, WindowError>
    where
        Self: Sized,
    {
        let resources = Arc::new(super::AppResources::default_for_testing());
        Self::new_with_resources(options, resources)
    }

    fn get_state(&self) -> WindowState {
        WindowState {
            title: self.current_window_state.title.clone(),
            size: self.current_window_state.size,
            position: self.current_window_state.position,
            flags: self.current_window_state.flags,
            theme: self.current_window_state.theme,
            debug_state: self.current_window_state.debug_state,
            keyboard_state: self.current_window_state.keyboard_state.clone(),
            mouse_state: self.current_window_state.mouse_state.clone(),
            touch_state: self.current_window_state.touch_state.clone(),
            ime_position: self.current_window_state.ime_position,
            platform_specific_options: self.current_window_state.platform_specific_options.clone(),
            renderer_options: self.current_window_state.renderer_options,
            background_color: self.current_window_state.background_color,
            layout_callback: self.current_window_state.layout_callback.clone(),
            close_callback: self.current_window_state.close_callback.clone(),
            monitor: Monitor::default(), /* Monitor info needs to be looked up from platform via
                                          * monitor_id */
        }
    }
    fn set_properties(&mut self, props: WindowProperties) -> Result<(), WindowError> {
        if let Some(title) = props.title {
            self.current_window_state.title = title.clone().into();
            let c_title = CString::new(title).unwrap();
            unsafe { (self.xlib.XStoreName)(self.display, self.window, c_title.as_ptr()) };
        }
        Ok(())
    }

    fn poll_event(&mut self) -> Option<Self::EventType> {
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
                    self.request_redraw();
                    ProcessEventResult::DoNothing
                }
                defines::ConfigureNotify => {
                    let ev = unsafe { &event.configure };
                    let (new_width, new_height) = (ev.width as u32, ev.height as u32);

                    // Check if size changed
                    let size_changed = self.current_window_state.size.get_physical_size()
                        != PhysicalSize::new(new_width, new_height);

                    // Check if position changed (might have moved to different monitor with
                    // different DPI)
                    let position_changed = match self.current_window_state.position {
                        azul_core::window::WindowPosition::Initialized(pos) => {
                            pos.x != ev.x || pos.y != ev.y
                        }
                        _ => true,
                    };

                    if size_changed {
                        self.current_window_state.size.dimensions =
                            LogicalSize::new(new_width as f32, new_height as f32);
                        self.regenerate_layout().ok();
                    }

                    // Update position
                    self.current_window_state.position =
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
                            let old_dpi = self.current_window_state.size.dpi;

                            // Only regenerate if DPI changed significantly (avoid rounding errors)
                            if (new_dpi as i32 - old_dpi as i32).abs() > 1 {
                                eprintln!(
                                    "[X11 DPI Change] {} -> {} (moved to different monitor)",
                                    old_dpi, new_dpi
                                );
                                self.current_window_state.size.dpi = new_dpi;
                                self.regenerate_layout().ok();
                            }
                        }
                    }

                    ProcessEventResult::DoNothing
                }
                defines::ClientMessage => {
                    if unsafe { event.client_message.data.l[0] } as Atom
                        == self.wm_delete_window_atom
                    {
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
                _ => ProcessEventResult::DoNothing,
            };

            // Request redraw if needed
            if result != ProcessEventResult::DoNothing {
                self.request_redraw();
            }
        }

        None
    }

    fn present(&mut self) -> Result<(), WindowError> {
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
                            self.current_window_state.size.dimensions.width as u32,
                            self.current_window_state.size.dimensions.height as u32,
                        );
                    }
                }
                unsafe { (self.xlib.XFlush)(self.display) };
                Ok(())
            }
        }
    }

    fn request_redraw(&mut self) {
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

    fn close(&mut self) {
        if self.is_open {
            self.is_open = false;
            unsafe {
                (self.xlib.XDestroyWindow)(self.display, self.window);
                (self.xlib.XCloseDisplay)(self.display);
            }
        }
    }

    fn get_render_context(&self) -> RenderContext {
        match &self.render_mode {
            RenderMode::Gpu(ctx, _) => RenderContext::OpenGL {
                context: ctx.egl_context as *mut _,
            },
            RenderMode::Cpu(_) => RenderContext::CPU,
        }
    }

    fn is_open(&self) -> bool {
        self.is_open
    }
}

impl X11Window {
    /// Create a new X11 window with shared resources
    ///
    /// This is the preferred way to create X11 windows, as it allows
    /// sharing font cache, app data, and system styling across windows.
    pub fn new_with_resources(
        options: WindowCreateOptions,
        resources: Arc<super::AppResources>,
    ) -> Result<Self, WindowError> {
        let xlib = Xlib::new()
            .map_err(|e| WindowError::PlatformError(format!("Failed to load libX11: {:?}", e)))?;
        let egl = Egl::new()
            .map_err(|e| WindowError::PlatformError(format!("Failed to load libEGL: {:?}", e)))?;
        let xkb = Xkb::new().map_err(|e| {
            WindowError::PlatformError(format!("Failed to load libxkbcommon: {:?}", e))
        })?;

        let display = unsafe { (xlib.XOpenDisplay)(std::ptr::null()) };
        if display.is_null() {
            return Err(WindowError::PlatformError(
                "Failed to open X display".into(),
            ));
        }

        let screen = unsafe { (xlib.XDefaultScreen)(display) };
        let root = unsafe { (xlib.XRootWindow)(display, screen) };

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

        let use_csd = options.state.flags.decorations == WindowDecorations::None;
        if use_csd {
            attributes.override_redirect = 1;
        }
        attributes.event_mask = event_mask;

        let size = options.state.size;
        let position = options.state.position;
        // Monitor ID is now stored in FullWindowState.monitor_id, not in WindowState
        // For now, we default to monitor 0
        let monitor_id = 0; // TODO: Get from options or detect primary monitor

        let window_handle = unsafe {
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
                gl_context.configure_vsync(options.state.renderer_options.vsync);
                let gl_functions = GlFunctions::initialize(&egl).unwrap();

                let new_frame_ready = Arc::new((Mutex::new(false), Condvar::new()));
                let (renderer, sender) = webrender::create_webrender_instance(
                    gl_functions.functions.clone(),
                    Box::new(Notifier {
                        new_frame_ready: new_frame_ready.clone(),
                    }),
                    wr_translate2::default_renderer_options(&options),
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
            Err(_) => {
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
            display,
            window: window_handle,
            is_open: true,
            wm_delete_window_atom,
            ime_manager,
            render_mode,
            layout_window: None,
            current_window_state: FullWindowState {
                title: options.state.title.clone(),
                size: options.state.size,
                position: options.state.position,
                flags: options.state.flags,
                theme: options.state.theme,
                debug_state: options.state.debug_state,
                keyboard_state: Default::default(),
                mouse_state: Default::default(),
                touch_state: Default::default(),
                ime_position: options.state.ime_position,
                platform_specific_options: options.state.platform_specific_options.clone(),
                renderer_options: options.state.renderer_options,
                background_color: options.state.background_color,
                layout_callback: options.state.layout_callback,
                close_callback: options.state.close_callback.clone(),
                monitor_id: None, // Monitor ID will be detected from platform
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
            new_frame_ready: Arc::new((Mutex::new(false), Condvar::new())),
            scrollbar_drag_state: None,
            last_hovered_node: None,
            frame_needs_regeneration: false,
            pending_window_creates: Vec::new(),
            #[cfg(feature = "gnome-menus")]
            gnome_menu: None, // Initialize as None, will be set up if enabled
            resources,
            #[cfg(feature = "accessibility")]
            accessibility_adapter: accessibility::LinuxAccessibilityAdapter::new(),
        };

        // Initialize accessibility adapter
        #[cfg(feature = "accessibility")]
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

        // Position window on requested monitor (or center on primary)
        // Convert u32 to MonitorId
        let monitor_id_typed = azul_core::window::MonitorId {
            index: monitor_id as usize,
            hash: 0,
        };
        window.position_window_on_monitor(monitor_id_typed, position, size);

        // Initialize GNOME native menus if enabled
        #[cfg(feature = "gnome-menus")]
        if options.state.flags.use_native_menus {
            let app_name = &options.state.title;
            match super::gnome_menu::GnomeMenuManager::new(app_name) {
                Some(menu_manager) => {
                    // Try to set window properties for GNOME Shell integration
                    match menu_manager.set_window_properties(window.window, display as *mut _) {
                        Ok(_) => {
                            super::gnome_menu::debug_log(&format!(
                                "GNOME menu integration enabled for window: {}",
                                app_name
                            ));
                            window.gnome_menu = Some(menu_manager);
                        }
                        Err(e) => {
                            super::gnome_menu::debug_log(&format!(
                                "Failed to set GNOME window properties: {} - falling back to CSD \
                                 menus",
                                e
                            ));
                            // Continue without GNOME menus - will use CSD fallback
                        }
                    }
                }
                None => {
                    super::gnome_menu::debug_log("GNOME menus not available - using CSD fallback");
                    // Continue without GNOME menus - will use CSD fallback
                }
            }
        }

        // Register window in global registry for multi-window support
        unsafe {
            super::registry::register_x11_window(window.window, &mut window as *mut _);
        }

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
            .find(|m| m.id.index == monitor_id.index)
            .or_else(|| {
                monitors
                    .as_slice()
                    .iter()
                    .find(|m| m.id.hash == monitor_id.hash && monitor_id.hash != 0)
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
        let result = self.process_window_events_recursive_v2(0);
        if result != ProcessEventResult::DoNothing {
            self.request_redraw();
        }
    }

    pub fn wait_for_events(&mut self) -> Result<(), WindowError> {
        unsafe { (self.xlib.XFlush)(self.display) };
        let mut event: XEvent = unsafe { std::mem::zeroed() };
        unsafe { (self.xlib.XNextEvent)(self.display, &mut event) };
        self.handle_event(&mut event);
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
            _ => ProcessEventResult::DoNothing,
        };

        // Request redraw if needed
        if result != ProcessEventResult::DoNothing {
            self.request_redraw();
        }
    }

    pub fn regenerate_layout(&mut self) -> Result<(), String> {
        let layout_window = self.layout_window.as_mut().ok_or("No layout window")?;

        // Call unified regenerate_layout from common module
        crate::desktop::shell2::common::layout_v2::regenerate_layout(
            layout_window,
            &self.resources.app_data,
            &self.current_window_state,
            &mut self.renderer_resources,
            self.render_api.as_mut().ok_or("No render API")?,
            &self.image_cache,
            &self.gl_context_ptr,
            &self.resources.fc_cache,
            &self.resources.system_style,
            self.document_id.ok_or("No document ID")?,
        )?;

        // Mark that frame needs regeneration (will be called once at event processing end)
        self.frame_needs_regeneration = true;

        Ok(())
    }

    /// Generate frame if needed and reset flag
    pub fn generate_frame_if_needed(&mut self) {
        if !self.frame_needs_regeneration {
            return;
        }

        if let (Some(ref mut layout_window), Some(ref mut render_api), Some(document_id)) = (
            self.layout_window.as_mut(),
            self.render_api.as_mut(),
            self.document_id,
        ) {
            crate::desktop::shell2::common::layout_v2::generate_frame(
                layout_window,
                render_api,
                document_id,
            );
        }

        self.frame_needs_regeneration = false;
    }

    /// Synchronize X11 window properties with current_window_state
    fn sync_window_state(&mut self) {
        use std::ffi::CString;

        // Get copies of previous and current state to avoid borrow checker issues
        let (previous, current) = match &self.previous_window_state {
            Some(prev) => (prev.clone(), self.current_window_state.clone()),
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
                WindowFrame::Normal | WindowFrame::Fullscreen => {
                    // Restore to normal - remove maximize state
                    if previous.flags.frame == WindowFrame::Maximized {
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
                    }
                }
            }
        }

        // Flush X11 commands
        unsafe {
            (self.xlib.XFlush)(self.display);
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

// ========================= PlatformWindowV2 Trait Implementation =========================

impl PlatformWindowV2 for X11Window {
    // =========================================================================
    // REQUIRED: Simple Getter Methods
    // =========================================================================

    fn get_layout_window_mut(&mut self) -> Option<&mut LayoutWindow> {
        self.layout_window.as_mut()
    }

    fn get_layout_window(&self) -> Option<&LayoutWindow> {
        self.layout_window.as_ref()
    }

    fn get_current_window_state(&self) -> &FullWindowState {
        &self.current_window_state
    }

    fn get_current_window_state_mut(&mut self) -> &mut FullWindowState {
        &mut self.current_window_state
    }

    fn get_previous_window_state(&self) -> &Option<FullWindowState> {
        &self.previous_window_state
    }

    fn set_previous_window_state(&mut self, state: FullWindowState) {
        self.previous_window_state = Some(state);
    }

    fn get_image_cache_mut(&mut self) -> &mut ImageCache {
        &mut self.image_cache
    }

    fn get_renderer_resources_mut(&mut self) -> &mut RendererResources {
        &mut self.renderer_resources
    }

    fn get_fc_cache(&self) -> &Arc<FcFontCache> {
        &self.resources.fc_cache
    }

    fn get_gl_context_ptr(&self) -> &OptionGlContextPtr {
        &self.gl_context_ptr
    }

    fn get_system_style(&self) -> &Arc<azul_css::system::SystemStyle> {
        &self.resources.system_style
    }

    fn get_app_data(&self) -> &Arc<RefCell<RefAny>> {
        &self.resources.app_data
    }

    fn get_scrollbar_drag_state(&self) -> Option<&ScrollbarDragState> {
        self.scrollbar_drag_state.as_ref()
    }

    fn get_scrollbar_drag_state_mut(&mut self) -> &mut Option<ScrollbarDragState> {
        &mut self.scrollbar_drag_state
    }

    fn set_scrollbar_drag_state(&mut self, state: Option<ScrollbarDragState>) {
        self.scrollbar_drag_state = state;
    }

    fn get_hit_tester(&self) -> &AsyncHitTester {
        self.hit_tester
            .as_ref()
            .expect("Hit tester must be initialized")
    }

    fn get_hit_tester_mut(&mut self) -> &mut AsyncHitTester {
        self.hit_tester
            .as_mut()
            .expect("Hit tester must be initialized")
    }

    fn get_last_hovered_node(&self) -> Option<&event_v2::HitTestNode> {
        self.last_hovered_node.as_ref()
    }

    fn set_last_hovered_node(&mut self, node: Option<event_v2::HitTestNode>) {
        self.last_hovered_node = node;
    }

    fn get_document_id(&self) -> DocumentId {
        self.document_id.expect("Document ID must be initialized")
    }

    fn get_id_namespace(&self) -> IdNamespace {
        self.id_namespace.expect("ID namespace must be initialized")
    }

    fn get_render_api(&self) -> &webrender::RenderApi {
        self.render_api
            .as_ref()
            .expect("Render API must be initialized")
    }

    fn get_render_api_mut(&mut self) -> &mut webrender::RenderApi {
        self.render_api
            .as_mut()
            .expect("Render API must be initialized")
    }

    fn get_renderer(&self) -> Option<&webrender::Renderer> {
        self.renderer.as_ref()
    }

    fn get_renderer_mut(&mut self) -> Option<&mut webrender::Renderer> {
        self.renderer.as_mut()
    }

    fn get_raw_window_handle(&self) -> RawWindowHandle {
        RawWindowHandle::Xlib(XlibHandle {
            window: self.window,
            display: self.display as *mut c_void,
        })
    }

    fn needs_frame_regeneration(&self) -> bool {
        self.frame_needs_regeneration
    }

    fn mark_frame_needs_regeneration(&mut self) {
        self.frame_needs_regeneration = true;
    }

    fn clear_frame_regeneration_flag(&mut self) {
        self.frame_needs_regeneration = false;
    }

    fn prepare_callback_invocation(&mut self) -> event_v2::InvokeSingleCallbackBorrows {
        let layout_window = self
            .layout_window
            .as_mut()
            .expect("Layout window must exist for callback invocation");

        event_v2::InvokeSingleCallbackBorrows {
            layout_window,
            window_handle: RawWindowHandle::Xlib(XlibHandle {
                window: self.window,
                display: self.display as *mut c_void,
            }),
            gl_context_ptr: &self.gl_context_ptr,
            image_cache: &mut self.image_cache,
            fc_cache_clone: (*self.resources.fc_cache).clone(),
            system_style: self.resources.system_style.clone(),
            previous_window_state: &self.previous_window_state,
            current_window_state: &self.current_window_state,
            renderer_resources: &mut self.renderer_resources,
        }
    }

    // =========================================================================
    // Timer Management (X11 Implementation - Stored in LayoutWindow)
    // =========================================================================

    fn start_timer(&mut self, timer_id: usize, timer: azul_layout::timer::Timer) {
        // X11 has no native timer API, so we just store timers in layout_window
        // They will be ticked manually in the event loop
        if let Some(layout_window) = self.layout_window.as_mut() {
            layout_window
                .timers
                .insert(azul_core::task::TimerId { id: timer_id }, timer);
        }

        // Mark for regeneration so the event loop checks timers
        self.frame_needs_regeneration = true;
    }

    fn stop_timer(&mut self, timer_id: usize) {
        if let Some(layout_window) = self.layout_window.as_mut() {
            layout_window
                .timers
                .remove(&azul_core::task::TimerId { id: timer_id });
        }
    }

    // =========================================================================
    // Thread Management (X11 Implementation - Stored in LayoutWindow)
    // =========================================================================

    fn start_thread_poll_timer(&mut self) {
        // For X11, we don't need a separate timer - threads are checked
        // in the event loop when layout_window.threads is non-empty
        // Just mark for regeneration to start checking
        self.frame_needs_regeneration = true;
    }

    fn stop_thread_poll_timer(&mut self) {
        // No-op for X11 - thread checking stops automatically when
        // layout_window.threads becomes empty
    }

    fn add_threads(
        &mut self,
        threads: std::collections::BTreeMap<azul_core::task::ThreadId, azul_layout::thread::Thread>,
    ) {
        if let Some(layout_window) = self.layout_window.as_mut() {
            for (thread_id, thread) in threads {
                layout_window.threads.insert(thread_id, thread);
            }
        }

        // Mark for regeneration to start thread polling
        self.frame_needs_regeneration = true;
    }

    fn remove_threads(
        &mut self,
        thread_ids: &std::collections::BTreeSet<azul_core::task::ThreadId>,
    ) {
        if let Some(layout_window) = self.layout_window.as_mut() {
            for thread_id in thread_ids {
                layout_window.threads.remove(thread_id);
            }
        }
    }
}

impl Drop for X11Window {
    fn drop(&mut self) {
        // Unregister from global registry before closing
        super::registry::unregister_x11_window(self.window);
        self.close();
    }
}

impl X11Window {
    /// Check timers and threads, trigger callbacks if needed
    /// This is called on every poll_event() to simulate timer ticks
    fn check_timers_and_threads(&mut self) {
        if let Some(layout_window) = self.layout_window.as_mut() {
            let system_callbacks = azul_layout::callbacks::ExternalSystemCallbacks::rust_internal();
            let current_time = (system_callbacks.get_system_time_fn.cb)();

            // Check if any timers expired
            let expired_timers = layout_window.tick_timers(current_time);
            if !expired_timers.is_empty() {
                self.frame_needs_regeneration = true;
            }

            // Check if we have active threads (they need periodic checking)
            if !layout_window.threads.is_empty() {
                self.frame_needs_regeneration = true;
            }
        }
    }
}
