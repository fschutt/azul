---
slug: scrolling-and-drag
title: Scrolling and Drag-and-Drop
language: en
canonical_slug: scrolling-and-drag
audience: external
maturity: wip
guide_order: 120
topic_only: false
prerequisites: [events]
tracked_files:
  - core/src/events.rs
  - core/src/callbacks.rs
last_generated_rev: 2acdeae71299faed9a65b0dddeea8d53c350e9ac
generated_at: 2026-05-01T20:34:08Z
---

# Scrolling and Drag-and-Drop

> **WIP** — Scrolling, scrollbar drag, and text-selection drag work end-to-end. The HTML5 drag-and-drop transfer API (`DragData`, `DragContext`) is wired through the event loop but the user-facing helper functions on `CallbackInfo` are still being polished.

A node becomes a scroll container as soon as its content overflows and one of `overflow`, `overflow-x`, or `overflow-y` is set to `auto` or `scroll`. The framework hit-tests scroll wheel and touch events against scroll containers, dispatches `Scroll`-family events, and updates the layout's scroll offset without re-running the layout callback.

```rust,no_run
# use azul::prelude::*;
let mut list = Dom::create_div();
list.set_inline_style("overflow-y: auto; height: 200px");
```

## Scroll events

Three `EventFilter` variants apply to scrollable containers:

| filter | fires when |
|---|---|
| `Hover(Scroll)` | A wheel/touch scroll happens over the node. |
| `Hover(ScrollStart)` | A scroll gesture begins. |
| `Hover(ScrollEnd)` | A scroll gesture ends (no further movement). |

`ScrollStart` / `ScrollEnd` are debounced by the scroll manager so they don't fire on every wheel tick — they bracket a continuous gesture. Use them to pause expensive rendering during a fling and resume when the user lets go.

```rust,no_run
# use azul::prelude::*;
# struct App { is_scrolling: bool }
# extern "C" fn scroll_start(_: RefAny, _: CallbackInfo) -> Update { Update::DoNothing }
# extern "C" fn scroll_end(_: RefAny, _: CallbackInfo) -> Update { Update::DoNothing }
# let data: RefAny = RefAny::new(App { is_scrolling: false });
let list = Dom::create_div()
    .with_callback(EventFilter::Hover(HoverEventFilter::ScrollStart), data.clone(), scroll_start)
    .with_callback(EventFilter::Hover(HoverEventFilter::ScrollEnd),   data,        scroll_end);
```

The plain `Scroll` filter fires repeatedly during the gesture; treat it as a high-frequency event.

## Programmatic scroll

Two methods on `CallbackInfo`:

```rust,ignore
info.scroll_to(dom_id, node_id, position);          // jump to position (logical px)
info.scroll_node_into_view(node_id, options);       // W3C scrollIntoView
```

`scroll_to` clamps the position to the container's bounds. `scroll_to_unclamped` skips clamping (used by the scroll-physics timer for rubber-band overscroll).

`scroll_node_into_view` takes `ScrollIntoViewOptions`:

```rust,ignore
ScrollIntoViewOptions::nearest()  // minimum scroll to make visible (default)
ScrollIntoViewOptions::center()   // center within container
ScrollIntoViewOptions::start()    // align to start
ScrollIntoViewOptions::end()      // align to end
```

Chain `.with_smooth()` or `.with_instant()` to override the default behaviour:

```rust,ignore
let opts = ScrollIntoViewOptions::center().with_smooth();
info.scroll_node_into_view(node_id, opts);
```

`Smooth` runs an internal animation timer (`SCROLL_MOMENTUM_TIMER_ID`) with the easing curve from `EasingFunction::EaseInOut` (`core/src/events.rs:26`). `Instant` jumps immediately. `Auto` defers to the CSS `scroll-behavior` property on the container.

## Scroll alignment

`ScrollLogicalPosition` controls per-axis alignment when used directly:

```rust,ignore
pub enum ScrollLogicalPosition {
    Start,    // align target's start edge with container's start edge
    Center,   // center within container
    End,      // align target's end edge with container's end edge
    Nearest,  // minimum scroll to make fully visible (default)
}
```

`ScrollIntoViewOptions` carries one for each axis (`block` for vertical, `inline_axis` for horizontal — named to avoid the `inline` C keyword). For mixed alignment:

```rust,ignore
let opts = ScrollIntoViewOptions {
    block: ScrollLogicalPosition::Start,
    inline_axis: ScrollLogicalPosition::Nearest,
    behavior: ScrollIntoViewBehavior::Smooth,
};
```

## Scroll deltas

Scroll events carry a `ScrollEventData` with two fields (`core/src/events.rs:390`):

```rust,ignore
pub struct ScrollEventData {
    pub delta: LogicalPosition,        // (dx, dy) in the unit specified by delta_mode
    pub delta_mode: ScrollDeltaMode,   // Pixel | Line | Page
}
```

Wheel devices typically emit `Line` (3 lines per notch on most platforms); precision touchpads emit `Pixel`; `PageDown` produces `Page`. Convert lines/pages to pixels using the container's font-metrics if you need to interpret the delta yourself — most callbacks don't, since the framework has already translated the wheel event into a scroll-offset update by the time the `Scroll` event fires.

## Drag-and-drop

Three drag families exist, each with its own event flow:

1. **Generic drag** — fired for any pointer-down + move. The set of `DragStart` / `Drag` / `DragEnd` events on a node fires while the mouse moves with a button held down.
2. **Drop-target drag** — `DragEnter` / `DragOver` / `DragLeave` / `Drop` on the *target* element. Use these to highlight the drop zone.
3. **Text selection drag** — handled internally by the cursor manager. You don't register handlers for it; the framework manages selection state.

