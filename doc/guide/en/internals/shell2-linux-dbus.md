---
slug: shell2-linux-dbus
title: Shell2 — Linux DBus
language: en
canonical_slug: shell2-linux-dbus
audience: contributor
maturity: wip
guide_order: null
topic_only: false
short_desc: Linux DBus integration for a11y, dialogs, notifications
prerequisites: [shell2-common, shell2-linux-x11]
tracked_files:
  - dll/src/desktop/shell2/linux/dbus/dlopen.rs
  - dll/src/desktop/shell2/linux/dbus/mod.rs
  - dll/src/desktop/shell2/linux/gnome_menu/actions_protocol.rs
  - dll/src/desktop/shell2/linux/gnome_menu/manager.rs
  - dll/src/desktop/shell2/linux/gnome_menu/menu_conversion.rs
  - dll/src/desktop/shell2/linux/gnome_menu/menu_protocol.rs
  - dll/src/desktop/shell2/linux/gnome_menu/mod.rs
  - dll/src/desktop/shell2/linux/gnome_menu/protocol_impl.rs
  - dll/src/desktop/shell2/linux/gnome_menu/shared_dbus.rs
  - dll/src/desktop/shell2/linux/gnome_menu/x11_properties.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
---

> **WIP** — works on Wayland-on-Mutter and X11-on-GNOME 3+ when the
> compositor exposes the GTK menu globals. KDE / non-GNOME compositors
> fall back to the in-window CSD menu bar.

The `dbus/` and `gnome_menu/` subtrees implement the GTK
`org.gtk.Menus` / `org.gtk.Actions` DBus protocol so Azul windows can
publish their menu bar to GNOME Shell's panel ("global menu"). Azul
loads `libdbus-1.so.3` at runtime — there is no compile-time
dependency on libdbus-dev, so cross-compiling from macOS still
produces a Linux binary that picks up DBus when the target system has
it installed.

## Two layers

```text
shell2/linux/
├── dbus/                      ← Generic libdbus-1 dlopen layer
│   ├── mod.rs                 (re-exports DBusLib, DBusConnection, …)
│   └── dlopen.rs              (DynamicLibrary loader for libdbus-1)
└── gnome_menu/                ← Application of dbus/ to GNOME's menu protocol
    ├── mod.rs                 (should_use_gnome_menus() entry point)
    ├── shared_dbus.rs         (OnceLock<Arc<DBusLib>>)
    ├── manager.rs             (GnomeMenuManager — owns the connection)
    ├── menu_conversion.rs     (azul Menu → DbusMenuGroup)
    ├── menu_protocol.rs       (DbusMenuItem / DbusMenuGroup types)
    ├── actions_protocol.rs    (PendingMenuCallback queue)
    ├── protocol_impl.rs       (object-path message handlers)
    └── x11_properties.rs      (sets _GTK_* atoms on the X11 window)
```

## DBusLib — the dlopen layer

`dbus/dlopen.rs:27` defines `DBusLib`:

```rust,ignore
pub struct DBusLib {
    _lib: Library,
    pub dbus_bus_get: unsafe extern "C" fn(c_int, *mut DBusError) -> *mut DBusConnection,
    pub dbus_connection_unref: unsafe extern "C" fn(*mut DBusConnection),
    pub dbus_connection_read_write_dispatch:
        unsafe extern "C" fn(*mut DBusConnection, c_int) -> c_int,
    pub dbus_connection_flush: unsafe extern "C" fn(*mut DBusConnection),
    pub dbus_bus_request_name:
        unsafe extern "C" fn(*mut DBusConnection, *const c_char, c_uint, *mut DBusError) -> c_int,
    pub dbus_connection_register_object_path: unsafe extern "C" fn(
        *mut DBusConnection, *const c_char, *const DBusObjectPathVTable, *mut c_void,
    ) -> c_int,
    pub dbus_message_new_method_return: unsafe extern "C" fn(*mut DBusMessage) -> *mut DBusMessage,
    pub dbus_message_iter_init:        unsafe extern "C" fn(*mut DBusMessage, *mut DBusMessageIter) -> c_int,
    pub dbus_message_iter_get_arg_type: unsafe extern "C" fn(*mut DBusMessageIter) -> c_int,
    // … ~30 more function pointers, one per libdbus-1 entry point we use
}
```

