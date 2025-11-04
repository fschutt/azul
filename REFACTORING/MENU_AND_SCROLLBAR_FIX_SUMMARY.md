# Menu and Scrollbar Fix Summary

## Session Progress Report

### Completed Tasks ✅

#### 1. Removed Duplicate Scrollbar Methods from macOS (NEW - Just Completed)

**Problem Identified:**
- macOS (`dll/src/desktop/shell2/macos/events.rs`) had duplicate implementations of scrollbar handling methods that shadowed the cross-platform trait methods
- The `PlatformWindowV2` trait in `event_v2.rs` already provides these as default methods
- This duplication violated the DRY principle and made maintenance harder

**Files Modified:**
- `/Users/fschutt/Development/azul/dll/src/desktop/shell2/macos/events.rs`

**Changes Made:**

1. **Removed 3 duplicate methods (~280 lines of code):**
   - `perform_scrollbar_hit_test()` 
   - `handle_scrollbar_click()`
   - `handle_scrollbar_drag()`
   - Also removed the helper: `handle_track_click()`

2. **Updated call sites to use trait methods:**
   ```rust
   // OLD:
   if let Some(scrollbar_hit_id) = self.perform_scrollbar_hit_test(position) {
       return self.handle_scrollbar_click(scrollbar_hit_id, position);
   }
   
   // NEW:
   if let Some(scrollbar_hit_id) = PlatformWindowV2::perform_scrollbar_hit_test(self, position) {
       let result = PlatformWindowV2::handle_scrollbar_click(self, scrollbar_hit_id, position);
       return Self::convert_process_result(result);
   }
   ```

3. **Added documentation comment:**
   ```rust
   // NOTE: perform_scrollbar_hit_test(), handle_scrollbar_click(), and handle_scrollbar_drag()
   // are now provided by the PlatformWindowV2 trait as default methods.
   // The trait methods are cross-platform and work identically.
   // See dll/src/desktop/shell2/common/event_v2.rs for the implementation.
   ```

**Benefits:**
- ✅ Eliminated ~280 lines of duplicate code
- ✅ Uses cross-platform implementation (same behavior on all platforms)
- ✅ Easier to maintain (one implementation for all platforms)
- ✅ No compilation errors
- ✅ macOS now properly integrated with V2 unified event system

**Key Insight:**
The difference between `ProcessEventResult` (cross-platform) and `EventProcessResult` (macOS-specific) was the reason for the duplication. By using the existing `convert_process_result()` helper, we can bridge these types and use the trait methods directly.

---

### ✅ 2. Fixed macOS Menu Callback Invocation (Previously Completed)
**Files Modified:** 
- `dll/src/desktop/shell2/macos/menu.rs`
- `dll/src/desktop/shell2/macos/mod.rs`

**Changes:**
- Modified `MenuState.command_map` to store `CoreMenuCallback` directly instead of just indices
- Updated `build_menu_items()` to store the full callback object when building native menus
- Implemented `handle_menu_action()` to properly invoke menu callbacks:
  - Converts `CoreMenuCallback` to `MenuCallback`
  - Creates `CallbackInfo` with all necessary context
  - Invokes the callback function
  - Processes the result (`Update::RefreshDom` triggers layout regeneration)

**Result:** macOS menu clicks now properly execute user callbacks and trigger DOM updates.

---

### ✅ 3. Fixed Windows Menu Callback Invocation (Previously Completed)
**Files Modified:**
- `dll/src/desktop/shell2/windows/mod.rs`

**Changes:**
- Implemented `WM_COMMAND` handler to look up and invoke menu callbacks
- Extracts `command_id` from `wparam`
- Looks up callback from `menu_bar` or `context_menu` maps
- Converts `CoreMenuCallback` to `MenuCallback`
- Creates `CallbackInfo` and invokes the callback
- Processes results to trigger layout regeneration if needed

**Result:** Windows menu clicks now properly execute user callbacks and trigger DOM updates.


---

## Remaining Critical Work

### 🔄 1. Remove Duplicate Scrollbar Methods from Windows (Next Priority)
**Status:** Not started
**Estimated Impact:** ~300 lines eliminated

Same issue as macOS - Windows has duplicate implementations in `dll/src/desktop/shell2/windows/mod.rs`.

