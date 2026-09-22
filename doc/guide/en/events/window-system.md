---
slug: events/window-system
title: Window & System
language: en
canonical_slug: events/window-system
audience: external
maturity: wip
guide_order: 68
topic_only: false
short_desc: Window state, monitors, transient windows, tooltips, screenshots and system capabilities
prerequisites: [hello-world, events, events/callbacks]
tracked_files:
  - layout/src/callbacks.rs
  - core/src/window.rs
  - core/src/gamepad.rs
  - core/src/keyring.rs
default-search-keys:
  - CallbackInfo
  - FullWindowState
  - WindowCreateOptions
  - Monitor
  - Capability
  - modify_window_state
  - create_window
  - get_monitors
  - get_permission_status
---

# Window & System

Everything a callback can ask about the world outside its own DOM: the
window it lives in, the monitor that window is on, and the machine's
capabilities - from safe-area insets to the keyring.

## Introduction

These methods are all on `CallbackInfo`. The window ones read and write
a state the shell owns; the system ones are thin wrappers over platform
services, and most of them can fail or be unavailable, so they return
options or results. Assume nothing is present.

## Reading and changing the window

The window is one struct, read whole and written whole:

```rust,ignore
let mut state = info.get_current_window_state();
state.title = "Untitled - edited".into();
info.modify_window_state(state);
```

`get_previous_window_state()` is the same struct one event ago, which
is how you detect *what changed*: a resize callback that wants the
delta, or a handler that should only act when the window actually
became focused rather than on every event while it is focused.

`get_current_window_flags()` and `get_previous_window_flags()` are the
cheaper pair for the boolean subset - maximised, minimised, fullscreen,
focused - when a whole `FullWindowState` is more than the question
needs.

`queue_window_state_sequence()` applies several states in order on
successive frames. That exists because some transitions are illegal as
a single jump on some platforms - leaving fullscreen *and* changing size
in one step, say - and the sequence lets the shell perform them the way
the platform requires.

## More windows

```rust,ignore
let mut opts = WindowCreateOptions::create(layout_settings);
opts.window_state.title = "Settings".into();
info.create_window(opts);
```

`create_window()` opens a real OS window sharing the same `RefAny` data
model; `close_window()` closes the one the callback is running in.
`get_current_window_handle()` hands back the raw platform handle for
interop with a native library that needs one.

## Transient windows

A popup, a dropdown, a tear-off panel: nodes in your DOM that the shell
can promote to real OS windows, so they escape the parent window's
bounds without you building a second DOM.

```rust,ignore
info.set_transient_window_open(node, true);   // show it
info.set_transient_window_torn(node, true);   // user dragged it off
```

`get_transient_window_zone(node)` resolves which transient zone a node
belongs to, which is what a handler inside a popup uses to find the
popup it is in.

## Monitors

```rust,ignore
let here = info.get_current_monitor();   // the one this window is on
let all  = info.get_monitors();          // every attached monitor
```

Each `Monitor` carries its position, size and scale factor. Place a new
window with these rather than assuming an origin, because on a
multi-monitor desktop the coordinate origin is not where you think it
is - and a secondary monitor can sit at negative coordinates.

`get_safe_area_insets()` is the mobile counterpart: the notch, the home
indicator and the rounded corners, as insets to pad your root by.

## Tooltips

```rust,ignore
info.show_tooltip("Delete permanently".into());
info.hide_tooltip();
```

Native tooltips, drawn by the platform, so they can leave the window.
`show_tooltip_at()` places one at a position of your choosing instead of
at the pointer.

## Screenshots

`take_screenshot(dom_id)` renders a DOM through azul's own pipeline and
returns PNG bytes, with `take_screenshot_base64()` for the same thing as
text. The `native` pair - `take_native_screenshot_bytes()` and
`take_native_screenshot_base64()` - asks the *compositor* for what is
actually on screen instead.

The distinction matters for testing. Only the native form proves a frame
reached the display; azul's own renderer can happily produce a correct
image for a window that never painted.

## Permissions and capabilities

Anything privacy-sensitive is gated:

```rust,ignore
match info.get_permission_status(Capability::Location) {
    PermissionState::Granted => { /* ... */ }
    _ => { /* ask, or degrade */ }
}
```

Check before use rather than calling and handling the failure -
platforms differ in whether a denied call is silent, and a check is the
only portable answer.

## Secrets and biometrics

The keyring stores a secret in the platform's own credential store -
Keychain, Credential Manager, Secret Service - never in your files:

```rust,ignore
info.keyring_store("api-token".into(), secret, /* require_biometry */ true);
info.keyring_get("api-token".into());
```

Both are asynchronous. `keyring_get()` starts a retrieval; the value
arrives at a later callback, where `get_keyring_result()` reads it. The
same shape applies to biometrics: `request_biometric_auth()` starts the
prompt and `get_biometric_result()` reads the outcome later, with
`get_biometric_kind()` reporting whether this machine does fingerprint,
face, or nothing at all.

## More methods

**Window** - `get_current_window_state`, `get_previous_window_state`,
`get_current_window_flags`, `get_previous_window_flags`,
`modify_window_state`, `queue_window_state_sequence`, `create_window`,
`close_window`, `get_current_window_handle`, `set_icon`.
`set_icon(node, spec)` sets the window icon from a node's rendered
content.

**Transient windows** - `set_transient_window_open`,
`set_transient_window_torn`, `get_transient_window_zone`.

**Monitors and layout environment** - `get_current_monitor`,
`get_monitors`, `get_safe_area_insets`.

**Tooltips** - `show_tooltip`, `show_tooltip_at`, `hide_tooltip`.

**Screenshots and colour** - `take_screenshot`, `take_screenshot_base64`,
`take_native_screenshot_bytes`, `take_native_screenshot_base64`,
`pick_screen_color`, `get_picked_screen_color`. The colour pair is the
eyedropper: `pick_screen_color()` starts the pick and
`get_picked_screen_color()` reads what the user landed on.

**System appearance and behaviour** - `get_system_style`,
`reload_system_fonts`, `get_system_natural_scroll`,
`has_system_natural_scroll`, `get_system_time_fn`.
`get_system_style()` is the platform's theme - light or dark, accent
colour, font sizes - and `has_system_natural_scroll()` tells you whether
the platform reported a preference at all before
`get_system_natural_scroll()` gives you its value.

**Devices** - `get_gamepad_state`, `get_primary_gamepad`,
`get_hid_devices`, `get_hid_reports`, `get_sensor_reading`,
`get_location_fix`. Sensors and location are permission-gated;
`get_sensor_reading(kind)` covers accelerometer, gyroscope and the rest
behind one call.

**Secrets** - `keyring_store`, `keyring_get`, `keyring_delete`,
`get_keyring_result`, `request_biometric_auth`, `get_biometric_result`,
`get_biometric_kind`.

**Remote selections** - `set_remote_selections`,
`clear_remote_selections`. These render other users' cursors and
selections in a collaborative document; see
[Text Selection](text-selection.md).

**Updates** - `check_for_updates`, which runs the check on a thread and
returns its `ThreadId`.
