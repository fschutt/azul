# Unified Cross-Platform Event Architecture

## Executive Summary

This document defines a unified event processing architecture that works across Windows, macOS, Linux (X11), and Wayland while respecting each platform's native paradigms. The goal is to achieve:

1. **Consistent behavior** across all platforms
2. **Platform-native feel** (respecting OS conventions)
3. **Efficient multi-window support**
4. **Clean separation** between platform code and core logic

---

## 1. Current State Analysis

### 1.1 Platform Event Models

| Platform | Event Model | Multi-Window | CSD Support | Menu System |
|----------|-------------|--------------|-------------|-------------|
| **Windows** | Message pump (GetMessage/DispatchMessage) | Native (HWND-based) | Optional (DWM) | Native (HMENU) |
| **macOS** | NSApplication event loop | Native (NSWindow-based) | Optional (NSWindowStyleMask) | Native (NSMenu) |
| **X11** | XNextEvent polling | Manual tracking | Optional (CSD via StyledDom) | CSD-based (StyledDom) |
| **Wayland** | wl_display_dispatch protocol | Manual tracking | Mandatory (protocol) | CSD-based (StyledDom) |

### 1.2 Key Architectural Differences

**Windows/macOS:**
- OS provides window management infrastructure
- Events dispatched per-window automatically
- Native decorations and menus
- Single global window registry managed by OS

**X11:**
- Direct X server connection
- Synchronous event polling (XNextEvent blocks)
- Window IDs (xulong) identify windows
- Manual decoration rendering if CSD enabled
- Freedom to implement any window management pattern

**Wayland:**
- Protocol-based, asynchronous
- No direct window IDs - uses wl_surface handles
- All decorations must be client-side (CSD mandatory)
- Compositor handles presentation
- Event dispatching via listener callbacks

---

## 2. Unified Architecture Design

### 2.1 Core Abstraction: The Event Processing Pipeline

All platforms follow this conceptual pipeline (no traits - just methods on platform Window structs):

```
┌──────────────────┐
│  OS Event Queue  │ ← Platform-specific event source
└────────┬─────────┘
         │
         ▼
┌──────────────────┐
│  poll_event()    │ ← Method on platform Window struct
└────────┬─────────┘   Returns Option<()> (event processed inline)
         │
         ▼
┌──────────────────┐
│ Event Handlers   │ ← handle_mouse_button(), handle_keyboard(), etc.
└────────┬─────────┘   Update current_window_state
         │
         ▼
┌──────────────────┐
│  State Diffing   │ ← create_events_from_states(previous, current)
└────────┬─────────┘   Detects what changed
         │
         ▼
┌──────────────────┐
│ dispatch_events()│ ← Routes to appropriate callbacks based on hit-test
└────────┬─────────┘   Returns list of matching callbacks
         │
         ▼
┌──────────────────┐
│ Callback Loop    │ ← invoke_callbacks_v2() - executes each callback
└────────┬─────────┘   Depth-limited recursion (max 5 levels)
         │
         ▼
┌──────────────────┐
│ Result Handler   │ ← process_callback_result_v2()
└────────┬─────────┘   DOM updates, window changes, timers, threads
         │
         ▼
┌──────────────────┐
│regenerate_layout│ ← If DOM changed: relayout + CSD injection
└────────┬─────────┘   + scrollbar calculation + display list rebuild
         │
         ▼
┌──────────────────┐
│sync_window_state│ ← Sync window properties back to OS
└────────┬─────────┘   (title, size, position, visibility, cursor, frame)
         │
         ▼
┌──────────────────┐
│  request_redraw  │ ← Trigger platform-specific redraw mechanism
└──────────────────┘
```

**Key Principle: No Traits, Just Methods**

Each platform implements these methods directly on their Window struct (Win32Window, MacOSWindow, X11Window, WaylandWindow). No trait abstraction needed - the methods are platform-specific and called directly by the event loop.

### 2.2 State Management

**Two-State System:**
```rust
struct PlatformWindow {
    previous_window_state: Option<FullWindowState>,
    current_window_state: FullWindowState,
    // ...
}
```

