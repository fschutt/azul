# Platform Features and Implementation Strategy

## Date: 18. Oktober 2025

## Overview
Comprehensive feature matrix for all supported platforms and implementation strategy. Focus on dynamic library loading (except macOS) to avoid linker errors and ensure maximum compatibility.

---

## Platform Support Matrix

### Core Platforms

| Platform | Backend(s) | Dynamic Loading | Compositor | Status |
|----------|-----------|-----------------|------------|--------|
| **macOS** | AppKit/Cocoa | ❌ (static link) | Metal, OpenGL | Priority 1 |
| **Windows** | Win32 | ✅ LoadLibrary | D3D11, OpenGL | Priority 1 |
| **Linux** | X11, Wayland | ✅ dlopen | OpenGL, Vulkan | Priority 1 |

### Future Platforms

| Platform | Backend(s) | Dynamic Loading | Compositor | Status |
|----------|-----------|-----------------|------------|--------|
| **Web** | WebAssembly | N/A | WebGL, Canvas | Future |
| **Android** | NDK | ✅ dlopen | OpenGL ES, Vulkan | Future |
| **iOS** | UIKit | ❌ (static link) | Metal | Future |

---

## Feature Requirements by Platform

### 1. macOS (AppKit/Cocoa)

#### Required System Frameworks
- **AppKit.framework** - Window management, menus, events
- **Foundation.framework** - Basic data types, strings
- **CoreGraphics.framework** - Graphics primitives
- **Metal.framework** - GPU rendering (modern)
- **OpenGL.framework** - GPU rendering (legacy)
- **QuartzCore.framework** - Core Animation

#### Linking Strategy
**Static Linking (Standard macOS Approach)**
```toml
[target.'cfg(target_os = "macos")'.dependencies]
# No special linking needed - frameworks linked by default
```

**Rationale:** 
- macOS system frameworks are always present and versioned
- No compatibility issues like on Linux
- Apple recommends static linking for system frameworks
- Dynamic loading adds complexity without benefit

#### Window Management
- ✅ Create/destroy windows
- ✅ Set window title
- ✅ Resize windows
- ✅ Move windows
- ✅ Minimize/maximize/fullscreen
- ✅ Multiple windows
- ✅ Window decorations (native)
- ✅ Window transparency
- ✅ Modal windows
- ✅ Window level (floating, normal, etc.)

#### Event Handling
- ✅ Mouse events (move, click, scroll, drag)
- ✅ Keyboard events (press, release, modifiers)
- ✅ Window events (resize, move, focus, close)
- ✅ Application events (activate, deactivate, quit)
- ✅ Trackpad gestures (pinch, rotate, swipe)
- ✅ High precision scrolling

#### Menu & UI Integration
- ✅ Menu bar (native macOS menu)
- ✅ Dock integration
- ✅ Dock menu
- ✅ Dock badge
- ✅ Application icon
- ✅ Notifications (NSUserNotification)
- ✅ File dialogs (NSOpenPanel, NSSavePanel)
- ✅ Color picker (NSColorPanel)

#### Rendering
- ✅ Metal compositor (modern, preferred)
- ✅ OpenGL compositor (legacy, deprecated by Apple)
- ✅ CPU compositor (software fallback)
- ✅ High DPI (Retina) support
- ✅ VSync control
- ✅ Hardware acceleration

#### System Integration
- ✅ Clipboard (NSPasteboard)
- ✅ Drag and drop (NSDraggingDestination)
- ✅ System appearance (light/dark mode)
- ✅ Accessibility (VoiceOver)
- ✅ System fonts
- ✅ Native file pickers

---

### 2. Windows (Win32)

#### Required System DLLs
- **user32.dll** - Window management, messages, input
- **kernel32.dll** - System functions, memory, threads
- **gdi32.dll** - Graphics Device Interface
- **opengl32.dll** - OpenGL rendering
- **d3d11.dll** - Direct3D 11 rendering
- **dwmapi.dll** - Desktop Window Manager (Aero)
- **shell32.dll** - Shell functions, dialogs
- **ole32.dll** - OLE, drag and drop
- **comctl32.dll** - Common controls

