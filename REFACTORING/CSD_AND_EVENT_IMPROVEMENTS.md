# CSD and Event System Improvements

## Overview

This document addresses the remaining criticisms in the windowing systems analysis and proposes improvements to make CSD titlebar dragging use standard DOM event callbacks and properly handle double-click events.

## Current Issues

### 1. CSD Titlebar Drag Implementation
**Current State:** The titlebar drag is implemented using `On::MouseOver` (which fires continuously while the mouse is over the element with the left button down). This is a workaround because dedicated drag events don't exist yet.

**Problem:** This isn't semantic - titlebar dragging should use proper drag events (`On::DragStart`, `On::Drag`, `On::DragEnd`) that are attached to the `.csd-titlebar` DOM node.

**File:** `dll/src/desktop/csd.rs:111` - `csd_titlebar_drag_callback`

### 2. Double-Click Event Handling
**Current State:** The `csd_titlebar_doubleclick_callback` exists but is **not wired up** to any DOM node. There's no `On::DoubleClick` event filter.

**Problem:** Double-click detection is platform-specific because each OS has different double-click timing settings. The application can't reliably detect double-clicks without platform support.

**File:** `dll/src/desktop/csd.rs:171` - `csd_titlebar_doubleclick_callback` (unused)

### 3. Event System Gaps
The current `HoverEventFilter` enum (in `core/src/events.rs:1336`) doesn't include:
- `DragStart` - Mouse button pressed down (start of potential drag)
- `Drag` - Mouse moved while button is down (dragging)
- `DragEnd` - Mouse button released (end of drag)
- `DoubleClick` - Two rapid clicks (timing determined by OS)

---

## Proposed Solution

### Phase 1: Add Missing Event Filters

#### 1.1 Extend `HoverEventFilter` enum

**File:** `core/src/events.rs` (around line 1336)

```rust
pub enum HoverEventFilter {
    MouseOver,
    MouseDown,
    LeftMouseDown,
    RightMouseDown,
    MiddleMouseDown,
    MouseUp,
    LeftMouseUp,
    RightMouseUp,
    MiddleMouseUp,
    MouseEnter,
    MouseLeave,
    Scroll,
    ScrollStart,
    ScrollEnd,
    TextInput,
    VirtualKeyDown,
    VirtualKeyUp,
    HoveredFile,
    DroppedFile,
    HoveredFileCancelled,
    TouchStart,
    TouchMove,
    TouchEnd,
    TouchCancel,
    
    // NEW: Drag events
    DragStart,      // Mouse button down, start of potential drag
    Drag,           // Mouse moved while button down (dragging)
    DragEnd,        // Mouse button up, end of drag
    
    // NEW: Double-click event
    DoubleClick,    // Two rapid clicks (OS determines timing)
}
```

#### 1.2 Add corresponding variants to `FocusEventFilter` and `WindowEventFilter`

These enums mirror `HoverEventFilter`, so they need the same additions:

```rust
pub enum FocusEventFilter {
    // ... existing variants ...
    DragStart,
    Drag,
    DragEnd,
    DoubleClick,
}

pub enum WindowEventFilter {
    // ... existing variants ...
    DragStart,
    Drag,
    DragEnd,
    DoubleClick,
}
```

#### 1.3 Update conversion methods

Ensure `to_focus_event_filter()` and `to_hover_event_filter()` in `events.rs` handle the new variants.

---

### Phase 2: Platform-Specific Event Generation

Each platform's event handler must detect and generate these new synthetic events.

#### 2.1 Double-Click Detection

**Approach:** The platform event loop receives native double-click events from the OS:
- **Windows:** `WM_LBUTTONDBLCLK` message
- **macOS:** `NSEvent.clickCount() == 2`
- **X11:** No native double-click - must track time between clicks
- **Wayland:** No native double-click - must track time between clicks

**Implementation Strategy:**

