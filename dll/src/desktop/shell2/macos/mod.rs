//! macOS implementation using AppKit/Cocoa.
//!
//! This module implements the PlatformWindow trait for macOS using:
//! - NSWindow for window management
//! - NSOpenGLContext for GPU rendering (optional)
//! - NSMenu for menu bar and context menus
//! - NSEvent for event handling
//!
//! Note: macOS uses static linking for system frameworks (standard approach).

use std::rc::Rc;

use azul_core::menu::Menu;
use azul_layout::window_state::{WindowCreateOptions, WindowState};
use objc2::{
    define_class, msg_send_id,
    rc::{Allocated, Retained},
    runtime::ProtocolObject,
    ClassType, DefinedClass, MainThreadOnly,
};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationPolicy, NSApplicationDelegate, NSBackingStoreType,
    NSBitmapImageRep, NSColor, NSCompositingOperation, NSEvent, NSEventMask, NSEventType, NSImage,
    NSMenu, NSMenuItem, NSOpenGLContext, NSOpenGLPixelFormat, NSOpenGLPixelFormatAttribute,
    NSOpenGLView, NSResponder, NSScreen, NSView, NSWindow, NSWindowStyleMask,
};
use objc2_foundation::{
    ns_string, MainThreadMarker, NSData, NSNotification, NSObject, NSPoint, NSRect, NSSize,
    NSString,
};

use crate::desktop::shell2::common::{
    Compositor, CompositorError, CompositorMode, PlatformWindow, RenderContext, WindowError,
    WindowProperties,
};

mod gl;
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
#[derive(Debug)]
pub struct GLViewIvars {
    gl_functions: Option<Rc<gl_context_loader::GenericGlContext>>,
    needs_reshape: bool,
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
            if let Some(ref gl) = self.ivars().gl_functions {
                unsafe {
                    // Clear to blue color
                    if let (Some(clear_color), Some(clear)) = (gl.glClearColor, gl.glClear) {
                        clear_color(0.2, 0.3, 0.8, 1.0); // Blue background
                        clear(0x00004000); // GL_COLOR_BUFFER_BIT
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
                    *self.ivars().gl_functions = Some(functions.get_context());
                    *self.ivars().needs_reshape = true;
                }
                Err(e) => {
                    eprintln!("Failed to load GL functions: {}", e);
                }
            }
        }

        #[unsafe(method(reshape))]
        fn reshape(&self) {
            // Update context
            unsafe {
                if let Some(context) = self.openGLContext() {
                    context.update();
                }
            }

            // Update viewport
            let bounds = unsafe { self.bounds() };
            let width = bounds.size.width as i32;
            let height = bounds.size.height as i32;

            if let Some(ref gl) = self.ivars().gl_functions {
                unsafe {
                    if let Some(viewport) = gl.glViewport {
                        viewport(0, 0, width, height);
                    }
                }
            }

            *self.ivars().needs_reshape = false;
        }