#### Linking Strategy
**Dynamic Loading (dlopen via LoadLibrary)**

```rust
// dll/src/desktop/shell2/windows/dlopen.rs

pub struct Win32Libraries {
    user32: HMODULE,
    kernel32: HMODULE,
    gdi32: HMODULE,
    opengl32: Option<HMODULE>,
    d3d11: Option<HMODULE>,
}

impl Win32Libraries {
    pub fn load() -> Result<Self, DlError> {
        unsafe {
            let user32 = LoadLibraryW(w!("user32.dll"));
            let kernel32 = LoadLibraryW(w!("kernel32.dll"));
            let gdi32 = LoadLibraryW(w!("gdi32.dll"));
            
            // Optional: Try to load graphics libraries
            let opengl32 = LoadLibraryW(w!("opengl32.dll"));
            let d3d11 = LoadLibraryW(w!("d3d11.dll"));
            
            Ok(Self {
                user32,
                kernel32,
                gdi32,
                opengl32: NonNull::new(opengl32),
                d3d11: NonNull::new(d3d11),
            })
        }
    }
    
    // Load function pointers
    pub fn get_create_window_ex_w(&self) -> CreateWindowExWFn {
        unsafe {
            let ptr = GetProcAddress(self.user32, c"CreateWindowExW");
            std::mem::transmute(ptr)
        }
    }
    
    // ... load all other functions
}
```

**Benefits:**
- ✅ Works on Windows 7, 8, 10, 11 without recompilation
- ✅ No linker errors due to missing DLLs
- ✅ Graceful fallback if optional DLLs missing (e.g., d3d11.dll)
- ✅ Supports different Windows versions elegantly

**Rationale:**
- Windows DLL versions vary significantly across OS versions
- Static linking causes "missing DLL" errors for users
- Dynamic loading = maximum compatibility

#### Window Management
- ✅ Create/destroy windows (CreateWindowExW)
- ✅ Set window title (SetWindowTextW)
- ✅ Resize windows (SetWindowPos)
- ✅ Move windows (SetWindowPos)
- ✅ Minimize/maximize/fullscreen (ShowWindow)
- ✅ Multiple windows
- ✅ Window decorations (WS_CAPTION, WS_BORDER, etc.)
- ✅ Window transparency (WS_EX_LAYERED)
- ✅ Modal windows (EnableWindow)
- ✅ Window styles (WS_POPUP, WS_CHILD, etc.)

#### Event Handling
- ✅ Mouse events (WM_MOUSEMOVE, WM_LBUTTONDOWN, etc.)
- ✅ Keyboard events (WM_KEYDOWN, WM_KEYUP, WM_CHAR)
- ✅ Window events (WM_SIZE, WM_MOVE, WM_CLOSE)
- ✅ Application events (WM_QUIT, WM_ACTIVATE)
- ✅ Mouse wheel (WM_MOUSEWHEEL)
- ✅ Touch events (WM_TOUCH) - Windows 7+
- ✅ High precision mouse (WM_INPUT)

#### Menu & UI Integration
- ✅ Menu bar (CreateMenu, AppendMenuW)
- ✅ Context menus (TrackPopupMenu)
- ✅ System tray icon (Shell_NotifyIconW)
- ✅ Taskbar integration
- ✅ Jump lists (Windows 7+)
- ✅ Notifications (toast notifications)
- ✅ File dialogs (IFileDialog COM interface)
- ✅ Color picker (ChooseColorW)

#### Rendering
- ✅ Direct3D 11 compositor (modern, preferred)
- ✅ OpenGL compositor (legacy, widely compatible)
- ✅ CPU compositor (software fallback)
- ✅ High DPI support (SetProcessDpiAwarenessContext)
- ✅ VSync control (IDXGISwapChain::Present)
- ✅ Hardware acceleration

#### System Integration
- ✅ Clipboard (OpenClipboard, GetClipboardData)
- ✅ Drag and drop (IDropTarget COM interface)
- ✅ System theme (dark/light mode) - Windows 10+
- ✅ Accessibility (UI Automation)
- ✅ System fonts (GDI, DirectWrite)
- ✅ Native file pickers (IFileDialog)

