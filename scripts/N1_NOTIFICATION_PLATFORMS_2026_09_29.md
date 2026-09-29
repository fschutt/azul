# N1 - notification platform leftovers - 2026-09-29

Branch `wt/n1-notification-platforms`, based on `0a326afe5` (pushed tip of
`fix/input-bugs-2026-09-19`, PR #476). Source: the "not done, written up as plans" section of
`scripts/F2_NOTIFICATIONS_2026_09_29.md`, and `NOTIFICATIONS_RESEARCH_2026_09_28.md` G4/G5/G6.

**Nothing here was compiled or run** (house rule: the parent compiles once). **No api.json
change.** The ABI part of item 4 (`AppConfig::app_id`) is a proposal for the user (section 3).

---

## 1. What was built

| # | item | status | RED | fix |
|---|---|---|---|---|
| 1 | iOS never marks the response that LAUNCHED the app | done | `715dc1f72` | `77740437b` |
| 2 | Android asks for a permission from the event-loop thread | done | `2fb54ca99` | `5a4676bbb` |
| 3 | Windows: no click after the app exited (COM activator) | done | `ffb2ff0e4` | `7e22e2a34` |
| 4 | One app identity | internal single source done; ABI field = proposal (section 3) | `217ca3977` | `067e30666` |
| 5 | Bundle step: icon, the dylibs' own dependencies, notarization | icon + walk done; notarization documented | `a162726cf` | `4c6255c1f` |
| 6 | Linux: Flatpak portal, async `Notify`, server restart | done | `333ff63b5` | `fbe4dc293` |

Also: `d2b410b12` (doc of `NotificationEvent::launched_app`), `73219938f` (wiring guards),
`e254a7dc3` (progress file; it rides along in the fix commits).

### 1. iOS `launched_app` (`wire::LaunchResponseMarker`)

One decision for every platform that has a launch response:

* **macOS** names it (`NSApplicationLaunchUserNotificationKey`): only that response is marked,
  whenever it arrives - also after `applicationDidBecomeActive:` (unchanged behaviour; the
  private `LAUNCH_RESPONSE_ID` is gone).
* **iOS** names nothing for a local notification. `install_launch_hooks` (from
  `didFinishLaunching`) calls `expect_first()`; the first response after it is the launch
  response, unless `applicationDidBecomeActive:` came first (`launch_finished()`).
* **Windows** (item 3): `expect_first()` when COM started the process with `-ToastActivated`.

`notifications::refresh_permission` became `notifications::app_became_active` (permission
re-read + end of the launch window). macOS calls it where it called `refresh_permission`; the
iOS delegate's `applicationDidBecomeActive:` now calls it too - iOS never re-read the permission
on activation before.

Known limit: an iOS app launched into the BACKGROUND (background fetch), which then gets a
tap before it was ever active, marks that tap `launched_app` too.

### 2. Android permission request on the UI thread

* `scripts/android/AzulPermissions.java`: `request(Activity, String[], int)` runs
  `activity.requestPermissions` inside `runOnUiThread`; a `RuntimeException` there reports a
  denial through `nativeOnPermissionResult`, so the Rust side never waits for an answer that
  cannot come.
* `extra/permission/android.rs::request_permission` calls it through `find_app_class` (the
  activity's class loader - `find_class` on a native thread sees no APK class). No fallback to
  the direct call: the same class forwards the answer, so an APK without it could never report
  one.
* RED is a source invariant (nothing headless observes a thread):
  `dll/src/desktop/notifications/platform_invariants.rs` (new; reuses
  `loop_wakeup_invariants::top_level_fn_body`, made `pub(crate)`).

### 3. Windows COM activator

* `wire`: `TOAST_ACTIVATED_SWITCH`, `launched_by_toast_activation(args)`,
  `toast_activator_clsid(aumid)` (a stable RFC 9562 version-8 UUID from two FNV-1a hashes of the
  AUMID), `guid_string`, `RegistryValue`, `toast_registry_values(aumid, display_name, exe)`
  (DisplayName; CustomActivator = `{CLSID}`; `CLSID\{CLSID}\LocalServer32` = `"<exe>"
  -ToastActivated`), `toast_activator_event(our_aumid, aumid, args)`.
* `notifications/windows.rs`:
  * `activator` module: `#[implement(INotificationActivationCallback)] ToastActivator` and
    `#[implement(IClassFactory)] ToastActivatorFactory`, registered with
    `CoRegisterClassObject(CLSCTX_LOCAL_SERVER, REGCLS_MULTIPLEUSE)` after
    `dnd::ensure_ole_initialized()` (STA: COM hands `Activate` to the main thread through its
    message loop). `Activate` translates like the in-process `Activated` event, sets
    `launched_app` from the marker, queues, wakes.
  * `install_launch_hooks()`: `-ToastActivated` on the command line -> `expect_first()` and
    register at once; otherwise register only if an earlier run registered the activator
    (`RegGetValueW` of `CustomActivator`), so a click on an old Action Center entry reaches the
    RUNNING process instead of starting a second one. An app that never posted pays nothing.
  * The first post writes the three values (`register_app`; `DisplayName` stays required, the
    activator values are best effort) and registers the class object.
  * The registry goes through one cached advapi32 (`write_registry_value`,
    `read_registry_string`) instead of a `LoadLibrary` per write.
* `shell2/run.rs` Windows `run()` calls `notifications::install_launch_hooks()` before its first
  window. `dll/Cargo.toml`: windows feature `Win32_UI_Notifications`.
* A click while the app runs may reach BOTH the toast's `Activated` handler and the activator;
  the registry delivers the first and swallows the second (the id ENDED).

### 4. One app identity (internal single source)

* `wire::AppIdentity { id, exe_name, source: AppIdSource }`, built by
  `AppIdentity::from_executable(exe)` (`com.azul.<exe name, lowercase, other runs -> '-'>`) or
  `AppIdentity::declared(id, exe)`; projections `apple_bundle_id()` (no `_`),
  `windows_aumid()`, `desktop_entry()` (declared id, else the exe name), `display_name()`.
* `dll/src/desktop/app_identity.rs::current()` - THE function every shell calls, read once:
  a `.app`'s `CFBundleIdentifier` (macOS/iOS, `notifications::apple_bundle_id`), `FLATPAK_ID`
  (Linux), the package from `/proc/self/cmdline` (Android), else the executable.
* Readers now: the Windows toast AUMID + `DisplayName`; the freedesktop `app_name` +
  `desktop-entry` hint; the Wayland `app_id` default; the X11 `WM_CLASS` default
  (`current_exe_name`); `azul-doc bundle macos`'s default `CFBundleIdentifier` (`bundle_id_for`
  delegates, and derives from the bundled BINARY's name like the running app does).
* **Behaviour change:** the Windows AUMID of an unnamed app is `com.azul.azwidgets` (was
  `azul.AzWidgets`), the same id the app gets as a macOS bundle. The old HKCU key is orphaned
  (manual check W5).
* **Twin left alone (reported, NO DUPLICATION rule):** `doc/src/mobile/run.rs::Target::resolve`
  derives `bundle_id = com.azul.<crate lowercase, '-' -> '_'>` for BOTH iOS and Android. For a
  hyphenated crate that is an invalid `CFBundleIdentifier` (Apple allows no `_`). The fix is two
  projections of one identity (`apple_bundle_id()` and an Android package form); not in this
  task's files.
* Before this there were four copies of "read `current_exe` and take its name" (Wayland, X11,
  freedesktop `app_name` (stem), Windows (stem)); now one.

### 5. The bundle step (`doc/src/bundle.rs`)

* **Icon:** `--icon <path>` or the crate's `[package.metadata.bundle] icon` (the `cargo-bundle`
  key, `configured_icons`; list or single path, multi-line arrays). An `.icns` is copied; square
  PNGs of 16/32/64/128/256/512/1024 px are wrapped into one as they are (`icns_from_pngs`: PNG
  elements `icp4 icp5 icp6 ic07 ic08 ic09 ic10`, no `iconutil`, no resampling); unusable files
  are named and skipped. Written to `Contents/Resources/AppIcon.icns`, `CFBundleIconFile` in the
  plist (`MacBundleSpec::icon_file`, `BundlePaths::resources`).
* **Dependency walk:** `plan_dylib_tree` walks breadth first: the binary's `otool -L`, then each
  planned dylib's. Its own install name (the first line `otool -L` prints for a dylib) is not a
  dependency; a library two dylibs share is bundled once; a cycle ends. The command gives each
  copy its bundle install name (`install_name_tool -id`) and rewrites every reference - in the
  binary AND in the dylibs (`-change`). Copies are made writable first (Homebrew ships 0444).
* **`--portable`:** `DylibScope::NonSystem` also bundles every library outside `/usr/lib` and
  `/System` (Homebrew, MacPorts); the default stays the build's own (`DylibScope::Build`, the
  existing `plan_dylibs` rule).
