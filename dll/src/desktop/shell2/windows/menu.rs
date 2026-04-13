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
    /// Hash of the Menu structure for change detection
    pub hash: u64,
    /// Stored DestroyMenu function pointer for cleanup in Drop
    destroy_menu_fn: unsafe extern "system" fn(HMENU) -> i32,
}

/// Global atomic counter for unique Win32 command IDs
/// 0 is reserved for "no command"
static WINDOWS_UNIQUE_COMMAND_ID_GENERATOR: AtomicUsize = AtomicUsize::new(1);

impl WindowsMenuBar {
    /// Creates a new WindowsMenuBar from an Azul Menu structure
    pub fn new(menu: &Menu, win32: &Win32Libraries) -> Self {
        let hash = menu.get_hash();
        let mut root = unsafe { (win32.user32.CreateMenu)() };
        let mut command_map = BTreeMap::new();

        Self::recursive_construct_menu(&mut root, menu.items.as_ref(), &mut command_map, win32);

        Self {
            _native_ptr: root,
            callbacks: command_map,
            hash,
            destroy_menu_fn: win32.user32.DestroyMenu,
        }
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
        win32: &Win32Libraries,
    ) {
        use super::dlopen::{encode_wide, constants::*};

        for item in items {
            match item {
                MenuItem::String(mi) => {
                    if mi.children.as_ref().is_empty() {
                        // Leaf menu item with optional callback
                        let command = match mi.callback.as_ref() {
                            None => 0,
                            Some(c) => {
                                let raw_id = Self::get_new_command_id() % (core::u16::MAX as usize);
                                // Skip 0 since it means "no command"
                                let new_command_id = if raw_id == 0 {
                                    Self::get_new_command_id() % (core::u16::MAX as usize)
                                } else {
                                    raw_id
                                } as u16;
                                command_map.insert(new_command_id, c.clone() as CoreMenuCallback);
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
/// 3. Updating an existing menu if the hash changed
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
        // Update menu if hash changed
        (Some(old_hash), Some(menu)) => {
            let menu_hash = menu.get_hash();
            if old_hash != menu_hash {
                let new_menu_bar = WindowsMenuBar::new(menu, win32);
                unsafe { (win32.user32.SetMenu)(hwnd, new_menu_bar._native_ptr) };
                *old_menu = Some(new_menu_bar);
            }
        }
        // No menu in either case
        (None, None) => {}
    }
}
