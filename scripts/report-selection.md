# Text Selection Architecture Report

## Problem Statement

The current text selection implementation has an architectural issue: **the `inline_layout_result` is stored on the IFC root (container) node, but the hit-test tags are assigned based on DOM node relationships, not layout tree relationships.**

This creates a mismatch:
- Text selection needs to map pixel coordinates → logical cursor position
- The `UnifiedLayout` (containing `hittest_cursor()`) is stored on the container `<p>` node
- But hit-test tags reference the DOM node ID of the container
- The actual text content is in `::text` nodes (DOM children), which don't have `inline_layout_result`

## Current Architecture

### Inline Formatting Context (IFC) Layout

```
DOM Tree:                    Layout Tree:
┌─────────────────┐         ┌──────────────────────────┐
│ <p> (node 1)    │    →    │ LayoutNode (IFC root)    │
├─────────────────┤         │   dom_node_id: Some(1)   │
│ ::text (node 2) │    →    │   inline_layout_result:  │
│ "Hello world"   │         │     UnifiedLayout {...}  │◄── Layout stored here
└─────────────────┘         │   children: [...]        │
                            └──────────────────────────┘
                                      │
                            ┌─────────┴──────────┐
                            ▼                    ▼
                    ┌───────────────┐   ┌───────────────┐
                    │ LayoutNode    │   │ (maybe more   │
                    │ dom_node_id:2 │   │  inline items)│
                    │ inline_layout │   └───────────────┘
                    │ _result: None │◄── No layout here!
                    └───────────────┘
```

### Why This Design Exists

The `UnifiedLayout` represents the **complete inline formatting context**, including:
- Multiple text runs from different DOM nodes
- Inline-block elements
- Images
- Markers (list bullets)

It cannot be split per-DOM-node because:
1. **Line breaking is holistic**: Text from `<span>A</span><span>B</span>` may break mid-word
2. **Bidi reordering**: Visual order differs from DOM order in RTL/mixed text
3. **Inline-blocks**: A `<span style="display:inline-block">` participates in the same IFC

### The Hit-Test Problem

When a click occurs:

1. **WebRender Hit-Test** returns `(tag_id, point_relative_to_item)`
2. Tag maps to DOM node ID via `TagIdToNodeIdMapping`
3. We need to find the `UnifiedLayout` to call `hittest_cursor(point)`
4. But the layout is on the **parent** node, not the tagged node

Currently, the code tags the **container** node (that has text children), which is correct for WebRender's perspective, but:
- The container's `used_size` may be 0x0 (text doesn't contribute to box size directly)
- We had to add fallback bounds computation from `inline_layout_result.bounds()`

## ContentIndex Architecture

`text3` already has a mechanism to track where each shaped item came from:

```rust
/// A stable, logical pointer to an item within the original `InlineContent` array.
pub struct ContentIndex {
    /// The index of the `InlineContent` run in the original input array.
    pub run_index: u32,
    /// The byte index of the character or item *within* that run's string.
    pub item_index: u32,
}

/// Each shaped cluster references its source
pub struct ShapedCluster {
    pub source_content_index: ContentIndex,
    // ...
}
```

This `ContentIndex` is used during layout to map back to the original `InlineContent` items.

## Proposed Solution: IFC-to-DOM Mapping Table

### New Data Structures

```rust
/// Unique identifier for an Inline Formatting Context
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct IfcId(pub u32);

/// Maps IFC runs to their source DOM nodes
pub struct IfcDomMapping {
    /// The DOM node that is the IFC root (the container)
    pub ifc_root_dom_id: NodeId,
    /// Map from run_index to the DOM node ID of the original text/inline node
    pub run_to_dom: Vec<Option<NodeId>>,
}

/// Extended LayoutNode with IFC tracking
pub struct LayoutNode {
    // ... existing fields ...
    
    /// If this node participates in an IFC (is inline content),
    /// this stores the IFC ID and which run within that IFC this node represents.
    pub ifc_membership: Option<IfcMembership>,
}

pub struct IfcMembership {
    /// Which IFC this node's content was laid out in
    pub ifc_id: IfcId,
    /// Which run index within the IFC corresponds to this node's text
    pub run_index: u32,
}
```

### Modified Layout Flow

During `collect_and_measure_inline_content`:

```rust
fn collect_and_measure_inline_content(...) -> Result<(Vec<InlineContent>, IfcDomMapping)> {
    let ifc_id = IfcId::next(); // Generate unique IFC ID
    let mut run_to_dom = Vec::new();
    
    for child in children {
        if let NodeType::Text(_) = node_data.get_node_type() {
            run_to_dom.push(Some(child_dom_id));
            // Also store on the child LayoutNode:
            child_layout_node.ifc_membership = Some(IfcMembership {
                ifc_id,
                run_index: run_to_dom.len() as u32 - 1,
            });
        }
    }
    
    Ok((content, IfcDomMapping { ifc_root_dom_id, run_to_dom }))
}
```

### Modified Selection Hit-Test

```rust
pub fn process_mouse_click_for_selection(&mut self, position: LogicalPosition, ...) {
    // 1. Get hit-test from WebRender (via HoverManager)
    let (tag_id, point_relative_to_item) = hover_manager.get_hit_at(position)?;
    
    // 2. Map tag to DOM node
    let hit_dom_id = tag_mapping.get(tag_id)?;
    
    // 3. Find the LayoutNode for this DOM ID
    let layout_node_idx = layout_tree.dom_to_layout.get(&hit_dom_id)?[0];
    let layout_node = layout_tree.get(layout_node_idx)?;
    
    // 4. Get the IFC and its UnifiedLayout
    let (ifc_layout, run_filter) = if let Some(cached) = &layout_node.inline_layout_result {
        // This is an IFC root - use whole layout
        (cached.layout.clone(), None)
    } else if let Some(membership) = &layout_node.ifc_membership {
        // This node is INSIDE an IFC - find the IFC root
        let ifc_root_idx = find_ifc_root(layout_tree, membership.ifc_id)?;
        let ifc_root = layout_tree.get(ifc_root_idx)?;
        let layout = ifc_root.inline_layout_result.as_ref()?.layout.clone();
        (layout, Some(membership.run_index))
    } else {
        return None; // No text content
    };
    
    // 5. Hit-test the cursor within the IFC layout
    let cursor = if let Some(run_idx) = run_filter {
        // Only consider items from this specific run
        ifc_layout.hittest_cursor_in_run(point_relative_to_item, run_idx)
    } else {
        ifc_layout.hittest_cursor(point_relative_to_item)
    }?;
    
    // 6. Create selection
    // ...
}
```

## Alternative: Store Layout Reference on Text Nodes

Instead of copying the layout, store a reference:

```rust
pub struct LayoutNode {
    // For IFC roots:
    pub inline_layout_result: Option<CachedInlineLayout>,
    
    // For text nodes inside an IFC:
    pub inline_layout_ref: Option<InlineLayoutRef>,
}

pub struct InlineLayoutRef {
    /// Index of the IFC root LayoutNode that has the actual layout
    pub ifc_root_index: usize,
    /// Which run within the IFC this text corresponds to
    pub run_index: u32,
    /// Byte range within the run (for partial text nodes if needed)
    pub byte_range: Range<u32>,
}
```

This avoids duplicating the layout and provides a clear path from text node → IFC layout.

## Impact on Selection Rendering

### Selection Rectangles

When rendering selection, we need to:

1. Get the `SelectionRange` (start cursor, end cursor)
2. Find all affected IFCs (a selection can span multiple paragraphs)
3. For each IFC, calculate visual rectangles for the selected text

```rust
impl UnifiedLayout {
    pub fn get_selection_rectangles(&self, range: &SelectionRange) -> Vec<Rect> {
        let mut rects = Vec::new();
        
        for item in &self.items {
            if let ShapedItem::Cluster(cluster) = &item.item {
                if is_cluster_in_range(cluster, range) {
                    rects.push(Rect {
                        x: item.position.x,
                        y: item.position.y,
                        width: cluster.advance,
                        height: cluster.line_height,
                    });
                }
            }
        }
        
        // Merge adjacent rectangles on same line
        merge_horizontal_rects(&mut rects);
        rects
    }
}
```

### Display List Integration

Selection rectangles should be rendered:
- **Behind** the text (as background highlight)
- With the `::selection` pseudo-element color (default: system highlight color)

```rust
// In display_list.rs
if let Some(selection) = get_selection_for_node(dom_id) {
    for rect in ifc_layout.get_selection_rectangles(&selection) {
        // Translate to absolute coordinates
        let abs_rect = rect.translate(node_absolute_pos);
        push_rect(display_list, abs_rect, selection_background_color);
    }
}
// Then push the text itself
push_text(display_list, ...);
```

## CSS `::selection` Pseudo-Element

