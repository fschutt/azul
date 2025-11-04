# Accessibility Event Integration Status

**Last Updated:** Current Session (Revised Architecture)  
**Status:** ✅ Clean Architecture Complete

## Final Architecture

### Simple, Clean Design ✅

The accessibility integration now follows the same pattern as `record_input_sample()` for gestures:

**Single Entry Point:**
```rust
fn record_accessibility_action(
    &mut self,
    dom_id: DomId,
    node_id: NodeId,
    action: AccessibilityAction,
) -> Vec<EventFilter>
```

**How It Works:**

1. **Platform receives A11y action** from screen reader (via callback, not polling)
2. **Call `record_accessibility_action()`** with the action details
3. **Function has direct access** to all managers via `layout_window`:
   - `scroll_states` - For scroll actions
   - `focus_manager` - For focus actions  
   - `cursor_manager` - For text cursor
   - `selection_manager` - For text selection
   - `text_cache` - For text editing
4. **Function modifies state directly** (scroll, focus, cursor, etc.)
5. **Function returns synthetic EventFilters** for callback actions
6. **Platform stores these** to merge with regular events
7. **Normal event loop processes everything** together

**Key Benefits:**
- ✅ No separate code path - unified event processing
- ✅ No polling - callback-based (platform implements callbacks)
- ✅ Direct manager access - no indirection
- ✅ Simple API - just like `record_input_sample()`
- ✅ Proper state management - all changes go through managers
- ✅ EventFilters integrate seamlessly with regular events

## Implementation Details

### event_v2.rs (Trait Method)

**Location:** `dll/src/desktop/shell2/common/event_v2.rs` lines ~695-720

```rust
/// V2: Record accessibility action and generate synthetic events.
///
/// Similar to `record_input_sample()` for gestures, this method takes an incoming
/// accessibility action from assistive technologies (screen readers), applies
/// necessary state changes to managers (scroll, focus, cursor, selection), and
/// returns synthetic EventFilters to be injected into the event processing loop.
#[cfg(feature = "accessibility")]
fn record_accessibility_action(
    &mut self,
    dom_id: azul_core::dom::DomId,
    node_id: azul_core::dom::NodeId,
    action: azul_core::dom::AccessibilityAction,
) -> Vec<EventFilter> {
    let layout_window = match self.get_layout_window_mut() {
        Some(lw) => lw,
        None => return Vec::new(),
    };

    let now = std::time::Instant::now();
    
    // Delegate to LayoutWindow's process_accessibility_action
    // This has direct mutable access to all managers and returns synthetic events
    layout_window.process_accessibility_action(dom_id, node_id, action, now)
}
```

**Status:** ✅ Complete - Cross-platform implementation

### window.rs (Manager Access)

**Location:** `layout/src/window.rs` lines 2410-2658

The existing `process_accessibility_action()` already has direct access to all managers:

```rust
pub fn process_accessibility_action(
    &mut self,
    dom_id: DomId,
    node_id: NodeId,
    action: AccessibilityAction,
    now: Instant
) -> Vec<EventFilter> {
    let mut synthetic_events = Vec::new();
    
    match action {
        // Manager actions - direct state manipulation
        AccessibilityAction::Focus => {
            self.focus_manager.set_focused_node(Some((dom_id, node_id)));
        }
        AccessibilityAction::ScrollUp => {
            self.scroll_states.scroll_by(dom_id, node_id, ...);
        }
        
        // Callback actions - return EventFilters
        AccessibilityAction::Default | Increment | Decrement => {
            if node_has_callback(event_type) {
                synthetic_events.push(event_type.into());
            } else {
                synthetic_events.push(HoverEventFilter::MouseUp.into());
            }
        }
    }
    
    synthetic_events
}
```

**Status:** ✅ Complete - Has all manager access needed

## Platform Integration Guide

### How Platforms Should Integrate

Each platform needs to:

1. **Implement callback-based A11y adapter** (not polling)
2. **Store incoming actions** in a queue or process immediately
3. **Call `record_accessibility_action()`** for each action
4. **Store returned EventFilters** to merge with regular events
5. **Pass to event processing** in `process_window_events_recursive_v2()`

### Example: macOS Integration

**Step 1:** Replace polling with callback (in AccessKit adapter setup)

```rust
// OLD: Poll-based (BAD)
pub fn poll_accessibility_actions(&mut self) -> Vec<(DomId, NodeId, AccessibilityAction)> {
    let adapter = self.accessibility_adapter.as_ref()?;
    let mut actions = Vec::new();
    while let Some(action) = adapter.poll_action() {
        actions.push(action);
    }
    actions
}

// NEW: Callback-based (GOOD)
impl MacOSWindow {
    fn setup_accessibility_callbacks(&mut self) {
        // Register callback with AccessKit adapter
        let window_ptr = Arc::new(Mutex::new(self as *mut MacOSWindow));
        
        self.accessibility_adapter.set_action_handler(move |dom_id, node_id, action| {
            // Store action in pending queue or process immediately
            if let Ok(window) = window_ptr.lock() {
                unsafe {
                    (*(*window)).pending_a11y_actions.push((dom_id, node_id, action));
                    // Trigger event processing
                    (*(*window)).request_redraw();
                }
            }
        });
    }
}
```

**Step 2:** Process pending actions in event loop

```rust
// In macOS event handler (handle_event or similar)
pub fn process_events(&mut self) -> ProcessEventResult {
    // Process accessibility actions first
    #[cfg(feature = "accessibility")]
    {
        let pending_actions = std::mem::take(&mut self.pending_a11y_actions);
        for (dom_id, node_id, action) in pending_actions {
            let synthetic_events = self.record_accessibility_action(dom_id, node_id, action);
            self.pending_synthetic_events.extend(synthetic_events);
        }
    }
    
    // Then process regular events with synthetic events merged
    self.process_window_events_with_a11y(0)
}
```

**Step 3:** Extend event detection to include synthetic events

```rust
fn process_window_events_with_a11y(&mut self, depth: usize) -> ProcessEventResult {
    // Regular event detection
    let mut events = window_state::create_events_from_states_with_gestures(
        current_state, previous_state, ...
    );
    
    // Add synthetic a11y events
    #[cfg(feature = "accessibility")]
    {
        for event_filter in std::mem::take(&mut self.pending_synthetic_events) {
            // Convert EventFilter to appropriate event type and add to events
            events.add_synthetic_event(event_filter);
        }
    }
    
    // Continue with normal dispatch...
}
```

### Windows Integration

Windows uses UI Automation. Similar approach:

1. Implement `IUIAutomationElement` provider
2. Handle action requests in provider callbacks
3. Queue actions and call `record_accessibility_action()`
4. Merge synthetic events with WM_* message events

### Linux Integration

Linux uses AT-SPI2. Similar approach:

1. Implement AT-SPI2 DBus interface
2. Handle action requests from Orca/other screen readers
3. Queue actions and call `record_accessibility_action()`
4. Merge synthetic events with X11/Wayland events

## Current Status

### ✅ Complete

1. **Clean API design** - `record_accessibility_action()` trait method
2. **Manager access** - Direct access via `layout_window`
3. **EventFilter return** - Proper synthetic event generation
4. **Cross-platform trait** - Works on all platforms
5. **Clean compilation** - 0 errors

### 🔄 TODO (Platform-Specific)

1. **macOS callback implementation** - Replace `poll_accessibility_actions()` with callbacks
2. **Windows UI Automation** - Implement provider and callbacks
3. **Linux AT-SPI2** - Implement DBus interface and callbacks
4. **Synthetic event merging** - Extend event detection to include a11y events
5. **Testing** - Test with VoiceOver, NVDA, Orca

### Next Steps

**Immediate (Next Session):**

1. Implement callback-based macOS adapter:
   - Replace polling with AccessKit action handler
   - Add `pending_a11y_actions` queue to MacOSWindow
   - Process queue in event loop before regular events