`DBusLib::new` calls `Library::load("libdbus-1.so.3")` then
`load_first_available::<Library>(&["libdbus-1.so.3", "libdbus-1.so"])`
under the hood; each function pointer is populated by `load_symbol!`.

Constants exposed from `dbus/mod.rs:9`:

- **`DBUS_BUS_SESSION`.** Connect to the user's session bus rather than the system bus.
- **`DBUS_NAME_FLAG_DO_NOT_QUEUE`.** Refuse to queue if the requested name is already taken.
- **`DBUS_HANDLER_RESULT_HANDLED` / `_NOT_YET_HANDLED` / `_NEED_MEMORY`.** Return values from object-path message handlers.
- **`DBUS_TYPE_STRING` / `_UINT32` / `_ARRAY` / `_VARIANT`.** Argument-type tags used during message marshalling.

Many other type constants
(`DBUS_TYPE_BOOLEAN`, `DBUS_TYPE_INT32`, `DBUS_TYPE_DOUBLE`, etc.) are
defined in `dlopen.rs:219` but not currently re-exported; the
autoreview report flags them as
`[MEDIUM] Dead Code` — they're kept for future protocol expansion.

`DBusError` (`dlopen.rs:177`) is the C-ABI error struct. The `dummy*`
and `pad*` field names mirror the upstream
`/usr/include/dbus-1.0/dbus/dbus-types.h` — these are not stub
fields, they are the layout libdbus expects.

## should_use_gnome_menus

`gnome_menu/mod.rs:66` is the gate:

```rust,ignore
pub fn should_use_gnome_menus() -> bool {
    if env::var("AZUL_DISABLE_GNOME_MENUS").unwrap_or_default() == "1" { return false; }
    let desktop = env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    if !desktop.to_lowercase().contains("gnome") { return false; }
    if env::var("DBUS_SESSION_BUS_ADDRESS").is_err() { return false; }
    true
}
```

Both X11 and Wayland windows call this at startup. When false, no
DBus connection is opened and the windowing backend renders its menu
bar in the title-bar area as part of the client-side decorations.

`AZUL_GNOME_MENU_DEBUG=1` enables verbose logging via `debug_log` —
useful when GNOME Shell silently ignores a published menu.

## GnomeMenuManager

`manager.rs:22`:

```rust,ignore
pub struct GnomeMenuManager {
    app_name: String,
    bus_name: String,                                   // "org.gtk.{app_name}"
    object_path: String,                                // "/org/gtk/{app}"
    dbus_lib: Arc<DBusLib>,
    connection: *mut DBusConnection,
    menu_groups: Arc<Mutex<HashMap<u32, DbusMenuGroup>>>,
    actions: Arc<Mutex<HashMap<String, DbusAction>>>,
}
```

`GnomeMenuManager::new(app_name, dbus_lib)`:

1. Sanitises `app_name` (`'.' / ' ' / '-' → '_'`) so it's a valid
   DBus name component.
2. Computes `bus_name = format!("org.gtk.{}", sanitized)` and
   `object_path = format!("/org/gtk/{}", sanitized.replace('_','/'))`.
3. `dbus_bus_get(DBUS_BUS_SESSION, &mut error)` to open the
   session bus connection.
4. `dbus_bus_request_name(connection, bus_name,
   DBUS_NAME_FLAG_DO_NOT_QUEUE, &mut error)` to claim the name.
5. `register_menus_interface(...)` and
   `register_actions_interface(...)` to install the message
   handlers on the object path.