### Current Support

The `::selection` pseudo-element should support:
- `color`: Text color when selected
- `background-color`: Selection highlight color
- `text-shadow`: Shadow on selected text (rare)

### Implementation

```rust
/// In styled_dom.rs or prop_cache.rs
pub fn get_selection_style(&self, node_id: NodeId) -> SelectionStyle {
    // Check if there's a ::selection rule for this node
    if let Some(style) = self.pseudo_element_styles.get(&(node_id, PseudoElement::Selection)) {
        SelectionStyle {
            color: style.color.unwrap_or(system_selection_text_color()),
            background: style.background_color.unwrap_or(system_selection_background()),
        }
    } else {
        // Default system colors
        SelectionStyle {
            color: system_selection_text_color(),
            background: system_selection_background(),
        }
    }
}
```

## Non-Text IFC Items

An IFC can contain items that are NOT text:

| Item Type | Selectable? | Notes |
|-----------|-------------|-------|
| Text runs | Yes | Primary selection target |
| Inline-blocks | No* | Selected as atomic unit |
| Images | No* | Selected as atomic unit |
| Markers (::marker) | No | List bullets shouldn't be selected |
| Line breaks | No | Control characters |

*Inline-blocks and images can be "selected" in the sense that they're included in a copy/paste, but you can't place a cursor inside them.

### Implementation Consideration

When hit-testing, we should:
1. First check if the hit item is a `ShapedItem::Cluster` (text)
2. If it's an `Object` (inline-block), treat it as an atomic unit
3. Allow selection to span across objects but cursor can only be before/after

## Migration Path

### Phase 1: Add IFC Membership (Non-Breaking) ✅ COMPLETED

1. ✅ Add `IfcId` type with atomic counter that resets per layout pass
2. ✅ Add `IfcMembership` struct with `ifc_id`, `ifc_root_layout_index`, `run_index`
3. ✅ Add `ifc_id` field to `LayoutNode` for IFC roots
4. ✅ Add `ifc_membership` field to `LayoutNode` for participating text nodes
5. ✅ Populate during `collect_and_measure_inline_content`
6. ✅ Reset IFC counter at start of `layout_document`

### Phase 2: Update Hit-Test Path ✅ COMPLETED

1. ✅ Modify `process_mouse_click_for_selection` to use IFC membership
2. ✅ Check `inline_layout_result` first (IFC root), then `ifc_membership` (text node)
3. ✅ Navigate from text node → IFC root via `ifc_root_layout_index`
4. ✅ Fallback path iterates only IFC roots (nodes with `inline_layout_result`)

### Phase 3: Debug Server Integration (IN PROGRESS - BLOCKED)

The debug server currently only modifies `mouse_state` without triggering the full
event processing pipeline. We attempted to fix this by:

1. ✅ Adding `get_mouse_position_with_fallback()` to use `mouse_state` when EventData is None
2. ✅ Removing duplicate `process_text_selection_click` call from debug server
3. ⚠️ The normal event pipeline now gets correct mouse position from `mouse_state`

**Current Status - BLOCKED:**

The selection IS being set correctly (verified by debug logs):
```
[DEBUG] process_mouse_click_for_selection: position=(58.0,28.0), time_ms=0
[DEBUG] HoverManager has hit test with 1 doms
[DEBUG] Setting selection on dom_id=DomId { inner: 0 }, node_id=NodeId(1)
```

But `GetSelectionState` returns empty selections. The issue is that:

1. **Text nodes have no rect in hit-testing**: When querying layout, text nodes return `rect: null`
   ```json
   {"node_id": 2, "tag": "text", "rect": null}
   ```
   This is correct - text nodes are inline content, not block boxes.

