# Linux Shell2 Implementation - Detailed Improvement Plan

**Created**: 2025-10-27  
**Status**: Implementation Phase 2 - Filling in Stubs and Fixing Issues  
**Goal**: Complete the Linux X11 and Wayland implementations by addressing all stub implementations, missing features, and architectural issues identified in the initial implementation.

---

## Executive Summary

The initial Linux implementation from `actionplan-linux.md` has been copied into the codebase. This document identifies specific areas that need improvement, completion, or proper implementation according to the `SHELL2_COMPLETION_ACTION_PLAN.md` standards.

**Current Implementation Status**:
- **X11**: ~30% complete - Basic structure exists, many stubs
- **Wayland**: ~5% complete - Minimal stub only
- **Common**: ~20% complete - Basic decorations stub

---

## Phase 1: X11 Core Improvements (Priority: HIGH)

### 1.1 Complete X11Window Initialization and Lifecycle

**Files**: `dll/src/desktop/shell2/linux/x11/mod.rs`

**Current Issues**:
- [ ] `X11Window::new()` creates dummy `fc_cache` and `app_data` instead of accepting them as parameters
- [ ] Missing proper error handling for display connection failures
- [ ] `regenerate_layout()` method is referenced but not implemented
- [ ] `process_events()` method is called but not implemented
- [ ] Missing XKB keyboard state initialization
- [ ] No proper cleanup in `Drop` implementation

**Required Changes**:

```rust
impl X11Window {
    // Update signature to match Windows/macOS pattern
    pub fn new(
        options: WindowCreateOptions,
        fc_cache: Arc<FcFontCache>,
        app_data: Arc<RefCell<RefAny>>,
    ) -> Result<Self, WindowError> {
        // ... existing code ...
    }

    // Implement missing lifecycle methods
    fn regenerate_layout(&mut self) -> Result<(), WindowError> {
        if let Some(layout_window) = &mut self.layout_window {
            // 1. Get new DOM from app callbacks
            // 2. Rebuild display list
            // 3. Update WebRender
            // 4. Request hit test update
        }
        Ok(())
    }

    fn process_events(&mut self) {
        if let (Some(prev), Some(layout_window)) = 
            (&self.previous_window_state, &mut self.layout_window) {
            // 1. Call create_events_from_states()
            // 2. Call process_window_events_recursive_v2()
            // 3. Handle ProcessEventResult variants
        }
    }
}

impl Drop for X11Window {
    fn drop(&mut self) {
        // Clean up resources in correct order
        self.renderer.take();
        self.render_api.take();
        if let RenderMode::Gpu(_, _) = &self.render_mode {
            // Clean up EGL context
        }
        unsafe {
            (self.xlib.XDestroyWindow)(self.display, self.window);
            (self.xlib.XCloseDisplay)(self.display);
        }
    }
}
```

**Reference**: `dll/src/desktop/shell2/macos/mod.rs` lines 300-450 (MacOSWindow lifecycle)

**Estimated Time**: 1 day

---

### 1.2 Fix X11 dlopen Missing Functions

**File**: `dll/src/desktop/shell2/linux/x11/dlopen.rs`

**Current Issues**:
- [ ] Missing `XGetWindowAttributes` - needed for window position/size queries
- [ ] Missing `XMoveWindow` - needed for window positioning
- [ ] Missing `XResizeWindow` - needed for programmatic resize
- [ ] Missing `XSetWindowAttributes` - needed for window attribute changes
- [ ] Missing `XGetGeometry` - needed for size queries
- [ ] Missing `XTranslateCoordinates` - needed for coordinate conversion
- [ ] Missing `XQueryPointer` - needed for mouse position queries
- [ ] Missing `XDefineCursor` / `XUndefineCursor` - for cursor management
- [ ] Missing `XCreateFontCursor` / `XFreeCursor` - for standard cursors
- [ ] Missing XRandR functions for DPI detection
- [ ] Missing clipboard functions (`XSetSelectionOwner`, `XConvertSelection`, etc.)

**Required Additions**:

```rust
pub struct Xlib {
    // ... existing fields ...
    pub XGetWindowAttributes: XGetWindowAttributes,
    pub XMoveWindow: XMoveWindow,
    pub XResizeWindow: XResizeWindow,
    pub XGetGeometry: XGetGeometry,
    pub XTranslateCoordinates: XTranslateCoordinates,
    pub XQueryPointer: XQueryPointer,
    pub XDefineCursor: XDefineCursor,
    pub XUndefineCursor: XUndefineCursor,
    pub XCreateFontCursor: XCreateFontCursor,
    pub XFreeCursor: XFreeCursor,
    pub XSetSelectionOwner: XSetSelectionOwner,
    pub XGetSelectionOwner: XGetSelectionOwner,
    pub XConvertSelection: XConvertSelection,
    pub XGetWindowProperty: XGetWindowProperty,
    pub XDeleteProperty: XDeleteProperty,
}

// Add XRandR library for DPI detection
pub struct Xrandr {
    _lib: Library,
    pub XRRGetScreenResources: XRRGetScreenResources,
    pub XRRFreeScreenResources: XRRFreeScreenResources,
    pub XRRGetOutputInfo: XRRGetOutputInfo,
    pub XRRFreeOutputInfo: XRRFreeOutputInfo,
    pub XRRGetCrtcInfo: XRRGetCrtcInfo,
    pub XRRFreeCrtcInfo: XRRFreeCrtcInfo,
}

impl Xrandr {
    pub fn new() -> Result<Rc<Self>, DlError> {
        let lib = load_first_available(&["libXrandr.so.2", "libXrandr.so"])?;
        // Load functions...
    }
}
```