**Action:**
- Remove local `perform_scrollbar_hit_test()` implementation
- Update call sites to use `PlatformWindowV2::perform_scrollbar_hit_test()`
- Use result conversion if Windows has platform-specific result type

---

### 🔄 2. Remove Duplicate Scrollbar Methods from X11 (High Priority)
**Status:** Not started
**Estimated Impact:** ~300 lines eliminated

Same issue - X11 has duplicate implementations in `dll/src/desktop/shell2/linux/x11/events.rs`.

**Action:**
- Remove local scrollbar method implementations
- Update call sites to use trait methods
- Use result conversion if needed

---

### 🔄 3. Extract `regenerate_layout()` to Common Module (High Priority)
### 🔄 3. Extract `regenerate_layout()` to Common Module (High Priority)
**Status:** Not started
**Estimated Impact:** ~300 lines eliminated (100 per platform)

**Problem:** `regenerate_layout()` is ~90% identical across platforms but duplicated in:
- `dll/src/desktop/shell2/macos/mod.rs`
- `dll/src/desktop/shell2/windows/mod.rs`
- `dll/src/desktop/shell2/linux/x11/mod.rs`

**Solution:** Create `dll/src/desktop/shell2/common/layout_v2.rs` with unified implementation.

---

### 🔄 4. Add Drag and Double-Click Events
**Status:** Not started

**Required Changes:**

#### 4.1 Core Event Types (`core/src/events.rs`)
Add to `HoverEventFilter` enum:
```rust
pub enum HoverEventFilter {
    // ... existing variants ...
    DragStart,      // Mouse button down, start of potential drag
    Drag,           // Mouse moved while button down
    DragEnd,        // Mouse button up, end of drag
    DoubleClick,    // Two rapid clicks (OS determines timing)
}
```

Mirror changes in `FocusEventFilter` and `WindowEventFilter`.

#### 4.2 Mouse State (`core/src/window.rs`)
Add to `MouseState`:
```rust
pub struct MouseState {
    // ... existing fields ...
    pub drag_state: DragState,
    pub double_click_detected: bool,
    
    // X11/Wayland double-click detection
    pub last_click_time: Option<Instant>,
    pub last_click_position: Option<LogicalPosition>,
}

pub enum DragState {
    NotDragging,
    DragStarted { start_pos: LogicalPosition },
    Dragging { start_pos: LogicalPosition },
}
```

#### 4.3 Event Generation (`event_v2.rs`)
Update `create_events_from_states()` to detect drag state transitions:
```rust
match (&prev.mouse_state.drag_state, &curr.mouse_state.drag_state) {
    (DragState::NotDragging, DragState::DragStarted { .. }) => {
        events.push(SyntheticEvent::DragStart);
    }
    (DragState::DragStarted { .. } | DragState::Dragging { .. }, 
     DragState::Dragging { .. }) => {
        events.push(SyntheticEvent::Drag);
    }
    (DragState::DragStarted { .. } | DragState::Dragging { .. }, 
     DragState::NotDragging) => {
        events.push(SyntheticEvent::DragEnd);
    }
    _ => {}
}

// Double-click detection
if curr.mouse_state.double_click_detected && !prev.mouse_state.double_click_detected {
    events.push(SyntheticEvent::DoubleClick);
}
```

#### 4.4 Platform Integration

**macOS:**
```rust
// In handle_mouse_down:
if event.clickCount() == 2 {
    current_window_state.mouse_state.double_click_detected = true;
}

// Drag state machine:
// Mouse down -> DragStarted
// Mouse dragged -> Dragging
// Mouse up -> NotDragging
```

**Windows:**
```rust
// Add WM_LBUTTONDBLCLK handler:
WM_LBUTTONDBLCLK => {
    current_window_state.mouse_state.double_click_detected = true;
    // ... existing mouse down logic ...
}

// Drag state machine in WM_LBUTTONDOWN, WM_MOUSEMOVE, WM_LBUTTONUP
```