2. Extend event detection to merge synthetic events:
   - Add `pending_synthetic_events` storage to platform windows
   - Modify event detection to include these events
   - Clear after processing

3. Test with VoiceOver on macOS

**Short Term:**

1. Implement Windows UI Automation callbacks
2. Implement Linux AT-SPI2 callbacks
3. Test cross-platform

**Long Term:**

1. Implement text editing (`edit_text_node`)
2. Cursor manager integration
3. Comprehensive tests

## Summary

The new architecture is **much cleaner**:

- ❌ **Old:** Complex `process_accessibility_events_v2()` with polling and unprocessed EventFilters
- ✅ **New:** Simple `record_accessibility_action()` that works just like gesture recording

**Key Insight:** Accessibility actions are just another input source (like mouse, keyboard, gestures). They should be recorded, processed through managers, and generate events that merge with regular events. No special code path needed.

**Files Modified:**
- ✅ `dll/src/desktop/shell2/common/event_v2.rs` - Added `record_accessibility_action()`
- ✅ `dll/src/desktop/shell2/macos/mod.rs` - Removed polling stub
- ✅ `dll/src/desktop/shell2/windows/mod.rs` - Removed polling stub
- ✅ `dll/src/desktop/shell2/linux/x11/mod.rs` - Removed polling stub
- ✅ `dll/src/desktop/shell2/linux/wayland/mod.rs` - Removed polling stub
- ✅ `layout/src/window.rs` - Already has manager access (no changes needed)

**Compilation:** ✅ Clean (0 errors)

### 1. Architecture Change: EventFilter Return Pattern ✅

Changed `process_accessibility_action()` in `layout/src/window.rs` to return `Vec<EventFilter>` instead of directly invoking callbacks.

**Rationale:** User corrected approach - accessibility events MUST go through the normal event system for proper:
- State management (prevent race conditions)
- Stop propagation (respect event bubbling)
- DOM regeneration (callbacks can change UI)
- Focus handling (FocusIn/FocusOut events)
- Recursion (max depth limiting)

**Key Logic:**
```rust
pub fn process_accessibility_action(
    &mut self,
    dom_id: DomId,
    node_id: NodeId,
    action: AccessibilityAction,
    now: Instant
) -> Vec<EventFilter> {
    let mut synthetic_events = Vec::new();
    
    match action {
        // Manager actions: Direct manipulation
        AccessibilityAction::Focus => {
            focus_manager.set_focused_node(...);
            // No EventFilter - state already changed
        }
        
        // Callback actions: Return EventFilters
        AccessibilityAction::Default | Increment | Decrement | Collapse | Expand => {
            let event_type = match action { /* map to On:: variant */ };
            
            if node_has_callback(event_type) {
                synthetic_events.push(event_type.into());
            } else {
                // Fallback: treat as regular click
                synthetic_events.push(EventFilter::Hover(HoverEventFilter::MouseUp));
            }
        }
    }
    
    synthetic_events
}
```

**Files Modified:**
- `layout/src/window.rs` (lines 2410-2658)
  - Function signature changed (void → Vec<EventFilter>)
  - Added synthetic_events collection
  - Implemented EventFilter generation for Default/Increment/Decrement/Collapse/Expand
  - Helper function `get_node_used_size_a11y()` to reduce indentation
  - Fixed all return statements
  - No-op version returns empty Vec

**Compilation:** ✅ Clean (0 errors, only unused field warnings in examples)

### 2. PlatformWindowV2 Trait Extension ✅

Added two new methods to the `PlatformWindowV2` trait:

```rust
/// Process accessibility actions from assistive technologies
#[cfg(feature = "accessibility")]
fn process_accessibility_events_v2(&mut self) -> ProcessEventResult;

/// Poll accessibility actions from platform adapter
#[cfg(feature = "accessibility")]
fn poll_accessibility_actions_v2(&mut self) 
    -> Vec<(DomId, NodeId, AccessibilityAction)>;
```