---

### 3. Linux (X11)

#### Required System Libraries
- **libX11.so** - X11 core protocol
- **libXext.so** - X11 extensions
- **libXrender.so** - X Render extension
- **libXrandr.so** - X Resize and Rotate
- **libXcursor.so** - X Cursor management
- **libXi.so** - X Input extension
- **libGL.so** - OpenGL rendering
- **libvulkan.so** - Vulkan rendering (optional)
- **libxcb.so** - X C Bindings (optional, for modern code)

#### Linking Strategy
**Dynamic Loading (dlopen)**

```rust
// dll/src/desktop/shell2/linux/x11/dlopen.rs

pub struct X11Libraries {
    libx11: *mut c_void,
    libxext: *mut c_void,
    libgl: *mut c_void,
    libvulkan: Option<*mut c_void>,
}

impl X11Libraries {
    pub fn load() -> Result<Self, DlError> {
        unsafe {
            // Try different library names for compatibility
            let libx11 = dlopen_first_available(&[
                "libX11.so.6",
                "libX11.so",
            ])?;
            
            let libxext = dlopen_first_available(&[
                "libXext.so.6",
                "libXext.so",
            ])?;
            
            let libgl = dlopen_first_available(&[
                "libGL.so.1",
                "libGL.so",
            ])?;
            
            // Optional: Vulkan
            let libvulkan = dlopen_first_available(&[
                "libvulkan.so.1",
                "libvulkan.so",
            ]).ok();
            
            Ok(Self {
                libx11,
                libxext,
                libgl,
                libvulkan,
            })
        }
    }
    
    pub fn get_xopen_display(&self) -> XOpenDisplayFn {
        unsafe {
            let ptr = dlsym(self.libx11, b"XOpenDisplay\0".as_ptr() as _);
            std::mem::transmute(ptr)
        }
    }
    
    // ... load all other functions
}

// Helper to try multiple library names
unsafe fn dlopen_first_available(names: &[&str]) -> Result<*mut c_void, DlError> {
    for name in names {
        let cname = CString::new(*name).unwrap();
        let handle = dlopen(cname.as_ptr(), RTLD_LAZY);
        if !handle.is_null() {
            return Ok(handle);
        }
    }
    Err(DlError::LibraryNotFound(names[0].to_string()))
}
```

**Benefits:**
- ✅ Works on Ubuntu, Fedora, Arch, Debian, etc. without recompilation
- ✅ No linker errors due to different library versions
- ✅ Handles different library names (.so.6 vs .so)
- ✅ Graceful fallback if optional libraries missing

**Rationale:**
- Linux distributions have vastly different library versions
- `/usr/lib/libX11.so.6` on Ubuntu, `/usr/lib64/libX11.so.6` on Fedora
- Static linking = "version GLIBC_2.XX not found" errors
- Dynamic loading = universal compatibility

#### Window Management
- ✅ Create/destroy windows (XCreateWindow)
- ✅ Set window title (XStoreName)
- ✅ Resize windows (XResizeWindow)
- ✅ Move windows (XMoveWindow)
- ✅ Minimize/maximize/fullscreen (_NET_WM_STATE)
- ✅ Multiple windows
- ✅ Window decorations (window manager hints)
- ✅ Window transparency (ARGB visual)
- ✅ Modal windows (_NET_WM_WINDOW_TYPE)
- ✅ Window urgency hints

#### Event Handling
- ✅ Mouse events (ButtonPress, ButtonRelease, MotionNotify)
- ✅ Keyboard events (KeyPress, KeyRelease)
- ✅ Window events (ConfigureNotify, MapNotify, UnmapNotify)
- ✅ Focus events (FocusIn, FocusOut)
- ✅ Mouse wheel (ButtonPress with button 4/5)
- ✅ XInput2 for high precision input
- ✅ ClientMessage for WM protocols

#### Menu & UI Integration
- ❌ No native menu bar (application must draw)
- ✅ Context menus (application drawn)
- ✅ System tray icon (XEmbed protocol)
- ✅ Desktop notifications (D-Bus, libnotify)
- ✅ File dialogs (portal-based, via D-Bus)
- ✅ Custom window decorations

