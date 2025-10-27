//! X11 menu handling (GNOME DBus Global Menu & X11 Popup Windows for context menus)

use std::{collections::BTreeMap, ffi::CString, rc::Rc};

use azul_core::menu::{CoreMenuCallback, Menu, MenuItem};

use super::{defines::*, dlopen::Xlib};
use crate::desktop::shell2::common::WindowError;

// For brevity, the full DBus implementation is omitted.
// This struct acts as a placeholder for a potential DBus menu manager.
pub struct MenuManager {
    // Internal DBus connection and state would go here.
}

impl MenuManager {
    pub fn new(_app_name: &str) -> Result<Self, String> {
        // In a full implementation, this would connect to DBus and register the application.
        // For now, it's a no-op that always succeeds.
        eprintln!("[X11 Menu] DBus global menu support is a stub.");
        Ok(Self {})
    }

    pub fn set_menu(&mut self, _menu: &Menu) {
        // Converts the Azul menu to a DBus-compatible format and updates the service.
    }

    pub fn set_x11_properties(&self, display: *mut Display, window: Window, xlib: &Rc<Xlib>) {
        // Sets atoms like _GTK_APPLICATION_ID on the window to integrate with the DE.
        let app_id = CString::new("my_app.desktop").unwrap();
        let atom = unsafe {
            (xlib.XInternAtom)(display, b"_GTK_APPLICATION_ID\0".as_ptr() as *const i8, 0)
        };
        if atom != 0 {
            unsafe {
                (xlib.XChangeProperty)(
                    display,
                    window,
                    atom,
                    8,
                    0,
                    0,
                    app_id.as_ptr() as *const u8,
                    app_id.as_bytes().len() as i32,
                );
            }
        }
    }
}

/// Creates and shows a context menu using a new, borderless X11 window.
pub fn show_context_menu(
    parent: Window,
    display: *mut Display,
    xlib: &Rc<Xlib>,
    menu: &Menu,
    x: i32,
    y: i32,
) -> Result<(Window, BTreeMap<u16, CoreMenuCallback>), WindowError> {
    let screen = unsafe { (xlib.XDefaultScreen)(display) };
    let root = unsafe { (xlib.XRootWindow)(display, screen) };

    let mut attributes: XSetWindowAttributes = unsafe { std::mem::zeroed() };
    attributes.override_redirect = 1; // Makes the window borderless and unmanaged by WM
    attributes.event_mask = ExposureMask | KeyPressMask | ButtonPressMask | StructureNotifyMask;

    // TODO: Calculate actual menu size based on items and font metrics
    let width = 200;
    let height = (menu.items.len() * 25) as u32;

    let window = unsafe {
        (xlib.XCreateSimpleWindow)(
            display, root, x, y, width, height, 1,          // border width
            0,          // border color
            0xFFFFFFFF, // background color (white)
        )
    };

    unsafe {
        (xlib.XChangeWindowAttributes)(display, window, CWOverrideRedirect, &mut attributes);
        (xlib.XMapWindow)(display, window);
        (xlib.XFlush)(display);
    }

    // In a real implementation, this window would have its own event loop or
    // have its events forwarded. We would draw the menu items into it.
    // The BTreeMap would map an item's index to its callback.
    let callbacks = BTreeMap::new();

    Ok((window, callbacks))
}
