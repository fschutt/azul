---
slug: events/pointer
title: Pointer, Pen & Touch
language: en
canonical_slug: events/pointer
audience: external
maturity: wip
guide_order: 66
topic_only: false
short_desc: Cursor state, pointer seats, capture and lock, pen pressure and tilt, touch coalescing, and gestures
prerequisites: [hello-world, events, events/callbacks]
tracked_files:
  - layout/src/callbacks.rs
  - layout/src/managers/gesture.rs
  - core/src/events.rs
  - core/src/window.rs
default-search-keys:
  - CallbackInfo
  - MouseState
  - PointerSource
  - get_cursor_position
  - capture_pointer
  - set_pointer_lock
  - get_pen_pressure
  - get_coalesced_touches
---

# Pointer, Pen & Touch

A mouse, a finger and a pen all arrive as the same events. This page is
about the state behind them: where the pointer is, which device it is,
how to keep events flowing to one node, and what the extra axes of a pen
or a touch screen give you.

## Introduction

Pointer events are dispatched the usual way - see
[Callbacks](callbacks.md) for how a handler is attached and
[Node Tree & Hit Testing](node-tree.md) for which node receives them.
What you get *inside* the handler is a snapshot of the pointer, and the
methods here read it. Nothing on this page rebuilds the DOM.

## Where the pointer is

Three coordinate spaces, three methods, and picking the wrong one is the
most common source of "my drag is offset by the title bar":

```rust,ignore
let in_window   = info.get_cursor_position();            // window-relative, logical px
let in_viewport = info.get_cursor_relative_to_viewport(); // scrolled content
let on_screen   = info.get_cursor_position_screen();      // desktop coordinates
```

All three are options, because there may be no pointer at all - a
keyboard-only session, or a touch device with nothing touching it.

For a position relative to a specific node - the usual thing a canvas or
a slider wants - use `get_cursor_relative_to_node(node)` rather than
subtracting rectangles yourself.

## Buttons and history

`get_current_mouse_state()` is the whole button and position state as of
this event. `get_previous_mouse_state()` is the same thing one event
ago, which is what you diff to find out *what changed*: a callback fires
on both press and release, and the pair tells you which.

```rust,ignore
let now = info.get_current_mouse_state();
let before = info.get_previous_mouse_state();
```

`was_double_clicked()` answers the question directly instead of making
you time clicks yourself.

## Seats: a window can have several cursors

A *seat* is one independent cursor with its own buttons and focus. Most
systems have exactly one, so most code can ignore this - but multi-seat
Wayland, multi-touch and pen-plus-finger input all produce more than one
at a time, and a handler that assumes one cursor will mix them up.

```rust,ignore
let seat = info.get_pointer_device_id();
let state = info.get_pointer_seat_state(seat);
```

A seat is not a device. Two devices driving the same cursor are one
seat; one touch screen reporting three fingers is three. Anything that
tracks a gesture across events must key its state by seat id, or a
second finger will overwrite the first one's start position.

The focus methods have `_for_seat` variants for the same reason - see
[Node Tree & Hit Testing](node-tree.md).

## Capture and lock

**Capture** routes every subsequent pointer event to one node, even once
the pointer leaves it. This is what makes a drag survive a fast mouse
movement out of the handle:

```rust,ignore
info.capture_pointer(handle);      // on press
// ... events keep arriving at `handle` ...
info.release_pointer_capture();    // on release
```

Always release. A capture that is never released makes the rest of the
window unclickable, and nothing times it out for you.

**Lock** is different: `set_pointer_lock(true)` hides the cursor and
stops reporting absolute positions, delivering relative motion instead.
That motion comes from `get_raw_mouse_motion()`, which is unaccelerated
and unclamped - the input a first-person camera or an infinite-drag
number field wants. Absolute position methods stop being useful while
locked, by design.

## Which device produced this event

```rust,ignore
match info.get_pointer_source() {
    PointerSource::Mouse => { /* ... */ }
    PointerSource::Touch => { /* ... */ }
    PointerSource::Pen   => { /* ... */ }
    _ => {}
}
```

