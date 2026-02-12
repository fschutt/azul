# Monitor & Coordinate Space API Report

## Status Quo

### 1. Coordinate Types in the Codebase

| Type | File | Fields | Unit | Used For |
|------|------|--------|------|----------|
| `LogicalPosition` | `core/src/geom.rs` | `x: f32, y: f32` | Logical pixels (CSS pixels) | Cursor position, hit testing, layout |
| `PhysicalPositionI32` | `core/src/geom.rs` | `x: i32, y: i32` | Hardware pixels | Window position (`WindowPosition`) |
| `LayoutPoint` | `azul-css` | `x: isize, y: isize` | Rounded logical pixels | Internal layout |
| `LayoutSize` | `azul-css` | `width: isize, height: isize` | Rounded logical pixels | Internal layout |
| `LogicalSize` | `core/src/geom.rs` | `width: f32, height: f32` | Logical pixels | Window dimensions |

### 2. Window Position vs Cursor Position — Critical Asymmetry

```
WindowPosition::Initialized(PhysicalPositionI32)   ← physical i32 (hardware pixels)
CursorPosition::InWindow(LogicalPosition)           ← logical f32 (CSS pixels)
```

On macOS this "works" because macOS global coordinates are in points (= logical pixels),
so `PhysicalPositionI32` is really storing logical points cast to i32. But on Windows,
window position is in physical pixels while cursor is in logical pixels — adding them
produces **wrong screen coordinates** when DPI ≠ 1.0.

### 3. What Each Platform Reports

| | Cursor coords | Cursor unit | Window position | Win pos unit | Screen-absolute available? |
|---|---|---|---|---|---|
| **macOS** | `locationInWindow()` | Points (logical) | `frame().origin` | Points (logical) | ✅ `[NSEvent mouseLocation]` or `convertPoint:toScreen:` |
| **Win32** | LPARAM in WM_MOUSEMOVE | Physical pixels (client area) | `GetWindowRect` | Physical pixels | ✅ `GetCursorPos()` (physical) |
| **X11** | `event.x` / `event.y` | Pixels | `XTranslateCoordinates` | Pixels | ✅ `event.x_root` / `event.y_root` (already in event struct, currently **unused**) |
| **Wayland** | `wl_pointer.motion` | Surface-local logical | **Not available** | N/A | ❌ **Impossible** by design |

### 4. Current Monitor Infrastructure

#### Monitor Struct (`core/src/window.rs`)

```rust
pub struct Monitor {
    pub monitor_id: MonitorId,       // { index: usize, hash: u64 }
    pub monitor_name: OptionString,
    pub size: LayoutSize,            // logical pixels
    pub position: LayoutPoint,       // virtual screen position
    pub scale_factor: f64,           // 1.0 = 96 DPI, 2.0 = Retina
    pub work_area: LayoutRect,       // minus taskbar/dock
    pub video_modes: VideoModeVec,
    pub is_primary_monitor: bool,
}
```

#### Discovery

- `dll/src/desktop/display.rs` — `enumerate_monitors()` per platform
- macOS: `[NSScreen screens]` with `backingScaleFactor`
- Win32: `EnumDisplayMonitors` + `GetMonitorInfoW` + `GetDpiForMonitor`
- X11: XRandR via dlopen
- Wayland: Probes CLI tools (`swaymsg`, `hyprctl`, `kscreen-doctor`, `wlr-randr`), fallback to env vars

#### Storage — **NOT cached anywhere**

- `App::get_monitors()` enumerates fresh every call
- **Bug**: On macOS/Windows, `App::get_monitors()` returns empty Vec!
  - Linux calls `enumerate_monitors_from_display()` (line 246 of `dll/src/desktop/app.rs`)
  - Non-Linux returns `Vec::new()` (line 252)
- `FullWindowState` only stores `monitor_id: OptionU32` — just an index, no Monitor data
- **Windows**: monitor_id is never set (stays None)

