# Native notifications - crates, platform rules, own-vs-depend, plan, gaps - 2026-09-28

Research for the user's question: *"research how notifications are done ... For notifications:
very important on mobile + arent there already crates for notifications? Or it would maybe be
better to own this and do it with the dylib dlopen pattern."* Addendum: *"how do do things like
creating cron jobs (on the web emulated with timers): but necessary for AzClock, i.e. for
registering a "alarm clock" callback? or integrating with "native" notifications / alarms?"*

Reviewed code: the native-notifications work now on `fix/input-bugs-2026-09-19` (HEAD
`e0009e01e`, cherry-picked from `wt/native-notifications`; design doc
`scripts/NATIVE_NOTIFICATIONS_2026_09_28.md`). Nothing was compiled for this report.

Source caveat: github.com was unreachable from the research sandbox. Crate facts were verified
against the published `.crate` sources (docs.rs source view) and raw.githubusercontent.com; claims
that rest only on a search-engine summary of a GitHub issue are marked **[snippet]**.

---

## 0. Recommendation

**Own it, on every platform. Do not add a notification crate.** No crate covers iOS + Android
without a Kotlin/Swift build system (Gradle / SwiftPM / `swiftc` in build.rs), and the desktop
crates would replace code azul already has with worse behaviour: the default macOS path
impersonates Finder/Terminal, the default Windows path impersonates PowerShell.

| platform | keep / change | mechanism |
|---|---|---|
| macOS | keep UN; fix delegate timing, the permission probe and the request API; ship a bundling step | `UNUserNotificationCenter` through the objc2 runtime and a dlopen'd framework, as now. Unbundled stays "unavailable" (it would abort). |
| iOS | NEW, but it is the macOS file | the same UN module compiled for `ios`; delegate installed in `did_finish_launching`; pumped from `display_tick` |
| Android | NEW | framework classes over JNI (`NotificationManager`, `Notification.Builder`, `NotificationChannel`, `PendingIntent`), plus one small Java helper in the APK's existing javac/d8 dex. Needed for: a `BroadcastReceiver` (dismissals, background buttons, scheduled alarms) and `AzulActivity.onNewIntent` + `launchMode="singleTop"` (warm taps). Permission through the existing `Capability::Notifications -> POST_NOTIFICATIONS` backend. |
| Windows | REPLACE the balloon as the primary backend | WinRT toast through the `windows` 0.62 crate azul already depends on (two more feature flags). AUMID self-registered under HKCU at first run, no installer. In-process `Activated`/`Dismissed`/`Failed` first; a COM activator (clicks after exit) second. The balloon stays as the fallback. |
| Linux | keep libdbus `org.freedesktop.Notifications` | add `desktop-entry`, `ActivationToken` (Wayland focus), async `Notify`; add the portal (`org.freedesktop.portal.Notification`, over the zbus the repo already uses for portals) for Flatpak only |
| web | later | browser `Notification` API; only while the page is open |

API changes needed before the api.json sync freezes the ABI:
* an app-level notification handler, for events whose notification's callback died with the
  process (the normal case on mobile);
* a `payload` string that survives process death;
* a channel/importance;
* an explicit permission request;
* a schedule (section 6).

**Alarms (AzClock), section 6.**
* **The OS owns the schedule where it can:**
  * Apple UN calendar triggers; iOS 26 AlarmKit rings like the Clock app, but it is Swift-only;
  * Android `AlarmManager.setAlarmClock`, with a Java receiver that posts by itself and re-arms
    after reboot;
  * Windows scheduled toasts, pre-scheduled as individual occurrences.
* **Linux and the web have no OS scheduler.** An alarm there only fires while the app (or tab) is
  running.
* **In-process, azul needs a WALL-CLOCK alarm.** Today's `Timer` is monotonic, does not count
  sleep, polls every 10 ms until a delay elapses, and is not driven on the web at all.

---

## 1. Existing crates

Versions and dates are from the crates.io API on 2026-09-28.