**Estimated Time**: 4 hours

---

### 1.3 Complete X11 Event Handling

**File**: `dll/src/desktop/shell2/linux/x11/events.rs`

**Current Issues**:
- [ ] `handle_mouse_button()` - Scroll wheel handling is incomplete (only delta, no accumulation)
- [ ] `handle_keyboard()` - No modifier key state tracking (Shift, Ctrl, Alt, Super)
- [ ] Missing `handle_focus_in()` / `handle_focus_out()` implementations
- [ ] No `SelectionNotify` handler for clipboard paste
- [ ] No `SelectionRequest` handler for clipboard copy
- [ ] No `PropertyNotify` handler for window state changes
- [ ] IME composition events are not tracked or exposed to UI
- [ ] No support for XI2 (XInput2) for better mouse/touch handling

**Required Improvements**:

```rust
// Add modifier key tracking
impl X11Window {
    fn update_modifier_state(&mut self, state: u32) {
        self.current_window_state.keyboard_state.shift_down = (state & ShiftMask) != 0;
        self.current_window_state.keyboard_state.ctrl_down = (state & ControlMask) != 0;
        self.current_window_state.keyboard_state.alt_down = (state & Mod1Mask) != 0;
        // Super key detection via XKB
    }
}

// Complete scroll accumulation
pub(super) fn handle_mouse_button(window: &mut X11Window, event: &XButtonEvent) {
    // ... existing code ...
    match event.button {
        4 => { // Scroll up
            if is_down {
                window.current_window_state.mouse_state.scroll_y += Au::from_px(1.0);
                // Don't reset scroll immediately - accumulate over frame
            }
        }
        5 => { // Scroll down
            if is_down {
                window.current_window_state.mouse_state.scroll_y -= Au::from_px(1.0);
            }
        }
        // ... rest ...
    }
}

// Add clipboard support
pub(super) fn handle_selection_notify(window: &mut X11Window, event: &XSelectionEvent) {
    // Read clipboard data from SelectionNotify event
    // Convert from UTF8_STRING to Rust String
    // Store in window.current_window_state.clipboard_contents
}

pub(super) fn handle_selection_request(window: &mut X11Window, event: &XSelectionRequestEvent) {
    // Respond to clipboard paste request
    // Send clipboard data via XChangeProperty + SelectionNotify
}
```

**Reference**: `dll/src/desktop/shell2/windows/mod.rs` lines 800-1200 (WindowProc message handlers)

**Estimated Time**: 2 days

---

### 1.4 Complete OpenGL/EGL Context Management

**File**: `dll/src/desktop/shell2/linux/x11/gl.rs`

**Current Issues**:
- [ ] Massive `load!` macro in `GlFunctions::load()` - this should use gl_context_loader properly
- [ ] No error checking after EGL calls (should check `eglGetError()`)
- [ ] No VSync control (missing `eglSwapInterval`)
- [ ] No context sharing for multi-window scenarios
- [ ] No fallback to GLX if EGL is not available
- [ ] OpenGL function loading is incomplete - should use existing `gl_context_loader` crate

**Required Changes**:

```rust
use gl_context_loader::GenericGlContext;

impl GlFunctions {
    pub fn initialize(egl: &Rc<Egl>) -> Result<Self, String> {
        // Use the existing gl_context_loader infrastructure
        let context = GenericGlContext::new(|name| {
            let c_name = CString::new(name).unwrap();
            unsafe { (egl.eglGetProcAddress)(c_name.as_ptr()) as *const _ }
        });
        
        Ok(Self {
            _opengl_lib_handle: None,
            functions: Rc::new(context),
        })
    }
}

impl GlContext {
    pub fn set_vsync(&self, enabled: bool) -> Result<(), WindowError> {
        let interval = if enabled { 1 } else { 0 };
        if unsafe { (self.egl.eglSwapInterval)(self.egl_display, interval) } == 0 {
            return Err(WindowError::PlatformError("eglSwapInterval failed".into()));
        }
        Ok(())
    }
    
    fn check_egl_error(&self, operation: &str) -> Result<(), WindowError> {
        let error = unsafe { (self.egl.eglGetError)() };
        if error != EGL_SUCCESS {
            return Err(WindowError::PlatformError(
                format!("{} failed with EGL error: 0x{:X}", operation, error)
            ));
        }
        Ok(())
    }
}
```

