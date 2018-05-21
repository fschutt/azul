//! Extension module to add a Win32 menu a winit window

use menu::{MenuItem, ApplicationMenu};
use menu::command_ids;

use glium::glutin::winapi::{
    shared::windef::{HMENU, HWND},
    um::winuser::{ ShowWindow, IsIconic, keybd_event, SetMenu, CreateMenu, AppendMenuW,
        GetForegroundWindow, SetForegroundWindow, MF_STRING, MF_SEPARATOR, MF_POPUP,
        KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, SW_RESTORE
    },
};

/// On Windows, if you open a window from the console, it won't bring the new window to
/// the front if any other window (like the code editor) is currently focused.
#[cfg(debug_assertions)]
fn win32_bring_window_to_top(hwnd: HWND) {
    // Not checked for errors since it isn't really important if it succeeds or not
    //
    // NOTE: SetForegroundWindow does not work if the user has focused a different window
    //
    // While the reason is understandable (not to steal user focus), sadly Windows
    // doesn't make it configurable whether you as the application developer wants to
    // respect this or not, it will always assume that you don't want to de-focus the
    // current window. Hence this workaround.

    // Check if the window already has focus
    if hwnd as usize == unsafe { GetForegroundWindow() } as usize {
        return;
    }

    // If window is minimized
    if unsafe { IsIconic(hwnd) } != 0 {
        unsafe { ShowWindow(hwnd, SW_RESTORE) };
    }

    // Simulate an ALT key press & release to trick Windows
    // into bringing the window into the foreground
    unsafe { keybd_event(0xA4, 0x45, KEYEVENTF_EXTENDEDKEY | 0, 0) };
    unsafe { keybd_event(0xA4, 0x45, KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP, 0) };

    unsafe { SetForegroundWindow(hwnd)  };
}

// Encode a Rust `&str` as a Vec<u16> compatible with the Win32 API
fn str_to_wide_vec_u16(input: &str) -> Vec<u16> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStrExt;
    let mut s: Vec<u16> = OsString::from(input).as_os_str().encode_wide().into_iter().collect();
    s.push(0);
    s
}

impl ApplicationMenu {

    /// NOTE: If the returned HMENU is a top-level menu, call `DestroyMenu` on it to clean up the resources.
    fn build(&self) -> HMENU {

        use std::ptr;
        use menu::MenuItem::*;

        // TODO: check for errors
        let current_menu = unsafe { CreateMenu() };

        for item in &self.items {
            match item {
                ClickableItem { id, text } => {
                    let text_u16 = str_to_wide_vec_u16(&text);
                    unsafe { AppendMenuW(current_menu, MF_STRING, id.0 as usize, text_u16.as_ptr()) };
                },
                Seperator => {
                    unsafe { AppendMenuW(current_menu, MF_SEPARATOR, 0, ptr::null_mut()) };
                },
                SubMenu { text, menu } => {
                    let text_u16 = str_to_wide_vec_u16(&text);
                    let menu_ptr = menu.build();
                    unsafe { AppendMenuW(current_menu, MF_POPUP, menu_ptr as usize, text_u16.as_ptr()) };
                    // NOTE: do not call DestroyMenu here.
                    //
                    // For some reason, Windows changes the **style of the menu** to the Windows 95 style if you do this.
                    // A resource leak does not happen, since destroying a pointer to a top-level-menu
                    // (as you should do on the result of this function) via `DestroyMenu` also recursively destroys all
                    // sub-menus.
                    //
                    // see: https://stackoverflow.com/questions/12392677/creating-modern-style-dynamic-menu-in-windows
                }
            }
        }

        current_menu
    }
}
/*
fn win32_create_menu(hwnd: HWND) {

    use self::command_ids::*;

    // Init the menu bar
    macro_rules! menu_item {
        ($id:expr, $text:expr) => (MenuItem::ClickableItem { id: $id, text: $text.into() })
    }
    macro_rules! seperator {
        () => (MenuItem::Seperator)
    }

    let menu = ApplicationMenu {
        items: vec![
            MenuItem::SubMenu {
                text: "&Test".into(),
                menu: Box::new(ApplicationMenu {
                    items: vec![
                        menu_item!(CMD_TEST, "&Hello\tCtrl+Shift+O"),
                    ]
                })
            },
        ]
    };

    let menu_ptr = menu.build();
    unsafe { SetMenu(hwnd, menu_ptr) };

    // NOTE: DestroyMenu changes the style of the app, see above
    // Not sure if this actually leaks the memory of the menu... this seems to be a
    // Windows problem. Since the menu is only created once on startup, it probably
    // doesn't matter much.
    //
    // unsafe { DestroyMenu(menu_ptr) };
}

// When calling Win32 functions, especially at startup, they have to be
// called from the same thread as the Win32 message loop, otherwise Windows
// will lock up the application.
pub fn win32_create_callback(hwnd: HWND) {
    // for release builds, respect the user focus
    win32_bring_window_to_top(hwnd);
    win32_create_menu(hwnd);
}
*/