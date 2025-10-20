//! macOS implementation using AppKit/Cocoa.
//!
//! This module implements the PlatformWindow trait for macOS using:
//! - NSWindow for window management
//! - NSOpenGLContext for GPU rendering (optional)
//! - NSMenu for menu bar and context menus
//! - NSEvent for event handling
//!
//! Note: macOS uses static linking for system frameworks (standard approach).

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use azul_core::menu::Menu;
use azul_layout::window_state::{FullWindowState, WindowCreateOptions, WindowState};
use objc2::{
    define_class,
    msg_send_id,
    rc::{Allocated, Retained},
    runtime::ProtocolObject,
    AnyThread, // For alloc() method
    ClassType,
    DeclaredClass,
    MainThreadMarker,
    MainThreadOnly,
};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSBackingStoreType,
    NSBitmapImageRep, NSColor, NSCompositingOperation, NSEvent, NSEventMask, NSEventType, NSImage,
    NSMenu, NSMenuItem, NSOpenGLContext, NSOpenGLPixelFormat, NSOpenGLPixelFormatAttribute,
    NSOpenGLView, NSResponder, NSScreen, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{
    ns_string, NSData, NSNotification, NSObject, NSPoint, NSRect, NSSize, NSString,
};

use crate::desktop::{
    shell2::common::{
        Compositor, CompositorError, CompositorMode, PlatformWindow, RenderContext, WindowError,
        WindowProperties,
    },
    wr_translate2::{
        default_renderer_options, translate_document_id_wr, translate_id_namespace_wr,
        wr_translate_document_id, AsyncHitTester, Compositor as WrCompositor, Notifier,
        WR_SHADER_CACHE,
    },
};

mod events;
mod gl;
mod menu;

use gl::GlFunctions;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum RenderBackend {
    OpenGL,
    CPU,
}

// ============================================================================
// GLView - OpenGL rendering view
// ============================================================================

/// Instance variables for GLView
pub struct GLViewIvars {
    gl_functions: RefCell<Option<Rc<gl_context_loader::GenericGlContext>>>,
    needs_reshape: Cell<bool>,
    mtm: MainThreadMarker, // Store MainThreadMarker to avoid unsafe new_unchecked
}

define_class!(
    #[unsafe(super(NSOpenGLView, NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "AzulGLView"]
    #[ivars = GLViewIvars]
    pub struct GLView;

    impl GLView {
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _rect: NSRect) {
            // Get GL functions from ivars
            if let Some(ref gl_context) = *self.ivars().gl_functions.borrow() {
                unsafe {
                    // Cast function pointers to proper types
                    type GlClearColorFn = unsafe extern "C" fn(f32, f32, f32, f32);
                    type GlClearFn = unsafe extern "C" fn(u32);

                    // Clear to blue color (0.2, 0.3, 0.8, 1.0)
                    if !gl_context.glClearColor.is_null() {
                        let clear_color: GlClearColorFn = std::mem::transmute(gl_context.glClearColor);
                        clear_color(0.2, 0.3, 0.8, 1.0);
                    }

                    // Clear color buffer (GL_COLOR_BUFFER_BIT = 0x00004000)
                    if !gl_context.glClear.is_null() {
                        let clear: GlClearFn = std::mem::transmute(gl_context.glClear);
                        clear(0x00004000);
                    }
                }
            }

            // Flush buffer
            unsafe {
                if let Some(context) = self.openGLContext() {
                    context.flushBuffer();
                }
            }
        }

        #[unsafe(method(prepareOpenGL))]
        fn prepare_opengl(&self) {
            // Load GL functions via dlopen
            match GlFunctions::initialize() {
                Ok(functions) => {
                    *self.ivars().gl_functions.borrow_mut() = Some(functions.get_context());
                    self.ivars().needs_reshape.set(true);
                }
                Err(e) => {
                    eprintln!("Failed to load GL functions: {}", e);
                }
            }
        }

        #[unsafe(method(reshape))]
        fn reshape(&self) {
            let mtm = self.ivars().mtm;

            // Update context
            unsafe {
                if let Some(context) = self.openGLContext() {
                    context.update(mtm);
                }
            }

            // Update viewport
            let bounds = unsafe { self.bounds() };
            let width = bounds.size.width as i32;
            let height = bounds.size.height as i32;

            if let Some(ref gl_context) = *self.ivars().gl_functions.borrow() {
                unsafe {
                    // Cast function pointer to proper type
                    type GlViewportFn = unsafe extern "C" fn(i32, i32, i32, i32);

                    if !gl_context.glViewport.is_null() {
                        let viewport: GlViewportFn = std::mem::transmute(gl_context.glViewport);
                        viewport(0, 0, width, height);
                    }
                }
            }

            self.ivars().needs_reshape.set(false);
        }

        // ===== Event Handling =====

        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            // Event will be handled by MacOSWindow via NSApplication event loop
            // This method is required for the view to accept mouse events
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method(rightMouseUp:))]
        fn right_mouse_up(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method(keyUp:))]
        fn key_up(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method(flagsChanged:))]
        fn flags_changed(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method_id(initWithFrame:pixelFormat:))]
        fn init_with_frame_pixel_format(
            this: Allocated<Self>,
            frame: NSRect,
            pixel_format: Option<&NSOpenGLPixelFormat>,
        ) -> Option<Retained<Self>> {
            // Get MainThreadMarker - we're guaranteed to be on main thread in init
            let mtm = MainThreadMarker::new().expect("init must be called on main thread");

            let this = this.set_ivars(GLViewIvars {
                gl_functions: RefCell::new(None),
                needs_reshape: Cell::new(true),
                mtm,
            });
            unsafe {
                msg_send_id![super(this), initWithFrame: frame, pixelFormat: pixel_format]
            }
        }
    }
);

