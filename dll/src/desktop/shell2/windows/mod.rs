//! Windows implementation using Win32 API.
//!
//! This module implements window management for Windows using the Win32 API
//! with dynamic loading to support cross-compilation from macOS.
//!
//! Architecture:
//! - Win32Window: Main window struct integrating LayoutWindow
//! - WindowProc: Win32 message handler
//! - Dynamic loading: All Win32 APIs loaded via dlopen
//!
//! Integration points:
//! - LayoutWindow: UI state and callbacks
//! - WebRender: Rendering and display lists
//! - Common shell2 modules: Compositor, error handling

pub mod dlopen;
mod dpi;
pub mod event;
mod gl;
pub mod menu;
mod process;
mod wcreate;

use std::{
    collections::{BTreeMap, HashMap},
    ffi::c_void,
    ptr,
    rc::Rc,
    sync::Arc,
};

use azul_core::{
    gl::OptionGlContextPtr,
    hit_test::{DocumentId, PipelineId},
    menu::CoreMenuCallback,
    refany::RefAny,
    resources::{DpiScaleFactor, IdNamespace, ImageCache, RendererResources},
    window::{OptionMouseCursorType, RendererType, WindowFrame},
};
use azul_layout::{
    hit_test::FullHitTest,
    window::LayoutWindow,
    window_state::{FullWindowState, WindowCreateOptions},
};
use rust_fontconfig::FcFontCache;
use webrender::{RenderApi as WrRenderApi, Renderer as WrRenderer, Transaction as WrTransaction};

use self::{
    dlopen::{DynamicLibrary, HDC, HGLRC, HINSTANCE, HMENU, HWND},
    dpi::DpiFunctions,
    gl::GlFunctions,
    process::ProcessEventResult,
};
use crate::desktop::{
    shell2::common::{Compositor, WindowError, WindowProperties},
    wr_translate2::{
        default_renderer_options, translate_document_id_wr, translate_id_namespace_wr,
        wr_translate_document_id, AsyncHitTester, Notifier, WR_SHADER_CACHE,
    },
};

/// Win32 window implementation using LayoutWindow API
pub struct Win32Window {
    /// Win32 window handle
    pub hwnd: HWND,
    /// Application instance handle
    pub hinstance: HINSTANCE,

    // LayoutWindow integration
    /// LayoutWindow for UI state management and callbacks
    pub layout_window: Option<LayoutWindow>,

    // Rendering infrastructure
    /// OpenGL context (None if running in software mode)
    pub gl_context: Option<HGLRC>,
    /// OpenGL function loader
    pub gl_functions: GlFunctions,
    /// OpenGL context pointer with compiled shaders
    pub gl_context_ptr: OptionGlContextPtr,
    /// WebRender renderer
    pub renderer: Option<WrRenderer>,
    /// WebRender render API
    pub render_api: WrRenderApi,
    /// Hit-tester for fast hit-testing
    pub hit_tester: AsyncHitTester,
    /// WebRender document ID
    pub document_id: DocumentId,
    /// WebRender ID namespace
    pub id_namespace: IdNamespace,

    // Win32 libraries
    /// Dynamically loaded Win32 libraries
    pub win32: dlopen::Win32Libraries,

    // Window state
    /// Window is open flag
    pub is_open: bool,
    /// Previous window state (for diffing)
    pub previous_window_state: Option<FullWindowState>,
    /// Current window state
    pub current_window_state: FullWindowState,

    // Resource caches
    /// Image cache
    pub image_cache: ImageCache,
    /// Renderer resources (textures, etc.)
    pub renderer_resources: RendererResources,

    // Menu and UI state
    /// Menu bar (if any)
    pub menu_bar: Option<menu::WindowsMenuBar>,
    /// Context menu callbacks (active when context menu is open)
    pub context_menu: Option<BTreeMap<u16, CoreMenuCallback>>,

    // Timers and threads
    /// Active timers (TimerId -> Win32 timer handle)
    pub timers: HashMap<usize, usize>,
    /// Thread timer (for polling thread messages)
    pub thread_timer_running: Option<usize>,

    // Input state
    /// High surrogate for UTF-16 character composition
    pub high_surrogate: Option<u16>,

    // System functions
    /// DPI functions
    pub dpi: DpiFunctions,

    // Shared resources
    /// Font cache (shared across all windows)
    pub fc_cache: Arc<FcFontCache>,
}