**FullWindowState vs WindowState:**
- `WindowState`: Public API type (what users create/modify)
- `FullWindowState`: Internal runtime type with additional fields:
  - `last_hit_test: FullHitTest` - Latest hit-testing results
  - `focused_node: Option<DomId, NodeId>` - Currently focused node
  - `selections: BTreeMap<DomId, TextSelection>` - Text selections per DOM
  - `hovered_file: Option<PathBuf>` - Drag-and-drop state
  - `dropped_file: Option<PathBuf>` - Drop event state
  - `window_focused: bool` - Window focus state

**State Diffing:**
```rust
// Save previous state before modifications
self.previous_window_state = Some(self.current_window_state.clone());

// Modify current state based on OS event
self.current_window_state.mouse.cursor_position = new_position;

// Detect changes and generate events
let events = create_events_from_states(
    &self.previous_window_state.unwrap(),
    &self.current_window_state
);

// Dispatch to callbacks
let callbacks = dispatch_events(events, &hit_test, &layout_results);
```

### 2.3 Event Handler Methods (No Traits)

All platforms implement these methods directly on their Window struct (Win32Window, MacOSWindow, X11Window, WaylandWindow):

```rust
// Example: X11Window
impl X11Window {
    pub fn handle_mouse_button(&mut self, event: &XButtonEvent) 
        -> ProcessEventResult { /* ... */ }
    
    pub fn handle_mouse_move(&mut self, event: &XMotionEvent) 
        -> ProcessEventResult { /* ... */ }
    
    pub fn handle_keyboard(&mut self, event: &mut XKeyEvent) 
        -> ProcessEventResult { /* ... */ }
    
    pub fn handle_mouse_crossing(&mut self, event: &XCrossingEvent) 
        -> ProcessEventResult { /* ... */ }
    
    pub fn handle_scroll(&mut self, event: &XButtonEvent) 
        -> ProcessEventResult { /* ... */ }
    
    pub fn sync_window_state(&mut self) { /* ... */ }
    
    pub fn regenerate_layout(&mut self) -> Result<(), String> { /* ... */ }
    
    pub fn generate_frame_if_needed(&mut self) { /* ... */ }
}

enum ProcessEventResult {
    DoNothing,           // No redraw needed
    RequestRedraw,       // Request redraw but don't regenerate layout
    RegenerateAndRedraw, // Full relayout + redraw
}
```

**Handler Responsibilities:**
1. Save previous state
2. Update current state from OS event
3. Update hit-test at event position
4. Call `process_window_events_v2()` for recursive callback processing
5. Return appropriate result flag

**Platform-Specific Parameters:**
- Each platform uses its native event types (XButtonEvent, MSG, NSEvent, etc.)
- No generic PlatformEvent abstraction needed
- Direct, efficient handling without conversion overhead

---

## 3. Multi-Window Architecture

### 3.1 Window Registry Pattern

All platforms use a global registry to track windows:

```rust
// Windows: HWND -> *mut Win32Window
static WINDOW_REGISTRY: Mutex<HashMap<HWND, *mut Win32Window>> = ...;

// macOS: NSWindow -> *mut MacOSWindow (via associated objects)
objc_setAssociatedObject(nswindow, KEY, window_ptr, ...);

// X11: Window (xulong) -> *mut X11Window
static X11_WINDOW_REGISTRY: Mutex<HashMap<Window, *mut X11Window>> = ...;

// Wayland: wl_surface -> *mut WaylandWindow
static WAYLAND_SURFACE_REGISTRY: Mutex<HashMap<*mut wl_surface, *mut WaylandWindow>> = ...;
```

**Registry API:**
```rust
pub fn register_window<H: WindowHandle>(handle: H, window_ptr: *mut PlatformWindow);
pub fn unregister_window<H: WindowHandle>(handle: H) -> Option<*mut PlatformWindow>;
pub fn get_window<H: WindowHandle>(handle: H) -> Option<*mut PlatformWindow>;
pub fn get_all_window_handles() -> Vec<H>;
```

### 3.2 Event Loop Patterns

**Single-Window Mode (Blocking):**
```rust
loop {
    // Blocks until event arrives
    let event = window.poll_event(); // GetMessage/NSApplication.nextEvent/XNextEvent
    
    if event.is_some() {
        // Event was processed, handle frame regeneration if needed
        window.generate_frame_if_needed();
    } else {
        // No more windows, exit
        break;
    }
}
```

