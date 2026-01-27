# Text Input Architecture Review: Complete Implementation Plan

## Executive Summary

We need a comprehensive architectural review and implementation plan for text input
in contenteditable elements. Several interconnected problems need to be solved together.

## Current Problems

### Problem 1: Text Input Doesn't Update Display
**Symptom:** When typing in a contenteditable div, no characters appear on screen.

**Root Cause Found:** The `update_text_cache_after_edit()` function is a TODO stub:
```rust
pub fn update_text_cache_after_edit(
    &mut self,
    dom_id: DomId,
    node_id: NodeId,
    new_inline_content: Vec<InlineContent>,
) {
    // TODO: Update the text cache with the new inline content
    let _ = (dom_id, node_id, new_inline_content);
}
```

The text is being transformed via `edit_text()` in `text3/edit.rs`, but never persisted.

### Problem 2: Cursor Doesn't Reposition on Click
**Symptom:** Clicking in contenteditable text doesn't move the cursor to the click position.

**Likely Cause:** Hit testing returns the container node, but we need to:
1. Hit test to find which text cluster was clicked
2. Calculate the cursor position within that cluster
3. Update CursorManager with the new position

### Problem 3: Focus Transfer Between Inputs
**Symptom:** Clicking on a second contenteditable doesn't properly transfer focus/cursor.

**Likely Cause:** Focus is being set, but cursor initialization for the new element
isn't happening correctly.

### Problem 4: Cursor Doesn't Move with Text Input
**Symptom:** Can't verify cursor movement because text isn't updating.

**Related to Problem 1:** Once text updates work, cursor should advance via
the `new_selections` returned from `edit_text()`.

## Architecture Requirements

### 1. Text Update Pipeline

When the user types a character:

```
Platform (macOS/Windows/Linux)
    ↓ record_text_input(text)
    ↓
TextInputManager.record_input()
    ↓ stores changeset
    ↓
Event Processing (event_v2.rs)
    ↓ fires On::TextInput callback (if registered)
    ↓ if !preventDefault
    ↓
apply_text_changeset()
    ↓ calls edit_text() to get new InlineContent
    ↓ calls update_text_cache_after_edit() <-- BROKEN
    ↓ updates CursorManager position
    ↓
Relayout + Repaint
    ↓
Display shows new text + cursor
```

### 2. Text Storage Architecture

Current (broken):
- Text is stored in `StyledDom.node_data[node_id].node_type = NodeType::Text(AzString)`
- `get_text_before_textinput()` reads from this
- But `update_text_cache_after_edit()` never writes back

Options:
A) **Mutate StyledDom directly** - Modify the NodeType::Text content
B) **Maintain shadow text cache** - Store edited text in a separate HashMap
C) **Use InlineContent as source of truth** - Store the shaped text, regenerate on layout

### 3. Callback Architecture for Text Input

W3C-like model for user callbacks:

```
beforeinput event (can preventDefault)
    ↓
if !defaultPrevented:
    apply text change
    ↓
input event (informational, after change applied)
```

User should be able to:
- Intercept text before it's applied
- Modify or reject the input
- Read the current text content afterward

### 4. Cursor Click Positioning

When user clicks in contenteditable:

```
Mouse Click
    ↓
Hit Test → find node under cursor
    ↓
If node has inline_layout_result:
    ↓
    hit_test_text_at_point(click_position)
    ↓ returns TextCursor { cluster_id, affinity }
    ↓
CursorManager.move_cursor_to(cursor)
    ↓
Repaint cursor at new position
```

### 5. Focus Transfer

When focus changes between contenteditables:

```
Click on Element B (while Element A is focused)
    ↓
FocusManager.set_focus(B)
    ↓
Stop cursor blink timer for A
Start cursor blink timer for B
    ↓
Initialize cursor in B:
    - If selection exists: use it
    - Else: place at end of text (or start, based on click position)
    ↓
CursorManager.cursor_location = (dom_id, B, cursor)
```

## Questions for Gemini

### Q1: What is the correct architecture for text storage?

Should we:
A) Mutate `NodeType::Text` in StyledDom directly
B) Keep a separate `HashMap<(DomId, NodeId), String>` for edited text
C) Use InlineContent and rebuild during layout
D) Some other approach

Consider:
- Undo/redo needs to work
- Layout needs to see the updated text
- Callbacks need to be able to read current text
- We need to handle multi-node selections (delete across nodes)

### Q2: How should `update_text_cache_after_edit()` work?

Provide the full implementation. It needs to:
- Update text so `get_text_before_textinput()` returns new text
- Trigger relayout of the affected node
- Handle the case where text is deleted entirely

### Q3: How should cursor click positioning work?

The `get_cursor_rect()` function exists to convert cursor → rect.
We need the inverse: point → cursor.

Provide implementation for:
```rust
fn hit_test_text_at_point(
    &self,
    layout: &ShapedLayout,
    point: LogicalPosition,
) -> Option<TextCursor>
```

### Q4: How should focus transfer work?

When clicking from one contenteditable to another:
1. What order do events fire?
2. How is the old cursor cleaned up?
3. How is the new cursor initialized?
4. How does this interact with the blink timer?

### Q5: Complete Implementation Plan

Provide a step-by-step implementation plan with:
1. Order of changes (dependencies)
2. Specific functions to add/modify
3. Test cases to verify each step
4. Potential edge cases

## Source Code Reference

The following source files are provided for analysis.
Pay special attention to:
- `layout/src/text3/edit.rs` - the edit_text function
- `layout/src/window.rs` - apply_text_changeset and update_text_cache_after_edit
- `layout/src/managers/text_input.rs` - TextInputManager
- `layout/src/managers/cursor.rs` - CursorManager
- `layout/src/text3/cache.rs` - get_cursor_rect and text layout

