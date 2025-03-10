use std::{
    collections::BTreeMap,
    ffi::{CStr, CString, c_void},
    sync::{
        Arc, Mutex,
        atomic::{AtomicI32, AtomicIsize, Ordering},
    },
};

use azul_core::{
    callbacks::CallbackInfo,
    styled_dom::NodeHierarchyItemId,
    window::{Menu, MenuCallback, MenuItem, ProcessEventResult},
    window_state::NodesToCheck,
};
use objc2::{
    MainThreadMarker,
    declare::ClassDecl,
    msg_send,
    rc::{Id, Retained},
    runtime::{AnyClass, AnyObject, NO, Object, Sel},
    sel,
};
use objc2_app_kit::{
    NSEventModifierFlags, NSMenu, NSMenuItem, NSUserInterfaceItemIdentification, NSWindow,
};
use objc2_foundation::NSString;
use once_cell::sync::Lazy;

use super::{AppData, MacApp, WindowId};
use crate::shell::{CommandId, CommandMap, MacOsMenuCommands, MenuTarget};

// If the app_data.active_menus[target] differs from the `menu`, creates a new
// NSMenu and returns it. Should only be called on the main thread.
pub fn reinit_nsmenu(
    mtm: &MainThreadMarker,
    target: MenuTarget,
    menu: &Menu,
    menu_handler_class: *mut Object,
) -> Option<(Retained<NSMenu>, MacOsMenuCommands)> {
    let menu_hash = menu.get_hash();
    let mut m = NSMenu::new(*mtm);
    let mut map = CommandMap::new();
    recursive_construct_menu(
        mtm,
        &menu.items.as_slice(),
        &mut m,
        &mut map,
        menu_handler_class,
    );
    Some((
        m,
        MacOsMenuCommands {
            menu_hash,
            commands: map,
        },
    ))
}

/// Recursively build an `NSMenu` from our `MenuItem` list
fn recursive_construct_menu(
    mtm: &MainThreadMarker,
    items: &[MenuItem],
    menu: &NSMenu,
    command_map: &mut CommandMap,
    menu_handler_class: *mut Object,
) {
    unsafe {
        for item in items {
            match item {
                MenuItem::String(mi) => {
                    if mi.children.is_empty() {
                        // Leaf menu item
                        let mut menu_item = NSMenuItem::new(*mtm);
                        menu_item.setTitle(&NSString::from_str(&mi.label));
                        menu_item.setAction(Some(sel!(menuItemClicked:)));
                        menu_item.setTarget(Some(&*menu_handler_class));

                        // If there's a callback, assign a fresh "tag" integer
                        if let Some(cb) = &mi.callback.as_ref() {
                            let new_tag = CommandId::new();
                            command_map.insert(new_tag, (*cb).clone());
                            menu_item.setTag(new_tag.0);
                        }

                        if let Some(vk) = mi.accelerator.as_ref() {
                            use azul_core::window::VirtualKeyCode;

                            let keys = vk.keys.as_slice();
                            if !keys.is_empty() {
                                let mut flags = NSEventModifierFlags::empty();

                                if keys.contains(&VirtualKeyCode::LShift)
                                    || keys.contains(&VirtualKeyCode::RShift)
                                {
                                    flags.insert(NSEventModifierFlags::Shift);
                                }
                                if keys.contains(&VirtualKeyCode::LControl)
                                    || keys.contains(&VirtualKeyCode::RControl)
                                {
                                    flags.insert(NSEventModifierFlags::Control);
                                }
                                if keys.contains(&VirtualKeyCode::LAlt)
                                    || keys.contains(&VirtualKeyCode::RAlt)
                                {
                                    flags.insert(NSEventModifierFlags::Option);
                                }
                                if keys.contains(&VirtualKeyCode::LWin)
                                    || keys.contains(&VirtualKeyCode::RWin)
                                {
                                    flags.insert(NSEventModifierFlags::Command);
                                }

                                // TODO: function keys!
                                let keys = keys
                                    .iter()
                                    .filter_map(|s| s.get_lowercase())
                                    .collect::<String>();

                                menu_item.setKeyEquivalentModifierMask(flags);
                                menu_item.setKeyEquivalent(&NSString::from_str(&keys));
                            }
                        }

                        menu.addItem(&menu_item);
                    } else {
                        let mut submenu_item = NSMenuItem::new(*mtm);
                        submenu_item.setTitle(&NSString::from_str(&mi.label));
                        submenu_item.setAction(Some(sel!(menuItemClicked:)));

                        // Create the submenu itself
                        let mut submenu = NSMenu::new(*mtm);
                        submenu.setTitle(&NSString::from_str(&mi.label));
                        recursive_construct_menu(
                            mtm,
                            mi.children.as_slice(),
                            &submenu,
                            command_map,
                            menu_handler_class,
                        );
                        menu.setSubmenu_forItem(Some(&*submenu), &submenu_item);
                        menu.addItem(&submenu_item);
                    }
                }
                MenuItem::Separator | MenuItem::BreakLine => {
                    let separator = NSMenuItem::separatorItem(*mtm);
                    menu.addItem(&separator);
                }
            }
        }
    }
}

// Returns the class definition for the Menu click handler
pub fn menu_handler_class() -> ClassDecl {
    let superclass = objc2::class!(NSObject);

    let c = CString::new("RustMenuHandler").unwrap();
    let mut decl =
        ClassDecl::new(&c, superclass).expect("MenuHandler class name is already registered?");

    unsafe {
        let c = CString::new("app").unwrap();
        decl.add_ivar::<*const c_void>(&c);
        decl.add_method(
            sel!(menuItemClicked:),
            menu_item_clicked as extern "C" fn(*mut Object, Sel, *mut Object),
        );
    }

    decl
}