impl Win32Window {
    /// Create a new Win32 window with given options
    pub fn new(
        options: WindowCreateOptions,
        fc_cache: Arc<FcFontCache>,
        app_data: Arc<std::cell::RefCell<RefAny>>,
    ) -> Result<Self, WindowError> {
        // Load Win32 libraries
        let win32 = dlopen::Win32Libraries::load().map_err(|e| {
            WindowError::PlatformError(format!("Failed to load Win32 libraries: {}", e))
        })?;

        // Get HINSTANCE from GetModuleHandleW(NULL)
        let hinstance = {
            let null_str = vec![0u16];
            unsafe { (win32.user32.GetModuleHandleW)(null_str.as_ptr()) }
        };

        if hinstance.is_null() {
            return Err(WindowError::PlatformError("Failed to get HINSTANCE".into()));
        }

        // Initialize DPI awareness
        let dpi_functions = DpiFunctions::init();
        dpi_functions.become_dpi_aware();

        // Register window class with our window procedure
        wcreate::register_window_class(hinstance, Some(window_proc), &win32)?;

        // Create HWND
        let hwnd = wcreate::create_hwnd(
            hinstance,
            &options,
            None,            // No parent window
            ptr::null_mut(), // User data will be set later
            &win32,
        )?;

        // Get DPI for window
        let dpi = unsafe { dpi_functions.hwnd_dpi(hwnd as _) };
        let dpi_factor = dpi::dpi_to_scale_factor(dpi);

        // Update options with actual DPI
        let mut options = options;
        options.state.size.dpi = dpi;

        // Initialize OpenGL context (if hardware rendering requested)
        let mut gl_context: Option<HGLRC> = None;
        let mut gl_functions = GlFunctions::initialize();
        let mut gl_context_ptr: OptionGlContextPtr = None.into();

        // Determine renderer type from options
        let should_use_hardware = match options.renderer.into_option() {
            Some(r) => match r.hw_accel {
                azul_core::window::HwAcceleration::Enabled => true,
                azul_core::window::HwAcceleration::Disabled => false,
                azul_core::window::HwAcceleration::DontCare => true, // Try hardware first
            },
            None => true, // Default to hardware
        };

        if should_use_hardware {
            // Try to create OpenGL context
            match wcreate::create_gl_context(hwnd, hinstance, &win32) {
                Ok(hglrc) => {
                    gl_context = Some(hglrc);
                    // Make context current and load GL functions
                    let hdc = unsafe { (win32.user32.GetDC)(hwnd) };
                    if !hdc.is_null() {
                        #[cfg(target_os = "windows")]
                        unsafe {
                            use winapi::um::wingdi::wglMakeCurrent;
                            wglMakeCurrent(
                                hdc as winapi::shared::windef::HDC,
                                hglrc as winapi::shared::windef::HGLRC,
                            );
                        }
                        gl_functions.load();
                        gl_context_ptr =
                            OptionGlContextPtr::Some(azul_core::gl::GlContextPtr::new(
                                RendererType::Hardware,
                                gl_functions.functions.clone(),
                            ));
                        #[cfg(target_os = "windows")]
                        unsafe {
                            use winapi::um::wingdi::wglMakeCurrent;
                            wglMakeCurrent(ptr::null_mut(), ptr::null_mut());
                        }
                        unsafe { (win32.user32.ReleaseDC)(hwnd, hdc) };
                    }
                }
                Err(_) => {
                    // Fall back to software rendering
                    gl_context_ptr = OptionGlContextPtr::None;
                }
            }
        }

        // Initialize WebRender
        let new_frame_ready =
            std::sync::Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
        let (mut renderer, sender) = webrender::create_webrender_instance(
            gl_functions.functions.clone(),
            Box::new(Notifier {
                new_frame_ready: new_frame_ready.clone(),
            }),
            default_renderer_options(&options),
            None, // shader cache
        )
        .map_err(|e| WindowError::PlatformError(format!("WebRender error: {:?}", e)))?;

        // Set up external image handler (Compositor)
        renderer.set_external_image_handler(Box::new(
            crate::desktop::wr_translate2::Compositor::default(),
        ));

        let mut render_api = sender.create_api();

        // Get window size
        let (width, height) = wcreate::get_client_rect(hwnd, &win32)?;
        let physical_size = azul_core::geom::PhysicalSize::new(width, height);

        // Create WebRender document
        let framebuffer_size =
            webrender::api::units::DeviceIntSize::new(width as i32, height as i32);
        let wr_doc_id = render_api.add_document(framebuffer_size);
        let document_id = translate_document_id_wr(wr_doc_id);
        let id_namespace = translate_id_namespace_wr(render_api.get_namespace_id());

        // Request initial hit-tester
        let hit_tester_request =
            render_api.request_hit_tester(wr_translate_document_id(document_id));
        let hit_tester = AsyncHitTester::Requested(hit_tester_request);

        // Update options size with actual window size
        options.state.size.dimensions = physical_size.to_logical(dpi_factor);

        // Determine renderer type
        let renderer_type = if gl_context.is_some() {
            RendererType::Hardware
        } else {
            RendererType::Software
        };

        // Create initial window state
        let initial_window_state = options.state.clone();

        // Create LayoutWindow with initial UI callback
        let mut layout_window = LayoutWindow::new((*fc_cache).clone()).map_err(|e| {
            WindowError::PlatformError(format!("Failed to create LayoutWindow: {:?}", e))
        })?;

        // Build FullWindowState from WindowState
        let current_window_state = FullWindowState {
            title: initial_window_state.title.clone(),
            size: initial_window_state.size.clone(),
            position: initial_window_state.position,
            flags: initial_window_state.flags,
            theme: initial_window_state.theme,
            debug_state: initial_window_state.debug_state,
            keyboard_state: Default::default(),
            mouse_state: Default::default(),
            touch_state: Default::default(),
            ime_position: initial_window_state.ime_position,
            platform_specific_options: initial_window_state.platform_specific_options.clone(),
            renderer_options: initial_window_state.renderer_options,
            background_color: initial_window_state.background_color,
            layout_callback: initial_window_state.layout_callback,
            close_callback: initial_window_state.close_callback.clone(),
            monitor: initial_window_state.monitor,
            hovered_file: None,
            dropped_file: None,
            focused_node: None,
            last_hit_test: FullHitTest::empty(None),
            selections: Default::default(),
            window_focused: true,
        };

        // Set document_id and id_namespace for this window
        layout_window.document_id = document_id;
        layout_window.id_namespace = id_namespace;
        layout_window.current_window_state = current_window_state.clone();
        layout_window.renderer_type = Some(renderer_type);

        // Set up menu bar if present
        // TODO: Menu bar needs to be extracted from window state
        let menu_bar = None;

        // Handle size_to_content
        // TODO: size_to_content needs to be implemented with new layout API
        /*
        if options.size_to_content {
            let content_size = layout_window.get_content_size();
            wcreate::set_window_size(
                hwnd,
                libm::roundf(content_size.width) as i32,
                libm::roundf(content_size.height) as i32,
                &win32,
            )?;
        }
        */

        // Show window with appropriate frame state
        wcreate::show_window_with_frame(
            hwnd,
            layout_window.current_window_state.flags.frame,
            layout_window.current_window_state.flags.is_visible,
            &win32,
        );

        let mut txn = WrTransaction::new();

        // Build initial display list - no resource_updates needed with new API
        let hidpi_factor = layout_window.current_window_state.size.get_hidpi_factor();
        crate::desktop::wr_translate2::rebuild_display_list(
            &mut txn,
            &mut layout_window,
            &mut render_api,
            &ImageCache::default(),
            Vec::new(),
            &mut RendererResources::default(),
            hidpi_factor,
        );

        render_api.flush_scene_builder();

        crate::desktop::wr_translate2::generate_frame(
            &mut txn,
            &mut layout_window,
            &mut render_api,
            true,
        );
        render_api.send_transaction(wr_translate_document_id(document_id), txn);
        render_api.flush_scene_builder();

        // Get current window state
        let current_window_state = layout_window.current_window_state.clone();

        // Build window structure
        Ok(Win32Window {
            hwnd,
            hinstance,
            layout_window: Some(layout_window),
            gl_context,
            gl_functions,
            gl_context_ptr,
            renderer: Some(renderer),
            render_api,
            hit_tester,
            document_id,
            id_namespace,
            win32, // Store Win32 libraries for later use
            is_open: true,
            previous_window_state: None,
            current_window_state,
            image_cache: ImageCache::default(),
            renderer_resources: RendererResources::default(),
            menu_bar,
            context_menu: None,
            timers: HashMap::new(),
            thread_timer_running: None,
            high_surrogate: None,
            dpi: dpi_functions,
            fc_cache,
        })
    }

