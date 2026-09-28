# Native notifications - 2026-09-28

Branch `wt/native-notifications`, based on `7e020dc49` (PR #476, `fix/input-bugs-2026-09-19`).

User request: *"toast should be removed from the demo, therefor we need to integrate native
notification handling (same as we did system tray)"*.

**Nothing in this branch has been compiled or run** (instruction: the parent compiles once at the
end). The api.json sync (`azul-doc autofix`) is also still to do; the demo builds against the
generated bindings, so it cannot build before that.

---

## 1. Design, and how it mirrors the tray

The tray is split in three: a data model in `azul_core::tray`, the OS plumbing in
`azul-dll/src/desktop/tray/{mod,macos,linux,windows}.rs`, and a process-wide mailbox that the OS
callbacks write and the run loop drains, with menu callbacks invoked against the first window through
`invoke_menu_callback` (`pump_tray` / `pump_tray_into_windows`). Notifications use the same split
and the same delivery path:

| | tray | notifications |
|---|---|---|
| data model | `azul_core::tray` | `azul_core::notification` |
| queues and routing | `desktop/tray/mod.rs` (`queue_tray_event`) | `azul_layout::managers::notification` (request queue, event mailbox, `NotificationRegistry`, current-event slot, headless recorder, `wire`) |
| OS plumbing | `desktop/tray/{macos,linux,windows}.rs` | `desktop/notifications/{macos,linux,windows}.rs` |
| live object | thread-local `LIVE_TRAY` | thread-local `SERVICE` (backend + registry), started by the first post |
| pumped from | `pump_tray()` in the macOS manual loop, the RunForever timer, the tray-only timer, the Linux loop | `pump_notifications()` in the same places, plus the Windows loop, the headless loop, and before the macOS manual loop parks |
| delivery | `invoke_menu_callback(cb, MenuInvocation::Native{..})` against the first window | the same call (`notifications::invoke_deliveries`), with the event installed for `CallbackInfo::get_notification_event` |
| Linux poll bound | `has_live_tray()` → 100 ms | `\|\| notifications::needs_polling()` on the same line |
| availability | `TrayIcon::is_available()` / `App::is_tray_available` | `PlatformCapability::notifications()` |

Where it differs, and why:

* **Requests go through a queue in azul-layout, not a direct call.** An app posts from a callback
  (`CallbackInfo::post_notification`), and azul-layout cannot call an OS API - the keyring's shape.
  The dll drains the queue on the main thread: from the capability pump at the top of every event
  pass (all targets, mobile included) and from `pump_notifications()`. The capability pump also arms
  its wake-up timer while a request is queued, so a post from a `DoNothing` callback does not wait for
  an unrelated event.
* **The callback belongs to the notification** (`Notification::with_callback(data, cb)`, the
  RefAny + `CoreCallback` pair a menu item carries), not to a DOM node. A notification outlives the
  DOM that posted it, and a tray menu item is the precedent.
* **Every event ends the notification.** `NotificationRegistry::route` forgets the callback as it
  routes. That is what keeps one click from arriving twice: a freedesktop server sends
  `ActionInvoked` and then `NotificationClosed` for the same click; Windows sends a hide after a
  click; a withdraw is confirmed by a close.
* **Failures are events.** A backend that cannot start (unbundled macOS, no freedesktop server,
  mobile) turns each post into a `Failed` event with the reason, delivered to the notification's own
  callback, and logs it (`plog_warn`). `PlatformCapability::notifications()` gives the same reason up
  front.
* **`wire`**: each platform's vocabulary (freedesktop action keys and close reasons, UN action
  identifiers and category ids, balloon messages, balloon UTF-16 truncation) is translated in
  azul-layout, where every host compiles and tests it - the `#[cfg(target_os)]` backends that call it
  compile on one OS each (the house rule stated in `shell2/common/event.rs`).

Found while mirroring the tray: **`drain_tray_events()` has no caller**. Tray menu items WITH a
callback work (they go through `pump_tray` → `invoke_menu_callback`), but a bare tray
`Activate` / `SecondaryActivate` / `ContextMenu` / `Scroll` / callback-less `MenuItem` event is queued
and never reaches the app. Not touched here.

---

## 2. Commits

