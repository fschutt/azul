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

use crate::desktop::shell2::common::debug_server::LogCategory;
use crate::impl_platform_window_getters;
use crate::{log_debug, log_error, log_info, log_trace, log_warn};

pub mod accessibility;
pub mod clipboard;
pub mod dlopen;
pub mod dnd;
mod dpi;
pub mod win_event;
mod gl;
pub mod menu;
pub mod registry;
pub(crate) mod system_style;
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
        event::{self, HitTestNode, PlatformWindow, BUTTON_STATE_LEFT, BUTTON_STATE_NONE},
        Compositor, WindowError,
    },
    wr_translate2::{
        create_program_cache, default_renderer_options, translate_document_id_wr,
        translate_id_namespace_wr, wr_translate_document_id, AsyncHitTester, Notifier,
    },
};

/// Rendering mode for the window (GPU via WebRender or CPU fallback)
enum RenderMode {
    /// GPU rendering via WebRender + OpenGL
    Gpu {
        gl_context: HGLRC,
        hdc: *mut std::ffi::c_void,
    },
    /// CPU software rendering via cpurender + StretchDIBits
    Cpu,
}

/// Win32 window implementation using LayoutWindow API
/// Posted by the WebRender Notifier (backend thread) when a frame finished
/// building — the wndproc presents it.
pub(crate) const WM_APP_FRAME_READY: u32 = 0x8000 + 0x0042; // WM_APP + 0x42

pub struct Win32Window {
    /// Win32 window handle
    pub hwnd: HWND,
    /// Application instance handle
    pub hinstance: HINSTANCE,

    // Rendering infrastructure
    /// Rendering mode (GPU or CPU)
    render_mode: RenderMode,
    /// OpenGL function loader (kept for WebRender even in CPU mode for fallback)
    pub gl_functions: GlFunctions,
    /// Signal from WebRender that a new frame is ready
    pub new_frame_ready: Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
    /// Shared CPU rendering backend (same as headless + X11 + Wayland + macOS):
    /// owns the retained pixmap, compositor, glyph cache, display-list damage diff
    /// AND the scroll-shift / eligibility / present-split machinery. Replaces the
    /// former per-backend glyph_cache / retained_pixmap / previous_display_list.
    #[cfg(feature = "cpurender")]
    cpu_backend: crate::desktop::shell2::headless::CpuBackend,
    /// Cached BGRA conversion buffer reused across CPU frames
    #[cfg(feature = "cpurender")]
    bgra_buffer: Vec<u8>,
    /// Damage rects for incremental rendering (CPU and GPU)
    /// When non-empty, only these regions need redrawing
    gpu_damage_rects: Vec<azul_core::geom::LogicalRect>,

    /// Common window state (layout, resources, WebRender, etc.)
    pub common: event::CommonWindowState,

    // Win32 libraries
    /// Dynamically loaded Win32 libraries
    pub win32: dlopen::Win32Libraries,

    // Window state
    /// Window is open flag
    pub is_open: bool,
    /// Whether the first frame has been shown (for deferred window visibility)
    pub first_frame_shown: bool,
    /// A redraw was explicitly requested (route_main_window_result invalidated
    /// after ShouldReRender/ShouldUpdateDisplayList). Read by the GPU
    /// skip-heuristic so explicitly requested presents are never skipped;
    /// cleared when the GPU render proceeds.
    pub needs_gpu_present: bool,

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

    // System functions
    /// DPI functions
    pub dpi: DpiFunctions,

