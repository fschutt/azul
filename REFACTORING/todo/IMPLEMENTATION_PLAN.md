# Implementation Plan: Critical Platform Features

**Date:** October 30, 2025  
**Status:** Planning Phase  
**Priority:** HIGH

## Overview

Three critical platform features need implementation/improvement:

1. **GNOME Native Menus** (Linux DBus integration)
2. **Wayland V2 Event System** (Complete implementation)
3. **Multi-Monitor API** (Improved with MonitorId system)

---

## 1. Multi-Monitor API Enhancement

### Status: **IN PROGRESS** ✅ Week 1 Complete

### Goals
- Stable `MonitorId` system for tracking monitors across frames ✅
- Single `get_monitors()` API returning all monitors indexed by `MonitorId` ✅
- API to query which `MonitorId` the current window is on ✅
- Proper work area calculation (excluding taskbars/panels) ✅

### API Design

```rust
// Core types (core/src/window.rs)
pub struct MonitorId { pub id: usize }  // ✅ IMPLEMENTED
pub struct Monitor {
    pub id: MonitorId,                 // ✅ IMPLEMENTED
    pub name: OptionAzString,
    pub size: LayoutSize,              // Full monitor size
    pub position: LayoutPoint,         // Position in virtual screen
    pub scale_factor: f64,
    pub work_area: LayoutRect,         // ✅ IMPLEMENTED - Bounds minus taskbars
    pub video_modes: VideoModeVec,
    pub is_primary_monitor: bool,
}

// Platform layer APIs (dll/src/desktop/display.rs)
pub fn get_monitors() -> MonitorVec;                          // ✅ IMPLEMENTED
pub fn get_window_display(pos: LogicalPosition, size: LogicalSize) -> Option<DisplayInfo>; // ✅ IMPLEMENTED
```

### Implementation Tasks

#### ✅ Completed (Week 1)
- [x] Added `MonitorId` type with `PRIMARY` constant
- [x] Updated `Monitor` struct with `work_area` field
- [x] Updated `Monitor::id` to use `MonitorId` type
- [x] **Windows**: Implemented `EnumDisplayMonitors` + `GetMonitorInfoW`
  - ✅ Location: `dll/src/desktop/display.rs` (windows module)
  - ✅ Uses `EnumDisplayMonitors` callback pattern
  - ✅ Extracts bounds from `MONITORINFO.rcMonitor`
  - ✅ Calculates work area from `MONITORINFO.rcWork` (actual taskbar exclusion)
  - ✅ Per-monitor DPI via `GetDpiForMonitor`
  - ✅ Primary monitor detection via `MONITORINFOF_PRIMARY` flag
  - ✅ Proper device name extraction from UTF-16
  - ✅ Fallback to 1920x1080 default if enumeration fails
  
- [x] **macOS**: Uses `NSScreen` array with stable IDs
  - ✅ Location: `dll/src/desktop/display.rs` (macos module)
  - ✅ Maps `NSScreen` index to stable `MonitorId`
  - ✅ Extracts `visibleFrame` for work area (menu bar + dock excluded)
  - ✅ Gets scale factor from `backingScaleFactor`
  - ✅ First screen is primary (`i == 0`)
  
- [x] **X11**: Implemented XRandR extension support
  - ✅ Location: `dll/src/desktop/display.rs` (linux module)
  - ✅ Dynamic loading of `libXrandr.so.2` / `libXrandr.so`
  - ✅ Uses `XRRGetScreenResourcesCurrent` + `XRRGetCrtcInfo`
  - ✅ Enumerates all CRTCs (monitors) with bounds and positions
  - ✅ Skips disabled CRTCs (width/height = 0)
  - ✅ Work area approximation (24px panel subtraction)
  - ✅ Fallback to single-display detection if XRandR unavailable
  - ✅ DPI calculation from screen physical size (mm)