* **Notarization:** documented only (module docs + usage): Developer ID signature with
  `--options runtime --timestamp` inside out, `ditto` + `xcrun notarytool submit --wait`,
  `xcrun stapler staple`, `spctl --assess`. It needs an Apple Developer account and credentials.

### 6. Linux

* **`Notify` is asynchronous.** `dbus_connection_send_with_reply` + a pending call; `pump()`
  drains the connection (which completes pending calls) and collects the replies; no reply
  within 25 s (the D-Bus default) is `Failed`. The bookkeeping is pure and tested:
  `wire::FreedesktopPosts` (server id <-> app id; a reply after a repost is stale and closed,
  unless it is the id a newer post replaces in place; a reply after a withdraw is closed; a
  server that ignores `replaces_id` gets the old one closed; an error is `Failed` for the NEWEST
  post only; `NotificationClosed` forgets the id). It replaces the old `SERVER_IDS` map.
* **Server restart:** a match on `NameOwnerChanged` for `org.freedesktop.Notifications`
  (`wire::freedesktop_server_left`: an old owner left - a quit or a restart). Every notification
  it showed ends as `Dismissed` ("the notification server went away (it quit or restarted)"),
  the next post starts with nothing to replace, and the probe cache is cleared.
* **Flatpak portal:** `wire::in_flatpak_sandbox` (`/.flatpak-info` or `FLATPAK_ID`) switches the
  backend to `org.freedesktop.portal.Notification` on the SAME libdbus connection (no zbus, no
  second connection, the same filter and wait set): `AddNotification(id, a{sv})` with `title`,
  `body`, `default-action`, `buttons` (`aa{sv}`); `RemoveNotification`; the portal's
  `ActionInvoked(id, action, av)` in the filter (`wire::portal_action_event`). An `app.`-prefixed
  button id is sent as `azul.app.<x>` (the portal would activate `app.*` on the app's D-Bus name
  instead of reporting it) and comes back as the app's id. Errors: `Failed` for the newest post
  under an id. The portal has no closed signal: no dismissal is ever reported there (the probe
  says so).
