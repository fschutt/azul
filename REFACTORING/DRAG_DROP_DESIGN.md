# HTML-Like Drag and Drop System Design

## Overview

This document outlines the design for a comprehensive drag-and-drop system that mirrors HTML5 drag-and-drop APIs, supporting both element dragging and file drop operations.

## Current State

The existing event system has:
- Basic mouse events (MouseDown, MouseUp, MouseOver, MouseEnter, MouseLeave)
- No semantic drag events (using MouseOver as workaround for CSD titlebar drag)
- No file drop support
- No drag-and-drop state tracking

## Proposed Architecture

### 1. Core Event Types (core/src/events.rs)

Add to `HoverEventFilter`, `FocusEventFilter`, and `WindowEventFilter`:

```rust
pub enum HoverEventFilter {
    // ... existing variants ...
    
    // Element drag events
    DragStart,      // Mouse button down + movement started
    Drag,           // Continuous movement while dragging
    DragEnd,        // Mouse button released
    
    // Drop target events (like HTML)
    DragEnter,      // Dragged element enters this node
    DragOver,       // Dragged element hovering over this node
    DragLeave,      // Dragged element leaves this node
    Drop,           // Dragged element dropped on this node
    
    // Double-click event
    DoubleClick,    // Two rapid clicks (OS timing)
}
```

### 2. MouseState Extensions (core/src/window.rs)

```rust
pub struct MouseState {
    // ... existing fields ...
    
    /// Current drag state
    pub drag_state: DragState,
    
    /// Double-click detection flag
    pub double_click_detected: bool,
    
    /// Information about what's being dragged
    pub drag_data: Option<DragData>,
    
    /// File drag/drop state (from OS)
    pub file_drop_state: Option<FileDropState>,
}

/// Drag state machine
pub enum DragState {
    NotDragging,
    DragStarted {
        /// Position where drag started
        start_pos: LogicalPosition,
        /// DOM node being dragged
        source_node: Option<DomNodeId>,
    },
    Dragging {
        start_pos: LogicalPosition,
        current_pos: LogicalPosition,
        source_node: Option<DomNodeId>,
        /// Current drop target (node under cursor)
        current_target: Option<DomNodeId>,
    },
}

/// Data being dragged (like HTML DataTransfer)
pub struct DragData {
    /// MIME type -> data mapping
    pub data: BTreeMap<AzString, Vec<u8>>,
    
    /// Allowed drag operations
    pub effect_allowed: DragEffect,
    
    /// Current drop operation
    pub drop_effect: DropEffect,
}

/// Drag effect (like HTML dropEffect)
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum DragEffect {
    None,
    Copy,
    Move,
    Link,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum DropEffect {
    None,
    Copy,
    Move,
    Link,
}

/// File drop state
pub struct FileDropState {
    /// List of files being dragged over window
    pub files: Vec<PathBuf>,
    
    /// Current position of file drag
    pub position: LogicalPosition,
    
    /// Drop target node
    pub target_node: Option<DomNodeId>,
}
```

### 3. ProcessEventResult Enhancement (core/src/events.rs)

```rust
pub enum ProcessEventResult {
    // ... existing variants ...
    
    /// Drag state changed - need to update UI
    DragStateChanged,
    
    /// File drop occurred - application should handle files
    FilesDropped {
        files: Vec<PathBuf>,
        target_node: DomNodeId,
    },
}
```

### 4. Event Generation (dll/src/desktop/shell2/common/event_v2.rs)

Update `create_events_from_states()` to detect drag transitions:

```rust
// Drag state machine detection
match (&prev.mouse_state.drag_state, &curr.mouse_state.drag_state) {
    (DragState::NotDragging, DragState::DragStarted { .. }) => {
        events.push(SyntheticEvent::DragStart);
    }
    
    (DragState::DragStarted { .. } | DragState::Dragging { .. }, 
     DragState::Dragging { current_target, .. }) => {
        events.push(SyntheticEvent::Drag);
        
        // Check if drag target changed
        let prev_target = match &prev.mouse_state.drag_state {
            DragState::Dragging { current_target, .. } => *current_target,
            _ => None,
        };
        
        if prev_target != *current_target {
            // Generate DragLeave for previous target
            if let Some(prev_node) = prev_target {
                events.push(SyntheticEvent::DragLeave(prev_node));
            }
            
            // Generate DragEnter for new target
            if let Some(new_node) = current_target {
                events.push(SyntheticEvent::DragEnter(*new_node));
            }
        }
        
        // Generate DragOver for current target
        if let Some(target) = current_target {
            events.push(SyntheticEvent::DragOver(*target));
        }
    }
    
    (DragState::DragStarted { .. } | DragState::Dragging { current_target, .. }, 
     DragState::NotDragging) => {
        // Generate DragLeave if there was a target
        if let Some(target) = current_target {
            events.push(SyntheticEvent::DragLeave(*target));
        }
        
        events.push(SyntheticEvent::DragEnd);
        
        // If ended over a valid drop target, generate Drop event
        if let Some(target) = current_target {
            events.push(SyntheticEvent::Drop(*target));
        }
    }
    
    _ => {}
}

// Double-click detection
if curr.mouse_state.double_click_detected && !prev.mouse_state.double_click_detected {
    events.push(SyntheticEvent::DoubleClick);
}

// File drop detection
match (&prev.mouse_state.file_drop_state, &curr.mouse_state.file_drop_state) {
    (None, Some(file_drop)) => {
        // File drag entered window
        events.push(SyntheticEvent::FileDragEnter);
    }
    
    (Some(_), None) => {
        // File drag left window
        events.push(SyntheticEvent::FileDragLeave);
    }
    
    (Some(prev_drop), Some(curr_drop)) if prev_drop.target_node != curr_drop.target_node => {
        // File drag target changed
        if let Some(prev_node) = prev_drop.target_node {
            events.push(SyntheticEvent::FileDragLeave);
        }
        if let Some(curr_node) = curr_drop.target_node {
            events.push(SyntheticEvent::FileDragEnter);
        }
    }
    
    _ => {}
}
```

