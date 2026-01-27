# Gemini Text Input Architecture Response

Of course. This is an excellent and well-documented architectural challenge. The problems are interconnected, and a holistic solution is required. Here is a comprehensive review and a complete implementation plan based on the provided information and W3C standards.

## Executive Summary of the Plan

The core issue is a broken data flow loop where text edits are calculated but not persisted back to the source of truth, preventing layout and rendering updates. The secondary issues of cursor positioning and focus transfer stem from incomplete event handling logic that also depends on a correctly updated layout.

This plan will address these issues in a sequential, dependency-aware order:
1.  **Fix Text Storage:** Implement a robust text storage strategy by mutating the `StyledDom` directly. This establishes a single source of truth.
2.  **Implement Text Updates:** Complete the `update_text_cache_after_edit` function to persist changes, which will make typing visible and enable cursor movement during input.
3.  **Implement Cursor Positioning on Click:** Implement point-to-cursor hit testing to allow users to place the cursor with the mouse.
4.  **Implement Focus Transfer:** Refine the focus and cursor management logic to ensure a seamless transition of focus and cursor state between editable elements.

This step-by-step approach ensures that foundational problems are solved first, providing a stable base for subsequent features.

---

## Q1: What is the correct architecture for text storage?

The best approach is **A) Mutate `NodeType::Text` in StyledDom directly**, but with a crucial clarification on how to handle complex edits.

**Rationale:**

*   **Single Source of Truth:** The `StyledDom` is the primary input to the layout engine. Keeping it synchronized is the most direct way to ensure that layout, rendering, accessibility, and callbacks all see the same state. Shadow caches (Option B) introduce complexity and synchronization bugs.
*   **Compatibility with Layout:** The layout engine is already built to read from `StyledDom`. By mutating it and marking the node as dirty, you leverage the existing layout pipeline without modification.
*   **Undo/Redo:** The existing `UndoRedoManager` is designed for this. It takes a `NodeStateSnapshot` *before* the operation. Mutating the `StyledDom` is the "operation," and the snapshot provides the data needed to revert it.
*   **`InlineContent` as a Transactional Type:** Treat `Vec<InlineContent>` as a temporary, transactional data structure. The lifecycle is:
    1.  Read from `StyledDom` -> `Vec<InlineContent>` (`get_text_before_textinput`)
    2.  Perform edits on `Vec<InlineContent>` -> `new Vec<InlineContent>` (`edit_text`)
    3.  Write back from `new Vec<InlineContent>` -> `StyledDom` (`update_text_cache_after_edit`)

**Handling Multi-Node Selections:**

Your concern about multi-node selections is valid. A simple `NodeType::Text(new_string)` update only works for single-node contenteditables. When edits span multiple nodes (e.g., deleting `<b>bold</b>` from `normal <b>bold</b> text`), you are not just changing text content; you are changing the DOM *structure*.

**Implementation Strategy:**

1.  **Phase 1 (Immediate Fix):** Assume a single text node within the contenteditable container. The new `Vec<InlineContent>` will likely contain a single `InlineContent::Text` run. Flatten this into a single string and update the single `NodeType::Text` in the `StyledDom`. This will solve the immediate problems.
2.  **Phase 2 (Long-Term):** For true multi-node support, `update_text_cache_after_edit` must become more sophisticated. It will need to diff the old and new `Vec<InlineContent>` and translate those changes into DOM mutations (removing nodes, merging text nodes, creating new text nodes). This is a significant undertaking and should be deferred until the basic single-node case is working perfectly.

For now, we will proceed with the Phase 1 strategy.

## Q2: How should `update_text_cache_after_edit()` work?

This function is the missing link. Its purpose is to take the result of a text edit (the `new_inline_content`) and persist it back into the `StyledDom`, which serves as the source of truth for the next layout pass.

Here is the full implementation based on the architecture from Q1 (Phase 1).