- [x] **API Integration**
  - ✅ `get_monitors()` - Returns `MonitorVec` with stable `MonitorId` values
  - ✅ `get_display_index_at_point()` - Find monitor by point
  - ✅ `get_window_display()` - Find monitor containing window center
  - ✅ `DisplayInfo::to_monitor()` - Conversion from internal to public API

#### 📋 Remaining Tasks (Week 2)
- [ ] Add `get_window_monitor_id()` using platform window handles (optional enhancement)
- [ ] Update menu positioning in `menu.rs` to use new monitor API
- [ ] Add multi-monitor tests for edge cases
- [ ] Document monitor reconnection behavior

### Testing Strategy
1. Test with single monitor (primary display)
2. Test with 2+ monitors in various layouts (side-by-side, stacked, mixed DPI)
3. Test menu positioning near screen edges
4. Test window moving between monitors
5. Test monitor disconnection/reconnection

---

## 2. GNOME Native Menus (DBus Integration)

### Status: **IN PROGRESS** - Architecture Complete, DBus Implementation Pending

### Background
The old implementation in `REFACTORING/shell/x11/menu.rs` used DBus `org.gtk.Menus` to integrate with GNOME's global menu bar. This was removed during the shell2 migration but needs to be re-implemented as an optional feature.

### Goals
- Re-implement DBus `org.gtk.Menus` protocol for GNOME Shell integration ✅
- Respect `WindowFlags::use_native_menus` configuration flag ✅
- Keep platform-specific code in `dll/src/desktop/shell2/linux/` directory ✅
- Provide graceful fallback to window-based menus ✅
- User control via environment variable ✅

### Architecture ✅ COMPLETE

```rust
// Environment variables for user control
AZUL_DISABLE_GNOME_MENUS=1  // Force CSD fallback
AZUL_GNOME_MENU_DEBUG=1     // Enable debug logging

// Location: dll/src/desktop/shell2/linux/gnome_menu.rs
pub struct GnomeMenuManager {
    app_name: String,
    is_active: Arc<AtomicBool>,
    // DBus state (to be added)
}

impl GnomeMenuManager {
    pub fn new(app_name: &str) -> Option<Self>;
    pub fn set_window_properties(&self, window_id: u64, display: *mut c_void) -> Result<(), GnomeMenuError>;
    pub fn update_menu(&self, menu: &Menu) -> Result<(), GnomeMenuError>;
    pub fn shutdown(&self);
}

// Detection function
pub fn should_use_gnome_menus() -> bool {
    // Checks:
    // 1. AZUL_DISABLE_GNOME_MENUS != 1
    // 2. XDG_CURRENT_DESKTOP contains "gnome"
    // 3. DBUS_SESSION_BUS_ADDRESS is set
}
```

### X11 Window Properties (GTK Protocol)

```
_GTK_APPLICATION_ID          = "org.example.MyApp"
_GTK_UNIQUE_BUS_NAME         = "org.gtk.MyApp"
_GTK_APPLICATION_OBJECT_PATH = "/org/gtk/MyApp"
_GTK_APP_MENU_OBJECT_PATH    = "/org/gtk/MyApp/menus/AppMenu"
_GTK_MENUBAR_OBJECT_PATH     = "/org/gtk/MyApp/menus/MenuBar"
```

### Implementation Tasks

#### ✅ Completed (Week 1)
- [x] Created `gnome_menu.rs` module with architecture
- [x] Implemented `should_use_gnome_menus()` detection function
- [x] Added environment variable support (`AZUL_DISABLE_GNOME_MENUS`)
- [x] Added debug logging (`AZUL_GNOME_MENU_DEBUG`)
- [x] Integrated into `X11Window` structure
- [x] Added graceful fallback to CSD menus
- [x] Checks `WindowFlags::use_native_menus` flag
- [x] Desktop environment detection (XDG_CURRENT_DESKTOP)
- [x] DBus session bus availability check
- [x] **REFACTORED TO MODULAR STRUCTURE** ✨
  - [x] Created separate `gnome_menu/` module directory
  - [x] `mod.rs` - Public API and coordination
  - [x] `dbus_connection.rs` - DBus connection management (structure complete)
  - [x] `menu_protocol.rs` - org.gtk.Menus implementation (logic complete)
  - [x] `actions_protocol.rs` - org.gtk.Actions implementation (logic complete)
  - [x] `menu_conversion.rs` - Menu → DBus conversion (structure ready)
  - [x] `x11_properties.rs` - X11 property setting (structure ready)
  - [x] Comprehensive unit tests for all components
  - [x] Full documentation in gnome_menu/README.md
  - [x] **Module is completely self-contained and independent**

