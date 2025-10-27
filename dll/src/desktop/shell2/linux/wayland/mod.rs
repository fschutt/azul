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

use self::dlopen::{Wayland, Xkb};
use crate::desktop::shell2::common::{PlatformWindow, RenderContext, WindowError, WindowProperties};
use crate::desktop::wr_translate2::{self, AsyncHitTester, Notifier};
use azul_core::{
    callbacks::LayoutCallbackInfo,
    dom::DomId,
    gl::{GlContextPtr, OptionGlContextPtr},
    hit_test::DocumentId,
    resources::{AppConfig, DpiScaleFactor, IdNamespace, ImageCache, RendererResources},
    window::{
        FullWindowState, HwAcceleration, KeyboardState, MouseButton, MouseCursorType,
        MouseState, RawWindowHandle, RendererType, WindowDecorations, WaylandHandle,
    },
    events::{ProcessEventResult},
};
use azul_layout::{
    window::LayoutWindow,
    window_state::{WindowCreateOptions, WindowState},
};
use std::{
    ffi::{c_void, CString},
    rc::Rc,
    sync::{Arc, Condvar, Mutex},
};
use webrender::Renderer as WrRenderer;
use rust_fontconfig::FcFontCache;

/// Tracks the current rendering mode of the window.
enum RenderMode {
    Gpu(gl::GlContext),
    Cpu(CpuFallbackState),
}

/// State for CPU fallback rendering.
struct CpuFallbackState {
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

    render_mode: RenderMode,
}

#[derive(Debug, Clone)]
pub enum WaylandEvent {
    Redraw,
    Close,
    Other,
}

impl PlatformWindow for WaylandWindow {
    type EventType = WaylandEvent;

    fn new(options: WindowCreateOptions) -> Result<Self, WindowError> {
        let wayland = Wayland::new().map_err(|e| {
            WindowError::PlatformError(format!("Failed to load libwayland-client: {:?}", e))
        })?;
        let xkb = Xkb::new()
            .map_err(|e| WindowError::PlatformError(format!("Failed to load libxkbcommon: {:?}", e)))?;

        let display = unsafe { (wayland.wl_display_connect)(std::ptr::null()) };
        if display.is_null() {
            return Err(WindowError::PlatformError(
                "Failed to connect to Wayland display".into(),
            ));
        }

        let event_queue = unsafe { (wayland.wl_display_create_event_queue)(display) };
        let registry = unsafe { (wayland.wl_display_get_registry)(display) };
        unsafe { (wayland.wl_proxy_set_queue)(registry as _, event_queue) };

        let mut temp_state = WaylandWindow {
            wayland: wayland.clone(), xkb, display, event_queue, registry,
            compositor: std::ptr::null_mut(), shm: std::ptr::null_mut(), seat: std::ptr::null_mut(),
            xdg_wm_base: std::ptr::null_mut(), surface: std::ptr::null_mut(),
            xdg_surface: std::ptr::null_mut(), xdg_toplevel: std::ptr::null_mut(),
            is_open: true,
            current_window_state: options.state.clone().into(),
            previous_window_state: None,
            layout_window: None,
            render_api: None, renderer: None, hit_tester: None, document_id: None,
            image_cache: ImageCache::default(), renderer_resources: RendererResources::default(),
            gl_context_ptr: None.into(),
            new_frame_ready: Arc::new((Mutex::new(false), Condvar::new())),
            id_namespace: None,
            keyboard_state: events::KeyboardState::new(), pointer_state: events::PointerState::new(),
            render_mode: RenderMode::Cpu(CpuFallbackState {
                pool: std::ptr::null_mut(), buffer: std::ptr::null_mut(), data: std::ptr::null_mut(),
                width: 0, height: 0, stride: 0
            }),
        };

        let listener = defines::wl_registry_listener {
            global: events::registry_global_handler,
            global_remove: events::registry_global_remove_handler,
        };
        unsafe { (temp_state.wayland.wl_registry_add_listener)(registry, &listener, &mut temp_state as *mut _ as *mut _) };
        unsafe { (temp_state.wayland.wl_display_roundtrip)(display) }; // Synchronous roundtrip to bind globals

        temp_state.surface = unsafe { (temp_state.wayland.wl_compositor_create_surface)(temp_state.compositor) };
        temp_state.xdg_surface = unsafe { (temp_state.wayland.xdg_wm_base_get_xdg_surface)(temp_state.xdg_wm_base, temp_state.surface) };

        let xdg_surface_listener = defines::xdg_surface_listener { configure: events::xdg_surface_configure_handler };
        unsafe { (temp_state.wayland.xdg_surface_add_listener)(temp_state.xdg_surface, &xdg_surface_listener, &mut temp_state as *mut _ as *mut _) };

        temp_state.xdg_toplevel = unsafe { (temp_state.wayland.xdg_surface_get_toplevel)(temp_state.xdg_surface) };
        let title = CString::new(options.state.title.as_str()).unwrap();
        unsafe { (temp_state.wayland.xdg_toplevel_set_title)(temp_state.xdg_toplevel, title.as_ptr()) };

        let width = options.state.size.dimensions.width as i32;
        let height = options.state.size.dimensions.height as i32;
        
        let render_mode = match gl::GlContext::new(&wayland, display, temp_state.surface, width, height) {
            Ok(gl_context) => {
                eprintln!("[Wayland] OpenGL context created successfully.");
                RenderMode::Gpu(gl_context)
            }
            Err(e) => {
                eprintln!("[Wayland] Failed to create OpenGL context: {:?}. Falling back to CPU rendering.", e);
                RenderMode::Cpu(CpuFallbackState::new(&wayland, temp_state.shm, width, height)?)
            }
        };

        temp_state.render_mode = render_mode;

        if let RenderMode::Gpu(gl_context) = &mut temp_state.render_mode {
            gl_context.make_current();
            let gl_functions = gl::GlFunctions::initialize(&gl_context.egl.as_ref().unwrap()).unwrap();
            
            // In a real app, the FcFontCache would be shared from the App struct
            let fc_cache = Arc::new(FcFontCache::build()); 
            
            temp_state.initialize_webrender(&options, &gl_functions, fc_cache)?;
        }

        unsafe { (temp_state.wayland.wl_surface_commit)(temp_state.surface) };
        unsafe { (temp_state.wayland.wl_display_flush)(display) };

        Ok(temp_state)
    }