```rust
// In layout/src/window.rs

pub fn update_text_cache_after_edit(
    &mut self,
    dom_id: DomId,
    node_id: NodeId,
    new_inline_content: Vec<InlineContent>,
) {
    // This function now mutates the StyledDom directly. The name is a bit of a
    // misnomer ("cache"), but we keep it for consistency with the existing code.
    // A better name would be `update_dom_after_edit`.

    let Some(layout_result) = self.layout_results.get_mut(&dom_id) else {
        // If the DOM isn't in the layout results, we can't update it.
        return;
    };

    // --- Step 1: Flatten the new InlineContent into a single string ---
    // This is a simplification for the single-node contenteditable case.
    let new_text = self.extract_text_from_inline_content(&new_inline_content);

    // --- Step 2: Find the target text node to update ---
    // For now, we assume `node_id` is the text node itself or the container
    // of a single text node. A more complex implementation would find the
    // correct child text node.
    let target_node_id = self.find_first_text_child(dom_id, node_id).unwrap_or(node_id);

    // --- Step 3: Mutate the NodeType in StyledDom ---
    let node_data_mut = layout_result.styled_dom.node_data.as_container_mut();
    if let Some(node_to_update) = node_data_mut.get_mut(target_node_id) {
        // Handle both existing text nodes and cases where a div might become a text node.
        // A robust solution would handle DOM structural changes, but for now, we just
        // update the content.
        match &mut node_to_update.node_type {
            NodeType::Text(existing_text) => {
                *existing_text = new_text.into();
            }
            // If the node isn't a text node (e.g., an empty div), we can't just
            // overwrite its type. The correct solution is to find or create a child
            // text node. For now, we'll log a warning and do nothing if we can't
            // find a text node to update.
            _ => {
                // This path is taken if the contenteditable is an empty div.
                // The correct action is to insert a new text node. For simplicity,
                // we'll just change the NodeType. This is a temporary hack.
                node_to_update.node_type = NodeType::Text(new_text.into());
            }
        }
    }

    // --- Step 4: Trigger Relayout ---
    // The caller (`apply_text_changeset`) is responsible for marking the node
    // as dirty, which will trigger a relayout in the next frame. This function
    // doesn't need to do anything else.
}

// Add this helper function to `LayoutWindow` in layout/src/window.rs
impl LayoutWindow {
    /// Finds the first direct child of a node that is a Text node.
    fn find_first_text_child(&self, dom_id: DomId, parent_node_id: NodeId) -> Option<NodeId> {
        let layout_result = self.layout_results.get(&dom_id)?;
        let styled_dom = &layout_result.styled_dom;
        let hierarchy = styled_dom.node_hierarchy.as_container();

        let mut current_child_id = hierarchy.get(parent_node_id)?.first_child_id(parent_node_id);
        while let Some(child_id) = current_child_id {
            if let Some(node_data) = styled_dom.node_data.as_container().get(child_id) {
                if matches!(node_data.node_type, NodeType::Text(_)) {
                    return Some(child_id);
                }
            }
            current_child_id = hierarchy.get(child_id)?.next_sibling_id();
        }
        None
    }
}
```

## Q3: How should cursor click positioning work?

You need a function that maps a `LogicalPosition` (a point) to a `TextCursor`. The existing code in `layout/src/text3/cache.rs` already contains an excellent implementation for this: `UnifiedLayout::hittest_cursor`. We should expose this and integrate it into the `MouseDown` event handler.

Here is the implementation, renamed to match your request for clarity, and to be placed in a suitable location.

