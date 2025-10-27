//! Wayland implementation for Linux.
//!
//! This module implements the PlatformWindow trait for Wayland.
//! It supports GPU-accelerated rendering via EGL and WebRender, with a
//! fallback to a CPU-rendered surface if GL context creation fails.
//!
//! Note: Uses dynamic loading (dlopen) to avoid linker errors
//! and ensure compatibility across Linux distributions.

mod defines;
mod dlopen;
mod events;
mod gl;

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
    hit_test::DocumentId,
    refany::RefAny,
    resources::{AppConfig, DpiScaleFactor, IdNamespace, ImageCache, RendererResources},
    window::{
        HwAcceleration, KeyboardState, MouseCursorType, MouseState, RawWindowHandle, RendererType,
        WaylandHandle, WindowDecorations,
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
    dlopen::{Library, Wayland, Xkb},
};
use super::common::gl::GlFunctions;
use crate::desktop::{
    shell2::common::{PlatformWindow, RenderContext, WindowError, WindowProperties},
    wr_translate2::{self, AsyncHitTester, Notifier},
};

/// Tracks the current rendering mode of the window.
enum RenderMode {
    Gpu(gl::GlContext, GlFunctions),
    Cpu(CpuFallbackState),
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
}

pub struct WaylandWindow {
    wayland: Rc<Wayland>,
    xkb: Rc<Xkb>,
    display: *mut defines::wl_display,
    registry: *mut defines::wl_registry,
    compositor: *mut defines::wl_compositor,
    shm: *mut defines::wl_shm,
    seat: *mut defines::wl_seat,
    xdg_wm_base: *mut defines::xdg_wm_base,
    surface: *mut defines::wl_surface,
    xdg_surface: *mut defines::xdg_surface,
    xdg_toplevel: *mut defines::xdg_toplevel,
    event_queue: *mut defines::wl_event_queue,
    keyboard_state: events::KeyboardState,
    pointer_state: events::PointerState,
    is_open: bool,
    configured: bool,

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

    render_mode: RenderMode,

    // Shared resources
    fc_cache: Arc<FcFontCache>,
    app_data: Arc<RefCell<RefAny>>,
}

#[derive(Debug, Clone, Copy)]
pub enum WaylandEvent {
    Redraw,
    Close,
    Other,
}

impl PlatformWindow for WaylandWindow {
    type EventType = WaylandEvent;

    fn new(options: WindowCreateOptions) -> Result<Self, WindowError>
    where
        Self: Sized,
    {
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
            unsafe { (self.wayland.xdg_toplevel_set_title)(self.xdg_toplevel, c_title.as_ptr()) };
        }
        Ok(())
    }

    fn poll_event(&mut self) -> Option<Self::EventType> {
        if unsafe {
            (self.wayland.wl_display_dispatch_queue_pending)(self.display, self.event_queue)
        } > 0
        {
            Some(WaylandEvent::Redraw) // Events were processed, a redraw might be needed.
        } else {
            None
        }
    }

    fn get_render_context(&self) -> RenderContext {
        match &self.render_mode {
            RenderMode::Gpu(ctx, _) => ctx
                .egl_context
                .map(|c| RenderContext::OpenGL {
                    context: c as *mut _,
                })
                .unwrap_or(RenderContext::CPU),
            RenderMode::Cpu(_) => RenderContext::CPU,
        }
    }

    fn present(&mut self) -> Result<(), WindowError> {
        match &self.render_mode {
            RenderMode::Gpu(gl_context, _) => gl_context.swap_buffers(),
            RenderMode::Cpu(cpu_state) => {
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
        }
    }

    fn is_open(&self) -> bool {
        self.is_open
    }
    fn close(&mut self) {
        self.is_open = false;
    }
    fn request_redraw(&mut self) {
        if self.configured {
            self.present().ok();
        }
    }
}

