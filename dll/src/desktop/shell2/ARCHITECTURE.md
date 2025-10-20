# shell2 Cross-Platform Window System Architecture

## Overview

shell2 is a platform-abstracted windowing system for Azul, designed to work consistently across macOS, Windows, and Linux (X11/Wayland). It provides both GPU-accelerated (OpenGL/Metal/D3D11/Vulkan) and CPU-only rendering paths.

## Core Abstraction

### PlatformWindow Trait (in `common/window.rs`)

```rust
pub trait PlatformWindow {
    type EventType: Copy;
    
    fn new(options: WindowCreateOptions) -> Result<Self, WindowError>;
    fn get_state(&self) -> WindowState;
    fn set_properties(&mut self, props: WindowProperties) -> Result<(), WindowError>;
    fn poll_event(&mut self) -> Option<Self::EventType>;
    fn wait_event(&mut self) -> Option<Self::EventType>;
    fn get_render_context(&self) -> RenderContext;
    fn present(&mut self) -> Result<(), WindowError>;
    fn is_open(&self) -> bool;
    fn close(&mut self);
    fn request_redraw(&mut self);
}
```

## Platform-Specific View Architecture

### Concept: Dual View System

Each platform implements **two distinct view types** for GPU vs CPU rendering:

#### macOS (AppKit)

- **GLView** - Subclass of `NSOpenGLView` (GPU rendering)
  - Has its own OpenGL context
  - Overrides: `drawRect:`, `prepareOpenGL`, `reshape`
  - Loads GL functions via `dlopen`
  
- **CPUView** - Subclass of `NSView` (CPU rendering)
  - Has framebuffer (`Vec<u8>`)
  - Overrides: `drawRect:`, `isOpaque`
  - Renders via `NSBitmapImageRep`

#### Windows (Win32)

- **GLWindow** - HWND with WGL OpenGL context
  - `CreateWindow` + `wglCreateContext`
  - `SwapBuffers` for present
  
- **CPUWindow** - HWND with DIB (Device Independent Bitmap)
  - `BitBlt` for framebuffer copy

#### Linux X11

- **GLXWindow** - X11 Window with GLX context
  - `glXCreateContext`
  - `glXSwapBuffers`
  
- **ShmWindow** - X11 Window with MIT-SHM extension
  - `XShmPutImage` for shared memory framebuffer

#### Linux Wayland

- **EGLSurface** - `wl_surface` with EGL context
  - `eglCreateWindowSurface`
  
- **ShmBuffer** - `wl_surface` with `wl_shm` pool
  - `wl_surface_attach`

## macOS Implementation Details

### Backend Selection

```rust
pub enum RenderBackend {
    OpenGL,
    CPU,
}
```

### View Classes (using `define_class!`)

#### GLView Structure

```rust
// Instance variables
pub struct GLViewIvars {
    gl_functions: Option<Rc<gl_context_loader::GenericGlContext>>,
    needs_reshape: bool,
}

// Class definition
define_class!(
    #[unsafe(super(NSOpenGLView))]
    #[thread_kind = MainThreadOnly]
    #[name = "AzulGLView"]
    #[ivars = GLViewIvars]
    pub struct GLView;
    
    unsafe impl GLView {
        #[method(drawRect:)]
        fn draw_rect(&self, rect: NSRect) {
            // 1. Get GL functions from ivars
            // 2. Make context current
            // 3. Execute GL commands (clear, draw, etc.)
            // 4. Flush buffer
        }
        
        #[method(prepareOpenGL)]
        fn prepare_opengl(&self) {
            // 1. Load GL functions via dlopen
            // 2. Store in ivars
            // 3. Set initial GL state
        }
        
        #[method(reshape)]
        fn reshape(&self) {
            // 1. Update context
            // 2. Update viewport to match new size
        }
        
        #[method_id(initWithFrame:pixelFormat:)]
        fn init(
            this: Allocated<Self>,
            frame: NSRect,
            pixel_format: Option<&NSOpenGLPixelFormat>,
        ) -> Option<Retained<Self>> {
            // 1. Initialize ivars
            // 2. Call super init
            // 3. Return initialized instance
        }
    }
);
```

#### CPUView Structure

