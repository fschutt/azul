---
slug: events/drag-and-drop
title: Drag & Drop
language: en
canonical_slug: events/drag-and-drop
audience: external
maturity: wip
guide_order: 67
topic_only: false
short_desc: Dragging nodes, MIME-keyed drag data, accepting drops, and files dragged in from the OS
prerequisites: [hello-world, events, events/callbacks]
tracked_files:
  - core/src/drag.rs
  - core/src/events.rs
  - layout/src/event_determination.rs
  - layout/src/callbacks.rs
default-search-keys:
  - CallbackInfo
  - DragData
  - DragEffect
  - DropEffect
  - DragState
  - accept_drop
  - set_drag_data
  - get_drag_data
  - get_dropped_files
---

# Drag & Drop

Drag and drop is two halves that never meet directly: a *source* that
says what is being dragged, and a *target* that says whether it will
take it. They communicate through a MIME-keyed payload, the same model
the W3C `DataTransfer` API uses.

## Introduction

The drag system covers selection drags, scrollbar thumbs, window moves,
window resizes, OS file drops and DOM-node drags. The framework handles
the first four itself. This page is about the last two, which are the
ones you write callbacks for.

*Dragging data out* of an azul window into another application is not
implemented on any platform. Everything else on this page works.

## Marking a node draggable

```rust,no_run
use azul::prelude::*;
let card = Dom::create_div()
    .with_attribute(AttributeType::Draggable(true))
    .with_css("padding: 12px; background: #fef;");
```

The gesture manager sees the attribute during hit testing and starts a
drag when the user begins one. Nothing else is needed to make a node
pick up.

## The event sequence

On the **source** node: `DragStart`, then `Drag` on every cursor move,
then `DragEnd` exactly once - on a successful drop, a rejected one, or a
cancel. `DragEnd` always fires, which makes it the only correct place to
undo optimistic UI.

On a **target** node: `DragEnter`, `DragOver` (throttled, repeated),
`DragLeave`, and `Drop` if the target accepted.

Each of these exists as `HoverEventFilter`, `FocusEventFilter` and
`WindowEventFilter`. Use the hover form for "this node is the drop
zone"; use the window form to observe drags anywhere in the window, for
example to highlight every valid target the moment a drag starts.

## Drag data

Set it on the source at `DragStart`, read it on the target at `Drop`:

```rust,ignore
// DragStart, on the source
info.set_drag_data("application/x-myapp-task".into(), payload_bytes);
info.set_drag_data("text/plain".into(), "Task 17".as_bytes().into());

// Drop, on the target
if let Some(bytes) = info.get_drag_data("application/x-myapp-task".into()).into_option() {
    // ...
}
```

Several MIME types per drag are allowed and the usual pattern is two:
your own structured type for drops inside the app, and `text/plain` so
the same drag means something to a foreign target.

`get_drag_types()` lists the types the current drag carries. It is
readable during `DragOver`, which is the point - a target decides
whether to accept *before* it is allowed to read the payload, exactly as
in the browser. `get_drag_data()` returns nothing until `Drop`.

## Accepting a drop

Targets opt in. A node that never calls `accept_drop()` is not a drop
target: the cursor shows the no-drop indicator and `Drop` never fires.

```rust,ignore
// DragOver, on the target
if info.get_drag_types().iter().any(|t| t.as_str() == "application/x-myapp-task") {
    info.accept_drop();
    info.set_drop_effect(DropEffect::Move);
}
```

`accept_drop()` is the equivalent of `event.preventDefault()` in a W3C
`dragover` handler, and the same trap applies: accepting only in
`DragEnter` is not enough, because a later `DragOver` that does not
accept revokes it.

## Effects

`DragEffect` is what the source *allows*, set at `DragStart`:
`Uninitialized`, `None`, `Copy`, `CopyLink`, `CopyMove`, `Link`,
`LinkMove`, `Move`, `All`.

`DropEffect` is what the target *chose*: `None`, `Copy`, `Link`, `Move`.

The chosen effect must be in the set the source allows, or the drop is
rejected. The effect also drives the cursor, so a target that means
"copy" but says `Move` will lie to the user.

## Styling during a drag

Two pseudo-classes, so drag feedback does not need callbacks:

```css
.card:dragging  { opacity: 0.4; }
.dropzone:drag-over { outline: 2px solid #38f; }
```

## Files dragged in from the OS

File drops arrive on the same pipeline, with their own events.
`HoverEventFilter::DroppedFile` fires when the drop lands, and the
hovered-file methods report what is over the window before it does:

```rust,no_run
use azul::prelude::*;

extern "C" fn on_dropped_file(_: RefAny, info: CallbackInfo) -> Update {
    for path in info.get_dropped_files().iter() {
        // ... open each file ...
    }
    Update::RefreshDom
}
```

`get_dropped_files()` is the full list and `get_dropped_file()` the
first of them; `get_hovered_files()` and `get_hovered_file()` are the
same pair while the drag is still in flight, which is what a drop zone
uses to preview "3 files" before the user lets go.
`is_file_drag_active()` distinguishes an OS file drag from an in-app
node drag, which matters because both are "a drag" to everything else.

## More methods

**Querying the drag** - `is_drag_active`, `is_dragging`,
`is_node_drag_active`, `is_file_drag_active`, `get_drag_state`,
`get_dragged_node`. `get_drag_state()` returns the whole picture at once
- drag type, source node, current drop target, and the file path for a
file drag - if you would rather read one struct than call four
predicates.

**Movement** - `get_drag_delta`, `get_drag_delta_screen`,
`get_drag_delta_screen_incremental`. The first is relative to the drag's
start in window coordinates, the second the same in desktop
coordinates, and the incremental form is the delta *since the previous
event* - which is the one a window-move or a resize handle wants, since
it never accumulates rounding error.

**Data and acceptance** - `set_drag_data`, `get_drag_data`,
`get_drag_types`, `accept_drop`, `set_drop_effect`.

**Files** - `get_dragged_file`, `get_dropped_file`, `get_dropped_files`,
`get_hovered_file`, `get_hovered_files`.

## Cross-references

- [Pointer, Pen & Touch](pointer.md) - pointer capture, which a
  hand-rolled drag needs and a `draggable` node does not.
- [Scrolling](scrolling.md) - drag auto-scroll runs on the scroll timer.
