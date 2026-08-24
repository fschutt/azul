//! System tray / status icon — platform dispatch.
//!
//! The data model lives in `azul_core::tray`; this module is the OS plumbing.
//!
//! # Architecture
//!
//! One `TrayIcon` owns one platform handle. The three backends look nothing
//! alike, so the shared surface is deliberately thin:
//!
//! | | Windows | macOS | Linux |
//! |---|---|---|---|
//! | mechanism | `Shell_NotifyIconW` on a hidden top-level HWND | `NSStatusItem` retained by us | `org.kde.StatusNotifierItem` over D-Bus |
//! | menu | `HMENU` we pop with `TrackPopupMenu` | `NSMenu` AppKit draws | `com.canonical.dbusmenu` the PANEL draws |
//! | re-register on | `TaskbarCreated` broadcast | never | watcher `NameOwnerChanged` |
//! | can it be absent? | practically no | no | **yes** — vanilla GNOME has no watcher |
//!
//! # Events
//!
//! Tray callbacks arrive on whatever thread the OS feels like (a D-Bus
//! dispatch on Linux, the message pump on Windows, the main run loop on
//! macOS), and none of them can hold a `CallbackInfo`. So they post into a
//! process-wide mailbox and the run loop drains it — the same shape
//! `gnome_menu::actions_protocol` already uses for menu activations.

use azul_core::tray::{TrayEvent, TrayIconData};

#[cfg(target_os = "windows")]
mod windows;
#[cfg(target_os = "macos")]
pub(crate) mod macos;
#[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
mod linux;

#[cfg(target_os = "windows")]
use windows as platform;
#[cfg(target_os = "macos")]
use macos as platform;
#[cfg(all(target_os = "linux", not(target_arch = "wasm32")))]
use linux as platform;

/// Why a tray icon could not be created or updated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayError {
    /// No tray exists on this desktop. The overwhelmingly common case is a
    /// vanilla GNOME session, where `org.kde.StatusNotifierWatcher` is simply
    /// not owned by anybody and nothing will ever display the item.
    ///
    /// This is NOT a bug and must not be treated as one: the app needs a story
    /// for running without a tray.
    Unavailable,
    /// The OS refused the call.
    Platform(String),
    /// This build has no tray backend for the target (web, android, ios).
    Unsupported,
}

impl core::fmt::Display for TrayError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Unavailable => write!(
                f,
                "no system tray is available on this desktop (on GNOME this usually means the \
                 AppIndicator extension is not installed)"
            ),
            Self::Platform(m) => write!(f, "system tray error: {m}"),
            Self::Unsupported => write!(f, "system tray is not supported on this platform"),
        }
    }
}

/// Process-wide tray event mailbox.
///
/// Tray callbacks cannot hold a `CallbackInfo` — they run on a D-Bus dispatch
/// thread, inside a `TrackPopupMenu` modal loop, or in an AppKit action — so
/// they queue here and the run loop drains it between frames.
static TRAY_EVENTS: std::sync::LazyLock<std::sync::Mutex<Vec<TrayEvent>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(Vec::new()));

/// Post an event from a platform callback. Never blocks the caller for long
/// and never panics across an FFI boundary (a poisoned lock is dropped, not
/// unwrapped — losing one tray click beats aborting the process from inside an
/// objc / D-Bus / Win32 callback).
pub(crate) fn queue_tray_event(ev: TrayEvent) {
    if let Ok(mut q) = TRAY_EVENTS.lock() {
        // A tray the app never drains must not grow without bound.
        const MAX_QUEUED: usize = 256;
        if q.len() < MAX_QUEUED {
            q.push(ev);
        }
    }
}

/// Take everything queued since the last call. Called by the run loop.
#[must_use]
pub fn drain_tray_events() -> Vec<TrayEvent> {
    TRAY_EVENTS
        .lock()
        .map(|mut q| core::mem::take(&mut *q))
        .unwrap_or_default()
}

/// The app's icon registry, so a tray can resolve a named icon.
///
/// `OnceLock` rather than a mutex because the icon SET and the resolver are
/// frozen once the provider is shared — `App::run` consumes the
/// `IconProviderHandle` and `SharedIconProvider` exposes no registration — so
/// there is nothing to update after this is set.
///
/// A global rather than a parameter because the tray is app-level and outlives
/// any window, while the provider is currently threaded through per-window
/// state.
static ICON_PROVIDER: std::sync::OnceLock<azul_core::icon::SharedIconProvider> =
    std::sync::OnceLock::new();

