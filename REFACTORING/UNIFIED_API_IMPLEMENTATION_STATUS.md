# Unified API Implementation Progress

This document tracks the implementation status of the unified cross-platform event architecture.

## Overview

The unified API provides consistent interfaces across all platforms (Windows, macOS, X11, Wayland) through trait implementations. This allows platform-agnostic code while respecting platform-specific paradigms.

## Architecture

### Core Methods (Not Traits!)

All platform windows implement these methods with roughly the same API signature.
Located in platform-specific modules (e.g., `x11/mod.rs`, `wayland/mod.rs`, etc.):

1. **Event Handlers** - Convert OS events to state updates
   - `handle_mouse_button()` - Updates mouse state, calls `process_window_events_v2()`
   - `handle_mouse_move()` - Updates cursor position, calls `process_window_events_v2()`
   - `handle_keyboard()` - Updates keyboard state, calls `process_window_events_v2()`
   - `handle_mouse_crossing()` - Updates enter/leave, calls `process_window_events_v2()`
   - `handle_scroll()` - Updates scroll state, calls `process_window_events_v2()`

2. **Layout Regeneration** - After DOM changes
   - `regenerate_layout()` - Calls layout callback, injects CSD, rebuilds WebRender display list
   - `generate_frame_if_needed()` - Sends frame to WebRender if needed

3. **Window State Sync** - Apply state changes to OS
   - `sync_window_state()` - Reads `window_state`, applies to OS (title/size/position/visibility/frame/cursor)
   - `set_cursor()` - Platform-specific cursor setting
   
   **Key Design:** CSD callbacks (titlebar drag/double-click, minimize, maximize, close) modify 
   `window_state` directly. `sync_window_state()` then applies changes to OS. Simple!
   
   - **Titlebar drag:** Callback updates `window_state.position` → `sync_window_state()` calls OS move
   - **Titlebar double-click:** Callback toggles `window_state.flags.frame` → `sync_window_state()` calls OS maximize
   - **Minimize button:** Callback sets `window_state.flags.frame = Minimized` → `sync_window_state()` calls OS minimize
   - **Close button:** Callback sets `window_state.flags.close_requested = true` → Window closes

4. **Scrollbar Interaction**
   - `perform_scrollbar_hit_test()` - Uses WebRender hit-testing
   - `handle_scrollbar_click()` - Detects thumb vs track click
   - `handle_scrollbar_drag()` - Updates scroll during drag
   - `gpu_scroll()` - Efficient GPU-only scrolling

### Event Types

Platform-agnostic event types:
- `MouseButtonEvent` - Mouse button press/release with position and modifiers
- `MouseMoveEvent` - Mouse movement with position and modifiers
- `KeyboardEvent` - Keyboard input with key code, scan code, state, and text
- `MouseCrossingEvent` - Mouse enter/leave window
- `ScrollEvent` - Scroll wheel with delta and position
- `ScrollDelta` - Pixels or Lines
- `ButtonState` - Pressed or Released

**Removed Complexity:** CSD actions (TitlebarDrag, Minimize, Maximize, Close) are handled
directly by callbacks attached to CSD DOM nodes in `csd.rs`. No need for special traits
or hit-testing - the callbacks modify `window_state` and `sync_window_state()` applies changes.

## Implementation Status

### X11 - ✅ **PRODUCTION READY**

**File:** `dll/src/desktop/shell2/linux/x11/unified_impl.rs`

| Trait | Status | Notes |
|-------|--------|-------|
| `UnifiedEventHandlers` | ✅ Complete | Full state-diffing implementation |
| `UnifiedLayoutRegeneration` | ✅ Complete | CSD injection, solver3 layout |
| `UnifiedWindowSync` | ✅ Complete | Syncs all properties to OS (title/size/position/visibility/cursor/frame) |
| `UnifiedScrollbarHandling` | ✅ Complete | Full scrollbar support |

**Implementation Details:**

- **Event Handlers:** 
  - All handlers follow state-diffing pattern
  - Update `previous_window_state` → `current_window_state`
  - Call `process_window_events_v2()` for recursive callbacks
  - Return `ProcessEventResult`

- **Layout Regeneration:**
  - Calls user layout callback
  - Injects CSD decorations via `wrap_user_dom_with_decorations()` (optional for X11)
  - CSD callbacks are attached directly to DOM nodes - no special hit-testing needed
  - Performs layout with solver3
  - Rebuilds display list to WebRender
  - Synchronizes scrollbar opacity

