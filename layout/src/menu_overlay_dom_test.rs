//! The Dom pass of the menus of a window with one surface: what the page gets for a menu an app
//! opened - the popup, the panel, the rows - as pure functions of the DOM and the record of the
//! menus. What the layout does with it is the integration test
//! `a_menu_of_a_window_with_one_surface_opens_inside_it`.

use azul_core::{
    a11y::{AccessibilityRole, AccessibilityState},
    dom::{Dom, NodeType},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    menu::{Menu, MenuItem, MenuItemIcon, MenuItemState, StringMenuItem},
    transient::{TransientAnchor, TransientDismiss},
};

use super::{
    present_menu_overlays, MENU_CLASS, MENU_ITEM_CHOSEN_CLASS, MENU_ITEM_CLASS,
    MENU_ITEM_DISABLED_CLASS, MENU_OVERLAY_CLASS, MENU_SEPARATOR_CLASS, MENU_SUBMENU_CLASS,
};
use crate::menu_overlay::{ExpandedSubmenus, MenuOverlays, MENU_ID_PREFIX};

fn item(label: &str) -> StringMenuItem {
    StringMenuItem::create(label.into())
}

fn menu_of(items: Vec<MenuItem>) -> Menu {
    Menu::create(items.into())
}

fn control() -> LogicalRect {
    LogicalRect::new(LogicalPosition::new(20.0, 100.0), LogicalSize::new(160.0, 40.0))
}

fn window() -> LogicalSize {
    LogicalSize::new(412.0, 915.0)
}

fn page() -> Dom {
    Dom::create_body().with_child(Dom::create_div_with_text("The page"))
}

fn has_class(dom: &Dom, class: &str) -> bool {
    dom.root
        .get_ids_and_classes()
        .iter()
        .any(|c| c.as_class() == Some(class))
}

/// The rows of the panel of the menu at the end of `page`, in tree order, one level deep.
fn rows(page: &Dom) -> Vec<&Dom> {
    let popup = page.children.as_ref().last().expect("the menu's popup");
    let panel = &popup.children.as_ref()[0];
    panel.children.as_ref().iter().collect()
}

fn shown(items: Vec<MenuItem>) -> (Dom, MenuOverlays) {
    let mut menus = MenuOverlays::new();
    menus.open(&menu_of(items), control(), window()).unwrap();
    let mut page = page();
    assert_eq!(present_menu_overlays(&mut page, &menus), 1);
    (page, menus)
}

/// The popup is a `<transient-window>` at the end of the page, named by its menu, that opens
/// below the control and closes on a press outside it.
#[test]
fn a_menu_is_a_popup_at_the_end_of_the_page() {
    let (page, menus) = shown(vec![MenuItem::String(item("Copy")), MenuItem::String(item("Paste"))]);
    assert_eq!(page.children.as_ref().len(), 2, "the page's own child, then the menu");

    let popup = page.children.as_ref().last().unwrap();
    let NodeType::TransientWindow(config) = popup.root.get_node_type() else {
        panic!("the menu's node is a transient window");
    };
    assert!(config.open);
    assert_eq!(config.anchor, TransientAnchor::Bottom);
    assert_eq!(config.dismiss, TransientDismiss::Outside);
    assert!(has_class(popup, MENU_OVERLAY_CLASS));
    let id = menus.menus()[0].id;
    assert!(
        popup
            .root
            .get_ids_and_classes()
            .iter()
            .any(|c| c.as_id() == Some(format!("{MENU_ID_PREFIX}{id}").as_str())),
        "named by its menu"
    );
}

/// A list you pick from leaves the keyboard where it is: the panel is a menu popup.
#[test]
fn the_panel_is_a_menu_that_leaves_the_keyboard_alone() {
    let (page, _) = shown(vec![MenuItem::String(item("Copy"))]);
    let popup = page.children.as_ref().last().unwrap();
    let panel = &popup.children.as_ref()[0];
    assert!(has_class(panel, MENU_CLASS));
    assert!(matches!(
        panel.root.get_accessibility_info().map(|a| a.role),
        Some(AccessibilityRole::MenuPopup)
    ));
}

