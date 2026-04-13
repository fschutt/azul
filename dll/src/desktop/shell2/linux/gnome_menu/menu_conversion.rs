//! Menu Conversion - Azul Menu → DBus Format
//!
//! Converts Azul's hierarchical `Menu` tree into the flat `DbusMenuGroup` /
//! `DbusAction` lists expected by GNOME Shell's application menu protocol.
//! Uses a two-pass approach: first converts menu items per level, then
//! recursively processes submenus into additional groups.

use std::sync::Arc;

use super::{
    actions_protocol::DbusAction,
    debug_log,
    menu_protocol::{DbusMenuGroup, DbusMenuItem},
    GnomeMenuError,
};

/// Converts an Azul `Menu` tree into flat `DbusMenuGroup` and `DbusAction`
/// lists for the GNOME Shell application menu DBus protocol.
///
/// This is a unit struct used as a namespace for the conversion functions
/// [`convert_menu`](Self::convert_menu) and [`extract_actions`](Self::extract_actions).
#[derive(Copy, Clone)]
pub struct MenuConversion;

impl MenuConversion {
    /// Convert an Azul Menu to DBus format
    ///
    /// Transforms the Menu tree into flat groups that DBus can understand.
    pub fn convert_menu(
        menu: &azul_core::menu::Menu,
    ) -> Result<Vec<DbusMenuGroup>, GnomeMenuError> {
        debug_log("Converting Menu to DBus format");

        let mut groups = Vec::new();
        let mut next_group_id = 0u32;

        Self::convert_menu_recursive(&menu.items, &mut groups, &mut next_group_id, 0)?;

        debug_log(&format!(
            "Menu conversion complete: {} groups",
            groups.len()
        ));
        Ok(groups)
    }

    /// Extract actions from Menu
    ///
    /// Collects all menu item callbacks and converts them to DBus actions.
    pub fn extract_actions(
        menu: &azul_core::menu::Menu,
    ) -> Result<Vec<DbusAction>, GnomeMenuError> {
        debug_log("Extracting actions from Menu");

        let mut actions = Vec::new();
        Self::extract_actions_recursive(&menu.items, "app", &mut actions)?;

        debug_log(&format!(
            "Action extraction complete: {} actions",
            actions.len()
        ));
        Ok(actions)
    }

    /// Recursively convert a level of menu items and their submenus into flat groups.
    ///
    /// Each level becomes a `DbusMenuGroup` with `current_group_id`. Submenu items
    /// get assigned fresh group IDs, then those submenus are recursively processed
    /// with the correct IDs.
    fn convert_menu_recursive(
        items: &azul_core::menu::MenuItemVec,
        groups: &mut Vec<DbusMenuGroup>,
        next_group_id: &mut u32,
        current_group_id: u32,
    ) -> Result<(), GnomeMenuError> {
        let mut dbus_items = Vec::new();
        let mut submenus: Vec<(u32, &azul_core::menu::MenuItemVec)> = Vec::new();

        for item in items.as_ref().iter() {
            match item {
                azul_core::menu::MenuItem::String(string_item) => {
                    let action_name = if string_item.callback.is_some() {
                        Some(Self::generate_action_name(&string_item.label))
                    } else {
                        None
                    };

                    let has_children = !string_item.children.as_ref().is_empty();
                    let submenu = if has_children {
                        *next_group_id += 1;
                        let assigned_id = *next_group_id;
                        submenus.push((assigned_id, &string_item.children));
                        Some((assigned_id, 0))
                    } else {
                        None
                    };

                    let enabled = match string_item.menu_item_state {
                        azul_core::menu::MenuItemState::Normal => true,
                        azul_core::menu::MenuItemState::Greyed
                        | azul_core::menu::MenuItemState::Disabled => false,
                    };

                    dbus_items.push(DbusMenuItem {
                        label: string_item.label.as_str().to_string(),
                        action: action_name,
                        target: None,
                        submenu,
                        section: None,
                        enabled,
                    });
                }
                azul_core::menu::MenuItem::Separator => {
                    dbus_items.push(DbusMenuItem {
                        label: String::new(),
                        action: None,
                        target: None,
                        submenu: None,
                        section: Some((0, 0)),
                        enabled: false,
                    });
                }
                azul_core::menu::MenuItem::BreakLine => {
                    continue;
                }
            }
        }

        groups.push(DbusMenuGroup {
            group_id: current_group_id,
            menu_id: 0,
            items: dbus_items,
        });

        for (submenu_group_id, children) in submenus {
            Self::convert_menu_recursive(children, groups, next_group_id, submenu_group_id)?;
        }

        Ok(())
    }