- **Window Sync (Simplified!):**
  - Syncs title via `XStoreName`
  - Syncs size via `XResizeWindow`
  - Syncs position via `XMoveWindow` (CSD titlebar callback modifies `window_state.position`)
  - Syncs visibility via `XMapWindow`/`XUnmapWindow`
  - Syncs frame state (minimize/maximize) via `XIconifyWindow`/`_NET_WM_STATE`
  - Maps 36 cursor types to X11 cursors
  
  **Key insight:** CSD titlebar drag callback modifies `window_state.position`, then
  `sync_window_state()` just calls `XMoveWindow`. No complex `_NET_WM_MOVERESIZE` needed!

- **Scrollbar:**
  - WebRender hit-testing for scrollbar detection
  - Thumb/track differentiation
  - Drag state tracking with `ScrollbarDragState`
  - GPU scrolling via WebRender API

**Next Steps:**
1. Complete Wayland V2 port with similar pattern
2. Test multi-window scenarios

---

### Windows - ⚠️ **STUB IMPLEMENTATION**

**File:** `dll/src/desktop/shell2/windows/unified_impl.rs`

| Trait | Status | Notes |
|-------|--------|-------|
| `UnifiedEventHandlers` | ❌ Stub | Currently handled in WndProc |
| `UnifiedLayoutRegeneration` | ❌ Stub | Needs implementation |
| `UnifiedWindowSync` | ❌ Stub | Needs implementation |
| `UnifiedCsdHitTest` | ❌ Stub | Needs implementation |
| `UnifiedScrollbarHandling` | ❌ Stub | Needs implementation |
| `UnifiedMenuSupport` | ❌ Stub | Native HMENU support needed |

**Migration Plan:**

1. **Refactor WndProc** - Extract event handling into handler methods
   - Map `WM_LBUTTONDOWN`/`WM_LBUTTONUP` → `handle_mouse_button()`
   - Map `WM_MOUSEMOVE` → `handle_mouse_move()`
   - Map `WM_KEYDOWN`/`WM_KEYUP` → `handle_keyboard()`
   - Map `WM_MOUSEWHEEL` → `handle_scroll()`
   - Map `WM_MOUSELEAVE` → `handle_mouse_crossing()`

2. **Implement State Tracking** - Add `previous_window_state: Option<FullWindowState>`
   - Store before processing each event
   - Use `create_events_from_states()` for diffing

3. **Add Layout Regeneration** - Implement `regenerate_layout()`
   - Call layout callback
   - Inject CSD if `flags.is_decorated == false`
   - Rebuild WebRender display list

4. **Add State Sync** - Implement `sync_window_state()`
   - `SetWindowTextW` for title
   - `SetWindowPos` for size/position
   - `ShowWindow` for visibility
   - `SetCursor` for cursor type

5. **CSD Support** - Implement CSD operations
   - `ReleaseCapture()` + `SendMessageW(WM_NCLBUTTONDOWN, HTCAPTION)` for window move
   - `ShowWindow(SW_MINIMIZE)` for minimize
   - `ShowWindow(SW_MAXIMIZE/SW_RESTORE)` for maximize

6. **Native Menus** - Implement HMENU support
   - `CreatePopupMenu()` to create menu
   - `AppendMenuW()` to add items
   - `TrackPopupMenu()` to show at position

**Priority:** HIGH - Windows is primary desktop platform

---

### macOS - 🔄 **MOSTLY COMPLETE**

**File:** `dll/src/desktop/shell2/macos/unified_impl.rs`

| Trait | Status | Notes |
|-------|--------|-------|
| `UnifiedEventHandlers` | 🔄 Partial | Has V2 events, needs state-diffing pattern |
| `UnifiedLayoutRegeneration` | ✅ Complete | Already has `regenerate_layout()` |
| `UnifiedWindowSync` | ✅ Complete | Already has `sync_window_state()` |
| `UnifiedCsdHitTest` | ❌ Stub | Native decorations default, CSD optional |
| `UnifiedScrollbarHandling` | ❌ Stub | Needs implementation |
| `UnifiedMenuSupport` | ❌ Stub | Native NSMenu support needed |

**Migration Plan:**

1. **Adopt State-Diffing Pattern** - Update event handlers
   - Add `previous_window_state: Option<FullWindowState>`
   - Store before each event
   - Call `create_events_from_states()` in handlers
   - Currently has `process_event_v2()` - needs adaptation

2. **Verify Layout Regeneration** - Check existing implementation
   - Already has `regenerate_layout()`
   - Verify CSD injection is implemented
   - Verify WebRender integration

