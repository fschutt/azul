# Text Selection Architecture Planning Document

## Current State

The current text selection implementation has several limitations:

1. **Single-node selection only**: Selection is stored per-node and cannot span multiple nodes
2. **Selection cleared on node change**: When dragging from one node to another, the first node's selection is cleared
3. **No logical ordering**: Selection doesn't consider DOM order when crossing node boundaries
4. **Per-frame clearing**: `clear_selection()` is called every drag frame, causing flickering

## Research: How Browsers Handle Text Selection

### Browser Selection Model (W3C Selection API)

Browsers use a **Range-based selection model**:

```
Selection {
    anchorNode: Node,      // Where the selection started
    anchorOffset: u32,     // Character offset in anchor node
    focusNode: Node,       // Where the selection currently ends
    focusOffset: u32,      // Character offset in focus node
    isCollapsed: bool,     // true if anchor == focus (caret, no selection)
}
```

Key concepts:
- **Anchor**: The fixed point where the user started the selection (mousedown)
- **Focus**: The movable point where the selection currently ends (current mouse position)
- **Direction**: Selection can be forward (anchor before focus) or backward (focus before anchor)

### DOM Order vs Visual Order

Browsers select based on **DOM tree order**, not visual position:

```html
<div style="display: flex; flex-direction: row-reverse;">
    <span>First</span>   <!-- Visually on right -->
    <span>Second</span>  <!-- Visually on left -->
</div>
```

When selecting from left to right visually, browsers still select "Second" before "First" because that's the DOM order. However, the **highlight rectangles** are computed based on visual positions.

### Selection Highlighting Algorithm

For each text node in the selection range:
1. Determine if this node is fully or partially selected
2. If partially selected, compute which characters are in range
3. Get bounding rectangles for selected characters
4. Render highlight behind text

### Platform Differences

| Platform | Behavior |
|----------|----------|
| **macOS** | Selection follows DOM order; triple-click selects paragraph |
| **Windows** | Same as macOS; double-click selects word |
| **Linux/X11** | Primary selection (middle-click paste) in addition to clipboard |
| **iOS/Android** | Touch-based selection with handles; word-snapping |

### Key Insight: Logical Selection Rectangle

When the user drags to select:

1. **Anchor point**: Top-left and bottom-left of the first selected character
2. **Focus point**: Current mouse position
3. **Logical rectangle**: From anchor's line-start to focus position

```
Selection starts here (anchor)
       |
       v
    +--[=====]--------+
    |  ||             |  <- Line 1: selected from anchor to line end
    |  [=====]        |
    |     ||          |  <- Line 2: fully selected
    |  [=====]        |
    |       ||        |
    |  [====]--+      |  <- Line 3: selected from line start to focus
    +----------^------+
               |
         Focus (current mouse)
```

For **multi-node selection**, the same logic applies but spans DOM nodes:

```
Node A (large text "5"):
    [=]    <- Partially selected

Node B (button "Increase Counter"):
    [================]  <- Fully selected (between anchor and focus in DOM order)

Node C (some text after button):
    [===]--+  <- Partially selected up to focus
           |
           Focus
```

## Proposed Architecture

### 1. New Selection Data Structure

