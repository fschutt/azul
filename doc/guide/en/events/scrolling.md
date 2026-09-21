---
slug: events/scrolling
title: Scrolling
language: en
canonical_slug: events/scrolling
audience: external
maturity: wip
guide_order: 64
topic_only: false
short_desc: Scroll containers, scroll events, programmatic scrolling and momentum
prerequisites: [hello-world, events]
tracked_files:
  - core/src/events.rs
  - layout/src/hit_test.rs
  - layout/src/callbacks.rs
  - layout/src/managers/scroll_state.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T06:00:00Z
default-search-keys:
  - CallbackInfo
  - EventFilter
  - HoverEventFilter
  - WindowEventFilter
  - scroll_to
---

# Scrolling

## Introduction

*WIP.* Scrolling is functional: scroll containers, wheel and trackpad
input, programmatic scrolling and momentum all work. Drag and drop moved
to its own page, [Drag & Drop](drag-and-drop.md), because the two only
share the input pipeline.

Scrolling is a gesture: the framework tracks the pointer across several
events, decides which gesture is in progress, and emits high-level
events you handle the same way as any other event filter from
[events](..md).

## Making a node scrollable

CSS does the work. Set `overflow` to `scroll`, `auto`, or `hidden` on a node whose content exceeds its box, and the framework gives it scrollbars and wires the wheel/trackpad/touchpad events to it.

```rust,no_run
use azul::prelude::*;
let scroll_box = Dom::create_div()
    .with_css("width: 400px; height: 200px; overflow-y: scroll;");
```

- `visible` (default). Children draw outside the box. No clip, no scrollbar, no `Scroll` events.
- `hidden`. Children clip to the box. No scrollbar. No scrolling.
- `scroll`. Children clip. Scrollbar always visible, even if not needed. Wheel scrolls.
- `auto`. Children clip. Scrollbar visible only when content overflows. Wheel scrolls.

`overflow-x` and `overflow-y` set the axes independently. The shorthand `overflow: scroll` sets both.

The container's scroll-clip size is its inner box (border-box minus borders, padding, and scrollbar track). The content size is the total extent of its children. Scrolling shifts which slice of the content sits inside the clip.

### What you don't get

- **No viewport-level scrolling.** The window itself doesn't scroll. Make `<body>` or a descendant the scroll container.
- **No automatic scroll-into-view on focus.** Call `info.scroll_to(...)` from a focus callback if you want this.

## Reading scroll events

`HoverEventFilter::Scroll` fires for wheel/trackpad/touch scroll over a scrollable node. `ScrollStart` and `ScrollEnd` bracket a contiguous gesture so you can debounce work.

```rust,no_run
use azul::prelude::*;

extern "C" fn on_scroll(_: RefAny, _: CallbackInfo) -> Update {
    Update::DoNothing
}

fn build(data: RefAny) -> Dom {
    let mut node = Dom::create_div();
    node.add_callback(
        EventFilter::Hover(HoverEventFilter::Scroll),
        data,
        on_scroll,
    );
    node
}
```

Inside the callback, query the current scroll state through `CallbackInfo`:

```rust,ignore
impl CallbackInfo {
    pub fn get_scroll_offset(&self) -> Option<LogicalPosition>;
    pub fn get_scroll_offset_for_node(&self, dom_id: DomId, node_id: NodeId)
        -> Option<LogicalPosition>;
    pub fn get_scroll_state(&self, /* ... */) -> Option<ScrollState>;
    pub fn get_scroll_delta(&self) -> LogicalPosition;
    pub fn had_scroll_activity(&self) -> bool;
}
```

`get_scroll_offset()` returns the offset for the hit node, which is convenient inside a `Scroll` callback. `ScrollState::scroll_position` is the live position.

## Scrolling programmatically

`CallbackInfo::scroll_to` queues a scroll that the runtime applies after the callback returns:

```rust,ignore
impl CallbackInfo {
    pub fn scroll_to(&mut self, /* ... */);
}
```

The position is in the scroll container's content space, so `(0, 0)` scrolls to the top-left of the content.

## Hit-test order

Front-to-back. The topmost element under the cursor is hit first; deeper nodes are tried only if the topmost doesn't claim the event. A scroll routes to the topmost scrollable ancestor of the hit node, not to whichever scroll container the cursor happens to sit inside. This matches browser behaviour: a button's `cursor: pointer` overrides the container's `cursor: text`, and a `<select>` swallows wheel events that would otherwise scroll the page.

## Smooth and momentum scrolling

The framework animates between scroll positions when `scroll_to` is called. Trackpad and wheel deltas are accumulated frame-to-frame; momentum drives the inertia phase after a fling. You don't manage the timer.

CSS `-azul-overflow-scrolling: touch` enables momentum on a node. `overscroll-behavior: contain` prevents scroll chaining to the parent.

## Cross-references

- [`events`](..md): the event filter system this page builds on.
- [`timers`](../animations/timers.md): scrolling momentum and drag auto-scroll run on reserved timers.
- [Drag & Drop](drag-and-drop.md): the drag half of this page, moved out.
