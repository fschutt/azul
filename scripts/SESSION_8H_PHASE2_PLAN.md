# Session 8H Phase 2: Remove CursorManager, Complete Multi-Cursor

## Status: In Progress

## Architecture Change

CursorManager (63 refs) + SelectionManager (40 refs) → MultiCursorState as single source of truth.

### TextEditManager (new shape)

```rust
pub struct TextEditManager {
    pub multi_cursor: Option<MultiCursorState>,  // Always Some when editing
    pub blink: BlinkState,                        // Extracted from CursorManager
    pub preedit_text: Option<String>,             // Moved from CursorManager
    pub preedit_cursor_begin: i32,
    pub preedit_cursor_end: i32,
    pub selection_manager: SelectionManager,       // Kept for non-editable drag-select
    pub display_list_dirty: bool,
}
```

### BlinkState (new)

```rust
pub struct BlinkState {
    pub is_visible: bool,
    pub last_input_time: Option<Instant>,
    pub blink_timer_active: bool,
}
```

## Phase 1: Restructure TextEditManager

1. Create BlinkState in text_edit.rs, move blink methods from CursorManager
2. Move preedit fields + methods from CursorManager to TextEditManager
3. Add convenience: has_active_editing(), get_editing_dom_id(), get_primary_cursor(),
   should_draw_cursor(), initialize_editing(), clear_editing()
4. Remove `cursor_manager` field from TextEditManager

## Phase 2: Migrate all cursor_manager call sites

### window.rs (44 refs)
- Display list generation (2 sites): cursor_is_visible, cursor_locations, preedit
- Focus/blink timer (8 sites): blink state
- finalize_pending_focus_changes (4 sites): initialize_editing
- Cursor movement (8 sites): multi_cursor.move_all_cursors
- Text editing (6 sites): multi_cursor.to_selections
- Mouse click (5 sites): blink reset
- Accessibility (4 sites): cursor_a11y_info from multi_cursor
- Selection sync, delete, scroll (7 sites)

### event.rs (25 refs)
- CallbackChange handlers: route through multi_cursor
- Blink timer handlers: route through blink

### Platform IME (22 refs)
- macOS: preedit, has_marked_text, cursor_a11y_info
- Wayland: preedit, cursor_location check
- X11: similar pattern

## Phase 3: Delete CursorManager

Remove layout/src/managers/cursor.rs entirely.

## Phase 4: Word Deletion

Add `word_delete: bool` to `DeleteTextSelection` in SystemChange.
In handler: use UnifiedLayout.move_cursor_to_prev/next_word to expand cursors to
word-boundary ranges, then call normal delete_selection.

## Phase 5: Smart Clipboard

### Ctrl+C with multiple selections
Concatenate selected text from all selections with "\n" separator.

### Ctrl+V with multiple selections
```
lines = paste_text.lines()
if lines.len() == selections.len():
    edit_text_multi(content, selections, lines)  # one line per cursor
else:
    edit_text(content, selections, Insert(paste_text))  # broadcast
```