    // Shared resources
    /// Async font registry for background font scanning
    pub font_registry: Option<Arc<rust_fontconfig::registry::FcFontRegistry>>,
    /// Dynamic selector context for evaluating conditional CSS properties
    /// (viewport size, OS, theme, etc.) - updated on resize and theme change
    pub dynamic_selector_context: azul_css::dynamic_selector::DynamicSelectorContext,
    /// Icon provider for resolving icon names to renderable content
    pub icon_provider: azul_core::icon::SharedIconProvider,

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
        mut options: WindowCreateOptions,
        config: azul_core::resources::AppConfig,
        fc_cache: Arc<FcFontCache>,
        font_registry: Option<Arc<rust_fontconfig::registry::FcFontRegistry>>,
        app_data: Arc<std::cell::RefCell<RefAny>>,
        undo_manager: event::SharedUndoManager,
    ) -> Result<Self, WindowError> {
        // If background_color is None and no material effect, use system window background
        // Note: When a material is set, the renderer will use transparent clear color automatically
        if options.window_state.background_color.is_none() {
            use azul_core::window::WindowBackgroundMaterial;
            if matches!(options.window_state.flags.background_material, WindowBackgroundMaterial::Opaque) {
                options.window_state.background_color = config.system_style.colors.window_background;
            }
            // For materials, leave background_color as None - renderer handles transparency
        }
        
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
            log_error!(
                LogCategory::Platform,
                "[Win32] Failed to load Win32 libraries: {}",
                e
            );
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

        // Initialize OpenGL context + WebRender (if hardware rendering requested)
        let mut gl_functions = GlFunctions::initialize();

        // Determine renderer type from options
        let should_use_hardware = match options.renderer.into_option() {
            Some(r) => match r.hw_accel {
                azul_core::window::HwAcceleration::Enabled => true,
                azul_core::window::HwAcceleration::Disabled => false,
                azul_core::window::HwAcceleration::DontCare => true, // Try hardware first
            },
            None => true, // Default to hardware
        };

        // Get window size
        let (width, height) = wcreate::get_client_rect(hwnd, &win32)?;
        let physical_size = azul_core::geom::PhysicalSize::new(width, height);

        let new_frame_ready =
            std::sync::Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));

        // Try GPU path: GL context + WebRender; fall back to CPU if anything fails
        let (render_mode, renderer, render_api, hit_tester, document_id, id_namespace, gl_context_ptr) =
            if should_use_hardware {
                let gpu_result: Result<_, WindowError> = (|| {
                    let vsync = options.window_state.renderer_options.vsync;
                    let hglrc = wcreate::create_gl_context(hwnd, hinstance, &win32, vsync)?;
                    let hdc = unsafe { (win32.user32.GetDC)(hwnd) };
                    if hdc.is_null() {
                        return Err(WindowError::PlatformError("Failed to get HDC".into()));
                    }
                    #[cfg(target_os = "windows")]
                    unsafe {
                        use winapi::um::wingdi::wglMakeCurrent;
                        wglMakeCurrent(
                            hdc as winapi::shared::windef::HDC,
                            hglrc as winapi::shared::windef::HGLRC,
                        );
                    }
                    gl_functions.load();
                    let gl_ctx_inner = azul_core::gl::GlContextPtr::new(
                        RendererType::Hardware,
                        gl_functions.functions.clone(),
                    );
                    // PROVE the context: if our SVG/brush shaders won't compile at
                    // any GLSL version the driver is too broken for GPU rendering --
                    // bail to the CPU path (mirrors the X11 backend). Returning Err
                    // triggers the CPU fallback in the match below, and skips the
                    // (now-pointless) WebRender renderer creation.
                    if !gl_ctx_inner.is_gl_usable() {
                        return Err(WindowError::PlatformError(
                            "GL context unusable (azul shaders failed to compile at any GLSL version)"
                                .into(),
                        ));
                    }
                    let gl_context_ptr = OptionGlContextPtr::Some(gl_ctx_inner);

                    // Wake the message loop from WebRender's backend thread:
                    // PostMessageW is documented thread-safe, and WaitMessage
                    // returns when a posted message arrives. Without this the
                    // frame-ready condvar signalled nobody and the final frame
                    // of an interaction stayed unpresented until the next
                    // input event.
                    let post_message = win32.user32.PostMessageW;
                    let hwnd_for_notifier = hwnd as usize;
                    let (mut renderer, sender) = webrender::create_webrender_instance(
                        gl_functions.functions.clone(),
                        Box::new(Notifier {
                            new_frame_ready: new_frame_ready.clone(),
                            wake: Some(std::sync::Arc::new(move || unsafe {
                                (post_message)(
                                    hwnd_for_notifier as HWND,
                                    WM_APP_FRAME_READY,
                                    0,
                                    0,
                                );
                            })),
                        }),
                        // WGL has no buffer-age query — no partial present.
                        default_renderer_options(
                            &options,
                            create_program_cache(&gl_functions.functions),
                            None,
                        ),
                        None,
                    )
                    .map_err(|e| WindowError::PlatformError(format!("WebRender error: {:?}", e)))?;

                    renderer.set_external_image_handler(Box::new(
                        crate::desktop::wr_translate2::Compositor::default(),
                    ));

                    let render_api = sender.create_api();
                    let framebuffer_size =
                        webrender::api::units::DeviceIntSize::new(width as i32, height as i32);
                    let wr_doc_id = render_api.add_document(framebuffer_size);
                    let document_id = translate_document_id_wr(wr_doc_id);
                    let id_namespace = translate_id_namespace_wr(render_api.get_namespace_id());
                    let hit_tester_request =
                        render_api.request_hit_tester(wr_translate_document_id(document_id));

                    log_debug!(
                        LogCategory::Rendering,
                        "[Win32] GPU rendering initialized ({}x{})",
                        width, height
                    );

                    Ok((
                        RenderMode::Gpu { gl_context: hglrc, hdc },
                        Some(renderer),
                        Some(render_api),
                        Some(AsyncHitTester::Requested(hit_tester_request)),
                        Some(document_id),
                        Some(id_namespace),
                        gl_context_ptr,
                    ))
                })();

                match gpu_result {
                    Ok(tuple) => tuple,
                    Err(e) => {
                        log_warn!(
                            LogCategory::Rendering,
                            "[Win32] GPU init failed: {:?}, falling back to CPU rendering",
                            e
                        );
                        (RenderMode::Cpu, None, None, None, None, None, OptionGlContextPtr::None)
                    }
                }
            } else {
                log_info!(
                    LogCategory::Rendering,
                    "[Win32] Hardware acceleration disabled, using CPU rendering"
                );
                (RenderMode::Cpu, None, None, None, None, None, OptionGlContextPtr::None)
            };
        timing_log!("Create rendering context");

        // Update options size with actual window size
        options.window_state.size.dimensions = physical_size.to_logical(dpi_factor);

        // Determine renderer type
        let renderer_type = if matches!(render_mode, RenderMode::Gpu { .. }) {
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
            active_route: azul_core::resources::OptionRouteMatch::None,
        };

        // Set document_id and id_namespace for this window
        if let Some(doc_id) = document_id {
            layout_window.document_id = doc_id;
        }
        if let Some(ns_id) = id_namespace {
            layout_window.id_namespace = ns_id;
        }
        layout_window.current_window_state = current_window_state.clone();
        layout_window.renderer_type = Some(renderer_type);
        layout_window.routes = config.routes.clone();

        // Initialize monitor cache once at window creation
        if let Ok(mut guard) = layout_window.monitors.lock() {
            *guard = crate::desktop::display::get_monitors();
        }
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
        log_trace!(
            LogCategory::Window,
            "[Win32] deferring show_window until after a11y init (is_visible: {})",
            should_show_window
        );

        // Position window on requested monitor (or center on primary)
        // This can be done before showing
        // TODO: Use monitor_id to look up actual Monitor from global state
        position_window_on_monitor(
            hwnd,
            Monitor::default().monitor_id,
            current_window_state.position,
            current_window_state.size,
            options.parent_window_id,
            &win32,
        );
        timing_log!("Position window");

        // File drag-and-drop is enabled via OLE `RegisterDragDrop` (modern
        // hover + drop) in `register_drag_drop()`, called from the run loop
        // AFTER the window pointer is in the global registry (the legacy
        // `DragAcceptFiles`/`WM_DROPFILES` drop-only path has been removed).

        // Get current window state
        let current_window_state = layout_window.current_window_state.clone();

        // Create dynamic selector context before building window
        let initial_viewport_width = current_window_state.size.dimensions.width;
        let initial_viewport_height = current_window_state.size.dimensions.height;
        let system_style = Arc::new(config.system_style.clone());
        let dynamic_selector_context = {
            let mut ctx =
                azul_css::dynamic_selector::DynamicSelectorContext::from_system_style(&system_style);
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
        let is_cpu_mode = matches!(render_mode, RenderMode::Cpu);
        let mut result = Win32Window {
            hwnd,
            hinstance,
            render_mode,
            gl_functions,
            new_frame_ready,
            #[cfg(feature = "cpurender")]
            cpu_backend: crate::desktop::shell2::headless::CpuBackend::new(),
            #[cfg(feature = "cpurender")]
            bgra_buffer: Vec::new(),
            gpu_damage_rects: Vec::new(),
            common: event::CommonWindowState {
                layout_window: Some(layout_window),
                gl_context_ptr,
                renderer,
                render_api,
                hit_tester,
                cpu_hit_tester: if is_cpu_mode {
                    Some(azul_layout::headless::CpuHitTester::new())
                } else {
                    None
                },
                document_id,
                id_namespace,
                frame_needs_regeneration: true,
                frame_relayout_only: false,
                next_relayout_reason: azul_core::callbacks::RelayoutReason::Initial, // Initial render deferred to WM_PAINT
                display_list_initialized: false,
                display_list_dirty: false,
                a11y_dirty: true,
                previous_window_state: None,
                current_window_state,
                image_cache: ImageCache::default(),
                renderer_resources: RendererResources::default(),
                last_hovered_node: None,
                scrollbar_drag_state: None,
                app_data,
                undo_manager,
                fc_cache,
                system_style,
            },
            win32, // Store Win32 libraries for later use
            is_open: true,
            first_frame_shown: false, // Window will be shown after first SwapBuffers
            needs_gpu_present: false,
            menu_bar,
            context_menu: None,
            timers: HashMap::new(),
            thread_timer_running: None,
            high_surrogate: None,
            ime_composition: None,
            dpi: dpi_functions,
            font_registry,
            dynamic_selector_context,
            icon_provider: azul_core::icon::SharedIconProvider::from_handle(config.icon_provider.clone()),
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
                log_warn!(
                    LogCategory::Platform,
                    "[Win32] a11y adapter init failed: {}, continuing without a11y",
                    e
                );
            }
        }
        timing_log!("Initialize accessibility adapter");

        // Apply initial background material if not Opaque
        // This enables Mica/Acrylic effects on Windows 11
        {
            use azul_core::window::WindowBackgroundMaterial;
            let initial_material = result.common.current_window_state.flags.background_material;
            if !matches!(initial_material, WindowBackgroundMaterial::Opaque) {
                log_trace!(
                    LogCategory::Window,
                    "[Win32] Applying initial background material: {:?}",
                    initial_material
                );
                result.apply_background_material(initial_material);
            }
        }
        timing_log!("Apply initial background material");

        // Render FIRST FRAME before showing window to avoid black flash
        // This ensures the window has content when it becomes visible
        // NOTE: We do NOT show the window here! The window will be shown by run.rs
        // after this function returns and after waiting for new_frame_ready signal.
        {
            // Send first frame: regenerate layout + full transaction
            if let Err(e) = result.regenerate_layout() {
                log_error!(LogCategory::Layout, "First frame layout error: {:?}", e);
            }

            // size_to_content: the HWND was created as a 1×1 hidden placeholder
            // (see wcreate::create_hwnd). Now that the first layout has produced
            // a root size, resize the window to fit content before the
            // first_frame_shown gate inside render_and_present calls ShowWindow.
            if options.size_to_content {
                if let Some(layout_window) = result.common.layout_window.as_ref() {
                    if let Some(dom_result) = layout_window
                        .layout_results
                        .get(&azul_core::dom::DomId::ROOT_ID)
                    {
                        let root_size = dom_result
                            .layout_tree
                            .get_content_size(dom_result.layout_tree.root);
                        let w = libm::roundf(root_size.width).max(1.0) as i32;
                        let h = libm::roundf(root_size.height).max(1.0) as i32;
                        log_trace!(
                            LogCategory::Window,
                            "[Win32] size_to_content: resizing window to {}x{}",
                            w,
                            h
                        );
                        if let Err(e) = wcreate::set_window_size(result.hwnd, w, h, &result.win32) {
                            log_warn!(
                                LogCategory::Window,
                                "[Win32] size_to_content set_window_size failed: {:?}",
                                e
                            );
                        }
                    }
                }
            }

            result.common.frame_needs_regeneration = false;
            let _ = result.render_and_present(true);
        }
        timing_log!("Render first frame (async - not waiting for completion)");

        // The window will be shown after the first frame renders via the
        // `first_frame_shown` gate inside render_and_present (CPU and GPU paths).
        timing_log!("Skip show window (will be shown after first frame render)");

        // Invoke create_callback if provided (for GL resource upload, config loading, etc.)
        // This runs AFTER GL context is ready but BEFORE any layout is done
        if let Some(mut callback) = create_callback.into_option() {
            use azul_core::window::RawWindowHandle;

            let raw_handle = RawWindowHandle::Windows(azul_core::window::WindowsHandle {
                hwnd: hwnd as *mut _,
                hinstance: hinstance as *mut _,
            });

            // Get mutable references needed for invoke_single_callback
            let layout_window = result
                .common
                .layout_window
                .as_mut()
                .expect("LayoutWindow should exist at this point");
            // Get app_data for callback
            let mut app_data_ref = result.common.app_data.borrow_mut();

            let (changes, _update) = layout_window.invoke_single_callback(
                &mut callback,
                &mut *app_data_ref,
                &raw_handle,
                &result.common.gl_context_ptr,
                result.common.system_style.clone(),
                &azul_layout::callbacks::ExternalSystemCallbacks::rust_internal(),
                &result.common.previous_window_state,
                &result.common.current_window_state,
                &result.common.renderer_resources,
            );

            drop(app_data_ref);
            use crate::desktop::shell2::common::event::PlatformWindow;
            for change in &changes {
                let r = result.apply_user_change(change);
                if r != azul_core::events::ProcessEventResult::DoNothing {
                    result.common.frame_needs_regeneration = true;
                }
            }
        }

        // Register debug timer is now done from run() with explicit channel + component map
        timing_log!("Final setup (callback)");

        // Apply initial window state for fields not set during window creation
        result.apply_initial_window_state();

        log_debug!(
            LogCategory::Window,
            "[Win32] ===== TOTAL Win32Window::new() took {:?} =====",
            total_start.elapsed()
        );
        Ok(result)
    }

    /// Apply a batch of timer changes — convenience over the per-timer
    /// `start_timer`/`stop_timer` trait methods, useful for callers that diff
    /// timer state and want to apply the result in one call.
    pub fn start_stop_timers(
        &mut self,
        added: HashMap<usize, azul_layout::timer::Timer>,
        removed: std::collections::BTreeSet<usize>,
    ) {
        use crate::desktop::shell2::common::event::PlatformWindow;
        for (id, timer) in added {
            PlatformWindow::start_timer(self, id, timer);
        }
        for id in removed {
            PlatformWindow::stop_timer(self, id);
        }
    }

    /// Win32 timer ID reserved for thread-polling (~60 FPS tick).
    const THREAD_POLL_TIMER_ID: usize = 0xFFFF;
    /// Interval in milliseconds for the thread-polling timer (~60 FPS).
    const THREAD_POLL_INTERVAL_MS: u32 = 16;

    /// Start the thread-polling tick timer (delegates to the trait
    /// `start_thread_poll_timer` so external callers have a stable inherent API).
    pub fn start_thread_tick_timer(&mut self) {
        use crate::desktop::shell2::common::event::PlatformWindow;
        PlatformWindow::start_thread_poll_timer(self);
    }

    /// Stop the thread-polling tick timer.
    pub fn stop_thread_tick_timer(&mut self) {
        use crate::desktop::shell2::common::event::PlatformWindow;
        PlatformWindow::stop_thread_poll_timer(self);
    }

    /// Render and present a frame.
    ///
    /// When `layout_was_regenerated = true`, the full WebRender transaction (display lists,
    /// fonts, images, scroll offsets, GPU values) was already sent by `regenerate_layout()`.
    /// When `layout_was_regenerated = false` (scroll-only update, image callback update),
    /// we send a lightweight transaction with just scroll offsets, GPU values and image
    /// callback re-invocations — no display list rebuild.
    pub fn render_and_present(&mut self, layout_was_regenerated: bool) -> Result<(), WindowError> {

        // CPU rendering path: skip WebRender, render directly via cpurender + StretchDIBits
        if let RenderMode::Cpu = &self.render_mode {
            // Tracks whether this frame actually blitted content. The first-frame
            // ShowWindow is gated on this (see the show block below) to avoid
            // flashing a white window before anything was painted. Declared in
            // this scope — not inside the cpurender block — so the show logic can
            // read it.
            #[allow(unused_assignments)]
            let mut rendered = false;
            // No CPU renderer compiled in: nothing to render or defer — show as before.
            #[cfg(not(feature = "cpurender"))]
            {
                rendered = true;
            }
            #[cfg(feature = "cpurender")]
            {
                use azul_core::dom::DomId;

                // Synchronize window state to layout_window before rendering
                if let Some(ref mut layout_window) = self.common.layout_window {
                    layout_window.current_window_state =
                        self.common.current_window_state.clone();

                    // Advance easing-based scroll animations
                    {
                        #[cfg(feature = "std")]
                        let now = azul_core::task::Instant::System(
                            std::time::Instant::now().into(),
                        );
                        #[cfg(not(feature = "std"))]
                        let now = azul_core::task::Instant::Tick(
                            azul_core::task::SystemTick { tick_counter: 0 },
                        );
                        let tick_result = layout_window.scroll_manager.tick(now);
                        if tick_result.needs_repaint {
                            layout_window.scroll_manager.calculate_scrollbar_states();
                        }
                    }
                }

                // Re-invoke any VirtualViews queued for in-place re-render (e.g.
                // MapWidget tiles delivered by a background writeback that called
                // trigger_all_virtual_view_rerender). The GPU path drains this
                // inside common::layout::generate_frame; the CPU path has no
                // generate_frame, so without this the queue is never drained and
                // async-loaded VirtualView content never appears (same fix the
                // X11 and Wayland CPU branches have). Must run BEFORE render_frame
                // reads layout_results.
                let mut vviews_rebuilt = false;
                if let Some(lw) = self.common.layout_window.as_mut() {
                    if !lw.pending_virtual_view_updates.is_empty() {
                        let system_callbacks =
                            azul_layout::callbacks::ExternalSystemCallbacks::rust_internal();
                        let current_window_state = lw.current_window_state.clone();
                        let renderer_resources = std::mem::take(&mut lw.renderer_resources);
                        let updated = lw.process_pending_virtual_view_updates(
                            &current_window_state,
                            &renderer_resources,
                            &system_callbacks,
                        );
                        lw.renderer_resources = renderer_resources;
                        vviews_rebuilt = !updated.is_empty();
                    }
                }
                // The drain REBUILT VirtualView child DOMs (fresh NodeIds). The
                // CPU hit-tester still indexes the previous generation's rects —
                // rebuild it now, or the next pointer move hit-tests stale
                // NodeIds (cursor panic / events on the wrong node).
                if vviews_rebuilt {
                    if let (Some(cpu_ht), Some(lw)) = (
                        self.common.cpu_hit_tester.as_mut(),
                        self.common.layout_window.as_ref(),
                    ) {
                        cpu_ht.rebuild_from_layout(&lw.layout_results);
                    }
                }

                // Resolve RenderImageCallback <img> nodes into CPU images (the
                // renderer can't invoke callbacks; e.g. the AzulPaint canvas).
                // gl=None forces the callback's CPU branch.
                if let Some(lw) = self.common.layout_window.as_mut() {
                    lw.invoke_cpu_image_callbacks(&azul_core::gl::OptionGlContextPtr::None);
                }

                if let Some(ref layout_window) = self.common.layout_window {
                    let dom_id = DomId { inner: 0 };
                    // render_frame looks up the layout result itself; we only need
                    // to know one exists before computing window dims.
                    if layout_window.layout_results.contains_key(&dom_id) {
                        let ws = &layout_window.current_window_state;
                        let width = ws.size.dimensions.width;
                        let height = ws.size.dimensions.height;
                        let dpi = ws.size.dpi as f32 / 96.0;

                        if width > 0.0 && height > 0.0 {
                            // Shared CPU renderer (same path as headless + X11 +
                            // Wayland + macOS): damage diff + scroll-offset feed +
                            // thin-strip scroll-shift with eligibility + offset-aware
                            // render. Replaces the logic that used to live here and
                            // lacked all the scroll machinery (#13/#14).
                            self.cpu_backend.render_frame(
                                layout_window,
                                &layout_window.renderer_resources,
                                width,
                                height,
                                dpi,
                            );

                            // Blit the rendered pixmap to the window via
                            // StretchDIBits — PARTIALLY: only the present-damage
                            // rects are swizzled + uploaded (each as its own
                            // packed top-down DIB, sidestepping the top-down
                            // sub-rect ySrc quirk). The old code converted +
                            // blitted the FULL frame on every WM_PAINT.
                            // FrameDamage::None → ONE full-window rect: WM_PAINT
                            // can mean "uncovered, repaint everything", so an
                            // unchanged frame still re-presents in full from the
                            // retained pixmap (status-quo correctness).
                            if let Some(ref pixmap) = self.cpu_backend.last_frame {
                                let pw = pixmap.width() as i32;
                                let ph = pixmap.height() as i32;
                                let data = pixmap.data();

                                let rects = self
                                    .cpu_backend
                                    .last_present_damage
                                    .to_present_rects_physical(
                                        dpi,
                                        pixmap.width(),
                                        pixmap.height(),
                                        false,
                                    )
                                    .unwrap_or_else(|| {
                                        vec![(0, 0, pixmap.width(), pixmap.height())]
                                    });

                                unsafe {
                                    let hdc = (self.win32.user32.GetDC)(self.hwnd);
                                    if !hdc.is_null() {
                                        let src_stride = (pw as usize) * 4;
                                        for (rx, ry, rw, rh) in rects {
                                            // Pack + swizzle ONLY this rect's rows
                                            // (RGBA → BGRA) into the reused buffer.
                                            let rect_bytes =
                                                (rw as usize) * (rh as usize) * 4;
                                            self.bgra_buffer.resize(rect_bytes, 0);
                                            for row in 0..rh as usize {
                                                let so = (ry as usize + row) * src_stride
                                                    + (rx as usize) * 4;
                                                let doff = row * (rw as usize) * 4;
                                                let n = (rw as usize) * 4;
                                                for (s, d) in data[so..so + n]
                                                    .chunks_exact(4)
                                                    .zip(
                                                        self.bgra_buffer[doff..doff + n]
                                                            .chunks_exact_mut(4),
                                                    )
                                                {
                                                    d[0] = s[2]; // B
                                                    d[1] = s[1]; // G
                                                    d[2] = s[0]; // R
                                                    d[3] = s[3]; // A
                                                }
                                            }

                                            let bmi = dlopen::BitmapInfoHeader {
                                                biSize: core::mem::size_of::<
                                                    dlopen::BitmapInfoHeader,
                                                >(
                                                )
                                                    as u32,
                                                biWidth: rw as i32,
                                                biHeight: -(rh as i32), // negative = top-down
                                                biPlanes: 1,
                                                biBitCount: 32,
                                                biCompression: 0, // BI_RGB
                                                biSizeImage: 0,
                                                biXPelsPerMeter: 0,
                                                biYPelsPerMeter: 0,
                                                biClrUsed: 0,
                                                biClrImportant: 0,
                                            };

                                            (self.win32.gdi32.StretchDIBits)(
                                                hdc,
                                                rx as i32,
                                                ry as i32,
                                                rw as i32,
                                                rh as i32, // dest rect
                                                0,
                                                0,
                                                rw as i32,
                                                rh as i32, // src rect (packed DIB)
                                                self.bgra_buffer.as_ptr() as *const c_void,
                                                &bmi,
                                                dlopen::DIB_RGB_COLORS,
                                                dlopen::SRCCOPY,
                                            );
                                        }
                                        (self.win32.user32.ReleaseDC)(self.hwnd, hdc);
                                    }
                                }
                                rendered = true;
                            }
                            // (previous-display-list tracking now lives inside
                            // CpuBackend::render_frame.)
                        }
                    }
                }

                if !rendered {
                    // Fallback: fill window with white if CPU rendering not yet available
                    log_trace!(
                        LogCategory::Rendering,
                        "[Win32 CPU] layout not ready, skipping render"
                    );
                }
            }

            self.common.display_list_initialized = true;

            // Show window after first CPU render — but ONLY once a frame has
            // actually rendered content. Showing on a not-ready frame
            // (`rendered == false`, the "layout not ready" fallback above)
            // produces a white window that persists until the next repaint
            // (the reported "white first frame"). When the buffer isn't ready
            // yet, keep the window hidden and request another paint; we show on
            // the first frame that has content. (Invisible windows still mark
            // first_frame_shown so we don't loop forever.)
            if !self.first_frame_shown {
                if self.common.current_window_state.flags.is_visible && !rendered {
                    log_trace!(
                        LogCategory::Rendering,
                        "[Win32 CPU] first frame not rendered yet — deferring ShowWindow"
                    );
                    self.request_redraw();
                } else {
                    if self.common.current_window_state.flags.is_visible {
                        use azul_core::window::WindowFrame;
                        use dlopen::constants::{SW_MAXIMIZE, SW_MINIMIZE, SW_SHOWNORMAL};
                        let show_cmd = match self.common.current_window_state.flags.frame {
                            WindowFrame::Normal => SW_SHOWNORMAL,
                            WindowFrame::Minimized => SW_MINIMIZE,
                            WindowFrame::Maximized | WindowFrame::Fullscreen => SW_MAXIMIZE,
                        };
                        unsafe {
                            (self.win32.user32.ShowWindow)(self.hwnd, show_cmd);
                            (self.win32.user32.UpdateWindow)(self.hwnd);
                        }
                    }
                    self.first_frame_shown = true;
                }
            }

            // Scrollbar fade animation
            let needs_fade_frame = self.common.layout_window.as_ref()
                .map(|lw| lw.gpu_state_manager.scrollbar_fade_active)
                .unwrap_or(false);
            if needs_fade_frame {
                self.request_redraw();
            }

            // CI testing
            if std::env::var("AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER").is_ok() {
                std::process::exit(0);
            }

            return Ok(());
        }

        // GPU rendering path (WebRender)
        let RenderMode::Gpu { gl_context, hdc: stored_hdc } = &self.render_mode else {
            return Err(WindowError::PlatformError("Invalid render mode".into()));
        };
        let hglrc = *gl_context;
        let stored_hdc = *stored_hdc;

        let renderer = self
            .common
            .renderer
            .as_mut()
            .ok_or_else(|| WindowError::PlatformError("No renderer available".into()))?;

        unsafe {
            let hdc = if !stored_hdc.is_null() {
                stored_hdc
            } else {
                let new_hdc = (self.win32.user32.GetDC)(self.hwnd);
                if new_hdc.is_null() {
                    return Err(WindowError::PlatformError("Failed to get HDC".into()));
                }
                new_hdc
            };

            // Make OpenGL context current
            #[cfg(target_os = "windows")]
            {
                use winapi::um::wingdi::wglMakeCurrent;
                wglMakeCurrent(
                    hdc as winapi::shared::windef::HDC,
                    hglrc as winapi::shared::windef::HGLRC,
                );
            }

            if !layout_was_regenerated {
                // Early-return optimization
                if self.common.display_list_initialized {
                    let scroll_active = self.common.layout_window.as_ref()
                        .map(|lw| lw.scroll_manager.has_active_animations())
                        .unwrap_or(false);
                    let scrollbar_fade = self.common.layout_window.as_ref()
                        .map(|lw| lw.gpu_state_manager.scrollbar_fade_active)
                        .unwrap_or(false);
                    let virtual_view_pending = self.common.layout_window.as_ref()
                        .map(|lw| !lw.pending_virtual_view_updates.is_empty())
                        .unwrap_or(false);
                    // want_redraw: this WM_PAINT was explicitly requested
                    // (InvalidateRect from route_main_window_result — drag GPU
                    // transforms, GPU-value updates, display-list rebuilds).
                    // The skip-heuristic used to guess "did anything change?"
                    // from scroll/fade/vview only, so those explicitly
                    // requested redraws were SKIPPED — a dragged node's
                    // transform froze on Windows GPU. X11 gained the same
                    // `!want_redraw` guard earlier; this mirrors it.
                    let want_redraw =
                        self.needs_gpu_present || self.common.display_list_dirty;
                    if !want_redraw && !scroll_active && !scrollbar_fade && !virtual_view_pending {
                        log_trace!(
                            LogCategory::Rendering,
                            "[Win32] No visual changes — skipping GPU render"
                        );
                        if stored_hdc.is_null() {
                            (self.win32.user32.ReleaseDC)(self.hwnd, hdc);
                        }
                        return Ok(());
                    }
                }
                // A present is happening — the explicit request is satisfied.
                self.needs_gpu_present = false;

                if let (Some(layout_window), Some(render_api)) = (
                    self.common.layout_window.as_mut(),
                    self.common.render_api.as_mut(),
                ) {
                    {
                        #[cfg(feature = "std")]
                        let now = azul_core::task::Instant::System(std::time::Instant::now().into());
                        #[cfg(not(feature = "std"))]
                        let now = azul_core::task::Instant::Tick(azul_core::task::SystemTick { tick_counter: 0 });
                        let tick_result = layout_window.scroll_manager.tick(now);
                        if tick_result.needs_repaint {
                            layout_window.scroll_manager.calculate_scrollbar_states();
                        }
                    }

                    let has_virtual_view_updates = !layout_window.pending_virtual_view_updates.is_empty();
                    // display_list_dirty: the DL was regenerated internally
                    // WITHOUT a relayout (caret blink, selection, text
                    // undo/redo, ChangeNodeImage). The image-only transaction
                    // below skip_scene_builder()s, so the new DL would never
                    // reach WebRender — caret/selection/undo looked frozen in
                    // GPU mode. Consume the flag and take the full-frame path
                    // (mirrors the macOS + X11 consumers).
                    let display_list_dirty = self.common.display_list_dirty;
                    self.common.display_list_dirty = false;
                    if has_virtual_view_updates || display_list_dirty {
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
                                "[Win32] Failed to build lightweight transaction: {}",
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

            // First content frame: the display list submitted by
            // regenerate_layout() is built asynchronously on WebRender's
            // scene-builder thread. If we render before that build completes,
            // the first frame is empty and the window shows white until the
            // next repaint (the reported "white first frame", which a resize
            // happened to fix). Block until the scene is built so the first
            // VISIBLE frame has content. Only the first frame pays this cost —
            // later frames repaint on demand and never reach here.
            if layout_was_regenerated && !self.first_frame_shown {
                if let Some(render_api) = self.common.render_api.as_mut() {
                    render_api.flush_scene_builder();
                }
            }

            // Update and render WebRender
            let renderer = self
                .common
                .renderer
                .as_mut()
                .ok_or_else(|| WindowError::PlatformError("No renderer available".into()))?;
            renderer.update();

            let (width, height) = wcreate::get_client_rect(self.hwnd, &self.win32)?;
            let framebuffer_size =
                webrender::api::units::DeviceIntSize::new(width as i32, height as i32);

            let results = renderer
                .render(framebuffer_size, 0)
                .map_err(|e| WindowError::PlatformError(format!("Render error: {:?}", e)))?;

            // Store WebRender's dirty rects for per-rect InvalidateRect calls.
            let dpi_scale = self.common.current_window_state.size.dpi as f32 / 96.0;
            self.gpu_damage_rects = results.dirty_rects.iter().map(|dr| {
                azul_core::geom::LogicalRect {
                    origin: azul_core::geom::LogicalPosition {
                        x: dr.min.x as f32 / dpi_scale,
                        y: dr.min.y as f32 / dpi_scale,
                    },
                    size: azul_core::geom::LogicalSize {
                        width: dr.width() as f32 / dpi_scale,
                        height: dr.height() as f32 / dpi_scale,
                    },
                }
            }).collect();

            self.common.display_list_initialized = true;

            // Swap buffers
            #[cfg(target_os = "windows")]
            {
                if let Some(gl) = self.common.gl_context_ptr.as_ref() {
                    gl.finish();
                }
                use winapi::um::wingdi::SwapBuffers;
                SwapBuffers(hdc as winapi::shared::windef::HDC);
            }

            // Show window after first successful render
            if !self.first_frame_shown {
                if self.common.current_window_state.flags.is_visible {
                    if let Some(ref dwmapi) = self.win32.dwmapi_funcs {
                        (dwmapi.DwmFlush)();
                    }
                    use azul_core::window::WindowFrame;
                    use dlopen::constants::{SW_MAXIMIZE, SW_MINIMIZE, SW_SHOWNORMAL};
                    let show_cmd = match self.common.current_window_state.flags.frame {
                        WindowFrame::Normal => SW_SHOWNORMAL,
                        WindowFrame::Minimized => SW_MINIMIZE,
                        WindowFrame::Maximized | WindowFrame::Fullscreen => SW_MAXIMIZE,
                    };
                    (self.win32.user32.ShowWindow)(self.hwnd, show_cmd);
                    (self.win32.user32.UpdateWindow)(self.hwnd);
                }
                self.first_frame_shown = true;
            }

            if stored_hdc.is_null() {
                (self.win32.user32.ReleaseDC)(self.hwnd, hdc);
            }

            // Clean up old textures
            if let Some(ref layout_window) = self.common.layout_window {
                crate::desktop::gl_texture_integration::remove_old_gl_textures(
                    &layout_window.document_id,
                    layout_window.epoch,
                );
            }

            // Scrollbar fade animation
            let needs_fade_frame = self.common.layout_window.as_ref()
                .map(|lw| lw.gpu_state_manager.scrollbar_fade_active)
                .unwrap_or(false);
            if needs_fade_frame {
                self.request_redraw();
            }

            // CI testing
            if std::env::var("AZ_EXIT_SUCCESS_AFTER_FRAME_RENDER").is_ok() {
                std::process::exit(0);
            }

            Ok(())
        }
    }

    /// Regenerate layout (called after DOM changes)
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
            &self.font_registry,
            &self.common.system_style,
            &self.icon_provider,
            &mut debug_messages,
        
            self.common.next_relayout_reason,
        )?;
        // Consumed; reset so an untagged regen sees the implicit RefreshDom.
        self.common.next_relayout_reason =
            azul_core::callbacks::RelayoutReason::RefreshDom;

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

        // Update accessibility tree after layout (take, not clone — the
        // flush_a11y_tree_update hook drains the same slot at end-of-pass;
        // MWA-A3e, matches the wayland/macOS backends)
        #[cfg(feature = "a11y")]
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            if let Some(tree_update) = layout_window.a11y_manager.last_tree_update.take() {
                self.accessibility_adapter.update_tree(tree_update);
            }
        }

        // Send frame to WebRender (GPU mode only - CPU mode reads display list directly)
        if let RenderMode::Gpu { gl_context: hglrc, hdc: stored_hdc } = &self.render_mode {
            // Make OpenGL context current BEFORE generate_frame
            #[cfg(target_os = "windows")]
            unsafe {
                use winapi::um::wingdi::wglMakeCurrent;
                let hdc = if !stored_hdc.is_null() {
                    *stored_hdc
                } else {
                    (self.win32.user32.GetDC)(self.hwnd)
                };
                wglMakeCurrent(
                    hdc as winapi::shared::windef::HDC,
                    *hglrc as winapi::shared::windef::HGLRC,
                );
            }

            if let (Some(layout_window), Some(render_api), Some(document_id)) = (
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
                render_api.flush_scene_builder();
            }
        }

        // CPU mode: rebuild the shared hit-tester from the new layout so pointer
        // events resolve to the correct node. GPU mode uses WebRender's async
        // hit-tester (common.hit_tester) instead. Mirrors macOS/headless; without
        // this, clicks in the CPU-render fallback hit nothing and widget callbacks
        // (e.g. a button's on_click) never fire.
        if !matches!(self.render_mode, RenderMode::Gpu { .. }) {
            if let Some(ref mut cpu_ht) = self.common.cpu_hit_tester {
                if let Some(lw) = self.common.layout_window.as_ref() {
                    cpu_ht.rebuild_from_layout(&lw.layout_results);
                }
            }
        }

        // Drain lifecycle events (Mount / AfterMount / Unmount) produced by this
        // layout's reconciliation — the SAME step headless + X11 run. Without it,
        // EventFilter::Component(AfterMount) callbacks never fire on Windows (e.g.
        // the MapWidget's first tile fetch never starts). Windows already polls
        // background threads via its WM_TIMER (start_thread_poll_timer → SetTimer),
        // so once AfterMount spawns them their writebacks drain.
        let _ = self.dispatch_pending_lifecycle_events();

        // Phase 2: Post-Layout callback - sync IME position after layout (MOST IMPORTANT)
        self.update_ime_position_from_cursor();
        self.sync_ime_position_to_os();

        Ok(result)
    }

    /// Build + send the WebRender display-list transaction (GPU) / rebuild the CPU
    /// hit-tester after an *incremental* relayout — the "finalize" tail that
    /// `regenerate_layout()` runs after layout, MINUS the layout-callback /
    /// StyledDom rebuild.
    ///
    /// `incremental_relayout()` (called from the `ShouldIncrementalRelayout` event
    /// arm) re-runs layout on the existing StyledDom but, unlike
    /// `regenerate_layout()`, does NOT send a frame. The `frame_relayout_only`
    /// WM_PAINT branch calls this so the restyle still reaches `generate_frame` /
    /// the present path. Mirrors `regenerate_layout()`'s GPU `generate_frame` + CPU
    /// hit-tester tail.
    fn send_frame_after_incremental_relayout(&mut self) {
        // Send frame to WebRender (GPU mode only - CPU mode reads display list directly)
        if let RenderMode::Gpu { gl_context: hglrc, hdc: stored_hdc } = &self.render_mode {
            // Make OpenGL context current BEFORE generate_frame
            #[cfg(target_os = "windows")]
            unsafe {
                use winapi::um::wingdi::wglMakeCurrent;
                let hdc = if !stored_hdc.is_null() {
                    *stored_hdc
                } else {
                    (self.win32.user32.GetDC)(self.hwnd)
                };
                wglMakeCurrent(
                    hdc as winapi::shared::windef::HDC,
                    *hglrc as winapi::shared::windef::HGLRC,
                );
            }

            if let (Some(layout_window), Some(render_api), Some(document_id)) = (
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
                render_api.flush_scene_builder();
            }
        }

        // CPU mode: rebuild the shared hit-tester from the new layout so pointer
        // events resolve to the correct node after a restyle changes node rects.
        // GPU mode uses WebRender's async hit-tester instead.
        if !matches!(self.render_mode, RenderMode::Gpu { .. }) {
            if let Some(ref mut cpu_ht) = self.common.cpu_hit_tester {
                if let Some(lw) = self.common.layout_window.as_ref() {
                    cpu_ht.rebuild_from_layout(&lw.layout_results);
                }
            }
        }
    }

    /// Route a `ProcessEventResult` produced by a MAIN-WINDOW input handler
    /// (`WM_MOUSEMOVE` / `WM_LBUTTONDOWN` / `WM_LBUTTONUP` / `WM_KEYDOWN` /
    /// `WM_KEYUP` / `WM_CHAR` / `WM_MOUSEWHEEL` / `WM_IME_CHAR` / …) exactly the
    /// way the `WM_COMMAND` menu-callback arm routes its `event_result`.
    ///
    /// Before this, every main-window input handler did
    /// `if !matches!(result, DoNothing) { InvalidateRect }` and IGNORED the
    /// variant — so a restyle / runtime edit triggered from plain input
    /// (hover/focus CSS, `set_css_property`, `set_node_text` →
    /// `ShouldIncrementalRelayout`, or a `ShouldRegenerateDom*`) never set
    /// `frame_needs_regeneration` NOR took the incremental-relayout fast path, and
    /// WM_PAINT then just repainted the STALE layout.
    ///
    /// Mirrors the `WM_COMMAND` `match event_result` arm:
    /// - `ShouldIncrementalRelayout` → `incremental_relayout()` on the existing
    ///   StyledDom + `frame_relayout_only` + `frame_needs_regeneration`, then
    ///   invalidate (WM_PAINT's `frame_relayout_only` branch sends the frame).
    /// - `ShouldRegenerateDom* | UpdateHitTesterAndProcessAgain` →
    ///   `frame_needs_regeneration` + invalidate (full `regenerate_layout()` in
    ///   WM_PAINT).
    /// - `ShouldUpdateDisplayListCurrentWindow | ShouldReRenderCurrentWindow` →
    ///   invalidate only (preserves the old `!DoNothing` repaint).
    /// - `DoNothing` → nothing (preserves the old no-op).
    fn route_main_window_result(
        &mut self,
        hwnd: HWND,
        result: azul_core::events::ProcessEventResult,
    ) {
        use azul_core::events::ProcessEventResult;
        match result {
            ProcessEventResult::ShouldIncrementalRelayout => {
                // Restyle / runtime edit (hover/focus CSS, set_css_property,
                // set_node_text): re-run layout on the EXISTING StyledDom instead of
                // a full regenerate_layout() (which would re-invoke the user's
                // layout_callback + rebuild the StyledDom). Mirrors the macOS backend
                // + the WM_COMMAND menu arm. frame_relayout_only then makes WM_PAINT
                // skip regenerate_layout and only rebuild + send the WebRender
                // transaction.
                if let Some(layout_window) = self.common.layout_window.as_mut() {
                    let mut debug_messages = None;
                    if let Err(e) =
                        crate::desktop::shell2::common::layout::incremental_relayout(
                            layout_window,
                            &self.common.current_window_state,
                            &mut self.common.renderer_resources,
                            &mut debug_messages,
                        )
                    {
                        log_warn!(LogCategory::Layout, "Incremental relayout failed: {}", e);
                    }
                }
                self.common.frame_relayout_only = true;
                self.common.frame_needs_regeneration = true;
                unsafe {
                    (self.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
                }
            }
            ProcessEventResult::ShouldRegenerateDomCurrentWindow
            | ProcessEventResult::ShouldRegenerateDomAllWindows
            | ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                // RefreshDomAllWindows: ALSO mark every other registered
                // window (mirrors the X11 fan-out). Without this, a
                // popup/second-window callback mutating shared app data
                // (app-global undo/redo) refreshed only itself; every other
                // window kept showing the stale DOM until its own input.
                if result == ProcessEventResult::ShouldRegenerateDomAllWindows {
                    for other_hwnd in registry::get_all_window_handles() {
                        if other_hwnd == hwnd {
                            continue;
                        }
                        if let Some(wptr) = registry::get_window(other_hwnd) {
                            let w = unsafe { &mut *wptr };
                            w.common.frame_needs_regeneration = true;
                            unsafe {
                                (w.win32.user32.InvalidateRect)(other_hwnd, ptr::null(), 0);
                            }
                        }
                    }
                }
                self.common.frame_needs_regeneration = true;
                unsafe {
                    (self.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
                }
            }
            // ShouldUpdateDisplayListCurrentWindow: pending VirtualView updates are
            // queued in layout_window.pending_virtual_view_updates and processed in
            // the render path — no full layout regeneration needed.
            ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
            | ProcessEventResult::ShouldReRenderCurrentWindow => {
                // Mark the request so the GPU skip-heuristic can't drop it.
                self.needs_gpu_present = true;
                unsafe {
                    (self.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
                }
            }
            ProcessEventResult::DoNothing => {
                // No action needed (matches the old `!DoNothing` no-op).
            }
        }
    }

    // --- File drag-and-drop (OLE IDropTarget) ------------------------------
    //
    // These three handlers mirror the macOS `NSDraggingDestination` flow
    // (`macos/events.rs` `handle_file_drag_entered`/`handle_file_drag_exited`/
    // `handle_file_drop`): save-prev-state -> mutate the `FileDropManager` ->
    // refresh the hit test at the cached cursor -> `process_window_events(0)`.
    // `FileHover`/`FileHoverCancel`/`FileDrop` are DERIVED from the manager
    // state in `event_determination.rs`. The OLE `IDropTarget` COM object in
    // `windows::dnd` forwards `DragEnter`/`DragOver` -> entered,
    // `DragLeave` -> exited, `Drop` -> drop, then routes the returned
    // `ProcessEventResult` via `route_main_window_result`.

    /// Refresh the hit test at the cached cursor position (OLE drags do not
    /// deliver `WM_MOUSEMOVE`, so the cached position is the best available —
    /// same approach as the macOS backend, which reuses its cached cursor).
    /// MWA-B7: convert an OLE drag POINTL (screen px) to logical window
    /// coords and make it the current cursor position — no WM_MOUSEMOVE
    /// arrives during an OS drag, so the cached cursor is stale.
    fn set_drag_cursor_from_screen(&mut self, screen_x: i32, screen_y: i32) {
        use azul_core::window::CursorPosition;
        let mut pt = dlopen::POINT { x: screen_x, y: screen_y };
        unsafe {
            (self.win32.user32.ScreenToClient)(self.hwnd, &mut pt);
        }
        let hf = self.common.current_window_state.size.get_hidpi_factor();
        let pos = azul_core::geom::LogicalPosition::new(
            pt.x as f32 / hf.inner.get(),
            pt.y as f32 / hf.inner.get(),
        );
        self.common.current_window_state.mouse_state.cursor_position =
            CursorPosition::InWindow(pos);
    }

    /// MWA-B7: the OS drag moved over the window (OLE DragOver) — refresh
    /// position + hit test so HoveredFile re-targets the node under the
    /// drag. Previously DragOver did nothing positional at all.
    pub fn handle_file_drag_moved(&mut self, screen_pt: (i32, i32)) -> ProcessEventResult {
        self.common.previous_window_state = Some(self.common.current_window_state.clone());
        self.set_drag_cursor_from_screen(screen_pt.0, screen_pt.1);
        self.update_file_drag_hit_test();
        self.process_window_events(0)
    }

    fn update_file_drag_hit_test(&mut self) {
        use azul_core::window::CursorPosition;
        if let CursorPosition::InWindow(pos) =
            self.common.current_window_state.mouse_state.cursor_position
        {
            self.update_hit_test_at(pos);
        }
    }

    /// Process a file drag entering / moving over the window (emits
    /// `EventType::FileHover`).
    pub fn handle_file_drag_entered(
        &mut self,
        paths: Vec<String>,
        screen_pt: (i32, i32),
    ) -> ProcessEventResult {
        self.common.previous_window_state = Some(self.common.current_window_state.clone());
        self.set_drag_cursor_from_screen(screen_pt.0, screen_pt.1); // MWA-B7

        if !paths.is_empty() {
            if let Some(layout_window) = self.common.layout_window.as_mut() {
                // MWA-B7: pass EVERY path — multi-file drops were silently
                // truncated to the first file at this ingress.
                layout_window
                    .file_drop_manager
                    .set_hovered_files(paths.iter().map(|p| p.clone().into()).collect());
            }
        }

        self.update_file_drag_hit_test();
        self.process_window_events(0)
    }

    /// Process a file drag leaving the window without a drop (emits
    /// `EventType::FileHoverCancel`).
    pub fn handle_file_drag_exited(&mut self) -> ProcessEventResult {
        self.common.previous_window_state = Some(self.common.current_window_state.clone());

        // The Some -> None transition latches the one-shot hover-cancel flag.
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window.file_drop_manager.set_hovered_file(None);
        }

        let result = self.process_window_events(0);

        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window.file_drop_manager.clear_hover_cancelled();
        }

        result
    }

    /// Process a file drop (the user released the dragged files over the
    /// window — emits `EventType::FileDrop`).
    pub fn handle_file_drop(
        &mut self,
        paths: Vec<String>,
        screen_pt: (i32, i32),
    ) -> ProcessEventResult {
        self.common.previous_window_state = Some(self.common.current_window_state.clone());
        self.set_drag_cursor_from_screen(screen_pt.0, screen_pt.1); // MWA-B7

        if !paths.is_empty() {
            if let Some(layout_window) = self.common.layout_window.as_mut() {
                // MWA-B7: pass EVERY path — multi-file drops were silently
                // truncated to the first file at this ingress.
                layout_window
                    .file_drop_manager
                    .set_dropped_files(paths.iter().map(|p| p.clone().into()).collect());
            }
        }

        self.update_file_drag_hit_test();
        let result = self.process_window_events(0);

        // Clear dropped file after processing (one-shot event).
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window.file_drop_manager.set_dropped_file(None);
        }

        result
    }

    /// Register this window as an OLE drop target (modern hover + drop).
    /// Replaces the legacy `DragAcceptFiles`/`WM_DROPFILES` (drop-only) path.
    /// Must be called AFTER the window pointer is in the global registry, so
    /// the COM callbacks can resolve `Win32Window` from the HWND.
    pub fn register_drag_drop(&self) {
        dnd::register_drag_drop(self.hwnd);
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
    /// - title (via CreateWindowExW)
    /// - size (via CreateWindowExW)
    /// - position (via position_window_on_monitor)
    /// - decorations (via wcreate.rs style flags)
    /// - background_material (via apply_background_material)
    /// - is_visible (deferred to first_frame_shown logic)
    /// - frame (handled by first_frame_shown show command)
    ///
    /// This method applies the remaining fields and sets previous_window_state
    /// so that sync_window_state() works correctly for future changes.
    fn apply_initial_window_state(&mut self) {
        // is_always_on_top
        if self.common.current_window_state.flags.is_always_on_top {
            use dlopen::constants::*;
            unsafe {
                (self.win32.user32.SetWindowPos)(
                    self.hwnd, HWND_TOPMOST, 0, 0, 0, 0,
                    SWP_NOMOVE | SWP_NOSIZE,
                );
            }
        }

        // is_resizable (default is true via WS_THICKFRAME; apply if user wants non-resizable)
        if !self.common.current_window_state.flags.is_resizable {
            use dlopen::constants::*;
            unsafe {
                let style = (self.win32.user32.GetWindowLongPtrW)(self.hwnd, GWL_STYLE);
                let new_style = style & !((WS_THICKFRAME | WS_MAXIMIZEBOX) as isize);
                (self.win32.user32.SetWindowLongPtrW)(self.hwnd, GWL_STYLE, new_style);
                (self.win32.user32.SetWindowPos)(
                    self.hwnd, std::ptr::null_mut(), 0, 0, 0, 0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
                );
            }
        }

        // is_top_level
        if self.common.current_window_state.flags.is_top_level {
            let _ = self.set_is_top_level(true);
        }

        // prevent_system_sleep
        if self.common.current_window_state.flags.prevent_system_sleep {
            let _ = self.set_prevent_system_sleep(true);
        }

        // CRITICAL: Set previous_window_state so sync_window_state() works for future changes
        self.common.previous_window_state = Some(self.common.current_window_state.clone());
    }

    /// Synchronize window state with Windows OS
    ///
    /// Applies changes from current_window_state to the OS window.
    /// Called after callbacks have potentially modified window state.
    fn sync_window_state(&mut self) {
        use std::{ffi::OsStr, os::windows::ffi::OsStrExt};

        // Get copies of previous and current state to avoid borrow checker issues
        let (previous, current) = match &self.common.previous_window_state {
            Some(prev) => (prev.clone(), self.common.current_window_state.clone()),
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
                // Relative (child) windows are positioned once at creation and not
                // re-synced at runtime; Uninitialized lets the OS decide.
                WindowPosition::Uninitialized | WindowPosition::RelativeToParentWindow(_) => {}
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
                    WindowFrame::Fullscreen => {
                        // Borderless fullscreen: remove WS_OVERLAPPEDWINDOW, resize to monitor
                        let style = (self.win32.user32.GetWindowLongPtrW)(
                            self.hwnd, dlopen::constants::GWL_STYLE,
                        );
                        let new_style = style & !(dlopen::constants::WS_OVERLAPPEDWINDOW as isize);
                        (self.win32.user32.SetWindowLongPtrW)(
                            self.hwnd, dlopen::constants::GWL_STYLE, new_style,
                        );
                        (self.win32.user32.ShowWindow)(self.hwnd, SW_MAXIMIZE);
                    }
                    WindowFrame::Normal => {
                        if previous.flags.frame == WindowFrame::Fullscreen {
                            // Restore window style first
                            let style = (self.win32.user32.GetWindowLongPtrW)(
                                self.hwnd, dlopen::constants::GWL_STYLE,
                            );
                            let new_style = style | (dlopen::constants::WS_OVERLAPPEDWINDOW as isize);
                            (self.win32.user32.SetWindowLongPtrW)(
                                self.hwnd, dlopen::constants::GWL_STYLE, new_style,
                            );
                            (self.win32.user32.ShowWindow)(self.hwnd, SW_RESTORE);
                        } else if previous.flags.frame == WindowFrame::Minimized
                            || previous.flags.frame == WindowFrame::Maximized
                        {
                            (self.win32.user32.ShowWindow)(self.hwnd, SW_RESTORE);
                        }
                    }
                }
            }
        }

        // Decorations changed?
        if previous.flags.decorations != current.flags.decorations {
            use azul_core::window::WindowDecorations;
            use dlopen::constants::*;
            unsafe {
                let style = (self.win32.user32.GetWindowLongPtrW)(
                    self.hwnd, GWL_STYLE,
                );
                let new_style = match current.flags.decorations {
                    WindowDecorations::None => {
                        (style & !((WS_CAPTION | WS_SYSMENU | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX) as isize))
                            | WS_POPUP as isize
                    }
                    _ => {
                        // Normal, NoTitle, NoTitleAutoInject, NoControls all keep basic chrome
                        (style & !(WS_POPUP as isize))
                            | (WS_CAPTION | WS_SYSMENU | WS_THICKFRAME | WS_MINIMIZEBOX | WS_MAXIMIZEBOX) as isize
                    }
                };
                (self.win32.user32.SetWindowLongPtrW)(
                    self.hwnd, GWL_STYLE, new_style,
                );
                (self.win32.user32.SetWindowPos)(
                    self.hwnd, std::ptr::null_mut(), 0, 0, 0, 0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
                );
            }
        }

        // Resizable changed?
        if previous.flags.is_resizable != current.flags.is_resizable {
            use dlopen::constants::*;
            unsafe {
                let style = (self.win32.user32.GetWindowLongPtrW)(
                    self.hwnd, GWL_STYLE,
                );
                let new_style = if current.flags.is_resizable {
                    style | (WS_THICKFRAME | WS_MAXIMIZEBOX) as isize
                } else {
                    style & !((WS_THICKFRAME | WS_MAXIMIZEBOX) as isize)
                };
                (self.win32.user32.SetWindowLongPtrW)(
                    self.hwnd, GWL_STYLE, new_style,
                );
                (self.win32.user32.SetWindowPos)(
                    self.hwnd, std::ptr::null_mut(), 0, 0, 0, 0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
                );
            }
        }

        // Focus changed?
        if !previous.flags.has_focus && current.flags.has_focus {
            unsafe {
                (self.win32.user32.SetForegroundWindow)(self.hwnd);
            }
        }

        // Always-on-top changed?
        if previous.flags.is_always_on_top != current.flags.is_always_on_top {
            use dlopen::constants::*;
            unsafe {
                let insert_after = if current.flags.is_always_on_top {
                    HWND_TOPMOST
                } else {
                    HWND_NOTOPMOST
                };
                (self.win32.user32.SetWindowPos)(
                    self.hwnd, insert_after, 0, 0, 0, 0,
                    SWP_NOMOVE | SWP_NOSIZE,
                );
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
                log_error!(
                    LogCategory::Window,
                    "Failed to set prevent_system_sleep: {}",
                    e
                );
            }
        }

        // Background material changed? (Windows 11 Mica/Acrylic effects)
        if previous.flags.background_material != current.flags.background_material {
            self.apply_background_material(current.flags.background_material);
        }

        // Mouse cursor synchronization - compute from current hit test
        if let Some(layout_window) = self.common.layout_window.as_ref() {
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
        use dlopen::{
            DWMWA_SYSTEMBACKDROP_TYPE, DWM_BB_BLURREGION, DWM_BB_ENABLE, DWM_BLURBEHIND,
            DWM_SYSTEMBACKDROP_TYPE, MARGINS,
        };

        let dwmapi = match self.win32.dwmapi_funcs.as_ref() {
            Some(d) => d,
            None => {
                log_debug!(
                    LogCategory::Platform,
                    "[Windows] dwmapi not available, skipping background material"
                );
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
                    log_debug!(
                        LogCategory::Platform,
                        "[Windows] DwmEnableBlurBehindWindow failed with HRESULT 0x{:08X}",
                        result as u32
                    );
                } else {
                    log_debug!(
                        LogCategory::Platform,
                        "[Windows] Enabled transparent background via DwmEnableBlurBehindWindow"
                    );
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

                log_debug!(
                    LogCategory::Platform,
                    "[Windows] Disabled transparency effects"
                );
                return;
            }

            // Map remaining WindowBackgroundMaterial values to DWM backdrop type
            // These are Windows 11 22H2+ Mica/Acrylic effects
            let backdrop_type = match material {
                WindowBackgroundMaterial::Sidebar
                | WindowBackgroundMaterial::Menu
                | WindowBackgroundMaterial::HUD => DWM_SYSTEMBACKDROP_TYPE::DWMSBT_TRANSIENTWINDOW, // Acrylic
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
                log_debug!(
                    LogCategory::Platform,
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
                log_warn!(
                    LogCategory::Platform,
                    "[Windows] DwmExtendFrameIntoClientArea failed: 0x{:08X}",
                    extend_result as u32
                );
            }

            log_debug!(
                LogCategory::Platform,
                "[Windows] Applied background material {:?} (backdrop type {:?})",
                material,
                backdrop_type
            );
        }
    }

    // Query WebRender hit-tester for scrollbar hits at given position
    //
    // NOTE: perform_scrollbar_hit_test(), handle_scrollbar_click(), and handle_scrollbar_drag()
    // are now provided by the PlatformWindow trait as default methods.
    // The trait methods are cross-platform and work identically.
    // See dll/src/desktop/shell2/common/event.rs for the implementation.
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
        self.common.current_window_state.size.get_hidpi_factor()
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
            // No layout regeneration happened here, but the transaction was already
            // sent when frame_needs_regeneration was processed in WM_PAINT.
            // If no transaction was pending, this is a no-op render.
            if let Err(e) = self.render_and_present(false) {
                log_error!(LogCategory::Rendering, "Failed to present frame: {:?}", e);
            }
        }

        // Check for close request
        if self.common.current_window_state.flags.close_requested {
            self.common.current_window_state.flags.close_requested = false;
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
            .common
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
                let context_menu = if let Some(ref lw) = self.common.layout_window {
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
                    if self.common.current_window_state.flags.use_native_context_menus {
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
                dlopen::constants::TPM_RIGHTBUTTON | dlopen::constants::TPM_LEFTALIGN,
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
        let parent_pos = match self.common.current_window_state.position {
            azul_core::window::WindowPosition::Initialized(pos) => {
                LogicalPosition::new(pos.x as f32, pos.y as f32)
            }
            _ => LogicalPosition::new(0.0, 0.0),
        };

        // Create menu window options using the unified menu system
        // This is identical to how menu bar menus work, but with cursor_pos instead of trigger_rect
        let menu_options = crate::desktop::menu::show_menu(
            menu.clone(),
            self.common.system_style.clone(),
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

/// Human-readable name for a Win32 window message (for raw-event tracing).
fn win32_msg_name(msg: u32) -> &'static str {
    match msg {
        0x0001 => "WM_CREATE",
        0x0002 => "WM_DESTROY",
        0x0003 => "WM_MOVE",
        0x0005 => "WM_SIZE",
        0x0007 => "WM_SETFOCUS",
        0x0008 => "WM_KILLFOCUS",
        0x000F => "WM_PAINT",
        0x0010 => "WM_CLOSE",
        0x0014 => "WM_ERASEBKGND",
        0x0024 => "WM_GETMINMAXINFO",
        0x0046 => "WM_WINDOWPOSCHANGING",
        0x0047 => "WM_WINDOWPOSCHANGED",
        0x0084 => "WM_NCHITTEST",
        0x0100 => "WM_KEYDOWN",
        0x0101 => "WM_KEYUP",
        0x0102 => "WM_CHAR",
        0x0113 => "WM_TIMER",
        0x0200 => "WM_MOUSEMOVE",
        0x0201 => "WM_LBUTTONDOWN",
        0x0202 => "WM_LBUTTONUP",
        0x0204 => "WM_RBUTTONDOWN",
        0x0205 => "WM_RBUTTONUP",
        0x020A => "WM_MOUSEWHEEL",
        0x020E => "WM_MOUSEHWHEEL",
        0x02E0 => "WM_DPICHANGED",
        _ => "WM_other",
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

// Cached function pointers — set once during WM_NCCREATE so subsequent
// messages avoid a full Win32Libraries::load() (multiple dlopen calls) per call.
static CACHED_GET_WINDOW_LONG_PTR_W: std::sync::atomic::AtomicPtr<core::ffi::c_void> =
    std::sync::atomic::AtomicPtr::new(core::ptr::null_mut());
static CACHED_DEF_WINDOW_PROC_W: std::sync::atomic::AtomicPtr<core::ffi::c_void> =
    std::sync::atomic::AtomicPtr::new(core::ptr::null_mut());

// Win32 message handler
impl Win32Window {
    /// Feed a WM_POINTER touch/pen sample into azul's input state. WM_POINTER
    /// fires alongside the promoted WM_MOUSE messages (which drive cursor +
    /// click), so this only adds the extra data Windows doesn't promote: pen
    /// pressure/tilt/eraser -> the gesture manager's pen state, and per-finger
    /// touch points -> the window's `touch_state`. `is_up` = WM_POINTERUP.
    /// Mirrors the iOS/Android pen+touch feed; no-op on pre-Win8 (fns absent).
    unsafe fn feed_pointer(&mut self, hwnd: HWND, pointer_id: u32, is_up: bool) {
        use winapi::um::winuser::{
            PEN_FLAG_BARREL, PEN_FLAG_ERASER, POINTER_FLAG_INCONTACT, POINTER_PEN_INFO,
            POINTER_TOUCH_INFO, PT_PEN, PT_TOUCH,
        };
        let get_type = match self.win32.user32.GetPointerType {
            Some(f) => f,
            None => return,
        };
        let mut ptype: u32 = 0;
        if get_type(pointer_id, &mut ptype) == 0 {
            return;
        }
        let hf = self
            .common
            .current_window_state
            .size
            .get_hidpi_factor()
            .inner
            .get();

        if ptype == PT_PEN {
            let get_pen = match self.win32.user32.GetPointerPenInfo {
                Some(f) => f,
                None => return,
            };
            let mut pi: POINTER_PEN_INFO = core::mem::zeroed();
            if get_pen(pointer_id, &mut pi) == 0 {
                return;
            }
            let mut pt = dlopen::POINT {
                x: pi.pointerInfo.ptPixelLocation.x,
                y: pi.pointerInfo.ptPixelLocation.y,
            };
            (self.win32.user32.ScreenToClient)(hwnd, &mut pt);
            let pos = azul_core::geom::LogicalPosition::new(pt.x as f32 / hf, pt.y as f32 / hf);
            let in_contact =
                !is_up && (pi.pointerInfo.pointerFlags & POINTER_FLAG_INCONTACT) != 0;
            if let Some(lw) = self.common.layout_window.as_mut() {
                // Windows pen: pressure 0..1024, tiltX/Y already in degrees, rotation degrees.
                lw.gesture_drag_manager.update_pen_state_full(
                    pos,
                    pi.pressure as f32 / 1024.0,
                    (pi.tiltX as f32, pi.tiltY as f32),
                    in_contact,
                    (pi.penFlags & PEN_FLAG_ERASER) != 0,
                    (pi.penFlags & PEN_FLAG_BARREL) != 0,
                    pointer_id as u64,
                    0.0,
                    (pi.rotation as f32) * core::f32::consts::PI / 180.0,
                    0,
                );
            }
        } else if ptype == PT_TOUCH {
            let get_touch = match self.win32.user32.GetPointerTouchInfo {
                Some(f) => f,
                None => return,
            };
            let mut ti: POINTER_TOUCH_INFO = core::mem::zeroed();
            if get_touch(pointer_id, &mut ti) == 0 {
                return;
            }
            let mut pt = dlopen::POINT {
                x: ti.pointerInfo.ptPixelLocation.x,
                y: ti.pointerInfo.ptPixelLocation.y,
            };
            (self.win32.user32.ScreenToClient)(hwnd, &mut pt);
            let pos = azul_core::geom::LogicalPosition::new(pt.x as f32 / hf, pt.y as f32 / hf);
            let force = if ti.pressure > 0 {
                ti.pressure as f32 / 1024.0
            } else {
                0.5
            };
            use azul_core::window::{TouchPoint, TouchPointVec};
            let ts = &mut self.common.current_window_state.touch_state;
            let mut pts: Vec<TouchPoint> = ts.touch_points.clone().into_library_owned_vec();
            let was_present = pts.iter().any(|p| p.id == pointer_id as u64);
            pts.retain(|p| p.id != pointer_id as u64);
            if !is_up {
                pts.push(TouchPoint {
                    id: pointer_id as u64,
                    position: pos,
                    force,
                });
            }
            ts.touch_points = TouchPointVec::from_vec(pts);
            ts.num_touches = ts.touch_points.len();
            // MWA-B4: per-finger gesture sessions (pinch/rotate need two
            // live sessions). Screen position from the raw pixel location.
            {
                let now = azul_core::task::Instant::from(std::time::Instant::now());
                let screen = azul_core::geom::LogicalPosition::new(
                    ti.pointerInfo.ptPixelLocation.x as f32 / hf,
                    ti.pointerInfo.ptPixelLocation.y as f32 / hf,
                );
                let window_position = self.common.current_window_state.position;
                if let Some(lw) = self.common.layout_window.as_mut() {
                    let gid = pointer_id as u64;
                    if is_up {
                        lw.gesture_drag_manager.touch_up(gid, pos, now, screen);
                    } else if was_present {
                        lw.gesture_drag_manager.touch_move(gid, pos, now, screen);
                    } else {
                        lw.gesture_drag_manager
                            .touch_down(gid, pos, now, window_position, screen);
                    }
                }
            }
        }
    }
}

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
    const WM_MOVE: u32 = 0x0003;
    const WM_MOUSEMOVE: u32 = 0x0200;
    const WM_LBUTTONDOWN: u32 = 0x0201;
    const WM_LBUTTONUP: u32 = 0x0202;
    const WM_RBUTTONDOWN: u32 = 0x0204;
    const WM_RBUTTONUP: u32 = 0x0205;
    const WM_MBUTTONDOWN: u32 = 0x0207;
    const WM_MBUTTONUP: u32 = 0x0208;
    const WM_MOUSEWHEEL: u32 = 0x020A;
    const WM_APP_FRAME_READY_LOCAL: u32 = WM_APP_FRAME_READY;
    const WM_MOUSEHWHEEL: u32 = 0x020E;
    const WM_GETMINMAXINFO: u32 = 0x0024;
    const WM_SETTINGCHANGE: u32 = 0x001A;
    const WM_THEMECHANGED: u32 = 0x031A;
    const WM_SETCURSOR: u32 = 0x0020;
    const HTCLIENT: isize = 1;
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
    const WM_DISPLAYCHANGE: u32 = 0x007E;
    const WM_POINTERUPDATE: u32 = 0x0245;
    const WM_POINTERDOWN: u32 = 0x0246;
    const WM_POINTERUP: u32 = 0x0247;

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

        let dpi = DpiFunctions::init();
        dpi.enable_non_client_dpi_scaling(hwnd as *mut _);

        let createstruct = lparam as *mut CREATESTRUCTW;
        let data_ptr = (*createstruct).lpCreateParams;
        (win32.user32.SetWindowLongPtrW)(hwnd, GWLP_USERDATA, data_ptr as isize);

        CACHED_GET_WINDOW_LONG_PTR_W.store(
            win32.user32.GetWindowLongPtrW as *mut core::ffi::c_void,
            std::sync::atomic::Ordering::Release,
        );
        CACHED_DEF_WINDOW_PROC_W.store(
            win32.user32.DefWindowProcW as *mut core::ffi::c_void,
            std::sync::atomic::Ordering::Release,
        );

        return (win32.user32.DefWindowProcW)(hwnd, msg, wparam, lparam);
    }

    // Use cached pointers (set during WM_NCCREATE) to avoid a full Win32Libraries::load()
    // — that load opens user32/kernel32/gdi32/… via dlopen on every message, which is
    // measurable overhead under high-frequency input (WM_MOUSEMOVE, WM_TIMER, etc.).
    let get_wlp = CACHED_GET_WINDOW_LONG_PTR_W.load(std::sync::atomic::Ordering::Acquire);
    let def_wp = CACHED_DEF_WINDOW_PROC_W.load(std::sync::atomic::Ordering::Acquire);

    if get_wlp.is_null() || def_wp.is_null() {
        return default_window_proc(hwnd, msg, wparam, lparam);
    }

    let get_window_long_ptr_w: unsafe extern "system" fn(HWND, i32) -> isize =
        core::mem::transmute(get_wlp);
    let def_window_proc_w: unsafe extern "system" fn(
        HWND,
        u32,
        dlopen::WPARAM,
        dlopen::LPARAM,
    ) -> dlopen::LRESULT = core::mem::transmute(def_wp);

    let window_ptr = get_window_long_ptr_w(hwnd, GWLP_USERDATA) as *mut Win32Window;

    if window_ptr.is_null() {
        // No user data yet, use default processing
        return def_window_proc_w(hwnd, msg, wparam, lparam);
    }

    let window = &mut *window_ptr;

    // Raw-event trace: every incoming window message, so the per-OS run shows
    // how raw system events map to app actions and surfaces message storms
    // (e.g. a flood of WM_WINDOWPOSCHANGED = a geometry feedback loop). Cheap
    // (trace-level, no-op unless logging is enabled). Mirrors the X11 [x11 ev] trace.
    crate::plog_trace!("[win32 ev] raw {} (0x{:04X})", win32_msg_name(msg), msg);

    // Handle messages
    match msg {
        WM_CREATE => {
            log_debug!(LogCategory::Window, "[Win32] WM_CREATE - Window created");
            0
        }

        WM_DESTROY => {
            log_debug!(LogCategory::Window, "[Win32] WM_DESTROY - Window destroyed");
            // Revoke the OLE drop target BEFORE the HWND dies (releases the
            // COM ref held by RegisterDragDrop). Must happen here, not in the
            // registry cleanup, because RevokeDragDrop needs a live HWND.
            dnd::revoke_drag_drop(hwnd);
            // Window destroyed - unregister from global registry
            window.is_open = false;
            registry::unregister_window(hwnd);
            log_debug!(LogCategory::Window, "[Win32] Window unregistered, remaining windows: {}", registry::window_count());
            0
        }

        WM_CLOSE => {
            log_debug!(LogCategory::Window, "[Win32] WM_CLOSE - Close requested");
            // User clicked close button - set close_requested flag
            // and process callbacks to allow cancellation
            window.common.current_window_state.flags.close_requested = true;

            // Process window events to trigger OnWindowClose callback.
            // A close callback can cancel the close AND restyle (e.g. show a styled
            // "unsaved changes" prompt) — route the result so any restyle takes the
            // incremental fast path / repaints, same as every other input handler.
            // If the close proceeds below, the InvalidateRect is harmless.
            let result = window.process_window_events(0);
            window.route_main_window_result(hwnd, result);

            // Check if callback cancelled the close
            if window.common.current_window_state.flags.close_requested {
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
            // Determine if layout needs regeneration (DOM changed)
            let layout_was_regenerated = if window.common.frame_relayout_only {
                // Restyle / runtime edit: incremental_relayout() already re-ran layout
                // on the existing StyledDom in the ShouldIncrementalRelayout event arm.
                // Skip the full regenerate_layout() (no layout_callback / StyledDom
                // rebuild), but still build + send the WebRender display-list
                // transaction (GPU) / rebuild the CPU hit-tester so the restyle reaches
                // the screen — render_and_present(true) then presents the new scene.
                window.send_frame_after_incremental_relayout();
                window.common.frame_relayout_only = false;
                window.common.frame_needs_regeneration = false;
                true
            } else if window.common.frame_needs_regeneration {
                if let Err(e) = window.regenerate_layout() {
                    log_error!(LogCategory::Layout, "Layout regeneration error: {:?}", e);
                }
                window.common.frame_needs_regeneration = false;
                true
            } else {
                false
            };

            match window.render_and_present(layout_was_regenerated) {
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

            // SIZE_MINIMIZED delivers 0x0 and used to fall through the size
            // gate without recording ANY state change: frame stayed Normal,
            // timers kept invalidating, and WM_PAINT kept doing full CPU
            // renders + blits of an invisible window. Record the minimize
            // through the diff pipeline (Minimize callbacks fire, render
            // paths see frame == Minimized) and skip the resize handling.
            const SIZE_MINIMIZED: usize = 1;
            if (wparam as usize) == SIZE_MINIMIZED {
                use azul_core::window::WindowFrame;
                window.common.previous_window_state =
                    Some(window.common.current_window_state.clone());
                window.common.current_window_state.flags.frame = WindowFrame::Minimized;
                let r = window.process_window_events(0);
                window.route_main_window_result(hwnd, r);
                return 0;
            }

            if width > 0 && height > 0 {
                use azul_core::{geom::PhysicalSizeU32, window::WindowSize};

                let physical_size = PhysicalSizeU32::new(width, height);
                let dpi = window.common.current_window_state.size.dpi;
                let hidpi_factor = dpi as f32 / 96.0;
                let logical_size = physical_size.to_logical(hidpi_factor);

                // Store old context for comparison
                let old_context = window.dynamic_selector_context.clone();

                // Update dynamic selector context with new viewport dimensions
                window.dynamic_selector_context.viewport_width = logical_size.width;
                window.dynamic_selector_context.viewport_height = logical_size.height;
                window.dynamic_selector_context.orientation =
                    if logical_size.width > logical_size.height {
                        azul_css::dynamic_selector::OrientationType::Landscape
                    } else {
                        azul_css::dynamic_selector::OrientationType::Portrait
                    };

                // Check if any CSS breakpoints were crossed
                if old_context
                    .viewport_breakpoint_changed(
                        &window.dynamic_selector_context,
                        crate::desktop::shell2::common::CSS_BREAKPOINTS,
                    )
                {
                    log_debug!(
                        LogCategory::Layout,
                        "[WM_SIZE] Breakpoint crossed: {}x{} -> {}x{}",
                        old_context.viewport_width,
                        old_context.viewport_height,
                        window.dynamic_selector_context.viewport_width,
                        window.dynamic_selector_context.viewport_height
                    );
                }

                // Update window state
                let mut new_window_state = window.common.current_window_state.clone();
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

                let hidpi_factor = window.common.current_window_state.size.get_hidpi_factor();

                // Update WebRender document view (GPU mode only — CPU mode has no render_api)
                if let (Some(render_api), Some(document_id)) = (
                    window.common.render_api.as_mut(),
                    window.common.document_id,
                ) {
                    let mut txn = WrTransaction::new();
                    // NOTE: azul_layout outputs coordinates in CSS pixels (logical pixels).
                    txn.set_document_view(
                        DeviceIntRect::from_size(DeviceIntSize::new(width as i32, height as i32)),
                        DevicePixelScale::new(hidpi_factor.inner.get()),
                    );
                    render_api.send_transaction(wr_translate_document_id(document_id), txn);
                }

                // F4: WM_SIZE is an OS-reported geometry/frame change (already
                // applied by the OS), so set BOTH current AND the sync baseline
                // (previous) to the new state. Setting previous to the OLD state
                // would leave a non-zero diff that sync_window_state() echoes back
                // via SetWindowPos — the OS→app→OS loop. (Source = Os, not App.)
                window.common.current_window_state = new_window_state.clone();
                window.common.previous_window_state = Some(new_window_state);

                // Tag the next regen as a resize so the user's layout()
                // callback can detect it via `info.relayout_reason()`.
                window.common.next_relayout_reason =
                    azul_core::callbacks::RelayoutReason::Resize;

                // Resize requires full display list rebuild
                window.common.frame_needs_regeneration = true;

                // Request redraw (will trigger regenerate_layout in WM_PAINT)
                (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
            }

            0
        }

        WM_MOVE => {
            // Window moved — update current_window_state.position from OS.
            // This is critical for incremental titlebar drag: the callback reads
            // current_window_state.position and adds the frame delta, so if the
            // OS independently moves the window (DPI change, clamping, snap),
            // the position must reflect the actual OS value.
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;
            let pos = azul_core::window::WindowPosition::Initialized(
                azul_core::geom::PhysicalPositionI32::new(x, y),
            );
            // F4: position REPORTED by the OS (source = Os) — acknowledge into both
            // current and the sync baseline so sync_window_state() doesn't echo it
            // back via SetWindowPos (the OS→app→OS geometry loop).
            window.common.update_window_state(
                crate::desktop::shell2::common::event::WindowStateSource::Os,
                |ws| ws.position = pos,
            );

            // Detect which monitor the window is on via MonitorFromWindow
            // This updates monitor_id so that DPI/MonitorChanged events can fire
            {
                const MONITOR_DEFAULTTONEAREST: u32 = 2;
                extern "system" {
                    fn MonitorFromWindow(hwnd: *mut core::ffi::c_void, flags: u32) -> *mut core::ffi::c_void;
                    fn GetMonitorInfoW(hmonitor: *mut core::ffi::c_void, lpmi: *mut MonitorInfoExW) -> i32;
                }
                #[repr(C)]
                #[allow(non_snake_case)]
                struct Rect { left: i32, top: i32, right: i32, bottom: i32 }
                #[repr(C)]
                #[allow(non_snake_case)]
                struct MonitorInfoExW {
                    cbSize: u32,
                    rcMonitor: Rect,
                    rcWork: Rect,
                    dwFlags: u32,
                    szDevice: [u16; 32],
                }
                let hmonitor = unsafe { MonitorFromWindow(hwnd as _, MONITOR_DEFAULTTONEAREST as u32) };
                if !hmonitor.is_null() {
                    let mut mi = MonitorInfoExW {
                        cbSize: core::mem::size_of::<MonitorInfoExW>() as u32,
                        rcMonitor: Rect { left: 0, top: 0, right: 0, bottom: 0 },
                        rcWork: Rect { left: 0, top: 0, right: 0, bottom: 0 },
                        dwFlags: 0,
                        szDevice: [0u16; 32],
                    };
                    if unsafe { GetMonitorInfoW(hmonitor, &mut mi) } != 0 {
                        // Find matching monitor in cache by position
                        if let Some(ref lw) = window.common.layout_window {
                            if let Ok(guard) = lw.monitors.lock() {
                                for m in guard.as_ref().iter() {
                                    if m.position.x == mi.rcMonitor.left as isize
                                        && m.position.y == mi.rcMonitor.top as isize
                                    {
                                        window.common.current_window_state.monitor_id =
                                            azul_css::corety::OptionU32::Some(m.monitor_id.index as u32);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if let Some(ref mut lw) = window.common.layout_window {
                lw.current_window_state.position = pos;
                lw.current_window_state.monitor_id = window.common.current_window_state.monitor_id;
            }
            0
        }

        WM_MOUSEMOVE => {
            // Mouse moved - similar to macOS handle_mouse_move
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;

            use azul_core::{geom::LogicalPosition, window::CursorPosition};

            let hidpi_factor = window.common.current_window_state.size.get_hidpi_factor();
            let logical_pos = LogicalPosition::new(
                x as f32 / hidpi_factor.inner.get(),
                y as f32 / hidpi_factor.inner.get(),
            );

            // Handle active scrollbar drag (special case - not part of normal event system)
            if window.common.scrollbar_drag_state.is_some() {
                // Route the result! handle_scrollbar_drag returns
                // ShouldReRenderCurrentWindow after gpu_scroll — discarding it
                // (`let _`) meant NO InvalidateRect: the content scrolled
                // internally but the screen froze until an unrelated event.
                let r = PlatformWindow::handle_scrollbar_drag(&mut *window, logical_pos);
                window.route_main_window_result(hwnd, r);
                return 0;
            }

            // Save previous state BEFORE making changes
            window.common.previous_window_state = Some(window.common.current_window_state.clone());

            // Update mouse state
            window.common.current_window_state.mouse_state.cursor_position =
                CursorPosition::InWindow(logical_pos);

            // Record input sample for gesture detection (movement during button press)
            let button_state = if window.common.current_window_state.mouse_state.left_down {
                BUTTON_STATE_LEFT
            } else {
                BUTTON_STATE_NONE
            };

            // Use GetCursorPos for accurate screen-absolute position (physical pixels → logical)
            let screen_pos = {
                let mut pt = dlopen::POINT { x: 0, y: 0 };
                unsafe { (window.win32.user32.GetCursorPos)(&mut pt); }
                let hf = hidpi_factor.inner.get();
                azul_core::geom::LogicalPosition::new(pt.x as f32 / hf, pt.y as f32 / hf)
            };
            window.record_input_sample(logical_pos, button_state, false, false, Some(screen_pos));

            // CPU mode (no WR hit_tester/document_id): resolve the hit test via
            // the shared perform_hit_test → cpu_hit_tester path. Without this,
            // events dispatched against a stale/empty hover state — hover CSS,
            // clicks, wheel targeting and MouseEnter/Leave were all dead in the
            // Windows CPU fallback (the GPU-gated block below has no CPU arm).
            if window.common.hit_tester.is_none() || window.common.document_id.is_none() {
                PlatformWindow::update_hit_test_at(&mut *window, logical_pos);
            }

            // Update hit test (GPU mode only — CPU mode handled above)
            if let Some(ref mut layout_window) = window.common.layout_window {
                if let (Some(hit_tester), Some(doc_id)) = (
                    window.common.hit_tester.as_mut(),
                    window.common.document_id,
                ) {
                    use crate::desktop::wr_translate2::fullhittest_new_webrender;

                    let hit_tester = hit_tester.resolve();
                    let hit_test = fullhittest_new_webrender(
                        &*hit_tester,
                        doc_id,
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
                    if window.common.current_window_state.mouse_state.mouse_cursor_type != new {
                        window.common.current_window_state.mouse_state.mouse_cursor_type = new;
                        window.set_cursor(new_cursor_type);
                    }
                }
            }

            // V2 system will detect MouseOver/MouseEnter/MouseLeave/Drag from state diff
            let result = window.process_window_events(0);

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
            window.route_main_window_result(hwnd, result);

            0
        }

        WM_MOUSELEAVE => {
            // Mouse left the window area
            // Save previous state
            window.common.previous_window_state = Some(window.common.current_window_state.clone());

            // Get last known position, or default
            let last_pos = match window.common.current_window_state.mouse_state.cursor_position {
                CursorPosition::InWindow(pos) => pos,
                CursorPosition::OutOfWindow(pos) => pos,
                CursorPosition::Uninitialized => LogicalPosition::new(0.0, 0.0),
            };

            // Clear mouse position (mouse is outside window)
            use azul_core::{geom::LogicalPosition, window::CursorPosition};
            window.common.current_window_state.mouse_state.cursor_position =
                CursorPosition::OutOfWindow(last_pos);

            // Process events - this will trigger MouseLeave callbacks
            let result = window.process_window_events(0);

            // Request redraw if needed to clear hover states
            window.route_main_window_result(hwnd, result);

            0
        }

        WM_POINTERDOWN | WM_POINTERUPDATE => {
            // Touch + pen (Win8+). Promoted WM_MOUSE messages still drive
            // cursor/click; this adds pressure/tilt + multi-touch state.
            let pointer_id = (wparam & 0xFFFF) as u32;
            window.feed_pointer(hwnd, pointer_id, false);
            def_window_proc_w(hwnd, msg, wparam, lparam)
        }
        WM_POINTERUP => {
            let pointer_id = (wparam & 0xFFFF) as u32;
            window.feed_pointer(hwnd, pointer_id, true);
            def_window_proc_w(hwnd, msg, wparam, lparam)
        }

        WM_LBUTTONDOWN => {
            // Left mouse button down - similar to macOS handle_mouse_down
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;

            use azul_core::{geom::LogicalPosition, window::CursorPosition};

            let hidpi_factor = window.common.current_window_state.size.get_hidpi_factor();
            let logical_pos = LogicalPosition::new(
                x as f32 / hidpi_factor.inner.get(),
                y as f32 / hidpi_factor.inner.get(),
            );

            // Check for scrollbar hit FIRST (before state changes)
            if let Some(scrollbar_hit_id) =
                PlatformWindow::perform_scrollbar_hit_test(&*window, logical_pos)
            {
                let r = PlatformWindow::handle_scrollbar_click(
                    &mut *window,
                    scrollbar_hit_id,
                    logical_pos,
                );
                // Capture the mouse so a fast thumb-drag leaving the client
                // area keeps receiving WM_MOUSEMOVE (this early-return used to
                // skip the SetCapture further down, so the drag died at the
                // window edge — and WM_LBUTTONUP's ReleaseCapture released a
                // capture that was never taken). Route the result so the
                // track-click jump repaints immediately.
                unsafe {
                    (window.win32.user32.SetCapture)(hwnd);
                }
                window.route_main_window_result(hwnd, r);
                return 0;
            }

            // Save previous state BEFORE making changes
            window.common.previous_window_state = Some(window.common.current_window_state.clone());

            // Update mouse state
            window.common.current_window_state.mouse_state.cursor_position =
                CursorPosition::InWindow(logical_pos);
            window.common.current_window_state.mouse_state.left_down = true;

            // Record input sample for gesture detection (button down starts new session)
            // Use GetCursorPos for accurate screen-absolute position (physical pixels → logical)
            let screen_pos = {
                let mut pt = dlopen::POINT { x: 0, y: 0 };
                unsafe { (window.win32.user32.GetCursorPos)(&mut pt); }
                let hf = hidpi_factor.inner.get();
                azul_core::geom::LogicalPosition::new(pt.x as f32 / hf, pt.y as f32 / hf)
            };
            window.record_input_sample(logical_pos, BUTTON_STATE_LEFT, true, false, Some(screen_pos));

            // CPU mode (no WR hit_tester/document_id): resolve the hit test via
            // the shared perform_hit_test → cpu_hit_tester path. Without this,
            // events dispatched against a stale/empty hover state — hover CSS,
            // clicks, wheel targeting and MouseEnter/Leave were all dead in the
            // Windows CPU fallback (the GPU-gated block below has no CPU arm).
            if window.common.hit_tester.is_none() || window.common.document_id.is_none() {
                PlatformWindow::update_hit_test_at(&mut *window, logical_pos);
            }

            // Update hit test (GPU mode only — CPU mode handled above)
            if let Some(ref mut layout_window) = window.common.layout_window {
                if let (Some(hit_tester), Some(doc_id)) = (
                    window.common.hit_tester.as_mut(),
                    window.common.document_id,
                ) {
                    use crate::desktop::wr_translate2::fullhittest_new_webrender;

                    let hit_tester = hit_tester.resolve();
                    let hit_test = fullhittest_new_webrender(
                        &*hit_tester,
                        doc_id,
                        layout_window.focus_manager.get_focused_node().copied(),
                        &layout_window.layout_results,
                        &CursorPosition::InWindow(logical_pos),
                        hidpi_factor,
                    );

                    layout_window
                        .hover_manager
                        .push_hit_test(InputPointId::Mouse, hit_test);
                }
            }

            // Capture mouse
            (window.win32.user32.SetCapture)(hwnd);

            // V2 system will detect MouseDown event
            let result = window.process_window_events(0);

            // Request redraw if needed
            window.route_main_window_result(hwnd, result);

            0
        }

        WM_LBUTTONUP => {
            // Left mouse button up - similar to macOS handle_mouse_up
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;

            use azul_core::{geom::LogicalPosition, window::CursorPosition};

            let hidpi_factor = window.common.current_window_state.size.get_hidpi_factor();
            let logical_pos = LogicalPosition::new(
                x as f32 / hidpi_factor.inner.get(),
                y as f32 / hidpi_factor.inner.get(),
            );

            // End scrollbar drag if active (before state changes)
            if window.common.scrollbar_drag_state.is_some() {
                window.common.scrollbar_drag_state = None;
                unsafe {
                    (window.win32.user32.ReleaseCapture)();
                }
                return 0;
            }

            // Save previous state BEFORE making changes
            window.common.previous_window_state = Some(window.common.current_window_state.clone());

            // Update mouse state
            window.common.current_window_state.mouse_state.cursor_position =
                CursorPosition::InWindow(logical_pos);
            window.common.current_window_state.mouse_state.left_down = false;

            // Record input sample for gesture detection (button up ends session)
            // Use GetCursorPos for accurate screen-absolute position (physical pixels → logical)
            let screen_pos = {
                let mut pt = dlopen::POINT { x: 0, y: 0 };
                unsafe { (window.win32.user32.GetCursorPos)(&mut pt); }
                let hf = hidpi_factor.inner.get();
                azul_core::geom::LogicalPosition::new(pt.x as f32 / hf, pt.y as f32 / hf)
            };
            window.record_input_sample(logical_pos, BUTTON_STATE_NONE, false, true, Some(screen_pos));

            // CPU mode (no WR hit_tester/document_id): resolve the hit test via
            // the shared perform_hit_test → cpu_hit_tester path. Without this,
            // events dispatched against a stale/empty hover state — hover CSS,
            // clicks, wheel targeting and MouseEnter/Leave were all dead in the
            // Windows CPU fallback (the GPU-gated block below has no CPU arm).
            if window.common.hit_tester.is_none() || window.common.document_id.is_none() {
                PlatformWindow::update_hit_test_at(&mut *window, logical_pos);
            }

            // Update hit test (GPU mode only — CPU mode handled above)
            if let Some(ref mut layout_window) = window.common.layout_window {
                if let (Some(hit_tester), Some(doc_id)) = (
                    window.common.hit_tester.as_mut(),
                    window.common.document_id,
                ) {
                    use crate::desktop::wr_translate2::fullhittest_new_webrender;

                    let hit_tester = hit_tester.resolve();
                    let hit_test = fullhittest_new_webrender(
                        &*hit_tester,
                        doc_id,
                        layout_window.focus_manager.get_focused_node().copied(),
                        &layout_window.layout_results,
                        &CursorPosition::InWindow(logical_pos),
                        hidpi_factor,
                    );

                    layout_window
                        .hover_manager
                        .push_hit_test(InputPointId::Mouse, hit_test);
                }
            }

            // Release mouse capture
            (window.win32.user32.ReleaseCapture)();

            // V2 system will detect MouseUp event
            let result = window.process_window_events(0);

            // Request redraw if needed
            window.route_main_window_result(hwnd, result);

            0
        }

        WM_RBUTTONDOWN => {
            // Right mouse button down
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;

            use azul_core::{geom::LogicalPosition, window::CursorPosition};

            let hidpi_factor = window.common.current_window_state.size.get_hidpi_factor();
            let logical_pos = LogicalPosition::new(
                x as f32 / hidpi_factor.inner.get(),
                y as f32 / hidpi_factor.inner.get(),
            );

            // Save previous state BEFORE making changes
            window.common.previous_window_state = Some(window.common.current_window_state.clone());

            // Update mouse state
            window.common.current_window_state.mouse_state.cursor_position =
                CursorPosition::InWindow(logical_pos);
            window.common.current_window_state.mouse_state.right_down = true;

            // CPU mode (no WR hit_tester/document_id): resolve the hit test via
            // the shared perform_hit_test → cpu_hit_tester path. Without this,
            // events dispatched against a stale/empty hover state — hover CSS,
            // clicks, wheel targeting and MouseEnter/Leave were all dead in the
            // Windows CPU fallback (the GPU-gated block below has no CPU arm).
            if window.common.hit_tester.is_none() || window.common.document_id.is_none() {
                PlatformWindow::update_hit_test_at(&mut *window, logical_pos);
            }

            // Update hit test (GPU mode only — CPU mode handled above)
            if let Some(ref mut layout_window) = window.common.layout_window {
                if let (Some(hit_tester), Some(doc_id)) = (
                    window.common.hit_tester.as_mut(),
                    window.common.document_id,
                ) {
                    use crate::desktop::wr_translate2::fullhittest_new_webrender;

                    let hit_tester = hit_tester.resolve();
                    let hit_test = fullhittest_new_webrender(
                        &*hit_tester,
                        doc_id,
                        layout_window.focus_manager.get_focused_node().copied(),
                        &layout_window.layout_results,
                        &CursorPosition::InWindow(logical_pos),
                        hidpi_factor,
                    );

                    layout_window
                        .hover_manager
                        .push_hit_test(InputPointId::Mouse, hit_test);
                }
            }

            // V2 system will detect MouseDown event
            let result = window.process_window_events(0);

            // Request redraw if needed
            window.route_main_window_result(hwnd, result);

            0
        }

        WM_RBUTTONUP => {
            // Right mouse button up - with context menu support
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;

            use azul_core::{geom::LogicalPosition, window::CursorPosition};

            let hidpi_factor = window.common.current_window_state.size.get_hidpi_factor();
            let logical_pos = LogicalPosition::new(
                x as f32 / hidpi_factor.inner.get(),
                y as f32 / hidpi_factor.inner.get(),
            );

            // Save previous state BEFORE making changes
            window.common.previous_window_state = Some(window.common.current_window_state.clone());

            // Update mouse state
            window.common.current_window_state.mouse_state.cursor_position =
                CursorPosition::InWindow(logical_pos);
            window.common.current_window_state.mouse_state.right_down = false;

            // CPU mode (no WR hit_tester/document_id): resolve the hit test via
            // the shared perform_hit_test → cpu_hit_tester path. Without this,
            // events dispatched against a stale/empty hover state — hover CSS,
            // clicks, wheel targeting and MouseEnter/Leave were all dead in the
            // Windows CPU fallback (the GPU-gated block below has no CPU arm).
            if window.common.hit_tester.is_none() || window.common.document_id.is_none() {
                PlatformWindow::update_hit_test_at(&mut *window, logical_pos);
            }

            // Update hit test (GPU mode only — CPU mode handled above)
            if let Some(ref mut layout_window) = window.common.layout_window {
                if let (Some(hit_tester), Some(doc_id)) = (
                    window.common.hit_tester.as_mut(),
                    window.common.document_id,
                ) {
                    use crate::desktop::wr_translate2::fullhittest_new_webrender;

                    let hit_tester = hit_tester.resolve();
                    let hit_test = fullhittest_new_webrender(
                        &*hit_tester,
                        doc_id,
                        layout_window.focus_manager.get_focused_node().copied(),
                        &layout_window.layout_results,
                        &CursorPosition::InWindow(logical_pos),
                        hidpi_factor,
                    );

                    layout_window
                        .hover_manager
                        .push_hit_test(InputPointId::Mouse, hit_test);
                }
            }

            // Try to show context menu first
            let showed_context_menu = window.try_show_context_menu(x, y);

            // If context menu was shown, skip normal mouse up processing
            if !showed_context_menu {
                // V2 system will detect MouseUp event
                let result = window.process_window_events(0);

                // Request redraw if needed
                window.route_main_window_result(hwnd, result);
            }

            0
        }
        WM_MBUTTONDOWN => {
            // Middle mouse button down
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;

            use azul_core::{geom::LogicalPosition, window::CursorPosition};

            let hidpi_factor = window.common.current_window_state.size.get_hidpi_factor();
            let logical_pos = LogicalPosition::new(
                x as f32 / hidpi_factor.inner.get(),
                y as f32 / hidpi_factor.inner.get(),
            );

            // Save previous state
            window.common.previous_window_state = Some(window.common.current_window_state.clone());

            // Update mouse state
            window.common.current_window_state.mouse_state.cursor_position =
                CursorPosition::InWindow(logical_pos);
            window.common.current_window_state.mouse_state.middle_down = true;

            // V2 system will detect MouseDown event
            let result = window.process_window_events(0);

            window.route_main_window_result(hwnd, result);

            0
        }

        WM_MBUTTONUP => {
            // Middle mouse button up
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;

            use azul_core::{geom::LogicalPosition, window::CursorPosition};

            let hidpi_factor = window.common.current_window_state.size.get_hidpi_factor();
            let logical_pos = LogicalPosition::new(
                x as f32 / hidpi_factor.inner.get(),
                y as f32 / hidpi_factor.inner.get(),
            );

            // Save previous state
            window.common.previous_window_state = Some(window.common.current_window_state.clone());

            // Update mouse state
            window.common.current_window_state.mouse_state.cursor_position =
                CursorPosition::InWindow(logical_pos);
            window.common.current_window_state.mouse_state.middle_down = false;

            // V2 system will detect MouseUp event
            let result = window.process_window_events(0);

            window.route_main_window_result(hwnd, result);

            0
        }

        WM_MOUSEWHEEL | WM_MOUSEHWHEEL => {
            // Mouse wheel scrolled - similar to macOS handle_scroll_wheel.
            // WM_MOUSEHWHEEL (tilt wheel / trackpad horizontal) previously fell
            // through to DefWindowProc — horizontal scroll containers were
            // unusable via wheel.
            let delta = ((wparam >> 16) & 0xFFFF) as i16 as i32;
            // Raw amount; direction sign is applied centrally in ScrollManager
            // (natural-scroll flag), not hardcoded here.
            let scroll_amount = delta as f32 / WHEEL_DELTA as f32;
            let horizontal = msg == WM_MOUSEHWHEEL;

            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;

            use azul_core::{geom::LogicalPosition, window::CursorPosition};

            let hidpi_factor = window.common.current_window_state.size.get_hidpi_factor();
            let logical_pos = LogicalPosition::new(
                x as f32 / hidpi_factor.inner.get(),
                y as f32 / hidpi_factor.inner.get(),
            );

            // Save previous state
            window.common.previous_window_state = Some(window.common.current_window_state.clone());

            // Queue scroll input for the physics timer instead of directly setting offsets.
            // The timer will consume these via ScrollInputQueue and push CallbackChange::ScrollTo.
            if delta.abs() > 0 {
                let mut should_start_timer = false;
                let mut input_queue_clone = None;

                if let Some(ref mut layout_window) = window.common.layout_window {
                    use azul_core::task::Instant;
                    use azul_layout::managers::scroll_state::ScrollInputSource;

                    let now = Instant::from(std::time::Instant::now());

                    if let Some((_dom_id, _node_id, start_timer)) =
                        layout_window.scroll_manager.record_scroll_from_hit_test(
                            if horizontal { scroll_amount * 20.0 } else { 0.0 },
                            if horizontal { 0.0 } else { scroll_amount * 20.0 },
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
                        use azul_core::task::{SCROLL_MOMENTUM_TIMER_ID};
                        use azul_layout::scroll_timer::{ScrollPhysicsState, scroll_physics_timer_callback};
                        use azul_layout::timer::{Timer, TimerCallbackType};
                        use azul_core::refany::RefAny;
                        use azul_core::task::Duration;

                        let physics_state = ScrollPhysicsState::new(queue, window.common.system_style.scroll_physics.clone());
                        let interval_ms = window.common.system_style.scroll_physics.timer_interval_ms;
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

                        window.start_timer(SCROLL_MOMENTUM_TIMER_ID.id, timer);
                    }
                }
            }

            // CPU mode (no WR hit_tester/document_id): resolve the hit test via
            // the shared perform_hit_test → cpu_hit_tester path. Without this,
            // events dispatched against a stale/empty hover state — hover CSS,
            // clicks, wheel targeting and MouseEnter/Leave were all dead in the
            // Windows CPU fallback (the GPU-gated block below has no CPU arm).
            if window.common.hit_tester.is_none() || window.common.document_id.is_none() {
                PlatformWindow::update_hit_test_at(&mut *window, logical_pos);
            }

            // Update hit test (GPU mode only — CPU mode handled above)
            if let Some(ref mut layout_window) = window.common.layout_window {
                if let (Some(hit_tester), Some(doc_id)) = (
                    window.common.hit_tester.as_mut(),
                    window.common.document_id,
                ) {
                    use crate::desktop::wr_translate2::fullhittest_new_webrender;

                    let hit_tester = hit_tester.resolve();
                    let hit_test = fullhittest_new_webrender(
                        &*hit_tester,
                        doc_id,
                        layout_window.focus_manager.get_focused_node().copied(),
                        &layout_window.layout_results,
                        &CursorPosition::InWindow(logical_pos),
                        hidpi_factor,
                    );

                    layout_window
                        .hover_manager
                        .push_hit_test(InputPointId::Mouse, hit_test);
                }
            }

            // V2 system will detect Scroll event from ScrollManager state
            let result = window.process_window_events(0);

            window.route_main_window_result(hwnd, result);

            0
        }

        WM_KEYDOWN | WM_SYSKEYDOWN => {
            // Key pressed - similar to macOS handle_key_down
            let vk_code = wparam as u32;
            let scan_code = ((lparam >> 16) & 0xFF) as u32;
            let repeat_count = (lparam & 0xFFFF) as u16;
            let is_repeat = repeat_count > 1 || ((lparam >> 30) & 1) == 1; // bit 30 = previous key state

            // Translate virtual key to azul key
            if let Some(virtual_key) = win_event::vkey_to_winit_vkey(vk_code as i32) {
                // Save previous state. For key repeats, clear current_virtual_keycode
                // in the snapshot so the state-diff sees None → Some(key).
                let mut prev_snapshot = window.common.current_window_state.clone();
                if is_repeat {
                    prev_snapshot.keyboard_state.current_virtual_keycode =
                        azul_core::window::OptionVirtualKeyCode::None;
                }
                window.common.previous_window_state = Some(prev_snapshot);

                // Update keyboard state
                window
                    .common
                    .current_window_state
                    .keyboard_state
                    .current_virtual_keycode =
                    azul_core::window::OptionVirtualKeyCode::Some(virtual_key);
                window
                    .common
                    .current_window_state
                    .keyboard_state
                    .pressed_virtual_keycodes
                    .insert_hm_item(virtual_key);
                window
                    .common
                    .current_window_state
                    .keyboard_state
                    .pressed_scancodes
                    .insert_hm_item(scan_code);

                // V2 system will detect VirtualKeyDown event
                let result = window.process_window_events(0);

                window.route_main_window_result(hwnd, result);
            }

            0
        }

        WM_KEYUP | WM_SYSKEYUP => {
            // Key released - similar to macOS handle_key_up
            let vk_code = wparam as u32;
            let scan_code = ((lparam >> 16) & 0xFF) as u32;

            // Translate virtual key
            if let Some(virtual_key) = win_event::vkey_to_winit_vkey(vk_code as i32) {
                // Save previous state
                window.common.previous_window_state = Some(window.common.current_window_state.clone());

                // Update keyboard state
                window
                    .common
                    .current_window_state
                    .keyboard_state
                    .current_virtual_keycode = azul_core::window::OptionVirtualKeyCode::None;
                window
                    .common
                    .current_window_state
                    .keyboard_state
                    .pressed_virtual_keycodes
                    .remove_hm_item(&virtual_key);
                window
                    .common
                    .current_window_state
                    .keyboard_state
                    .pressed_scancodes
                    .remove_hm_item(&scan_code);

                // V2 system will detect VirtualKeyUp event
                let result = window.process_window_events(0);

                window.route_main_window_result(hwnd, result);
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
                window.common.previous_window_state = Some(window.common.current_window_state.clone());

                // Record text input in the TextInputManager
                if let Some(ref mut layout_window) = window.common.layout_window {
                    let text_str = chr.to_string();
                    let _ = layout_window.record_text_input(&text_str);
                }

                // V2 system will detect TextInput event
                let result = window.process_window_events(0);

                window.route_main_window_result(hwnd, result);
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
                // Clear preedit in cursor manager
                if let Some(ref mut lw) = window.common.layout_window {
                    lw.text_edit_manager.clear_preedit();
                }
                // Redraw to clear preedit underline
                (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);

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
                                    let comp_str = String::from_utf16(&buffer).ok();
                                    window.ime_composition = comp_str.clone();
                                    // Store preedit in cursor manager for inline rendering
                                    if let Some(ref mut lw) = window.common.layout_window {
                                        if let Some(ref text) = comp_str {
                                            lw.text_edit_manager.set_preedit(
                                                text.clone(), 0, text.len() as i32,
                                            );
                                        }
                                    }
                                    log_trace!(
                                        LogCategory::Input,
                                        "IME Composition: {:?}",
                                        window.ime_composition
                                    );
                                }
                            }

                            // Release context
                            (imm32.ImmReleaseContext)(hwnd, himc);
                        }
                    }
                }

                // Trigger redraw so preedit indicator is rendered
                (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
                // Let Windows show composition window by default
                (window.win32.user32.DefWindowProcW)(hwnd, msg, wparam, lparam)
            } else {
                (window.win32.user32.DefWindowProcW)(hwnd, msg, wparam, lparam)
            }
        }

        WM_IME_ENDCOMPOSITION => {
            // IME composition ended - clear composition preview
            window.ime_composition = None;
            // Clear preedit in cursor manager
            if let Some(ref mut lw) = window.common.layout_window {
                lw.text_edit_manager.clear_preedit();
            }
            // Redraw to clear preedit underline
            (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
            (window.win32.user32.DefWindowProcW)(hwnd, msg, wparam, lparam)
        }

        WM_IME_CHAR => {
            // Double-byte character from IME (e.g., Japanese, Chinese, Korean)
            // The new V2 input system handles text input through a different mechanism
            // This character will be processed by the event system automatically
            let char_code = wparam as u32;

            if let Some(chr) = char::from_u32(char_code) {
                if !chr.is_control() {
                    window.common.previous_window_state = Some(window.common.current_window_state.clone());

                    // Record text input in the TextInputManager
                    if let Some(ref mut layout_window) = window.common.layout_window {
                        let text_str = chr.to_string();
                        let _ = layout_window.record_text_input(&text_str);
                    }

                    // V2 system will detect TextInput event
                    let result = window.process_window_events(0);

                    window.route_main_window_result(hwnd, result);
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
            window.common.previous_window_state = Some(window.common.current_window_state.clone());
            window.common.current_window_state.flags.has_focus = true;
            window.common.current_window_state.window_focused = true;
            window.dynamic_selector_context.window_focused = true;

            // Phase 2: OnFocus callback - sync IME position after focus
            window.sync_ime_position_to_os();

            // Run the state-diff pass NOW: focus/blur callbacks fire off the
            // window_focused transition, and focus-conditional styling needs a
            // repaint. Returning without processing let the next input event
            // overwrite previous_window_state, erasing the transition —
            // focus/blur callbacks never fired on Windows.
            let r = window.process_window_events(0);
            window.route_main_window_result(hwnd, r);

            0
        }

        WM_KILLFOCUS => {
            // Window lost focus
            window.common.previous_window_state = Some(window.common.current_window_state.clone());
            window.common.current_window_state.flags.has_focus = false;
            window.common.current_window_state.window_focused = false;
            window.dynamic_selector_context.window_focused = false;

            // Same as WM_SETFOCUS: process + route so blur callbacks fire and
            // unfocused styling repaints.
            let r = window.process_window_events(0);
            window.route_main_window_result(hwnd, r);

            0
        }

        WM_TIMER => {
            // Timer fired — process_timers_and_threads() handles both user timers
            // (invoke_expired_timers) and thread polling (invoke_thread_callbacks).
            use crate::desktop::shell2::common::event::PlatformWindow;
            if window.process_timers_and_threads() {
                (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
            }

            0
        }

        WM_COMMAND => {
            // Menu command
            let command_id = (wparam & 0xFFFF) as u16;

            log_trace!(
                LogCategory::EventLoop,
                "WM_COMMAND received, command_id: {}",
                command_id
            );

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
                if let Some(layout_window) = window.common.layout_window.as_mut() {
                    use azul_core::window::RawWindowHandle;

                    let raw_handle = RawWindowHandle::Windows(azul_core::window::WindowsHandle {
                        hwnd: hwnd as *mut _,
                        hinstance: ptr::null_mut(), // Not needed for menu callbacks
                    });

                    let (changes, update) = layout_window.invoke_single_callback(
                        &mut menu_callback.callback,
                        &mut menu_callback.refany,
                        &raw_handle,
                        &window.common.gl_context_ptr,
                        window.common.system_style.clone(),
                        &azul_layout::callbacks::ExternalSystemCallbacks::rust_internal(),
                        &window.common.previous_window_state,
                        &window.common.current_window_state,
                        &window.common.renderer_resources,
                    );

                    use crate::desktop::shell2::common::event::PlatformWindow;
                    let mut event_result = azul_core::events::ProcessEventResult::DoNothing;
                    for change in &changes {
                        event_result = event_result.max(window.apply_user_change(change));
                    }
                    use azul_core::callbacks::Update;
                    match update {
                        Update::RefreshDom | Update::RefreshDomAllWindows => {
                            event_result = event_result.max(azul_core::events::ProcessEventResult::ShouldRegenerateDomCurrentWindow);
                        }
                        Update::DoNothing => {}
                    }

                    // Sync window state changes to Win32 (title, position, size, etc.)
                    window.sync_window_state();

                    // Handle the event result
                    use azul_core::events::ProcessEventResult;
                    match event_result {
                        ProcessEventResult::ShouldIncrementalRelayout => {
                            // Restyle / runtime edit (hover/focus CSS, set_css_property,
                            // set_node_text): re-run layout on the EXISTING StyledDom
                            // instead of a full regenerate_layout() (which would
                            // re-invoke the user's layout_callback + rebuild the
                            // StyledDom). Mirrors the macOS backend's
                            // ShouldIncrementalRelayout arm. frame_relayout_only then
                            // makes WM_PAINT skip regenerate_layout and only rebuild +
                            // send the WebRender transaction.
                            if let Some(layout_window) = window.common.layout_window.as_mut() {
                                let mut debug_messages = None;
                                if let Err(e) =
                                    crate::desktop::shell2::common::layout::incremental_relayout(
                                        layout_window,
                                        &window.common.current_window_state,
                                        &mut window.common.renderer_resources,
                                        &mut debug_messages,
                                    )
                                {
                                    log_warn!(
                                        LogCategory::Layout,
                                        "Incremental relayout failed: {}",
                                        e
                                    );
                                }
                            }
                            window.common.frame_relayout_only = true;
                            window.common.frame_needs_regeneration = true;
                            (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
                        }
                        ProcessEventResult::ShouldRegenerateDomCurrentWindow
                        | ProcessEventResult::ShouldRegenerateDomAllWindows
                        | ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                            window.common.frame_needs_regeneration = true;
                            (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
                        }
                        // ShouldUpdateDisplayListCurrentWindow: pending VirtualView updates are
                        // queued in layout_window.pending_virtual_view_updates and will be processed
                        // in the render path — no full layout regeneration needed.
                        ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
                        | ProcessEventResult::ShouldReRenderCurrentWindow => {
                            (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
                        }
                        ProcessEventResult::DoNothing => {
                            // No action needed
                        }
                    }
                } else {
                    log_warn!(
                        LogCategory::Callbacks,
                        "No layout window available for menu callback"
                    );
                }
            } else {
                log_debug!(
                    LogCategory::Callbacks,
                    "No callback found for command_id: {}",
                    command_id
                );
            }

            (window.win32.user32.DefWindowProcW)(hwnd, msg, wparam, lparam)
        }

        WM_DPICHANGED => {
            // DPI changed
            let new_dpi = ((wparam >> 16) & 0xFFFF) as u32;

            // Save previous state
            window.common.previous_window_state = Some(window.common.current_window_state.clone());

            // Update DPI in window state
            window.common.current_window_state.size.dpi = new_dpi;

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
                window.common.current_window_state.size.dimensions = logical_size;
            }

            // DPI change requires full relayout
            window.common.frame_needs_regeneration = true;

            // Request redraw
            unsafe {
                (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
            }

            0
        }

        // NOTE: the legacy `WM_DROPFILES` (drop-only) arm has been removed —
        // file drag-and-drop now goes through the OLE `IDropTarget` COM object
        // (`windows::dnd`), which delivers hover (`DragEnter`/`DragOver`),
        // leave (`DragLeave`) AND drop (`Drop`). OLE supersedes `WM_DROPFILES`;
        // keeping both would double-fire the drop.

        WM_DISPLAYCHANGE => {
            // Monitor topology changed (monitor added/removed/resolution changed)
            // Refresh the cached monitor list
            if let Some(ref lw) = window.common.layout_window {
                if let Ok(mut guard) = lw.monitors.lock() {
                    *guard = crate::desktop::display::get_monitors();
                }
            }
            0
        }

        WM_APP_FRAME_READY_LOCAL => {
            // WebRender finished an async frame build — consume the signal
            // and schedule the present (needs_gpu_present defeats the GPU
            // skip-heuristic; WM_PAINT renders + swaps the built frame).
            let ready = {
                let (lock, _) = &*window.new_frame_ready;
                let mut g = lock.lock().unwrap();
                std::mem::take(&mut *g)
            };
            if ready {
                window.needs_gpu_present = true;
                unsafe {
                    (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
                }
            }
            0
        }

        WM_GETMINMAXINFO => {
            // Enforce min/max size constraints from the window state. Without
            // this handler users could drag-resize below min_dimensions /
            // above max_dimensions (every other platform enforces them).
            #[repr(C)]
            struct MinMaxInfo {
                pt_reserved: dlopen::POINT,
                pt_max_size: dlopen::POINT,
                pt_max_position: dlopen::POINT,
                pt_min_track_size: dlopen::POINT,
                pt_max_track_size: dlopen::POINT,
            }
            let mmi = lparam as *mut MinMaxInfo;
            if !mmi.is_null() {
                let hidpi = window.common.current_window_state.size.get_hidpi_factor();
                let hf = hidpi.inner.get();
                // Frame overhead: constraints are on the CLIENT area; the
                // track size is the OUTER window. Derive the current frame
                // delta from the actual window vs client rects.
                let (frame_w, frame_h) = unsafe {
                    let mut wr: dlopen::RECT = std::mem::zeroed();
                    let mut cr: dlopen::RECT = std::mem::zeroed();
                    (window.win32.user32.GetWindowRect)(hwnd, &mut wr);
                    (window.win32.user32.GetClientRect)(hwnd, &mut cr);
                    (
                        (wr.right - wr.left) - (cr.right - cr.left),
                        (wr.bottom - wr.top) - (cr.bottom - cr.top),
                    )
                };
                if let Some(min) = window
                    .common
                    .current_window_state
                    .size
                    .min_dimensions
                    .into_option()
                {
                    unsafe {
                        (*mmi).pt_min_track_size.x =
                            (min.width * hf).round() as i32 + frame_w;
                        (*mmi).pt_min_track_size.y =
                            (min.height * hf).round() as i32 + frame_h;
                    }
                }
                if let Some(max) = window
                    .common
                    .current_window_state
                    .size
                    .max_dimensions
                    .into_option()
                {
                    unsafe {
                        (*mmi).pt_max_track_size.x =
                            (max.width * hf).round() as i32 + frame_w;
                        (*mmi).pt_max_track_size.y =
                            (max.height * hf).round() as i32 + frame_h;
                    }
                }
            }
            0
        }

        WM_SETTINGCHANGE | WM_THEMECHANGED => {
            // System theme / colors / metrics changed at runtime (dark-mode
            // toggle). The style was captured once at creation, so apps kept
            // the startup theme until restart. Re-discover the system style,
            // update the window theme through the diff pipeline (ThemeChange
            // events fire) and rebuild.
            window.common.previous_window_state =
                Some(window.common.current_window_state.clone());
            let new_style =
                std::sync::Arc::new(crate::desktop::app::discover_system_style());
            window.common.current_window_state.theme = match new_style.theme {
                azul_css::system::Theme::Dark => azul_core::window::WindowTheme::DarkMode,
                azul_css::system::Theme::Light => azul_core::window::WindowTheme::LightMode,
            };
            window.common.system_style = new_style;
            let r = window.process_window_events(0);
            window.route_main_window_result(hwnd, r);
            window.common.frame_needs_regeneration = true;
            unsafe {
                (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
            }
            0
        }

        WM_SETCURSOR => {
            // The window class registers hCursor = NULL, so DefWindowProc
            // never resets the cursor — entering the client area from a
            // resize border kept the sizing arrows. In the client area,
            // re-assert the current CSS cursor (or the default arrow);
            // elsewhere let DefWindowProc handle the frame cursors.
            let hit = (lparam & 0xFFFF) as isize;
            if hit == HTCLIENT {
                let cursor_type = match window
                    .common
                    .current_window_state
                    .mouse_state
                    .mouse_cursor_type
                {
                    azul_core::window::OptionMouseCursorType::Some(t) => t,
                    azul_core::window::OptionMouseCursorType::None => {
                        azul_core::window::MouseCursorType::Default
                    }
                };
                window.set_cursor(cursor_type);
                1
            } else {
                (window.win32.user32.DefWindowProcW)(hwnd, msg, wparam, lparam)
            }
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

// Lifecycle methods (formerly on PlatformWindow V1 trait)

impl Win32Window {
    pub fn poll_event(&mut self) -> Option<Win32Event> {
        // The existing poll_event_internal returns bool
        // We need to convert this to return Option<Win32Event>
        // For now, return None - will be implemented in phase 1.2
        if self.poll_event_internal() {
            Some(Win32Event::Other)
        } else {
            None
        }
    }

    pub fn present(&mut self) -> Result<(), WindowError> {
        // present() is called from external code — always send lightweight transaction
        // to ensure any pending scroll/GPU changes are flushed
        self.render_and_present(false)
            .map_err(|e| WindowError::PlatformError(format!("Present failed: {}", e)))
    }

    /// Process pending accessibility actions from assistive technology (e.g. Narrator)
    #[cfg(feature = "a11y")]
    pub fn process_accessibility_actions(&mut self) {
        let mut actions = Vec::new();
        while let Some(action) = self.accessibility_adapter.poll_action() {
            actions.push(action);
        }
        if actions.is_empty() {
            return;
        }

        let now = std::time::Instant::now();
        for (dom_id, node_id, action) in actions {
            if let Some(lw) = self.common.layout_window.as_mut() {
                let affected = lw.process_accessibility_action(dom_id, node_id, action, now);
                if !affected.is_empty() {
                    self.common.display_list_dirty = true;
                    // Invoke the callbacks the action mapped to (synthetic
                    // MouseUp for the Default/click action, etc.) — previously
                    // this map was dropped and screen-reader activation did
                    // nothing.
                    use crate::desktop::shell2::common::event::PlatformWindow as _;
                    let update = self.dispatch_accessibility_events(&affected);
                    if !matches!(update, azul_core::callbacks::Update::DoNothing) {
                        // The callback asked for a refresh (e.g. RefreshDom
                        // from a zoom button) — regenerate on the next frame,
                        // exactly like pointer-event dispatch does.
                        self.common.frame_needs_regeneration = true;
                    }
                }
            }
        }

        self.common.a11y_dirty = true;
        self.request_redraw();
    }

    pub fn is_open(&self) -> bool {
        self.is_open
    }

    pub fn close(&mut self) {
        // Close the window by posting WM_CLOSE
        unsafe {
            const WM_CLOSE: u32 = 0x0010;
            (self.win32.user32.PostMessageW)(self.hwnd, WM_CLOSE, 0, 0);
        }
        if let Some(doc_id) = self.common.document_id {
            crate::desktop::gl_texture_integration::remove_document_textures(&doc_id);
        }
        self.is_open = false;
    }

    pub fn request_redraw(&mut self) {
        // Use per-rect damage when available (reduces compositor work)
        if !self.gpu_damage_rects.is_empty() {
            let dpi = self.common.current_window_state.size.dpi as f32 / 96.0;
            let rects: Vec<_> = self.gpu_damage_rects.drain(..).collect();
            for dr in &rects {
                let rect = dlopen::RECT {
                    left: (dr.origin.x * dpi) as i32,
                    top: (dr.origin.y * dpi) as i32,
                    right: ((dr.origin.x + dr.size.width) * dpi) as i32 + 1,
                    bottom: ((dr.origin.y + dr.size.height) * dpi) as i32 + 1,
                };
                unsafe {
                    (self.win32.user32.InvalidateRect)(self.hwnd, &rect as *const _ as *const _, 0);
                }
            }
            return;
        }
        // Full-surface redraw fallback
        unsafe {
            (self.win32.user32.InvalidateRect)(self.hwnd, ptr::null(), 0);
        }
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
            if let Some(layout_window) = self.common.layout_window.as_ref() {
                // Get menu from layout_window's root DOM (dom_id 0, node_id 0)
                layout_window
                    .layout_results
                    .get(&DomId::ROOT_ID)
                    .and_then(|lr| {
                        let node_container = lr.styled_dom.node_data.as_container();
                        node_container
                            .get(NodeId::ZERO)
                            .and_then(|n| n.get_menu_bar())
                            .map(|boxed_menu| boxed_menu.clone())
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

    /// Returns the DPI of the window.
    pub fn get_window_dpi(&self) -> u32 {
        unsafe { self.dpi.hwnd_dpi(self.hwnd as _) }
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

// PlatformWindow Trait Implementation

impl PlatformWindow for Win32Window {

    impl_platform_window_getters!(common);

    fn get_raw_window_handle(&self) -> RawWindowHandle {
        RawWindowHandle::Windows(WindowsHandle {
            hwnd: self.hwnd as *mut std::ffi::c_void,
            hinstance: self.hinstance as *mut std::ffi::c_void,
        })
    }

    fn prepare_callback_invocation(&mut self) -> event::InvokeSingleCallbackBorrows {
        let layout_window = self
            .common
            .layout_window
            .as_mut()
            .expect("Layout window must exist for callback invocation");

        event::InvokeSingleCallbackBorrows {
            layout_window,
            window_handle: RawWindowHandle::Windows(WindowsHandle {
                hwnd: self.hwnd as *mut std::ffi::c_void,
                hinstance: self.hinstance as *mut std::ffi::c_void,
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

    fn flush_a11y_tree_update(&mut self) {
        // MWA-A3e: push incremental a11y updates (text edits / caret moves)
        // parked in last_tree_update by the event pass; previously they only
        // reached UIA on the next full relayout.
        #[cfg(feature = "a11y")]
        {
            let pending = self
                .common
                .layout_window
                .as_mut()
                .and_then(|lw| lw.a11y_manager.last_tree_update.take());
            if let Some(update) = pending {
                self.accessibility_adapter.update_tree(update);
            }
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
        if let Some(layout_window) = self.common.layout_window.as_mut() {
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
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            layout_window
                .timers
                .remove(&azul_core::task::TimerId { id: timer_id });
        }
    }

    // Thread Management (Win32 Implementation)

    fn start_thread_poll_timer(&mut self) {
        if self.thread_timer_running.is_none() {
            let timer_id = unsafe {
                (self.win32.user32.SetTimer)(
                    self.hwnd,
                    Self::THREAD_POLL_TIMER_ID,
                    Self::THREAD_POLL_INTERVAL_MS,
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
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            for (thread_id, thread) in threads {
                layout_window.threads.insert(thread_id, thread);
            }
        }
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

    fn sync_window_state(&mut self) {
        Win32Window::sync_window_state(self);
    }
}

impl Win32Window {
    /// Show a native Win32 popup menu at the given logical position using `TrackPopupMenu`.
    fn show_native_menu_at_position(
        &mut self,
        menu: &azul_core::menu::Menu,
        position: azul_core::geom::LogicalPosition,
    ) {
        let mut hmenu = unsafe { (self.win32.user32.CreatePopupMenu)() };
        if hmenu.is_null() {
            self.show_fallback_menu(menu, position);
            return;
        }

        let mut callbacks = BTreeMap::new();
        menu::WindowsMenuBar::recursive_construct_menu(
            &mut hmenu,
            menu.items.as_ref(),
            &mut callbacks,
            &self.win32,
        );

        let dpi_factor = unsafe { self.dpi.hwnd_dpi(self.hwnd as _) } as f32 / 96.0;
        let mut pt = dlopen::POINT {
            x: (position.x * dpi_factor) as i32,
            y: (position.y * dpi_factor) as i32,
        };
        unsafe {
            (self.win32.user32.ClientToScreen)(self.hwnd, &mut pt);
        }

        self.context_menu = Some(callbacks);

        unsafe {
            (self.win32.user32.SetForegroundWindow)(self.hwnd);
            (self.win32.user32.TrackPopupMenu)(
                hmenu,
                dlopen::constants::TPM_RIGHTBUTTON | dlopen::constants::TPM_LEFTALIGN,
                pt.x,
                pt.y,
                0,
                self.hwnd,
                ptr::null(),
            );
            (self.win32.user32.DestroyMenu)(hmenu);
        }
    }

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
            "Queuing fallback menu window at ({}, {}) - will be created in event loop",
            position.x,
            position.y
        );

        self.pending_window_creates.push(menu_options);
    }
}

/// Position window on requested monitor, or center on primary monitor
/// Resolve a parent window's stored top-left from the registry, for
/// `WindowPosition::RelativeToParentWindow`. Returns `None` if there is no
/// parent or it has no concrete position yet (caller treats the offset as
/// monitor-relative).
fn resolve_windows_parent_origin(parent_window_id: u64) -> Option<(i32, i32)> {
    if parent_window_id == 0 {
        return None;
    }
    unsafe {
        let wptr = registry::get_window(parent_window_id as usize as HWND)?;
        match (*wptr).common.current_window_state.position {
            azul_core::window::WindowPosition::Initialized(pos) => Some((pos.x, pos.y)),
            _ => None,
        }
    }
}

fn position_window_on_monitor(
    hwnd: HWND,
    monitor_id: azul_core::window::MonitorId,
    position: azul_core::window::WindowPosition,
    size: azul_core::window::WindowSize,
    parent_window_id: u64,
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
        WindowPosition::RelativeToParentWindow(offset) => {
            // Child window (menu/dropdown/popup): place at parent_top_left +
            // offset. Resolve the parent's absolute origin from the registry;
            // fall back to monitor-relative if the parent is unknown.
            match resolve_windows_parent_origin(parent_window_id) {
                Some((px, py)) => (px + offset.x, py + offset.y),
                None => (
                    (target_monitor.position.x + offset.x as isize) as i32,
                    (target_monitor.position.y + offset.y as isize) as i32,
                ),
            }
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

        if let ImePosition::Initialized(rect) = self.common.current_window_state.ime_position {
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
