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

use crate::{log_debug, log_error, log_info, log_warn, log_trace};
use crate::desktop::shell2::common::debug_server::LogCategory;

pub mod accessibility;
pub mod clipboard;
pub mod dlopen;
mod dpi;
pub mod event;
mod gl;
pub mod menu;
pub mod registry;
mod tooltip;
mod wcreate;

use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    ffi::c_void,
    ptr,
    rc::Rc,
    sync::Arc,
};

use azul_core::{
    dom::{DomId, NodeId},
    events::ProcessEventResult,
    geom::LogicalPosition,
    gl::OptionGlContextPtr,
    hit_test::{DocumentId, PipelineId},
    menu::CoreMenuCallback,
    refany::RefAny,
    resources::{DpiScaleFactor, IdNamespace, ImageCache, RendererResources},
    window::{
        Monitor, OptionMouseCursorType, RawWindowHandle, RendererType, WindowFrame, WindowsHandle,
    },
};
use azul_css::corety::OptionU32;
use azul_layout::{
    hit_test::FullHitTest,
    managers::hover::InputPointId,
    window::LayoutWindow,
    window_state::{FullWindowState, WindowCreateOptions},
    ScrollbarDragState,
};
use rust_fontconfig::FcFontCache;
use webrender::{RenderApi as WrRenderApi, Renderer as WrRenderer, Transaction as WrTransaction};

use self::{
    dlopen::{DynamicLibrary, HDC, HGLRC, HINSTANCE, HMENU, HWND},
    dpi::DpiFunctions,
    gl::GlFunctions,
};
use crate::desktop::{
    shell2::common::{
        event_v2::{self, PlatformWindowV2},
        Compositor, WindowError, WindowProperties,
    },
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
    /// Device context for OpenGL (must stay valid for the lifetime of the GL context)
    pub hdc: *mut std::ffi::c_void,

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
    /// Signal from WebRender that a new frame is ready
    pub new_frame_ready: Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,

    // Win32 libraries
    /// Dynamically loaded Win32 libraries
    pub win32: dlopen::Win32Libraries,

    // Window state
    /// Window is open flag
    pub is_open: bool,
    /// Whether the first frame has been shown (for deferred window visibility)
    pub first_frame_shown: bool,
    /// Flag indicating frame needs regeneration in next WM_PAINT
    pub frame_needs_regeneration: bool,
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
    /// IME composition string (for preview during typing)
    pub ime_composition: Option<String>,
    /// Last hovered node (for hover state tracking)
    pub last_hovered_node: Option<event_v2::HitTestNode>,
    /// Scrollbar drag state (for drag interactions)
    pub scrollbar_drag_state: Option<azul_layout::ScrollbarDragState>,

    // System functions
    /// DPI functions
    pub dpi: DpiFunctions,

    // Shared resources
    /// Shared application data (used by callbacks, shared across windows)
    pub app_data: Arc<RefCell<RefAny>>,
    /// Font cache (shared across all windows)
    pub fc_cache: Arc<FcFontCache>,
    /// System style (shared across all windows)
    pub system_style: Arc<azul_css::system::SystemStyle>,
    /// Dynamic selector context for evaluating conditional CSS properties
    /// (viewport size, OS, theme, etc.) - updated on resize and theme change
    pub dynamic_selector_context: azul_css::dynamic_selector::DynamicSelectorContext,

    // Multi-window support
    /// Pending window creation requests (for popup menus, dialogs, etc.)
    /// Processed in Phase 3 of the event loop
    pub pending_window_creates: Vec<WindowCreateOptions>,

    // Tooltip
    /// Tooltip window (for programmatic tooltip display)
    pub tooltip: Option<tooltip::TooltipWindow>,

    // Accessibility
    /// Windows accessibility adapter
    #[cfg(feature = "a11y")]
    pub accessibility_adapter: accessibility::WindowsAccessibilityAdapter,
}

impl Win32Window {
    /// Create a new Win32 window with given options
    pub fn new(
        options: WindowCreateOptions,
        fc_cache: Arc<FcFontCache>,
        app_data: Arc<std::cell::RefCell<RefAny>>,
    ) -> Result<Self, WindowError> {
        let total_start = std::time::Instant::now();
        let mut step_start = std::time::Instant::now();
        
        macro_rules! timing_log {
            ($step:expr) => {{
                let elapsed = step_start.elapsed();
                log_debug!(LogCategory::Window, "[Win32] {} took {:?}", $step, elapsed);
                step_start = std::time::Instant::now();
            }};
        }
        
        log_trace!(LogCategory::Window, "[Win32] Win32Window::new() called");
        // Load Win32 libraries
        let win32 = dlopen::Win32Libraries::load().map_err(|e| {
            log_error!(LogCategory::Platform, "[Win32] Failed to load Win32 libraries: {}", e);
            WindowError::PlatformError(format!("Failed to load Win32 libraries: {}", e))
        })?;
        timing_log!("Load Win32 libraries");

        // Get HINSTANCE from GetModuleHandleW(NULL)
        log_trace!(LogCategory::Window, "[Win32] getting HINSTANCE");
        let hinstance = if let Some(ref k32) = win32.kernel32 {
            unsafe { (k32.GetModuleHandleW)(ptr::null()) }
        } else {
            log_error!(LogCategory::Platform, "[Win32] kernel32.dll not available");
            return Err(WindowError::PlatformError(
                "kernel32.dll not available".into(),
            ));
        };
        timing_log!("Get HINSTANCE");

        if hinstance.is_null() {
            log_error!(LogCategory::Platform, "[Win32] Failed to get HINSTANCE");
            return Err(WindowError::PlatformError("Failed to get HINSTANCE".into()));
        }

        // Initialize DPI awareness
        let dpi_functions = DpiFunctions::init();
        dpi_functions.become_dpi_aware();
        timing_log!("DPI awareness init");

        // Register window class with our window procedure
        wcreate::register_window_class(hinstance, Some(window_proc), &win32)?;
        timing_log!("Register window class");

        // Create HWND (invisible initially to avoid black flash)
        let hwnd = wcreate::create_hwnd(
            hinstance,
            &options,
            None,            // No parent window
            ptr::null_mut(), // User data will be set later
            &win32,
        )?;
        timing_log!("Create HWND");

        // Get DPI for window
        let dpi = unsafe { dpi_functions.hwnd_dpi(hwnd as _) };
        let dpi_factor = dpi::dpi_to_scale_factor(dpi);
        timing_log!("Get window DPI");

        // Update options with actual DPI
        let mut options = options;
        options.window_state.size.dpi = dpi;

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

        // We need to keep the HDC alive for the GL context - store it for later
        let mut active_hdc: *mut std::ffi::c_void = ptr::null_mut();
        
        if should_use_hardware {
            let vsync = options.window_state.renderer_options.vsync;
            match wcreate::create_gl_context(hwnd, hinstance, &win32, vsync) {
                Ok(hglrc) => {
                    gl_context = Some(hglrc);
                    let hdc = unsafe { (win32.user32.GetDC)(hwnd) };
                    if !hdc.is_null() {
                        log_trace!(LogCategory::Rendering, "[Win32] activating GL context for WebRender init");
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
                        // IMPORTANT: Keep the GL context ACTIVE and HDC valid for WebRender initialization!
                        // We do NOT call wglMakeCurrent(null, null) or ReleaseDC here.
                        // The context must be current when webrender::create_webrender_instance is called.
                        active_hdc = hdc;
                    }
                }
                Err(e) => {
                    // Fall back to software rendering
                    log_warn!(LogCategory::Rendering, "[Win32] GL context creation failed: {:?}, falling back to software", e);
                    gl_context_ptr = OptionGlContextPtr::None;
                }
            }
        }
        timing_log!("Create GL context");

        // Initialize WebRender (GL context must be active!)
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
        timing_log!("Create WebRender instance");

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
        options.window_state.size.dimensions = physical_size.to_logical(dpi_factor);

        // Determine renderer type
        let renderer_type = if gl_context.is_some() {
            RendererType::Hardware
        } else {
            RendererType::Software
        };

        // Extract create_callback before cloning (will be invoked after window is ready)
        let create_callback = options.create_callback.clone();

        // Create initial window state
        let initial_window_state = options.window_state.clone();

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
            monitor_id: OptionU32::None, // Monitor ID will be detected from platform
            window_id: initial_window_state.window_id.clone(),
            window_focused: true,
        };

        // Set document_id and id_namespace for this window
        layout_window.document_id = document_id;
        layout_window.id_namespace = id_namespace;
        layout_window.current_window_state = current_window_state.clone();
        layout_window.renderer_type = Some(renderer_type);
        timing_log!("Create LayoutWindow");

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

        // IMPORTANT: Do NOT show window yet! 
        // AccessKit's SubclassingAdapter requires the window to be invisible when initialized.
        // We'll show the window AFTER a11y is set up.
        let should_show_window = layout_window.current_window_state.flags.is_visible;
        let window_frame = layout_window.current_window_state.flags.frame;
        log_trace!(LogCategory::Window, "[Win32] deferring show_window until after a11y init (is_visible: {})", should_show_window);

        // Position window on requested monitor (or center on primary)
        // This can be done before showing
        // TODO: Use monitor_id to look up actual Monitor from global state
        position_window_on_monitor(
            hwnd,
            Monitor::default().monitor_id,
            current_window_state.position,
            current_window_state.size,
            &win32,
        );
        timing_log!("Position window");

        // Enable drag-and-drop if shell32.dll is available
        if let Some(ref shell32) = win32.shell32 {
            unsafe {
                (shell32.DragAcceptFiles)(hwnd, 1); // 1 = TRUE (enable drag-drop)
            }
        }

        // Get current window state
        let current_window_state = layout_window.current_window_state.clone();

        // Create dynamic selector context before building window
        let initial_viewport_width = current_window_state.size.dimensions.width;
        let initial_viewport_height = current_window_state.size.dimensions.height;
        let dynamic_selector_context = {
            let sys = azul_css::system::SystemStyle::new();
            let mut ctx = azul_css::dynamic_selector::DynamicSelectorContext::from_system_style(&sys);
            ctx.viewport_width = initial_viewport_width;
            ctx.viewport_height = initial_viewport_height;
            ctx.orientation = if initial_viewport_width > initial_viewport_height {
                azul_css::dynamic_selector::OrientationType::Landscape
            } else {
                azul_css::dynamic_selector::OrientationType::Portrait
            };
            ctx
        };

