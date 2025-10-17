# Text Cursor and Layout Improvements Plan

## Date: 2025-10-17

## Overview
This document outlines improvements to text cursor handling, layout robustness, and debugging capabilities.

## 1. Layout Robustness: Empty Font Cache Test

### Goal
Ensure layout doesn't crash when font cache is completely empty.

### Implementation
- **Test**: `test_layout_with_empty_font_cache()` in `solver3/tests.rs`
- **Expected Behavior**: Layout succeeds with fallback sizes, display list is generated (possibly empty or with default-sized boxes)
- **Location**: `layout/src/solver3/tests.rs`

### Status
✅ Already implemented - fallback mechanism in `sizing.rs` returns default sizes when fonts fail to load.

---

## 2. Debug Message Pattern for Cursor API

### Goal
Add structured debug messages to cursor operations instead of println! statements.

### Changes Needed

#### A. Add DebugMessages to Cursor Functions
**File**: `layout/src/text3/cache.rs`

Functions to instrument:
- `move_cursor_left()`
- `move_cursor_right()` 
- `move_cursor_up()`
- `move_cursor_down()`
- `move_cursor_to_line_start()`
- `move_cursor_to_line_end()`
- `move_cursor_to_start()`
- `move_cursor_to_end()`

Each function should:
1. Accept `&mut Option<Vec<String>>` parameter for debug messages
2. Log cursor state changes
3. Log boundary conditions
4. Log computed positions

#### B. Create Cursor Debug Message Types
```rust
pub enum CursorDebugMessage {
    MovementRequest { from: TextCursor, direction: Direction },
    BoundaryHit { boundary: TextBoundary, cursor: TextCursor },
    PositionCalculation { x: f32, y: f32, cluster: GraphemeClusterId },
    ClusterAdvance { from: usize, to: usize },
    LineChange { from_line: usize, to_line: usize },
}
```

---

## 3. Mouse Position to Text Cursor Mapping

### Goal
Convert mouse click coordinates to text cursor position.

### New Function
**File**: `layout/src/text3/cache.rs`

```rust
impl<T: ParsedFontTrait> UnifiedLayout<T> {
    /// Map a mouse position (x, y) to a text cursor position
    /// Returns the nearest cursor position to the click point
    pub fn hit_test_position(
        &self,
        x: f32,
        y: f32,
        debug: &mut Option<Vec<String>>
    ) -> Option<TextCursor> {
        // 1. Find line containing y coordinate
        // 2. Find positioned item nearest to x coordinate on that line
        // 3. Determine if click is before or after character center
        // 4. Return TextCursor at that position
    }
}
```

### Algorithm
1. Binary search lines by Y coordinate
2. Binary search items in line by X coordinate
3. Check if X is closer to item start or item end
4. Return cursor with appropriate affinity (Leading/Trailing)

---

## 4. Cursor Movement Bounds Checking

### Goal
Make cursor movement functions return `Result<TextCursor, CursorBoundsError>` instead of silently staying in place.

