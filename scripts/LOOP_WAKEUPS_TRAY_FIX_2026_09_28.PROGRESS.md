# Loop wake-ups + tray clicks + app-event window rule - PROGRESS (checkpoint)

Branch `wt/loop-wakeups-tray` (base `5414bfa6b`, PR #476 branch `fix/input-bugs-2026-09-19`).
Rules: NO cargo / rustc / rust-analyzer; RED test commit first where testable; `git commit -F -`;
explicit staging; never touch `layout/src/solver3/page_breaks.rs` or
`layout/tests/a_padded_table_cell_stays_in_its_row.rs`; do not edit `api.json`.
This file is replaced by `scripts/LOOP_WAKEUPS_TRAY_FIX_2026_09_28.md` in the last commit.

## DONE

- `ff02ca4d4` RED: `dll/src/desktop/loop_wakeup_invariants.rs` (7 source-text tests, `#[cfg(test)]`
  from `desktop/mod.rs`). All 7 RED at that commit.
- `b9a748b85` fix(linux): `dll/src/desktop/loop_waker.rs` (wait fds, cross-thread wake, must_not_park,
  service_sources, D-Bus watch); libdbus symbols `dbus_connection_ref/_get_unix_fd/_read_write/
  _dispatch/_get_dispatch_status` + `linux::dbus::drain_connection / has_undispatched_messages /
  connection_fd`; X11 + Wayland + multi-window waits poll the app fds, 100 ms cap removed; X11
  hotkey grab fd (XConnectionNumber); portal listener wakes; tray / notifications / GNOME menu
  drain fully + register the connection; macOS notification NSEvent wake moved into loop_waker.
  Greens 4 of the 7 RED tests.
- `b457f329a` fix(desktop): `layout/src/managers/app_target.rs` (the rule) + `layout/tests/
  app_target.rs` (7 tests); `CommonWindowState::app_order` + `note_focus_gained()` at the 5
  focus-in writes; `dll/src/desktop/app_events.rs` collector; every loop (macOS RunForever timer,
  manual top + before park, Win32, Linux, run_tray_only) delivers through it; old
  `pump_*_into_windows`, `invoke_tray_callbacks`, `global_hotkey::pump_into_first_*` removed.
  Greens 2 more RED tests.

## IN PROGRESS (uncommitted in the worktree)

Tray events delivered (greens the last RED, `plain_tray_clicks_reach_the_app`):
- `core/src/tray.rs`: `TrayIconData::callback: OptionCoreMenuCallback` (LAST field) +
  `with_callback(data, callback)`.
- `layout/src/managers/tray_event.rs` (new): mailbox `queue_tray_event / drain_tray_events /
  has_queued_tray_events`, `TrayDelivery`, `route_tray_events`, `with_current_tray_event`,
  `current_tray_event`; registered in `managers/mod.rs`.
- `layout/src/callbacks.rs`: `CallbackInfo::get_tray_event() -> Option<TrayEvent>`.
- `dll/src/desktop/tray/mod.rs`: mailbox forwards to layout; new `take_tray_deliveries()`.
- `dll/src/desktop/tray/macos.rs`: status item button target/action/tag -> `Activate`
  (`alloc_menu_tags` made `pub(crate)` in `shell2/macos/menu.rs`).
- `dll/src/desktop/tray/linux.rs`: callback-less dbusmenu picks queue `MenuItem` events.
- `dll/src/desktop/app_events.rs`: `tray: Vec<TrayDelivery>` collected + invoked with the event
  installed; `loop_waker::must_not_park` also checks `has_queued_tray_events()`.
- `layout/tests/tray_events.rs` (5 tests) registered in `layout/tests/all.rs`.

## NEXT (in order)

1. Commit the tray-events fix (the RED can't be a pure unit test: the API is new; the source RED
   `plain_tray_clicks_reach_the_app` from `ff02ca4d4` is the red half).
2. Re-read all changed Linux code once more for compile risks (cfg gates, borrows, unsafe).
3. Write `scripts/LOOP_WAKEUPS_TRAY_FIX_2026_09_28.md` (commits + expected REDs, per-platform
   behaviour, manual check recipes, least-sure-to-compile spots, open items, public API changes);
   delete this PROGRESS file in the same commit.

## Helpers other agents code against

- `crate::desktop::loop_waker` (dll): `wake()` (any thread), `is_pending()`, `take_pending()`,
  `service_sources()`, `must_not_park()`, `wait_fds()` [az_x11], `watch_dbus_connection(&Arc<DBusLib>,
  *mut DBusConnection)` [az_x11].
- `crate::desktop::shell2::linux::dbus::{drain_connection, has_undispatched_messages,
  connection_fd, MAX_DISPATCH_PER_DRAIN}`.
- `azul_layout::managers::app_target::{WindowActivationOrder, AppTargetCandidate,
  pick_app_target}`; dll `CommonWindowState::app_order` + `note_focus_gained()`.
- `crate::desktop::app_events` (dll): `AppEvents::{collect, invoke, is_empty, len}`,
  `deliver_to(window)`, `deliver_to_macos_windows()`, `deliver_to_win32_windows()`,
  `deliver_to_linux_windows()` [az_x11].
- `crate::desktop::global_hotkey::{loop_wait_fd, has_buffered_input}`.
- (in progress) `azul_layout::managers::tray_event::*`, `crate::desktop::tray::take_tray_deliveries`.

## Open questions

- Headless loop still polls at 60 Hz while a simulated hotkey is registered (its presses come
  from a test thread with no handle to the condvar); not in this task's scope.
- Multi-window Linux wait keeps its 16 ms cap (timers of all windows); only the app fds were added.
- macOS right-click on a menu-less status item is still not reported (needs `sendActionOn:` +
  reading `NSApp.currentEvent`); Windows tray backend is still a stub.
- Public API (needs azul-doc autofix, do not hand-edit api.json): `TrayIconData.callback`
  (by-value C ABI change, new last field), `TrayIconData::with_callback`,
  `CallbackInfo::get_tray_event`.