For **Windows** (`dll/src/desktop/shell2/windows/mod.rs`):
```rust
// In window_proc:
WM_LBUTTONDBLCLK => {
    // Update current_window_state
    current_window_state.mouse_state.left_down = true;
    current_window_state.mouse_state.double_click_detected = true; // NEW FLAG
    
    // Process events (state-diff will generate DoubleClick event)
    let result = self.process_window_events_recursive_v2(&previous_window_state);
    
    // Clear flag after processing
    current_window_state.mouse_state.double_click_detected = false;
    
    // ... handle result ...
}
```

For **macOS** (`dll/src/desktop/shell2/macos/events.rs`):
```rust
// In handle_mouse_button:
if event.clickCount() == 2 {
    current_window_state.mouse_state.double_click_detected = true;
}
```

For **X11/Wayland** (no native support):
```rust
// Add to WindowState:
pub struct MouseState {
    // ... existing fields ...
    last_click_time: Option<Instant>,
    last_click_position: Option<LogicalPosition>,
}

// In mouse button down handler:
let now = Instant::now();
let is_double_click = if let Some(last_time) = last_click_time {
    let elapsed = now.duration_since(last_time);
    // Use 500ms as default (could be configurable)
    elapsed < Duration::from_millis(500) &&
    mouse_position.distance(last_click_position) < 5.0  // Small movement tolerance
} else {
    false
};

if is_double_click {
    current_window_state.mouse_state.double_click_detected = true;
}

last_click_time = Some(now);
last_click_position = Some(mouse_position);
```

**Key Insight:** The application **cannot** reliably determine the OS double-click timing without platform information. Therefore:
- Native double-click events should be preferred when available
- Fallback implementation uses a reasonable default (500ms)
- This could be exposed as a configurable setting later

#### 2.2 Drag Event Detection

**State Machine Approach:**

```rust
pub struct MouseState {
    // ... existing fields ...
    pub drag_state: DragState,
}

pub enum DragState {
    NotDragging,
    DragStarted { start_pos: LogicalPosition },
    Dragging { start_pos: LogicalPosition },
}
```

**State Transitions:**

1. **Mouse Down** → `DragState::DragStarted { start_pos }`
   - Generates `DragStart` synthetic event
   
2. **Mouse Move** (while button down) → `DragState::Dragging { start_pos }`
   - Generates `Drag` synthetic event
   
3. **Mouse Up** → `DragState::NotDragging`
   - Generates `DragEnd` synthetic event

**Implementation in `event_v2.rs::create_events_from_states()`:**

```rust
// After processing mouse button states:
let drag_events = match (
    &previous_state.mouse_state.drag_state,
    &current_state.mouse_state.drag_state,
) {
    // Drag started
    (DragState::NotDragging, DragState::DragStarted { .. }) => {
        vec![SyntheticEvent::DragStart]
    }
    
    // Dragging continues
    (DragState::DragStarted { .. }, DragState::Dragging { .. }) |
    (DragState::Dragging { .. }, DragState::Dragging { .. }) => {
        vec![SyntheticEvent::Drag]
    }
    
    // Drag ended
    (DragState::DragStarted { .. } | DragState::Dragging { .. }, DragState::NotDragging) => {
        vec![SyntheticEvent::DragEnd]
    }
    
    _ => vec![],
};

events.extend(drag_events);
```

---

### Phase 3: Refactor CSD Callbacks

#### 3.1 Replace `csd_titlebar_drag_callback` with proper drag events

**Current implementation** (`dll/src/desktop/csd.rs:111`):
```rust
extern "C" fn csd_titlebar_drag_callback(_data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    // Uses MouseOver + checks if left_down
    // Calculates delta from previous position
    // Updates window position
}
```

**New implementation:**

