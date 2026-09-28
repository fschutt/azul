# Loop wake-ups, plain tray clicks, and which window gets app events - 2026-09-28

Branch `wt/loop-wakeups-tray`, based on `5414bfa6b` (PR #476, `fix/input-bugs-2026-09-19`).
Nothing here was compiled or type-checked (wave rule); every claim about behaviour below is from
reading the code.

Three problems, one shape: the tray, native notifications and global hotkeys belong to the APP.
Each has an OS callback that cannot hold a `CallbackInfo`, parks something in a process-wide
mailbox, and relies on the run loop to (a) wake up and (b) run the app's callback against some
window. Before this branch:

1. **Timed polling.** The X11 and Wayland loops capped their `poll(2)` park at 100 ms whenever a
   tray was live, a notification was outstanding or a hotkey was registered. The D-Bus socket, the
   X11 hotkey grab connection and the portal listener thread were not in the wait set. That meant
   ten wake-ups a second for the life of a tray icon, and up to 100 ms of latency per click.
2. **Plain tray clicks were lost.** `drain_tray_events()` had no caller. On top of that, a
   menu-less macOS status item had no button action, so nothing was even queued, and Linux dropped
   a dbusmenu pick whose item had no callback.
3. **"The first window" was arbitrary.** Each registry's order decided it:
   - macOS: a `BTreeMap` keyed by `NSWindow` pointer;
   - Linux: a `HashMap` of X ids / `wl_surface` pointers;
   - Win32: a `BTreeMap` of `HWND`s.

## 1. Commits

| commit | what | tests |
|---|---|---|
| `ff02ca4d4` | **RED.** `dll/src/desktop/loop_wakeup_invariants.rs`: 7 source-text invariants (the loops are `#[cfg(target_os)]` code; the file follows `event.rs`'s `include_str!` precedent) | all 7 RED, see below |
| `b9a748b85` | `loop_waker` + D-Bus / X fds in the Linux wait sets; 100 ms cap removed | greens 4; +3 unit tests in `loop_waker.rs` |
| `b457f329a` | `app_target` rule, focus stamps, the `app_events` collector used by every loop | greens 2; +7 tests `layout/tests/app_target.rs` |
| `ea0ef30c9` | classify `app_target` in the two e2e manager-accounting gates | fixes a RED that `b457f329a` introduced (see below) |
| `64da53fad` | tray events delivered: `TrayIconData::callback`, `CallbackInfo::get_tray_event`, macOS button action, Linux callback-less picks | greens the last; +5 tests `layout/tests/tray_events.rs` |
| `3c59d79ea` | X11 hotkey wait-set queries do not dlopen libX11 before a grab exists | manual recipe in the commit |
| (this) | this report; the PROGRESS checkpoint file is deleted | - |

The checkpoint commits `28e0ae0c5`, `e7b57d183` and `86ff0cf62` only touch
`scripts/LOOP_WAKEUPS_TRAY_FIX_2026_09_28.PROGRESS.md`.

### Expected REDs, by commit

At `ff02ca4d4`, all seven fail. Each value is "today" vs "expected":

- **`no_linux_loop_caps_its_park_for_the_tray_notifications_or_hotkeys`**
  - Today, `x11/mod.rs` and `wayland/mod.rs` contain `has_tray || has_hotkeys`.
  - Neither calls `loop_waker::wait_fds()` or `must_not_park()`.
  - Green at `b9a748b85`.
- **`the_multi_window_linux_wait_includes_the_app_sources`**
  - Today, `wait_for_linux_window_activity` polls only the windows' connections.
  - Green at `b9a748b85`.
- **`every_d_bus_user_drains_the_shared_connection_completely`**
  - Today, `tray/linux.rs`, `notifications/linux.rs` and `gnome_menu/manager.rs` call `dbus_connection_read_write_dispatch)(`.
  - None of them calls `drain_connection(` or `loop_waker::watch_dbus_connection(`.
  - Green at `b9a748b85`.
- **`hotkey_backends_wake_the_loop_instead_of_being_polled`**
  - Today, there is no `XConnectionNumber`, and both backends set `needs_loop_polling: true`.
  - The portal's `push_fired` is not followed by `loop_waker::wake()`.
  - Green at `b9a748b85`.
- **`app_level_events_do_not_run_against_an_arbitrary_first_window`**
  - Today, `pump_into_first_*` and `pump_*_into_windows` exist.
  - The `app_events::deliver_to_*_windows()` calls are at 0/0/0; expected ≥3 macOS, ≥1 Win32, ≥1 Linux.
  - Green at `b457f329a`.
- **`the_manual_macos_loop_delivers_every_app_source_right_before_it_parks`**
  - Today, the tray pump is missing from the 2500 characters before `runMode_beforeDate`.
  - Green at `b457f329a`.
- **`plain_tray_clicks_reach_the_app`**
  - Today, it panics with "fn take_tray_deliveries not found". `run.rs` calls `tray::pump_tray()` by hand, and the macOS button has no `setAction`.
  - Green at `64da53fad`.

The six tests at `ff02ca4d4` are still RED at `b9a748b85`.

`b457f329a` also turned two existing guards RED. `every_manager_module_is_either_checked_or_declared_unobservable` and `..._fingerprinted_or_declared_not_fingerprintable` in `layout/src/e2e/full.rs` reported unclassified `["app_target"]`. `ea0ef30c9` fixes both. `64da53fad` classifies `tray_event` in the same commit that adds it.

**If you run one intermediate commit on its own:** `b457f329a` alone fails those two guards. Take `b457f329a` and `ea0ef30c9` together.

The unit tests for new APIs are green when they land; a RED version could not compile. This follows the ledger's ListView/TreeView precedent:
- `loop_waker` tests;
- `layout/tests/app_target.rs`;
- `layout/tests/tray_events.rs`.

### Where the tests are

- **dll:** `desktop::loop_wakeup_invariants::*` and `desktop::loop_waker::tests::*`.
  - `a_wake_makes_the_wake_descriptor_readable_until_served` is `#[cfg(az_x11)]`, so it runs on Linux, or on macOS with `x11-macos`.
- **layout:**
  - `cargo test --release -p azul-layout --test all -- app_target::`
  - `cargo test --release -p azul-layout --test all -- tray_events::`

## 2. Public API changes (api.json via azul-doc autofix - NOT hand-edited)

- **`TrayIconData.callback: OptionCoreMenuCallback`.** This is a new LAST field on a `#[repr(C)]` by-value struct.
  - **Critical, like `VideoConfig.paused`:** the C layout changes. Regenerate before building bindings. Any generated size assertion fails until the autofix runs.
- **`TrayIconData::with_callback(data: RefAny, callback: impl Into<CoreCallback>) -> TrayIconData`.**
- **`CallbackInfo::get_tray_event() -> Option<TrayEvent>`.** It mirrors `get_notification_event`.
- **Rust-only, not in api.json:**
  - `azul_layout::managers::{app_target, tray_event}`;
  - `CommonWindowState::app_order` and `note_focus_gained()`;
  - `dll::desktop::{loop_waker, app_events}`;
  - `tray::take_tray_deliveries`;
  - `global_hotkey::{loop_wait_fd, has_buffered_input}`;
  - `linux::dbus::{drain_connection, has_undispatched_messages, connection_fd, MAX_DISPATCH_PER_DRAIN, DBUS_DISPATCH_*}`.
- **Removed (dll-internal):**
  - `run.rs::{pump_tray_into_windows, pump_notifications_into_windows, invoke_tray_callbacks}`;
  - `global_hotkey::pump_into_first_{macos,win32,linux}_window`.
  - The dll-internal `TRAY_EVENTS` mailbox moved to layout.
- **Examples not updated:** `examples/rust/src/tray*.rs` do not call `with_callback`, because the generated binding name only exists after the autofix.

**Doc changes.** Several docs said "the first window", in `core/src/{notification,global_hotkey}.rs`, the layout managers and `CallbackInfo::register_global_hotkey`'s doc. They now state the new rule. The autofix will pick up doc drift.

## 3. What it does, per platform

**The shared pieces:**
- **`dll/src/desktop/loop_waker.rs`** is the one wake line. Its functions:
  - `wake()`: callable from any thread;
  - `wait_fds()`;
  - `must_not_park()`;
  - `service_sources()`;
  - `watch_dbus_connection()`.
- **`azul_layout::managers::app_target::pick_app_target`** is the one window rule:
  - pick the most recently focused window, else the oldest;
  - menus and tooltips count only when nothing else is open;
  - the result is independent of registry order.
  - Focus stamps come from a process-wide monotonic clock. They are set at `CommonWindowState::new` and by `note_focus_gained()` at the five focus-in writes: macOS `windowDidBecomeKey`, X11 `FocusIn`, Wayland `wl_keyboard.enter` plus its key-press focus inference, and Win32 `WM_SETFOCUS`.
- **`dll/src/desktop/app_events.rs`** is the one collector.
  1. `service_sources()`.
  2. `pump_tray()` (menu items with callbacks).
  3. `take_tray_deliveries()` (everything else, sent to `TrayIconData::callback` with `get_tray_event`).
  4. `pump_notifications()`.
  5. `take_fired()`.
  6. Everything runs through `invoke_menu_callback` against the picked window. The window gets `request_redraw` if anything asked for it.

| | Linux X11 | Linux Wayland | macOS | Windows |
|---|---|---|---|---|
| wait set | X conn + timerfds + ... + **wake fd, D-Bus session socket, X11 hotkey grab conn** | display + timerfds + ... + **wake fd, D-Bus socket** | NSRunLoop (unchanged) | `WaitMessage` (unchanged) |
| park cap | `-1` (16 ms only while threads run) | same | n/a | n/a |
| before parking | `must_not_park()` → return instead of parking | same | collector runs right before `runMode:beforeDate:` | collector runs in the same iteration as the thread-queue drain |
| tray | SNI/dbusmenu answered as bytes arrive; `Activate`, `SecondaryActivate`, `Scroll`, `ContextMenu` and callback-less picks go to the tray callback | same | menu-less icon click → `Activate` (button target/action/tag); callback-less picks → `MenuItem` | backend is still a stub, so the collector gets nothing |
| notifications | `ActionInvoked` / `NotificationClosed` dispatched from the socket | same | UN delegate → `loop_waker::wake()` (the NSEvent post moved here) | `NIN_BALLOON*` via the thread drain |
| hotkeys | grab conn fd in the set; `XPending` checked before parking | portal listener → `wake()` after `push_fired` / `report` | Carbon inside `sendEvent:` → collector before park | `WM_HOTKEY` via the thread drain |
| target window | `pick_app_target` over the registry | same | same | same |
| RunForever / tray-only | - | - | 33 ms `NSTimer` (unchanged cadence) → collector; `run_tray_only` → `deliver_to(headless stub)` | - |

**The D-Bus detail that made polling necessary.**
- `dbus_connection_read_write_dispatch` either dispatches ONE queued message or reads, never both.
- A message it read therefore sat parsed in libdbus's queue, where `poll(2)` on the socket cannot see it.
- The fix: `linux::dbus::drain_connection` reads once, then dispatches until `DBUS_DISPATCH_COMPLETE`.
  - It stops at 256 messages, then calls `wake()`.
- `must_not_park` also checks `dbus_connection_get_dispatch_status == DATA_REMAINS`. That status means `send_with_reply_and_block` parsed messages while it waited for its reply.
- **Connection lifetime:**
  - The waker takes its own `dbus_connection_ref`, so the GNOME exporter's unref-on-drop cannot leave it polling a finalized connection.
  - It drops a connection that reports itself closed, because POLLHUP would otherwise spin the loop.

## 4. Manual check recipes (the platform halves no test here can run)

**Linux, no more idle wake-ups.** Run any app with `App::set_tray` on KDE, or GNOME with AppIndicator.
```sh
strace -f -tt -e trace=poll,ppoll -p $(pidof <app>) 2>&1 | head -50
```
- Before this branch, an idle app showed `poll(..., 100)` returning 0 ten times a second.
- After, it shows a single `poll(..., -1)` blocking until you interact.
- Then post a notification from a callback and leave it on screen. It should still show one blocking poll, with no 100 ms timeouts.

**Linux, tray latency and plain clicks.**
1. Build a tray with `.with_callback(data, cb)`, where `cb` prints `info.get_tray_event()`.
2. Left-click the icon on KDE Plasma. It prints `Activate` immediately.
3. Middle-click prints `SecondaryActivate`.
4. Scroll over the icon prints `Scroll` with a delta.
5. Pick a menu item without a callback. It prints `MenuItem` with its dbusmenu id.
6. `dbus-monitor "interface='org.kde.StatusNotifierItem'"` shows `Activate` calls answered at once, with no ~100 ms gap before the reply.

**Linux, notification click.** Run the demo's notification button under dunst or GNOME, then click the banner. The callback runs immediately, with no park cap. Before, it took up to 100 ms.

**X11 hotkey.**
1. Register `Ctrl+Alt+K`, then leave the app idle, with no other events.
2. Run `xdotool key ctrl+alt+k` from another terminal. The callback fires at once.
3. `strace` shows the `poll` returning on the grab connection's fd, not on a timeout.

**Wayland hotkey (portal).** On KDE 5.27+ or GNOME 48+, bind a hotkey and accept the dialog. Press it while another app has focus. The callback fires at once, because the listener thread writes the wake eventfd.

**macOS, menu-less status item.**
1. Use `set_tray` with no menu and a callback.
2. Click the menu-bar icon. The callback runs with `Activate`, in the default `EndProcess` manual loop and under `RunForever`.
3. With a menu attached, the menu opens and no `Activate` arrives. That is AppKit's behaviour, and it is documented.

**macOS, tray pick delivered before the park.**
1. Pick a tray menu item, then do nothing else.
2. The callback must run without moving the mouse. Before, it waited for the next event if the pick was handled in the top-of-loop drain.
3. Same for a Carbon hotkey pressed while the app is in the background.

**Which window.**
1. Open a second window (for example the AzWidgets inspector), click into it, then press a registered hotkey or click the tray icon.
2. The callback runs against the second window, which you can check with `info.get_window_state()` or a title print.
3. Click back into the first window and repeat. It runs against the first.
4. With no focus history (hotkey pressed right after launch, no clicks), it runs against the oldest window.
5. On Linux the answer is now the same from run to run. Before, it was `HashMap` order.

**Windows hotkey.** Press a registered hotkey while another app is focused. The callback runs in the same loop iteration as the `WM_HOTKEY`, as before. The target now follows the focus rule instead of the lowest `HWND`.

## 5. Least sure to compile (read these first when the wave compiles)

1. **`dll/src/desktop/tray/macos.rs` `new()`: `button.setTarget(Some(&target))`.**
   - It relies on deref coercion from `&Retained<AzulMenuTarget>` to `&AnyObject`, the same pattern as `menu.rs:477`.
   - It also uses `button.setTag(..)`, `NSControl`'s safe setter, reached through `NSStatusBarButton → NSButton → NSControl`.
   - `menuItemAction:` receives the button typed as `Option<&NSMenuItem>`. At runtime that is fine, since `tag` exists on both classes. Check objc2's debug `verify` does not complain.
2. **`dll/src/desktop/loop_waker.rs` macOS arm.** It uses `AnyClass::get(c"NSEvent")`, a C-string literal, and a `msg_send!` returning `*mut AnyObject`. This is moved code from `notifications/macos.rs`, which was itself never compiled.
3. **`loop_waker.rs` `#[cfg(az_x11)] if dbus_watch::any_undispatched() { return true; }`.** This is a `cfg` on an `if` expression statement. The attribute should be stable on statements. If rustc complains, wrap the `if` in a `#[cfg(az_x11)] { ... }` block.
4. **`linux::dbus::dlopen.rs`.** It adds five required symbols: `dbus_connection_ref`, `_get_unix_fd`, `_read_write`, `_dispatch` and `_get_dispatch_status`. They exist in every libdbus-1, but if one is missing, the whole `DBusLib::new` fails, and with it the tray, notifications and GNOME menus.
5. **`dll/src/desktop/app_events.rs` Linux arm.**
   - `match unsafe { &*p } { LinuxWindow::X11(w) => &w.common, #[cfg(target_os="linux")] LinuxWindow::Wayland(w) => &w.common }`.
   - `events` is moved in either arm of the second match.
   - Under `x11-macos`, `LinuxWindow` has one variant.
6. **`layout/src/managers/app_target.rs`.** It uses the closure `let eligible = |c: &&AppTargetCandidate<K>| ...` twice, relying on it being `Copy`, including once inside `or_else`.
7. **`dll/src/desktop/tray/mod.rs`.** `use azul_layout::managers::tray_event::{self as events, TrayDelivery};` means the module name `events` shadows nothing. Check that no local called `events` was introduced later.
8. **Clippy (`#![deny(clippy::all)]` in `dll/src/desktop`).**
   - `loop_waker::watch_dbus_connection` is a `pub fn` taking `*mut DBusConnection`. It only forwards the pointer to a safe private fn, so `not_unsafe_ptr_arg_deref` should not fire. If it does, mark it `unsafe`.
   - For the cfg-split helpers, `global_hotkey::{loop_wait_fd, has_buffered_input}` are written as two cfg'd functions to avoid `let_and_return`.

## 6. Open items

- **Headless loop.** It still polls at 60 Hz while a simulated hotkey is registered, because presses come from a test thread with no handle to its condvar. The fix would register `HeadlessWindow::wake` as a `loop_waker` hook. Not done: the task named X11/Wayland.
- **Multi-window Linux wait.** It keeps its pre-existing 16 ms cap (timers of all windows). Only the app fds were added. Giving it the per-window timerfds would remove the cap.
- **macOS `RunForever` and `run_tray_only`.** They still drain on the 33 ms `NSTimer`, so a fire waits at most 33 ms, never for an unrelated event. `wake()` posts an NSEvent there too, but `NSApplication.run` discards it. A `CFRunLoopPerformBlock`-based waker could make it immediate.
- **macOS right-click on a menu-less status item** reports nothing. It needs `sendActionOn: LeftMouseUp|RightMouseUp` and a read of `NSApp.currentEvent.type` to map a right click to `ContextMenu`. Double-click is not a status-item concept on macOS or SNI.
- **Windows tray** is still the stub (`tray/windows.rs`). The collector already runs on Windows, so a future backend's `queue_tray_event` / `pump` is delivered with no loop changes.
- **Modal loops.**
  - Win32: a hotkey or balloon handled inside a modal loop (window move or size, `TrackPopupMenu`) is delivered only when that loop returns.
  - macOS: menu tracking in `NSEventTrackingRunLoopMode` defers the manual loop's collector until tracking ends. The `RunForever` timer runs in common modes and is unaffected.
- **Hidden-to-tray apps.** A window that is hidden stays eligible, which is deliberate: its callback may re-show it. When every window is CLOSED under `RunForever`, events are logged ("had no window to run against") and dropped. The design doc's Q9 "lazily created app stub" would fix that.
- **`TrayEvent.menu_command` semantics differ by platform.** macOS reports the global menu tag; Linux reports the dbusmenu flattened index. Both are pre-existing. Neither is an app-chosen id; that would need a `StringMenuItem` id field.
- **D-Bus main-loop integration** uses `dbus_connection_get_unix_fd` plus read / dispatch, not `DBusWatch` / `DBusTimeout`. That is fine for one authenticated socket. libdbus's own timeouts (pending-call timeouts) are not driven, but every call here is blocking (`send_with_reply_and_block`), so none are pending.
- **The global-hotkeys redesign** (`scripts/GLOBAL_HOTKEYS_DECLARATIVE_DESIGN_2026_09_28.md` §4.3 / §5):
  - Its "wakers" row is implemented here as `loop_waker`.
  - `pick_owner`'s window tie-break should call `app_target::pick_app_target` rather than keep its own stamps.
  - `CommonWindowState::app_order` is the `note_focus(seq)` hook it asks for.

## 7. Helpers other agents code against

- **`crate::desktop::loop_waker`:**
  - `wake()`, from any thread, AFTER the work is in its mailbox;
  - `is_pending()`;
  - `take_pending()`;
  - `service_sources()`;
  - `must_not_park()`;
  - `wait_fds()` [az_x11];
  - `watch_dbus_connection(&Arc<DBusLib>, *mut DBusConnection)` [az_x11].
- **`crate::desktop::shell2::linux::dbus`:** `drain_connection`, `has_undispatched_messages`, `connection_fd`, `MAX_DISPATCH_PER_DRAIN`.
- **`azul_layout::managers::app_target`:** `WindowActivationOrder::{for_new_window, note_focused, was_ever_focused}`, `AppTargetCandidate<K> { key, order, transient }`, `pick_app_target`. The dll side is `CommonWindowState::app_order` and `note_focus_gained()`.
- **`crate::desktop::app_events`:**
  - `AppEvents::{collect, invoke, is_empty, len}`;
  - `deliver_to(window)`;
  - `deliver_to_macos_windows()`;
  - `deliver_to_win32_windows()`;
  - `deliver_to_linux_windows()` [az_x11].
  - A new app-level source adds a field here and a check in `loop_waker::must_not_park`.
- **`azul_layout::managers::tray_event`:** `queue_tray_event`, `drain_tray_events`, `has_queued_tray_events`, `TrayDelivery`, `route_tray_events`, `with_current_tray_event`, `current_tray_event`, `MAX_QUEUED_TRAY_EVENTS`. On the dll side: `tray::take_tray_deliveries()`.
- **`crate::desktop::global_hotkey`:** `loop_wait_fd()`, `has_buffered_input()`. `deliver_fired(window)` remains for the headless loop.