| commit | what | expected RED |
|---|---|---|
| `a40327d51` test(notifications): the platform-independent half of native notifications | `layout/tests/native_notifications.rs` (+ `all.rs` registration): queue/dispatch, drain → callback delivery, headless recorder, wire translations - 25 tests | the `all` integration target does not compile: `azul_core::notification`, `azul_layout::managers::notification` and `CallbackInfo::{post_notification, withdraw_notification, get_notification_event}` do not exist |
| `cfb48530b` feat(notifications): the model, the queues and the routing | `core/src/notification.rs`, `layout/src/managers/notification.rs`, the three `CallbackInfo` methods, e2e classification of the new manager module + `assert_notification`, autofix routing to a `notification` module | turns the above green |
| `b9ac9322a` feat(notifications): native backends on macOS, Linux and Windows | `dll/src/desktop/notifications/{mod,macos,linux,windows}.rs`, two libdbus symbols, `PlatformCapability::notifications()` (+ wasm stub), capability-pump dispatch and timer, the run-loop / headless / tray-only pumps, the X11/Wayland poll bound, Windows shutdown | platform code - no unit test possible here; recipes in §5 |
| `b13aedc03` feat(examples): the widgets demo posts a native notification instead of a Toast | `examples/azul-widgets/src/notifications.rs`, three hooks in `lib.rs`, Toast entry + `on_toast_dismiss` removed, `Cargo.toml` description, a theme test for the new module | - |
| (this file) chore(scripts): native notifications report | | |

The RED commit is a compile RED: the whole API is new. The test file was amended once before the
feature commit (the Apple-dismiss assertion now checks `kind` + id, since a dismissal carries a
reason), so what is in `a40327d51` is exactly what `cfb48530b` makes green.

Demo tests: `layout/tests/azul_widgets_demo_follows_the_theme.rs` is the only test that reads the
demo. It locates the page frame by the literals `"Azul Widget Showcase"` / `"custom titlebar"` in
`lib.rs` (unaffected: the Toast removal and the new hooks add no literal there) and scanned only
`lib.rs`; a new test, `the_notifications_section_paints_from_the_system_palette_too`, holds
`notifications.rs` to the same `system:`-palette rule. Nothing referenced the Toast
(`scripts/preflight_contracts.py` lists `toast.rs` only as widget-library debt, which stays).

---

## 3. Public API additions (for `azul-doc autofix`)

