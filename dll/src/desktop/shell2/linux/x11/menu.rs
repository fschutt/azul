//! X11 menu handling (GNOME DBus Global Menu & X11 Popup Windows for context menus)

use super::dlopen::Xlib;
use super::defines::*;
use crate::desktop::shell2::common::WindowError;
use azul_core::menu::{Menu, MenuItem};
use std::rc::Rc;
use std::collections::BTreeMap;
use azul_core::callbacks::CoreMenuCallback;

// For brevity, the full DBus implementation from `old/x11/menu.rs` is omitted.
// It is complex and would be its own large file. Assume a `MenuManager` exists:

pub struct MenuManager {
    // Internal DBus connection and state
}

impl MenuManager {
    pub fn new(app_name: &str) -> Result<Self, String> {
        // ... Connects to DBus, registers the application ...
        Ok(Self {})
    }

    pub fn set_menu(&mut self, menu: &Menu) {
        // ... Converts the Azul menu to a DBus-compatible format and updates the service ...
    }
    
    pub fn set_x11_properties(&self, display: *mut Display, window: Window, xlib: &Rc<Xlib>) {
        // ... Sets the _GTK_APPLICATION_ID and other atoms on the window ...
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
    
    // TODO: Calculate actual menu size
    let width = 200;
    let height = (menu.items.len() * 25) as u32;

    let window = unsafe {
        (xlib.XCreateWindow)(
            display,
            root,
            x,
            y,
            width,
            height,
            0,
            CopyFromParent,
            InputOutput as u32,
            std::ptr::null_mut(),
            CWOverrideRedirect | CWEventMask,
            &mut attributes,
        )
    };

    unsafe {
        (xlib.XMapWindow)(display, window);
        (xlib.XFlush)(display);
    }

    // In a real implementation, this window would have its own event loop or
    // have its events forwarded. We would draw the menu items into it.
    // The BTreeMap would map an item's index to its callback.
    let callbacks = BTreeMap::new(); 

    Ok((window, callbacks))
}