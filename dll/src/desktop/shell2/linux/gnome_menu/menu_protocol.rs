//! org.gtk.Menus data types
//!
//! Defines the menu item and group types used by `protocol_impl.rs` and
//! `menu_conversion.rs` for GNOME Shell DBus menu integration.

/// Represents a menu item in DBus format
#[derive(Debug, Clone)]
pub struct DbusMenuItem {
    pub label: String,
    pub action: Option<String>,
    pub target: Option<String>,
    pub submenu: Option<(u32, u32)>, // (group_id, menu_id)
    pub section: Option<(u32, u32)>, // For separators
    pub enabled: bool,
}

/// Represents a menu group (subscription group)
#[derive(Debug, Clone)]
pub struct DbusMenuGroup {
    pub group_id: u32,
    pub menu_id: u32,
    pub items: Vec<DbusMenuItem>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dbus_menu_item_creation() {
        let item = DbusMenuItem {
            label: "File".to_string(),
            action: None,
            target: None,
            submenu: Some((1, 0)),
            section: None,
            enabled: true,
        };

        assert_eq!(item.label, "File");
        assert!(item.submenu.is_some());
        assert_eq!(item.submenu.unwrap(), (1, 0));
        assert!(item.enabled);
    }

    #[test]
    fn test_dbus_menu_group_creation() {
        let group = DbusMenuGroup {
            group_id: 0,
            menu_id: 0,
            items: vec![],
        };

        assert_eq!(group.group_id, 0);
        assert_eq!(group.items.len(), 0);
    }
}