        #[unsafe(method_id(initWithFrame:pixelFormat:))]
        fn init_with_frame_pixel_format(
            this: Allocated<Self>,
            frame: NSRect,
            pixel_format: Option<&NSOpenGLPixelFormat>,
        ) -> Option<Retained<Self>> {
            let this = this.set_ivars(GLViewIvars {
                gl_functions: None,
                needs_reshape: true,
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
#[derive(Debug)]
pub struct CPUViewIvars {
    framebuffer: Vec<u8>,
    width: usize,
    height: usize,
    needs_redraw: bool,
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
            if ivars.width != width || ivars.height != height {
                *ivars.width = width;
                *ivars.height = height;
                ivars.framebuffer.resize(width * height * 4, 0);
            }

            // Render blue gradient to framebuffer
            for y in 0..height {
                for x in 0..width {
                    let idx = (y * width + x) * 4;
                    ivars.framebuffer[idx] = (x * 128 / width.max(1)) as u8; // R
                    ivars.framebuffer[idx + 1] = (y * 128 / height.max(1)) as u8; // G
                    ivars.framebuffer[idx + 2] = 255; // B - Blue
                    ivars.framebuffer[idx + 3] = 255; // A
                }
            }

            // Blit framebuffer to window
            unsafe {
                let mtm = MainThreadMarker::new_unchecked();
                let data = NSData::with_bytes(&ivars.framebuffer);

                if let Some(bitmap) = NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
                    mtm.alloc(),
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
                    std::ptr::copy_nonoverlapping(
                        data.bytes().as_ptr(),
                        bitmap.bitmapData(),
                        ivars.framebuffer.len(),
                    );

                    if let Some(image) = NSImage::initWithSize(mtm.alloc(), bounds.size) {
                        image.addRepresentation(&bitmap);
                        image.drawInRect(bounds);
                    }
                }
            }
        }

        #[unsafe(method(isOpaque))]
        fn is_opaque(&self) -> bool {
            true
        }

        #[unsafe(method_id(initWithFrame:))]
        fn init_with_frame(
            this: Allocated<Self>,
            frame: NSRect,
        ) -> Option<Retained<Self>> {
            let this = this.set_ivars(CPUViewIvars {
                framebuffer: Vec::new(),
                width: 0,
                height: 0,
                needs_redraw: true,
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

    unsafe { NSOpenGLPixelFormat::initWithAttributes(mtm.alloc(), attrs.as_ptr()) }
        .ok_or_else(|| WindowError::ContextCreationFailed)
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

        // 2. Check options.renderer
        if let Some(renderer_opts) = &options.renderer {
            // If HW acceleration explicitly disabled -> CPU
            if let Some(hw_accel) = renderer_opts.hw_accel {
                if !hw_accel {
                    return RenderBackend::CPU;
                }
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
        let gl_view = unsafe {
            msg_send_id![
                mtm.alloc::<GLView>(),
                initWithFrame: frame,
                pixelFormat: Some(&pixel_format),
            ]
        }
        .ok_or_else(|| WindowError::PlatformError("Failed to create GLView".into()))?;

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
        unsafe { msg_send_id![mtm.alloc::<CPUView>(), initWithFrame: frame] }
            .expect("Failed to create CPUView")
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
        let content_view: &NSView = if let Some(ref gl) = gl_view {
            unsafe { std::mem::transmute(&**gl) }
        } else if let Some(ref cpu) = cpu_view {
            unsafe { std::mem::transmute(&**cpu) }
        } else {
            return Err(WindowError::PlatformError("No content view created".into()));
        };

        unsafe {
            window.setContentView(Some(content_view));
            window.center();
            window.makeKeyAndOrderFront(None);
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
        })
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
        if let Some(title) = props.title {
            let ns_title = NSString::from_str(&title);
            self.window.setTitle(&ns_title);
        }

        if let Some(size) = props.size {
            let new_size = NSSize::new(size.width as f64, size.height as f64);
            let mut frame = self.window.frame();
            frame.size = new_size;
            self.window.setFrame_display(frame, true);
        }

        if let Some(visible) = props.visible {
            if visible {
                self.window.makeKeyAndOrderFront(None);
            } else {
                self.window.orderOut(None);
            }
        }

        Ok(())
    }

    fn poll_event(&mut self) -> Option<Self::EventType> {
        // TODO: Implement event polling
        None
    }

    fn wait_event(&mut self) -> Option<Self::EventType> {
        // TODO: Implement event waiting
        None
    }

    fn get_render_context(&self) -> RenderContext {
        match self.backend {
            RenderBackend::OpenGL => RenderContext::OpenGL {
                context: self
                    .gl_context
                    .as_ref()
                    .map(|ctx| ctx.as_ptr() as *mut _)
                    .unwrap_or(std::ptr::null_mut()),
            },
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