#### HiDPI Change Detection

| Platform | HiDPI change detected? | Mechanism |
|---|---|---|
| macOS | ✅ | `windowDidChangeBackingProperties` → `handle_dpi_change()` |
| Win32 | ✅ | `WM_DPICHANGED` → updates `size.dpi`, resizes window |
| Wayland | ✅ | `wl_surface.enter/leave` → recalculates max scale of active outputs |
| X11 | ❌ | No XRandR notification listener |

#### Monitor Hot-Plug Detection

| Platform | Monitor add/remove detected? |
|---|---|
| macOS | ❌ No `NSApplicationDidChangeScreenParametersNotification` |
| Win32 | ❌ No `WM_DISPLAYCHANGE` handler |
| X11 | ❌ No `XRRSelectInput` / `RRScreenChangeNotify` |
| Wayland | ✅ `wl_registry` events for `wl_output` bind/destroy |

---

## Problems

### Problem 1: The "Jiggle" Bug (FIXED in this session)

Drag delta was computed in **window-local** coordinates. Moving the window shifts
the cursor's local position, creating a feedback loop. Fixed by adding
`screen_position` to `InputSample` (computed as `window_pos + cursor_local`).

`titlebar_drag()` now uses `get_drag_delta_screen()` which is stable.

### Problem 2: Win32 Screen Position Is Wrong at DPI ≠ 1.0

`record_input_sample` computes:
```rust
screen_position = window_pos (physical i32) + cursor_local (logical f32)
```
This adds physical + logical pixels — **wrong** when DPI ≠ 1.0.

**Fix needed**: Either:
- Use `GetCursorPos()` directly on Win32 (returns physical screen coords)
- Or convert cursor to physical first: `screen_pos = win_pos + cursor_local * hidpi_factor`

### Problem 3: Wayland Cannot Do CSD Titlebar Drag via Position

Wayland does not expose global window position. The current titlebar drag
callback computes `new_pos = initial_pos + delta` which:
- `initial_pos` = `Uninitialized` → `if let ... Initialized` fails → **no-op**

Wayland window moves must go through the compositor:
```c
xdg_toplevel_move(toplevel, seat, serial);
```
This binding doesn't exist in azul's Wayland backend yet.

### Problem 4: No Way to Query "What Monitor Is My Window On?"

Users cannot ask from a callback: "which monitor am I on?" or "what's the DPI of the monitor I'm currently on?" because:
- `FullWindowState.monitor_id` is just `OptionU32` — a bare index
- Windows backend never sets it
- No method to go from monitor_id → Monitor struct
- `App::get_monitors()` is broken on macOS/Win32 (returns empty Vec)

### Problem 5: No Monitor Change Callbacks

When a window is dragged to a different monitor:
- DPI change is detected (macOS/Win32/Wayland) ✅
- But there's no **user-facing callback** for "window moved to new monitor"
- No way to listen for monitor hot-plug (add/remove displays)

### Problem 6: No Coordinate Space Clarity in the API

Users get:
- `get_cursor_relative_to_node()` → LogicalPosition (relative to hit node)
- `get_cursor_relative_to_viewport()` → LogicalPosition (window-local)
- `get_current_mouse_state().cursor_position` → CursorPosition (window-local)
- `get_cursor_position_screen()` → LogicalPosition (screen, just added)

But there's no documentation or type-level safety about which space you're in.
All return `LogicalPosition` with no indication of the coordinate space.

---

## Proposed API Design

### Phase 1: Core Coordinate Spaces (minimum viable)

Add a `CursorCoordSpace` query to `CallbackInfo`:

```rust
/// Coordinate space for cursor position queries.
/// All values are in logical pixels (HiDPI-independent) unless noted.
pub enum CursorCoordSpace {
    /// Relative to the top-left of the hit-test node that triggered the callback.
    /// Origin: top-left of the node's border box.
    /// Useful for: drawing at cursor position within a widget.
    Node,

    /// Relative to the top-left of the window's content area (viewport).
    /// Origin: top-left corner of the window content (below titlebar).
    /// This is what `cursor_position` in `MouseState` already stores.
    /// Useful for: most UI interactions, hit testing.
    Window,

    /// Relative to the top-left of the virtual screen (all monitors combined).
    /// Origin: top-left of the primary monitor.
    /// Computed as: window_position + cursor_in_window.
    /// On Wayland: falls back to Window space (compositor hides global position).
    /// Useful for: window dragging, cross-window DnD, multi-monitor awareness.
    Screen,

    /// Relative to the top-left of the **current monitor's** work area.
    /// Computed as: screen_position - monitor.work_area.origin.
    /// Useful for: snapping windows to monitor edges.
    Monitor,

    /// Delta from the cursor position at the start of the current drag session.
    /// Coordinate space: window-local (for node DnD) or screen (for window drag).
    /// Returns (0,0) at DragStart, increases during Drag.
    /// Useful for: drag offset computation.
    DragDelta,

    /// Delta from drag start in **screen** coordinates (stable during window moves).
    /// Useful for: titlebar drag computation.
    DragDeltaScreen,
}
```

Then:

```c
// C API
AzLogicalPosition AzCallbackInfo_getCursorPosition(AzCallbackInfo* info, AzCursorCoordSpace space);
bool              AzCallbackInfo_getCursorPositionValid(AzCallbackInfo* info, AzCursorCoordSpace space);
```

### Phase 2: Monitor API

#### 2a. Cache Monitors in App State

```rust
pub struct AppState {
    // ... existing fields ...
    /// Cached monitor list, refreshed on monitor change events
    pub monitors: MonitorVec,
    /// Generation counter, incremented on monitor topology change
    pub monitor_generation: u64,
}
```

Refresh on:
- macOS: `NSApplicationDidChangeScreenParametersNotification`
- Win32: `WM_DISPLAYCHANGE`
- X11: `RRScreenChangeNotify` (needs `XRRSelectInput`)
- Wayland: `wl_output` events (already implemented)

#### 2b. Per-Window Monitor Tracking

```rust
pub struct FullWindowState {
    // ... existing fields ...
    /// The monitor this window is currently on (updated on move/DPI change)
    pub current_monitor: OptionMonitor,  // Full Monitor struct, not just index
}
```

Updated when:
- Window is created (initial placement)
- `windowDidMove` / `WM_MOVE` / `ConfigureNotify`
- DPI change (implies monitor change)

#### 2c. CallbackInfo Methods

```rust
impl CallbackInfo {
    /// Get the monitor the window is currently on.
    fn get_current_monitor(&self) -> Option<&Monitor>;

    /// Get all connected monitors.
    fn get_all_monitors(&self) -> &[Monitor];

    /// Get the current window's DPI scale factor.
    fn get_current_hidpi_factor(&self) -> f32;

    /// Get the window position in screen coordinates.
    fn get_window_position_screen(&self) -> Option<LogicalPosition>;

    /// Get the window bounds in screen coordinates.
    fn get_window_rect_screen(&self) -> Option<LogicalRect>;
}
```

#### 2d. Monitor Change Event

New event filter:

```rust
EventFilter::Window(WindowEventFilter::MonitorChanged)
EventFilter::Window(WindowEventFilter::DpiChanged)
EventFilter::Window(WindowEventFilter::MonitorsChanged)  // global: monitor added/removed
```

Callback gets access to old + new monitor info.

### Phase 3: Cross-Monitor Drag Handling

#### 3a. DPI-Aware Window Drag

When dragging a window across monitors with different DPI:

1. Track the drag delta in **screen coordinates** (already done with `get_drag_delta_screen()`)
2. When the window center crosses a monitor boundary:
   - Fire `MonitorChanged` event
   - Update `current_monitor`
   - Platform layer handles DPI change (already works on macOS/Win32)
