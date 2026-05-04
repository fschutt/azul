---
slug: dom/datasets
title: Datasets and Marker Structs
language: en
canonical_slug: dom/datasets
audience: external
maturity: mature
guide_order: 32
topic_only: false
short_desc: Attaching arbitrary state to a node — for navigation in callbacks, for ephemeral RefAnys, and as the slot widgets use to keep instance-local state.
prerequisites: [dom]
tracked_files:
  - core/src/dom.rs
  - core/src/callbacks.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:53:30Z
---

# Datasets and Marker Structs

The [`Dom` is frozen the moment you return it from `layout()`](../dom.md).
Application data lives in a `RefAny` you own; the tree is a fresh value
every time. So where do you put the state that exists only because *this
particular widget instance* exists — the cursor inside an `<input>`, the
expansion flag of a tree-view row, an "I am the save button"
self-identification a callback can use to navigate the new tree?

`Dom::with_dataset(OptionRefAny)` (`core/src/dom.rs:5145`) is the slot.
It stamps a `RefAny` onto a node. The framework doesn't read it; it just
hands it back to your callback when the user interacts with the node. The
dataset is your scratch slot for per-node, per-instance state — and, as
this page covers, also the canonical way to identify *which* node a
generic callback was fired from.

For widgets that hold *resources expensive to recreate* (a video decoder,
a GL texture, a websocket), the dataset pairs with a merge callback that
the framework runs during reconciliation. That side of the story is on
its own page: see [Merge Callbacks](merge-callbacks.md).

## What a dataset is

```rust,no_run
# use azul::prelude::*;
struct EditorState { text: String, cursor: usize }

let state = RefAny::new(EditorState { text: "hello".into(), cursor: 0 });
let _ = Dom::create_input_no_a11y("text".into(), "editor".into(), "hello".into())
    .with_dataset(OptionRefAny::Some(state));
```

The dataset is **read-write at callback time** via
`CallbackInfo::get_dataset(node_id)` — typically `info.get_hit_node()`
during a click handler:

```rust,no_run
# use azul::prelude::*;
# struct EditorState { text: String, cursor: usize }
extern "C" fn on_keydown(_unused: RefAny, mut info: CallbackInfo) -> Update {
    let hit = info.get_hit_node();
    let mut ds = match info.get_dataset(hit) { Some(d) => d, None => return Update::DoNothing };
    let mut state = match ds.downcast_mut::<EditorState>() { Some(s) => s, None => return Update::DoNothing };
    state.text.push_str("…");
    Update::RefreshDom
}
```

Borrows follow the [`RefAny` rules](../architecture/understanding-refany.md):
a `downcast_ref` blocks `downcast_mut` and vice versa. Drop the guard
before triggering anything that might re-enter the same dataset.

## Marker structs — datasets as navigation handles

A dataset doesn't have to *carry* state. It can just *identify* the
node, so a callback that runs at the page level can navigate to the
node it was fired against without selectors or hit-test math.

```rust,no_run
# use azul::prelude::*;
// Marker structs - they hold no fields, they exist only to identify a slot.
struct SaveButtonMarker;
struct CancelButtonMarker;

fn dialog_buttons() -> Dom {
    Dom::create_div()
        .with_child(
            Dom::create_button_no_a11y("Save".into())
                .with_dataset(OptionRefAny::Some(RefAny::new(SaveButtonMarker)))
                .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp),
                               RefAny::new(()), on_dialog_click))
        .with_child(
            Dom::create_button_no_a11y("Cancel".into())
                .with_dataset(OptionRefAny::Some(RefAny::new(CancelButtonMarker)))
                .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp),
                               RefAny::new(()), on_dialog_click))
}

extern "C" fn on_dialog_click(_unused: RefAny, mut info: CallbackInfo) -> Update {
    let hit = info.get_hit_node();
    let mut ds = match info.get_dataset(hit) { Some(d) => d, None => return Update::DoNothing };
    if ds.downcast_ref::<SaveButtonMarker>().is_some() {
        // Save path
    } else if ds.downcast_ref::<CancelButtonMarker>().is_some() {
        // Cancel path
    }
    Update::RefreshDom
}
```

Both buttons point at the same callback function. The callback uses the
dataset's *type* to dispatch — no string matching, no node-id juggling,
no `get_node_by_class("save-button")` round-trip through the styled DOM.
This is the cheapest possible "tell me which thing was clicked" channel.

The pattern composes upward. A dataset can hold a struct of fields too:

```rust,no_run
# use azul::prelude::*;
// One dataset, one callback, many rows.
#[derive(Debug)]
struct RowMarker {
    row_id: u64,
    column: ColumnKind,
}
#[derive(Debug, Copy, Clone)]
enum ColumnKind { Name, Email, Avatar, DeleteButton }

fn row(row_id: u64, kind: ColumnKind, label: &str) -> Dom {
    Dom::create_td()
        .with_child(Dom::create_text(label))
        .with_dataset(OptionRefAny::Some(RefAny::new(RowMarker { row_id, column: kind })))
        .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp),
                       RefAny::new(()), on_cell_click)
}

extern "C" fn on_cell_click(_unused: RefAny, mut info: CallbackInfo) -> Update {
    let hit = info.get_hit_node();
    let mut ds = match info.get_dataset(hit) { Some(d) => d, None => return Update::DoNothing };
    let marker = match ds.downcast_ref::<RowMarker>() { Some(m) => m, None => return Update::DoNothing };
    // marker.row_id and marker.column tell us exactly which cell fired.
    let _ = marker;
    Update::RefreshDom
}
```

The callback is one function for the whole table. The dataset narrows
the call site to "which row, which column" without traversing the tree.

## Walking the tree from a marker

Once you have a node id (typically `info.get_hit_node()`), `CallbackInfo`
exposes navigation getters that read the styled-DOM hierarchy:

```rust,ignore
let hit = info.get_hit_node();

// Up:
let parent  = info.get_parent(hit);

// Sideways (returns OptionDomNodeId):
let next    = info.get_next_sibling(hit);
let prev    = info.get_previous_sibling(hit);

// Down:
let first   = info.get_first_child(hit);
let last    = info.get_last_child(hit);
```

Combine the marker pattern with the navigators when a callback needs to
reach a *related* node — e.g. the row's "delete" button knows its
`row_id` from its marker, and walks up to the row container, then to the
row's avatar cell, all without needing to thread node ids through the
state model.

For deeper queries, `info.get_dataset(some_other_node_id)` works against
any node — useful when, e.g., a parent row's dataset holds the row's
data and a click on a child cell wants to read the parent's state.

## Ephemeral `RefAny` instances

A subtle but important point: the `RefAny` you hand to `with_dataset` is
**created during `layout()`** and lives only as long as the framework
holds the node. When the next `layout()` returns a fresh tree, the old
node and its dataset get dropped. Three implications:

- **The dataset is rebuilt every frame.** Anything you read from it in
  a callback must come back into application state via the callback's
  own `RefAny` — otherwise the next `layout()` call will overwrite it
  with whatever you put on the new node.
- **Marker structs cost nothing.** A zero-field struct wrapped in a
  `RefAny` is one allocation per node per frame, but the allocator is
  well-suited to that pattern, and the marker doesn't even need a
  destructor body.
- **Heavy resources need a merge callback.** If the dataset *owns*
  something expensive (a decoder, a GPU texture, a websocket), you
  don't want it to be rebuilt and the old one freed every frame. That
  is exactly what [Merge Callbacks](merge-callbacks.md) are for.

There is also a related pattern called the *double update*, where a
callback writes once to the application data and a second time to the
node-attached dataset to give an input field "what the user just typed"
semantics in the same frame. That shows up most prominently in text
input handling — covered in [Text Input and Selection](../text-input.md)
once that page lands; the gist is that the dataset is the only "hot"
slot a callback can touch *between* the application-data write and the
next `layout()`.

## Reading datasets in callbacks — the full surface

`CallbackInfo` exposes:

- **`info.get_dataset(node_id)`** — returns the node's dataset as a
  `RefAny`, or `None`. Borrow rules follow the underlying `RefAny`.
- **`info.get_hit_node()`** — node id of the node the event landed on,
  after event-filter propagation. The standard "which node was
  clicked" call.
- **`info.get_focused_node()`** — node id of whichever node currently
  has keyboard focus, useful for keyboard-driven callbacks.
- **`info.get_parent(node_id)`** / **`get_next_sibling`** /
  **`get_previous_sibling`** / **`get_first_child`** /
  **`get_last_child`** — hierarchy navigation; each returns an
  `OptionDomNodeId`.
- **`info.get_string_contents(node_id)`** — returns the node's text
  content (after layout has resolved any `Text` children), useful for
  reading what the user typed into an editable node.

Drop the dataset borrow before calling any of these that might
internally re-borrow the same `RefAny`.

## Where to read the source

- `core/src/dom.rs:1781` — `NodeDataExt.dataset: Option<RefAny>` slot
- `core/src/dom.rs:5145` — `Dom::with_dataset`
- `core/src/dom.rs:2503` — `NodeData::set_dataset`
- `core/src/dom.rs:2497` — `NodeData::get_dataset`
- `core/src/callbacks.rs` — `CallbackInfo::get_dataset` and the
  `get_*_sibling` / `get_first_child` / `get_last_child` navigators