**X11:**
```rust
// Timing-based double-click (no native support):
ButtonPress => {
    let now = Instant::now();
    let is_double_click = if let Some(last_time) = last_click_time {
        let elapsed = now.duration_since(last_time);
        elapsed < Duration::from_millis(500) && 
        mouse_position.distance(last_click_position) < 5.0
    } else {
        false
    };
    
    if is_double_click {
        current_window_state.mouse_state.double_click_detected = true;
    }
    
    last_click_time = Some(now);
    last_click_position = Some(mouse_position);
}

// Drag state machine in ButtonPress, MotionNotify, ButtonRelease
```

---

### 🔄 5. Refactor CSD Titlebar Callbacks
**Status:** Not started

**Current:** Uses `MouseOver` event as workaround
**Target:** Use proper `DragStart`/`Drag`/`DragEnd` events

**File:** `dll/src/desktop/csd.rs`

**Changes Required:**
1. Split `csd_titlebar_drag_callback` into three callbacks:
   - `csd_titlebar_drag_start_callback` (On::DragStart)
   - `csd_titlebar_drag_callback` (On::Drag)
   - `csd_titlebar_drag_end_callback` (On::DragEnd)

2. Wire up `csd_titlebar_doubleclick_callback` properly in `create_titlebar_dom()`:
```rust
CoreCallbackData {
    event: EventFilter::Hover(HoverEventFilter::DoubleClick),
    callback: CoreCallback {
        cb: csd_titlebar_doubleclick_callback as usize,
    },
    data: RefAny::new(()),
}
```

**Dependencies:** Requires drag/double-click events to be implemented first (item 4).

---

### 🔄 6. Extract `regenerate_layout()` to Common Module
**Status:** Not started

**Problem:** `regenerate_layout()` is duplicated across:
- `dll/src/desktop/shell2/macos/mod.rs`
- `dll/src/desktop/shell2/windows/mod.rs`
- `dll/src/desktop/shell2/linux/x11/mod.rs`

**Solution:** Create `dll/src/desktop/shell2/common/layout_v2.rs` with unified implementation.

**Estimated Impact:** ~100 lines eliminated per platform (~300 lines total)

---

### 🔄 7. Add Native Menu Preference
**Status:** Not started

**Goal:** Allow users to choose between native and cross-platform menus

**Changes Required:**
1. Add to `WindowState` (in `azul_layout`):
```rust
pub struct WindowState {
    // ... existing fields ...
    pub prefer_native_menus: bool, // Default: true
}
```

2. Check flag when creating menus:
```rust
if window_state.prefer_native_menus {
    // Use native menus (macOS NSMenu, Windows HMENU)
} else {
    // Use cross-platform window-based menus
}
```

3. For X11/GNOME:
   - If `prefer_native_menus == true`, use DBus `org.gtk.Menus` integration
   - Otherwise use window-based menus

**Note:** Native menus not possible on Wayland (always use window-based).

---

### 🔄 8. Restore X11/GNOME Native Menus (Optional)
**Status:** Not started

**Regression:** The old `REFACTORING/shell/x11/menu.rs` had DBus integration for native GNOME menus, which was removed in shell2.

**Decision Required:** Should this be restored?
- **Pro:** Native feel on GNOME desktops
- **Con:** Complex DBus protocol, maintenance burden

If restored:
- Implement in `dll/src/desktop/shell2/linux/x11/gnome_menu.rs`
- Only activate if `prefer_native_menus == true`
- Fallback to window-based menus if DBus unavailable

---

## Testing Checklist

### Menu Callbacks
- [x] macOS menu bar clicks invoke callbacks
- [x] Windows menu bar clicks invoke callbacks  
- [ ] X11 menu clicks invoke callbacks (needs testing)
- [ ] Context menu clicks invoke callbacks (all platforms)
- [ ] Nested menu items work correctly
- [ ] Menu callbacks can trigger `Update::RefreshDom`

### Scrollbar Interaction
- [ ] Scrollbar hit-test works on all platforms
- [ ] Thumb drag scrolls content correctly
- [ ] Track click jumps to position
- [ ] GPU scroll updates work (no relayout)
- [ ] Scrollbar appears/disappears based on content size

### Drag Events (Future)
- [ ] DragStart fires on mouse down
- [ ] Drag fires continuously while dragging
- [ ] DragEnd fires on mouse up
- [ ] CSD titlebar drag moves window
- [ ] Drag events work on custom UI components

