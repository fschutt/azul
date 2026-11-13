//! A menu bar built again with fresh callback data is the same menu to the
//! platform: only its callbacks moved.
//!
//! The user's report: dragging a slider in AzWidgets makes the macOS menu bar
//! flicker. Every value change rebuilds the DOM, the layout callback builds
//! its menu bar again with a `RefAny::new` per item, and the identity of that
//! data is part of `Menu::get_hash`. So the shell saw a new menu on every
//! frame of the drag and installed it again. What the platform menu shows -
//! labels, shortcuts, states, the tree - did not change, and that is what
//! `get_structure_hash` answers; `leaf_callbacks` hands the new callbacks over
//! in the order the native menus number their items.

use azul_core::{
    menu::{Menu, MenuItem, MenuItemState, StringMenuItem},
    refany::RefAny,
};

/// A callback the items point at; never called here.
const ON_ITEM: usize = 0x1000;

/// File (New, Open, -, Quit) and Edit (Undo): each leaf carries its own
/// fresh `RefAny`, as an app builds it in every layout pass.
fn menu_bar(app_state: u32) -> Menu {
    menu_bar_with(app_state, "File", MenuItemState::Normal)
}

/// [`menu_bar`] with the first menu named `file` and the second in `edit`.
fn menu_bar_with(app_state: u32, file: &str, edit: MenuItemState) -> Menu {
    let item = |label: &str| {
        MenuItem::String(
            StringMenuItem::create(label.into())
                .with_callback(RefAny::new((app_state, label.to_string())), ON_ITEM),
        )
    };
    let mut edit_menu =
        StringMenuItem::create("Edit".into()).with_children(vec![item("Undo")].into());
    edit_menu.menu_item_state = edit;
    Menu::create(
        vec![
            MenuItem::String(StringMenuItem::create(file.into()).with_children(
                vec![item("New"), item("Open"), MenuItem::Separator, item("Quit")].into(),
            )),
            MenuItem::String(edit_menu),
        ]
        .into(),
    )
}

#[test]
fn a_menu_rebuilt_with_fresh_callback_data_keeps_its_structure_hash() {
    let first = menu_bar(1);
    let again = menu_bar(2);
    assert_ne!(
        first.get_hash(),
        again.get_hash(),
        "premise: the full hash sees the new callback data"
    );
    assert_eq!(first.get_structure_hash(), again.get_structure_hash());
}

#[test]
fn a_changed_label_or_state_changes_the_structure_hash() {
    let first = menu_bar(1);
    let renamed = menu_bar_with(1, "Datei", MenuItemState::Normal);
    assert_ne!(first.get_structure_hash(), renamed.get_structure_hash());
    let greyed = menu_bar_with(1, "File", MenuItemState::Greyed);
    assert_ne!(first.get_structure_hash(), greyed.get_structure_hash());
}

#[test]
fn the_leaf_callbacks_come_in_the_order_of_the_items() {
    let menu = menu_bar(7);
    let leaves = menu.leaf_callbacks();
    // New, Open, Quit, Undo: the separator is no leaf, the submenus are not
    // leaves either.
    assert_eq!(leaves.len(), 4);
    let labels: Vec<String> = leaves
        .into_iter()
        .map(|cb| {
            let mut data = cb.expect("every leaf here has a callback").refany.clone();
            let pair = data
                .downcast_ref::<(u32, String)>()
                .expect("the item's own data");
            pair.1.clone()
        })
        .collect();
    assert_eq!(labels, ["New", "Open", "Quit", "Undo"]);
}