**Files Modified:**
- `dll/src/desktop/shell2/common/event_v2.rs` (lines 675-740)
  - Added `process_accessibility_events_v2()` with full workflow
  - Trait method declaration for `poll_accessibility_actions_v2()`
  - TODO comments for future improvements

**Design:**
- `process_accessibility_events_v2()` is cross-platform (provided implementation)
- `poll_accessibility_actions_v2()` is platform-specific (must be implemented per platform)
- Returns `ProcessEventResult` to integrate with existing event flow

### 3. Platform-Specific Implementations ✅

Implemented `poll_accessibility_actions_v2()` for all platforms:

#### macOS (`dll/src/desktop/shell2/macos/mod.rs` line ~1270)
```rust
#[cfg(feature = "accessibility")]
fn poll_accessibility_actions_v2(&mut self) -> Vec<...> {
    self.poll_accessibility_actions() // Delegates to existing function
}
```
**Status:** ✅ Working - Uses existing AccessKit macOS adapter polling

#### Windows (`dll/src/desktop/shell2/windows/mod.rs` line ~2610)
```rust
#[cfg(feature = "accessibility")]
fn poll_accessibility_actions_v2(&mut self) -> Vec<...> {
    // TODO: Implement Windows accessibility polling
    // Windows uses UI Automation
    Vec::new()
}
```
**Status:** 🔄 TODO - Needs UI Automation integration

#### Linux X11 (`dll/src/desktop/shell2/linux/x11/mod.rs` line ~1205)
```rust
#[cfg(feature = "accessibility")]
fn poll_accessibility_actions_v2(&mut self) -> Vec<...> {
    // TODO: Implement X11/AT-SPI accessibility polling
    Vec::new()
}
```
**Status:** 🔄 TODO - Needs AT-SPI2 integration

#### Linux Wayland (`dll/src/desktop/shell2/linux/wayland/mod.rs` line ~772)
```rust
#[cfg(feature = "accessibility")]
fn poll_accessibility_actions_v2(&mut self) -> Vec<...> {
    // TODO: Implement Wayland/AT-SPI accessibility polling
    Vec::new()
}
```
**Status:** 🔄 TODO - Needs AT-SPI2 integration

**Compilation:** ✅ All platforms compile cleanly

## What Still Needs To Be Done

### CRITICAL: EventFilter Processing 🔥

**Current State:**
The `process_accessibility_events_v2()` method collects EventFilters but **DOES NOT PROCESS THEM**.

```rust
// Current implementation in event_v2.rs line ~725
if all_synthetic_events.is_empty() {
    return ProcessEventResult::DoNothing;
}

// TODO: Convert EventFilters to callback dispatch and invoke
// For now, just trigger a re-render since actions may have changed state
ProcessEventResult::ShouldReRenderCurrentWindow
```

**Problem:** The synthetic events are collected but never dispatched to callbacks. This means:
- ❌ Accessibility actions don't trigger user callbacks
- ❌ EventFilters are generated but ignored
- ✅ Manager actions (focus, scroll) work directly
- ⚠️ Only triggers re-render, not actual event processing

**What Needs To Happen:**

The EventFilters need to be converted into the same format that `process_window_events_recursive_v2()` uses.

#### Option A: Extend Event Detection Logic

The main event loop uses `create_events_from_states_with_gestures()` to detect events by comparing current vs previous state. We could extend this to accept synthetic events:

```rust
// In event_v2.rs process_window_events_recursive_v2()
let mut events = window_state::create_events_from_states_with_gestures(
    current_state,
    previous_state,
    gesture_manager,
    fm, previous_focus,
    fdm, previous_file_drop,
    hm,
);

// Add synthetic accessibility events
if let Some(a11y_events) = self.get_pending_a11y_events() {
    events.extend(a11y_events);
}
```

**Pros:**
- Minimal changes to event processing
- EventFilters naturally merge with regular events
- Same dispatch logic handles both

**Cons:**
- Need to store pending accessibility events somewhere
- Timing - when to clear/process them?

#### Option B: Separate Dispatch Pass

