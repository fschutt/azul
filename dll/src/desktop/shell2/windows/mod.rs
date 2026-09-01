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

/// `WM_SIZE` events seen. The Win32 half of the cross-backend resize census —
/// see the Wayland `CONFIGURES_SEEN` for why the count matters.
pub(super) static WM_SIZE_SEEN: core::sync::atomic::AtomicUsize =
    core::sync::atomic::AtomicUsize::new(0);
use crate::impl_platform_window_getters;
use crate::{log_debug, log_error, log_info, log_trace, log_warn};

pub mod accessibility;
pub mod clipboard;
pub mod dlopen;
pub mod dnd;
mod dpi;
mod gl;
pub mod menu;
pub mod registry;
pub(crate) mod system_style;
mod tooltip;
mod wcreate;
pub mod win_event;

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
/// Present a native popup menu that was PARKED by a handler.
///
/// `TrackPopupMenu` runs a modal message loop, so calling it from inside
/// `window_proc` re-enters `window_proc` (WM_TIMER is delivered in that loop)
/// and hands out a SECOND `&mut Win32Window` while the outer one is still
/// live — aliased `&mut`, i.e. UB. The handler parks the menu and posts this
/// instead; the arm takes it and tracks with no borrow held. Same shape macOS
/// uses for its nested tracking runloop.
pub(crate) const WM_APP_SHOW_PENDING_MENU: u32 = 0x8000 + 0x0043; // WM_APP + 0x43

/// A popup menu built and positioned, waiting for a turn of the message loop
/// on which no `&mut Win32Window` is live. Holds only OS handles and screen
/// coordinates — no Rust references into the window.
pub(crate) struct PendingNativeMenu {
    hmenu: HMENU,
    /// Screen coordinates, already converted.
    x: i32,
    y: i32,
}

/// #27 native backbuffer (Windows): one persistent RGBA-mask DIB section per
/// window — the CPU renderer's direct target. Created (and probe-verified)
/// in the present path; recreated on size change.
#[cfg(feature = "cpurender")]
struct NativeDib {
    mem_dc: dlopen::HDC,
    bitmap: *mut core::ffi::c_void,
    old_bitmap: *mut core::ffi::c_void,
    ptr: *mut u8,
    w: i32,
    h: i32,
}

/// Set to `false` the first time the RGBA-mask probe fails — GDI stacks that
/// ignore BI_BITFIELDS byte order get the legacy path for the whole process
/// instead of a per-window retry loop (#27).
#[cfg(feature = "cpurender")]
static NATIVE_DIB_SUPPORTED: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(true);

pub struct Win32Window {
    /// Win32 window handle
    pub hwnd: HWND,
    /// This window was created as an OWNED popup of another azul window
    /// (a transient window or fallback menu): never made topmost.
    owned_popup: bool,
    /// The owner's registry id (its HWND), for re-placing a
    /// `RelativeToParentWindow` popup against the owner's live position.
    owner_id: u64,
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
    /// #27 native backbuffer: persistent RGBA-mask DIB section the renderer
    /// draws into directly (probe-verified per process; see the present
    /// path). Recreated on size change; the GDI objects of the final DIB are
    /// reclaimed by the OS at process exit (windows close rarely enough that
    /// an explicit teardown hook wasn't added — noted for the live review).
    #[cfg(feature = "cpurender")]
    native_dib: Option<NativeDib>,
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
    /// A native popup menu parked by a handler, presented from
    /// [`WM_APP_SHOW_PENDING_MENU`] once no `&mut` to this window is live.
    pending_native_menu: Option<PendingNativeMenu>,

    // Timers and threads
    /// Active timers (TimerId -> Win32 timer handle)
    pub timers: HashMap<usize, usize>,
    /// Thread timer (for polling thread messages)
    pub thread_timer_running: Option<usize>,

