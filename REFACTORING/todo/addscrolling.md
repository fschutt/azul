# Scroll/IFrame/GPU Management Architecture

**Document Version:** 2.0 (Updated: October 18, 2025)  
**Previous Version:** 1.0 (Original implementation plan - DEPRECATED)  
**Current Status:** Phase 3 Complete (90/90 tests passing), Phases 4-6 pending refactoring

---

## Executive Summary

This document describes the **current implementation** (Phase 3) and the **planned refactoring** (Phases 4-6) for scroll, IFrame, and GPU state management in the Azul layout engine.

**Key Insight:** Phase 3 successfully implemented scrolling functionality but mixed responsibilities across managers. Phases 4-6 will refactor into a clean 3-manager architecture with event source classification and centralized GPU management.

---

## Table of Contents

1. [Phase 3: Current Implementation (COMPLETE)](#phase-3-current-implementation)
2. [Architectural Issues](#architectural-issues)
3. [Proposed Refactoring (Phases 4-6)](#proposed-refactoring-phases-4-6)
4. [New Architecture: Three Managers](#new-architecture-three-managers)
5. [Animation Strategy: Internal Manager Ticks](#animation-strategy-internal-manager-ticks)
6. [Event System Enhancement](#event-system-enhancement)
7. [Migration Plan](#migration-plan)
8. [Testing Strategy](#testing-strategy)
9. [Timeline & Estimates](#timeline-estimates)

---

## Phase 3: Current Implementation (COMPLETE)

### What Was Built

Phase 3 successfully implemented a working scroll system with IFrame integration and GPU scrollbar opacity. **All 90 tests pass.**

---

## Phase 3.5: Event System Refactoring (REQUIRED BEFORE PHASE 4)

**Status:** NOT YET STARTED  
**Priority:** CRITICAL - Must be completed before manager refactoring  
**Estimated Time:** 6-8 hours

### Why This Is Critical

The current event system has fundamental design issues that will block Phases 4-6:

1. **No Event Source Classification**: Can't distinguish User vs. Programmatic vs. Synthetic events
2. **No Event Propagation Control**: Can't implement proper bubbling/capturing phases
3. **Missing Modern Event Types**: No support for lifecycle events (OnMount, OnUnmount), clipboard, media, forms
4. **Inconsistent Event Filtering**: `On` enum maps to `EventFilter`, but logic is scattered
5. **No SyntheticEvent Pattern**: Events are raw OS events, not normalized cross-platform wrappers

**Impact on Phases 4-6:**
- ❌ Phase 5 (Scrollbar Hit-Testing) needs `EventSource::Synthetic` for scrollbar drag
- ❌ Phase 4 (Manager Refactoring) needs event source tracking in ScrollManager
- ❌ Phase 6 (WebRender) needs lifecycle events for IFrame mounting
- ❌ All phases need proper event propagation to prevent feedback loops

---

### Current Event System Analysis

#### Current Structure (`core/src/dom.rs`)

```rust
// User-facing API - simplified enum
pub enum On {
    MouseOver, MouseDown, LeftMouseDown, RightMouseDown, 
    MouseUp, LeftMouseUp, MouseEnter, MouseLeave,
    Scroll, TextInput, VirtualKeyDown, VirtualKeyUp,
    HoveredFile, DroppedFile, HoveredFileCancelled,
    FocusReceived, FocusLost,
}

// Internal filtering - complex enum
pub enum EventFilter {
    Hover(HoverEventFilter),      // Element is hovered
    Not(NotEventFilter),          // Inverse filtering
    Focus(FocusEventFilter),      // Element is focused
    Window(WindowEventFilter),    // Window-global events
    Component(ComponentEventFilter),   // Lifecycle events (STUB!)
    Application(ApplicationEventFilter), // App-level events (STUB!)
}

// Conversion: On → EventFilter (hard-coded logic)
impl From<On> for EventFilter {
    fn from(input: On) -> EventFilter {
        match input {
            On::MouseOver => EventFilter::Hover(HoverEventFilter::MouseOver),
            On::TextInput => EventFilter::Focus(FocusEventFilter::TextInput),
            On::VirtualKeyDown => EventFilter::Window(WindowEventFilter::VirtualKeyDown),
            // ... 20+ more variants
        }
    }
}
```

**Problems:**
- ❌ `ComponentEventFilter` and `ApplicationEventFilter` are defined but **NEVER USED**
- ❌ No way to specify event propagation phase (capture vs. bubble)
- ❌ `From<On>` hard-codes filter logic (should be configurable)
- ❌ Events have no source tracking (User/Programmatic/Synthetic)
- ❌ No lifecycle events (OnMount, OnUnmount, etc.)

---

### Proposed: React-like SyntheticEvent System

#### Design Goals

1. **Unified Event Object**: All events go through `SyntheticEvent` wrapper
2. **Event Source Classification**: User/Programmatic/Synthetic tracking
3. **Propagation Control**: Capture/Bubble phases with `stopPropagation()`
4. **Lifecycle Events**: OnMount, OnUnmount, OnResize for components
5. **Sane Defaults**: `On` enum provides sensible defaults, but customizable
6. **Cross-Platform**: Normalize OS-specific events into consistent interface

---

#### New Core Types

**1. EventSource (from original plan):**

```rust
/// Tracks the origin of an event for proper handling
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum EventSource {
    /// Direct user input (mouse, keyboard, touch)
    User,
    /// API call (programmatic scroll, focus change)
    Programmatic,
    /// Generated from UI interaction (scrollbar drag, synthetic events)
    Synthetic,
    /// Generated from lifecycle hooks (mount, unmount)
    Lifecycle,
}
```

**2. EventPhase (NEW):**

```rust
/// Event propagation phase (à la DOM Level 2 Events)
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub enum EventPhase {
    /// Event travels from root down to target (rarely used)
    Capture,
    /// Event is at the target element
    Target,
    /// Event bubbles from target back up to root (most common)
    Bubble,
}
```

**3. SyntheticEvent (NEW):**

```rust
/// Unified event wrapper (similar to React's SyntheticEvent)
#[derive(Debug, Clone)]
pub struct SyntheticEvent {
    /// The type of event (mouse, keyboard, etc.)
    pub event_type: EventType,
    
    /// Where the event came from
    pub source: EventSource,
    
    /// Current propagation phase
    pub phase: EventPhase,
    
    /// Target node that the event was dispatched to
    pub target: DomNodeId,
    
    /// Current node in the propagation path
    pub current_target: DomNodeId,
    
    /// Timestamp when event was created
    pub timestamp: Instant,
    
    /// Type-specific event data
    pub data: EventData,
    
    /// Propagation control flags
    pub stopped: bool,
    pub stopped_immediate: bool,
    pub prevented_default: bool,
}

impl SyntheticEvent {
    pub fn stop_propagation(&mut self) {
        self.stopped = true;
    }
    
    pub fn stop_immediate_propagation(&mut self) {
        self.stopped_immediate = true;
    }
    
    pub fn prevent_default(&mut self) {
        self.prevented_default = true;
    }
}
```

**4. EventType (Replaces/Extends `On`):**

```rust
/// High-level event categories
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C, u8)]
pub enum EventType {
    // Mouse Events
    MouseOver, MouseEnter, MouseLeave,
    MouseDown, MouseUp, Click, DoubleClick, ContextMenu,
    
    // Keyboard Events
    KeyDown, KeyUp, KeyPress,
    
    // Focus Events
    Focus, Blur, FocusIn, FocusOut,
    
    // Input Events
    Input, Change, Submit, Reset, Invalid,
    
    // Scroll Events
    Scroll, ScrollStart, ScrollEnd,
    
    // Drag Events
    DragStart, Drag, DragEnd, DragEnter, DragOver, DragLeave, Drop,
    
    // Touch Events
    TouchStart, TouchMove, TouchEnd, TouchCancel,
    
    // Clipboard Events (NEW!)
    Copy, Cut, Paste,
    
    // Media Events (NEW!)
    Play, Pause, Ended, TimeUpdate, VolumeChange, MediaError,
    
    // Lifecycle Events (NEW!)
    Mount, Unmount, Update, Resize,
    
    // Window Events
    WindowResize, WindowMove, WindowClose,
    WindowFocusIn, WindowFocusOut, ThemeChange,
    
    // File Events
    FileHover, FileDrop, FileHoverCancel,
}
```

**5. EventData (Union of event-specific data):**

```rust
/// Type-specific event data
#[derive(Debug, Clone)]
pub enum EventData {
    Mouse(MouseEventData),
    Keyboard(KeyboardEventData),
    Scroll(ScrollEventData),
    Touch(TouchEventData),
    Clipboard(ClipboardEventData),
    Lifecycle(LifecycleEventData),
    Window(WindowEventData),
    None, // For simple events
}

#[derive(Debug, Clone)]
pub struct MouseEventData {
    pub position: LogicalPosition,
    pub button: MouseButton,
    pub buttons: u8, // Bitmask of currently pressed buttons
    pub modifiers: KeyModifiers,
}

#[derive(Debug, Clone)]
pub struct ScrollEventData {
    pub delta: LogicalPosition,
    pub delta_mode: ScrollDeltaMode, // Pixel, Line, Page
}

#[derive(Debug, Clone)]
pub struct LifecycleEventData {
    pub reason: LifecycleReason,
    pub previous_bounds: Option<LogicalRect>,
    pub current_bounds: LogicalRect,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum LifecycleReason {
    InitialMount,   // First appearance in DOM
    Remount,        // Removed and re-added
    Resize,         // Layout bounds changed
    Update,         // Props or state changed
}
```

---

#### Updated EventFilter with Propagation

```rust
/// Enhanced EventFilter with propagation control
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct EventFilter {
    /// What type of event to listen for
    pub event_type: EventType,
    
    /// When to fire the callback
    pub target: EventTarget,
    
    /// Which propagation phase to listen on
    pub phase: EventPhase,
    
    /// Optional: only fire if these conditions are met
    pub conditions: Vec<EventCondition>,
}

/// Where the event should be attached
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
#[repr(C, u8)]
pub enum EventTarget {
    /// Fire when this element is hovered
    Hover,
    /// Fire when this element is focused
    Focus,
    /// Fire on window (regardless of target)
    Window,
    /// Fire when this element is NOT hovered/focused (inverse)
    Not(Box<EventTarget>),
}

/// Additional filtering conditions
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[repr(C, u8)]
pub enum EventCondition {
    /// Only fire if source matches
    Source(EventSource),
    /// Only fire if specific mouse button
    Button(MouseButton),
    /// Only fire if modifiers are pressed
    Modifiers(KeyModifiers),
    /// Only fire if moving in specific direction
    ScrollDirection(ScrollDirection),
}
```

---

#### Simplified `On` Enum with Sane Defaults

```rust
/// User-facing API - provides sane defaults
pub enum On {
    // Mouse Events (default: Hover + Bubble)
    Click,          // → EventFilter::new(Click, Hover, Bubble)
    DoubleClick,    // → EventFilter::new(DoubleClick, Hover, Bubble)
    ContextMenu,    // → EventFilter::new(ContextMenu, Hover, Bubble)
    MouseDown,      // → EventFilter::new(MouseDown, Hover, Bubble)
    MouseUp,        // → EventFilter::new(MouseUp, Hover, Bubble)
    MouseEnter,     // → EventFilter::new(MouseEnter, Hover, Bubble)
    MouseLeave,     // → EventFilter::new(MouseLeave, Hover, Bubble)
    
    // Keyboard Events (default: Focus + Bubble)
    KeyDown,        // → EventFilter::new(KeyDown, Focus, Bubble)
    KeyUp,          // → EventFilter::new(KeyUp, Focus, Bubble)
    TextInput,      // → EventFilter::new(Input, Focus, Bubble)
    
    // Focus Events (default: Focus + Bubble)
    Focus,          // → EventFilter::new(Focus, Focus, Bubble)
    Blur,           // → EventFilter::new(Blur, Focus, Bubble)
    
    // Scroll Events (default: Hover + Bubble)
    Scroll,         // → EventFilter::new(Scroll, Hover, Bubble)
    
    // Lifecycle Events (NEW! default: Hover + Target)
    Mount,          // → EventFilter::new(Mount, Hover, Target)
    Unmount,        // → EventFilter::new(Unmount, Hover, Target)
    Resize,         // → EventFilter::new(Resize, Hover, Target)
    
    // Clipboard Events (NEW! default: Focus + Bubble)
    Copy,           // → EventFilter::new(Copy, Focus, Bubble)
    Cut,            // → EventFilter::new(Cut, Focus, Bubble)
    Paste,          // → EventFilter::new(Paste, Focus, Bubble)
    
    // Form Events (NEW! default: Focus + Bubble)
    Change,         // → EventFilter::new(Change, Focus, Bubble)
    Submit,         // → EventFilter::new(Submit, Focus, Bubble)
    Invalid,        // → EventFilter::new(Invalid, Focus, Bubble)
    
    // Window Events (default: Window + Bubble)
    WindowResize,   // → EventFilter::new(WindowResize, Window, Bubble)
    WindowClose,    // → EventFilter::new(WindowClose, Window, Bubble)
}

impl From<On> for EventFilter {
    fn from(on: On) -> EventFilter {
        use EventType::*;
        use EventTarget::*;
        use EventPhase::*;
        
        match on {
            On::Click => EventFilter::new(Click, Hover, Bubble),
            On::MouseEnter => EventFilter::new(MouseEnter, Hover, Bubble),
            On::KeyDown => EventFilter::new(KeyDown, Focus, Bubble),
            On::Mount => EventFilter::new(Mount, Hover, Target),
            On::Scroll => EventFilter::new(Scroll, Hover, Bubble),
            // ... etc.
        }
    }
}
```

**Custom Filters (Advanced Usage):**

```rust
// Example: Capture left-click on window (before bubbling)
let filter = EventFilter {
    event_type: EventType::Click,
    target: EventTarget::Window,
    phase: EventPhase::Capture,
    conditions: vec![
        EventCondition::Button(MouseButton::Left),
        EventCondition::Source(EventSource::User),
    ],
};

// Example: Only fire on synthetic scroll events
let filter = EventFilter {
    event_type: EventType::Scroll,
    target: EventTarget::Hover,
    phase: EventPhase::Bubble,
    conditions: vec![
        EventCondition::Source(EventSource::Synthetic),
    ],
};
```

---

### Migration Strategy

#### Step 1: Add New Types (No Breaking Changes)

1. Add `EventSource`, `EventPhase`, `EventType` enums
2. Add `SyntheticEvent` struct
3. Add `EventData` variants
4. Add new lifecycle events to `ComponentEventFilter`
5. **Tests:** Unit tests for new types

**Verification:** All existing code still compiles

---

#### Step 2: Implement Event Wrapper Layer

1. Create `event_wrapper.rs` module
2. Implement `SyntheticEvent::from_raw_event()`
3. Add event normalization (OS-specific → cross-platform)
4. **Tests:** Event conversion tests

```rust
impl SyntheticEvent {
    /// Convert raw OS event to SyntheticEvent
    pub fn from_raw_event(
        raw: &RawEvent,
        target: DomNodeId,
        source: EventSource,
    ) -> Self {
        let event_type = match raw {
            RawEvent::MouseDown { button, .. } => EventType::MouseDown,
            RawEvent::KeyDown { key, .. } => EventType::KeyDown,
            // ... normalize all event types
        };
        
        let data = match raw {
            RawEvent::MouseDown { button, position, modifiers } => {
                EventData::Mouse(MouseEventData {
                    position: *position,
                    button: *button,
                    buttons: 0, // TODO: track pressed buttons
                    modifiers: *modifiers,
                })
            }
            // ... convert all event data
        };
        
        SyntheticEvent {
            event_type,
            source,
            phase: EventPhase::Target,
            target,
            current_target: target,
            timestamp: Instant::now(),
            data,
            stopped: false,
            stopped_immediate: false,
            prevented_default: false,
        }
    }
}
```

**Verification:** Raw events convert to SyntheticEvents correctly

---

#### Step 3: Implement Event Propagation

1. Add `propagate_event()` function in `events.rs`
2. Implement capture phase (root → target)
3. Implement bubble phase (target → root)
4. Handle `stopPropagation()` and `stopImmediatePropagation()`
5. **Tests:** Propagation tests

```rust
/// Propagate event through DOM tree
pub fn propagate_event(
    event: &mut SyntheticEvent,
    dom: &StyledDom,
    callbacks: &BTreeMap<NodeId, Vec<(EventFilter, Callback)>>,
) -> Vec<(NodeId, Callback)> {
    let mut to_invoke = Vec::new();
    
    // Get path from root to target
    let path = get_dom_path(dom, event.target.node);
    
    // Phase 1: Capture (root → target)
    event.phase = EventPhase::Capture;
    for &node_id in &path {
        if event.stopped_immediate { break; }
        event.current_target = DomNodeId { dom: event.target.dom, node: node_id };
        
        if let Some(node_callbacks) = callbacks.get(&node_id) {
            for (filter, callback) in node_callbacks {
                if filter.matches(event, EventPhase::Capture) {
                    to_invoke.push((node_id, callback.clone()));
                    if event.stopped_immediate { break; }
                }
            }
        }
        
        if event.stopped { break; }
    }
    
    // Phase 2: Target
    if !event.stopped {
        event.phase = EventPhase::Target;
        event.current_target = event.target;
        // ... invoke target callbacks
    }
    
    // Phase 3: Bubble (target → root)
    if !event.stopped {
        event.phase = EventPhase::Bubble;
        for &node_id in path.iter().rev() {
            if event.stopped_immediate { break; }
            event.current_target = DomNodeId { dom: event.target.dom, node: node_id };
            
            if let Some(node_callbacks) = callbacks.get(&node_id) {
                for (filter, callback) in node_callbacks {
                    if filter.matches(event, EventPhase::Bubble) {
                        to_invoke.push((node_id, callback.clone()));
                        if event.stopped_immediate { break; }
                    }
                }
            }
            
            if event.stopped { break; }
        }
    }
    
    to_invoke
}
```

**Verification:** Events propagate correctly through DOM tree

---

#### Step 4: Add Lifecycle Event Detection

1. Track node additions/removals in DOM diff
2. Generate `Mount` events for new nodes
3. Generate `Unmount` events for removed nodes
4. Generate `Resize` events for layout changes
5. **Tests:** Lifecycle event tests

```rust
/// Detect lifecycle events by comparing old and new DOM
pub fn detect_lifecycle_events(
    old_dom: &StyledDom,
    new_dom: &StyledDom,
    old_layout: &BTreeMap<NodeId, LogicalRect>,
    new_layout: &BTreeMap<NodeId, LogicalRect>,
) -> Vec<SyntheticEvent> {
    let mut events = Vec::new();
    
    // Find newly mounted nodes
    let old_nodes: BTreeSet<_> = old_dom.node_hierarchy.as_container()
        .iter().map(|(id, _)| id).collect();
    let new_nodes: BTreeSet<_> = new_dom.node_hierarchy.as_container()
        .iter().map(|(id, _)| id).collect();
    
    for &node_id in new_nodes.difference(&old_nodes) {
        events.push(SyntheticEvent {
            event_type: EventType::Mount,
            source: EventSource::Lifecycle,
            phase: EventPhase::Target,
            target: DomNodeId { dom: new_dom.dom_id, node: node_id },
            current_target: DomNodeId { dom: new_dom.dom_id, node: node_id },
            timestamp: Instant::now(),
            data: EventData::Lifecycle(LifecycleEventData {
                reason: LifecycleReason::InitialMount,
                previous_bounds: None,
                current_bounds: new_layout[&node_id],
            }),
            stopped: false,
            stopped_immediate: false,
            prevented_default: false,
        });
    }
    
    // Find unmounted nodes
    for &node_id in old_nodes.difference(&new_nodes) {
        events.push(SyntheticEvent {
            event_type: EventType::Unmount,
            source: EventSource::Lifecycle,
            // ... similar construction
        });
    }
    
    // Find resized nodes
    for (&node_id, &old_bounds) in old_layout {
        if let Some(&new_bounds) = new_layout.get(&node_id) {
            if old_bounds != new_bounds {
                events.push(SyntheticEvent {
                    event_type: EventType::Resize,
                    source: EventSource::Lifecycle,
                    data: EventData::Lifecycle(LifecycleEventData {
                        reason: LifecycleReason::Resize,
                        previous_bounds: Some(old_bounds),
                        current_bounds: new_bounds,
                    }),
                    // ...
                });
            }
        }
    }
    
    events
}
```

**Verification:** Lifecycle events fire on mount/unmount/resize

---

#### Step 5: Update Callback Signatures

1. Change callbacks to accept `&SyntheticEvent` instead of raw data
2. Update `CallbackInfo` to provide event access
3. Maintain backward compatibility with deprecated functions
4. **Tests:** Callback integration tests

```rust
// Old signature (deprecated)
pub type CallbackType = extern "C" fn(&mut RefAny, &mut CallbackInfo) -> Update;

// New signature (preferred)
pub type EventCallbackType = extern "C" fn(
    &mut RefAny, 
    &mut CallbackInfo,
    &SyntheticEvent,  // NEW!
) -> Update;
```

**Verification:** Callbacks receive SyntheticEvents correctly

---

### Test Plan for Phase 3.5

#### Unit Tests (`core/tests/event_tests.rs`)

1. **Event Type Tests**:
   - ✅ All EventType variants defined
   - ✅ EventType → EventFilter conversion correct
   - ✅ EventSource tracking works

2. **Event Propagation Tests**:
   - ✅ Capture phase fires root → target
   - ✅ Bubble phase fires target → root
   - ✅ stopPropagation() halts propagation
   - ✅ stopImmediatePropagation() halts immediately
   - ✅ preventDefault() works

3. **Event Filter Tests**:
   - ✅ EventCondition filtering works
   - ✅ Phase-specific filtering works
   - ✅ `On` enum provides correct defaults

4. **Lifecycle Event Tests**:
   - ✅ Mount event fires on node addition
   - ✅ Unmount event fires on node removal
   - ✅ Resize event fires on layout change
   - ✅ Lifecycle events have correct data

#### Integration Tests (`layout/tests/event_integration_tests.rs`)

1. **Event Flow Tests**:
   - ✅ User mouse click propagates correctly
   - ✅ Programmatic scroll doesn't trigger hover
   - ✅ Synthetic scrollbar drag doesn't recurse

2. **Lifecycle Integration**:
   - ✅ OnMount callback fires once per node
   - ✅ OnUnmount cleanup happens correctly
   - ✅ OnResize updates layout-dependent state

3. **Backward Compatibility**:
   - ✅ Old callback signatures still work
   - ✅ Old event filtering still works
   - ✅ Gradual migration path works

---

### Timeline for Phase 3.5

**Total Estimated Time: 6-8 hours**

- **Step 1:** Add new types (1 hour)
- **Step 2:** Event wrapper layer (1-2 hours)
- **Step 3:** Event propagation (2-3 hours)
- **Step 4:** Lifecycle events (1-2 hours)
- **Step 5:** Update callbacks (1 hour)

**Milestones:**
- After Step 2: Can convert raw events to SyntheticEvents
- After Step 3: Events propagate through DOM tree
- After Step 4: Lifecycle events work
- After Step 5: All tests pass, ready for Phase 4

---

### Benefits for Phases 4-6

**Phase 4 (Manager Refactoring):**
- ✅ `EventSource` tracking in ScrollManager
- ✅ Different animations per source (User: smooth, Synthetic: instant)
- ✅ No feedback loops from synthetic events

**Phase 5 (Scrollbar Transforms):**
- ✅ `EventSource::Synthetic` for scrollbar drag
- ✅ Proper event filtering for scrollbar hits
- ✅ stopPropagation() prevents double-handling

**Phase 6 (WebRender Integration):**
- ✅ `OnMount` lifecycle for IFrame initialization
- ✅ `OnUnmount` cleanup for IFrame disposal
- ✅ `OnResize` for IFrame bounds updates

---

## Phase 3: Current Implementation (COMPLETE - Continued)

#### 1. ScrollManager (`layout/src/scroll.rs` - 817 lines)

**Current Responsibilities (TOO MANY!):**
- ✅ Scroll state tracking (offsets, velocities)
- ✅ Scroll animation management
- ✅ IFrame re-invocation logic ⚠️ **SHOULD BE SEPARATE**
- ✅ Scrollbar opacity calculation ⚠️ **SHOULD BE SEPARATE**
- ✅ Frame lifecycle management

**Current Structure:**
```rust
pub struct ScrollManager {
    states: BTreeMap<(DomId, NodeId), ScrollState>,
    had_scroll_activity: bool,
    had_programmatic_scroll: bool,
    had_new_doms: bool,
}

struct ScrollState {
    // Pure scroll logic:
    current_offset: LogicalPosition,
    scroll_size: LogicalSize,
    animation: Option<ScrollAnimation>,
    
    // IFrame logic (❌ MIXED RESPONSIBILITY):
    iframe_scroll_size: Option<LogicalSize>,
    iframe_was_invoked: bool,
    invoked_for_current_expansion: bool,
    invoked_for_current_edge: bool,
    last_edge_triggered: EdgeFlags,
}
```

**Key Methods:**
- `process_scroll_event()` - Handle user/programmatic scroll
- `tick()` - Animation updates, returns IFrames to re-invoke
- `get()` - Get current scroll info (returns `content_rect`)
- `get_scrollbar_opacity()` - Calculate opacity based on activity ⚠️
- `check_iframe_reinvoke_condition()` - IFrame re-invocation detection ⚠️

**What Works Well:**
- ✅ Scroll state persistence across frames
- ✅ Smooth scroll animations
- ✅ IFrame re-invocation on edge scroll
- ✅ Prevention of duplicate InitialRender callbacks
- ✅ Frame lifecycle (begin_frame/end_frame)

**Architectural Problems:**
- ❌ **Violates Single Responsibility Principle** (scroll + IFrame + opacity)
- ❌ **No event source classification** (can't distinguish user vs. programmatic)
- ❌ **GPU logic mixed with scroll logic**
- ❌ **816 lines is too large for a single manager**

---

#### 2. GPU Scrollbar Opacity System (`core/src/gpu.rs`)

**Current Implementation:**
```rust
pub enum GpuScrollbarOpacityEvent {
    Initial,     // Opacity 0.0 → 1.0 over 200ms
    FadeIn,      // Opacity 0.0 → 1.0 over 200ms
    Visible,     // Opacity 1.0 (stable)
    FadeOut,     // Opacity 1.0 → 0.0 over 200ms
    Hidden,      // Opacity 0.0 (stable)
}

pub struct GpuValueCache {
    pub scrollbar_opacity_keys: Vec<DomId>,
    pub scrollbar_opacity_values: Vec<f32>,
    // ... transform keys for Phase 5+
}
```

**Integration Points:**
- `ScrollManager::get_scrollbar_opacity()` - Calculate current opacity
- `window.rs::synchronize_scrollbar_opacity()` - Update GPU cache ⚠️ **SCATTERED**
- WebRender receives opacity updates via property bindings

**What Works Well:**
- ✅ Smooth fade-in/fade-out transitions
- ✅ Configurable fade delay (500ms) and duration (200ms)
- ✅ Separate from transform keys

**Architectural Problems:**
- ❌ **Opacity calculation in ScrollManager** (wrong place)
- ❌ **GPU synchronization in window.rs** (should be centralized)
- ❌ **No unified GPU key lifecycle management**

---

#### 3. IFrame Integration (`layout/src/window.rs`)

**Current Implementation:**
```rust
impl LayoutWindow {
    pub fn invoke_iframe_callback(...) -> Result<Dom> {
        // 1. Get IFrames to update from ScrollManager.tick()
        // 2. Invoke callback with reason (InitialRender, EdgeScrolled, etc.)
        // 3. Mark as invoked in ScrollManager
        // 4. Return new Dom or empty Dom::div()
    }
}
```

**IFrame Re-Invocation Reasons:**
- `InitialRender` - First render of IFrame
- `ContentSizeChanged` - IFrame content grew/shrank
- `EdgeScrolled(EdgeFlags)` - Scrolled to edge (infinite scroll)

**What Works Well:**
- ✅ Prevents duplicate InitialRender callbacks
- ✅ Detects edge scrolling for infinite scroll
- ✅ Handles content expansion correctly
- ✅ Returns empty `Dom::div()` fallback on None return

**Architectural Problems:**
- ❌ **IFrame logic embedded in ScrollManager** (should be separate)
- ❌ **No PipelineId management for WebRender**
- ❌ **No nested DisplayList rendering**

---

### Test Coverage (90/90 Passing)

**Test Files:**
- `layout/tests/src/scroll_tests.rs` - Basic scrolling
- `layout/tests/src/iframe_tests.rs` - IFrame re-invocation
- `layout/tests/src/integration_tests.rs` - End-to-end scenarios

**Key Test Scenarios:**
- ✅ Scroll position persistence
- ✅ IFrame initial render (only once)
- ✅ IFrame edge scrolling detection
- ✅ IFrame content expansion tracking
- ✅ Scrollbar opacity state machine
- ✅ Multiple simultaneous scrollable elements
- ✅ Nested IFrames
- ✅ Programmatic vs. user scroll events

**Critical Bug Fixes in Phase 3:**
1. **ScrollManager.get() returned wrong rect**: Fixed to return `content_rect` instead of container
2. **Duplicate InitialRender**: Added `iframe_was_invoked` flag
3. **None callback return**: Added `Dom::div()` fallback

---

## Architectural Issues

### Issue 1: Mixed Responsibilities (Violation of SRP)

**Current State:**
```
ScrollManager (817 lines)
├── Scroll state management          ✅ CORRECT
├── Scroll animations                ✅ CORRECT
├── IFrame re-invocation logic       ❌ WRONG - should be IFrameManager
├── Scrollbar opacity calculation    ❌ WRONG - should be GpuStateManager
└── GPU opacity events               ❌ WRONG - should be GpuStateManager
```

**Problem:**
- Hard to test individual features in isolation
- Changes to one feature risk breaking others
- 817 lines is too large for maintainability
- Violates Single Responsibility Principle

**Impact:**
- Future features (transforms, filters) will make it worse
- Hard to extend with new GPU properties
- Difficult to debug when issues span multiple concerns

---

### Issue 2: No Event Source Classification

**Current State:**
```rust
pub struct ScrollEvent {
    pub dom_id: DomId,
    pub node_id: NodeId,
    pub delta: LogicalPosition,
    // ❌ NO SOURCE TRACKING!
}
```

**Problem:**
- Can't distinguish user scroll (wheel/touch) from programmatic (API)
- Can't identify synthetic events (from scrollbar interaction)
- Impossible to prevent feedback loops
- Can't implement proper scrollbar dragging

**Impact:**
- Scrollbar drag will trigger scroll events, which would recursively update scrollbar
- No way to track event provenance for debugging
- Can't implement "scroll on drag outside bounds" for text selection

---

### Issue 3: Scattered GPU Updates

**Current State:**
```
ScrollManager::get_scrollbar_opacity()     ← Opacity calculation
       ↓
window.rs::synchronize_scrollbar_opacity() ← GPU cache update
       ↓
WebRender property bindings                ← Final rendering
```

**Problem:**
- GPU logic split across 2 files (scroll.rs, window.rs)
- No centralized GPU key lifecycle management
- Hard to add new GPU properties (transforms, filters)
- Opacity keys managed differently than transform keys

**Impact:**
- Adding scrollbar transforms (Phase 5) will scatter logic further
- No single source of truth for GPU state
- Difficult to optimize GPU updates

---

### Issue 4: No PipelineId Management

**Current State:**
- IFrames render into parent DOM's DisplayList
- No isolated rendering context for IFrames
- No way to do nested DisplayList rendering

**Problem:**
- Can't implement proper WebRender integration
- IFrame scroll doesn't work with WebRender scroll layers
- No way to cache IFrame rendering separately
- Can't handle IFrame transforms/clipping correctly

**Impact:**
- Phase 6 (WebRender sync) will require major refactoring anyway
- IFrame performance is suboptimal
- Can't implement IFrame-specific rendering features

---

## Proposed Refactoring (Phases 4-6)

### Design Goals

1. **Single Responsibility**: Each manager has ONE clear job
2. **Testability**: Each manager testable in isolation
3. **Extensibility**: Easy to add new GPU properties
4. **WebRender Compatibility**: Proper PipelineId management
5. **Event Provenance**: Track where every event came from
6. **Performance**: Minimize redundant calculations

---

### Architecture Comparison

#### Before (Phase 3 - MONOLITHIC):

```
┌─────────────────────────────────────────┐
│       ScrollManager (817 lines)         │
│  ┌───────────────────────────────────┐  │
│  │ Scroll State                      │  │
│  │ Scroll Animations                 │  │
│  │ IFrame Re-invocation ← MIXED      │  │
│  │ Scrollbar Opacity    ← MIXED      │  │
│  │ GPU Events           ← MIXED      │  │
│  └───────────────────────────────────┘  │
└─────────────────────────────────────────┘
            ↓
┌─────────────────────────────────────────┐
│           window.rs                     │
│  synchronize_scrollbar_opacity() ← SCATTERED
└─────────────────────────────────────────┘
```

#### After (Phases 4-6 - CLEAN SEPARATION):

```
┌──────────────────────────────────────────────────────────────┐
│                      Event Loop                              │
│  (User Input → Event Classification → Manager Dispatch)     │
└──────────────────────────────────────────────────────────────┘
                    ↓
    ┌───────────────┴───────────────┬──────────────────┐
    ↓                               ↓                  ↓
┌─────────────────┐    ┌──────────────────┐    ┌────────────────┐
│ ScrollManager   │    │ IFrameManager    │    │GpuStateManager │
│  (300 lines)    │    │  (200 lines)     │    │  (250 lines)   │
├─────────────────┤    ├──────────────────┤    ├────────────────┤
│• Scroll state   │    │• Re-invocation   │    │• Opacity keys  │
│• Animations     │───→│  detection       │───→│• Transform keys│
│• Event source   │    │• PipelineIds     │    │• GPU lifecycle │
│  tracking       │    │• Edge detection  │    │• Fades/Tweens  │
│• Activity time  │    │                  │    │                │
└─────────────────┘    └──────────────────┘    └────────────────┘
        │                      │                       │
        └──────────────────────┴───────────────────────┘
                               ↓
                    ┌──────────────────────┐
                    │   WebRender Sync     │
                    │ • DisplayLists       │
                    │ • Property Bindings  │
                    │ • Scroll Layers      │
                    └──────────────────────┘
```

---

## New Architecture: Three Managers

### 1. ScrollManager (Pure Scroll State)

**Single Responsibility:** Track scroll positions and smooth scrolling

**File:** `layout/src/scroll.rs` (~300 lines after refactoring)

**Structure:**
```rust
pub struct ScrollManager {
    states: BTreeMap<(DomId, NodeId), ScrollState>,
    
    // Activity tracking for repaint optimization
    had_scroll_activity: bool,
    had_programmatic_scroll: bool,
    had_new_doms: bool,
}

struct ScrollState {
    // ONLY scroll-related fields:
    current_offset: LogicalPosition,
    target_offset: Option<LogicalPosition>,  // For smooth scrolling
    
    // Bounds (from layout)
    content_rect: LogicalRect,
    container_rect: LogicalRect,
    
    // Animation config (per-item)
    scroll_duration: Duration,     // Default: 200ms
    last_update_time: Instant,     // For smooth interpolation
    last_update_source: EventSource, // User vs Programmatic
}
```

**Public API:**
```rust
impl ScrollManager {
    // === Input: Process events with source classification ===
    
    pub fn process_event(
        &mut self,
        event: ScrollEvent,  // Contains EventSource
        now: Instant,
    ) -> bool;  // Returns: needs_repaint
    
    // === Output: Get current positions ===
    
    pub fn get(
        &self,
        dom_id: DomId,
        node_id: NodeId,
    ) -> Option<ScrollInfo>;
    
    // === Animation: Internal smooth interpolation ===
    
    pub fn tick(&mut self, now: Instant) -> ScrollTickResult;
    
    // === Frame lifecycle ===
    
    pub fn begin_frame(&mut self);
    pub fn end_frame(&mut self) -> FrameScrollInfo;
}

pub struct ScrollTickResult {
    pub needs_repaint: bool,  // Any animation in progress?
    pub updated_nodes: Vec<(DomId, NodeId)>,  // Which nodes changed?
}
```

**Key Changes from Phase 3:**
- ❌ **REMOVE** all `iframe_*` fields → moved to IFrameManager
- ❌ **REMOVE** `get_scrollbar_opacity()` → moved to GpuStateManager
- ❌ **REMOVE** `check_iframe_reinvoke_condition()` → moved to IFrameManager
- ✅ **ADD** `EventSource` tracking (User/Programmatic/Synthetic)
- ✅ **ADD** per-item animation configuration
- ✅ **ADD** internal smooth interpolation (tick returns needs_repaint)

**Smooth Scrolling Strategy:**
- When user scrolls: `target_offset = current + delta`, animate over `scroll_duration`
- When programmatic: `target_offset = new_position`, animate or instant based on config
- When synthetic (scrollbar drag): instant update, no animation
- `tick()` interpolates `current_offset` toward `target_offset` based on elapsed time
- Returns `needs_repaint = true` if any animation is active

---

### 2. IFrameManager (IFrame Lifecycle)

**Single Responsibility:** Manage IFrame re-invocation and WebRender PipelineIds

**File:** `layout/src/iframe_manager.rs` (NEW - ~200 lines)

**Structure:**
```rust
pub struct IFrameManager {
    states: BTreeMap<(DomId, NodeId), IFrameState>,
    pipeline_ids: BTreeMap<(DomId, NodeId), PipelineId>,
}

struct IFrameState {
    // Re-invocation tracking (moved from ScrollState)
    iframe_scroll_size: Option<LogicalSize>,
    iframe_was_invoked: bool,
    invoked_for_current_expansion: bool,
    invoked_for_current_edge: bool,
    last_edge_triggered: EdgeFlags,
    
    // Nested DOM tracking
    nested_dom_id: DomId,  // The IFrame's content DOM
    
    // Last known bounds (for change detection)
    last_bounds: LogicalRect,
}
```

**Public API:**
```rust
impl IFrameManager {
    // === Input: Check if re-invocation needed ===
    
    pub fn check_reinvoke(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        scroll_info: &ScrollInfo,      // From ScrollManager
        layout_bounds: LogicalRect,    // From layout
    ) -> Option<IFrameCallbackReason>;
    
    // === Mark as invoked (prevents duplicate callbacks) ===
    
    pub fn mark_invoked(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        reason: IFrameCallbackReason,
    );
    
    // === PipelineId management for WebRender ===
    
    pub fn get_or_create_pipeline_id(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
    ) -> PipelineId;
    
    pub fn get_nested_dom_id(
        &self,
        dom_id: DomId,
        node_id: NodeId,
    ) -> Option<DomId>;
    
    // === Frame lifecycle ===
    
    pub fn begin_frame(&mut self);
}

pub enum IFrameCallbackReason {
    InitialRender,
    ContentSizeChanged,
    EdgeScrolled(EdgeFlags),
}
```

**Key Features:**
- ✅ All `iframe_*` logic moved here from ScrollManager
- ✅ PipelineId allocation and tracking
- ✅ Nested DOM ID mapping
- ✅ Clean separation from scroll state
- ✅ Ready for WebRender nested DisplayList rendering

---

### 3. GpuStateManager (All GPU Keys)

**Single Responsibility:** Manage ALL GPU property bindings (opacity, transforms, filters)

**File:** `layout/src/gpu_manager.rs` (NEW - ~250 lines)

**Structure:**
```rust
pub struct GpuStateManager {
    // Scrollbar opacity tracking
    opacity_keys: BTreeMap<(DomId, NodeId), GpuPropertyId>,
    opacity_states: BTreeMap<(DomId, NodeId), OpacityState>,
    
    // Scrollbar transform tracking (Phase 5+)
    transform_keys: BTreeMap<(DomId, NodeId), GpuPropertyId>,
    transform_values: BTreeMap<(DomId, NodeId), LayoutTransform>,
    
    // Global configuration
    fade_delay: Duration,      // Default: 500ms
    fade_duration: Duration,   // Default: 200ms
}

struct OpacityState {
    current_value: f32,
    target_value: f32,
    last_activity_time: Instant,
    transition_start_time: Option<Instant>,
}
```

**Public API:**
```rust
impl GpuStateManager {
    // === Input: Update from scroll/layout changes ===
    
    pub fn update_scrollbar_opacity(
        &mut self,
        scroll_info: &BTreeMap<(DomId, NodeId), ScrollInfo>,
        scrollbar_info: &BTreeMap<(DomId, NodeId), ScrollbarInfo>,
        now: Instant,
    ) -> GpuUpdateResult;
    
    pub fn update_scrollbar_transforms(
        &mut self,
        scroll_info: &BTreeMap<(DomId, NodeId), ScrollInfo>,
        scrollbar_info: &BTreeMap<(DomId, NodeId), ScrollbarInfo>,
    ) -> GpuUpdateResult;
    
    // === Animation: Internal fade transitions ===
    
    pub fn tick(&mut self, now: Instant) -> GpuTickResult;
    
    // === Output: Generate GPU cache for WebRender ===
    
    pub fn get_gpu_value_cache(&self) -> GpuValueCache;
}

pub struct GpuTickResult {
    pub needs_repaint: bool,  // Any fade in progress?
    pub updated_properties: Vec<GpuPropertyId>,
}

pub struct GpuUpdateResult {
    pub properties_added: Vec<GpuPropertyId>,
    pub properties_updated: Vec<GpuPropertyId>,
    pub properties_removed: Vec<GpuPropertyId>,
}
```

**Key Features:**
- ✅ **ALL GPU logic centralized here**
- ✅ Moved `get_scrollbar_opacity()` from ScrollManager
- ✅ Moved `synchronize_scrollbar_opacity()` from window.rs
- ✅ Internal fade transition management
- ✅ Ready for transform keys (Phase 5)
- ✅ Ready for future properties (filters, blend modes)
- ✅ Clean input (scroll info) → output (GPU cache) interface

**Fade Strategy:**
- Track `last_activity_time` per scrollbar
- On activity: `target_value = 1.0`, start fade-in if needed
- On tick: check `time_since_activity`
  - If < `fade_delay`: maintain `current_value = 1.0`
  - If > `fade_delay`: interpolate toward `target_value = 0.0` over `fade_duration`
- Returns `needs_repaint = true` if any fade is in progress

---

## Animation Strategy: Internal Manager Ticks

### Core Principle: Managers Handle Their Own Animations

**Key Insight:** Smooth scrolling and fade transitions are **internal concerns** of their respective managers. The event loop doesn't need to orchestrate animations—it just needs to know **if a repaint is needed**.

---

### ScrollManager: Smooth Scroll Interpolation

**Problem:** When user scrolls, we want smooth animation. When programmatically scrolled, we want instant update (or configurable).

**Solution:** ScrollManager tracks `target_offset` and interpolates internally.

```rust
impl ScrollManager {
    pub fn tick(&mut self, now: Instant) -> ScrollTickResult {
        let mut needs_repaint = false;
        let mut updated_nodes = Vec::new();
        
        for ((dom_id, node_id), state) in &mut self.states {
            // Check if we have a target to animate toward
            if let Some(target) = state.target_offset {
                if state.current_offset != target {
                    // Calculate interpolation factor
                    let elapsed = now.duration_since(state.last_update_time);
                    let t = (elapsed.as_millis() as f32 
                           / state.scroll_duration.as_millis() as f32)
                           .min(1.0);
                    
                    // Apply easing (for now, linear)
                    let delta_x = (target.x - state.current_offset.x) * t;
                    let delta_y = (target.y - state.current_offset.y) * t;
                    
                    state.current_offset.x += delta_x;
                    state.current_offset.y += delta_y;
                    
                    needs_repaint = true;
                    updated_nodes.push((*dom_id, *node_id));
                    
                    // If we've reached the target, clear it
                    if t >= 1.0 {
                        state.current_offset = target;
                        state.target_offset = None;
                    }
                }
            }
        }
        
        ScrollTickResult { needs_repaint, updated_nodes }
    }
    
    pub fn process_event(&mut self, event: ScrollEvent, now: Instant) -> bool {
        let state = self.states.entry((event.dom_id, event.node_id))
            .or_insert_with(|| ScrollState::new(now));
        
        match event.source {
            EventSource::User => {
                // User scroll: animate smoothly
                state.target_offset = Some(LogicalPosition {
                    x: state.current_offset.x + event.delta.x,
                    y: state.current_offset.y + event.delta.y,
                });
                state.last_update_time = now;
                state.last_update_source = EventSource::User;
                true  // needs_repaint
            }
            EventSource::Programmatic => {
                // API call: instant update (or configurable)
                state.current_offset = event.delta;  // delta is absolute in this case
                state.target_offset = None;  // Cancel any ongoing animation
                state.last_update_time = now;
                state.last_update_source = EventSource::Programmatic;
                true  // needs_repaint
            }
            EventSource::Synthetic => {
                // Scrollbar drag: instant update
                state.current_offset = event.delta;
                state.target_offset = None;
                state.last_update_time = now;
                state.last_update_source = EventSource::Synthetic;
                true  // needs_repaint
            }
        }
    }
}
```

**Benefits:**
- ✅ Smooth user scrolling without event loop involvement
- ✅ Instant programmatic updates (can be made configurable)
- ✅ Instant synthetic updates (scrollbar drag feels responsive)
- ✅ Animation can be stopped mid-flight by new events
- ✅ Per-item configuration (different durations per scrollable element)

---

### GpuStateManager: Fade Transitions

**Problem:** Scrollbars should fade in/out smoothly. WebRender expects opacity values, not animation state.

**Solution:** GpuStateManager tracks `last_activity_time` and interpolates opacity internally.

```rust
impl GpuStateManager {
    pub fn tick(&mut self, now: Instant) -> GpuTickResult {
        let mut needs_repaint = false;
        let mut updated_properties = Vec::new();
        
        for ((dom_id, node_id), opacity_state) in &mut self.opacity_states {
            let time_since_activity = now.duration_since(opacity_state.last_activity_time);
            
            // Phase 1: Delay (scrollbar stays visible)
            if time_since_activity < self.fade_delay {
                if opacity_state.current_value != 1.0 {
                    // Fade in to full opacity
                    opacity_state.target_value = 1.0;
                    needs_repaint = true;
                }
                continue;
            }
            
            // Phase 2: Fade out
            let time_into_fade = time_since_activity - self.fade_delay;
            if time_into_fade < self.fade_duration {
                // Interpolate toward 0.0
                let t = time_into_fade.as_millis() as f32 
                      / self.fade_duration.as_millis() as f32;
                let new_opacity = 1.0 - t;
                
                if opacity_state.current_value != new_opacity {
                    opacity_state.current_value = new_opacity;
                    needs_repaint = true;
                    updated_properties.push(self.opacity_keys[&(*dom_id, *node_id)]);
                }
            } else {
                // Phase 3: Fully faded
                if opacity_state.current_value != 0.0 {
                    opacity_state.current_value = 0.0;
                    updated_properties.push(self.opacity_keys[&(*dom_id, *node_id)]);
                }
            }
        }
        
        GpuTickResult { needs_repaint, updated_properties }
    }
    
    pub fn update_scrollbar_opacity(
        &mut self,
        scroll_info: &BTreeMap<(DomId, NodeId), ScrollInfo>,
        scrollbar_info: &BTreeMap<(DomId, NodeId), ScrollbarInfo>,
        now: Instant,
    ) -> GpuUpdateResult {
        let mut result = GpuUpdateResult::default();
        
        for ((dom_id, node_id), _) in scrollbar_info {
            let opacity_state = self.opacity_states
                .entry((*dom_id, *node_id))
                .or_insert_with(|| OpacityState {
                    current_value: 0.0,
                    target_value: 0.0,
                    last_activity_time: now,
                    transition_start_time: None,
                });
            
            // Update activity time (triggers fade-in)
            opacity_state.last_activity_time = now;
            opacity_state.target_value = 1.0;
            
            // Ensure we have a GPU property ID
            if !self.opacity_keys.contains_key(&(*dom_id, *node_id)) {
                let property_id = allocate_gpu_property_id();
                self.opacity_keys.insert((*dom_id, *node_id), property_id);
                result.properties_added.push(property_id);
            } else {
                result.properties_updated.push(
                    self.opacity_keys[&(*dom_id, *node_id)]
                );
            }
        }
        
        result
    }
}
```

**Benefits:**
- ✅ Fade transitions without event loop involvement
- ✅ Automatic fade-out after inactivity
- ✅ Fade-in on new scroll activity
- ✅ Per-item configuration (different fade timings)
- ✅ WebRender receives clean opacity values, not state machines

---

### Event Loop Integration

**The event loop becomes much simpler:**

```rust
// Main render loop (pseudo-code)
loop {
    let now = Instant::now();
    
    // 1. Process input events
    for event in input_events {
        match event {
            InputEvent::Scroll(scroll_event) => {
                let needs_repaint = scroll_manager.process_event(scroll_event, now);
                if needs_repaint {
                    request_repaint();
                }
            }
            // ... other events
        }
    }
    
    // 2. Tick all managers (internal animations)
    let scroll_tick = scroll_manager.tick(now);
    let gpu_tick = gpu_state_manager.tick(now);
    
    if scroll_tick.needs_repaint || gpu_tick.needs_repaint {
        request_repaint();
    }
    
    // 3. If repaint requested, do layout + render
    if should_repaint {
        // 3a. Check for IFrame re-invocations
        let scroll_positions = scroll_manager.get_all_positions();
        for (dom_id, node_id) in iframes {
            if let Some(reason) = iframe_manager.check_reinvoke(
                dom_id, node_id, &scroll_positions[&(dom_id, node_id)], layout_bounds
            ) {
                invoke_iframe_callback(dom_id, node_id, reason);
                iframe_manager.mark_invoked(dom_id, node_id, reason);
            }
        }
        
        // 3b. Update GPU state
        let scrollbar_info = compute_scrollbar_info_from_layout();
        gpu_state_manager.update_scrollbar_opacity(
            &scroll_positions, &scrollbar_info, now
        );
        
        // 3c. Generate display list
        let gpu_cache = gpu_state_manager.get_gpu_value_cache();
        let display_list = generate_display_list(..., &gpu_cache);
        
        // 3d. Send to renderer
        webrender.update(display_list, gpu_cache);
    }
    
    // 4. Sleep until next frame or event
    wait_for_events_or_timeout(16ms);
}
```

**Key Points:**
- Event loop **asks** managers if repaint is needed
- Managers **internally** handle their animations
- Event loop **doesn't orchestrate** smooth scrolling or fades
- Clean separation of concerns

---

## Event System Enhancement

### EventSource Classification

**New Enum:**
```rust
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum EventSource {
    /// Direct user input (mouse wheel, touch, keyboard)
    User,
    
    /// API call (set_scroll_position, scroll_to, scroll_by)
    Programmatic,
    
    /// Generated from UI interaction (scrollbar drag, arrow click)
    Synthetic,
}
```

**Updated ScrollEvent:**
```rust
pub struct ScrollEvent {
    pub dom_id: DomId,
    pub node_id: NodeId,
    pub delta: LogicalPosition,
    pub source: EventSource,  // NEW!
}
```

**Benefits:**
- ✅ Prevents feedback loops (synthetic events don't trigger scrollbar updates)
- ✅ Different animation behaviors per source
- ✅ Better debugging (know where events came from)
- ✅ Enables proper scrollbar drag implementation

---

### Scrollbar Hit IDs

**New Enum for Hit-Testing:**
```rust
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum ScrollbarHitId {
    // Vertical scrollbar
    VerticalTrack(DomId, NodeId),
    VerticalThumb(DomId, NodeId),
    VerticalArrowUp(DomId, NodeId),
    VerticalArrowDown(DomId, NodeId),
    
    // Horizontal scrollbar
    HorizontalTrack(DomId, NodeId),
    HorizontalThumb(DomId, NodeId),
    HorizontalArrowLeft(DomId, NodeId),
    HorizontalArrowRight(DomId, NodeId),
}
```

**Event Flow:**
```
User clicks scrollbar thumb
    ↓
Hit-test system returns: ScrollbarHitId::VerticalThumb(dom, node)
    ↓
Event loop generates: ScrollEvent {
    dom_id: dom,
    node_id: node,
    delta: calculate_from_thumb_position(),
    source: EventSource::Synthetic,  ← KEY!
}
    ↓
ScrollManager.process_event() updates scroll position (instant, no animation)
    ↓
GpuStateManager.update_scrollbar_opacity() resets fade timer
    ↓
WebRender receives updated scroll offset + opacity = 1.0
```

---

## Migration Plan

### Phase 4a: Extract IFrameManager (2-3 hours)

**Goal:** Move all IFrame logic out of ScrollManager into new IFrameManager

**Steps:**
1. Create `layout/src/iframe_manager.rs`
2. Define `IFrameManager` and `IFrameState` structs
3. Move fields from `ScrollState`:
   - `iframe_scroll_size`
   - `iframe_was_invoked`
   - `invoked_for_current_expansion`
   - `invoked_for_current_edge`
   - `last_edge_triggered`
4. Move methods from `ScrollManager`:
   - `check_iframe_reinvoke_condition()` → `IFrameManager::check_reinvoke()`
   - `mark_iframe_invoked()` → `IFrameManager::mark_invoked()`
   - `update_iframe_scroll_info()` → `IFrameManager::update_state()`
5. Update `window.rs::invoke_iframe_callback()` to use both managers:
   ```rust
   // Get scroll info from ScrollManager
   let scroll_info = scroll_manager.get(dom_id, node_id);
   
   // Check if re-invocation needed from IFrameManager
   if let Some(reason) = iframe_manager.check_reinvoke(dom_id, node_id, &scroll_info, bounds) {
       // Invoke callback
       let new_dom = callback.invoke(reason);
       
       // Mark as invoked
       iframe_manager.mark_invoked(dom_id, node_id, reason);
   }
   ```
6. Run all tests: **90/90 must still pass**

**Verification:**
- `ScrollState` has no `iframe_*` fields
- `ScrollManager` has no IFrame-specific methods
- `IFrameManager` is single source of truth for IFrame logic
- All 90 tests pass unchanged

---

### Phase 4b: Extract GpuStateManager (2-3 hours)

**Goal:** Move all GPU logic out of ScrollManager and window.rs into new GpuStateManager

**Steps:**
1. Create `layout/src/gpu_manager.rs`
2. Define `GpuStateManager` and `OpacityState` structs
3. Move method from `ScrollManager`:
   - `get_scrollbar_opacity()` → `GpuStateManager::calculate_opacity()`
4. Move method from `window.rs`:
   - `synchronize_scrollbar_opacity()` → `GpuStateManager::update_scrollbar_opacity()`
5. Add `tick()` method for internal fade transitions
6. Update render loop to use GpuStateManager:
   ```rust
   // Get scroll info from ScrollManager
   let scroll_positions = scroll_manager.get_all_positions();
   
   // Get scrollbar info from layout
   let scrollbar_info = compute_scrollbar_info();
   
   // Update GPU state (returns needs_repaint)
   gpu_manager.update_scrollbar_opacity(&scroll_positions, &scrollbar_info, now);
   
   // Tick for fade transitions
   let gpu_tick = gpu_manager.tick(now);
   if gpu_tick.needs_repaint {
       request_repaint();
   }
   
   // Get GPU cache for rendering
   let gpu_cache = gpu_manager.get_gpu_value_cache();
   ```
7. Run all tests: **90/90 must still pass**

**Verification:**
- `ScrollManager` has no GPU-related methods
- `window.rs` has no `synchronize_scrollbar_opacity()`
- `GpuStateManager` is single source of truth for GPU keys
- All 90 tests pass unchanged

---

### Phase 4c: Event Source Classification (1-2 hours)

**Goal:** Add event source tracking to enable proper scrollbar interaction

**Steps:**
1. Add `EventSource` enum to `core/src/events.rs`
2. Update `ScrollEvent` struct to include `source: EventSource`
3. Update all event generation sites:
   - User scroll: `EventSource::User`
   - API calls: `EventSource::Programmatic`
   - (Phase 5: Scrollbar interaction: `EventSource::Synthetic`)
4. Update `ScrollManager::process_event()` to handle source:
   ```rust
   match event.source {
       EventSource::User => {
           // Smooth animation
           state.target_offset = Some(state.current_offset + event.delta);
       }
       EventSource::Programmatic => {
           // Instant update
           state.current_offset = event.delta;
           state.target_offset = None;
       }
       EventSource::Synthetic => {
           // Instant update (Phase 5)
           state.current_offset = event.delta;
           state.target_offset = None;
       }
   }
   ```
5. Add tests for event source handling
6. Run all tests: **90/90 must still pass** + new event source tests

**Verification:**
- All scroll events have explicit `EventSource`
- Different behaviors for User vs. Programmatic
- Event source preserved through event pipeline
- Tests verify source-specific behavior

---

### Phase 5: Scrollbar Transforms (4-5 hours)

**Goal:** Synchronize scrollbar thumb position with scroll offset

**Steps:**
1. Add `ScrollbarHitId` enum to `core/src/hit_test.rs`
2. Update hit-testing to return scrollbar component IDs
3. Add scrollbar interaction handlers:
   - Track drag: generate `EventSource::Synthetic` scroll events
   - Arrow click: generate programmatic scroll events
4. Add transform key management to `GpuStateManager`:
   ```rust
   pub fn update_scrollbar_transforms(
       &mut self,
       scroll_positions: &BTreeMap<(DomId, NodeId), ScrollInfo>,
       scrollbar_info: &BTreeMap<(DomId, NodeId), ScrollbarInfo>,
   ) -> GpuUpdateResult {
       for ((dom_id, node_id), scroll_info) in scroll_positions {
           let scrollbar = &scrollbar_info[&(*dom_id, *node_id)];
           
           // Calculate thumb position
           let scroll_ratio = scroll_info.current_offset.y / scroll_info.content_rect.height;
           let thumb_offset = scroll_ratio * scrollbar.track_height;
           
           // Update transform key
           let transform = LayoutTransform::translate(0.0, thumb_offset, 0.0);
           self.transform_values.insert((*dom_id, *node_id), transform);
       }
       // ... return updated property IDs
   }
   ```
5. Add configurable arrow key step sizes
6. Add tests for scrollbar interaction
7. Run all tests: **90/90 existing + new scrollbar tests must pass**

**Verification:**
- Scrollbar thumb moves with scroll position
- Dragging thumb updates scroll offset
- Arrow buttons work
- Transform keys in GPU cache
- All tests pass

---

### Phase 6: WebRender Integration (5-6 hours)

**Goal:** Proper scroll layer synchronization and IFrame PipelineIds

**Steps:**
1. Add `PipelineId` tracking to `IFrameManager`:
   ```rust
   pub fn get_or_create_pipeline_id(
       &mut self,
       dom_id: DomId,
       node_id: NodeId,
   ) -> PipelineId {
       self.pipeline_ids.entry((dom_id, node_id))
           .or_insert_with(|| allocate_pipeline_id())
           .clone()
   }
   ```
2. Update DisplayList generation to use PipelineIds for IFrames
3. Implement nested DisplayList rendering:
   ```rust
   // For each IFrame:
   let pipeline_id = iframe_manager.get_or_create_pipeline_id(dom_id, node_id);
   let nested_dom_id = iframe_manager.get_nested_dom_id(dom_id, node_id);
   
   // Generate nested DisplayList
   let nested_display_list = generate_display_list(nested_dom_id, ...);
   
   // Push IFrame primitive
   builder.push_iframe(bounds, pipeline_id, nested_display_list);
   ```
4. Send scroll offsets to WebRender scroll layers:
   ```rust
   for ((dom_id, node_id), scroll_info) in scroll_positions {
       webrender.set_scroll_offset(
           scroll_id_for_node(dom_id, node_id),
           scroll_info.current_offset,
       );
   }
   ```
5. Synchronize GPU property bindings with scroll layers
6. Add tests for IFrame rendering and scroll propagation
7. Run all tests: **90/90 existing + Phase 5 + Phase 6 tests must pass**

**Verification:**
- IFrames have unique PipelineIds
- Nested DisplayLists render correctly
- Scroll offsets sent to WebRender
- GPU properties bound to correct scroll layers
- All tests pass

---

## Testing Strategy

### Maintaining Test Coverage

**Golden Rule:** All existing 90 tests must pass after each refactoring phase.

**Test Categories:**
1. **Unit Tests** (per manager):
   - `scroll_manager_tests.rs` - Scroll state, animations
   - `iframe_manager_tests.rs` - Re-invocation detection
   - `gpu_manager_tests.rs` - Opacity/transform calculations

2. **Integration Tests** (multi-manager):
   - `scroll_iframe_integration_tests.rs` - Scroll triggers IFrame re-invocation
   - `scroll_gpu_integration_tests.rs` - Scroll triggers opacity updates
   - `end_to_end_tests.rs` - Full pipeline tests

3. **Regression Tests** (from Phase 3 bugs):
   - ScrollManager.get() returns content_rect (not container)
   - IFrame InitialRender only called once
   - None callback returns Dom::div() fallback

---

### New Tests for Phases 4-6

**Phase 4a (IFrameManager):**
- ✅ IFrame re-invocation on edge scroll
- ✅ IFrame re-invocation on content expansion
- ✅ Prevention of duplicate InitialRender
- ✅ PipelineId allocation and persistence
- ✅ Nested DOM ID mapping

**Phase 4b (GpuStateManager):**
- ✅ Opacity calculation from last activity time
- ✅ Fade-in transition (0.0 → 1.0 over 200ms)
- ✅ Fade-out transition (1.0 → 0.0 over 200ms)
- ✅ Fade delay (500ms of full visibility)
- ✅ Multiple simultaneous fades
- ✅ GPU property ID lifecycle

**Phase 4c (Event Source):**
- ✅ User events trigger smooth scrolling
- ✅ Programmatic events instant update
- ✅ Synthetic events instant update
- ✅ Event source preserved through pipeline
- ✅ Different animation behaviors per source

**Phase 5 (Scrollbar Transforms):**
- ✅ Thumb position matches scroll offset
- ✅ Dragging thumb updates scroll position
- ✅ Arrow button clicks scroll correctly
- ✅ Track click scrolls by page
- ✅ Transform keys in GPU cache
- ✅ Scrollbar hit IDs in hit-test results

**Phase 6 (WebRender Integration):**
- ✅ IFrame PipelineIds allocated correctly
- ✅ Nested DisplayLists generated
- ✅ Scroll offsets sent to WebRender
- ✅ GPU properties bound to scroll layers
- ✅ IFrame scroll isolation

---

## Timeline & Estimates

### Phase 4: Refactoring (5-8 hours)

- **Phase 4a:** IFrameManager extraction - **2-3 hours**
  - Create new file, move structs/methods
  - Update window.rs integration
  - Run tests, fix breakages
  
- **Phase 4b:** GpuStateManager extraction - **2-3 hours**
  - Create new file, move GPU logic
  - Update render loop integration
  - Run tests, fix breakages
  
- **Phase 4c:** Event source classification - **1-2 hours**
  - Add EventSource enum
  - Update event generation
  - Add tests

**Milestone:** Clean 3-manager architecture, 90/90 tests pass

---

### Phase 5: Scrollbar Transforms (4-5 hours)

- **Scrollbar hit-testing** - **1-2 hours**
  - Add ScrollbarHitId enum
  - Update hit-test system
  
- **Scrollbar interaction** - **2-2 hours**
  - Thumb drag implementation
  - Arrow button clicks
  - Track clicks
  
- **Transform synchronization** - **1-2 hours**
  - Add transform keys to GpuStateManager
  - Calculate thumb position
  - Update GPU cache
  
- **Testing** - **1 hour**
  - Write interaction tests
  - Verify transform updates

**Milestone:** Functional scrollbars, all tests pass

---

### Phase 6: WebRender Integration (5-6 hours)

- **PipelineId management** - **2 hours**
  - Add to IFrameManager
  - Update IFrame rendering
  
- **Nested DisplayLists** - **2-3 hours**
  - Generate per-IFrame DisplayLists
  - Push IFrame primitives correctly
  
- **Scroll layer sync** - **1-2 hours**
  - Send scroll offsets to WebRender
  - Bind GPU properties to layers
  
- **Testing** - **1 hour**
  - IFrame rendering tests
  - Scroll propagation tests

**Milestone:** Full WebRender integration, all tests pass

---

### Total Estimated Time: 14-19 hours

**Breakdown:**
- Phase 4a-c: 5-8 hours (refactoring)
- Phase 5: 4-5 hours (scrollbar interaction)
- Phase 6: 5-6 hours (WebRender integration)

**Realistic Schedule:**
- Week 1: Phase 4 (refactoring)
- Week 2: Phase 5 (scrollbar transforms)
- Week 3: Phase 6 (WebRender integration)
- Week 4: Polish, edge cases, documentation

---

## Conclusion

### Current State (Phase 3)

✅ **Working scroll system** with 90/90 tests passing  
❌ **Architectural debt** from mixed responsibilities  
❌ **No event source tracking** for synthetic events  
❌ **No PipelineId management** for WebRender  

### Future State (After Phases 4-6)

✅ **Clean 3-manager architecture** (ScrollManager, IFrameManager, GpuStateManager)  
✅ **Event source classification** (User, Programmatic, Synthetic)  
✅ **Internal animation management** (smooth scrolling, fades)  
✅ **Centralized GPU key management** (opacity, transforms)  
✅ **WebRender integration** (PipelineIds, nested DisplayLists, scroll layers)  
✅ **Full scrollbar interaction** (drag, arrow clicks, track clicks)  

### Key Benefits

1. **Maintainability:** Each manager has a single, clear responsibility
2. **Testability:** Managers can be tested in isolation
3. **Extensibility:** Easy to add new GPU properties (filters, blend modes)
4. **Performance:** Internal animations don't burden event loop
5. **Correctness:** Event source tracking prevents feedback loops
6. **WebRender Ready:** Proper PipelineIds and scroll layer synchronization

### Next Steps

1. **Review this document** with stakeholders
2. **Start Phase 4a** (IFrameManager extraction)
3. **Verify 90/90 tests** still pass after each phase
4. **Iterate** based on findings during refactoring
5. **Document** any deviations from this plan

---

**Document Status:** ACTIVE (Replaces v1.0)  
**Last Updated:** October 18, 2025  
**Next Review:** After Phase 4 completion
