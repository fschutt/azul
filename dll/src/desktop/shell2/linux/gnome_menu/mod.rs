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
mod manager_v2; // New dlopen-based manager
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

use crate::{log_debug, log_error, log_info, log_warn, log_trace};
use super::super::common::debug_server::LogCategory;

pub use actions_protocol::{
    drain_pending_menu_callbacks, queue_menu_callback, ActionsProtocol, DbusAction,
    PendingMenuCallback,
};
pub use dbus_connection::DbusConnection;
pub use manager_v2::GnomeMenuManagerV2; // New dlopen-based manager
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

/// GNOME menu manager - handles DBus services and menu registration
///
/// This is the main entry point for GNOME menu integration.
/// It coordinates all submodules and provides a clean API.
pub struct GnomeMenuManager {
    app_name: String,
    is_active: Arc<AtomicBool>,
    dbus_connection: Option<DbusConnection>,
    menu_protocol: Option<MenuProtocol>,
    actions_protocol: Option<ActionsProtocol>,
}

impl GnomeMenuManager {
    /// Create a new GNOME menu manager
    ///
    /// Returns `None` if GNOME menus should not be used (see `should_use_gnome_menus()`)
    pub fn new(app_name: &str) -> Option<Self> {
        if !should_use_gnome_menus() {
            return None;
        }

        debug_log(&format!(
            "Creating GNOME menu manager for app: {}",
            app_name
        ));

        // Try to establish DBus connection
        let dbus_connection = match DbusConnection::new(app_name) {
            Ok(conn) => {
                debug_log("DBus connection established");
                Some(conn)
            }
            Err(e) => {
                debug_log(&format!(
                    "Failed to establish DBus connection: {} - falling back to CSD",
                    e
                ));
                return None;
            }
        };

        // Initialize protocols
        let menu_protocol = MenuProtocol::new();
        let actions_protocol = ActionsProtocol::new();

        Some(Self {
            app_name: app_name.to_string(),
            is_active: Arc::new(AtomicBool::new(true)),
            dbus_connection,
            menu_protocol: Some(menu_protocol),
            actions_protocol: Some(actions_protocol),
        })
    }

    /// Set X11 window properties to advertise DBus menu services
    ///
    /// This tells GNOME Shell where to find our menu via DBus.
    pub fn set_window_properties(
        &self,
        window_id: u64,
        display: *mut std::ffi::c_void,
    ) -> Result<(), GnomeMenuError> {
        debug_log("Setting X11 window properties for GNOME menu");

        if let Some(ref conn) = self.dbus_connection {
            X11Properties::set_properties(
                window_id,
                display,
                &self.app_name,
                conn.get_bus_name(),
                conn.get_object_path(),
            )?;

            debug_log("X11 window properties set successfully");
            Ok(())
        } else {
            Err(GnomeMenuError::NotInitialized)
        }
    }

    /// Update menu structure
    ///
    /// Converts `Menu` to DBus format and updates the menu service.
    pub fn update_menu(&self, menu: &azul_core::menu::Menu) -> Result<(), GnomeMenuError> {
        debug_log("Updating GNOME menu structure");

        if !self.is_active.load(Ordering::Relaxed) {
            return Err(GnomeMenuError::NotInitialized);
        }

        // Convert menu to DBus format
        let dbus_menu = MenuConversion::convert_menu(menu)?;

        // Update menu protocol
        if let Some(ref protocol) = self.menu_protocol {
            protocol.update_menu(dbus_menu)?;
        }

        // Extract and register actions
        let actions = MenuConversion::extract_actions(menu)?;
        if let Some(ref protocol) = self.actions_protocol {
            protocol.register_actions(actions)?;
        }

        debug_log("Menu update complete");
        Ok(())
    }

    /// Shutdown the menu manager and cleanup DBus services
    pub fn shutdown(&self) {
        if self.is_active.load(Ordering::Relaxed) {
            debug_log("Shutting down GNOME menu manager");
            self.is_active.store(false, Ordering::Relaxed);

            // Cleanup is handled by Drop implementations in submodules
        }
    }

    /// Check if the manager is currently active
    pub fn is_active(&self) -> bool {
        self.is_active.load(Ordering::Relaxed)
    }
}

impl Drop for GnomeMenuManager {
    fn drop(&mut self) {
        self.shutdown();
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
    fn test_should_use_gnome_menus_respects_disable_flag() {
        env::set_var("AZUL_DISABLE_GNOME_MENUS", "1");
        assert!(!should_use_gnome_menus());
        env::remove_var("AZUL_DISABLE_GNOME_MENUS");
    }

    #[test]
    fn test_gnome_menu_manager_returns_none_when_disabled() {
        env::set_var("AZUL_DISABLE_GNOME_MENUS", "1");
        let manager = GnomeMenuManager::new("test.app");
        assert!(manager.is_none());
        env::remove_var("AZUL_DISABLE_GNOME_MENUS");
    }

    #[test]
    fn test_debug_log_only_prints_when_enabled() {
        env::remove_var("AZUL_GNOME_MENU_DEBUG");
        debug_log("Should not print");

        env::set_var("AZUL_GNOME_MENU_DEBUG", "1");
        debug_log("Should print to stderr");
        env::remove_var("AZUL_GNOME_MENU_DEBUG");
    }
}