    fn get_state(&self) -> WindowState { self.current_window_state.clone().into() }
    fn set_properties(&mut self, props: WindowProperties) -> Result<(), WindowError> {
        if let Some(title) = props.title {
            self.current_window_state.title = title.clone().into();
            let c_title = CString::new(title).unwrap();
            unsafe { (self.wayland.xdg_toplevel_set_title)(self.xdg_toplevel, c_title.as_ptr()) };
        }
        Ok(())
    }

    fn poll_event(&mut self) -> Option<Self::EventType> {
        // Dispatches all pending events in the queue without blocking.
        // Returns > 0 if events were dispatched.
        if unsafe { (self.wayland.wl_display_dispatch_queue_pending)(self.display, self.event_queue) } > 0 {
            Some(WaylandEvent::Redraw)
        } else {
            None
        }
    }

    fn wait_for_events(&mut self) -> Result<(), WindowError> {
        // Flushes pending requests and blocks until an event is received.
        if unsafe { (self.wayland.wl_display_dispatch_queue)(self.display, self.event_queue) } == -1 {
            Err(WindowError::PlatformError("Wayland connection closed".into()))
        } else {
            Ok(())
        }
    }

    fn get_render_context(&self) -> RenderContext {
        match &self.render_mode {
            RenderMode::Gpu(ctx) => ctx.egl_context.map(|c| RenderContext::OpenGL { context: c }).unwrap_or(RenderContext::CPU),
            RenderMode::Cpu(_) => RenderContext::CPU,
        }
    }

    fn present(&mut self) -> Result<(), WindowError> {
        match &self.render_mode {
            RenderMode::Gpu(gl_context) => gl_context.swap_buffers(),
            RenderMode::Cpu(cpu_state) => {
                cpu_state.draw_blue();
                unsafe {
                    (self.wayland.wl_surface_attach)(self.surface, cpu_state.buffer, 0, 0);
                    (self.wayland.wl_surface_damage)(self.surface, 0, 0, cpu_state.width, cpu_state.height);
                    (self.wayland.wl_surface_commit)(self.surface);
                }
                Ok(())
            }
        }
    }

    fn is_open(&self) -> bool { self.is_open }
    fn close(&mut self) { self.is_open = false; }
    fn request_redraw(&mut self) { self.present().ok(); }
}