        // Build window structure
        let mut result = Win32Window {
            hwnd,
            hinstance,
            hdc: active_hdc, // Keep HDC alive for OpenGL rendering
            layout_window: Some(layout_window),
            gl_context,
            gl_functions,
            gl_context_ptr,
            renderer: Some(renderer),
            render_api,
            hit_tester,
            document_id,
            id_namespace,
            new_frame_ready,
            win32, // Store Win32 libraries for later use
            is_open: true,
            first_frame_shown: false, // Window will be shown after first SwapBuffers
            frame_needs_regeneration: true, // Initial render deferred to WM_PAINT
            previous_window_state: None,
            current_window_state,
            image_cache: ImageCache::default(),
            renderer_resources: RendererResources::default(),
            menu_bar,
            context_menu: None,
            timers: HashMap::new(),
            thread_timer_running: None,
            high_surrogate: None,
            ime_composition: None,
            last_hovered_node: None,
            scrollbar_drag_state: None,
            dpi: dpi_functions,
            app_data,
            fc_cache,
            system_style: Arc::new(azul_css::system::SystemStyle::new()),
            dynamic_selector_context,
            pending_window_creates: Vec::new(),
            tooltip: None, // Created lazily when first needed
            #[cfg(feature = "a11y")]
            accessibility_adapter: accessibility::WindowsAccessibilityAdapter::new(),
        };
        timing_log!("Build Win32Window struct");

        // Initialize accessibility adapter BEFORE showing the window
        // AccessKit's SubclassingAdapter requires the window to be invisible when initialized
        #[cfg(feature = "a11y")]
        {
            if let Err(e) = result.accessibility_adapter.initialize(hwnd) {
                // Don't fail window creation if a11y fails, just log and continue
                log_warn!(LogCategory::Platform, "[Win32] a11y adapter init failed: {}, continuing without a11y", e);
            }
        }
        timing_log!("Initialize accessibility adapter");

        // Render FIRST FRAME before showing window to avoid black flash
        // This ensures the window has content when it becomes visible
        // NOTE: We do NOT show the window here! The window will be shown by run.rs
        // after this function returns and after waiting for new_frame_ready signal.
        {
            result.frame_needs_regeneration = true;
            let _ = result.render_and_present();
        }
        timing_log!("Render first frame (async - not waiting for completion)");

        // Store visibility flags for run.rs to use when showing the window
        // The window will be shown by run.rs after waiting for new_frame_ready
        // DO NOT call show_window_with_frame here!
        timing_log!("Skip show window (will be shown by run.rs after first frame ready)");

        // Invoke create_callback if provided (for GL resource upload, config loading, etc.)
        // This runs AFTER GL context is ready but BEFORE any layout is done
        if let Some(mut callback) = create_callback.into_option() {
            use azul_core::window::RawWindowHandle;
            
            let raw_handle = RawWindowHandle::Windows(azul_core::window::WindowsHandle {
                hwnd: hwnd as *mut _,
                hinstance: hinstance as *mut _,
            });
            
            // Get mutable references needed for invoke_single_callback
            let layout_window = result.layout_window.as_mut()
                .expect("LayoutWindow should exist at this point");
            let mut fc_cache_clone = (*result.fc_cache).clone();
            
            // Get app_data for callback
            let mut app_data_ref = result.app_data.borrow_mut();
            
            let callback_result = layout_window.invoke_single_callback(
                &mut callback,
                &mut *app_data_ref,
                &raw_handle,
                &result.gl_context_ptr,
                &mut result.image_cache,
                &mut fc_cache_clone,
                result.system_style.clone(),
                &azul_layout::callbacks::ExternalSystemCallbacks::rust_internal(),
                &result.previous_window_state,
                &result.current_window_state,
                &result.renderer_resources,
            );
            
            // Process callback result (timers, threads, etc.)
            drop(app_data_ref); // Release borrow before process_callback_result_v2
            use crate::desktop::shell2::common::event_v2::PlatformWindowV2;
            let _ = result.process_callback_result_v2(&callback_result);
        }

        // Register debug timer if AZUL_DEBUG is enabled
        #[cfg(feature = "std")]
        if crate::desktop::shell2::common::debug_server::is_debug_enabled() {
            use azul_core::task::TimerId;
            use azul_layout::callbacks::ExternalSystemCallbacks;
            
            let timer_id: usize = 0xDEBE; // Special debug timer ID
            let debug_timer = crate::desktop::shell2::common::debug_server::create_debug_timer(
                ExternalSystemCallbacks::rust_internal().get_system_time_fn
            );
            
            // Insert into layout_window
            if let Some(layout_window) = result.layout_window.as_mut() {
                layout_window.timers.insert(TimerId { id: timer_id }, debug_timer.clone());
            }
            
            // Also create native Win32 timer
            let interval_ms = debug_timer.tick_millis().min(u32::MAX as u64) as u32;
            let native_timer_id = unsafe { 
                (result.win32.user32.SetTimer)(result.hwnd, timer_id, interval_ms, ptr::null()) 
            };
            result.timers.insert(timer_id, native_timer_id);
            log_debug!(LogCategory::Timer, "Debug timer registered with ID 0x{:X}, interval {}ms", timer_id, interval_ms);
        }
        timing_log!("Final setup (callback + debug timer)");

        log_debug!(LogCategory::Window, "[Win32] ===== TOTAL Win32Window::new() took {:?} =====", total_start.elapsed());
        Ok(result)
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

