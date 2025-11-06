//! Desktop implementation of the Azul GUI toolkit
//!
//! # Unified Cross-Platform Event Architecture
//!
//! This module documents the unified event processing architecture used across all platforms
//! (Windows, macOS, X11, Wayland). The architecture provides consistent behavior while respecting
//! platform-specific paradigms.
//!
//! ## Core Concepts
//!
//! ### 1. Event Processing Pipeline
//!
//! All platforms follow this conceptual flow:
//!
//! ```text
//! OS Event → poll_event() → Event Handlers → State Diffing →
//! dispatch_events() → Callbacks → Result Processing →
//! regenerate_layout() → sync_window_state() → request_redraw()
//! ```
//!
//! ### 2. State Management
//!
//! Each window maintains two states:
//!
//! ```rust,ignore
//! pub struct PlatformWindow {
//!     previous_window_state: Option<FullWindowState>,
//!     current_window_state: FullWindowState,
//!     // ...
//! }
//! ```
//!
//! **State Types:**
//!
//! - [`WindowState`](azul_layout::window_state::WindowState): Public API type
//!   - User-facing window properties (title, size, position, flags)
//!   - Created by users, passed to `WindowCreateOptions`
//!
//! - [`FullWindowState`](azul_layout::window_state::FullWindowState): Internal runtime type
//!   - Extends `WindowState` with runtime tracking:
//!     - `last_hit_test: FullHitTest` - Latest hit-testing results from WebRender
//!     - `focused_node: Option<(DomId, NodeId)>` - Currently focused DOM node
//!     - `selections: BTreeMap<DomId, TextSelection>` - Text selections per DOM
//!     - `hovered_file: Option<PathBuf>` - Drag-and-drop hover state
//!     - `dropped_file: Option<PathBuf>` - Completed drop event
//!     - `window_focused: bool` - Window has OS focus
//!
//! ### 3. State Diffing Pattern
//!
//! The core insight: **detect changes by comparing states, not by tracking events**.
//!
//! ```rust,ignore
//! // 1. Save previous state
//! self.previous_window_state = Some(self.current_window_state.clone());
//!
//! // 2. Update current state from OS event
//! self.current_window_state.mouse.cursor_position = new_position;
//!
//! // 3. Detect changes automatically
//! let events = create_events_from_states(
//!     &self.previous_window_state.unwrap(),
//!     &self.current_window_state
//! );
//!
//! // 4. Route to callbacks based on hit-test
//! let callbacks = dispatch_events(events, &hit_test, &layout_results);
//!
//! // 5. Execute callbacks recursively (max depth: 5)
//! self.invoke_callbacks_v2(callbacks);
//! ```
//!
//! ## Platform-Specific Implementations
//!
//! ### Windows (Win32)
//!
//! **Event Model:** Message pump with `GetMessage`/`DispatchMessage`
//!
//! **Structures:**
//! - `Win32Window` - Main window structure
//! - Window procedure (`WndProc`) converts `MSG` to handler calls
//! - Window registry: `HashMap<HWND, *mut Win32Window>`
//!
//! **Event Flow:**
//! ```rust,ignore
//! // In WndProc callback
//! let window = get_window(hwnd);
//!
//! match msg {
//!     WM_LBUTTONDOWN => {
//!         window.handle_mouse_button(&event);
//!     }
//!     WM_MOUSEMOVE => {
//!         window.handle_mouse_move(&event);
//!     }
//!     // ...
//! }
//! ```
//!
//! **CSD Support:** Optional via DWM (Desktop Window Manager)
//! - Native decorations by default
//! - Can inject custom decorations via `wrap_user_dom_with_decorations()`
//!
//! **Menu Support:** Native HMENU
//! - Create via `CreateMenu()`/`AppendMenuW()`
//! - Attach via `SetMenu(hwnd, hmenu)`
//! - Callbacks invoked via `WM_COMMAND` message
//!
//! ### macOS (Cocoa)
//!
//! **Event Model:** NSApplication event loop
//!
//! **Structures:**
//! - `MacOSWindow` - Main window structure
//! - `NSWindow` with custom delegate
//! - Window stored via `objc_setAssociatedObject`
//!
//! **Event Flow:**
//! ```rust,ignore
//! // In poll_event()
//! let event = NSApplication.nextEventMatchingMask(...);
//!
//! if let Some(event) = event {
//!     let macos_event = MacOSEvent::from_nsevent(&event);
//!     self.process_event(&event, &macos_event);
//!     NSApplication.sendEvent(&event); // Forward to system
//! }
//! ```
//!
//! **CSD Support:** Optional via NSWindowStyleMask
//! - Native titlebar by default
//! - Can hide and inject custom via StyledDom
//!
//! **Menu Support:** Native NSMenu
//! - Create via `NSMenu::new()`
//! - Items via `NSMenuItem`
//! - Set via `NSApplication.setMainMenu()`
//! - Callbacks via selector routing (`menuAction:`)
//!
//! ### X11 (Linux)
//!
//! **Event Model:** Direct XNextEvent polling
//!
//! **Structures:**
//! - `X11Window` - Main window structure
//! - Direct connection to X11 display
//! - Window registry: `HashMap<Window, *mut X11Window>`
//!
//! **Event Flow:**
//! ```rust,ignore
//! // In poll_event()
//! while XPending(display) > 0 {
//!     XNextEvent(display, &mut event);
//!
//!     match event.type_ {
//!         ButtonPress => self.handle_mouse_button(&event.button),
//!         MotionNotify => self.handle_mouse_move(&event.motion),
//!         KeyPress => self.handle_keyboard(&mut event.key),
//!         // ...
//!     }
//! }
//! ```
//!
//! **CSD Support:** Full support via StyledDom
//! - Window manager provides native decorations by default
//! - Can request no decorations and inject custom
//! - Uses `_NET_WM_MOVERESIZE` for titlebar drag
//!
//! **Menu Support:** CSD menus (Azul windows)
//! - No native X11 menu API
//! - Menus rendered as separate Azul windows
//! - Full styling control
//!
//! ### Wayland (Linux)
//!
//! **Event Model:** Protocol-based with listener callbacks
//!
//! **Structures:**
//! - `WaylandWindow` - Main window structure
//! - Protocol objects: `wl_surface`, `xdg_toplevel`, `wl_seat`
//! - Surface registry: `HashMap<*mut wl_surface, *mut WaylandWindow>`
//!
//! **Event Flow (Different!):**
//! ```rust,ignore
//! // Setup listeners once
//! wl_pointer_add_listener(pointer, &listener, window_ptr);
//!
//! // Listener callbacks fired asynchronously
//! extern "C" fn pointer_button_handler(data: *mut c_void, ...) {
//!     let window = unsafe { &mut *(data as *mut WaylandWindow) };
//!     window.handle_mouse_button_internal(button, state);
//!     window.state_dirty = true; // Mark for sync
//! }
//!
//! // In poll_event() - sync point
//! wl_display_dispatch_queue_pending(display, queue);
//! if self.state_dirty {
//!     self.sync_and_process_events(); // Process all accumulated changes
//! }
//! ```
//!
//! **CSD Support:** MANDATORY
//! - Wayland has no native decoration protocol
//! - All decorations must be client-side
//! - Always inject via `wrap_user_dom_with_decorations()`
//!
//! **Menu Support:** CSD menus (Azul windows)
//! - Same as X11 - no native API
//! - Rendered as Azul windows
//!
//! ## Key Data Structures
//!
//! ### Event Handler Results
//!
//! ```rust,ignore
//! pub enum ProcessEventResult {
//!     DoNothing,           // No redraw needed
//!     RequestRedraw,       // Redraw but don't regenerate layout
//!     RegenerateAndRedraw, // Full relayout + redraw
//! }
//! ```
//!
//! ### Scrollbar State
//!
//! ```rust,ignore
//! pub struct ScrollbarDragState {
//!     pub scrollbar_id: ScrollbarHitId,
//!     pub drag_start_position: LogicalPosition,
//!     pub scroll_position_at_drag_start: f32,
//! }
//! ```
//!
//! Tracks active scrollbar dragging across multiple mouse move events.
//!
//! ### CSD Actions
//!
//! ```rust,ignore
//! pub enum CsdAction {
//!     TitlebarDrag,  // Drag window by titlebar
//!     Minimize,      // Minimize button clicked
//!     Maximize,      // Maximize/restore button clicked
//!     Close,         // Close button clicked
//! }
//! ```
//!
//! Detected via hit-testing against CSD control nodes in layout tree.
//!
//! ## Handler Method Signatures
//!
//! All platform windows implement these unified handler methods:
//!
//! ```rust,ignore
//! impl PlatformWindow {
//!     /// Handle mouse button press/release
//!     fn handle_mouse_button(&mut self, event: &PlatformMouseButtonEvent)
//!         -> ProcessEventResult;
//!
//!     /// Handle mouse movement
//!     fn handle_mouse_move(&mut self, event: &PlatformMouseMoveEvent)
//!         -> ProcessEventResult;
//!
//!     /// Handle keyboard input
//!     fn handle_keyboard(&mut self, event: &mut PlatformKeyEvent)
//!         -> ProcessEventResult;
//!
//!     /// Handle mouse enter/leave window
//!     fn handle_mouse_crossing(&mut self, event: &PlatformCrossingEvent)
//!         -> ProcessEventResult;
//!
//!     /// Handle scroll wheel
//!     fn handle_scroll(&mut self, event: &PlatformScrollEvent)
//!         -> ProcessEventResult;
//! }
//! ```
//!
//! **Handler Responsibilities:**
//! 1. Save `previous_window_state`
//! 2. Update `current_window_state` from OS event
//! 3. Update hit-test via WebRender
//! 4. Call `process_window_events_v2()` for recursive callbacks
//! 5. Return result indicating redraw necessity
//!
//! ## Layout Regeneration
//!
//! When DOM changes (via callbacks), layout must be regenerated:
//!
//! ```rust,ignore
//! pub fn regenerate_layout(&mut self) -> Result<(), String> {
//!     // 1. Call user's layout callback
//!     let user_styled_dom = invoke_layout_callback(&self.app_data, &callback_info);
//!
//!     // 2. Inject CSD if needed
//!     let styled_dom = if should_inject_csd(...) {
//!         crate::desktop::csd::wrap_user_dom_with_decorations(
//!             user_styled_dom,
//!             &self.current_window_state.title,
//!             true, true, true,  // titlebar, minimize, maximize
//!             &self.system_style,
//!         )
//!     } else {
//!         user_styled_dom
//!     };
//!
//!     // 3. Perform layout with solver3
//!     layout_window.layout_and_generate_display_list(...)?;
//!
//!     // 4. Calculate scrollbar states
//!     layout_window.scroll_manager.calculate_scrollbar_states();
//!
//!     // 5. Rebuild display list to WebRender
//!     rebuild_display_list(&mut txn, layout_window, &mut render_api, ...);
//!
//!     // 6. Synchronize scrollbar opacity for fade effects
//!     LayoutWindow::synchronize_scrollbar_opacity(...);
//!
//!     // 7. Mark frame needs regeneration
//!     self.frame_needs_regeneration = true;
//!
//!     Ok(())
//! }
//! ```
//!
//! ## State Synchronization
//!
//! After state changes, sync back to OS:
//!
//! ```rust,ignore
//! fn sync_window_state(&mut self) {
//!     let (previous, current) = match &self.previous_window_state {
//!         Some(prev) => (prev.clone(), self.current_window_state.clone()),
//!         None => return, // First frame
//!     };
//!
//!     // Title changed?
//!     if previous.title != current.title {
//!         #[cfg(windows)]
//!         SetWindowTextW(self.hwnd, ...);
//!
//!         #[cfg(target_os = "macos")]
//!         self.window.setTitle(...);
//!
//!         #[cfg(target_os = "linux")]
//!         XStoreName(self.display, self.window, ...);
//!     }
//!
//!     // Size changed?
//!     if previous.size != current.size {
//!         // Platform-specific resize calls
//!     }
//!
//!     // Cursor changed?
//!     if let Some(layout_window) = &self.layout_window {
//!         let cursor_type = layout_window.compute_cursor_type_hit_test(&current.last_hit_test);
//!         self.set_cursor(cursor_type.cursor_icon);
//!     }
//!
//!     // ... other properties
//! }
//! ```
//!
//! ## Multi-Window Support
//!
//! ### Window Registry
//!
//! Each platform maintains a global registry:
//!
//! **Windows:**
//! ```rust,ignore
//! static WINDOW_REGISTRY: Mutex<HashMap<HWND, *mut Win32Window>>;
//! ```
//!
//! **macOS:**
//! ```rust,ignore
//! // Via Objective-C associated objects
//! objc_setAssociatedObject(nswindow, KEY, window_ptr, ...);
//! ```
//!
//! **X11:**
//! ```rust,ignore
//! static X11_WINDOW_REGISTRY: Mutex<HashMap<Window, *mut X11Window>>;
//! ```
//!
//! **Wayland:**
//! ```rust,ignore
//! static WAYLAND_SURFACE_REGISTRY: Mutex<HashMap<*mut wl_surface, *mut WaylandWindow>>;
//! ```
//!
//! ### Event Loop Patterns
//!
//! **Single-Window (Blocking):**
//! ```rust,ignore
//! loop {
//!     if let Some(event) = window.poll_event() {
//!         // Process event
//!         window.generate_frame_if_needed();
//!     } else {
//!         break; // Window closed
//!     }
//! }
//! ```
//!
//! **Multi-Window (Non-Blocking):**
//! ```rust,ignore
//! loop {
//!     let handles = get_all_window_handles();
//!     if handles.is_empty() { break; }
//!
//!     let mut had_events = false;
//!     for handle in handles {
//!         let window = get_window(handle).unwrap();
//!         while let Some(_) = window.poll_event_nonblocking() {
//!             had_events = true;
//!         }
//!         window.generate_frame_if_needed();
//!     }
//!
//!     if !had_events {
//!         std::thread::sleep(Duration::from_millis(1));
//!     }
//! }
//! ```
//!
//! ## Client-Side Decorations (CSD)
//!
//! ### Decision Logic
//!
//! ```rust,ignore
//! fn should_inject_csd(
//!     has_decorations: bool,
//!     decorations: WindowDecorations,
//! ) -> bool {
//!     match decorations {
//!         WindowDecorations::None => has_decorations,  // Want decorations but said None
//!         WindowDecorations::Native => false,          // Use platform native
//!         WindowDecorations::Custom => true,           // Always inject
//!     }
//! }
//! ```
//!
//! ### Platform Matrix
//!
//! | Platform | Native Support | CSD Support | Default Behavior |
//! |----------|---------------|-------------|------------------|
//! | Windows  | Yes (DWM)     | Optional    | Native           |
//! | macOS    | Yes (NSWindowStyleMask) | Optional | Native  |
//! | X11      | Via WM        | Full        | Native (WM)      |
//! | Wayland  | No            | Mandatory   | Always CSD       |
//!
//! ### CSD Hit-Testing
//!
//! CSD controls are detected via special node IDs in the layout tree:
//!
//! ```rust,ignore
//! fn check_csd_hit(&self, position: LogicalPosition) -> Option<CsdAction> {
//!     if !should_inject_csd(...) {
//!         return None;
//!     }
//!
//!     if let Some(layout_window) = &self.layout_window {
//!         for (dom_id, layout_result) in &layout_window.layout_results {
//!             if let Some(action) = csd::hit_test_csd_controls(
//!                 position,
//!                 &layout_result.layout_tree,
//!             ) {
//!                 return Some(action);
//!             }
//!         }
//!     }
//!
//!     None
//! }
//! ```
//!
//! ## Menu System
//!
//! ### Native Menus (Windows/macOS)
//!
//! **Windows:**
//! - Create via `CreateMenu()`
//! - Attach via `SetMenu(hwnd, hmenu)`
//! - Events via `WM_COMMAND` message
//!
//! **macOS:**
//! - Create via `NSMenu::new()`
//! - Set via `NSApplication.setMainMenu()`
//! - Callbacks via selector routing
//!
//! ### CSD Menus (X11/Wayland)
//!
//! Menus rendered as separate Azul windows:
//!
//! ```rust,ignore
//! pub fn create_menu_window_options(
//!     menu: &Menu,
//!     parent_handle: RawWindowHandle,
//!     position: LogicalPosition,
//!     system_style: &SystemStyle,
//! ) -> WindowCreateOptions {
//!     let mut state = WindowState::default();
//!
//!     state.flags.is_always_on_top = true;
//!     state.flags.decorations = WindowDecorations::None;
//!     state.flags.is_resizable = false;
//!     state.position = WindowPosition::Initialized(position);
//!
//!     state.layout_callback = LayoutCallback::Marshaled(MarshaledLayoutCallback {
//!         marshal_data: RefAny::new(MenuLayoutData { menu, system_style }),
//!         cb: menu_layout_callback, // Generates StyledDom
//!     });
//!
//!     WindowCreateOptions {
//!         state,
//!         size_to_content: true,  // Auto-size to menu
//!         // ...
//!     }
//! }
//! ```
//!
//! ## Completed Features (Phase 1)
//!
//! ✅ **All Platform V2 Ports Complete:**
//!    - macOS: Full event_v2 integration with NSApplication event loop
//!    - Windows: Complete V2 implementation with frame regeneration tracking
//!    - X11: Full V2 architecture with state diffing
//!    - Wayland: Complete V2 port with wl_output protocol for monitor enumeration
//!
//! ✅ **Native Menu Systems:**
//!    - macOS: Full NSMenu integration with AzulMenuTarget callback bridge
//!    - Windows: HMENU integration planned for Phase 2
//!    - Linux: Native menu support planned for Phase 2
//!
//! ## Next Steps (Phase 2)
//!
//! 1. **Enhanced Menu Systems:**
//!    - Windows HMENU integration
//!    - Linux native menu support
//!
//! 2. **Testing & Validation:**
//!    - Multi-window stress tests
//!    - CSD interaction tests
//!    - Menu callback routing tests
//!    - Cross-platform behavior validation
//!
//! ## References
//!
//! - See `REFACTORING/UNIFIED_EVENT_ARCHITECTURE.md` for detailed design document
//! - See `shell2::common::PlatformWindow` trait for interface definition
//! - See `azul_core::events::dispatch_events()` for event routing logic
//! - See `azul_layout::window_state::create_events_from_states()` for state diffing
//! - See `crate::desktop::csd::wrap_user_dom_with_decorations()` for CSD injection

