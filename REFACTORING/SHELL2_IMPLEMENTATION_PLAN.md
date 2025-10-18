# shell2 Implementation Plan

## Date: 18. Oktober 2025

## Overview
Complete rewrite of the windowing/platform layer as `shell2` module. Keep old `shell` code as reference but don't attempt to patch it. Focus on clean architecture with CPU/GPU compositor choice and dynamic library loading.

## Design Goals

### 1. CPU/GPU Compositor Flexibility
- Each window can independently choose CPU or GPU rendering
- Initially: GPU compositor (webrender) with CPU stub
- Future: Replace azul-webrender with custom-webrender crate
- Reference: webrender's `sw_compositor.rs` for CPU implementation

### 2. Dynamic Library Loading
- **macOS**: Link system frameworks directly (standard approach)
- **Linux**: Use `dlopen()` for all system libraries
- **Windows**: Use `LoadLibrary()` for all system DLLs
- **Benefit**: No linker errors for users, works on any system version

### 3. Clean Platform Abstraction
```
dll/src/desktop/shell2/
├── mod.rs                    # Public API, platform selection
├── common/                   # Platform-agnostic code
│   ├── event.rs             # Event type definitions
│   ├── window_state.rs      # Window state management
│   ├── compositor.rs        # Compositor trait
│   └── dlopen.rs            # Dynamic loading helpers
├── macos/                   # macOS implementation
│   ├── mod.rs
│   ├── appkit.rs            # AppKit windowing
│   ├── event.rs             # AppKit event handling
│   └── compositor.rs        # Metal/OpenGL compositor
├── windows/                 # Windows implementation
│   ├── mod.rs
│   ├── win32.rs             # Win32 windowing
│   ├── event.rs             # Win32 event handling
│   ├── compositor.rs        # D3D11/OpenGL compositor
│   └── dlopen.rs            # LoadLibrary() for user32.dll etc.
├── linux/                   # Linux implementation
│   ├── mod.rs               # X11/Wayland selection
│   ├── x11/                 # X11 backend
│   │   ├── mod.rs
│   │   ├── window.rs
│   │   ├── event.rs
│   │   └── dlopen.rs        # dlopen() for libX11.so etc.
│   ├── wayland/             # Wayland backend
│   │   ├── mod.rs
│   │   ├── window.rs
│   │   ├── event.rs
│   │   └── dlopen.rs        # dlopen() for libwayland-client.so etc.
│   └── compositor.rs        # OpenGL/Vulkan compositor
└── stub/                    # Headless/testing backend
    └── mod.rs
```

## Architecture

### Core Traits

```rust
/// A platform window that can render content
pub trait PlatformWindow {
    type EventType;
    
    /// Create a new window with given options
    fn new(options: WindowCreateOptions) -> Result<Self, WindowError>
    where
        Self: Sized;
    
    /// Get current window state (size, position, etc.)
    fn get_state(&self) -> WindowState;
    
    /// Set window properties (title, size, etc.)
    fn set_properties(&mut self, props: WindowProperties) -> Result<(), WindowError>;
    
    /// Get the next event (blocking or non-blocking)
    fn poll_event(&mut self) -> Option<Self::EventType>;
    
    /// Get GL/Metal context for rendering
    fn get_render_context(&self) -> RenderContext;
    
    /// Swap buffers / present frame
    fn present(&mut self) -> Result<(), WindowError>;
    
    /// Close the window
    fn close(&mut self);
}

/// Compositor abstraction - CPU or GPU rendering
pub trait Compositor {
    /// Initialize compositor with window context
    fn new(context: RenderContext, mode: CompositorMode) -> Result<Self, CompositorError>
    where
        Self: Sized;
    
    /// Render a display list to the window
    fn render(&mut self, display_list: &DisplayList) -> Result<(), CompositorError>;
    
    /// Resize framebuffer
    fn resize(&mut self, new_size: PhysicalSize) -> Result<(), CompositorError>;
    
    /// Get current mode (CPU/GPU)
    fn get_mode(&self) -> CompositorMode;
    
    /// Try to switch compositor mode at runtime
    fn try_switch_mode(&mut self, mode: CompositorMode) -> Result<(), CompositorError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositorMode {
    /// Hardware GPU rendering (OpenGL/Metal/D3D/Vulkan)
    GPU,
    /// Software CPU rendering (like webrender sw_compositor)
    CPU,
    /// Automatic selection based on capabilities
    Auto,
}
```

