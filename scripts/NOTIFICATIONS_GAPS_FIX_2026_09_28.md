# Native notifications - the gaps, fixed - 2026-09-28

Branch `wt/notifications-gaps`, based on `5414bfa6b` (PR #476, `fix/input-bugs-2026-09-19`).
Source: `scripts/NOTIFICATIONS_RESEARCH_2026_09_28.md` (gaps G1-G9) on top of the first round
(`scripts/NATIVE_NOTIFICATIONS_2026_09_28.md`).

**Nothing here was compiled or run** (the rule for this wave: the parent compiles once). The
api.json sync (`azul-doc autofix`) is also still to do.

---

## 1. Commits

| commit | what | expected RED / how it is checked |
|---|---|---|
| `ac69f68be` | test: `mod gaps` appended to `layout/tests/native_notifications.rs` (already in `all.rs`; appended there so it shares that file's `serial()` lock over the process-global queues) | **Compile RED** of the `all` target: `Notification::with_payload`, `NotificationEvent::{payload, launched_app}`, `NotificationRegistry::set_app_handler`, the deliveries queue, the app-handler slot, `CallbackInfo::request_notification_permission` and the new `wire::*` do not exist. Behaviour pinned: see below |
| `632f16e4c` | core + layout: payload, launched_app, `AppConfig::notification_handler`, registry routing, deliveries queue, `reject_notification`, permission flag, wire | turns `ac69f68be` green |
| `274fc0f60` | dll service (`notifications/mod.rs`), `macos.rs` -> `apple.rs` (macOS + iOS), launch-time UN delegate, UN authorization read, payload in `userInfo`, `launched_app`, app handler install, deferral in the loops, iOS pump | platform code; recipes in section 3 |
| `763256639` | Android backend: `notifications/android.rs` + `scripts/android/AzulNotifications.java`, AzulActivity hooks, manifest, `android_main` pump, permission request | pure parts pinned by the `wire::android_*` tests; emulator recipe in section 3 |
| `5c2d8379a` | Windows: WinRT toast with an HKCU-registered AUMID, balloon fallback | pure parts pinned by the toast/AUMID wire tests; recipe in section 3 |
| `2c0ee12f8` | Linux: `desktop-entry` hint, `ActivationToken` -> Wayland `xdg_activation_v1.activate` | `wire::desktop_entry` test; recipe in section 3 |
| `a5e052252`, `1a860d517`, `5ba85590c`, `bf0e69f88` | progress checkpoints (the coordinator's rule) | - |
| (this commit) | this report; deletes the progress file; one stale log line in `apple.rs` | - |

The behavioural REDs inside `ac69f68be` (value today -> expected):

| test | assert | today | expected |
|---|---|---|---|
| `an_event_for_a_notification_this_process_never_posted_reaches_the_app_handler` | `route()` deliveries | 0 (dropped in `NotificationRegistry::route`) | 1, to the app handler, event unchanged (payload, `launched_app`) |
| `a_notification_without_its_own_callback_reports_to_the_app_handler` | deliveries | 0 | 1, payload filled from the post |
| `a_close_after_a_click_does_not_reach_the_app_handler_either` / `a_withdrawn_notification_does_not_reach_the_app_handler` | a trailing close | - | swallowed (ids that ENDED are remembered) |
| `the_payload_comes_back_in_the_event` / `a_payload_the_platform_reports_wins` | `event.payload` | no such field | the posted payload; the platform's wins |
| `a_post_to_a_full_queue_reports_failed_to_its_own_callback` | deliveries after posting into a full queue | 0 (`post_notification` discarded the `false`) | 1 `Failed` to the notification's callback, with payload and reason |
| `a_post_to_a_full_queue_without_a_callback_still_reports_failed` | mailbox | empty | 1 `Failed` (routed to the app handler) |
| `deliveries_wait_in_order_until_a_window_takes_them` | FIFO, bounded at `MAX_QUEUED_EVENTS` | no such queue (loops logged and dropped) | in order, bounded |
| `a_permission_request_is_queued_for_the_platform` | `has_queued_requests()` after `request_notification_permission` | no API | `true`, taken once |
| wire: `apple_authorization_statuses_become_permission_states`, `toast_arguments_carry_id_action_and_payload_both_ways`, `toast_xml_escapes_text_and_lists_the_buttons`, `a_toast_that_moves_to_the_action_center_has_not_ended`, `the_windows_aumid_is_safe_for_the_registry`, `the_desktop_entry_is_the_executable_name_like_the_wayland_app_id`, `android_request_codes_are_distinct_per_notification_and_leave_room_for_buttons`, `android_pending_intents_are_immutable`, `android_intents_become_events` | the pure vocabularies of the new backends | functions missing | pinned values |

The RED commit was amended once before the fix commit: the toast-XML test first compared the
`launch` attribute unescaped; the arguments contain `&`, which the attribute must carry as `&amp;`.

## 2. What changed, per gap

* **G2 (orphan events, payload).** `AppConfig::notification_handler` receives every event no
  live notification callback owns: a tap on a notification an earlier run posted (cold launch on
  iOS/Android, a relaunch from Notification Center on macOS) and the events of a notification
  posted without a callback. `App::run` / `run_tray_only` install it before any loop runs; the
  slot is process-wide because on Android `App::run` and `android_main` are different threads.
  The registry now remembers ENDED ids (routed or withdrawn, bounded 256) so the freedesktop close
  after a click and the close that confirms a withdraw stay swallowed even with a handler set.
  `Notification::payload` travels with the notification (UN `userInfo["azul.payload"]`, Android
  extras, toast arguments) and comes back as `NotificationEvent::payload`; freedesktop and the
  balloon get it from the registry. `NotificationEvent::launched_app`: Android launch intent, and
  macOS when the response is the one `NSApplicationLaunchUserNotificationKey` named.
* **G3 (permission, full queue).** `CallbackInfo::request_notification_permission()` sets a flag the
  dll's dispatch (capability pump and loop pump) runs on the main thread: macOS/iOS
  `requestAuthorizationWithOptions:` (unbundled: `Restricted`, UN untouched), Android the
  POST_NOTIFICATIONS dialog through the existing permission backend (API < 33: answered from
  `areNotificationsEnabled`), Windows from the toast notifier's setting, Linux Granted/Restricted
  from the server probe, headless Granted. Answers land in the permission manager as
  `Capability::Notifications` (`PermissionChanged`). A post that does not fit the request queue
  becomes a `Failed` event (own callback, else app handler).
* **G4 / macOS (task 1).** The UN delegate is installed by `install_launch_hooks()` right after the
  AppDelegate in `run.rs` (before the first window and `finishLaunching`) and in the tray-only run;
  on iOS from `did_finish_launching`. The authorization is READ from UN
  (`getNotificationSettingsWithCompletionHandler:`) at launch, on every activation
  (`applicationDidBecomeActive:`) and - throttled to 2 s - by the capability probe, so the probe
  says "unavailable ... turned off" once the user disabled notifications, also after a relaunch;
  `extra/permission/{macos,ios}.rs` answer Notifications from that reading. Unbundled stays
  capability-false and never touches UN.
* **G7 (no window).** The macOS pump, the Windows loop and the Linux loop park deliveries with
  `notifications::defer_deliveries` when they have no window (bounded, warned when full); the next
  pump that has one runs them first. Deferred deliveries do not count in `needs_polling()`.
* **G5 (Windows, task 3).** WinRT toasts: the AUMID `wire::windows_aumid("azul.<exe stem>")` is
  written under `HKCU\Software\Classes\AppUserModelId\<aumid>` (`DisplayName` = exe stem) through
  a dlopen'd advapi32; `CreateToastNotifierWithId`; content from `wire::toast_xml`; Tag = id (<= 64
  chars), Group `azul`; `Activated`/`Dismissed`/`Failed` queue events and post `WM_AZ_TOAST_WAKE`
  to the hidden window. A timed-out toast (it moved to the Action Center) ends nothing. A notifier
  setting other than `Enabled` fails the post - no balloon workaround around the user's choice.
  The balloon is the fallback when registration or the notifier fails.
* **G6 (Linux, task 4).** `desktop-entry` hint = the executable's name (Wayland `app_id` / X11
  `WM_CLASS` default); `ActivationToken` kept and, in the Linux loop's notification block, spent on
  `WaylandWindow::activate_with_token` (`xdg_activation_v1.activate`, newly bound). X11 drops it.
* **G1 (mobile, task 6).** iOS: the Apple file compiles for iOS (the NSEvent wake and the AppKit
  launch key are macOS-only), delegate at launch, `display_tick` pumps. Android: see section 3; the
  loop is woken from Java callbacks through an `AndroidAppWaker` (`wake_event_loop`), since
  `android_main` parks without a timeout in the background.
* **G9.** `doc/src/mobile/assets.rs` now embeds all 13 Java helpers (9 were missing); the manifest
  declares POST_NOTIFICATIONS, `launchMode="singleTop"` and the receiver; `AzulActivity` has
  `onNewIntent`.

**Java helper (task 6):** `scripts/android/AzulNotifications.java`, package `com.azul.notify`. It
lives where `build-android.sh` compiles Java from (every `scripts/android/*.java`, javac + d8),
and is embedded for a downloaded azul-doc via `doc/src/mobile/assets.rs`.

## 3. API changes (for `azul-doc autofix`)

Module routing is unchanged (`azul_core::notification::*` -> `notification`, from the first round).

### Changed types

| type | change | repr |
|---|---|---|
| `Notification` | new field `payload: String`, between `sound` and `callback` | C |
| `NotificationEvent` | new fields `payload: String`, `launched_app: bool` (at the end) | C |
| `AppConfig` (module `app`, `azul_core::resources`) | new field `notification_handler: OptionNotificationCallback` (at the end) | C |

### New functions

| class | fn | args -> return | fn_body |
|---|---|---|---|
| `Notification` | `with_payload` | `self, payload: String` -> `Notification` | `object.with_payload(payload)` |
| `AppConfig` | `set_notification_handler` | `&mut self, data: RefAny, callback: CallbackType` | `object.set_notification_handler(data, azul_layout::callbacks::Callback::create(callback).to_core())` (as `Notification::with_callback`) |
| `CallbackInfo` | `request_notification_permission` | `&mut self` | `object.request_notification_permission()` |

`OptionNotificationCallback` / `NotificationCallback` already exist in api.json from the first
round.

### Rust-only (not for api.json)

* `azul_layout::managers::notification`: `try_push_notification_request`, `reject_notification`,
  `queue_notification_delivery`, `drain_notification_deliveries`, `has_queued_deliveries`,
  `set_app_notification_handler`, `app_notification_handler`, `request_notification_permission`,
  `take_notification_permission_request`, `NotificationRegistry::set_app_handler`, and the `wire`
  additions (`APPLE_PAYLOAD_KEY`, `apple_authorization_status`, `TOAST_*`, `toast_arguments`,
  `parse_toast_arguments`, `toast_activated_event`, `toast_dismissed_event`, `toast_xml`,
  `windows_aumid`, `aumid_registry_key`, `desktop_entry`, `ANDROID_*`, `android_request_code`,
  `android_pending_intent_flags`, `android_event`).
* `azul_dll::desktop::notifications`: `set_app_handler`, `install_launch_hooks`,
  `note_launch_notification` (macOS), `refresh_permission`, `apple_permission_state`
  (macOS/iOS), `take_activation_token`, `defer_deliveries`; `pub mod android` (for its JNI symbol
  `Java_com_azul_notify_AzulNotifications_nativeOnNotificationEvent`).
* `azul_dll::desktop::extra::permission::android::request(Capability) -> bool` (factored out of
  `handle_event`); `shell2::android::wake_event_loop()`; `WaylandWindow::activate_with_token`
  (crate).

## 4. Manual checks

Never drive the real Mac's input while the user works (memory rule).

### macOS (bundled; build the `.app` as in the first round's §5, but install under `~/Applications`, sign inner dylibs first, no `--deep`)

1. **Launch tap (task 1).** Set an app-level handler in the demo (or any app):
   `config.set_notification_handler(data, on_orphan)` that shows `get_notification_event()`
   (kind, id, payload, launched_app). Post a notification with `.with_payload("x")`, quit the app
   (Cmd+Q), click the notification in Notification Center. The app launches and `on_orphan` runs
   with `Activated`, payload `x`, `launched_app = true`. Before this branch nothing arrived.
2. Same with the app running but the notification from an earlier run: `launched_app = false`,
   payload `x`, delivered to the handler.
3. **Permission truth.** System Settings > Notifications > the app > off; switch back to the app:
   "Platform support" reads *Unavailable - notifications are turned off*; relaunch: still
   *Unavailable* (the first round claimed "available" here). Turn it on again, activate the app:
   *Available*. `get_permission_status(Notifications)` follows (`PermissionChanged` fires).
4. `request_notification_permission()` from a button on a fresh bundle id (`tccutil reset
   UserNotification <id>`): the prompt appears without a post; Allow -> `Granted(Full)`.
5. Unbundled `target/release/AzWidgets`: no crash, capability false with the bundle reason;
   `request_notification_permission()` -> `Restricted`.

### iOS (simulator; no Xcode here - CI iOS job)

1. Post, background the app, tap the banner: the callback runs (display tick pump).
2. Kill the app, tap a delivered notification: the app launches and the app-level handler gets
   `Activated` with the payload (`launched_app` is false on iOS - see open items).
3. `request_notification_permission()`: the system prompt; the answer reaches
   `get_permission_status`.

### Android (`azul-doc mobile` on the headless emulator, API 34 image)

1. Build an APK (`bash scripts/build-android.sh ...`); `aapt dump xmltree` the manifest: POST_NOTIFICATIONS,
   `launchMode=singleTop`, the `AzulNotifications$Receiver`.
2. Post before granting: the event is `Failed` "... POST_NOTIFICATIONS ... being asked" and the
   permission dialog appears (or `adb shell pm grant <pkg> android.permission.POST_NOTIFICATIONS`).
   Post again: `adb shell dumpsys notification --noredact | grep -A5 <pkg>` shows the title, channel
   `azul.default`.
3. Tap it with the app in front: the notification's callback (via `onNewIntent`). Buttons: at most 3.
4. Cold start: `adb shell am force-stop <pkg>` is wrong here (it clears notifications) - post, then
   `adb shell am kill <pkg>` after backgrounding, tap the notification: the app-level handler gets
   `Activated`, `launched_app = true`, the payload. Equivalent without a tap:
   `adb shell am start -n <pkg>/com.azul.app.AzulActivity --es azul.notification.id X --es
   azul.notification.action default --es azul.notification.payload p`.
5. Swipe a notification away with the app running: `Dismissed` ("dismissed by the user"). With the
   process killed: the next start delivers it to the app-level handler (SharedPreferences queue).
6. `withdraw_notification(id)` removes it from the shade.

### Windows 10 / 11

1. Run `AzWidgets.exe` unpackaged. Log: `WinRT toast backend ready (AppUserModelID azul.AzWidgets)`;
   `reg query HKCU\Software\Classes\AppUserModelId\azul.AzWidgets` shows `DisplayName`.
   "Platform support": *Available - WinRT toast (AppUserModelID azul.AzWidgets)*.
2. Post: a toast attributed to "AzWidgets" with the "Show me" button (buttons are new). Click the
   body -> *Clicked*; click the button -> *Button pressed*; post two with different ids -> both
   show (the balloon replaced).
3. Let a toast time out: nothing reported; open the Action Center and click it -> *Clicked*
   (still delivered: a timeout does not end it). Close it with its X -> *Dismissed*.
4. `withdraw` -> gone from screen and Action Center.
5. Settings > System > Notifications > AzWidgets off: capability *Unavailable - turned off for
   this app*; a post -> *Not shown: ...* (no balloon workaround).
6. Force the fallback (e.g. deny HKCU write, or Windows 8.1): the balloon backend as in the first
   round, capability reason ends with "(WinRT toasts are unavailable: ...)".
7. After quitting, click an Action Center entry: nothing reaches the app (no COM activator, open).

### Linux

1. `dbus-monitor "interface='org.freedesktop.Notifications'"`: every `Notify` carries
   `"desktop-entry": <variant string "AzWidgets">`.
2. GNOME Wayland (`AZ_WINDOW=wayland`): put another window in front, click a notification: the
   monitor shows `ActivationToken` then `ActionInvoked`; the azul window comes to the front (log
   `[Wayland] Bound xdg_activation_v1` at startup). Before this branch it stayed behind.
3. X11: unchanged behaviour (token dropped).

### Everywhere

* Close the last window under a keep-running configuration, have a notification clicked, open a
  window again: the delivery runs then (log `event(s) wait for a window`).
* Post 257 notifications from one callback without letting the loop run: the 257th reports
  `Failed` ("... requests are already waiting ...").

## 5. Least sure to compile

1. **Android JNI** (`notifications/android.rs`): the generic `with_helper<R, F>` with
   `for<'a> FnOnce(&mut JNIEnv<'a>, &JObject<'a>, &JClass<'a>)` (copied from
   `media_keys/android.rs`, made generic over `R`); `new_object_array(len, "java/lang/String",
   JObject::null())`; `JValue::Object(&ids)` relying on `JObjectArray: Deref<Target = JObject>`;
   the JNI entry's `FnMut` closure that borrows `env` for `get_string`; `JValue::Bool(u8)`.
2. **Windows toast** (`notifications/windows.rs::toast`): the three `TypedEventHandler::new`
   closures (`Fn + Send + 'static`, capturing `String` + `isize`); `args.cast::<ToastActivatedEventArgs>()`
   on `&IInspectable`; `HSTRING::to_string_lossy`; matching `NotificationSetting` associated consts
   in patterns (it derives `PartialEq, Eq`); the non-`move` outer closure that borrows `self` and
   `id` in a `&mut self` method; the advapi32 fn-pointer types with `isize` HKEYs.
3. **Apple** (`apple.rs`): the new AppKit extern static `NSApplicationLaunchUserNotificationKey`
   (one deref: the static IS the `NSString *` variable); `objc2::sel!(notification)` into
   `respondsToSelector:`; `isKindOfClass:` with `&AnyClass`; `RcBlock::new(|settings: *mut AnyObject| ..)`;
   `iOS` compile of the file (objc2-foundation's `NSObject`/`NSObjectProtocol` on iOS, `NSPoint`
   gated to macOS, libloading on iOS through `_internal_deps`).
4. **AppDelegate** `applicationDidFinishLaunching:` added to a `MainThreadOnly` `define_class!`
   with an `Option<&AnyObject>` argument (same shape as `applicationDidBecomeActive:`).
5. **Wayland** `activate_with_token`: the variadic `wl_proxy_marshal` transmuted to a 4-argument
   fn with a `*const c_char` (the file's established pattern); the registry arm's cast.
6. **cfg matrix** in `notifications/mod.rs`: Android without `jni` falls to the "unsupported" arm;
   `UNSUPPORTED` is only defined there; `pub mod android` (the JNI symbol must not trip
   `unreachable_pub`).
7. **run.rs Linux block**: `deliveries` moved inside `&&` in one match arm and used in the `else if`
   of the outer `if let` - separate branches, but the borrow checker is the judge.
8. **layout**: `rsplit(|c: char| ..)`, `fnv1a64(&[b"..".as_slice(), ..])`, `(reason == X).then(..)`;
   the test module `mod gaps` reaches the parent's private helpers through `use super::*`.
9. **Java** (javac `-source 11` against android-34): `new Notification.Action.Builder((Icon) null, ..)`,
   the deprecated `Notification.Builder(Context)` (a warning), `org.json` in a static helper.

## 6. Open items

* **api.json + codegen** (section 3), then the widgets demo could show the app-level handler and
  the payload (it does not use them yet).
* **Windows COM activator** (`INotificationActivationCallback`, `CustomActivator` +
  `LocalServer32`): clicks on Action Center entries after the app exited. And
  `SetCurrentProcessExplicitAppUserModelID` was deliberately NOT called - it regroups taskbar
  buttons of apps the user pinned by path; a registry `IconUri` (PNG) is not written, so toasts
  show a generic icon.
* **`AppConfig::app_id`** (research step 1): the AUMID, the `desktop-entry` hint and the Android
  channel name derive from the executable / constants until the app can name itself once.
* **Channel / importance** (G8): one default + one silent channel on Android; no
  `NotificationImportance`, no `reply_text`, no schedules (research section 6).
* **iOS `launched_app`** stays false: UN gives iOS no launch marker comparable to
  `NSApplicationLaunchUserNotificationKey`.
* **macOS category race** (G4): `setNotificationCategories:` is still issued right before the add;
  a first notification of a new button set may show without buttons.
* **Android permission request thread**: `requestPermissions` is called from `android_main`'s
  thread (the existing backend's "works in theory" note); if a device refuses, it must be posted
  to the UI thread.
* **macOS bundling step** (`azul-doc bundle macos`) and the Linux portal backend / async `Notify` /
  `NameOwnerChanged` (G6 rest) are unchanged from the research list.
* **E2E**: no op injects a notification event with a payload; `assert_notification` has no
  `payload?` parameter yet.
* **Tray**: `drain_tray_events()` still has no caller (not touched; another agent owns the tray
  drain).
* Merge hot spots with the parallel agents: `dll/src/desktop/shell2/run.rs` (the three
  notification blocks, the macOS launch hook), `shell2/macos/mod.rs` (AppDelegate),
  `shell2/linux/wayland/{mod,events,defines}.rs` (xdg_activation), `dll/Cargo.toml` (windows
  features), `layout/tests/native_notifications.rs`.
