---
slug: events
title: Events and Input
language: en
canonical_slug: events
audience: external
maturity: mature
guide_order: 90
topic_only: false
short_desc: Callback registration, event filters, the Update enum, and how the framework re-runs layout when state changes.
prerequisites: [hello-world]
tracked_files:
  - core/src/events.rs
  - core/src/callbacks.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:51:14Z
---

# Events and Input

A callback is a function pointer plus a `RefAny`, registered against an event filter on a DOM node. When the framework detects that an event matches a registered filter, it borrows your `RefAny`, calls the callback, and reads the returned `Update` to decide whether to re-run the layout.

```rust,no_run
# use azul::prelude::*;
# struct Counter { n: usize }
# extern "C" fn on_click(_: RefAny, _: CallbackInfo) -> Update { Update::DoNothing }
# fn build(mut data: RefAny) -> Dom {
let mut button = Dom::create_div();
button.add_callback(EventFilter::Hover(HoverEventFilter::MouseUp), data, on_click);
# button
# }
```

## Adding a callback

The primitive is `Dom::add_callback` (and the builder form `Dom::with_callback`). Both take three arguments: the filter, a `RefAny` (your data), and a function pointer with the signature `extern "C" fn(RefAny, CallbackInfo) -> Update`.

```rust,no_run
# use azul::prelude::*;
# struct State;
# extern "C" fn handler(_: RefAny, _: CallbackInfo) -> Update { Update::DoNothing }
# let data: RefAny = RefAny::new(State);
let mut node = Dom::create_div()
    .with_callback(EventFilter::Hover(HoverEventFilter::MouseUp), data.clone(), handler)
    .with_callback(EventFilter::Focus(FocusEventFilter::FocusReceived), data, handler);
```

The callback signature is in `core/src/callbacks.rs`. The `Update` enum has three variants:

| variant | effect |
|---|---|
| `Update::DoNothing` | No re-layout. The frame already on screen stays. |
| `Update::RefreshDom` | Re-run the layout callback for this window. |
| `Update::RefreshDomAllWindows` | Re-run the layout callback for every window. |

Returning `RefreshDom` from a non-mutating handler is wasteful; returning `DoNothing` from a handler that mutated the model leaves the screen out of sync. Match the return value to what you actually changed.

## Event filters

`EventFilter` is the enum you pass to `add_callback`. It has five variants, each scoped to a different fire condition:

| filter | fires when |
|---|---|
| `Hover(HoverEventFilter)` | Mouse cursor is over this node when the event happens. |
| `Focus(FocusEventFilter)` | This node has keyboard focus when the event happens. Requires a tab index. |
| `Window(WindowEventFilter)` | The event happens anywhere in the focused window. |
| `Component(ComponentEventFilter)` | The node was mounted, unmounted, resized, or updated. |
| `Application(ApplicationEventFilter)` | A device or monitor was plugged in / removed. |

The same physical event can match multiple filters. A left mouse-button release while the cursor is inside `<button>` matches `Hover(MouseUp)`, `Hover(LeftMouseUp)`, and — if the button has focus — `Focus(MouseUp)` and `Focus(LeftMouseUp)`. All registered handlers fire.

### `HoverEventFilter`

The element the mouse is currently over. Common variants:

```rust,ignore
HoverEventFilter::MouseOver        // mouse moves while over the node
HoverEventFilter::MouseDown        // any button pressed
HoverEventFilter::LeftMouseDown    // left only (also Right/Middle)
HoverEventFilter::MouseUp          // any button released
HoverEventFilter::LeftMouseUp      // left only — use this for "click"
HoverEventFilter::DoubleClick      // double-click detected
HoverEventFilter::MouseEnter       // cursor crossed into the node
HoverEventFilter::MouseLeave       // cursor crossed out of the node
HoverEventFilter::Scroll           // wheel / trackpad over the node
HoverEventFilter::DroppedFile      // a file was dropped on the node
```

For "click", use `LeftMouseUp` rather than `MouseDown`: it matches the W3C activation pattern (press, move out, release does not click).

### `FocusEventFilter`

Same vocabulary as `HoverEventFilter`, but the node must currently hold keyboard focus. Set a tab index (or focus programmatically) for the filter to ever fire:

```rust,no_run
# use azul::prelude::*;
let input = Dom::create_div().with_tab_index(TabIndex::Auto);
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

`FocusEventFilter::TextInput` carries the produced character, respecting the OS keyboard layout (German `ä`, IME composition, etc.). `VirtualKeyDown` carries a layout-independent key code — use it for shortcuts and games.

### `WindowEventFilter`

Fires on any node registered with this filter regardless of where the cursor or focus is, as long as the window has OS focus:

```rust,ignore
WindowEventFilter::Resized           // window dimensions changed
WindowEventFilter::Moved             // window position changed
WindowEventFilter::CloseRequested    // user clicked the close button
WindowEventFilter::ThemeChanged      // OS switched light/dark mode
WindowEventFilter::DpiChanged        // window moved to monitor with different DPI
WindowEventFilter::VirtualKeyDown    // any keypress in the window
```

Use `Window` for global shortcuts (Ctrl+S, Esc) where the source node does not matter.

### `ComponentEventFilter`

Lifecycle events fire after the framework reconciles a new DOM against the previous frame:

```rust,ignore
ComponentEventFilter::AfterMount     // node appeared this frame
ComponentEventFilter::BeforeUnmount  // node will disappear next frame
ComponentEventFilter::NodeResized    // layout bounds of this node changed
ComponentEventFilter::Updated        // a keyed node's content changed
```

Reconciliation matches nodes across frames by stable key (`Dom::with_key("id")`) first, then by content hash. Keyed nodes track identity across reorders, so `Updated` only fires when the keyed content actually changes.

### `ApplicationEventFilter`

Fires for global hardware changes — useful only on the root DOM node:

```rust,ignore
ApplicationEventFilter::DeviceConnected
ApplicationEventFilter::DeviceDisconnected
ApplicationEventFilter::MonitorConnected
ApplicationEventFilter::MonitorDisconnected
```

## Event propagation

For each event the framework computes the path from root to target and calls handlers in three phases (W3C DOM Level 2):

1. **Capture** — root → target (rarely used; only nodes with capture-phase handlers are visited).
2. **Target** — handlers on the target node itself.
3. **Bubble** — target → root.

A click on a deeply nested `<span>` walks back up through its ancestors, firing `Hover(MouseUp)` handlers on every node along the way that registered one. To stop the walk, call one of the propagation methods on `CallbackInfo`:

```rust,no_run
# use azul::prelude::*;
extern "C" fn handler(_: RefAny, mut info: CallbackInfo) -> Update {
    info.stop_propagation();           // remaining handlers on the same node still run
    info.stop_immediate_propagation(); // nothing else runs
    Update::DoNothing
}
```

`stop_propagation` matches W3C `event.stopPropagation()`. `stop_immediate_propagation` matches `event.stopImmediatePropagation()`. See `core/src/events.rs:793` for the propagation implementation.

## Default actions

Some events have built-in behaviour that runs *after* every callback returns, unless a callback prevented it:

| event | default action |
|---|---|
| `Tab` | Move focus to next focusable element |
| `Shift+Tab` | Move focus to previous focusable element |
| `Enter` / `Space` on focused button | Synthetic click on the button |
| `Escape` | Clear focus or close modal |
| `Ctrl+A` in text input | Select all |
| Arrow keys in scroll container | Scroll by line |

To suppress the default action from a callback:

```rust,no_run
# use azul::prelude::*;
extern "C" fn on_keydown(_: RefAny, mut info: CallbackInfo) -> Update {
    info.prevent_default();
    Update::DoNothing
}
```

`prevent_default` corresponds to W3C `event.preventDefault()`. The W3C semantics are: the default action does not fire, but other callbacks for the same event still run. Combine with `stop_propagation` to also halt the propagation walk.

## Reading input state

`CallbackInfo` exposes the current input snapshot:

```rust,ignore
let kbd: KeyboardState = info.get_current_keyboard_state();
let mouse: MouseState  = info.get_current_mouse_state();
let win:  WindowFlags  = info.get_current_window_flags();
let state: &FullWindowState = info.get_current_window_state();
```

`KeyboardState` carries `pressed_virtual_keycodes` (a vec of currently held keys), `current_virtual_keycode` (the most recent), and helpers `shift_down()`, `ctrl_down()`, `alt_down()`, `super_down()`. `MouseState` carries `cursor_position`, `left_down`, `right_down`, `middle_down`. Definitions live in `core/src/window.rs:315` and `core/src/window.rs:418`.

Reading state inside the callback is the right way to check modifier keys for shortcuts:

```rust,no_run
# use azul::prelude::*;
# struct App;
extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let kbd = info.get_current_keyboard_state();
    if kbd.ctrl_down() && kbd.is_key_down(VirtualKeyCode::S) {
        info.prevent_default();
        // ... save ...
        return Update::RefreshDom;
    }
    Update::DoNothing
}
```

## Accelerator chords

`AcceleratorKey` lets you check a fixed chord against the current keyboard state:

```rust,no_run
# use azul::prelude::*;
# extern "C" fn handler(_: RefAny, info: CallbackInfo) -> Update {
let chord = [
    AcceleratorKey::Ctrl,
    AcceleratorKey::Shift,
    AcceleratorKey::Key(VirtualKeyCode::S),
];
let kbd = info.get_current_keyboard_state();
if kbd.matches_accelerator(&chord) {
    // Ctrl+Shift+S
}
# Update::DoNothing
# }
```

Use this for menu accelerators and global shortcuts. The implementation is in `core/src/window.rs:363`.

## The `On` shorthand

For the most common filters, `dom::On` covers the cases without making you spell out the variant:

```rust,no_run
# use azul::prelude::*;
# struct State;
# extern "C" fn cb(_: RefAny, _: CallbackInfo) -> Update { Update::DoNothing }
# let data: RefAny = RefAny::new(State);
let node = Dom::create_div()
    .with_callback(On::MouseUp.into(),     data.clone(), cb)
    .with_callback(On::TextInput.into(),   data.clone(), cb)
    .with_callback(On::FocusReceived.into(), data,       cb);