**Estimated Time**: 1 day

---

### 1.5 Implement Proper Client-Side Decorations

**File**: `dll/src/desktop/shell2/linux/x11/decorations.rs`

**Current Issues**:
- [ ] Stub implementation with incomplete DOM rendering
- [ ] `XGetWindowAttributes` is called but not defined in dlopen
- [ ] Window dragging logic is present but untested
- [ ] No window resizing support (corner/edge dragging)
- [ ] No shadow/border rendering
- [ ] Button hover states are tracked but not rendered properly
- [ ] Missing integration with actual window rendering pipeline

**Required Implementation**:

```rust
pub struct Decorations {
    pub is_dragging: bool,
    pub drag_start_pos: (i32, i32),
    pub window_start_pos: (i32, i32),
    pub is_resizing: bool,
    pub resize_edge: ResizeEdge,
    pub close_button_hover: bool,
    pub maximize_button_hover: bool,
    pub minimize_button_hover: bool,
}

pub enum ResizeEdge {
    None,
    Top, Bottom, Left, Right,
    TopLeft, TopRight, BottomLeft, BottomRight,
}

pub fn handle_decoration_event(window: &mut X11Window, event: &XEvent) -> bool {
    // ... existing drag logic ...
    
    // Add resize logic
    if window.decorations.is_resizing {
        // Calculate new size based on edge being dragged
        // Call XResizeWindow
        return true;
    }
    
    // Detect resize edge on button press
    if event.type_ == ButtonPress {
        if let Some(edge) = detect_resize_edge(window, bev.x, bev.y) {
            window.decorations.is_resizing = true;
            window.decorations.resize_edge = edge;
            return true;
        }
    }
    
    false
}

fn detect_resize_edge(window: &X11Window, x: i32, y: i32) -> Option<ResizeEdge> {
    const BORDER_WIDTH: i32 = 5;
    let w = window.current_window_state.size.dimensions.width;
    let h = window.current_window_state.size.dimensions.height;
    
    // Check corners first (they have priority)
    if x < BORDER_WIDTH && y < BORDER_WIDTH { return Some(ResizeEdge::TopLeft); }
    // ... etc for all edges
    
    None
}
```

**Reference**: GNOME Mutter source code for CSD implementation patterns

**Estimated Time**: 2 days

---

### 1.6 Implement Menu Support

**File**: `dll/src/desktop/shell2/linux/x11/menu.rs`

**Current Issues**:
- [ ] Complete stub - only placeholder structs
- [ ] DBus global menu integration is mentioned but not implemented
- [ ] Context menu window creation exists but doesn't render anything
- [ ] No menu bar rendering for fallback mode
- [ ] No menu item callback dispatch

**Required Implementation Options**:

**Option A: DBus Global Menu (GNOME/Unity)**
```rust
use dbus::{Connection, BusType, Message};

pub struct MenuManager {
    connection: Connection,
    menu_object_path: String,
    menu_items: BTreeMap<u32, CoreMenuCallback>,
    item_id_counter: AtomicU32,
}

impl MenuManager {
    pub fn new(app_name: &str) -> Result<Self, String> {
        let conn = Connection::get_private(BusType::Session)
            .map_err(|e| format!("DBus connection failed: {}", e))?;
        
        // Register with com.canonical.AppMenu.Registrar
        let menu_path = format!("/com/canonical/dbusmenu/{}", std::process::id());
        
        Ok(Self {
            connection: conn,
            menu_object_path: menu_path,
            menu_items: BTreeMap::new(),
            item_id_counter: AtomicU32::new(1),
        })
    }
    
    pub fn set_menu(&mut self, menu: &Menu) {
        // Convert Azul Menu to DBus menu structure
        // Export via com.canonical.dbusmenu interface
    }
    
    pub fn poll_events(&mut self) -> Vec<(u32, CoreMenuCallback)> {
        // Check for ItemActivated signals
        // Return list of callbacks to invoke
    }
}
```

**Option B: Fallback In-Window Menu Bar**
```rust
pub fn render_menu_bar(menu: &Menu) -> Dom {
    // Render menu bar as DOM elements
    // Similar to Windows menu implementation
    Dom::div()
        .with_inline_css_props(vec![/* menu bar styling */])
        .with_children(menu.items.iter().map(|item| {
            render_menu_item(item)
        }).collect())
}

pub fn show_popup_menu(
    parent: Window,
    display: *mut Display,
    xlib: &Rc<Xlib>,
    menu: &Menu,
    x: i32,
    y: i32,
) -> Result<Window, WindowError> {
    // Create override-redirect window for popup
    // Render menu items into window
    // Handle mouse events for selection
}
```