### New Error Type
**File**: `layout/src/text3/cache.rs`

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextBoundary {
    Top,
    Bottom,
    Start,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorBoundsError {
    pub boundary: TextBoundary,
    pub cursor: TextCursor, // Current cursor position
}
```

### Changes to Functions

#### Before
```rust
pub fn move_cursor_up(&self, cursor: TextCursor, goal_x: &mut Option<f32>) -> TextCursor
```

#### After
```rust
pub fn move_cursor_up(
    &self, 
    cursor: TextCursor, 
    goal_x: &mut Option<f32>,
    debug: &mut Option<Vec<String>>
) -> Result<TextCursor, CursorBoundsError>
```

### Boundary Rules
- **move_cursor_up()**: Return `Err(Top)` if already on first line
- **move_cursor_down()**: Return `Err(Bottom)` if already on last line
- **move_cursor_left()**: Return `Err(Start)` if at first character
- **move_cursor_right()**: Return `Err(End)` if at last character
- **move_cursor_to_line_start()**: Return `Err(Start)` if already at line start
- **move_cursor_to_line_end()**: Return `Err(End)` if already at line end

### Common Sense Checks
All cursor functions should:
1. Validate cursor is within valid range
2. Check boundary conditions BEFORE attempting movement
3. Return current cursor in error (so caller knows where cursor stayed)
4. Log decision in debug messages

---

## 5. Cross-Paragraph Cursor Navigation

### Goal
Enable cursor to jump between text nodes when reaching paragraph boundaries.

### New Functions on LayoutWindow
**File**: `layout/src/window.rs`

```rust
impl LayoutWindow {
    /// Find the next selectable text node after the current one
    pub fn find_next_text_node(
        &self,
        current_dom: DomId,
        current_node: NodeId,
        direction: CursorNavigationDirection,
    ) -> Option<(DomId, NodeId)> {
        // Iterate through nodes in DOM order
        // Check user-select CSS property
        // Return first selectable node
    }
    
    /// Try to move cursor across paragraphs
    pub fn move_cursor_cross_paragraph(
        &self,
        current_dom: DomId,
        current_node: NodeId,
        cursor: TextCursor,
        direction: CursorNavigationDirection,
        goal_x: Option<f32>,
    ) -> Result<(DomId, NodeId, TextCursor), NoCursorDestination> {
        // 1. Get current text layout
        // 2. Try to move cursor in current layout
        // 3. If boundary hit, find next/prev text node
        // 4. Initialize cursor in new node at appropriate position
        //    - For Up/Down: use goal_x to position horizontally
        //    - For Left: position at end of previous node
        //    - For Right: position at start of next node
    }
}

pub enum CursorNavigationDirection {
    Up,
    Down,
    Left,
    Right,
    LineStart,
    LineEnd,
    DocumentStart,
    DocumentEnd,
}

pub struct NoCursorDestination {
    pub reason: String,
}
```

### Integration with CallbackInfo
**File**: `layout/src/window.rs` or create `layout/src/cursor_api.rs`

```rust
/// High-level cursor API for use in callbacks
impl LayoutWindow {
    pub fn handle_cursor_movement(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        current_cursor: TextCursor,
        direction: CursorNavigationDirection,
        goal_x: &mut Option<f32>,
    ) -> CursorMovementResult {
        // This is the main entry point for cursor movement from callbacks
        // Handles both intra-paragraph and cross-paragraph movement
    }
}

pub enum CursorMovementResult {
    MovedWithinNode(TextCursor),
    MovedToNode { dom_id: DomId, node_id: NodeId, cursor: TextCursor },
    AtBoundary { boundary: TextBoundary, cursor: TextCursor },
}
```

---

## 6. User-Select CSS Property Check

### Goal
Check if a text node is selectable based on CSS `user-select` property.

### Implementation
**File**: `layout/src/solver3/getters.rs`

```rust
pub fn is_text_selectable(styled_dom: &StyledDom, node_id: NodeId) -> bool {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    let node_state = &styled_dom.styled_nodes.as_container()[node_id].state;
    
    // Check user-select CSS property
    styled_dom
        .css_property_cache
        .ptr
        .get_user_select(node_data, &node_id, node_state)
        .and_then(|us| us.get_property())
        .map(|us| us.inner != StyleUserSelect::None)
        .unwrap_or(true) // Default: text is selectable
}
```

---

## Implementation Order

### Phase 1: Foundation (High Priority)
1. ✅ Empty font cache test
2. Add `CursorBoundsError` type
3. Add debug messages parameter to cursor functions
4. Implement bounds checking in cursor functions

### Phase 2: Mouse Interaction
5. Implement `hit_test_position()`
6. Add tests for hit testing

### Phase 3: Cross-Paragraph Navigation
7. Implement `find_next_text_node()`
8. Add `user-select` property check
9. Implement `move_cursor_cross_paragraph()`
10. Create high-level `handle_cursor_movement()` API

### Phase 4: Testing & Refinement
11. Fix existing cursor tests
12. Add comprehensive cursor movement tests
13. Add cross-paragraph navigation tests
14. Document cursor API usage patterns

---

## Breaking Changes

### API Changes
- All cursor movement functions now return `Result<TextCursor, CursorBoundsError>`
- All cursor movement functions now accept `debug: &mut Option<Vec<String>>`

### Migration Guide
```rust
// Before
let new_cursor = layout.move_cursor_up(cursor, &mut goal_x);

// After
let new_cursor = match layout.move_cursor_up(cursor, &mut goal_x, &mut debug) {
    Ok(cursor) => cursor,
    Err(CursorBoundsError { boundary: TextBoundary::Top, cursor }) => {
        // Handle top boundary - try previous paragraph
        cursor
    }
};
```

---

## Testing Strategy

### Unit Tests
- Cursor movement within single line
- Cursor movement across lines
- Boundary detection
- Hit testing edge cases
- Empty layout handling

### Integration Tests
- Cross-paragraph navigation
- User-select property handling
- Mouse click to cursor conversion
- Goal-X preservation across paragraphs

### Property Tests
- Any valid cursor position can be reached by mouse click
- Cursor movement is reversible (up/down, left/right)
- Bounds checking is consistent

---

## Files to Modify

1. ✅ `layout/src/solver3/tests.rs` - Add empty cache test
2. `layout/src/text3/cache.rs` - Cursor functions, bounds checking, hit testing
3. `layout/src/text3/tests/` - Update existing tests, add new tests
4. `layout/src/window.rs` - Cross-paragraph navigation API
5. `layout/src/solver3/getters.rs` - User-select property getter
6. `layout/src/lib.rs` - Export new types

---

## Success Criteria

- ✅ Layout never crashes with empty font cache
- All cursor functions return Result with proper boundary handling
- Mouse clicks can be converted to cursor positions
- Cursor can navigate across paragraphs preserving horizontal position
- All tests pass (including fixed cursor tests)
- Debug messages provide clear insight into cursor operations