### Dynamic Library Loading

```rust
/// Platform-specific dynamic library loader
pub trait DynamicLibrary {
    /// Load a system library by name
    fn load(name: &str) -> Result<Self, DlError>
    where
        Self: Sized;
    
    /// Get function pointer by symbol name
    fn get_symbol<T>(&self, name: &str) -> Result<T, DlError>;
    
    /// Unload library (automatic on Drop)
    fn unload(&mut self);
}

// Example: Linux X11
struct X11Library {
    handle: *mut c_void,
    // Function pointers loaded dynamically
    XOpenDisplay: unsafe extern "C" fn(*const c_char) -> *mut Display,
    XCreateWindow: unsafe extern "C" fn(...) -> Window,
    XNextEvent: unsafe extern "C" fn(*mut Display, *mut XEvent) -> c_int,
    // ... all X11 functions we need
}

impl X11Library {
    pub fn load() -> Result<Self, DlError> {
        let handle = unsafe { dlopen(b"libX11.so\0".as_ptr() as _, RTLD_LAZY) };
        if handle.is_null() {
            return Err(DlError::LibraryNotFound("libX11.so".into()));
        }
        
        // Load all function pointers
        let XOpenDisplay = unsafe {
            let sym = dlsym(handle, b"XOpenDisplay\0".as_ptr() as _);
            std::mem::transmute(sym)
        };
        
        Ok(Self {
            handle,
            XOpenDisplay,
            // ... load all other functions
        })
    }
}
```

## Implementation Phases

### Phase 1: Core Infrastructure (Week 1)
**Goal:** Basic window creation on all platforms

- [ ] Create `dll/src/desktop/shell2/` directory structure
- [ ] Define core traits: `PlatformWindow`, `Compositor`, `DynamicLibrary`
- [ ] Implement `CompositorMode` enum and selection logic
- [ ] Create stub CPU compositor (returns unimplemented!() for now)

**Deliverable:** Module structure compiles, traits defined

### Phase 2: macOS Implementation (Week 1-2)
**Goal:** Working macOS windows with GPU rendering

- [ ] Implement `MacOSWindow` using AppKit/Cocoa
- [ ] AppKit event loop integration
- [ ] Create Metal/OpenGL compositor (reuse webrender)
- [ ] Window creation, resizing, closing
- [ ] Event handling: mouse, keyboard, window events
- [ ] Menu bar support

**Deliverable:** macOS windows work, can render simple content

### Phase 3: Linux X11 Implementation (Week 2-3)
**Goal:** Working X11 windows with dynamic loading

- [ ] Implement `X11DynamicLibrary` with dlopen()
- [ ] Load libX11.so, libXext.so, libGL.so dynamically
- [ ] Implement `X11Window` using loaded functions
- [ ] X11 event loop
- [ ] OpenGL context creation (GLX)
- [ ] Event handling: mouse, keyboard, window events

**Deliverable:** Linux X11 windows work, no linker dependencies

### Phase 4: Windows Implementation (Week 3-4)
**Goal:** Working Windows windows with dynamic loading

- [ ] Implement `Win32DynamicLibrary` with LoadLibrary()
- [ ] Load user32.dll, kernel32.dll, opengl32.dll dynamically
- [ ] Implement `Win32Window` using loaded functions
- [ ] Win32 message pump
- [ ] OpenGL context creation (WGL)
- [ ] Event handling: mouse, keyboard, window events

**Deliverable:** Windows windows work, no linker dependencies

### Phase 5: Linux Wayland Implementation (Week 4-5)
**Goal:** Working Wayland windows with dynamic loading

- [ ] Implement `WaylandDynamicLibrary` with dlopen()
- [ ] Load libwayland-client.so, libwayland-egl.so dynamically
- [ ] Implement `WaylandWindow` using loaded functions
- [ ] Wayland event loop
- [ ] EGL context creation
- [ ] Event handling: mouse, keyboard, window events

