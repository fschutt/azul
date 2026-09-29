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

- C. Windows COM activator (item 3): RED `ffb2ff0e4`, fix = the commit after it.
  `wire::{TOAST_ACTIVATED_SWITCH, launched_by_toast_activation, toast_activator_clsid,
  guid_string, RegistryValue, toast_registry_values, toast_activator_event}`; windows.rs
  `activator` module (`#[implement]` activator + class factory, `CoRegisterClassObject`),
  `install_launch_hooks` (run.rs Windows `run()` calls it before the first window), registry
  via one cached advapi32 (`write_registry_value`, `read_registry_string`); dnd.rs
  `ensure_ole_initialized` made `pub(crate)`; Cargo feature `Win32_UI_Notifications`.

- D. Android permission request on the UI thread (item 2): RED `2fb54ca99`, fix = the commit
  after it. `AzulPermissions.request` (Java, `runOnUiThread`, a refused start reports a
  denial), `permission/android.rs::request_permission` calls it via `find_app_class`. RED is a
  source invariant: new `dll/src/desktop/notifications/platform_invariants.rs` (reuses
  `loop_wakeup_invariants::top_level_fn_body`, made `pub(crate)`).

## IN PROGRESS

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