```rust
// This function can be a method on `UnifiedLayout` or a standalone helper.
// Let's add it as a new public function in `layout/src/text3/selection.rs`
// since it's related to selection.

use azul_core::geom::LogicalPosition;
use azul_core::selection::{TextCursor, CursorAffinity};
use crate::text3::cache::{UnifiedLayout, PositionedItem, ShapedItem};

/// Takes a point relative to the layout's origin and returns the closest
/// logical cursor position.
pub fn hit_test_text_at_point(
    layout: &UnifiedLayout,
    point: LogicalPosition,
) -> Option<TextCursor> {
    if layout.items.is_empty() {
        return None;
    }

    // --- Step 1: Find the line closest to the click's Y coordinate ---
    let mut closest_line_idx = 0;
    let mut min_vertical_dist = f32::MAX;

    // Group items by line to find the vertical center of each line
    let mut line_bounds: BTreeMap<usize, (f32, f32)> = BTreeMap::new(); // (min_y, max_y)
    for item in &layout.items {
        let item_bounds = item.item.bounds();
        let (min_y, max_y) = line_bounds.entry(item.line_index).or_insert((f32::MAX, f32::MIN));
        *min_y = min_y.min(item.position.y);
        *max_y = max_y.max(item.position.y + item_bounds.height);
    }

    for (line_idx, (min_y, max_y)) in line_bounds {
        let line_center_y = min_y + (max_y - min_y) / 2.0;
        let dist = (point.y - line_center_y).abs();
        if dist < min_vertical_dist {
            min_vertical_dist = dist;
            closest_line_idx = line_idx;
        }
    }

    // --- Step 2: Find the horizontally closest cluster on that line ---
    let mut closest_cluster_item: Option<&PositionedItem> = None;
    let mut min_horizontal_dist = f32::MAX;

    for item in layout.items.iter().filter(|i| i.line_index == closest_line_idx) {
        if let ShapedItem::Cluster(_) = &item.item {
            let item_bounds = item.item.bounds();
            let dist = if point.x < item.position.x {
                item.position.x - point.x
            } else if point.x > item.position.x + item_bounds.width {
                point.x - (item.position.x + item_bounds.width)
            } else {
                0.0 // Inside the cluster horizontally
            };

            if dist < min_horizontal_dist {
                min_horizontal_dist = dist;
                closest_cluster_item = Some(item);
            }
        }
    }
    
    // If no cluster is found on the line (e.g., an empty line), find the last cluster
    // on the previous line or the first on the next to handle clicks in empty space.
    let target_item = closest_cluster_item.or_else(|| {
        layout.items.iter().rev().find(|i| i.line_index < closest_line_idx && i.item.as_cluster().is_some())
    })?;

    let cluster = target_item.item.as_cluster()?;

    // --- Step 3: Determine affinity based on which half of the cluster was clicked ---
    let cluster_mid_x = target_item.position.x + cluster.advance / 2.0;
    let affinity = if point.x < cluster_mid_x {
        CursorAffinity::Leading
    } else {
        CursorAffinity::Trailing
    };

    Some(TextCursor {
        cluster_id: cluster.source_cluster_id,
        affinity,
    })
}
```

## Q4: How should focus transfer work?

Focus transfer involves coordinating `FocusManager`, `CursorManager`, and the blink timer. The key is using the "flag and defer" pattern for cursor initialization.

1.  **Event Order:** When clicking from contenteditable A (focused) to contenteditable B:
    *   `MouseDown` on B.
    *   `process_window_events` runs.
    *   A `FocusLost` event is generated for A.
    *   A `FocusReceived` event is generated for B.
    *   `MouseUp` on B.
    *   `Click` on B.

2.  **Old Cursor Cleanup:**
    *   The `FocusLost` event on A triggers `handle_focus_change_for_cursor_blink`.
    *   Inside this function, `self.cursor_manager.clear()` is called.
    *   The function returns `CursorBlinkTimerAction::Stop`.
    *   The platform shell (`events.rs`) receives this and stops the native timer for A's cursor.

3.  **New Cursor Initialization:**
    *   The `FocusReceived` event on B triggers `handle_focus_change_for_cursor_blink`.
    *   This function sees the new node is contenteditable and calls `self.focus_manager.set_pending_contenteditable_focus(...)`. This sets the `cursor_needs_initialization` flag.
    *   It returns `CursorBlinkTimerAction::Start`. The platform shell starts the native timer.
    *   The `MouseDown` handler also runs. It should perform hit-testing using the function from Q3. It finds the precise `TextCursor` position for the click. It then calls `self.cursor_manager.set_cursor_with_time(...)`.
    *   This sets the cursor *immediately* and resets the blink timer. It effectively overrides the deferred initialization. This is correct and desirable for click-to-position.
    *   If focus is set programmatically (e.g., Tab key), the `MouseDown` handler doesn't run. In this case, `finalize_pending_focus_changes()` runs after layout and places the cursor at the end of the text.