/// Publish the icon registry for tray (and later app-icon) rendering. Called
/// once from `run()` after the provider is built. Subsequent calls are ignored.
pub fn set_icon_provider(provider: azul_core::icon::SharedIconProvider) {
    let _ = ICON_PROVIDER.set(provider);
}

/// Render a registry icon spec to RGBA at `size_px` square.
///
/// Returns `None` when no provider has been published (a headless or
/// pre-`run()` caller), or when the spec resolves to nothing — the latter is
/// deliberately distinguished from a blank bitmap, since an all-transparent
/// icon in a tray looks exactly like a working one.
pub(crate) fn render_named_icon(
    spec: &str,
    size_px: u32,
) -> Option<azul_layout::tray_icon::RenderedIcon> {
    let provider = ICON_PROVIDER.get()?;
    azul_layout::tray_icon::render_icon_to_rgba(
        spec,
        size_px,
        provider,
        &azul_css::system::SystemStyle::default(),
        None,
    )
}

/// A live system-tray icon.
///
/// Dropping it removes the icon. On macOS that is the ONLY removal API —
/// `NSStatusBar` does not retain status items, so the `Retained<NSStatusItem>`
/// this owns is what keeps the icon on screen.
#[derive(Debug)]
pub struct TrayIcon {
    #[cfg(any(
        target_os = "windows",
        target_os = "macos",
        all(target_os = "linux", not(target_arch = "wasm32"))
    ))]
    inner: platform::PlatformTray,
    data: TrayIconData,
}

impl TrayIcon {
    /// Is there anything on this system that would display a tray icon?
    ///
    /// Returns `true` on Windows and macOS (the notification area / menu bar
    /// always exist). On Linux this is a real question: it checks that
    /// `org.kde.StatusNotifierWatcher` is owned AND that a host has registered
    /// with it. Both halves matter — a watcher with no host is a real state,
    /// because the watcher can win the startup race against the panel.
    #[must_use]
    pub fn is_available() -> bool {
        #[cfg(any(
            target_os = "windows",
            target_os = "macos",
            all(target_os = "linux", not(target_arch = "wasm32"))
        ))]
        {
            platform::is_available()
        }
        #[cfg(not(any(
            target_os = "windows",
            target_os = "macos",
            all(target_os = "linux", not(target_arch = "wasm32"))
        )))]
        {
            false
        }
    }

    /// Create and show the tray icon.
    ///
    /// # Errors
    /// [`TrayError::Unavailable`] when no tray exists (see [`Self::is_available`]),
    /// [`TrayError::Unsupported`] on a target with no backend.
    pub fn new(data: TrayIconData) -> Result<Self, TrayError> {
        #[cfg(any(
            target_os = "windows",
            target_os = "macos",
            all(target_os = "linux", not(target_arch = "wasm32"))
        ))]
        {
            let inner = platform::PlatformTray::new(&data)?;
            Ok(Self { inner, data })
        }
        #[cfg(not(any(
            target_os = "windows",
            target_os = "macos",
            all(target_os = "linux", not(target_arch = "wasm32"))
        )))]
        {
            let _ = data;
            Err(TrayError::Unsupported)
        }
    }

    /// Replace the icon's state — icon, tooltip, status, menu.
    ///
    /// This is a whole-state set rather than a set of individual mutators
    /// because the Linux backend has to be able to answer the panel's
    /// `GetLayout` at any moment, and because SNI wants one `New*` signal per
    /// changed property rather than a stream of them.
    ///
    /// # Errors
    /// Propagates whatever the platform reports.
    pub fn update(&mut self, data: TrayIconData) -> Result<(), TrayError> {
        #[cfg(any(
            target_os = "windows",
            target_os = "macos",
            all(target_os = "linux", not(target_arch = "wasm32"))
        ))]
        {
            self.inner.update(&self.data, &data)?;
        }
        self.data = data;
        Ok(())
    }

    /// The state last successfully applied.
    #[must_use]
    pub const fn data(&self) -> &TrayIconData {
        &self.data
    }
}