### Double-Click Events (Future)
- [ ] Double-click detected on macOS (native)
- [ ] Double-click detected on Windows (native)
- [ ] Double-click detected on X11 (timing-based)
- [ ] CSD titlebar double-click toggles maximize
- [ ] Double-click events work on custom UI components

---

## Architecture Impact

### Code Elimination Progress
- ✅ Menu callback logic: ~50 lines added (fixes regression)
- 🔄 Scrollbar methods: ~900 lines can be eliminated (when migration complete)
- 🔄 Layout regeneration: ~300 lines can be eliminated (when unified)
- **Total potential:** ~1150 lines eliminated

### Remaining Duplication
1. Scrollbar methods (macOS, Windows, X11)
2. Layout regeneration (macOS, Windows, X11)  
3. Platform-specific event type conversions

---

## Priority Ranking

### High Priority (Blocking Production)
1. ✅ **Menu callback invocation** (macOS, Windows) - COMPLETE
2. 🔄 **Scrollbar hit-test unification** - IN PROGRESS
3. 🔄 **Menu callback invocation (X11)** - NEEDS IMPLEMENTATION

### Medium Priority (Nice to Have)
4. Extract `regenerate_layout()` to common module
5. Add native menu preference flag
6. Implement drag/double-click events

### Low Priority (Future Enhancement)
7. Restore X11/GNOME native DBus menus
8. Wayland V2 event port completion
9. XRandR multi-monitor support for X11

---

## Conclusion

The critical menu callback regression has been fixed for macOS and Windows. The scrollbar handling unification is architecturally sound but needs final migration to eliminate duplicate code. The addition of proper drag and double-click events will complete the semantic correctness of the event system.

Next immediate steps:
1. ✅ **DONE:** Remove duplicate scrollbar methods from macOS (~280 lines)
2. 🔄 **NEXT:** Remove duplicate scrollbar methods from Windows (~300 lines)
3. 🔄 Remove duplicate scrollbar methods from X11 (~300 lines)
4. 🔄 Extract `regenerate_layout()` to common module (~300 lines)
5. 🔄 Implement drag/double-click event system
6. Test menu callbacks on X11
7. Verify all platforms use cross-platform event processing correctly

---

## Current Session Metrics

## Current Session Metrics

**Code Elimination Progress:**
- ✅ macOS scrollbar methods: ~280 lines removed
- ✅ Windows scrollbar methods: ~300 lines removed
- ✅ X11 scrollbar methods: Already using trait methods (no duplicates)
- ✅ `regenerate_layout()` duplication: ~300 lines removed (NEW - COMPLETE)
- **Total scrollbar elimination: ~580 lines**
- **Total regenerate_layout elimination: ~300 lines**
- **Grand total eliminated: ~880 lines**

**Tasks Completed This Session:**
- 3/20 major tasks completed:
  - ✅ Task #13: macOS scrollbar duplicates removed
  - ✅ Task #14: Windows scrollbar duplicates removed
  - ✅ Task #18: regenerate_layout extracted to common/layout_v2.rs (NEW - USER TOP PRIORITY COMPLETE)
- ✅ Fixed Windows GNU compilation errors (duplicate import, menu_bar field access)
- ✅ All platforms compile successfully (macOS, Windows GNU, Linux)
- ✅ All platforms properly integrated with V2 unified event and layout systems

**Architecture Improvements:**
- ✅ macOS now uses cross-platform scrollbar handling from `PlatformWindowV2` trait
- ✅ Windows now uses cross-platform scrollbar handling from `PlatformWindowV2` trait
- ✅ X11 already using cross-platform scrollbar handling (verified)
- ✅ macOS now uses unified `layout_v2::regenerate_layout()` (NEW)
- ✅ Windows now uses unified `layout_v2::regenerate_layout()` with full layout callback support (NEW - previously incomplete)
- ✅ X11 now uses unified `layout_v2::regenerate_layout()` (NEW)
- ✅ Demonstrated the pattern for bridging platform-specific result types
- ✅ Reduced maintenance burden - one implementation for both scrollbar and layout logic across all platforms
- ✅ Eliminated ~880 lines of duplicate code (74.5% of original 1180 line target)
