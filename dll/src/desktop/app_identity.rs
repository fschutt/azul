//! The app's identity - ONE value every shell asks for, instead of each
//! deriving the app's name from the executable on its own.
//!
//! `current()` reads the process once and hands everything that may name
//! the app to one pure rule (`wire::AppIdentity::resolve`): what the
//! platform DECLARED, where it declares something, wins; else the app's own
//! `AppConfig::app_id` ([`declare`]); else the executable's name. Each OS
//! service then takes its own form of the same identity
//! (`azul_layout::managers::notification::wire::AppIdentity`):
//!
//! | who | reads |
//! |---|---|
//! | Windows toasts | `windows_aumid()` (the AUMID registered under HKCU, and the COM activator's key), `display_name()` |
//! | freedesktop notifications | `desktop_entry()` (the `desktop-entry` hint), `display_name()` (`app_name`) |
//! | Wayland windows | `desktop_entry()` (the default `app_id`; a window's `wayland_app_id` overrides it) |
//! | X11 windows | `desktop_entry()` (the default `WM_CLASS` instance) |
//! | `azul-doc bundle macos` | `apple_bundle_id()` of the binary it bundles (its default `CFBundleIdentifier`, unless the crate's `[package.metadata.bundle] identifier` names one) |
//!
//! What the platforms declare: a macOS / iOS app bundle its `CFBundleIdentifier`, a Flatpak
//! sandbox `FLATPAK_ID`, an Android process its package. There the app's `app_id` only
//! produces a warning when it names another app; on Windows and on Linux outside Flatpak it
//! is the id.

use std::sync::OnceLock;

use azul_layout::managers::notification::wire::{AppIdentity, PlatformAppId};

/// The app's own id (`AppConfig::app_id`), as [`declare`] received it.
static DECLARED: OnceLock<String> = OnceLock::new();
/// The identity, resolved at the first [`current`] call.
static IDENTITY: OnceLock<AppIdentity> = OnceLock::new();

/// Declare the app's own id, `AppConfig::app_id` (empty = none). Called by
/// `App::create`, the one place every run path passes through before any
/// window, launch hook or notification reads the identity.
///
/// The identity is resolved right away, so a conflict with the platform's
/// declaration is logged at startup. A process has ONE identity - its
/// registry keys, activator and window classes are already out - so a
/// second, different declaration (a second `App`) is logged and ignored.
pub fn declare(app_id: &str) {
    let app_id = app_id.trim();
    if app_id.is_empty() {
        return;
    }
    if let Err(rejected) = DECLARED.set(app_id.to_string()) {
        let first = DECLARED.get().map_or("", String::as_str);
        if first != rejected {
            crate::plog_warn!(
                "[azul] AppConfig::app_id {rejected:?} ignored: this process already declared \
                 {first:?}, and an app keeps one identity for its whole run"
            );
        }
        return;
    }
    if let Some(identity) = IDENTITY.get() {
        crate::plog_warn!(
            "[azul] AppConfig::app_id {app_id:?} ignored: the app's identity was read before the \
             App was created and stays {:?} for this run",
            identity.id
        );
        return;
    }
    let _ = current();
}

/// This process's identity, read once.
#[must_use]
pub fn current() -> &'static AppIdentity {
    IDENTITY.get_or_init(|| {
        let exe = std::env::current_exe()
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        let app_id = DECLARED.get().map_or("", String::as_str);
        let platform = platform_app_id();
        let resolved = AppIdentity::resolve(app_id, platform.as_ref(), &exe);
        if let Some(warning) = &resolved.warning {
            crate::plog_warn!("[azul] {warning}");
        }
        resolved.identity
    })
}

/// The bundle's `CFBundleIdentifier` - only when the process runs from a
/// `.app` (an unbundled binary's main bundle is its directory, whose
/// identifier nobody chose).
#[cfg(any(target_os = "macos", target_os = "ios"))]
fn platform_app_id() -> Option<PlatformAppId> {
    crate::desktop::notifications::apple_bundle_id().map(PlatformAppId::AppleBundle)
}

/// Inside a Flatpak sandbox, `FLATPAK_ID`: the id the portal attributes
/// notifications to and the `.desktop` file is named after.
#[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
fn platform_app_id() -> Option<PlatformAppId> {
    std::env::var("FLATPAK_ID")
        .ok()
        .filter(|id| !id.trim().is_empty())
        .map(PlatformAppId::Flatpak)
}

/// The package: an Android app process is named after it (`current_exe` is
/// the zygote's `app_process`, which names nothing). A secondary process is
/// `<package>:<name>`.
#[cfg(target_os = "android")]
fn platform_app_id() -> Option<PlatformAppId> {
    let cmdline = std::fs::read("/proc/self/cmdline").ok()?;
    let first = cmdline.split(|b| *b == 0).next()?;
    let name = String::from_utf8_lossy(first);
    let package = name.split(':').next().unwrap_or("").trim();
    (!package.is_empty()).then(|| PlatformAppId::AndroidPackage(package.to_string()))
}

/// Windows (an unpackaged exe declares nothing: the app's `app_id` names it)
/// and everything else.
#[cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "android",
    all(target_os = "linux", not(target_arch = "wasm32"))
)))]
fn platform_app_id() -> Option<PlatformAppId> {
    None
}