#### 🔄 In Progress (Week 2)
- [ ] Add `dbus` crate dependency
- [ ] Implement DBus connection setup
- [ ] Register `org.gtk.Menus` interface
- [ ] Register `org.gtk.Actions` interface
- [ ] Implement X11 property setting (XInternAtom + XChangeProperty)
- [ ] Convert `Menu` structure to DBus format

#### 📋 Pending (Week 3)
- [ ] Implement action callback dispatch
- [ ] Handle menu item enable/disable
- [ ] Add submenu support
- [ ] Test on GNOME Shell 40+, 42+, 45+
- [ ] Test fallback on non-GNOME desktops (KDE, XFCE)

```
┌─────────────────────────────────────────┐
│  Application Menu (azul_core::menu)    │
└─────────────┬───────────────────────────┘
              │
      ┌───────▼────────┐
      │ Menu Backend   │
      │ Selection      │
      └───┬────────┬───┘
          │        │
    Native│        │Window-based
          │        │
┌─────────▼─┐  ┌──▼──────────┐
│ DBus Menu │  │ Azul Window │
│ (GNOME)   │  │ Popup       │
└───────────┘  └─────────────┘
```

### Implementation Location

**Primary file:** `dll/src/desktop/shell2/linux/gnome_menu.rs` (new)

**Integration points:**
- `dll/src/desktop/shell2/linux/x11/mod.rs` - Check for GNOME, call gnome_menu if available
- `dll/src/desktop/shell2/linux/wayland/mod.rs` - Same for Wayland+GNOME
- `dll/src/desktop/menu.rs` - Add backend selection logic

### DBus Protocol Overview

**Service:** `org.gtk.Menus`  
**Interface:** `com.canonical.dbusmenu`  
**Object Path:** `/com/canonical/menu/{window_id}`

**Key Methods:**
- `GetLayout()` → XML menu structure
- `Event(itemId, eventType, data, timestamp)` ← Menu click
- `AboutToShow(itemId)` → Pre-show notification

**Signals:**
- `ItemsPropertiesUpdated` - Menu items changed
- `LayoutUpdated` - Structure changed

### Implementation Tasks

#### Phase 1: DBus Connection (Week 1)
- [ ] Add `dbus` crate dependency (or use `zbus` for async)
- [ ] Create `GnomeMenuBridge` struct to manage DBus connection
- [ ] Detect GNOME Shell availability (`XDG_CURRENT_DESKTOP=GNOME`)
- [ ] Establish session bus connection
- [ ] Register application on `org.gtk.Menus` interface

#### Phase 2: Menu Translation (Week 1-2)
- [ ] Convert `azul_core::menu::Menu` → DBus menu XML
- [ ] Assign stable IDs to menu items (for click callbacks)
- [ ] Implement `GetLayout()` method
- [ ] Handle menu updates (diff old/new structure)

#### Phase 3: Event Handling (Week 2)
- [ ] Implement `Event()` handler for menu clicks
- [ ] Map DBus item IDs back to `CoreMenuCallback`
- [ ] Invoke callbacks via existing callback system
- [ ] Handle submenus and separators

#### Phase 4: Integration & Fallback (Week 2-3)
- [ ] Add `use_native_menus` flag check
- [ ] Implement fallback to window-based menus if:
  - DBus unavailable
  - Not running GNOME
  - User disabled native menus
- [ ] Add error logging for DBus failures
- [ ] Test with GNOME 40, 41, 42, 43, 44

