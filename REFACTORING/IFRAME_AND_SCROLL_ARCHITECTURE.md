# IFrame and Scrolling Architecture - Complete Design

## Current Status

### What Works
- ✅ RefAny is memory-safe (all Miri tests pass)
- ✅ `layout_document` performs layout on a single StyledDom
- ✅ Basic scroll position tracking exists but is incomplete
- ✅ IFrame callbacks are defined in core but never invoked

### What's Broken
- ❌ IFrame callbacks are NEVER called during layout
- ❌ No integration between IFrame rendering and layout tree
- ❌ Scroll state management is incomplete
- ❌ No connection between scrolling and IFrame re-rendering
- ❌ No cursor-scroll interaction
- ❌ No "infinite DOM" pattern support

## Problem Analysis

### Why IFrame Callbacks Don't Work

**Root Cause**: `layout_document` is a single-DOM function. It has no concept of:
1. **Nested DOMs** (IFrames create child DOMs)
2. **Callback invocation** (it's a pure layout function)
3. **Multi-pass rendering** (IFrames need recursive layout)

**Current Call Chain**:
```
Test calls layout_document()
  └─> Lays out DOM with IFrame node
  └─> Generates display list
  └─> Returns
  └─> IFrame callback NEVER CALLED ❌
```

**Expected Call Chain**:
```
LayoutWindow::layout_and_generate_display_list()
  └─> Call layout_document() for root DOM
  └─> Scan layout tree for IFrame nodes
  └─> For each IFrame:
      ├─> Invoke IFrame callback
      ├─> Get returned StyledDom
      ├─> Assign child DomId
      ├─> Recursively call layout_document()
      └─> Integrate child display list
  └─> Generate final composite display list
```

### Current Architecture Gaps

```
┌─────────────────────────────────────────────────────────────────┐
│                       CURRENT (BROKEN)                          │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Test → layout_document()                                       │
│           │                                                     │
│           ├─> Layouts DOM with IFrame node                     │
│           ├─> IFrame is treated as empty box                   │
│           └─> Returns display list                             │
│                                                                 │
│  ❌ IFrame callback never invoked                              │
│  ❌ No child DOM generated                                     │
│  ❌ No recursive layout                                        │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                       NEEDED (FIXED)                            │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  Test → LayoutWindow::layout()                                  │
│           │                                                     │
│           ├─> layout_document(root_dom)                        │
│           │     └─> Returns layout tree + positions            │
│           │                                                     │
│           ├─> Scan for IFrame nodes in layout tree             │
│           │                                                     │
│           ├─> For each IFrame node:                            │
│           │   ├─> Get bounds from layout tree                  │
│           │   ├─> Create IFrameCallbackInfo {                  │
│           │   │     bounds,                                    │
│           │   │     scroll_position,                           │
│           │   │     parent_dom_id                              │
│           │   │   }                                            │
│           │   ├─> Invoke callback(data, info)                  │
│           │   ├─> Get IFrameCallbackReturn {                   │
│           │   │     dom,                                       │
│           │   │     css,                                       │
│           │   │     scroll_size                                │
│           │   │   }                                            │
│           │   ├─> Assign child DomId                           │
│           │   ├─> Store IFrameState                            │
│           │   └─> Recursive: layout_document(child_dom)        │
│           │                                                     │
│           └─> Merge all display lists                          │
│                                                                 │
│  ✅ IFrame callbacks properly invoked                          │
│  ✅ Child DOMs generated and laid out                          │
│  ✅ Recursive layout handled                                   │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

## Complete Architecture Design

### 1. Core Data Structures

#### `IFrameState` (in LayoutWindow)
```rust
struct IFrameState {
    /// Unique ID for this IFrame's DOM
    child_dom_id: DomId,
    
    /// Last bounds when callback was invoked
    last_bounds: LogicalRect,
    
    /// Last scroll position when callback was invoked
    last_scroll: LogicalPosition,
    
    /// Content size reported by IFrame callback
    content_size: LogicalSize,
    
    /// Scroll size (may be different from content_size)
    /// Allows IFrame to control scrollbar behavior
    virtual_scroll_size: LogicalSize,
    
    /// Is this IFrame scrollable?
    scrollable: bool,
    
    /// Callback data and function pointer
    callback: IFrameCallback,
    callback_data: RefAny,
}
```

#### `ScrollState` (in ScrollManager)
```rust
struct ScrollState {
    /// Current scroll offset (animated value)
    current_offset: LogicalPosition,
    
    /// Target scroll offset (for animations)
    target_offset: Option<LogicalPosition>,
    
    /// Animation state
    animation: Option<ScrollAnimation>,
    
    /// Container bounds (visible area)
    container_bounds: LogicalRect,
    
    /// Content bounds (total scrollable area)
    content_bounds: LogicalRect,
    
    /// Is this an IFrame's scroll area?
    is_iframe: bool,
    
    /// If IFrame, link back to parent
    iframe_parent: Option<(DomId, NodeId)>,
    
    /// Last activity timestamp (for fading scrollbars)
    last_activity: Instant,
    
    /// Scroll sensitivity (for infinite DOM triggering)
    edge_threshold: f32, // Default: 100px from edge
}
```

### 2. Layout Flow with IFrames

```
Phase 1: Initial Layout
├─> layout_document(root_dom)
├─> Generate base layout tree
└─> Store in LayoutWindow.layout_cache

Phase 2: IFrame Detection & Invocation
├─> Scan layout tree for IFrame nodes
├─> For each IFrame:
│   ├─> Check if callback needs invocation:
│   │   ├─> First render? → YES
│   │   ├─> Bounds changed? → YES
│   │   ├─> Scroll position changed? → YES
│   │   ├─> Scroll near edge + infinite mode? → YES
│   │   └─> Otherwise → NO
│   │
│   ├─> If YES: Invoke callback
│   │   ├─> Create IFrameCallbackInfo
│   │   ├─> Call callback(data, info)
│   │   ├─> Get IFrameCallbackReturn
│   │   └─> Store/update IFrameState
│   │
│   └─> Recursive layout:
│       ├─> Create StyledDom from returned Dom + CSS
│       ├─> Assign unique child_dom_id
│       ├─> Call layout_document(child_dom, iframe_bounds)
│       └─> Store child layout result

Phase 3: Composite Display List Generation
├─> Generate root display list
├─> For each IFrame:
│   ├─> Insert PushClip (iframe bounds)
│   ├─> Insert PushScrollFrame (if scrollable)
│   ├─> Insert child display list (translated)
│   ├─> Insert scrollbar primitives (if needed)
│   ├─> Insert PopScrollFrame
│   └─> Insert PopClip
└─> Return final display list
```

### 3. Scrolling System Design

#### Scroll Manager Responsibilities

1. **State Tracking**
   - Track scroll position for every scrollable element
   - Track animation state (smooth scrolling)
   - Track content/container bounds
   - Detect edge proximity for infinite DOM triggering

2. **Animation System**
   ```
   User action (wheel, drag, API call)
     ↓
   ScrollManager::start_scroll_to(target, duration)
     ↓
   Per-frame: ScrollManager::tick(now)
     ├─> Interpolate current_offset
     ├─> Check if near edge (for infinite DOM)
     ├─> Return ScrollTickResult {
     │     needs_repaint: bool,
     │     iframes_to_reinvoke: Vec<(DomId, NodeId)>
     │   }
     └─> Caller triggers repa int and/or IFrame callbacks
   ```

3. **Edge Detection for Infinite DOM**
   ```rust
   fn check_scroll_edges(&self, scroll_state: &ScrollState) -> EdgeState {
       let threshold = scroll_state.edge_threshold;
       
       let near_top = scroll_state.current_offset.y < threshold;
       let near_bottom = (scroll_state.content_bounds.height 
           - scroll_state.current_offset.y 
           - scroll_state.container_bounds.height) < threshold;
           
       let near_left = scroll_state.current_offset.x < threshold;
       let near_right = (scroll_state.content_bounds.width 
           - scroll_state.current_offset.x 
           - scroll_state.container_bounds.width) < threshold;
           
       EdgeState { near_top, near_bottom, near_left, near_right }
   }
   ```

4. **IFrame Integration**
   ```
   Scroll event on IFrame content
     ↓
   ScrollManager detects near edge
     ↓
   Returns iframe_id in ScrollTickResult
     ↓
   LayoutWindow::handle_iframe_edge_scroll()
     ├─> Invoke IFrame callback again
     ├─> Append/prepend new content to DOM
     ├─> Adjust scroll position to maintain visual position
     └─> Trigger layout update
   ```

### 4. Cursor-Scroll Interaction

#### Scenario 1: Cursor Movement Beyond Visible Area

```
Text editor with cursor at line 100
Viewport shows lines 1-50
User presses Down arrow
  ↓
Cursor moves to line 101 (outside viewport)
  ↓
CursorPosition::ensure_visible()
  ├─> Calculate needed scroll offset
  ├─> ScrollManager::scroll_to(target, smooth=true)
  └─> Return Update::RefreshDomPartial
```

#### Scenario 2: Cursor in IFrame Near Edge

```
Cursor in IFrame at bottom of content
IFrame is scrollable with infinite mode
User presses Down arrow
  ↓
Cursor tries to move down
  ├─> Check if at edge of IFrame content
  ├─> If yes: trigger edge scroll
  │   ├─> IFrame callback invoked
  │   ├─> New DOM content appended
  │   └─> Cursor moves into new content
  └─> If no: normal movement
```

### 5. Display List Integration

#### Scrollbar Rendering

```rust
// In generate_display_list():

for iframe_state in layout_window.iframe_states.values() {
    if !iframe_state.scrollable {
        continue;
    }
    
    let scroll_state = scroll_manager.get(iframe_state.child_dom_id, root_node)?;
    
    // Calculate scrollbar bounds and positions
    let v_scrollbar_needed = scroll_state.content_bounds.height 
        > scroll_state.container_bounds.height;
    let h_scrollbar_needed = scroll_state.content_bounds.width 
        > scroll_state.container_bounds.width;
    
    if v_scrollbar_needed {
        let thumb_height = calculate_thumb_size(
            scroll_state.container_bounds.height,
            scroll_state.content_bounds.height
        );
        let thumb_position = calculate_thumb_position(
            scroll_state.current_offset.y,
            scroll_state.content_bounds.height,
            scroll_state.container_bounds.height
        );
        
        // Get opacity from scroll manager (for fading)
        let opacity = scroll_manager.get_scrollbar_opacity(
            iframe_state.child_dom_id,
            root_node,
            fade_duration
        );
        
        display_list.push(DisplayListItem::ScrollBar {
            bounds: LogicalRect {
                origin: LogicalPosition::new(
                    iframe_state.last_bounds.max_x() - SCROLLBAR_WIDTH,
                    iframe_state.last_bounds.min_y() + thumb_position
                ),
                size: LogicalSize::new(SCROLLBAR_WIDTH, thumb_height)
            },
            color: Color::rgba(128, 128, 128, opacity),
            orientation: ScrollbarOrientation::Vertical,
        });
    }
    
    // Same for horizontal scrollbar...
}
```

### 6. Infinite DOM Pattern

#### Use Case: Infinite Scrolling List

```rust
extern "C" fn infinite_list_callback(
    data: &mut RefAny,
    info: &mut IFrameCallbackInfo
) -> IFrameCallbackReturn {
    let state = data.downcast_mut::<InfiniteListState>().unwrap();
    
    // info contains:
    // - bounds: the IFrame's layout bounds
    // - scroll_offset: current scroll position
    // - scroll_size: last reported virtual scroll size
    // - is_edge_scroll: true if called due to edge proximity
    // - edge_state: which edges are near
    
    if info.is_edge_scroll {
        if info.edge_state.near_bottom {
            // Load more items at bottom
            state.load_more_items_bottom();
        }
        if info.edge_state.near_top {
            // Load more items at top
            state.load_more_items_top();
            
            // CRITICAL: Adjust scroll to maintain visual position
            let items_added = state.last_items_added_count;
            let item_height = 50.0;
            info.adjust_scroll_by(LogicalPosition::new(
                0.0,
                items_added as f32 * item_height
            ));
        }
    }
    
    // Generate DOM for currently loaded items
    let dom = state.generate_dom();
    let css = state.get_css();
    
    // Report virtual scroll size
    // (may be much larger than actual DOM content)
    let virtual_scroll_size = LogicalSize::new(
        info.bounds.width,
        state.total_items_count as f32 * 50.0 // Estimated height
    );
    
    IFrameCallbackReturn {
        dom,
        css,
        scroll_size: Some(virtual_scroll_size),
    }
}
```

#### Implementation Strategy

1. **Windowing Pattern**
   - IFrame keeps track of "visible window" of items (e.g., items 900-1000)
   - DOM contains only visible items + buffer (e.g., items 850-1050)
   - Virtual scroll size represents ALL items (e.g., 10,000 items)

2. **Edge Triggering**
   - When scroll nears edge, callback is invoked
   - IFrame loads more items
   - Scroll position is adjusted to maintain visual continuity

3. **Scroll Position Management**
   ```
   Before adding items at top:
     scroll_offset = 100px (user has scrolled down a bit)
     visible_range = items 900-1000
   
   Add 50 items at top:
     visible_range = items 850-1050
     Need to adjust scroll: 100px + (50 items * 50px/item) = 2600px
     
   Result: User sees the same content, but can now scroll up further
   ```

## Implementation Plan

### Phase 1: Fix IFrame Invocation (PRIORITY)
1. ✅ Document architecture (this file)
2. ⬜ Create `LayoutWindow::scan_for_iframes()` method
3. ⬜ Create `LayoutWindow::invoke_iframe_callback()` method
4. ⬜ Update `layout_and_generate_display_list()` to handle IFrames
5. ⬜ Fix test to use `LayoutWindow` instead of bare `layout_document`
6. ⬜ Verify IFrame callback is invoked (count == 1)

### Phase 2: Basic Scroll Manager
1. ⬜ Implement `ScrollState` struct
2. ⬜ Implement `ScrollManager` with basic set/get
3. ⬜ Integrate with `LayoutWindow`
4. ⬜ Add scroll offset tracking during layout
5. ⬜ Generate scroll primitives in display list

### Phase 3: Scroll Animations
1. ⬜ Add `ScrollAnimation` struct
2. ⬜ Implement `ScrollManager::tick()`
3. ⬜ Add smooth scrolling API
4. ⬜ Integrate with main event loop

### Phase 4: Scrollbar Rendering
1. ⬜ Add `DisplayListItem::ScrollBar`
2. ⬜ Calculate scrollbar bounds during display list generation
3. ⬜ Implement fading animation via opacity
4. ⬜ Add scrollbar interaction hit-testing

### Phase 5: IFrame-Scroll Integration
1. ⬜ Add edge detection to `ScrollManager`
2. ⬜ Add `is_edge_scroll` to `IFrameCallbackInfo`
3. ⬜ Implement IFrame re-invocation on edge scroll
4. ⬜ Add scroll position adjustment API

### Phase 6: Cursor-Scroll Integration
1. ⬜ Add `CursorPosition::ensure_visible()`
2. ⬜ Integrate with `ScrollManager`
3. ⬜ Handle cross-IFrame cursor navigation
4. ⬜ Implement auto-scroll on selection drag

### Phase 7: Infinite DOM Support
1. ⬜ Document windowing pattern
2. ⬜ Create example implementation
3. ⬜ Add `adjust_scroll_by()` to `IFrameCallbackInfo`
4. ⬜ Test with 10,000+ item list

## Testing Strategy

### Unit Tests
- ✅ RefAny memory safety (Miri)
- ⬜ ScrollManager state transitions
- ⬜ Edge detection logic
- ⬜ Scroll position calculations
- ⬜ Animation interpolation

### Integration Tests
- ⬜ IFrame callback invocation
- ⬜ Recursive layout
- ⬜ Scroll event propagation
- ⬜ Cursor-scroll interaction
- ⬜ Infinite DOM pattern

### Manual Testing
- ⬜ Smooth scrolling feels natural
- ⬜ Scrollbars fade correctly
- ⬜ Infinite list doesn't flicker
- ⬜ Cursor auto-scrolls properly

## Open Questions

1. **Performance**: How many IFrames can we handle before it's too slow?
   - Mitigation: Lazy layout (only layout visible IFrames)
   
2. **Nested IFrames**: Do we support IFrames within IFrames?
   - Answer: Yes, via recursive layout. Track depth to prevent infinite recursion.
   
3. **Scroll Synchronization**: Can multiple elements share a scroll position?
   - Answer: Yes, via `ScrollGroup` concept (future work)
   
4. **Virtual Scrolling**: Should we support CSS-like `scroll-snap`?
   - Answer: Yes, add `scroll_snap_type` and `scroll_snap_align` (future work)

## Success Criteria

1. ✅ All Miri tests pass (memory safety)
2. ⬜ IFrame callback test passes (count == 1)
3. ⬜ Infinite list with 10,000 items scrolls smoothly (60 FPS)
4. ⬜ Cursor navigation auto-scrolls correctly
5. ⬜ Scrollbars render and fade properly
6. ⬜ Edge-triggered IFrame callbacks work
7. ⬜ No flickering or visual discontinuities

## Next Immediate Actions

**RIGHT NOW**: Fix IFrame callback invocation
1. Read how `LayoutWindow` is structured
2. Add `scan_for_iframes()` method
3. Add `invoke_iframe_callback()` method  
4. Integrate into `layout_and_generate_display_list()`
5. Fix the test to use proper API
6. Verify callback count == 1

**THEN**: Start on Phase 2 (Basic Scroll Manager)
