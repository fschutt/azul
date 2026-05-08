---
slug: events
title: Events
language: en
canonical_slug: events
audience: external
maturity: mature
guide_order: 90
topic_only: false
short_desc: Callbacks, event filters, and how state triggers relayout
prerequisites: [hello-world]
tracked_files:
  - core/src/events.rs
  - core/src/callbacks.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:51:14Z
---

# Events

## Overview

A callback is a function pointer plus a `RefAny`, registered against an event filter on a DOM node. When an event matches a registered filter, the framework borrows your `RefAny`, calls the callback, and reads the returned `Update` to decide whether to re-run the layout.

```rust,no_run
use azul::prelude::*;

struct Counter { n: usize }

extern "C" fn on_click(_: RefAny, _: CallbackInfo) -> Update { Update::DoNothing }

fn build(mut data: RefAny) -> Dom {
    Dom::create_div()
        .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp), data, on_click)
}
```

## Adding a callback

The primitive is `Dom::add_callback` (and the builder form `Dom::with_callback`). Both take three arguments: the filter, a `RefAny` (your data), and a function pointer with the signature `extern "C" fn(RefAny, CallbackInfo) -> Update`.

```rust,no_run
use azul::prelude::*;

struct State;

extern "C" fn handler(_: RefAny, _: CallbackInfo) -> Update { 
    Update::DoNothing 
}

fn main() {
    let data: RefAny = RefAny::new(State);
    let node = Dom::create_div()
        .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp), data.clone(), handler)
        .with_callback(EventFilter::Focus(FocusEventFilter::FocusReceived), data, handler);   
}
```

The `Update` enum has three variants:

- `Update::DoNothing`: no re-layout. The current frame stays on screen.
- `Update::RefreshDom`: re-run the layout callback for this window.
- `Update::RefreshDomAllWindows`: re-run the layout callback for every window.

Returning `RefreshDom` from a non-mutating handler is wasteful. Returning `DoNothing` from a handler that mutated the model leaves the screen out of sync. Match the return value to what you actually changed.

## Event filters

`EventFilter` is the enum you pass to `add_callback`. It has five variants, each scoped to a different fire condition:

- `EventFilter::Hover(HoverEventFilter)`: fires when the cursor is over this node.
- `EventFilter::Focus(FocusEventFilter)`: fires when this node has keyboard focus. The node needs a tab index.
- `EventFilter::Window(WindowEventFilter)`: fires for events anywhere in the focused window.
- `EventFilter::Component(ComponentEventFilter)`: fires when the node is mounted, unmounted, resized, or updated.
- `EventFilter::Application(ApplicationEventFilter)`: fires when a device or monitor is plugged in or removed.

The same physical event can match multiple filters. A left mouse-button release while the cursor is inside a button matches `Hover(MouseUp)`, `Hover(LeftMouseUp)`, and, if the button has focus, `Focus(MouseUp)` and `Focus(LeftMouseUp)`. All registered handlers fire.

### HoverEventFilter

The element the mouse is currently over. Common variants:

```rust,ignore
HoverEventFilter::MouseOver        // mouse moves while over the node
HoverEventFilter::MouseDown        // any button pressed
HoverEventFilter::LeftMouseDown    // left only (also Right/Middle)
HoverEventFilter::MouseUp          // any button released
HoverEventFilter::LeftMouseUp      // left only; use this for "click"
HoverEventFilter::DoubleClick      // double-click detected
HoverEventFilter::MouseEnter       // cursor crossed into the node
HoverEventFilter::MouseLeave       // cursor crossed out of the node
HoverEventFilter::Scroll           // wheel / trackpad over the node
HoverEventFilter::DroppedFile      // a file was dropped on the node
```

For "click", use `LeftMouseUp` rather than `MouseDown`. It matches the W3C activation pattern: press, move out, release does not click.

### FocusEventFilter

Same vocabulary as `HoverEventFilter`, but the node must currently hold keyboard focus. Set a tab index (or focus programmatically) for the filter to fire:

```rust,no_run
use azul::prelude::*;

let input = Dom::create_div()
    .with_tab_index(TabIndex::Auto);
```

