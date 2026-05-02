---
slug: text-selection
title: Text Selection
language: en
canonical_slug: text-selection
audience: external
maturity: wip
guide_order: 94
topic_only: false
short_desc: Selection ranges, cursor placement, copy / paste integration, and the selection API exposed to callbacks.
prerequisites: [events, text-input]
tracked_files:
  - core/src/hit_test.rs
  - core/src/selection.rs
  - layout/src/widgets/text_input.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T12:00:00Z
---

# Text Selection

> **WIP.** Selection rendering, hit-testing, and the per-frame management code are wired but the high-level mouse-driven flow (mousedown → set anchor, mousemove → update focus, mouseup → finalize) is partially implemented. Cross-IFC selection works in the data model but is not yet rendered for every layout case. APIs may change.

Selection in Azul follows the W3C Selection API model: a single, directed range with an **anchor** (where the user pressed the mouse) and a **focus** (where the user is now). The range can span any subtree of the DOM, not just the inside of a single text-input. The data structures live in `core/src/selection.rs`.

```rust,ignore
use azul_core::selection::*;
use azul_core::dom::{DomId, NodeId};
use azul_core::geom::{LogicalRect, LogicalPosition};
fn make(dom: DomId, node: NodeId, cursor: TextCursor, bounds: LogicalRect, mouse: LogicalPosition) {
    let sel = TextSelection::new_collapsed(dom, node, cursor, bounds, mouse);
    assert!(sel.is_collapsed());
}
```

## Positions: `TextCursor`, affinity, grapheme clusters

A position in editable text is a `TextCursor` (`core/src/selection.rs:93`):

```rust,ignore
use azul_core::selection::*;
let cursor = TextCursor {
    cluster_id: GraphemeClusterId { source_run: 0, start_byte_in_run: 5 },
    affinity: CursorAffinity::Leading,
};
```

`GraphemeClusterId` is a stable, logical pointer into the *original* `InlineContent` array — not into a flattened string. It survives Bidi reordering and line breaking. `CursorAffinity` disambiguates the two visual positions a single logical index can have:

| affinity | LTR text | RTL text |
|---|---|---|
| `Leading` | left edge of the cluster | right edge of the cluster |
| `Trailing` | right edge of the cluster | left edge of the cluster |

The pair `(GraphemeClusterId, Leading)` and `(previous-cluster, Trailing)` describe the same visual point, but only one of them is correct after a line wrap or a Bidi run boundary. The cursor renderer respects affinity when picking which line to draw on.

## Ranges: `SelectionRange` and `Selection`

```rust,ignore
use azul_core::selection::*;
fn build(start: TextCursor, end: TextCursor) {
    let range = SelectionRange { start, end };
    let sel: Selection = Selection::Range(range);          // highlighted
    let caret: Selection = Selection::Cursor(start);       // blinking caret
}
```

A `Selection` is either a `Cursor` (collapsed — a blinking caret) or a `Range` (highlighted — a selection rectangle). Direction is implicit: `start` may be logically after `end` if the user dragged backwards. The renderer normalises before drawing.

## Multi-node selection: `TextSelection`

A drag that crosses node boundaries cannot be a single per-node range. `TextSelection` (`core/src/selection.rs:584`) is the cross-DOM type:

```rust,ignore
use azul_core::selection::*;
use std::collections::BTreeMap;
use azul_core::dom::{DomId, NodeId};
fn build(dom: DomId, anchor: SelectionAnchor, focus: SelectionFocus, ranges: BTreeMap<NodeId, SelectionRange>) {
    let sel = TextSelection {
        dom_id: dom,
        anchor,
        focus,
        affected_nodes: ranges,
        is_forward: true,
    };
}
```

| field | meaning |
|---|---|
| `anchor` | `SelectionAnchor` — IFC root + cursor + visual `char_bounds` of the anchor character |
| `focus` | `SelectionFocus` — IFC root + cursor + current viewport mouse position |
| `affected_nodes` | `BTreeMap<NodeId, SelectionRange>` — one entry per IFC root that intersects the selection |
| `is_forward` | `true` if anchor is before focus in DOM order |

The map keys are **IFC root** node IDs — the nodes that actually own a `UnifiedLayout` (typically `<p>`, `<div>`, anything that establishes an inline-formatting context). The `BTreeMap` gives O(log N) lookup during render: the painter walks visible IFC roots and asks `selection.get_range_for_node(&id)` for each one.

## Anchor and focus

```rust,ignore
use azul_core::selection::*;
use azul_core::dom::NodeId;
use azul_core::geom::{LogicalRect, LogicalPosition};
fn build(node: NodeId, cursor: TextCursor, bounds: LogicalRect, pos: LogicalPosition) {
    let anchor = SelectionAnchor {
        ifc_root_node_id: node,
        cursor,
        char_bounds: bounds,
        mouse_position: pos,
    };
    let focus = SelectionFocus {
        ifc_root_node_id: node,
        cursor,
        mouse_position: pos,
    };
}
```

The anchor stays fixed during a drag; only the focus moves. `char_bounds` on the anchor records the visual rectangle of the character under the press — used by the renderer to compute the **logical selection rectangle** for multi-line and multi-node selections (so the highlight extends to the line edge for middle lines, not just to the focus X-coordinate).

## DOM order, not visual order

Selection always follows DOM tree order, even when the visual layout reverses it (`flex-direction: row-reverse`, `direction: rtl`). Dragging visually left-to-right across a `row-reverse` flex container still selects "the second sibling, then the first sibling" because that is the source order. The highlight rectangles are computed from visual positions, but the *contents* of the selection follow the tree.

