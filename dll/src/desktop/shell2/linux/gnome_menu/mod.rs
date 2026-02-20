//! GNOME Native Menu Integration via DBus - Self-Contained Module
//!
//! This module implements the complete GTK DBus menu protocol for GNOME Shell integration.
//! It is designed to be completely independent and can be easily disabled or removed.
//!
//! ## Module Structure
//!
//! - `mod.rs` - Public API and detection logic (this file)
//! - `dbus_connection.rs` - DBus connection management
//! - `menu_protocol.rs` - org.gtk.Menus interface implementation
//! - `actions_protocol.rs` - org.gtk.Actions interface implementation
//! - `menu_conversion.rs` - Menu → DBus format conversion
//! - `x11_properties.rs` - X11 window property setting
//!
//! ## Environment Variables
//!
//! - `AZUL_DISABLE_GNOME_MENUS=1` - Force fallback to CSD menus
//! - `AZUL_GNOME_MENU_DEBUG=1` - Enable debug logging
//!
//! ## Architecture
//!
//! ```text
//! GnomeMenuManager
//!     ├── DbusConnection (dbus_connection.rs)
//!     │   └── Session bus, service registration
//!     ├── MenuProtocol (menu_protocol.rs)
//!     │   └── org.gtk.Menus interface
//!     ├── ActionsProtocol (actions_protocol.rs)
//!     │   └── org.gtk.Actions interface
//!     └── MenuConversion (menu_conversion.rs)
//!         └── Menu → DBus format
//! ```

mod actions_protocol;
mod dbus_connection;
mod manager; // New dlopen-based manager
mod menu_conversion;
mod menu_protocol;
mod protocol_impl; // New dlopen-based implementation
mod shared_dbus; // Shared DBus library instance
mod x11_properties;

use std::{
    env,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use super::super::common::debug_server::LogCategory;
use crate::{log_debug, log_error, log_info, log_trace, log_warn};

pub use actions_protocol::{
    drain_pending_menu_callbacks, queue_menu_callback, ActionsProtocol, DbusAction,
    PendingMenuCallback,
};
pub use dbus_connection::DbusConnection;
pub use manager::GnomeMenuManager; // New dlopen-based manager
pub use menu_conversion::MenuConversion;
pub use menu_protocol::{DbusMenuGroup, DbusMenuItem, MenuProtocol};
pub use protocol_impl::{register_actions_interface, register_menus_interface};
pub use shared_dbus::{get_shared_dbus_lib, is_dbus_available}; // Shared DBus library
pub use x11_properties::X11Properties;

/// Check if GNOME native menus should be used
///
/// Returns `false` if:
/// - `AZUL_DISABLE_GNOME_MENUS=1` environment variable is set
/// - Not running on GNOME desktop (checks `XDG_CURRENT_DESKTOP`)
/// - DBus session bus not available
pub fn should_use_gnome_menus() -> bool {
    // Check explicit disable flag
    if env::var("AZUL_DISABLE_GNOME_MENUS").unwrap_or_default() == "1" {
        debug_log("GNOME menus disabled via AZUL_DISABLE_GNOME_MENUS=1");
        return false;
    }

    // Check if running on GNOME
    let desktop = env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    if !desktop.to_lowercase().contains("gnome") {
        debug_log(&format!(
            "Not running on GNOME desktop: XDG_CURRENT_DESKTOP={}",
            desktop
        ));
        return false;
    }

    // Check if DBus session bus is available
    if env::var("DBUS_SESSION_BUS_ADDRESS").is_err() {
        debug_log("DBus session bus not available (DBUS_SESSION_BUS_ADDRESS not set)");
        return false;
    }

    debug_log("GNOME menus available and enabled");
    true
}

/// Print debug log if `AZUL_GNOME_MENU_DEBUG=1`
pub(crate) fn debug_log(msg: &str) {
    if env::var("AZUL_GNOME_MENU_DEBUG").unwrap_or_default() == "1" {
        log_debug!(LogCategory::Platform, "[AZUL GNOME MENU] {}", msg);
    }
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
mod tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore)] // Miri has issues with env var manipulation
    fn test_should_use_gnome_menus_respects_disable_flag() {
        env::set_var("AZUL_DISABLE_GNOME_MENUS", "1");
        assert!(!should_use_gnome_menus());
        env::remove_var("AZUL_DISABLE_GNOME_MENUS");
    }

    #[test]
    #[cfg_attr(miri, ignore)] // Miri has issues with env var manipulation
    fn test_debug_log_only_prints_when_enabled() {
        env::remove_var("AZUL_GNOME_MENU_DEBUG");
        debug_log("Should not print");

        env::set_var("AZUL_GNOME_MENU_DEBUG", "1");
        debug_log("Should print to stderr");
        env::remove_var("AZUL_GNOME_MENU_DEBUG");
    }
}