        // Use the stored HDC that was used to create the GL context
        // IMPORTANT: The GL context is bound to a specific HDC, so we must use the same one!
        unsafe {
            // If we have a stored HDC (from GL context creation), use it
            // Otherwise get a new one (software rendering path)
            let hdc = if !self.hdc.is_null() {
                self.hdc
            } else {
                let new_hdc = (self.win32.user32.GetDC)(self.hwnd);
                if new_hdc.is_null() {
                    return Err(WindowError::PlatformError("Failed to get HDC".into()));
                }
                new_hdc
            };

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
                    // glFinish() ensures all GPU commands complete before SwapBuffers
                    // This is crucial for the first frame to avoid black flash
                    if let Some(gl) = self.gl_context_ptr.as_ref() {
                        gl.finish();
                    }
                    
                    use winapi::um::wingdi::SwapBuffers;
                    SwapBuffers(hdc as winapi::shared::windef::HDC);
                }
            }

            // Show window after FIRST successful render + SwapBuffers
            // renderer.render() is synchronous, so if we get here, the frame was rendered.
            // We trust that after SwapBuffers, pixels are on screen.
            if !self.first_frame_shown {
                // Check if user wants the window visible
                if self.current_window_state.flags.is_visible {
                    log_trace!(LogCategory::Rendering, "[Win32] First frame rendered + SwapBuffers done - showing window NOW");
                    
                    // Force DWM to latch the new frame buffer before making the window visible.
                    // This prevents the "Black Frame" flash by blocking until DWM composition is done.
                    if let Some(ref dwmapi) = self.win32.dwmapi_funcs {
                        (dwmapi.DwmFlush)();
                        log_trace!(LogCategory::Rendering, "[Win32] DwmFlush completed");
                    }
                    
                    use dlopen::constants::SW_SHOW;
                    (self.win32.user32.ShowWindow)(self.hwnd, SW_SHOW);
                    (self.win32.user32.UpdateWindow)(self.hwnd);
                    log_trace!(LogCategory::Rendering, "[Win32] Window shown after first real frame");
                }
                self.first_frame_shown = true;
            }

            // Only release DC if we obtained a new one (not using stored HDC)
            // The stored HDC must stay valid for the lifetime of the GL context!
            if self.hdc.is_null() {
                (self.win32.user32.ReleaseDC)(self.hwnd, hdc);
            }

            // CI testing: Exit successfully after first frame render if env var is set
            if std::env::var("AZUL_EXIT_SUCCESS_AFTER_FRAME_RENDER").is_ok() {
                log_info!(LogCategory::General, "AZUL_EXIT_SUCCESS_AFTER_FRAME_RENDER set - exiting with success");
                std::process::exit(0);
            }

            Ok(())
        }
    }

    /// Regenerate layout (called after DOM changes)
    pub fn regenerate_layout(&mut self) -> Result<(), String> {
        let layout_window = self.layout_window.as_mut().ok_or("No layout window")?;

        // Collect debug messages if debug server is enabled
        let debug_enabled = crate::desktop::shell2::common::debug_server::is_debug_enabled();
        let mut debug_messages = if debug_enabled { Some(Vec::new()) } else { None };

        // Call unified regenerate_layout from common module
        crate::desktop::shell2::common::layout_v2::regenerate_layout(
            layout_window,
            &self.app_data,
            &self.current_window_state,
            &mut self.renderer_resources,
            &mut self.render_api,
            &self.image_cache,
            &self.gl_context_ptr,
            &self.fc_cache,
            &self.system_style,
            self.document_id,
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
        if let Some(layout_window) = self.layout_window.as_ref() {
            if let Some(tree_update) = layout_window.a11y_manager.last_tree_update.clone() {
                self.accessibility_adapter.update_tree(tree_update);
            }
        }

        // Send frame immediately (Windows doesn't batch like macOS/X11)
        let layout_window = self.layout_window.as_mut().unwrap();
        crate::desktop::shell2::common::layout_v2::generate_frame(
            layout_window,
            &mut self.render_api,
            self.document_id,
        );
        self.render_api.flush_scene_builder();

        // Phase 2: Post-Layout callback - sync IME position after layout (MOST IMPORTANT)
        self.update_ime_position_from_cursor();
        self.sync_ime_position_to_os();

        Ok(())
    }

    /// Update ime_position in window state from focused text cursor
    /// Called after layout to ensure IME window appears at correct position
    fn update_ime_position_from_cursor(&mut self) {
        use azul_core::window::ImePosition;

        if let Some(layout_window) = &self.layout_window {
            if let Some(cursor_rect) = layout_window.get_focused_cursor_rect_viewport() {
                // Successfully calculated cursor position from text layout
                self.current_window_state.ime_position = ImePosition::Initialized(cursor_rect);
            }
        }
    }

    /// Synchronize window state with Windows OS
    ///
    /// Applies changes from current_window_state to the OS window.
    /// Called after callbacks have potentially modified window state.
    fn sync_window_state(&mut self) {
        use std::{ffi::OsStr, os::windows::ffi::OsStrExt};

        // Get copies of previous and current state to avoid borrow checker issues
        let (previous, current) = match &self.previous_window_state {
            Some(prev) => (prev.clone(), self.current_window_state.clone()),
            None => return, // First frame, nothing to sync
        };

        // Title changed?
        if previous.title != current.title {
            let wide_title: Vec<u16> = OsStr::new(current.title.as_str())
                .encode_wide()
                .chain(Some(0))
                .collect();
            unsafe {
                (self.win32.user32.SetWindowTextW)(self.hwnd, wide_title.as_ptr());
            }
        }

        // Size changed?
        if previous.size.dimensions != current.size.dimensions {
            let width = current.size.dimensions.width as i32;
            let height = current.size.dimensions.height as i32;
            unsafe {
                use dlopen::constants::{SWP_NOMOVE, SWP_NOZORDER};
                (self.win32.user32.SetWindowPos)(
                    self.hwnd,
                    std::ptr::null_mut(),
                    0,
                    0,
                    width,
                    height,
                    SWP_NOMOVE | SWP_NOZORDER,
                );
            }
        }

        // Position changed?
        if previous.position != current.position {
            use azul_core::window::WindowPosition;
            match current.position {
                WindowPosition::Initialized(pos) => unsafe {
                    use dlopen::constants::{SWP_NOSIZE, SWP_NOZORDER};
                    (self.win32.user32.SetWindowPos)(
                        self.hwnd,
                        std::ptr::null_mut(),
                        pos.x,
                        pos.y,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOZORDER,
                    );
                },
                WindowPosition::Uninitialized => {}
            }
        }

        // Visibility changed?
        if previous.flags.is_visible != current.flags.is_visible {
            unsafe {
                use dlopen::constants::{SW_HIDE, SW_SHOW};
                if current.flags.is_visible {
                    (self.win32.user32.ShowWindow)(self.hwnd, SW_SHOW);
                } else {
                    (self.win32.user32.ShowWindow)(self.hwnd, SW_HIDE);
                }
            }
        }

        // Window frame state changed? (Minimize/Maximize/Normal)
        if previous.flags.frame != current.flags.frame {
            use azul_core::window::WindowFrame;
            use dlopen::constants::{SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE};
            unsafe {
                match current.flags.frame {
                    WindowFrame::Minimized => {
                        (self.win32.user32.ShowWindow)(self.hwnd, SW_MINIMIZE);
                    }
                    WindowFrame::Maximized => {
                        (self.win32.user32.ShowWindow)(self.hwnd, SW_MAXIMIZE);
                    }
                    WindowFrame::Normal | WindowFrame::Fullscreen => {
                        // Restore from minimized/maximized
                        if previous.flags.frame == WindowFrame::Minimized
                            || previous.flags.frame == WindowFrame::Maximized
                        {
                            (self.win32.user32.ShowWindow)(self.hwnd, SW_RESTORE);
                        }
                    }
                }
            }
        }

        // is_top_level flag changed?
        if previous.flags.is_top_level != current.flags.is_top_level {
            if let Err(e) = self.set_is_top_level(current.flags.is_top_level) {
                log_error!(LogCategory::Window, "Failed to set is_top_level: {}", e);
            }
        }

        // prevent_system_sleep flag changed?
        if previous.flags.prevent_system_sleep != current.flags.prevent_system_sleep {
            if let Err(e) = self.set_prevent_system_sleep(current.flags.prevent_system_sleep) {
                log_error!(LogCategory::Window, "Failed to set prevent_system_sleep: {}", e);
            }
        }

        // Background material changed? (Windows 11 Mica/Acrylic effects)
        if previous.flags.background_material != current.flags.background_material {
            self.apply_background_material(current.flags.background_material);
        }

        // Mouse cursor synchronization - compute from current hit test
        if let Some(layout_window) = self.layout_window.as_ref() {
            if let Some(hit_test) = layout_window
                .hover_manager
                .get_current(&InputPointId::Mouse)
            {
                let cursor_test = layout_window.compute_cursor_type_hit_test(hit_test);
                self.set_cursor(cursor_test.cursor_icon);
            }
        }
    }

    /// Set the mouse cursor for this window
    fn set_cursor(&mut self, cursor_type: azul_core::window::MouseCursorType) {
        use dlopen::constants::*;

        // Map MouseCursorType to Windows cursor constants
        let cursor_id = match cursor_type {
            azul_core::window::MouseCursorType::Default
            | azul_core::window::MouseCursorType::Arrow => IDC_ARROW,
            azul_core::window::MouseCursorType::Crosshair => IDC_CROSS,
            azul_core::window::MouseCursorType::Hand => IDC_HAND,
            azul_core::window::MouseCursorType::Move => IDC_SIZEALL,
            azul_core::window::MouseCursorType::Text => IDC_IBEAM,
            azul_core::window::MouseCursorType::Wait => IDC_WAIT,
            azul_core::window::MouseCursorType::Progress => IDC_APPSTARTING,
            azul_core::window::MouseCursorType::NotAllowed
            | azul_core::window::MouseCursorType::NoDrop => IDC_NO,
            azul_core::window::MouseCursorType::EResize
            | azul_core::window::MouseCursorType::WResize
            | azul_core::window::MouseCursorType::EwResize
            | azul_core::window::MouseCursorType::ColResize => IDC_SIZEWE,
            azul_core::window::MouseCursorType::NResize
            | azul_core::window::MouseCursorType::SResize
            | azul_core::window::MouseCursorType::NsResize
            | azul_core::window::MouseCursorType::RowResize => IDC_SIZENS,
            azul_core::window::MouseCursorType::NeResize
            | azul_core::window::MouseCursorType::SwResize
            | azul_core::window::MouseCursorType::NeswResize => IDC_SIZENESW,
            azul_core::window::MouseCursorType::NwResize
            | azul_core::window::MouseCursorType::SeResize
            | azul_core::window::MouseCursorType::NwseResize => IDC_SIZENWSE,
            azul_core::window::MouseCursorType::Help => IDC_HELP,
            // Fallback to arrow for unsupported cursor types
            _ => IDC_ARROW,
        };

        unsafe {
            let cursor = (self.win32.user32.LoadCursorW)(std::ptr::null_mut(), cursor_id);
            (self.win32.user32.SetCursor)(cursor);
        }
    }

    /// Apply window background material using DWM (Windows 11+)
    ///
    /// This enables Mica, Acrylic, or transparent window effects using the
    /// Desktop Window Manager (DWM) on Windows 11 22H2 and later.
    ///
    /// For `Transparent`, uses DwmEnableBlurBehindWindow with an empty blur region
    /// to achieve true background transparency while keeping rendered content opaque.
    /// This requires an alpha channel in the pixel format and glClearColor(0,0,0,0).
    ///
    /// On older Windows versions, this will gracefully fail (DWM returns error)
    /// and the window will remain opaque.
    fn apply_background_material(&mut self, material: azul_core::window::WindowBackgroundMaterial) {
        use azul_core::window::WindowBackgroundMaterial;
        use dlopen::{DWM_SYSTEMBACKDROP_TYPE, DWMWA_SYSTEMBACKDROP_TYPE, MARGINS, 
                     DWM_BLURBEHIND, DWM_BB_ENABLE, DWM_BB_BLURREGION};

        let dwmapi = match self.win32.dwmapi_funcs.as_ref() {
            Some(d) => d,
            None => {
                log_debug!(LogCategory::Platform, 
                    "[Windows] dwmapi not available, skipping background material");
                return;
            }
        };

        unsafe {
            // For Transparent: use DwmEnableBlurBehindWindow with a minimal blur region
            // This achieves true OpenGL background transparency where:
            // - Background is fully transparent (shows desktop/windows behind)
            // - Rendered content (UI elements) remains opaque
            // Based on: https://stackoverflow.com/a/12290229
            if material == WindowBackgroundMaterial::Transparent {
                // Create a minimal region (0, 0, -1, -1) which effectively disables blur
                // but enables the transparent background compositing
                let hrgn = (self.win32.gdi32.CreateRectRgn)(0, 0, -1, -1);
                
                let bb = DWM_BLURBEHIND {
                    dwFlags: DWM_BB_ENABLE | DWM_BB_BLURREGION,
                    fEnable: 1, // TRUE
                    hRgnBlur: hrgn as *mut core::ffi::c_void,
                    fTransitionOnMaximized: 0,
                };
                
                let result = (dwmapi.DwmEnableBlurBehindWindow)(self.hwnd, &bb);
                
                // Clean up the region handle
                if !hrgn.is_null() {
                    (self.win32.gdi32.DeleteObject)(hrgn as *mut core::ffi::c_void);
                }
                
                if result != 0 {
                    log_debug!(LogCategory::Platform,
                        "[Windows] DwmEnableBlurBehindWindow failed with HRESULT 0x{:08X}",
                        result as u32
                    );
                } else {
                    log_debug!(LogCategory::Platform,
                        "[Windows] Enabled transparent background via DwmEnableBlurBehindWindow");
                }
                return;
            }

            // For Opaque: disable blur-behind
            if material == WindowBackgroundMaterial::Opaque {
                let bb = DWM_BLURBEHIND {
                    dwFlags: DWM_BB_ENABLE,
                    fEnable: 0, // FALSE - disable blur
                    hRgnBlur: std::ptr::null_mut(),
                    fTransitionOnMaximized: 0,
                };
                let _ = (dwmapi.DwmEnableBlurBehindWindow)(self.hwnd, &bb);
                
                // Also reset backdrop type
                let value = DWM_SYSTEMBACKDROP_TYPE::DWMSBT_NONE as i32;
                let _ = (dwmapi.DwmSetWindowAttribute)(
                    self.hwnd,
                    DWMWA_SYSTEMBACKDROP_TYPE,
                    &value as *const _ as *const core::ffi::c_void,
                    std::mem::size_of::<i32>() as u32,
                );
                
                log_debug!(LogCategory::Platform, "[Windows] Disabled transparency effects");
                return;
            }

            // Map remaining WindowBackgroundMaterial values to DWM backdrop type
            // These are Windows 11 22H2+ Mica/Acrylic effects
            let backdrop_type = match material {
                WindowBackgroundMaterial::Sidebar |
                WindowBackgroundMaterial::Menu |
                WindowBackgroundMaterial::HUD => DWM_SYSTEMBACKDROP_TYPE::DWMSBT_TRANSIENTWINDOW, // Acrylic
                WindowBackgroundMaterial::Titlebar => DWM_SYSTEMBACKDROP_TYPE::DWMSBT_MAINWINDOW, // Mica
                WindowBackgroundMaterial::MicaAlt => DWM_SYSTEMBACKDROP_TYPE::DWMSBT_TABBEDWINDOW,
                _ => return, // Already handled above
            };

            // Set the system backdrop type
            let value = backdrop_type as i32;
            let result = (dwmapi.DwmSetWindowAttribute)(
                self.hwnd,
                DWMWA_SYSTEMBACKDROP_TYPE,
                &value as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<i32>() as u32,
            );

            if result != 0 {
                // HRESULT != S_OK - this is expected on Windows 10 or older Windows 11 versions
                log_debug!(LogCategory::Platform,
                    "[Windows] DwmSetWindowAttribute failed with HRESULT 0x{:08X} - \
                     likely Windows 10 or pre-22H2 Windows 11",
                    result as u32
                );
                return;
            }

            // For Mica/Acrylic effects, extend frame into client area
            // This is required for the effect to be visible
            let margins = MARGINS::full_window();
            let extend_result = (dwmapi.DwmExtendFrameIntoClientArea)(self.hwnd, &margins);
            if extend_result != 0 {
                log_warn!(LogCategory::Platform,
                    "[Windows] DwmExtendFrameIntoClientArea failed: 0x{:08X}",
                    extend_result as u32
                );
            }

            log_debug!(LogCategory::Platform,
                "[Windows] Applied background material {:?} (backdrop type {:?})",
                material, backdrop_type
            );
        }
    }

    // Query WebRender hit-tester for scrollbar hits at given position
    //
    // NOTE: perform_scrollbar_hit_test(), handle_scrollbar_click(), and handle_scrollbar_drag()
    // are now provided by the PlatformWindowV2 trait as default methods.
    // The trait methods are cross-platform and work identically.
    // See dll/src/desktop/shell2/common/event_v2.rs for the implementation.
    //
    // Windows-specific note: Mouse capture (SetCapture) is handled in WM_LBUTTONDOWN,
    // and redraw requests (InvalidateRect) are handled by checking ProcessEventResult.

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
            .scroll_manager
            .scroll_node_with_id(dom_id, node_id, scroll_x, scroll_y);
        */

        // Generate frame with updated scroll state
        crate::desktop::shell2::common::layout_v2::generate_frame(
            layout_window,
            &mut self.render_api,
            self.document_id,
        );
        self.render_api.flush_scene_builder();

        Ok(())
    }

    /// Non-blocking event polling for Windows.
    /// Processes one event if available, returns immediately if not.
    pub fn poll_event_internal(&mut self) -> bool {
        // Check if a frame is ready without blocking
        let frame_ready = {
            let &(ref lock, _) = &*self.new_frame_ready;
            let mut ready_guard = lock.lock().unwrap();
            if *ready_guard {
                *ready_guard = false; // Consume the signal
                true
            } else {
                false
            }
        };

        if frame_ready {
            // A frame is ready in WebRender's backbuffer - present it
            if let Err(e) = self.render_and_present() {
                log_error!(LogCategory::Rendering, "Failed to present frame: {:?}", e);
            }
        }

        // Check for close request
        if self.current_window_state.flags.close_requested {
            self.current_window_state.flags.close_requested = false;
            // Close request will be handled by window_proc setting WM_QUIT
            return true;
        }

        // Poll Windows message queue (non-blocking)
        use self::dlopen::{MSG, PM_REMOVE};

        let mut msg: MSG = unsafe { std::mem::zeroed() };
        let has_message = unsafe {
            (self.win32.user32.PeekMessageW)(
                &mut msg, self.hwnd, // Filter for this window only
                0, 0, PM_REMOVE,
            )
        };

        if has_message != 0 {
            // Translate and dispatch message
            // window_proc will be called to handle it
            unsafe {
                (self.win32.user32.TranslateMessage)(&msg);
                (self.win32.user32.DispatchMessageW)(&msg);
            }
            true
        } else {
            false
        }
    }

    /// Try to show context menu at the given screen position
    /// Returns true if a context menu was shown
    fn try_show_context_menu(&mut self, client_x: i32, client_y: i32) -> bool {
        // Get the topmost hovered node from hit test
        let hit_test = self
            .layout_window
            .as_ref()
            .and_then(|lw| lw.hover_manager.get_current(&InputPointId::Mouse))
            .cloned()
            .unwrap_or_else(|| FullHitTest::empty(None));

        if hit_test.is_empty() {
            return false;
        }

        // Find first node with a context menu
        for (dom_id, node_hit_test) in &hit_test.hovered_nodes {
            // Check regular hit test nodes
            for (node_id, hit_item) in &node_hit_test.regular_hit_test_nodes {
                // Try to get the context menu by cloning it
                let context_menu = if let Some(ref lw) = self.layout_window {
                    if let Some(lr) = lw.layout_results.get(dom_id) {
                        if let Some(nd) = lr
                            .styled_dom
                            .node_data
                            .as_container()
                            .get((*node_id).into())
                        {
                            nd.get_context_menu().cloned()
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    return false;
                };

                if let Some(menu) = context_menu {
                    // Check if native context menus are enabled
                    if self.current_window_state.flags.use_native_context_menus {
                        self.show_native_context_menu(&menu, client_x, client_y, *dom_id, *node_id);
                    } else {
                        self.show_window_based_context_menu(
                            &menu, client_x, client_y, *dom_id, *node_id,
                        );
                    }
                    return true;
                }
            }
        }

        false
    }

    /// Show a context menu using native Win32 popup menu
    fn show_native_context_menu(
        &mut self,
        menu: &azul_core::menu::Menu,
        client_x: i32,
        client_y: i32,
        dom_id: azul_core::dom::DomId,
        node_id: azul_core::dom::NodeId,
    ) {
        use self::dlopen::POINT;

        // Create popup menu
        let mut hmenu = unsafe { (self.win32.user32.CreatePopupMenu)() };
        if hmenu.is_null() {
            return;
        }

        // Build menu items and collect callbacks
        let mut callbacks = BTreeMap::new();
        menu::WindowsMenuBar::recursive_construct_menu(
            &mut hmenu,
            menu.items.as_ref(),
            &mut callbacks,
            &self.win32,
        );

        // Convert client to screen coordinates
        let mut pt = POINT {
            x: client_x,
            y: client_y,
        };
        unsafe {
            (self.win32.user32.ClientToScreen)(self.hwnd, &mut pt);
        }

        // Store callbacks for WM_COMMAND
        self.context_menu = Some(callbacks);

        // Show menu (blocks until closed)
        unsafe {
            (self.win32.user32.SetForegroundWindow)(self.hwnd);
            (self.win32.user32.TrackPopupMenu)(
                hmenu,
                0x0008 | 0x0000, // TPM_RIGHTBUTTON | TPM_LEFTALIGN
                pt.x,
                pt.y,
                0,
                self.hwnd,
                ptr::null(),
            );
            (self.win32.user32.DestroyMenu)(hmenu);
        }
    }

    /// Show a context menu using Azul window-based menu system
    ///
    /// This uses the same unified menu system as regular menus (crate::desktop::menu::show_menu)
    /// but spawns at cursor position instead of below a trigger rect.
    ///
    /// The menu window creation is queued and will be processed in Phase 3 of the event loop.
    fn show_window_based_context_menu(
        &mut self,
        menu: &azul_core::menu::Menu,
        client_x: i32,
        client_y: i32,
        _dom_id: azul_core::dom::DomId,
        _node_id: azul_core::dom::NodeId,
    ) {
        // Convert client coordinates to screen coordinates
        use self::dlopen::POINT;
        let mut pt = POINT {
            x: client_x,
            y: client_y,
        };
        unsafe {
            (self.win32.user32.ClientToScreen)(self.hwnd, &mut pt);
        }

        let cursor_pos = LogicalPosition::new(pt.x as f32, pt.y as f32);

        // Get parent window position
        let parent_pos = match self.current_window_state.position {
            azul_core::window::WindowPosition::Initialized(pos) => {
                LogicalPosition::new(pos.x as f32, pos.y as f32)
            }
            _ => LogicalPosition::new(0.0, 0.0),
        };

        // Create menu window options using the unified menu system
        // This is identical to how menu bar menus work, but with cursor_pos instead of trigger_rect
        let menu_options = crate::desktop::menu::show_menu(
            menu.clone(),
            self.system_style.clone(),
            parent_pos,
            None,             // No trigger rect for context menus (they spawn at cursor)
            Some(cursor_pos), // Cursor position for menu positioning
            None,             // No parent menu
        );

        // Queue window creation request for processing in Phase 3 of the event loop
        // The event loop will create the window with Win32Window::new()
        log_debug!(
            LogCategory::Window,
            "Queuing window-based context menu at screen ({}, {}) - will be created in event loop Phase 3",
            pt.x, pt.y
        );

        self.pending_window_creates.push(menu_options);
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
    const WM_DROPFILES: u32 = 0x0233;

    // IME (Input Method Editor) messages
    const WM_IME_SETCONTEXT: u32 = 0x0281;
    const WM_IME_NOTIFY: u32 = 0x0282;
    const WM_IME_CONTROL: u32 = 0x0283;
    const WM_IME_COMPOSITIONFULL: u32 = 0x0284;
    const WM_IME_SELECT: u32 = 0x0285;
    const WM_IME_CHAR: u32 = 0x0286;
    const WM_IME_REQUEST: u32 = 0x0288;
    const WM_IME_STARTCOMPOSITION: u32 = 0x010D;
    const WM_IME_COMPOSITION: u32 = 0x010F;
    const WM_IME_ENDCOMPOSITION: u32 = 0x010E;

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
            log_debug!(LogCategory::Window, "[Win32] WM_CREATE - Window created");
            0
        }

        WM_DESTROY => {
            log_debug!(LogCategory::Window, "[Win32] WM_DESTROY - Window destroyed");
            // Window destroyed - unregister from global registry
            window.is_open = false;
            registry::unregister_window(hwnd);
            0
        }

        WM_CLOSE => {
            log_debug!(LogCategory::Window, "[Win32] WM_CLOSE - Close requested");
            // User clicked close button - set close_requested flag
            // and process callbacks to allow cancellation
            window.current_window_state.flags.close_requested = true;

            // Process window events to trigger OnWindowClose callback
            let _ = window.process_window_events_recursive_v2(0);

            // Check if callback cancelled the close
            if window.current_window_state.flags.close_requested {
                // Not cancelled - proceed with close
                window.is_open = false;
                (window.win32.user32.DestroyWindow)(hwnd);
            } else {
                // Callback cancelled close - clear flag and keep window open
                log_debug!(LogCategory::Callbacks, "WM_CLOSE cancelled by callback");
            }

            0
        }

        WM_ERASEBKGND => {
            // Don't erase background, we'll paint everything
            1
        }

        WM_PAINT => {
            // Render frame if needed
            if window.frame_needs_regeneration {
                // Initial render: build display list and generate frame
                if let Err(e) = window.regenerate_layout() {
                    log_error!(LogCategory::Layout, "Layout regeneration error: {:?}", e);
                }
                window.frame_needs_regeneration = false;
            }

            match window.render_and_present() {
                Ok(_) => {}
                Err(e) => {
                    log_error!(LogCategory::Rendering, "Render error: {:?}", e);
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

                // Store old context for comparison
                let old_context = window.dynamic_selector_context.clone();

                // Update dynamic selector context with new viewport dimensions
                window.dynamic_selector_context.viewport_width = logical_size.width;
                window.dynamic_selector_context.viewport_height = logical_size.height;
                window.dynamic_selector_context.orientation = if logical_size.width > logical_size.height {
                    azul_css::dynamic_selector::OrientationType::Landscape
                } else {
                    azul_css::dynamic_selector::OrientationType::Portrait
                };

                // Check if any CSS breakpoints were crossed
                let breakpoints = [320.0, 480.0, 640.0, 768.0, 1024.0, 1280.0, 1440.0, 1920.0];
                if old_context.viewport_breakpoint_changed(&window.dynamic_selector_context, &breakpoints) {
                    log_debug!(LogCategory::Layout,
                        "[WM_SIZE] Breakpoint crossed: {}x{} -> {}x{}",
                        old_context.viewport_width,
                        old_context.viewport_height,
                        window.dynamic_selector_context.viewport_width,
                        window.dynamic_selector_context.viewport_height
                    );
                }

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
                    api::units::{DeviceIntRect, DeviceIntSize, DevicePixelScale},
                    Transaction as WrTransaction,
                };

                use crate::desktop::wr_translate2::wr_translate_document_id;

                let hidpi_factor = window.current_window_state.size.get_hidpi_factor();
                let mut txn = WrTransaction::new();
                txn.set_document_view(
                    DeviceIntRect::from_size(DeviceIntSize::new(width as i32, height as i32)),
                    DevicePixelScale::new(hidpi_factor.inner.get()),
                );

                window
                    .render_api
                    .send_transaction(wr_translate_document_id(window.document_id), txn);

                // Update previous and current window state
                window.previous_window_state = Some(window.current_window_state.clone());
                window.current_window_state = new_window_state;

                // Resize requires full display list rebuild
                window.frame_needs_regeneration = true;

                // Request redraw (will trigger regenerate_layout in WM_PAINT)
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

            // Handle active scrollbar drag (special case - not part of normal event system)
            if window.scrollbar_drag_state.is_some() {
                PlatformWindowV2::handle_scrollbar_drag(&mut *window, logical_pos);
                return 0;
            }

            // Save previous state BEFORE making changes
            window.previous_window_state = Some(window.current_window_state.clone());

            // Update mouse state
            window.current_window_state.mouse_state.cursor_position =
                CursorPosition::InWindow(logical_pos);

            // Record input sample for gesture detection (movement during button press)
            let button_state = if window.current_window_state.mouse_state.left_down {
                0x01
            } else {
                0x00
            };
            window.record_input_sample(logical_pos, button_state, false, false);

            // Update hit test
            if let Some(ref mut layout_window) = window.layout_window {
                use crate::desktop::wr_translate2::fullhittest_new_webrender;

                let hit_tester = window.hit_tester.resolve();
                let hit_test = fullhittest_new_webrender(
                    &*hit_tester,
                    window.document_id,
                    layout_window.focus_manager.get_focused_node().copied(),
                    &layout_window.layout_results,
                    &CursorPosition::InWindow(logical_pos),
                    hidpi_factor,
                );

                layout_window
                    .hover_manager
                    .push_hit_test(InputPointId::Mouse, hit_test.clone());

                // Update cursor based on CSS cursor properties
                // This is done BEFORE callbacks so callbacks can override the cursor
                let cursor_type_hit_test = layout_window.compute_cursor_type_hit_test(&hit_test);
                let new_cursor_type = cursor_type_hit_test.cursor_icon;
                let new = OptionMouseCursorType::Some(new_cursor_type);

                // Update cursor type if changed
                if window.current_window_state.mouse_state.mouse_cursor_type != new {
                    window.current_window_state.mouse_state.mouse_cursor_type = new;
                    event::set_cursor(new_cursor_type, &window.win32);
                }
            }

            // V2 system will detect MouseOver/MouseEnter/MouseLeave/Drag from state diff
            let result = window.process_window_events_recursive_v2(0);

            // Request WM_MOUSELEAVE notification
            use self::dlopen::{TME_LEAVE, TRACKMOUSEEVENT};
            unsafe {
                let mut tme = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                (window.win32.user32.TrackMouseEvent)(&mut tme);
            }

            // Request redraw if needed
            if !matches!(result, azul_core::events::ProcessEventResult::DoNothing) {
                (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
            }

            0
        }

        WM_MOUSELEAVE => {
            // Mouse left the window area
            // Save previous state
            window.previous_window_state = Some(window.current_window_state.clone());

            // Get last known position, or default
            let last_pos = match window.current_window_state.mouse_state.cursor_position {
                CursorPosition::InWindow(pos) => pos,
                CursorPosition::OutOfWindow(pos) => pos,
                CursorPosition::Uninitialized => LogicalPosition::new(0.0, 0.0),
            };

            // Clear mouse position (mouse is outside window)
            use azul_core::{geom::LogicalPosition, window::CursorPosition};
            window.current_window_state.mouse_state.cursor_position =
                CursorPosition::OutOfWindow(last_pos);

            // Process events - this will trigger MouseLeave callbacks
            let result = window.process_window_events_recursive_v2(0);

            // Request redraw if needed to clear hover states
            if !matches!(result, azul_core::events::ProcessEventResult::DoNothing) {
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

            // Check for scrollbar hit FIRST (before state changes)
            if let Some(scrollbar_hit_id) =
                PlatformWindowV2::perform_scrollbar_hit_test(&*window, logical_pos)
            {
                PlatformWindowV2::handle_scrollbar_click(
                    &mut *window,
                    scrollbar_hit_id,
                    logical_pos,
                );
                return 0;
            }

            // Save previous state BEFORE making changes
            window.previous_window_state = Some(window.current_window_state.clone());

            // Update mouse state
            window.current_window_state.mouse_state.cursor_position =
                CursorPosition::InWindow(logical_pos);
            window.current_window_state.mouse_state.left_down = true;

            // Record input sample for gesture detection (button down starts new session)
            window.record_input_sample(logical_pos, 0x01, true, false);

            // Update hit test
            if let Some(ref mut layout_window) = window.layout_window {
                use crate::desktop::wr_translate2::fullhittest_new_webrender;

                let hit_tester = window.hit_tester.resolve();
                let hit_test = fullhittest_new_webrender(
                    &*hit_tester,
                    window.document_id,
                    layout_window.focus_manager.get_focused_node().copied(),
                    &layout_window.layout_results,
                    &CursorPosition::InWindow(logical_pos),
                    hidpi_factor,
                );

                layout_window
                    .hover_manager
                    .push_hit_test(InputPointId::Mouse, hit_test);
            }

            // Capture mouse
            (window.win32.user32.SetCapture)(hwnd);

            // V2 system will detect MouseDown event
            let result = window.process_window_events_recursive_v2(0);

            // Request redraw if needed
            if !matches!(result, azul_core::events::ProcessEventResult::DoNothing) {
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

            // End scrollbar drag if active (before state changes)
            if window.scrollbar_drag_state.is_some() {
                window.scrollbar_drag_state = None;
                unsafe {
                    (window.win32.user32.ReleaseCapture)();
                }
                return 0;
            }

            // Save previous state BEFORE making changes
            window.previous_window_state = Some(window.current_window_state.clone());

            // Update mouse state
            window.current_window_state.mouse_state.cursor_position =
                CursorPosition::InWindow(logical_pos);
            window.current_window_state.mouse_state.left_down = false;

            // Record input sample for gesture detection (button up ends session)
            window.record_input_sample(logical_pos, 0x00, false, true);

            // Update hit test
            if let Some(ref mut layout_window) = window.layout_window {
                use crate::desktop::wr_translate2::fullhittest_new_webrender;

                let hit_tester = window.hit_tester.resolve();
                let hit_test = fullhittest_new_webrender(
                    &*hit_tester,
                    window.document_id,
                    layout_window.focus_manager.get_focused_node().copied(),
                    &layout_window.layout_results,
                    &CursorPosition::InWindow(logical_pos),
                    hidpi_factor,
                );

                layout_window
                    .hover_manager
                    .push_hit_test(InputPointId::Mouse, hit_test);
            }

            // Release mouse capture
            (window.win32.user32.ReleaseCapture)();

            // V2 system will detect MouseUp event
            let result = window.process_window_events_recursive_v2(0);

            // Request redraw if needed
            if !matches!(result, azul_core::events::ProcessEventResult::DoNothing) {
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
            if let Some(ref mut layout_window) = window.layout_window {
                use crate::desktop::wr_translate2::fullhittest_new_webrender;

                let hit_tester = window.hit_tester.resolve();
                let hit_test = fullhittest_new_webrender(
                    &*hit_tester,
                    window.document_id,
                    layout_window.focus_manager.get_focused_node().copied(),
                    &layout_window.layout_results,
                    &CursorPosition::InWindow(logical_pos),
                    hidpi_factor,
                );

                layout_window
                    .hover_manager
                    .push_hit_test(InputPointId::Mouse, hit_test);
            }

            // V2 system will detect MouseDown event
            let result = window.process_window_events_recursive_v2(0);

            // Request redraw if needed
            if !matches!(result, azul_core::events::ProcessEventResult::DoNothing) {
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
            if let Some(ref mut layout_window) = window.layout_window {
                use crate::desktop::wr_translate2::fullhittest_new_webrender;

                let hit_tester = window.hit_tester.resolve();
                let hit_test = fullhittest_new_webrender(
                    &*hit_tester,
                    window.document_id,
                    layout_window.focus_manager.get_focused_node().copied(),
                    &layout_window.layout_results,
                    &CursorPosition::InWindow(logical_pos),
                    hidpi_factor,
                );

                layout_window
                    .hover_manager
                    .push_hit_test(InputPointId::Mouse, hit_test);
            }

            // Try to show context menu first
            let showed_context_menu = window.try_show_context_menu(x, y);

            // If context menu was shown, skip normal mouse up processing
            if !showed_context_menu {
                // V2 system will detect MouseUp event
                let result = window.process_window_events_recursive_v2(0);

                // Request redraw if needed
                if !matches!(result, azul_core::events::ProcessEventResult::DoNothing) {
                    (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
                }
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
            let result = window.process_window_events_recursive_v2(0);

            if !matches!(result, azul_core::events::ProcessEventResult::DoNothing) {
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
            let result = window.process_window_events_recursive_v2(0);

            if !matches!(result, azul_core::events::ProcessEventResult::DoNothing) {
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

            // Record scroll sample using ScrollManager (if delta is significant)
            // The ScrollManager will update its internal state, and during the next render,
            // scroll_all_nodes() will synchronize the offsets to WebRender automatically.
            let hovered_node_for_scroll = if delta.abs() > 0 {
                if let Some(ref mut layout_window) = window.layout_window {
                    use azul_core::task::Instant;

                    let now = Instant::from(std::time::Instant::now());
                    let _scroll_node = layout_window.scroll_manager.record_sample(
                        0.0,                  // No horizontal scroll from mousewheel
                        scroll_amount * 20.0, // Scale for pixel scrolling
                        &layout_window.hover_manager,
                        &InputPointId::Mouse,
                        now,
                    );

                    // Note: We do NOT call gpu_scroll() here - it would cause double-scrolling!
                    // The scroll state will be automatically synchronized to WebRender during
                    // the next render_and_present() call via scroll_all_nodes().

                    _scroll_node
                } else {
                    None
                }
            } else {
                None
            };

            // Update hit test
            if let Some(ref mut layout_window) = window.layout_window {
                use crate::desktop::wr_translate2::fullhittest_new_webrender;

                let hit_tester = window.hit_tester.resolve();
                let hit_test = fullhittest_new_webrender(
                    &*hit_tester,
                    window.document_id,
                    layout_window.focus_manager.get_focused_node().copied(),
                    &layout_window.layout_results,
                    &CursorPosition::InWindow(logical_pos),
                    hidpi_factor,
                );

                layout_window
                    .hover_manager
                    .push_hit_test(InputPointId::Mouse, hit_test);
            }

            // V2 system will detect Scroll event from ScrollManager state
            let result = window.process_window_events_recursive_v2(0);

            if !matches!(result, azul_core::events::ProcessEventResult::DoNothing) {
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
                let result = window.process_window_events_recursive_v2(0);

                if !matches!(result, azul_core::events::ProcessEventResult::DoNothing) {
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
                let result = window.process_window_events_recursive_v2(0);

                if !matches!(result, azul_core::events::ProcessEventResult::DoNothing) {
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

                // Record text input in the TextInputManager
                if let Some(ref mut layout_window) = window.layout_window {
                    let text_str = chr.to_string();
                    layout_window.record_text_input(&text_str);
                }

                // V2 system will detect TextInput event
                let result = window.process_window_events_recursive_v2(0);

                if !matches!(result, azul_core::events::ProcessEventResult::DoNothing) {
                    (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
                }
            }

            0
        }

        WM_IME_STARTCOMPOSITION => {
            // IME composition started (e.g., user starts typing Japanese)
            // Phase 2: OnCompositionStart callback - sync IME position
            window.sync_ime_position_to_os();

            // Let Windows handle the composition window by default
            (window.win32.user32.DefWindowProcW)(hwnd, msg, wparam, lparam)
        }

        WM_IME_COMPOSITION => {
            // IME composition in progress or completed
            // lparam flags indicate what changed:
            // GCS_RESULTSTR (0x0800) = final composed string ready
            // GCS_COMPSTR (0x0008) = intermediate composition string

            const GCS_RESULTSTR: isize = 0x0800;
            const GCS_COMPSTR: isize = 0x0008;

            if lparam & GCS_RESULTSTR != 0 {
                // Final composed string is ready - clear composition preview
                window.ime_composition = None;

                // Let default processing handle it which will generate WM_IME_CHAR messages
                (window.win32.user32.DefWindowProcW)(hwnd, msg, wparam, lparam)
            } else if lparam & GCS_COMPSTR != 0 {
                // Intermediate composition - extract and store it
                if let Some(ref imm32) = window.win32.imm32 {
                    unsafe {
                        // Get IME context
                        let himc = (imm32.ImmGetContext)(hwnd);
                        if !himc.is_null() {
                            // Get composition string length
                            let len = (imm32.ImmGetCompositionStringW)(
                                himc,
                                GCS_COMPSTR as u32,
                                ptr::null_mut(),
                                0,
                            );

                            if len > 0 {
                                // Allocate buffer (len is in bytes, need len/2 u16s)
                                let buf_len = (len as usize) / 2;
                                let mut buffer: Vec<u16> = vec![0; buf_len];

                                // Get the actual string
                                let result = (imm32.ImmGetCompositionStringW)(
                                    himc,
                                    GCS_COMPSTR as u32,
                                    buffer.as_mut_ptr() as *mut _,
                                    len as u32,
                                );

                                if result > 0 {
                                    // Convert to String and store
                                    window.ime_composition = String::from_utf16(&buffer).ok();
                                    log_trace!(LogCategory::Input, "IME Composition: {:?}", window.ime_composition);
                                }
                            }

                            // Release context
                            (imm32.ImmReleaseContext)(hwnd, himc);
                        }
                    }
                }

                // Let Windows show composition window by default
                (window.win32.user32.DefWindowProcW)(hwnd, msg, wparam, lparam)
            } else {
                (window.win32.user32.DefWindowProcW)(hwnd, msg, wparam, lparam)
            }
        }

        WM_IME_ENDCOMPOSITION => {
            // IME composition ended - clear composition preview
            window.ime_composition = None;
            (window.win32.user32.DefWindowProcW)(hwnd, msg, wparam, lparam)
        }

        WM_IME_CHAR => {
            // Double-byte character from IME (e.g., Japanese, Chinese, Korean)
            // The new V2 input system handles text input through a different mechanism
            // This character will be processed by the event system automatically
            let char_code = wparam as u32;

            if let Some(chr) = char::from_u32(char_code) {
                if !chr.is_control() {
                    window.previous_window_state = Some(window.current_window_state.clone());

                    // Record text input in the TextInputManager
                    if let Some(ref mut layout_window) = window.layout_window {
                        let text_str = chr.to_string();
                        layout_window.record_text_input(&text_str);
                    }

                    // V2 system will detect TextInput event
                    let result = window.process_window_events_recursive_v2(0);

                    if !matches!(result, azul_core::events::ProcessEventResult::DoNothing) {
                        (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
                    }
                }
            }

            0
        }

        WM_IME_NOTIFY | WM_IME_SETCONTEXT => {
            // Other IME events - use default processing
            (window.win32.user32.DefWindowProcW)(hwnd, msg, wparam, lparam)
        }

        WM_SETFOCUS => {
            // Window gained focus
            window.previous_window_state = Some(window.current_window_state.clone());
            window.current_window_state.flags.has_focus = true;
            window.current_window_state.window_focused = true;

            // Phase 2: OnFocus callback - sync IME position after focus
            window.sync_ime_position_to_os();

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
                // Thread polling timer - threads are managed by LayoutWindow
                // Thread results will be processed during regenerate_layout
                if let Some(ref layout_window) = window.layout_window {
                    if !layout_window.threads.is_empty() {
                        window.frame_needs_regeneration = true;
                        (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
                    }
                }
            } else {
                // User timer from LayoutWindow - invoke expired timer callbacks
                use crate::desktop::shell2::common::event_v2::PlatformWindowV2;
                use azul_core::callbacks::Update;
                
                let timer_results = window.invoke_expired_timers();
                
                // Process each callback result to handle window state modifications
                let mut needs_redraw = false;
                for result in &timer_results {
                    // Apply window state changes from callback result
                    if result.modified_window_state.is_some() {
                        // Save previous state BEFORE applying changes (for sync_window_state diff)
                        window.previous_window_state = Some(window.current_window_state.clone());
                        let _ = window.process_callback_result_v2(result);
                        // Synchronize window state with OS immediately after change
                        window.sync_window_state();
                    }
                    // Check if redraw needed
                    if matches!(result.callbacks_update_screen, Update::RefreshDom | Update::RefreshDomAllWindows) {
                        needs_redraw = true;
                    }
                }
                
                if needs_redraw {
                    log_trace!(LogCategory::Timer, "Invoked {} timer callbacks", timer_results.len());
                    window.frame_needs_regeneration = true;
                    (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
                }
            }

            0
        }

        WM_COMMAND => {
            // Menu command
            let command_id = (wparam & 0xFFFF) as u16;

            log_trace!(LogCategory::EventLoop, "WM_COMMAND received, command_id: {}", command_id);

            // Look up menu callback and invoke it
            let callback_opt = if let Some(menu_bar) = &window.menu_bar {
                menu_bar.callbacks.get(&command_id).cloned()
            } else if let Some(context_menu) = &window.context_menu {
                context_menu.get(&command_id).cloned()
            } else {
                None
            };

            if let Some(callback) = callback_opt {
                log_trace!(
                    LogCategory::Callbacks,
                    "Found menu callback for command_id: {}",
                    command_id
                );

                // Convert CoreMenuCallback to layout MenuCallback
                use azul_layout::callbacks::{Callback, MenuCallback};

                let layout_callback = Callback::from_core(callback.callback);
                let mut menu_callback = MenuCallback {
                    callback: layout_callback,
                    refany: callback.refany,
                };

                // Get layout window
                if let Some(layout_window) = window.layout_window.as_mut() {
                    use azul_core::window::RawWindowHandle;

                    let raw_handle = RawWindowHandle::Windows(azul_core::window::WindowsHandle {
                        hwnd: hwnd as *mut _,
                        hinstance: ptr::null_mut(), // Not needed for menu callbacks
                    });

                    // Clone fc_cache (cheap Arc clone) since invoke_single_callback needs &mut
                    let mut fc_cache_clone = (*window.fc_cache).clone();

                    // Use LayoutWindow::invoke_single_callback which handles all the borrow
                    // complexity
                    let callback_result = layout_window.invoke_single_callback(
                        &mut menu_callback.callback,
                        &mut menu_callback.refany,
                        &raw_handle,
                        &window.gl_context_ptr,
                        &mut window.image_cache,
                        &mut fc_cache_clone,
                        window.system_style.clone(),
                        &azul_layout::callbacks::ExternalSystemCallbacks::rust_internal(),
                        &window.previous_window_state,
                        &window.current_window_state,
                        &window.renderer_resources,
                    );

                    // Process callback result using the V2 unified system
                    // This handles timers, threads, window state changes, and Update
                    use crate::desktop::shell2::common::event_v2::PlatformWindowV2;
                    let event_result = window.process_callback_result_v2(&callback_result);

                    // Sync window state changes to Win32 (title, position, size, etc.)
                    window.sync_window_state();

                    // Handle the event result
                    use azul_core::events::ProcessEventResult;
                    match event_result {
                        ProcessEventResult::ShouldRegenerateDomCurrentWindow
                        | ProcessEventResult::ShouldRegenerateDomAllWindows
                        | ProcessEventResult::ShouldReRenderCurrentWindow
                        | ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
                        | ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                            window.frame_needs_regeneration = true;
                            (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
                        }
                        ProcessEventResult::DoNothing => {
                            // No action needed
                        }
                    }
                } else {
                    log_warn!(LogCategory::Callbacks, "No layout window available for menu callback");
                }
            } else {
                log_debug!(LogCategory::Callbacks, "No callback found for command_id: {}", command_id);
            }

            (window.win32.user32.DefWindowProcW)(hwnd, msg, wparam, lparam)
        }

        WM_DPICHANGED => {
            // DPI changed
            let new_dpi = ((wparam >> 16) & 0xFFFF) as u32;

            // Save previous state
            window.previous_window_state = Some(window.current_window_state.clone());

            // Update DPI in window state
            window.current_window_state.size.dpi = new_dpi;

            // Get suggested size from lParam (RECT*)
            if lparam != 0 {
                let rect = unsafe { &*(lparam as *const dlopen::RECT) };
                let width = rect.right - rect.left;
                let height = rect.bottom - rect.top;

                // Update window size to suggested dimensions
                unsafe {
                    (window.win32.user32.SetWindowPos)(
                        hwnd,
                        ptr::null_mut(),
                        rect.left,
                        rect.top,
                        width,
                        height,
                        0x0004 | 0x0002, // SWP_NOZORDER | SWP_NOACTIVATE
                    );
                }

                // Update logical size with new DPI
                use azul_core::geom::PhysicalSizeU32;
                let physical_size = PhysicalSizeU32::new(width as u32, height as u32);
                let hidpi_factor = new_dpi as f32 / 96.0;
                let logical_size = physical_size.to_logical(hidpi_factor);
                window.current_window_state.size.dimensions = logical_size;
            }

            // DPI change requires full relayout
            window.frame_needs_regeneration = true;

            // Request redraw
            unsafe {
                (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
            }

            0
        }

        WM_DROPFILES => {
            // File drag-and-drop
            let hdrop = wparam as dlopen::HDROP;

            // Only process if shell32.dll is available
            if let Some(ref shell32) = window.win32.shell32 {
                unsafe {
                    // Get drop point
                    let mut pt = dlopen::POINT { x: 0, y: 0 };
                    (shell32.DragQueryPoint)(hdrop, &mut pt);

                    // Get number of files
                    let file_count =
                        (shell32.DragQueryFileW)(hdrop, 0xFFFFFFFF, ptr::null_mut(), 0);

                    let mut dropped_files = Vec::new();

                    for i in 0..file_count {
                        // Get required buffer size
                        let len = (shell32.DragQueryFileW)(hdrop, i, ptr::null_mut(), 0);

                        if len > 0 {
                            // Allocate buffer (+1 for null terminator)
                            let mut buffer = vec![0u16; (len + 1) as usize];

                            // Get file path
                            (shell32.DragQueryFileW)(hdrop, i, buffer.as_mut_ptr(), len + 1);

                            // Convert to Rust String
                            let path_str = String::from_utf16_lossy(&buffer[..len as usize]);
                            dropped_files.push(path_str);
                        }
                    }

                    (shell32.DragFinish)(hdrop);

                    // Update window state with dropped files
                    if !dropped_files.is_empty() {
                        window.previous_window_state = Some(window.current_window_state.clone());

                        // Store first file in cursor_manager (API limitation)
                        if let Some(ref mut layout_window) = window.layout_window {
                            if let Some(first_file) = dropped_files.first() {
                                layout_window
                                    .file_drop_manager
                                    .set_dropped_file(Some(first_file.clone().into()));
                            }
                        }

                        // Process window events to trigger FileDrop callbacks
                        window.process_window_events_recursive_v2(0);

                        // Clear dropped file after processing
                        if let Some(ref mut layout_window) = window.layout_window {
                            layout_window.file_drop_manager.set_dropped_file(None);
                        }
                    }
                }
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

/// Windows event type.
#[derive(Debug, Clone, Copy)]
pub enum Win32Event {
    /// Window close requested
    Close,
    /// Window resized
    Resize { width: u32, height: u32 },
    /// Mouse moved
    MouseMove { x: f64, y: f64 },
    /// Mouse button pressed
    MouseDown { button: u8, x: f64, y: f64 },
    /// Mouse button released
    MouseUp { button: u8, x: f64, y: f64 },
    /// Key pressed
    KeyDown { key_code: u16 },
    /// Key released
    KeyUp { key_code: u16 },
    /// DPI changed
    DpiChanged { new_dpi: u32 },
    /// Other event
    Other,
}

// PlatformWindow trait implementation

use crate::desktop::shell2::common::{PlatformWindow, RenderContext};

impl PlatformWindow for Win32Window {
    type EventType = Win32Event;

    fn new(options: WindowCreateOptions, app_data: RefAny) -> Result<Self, WindowError> {
        // Use existing new() implementation with provided app_data
        let fc_cache = Arc::new(rust_fontconfig::FcFontCache::build());
        let app_data_arc = Arc::new(std::cell::RefCell::new(app_data));
        Self::new(options, fc_cache, app_data_arc)
    }

    fn get_state(&self) -> FullWindowState {
        self.current_window_state.clone()
    }

    fn set_properties(&mut self, _props: WindowProperties) -> Result<(), WindowError> {
        // TODO: Implement property setting (title, size, etc.)
        Ok(())
    }

    fn poll_event(&mut self) -> Option<Self::EventType> {
        // The existing poll_event_internal returns bool
        // We need to convert this to return Option<Win32Event>
        // For now, return None - will be implemented in phase 1.2
        if self.poll_event_internal() {
            Some(Win32Event::Other)
        } else {
            None
        }
    }

    fn get_render_context(&self) -> RenderContext {
        if let Some(gl_context) = self.gl_context {
            RenderContext::OpenGL {
                context: gl_context as *mut std::ffi::c_void,
            }
        } else {
            // Software rendering - return null OpenGL context
            RenderContext::OpenGL {
                context: std::ptr::null_mut(),
            }
        }
    }

    fn present(&mut self) -> Result<(), WindowError> {
        self.render_and_present()
            .map_err(|e| WindowError::PlatformError(format!("Present failed: {}", e)))
    }

    fn is_open(&self) -> bool {
        self.is_open
    }

    fn close(&mut self) {
        // Close the window by posting WM_CLOSE
        unsafe {
            use self::dlopen::constants::WM_CLOSE;
            (self.win32.user32.PostMessageW)(self.hwnd, WM_CLOSE, 0, 0);
        }
        self.is_open = false;
    }

    fn request_redraw(&mut self) {
        // Mark that frame needs regeneration
        self.frame_needs_regeneration = true;

        // Post WM_PAINT message to trigger redraw
        unsafe {
            use self::dlopen::constants::WM_PAINT;
            (self.win32.user32.PostMessageW)(self.hwnd, WM_PAINT, 0, 0);
        }
    }

    fn sync_clipboard(
        &mut self,
        clipboard_manager: &mut azul_layout::managers::clipboard::ClipboardManager,
    ) {
        clipboard::sync_clipboard(clipboard_manager);
    }
}

impl Win32Window {
    /// Inject a menu bar into the window
    ///
    /// On Windows, this creates a native HMENU hierarchy attached to the window.
    /// Menu callbacks are wired up to trigger via WM_COMMAND messages.
    ///
    /// # Returns
    /// * `Ok(())` if menu injection succeeded
    /// * `Err(String)` if menu injection failed
    pub fn inject_menu_bar(&mut self) -> Result<(), String> {
        // Extract menu from current window state (clone since we can't return a reference)
        let menu_opt: Option<azul_core::menu::Menu> =
            if let Some(layout_window) = self.layout_window.as_ref() {
                // Get menu from layout_window's root DOM (dom_id 0, node_id 0)
                layout_window
                    .layout_results
                    .get(&DomId::ROOT_ID)
                    .and_then(|lr| {
                        let node_container = lr.styled_dom.node_data.as_container();
                        node_container
                            .get(NodeId::ZERO)
                            .and_then(|n| n.get_menu_bar())
                            .map(|boxed_menu| (**boxed_menu).clone())
                    })
            } else {
                None
            };

        // Update menu bar using the helper function from menu.rs
        // This handles creation, update (via hash diff), and removal
        menu::set_menu_bar(
            self.hwnd,
            &mut self.menu_bar,
            menu_opt.as_ref(),
            &self.win32,
        );

        // Force window to redraw with new menu
        unsafe {
            (self.win32.user32.DrawMenuBar)(self.hwnd);
        }

        Ok(())
    }

    /// Gets the monitor information for the monitor that the window is currently on.
    #[cfg(target_os = "windows")]
    pub fn get_monitor_info(&self) -> Option<winapi::um::winuser::MONITORINFO> {
        use winapi::um::winuser::{
            GetMonitorInfoW, MonitorFromWindow, MONITORINFO, MONITOR_DEFAULTTONEAREST,
        };

        let monitor = unsafe { MonitorFromWindow(self.hwnd as _, MONITOR_DEFAULTTONEAREST) };

        if monitor.is_null() {
            return None;
        }

        let mut monitor_info: MONITORINFO = unsafe { std::mem::zeroed() };
        monitor_info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;

        let result = unsafe { GetMonitorInfoW(monitor, &mut monitor_info as *mut _) };

        if result != 0 {
            Some(monitor_info)
        } else {
            None
        }
    }

    #[cfg(not(target_os = "windows"))]
    pub fn get_monitor_info(&self) -> Option<()> {
        None
    }

    /// Returns the position and size of the window in physical pixels.
    pub fn get_window_rect(&self) -> Option<dlopen::RECT> {
        let mut rect: dlopen::RECT = Default::default();
        if unsafe { (self.win32.user32.GetWindowRect)(self.hwnd, &mut rect) } != 0 {
            Some(rect)
        } else {
            None
        }
    }

    /// Returns the DPI of the window.
    pub fn get_window_dpi(&self) -> u32 {
        unsafe { self.dpi.hwnd_dpi(self.hwnd as _) }
    }

    /// Get display information for the monitor this window is on
    pub fn get_window_display_info(&self) -> Option<crate::desktop::display::DisplayInfo> {
        use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};

        let monitor_info = self.get_monitor_info()?;

        let bounds = LogicalRect::new(
            LogicalPosition::new(
                monitor_info.rcMonitor.left as f32,
                monitor_info.rcMonitor.top as f32,
            ),
            LogicalSize::new(
                (monitor_info.rcMonitor.right - monitor_info.rcMonitor.left) as f32,
                (monitor_info.rcMonitor.bottom - monitor_info.rcMonitor.top) as f32,
            ),
        );

        let work_area = LogicalRect::new(
            LogicalPosition::new(
                monitor_info.rcWork.left as f32,
                monitor_info.rcWork.top as f32,
            ),
            LogicalSize::new(
                (monitor_info.rcWork.right - monitor_info.rcWork.left) as f32,
                (monitor_info.rcWork.bottom - monitor_info.rcWork.top) as f32,
            ),
        );

        let dpi = self.get_window_dpi();
        let scale_factor = dpi as f32 / 96.0;

        Some(crate::desktop::display::DisplayInfo {
            name: "Current Monitor".to_string(),
            bounds,
            work_area,
            scale_factor,
            is_primary: false,
            video_modes: vec![azul_core::window::VideoMode {
                size: azul_css::props::basic::LayoutSize::new(
                    bounds.size.width as isize,
                    bounds.size.height as isize,
                ),
                bit_depth: 32,
                refresh_rate: 60,
            }],
        })
    }

    /// Show a tooltip with the given text at the specified position
    ///
    /// Position is in logical coordinates. The tooltip will be created on first use.
    pub fn show_tooltip(&mut self, text: &str, position: LogicalPosition) -> Result<(), String> {
        // Lazily create tooltip if needed
        if self.tooltip.is_none() {
            self.tooltip = Some(tooltip::TooltipWindow::new(self.hwnd, self.win32.clone())?);
        }

        let dpi_factor = DpiScaleFactor::new(self.get_window_dpi() as f32 / 96.0);

        if let Some(ref mut tooltip) = self.tooltip {
            tooltip.show(text, position, dpi_factor)?;
        }

        Ok(())
    }

    /// Hide the currently displayed tooltip
    ///
    /// Does nothing if no tooltip is shown.
    pub fn hide_tooltip(&mut self) -> Result<(), String> {
        if let Some(ref mut tooltip) = self.tooltip {
            tooltip.hide()?;
        }
        Ok(())
    }

    /// Set the window to be always on top (or not)
    ///
    /// Uses SetWindowPos with HWND_TOPMOST/HWND_NOTOPMOST.
    pub fn set_is_top_level(&mut self, is_top_level: bool) -> Result<(), String> {
        const HWND_TOPMOST: HWND = -1isize as HWND;
        const HWND_NOTOPMOST: HWND = -2isize as HWND;
        const SWP_NOMOVE: u32 = 0x0002;
        const SWP_NOSIZE: u32 = 0x0001;
        const SWP_NOACTIVATE: u32 = 0x0010;

        let hwnd_insert_after = if is_top_level {
            HWND_TOPMOST
        } else {
            HWND_NOTOPMOST
        };

        let result = unsafe {
            (self.win32.user32.SetWindowPos)(
                self.hwnd,
                hwnd_insert_after,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            )
        };

        if result == 0 {
            Err("SetWindowPos failed for is_top_level".to_string())
        } else {
            Ok(())
        }
    }

    /// Prevent the system from sleeping (or allow it to sleep)
    ///
    /// Uses SetThreadExecutionState with ES_CONTINUOUS and ES_DISPLAY_REQUIRED.
    pub fn set_prevent_system_sleep(&mut self, prevent: bool) -> Result<(), String> {
        const ES_CONTINUOUS: u32 = 0x80000000;
        const ES_DISPLAY_REQUIRED: u32 = 0x00000002;

        if let Some(ref kernel32) = self.win32.kernel32 {
            let flags = if prevent {
                ES_CONTINUOUS | ES_DISPLAY_REQUIRED
            } else {
                ES_CONTINUOUS
            };

            let result = unsafe { (kernel32.SetThreadExecutionState)(flags) };

            if result == 0 {
                Err("SetThreadExecutionState failed".to_string())
            } else {
                Ok(())
            }
        } else {
            Err("kernel32.dll not loaded - cannot set prevent_system_sleep".to_string())
        }
    }
}

// PlatformWindowV2 Trait Implementation

impl PlatformWindowV2 for Win32Window {
    // REQUIRED: Simple Getter Methods

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
        &self.fc_cache
    }

    fn get_gl_context_ptr(&self) -> &OptionGlContextPtr {
        &self.gl_context_ptr
    }

    fn get_system_style(&self) -> &Arc<azul_css::system::SystemStyle> {
        &self.system_style
    }

    fn get_app_data(&self) -> &Arc<RefCell<RefAny>> {
        &self.app_data
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
        &self.hit_tester
    }

    fn get_hit_tester_mut(&mut self) -> &mut AsyncHitTester {
        &mut self.hit_tester
    }

    fn get_last_hovered_node(&self) -> Option<&event_v2::HitTestNode> {
        self.last_hovered_node.as_ref()
    }

    fn set_last_hovered_node(&mut self, node: Option<event_v2::HitTestNode>) {
        self.last_hovered_node = node;
    }

    fn get_document_id(&self) -> DocumentId {
        self.document_id
    }

    fn get_id_namespace(&self) -> IdNamespace {
        self.id_namespace
    }

    fn get_render_api(&self) -> &WrRenderApi {
        &self.render_api
    }

    fn get_render_api_mut(&mut self) -> &mut WrRenderApi {
        &mut self.render_api
    }

    fn get_renderer(&self) -> Option<&webrender::Renderer> {
        self.renderer.as_ref()
    }

    fn get_renderer_mut(&mut self) -> Option<&mut webrender::Renderer> {
        self.renderer.as_mut()
    }

    fn get_raw_window_handle(&self) -> RawWindowHandle {
        RawWindowHandle::Windows(WindowsHandle {
            hwnd: self.hwnd as *mut std::ffi::c_void,
            hinstance: self.hinstance as *mut std::ffi::c_void,
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
            window_handle: RawWindowHandle::Windows(WindowsHandle {
                hwnd: self.hwnd as *mut std::ffi::c_void,
                hinstance: self.hinstance as *mut std::ffi::c_void,
            }),
            gl_context_ptr: &self.gl_context_ptr,
            image_cache: &mut self.image_cache,
            fc_cache_clone: (*self.fc_cache).clone(),
            system_style: self.system_style.clone(),
            previous_window_state: &self.previous_window_state,
            current_window_state: &self.current_window_state,
            renderer_resources: &mut self.renderer_resources,
        }
    }

    // Timer Management (Win32 Implementation)

    fn start_timer(&mut self, timer_id: usize, timer: azul_layout::timer::Timer) {
        let interval_ms = timer.tick_millis().min(u32::MAX as u64) as u32;

        // Start Win32 timer
        let win32_timer_id =
            unsafe { (self.win32.user32.SetTimer)(self.hwnd, timer_id, interval_ms, ptr::null()) };

        self.timers.insert(timer_id, win32_timer_id);

        // Also store in layout_window for tick_timers() to work
        if let Some(layout_window) = self.layout_window.as_mut() {
            layout_window
                .timers
                .insert(azul_core::task::TimerId { id: timer_id }, timer);
        }
    }

    fn stop_timer(&mut self, timer_id: usize) {
        // Stop Win32 timer
        if let Some(win32_timer_id) = self.timers.remove(&timer_id) {
            unsafe {
                (self.win32.user32.KillTimer)(self.hwnd, win32_timer_id);
            };
        }

        // Remove from layout_window
        if let Some(layout_window) = self.layout_window.as_mut() {
            layout_window
                .timers
                .remove(&azul_core::task::TimerId { id: timer_id });
        }
    }

    // Thread Management (Win32 Implementation)

    fn start_thread_poll_timer(&mut self) {
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

    fn stop_thread_poll_timer(&mut self) {
        if let Some(timer_id) = self.thread_timer_running.take() {
            unsafe {
                (self.win32.user32.KillTimer)(self.hwnd, timer_id);
            };
        }
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

    // REQUIRED: Menu Display

    fn show_menu_from_callback(
        &mut self,
        menu: &azul_core::menu::Menu,
        position: azul_core::geom::LogicalPosition,
    ) {
        // Check if native menus are enabled
        if self.current_window_state.flags.use_native_context_menus {
            // Show native Win32 menu
            self.show_native_menu_at_position(menu, position);
        } else {
            // Show fallback DOM-based menu
            self.show_fallback_menu(menu, position);
        }
    }

    fn show_tooltip_from_callback(
        &mut self,
        text: &str,
        position: azul_core::geom::LogicalPosition,
    ) {
        if let Err(e) = self.show_tooltip(text, position) {
            log_error!(LogCategory::Window, "Failed to show tooltip: {}", e);
        }
    }

    fn hide_tooltip_from_callback(&mut self) {
        if let Err(e) = self.hide_tooltip() {
            log_error!(LogCategory::Window, "Failed to hide tooltip: {}", e);
        }
    }
}

impl Win32Window {
    /// Show a native Win32 menu at the given position
    fn show_native_menu_at_position(
        &mut self,
        menu: &azul_core::menu::Menu,
        position: azul_core::geom::LogicalPosition,
    ) {
        // TODO: Implement native Win32 TrackPopupMenu
        // For now, fall back to window-based menu
        log_debug!(
            LogCategory::Window,
            "Native menu at ({}, {}) - not yet implemented, using fallback",
            position.x, position.y
        );
        self.show_fallback_menu(menu, position);
    }

    /// Show a fallback window-based menu at the given position
    fn show_fallback_menu(
        &mut self,
        menu: &azul_core::menu::Menu,
        position: azul_core::geom::LogicalPosition,
    ) {
        // Get parent window position
        let parent_pos = match self.current_window_state.position {
            azul_core::window::WindowPosition::Initialized(pos) => {
                azul_core::geom::LogicalPosition::new(pos.x as f32, pos.y as f32)
            }
            _ => azul_core::geom::LogicalPosition::new(0.0, 0.0),
        };

        // Create menu window options
        let menu_options = crate::desktop::menu::show_menu(
            menu.clone(),
            self.system_style.clone(),
            parent_pos,
            None,           // No trigger rect
            Some(position), // Position for menu
            None,           // No parent menu
        );

        // Queue window creation request
        log_debug!(
            LogCategory::Window,
            "Queuing fallback menu window at ({}, {}) - will be created in event loop",
            position.x, position.y
        );

        self.pending_window_creates.push(menu_options);
    }
}

/// Position window on requested monitor, or center on primary monitor
fn position_window_on_monitor(
    hwnd: HWND,
    monitor_id: azul_core::window::MonitorId,
    position: azul_core::window::WindowPosition,
    size: azul_core::window::WindowSize,
    win32: &dlopen::Win32Libraries,
) {
    use azul_core::window::WindowPosition;

    use crate::desktop::display::get_monitors;

    // Get all available monitors
    let monitors = get_monitors();
    if monitors.len() == 0 {
        return; // No monitors available, use Windows default positioning
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
        use dlopen::constants::{SWP_NOSIZE, SWP_NOZORDER};
        (win32.user32.SetWindowPos)(
            hwnd,
            ptr::null_mut(), // No Z-order change
            x,
            y,
            0, // Width (ignored with SWP_NOSIZE)
            0, // Height (ignored with SWP_NOSIZE)
            SWP_NOZORDER | SWP_NOSIZE,
        );
    }
}

// IME Position Management

impl Win32Window {
    /// Set IME composition window position and area
    /// Called when ime_position is updated in window state
    pub fn set_ime_composition_window(&self, rect: azul_core::geom::LogicalRect) {
        if let Some(ref imm32) = self.win32.imm32 {
            unsafe {
                let hwnd = self.hwnd;
                let himc = (imm32.ImmGetContext)(hwnd);

                if !himc.is_null() {
                    use dlopen::{CFS_RECT, COMPOSITIONFORM, POINT, RECT};

                    let mut comp_form = COMPOSITIONFORM {
                        dwStyle: CFS_RECT,
                        ptCurrentPos: POINT {
                            x: rect.origin.x as i32,
                            y: rect.origin.y as i32,
                        },
                        rcArea: RECT {
                            left: rect.origin.x as i32,
                            top: rect.origin.y as i32,
                            right: (rect.origin.x + rect.size.width) as i32,
                            bottom: (rect.origin.y + rect.size.height) as i32,
                        },
                    };

                    (imm32.ImmSetCompositionWindow)(himc, &comp_form);
                    (imm32.ImmReleaseContext)(hwnd, himc);
                }
            }
        }
    }

    /// Sync ime_position from window state to OS
    pub fn sync_ime_position_to_os(&self) {
        use azul_core::window::ImePosition;

        if let ImePosition::Initialized(rect) = self.current_window_state.ime_position {
            self.set_ime_composition_window(rect);
        }
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