    /// Start or stop timers based on changes
    pub fn start_stop_timers(
        &mut self,
        added: HashMap<usize, azul_layout::timer::Timer>,
        removed: std::collections::BTreeSet<usize>,
    ) {
        // Start new timers
        for (id, timer) in added {
            let interval_ms = timer.tick_millis().min(u32::MAX as u64) as u32;
            let timer_id =
                unsafe { (self.win32.user32.SetTimer)(self.hwnd, id, interval_ms, ptr::null()) };
            self.timers.insert(id, timer_id);
        }

        // Stop removed timers
        for id in removed {
            if let Some(timer_id) = self.timers.remove(&id) {
                unsafe { (self.win32.user32.KillTimer)(self.hwnd, timer_id) };
            }
        }
    }

    /// Start or stop threads based on changes
    pub fn start_thread_tick_timer(&mut self) {
        if self.thread_timer_running.is_none() {
            // Start thread polling timer (16ms = ~60 FPS)
            let timer_id = unsafe {
                (self.win32.user32.SetTimer)(
                    self.hwnd,
                    0xFFFF, // Special ID for thread timer
                    16,
                    ptr::null(),
                )
            };
            self.thread_timer_running = Some(timer_id);
        }
    }

    pub fn stop_thread_tick_timer(&mut self) {
        if let Some(timer_id) = self.thread_timer_running.take() {
            unsafe { (self.win32.user32.KillTimer)(self.hwnd, timer_id) };
        }
    }

    /// Render and present a frame
    pub fn render_and_present(&mut self) -> Result<(), WindowError> {
        let renderer = self
            .renderer
            .as_mut()
            .ok_or_else(|| WindowError::PlatformError("No renderer available".into()))?;

        // Get device context
        unsafe {
            let hdc = (self.win32.user32.GetDC)(self.hwnd);
            if hdc.is_null() {
                return Err(WindowError::PlatformError("Failed to get HDC".into()));
            }

            // Make OpenGL context current if we have one
            if let Some(hglrc) = self.gl_context {
                #[cfg(target_os = "windows")]
                unsafe {
                    use winapi::um::wingdi::wglMakeCurrent;
                    wglMakeCurrent(
                        hdc as winapi::shared::windef::HDC,
                        hglrc as winapi::shared::windef::HGLRC,
                    );
                }
            }

            // Update WebRender
            renderer.update();

            // Render frame
            let (width, height) = wcreate::get_client_rect(self.hwnd, &self.win32)?;
            let framebuffer_size =
                webrender::api::units::DeviceIntSize::new(width as i32, height as i32);

            renderer
                .render(framebuffer_size, 0)
                .map_err(|e| WindowError::PlatformError(format!("Render error: {:?}", e)))?;

            // Swap buffers if we have OpenGL context
            if self.gl_context.is_some() {
                #[cfg(target_os = "windows")]
                unsafe {
                    use winapi::um::wingdi::SwapBuffers;
                    SwapBuffers(hdc as winapi::shared::windef::HDC);
                }
            }

            // Release device context
            (self.win32.user32.ReleaseDC)(self.hwnd, hdc);

            Ok(())
        }
    }

    // ========================================================================
    // V2 Cross-Platform Event Processing (similar to macOS implementation)
    // ========================================================================

    /// V2: Process window events using cross-platform dispatch system.
    pub(crate) fn process_window_events_v2(&mut self) -> process::ProcessEventResult {
        self.process_window_events_recursive_v2(0)
    }

    /// V2: Recursive event processing with depth limit.
    fn process_window_events_recursive_v2(&mut self, depth: usize) -> process::ProcessEventResult {
        const MAX_EVENT_RECURSION_DEPTH: usize = 5;

        if depth >= MAX_EVENT_RECURSION_DEPTH {
            eprintln!(
                "[Events] Max recursion depth {} reached",
                MAX_EVENT_RECURSION_DEPTH
            );
            return process::ProcessEventResult::DoNothing;
        }

        use azul_core::events::{dispatch_events, CallbackTarget as CoreCallbackTarget};
        use azul_layout::window_state::create_events_from_states;

        // Get previous state (or use current as fallback for first frame)
        let previous_state = self
            .previous_window_state
            .as_ref()
            .unwrap_or(&self.current_window_state);

        // Detect all events that occurred by comparing states
        let events = create_events_from_states(&self.current_window_state, previous_state);

        if events.is_empty() {
            return process::ProcessEventResult::DoNothing;
        }

        // Get hit test if available
        let hit_test = if !self.current_window_state.last_hit_test.is_empty() {
            Some(&self.current_window_state.last_hit_test)
        } else {
            None
        };

        // Use cross-platform dispatch logic to determine which callbacks to invoke
        let dispatch_result = dispatch_events(&events, hit_test);

        if dispatch_result.is_empty() {
            return process::ProcessEventResult::DoNothing;
        }

        // Invoke all callbacks and collect results
        let mut result = process::ProcessEventResult::DoNothing;
        let mut should_stop_propagation = false;
        let mut should_recurse = false;

        for callback_to_invoke in &dispatch_result.callbacks {
            if should_stop_propagation {
                break;
            }

            // Convert core CallbackTarget to process CallbackTarget
            let target = match &callback_to_invoke.target {
                CoreCallbackTarget::Node { dom_id, node_id } => {
                    process::CallbackTarget::Node(process::HitTestNode {
                        dom_id: dom_id.inner as u64,
                        node_id: node_id.index() as u64,
                    })
                }
                CoreCallbackTarget::RootNodes => process::CallbackTarget::RootNodes,
            };

            // Invoke callbacks through process module
            let callback_results =
                process::invoke_callbacks(self, target, callback_to_invoke.event_filter);

            for callback_result in callback_results {
                let event_result = process::process_callback_result(self, &callback_result);
                result = result.max(event_result);

                // Check if we should stop propagation
                if callback_result.stop_propagation {
                    should_stop_propagation = true;
                    break;
                }

                // Check if we need to recurse (DOM was regenerated)
                use azul_core::callbacks::Update;
                if matches!(
                    callback_result.callbacks_update_screen,
                    Update::RefreshDom | Update::RefreshDomAllWindows
                ) {
                    should_recurse = true;
                }
            }
        }

        // Recurse if needed
        if should_recurse && depth + 1 < MAX_EVENT_RECURSION_DEPTH {
            let recursive_result = self.process_window_events_recursive_v2(depth + 1);
            result = result.max(recursive_result);
        }

        result
    }

