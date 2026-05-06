---
slug: text-selection
title: Text Selection
language: en
canonical_slug: text-selection
audience: external
maturity: wip
guide_order: 94
topic_only: false
short_desc: Selection ranges, cursors, and copy/paste
prerequisites: [events, text-input]
tracked_files:
  - core/src/hit_test.rs
  - core/src/selection.rs
  - layout/src/widgets/text_input.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T12:00:00Z
---

# Text Selection

> **WIP.** Selection rendering and the per-frame management code are wired but the high-level mouse-driven flow is partially implemented. Cross-node selection works in the data model but is not yet rendered for every layout case. APIs may change.

Selection in Azul follows the W3C Selection API model: a directed range with an **anchor** (where the user pressed) and a **focus** (where the user is now). The range can span any subtree of the DOM, not just the inside of a single text input.

## Positions: `TextCursor`

A position in editable text is a `TextCursor`:

```rust,ignore
use azul::prelude::*;
let cursor = TextCursor {
    cluster_id: GraphemeClusterId { source_run: 0, start_byte_in_run: 5 },
    affinity: CursorAffinity::Leading,
};
```

`GraphemeClusterId` is a stable, logical pointer into the original inline content. It survives Bidi reordering and line breaking. `CursorAffinity` disambiguates the two visual positions a single logical index can have.

| affinity | LTR text | RTL text |
|---|---|---|
| `Leading` | left edge of the cluster | right edge of the cluster |
| `Trailing` | right edge of the cluster | left edge of the cluster |

The pair `(cluster, Leading)` and `(previous-cluster, Trailing)` describe the same visual point, but only one is correct after a line wrap or a Bidi run boundary.

## Ranges: `SelectionRange` and `Selection`

```rust,ignore
use azul::prelude::*;
fn build(start: TextCursor, end: TextCursor) {
    let range = SelectionRange { start, end };
    let sel: Selection = Selection::Range(range);          // highlighted
    let caret: Selection = Selection::Cursor(start);       // blinking caret
}
```

A `Selection` is either `Cursor` (collapsed, a blinking caret) or `Range` (highlighted, a selection rectangle). Direction is implicit: `start` may be logically after `end` if the user dragged backwards.

## DOM order, not visual order

Selection always follows DOM tree order, even when the visual layout reverses it (`flex-direction: row-reverse`, `direction: rtl`). Dragging visually left-to-right across a `row-reverse` flex container still selects "the second sibling, then the first sibling" because that's the source order. The highlight rectangles are computed from visual positions, but the contents of the selection follow the tree.

This matches browser behaviour.

## Reading the current selection

`CallbackInfo` exposes the public selection API:

```rust,ignore
impl CallbackInfo {
    pub fn has_selection(&self) -> bool;
    pub fn node_has_selection(&self, node_id: DomNodeId) -> bool;
    pub fn get_selection(&self) -> Option<SelectionState>;
    pub fn get_selection_count(&self, node_id: DomNodeId) -> usize;
    pub fn get_selection_ranges(&self) -> SelectionRangeVec;
    pub fn get_node_selection_ranges(&self, node_id: DomNodeId) -> SelectionRangeVec;
}
```

`SelectionState` carries a `DomNodeId` and a `SelectionVec` of the active selections on that node. `SelectionRangeVec` is the FFI-friendly vector of `SelectionRange` values.

To respond to selection changes, register a callback on `Hover(MouseUp)` or on `FocusEventFilter::FocusReceived` and read `get_selection()` from the callback:

```rust,no_run
# use azul::prelude::*;
extern "C" fn on_select(_data: RefAny, info: CallbackInfo) -> Update {
    if let Some(state) = info.get_selection() {
        let _node = state.node_id;
        let _ranges = &state.selections;
        // ... update UI ...
    }
    Update::DoNothing
}
```

## Mutating selection

`CallbackInfo` lets you add, remove, or replace selections programmatically:

```rust,ignore
impl CallbackInfo {
    pub fn add_selection_range(&mut self, /* ... */);
    pub fn remove_selection_by_id(&mut self, id: SelectionId);
}
```

Each range carries a stable `SelectionId` so external code can refer to a specific selection across edits.

## Copy, cut, paste

Clipboard reads and writes go through `CallbackInfo`:

```rust,ignore
impl CallbackInfo {
    /// Read the OS clipboard.
    pub fn get_clipboard_content(&self) -> Option<String>;

    /// Write to the OS clipboard.
    pub fn set_clipboard_content(&mut self, text: String);

    /// Set the data to be copied when the user invokes Copy.
    pub fn set_copy_content(&mut self, text: String);

    /// Set the data to be cut when the user invokes Cut.
    pub fn set_cut_content(&mut self, text: String);

    /// Inspect what would be copied without actually copying.
    pub fn inspect_copy_changeset(&self) -> Option<String>;
    pub fn inspect_cut_changeset(&self) -> Option<String>;
    pub fn inspect_paste_target_range(&self) -> Option<SelectionRange>;
}
```

The default Ctrl+C / Ctrl+X / Ctrl+V keystrokes copy the current selection to the clipboard, cut it, or paste at the caret. To customise, register a callback for the keystroke and call `set_copy_content` / `set_cut_content` with your own payload before `prevent_default`.

## Painting the highlight

The painter renders selection highlights as rectangles behind the text. Selection updates flow through the same incremental display-list path as text edits (see [Text Input](text-input.md)), so extending a selection doesn't run the layout callback.

CSS `selection-background-color` and `selection-color` style the highlight:

```css
::selection {
    background-color: #b3d4fc;
    color: #000;
}
```

## Known limitations

- **Selection clears between drag frames** in some configurations. The legacy per-frame `clear_selection()` call hasn't been fully removed from the mouse-drag path; on affected platforms the highlight flickers during a drag.
- **Cross-node rendering is incomplete.** The data model is correct but the painter currently renders only the anchor's container in some layouts.
- **No primary-selection clipboard on Linux/X11.** Middle-click paste between Azul and other apps doesn't work yet.
- **No RTL-aware direction handling.** `direction: rtl` isn't yet considered when ordering the visual highlight rectangles for the first/last line.
- **No vertical writing mode.** `writing-mode: vertical-*` isn't respected by the selection axis.

## Next

- [Text Input](text-input.md): the editing flows that write into selections.
- [Events](events.md): the underlying mouse and keyboard event model.