`doc/src/autofix/module_map.rs` now routes `azul_core::notification::*` to a new API module
**`notification`** (MODULES entry after `tray` + a path arm next to the tray's). Structural types
(`Option*`, `*Vec*`) still go to `option` / `vec` as usual.

### New types - `azul_core::notification` → module `notification`

| type | kind | fields / variants | repr |
|---|---|---|---|
| `NotificationAction` | struct | `id: String`, `label: String` | C |
| `OptionNotificationAction` | option | `None`, `Some(NotificationAction)` | C, u8 |
| `NotificationActionVec` (+ `NotificationActionVecDestructor`, `NotificationActionVecDestructorType`, `NotificationActionVecSlice`) | vec | of `NotificationAction` | C |
| `NotificationSound` | enum | `Default`, `Silent`, `Named(String)` | C, u8 |
| `NotificationCallback` | struct | `refany: RefAny`, `callback: CoreCallback` | C |
| `OptionNotificationCallback` | option | | C, u8 |
| `Notification` | struct | `id: String`, `title: String`, `body: String`, `icon: OptionString`, `actions: NotificationActionVec`, `sound: NotificationSound`, `callback: OptionNotificationCallback` | C |
| `NotificationEventType` | enum | `Activated`, `ActionInvoked`, `Dismissed`, `Failed` | C |
| `NotificationEvent` | struct | `kind: NotificationEventType`, `notification_id: String`, `action_id: String`, `reason: String` | C |
| `OptionNotificationEvent` | option | | C, u8 |

Derives: `NotificationAction`, `NotificationSound`, `NotificationCallback`, `NotificationEvent` and
their options: Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash. `NotificationEventType`: those plus
Copy. `Notification`: Debug, Clone, PartialEq.

### New functions

| class | fn | args → return | fn_body |
|---|---|---|---|
| `NotificationAction` | constructor `create` | `id: String, label: String` | `azul_core::notification::NotificationAction::create(id, label)` |
| `Notification` | constructor `create` | `id: String, title: String` | `azul_core::notification::Notification::create(id, title)` |
| `Notification` | `with_body` | `self, body: String` → `Notification` | `object.with_body(body)` |
| `Notification` | `with_icon` | `self, path: String` → `Notification` | `object.with_icon(path)` |
| `Notification` | `with_action` | `self, id: String, label: String` → `Notification` | `object.with_action(id, label)` |
| `Notification` | `with_actions` | `self, actions: NotificationActionVec` → `Notification` | `object.with_actions(actions)` |
| `Notification` | `with_sound` | `self, sound: NotificationSound` → `Notification` | `object.with_sound(sound)` |
| `Notification` | `with_callback` | `self, data: RefAny, callback: CallbackType` → `Notification` | `object.with_callback(data, azul_layout::callbacks::Callback::create(callback).to_core())` (as `StringMenuItem::with_callback`) |
| `NotificationEvent` | constructors `activated`, `dismissed` | `notification_id: String` | `azul_core::notification::NotificationEvent::activated(notification_id)` etc. |
| `NotificationEvent` | constructors `action_invoked` | `notification_id: String, action_id: String` | |
| `NotificationEvent` | constructors `dismissed_because`, `failed` | `notification_id: String, reason: String` | |
| `CallbackInfo` | `post_notification` | `&mut self, notification: Notification` | `object.post_notification(notification)` |
| `CallbackInfo` | `withdraw_notification` | `&mut self, id: String` | `object.withdraw_notification(id)` |
| `CallbackInfo` | `get_notification_event` | `&self` → `OptionNotificationEvent` | `object.get_notification_event().into()` |
| `PlatformCapability` (module `window`) | constructor `notifications` | none | `azul_dll::unified::capability::PlatformCapability::notifications()` |

### Rust-only (not for api.json)

* `azul_layout::managers::notification`: `NotificationRequest`, `NotificationRegistry`,
  `NotificationDelivery`, `NotificationRecorder`, `RecordedNotification`, `MAX_QUEUED_REQUESTS`,
  `MAX_QUEUED_EVENTS`, `push_notification_request`, `drain_notification_requests`,
  `has_queued_requests`, `queue_notification_event`, `drain_notification_events`,
  `has_queued_events`, `with_current_notification_event`, `current_notification_event`,
  `record_posted_notification`, `record_withdrawn_notification`, `recorded_notifications`,
  `clear_recorded_notifications`, `wire::*`.
* `azul_dll::desktop::notifications`: `probe`, `pump_notifications`, `dispatch_queued_requests`,
  `needs_polling`, `shutdown`, `use_headless_backend`, `invoke_deliveries` (crate).
* `DBusLib::{dbus_bus_add_match, dbus_connection_add_filter}` + `type DBusHandleMessageFunction`.
* E2E assertion `assert_notification` (`id?`, `title?`, `body?`, `action?`, `withdrawn?`, `count?`)
  in `layout/src/e2e/full.rs`; `gen-e2e` discovers it from the dispatch arm.

---

## 4. Per-platform behaviour

| | backend | available when | buttons | events | failure |
|---|---|---|---|---|---|
| macOS | `UNUserNotificationCenter` (framework dlopen'd, `msg_send`) | the process is a `.app` with `CFBundleIdentifier`, and the user has not turned notifications off | yes, a `UNNotificationCategory` per button set | click, button, dismissal (clearing from Notification Center; custom-dismiss category option) | unbundled / no framework / permission denied / `addNotificationRequest` error → `Failed` |
| Linux X11 + Wayland | `org.freedesktop.Notifications` on the tray's shared libdbus session connection | a server answers `GetServerInformation` (probe cached 10 s) | if the server advertises `actions` (probe reason says so otherwise) | click (`default` action), button, dismissal with reason (expired / by user / by call) | no server / `Notify` error → `Failed` |
| Windows | `Shell_NotifyIconW` `NIF_INFO` balloon on a hidden top-level window; Win10/11 show it as a toast attributed to the exe | shell32/user32 load | **no** (dropped, warned once) | click, timeout/closed, hidden | `NIM_ADD` / `NIM_MODIFY` failure → `Failed`; a replaced balloon → `Dismissed` |
| headless (`AZ_BACKEND=headless`) | the layout recorder | never (capability false) | recorded | none from the OS (a test may queue events) | - |
| iOS, Android | none | never | - | - | every post → logged `Failed` (not delivered: no loop pumps on mobile) |
| web | none | never (wasm stub) | - | - | requests stay in the bounded queue |

### The unbundled-macOS decision

`+[UNUserNotificationCenter currentNotificationCenter]` in a process without a bundle record raises
`NSInternalInconsistencyException` ("bundleProxyForCurrentProcess is nil"); unwinding into Rust aborts
the process. AzWidgets runs from `target/release`, i.e. unbundled. So:

* `bundle_status()` checks `NSBundle.mainBundle` - path ends in `.app` AND `bundleIdentifier` is
  non-empty - and UN is **never touched** otherwise (not by the probe, not by the backend).
* Unbundled: `PlatformCapability::notifications()` → `available: false`, backend
  `UNUserNotificationCenter`, reason *"this process does not run from a .app bundle (its main bundle
  is "…/target/release"): UNUserNotificationCenter needs an app bundle whose Info.plist sets
  CFBundleIdentifier …"*. The service logs it once at warn; every post becomes a `Failed` event with
  the same reason, which the demo shows as "Not shown: …".
* **No fallback, on purpose:** `NSUserNotificationCenter` (deprecated) is nil without a bundle id
  too; swizzling `-[NSBundle bundleIdentifier]` (what some crates do) impersonates another app;
  `osascript display notification` works unbundled but is attributed to Script Editor, has no buttons
  and reports nothing back - a backend that looks alive while every callback stays silent.
* To see notifications on macOS, run from a bundle (recipe below). A proper `.app` bundling step for
  the examples is open (§7).

---

## 5. Manual checks

Never drive the real Mac's input while the user works (memory rule) - these are for a person, or for
the parent to schedule.

### macOS, unbundled (the default run)

1. `cargo build --release -p AzWidgets`, run `target/release/AzWidgets`.
2. "Notifications" → "Platform support": *Unavailable - UNUserNotificationCenter: this process does
   not run from a .app bundle …*. Log: `[notifications] no notification backend: …` once the first
   post starts the service.
3. "Post a notification" → "Last event": *Not shown: this process does not run from a .app bundle …*.
   No crash (the crash is what the bundle check exists for).

### macOS, bundled

```sh
APP=/tmp/AzWidgets.app
rm -rf "$APP"; mkdir -p "$APP/Contents/MacOS"
cp target/release/AzWidgets "$APP/Contents/MacOS/"
# link-dynamic: the dylib must be where the binary's install name / rpath looks
otool -L target/release/AzWidgets | grep -i azul   # then copy libazul*.dylib accordingly
cp target/release/libazul*.dylib "$APP/Contents/MacOS/" 2>/dev/null || true
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleIdentifier</key><string>rs.azul.widgets-demo</string>
  <key>CFBundleExecutable</key><string>AzWidgets</string>
  <key>CFBundleName</key><string>AzWidgets</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.1</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
codesign --force --deep -s - "$APP"
open "$APP"
```

1. "Platform support": *Available - UNUserNotificationCenter (the notification permission is asked on
   the first post)*.
2. "Post a notification" → the permission prompt → Allow → a banner "Azul Widget Showcase" / "A native
   notification, posted by the widgets demo." (shown although the app is frontmost:
   `willPresentNotification`). "Last event": *Posted - …*.