    /// Regenerate layout (called after DOM changes)
    pub fn regenerate_layout(&mut self) -> Result<(), String> {
        let layout_window = match self.layout_window.as_mut() {
            Some(lw) => lw,
            None => return Err("No layout window".into()),
        };

        eprintln!("[regenerate_layout] Regenerating display list");

        // Rebuild display list via WebRender
        let mut resource_updates = Vec::new();
        let mut txn = WrTransaction::new();
        crate::desktop::wr_translate2::rebuild_display_list(
            &mut txn,
            layout_window,
            &mut self.render_api,
            &self.image_cache,
            resource_updates,
            &mut self.renderer_resources,
            layout_window.current_window_state.size.get_hidpi_factor(),
        );
        self.render_api
            .send_transaction(wr_translate_document_id(self.document_id), txn);
        self.render_api.flush_scene_builder();

        let mut txn = WrTransaction::new();
        crate::desktop::wr_translate2::generate_frame(
            &mut txn,
            layout_window,
            &mut self.render_api,
            true,
        );
        self.render_api
            .send_transaction(wr_translate_document_id(self.document_id), txn);
        self.render_api.flush_scene_builder();

        eprintln!("[regenerate_layout] Display list regenerated");

        Ok(())
    }

    /// Get raw window handle for callbacks
    pub fn get_raw_window_handle(&self) -> azul_core::window::RawWindowHandle {
        azul_core::window::RawWindowHandle::Windows(azul_core::window::WindowsHandle {
            hwnd: self.hwnd as *mut core::ffi::c_void,
            hinstance: self.hinstance as *mut core::ffi::c_void,
        })
    }

    /// Get HiDPI factor from current window
    pub fn get_hidpi_factor(&self) -> DpiScaleFactor {
        self.current_window_state.size.get_hidpi_factor()
    }

    /// GPU scroll implementation (similar to macOS)
    pub fn gpu_scroll(
        &mut self,
        dom_id: u64,
        node_id: u64,
        scroll_x: f32,
        scroll_y: f32,
    ) -> Result<(), String> {
        let layout_window = match self.layout_window.as_mut() {
            Some(lw) => lw,
            None => return Err("No layout window".into()),
        };

        use azul_core::dom::{DomId, NodeId};

        let dom_id = DomId {
            inner: dom_id as usize,
        };
        let node_id = match NodeId::from_usize(node_id as usize) {
            Some(nid) => nid,
            None => return Err("Invalid node ID".into()),
        };

        // Apply scroll delta
        // TODO: ScrollManager API changed - need to update this
        /*
        layout_window
            .scroll_states
            .scroll_node_with_id(dom_id, node_id, scroll_x, scroll_y);
        */

        // Update WebRender
        let mut resource_updates = Vec::new();
        let mut txn = WrTransaction::new();
        crate::desktop::wr_translate2::rebuild_display_list(
            &mut txn,
            layout_window,
            &mut self.render_api,
            &self.image_cache,
            resource_updates,
            &mut self.renderer_resources,
            self.current_window_state.size.get_hidpi_factor(),
        );

        self.render_api
            .send_transaction(wr_translate_document_id(self.document_id), txn);
        self.render_api.flush_scene_builder();

        Ok(())
    }
}

// Helper function for default window processing when Win32 libraries aren't available
#[inline]
unsafe fn default_window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: dlopen::WPARAM,
    lparam: dlopen::LPARAM,
) -> dlopen::LRESULT {
    #[cfg(target_os = "windows")]
    {
        use winapi::um::winuser::DefWindowProcW;
        DefWindowProcW(hwnd as winapi::shared::windef::HWND, msg, wparam, lparam)
    }
    #[cfg(not(target_os = "windows"))]
    {
        0
    }
}

