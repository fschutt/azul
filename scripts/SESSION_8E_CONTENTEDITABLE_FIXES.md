# Session 8E: Contenteditable Bug Fixes

**Date**: 2026-03-30
**Branch**: `layout-debug-clean`

---

## Status After UpdateDisplayList Fix

Text now appears when typing (no beep, cursor moves). But 7 bugs remain.

## Bug A: Focus ring doesn't show on click (only after resize)

**Root cause**: `SystemChange::SetFocus` handler in `event.rs:2438-2461` calls
`apply_focus_restyle()` but IGNORES the return value (assigned to `_restyle_result`).
It unconditionally returns `ShouldReRenderCurrentWindow` which maps to `RequestRedraw`
— this does NOT rebuild the display list with the new `:focus` CSS styling.

On resize, `frame_needs_regeneration` triggers full rebuild which applies focus
styling via `apply_runtime_states_before_layout()`.

**Fix**: Use the `apply_focus_restyle` return value:
```rust
// event.rs:2461 — replace:
//   ProcessEventResult::ShouldReRenderCurrentWindow
// with:
result.max(restyle_result)
```

**File**: `dll/src/desktop/shell2/common/event.rs:2438-2461`

---

## Bug B + H: Cursor at position 0, not where clicked / Mouse click doesn't reposition

**Root cause**: Two issues:

1. `TextSelectionClick` fires BEFORE `SetFocus` on first click. It correctly
   computes the click position via `hittest_cursor()` and stores it in
   `selection_manager` — but NOT in `cursor_manager` which controls the caret.

2. `SetFocus` → `finalize_pending_focus_changes()` overrides cursor with
   `initialize_cursor_at_end()`, ignoring the click position.

**Fix**:
- `process_mouse_click_for_selection()` (window.rs:5509) must also call
  `cursor_manager.set_cursor_with_time()` to update the caret position.
- `finalize_pending_focus_changes()` should check if a selection click already
  positioned the cursor in the same event cycle and skip the end-of-text default.

**Files**: `layout/src/window.rs:5509`, `layout/src/window.rs` (finalize_pending_focus_changes)

---

## Bug C: Resize reverts text to original

**Root cause**: Resize triggers `frame_needs_regeneration` → `regenerate_layout()`
calls the user's layout callback which returns DOM with ORIGINAL text. The
`dirty_text_nodes` map survives but is never consulted during rebuild — the new
StyledDom overwrites the edited text.

**Fix (architectural)**:
The V3 plan's intended solution: user callbacks should update their data model on
`On::TextInput`, so `layout()` returns the updated text. For the framework side,
`dirty_text_nodes` should be cleared on full DOM rebuild (document that unsaved
edits are lost on resize/RefreshDom as intended behavior).

Alternatively: after state migration in `regenerate_layout()`, apply dirty_text_node
content back into the new StyledDom before layout. This is complex but preserves
edits across rebuilds.

**File**: `dll/src/desktop/shell2/common/layout.rs:72-277`

---

## Bug D: Arrow keys "hide" text but move cursor

**Root cause**: `ArrowKeyNavigation` handler (event.rs:2037-2103) calls
`move_cursor_in_node()` and `handle_cursor_movement()` which update cursor state,
then returns `ShouldUpdateDisplayListCurrentWindow`. This sets `display_list_dirty`
which re-sends the OLD display list to WebRender — but the display list was never
actually regenerated with the new cursor position.

**Fix**: `handle_cursor_movement()` must call `regenerate_display_list_for_dom()`
to rebuild the display list with the new cursor position before returning.

**File**: `layout/src/window.rs` (handle_cursor_movement), `common/event.rs:2099-2102`

---

## Bug E: Backspace doesn't work

**Root cause**: In `core/events.rs:2785-2798`, the `DeleteTextSelection` system
change is gated on `selection_manager.has_selection()`. A cursor without a visual
selection is NOT considered a "selection" by the selection_manager. So pressing
Backspace with just a cursor (no highlighted text) generates NO system change.

**Fix**: Remove the `has_selection()` guard for Backspace/Delete. When there's a
cursor but no selection, delete one character before (Backspace) or after (Delete)
the cursor:

```rust
// When no selection exists but cursor is present:
// Backspace → delete char before cursor
// Delete → delete char after cursor
```

Need to add a new `SystemChange::DeleteCharacterAtCursor` variant or modify
`delete_selection()` to handle cursor-only case.

**File**: `core/src/events.rs:2785-2798`, `layout/src/window.rs:5959` (delete_selection)

---

## Bug F: Cmd+Q doesn't work from key handler

**Not a bug**: Cmd+Q is handled by macOS native menu system via `NSApplication
terminate:`, independent of `keyDown:`. The key handler does not intercept it.
If Cmd+Q appears slow, it's because the responder chain takes time.

---

## Bug G: Text selection doesn't work

**Root cause**: Multiple issues:
- Mouse drag: `TextSelectionDrag` is generated when mouse moves with button down,
  but requires `click_state.click_count > 0` from prior `TextSelectionClick`.
  If the click state isn't properly recorded, drags are ignored.
- Shift+arrow: `ArrowKeyNavigation` has `extend_selection` flag from Shift key,
  but `handle_cursor_movement()` may not create/extend SelectionRange properly.
- Rendering: Selection highlight is rendered from `selection_manager` data in
  `paint_selections()`. If selections are stored but display list isn't rebuilt,
  highlights don't appear.

**Fix**: Verify click state recording in `process_mouse_click_for_selection()`.
Verify `handle_cursor_movement(extend_selection=true)` creates SelectionRange.
Ensure display list regeneration after selection changes.

**Files**: `layout/src/managers/selection.rs`, `layout/src/window.rs`,
`common/event.rs:2012-2026`

---

## Fix Priority

| # | Bug | Impact | Effort | Fix |
|---|-----|--------|--------|-----|
| 1 | B+H | HIGH | 1 hr | cursor_manager update in click handler |
| 2 | A | HIGH | 15 min | Use restyle_result in SetFocus |
| 3 | E | HIGH | 1 hr | Remove has_selection guard for backspace |
| 4 | D | MEDIUM | 30 min | regenerate_display_list in cursor movement |
| 5 | C | MEDIUM | 2 hr | dirty_text_nodes integration or clear |
| 6 | G | MEDIUM | 2 hr | Selection manager + display list sync |

---

## Key Files

| Component | File |
|-----------|------|
| Focus restyle | `dll/src/desktop/shell2/common/event.rs:2438-2461` |
| Click-to-cursor | `layout/src/window.rs:5509` |
| Pending focus | `layout/src/window.rs` (finalize_pending_focus_changes) |
| Arrow key nav | `dll/src/desktop/shell2/common/event.rs:2037-2103` |
| Cursor movement | `layout/src/window.rs` (handle_cursor_movement) |
| Backspace guard | `core/src/events.rs:2785-2798` |
| Delete selection | `layout/src/window.rs:5959` |
| DOM rebuild | `dll/src/desktop/shell2/common/layout.rs:72-277` |
| Selection painting | `layout/src/solver3/display_list.rs` (paint_selections) |