// ============================================================================
// CPUView - CPU rendering view
// ============================================================================

/// Instance variables for CPUView
pub struct CPUViewIvars {
    framebuffer: RefCell<Vec<u8>>,
    width: Cell<usize>,
    height: Cell<usize>,
    needs_redraw: Cell<bool>,
    mtm: MainThreadMarker, // Store MainThreadMarker to avoid unsafe new_unchecked
}

define_class!(
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "AzulCPUView"]
    #[ivars = CPUViewIvars]
    pub struct CPUView;

    impl CPUView {
        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty_rect: NSRect) {
            let bounds = unsafe { self.bounds() };
            let width = bounds.size.width as usize;
            let height = bounds.size.height as usize;

            let ivars = self.ivars();

            // Resize framebuffer if needed
            let current_width = ivars.width.get();
            let current_height = ivars.height.get();

            if current_width != width || current_height != height {
                ivars.width.set(width);
                ivars.height.set(height);
                ivars.framebuffer.borrow_mut().resize(width * height * 4, 0);
            }

            // Render blue gradient to framebuffer
            {
                let mut framebuffer = ivars.framebuffer.borrow_mut();
                for y in 0..height {
                    for x in 0..width {
                        let idx = (y * width + x) * 4;
                        framebuffer[idx] = (x * 128 / width.max(1)) as u8; // R
                        framebuffer[idx + 1] = (y * 128 / height.max(1)) as u8; // G
                        framebuffer[idx + 2] = 255; // B - Blue
                        framebuffer[idx + 3] = 255; // A
                    }
                }
            }

            // Blit framebuffer to window
            unsafe {
                let mtm = ivars.mtm; // Get mtm from ivars
                let framebuffer = ivars.framebuffer.borrow();

                // Use NSData::with_bytes to wrap our framebuffer
                let data = NSData::with_bytes(&framebuffer[..]);

                if let Some(bitmap) = NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
                    NSBitmapImageRep::alloc(),
                    std::ptr::null_mut(),
                    width as isize,
                    height as isize,
                    8,
                    4,
                    true,
                    false,
                    ns_string!("NSCalibratedRGBColorSpace"),
                    (width * 4) as isize,
                    32,
                ) {
                    // Copy framebuffer data to bitmap
                    std::ptr::copy_nonoverlapping(
                        framebuffer.as_ptr(),
                        bitmap.bitmapData(),
                        framebuffer.len(),
                    );

                    // Create image and draw
                    let image = NSImage::initWithSize(NSImage::alloc(), bounds.size);
                    image.addRepresentation(&bitmap);
                    image.drawInRect(bounds);
                }
            }
        }

        #[unsafe(method(isOpaque))]
        fn is_opaque(&self) -> bool {
            true
        }

        // ===== Event Handling =====

        #[unsafe(method(acceptsFirstResponder))]
        fn accepts_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            // Event will be handled by MacOSWindow
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method(rightMouseDown:))]
        fn right_mouse_down(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method(rightMouseUp:))]
        fn right_mouse_up(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method(scrollWheel:))]
        fn scroll_wheel(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method(keyDown:))]
        fn key_down(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method(keyUp:))]
        fn key_up(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method(flagsChanged:))]
        fn flags_changed(&self, event: &NSEvent) {
            // Event handled by MacOSWindow
        }

        #[unsafe(method_id(initWithFrame:))]
        fn init_with_frame(
            this: Allocated<Self>,
            frame: NSRect,
        ) -> Option<Retained<Self>> {
            // Get MainThreadMarker - we're guaranteed to be on main thread in init
            let mtm = MainThreadMarker::new().expect("init must be called on main thread");

            let this = this.set_ivars(CPUViewIvars {
                framebuffer: RefCell::new(Vec::new()),
                width: Cell::new(0),
                height: Cell::new(0),
                needs_redraw: Cell::new(true),
                mtm,
            });
            unsafe {
                msg_send_id![super(this), initWithFrame: frame]
            }
        }
    }
);

// ============================================================================
// Helper Functions
// ============================================================================

/// Create OpenGL pixel format with proper attributes
fn create_opengl_pixel_format(
    mtm: MainThreadMarker,
) -> Result<Retained<NSOpenGLPixelFormat>, WindowError> {
    // OpenGL 3.2 Core Profile attributes
    let attrs: Vec<u32> = vec![
        5, // NSOpenGLPFADoubleBuffer
        12, 24, // NSOpenGLPFADepthSize(24)
        99, 0x3200, // NSOpenGLPFAOpenGLProfile(3.2 Core)
        8, 24, // NSOpenGLPFAColorSize(24)
        11, 8,  // NSOpenGLPFAAlphaSize(8)
        73, // NSOpenGLPFAAccelerated
        0,  // Null terminator
    ];

    // Note: NSOpenGLPixelFormat::initWithAttributes expects NonNull<u32> in objc2-app-kit 0.3.2
    unsafe {
        let attrs_ptr = std::ptr::NonNull::new_unchecked(attrs.as_ptr() as *mut u32);
        NSOpenGLPixelFormat::initWithAttributes(NSOpenGLPixelFormat::alloc(), attrs_ptr)
            .ok_or_else(|| WindowError::ContextCreationFailed)
    }
}

