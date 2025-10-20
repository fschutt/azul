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

use crate::desktop::shell2::common::{
    Compositor, CompositorError, CompositorMode, PlatformWindow, RenderContext, WindowError,
    WindowProperties,
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

        // Initialize window states
        let current_window_state = FullWindowState {
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
            layout_window: None, // TODO: Initialize with LayoutWindow from options
            menu_state: menu::MenuState::new(),
        })
    }

    /// Synchronize window state with the OS based on diff between previous and current state
    /// Regenerate layout and display list for the current window.
    ///
    /// This should be called when:
    /// - The window is resized
    /// - The DOM changes (via callbacks)
    /// - Layout callback changes
    pub fn regenerate_layout(&mut self) -> Result<(), String> {
        // TODO: Implement full layout regeneration
        // This requires:
        // 1. Calling layout_callback to get styled_dom
        // 2. layout_window.layout_and_generate_display_list()
        // 3. Updating display list for rendering
        //
        // For now, just return Ok
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
        // TODO: Implement GPU scrolling
        // This requires:
        // 1. Getting current scroll position from layout_window
        // 2. Updating scroll position by delta
        // 3. Updating GPU transforms (WebRender scroll layers)
        // 4. Triggering a redraw without relayout
        //
        // For now, just trigger a redraw
        Ok(())
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