#[test]
fn every_item_is_a_row_and_a_separator_is_a_line() {
    let (page, _) = shown(vec![
        MenuItem::String(item("Copy")),
        MenuItem::Separator,
        MenuItem::String(item("Paste")),
        MenuItem::BreakLine,
    ]);
    let rows = rows(&page);
    assert_eq!(rows.len(), 3, "a line break is for a bar: no row");
    assert!(has_class(rows[0], MENU_ITEM_CLASS));
    assert!(has_class(rows[1], MENU_SEPARATOR_CLASS));
    assert!(has_class(rows[2], MENU_ITEM_CLASS));
    assert!(matches!(
        rows[0].root.get_accessibility_info().map(|a| a.role),
        Some(AccessibilityRole::MenuItem)
    ));
}

extern "C" fn nothing(
    _data: azul_core::refany::RefAny,
    _info: crate::callbacks::CallbackInfo,
) -> azul_core::callbacks::Update {
    azul_core::callbacks::Update::DoNothing
}

fn with_callback(label: &str) -> StringMenuItem {
    item(label).with_callback(azul_core::refany::RefAny::new(()), nothing as usize)
}

/// What an item with a callback can be pressed for; one that cannot be picked has no handler, no
/// hover wash, and says unavailable.
#[test]
fn a_disabled_item_cannot_be_pressed() {
    let mut greyed = with_callback("Paste");
    greyed.menu_item_state = MenuItemState::Greyed;
    let (page, _) = shown(vec![MenuItem::String(with_callback("Copy")), MenuItem::String(greyed)]);
    let rows = rows(&page);

    assert!(!rows[0].root.get_callbacks().as_ref().is_empty(), "an item with a callback is pressed");
    assert!(has_class(rows[1], MENU_ITEM_DISABLED_CLASS));
    assert!(rows[1].root.get_callbacks().as_ref().is_empty(), "a greyed item is not");
    assert!(
        rows[1]
            .root
            .get_accessibility_info()
            .is_some_and(|a| a.states.as_ref().contains(&AccessibilityState::Unavailable)),
        "and a screen reader says so"
    );
    // No hover wash on it.
    for (_property, conditions) in rows[1].root.style.iter_inline_properties() {
        assert!(
            !conditions.as_ref().iter().any(|c| matches!(
                c,
                azul_css::dynamic_selector::DynamicSelector::PseudoState(
                    azul_css::dynamic_selector::PseudoStateType::Hover
                )
            )),
            "a row that cannot be picked is not washed under the pointer"
        );
    }
}

/// The tick of a drop-down's chosen choice: its row is marked, and every row of the menu has the
/// column for a tick so the labels line up.
#[test]
fn the_chosen_item_is_ticked_and_the_labels_line_up() {
    let mut chosen = with_callback("Bold");
    chosen.icon = Some(MenuItemIcon::Checkbox(true)).into();
    let mut other = with_callback("Italic");
    other.icon = Some(MenuItemIcon::Checkbox(false)).into();
    let (page, _) = shown(vec![MenuItem::String(chosen), MenuItem::String(other), MenuItem::String(item("Plain"))]);
    let rows = rows(&page);

    assert!(has_class(rows[0], MENU_ITEM_CHOSEN_CLASS));
    assert!(!has_class(rows[1], MENU_ITEM_CHOSEN_CLASS));
    // icon column, label: two children each - and the tick only in the chosen row.
    for row in &rows {
        assert_eq!(row.children.as_ref().len(), 2, "a column for the tick, then the label");
    }
    assert!(!rows[0].children.as_ref()[0].children.as_ref().is_empty(), "the tick");
    assert!(rows[1].children.as_ref()[0].children.as_ref().is_empty(), "no tick, but its column");
    assert!(
        rows[0]
            .root
            .get_accessibility_info()
            .is_some_and(|a| a.states.as_ref().contains(&AccessibilityState::CheckedTrue)),
        "ticked for a screen reader too"
    );
}

