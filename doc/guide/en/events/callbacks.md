---
slug: events/callbacks
title: Callbacks
language: en
canonical_slug: events/callbacks
audience: external
maturity: mature
guide_order: 61
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

[DOM](../dom.md) showed how to attach a callback to a node. The other
side of the wire — what a callback receives, can read, can change,
and returns — is what this page documents.

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

## Finding the right method

`CallbackInfo` has around 350 methods. They fall into groups, and most
of them are documented on a page of their own:

| Group | Page |
|---|---|
| Finding, walking and editing nodes; hit testing; geometry | [Node Tree & Hit Testing](node-tree.md) |
| Cursor position, seats, capture and lock, pen, touch, gestures | [Pointer, Pen & Touch](pointer.md) |
| Drag sources, drop targets, drag data, OS file drops | [Drag & Drop](drag-and-drop.md) |
| Scroll containers, offsets, programmatic scrolling | [Scrolling](scrolling.md) |
| Window state, monitors, transient windows, tooltips, screenshots, system capabilities | [Window & System](window-system.md) |
| Audio and video playback, now-playing, media keys | [Media Playback](media.md) |
| Locale-aware numbers, dates, plurals, collation | [Locale Formatting](formatting.md) |
| Typing, IME, undo, document sync | [Text Input](text-input.md) |
| Cursors, selection ranges, clipboard | [Text Selection](text-selection.md) |
| Route pattern and parameters | [Routing](../architecture/routing.md) |

What stays on this page is the part that is not a group: the signature,
the return value, reading state, propagation, focus, async work, and
the styling and cache queries.

## Identifying the node that fired

```rust,ignore
let hit: DomNodeId = info.get_hit_node();
let rect = info.get_node_rect(hit);
```

A `DomNodeId` is `(DomId, NodeId)`, and for a callback attached to
several nodes the dataset is how you tell instances apart. Addressing,
walking and mutating nodes is the subject of
[Node Tree & Hit Testing](node-tree.md).

## Reading input state

The framework hands you the input state as of the event, plus the
snapshot from the previous one, so transitions are a diff rather than
bookkeeping you keep yourself:

```rust,ignore
info.get_current_keyboard_state();    // modifiers, pressed scancodes, reported chars
info.get_previous_keyboard_state();   // the same, one event ago
info.get_key_modifiers();             // just the modifier set
info.get_key_locks();                 // caps / num / scroll
info.is_key_repeat();                 // held, not newly pressed
```

`get_physical_key()` is the layout-independent key - the *position* on
the keyboard - which is what a shortcut like "W A S D" must use so it
stays a square on an AZERTY keyboard.

The pointer equivalents (`get_current_mouse_state`, the three cursor
position spaces, pen and touch) are on
[Pointer, Pen & Touch](pointer.md).

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
[Reconciliation](../dom/reconciliation.md) keeps the work proportional
to what changed.

For structural edits (insert a child, delete a node), use
`insert_child_node` and `delete_node`. Larger changes — anything
beyond a handful of nodes — are usually clearer expressed as a fresh
`Dom` from `layout()` plus `Update::RefreshDom`.

## Focus

```rust,ignore
info.set_focus(FocusTarget::Node(node_id));
info.set_focus_to_path(dom_id, css_path);     // by selector
info.has_focus(node_id);
info.clear_focus();
```

Focus moves on the next frame, and the reconciler migrates it across a
`RefreshDom` for nodes that still match - so a focused text field
survives a rebuild without you re-focusing it.

`focus_next()`, `focus_previous()`, `focus_first()` and `focus_last()`
walk the focus order, which is what a custom key handler calls instead
of computing the next focusable node itself.

Every one of these has a `_for_seat` twin - `set_focus_for_seat()`,
`clear_focus_for_seat()` - because a window can have several
independent cursors; see [Pointer, Pen & Touch](pointer.md).
`is_dom_focused(dom_id)` asks the same question of a whole sub-DOM.

Accessibility rides on the same axis: `set_accessibility_state()` and
`set_accessibility_value()` update what assistive technology reports
for a node, and `perform_accessibility_action()` performs an action
the assistive layer requested.

Scrolling a node into view is `scroll_node_into_view(node)`, and
`set_cursor_visibility(false)` hides the pointer - see
[Scrolling](scrolling.md) and [Pointer, Pen & Touch](pointer.md).

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
[Events and Input](..md).

`get_current_event_id()` identifies the event being dispatched, which is
how two callbacks on different nodes tell "the same click" from "two
clicks", and `get_last_input_sample()` is the raw sample it came from.

## Logging and metrics

A callback runs inside the framework's loop, so `println!` lands
wherever the host put stdout - which on a windowed app is often
nowhere. Log through the info instead, and it goes to the same sink as
the framework's own output:

```rust,ignore
info.log(AppLogLevel::Info, "export finished".into());
info.warn("texture atlas is full".into());
```