3. Click the banner → *Clicked (the notification itself).*
4. Post, hover the banner, open its options → "Show me" → *Button pressed: "show-me".*
5. Post, then clear it in Notification Center → *Dismissed (cleared by the user).*
6. Post, then "Withdraw it" → gone from Notification Center; *Withdrawn - …*; nothing later.
7. System Settings → Notifications → AzWidgets → off; post → *Not shown: …* (UN's error); relaunch →
   "Platform support" *Unavailable … turned off*.
8. `AZ_LOG=info` shows `[notifications] UNUserNotificationCenter ready`.

### Linux (X11 and Wayland)

1. In a second terminal: `dbus-monitor "interface='org.freedesktop.Notifications'"`.
2. Run AzWidgets under GNOME / KDE / sway+mako / dunst, once with `AZ_WINDOW=x11`, once with
   `AZ_WINDOW=wayland`.
3. "Platform support": *Available - org.freedesktop.Notifications (<server> <version>)*; on a server
   without `actions` the reason says buttons/clicks are not reported.
4. Post → `Notify` with `actions ["default","Open","show-me","Show me"]` in the monitor; a notification
   appears.
5. Click the body → `ActionInvoked(id, "default")` → *Clicked (the notification itself).* (The
   `NotificationClosed` that follows must NOT change the label again.)
6. Post, press "Show me" → *Button pressed: "show-me".*
7. Post, close it → *Dismissed (dismissed by the user).*; post and wait for expiry → *Dismissed
   (expired).*
8. Post, "Withdraw it" → `CloseNotification`; the label stays *Withdrawn - …*.
9. No server: `dbus-run-session -- ./target/release/AzWidgets` in a session without a daemon →
   *Unavailable - …: no notification server on the session bus …*; post → *Not shown: …*.

### Windows 10 / 11

1. Run `target\release\AzWidgets.exe` (unpackaged).
2. "Platform support": *Available - Shell_NotifyIconW balloon (NIF_INFO) (shown as a toast attributed
   to the executable; no buttons …)*.
3. Post → a toast titled "Azul Widget Showcase", attributed to AzWidgets.exe; an icon appears in the
   notification area while it is up. Log warns once that the button is not shown.
4. Click the toast → *Clicked (the notification itself).*; the tray icon goes away.
5. Post, let it time out (or close it with its X) → *Dismissed (timed out or closed by the user).*
6. Post twice → one toast; the first reports nothing visible (it was replaced; its callback got
   `Dismissed`, which the demo shows only until the second post's *Posted* overwrites it).
7. "Withdraw it" → toast and icon gone.
8. Quit → no ghost icon left in the notification area.
9. Focus Assist / Do Not Disturb on → the balloon respects quiet time (`NIIF_RESPECT_QUIET_TIME`).

### Headless / AZ_E2E

```json
[
  {
    "name": "widgets_demo_posts_a_notification",
    "steps": [
      { "op": "click", "text": "Post a notification" },
      { "op": "wait_frame" },
      { "op": "assert_notification", "id": "azul-widgets-demo", "title": "Azul Widget Showcase",
        "action": "show-me", "withdrawn": false, "count": 1 },
      { "op": "click", "text": "Withdraw it" },
      { "op": "wait_frame" },
      { "op": "assert_notification", "id": "azul-widgets-demo", "withdrawn": true }
    ]
  }
]
```

`AZ_BACKEND=headless AZ_E2E=that.json target/release/AzWidgets` (a build with `e2e-scripting`,
which the demo enables). The post reaches the recorder at the next event pass (capability pump) or
the headless loop's Phase 1c - one `wait_frame` should do; add `{ "op": "wait", "ms": 100 }` if not.

---

## 6. Least sure to compile

1. **macOS `define_class!` delegate** (`notifications/macos.rs`): the two methods take the UN
   completion handlers as `*mut block2::Block<dyn Fn()>` and `*mut Block<dyn Fn(usize)>` and call
   `.call(())` / `.call((opts,))`. Relies on block2 0.6 giving raw block pointers `EncodeArgument`.
   Also no `#[thread_kind]` (AllocAnyThread default, like `screencap/macos.rs`).
2. **macOS `extern "C" { static NSApp: *mut AnyObject; }`** under `#[link(name = "AppKit")]`, read
   from UN's queue to post the wake event without calling `sharedApplication` off the main thread.
3. **macOS `msg_send!` typing**: `Option<Retained<AnyObject>>` returns for `new` (method family) and
   for class convenience constructors (`array`, `set`, `categoryWithIdentifier:…`,
   `attachmentWithIdentifier:URL:options:error:` with an explicit `&mut *mut AnyObject` error
   out-param, per the objc2 trap in memory); the 9-argument `otherEventWithType:…` with `NSPoint`,
   `usize`, `f64`, `isize`, `i16`.
4. **`RcBlock::new(move |granted: Bool, error: *mut AnyObject| …)`** capturing an owned
   `PendingPost` and the center as `usize`; nested `objc2::rc::autoreleasepool`.
5. **Windows `NotifyIconDataW`** is hand-declared (976 bytes on x64 expected; `guidItem` as
   `[u32; 4]`) and `Shell_NotifyIconW` / `LoadIconW` / `LoadImageW` are resolved with `libloading`
   (`*lib.get::<FnPtr>(b"…\0").ok()?`); `WNDCLASSW` / `CreateWindowExW` from `Win32Libraries::shared()`.
6. **Linux** `match CStr::to_bytes() { b"ActionInvoked" => … }` byte-string patterns on `&[u8]`,
   `Some(notification_filter)` into the new `DBusHandleMessageFunction` parameter, and the two new
   `load_symbol!` entries.
7. **`invoke_deliveries<W: PlatformWindow>`** called with `&mut self` inside `HeadlessWindow::run(mut
   self)`, with `X11Window` / `WaylandWindow` / `Win32Window` / `MacOSWindow` from the loops.
8. **Clippy (`#![deny(clippy::all)]` in `desktop/`)**: written to avoid `match_single_binding`,
   `single_match`, `unnecessary_cast`, `type_complexity`; `1usize as *const u16` (MAKEINTRESOURCE)
   and `let _: () = msg_send![…]` follow existing code.
9. **The demo against the generated bindings**: assumes autofix puts the types in
   `azul::notification`, keeps `PlatformCapability` in `azul::window`, generates
   `OptionNotificationEvent::into_option`, and that generated `create`/`with_*` take
   `Into<String>` (as `StringMenuItem::create` does).
10. **`core`**: `impl_option!(NotificationCallback, …, [.., PartialOrd, Ord, Hash])` relies on
    `RefAny` / `CoreCallback` being `Ord + Hash` (they are, via `CoreMenuCallback`'s derives).
11. **The delivery test** runs `LayoutWindow::invoke_single_callback` on a window laid out from an
    empty body with `FcFontCache::default()`.

---

## 7. Open

* **api.json + codegen** (parent): §3. The demo cannot build before it.
* **macOS app bundling** for the examples: without it the demo reports "Unavailable" from
  `target/release`. The recipe above is manual.
* **Windows AUMID / WinRT toast** for installed apps (buttons, several notifications at once) - the
  research doc's "optional AUMID hook". `TaskbarCreated` (explorer restart) is not handled for the
  transient notify icon; an Action Center entry is inert once the icon is removed.
* **Mobile**: iOS could use the same UN code (an iOS app is always bundled); Android needs
  `NotificationManager` over JNI + `POST_NOTIFICATIONS`. Neither delivers events today - no mobile
  run loop calls `pump_notifications`; posts only produce a logged `Failed`.
* **Web**: the browser `Notification` API is not wired; requests stay in the bounded queue.
* **Linux**: `Notify` blocks the UI thread for its reply (up to 3 s; an async pending call needs more
  libdbus symbols). A notification server restart is not watched (`NameOwnerChanged`), so a
  notification shown by the old server never reports. The loop polls at 100 ms while any posted
  notification is still live - including callback-less ones - until the server reports it closed.
* **macOS**: a banner that simply times out reports nothing (UN reports a dismissal only when the
  user clears it from Notification Center); `willPresentNotification` always shows banners.
* **Registry**: an entry the OS never reports on is never forgotten (bounded only by the app's
  ids - the same id replaces).
* **Tray**: `drain_tray_events()` has no caller (§1) - bare tray events never reach the app.
* **E2E**: no op injects a platform notification event (Rust tests queue into the mailbox directly);
  no scenario for the demo is committed (the one in §5 is untested).
* **Merge hot spots** with the parallel agents: `doc/src/autofix/module_map.rs` (MODULES list),
  `dll/src/desktop/shell2/run.rs` (loop bodies), `dll/src/desktop/extra/capability.rs`,
  `layout/tests/all.rs`, `examples/azul-widgets/src/lib.rs` (`Showcase` fields, the body's
  `with_child` chain, `start()`).