**Decision**: Implement both - try DBus first, fallback to in-window

**Estimated Time**: 3 days

---

### 1.7 Add DPI Detection and Multi-Monitor Support

**File**: `dll/src/desktop/shell2/linux/x11/monitor.rs` (NEW)

**Current Issues**:
- [ ] No DPI detection - hardcoded to 96 DPI
- [ ] No monitor enumeration
- [ ] No primary monitor detection
- [ ] Window doesn't adjust when moved between monitors

**Required Implementation**:

```rust
use super::dlopen::{Xlib, Xrandr};
use azul_core::window::Monitor;

pub struct MonitorManager {
    xlib: Rc<Xlib>,
    xrandr: Option<Rc<Xrandr>>,
    monitors: Vec<Monitor>,
}

impl MonitorManager {
    pub fn new(xlib: Rc<Xlib>, display: *mut Display) -> Self {
        let xrandr = Xrandr::new().ok();
        let mut manager = Self {
            xlib,
            xrandr,
            monitors: Vec::new(),
        };
        manager.refresh_monitors(display);
        manager
    }
    
    pub fn refresh_monitors(&mut self, display: *mut Display) {
        if let Some(xrandr) = &self.xrandr {
            // Use XRandR to enumerate monitors
            let resources = unsafe {
                (xrandr.XRRGetScreenResources)(display, root_window)
            };
            
            for i in 0..(*resources).noutput {
                let output_info = unsafe {
                    (xrandr.XRRGetOutputInfo)(display, resources, outputs[i])
                };
                
                // Calculate DPI from physical size
                let dpi = calculate_dpi(output_info);
                
                // Create Monitor struct
                self.monitors.push(Monitor {
                    name: get_output_name(output_info),
                    size: get_output_size(output_info),
                    dpi_scale_factor: DpiScaleFactor::new(dpi / 96.0),
                    // ...
                });
            }
        } else {
            // Fallback: Use XDisplayWidth/Height and physical size
            let dpi = detect_dpi_fallback(self.xlib, display);
            // Create single monitor entry
        }
    }
    
    pub fn get_monitor_for_window(&self, window: Window) -> Option<&Monitor> {
        // Query window position
        // Find which monitor contains window center
    }
}

fn calculate_dpi(output_info: *mut XRROutputInfo) -> f32 {
    let width_mm = unsafe { (*output_info).mm_width };
    let height_mm = unsafe { (*output_info).mm_height };
    let width_px = /* from mode info */;
    
    if width_mm > 0 {
        (width_px as f32 / width_mm as f32) * 25.4
    } else {
        96.0 // Default fallback
    }
}
```

**Reference**: `SHELL2_COMPLETION_ACTION_PLAN.md` Phase 3.6

**Estimated Time**: 1 day

---

### 1.8 Implement Clipboard Support

**File**: `dll/src/desktop/shell2/linux/x11/clipboard.rs` (NEW)

**Current Issues**:
- [ ] No clipboard implementation at all
- [ ] Missing X11 selection protocol handling

**Required Implementation**:

```rust
pub struct ClipboardManager {
    xlib: Rc<Xlib>,
    display: *mut Display,
    window: Window,
    clipboard_atom: Atom,
    utf8_string_atom: Atom,
    targets_atom: Atom,
    clipboard_contents: Option<String>,
}

impl ClipboardManager {
    pub fn new(xlib: Rc<Xlib>, display: *mut Display, window: Window) -> Self {
        let clipboard_atom = unsafe {
            (xlib.XInternAtom)(display, b"CLIPBOARD\0".as_ptr() as _, 0)
        };
        let utf8_string_atom = unsafe {
            (xlib.XInternAtom)(display, b"UTF8_STRING\0".as_ptr() as _, 0)
        };
        let targets_atom = unsafe {
            (xlib.XInternAtom)(display, b"TARGETS\0".as_ptr() as _, 0)
        };
        
        Self {
            xlib,
            display,
            window,
            clipboard_atom,
            utf8_string_atom,
            targets_atom,
            clipboard_contents: None,
        }
    }
    
    pub fn set_clipboard(&mut self, text: String) {
        self.clipboard_contents = Some(text);
        unsafe {
            (self.xlib.XSetSelectionOwner)(
                self.display,
                self.clipboard_atom,
                self.window,
                CurrentTime,
            );
        }
    }
    
    pub fn get_clipboard(&self) -> Option<String> {
        // Request clipboard data via XConvertSelection
        unsafe {
            (self.xlib.XConvertSelection)(
                self.display,
                self.clipboard_atom,
                self.utf8_string_atom,
                self.clipboard_atom, // Property to store result
                self.window,
                CurrentTime,
            );
        }
        // Result will arrive via SelectionNotify event
        None // Must be called asynchronously
    }
    
    pub fn handle_selection_request(&self, event: &XSelectionRequestEvent) {
        // Respond to paste request from another application
        if let Some(data) = &self.clipboard_contents {
            // Write data to requestor's property
            unsafe {
                (self.xlib.XChangeProperty)(
                    self.display,
                    event.requestor,
                    event.property,
                    self.utf8_string_atom,
                    8, // 8-bit data
                    PropModeReplace,
                    data.as_ptr(),
                    data.len() as i32,
                );
            }
        }
        
        // Send SelectionNotify to requestor
        let mut notify_event: XEvent = unsafe { std::mem::zeroed() };
        // ... fill notify_event ...
        unsafe {
            (self.xlib.XSendEvent)(
                self.display,
                event.requestor,
                0,
                0,
                &mut notify_event,
            );
        }
    }
    
    pub fn handle_selection_notify(&mut self, event: &XSelectionEvent) -> Option<String> {
        // Read clipboard data from property
        let mut actual_type = 0;
        let mut actual_format = 0;
        let mut nitems = 0;
        let mut bytes_after = 0;
        let mut prop: *mut u8 = std::ptr::null_mut();
        
        unsafe {
            (self.xlib.XGetWindowProperty)(
                self.display,
                self.window,
                self.clipboard_atom,
                0,
                1024, // Max length
                0, // Don't delete
                AnyPropertyType,
                &mut actual_type,
                &mut actual_format,
                &mut nitems,
                &mut bytes_after,
                &mut prop,
            );
        }
        
        if !prop.is_null() && actual_format == 8 {
            let slice = unsafe {
                std::slice::from_raw_parts(prop, nitems as usize)
            };
            let result = String::from_utf8_lossy(slice).into_owned();
            unsafe { libc::free(prop as *mut _); }
            return Some(result);
        }
        
        None
    }
}
```