#![allow(dead_code)]
#![deny(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(missing_copy_implementations)]
#![deny(clippy::all)]
#![allow(warnings)]

/// Manages application state (`App` / `AppState` / `AppResources`), wrapping resources and app
/// state
pub mod app;
/// New compositor integration for shell2 - WebRender bridge
pub mod compositor2;
/// Extensions for LayoutCallbackInfo to support SystemStyle
/// Client-Side Decorations (CSD) - Custom window titlebar
pub mod csd;
/// CSS type definitions / CSS parsing functions
#[cfg(any(feature = "css_parser", feature = "native_style"))]
pub mod css;
/// Bindings to the native file-chooser, color picker, etc. dialogs
pub mod dialogs;
/// Display/Monitor management for menu positioning
pub mod display;
/// Extra functions for file IO (for C / C++ developers)
pub mod file;
#[cfg(feature = "logging")]
mod logging;
/// Unified menu system using window-based approach
pub mod menu;
/// Menu rendering - Converts Menu structures to StyledDom
pub mod menu_renderer;
/// New windowing backend (shell2) - modern, clean architecture
pub mod shell2;
/// WebRender type translations and hit-testing for shell2
pub mod wr_translate2;
/// OpenGL texture cache for external image support
pub mod gl_texture_cache;
/// Font & image resource handling, lookup and caching
pub mod resources {
    pub use azul_core::resources::*;
    pub use azul_layout::{font::*, image::*};
}
/// Handles text layout (modularized, can be used as a standalone module)
pub mod text_layout {
    pub use azul_layout::text3::*;
}
/// SVG parsing + rendering
pub mod svg {
    pub use azul_layout::xml::svg::*;
}
/// XML parsing
pub mod xml {
    pub use azul_layout::xml::*;
}
/// Re-exports of errors
pub mod errors {
    // TODO: re-export the sub-types of ClipboardError!
    #[cfg(all(feature = "font_loading", feature = "std"))]
    pub use azul_layout::font::loading::FontReloadError;
    pub use clipboard2::ClipboardError;
}

pub use azul_core::{callbacks, dom, gl, style, styled_dom, task};

#[cfg(target_os = "macos")]
#[link(name = "CoreText", kind = "framework")]
fn __macos() {}
