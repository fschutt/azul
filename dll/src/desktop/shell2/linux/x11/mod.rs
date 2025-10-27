//! X11 implementation for Linux using the shell2 architecture.

use super::common::{PlatformWindow, RenderContext, WindowError, WindowProperties};
use crate::desktop::wr_translate2::{self, AsyncHitTester, Notifier};
use azul_core::{
    callbacks::LayoutCallbackInfo,
    dom::DomId,
    gl::{GlContextPtr, OptionGlContextPtr},
    hit_test::DocumentId,
    resources::{AppConfig, DpiScaleFactor, IdNamespace, ImageCache, RendererResources},
    window::{
        FullWindowState, HwAcceleration, KeyboardState, MouseButton, MouseCursorType,
        MouseState, RawWindowHandle, RendererType, WindowDecorations, X11Handle,
    },
};
use azul_layout::{
    window::LayoutWindow,
    window_state::{WindowCreateOptions, WindowState},
};
use std::{
    ffi::CStr,
    rc::Rc,
    sync::{Arc, Condvar, Mutex},
};

use rust_fontconfig::FcFontCache;
use webrender::Renderer as WrRenderer;

use self::defines::*;
use self::dlopen::{Egl, Xlib};
mod decorations;
mod defines;
mod dlopen;
mod events;
mod gl;
mod menu;

/// Tracks the current rendering mode of the window.
enum RenderMode {
    Gpu(gl::GlContext, gl::GlFunctions),
    Cpu(Option<GC>), // Option to hold the Graphics Context
}

pub struct X11Window {
    pub xlib: Rc<Xlib>,
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
    new_frame_ready: Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
    id_namespace: Option<IdNamespace>,

    // X11 specific state
    decorations: decorations::Decorations,
    menu_manager: Option<menu::MenuManager>,
}

#[derive(Debug, Clone)]
pub enum X11Event {
    Redraw,
    Close,
    Other,
}

impl PlatformWindow for X11Window {
    type EventType = X11Event;