**Reference**: `SHELL2_COMPLETION_ACTION_PLAN.md` Phase 3.8

**Estimated Time**: 1 day

---

### 1.9 Implement Cursor Management

**File**: `dll/src/desktop/shell2/linux/x11/cursor.rs` (NEW)

**Current Issues**:
- [ ] No cursor management at all
- [ ] `MouseCursorType` changes are ignored

**Required Implementation**:

```rust
use azul_core::window::MouseCursorType;
use super::dlopen::Xlib;
use super::defines::*;
use std::collections::HashMap;

pub struct CursorManager {
    xlib: Rc<Xlib>,
    display: *mut Display,
    window: Window,
    cursors: HashMap<MouseCursorType, Cursor>,
    current_cursor: Option<MouseCursorType>,
}

impl CursorManager {
    pub fn new(xlib: Rc<Xlib>, display: *mut Display, window: Window) -> Self {
        Self {
            xlib,
            display,
            window,
            cursors: HashMap::new(),
            current_cursor: None,
        }
    }
    
    pub fn set_cursor(&mut self, cursor_type: MouseCursorType) {
        if self.current_cursor == Some(cursor_type) {
            return; // Already set
        }
        
        let cursor = self.cursors.entry(cursor_type).or_insert_with(|| {
            self.create_cursor(cursor_type)
        });
        
        unsafe {
            (self.xlib.XDefineCursor)(self.display, self.window, *cursor);
            (self.xlib.XFlush)(self.display);
        }
        
        self.current_cursor = Some(cursor_type);
    }
    
    fn create_cursor(&self, cursor_type: MouseCursorType) -> Cursor {
        let cursor_id = match cursor_type {
            MouseCursorType::Default => 2,       // XC_left_ptr
            MouseCursorType::Pointer => 60,      // XC_hand2
            MouseCursorType::Text => 152,        // XC_xterm
            MouseCursorType::Wait => 150,        // XC_watch
            MouseCursorType::Crosshair => 34,    // XC_crosshair
            MouseCursorType::EWResize => 108,    // XC_sb_h_double_arrow
            MouseCursorType::NSResize => 116,    // XC_sb_v_double_arrow
            MouseCursorType::NESWResize => 12,   // XC_bottom_left_corner
            MouseCursorType::NWSEResize => 14,   // XC_bottom_right_corner
            MouseCursorType::Move => 52,         // XC_fleur
            MouseCursorType::NotAllowed => 0,    // XC_X_cursor
            _ => 2, // Default fallback
        };
        
        unsafe {
            (self.xlib.XCreateFontCursor)(self.display, cursor_id)
        }
    }
}

impl Drop for CursorManager {
    fn drop(&mut self) {
        for (_, cursor) in self.cursors.drain() {
            unsafe {
                (self.xlib.XFreeCursor)(self.display, cursor);
            }
        }
    }
}
```

**Reference**: `SHELL2_COMPLETION_ACTION_PLAN.md` Phase 3.9

**Estimated Time**: 4 hours

---

### 1.10 Integrate LayoutWindow and Rendering Pipeline

**File**: `dll/src/desktop/shell2/linux/x11/mod.rs`

