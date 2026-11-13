# Global hotkeys — 2026-09-28

User request: "global hotkeys are the last thing still missing".

Branch `wt/global-hotkeys`, based on `7e020dc49` (PR #476, `fix/input-bugs-2026-09-19`).
Nothing here was compiled (parallel-agent rule): the parent compiles once and syncs
`api.json` with `azul-doc autofix`.

## 1. Design

### The shape: the tray's, not the media keys'

A global hotkey fires while ANOTHER app has the keyboard, on something that cannot
hold a `CallbackInfo`:

- the Carbon handler runs inside AppKit's dispatch;
- `WM_HOTKEY` lands on a message-only window procedure;
- X11 reports a `KeyPress` on the root window of a second display connection;
- the Wayland portal's `Activated` arrives on a D-Bus thread.

So the backends only **park an id** (`azul_layout::managers::global_hotkey::push_fired`).
Each run loop then calls a pump next to the tray's. The pump takes the fired
registrations (`take_fired`) and runs their callbacks against the app's **first
window** through `PlatformWindow::invoke_menu_callback`. That is exactly how a tray
menu click is delivered (`pump_tray` / `pump_tray_into_windows`), with the same
`MenuInvocation::Native` baseline handling.

Each registration carries its own callback and `RefAny`, like a menu item. There is
no DOM-level event filter. This avoids the "which window's DOM holds the listener"
problem that an app-level event would have in a multi-window app, and it needs no
`EventType` / `EventData` / filter-table plumbing (see §7 if a DOM route is wanted
later).

### App-wide, not per window (decided)

Every OS registers a hotkey for the **process**:

- Carbon: the application event target.
- Win32: the registering thread's queue.
- X11: the root window.
- The portal: the app's D-Bus session.

None of them can tie a hotkey to one window. A per-window API would be a lie the
moment its window closes while the grab stays. So:

- the id is app-wide;
- the registry is a process-global in `azul-layout` (not a `LayoutWindow` field);
- the callback runs against the first window;
- the registration outlives the window that made it.

### Layers

| layer | file | what |
|---|---|---|
| model | `core/src/global_hotkey.rs` | `GlobalHotkey` (modifiers + ONE key), `HotkeyModifiers`, `GlobalHotkeyId`, `GlobalHotkeyError`, `GlobalHotkeyStatus`, the accelerator parser / normaliser / validator, display string, the xkb-name / portal-trigger table |
| registry | `layout/src/managers/global_hotkey.rs` | `GlobalHotkeyRegistry` (testable value) + one process-wide instance, the backend seam `GlobalHotkeyBackend` (fn-pointer table, like `capture_common::CaptureVTable`), the bounded fire mailbox, `simulate`, `simulated_backend` |
| API | `layout/src/callbacks.rs`, `dll/src/desktop/app.rs` | `CallbackInfo::{register,unregister}_global_hotkey`, `get_global_hotkey_status`, `raise_window`; `App::{register,unregister}_global_hotkey` (before `run()`) |
| OS | `dll/src/desktop/global_hotkey/{mod,macos,windows,x11,portal}.rs` | the four backends, backend selection, the per-loop pumps |
| probe | `dll/src/desktop/extra/capability.rs`, `dll/src/unified/capability.rs` | `PlatformCapability::global_hotkeys()` |
| test hook | `layout/src/e2e/full.rs` | `{ "op": "global_hotkey", "accelerator": "Ctrl+Alt+K" }` |

### The rules the model enforces

- **Normalised**:
  - Left and right modifiers are one modifier.
  - Case, spacing and modifier order do not matter.
  - Equality is struct equality, so "already registered" is detectable.
- **Exactly one key.** Its modifiers are `ctrl`, `alt` (Option), `shift` and `meta`. `meta` is the PHYSICAL Cmd / Windows / Super key.
  - `CmdOrCtrl` / `Primary` in the parser means the platform's primary modifier.
  - `LWin` in a menu `VirtualKeyCodeCombo` means the same, following `menu::accelerator_matches`.
- **A key that types or moves a caret needs Ctrl, Alt or Cmd/Super.** Grabbing a bare or Shift-only `K` would swallow it in every other app.
  - Function, media and system keys (PrintScreen, Pause, ScrollLock) may stand alone.
  - Modifiers and lock keys are never the key.
- **One key table** (`NAMED_KEYS`) feeds the parser, the display string, the X11 grab (`XStringToKeysym`) and the portal trigger. A key cannot be parseable yet ungrabbable.
- **macOS and Win32 invert the existing keycode tables** (`macos_keycode_to_virtual_key`, `win32_vkey_to_virtual_key`), so no new platform table was added. `keycode_table_manifest_is_exhaustive` is untouched.

### The registry's contract

- `register`:
  - validate;
  - refuse a duplicate with `AlreadyRegistered(holder)`;
  - reserve an id (never reused, never 0);
  - ask the backend WITHOUT the lock.
  - An error leaves nothing behind, and a retry asks the backend again.
- `Pending` (the portal) is settled later by `report(id, Ok | Err)`. A `Failed` registration stays readable, with its reason, until it is unregistered. It does not fire and does not block a retry.
- `install_backend` MOVES live registrations onto the new backend: the old grab is released and the new one taken. This is how a registration made before `run()` ends up on the simulation in a headless run.
- The mailbox is bounded (64). Fires are not de-duplicated: two presses are two callbacks. A fire for a registration that is gone or failed is dropped.

### Loop wake-ups

| backend | arrives on | wakes the loop by itself | `needs_loop_polling` |
|---|---|---|---|
| Carbon | NSEvent → handler in `sendEvent:` | yes | false |
| Win32 | `WM_HOTKEY` in the loop's own queue | yes (`WaitMessage`) | false |
| X11 | a second X connection (fd not in the poll set) | no | true → X11/Wayland loops cap their park at 100 ms, like a live tray |
| portal | a D-Bus listener thread | no | true (same cap) |
| simulated | `simulate()` | no | true → headless loop polls at 60 Hz while one is registered |

## 2. Commits

| # | hash | subject |
|---|---|---|
| 1 | `94a35822a` | test(hotkeys): a global hotkey parses, registers, fires and is simulated headless — **RED** |
| 2 | `5e350dad7` | feat(hotkeys): the global-hotkey accelerator, registry and headless simulation |
| 3 | `f0f0d4118` | feat(hotkeys): Carbon, Win32, X11 and portal backends, delivered like tray clicks |
| 4 | `a8ee1f896` | feat(examples): a "Global hotkey" section in the widgets demo |
| 5 | (this file) | chore(scripts): the global-hotkeys report |

### Expected RED of commit 1

Commit 1 ships the final types with stubbed bodies:

- `parse_for` / `from_combo_for` → `InvalidAccelerator("not implemented yet")`;
- `validate` → `Ok`;
- `to_display_string_for` → `""`;
- the name functions → `None`;
- `begin_register` → `Unsupported`;
- the drains → empty.

`layout/tests/global_hotkeys.rs` (registered in `all.rs`):

| test | RED because |
|---|---|
| an_accelerator_parses_into_modifiers_and_one_key | `parse_for` is `Err` |
| an_accelerator_is_normalised | same |
| cmd_or_ctrl_is_the_platform_primary_modifier | same |
| a_malformed_accelerator_is_refused_with_a_reason | loop passes, the reason lacks "ONE key" |
| a_typing_key_needs_ctrl_alt_or_cmd | `"F13"` refused; `validate()` of a bare K is `Ok` |
| a_menu_combo_converts_with_the_menu_rules | `from_combo_for` is `Err` |
| the_display_string_is_the_platform_spelling_and_round_trips | `""` |
| the_linux_backends_get_xkb_names_and_a_portal_trigger | `None` |
| registering_grabs_at_the_backend_and_is_active | `register` → `Unsupported`, `expect` panics |
| a_duplicate_is_refused_and_names_the_holder | `unwrap` panics |
| a_combination_another_app_owns_is_reported_and_leaves_nothing_behind | reads `Unsupported`, not `TakenByAnotherApp` |
| an_invalid_combination_never_reaches_the_backend | `Unsupported`, not `InvalidAccelerator` |
| unregistering_releases_the_grab_and_the_id_is_never_reused | `unwrap` panics |
| a_pending_registration_is_settled_by_a_later_report | `unwrap` panics |
| replacing_the_backend_moves_the_registrations | `unwrap` panics |
| a_fire_is_delivered_with_its_own_callback_and_data | `unwrap` panics |
| a_fire_for_a_dropped_registration_is_not_delivered | `unwrap` panics |
| a_stuck_sender_cannot_grow_the_mailbox | `unwrap` panics |
| a_simulated_press_fires_the_registration_however_it_is_spelled | `unwrap` panics |
| the_process_wide_registry_runs_on_the_simulation | backend + probe pass, `register(..).expect` panics |
| without_a_backend_registration_is_unsupported | **passes** (control) |

Also:

- `layout/src/callbacks.rs` → `callback_info_flag_mutators_queue_exactly_one_matching_change` is RED on the new `raise_window()` row ("expected exactly one queued change"), because the stub queues nothing.
- `doc/src/autofix/module_map.rs` → `global_hotkey_types_resolve_to_app_not_gl` is RED: `GlobalHotkey` resolves to `gl`, because the module name `gl` matches inside "GLobal".
- The e2e manager-accounting gates stay green: `global_hotkey` is recorded in `UNOBSERVABLE_MANAGERS` and in `not_fingerprintable()`, with reasons.

Commit 2 makes every row green. Commit 3 is platform code; it has no unit tests (see §5).

## 3. Public API additions (for `azul-doc autofix`)

`api.json` was NOT edited. The module map now files `GlobalHotkey*` and
`HotkeyModifiers` under **`app`**; `GlobalHotkeyError`, the `Result*` types and
`OptionGlobalHotkeyId` go to `error` / `option` through the structural rules. The
demo's `use` lines assume exactly that.

### New types — `azul_core::global_hotkey`

**`HotkeyModifiers`**
- `#[repr(C)]`; derives Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default.
- Fields: `ctrl: bool`, `alt: bool`, `shift: bool`, `meta: bool`.
- Associated const `NONE`.
- Methods: `is_empty(&self) -> bool`, `has_non_shift(&self) -> bool`, `union(self, other) -> Self`, `primary_for(mac: bool) -> Self`, `primary() -> Self`.

**`GlobalHotkeyId`** and **`OptionGlobalHotkeyId`**
- `#[repr(C)]`; derives Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash.
- Field: `id: u32`.

**`GlobalHotkey`**
- `#[repr(C)]`; derives Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash.
- Fields: `modifiers: HotkeyModifiers`, `key: VirtualKeyCode`.
- Methods:
  - `new(modifiers, key)` (const)
  - `parse(&str) -> Result<Self, GlobalHotkeyError>`
  - `parse_for(&str, mac: bool)`
  - `from_combo(&VirtualKeyCodeCombo)`
  - `from_combo_for(&VirtualKeyCodeCombo, mac: bool)`
  - `validate(&self) -> Result<(), GlobalHotkeyError>`
  - `to_display_string(&self) -> AzString`
  - `to_display_string_for(&self, mac: bool) -> String`
- Suggested C entries:
  - constructor `new(modifiers, key)`
  - constructor `parse(accelerator: String) -> ResultGlobalHotkeyGlobalHotkeyError`, body `azul_core::global_hotkey::GlobalHotkey::parse(accelerator.as_str()).into()`
  - constructor `from_combo(combo: VirtualKeyCodeCombo) -> ResultGlobalHotkeyGlobalHotkeyError`, body `…::from_combo(&combo).into()`
  - function `to_display_string`

**`GlobalHotkeyError`**
- `#[repr(C, u8)]`; derives Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash.
- Variants: `InvalidAccelerator(AzString)`, `AlreadyRegistered(GlobalHotkeyId)`, `TakenByAnotherApp`, `KeyNotMappable`, `Denied`, `Unavailable(AzString)`, `Unsupported`, `Platform(AzString)`.
- Method: `to_display_string(&self) -> AzString`. Also implements `Display` and `std::error::Error`.

**`GlobalHotkeyStatus`**
- `#[repr(C, u8)]`.
- Variants: `NotRegistered`, `Pending`, `Active`, `Failed(GlobalHotkeyError)`.
- Method: `is_live(&self) -> bool`.

**Result types:** `ResultGlobalHotkeyIdGlobalHotkeyError` and `ResultGlobalHotkeyGlobalHotkeyError`, both from `impl_result!`.

**Free functions (internal, NOT for api.json):** `key_display_name`, `xkb_keysym_name`, `portal_trigger`.

### New methods on existing types

`azul_layout::callbacks::CallbackInfo`:
- `register_global_hotkey(&mut self, hotkey: GlobalHotkey, data: RefAny, callback: CoreCallback) -> ResultGlobalHotkeyIdGlobalHotkeyError`
  - api.json args: `self` refmut, `hotkey: GlobalHotkey`, `data: RefAny`, `callback: CallbackType`
  - body: `object.register_global_hotkey(hotkey, data, azul_layout::callbacks::Callback::create(callback).to_core())`
- `unregister_global_hotkey(&mut self, id: GlobalHotkeyId) -> bool`
- `get_global_hotkey_status(&self, id: GlobalHotkeyId) -> GlobalHotkeyStatus`
- `raise_window(&mut self)`

`azul_layout::callbacks::CallbackChange`:
- New variant `RaiseWindow`. It has arms in the dll's `apply_user_change` (calls `extra::window_activation::raise_window`) and in the headless e2e runner (a no-op).

`azul_dll::desktop::app::App`:
- `register_global_hotkey(&mut self, hotkey, data: RefAny, callback: CoreCallback) -> ResultGlobalHotkeyIdGlobalHotkeyError`, with the same body shape as the `CallbackInfo` one.
- `unregister_global_hotkey(&mut self, id) -> bool`

`PlatformCapability`:
- `global_hotkeys() -> PlatformCapability`, in both desktop `extra/capability.rs` and wasm `unified/capability.rs`.
- body: `azul_dll::unified::capability::PlatformCapability::global_hotkeys()`

### Internal (no api.json)

- `azul_layout::managers::global_hotkey::*`: `BackendGrant`, `GlobalHotkeyBackend`, `FiredHotkey`, `GlobalHotkeyRegistry`, `GlobalHotkeyProbe`, `MAX_PENDING_FIRES`, `SIMULATED_BACKEND_NAME`, `simulated_backend`, and the process-wide functions.
- `azul_dll::desktop::global_hotkey::*`
- The e2e `DebugEvent::GlobalHotkey` op.
- The `doc` module-map entries.

## 4. Per-platform behaviour and limits

### macOS — Carbon `RegisterEventHotKey` (HIToolbox, dlopen'd)

- **Permission:** none. A `CGEventTap` or `addGlobalMonitorForEvents…` would need Accessibility / Input Monitoring, and a global monitor cannot consume the key.
- **Deprecation status:** most of Carbon is deprecated. The hot-key calls carry no deprecation macro in `CarbonEvents.h`, still ship in 64-bit HIToolbox, have no Cocoa replacement, and are what Alfred, Raycast, Rectangle and Electron use. They are dlopen'd: a macOS that drops them degrades to `Unavailable` / capability `false` instead of failing to launch.
- **Registration:**
  - Exclusive (`kEventHotKeyExclusive`), so a combination another process holds exclusively gives `eventHotKeyExistsErr` → `TakenByAnotherApp`.
  - WindowServer-reserved shortcuts (Cmd+Tab, Cmd+Space) may register fine and never fire. macOS reports nothing for them.
- **Keys are POSITIONAL** (ANSI keycodes), the same as the window's own `keyDown:` table. On AZERTY, "Cmd+Shift+Q" is the key in the ANSI Q position.
- **Delivery:** in both run loops (the `RunForever` drain timer, and the manual loop before it parks) and in `run_tray_only`, so a menu-bar utility's summon key works.
- **Not delivered** when the X11 backend (`x11-macos`) draws the windows: NSApp is not pumped there.

### Windows — `RegisterHotKey`

- `WM_HOTKEY` goes to a message-only window, created lazily on the event-loop thread at the first registration. `MOD_NOREPEAT` is set.
- `ERROR_HOTKEY_ALREADY_REGISTERED` → `TakenByAnotherApp`.
- The VK comes from inverting `win32_vkey_to_virtual_key`. The OEM punctuation keys (`;` `/` `` ` `` `[` `\` `]` `'`) need the layout's character and come back `KeyNotMappable`. `Minus`, `Equals`, `Comma` and `Period` work.
- Win32 ids are mapped into `1..=0xBFFF`.
- Raising: a process whose hotkey just fired may take the foreground, so `raise_window()` works there.

### Linux X11 — `XGrabKey`

- The grab is on the root window of a dedicated connection. It is grabbed once per subset of {CapsLock, NumLock, ScrollLock} masks; NumLock's and ScrollLock's bits are read from the modifier mapping.
- `BadAccess` is caught by a temporary error handler + `XSync` → `TakenByAnotherApp`. The partial grabs are released.
- Detectable auto-repeat is on, so a held chord fires once.
- **Limits:**
  - Alt = Mod1 and Super = Mod4 are assumed, not read from the keymap.
  - A keymap change after registration does not re-grab.
  - The loop polls at 100 ms while a hotkey is registered, so worst-case latency is 100 ms.

### Linux Wayland — xdg-desktop-portal `GlobalShortcuts`

- **Chosen by session:** `WAYLAND_DISPLAY` set → portal, even when azul's windows go through XWayland (an X grab only sees XWayland-focused keys).
- **Availability:**
  - It is probed for real, once: the interface's `version` property.
  - It needs xdg-desktop-portal 1.17+ and a backend that implements the interface: KDE Plasma 5.27+, GNOME 48+, Hyprland.
  - sway / wlroots-portal and older GNOME → capability `false` with that reason, and registration → `Unavailable`.
- **Handshake:**
  - One session per registration: `CreateSession` then `BindShortcuts` with one shortcut. It runs on a thread because the desktop may show the user a dialog.
  - The registration is `Pending` until then: `Denied` if the user cancels, `Platform` on other refusals.
  - Unregister = `Session.Close`.
  - `Activated` is received by one listener thread.
- **Transport:** zbus, not the tray's libdbus table. That table cannot receive signals: it has no match rules or filters, and the tray's own docs say so. Every portal `Response` and `Activated` is a signal, and zbus is what the repo's other portal clients use (eyedropper, permission, screencap).
- **Limits:**
  - The user may pick a different trigger in the dialog. The app is not told (`ShortcutsChanged` is not wired).
  - Unsandboxed (host) apps may need an app id registered with the portal (`org.freedesktop.host.portal.Registry`, portal 1.19+). This is not done yet, and some desktops may refuse without it.
  - `raise_window()` is refused on Wayland by design (no `xdg_activation` token). Newer portal versions pass an `activation_token` in `Activated`'s options; that is unused.

### Other platforms

- **iOS / Android / web:** no backend. Capability `false`; registration → `Unsupported`.
- **Headless** (`AZ_BACKEND=headless`):
  - `run_headless` installs the simulated backend, so nothing is grabbed at the OS, and moves earlier registrations onto it.
  - Presses come from `simulate()` or the `global_hotkey` e2e op.
  - The headless loop delivers them in its new Phase 1c.

## 5. Manual check recipes

Build the widgets demo with the rebuilt dll. Open the **Global hotkey** section, below Menus.

### macOS

1. **Capability:** reads `available - Carbon RegisterEventHotKey (HIToolbox)`.
2. **Register:** click **Register Cmd+Shift+K**. Status reads `Cmd+Shift+K is active …`.
3. **Fire from another app:** focus Finder and press Cmd+Shift+K. AzWidgets comes to the front and **Fired** reads 1. With `AZ_LOG=debug` the log shows `[global-hotkey] Cmd+Shift+K fired`.
4. **Conflict:** start a second AzWidgets and register there. It shows `…the combination is taken…`, because the first holds it exclusively.
5. **Unregister:** click Unregister, then press the combination. Nothing happens, and the second instance can now register it.
6. **Default loop:** repeat step 3 with the default `EndProcess` termination (the manual loop) and with `RunForever`.

### Windows

1. **Capability:** reads `available - Win32 RegisterHotKey (message-only window)`.
2. **Fire from another app:** register Ctrl+Alt+K, focus Notepad and press it. AzWidgets comes to the foreground and the counter increments.
3. **Conflict:** a second instance shows "taken".
4. **Auto-repeat:** hold the chord. The counter moves by one (`MOD_NOREPEAT`).
5. **Optional:** Spy++ shows the message-only window `AzulGlobalHotkeyWindow`.

### Linux, X11 session (`echo $XDG_SESSION_TYPE` → `x11`)

1. **Capability:** reads `X11 XGrabKey (root window)`.
2. **Fire from another app:** register Ctrl+Alt+K, focus a terminal and press it. The counter increments and the window is raised through `_NET_ACTIVE_WINDOW`.
3. **Lock keys:** toggle NumLock and CapsLock on. The chord still fires.
4. **Auto-repeat:** hold the chord. The counter moves by one.
5. **Conflict:** a second instance, or `xbindkeys` bound to Ctrl+Alt+K, shows "taken". The process must NOT exit (the default Xlib error handler would have exited it).

### Linux, Wayland

1. **Check the portal:**

   ```
   busctl --user get-property org.freedesktop.portal.Desktop \
     /org/freedesktop/portal/desktop org.freedesktop.portal.GlobalShortcuts version
   ```

2. **KDE Plasma 6 / GNOME 48+:**
   - Capability reads `xdg-desktop-portal GlobalShortcuts`.
   - Register. Status reads `waiting for the desktop to confirm`, and the desktop's dialog appears.
   - Accept, then press the chord from another app. The counter increments; the raise is refused, which is logged.
3. **Cancel the dialog:** a later Register/Unregister cycle shows the refusal. Note that the status text only refreshes when the section re-renders (see §7).
4. **sway without the interface:** capability reads `unavailable - …: xdg-desktop-portal has no GlobalShortcuts interface…`, and Register shows the `Unavailable` text.

### Headless / AZ_E2E

1. Run with `AZ_BACKEND=headless` and the debug server (`AZ_DEBUG=<port>`), or as an `AZ_E2E` scenario.
2. `{"op":"find_node_by_text","text":"Register Ctrl+Alt+K"}`, then `{"op":"click_node","node_id":N}`. On a Mac host the button reads Cmd+Shift+K.
3. `{"op":"global_hotkey","accelerator":"Ctrl+Alt+K"}`, then `{"op":"wait_frame"}`.
4. `{"op":"find_node_by_text","text":"1 time(s) since it was registered"}` finds the node.

`global_hotkey` with an unregistered combination returns an error that lists the live registrations.

## 6. Least sure to compile

1. **`portal.rs` against zbus 5.19:**
   - a single-argument body `&create_options` (a `HashMap<&str, Value>`);
   - the 4-tuple `BindShortcuts` body containing `ObjectPath`, `Vec<(&str, HashMap<&str, Value>)>`, `&str` and `HashMap`;
   - `zbus::blocking::proxy::SignalIterator<'_>` as a parameter type;
   - `get_property::<u32>`;
   - `message.body().deserialize::<(OwnedObjectPath, String, u64, HashMap<String, OwnedValue>)>()`.
2. **libloading function symbols:** `*lib.get::<FnType>(b"…\0").ok()?` inside `OnceLock::get_or_init(|| unsafe { … })` closures returning `Option<_>`. This is used in `macos.rs`, `windows.rs` and `x11.rs`; it is the pattern of `permission/macos.rs`, but more of it.
3. **`x11.rs`:**
   - `Option<XErrorHandler>` as an FFI argument / return type in `XSetErrorHandlerFn`, and `Some(grab_error_handler as XErrorHandler)`;
   - the `XEvent` → `XKeyEvent` pointer cast;
   - `1 << modifier` typed as `c_uint`.
4. **`macos.rs`:** `c_ulong` for Carbon's `ItemCount` / `ByteCount` (unsigned long, 64-bit), and the `EventHotKeyId` struct passed by value through a fn pointer.
5. **`windows.rs`:** `WndClassExW` with `Option<unsafe extern "system" fn…>`, `(-3_isize) as Hwnd`, and the `if let … state()` guard scopes.
6. **`global_hotkey/mod.rs`:**
   - `deliver_fired<W: PlatformWindow>` calling the trait's default `invoke_menu_callback` on `Win32Window` / `HeadlessWindow` / X11 / Wayland windows;
   - `request_redraw()` being inherent on `MacOSWindow` / `X11Window` / `WaylandWindow` (the tray relies on the same);
   - `#[cfg(az_x11)]` on `pump_into_first_linux_window`.
7. **`core/src/global_hotkey.rs`:**
   - `use crate::window::{VirtualKeyCode, VirtualKeyCode as K, …}`;
   - the `impl_option!` / `impl_result!` arms;
   - inline `{accelerator:?}` args in `alloc::format!` under `no_std`;
   - the `match k { K::LControl | … , other => { …; continue; } }` over `&VirtualKeyCode`.
8. **`layout/src/managers/global_hotkey.rs`:** `#[derive(Debug)]` on a struct of higher-ranked fn pointers (the same shape as `CaptureVTable`), and `Mutex::new(GlobalHotkeyRegistry::new())` in a `static` (a const fn with `Vec::new()`).
9. **`layout/tests/global_hotkeys.rs`:** `callback_a as usize` fn-item casts, `VirtualKeyCodeVec::from_vec`, and `RefAny::downcast_ref` on a clone.
10. **The demo** (`examples/azul-widgets/src/hotkeys.rs`) depends on the GENERATED bindings:
    - module paths `azul::app::{GlobalHotkey, GlobalHotkeyId, GlobalHotkeyStatus, HotkeyModifiers}`, `azul::error::ResultGlobalHotkeyIdGlobalHotkeyError` and `azul::window::PlatformCapability`;
    - methods `register_global_hotkey` / `unregister_global_hotkey` / `get_global_hotkey_status` / `raise_window` and `GlobalHotkeyError::to_display_string`.

    If autofix files a type elsewhere, only the `use` block moves.
11. **`layout/src/e2e/full.rs`:** the new `GlobalHotkey` op arm assumes `needs_update` is in scope there. It is used by the neighbouring arms.

**Merge-conflict risk.** The notifications agent also edits the tray pump sites. The likely spots:
- the `has_tray` timeout lines in `x11/mod.rs` and `wayland/mod.rs`;
- the pump sites in `run.rs`;
- `layout/tests/all.rs`.

My insertions are separate lines.

## 7. Still open

- **api.json sync** through `azul-doc autofix` (§3). This includes the `callback` → `CoreCallback` conversion in the two `register_global_hotkey` bodies.
- **Portal gaps:**
  - The host-app app id (`org.freedesktop.host.portal.Registry.Register`) is not registered.
  - `ShortcutsChanged` (the user re-bound the trigger) is not wired.
  - `activation_token` → `xdg_activation` is not used, so a Wayland raise is refused.
- **Status freshness:** a late portal answer (`Pending` → `Active` / `Failed`) is readable through `get_global_hotkey_status`, but no callback is told. The demo refreshes its status line only on its own button. A "status changed" notification would need an event or a registration-level callback.
- **No DOM event route:** there is no `ApplicationEventFilter::GlobalHotkey` / `EventType::GlobalHotkey`. Delivery is per-registration callbacks only.
- **Windows:** OEM punctuation keys need `VkKeyScanW`.
- **X11:** Mod1/Mod4 are assumed; there is no re-grab on a keymap change (`MappingNotify`).
- **Headless runner:** the layout-level e2e runner (`layout/src/e2e/runner.rs`) does not pump hotkeys. Only the dll's headless shell does. An `AZ_E2E` scenario therefore runs under the dll.
- **No scenario test yet** exercises the `global_hotkey` op end to end. The registry-level simulation is unit-tested.
- **`run_tray_only`** (and so tray-only hotkeys) is macOS-only, as the tray-only mode already was.