### 5. Platform Integration

#### macOS (dll/src/desktop/shell2/macos/events.rs)

```rust
// In handle_mouse_down:
fn handle_mouse_down(&mut self, event: &NSEvent) -> ProcessEventResult {
    // ... existing position logic ...
    
    // Check for double-click
    if event.clickCount() == 2 {
        self.current_window_state.mouse_state.double_click_detected = true;
    }
    
    // Start drag state
    self.current_window_state.mouse_state.drag_state = DragState::DragStarted {
        start_pos: position,
        source_node: self.current_window_state.last_hit_test.items.first()
            .map(|hit| DomNodeId { dom: hit.dom_id, node: hit.node_id }),
    };
    
    // ... rest of logic ...
}

// In handle_mouse_dragged:
fn handle_mouse_dragged(&mut self, event: &NSEvent) -> ProcessEventResult {
    // ... existing logic ...
    
    // Update drag state
    if let DragState::DragStarted { start_pos, source_node } = 
        self.current_window_state.mouse_state.drag_state 
    {
        let current_target = self.current_window_state.last_hit_test.items.first()
            .map(|hit| DomNodeId { dom: hit.dom_id, node: hit.node_id });
            
        self.current_window_state.mouse_state.drag_state = DragState::Dragging {
            start_pos,
            current_pos: position,
            source_node,
            current_target,
        };
    }
    
    // ... rest of logic ...
}

// In handle_mouse_up:
fn handle_mouse_up(&mut self, event: &NSEvent) -> ProcessEventResult {
    // ... existing logic ...
    
    // End drag state
    self.current_window_state.mouse_state.drag_state = DragState::NotDragging;
    
    // ... rest of logic ...
}

// File drop support (NSWindow drag destination):
impl MacOSWindow {
    fn register_drag_types(&self) {
        use objc2_app_kit::{NSFilenamesPboardType, NSDragOperation};
        
        self.window.registerForDraggedTypes(&[NSFilenamesPboardType]);
    }
    
    // NSDraggingDestination protocol methods:
    extern "C" fn dragging_entered(this: &Object, _cmd: Sel, sender: id) -> NSDragOperation {
        // Extract file paths from pasteboard
        // Update file_drop_state in current_window_state
        // Return NSDragOperationCopy
    }
    
    extern "C" fn dragging_updated(this: &Object, _cmd: Sel, sender: id) -> NSDragOperation {
        // Update file_drop_state position and target_node
    }
    
    extern "C" fn dragging_exited(this: &Object, _cmd: Sel, sender: id) {
        // Clear file_drop_state
    }
    
    extern "C" fn perform_drag_operation(this: &Object, _cmd: Sel, sender: id) -> BOOL {
        // Generate FilesDropped event
        // Clear file_drop_state
    }
}
```

#### Windows (dll/src/desktop/shell2/windows/mod.rs)

