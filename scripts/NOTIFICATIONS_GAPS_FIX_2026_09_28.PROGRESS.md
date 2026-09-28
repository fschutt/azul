# Notification gaps fix - PROGRESS (checkpoint, deleted by the final commit)

Branch `wt/notifications-gaps` (from `5414bfa6b`, PR #476 `fix/input-bugs-2026-09-19`).
Source of truth: `scripts/NOTIFICATIONS_RESEARCH_2026_09_28.md` (G1-G9) and
`scripts/NATIVE_NOTIFICATIONS_2026_09_28.md`. Rules: NO cargo/rustc/LSP; RED test commit first
where testable; `git commit -F -`; stage explicit paths; do not touch
`layout/src/solver3/page_breaks.rs` / `layout/tests/a_padded_table_cell_stays_in_its_row.rs`;
no api.json edits; loop-side changes limited to the notification pump calls (another agent owns
the Linux/X11/Wayland park/timeout logic and a "which window gets app events" helper).
Final report goes to `scripts/NOTIFICATIONS_GAPS_FIX_2026_09_28.md`.

## DONE

* `ac69f68be` test(notifications): RED `mod gaps` appended to `layout/tests/native_notifications.rs`
  (already registered in all.rs) - app handler, payload, full queue, deliveries queue, permission
  request, wire (apple auth status, toast args/xml/dismissal/AUMID, desktop_entry, Android).
* `632f16e4c` feat(notifications): core `Notification::payload`/`with_payload`,
  `NotificationEvent::{payload, launched_app}`, `AppConfig::notification_handler` +
  `set_notification_handler`; layout registry (ended ids, app handler, payload fill), deliveries
  queue, `reject_notification`, `try_push_notification_request`, app-handler slot, permission flag,
  new `wire::*`; `CallbackInfo::{post_notification (full queue -> Failed),
  request_notification_permission}`.

* `274fc0f60` feat(notifications): dll service (mod.rs), macos.rs -> apple.rs (launch-time
  delegate, UN authorization read, payload/userInfo, launched_app), app.rs handler install,
  run.rs launch hook + `defer_deliveries` + Wayland token hand-off, AppDelegate hooks, iOS
  launch hook + display_tick pump, permission/{macos,ios}.rs. mod.rs names android/windows/linux
  functions that are NOT YET WRITTEN (see NEXT).

* `763256639` feat(notifications): Android backend (notifications/android.rs + AzulNotifications.java,
  AzulActivity hooks, manifest, build script, assets, permission/android.rs `request`, android_main
  pump + LOOP_WAKER). NEXT step 1 below is DONE.

## IN PROGRESS (written in the worktree, not yet committed)

* (committed in 274fc0f60, kept for reference:) dll service `dll/src/desktop/notifications/mod.rs` rewritten: backends `apple` (macOS+iOS),
  `android` (android+jni), `linux`, `windows`; `set_app_handler`, `install_launch_hooks`,
  `note_launch_notification` (macOS), `refresh_permission`, `apple_permission_state`,
  `take_activation_token`, `defer_deliveries`, permission dispatch in `dispatch_queued_requests`,
  pump returns waiting deliveries first.
* `notifications/macos.rs` -> `apple.rs` (git mv): delegate at launch, authorization read via
  `getNotificationSettingsWithCompletionHandler:` (launch, activation, throttled probe), payload in
  `userInfo["azul.payload"]`, `launched_app` via `NSApplicationLaunchUserNotificationKey`,
  `request_permission`.
* run.rs: `install_launch_hooks()` after the AppDelegate (macOS run + tray-only); no-window
  branches -> `defer_deliveries` (macOS pump fn, Windows loop, Linux loop); Linux loop hands
  `take_activation_token()` to `WaylandWindow::activate_with_token` (NOT YET WRITTEN).
* app.rs: `set_app_handler(config.notification_handler)` in `App::run` and `run_tray_only`.
* shell2/macos/mod.rs AppDelegate: `applicationDidFinishLaunching:` -> note_launch_notification;
  `applicationDidBecomeActive:` -> refresh_permission.
* iOS: `did_finish_launching` -> install_launch_hooks; `display_tick` pumps + invokes deliveries.
* permission/{macos,ios}.rs: Notifications -> `apple_permission_state()`.
* Android: `shell2/android/mod.rs` pump in android_main + `LOOP_WAKER`/`wake_event_loop()`;
  `scripts/android/AzulNotifications.java` (new: post/cancel/permissionState/onIntent/Receiver/
  SharedPreferences queue, native `nativeOnNotificationEvent(String,String,String,boolean)`);
  AzulActivity onCreate/onNewIntent hooks; manifest POST_NOTIFICATIONS + singleTop + receiver;
  build-android.sh skips a duplicate POST_NOTIFICATIONS; doc/src/mobile/assets.rs embeds all 13
  Java helpers; permission/android.rs `pub fn request(capability) -> bool`.

## NEXT (in order)

1. DONE in 763256639: `dll/src/desktop/notifications/android.rs`: `PlatformNotifier::{new, post,
   withdraw}`, `probe() -> (bool, String)`, `request_permission()`, JNI entry
   `Java_com_azul_notify_AzulNotifications_nativeOnNotificationEvent` -> `wire::android_event` ->
   queue + `shell2::android::wake_event_loop()`. Helper via `extra::find_app_class`
   ("com/azul/notify/AzulNotifications"), pattern of `extra/media_keys/android.rs::with_helper`.
   Java `post` signature: `(Landroid/app/Activity;Ljava/lang/String;Ljava/lang/String;ILjava/lang/String;Ljava/lang/String;[Ljava/lang/String;[Ljava/lang/String;Ljava/lang/String;ZI)Ljava/lang/String;`
   (activity, channelName, tag, requestCode, title, body, actionIds, actionLabels, payload,
   silent, pendingIntentFlags) -> "" or error. `permissionState(Activity)I` 0/1/2, `cancel(Activity,String)V`,
   `sdkInt()I`.
2. DONE (274fc0f60, 763256639).
3. Windows: `windows` crate features `UI_Notifications`, `Data_Xml_Dom` in dll/Cargo.toml; toast
   inside `notifications/windows.rs` `PlatformNotifier` (toast first, balloon fallback), HKCU
   `AppUserModelId\<aumid>` DisplayName via libloading advapi32 (RegCreateKeyExW/RegSetValueExW/
   RegCloseKey), `windows::probe() -> (bool, String backend, String reason)`,
   `windows::permission_state() -> PermissionState` (ToastNotifier::Setting), Activated/Dismissed/
   Failed handlers -> mailbox + PostMessageW wake to the hidden window.
4. Linux: `desktop-entry` hint (`wire::desktop_entry(current_exe)`), `ActivationToken` signal in
   the filter -> `take_activation_token()`; Wayland `xdg_activation_v1` interface in
   `wayland/defines.rs`, bind in `wayland/events.rs` registry handler, field + init + destroy +
   `pub(crate) fn activate_with_token(&mut self, token: &str) -> bool` (opcode 2 "so") in
   `wayland/mod.rs`.
5. Report `scripts/NOTIFICATIONS_GAPS_FIX_2026_09_28.md` (commits+REDs, API changes, manual
   checks per platform, least-sure-to-compile, open items); delete this file in that commit.

## Open questions

* Windows: `SetCurrentProcessExplicitAppUserModelID` deliberately NOT called (taskbar pin
  regrouping); no COM activator (clicks after exit) - open item.
* No `AppConfig::app_id` yet: AUMID / desktop-entry derive from the executable name.
* iOS `launched_app` stays false (UN gives no launch marker there).