Add a separate dispatch pass specifically for accessibility events:

```rust
fn process_accessibility_events_v2(&mut self) -> ProcessEventResult {
    // ... collect all_synthetic_events ...
    
    if all_synthetic_events.is_empty() {
        return ProcessEventResult::DoNothing;
    }

    // Convert EventFilters to dispatch events
    let mut dispatch_events = Vec::new();
    for event_filter in all_synthetic_events {
        // For each filter, create a dispatch event at the target node
        let dispatch = /* convert filter to dispatch event */;
        dispatch_events.push(dispatch);
    }

    // Invoke callbacks directly
    let callback_results = self.invoke_callbacks_for_events(&dispatch_events);
    
    // Process results (DOM regeneration, etc.)
    self.process_callback_results(callback_results)
}
```

**Pros:**
- Self-contained - doesn't mix with regular events
- Clear separation of concerns
- Can be called independently

**Cons:**
- Duplicates some logic from main event loop
- Risk of inconsistency between two code paths
- Harder to maintain two implementations

#### Option C: Inject Into Main Loop (RECOMMENDED)

Call `process_accessibility_events_v2()` from platform event handlers BEFORE calling `process_window_events_recursive_v2()`:

```rust
// In platform-specific event handling (e.g., macOS handleEvent)
pub fn handle_os_event(...) -> ProcessEventResult {
    // Update window state from OS event
    self.update_window_state_from_event(event);
    
    // Process accessibility events FIRST
    #[cfg(feature = "accessibility")]
    {
        let a11y_result = self.process_accessibility_events_v2();
        if a11y_result != ProcessEventResult::DoNothing {
            return a11y_result; // Early return if a11y action changed something
        }
    }
    
    // Then process normal events
    self.process_window_events_recursive_v2(0)
}
```

**Pros:**
- Uses existing event processing (no duplication)
- Accessibility events trigger before regular events (proper priority)
- Simple integration - just an extra call
- Still goes through normal event loop

**Cons:**
- Need to identify where in each platform's event handler to insert call
- Potentially processes events twice (a11y pass + regular pass)

**Recommended Implementation:**

1. Store synthetic events in `LayoutWindow` or `PlatformWindow`:
   ```rust
   struct LayoutWindow {
       // ... existing fields ...
       pending_a11y_events: Vec<(DomId, NodeId, EventFilter)>,
   }
   ```

2. Modify `process_accessibility_events_v2()` to store events:
   ```rust
   fn process_accessibility_events_v2(&mut self) -> ProcessEventResult {
       // ... collect all_synthetic_events ...
       
       if let Some(layout_window) = self.get_layout_window_mut() {
           layout_window.pending_a11y_events.extend(all_synthetic_events);
       }
       
       // Return flag to process events on next frame
       ProcessEventResult::ShouldReRenderCurrentWindow
   }
   ```

3. Modify `create_events_from_states_with_gestures()` to consume pending events:
   ```rust
   pub fn create_events_from_states_with_gestures(
       current: &FullWindowState,
       previous: &FullWindowState,
       // ... other params ...
       pending_a11y: &mut Vec<(DomId, NodeId, EventFilter)>,
   ) -> BTreeMap<DomId, Vec<CallbackInvocation>> {
       let mut events = BTreeMap::new();
       
       // Process regular events (mouse, keyboard, focus, etc.)
       // ...
       
       // Inject accessibility events
       for (dom_id, node_id, event_filter) in pending_a11y.drain(..) {
           // Convert EventFilter to CallbackInvocation
           // Add to events map
       }
       
       events
   }
   ```

4. Events are automatically dispatched through normal flow

### Other TODOs

#### 1. Callback-Based macOS Adapter (MEDIUM Priority)

**Current:** Polling with `poll_accessibility_actions()`  
**Goal:** Callback-based approach using AccessKit action handler

**Why:** Polling has latency - screen reader action → next poll cycle → process. Callbacks are immediate.