#### Phase 5: Polish (Week 3)
- [ ] Handle menu icons (convert to DBus format)
- [ ] Support keyboard accelerators
- [ ] Support disabled/checked menu states
- [ ] Handle menu bar hiding/showing
- [ ] Document GNOME version compatibility

### Reference Implementation
See `REFACTORING/shell/x11/menu.rs` lines 1-500 for the old DBus integration code.

Key differences in new implementation:
- Use `shell2` architecture with `PlatformWindowV2` trait
- Respect `use_native_menus` flag
- Cleaner error handling with proper fallback
- Better separation of concerns (menu.rs vs gnome_menu.rs)

---

## 3. Wayland V2 Event System

### Status: **PENDING**

### Background
The Wayland backend has correct protocol scaffolding but incomplete event handlers. The V2 state-diffing system needs full integration.

### Goals
- Complete all event handler implementations
- Integrate with V2 state-diffing (`process_window_events_recursive_v2`)
- Implement `state_dirty` flag for async event accumulation
- Test across multiple compositors (Mutter, Sway, KWin)

### Current State Analysis

**File:** `dll/src/desktop/shell2/linux/wayland/mod.rs`

**Completed:**
- ✅ Protocol loading (wl_compositor, wl_seat, xdg_wm_base)
- ✅ Surface creation
- ✅ Window creation scaffolding
- ✅ EGL context setup (GPU rendering)
- ✅ SHM buffer setup (CPU fallback)

**Incomplete (Stubs):**
- ❌ Pointer event handlers (motion, button, axis)
- ❌ Keyboard event handlers (key, modifiers, repeat)
- ❌ Touch event handlers
- ❌ Window state synchronization
- ❌ V2 event processing integration

### Implementation Tasks

#### Phase 1: Pointer Events (Week 1)
- [ ] Implement `pointer_motion_handler`
  - Update `current_window_state.mouse_state.cursor_position`
  - Handle enter/leave events
  - Update hit-test results
  
- [ ] Implement `pointer_button_handler`
  - Detect mouse button down/up
  - Call `record_input_sample()` for gesture manager
  - Trigger scrollbar hit-tests
  
- [ ] Implement `pointer_axis_handler`
  - Update scroll_x/scroll_y in mouse state
  - Call `gpu_scroll()` for smooth scrolling

#### Phase 2: Keyboard Events (Week 1)
- [ ] Implement `keyboard_key_handler`
  - Map Wayland keycodes → `VirtualKeyCode`
  - Update `pressed_virtual_keycodes`
  - Handle text input events
  
- [ ] Implement `keyboard_modifiers_handler`
  - Track Shift, Ctrl, Alt, Super states
  - Update `KeyboardState`
  
- [ ] Implement `keyboard_repeat_info_handler`
  - Configure key repeat delay/rate

#### Phase 3: State Synchronization (Week 2)
- [ ] Implement `state_dirty` flag pattern:
  ```rust
  pub struct WaylandWindow {
      state_dirty: Arc<AtomicBool>,
      current_window_state: FullWindowState,
      // ...
  }
  ```
  
- [ ] Implement `sync_and_process_events()`:
  - Check `state_dirty` flag
  - If dirty: run state-diff + callbacks
  - Clear `state_dirty`
  
- [ ] Implement `sync_window_state()`:
  - Update window title (xdg_toplevel.set_title)
  - Update window size (xdg_surface.set_window_geometry)
  - Update window state (maximize/minimize/fullscreen)

#### Phase 4: V2 Integration (Week 2)
- [ ] Call `process_window_events_recursive_v2()` from event loop
- [ ] Implement previous_window_state tracking
- [ ] Handle `ProcessEventResult` enum:
  - `ShouldRegenerateDomCurrentWindow` → call regenerate_layout()
  - `ShouldReRenderCurrentWindow` → request frame callback
  
- [ ] Integrate with frame callbacks:
  - Set `frame_callback_pending` flag
  - Trigger rendering on `frame_done_callback`

#### Phase 5: Compositor Testing (Week 3)
- [ ] Test on GNOME (Mutter compositor)
  - X11 + XWayland compatibility
  - Wayland native
  
