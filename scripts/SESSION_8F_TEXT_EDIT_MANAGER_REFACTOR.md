# Session 8F: TextEditManager — Merge CursorManager + SelectionManager

**Date**: 2026-03-30

---

## Why the Current Architecture is Brittle

Text editing state is fragmented across **3 managers** that must be manually
synchronized by every call site:

| State | Owner | Also needed by |
|-------|-------|---------------|
| Cursor position | `CursorManager.cursor` | SelectionManager, display list |
| Cursor DOM location | `CursorManager.cursor_location` | FocusManager, a11y |
| Selection ranges | `SelectionManager.selections` | display list, delete |
| Click state | `SelectionManager.click_state` | drag detection |
| Blink timer | `CursorManager.is_visible` | display list |
| IME preedit | `CursorManager.preedit_text` | display list |

Every bug comes from forgetting to update one manager when another changes:
- Bug B+H: Click updates selection_manager but not cursor_manager
- Bug D: Arrow keys update cursor_manager but don't regenerate display list
- Bug E: Backspace checks selection_manager.has_selection() but not cursor_manager
- Bug G: Selection and cursor can diverge

---

## Solution: Single `TextEditManager`

A cursor IS a collapsed selection. Merge into one manager with one source of truth.

### Core struct

```rust
pub struct TextEditManager {
    /// The active editing state (at most one node edited at a time).
    /// Selection::Cursor = blinking caret, Selection::Range = highlighted text.
    pub active: Option<TextEditState>,

    // Blink state
    pub cursor_visible: bool,
    pub last_input_time: Option<Instant>,
    pub blink_timer_active: bool,

    // IME
    pub preedit_text: Option<String>,
    pub preedit_cursor_begin: i32,
    pub preedit_cursor_end: i32,

    // Click detection
    pub click_state: ClickState,

    // Multi-node selection (drag across nodes)
    pub text_selections: BTreeMap<DomId, TextSelection>,

    // KEY ADDITION: automatic dirty flag
    pub display_list_dirty: bool,
}

pub struct TextEditState {
    pub selection: Option<Selection>,  // Cursor or Range
    pub dom_id: DomId,
    pub node_id: NodeId,
    pub contenteditable_key: u64,
}
```

### Key design principle

**Every mutation sets `display_list_dirty = true`.** The event loop checks this
after processing events and calls `regenerate_display_list_for_dom()` if true.
This eliminates Bug D permanently — you cannot move the cursor without the display
list being regenerated.

### How each bug is fixed

**Bug A (focus ring)**: `SetFocus` handler uses `apply_focus_restyle` return value
instead of ignoring it. (Independent fix, not part of merge.)

**Bug B+H (cursor at wrong position)**: `on_click()` sets BOTH cursor position
AND selection anchor in one call. No second manager to forget.

**Bug C (resize reverts)**: Separate fix — either clear dirty_text_nodes on rebuild
(document as intended behavior) or apply dirty content back into new StyledDom.

**Bug D (arrow hides text)**: `move_cursor()` sets `display_list_dirty = true`.
Event loop automatically regenerates display list.

**Bug E (backspace)**: `has_selection()` returns true when there's a cursor (not
just a range). Backspace handler gets `Selection::Cursor` and deletes one char.

**Bug G (no selection)**: Selection and cursor are the same `Selection` enum.
Click records state, drag extends it, all in one manager.

---

## Migration Plan

### Step 1: Create `layout/src/managers/text_edit.rs`

New file with `TextEditManager`, `TextEditState`, `CursorRenderInfo`. Implement
all methods: `on_focus_enter`, `on_focus_leave`, `on_click`, `move_cursor`,
`get_editing_selection`, `after_text_edit`, `take_display_list_dirty`.

### Step 2: Add field to LayoutWindow

```rust
pub text_edit_manager: TextEditManager,
```

Keep old managers as deprecated during migration.

### Step 3: Migrate mouse click handler

Replace `process_mouse_click_for_selection` (50+ lines) with single
`text_edit_manager.on_click()` call.

### Step 4: Migrate cursor movement

Replace `handle_cursor_movement` (50+ lines) with
`text_edit_manager.move_cursor()`.

### Step 5: Migrate text changeset

Replace cursor/selection gathering in `apply_text_changeset` with
`text_edit_manager.get_editing_selection()`.

### Step 6: Fix delete_selection

Handle `Selection::Cursor` case (delete one char) in addition to
`Selection::Range` (delete range).

### Step 7: Add display_list_dirty check to event loop

After all system changes:
```rust
if layout_window.text_edit_manager.take_display_list_dirty() {
    layout_window.regenerate_display_list_for_dom(dom_id);
    result = result.max(ShouldUpdateDisplayListCurrentWindow);
}
```

### Step 8: Migrate remaining call sites (~30)

Replace all `cursor_manager` and `selection_manager` references in event.rs
with `text_edit_manager` calls.

### Step 9: Remove old managers

Delete `cursor.rs`, gut `selection.rs`.

---

## Quick Fixes (Before Full Refactor)

These can be done NOW without the full merge:

1. **Bug A**: Use `apply_focus_restyle` return value (1 line in event.rs:2461)
2. **Bug B+H**: Add `cursor_manager.set_cursor_with_time()` call in
   `process_mouse_click_for_selection` (3 lines)
3. **Bug E**: Remove `has_selection()` guard in core/events.rs:2785 (5 lines)
4. **Bug D**: Call `regenerate_display_list_for_dom()` in `handle_cursor_movement`

---

## Files

| Component | File |
|-----------|------|
| New TextEditManager | `layout/src/managers/text_edit.rs` (NEW) |
| LayoutWindow | `layout/src/window.rs` |
| Event processing | `dll/src/desktop/shell2/common/event.rs` |
| Core event filter | `core/src/events.rs` |
| Old CursorManager | `layout/src/managers/cursor.rs` (DELETE) |
| Old SelectionManager | `layout/src/managers/selection.rs` (GUT) |