3. Window resize (if needed for DPI change) happens automatically via
   `WM_DPICHANGED` (Win32) or `backingScaleFactorChanged` (macOS)

#### 3b. Wayland: Compositor-Managed Drag

On Wayland, CSD titlebar drag should **not** compute new window position.
Instead:

```rust
fn titlebar_drag_start(...) -> Update {
    #[cfg(target_os = "linux")]
    if is_wayland() {
        // Tell compositor to start interactive move
        info.begin_interactive_move();  // → xdg_toplevel_move(toplevel, seat, serial)
        return Update::DoNothing;
    }
    // ... existing logic for macOS/Win32/X11 ...
}
```

This requires:
1. Add `xdg_toplevel_move` binding to `dlopen.rs`
2. Store `wl_seat` + last serial in window state
3. Add `CallbackInfo::begin_interactive_move()` method
4. Platform trait method: `fn begin_interactive_move(&mut self)`

### Phase 4: Type-Safe Coordinate Spaces (optional, future)

Replace raw `LogicalPosition` with tagged newtypes:

```rust
/// Position relative to window content area (viewport)
pub struct WindowPosition(pub LogicalPosition);

/// Position relative to virtual screen (all monitors)
pub struct ScreenPosition(pub LogicalPosition);

/// Position relative to a specific node
pub struct NodePosition(pub LogicalPosition);

/// Position relative to current monitor's work area
pub struct MonitorPosition(pub LogicalPosition);
```

This makes coordinate space errors compile-time failures instead of runtime bugs.

---

## Platform-Specific Fix Plan

### macOS

| Issue | Fix | Priority |
|-------|-----|----------|
| `App::get_monitors()` returns empty | Call `enumerate_monitors()` | HIGH |
| No monitor hot-plug detection | Add `NSApplicationDidChangeScreenParametersNotification` observer | MEDIUM |
| Window position stored as PhysicalPositionI32 but is really logical points | Document or use LogicalPosition | LOW |

### Win32

| Issue | Fix | Priority |
|-------|-----|----------|
| Screen position wrong at DPI ≠ 1.0 | Use `GetCursorPos()` for screen coords | HIGH |
| `monitor_id` never set | Call `MonitorFromWindow()` in `WM_MOVE` | HIGH |
| `App::get_monitors()` returns empty | Call `enumerate_monitors()` | HIGH |
| No `WM_DISPLAYCHANGE` handler | Add handler to WndProc | MEDIUM |

### X11

| Issue | Fix | Priority |
|-------|-----|----------|
| `x_root`/`y_root` not used | Read them in event handlers, pass as screen_position | HIGH |
| No DPI change detection | Add `XRRSelectInput` + `RRScreenChangeNotify` | MEDIUM |
| No monitor hot-plug | Same as above (XRandR notifications) | MEDIUM |

### Wayland

| Issue | Fix | Priority |
|-------|-----|----------|
| CSD drag is no-op (no global position) | Add `xdg_toplevel_move` binding | HIGH |
| Screen position = window-local (no global coords) | Document this limitation, fallback is correct | LOW |
| Monitor hot-plug already works | — | DONE ✅ |
| DPI change already works | — | DONE ✅ |

---

## C API Exposure Plan

### Current C API Surface (from api.json)

#### Types Already Exposed

| Type | api.json | repr | Fields |
|------|----------|------|--------|
| `LogicalPosition` | ✅ | C | `x: f32, y: f32` |
| `PhysicalPositionI32` | ✅ | C | type alias for `PhysicalPosition<i32>` → `x: i32, y: i32` |
| `WindowPosition` | ✅ | `C, u8` | enum: `Uninitialized` / `Initialized(PhysicalPositionI32)` |
| `CursorPosition` | ✅ | `C, u8` | enum: `InWindow(LogicalPosition)` / `OutOfWindow(LogicalPosition)` / `Uninitialized` |
| `Monitor` | ✅ | C | `monitor_id`, `monitor_name`, `size`, `position`, `scale_factor`, `work_area`, `video_modes`, `is_primary_monitor` |
| `MonitorVec` | ✅ | C | vec wrapper |
| `DragState` | ✅ | C | `drag_type`, `source_node`, `current_drop_target`, `file_path` |