// ============================================================================
// MacOSWindow - Main window implementation
// ============================================================================

/// macOS window implementation with dual rendering backend support
pub struct MacOSWindow {
    /// The NSWindow instance
    window: Retained<NSWindow>,

    /// Selected rendering backend
    backend: RenderBackend,

    /// OpenGL rendering components (if backend == OpenGL)
    gl_view: Option<Retained<GLView>>,
    gl_context: Option<Retained<NSOpenGLContext>>,
    gl_functions: Option<Rc<GlFunctions>>,

    /// CPU rendering components (if backend == CPU)
    cpu_view: Option<Retained<CPUView>>,

    /// Window is open flag
    is_open: bool,

    /// Main thread marker (required for AppKit)
    mtm: MainThreadMarker,

    /// Window state from previous frame (for diff detection)
    previous_window_state: Option<FullWindowState>,

    /// Current window state
    current_window_state: FullWindowState,

    /// Last hovered node (for hover state tracking)
    last_hovered_node: Option<events::HitTestNode>,

    /// LayoutWindow integration (for UI callbacks and display list)
    layout_window: Option<azul_layout::window::LayoutWindow>,

    /// Menu state (for hash-based diff updates)
    menu_state: menu::MenuState,

    // Resource caches for LayoutWindow
    /// Image cache for texture management
    image_cache: azul_core::resources::ImageCache,

    /// Renderer resources (GPU textures, etc.)
    renderer_resources: azul_core::resources::RendererResources,

    // WebRender infrastructure for proper hit-testing and rendering
    /// Main render API for registering fonts, images, display lists
    pub(crate) render_api: webrender::RenderApi,

    /// WebRender renderer (software or hardware depending on backend)
    pub(crate) renderer: Option<webrender::Renderer>,

    /// Hit-tester for fast asynchronous hit-testing (updated on layout changes)
    pub(crate) hit_tester: crate::desktop::wr_translate2::AsyncHitTester,

    /// WebRender document ID
    pub(crate) document_id: azul_core::hit_test::DocumentId,

    /// WebRender ID namespace
    pub(crate) id_namespace: azul_core::resources::IdNamespace,

    /// OpenGL context pointer with compiled SVG and FXAA shaders
    pub(crate) gl_context_ptr: azul_core::gl::OptionGlContextPtr,
}

impl MacOSWindow {
    /// Determine which rendering backend to use
    fn determine_backend(options: &WindowCreateOptions) -> RenderBackend {
        // 1. Check environment variable override
        if let Ok(val) = std::env::var("AZUL_RENDERER") {
            match val.to_lowercase().as_str() {
                "cpu" => return RenderBackend::CPU,
                "opengl" | "gl" => return RenderBackend::OpenGL,
                _ => {}
            }
        }

        // 2. Check options.renderer - if it's Some, check hw_accel field
        use azul_core::window::{HwAcceleration, OptionRendererOptions};
        if let Some(renderer) = options.renderer.as_option() {
            match renderer.hw_accel {
                HwAcceleration::Disabled => return RenderBackend::CPU,
                HwAcceleration::Enabled => return RenderBackend::OpenGL,
                HwAcceleration::DontCare => {} // Continue to default
            }
        }

        // 3. Default: Try OpenGL
        RenderBackend::OpenGL
    }

    /// Create OpenGL view with context and functions
    fn create_gl_view(
        frame: NSRect,
        mtm: MainThreadMarker,
    ) -> Result<(Retained<GLView>, Retained<NSOpenGLContext>, Rc<GlFunctions>), WindowError> {
        // Create pixel format
        let pixel_format = create_opengl_pixel_format(mtm)?;

        // Create GLView
        let gl_view: Option<Retained<GLView>> = unsafe {
            msg_send_id![
                GLView::alloc(mtm),
                initWithFrame: frame,
                pixelFormat: &*pixel_format,
            ]
        };

        let gl_view =
            gl_view.ok_or_else(|| WindowError::PlatformError("Failed to create GLView".into()))?;

        // Get OpenGL context
        let gl_context =
            unsafe { gl_view.openGLContext() }.ok_or_else(|| WindowError::ContextCreationFailed)?;

        // Load GL functions
        let gl_functions = GlFunctions::initialize()
            .map_err(|e| WindowError::PlatformError(format!("Failed to load GL: {}", e).into()))?;

        Ok((gl_view, gl_context, Rc::new(gl_functions)))
    }

    /// Create CPU view
    fn create_cpu_view(frame: NSRect, mtm: MainThreadMarker) -> Retained<CPUView> {
        let view: Option<Retained<CPUView>> =
            unsafe { msg_send_id![CPUView::alloc(mtm), initWithFrame: frame] };
        view.expect("Failed to create CPUView")
    }

    /// Create a new macOS window with given options.
    pub fn new_with_options(
        options: WindowCreateOptions,
        mtm: MainThreadMarker,
    ) -> Result<Self, WindowError> {
        // Initialize NSApplication if needed
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Regular);