impl WaylandWindow {
    pub fn new(
        options: WindowCreateOptions,
        fc_cache: Arc<FcFontCache>,
        app_data: Arc<RefCell<RefAny>>,
    ) -> Result<Self, WindowError> {
        let wayland = Wayland::new().map_err(|e| {
            WindowError::PlatformError(format!("Failed to load libwayland-client: {:?}", e))
        })?;
        let xkb = Xkb::new().map_err(|e| {
            WindowError::PlatformError(format!("Failed to load libxkbcommon: {:?}", e))
        })?;

        let display = unsafe { (wayland.wl_display_connect)(std::ptr::null()) };
        if display.is_null() {
            return Err(WindowError::PlatformError(
                "Failed to connect to Wayland display".into(),
            ));
        }

        let event_queue = unsafe { (wayland.wl_display_create_event_queue)(display) };
        let registry = unsafe { (wayland.wl_display_get_registry)(display) };
        unsafe { (wayland.wl_proxy_set_queue)(registry as _, event_queue) };

        let mut window = Self {
            wayland: wayland.clone(),
            xkb,
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
            current_window_state: options.state.clone(),
            previous_window_state: None,
            layout_window: None,
            render_api: None,
            renderer: None,
            hit_tester: None,
            document_id: None,
            image_cache: ImageCache::default(),
            renderer_resources: RendererResources::default(),
            gl_context_ptr: None.into(),
            new_frame_ready: Arc::new((Mutex::new(false), Condvar::new())),
            id_namespace: None,
            keyboard_state: events::KeyboardState::new(),
            pointer_state: events::PointerState::new(),
            render_mode: RenderMode::Cpu(CpuFallbackState::new(
                &wayland,
                std::ptr::null_mut(),
                0,
                0,
            )?), // Placeholder
            fc_cache,
            app_data,
        };

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
        let title = CString::new(options.state.title.as_str()).unwrap();
        unsafe { (window.wayland.xdg_toplevel_set_title)(window.xdg_toplevel, title.as_ptr()) };

        let width = options.state.size.dimensions.width as i32;
        let height = options.state.size.dimensions.height as i32;

        let render_mode = match gl::GlContext::new(&wayland, display, window.surface, width, height)
        {
            Ok(mut gl_context) => {
                let gl_functions =
                    GlFunctions::initialize(gl_context.egl.as_ref().unwrap()).unwrap();
                RenderMode::Gpu(gl_context, gl_functions)
            }
            Err(e) => {
                eprintln!(
                    "[Wayland] GPU context failed: {:?}. Falling back to CPU.",
                    e
                );
                RenderMode::Cpu(CpuFallbackState::new(&wayland, window.shm, width, height)?)
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

        Ok(window)
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
            wr_translate2::default_renderer_options(options),
            None,
        )
        .map_err(|e| WindowError::PlatformError(format!("WebRender init failed: {:?}", e)))?;

        self.renderer = Some(renderer);
        self.render_api = Some(sender.create_api());
        let render_api = self.render_api.as_mut().unwrap();

        let framebuffer_size = webrender::api::units::DeviceIntSize::new(
            self.current_window_state.size.dimensions.width as i32,
            self.current_window_state.size.dimensions.height as i32,
        );
        let wr_doc_id = render_api.add_document(framebuffer_size);
        self.document_id = Some(wr_translate2::translate_document_id_wr(wr_doc_id));
        self.id_namespace = Some(wr_translate2::translate_id_namespace_wr(
            render_api.get_namespace_id(),
        ));
        let hit_tester_request = render_api.request_hit_tester(wr_doc_id);
        self.hit_tester = Some(AsyncHitTester::Requested(hit_tester_request));
        self.gl_context_ptr = OptionGlContextPtr::Some(GlContextPtr::new(
            RendererType::Hardware,
            gl_functions.functions.clone(),
        ));
        self.new_frame_ready = new_frame_ready;

        Ok(())
    }

    pub fn wait_for_events(&mut self) -> Result<(), WindowError> {
        if unsafe { (self.wayland.wl_display_dispatch_queue)(self.display, self.event_queue) } == -1
        {
            Err(WindowError::PlatformError(
                "Wayland connection closed".into(),
            ))
        } else {
            Ok(())
        }
    }

    // The rest of the implementation (event handlers, etc.) would be similar to macOS,
    // calling into the cross-platform event processing system.
    fn process_events(&mut self) { /* ... same as X11 ... */
    }
    fn handle_key(&mut self, key: u32, state: u32) { /* ... same as X11 ... */
    }
    fn handle_pointer_motion(&mut self, x: f64, y: f64) { /* ... */
    }
    fn handle_pointer_button(&mut self, serial: u32, button: u32, state: u32) { /* ... */
    }
    fn handle_pointer_axis(&mut self, axis: u32, value: f64) { /* ... */
    }
    fn handle_pointer_enter(&mut self, serial: u32, x: f64, y: f64) { /* ... */
    }
    fn handle_pointer_leave(&mut self, serial: u32) { /* ... */
    }
    fn update_hit_test(&mut self, position: LogicalPosition) { /* ... */
    }
    pub(crate) fn process_window_events_v2(&mut self) -> ProcessEventResult {
        /* ... */
        ProcessEventResult::DoNothing
    }
}

impl Drop for WaylandWindow {
    fn drop(&mut self) {
        unsafe {
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

        let fd = unsafe {
            libc::memfd_create(CString::new("azul-fb").unwrap().as_ptr(), libc::MFD_CLOEXEC)
        };
        if fd == -1 {
            return Err(WindowError::PlatformError("memfd_create failed".into()));
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
        // The fd can be closed after mmap as it's now managed by the kernel
        unsafe { libc::close(fd) };

        if data == libc::MAP_FAILED {
            return Err(WindowError::PlatformError("mmap failed".into()));
        }

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
        }
    }
}
