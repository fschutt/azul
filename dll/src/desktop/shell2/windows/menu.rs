//! Win32 menu bar implementation
//!
//! Provides [`WindowsMenuBar`] for creating native Win32 menus from Azul
//! [`Menu`] structures, and [`set_menu_bar`] for attaching / updating /
//! removing the menu bar on a window.  Uses Win32 function pointers from
//! [`super::dlopen::Win32Libraries`].

use alloc::{collections::BTreeMap, vec::Vec};
use core::{
    ptr,
    sync::atomic::{AtomicUsize, Ordering as AtomicOrdering},
};

use azul_core::menu::{CoreMenuCallback, Menu, MenuItem};

use super::dlopen::{Win32Libraries, HMENU};

/// Win32 menu bar with native HMENU handle
#[derive(Debug)]
pub struct WindowsMenuBar {
    /// Native Win32 menu handle
    pub _native_ptr: HMENU,
    /// Map from Win32 command ID -> azul CoreMenuCallback
    pub callbacks: BTreeMap<u16, CoreMenuCallback>,
    /// What the bar shows (`Menu::get_structure_hash`): a menu built again
    /// with equal items only rebinds its callbacks (`set_menu_bar`).
    pub hash: u64,
    /// Every leaf's command id in `Menu::leaf_callbacks` order, `None` for a
    /// leaf without a callback: where a rebuilt menu's callbacks go.
    leaf_commands: Vec<Option<u16>>,
    /// Stored DestroyMenu function pointer for cleanup in Drop
    destroy_menu_fn: unsafe extern "system" fn(HMENU) -> i32,
}

/// Global atomic counter for unique Win32 command IDs
/// 0 is reserved for "no command"
static WINDOWS_UNIQUE_COMMAND_ID_GENERATOR: AtomicUsize = AtomicUsize::new(1);

impl WindowsMenuBar {
    /// Creates a new WindowsMenuBar from an Azul Menu structure
    pub fn new(menu: &Menu, win32: &Win32Libraries) -> Self {
        let hash = menu.get_structure_hash();
        let mut root = unsafe { (win32.user32.CreateMenu)() };
        let mut command_map = BTreeMap::new();
        let mut leaf_commands = Vec::new();

        Self::recursive_construct_menu(
            &mut root,
            menu.items.as_ref(),
            &mut command_map,
            &mut leaf_commands,
            win32,
        );

        Self {
            _native_ptr: root,
            callbacks: command_map,
            hash,
            leaf_commands,
            destroy_menu_fn: win32.user32.DestroyMenu,
        }
    }

    /// Takes `menu`'s callbacks into the commands this bar has, when `menu`
    /// shows the same items. `false` when the bar must be built again.
    fn rebind(&mut self, menu: &Menu) -> bool {
        if menu.get_structure_hash() != self.hash {
            return false;
        }
        let Some(bound) = menu.rebind_leaf_callbacks(&self.leaf_commands) else {
            return false;
        };
        self.callbacks = bound.into_iter().collect();
        true
    }

    /// Generates a new unique Win32 command ID
    fn get_new_command_id() -> usize {
        WINDOWS_UNIQUE_COMMAND_ID_GENERATOR.fetch_add(1, AtomicOrdering::SeqCst)
    }

    /// Recursively constructs the Win32 menu structure
    pub fn recursive_construct_menu(
        menu: &mut HMENU,
        items: &[MenuItem],
        command_map: &mut BTreeMap<u16, CoreMenuCallback>,
        leaf_commands: &mut Vec<Option<u16>>,
        win32: &Win32Libraries,
    ) {
        use super::dlopen::{constants::*, encode_wide};

        for item in items {
            match item {
                MenuItem::String(mi) => {
                    if mi.children.as_ref().is_empty() {
                        // Leaf menu item with optional callback
                        let command = match mi.callback.as_ref() {
                            None => {
                                leaf_commands.push(None);
                                0
                            }
                            Some(c) => {
                                let raw_id = Self::get_new_command_id() % (core::u16::MAX as usize);
                                // Skip 0 since it means "no command"
                                let new_command_id = if raw_id == 0 {
                                    Self::get_new_command_id() % (core::u16::MAX as usize)
                                } else {
                                    raw_id
                                } as u16;
                                command_map.insert(new_command_id, c.clone() as CoreMenuCallback);
                                leaf_commands.push(Some(new_command_id));
                                new_command_id as usize
                            }
                        };
                        unsafe {
                            (win32.user32.AppendMenuW)(
                                *menu,
                                MF_STRING,
                                command,
                                encode_wide(mi.label.as_str()).as_ptr(),
                            )
                        };
                    } else {
                        // Submenu with children
                        let mut submenu = unsafe { (win32.user32.CreateMenu)() };
                        Self::recursive_construct_menu(
                            &mut submenu,
                            mi.children.as_ref(),
                            command_map,
                            leaf_commands,
                            win32,
                        );
                        unsafe {
                            (win32.user32.AppendMenuW)(
                                *menu,
                                MF_POPUP,
                                submenu as usize,
                                encode_wide(mi.label.as_str()).as_ptr(),
                            )
                        };
                    }
                }
                MenuItem::Separator => {
                    unsafe { (win32.user32.AppendMenuW)(*menu, MF_SEPARATOR, 0, ptr::null_mut()) };
                }
                MenuItem::BreakLine => {
                    unsafe { (win32.user32.AppendMenuW)(*menu, MF_MENUBREAK, 0, ptr::null_mut()) };
                }
            }
        }
    }
}

impl Drop for WindowsMenuBar {
    fn drop(&mut self) {
        if !self._native_ptr.is_null() {
            unsafe { (self.destroy_menu_fn)(self._native_ptr) };
        }
    }
}

/// Creates or updates the menu bar for a window
///
/// This function handles three cases:
/// 1. Removing an existing menu (new_menu = None)
/// 2. Creating a new menu (old_menu = None, new_menu = Some)
/// 3. Updating an existing menu: built again when what it shows changed,
///    otherwise only its callbacks move. An app builds its bar again with
///    fresh callback data in every layout pass, and a new HMENU per pass
///    redrew the bar on every frame of a slider drag.
pub fn set_menu_bar(
    hwnd: super::dlopen::HWND,
    old_menu: &mut Option<WindowsMenuBar>,
    new_menu: Option<&Menu>,
    win32: &Win32Libraries,
) {
    let old_hash = old_menu.as_ref().map(|m| m.hash);

    match (old_hash, new_menu) {
        // Remove existing menu
        (Some(_), None) => {
            unsafe { (win32.user32.SetMenu)(hwnd, ptr::null_mut()) };
            *old_menu = None;
        }
        // Create new menu
        (None, Some(menu)) => {
            let new_menu_bar = WindowsMenuBar::new(menu, win32);
            unsafe { (win32.user32.SetMenu)(hwnd, new_menu_bar._native_ptr) };
            *old_menu = Some(new_menu_bar);
        }
        (Some(_), Some(menu)) => {
            let rebound = old_menu.as_mut().is_some_and(|bar| bar.rebind(menu));
            if !rebound {
                let new_menu_bar = WindowsMenuBar::new(menu, win32);
                unsafe { (win32.user32.SetMenu)(hwnd, new_menu_bar._native_ptr) };
                *old_menu = Some(new_menu_bar);
            }
        }
        // No menu in either case
        (None, None) => {}
    }
}