**Current Issues**:
- [ ] `layout_window` field is `Option<LayoutWindow>` but never initialized
- [ ] No DOM generation or display list creation
- [ ] No WebRender integration
- [ ] `present()` doesn't actually render anything meaningful
- [ ] No frame callbacks or render loop timing

**Required Implementation**:

```rust
impl X11Window {
    pub fn new(...) -> Result<Self, WindowError> {
        // ... existing initialization ...
        
        // Initialize LayoutWindow
        let layout_window = LayoutWindow::new(
            options.create_callback,
            options.callback_data.clone(),
            fc_cache.clone(),
            app_data.clone(),
        );
        
        // ... rest of window setup ...
        
        let mut window = Self {
            layout_window: Some(layout_window),
            // ... rest of fields ...
        };
        
        // Generate initial layout
        window.regenerate_layout()?;
        
        Ok(window)
    }
    
    fn regenerate_layout(&mut self) -> Result<(), WindowError> {
        let layout_window = self.layout_window.as_mut()
            .ok_or(WindowError::PlatformError("No layout window".into()))?;
        
        // 1. Call layout callbacks to get new DOM
        let new_dom = layout_window.layout(
            &self.current_window_state,
            &self.image_cache,
            &self.renderer_resources,
        );
        
        // 2. Build display list from DOM
        let display_list = super::super::compositor2::build_display_list(
            &new_dom,
            &self.current_window_state,
        )?;
        
        // 3. Send to WebRender
        if let (Some(render_api), Some(document_id)) = 
            (&self.render_api, self.document_id) {
            let mut txn = webrender::Transaction::new();
            
            // Translate and send display list
            let wr_display_list = wr_translate2::translate_display_list(
                display_list,
                self.id_namespace.unwrap(),
            );
            
            txn.set_display_list(
                wr_translate2::translate_epoch(0),
                None,
                self.current_window_state.size.dimensions,
                wr_display_list,
                true,
            );
            
            render_api.send_transaction(
                wr_translate2::translate_document_id(document_id),
                txn,
            );
        }
        
        // 4. Request hit-tester update
        if let Some(render_api) = &self.render_api {
            self.hit_tester = Some(AsyncHitTester::Requested(
                render_api.request_hit_tester(
                    wr_translate2::translate_document_id(self.document_id.unwrap())
                )
            ));
        }
        
        Ok(())
    }
    
    fn render_frame(&mut self) -> Result<(), WindowError> {
        if let RenderMode::Gpu(gl_context, gl_functions) = &self.render_mode {
            gl_context.make_current();
            
            if let Some(renderer) = &mut self.renderer {
                // Wait for frame ready signal from WebRender
                let (lock, cvar) = &*self.new_frame_ready;
                let mut ready = lock.lock().unwrap();
                while !*ready {
                    ready = cvar.wait(ready).unwrap();
                }
                *ready = false;
                
                // Render the frame
                renderer.update();
                renderer.render(
                    self.current_window_state.size.get_physical_size().into(),
                    0, // buffer_age
                )?;
                
                // Swap buffers
                gl_context.swap_buffers()?;
            }
        }
        
        Ok(())
    }
}
```

**Reference**: `dll/src/desktop/shell2/macos/mod.rs` lines 600-800 (rendering pipeline)

**Estimated Time**: 2 days

---

### 1.11 Fix defines.rs Type Safety Issues

**File**: `dll/src/desktop/shell2/linux/x11/defines.rs`

**Current Issues**:
- [ ] Many stub structs with `_private: [u8; 0]` - should use proper opaque types
- [ ] Union types are not properly initialized
- [ ] Missing `#[repr(C)]` on some structs
- [ ] Keysym constants are incomplete
- [ ] Missing many event type constants

**Required Fixes**:

```rust
// Use proper opaque pointer pattern
#[repr(C)]
pub struct Display {
    _private: [u8; 0],
}

// Add missing event constants
pub const UnmapNotify: c_int = 18;
pub const MapNotify: c_int = 19;
pub const MapRequest: c_int = 20;
pub const ReparentNotify: c_int = 21;
pub const SelectionClear: c_int = 29;
pub const SelectionRequest: c_int = 30;
pub const SelectionNotify: c_int = 31;
pub const PropertyNotify: c_int = 28;

// Add missing keysyms for numpad
pub const XK_KP_0: u32 = 0xFFB0;
pub const XK_KP_1: u32 = 0xFFB1;
// ... etc

// Add missing atoms
pub const AnyPropertyType: c_ulong = 0;
pub const PropModeReplace: c_int = 0;
pub const PropModePrepend: c_int = 1;
pub const PropModeAppend: c_int = 2;

// Proper XEvent union initialization helper
impl XEvent {
    pub fn new_expose(display: *mut Display, window: Window) -> Self {
        let mut event: XEvent = unsafe { std::mem::zeroed() };
        unsafe {
            event.expose.type_ = Expose;
            event.expose.display = display;
            event.expose.window = window;
        }
        event
    }
}
```