impl WaylandWindow {
    /// The main cross-platform event processing entry point.
    fn process_events(&mut self) {
        if self.renderer.is_none() { return; }
        let result = self.process_window_events_v2();
        self.handle_process_result(result);
    }

    /// Translates a `ProcessEventResult` into actions like redrawing.
    fn handle_process_result(&mut self, result: ProcessEventResult) {
        if result != ProcessEventResult::DoNothing {
            self.request_redraw();
        }
    }

    fn handle_key(&mut self, key: u32, state: u32) {
        let is_down = state == WL_KEYBOARD_KEY_STATE_PRESSED;
        let keycode = key + 8; // Wayland keycodes are 8 less than XKB

        self.previous_window_state = Some(self.current_window_state.clone());

        let keysym = unsafe { (self.xkb.xkb_state_key_get_one_sym)(self.keyboard_state.state, keycode) };
        let virtual_key = keysym_to_virtual_keycode(keysym);
        
        let mut buffer = [0i8; 8];
        let len = unsafe { (self.xkb.xkb_state_key_get_utf8)(self.keyboard_state.state, keycode, buffer.as_mut_ptr(), buffer.len()) };
        let char_str = if len > 0 { unsafe { Some(CStr::from_ptr(buffer.as_ptr()).to_string_lossy().to_string()) } } else { None };

        // Update WindowState
        if let Some(vk) = virtual_key {
            if is_down {
                self.current_window_state.keyboard_state.pressed_virtual_keycodes.insert_hm_item(vk);
                self.current_window_state.keyboard_state.current_virtual_keycode = Some(vk).into();
            } else {
                self.current_window_state.keyboard_state.pressed_virtual_keycodes.remove_hm_item(&vk);
                self.current_window_state.keyboard_state.current_virtual_keycode = None.into();
            }
        }
        if is_down {
            self.current_window_state.keyboard_state.current_char = char_str.and_then(|s| s.chars().next()).map(|c| c as u32).into();
        } else {
            self.current_window_state.keyboard_state.current_char = None.into();
        }

        self.process_events();
    }

    fn handle_pointer_motion(&mut self, x: f64, y: f64) {
        let position = LogicalPosition::new(x as f32, y as f32);
        self.previous_window_state = Some(self.current_window_state.clone());
        self.current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(position);

        self.update_hit_test(position);
        self.process_events();
    }

    fn handle_pointer_button(&mut self, serial: u32, button: u32, state: u32) {
        let is_down = state == WL_POINTER_BUTTON_STATE_PRESSED;
        self.pointer_state.serial = serial;
        
        // Linux button codes: 272=left, 273=right, 274=middle
        let mouse_button = match button {
            272 => MouseButton::Left,
            273 => MouseButton::Right,
            274 => MouseButton::Middle,
            _ => MouseButton::Other(button as u8),
        };

        self.previous_window_state = Some(self.current_window_state.clone());
        match mouse_button {
            MouseButton::Left => self.current_window_state.mouse_state.left_down = is_down,
            MouseButton::Right => self.current_window_state.mouse_state.right_down = is_down,
            MouseButton::Middle => self.current_window_state.mouse_state.middle_down = is_down,
            _ => {}
        }

        if is_down {
            self.pointer_state.button_down = Some(mouse_button);
        } else {
            self.pointer_state.button_down = None;
        }

        self.process_events();
    }

    fn handle_pointer_axis(&mut self, axis: u32, value: f64) {
        self.previous_window_state = Some(self.current_window_state.clone());
        let scroll_value = (value / 10.0) as f32; // Convert 'degrees' to lines

        match axis {
            WL_POINTER_AXIS_VERTICAL_SCROLL => {
                let current = self.current_window_state.mouse_state.scroll_y.into_option().unwrap_or(0.0);
                self.current_window_state.mouse_state.scroll_y = (current - scroll_value).into();
            },
            WL_POINTER_AXIS_HORIZONTAL_SCROLL => {
                let current = self.current_window_state.mouse_state.scroll_x.into_option().unwrap_or(0.0);
                self.current_window_state.mouse_state.scroll_x = (current + scroll_value).into();
            },
            _ => {}
        }

        self.process_events();
    }

    fn handle_pointer_enter(&mut self, serial: u32, x: f64, y: f64) {
        self.pointer_state.serial = serial;
        let position = LogicalPosition::new(x as f32, y as f32);
        self.previous_window_state = Some(self.current_window_state.clone());
        self.current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(position);

        self.update_hit_test(position);
        self.process_events();
    }

