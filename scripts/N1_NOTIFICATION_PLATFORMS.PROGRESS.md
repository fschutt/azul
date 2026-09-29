# N1_NOTIFICATION_PLATFORMS - progress

Branch `wt/n1-notification-platforms`, based on `0a326afe5`. House rules:
`scratchpad/wave3_common.md`. Nothing is compiled here (the parent compiles once).

## DONE

- A. iOS `launched_app` (item 1): RED `715dc1f72`, fix `77740437b`. `wire::LaunchResponseMarker`;
  `notifications::refresh_permission` renamed `app_became_active` (macOS + iOS delegates call it).
  Tests: `layout/tests/native_notifications.rs` `mod platforms` (appended at the END).

- B. One app identity (item 4): RED `217ca3977`, fix = the commit after it. `wire::AppIdentity`
  + `dll/src/desktop/app_identity.rs::current()`; readers: Windows toast AUMID/DisplayName,
  Linux `app_name` + `desktop-entry`, Wayland `app_id` default, X11 `WM_CLASS` default,
  `azul-doc bundle` default bundle id (now from the BINARY name). AUMID default changed
  `azul.AzWidgets` -> `com.azul.azwidgets` (report it). Twin left alone: mobile
  `Target::resolve` bundle id (`com.azul.<crate _>`), report it.

## IN PROGRESS

- C. Windows COM activator (item 3): `wire::toast_activator_clsid`, `wire::guid_string`,
  `wire::toast_activator_registry`, `wire::launched_by_toast_activation`; windows.rs
  `#[implement(INotificationActivationCallback)]` + `IClassFactory`, `CoRegisterClassObject`
  at startup (run.rs Windows `run()` -> `notifications::install_launch_hooks()`).
- D. Android permission request on the UI thread (item 2): `AzulPermissions.request` (Java,
  `runOnUiThread`), `permission/android.rs::request_permission` calls it through the activity's
  class loader. RED = a source-invariant test (no headless observer exists).
- E. Linux (item 6): async `Notify` (`dbus_connection_send_with_reply` + pending-call poll,
  `wire::FreedesktopPosts` bookkeeping), `NameOwnerChanged` watch (server restart ->
  Dismissed events), Flatpak portal transport over the same libdbus connection
  (`wire::portal_notification`, `wire::in_flatpak_sandbox`).
- F. Bundle step (item 5): `.icns` from `[package.metadata.bundle] icon` / `--icon`
  (PNG -> ICNS container in pure Rust, or an `.icns` copied), `CFBundleIconFile`; recursive
  dylib walk (`plan_dylib_tree`, own-id skip, `install_name_tool -id/-change`), `--portable`.
  Notarization: documented only.
- G. Report `scripts/N1_NOTIFICATION_PLATFORMS_2026_09_29.md`.

## Open questions

- none yet
