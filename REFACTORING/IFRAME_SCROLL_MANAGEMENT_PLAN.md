# IFrame Scroll Management and Conditional Re-invocation Plan

## Overview

This document outlines the architecture for managing IFrame callbacks with intelligent conditional re-invocation based on scroll position and viewport changes. The system integrates with the scroll manager to provide lazy loading and efficient rendering of large virtualized content.

## Core Concepts

### Dual Size Model

IFrame callbacks return **two distinct size/offset pairs**:

1. **Actual Scroll Bounds** (`scroll_size` + `scroll_offset`):
   - The rectangle of content that has **actually been rendered** by the IFrame
   - This is what's currently available in the DOM
   - Example: A table might render 20 rows × 5 columns

2. **Virtual Scroll Bounds** (`virtual_scroll_size` + `virtual_scroll_offset`):
   - The rectangle that the IFrame **pretends to have**
   - Used for scrollbar sizing and positioning
   - Example: The same table might pretend it has 1000 rows × 50 columns

### Purpose

This dual model enables:
- **Lazy loading**: Render only visible content
- **Virtualization**: Show large datasets without memory overhead
- **Smooth scrolling**: Scrollbars reflect the full virtual content size
- **Progressive rendering**: Load more content as user scrolls

## IFrameCallbackReturn Structure

### Current Structure (core/src/callbacks.rs)

```rust
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct IFrameCallbackReturn {
    /// The styled DOM that was rendered (the actual content)
    pub dom: StyledDom,
    
    /// Size of the actual rendered content rectangle
    pub scroll_size: LogicalSize,
    
    /// Offset of the actual rendered content within the virtual space
    pub scroll_offset: LogicalPosition,
    
    /// Size of the virtual content rectangle (for scrollbar sizing)
    pub virtual_scroll_size: LogicalSize,
    
    /// Offset of the virtual content (usually zero)
    pub virtual_scroll_offset: LogicalPosition,
}
```

### Proposed Enhancement

Add an optional DOM to allow callbacks to signal "no new content":

```rust
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct IFrameCallbackReturn {
    /// The styled DOM that was rendered, or None if no update is needed
    /// When None, the layout engine will keep using the previous DOM
    pub dom: OptionStyledDom,  // CHANGED: Was StyledDom, now OptionStyledDom
    
    /// Size of the actual rendered content rectangle
    pub scroll_size: LogicalSize,
    
    /// Offset of the actual rendered content within the virtual space
    pub scroll_offset: LogicalPosition,
    
    /// Size of the virtual content rectangle (for scrollbar sizing)
    pub virtual_scroll_size: LogicalSize,
    
    /// Offset of the virtual content (usually zero)
    pub virtual_scroll_offset: LogicalPosition,
}
```

