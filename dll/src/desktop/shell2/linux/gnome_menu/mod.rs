//! GNOME Native Menu Integration via DBus - Self-Contained Module
//!
//! This module implements the complete GTK DBus menu protocol for GNOME Shell integration.
//! It is designed to be completely independent and can be easily disabled or removed.
//!
//! ## Module Structure
//!
//! - `mod.rs` - Public API and detection logic (this file)
//! - `menu_protocol.rs` - org.gtk.Menus interface implementation
//! - `actions_protocol.rs` - org.gtk.Actions interface implementation
//! - `menu_conversion.rs` - Menu → DBus format conversion
//! - `x11_properties.rs` - X11 window property setting
//! - `manager.rs` - dlopen-based menu manager
//! - `protocol_impl.rs` - dlopen-based protocol implementation
//! - `shared_dbus.rs` - Shared DBus library instance
//!
//! ## Environment Variables
//!
//! Read once at the start of the app (`LinuxSettings`), not here:
//!
//! - `AZ_DISABLE_GNOME_MENUS=1` - Force fallback to CSD menus
//! - `AZ_GNOME_MENU_DEBUG=1` - Enable debug logging
//!
//! ## Architecture
//!
//! ```text
//! GnomeMenuManager (manager.rs)
//!     ├── SharedDbusLib (shared_dbus.rs)
//!     │   └── dlopen'd libdbus-1 instance
//!     ├── menu_protocol.rs
//!     │   └── DbusMenuGroup / DbusMenuItem types
//!     ├── actions_protocol.rs
//!     │   └── org.gtk.Actions types and callback queue
//!     ├── ProtocolImpl (protocol_impl.rs)
//!     │   └── dlopen-based interface registration
//!     └── MenuConversion (menu_conversion.rs)
//!         └── Menu → DBus format
//! ```

mod actions_protocol;
mod manager;
mod menu_conversion;
mod menu_protocol;
mod protocol_impl;
mod shared_dbus;
mod x11_properties;

pub use actions_protocol::drain_pending_menu_callbacks;
pub(crate) use actions_protocol::{queue_menu_callback, DbusAction, PendingMenuCallback};
pub use manager::GnomeMenuManager;
pub(crate) use menu_conversion::MenuConversion;
pub(crate) use menu_protocol::{DbusMenuGroup, DbusMenuItem};
pub(crate) use protocol_impl::{register_actions_interface, register_menus_interface};
pub use shared_dbus::{get_shared_dbus_lib, get_shared_dbus_lib_traced};
pub(crate) use x11_properties::X11Properties;

use super::super::common::debug_server::LogCategory;
use super::linux_settings::LinuxSettings;
use crate::{desktop::platform_settings::PlatformEnv, log_debug};

/// Whether the GNOME menu integration logs its steps (`AZ_GNOME_MENU_DEBUG=1`).
///
/// Decided once, when the app starts ([`LinuxSettings::gnome_menu_debug`]), and carried by
/// everything that logs: the manager, its DBus handlers, the conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MenuTrace(bool);

impl MenuTrace {
    /// Logs nothing.
    pub const OFF: Self = Self(false);

    /// A trace that logs when `on`.
    #[must_use]
    pub const fn new(on: bool) -> Self {
        Self(on)
    }

    /// Log `msg` (in the platform category) when the trace is on.
    pub fn log(self, msg: &str) {
        if self.0 {
            log_debug!(LogCategory::Platform, "[AZUL GNOME MENU] {}", msg);
        }
    }
}

/// Check if GNOME native menus should be used
///
/// Returns `false` if:
/// - `AZ_DISABLE_GNOME_MENUS=1` is set ([`LinuxSettings::disable_gnome_menus`])
/// - Not running on GNOME desktop (checks `XDG_CURRENT_DESKTOP`)
/// - DBus session bus not available (`DBUS_SESSION_BUS_ADDRESS`)
///
/// `linux` and `env` are what the app read at its start; the call reads nothing.
pub fn should_use_gnome_menus(linux: &LinuxSettings, env: &PlatformEnv) -> bool {
    let trace = MenuTrace::new(linux.gnome_menu_debug);

    // Check explicit disable flag
    if linux.disable_gnome_menus {
        trace.log("GNOME menus disabled via AZ_DISABLE_GNOME_MENUS=1");
        return false;
    }

    // Check if running on GNOME
    let desktop = env.xdg_current_desktop.as_deref().unwrap_or_default();
    if !desktop.to_lowercase().contains("gnome") {
        trace.log(&format!(
            "Not running on GNOME desktop: XDG_CURRENT_DESKTOP={}",
            desktop
        ));
        return false;
    }

    // Check if DBus session bus is available
    if env.dbus_session_bus_address.is_none() {
        trace.log("DBus session bus not available (DBUS_SESSION_BUS_ADDRESS not set)");
        return false;
    }

    trace.log("GNOME menus available and enabled");
    true
}

/// Errors that can occur during GNOME menu operations
#[derive(Debug)]
pub enum GnomeMenuError {
    /// DBus connection failed
    DbusConnectionFailed(String),
    /// Failed to register DBus service
    ServiceRegistrationFailed(String),
    /// Failed to set X11 window properties
    X11PropertyFailed(String),
    /// Menu conversion failed
    MenuConversionFailed(String),
    /// Action registration failed
    ActionRegistrationFailed(String),
    /// Manager not initialized
    NotInitialized,
    /// Feature not yet implemented
    NotImplemented,
}

impl std::fmt::Display for GnomeMenuError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GnomeMenuError::DbusConnectionFailed(e) => {
                write!(f, "DBus connection failed: {}", e)
            }
            GnomeMenuError::ServiceRegistrationFailed(e) => {
                write!(f, "Failed to register DBus service: {}", e)
            }
            GnomeMenuError::X11PropertyFailed(e) => {
                write!(f, "Failed to set X11 window properties: {}", e)
            }
            GnomeMenuError::MenuConversionFailed(e) => {
                write!(f, "Menu conversion failed: {}", e)
            }
            GnomeMenuError::ActionRegistrationFailed(e) => {
                write!(f, "Action registration failed: {}", e)
            }
            GnomeMenuError::NotInitialized => {
                write!(f, "GNOME menu manager not initialized")
            }
            GnomeMenuError::NotImplemented => {
                write!(f, "Feature not yet implemented")
            }
        }
    }
}

impl std::error::Error for GnomeMenuError {}

#[cfg(test)]
#[path = "mod_test.rs"]
mod tests;