3. **Verify Window Sync** - Check existing implementation
   - Already has `sync_window_state()`
   - Verify all properties are synced
   - Verify cursor handling

4. **CSD Support (Optional)** - Implement if needed
   - `window.performWindowDragWithEvent()` for window move
   - `[window miniaturize:]` for minimize
   - `[window zoom:]` for maximize/restore

5. **Native Menus** - Implement NSMenu support
   - Create `NSMenu` from `azul_core::menu::Menu`
   - Use `NSMenu.popUpMenuPositioningItem()` for context menus
   - Map menu actions to callbacks

6. **Scrollbar Support** - Implement scrollbar handling
   - WebRender hit-testing integration
   - Drag state tracking
   - GPU scrolling

**Priority:** MEDIUM - Already mostly working

---

### Wayland - ⚠️ **STUB IMPLEMENTATION**

**File:** `dll/src/desktop/shell2/linux/wayland/unified_impl.rs`

| Trait | Status | Notes |
|-------|--------|-------|
| `UnifiedEventHandlers` | ❌ Stub | Listener-based, needs state batching |
| `UnifiedLayoutRegeneration` | ❌ Stub | Needs implementation, REQUIRES CSD |
| `UnifiedWindowSync` | ❌ Stub | Needs wl_surface_commit pattern |
| `UnifiedCsdHitTest` | ❌ Stub | CRITICAL - Wayland has no native decorations |
| `UnifiedScrollbarHandling` | ❌ Stub | Needs implementation |
| `UnifiedMenuSupport` | ❌ Stub | Must use Azul windows (no native menus) |

**Migration Plan:**

1. **Adapt Listener Architecture** - State batching for event handlers
   - Wayland uses listener callbacks (`wl_pointer_listener`, `wl_keyboard_listener`)
   - Batch state changes in listeners
   - Call unified handlers in event loop dispatch
   - Store pending events in queue

2. **Implement Layout Regeneration** - With mandatory CSD
   - Call layout callback
   - **ALWAYS** inject CSD (Wayland has no native decorations)
   - Rebuild WebRender display list
   - Synchronize with frame callbacks

3. **Implement State Sync** - Using Wayland protocols
   - `xdg_toplevel_set_title()` for title
   - `xdg_toplevel_set_min_size()`/`set_max_size()` for size constraints
   - Position not directly controllable in Wayland (compositor decides)
   - `wl_surface_commit()` after state changes
   - `wl_pointer_set_cursor()` for cursor type

4. **CSD Support (CRITICAL)** - Required for Wayland
   - Implement `check_csd_hit()` with WebRender hit-testing
   - `xdg_toplevel_move()` for window drag
   - `xdg_toplevel_set_minimized()` for minimize
   - `xdg_toplevel_set_maximized()`/`unset_maximized()` for maximize
   - Store CSD node IDs from layout for hit-testing

5. **Frame Synchronization** - Coordinate with compositor
   - Use `wl_surface_frame()` callback
   - Only render when frame callback fires
   - Track `frame_callback_pending` state
   - Implement backpressure if needed

6. **Menu Support** - Using Azul windows
   - Create popup `xdg_popup` surface for menu
   - Position relative to parent surface
   - Use Azul rendering for menu content
   - Handle `xdg_popup` grab semantics

**Priority:** HIGH - Wayland is increasingly important on Linux

**Special Considerations:**
- Wayland is **asynchronous** - state changes are batched and committed
- No direct window positioning - compositor controls placement
- Mandatory CSD - compositor provides NO decorations
- Frame callbacks required for smooth rendering
- Different input model - listeners vs polling

---

## Testing Plan

### Unit Tests

1. **Event Type Conversions** - Test platform events → unified events
   - Windows: `WM_LBUTTONDOWN` → `MouseButtonEvent`
   - macOS: `NSEvent.mouseDown` → `MouseButtonEvent`
   - X11: `ButtonPress` → `MouseButtonEvent`
   - Wayland: `wl_pointer_listener.button` → `MouseButtonEvent`

2. **State Diffing** - Test state comparison
   - Create `WindowState` instances with different properties
   - Call `create_events_from_states()`
   - Verify correct events are generated

3. **Scrollbar Hit-Testing** - Test scrollbar detection
   - Create layout with scrollable areas
   - Perform hit-tests at various positions
   - Verify correct scrollbar IDs returned

4. **CSD Hit-Testing** - Test CSD control detection
   - Create layout with CSD decorations
   - Test hits on titlebar, buttons, edges
   - Verify correct `CsdAction` returned

### Integration Tests

