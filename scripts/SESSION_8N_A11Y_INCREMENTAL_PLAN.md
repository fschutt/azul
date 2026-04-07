# Session 8N: Incremental A11y + Range Selection + Architecture

## Status After 8M

| Feature | Status |
|---------|--------|
| Wayland IME activation | **Done** — enable/disable/set_content_type/commit all wired up |
| A11y reads edited text | **Done** — dirty_text_overrides passed to tree builder |
| Cursor exposure | **Done** — set_text_selection with caret position |
| macOS action dispatch | **Done** — delegates to comprehensive handler |
| Range selection exposure | **TODO** — only caret exposed, not ranges |
| Incremental updates | **TODO** — full tree rebuilt on every keystroke |

---

## 1. Incremental A11y Updates

### Problem
`update_a11y_tree()` does 3 passes over the entire DOM on every keystroke.
For a 500-node DOM, this is ~1500 node iterations + HashMap allocations per
keypress. For text edits and cursor moves, only 1 node actually changes.

### Architecture

accesskit's `TreeUpdate` supports incremental updates:
- `tree: Option<Tree>` — can be `None` after first update
- `nodes: Vec<(NodeId, Node)>` — can contain only changed nodes
- Platform adapters diff internally and only fire events for changes

### Implementation

1. Add `tree_initialized: bool` to `A11yManager`
2. Add `push_node_update(node_id, node, focus) -> TreeUpdate` that creates
   a minimal single-node update with `tree: None`
3. For text-edit and cursor-move paths: call `push_node_update` instead of
   full `update_tree()`
4. For structural changes (RefreshDom, resize): use full `update_tree()`
   and set `tree_initialized = true`

### Call Site Strategy

| Trigger | Update Type |
|---------|-------------|
| `layout_and_generate_display_list()` | Full rebuild |
| `regenerate_display_list_for_dom()` | Incremental (cursor/text only) |
| Focus change | Incremental (focus field change) |
| Resize | Full rebuild (bounds change) |

---

## 2. Range Selection Exposure

### Problem
`CursorA11yInfo` only carries `cursor_offset: usize` (single position).
Selections (e.g. Shift+Arrow, double-click word select) have distinct
anchor and focus positions that should be exposed.

### Implementation

Expand `CursorA11yInfo`:
```rust
pub struct CursorA11yInfo {
    pub dom_id: DomId,
    pub node_id: NodeId,
    pub anchor_offset: usize,  // byte offset of selection anchor
    pub focus_offset: usize,   // byte offset of selection focus (== anchor for cursor)
}
```

In `update_a11y_tree()`, extract from `MultiCursorState.get_primary()`:
- `Selection::Cursor(c)` → anchor == focus == c.cluster_id.start_byte_in_run
- `Selection::Range(r)` → anchor = r.start, focus = r.end

In `update_tree()`, convert both offsets to UTF-16 character indices and
set `TextSelection { anchor, focus }` with potentially different positions.

---

## 3. Surrounding Text for Wayland IME

### Current State
`send_surrounding_text()` sends empty string `""` with cursor at 0.
This is protocol-valid but suboptimal — IMEs use surrounding text for
prediction and auto-correction.

### Fix (Low Priority)
Read the current text from `dirty_text_nodes` or `StyledDom` and send
the actual content with the correct cursor byte offset. Only matters
for CJK IMEs that use context for prediction.

---

## Execution Order

```
1. Range selection exposure    (small, 20 min)
2. Incremental a11y updates    (medium, 45 min)
3. Surrounding text for IME    (low priority, future)
```