`TabIndex::Auto` makes the node focusable in source order. `TabIndex::NoKeyboardFocus` makes it focusable programmatically but skips it in tab navigation. `TabIndex::OverrideInParent(n)` pins the node at slot `n` within its parent's tab order.

Common focus filters:

```rust,ignore
FocusEventFilter::FocusReceived    // node became the focused element
FocusEventFilter::FocusLost        // focus moved elsewhere
FocusEventFilter::TextInput        // a unicode character arrived
FocusEventFilter::VirtualKeyDown   // a non-text key (arrow, F1, ...)
FocusEventFilter::VirtualKeyUp
```

`FocusEventFilter::TextInput` carries the produced character, respecting the OS keyboard layout (German `ä`, IME composition, etc.). `VirtualKeyDown` carries a layout-independent key code. Use it for shortcuts and games.

### WindowEventFilter

Fires on any node registered with this filter regardless of where the cursor or focus is, as long as the window has OS focus:

```rust,ignore
WindowEventFilter::Resized           // window dimensions changed
WindowEventFilter::Moved             // window position changed
WindowEventFilter::CloseRequested    // user clicked the close button
WindowEventFilter::ThemeChanged      // OS switched light/dark mode
WindowEventFilter::DpiChanged        // window moved to monitor with different DPI
WindowEventFilter::VirtualKeyDown    // any keypress in the window
```

Use `Window` for global shortcuts (Ctrl+S, Esc) where the source node doesn't matter.

### ComponentEventFilter

Lifecycle events fire after a new DOM is reconciled against the previous frame:

```rust,ignore
ComponentEventFilter::AfterMount     // node appeared this frame
ComponentEventFilter::BeforeUnmount  // node will disappear next frame
ComponentEventFilter::NodeResized    // layout bounds of this node changed
ComponentEventFilter::Updated        // a keyed node's content changed
```

Reconciliation matches nodes across frames by stable key (`Dom::with_id("id")`) first, then by content hash. Keyed nodes track identity across reorders, so `Updated` only fires when keyed content actually changes.

### ApplicationEventFilter

Fires for global hardware changes. Useful only on the root DOM node:

```rust,ignore
ApplicationEventFilter::DeviceConnected
ApplicationEventFilter::DeviceDisconnected
ApplicationEventFilter::MonitorConnected
ApplicationEventFilter::MonitorDisconnected
```

## Event propagation

For each event the framework computes the path from root to target and calls handlers in three phases (W3C DOM Level 2):

1. **Capture**: root to target. Rare; only nodes with capture-phase handlers are visited.
2. **Target**: handlers on the target node itself.
3. **Bubble**: target to root.

A click on a deeply nested span walks back up through its ancestors, firing `Hover(MouseUp)` handlers on every node along the way that registered one. To stop the walk, call one of the propagation methods on `CallbackInfo`:

```rust,no_run
use azul::prelude::*;

extern "C" 
fn handler(_: RefAny, mut info: CallbackInfo) -> Update {
    // remaining handlers on the same node still run
    info.stop_propagation();
    // nothing else runs
    info.stop_immediate_propagation();
    Update::DoNothing
}
```

`stop_propagation` matches W3C `event.stopPropagation()`. `stop_immediate_propagation` matches `event.stopImmediatePropagation()`.

## Default actions

Some events have built-in behaviour that runs after every callback returns, unless a callback prevented it:

- `Tab`: move focus to next focusable element.
- `Shift+Tab`: move focus to previous focusable element.
- `Enter` / `Space` on focused button: synthetic click on the button.
- `Escape`: clear focus or close modal.
- `Ctrl+A` in text input: select all.
- Arrow keys in scroll container: scroll by line.

To suppress the default action from a callback:

```rust,no_run
use azul::prelude::*;

extern "C" fn on_keydown(_: RefAny, mut info: CallbackInfo) -> Update {
    info.prevent_default();
    Update::DoNothing
}
```

`prevent_default` corresponds to W3C `event.preventDefault()`. The W3C semantics: the default action doesn't fire, but other callbacks for the same event still run. Combine with `stop_propagation` to also halt the propagation walk.

