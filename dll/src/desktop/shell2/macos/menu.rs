//! macOS menu integration for shell2
//!
//! Provides NSMenu creation and updates with hash-based diff to avoid
//! unnecessary menu recreation.

use std::collections::HashMap;

use azul_core::menu::{Menu, MenuItem};
use objc2::rc::Retained;
use objc2_app_kit::{NSMenu, NSMenuItem};
use objc2_foundation::{MainThreadMarker, NSString};

/// Menu state tracking for diff-based updates
pub struct MenuState {
    /// Current menu hash
    current_hash: u64,
    /// The NSMenu instance
    ns_menu: Option<Retained<NSMenu>>,
    /// Command ID to callback mapping
    command_map: HashMap<i64, usize>, // tag -> callback_index
}

impl MenuState {
    pub fn new() -> Self {
        Self {
            current_hash: 0,
            ns_menu: None,
            command_map: HashMap::new(),
        }
    }

    /// Update menu if hash changed, returns true if menu was recreated
    pub fn update_if_changed(&mut self, menu: &Menu, mtm: MainThreadMarker) -> bool {
        let new_hash = menu.get_hash();

        if new_hash != self.current_hash {
            // Menu changed, rebuild it
            let (ns_menu, command_map) = create_nsmenu(menu, mtm);
            self.ns_menu = Some(ns_menu);
            self.command_map = command_map;
            self.current_hash = new_hash;
            true
        } else {
            false
        }
    }

    /// Get the current NSMenu (if any)
    pub fn get_nsmenu(&self) -> Option<&Retained<NSMenu>> {
        self.ns_menu.as_ref()
    }

    /// Look up callback for a command tag
    pub fn get_callback_for_tag(&self, tag: i64) -> Option<usize> {
        self.command_map.get(&tag).copied()
    }
}

/// Create an NSMenu from Azul Menu structure
fn create_nsmenu(menu: &Menu, mtm: MainThreadMarker) -> (Retained<NSMenu>, HashMap<i64, usize>) {
    let ns_menu = NSMenu::new(mtm);
    let mut command_map = HashMap::new();
    let mut next_tag = 1i64;

    // Build menu items recursively
    build_menu_items(&menu.items, &ns_menu, &mut command_map, &mut next_tag, mtm);

    (ns_menu, command_map)
}

/// Recursively build menu items
fn build_menu_items(
    items: &azul_core::menu::MenuItemVec,
    parent_menu: &NSMenu,
    command_map: &mut HashMap<i64, usize>,
    next_tag: &mut i64,
    mtm: MainThreadMarker,
) {
    let items = items.as_slice();
    for (index, item) in items.iter().enumerate() {
        match item {
            MenuItem::String(string_item) => {
                if string_item.children.is_empty() {
                    // Leaf menu item
                    let menu_item = NSMenuItem::new(mtm);
                    let title = NSString::from_str(&string_item.label);
                    menu_item.setTitle(&title);

                    // If has callback, assign tag
                    if string_item.callback.is_some() {
                        let tag = *next_tag;
                        *next_tag += 1;

                        menu_item.setTag(tag as isize);
                        command_map.insert(tag, index);

                        // TODO: Set action and target for callback dispatch
                        // menu_item.setAction(Some(sel!(menuItemClicked:)));
                        // menu_item.setTarget(Some(&*menu_handler));
                    }

                    // TODO: Set keyboard accelerator if present
                    // if let Some(accel) = &string_item.accelerator { ... }

                    parent_menu.addItem(&menu_item);
                } else {
                    // Submenu
                    let submenu = NSMenu::new(mtm);
                    let title = NSString::from_str(&string_item.label);
                    submenu.setTitle(&title);

                    let menu_item = NSMenuItem::new(mtm);
                    menu_item.setTitle(&title);
                    menu_item.setSubmenu(Some(&submenu));

                    // Recursively build children
                    build_menu_items(&string_item.children, &submenu, command_map, next_tag, mtm);

                    parent_menu.addItem(&menu_item);
                }
            }
            MenuItem::Separator => {
                let separator = unsafe { NSMenuItem::separatorItem(mtm) };
                parent_menu.addItem(&separator);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_state_new() {
        let state = MenuState::new();
        assert_eq!(state.current_hash, 0);
        assert!(state.ns_menu.is_none());
        assert!(state.command_map.is_empty());
    }

    // TODO: Add more tests for menu creation and updates
}