// Win32 message handler
unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: dlopen::WPARAM,
    lparam: dlopen::LPARAM,
) -> dlopen::LRESULT {
    // Message constants
    const WM_NCCREATE: u32 = 0x0081;
    const WM_CREATE: u32 = 0x0001;
    const WM_DESTROY: u32 = 0x0002;
    const WM_PAINT: u32 = 0x000F;
    const WM_CLOSE: u32 = 0x0010;
    const WM_ERASEBKGND: u32 = 0x0014;
    const WM_SIZE: u32 = 0x0005;
    const WM_MOUSEMOVE: u32 = 0x0200;
    const WM_LBUTTONDOWN: u32 = 0x0201;
    const WM_LBUTTONUP: u32 = 0x0202;
    const WM_RBUTTONDOWN: u32 = 0x0204;
    const WM_RBUTTONUP: u32 = 0x0205;
    const WM_MBUTTONDOWN: u32 = 0x0207;
    const WM_MBUTTONUP: u32 = 0x0208;
    const WM_MOUSEWHEEL: u32 = 0x020A;
    const WM_KEYDOWN: u32 = 0x0100;
    const WM_KEYUP: u32 = 0x0101;
    const WM_CHAR: u32 = 0x0102;
    const WM_SYSKEYDOWN: u32 = 0x0104;
    const WM_SYSKEYUP: u32 = 0x0105;
    const WM_SYSCHAR: u32 = 0x0106;
    const WM_SETFOCUS: u32 = 0x0007;
    const WM_KILLFOCUS: u32 = 0x0008;
    const WM_TIMER: u32 = 0x0113;
    const WM_COMMAND: u32 = 0x0111;
    const WM_MOUSELEAVE: u32 = 0x02A3;
    const WM_DPICHANGED: u32 = 0x02E0;

    const GWLP_USERDATA: i32 = -21;
    const WHEEL_DELTA: i32 = 120;

    // For WM_NCCREATE, we need to load Win32 libraries temporarily just to set up window
    if msg == WM_NCCREATE {
        let win32 = match dlopen::Win32Libraries::load() {
            Ok(w) => w,
            Err(_) => return default_window_proc(hwnd, msg, wparam, lparam),
        };

        #[repr(C)]
        struct CREATESTRUCTW {
            lpCreateParams: *mut core::ffi::c_void,
            hInstance: HINSTANCE,
            hMenu: dlopen::HMENU,
            hwndParent: HWND,
            cy: i32,
            cx: i32,
            y: i32,
            x: i32,
            style: i32,
            lpszName: *const u16,
            lpszClass: *const u16,
            dwExStyle: u32,
        }

        let createstruct = lparam as *mut CREATESTRUCTW;
        let data_ptr = (*createstruct).lpCreateParams;
        (win32.user32.SetWindowLongPtrW)(hwnd, GWLP_USERDATA, data_ptr as isize);
        return (win32.user32.DefWindowProcW)(hwnd, msg, wparam, lparam);
    }

    // Get window user data (Win32Window pointer) - need temporary Win32 libs for this lookup
    let temp_win32 = match dlopen::Win32Libraries::load() {
        Ok(w) => w,
        Err(_) => return default_window_proc(hwnd, msg, wparam, lparam),
    };

    let window_ptr = (temp_win32.user32.GetWindowLongPtrW)(hwnd, GWLP_USERDATA) as *mut Win32Window;

    if window_ptr.is_null() {
        // No user data yet, use default processing
        return (temp_win32.user32.DefWindowProcW)(hwnd, msg, wparam, lparam);
    }

    let window = &mut *window_ptr;
    // Now we can use window.win32 instead of temp_win32 for the rest of the function

    // Handle messages
    match msg {
        WM_CREATE => {
            // Window created
            0
        }

        WM_DESTROY => {
            // Window destroyed
            window.is_open = false;
            0
        }

        WM_CLOSE => {
            // User clicked close button
            window.is_open = false;
            (window.win32.user32.DestroyWindow)(hwnd);
            0
        }

        WM_ERASEBKGND => {
            // Don't erase background, we'll paint everything
            1
        }

        WM_PAINT => {
            // Render frame
            match window.render_and_present() {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("Render error: {:?}", e);
                }
            }
            (window.win32.user32.DefWindowProcW)(hwnd, msg, wparam, lparam)
        }

        WM_SIZE => {
            // Window resized
            let width = (lparam & 0xFFFF) as u32;
            let height = ((lparam >> 16) & 0xFFFF) as u32;

            if width > 0 && height > 0 {
                use azul_core::{geom::PhysicalSizeU32, window::WindowSize};

                let physical_size = PhysicalSizeU32::new(width, height);
                let dpi = window.current_window_state.size.dpi;
                let hidpi_factor = dpi as f32 / 96.0;
                let logical_size = physical_size.to_logical(hidpi_factor);

                // Update window state
                let mut new_window_state = window.current_window_state.clone();
                new_window_state.size.dimensions = logical_size;

                // Determine window frame state
                use azul_core::window::WindowFrame;
                let frame = match wparam as u32 {
                    0x0002 => WindowFrame::Maximized, // SIZE_MAXIMIZED
                    0x0001 => WindowFrame::Minimized, // SIZE_MINIMIZED
                    _ => WindowFrame::Normal,         // SIZE_RESTORED
                };
                new_window_state.flags.frame = frame;

                // Update WebRender document view
                use webrender::{
                    api::units::{DeviceIntRect, DeviceIntSize},
                    Transaction as WrTransaction,
                };

                use crate::desktop::wr_translate2::wr_translate_document_id;

                let mut txn = WrTransaction::new();
                txn.set_document_view(DeviceIntRect::from_size(DeviceIntSize::new(
                    width as i32,
                    height as i32,
                )));

                window
                    .render_api
                    .send_transaction(wr_translate_document_id(window.document_id), txn);

                // Update previous and current window state
                window.previous_window_state = Some(window.current_window_state.clone());
                window.current_window_state = new_window_state;

                // Resize requires full display list rebuild - similar to macOS handle_resize
                // The regenerate_layout will be called in the render loop if needed

                // Request redraw
                (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
            }

            0
        }

        WM_MOUSEMOVE => {
            // Mouse moved - similar to macOS handle_mouse_move
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;

            use azul_core::{geom::LogicalPosition, window::CursorPosition};

            let hidpi_factor = window.current_window_state.size.get_hidpi_factor();
            let logical_pos = LogicalPosition::new(
                x as f32 / hidpi_factor.inner.get(),
                y as f32 / hidpi_factor.inner.get(),
            );

            // Save previous state BEFORE making changes
            window.previous_window_state = Some(window.current_window_state.clone());

            // Update mouse state
            window.current_window_state.mouse_state.cursor_position =
                CursorPosition::InWindow(logical_pos);

            // Update hit test
            if let Some(ref layout_window) = window.layout_window {
                use crate::desktop::wr_translate2::fullhittest_new_webrender;

                let hit_tester = window.hit_tester.resolve();
                let hit_test = fullhittest_new_webrender(
                    &*hit_tester,
                    window.document_id,
                    window.current_window_state.focused_node,
                    &layout_window.layout_results,
                    &CursorPosition::InWindow(logical_pos),
                    hidpi_factor,
                );

                window.current_window_state.last_hit_test = hit_test;

                let cursor_type_hit_test = layout_window
                    .compute_cursor_type_hit_test(&window.current_window_state.last_hit_test);

                // Extract the cursor icon from the hit test result
                let new_cursor_type = cursor_type_hit_test.cursor_icon;
                let new = OptionMouseCursorType::Some(new_cursor_type);

                // Update cursor type
                if window.current_window_state.mouse_state.mouse_cursor_type != new {
                    window.current_window_state.mouse_state.mouse_cursor_type = new;
                    event::set_cursor(new_cursor_type, &window.win32);
                }
            }

            // V2 system will detect MouseOver/MouseEnter/MouseLeave/Drag from state diff
            let result = window.process_window_events_v2();

            // Request redraw if needed
            if !matches!(result, process::ProcessEventResult::DoNothing) {
                (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
            }

            0
        }

        WM_LBUTTONDOWN => {
            // Left mouse button down - similar to macOS handle_mouse_down
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;

            use azul_core::{geom::LogicalPosition, window::CursorPosition};

            let hidpi_factor = window.current_window_state.size.get_hidpi_factor();
            let logical_pos = LogicalPosition::new(
                x as f32 / hidpi_factor.inner.get(),
                y as f32 / hidpi_factor.inner.get(),
            );

            // Save previous state BEFORE making changes
            window.previous_window_state = Some(window.current_window_state.clone());

            // Update mouse state
            window.current_window_state.mouse_state.cursor_position =
                CursorPosition::InWindow(logical_pos);
            window.current_window_state.mouse_state.left_down = true;

            // Update hit test
            if let Some(ref layout_window) = window.layout_window {
                use crate::desktop::wr_translate2::fullhittest_new_webrender;

                let hit_tester = window.hit_tester.resolve();
                let hit_test = fullhittest_new_webrender(
                    &*hit_tester,
                    window.document_id,
                    window.current_window_state.focused_node,
                    &layout_window.layout_results,
                    &CursorPosition::InWindow(logical_pos),
                    hidpi_factor,
                );

                window.current_window_state.last_hit_test = hit_test;
            }

            // Capture mouse
            (window.win32.user32.SetCapture)(hwnd);

            // V2 system will detect MouseDown event
            let result = window.process_window_events_v2();

            // Request redraw if needed
            if !matches!(result, process::ProcessEventResult::DoNothing) {
                (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
            }

            0
        }

        WM_LBUTTONUP => {
            // Left mouse button up - similar to macOS handle_mouse_up
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;

            use azul_core::{geom::LogicalPosition, window::CursorPosition};

            let hidpi_factor = window.current_window_state.size.get_hidpi_factor();
            let logical_pos = LogicalPosition::new(
                x as f32 / hidpi_factor.inner.get(),
                y as f32 / hidpi_factor.inner.get(),
            );

            // Save previous state BEFORE making changes
            window.previous_window_state = Some(window.current_window_state.clone());

            // Update mouse state
            window.current_window_state.mouse_state.cursor_position =
                CursorPosition::InWindow(logical_pos);
            window.current_window_state.mouse_state.left_down = false;

            // Update hit test
            if let Some(ref layout_window) = window.layout_window {
                use crate::desktop::wr_translate2::fullhittest_new_webrender;

                let hit_tester = window.hit_tester.resolve();
                let hit_test = fullhittest_new_webrender(
                    &*hit_tester,
                    window.document_id,
                    window.current_window_state.focused_node,
                    &layout_window.layout_results,
                    &CursorPosition::InWindow(logical_pos),
                    hidpi_factor,
                );

                window.current_window_state.last_hit_test = hit_test;
            }

            // Release mouse capture
            (window.win32.user32.ReleaseCapture)();

            // V2 system will detect MouseUp event
            let result = window.process_window_events_v2();

            // Request redraw if needed
            if !matches!(result, process::ProcessEventResult::DoNothing) {
                (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
            }

            0
        }

        WM_RBUTTONDOWN => {
            // Right mouse button down
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;

            use azul_core::{geom::LogicalPosition, window::CursorPosition};

            let hidpi_factor = window.current_window_state.size.get_hidpi_factor();
            let logical_pos = LogicalPosition::new(
                x as f32 / hidpi_factor.inner.get(),
                y as f32 / hidpi_factor.inner.get(),
            );

            // Save previous state BEFORE making changes
            window.previous_window_state = Some(window.current_window_state.clone());

            // Update mouse state
            window.current_window_state.mouse_state.cursor_position =
                CursorPosition::InWindow(logical_pos);
            window.current_window_state.mouse_state.right_down = true;

            // Update hit test
            if let Some(ref layout_window) = window.layout_window {
                use crate::desktop::wr_translate2::fullhittest_new_webrender;

                let hit_tester = window.hit_tester.resolve();
                let hit_test = fullhittest_new_webrender(
                    &*hit_tester,
                    window.document_id,
                    window.current_window_state.focused_node,
                    &layout_window.layout_results,
                    &CursorPosition::InWindow(logical_pos),
                    hidpi_factor,
                );

                window.current_window_state.last_hit_test = hit_test;
            }

            // V2 system will detect MouseDown event
            let result = window.process_window_events_v2();

            // Request redraw if needed
            if !matches!(result, process::ProcessEventResult::DoNothing) {
                (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
            }

            0
        }

        WM_RBUTTONUP => {
            // Right mouse button up - with context menu support
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;

            use azul_core::{geom::LogicalPosition, window::CursorPosition};

            let hidpi_factor = window.current_window_state.size.get_hidpi_factor();
            let logical_pos = LogicalPosition::new(
                x as f32 / hidpi_factor.inner.get(),
                y as f32 / hidpi_factor.inner.get(),
            );

            // Save previous state BEFORE making changes
            window.previous_window_state = Some(window.current_window_state.clone());

            // Update mouse state
            window.current_window_state.mouse_state.cursor_position =
                CursorPosition::InWindow(logical_pos);
            window.current_window_state.mouse_state.right_down = false;

            // Update hit test
            if let Some(ref layout_window) = window.layout_window {
                use crate::desktop::wr_translate2::fullhittest_new_webrender;

                let hit_tester = window.hit_tester.resolve();
                let hit_test = fullhittest_new_webrender(
                    &*hit_tester,
                    window.document_id,
                    window.current_window_state.focused_node,
                    &layout_window.layout_results,
                    &CursorPosition::InWindow(logical_pos),
                    hidpi_factor,
                );

                window.current_window_state.last_hit_test = hit_test;
            }

            // Show context menu if available
            // TODO: Context menu needs to be extracted from window state/callbacks
            /*
            if let Some(context_menu) = &window.context_menu {
                // Convert to screen coordinates
                let mut pt = event::POINT { x, y };
                unsafe { (window.win32.user32.ClientToScreen)(hwnd, &mut pt as *mut _) };
                menu::create_and_show_context_menu(hwnd, context_menu, pt.x, pt.y, &window.win32);
            }
            */

            // V2 system will detect MouseUp event
            let result = window.process_window_events_v2();

            // Request redraw if needed
            if !matches!(result, process::ProcessEventResult::DoNothing) {
                (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
            }

            0
        }

        WM_MBUTTONDOWN => {
            // Middle mouse button down
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;

            use azul_core::{geom::LogicalPosition, window::CursorPosition};

            let dpi = window.current_window_state.size.dpi;
            let hidpi_factor = dpi as f32 / 96.0;
            let logical_pos =
                LogicalPosition::new(x as f32 / hidpi_factor, y as f32 / hidpi_factor);

            // Save previous state
            window.previous_window_state = Some(window.current_window_state.clone());

            // Update mouse state
            window.current_window_state.mouse_state.cursor_position =
                CursorPosition::InWindow(logical_pos);
            window.current_window_state.mouse_state.middle_down = true;

            // V2 system will detect MouseDown event
            let result = window.process_window_events_v2();

            if !matches!(result, process::ProcessEventResult::DoNothing) {
                (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
            }

            0
        }

        WM_MBUTTONUP => {
            // Middle mouse button up
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;

            use azul_core::{geom::LogicalPosition, window::CursorPosition};

            let dpi = window.current_window_state.size.dpi;
            let hidpi_factor = dpi as f32 / 96.0;
            let logical_pos =
                LogicalPosition::new(x as f32 / hidpi_factor, y as f32 / hidpi_factor);

            // Save previous state
            window.previous_window_state = Some(window.current_window_state.clone());

            // Update mouse state
            window.current_window_state.mouse_state.cursor_position =
                CursorPosition::InWindow(logical_pos);
            window.current_window_state.mouse_state.middle_down = false;

            // V2 system will detect MouseUp event
            let result = window.process_window_events_v2();

            if !matches!(result, process::ProcessEventResult::DoNothing) {
                (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
            }

            0
        }

        WM_MOUSEWHEEL => {
            // Mouse wheel scrolled - similar to macOS handle_scroll_wheel
            let delta = ((wparam >> 16) & 0xFFFF) as i16 as i32;
            let scroll_amount = -(delta as f32 / WHEEL_DELTA as f32); // Invert for natural scrolling

            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;

            use azul_core::{geom::LogicalPosition, window::CursorPosition};

            let hidpi_factor = window.current_window_state.size.get_hidpi_factor();
            let logical_pos = LogicalPosition::new(
                x as f32 / hidpi_factor.inner.get(),
                y as f32 / hidpi_factor.inner.get(),
            );

            // Save previous state
            window.previous_window_state = Some(window.current_window_state.clone());

            // Update scroll state
            use azul_css::OptionF32;
            let current_y = window
                .current_window_state
                .mouse_state
                .scroll_y
                .into_option()
                .unwrap_or(0.0);

            window.current_window_state.mouse_state.scroll_y =
                OptionF32::Some(current_y + scroll_amount);

            // Update hit test
            if let Some(ref layout_window) = window.layout_window {
                use crate::desktop::wr_translate2::fullhittest_new_webrender;

                let hit_tester = window.hit_tester.resolve();
                let hit_test = fullhittest_new_webrender(
                    &*hit_tester,
                    window.document_id,
                    window.current_window_state.focused_node,
                    &layout_window.layout_results,
                    &CursorPosition::InWindow(logical_pos),
                    hidpi_factor,
                );

                // GPU scroll for hovered node (before moving hit_test)
                if delta.abs() > 0 {
                    if let Some(first_hovered) = hit_test.hovered_nodes.iter().next() {
                        let (dom_id, ht) = first_hovered;
                        if let Some(node_id) = ht.regular_hit_test_nodes.keys().next() {
                            let _ = window.gpu_scroll(
                                dom_id.inner as u64,
                                node_id.index() as u64,
                                0.0,
                                -scroll_amount * 20.0, // Scale for pixel scrolling
                            );
                        }
                    }
                }

                window.current_window_state.last_hit_test = hit_test;
            }

            // V2 system will detect Scroll event
            let result = window.process_window_events_v2();

            if !matches!(result, process::ProcessEventResult::DoNothing) {
                (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
            }

            0
        }

        WM_KEYDOWN | WM_SYSKEYDOWN => {
            // Key pressed - similar to macOS handle_key_down
            let vk_code = wparam as u32;
            let scan_code = ((lparam >> 16) & 0xFF) as u32;
            let _repeat_count = (lparam & 0xFFFF) as u16;

            // Translate virtual key to azul key
            if let Some(virtual_key) = event::vkey_to_winit_vkey(vk_code as i32) {
                // Save previous state
                window.previous_window_state = Some(window.current_window_state.clone());

                // Update keyboard state
                window
                    .current_window_state
                    .keyboard_state
                    .current_virtual_keycode =
                    azul_core::window::OptionVirtualKeyCode::Some(virtual_key);
                window
                    .current_window_state
                    .keyboard_state
                    .pressed_virtual_keycodes
                    .insert_hm_item(virtual_key);
                window
                    .current_window_state
                    .keyboard_state
                    .pressed_scancodes
                    .insert_hm_item(scan_code);

                // V2 system will detect VirtualKeyDown event
                let result = window.process_window_events_v2();

                if !matches!(result, process::ProcessEventResult::DoNothing) {
                    (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
                }
            }

            0
        }

        WM_KEYUP | WM_SYSKEYUP => {
            // Key released - similar to macOS handle_key_up
            let vk_code = wparam as u32;
            let scan_code = ((lparam >> 16) & 0xFF) as u32;

            // Translate virtual key
            if let Some(virtual_key) = event::vkey_to_winit_vkey(vk_code as i32) {
                // Save previous state
                window.previous_window_state = Some(window.current_window_state.clone());

                // Update keyboard state
                window
                    .current_window_state
                    .keyboard_state
                    .current_virtual_keycode = azul_core::window::OptionVirtualKeyCode::None;
                window
                    .current_window_state
                    .keyboard_state
                    .pressed_virtual_keycodes
                    .remove_hm_item(&virtual_key);
                window
                    .current_window_state
                    .keyboard_state
                    .pressed_scancodes
                    .remove_hm_item(&scan_code);

                // V2 system will detect VirtualKeyUp event
                let result = window.process_window_events_v2();

                if !matches!(result, process::ProcessEventResult::DoNothing) {
                    (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
                }
            }

            0
        }

        WM_CHAR | WM_SYSCHAR => {
            // Character input - for text input
            let char_code = wparam as u32;

            // Handle UTF-16 surrogate pairs
            let is_high_surrogate = 0xD800 <= char_code && char_code <= 0xDBFF;
            let is_low_surrogate = 0xDC00 <= char_code && char_code <= 0xDFFF;

            let mut char_opt = None;
            if is_high_surrogate {
                window.high_surrogate = Some(char_code as u16);
            } else if is_low_surrogate {
                if let Some(high) = window.high_surrogate {
                    // Decode UTF-16 surrogate pair
                    let pair = [high, char_code as u16];
                    if let Some(Ok(chr)) = char::decode_utf16(pair.iter().copied()).next() {
                        char_opt = Some(chr);
                    }
                }
                window.high_surrogate = None;
            } else {
                window.high_surrogate = None;
                if let Some(chr) = char::from_u32(char_code) {
                    if !chr.is_control() {
                        char_opt = Some(chr);
                    }
                }
            }

            // Update keyboard state with character
            if let Some(chr) = char_opt {
                window.previous_window_state = Some(window.current_window_state.clone());
                window.current_window_state.keyboard_state.current_char =
                    azul_core::window::OptionChar::Some(chr as u32);

                // V2 system will detect TextInput event
                let result = window.process_window_events_v2();

                // Clear character after processing
                window.current_window_state.keyboard_state.current_char =
                    azul_core::window::OptionChar::None;

                if !matches!(result, process::ProcessEventResult::DoNothing) {
                    (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
                }
            }

            0
        }

        WM_SETFOCUS => {
            // Window gained focus
            window.previous_window_state = Some(window.current_window_state.clone());
            window.current_window_state.flags.has_focus = true;
            window.current_window_state.window_focused = true;

            0
        }

        WM_KILLFOCUS => {
            // Window lost focus
            window.previous_window_state = Some(window.current_window_state.clone());
            window.current_window_state.flags.has_focus = false;
            window.current_window_state.window_focused = false;

            0
        }

        WM_TIMER => {
            // Timer fired
            let timer_id = wparam;

            if timer_id == 0xFFFF {
                // Thread polling timer
                // Poll thread messages via LayoutWindow
                // TODO: This would normally call process::process_threads()
                // but that requires access to higher-level state (fc_cache, etc.)
                // For now, just acknowledge the timer
            } else {
                // User timer from LayoutWindow
                // Tick timers in the layout window
                if let Some(ref mut layout_window) = window.layout_window {
                    let system_callbacks =
                        azul_layout::callbacks::ExternalSystemCallbacks::rust_internal();
                    let current_time = (system_callbacks.get_system_time_fn.cb)();

                    let expired_timers = layout_window.tick_timers(current_time);

                    // TODO: Timer callbacks would be invoked via process::process_timer()
                    // which requires higher-level context (fc_cache, image_cache, etc.)
                    // Mark that we need to process these timers
                    if !expired_timers.is_empty() {
                        // Request redraw to process callbacks
                        (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
                    }
                }
            }

            0
        }

        WM_COMMAND => {
            // Menu command
            let command_id = (wparam & 0xFFFF) as u16;

            // Look up menu callback and invoke it via LayoutWindow
            if let Some(menu_bar) = &window.menu_bar {
                if let Some(ref mut layout_window) = window.layout_window {
                    // TODO: Menu callbacks are invoked via layout_window.invoke_menu_callback()
                    // This requires access to app data and other context
                    // For now, just mark that a menu was selected

                    // TODO: In a full implementation, this would call:
                    // layout_window.invoke_menu_callback(command_id, ...)

                    // Request redraw
                    (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
                }
            }

            (window.win32.user32.DefWindowProcW)(hwnd, msg, wparam, lparam)
        }

        WM_MOUSELEAVE => {
            // Mouse left window
            if let Some(ref mut layout_window) = window.layout_window {
                // TODO: Call layout_window.handle_mouse_leave() when API available
            }
            0
        }

        WM_DPICHANGED => {
            // DPI changed
            let new_dpi = (wparam >> 16) & 0xFFFF;

            // Get suggested size from lParam (RECT*)
            if lparam != 0 {
                let rect = lparam as *const dlopen::RECT;
                let width = (*rect).right - (*rect).left;
                let height = (*rect).bottom - (*rect).top;

                // Update window size to suggested dimensions
                (window.win32.user32.SetWindowPos)(
                    hwnd,
                    ptr::null_mut(),
                    (*rect).left,
                    (*rect).top,
                    width,
                    height,
                    0x0004 | 0x0002, // SWP_NOZORDER | SWP_NOACTIVATE
                );
            }

            // Update DPI in LayoutWindow
            if let Some(ref mut layout_window) = window.layout_window {
                // TODO: Call layout_window.set_dpi(new_dpi) when API available
            }

            0
        }

        _ => {
            // Unknown message, use default processing
            (window.win32.user32.DefWindowProcW)(hwnd, msg, wparam, lparam)
        }
    }
}

// Helper functions for string encoding

/// Encode a string as null-terminated ASCII bytes
fn encode_ascii(s: &str) -> Vec<u8> {
    let mut bytes = s.as_bytes().to_vec();
    bytes.push(0);
    bytes
}

/// Encode a string as null-terminated UTF-16 (wide) bytes
fn encode_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Load a DLL by name, returns None if loading fails
fn load_dll(name: &str) -> Option<HINSTANCE> {
    use winapi::um::libloaderapi::LoadLibraryA;

    let mut dll_name = encode_ascii(name);
    let handle = unsafe { LoadLibraryA(dll_name.as_mut_ptr() as *const i8) };

    if handle.is_null() {
        None
    } else {
        Some(handle as *mut c_void)
    }
}

/// Returns a default PIXELFORMATDESCRIPTOR for OpenGL context creation
fn get_default_pfd() -> winapi::um::wingdi::PIXELFORMATDESCRIPTOR {
    use winapi::um::wingdi::*;

    winapi::um::wingdi::PIXELFORMATDESCRIPTOR {
        nSize: std::mem::size_of::<PIXELFORMATDESCRIPTOR>() as u16,
        nVersion: 1,
        dwFlags: PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL | PFD_DOUBLEBUFFER,
        iPixelType: PFD_TYPE_RGBA,
        cColorBits: 32,
        cRedBits: 0,
        cRedShift: 0,
        cGreenBits: 0,
        cGreenShift: 0,
        cBlueBits: 0,
        cBlueShift: 0,
        cAlphaBits: 8,
        cAlphaShift: 0,
        cAccumBits: 0,
        cAccumRedBits: 0,
        cAccumGreenBits: 0,
        cAccumBlueBits: 0,
        cAccumAlphaBits: 0,
        cDepthBits: 24,
        cStencilBits: 8,
        cAuxBuffers: 0,
        iLayerType: PFD_MAIN_PLANE,
        bReserved: 0,
        dwLayerMask: 0,
        dwVisibleMask: 0,
        dwDamageMask: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_win32window_struct_size() {
        // Just ensure the struct compiles
        let size = std::mem::size_of::<Win32Window>();
        assert!(size > 0);
    }
}