2. **Click handlers confirm the issue**: When adding click handlers:
   - Paragraph div handler fires: `[CLICK] Paragraph 1 was clicked!`
   - Text node handler does NOT fire (text nodes aren't hit-testable)

3. **Selection is set on correct node**: The IFC root (paragraph div, NodeId 1) has the
   `inline_layout_result`, and the selection is correctly stored there.

4. **But selection disappears**: Between `Setting selection` and `GetSelectionState`,
   the selection is lost. Possible causes:
   - Different `LayoutWindow` instances between timer callback invocations
   - Selection cleared by some intermediate operation
   - Timer callback CallbackInfo doesn't reference the same LayoutWindow

**Investigation Needed:**

The flow is:
1. Timer tick → MouseDown event → `process_callback_result_v2`
2. `mouse_state_changed = true` → calls `process_window_events_recursive_v2`
3. This generates `TextClick` internal event
4. `process_mouse_click_for_selection` is called → sets selection on `self.selection_manager`
5. Timer callback returns with `RefreshDom`
6. Next timer tick → GetSelectionState → reads from `callback_info.get_layout_window().selection_manager`

The question: Is step 6 reading from the same `LayoutWindow` as step 4 wrote to?

### Phase 4: Selection Rendering (TODO)

1. Implement `UnifiedLayout::get_selection_rectangles()`
2. Add selection rect rendering in display list generation
3. Support `::selection` pseudo-element styling

## Summary

| Issue | Current State | Proposed Solution |
|-------|--------------|-------------------|
| Layout stored on wrong node | IFC root has layout, text nodes don't | Add `ifc_membership` to text nodes |
| Hit-test requires fallback | Manual search through all nodes | Direct lookup via IFC membership |
| Selection rendering | Not implemented | `get_selection_rectangles()` in display list |
| Multi-IFC selection | Not handled | Selection manager tracks per-DOM-ID |
| `::selection` styling | Not implemented | Pseudo-element style lookup |
| Debug API selection | ⚠️ Selection set but not persisted | Investigate LayoutWindow identity |

## Debug Session Log (2026-01-13)

### Test Setup
- E2E test: `tests/e2e/selection.sh`
- C example: `tests/e2e/selection.c` with click handlers on paragraph div and text node
- Debug port: 8766

### Findings

1. **Text nodes don't have hit-test rects:**
   ```json
   {"node_id": 1, "tag": "div", "rect": {"x": 8.0, "y": 8.0, "width": 816.7, "height": 121.6}}
   {"node_id": 2, "tag": "text", "rect": null}
   ```

2. **Click handlers reveal hit-test target:**
   - `on_p1_click` (div) fires: YES
   - `on_p1_text_click` (text) fires: NO

3. **Selection IS being set (from logs):**
   ```
   [DEBUG] process_mouse_click_for_selection: position=(58.0,28.0), time_ms=0
   [DEBUG] HoverManager has hit test with 1 doms
   [DEBUG] Setting selection on dom_id=DomId { inner: 0 }, node_id=NodeId(1)
   ```

4. **But GetSelectionState returns empty:**
   ```json
   {"has_selection": false, "selection_count": 0, "selections": []}
   ```

### Code Changes Made

1. **core/src/events.rs**: Added `get_mouse_position_with_fallback()` that reads from
   `mouse_state.cursor_position` when `EventData::Mouse` is not available.

2. **debug_server.rs**: Removed manual `process_text_selection_click` call from MouseDown
   handler - now relies on normal event pipeline.

### Next Steps

1. Add debug log to `GetSelectionState` handler to see if selection_manager is empty
2. Verify that `callback_info.get_layout_window()` returns the persistent LayoutWindow
3. Check if selection is cleared between timer ticks
4. Consider storing selection in a separate location (not on LayoutWindow) for debug API

## Open Questions

1. **Should IFC ID be global or per-layout-pass?**
   - ✅ RESOLVED: Global with reset per layout pass
   - Counter resets at `layout_document()` entry
   - Same DOM structure → same IFC IDs across frames

2. **How to handle contenteditable?**
   - Text insertion changes the IFC content
   - Need to invalidate and rebuild IFC
   - Selection positions need remapping
   - Will become important for `<input type=text>` implementation

3. **Performance of multi-IFC selection?**
   - Selecting across 3 paragraphs = 3 IFC lookups
   - Should be fine for typical use cases
   - IFC root lookup is O(1) via `ifc_root_layout_index`

4. **RTL/Bidi selection rectangles?**
   - ✅ RESOLVED: Store logical cursors only
   - Selection rectangles computed at render time
   - Handles RTL/LTR mixed text correctly

5. **Debug Server LayoutWindow identity?**
   - ⚠️ UNDER INVESTIGATION
   - Timer callbacks may not share the same LayoutWindow reference
   - CallbackInfo is created fresh for each timer invocation
   - Need to verify that `callback_info.get_layout_window()` returns the actual
     window's LayoutWindow, not a copy

6. **Why do text nodes not participate in hit-testing?**
   - Text nodes have `rect: null` because they're inline content
   - Only block-level containers have rects
   - This is correct, but means text node click handlers don't fire
   - Selection must work via the IFC root (parent div) hit test