        // Get screen dimensions for window positioning
        let screen = NSScreen::mainScreen(mtm)
            .ok_or_else(|| WindowError::PlatformError("No main screen".into()))?;
        let screen_frame = screen.frame();

        // Determine window size from options
        let window_size = options.state.size.dimensions;
        let width = window_size.width as f64;
        let height = window_size.height as f64;

        // Center window on screen
        let x = (screen_frame.size.width - width) / 2.0;
        let y = (screen_frame.size.height - height) / 2.0;

        let content_rect = NSRect::new(NSPoint::new(x, y), NSSize::new(width, height));

        // Determine rendering backend
        let requested_backend = Self::determine_backend(&options);

        // Create content view based on backend
        let (backend, gl_view, gl_context, gl_functions, cpu_view) = match requested_backend {
            RenderBackend::OpenGL => match Self::create_gl_view(content_rect, mtm) {
                Ok((view, ctx, funcs)) => (
                    RenderBackend::OpenGL,
                    Some(view),
                    Some(ctx),
                    Some(funcs),
                    None,
                ),
                Err(e) => {
                    eprintln!("OpenGL initialization failed: {}, falling back to CPU", e);
                    let view = Self::create_cpu_view(content_rect, mtm);
                    (RenderBackend::CPU, None, None, None, Some(view))
                }
            },
            RenderBackend::CPU => {
                let view = Self::create_cpu_view(content_rect, mtm);
                (RenderBackend::CPU, None, None, None, Some(view))
            }
        };

        // Create window style mask
        let style_mask = NSWindowStyleMask::Titled
            | NSWindowStyleMask::Closable
            | NSWindowStyleMask::Miniaturizable
            | NSWindowStyleMask::Resizable;