#### Functions Already Exposed on `CallbackInfo`

| Function | Returns | Coordinate Space |
|----------|---------|-----------------|
| `get_cursor_position()` | `OptionLogicalPosition` | Window-local |
| `get_cursor_relative_to_node()` | `OptionLogicalPosition` | Node-local |
| `get_cursor_relative_to_viewport()` | `OptionLogicalPosition` | Window-local (viewport) |
| `get_current_mouse_state()` | `MouseState` (contains `CursorPosition`) | Window-local |
| `get_hidpi_factor()` | `f32` | — |
| `is_dragging()` | `bool` | — |
| `get_drag_state()` | `OptionDragState` | — |
| `get_dragged_node()` | `OptionDomNodeId` | — |

#### Functions on `App`

| Function | Returns | Status |
|----------|---------|--------|
| `get_monitors()` | `MonitorVec` | **Broken**: returns empty on macOS/Win32 (only Linux works) |

### What's NOT Exposed

| Functionality | Rust Exists? | In api.json? | Notes |
|---|---|---|---|
| `get_cursor_position_screen()` | ✅ just added | ❌ | Returns `LogicalPosition` (screen-absolute) |
| `get_drag_delta()` | ✅ on `GestureAndDragManager` | ❌ | Window-local drag delta |
| `get_drag_delta_screen()` | ✅ just added | ❌ | Screen-absolute drag delta (stable during moves) |
| Monitor query from callback | ❌ | ❌ | Must go through `App::get_monitors()` (broken) |
| `get_current_monitor()` | ❌ | ❌ | No way to ask "which monitor is my window on?" |
| Coordinate space enum | ❌ | ❌ | All functions return raw `LogicalPosition` |
| `begin_interactive_move()` | ❌ | ❌ | Needed for Wayland CSD drag |
| Monitor change callback | ❌ | ❌ | No user-facing event when window moves to new monitor |

### Architecture: `Arc<Mutex<MonitorVec>>` at App Level

The key problem: `App::get_monitors()` re-enumerates every call and is broken on
macOS/Win32. Callbacks need fast, reliable monitor access. Solution:

```rust
pub struct App {
    // ... existing fields ...
    /// Cached monitor list, shared with all windows.
    /// Updated by platform event loop on monitor topology changes.
    monitors: Arc<Mutex<MonitorVec>>,
}
```

**Refresh triggers** (platform → updates `Arc<Mutex<MonitorVec>>`):
- macOS: `NSApplicationDidChangeScreenParametersNotification`
- Win32: `WM_DISPLAYCHANGE`
- X11: `RRScreenChangeNotify` (after adding `XRRSelectInput`)
- Wayland: `wl_output` bind/destroy (already detected)
- App startup: initial enumeration

**Access from callbacks** via `CallbackInfo`:
```rust
impl CallbackInfo {
    /// Get a snapshot of all connected monitors.
    /// Fast: reads cached Arc<Mutex<MonitorVec>>, no syscalls.
    pub fn get_monitors(&self) -> MonitorVec;

    /// Get the monitor this window is currently on.
    /// Uses the cached list + window's stored monitor_id.
    pub fn get_current_monitor(&self) -> Option<Monitor>;
}
```

**Thread safety**: The `Arc<Mutex<MonitorVec>>` is only written by the
event-loop thread when a monitor topology change is detected. Reads from
callbacks happen on the same thread (callbacks execute synchronously on
the event-loop thread), so contention is zero in practice. The Mutex is
just for correctness if the user stores a reference across threads.

