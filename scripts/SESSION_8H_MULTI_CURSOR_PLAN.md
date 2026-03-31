# Session 8H: Multi-Cursor / Multi-Selection System (Sublime Text Style)

## Overview

Replace the split CursorManager + SelectionManager with a unified `MultiCursorState`
that supports multiple simultaneous cursors/selections, each identified by a stable
`SelectionId` (monotonic u64 counter, not UUID — C-API friendly and Copy).

## New Data Structures

### SelectionId — Stable Identity

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct SelectionId(pub u64);

impl SelectionId {
    pub fn new() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        SelectionId(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}
```

### IdentifiedSelection — Selection with Stable ID

```rust
#[repr(C)]
pub struct IdentifiedSelection {
    pub id: SelectionId,
    pub selection: Selection,  // Cursor(TextCursor) or Range(SelectionRange)
}
```

### MultiCursorState — Replaces CursorManager + legacy SelectionState

```rust
pub struct MultiCursorState {
    /// Sorted by position, non-overlapping. Primary = last added (highest index).
    pub selections: Vec<IdentifiedSelection>,
    pub node_id: DomNodeId,
    pub contenteditable_key: u64,  // Survives DOM rebuilds
}
```

Key operations:
- `add_cursor(cursor) -> SelectionId` — merges if overlapping
- `add_selection(range) -> SelectionId` — merges overlapping
- `remove_selection(id) -> bool`
- `get_primary() -> Option<&IdentifiedSelection>` — last added
- `to_selections() -> Vec<Selection>` — for `edit_text()`
- `merge_overlapping()` — sorts + merges after any mutation

## TextEditManager Changes

```rust
pub struct TextEditManager {
    pub multi_cursor: Option<MultiCursorState>,  // Active contenteditable
    pub selection_manager: SelectionManager,       // Non-editable text drag-select
    pub blink_state: BlinkState,                   // Shared across all cursors
    pub preedit_text: Option<String>,              // Primary cursor only
    pub display_list_dirty: bool,
}
```

## apply_text_changeset — Multi-Selection

Pass all selections to `edit_text()` (already handles multiple back-to-front):

```rust
let current_selection = match &self.text_edit_manager.multi_cursor {
    Some(mc) => mc.to_selections(),
    None => vec![default_cursor_at_start()],
};
let (new_content, new_selections) = edit_text(&content, &current_selection, &edit);
mc.update_from_edit_result(&new_selections);
```

## Ctrl+V Paste — Intelligent Multi-Selection

```rust
let lines: Vec<&str> = paste_text.lines().collect();
if lines.len() == mc.selections.len() {
    // N lines → N selections: paste one line per selection
    edit_text_multi(content, selections, lines)
} else {
    // Broadcast: paste full text at each cursor
    edit_text(content, selections, TextEdit::Insert(paste_text))
}
```

Requires new `edit_text_multi(content, selections, texts_per_selection)`.

## Cursor Movement — All Cursors

Move ALL cursors in the same direction. Merge any that collide after movement.
With Shift: extend each selection's focus end.

## Ctrl+Click — Add Cursor

Hit-test click position → `mc.add_cursor(clicked_position)`.

## Ctrl+D — Select Next Occurrence

1. Primary selection → if cursor, expand to word
2. Get selected text string
3. Search forward for next occurrence
4. `mc.add_selection(occurrence_range)`

## Display List Changes

### LayoutContext

```rust
// Replace single cursor:
pub cursor_locations: Vec<(DomId, NodeId, TextCursor)>,
pub selection_ranges_for_node: BTreeMap<(DomId, NodeId), Vec<SelectionRange>>,
```

### paint_cursor — Iterate over all cursors
### paint_selections — Render from selection_ranges_for_node

## Scroll-Into-View

After any modification, scroll primary cursor into view (Sublime behavior).

## C API (CallbackInfo methods → api.json)

```c
AzSelectionId AzCallbackInfo_addCursor(info, dom_id, node_id, cursor);
AzSelectionId AzCallbackInfo_addSelectionRange(info, dom_id, node_id, range);
bool AzCallbackInfo_removeSelectionById(info, selection_id);
AzIdentifiedSelectionVec AzCallbackInfo_getAllSelections(info, dom_id);
AzOptionIdentifiedSelection AzCallbackInfo_getPrimarySelection(info, dom_id);
size_t AzCallbackInfo_getSelectionCount(info, dom_id);
AzSelectionIdVec AzCallbackInfo_selectAllOccurrences(info, dom_id, node_id, text);
```

## Known Bugs to Fix

1. **Arrow keys insert spaces** — Fixed: macOS function key chars (U+F700-F7FF) now
   filtered from text input path.

2. **Node ID mismatches in selection rendering** — `MultiCursorState` uses
   `contenteditable_key` for stable identity across DOM rebuilds.

3. **`delete_selection` doesn't delete range content** — Range branch sets zero-width
   selection but never calls `edit_text()`. Fixed by unified delete-as-selection
   refactor (Session 8G).

## Implementation Sequence

| Step | Files | Description |
|------|-------|-------------|
| 1 | `core/src/selection.rs` | Add SelectionId, IdentifiedSelection, MultiCursorState |
| 2 | `layout/src/managers/text_edit.rs` | Replace with MultiCursorState field |
| 3 | `layout/src/window.rs` | apply_text_changeset, handle_cursor_movement, delete_selection |
| 4 | `layout/src/text3/edit.rs` | Add edit_text_multi for per-selection paste |
| 5 | `layout/src/solver3/mod.rs` | LayoutContext cursor_locations: Vec |
| 6 | `layout/src/solver3/display_list.rs` | paint_cursor/paint_selections iterate |
| 7 | `dll/src/desktop/shell2/common/event.rs` | Ctrl+Click, Ctrl+D handlers |
| 8 | `layout/src/callbacks.rs` | add_cursor, add_selection_range API |
| 9 | `api.json` | C API bindings |

## Testing

- Unit: `edit_text` with 2-3 cursors, index adjustment
- Unit: `MultiCursorState::merge_overlapping`
- Unit: paste-N-lines-to-N-selections
- E2E: Ctrl+Click adds cursors, type "hello", verify all positions
- E2E: Select word, Ctrl+D twice, type replacement, verify all occurrences
