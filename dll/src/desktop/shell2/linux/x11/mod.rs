//! X11 implementation for Linux using the shell2 architecture.

mod decorations;
pub(crate) mod defines; // Make defines public within crate for Wayland to access
pub(crate) mod dlopen; // Make dlopen public within crate for Wayland to access
pub(crate) mod events; // Make events public within crate for Wayland to access
pub(crate) mod gl; // Make gl public within crate for Wayland to access
mod menu;

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
    window_state::{WindowCreateOptions, WindowState},
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
    pub current_window_state: WindowState,
    pub previous_window_state: Option<WindowState>,
    pub render_api: Option<webrender::RenderApi>,
    pub renderer: Option<WrRenderer>,
    pub hit_tester: Option<AsyncHitTester>,
    pub document_id: Option<DocumentId>,
    pub image_cache: ImageCache,
    pub renderer_resources: RendererResources,
    gl_context_ptr: OptionGlContextPtr,
    new_frame_ready: Arc<(Mutex<bool>, Condvar)>,
    id_namespace: Option<IdNamespace>,

    // X11 specific state
    menu_manager: Option<menu::MenuManager>,

    // Shared resources
    fc_cache: Arc<FcFontCache>,
    app_data: Arc<RefCell<RefAny>>,
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
        // This is a stand-in since the real `App` isn't passed down yet.
        let fc_cache = Arc::new(FcFontCache::build());
        let app_data = Arc::new(RefCell::new(RefAny::new(())));
        Self::new(options, fc_cache, app_data)
    }

    fn get_state(&self) -> WindowState {
        self.current_window_state.clone().into()
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

            if decorations::handle_decoration_event(self, &event) {
                self.process_events(); // CSD interactions might require UI updates
                continue;
            }

            self.previous_window_state = Some(self.current_window_state.clone());

            match unsafe { event.type_ } {
                defines::Expose => self.request_redraw(),
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
                }
                defines::ClientMessage => {
                    if unsafe { event.client_message.data.l[0] } as Atom
                        == self.wm_delete_window_atom
                    {
                        self.is_open = false;
                        return Some(X11Event::Close);
                    }
                }
                defines::ButtonPress | defines::ButtonRelease => {
                    events::handle_mouse_button(self, unsafe { &event.button })
                }
                defines::MotionNotify => events::handle_mouse_move(self, unsafe { &event.motion }),
                defines::KeyPress | defines::KeyRelease => {
                    events::handle_keyboard(self, unsafe { &mut event.key })
                }
                defines::EnterNotify | defines::LeaveNotify => {
                    events::handle_mouse_crossing(self, unsafe { &event.crossing })
                }
                _ => {}
            }

            self.process_events();
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
    pub fn new(
        options: WindowCreateOptions,
        fc_cache: Arc<FcFontCache>,
        app_data: Arc<RefCell<RefAny>>,
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
            current_window_state: options.state.into(),
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
            menu_manager: None,
            fc_cache,
            app_data,
        };

        unsafe { (window.xlib.XMapWindow)(display, window.window) };
        unsafe { (window.xlib.XFlush)(display) };

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
        self.previous_window_state = Some(self.current_window_state.clone());
        match unsafe { event.type_ } {
            defines::Expose => self.request_redraw(),
            defines::KeyPress | defines::KeyRelease => {
                events::handle_keyboard(self, unsafe { &mut event.key })
            }
            // ... other event handlers ...
            _ => {}
        }
        self.process_events();
    }

    pub fn regenerate_layout(&mut self) -> Result<(), String> {
        // Stub for now. A full implementation would call
        // LayoutWindow::layout_and_generate_display_list
        Ok(())
    }

    pub(crate) fn process_window_events_v2(&mut self) -> ProcessEventResult {
        // Stub for now. A full implementation would use state-diffing.
        ProcessEventResult::DoNothing
    }
}

impl Drop for X11Window {
    fn drop(&mut self) {
        self.close();
    }
}
