---
slug: callbacks
title: Callbacks
language: en
canonical_slug: callbacks
audience: external
maturity: mature
guide_order: 31
topic_only: false
short_desc: What CallbackInfo exposes — state, DOM mutation, focus, async work
prerequisites: [dom]
tracked_files:
  - core/src/callbacks.rs
  - layout/src/callbacks.rs
default-search-keys:
  - CallbackInfo
  - RefAny
  - Update
  - EventFilter
  - FocusTarget
  - KeyboardState
  - MouseState
  - DomNodeId
---

# Callbacks

[DOM](dom.md) covered how to attach callbacks to a node. This page
covers the other side of the wire: what your callback function
receives, what it can read, what it can change, and what it returns.

## The callback signature

Every callback the framework invokes has the same C-compatible
signature:

```rust,ignore
extern "C" fn(data: RefAny, info: CallbackInfo) -> Update
```

- `data` is the `RefAny` you passed to `with_callback`. Downcast it
  to your concrete type to read or mutate application state.
- `info` is a borrowed view into the framework's frame state — the
  hit node, the current input state, the layout result, and a
  handful of dispatch helpers.
- `Update` tells the framework what to do next: nothing, re-run
  `layout()` for this window, or re-run for every window.

```rust,no_run
use azul::prelude::*;

struct Counter { value: i64 }

extern "C" fn on_click(mut data: RefAny, _info: CallbackInfo) -> Update {
    let mut c = match data.downcast_mut::<Counter>() {
        Some(c) => c,
        None => return Update::DoNothing,
    };
    c.value += 1;
    Update::RefreshDom
}
```