### Proposed api.json Additions

#### New Types

```json
"ScreenPosition": {
    "external": "azul_core::geom::ScreenPosition",
    "derive": ["Debug", "Copy", "Clone", "PartialEq", "Default"],
    "struct_fields": [{ "x": { "type": "f32" }, "y": { "type": "f32" } }],
    "repr": "C",
    "doc": ["Position in screen coordinates (logical pixels, relative to primary monitor origin). On Wayland: falls back to window-local since global coords are unavailable."]
},
"NodePosition": {
    "external": "azul_core::geom::NodePosition",
    "derive": ["Debug", "Copy", "Clone", "PartialEq", "Default"],
    "struct_fields": [{ "x": { "type": "f32" }, "y": { "type": "f32" } }],
    "repr": "C",
    "doc": ["Position relative to a DOM node's border box origin (logical pixels)."]
},
"DragDelta": {
    "external": "azul_core::geom::DragDelta",
    "derive": ["Debug", "Copy", "Clone", "PartialEq", "Default"],
    "struct_fields": [{ "dx": { "type": "f32" }, "dy": { "type": "f32" } }],
    "repr": "C",
    "doc": ["Drag offset from the cursor position at drag start (logical pixels)."]
},
"OptionScreenPosition": {
    "external": "core::option::Option<azul_core::geom::ScreenPosition>",
    "derive": ["Debug", "Copy", "Clone", "PartialEq"],
    "enum_fields": [{ "None": {}, "Some": { "type": "ScreenPosition" } }],
    "repr": "C, u8"
},
"OptionNodePosition": {
    "external": "core::option::Option<azul_core::geom::NodePosition>",
    "derive": ["Debug", "Copy", "Clone", "PartialEq"],
    "enum_fields": [{ "None": {}, "Some": { "type": "NodePosition" } }],
    "repr": "C, u8"
},
"OptionDragDelta": {
    "external": "core::option::Option<azul_core::geom::DragDelta>",
    "derive": ["Debug", "Copy", "Clone", "PartialEq"],
    "enum_fields": [{ "None": {}, "Some": { "type": "DragDelta" } }],
    "repr": "C, u8"
}
```

#### New Functions on `CallbackInfo`

```json
"get_cursor_position_screen": {
    "doc": [
        "Get the cursor position in screen coordinates (logical pixels).",
        "",
        "Returns the cursor position relative to the primary monitor origin.",
        "Computed as window_position + cursor_in_window.",
        "On Wayland: returns None (no global coordinates available)."
    ],
    "fn_args": [{ "self": "ref" }],
    "returns": { "type": "OptionScreenPosition" },
    "fn_body": "object.get_cursor_position_screen().map(|p| ScreenPosition { x: p.x, y: p.y }).into()"
},
"get_drag_delta": {
    "doc": [
        "Get the drag delta in window-local coordinates.",
        "",
        "Returns the offset from drag start to current cursor position.",
        "Returns None if no drag is active."
    ],
    "fn_args": [{ "self": "ref" }],
    "returns": { "type": "OptionDragDelta" },
    "fn_body": "object.get_drag_delta().map(|d| DragDelta { dx: d.x, dy: d.y }).into()"
},
"get_drag_delta_screen": {
    "doc": [
        "Get the drag delta in screen coordinates.",
        "",
        "Unlike get_drag_delta(), this is stable even when the window moves",
        "(e.g., during titlebar drag). Returns None if no drag is active.",
        "On Wayland: falls back to window-local delta."
    ],
    "fn_args": [{ "self": "ref" }],
    "returns": { "type": "OptionDragDelta" },
    "fn_body": "object.get_drag_delta_screen().map(|d| DragDelta { dx: d.x, dy: d.y }).into()"
},
"get_current_monitor": {
    "doc": [
        "Get the monitor the window is currently displayed on.",
        "",
        "Returns the full Monitor struct with DPI, size, position, etc.",
        "Returns None if monitor information is unavailable."
    ],
    "fn_args": [{ "self": "ref" }],
    "returns": { "type": "OptionMonitor" },
    "fn_body": "object.get_current_monitor().into()"
},
"get_monitors": {
    "doc": [
        "Get all connected monitors (cached, no syscall).",
        "",
        "Returns the monitor list from the app-level cache.",
        "Updated automatically when monitors are added/removed."
    ],
    "fn_args": [{ "self": "ref" }],
    "returns": { "type": "MonitorVec" },
    "fn_body": "object.get_monitors()"
}
```