**Multi-Window Mode (Non-Blocking):**
```rust
loop {
    let window_handles = get_all_window_handles();
    
    if window_handles.is_empty() {
        break; // All windows closed
    }
    
    let mut had_events = false;
    
    for handle in &window_handles {
        let window = get_window(handle).unwrap();
        
        // Non-blocking poll
        while let Some(event) = window.poll_event_nonblocking() {
            had_events = true;
            // Event processed inside poll_event
        }
        
        // Generate frame if layout changed
        window.generate_frame_if_needed();
    }
    
    if !had_events {
        // Sleep briefly to avoid busy-loop
        std::thread::sleep(Duration::from_millis(1));
    }
}
```

### 3.3 Platform-Specific Multi-Window Handling

**Windows:**
```rust
// PeekMessage for non-blocking, GetMessage for blocking
if is_multi_window {
    for hwnd in window_handles {
        while PeekMessageW(&mut msg, hwnd, 0, 0, PM_REMOVE) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg); // Calls WndProc -> registered window
        }
    }
} else {
    GetMessageW(&mut msg, hwnd, 0, 0); // Blocks
    // ...
}
```

**macOS:**
```rust
// NSApplication event loop handles multi-window automatically
// Events are dispatched to NSWindow -> WindowDelegate -> registered window

loop {
    let event = NSApplication.nextEventMatchingMask(
        Any, 
        None, // Non-blocking
        NSDefaultRunLoopMode, 
        true
    );
    
    if let Some(event) = event {
        // Get window from event.window
        let nswindow = event.window();
        let window_ptr = objc_getAssociatedObject(nswindow, KEY);
        
        // Process event for that window
        window_ptr.process_event(event);
        
        NSApplication.sendEvent(event); // Forward to system
    }
}
```

**X11:**
```rust
// Manual polling per display
for (window_id, display) in &x11_windows {
    while XPending(display) > 0 {
        let mut event: XEvent = zeroed();
        XNextEvent(display, &mut event);
        
        // Get target window from event
        let target_window_id = event.any.window;
        
        if let Some(window) = get_x11_window(target_window_id) {
            window.handle_event(&mut event);
        }
    }
}
```

**Wayland:**
```rust
// Protocol-based dispatching via event queue
for (surface, window) in &wayland_windows {
    // Non-blocking dispatch of pending events
    while wl_display_dispatch_queue_pending(display, event_queue) > 0 {
        // Events dispatched to registered listeners automatically
        // Listeners have access to window pointer via callback data
    }
    
    // Check if compositor synchronized frame
    if window.frame_callback_done {
        window.generate_frame_if_needed();
        window.frame_callback_done = false;
    }
}
```

---

## 4. Client-Side Decorations (CSD)

### 4.1 CSD Decision Matrix

| Platform | Native Decorations | CSD Support | When to Use CSD |
|----------|-------------------|-------------|-----------------|
| Windows | Yes (DWM) | Optional (custom drawing) | `WindowDecorations::None` |
| macOS | Yes (NSWindowStyleMask) | Optional (custom views) | `WindowDecorations::None` |
| X11 | Depends on WM | Full support | `WindowDecorations::None` |
| Wayland | No | Mandatory | Always |

### 4.2 CSD Injection Pattern

**Unified CSD Check:**
```rust
fn should_inject_csd(
    has_decorations: bool,     // Does window have any decorations?
    decorations: WindowDecorations, // What kind?
) -> bool {
    match decorations {
        WindowDecorations::None => has_decorations, // User wants decorations but said None
        WindowDecorations::Native => false,         // Use platform native
        WindowDecorations::Custom => true,          // Always inject custom
    }
}
```