#### Rendering
- ✅ OpenGL compositor (GLX context)
- ✅ Vulkan compositor (optional)
- ✅ CPU compositor (software fallback via XImage)
- ✅ High DPI support (Xft.dpi, _NET_WM_SCALE_FACTOR)
- ✅ VSync control (GLX_EXT_swap_control)
- ✅ Hardware acceleration

#### System Integration
- ✅ Clipboard (XA_PRIMARY, XA_CLIPBOARD selections)
- ✅ Drag and drop (XDND protocol)
- ✅ System theme (GTK settings via gsettings)
- ✅ Accessibility (AT-SPI via D-Bus)
- ✅ System fonts (fontconfig)
- ✅ File pickers (XDG Desktop Portal via D-Bus)

---

### 4. Linux (Wayland)

#### Required System Libraries
- **libwayland-client.so** - Wayland client protocol
- **libwayland-egl.so** - Wayland EGL integration
- **libwayland-cursor.so** - Cursor support
- **libEGL.so** - EGL context creation
- **libGL.so** - OpenGL rendering
- **libxkbcommon.so** - Keyboard handling
- **libvulkan.so** - Vulkan rendering (optional)

#### Linking Strategy
**Dynamic Loading (dlopen)**

```rust
// dll/src/desktop/shell2/linux/wayland/dlopen.rs

pub struct WaylandLibraries {
    libwayland_client: *mut c_void,
    libwayland_egl: *mut c_void,
    libegl: *mut c_void,
    libgl: *mut c_void,
    libxkbcommon: *mut c_void,
    libvulkan: Option<*mut c_void>,
}

impl WaylandLibraries {
    pub fn load() -> Result<Self, DlError> {
        unsafe {
            let libwayland_client = dlopen_first_available(&[
                "libwayland-client.so.0",
                "libwayland-client.so",
            ])?;
            
            let libwayland_egl = dlopen_first_available(&[
                "libwayland-egl.so.1",
                "libwayland-egl.so",
            ])?;
            
            let libegl = dlopen_first_available(&[
                "libEGL.so.1",
                "libEGL.so",
            ])?;
            
            let libgl = dlopen_first_available(&[
                "libGL.so.1",
                "libGL.so",
            ])?;
            
            let libxkbcommon = dlopen_first_available(&[
                "libxkbcommon.so.0",
                "libxkbcommon.so",
            ])?;
            
            let libvulkan = dlopen_first_available(&[
                "libvulkan.so.1",
                "libvulkan.so",
            ]).ok();
            
            Ok(Self {
                libwayland_client,
                libwayland_egl,
                libegl,
                libgl,
                libxkbcommon,
                libvulkan,
            })
        }
    }
    
    pub fn get_wl_display_connect(&self) -> WlDisplayConnectFn {
        unsafe {
            let ptr = dlsym(self.libwayland_client, b"wl_display_connect\0".as_ptr() as _);
            std::mem::transmute(ptr)
        }
    }
    
    // ... load all other functions
}
```

**Benefits:**
- ✅ Same as X11 - universal compatibility
- ✅ Works on GNOME, KDE, Sway, etc.
- ✅ Handles compositor-specific extensions gracefully

**Rationale:**
- Wayland is even more fragmented than X11
- Different compositors support different protocols
- Dynamic loading allows runtime protocol negotiation

#### Window Management
- ✅ Create/destroy surfaces (wl_surface)
- ✅ Set window title (xdg_toplevel.set_title)
- ✅ Resize windows (xdg_toplevel.set_size)
- ✅ Move windows (compositor decides position)
- ✅ Minimize/maximize/fullscreen (xdg_toplevel states)
- ✅ Multiple windows
- ✅ Window decorations (client-side or server-side)
- ✅ Window transparency (ARGB buffer)
- ✅ Modal windows (xdg_toplevel parent)
- ⚠️ No direct window positioning (compositor controlled)

