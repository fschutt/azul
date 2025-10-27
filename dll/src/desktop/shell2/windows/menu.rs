//! Win32 menu bar implementation

use alloc::{collections::BTreeMap, string::String, vec::Vec};
use core::{
    mem, ptr,
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
        /// Converts a UTF-8 string to UTF-16 null-terminated wide string
        fn convert_widestring(input: &str) -> Vec<u16> {
            let mut v: Vec<u16> = input
                .chars()
                .filter_map(|s| {
                    use core::convert::TryInto;
                    (s as u32).try_into().ok()
                })
                .collect();
            v.push(0);
            v
        }

        // Win32 menu flags
        const MF_STRING: u32 = 0x00000000;
        const MF_POPUP: u32 = 0x00000010;
        const MF_SEPARATOR: u32 = 0x00000800;
        const MF_MENUBREAK: u32 = 0x00000040;

        for item in items {
            match item {
                MenuItem::String(mi) => {
                    if mi.children.as_ref().is_empty() {
                        // Leaf menu item with optional callback
                        let command = match mi.callback.as_ref() {
                            None => 0,
                            Some(c) => {
                                let new_command_id =
                                    Self::get_new_command_id().min(core::u16::MAX as usize) as u16;
                                command_map.insert(new_command_id, c.clone() as CoreMenuCallback);
                                new_command_id as usize
                            }
                        };
                        unsafe {
                            (win32.user32.AppendMenuW)(
                                *menu,
                                MF_STRING,
                                command,
                                convert_widestring(mi.label.as_str()).as_ptr(),
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
                                convert_widestring(mi.label.as_str()).as_ptr(),
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
        // Note: Win32 automatically destroys menus when the window is destroyed
        // or when SetMenu() is called with a different menu.
        // Explicit cleanup with DestroyMenu() could be added here if needed.
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

/// Creates and displays a context (popup) menu
///
/// Returns the menu callbacks so the caller can handle WM_COMMAND messages
pub fn create_and_show_context_menu(
    hwnd: super::dlopen::HWND,
    menu: &Menu,
    screen_x: i32,
    screen_y: i32,
    win32: &Win32Libraries,
) -> BTreeMap<u16, CoreMenuCallback> {
    // Win32 popup menu flags
    const TPM_LEFTALIGN: u32 = 0x0000;
    const TPM_TOPALIGN: u32 = 0x0000;

    let mut popup_menu = unsafe { (win32.user32.CreatePopupMenu)() };
    let mut callbacks = BTreeMap::new();

    WindowsMenuBar::recursive_construct_menu(
        &mut popup_menu,
        menu.items.as_ref(),
        &mut callbacks,
        win32,
    );

    let align = TPM_TOPALIGN | TPM_LEFTALIGN; // TODO: support menu.position

    // Make the window the foreground window (required for popup menus)
    unsafe { (win32.user32.SetForegroundWindow)(hwnd) };

    // Display the popup menu
    unsafe {
        (win32.user32.TrackPopupMenu)(
            popup_menu,
            align,
            screen_x,
            screen_y,
            0,
            hwnd,
            ptr::null_mut(),
        )
    };

    callbacks
}