The `extern "C"` is mandatory. Callbacks are C function pointers,
which is what makes the FFI bindings (Python, JavaScript, C#, …) work
the same way.

## The Update return

`Update` has three values:

| Variant | Meaning |
|---|---|
| `DoNothing` | No re-layout, no re-render. Use this when the callback only mutates internal state that doesn't show up on screen yet. |
| `RefreshDom` | Re-run `layout()` for the window the event came from. The framework reconciles old vs new tree. |
| `RefreshDomAllWindows` | Re-run `layout()` for every open window. Use sparingly — pick this when the change touches global state that every window's layout reads. |

If a single event fans out to multiple callbacks (e.g. propagation
from child to parent), the framework takes the strongest `Update`
across all of them.

## Reading application state

`RefAny::downcast_ref::<T>()` and `downcast_mut::<T>()` recover the
typed payload. The downcast checks the type id, so passing the wrong
type returns `None` rather than reinterpreting memory.

```rust,ignore
extern "C" fn save(data: RefAny, _info: CallbackInfo) -> Update {
    let model = match data.downcast_ref::<AppModel>() {
        Some(m) => m,
        None => return Update::DoNothing,  // wrong RefAny type
    };
    write_to_disk(&*model);
    Update::DoNothing
}
```

For a callback that needs to mutate, use `downcast_mut`. The mutable
borrow lasts the callback body.

## Identifying the node that fired

```rust,ignore
let hit: DomNodeId = info.get_hit_node();           // DomId + NodeId
let rect = info.get_node_rect(hit);                 // optional
let css = info.override_node_css_properties(hit, …); // example mutation
```

A `DomNodeId` is `(DomId, NodeId)`. Most apps have a single root DOM
(`DomId::ROOT_ID`). Sub-DOMs come from `IFrame` nodes and virtual
views.

For a generic callback that fires from many call sites — say a
"submit" callback that's attached to several forms — the dataset is
the cleanest way to identify which instance fired:

```rust,ignore
let me = match info.get_dataset(info.get_hit_node()) {
    Some(d) => d,
    None => return Update::DoNothing,
};
let row = match me.downcast_ref::<TableRow>() {
    Some(r) => r,
    None => return Update::DoNothing,
};
```

`get_node_id_of_root_dataset(search_key)` walks up from the hit node
to find the nearest ancestor whose dataset matches `search_key`.
Useful for "click anywhere on this card" patterns where the card
root holds the dataset and the actual click landed on a label inside.

## Reading input state

The framework hands you the input state at event time. Rules of
thumb:

- `info.get_current_keyboard_state()` — modifier keys, currently
  pressed scancodes, the chars the platform reports for the most
  recent key event.
- `info.get_current_mouse_state()` — button state, scroll delta,
  whether the cursor is captured.
- `info.get_previous_keyboard_state()` / `get_previous_mouse_state()`
  — the snapshot from the *previous* frame. Useful for transition
  detection (`pressed_now && !pressed_last_frame == "just pressed"`).

For position queries:

```rust,ignore
info.get_cursor_position_screen()        // LogicalPosition relative to screen
info.get_cursor_relative_to_viewport()   // LogicalPosition in window coords
info.get_cursor_relative_to_node()       // (node_id, LogicalPosition) for the hit node
```

## Mutating the DOM without rebuilding

You don't have to return `RefreshDom` for small changes. The
framework exposes targeted mutations on `info` that go through a
faster path:

- `info.change_node_text(node_id, text)` — replaces the text content of
  a node.
- `info.change_node_image(node_id, image_ref, ...)` — swaps an image.
- `info.set_css_property(node_id, prop)` / `override_node_css_properties(...)`
  — set or override a CSS declaration without re-running the cascade
  for the whole tree.
- `info.change_node_image_mask(node_id, mask)` — update a clip mask.

These produce a `CallbackChange` queued on the info; the framework
applies them between the callback returning and the next paint. The
restyle and damage-rect machinery covered in
[Reconciliation](dom/reconciliation.md) keeps the work proportional
to what changed.

For structural edits (insert a child, delete a node), use
`insert_child_node` and `delete_node`. Larger changes — anything
beyond a handful of nodes — are usually clearer expressed as a fresh
`Dom` from `layout()` plus `Update::RefreshDom`.

## Focus, scroll, cursor

```rust,ignore
info.set_focus(FocusTarget::Node(node_id));         // focus a specific node
info.set_focus(FocusTarget::Path(/* ... */));       // by selector path
info.scroll_to(node_id, position, alignment);
info.scroll_node_into_view(node_id);
info.is_node_focused(node_id);
info.set_cursor_visibility(false);
info.start_cursor_blink_timer();
```

A focus change adjusts which node receives keyboard input on the next
frame. The reconciler migrates the focus across a `RefreshDom` for
nodes that match.

For text inputs and contenteditable surfaces, the cursor and
selection helpers (`add_cursor`, `add_selection_range`,
`get_primary_selection`, …) are documented separately in
[Text Selection](text-selection.md).

## Stopping propagation

Events bubble from the hit node to the root by default. Two opt-outs:

- `info.stop_propagation()` — finish the current node's callbacks,
  then stop. Other callbacks attached to *this* node still run.
- `info.stop_immediate_propagation()` — stop right now. No further
  callbacks at this node, no parents.

`info.prevent_default()` is the third opt-out: it tells the framework
not to apply the built-in handling for the event (e.g. don't insert a
character on `KeyDown` after your callback handled it). Browsers use
the same name for the same idea.

Event filtering — `EventFilter::Hover(...)` vs `Focus(...)` vs
`Window(...)`, propagation order, NotEvent — is in
[Events and Input](events.md).

## Async work: timers and threads

The callback runs on the UI thread. Any work it does blocks the next
frame. For anything slow, schedule it.

```rust,ignore
let timer = Timer::new(/* interval */ 100.ms, refany.clone(), tick);
info.add_timer(TimerId::unique(), timer);
```

`add_timer` registers a recurring callback the framework drives on
the main loop. The timer callback returns a
`TimerCallbackReturn { update, terminate }` that controls both
whether to re-run layout and whether the timer fires again.

For background work, `add_thread` spawns a worker thread tied to a
`RefAny`. The thread sends messages back to a `merge_callback` on the
main thread — the framework already understands cross-thread message
delivery, so you don't need a manual mutex. See
[Background Tasks](background-tasks.md).

## Window control

```rust,ignore
info.create_window(WindowCreateOptions::new(layout_fn));
info.close_window();                          // close the current window
info.modify_window_state(new_state);          // resize, retitle, fullscreen, ...
info.begin_interactive_move();                // start an OS-level drag
info.queue_window_state_sequence(states);     // animate state changes
```

Routing across pages is done with `switch_route(pattern, params)`,
which updates the active route and re-runs layout. Read the current
route with `get_route_pattern` / `get_route_param`.

## Image and font caches

```rust,ignore
info.add_image_to_cache("logo".into(), image_ref);
info.remove_image_from_cache("logo".into());
info.reload_system_fonts();
```

Cached images are addressable by name from any layout pass. Reloading
system fonts is the right thing to do after a font config change
(rare, but desktop environments do change font defaults at runtime).

## Layout queries

`CallbackInfo` exposes the post-layout geometry of every node — the
same data the renderer reads. Useful for hit-testing your own widgets
or implementing "click on the row but only outside the buttons":

```rust,ignore
info.get_node_size(node_id);            // LogicalSize
info.get_node_position(node_id);        // LogicalPosition (in viewport coords)
info.get_node_rect(node_id);            // LogicalRect = position + size
info.get_node_hit_test_bounds(node_id); // includes overflow padding
info.get_hit_node_rect();               // rect of the node that fired
```

For deeper tree walks, `get_parent_node`, `get_first_child_node`,
`get_all_children_nodes`, and `get_children_count` give you the same
hierarchy the framework uses internally.

## Working with sub-DOMs

`info.trigger_virtual_view_rerender(dom_id, node_id)` re-runs the
virtual-view callback for one specific sub-DOM. Use it when the
virtual view's source data changed but the parent layout hasn't.

`info.update_image_callback(dom_id, node_id)` triggers a re-render
of an `ImageCallback`-backed node — the GPU canvas pattern documented
in [SVG and Canvas](images/svg.md).

## A complete example

A "delete row" button that lives inside a row's dataset, finds its
row, removes it from the model, and refreshes:

```rust,no_run
use azul::prelude::*;

struct App { rows: Vec<String> }

#[repr(C)]
struct RowMarker { index: usize }

extern "C" fn on_delete(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let mut app = match data.downcast_mut::<App>() {
        Some(a) => a,
        None => return Update::DoNothing,
    };

    let row_node = match info.get_node_id_of_root_dataset(
        RefAny::new(RowMarker { index: 0 }) // type-id only, value is ignored
    ) {
        Some(n) => n,
        None => return Update::DoNothing,
    };
    let marker = match info.get_dataset(row_node)
        .and_then(|d| d.downcast_ref::<RowMarker>().map(|r| r.index))
    {
        Some(i) => i,
        None => return Update::DoNothing,
    };

    if marker < app.rows.len() {
        app.rows.remove(marker);
    }

    Update::RefreshDom
}
```

The pattern — dataset on the row, generic callback that walks up to
find its row marker, mutate the model, return `RefreshDom` — works for
nearly every "this widget acts on its container" interaction.

## Coming Up Next

- [Events and Input](events.md) — Event filters, propagation, NotEvent
- [Background Tasks](background-tasks.md) — Timers, threads, and merge callbacks
- [Datasets](dom/datasets.md) — Per-node state and the navigation patterns that read it
- [Reconciliation](dom/reconciliation.md) — How RefreshDom maps old to new
