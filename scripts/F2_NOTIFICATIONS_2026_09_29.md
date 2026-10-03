# F2 - notification follow-ups - 2026-09-29

Branch `wt/f2-notifications`, based on `282890483` (local tip of `fix/input-bugs-2026-09-19`, PR #476).
User instruction: "fix all follow ups recorded so far".

**Nothing here was compiled or run** (house rule: the parent compiles once). No api.json change.

---

## 1. Audit: the ledger's open list vs the code

The ledger's list was written before the wave-1 agent `notifications-gaps` (G1-G9) was
integrated. That agent's commits are in the base: `26c4fad24` (RED), `fc43076e5` (core + layout),
`817e98e07` (service, Apple, iOS), `3bce47136` (Android), `1bc3a1882` (Windows toasts),
`8a75ecaff` (Linux); api.json `4b1786f93`.

| # | item | before F2 | after F2 |
|---|---|---|---|
| 1 | macOS: unbundled apps report Unavailable; `azul-doc bundle macos` | STILL OPEN | **DONE** `a33e1365d` + `9dd190762` (the minimal version, see section 3) |
| 2 | macOS: UN delegate before the run loop | DONE `817e98e07` (`shell2/run.rs:960`, `:2926`; `notifications/apple.rs:386`; iOS `ios/mod.rs:1392`) | unchanged |
| 3 | macOS: permission state not persisted | DONE `817e98e07` (`apple.rs:269` probe reads the UN authorization; refreshed at launch, on activation `macos/mod.rs:2921`, and by the probe; `extra/permission/{macos.rs:109, ios.rs:54}`) | unchanged. Residual kept: the very first probe before the launch reading lands says "available (not read yet)" |
| 4 | Windows: registered toasts instead of NIF_INFO | DONE `1bc3a1882` (`notifications/windows.rs:537-851`, `dll/Cargo.toml:472-473`) | unchanged. The COM activator (a click after the app exited) is still open (section 6) |
| 5 | Linux: `desktop-entry` hint + Wayland activation token | **PARTIAL**: hint done; the token was captured (`linux.rs:388`) and `WaylandWindow::activate_with_token` existed (`wayland/mod.rs:10559`), but NOTHING took the token - the run.rs block that spent it went away when the loops moved onto `app_events` | **DONE** `89d51e923` + `0d5f2c94b` |
| 6 | Mobile: iOS / Android backend + pump, Android permission request, `onNewIntent` | DONE `3bce47136`, `817e98e07` (`notifications/android.rs`, `scripts/android/AzulNotifications.java`, `AzulActivity.java:144`, pumps `android/mod.rs:813`, `ios/mod.rs:1068`) | unchanged. Residuals are plans (section 6): iOS `launched_app`, the Android `requestPermissions` thread |
| 7 | Events for notifications this process did not post | DONE `fc43076e5` (`managers/notification.rs:370` route -> app handler; payload; `AppConfig::notification_handler`). **Residual bug**: a post the full queue rejected WITHOUT a callback went back through the mailbox, and routing handed its `Failed` to the live notification under the same id (ending it) or swallowed it as an ended id's echo | **DONE** `3c5fcd9dd` + `1b9d2f601` |
| 8 | Posts dropped with no window; a full queue drops silently | DONE for posts (`callbacks.rs:5523` `reject_notification`; `app_events.rs:147` parks deliveries via `defer_deliveries`). **Residuals**: a withdraw that met a full queue vanished; a backend failure's `Failed` was dropped silently when the mailbox was full | **DONE** `3c5fcd9dd` + `1b9d2f601` |
| - | (gaps report open item) macOS category race | open | **DONE** `5ce7e18b6` (Apple-only, manual check) |

AzClock alarms / scheduled notifications: NOT IN SCOPE (a separate feature).

## 2. Commits

| commit | kind | what |
|---|---|---|
| `36456d402` | chore | audit (first section of `scripts/F2_NOTIFICATIONS.PROGRESS.md`) |
| `3c5fcd9dd` | test RED | `layout/tests/native_notifications.rs`: `gaps::follow_ups` (4 tests) + `a_post_to_a_full_queue_without_a_callback_still_reports_failed` rewritten to the correct behaviour; `AppHandlerForThisTest` guard |
| `1b9d2f601` | fix | `layout/src/managers/notification.rs`: `reject_notification` delivers directly; withdraw headroom. `layout/src/callbacks.rs` doc. `dll/.../notifications/mod.rs`: warn when a `Failed` does not fit the mailbox |
| `89d51e923` | test RED | `dll/src/desktop/loop_wakeup_invariants.rs`: `a_notification_click_raises_the_wayland_window_with_its_activation_token` |
| `0d5f2c94b` | fix | `dll/src/desktop/app_events.rs::deliver_to_linux_windows` spends the token |
| `a33e1365d` | test RED | `doc/src/bundle.rs` (pure half stubbed with `unimplemented!`) + 11 unit tests; `pub mod bundle` in `doc/src/main.rs` |
| `9dd190762` | feat | `doc/src/bundle.rs` implemented + the command; `main.rs` dispatch (`["bundle", rest @ ..]`) and help; `apple.rs` unbundled reason names the command |
| `5ce7e18b6` | fix | `apple.rs`: a new category is read back before the request is added |
| progress commits | chore | `d2d924af4`, `8a8282102`, `30a7ef1cd`, `1787234c5`, `6c824f0ac`, `cdb5d09ce`, `09dbe77c6` |
| (this commit) | chore | this report |

### Expected RED -> green

| RED commit | test | today | after the fix |
|---|---|---|---|
| `3c5fcd9dd` | `gaps::a_post_to_a_full_queue_without_a_callback_still_reports_failed` | 0 deliveries (the `Failed` sits in the mailbox) | 1 delivery to the app handler, mailbox empty |
| | `gaps::follow_ups::a_rejected_post_does_not_end_the_live_notification_under_its_id` | `is_live("overflow-plain")` false (the live one got the `Failed` and ended) | still live; the `Failed` goes to the app handler |
| | `gaps::follow_ups::a_rejected_post_under_an_ended_id_still_reports_failed` | 0 deliveries (swallowed as the ended id's echo) | 1 |
| | `gaps::follow_ups::a_withdraw_still_reaches_the_platform_when_posts_filled_the_queue` | 256 requests | 257, the last is the withdraw |
| | `gaps::follow_ups::posts_cannot_take_the_room_withdraws_are_given_and_withdraws_are_bounded_too` | the first withdraw into a full queue fails | 256 withdraws fit, the 257th does not |
| `89d51e923` | `loop_wakeup_invariants::a_notification_click_raises_the_wayland_window_with_its_activation_token` | `expect` panics: no `take_activation_token()` in `deliver_to_linux_windows` | green |
| `a33e1365d` | `bundle::tests::*` (11) | panic `not implemented` | green |

Commands: `cargo test -p azul-layout --test all native_notifications`,
`cargo test -p azul-dll --lib loop_wakeup_invariants`, `cargo test -p azul-doc --bin azul-doc bundle::`.

## 3. What changed, per item

* **7/8 (engine).** `reject_notification` sends the `Failed` of a post that did not fit to its
  owner as a waiting delivery: the post's own callback, else the app-level handler, else nobody.
  It never goes through the mailbox, because routing takes the id for whatever an EARLIER post
  under that id left. `try_push_notification_request`: posts stop at `MAX_QUEUED_REQUESTS` (and
  report `Failed`). Withdraws may use as many slots again, because a withdraw has no event to
  report a refusal through. It is still bounded (512 in all). dll: a backend failure whose
  `Failed` does not fit the mailbox is logged.
* **5 (Linux).** `deliver_to_linux_windows` takes `notifications::take_activation_token()`
  right after `AppEvents::collect()`, whose D-Bus drain dispatched the `ActivationToken` signal.
  It runs when there is a token even if no callback was collected: a click on a callback-less
  notification still raises the app. On a Wayland target it calls `activate_with_token` BEFORE
  the callbacks. X11 drops the token. The token is never left behind for a later, unrelated
  delivery.
* **1 (macOS bundling), the minimal version.**
  * `azul-doc bundle macos <crate>` resolves the crate like `mobile build`: a Cargo.toml, a
    directory, or `examples/<name>`.
  * It bundles the ALREADY BUILT `target/<profile>/<bin>`:
    * `CARGO_TARGET_DIR` is honoured;
    * profile flags: `--release` (default), `--debug`, `--profile`;
    * `--bin` and `--exe` pick another binary;
    * a missing binary prints the exact `cargo build` line.
  * It writes `<out>/<Name>.app`:
    * default `~/Applications`, fresh on every run;
    * `/var/folders` is refused (UN refuses bundles there).
  * Contents: `Contents/MacOS/<exe>`, `Info.plist`, `PkgInfo`.
    * `Info.plist` holds `CFBundleIdentifier` (default `com.azul.<crate>`, `--bundle-id` to
      override), `CFBundleExecutable`, `CFBundleName`/`DisplayName`, `APPL`, the version from
      `[package]` (numeric part), `InfoDictionaryVersion` and `NSHighResolutionCapable`.
  * Dylibs, from `otool -L`: every `@rpath/...` reference and every absolute path inside
    `target/` is copied to `Contents/Frameworks/`, and the binary is relinked to
    `@executable_path/../Frameworks/<name>`.
    * AzWidgets links `target/release/build/azul-dll-*/out/libazul.dylib` by absolute path; I
      checked the local build with `otool`.
    * A relink that does not fit the header is a warning: the original path still loads on this
      machine.
  * Signing: `codesign --force --sign -` on each dylib, then on the bundle. Inside out, no
    `--deep`.
  * Registration: `lsregister -f`. A failure is a warning; `--no-register` skips it.
  * `--dry-run` prints the plan on any host.
  * The unbundled "Unavailable" reason now ends with "`azul-doc bundle macos <crate>` builds one
    in ~/Applications".
* **Category race (macOS).** `ensure_category` reports whether it registered the category just
  now.
  * If it did, `add_request` retains the request, calls
    `getNotificationCategoriesWithCompletionHandler:`, and adds the request inside that handler.
  * `add_now` is factored out.
  * A known button set is added at once, as before.

## 4. API

**No api.json change.** No public type or function of `azul-core`, `azul-layout` or `azul-dll`
was added or changed in shape. The behaviour changes are inside existing functions
(`reject_notification`, `try_push_notification_request`, `CallbackInfo::withdraw_notification`
docs).

Rust-only additions, all in the `azul-doc` binary crate (`doc/src/bundle.rs`, not api.json):

* types: `MacBundleSpec`, `BundlePaths`, `BundledDylib`;
* functions: `info_plist`, `bundle_version`, `bundle_id_for`, `package_version`, `plan_dylibs`,
  `relinked_reference`, `is_refused_location`, `bundle_paths`, `handle_bundle_command`.

## 5. Least sure to compile

1. **`apple.rs` `add_request` / `add_now`** (macOS + iOS):
   * `Retained::retain(request)` (objc2 0.6, an `unsafe fn`, called inside the existing
     `unsafe` block);
   * the `RcBlock::new(move |_categories: *mut AnyObject| ..)` closure capturing a
     `Retained<AnyObject>`, assumed fine because block2 asks only for `Fn + 'static`;
   * `add_now(center, &request, ..)` relying on deref coercion `&Retained<AnyObject>` ->
     `&AnyObject`;
   * `msg_send![center, addNotificationRequest: request, ..]` with `request: &AnyObject` (it was
     `*mut AnyObject`);
   * `return add_now(center, &*request, ..)` from inside `unsafe { }` in a `()` fn.
2. **`doc/src/bundle.rs`**:
   * `line.starts_with(char::is_whitespace)`;
   * `[..].iter().any(|prefix| reference.starts_with(prefix))` (a `&&str` pattern);
   * `String == &str` in the dedupe;
   * the `value` closure in `parse_args` borrowing `args` while `a` is mutated;
   * `format!` mixing the captured `{flag}` with positional `{}`;
   * `crate::mobile::Opts::default()` and `crate::mobile::run::Target::resolve` (both `pub`);
   * `run(Command::new(..).arg(..))` on a temporary;
   * `format!(r#"..."#)` with the captured `{name}`/`{executable}`/`{id}`/`{version}`.
3. **`layout/tests/native_notifications.rs`**: the nested `gaps::follow_ups` module imports its
   grandparent's private items (`use super::super::{notification_with_callback, s, serial,
   Seen}`) and its parent's private helpers (`use super::{post_a_plain_overflow, run_callback,
   AppHandlerForThisTest}`). Both are legal paths into ancestors.
4. **`app_events.rs`**: `activation_token.as_deref()` into `w.activate_with_token(token)` in the
   `LinuxWindow::Wayland(w)` arm. On an X11-on-macOS build (`az_x11`, not Linux) the variable is
   used only in the early-return check.
5. **`managers/notification.rs`**: `match &request` for the bound; the nested match on
   `app_notification_handler()` moving the handler out.

## 6. Manual checks

Never drive the real Mac's input while the user works (memory rule). These are for a person.

### macOS - the bundle step (item 1)

```sh
cargo build --release -p AzWidgets
cargo run --release -p azul-doc -- bundle macos azul-widgets --dry-run
#   [bundle] .../target/release/AzWidgets -> ~/Applications/AzWidgets.app
#   [bundle]   CFBundleIdentifier com.azul.azwidgets
#   [bundle]   .../out/libazul.dylib -> Contents/Frameworks/libazul.dylib
cargo run --release -p azul-doc -- bundle macos azul-widgets
ls -R ~/Applications/AzWidgets.app/Contents          # Info.plist PkgInfo MacOS/AzWidgets Frameworks/libazul.dylib
plutil -lint ~/Applications/AzWidgets.app/Contents/Info.plist        # OK
codesign --verify --strict --verbose=2 ~/Applications/AzWidgets.app  # valid on disk, satisfies its Designated Requirement
codesign -dv ~/Applications/AzWidgets.app 2>&1 | grep Signature      # Signature=adhoc
otool -L ~/Applications/AzWidgets.app/Contents/MacOS/AzWidgets       # @executable_path/../Frameworks/libazul.dylib
/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister -dump | grep -B2 -A2 com.azul.azwidgets
open ~/Applications/AzWidgets.app
```

1. In the app, go to "Notifications" -> "Platform support". Expected: *Available -
   UNUserNotificationCenter*.
2. Post a notification. Expected: the permission prompt, then Allow, then the banner. Then the
   whole macOS list of `NOTIFICATIONS_GAPS_FIX_2026_09_28.md` section 4 applies (launch tap,
   `launched_app`, permission truth, request API).
3. Run the command twice. Expected: no error; a fresh bundle.
4. Run with `--out /var/folders/x`. Expected: refused, with the reason.
5. Run with the binary not built. Expected: the `cargo build --release -p AzWidgets` line.
6. Run the unbundled `target/release/AzWidgets`. Expected: "Platform support" ends with
   "`azul-doc bundle macos <crate>` builds one in ~/Applications".
7. Stale denial: `tccutil reset UserNotification com.azul.azwidgets`.

### macOS - category race

1. Bundle with a bundle id that has never posted before, for example
   `bundle macos azul-widgets --bundle-id com.azul.azwidgets.f2`.
2. Launch it, allow notifications, and post ONCE.
3. Expected: the FIRST banner already offers "Show me" in its options. Before this fix, the first
   banner of a new button set could come without it. The race was not reproduced here, so an
   "always had buttons" result does not prove the fix, but a missing button on the first post
   would disprove it.

### Linux - activation token (item 5)

1. Run `AZ_WINDOW=wayland AZ_LOG=debug target/release/AzWidgets` under GNOME or KDE Wayland.
   Expected startup log: `[Wayland] Bound xdg_activation_v1`.
2. Run `dbus-monitor "interface='org.freedesktop.Notifications'"` in a second terminal.
3. Post, put another window in front of azul's, and click the notification body.
   Expected:
   * the monitor shows `ActivationToken` and then `ActionInvoked(.., "default")`;
   * **the azul window comes to the front**;
   * "Last event" reads *Clicked*;
   * no "could not be spent" debug line.
4. Repeat with a notification that has no callback (with or without an app-level handler).
   Expected: the window still comes to the front.
5. X11 session: unchanged. No raise from the token (it is dropped).

### Everywhere (headless-tested; optional live check)

* Post 257 notifications without a callback from one callback, with an app-level handler set.
  Expected: the handler gets one `Failed` for the 257th. The notification already on screen
  under that id (if any) stays and keeps its callback.
* Post 256, then withdraw one that is on screen in the same callback. Expected: it goes away.

## 7. What is left (not done here, with the plan)

* **iOS `launched_app`** stays false: UN gives iOS no launch marker for LOCAL notifications.
  * Plan: an `AtomicBool EVER_ACTIVE`, set by the iOS app delegate's
    `applicationDidBecomeActive`. `apple.rs::handle_response` (iOS only) marks the FIRST
    response `launched_app = true` if it arrives while `EVER_ACTIVE` is false.
  * Verify first, on a device, that a cold-launch tap delivers `didReceiveNotificationResponse`
    BEFORE the first `applicationDidBecomeActive`. Otherwise the heuristic is wrong.
* **Android `requestPermissions` thread**: it is called from `android_main`'s thread
  (`extra/permission/android.rs:225`). It works on the devices tried so far ("works in theory").
  * Plan, only if a device refuses: a static `AzulPermissions.request(Activity, String[], int)`
    that calls `activity.runOnUiThread(() -> activity.requestPermissions(..))`.
  * Call it through the ACTIVITY's class loader (the `with_helper` pattern of
    `notifications/android.rs`). `find_class` on a native-attached thread cannot see app
    classes.
* **Windows COM activator**: a click on an Action Center entry after the app exited.
  * Plan, registry side: register a CLSID under HKCU
    `Software\Classes\CLSID\{guid}\LocalServer32 = "<exe>" -ToastActivated`, and a
    `CustomActivator` value `{guid}` under the AUMID key.
  * Plan, code side: implement `INotificationActivationCallback` with `windows::core::implement`,
    `CoRegisterClassObject` at startup, and map `Activate(aumid, args, ..)` through
    `wire::toast_activated_event` with `launched_app = true` into the mailbox.
  * Needs the `windows` feature that carries `INotificationActivationCallback` (check the 0.62
    feature name). Not testable here.
* **`AppConfig::app_id`** (research step 1): one reverse-DNS id driving the Windows AUMID, the
  `desktop-entry` hint, the Wayland `app_id`, the Android channel prefix and the bundle step's
  default `--bundle-id`.
  * It is an ABI change of `AppConfig`: a field placed to keep
    `app_config_has_no_padding_between_its_fields` green, plus `set_app_id(&mut self, id:
    String)` and `with_app_id(self, id: String) -> AppConfig` in api.json.
  * The precedence against the bundle's own `CFBundleIdentifier` (which the OS owns on macOS and
    iOS) needs a decision. Not started.
* **E2E**: no op injects a notification event, and `assert_notification` has no `payload?`.
  * Plan: a `notification_event` op (`id`, `kind`, `action?`, `payload?`, `launched_app?`)
    queuing into the mailbox; `payload?` on `assert_notification` (`layout/src/e2e/full.rs`,
    gen-e2e discovers it).
  * Deferred because the main checkout has uncommitted `layout/src/e2e/runner.rs` work from
    another session.
* **Bundle step extras**: an `.icns` icon, a `--build` flag that runs cargo first, the bundled
  dylibs' own non-system dependencies (not recursive today), Developer ID signing, and
  notarization.
* **Linux G6 rest**: a portal backend (Flatpak), async `Notify` (it still blocks up to 3 s),
  and a `NameOwnerChanged` watch.
* **Model (G8)**: channel / importance, reply text, and the per-platform meaning of `icon`.
* **macOS first probe**: it says "available (not read yet)" until the launch-time authorization
  reading lands.
* The widgets demo does not use the app-level handler or the payload yet.
* Done elsewhere since the gaps report: the tray's `drain_tray_events()` now has a caller
  (loop-wakeups, `take_tray_deliveries`; pinned by `plain_tray_clicks_reach_the_app`).