**CSD Injection in regenerate_layout():**
```rust
pub fn regenerate_layout(&mut self) -> Result<(), String> {
    // 1. Call user's layout callback
    let user_styled_dom = match &self.current_window_state.layout_callback {
        LayoutCallback::Raw(inner) => (inner.cb)(&mut app_data, &mut callback_info),
        LayoutCallback::Marshaled(m) => (m.cb.cb)(&mut m.marshal_data, &mut app_data, &mut callback_info),
    };
    
    // 2. Wrap with CSD if needed
    let styled_dom = if should_inject_csd(
        self.current_window_state.flags.has_decorations,
        self.current_window_state.flags.decorations,
    ) {
        crate::desktop::csd::wrap_user_dom_with_decorations(
            user_styled_dom,
            &self.current_window_state.title,
            true,  // has_titlebar
            true,  // has_minimize
            true,  // has_maximize
            &self.system_style, // For native look
        )
    } else {
        user_styled_dom
    };
    
    // 3. Layout with solver3
    layout_window.layout_and_generate_display_list(
        styled_dom,
        &self.current_window_state,
        &self.renderer_resources,
        &ExternalSystemCallbacks::rust_internal(),
        &mut None,
    )?;
    
    // 4. Calculate scrollbar states
    layout_window.scroll_states.calculate_scrollbar_states();
    
    // 5. Rebuild display list to WebRender
    let mut txn = Transaction::new();
    rebuild_display_list(&mut txn, layout_window, &mut self.render_api, ...);
    
    // 6. Synchronize scrollbar opacity for fade effects
    LayoutWindow::synchronize_scrollbar_opacity(...);
    
    // 7. Mark frame needs regeneration
    self.frame_needs_regeneration = true;
    
    Ok(())
}
```

### 4.3 CSD Hit-Testing

**Titlebar Drag:**
```rust
fn handle_mouse_button(&mut self, event: &MouseButtonEvent) -> ProcessEventResult {
    // Check if click is on CSD titlebar
    if let Some(csd_action) = self.check_csd_hit(event.position) {
        match csd_action {
            CsdAction::TitlebarDrag => {
                #[cfg(windows)]
                {
                    // Windows: Use DragMove API
                    ReleaseCapture();
                    SendMessageW(self.hwnd, WM_NCLBUTTONDOWN, HTCAPTION, 0);
                }
                
                #[cfg(target_os = "macos")]
                {
                    // macOS: performWindowDragWithEvent
                    unsafe {
                        self.window.performWindowDragWithEvent(&event.ns_event);
                    }
                }
                
                #[cfg(target_os = "linux")]
                {
                    match self {
                        LinuxWindow::X11(x11) => {
                            // X11: _NET_WM_MOVERESIZE message
                            x11.start_window_move(event.position);
                        }
                        LinuxWindow::Wayland(wl) => {
                            // Wayland: xdg_toplevel.move()
                            (wl.wayland.xdg_toplevel_move)(
                                wl.xdg_toplevel,
                                wl.seat,
                                wl.pointer_state.serial,
                            );
                        }
                    }
                }
                
                return ProcessEventResult::DoNothing;
            }
            
            CsdAction::Minimize => {
                self.minimize();
                return ProcessEventResult::DoNothing;
            }
            
            CsdAction::Maximize => {
                self.toggle_maximize();
                return ProcessEventResult::RequestRedraw;
            }
            
            CsdAction::Close => {
                self.close();
                return ProcessEventResult::DoNothing;
            }
        }
    }
    
    // Normal event processing
    // ...
}

fn check_csd_hit(&self, position: LogicalPosition) -> Option<CsdAction> {
    if !should_inject_csd(...) {
        return None; // Not using CSD
    }
    
    // Query layout tree for CSD hit-test nodes
    if let Some(layout_window) = &self.layout_window {
        for (dom_id, layout_result) in &layout_window.layout_results {
            // Check if position hits any CSD control nodes
            // These have special node IDs set during CSD injection
            if let Some(action) = csd::hit_test_csd_controls(
                position,
                &layout_result.layout_tree,
            ) {
                return Some(action);
            }
        }
    }
    
    None
}
```

---

## 5. Menu System Architecture

### 5.1 Menu Abstraction Layers

```
User API (azul_core::menu::Menu)
         ↓
Platform Adapter
         ↓
Platform Implementation
```

**Three Strategies:**

1. **Native Menus** (Windows HMENU, macOS NSMenu)
   - Best performance
   - Native look and feel
   - Platform-specific code

2. **CSD Menus** (X11, Wayland, optional on other platforms)
   - Rendered as Azul windows
   - Full styling control
   - Consistent across platforms

3. **Hybrid** (fallback pattern)
   - Try native first
   - Fall back to CSD if native unavailable

### 5.2 Menu Window Pattern (CSD Menus)