```rust
// Instance variables
pub struct CPUViewIvars {
    framebuffer: Vec<u8>,
    width: usize,
    height: usize,
    needs_redraw: bool,
}

// Class definition
define_class!(
    #[unsafe(super(NSView))]
    #[thread_kind = MainThreadOnly]
    #[name = "AzulCPUView"]
    #[ivars = CPUViewIvars]
    pub struct CPUView;
    
    unsafe impl CPUView {
        #[method(drawRect:)]
        fn draw_rect(&self, rect: NSRect) {
            // 1. Get bounds
            // 2. Resize framebuffer if needed
            // 3. Render to framebuffer (e.g., blue gradient)
            // 4. Create NSBitmapImageRep from framebuffer
            // 5. Blit to window via NSImage::drawInRect
        }
        
        #[method(isOpaque)]
        fn is_opaque(&self) -> bool {
            true
        }
        
        #[method_id(initWithFrame:)]
        fn init(
            this: Allocated<Self>,
            frame: NSRect,
        ) -> Option<Retained<Self>> {
            // 1. Initialize ivars with empty framebuffer
            // 2. Call super init
            // 3. Return initialized instance
        }
    }
);
```

### Main Window Structure

```rust
pub struct MacOSWindow {
    // Core window
    window: Retained<NSWindow>,
    
    // Rendering backend
    backend: RenderBackend,
    
    // GPU rendering (if backend == OpenGL)
    gl_view: Option<Retained<GLView>>,
    gl_context: Option<Retained<NSOpenGLContext>>,
    gl_functions: Option<Rc<GlFunctions>>,
    
    // CPU rendering (if backend == CPU)
    cpu_view: Option<Retained<CPUView>>,
    
    // Window state
    is_open: bool,
    mtm: MainThreadMarker,
}
```

### Backend Selection Strategy

```rust
impl MacOSWindow {
    fn determine_backend(options: &WindowCreateOptions) -> RenderBackend {
        // 1. Environment variable override
        if let Ok(val) = std::env::var("AZUL_RENDERER") {
            match val.to_lowercase().as_str() {
                "cpu" => return RenderBackend::CPU,
                "opengl" | "gl" => return RenderBackend::OpenGL,
                _ => {}
            }
        }
        
        // 2. Check options.renderer
        if let Some(renderer_opts) = &options.renderer {
            if !renderer_opts.hw_accel.unwrap_or(true) {
                return RenderBackend::CPU;
            }
        }
        
        // 3. Default: Try OpenGL
        RenderBackend::OpenGL
    }
}
```

### View Creation

```rust
impl MacOSWindow {
    fn create_gl_view(
        frame: NSRect,
        mtm: MainThreadMarker,
    ) -> Result<(Retained<GLView>, Retained<NSOpenGLContext>, Rc<GlFunctions>), WindowError> {
        // 1. Create NSOpenGLPixelFormat with attributes:
        //    - DoubleBuffer
        //    - DepthSize(24)
        //    - OpenGLProfile(3.2 Core)
        //    - ColorSize(24)
        //    - AlphaSize(8)
        //    - Accelerated
        
        // 2. Create GLView with pixel format
        
        // 3. Get OpenGL context from view
        
        // 4. Load GL functions via dlopen
        
        // 5. Return (view, context, functions)
    }
    
    fn create_cpu_view(
        frame: NSRect,
        mtm: MainThreadMarker,
    ) -> Retained<CPUView> {
        // 1. Create CPUView with frame
        // 2. Return view
    }
}
```

### Window Creation Flow

