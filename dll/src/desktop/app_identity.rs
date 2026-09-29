//! The app's identity - ONE value every shell asks for, instead of each
//! deriving the app's name from the executable on its own.
//!
//! `current()` reads the process once: what the platform DECLARED, where it
//! declares something, else the executable's name. Each OS service then takes
//! its own form of the same identity (`azul_layout::managers::notification::wire::AppIdentity`):
//!
//! | who | reads |
//! |---|---|
//! | Windows toasts | `windows_aumid()` (the AUMID registered under HKCU, and the COM activator's key), `display_name()` |
//! | freedesktop notifications | `desktop_entry()` (the `desktop-entry` hint), `display_name()` (`app_name`) |
//! | Wayland windows | `desktop_entry()` (the default `app_id`) |
//! | X11 windows | `desktop_entry()` (the default `WM_CLASS` instance) |
//! | `azul-doc bundle macos` | `apple_bundle_id()` of the binary it bundles (its default `CFBundleIdentifier`) |
//!
//! What the platforms declare: a macOS / iOS app bundle its `CFBundleIdentifier`, a Flatpak
//! sandbox `FLATPAK_ID`, an Android process its package. An app cannot name itself yet: an
//! `AppConfig::app_id` is an ABI change, proposed in `scripts/N1_NOTIFICATION_PLATFORMS_2026_09_29.md`
//! and not built.

use std::sync::OnceLock;

use azul_layout::managers::notification::wire::AppIdentity;

/// This process's identity, read once.
#[must_use]
pub fn current() -> &'static AppIdentity {
    static IDENTITY: OnceLock<AppIdentity> = OnceLock::new();
    IDENTITY.get_or_init(|| {
        let exe = std::env::current_exe()
            .ok()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_default();
        match declared_id() {
            Some(id) => AppIdentity::declared(&id, &exe),
            None => AppIdentity::from_executable(&exe),
        }
    })
}

/// The bundle's `CFBundleIdentifier` - only when the process runs from a
/// `.app` (an unbundled binary's main bundle is its directory, whose
/// identifier nobody chose).
#[cfg(any(target_os = "macos", target_os = "ios"))]
fn declared_id() -> Option<String> {
    crate::desktop::notifications::apple_bundle_id()
}

/// Inside a Flatpak sandbox, `FLATPAK_ID`: the id the portal attributes
/// notifications to and the `.desktop` file is named after.
#[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
fn declared_id() -> Option<String> {
    std::env::var("FLATPAK_ID")
        .ok()
        .filter(|id| !id.trim().is_empty())
}

/// The package: an Android app process is named after it (`current_exe` is
/// the zygote's `app_process`, which names nothing). A secondary process is
/// `<package>:<name>`.
#[cfg(target_os = "android")]
fn declared_id() -> Option<String> {
    let cmdline = std::fs::read("/proc/self/cmdline").ok()?;
    let first = cmdline.split(|b| *b == 0).next()?;
    let name = String::from_utf8_lossy(first);
    let package = name.split(':').next().unwrap_or("").trim();
    (!package.is_empty()).then(|| package.to_string())
}

/// Windows (an unpackaged exe declares nothing) and everything else.
#[cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "android",
    all(target_os = "linux", not(target_arch = "wasm32"))
)))]
fn declared_id() -> Option<String> {
    None
}