Three metric recorders sit alongside them - `record_counter()`,
`record_gauge()` and `record_histogram()`, each taking a name, a value
and a label set. They feed the observability pipeline, so an app can be
instrumented without a metrics crate of its own.

`get_ctx()` returns the host-provided context `RefAny`, when one was
installed.

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
[Background Tasks](../data/background-tasks.md).

## Window control

```rust,ignore
info.create_window(WindowCreateOptions::create(layout_fn));
info.close_window();
info.modify_window_state(new_state);
info.begin_interactive_move();       // hand the drag to the OS
```

`begin_interactive_move()` is the one worth singling out: it tells the
window manager to take over a drag, so a custom title bar moves the
window with the platform's own snapping and animations instead of a
per-frame reposition. Monitors, transient windows, tooltips and
screenshots are on [Window & System](window-system.md).

Routing across pages is `switch_route(pattern, params)`, with
`get_route_pattern()` and `get_route_param()` to read the active route;
see [Routing](../architecture/routing.md).

## Image and font caches

```rust,ignore
info.add_image_to_cache("logo".into(), image_ref);
info.remove_image_from_cache("logo".into());
info.reload_system_fonts();
```

Cached images are addressable by name from any layout pass. Reloading
system fonts is the right thing to do after a font config change
(rare, but desktop environments do change font defaults at runtime).

## Styling and layout queries

`set_css_property()` and `override_css_property()` change one
declaration on one node without re-running the cascade for the tree.
Reading back is the computed value, after the cascade:

```rust,ignore
info.get_computed_width(node);
info.get_computed_height(node);
info.get_computed_css_property(node, CssPropertyType::Display);
```

`has_pending_relayout_change()` reports whether anything queued so far
this callback will force a relayout - worth checking before adding
more, in a handler that runs every frame.

`get_animation_momentum()` and `set_animation_momentum()` read and
inject the velocity a node is carrying, which is how a custom gesture
hands a fling over to the built-in animation.

`get_gl_context()` returns the GL context for a node that draws its own
content, and `query_pagination()` measures how a styled DOM would break
across pages. Node geometry - sizes, positions, hit-test bounds - is on
[Node Tree & Hit Testing](node-tree.md).

## Working with sub-DOMs

`info.trigger_virtual_view_rerender(dom_id, node_id)` re-runs the
virtual-view callback for one specific sub-DOM. Use it when the
virtual view's source data changed but the parent layout hasn't.

`info.update_image_callback(dom_id, node_id)` triggers a re-render
of an `ImageCallback`-backed node — the GPU canvas pattern documented
in [SVG and Canvas](../images/svg.md).

## Finding another node: markers

A callback sometimes needs to act on a node it did *not* fire on — a
canvas callback moving a meter in the header, a timer updating a
preview tile. The address for that jump is a **marker**: a string you
mint at `layout()` time (use `Uuid::short()` — collision-free with no
coordination), stamp on the target with `Dom::with_marker`, and keep
wherever the driving callback can see it. Resolve it back to a node
with `get_node_id_by_marker`:

```rust,no_run
use azul::prelude::*;

struct App { meter_marker: String } // filled with Uuid::short() in layout()

extern "C" fn on_pen_move(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let marker = match data.downcast_ref::<App>() {
        Some(a) => a.meter_marker.clone(),
        None => return Update::DoNothing,
    };
    if let Some(node) = info.get_node_id_by_marker(marker).into_option() {
        // Drive the widget through ITS public API - its dataset stays private.
        ProgressBar::update_progress(info, node, 63.0);
    }
    Update::DoNothing // the widget repaints itself, no relayout needed
}
```

Markers are invisible to CSS matching and **excluded from node
equality**, so minting a fresh UUID every `layout()` never makes the
DOM diff consider a node "changed". The full pattern — when to prefer
it over `RefreshDom`, and how widgets like `ProgressBar` expose update
functions for it — is the architecture guide's
["inter-widget fast path"](../architecture.md#inter-widget-communication)
section.

## A complete example

A "delete row" button: each row's button callback carries a dataset
naming its row plus a backreference to the app (the
[backreference pattern](../architecture.md)), mutates the model, and
refreshes:

```rust,no_run
use azul::prelude::*;

struct App { rows: Vec<String> }

/// Attached as the RefAny of each row's delete-button callback.
struct DeleteRow { index: usize, app: RefAny }

extern "C" fn on_delete(mut data: RefAny, _info: CallbackInfo) -> Update {
    let (index, mut app_ref) = match data.downcast_ref::<DeleteRow>() {
        Some(d) => (d.index, d.app.clone()),
        None => return Update::DoNothing,
    };
    let mut app = match app_ref.downcast_mut::<App>() {
        Some(a) => a,
        None => return Update::DoNothing,
    };

    if index < app.rows.len() {
        app.rows.remove(index);
    }

    Update::RefreshDom
}
```

The pattern — a per-row dataset naming the row, a backreference to the
model, mutate, return `RefreshDom` — works for nearly every "this
widget acts on its container" interaction.