/// A menu with no icon at all has no column for one.
#[test]
fn a_menu_without_icons_has_no_icon_column() {
    let (page, _) = shown(vec![MenuItem::String(item("Copy"))]);
    assert_eq!(rows(&page)[0].children.as_ref().len(), 1, "the label alone");
}

/// An item with a submenu unfolds IN PLACE: closed it is a row with a chevron, open the rows of the
/// submenu stand under it.
#[test]
fn a_submenu_unfolds_under_its_item() {
    let parent = item("Share").with_children(
        vec![MenuItem::String(with_callback("Mail")), MenuItem::String(with_callback("Chat"))].into(),
    );
    let (closed, menus) = shown(vec![MenuItem::String(parent), MenuItem::String(item("Delete"))]);
    let closed_rows = rows(&closed);
    assert_eq!(closed_rows.len(), 2);
    assert_eq!(closed_rows[0].children.as_ref().len(), 2, "label and chevron");
    assert!(
        closed_rows[0]
            .root
            .get_accessibility_info()
            .is_some_and(|a| a.states.as_ref().contains(&AccessibilityState::Collapsed)),
        "folded"
    );

    // A tap on the item flips its fold in the record the rows share ...
    let mut folds = menus.menus()[0].expanded.clone();
    folds.downcast_mut::<ExpandedSubmenus>().unwrap().toggle(&[0]);
    // ... and the next page the pass makes has the rows of the submenu under it.
    let mut page = page();
    present_menu_overlays(&mut page, &menus);
    let open_rows = rows(&page);
    assert_eq!(open_rows.len(), 2, "the item with its submenu is one entry, then Delete");
    let group = open_rows[0];
    assert_eq!(group.children.as_ref().len(), 2, "the item's row, then its submenu");
    assert!(
        group.children.as_ref()[0]
            .root
            .get_accessibility_info()
            .is_some_and(|a| a.states.as_ref().contains(&AccessibilityState::Expanded)),
        "unfolded"
    );
    let submenu = &group.children.as_ref()[1];
    assert!(has_class(submenu, MENU_SUBMENU_CLASS));
    assert_eq!(submenu.children.as_ref().len(), 2, "Mail and Chat");
}

/// The shell runs the pass where the DOM is made and the styling runs it again.
#[test]
fn the_pass_is_idempotent() {
    let (mut page, menus) = shown(vec![MenuItem::String(item("Copy"))]);
    assert_eq!(present_menu_overlays(&mut page, &menus), 0, "the menu is in the page already");
    assert_eq!(page.children.as_ref().len(), 2);
}

/// A page with no menu open is left as the app made it.
#[test]
fn a_page_without_a_menu_is_not_touched() {
    let mut page = page();
    let before = page.clone();
    assert_eq!(present_menu_overlays(&mut page, &MenuOverlays::new()), 0);
    assert!(page == before);
}

/// Two menus are two popups, the newer last.
#[test]
fn two_menus_are_two_popups_the_newer_last() {
    let mut menus = MenuOverlays::new();
    menus.open(&menu_of(vec![MenuItem::String(item("One"))]), control(), window()).unwrap();
    let other = LogicalRect::new(LogicalPosition::new(20.0, 400.0), LogicalSize::new(160.0, 40.0));
    menus.open(&menu_of(vec![MenuItem::String(item("Two"))]), other, window()).unwrap();
    let mut page = page();
    assert_eq!(present_menu_overlays(&mut page, &menus), 2);
    assert_eq!(page.children.as_ref().len(), 3);
}

/// A list of thousands is no menu: the first 2,000 items of a level are shown and the rest left
/// out, never a page the size of the list.
#[test]
fn a_menu_of_thousands_of_items_shows_the_first_two_thousand() {
    let items: Vec<MenuItem> = (0..2_500)
        .map(|i| MenuItem::String(item(&format!("Item {i}"))))
        .collect();
    let (page, _) = shown(items);
    assert_eq!(rows(&page).len(), 2_000);
}