#### Event Handling
- ✅ Mouse events (wl_pointer)
- ✅ Keyboard events (wl_keyboard with xkbcommon)
- ✅ Touch events (wl_touch)
- ✅ Window events (xdg_surface.configure)
- ✅ Focus events (wl_keyboard.enter/leave)
- ✅ Scroll events (wl_pointer.axis)
- ✅ High precision scrolling

#### Menu & UI Integration
- ❌ No native menu bar (client-side decorations)
- ✅ Context menus (application drawn)
- ❌ No system tray standard (compositor-dependent)
- ✅ Desktop notifications (D-Bus, portal)
- ✅ File dialogs (XDG Desktop Portal via D-Bus)
- ✅ Custom window decorations (mandatory on some compositors)

#### Rendering
- ✅ OpenGL compositor (EGL + wl_egl_window)
- ✅ Vulkan compositor (VK_KHR_wayland_surface)
- ✅ CPU compositor (wl_shm, software fallback)
- ✅ High DPI support (wl_output.scale)
- ✅ VSync (implicit, compositor controlled)
- ✅ Hardware acceleration

#### System Integration
- ✅ Clipboard (wl_data_device protocol)
- ✅ Drag and drop (wl_data_device)
- ✅ System theme (via D-Bus, gsettings)
- ✅ Accessibility (AT-SPI via D-Bus)
- ✅ System fonts (fontconfig)
- ✅ File pickers (XDG Desktop Portal)

---

## Platform Selection Logic

### Compile-Time Selection

```rust
// dll/src/desktop/shell2/mod.rs

cfg_if::cfg_if! {
    if #[cfg(target_os = "macos")] {
        pub mod macos;
        pub use macos::MacOSWindow as PlatformWindow;
    } else if #[cfg(target_os = "windows")] {
        pub mod windows;
        pub use windows::Win32Window as PlatformWindow;
    } else if #[cfg(target_os = "linux")] {
        pub mod linux;
        // Runtime selection between X11 and Wayland
        pub use linux::LinuxWindow as PlatformWindow;
    } else {
        compile_error!("Unsupported platform");
    }
}
```

### Runtime Selection (Linux Only)

```rust
// dll/src/desktop/shell2/linux/mod.rs

pub enum LinuxWindow {
    X11(X11Window),
    Wayland(WaylandWindow),
}

impl LinuxWindow {
    pub fn new(options: WindowCreateOptions) -> Result<Self, WindowError> {
        // Try Wayland first (modern), fall back to X11
        if let Ok(window) = WaylandWindow::new(options.clone()) {
            return Ok(Self::Wayland(window));
        }
        
        if let Ok(window) = X11Window::new(options) {
            return Ok(Self::X11(window));
        }
        
        Err(WindowError::NoBackendAvailable)
    }
}

// User can override via environment variable
// AZUL_BACKEND=x11 ./myapp  # Force X11
// AZUL_BACKEND=wayland ./myapp  # Force Wayland
```

---

## Dynamic Loading Implementation Details

### Function Pointer Storage

```rust
// Generic pattern for all platforms

pub struct DynamicFunctions {
    // Store raw pointers
    create_window: *const c_void,
    destroy_window: *const c_void,
    // ... all other functions
}

impl DynamicFunctions {
    // Safe wrapper functions
    pub fn create_window(&self, /* args */) -> Result<WindowHandle, Error> {
        unsafe {
            let f: CreateWindowFn = std::mem::transmute(self.create_window);
            let result = f(/* args */);
            // Error handling
            Ok(result)
        }
    }
}
```

### Error Handling for Missing Libraries

```rust
pub enum DlError {
    LibraryNotFound { 
        name: String,
        tried: Vec<String>,
        suggestion: String,
    },
    SymbolNotFound {
        symbol: String,
        library: String,
        suggestion: String,
    },
    VersionMismatch {
        found: String,
        required: String,
    },
}

impl fmt::Display for DlError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            DlError::LibraryNotFound { name, tried, suggestion } => {
                write!(f, "Failed to load library '{}'.\nTried: {:?}\n\nSuggestion: {}",
                    name, tried, suggestion)
            }
            // ... other variants
        }
    }
}

// Example error messages:
// "Failed to load library 'libX11.so'.
//  Tried: ["libX11.so.6", "libX11.so"]
//  
//  Suggestion: Install X11 development libraries:
//    Ubuntu/Debian: sudo apt install libx11-dev
//    Fedora: sudo dnf install libX11-devel
//    Arch: sudo pacman -S libx11"
```