#### New Function on `App` (fix existing)

```json
"get_monitors": {
    "doc": [
        "Get all connected monitors.",
        "Returns the cached monitor list (refreshed on topology changes)."
    ],
    "fn_args": [{ "self": "ref" }],
    "returns": { "type": "MonitorVec" },
    "fn_body": "object.get_monitors()"
}
```

This is already defined at line 72182 of api.json — the implementation just
needs fixing (macOS/Win32 currently return empty).

### Newtype Migration Path

The newtypes (`ScreenPosition`, `NodePosition`, `DragDelta`) replace raw
`LogicalPosition` returns, making coordinate space errors impossible at
compile time in Rust and immediately visible in C:

```c
// Before: all LogicalPosition — which space? who knows
AzLogicalPosition pos1 = AzCallbackInfo_getCursorPosition(&info);
AzLogicalPosition pos2 = AzCallbackInfo_getCursorPositionScreen(&info);  // same type!

// After: distinct types — can't accidentally mix
AzOptionLogicalPosition pos1 = AzCallbackInfo_getCursorPosition(&info);  // window-local
AzOptionScreenPosition  pos2 = AzCallbackInfo_getCursorPositionScreen(&info);  // screen
AzOptionNodePosition    pos3 = AzCallbackInfo_getCursorRelativeToNode(&info);  // node
AzOptionDragDelta       delta = AzCallbackInfo_getDragDeltaScreen(&info);      // delta
```

**Backward compatibility**: The existing `get_cursor_position()` → `OptionLogicalPosition`
stays unchanged (it's window-local, and `LogicalPosition` is the right type for
window-local coordinates). Only **new** functions use the new types.

Existing `get_cursor_relative_to_node()` currently returns `OptionLogicalPosition`.
It should be migrated to return `OptionNodePosition` in a future breaking change,
but this can be deferred.

### InputSample: MonitorID Tracking

Each `InputSample` in the gesture manager should record which monitor the cursor
was on at that moment:

```rust
pub struct InputSample {
    pub timestamp: Instant,
    pub position: LogicalPosition,     // window-local
    pub screen_position: LogicalPosition, // screen-absolute (already added)
    pub monitor_id: OptionMonitorId,   // NEW: which monitor this sample was on
}
```

This enables:
- Detecting when a drag crosses a monitor boundary (monitor_id changes)
- Computing per-monitor DPI-adjusted deltas
- Firing `MonitorChanged` events precisely when the cursor (not window center) crosses

The `monitor_id` is determined by hit-testing `screen_position` against
the cached `MonitorVec`'s position + size rectangles.

### Implementation Priority for C API