**Deliverable:** Linux Wayland windows work, no linker dependencies

### Phase 6: Integration with azul-layout (Week 5-6)
**Goal:** Connect shell2 to azul-layout

- [ ] Create window → LayoutWindow integration
- [ ] Connect event loop to layout system
- [ ] Implement render callback from layout to compositor
- [ ] Test with simple UI examples
- [ ] Performance benchmarking

**Deliverable:** Complete working window system

### Phase 7: CPU Compositor (Week 6-7)
**Goal:** Implement software rendering fallback

- [ ] Study webrender's sw_compositor.rs
- [ ] Implement CPU rasterizer for DisplayList
- [ ] Scanline rendering, blending, clipping
- [ ] Text rendering fallback
- [ ] Runtime mode switching
- [ ] Benchmark CPU vs GPU performance

**Deliverable:** Can render without GPU

### Phase 8: Advanced Features (Week 7-8)
**Goal:** Polish and advanced features

- [ ] Multiple window support
- [ ] Window decorations customization
- [ ] Drag and drop
- [ ] Clipboard integration
- [ ] High DPI support
- [ ] Window icons
- [ ] System tray icons
- [ ] Notifications

**Deliverable:** Feature-complete windowing system

## Testing Strategy

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    #[test]
    fn test_dynamic_library_loading() {
        #[cfg(target_os = "linux")]
        {
            let lib = X11Library::load().expect("Failed to load libX11.so");
            // Verify function pointers are non-null
        }
    }
    
    #[test]
    fn test_compositor_mode_selection() {
        let mode = CompositorMode::Auto;
        let resolved = resolve_compositor_mode(mode);
        assert!(matches!(resolved, CompositorMode::GPU | CompositorMode::CPU));
    }
}
```

### Integration Tests
```rust
#[test]
fn test_window_creation() {
    let window = PlatformWindow::new(WindowCreateOptions::default())
        .expect("Failed to create window");
    assert!(window.get_state().is_open);
}

#[test]
fn test_event_loop() {
    let mut window = create_test_window();
    let mut events = Vec::new();
    
    for _ in 0..100 {
        if let Some(event) = window.poll_event() {
            events.push(event);
        }
    }
    
    assert!(events.len() > 0);
}
```

### Platform-Specific Tests
```rust
#[cfg(target_os = "macos")]
#[test]
fn test_macos_specific_features() {
    // Menu bar, dock integration, etc.
}

#[cfg(all(target_os = "linux", feature = "x11"))]
#[test]
fn test_x11_dynamic_loading() {
    // Verify all X11 functions load correctly
}
```

## Error Handling

```rust
#[derive(Debug)]
pub enum WindowError {
    /// Platform-specific error
    PlatformError(String),
    /// Failed to create rendering context
    ContextCreationFailed,
    /// Window was closed
    WindowClosed,
    /// Invalid window state
    InvalidState,
}

#[derive(Debug)]
pub enum CompositorError {
    /// GPU not available
    NoGPU,
    /// Shader compilation failed
    ShaderError(String),
    /// Out of memory
    OutOfMemory,
    /// Context lost (GPU reset)
    ContextLost,
}

#[derive(Debug)]
pub enum DlError {
    /// Library file not found
    LibraryNotFound(String),
    /// Symbol not found in library
    SymbolNotFound(String),
    /// Invalid library format
    InvalidLibrary,
}
```

## Performance Considerations

### Memory Management
- Pool window events to avoid allocations
- Reuse compositor framebuffers across frames
- Cache loaded library handles globally

### Threading
- Main thread: Event loop and window management
- Render thread: Compositor and GPU commands (optional)
- Layout thread: Can run layout off main thread

### Hot Paths
- Event polling: < 1µs per event
- Frame presentation: < 16ms for 60fps
- Mode switching: < 100ms to switch CPU ↔ GPU

## Migration Path

### From old shell to shell2

```rust
// Old code (reference only, don't modify)
dll/src/desktop/shell/  
    ├── appkit/
    ├── win32/
    └── x11/

// New code (clean implementation)
dll/src/desktop/shell2/
    ├── macos/
    ├── windows/
    └── linux/