**Menu as Azul Window:**
```rust
pub fn create_menu_window_options(
    menu: &Menu,
    parent_window_handle: RawWindowHandle,
    position: LogicalPosition,
    system_style: &SystemStyle,
) -> WindowCreateOptions {
    let mut window_state = WindowState::default();
    
    // Menu-specific window configuration
    window_state.flags.is_always_on_top = true;
    window_state.flags.is_visible = true;
    window_state.flags.decorations = WindowDecorations::None;
    window_state.flags.is_resizable = false;
    window_state.position = WindowPosition::Initialized(position);
    
    // Layout callback to render menu
    let menu_data = MenuLayoutData {
        menu: menu.clone(),
        system_style: system_style.clone(),
    };
    
    window_state.layout_callback = LayoutCallback::Marshaled(MarshaledLayoutCallback {
        marshal_data: RefAny::new(menu_data),
        cb: MarshaledLayoutCallbackInner {
            cb: menu_layout_callback, // Generates StyledDom for menu
        },
    });
    
    WindowCreateOptions {
        state: window_state,
        size_to_content: true,  // Auto-size to menu content
        renderer: None.into(),
        theme: None.into(),
        create_callback: None.into(),
        hot_reload: false.into(),
    }
}

extern "C" fn menu_layout_callback(
    data: &mut RefAny,
    _system_style: &mut RefAny,
    _info: &mut LayoutCallbackInfo,
) -> StyledDom {
    let menu_data = data.downcast_ref::<MenuLayoutData>().unwrap();
    
    // Use menu_renderer to create styled DOM
    crate::desktop::menu_renderer::create_menu_styled_dom(
        &menu_data.menu,
        &menu_data.system_style,
    )
}
```

**Menu Event Handling:**
```rust
// In menu window's event handlers
fn handle_mouse_button(&mut self, event: &MouseButtonEvent) -> ProcessEventResult {
    if event.button == MouseButton::Left && event.state == ButtonState::Released {
        // Check which menu item was clicked via hit-testing
        if let Some(menu_item_id) = self.get_clicked_menu_item(event.position) {
            // Invoke menu callback
            if let Some(callback) = self.menu.get_callback(menu_item_id) {
                // Execute callback
                (callback)(&mut app_data, &mut callback_info);
                
                // Close menu after selection
                self.close();
                
                // Notify parent window to update
                self.notify_parent_window();
            }
        }
    }
    
    ProcessEventResult::DoNothing
}
```

### 5.3 Platform-Specific Menu Integration

**Windows:**
```rust
pub fn inject_native_menu(&mut self, menu: &Menu) -> Result<(), String> {
    let hmenu = CreateMenu();
    
    for item in &menu.items {
        match item {
            MenuItem::Action { id, label, callback, .. } => {
                AppendMenuW(hmenu, MF_STRING, *id as usize, to_wstring(label).as_ptr());
            }
            MenuItem::Separator => {
                AppendMenuW(hmenu, MF_SEPARATOR, 0, ptr::null());
            }
            MenuItem::Submenu { label, submenu, .. } => {
                let hsubmenu = self.create_submenu(submenu)?;
                AppendMenuW(hmenu, MF_POPUP, hsubmenu as usize, to_wstring(label).as_ptr());
            }
        }
    }
    
    SetMenu(self.hwnd, hmenu);
    DrawMenuBar(self.hwnd);
    
    Ok(())
}

// In WndProc
WM_COMMAND => {
    let command_id = (wparam & 0xFFFF) as u16;
    
    if let Some(callback) = window.menu_callbacks.get(&command_id) {
        // Invoke callback via layout_window
        window.invoke_menu_callback(callback);
    }
    
    0
}
```

