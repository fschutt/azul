# ContentEditable Bugs Analysis - January 28, 2026

## Executive Summary

After implementing the V3 Text Input Plan with `CallbackChange::CreateTextInput` and removing the `PENDING_DEBUG_TEXT_INPUT` hack, the contenteditable test reveals multiple interconnected bugs. The debug output shows the text input flow is partially working (text does update on screen), but there are serious issues with cursor positioning, duplicate input, layout, and event handling.

---

## Bug 1: Cursor Not Appearing on Click

### Symptoms
- Clicking on the text input focuses it (blue outline appears)
- But NO cursor appears (neither at click position nor at end of text)
- Debug output shows:
  ```
  [DEBUG] process_mouse_click_for_selection: position=(1090.9,132.6), time_ms=0
  [DEBUG] HoverManager has hit test with 1 doms
  [DEBUG] Setting selection on dom_id=DomId { inner: 0 }, node_id=NodeId(2)
  ```

### Expected Behavior
- Cursor should appear at the clicked position within the text
- Cursor blink timer should start

### Likely Causes
1. `hit_test_text_at_point()` may not be correctly mapping click position to cursor position
2. `CursorManager.set_cursor_with_time()` may not be called after focus
3. Cursor blink timer not starting on focus
4. `finalize_pending_focus_changes()` not being called or not working

### Files to Investigate
- `layout/src/managers/cursor.rs` - CursorManager
- `layout/src/managers/focus_cursor.rs` - FocusManager  
- `layout/src/text3/selection.rs` - hit_test_text_at_point
- `dll/src/desktop/shell2/common/event_v2.rs` - Mouse click handling

---

## Bug 2: Double Input (Pressing 'j' inserts 'jj')

### Symptoms
- Pressing 'j' once inserts 'jj' (two characters)
- Debug output shows `get_pending_changeset` being called multiple times for single input
- From debugoutput.txt:
  ```
  [record_text_input] Called with text: 'j'
  ...
  Updated single_line_text: 'Hello World - Click here and type!j'
  ...
  Updated single_line_text: 'Hello World - Click here and type!jj'
  ```

### Expected Behavior
- Single keypress should insert exactly one character

### Likely Causes
1. `process_text_input()` being called twice
2. Timer callback AND event processing both triggering text input
3. `text_input_triggered` events being processed multiple times
4. `CreateTextInput` being pushed twice in callback changes

### Files to Investigate
- `layout/src/window.rs` - `process_text_input()`, `apply_text_changeset()`
- `layout/src/managers/text_input.rs` - TextInputManager
- `dll/src/desktop/shell2/common/event_v2.rs` - `process_callback_result_v2()`

---

## Bug 3: Wrong Text Input Affected (Second input shifts when typing in first)

### Symptoms
- Typing in the first (single-line) text input
- The SECOND (multi-line) text input shifts/moves
- Suggests layout recalculation is affecting wrong nodes

### Expected Behavior
- Only the focused text input should be affected by typing

### Likely Causes
1. `dirty_text_nodes` marking wrong node as dirty
2. Relayout affecting sibling/parent nodes incorrectly
3. `DomNodeId` confusion between DOM nodes
4. Scroll states being incorrectly updated

### Files to Investigate
- `layout/src/window.rs` - `relayout_dirty_nodes()`, `update_text_cache_after_edit()`
- `layout/src/solver3/mod.rs` - Layout tree building

---

## Bug 4: Mouse Move Triggers Horrible Resize

### Symptoms
- After typing, moving the mouse causes the first text input to resize horribly
- Many repeated debug outputs in debugoutput.txt showing the same character being processed
- Text grows exponentially

### Expected Behavior
- Mouse movement should not affect text content or trigger text input

### Likely Causes
1. Mouse move event incorrectly triggering text input processing
2. `text_input_affected_nodes` not being cleared after processing
3. Hover events incorrectly invoking text input callbacks
4. State machine in event_v2.rs not correctly separating mouse from keyboard events

### Files to Investigate
- `dll/src/desktop/shell2/common/event_v2.rs` - Event filtering
- `layout/src/managers/text_input.rs` - Changeset clearing

---

## Bug 5: Single-Line Input Breaking onto Multiple Lines

### Symptoms
- Single-line text input (should have `white-space: nowrap`) breaks onto multiple lines
- Screenshot shows text wrapping character-by-character
- `overflow: hidden` also seems ignored

### Expected Behavior
- Single-line input should:
  - Not wrap text (white-space: nowrap)
  - Clip overflow (overflow: hidden)
  - Scroll horizontally if needed

### Likely Causes
1. CSS `white-space: nowrap` not being applied during relayout
2. Text constraints cache not including white-space property
3. `update_text_cache_after_edit()` not respecting original constraints
4. Container width being ignored during text layout