// In dll/src/desktop/mod.rs:
#[cfg(feature = "shell2")]
pub mod shell2;

#[cfg(not(feature = "shell2"))]
pub mod shell;  // Old code, deprecated

// Users can opt-in with:
// cargo build --features shell2
```

### Feature Flag Strategy
```toml
[features]
default = ["shell2"]
shell2 = []  # New implementation
shell-legacy = []  # Old implementation (fallback)

# Platform backends
x11 = []
wayland = []
win32 = []
appkit = []
```

## Future: Custom WebRender Integration

Once shell2 is stable, replace azul-webrender with custom-webrender:

```rust
// Current: azul-webrender (fixed GPU or CPU)
extern crate webrender;

// Future: custom-webrender (per-window choice)
extern crate custom_webrender;

impl Compositor for CustomWebRenderCompositor {
    fn new(context: RenderContext, mode: CompositorMode) -> Result<Self, CompositorError> {
        match mode {
            CompositorMode::GPU => {
                // Use webrender GPU backend
                let renderer = Renderer::new_gpu(context)?;
                Ok(Self::GPU(renderer))
            }
            CompositorMode::CPU => {
                // Use webrender sw_compositor
                let renderer = Renderer::new_cpu(context)?;
                Ok(Self::CPU(renderer))
            }
            CompositorMode::Auto => {
                // Try GPU first, fall back to CPU
                Self::new(context, CompositorMode::GPU)
                    .or_else(|_| Self::new(context, CompositorMode::CPU))
            }
        }
    }
}
```

## Success Metrics

### Phase 1-6 (Core Implementation)
- ✅ All platforms compile without system library linking
- ✅ Windows open and close without crashes
- ✅ Events are delivered correctly
- ✅ Simple UI renders at 60fps
- ✅ Memory usage < 50MB per window

### Phase 7-8 (Advanced Features)
- ✅ CPU compositor renders correctly (visual regression tests)
- ✅ Mode switching works seamlessly
- ✅ High DPI works on all platforms
- ✅ Drag and drop works
- ✅ Clipboard works

## Risk Mitigation

### Risk: Dynamic loading adds complexity
**Mitigation:** 
- Provide helper macros for function loading
- Comprehensive error messages
- Fallback to static linking on macOS

### Risk: CPU compositor too slow
**Mitigation:**
- Profile early with representative workloads
- SIMD optimizations for hot paths
- Tile-based rendering to limit per-frame work

### Risk: Platform API differences
**Mitigation:**
- Clean trait abstraction hides differences
- Platform-specific tests for each backend
- Document platform limitations clearly

### Risk: Maintenance burden
**Mitigation:**
- Excellent documentation
- CI tests on all platforms
- Small, focused modules
- Keep old shell as reference

## Documentation Plan

### User Documentation
- [ ] Platform-specific build instructions
- [ ] How to choose CPU vs GPU compositor
- [ ] Performance tuning guide
- [ ] Troubleshooting common issues

### Developer Documentation
- [ ] Architecture overview
- [ ] Adding a new platform backend
- [ ] Debugging window system issues
- [ ] Contributing guidelines

### API Documentation
- [ ] Rustdoc for all public APIs
- [ ] Usage examples
- [ ] Migration guide from shell to shell2

## Timeline Summary

| Phase | Duration | Deliverable |
|-------|----------|-------------|
| Phase 1 | 1 week | Core infrastructure |
| Phase 2 | 1 week | macOS working |
| Phase 3 | 1 week | Linux X11 working |
| Phase 4 | 1 week | Windows working |
| Phase 5 | 1 week | Linux Wayland working |
| Phase 6 | 1 week | Integration complete |
| Phase 7 | 1 week | CPU compositor |
| Phase 8 | 1 week | Advanced features |
| **TOTAL** | **8 weeks** | **Production-ready** |

## Next Steps

1. Create `dll/src/desktop/shell2/` directory
2. Define core traits in `shell2/common/`
3. Start Phase 1 implementation
4. Set up CI for multi-platform testing
5. Write initial documentation

---

**Status:** Planning Complete - Ready for Implementation
**Owner:** TBD
**Started:** 18. Oktober 2025
**Target Completion:** December 2025
