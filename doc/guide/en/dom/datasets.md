---
slug: dom/datasets
title: Datasets
language: en
canonical_slug: dom/datasets
audience: external
maturity: mature
guide_order: 32
topic_only: false
short_desc: Attaching state to a node for navigation and per-instance state
prerequisites: [dom]
tracked_files:
  - core/src/dom.rs
  - core/src/callbacks.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:53:30Z
---

# Datasets

A dataset is a `RefAny` attached to a single node. You attach it with
`Dom::with_dataset(OptionRefAny)`. The framework doesn't read the
contents. It stores the `RefAny` on the node and hands it back to your
callback through `info.get_dataset(node_id)`.

That's the whole mechanism. Everything on this page follows from it.

The dataset is the slot for state that exists only because a specific
node exists. The cursor inside one text input. The expanded flag on one
tree row. A marker that tells a shared callback which button fired.
When `layout()` returns a fresh tree, the old node's dataset is dropped
along with the node. The dataset's lifetime is the node's lifetime.

There are two main use cases.

The first is **marker structs**: zero-field types whose only job is to
identify the node so a shared callback can dispatch on type. The second
is **per-instance state**: a struct of fields the callback reads and
mutates on every interaction.

For widgets that own resources expensive to recreate (a video decoder,
a GL texture, a websocket), the dataset is paired with a merge callback
so the framework can carry the `RefAny` across reconciliation instead
of dropping and rebuilding it. That mechanism has its own page. See
[Merge Callbacks](merge-callbacks.md).

## Attaching a dataset

The signature is `Dom::with_dataset(OptionRefAny) -> Dom`. Pass
`OptionRefAny::Some(RefAny::new(my_value))` to attach. Pass
`OptionRefAny::None` to leave the slot empty.

```rust,no_run
use azul::prelude::*;

struct EditorState { 
    text: String, 
    cursor: usize 
}

let state = RefAny::new(EditorState { 
    text: "hello".into(), 
    cursor: 0 
});

let _ = Dom::create_input_no_a11y("text".into(), "editor".into(), "hello".into())
    .with_dataset(OptionRefAny::Some(state));
```

`NodeData::set_dataset` is the underlying setter if you build
`NodeData` directly.

## Reading a dataset in a callback

Inside a callback, `info.get_hit_node()` returns the `DomNodeId` of the
node the event landed on. `info.get_dataset(node_id)` returns the
`RefAny` attached to that node, or `None`.

```rust,no_run
use azul::prelude::*;

struct EditorState { 
    text: String, 
    cursor: usize 
}

extern "C" fn on_keydown(_unused: RefAny, mut info: CallbackInfo) -> Update {
    let hit = info.get_hit_node();
    let mut ds = match info.get_dataset(hit) {
        Some(d) => d,
        None => return Update::DoNothing,
    };
    let mut state = match ds.downcast_mut::<EditorState>() {
        Some(s) => s,
        None => return Update::DoNothing,
    };
    state.text.push_str("...");
    Update::RefreshDom
}
```

Borrow rules follow `RefAny`. A `downcast_ref` blocks a `downcast_mut`.
Drop the guard before calling anything that may borrow the same
dataset. The full rules are in
[Understanding RefAny](../architecture/understanding-refany.md).

## Marker structs

A dataset doesn't have to carry data. A zero-field struct works as a
type-level tag. One callback handles many nodes. The dataset's *type*
tells the callback which node fired.

```rust,no_run
use azul::prelude::*;

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

extern "C" 
fn on_dialog_click(_unused: RefAny, mut info: CallbackInfo) -> Update {
    let hit = info.get_hit_node();
    let mut ds = match info.get_dataset(hit) {
        Some(d) => d,
        None => return Update::DoNothing,
    };
    if ds.downcast_ref::<SaveButtonMarker>().is_some() {
        // Save path.
    } else if ds.downcast_ref::<CancelButtonMarker>().is_some() {
        // Cancel path.
    }
    Update::RefreshDom
}
```

Both buttons share `on_dialog_click`. Dispatch happens on the dataset
type. There's no string match on a class name. There's no second
hit-test pass.

A marker can also carry fields. That's how a single callback handles a
whole table.

```rust,no_run
use azul::prelude::*;

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

extern "C" 
fn on_cell_click(_unused: RefAny, mut info: CallbackInfo) -> Update {
    let hit = info.get_hit_node();
    let mut ds = match info.get_dataset(hit) {
        Some(d) => d,
        None => return Update::DoNothing,
    };
    let marker = match ds.downcast_ref::<RowMarker>() {
        Some(m) => m,
        None => return Update::DoNothing,
    };
    let _ = marker.row_id;
    let _ = marker.column;
    Update::RefreshDom
}
```

`marker.row_id` and `marker.column` identify the cell directly. No tree
walk. No DOM query.

## Walking the tree from a hit node

`CallbackInfo` exposes a small set of navigators. Each takes a
`DomNodeId` and returns `Option<DomNodeId>`. They read the styled-DOM
hierarchy.

```rust,ignore
let hit = info.get_hit_node();

let parent = info.get_parent(hit);
let next   = info.get_next_sibling(hit);
let prev   = info.get_previous_sibling(hit);
let first  = info.get_first_child(hit);
let last   = info.get_last_child(hit);
```

`info.get_dataset(other_node_id)` works against any node, not just the
hit node. A click on a child cell can read the parent row's dataset
without any application-side mapping.

`info.get_focused_node()` returns the node that currently holds
keyboard focus, useful for keyboard-driven callbacks where there's no
mouse hit.

## Datasets are rebuilt every frame

Each `layout()` call returns a fresh `Dom`. Each `with_dataset` call
inside that `layout()` builds a fresh `RefAny`. The previous tree's
nodes (and their datasets) get dropped.

This has consequences.

Anything a callback writes into a dataset is gone after the next
`layout()` unless the callback also writes it back into the
application-level `RefAny`. The dataset is short-term scratch. The
application data is the system of record.

Marker structs are cheap. A zero-field marker has no destructor body.
The allocation cost is the price of attaching it.

Heavy resources don't belong in a dataset that gets rebuilt. Use a
merge callback to carry them across reconciliation. See
[Merge Callbacks](merge-callbacks.md).

There's also a related pattern called the **double update**. A text
input callback writes the new character to the application data and
also writes it directly into the node's dataset, so the input shows
the latest keystroke before the next `layout()` runs. That's covered
in [Text Input and Selection](../text-input.md).

## What CallbackInfo exposes for dataset work

The handful of methods that matter on this page:

- `info.get_hit_node()` returns the `DomNodeId` of the node the event
  landed on.
- `info.get_dataset(node_id)` returns the dataset `RefAny` attached to
  any node, or `None`.
- `info.get_focused_node()` returns the focused node id when there is
  one.
- `info.get_parent` / `get_next_sibling` / `get_previous_sibling` /
  `get_first_child` / `get_last_child` walk the styled-DOM hierarchy.
- `info.get_string_contents(node_id)` returns the node's resolved text
  content, useful for reading what was typed into an editable node.

Drop any active dataset borrow before calling another method that
might re-borrow the same `RefAny`.


## Coming Up Next

- [Merge Callbacks](merge-callbacks.md) — How widgets keep heavy resources across a layout rebuild
- [Virtual Views](virtual-views.md) — A node that materialises lazily, for infinite lists and embedded sub-DOMs
- [Components](components.md) — Reusable UI fragments - named functions of (args) -> Dom
- [Events](../events.md) — Callbacks, event filters, and how state triggers relayout