        // Create the window
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                mtm.alloc(),
                content_rect,
                style_mask,
                NSBackingStoreType::Buffered,
                false,
            )
        };

        // Set window title
        let title = NSString::from_str(&options.state.title);
        window.setTitle(&title);

        // Set content view (either GL or CPU)
        // SAFE: Both GLView and CPUView inherit from NSView, so we can upcast safely
        if let Some(ref gl) = gl_view {
            unsafe {
                // GLView is a subclass of NSView, so we can use it as NSView
                let view_ptr = Retained::as_ptr(gl) as *const NSView;
                let view_ref = &*view_ptr;
                window.setContentView(Some(view_ref));
            }
        } else if let Some(ref cpu) = cpu_view {
            unsafe {
                // CPUView is a subclass of NSView, so we can use it as NSView
                let view_ptr = Retained::as_ptr(cpu) as *const NSView;
                let view_ref = &*view_ptr;
                window.setContentView(Some(view_ref));
            }
        } else {
            return Err(WindowError::PlatformError("No content view created".into()));
        }

        unsafe {
            window.center();
            window.makeKeyAndOrderFront(None);
        }

        // Make OpenGL context current before initializing WebRender
        if let Some(ref ctx) = gl_context {
            unsafe {
                ctx.makeCurrentContext();
            }
        }

        // Initialize WebRender renderer
        use azul_core::window::{HwAcceleration, RendererType};

        let renderer_type = match backend {
            RenderBackend::OpenGL => RendererType::Hardware,
            RenderBackend::CPU => RendererType::Software,
        };

        let gl_funcs = if let Some(ref f) = gl_functions {
            f.functions.clone()
        } else {
            // Fallback for CPU backend - initialize GL functions or fail gracefully
            match gl::GlFunctions::initialize() {
                Ok(f) => f.functions.clone(),
                Err(e) => {
                    return Err(WindowError::PlatformError(format!(
                        "Failed to initialize GL functions: {}",
                        e
                    )));
                }
            }
        };

        let wr_renderer = webrender::Renderer::new(
            gl_funcs.clone(),
            Box::new(Notifier {}),
            default_renderer_options(&options),
            WR_SHADER_CACHE,
        )
        .map_err(|e| {
            WindowError::PlatformError(format!("WebRender initialization failed: {:?}", e))
        })?;

        let (mut renderer, sender) = wr_renderer;
        renderer.set_external_image_handler(Box::new(WrCompositor::default()));

        let mut render_api = sender.create_api();

        // Get physical size for framebuffer
        let physical_size = azul_core::geom::PhysicalSize {
            width: (options.state.size.dimensions.width * options.state.size.get_hidpi_factor())
                as u32,
            height: (options.state.size.dimensions.height * options.state.size.get_hidpi_factor())
                as u32,
        };

        let framebuffer_size = webrender::api::units::DeviceIntSize::new(
            physical_size.width as i32,
            physical_size.height as i32,
        );

        // Create WebRender document (one per window)
        let document_id = translate_document_id_wr(render_api.add_document(framebuffer_size));
        let id_namespace = translate_id_namespace_wr(render_api.get_namespace_id());

        // Request hit tester for this document
        let hit_tester = render_api
            .request_hit_tester(wr_translate_document_id(document_id))
            .resolve();

        // Create GlContextPtr for LayoutWindow
        let gl_context_ptr: azul_core::gl::OptionGlContextPtr = gl_context
            .as_ref()
            .map(|_| azul_core::gl::GlContextPtr::new(renderer_type, gl_funcs.clone()))
            .into();

        // Initialize window state
        let mut current_window_state = FullWindowState {
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
        };

        // Initialize resource caches
        let image_cache = azul_core::resources::ImageCache::default();
        let renderer_resources = azul_core::resources::RendererResources::default();

        // Initialize LayoutWindow (fc_cache will be passed from App later)
        // For now, use a temporary cache that will be replaced on first layout
        let temp_fc_cache = rust_fontconfig::FcFontCache::default();
        let mut layout_window =
            azul_layout::window::LayoutWindow::new(temp_fc_cache).map_err(|e| {
                WindowError::PlatformError(format!("Failed to create LayoutWindow: {:?}", e))
            })?;

        // Set document_id and id_namespace for this window
        layout_window.document_id = document_id;
        layout_window.id_namespace = id_namespace;
        layout_window.current_window_state = current_window_state.clone();
        layout_window.renderer_type = Some(renderer_type);

        // Clear OpenGL context after initialization
        if gl_context.is_some() {
            unsafe {
                use objc2_app_kit::NSOpenGLContext;
                NSOpenGLContext::clearCurrentContext();
            }
        }

        Ok(Self {
            window,
            backend,
            gl_view,
            gl_context,
            gl_functions,
            cpu_view,
            is_open: true,
            mtm,
            previous_window_state: None,
            current_window_state,
            last_hovered_node: None,
            layout_window: Some(layout_window),
            menu_state: menu::MenuState::new(),
            image_cache,
            renderer_resources,
            render_api,
            renderer: Some(renderer),
            hit_tester: AsyncHitTester::Resolved(hit_tester),
            document_id,
            id_namespace,
            gl_context_ptr,
        })
    }

    /// Synchronize window state with the OS based on diff between previous and current state
    /// Regenerate layout and display list for the current window.
    ///
    /// This should be called when:
    /// - The window is resized
    /// - The DOM changes (via callbacks)
    /// - Layout callback changes
    pub fn regenerate_layout(
        &mut self,
        app_data: &mut azul_core::refany::RefAny,
        fc_cache: &mut rust_fontconfig::FcFontCache,
    ) -> Result<(), String> {
        use azul_core::callbacks::LayoutCallback;

        let layout_window = self.layout_window.as_mut().ok_or("No layout window")?;

        // Update layout_window's fc_cache with the shared one from App
        layout_window.font_manager.fc_cache = fc_cache.clone();

        // 1. Call layout_callback to get styled_dom
        let mut callback_info = azul_core::callbacks::LayoutCallbackInfo {
            window_id: 0, // TODO: proper window ID tracking
            window_state: self.current_window_state.clone().into(),
            layout_window_ptr: layout_window as *mut _,
        };

        let styled_dom = match &self.current_window_state.layout_callback {
            LayoutCallback::Raw(inner) => (inner.cb)(app_data, &mut callback_info),
            LayoutCallback::Marshaled(marshaled) => (marshaled.cb.cb)(
                &mut marshaled.marshal_data.clone(),
                app_data,
                &mut callback_info,
            ),
        };

        // 2. Perform layout with solver3
        layout_window
            .layout_and_generate_display_list(
                styled_dom,
                &self.current_window_state,
                &self.renderer_resources,
                &azul_layout::callbacks::ExternalSystemCallbacks::default(),
                &mut None, // No debug messages for now
            )
            .map_err(|e| format!("Layout error: {:?}", e))?;

        // 3. Rebuild display list and send to WebRender (stub for now)
        crate::desktop::wr_translate2::rebuild_display_list(
            layout_window,
            &mut self.render_api,
            &self.image_cache,
            Vec::new(), // No resource updates for now
        );

        // 4. Generate frame
        crate::desktop::wr_translate2::generate_frame(
            layout_window,
            &mut self.render_api,
            true, // Display list was rebuilt
        );

        Ok(())
    }

    /// Perform GPU scrolling - updates scroll transforms without full relayout
    pub fn gpu_scroll(
        &mut self,
        dom_id: u64,
        node_id: u64,
        delta_x: f32,
        delta_y: f32,
    ) -> Result<(), String> {
        use std::time::Duration;

        use azul_core::{
            dom::{DomId, NodeId},
            events::{EasingFunction, EventSource},
            geom::LogicalPosition,
        };
        use azul_layout::scroll::ScrollEvent;

        let layout_window = self.layout_window.as_mut().ok_or("No layout window")?;

        let dom_id_typed = DomId {
            inner: dom_id as usize,
        };
        let node_id_typed = node_id as u32; // NodeId is u32 in scroll system

        // 1. Create scroll event and process it
        let scroll_event = ScrollEvent {
            dom_id: dom_id_typed,
            node_id: node_id_typed,
            delta: LogicalPosition::new(delta_x, delta_y),
            source: EventSource::User,
            duration: None, // Instant scroll
            easing: EasingFunction::Linear,
        };

        layout_window
            .scroll_states
            .apply_scroll_event(scroll_event)
            .map_err(|e| format!("Scroll error: {:?}", e))?;

        // 2. Update WebRender scroll layers and GPU transforms
        let mut txn = crate::desktop::wr_translate2::WrTransaction::new();

        // Scroll all nodes in the scroll manager to WebRender
        // This updates external scroll IDs with new offsets
        self.scroll_all_nodes(&layout_window.scroll_states, &mut txn);

        // Synchronize GPU-animated values (transforms, opacities)
        // Note: We need mutable access for gpu_state_manager updates
        self.synchronize_gpu_values(layout_window, &mut txn);

        // Send transaction and generate frame (without rebuilding display list)
        self.render_api.send_transaction(
            crate::desktop::wr_translate2::wr_translate_document_id(self.document_id),
            txn,
        );

        crate::desktop::wr_translate2::generate_frame(
            layout_window,
            &mut self.render_api,
            false, // Display list not rebuilt, just transforms updated
        );

        Ok(())
    }

    /// Internal: Scroll all nodes to WebRender
    fn scroll_all_nodes(
        &mut self,
        scroll_manager: &azul_layout::scroll::ScrollManager,
        txn: &mut crate::desktop::wr_translate2::WrTransaction,
    ) {
        // TODO: Implement once we have external_scroll_id mapping
        // For each scroll state in scroll_manager:
        // 1. Get external_scroll_id from layout_result
        // 2. Call txn.scroll_node_with_id(offset, external_scroll_id,
        //    ScrollClamping::ToContentBounds)
    }

    /// Internal: Synchronize GPU-animated values to WebRender
    fn synchronize_gpu_values(
        &mut self,
        layout_window: &mut azul_layout::window::LayoutWindow,
        txn: &mut crate::desktop::wr_translate2::WrTransaction,
    ) {
        use webrender::api::{
            DynamicProperties as WrDynamicProperties, PropertyBindingKey as WrPropertyBindingKey,
            PropertyValue as WrPropertyValue,
        };

        let dpi = layout_window.current_window_state.size.get_hidpi_factor();

        // Update scrollbar transforms using GpuStateManager
        for (dom_id, layout_result) in &layout_window.layout_results {
            layout_window.gpu_state_manager.update_scrollbar_transforms(
                *dom_id,
                &layout_window.scroll_states,
                &layout_result.layout_tree,
            );
        }

        // Collect all transform keys and values from GPU caches
        let transforms = layout_window
            .gpu_state_manager
            .caches
            .values()
            .flat_map(|gpu_cache| {
                gpu_cache
                    .transform_keys
                    .iter()
                    .filter_map(|(node_id, key)| {
                        let mut value = gpu_cache.current_transform_values.get(node_id)?.clone();
                        value.scale_for_dpi(dpi);
                        Some((key, value))
                    })
            })
            .map(|(k, v)| WrPropertyValue {
                key: WrPropertyBindingKey::new(k.id as u64),
                value: self.wr_translate_layout_transform(&v),
            })
            .collect::<Vec<_>>();

        // Collect all opacity keys and values (including scrollbar opacities)
        let floats = layout_window
            .gpu_state_manager
            .caches
            .values()
            .flat_map(|gpu_cache| {
                // Regular opacity values
                let mut opacity_values = gpu_cache
                    .opacity_keys
                    .iter()
                    .filter_map(|(node_id, key)| {
                        let value = gpu_cache.current_opacity_values.get(node_id)?;
                        Some((key, *value))
                    })
                    .collect::<Vec<_>>();

                // Vertical scrollbar opacities
                opacity_values.extend(gpu_cache.scrollbar_v_opacity_keys.iter().filter_map(
                    |(key_tuple, key)| {
                        let value = gpu_cache.scrollbar_v_opacity_values.get(key_tuple)?;
                        Some((key, *value))
                    },
                ));

                // Horizontal scrollbar opacities
                opacity_values.extend(gpu_cache.scrollbar_h_opacity_keys.iter().filter_map(
                    |(key_tuple, key)| {
                        let value = gpu_cache.scrollbar_h_opacity_values.get(key_tuple)?;
                        Some((key, *value))
                    },
                ));

                opacity_values.into_iter()
            })
            .map(|(k, v)| WrPropertyValue {
                key: WrPropertyBindingKey::new(k.id as u64),
                value: v,
            })
            .collect::<Vec<_>>();

        // Update dynamic properties in WebRender
        txn.update_dynamic_properties(WrDynamicProperties {
            transforms,
            floats,
            colors: Vec::new(), // No color animations for now
        });
    }

    /// Helper: Translate ComputedTransform3D to WebRender LayoutTransform
    fn wr_translate_layout_transform(
        &self,
        transform: &azul_core::transform::ComputedTransform3D,
    ) -> webrender::api::units::LayoutTransform {
        webrender::api::units::LayoutTransform::new(
            transform.m11,
            transform.m12,
            transform.m13,
            transform.m14,
            transform.m21,
            transform.m22,
            transform.m23,
            transform.m24,
            transform.m31,
            transform.m32,
            transform.m33,
            transform.m34,
            transform.m41,
            transform.m42,
            transform.m43,
            transform.m44,
        )
    }

    fn sync_window_state(&mut self) {
        let previous = match &self.previous_window_state {
            Some(prev) => prev,
            None => return, // First frame, nothing to sync
        };

        let current = &self.current_window_state;

        // Title changed?
        if previous.title != current.title {
            let title = NSString::from_str(&current.title);
            self.window.setTitle(&title);
        }

        // Size changed?
        if previous.size.dimensions != current.size.dimensions {
            let size = NSSize::new(
                current.size.dimensions.width as f64,
                current.size.dimensions.height as f64,
            );
            unsafe {
                self.window.setContentSize(size);
            }
        }

        // Position changed?
        if previous.position != current.position {
            use azul_core::window::WindowPosition;
            match current.position {
                WindowPosition::Initialized(pos) => {
                    let origin = NSPoint::new(pos.x as f64, pos.y as f64);
                    unsafe {
                        self.window.setFrameTopLeftPoint(origin);
                    }
                }
                WindowPosition::Uninitialized => {}
            }
        }

        // Window flags changed?
        if previous.flags != current.flags {
            let mut style_mask = NSWindowStyleMask::Titled;

            if current.flags.is_resizable {
                style_mask |= NSWindowStyleMask::Resizable;
            }
            if current.flags.has_decorations {
                style_mask |= NSWindowStyleMask::Closable | NSWindowStyleMask::Miniaturizable;
            }

            self.window.setStyleMask(style_mask);
        }

        // Visibility changed?
        if previous.flags.is_visible != current.flags.is_visible {
            if current.flags.is_visible {
                self.window.makeKeyAndOrderFront(None);
            } else {
                self.window.orderOut(None);
            }
        }
    }

    /// Update window state at the end of each frame (before rendering)
    ///
    /// This should be called after all callbacks have been processed but before
    /// `present()` is called. It prepares for the next frame by moving current
    /// state to previous state.
    pub fn update_window_state(&mut self, new_state: WindowState) {
        // Save current state as previous for next frame's diff
        self.previous_window_state = Some(self.current_window_state.clone());

        // Update current state from new WindowState
        self.current_window_state.title = new_state.title;
        self.current_window_state.size = new_state.size;
        self.current_window_state.position = new_state.position;
        self.current_window_state.flags = new_state.flags;
        self.current_window_state.theme = new_state.theme;
        self.current_window_state.debug_state = new_state.debug_state;
        self.current_window_state.keyboard_state = new_state.keyboard_state;
        self.current_window_state.mouse_state = new_state.mouse_state;
        self.current_window_state.touch_state = new_state.touch_state;
        self.current_window_state.ime_position = new_state.ime_position;
        self.current_window_state.platform_specific_options = new_state.platform_specific_options;
        self.current_window_state.renderer_options = new_state.renderer_options;
        self.current_window_state.background_color = new_state.background_color;
        self.current_window_state.layout_callback = new_state.layout_callback;
        self.current_window_state.close_callback = new_state.close_callback;
        self.current_window_state.monitor = new_state.monitor;

        // Synchronize with OS
        self.sync_window_state();
    }

    /// Process an NSEvent and dispatch to appropriate handler
    fn process_event(&mut self, event: &NSEvent, macos_event: &MacOSEvent) {
        use azul_core::events::MouseButton;

        match event.r#type() {
            NSEventType::LeftMouseDown => {
                let _ = self.handle_mouse_down(event, MouseButton::Left);
            }
            NSEventType::LeftMouseUp => {
                let _ = self.handle_mouse_up(event, MouseButton::Left);
            }
            NSEventType::RightMouseDown => {
                let _ = self.handle_mouse_down(event, MouseButton::Right);
            }
            NSEventType::RightMouseUp => {
                let _ = self.handle_mouse_up(event, MouseButton::Right);
            }
            NSEventType::OtherMouseDown => {
                let _ = self.handle_mouse_down(event, MouseButton::Middle);
            }
            NSEventType::OtherMouseUp => {
                let _ = self.handle_mouse_up(event, MouseButton::Middle);
            }
            NSEventType::MouseMoved
            | NSEventType::LeftMouseDragged
            | NSEventType::RightMouseDragged => {
                let _ = self.handle_mouse_move(event);
            }
            NSEventType::ScrollWheel => {
                let _ = self.handle_scroll_wheel(event);
            }
            NSEventType::KeyDown => {
                let _ = self.handle_key_down(event);
            }
            NSEventType::KeyUp => {
                let _ = self.handle_key_up(event);
            }
            _ => {
                // Other events not handled yet
            }
        }
    }
}

