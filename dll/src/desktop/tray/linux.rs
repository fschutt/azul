//! Linux system tray — `org.kde.StatusNotifierItem` over D-Bus.
//! **NOT IMPLEMENTED YET.**
//!
//! This stub exists so the branch cross-compiles while the backend is written.
//! It reports the tray as unavailable rather than pretending to work — which,
//! on this platform, is also frequently the TRUE answer (see below).
//!
//! Full recipe: `scripts/MULTIMONITOR_AND_TRAY_RESEARCH_2026_08_24.md` §2.2
//! (Linux section) and §2.1 for what in `linux/dbus/` is reusable.
//!
//! # The thing to know before starting
//!
//! **`linux/gnome_menu/` implements `org.gtk.Menus` + `org.gtk.Actions`, NOT
//! `com.canonical.dbusmenu`.** SNI's `Menu` property points at a *dbusmenu*
//! object — a recursive `(id, a{sv}, av)` tree served through
//! `GetLayout(parentId, recursionDepth, propertyNames)` — which is a different
//! model from `org.gtk.Menus`' flat `(group_id, menu_id)` scheme. The protocol
//! layer is a rewrite, not an adaptation. Roughly 30% of the existing D-Bus
//! code carries over, and it is the boring 30%.
//!
//! # Reusable as-is
//!
//! * `dbus/dlopen.rs` — the whole `DBusLib` wrapper (the "no compile-time
//!   libdbus" property comes free).
//! * `gnome_menu/shared_dbus.rs::get_shared_dbus_lib()` — genuinely generic.
//! * `dbus/mod.rs:25-47` — the `dbus_bus_name_has_owner` probe; retarget one
//!   string literal to `org.kde.StatusNotifierWatcher`.
//! * `gnome_menu/actions_protocol.rs:31-63` — the deferred-callback mailbox
//!   (`PendingMenuCallback` + `LazyLock<Mutex<Vec<_>>>`). Exactly the pattern
//!   needed here, since a D-Bus handler thread cannot hold a `CallbackInfo`.
//!   `super::queue_tray_event` is the tray's equivalent.
//!
//! # Blockers to clear first
//!
//! None of these symbols are in `dbus/dlopen.rs` today:
//!
//! * **`dbus_message_new_signal` — hard blocker.** SNI requires emitting
//!   `NewIcon`/`NewStatus`/`NewAttentionIcon`/`NewToolTip`; dbusmenu requires
//!   `LayoutUpdated`/`ItemsPropertiesUpdated`. The code can currently only
//!   *reply*, never *emit*.
//! * `dbus_bus_add_match` + `dbus_connection_add_filter` — to watch
//!   `NameOwnerChanged` for the watcher and re-register when the tray applet
//!   restarts (plasmashell/waybar restarts are routine).
//! * `dbus_message_iter_append_fixed_array` — for `IconPixmap` (`a(iiay)`).
//! * `dbus_message_get_path` / `_get_sender`.
//! * **An `org.freedesktop.DBus.Properties` handler — SNI is ~90% properties**,
//!   and there is none anywhere in the tree today. `Introspectable` too; several
//!   hosts call it first.
//!
//! `GnomeMenuManager::new` also fuses connect + `request_name` + register-two-
//! fixed-interfaces into one non-parameterisable function with the bus name and
//! object path hardcoded to the GTK convention (`manager.rs:79-80`), so a
//! generic `register_service(bus_name, path, vtable)` has to be extracted.
//!
//! # Registration sequence
//!
//! 1. Own `org.kde.StatusNotifierItem-<pid>-<n>` on the session bus. **Use
//!    `org.kde.*`, not `org.freedesktop.*`** — the published fd.o text says the
//!    latter but the reference implementation and the entire KDE stack use the
//!    former, and Electron 43 broke Waybar by switching (Waybar#5240).
//! 2. Export `org.kde.StatusNotifierItem` at `/StatusNotifierItem`.
//! 3. Export `com.canonical.dbusmenu` (conventionally `/MenuBar`); point the
//!    `Menu` property at it.
//! 4. `org.kde.StatusNotifierWatcher.RegisterStatusNotifierItem` at
//!    `/StatusNotifierWatcher`.
//! 5. Watch `NameOwnerChanged` for the watcher and **redo step 4** when it
//!    returns. Make that the same code path as initial creation so it cannot rot.
//!
//! # Availability is a real question here
//!
//! `is_available()` must check that the watcher name is owned **AND** that
//! `IsStatusNotifierHostRegistered` is true — a watcher with no host is a real
//! state, because the watcher can win the startup race against the panel
//! (cinnamon#13740). Poll/retry rather than deciding once at startup.
//!
//! **On a vanilla GNOME there is no watcher at all**: registration fails
//! silently and no icon ever appears. That is not a bug to work around; the app
//! needs a story for having no tray.
//!
//! # Icons
//!
//! Prefer `IconPixmap` (`a(iiay)`, ARGB32 **big-endian** — use
//! `TrayIconImage::to_argb32_be`, which exists for exactly this) over
//! `IconName`. `IconName` needs the icon installed in an XDG theme, and
//! `IconThemePath` is non-standard and ignored by several hosts. Publish 2-3
//! sizes (22/24/48) and let the host pick.
//!
//! Emit **both** the `New*` signals and `PropertiesChanged` — several hosts
//! (notably KDE historically) ignore the latter for SNI.
//!
//! Model to copy: [`ksni`](https://github.com/iovxw/ksni) — pure D-Bus, no GTK,
//! correct watcher offline/online handling. Model to avoid: Tauri's `tray-icon`
//! (GTK + libxdo + libayatana; its bug list is a catalogue of why).
//!
//! # Do NOT implement XEmbed
//!
//! X11-only (dead under Wayland), makes us responsible for rendering into a
//! reparented window with the tray's `_NET_SYSTEM_TRAY_VISUAL`, and duplicates
//! icons where a bridge already exists (Cinnamon). Point users at `snixembed`.

use azul_core::tray::TrayIconData;

use super::TrayError;

/// Whether a tray exists is a genuine question on Linux, but this backend does
/// not exist yet, so the answer is a flat no rather than a probe that would
/// then fail to produce an icon.
pub(super) fn is_available() -> bool {
    false
}

#[derive(Debug)]
pub(super) struct PlatformTray {
    _never: core::convert::Infallible,
}

impl PlatformTray {
    pub(super) fn new(
        _data: &TrayIconData,
        _provider: &azul_core::icon::SharedIconProvider,
        _font_manager: &azul_layout::font_traits::FontManager<azul_css::props::basic::FontRef>,
    ) -> Result<Self, TrayError> {
        Err(TrayError::Unsupported)
    }

    pub(super) fn update(
        &mut self,
        _old: &TrayIconData,
        _new: &TrayIconData,
        _provider: &azul_core::icon::SharedIconProvider,
        _font_manager: &azul_layout::font_traits::FontManager<azul_css::props::basic::FontRef>,
    ) -> Result<(), TrayError> {
        match self._never {}
    }
}

impl PlatformTray {
    /// Unreachable — `new()` never returns a value on this platform yet.
    pub(super) fn pump(&mut self) -> Vec<azul_core::menu::CoreMenuCallback> {
        // No menu backend yet, so nothing can be delivered.
        Vec::new()
    }

    #[allow(dead_code)]
    fn pump_unused(&mut self) {
        match self._never {}
    }
}