| crate | version / date | license | platforms | how it links | weight | buttons | click / close back to the app | unbundled mac / unpackaged win | maintenance |
|---|---|---|---|---|---|---|---|---|---|
| **notify-rust** | 4.18.1 / 2026-09-27 | MIT/Apache | Linux+BSD, macOS, Windows | Linux: zbus (default) or dbus-rs (build-time libdbus via pkg-config). mac: mac-notification-sys. Win: tauri-winrt-notification | zbus ~330K SLoC (already in azul's Linux graph) | yes | Linux: `ActionInvoked` + `NotificationClosed`; mac/Win: BLOCKING `wait_for_action` / `on_close` | mac: swizzled bundle id (Finder/Terminal). Win: PowerShell AUMID by default | very active |
| **mac-notification-sys** | 0.6.15 / 2026-06-16 | MIT/Apache | macOS | build.rs compiles `objc/notify.m` with `cc`, links AppKit | small | 1 button or a dropdown (private KVC keys) | blocking; dismissal found by polling | NSUserNotification + process-wide swizzle of `-[NSBundle bundleIdentifier]`, default `com.apple.Finder` via an AppleScript lookup | active; never moved to UN |
| mac-usernotifications | 0.3.1 / 2026-06-12 | MIT/Apache | macOS | objc2-user-notifications (framework linked at load time) | objc2 + futures-lite | yes, reply | async + blocking, close reason, update in place | needs a signed `.app`; offers `check_bundle()` | new (May 2026) |
| **tauri-winrt-notification** | 0.8.1 / 2026-07-17 | MIT/Apache | Win 8.1-11 | `windows` 0.62 (same as azul) | windows-version, thiserror | yes | in-process `on_activated` / `on_dismissed`; no `Failed`; no remove/history | caller's AUMID; ships `POWERSHELL_APP_ID` | active (tauri org) |
| winrt-notification | 0.5.1 / 2022-01-11 | MIT | Win | windows 0.24 | small | no | no | PowerShell | abandoned |
| win-toast-notify | 0.1.6 / 2024-08-01 | MIT | Win | spawns `powershell.exe` | tiny | URL buttons | no | PowerShell | stale |
| winrt-toast-reborn | 0.3.8 / 2025-09-01 | MIT | Win 10/11 | windows 0.61 + registry | small | yes | Activated / Dismissed / Failed | `register()` writes HKCU AUMID | low activity |
| **win32_notif** | 0.15.3 / 2026-06-08 | Apache-2.0 | Win | windows 0.62 + windows-registry | moderate | actions, inputs, progress | foreground events; COM activator is a stub that prints | refuses unpackaged without an AUMID; HKCU registration builder incl. `CustomActivator`; history remove/clear | active |
| user-notify (DeltaChat) | 0.4.2 / 2026-01-23 | **LGPL-3.0+** (azul is MIT) | mac (UN), Linux, Win | objc2-user-notifications, windows 0.61 | full tokio | mac only | UN delegate; Win protocol deep links | mac unbundled: silently a MOCK | 2 versions |
| **tauri-plugin-notification** | 2.5.0 / 2026-09-26 | MIT/Apache | desktop (via notify-rust), Android (Kotlin), iOS (Swift) | Gradle builds the Kotlin, SwiftPM the Swift; needs the Tauri runtime | all of tauri | desktop: ignored; mobile: yes | desktop: none; mobile: events to JS listeners only, no Rust listener | dev: `com.apple.Terminal` / PowerShell | active |
| objc2-user-notifications | 0.3.2 / 2025-10-04 | Zlib/Apache/MIT | macOS, iOS, ... | unconditional `#[link(name="UserNotifications", kind="framework")]` | objc2 stack (already used) | raw bindings | you write the delegate | UN itself needs a bundle | active |
| ashpd | 0.13.13 / 2026-07-17 | MIT | Linux portal | zbus, default tokio | moderate | portal buttons | `ActionInvoked` stream; the portal has no closed signal | - | active |
| waterkit-notification | 0.1.4 / 2026-09-17 | MIT/Apache | 5 platforms | swiftc + kotlinc + d8 in build.rs | heavy | - | Android click dispatch is a no-op | UN if bundled | new |

Details that decide the question:

* **notify-rust**
  * Opens a NEW session-bus connection per notification (`zbus::Connection::session()` in
    `xdg/zbus_rs.rs`).
  * Never handles `ActivationToken` and has no portal support.
  * Its default macOS path is the deprecated NSUserNotification stack ("deprecated on macOS 14+,
    but still works", its own source), and its UN path panics when UN refuses delivery.
  * Windows defaults to `app_id.unwrap_or(Toast::POWERSHELL_APP_ID)`.
  * <https://docs.rs/crate/notify-rust/4.18.1/source/src/xdg/zbus_rs.rs>,
    <https://docs.rs/crate/notify-rust/4.18.1/source/src/windows.rs>
* **mac-notification-sys**
  * `send_notification` always calls `ensure_application_set()`, so even a BUNDLED app is
    relabelled Finder unless it calls `set_application(own_id)` first.
  * The swizzle is process-wide, so it would also change the bundle id azul's own
    `bundle_status()` reads.
  * It takes the single `NSUserNotificationCenter.delegate` slot and does not implement
    `shouldPresentNotification:`, so a frontmost app gets no banner.
  * It needs a C compiler plus the macOS SDK at build time.
  * <https://docs.rs/crate/mac-notification-sys/0.6.15/source/objc/notify.h>,
    <https://docs.rs/crate/mac-notification-sys/0.6.15/source/build.rs>
* **The Windows crates** are thin wrappers over the same `windows` 0.62 bindings azul already
  compiles.
  * None of them ships a working COM activator: win32_notif's `Activate` does
    `println!("Called")`.
  * The `ToastNotification.Activated` event only fires while the process runs: *"Apps that are
    running subscribe to this event"*.
  * <https://learn.microsoft.com/en-us/uwp/api/windows.ui.notifications.toastnotification.activated>
* **Windows App SDK `AppNotificationManager`** has no usable Rust binding. `microsoft/windows-app-rs`
  was archived in 2022, and the crates.io `windows-app` versions are yanked. Unpackaged use also
  needs the runtime installed, and it depends on the Singleton MSIX package even when
  self-contained (section 2.2).
* **tauri-plugin-notification's mobile side** (for the Android/iOS design, not as a dependency):
  * Android:
    * A `"default"` channel is created on load.
    * Taps and actions use `PendingIntent.getActivity` with extras.
    * Dismissals go to a manifest `BroadcastReceiver`; schedules use `AlarmManager` plus a
      `BOOT_COMPLETED` restore receiver.
    * A warm tap arrives through `onNewIntent`.
  * iOS: `center.delegate = self`, `requestAuthorization([.badge,.alert,.sound])`, and the
    delegate handles `willPresent` / `didReceive`.
  * <https://docs.rs/crate/tauri-plugin-notification/2.5.0/source/android/src/main/java/NotificationPlugin.kt>,
    <https://docs.rs/crate/tauri-plugin-notification/2.5.0/source/ios/Sources/NotificationManager.swift>
* **objc2-user-notifications** proves UN from Rust needs no Swift, even on iOS
  (`bevy_ios_notifications` implements the delegate with `define_class!`). Its unconditional
  framework link is harmless on iOS, where UN has existed since iOS 10. azul's runtime `msg_send!`
  code already does the same job without the link.

---

## 2. Platform requirements

### 2.1 macOS

* **Unbundled = abort, confirmed.**
  * `+[UNUserNotificationCenter currentNotificationCenter]` in a bare executable fails an
    assertion at `UNUserNotificationCenter.m:44`.
  * It then throws an uncaught `NSInternalInconsistencyException` "bundleProxyForCurrentProcess is
    nil: mainBundle.bundleURL file:///.../", followed by SIGABRT. It does not return nil or an
    error.
  * The check is a LaunchServices lookup, not an Info.plist check.
  * <https://developer.apple.com/forums/thread/724249>,
    <https://developer.apple.com/forums/thread/679326>,
    <https://developer.apple.com/forums/thread/133303>,
    <https://raw.githubusercontent.com/go-macos/usernotifications/main/README.md>
* **An embedded Info.plist (`-sectcreate __TEXT __info_plist`) is not a known fix.** It gives
  `NSBundle.mainBundle.bundleIdentifier` a value, but `bundleURL` stays the directory and nothing is
  registered with LaunchServices. It would defeat a guard that only checks `bundleIdentifier`;
  azul's guard also checks `bundlePath` ends in `.app` (`notifications/macos.rs:203`), which is the
  correct form. Unverified either way; a 5-minute local test settles it.
* **Three conditions, measured by go-macos on macOS 15:**

  | missing | result |
  |---|---|
  | a bundle identifier | SIGABRT |
  | a code signature (ad-hoc is enough) | `UNErrorDomain` 1, "Notifications are not allowed for this application" |
  | LaunchServices registration | the same error 1 |

  * The linker's automatic ad-hoc signature is not enough: the assembled bundle must be re-signed
    with `codesign --force --sign - X.app` **[snippet]**.
  * A bundle under `/var/folders` is registered but refused; the same bundle under
    `~/Library/Application Support` works.
  * `open` is not required. Running `Contents/MacOS/<exe>` works once the bundle is registered
    (Finder, the Applications-folder scan, or `LSRegisterURL`).
  * A stale "deny" is keyed on the bundle id: reset it with `tccutil reset UserNotification <id>`
    **[snippet]**.
  * <https://docs.rs/mac-usernotifications/latest/mac_usernotifications/>,
    <https://docs.deno.com/runtime/desktop/notifications/>,
    <https://raw.githubusercontent.com/electron/electron/main/docs/tutorial/notifications.md>
* **Minimal `.app`:** `X.app/Contents/Info.plist` with `CFBundleIdentifier`,
  `CFBundleExecutable`, `CFBundlePackageType=APPL`, `CFBundleName`, `CFBundleVersion` and
  `CFBundleShortVersionString`, plus `X.app/Contents/MacOS/<exe>` (and `Resources/*.icns`). Sign
  inner dylibs first, then the bundle, with `codesign --force --sign -`. No entitlements are needed
  for local notifications; push, critical alerts and time-sensitive need entitlements.
  * <https://developer.apple.com/library/archive/documentation/CoreFoundation/Conceptual/CFBundles/BundleTypes/BundleTypes.html>,
    <https://raw.githubusercontent.com/julienXX/terminal-notifier/master/Makefile>
* **The alternatives are dead ends.**
  * NSUserNotificationCenter:
    * Deprecated since macOS 11.
    * Does not work from a Foundation tool.
    * The bundle-id swizzle is a private-method override (not allowed by App Store rules). It shows
      the impersonated app's icon, and a click LAUNCHES that app.
    * Breakage reports exist on Sonoma, Sequoia and macOS 26 **[snippet]**.
    * terminal-notifier 3 itself moved to UN and dropped `-sender`.
  * `osascript display notification` is attributed to Script Editor, a click opens Script Editor,
    and nothing comes back to the app.
  * <https://developer.apple.com/documentation/foundation/nsusernotificationcenter>,
    <https://developer.apple.com/library/archive/documentation/LanguagesUtilities/Conceptual/MacAutomationScriptingGuide/DisplayNotifications.html>
* **API facts the backend must honour:**
  * Assign the delegate "before your app finishes launching", or the launching response is
    missed. It is a WEAK property; azul leaks the delegate on purpose, which is correct.
  * Without `willPresent`, a frontmost app shows nothing.
  * `UNNotificationDismissActionIdentifier` needs the category option `customDismissAction`, and a
    flicked-away banner never reports.
  * A click with the app not running relaunches it and calls `didReceive`. At launch,
    `NSApplicationLaunchUserNotificationKey` holds a `UNNotificationResponse`.
  * `provisional` authorization is granted silently (quiet delivery).
  * `.alert` is deprecated; use `.banner | .list`.
  * <https://developer.apple.com/documentation/usernotifications/unusernotificationcenterdelegate>,
    <https://developer.apple.com/documentation/usernotifications/unusernotificationcenterdelegate/usernotificationcenter(_:willpresent:withcompletionhandler:)>,
    <https://developer.apple.com/documentation/usernotifications/unnotificationdismissactionidentifier>,
    <https://developer.apple.com/documentation/usernotifications/handling-notifications-and-notification-related-actions>

### 2.2 Windows

* **The balloon (what the agent shipped).**
  * On Windows 10 a balloon becomes a banner that stays in Notification Center until dismissed.
  * **On Windows 11, a banner that times out is NOT kept in Notification Center**
    (Shell_NotifyIconW remarks).
  * A notification-area icon must exist while it shows; only one balloon shows at a time.
  * `NIN_BALLOONHIDE` is sent when the icon is deleted, not on a timeout or a click.
  * No buttons; 64-char title and 256-char text.
  * The Group Policy `EnableLegacyBalloonNotifications` turns balloons back into legacy balloons.
  * Attribution comes from the process: set `FileDescription` in the version resource.
  * <https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shell_notifyiconw>,
    <https://learn.microsoft.com/en-us/windows/win32/api/shellapi/ns-shellapi-notifyicondataw>,
    <https://admx.doctool.app/windows-11/user/startmenu/enablelegacyballoonnotifications>
* **WinRT toast for an unpackaged exe: the options.**
  * **(a) A Start-menu `.lnk`** with `System.AppUserModel.ID`, written at runtime to `%APPDATA%`
    (no admin needed). For activation, add `ToastActivatorCLSID`.
    <https://learn.microsoft.com/en-us/windows/win32/shell/enable-desktop-toast-with-appusermodelid>
  * **(b) Registry only: the one to use.**
    * Write `HKCU\Software\Classes\AppUserModelId\<AUMID>` with `DisplayName`, `IconUri`,
      `IconBackgroundColor`, and optionally `CustomActivator={CLSID}`.
    * For clicks after exit, also write `HKCU\Software\Classes\CLSID\{CLSID}\LocalServer32 =
      "<exe>" -ToastActivated` and implement `INotificationActivationCallback` via
      `CoRegisterClassObject(CLSCTX_LOCAL_SERVER, REGCLS_MULTIPLEUSE)`.
    * Community Toolkit 7 (`ToastNotificationManagerCompat`), Firefox portable and the Windows App
      SDK all do exactly this at runtime, with no installer.
    * Two traps: AUMIDs with `\` break on Windows 10 builds up to 19042, and AUMIDs over 129 chars
      must be hashed.
    * The Microsoft page documenting (b) has been removed. The Windows App SDK depends on it and
      supports 1809+.
    * <https://raw.githubusercontent.com/CommunityToolkit/WindowsCommunityToolkit/main/Microsoft.Toolkit.Uwp.Notifications/Toasts/Compat/ToastNotificationManagerCompat.cs>,
      <https://raw.githubusercontent.com/mozilla-firefox/firefox/main/widget/windows/ToastNotification.cpp>,
      <https://raw.githubusercontent.com/microsoft/WindowsAppSDK/main/dev/AppNotifications/AppNotificationUtility.cpp>
  * **(c) Windows App SDK `AppNotificationManager`:** needs the runtime installed plus the
    bootstrapper, and depends on the Singleton package even when self-contained. Not viable for a
    bare exe.
    <https://raw.githubusercontent.com/MicrosoftDocs/windows-dev-docs/docs/hub/apps/package-and-deploy/self-contained-deploy/deploy-self-contained-apps.md>
  * **(d) Borrowed PowerShell AUMID:** attributed to PowerShell, and a click after exit opens a
    PowerShell window **[snippet]**.
  * **(e) `SetCurrentProcessExplicitAppUserModelID` + (b):** the Windows App SDK honours the
    process's explicit AUMID and registers it without a shortcut, which is the best evidence this
    combination works. It also changes taskbar grouping, so set it at startup, before the first
    window.
* **What needs the COM activator.** Toasts show without one, and the in-process
  `Activated`/`Dismissed`/`Failed` events work while the process lives. Anything after exit needs
  it: Action Center clicks, buttons and replies. Also:
  * There are reports of Action Center clicks not firing `Activated` without a COM server even
    while the app runs **[snippet]**.
  * Activation fails for ELEVATED processes; the Toolkit's workaround is HKLM `RunAs=Interactive
    User`.
  * <https://learn.microsoft.com/en-us/previous-versions/windows/desktop/win32_tile_badge_notif/respond-to-toast-activations>,
    <https://learn.microsoft.com/en-us/archive/blogs/tiles_and_toasts/quickstart-handling-toast-activations-from-win32-apps-in-windows-10>
* **Raw WinRT without windows-rs is possible** (WinToast `LoadLibrary`s combase for
  `RoGetActivationFactory`; there are plain-C gists), but unnecessary: azul already compiles
  windows-rs WinRT for SMTC. The IIDs, if ever needed:
  * `IToastNotificationManagerStatics` 50AC103F-D235-4598-BBEF-98FE4D1A3AD4
  * `IToastNotificationFactory` 04124B20-82C6-4229-B109-FD9ED4662B53
  * `IXmlDocumentIO` 6CD0E74E-EE65-4489-9EBF-CA43E87BA637
  * `INotificationActivationCallback` 53E31837-6600-4A81-9395-75CFFE746F94
  * <https://raw.githubusercontent.com/mohabouje/WinToast/master/src/wintoastlib.cpp>

### 2.3 Linux

* **Spec 1.3** (2024-08-18; the only change from 1.2 was new `call.*` categories). `ActivationToken`
  was added in 2021 under 1.2.
  * `Notify(susssasa{sv}i) -> u`, `CloseNotification`, `GetCapabilities`, `GetServerInformation`.
  * `NotificationClosed` reasons: 1 expired, 2 dismissed, 3 closed by call, 4 undefined.
  * `ActionInvoked`; `ActivationToken(u, s)`, which arrives BEFORE `ActionInvoked`.
  * Capabilities include `actions`, `persistence`, `sound`, `body-markup`, `icon-static`, etc.
  * The body click is the `"default"` action key.
  * Hints: `urgency` (byte 0/1/2), `category`, `desktop-entry`, `image-path`, `image-data`,
    `sound-name`, `suppress-sound`, `transient`, `resident`.
  * <https://specifications.freedesktop.org/notification/latest/protocol.html>,
    <https://specifications.freedesktop.org/notification/latest/hints.html>
* **GNOME Shell**
  * How it finds the app: the sender PID → its window → the `.desktop`, then the `desktop-entry`
    hint, then `app_name`. With no match, the notification gets a generic source with no per-app
    settings.
  * With a `"default"` action (azul always sends one), a click emits `ActivationToken` then
    `ActionInvoked`.
  * When a matched sender's bus name vanishes, GNOME DESTROYS its notifications (close reason 3).
    Unmatched ones stay, and clicks go to a dead name.
  * KDE adds `inline-reply` (`NotificationReplied`), derives the desktop entry from the PID, and
    emits `ActivationToken`.
  * <https://github.com/GNOME/gnome-shell/blob/main/js/ui/notificationDaemon.js>,
    <https://invent.kde.org/plasma/plasma-workspace/-/blob/master/libnotificationmanager/server_p.cpp>
* **Portal `org.freedesktop.portal.Notification`**
  * Interface: `AddNotification(id, a{sv})`, `RemoveNotification`, and
    `ActionInvoked(id, action, av)`, whose platform data carries `activation-token`. v2 adds
    `display-hint`, `category`, `sound` and `buttons[].purpose`.
  * **There is no closed signal.**
  * Needed in Flatpak, which filters the session bus. Snap allows `Notify` but does not pass
    `ActivationToken`.
  * For UNSANDBOXED apps the GNOME backend silently drops notifications whose app id has no
    `.desktop` file. So: talk to `org.freedesktop.Notifications` directly when unsandboxed, and use
    the portal only inside Flatpak.
  * <https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Notification.html>,
    <https://docs.flatpak.org/en/latest/sandbox-permissions.html>,
    <https://gitlab.gnome.org/GNOME/xdg-desktop-portal-gnome/-/blob/main/src/notification.c>
* **Wayland focus.** A notification click carries no input serial, so the `ActivationToken` (or
  the portal's `activation-token`) is the only legitimate way to raise the window through
  `xdg_activation_v1.activate(token, surface)`. On X11 the same string is a startup id.
  <https://wayland.app/protocols/xdg-activation-v1>

### 2.4 iOS

* Same framework and API as macOS (iOS 10+ / macOS 10.14+). The differences:
  * the app is always bundled;
  * `.banner`/`.list` (iOS 14+);
  * `setBadgeCount` (iOS 16+).
* **Authorization:** `requestAuthorizationWithOptions:` prompts once. Option bits: Badge 1<<0,
  Sound 1<<1, Alert 1<<2, Provisional 1<<6 (quiet delivery with no prompt). Read the state with
  `getNotificationSettingsWithCompletionHandler:`. No Info.plist usage string and no entitlement
  are needed for local notifications. `aps-environment` is for push only; critical alerts and
  time-sensitive need Apple-granted entitlements.
  <https://developer.apple.com/documentation/usernotifications/asking-permission-to-use-notifications>
* **Posting:** a nil trigger delivers at once, and reusing an identifier replaces the notification.
  Buttons come from categories registered with `setNotificationCategories:`. Action options:
  Foreground 1<<2, Destructive 1<<1, AuthenticationRequired 1<<0. `UNTextInputNotificationAction`
  gives a typed reply.
  <https://developer.apple.com/documentation/usernotifications/declaring-your-actionable-notification-types>
* **The delegate must be set before launch finishes** (`did_finish_launching`), or a cold-launch
  tap is lost. `didReceive` must call its completion handler.
* **Limits:** the system keeps the soonest 64 pending local notifications; a repeating one counts
  once.
  <https://developer.apple.com/documentation/uikit/uilocalnotification>

### 2.5 Android

* **Channels:** required when targeting 26+, or the notification is not shown (a logged error).
  Importance values: NONE 0, MIN 1, LOW 2, DEFAULT 3, HIGH 4 (heads-up). Importance cannot change
  after creation. The framework `android.app.Notification.Builder(Context, channelId)` is enough;
  no AndroidX.
  <https://developer.android.com/develop/ui/views/notifications/channels>
* **Small icon is mandatory.** Without one: `IllegalArgumentException("Invalid notification (no
  valid small icon)")`. There are three ways to supply it:
  * `android.R.drawable.ic_dialog_info`, the framework resource Tauri uses as a fallback;
  * `Icon.createWithBitmap` (API 23), which renders as an alpha silhouette;
  * the launcher icon, which is 0 when the APK has no resources, as azul's template APK has none.
  * <https://developer.android.com/develop/ui/views/notifications/build-notification>
* **`POST_NOTIFICATIONS` (API 33+):**
  * Declare it in the manifest. It is OFF by default for new installs.
  * Request it with `Activity.requestPermissions`; the answer comes to `onRequestPermissionsResult`,
    an Activity override that `NativeActivity` does not have. azul's `AzulActivity` already
    forwards it.
  * Posting while denied does not throw; the notification is dropped.
  * Apps targeting 32 or lower get a system prompt when they create their first channel.
  * <https://developer.android.com/develop/ui/views/notifications/notification-permission>
* **PendingIntents:**
  * Targeting 31+ requires `FLAG_IMMUTABLE` (0x04000000); mutable only for `RemoteInput`.
  * Intents that differ only in extras are the SAME PendingIntent, so vary the request code.
  * Targeting 34+: a mutable implicit PendingIntent throws.
  * Trampolines are banned (12+): a receiver started by a tap cannot `startActivity`, so content
    taps must use `getActivity`.
  * `setDeleteIntent` fires on swipe / Clear all.
  * A runtime receiver needs `RECEIVER_NOT_EXPORTED` (34+) and dies with the process. Taps after
    process death need a MANIFEST receiver.
  * <https://developer.android.com/about/versions/12/behavior-changes-12>,
    <https://developer.android.com/about/versions/14/behavior-changes-14>
* **Re-entry:**
  * Cold start: `activity.getIntent()` extras over JNI.
  * Warm: `onNewIntent`, with `launchMode="singleTop"` or `FLAG_ACTIVITY_SINGLE_TOP`; call
    `setIntent`.
  * `NativeActivity`, `ANativeActivityCallbacks`, `GameActivity` and android-activity 0.6's
    `MainEvent` expose neither `onNewIntent` nor `onRequestPermissionsResult`. The android-activity
    maintainers recommend a Java subclass, which azul already has.
  * <https://developer.android.com/reference/android/app/Activity>,
    <https://github.com/rust-mobile/android-activity/issues/174>
* **What JNI can do alone:** channel, builder, `notify(tag,id,n)` / `cancel(tag,id)`, the
  permission check and request, content PendingIntents and cold-start taps. `android.*` classes
  resolve with `FindClass` from an attached native thread; APK classes need
  `find_app_class`.
* **What needs Java:**
  * a `BroadcastReceiver` subclass (abstract, so `java.lang.reflect.Proxy` cannot implement it) for
    dismissals, background buttons and alarms;
  * the `onNewIntent` override.
  * azul already compiles Java into the APK (`scripts/build-android.sh:108-142`), so this is one
    more file, not new machinery.
* The JNI call list with signatures is in section 5, step 4.

---

## 3. Own vs depend, against azul's constraints

| constraint | depend (notify-rust + tauri-plugin-style mobile) | own (dlopen / runtime objc / JNI / windows-rs) |
|---|---|---|
| **mobile** | Not coverable: every mobile-capable crate needs Gradle + Kotlin, SwiftPM / `swiftc`, or the Tauri runtime, and emits events to JS rather than Rust. azul's APK is built by javac + d8 with no Gradle, and the iOS app with no Xcode project. | Android = JNI + one Java file in the existing dex step; iOS = the macOS UN file under `cfg(any(macos, ios))`. |
| **behaviour** | mac default impersonates Finder/Terminal (process-wide swizzle, deprecated API, breaking on 14-26); Windows default impersonates PowerShell; Linux: no ActivationToken, no portal, a new connection per post, blocking waits on mac/Win | azul's rules: unbundled = honest "unavailable"; own AUMID; `ActivationToken`; routed events |
| **binary size** | Linux: zbus is already in the graph (`dll/Cargo.toml:235`), so notify-rust adds little there. mac: +1 ObjC object. Win: tauri-winrt-notification's own code is small. | ~0: the ~1700 lines of backend already exist; Windows adds the codegen for two windows-rs namespaces. |
| **supply chain** (`scripts/supply-chain/`) | each crate needs a justification entry, cargo-vet audit and cooldown. mac-notification-sys has a `build.rs` running `cc` (a digest-pinned build-script policy entry). notify-rust ships ~7 releases in 5 months (cooldown churn). user-notify is LGPL (incompatible with an MIT static lib). | no new crates. Windows: two more features on an already-audited crate. Linux portal: zbus, already audited. |
| **cross-compilation from macOS** | mac-notification-sys needs clang + the macOS SDK in build.rs. The windows crate is fine (raw-dylib). Kotlin/Swift crates need their toolchains. | pure Rust + libloading + JNI; Java goes through javac/d8, which the mobile tooling already requires |
| **C ABI** | crate types never cross the FFI; azul still needs its own `repr(C)` model, queue and routing. A crate would only replace the ~300-line OS-call bodies. | same model, one implementation |
| **consistency** | - | tray (libdbus / `Shell_NotifyIconW` / NSStatusItem), media keys (MPRIS, windows-rs SMTC, MediaPlayer via dlopen, `AzulMediaSession.java`), keyring, sensors and the permission backends are all owned this way |

**Verdict:** own it. The one thing worth borrowing is DESIGN:

* Tauri's Android split: `getActivity` for taps, a manifest receiver for dismissals and alarms, a
  boot receiver for schedules, warm taps through `onNewIntent`.
* The Community Toolkit's HKCU AUMID + `LocalServer32` activator registration.
* go-macos's bundle/sign/register findings.

Should macOS switch to `objc2-user-notifications` for typed bindings? Not needed. The dlopen
keeps a UN-less or unbundled process free of the framework, and the existing runtime code already
works (subject to the gaps below).

---

## 4. Proposed API (additions to `azul_core::notification`, before the api.json sync)

```text
Notification
  + payload: AzString                 // app data that survives process death:
                                      // UN userInfo["azul.payload"], Android Intent extra,
                                      // toast launch arg; freedesktop keeps it in-process only
  + channel: OptionNotificationChannel
  + schedule: OptionNotificationSchedule   // section 6
NotificationChannel { id: AzString, name: AzString, importance: NotificationImportance }
NotificationImportance { Low, Default, High, Alarm }   // Android importance, freedesktop urgency,
                                                       // UN interruptionLevel (time-sensitive needs
                                                       // an entitlement), toast scenario
NotificationEvent
  + payload: AzString
  + reply_text: AzString              // reserve now: UN text input / RemoteInput / toast <input>
  + launched_app: bool                // this event started the process (cold start)
NotificationEventType + Delivered     // optional: a scheduled one fired while the app runs

AppConfig / App
  + notification_handler: OptionNotificationCallback
      // receives every event whose notification has no live callback: the process restarted,
      // a scheduled notification fired, a cold-start tap. On mobile this is the MAIN path.
CallbackInfo
  + request_notification_permission()          // -> Capability::Notifications through the
                                               //    existing permission manager; the answer
                                               //    arrives as the existing PermissionChanged event
  + get_permission_status(Capability::Notifications)   // exists; must be made truthful per OS
  + list_scheduled_notifications() / cancel_scheduled_notification(id)   // section 6
PlatformCapability::notifications()            // exists; add scheduled_notifications(), exact_alarms()
```

How a click or action comes back, per platform:

| | while running | after the process died |
|---|---|---|
| macOS | UN delegate `didReceive` (any thread) → mailbox → NSEvent wake → `pump_notifications` → the notification's callback | LaunchServices relaunches the bundle; the delegate (installed before `finishLaunching`) gets the response → the app-level handler with `launched_app = true` |
| iOS | same delegate → mailbox → `display_tick` pump | cold launch; delegate set in `did_finish_launching` → the app-level handler |
| Android | `onNewIntent(intent)` (content/foreground buttons, via `getActivity`) or `AzulNotifications.Receiver` (dismiss/background buttons) → `nativeOnNotificationEvent(id, action, payload)` → mailbox → `android_main` pump | `onCreate` → `getIntent()` extras read by Rust after `publish_jni_context` → the app-level handler; a receiver in a dead process starts the process without the activity, so it must persist the event (a SharedPreferences queue) that Rust drains at next start |
| Windows | `ToastNotification.Activated/Dismissed/Failed` (thread-pool thread) → mailbox → `PostMessageW` to the hidden window → the loop's pump | phase 2: COM `INotificationActivationCallback::Activate(aumid, args, inputs)` in a relaunched process started by `LocalServer32 ... -ToastActivated` → the app-level handler |
| Linux | `ActivationToken` + `ActionInvoked` / `NotificationClosed` via the libdbus filter → mailbox → pump; the token is passed to xdg-activation to raise the window | GNOME destroys a matched app's notifications when it exits; portal: re-activates the app (`org.freedesktop.Application.Activate`, needs a `.desktop`) |

---

## 5. Implementation plan (ordered; each step starts with its RED test)

Test files: `layout/tests/native_notifications.rs` (existing, 25 tests), `wire::*` pure functions,
azul-doc unit tests, and the AZ_E2E `assert_notification` op (`layout/src/e2e/full.rs:4491`).

1. **App identity, once.** One reverse-DNS `AppConfig::app_id` drives:
   * `CFBundleIdentifier` (bundling);
   * the Android package;
   * the freedesktop `desktop-entry` and the Wayland `app_id` (today separate,
     `shell2/linux/wayland/mod.rs:2538`);
   * the Windows AUMID.

   RED: `wire::windows_aumid("rs.azul.widgets")` is ≤129 chars and has no `\`;
   `wire::desktop_entry(..)` equals the Wayland `app_id` default.
2. **Shared core fixes (layout/core, host-testable).**
   * RED `an_event_for_an_unknown_id_reaches_the_app_handler` (today dropped at
     `managers/notification.rs:202-212`).
   * RED `a_post_to_a_full_queue_reports_failed` (`callbacks.rs:5352-5356` drops it).
   * RED `a_delivery_with_no_window_is_kept_until_one_exists` (`run.rs:1825/2410/2831`).
   * Then add `payload`, `channel`, `reply_text` and `launched_app` with builder tests, and
     `request_notification_permission()` pushing `Capability::Notifications` into the permission
     manager. RED: after `push_async_result(Notifications, Granted)`,
     `get_permission_status` answers Granted.
3. **macOS.**
   * a. Install the UN delegate between `run.rs:801` and `run.rs:949` when `bundle_status()` is Ok.
   * b. `getNotificationSettingsWithCompletionHandler:` feeds `probe()` and the permission
     manager.
     RED: `wire::apple_authorization_status(0..=4)` → NotDetermined / Denied / Granted(Full) /
     Granted(Reduced = provisional) / Granted(ephemeral).
   * c. Register categories eagerly, and check them with `getNotificationCategories` before the
     first add.
   * d. `userInfo` carries `azul.payload`.
     RED: `wire::apple_user_info` round-trips id + payload.
   * e. **`azul-doc bundle macos`**: writes `X.app` (Info.plist from `app_id`, exe, dylibs,
     `.icns`), signs the dylibs then the bundle with `codesign --force --sign -`, and
     `lsregister`s it. Installs to `~/Applications`, never `/var/folders`.
     RED (azul-doc unit test): the generated plist has the six keys and `CFBundleExecutable` ==
     the binary name.
   * f. The unbundled reason string names the command.
   * Manual: the design doc's §5 recipe, relocated to `~/Applications`.
4. **Android.**
   * a. Manifest template:
     * declare `POST_NOTIFICATIONS` (it prompts nothing by itself);
     * `launchMode="singleTop"` on `AzulActivity`;
     * `<receiver android:name="com.azul.notify.AzulNotifications$Receiver"
       android:exported="false"/>`;
     * add `AzulNotifications.java` to `doc/src/mobile/assets.rs` (and the 9 missing helpers, G9).
   * b. `AzulActivity.onNewIntent` → `setIntent` + `AzulNotifications.onIntent(intent)`.
   * c. `AzulNotifications.java` (static, like `AzulMediaSession`):
     * `ensureChannel(id, name, importance)`;
     * `post(tag, id, channel, title, body, actionIds[], actionLabels[], payload, iconBitmapOrNull)`;
     * `cancel(tag, id)`;
     * `Receiver.onReceive` → `nativeOnNotificationEvent(...)`, or a SharedPreferences queue when
       the native lib is not loaded.
   * d. Rust `notifications/android.rs` calls it through `find_app_class` + `call_static_method`,
     and reads cold-start extras from `getIntent()` after `publish_jni_context`
     (`android/mod.rs`).
   * e. Pump in `android_main` after `process_timers_and_threads` (`android/mod.rs:797`).
   * f. Permission: the existing `extra/permission/android.rs` (`POST_NOTIFICATIONS`) behind
     `request_notification_permission()`.
   * g. Small icon: an app-supplied monochrome icon-registry glyph rendered to a Bitmap
     (`Icon.createWithBitmap`), falling back to `android.R.drawable.ic_dialog_info`.

   Pure-function RED tests:
   * `wire::android_importance(Importance) -> i32`;
   * `wire::android_pending_intent_flags(sdk)` has `FLAG_IMMUTABLE` for sdk ≥ 31;
   * `wire::android_intent_extras` round-trip;
   * `wire::android_request_code(id)` is distinct per id.

   Emulator E2E (the `azul-doc mobile e2e` path works on a headless emulator): post → `adb shell
   dumpsys notification --noredact` shows the title; `am start -n .../com.azul.app.AzulActivity
   --es azul.notification.id X --es azul.notification.action default` reaches the callback.

   JNI list, if the builder is done from Rust instead:
   * `Context.getSystemService("notification")`;
   * `NotificationChannel.<init>(Ljava/lang/String;Ljava/lang/CharSequence;I)V`;
   * `NotificationManager.createNotificationChannel`;
   * `Intent.<init>(Landroid/content/Context;Ljava/lang/Class;)V` +
     `setFlags(0x10000000|0x20000000)` + `putExtra`;
   * `PendingIntent.getActivity(Context,int,Intent,int)` with 0x04000000|0x08000000;
   * `Notification$Builder.<init>(Landroid/content/Context;Ljava/lang/String;)V`;
   * `setSmallIcon(Landroid/graphics/drawable/Icon;)`, `setContentTitle`, `setContentText`,
     `setContentIntent`, `setDeleteIntent`, `setAutoCancel(Z)`;
   * `Notification$Action$Builder.<init>(Icon,CharSequence,PendingIntent)` + `addAction`;
   * `build()`;
   * `NotificationManager.notify(Ljava/lang/String;ILandroid/app/Notification;)V` /
     `cancel(String,int)`.
5. **iOS.**
   * `mod macos` becomes `mod apple` under `cfg(any(target_os="macos", target_os="ios"))`.
   * Split out the two macOS-only pieces: the `NSApp`/`NSEvent` wake (on iOS the `display_tick`
     pump suffices) and the `.app` path check (always true on iOS).
   * Install the delegate in `did_finish_launching` (`ios/mod.rs:1352`); pump in `display_tick`
     (`ios/mod.rs:1038`).
   * Make `extra/permission/ios.rs:53` answer Notifications from `getNotificationSettings`.
   * RED: the wire tests from step 3 plus `mobile-check-all.sh` for `aarch64-apple-ios`; runtime
     in the CI iOS simulator job (no Xcode on this machine).
6. **Windows toast** (`notifications/windows.rs` keeps the balloon as `fallback`).
   * a. `dll/Cargo.toml:428` windows features `+ "UI_Notifications", "Data_Xml_Dom"`; registry
     through the dlopen'd `advapi32` the permission backend already uses, or
     `Win32_System_Registry`.
   * b. At startup (before the first window): `SetCurrentProcessExplicitAppUserModelID(aumid)`.
   * c. First post: write `HKCU\Software\Classes\AppUserModelId\<aumid>` (`DisplayName`,
     `IconUri` = a PNG written to `%LOCALAPPDATA%`).
   * d. `ToastNotificationManager::CreateToastNotifierWithId(aumid)`; the XML from
     `wire::toast_xml`; `Tag`/`Group` = app id; `Activated`/`Dismissed`/`Failed` →
     mailbox + `PostMessageW` wake.
   * e. Withdraw through `ToastNotificationManager::History().Remove(tag, group)`.
   * f. Fall back to the balloon if activation fails, when elevated, or under the legacy-balloon
     policy.
   * g. Phase 2: `CustomActivator` + `LocalServer32` + a `#[implement(INotificationActivationCallback)]`
     COM class (precedent: `#[implement(IDropTarget)]` in `windows/dnd.rs`).

   RED:
   * `wire::toast_xml` escapes `<&"`, emits `<action content=.. arguments=..>` per button and a
     `launch` carrying id/payload;
   * `wire::toast_args` parses back into an event;
   * `wire::aumid_registry_values`.

   Manual on Windows 10 + 11.
7. **Linux.**
   * a. `desktop-entry` + `urgency` + `image-path` hints. RED: `wire::freedesktop_hints(n, app_id)`.
   * b. Handle `ActivationToken` and hand the token to the Wayland window's xdg-activation, or the
     X11 startup id.
   * c. `Notify` async (`dbus_connection_send_with_reply` + a pending-call poll; two more libdbus
     symbols).
   * d. `NameOwnerChanged` on the server.
   * e. Portal backend (zbus) when `/.flatpak-info` exists. RED:
     `wire::portal_notification(n)` builds `title`/`body`/`default-action`/`buttons`.
8. **E2E + demo.** Add an E2E op `notification_event {id, kind, action?, payload?}` that queues
   into the mailbox (today only Rust tests can), and commit the demo scenario from the design doc's
   §5.
9. **Scheduling** (section 6), after 2-7, because it rides on the same backends.

---

## 6. Scheduled alarms and notifications (AzClock)

### 6.0 Short answer

There are two mechanisms, and an alarm clock needs both.

1. **An OS-owned scheduled notification** fires while the app is NOT running. The OS keeps the
   schedule and shows the notification; a tap relaunches the app.
   * **Native:** Apple (UN triggers; AlarmKit on iOS 26), Android (`AlarmManager` + a Java receiver
     that posts the notification itself), Windows (`ScheduledToastNotification`).
   * **None on Linux.** Neither freedesktop nor the portal can schedule; an alarm needs a running
     process, and GNOME Clocks and KAlarm simply stay running. An opt-in systemd user timer can
     relaunch the app.
   * **None on the web.** A tab must be open; Notification Triggers was abandoned.
2. **An in-process WALL-CLOCK alarm** runs while the app IS running. azul's `Timer` cannot be one
   today: it is monotonic and sleep-blind, and it polls at 100 Hz until a delay elapses (6.1).

The combination: the app registers a schedule once. azul stores it, has each OS schedule what it
can, and runs an in-process alarm for the occurrences the OS cannot own or while the app is in the
foreground. Section 6.4 has the flow and 6.5 the API.

### 6.1 What azul's `Timer` does today

* A `Timer` is RELATIVE: `delay` / `interval` / `timeout` are `Duration`s measured on
  `azul_core::task::Instant`, which is `std::time::Instant` (`core/src/task.rs:255-260`,
  `:447-449`).
  * Rust leaves open whether a suspend counts as elapsed time.
  * Darwin uses `CLOCK_UPTIME_RAW` ("does not increment while the system is asleep",
    `man clock_gettime`), and Linux uses `CLOCK_MONOTONIC` (excludes suspend; `CLOCK_BOOTTIME`
    includes it).
  * Example: a timer created at 23:00 with `with_delay(8 h)`, on a laptop that sleeps
    23:05-07:00, fires at about 14:55.
  * <https://doc.rust-lang.org/std/time/struct.Instant.html>,
    <https://man7.org/linux/man-pages/man2/clock_gettime.2.html>
* The OS wake-up is armed from `Timer::tick_millis()` (`layout/src/timer.rs:141-155`): the
  INTERVAL, or 10 ms when there is none.
  * So a delay-only timer wakes the process 100 times a second until its delay elapses. An 8-hour
    alarm costs about 2.9 million wake-ups.
  * `layout/src/timer.rs:1022-1031` pins this as intended, so changing it is a decision, not a
    fix.
  * Platform arming: `shell2/macos/mod.rs:4297` (repeating NSTimer), `shell2/linux/timer.rs:32-34`
    (timerfd on `CLOCK_MONOTONIC`, relative), `shell2/windows/mod.rs:7374-7378` (`SetTimer`).
* Nothing anywhere in `dll/`, `layout/` or `core/` reacts to wake-from-sleep, a wall-clock change
  or a time-zone change. None of these is handled: `NSWorkspaceDidWakeNotification`,
  `NSSystemClockDidChangeNotification`, `NSSystemTimeZoneDidChangeNotification`,
  `WM_POWERBROADCAST` / `WM_TIMECHANGE`, logind `PrepareForSleep`,
  `UIApplicationSignificantTimeChangeNotification`, Android `ACTION_TIME_CHANGED` /
  `ACTION_TIMEZONE_CHANGED`.
* There is no wall-clock or time-zone type in the public API. `chrono` with `clock` (local zone via
  `iana-time-zone`) is an OPTIONAL azul-layout dependency behind `icu_chrono`
  (`layout/Cargo.toml:108-109, 329-330`).
* On the web, `Instant::now()` is a FRAME counter (`core/src/task.rs:453-469`), and the web runtime
  does not drive `Timer`s at all:
  * `dll/src/web/EVENT_PATCH_SCHEMA.md:136` and `:198` list `AddTimer`/`RemoveTimer` as deferred
    ("needs JS `setInterval` + `AzStartup_fireTimer`").
  * No `AzStartup_fireTimer` exists.
  * So "emulated with timers" is, today, not emulated.

### 6.2 OS scheduling, per platform

**Apple (iOS + macOS, UserNotifications)**
* **Triggers.**
  * `UNCalendarNotificationTrigger(dateMatching:repeats:)` expresses:
    * `{hour,minute}` = daily;
    * `{weekday,hour,minute}` = weekly (ONE weekday, so Mon-Fri needs 5 requests);
    * `{day,hour,minute}` = monthly;
    * `{month,day,..}` = yearly.
  * It cannot express every N days, every 2 weeks, the last day of the month, or "starting from".
  * `UNTimeIntervalNotificationTrigger` needs ≥ 60 s when it repeats.
  * <https://developer.apple.com/documentation/usernotifications/uncalendarnotificationtrigger>,
    <https://developer.apple.com/documentation/usernotifications/untimeintervalnotificationtrigger/init(timeinterval:repeats:)>
* **Time zones: CONFLICT.** An Apple engineer says components without `timeZone` follow the
  current local time; a developer reports they stay locked to the zone at scheduling time. Test on a
  device. DST is not documented. <https://developer.apple.com/forums/thread/811265>
* **Ownership.** The system owns and delivers scheduled requests "when your app isn't running",
  until they fire or are cancelled. List with `getPendingNotificationRequests`, cancel with
  `removePendingNotificationRequestsWithIdentifiers:`, and replace by reusing the identifier.
  <https://developer.apple.com/documentation/usernotifications/scheduling-a-notification-locally-from-your-app>
* **Limit.** iOS keeps the soonest 64 pending (a repeating request counts once). A flutter README
  claims "the last 64 set" instead. No number is documented for macOS. Budget ≤ 64 and top up
  whenever the app runs.
* **Sounds and interruption levels.**
  * Custom sounds must be under 30 s (aiff/wav/caf).
  * `.timeSensitive` (iOS 15 / macOS 12) breaks through Focus. It needs the
    `com.apple.developer.usernotifications.time-sensitive` entitlement, but no Apple approval.
  * Critical alerts (ignore mute and DND) need an Apple-APPROVED entitlement.
  * <https://developer.apple.com/documentation/usernotifications/unnotificationsound>,
    <https://developer.apple.com/videos/play/wwdc2021/10091/>
* **AlarmKit (iOS/iPadOS 26, no macOS).** This is the only way a third-party iOS app can RING like
  the Clock app: it overrides silent mode and Focus, with a full-screen alert, the Lock Screen and
  the Dynamic Island.
  * **Swift-only.** The ObjC variant exposes nothing but version symbols, and the API is `async`
    and generic. So azul would need a small compiled Swift shim (`@_cdecl` functions), i.e.
    `swiftc` in the iOS build.
  * `Alarm.Schedule.fixed(Date)` or `.relative(time, repeats: .never | .weekly([weekdays]))`: one
    alarm can cover Mon-Fri.
  * Needs `NSAlarmKitUsageDescription` and authorization.
  * A widget extension is required only for countdown presentations.
  * <https://developer.apple.com/documentation/alarmkit>,
    <https://developer.apple.com/documentation/alarmkit/scheduling-an-alarm-with-alarmkit>,
    <https://developer.apple.com/documentation/alarmkit?language=objc>

**Android**
* **Exactness.**
  * `setAlarmClock(AlarmClockInfo, PendingIntent)` is exact and Doze-exempt, and shows the alarm
    icon (`getNextAlarmClock`).
  * `setExactAndAllowWhileIdle` is throttled in Doze (about 1-15 min).
  * All repeating alarms are inexact since API 19, so re-arm after every fire.
  * <https://developer.android.com/develop/background-work/services/alarms/schedule>,
    <https://developer.android.com/reference/android/app/AlarmManager>
* **Permissions.**
  * `setAlarmClock` and the exact setters need `SCHEDULE_EXACT_ALARM` when targeting 31+.
  * On Android 14 it is NOT pre-granted to fresh installs targeting 33+. Revoking it deletes all
    exact alarms and kills the app.
  * `USE_EXACT_ALARM` (33+) is granted at install and cannot be revoked, but Play allows it only
    for alarm/timer and calendar apps. That fits AzClock: declare `USE_EXACT_ALARM` plus
    `SCHEDULE_EXACT_ALARM maxSdkVersion="32"`.
  * Probe with `canScheduleExactAlarms()`; request through `ACTION_REQUEST_SCHEDULE_EXACT_ALARM`.
  * <https://developer.android.com/about/versions/14/changes/schedule-exact-alarms>,
    <https://support.google.com/googleplay/android-developer/answer/16558241>
* **Persistence.**
  * Alarms are cleared on reboot and on force-stop; they survive an update. Cap: 500 per UID
    (AOSP `AlarmManagerService`).
  * The app must PERSIST its own schedule and re-arm it from manifest receivers for
    `BOOT_COMPLETED` / `LOCKED_BOOT_COMPLETED` (`directBootAware`, device-protected storage; alarm
    clocks are the docs' canonical example), `TIME_SET` and `TIMEZONE_CHANGED`. These are exempt
    from the implicit-broadcast ban.
  * The boot broadcast arrives only after the user has launched the app once.
  * <https://developer.android.com/develop/background-work/background-tasks/broadcasts/broadcast-exceptions>,
    <https://developer.android.com/privacy-and-security/direct-boot>
* **When it fires with the process dead.**
  * The system starts the process and runs the RECEIVER on the main thread, with about 10 s before
    an ANR.
  * `NativeActivity` loads the native library only in `onCreate`, so `android_main` does not run.
    **The Java receiver must post the notification itself** from the persisted schedule, then arm
    the next occurrence. Alternatively it can `System.loadLibrary` and call a JNI entry, but it must
    not wait for Rust's loop.
  * <https://developer.android.com/reference/android/content/BroadcastReceiver>,
    <https://github.com/aosp-mirror/platform_frameworks_base/blob/main/core/java/android/app/NativeActivity.java>
* **Alarm UI.**
  * A channel with `IMPORTANCE_HIGH`, sound set at channel creation with
    `AudioAttributes.USAGE_ALARM` (4), and `CATEGORY_ALARM`.
  * `setFullScreenIntent` needs `USE_FULL_SCREEN_INTENT`; on Android 14 it is limited to calling
    and alarm apps. Check `canUseFullScreenIntent()`. Without it, the notification is a heads-up
    for 60 s.
  * <https://developer.android.com/reference/android/app/Notification.Builder>,
    <https://developer.android.com/about/versions/14/behavior-changes-14>
* **Re-entry:** a tap goes through `getActivity`, arriving via `getIntent()` on a cold start or
  `onNewIntent` on a warm one (section 4).
* **Inexact periodic work** without AndroidX: JobScheduler (minimum 15 min, `setPersisted`).

**Windows**
* **Scheduling.** `ScheduledToastNotification(xml, deliveryTime)` + `ToastNotifier.AddToSchedule`
  shows the toast whether or not the app runs.
  * Delivery is within a **5-minute window**, and the toast is DROPPED if the PC is off longer than
    that.
  * There are no repeats (the snooze constructor is deprecated), so pre-schedule individual
    occurrences.
  * The cap is 4096 (`0x80070718`).
  * Cancel through `GetScheduledToastNotifications` + `RemoveFromSchedule`; Tag+Group is the key.
  * <https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-scheduled>,
    <https://learn.microsoft.com/en-us/uwp/api/windows.ui.notifications.toastnotifier.addtoschedule>
* **Unpackaged use.**
  * An old remark on the `CreateToastNotifier` page says "Desktop apps cannot schedule a toast".
    Current practice contradicts it: the Community Toolkit's HKCU-AUMID path and
    flutter_local_notifications both call `AddToSchedule` from unpackaged apps.
  * A click after exit needs the COM activator (section 2.2).
  * The Windows App SDK `AppNotificationManager` has NO scheduling API.
* **Alarm style.** `<toast scenario="alarm">` stays until dismissed and loops alarm audio. It needs
  at least one button. <https://learn.microsoft.com/en-us/uwp/schemas/tiles/toastschema/element-toast>
* **Waking the machine.** Only Task Scheduler (`WakeToRun`) can wake it. On Modern Standby it wakes
  the SoC but cannot turn the display on; only a toast does. That is a system change the app must
  opt into, not something the toolkit does silently.
  <https://learn.microsoft.com/en-us/windows-hardware/design/device-experiences/modern-standby-wake-sources>

**Linux**
* **No scheduling in the notification APIs.** Freedesktop has only `Notify(..., expire_timeout)`;
  the portal has only `AddNotification` / `RemoveNotification`.
* **GNOME Clocks and KAlarm do not ring when not running.** The GNOME Clocks issue "Allow that
  alarms also beep when gnome-clocks is not running" is still open; KAlarm autostarts at login
  instead. <https://gitlab.gnome.org/GNOME/gnome-clocks/-/work_items/1>,
  <https://docs.kde.org/trunk_kf6/en/kalarm/kalarm/enable-disable.html>
* **The opt-in option: a systemd user timer.**
  * Created with `systemd-run --user --on-calendar='Mon..Fri *-*-* 07:00' --unit=...`. That unit is
    transient (lives in `$XDG_RUNTIME_DIR`, so it is lost at reboot); persistent units go in
    `~/.config/systemd/user` with `Persistent=true`.
  * `AccuracySec` defaults to 1 min.
  * `OnClockChange=` / `OnTimezoneChange=` are available.
  * No wake: `WakeSystem=` needs the system manager.
  * It runs only while the user manager does (logged in, or with linger).
  * It relaunches the app, which then posts the notification.
  * <https://www.freedesktop.org/software/systemd/man/latest/systemd.timer.html>,
    <https://www.freedesktop.org/software/systemd/man/latest/systemd.time.html>
* **In-process wall clock.**
  * timerfd on `CLOCK_REALTIME` with `TFD_TIMER_ABSTIME | TFD_TIMER_CANCEL_ON_SET` fires at an
    absolute wall time and reports `ECANCELED` when the clock is set.
  * `CLOCK_REALTIME_ALARM` wakes the system from suspend but needs `CAP_WAKE_ALARM`.
  * <https://man7.org/linux/man-pages/man2/timerfd_create.2.html>
* **Flatpak:** `org.freedesktop.portal.Background.RequestBackground` (autostart + run in the
  background).
* **Recommendation:** in-process alarms by default, plus an opt-in "keep running in the background
  / start at login". The systemd timer is an explicit opt-in API, never implicit.

**Web**
* **Timers.**
  * Hidden tabs get about 1 s timer resolution. After 5 minutes hidden, Chrome's intensive
    throttling checks timers once per minute.
  * Nothing fires once the tab is closed or discarded.
  * `setTimeout` caps at 2^31-1 ms (about 24.8 days).
  * <https://developer.chrome.com/blog/timer-throttling-in-chrome-88>,
    <https://developer.mozilla.org/en-US/docs/Web/API/Window/setTimeout>
* **Notification Triggers** (`showTrigger: new TimestampTrigger`) ended after the Chrome 80-83
  origin trial. <https://developer.chrome.com/docs/web-platform/notification-triggers>
* **Push** fires with the app closed but needs a service worker and a SERVER that sends at the
  right time.
* **Periodic Background Sync** is Chromium-only, installed PWAs only, at least about 12 h apart,
  and not exact.
* **Honest promise:** "a reminder while this page is open, possibly up to a minute late while the
  tab is hidden". And that only once the web runtime drives `Timer`s at all (6.1).

### 6.3 Capability matrix

| | fires with the app closed | exact | survives reboot | native repeats | permission | max pending |
|---|---|---|---|---|---|---|
| macOS (bundled) / iOS UN | yes | ~yes | implied | daily, weekly (1 day), monthly, yearly; interval ≥ 60 s | notification auth; time-sensitive entitlement; critical = Apple approval | 64 on iOS |
| iOS 26 AlarmKit | yes, rings through silent mode and Focus | yes | system-owned | weekly with any weekday set; one-shot | `NSAlarmKitUsageDescription` + auth; Swift shim | undocumented |
| Android `setAlarmClock` + receiver | yes (the receiver posts) | yes | NO: re-arm on boot | none: re-arm each fire | `USE_EXACT_ALARM` or `SCHEDULE_EXACT_ALARM`; `POST_NOTIFICATIONS`; `USE_FULL_SCREEN_INTENT` for ringing UI | 500 per UID |
| Windows scheduled toast | yes | ±5 min, dropped if off | implied | none: pre-schedule N | HKCU AUMID | 4096 |
| Windows Task Scheduler (opt-in) | relaunches the exe | yes, can wake | yes | rich | none for the current user | - |
| Linux | NO (in-process only) | yes while running | no | app logic | - | - |
| Linux systemd user timer (opt-in) | relaunches the app while the user manager runs | 1 min default accuracy | as a unit file | rich `OnCalendar` | none; no wake | - |
| Web | NO | late when hidden | no | app logic | notification permission | - |

### 6.4 How the two combine (AzClock)

1. The app calls `schedule_notification(id, notification, schedule)` once.
2. azul persists the canonical schedule: a small JSON store in the app data dir. On Android it
   lives in device-protected storage, so the boot receiver can read it before unlock.
3. azul expands the schedule per backend:
   * **Apple:** calendar triggers, one per weekday for a weekday set, within the 64 budget.
   * **Android:** the next occurrence via `setAlarmClock`; the receiver re-arms.
   * **Windows:** the next N occurrences (e.g. 14 days); topped up at every app start.
   * **Linux / web:** nothing; in-process only.
4. **App running.** An in-process wall-clock alarm fires at the same instant and delivers a
   `Delivered` event (with `scheduled_for` and `late_by`) to the notification's callback or the
   app-level handler. The app shows its own ringing UI. The OS copy:
   * Apple: `willPresent` answers "no banner" when the app handled it (the app's return decides).
   * Android: the receiver runs in the live process and forwards to Rust
     (`nativeOnScheduledFire`); Rust tells Java whether to post.
   * Windows: the scheduled toast still appears, and it cannot be intercepted. Accept it, or
     `RemoveFromSchedule` that occurrence when the app is in the foreground shortly before the
     time. That is racy; document it.
   * Linux / web: the in-process alarm POSTS the notification itself.
5. **App closed.** The OS shows it (Android on the alarm channel, iOS 26 through AlarmKit, Windows
   with `scenario="alarm"`). A tap relaunches the app, and the app-level handler gets `Activated`
   with `launched_app = true`, the schedule id and the occurrence time in `payload`. The app then
   calls `list_scheduled_notifications()` to rebuild its UI, and azul tops up the
   iOS/Windows/Android windows.

### 6.5 Proposed API

```text
NotificationSchedule (repr C, u8)
  At      { unix_ms: u64 }                                  // absolute instant (UTC)
  Daily   { hour: u8, minute: u8 }                          // local wall time
  Weekly  { weekdays: u8 /* bit 0 = Mon .. bit 6 = Sun */, hour: u8, minute: u8 }
  Monthly { day: u8 /* 1..=31, skips short months like UN */, hour: u8, minute: u8 }
  Every   { seconds: u64 /* >= 60 (UN's floor) */ }
ScheduleTimeZone { Floating /* follows the device zone, the default */, Fixed(AzString /* IANA */) }
ScheduledNotification { id, notification: Notification, schedule, time_zone,
                        alarm: bool /* alarm channel / scenario="alarm" / AlarmKit where available */,
                        missed: MissedPolicy { FireLate { max_late_ms }, Skip } }

App / CallbackInfo
  schedule_notification(ScheduledNotification) -> Result<(), ScheduleError /* Unsupported, Denied,
                                                   LimitReached, ExactAlarmNotPermitted, .. */>
  cancel_scheduled_notification(id)
  list_scheduled_notifications() -> ScheduledNotificationVec   // azul's own store, the source of truth
  request_exact_alarm_permission()                            // Android settings intent; no-op elsewhere

WallClockAlarm (in-process; also usable without any notification)
  Timer::at_wall_clock(unix_ms) / Timer::with_schedule(NotificationSchedule, ScheduleTimeZone)
  TimerCallbackInfo + scheduled_for_unix_ms, late_by_ms

PlatformCapability::scheduled_notifications()  // available / backend / reason, plus
  ScheduleCapabilities { fires_when_closed, exact, survives_reboot, max_pending, rings_through_silent }
```

**Cron.** A 5-field cron subset maps onto these rules: `M H * * 1-5` → `Weekly{Mon..Fri}`,
`M H * * *` → `Daily`, `M H D * *` → `Monthly`. Anything else (steps, `L`, `W`, ranges across
hours) is expanded into the next N `At` occurrences and topped up. That keeps it honest per
platform: no backend can store an arbitrary cron expression, so azul re-arms.

**The in-process alarm must be wall-clock, not a `Timer` delay:**
* Arm the OS timer for `min(remaining, 60 s)`, never 10 ms, and compare `SystemTime::now()` to the
  target on every wake.
* Re-evaluate on resume and on clock or zone changes: the notifications listed in 6.1, plus
  timerfd `TFD_TIMER_CANCEL_ON_SET` and logind `PrepareForSleep(false)`.
* A local-time rule needs a tz database: chrono + iana-time-zone, already optional.
* DST: a spring-forward gap fires at the first valid instant after it; a fall-back overlap fires
  once, on the first occurrence.
* A missed occurrence (the machine slept through it) follows `MissedPolicy`, and `late_by_ms` tells
  the app.

### 6.6 Plan for scheduling (after section 5, steps 2-7)

1. RED `wire::next_occurrence(rule, now_utc, tz_offset_fn)`. Cases: the DST gap, the DST overlap,
   `Monthly{31}` in February, a Mon-Fri set that crosses the weekend, `Every` phase. The pure
   function takes an injected UTC-offset closure, so no tz database is needed in tests.
2. RED `wire::cron_to_schedule("0 7 * * 1-5") == Weekly{0b0011111, 7, 0}`; an unsupported
   expression returns `Err` (it will be expanded instead).
3. RED `wire::apple_triggers(Weekly{Mon..Fri})` produces 5 date-component sets.
   `wire::windows_occurrences(rule, now, 14 days)` is capped at 4096.
   `wire::android_next_trigger_ms`.
4. RED in-process: `a_wall_clock_alarm_fires_by_wall_time_after_a_suspend`, using an injected
   `SystemTime` that jumps while `Instant` does not, and
   `a_wall_clock_alarm_rearms_after_a_time_zone_change`. The existing E2E `tick_ms` op moves only
   `Instant`, so it needs a wall-clock twin.
5. Backends: UN triggers (macOS bundled + iOS); Android `AzulAlarms.java`
   (`setAlarmClock`, the fire receiver, the boot/time/tz receivers, and a persisted JSON schedule
   in device-protected storage); Windows `AddToSchedule` with Tag+Group. Linux and web get the
   in-process alarm only, and the capability reports it.
   * Emulator E2E: `adb shell dumpsys alarm | grep <package>` shows the armed alarm;
     `adb shell am broadcast -n <pkg>/com.azul.notify.AzulAlarms\$Receiver --es id X` simulates
     the fire with the process dead (`am force-stop` does NOT fit, because it clears alarms; use
     `am kill`).
6. Later, opt-in: the iOS 26 AlarmKit Swift shim (needs `swiftc` in the iOS pipeline), the
   Windows Task Scheduler wake, and the Linux systemd user timer / autostart. Each is an explicit
   app API, never a default.

---

## 7. Gaps in the current implementation (file:line, HEAD e0009e01e)

Ordered by impact, mobile first. "model" = `core/src/notification.rs`, "queue" =
`layout/src/managers/notification.rs`, "svc" = `dll/src/desktop/notifications/mod.rs`.

### G1. Mobile: no backend, no pump, no delivery (blocks the user's main target)

* svc:47-52 and svc:107-114 compile a backend only for macOS / Linux / Windows. iOS and Android
  fall to `Backend::Unavailable(UNSUPPORTED)` (svc:59-61). The iOS UN code is ~90% the macOS file
  (`notifications/macos.rs`), which is gated `#[cfg(target_os = "macos")]` only.
* No mobile loop calls `pump_notifications()`. The existing call sites are `shell2/run.rs:1810`
  (Windows), `:2388` (Linux), `:2816` / `:2986` (macOS) and `headless/mod.rs:2989`. The natural
  mobile slots exist and are unused:
  * iOS `display_tick` (`shell2/ios/mod.rs:1038`), which already runs
    `process_timers_and_threads`;
  * Android `android_main` right after `process_timers_and_threads` (`shell2/android/mod.rs:797`).

  Both `IOSWindow` and `AndroidWindow` implement `PlatformWindow` (`ios/mod.rs:2001`,
  `android/mod.rs:535`), so `invoke_deliveries` works there as-is.
* So on mobile a post becomes a `Failed` event that is QUEUED and NEVER DELIVERED: the capability
  pump only dispatches (`common/capability_pump.rs:169-176`). The design doc admits this (§4 table,
  §7).

### G2. Events for a notification the PROCESS no longer knows are dropped

* queue:202-212 `NotificationRegistry::route` drops every event whose id is not live. The registry
  lives in memory (svc:210) and starts lazily on the first post (svc:213-220).
* That is the COMMON case on mobile and a normal one on desktop, and in every case the
  notification's `RefAny` callback is gone:
  * iOS and Android kill background apps; a tap cold-launches the app with the response (iOS
    `didReceiveNotificationResponse`, Android the launch `Intent`).
  * macOS relaunches a bundled app on a click from Notification Center.
  * A Windows toast activates through COM.
* Missing API: an APP-LEVEL handler that receives every event with no live callback, including
  the cold-launch response (section 4).
* Missing data: `Notification` (model:124-145) has no `payload`, the only thing that survives
  process death.

### G3. Permission is asked implicitly on every post; no request API, no truthful status

* `notifications/macos.rs:649-675` calls `requestAuthorizationWithOptions:` on EVERY post, so the
  only way to trigger the prompt is to post. Apple's guidance is to ask in context; `provisional`
  exists for the rest.
* The permission system already has `Capability::Notifications`
  (`layout/src/managers/permission.rs:69-70`), and Android already maps it to `POST_NOTIFICATIONS`
  (`extra/permission/android.rs:105`), with a working `onRequestPermissionsResult` forward in
  `scripts/android/AzulActivity.java`. But nothing requests it: no NodeType bears it and no
  `CallbackInfo` method asks for it. **On Android an app cannot obtain the permission at all.**
* Status probes are wrong:
  * iOS answers `NotDetermined` for Notifications forever (`extra/permission/ios.rs:53-56`).
  * macOS keeps the answer in a per-process static (`notifications/macos.rs:93`). After a
    relaunch, `probe()` (macos.rs:224-247) says "available, asked on first post" even when the
    user turned notifications off.
  * So the design doc's §5 step 7 ("relaunch → Unavailable ... turned off") is not what the code
    does. `getNotificationSettingsWithCompletionHandler:` is missing.
* `CallbackInfo::post_notification` (`layout/src/callbacks.rs:5352-5356`) discards the `bool`
  from `push_notification_request`, so a full queue drops the post silently. That contradicts
  "failures are events".

### G4. macOS: delegate too late, category race, no bundling

* `install_delegate` runs inside `PlatformNotifier::new` (macos.rs:641), i.e. at the process's
  FIRST POST. Apple requires the delegate before launch finishes, otherwise the response that
  launched the app is never delivered. The slot is between `shell2/run.rs:801-802` (the
  AppDelegate is set) and `run.rs:949` (`finishLaunching`). The same applies on iOS in
  `did_finish_launching` (`shell2/ios/mod.rs:1352`).
* `ensure_category` (macos.rs:477-500) registers a new button set right before
  `addNotificationRequest:`. `setNotificationCategories:` is asynchronous, so the first
  notification of a new button set can show without its buttons (a known UN race; not verified
  here).
* Unbundled = unavailable is right, but nothing in the repo produces a `.app`: there is no macOS
  bundling in `azul-doc`, the examples or CI (only the iOS job bundles). So every cargo-built azul
  macOS app reports "Unavailable".
* The manual recipe in the design doc (§5) signs with `--deep` (deprecated for signing; sign the
  inner dylibs first) and uses `/tmp/AzWidgets.app`. go-macos found bundles under `/var/folders`
  refused; use `~/Applications`.
* `macos.rs:95-103` links AppKit at build time for `NSApp`. That is fine on macOS, but it is the
  one line that keeps the file from compiling for iOS as-is (step 5).

### G5. Windows: the balloon is a dead end

* `notifications/windows.rs` (whole file) has no buttons. A new post dismisses the previous one
  (windows.rs:311-325). The icon is deleted as soon as the balloon ends (windows.rs:419-423).
* The module doc's claim that the balloon "lands in the Action Center" (windows.rs:11-13) holds on
  Windows 10 only. **On Windows 11 a timed-out balloon is not kept in Notification Center at all.**
  On Windows 10 the kept entry belongs to an icon that no longer exists, so it cannot reach the app.
* It adds its OWN notify icon (windows.rs:328-346). Once the tray's Windows backend lands
  (`tray/windows.rs` is a stub), an app with a tray icon gets a second, transient icon per
  notification. The balloon should hang off the tray icon when one exists.
* The rejection of toasts rests on "`ToastNotifier::Show` fails silently without an AUMID"
  (windows.rs:5-11). A HKCU AUMID written at first run needs no installer, and the `windows` 0.62
  crate is ALREADY a dependency (`dll/Cargo.toml:428-469`), already used for WinRT with
  `TypedEventHandler` (`extra/media_keys/windows.rs:103-120`).
* Attribution: the balloon toast shows the process's `FileDescription`. No crate in the repo
  embeds a Windows version resource (no `winres` / `embed-resource` / `.rc` under `examples/` or
  `dll/`), so it falls back to the file name. A toast with a registered AUMID uses the
  registry's `DisplayName` instead.

### G6. Linux: identity, Wayland focus, sandbox, blocking

* No `desktop-entry` hint (`notifications/linux.rs:568-586` sends only sound hints), and
  `app_name` is the exe stem (linux.rs:471-477).
  * GNOME matches by PID → window first, so attribution usually works while a window is mapped.
  * A windowless or tray-only process falls back to `desktop-entry` / `app_name` and otherwise
    gets a generic source with no per-app settings.
  * The Wayland `app_id` already exists (`shell2/linux/wayland/mod.rs:2538-2546`) and should be
    the same string.
* The `ActivationToken` signal is ignored (filter, linux.rs:357-373). On Wayland a click cannot
  raise the app's window: the callback runs and the window stays behind.
* No portal backend, so there are no notifications inside Flatpak. The zbus portal transport
  exists in `global_hotkey/portal.rs:28-54`.
* `Notify` blocks the UI thread up to 3 s (linux.rs:66, :591); the design doc acknowledges this.
* No `NameOwnerChanged` watch on the server. No `urgency`, `category` or `image-path` hints.

### G7. Delivery needs a window

* Every loop routes the events (the registry FORGETS them, queue:206), then drops them when no
  window exists ("had no window to run against"): `run.rs:1825` (Windows), `:2410` (Linux),
  `:2831` (macOS). For example, the last window was closed while the app keeps running.
* `run_tray_only` (with its headless stand-in, `run.rs:2986`) exists on macOS only
  (`run.rs:2898`).
* Keep undeliverable deliveries queued, or hand them to the app-level handler.

### G8. Model gaps to close before api.json freezes the ABI

* No channel / importance. Android REQUIRES a channel on API 26+.
* No schedule (section 6).
* No text reply.
* No payload (G2).
* `icon` means a PNG path on macOS, a `.ico` on Windows and a theme name on Linux, and there is no
  answer for Android's monochrome small icon (the manifest template has no `android:icon`,
  `scripts/android/AndroidManifest.xml:41-43`). Better as an icon-registry spec rendered per
  platform, like the tray's.

### G9. Adjacent tooling gaps

* `doc/src/mobile/assets.rs:55-90` embeds only 4 of the 13 Java helpers (`AzulActivity`,
  `AzulAccessibilityBridge`, `AzulFilePicker`, `NativeGestureBridge`). `AzulActivity.java`
  imports 5 of the 9 missing ones (`NativeTextBridge`, `AzulGamepad`, `AzulPermissions`,
  `AzulMediaSession`, `AzulSensors`). A downloaded `azul-doc` building outside the checkout (the
  cache path in `ensure`, assets.rs:111-150) would fail `javac`.
* `scripts/android/AndroidManifest.xml` has three gaps:
  * `POST_NOTIFICATIONS` is only injectable via `AZ_ANDROID_PERMISSIONS`. With
    `targetSdkVersion=34`, requesting an undeclared permission is refused without a dialog.
  * No `launchMode`.
  * `AzulActivity` has no `onNewIntent`.
* `drain_tray_events()` has no caller (found by the notifications agent, design doc §1): bare tray
  events never reach the app.