impl PlatformWindow for MacOSWindow {
    type EventType = MacOSEvent;

    fn new(options: WindowCreateOptions) -> Result<Self, WindowError>
    where
        Self: Sized,
    {
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| WindowError::PlatformError("Not on main thread".into()))?;
        Self::new_with_options(options, mtm)
    }

    fn get_state(&self) -> WindowState {
        let frame = self.window.frame();
        let mut state = WindowState::default();

        // Update size (dimensions is LogicalSize)
        state.size.dimensions.width = frame.size.width as f32;
        state.size.dimensions.height = frame.size.height as f32;

        // Update title
        state.title = self.window.title().to_string().into();

        state
    }

    fn set_properties(&mut self, props: WindowProperties) -> Result<(), WindowError> {
        // Update current_window_state based on properties
        if let Some(title) = props.title {
            self.current_window_state.title = title.into();
        }

        if let Some(size) = props.size {
            use azul_core::geom::LogicalSize;
            // Get actual DPI scale from window
            let scale_factor = unsafe {
                self.window
                    .screen()
                    .map(|screen| screen.backingScaleFactor())
                    .unwrap_or(1.0)
            };

            // Convert PhysicalSize to LogicalSize using actual DPI
            self.current_window_state.size.dimensions = LogicalSize {
                width: (size.width as f64 / scale_factor) as f32,
                height: (size.height as f64 / scale_factor) as f32,
            };
        }

        if let Some(visible) = props.visible {
            self.current_window_state.flags.is_visible = visible;
        }

        // Synchronize changes with the OS
        self.sync_window_state();

        Ok(())
    }

    fn poll_event(&mut self) -> Option<Self::EventType> {
        let app = NSApplication::sharedApplication(self.mtm);

        // Poll event (non-blocking)
        let event = unsafe {
            app.nextEventMatchingMask_untilDate_inMode_dequeue(
                NSEventMask::Any,
                None, // No wait time = non-blocking
                objc2_foundation::NSDefaultRunLoopMode,
                true,
            )
        };

        if let Some(event) = event {
            // Convert and process event
            let macos_event = MacOSEvent::from_nsevent(&event);

            // Dispatch event to handlers
            self.process_event(&event, &macos_event);

            // Forward event to system
            unsafe {
                app.sendEvent(&event);
            }

            Some(macos_event)
        } else {
            None
        }
    }

    fn wait_event(&mut self) -> Option<Self::EventType> {
        let app = NSApplication::sharedApplication(self.mtm);

        // Wait for event (blocking)
        let event = unsafe {
            app.nextEventMatchingMask_untilDate_inMode_dequeue(
                NSEventMask::Any,
                Some(&objc2_foundation::NSDate::distantFuture()), // Wait indefinitely
                objc2_foundation::NSDefaultRunLoopMode,
                true,
            )
        };

        if let Some(event) = event {
            // Convert and process event
            let macos_event = MacOSEvent::from_nsevent(&event);

            // Dispatch event to handlers
            self.process_event(&event, &macos_event);

            // Forward event to system
            unsafe {
                app.sendEvent(&event);
            }

            Some(macos_event)
        } else {
            // Window closed
            None
        }
    }

    fn get_render_context(&self) -> RenderContext {
        match self.backend {
            RenderBackend::OpenGL => {
                let context_ptr = self
                    .gl_context
                    .as_ref()
                    .map(|ctx| Retained::as_ptr(ctx) as *mut _)
                    .unwrap_or(std::ptr::null_mut());

                RenderContext::OpenGL {
                    context: context_ptr,
                }
            }
            RenderBackend::CPU => RenderContext::CPU,
        }
    }

    fn present(&mut self) -> Result<(), WindowError> {
        match self.backend {
            RenderBackend::OpenGL => {
                if let Some(ref gl_view) = self.gl_view {
                    unsafe {
                        gl_view.setNeedsDisplay(true);
                    }
                }
            }
            RenderBackend::CPU => {
                if let Some(ref cpu_view) = self.cpu_view {
                    unsafe {
                        cpu_view.setNeedsDisplay(true);
                    }
                }
            }
        }
        Ok(())
    }

    fn is_open(&self) -> bool {
        self.is_open
    }

    fn close(&mut self) {
        self.window.close();
        self.is_open = false;
    }

    fn request_redraw(&mut self) {
        // TODO: Implement redraw request
    }
}