### Version Checking

```rust
// Verify library versions at runtime
impl X11Libraries {
    pub fn verify_version(&self) -> Result<(), DlError> {
        unsafe {
            let version_fn: XVersionFn = self.get_symbol("XProtocolVersion")?;
            let version = version_fn();
            
            if version < MINIMUM_X11_VERSION {
                return Err(DlError::VersionMismatch {
                    found: format!("{}", version),
                    required: format!("{}", MINIMUM_X11_VERSION),
                });
            }
            
            Ok(())
        }
    }
}
```

---

## Compositor Selection Strategy

### Automatic Selection

```rust
pub fn select_compositor(
    platform: Platform,
    requested: CompositorMode,
    capabilities: &SystemCapabilities,
) -> CompositorMode {
    match requested {
        CompositorMode::Auto => {
            // macOS: Prefer Metal, fallback OpenGL
            if platform.is_macos() {
                if capabilities.has_metal() {
                    CompositorMode::GPU  // Metal
                } else {
                    CompositorMode::GPU  // OpenGL
                }
            }
            // Windows: Prefer D3D11, fallback OpenGL
            else if platform.is_windows() {
                if capabilities.has_d3d11() {
                    CompositorMode::GPU  // D3D11
                } else if capabilities.has_opengl() {
                    CompositorMode::GPU  // OpenGL
                } else {
                    CompositorMode::CPU
                }
            }
            // Linux: Prefer OpenGL, Vulkan optional
            else if platform.is_linux() {
                if capabilities.has_opengl() {
                    CompositorMode::GPU  // OpenGL
                } else if capabilities.has_vulkan() {
                    CompositorMode::GPU  // Vulkan
                } else {
                    CompositorMode::CPU
                }
            }
            else {
                CompositorMode::CPU  // Unknown platform
            }
        }
        CompositorMode::GPU => {
            if capabilities.has_any_gpu() {
                CompositorMode::GPU
            } else {
                // Fallback to CPU if GPU not available
                CompositorMode::CPU
            }
        }
        CompositorMode::CPU => CompositorMode::CPU,
    }
}
```

### User Override

```rust
// Environment variable override
// AZUL_COMPOSITOR=cpu ./myapp
// AZUL_COMPOSITOR=gpu ./myapp
// AZUL_COMPOSITOR=auto ./myapp (default)

pub fn get_compositor_mode_from_env() -> Option<CompositorMode> {
    std::env::var("AZUL_COMPOSITOR")
        .ok()
        .and_then(|s| match s.to_lowercase().as_str() {
            "cpu" => Some(CompositorMode::CPU),
            "gpu" => Some(CompositorMode::GPU),
            "auto" => Some(CompositorMode::Auto),
            _ => None,
        })
}
```

---

## Testing Strategy per Platform

### macOS Testing
```rust
#[cfg(target_os = "macos")]
mod macos_tests {
    #[test]
    fn test_appkit_window_creation() {
        let window = MacOSWindow::new(WindowCreateOptions::default()).unwrap();
        assert!(window.is_visible());
    }
    
    #[test]
    fn test_metal_compositor() {
        let compositor = MetalCompositor::new(/* context */).unwrap();
        // Render test frame
    }
    
    #[test]
    fn test_retina_scaling() {
        let window = MacOSWindow::new(/* options */).unwrap();
        let scale = window.get_scale_factor();
        assert!(scale == 1.0 || scale == 2.0);  // Retina or non-Retina
    }
}
```

### Windows Testing
```rust
#[cfg(target_os = "windows")]
mod windows_tests {
    #[test]
    fn test_win32_dynamic_loading() {
        let libs = Win32Libraries::load().unwrap();
        // Verify all function pointers are non-null
        assert!(!libs.user32.is_null());
    }
    
    #[test]
    fn test_d3d11_compositor() {
        let compositor = D3D11Compositor::new(/* context */).unwrap();
        // Render test frame
    }
    
    #[test]
    fn test_high_dpi() {
        let window = Win32Window::new(/* options */).unwrap();
        let scale = window.get_dpi_scale();
        assert!(scale >= 1.0 && scale <= 3.0);
    }
}
```