    fn handle_pointer_leave(&mut self, serial: u32) {
        self.pointer_state.serial = serial;
        self.previous_window_state = Some(self.current_window_state.clone());
        let last_pos = self.current_window_state.mouse_state.cursor_position.get_position().unwrap_or_default();
        self.current_window_state.mouse_state.cursor_position = CursorPosition::OutOfWindow(last_pos);

        self.process_events();
    }

    /// Update hit test at given position and store in current_window_state.
    fn update_hit_test(&mut self, position: LogicalPosition) {
        if self.hit_tester.is_none() || self.layout_window.is_none() || self.document_id.is_none() { return; }

        let layout_window = self.layout_window.as_ref().unwrap();
        let hit_tester = self.hit_tester.as_mut().unwrap().resolve();

        let cursor_position = CursorPosition::InWindow(position);
        let hit_test = wr_translate2::fullhittest_new_webrender(
            &*hit_tester,
            self.document_id.unwrap(),
            self.current_window_state.focused_node,
            &layout_window.layout_results,
            &cursor_position,
            self.current_window_state.size.get_hidpi_factor(),
        );
        self.current_window_state.last_hit_test = hit_test;
    }
    
    // V2 Cross-Platform Event Processing (adapted from macOS)
    pub(crate) fn process_window_events_v2(&mut self) -> ProcessEventResult {
        use azul_core::events::{dispatch_events, CallbackTarget as CoreCallbackTarget};
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

        // In a full implementation, you would iterate dispatch_result.callbacks
        // and invoke them. For now, we just acknowledge that work needs to be done.
        
        ProcessEventResult::ShouldReRenderCurrentWindow
    }
}

impl Drop for WaylandWindow {
    fn drop(&mut self) {
        unsafe {
            if !self.xdg_toplevel.is_null() { (self.wayland.xdg_toplevel_destroy)(self.xdg_toplevel); }
            if !self.xdg_surface.is_null() { (self.wayland.xdg_surface_destroy)(self.xdg_surface); }
            if !self.surface.is_null() { (self.wayland.wl_surface_destroy)(self.surface); }
            if !self.event_queue.is_null() { (self.wayland.wl_event_queue_destroy)(self.event_queue); }
            if !self.display.is_null() { (self.wayland.wl_display_disconnect)(self.display); }
        }
    }
}

impl CpuFallbackState {
    fn new(wayland: &Rc<Wayland>, shm: *mut wl_shm, width: i32, height: i32) -> Result<Self, WindowError> {
        let stride = width * 4;
        let size = stride * height;

        let fd = unsafe { libc::memfd_create(CString::new("azul-fb").unwrap().as_ptr(), libc::MFD_CLOEXEC) };
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
        unsafe { libc::close(fd) }; // The fd can be closed after mmap

        if data == libc::MAP_FAILED {
            return Err(WindowError::PlatformError("mmap failed".into()));
        }

        let pool = unsafe { (wayland.wl_shm_create_pool)(shm, fd, size) };
        let buffer = unsafe { (wayland.wl_shm_pool_create_buffer)(pool, 0, width, height, stride, WL_SHM_FORMAT_ARGB8888) };

        Ok(Self { pool, buffer, data: data as *mut u8, width, height, stride })
    }

    fn draw_blue(&self) {
        let size = (self.stride * self.height) as usize;
        let slice = unsafe { std::slice::from_raw_parts_mut(self.data, size) };
        for chunk in slice.chunks_exact_mut(4) {
            chunk[0] = 0xFF; // Blue
            chunk[1] = 0x00; // Green
            chunk[2] = 0x00; // Red
            chunk[3] = 0xFF; // Alpha
        }
    }
}

impl Drop for CpuFallbackState {
    fn drop(&mut self) {
        unsafe {
            // Unmap memory before destroying the pool
            if !self.data.is_null() {
                libc::munmap(self.data as *mut _, (self.stride * self.height) as usize);
            }
            if !self.buffer.is_null() {
                (Wayland::new().unwrap().wl_buffer_destroy)(self.buffer);
            }
            if !self.pool.is_null() {
                (Wayland::new().unwrap().wl_shm_pool_destroy)(self.pool);
            }
        }
    }
}