```rust
/// Represents an ongoing or completed text selection
pub struct TextSelection {
    /// The DOM where the selection started
    pub dom_id: DomId,
    
    /// Anchor: where the selection started (fixed during drag)
    pub anchor: SelectionAnchor,
    
    /// Focus: where the selection currently ends (moves during drag)
    pub focus: SelectionFocus,
    
    /// Cached list of affected nodes with their selection ranges
    /// Recomputed when focus changes
    pub affected_nodes: Vec<NodeSelectionRange>,
}

pub struct SelectionAnchor {
    /// The node where selection started
    pub node_id: NodeId,
    /// The IFC root containing this node (for text layout access)
    pub ifc_root_id: NodeId,
    /// Character offset in the text
    pub offset: u32,
    /// Visual bounds of the anchor character (for logical rect calculation)
    pub char_bounds: LogicalRect,
}

pub struct SelectionFocus {
    /// The node where selection currently ends (may differ from anchor)
    pub node_id: NodeId,
    /// The IFC root containing this node
    pub ifc_root_id: NodeId,
    /// Character offset in the text
    pub offset: u32,
    /// Current mouse position in viewport coordinates
    pub viewport_position: LogicalPosition,
}

pub struct NodeSelectionRange {
    pub node_id: NodeId,
    pub ifc_root_id: NodeId,
    /// Start character in this node (0 if fully selected from start)
    pub start_offset: u32,
    /// End character in this node (len if fully selected to end)
    pub end_offset: u32,
    /// Whether this is the anchor node, focus node, or in-between
    pub selection_type: NodeSelectionType,
}

pub enum NodeSelectionType {
    /// This is the anchor node (selection started here)
    Anchor,
    /// This is the focus node (selection currently ends here)
    Focus,
    /// This node is between anchor and focus (fully selected)
    InBetween,
    /// Anchor and focus are in same node
    AnchorAndFocus,
}
```

### 2. Selection Algorithm

#### On MouseDown (start selection):
1. Hit-test to find the text node and character under cursor
2. Create `SelectionAnchor` with node, offset, and character bounds
3. Set `focus = anchor` (collapsed selection / caret)
4. Clear any existing selection

#### On MouseMove (extend selection):
1. Hit-test to find node/character under current cursor
2. Update `focus` with new node and offset
3. **Compute affected nodes** (see below)
4. Update selection rendering

#### Computing Affected Nodes:
```rust
fn compute_affected_nodes(
    anchor: &SelectionAnchor,
    focus: &SelectionFocus,
    dom: &StyledDom,
    layout_tree: &LayoutTree,
) -> Vec<NodeSelectionRange> {
    // 1. Determine DOM order of anchor and focus
    let (start_node, end_node, is_forward) = if anchor.node_id <= focus.node_id {
        (anchor.node_id, focus.node_id, true)
    } else {
        (focus.node_id, anchor.node_id, false)
    };
    
    // 2. Collect all text nodes between start and end in DOM order
    let nodes_in_range = collect_text_nodes_in_range(dom, start_node, end_node);
    
    // 3. For each node, determine selection range
    nodes_in_range.iter().map(|node_id| {
        let text_len = get_text_length(dom, *node_id);
        
        if *node_id == anchor.node_id && *node_id == focus.node_id {
            // Same node: partial selection between anchor and focus
            let (start, end) = if is_forward {
                (anchor.offset, focus.offset)
            } else {
                (focus.offset, anchor.offset)
            };
            NodeSelectionRange {
                node_id: *node_id,
                start_offset: start.min(end),
                end_offset: start.max(end),
                selection_type: NodeSelectionType::AnchorAndFocus,
            }
        } else if *node_id == start_node {
            // First node: from anchor/focus offset to end
            NodeSelectionRange {
                node_id: *node_id,
                start_offset: if is_forward { anchor.offset } else { focus.offset },
                end_offset: text_len,
                selection_type: if is_forward { NodeSelectionType::Anchor } else { NodeSelectionType::Focus },
            }
        } else if *node_id == end_node {
            // Last node: from start to anchor/focus offset
            NodeSelectionRange {
                node_id: *node_id,
                start_offset: 0,
                end_offset: if is_forward { focus.offset } else { anchor.offset },
                selection_type: if is_forward { NodeSelectionType::Focus } else { NodeSelectionType::Anchor },
            }
        } else {
            // Middle node: fully selected
            NodeSelectionRange {
                node_id: *node_id,
                start_offset: 0,
                end_offset: text_len,
                selection_type: NodeSelectionType::InBetween,
            }
        }
    }).collect()
}
```