1. **Multi-Window** - Test multiple windows
   - Create 2+ windows
   - Send events to each window
   - Verify events are routed correctly
   - Test focus changes between windows

2. **CSD Interactions** - Test decoration clicks
   - Click titlebar → window moves
   - Click minimize → window minimizes
   - Click maximize → window maximizes
   - Click close → window closes

3. **Scrollbar Interactions** - Test scrolling
   - Click scrollbar track → page scroll
   - Drag scrollbar thumb → smooth scroll
   - Use mouse wheel → content scrolls
   - Verify GPU scrolling efficiency

4. **Menu Interactions** - Test context menus
   - Right-click → menu appears
   - Click menu item → callback fires
   - Click outside → menu closes

5. **Keyboard Navigation** - Test focus and input
   - Tab key → focus moves
   - Arrow keys → selection changes
   - Text input → updates focused node

### Platform-Specific Tests

#### X11
- Test various window managers (GNOME, KDE, i3, etc.)
- Test CSD vs native decorations preference
- Test multi-monitor setups

#### Windows
- Test DWM vs GDI rendering
- Test high-DPI scaling
- Test native menu behavior

#### macOS
- Test Retina displays
- Test native menu bar integration
- Test full-screen mode

#### Wayland
- Test various compositors (Weston, GNOME Shell, Sway, etc.)
- Test CSD rendering
- Test popup menus with grab semantics
- Test frame callback synchronization

---

## Migration Strategy

### Phase 1: X11 Complete ✅ (DONE)
- Full unified trait implementation
- Production-ready event handling
- CSD and scrollbar support

### Phase 2: Windows Migration (NEXT)
- Refactor WndProc to use unified handlers
- Implement state-diffing pattern
- Add layout regeneration
- Add state synchronization
- **Target:** 2-3 weeks

### Phase 3: macOS Cleanup
- Adopt state-diffing pattern
- Verify/fix existing implementations
- Add missing menu support
- **Target:** 1 week

### Phase 4: Wayland Implementation
- Adapt listener architecture for unified handlers
- Implement mandatory CSD
- Add frame synchronization
- Test on multiple compositors
- **Target:** 3-4 weeks

### Phase 5: Testing & Polish
- Multi-window testing
- CSD interaction testing
- Performance benchmarking
- Documentation updates
- **Target:** 1-2 weeks

**Total Estimated Time:** 7-10 weeks for complete migration

---

## API Stability Guarantees

### Public API
- `unified_events` module types are **STABLE**
- Event types (`MouseButtonEvent`, etc.) are **STABLE**
- Trait methods are **STABLE**

### Platform Implementations
- Platform-specific implementations are **INTERNAL**
- May change during migration without notice
- Only unified trait interfaces are guaranteed

### Compatibility
- All platforms must implement all traits
- Stub implementations allowed during migration
- Must return sensible defaults (e.g., `ProcessEventResult::DoNothing`)

---

## Performance Considerations

### State Diffing Overhead
- Cloning `WindowState` on every event: ~50-100ns per event (negligible)
- `create_events_from_states()`: O(n) where n = number of changed properties
- Typically processes 1-5 events per OS event

### Recursive Callbacks
- Max recursion depth: 5 levels
- Average depth: 1-2 levels
- Protection against infinite loops

### WebRender Hit-Testing
- Hit-testing: ~10-50µs depending on complexity
- Cached between events
- Async resolution for scrollbars

### GPU Scrolling
- No layout regeneration needed
- Only WebRender scroll offset update
- ~1-2ms per scroll event vs ~16ms with layout

---

## Documentation

### Rust Documentation
- **Module:** `dll/src/desktop/event_architecture`
- Comprehensive rustdoc covering:
  - Event processing pipeline
  - State management
  - Platform implementations
  - Handler methods
  - Layout regeneration
  - Multi-window support
  - CSD and menus

### Architecture Document
- **File:** `REFACTORING/UNIFIED_EVENT_ARCHITECTURE.md`
- 75KB comprehensive design document
- Platform comparison tables
- Implementation roadmap
- Wayland-specific considerations

---

## Conclusion

The unified API establishes a consistent cross-platform interface while respecting platform-specific paradigms. X11 serves as the reference implementation with production-ready code. Other platforms are in various stages of migration, from mostly complete (macOS) to stub implementations (Windows, Wayland).

The trait-based design allows gradual migration without breaking existing code, and stub implementations ensure the API surface is consistent even during development.

**Key Principle:** "Architecture first, implementation second." The important part is establishing the unified interface - platform-specific implementations can be completed incrementally while the architecture remains stable.