    /// Extract actions recursively
    fn extract_actions_recursive(
        items: &azul_core::menu::MenuItemVec,
        prefix: &str,
        actions: &mut Vec<DbusAction>,
    ) -> Result<(), GnomeMenuError> {
        for item in items.as_ref().iter() {
            if let azul_core::menu::MenuItem::String(string_item) = item {
                // Extract callback if present
                if let Some(callback) = string_item.callback.as_ref() {
                    let action_name = Self::generate_action_name(&string_item.label);
                    let enabled = match string_item.menu_item_state {
                        azul_core::menu::MenuItemState::Normal => true,
                        azul_core::menu::MenuItemState::Greyed
                        | azul_core::menu::MenuItemState::Disabled => false,
                    };

                    // Clone the menu callback data for storage
                    let menu_callback = callback.clone();
                    let action_name_for_closure = action_name.clone();
                    let menu_callback_for_closure = menu_callback.clone();

                    actions.push(DbusAction {
                        name: action_name,
                        enabled,
                        parameter_type: None,
                        state: None,
                        // When the DBus action is activated, queue the callback
                        // for processing in the main event loop where we have
                        // access to the full window state (CallbackInfo)
                        callback: Arc::new(move |_param| {
                            super::queue_menu_callback(super::PendingMenuCallback {
                                action_name: action_name_for_closure.clone(),
                                menu_callback: menu_callback_for_closure.clone(),
                            });
                        }),
                        menu_callback: Some(menu_callback),
                    });
                }

                // Recursively extract from children
                if !string_item.children.as_ref().is_empty() {
                    Self::extract_actions_recursive(&string_item.children, prefix, actions)?;
                }
            }
        }

        Ok(())
    }

    /// Generate a valid DBus action name from a label
    fn generate_action_name(label: &azul_css::AzString) -> String {
        let label_str = label.as_str();
        let sanitized = label_str
            .to_lowercase()
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || c == '_' {
                    c
                } else if c.is_whitespace() || c == '-' {
                    '.'
                } else {
                    '_'
                }
            })
            .collect::<String>();

        format!("app.{}", sanitized)
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_item_conversion() {
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
    fn test_action_creation() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let called = Arc::new(AtomicBool::new(false));
        let called_clone = called.clone();

        let action = DbusAction {
            name: "app.test".to_string(),
            enabled: true,
            parameter_type: None,
            state: None,
            callback: Arc::new(move |_| {
                called_clone.store(true, Ordering::Relaxed);
            }),
            menu_callback: None,
        };

        assert_eq!(action.name, "app.test");
        assert!(action.enabled);

        (action.callback)(None);
        assert!(called.load(Ordering::Relaxed));
    }

    #[test]
    fn test_action_name_generation() {
        let label = azul_css::AzString::from_const_str("File > New");
        let action_name = MenuConversion::generate_action_name(&label);
        assert_eq!(action_name, "app.file._.new");
    }

    #[test]
    fn test_convert_empty_menu() {
        let menu = azul_core::menu::Menu::create(azul_core::menu::MenuItemVec::from_vec(vec![]));
        let result = MenuConversion::convert_menu(&menu).unwrap();

        // Should have at least the root group
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].group_id, 0);
        assert_eq!(result[0].items.len(), 0);
    }
}