- [ ] Test on KDE (KWin compositor)
  - Window decorations
  - Multi-monitor
  
- [ ] Test on Sway (wlroots compositor)
  - Tiling behavior
  - Keyboard focus
  
- [ ] Test on Weston (reference compositor)
  - Basic functionality baseline

### Testing Strategy
1. **Input Events**: Test mouse, keyboard, scroll on each compositor
2. **Window State**: Test resize, maximize, minimize, fullscreen
3. **Multi-Window**: Test menu popups, multiple main windows
4. **Performance**: Measure frame times, ensure 60 FPS
5. **Edge Cases**: Monitor disconnect, compositor restart, screensaver

### Reference Implementation
See `dll/src/desktop/shell2/linux/x11/events.rs` for the X11 implementation pattern.

---

## Implementation Schedule

### Week 1 (Current)
- [x] Multi-Monitor API design
- [x] Update Monitor struct with MonitorId
- [ ] Windows multi-monitor implementation
- [ ] macOS multi-monitor implementation
- [ ] Start Wayland pointer events

### Week 2
- [ ] X11 XRandR implementation
- [ ] Wayland keyboard events
- [ ] Wayland state synchronization
- [ ] Start GNOME DBus connection

### Week 3
- [ ] GNOME menu translation
- [ ] GNOME event handling
- [ ] Wayland V2 integration
- [ ] Compositor testing

### Week 4
- [ ] GNOME fallback logic
- [ ] Wayland multi-compositor testing
- [ ] Integration testing
- [ ] Documentation updates

---

## Dependencies

### New Crates Needed
- `dbus` or `zbus` - For GNOME menu integration
- `xrandr` (via dlopen) - For X11 multi-monitor
- No new crates for Wayland (protocols already loaded)

### Existing Code References
- `REFACTORING/shell/x11/menu.rs` - Old GNOME menu implementation
- `dll/src/desktop/shell2/linux/x11/events.rs` - X11 event pattern
- `dll/src/desktop/shell2/common/event_v2.rs` - V2 state-diffing logic

---

## Success Criteria

### Multi-Monitor
- ✅ All platforms enumerate monitors correctly
- ✅ Menus don't overflow screen edges
- ✅ Window position tracked across monitors
- ✅ DPI changes handled when moving windows

### GNOME Menus
- ✅ Native menu bar appears in GNOME Shell
- ✅ Menu clicks invoke correct callbacks
- ✅ Graceful fallback when DBus unavailable
- ✅ Works on GNOME 40+

### Wayland V2
- ✅ All input events work correctly
- ✅ State-diffing produces correct synthetic events
- ✅ Callbacks invoked properly
- ✅ 60 FPS rendering on all compositors tested

---

## Notes

- All platform-specific code stays in respective directories:
  - `dll/src/desktop/shell2/windows/` - Windows
  - `dll/src/desktop/shell2/macos/` - macOS
  - `dll/src/desktop/shell2/linux/` - Linux (X11/Wayland/GNOME)

- GNOME menu integration is **optional** - controlled by `use_native_menus` flag
- Multi-monitor API is **required** for production quality
- Wayland V2 is **high priority** for future Linux support

---

## Questions / Blockers

1. **GNOME Menus**: Should we support older GNOME versions (< 40) or only modern ones?
   - **Decision**: Support GNOME 40+ only, document minimum version

2. **Wayland**: Should we implement wlr-layer-shell for popups or stick with xdg-popup?
   - **Decision**: Use xdg-popup (standard protocol)

3. **Multi-Monitor**: How to handle hot-plugging monitors?
   - **Decision**: Re-query monitors on display change events, invalidate cached MonitorIds

---

## Completion Checklist

- [ ] Multi-Monitor API complete on all platforms
- [ ] GNOME native menus working with fallback
- [ ] Wayland V2 event system complete
- [ ] All tests passing
- [ ] Documentation updated
- [ ] `stilltodo.md` status updated to reflect completion