/// macOS event type.
#[derive(Debug, Clone, Copy)]
pub enum MacOSEvent {
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
    /// Other event
    Other,
}

impl MacOSEvent {
    /// Convert NSEvent to MacOSEvent.
    fn from_nsevent(event: &NSEvent) -> Self {
        match event.r#type() {
            NSEventType::LeftMouseDown => {
                let loc = event.locationInWindow();
                MacOSEvent::MouseDown {
                    button: 0,
                    x: loc.x,
                    y: loc.y,
                }
            }
            NSEventType::LeftMouseUp => {
                let loc = event.locationInWindow();
                MacOSEvent::MouseUp {
                    button: 0,
                    x: loc.x,
                    y: loc.y,
                }
            }
            NSEventType::RightMouseDown => {
                let loc = event.locationInWindow();
                MacOSEvent::MouseDown {
                    button: 1,
                    x: loc.x,
                    y: loc.y,
                }
            }
            NSEventType::RightMouseUp => {
                let loc = event.locationInWindow();
                MacOSEvent::MouseUp {
                    button: 1,
                    x: loc.x,
                    y: loc.y,
                }
            }
            NSEventType::MouseMoved => {
                let loc = event.locationInWindow();
                MacOSEvent::MouseMove { x: loc.x, y: loc.y }
            }
            NSEventType::KeyDown => MacOSEvent::KeyDown {
                key_code: event.keyCode(),
            },
            NSEventType::KeyUp => MacOSEvent::KeyUp {
                key_code: event.keyCode(),
            },
            _ => MacOSEvent::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_hash_changes() {
        use azul_core::menu::{Menu, MenuItem, MenuItemVec, StringMenuItem};
        use azul_css::AzString;

        let menu1 = Menu::new(MenuItemVec::from_const_slice(&[MenuItem::String(
            StringMenuItem::new(AzString::from_const_str("Item 1")),
        )]));

        let menu2 = Menu::new(MenuItemVec::from_const_slice(&[MenuItem::String(
            StringMenuItem::new(AzString::from_const_str("Item 2")),
        )]));

        assert_ne!(menu1.get_hash(), menu2.get_hash());
    }
}