**Estimated Time**: 4 hours

---

## Phase 2: Wayland Core Implementation (Priority: MEDIUM)

### 2.1 Complete Wayland Stub to Minimal Functional Implementation

**Files**: `dll/src/desktop/shell2/linux/wayland/*.rs`

**Current Status**: Almost complete stub - protocol definitions exist but window creation is not functional

**Required Steps** (in order):

1. **Complete dlopen.rs** (1 day)
   - Load libwayland-client.so.0
   - Load libwayland-egl.so.1  
   - Load libxkbcommon.so.0
   - Verify all function pointers are correct

2. **Fix defines.rs** (4 hours)
   - Verify all protocol interface structs
   - Add missing wl_interface definitions
   - Fix any type mismatches

3. **Implement WaylandWindow::new()** (2 days)
   - Connect to Wayland display
   - Bind global objects (compositor, seat, shm, xdg_wm_base)
   - Create xdg_surface and xdg_toplevel
   - Set up EGL context OR software buffer
   - Register all listeners

4. **Implement Event Loop** (2 days)
   - wl_display_dispatch for events
   - Process frame callbacks
   - Handle configure events
   - Update window state

5. **Implement Basic Rendering** (1 day)
   - EGL swap buffers path
   - OR software buffer path via wl_shm
   - Frame callbacks for vsync

**Total Estimated Time**: 6 days

**Note**: Due to the complexity and the fact that X11 works as fallback, Wayland implementation can be deferred until X11 is stable.

---

## Phase 3: Common Features

### 3.1 Improve common/decorations.rs

**File**: `dll/src/desktop/shell2/linux/common/decorations.rs`

**Current Issues**:
- [ ] Stub implementation with placeholder colors
- [ ] No actual button rendering
- [ ] Callbacks are defined but don't properly communicate with window state
- [ ] Missing title text rendering

**Required Improvements**:

```rust
use azul_css::*;

const TITLE_BAR_HEIGHT: f32 = 32.0;
const BUTTON_WIDTH: f32 = 46.0;

pub fn render_decorations(title: &str, state: &DecorationsState) -> Dom {
    Dom::div()
        .with_id("csd-titlebar")
        .with_inline_style("width: 100%; height: 32px; background: #2d2d2d; display: flex; align-items: center;")
        .with_children(vec![
            // App icon (optional)
            render_app_icon(),
            
            // Title
            Dom::text(title)
                .with_inline_style("flex: 1; color: #eeeeee; font-size: 13px; padding-left: 12px;"),
            
            // Minimize button
            render_button("─", state.minimize_button_hover, on_minimize_click),
            
            // Maximize button  
            render_button(
                if state.is_maximized { "▢" } else { "□" },
                state.maximize_button_hover,
                on_maximize_click
            ),
            
            // Close button
            render_button("✕", state.close_button_hover, on_close_click)
                .with_inline_style("background: #c42b1c;") // Red close button
        ])
}

fn render_button(
    text: &str,
    is_hovered: bool,
    callback: extern "C" fn(&mut RefAny, &mut CallbackInfo) -> Update
) -> Dom {
    Dom::div()
        .with_inline_style(&format!(
            "width: {}px; height: 32px; background: {}; display: flex; align-items: center; justify-content: center; color: white; cursor: pointer;",
            BUTTON_WIDTH,
            if is_hovered { "#404040" } else { "transparent" }
        ))
        .with_child(Dom::text(text))
        .with_callback(/* mouse hover/click callbacks */)
}
```

**Estimated Time**: 1 day

---

## Phase 4: Testing and Validation

### 4.1 Create X11 Test Suite

**File**: `dll/tests/test_x11_window.rs` (NEW)

**Required Tests**:
- [ ] Window creation and destruction
- [ ] Keyboard input (all key types)
- [ ] Mouse input (buttons, motion, scroll)
- [ ] Window resize and move
- [ ] Clipboard copy and paste
- [ ] Cursor changes
- [ ] DPI detection
- [ ] Multi-monitor setup
- [ ] Focus handling
- [ ] IME composition

**Estimated Time**: 2 days

---

### 4.2 Cross-Distribution Testing

**Environments to Test**:
- [ ] Ubuntu 24.04 LTS (GNOME + X11)
- [ ] Ubuntu 24.04 LTS (GNOME + Wayland)
- [ ] Fedora 40 (GNOME + Wayland)
- [ ] Arch Linux (KDE Plasma + X11)
- [ ] Arch Linux (KDE Plasma + Wayland)
- [ ] Debian 12 (XFCE + X11)
- [ ] Pop!_OS 22.04 (COSMIC + X11)
- [ ] Linux Mint 21 (Cinnamon + X11)

**Test Matrix per Environment**:
- Window creation
- Rendering (GPU and CPU fallback)
- Input events
- Clipboard
- File drag and drop
- Menu bar (DBus and fallback)
- HiDPI scaling