| Step | What | Complexity | Depends On |
|------|------|-----------|------------|
| 1 | Add `ScreenPosition`, `NodePosition`, `DragDelta` newtypes to `core/src/geom.rs` | Low | — |
| 2 | Add `get_drag_delta()` and `get_drag_delta_screen()` wrappers on `CallbackInfo` | Low | Rust methods exist |
| 3 | Change `get_cursor_position_screen()` to return `ScreenPosition` instead of `LogicalPosition` | Low | Step 1 |
| 4 | Add all new types + functions to api.json | Medium | Steps 1–3 |
| 5 | Fix `App::get_monitors()` on macOS/Win32 | Medium | — |
| 6 | Add `Arc<Mutex<MonitorVec>>` to App, wire up refresh triggers | Medium | Step 5 |
| 7 | Add `get_monitors()` and `get_current_monitor()` to `CallbackInfo` | Low | Step 6 |
| 8 | Add `begin_interactive_move()` for Wayland | High | xdg_toplevel_move binding |
| 9 | Add `MonitorChanged` / `MonitorsChanged` events | Medium | Step 6 |
| 10 | Migrate `get_cursor_relative_to_node()` → `OptionNodePosition` | Low (breaking) | Step 1 |

### Generated C API Preview

After codegen from api.json, C users would get:

```c
// Newtypes (all repr(C), identical memory layout to LogicalPosition)
typedef struct { float x; float y; } AzScreenPosition;
typedef struct { float x; float y; } AzNodePosition;
typedef struct { float dx; float dy; } AzDragDelta;

// Option wrappers (repr(C, u8))
typedef struct { uint8_t tag; AzScreenPosition payload; } AzOptionScreenPosition;
typedef struct { uint8_t tag; AzNodePosition payload; } AzOptionNodePosition;
typedef struct { uint8_t tag; AzDragDelta payload; } AzOptionDragDelta;

// New functions on CallbackInfo
AzOptionScreenPosition AzCallbackInfo_getCursorPositionScreen(
    const AzCallbackInfoRef* info
);
AzOptionDragDelta AzCallbackInfo_getDragDelta(
    const AzCallbackInfoRef* info
);
AzOptionDragDelta AzCallbackInfo_getDragDeltaScreen(
    const AzCallbackInfoRef* info
);
AzOptionMonitor AzCallbackInfo_getCurrentMonitor(
    const AzCallbackInfoRef* info
);
AzMonitorVec AzCallbackInfo_getMonitors(
    const AzCallbackInfoRef* info
);
```

### Example: C User Implementing Custom Titlebar Drag

```c
AzUpdate titlebar_drag(AzCallbackInfo* info) {
    AzOptionDragDelta delta = AzCallbackInfo_getDragDeltaScreen(info);
    if (delta.tag != AzOptionDragDelta_Some) {
        return AzUpdate_DoNothing;
    }

    // Get initial window position (stored in drag-start data)
    AzOptionLogicalPosition initial = /* ... stored at DragStart ... */;

    // Compute new position
    AzWindowPosition new_pos;
    new_pos.tag = AzWindowPosition_Initialized;
    new_pos.payload.x = (int32_t)(initial.payload.x + delta.payload.dx);
    new_pos.payload.y = (int32_t)(initial.payload.y + delta.payload.dy);

    // Apply
    AzCallbackInfo_setWindowPosition(info, new_pos);
    return AzUpdate_RefreshDom;
}
```

---

## Summary of Immediate Actions (This Session)

Already done:
1. ✅ Added `screen_position` field to `InputSample`
2. ✅ Added `get_drag_delta_screen()` to `GestureAndDragManager`
3. ✅ Fixed `titlebar_drag()` to use screen delta (eliminates jiggling)
4. ✅ Added `get_cursor_position_screen()` to `CallbackInfo`
5. ✅ `record_input_sample` in `event_v2.rs` computes screen position from `window_pos + cursor_local`

Still to do (this session):
6. Remove debug logging
7. Build and test

Future work (next sessions, ordered by priority):
1. Fix `App::get_monitors()` on macOS/Win32
2. Fix Win32 screen position (physical/logical mismatch)
3. Read X11 `x_root`/`y_root` as native screen-absolute coords
4. Add `xdg_toplevel_move` for Wayland CSD drag
5. Add `MonitorChanged`/`DpiChanged` user events
6. Cache monitors in app state
7. Type-safe coordinate space newtypes
