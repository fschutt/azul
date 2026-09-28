---
slug: events/text-selection
title: Text Selection
language: en
canonical_slug: events/text-selection
audience: external
maturity: wip
guide_order: 63
topic_only: false
short_desc: Selection ranges, multiple cursors, remote selections and the rich clipboard
prerequisites: [events, events/text-input]
tracked_files:
  - core/src/hit_test.rs
  - core/src/selection.rs
  - layout/src/widgets/text_input.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T12:00:00Z
default-search-keys:
  - CallbackInfo
  - Selection
  - SelectionState
  - SelectionRange
  - SelectionVec
  - SelectionRangeVec
  - SelectionId
  - GraphemeClusterId
  - CursorAffinity
  - TextCursor
  - DomNodeId
---

# Text Selection

## Introduction

*WIP.* Selection rendering and the per-frame management code are wired but the high-level mouse-driven flow is partially implemented. Cross-node selection works in the data model but is not yet rendered for every layout case; APIs may change.

Selection in Azul follows the W3C Selection API model: a directed range with an **anchor** (where the user pressed) and a **focus** (where the user is now). The range can span any subtree of the DOM, not just the inside of a single text input.

## Positions: TextCursor

A position in editable text is a `TextCursor`:

```rust,ignore
use azul::prelude::*;
let cursor = TextCursor {
    cluster_id: GraphemeClusterId { source_run: 0, start_byte_in_run: 5 },
    affinity: CursorAffinity::Leading,
};
```

`GraphemeClusterId` is a stable, logical pointer into the original inline content. It survives Bidi reordering and line breaking. `CursorAffinity` disambiguates the two visual positions a single logical index can have.

- `Leading`. In LTR text, the left edge of the cluster. In RTL text, the right edge.
- `Trailing`. In LTR text, the right edge of the cluster. In RTL text, the left edge.

The pair `(cluster, Leading)` and `(previous-cluster, Trailing)` describe the same visual point, but only one is correct after a line wrap or a Bidi run boundary.

## Ranges: SelectionRange and Selection

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

Every read is scoped to a DOM, because a window can have several - a
main document and an iframe each have their own selection:

```rust,ignore
let any     = info.has_any_selection();               // anywhere in the window
let here    = info.has_selection(dom_id);
let state   = info.get_selection(dom_id);             // OptionSelectionState
let ranges  = info.get_selection_ranges(dom_id);
let n       = info.get_selection_count(dom_id);
```

`SelectionState` carries the node and the active selections on it.
Per-node questions - `node_has_selection(node)` and
`get_node_selection_ranges(node)` - are on
[Node Tree & Hit Testing](node-tree.md).

To respond to selection changes, register a callback on
`Hover(MouseUp)` or on `FocusEventFilter::FocusReceived` and read the
selection from it:

```rust,no_run
use azul::prelude::*;

extern "C" fn on_select(_data: RefAny, info: CallbackInfo) -> Update {
    if let Some(state) = info.get_selection(DomId::ROOT_ID).into_option() {
        let _ranges = &state.selections;
        // ... update UI ...
    }
    Update::DoNothing
}
```

## Multiple cursors

The model is multi-cursor from the ground up; a single caret is just
the case where there is one.

```rust,ignore
let id = info.add_cursor(dom_id, node_id, cursor);              // a caret
let id = info.add_selection_range(dom_id, node_id, range);      // a range
info.remove_selection_by_id(id);
```

Both return a `SelectionId` that stays stable across edits, which is
what lets you remove or update one cursor out of many later.

`get_primary_cursor(dom_id)` is the main caret - the one that scrolls
into view and that typing follows - and `get_primary_selection(dom_id)`
the selection belonging to it. `get_multi_cursor_selections(dom_id)`
returns every cursor with its id, which is what a "Select all
occurrences" command produces and what your rendering must then handle.

`set_selection(dom_id, node_id, selection)` replaces the whole
selection state at once, which is the right call when you are setting a
selection rather than adding to one.

`process_text_selection_click(position, time_ms)` feeds a click into
the selection state machine, and the timestamp is why: it is what turns
two clicks into a word selection and three into a line.

## Other users' selections

A collaborative editor shows where everyone else is. Remote selections
are owned, coloured and drawn by the framework rather than by your DOM:

```rust,ignore
info.set_selection_owner_color(owner, ColorU::from_str("#e2725b"));
info.set_remote_selections(owner, selections);
// on disconnect:
info.clear_remote_selections(owner);
info.clear_selection_owner_color(owner);
```

Each `SelectionOwner` is one remote participant. Because the framework
paints them, they behave like real selections - they reflow with the
text, survive edits and do not need a parallel overlay of absolutely
positioned rectangles.

`get_document_selection()` returns the selection in document
coordinates, as spans, which is the form to send over the wire: node
ids mean nothing on the other machine.

## Copy, cut, paste

The clipboard carries `ClipboardContent`, not a string - rich text,
HTML and images travel alongside the plain-text fallback, in both
directions:

```rust,ignore
let content = info.get_clipboard_content();        // OptionClipboardContent
info.set_clipboard_content(content);
```

To customise what Copy and Cut produce, set the content for the target
node before the default action runs:

```rust,ignore
info.set_copy_content(node, content);
info.set_cut_content(node, content);
```

The default Ctrl+C / Ctrl+X / Ctrl+V keystrokes copy the current
selection, cut it, or paste at the caret. `inspect_copy_changeset()`,
`inspect_cut_changeset()` and `inspect_paste_target_range()` compute
what each would do without doing it - see the `inspect_` family in
[Text Input](text-input.md).

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

## More methods

**Reading** - `has_any_selection`, `has_selection`, `get_selection`,
`get_selection_ranges`, `get_selection_count`,
`get_document_selection`.

**Cursors and ranges** - `add_cursor`, `add_selection_range`,
`set_selection`, `set_select_all_range`, `remove_selection_by_id`,
`get_primary_cursor`, `get_primary_selection`,
`get_multi_cursor_selections`, `process_text_selection_click`.
`set_select_all_range(target, range)` defines what Ctrl+A selects on a
node, for a surface where "everything" is narrower than the node's
whole content.

**Remote selections** - `set_remote_selections`,
`clear_remote_selections`, `set_selection_owner_color`,
`clear_selection_owner_color`.

**Clipboard** - `get_clipboard_content`, `set_clipboard_content`,
`set_copy_content`, `set_cut_content`.