The `dbus_lib` is loaded once via
`shared_dbus::get_shared_dbus_lib()` — a `OnceLock<Option<Arc<DBusLib>>>`
that ensures a single `dlopen` per process even with many windows.

The `*mut DBusConnection` is **not** wrapped in `Arc` —
`Drop for GnomeMenuManager` calls `dbus_connection_unref` when the
window is closed.

## org.gtk.Menus interface

`menu_protocol.rs:8`:

```rust,ignore
pub struct DbusMenuItem {
    pub label: String,
    pub action: Option<String>,           // "app.{action_name}"
    pub target: Option<String>,           // optional argument
    pub submenu: Option<(u32, u32)>,      // (group_id, menu_id)
    pub section: Option<(u32, u32)>,      // for separators
    pub enabled: bool,
}

pub struct DbusMenuGroup {
    pub group_id: u32,
    pub menu_id: u32,
    pub items: Vec<DbusMenuItem>,
}
```

`menu_conversion.rs:29` (`MenuConversion::convert_menu`) walks the
recursive `azul_core::menu::Menu` tree and flattens it: each level of
submenus becomes its own `DbusMenuGroup` with a fresh `group_id`.
The `submenu: Some((id, 0))` field on a parent item references the
child group by id — that is how GTK reconstructs the hierarchy on
the panel side.

`extract_actions` (`menu_conversion.rs:49`) is the second pass over
the same tree: every menu item with a callback becomes a `DbusAction`
named `app.{label}` (lower-cased + sanitised). The callback itself is
kept on `DbusAction.menu_callback: Option<CoreMenuCallback>` so the
event loop can dispatch it later.

The `menu_protocol.rs` autoreview report flags that `DbusMenuItem.enabled`
may not be serialised into the DBus variant dict — verify against
`protocol_impl.rs::menus_message_handler` before relying on it.

## org.gtk.Actions interface

`actions_protocol.rs` documents the four DBus methods Azul implements
on its `org.gtk.Actions` object:

- **`List`.** Signature `() → as`. Returns the action-name array.
- **`Describe`.** Signature `(s) → (bsav)`. Returns `(enabled, param_type, state)` for one action.
- **`DescribeAll`.** Signature `() → a{s(bsav)}`. Returns all actions with descriptions.
- **`Activate`.** Signature `(s, av, a{sv})`. Invokes a callback.

Activation runs on the libdbus dispatch thread, *not* the window's
event-loop thread. Calling the Azul callback there is unsafe — it
would race the layout pass.

The fix:

```rust,ignore
pub struct PendingMenuCallback {
    pub action_name: String,
    pub menu_callback: CoreMenuCallback,    // RefAny + fn-ptr
}

static PENDING_MENU_CALLBACKS: LazyLock<Mutex<Vec<PendingMenuCallback>>> =
    LazyLock::new(|| Mutex::new(Vec::new()));

pub fn queue_menu_callback(callback: PendingMenuCallback) { /* push */ }
pub fn drain_pending_menu_callbacks() -> Vec<PendingMenuCallback> { /* take */ }
```

The DBus handler queues; the X11/Wayland event loop drains and
dispatches the callbacks at a safe point (between
`process_window_events` and `regenerate_layout`).

## Object-path registration

`protocol_impl.rs:32` (`register_menus_interface`) and `:80`
(`register_actions_interface`) install message handlers via
`dbus_connection_register_object_path`:

```rust,ignore
let vtable = DBusObjectPathVTable {
    unregister_function: Some(menus_unregister_handler),
    message_function:    Some(menus_message_handler),
    ..
};
let state = Box::new(HandlerState { dbus_lib, menu_groups, actions });
dbus_connection_register_object_path(
    connection,
    object_path_cstr.as_ptr(),
    &vtable as *const _,
    Box::into_raw(state) as *mut c_void,
);
```

The `state` box is leaked — it must outlive every async DBus
message; libdbus passes it back as `user_data` to the message
function. The `unregister_function` is the cleanup hook, fired by
libdbus when the connection drops.

