# F2 - notification follow-ups - PROGRESS

Branch `wt/f2-notifications`, from `282890483` (local tip of `fix/input-bugs-2026-09-19`, PR #476).
User instruction: "fix all follow ups recorded so far". Nothing here is compiled (the parent compiles).

## 1. AUDIT (the ledger's open list vs the code at 282890483)

The ledger list predates the wave-1 agent `notifications-gaps` (G1-G9). That agent's commits ARE in
this base: `26c4fad24` (RED), `fc43076e5` (core + layout), `817e98e07` (service, Apple, iOS),
`3bce47136` (Android), `1bc3a1882` (Windows toasts), `8a75ecaff` (Linux), api.json synced in
`4b1786f93`. The integration then moved every loop's notification pump onto the app-event
collector (`dll/src/desktop/app_events.rs`).

| # | item | status | where / what is missing |
|---|---|---|---|
| 1 | macOS: unbundled apps report Unavailable; `azul-doc bundle macos` | **STILL OPEN** | no `bundle` command in `doc/src/main.rs`; nothing in the repo produces a `.app` for a desktop example. The unbundled reason (`dll/src/desktop/notifications/apple.rs:248`) does not name a command either |
| 2 | macOS: UN delegate before the run loop | **DONE** `817e98e07` | `shell2/run.rs:960` (after the AppDelegate, before the first window and `finishLaunching` at :1106 / `app.run()` at :1223), `run.rs:2926` (tray-only), `notifications/apple.rs:386` `install_launch_hooks`; iOS `shell2/ios/mod.rs:1392`; launch marker `macos/mod.rs:2927-2940` -> `apple.rs:423` |
| 3 | macOS: permission not persisted / probe says available after disabling | **DONE** `817e98e07` | `apple.rs:269` probe reads `AUTH_STATUS` from `getNotificationSettingsWithCompletionHandler:` (`apple.rs:353`), refreshed at launch, on `applicationDidBecomeActive:` (`macos/mod.rs:2921`) and by the probe (2 s throttle); `extra/permission/macos.rs:109`, `ios.rs:54`. Residual: the very first probe before the launch reading lands says "available (not read yet)" - by design, not fixed |
| 4 | Windows: registered WinRT toasts instead of NIF_INFO | **DONE** `1bc3a1882` | `notifications/windows.rs:537-851` (`mod toast`: HKCU AUMID, `CreateToastNotifierWithId`, Activated/Dismissed/Failed), balloon only as fallback; `dll/Cargo.toml:472-473` (`UI_Notifications`, `Data_Xml_Dom`). Not in the item, still open: COM activator for a click after the app exited |
| 5 | Linux: `desktop-entry` hint + Wayland activation token | **PARTIAL** | hint DONE (`notifications/linux.rs:515,573`, `8a75ecaff`); the token is CAPTURED (`linux.rs:388`) and `WaylandWindow::activate_with_token` exists (`wayland/mod.rs:10559`), but **nothing calls `notifications::take_activation_token()`**: the run.rs notification block that spent it was replaced by `app_events::deliver_to_linux_windows` (`app_events.rs:210-250`) at integration. A click still cannot raise the Wayland window |
| 6 | Mobile: iOS / Android backend + pump, Android permission request, `onNewIntent` | **DONE** `3bce47136`, `817e98e07` | Android `notifications/android.rs` (JNI), `scripts/android/AzulNotifications.java`, `AzulActivity.java:144` `onNewIntent`, manifest POST_NOTIFICATIONS / singleTop / receiver, pump `shell2/android/mod.rs:813`, permission `android.rs:139`; iOS = `apple.rs` (compiles for iOS), pump `shell2/ios/mod.rs:1068`, launch hook `ios/mod.rs:1392`. Residuals (open, see report): iOS `launched_app` always false; Android `requestPermissions` from `android_main`'s thread |
| 7 | Events for notifications this process did not post are dropped; app handler + payload | **DONE** `fc43076e5` (+ api.json `4b1786f93`) | `layout/src/managers/notification.rs:370` `route` -> app handler; `AppConfig::notification_handler` (`core/src/resources.rs:1010`, installed `dll/src/desktop/app.rs:316,371`); `Notification::payload`, `NotificationEvent::{payload, launched_app}`. **Residual bug found**: a post the full request queue rejects WITHOUT a callback goes back through the MAILBOX (`managers/notification.rs:247`), so routing (a) hands its `Failed` to a LIVE notification under the same id - ending that one, which is still on screen - or (b) swallows it when the id ended earlier |
| 8 | Posts dropped with no window (win/linux); full queue drops silently | **DONE** (posts) `fc43076e5`, `817e98e07` | full queue -> `Failed` (`layout/src/callbacks.rs:5523-5539`, `reject_notification`); no window -> `app_events.rs:147` `report_undelivered` parks them (`notifications/mod.rs:465` `defer_deliveries`). **Residuals found**: a WITHDRAW that meets a full queue is dropped silently (`callbacks.rs:5557`), so the notification stays up and its callback still fires; a backend failure's `Failed` event is dropped without a word when the mailbox is full (`notifications/mod.rs:273`) |

AzClock alarms / scheduled notifications: NOT IN SCOPE (separate feature).

## 2. Plan (priority from the brief)

1. 7/8 residuals (engine, headless tests in `layout/tests/native_notifications.rs`): rejected
   callback-less post goes straight to the app handler, never through routing; withdraws get
   headroom posts cannot take.
2. 5: spend the activation token in the Linux collector (source-text invariant test in
   `dll/src/desktop/loop_wakeup_invariants.rs`, the house pattern for cfg'd loop code).
3. 1: `azul-doc bundle macos` (new `doc/src/bundle.rs`: Info.plist, `.app` layout, dylibs,
   ad-hoc codesign inner-first, `lsregister`, default `~/Applications`), unit tests in the module;
   the unbundled reason names the command.
4. 6 residuals: documented, not implemented (see report).

## 3. DONE (commits)

- `36456d402` audit (this file)
- `3c5fcd9dd` RED: rejected post reports to its owner (not via routing); withdraw fits a full queue
  (`layout/tests/native_notifications.rs`, `gaps::follow_ups` + the updated
  `a_post_to_a_full_queue_without_a_callback_still_reports_failed`)
- `1b9d2f601` fix: `reject_notification` -> own callback / app handler as a waiting delivery;
  withdraws get 2 x MAX room; dll logs a Failed the full mailbox refused

- `89d51e923` RED (item 5): `loop_wakeup_invariants::a_notification_click_raises_the_wayland_window_with_its_activation_token`

- `0d5f2c94b` fix (item 5): `app_events::deliver_to_linux_windows` takes the token after
  `collect()`, spends it on a Wayland target before the callbacks

- `a33e1365d` RED (item 1): `doc/src/bundle.rs` pure half stubbed + 11 unit tests, `pub mod bundle` in main.rs

- `9dd190762` feat (item 1): `azul-doc bundle macos` (pure half + command + dispatch/help); the
  unbundled reason names it
- `5ce7e18b6` fix (G4 residual): a new button set's first notification waits for its category
  (`getNotificationCategoriesWithCompletionHandler:` before the add)

- `9c0fad302` report `scripts/F2_NOTIFICATIONS_2026_09_29.md`

## 4. IN PROGRESS

- none (DONE): final report `scripts/F2_NOTIFICATIONS_2026_09_29.md`. iOS `launched_app`, the Android
  `requestPermissions` thread, the Windows COM activator, `AppConfig::app_id` and the E2E
  notification op are PLANS in the report, not code.

## 5. Open questions

- none