**macOS:**
```rust
pub fn inject_native_menu(&mut self, menu: &Menu) -> Result<(), String> {
    let nsmenu = NSMenu::new(self.mtm);
    
    for item in &menu.items {
        match item {
            MenuItem::Action { label, callback, .. } => {
                let nsitem = NSMenuItem::alloc(self.mtm);
                nsitem.initWithTitle_action_keyEquivalent(
                    &NSString::from_str(label),
                    Some(sel!(menuAction:)),
                    &NSString::new(),
                );
                
                // Store callback reference
                let tag = register_menu_callback(callback);
                nsitem.setTag(tag as isize);
                
                nsmenu.addItem(&nsitem);
            }
            // ... handle other item types
        }
    }
    
    NSApplication::sharedApplication(self.mtm).setMainMenu(&nsmenu);
    
    Ok(())
}

// Menu action handler (registered selector)
#[sel(menuAction:)]
fn menu_action(&self, sender: &NSMenuItem) {
    let tag = sender.tag();
    if let Some(callback) = get_menu_callback(tag as u64) {
        // Queue menu action to be processed in main loop
        queue_menu_action(tag as u64);
    }
}

// In poll_event()
let pending_actions = take_pending_menu_actions();
for tag in pending_actions {
    self.handle_menu_action(tag);
}
```

**X11 / Wayland (CSD Menus):**
```rust
pub fn show_context_menu(&mut self, menu: &Menu, position: LogicalPosition) -> Result<(), String> {
    // Create menu window
    let menu_options = create_menu_window_options(
        menu,
        self.get_raw_window_handle(),
        position,
        &self.resources.system_style,
    );
    
    // Create and show menu window
    let menu_window = PlatformWindow::new(menu_options)?;
    
    // Register as child window
    registry::register_menu_window(menu_window);
    
    // Menu window handles its own events in the main loop
    // Closes itself when item selected or focus lost
    
    Ok(())
}
```

---

## 6. Wayland-Specific Considerations

### 6.1 Protocol-Based Architecture

Wayland is fundamentally different - it's protocol-oriented, not event-oriented:

```rust
pub struct WaylandWindow {
    // Protocol objects
    display: *mut wl_display,
    compositor: *mut wl_compositor,
    surface: *mut wl_surface,
    xdg_wm_base: *mut xdg_wm_base,
    xdg_surface: *mut xdg_surface,
    xdg_toplevel: *mut xdg_toplevel,
    seat: *mut wl_seat,
    
    // Event queue for this window
    event_queue: *mut wl_event_queue,
    
    // Frame synchronization
    frame_callback: *mut wl_callback,
    frame_callback_done: bool,
    
    // State (same as other platforms)
    current_window_state: FullWindowState,
    previous_window_state: Option<FullWindowState>,
    // ...
}
```

### 6.2 Wayland Event Flow

**Different from X11:**

```rust
// X11: Synchronous event polling
while XPending(display) > 0 {
    XNextEvent(display, &mut event);
    handle_event(&mut event);
}

// Wayland: Asynchronous listener callbacks
impl WaylandWindow {
    pub fn setup_listeners(&mut self) {
        // Pointer events
        let pointer_listener = wl_pointer_listener {
            enter: pointer_enter_handler,
            leave: pointer_leave_handler,
            motion: pointer_motion_handler,
            button: pointer_button_handler,
            axis: pointer_axis_handler,
            // ...
        };
        
        wl_pointer_add_listener(
            self.pointer,
            &pointer_listener,
            self as *mut _ as *mut c_void, // Window pointer as user data
        );
        
        // Keyboard events
        let keyboard_listener = wl_keyboard_listener {
            keymap: keyboard_keymap_handler,
            enter: keyboard_enter_handler,
            leave: keyboard_leave_handler,
            key: keyboard_key_handler,
            modifiers: keyboard_modifiers_handler,
            // ...
        };
        
        wl_keyboard_add_listener(
            self.keyboard,
            &keyboard_listener,
            self as *mut _ as *mut c_void,
        );
        
        // Surface events
        let surface_listener = wl_surface_listener {
            enter: surface_enter_handler,
            leave: surface_leave_handler,
        };
        
        wl_surface_add_listener(
            self.surface,
            &surface_listener,
            self as *mut _ as *mut c_void,
        );
        
        // XDG surface events
        let xdg_surface_listener = xdg_surface_listener {
            configure: xdg_surface_configure_handler,
        };
        
        xdg_surface_add_listener(
            self.xdg_surface,
            &xdg_surface_listener,
            self as *mut _ as *mut c_void,
        );
        
        // XDG toplevel events
        let xdg_toplevel_listener = xdg_toplevel_listener {
            configure: xdg_toplevel_configure_handler,
            close: xdg_toplevel_close_handler,
        };
        
        xdg_toplevel_add_listener(
            self.xdg_toplevel,
            &xdg_toplevel_listener,
            self as *mut _ as *mut c_void,
        );
    }
}

// Listener handlers extract window pointer and call methods
extern "C" fn pointer_button_handler(
    data: *mut c_void,
    pointer: *mut wl_pointer,
    serial: u32,
    time: u32,
    button: u32,
    state: u32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    
    // Convert Wayland button event to platform-agnostic format
    let mouse_button = match button {
        0x110 => MouseButton::Left,
        0x111 => MouseButton::Right,
        0x112 => MouseButton::Middle,
        _ => return,
    };
    
    let button_state = match state {
        0 => ButtonState::Released,
        1 => ButtonState::Pressed,
        _ => return,
    };
    
    // Save serial for move/resize operations
    window.pointer_state.serial = serial;
    
    // Call unified event handler
    window.handle_mouse_button_internal(mouse_button, button_state);
}
```