The hover filters relevant to drag:

```rust,ignore
HoverEventFilter::DragStart    // pointer started a drag from this node
HoverEventFilter::Drag         // continues to fire while drag is in progress
HoverEventFilter::DragEnd      // pointer released (drag completed or cancelled)
HoverEventFilter::DragEnter    // dragged item entered this node (drop target)
HoverEventFilter::DragOver     // dragged item is over this node (fires continuously)
HoverEventFilter::DragLeave    // dragged item left this node
HoverEventFilter::Drop         // dragged item was dropped on this node
```

The `Window`-scoped variants of these (e.g. `WindowEventFilter::DragOver`) fire for any drag in the window, regardless of which node the cursor is over — useful for global drop overlays.

## Drag payload

`DragData` (`core/src/drag.rs:294`) carries the dragged content as a vec of `(mime_type, bytes)` pairs, plus an `effect_allowed` flag. It maps to the HTML5 `DataTransfer` API:

```rust,ignore
let mut data = DragData::new();
data.set_text("Hello, world");                   // text/plain
data.set_data("application/json", json_bytes);   // arbitrary MIME
data.effect_allowed = DragEffect::CopyMove;      // source allows copy or move
```

`DragEffect` is the "what's permitted" set:

| variant | meaning |
|---|---|
| `Uninitialized` | Default; treated as `All`. |
| `None` | No drop permitted. |
| `Copy` / `Move` / `Link` | Single operation only. |
| `CopyLink`, `CopyMove`, `LinkMove` | Two-of-three. |
| `All` | Any of copy/move/link. |

Reading `DragData` from inside a `Drop` callback is the planned API surface — the helper methods on `CallbackInfo` are still being polished. Until those land, query `info.get_current_window_state()` and walk the live drag state in `FullWindowState` directly.

## File drops from the OS

Three `Hover` filters cover OS-initiated file drags (drag a file from Finder/Explorer onto your window):

```rust,ignore
HoverEventFilter::HoveredFile           // file is hovering over this node
HoverEventFilter::DroppedFile           // file was dropped on this node
HoverEventFilter::HoveredFileCancelled  // user cancelled (dragged out, hit Escape)
```

These fire once per file, not per move, so you don't need to debounce. The file path arrives via the `EventData::Mouse` payload's accompanying drag-state — check the file-drop section in `windowing.md` for the full plumbing.

## Auto-scroll during drag

When a drag's pointer reaches within 20px of a scroll container's edge, the framework starts a `DRAG_AUTOSCROLL_TIMER_ID` internal timer that continues scrolling at a constant rate until the pointer moves back into the safe zone or the drag ends. This is automatic; you don't register or remove the timer. See [Timers](timers.md) for the reserved timer ID list.

## Scroll momentum (smooth scroll)

When `WindowFlags::smooth_scroll_enabled` is `true` (default), wheel and touch deltas decay with inertia after the gesture ends. The internal timer `SCROLL_MOMENTUM_TIMER_ID` runs the decay and emits scroll updates until the velocity drops below a threshold. Toggle smoothing per-window:

```rust,no_run
# use azul::prelude::*;
# extern "C" fn handler(_: RefAny, mut info: CallbackInfo) -> Update {
let mut state = info.get_current_window_state().clone();
state.flags.smooth_scroll_enabled = false;
info.modify_window_state(state);
# Update::DoNothing
# }
```

Disabling smoothing makes wheel events translate immediately to scroll-offset updates, with no decay.

## Common patterns

**Pause expensive work during scroll**:

```rust,no_run
# use azul::prelude::*;
# struct App { paused: bool }
extern "C" fn on_scroll_start(mut data: RefAny, _: CallbackInfo) -> Update {
    if let Some(mut a) = data.downcast_mut::<App>() {
        a.paused = true;
    }
    Update::DoNothing
}
extern "C" fn on_scroll_end(mut data: RefAny, _: CallbackInfo) -> Update {
    if let Some(mut a) = data.downcast_mut::<App>() {
        a.paused = false;
    }
    Update::RefreshDom
}
```

**Drop zone with visual feedback**:

```rust,no_run
# use azul::prelude::*;
# struct State { hover: bool }
# extern "C" fn enter(_: RefAny, _: CallbackInfo) -> Update { Update::RefreshDom }
# extern "C" fn leave(_: RefAny, _: CallbackInfo) -> Update { Update::RefreshDom }
# extern "C" fn drop_(_: RefAny, _: CallbackInfo) -> Update { Update::RefreshDom }
# let data: RefAny = RefAny::new(State { hover: false });
let zone = Dom::create_div()
    .with_callback(EventFilter::Hover(HoverEventFilter::DragEnter), data.clone(), enter)
    .with_callback(EventFilter::Hover(HoverEventFilter::DragLeave), data.clone(), leave)
    .with_callback(EventFilter::Hover(HoverEventFilter::Drop),      data,        drop_);
```

The `enter` / `leave` handlers toggle a `hover` flag; the layout callback reads the flag and adds/removes a CSS class. `drop_` reads the payload and clears the flag.

## What is not covered

- The internal `ScrollManager` (`layout/src/managers/scroll_state.rs`) — see the contributor docs.
- Custom scrollbar styling — covered in the styling guide.
- Touch-driven multi-finger gestures (pinch, swipe) on scroll surfaces — partly wired but not exposed yet.
- Programmatic drag initiation from a callback — pending.

## Next

- [Windows, Menus, Decorations](windowing.md) — multi-window apps.
- [Timers](timers.md) — the scroll-momentum / drag-autoscroll timers under the hood.
- [Events and Input](events.md) — the underlying event-filter mechanism.