### 3. Logical Rectangle for Multi-Line Selection

When anchor and focus are in different nodes, use a "logical selection rectangle":

```rust
fn is_node_in_selection_rect(
    node_bounds: LogicalRect,
    anchor_char_bounds: LogicalRect,
    focus_position: LogicalPosition,
) -> bool {
    // Selection rectangle extends from:
    // - Top: min of anchor top and focus Y
    // - Bottom: max of anchor bottom and focus Y
    // - Left: 0 (start of line) for multi-line, or anchor X for same-line
    // - Right: viewport width for multi-line, or focus X for same-line
    
    let selection_top = anchor_char_bounds.origin.y.min(focus_position.y);
    let selection_bottom = (anchor_char_bounds.origin.y + anchor_char_bounds.size.height)
        .max(focus_position.y);
    
    // Check if node's vertical bounds intersect selection rect
    let node_top = node_bounds.origin.y;
    let node_bottom = node_bounds.origin.y + node_bounds.size.height;
    
    node_top < selection_bottom && node_bottom > selection_top
}
```

### 4. Changes Required

#### Files to Modify:

1. **`core/src/selection.rs`**
   - Add new `TextSelection`, `SelectionAnchor`, `SelectionFocus` structs
   - Modify `SelectionState` to hold `TextSelection` instead of per-node ranges

2. **`layout/src/managers/selection.rs`** (or create if doesn't exist)
   - Implement `compute_affected_nodes()` algorithm
   - Handle DOM traversal for finding nodes between anchor and focus

3. **`layout/src/window.rs`**
   - `process_mouse_click_for_selection`: Create anchor, don't clear existing selection yet
   - `process_mouse_drag_for_selection`: Update focus, recompute affected nodes
   - Add `process_mouse_up_for_selection`: Finalize selection or clear if needed

4. **`core/src/events.rs`**
   - Ensure MouseUp event triggers selection finalization
   - Track `drag_start_position` for selection anchor

5. **`layout/src/solver3/display_list.rs`**
   - `paint_selection_and_cursor`: Render selection rects for all affected nodes

### 5. Edge Cases to Handle

1. **Selection across IFC boundaries**: Anchor and focus in different IFC roots
2. **Non-text nodes in selection**: Buttons, images should be "selected" as units
3. **Hidden/collapsed nodes**: Skip nodes with `display: none` or `visibility: hidden`
4. **RTL text**: Selection direction may differ from visual order
5. **Vertical text**: `writing-mode: vertical-*` changes selection axis
6. **Nested inline elements**: `<span>Hel<b>lo</b> World</span>` - selection crosses tag boundaries

### 6. Implementation Order

1. **Phase 1**: New data structures in `core/src/selection.rs`
2. **Phase 2**: Basic anchor/focus tracking without multi-node support
3. **Phase 3**: DOM traversal for nodes between anchor and focus
4. **Phase 4**: Logical rectangle hit-testing for multi-line
5. **Phase 5**: Selection rendering for multiple nodes
6. **Phase 6**: Edge cases and polish

## Problem Summary for Next Chat

**Goal**: Implement browser-style text selection that can span multiple DOM nodes.

**Current Problem**: 
- Selection is per-node only
- Dragging to another node clears the first node's selection
- No concept of "anchor" (start) and "focus" (end) spanning multiple nodes

**Required Changes**:
1. Replace per-node selection with anchor/focus model
2. Compute list of affected nodes between anchor and focus in DOM order
3. For each affected node, determine partial or full selection
4. Render selection highlights for all affected nodes
5. Handle MouseDown (create anchor), MouseMove (update focus), MouseUp (finalize)

**Key Files**:
- `core/src/selection.rs` - Data structures
- `layout/src/window.rs` - Mouse event handling
- `layout/src/solver3/display_list.rs` - Selection rendering

**Reference**: See `TEXT_SELECTION_ARCHITECTURE.md` for full design.
