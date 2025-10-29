//! X11 implementation for Linux using the shell2 architecture.

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
        HwAcceleration, KeyboardState, MouseCursorType, MouseState, RawWindowHandle, RendererType,
        WindowDecorations,
    },
};
use azul_layout::{
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
    shell2::common::{PlatformWindow, RenderContext, WindowError, WindowProperties},
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
    pub frame_needs_regeneration: bool,

    // Shared resources
    pub resources: Arc<super::AppResources>,
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
        // This is a stand-in for tests - use new_with_resources() in production
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
            monitor: self.current_window_state.monitor.clone(),
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
                    if self.current_window_state.size.get_physical_size()
                        != PhysicalSize::new(new_width, new_height)
                    {
                        self.current_window_state.size.dimensions =
                            LogicalSize::new(new_width as f32, new_height as f32);
                        self.regenerate_layout().ok();
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
                monitor: options.state.monitor,
                hovered_file: None,
                dropped_file: None,
                focused_node: None,
                last_hit_test: azul_layout::hit_test::FullHitTest::empty(None),
                selections: Default::default(),
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
            frame_needs_regeneration: false,
            resources,
        };

        unsafe { (window.xlib.XMapWindow)(display, window.window) };
        unsafe { (window.xlib.XFlush)(display) };

        // Register window in global registry for multi-window support
        unsafe {
            super::registry::register_x11_window(window.window, &mut window as *mut _);
        }

        Ok(window)
    }

    fn process_events(&mut self) {
        let result = self.process_window_events_v2();
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
        use std::io::Write;

        use azul_core::callbacks::LayoutCallback;

        eprintln!("[regenerate_layout] START");

        let layout_window = self.layout_window.as_mut().ok_or("No layout window")?;

        // Borrow app_data from Arc<RefCell<>>
        let mut app_data_borrowed = self.resources.app_data.borrow_mut();

        // Update layout_window's fc_cache with the shared one from resources
        layout_window.font_manager.fc_cache = self.resources.fc_cache.clone();

        // 1. Call layout_callback to get styled_dom
        // Create LayoutCallbackInfo with Arc<SystemStyle>
        let mut callback_info = azul_core::callbacks::LayoutCallbackInfo::new(
            self.current_window_state.size.clone(),
            self.current_window_state.theme,
            &self.image_cache,
            &self.gl_context_ptr,
            &*self.resources.fc_cache,
            self.resources.system_style.clone(),
        );

        eprintln!("[regenerate_layout] Calling layout_callback");
        let _ = std::io::stderr().flush();

        let user_styled_dom = match &self.current_window_state.layout_callback {
            LayoutCallback::Raw(inner) => (inner.cb)(&mut *app_data_borrowed, &mut callback_info),
            LayoutCallback::Marshaled(marshaled) => (marshaled.cb.cb)(
                &mut marshaled.marshal_data.clone(),
                &mut *app_data_borrowed,
                &mut callback_info,
            ),
        };

        // Inject CSD decorations if needed
        let styled_dom = if crate::desktop::csd::should_inject_csd(
            self.current_window_state.flags.has_decorations,
            self.current_window_state.flags.decorations,
        ) {
            eprintln!("[regenerate_layout] Injecting CSD decorations");
            crate::desktop::csd::wrap_user_dom_with_decorations(
                user_styled_dom,
                &self.current_window_state.title,
                true,                         // inject titlebar
                true,                         // has minimize
                true,                         // has maximize
                &self.resources.system_style, // pass SystemStyle for native look
            )
        } else {
            user_styled_dom
        };

        eprintln!(
            "[regenerate_layout] StyledDom received: {} nodes",
            styled_dom.styled_nodes.len()
        );
        eprintln!(
            "[regenerate_layout] StyledDom hierarchy length: {}",
            styled_dom.node_hierarchy.len()
        );
        let _ = std::io::stderr().flush();

        // 2. Perform layout with solver3
        eprintln!("[regenerate_layout] Calling layout_and_generate_display_list");
        layout_window
            .layout_and_generate_display_list(
                styled_dom,
                &self.current_window_state,
                &self.renderer_resources,
                &azul_layout::callbacks::ExternalSystemCallbacks::rust_internal(),
                &mut None, // No debug messages for now
            )
            .map_err(|e| format!("Layout error: {:?}", e))?;

        eprintln!(
            "[regenerate_layout] Layout completed, {} DOMs",
            layout_window.layout_results.len()
        );

        // 3. Calculate scrollbar states based on new layout
        // This updates scrollbar geometry (thumb position/size ratios, visibility)
        layout_window.scroll_states.calculate_scrollbar_states();

        // 4. Rebuild display list and send to WebRender
        let dpi_factor = self.current_window_state.size.get_hidpi_factor();
        let mut txn = webrender::Transaction::new();
        let render_api = self.render_api.as_mut().ok_or("No render API")?;
        crate::desktop::wr_translate2::rebuild_display_list(
            &mut txn,
            layout_window,
            render_api,
            &self.image_cache,
            Vec::new(),
            &mut self.renderer_resources,
            dpi_factor,
        );

        // 5. Synchronize scrollbar opacity with GPU cache AFTER display list submission
        // This enables smooth fade-in/fade-out without display list rebuild
        let system_callbacks = azul_layout::callbacks::ExternalSystemCallbacks::rust_internal();
        for (dom_id, layout_result) in &layout_window.layout_results {
            azul_layout::LayoutWindow::synchronize_scrollbar_opacity(
                &mut layout_window.gpu_state_manager,
                &layout_window.scroll_states,
                *dom_id,
                &layout_result.layout_tree,
                &system_callbacks,
                azul_core::task::Duration::System(azul_core::task::SystemTimeDiff::from_millis(
                    500,
                )), // fade_delay
                azul_core::task::Duration::System(azul_core::task::SystemTimeDiff::from_millis(
                    200,
                )), // fade_duration
            );
        }

        // 6. Mark that frame needs regeneration (will be called once at event processing end)
        self.frame_needs_regeneration = true;

        Ok(())
    }

    /// Generate frame if needed and reset flag
    pub fn generate_frame_if_needed(&mut self) {
        if !self.frame_needs_regeneration {
            return;
        }

        if let (Some(ref mut layout_window), Some(ref mut render_api)) =
            (self.layout_window.as_mut(), self.render_api.as_mut())
        {
            let mut txn = webrender::Transaction::new();
            crate::desktop::wr_translate2::generate_frame(
                &mut txn,
                layout_window,
                render_api,
                true, // Display list was rebuilt
            );
            render_api.send_transaction(
                crate::desktop::wr_translate2::wr_translate_document_id(self.document_id.unwrap()),
                txn,
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
                WindowFrame::Minimized => {
                    // TODO: Minimize via XIconifyWindow (not yet in dlopen wrapper)
                    // For now, use XUnmapWindow as fallback
                    unsafe {
                        (self.xlib.XUnmapWindow)(self.display, self.window);
                    }
                }
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

        // Mouse cursor synchronization - compute from current hit test
        if let Some(layout_window) = self.layout_window.as_ref() {
            let cursor_test = layout_window.compute_cursor_type_hit_test(&current.last_hit_test);
            self.set_cursor(cursor_test.cursor_icon);
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
        // TODO: Implement proper DPI detection with XRandR
        // For now, return default DPI
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
            })
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