```rust
/// Callback for titlebar drag start - records initial drag position
extern "C" fn csd_titlebar_drag_start_callback(_data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    use azul_core::geom::LogicalPosition;
    
    let mouse_state = info.get_current_mouse_state();
    let drag_start_pos = match mouse_state.cursor_position.get_position() {
        Some(pos) => pos,
        None => return Update::DoNothing,
    };
    
    // Store drag start position in window state (could use a custom flag)
    eprintln!("[CSD] Titlebar drag started at ({}, {})", drag_start_pos.x, drag_start_pos.y);
    
    Update::DoNothing
}

/// Callback for titlebar drag - updates window position based on mouse delta
extern "C" fn csd_titlebar_drag_callback(_data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    use azul_core::{geom::PhysicalPosition, window::WindowPosition};
    
    // Get current and previous mouse positions
    let mouse_state = info.get_current_mouse_state();
    let prev_mouse_state = match info.get_previous_mouse_state() {
        Some(s) => s,
        None => return Update::DoNothing,
    };
    
    let current_pos = match mouse_state.cursor_position.get_position() {
        Some(pos) => pos,
        None => return Update::DoNothing,
    };
    
    let prev_pos = match prev_mouse_state.cursor_position.get_position() {
        Some(pos) => pos,
        None => return Update::DoNothing,
    };
    
    // Calculate delta
    let delta_x = (current_pos.x - prev_pos.x) as i32;
    let delta_y = (current_pos.y - prev_pos.y) as i32;
    
    if delta_x == 0 && delta_y == 0 {
        return Update::DoNothing;
    }
    
    // Update window position
    let mut window_state = info.get_current_window_state();
    match window_state.position {
        WindowPosition::Initialized(ref mut pos) => {
            pos.x += delta_x;
            pos.y += delta_y;
            info.set_window_state(window_state);
            Update::DoNothing
        }
        WindowPosition::Uninitialized => Update::DoNothing,
    }
}

/// Callback for titlebar drag end - finalize drag operation
extern "C" fn csd_titlebar_drag_end_callback(_data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    eprintln!("[CSD] Titlebar drag ended");
    Update::DoNothing
}
```

#### 3.2 Wire up double-click callback properly

**Current:** `csd_titlebar_doubleclick_callback` is defined but **never attached** to any DOM node.

**Fix:** In `create_titlebar_dom()`, attach it to the title element:

```rust
fn create_titlebar_dom(
    title: &str,
    has_minimize: bool,
    has_maximize: bool,
    has_close: bool,
) -> Dom {
    // ... existing button code ...
    
    // Title text with drag AND double-click callbacks
    let title_classes = IdOrClassVec::from_vec(vec![IdOrClass::Class("csd-title".into())]);
    let title_text = Dom::div()
        .with_ids_and_classes(title_classes)
        .with_child(Dom::text(title))
        .with_callbacks(
            vec![
                // Drag start callback
                CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::DragStart),
                    callback: CoreCallback {
                        cb: csd_titlebar_drag_start_callback as usize,
                    },
                    data: RefAny::new(()),
                },
                // Drag callback
                CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::Drag),
                    callback: CoreCallback {
                        cb: csd_titlebar_drag_callback as usize,
                    },
                    data: RefAny::new(()),
                },
                // Drag end callback
                CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::DragEnd),
                    callback: CoreCallback {
                        cb: csd_titlebar_drag_end_callback as usize,
                    },
                    data: RefAny::new(()),
                },
                // Double-click callback (maximize/restore)
                CoreCallbackData {
                    event: EventFilter::Hover(HoverEventFilter::DoubleClick),
                    callback: CoreCallback {
                        cb: csd_titlebar_doubleclick_callback as usize,
                    },
                    data: RefAny::new(()),
                },
            ]
            .into(),
        );
    
    // ... rest of function ...
}
```

---

### Phase 4: Platform Integration Checklist

For each platform, ensure:

#### ✅ macOS (`dll/src/desktop/shell2/macos/`)
- [ ] Add `double_click_detected` flag to `MouseState`
- [ ] Detect double-clicks via `NSEvent.clickCount()`
- [ ] Implement drag state machine in mouse handlers
- [ ] Generate `DragStart`, `Drag`, `DragEnd` synthetic events
- [ ] Update `create_events_from_states()` to handle new events