```rust
// Add WM_LBUTTONDBLCLK handler:
WM_LBUTTONDBLCLK => {
    // ... same as WM_LBUTTONDOWN but set double_click_detected ...
    window.current_window_state.mouse_state.double_click_detected = true;
    // ... rest of mouse down logic ...
}

// Drag state machine in existing handlers:
WM_LBUTTONDOWN => {
    // ... existing logic ...
    
    window.current_window_state.mouse_state.drag_state = DragState::DragStarted {
        start_pos: logical_pos,
        source_node: window.current_window_state.last_hit_test.items.first()
            .map(|hit| DomNodeId { dom: hit.dom_id, node: hit.node_id }),
    };
    
    // ... rest ...
}

WM_MOUSEMOVE => {
    // ... existing logic ...
    
    if let DragState::DragStarted { start_pos, source_node } = 
        window.current_window_state.mouse_state.drag_state 
    {
        let current_target = window.current_window_state.last_hit_test.items.first()
            .map(|hit| DomNodeId { dom: hit.dom_id, node: hit.node_id });
            
        window.current_window_state.mouse_state.drag_state = DragState::Dragging {
            start_pos,
            current_pos: logical_pos,
            source_node,
            current_target,
        };
    }
    
    // ... rest ...
}

WM_LBUTTONUP => {
    // ... existing logic ...
    
    window.current_window_state.mouse_state.drag_state = DragState::NotDragging;
    
    // ... rest ...
}

// File drop support (IDropTarget interface):
use windows_sys::Win32::System::Ole::{
    IDropTarget, IDropTargetVtbl, DragEnter, DragOver, DragLeave, Drop,
    DROPEFFECT_COPY, DROPEFFECT_NONE,
};

impl Win32Window {
    fn register_drag_drop(&self) {
        unsafe {
            RegisterDragDrop(self.hwnd, &self.drop_target as *const _ as *mut _);
        }
    }
}

// Implement IDropTarget callbacks similar to macOS
```

#### X11 (dll/src/desktop/shell2/linux/x11/events.rs)

```rust
// Timing-based double-click detection:
ButtonPress => {
    let now = Instant::now();
    
    // Check for double-click
    let is_double_click = if let Some(last_time) = self.last_click_time {
        let elapsed = now.duration_since(last_time);
        let distance = position.distance(self.last_click_position);
        elapsed < Duration::from_millis(500) && distance < 5.0
    } else {
        false
    };
    
    if is_double_click {
        self.current_window_state.mouse_state.double_click_detected = true;
        self.last_click_time = None; // Reset for next double-click
    } else {
        self.last_click_time = Some(now);
        self.last_click_position = position;
    }
    
    // Start drag state
    self.current_window_state.mouse_state.drag_state = DragState::DragStarted {
        start_pos: position,
        source_node: self.current_window_state.last_hit_test.items.first()
            .map(|hit| DomNodeId { dom: hit.dom_id, node: hit.node_id }),
    };
}

MotionNotify => {
    // ... existing logic ...
    
    if let DragState::DragStarted { start_pos, source_node } = 
        self.current_window_state.mouse_state.drag_state 
    {
        let current_target = self.current_window_state.last_hit_test.items.first()
            .map(|hit| DomNodeId { dom: hit.dom_id, node: hit.node_id });
            
        self.current_window_state.mouse_state.drag_state = DragState::Dragging {
            start_pos,
            current_pos: position,
            source_node,
            current_target,
        };
    }
}

ButtonRelease => {
    // ... existing logic ...
    
    self.current_window_state.mouse_state.drag_state = DragState::NotDragging;
}

// File drop support via XDND protocol:
// Handle ClientMessage events with XdndEnter, XdndPosition, XdndLeave, XdndDrop atoms
```

## Implementation Plan

### Phase 1: Basic Drag Events (Week 1)
1. Add DragStart/Drag/DragEnd to event enums
2. Add DragState to MouseState
3. Implement drag state machine in event_v2.rs
4. Add platform-specific drag detection (macOS, Windows, X11)
5. Update CSD titlebar to use proper drag events

### Phase 2: Drop Target Events (Week 2)
1. Add DragEnter/DragOver/DragLeave/Drop to event enums
2. Implement drop target tracking in event_v2.rs
3. Add DragData structure for data transfer
4. Test element-to-element drag and drop

### Phase 3: File Drop Support (Week 3)
1. Add FileDropState to MouseState
2. Implement platform-specific file drop registration
3. macOS: NSDraggingDestination protocol
4. Windows: IDropTarget COM interface
5. X11: XDND protocol implementation
6. Generate FilesDropped events

### Phase 4: Double-Click Support (Week 4)
1. Add DoubleClick to event enums
2. Platform-specific double-click detection
3. Test CSD titlebar double-click maximize

## Benefits

1. **Semantic Correctness**: Proper drag events instead of MouseOver workarounds
2. **HTML Compatibility**: Familiar API for web developers
3. **File Drop Support**: Native OS file drag-and-drop integration
4. **Extensibility**: Easy to add custom drag data types
5. **Accessibility**: Screen readers can announce drag operations

## Testing Strategy

1. Unit tests for drag state machine transitions
2. Integration tests for element dragging
3. Platform-specific file drop tests
4. CSD titlebar drag and double-click tests
5. Performance tests (drag event frequency)

## Migration Path

1. Add new drag events (non-breaking)
2. Deprecate MouseOver workaround in CSD
3. Update examples to use new drag APIs
4. Document migration guide for users

---

**Status**: Design phase
**Priority**: High (after regenerate_layout extraction)
**Estimated effort**: 4 weeks
**Dependencies**: None (can be implemented incrementally)