### Linux Testing
```rust
#[cfg(target_os = "linux")]
mod linux_tests {
    #[test]
    fn test_x11_dynamic_loading() {
        let libs = X11Libraries::load().unwrap();
        assert!(!libs.libx11.is_null());
    }
    
    #[test]
    fn test_wayland_dynamic_loading() {
        if let Ok(libs) = WaylandLibraries::load() {
            assert!(!libs.libwayland_client.is_null());
        }
        // OK if Wayland not available
    }
    
    #[test]
    fn test_backend_selection() {
        let window = LinuxWindow::new(WindowCreateOptions::default()).unwrap();
        // Should have selected X11 or Wayland
    }
}
```

---

## Distribution Guidelines

### Linux Distribution Packages

Users should NOT need to install development packages:

**Runtime dependencies only:**
```bash
# Ubuntu/Debian
apt install libx11-6 libxext6 libgl1

# Fedora
dnf install libX11 libXext mesa-libGL

# Arch
pacman -S libx11 libxext libgl
```

**NOT required (no -dev/-devel packages):**
```bash
# These are NOT needed because we use dlopen()
# ❌ libx11-dev
# ❌ libxext-dev
# ❌ libgl-dev
```

### Windows Distribution

No additional dependencies - all DLLs are included with Windows:
- ✅ user32.dll (always present)
- ✅ kernel32.dll (always present)
- ✅ gdi32.dll (always present)
- ✅ opengl32.dll (always present)

### macOS Distribution

No additional dependencies - all frameworks are included with macOS:
- ✅ AppKit.framework (always present)
- ✅ Foundation.framework (always present)
- ✅ Metal.framework (macOS 10.11+)
- ✅ OpenGL.framework (deprecated but present)

---

## Future Considerations

### Web (WebAssembly)
- Canvas 2D API for CPU rendering
- WebGL for GPU rendering
- Browser event handling
- No dynamic loading needed (browser provides APIs)

### Mobile (Android/iOS)
- Touch-first input model
- Lifecycle management (background/foreground)
- Different window management paradigms
- Platform-specific UI guidelines

### Embedded
- Framebuffer rendering (/dev/fb0)
- DRM/KMS for modern Linux systems
- No windowing system
- Direct GPU access

---

## Summary

| Feature | macOS | Windows | Linux X11 | Linux Wayland |
|---------|-------|---------|-----------|---------------|
| **Dynamic Loading** | ❌ Static | ✅ Yes | ✅ Yes | ✅ Yes |
| **GPU Compositor** | Metal, OpenGL | D3D11, OpenGL | OpenGL, Vulkan | OpenGL, Vulkan |
| **CPU Compositor** | ✅ Yes | ✅ Yes | ✅ Yes | ✅ Yes |
| **Multiple Windows** | ✅ Yes | ✅ Yes | ✅ Yes | ✅ Yes |
| **Native Menus** | ✅ Yes | ✅ Yes | ❌ No | ❌ No |
| **System Tray** | ✅ Dock | ✅ Yes | ✅ XEmbed | ⚠️ Limited |
| **Transparency** | ✅ Yes | ✅ Yes | ✅ Yes | ✅ Yes |
| **High DPI** | ✅ Retina | ✅ Yes | ✅ Yes | ✅ Yes |
| **Drag & Drop** | ✅ Yes | ✅ Yes | ✅ XDND | ✅ Yes |
| **Clipboard** | ✅ Yes | ✅ Yes | ✅ Yes | ✅ Yes |

**Priority Implementation Order:**
1. macOS (static linking, simpler)
2. Windows (dynamic loading pattern)
3. Linux X11 (reuse Windows patterns)
4. Linux Wayland (modernize X11 approach)

---

**Status:** Planning Complete - Ready for Implementation
**Next Steps:** Begin Phase 1 of SHELL2_IMPLEMENTATION_PLAN.md