## Reading input state

`CallbackInfo` exposes the current input snapshot:

```rust,ignore
let kbd: KeyboardState = info.get_current_keyboard_state();
let mouse: MouseState  = info.get_current_mouse_state();
let win:  WindowFlags  = info.get_current_window_flags();
let state: &FullWindowState = info.get_current_window_state();
```

`KeyboardState` carries `pressed_virtual_keycodes` (a vec of currently held keys) and `current_virtual_keycode` (the most recent). `MouseState` carries `cursor_position`, `left_down`, `right_down`, `middle_down`.

Read state inside the callback to check modifier keys for shortcuts:

```rust,no_run
use azul::prelude::*;

struct App;

extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let kbd = info.get_current_keyboard_state();
    let pressed = kbd.pressed_virtual_keycodes.as_slice();
    let ctrl = pressed.iter().any(|k| matches!(k, VirtualKeyCode::LControl | VirtualKeyCode::RControl));
    let s = pressed.iter().any(|k| *k == VirtualKeyCode::S);
    if ctrl && s {
        info.prevent_default();
        // ... save ...
        return Update::RefreshDom;
    }
    Update::DoNothing
}
```

## Common patterns

**Click:**

```rust,no_run
use azul::prelude::*;

struct S;

extern "C" fn click(_: RefAny, _: CallbackInfo) -> Update { 
    Update::DoNothing 
}

fn main() {
    let data: RefAny = RefAny::new(S);
    Dom::create_div().with_callback(
        EventFilter::Hover(HoverEventFilter::LeftMouseUp),
        data,
        click,
    );
}
```

**Hover effect** (use CSS `:hover` for visual change; use a callback only when you need to mutate state):

```rust,no_run
use azul::prelude::*;

struct S;

extern "C" 
fn enter(_: RefAny, _: CallbackInfo) -> Update { 
    Update::DoNothing 
}

extern "C" 
fn leave(_: RefAny, _: CallbackInfo) -> Update { 
    Update::DoNothing 
}

fn main() {
    let data: RefAny = RefAny::new(S);
    Dom::create_div()
        .with_callback(EventFilter::Hover(HoverEventFilter::MouseEnter), data.clone(), enter)
        .with_callback(EventFilter::Hover(HoverEventFilter::MouseLeave), data,        leave);   
}
```

**Window-level keyboard shortcut:**

```rust,no_run
use azul::prelude::*;

struct S;

extern "C" fn on_key(_: RefAny, _: CallbackInfo) -> Update { 
    Update::DoNothing 
}

fn main() {
    let data: RefAny = RefAny::new(S);
    Dom::create_body().with_callback(
        EventFilter::Window(WindowEventFilter::VirtualKeyDown),
        data,
        on_key,
    );
}
```

**Tab order:** call `.with_tab_index(TabIndex::Auto)` to make a node focusable. Tab and Shift+Tab move through nodes in DOM order. Use `TabIndex::NoKeyboardFocus` to make a node focusable programmatically but skip it in tab navigation.

## Potential Problems

- **Callback never fires.** Check the filter scope. `Hover(LeftMouseUp)` only fires when the cursor is over the node at the moment of release. If the user pressed inside, dragged out, and released outside, no click event fires on either node.
- **`Focus(...)` never fires.** The node has no tab index. Add `.with_tab_index(TabIndex::Auto)` so the node can receive focus.
- **Counter doesn't update.** The callback returned `Update::DoNothing`. Return `Update::RefreshDom` after mutating the model.
- **Default action still happens after `prevent_default`.** Verify the call is on `CallbackInfo`, not on a stale copy. The change is applied after the callback returns. Calling it twice is harmless.

## Coming Up Next

- [Text Input](text-input.md) — Editable text, IME, and the selection model
- [Scrolling](scrolling-and-drag.md) — Scroll containers, drag-and-drop, hit testing
- [Timers](timers.md) — Timers, threads, and scheduled work
- [Windows, Menus, Decorations](windowing.md) — Windows, menus, decorations, and per-window state