* libdbus: 7 more symbols (`dbus_connection_send_with_reply`,
  `dbus_pending_call_get_completed` / `steal_reply` / `cancel` / `unref`,
  `dbus_message_get_type`, `dbus_set_error_from_message`) + `DBusPendingCall`,
  `DBUS_MESSAGE_TYPE_ERROR`, `DBUS_TIMEOUT_USE_DEFAULT`. All exist in every libdbus-1.

---

## 2. Commits

| commit | kind | what |
|---|---|---|
| `e254a7dc3` | chore | progress file |
| `715dc1f72` | test RED | `mod platforms` appended to `layout/tests/native_notifications.rs`; `wire::LaunchResponseMarker` stub |
| `77740437b` | fix | marker; apple.rs; `app_became_active` (mod.rs, macos/mod.rs, ios/mod.rs) |
| `217ca3977` | test RED | identity tests; `wire::AppIdentity` stub |
| `067e30666` | fix | `AppIdentity`; `desktop/app_identity.rs`; readers in windows.rs, linux.rs, wayland, x11, bundle.rs |
| `ffb2ff0e4` | test RED | activator tests; wire stubs |
| `7e22e2a34` | feat | activator wire + windows.rs `activator`, `install_launch_hooks`, registry; run.rs; dnd.rs; Cargo feature |
| `2fb54ca99` | test RED | `platform_invariants.rs` Android invariant |
| `5a4676bbb` | fix | `AzulPermissions.request`; `permission/android.rs` |
| `333ff63b5` | test RED | Linux + portal tests; wire stubs |
| `fbe4dc293` | feat | `FreedesktopPosts` etc.; linux.rs rewrite; dbus dlopen |
| `a162726cf` | test RED | 9 bundle tests; stubs; `MacBundleSpec::icon_file` |
| `4c6255c1f` | feat | icon + walk + `--portable` + notarization docs |
| `d2b410b12` | docs | `NotificationEvent::launched_app` field doc (core) |
| `73219938f` | test | 3 wiring guards (green on arrival) |