### Files to Investigate
- `layout/src/text3/cache.rs` - UnifiedConstraints, word breaking
- `layout/src/window.rs` - `text_constraints_cache`
- `layout/src/solver3/fc.rs` - IFC layout

---

## Bug 6: No Scroll Into View

### Symptoms
- When text extends beyond visible area, view doesn't scroll to cursor
- User cannot see what they're typing

### Expected Behavior
- Cursor should always be visible
- Container should scroll to keep cursor in view

### Likely Causes
1. `scroll_active_cursor_into_view()` not being called
2. `PostCallbackSystemEvent::ScrollIntoView` not being generated
3. Scroll manager not receiving scroll commands

### Files to Investigate
- `layout/src/window.rs` - `scroll_active_cursor_into_view()`
- `dll/src/desktop/shell2/common/event_v2.rs` - Post-callback scroll handling

---

## Architecture Overview

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                            TEXT INPUT FLOW (V3)                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  1. User clicks on contenteditable                                          │
│     └─► Mouse event → Hit test → Focus node → Start cursor blink timer     │
│                                         ▲                                    │
│                                         │ BUG 1: Cursor not appearing        │
│                                                                              │
│  2. User types 'j'                                                           │
│     └─► Keyboard event → Timer callback → create_text_input()               │
│                              │                                               │
│                              ▼                                               │
│     └─► CallbackChange::CreateTextInput { text: "j" }                       │
│                              │                                               │
│                              ▼                                               │
│     └─► apply_callback_changes() → process_text_input()                    │
│                              │         ▲                                     │
│                              │         │ BUG 2: Called twice?                │
│                              ▼                                               │
│     └─► text_input_triggered populated                                      │
│                              │                                               │
│                              ▼                                               │
│     └─► process_callback_result_v2() invokes user callbacks                │
│                              │                                               │
│                              ▼                                               │
│     └─► apply_text_changeset() → update_text_cache_after_edit()            │
│                              │         ▲                                     │
│                              │         │ BUG 5: Constraints not preserved    │
│                              ▼                                               │
│     └─► relayout_dirty_nodes() → mark for repaint                          │
│                              │    ▲                                          │
│                              │    │ BUG 3: Wrong nodes affected              │
│                              │    │ BUG 4: Mouse move re-triggers            │
│                              ▼                                               │
│     └─► Display list updated → Text appears on screen                      │
│                                    ▲                                         │
│                                    │ BUG 6: No scroll into view              │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

---

## Debug Information Needed

1. **Full event trace**: Every event from click to text appearing
2. **CallbackChange log**: What changes are pushed and when
3. **TextInputManager state**: Pending changeset lifecycle
4. **Cursor position**: Where cursor is set (or not set)
5. **Dirty nodes list**: Which nodes marked as dirty
6. **Text constraints**: What constraints are used for relayout
7. **Layout tree before/after**: How layout changes with each character

---

## Files Relevant to Analysis

### Core Event Handling
- `dll/src/desktop/shell2/common/event_v2.rs`
- `dll/src/desktop/shell2/common/debug_server.rs`

### Callback System
- `layout/src/callbacks.rs`
- `layout/src/window.rs`

### Text Input Management
- `layout/src/managers/text_input.rs`
- `layout/src/managers/cursor.rs`
- `layout/src/managers/focus_cursor.rs`
- `layout/src/managers/selection.rs`

### Text Layout
- `layout/src/text3/cache.rs`
- `layout/src/text3/edit.rs`
- `layout/src/text3/selection.rs`

### Layout Solver
- `layout/src/solver3/mod.rs`
- `layout/src/solver3/fc.rs`
- `layout/src/solver3/display_list.rs`

---

## Recent Changes (Last 10 Commits to Review)

Need to check if recent changes introduced regressions:
1. Adding `text_input_triggered` to `CallCallbacksResult`
2. Removing `PENDING_DEBUG_TEXT_INPUT` hack
3. Changes to `process_callback_result_v2()`
4. Changes to `apply_callback_changes()`
5. Changes to `CreateTextInput` handling

---

## Test Case

```c
// tests/e2e/contenteditable.c
// Creates:
// 1. Single-line text input with "Hello World - Click here and type!"
// 2. Multi-line text area
// Both are contenteditable divs
```

Expected behavior:
1. Click on text → cursor appears at click position
2. Type 'j' → 'j' appears at cursor position
3. Cursor moves right one position
4. Text scrolls into view if needed
5. Single-line input doesn't wrap

Actual behavior:
1. Click → focus but NO cursor
2. Type 'j' → 'jj' appears
3. Second input shifts
4. Mouse move → text explodes
5. Text wraps onto many lines