`menus_message_handler` decodes the incoming `DBusMessage`, switches
on `dbus_message_get_member` (`"Start"`, `"End"`, `"List"`, …), and
marshals the response with `dbus_message_iter_open_container` /
`dbus_message_iter_append_basic`.

## X11 window properties

GNOME Shell needs to know which DBus name and object path serves a
given window. On X11, this is published through window properties
(`x11_properties.rs:23`):

- Atom `_GTK_APPLICATION_ID` carries the app name string.
- Atom `_GTK_UNIQUE_BUS_NAME` carries `org.gtk.{app_name}`.
- Atom `_GTK_APPLICATION_OBJECT_PATH` carries `/org/gtk/{app_name_with_slashes}`.
- Atom `_GTK_APP_MENU_OBJECT_PATH` carries `{object_path}/menus/AppMenu`.
- Atom `_GTK_MENUBAR_OBJECT_PATH` carries `{object_path}/menus/MenuBar`.

All five atoms are interned via `XInternAtom`, and values are written
with `XChangeProperty(format=8, type=UTF8_STRING)`. GNOME Shell polls
these properties when the window maps; once set, the menu bar
appears in the panel.

The Wayland equivalent uses the same protocol but a different
discovery path (Mutter introspects the `org.gtk.*` services via
`org.freedesktop.DBus.ListNames` instead of window properties) — the
manager publishes identically; the X11-properties step is simply
skipped on Wayland.

## End-to-end flow

```text
Azul Menu (azul_core::menu::Menu)
   │
   │ MenuConversion::convert_menu
   ▼
Vec<DbusMenuGroup>  +  Vec<DbusAction>
   │                       │
   │  menu_groups Arc      │  actions Arc
   ▼                       ▼
GnomeMenuManager
   │   register_menus_interface (org.gtk.Menus)
   │   register_actions_interface (org.gtk.Actions)
   │   X11Properties::set_properties (X11 only)
   ▼
GNOME Shell ─── panel renders menus
   │
   │  user clicks → DBus org.gtk.Actions.Activate("app.foo", ...)
   ▼
actions_protocol::queue_menu_callback (DBus thread)
   │
   ▼
event loop ── drain_pending_menu_callbacks ── invoke CoreMenuCallback
                                              with full CallbackInfo
```

## Testing without DBus

The handful of unit tests in this subtree
(`mod.rs:154`, `dlopen.rs:459`, `protocol_impl.rs`) all
`#[cfg_attr(miri, ignore)]` because Miri can't dlopen, and they
gracefully degrade when `libdbus-1.so` is not installed
(`shared_dbus::test_dbus_library_loading` reports either
"loaded successfully" or "not available").

For integration testing on a CI runner without GNOME, set
`AZUL_DISABLE_GNOME_MENUS=1` to force the in-window menu bar path
and avoid every code path on this page.

## Where to add new menu features

- **Map a new `MenuItem` variant (e.g., toggleable check).** Handle it in the recursive walk.
  - `menu_conversion.rs`
- **Add a new DBus method on `org.gtk.Actions`.** Update the interface comment and the message handler.
  - `actions_protocol.rs`
  - `protocol_impl.rs::actions_message_handler`
- **Support a new desktop-environment-specific atom.** Add the atom write.
  - `x11_properties.rs::set_properties`
- **Support a non-GNOME menu protocol (KDE Plasma, ...).** Add a new sibling module under `linux/` and reuse `dbus/dlopen.rs`.

## Coming Up Next

- [Shell2 — Linux Wayland](shell2-linux-wayland.md) — Linux Wayland shell - wl_surface, xdg-shell, libinput
- [Shell2 — Linux X11](shell2-linux-x11.md) — Linux X11 shell - Xlib, GLX, XInput2
- [Accessibility Backends](accessibility-backends.md) — Per-platform a11y back-ends - UIA, AT-SPI, NSAccessibility