This matches browser behaviour and means `TextSelection::is_forward` reflects DOM order, not screen direction.

## Mouse-driven selection lifecycle

The selection follows the W3C model:

| event | action |
|---|---|
| `MouseDown` | hit-test → grapheme position; create `SelectionAnchor`; set `focus = anchor` (collapsed) |
| `MouseMove` (with primary button held) | hit-test the current cursor → update `focus`; recompute `affected_nodes` |
| `MouseUp` | finalise — keep the selection until the next `MouseDown` clears it |

Recomputing `affected_nodes` on every move is the part that makes the model multi-node:

1. Sort anchor and focus into DOM order (`is_forward` records which way).
2. Walk the DOM from start to end, collecting IFC root nodes.
3. For each collected node, build a `SelectionRange`:
   - Anchor's IFC root: `start_offset = anchor.cursor`, `end_offset = end-of-text`.
   - Focus's IFC root: `start_offset = 0`, `end_offset = focus.cursor`.
   - In-between nodes: fully selected (`0 .. text_len`).
   - Same node for anchor and focus: partial range bounded by both cursors.

The full algorithm draft lives in `scripts/TEXT_SELECTION_ARCHITECTURE.md`. The `NodeSelectionType` enum from that draft (`Anchor`, `Focus`, `InBetween`, `AnchorAndFocus`) is intended to be carried alongside each `SelectionRange` so the renderer can pick the right highlight shape for partial vs full selection.

## `SelectionState` vs `MultiCursorState`

There are two storage types for a node's selection set, with different intended use sites:

| type | location | used by | notes |
|---|---|---|---|
| [`SelectionState`](#selectionstate-ffi-friendly-c-api) | `core/src/selection.rs:154` | C / FFI surface, `layout/src/managers/selection.rs` | `SelectionVec` (FFI-safe), simple add-and-sort merge |
| [`MultiCursorState`](text-input.md#multi-cursor) | `core/src/selection.rs:255` | internal Rust, `layout/src/managers/text_edit.rs` | `Vec<IdentifiedSelection>` with stable IDs, full merge logic |

### `SelectionState` (FFI-friendly, C API)

`SelectionState` is the type the C/Python bindings see. It carries a `DomNodeId` and an `SelectionVec` (the FFI-encoded vector). The `add()` method is currently a stub — it sorts and dedups but does not merge overlapping ranges. The internal Rust path uses `MultiCursorState` instead.

### `MultiCursorState` for editing

For text-editing flows (typing, arrow-key motion, multi-cursor) use `MultiCursorState` from the [text-input page](text-input.md#multi-cursor). It enforces the "sorted, non-overlapping" invariant, gives every cursor a stable `SelectionId`, and supports `move_all_cursors()` for batched motion.

## Hit testing for selection

A click that lands on text needs to map a viewport pixel to a grapheme position. The framework feeds the click through `FullHitTest` (`core/src/hit_test.rs:313`), which returns the per-DOM `HitTest` data:

```rust,ignore
use azul_core::hit_test::*;
fn walk(h: HitTest) {
    for (_node_id, item) in &h.regular_hit_test_nodes {
        let _: HitTestItem = *item;
        // item.point_in_viewport      — viewport coordinates
        // item.point_relative_to_item — local to the node
        // item.hit_depth              — z-order, 0 = topmost
    }
}
```

`HitTestItem::point_relative_to_item` is the input to the per-node text-position lookup: the IFC's `UnifiedLayout` converts a local `(x, y)` into a `(GraphemeClusterId, CursorAffinity)` pair. That pair, together with the IFC root node id, is exactly what `SelectionAnchor` needs.

`hit_depth` lets the selection code prefer the topmost text under the cursor when overlapping siblings exist (e.g. a tooltip drawn on top of the document). Lower values are closer to the user.

`CursorHitTestItem` (`core/src/hit_test.rs:37`) is a separate per-node entry that records the CSS `cursor:` value of the topmost element under the pointer — this is what changes the OS cursor icon to "I-beam" over editable text. It is decoupled from selection state; it only affects the cursor sprite.

## Painting the highlight

For each visible IFC root, the painter calls `selection.get_range_for_node(&ifc_root_id)`. If a `SelectionRange` is returned, the layout's `UnifiedLayout::get_selection_rects(range)` produces one or more `LogicalRect` values per visual line, and those rects are emitted into the display list behind the text. The selection rectangles are added to the same incremental display-list path as text edits ([Text Input](text-input.md#how-edits-avoid-a-full-re-layout)) — extending a selection does not run the layout callback.

## Known limitations

- **Selection clears between drag frames** in some configurations. The legacy per-frame `clear_selection()` call has not been fully removed from the mouse-drag path; on affected platforms the highlight flickers during a drag. The data model is correct; this is a renderer-side bug.
- **Cross-IFC rendering is incomplete.** `TextSelection::affected_nodes` is populated correctly but the painter currently renders only the anchor's IFC root in some layouts.
- **No primary-selection clipboard on Linux/X11.** The X11 backend does not yet copy the current selection into the PRIMARY selection on selection change; middle-click paste between Azul and other apps does not work.
- **No RTL-aware direction handling.** Direction is computed from `is_forward` alone; `direction: rtl` is not yet considered when ordering the visual highlight rectangles for the first/last line.
- **No vertical writing mode.** `writing-mode: vertical-*` is not respected by the selection axis.
- **`SelectionState::add()` is a stub.** It sorts and dedups but does not merge overlapping ranges. Use `MultiCursorState` for any real merge logic.

## Next

- [Text Input](text-input.md) — the editing flows that write into selections.
- [Events and Input](events.md) — the underlying mouse and keyboard event model.