4.  **Blink Timer Interaction:**
    *   The `handle_focus_change_for_cursor_blink` function in `window.rs` is the central controller.
    *   When focus is lost on an editable element, it deactivates the timer (`self.cursor_manager.set_blink_timer_active(false)`) and returns `CursorBlinkTimerAction::Stop`.
    *   When focus is gained on an editable element, it activates the timer (`self.cursor_manager.set_blink_timer_active(true)`) and returns `CursorBlinkTimerAction::Start`.
    *   Any text input or cursor movement (including from a click) should call `cursor_manager.reset_blink_on_input(now)`, which makes the cursor visible and resets the time-since-last-input, preventing blinking while typing.

---

## Q5: Complete Implementation Plan

Here is a step-by-step plan to fix all reported issues.

### **Step 1: Fix Text Storage and Update Pipeline**

This step addresses **Problem 1** (Text Input Doesn't Update Display) and is the prerequisite for **Problem 4**.

1.  **Modify `update_text_cache_after_edit`:**
    *   **File:** `layout/src/window.rs`
    *   **Action:** Replace the `// TODO` stub with the full implementation provided in the **Q2 answer** above. Also add the `find_first_text_child` helper method to `LayoutWindow`.
    *   **Goal:** This function will now take the `new_inline_content` from `edit_text`, flatten it to a string, and write it back into the `NodeType::Text` of the correct node in the `StyledDom`.

2.  **Verify `apply_text_changeset`:**
    *   **File:** `layout/src/window.rs`
    *   **Action:** No changes are needed, but verify its logic. It calls `edit_text`, then `update_text_cache_after_edit`, then `update_cursor_manager`, and finally returns the dirty nodes. After your change in the previous step, this pipeline will now be complete. The `new_selections` returned from `edit_text` will correctly update the cursor position via `cursor_manager.move_cursor_to`.

3.  **Test Cases for Step 1:**
    *   Focus a contenteditable div.
    *   Type a character. **Expected:** The character appears on screen.
    *   Type several more characters. **Expected:** The text updates, and the cursor advances with each character.
    *   Use the Backspace key. **Expected:** The last character is deleted, and the cursor moves back.
    *   Use the Delete key. **Expected:** The character after the cursor is deleted, and the cursor stays in place.

### **Step 2: Implement Cursor Positioning on Click**

This step addresses **Problem 2** (Cursor Doesn't Reposition on Click).

1.  **Add `hit_test_text_at_point` function:**
    *   **File:** `layout/src/text3/selection.rs`
    *   **Action:** Add the full implementation provided in the **Q3 answer** above. Make sure to add `pub use crate::text3::selection::hit_test_text_at_point;` where needed.

2.  **Integrate Hit-Testing into `MouseDown` handler:**
    *   **File:** `dll/src/desktop/shell2/common/event_v2.rs`
    *   **Action:** Modify the `handle_mouse_down` logic within the `PlatformWindowV2` trait.
    ```rust
    // Inside a function like process_mouse_click_for_selection or a new one
    // called from the MouseDown handler in event_v2.rs

    // ... after getting the hit test result ...
    let hit_node_id: DomNodeId = /* ... get from hit test ... */;

    if let Some(layout_window) = self.get_layout_window_mut() {
        let node_id_internal = hit_node_id.node.into_crate_internal().unwrap();
        
        // Check if the hit node is inside a contenteditable container
        if layout_window.is_node_contenteditable_inherited_internal(hit_node_id.dom, node_id_internal) {
            
            // Find the IFC root for this text node
            let ifc_root_id = layout_window.find_contenteditable_ancestor(hit_node_id.dom, node_id_internal)
                .unwrap_or(node_id_internal);

            if let Some(layout) = layout_window.get_inline_layout_for_node(hit_node_id.dom, ifc_root_id) {
                
                // Get node's absolute position to calculate local click position
                let node_pos = layout_window.get_node_position(DomNodeId { dom: hit_node_id.dom, node: ifc_root_id.into() }).unwrap_or_default();
                let local_click_pos = LogicalPosition {
                    x: position.x - node_pos.x,
                    y: position.y - node_pos.y,
                };

                // Perform hit test
                if let Some(new_cursor) = azul_layout::text3::selection::hit_test_text_at_point(&layout, local_click_pos) {
                    
                    // Set focus to the contenteditable container
                    layout_window.focus_manager.set_focused_node(Some(DomNodeId { dom: hit_node_id.dom, node: ifc_root_id.into() }));

                    // Set cursor position and reset blink timer
                    let now = azul_core::task::Instant::now();
                    layout_window.cursor_manager.set_cursor_with_time(Some(new_cursor), Some(CursorLocation { dom_id: hit_node_id.dom, node_id: ifc_root_id }), now);

                    // Clear any existing selection
                    layout_window.selection_manager.clear_text_selection(&hit_node_id.dom);
                }
            }
        }
    }
    ```

3.  **Test Cases for Step 2:**
    *   Click at the beginning of a line of text. **Expected:** Cursor appears at the start.
    *   Click in the middle of a word. **Expected:** Cursor appears in the middle.
    *   Click at the end of a line. **Expected:** Cursor appears at the end.
    *   Click on a different line in a multi-line input. **Expected:** Cursor moves to the clicked line.

### **Step 3: Fix Focus Transfer Between Inputs**

This step addresses **Problem 3** (Focus Transfer). It relies on the logic from Step 2 being integrated into the `MouseDown` handler, which correctly triggers focus changes.

1.  **Review `handle_focus_change_for_cursor_blink`:**
    *   **File:** `layout/src/window.rs`
    *   **Action:** Ensure this function correctly handles both gaining and losing focus on a contenteditable element.
        *   **On focus gain:** It should set the `pending_contenteditable_focus` flag in `FocusManager` and return `CursorBlinkTimerAction::Start`.
        *   **On focus loss:** It should call `cursor_manager.clear()`, `focus_manager.clear_pending_contenteditable_focus()`, and return `CursorBlinkTimerAction::Stop`.
    *   The existing code for this function seems mostly correct and follows the "flag and defer" pattern. The main fix is ensuring it's called correctly.

2.  **Review `finalize_pending_focus_changes`:**
    *   **File:** `layout/src/window.rs`
    *   **Action:** Ensure this function is called *after* the layout pass in your main event loop. It should check the flag, get the now-valid layout, and initialize the cursor at the end of the text. This serves as the fallback for non-click focus events (like Tab).

3.  **Test Cases for Step 3:**
    *   Type some text in the first input.
    *   Click on the second input. **Expected:** The cursor from the first input disappears, and a new cursor appears at the click location in the second input. The blink timer should be active for the second input.
    *   Type text in the second input.
    *   Click back on the first input. **Expected:** Focus and cursor transfer back correctly.
    *   Use the Tab key to switch between inputs. **Expected:** Focus moves, and a cursor appears (likely at the end of the text) in the newly focused input.

### **Step 4: Final Polish and Edge Cases**

1.  **Empty ContentEditable:**
    *   **Action:** Test clicking and typing in an empty contenteditable `div`. The `update_text_cache_after_edit` logic should handle creating a `NodeType::Text` if one doesn't exist. The hit-testing should gracefully handle an empty `UnifiedLayout` by placing the cursor at the start.
2.  **Deleting All Text:**
    *   **Action:** Test deleting all text from an input. `update_text_cache_after_edit` should correctly set the `NodeType::Text` to an empty string. The cursor should remain at position 0.
3.  **Undo/Redo Integration:**
    *   **File:** `layout/src/window.rs` in `apply_text_changeset`
    *   **Action:** Ensure that a `NodeStateSnapshot` is created *before* `update_text_cache_after_edit` is called and that `undo_redo_manager.record_operation` is called *after*. The current code seems to have placeholders for this, which should now work correctly.

This complete plan addresses all identified problems by fixing the core data flow issue first, then building the interactive features (cursor positioning, focus) on top of that stable foundation.