```

`On` converts to `EventFilter` automatically (`From<On> for EventFilter`). The mapping is opinionated: `On::TextInput` becomes a `Focus` filter; `On::VirtualKeyDown` becomes a `Window` filter; mouse events become `Hover`. See `core/src/events.rs:2105`.

When you need a non-default scope (e.g. text input on a `Hover`-scoped node), spell out `EventFilter::Hover(HoverEventFilter::TextInput)` directly.

## Common patterns

**Click:**

```rust,no_run
# use azul::prelude::*;
# struct S;
# extern "C" fn click(_: RefAny, _: CallbackInfo) -> Update { Update::DoNothing }
# let data: RefAny = RefAny::new(S);
Dom::create_div().with_callback(
    EventFilter::Hover(HoverEventFilter::LeftMouseUp),
    data,
    click,
);
```

**Hover effect** (use CSS `:hover` for visual change; use a callback only when you need to mutate state):

```rust,no_run
# use azul::prelude::*;
# struct S;
# extern "C" fn enter(_: RefAny, _: CallbackInfo) -> Update { Update::DoNothing }
# extern "C" fn leave(_: RefAny, _: CallbackInfo) -> Update { Update::DoNothing }
# let data: RefAny = RefAny::new(S);
Dom::create_div()
    .with_callback(EventFilter::Hover(HoverEventFilter::MouseEnter), data.clone(), enter)
    .with_callback(EventFilter::Hover(HoverEventFilter::MouseLeave), data,        leave);
```

**Window-level keyboard shortcut**:

```rust,no_run
# use azul::prelude::*;
# struct S;
# extern "C" fn on_key(_: RefAny, _: CallbackInfo) -> Update { Update::DoNothing }
# let data: RefAny = RefAny::new(S);
Dom::create_body().with_callback(
    EventFilter::Window(WindowEventFilter::VirtualKeyDown),
    data,
    on_key,
);
```

**Tab order**: call `.with_tab_index(TabIndex::Auto)` on the node to make it focusable; tab/shift-tab move through nodes in DOM order. Use `TabIndex::NoKeyboardFocus` to make a node focusable programmatically but skip it in tab navigation. Definitions in `core/src/dom.rs:1923`.

## Where it goes wrong

- **Callback never fires** — check the filter scope. `Hover(LeftMouseUp)` only fires when the cursor is over the node *at the moment of release*. If the user pressed inside, dragged out, and released outside, no click event fires on either node.
- **`Focus(...)` never fires** — the node has no tab index. Add `.with_tab_index(TabIndex::Auto)` so the node can receive focus.
- **Counter does not update** — the callback returned `Update::DoNothing`. Return `Update::RefreshDom` after mutating the model.
- **Default action still happens after `prevent_default`** — verify the call is on `CallbackInfo`, not on a stale copy. The change is applied after the callback returns; calling it twice is harmless.

## Next

- [Timers](timers.md) — frame-driven callbacks for animation and polling.
- [Scrolling and Drag-and-Drop](scrolling-and-drag.md) — scroll events and drag tracking.
- [Windows, Menus, Decorations](windowing.md) — multi-window, menu bars, context menus.