**Rationale**: The callback might determine that no new content is needed (e.g., user scrolled down but we've already rendered ahead). Returning `None` signals "keep using the current DOM."

## Conditional Re-invocation Rules

The IFrame callback should be re-invoked **sparingly** to avoid performance issues. The rules are designed to call it only when new content would be visible.

### Rule 1: Initial Invocation

- **When**: IFrame node first appears in layout
- **Reason**: Need initial content to display

### Rule 2: Parent DOM Recreated

- **When**: The parent DOM containing the IFrame is recreated (not just re-laid-out)
- **Reason**: Layout cache invalidated, need fresh content

### Rule 3: Window Resize (Expansion Only, Once)

- **When**: Window resizes such that the IFrame's available bounds **expand** beyond the current `scroll_size`
- **Constraint**: Only invoke **once per expansion** - when the frame that was previously fully covered becomes uncovered
- **Non-trigger**: Window resizing to be **smaller** does NOT trigger re-invocation
- **Non-trigger**: If the expanded area is still within the existing `scroll_size`, do NOT re-invoke

**Example**:
```
Frame 0: IFrame bounds = 800×600, scroll_size = 800×600 (perfectly covered)
Frame 1: Window resizes to 1000×700 (larger)
  -> IFrame bounds = 1000×700
  -> Bounds no longer covered by scroll_size (800×600)
  -> RE-INVOKE callback once
  
Frame 2: Window resizes to 1100×800 (even larger)
  -> If callback returned scroll_size = 1100×800, fully covered again
  -> Do NOT re-invoke (content covers new bounds)
  -> If callback returned scroll_size = 1000×700, not fully covered
  -> RE-INVOKE again (new uncovered area)
```

### Rule 4: Scroll Near Edge

- **When**: User scrolls and the visible area approaches the edge of the `scroll_size` rectangle
- **Threshold**: Within `SCROLL_THRESHOLD` pixels of the edge (e.g., 200px)
- **Constraint**: Only invoke **once per edge approach** - use a flag to prevent repeated calls
- **Reset**: Flag is reset when scroll moves away from edge or callback returns expanded content

**Example**:
```
scroll_size = 1000×2000 (width × height)
Container = 800×600
Current scroll_offset = 0×0

User scrolls to scroll_offset = 0×1500:
  -> Bottom edge at 1500 + 600 = 2100
  -> Within 200px of scroll_size.height (2000)
  -> RE-INVOKE callback to load more content

Callback returns:
  -> New scroll_size = 1000×4000 (doubled)
  -> User continues scrolling without re-invoke until near new edge
```

### Rule 5: Programmatic Scroll Position Change

- **When**: `set_scroll_position()` is called programmatically and moves visible area beyond `scroll_size`
- **Reason**: API call might scroll to unrendered area
- **Same constraints** as Rule 4 (threshold and once-per-edge)

## State Management

### IFrameState Structure (layout/src/window.rs)

```rust
/// Tracks the state of an IFrame for conditional re-invocation
#[derive(Debug, Clone)]
struct IFrameState {
    /// The bounds of the iframe node at last callback invocation
    last_invoked_bounds: LogicalRect,
    
    /// The scroll offset at last callback invocation
    last_invoked_scroll_offset: LogicalPosition,
    
    /// The DomId assigned to this iframe's content
    dom_id: DomId,
    
    /// The last returned scroll_size (actual rendered content size)
    current_scroll_size: LogicalSize,
    
    /// The last returned virtual_scroll_size (for scrollbar)
    current_virtual_scroll_size: LogicalSize,
    
    /// The cached DOM from the last successful invocation
    cached_dom: StyledDom,
    
    /// Flag: Have we already invoked for current bounds expansion?
    invoked_for_current_expansion: bool,
    
    /// Flag: Have we already invoked for current scroll edge approach?
    invoked_for_current_edge: bool,
    
    /// Which edge(s) we last invoked for (to reset flags when scrolling away)
    last_edge_triggered: EdgeFlags,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct EdgeFlags {
    top: bool,
    bottom: bool,
    left: bool,
    right: bool,
}
```

### Integration with ScrollManager

The existing `ScrollManager` (from `layout/src/scroll.rs`) already tracks:
- Current scroll offsets for all nodes
- Scroll animations
- Scrollbar opacity (for fading)

**Integration points**:

1. **After layout**: Update `ScrollManager` with IFrame's container and content sizes
2. **On scroll event**: Check if scroll position triggers IFrame re-invocation
3. **On `tick()`**: `ScrollManager` returns `iframes_to_update` vector
4. **Layout loop**: Re-invoke IFrame callbacks for nodes in `iframes_to_update`

## Implementation Plan

### Phase 1: Update Core Types

**Files to modify**:
- `core/src/callbacks.rs` - Change `dom: StyledDom` to `dom: OptionStyledDom` in `IFrameCallbackReturn`
- `core/src/dom.rs` - Add `OptionStyledDom` type if not already present

**API changes**:
```rust
// Before
impl IFrameCallbackReturn {
    pub fn new(dom: StyledDom, ...) -> Self { ... }
}

// After
impl IFrameCallbackReturn {
    /// Returns new content to be rendered
    pub fn with_dom(dom: StyledDom, ...) -> Self {
        Self {
            dom: OptionStyledDom::Some(dom),
            ...
        }
    }
    
    /// Signals that current content should be kept (no update)
    pub fn keep_current(...) -> Self {
        Self {
            dom: OptionStyledDom::None,
            ...
        }
    }
}
```

### Phase 2: Enhance IFrameState Tracking

**Files to modify**:
- `layout/src/window.rs` - Enhance `IFrameState` structure
- Add methods for checking re-invocation conditions

**New methods**:
```rust
impl LayoutWindow {
    /// Check if an IFrame needs re-invocation based on current conditions
    fn should_reinvoke_iframe(
        &self,
        iframe_node_id: (DomId, NodeId),
        current_bounds: LogicalRect,
        current_scroll: LogicalPosition,
    ) -> bool {
        // Implement Rules 3, 4, 5 here
    }
    
    /// Update IFrame state after successful invocation
    fn update_iframe_state(
        &mut self,
        iframe_node_id: (DomId, NodeId),
        return_value: &IFrameCallbackReturn,
        invoked_bounds: LogicalRect,
        invoked_scroll: LogicalPosition,
    ) {
        // Store new state, reset flags
    }
}
```

### Phase 3: Integrate with Scroll Manager

**Files to modify**:
- `layout/src/scroll.rs` - Add IFrame edge detection

**Enhancements**:
```rust
impl ScrollManager {
    /// Check if scrolling has approached the edge of an IFrame's content
    pub fn check_iframe_scroll_edges(
        &self,
        dom_id: DomId,
        node_id: NodeId,
        scroll_size: LogicalSize,
        threshold: f32,
    ) -> EdgeFlags {
        // Return which edges are within threshold
    }
}

impl ScrollState {
    /// Enhanced version with IFrame-specific checks
    fn is_scrolled_near_edge(&self, threshold: f32) -> EdgeFlags {
        EdgeFlags {
            top: self.current_offset.y < threshold,
            bottom: self.current_offset.y + self.container_rect.size.height 
                   >= self.content_rect.size.height - threshold,
            left: self.current_offset.x < threshold,
            right: self.current_offset.x + self.container_rect.size.width
                  >= self.content_rect.size.width - threshold,
        }
    }
}
```

### Phase 4: Update Layout Loop

**Files to modify**:
- `layout/src/solver3/mod.rs` - Main layout loop
- `layout/src/solver3/cache.rs` - IFrame invocation logic

**Layout loop changes**:
```rust
pub fn layout_document(...) -> Result<DisplayList, LayoutError> {
    loop {
        // ... existing layout passes ...
        
        // NEW: Check for IFrame re-invocations needed
        let iframes_to_invoke = layout_window.check_iframe_reinvocation_conditions();
        
        if !iframes_to_invoke.is_empty() {
            for (dom_id, node_id) in iframes_to_invoke {
                // Re-invoke IFrame callback
                let return_value = invoke_iframe_callback(...);
                
                // Handle OptionStyledDom
                match return_value.dom {
                    OptionStyledDom::Some(new_dom) => {
                        // Replace cached DOM, mark subtree dirty
                        layout_window.update_iframe_dom(dom_id, node_id, new_dom);
                    }
                    OptionStyledDom::None => {
                        // Keep current DOM, just update scroll bounds
                        layout_window.update_iframe_scroll_bounds(dom_id, node_id, &return_value);
                    }
                }
            }
            
            // Restart layout with updated IFrame content
            continue;
        }
        
        break; // No more IFrames need updating
    }
    
    // ... display list generation ...
}
```

### Phase 5: Update Tests

**Files to modify**:
- `layout/src/solver3/tests.rs` - Update existing IFrame tests

**Test scenarios to add**:
1. IFrame returns `OptionStyledDom::None` (keep current)
2. Window resize triggers re-invocation (expansion)
3. Window resize does NOT trigger (shrinking)
4. Scroll near edge triggers re-invocation
5. Scroll away from edge does not trigger again
6. Multiple IFrames with independent scroll states

### Phase 6: Documentation

**Files to create/update**:
- `REFACTORING/IFRAME_SCROLL_MANAGEMENT_PLAN.md` (this file)
- `core/src/callbacks.rs` - Add detailed doc comments on `IFrameCallbackReturn`

**Documentation to add**:

```rust
/// Return value for an IFrame rendering callback.
///
/// # Dual Size Model
///
/// IFrame callbacks return two size/offset pairs:
///
/// ## Actual Content (`scroll_size` + `scroll_offset`)
/// 
/// The size and position of content that has **actually been rendered**. This is
/// the content currently present in the returned DOM.
///
/// Example: A table view might render only 20 visible rows.
///
/// ## Virtual Content (`virtual_scroll_size` + `virtual_scroll_offset`)
///
/// The size and position of content that the IFrame **pretends to have**. This is
/// used for scrollbar sizing and positioning, allowing the scrollbar to represent
/// the full dataset even when only a subset is rendered.
///
/// Example: The same table might pretend to have 1000 rows.
///
/// # Conditional Re-invocation
///
/// The IFrame callback will be invoked in these situations:
///
/// 1. **Initial render** - First time the IFrame appears
/// 2. **Parent DOM recreated** - The parent DOM was rebuilt from scratch
/// 3. **Window resize (expansion)** - Window grows and IFrame bounds exceed `scroll_size`
///    - Only triggers ONCE per expansion (when bounds become uncovered)
///    - Does NOT trigger when window shrinks
/// 4. **Scroll near edge** - User scrolls within threshold of content edge
///    - Only triggers ONCE per edge approach
///    - Threshold is typically 200px from edge
/// 5. **Programmatic scroll** - `set_scroll_position()` scrolls beyond rendered content
///
/// # Optimization: Returning None
///
/// If the callback determines that no new content is needed (e.g., sufficient content
/// has already been rendered ahead of the scroll position), it can return
/// `OptionStyledDom::None` for the `dom` field. This signals the layout engine to
/// keep using the current DOM.
///
/// ```rust
/// fn my_iframe_callback(data: &mut MyData, info: &mut IFrameCallbackInfo) -> IFrameCallbackReturn {
///     let current_scroll = info.scroll_offset;
///     
///     // Check if we've already rendered content that covers this scroll position
///     if data.already_rendered_area_covers(current_scroll, info.bounds.size) {
///         return IFrameCallbackReturn {
///             dom: OptionStyledDom::None,  // Keep current DOM
///             scroll_size: data.current_scroll_size,
///             scroll_offset: data.current_scroll_offset,
///             virtual_scroll_size: data.virtual_size,
///             virtual_scroll_offset: LogicalPosition::zero(),
///         };
///     }
///     
///     // Otherwise, render new content
///     let new_dom = data.render_more_content(...);
///     IFrameCallbackReturn {
///         dom: OptionStyledDom::Some(new_dom),
///         ...
///     }
/// }
/// ```
///
/// # Example: Virtualized Table
///
/// ```rust
/// struct TableData {
///     total_rows: usize,      // 1000 rows in full dataset
///     row_height: f32,        // 30px per row
///     visible_rows: Vec<Row>, // Currently rendered rows (e.g., rows 0-29)
///     first_visible_row: usize, // Index of first rendered row
/// }
///
/// fn table_iframe_callback(data: &mut TableData, info: &mut IFrameCallbackInfo) -> IFrameCallbackReturn {
///     let container_height = info.bounds.size.height;
///     let scroll_y = info.scroll_offset.y;
///     
///     // Calculate which rows should be visible
///     let first_row = (scroll_y / data.row_height) as usize;
///     let visible_count = (container_height / data.row_height).ceil() as usize + 2; // +2 for buffer
///     
///     // Check if we need to render new rows
///     if first_row >= data.first_visible_row 
///        && first_row + visible_count <= data.first_visible_row + data.visible_rows.len() {
///         // Current content covers the visible area - no update needed
///         return IFrameCallbackReturn {
///             dom: OptionStyledDom::None,
///             scroll_size: LogicalSize::new(
///                 info.bounds.size.width,
///                 data.visible_rows.len() as f32 * data.row_height,
///             ),
///             scroll_offset: LogicalPosition::new(
///                 0.0,
///                 data.first_visible_row as f32 * data.row_height,
///             ),
///             virtual_scroll_size: LogicalSize::new(
///                 info.bounds.size.width,
///                 data.total_rows as f32 * data.row_height,
///             ),
///             virtual_scroll_offset: LogicalPosition::zero(),
///         };
///     }
///     
///     // Render new rows
///     data.visible_rows = data.fetch_rows(first_row, visible_count);
///     data.first_visible_row = first_row;
///     
///     let dom = Dom::body()
///         .with_children(
///             data.visible_rows.iter().map(|row| {
///                 Dom::div()
///                     .with_child(Dom::text(row.text.clone()))
///                     .with_inline_css_props(css_property_vec![
///                         ("height", format!("{}px", data.row_height)),
///                     ])
///             }).collect()
///         );
///     
///     IFrameCallbackReturn {
///         dom: OptionStyledDom::Some(dom.style(Css::empty())),
///         scroll_size: LogicalSize::new(
///             info.bounds.size.width,
///             data.visible_rows.len() as f32 * data.row_height,
///         ),
///         scroll_offset: LogicalPosition::new(
///             0.0,
///             first_row as f32 * data.row_height,
///         ),
///         virtual_scroll_size: LogicalSize::new(
///             info.bounds.size.width,
///             data.total_rows as f32 * data.row_height,
///         ),
///         virtual_scroll_offset: LogicalPosition::zero(),
///     }
/// }
/// ```
///
/// In this example:
/// - `scroll_size` is the size of the 20-30 rendered rows (~600-900px tall)
/// - `scroll_offset` is where those rows start in the virtual space (e.g., y=300 if showing rows 10-30)
/// - `virtual_scroll_size` is the size if all 1000 rows were rendered (30,000px tall)
/// - The scrollbar shows the full 30,000px range, but only 600-900px of content is actually rendered
pub struct IFrameCallbackReturn { ... }
```

## Integration with Scrolling System

### Scroll Event Flow

```
User scrolls IFrame
    ↓
ScrollManager.tick() or scroll_by()
    ↓
Check: Is scroll near edge of scroll_size?
    ↓
Yes → Add to iframes_to_update
    ↓
Layout loop detects iframes_to_update
    ↓
Invoke IFrame callback with new bounds/scroll
    ↓
Callback returns new content (or None)
    ↓
Update cached DOM (if new) and scroll bounds
    ↓
Mark IFrame subtree dirty
    ↓
Continue layout loop
    ↓
Generate display list with updated content
```

### Window Resize Flow

```
Window resizes
    ↓
Layout loop starts with new viewport
    ↓
IFrame node layout calculated
    ↓
Check: Do new bounds exceed last scroll_size?
    ↓
Yes + not invoked for this expansion yet?
    ↓
Mark IFrame for re-invocation
    ↓
Invoke callback with new bounds
    ↓
Update scroll_size and reset expansion flag
    ↓
Continue layout
```

## Performance Considerations

### Preventing Infinite Loops

**Problem**: Callback returns content that triggers immediate re-invocation.

**Solution**: Track `last_invoked_bounds` and `last_invoked_scroll` - only re-invoke if conditions have meaningfully changed (e.g., scrolled by more than threshold).

### Preventing Repeated Invocations

**Problem**: Window keeps growing or user keeps scrolling, causing repeated callbacks every frame.

**Solution**: Use `invoked_for_current_expansion` and `invoked_for_current_edge` flags. Reset only when:
- Callback returns expanded content that covers new area
- Scroll moves away from edge by more than threshold

### Batch Updates

**Problem**: Multiple IFrames all need re-invocation in same frame.

**Solution**: Collect all IFrames needing updates, invoke all callbacks, then do single layout pass with all new content.

## Testing Strategy

### Unit Tests

1. **Bounds expansion detection**
   - Window grows: bounds exceed scroll_size → should reinvoke
   - Window shrinks: bounds smaller → should NOT reinvoke
   - Window grows but within scroll_size → should NOT reinvoke

2. **Scroll edge detection**
   - Scroll within threshold of bottom → should reinvoke
   - Scroll away from edge → flag resets
   - Scroll back to edge → should reinvoke again

3. **OptionStyledDom handling**
   - Callback returns Some(dom) → DOM updated
   - Callback returns None → DOM kept, bounds updated

### Integration Tests

1. **Large table virtualization**
   - 10,000 row table
   - Only 50 rows rendered at a time
   - Scroll smoothly through entire table
   - Verify callback invoked only when needed

2. **Multiple IFrames**
   - 3 independent IFrames with different content
   - Scroll one → only that IFrame reinvokes
   - Resize window → all IFrames check conditions independently

3. **Nested IFrames**
   - IFrame contains another IFrame
   - Scroll outer → inner checks its conditions
   - Scroll inner → only inner reinvokes

## Migration Path for Existing Code

### Breaking Change

Changing `dom: StyledDom` to `dom: OptionStyledDom` is a breaking API change.

### Migration Steps

1. **Add OptionStyledDom to core types**
2. **Update IFrameCallbackReturn**
3. **Provide migration helpers**:

```rust
impl IFrameCallbackReturn {
    /// DEPRECATED: Use `with_dom()` instead
    #[deprecated(since = "1.0.0", note = "Use with_dom() for new content or keep_current() for no update")]
    pub fn new(dom: StyledDom, ...) -> Self {
        Self::with_dom(dom, ...)
    }
}
```

4. **Update all examples and tests**
5. **Document migration in CHANGELOG**

## Future Enhancements

### Predictive Loading

Add `scroll_velocity` to `IFrameCallbackInfo`:
- Callback can predict scroll direction
- Pre-render content before user reaches edge
- Smoother experience for fast scrolling

### Bidirectional Virtualization

Current plan focuses on vertical scrolling. Enhancement:
- Track horizontal scroll edges too
- Support 2D virtualization (large grids)
- Adjust thresholds per axis

### Priority Hints

Add priority levels to IFrame invocations:
- High priority: User actively scrolling
- Low priority: Pre-loading off-screen content
- Layout engine can defer low-priority invocations

## Summary

This architecture provides:

✅ **Efficient lazy loading** - Only render visible content
✅ **Smooth scrolling** - Virtual sizes provide accurate scrollbars
✅ **Smart re-invocation** - Callbacks only when truly needed
✅ **Flexible control** - Callbacks can decline updates with None
✅ **Integration with scroll system** - Works with existing ScrollManager
✅ **Performance** - Prevents unnecessary DOM rebuilds

The system is designed to handle large datasets efficiently while providing a smooth user experience, making it suitable for virtualized tables, infinite scroll feeds, and other data-heavy UIs.