/// Creates an instance of the class, with a pointer to the `MacApp` stored in the `app` ivar.
fn create_menu_handler_instance(cls: &AnyClass, megaclass: &MacApp) -> *mut Object {
    unsafe {
        let instance: *mut Object = msg_send![cls, new];
        *((*instance).get_mut_ivar("app")) = megaclass as *const _ as *const c_void;
        instance
    }
}

/// The actual callback method for your NSMenuItem action
extern "C" fn menu_item_clicked(this: *mut Object, _sel: Sel, sender: *mut Object) {
    unsafe {
        // `sender` is an NSMenuItem
        let tag: isize = msg_send![sender, tag];

        let ptr = (*this).get_ivar::<*const c_void>("app");
        let ptr = *ptr as *const MacApp;
        let ptr = &*ptr;
        let windowid = *(*this).get_ivar::<i64>("windowid");

        let mut app_borrow = ptr.data.lock().unwrap();
        let mut app_borrow = &mut *app_borrow;

        let cb = app_borrow
            .active_menus
            .values()
            .find_map(|s| s.get(&CommandId(tag)));

        let callback = match cb {
            Some(s) => s,
            None => return,
        };

        let mut ret = ProcessEventResult::DoNothing;
        // let mut new_windows = Vec::new();
        // let mut destroyed_windows = Vec::new();
        let mut ab = &mut app_borrow.userdata;

        let windows = &mut ab.windows;
        let data = &mut ab.data;
        let image_cache = &mut ab.image_cache;
        let fc_cache = &mut ab.fc_cache;
        let config = &ab.config;

        let mut current_window = match app_borrow.windows.get_mut(&WindowId {
            id: windowid as i64,
        }) {
            Some(s) => s,
            None => return,
        };

        let ntc = NodesToCheck::empty(
            current_window
                .internal
                .current_window_state
                .mouse_state
                .mouse_down(),
            current_window.internal.current_window_state.focused_node,
        );

        /*

            let call_callback_result = {

                let mb = &mut current_window.menu_bar;
                let internal = &mut current_window.internal;
                let context_menu = current_window.context_menu.as_mut();
                let gl_context_ptr = &current_window.gl_context_ptr;

                if let Some(menu_callback) = mb.as_mut().and_then(|m| m.callbacks.get_mut(&loword)) {
                    Some(fc_cache.apply_closure(|fc_cache| {
                        internal.invoke_menu_callback(
                            menu_callback,
                            DomNodeId {
                                dom: DomId::ROOT_ID,
                                node: NodeHierarchyItemId::from_crate_internal(None),
                            },
                            &window_handle,
                            &gl_context_ptr,
                            image_cache,
                            fc_cache,
                            &config.system_callbacks,
                        )
                    }))
                } else if let Some(context_menu) = context_menu {
                    let hit_dom_node = context_menu.hit_dom_node;
                    if let Some(menu_callback) = context_menu.callbacks.get_mut(&loword) {
                        Some(fc_cache.apply_closure(|fc_cache| {
                            internal.invoke_menu_callback(
                                menu_callback,
                                hit_dom_node,
                                &window_handle,
                                &gl_context_ptr,
                                image_cache,
                                fc_cache,
                                &config.system_callbacks,
                            )
                        }))
                    } else {
                        None
                    }
                } else {
                    None
                }
            };

            if let Some(ccr) = call_callback_result {
                ret = process_callback_results(
                    ccr,
                    current_window,
                    &ntc,
                    image_cache,
                    fc_cache,
                    &mut new_windows,
                    &mut destroyed_windows,
                );
            };

            // same as invoke_timers(), invoke_threads(), ...

            mem::drop(ab);
            mem::drop(app_borrow);
            create_windows(hinstance, shared_application_data, new_windows);

            let mut app_borrow = shared_application_data.inner.try_borrow_mut().unwrap();
            let mut ab = &mut *app_borrow;
            destroy_windows(ab, destroyed_windows);

            match ret {
                ProcessEventResult::DoNothing => { },
                ProcessEventResult::ShouldRegenerateDomCurrentWindow => {
                    PostMessageW(hwnd, AZ_REGENERATE_DOM, 0, 0);
                },
                ProcessEventResult::ShouldRegenerateDomAllWindows => {
                    for window in app_borrow.windows.values() {
                        PostMessageW(window.hwnd, AZ_REGENERATE_DOM, 0, 0);
                    }
                },
                ProcessEventResult::ShouldUpdateDisplayListCurrentWindow => {
                    PostMessageW(hwnd, AZ_REGENERATE_DISPLAY_LIST, 0, 0);
                },
                ProcessEventResult::UpdateHitTesterAndProcessAgain => {
                    if let Some(w) = app_borrow.windows.get_mut(&hwnd_key) {
                        w.internal.previous_window_state = Some(w.internal.current_window_state.clone());
                        // TODO: submit display list, wait for new hit-tester and update hit-test results
                        PostMessageW(hwnd, AZ_REGENERATE_DISPLAY_LIST, 0, 0);
                        PostMessageW(hwnd, AZ_REDO_HIT_TEST, 0, 0);
                    }
                },
                ProcessEventResult::ShouldReRenderCurrentWindow => {
                    PostMessageW(hwnd, AZ_GPU_SCROLL_RENDER, 0, 0);
                },
            }

            mem::drop(app_borrow);
            return 0;

        */
    }
}