    fn new(options: WindowCreateOptions) -> Result<Self, WindowError> {
        let xlib = Xlib::new()
            .map_err(|e| WindowError::PlatformError(format!("Failed to load libX11: {:?}", e)))?;
        let egl =
            Egl::new().map_err(|e| WindowError::PlatformError(format!("Failed to load libEGL: {:?}", e)))?;

        let display = unsafe { (xlib.XOpenDisplay)(std::ptr::null()) };
        if display.is_null() {
            return Err(WindowError::PlatformError("Failed to open X display".into()));
        }

        let screen = unsafe { (xlib.XDefaultScreen)(display) };
        let root = unsafe { (xlib.XRootWindow)(display, screen) };

        let mut attributes: XSetWindowAttributes = unsafe { std::mem::zeroed() };
        let event_mask = ExposureMask | KeyPressMask | KeyReleaseMask | ButtonPressMask | ButtonReleaseMask | PointerMotionMask | StructureNotifyMask | EnterWindowMask | LeaveWindowMask;

        let use_csd = options.state.flags.decorations == WindowDecorations::None;
        if use_csd { attributes.override_redirect = 1; }
        attributes.event_mask = event_mask;
        
        let size = options.state.size;
        let window_handle = unsafe {
            (xlib.XCreateWindow)(
                display, root, 0, 0,
                size.dimensions.width as u32, size.dimensions.height as u32, 0,
                CopyFromParent, InputOutput as u32, std::ptr::null_mut(),
                CWEventMask | if use_csd { CWOverrideRedirect } else { 0 },
                &mut attributes,
            )
        };

        let ime_manager = events::ImeManager::new(&xlib, display, window_handle);
        let wm_delete_window_atom = unsafe { (xlib.XInternAtom)(display, b"WM_DELETE_WINDOW\0".as_ptr() as _, 0) };
        unsafe { (xlib.XSetWMProtocols)(display, window_handle, &mut [wm_delete_window_atom].as_mut_ptr(), 1) };

        // -- Begin GL/CPU Fallback Initialization --
        
        let (render_mode, renderer, render_api, hit_tester, document_id, id_namespace, gl_context_ptr) = 
            match gl::GlContext::new(&xlib, &egl, display, window_handle) {
                Ok(gl_context) => {
                    // GPU success path
                    eprintln!("[X11] OpenGL context created successfully.");
                    gl_context.make_current();
                    let mut gl_functions = gl::GlFunctions::initialize(&egl).unwrap();
                    gl_functions.load(&egl);

                    let new_frame_ready = Arc::new((Mutex::new(false), Condvar::new()));
                    let (renderer, sender) = webrender::create_webrender_instance(
                        gl_functions.functions.clone(),
                        Box::new(Notifier { new_frame_ready: new_frame_ready.clone() }),
                        wr_translate2::default_renderer_options(&options),
                        None,
                    ).map_err(|e| WindowError::PlatformError(format!("WebRender init failed: {:?}", e)))?;
                    
                    let render_api = sender.create_api();
                    let framebuffer_size = webrender::api::units::DeviceIntSize::new(size.dimensions.width as i32, size.dimensions.height as i32);
                    let wr_doc_id = render_api.add_document(framebuffer_size);
                    let document_id = wr_translate2::translate_document_id_wr(wr_doc_id);
                    let id_namespace = wr_translate2::translate_id_namespace_wr(render_api.get_namespace_id());
                    let hit_tester_request = render_api.request_hit_tester(wr_doc_id);

                    let gl_context_ptr = OptionGlContextPtr::Some(GlContextPtr::new(RendererType::Hardware, gl_functions.functions.clone()));

                    (RenderMode::Gpu(gl_context, gl_functions), Some(renderer), Some(render_api), Some(AsyncHitTester::Requested(hit_tester_request)), Some(document_id), Some(id_namespace), gl_context_ptr)
                },
                Err(e) => {
                    // CPU fallback path
                    eprintln!("[X11] Failed to create OpenGL context: {:?}. Falling back to CPU rendering.", e);
                    let gc = unsafe { (xlib.XCreateGC)(display, window_handle, 0, std::ptr::null_mut()) };
                    (RenderMode::Cpu(Some(gc)), None, None, None, None, None, None.into())
                }
            };
        
        // -- End GL/CPU Fallback Initialization --

        let mut window = Self {
            xlib, display, window: window_handle,
            is_open: true, wm_delete_window_atom, ime_manager,
            render_mode,
            layout_window: None, // Will be initialized later if in GPU mode
            current_window_state: options.state.into(),
            previous_window_state: None,
            renderer, render_api, hit_tester, document_id, id_namespace,
            image_cache: ImageCache::default(), renderer_resources: RendererResources::default(),
            gl_context_ptr, new_frame_ready: Arc::new((Mutex::new(false), Condvar::new())) ,
            decorations: decorations::Decorations::default(), menu_manager: None,
        };

        if std::env::var("XDG_CURRENT_DESKTOP").map(|s| s.contains("GNOME")).unwrap_or(false) {
            let mut menu_manager = menu::MenuManager::new("my_app").unwrap();
            menu_manager.set_x11_properties(display, window.window, &window.xlib);
            window.menu_manager = Some(menu_manager);
        }

        unsafe { (window.xlib.XMapWindow)(display, window.window) };
        unsafe { (window.xlib.XFlush)(display) };

        Ok(window)
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
                continue;
            }

            self.previous_window_state = Some(self.current_window_state.clone());