#### ✅ Windows (`dll/src/desktop/shell2/windows/`)
- [ ] Add `double_click_detected` flag to `MouseState`
- [ ] Handle `WM_LBUTTONDBLCLK` message
- [ ] Implement drag state machine in `window_proc`
- [ ] Generate drag events in state-diff

#### ✅ X11 (`dll/src/desktop/shell2/linux/x11/`)
- [ ] Add `last_click_time` and `last_click_position` to `MouseState`
- [ ] Implement double-click detection with 500ms threshold
- [ ] Implement drag state machine
- [ ] Generate synthetic events

#### ⚠️ Wayland (`dll/src/desktop/shell2/linux/wayland/`)
- [ ] Same as X11 (once V2 port is complete)

---

## Benefits

### 1. Semantic Correctness
- Drag operations use proper `DragStart`/`Drag`/`DragEnd` events
- Double-click uses OS-aware timing
- Events match user expectations and platform conventions

### 2. Reduced Special-Case Code
- CSD titlebar drag is now just a regular DOM callback
- No more `MouseOver` workaround checking `left_down`
- Double-click callback is properly wired up

### 3. Reusability
- New drag events can be used by **any** UI component (not just titlebar)
- Other components can implement draggable behavior
- Double-click can be used for any UI element

### 4. Platform Correctness
- Windows: Uses native `WM_LBUTTONDBLCLK`
- macOS: Uses `NSEvent.clickCount()`
- X11/Wayland: Fallback with reasonable defaults

---

## Implementation Priority

### High Priority (Blocking CSD completion)
1. Add `DragStart`, `Drag`, `DragEnd` to `HoverEventFilter`
2. Implement drag state machine in `event_v2.rs`
3. Update all platform event handlers to generate drag events
4. Refactor `csd_titlebar_drag_callback` to use new events

### Medium Priority (Nice to have)
1. Add `DoubleClick` to `HoverEventFilter`
2. Implement platform-specific double-click detection
3. Wire up `csd_titlebar_doubleclick_callback` properly

### Low Priority (Future enhancement)
1. Expose double-click timing as configurable setting
2. Add drag threshold (minimum movement before drag starts)
3. Implement drag preview/visual feedback system

---

## Estimated Code Changes

### Lines to Add: ~300
- Event enum variants: ~20 lines
- State machine logic: ~100 lines
- Platform handlers (4 platforms × 30 lines): ~120 lines
- CSD callback refactoring: ~60 lines

### Lines to Remove: ~50
- Old `MouseOver` workaround: ~30 lines
- Unused callback stub comments: ~20 lines

### Net Change: ~+250 lines
This is acceptable given the semantic correctness gained.

---

## Testing Strategy

### Unit Tests
- Test drag state transitions
- Test double-click timing logic (X11/Wayland)
- Test event generation from state diffs

### Integration Tests
- Drag titlebar to move window (all platforms)
- Double-click titlebar to maximize (all platforms)
- Verify OS double-click settings are respected (Windows/macOS)

### Manual Testing
- Test with different DPI scaling
- Test with multi-monitor setups
- Test with accessibility tools (screen readers shouldn't see spurious drag events)

---

## Summary

This refactoring addresses the two main criticisms:

1. **"handle CSD titlebar drag should be a simple On::DragStart / On::Drag / On::DragEnd callback"**
   - ✅ Fixed by adding drag events to `HoverEventFilter`
   - ✅ Fixed by implementing drag state machine
   - ✅ Fixed by refactoring CSD callbacks to use semantic events

2. **"double click event handling is special because the application can't know system settings"**
   - ✅ Fixed by using native double-click events (Windows/macOS)
   - ✅ Fixed by implementing reasonable fallback (X11/Wayland)
   - ✅ Fixed by properly wiring up the callback

The solution is architecturally sound, maintains the V2 unified event model, and improves code reusability across the entire framework.