### 6.3 Wayland State Diffing Adaptation

**Challenge:** Wayland listeners fire independently, not in a single event loop iteration.

**Solution:** Defer state diffing until explicit sync point.

```rust
impl WaylandWindow {
    fn handle_mouse_button_internal(&mut self, button: MouseButton, state: ButtonState) {
        // Mark that state changed
        self.state_dirty = true;
        
        // Update state immediately
        self.current_window_state.mouse.button_states.set(button, state);
        
        // DON'T call process_window_events_v2() here - wait for sync
    }
    
    fn handle_mouse_motion_internal(&mut self, x: f64, y: f64) {
        self.state_dirty = true;
        self.current_window_state.mouse.cursor_position = 
            CursorPosition::InWindow(LogicalPosition::new(x as f32, y as f32));
    }
    
    // Called from poll_event() after all pending events dispatched
    pub fn sync_and_process_events(&mut self) -> ProcessEventResult {
        if !self.state_dirty {
            return ProcessEventResult::DoNothing;
        }
        
        // Save previous state
        self.previous_window_state = Some(self.current_window_state.clone());
        
        // Update hit-test
        self.update_hit_test(self.current_window_state.mouse.cursor_position.get_position());
        
        // Process with state diffing
        let result = self.process_window_events_v2();
        
        // Clear dirty flag
        self.state_dirty = false;
        
        result
    }
}

// In poll_event()
fn poll_event(&mut self) -> Option<WaylandEvent> {
    // Dispatch all pending events (non-blocking)
    // This fires listeners which update state
    let dispatched = unsafe {
        wl_display_dispatch_queue_pending(self.display, self.event_queue)
    };
    
    if dispatched > 0 {
        // Events were processed, now sync state and run callbacks
        let result = self.sync_and_process_events();
        
        if result != ProcessEventResult::DoNothing {
            self.frame_needs_regeneration = true;
        }
        
        Some(WaylandEvent::Other)
    } else {
        None
    }
}
```

### 6.4 Wayland Frame Synchronization

**Compositor-driven rendering:**

```rust
impl WaylandWindow {
    pub fn request_redraw(&mut self) {
        if self.frame_callback.is_null() {
            // Request frame callback from compositor
            self.frame_callback = unsafe {
                (self.wayland.wl_surface_frame)(self.surface)
            };
            
            let frame_listener = wl_callback_listener {
                done: frame_done_handler,
            };
            
            unsafe {
                (self.wayland.wl_callback_add_listener)(
                    self.frame_callback,
                    &frame_listener,
                    self as *mut _ as *mut c_void,
                );
            }
            
            // Commit surface to request frame
            unsafe {
                (self.wayland.wl_surface_commit)(self.surface);
            }
        }
    }
}

extern "C" fn frame_done_handler(
    data: *mut c_void,
    callback: *mut wl_callback,
    callback_data: u32,
) {
    let window = unsafe { &mut *(data as *mut WaylandWindow) };
    
    // Destroy callback
    unsafe {
        (window.wayland.wl_callback_destroy)(callback);
    }
    window.frame_callback = ptr::null_mut();
    
    // Mark frame callback done
    window.frame_callback_done = true;
    
    // Generate frame if needed
    if window.frame_needs_regeneration {
        window.generate_frame_if_needed();
    }
}
```

### 6.5 Wayland CSD (Mandatory)