### Expected RED -> green

| RED | tests | today (at the RED commit) | after the fix |
|---|---|---|---|
| `715dc1f72` | `platforms::the_tap_that_cold_launched_the_app_is_marked_where_the_os_names_no_notification`, `macos_marks_exactly_the_response_the_launch_named`, `a_named_launch_response_stays_marked_when_it_arrives_after_the_activation` | the stub marks nothing | green; the two guards (`a_tap_on_an_app_that_was_already_active_did_not_launch_it`, `a_process_that_was_not_launched_for_a_notification_marks_nothing`) green throughout |
| `217ca3977` | the 5 identity tests | empty strings | green |
| `ffb2ff0e4` | the 5 activator tests | 0 / empty / `None` | green |
| `2fb54ca99` | `platform_invariants::the_android_permission_dialog_is_requested_on_the_ui_thread` | `request_permission` contains `"requestPermissions"` | green |
| `333ff63b5` | the 13 Linux/portal tests | the stubs map nothing | green |
| `a162726cf` | the 9 new `bundle::tests` | empty / `None` / `Err("not implemented")` | green; the 11 old ones green throughout |

---

## 3. Proposal: `AppConfig::app_id` (the ABI part of item 4 - the USER's decision)

**Field.** `pub app_id: AzString` in `AppConfig` (`core/src/resources.rs`), placed right after
`theme: AzString` - among the 8-aligned fields, before the 4-aligned `color_scheme`. An `AzString`
is 8-aligned and a multiple of 8 in size, so `app_config_has_no_padding_between_its_fields`
(`core/src/resources_test.rs`) stays green; the api.json `struct_fields` order must match.