            match unsafe { event.type_ } {
                Expose => self.request_redraw(),
                ConfigureNotify => {
                    let ev = unsafe { event.configure };
                    self.current_window_state.size.dimensions.width = ev.width as f32;
                    self.current_window_state.size.dimensions.height = ev.height as f32;
                    self.regenerate_layout().ok();
                }
                ClientMessage => {
                    if unsafe { event.client_message.data.as_longs()[0] } as Atom == self.wm_delete_window_atom {
                        self.is_open = false;
                        return Some(X11Event::Close);
                    }
                }
                ButtonPress | ButtonRelease => events::handle_mouse_button(self, unsafe { &event.button }),
                MotionNotify => events::handle_mouse_move(self, unsafe { &event.motion }),
                KeyPress | KeyRelease => events::handle_keyboard(self, unsafe { &mut event.key }),
                EnterNotify | LeaveNotify => events::handle_mouse_crossing(self, unsafe { &event.crossing }),
                _ => {}
            }

            let processing_result = self.process_window_events_v2();
            if !matches!(processing_result, azul_core::events::ProcessEventResult::DoNothing) {
                self.request_redraw();
            }
        }
        None
    }

    fn present(&mut self) -> Result<(), WindowError> {
        match &self.render_mode {
            RenderMode::Gpu(gl_context, _) => gl_context.swap_buffers(),
            RenderMode::Cpu(_) => {
                // For CPU, we draw directly, so just flush
                unsafe { (self.xlib.XFlush)(self.display) };
                Ok(())
            }
        }
    }

    fn request_redraw(&mut self) {
        match &self.render_mode {
            RenderMode::Gpu(..) => {
                 let mut event: XEvent = unsafe { std::mem::zeroed() };
                 let mut expose = unsafe { &mut event.expose };
                 expose.type_ = Expose;
                 expose.display = self.display;
                 expose.window = self.window;
                 expose.count = 0;
                 unsafe {
                     (self.xlib.XSendEvent)(self.display, self.window, 0, ExposureMask, &mut event);
                     (self.xlib.XFlush)(self.display);
                 }
            },
            RenderMode::Cpu(gc) => {
                if let Some(gc) = gc {
                    unsafe {
                        let blue_color = 0x0000FF;
                        (self.xlib.XSetForeground)(self.display, *gc, blue_color);
                        (self.xlib.XFillRectangle)(self.display, self.window, *gc, 0, 0, self.current_window_state.size.dimensions.width as u32, self.current_window_state.size.dimensions.height as u32);
                        (self.xlib.XFlush)(self.display);
                    }
                }
            }
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

    fn get_state(&self) -> WindowState { self.current_window_state.clone().into() }
    fn set_properties(&mut self, _props: WindowProperties) -> Result<(), WindowError> { Ok(()) }
    fn get_render_context(&self) -> RenderContext { 
        match &self.render_mode {
            RenderMode::Gpu(ctx, _) => RenderContext::OpenGL { context: ctx.egl_context as *mut _ },
            RenderMode::Cpu(_) => RenderContext::CPU,
        }
    }
    fn is_open(&self) -> bool { self.is_open }
}

impl X11Window {
    // V2 Cross-Platform Event Processing
    pub(crate) fn process_window_events_v2(&mut self) -> azul_core::events::ProcessEventResult {
        use azul_core::events::{dispatch_events, ProcessEventResult};
        use azul_layout::window_state::create_events_from_states;

        let previous_state = self.previous_window_state.as_ref().unwrap_or(&self.current_window_state);
        let events = create_events_from_states(&self.current_window_state, previous_state);

        if events.is_empty() { return ProcessEventResult::DoNothing; }

        let hit_test = if !self.current_window_state.last_hit_test.is_empty() {
            Some(&self.current_window_state.last_hit_test)
        } else {
            None
        };

        let dispatch_result = dispatch_events(&events, hit_test);
        if dispatch_result.is_empty() { return ProcessEventResult::DoNothing; }

        ProcessEventResult::ShouldReRenderCurrentWindow
    }

    pub fn regenerate_layout(&mut self) -> Result<(), String> {
        if let Some(layout_window) = self.layout_window.as_mut() {
            layout_window.current_window_state = self.current_window_state.clone();
        }
        Ok(())
    }
}

impl Drop for X11Window {
    fn drop(&mut self) { self.close(); }
}