Unlike X11 where CSD is optional, Wayland requires client-side decorations:

```rust
impl WaylandWindow {
    pub fn new(options: WindowCreateOptions) -> Result<Self, WindowError> {
        // ... setup Wayland objects ...
        
        // Force CSD on Wayland
        let mut state = options.state;
        if state.flags.decorations != WindowDecorations::None {
            // Wayland doesn't support native decorations
            state.flags.decorations = WindowDecorations::None;
            state.flags.has_decorations = true; // But we want them (CSD)
        }
        
        // ... continue initialization ...
    }
    
    pub fn regenerate_layout(&mut self) -> Result<(), String> {
        // ... same as X11 ...
        
        // CSD injection always happens on Wayland
        let styled_dom = crate::desktop::csd::wrap_user_dom_with_decorations(
            user_styled_dom,
            &self.current_window_state.title,
            true, true, true,
            &self.system_style,
        );
        
        // ... rest of layout ...
    }
}
```

---

## 7. Implementation Roadmap

### Phase 1: Core V2 Architecture (✅ COMPLETE for X11)
- [x] State-diffing event system
- [x] Recursive callback processing
- [x] `regenerate_layout()` with CSD injection
- [x] `sync_window_state()`
- [x] Scrollbar integration
- [x] Hit-testing integration

### Phase 2: Menu System
- [ ] Windows native menu (HMENU)
- [ ] macOS native menu (NSMenu)
- [x] X11 CSD menu (Azul window)
- [ ] Wayland CSD menu (Azul window)
- [ ] Menu callback routing
- [ ] Context menu support

### Phase 3: Wayland V2 Port
- [ ] Listener-based event handling
- [ ] State batching and sync points
- [ ] Frame callback integration
- [ ] Mandatory CSD
- [ ] Multi-window registry

### Phase 4: Multi-Window Polish
- [ ] Window focus management
- [ ] Modal window support
- [ ] Window Z-order management
- [ ] Parent-child relationships
- [ ] Window groups

### Phase 5: Testing & Optimization
- [ ] Unit tests for state diffing
- [ ] Integration tests per platform
- [ ] Performance benchmarks
- [ ] Memory leak checks
- [ ] Multi-window stress tests

---

## 8. Key Principles

1. **Platform Abstraction Without Compromise**
   - Respect native paradigms
   - Don't force all platforms into the same mold
   - Provide unified behavior, not identical implementation

2. **State as Source of Truth**
   - All platforms maintain `FullWindowState`
   - State diffing drives event generation
   - Callbacks modify state, regenerate_layout() responds

3. **Explicit Sync Points**
   - Clear separation: event → state update → callback → layout → render
   - Platform-specific timing (blocking vs non-blocking)
   - Frame synchronization via platform mechanisms

4. **CSD as First-Class**
   - X11: Optional but well-supported
   - Wayland: Mandatory and primary path
   - Windows/macOS: Opt-in alternative to native

5. **Menu Flexibility**
   - Native when available and appropriate
   - CSD fallback always available
   - Consistent callback interface regardless of implementation

---

## 9. Migration Path

### For Existing Code

**X11:** ✅ Already migrated to V2 architecture

**Wayland:** Needs adaptation
1. Add listener-based event handlers (keep existing structure)
2. Add `state_dirty` flag and `sync_and_process_events()`
3. Implement `regenerate_layout()` (copy from X11)
4. Implement `sync_window_state()` (Wayland-specific protocol calls)
5. Test frame synchronization with compositor

**Windows:** Mostly compatible
1. Move WndProc logic to handler methods
2. Add `frame_needs_regeneration` tracking
3. Implement `regenerate_layout()` (copy from X11/macOS)
4. Implement `sync_window_state()` (Win32 API calls)

**macOS:** ✅ Already has V2 patterns, may need minor cleanup

---

## 10. Conclusion

This unified architecture provides:

- **Consistency:** Same event processing flow across all platforms
- **Flexibility:** Platform-specific optimizations where needed
- **Maintainability:** Shared code for state diffing, callbacks, layout
- **Extensibility:** Easy to add new window types (tooltips, popups, etc.)

The key insight is that **all platforms can use state diffing**, even though the event sources differ. By separating "event collection" from "event processing," we achieve both platform-native behavior and cross-platform code reuse.