```rust
impl MacOSWindow {
    pub fn new(options: WindowCreateOptions) -> Result<Self, WindowError> {
        let mtm = MainThreadMarker::new()
            .ok_or_else(|| WindowError::PlatformError("Not on main thread".into()))?;
        
        // 1. Initialize NSApplication
        let app = NSApplication::sharedApplication(mtm);
        app.setActivationPolicy(NSApplicationActivationPolicy::Regular);
        
        // 2. Determine backend
        let backend = Self::determine_backend(&options);
        
        // 3. Calculate window frame
        let screen = NSScreen::mainScreen(mtm)?;
        let frame = calculate_centered_frame(screen, &options);
        
        // 4. Create content view based on backend
        let (gl_view, gl_context, gl_functions, cpu_view) = match backend {
            RenderBackend::OpenGL => {
                match Self::create_gl_view(frame, mtm) {
                    Ok((view, ctx, funcs)) => {
                        (Some(view), Some(ctx), Some(funcs), None)
                    }
                    Err(e) => {
                        eprintln!("OpenGL failed: {}, falling back to CPU", e);
                        let view = Self::create_cpu_view(frame, mtm);
                        (None, None, None, Some(view))
                    }
                }
            }
            RenderBackend::CPU => {
                let view = Self::create_cpu_view(frame, mtm);
                (None, None, None, Some(view))
            }
        };
        
        // 5. Create NSWindow
        let window = NSWindow::initWithContentRect_styleMask_backing_defer(...);
        
        // 6. Set content view (either GL or CPU)
        let content_view: &NSView = if let Some(ref gl) = gl_view {
            unsafe { std::mem::transmute(&**gl) }
        } else {
            unsafe { std::mem::transmute(&**cpu_view.as_ref().unwrap()) }
        };
        window.setContentView(Some(content_view));
        
        // 7. Configure and show window
        window.setTitle(...);
        window.center();
        window.makeKeyAndOrderFront(None);
        
        Ok(Self {
            window,
            backend: if gl_view.is_some() { RenderBackend::OpenGL } else { RenderBackend::CPU },
            gl_view,
            gl_context,
            gl_functions,
            cpu_view,
            is_open: true,
            mtm,
        })
    }
}
```

## Rendering Flow

### OpenGL Path

1. `MacOSWindow::present()` → `gl_view.setNeedsDisplay(true)`
2. AppKit calls `GLView::drawRect()`
3. `drawRect()` uses `gl_functions` to execute GL commands
4. `NSOpenGLContext::flushBuffer()` swaps buffers

### CPU Path

1. `MacOSWindow::present()` → update CPU framebuffer + `cpu_view.setNeedsDisplay(true)`
2. AppKit calls `CPUView::drawRect()`
3. `drawRect()` creates `NSBitmapImageRep` from framebuffer
4. `NSImage::drawInRect()` blits to window

## RenderContext

```rust
impl MacOSWindow {
    fn get_render_context(&self) -> RenderContext {
        match self.backend {
            RenderBackend::OpenGL => {
                RenderContext::OpenGL {
                    context: self.gl_context
                        .as_ref()
                        .map(|ctx| ctx.as_ptr() as *mut _)
                        .unwrap_or(std::ptr::null_mut()),
                }
            }
            RenderBackend::CPU => {
                RenderContext::CPU
            }
        }
    }
}
```

## Error Handling

- **OpenGL creation failure** → Automatic fallback to CPU
- **Window creation failure** → Return `WindowError::PlatformError`
- **Invalid main thread** → Return `WindowError::PlatformError("Not on main thread")`

## Cross-Platform Compatibility

### Common Interface

All platforms must provide:
- Window creation with backend selection
- `RenderContext` exposing GL context OR CPU framebuffer
- `present()` triggering redraw
- Event polling/waiting
- Window state management

### Platform Differences

| Feature | macOS | Windows | Linux X11 | Linux Wayland |
|---------|-------|---------|-----------|---------------|
| Window Type | NSWindow | HWND | X11 Window | wl_surface |
| GL API | NSOpenGLContext | WGL | GLX | EGL |
| CPU Blit | NSBitmapImageRep | BitBlt | XShmPutImage | wl_shm |
| Event Loop | NSApplication | GetMessage | XNextEvent | wl_display_dispatch |

## Benefits

✅ **Clean separation:** GL vs CPU rendering isolated in separate view classes  
✅ **Cross-platform:** Same pattern works across all platforms  
✅ **Fallback support:** Automatic fallback from GL to CPU on failure  
✅ **Type safety:** `define_class!` provides proper Objective-C integration  
✅ **Performance:** GL rendering uses native AppKit drawing cycle  
✅ **Testability:** Can force CPU mode for testing without GPU  

## Future Extensions

- **Metal support:** Add MetalView subclass for macOS Metal rendering
- **Vulkan support:** Add VulkanSurface for cross-platform Vulkan
- **HDR support:** Extend pixel format attributes for HDR displays
- **Multi-window:** Support multiple windows per application