**Default.** Empty = not declared: the identity is what `desktop::app_identity::current()`
derives today (the platform's declaration, else `com.azul.<executable>`). No behaviour change
for an app that does not set it.

**API (api.json terms).**

| class | fn | args -> return | fn_body |
|---|---|---|---|
| `AppConfig` | `set_app_id` | `&mut self, app_id: String` | `object.set_app_id(app_id)` |
| `AppConfig` | `with_app_id` | `self, app_id: String` -> `AppConfig` | `object.with_app_id(app_id)` |

Doc (ASCII): "Reverse-DNS id of the app (`org.example.Editor`): the Windows toast AUMID, the
freedesktop `desktop-entry` and the default Wayland `app_id` / X11 `WM_CLASS`. macOS, iOS and
Android keep the id their bundle / package declares."

**How each OS uses it** (the runtime side is the one function that exists now:
`App::run` would call a `desktop::app_identity::declare(&config.app_id)` before
`install_launch_hooks` and any window, and `current()` then builds `AppIdentity::declared`).

| OS | precedence | effect |
|---|---|---|
| macOS / iOS | the running bundle's `CFBundleIdentifier` WINS | UN, TCC and LaunchServices key on it and an app cannot change it at run time. A differing `app_id` is logged as a warning. |
| Android | the manifest package WINS | the same reasoning; `app_id` feeds the build tools' default only |
| Windows | `app_id` wins | AUMID = `windows_aumid(app_id)`; the COM activator's CLSID follows (derived from the AUMID) |
| Linux | `FLATPAK_ID` wins inside Flatpak (the sandbox is authoritative); else `app_id` | the `desktop-entry` hint, the Wayland `app_id` default (a window's `wayland_app_id` still overrides), the X11 `WM_CLASS` instance default. GNOME then needs `<app_id>.desktop` installed; without it attribution works only through the PID while a window is mapped. |

**The build tools** cannot read a runtime `AppConfig`. Proposal: the build-time declaration is
`[package.metadata.bundle] identifier` (the `cargo-bundle` key - the icon already comes from
that table); `azul-doc bundle macos` and `mobile build` default `--bundle-id` / `--package` to
it, and the app sets the same string in `AppConfig::app_id`. A debug build could warn when the
bundle's id and `app_id` disagree.

**Migration.** Source-compatible (empty default). It changes `AppConfig`'s size, so it is an ABI
break for prebuilt binaries of every binding (api.json + codegen regenerate). An app that
starts setting `app_id` on Windows gets a new Settings > Notifications entry; its old AUMID key
(`com.azul.<exe>`, and the pre-N1 `azul.<exe>`) stays in HKCU unless the app deletes it (a
one-time `RegDeleteTreeW` could be offered). On Linux the `.desktop` file must be renamed to
`<app_id>.desktop`.

**Validation.** Keep the id as given; each projection already enforces its OS's characters
(`apple_bundle_id` drops `_`, `windows_aumid` drops `\` and caps at 129). Suggested rule for the
doc: 2+ dot-separated elements of `[A-Za-z0-9-]`, each starting with a letter.

---

## 4. API

**No api.json change.** No public type of `azul-core` / `azul-layout` / `azul-dll` that api.json
lists was added or changed (`api.json` has no entry for any name below; the `launched_app` field
has no doc string there).

Rust-only additions:

* `azul_layout::managers::notification::wire`: `LaunchResponseMarker` (`new`, `name`,
  `expect_first`, `launch_finished`, `launched_app`); `AppIdSource`, `AppIdentity`
  (`from_executable`, `declared`, `apple_bundle_id`, `windows_aumid`, `desktop_entry`,
  `display_name`); `TOAST_ACTIVATED_SWITCH`, `launched_by_toast_activation`,
  `toast_activator_clsid`, `guid_string`, `RegistryValue`, `toast_registry_values`,
  `toast_activator_event`; `FreedesktopActions`, `FreedesktopPosts` (`new`, `post`, `replied`,
  `expired`, `withdraw`, `app_id_of`, `replaces_id`, `closed`, `server_gone`),
  `FREEDESKTOP_SERVER_NAME`, `freedesktop_server_left`; `PortalNotification`,
  `portal_notification`, `portal_action_event`, `in_flatpak_sandbox`.
* `azul_dll::desktop`: `app_identity::current`; `notifications::app_became_active` (was
  `refresh_permission`), `notifications::apple_bundle_id`; `shell2::linux::dbus::{DBusPendingCall,
  DBUS_MESSAGE_TYPE_ERROR, DBUS_TIMEOUT_USE_DEFAULT}` and 7 `DBusLib` fields;
  `shell2::windows::dnd::ensure_ole_initialized` is `pub(crate)`.
* `azul-doc` (binary): `DylibScope`, `RelinkFile`, `Relink`, `DylibTree`, `plan_dylibs_in`,
  `plan_dylib_tree`, `configured_icons`, `png_size`, `icns_type_for`, `icns_from_pngs`,
  `ICON_FILE`; `MacBundleSpec::icon_file`, `BundlePaths::resources`; flags `--icon`,
  `--portable`.
* Java: `com.azul.permission.AzulPermissions.request(Activity, String[], int)`.
* Cargo: `windows` feature `Win32_UI_Notifications`.

---

## 5. Least sure to compile

`rustfmt --check` parsed every changed file (syntax only). Type-level risks, most likely first:

1. **`notifications/windows.rs` `activator`** (Windows only):
   * `#[implement(INotificationActivationCallback)]` / `#[implement(IClassFactory)]` on structs
     with a `String` field; `impl INotificationActivationCallback_Impl for ToastActivator_Impl`
     with `fn Activate(&self, &PCWSTR, &PCWSTR, *const NOTIFICATION_USER_INPUT_DATA, u32)` (the
     0.62.2 trait, read from the vendored source);
   * `IClassFactory_Impl::CreateInstance(&self, Ref<IUnknown>, *const GUID, *mut *mut c_void)`,
     `punkouter.is_some()` through `Ref`'s `Deref<Target = Option<IUnknown>>`,
     `unsafe { activator.query(riid, ppvobject) }.ok()` (`Interface::query` in scope);
   * `LockServer(&self, _flock: BOOL)` with `windows::core::BOOL` (re-exported from
     windows-result 0.4);
   * `CoRegisterClassObject(&clsid, &factory, CLSCTX_LOCAL_SERVER, REGCLS_MULTIPLEUSE)` relying
     on `Param<IUnknown> for &IClassFactory` (CanInto from `interface_hierarchy!`);
   * `PCWSTR::to_string()` is `unsafe` and returns `Result<String, FromUtf16Error>`;
   * the `Win32_UI_Notifications` feature must be enough for `INotificationActivationCallback_Impl`
     (dnd's `IDropTarget_Impl` needs nothing extra, so it should be);
   * `toast`: `OnceLock<Result<Advapi32, String>>` with `?` inside `get_or_init(|| unsafe {..})`,
     `.as_ref().map_err(Clone::clone)`.
2. **`notifications/linux.rs`** (Linux only): the `(&str, &str, &str)` consts passed as a
   destructured fn parameter; `&[u8] == b"..."` comparisons and byte-string match patterns on
   `&[u8]`; `unsafe fn send_notify(&self)`; `let ... else { return; }` inside `unsafe { }` in a
   match arm; the `if / else if / else { ...; continue; }` expression in the reply loops; format
   strings capturing consts (`{PORTAL_IFACE}`, `{BUS_IFACE}`).
3. **`layout/src/managers/notification.rs`**: `Ok(server_id) if is_newest` (bind-by-move guard),
   `BTreeMap::into_values`, `const fn FreedesktopPosts::new` with three `BTreeMap::new()`.
4. **`extra/permission/android.rs`**: `find_app_class(env, &activity, ..)` with `env: &mut JNIEnv`
   from `attach`'s closure (implicit reborrow), `call_static_method(&helper, ..)`.
5. **`apple.rs`**: `#[cfg(target_os = "ios")]` on an expression statement (a method chain).
6. **`doc/src/bundle.rs`**: `icns_type_for`'s `Some(match .. { _ => return None })`,
   `png[..8] != SIGNATURE`, the nested `fn is_icns(&Path)` called with `&&PathBuf`, the test's
   `listing_of` closure passed twice (it must be `Copy`).
7. **`platform_invariants.rs`**: `include_str!` paths (`../extra/permission/android.rs`,
   `../../../../scripts/android/AzulPermissions.java`, `../shell2/run.rs`,
   `../shell2/ios/mod.rs`, `linux.rs`). The string anchors were checked with a Python replay.

---

## 6. Test commands (for the parent)

```sh
cargo test -p azul-layout --test all native_notifications          # incl. native_notifications::platforms (28 new)
cargo test -p azul-dll --lib platform_invariants                   # 4 (1 RED->green, 3 guards)
cargo test -p azul-dll --lib loop_wakeup_invariants                # unchanged, must stay green (reads linux.rs)
cargo test -p azul-doc --bin azul-doc bundle::                     # 20 (9 new)
# the platform code this host cannot run - cross checks:
cargo check -p azul-dll --target x86_64-pc-windows-msvc            # windows.rs activator, run.rs, dnd.rs
cargo check -p azul-dll --target x86_64-unknown-linux-gnu          # linux.rs, dbus dlopen, wayland, x11
cargo check -p azul-dll --target aarch64-apple-ios                 # apple.rs, ios/mod.rs, app_identity
cargo check -p azul-dll --target aarch64-linux-android --features jni   # permission/android.rs
```

---

## 7. Manual checks, per OS

Never drive the real Mac's input while the user works (memory rule). These are for a person.

### iOS (simulator or device; set an app-level handler that shows kind, payload, launched_app)

1. Post with `.with_payload("x")`; kill the app from the app switcher; tap the notification.
   Expected: the app launches; the handler gets `Activated`, payload `x`, **`launched_app = true`**.
2. Background the app (Home), tap a notification. Expected: `launched_app = false`.
3. App in front, tap the banner. Expected: `launched_app = false`.
4. On a DEVICE, check the order in the log for step 1: the handler's event must come BEFORE
   `[iOS] applicationDidBecomeActive:`. If the device delivers the response after the first
   activation, the heuristic is wrong - report it.
5. Settings > Notifications > the app > off; switch back to the app. Expected: the capability
   now reads "turned off" (iOS re-reads on activation since this branch).

### Android (API 33+ emulator, `azul-doc mobile`)

1. Fresh install; press a button calling `request_notification_permission()`. Expected: the
   system dialog; `adb logcat | grep -iE "AzulPermissions|requestPermissions|Looper"` shows no
   thread warning; Allow -> `PermissionChanged` -> Granted.
2. `adb shell pm revoke <pkg> android.permission.POST_NOTIFICATIONS` and
   `adb shell pm clear-permission-flags <pkg> android.permission.POST_NOTIFICATIONS user-set user-fixed`,
   request again. Expected: the dialog again. Deny -> Denied.
3. A camera or location permission request (same path). Expected: its dialog, its answer.

### Windows 10 / 11 (unpackaged `AzWidgets.exe`)

1. Post once. `reg query "HKCU\Software\Classes\AppUserModelId\com.azul.azwidgets"` shows
   `DisplayName REG_SZ AzWidgets` and `CustomActivator REG_SZ {GUID}`;
   `reg query "HKCU\Software\Classes\CLSID\{GUID}\LocalServer32"` shows
   `"C:\...\AzWidgets.exe" -ToastActivated`. Log: `toast activator {GUID} registered for
   com.azul.azwidgets`.
2. Quit the app. Open the Action Center, click the entry. Expected: the app starts (Task
   Manager's command line column: `... -ToastActivated -Embedding`), log `started by COM for a
   click on a toast`, the app-level handler gets `Activated` with the payload and
   **`launched_app = true`**. Repeat with a button of a kept entry: `ActionInvoked` + the id.
3. App running, click an Action Center entry of this run. Expected: exactly ONE event.
4. Start the app normally after an earlier run posted; click an OLD entry. Expected: no second
   process; the running app gets the event (`launched_app = false`).
5. Migration: the pre-N1 AUMID `azul.AzWidgets` still has its own Settings entry; remove it with
   `reg delete "HKCU\Software\Classes\AppUserModelId\azul.AzWidgets" /f`.
6. Run elevated (Run as administrator): step 2 does not start the app (known COM limit).

### Linux

1. **Async Notify** (a dunst or mako session, e.g. sway + mako): `killall -STOP mako`, post.
   Expected: the window keeps repainting and reacting (before: frozen up to 3 s, then `Failed`).
   `killall -CONT mako` within 25 s: the notification appears and its clicks work. Leave it
   stopped for 25 s and move the mouse over the window: `Not shown: the notification server did
   not answer Notify within 25 s`.
2. **Repost before the reply**: post twice quickly under one id (freeze mako as in 1 between the
   two, then continue). Expected: ONE notification on screen (the stale one is closed).
3. **Withdraw before the reply**: post + withdraw in one callback (mako frozen, then continued).
   Expected: nothing stays on screen.
4. **Server restart**: with notifications on screen, `killall mako; mako &` (or
   `systemctl --user restart dunst`). Expected: each one's callback gets `Dismissed` - "the
   notification server went away (it quit or restarted)"; the next post shows on the new server.
   `dbus-monitor "type='signal',interface='org.freedesktop.DBus',member='NameOwnerChanged'"`
   shows `org.freedesktop.Notifications` changing owner.
5. **Unchanged identity**: `dbus-monitor "interface='org.freedesktop.Notifications'"` - every
   `Notify` still carries `"desktop-entry": <"AzWidgets">` and app_name `AzWidgets`.
6. **Flatpak**: build a Flatpak of AzWidgets (id e.g. `rs.azul.Widgets`, `--socket=wayland
   --share=ipc`, WITHOUT `--talk-name=org.freedesktop.Notifications`). Run it. Expected: log
   `inside a Flatpak sandbox: notifications go through org.freedesktop.portal.Notification`;
   "Platform support" reads the portal backend; a post shows attributed to `rs.azul.Widgets`;
   body click -> *Clicked*; "Show me" -> *Button pressed*; swiping it away reports nothing (the
   portal has no closed signal). `WAYLAND_DEBUG=1 flatpak run rs.azul.Widgets 2>&1 | grep
   set_app_id` shows `rs.azul.Widgets` (the one identity: `FLATPAK_ID`).

### macOS - the bundle step

```sh
cargo build --release -p AzWidgets
sips -z 1024 1024 <any square png> --out /tmp/icon-1024.png
cargo run --release -p azul-doc -- bundle macos azul-widgets --icon /tmp/icon-1024.png --dry-run
#   [bundle]   icon -> Contents/Resources/AppIcon.icns
#   [bundle]   .../out/libazul.dylib -> Contents/Frameworks/libazul.dylib
cargo run --release -p azul-doc -- bundle macos azul-widgets --icon /tmp/icon-1024.png
plutil -p ~/Applications/AzWidgets.app/Contents/Info.plist | grep CFBundleIconFile   # "AppIcon.icns"
iconutil -c iconset ~/Applications/AzWidgets.app/Contents/Resources/AppIcon.icns -o /tmp/x.iconset   # succeeds
otool -L ~/Applications/AzWidgets.app/Contents/MacOS/AzWidgets      # @executable_path/../Frameworks/libazul.dylib
otool -D ~/Applications/AzWidgets.app/Contents/Frameworks/libazul.dylib   # @executable_path/../Frameworks/libazul.dylib
codesign --verify --strict --verbose=2 ~/Applications/AzWidgets.app       # valid on disk
open ~/Applications/AzWidgets.app                                           # Dock shows the icon
```

1. The icon in Finder / the Dock (a stale icon cache: `touch ~/Applications/AzWidgets.app`).
2. `--portable` with a binary that links a Homebrew library: `otool -L` of the binary and of every
   `Contents/Frameworks/*.dylib` names no `/opt/homebrew` path;
   `DYLD_PRINT_LIBRARIES=1 ~/Applications/X.app/Contents/MacOS/X 2>&1 | grep homebrew` is empty.
3. A crate with `[package.metadata.bundle] icon = ["icons/32x32.png", "icons/128x128.png"]`:
   bundled without `--icon`; a non-square PNG in the list is named and skipped.

---

## 8. Files other tasks may own (minimal edits)

`dll/src/desktop/shell2/run.rs` (one call in the Windows `run()`), `shell2/macos/mod.rs` (one
renamed call), `shell2/ios/mod.rs` (one call), `shell2/linux/wayland/mod.rs` and
`shell2/linux/x11/mod.rs` (the exe-name fallback -> the identity), `shell2/windows/dnd.rs`
(`pub(crate)`), `loop_wakeup_invariants.rs` (`pub(crate)` helper), `dll/Cargo.toml` (one
feature), `core/src/notification.rs` (one doc comment),
`layout/tests/native_notifications.rs` (a module APPENDED at the end). Not touched:
`layout/src/e2e/` (E1), `page_breaks.rs`, `a_padded_table_cell_stays_in_its_row.rs`.

---

## 9. What is left

* **`AppConfig::app_id`**: section 3, the user's decision.
* **Portal extras**: the icon (a serialized `GIcon`), the `activation-token` in the portal's
  `ActionInvoked` platform data (Wayland raise inside Flatpak), sound / priority (portal v2).
* **Linux**: the backend's START still waits once (`GetServerInformation` + `GetCapabilities` at
  the first post, up to 3 s each when a daemon hangs); an unanswered `Notify` is noticed when the
  loop next wakes after 25 s (nothing wakes it for the timeout alone).
* **Windows**: elevated processes (COM refuses the activation; the Toolkit's HKLM `RunAs`
  workaround is not done); toast text-box input (`data`) is unused; in an MTA main thread
  `Activate` runs on an RPC thread and `loop_waker::wake` has no Windows wake - the event then
  waits for the next message (the class object is registered after `OleInitialize`, i.e. STA,
  unless something initialised the thread MTA first).
* **iOS**: the heuristic is unverified on a device (manual check iOS 4); the background-launch
  false positive (section 1.1).
* **Bundle**: Developer ID signing and notarization (documented), entitlements, a `--build` flag.
* **Twin**: `mobile::run::Target::resolve`'s bundle id (section 1.4).
* **E2E**: the `notification_event` op is E1's.