**Estimated Time**: 3 days

---

### 4.3 Performance Profiling

**Areas to Profile**:
- [ ] Event loop overhead (should be <1% CPU when idle)
- [ ] Frame rendering time (should be <16ms for 60 FPS)
- [ ] Memory leaks (valgrind check)
- [ ] X11 protocol traffic (xscope analysis)

**Estimated Time**: 1 day

---

## Phase 5: Documentation and Cleanup

### 5.1 Add Comprehensive Documentation

**Files to Document**:
- [ ] `linux/mod.rs` - Backend selection logic
- [ ] `linux/x11/mod.rs` - X11Window lifecycle
- [ ] `linux/x11/events.rs` - Event handling patterns
- [ ] `linux/x11/gl.rs` - EGL setup and common issues

**Documentation Standards**:
- Module-level documentation explaining architecture
- Function-level documentation for public API
- Example code for common use cases
- Troubleshooting section for common issues

**Estimated Time**: 1 day

---

### 5.2 Code Cleanup

**Tasks**:
- [ ] Remove all eprintln! debug logging (use log crate)
- [ ] Add proper error types (replace String errors)
- [ ] Fix all clippy warnings
- [ ] Run rustfmt on all files
- [ ] Remove unused imports
- [ ] Remove dead code

**Estimated Time**: 4 hours

---

## Summary of Estimated Times

| Phase | Description | Time |
|-------|-------------|------|
| 1.1 | X11Window lifecycle | 1 day |
| 1.2 | dlopen missing functions | 4 hours |
| 1.3 | Event handling completion | 2 days |
| 1.4 | OpenGL/EGL fixes | 1 day |
| 1.5 | Client-side decorations | 2 days |
| 1.6 | Menu support | 3 days |
| 1.7 | DPI and monitors | 1 day |
| 1.8 | Clipboard | 1 day |
| 1.9 | Cursor management | 4 hours |
| 1.10 | LayoutWindow integration | 2 days |
| 1.11 | defines.rs fixes | 4 hours |
| **Phase 1 Total** | **X11 Core** | **~15 days** |
| 2.1 | Wayland implementation | 6 days |
| **Phase 2 Total** | **Wayland** | **~6 days** |
| 3.1 | Common decorations | 1 day |
| **Phase 3 Total** | **Common** | **~1 day** |
| 4.1-4.3 | Testing | 6 days |
| **Phase 4 Total** | **Testing** | **~6 days** |
| 5.1-5.2 | Documentation | 1.5 days |
| **Phase 5 Total** | **Docs** | **~1.5 days** |
| **GRAND TOTAL** | | **~29.5 days** |

---

## Priority Order for Implementation

1. **X11Window lifecycle and LayoutWindow integration** (1.1, 1.10) - 3 days - CRITICAL
2. **Event handling completion** (1.3) - 2 days - HIGH  
3. **OpenGL/EGL fixes** (1.4) - 1 day - HIGH
4. **DPI and monitors** (1.7) - 1 day - MEDIUM
5. **Clipboard** (1.8) - 1 day - MEDIUM
6. **Cursor management** (1.9) - 4 hours - MEDIUM
7. **dlopen missing functions** (1.2) - 4 hours - LOW (as needed)
8. **Client-side decorations** (1.5) - 2 days - LOW (nice to have)
9. **Menu support** (1.6) - 3 days - LOW (can be deferred)
10. **Wayland** (2.1) - 6 days - DEFERRED (X11 works as fallback)

---

## Success Criteria

**Minimum Viable Product (MVP)**:
- ✅ Window creates and displays
- ✅ Keyboard and mouse input works
- ✅ Rendering displays UI correctly
- ✅ Window can be moved and resized
- ✅ Basic clipboard copy/paste works
- ✅ Compiles and runs on major distributions

**Full Feature Parity**:
- ✅ All MVP features
- ✅ Client-side decorations work
- ✅ Menu bar works (DBus or fallback)
- ✅ Multi-monitor support with DPI detection
- ✅ IME composition works
- ✅ No memory leaks
- ✅ Performance matches macOS/Windows implementations

---

## Notes

1. **X11 vs Wayland**: Focus on X11 first as it's the universal fallback. Wayland can be implemented later since detection and fallback is already in place.

2. **Testing Strategy**: Test early and often on real hardware. X11 behavior varies significantly between window managers and desktop environments.

3. **Error Handling**: Current implementation uses `.unwrap()` and `.ok()` liberally. Should be replaced with proper error propagation.

4. **Memory Management**: X11 resources must be freed explicitly. Implement Drop traits carefully to prevent leaks.

5. **Thread Safety**: X11 is not thread-safe. All X11 calls must happen on the same thread. Document this clearly.

6. **DPI Handling**: Different DEs handle DPI differently (some via Xft.dpi, some via XRandR, some via GDK settings). Need to try multiple methods.
