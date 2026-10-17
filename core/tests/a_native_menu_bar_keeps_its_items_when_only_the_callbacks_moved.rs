//! A native menu bar keeps its items when the app's menu was only built again
//! with fresh callback data: the new callbacks are bound to the items the bar
//! already has.
//!
//! The Win32 bar gives a command id to every leaf that has a callback, in the
//! order the leaves come; the GNOME bar names an action per leaf. Both
//! installed a whole new bar whenever `Menu::get_hash` moved, which it does on
//! every layout pass of an app that builds its items with a fresh `RefAny`,
//! so the bar was rebuilt on every frame of a slider drag.
//! `Menu::rebind_leaf_callbacks` pairs the native ids with the new callbacks,
//! or says the bar must be built again.

use azul_core::{
    menu::{Menu, MenuItem, StringMenuItem},
    refany::RefAny,
};

/// A callback the items point at; never called here.
const ON_ITEM: usize = 0x1000;

/// File (New, Open, -, Quit), every leaf with its own fresh data, and an
/// About leaf that has a callback only when `about` is set.
fn menu_bar(app_state: u32, about: bool) -> Menu {
    let item = |label: &str| {
        MenuItem::String(
            StringMenuItem::create(label.into())
                .with_callback(RefAny::new((app_state, label.to_string())), ON_ITEM),
        )
    };
    let about_item = if about {
        item("About")
    } else {
        MenuItem::String(StringMenuItem::create("About".into()))
    };
    Menu::create(
        vec![
            MenuItem::String(StringMenuItem::create("File".into()).with_children(
                vec![item("New"), item("Open"), MenuItem::Separator, item("Quit")].into(),
            )),
            about_item,
        ]
        .into(),
    )
}

/// The label a bound callback's data carries.
fn label_of(cb: &azul_core::menu::CoreMenuCallback) -> String {
    let mut data = cb.refany.clone();
    data.downcast_ref::<(u32, String)>()
        .map(|pair| format!("{}:{}", pair.1, pair.0))
        .expect("the item's own data")
}

#[test]
fn a_native_menu_bar_keeps_its_items_when_only_the_callbacks_moved() {
    // The bar as the shell built it: ids 11..13 for New, Open, Quit; About
    // had no callback, so no command.
    let ids = [Some(11_u16), Some(12), Some(13), None];
    let rebuilt = menu_bar(2, false);
    let bound = rebuilt
        .rebind_leaf_callbacks(&ids)
        .expect("the same items with new data: rebind, do not rebuild");
    let bound: Vec<(u16, String)> = bound.iter().map(|(id, cb)| (*id, label_of(cb))).collect();
    assert_eq!(
        bound,
        [
            (11, "New:2".to_string()),
            (12, "Open:2".to_string()),
            (13, "Quit:2".to_string())
        ],
        "each id takes the new callback of its own item"
    );
}

#[test]
fn a_leaf_that_gains_a_callback_needs_a_new_native_bar() {
    let ids = [Some(11_u16), Some(12), Some(13), None];
    assert!(
        menu_bar(2, true).rebind_leaf_callbacks(&ids).is_none(),
        "About has no command id to bind its new callback to"
    );
    assert!(
        menu_bar(2, false).rebind_leaf_callbacks(&ids[..3]).is_none(),
        "a bar with another number of leaves is another bar"
    );
}