**How:**
- AccessKit macOS adapter supports action callbacks
- Register callback when creating adapter
- Callback pushes action to queue
- Process queue in event loop (or immediately)

**Files:** `dll/src/desktop/shell2/macos/mod.rs`

#### 2. Windows UI Automation Integration (HIGH Priority)

**Status:** Not implemented (returns empty Vec)

**What's Needed:**
- Use `windows-rs` crate for UI Automation APIs
- Implement `IAccessible` interface or UIA provider
- Poll for action requests
- Convert to AccessibilityAction enum

**Files:** `dll/src/desktop/shell2/windows/mod.rs`

#### 3. Linux AT-SPI2 Integration (HIGH Priority)

**Status:** Not implemented (returns empty Vec for both X11 and Wayland)

**What's Needed:**
- Use `atspi` crate or DBus directly
- Implement AT-SPI2 provider interface
- Handle action requests from Orca/other screen readers
- Convert to AccessibilityAction enum

**Files:**
- `dll/src/desktop/shell2/linux/x11/mod.rs`
- `dll/src/desktop/shell2/linux/wayland/mod.rs`

#### 4. edit_text_node() Implementation (MEDIUM Priority)

**Current:** Stub function that does nothing

**Goal:** Enable text editing via accessibility APIs

**Workflow:**
1. Check if node has `contenteditable` attribute
2. Find text in node or immediate children
3. Look up layouted text from `text_cache`
4. Get cursor/selection from managers
5. Apply edit using `text3/edit::edit_text()`
6. Update DOM and trigger re-layout

**Files:** `layout/src/window.rs` (accessibility feature)

#### 5. Cursor Manager Integration (LOW Priority)

**Goal:** Properly initialize/clear cursor on focus changes

**Logic:**
- When focusing contenteditable node: initialize cursor at text end
- When focusing non-editable node: clear cursor
- Store in `CursorManager` (if it exists, or create it)

**Files:**
- `layout/src/managers/cursor.rs` (if exists)
- `layout/src/window.rs` (Focus action handler)

#### 6. Tests (LOW Priority)

**Goal:** Validate manager interactions

**Approach:**
- Create fake `LayoutWindow` for testing
- Simulate accessibility actions
- Verify manager state changes
- Check EventFilter generation

**Files:** New file `layout/src/window_test.rs` or similar

## Summary

### ✅ Complete
1. EventFilter return architecture
2. PlatformWindowV2 trait extension
3. All platform stub implementations
4. Clean compilation

### 🔄 In Progress
1. EventFilter processing/dispatch (CRITICAL)

### ⏳ TODO
1. Callback-based macOS adapter (MEDIUM)
2. Windows UI Automation (HIGH)
3. Linux AT-SPI2 (HIGH)
4. edit_text_node() (MEDIUM)
5. Cursor manager integration (LOW)
6. Tests (LOW)

### Priority Order
1. **CRITICAL:** Implement EventFilter dispatch in `process_accessibility_events_v2()`
2. **HIGH:** Windows + Linux platform adapters (enable cross-platform accessibility)
3. **MEDIUM:** Text editing + callback-based macOS
4. **LOW:** Cursor manager + tests

## Next Steps

**Immediate (Next Session):**

1. Implement Option C (Inject Into Main Loop):
   - Add `pending_a11y_events: Vec<(DomId, NodeId, EventFilter)>` to `LayoutWindow`
   - Modify `process_accessibility_events_v2()` to store events instead of process
   - Extend `create_events_from_states_with_gestures()` to accept pending events
   - Convert EventFilters to `CallbackInvocation` format
   - Test with simple example (button with On::Click callback)

2. Call integration method from macOS event handler:
   - Locate macOS event handling code
   - Add call to `process_accessibility_events_v2()` before main event processing
   - Test with VoiceOver

**Short Term (Next Few Sessions):**

1. Implement Windows UI Automation support
2. Implement Linux AT-SPI2 support
3. Replace macOS polling with callbacks

**Long Term:**

1. Implement text editing
2. Add comprehensive tests
3. Document accessibility features for users