    // Input state
    /// High surrogate for UTF-16 character composition
    /// Contact separation at the start of the current WM_GESTURE zoom.
    /// GID_ZOOM reports an absolute distance in pixels, not a ratio, so the
    /// first message is a baseline rather than a scale.
    pub gesture_zoom_baseline: f32,
    pub high_surrogate: Option<u16>,
    /// IME composition string (for preview during typing)
    pub ime_composition: Option<String>,
    /// MWA-C-text_input: whether the window's IME context is currently
    /// associated (starts true — Windows associates a default context).
    /// Diffed against editable focus in sync_ime_enabled_state.
    pub ime_enabled: bool,
    /// The HIMC returned by ImmAssociateContext(hwnd, NULL) when we disable
    /// the IME, restored on re-enable (null while enabled).
    pub ime_saved_himc: dlopen::HIMC,
    /// The focus / editing-node / caret identity the OS-side IME was last
    /// synced to. `sync_ime_state` recomputes the composition + candidate
    /// windows only when this changes, so it can be called from every event
    /// pass and every frame without walking the layout tree 60 times a second.
    ime_sync_key: event::ImeSyncKey,

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
    /// The `AppConfig` this window was built from, kept so a window can be
    /// created from a context that has no access to `run.rs`'s locals — the
    /// modal size/move pump, which has to service `pending_window_creates`
    /// while USER32 owns the loop.
    pub app_config: azul_core::resources::AppConfig,

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
        // THE app-level font manager, so every window shares one set of font
        // pools; see `layout_window_sharing_fonts`.
        app_font_manager: Option<
            Arc<azul_layout::font_traits::FontManager<azul_css::props::basic::FontRef>>,
        >,
    ) -> Result<Self, WindowError> {
        // If background_color is None and no material effect, use system window background
        // Note: When a material is set, the renderer will use transparent clear color automatically
        if options.window_state.background_color.is_none() {
            use azul_core::window::WindowBackgroundMaterial;
            if matches!(
                options.window_state.flags.background_material,
                WindowBackgroundMaterial::Opaque
            ) {
                options.window_state.background_color =
                    config.system_style.colors.window_background;
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

        // A parent-owned popup (Menu type + parent id) gets its parent's HWND
        // as OWNER: it orders above the owner, hides/minimises with it, and
        // (with WS_EX_TOOLWINDOW, see create_hwnd) has no taskbar button —
        // instead of HWND_TOPMOST floating over every other application.
        let owner_hwnd: Option<HWND> = if options.window_state.flags.window_type
            == azul_core::window::WindowType::Menu
            && options.parent_window_id != 0
        {
            registry::get_window(options.parent_window_id as usize as HWND)
                .map(|_| options.parent_window_id as usize as HWND)
        } else {
            None
        };

        // Create HWND (invisible initially to avoid black flash)
        let hwnd = wcreate::create_hwnd(
            hinstance,
            &options,
            owner_hwnd,
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

        // The HWND was created with the requested LOGICAL client size passed
        // raw to CreateWindowExW — which interprets it as the OUTER frame
        // size in PHYSICAL px, so the client area came out smaller by the
        // frame border (and by the whole DPI factor on scaled monitors). Now
        // that the real per-monitor DPI is known, resize so the CLIENT area
        // matches the requested logical size; get_client_rect below reads
        // back the corrected size for everything downstream. size_to_content
        // keeps its 1×1 placeholder and is sized after the first layout.
        if !options.size_to_content {
            let want_w = libm::roundf(options.window_state.size.dimensions.width * dpi_factor)
                .max(1.0) as i32;
            let want_h = libm::roundf(options.window_state.size.dimensions.height * dpi_factor)
                .max(1.0) as i32;
            if let Err(e) = wcreate::set_client_size(hwnd, want_w, want_h, &win32) {
                log_warn!(
                    LogCategory::Window,
                    "[Win32] initial client-size correction failed: {:?}",
                    e
                );
            }
        }

        // Initialize OpenGL context + WebRender (if hardware rendering requested)
        let mut gl_functions = GlFunctions::initialize();

        // Determine renderer type via the unified backend resolution
        // (AZ_BACKEND env var > programmatic hw_accel > Auto). Auto/Gpu try
        // hardware first; the shader probe below falls back to CPU when the
        // driver is unusable. Previously this read only options.renderer and
        // ignored AZ_BACKEND entirely (the env var worked on every OTHER
        // backend).
        let should_use_hardware = {
            use crate::desktop::shell2::common::compositor::AzBackend;
            let hw_accel = options
                .renderer
                .as_option()
                .map(|r| r.hw_accel)
                .or(Some(options.window_state.renderer_options.hw_accel));
            !matches!(
                AzBackend::resolve(hw_accel),
                AzBackend::Cpu | AzBackend::Headless
            )
        };

        // Get window size
        let (width, height) = wcreate::get_client_rect(hwnd, &win32)?;
        let physical_size = azul_core::geom::PhysicalSize::new(width, height);

        let new_frame_ready =
            std::sync::Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));

        // Try GPU path: GL context + WebRender; fall back to CPU if anything fails
        let (
            render_mode,
            renderer,
            render_api,
            hit_tester,
            document_id,
            id_namespace,
            gl_context_ptr,
        ) = if should_use_hardware {
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
                            (post_message)(hwnd_for_notifier as HWND, WM_APP_FRAME_READY, 0, 0);
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
                    width,
                    height
                );

                Ok((
                    RenderMode::Gpu {
                        gl_context: hglrc,
                        hdc,
                    },
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
                    (
                        RenderMode::Cpu,
                        None,
                        None,
                        None,
                        None,
                        None,
                        OptionGlContextPtr::None,
                    )
                }
            }
        } else {
            log_info!(
                LogCategory::Rendering,
                "[Win32] Hardware acceleration disabled, using CPU rendering"
            );
            (
                RenderMode::Cpu,
                None,
                None,
                None,
                None,
                None,
                OptionGlContextPtr::None,
            )
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
        // Shares the app-level manager's font pools rather than starting a
        // private universe; falls back to a fresh one when there is none.
        let mut layout_window =
            crate::desktop::shell2::common::layout::layout_window_sharing_fonts(
                app_font_manager.as_ref(),
                &fc_cache,
            )
            .map_err(|e| {
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

        // Position window on the REQUESTED monitor (or centre on the first
        // one). `Monitor::default().monitor_id` is MonitorId::PRIMARY, so
        // passing it here threw `options.window_state.monitor_id` away and
        // opened every window on monitors[0] regardless of what the caller
        // asked for. `hash: 0` is right: position_window_on_monitor matches on
        // index first and only falls back to a NON-zero hash, so a
        // hash-less id means "resolve me by index".
        let target_monitor_id = azul_core::window::MonitorId {
            index: current_window_state.monitor_id.into_option().unwrap_or(0) as usize,
            hash: 0,
        };
        position_window_on_monitor(
            hwnd,
            target_monitor_id,
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
            let mut ctx = azul_css::dynamic_selector::DynamicSelectorContext::from_system_style(
                &system_style,
            );
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
        let mut common = event::CommonWindowState::new(
            current_window_state,
            fc_cache,
            system_style,
            app_data,
            undo_manager,
        );
        common.layout_window = Some(layout_window);
        common.gl_context_ptr = gl_context_ptr;
        common.renderer = renderer;
        common.render_api = render_api;
        common.hit_tester = hit_tester;
        // Always allocated, GPU mode included. The CPU tester is now the ONLY
        // hit tester (`perform_hit_test` no longer consults WebRender's), so
        // gating it on the render backend left GPU windows with `None` and no
        // way to resolve a pointer event at all.
        common.cpu_hit_tester = Some(azul_layout::headless::CpuHitTester::new());
        common.document_id = document_id;
        common.id_namespace = id_namespace;

        let mut result = Win32Window {
            hwnd,
            owned_popup: owner_hwnd.is_some(),
            owner_id: owner_hwnd.map_or(0, |h| h as usize as u64),
            hinstance,
            render_mode,
            gl_functions,
            new_frame_ready,
            #[cfg(feature = "cpurender")]
            cpu_backend: crate::desktop::shell2::headless::CpuBackend::new(),
            #[cfg(feature = "cpurender")]
            bgra_buffer: Vec::new(),
            #[cfg(feature = "cpurender")]
            native_dib: None,
            gpu_damage_rects: Vec::new(),
            common,
            win32, // Store Win32 libraries for later use
            is_open: true,
            first_frame_shown: false, // Window will be shown after first SwapBuffers
            needs_gpu_present: false,
            menu_bar,
            context_menu: None,
            pending_native_menu: None,
            timers: HashMap::new(),
            thread_timer_running: None,
            high_surrogate: None,
            gesture_zoom_baseline: 0.0,
            ime_composition: None,
            ime_enabled: true,
            ime_saved_himc: std::ptr::null_mut(),
            ime_sync_key: event::ImeSyncKey::default(),
            dpi: dpi_functions,
            font_registry,
            dynamic_selector_context,
            icon_provider: azul_core::icon::SharedIconProvider::from_handle(
                config.icon_provider.clone(),
            ),
            app_config: config.clone(),
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
            let initial_material = result
                .common
                .current_window_state()
                .flags
                .background_material;
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
            // Send first frame: regenerate layout + full transaction.
            // Epoch captured BEFORE the pass so the retirement below cannot eat
            // a request raised by a lifecycle callback running inside it.
            // Window / taskbar icon, before the first show so the title bar and
            // Alt+Tab never flash the default icon.
            unsafe {
                apply_window_icons(
                    &result.win32,
                    result.hwnd,
                    &options
                        .window_state
                        .platform_specific_options
                        .windows_options,
                );
            }

            let regen_epoch_seen = result.common.regen_epoch();
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
                        let root_size = dom_result.layout_tree.get_content_size(
                            azul_layout::solver3::layout_tree::LayoutNodeId::new(
                                dom_result.layout_tree.root,
                            ),
                        );
                        // root_size is LOGICAL; the OS sizes the OUTER frame
                        // in PHYSICAL px — scale by DPI and fit the CLIENT
                        // area (set_client_size adds the frame delta), or the
                        // content gets clipped by frame + DPI factor.
                        let w = libm::roundf(root_size.width * dpi_factor).max(1.0) as i32;
                        let h = libm::roundf(root_size.height * dpi_factor).max(1.0) as i32;
                        log_trace!(
                            LogCategory::Window,
                            "[Win32] size_to_content: resizing client area to {}x{}px",
                            w,
                            h
                        );
                        if let Err(e) = wcreate::set_client_size(result.hwnd, w, h, &result.win32) {
                            log_warn!(
                                LogCategory::Window,
                                "[Win32] size_to_content set_client_size failed: {:?}",
                                e
                            );
                        }
                    }
                }
            }

            // The initial request is satisfied by the pass above — retire only
            // what that pass observed.
            result
                .common
                .clear_regeneration_unless_reraised(regen_epoch_seen);
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
            let app_data = result.common.app_data.clone();
            let borrows = result.common.layout_borrows();
            let layout_window = borrows
                .layout_window
                .expect("LayoutWindow should exist at this point");
            // Get app_data for callback
            let mut app_data_ref = app_data.borrow_mut();

            let (changes, _update) = layout_window.invoke_single_callback(
                &mut callback,
                &mut *app_data_ref,
                &raw_handle,
                borrows.gl_context_ptr,
                borrows.system_style.clone(),
                &azul_layout::callbacks::ExternalSystemCallbacks::rust_internal(),
                borrows.previous_window_state,
                borrows.current_window_state,
                borrows.renderer_resources,
            );

            drop(app_data_ref);
            use crate::desktop::shell2::common::event::PlatformWindow;
            for change in &changes {
                let r = result.apply_user_change(change);
                if r != azul_core::events::ProcessEventResult::DoNothing {
                    result
                        .common
                        .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
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

    /// Win32 timer ID reserved for the modal size/move pump (~60 FPS tick).
    ///
    /// Distinct from [`Self::THREAD_POLL_TIMER_ID`] on purpose: `SetTimer` with
    /// an id that is already in use REPLACES that timer, so sharing one would
    /// silently kill background-thread polling for the rest of the run and the
    /// `KillTimer` at `WM_EXITSIZEMOVE` would never bring it back.
    pub(crate) const MODAL_LOOP_TIMER_ID: usize = 0xFFFE;
    /// Interval in milliseconds for the modal size/move pump (~60 FPS).
    const MODAL_LOOP_INTERVAL_MS: u32 = 16;

    /// Arm the stand-in for the outer event loop, for the duration of a modal
    /// size/move loop (`WM_ENTERSIZEMOVE` … `WM_EXITSIZEMOVE`).
    ///
    /// See [`pump_modal_loop_work`] for what stalls without it.
    pub(crate) fn start_modal_loop_pump(&mut self) {
        unsafe {
            (self.win32.user32.SetTimer)(
                self.hwnd,
                Self::MODAL_LOOP_TIMER_ID,
                Self::MODAL_LOOP_INTERVAL_MS,
                ptr::null(),
            );
        }
    }

    /// Disarm it again. `KillTimer` must be passed the SAME nIDEvent given to
    /// `SetTimer` for window timers (the same contract as `stop_timer`), and is
    /// harmless if the timer was never armed.
    pub(crate) fn stop_modal_loop_pump(&mut self) {
        unsafe {
            (self.win32.user32.KillTimer)(self.hwnd, Self::MODAL_LOOP_TIMER_ID);
        }
    }

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
                let window_state = self.common.current_window_state().clone();
                if let Some(ref mut layout_window) = self.common.layout_window {
                    layout_window.current_window_state = window_state;

                    // Advance easing-based scroll animations
                    {
                        #[cfg(feature = "std")]
                        let now =
                            azul_core::task::Instant::System(std::time::Instant::now().into());
                        #[cfg(not(feature = "std"))]
                        let now = azul_core::task::Instant::Tick(azul_core::task::SystemTick {
                            tick_counter: 0,
                        });
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
                // One drain for every backend: it re-invokes in place AND rebuilds
                // the CPU hit-tester (the rebuilt child DOMs carry fresh NodeIds).
                self.common.drain_virtual_view_updates();

                // Shared per-frame content preparation (journal clock, image
                // callbacks through the content chokepoint, scrollbar cache).
                // The logic lives in LayoutWindow so no backend can skip a piece.
                if let Some(lw) = self.common.layout_window.as_mut() {
                    lw.prepare_frame_cpu();
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
                            // #27 native backbuffer (Windows): a persistent
                            // RGBA-mask DIB section is the render target —
                            // the renderer draws into it directly and the
                            // present BitBlts damage rects from the memory
                            // DC. Whether GDI honors BI_BITFIELDS masks in
                            // R=0x000000FF order varies by stack, so a
                            // one-time RUNTIME PROBE (write a red pixel, read
                            // it back via GetPixel) decides; failure disables
                            // native mode for the process and the legacy
                            // swizzle+StretchDIBits path continues unchanged.
                            // Single buffer + synchronous BitBlt ⇒ it always
                            // holds frame N−1 (catch-up by construction).
                            // BLIND-IMPLEMENTED per USER directive 2026-08-12
                            // — not yet run on real Windows.
                            let native_pw = (width * dpi).ceil() as i32;
                            let native_ph = (height * dpi).ceil() as i32;
                            if crate::desktop::shell2::headless::native_backbuffer_enabled()
                                && NATIVE_DIB_SUPPORTED.load(core::sync::atomic::Ordering::Relaxed)
                                && native_pw > 0
                                && native_ph > 0
                            {
                                let needs_new = match self.native_dib {
                                    Some(ref d) => d.w != native_pw || d.h != native_ph,
                                    None => true,
                                };
                                if needs_new {
                                    if let Some(d) = self.native_dib.take() {
                                        unsafe {
                                            (self.win32.gdi32.SelectObject)(d.mem_dc, d.old_bitmap);
                                            (self.win32.gdi32.DeleteObject)(d.bitmap);
                                            (self.win32.gdi32.DeleteDC)(d.mem_dc);
                                        }
                                    }
                                    unsafe {
                                        let wnd_dc = (self.win32.user32.GetDC)(self.hwnd);
                                        if !wnd_dc.is_null() {
                                            let bmi = dlopen::BitmapInfoBitfields {
                                                header: dlopen::BitmapInfoHeader {
                                                    biSize: core::mem::size_of::<
                                                        dlopen::BitmapInfoHeader,
                                                    >(
                                                    )
                                                        as u32,
                                                    biWidth: native_pw,
                                                    // negative = top-down rows,
                                                    // matching the renderer.
                                                    biHeight: -native_ph,
                                                    biPlanes: 1,
                                                    biBitCount: 32,
                                                    biCompression: dlopen::BI_BITFIELDS,
                                                    biSizeImage: 0,
                                                    biXPelsPerMeter: 0,
                                                    biYPelsPerMeter: 0,
                                                    biClrUsed: 0,
                                                    biClrImportant: 0,
                                                },
                                                // R,G,B in the renderer's byte
                                                // order (alpha = remaining byte).
                                                masks: [0x0000_00FF, 0x0000_FF00, 0x00FF_0000],
                                            };
                                            let mut bits: *mut c_void = core::ptr::null_mut();
                                            let bitmap = (self.win32.gdi32.CreateDIBSection)(
                                                wnd_dc,
                                                &bmi as *const _ as *const dlopen::BitmapInfoHeader,
                                                dlopen::DIB_RGB_COLORS,
                                                &mut bits,
                                                core::ptr::null_mut(),
                                                0,
                                            );
                                            if !bitmap.is_null() && !bits.is_null() {
                                                let mem_dc =
                                                    (self.win32.gdi32.CreateCompatibleDC)(wnd_dc);
                                                if !mem_dc.is_null() {
                                                    let old = (self.win32.gdi32.SelectObject)(
                                                        mem_dc, bitmap,
                                                    );
                                                    // PROBE: RGBA red at (0,0)
                                                    // must read back as COLORREF
                                                    // red (0x000000FF).
                                                    let p = bits as *mut u8;
                                                    *p.add(0) = 0xFF;
                                                    *p.add(1) = 0x00;
                                                    *p.add(2) = 0x00;
                                                    *p.add(3) = 0xFF;
                                                    let col =
                                                        (self.win32.gdi32.GetPixel)(mem_dc, 0, 0);
                                                    if col & 0x00FF_FFFF == 0x0000_00FF {
                                                        self.native_dib = Some(NativeDib {
                                                            mem_dc,
                                                            bitmap,
                                                            old_bitmap: old,
                                                            ptr: bits as *mut u8,
                                                            w: native_pw,
                                                            h: native_ph,
                                                        });
                                                    } else {
                                                        log_warn!(
                                                            LogCategory::Rendering,
                                                            "[native-bb] GDI ignores RGBA \
                                                             DIB masks (probe read {:#08x}) \
                                                             — legacy path for this process",
                                                            col
                                                        );
                                                        NATIVE_DIB_SUPPORTED.store(
                                                            false,
                                                            core::sync::atomic::Ordering::Relaxed,
                                                        );
                                                        (self.win32.gdi32.SelectObject)(
                                                            mem_dc, old,
                                                        );
                                                        (self.win32.gdi32.DeleteObject)(bitmap);
                                                        (self.win32.gdi32.DeleteDC)(mem_dc);
                                                    }
                                                } else {
                                                    (self.win32.gdi32.DeleteObject)(bitmap);
                                                }
                                            }
                                            (self.win32.user32.ReleaseDC)(self.hwnd, wnd_dc);
                                        }
                                    }
                                }
                                if let Some(ref d) = self.native_dib {
                                    self.cpu_backend.native_target = unsafe {
                                        azul_layout::cpurender::AzulPixmap::from_external(
                                            d.ptr, d.w as u32, d.h as u32,
                                        )
                                    };
                                }
                            }

                            // Shared CPU renderer (same path as headless + X11 +
                            // Wayland + macOS): damage diff + scroll-offset feed +
                            // thin-strip scroll-shift with eligibility + offset-aware
                            // render. Replaces the logic that used to live here and
                            // lacked all the scroll machinery (#13/#14).
                            // Transparent material: the frame clears to alpha 0;
                            // the DWM (blur-behind, see apply_background_material)
                            // composites the DIB's premultiplied alpha.
                            self.cpu_backend
                                .sync_window_flags(&layout_window.current_window_state);
                            self.cpu_backend.render_frame(
                                layout_window,
                                &layout_window.renderer_resources,
                                width,
                                height,
                                dpi,
                            );
                            // Dangle guard: render_frame's early returns must not
                            // leave a pointer into the DIB armed across frames.
                            self.cpu_backend.native_target = None;

                            if self.cpu_backend.rendered_native {
                                // #27: pixels are already in the DIB section —
                                // present = BitBlt the damage rects. WM_PAINT
                                // full-rect fallback mirrors the legacy path
                                // (FrameDamage::None → one full rect).
                                if let Some(ref d) = self.native_dib {
                                    let rects = self
                                        .cpu_backend
                                        .last_present_damage
                                        .to_present_rects_physical(
                                            dpi, d.w as u32, d.h as u32, false,
                                        )
                                        .unwrap_or_else(|| vec![(0, 0, d.w as u32, d.h as u32)]);
                                    unsafe {
                                        let hdc = (self.win32.user32.GetDC)(self.hwnd);
                                        if !hdc.is_null() {
                                            for (rx, ry, rw, rh) in rects {
                                                (self.win32.gdi32.BitBlt)(
                                                    hdc,
                                                    rx as i32,
                                                    ry as i32,
                                                    rw as i32,
                                                    rh as i32,
                                                    d.mem_dc,
                                                    rx as i32,
                                                    ry as i32,
                                                    dlopen::SRCCOPY,
                                                );
                                            }
                                            (self.win32.user32.ReleaseDC)(self.hwnd, hdc);
                                        }
                                    }
                                }
                                rendered = true;
                            } else
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
                                            let rect_bytes = (rw as usize) * (rh as usize) * 4;
                                            self.bgra_buffer.resize(rect_bytes, 0);
                                            for row in 0..rh as usize {
                                                let so = (ry as usize + row) * src_stride
                                                    + (rx as usize) * 4;
                                                let doff = row * (rw as usize) * 4;
                                                let n = (rw as usize) * 4;
                                                for (s, d) in data[so..so + n].chunks_exact(4).zip(
                                                    self.bgra_buffer[doff..doff + n]
                                                        .chunks_exact_mut(4),
                                                ) {
                                                    d[0] = s[2]; // B
                                                    d[1] = s[1]; // G
                                                    d[2] = s[0]; // R
                                                    d[3] = s[3]; // A
                                                }
                                            }

                                            let bmi = dlopen::BitmapInfoHeader {
                                                biSize: core::mem::size_of::<dlopen::BitmapInfoHeader>(
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
                            // A transparent window's input + visual shape
                            // follows the frame's alpha (SetWindowRgn).
                            if let Some(rects) = self.cpu_backend.take_changed_shape() {
                                self.apply_window_shape(&rects);
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
                if self.common.current_window_state().flags.is_visible && !rendered {
                    log_trace!(
                        LogCategory::Rendering,
                        "[Win32 CPU] first frame not rendered yet — deferring ShowWindow"
                    );
                    self.request_redraw();
                } else {
                    if self.common.current_window_state().flags.is_visible {
                        use azul_core::window::WindowFrame;
                        use dlopen::constants::{SW_MAXIMIZE, SW_MINIMIZE, SW_SHOWNORMAL};
                        let show_cmd = match self.common.current_window_state().flags.frame {
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
            let needs_fade_frame = self
                .common
                .layout_window
                .as_ref()
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
        let RenderMode::Gpu {
            gl_context,
            hdc: stored_hdc,
        } = &self.render_mode
        else {
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
                    let scroll_active = self
                        .common
                        .layout_window
                        .as_ref()
                        .map(|lw| {
                            lw.scroll_manager.has_active_animations() || lw.needs_animation_frame()
                        })
                        .unwrap_or(false);
                    let scrollbar_fade = self
                        .common
                        .layout_window
                        .as_ref()
                        .map(|lw| lw.gpu_state_manager.scrollbar_fade_active)
                        .unwrap_or(false);
                    let virtual_view_pending = self
                        .common
                        .layout_window
                        .as_ref()
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
                    let want_redraw = self.needs_gpu_present || self.common.display_list_dirty;
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
                        let now =
                            azul_core::task::Instant::System(std::time::Instant::now().into());
                        #[cfg(not(feature = "std"))]
                        let now = azul_core::task::Instant::Tick(azul_core::task::SystemTick {
                            tick_counter: 0,
                        });
                        let tick_result = layout_window.scroll_manager.tick(now);
                        if tick_result.needs_repaint {
                            layout_window.scroll_manager.calculate_scrollbar_states();
                        }
                    }

                    let has_virtual_view_updates =
                        !layout_window.pending_virtual_view_updates.is_empty();
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
                                crate::desktop::wr_translate2::wr_translate_document_id(
                                    document_id,
                                ),
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
            let dpi_scale = self.common.current_window_state().size.dpi as f32 / 96.0;
            self.gpu_damage_rects = results
                .dirty_rects
                .iter()
                .map(|dr| azul_core::geom::LogicalRect {
                    origin: azul_core::geom::LogicalPosition {
                        x: dr.min.x as f32 / dpi_scale,
                        y: dr.min.y as f32 / dpi_scale,
                    },
                    size: azul_core::geom::LogicalSize {
                        width: dr.width() as f32 / dpi_scale,
                        height: dr.height() as f32 / dpi_scale,
                    },
                })
                .collect();

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
                if self.common.current_window_state().flags.is_visible {
                    if let Some(ref dwmapi) = self.win32.dwmapi_funcs {
                        (dwmapi.DwmFlush)();
                    }
                    use azul_core::window::WindowFrame;
                    use dlopen::constants::{SW_MAXIMIZE, SW_MINIMIZE, SW_SHOWNORMAL};
                    let show_cmd = match self.common.current_window_state().flags.frame {
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
            let needs_fade_frame = self
                .common
                .layout_window
                .as_ref()
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
    pub fn regenerate_layout_inner(
        &mut self,
    ) -> Result<crate::desktop::shell2::common::layout::LayoutRegenerateResult, String> {
        // Consume the reason tag BEFORE borrowing the layout window: this is
        // the regeneration this window asked for, and the tag travels with
        // the request (see CommonWindowState::request_regeneration).
        let relayout_reason = self.common.take_relayout_reason();

        let borrows = self.common.layout_borrows();
        let layout_window = borrows.layout_window.ok_or("No layout window")?;

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
            borrows.app_data,
            borrows.current_window_state,
            borrows.renderer_resources,
            borrows.gl_context_ptr,
            borrows.fc_cache,
            &self.font_registry,
            borrows.system_style,
            &self.icon_provider,
            &mut debug_messages,
            relayout_reason,
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

        // Update accessibility tree after layout (take, not clone — the
        // flush_a11y_tree_update hook drains the same slot at end-of-pass;
        // MWA-A3e, matches the wayland/macOS backends)
        #[cfg(feature = "a11y")]
        {
            // Scroll moved the content: throttled full rebuild into the slot
            // (bounds + scroll_x/y) before the slot is drained below.
            use crate::desktop::shell2::common::event::PlatformWindow;
            self.rebuild_a11y_after_scroll_if_due();
        }
        #[cfg(feature = "a11y")]
        if let Some(layout_window) = self.common.layout_window.as_mut() {
            if let Some(tree_update) = layout_window.a11y_manager.take_pending() {
                self.accessibility_adapter.update_tree(tree_update);
            }
        }

        // Send frame to WebRender (GPU mode only - CPU mode reads display list directly)
        if let RenderMode::Gpu {
            gl_context: hglrc,
            hdc: stored_hdc,
        } = &self.render_mode
        {
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
        // Unconditional: the CPU tester is the only hit tester now, so a GPU
        // window needs it rebuilt exactly as much as a CPU one.
        self.common.rebuild_cpu_hit_tester();

        // Drain lifecycle events (Mount / AfterMount / Unmount) produced by this
        // layout's reconciliation — the SAME step headless + X11 run. Without it,
        // EventFilter::Component(AfterMount) callbacks never fire on Windows (e.g.
        // the MapWidget's first tile fetch never starts). Windows already polls
        // background threads via its WM_TIMER (start_thread_poll_timer → SetTimer),
        // so once AfterMount spawns them their writebacks drain.

        // Phase 2: Post-Layout callback - sync IME position after layout (MOST IMPORTANT)
        self.sync_ime_state();

        Ok(result)
    }

    /// Build + send the WebRender display-list transaction (GPU) / rebuild the CPU
    /// hit-tester after an *incremental* relayout — the "finalize" tail that
    /// `regenerate_layout()` runs after layout, MINUS the layout-callback /
    /// StyledDom rebuild.
    ///
    /// `incremental_relayout()` (called from the `ShouldIncrementalRelayout` event
    /// arm) re-runs layout on the existing StyledDom but, unlike
    /// `regenerate_layout()`, does NOT send a frame. The relayout-only
    /// WM_PAINT branch calls this so the restyle still reaches `generate_frame` /
    /// the present path. Mirrors `regenerate_layout()`'s GPU `generate_frame` + CPU
    /// hit-tester tail.
    fn send_frame_after_incremental_relayout(&mut self) {
        // Send frame to WebRender (GPU mode only - CPU mode reads display list directly)
        if let RenderMode::Gpu {
            gl_context: hglrc,
            hdc: stored_hdc,
        } = &self.render_mode
        {
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
        // Unconditional: the CPU tester is the only hit tester now, so a GPU
        // window needs it rebuilt exactly as much as a CPU one.
        self.common.rebuild_cpu_hit_tester();
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
    /// requested a regeneration NOR took the incremental-relayout fast path, and
    /// WM_PAINT then just repainted the STALE layout.
    ///
    /// Mirrors the `WM_COMMAND` `match event_result` arm:
    /// - `ShouldIncrementalRelayout` → `incremental_relayout()` on the existing
    ///   StyledDom + `request_relayout_only()`, then invalidate (WM_PAINT's
    ///   relayout-only branch sends the frame).
    /// - `ShouldRegenerateDom* | UpdateHitTesterAndProcessAgain` →
    ///   `request_regeneration()` + invalidate (full `regenerate_layout()` in
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
                // + the WM_COMMAND menu arm. The relayout-only request then makes WM_PAINT
                // skip regenerate_layout and only rebuild + send the WebRender
                // transaction.
                let mut debug_messages = None;
                if let Err(e) = self.incremental_relayout_dispatching(
                    crate::desktop::shell2::common::event::IncrementalRelayout::Restyle,
                    &mut debug_messages,
                ) {
                    log_warn!(LogCategory::Layout, "Incremental relayout failed: {}", e);
                }
                self.common.request_relayout_only();
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
                            w.common.request_regeneration(
                                azul_core::callbacks::RelayoutReason::RefreshDom,
                            );
                            unsafe {
                                (w.win32.user32.InvalidateRect)(other_hwnd, ptr::null(), 0);
                            }
                        }
                    }
                }
                self.common
                    .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
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

        // The focus, the editing session or the caret may have moved in the
        // pass whose result this is routing — the IME association and the
        // over-the-spot composition/candidate windows follow from exactly
        // those, and NONE of the routes above run `regenerate_layout_inner`'s
        // tail (the only place this used to happen). Gated internally on the
        // identity actually having changed. Every main-window input handler
        // plus WM_SETFOCUS / WM_KILLFOCUS ends here; the WM_COMMAND menu arm
        // routes its own result and is picked up by the WM_PAINT it schedules.
        self.sync_ime_state();
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
        let mut pt = dlopen::POINT {
            x: screen_x,
            y: screen_y,
        };
        unsafe {
            (self.win32.user32.ScreenToClient)(self.hwnd, &mut pt);
        }
        let hf = self.common.current_window_state().size.get_hidpi_factor();
        let pos = azul_core::geom::LogicalPosition::new(
            pt.x as f32 / hf.inner.get(),
            pt.y as f32 / hf.inner.get(),
        );
        self.common.mouse_state_mut().cursor_position = CursorPosition::InWindow(pos);
    }

    /// MWA-B7: the OS drag moved over the window (OLE DragOver) — refresh
    /// position + hit test so HoveredFile re-targets the node under the
    /// drag. Previously DragOver did nothing positional at all.
    pub fn handle_file_drag_moved(&mut self, screen_pt: (i32, i32)) -> ProcessEventResult {
        self.snapshot_window_state_baseline("windows.handle_file_drag_moved");
        self.set_drag_cursor_from_screen(screen_pt.0, screen_pt.1);
        self.update_file_drag_hit_test();
        self.process_window_events(0)
    }

    fn update_file_drag_hit_test(&mut self) {
        use azul_core::window::CursorPosition;
        if let CursorPosition::InWindow(pos) = self
            .common
            .current_window_state()
            .mouse_state
            .cursor_position
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
        self.snapshot_window_state_baseline("windows.handle_file_drag_entered");
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
        self.snapshot_window_state_baseline("windows.handle_file_drag_exited");

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
        self.snapshot_window_state_baseline("windows.handle_file_drop");
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
                self.common.update_unsynced_state(|ws| {
                    ws.ime_position = ImePosition::Initialized(cursor_rect);
                });
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
    /// This method applies the remaining fields and seeds both baselines
    /// (event-diff and OS-sync) so that sync_window_state() works correctly for
    /// future changes.
    /// Created as an owned popup of another azul window (see `new`).
    const fn is_owned_popup(&self) -> bool {
        self.owned_popup
    }

    fn apply_initial_window_state(&mut self) {
        // is_always_on_top — except for an owned popup, which already orders
        // above its owner and must not float over other applications.
        if self.common.current_window_state().flags.is_always_on_top && !self.is_owned_popup() {
            use dlopen::constants::*;
            unsafe {
                (self.win32.user32.SetWindowPos)(
                    self.hwnd,
                    HWND_TOPMOST,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE,
                );
            }
        }

        // is_resizable (default is true via WS_THICKFRAME; apply if user wants non-resizable)
        if !self.common.current_window_state().flags.is_resizable {
            use dlopen::constants::*;
            unsafe {
                let style = (self.win32.user32.GetWindowLongPtrW)(self.hwnd, GWL_STYLE);
                let new_style = style & !((WS_THICKFRAME | WS_MAXIMIZEBOX) as isize);
                (self.win32.user32.SetWindowLongPtrW)(self.hwnd, GWL_STYLE, new_style);
                (self.win32.user32.SetWindowPos)(
                    self.hwnd,
                    std::ptr::null_mut(),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
                );
            }
        }

        // is_top_level
        if self.common.current_window_state().flags.is_top_level {
            let _ = self.set_is_top_level(true);
        }

        // prevent_system_sleep
        if self
            .common
            .current_window_state()
            .flags
            .prevent_system_sleep
        {
            let _ = self.set_prevent_system_sleep(true);
        }

        // Seed BOTH baselines: the event-diff one (so the first pass has
        // something to diff against) and the OS-sync one — everything above is
        // now applied on the HWND, so the first sync_window_state() must not
        // re-push the whole initial state (title, size, position, visibility,
        // frame) at the OS.
        self.seed_window_state_baseline("windows.apply_initial_window_state");
        self.common.mark_os_synced();
    }

    /// Hand the drag to WINDOWS via `WM_NCLBUTTONDOWN` with `HTCAPTION` — the
    /// standard way to say "treat this as a title-bar press".
    ///
    /// `ReleaseCapture()` first, because the click that started the drag left
    /// this window holding the mouse capture and Windows will not run its own
    /// move loop while someone else owns it.
    ///
    /// The manual alternative — reading the cursor each mouse-move and writing
    /// a new window position — makes the window trail the pointer, and once the
    /// pointer leaves the dragged element the moves stop arriving at all, so
    /// the drag dies mid-gesture. The OS loop owns the pointer until the button
    /// comes up, and brings snap layouts and multi-monitor DPI with it.
    ///
    /// Wayland and macOS already took their native paths; Windows and X11 fell
    /// through to the no-op default in `common::event`, which is why dragging
    /// felt worse on exactly those two.
    fn handle_begin_interactive_move(&mut self) {
        const WM_NCLBUTTONDOWN: u32 = 0x00A1;
        const HTCAPTION: usize = 2;
        unsafe {
            (self.win32.user32.ReleaseCapture)();
            // `WM_NCLBUTTONDOWN`'s lParam is documented as the cursor in
            // SCREEN coordinates. DefWindowProc's move loop anchors on the
            // live cursor rather than on lParam, so `0` works — but a
            // hook or a future `WM_NCHITTEST` handler reads it, so send
            // what the contract says.
            let mut pt = dlopen::POINT { x: 0, y: 0 };
            (self.win32.user32.GetCursorPos)(&mut pt);
            // MAKELPARAM: LOWORD = x, HIWORD = y, each a WORD — a negative
            // (left-of-primary) coordinate must truncate, not sign-extend.
            let lparam = ((pt.y as u16 as usize) << 16) | (pt.x as u16 as usize);
            (self.win32.user32.PostMessageW)(
                self.hwnd,
                WM_NCLBUTTONDOWN,
                HTCAPTION,
                lparam as dlopen::LPARAM,
            );
        }
    }

    /// A native size/move loop just ended. USER32 swallows the `WM_LBUTTONUP`
    /// that ends it (the loop breaks on it without dispatching; the app only
    /// sees `WM_EXITSIZEMOVE` / `WM_CAPTURECHANGED`), so `left_down` — set
    /// true by the press that STARTED the drag — would stay latched: the next
    /// press diffs `true → true` and produces no `MouseDown`, and every motion
    /// reads as a drag. Same shape as the `WM_KILLFOCUS` reset, gated on the
    /// button really being up so a loop that ends for another reason (Esc)
    /// with the button still held does not fake a release.
    fn release_buttons_swallowed_by_modal_loop(&mut self, hwnd: HWND) {
        const VK_LBUTTON: i32 = 0x01;
        const VK_RBUTTON: i32 = 0x02;
        const VK_MBUTTON: i32 = 0x04;
        let up = |vk: i32| unsafe { (self.win32.user32.GetKeyState)(vk) } >= 0;
        let (left_up, right_up, middle_up) = (up(VK_LBUTTON), up(VK_RBUTTON), up(VK_MBUTTON));
        let latched = {
            let ms = self.common.mouse_state_mut();
            (ms.left_down && left_up)
                || (ms.right_down && right_up)
                || (ms.middle_down && middle_up)
        };
        if !latched {
            return;
        }
        let prev_snapshot = self.common.current_window_state().clone();
        {
            let ms = self.common.mouse_state_mut();
            if left_up {
                ms.left_down = false;
            }
            if right_up {
                ms.right_down = false;
            }
            if middle_up {
                ms.middle_down = false;
            }
        }
        self.set_previous_window_state(prev_snapshot);
        let r = self.process_window_events(0);
        self.route_main_window_result(hwnd, r);
    }

    /// `WM_NCCALCSIZE` for a frameless window: the whole window rect is
    /// client area — the frame styles stay (so the DWM draws the shadow and
    /// corners, and `SC_SIZE` can resize), the frame's AREA goes to us.
    ///
    /// Maximized, the OS lays the (now invisible) frame out OUTSIDE the
    /// monitor, so "the whole window rect" would overhang it on every side
    /// and, without a caption to reserve it, cover the taskbar. Pin the
    /// client rect to the monitor's work area instead.
    ///
    /// Returns `Some(lresult)` when handled, `None` for DefWindowProc.
    fn handle_nccalcsize(
        &mut self,
        hwnd: HWND,
        wparam: dlopen::WPARAM,
        lparam: dlopen::LPARAM,
    ) -> Option<dlopen::LRESULT> {
        use azul_core::window::WindowDecorations;
        if !matches!(
            self.common.current_window_state().flags.decorations,
            WindowDecorations::None
        ) {
            return None;
        }
        // wParam == FALSE: lParam is a bare RECT and "return 0" already means
        // "client = window". wParam == TRUE: lParam is NCCALCSIZE_PARAMS,
        // whose first RECT is the proposed window rect to turn into the
        // client rect — the same memory either way.
        if lparam == 0 {
            return Some(0);
        }
        if wparam != 0 {
            unsafe {
                let rect = lparam as *mut dlopen::RECT;
                if (self.win32.user32.IsZoomed)(hwnd) != 0 {
                    let monitor = (self.win32.user32.MonitorFromWindow)(
                        hwnd,
                        dlopen::MONITOR_DEFAULTTONEAREST,
                    );
                    if !monitor.is_null() {
                        let mut mi: dlopen::MONITORINFOEXW = core::mem::zeroed();
                        mi.cbSize = core::mem::size_of::<dlopen::MONITORINFOEXW>() as u32;
                        if (self.win32.user32.GetMonitorInfoW)(monitor, &mut mi) != 0 {
                            *rect = mi.rcWork;
                        }
                    }
                }
            }
        }
        Some(0)
    }

    /// `WM_NCHITTEST` for a frameless window.
    ///
    /// DefWindowProc answers this from the window STYLE, not from what
    /// `WM_NCCALCSIZE` said: with the frame styles kept (see `wcreate.rs`)
    /// it would still report the top ~31 px of our client area as
    /// `HTCAPTION` — every press there would start a native drag or, on a
    /// double-click, maximize — and the outer 8 px as a resize band. So
    /// answer it ourselves: the resize band (when resizable and not
    /// maximized) stays native, because `SC_SIZE` is what makes the CSD
    /// edges resize at all; everything else is `HTCLIENT`, and the
    /// `-azul-app-region: drag` path turns the regions the app chose into a
    /// native move via `WM_NCLBUTTONDOWN` / `HTCAPTION` on its own.
    ///
    /// Returns `Some(hit code)` when handled, `None` for DefWindowProc.
    fn handle_nchittest(&mut self, hwnd: HWND, lparam: dlopen::LPARAM) -> Option<dlopen::LRESULT> {
        use azul_core::window::WindowDecorations;
        const HTCLIENT: isize = 1;
        const HTLEFT: isize = 10;
        const HTRIGHT: isize = 11;
        const HTTOP: isize = 12;
        const HTTOPLEFT: isize = 13;
        const HTTOPRIGHT: isize = 14;
        const HTBOTTOM: isize = 15;
        const HTBOTTOMLEFT: isize = 16;
        const HTBOTTOMRIGHT: isize = 17;

        let ws = self.common.current_window_state();
        if !matches!(ws.flags.decorations, WindowDecorations::None) {
            return None;
        }
        let resizable = ws.flags.is_resizable;
        let dpi_factor = dpi::dpi_to_scale_factor(ws.size.dpi);

        // lParam: cursor in SCREEN coordinates, signed 16-bit halves.
        let x = (lparam & 0xFFFF) as u16 as i16 as i32;
        let y = ((lparam >> 16) & 0xFFFF) as u16 as i16 as i32;

        let (maximized, rect) = unsafe {
            let mut wr = dlopen::RECT::default();
            let ok = (self.win32.user32.GetWindowRect)(hwnd, &mut wr) != 0;
            ((self.win32.user32.IsZoomed)(hwnd) != 0, ok.then_some(wr))
        };
        let Some(wr) = rect else {
            return Some(HTCLIENT);
        };
        if maximized || !resizable {
            return Some(HTCLIENT);
        }

        // ONE edge classifier for every frameless backend (X11 / Wayland /
        // Win32): `csd_resize_edge_at` is pure and unit-tested on every CI
        // host, so the band geometry cannot drift per platform. Everything
        // here is in PHYSICAL screen pixels — position, size and band alike.
        use crate::desktop::shell2::common::event::{
            csd_resize_edge_at, CsdResizeEdge, CSD_RESIZE_BAND_PX,
        };
        use azul_core::geom::{LogicalPosition, LogicalSize};
        let band = libm::roundf(CSD_RESIZE_BAND_PX * dpi_factor).max(1.0);
        let edge = csd_resize_edge_at(
            LogicalPosition::new((x - wr.left) as f32, (y - wr.top) as f32),
            LogicalSize::new((wr.right - wr.left) as f32, (wr.bottom - wr.top) as f32),
            band,
        );
        Some(match edge {
            Some(CsdResizeEdge::TopLeft) => HTTOPLEFT,
            Some(CsdResizeEdge::TopRight) => HTTOPRIGHT,
            Some(CsdResizeEdge::BottomLeft) => HTBOTTOMLEFT,
            Some(CsdResizeEdge::BottomRight) => HTBOTTOMRIGHT,
            Some(CsdResizeEdge::Top) => HTTOP,
            Some(CsdResizeEdge::Bottom) => HTBOTTOM,
            Some(CsdResizeEdge::Left) => HTLEFT,
            Some(CsdResizeEdge::Right) => HTRIGHT,
            None => HTCLIENT,
        })
    }

    /// A window CREATED frameless got its creation-time `WM_NCCALCSIZE`
    /// before `GWLP_USERDATA` pointed at this struct, i.e. DefWindowProc
    /// answered it and the window still has a caption-sized non-client
    /// area. Now that `window_proc` can reach `handle_nccalcsize`, recompute
    /// the frame, then re-apply the requested client size against the real
    /// (zero) frame. Call once, right after `GWLP_USERDATA` is set.
    pub fn finish_frameless_frame(&mut self) {
        use azul_core::window::WindowDecorations;
        use dlopen::constants::*;
        if !matches!(
            self.common.current_window_state().flags.decorations,
            WindowDecorations::None
        ) {
            return;
        }
        let want = self.common.current_window_state().size.dimensions;
        let dpi_factor = dpi::dpi_to_scale_factor(self.common.current_window_state().size.dpi);
        unsafe {
            (self.win32.user32.SetWindowPos)(
                self.hwnd,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
        let want_w = libm::roundf(want.width * dpi_factor).max(1.0) as i32;
        let want_h = libm::roundf(want.height * dpi_factor).max(1.0) as i32;
        if let Err(e) = wcreate::set_client_size(self.hwnd, want_w, want_h, &self.win32) {
            log_warn!(
                LogCategory::Window,
                "[Win32] frameless client-size correction failed: {:?}",
                e
            );
        }
    }

    /// Synchronize window state with Windows OS
    ///
    /// Applies changes from current_window_state to the OS window.
    /// Called after callbacks have potentially modified window state.
    fn sync_window_state(&mut self) {
        use std::{ffi::OsStr, os::windows::ffi::OsStrExt};

        // Diff against the OS-SYNC baseline, never `previous_window_state` (the
        // event-diff baseline, which is free to hold a live delta): diffing that
        // here echoed WM_SIZE back as a SetWindowPos and re-issued
        // SetFocus/SetForegroundWindow after WM_SETFOCUS, stealing focus back
        // from whatever the user Alt+Tabbed to. `take_os_sync_diff` advances the
        // baseline as part of the call.
        let (previous, current) = match self.common.take_os_sync_diff() {
            Some(pair) => pair,
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
            // dimensions are the LOGICAL CLIENT-AREA size; SetWindowPos takes
            // the OUTER frame size in PHYSICAL px. Scale by the window DPI and
            // add the current frame delta (outer − client, same trick as the
            // WM_GETMINMAXINFO arm) — passing the raw logical values shrank
            // the client by the frame border on every programmatic resize,
            // and by the whole DPI factor on scaled monitors.
            let hf = current.size.get_hidpi_factor().inner.get();
            let client_w = libm::roundf(current.size.dimensions.width * hf) as i32;
            let client_h = libm::roundf(current.size.dimensions.height * hf) as i32;
            unsafe {
                use dlopen::constants::{SWP_NOMOVE, SWP_NOZORDER};
                let mut wr: dlopen::RECT = std::mem::zeroed();
                let mut cr: dlopen::RECT = std::mem::zeroed();
                (self.win32.user32.GetWindowRect)(self.hwnd, &mut wr);
                (self.win32.user32.GetClientRect)(self.hwnd, &mut cr);
                let frame_w = (wr.right - wr.left) - (cr.right - cr.left);
                let frame_h = (wr.bottom - wr.top) - (cr.bottom - cr.top);
                (self.win32.user32.SetWindowPos)(
                    self.hwnd,
                    std::ptr::null_mut(),
                    0,
                    0,
                    client_w + frame_w,
                    client_h + frame_h,
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
                // A popup's offset from its owner changed (a
                // `<transient-window>` following its anchor, or its tear-off
                // drag): re-place it against the owner's live position.
                WindowPosition::RelativeToParentWindow(offset) => {
                    if let Some((px, py)) = resolve_windows_parent_origin(self.owner_id) {
                        unsafe {
                            use dlopen::constants::{SWP_NOSIZE, SWP_NOZORDER};
                            (self.win32.user32.SetWindowPos)(
                                self.hwnd,
                                std::ptr::null_mut(),
                                px + offset.x,
                                py + offset.y,
                                0,
                                0,
                                SWP_NOSIZE | SWP_NOZORDER,
                            );
                        }
                    }
                }
                // Uninitialized lets the OS decide.
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
                    WindowFrame::Fullscreen => {
                        // Borderless fullscreen: remove WS_OVERLAPPEDWINDOW, resize to monitor
                        let style = (self.win32.user32.GetWindowLongPtrW)(
                            self.hwnd,
                            dlopen::constants::GWL_STYLE,
                        );
                        let new_style = style & !(dlopen::constants::WS_OVERLAPPEDWINDOW as isize);
                        (self.win32.user32.SetWindowLongPtrW)(
                            self.hwnd,
                            dlopen::constants::GWL_STYLE,
                            new_style,
                        );
                        (self.win32.user32.ShowWindow)(self.hwnd, SW_MAXIMIZE);
                    }
                    WindowFrame::Normal => {
                        if previous.flags.frame == WindowFrame::Fullscreen {
                            // Restore window style first
                            let style = (self.win32.user32.GetWindowLongPtrW)(
                                self.hwnd,
                                dlopen::constants::GWL_STYLE,
                            );
                            let new_style =
                                style | (dlopen::constants::WS_OVERLAPPEDWINDOW as isize);
                            (self.win32.user32.SetWindowLongPtrW)(
                                self.hwnd,
                                dlopen::constants::GWL_STYLE,
                                new_style,
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
                let style = (self.win32.user32.GetWindowLongPtrW)(self.hwnd, GWL_STYLE);
                // Frameless keeps EVERY frame style (see `wcreate.rs` for why:
                // the DWM shadow, the corners, snap and `SC_SIZE` all need a
                // frame to exist) and loses only the non-client AREA, which
                // `WM_NCCALCSIZE` hands to the client while `WS_POPUP` is set.
                let new_style = match current.flags.decorations {
                    WindowDecorations::None => {
                        style
                            | (WS_POPUP
                                | WS_CAPTION
                                | WS_SYSMENU
                                | WS_THICKFRAME
                                | WS_MINIMIZEBOX
                                | WS_MAXIMIZEBOX) as isize
                    }
                    _ => {
                        // Normal, NoTitle, NoTitleAutoInject, NoControls all keep basic chrome
                        (style & !(WS_POPUP as isize))
                            | (WS_CAPTION
                                | WS_SYSMENU
                                | WS_THICKFRAME
                                | WS_MINIMIZEBOX
                                | WS_MAXIMIZEBOX) as isize
                    }
                };
                (self.win32.user32.SetWindowLongPtrW)(self.hwnd, GWL_STYLE, new_style);
                (self.win32.user32.SetWindowPos)(
                    self.hwnd,
                    std::ptr::null_mut(),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED,
                );

                // A frameless window loses more than its title bar. WS_POPUP
                // strips the DROP SHADOW, the rounded corners on Windows 11,
                // and the snap-layouts affordance with it — the window ends up
                // a flat rectangle that does not look like it belongs to the
                // desktop.
                //
                // DwmExtendFrameIntoClientArea pushes the DWM frame back INTO
                // the client area, which restores all three while leaving the
                // whole surface ours to draw. This is what Electron does — and
                // it is not an option Electron exposes: Chromium calls it
                // internally for every frameless window, which is why an
                // Electron app with `frame: false` still has a shadow and still
                // snaps. So azul does it here, in the backend, rather than
                // making an application ask.
                //
                // A ONE-pixel top margin, not -1: `-1` ("sheet of glass")
                // extends the frame over the entire client area and the DWM
                // then composites the whole window as frame, which shows
                // through anywhere the app draws with alpha. One pixel is
                // enough for the shadow and the corners.
                if matches!(current.flags.decorations, WindowDecorations::None) {
                    if let Some(ref dwm) = self.win32.dwmapi_funcs {
                        let margins = dlopen::MARGINS {
                            cxLeftWidth: 0,
                            cxRightWidth: 0,
                            cyTopHeight: 1,
                            cyBottomHeight: 0,
                        };
                        (dwm.DwmExtendFrameIntoClientArea)(self.hwnd, &margins);
                    }
                }
            }
        }

        // Resizable changed?
        if previous.flags.is_resizable != current.flags.is_resizable {
            use dlopen::constants::*;
            unsafe {
                let style = (self.win32.user32.GetWindowLongPtrW)(self.hwnd, GWL_STYLE);
                let new_style = if current.flags.is_resizable {
                    style | (WS_THICKFRAME | WS_MAXIMIZEBOX) as isize
                } else {
                    style & !((WS_THICKFRAME | WS_MAXIMIZEBOX) as isize)
                };
                (self.win32.user32.SetWindowLongPtrW)(self.hwnd, GWL_STYLE, new_style);
                (self.win32.user32.SetWindowPos)(
                    self.hwnd,
                    std::ptr::null_mut(),
                    0,
                    0,
                    0,
                    0,
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
                    self.hwnd,
                    insert_after,
                    0,
                    0,
                    0,
                    0,
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
        self.common.current_window_state().size.get_hidpi_factor()
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
            // sent when the regeneration request was processed in WM_PAINT.
            // If no transaction was pending, this is a no-op render.
            if let Err(e) = self.render_and_present(false) {
                log_error!(LogCategory::Rendering, "Failed to present frame: {:?}", e);
            }
        }

        // Check for close request
        if self.common.current_window_state().flags.close_requested {
            self.common
                .update_window_state(event::WindowStateSource::Os, |ws| {
                    ws.flags.close_requested = false;
                });
            // WM_CLOSE's pass already derived and delivered WindowClose; this is
            // the polling API consuming the flag afterwards, so the clear must
            // not read as a second, lost close event.
            self.discard_input_delta("windows.poll_event.close_consumed");
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
            return true;
        }

        // --- Drain the THREAD queue (hwnd filter = NULL) ---
        // The hwnd-filtered peek above cannot see two whole classes of
        // message (same hole run.rs's Win32 loop had, fixed the same way):
        //   * WM_QUIT, which `PostQuitMessage` posts to the THREAD and which
        //     is associated with no window at all — an hwnd-filtered
        //     PeekMessage/GetMessage can NEVER retrieve it, so a
        //     PostQuitMessage from user or library code was invisible to
        //     this pump;
        //   * genuine thread messages (`PostThreadMessage`, hwnd == NULL),
        //     which stayed in the queue forever and, being "available",
        //     defeat any WaitMessage a caller blocks on between polls — an
        //     idle block turns into a spin.
        // An hwnd filter of NULL retrieves messages for any window of this
        // thread PLUS thread messages, which is exactly the remainder;
        // DispatchMessageW routes window messages by msg.hwnd, so nothing is
        // misdelivered.
        let has_thread_message = unsafe {
            (self.win32.user32.PeekMessageW)(&mut msg, std::ptr::null_mut(), 0, 0, PM_REMOVE)
        };
        if has_thread_message != 0 {
            // WM_QUIT = "terminate the message loop". The bool contract here
            // cannot say "quit", so surface it the way poll-driven callers
            // observe shutdown: mark the window closed and report the event
            // as handled. (Deliberately NOT close_requested — that flag is
            // the WM_CLOSE veto-protocol's, and WM_QUIT is not veto-able.)
            const WM_QUIT: u32 = 0x0012;
            if msg.message == WM_QUIT {
                self.is_open = false;
                return true;
            }
            unsafe {
                (self.win32.user32.TranslateMessage)(&msg);
                (self.win32.user32.DispatchMessageW)(&msg);
            }
            return true;
        }

        false
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
                    if self
                        .common
                        .current_window_state()
                        .flags
                        .use_native_context_menus
                    {
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
        // PARK, do not track: TrackPopupMenu is a modal loop and `&mut self`
        // is live all the way up to window_proc.
        self.park_native_menu(hmenu, pt.x, pt.y);
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
        let parent_pos = match self.common.current_window_state().position {
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
        0x020B => "WM_XBUTTONDOWN",
        0x020C => "WM_XBUTTONUP",
        0x020E => "WM_MOUSEHWHEEL",
        0x0231 => "WM_ENTERSIZEMOVE",
        0x0232 => "WM_EXITSIZEMOVE",
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
    ///
    /// Returns `true` when it actually changed input state, i.e. when the
    /// caller owes the state-diff pipeline a pass (touch/gesture transitions
    /// are derived from the previous→current delta like everything else).
    unsafe fn feed_pointer(&mut self, hwnd: HWND, pointer_id: u32, is_up: bool) -> bool {
        use winapi::um::winuser::{
            PEN_FLAG_BARREL, PEN_FLAG_ERASER, POINTER_FLAG_INCONTACT, POINTER_PEN_INFO,
            POINTER_TOUCH_INFO, PT_PEN, PT_TOUCH,
        };
        let get_type = match self.win32.user32.GetPointerType {
            Some(f) => f,
            None => return false,
        };
        let mut ptype: u32 = 0;
        if get_type(pointer_id, &mut ptype) == 0 {
            return false;
        }
        let hf = self
            .common
            .current_window_state()
            .size
            .get_hidpi_factor()
            .inner
            .get();

        if ptype == PT_PEN {
            let get_pen = match self.win32.user32.GetPointerPenInfo {
                Some(f) => f,
                None => return false,
            };
            let mut pi: POINTER_PEN_INFO = core::mem::zeroed();
            if get_pen(pointer_id, &mut pi) == 0 {
                return false;
            }
            let mut pt = dlopen::POINT {
                x: pi.pointerInfo.ptPixelLocation.x,
                y: pi.pointerInfo.ptPixelLocation.y,
            };
            (self.win32.user32.ScreenToClient)(hwnd, &mut pt);
            let pos = azul_core::geom::LogicalPosition::new(pt.x as f32 / hf, pt.y as f32 / hf);
            let in_contact = !is_up && (pi.pointerInfo.pointerFlags & POINTER_FLAG_INCONTACT) != 0;
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
                None => return false,
            };
            let mut ti: POINTER_TOUCH_INFO = core::mem::zeroed();
            if get_touch(pointer_id, &mut ti) == 0 {
                return false;
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
            let ts = self.common.touch_state_mut();
            let mut pts: Vec<TouchPoint> = ts.touch_points.clone().into_library_owned_vec();
            let was_present = pts.iter().any(|p| p.id == pointer_id as u64);
            pts.retain(|p| p.id != pointer_id as u64);
            if !is_up {
                // POINTER_TOUCH_INFO carries the contact RECTANGLE, in
                // physical px. rcContact defaults to a zero-size rect centred
                // on the pointer when the digitizer reports no area, so a
                // degenerate rect means "not reported" rather than "a contact
                // of zero size" — and must not be turned into a 0.0 x 0.0
                // ellipse that a caller would read as real.
                let (major, minor) = {
                    let w = (ti.rcContact.right - ti.rcContact.left) as f32 / hf;
                    let h = (ti.rcContact.bottom - ti.rcContact.top) as f32 / hf;
                    if w > 0.0 && h > 0.0 {
                        (w.max(h), w.min(h))
                    } else {
                        (0.0, 0.0)
                    }
                };
                // `orientation` is DEGREES clockwise from the x-axis, 0..359 —
                // already the axis TouchPoint uses, unlike Wayland's.
                let orientation_rad = (ti.orientation as f32).to_radians();
                pts.push(TouchPoint {
                    id: pointer_id as u64,
                    position: pos,
                    force,
                    major,
                    minor,
                    orientation_rad,
                    // WM_POINTER splits pen and touch into separate message
                    // families, and this is the touch one — a stylus arrives
                    // through the PT_PEN branch and feeds PenState instead.
                    tool_type: azul_core::window::TouchToolType::Finger,
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
                let window_position = self.common.current_window_state().position;
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
        } else {
            return false;
        }

        true
    }

    /// [`Self::feed_pointer`] plus the state-diff pass it owes.
    ///
    /// Touch points live in `current_window_state.touch_state` and the gesture
    /// sessions in the layout window — both are diff-derived like every other
    /// input. A SECOND finger going down is not promoted to a WM_MOUSE
    /// message, so without a pass here that transition reached nobody: the
    /// gesture recogniser saw pinch/rotate start one event late, or not at
    /// all. The pass runs BEFORE `DefWindowProc` promotes the pointer to
    /// WM_MOUSE messages, because those arms snapshot `previous` themselves
    /// and would otherwise consume the touch delta first.
    unsafe fn feed_pointer_and_dispatch(&mut self, hwnd: HWND, pointer_id: u32, is_up: bool) {
        let prev_snapshot = self.common.current_window_state().clone();
        if !self.feed_pointer(hwnd, pointer_id, is_up) {
            return;
        }
        self.set_previous_window_state(prev_snapshot);
        let r = self.process_window_events(0);
        self.route_main_window_result(hwnd, r);
    }

    /// (Re-)arm the `WM_MOUSELEAVE` notification.
    ///
    /// `TrackMouseEvent(TME_LEAVE)` is a ONE-SHOT: it has to be re-armed after
    /// every pointer move, and again after a captured drag ends — the leave
    /// that fired mid-capture was suppressed, and Win32 posts a fresh one
    /// immediately if the pointer is already outside the client area.
    unsafe fn arm_mouse_leave(&self) {
        use self::dlopen::{TME_LEAVE, TRACKMOUSEEVENT};
        let mut tme = TRACKMOUSEEVENT {
            cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
            dwFlags: TME_LEAVE,
            hwndTrack: self.hwnd,
            dwHoverTime: 0,
        };
        (self.win32.user32.TrackMouseEvent)(&mut tme);
    }

    /// Re-read the pressed-key set from the OS.
    ///
    /// Key RELEASES go to whichever window has the focus: after Alt+Tab this
    /// window saw LAlt's `WM_KEYDOWN` but never its `WM_KEYUP`, so Alt stayed
    /// latched in `pressed_virtual_keycodes` and every later click behaved as
    /// Alt+click. `GetKeyboardState` is the only way to learn about those
    /// releases — on focus GAIN it also preserves a modifier that is genuinely
    /// still held (clicking into a window with Shift down).
    ///
    /// Mutates `current_window_state` only; the caller owns the diff snapshot
    /// and the event pass.
    unsafe fn resync_keyboard_state_from_os(&mut self) {
        use azul_core::window::{
            OptionVirtualKeyCode, ScanCodeVec, VirtualKeyCode, VirtualKeyCodeVec,
        };

        let mut keys = [0u8; 256];
        let ok = (self.win32.user32.GetKeyboardState)(keys.as_mut_ptr()) != 0;

        let mut pressed: Vec<VirtualKeyCode> = Vec::new();
        if ok {
            // High bit set = key is down. Mouse buttons (VK_LBUTTON …
            // VK_XBUTTON2) have no VirtualKeyCode and drop out in the
            // translation; the generic and side-specific modifier codes both
            // map onto the left variant, hence the dedup.
            for (vk, state) in keys.iter().enumerate() {
                if *state & 0x80 == 0 {
                    continue;
                }
                if let Some(k) = win_event::vkey_to_winit_vkey(vk as i32) {
                    if !pressed.contains(&k) {
                        pressed.push(k);
                    }
                }
            }
        }

        let ks = self.common.keyboard_state_mut();
        ks.current_virtual_keycode = OptionVirtualKeyCode::None;
        ks.pressed_virtual_keycodes = VirtualKeyCodeVec::from_vec(pressed);
        // Scancodes are physical-key ids and cannot be recovered from a
        // virtual-key snapshot; dropping the set is the honest reading (the
        // next WM_KEYDOWN refills it) — keeping it would keep exactly the
        // stale entries this resync exists to remove.
        ks.pressed_scancodes = ScanCodeVec::from_vec(Vec::new());
    }
}

thread_local! {
    /// Set while [`pump_modal_loop_work`] is running, so a nested message loop
    /// started from inside it cannot re-enter and alias a live borrow.
    static MODAL_PUMP_ACTIVE: std::cell::Cell<bool> = std::cell::Cell::new(false);
}

/// One iteration of the outer event loop's per-window work, for use INSIDE a
/// modal size/move loop.
///
/// `WM_ENTERSIZEMOVE` … `WM_EXITSIZEMOVE` brackets a loop USER32 runs itself:
/// `DispatchMessageW` does not return until the user releases the mouse, so for
/// the whole of a drag-resize or a drag-move the body of `run.rs`'s
/// `'event_loop` — pending window creates, the accessibility drain, the
/// cross-window regeneration sweep and the deferred free of closed windows —
/// simply does not run. A window opened from a callback (a tooltip, a menu, a
/// progress dialog) stayed queued, a screen reader's queued actions stayed
/// unhandled, and a SECOND window whose DOM a resize callback invalidated kept
/// rendering the stale one until the user let go. Messages ARE pumped inside
/// that loop, so a `WM_TIMER` gets us back in; this is what it runs.
///
/// The dragged window itself is unaffected by the missing render sweep — it
/// keeps getting `WM_SIZE` + `WM_PAINT` — which is exactly why the stall was
/// invisible with one window open.
///
/// Every window is re-borrowed from the registry for the shortest possible
/// span and no `&mut Win32Window` is live across a call that can create or free
/// one, which is the same rule the outer loop follows for the same reason.
fn pump_modal_loop_work() {
    // RE-ENTRANCY GUARD. The sweep below holds a `&mut Win32Window` across
    // `regenerate_layout()`, which runs user callbacks — and anything those do
    // that spins yet another nested message loop (a modal dialog, a tracked
    // menu) delivers this same `WM_TIMER` again and would hand a SECOND `&mut`
    // to the very window we are holding. One pump at a time.
    if MODAL_PUMP_ACTIVE.with(|active| active.get()) {
        return;
    }
    MODAL_PUMP_ACTIVE.with(|active| active.set(true));

    // Windows destroyed during this modal loop parked their boxes; this is the
    // only place they are reclaimed while USER32 owns the loop.
    registry::drain_closed_windows();

    for hwnd in registry::get_all_window_handles() {
        // Pending window creates. Re-fetched every iteration: creating a window
        // runs a whole `Win32Window::new`, and the registry may have moved on.
        loop {
            let Some(wptr) = registry::get_window(hwnd) else {
                break;
            };
            let next = unsafe {
                let window = &mut *wptr;
                match window.pending_window_creates.pop() {
                    Some(options) => Some((
                        options,
                        window.app_config.clone(),
                        window.common.fc_cache.clone(),
                        window.font_registry.clone(),
                        window.common.app_data.clone(),
                        window.common.undo_manager.clone(),
                    )),
                    None => None,
                }
            };
            let Some((options, config, fc_cache, font_registry, app_data, undo_manager)) = next
            else {
                break;
            };

            match Win32Window::new(
                options,
                config,
                fc_cache,
                font_registry,
                app_data,
                undo_manager,
                None,
            ) {
                Ok(new_window) => unsafe {
                    let new_window_ptr = Box::into_raw(Box::new(new_window));
                    let new_hwnd = (*new_window_ptr).hwnd;
                    ((*new_window_ptr).win32.user32.SetWindowLongPtrW)(
                        new_hwnd,
                        dlopen::constants::GWLP_USERDATA,
                        new_window_ptr as isize,
                    );
                    registry::register_window(new_hwnd, new_window_ptr);
                    (*new_window_ptr).register_drag_drop();
                    (*new_window_ptr).finish_frameless_frame();
                },
                Err(e) => {
                    log_error!(
                        LogCategory::Window,
                        "[Windows] Failed to create window during a modal size/move loop: {:?}",
                        e
                    );
                }
            }
        }

        // Accessibility actions queued by the UI Automation adapter.
        #[cfg(feature = "a11y")]
        {
            if let Some(wptr) = registry::get_window(hwnd) {
                unsafe {
                    (*wptr).process_accessibility_actions();
                }
            }
        }

        // Cross-window regeneration sweep.
        if let Some(wptr) = registry::get_window(hwnd) {
            unsafe {
                let window = &mut *wptr;
                if window.common.regeneration_pending() {
                    // Captured BEFORE the regeneration: a lifecycle callback
                    // inside it can raise a new request, and only what we saw
                    // here may be retired.
                    let regen_epoch_seen = window.common.regen_epoch();
                    if let Err(e) = window.regenerate_layout() {
                        log_error!(
                            LogCategory::Layout,
                            "[Windows] Layout regeneration error during a modal size/move loop: {}",
                            e
                        );
                    }
                    window
                        .common
                        .clear_regeneration_unless_reraised(regen_epoch_seen);
                    (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
                }
            }
        }
    }

    // No early return above, and a panic out of a wndproc aborts, so this is
    // the only exit.
    MODAL_PUMP_ACTIVE.with(|active| active.set(false));
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: u32,
    wparam: dlopen::WPARAM,
    lparam: dlopen::LPARAM,
) -> dlopen::LRESULT {
    // Message constants
    const WM_NCCREATE: u32 = 0x0081;
    const WM_NCDESTROY: u32 = 0x0082;
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
    const WM_DEVICECHANGE: u32 = 0x0219;
    const WM_GESTURE: u32 = 0x0119;
    const WM_APPCOMMAND: u32 = 0x0319;
    const WM_XBUTTONDOWN: u32 = 0x020B;
    const WM_XBUTTONUP: u32 = 0x020C;
    const WM_ENTERSIZEMOVE: u32 = 0x0231;
    const WM_EXITSIZEMOVE: u32 = 0x0232;
    const WM_NCCALCSIZE: u32 = 0x0083;
    const WM_NCHITTEST: u32 = 0x0084;
    const WM_MOUSEWHEEL: u32 = 0x020A;
    const WM_APP_FRAME_READY_LOCAL: u32 = WM_APP_FRAME_READY;
    const WM_APP_SHOW_PENDING_MENU_LOCAL: u32 = WM_APP_SHOW_PENDING_MENU;
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
            // Fallback teardown for destroy paths that never went through
            // WM_CLOSE (a destroyed parent, an external DestroyWindow): the
            // HWND and the GL context are still alive here, and
            // deinit_renderer() takes the renderer, so the WM_CLOSE path
            // having run first makes this a no-op.
            window.release_gpu_resources();
            // Window destroyed - unregister from global registry
            window.is_open = false;
            // The registry holds the only owning pointer (run.rs boxed the
            // window and handed it over). Dropping it HERE is a
            // use-after-free — WM_DESTROY is dispatched from inside
            // DestroyWindow, i.e. inside this very window procedure, and the
            // event loop holds a `&mut Win32Window` across the dispatch. Park
            // it instead; the loop reclaims it via
            // registry::drain_closed_windows() at a safe point. Discarding the
            // pointer (what this used to do) leaked the whole window — GL
            // context, WebRender renderer and layout window — per closed
            // window.
            if let Some(freed) = registry::unregister_window(hwnd) {
                registry::queue_window_free(freed);
            }
            log_debug!(
                LogCategory::Window,
                "[Win32] Window unregistered, remaining windows: {}",
                registry::window_count()
            );
            0
        }

        WM_NCDESTROY => {
            // Belt-and-braces half of the deferred-free fix: WM_NCDESTROY is
            // the LAST message a window receives (after WM_DESTROY parked the
            // box for registry::drain_closed_windows). Null GWLP_USERDATA here
            // so any straggler SendMessage that still reaches this HWND takes
            // window_proc's null early-out instead of dereferencing a pointer
            // whose box the event loop may since have reclaimed. The parked
            // box itself is still alive at THIS point (the drain only runs
            // from the loop, never from inside a dispatch), so touching
            // `window` above this line was sound — but nothing after this arm
            // may use it again.
            (window.win32.user32.SetWindowLongPtrW)(hwnd, GWLP_USERDATA, 0);
            def_window_proc_w(hwnd, msg, wparam, lparam)
        }

        WM_CLOSE => {
            log_debug!(LogCategory::Window, "[Win32] WM_CLOSE - Close requested");
            // User clicked close button - set close_requested flag
            // and process callbacks to allow cancellation. Snapshot first so the
            // false -> true transition is what the pass diffs, rather than
            // relying on the last completed pass having left the two equal.
            // A close callback can cancel the close AND restyle (e.g. show a styled
            // "unsaved changes" prompt) — route the result so any restyle takes the
            // incremental fast path / repaints, same as every other input handler.
            // If the close proceeds below, the InvalidateRect is harmless.
            let outcome = window.request_window_close("windows.wm_close");
            window.route_main_window_result(hwnd, outcome.result);

            // Check if callback cancelled the close
            if outcome.confirmed {
                // Not cancelled - proceed with close
                window.is_open = false;
                // Release the GPU side while the HWND and the GL context are
                // still alive — WebRender's Renderer must be deinit()'d, not
                // dropped (texture deletion has to happen inside a frame).
                // Only Win32Window::close() used to do this, and nothing calls
                // it on the user-clicked-X path, so closing a window through
                // its title-bar button leaked the renderer.
                window.release_gpu_resources();
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
            // Retire the update region FIRST. "The DefWindowProc function
            // validates the update region" (MSDN WM_PAINT) — and it validates
            // it AS IT STANDS at call time. This call used to sit at the END
            // of this arm, AFTER the render: any InvalidateRect raised DURING
            // the render (scrollbar-fade next-frame request, the deferred
            // first-frame request_redraw when layout wasn't ready, a
            // callback-driven repaint) was part of the update region by then
            // and was validated away one line later — the requested WM_PAINT
            // never arrived, freezing the fade mid-animation and leaving a
            // not-ready first frame with no repaint scheduled (window could
            // stay hidden until the next input event). Validating first means
            // invalidations raised by the render below stay pending and
            // generate the next WM_PAINT.
            let def_result = (window.win32.user32.DefWindowProcW)(hwnd, msg, wparam, lparam);

            // Determine if layout needs regeneration (DOM changed).
            // Captured BEFORE either branch renders: a callback inside the render
            // can raise a new regeneration request, and only what we saw here may
            // be retired.
            // RESIZE FAST PATH (coalesced): any number of WM_SIZE events since
            // the last paint become ONE incremental relayout of the EXISTING
            // StyledDom at the latest size. A concurrent full regeneration
            // request supersedes it (it lays out at the new size anyway).
            if window.common.take_resize_relayout() && !window.common.regeneration_pending() {
                let mut resize_relayout_failed = false;
                let mut debug_messages = None;
                if let Err(e) = window.incremental_relayout_dispatching(
                    crate::desktop::shell2::common::event::IncrementalRelayout::Resize,
                    &mut debug_messages,
                ) {
                    log_error!(
                        LogCategory::Layout,
                        "[Win32] resize fast-path relayout failed: {e} — falling back to a \
                         full regeneration"
                    );
                    resize_relayout_failed = true;
                }
                if resize_relayout_failed {
                    window
                        .common
                        .request_regeneration(azul_core::callbacks::RelayoutReason::Resize);
                } else {
                    window.common.request_relayout_only();
                }
            }

            let regen_epoch_seen = window.common.regen_epoch();
            let layout_was_regenerated = if window.common.take_relayout_only() {
                // Restyle / runtime edit: incremental_relayout() already re-ran layout
                // on the existing StyledDom in the ShouldIncrementalRelayout event arm.
                // Skip the full regenerate_layout() (no layout_callback / StyledDom
                // rebuild), but still build + send the WebRender display-list
                // transaction (GPU) / rebuild the CPU hit-tester so the restyle reaches
                // the screen — render_and_present(true) then presents the new scene.
                window.send_frame_after_incremental_relayout();
                // Retire ONLY the request this frame observed: a lifecycle callback
                // running inside the render above can raise a new one, and a bare
                // `= false` here would erase it.
                window
                    .common
                    .clear_regeneration_unless_reraised(regen_epoch_seen);
                true
            } else if window.common.regeneration_pending() {
                if let Err(e) = window.regenerate_layout() {
                    log_error!(LogCategory::Layout, "Layout regeneration error: {:?}", e);
                }
                // Retire ONLY the request this frame observed: a lifecycle callback
                // running inside the render above can raise a new one, and a bare
                // `= false` here would erase it.
                window
                    .common
                    .clear_regeneration_unless_reraised(regen_epoch_seen);
                true
            } else {
                false
            };

            // The caret may have moved on ANY of those paths — including the
            // relayout-only fast path, which skips `regenerate_layout_inner`'s
            // tail entirely. Without this an over-the-spot IME kept drawing its
            // composition and candidate windows wherever the caret was at the
            // last FULL regeneration. Gated internally on the caret/focus
            // identity actually having changed.
            window.sync_ime_state();

            match window.render_and_present(layout_was_regenerated) {
                Ok(_) => {}
                Err(e) => {
                    log_error!(LogCategory::Rendering, "Render error: {:?}", e);
                }
            }
            // Update region already validated by the DefWindowProc call ABOVE
            // (before the render) — calling it again here would swallow any
            // repaint the render just requested.
            def_result
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
                let prev_snapshot = window.common.current_window_state().clone();
                window.common.update_window_state(
                    crate::desktop::shell2::common::event::WindowStateSource::Os,
                    |ws| ws.flags.frame = WindowFrame::Minimized,
                );
                window.set_previous_window_state(prev_snapshot);
                let r = window.process_window_events(0);
                window.route_main_window_result(hwnd, r);
                return 0;
            }

            if width > 0 && height > 0 {
                use azul_core::{geom::PhysicalSizeU32, window::WindowSize};

                // Resize census, matching the Wayland/X11 configure traces. A
                // drag delivers one WM_SIZE per frame, so per-event work runs at
                // frame rate — count them and time what they cause.
                crate::log_debug!(
                    LogCategory::Window,
                    "[Win32 WM_SIZE] #{} phys {}x{} wparam={}",
                    WM_SIZE_SEEN.fetch_add(1, core::sync::atomic::Ordering::Relaxed) + 1,
                    width,
                    height,
                    wparam as usize
                );

                let physical_size = PhysicalSizeU32::new(width, height);
                let dpi = window.common.current_window_state().size.dpi;
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
                if old_context.viewport_breakpoint_changed(
                    &window.dynamic_selector_context,
                    crate::desktop::shell2::common::CSS_BREAKPOINTS,
                ) {
                    log_debug!(
                        LogCategory::Layout,
                        "[WM_SIZE] Breakpoint crossed: {}x{} -> {}x{}",
                        old_context.viewport_width,
                        old_context.viewport_height,
                        window.dynamic_selector_context.viewport_width,
                        window.dynamic_selector_context.viewport_height
                    );
                }

                // The EVENT-DIFF baseline for the pass at the end of this arm:
                // the state as it was BEFORE the resize was applied.
                let prev_snapshot = window.common.current_window_state().clone();

                // Determine window frame state
                use azul_core::window::WindowFrame;
                let frame = match wparam as u32 {
                    0x0002 => WindowFrame::Maximized, // SIZE_MAXIMIZED
                    0x0001 => WindowFrame::Minimized, // SIZE_MINIMIZED
                    _ => WindowFrame::Normal,         // SIZE_RESTORED
                };

                // WM_SIZE is an OS-reported geometry/frame change (already
                // applied by the OS), so it is acknowledged into the OS-sync
                // baseline — otherwise sync_window_state() pushes it straight
                // back out via SetWindowPos/ShowWindow, the OS→app→OS loop.
                // (Source = Os, not App.)
                window.common.update_window_state(
                    crate::desktop::shell2::common::event::WindowStateSource::Os,
                    |ws| {
                        ws.size.dimensions = logical_size;
                        ws.flags.frame = frame;
                    },
                );

                // Update WebRender document view
                use webrender::{
                    api::units::{DeviceIntRect, DeviceIntSize, DevicePixelScale},
                    Transaction as WrTransaction,
                };

                use crate::desktop::wr_translate2::wr_translate_document_id;

                let hidpi_factor = window.common.current_window_state().size.get_hidpi_factor();

                // Update WebRender document view (GPU mode only — CPU mode has no render_api)
                if let (Some(render_api), Some(document_id)) =
                    (window.common.render_api.as_mut(), window.common.document_id)
                {
                    let mut txn = WrTransaction::new();
                    // NOTE: azul_layout outputs coordinates in CSS pixels (logical pixels).
                    txn.set_document_view(
                        DeviceIntRect::from_size(DeviceIntSize::new(width as i32, height as i32)),
                        DevicePixelScale::new(hidpi_factor.inner.get()),
                    );
                    render_api.send_transaction(wr_translate_document_id(document_id), txn);
                }

                // RESIZE POLICY (user ruling 2026-08-08, same as Wayland/X11):
                // a drag delivers one WM_SIZE per frame; the app's layout() is
                // only re-invoked when a recorded window-size query answer flips
                // or a CSS breakpoint / orientation is crossed. Everything else
                // re-flows the existing StyledDom — one coalesced relayout per
                // WM_PAINT, at the latest size.
                let old_logical = azul_core::geom::LogicalSize::new(
                    old_context.viewport_width,
                    old_context.viewport_height,
                );
                let full = window
                    .common
                    .request_regeneration_for_resize(old_logical, logical_size);
                if full {
                    log_debug!(
                        LogCategory::Layout,
                        "[WM_SIZE] boundary crossed — full regeneration at {}x{}",
                        logical_size.width,
                        logical_size.height
                    );
                }

                // The relayout is already scheduled above; this pass exists so
                // the app's WindowResize callbacks actually run. `WindowResize`
                // is DERIVED from the previous→current delta, so an arm that
                // applies the new geometry to both sides and never runs a pass
                // (what this used to do) cannot fire it at all — no resize
                // event has ever reached a Windows app. Mirrors the Wayland
                // xdg_toplevel.configure handler.
                //
                // The MAXIMIZE/RESTORE half of `wparam` fires NO callback: there
                // is no frame `EventType` (no Maximize, no Restore, no
                // Minimize, no Fullscreen) and `first_differing_state_field`
                // deliberately excludes `flags.frame` — including it would make
                // every maximize trip the unconsumed-delta guard. Writing it is
                // still required: it is what the app reads back from the window
                // state and what the CSD titlebar widget draws its
                // maximize/restore button from, and advancing `os_synced_state`
                // in lockstep (source = Os above) is what stops
                // `sync_window_state` from re-issuing `ShowWindow` off a stale
                // diff. The `size.dimensions` half of this same delta IS
                // event-bearing and is the reason for the pass.
                window.set_previous_window_state(prev_snapshot);
                let r = window.process_window_events(0);
                window.route_main_window_result(hwnd, r);

                // Request redraw (WM_PAINT consumes whichever path was chosen)
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
            // MWA-C-csd: WM_MOVE's lparam is the CLIENT-area origin, but
            // sync_window_state positions via SetWindowPos, which takes the
            // FRAME origin — storing the client origin made the round-trip
            // drift by the frame border on decorated windows (coincided
            // only for WS_POPUP). Read the frame rect instead; fall back to
            // the client origin if the call fails.
            let (x, y) = {
                let mut rect = dlopen::RECT {
                    left: 0,
                    top: 0,
                    right: 0,
                    bottom: 0,
                };
                if unsafe { (window.win32.user32.GetWindowRect)(hwnd, &mut rect) } != 0 {
                    (rect.left, rect.top)
                } else {
                    (
                        (lparam & 0xFFFF) as i16 as i32,
                        ((lparam >> 16) & 0xFFFF) as i16 as i32,
                    )
                }
            };
            let pos = azul_core::window::WindowPosition::Initialized(
                azul_core::geom::PhysicalPositionI32::new(x, y),
            );
            // The EVENT-DIFF baseline for the pass at the end of this arm: the
            // state as it was BEFORE the move was applied.
            let prev_snapshot = window.common.current_window_state().clone();
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
                use dlopen::{MONITORINFOEXW, MONITOR_DEFAULTTONEAREST};
                let hmonitor = unsafe {
                    (window.win32.user32.MonitorFromWindow)(hwnd, MONITOR_DEFAULTTONEAREST)
                };
                if !hmonitor.is_null() {
                    let mut mi = MONITORINFOEXW {
                        cbSize: core::mem::size_of::<MONITORINFOEXW>() as u32,
                        rcMonitor: dlopen::RECT::default(),
                        rcWork: dlopen::RECT::default(),
                        dwFlags: 0,
                        szDevice: [0u16; 32],
                    };
                    if unsafe { (window.win32.user32.GetMonitorInfoW)(hmonitor, &mut mi) } != 0 {
                        // Find matching monitor in cache by position
                        let found = window.common.layout_window.as_ref().and_then(|lw| {
                            let guard = lw.monitors.lock().ok()?;
                            guard
                                .as_ref()
                                .iter()
                                .find(|m| {
                                    m.position.x == mi.rcMonitor.left as isize
                                        && m.position.y == mi.rcMonitor.top as isize
                                })
                                .map(|m| m.monitor_id.index as u32)
                        });
                        if let Some(index) = found {
                            // Also OS-reported, and it goes through the same
                            // helper as the position above — writing
                            // current_window_state directly left the two
                            // halves of one OS geometry report on different
                            // state authorities.
                            window.common.update_window_state(
                                crate::desktop::shell2::common::event::WindowStateSource::Os,
                                |ws| {
                                    ws.monitor_id = azul_css::corety::OptionU32::Some(index);
                                },
                            );
                        }
                    }
                }
            }

            let monitor_id = window.common.current_window_state().monitor_id;
            if let Some(ref mut lw) = window.common.layout_window {
                lw.current_window_state.position = pos;
                lw.current_window_state.monitor_id = monitor_id;
            }

            // Dispatch WindowMove / WindowMonitorChanged NOW rather than
            // leaving a live delta for whatever pass happens to run next
            // (which, before this, was also what tripped
            // check_input_delta_consumed's "position" arm at the next
            // WM_TIMER under AZ_VALIDATE / debug). The old feedback-loop
            // worry — a move callback writing `position` back — died with
            // the baseline split: an OS-equal write leaves a zero
            // current-vs-os_synced diff, so sync_window_state() has nothing
            // to echo. Same snapshot/ack/restore/pass shape as WM_SIZE.
            window.set_previous_window_state(prev_snapshot);
            let r = window.process_window_events(0);
            window.route_main_window_result(hwnd, r);

            0
        }

        WM_MOUSEMOVE => {
            // Mouse moved - similar to macOS handle_mouse_move
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;

            use azul_core::{geom::LogicalPosition, window::CursorPosition};

            let hidpi_factor = window.common.current_window_state().size.get_hidpi_factor();
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
            window.snapshot_window_state_baseline("windows.wm_mousemove");

            // Update mouse state
            window.common.mouse_state_mut().cursor_position = CursorPosition::InWindow(logical_pos);

            // Record input sample for gesture detection (movement during button press)
            let button_state = if window.common.current_window_state().mouse_state.left_down {
                BUTTON_STATE_LEFT
            } else {
                BUTTON_STATE_NONE
            };

            // Use GetCursorPos for accurate screen-absolute position (physical pixels → logical)
            let screen_pos = {
                let mut pt = dlopen::POINT { x: 0, y: 0 };
                unsafe {
                    (window.win32.user32.GetCursorPos)(&mut pt);
                }
                let hf = hidpi_factor.inner.get();
                azul_core::geom::LogicalPosition::new(pt.x as f32 / hf, pt.y as f32 / hf)
            };
            window.record_input_sample(logical_pos, button_state, false, false, Some(screen_pos));

            // CPU mode (no WR hit_tester/document_id): resolve the hit test via
            // the shared perform_hit_test → cpu_hit_tester path. Without this,
            // events dispatched against a stale/empty hover state — hover CSS,
            // clicks, wheel targeting and MouseEnter/Leave were all dead in the
            // Windows CPU fallback (the GPU-gated block below has no CPU arm).
            // One hit tester for every render mode. This used to run only as a
            // CPU fallback, with a parallel WebRender path below for GPU mode;
            // the two disagreed on coordinate space, so which node a click
            // resolved to depended on the renderer.
            PlatformWindow::update_hit_test_at(&mut *window, logical_pos);
            let hit_test = window.common.perform_hit_test(logical_pos);

            // Update hit test (GPU mode only — CPU mode handled above)
            if let Some(ref mut layout_window) = window.common.layout_window {
                {

                    layout_window
                        .hover_manager
                        .push_hit_test(InputPointId::Mouse, hit_test.clone());

                    // Update cursor based on CSS cursor properties
                    // This is done BEFORE callbacks so callbacks can override the cursor
                    let cursor_type_hit_test =
                        layout_window.compute_cursor_type_hit_test(&hit_test);
                    let new_cursor_type = cursor_type_hit_test.cursor_icon;
                    let new = OptionMouseCursorType::Some(new_cursor_type);

                    // Update cursor type if changed
                    if window
                        .common
                        .current_window_state()
                        .mouse_state
                        .mouse_cursor_type
                        != new
                    {
                        window.common.mouse_state_mut().mouse_cursor_type = new;
                        window.set_cursor(new_cursor_type);
                    }
                }
            }

            // V2 system will detect MouseOver/MouseEnter/MouseLeave/Drag from state diff
            let result = window.process_window_events(0);

            // Request WM_MOUSELEAVE notification
            window.arm_mouse_leave();

            // Request redraw if needed
            window.route_main_window_result(hwnd, result);

            0
        }

        WM_MOUSELEAVE => {
            // Mouse left the window area.
            //
            // NOT while a capture is held: TME_LEAVE is re-armed on every
            // WM_MOUSEMOVE and fires as soon as the pointer crosses the client
            // edge, but a captured drag (text selection, scrollbar thumb) is
            // still receiving those moves and is still logically inside the
            // window. Handling the leave there pushed an empty hit test and
            // OutOfWindow, zeroing the hover chain mid-drag. WM_LBUTTONUP
            // re-arms TME_LEAVE right after ReleaseCapture, and Win32 then
            // posts WM_MOUSELEAVE immediately if the pointer really is
            // outside — so the leave is deferred to the end of the drag, not
            // lost.
            if (window.win32.user32.GetCapture)() == hwnd {
                return 0;
            }

            // Save previous state
            window.snapshot_window_state_baseline("windows.wm_mouseleave");

            // Get last known position, or default
            let last_pos = match window
                .common
                .current_window_state()
                .mouse_state
                .cursor_position
            {
                CursorPosition::InWindow(pos) => pos,
                CursorPosition::OutOfWindow(pos) => pos,
                CursorPosition::Uninitialized => LogicalPosition::new(0.0, 0.0),
            };

            // Clear mouse position (mouse is outside window)
            use azul_core::{geom::LogicalPosition, window::CursorPosition};
            window.common.mouse_state_mut().cursor_position = CursorPosition::OutOfWindow(last_pos);

            // MWA-C-hover: clear the hover manager (macOS/X11/Wayland all
            // push an empty hit test on leave) — without it the hover-chain
            // diff saw no change, so per-node MouseLeave never fired and
            // stale :hover styling persisted while the pointer was
            // off-window.
            if let Some(ref mut layout_window) = window.common.layout_window {
                layout_window.hover_manager.push_hit_test(
                    InputPointId::Mouse,
                    azul_core::hit_test::FullHitTest::empty(None),
                );
            }

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
            window.feed_pointer_and_dispatch(hwnd, pointer_id, false);
            def_window_proc_w(hwnd, msg, wparam, lparam)
        }
        WM_POINTERUP => {
            let pointer_id = (wparam & 0xFFFF) as u32;
            window.feed_pointer_and_dispatch(hwnd, pointer_id, true);
            def_window_proc_w(hwnd, msg, wparam, lparam)
        }

        WM_LBUTTONDOWN => {
            // Left mouse button down - similar to macOS handle_mouse_down
            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;

            use azul_core::{geom::LogicalPosition, window::CursorPosition};

            let hidpi_factor = window.common.current_window_state().size.get_hidpi_factor();
            let logical_pos = LogicalPosition::new(
                x as f32 / hidpi_factor.inner.get(),
                y as f32 / hidpi_factor.inner.get(),
            );

            // Check for scrollbar hit FIRST (before state changes)
            // MWA-B11: CSD resize edges — a left press in the border band
            // of a frameless window hands the sizing loop to the OS via
            // WM_NCLBUTTONDOWN with the matching hit-test code.
            if window.common.current_window_state().flags.decorations
                == azul_core::window::WindowDecorations::None
            {
                use crate::desktop::shell2::common::event::{
                    csd_resize_edge_at, CsdResizeEdge, CSD_RESIZE_BAND_PX,
                };
                let size = window.common.current_window_state().size.dimensions;
                if let Some(edge) = csd_resize_edge_at(logical_pos, size, CSD_RESIZE_BAND_PX) {
                    const WM_NCLBUTTONDOWN: u32 = 0x00A1;
                    let ht: usize = match edge {
                        CsdResizeEdge::Left => 10,        // HTLEFT
                        CsdResizeEdge::Right => 11,       // HTRIGHT
                        CsdResizeEdge::Top => 12,         // HTTOP
                        CsdResizeEdge::TopLeft => 13,     // HTTOPLEFT
                        CsdResizeEdge::TopRight => 14,    // HTTOPRIGHT
                        CsdResizeEdge::Bottom => 15,      // HTBOTTOM
                        CsdResizeEdge::BottomLeft => 16,  // HTBOTTOMLEFT
                        CsdResizeEdge::BottomRight => 17, // HTBOTTOMRIGHT
                    };
                    unsafe {
                        (window.win32.user32.ReleaseCapture)();
                        // lParam of WM_NCLBUTTONDOWN is the cursor in SCREEN
                        // coordinates; WM_LBUTTONDOWN's is client-relative.
                        let mut pt = dlopen::POINT { x: 0, y: 0 };
                        (window.win32.user32.GetCursorPos)(&mut pt);
                        let screen_lparam = (((pt.y as u16 as usize) << 16)
                            | (pt.x as u16 as usize))
                            as dlopen::LPARAM;
                        (window.win32.user32.SendMessageW)(
                            hwnd,
                            WM_NCLBUTTONDOWN,
                            ht as dlopen::WPARAM,
                            screen_lparam,
                        );
                    }
                    return 0;
                }
            }

            if let Some(scrollbar_hit_id) =
                PlatformWindow::perform_scrollbar_hit_test(&*window, logical_pos)
            {
                // The scrollbar consumes the press, but the button is still
                // PHYSICALLY DOWN: the shared helper records `left_down` and
                // the cursor position before swallowing the delta, so the live
                // pointer state agrees with the hardware for the whole drag.
                let r = PlatformWindow::handle_scrollbar_press(
                    &mut *window,
                    scrollbar_hit_id,
                    logical_pos,
                    azul_core::events::MouseButton::Left,
                    "windows.wm_lbuttondown.scrollbar_click",
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
            window.snapshot_window_state_baseline("windows.wm_lbuttondown");

            // Update mouse state
            window.common.mouse_state_mut().cursor_position = CursorPosition::InWindow(logical_pos);
            window.common.mouse_state_mut().left_down = true;

            // Record input sample for gesture detection (button down starts new session)
            // Use GetCursorPos for accurate screen-absolute position (physical pixels → logical)
            let screen_pos = {
                let mut pt = dlopen::POINT { x: 0, y: 0 };
                unsafe {
                    (window.win32.user32.GetCursorPos)(&mut pt);
                }
                let hf = hidpi_factor.inner.get();
                azul_core::geom::LogicalPosition::new(pt.x as f32 / hf, pt.y as f32 / hf)
            };
            window.record_input_sample(
                logical_pos,
                BUTTON_STATE_LEFT,
                true,
                false,
                Some(screen_pos),
            );

            // CPU mode (no WR hit_tester/document_id): resolve the hit test via
            // the shared perform_hit_test → cpu_hit_tester path. Without this,
            // events dispatched against a stale/empty hover state — hover CSS,
            // clicks, wheel targeting and MouseEnter/Leave were all dead in the
            // Windows CPU fallback (the GPU-gated block below has no CPU arm).
            // One hit tester for every render mode. This used to run only as a
            // CPU fallback, with a parallel WebRender path below for GPU mode;
            // the two disagreed on coordinate space, so which node a click
            // resolved to depended on the renderer.
            PlatformWindow::update_hit_test_at(&mut *window, logical_pos);
            let hit_test = window.common.perform_hit_test(logical_pos);

            // Update hit test (GPU mode only — CPU mode handled above)
            if let Some(ref mut layout_window) = window.common.layout_window {
                {

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

            let hidpi_factor = window.common.current_window_state().size.get_hidpi_factor();
            let logical_pos = LogicalPosition::new(
                x as f32 / hidpi_factor.inner.get(),
                y as f32 / hidpi_factor.inner.get(),
            );

            // End scrollbar drag if active (before state changes). The shared
            // helper also CLEARS the button the press latched — a release that
            // skipped that write left `left_down == true` for good after the
            // first thumb drag.
            let scrollbar_release = PlatformWindow::end_scrollbar_drag(
                &mut *window,
                logical_pos,
                azul_core::events::MouseButton::Left,
                "windows.wm_lbuttonup.scrollbar_drag",
            );
            if scrollbar_release.is_some() {
                unsafe {
                    (window.win32.user32.ReleaseCapture)();
                }
                // Re-arm the leave notification the capture suppressed: if the
                // thumb drag ended outside the client area, Win32 posts
                // WM_MOUSELEAVE right away and the hover state is cleaned up.
                window.arm_mouse_leave();
                return 0;
            }

            // Save previous state BEFORE making changes
            window.snapshot_window_state_baseline("windows.wm_lbuttonup");

            // Update mouse state
            window.common.mouse_state_mut().cursor_position = CursorPosition::InWindow(logical_pos);
            window.common.mouse_state_mut().left_down = false;

            // Record input sample for gesture detection (button up ends session)
            // Use GetCursorPos for accurate screen-absolute position (physical pixels → logical)
            let screen_pos = {
                let mut pt = dlopen::POINT { x: 0, y: 0 };
                unsafe {
                    (window.win32.user32.GetCursorPos)(&mut pt);
                }
                let hf = hidpi_factor.inner.get();
                azul_core::geom::LogicalPosition::new(pt.x as f32 / hf, pt.y as f32 / hf)
            };
            window.record_input_sample(
                logical_pos,
                BUTTON_STATE_NONE,
                false,
                true,
                Some(screen_pos),
            );

            // CPU mode (no WR hit_tester/document_id): resolve the hit test via
            // the shared perform_hit_test → cpu_hit_tester path. Without this,
            // events dispatched against a stale/empty hover state — hover CSS,
            // clicks, wheel targeting and MouseEnter/Leave were all dead in the
            // Windows CPU fallback (the GPU-gated block below has no CPU arm).
            // One hit tester for every render mode. This used to run only as a
            // CPU fallback, with a parallel WebRender path below for GPU mode;
            // the two disagreed on coordinate space, so which node a click
            // resolved to depended on the renderer.
            PlatformWindow::update_hit_test_at(&mut *window, logical_pos);
            let hit_test = window.common.perform_hit_test(logical_pos);

            // Update hit test (GPU mode only — CPU mode handled above)
            if let Some(ref mut layout_window) = window.common.layout_window {
                {

                    layout_window
                        .hover_manager
                        .push_hit_test(InputPointId::Mouse, hit_test);
                }
            }

            // Release mouse capture
            (window.win32.user32.ReleaseCapture)();
            // Re-arm the leave notification the capture suppressed: a
            // selection drag that ended outside the client area gets its
            // WM_MOUSELEAVE now instead of never.
            window.arm_mouse_leave();

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

            let hidpi_factor = window.common.current_window_state().size.get_hidpi_factor();
            let logical_pos = LogicalPosition::new(
                x as f32 / hidpi_factor.inner.get(),
                y as f32 / hidpi_factor.inner.get(),
            );

            // Save previous state BEFORE making changes
            window.snapshot_window_state_baseline("windows.wm_rbuttondown");

            // Update mouse state
            window.common.mouse_state_mut().cursor_position = CursorPosition::InWindow(logical_pos);
            window.common.mouse_state_mut().right_down = true;

            // CPU mode (no WR hit_tester/document_id): resolve the hit test via
            // the shared perform_hit_test → cpu_hit_tester path. Without this,
            // events dispatched against a stale/empty hover state — hover CSS,
            // clicks, wheel targeting and MouseEnter/Leave were all dead in the
            // Windows CPU fallback (the GPU-gated block below has no CPU arm).
            // One hit tester for every render mode. This used to run only as a
            // CPU fallback, with a parallel WebRender path below for GPU mode;
            // the two disagreed on coordinate space, so which node a click
            // resolved to depended on the renderer.
            PlatformWindow::update_hit_test_at(&mut *window, logical_pos);
            let hit_test = window.common.perform_hit_test(logical_pos);

            // Update hit test (GPU mode only — CPU mode handled above)
            if let Some(ref mut layout_window) = window.common.layout_window {
                {

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

            let hidpi_factor = window.common.current_window_state().size.get_hidpi_factor();
            let logical_pos = LogicalPosition::new(
                x as f32 / hidpi_factor.inner.get(),
                y as f32 / hidpi_factor.inner.get(),
            );

            // Save previous state BEFORE making changes
            window.snapshot_window_state_baseline("windows.wm_rbuttonup");

            // Update mouse state
            window.common.mouse_state_mut().cursor_position = CursorPosition::InWindow(logical_pos);
            window.common.mouse_state_mut().right_down = false;

            // CPU mode (no WR hit_tester/document_id): resolve the hit test via
            // the shared perform_hit_test → cpu_hit_tester path. Without this,
            // events dispatched against a stale/empty hover state — hover CSS,
            // clicks, wheel targeting and MouseEnter/Leave were all dead in the
            // Windows CPU fallback (the GPU-gated block below has no CPU arm).
            // One hit tester for every render mode. This used to run only as a
            // CPU fallback, with a parallel WebRender path below for GPU mode;
            // the two disagreed on coordinate space, so which node a click
            // resolved to depended on the renderer.
            PlatformWindow::update_hit_test_at(&mut *window, logical_pos);
            let hit_test = window.common.perform_hit_test(logical_pos);

            // Update hit test (GPU mode only — CPU mode handled above)
            if let Some(ref mut layout_window) = window.common.layout_window {
                {

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

            let hidpi_factor = window.common.current_window_state().size.get_hidpi_factor();
            let logical_pos = LogicalPosition::new(
                x as f32 / hidpi_factor.inner.get(),
                y as f32 / hidpi_factor.inner.get(),
            );

            // Save previous state
            window.snapshot_window_state_baseline("windows.wm_mbuttondown");

            // Update mouse state
            window.common.mouse_state_mut().cursor_position = CursorPosition::InWindow(logical_pos);
            window.common.mouse_state_mut().middle_down = true;

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

            let hidpi_factor = window.common.current_window_state().size.get_hidpi_factor();
            let logical_pos = LogicalPosition::new(
                x as f32 / hidpi_factor.inner.get(),
                y as f32 / hidpi_factor.inner.get(),
            );

            // Save previous state
            window.snapshot_window_state_baseline("windows.wm_mbuttonup");

            // Update mouse state
            window.common.mouse_state_mut().cursor_position = CursorPosition::InWindow(logical_pos);
            window.common.mouse_state_mut().middle_down = false;

            // V2 system will detect MouseUp event
            let result = window.process_window_events(0);

            window.route_main_window_result(hwnd, result);

            0
        }

        WM_XBUTTONDOWN | WM_XBUTTONUP => {
            // Mouse buttons 4 and 5 (the thumb "back"/"forward" pair). There
            // was no arm at all, so on Windows they fell through to
            // DefWindowProc and the engine never even learned the cursor had
            // moved with them — macOS routes the same buttons through
            // `otherMouse*` and X11 through buttons 8/9.
            //
            // Two Win32 peculiarities, both of them traps: the button is in the
            // HIGH word of wParam (the low word is the modifier set), and MSDN
            // requires TRUE, not 0, as the return value.
            let is_down = msg == WM_XBUTTONDOWN;
            let button = match crate::desktop::shell2::common::event::win32_xbutton_to_mouse_button(
                wparam,
            ) {
                Some(button) => button,
                None => return 1,
            };

            let x = (lparam & 0xFFFF) as i16 as i32;
            let y = ((lparam >> 16) & 0xFFFF) as i16 as i32;

            use azul_core::geom::LogicalPosition;

            let hidpi_factor = window.common.current_window_state().size.get_hidpi_factor();
            let logical_pos = LogicalPosition::new(
                x as f32 / hidpi_factor.inner.get(),
                y as f32 / hidpi_factor.inner.get(),
            );

            // Save previous state
            window.snapshot_window_state_baseline(if is_down {
                "windows.wm_xbuttondown"
            } else {
                "windows.wm_xbuttonup"
            });

            // `MouseState.other_down` now carries buttons 4/5, and
            // `apply_pointer_button_state` records them, so a press reaches
            // callbacks as a real MouseDown/MouseUp with a Back/Forward
            // filter rather than as bare pointer motion.
            crate::desktop::shell2::common::event::apply_pointer_button_state(
                window.common.mouse_state_mut(),
                logical_pos,
                button,
                is_down,
            );

            let result = window.process_window_events(0);

            window.route_main_window_result(hwnd, result);

            1
        }

        WM_ENTERSIZEMOVE => {
            // A drag-resize or drag-move hands control to a MODAL loop inside
            // USER32: `DispatchMessageW` does not return until the user lets
            // go, so `run.rs`'s loop body — pending window creates, the
            // accessibility drain, the cross-window regeneration sweep and the
            // deferred free of closed windows — does not run for the whole
            // drag. Messages ARE still pumped inside that loop, so a timer is
            // the way back in: `WM_TIMER` runs the same per-iteration work.
            window.start_modal_loop_pump();
            0
        }

        WM_EXITSIZEMOVE => {
            // The outer loop is about to get control back, so the stand-in
            // timer must go — leaving it armed would run the sweep twice per
            // frame forever after the first resize.
            window.stop_modal_loop_pump();
            // The loop ate the button-up that ended it; release what is
            // latched (see the method for the symptom).
            window.release_buttons_swallowed_by_modal_loop(hwnd);
            0
        }

        WM_NCCALCSIZE => {
            // Frameless windows hand the frame's area to the client here;
            // everything else keeps DefWindowProc's answer.
            match window.handle_nccalcsize(hwnd, wparam, lparam) {
                Some(r) => r,
                None => def_window_proc_w(hwnd, msg, wparam, lparam),
            }
        }

        WM_NCHITTEST => {
            // Frameless windows: resize band or client, never HTCAPTION
            // from the style (see `handle_nchittest`).
            match window.handle_nchittest(hwnd, lparam) {
                Some(r) => r,
                None => def_window_proc_w(hwnd, msg, wparam, lparam),
            }
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

            // Pixels per wheel notch, from the USER's setting.
            // SystemParametersInfo(SPI_GETWHEELSCROLLLINES) was already being
            // captured into SystemStyle.input.wheel_scroll_lines and then read
            // by nobody — the notch was hardcoded at WHEEL_TICK_PIXELS, so the
            // Control Panel / mouse-driver scroll speed did nothing on
            // Windows.
            //
            // The ratio-against-the-default rule and the WHEEL_PAGESCROLL
            // sentinel live in `common` so they are tested on every host —
            // nothing in CI compiles this module. 0 lines is a legal setting
            // meaning "wheel scrolling off", and the `px_per_notch > 0.0` gate
            // below is what honours it.
            let wheel_lines = window.common.system_style.input.wheel_scroll_lines;
            let dims = window.common.current_window_state().size.dimensions;
            let px_per_notch = crate::desktop::shell2::common::event::win32_wheel_pixels_per_notch(
                wheel_lines,
                if horizontal { dims.width } else { dims.height },
            );

            // MWA-C-scroll: WM_MOUSEWHEEL/WM_MOUSEHWHEEL carry SCREEN
            // coordinates in lparam (unlike the client-relative WM_MOUSE*
            // messages) — convert first, or on any window not at the
            // desktop origin the wheel hit-tested a spot offset by the
            // window position and scrolled the wrong (or no) container.
            let mut wheel_pt = dlopen::POINT {
                x: (lparam & 0xFFFF) as i16 as i32,
                y: ((lparam >> 16) & 0xFFFF) as i16 as i32,
            };
            unsafe {
                (window.win32.user32.ScreenToClient)(hwnd, &mut wheel_pt);
            }
            let (x, y) = (wheel_pt.x, wheel_pt.y);

            use azul_core::{geom::LogicalPosition, window::CursorPosition};

            let hidpi_factor = window.common.current_window_state().size.get_hidpi_factor();
            let logical_pos = LogicalPosition::new(
                x as f32 / hidpi_factor.inner.get(),
                y as f32 / hidpi_factor.inner.get(),
            );

            // Save previous state
            window.snapshot_window_state_baseline("windows.wm_mousewheel");

            // MWA-C-scroll: refresh the hit test BEFORE recording the wheel
            // delta (macOS/X11 order). record_scroll_from_hit_test targets
            // hover_manager.get_current(), so recording first aimed the
            // delta at wherever the pointer was on the LAST mouse-move —
            // wrong container right after a layout change or fast move.
            // CPU mode (no WR hit_tester/document_id) uses the shared
            // perform_hit_test → cpu_hit_tester path.
            // One hit tester for every render mode. This used to run only as a
            // CPU fallback, with a parallel WebRender path below for GPU mode;
            // the two disagreed on coordinate space, so which node a click
            // resolved to depended on the renderer.
            PlatformWindow::update_hit_test_at(&mut *window, logical_pos);
            let hit_test = window.common.perform_hit_test(logical_pos);
            if let Some(ref mut layout_window) = window.common.layout_window {
                {

                    layout_window
                        .hover_manager
                        .push_hit_test(InputPointId::Mouse, hit_test);
                }
            }

            // Queue scroll input for the physics timer instead of directly setting offsets.
            // The timer will consume these via ScrollInputQueue and push CallbackChange::ScrollTo.
            if delta.abs() > 0 && px_per_notch > 0.0 {
                let mut should_start_timer = false;
                let mut input_queue_clone = None;

                if let Some(ref mut layout_window) = window.common.layout_window {
                    use azul_core::task::Instant;
                    use azul_layout::managers::scroll_state::ScrollInputSource;

                    let now = Instant::from(std::time::Instant::now());

                    if let Some((_dom_id, _node_id, start_timer)) =
                        layout_window.scroll_manager.record_scroll_from_hit_test(
                            // MWA-C-scroll: WM_MOUSEHWHEEL positive = wheel
                            // tilted RIGHT (MSDN), but azul's raw-delta
                            // chokepoint uses the X11 convention where
                            // button 6 / LEFT = +1 and button 7 / RIGHT = −1
                            // (Wayland negates its positive-right axis value
                            // the same way). Vertical already matches
                            // (positive = rotated away = up = +1); horizontal
                            // must be NEGATED or tilt-wheel / trackpad
                            // horizontal scrolling runs backwards.
                            if horizontal {
                                -scroll_amount * px_per_notch
                            } else {
                                0.0
                            },
                            if horizontal {
                                0.0
                            } else {
                                scroll_amount * px_per_notch
                            },
                            ScrollInputSource::WheelDiscrete,
                            // WM_MOUSEWHEEL is also what precision touchpads
                            // fall back to; without DirectManipulation there
                            // is no reliable way to tell them apart, so the
                            // physical wheel is the honest default here.
                            azul_layout::managers::scroll_state::ScrollInputDevice::MouseWheel,
                            &layout_window.hover_manager,
                            &InputPointId::Mouse,
                            now,
                        )
                    {
                        // GUARD: `start_timer` only means "the input queue was drained
                        // when this event arrived", which the 16 ms physics tick
                        // makes true for almost every event of a gesture. Without
                        // also checking that the timer is not already registered,
                        // `start_timer` below REPLACED the live `ScrollPhysicsState`
                        // — throwing away velocity, animate targets and pending
                        // positions mid-gesture, and resetting the tick phase. The
                        // shared arming site in `common/event.rs` has always had
                        // this check.
                        should_start_timer = start_timer
                            && !layout_window
                                .timers
                                .contains_key(&azul_core::task::SCROLL_MOMENTUM_TIMER_ID);
                        if start_timer {
                            input_queue_clone =
                                Some(layout_window.scroll_manager.get_input_queue());
                        }
                    }
                }

                // Start the scroll momentum timer if this is the first input
                if should_start_timer {
                    if let Some(queue) = input_queue_clone {
                        use azul_core::refany::RefAny;
                        use azul_core::task::Duration;
                        use azul_core::task::SCROLL_MOMENTUM_TIMER_ID;
                        use azul_layout::scroll_timer::{
                            scroll_physics_timer_callback, ScrollPhysicsState,
                        };
                        use azul_layout::timer::{Timer, TimerCallbackType};

                        let physics_state = ScrollPhysicsState::new(
                            queue,
                            window.common.system_style.scroll_physics.clone(),
                        );
                        let interval_ms =
                            window.common.system_style.scroll_physics.timer_interval_ms;
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

            // (hit test already refreshed above, before the delta was recorded)

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

            // Translate virtual key to azul key. `None` — a key the table has
            // no entry for — is NOT a reason to skip the handler: the SCANCODE
            // names the physical key and needs no table to be true, and gating
            // its write on the translation left media keys, the browser cluster
            // and every OEM key on a non-US layout missing from
            // `pressed_scancodes`. The pass has to run either way, or the
            // keyboard-state delta this leaves behind trips the
            // unconsumed-input guard in the next handler.
            let virtual_key = win_event::vkey_to_winit_vkey(vk_code as i32);

            // Save previous state. For key repeats, clear current_virtual_keycode
            // in the snapshot so the state-diff sees None → Some(key).
            let mut prev_snapshot = window.common.current_window_state().clone();
            if is_repeat {
                prev_snapshot.keyboard_state.current_virtual_keycode =
                    azul_core::window::OptionVirtualKeyCode::None;
            }
            window.set_previous_window_state(prev_snapshot);

            // Update keyboard state
            crate::desktop::shell2::common::event::apply_win32_key_state_change(
                window.common.keyboard_state_mut(),
                virtual_key,
                scan_code,
                true,
            );

            // V2 system will detect VirtualKeyDown event
            let result = window.process_window_events(0);

            window.route_main_window_result(hwnd, result);

            // The SYS variants MUST reach DefWindowProc: that is what turns
            // Alt+F4's WM_SYSKEYDOWN into WM_SYSCOMMAND/SC_CLOSE and what puts
            // F10 / Alt into menu mode. Neither produces a WM_SYSCHAR (the arm
            // below that already forwards), so swallowing WM_SYSKEYDOWN killed
            // Alt+F4, F10 and every Alt mnemonic outright. The plain
            // WM_KEYDOWN stays swallowed — DefWindowProc has nothing to add
            // there, and the WM_CHAR text path comes from TranslateMessage.
            if msg == WM_SYSKEYDOWN {
                def_window_proc_w(hwnd, msg, wparam, lparam)
            } else {
                0
            }
        }

        WM_KEYUP | WM_SYSKEYUP => {
            // Key released - similar to macOS handle_key_up
            let vk_code = wparam as u32;
            let scan_code = ((lparam >> 16) & 0xFF) as u32;

            // Translate virtual key. Ungated for the same reason as the
            // key-down arm: an unmapped key that got INTO `pressed_scancodes`
            // has to be able to come back out, and the gate used to stop both
            // halves — so any key the table does not know would have stayed
            // latched down for the rest of the session.
            let virtual_key = win_event::vkey_to_winit_vkey(vk_code as i32);

            // Save previous state
            window.snapshot_window_state_baseline("windows.wm_keyup");

            // Update keyboard state
            crate::desktop::shell2::common::event::apply_win32_key_state_change(
                window.common.keyboard_state_mut(),
                virtual_key,
                scan_code,
                false,
            );

            // V2 system will detect VirtualKeyUp event
            let result = window.process_window_events(0);

            window.route_main_window_result(hwnd, result);

            // Same as WM_SYSKEYDOWN: DefWindowProc completes the menu-mode
            // entry a bare Alt press starts (Alt down + up activates the menu
            // bar), so the SYS variant falls through.
            if msg == WM_SYSKEYUP {
                def_window_proc_w(hwnd, msg, wparam, lparam)
            } else {
                0
            }
        }

        WM_SYSCHAR => {
            // Alt+key (WM_SYSKEYDOWN → TranslateMessage). This is NOT text:
            // feeding it into record_text_input made Alt+X type an 'x' into
            // the focused field, and returning 0 without DefWindowProc ALSO
            // ate the system-menu / menu-mnemonic handling (Alt+Space,
            // Alt+F for a native HMENU menu bar). AltGr characters on
            // international layouts arrive as plain WM_CHAR (AltGr =
            // Ctrl+Alt clears the sys flag), so text input is unaffected.
            (window.win32.user32.DefWindowProcW)(hwnd, msg, wparam, lparam)
        }

        WM_CHAR => {
            // Character input - for text input
            let char_code = wparam as u32;

            // UTF-16 surrogate pairing lives in `common` so it is tested on
            // every host — nothing in CI compiles this module.
            let char_opt = crate::desktop::shell2::common::event::win32_utf16_stream_char(
                &mut window.high_surrogate,
                char_code,
            );

            // Update keyboard state with character
            if let Some(chr) = char_opt {
                window.snapshot_window_state_baseline("windows.wm_char");

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
                    // Commit rather than a bare clear, so `CompositionEnd`
                    // can report what was committed.
                    //
                    // Win32 does not hand us the result string here — it lets
                    // DefWindowProc turn it into WM_IME_CHAR messages, which
                    // arrive after this branch returns. The preedit as it
                    // stands at GCS_RESULTSTR is what the user was looking at
                    // when they accepted the candidate, so that is what the
                    // event reports; the actual insertion still happens via
                    // WM_IME_CHAR -> record_text_input as before.
                    let committed = lw.text_edit_manager.preedit_text.clone().unwrap_or_default();
                    lw.text_edit_manager.commit_composition(committed);
                    // MWA-C-text_input: restore the pre-preedit text cache
                    // (apply with empty preedit = restore + re-shape).
                    if let Some((dom_id, node_id)) = lw
                        .text_edit_manager
                        .get_editing_dom_id()
                        .zip(lw.text_edit_manager.get_editing_node_id())
                    {
                        lw.apply_preedit_to_text_cache(dom_id, node_id);
                    }
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
                                                text.clone(),
                                                0,
                                                text.len() as i32,
                                            );
                                            // MWA-C-text_input: splice the composition
                                            // glyphs into the text cache (macOS-only
                                            // before) — Windows CJK composition showed
                                            // only an approximate-width underline with
                                            // no visible text.
                                            if let Some((dom_id, node_id)) = lw
                                                .text_edit_manager
                                                .get_editing_dom_id()
                                                .zip(lw.text_edit_manager.get_editing_node_id())
                                            {
                                                lw.apply_preedit_to_text_cache(dom_id, node_id);
                                            }
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

                // Re-run layout on the existing StyledDom so the caret rect
                // accounts for the preedit that was just spliced into the text
                // cache, then push the new rect to the IME. Only
                // WM_IME_STARTCOMPOSITION, WM_SETFOCUS and the
                // regenerate_layout tail used to sync it, so the composition
                // and candidate windows stayed pinned to the
                // composition-START rect while the preedit grew and wrapped —
                // a long Japanese phrase ended up with its candidate list
                // lines away from the text it belonged to.
                let mut debug_messages = None;
                if let Err(e) = window.incremental_relayout_dispatching(
                    crate::desktop::shell2::common::event::IncrementalRelayout::Restyle,
                    &mut debug_messages,
                ) {
                    log_warn!(LogCategory::Layout, "IME preedit relayout failed: {}", e);
                }
                window.common.request_relayout_only();
                window.update_ime_position_from_cursor();
                window.sync_ime_position_to_os();

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
                // MWA-C-text_input: restore the pre-preedit text cache.
                if let Some((dom_id, node_id)) = lw
                    .text_edit_manager
                    .get_editing_dom_id()
                    .zip(lw.text_edit_manager.get_editing_node_id())
                {
                    lw.apply_preedit_to_text_cache(dom_id, node_id);
                }
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

            // WM_IME_CHAR carries UTF-16 exactly like WM_CHAR, so a
            // supplementary-plane commit (rare CJK ideographs, emoji from an
            // IME) arrives as two surrogate halves. Feeding a half straight to
            // char::from_u32 returns None, which silently DROPPED both halves
            // — pair them with the same `high_surrogate` slot WM_CHAR uses
            // (the two messages never interleave: an IME commit produces one
            // or the other, not both).
            let char_opt = crate::desktop::shell2::common::event::win32_utf16_stream_char(
                &mut window.high_surrogate,
                char_code,
            );

            if let Some(chr) = char_opt {
                window.snapshot_window_state_baseline("windows.wm_ime_char");

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

        WM_IME_REQUEST => {
            // IMR_QUERYCHARPOSITION is how a TSF-based IME (i.e. every modern
            // one) asks where the caret is; it does NOT read the IMM
            // composition form. The message was declared but never matched, so
            // it fell through to DefWindowProc, which answers FALSE — and the
            // IME then guessed, parking the candidate list at the window
            // origin. Answer it with the live caret rect in SCREEN coords.
            const IMR_QUERYCHARPOSITION: usize = 0x0006;

            if (wparam as usize) == IMR_QUERYCHARPOSITION && lparam != 0 {
                use azul_core::window::ImePosition;

                let cp = &mut *(lparam as *mut dlopen::IMECHARPOSITION);
                // The IME declares how much of the struct it allocated; a
                // smaller one is a different (older) layout and must not be
                // written through.
                let caret = match window.common.current_window_state().ime_position {
                    ImePosition::Initialized(r)
                        if cp.dwSize as usize
                            >= core::mem::size_of::<dlopen::IMECHARPOSITION>() =>
                    {
                        Some(r)
                    }
                    _ => None,
                };

                if let Some(rect) = caret {
                    let hf = window
                        .common
                        .current_window_state()
                        .size
                        .get_hidpi_factor()
                        .inner
                        .get();

                    // Caret origin: client (logical) -> client (physical) -> screen.
                    let mut pt = dlopen::POINT {
                        x: libm::roundf(rect.origin.x * hf) as i32,
                        y: libm::roundf(rect.origin.y * hf) as i32,
                    };
                    (window.win32.user32.ClientToScreen)(hwnd, &mut pt);

                    // Document area: the whole client rect, also in screen coords.
                    let mut client = dlopen::RECT::default();
                    (window.win32.user32.GetClientRect)(hwnd, &mut client);
                    let mut tl = dlopen::POINT {
                        x: client.left,
                        y: client.top,
                    };
                    let mut br = dlopen::POINT {
                        x: client.right,
                        y: client.bottom,
                    };
                    (window.win32.user32.ClientToScreen)(hwnd, &mut tl);
                    (window.win32.user32.ClientToScreen)(hwnd, &mut br);

                    cp.pt = pt;
                    cp.cLineHeight = (libm::roundf(rect.size.height * hf) as i32).max(1) as u32;
                    cp.rcDocument = dlopen::RECT {
                        left: tl.x,
                        top: tl.y,
                        right: br.x,
                        bottom: br.y,
                    };
                    return 1;
                }
            }

            (window.win32.user32.DefWindowProcW)(hwnd, msg, wparam, lparam)
        }

        WM_IME_NOTIFY | WM_IME_SETCONTEXT => {
            // Other IME events - use default processing
            (window.win32.user32.DefWindowProcW)(hwnd, msg, wparam, lparam)
        }

        WM_SETFOCUS => {
            // Window gained focus
            let prev_snapshot = window.common.current_window_state().clone();
            // Focus is OS-reported (source = Os): acknowledging it into the
            // sync baseline is what stops sync_window_state() from answering
            // with a SetFocus/SetForegroundWindow of its own.
            window.common.update_window_state(
                crate::desktop::shell2::common::event::WindowStateSource::Os,
                |ws| {
                    ws.flags.has_focus = true;
                    ws.window_focused = true;
                },
            );
            window.dynamic_selector_context.window_focused = true;

            // Re-read the pressed-key set: the releases that happened while
            // another window had the focus never reached us (Alt+Tab's LAlt
            // release is the classic one), so without this Alt stays latched
            // and every later click is treated as an Alt+click.
            window.resync_keyboard_state_from_os();

            window.set_previous_window_state(prev_snapshot);

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
            let prev_snapshot = window.common.current_window_state().clone();
            window.common.update_window_state(
                crate::desktop::shell2::common::event::WindowStateSource::Os,
                |ws| {
                    ws.flags.has_focus = false;
                    ws.window_focused = false;
                },
            );
            window.dynamic_selector_context.window_focused = false;

            // Drop every held key. Nothing that happens while we are unfocused
            // reaches us — least of all the KEY-UP of the modifier that caused
            // the focus change (Alt of Alt+Tab), which is exactly the key that
            // would stay latched. `current_virtual_keycode = None` also makes
            // the diff fire the matching KeyUp.
            {
                use azul_core::window::{OptionVirtualKeyCode, ScanCodeVec, VirtualKeyCodeVec};
                let ks = window.common.keyboard_state_mut();
                ks.current_virtual_keycode = OptionVirtualKeyCode::None;
                ks.pressed_virtual_keycodes = VirtualKeyCodeVec::from_vec(Vec::new());
                ks.pressed_scancodes = ScanCodeVec::from_vec(Vec::new());
            }

            // The same argument, for the mouse — and it was missing on every
            // platform. A button held when focus leaves has its BUTTON-UP
            // delivered to whoever took focus, so `left_down` stays latched
            // exactly like the Alt of Alt+Tab. Every later move then reads as a
            // drag: text selects and buttons stop clicking, with nothing able
            // to clear it. Clearing here makes the diff fire the matching
            // MouseUp.
            {
                let ms = window.common.mouse_state_mut();
                ms.left_down = false;
                ms.right_down = false;
                ms.middle_down = false;
            }

            window.set_previous_window_state(prev_snapshot);

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
            let modal_tick = wparam == Win32Window::MODAL_LOOP_TIMER_ID;
            if window.process_timers_and_threads() {
                (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
            }

            if modal_tick {
                // Inside a modal size/move loop this timer is the only thing
                // still driving `run.rs`'s per-iteration work. NOTHING below
                // may touch `window` again: the pump re-borrows every window
                // from the registry — this one included — and may free one, so
                // the `&mut Win32Window` above has to be dead by here.
                pump_modal_loop_work();
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
                let borrows = window.common.layout_borrows();
                if let Some(layout_window) = borrows.layout_window {
                    use azul_core::window::RawWindowHandle;

                    let raw_handle = RawWindowHandle::Windows(azul_core::window::WindowsHandle {
                        hwnd: hwnd as *mut _,
                        hinstance: ptr::null_mut(), // Not needed for menu callbacks
                    });

                    let (changes, update) = layout_window.invoke_single_callback(
                        &mut menu_callback.callback,
                        &mut menu_callback.refany,
                        &raw_handle,
                        borrows.gl_context_ptr,
                        borrows.system_style.clone(),
                        &azul_layout::callbacks::ExternalSystemCallbacks::rust_internal(),
                        borrows.previous_window_state,
                        borrows.current_window_state,
                        borrows.renderer_resources,
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
                            // ShouldIncrementalRelayout arm. The relayout-only request then
                            // makes WM_PAINT skip regenerate_layout and only rebuild +
                            // send the WebRender transaction.
                            let mut debug_messages = None;
                            if let Err(e) = window.incremental_relayout_dispatching(
                                crate::desktop::shell2::common::event::IncrementalRelayout::Restyle,
                                &mut debug_messages,
                            ) {
                                log_warn!(
                                    LogCategory::Layout,
                                    "Incremental relayout failed: {}",
                                    e
                                );
                            }
                            window.common.request_relayout_only();
                            (window.win32.user32.InvalidateRect)(hwnd, ptr::null(), 0);
                        }
                        ProcessEventResult::ShouldRegenerateDomCurrentWindow
                        | ProcessEventResult::ShouldRegenerateDomAllWindows
                        | ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                            window.common.request_regeneration(
                                azul_core::callbacks::RelayoutReason::RefreshDom,
                            );
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
            let old_dpi = window.common.current_window_state().size.dpi;

            // Update DPI in window state. The SetWindowPos below dispatches
            // WM_SIZE SYNCHRONOUSLY and that handler reads size.dpi to convert
            // the physical client rect, so the new DPI has to be in place
            // first — which is also why the diff baseline cannot simply be
            // snapshotted here: the nested WM_SIZE runs its own pass, and a
            // pass ends by consuming the delta (previous = current), so a
            // baseline taken now would be gone before this arm could dispatch
            // WindowDpiChanged. It is rebuilt AFTER the nested resize settles,
            // at the bottom of this arm.
            window.common.update_window_state(
                crate::desktop::shell2::common::event::WindowStateSource::Os,
                |ws| ws.size.dpi = new_dpi,
            );

            // Get suggested size from lParam (RECT*). Per MSDN this is "a
            // suggested size and position of the current window scaled for
            // the new DPI" — an OUTER window rect (frame included, screen
            // coords), to be applied verbatim via SetWindowPos.
            if lparam != 0 {
                let rect = unsafe { &*(lparam as *const dlopen::RECT) };
                let width = rect.right - rect.left;
                let height = rect.bottom - rect.top;

                // Update window size to suggested dimensions. SetWindowPos
                // dispatches WM_SIZE synchronously, and THAT handler derives
                // size.dimensions from the actual CLIENT rect with the new
                // DPI (stored above). Do NOT overwrite dimensions from this
                // rect afterwards: it is the frame-inclusive WINDOW size, and
                // treating it as the client size inflated the logical layout
                // by the title bar + borders after every monitor change —
                // oversized layout, clipped bottom/right, offset hit tests.
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
            }

            // Belt-and-braces: if the suggested rect happened to equal the
            // current geometry (or lparam was 0), SetWindowPos dispatched no
            // WM_SIZE — re-derive the logical size from the ACTUAL client
            // rect under the new DPI so dimensions are consistent either way
            // (same physical client / new scale = different logical size).
            if let Ok((w, h)) = wcreate::get_client_rect(hwnd, &window.win32) {
                let physical_size = azul_core::geom::PhysicalSizeU32::new(w, h);
                let logical = physical_size.to_logical(new_dpi as f32 / 96.0);
                window.common.update_window_state(
                    crate::desktop::shell2::common::event::WindowStateSource::Os,
                    |ws| ws.size.dimensions = logical,
                );
            }

            // DPI change requires a full relayout, tagged `Resize` — the enum's
            // own definition covers "DPI scale change", and the X11 DPI path
            // already tags it that way. WM_DPICHANGED used to leave the tag
            // untouched, so the same physical event reported a different reason
            // to the user's `layout()` depending on which OS delivered it.
            window
                .common
                .request_regeneration(azul_core::callbacks::RelayoutReason::Resize);

            // Dispatch the DPI transition. `WindowDpiChanged` is derived from
            // the size.dpi delta, so the baseline is the state as it stands
            // NOW with the dpi rolled back — that is the one and only
            // difference left, so the nested WM_SIZE's WindowResize is not
            // dispatched a second time here. Building the baseline this way
            // (rather than snapshotting at the top of the arm) is what makes
            // the transition survive that nested pass.
            {
                let mut baseline = window.common.current_window_state().clone();
                baseline.size.dpi = old_dpi;
                window.set_previous_window_state(baseline);
                let r = window.process_window_events(0);
                window.route_main_window_result(hwnd, r);
            }

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
            // Refresh the cached monitor list, and turn the before/after count
            // into MonitorConnected / MonitorDisconnected.
            //
            // WM_DISPLAYCHANGE does not say WHAT changed — it fires for a
            // resolution or colour-depth change just as readily as for a
            // monitor being unplugged — so the count is the only signal that
            // separates a hotplug from a mode change. Equal counts emit
            // nothing, which is what keeps dragging a window between displays
            // from looking like an unplug.
            if let Some(ref mut lw) = window.common.layout_window {
                let before = lw.monitors.lock().map(|g| g.len()).unwrap_or(0);
                let after = {
                    let refreshed = crate::desktop::display::refresh_monitors();
                    let n = refreshed.len();
                    if let Ok(mut guard) = lw.monitors.lock() {
                        *guard = refreshed;
                    }
                    n
                };
                lw.device_event_manager
                    .note_monitor_count_change(before, after);
            }
            0
        }

        WM_GESTURE => {
            // Touchscreen gestures. DefWindowProc is still called on the
            // WM_POINTER* arms, so Windows does synthesize these for us — they
            // simply had no handler.
            //
            // Scope worth being honest about: WM_GESTURE is the touch path.
            // A Windows precision touchpad does NOT deliver pinch here — it
            // reports pan and zoom as WM_MOUSEWHEEL / WM_MOUSEHWHEEL (zoom as
            // Ctrl+wheel, which is the convention browsers zoom on), and the
            // raw finger geometry is only reachable through Direct
            // Manipulation. So this closes the touchscreen half on Windows;
            // the touchpad half is a different API and a separate item.
            let (Some(get_info), Some(close_info)) = (
                window.win32.user32.GetGestureInfo,
                window.win32.user32.CloseGestureInfoHandle,
            ) else {
                return def_window_proc_w(hwnd, msg, wparam, lparam);
            };

            let mut gi = dlopen::GESTUREINFO {
                cbSize: core::mem::size_of::<dlopen::GESTUREINFO>() as u32,
                ..Default::default()
            };
            if unsafe { get_info(lparam, &mut gi) } == 0 {
                return def_window_proc_w(hwnd, msg, wparam, lparam);
            }

            // ptsLocation is in SCREEN coordinates; every other gesture path
            // reports a client-space centre.
            // ptsLocation is in SCREEN coordinates, like WM_MOUSEWHEEL's
            // lParam and unlike the client-relative WM_MOUSE* messages — so
            // it needs the same ScreenToClient + hidpi conversion the wheel
            // path does, or a gesture on a window away from the desktop
            // origin lands at an offset.
            let center = {
                let mut pt = dlopen::POINT {
                    x: i32::from(gi.ptsLocation_x),
                    y: i32::from(gi.ptsLocation_y),
                };
                unsafe {
                    (window.win32.user32.ScreenToClient)(hwnd, &mut pt);
                }
                let hidpi = window.common.current_window_state().size.get_hidpi_factor();
                azul_core::geom::LogicalPosition::new(
                    pt.x as f32 / hidpi.inner.get(),
                    pt.y as f32 / hidpi.inner.get(),
                )
            };

            use azul_layout::managers::gesture::{
                DetectedLongPress, DetectedPinch, DetectedRotation, NativeGestureEvent,
            };
            const PINCH_NOMINAL_DISTANCE: f32 = 100.0;

            match gi.dwID {
                dlopen::GID_ZOOM => {
                    // ullArguments is the current distance between the two
                    // contacts, in pixels. The FIRST message of a zoom carries
                    // the baseline and no scale, so it is stored, not acted on.
                    let distance = gi.ullArguments as f32;
                    if gi.dwFlags & dlopen::GF_BEGIN != 0 || window.gesture_zoom_baseline <= 0.0 {
                        window.gesture_zoom_baseline = distance.max(1.0);
                    } else if let Some(ref mut lw) = window.common.layout_window {
                        let scale = distance / window.gesture_zoom_baseline;
                        lw.gesture_drag_manager.inject_native_gesture(
                            NativeGestureEvent::Pinch(DetectedPinch {
                                scale,
                                center,
                                initial_distance: PINCH_NOMINAL_DISTANCE,
                                current_distance: PINCH_NOMINAL_DISTANCE * scale,
                                duration_ms: 0,
                            }),
                        );
                    }
                }
                dlopen::GID_ROTATE => {
                    // The low 32 bits are a rotation angle encoded by
                    // GID_ROTATE_ANGLE_FROM_ARGUMENT: 0..=65535 maps onto
                    // -pi..=pi. As with zoom, the begin message is the origin.
                    if gi.dwFlags & dlopen::GF_BEGIN == 0 {
                        let raw = (gi.ullArguments & 0xFFFF) as f32;
                        let angle = (raw / 65535.0) * (core::f32::consts::PI * 2.0)
                            - core::f32::consts::PI;
                        if let Some(ref mut lw) = window.common.layout_window {
                            lw.gesture_drag_manager.inject_native_gesture(
                                NativeGestureEvent::Rotation(DetectedRotation {
                                    angle_radians: angle,
                                    center,
                                    duration_ms: 0,
                                }),
                            );
                        }
                    }
                }
                dlopen::GID_PRESSANDTAP | dlopen::GID_TWOFINGERTAP => {
                    // Both are the touch spelling of "secondary action" — the
                    // same reading the X11 pinch-with-no-movement gets.
                    if let Some(ref mut lw) = window.common.layout_window {
                        lw.gesture_drag_manager.inject_native_gesture(
                            NativeGestureEvent::LongPress(DetectedLongPress {
                                position: center,
                                duration_ms: 0,
                                callback_invoked: false,
                                session_id: 0,
                            }),
                        );
                    }
                }
                dlopen::GID_END => {
                    window.gesture_zoom_baseline = 0.0;
                }
                _ => {}
            }

            unsafe { close_info(lparam) };
            // Still hand it to DefWindowProc: it owns the inertia and the
            // panning fallback, and swallowing the message loses both.
            def_window_proc_w(hwnd, msg, wparam, lparam)
        }

        WM_APPCOMMAND => {
            // Media and browser keys. Windows does NOT deliver these as
            // WM_KEYDOWN — a keyboard's media row and a mouse's thumb buttons
            // both arrive here, on a separate message — so with no handler
            // they reached DefWindowProc and vanished.
            //
            // The command id lives in the HIGH word of lParam with the device
            // and key-state bits masked off; the low word is the window
            // handle, not a coordinate.
            const FAPPCOMMAND_MASK: u16 = 0xF000;
            let cmd = (((lparam >> 16) & 0xFFFF) as u16) & !FAPPCOMMAND_MASK;

            let Some(vk) =
                crate::desktop::shell2::common::event::win32_appcommand_to_virtual_key(cmd)
            else {
                return def_window_proc_w(hwnd, msg, wparam, lparam);
            };

            window.snapshot_window_state_baseline("windows.wm_appcommand");

            // Delivered as a KEY, because that is what VirtualKeyCode::
            // PlayPause already is — an app binding it works unchanged on the
            // platforms that do route these as ordinary keys. WM_APPCOMMAND
            // has no release message, so the press is immediately followed by
            // the release: leaving the key latched would make it look held
            // forever.
            {
                use azul_core::window::OptionVirtualKeyCode;
                let kb = window.common.keyboard_state_mut();
                kb.current_virtual_keycode = OptionVirtualKeyCode::Some(vk);
            }
            let result = window.process_window_events(0);
            window.route_main_window_result(hwnd, result);

            {
                use azul_core::window::OptionVirtualKeyCode;
                let kb = window.common.keyboard_state_mut();
                kb.current_virtual_keycode = OptionVirtualKeyCode::None;
            }
            let result = window.process_window_events(0);
            window.route_main_window_result(hwnd, result);

            // Returning non-zero says "handled"; DefWindowProc would
            // otherwise forward it to the shell and the OS would act on it
            // too, so a play/pause would toggle twice.
            1
        }

        WM_DEVICECHANGE => {
            // DBT_DEVNODES_CHANGED (0x0007) is the broadcast every top-level
            // window receives with no RegisterDeviceNotification call at all —
            // which is why it is worth handling: the richer per-interface
            // notifications need a registration and a filter, and this one
            // covers the case an app actually cares about (something was
            // plugged in or pulled out).
            //
            // It does not say which direction, so it cannot be turned into a
            // connect/disconnect pair honestly. Gamepads are already covered
            // by gilrs (4b) with real edges; this is the catch-all for
            // everything else, reported as an arrival because that is the
            // transition an app reacts to by re-enumerating.
            const DBT_DEVNODES_CHANGED: usize = 0x0007;
            if wparam == DBT_DEVNODES_CHANGED {
                if let Some(ref mut lw) = window.common.layout_window {
                    lw.device_event_manager.note_device(true);
                }
            }
            0
        }

        WM_APP_SHOW_PENDING_MENU_LOCAL => {
            // Take the menu and the function pointers, then STOP touching
            // `window`. TrackPopupMenu below runs a modal loop that re-enters
            // window_proc, which fetches its own `&mut` from GWLP_USERDATA —
            // sound only because this one is dead by then.
            let Some(pending) = window.pending_native_menu.take() else {
                return 0;
            };
            let hwnd_local = window.hwnd;
            let set_foreground = window.win32.user32.SetForegroundWindow;
            let track = window.win32.user32.TrackPopupMenu;
            let destroy = window.win32.user32.DestroyMenu;

            unsafe {
                set_foreground(hwnd_local);
                track(
                    pending.hmenu,
                    dlopen::constants::TPM_RIGHTBUTTON | dlopen::constants::TPM_LEFTALIGN,
                    pending.x,
                    pending.y,
                    0,
                    hwnd_local,
                    ptr::null(),
                );
                destroy(pending.hmenu);
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
                let hidpi = window.common.current_window_state().size.get_hidpi_factor();
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
                    .current_window_state()
                    .size
                    .min_dimensions
                    .into_option()
                {
                    unsafe {
                        (*mmi).pt_min_track_size.x = (min.width * hf).round() as i32 + frame_w;
                        (*mmi).pt_min_track_size.y = (min.height * hf).round() as i32 + frame_h;
                    }
                }
                if let Some(max) = window
                    .common
                    .current_window_state()
                    .size
                    .max_dimensions
                    .into_option()
                {
                    unsafe {
                        (*mmi).pt_max_track_size.x = (max.width * hf).round() as i32 + frame_w;
                        (*mmi).pt_max_track_size.y = (max.height * hf).round() as i32 + frame_h;
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
            window.snapshot_window_state_baseline("windows.wm_settingchange");
            let new_style = std::sync::Arc::new(crate::desktop::app::discover_system_style());
            let new_theme = match new_style.theme {
                azul_css::system::Theme::Dark => azul_core::window::WindowTheme::DarkMode,
                azul_css::system::Theme::Light => azul_core::window::WindowTheme::LightMode,
            };
            // OS-reported (source = Os): the theme is already the system's, so
            // the OS-sync baseline advances with `current` and only the event
            // diff carries the transition.
            window.common.update_window_state(
                crate::desktop::shell2::common::event::WindowStateSource::Os,
                |ws| ws.theme = new_theme,
            );
            let r = window.process_window_events(0);
            window.route_main_window_result(hwnd, r);
            // Full rebuild or restyle, decided from what the app's `layout()`
            // declared it reads — see `PlatformWindow::adopt_system_style`.
            // The rebuild is tagged ThemeChange, not RefreshDom: the reason
            // reaches the user's layout callback via
            // LayoutCallbackInfo::relayout_reason(), and a theme switch is
            // exactly the case where a callback wants to know it may re-read
            // system colours rather than assume a generic refresh.
            //
            // WM_SETTINGCHANGE is also the arm that fires for settings this
            // window does not care about at all — it is broadcast for
            // environment and policy changes too — so the equality check
            // inside `adopt_system_style` is what keeps those free.
            window.adopt_system_style(new_style);
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
                    .current_window_state()
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

        // Body shared with every other backend
        // (`PlatformWindow::dispatch_accessibility_actions`): apply each action,
        // mark the display list dirty for a non-empty affected set, dispatch the
        // callbacks it mapped to and honour the `Update` they return. This used
        // to be a hand-copy per backend, and a hand-copy is how the callback
        // dispatch went missing in the first place — Narrator could Invoke a
        // button and no `on_click` ran.
        use crate::desktop::shell2::common::event::PlatformWindow as _;
        self.dispatch_accessibility_actions(actions);
        self.request_redraw();
    }

    pub fn is_open(&self) -> bool {
        self.is_open
    }

    /// Release the GPU-side resources of this window.
    ///
    /// WebRender's `Renderer` must be `deinit()`'d, not dropped — texture
    /// deletion has to happen inside a frame. Never doing so crashed debug
    /// builds on close and leaked GPU resources in release.
    ///
    /// `deinit()` issues real GL calls, so OUR context must be current: with
    /// several windows open, whichever window rendered last left ITS context
    /// current, and deinit would then delete textures/programs in the wrong
    /// context (cross-context corruption + leaking the real resources). Same
    /// reasoning, and the same prologue, as `X11Window::close`.
    ///
    /// Must run while the HWND and the GL context are still alive, i.e. before
    /// `DestroyWindow`. `deinit_renderer()` takes the renderer out of the
    /// common state, so calling this twice is a no-op.
    ///
    /// `pub(crate)` because the WM_QUIT exit in `shell2::run` tears its windows
    /// down without a WM_CLOSE/WM_DESTROY ever running.
    pub(crate) fn release_gpu_resources(&mut self) {
        if let RenderMode::Gpu {
            gl_context: hglrc,
            hdc: stored_hdc,
        } = &self.render_mode
        {
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
        }
        self.common.deinit_renderer();
        if let Some(doc_id) = self.common.document_id {
            crate::desktop::gl_texture_integration::remove_document_textures(&doc_id);
        }
    }

    pub fn close(&mut self) {
        // Request the close through WM_CLOSE, which is VETO-ABLE: the close
        // callback can cancel it. Tearing the renderer down here (what this
        // used to do, before the message was even posted) left a cancelled
        // close with a live, visible window and no renderer — every later
        // frame had nothing to render with. The WM_CLOSE handler releases the
        // GPU resources on the path where the close actually proceeds, and
        // WM_DESTROY clears `is_open`.
        unsafe {
            const WM_CLOSE: u32 = 0x0010;
            (self.win32.user32.PostMessageW)(self.hwnd, WM_CLOSE, 0, 0);
        }
    }

    pub fn request_redraw(&mut self) {
        // Use per-rect damage when available (reduces compositor work)
        if !self.gpu_damage_rects.is_empty() {
            let dpi = self.common.current_window_state().size.dpi as f32 / 96.0;
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
    fn capture_screen_for_eyedropper(&mut self) -> Option<crate::desktop::eyedropper::Screenshot> {
        crate::desktop::eyedropper::windows::capture(self)
    }

    /// `SetWindowRgn` with the union of the alpha-shape rects (window-client
    /// coordinates: the DIB covers the client area of a borderless popup;
    /// a decorated window's region is offset by its frame, which a shaped
    /// window does not have). The region's ownership passes to the system.
    fn apply_window_shape(&mut self, rects: &[azul_layout::cpurender::ShapeRect]) {
        const RGN_OR: i32 = 2;
        if rects.is_empty() {
            return;
        }
        unsafe {
            let gdi32 = &self.win32.gdi32;
            let region = (gdi32.CreateRectRgn)(0, 0, 0, 0);
            if region.is_null() {
                return;
            }
            #[allow(clippy::cast_possible_wrap)] // pixel coordinates
            for r in rects {
                let piece = (gdi32.CreateRectRgn)(
                    r.x as i32,
                    r.y as i32,
                    (r.x + r.width) as i32,
                    (r.y + r.height) as i32,
                );
                if piece.is_null() {
                    continue;
                }
                (gdi32.CombineRgn)(region, region, piece, RGN_OR);
                (gdi32.DeleteObject)(piece);
            }
            // Ownership of `region` passes to the window; no redraw needed,
            // the frame that produced the shape was just presented.
            (self.win32.user32.SetWindowRgn)(self.hwnd, region, 0);
        }
    }

    fn regenerate_layout_once(
        &mut self,
    ) -> Result<crate::desktop::shell2::common::layout::LayoutRegenerateResult, String> {
        // The single pass. The bounded lifecycle loop lives in the trait
        // default `regenerate_layout`, which is what frame paths call.
        self.regenerate_layout_inner()
    }

    impl_platform_window_getters!(common);

    fn get_raw_window_handle(&self) -> RawWindowHandle {
        RawWindowHandle::Windows(WindowsHandle {
            hwnd: self.hwnd as *mut std::ffi::c_void,
            hinstance: self.hinstance as *mut std::ffi::c_void,
        })
    }

    fn prepare_callback_invocation(&mut self) -> event::InvokeSingleCallbackBorrows {
        let borrows = self.common.layout_borrows();

        event::InvokeSingleCallbackBorrows {
            layout_window: borrows
                .layout_window
                .expect("Layout window must exist for callback invocation"),
            window_handle: RawWindowHandle::Windows(WindowsHandle {
                hwnd: self.hwnd as *mut std::ffi::c_void,
                hinstance: self.hinstance as *mut std::ffi::c_void,
            }),
            gl_context_ptr: borrows.gl_context_ptr,
            fc_cache_clone: (**borrows.fc_cache).clone(),
            system_style: borrows.system_style.clone(),
            previous_window_state: borrows.previous_window_state,
            current_window_state: borrows.current_window_state,
            renderer_resources: borrows.renderer_resources,
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
                .and_then(|lw| lw.a11y_manager.take_pending());
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
        // Stop Win32 timer. KillTimer must be passed the SAME nIDEvent given
        // to SetTimer for window timers — for hwnd != NULL, SetTimer's return
        // value is only documented as "a nonzero integer", NOT the timer id,
        // so killing by the stored return value relied on an implementation
        // detail (a mismatch leaves the timer firing forever).
        if self.timers.remove(&timer_id).is_some() {
            unsafe {
                (self.win32.user32.KillTimer)(self.hwnd, timer_id);
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
        // Same nIDEvent contract as stop_timer: kill by the id we registered
        // (THREAD_POLL_TIMER_ID), not by SetTimer's return value.
        if self.thread_timer_running.take().is_some() {
            unsafe {
                (self.win32.user32.KillTimer)(self.hwnd, Self::THREAD_POLL_TIMER_ID);
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

    fn request_regeneration_all_windows(&mut self) {
        let hwnd = self.hwnd;
        for other_hwnd in registry::get_all_window_handles() {
            if other_hwnd == hwnd {
                continue;
            }
            if let Some(wptr) = registry::get_window(other_hwnd) {
                let w = unsafe { &mut *wptr };
                w.common
                    .request_regeneration(azul_core::callbacks::RelayoutReason::RefreshDom);
                unsafe {
                    (w.win32.user32.InvalidateRect)(other_hwnd, ptr::null(), 0);
                }
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
        anchor: Option<azul_core::geom::LogicalRect>,
    ) {
        // Check if native menus are enabled
        if self
            .common
            .current_window_state()
            .flags
            .use_native_context_menus
        {
            // Show native Win32 menu. Win32's TrackPopupMenu sizes itself
            // to its items and offers no minimum width short of owner-draw,
            // so `anchor` only reaches the fallback path here.
            self.show_native_menu_at_position(menu, position);
        } else {
            // Show fallback DOM-based menu
            self.show_fallback_menu(menu, position, anchor);
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
    /// Park a built menu for presentation on a later turn of the message loop.
    ///
    /// Replacing an already-parked menu destroys it first, so a second
    /// right-click before the first was presented cannot leak an HMENU.
    fn park_native_menu(&mut self, hmenu: HMENU, screen_x: i32, screen_y: i32) {
        if let Some(stale) = self.pending_native_menu.take() {
            unsafe {
                (self.win32.user32.DestroyMenu)(stale.hmenu);
            }
        }
        self.pending_native_menu = Some(PendingNativeMenu {
            hmenu,
            x: screen_x,
            y: screen_y,
        });
        unsafe {
            (self.win32.user32.PostMessageW)(self.hwnd, WM_APP_SHOW_PENDING_MENU, 0, 0);
        }
    }

    /// Show a native Win32 popup menu at the given logical position using `TrackPopupMenu`.
    fn show_native_menu_at_position(
        &mut self,
        menu: &azul_core::menu::Menu,
        position: azul_core::geom::LogicalPosition,
    ) {
        let mut hmenu = unsafe { (self.win32.user32.CreatePopupMenu)() };
        if hmenu.is_null() {
            // PRE-EXISTING: `show_fallback_menu` gained an `anchor` in
            // 35de92bbe (drop-down width follows its control) and this call
            // site was missed, because nothing has compiled this target since.
            // `None` is the correct value here: this is the fallback path,
            // which has no control to anchor to.
            self.show_fallback_menu(menu, position, None);
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

        // PARK, do not track: TrackPopupMenu is a modal loop and `&mut self`
        // is live all the way up to window_proc.
        self.park_native_menu(hmenu, pt.x, pt.y);
    }

    /// Show a fallback window-based menu at the given position
    fn show_fallback_menu(
        &mut self,
        menu: &azul_core::menu::Menu,
        position: azul_core::geom::LogicalPosition,
        anchor: Option<azul_core::geom::LogicalRect>,
    ) {
        // Get parent window position
        let parent_pos = match self.common.current_window_state().position {
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
            anchor,         // The node the menu was opened for (drives min-width)
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
        match (*wptr).common.current_window_state().position {
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
            // No explicit position - center on target monitor.
            // Monitor geometry is PHYSICAL px; dimensions are LOGICAL —
            // scale, or centering drifts right/down on scaled monitors.
            let hf = size.get_hidpi_factor().inner.get();
            let window_width = libm::roundf(size.dimensions.width * hf) as isize;
            let window_height = libm::roundf(size.dimensions.height * hf) as isize;

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
                    use dlopen::{
                        CANDIDATEFORM, CFS_CANDIDATEPOS, CFS_RECT, COMPOSITIONFORM, POINT, RECT,
                    };

                    // rect is LOGICAL (cursor rect from layout);
                    // COMPOSITIONFORM takes CLIENT-AREA coordinates in
                    // PHYSICAL px — unscaled, the IME candidate window
                    // drifted toward the top-left of the caret on any
                    // scaled monitor (off by 1.5x/2x the caret offset).
                    let hf = self
                        .common
                        .current_window_state()
                        .size
                        .get_hidpi_factor()
                        .inner
                        .get();
                    let left = libm::roundf(rect.origin.x * hf) as i32;
                    let top = libm::roundf(rect.origin.y * hf) as i32;
                    let right = libm::roundf((rect.origin.x + rect.size.width) * hf) as i32;
                    let bottom = libm::roundf((rect.origin.y + rect.size.height) * hf) as i32;
                    let comp_form = COMPOSITIONFORM {
                        dwStyle: CFS_RECT,
                        ptCurrentPos: POINT { x: left, y: top },
                        rcArea: RECT {
                            left,
                            top,
                            right,
                            bottom,
                        },
                    };

                    (imm32.ImmSetCompositionWindow)(himc, &comp_form);

                    // The CANDIDATE list is a separate form: without this it
                    // stays wherever the IME first opened it while the caret
                    // moves on. CFS_CANDIDATEPOS places its top-left corner,
                    // so anchor it under the caret's BOTTOM-left.
                    let cand_form = CANDIDATEFORM {
                        dwIndex: 0,
                        dwStyle: CFS_CANDIDATEPOS,
                        ptCurrentPos: POINT { x: left, y: bottom },
                        rcArea: RECT::default(),
                    };
                    (imm32.ImmSetCandidateWindow)(himc, &cand_form);

                    (imm32.ImmReleaseContext)(hwnd, himc);
                }
            }
        }
    }

    /// Sync ime_position from window state to OS
    /// MWA-C-text_input: associate/dissociate the window's IME context per
    /// editable focus (Wayland/macOS gate their IME the same way) — the
    /// context was always associated, so the IME candidate window could
    /// activate while nothing editable was focused.
    pub fn sync_ime_enabled_state(&mut self) {
        let Some(ref imm32) = self.win32.imm32 else {
            return;
        };
        let want = self
            .common
            .layout_window
            .as_ref()
            .is_some_and(|lw| lw.text_edit_manager.has_active_editing());
        if want == self.ime_enabled {
            return;
        }
        unsafe {
            if want {
                // Re-associate the context we saved when disabling.
                (imm32.ImmAssociateContext)(self.hwnd, self.ime_saved_himc);
                self.ime_saved_himc = std::ptr::null_mut();
            } else {
                self.ime_saved_himc = (imm32.ImmAssociateContext)(self.hwnd, std::ptr::null_mut());
            }
        }
        self.ime_enabled = want;
    }

    pub fn sync_ime_position_to_os(&self) {
        use azul_core::window::ImePosition;

        if let ImePosition::Initialized(rect) = self.common.current_window_state().ime_position {
            self.set_ime_composition_window(rect);
        }
    }

    /// Keep the OS-side IME in step with the LIVE focus / editing / caret state.
    ///
    /// The three calls this replaces had exactly ONE call site between them —
    /// the tail of a FULL `regenerate_layout_inner`. Clicking into a
    /// contenteditable produces a focus restyle, an INCREMENTAL relayout and a
    /// re-render, none of which run that tail, so the IME context stayed
    /// dissociated and a CJK input method never engaged at all; and while
    /// typing, the relayout-only frame path never moved the composition or
    /// candidate window, so they kept drawing at the caret's position from
    /// whenever the last full regeneration happened.
    ///
    /// Called from every event pass (`route_main_window_result`), every frame
    /// (`WM_PAINT`) and the regeneration tail. The association diffs internally
    /// against `ime_enabled`; the caret-rect recompute — a walk of the layout
    /// tree — is gated on [`event::ImeSyncKey`] actually having changed. The
    /// X11 backend's `sync_ime_state`, same contract.
    pub(crate) fn sync_ime_state(&mut self) {
        let key = event::ime_sync_key(
            self.common.current_window_state().window_focused,
            self.common.layout_window.as_ref(),
        );

        // Cheap: diffs against `ime_enabled` internally.
        self.sync_ime_enabled_state();

        if key != self.ime_sync_key {
            self.ime_sync_key = key;
            self.update_ime_position_from_cursor();
            self.sync_ime_position_to_os();
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

    /// `SetTimer` with an nIDEvent that is already in use REPLACES that timer.
    /// The modal size/move pump sharing the thread-poll id would therefore kill
    /// background-thread polling for the rest of the run, silently — and the
    /// `KillTimer` at `WM_EXITSIZEMOVE` would never bring it back.
    #[test]
    fn reserved_win32_timer_ids_do_not_collide() {
        assert_ne!(
            Win32Window::MODAL_LOOP_TIMER_ID,
            Win32Window::THREAD_POLL_TIMER_ID,
            "the modal size/move pump and the thread poll need separate timers"
        );
    }

    /// Buttons 4/5 arrive with the button in the HIGH word of wParam and the
    /// modifier keys in the low one. This is the mapping the `WM_XBUTTON*` arm
    /// runs; it lives in `common` so it is also tested on hosts that cannot
    /// compile this module at all.
    #[test]
    fn xbutton_wparam_names_the_thumb_buttons() {
        use crate::desktop::shell2::common::event::win32_xbutton_to_mouse_button;
        use azul_core::events::MouseButton;

        assert_eq!(
            win32_xbutton_to_mouse_button(0x0001 << 16),
            Some(MouseButton::Other(3))
        );
        assert_eq!(
            win32_xbutton_to_mouse_button(0x0002 << 16),
            Some(MouseButton::Other(4))
        );
    }
}

/// Build an `HICON` from straight-alpha RGBA8, or `None` if GDI refuses.
///
/// Three traps, each of which silently produces a wrong icon rather than an
/// error:
///
/// 1. **`CreateIconIndirect` wants STRAIGHT alpha**, unlike `AlphaBlend` /
///    `UpdateLayeredWindow` which want premultiplied. Feeding it premultiplied
///    pixels gives dark fringes on every antialiased edge.
/// 2. **Channel order is B,G,R,A**, not the R,G,B,A we are handed.
/// 3. **Rows are bottom-up unless the height is NEGATIVE.** A positive height
///    with top-down data yields a vertically mirrored icon, which reads as
///    "wrong icon" rather than as a bug.
///
/// The 1bpp AND mask is required by `ICONINFO` even though a 32bpp colour
/// bitmap blends by its own alpha; all-zero means "draw every pixel".
///
/// Both bitmaps are owned by US - `CreateIconIndirect` copies them - so they are
/// deleted before returning, or every icon update leaks two GDI objects.
unsafe fn hicon_from_rgba(
    win32: &dlopen::Win32Libraries,
    rgba: &[u8],
    width: u32,
    height: u32,
) -> Option<dlopen::HICON> {
    use dlopen::IconInfo;

    let (w, h) = (width as usize, height as usize);
    if w == 0 || h == 0 || rgba.len() < w * h * 4 {
        return None;
    }

    // BGRA, straight alpha.
    let mut bgra = alloc::vec::Vec::<u8>::with_capacity(w * h * 4);
    for px in rgba[..w * h * 4].chunks_exact(4) {
        bgra.extend_from_slice(&[px[2], px[1], px[0], px[3]]);
    }

    let header = dlopen::BitmapInfoHeader {
        biSize: core::mem::size_of::<dlopen::BitmapInfoHeader>() as u32,
        biWidth: width as i32,
        // NEGATIVE: our rows are top-down. See trap 3.
        biHeight: -(height as i32),
        biPlanes: 1,
        biBitCount: 32,
        biCompression: 0, // BI_RGB
        biSizeImage: 0,
        biXPelsPerMeter: 0,
        biYPelsPerMeter: 0,
        biClrUsed: 0,
        biClrImportant: 0,
    };

    let mut bits: *mut core::ffi::c_void = core::ptr::null_mut();
    let color = (win32.gdi32.CreateDIBSection)(
        core::ptr::null_mut(),
        &header,
        0, // DIB_RGB_COLORS
        &mut bits,
        core::ptr::null_mut(),
        0,
    );
    if color.is_null() || bits.is_null() {
        if !color.is_null() {
            (win32.gdi32.DeleteObject)(color);
        }
        return None;
    }
    core::ptr::copy_nonoverlapping(bgra.as_ptr(), bits.cast::<u8>(), bgra.len());

    // 1bpp AND mask, all zero.
    let mask_stride = ((w + 31) / 32) * 4; // 1bpp rows are DWORD-aligned
    let mask_bits = alloc::vec![0u8; mask_stride * h];
    let mask =
        (win32.gdi32.CreateBitmap)(width as i32, height as i32, 1, 1, mask_bits.as_ptr().cast());
    if mask.is_null() {
        (win32.gdi32.DeleteObject)(color);
        return None;
    }

    let info = IconInfo {
        fIcon: 1,
        xHotspot: 0,
        yHotspot: 0,
        hbmMask: mask,
        hbmColor: color,
    };
    let icon = (win32.user32.CreateIconIndirect)(&info);

    // CreateIconIndirect COPIES both bitmaps; ours leak otherwise.
    (win32.gdi32.DeleteObject)(color);
    (win32.gdi32.DeleteObject)(mask);

    if icon.is_null() {
        None
    } else {
        Some(icon)
    }
}

/// Apply `WindowsWindowOptions::{window_icon, taskbar_icon}` to a live HWND.
///
/// Both fields were public API that NO backend read - setting them did nothing,
/// silently.
///
/// `WM_SETICON` has exactly two settable slots: `ICON_SMALL` (title bar, Alt+Tab)
/// and `ICON_BIG` (the taskbar button of a running, un-pinned window).
/// `ICON_SMALL2` is NOT settable - Windows derives it - so there is no third
/// call to make. A PINNED taskbar entry shows the shortcut's icon and is not
/// ours to change, and the EXE resource icon cannot be rewritten while the
/// process is running at all.
///
/// `WM_SETICON` does not take ownership and RETURNS the previous icon; that one
/// is destroyed here so repeated updates do not leak.
unsafe fn apply_window_icons(
    win32: &dlopen::Win32Libraries,
    hwnd: HWND,
    options: &azul_core::window::WindowsWindowOptions,
) {
    use azul_core::window::WindowIcon;

    const WM_SETICON: u32 = 0x0080;
    const ICON_SMALL: usize = 0;
    const ICON_BIG: usize = 1;

    let mut set = |slot: usize, rgba: &[u8], w: u32, h: u32| {
        if let Some(icon) = hicon_from_rgba(win32, rgba, w, h) {
            let previous = (win32.user32.SendMessageW)(hwnd, WM_SETICON, slot, icon as isize);
            if previous != 0 {
                (win32.user32.DestroyIcon)(previous as dlopen::HICON);
            }
        }
    };

    if let Some(icon) = options.window_icon.as_ref() {
        match icon {
            WindowIcon::Small(i) => set(ICON_SMALL, i.rgba_bytes.as_ref(), 16, 16),
            WindowIcon::Large(i) => set(ICON_BIG, i.rgba_bytes.as_ref(), 32, 32),
        }
    }
    if let Some(t) = options.taskbar_icon.as_ref() {
        set(ICON_BIG, t.rgba_bytes.as_ref(), 256, 256);
    }
}