Worth branching on more often than people do: a 44-px hit target and a
long-press menu are right for touch and wrong for a mouse, and the same
callback serves both.

## Pen

A pen is a pointer with extra axes, all of them optional because not
every tablet reports every one:

```rust,ignore
let pressure = info.get_pen_pressure();        // 0.0 ..= 1.0
let tilt     = info.get_pen_tilt();            // two angles
let distance = info.get_pen_hover_distance();  // above the surface, not touching
```

`is_pen_in_contact()` separates hovering from drawing, `is_pen_eraser()`
reports the inverted end of the stylus, and
`is_pen_barrel_button_pressed()` the side button.
`get_pen_tool_kind()` is the coarser question - pen, eraser, airbrush,
lens - and `get_pen_state()` returns the whole set at once if you would
rather read one struct than six options.

Treat every one of these as absent-by-default. A drawing app that
requires pressure will draw nothing on hardware that does not report it;
fall back to a constant.

## Touch: coalescing and prediction

A touch screen samples far faster than the display refreshes, so a
single frame's event carries more than one sample:

```rust,ignore
for point in info.get_coalesced_touches().iter() { /* every real sample */ }
for point in info.get_predicted_touches().iter() { /* extrapolated, not yet real */ }
```

Draw the coalesced points to get a smooth stroke instead of a polyline
between frames. Draw the predicted ones to hide input latency - but
render them as provisional and replace them when the real samples
arrive, because the prediction is a guess and will sometimes overshoot a
sharp corner.

## Gestures

The recognised gestures are read, not subscribed to. Each returns an
option that is `Some` only on the frame the gesture is active:

```rust,ignore
if let Some(pinch) = info.get_pinch().into_option() { /* scale */ }
if let Some(rot) = info.get_rotation().into_option() { /* angle */ }
if let Some(dir) = info.get_swipe_direction().into_option() { /* ... */ }
if let Some(press) = info.get_long_press().into_option() { /* ... */ }
```

`has_sufficient_history_for_gestures()` guards the case where a gesture
begins before enough samples exist to measure it - checking it stops a
pinch from reporting a wild first scale factor.
`settle_scroll_gesture()` ends an in-flight scroll gesture early, which
is what you call when a pinch takes over from a pan.

## More methods

Everything else in this group, by what it answers.

**Position and state** - `get_cursor_position`,
`get_cursor_position_screen`, `get_cursor_relative_to_viewport`,
`get_current_mouse_state`, `get_previous_mouse_state`,
`get_pointer_seat_state`, `get_pointer_device_id`, `get_pointer_source`,
`was_double_clicked`, `get_raw_mouse_motion`.

**Capture and lock** - `capture_pointer`, `release_pointer_capture`,
`set_pointer_lock`.

**Pen and tablet** - `get_pen_pressure`, `get_pen_tilt`,
`get_pen_hover_distance`, `get_pen_state`, `get_pen_tool_kind`,
`is_pen_in_contact`, `is_pen_eraser`, `is_pen_barrel_button_pressed`,
`get_proximity`, `get_tablet_devices`, `get_tablet_pad`,
`get_dial_state`. `get_tablet_devices()` enumerates connected tablets;
`get_tablet_pad()` and `get_dial_state()` read the buttons, rings and
dials on the tablet body rather than the stylus.

**Touch** - `get_coalesced_touches`, `get_predicted_touches`.

**Gestures** - `get_pinch`, `get_rotation`, `get_swipe_direction`,
`get_long_press`, `has_sufficient_history_for_gestures`,
`settle_scroll_gesture`, `inject_native_gesture`.
`inject_native_gesture()` feeds a synthetic gesture into the same
pipeline, which is how automated tests drive one.

**Menus and feedback** - `open_menu`, `open_menu_at`,
`invoke_system_dialog`, `play_haptic`, `play_haptic_request`.
`open_menu()` places a context menu at the pointer and `open_menu_at()`
at a position you choose; `play_haptic()` takes a pattern and a target,
`play_haptic_request()` a prepared request.

See also [Scrolling](scrolling.md) for wheel and momentum, and
[Node Tree & Hit Testing](node-tree.md) for resolving which node a
pointer is over.
