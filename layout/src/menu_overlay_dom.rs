//! The Dom pass of the menus of a window with ONE surface: puts the `<transient-window>` of every
//! open menu ([`crate::menu_overlay`]) at the end of the page's DOM, with the rows of the menu in
//! it.
//!
//! The node is an ordinary popup of the page - anchored below the control the menu dropped from
//! (the manager places it against the rect the menu was opened for, not against the node's
//! parent), dismissed by a press outside it - and [`crate::transient_overlay`] gives it the layer
//! style like every other, which is why this pass runs first.
//!
//! The rows are made for a finger: 48 px high, the label to the left, a tick for the chosen item, a
//! chevron for an item with a submenu - which unfolds IN PLACE under the item, as a phone's menus
//! do, instead of opening another menu beside it. The look is the theme's: the panel is the leaf of
//! the theme's combo-box list and a row its option row (hover wash, ink, focus ring), so a menu is
//! flat in a flat app and flora in a flora app.
//!
//! Pressing an item runs the item's callback in the pass of the press - the window's own state, its
//! timers, its DOM - and closes the menu; pressing an item with a submenu unfolds or folds it.
//!
//! The pass is idempotent: the shell runs it where the DOM is made and the styling runs it again,
//! and a menu that is in the DOM already is left as it is.

use alloc::{string::String, vec::Vec};

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole, AccessibilityState, AccessibilityStateVec},
    callbacks::Update,
    dom::{Dom, DomVec, EventFilter, HoverEventFilter, IdOrClass, NodeData, NodeType},
    menu::{CoreMenuCallback, MenuItem, MenuItemIcon, MenuItemState, StringMenuItem},
    refany::RefAny,
    transient::{TransientAnchor, TransientDismiss, TransientWindowConfig},
};
use azul_css::{
    dynamic_selector::{CssPropertyWithConditions, CssPropertyWithConditionsVec},
    AzString,
};

use crate::{
    callbacks::{Callback, CallbackInfo},
    menu_overlay::{ExpandedSubmenus, MenuOverlay, MenuOverlays, MENU_ID_PREFIX},
    widgets::{
        combobox::{skin_of, ComboBoxSkin},
        themes::OptionUiTheme,
    },
};

/// The class of the `<transient-window>` that shows a menu.
pub const MENU_OVERLAY_CLASS: &str = "__azul-menu-overlay";
/// The class of the panel that holds the rows of a menu.
pub const MENU_CLASS: &str = "__azul-menu";
/// The class of a row.
pub const MENU_ITEM_CLASS: &str = "__azul-menu-item";
/// The class of the row of the item the app marked chosen (a ticked check box).
pub const MENU_ITEM_CHOSEN_CLASS: &str = "__azul-menu-item-chosen";
/// The class of the row of an item that cannot be picked.
pub const MENU_ITEM_DISABLED_CLASS: &str = "__azul-menu-item-disabled";
/// The class of the rows of a submenu, in their own box under the item that holds it.
pub const MENU_SUBMENU_CLASS: &str = "__azul-menu-submenu";
/// The class of the line between two groups of items.
pub const MENU_SEPARATOR_CLASS: &str = "__azul-menu-separator";

/// How deep the submenus of a menu are shown; an item below that depth is left out (and a
/// diagnostic says so). A menu nests a few levels, a tree of thousands is not one.
const MAX_SUBMENU_DEPTH: usize = 8;

/// The panel: the leaf of the theme, never wider than the window, rounded as a phone's menus are.
const PANEL_STYLE: &str = "box-sizing: border-box; min-width: 160px; max-width: 100%; \
     padding-top: 4px; padding-bottom: 4px; padding-left: 0px; padding-right: 0px; \
     border-radius: 8px; overflow-x: hidden;";

/// A row: finger high, the contents in a line.
const ROW_STYLE: &str = "box-sizing: border-box; display: flex; flex-direction: row; \
     align-items: center; min-height: 48px; padding-top: 0px; padding-bottom: 0px; \
     padding-left: 16px; padding-right: 16px; font-size: 16px; cursor: pointer; \
     user-select: none;";

/// The row of the chosen item.
const CHOSEN_STYLE: &str = "font-weight: 600;";

/// The label of a row: the rest of the line.
const LABEL_STYLE: &str = "flex-grow: 1; flex-shrink: 1; min-width: 0px;";

/// The tick column of a menu in which an item has an icon (a drop-down: its chosen choice), so the
/// labels line up.
const ICON_STYLE: &str = "box-sizing: border-box; display: flex; align-items: center; \
     justify-content: center; width: 24px; height: 24px; margin-right: 12px; flex-shrink: 0; \
     font-size: 20px;";

/// The chevron of an item with a submenu.
const CHEVRON_STYLE: &str = "width: 24px; height: 24px; margin-left: 12px; flex-shrink: 0; \
     font-size: 20px;";

/// The line between two groups of items.
const SEPARATOR_STYLE: &str = "height: 1px; margin-top: 4px; margin-bottom: 4px; \
     background: system:separator;";

/// The rows of a submenu, shifted in under their item.
const SUBMENU_STYLE: &str = "padding-left: 16px;";

/// What a row that picks an item carries: the item's callback.
struct PickData {
    callback: CoreMenuCallback,
}

/// What a row that unfolds a submenu carries: the folds of its menu and the item's path in it.
struct ToggleData {
    expanded: RefAny,
    path: Vec<usize>,
}

/// An item was pressed: the menu closes, and the item's callback runs in this pass as the window's
/// own - the state it changes, the timers it starts and the rebuild it asks for are the window's.
extern "C" fn item_picked(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let picked = match data.downcast_ref::<PickData>() {
        Some(pick) => pick.callback.clone(),
        None => return Update::DoNothing,
    };
    // First: the item may open a menu of its own, which must find this one gone.
    crate::transient_overlay::close_window_around(&mut info);
    Callback::from_core(picked.callback).invoke(picked.refany, info)
}

/// An item with a submenu was pressed: the submenu unfolds, or folds again.
extern "C" fn submenu_toggled(mut data: RefAny, _info: CallbackInfo) -> Update {
    let (mut expanded, path) = match data.downcast_ref::<ToggleData>() {
        Some(toggle) => (toggle.expanded.clone(), toggle.path.clone()),
        None => return Update::DoNothing,
    };
    if let Some(mut folds) = expanded.downcast_mut::<ExpandedSubmenus>() {
        folds.toggle(&path);
    }
    Update::RefreshDom
}

fn styled(base: &[CssPropertyWithConditions], extra: &str) -> CssPropertyWithConditionsVec {
    let mut style = base.to_vec();
    style.extend(CssPropertyWithConditionsVec::parse(extra).into_library_owned_vec());
    CssPropertyWithConditionsVec::from_vec(style)
}

const fn class(name: &'static str) -> IdOrClass {
    IdOrClass::Class(AzString::from_const_str(name))
}

/// Whether a row of `items` has an icon: then every row of the level has a column for it.
fn level_has_icons(items: &[MenuItem]) -> bool {
    items
        .iter()
        .any(|item| matches!(item, MenuItem::String(s) if s.icon.as_option().is_some()))
}

/// What a screen reader says for a row: the label, unavailable when the item cannot be picked,
/// ticked when it is chosen, unfolded or folded when it has a submenu.
fn row_accessibility(item: &StringMenuItem, expanded: Option<bool>, chosen: bool) -> AccessibilityInfo {
    let mut states: Vec<AccessibilityState> = Vec::new();
    if item.menu_item_state != MenuItemState::Normal {
        states.push(AccessibilityState::Unavailable);
    }
    if matches!(item.icon.as_option(), Some(MenuItemIcon::Checkbox(_))) {
        states.push(if chosen {
            AccessibilityState::CheckedTrue
        } else {
            AccessibilityState::CheckedFalse
        });
    }
    match expanded {
        Some(true) => states.push(AccessibilityState::Expanded),
        Some(false) => states.push(AccessibilityState::Collapsed),
        None => {}
    }
    AccessibilityInfo {
        role: AccessibilityRole::MenuItem,
        accessibility_name: Some(item.label.clone()).into(),
        states: AccessibilityStateVec::from_vec(states),
        ..Default::default()
    }
}

/// The tick column of a row.
fn icon_box(item: &StringMenuItem) -> Dom {
    let mut column = Dom::create_div().with_css(ICON_STYLE);
    match item.icon.as_option() {
        Some(MenuItemIcon::Checkbox(true)) => {
            column = column.with_child(Dom::create_icon("system:checkmark,check"));
        }
        Some(MenuItemIcon::Image(image)) => {
            column = column.with_child(Dom::create_image(image.clone()).with_css("width: 20px; height: 20px;"));
        }
        _ => {}
    }
    column
}

/// One item that has a label.
fn string_row(
    item: &StringMenuItem,
    path: &[usize],
    menu: &MenuOverlay,
    skin: &ComboBoxSkin,
    icons: bool,
    expanded: Option<bool>,
    depth: usize,
) -> Dom {
    let enabled = item.menu_item_state == MenuItemState::Normal;
    let chosen = matches!(item.icon.as_option(), Some(MenuItemIcon::Checkbox(true)));
    let has_children = !item.children.as_ref().is_empty();

    let mut classes = alloc::vec![class(MENU_ITEM_CLASS)];
    if chosen {
        classes.push(class(MENU_ITEM_CHOSEN_CLASS));
    }
    if !enabled {
        classes.push(class(MENU_ITEM_DISABLED_CLASS));
    }
    // A row that cannot be picked has no hover wash and is dimmed.
    let base: Vec<CssPropertyWithConditions> = if enabled {
        skin.option.as_ref().to_vec()
    } else {
        crate::widgets::button::disabled_style(skin.option.as_ref())
    };
    let mut style = styled(&base, ROW_STYLE).into_library_owned_vec();
    if chosen {
        style.extend(CssPropertyWithConditionsVec::parse(CHOSEN_STYLE).into_library_owned_vec());
    }
    let mut row = Dom::create_div()
        .with_ids_and_classes(classes.into())
        .with_css_props(CssPropertyWithConditionsVec::from_vec(style))
        .with_accessibility_info(row_accessibility(item, expanded, chosen));

    if icons {
        row = row.with_child(icon_box(item));
    }
    row = row.with_child(
        crate::widgets::widget_p_with_text(item.label.clone()).with_css(LABEL_STYLE),
    );
    if has_children {
        let chevron = if expanded == Some(true) {
            "system:arrow-down,expand_more"
        } else {
            "system:arrow-right,chevron_right"
        };
        row = row.with_child(Dom::create_div().with_css(CHEVRON_STYLE).with_child(Dom::create_icon(chevron)));
    }

    if enabled && has_children && depth < MAX_SUBMENU_DEPTH {
        row = row.with_callback(
            EventFilter::Hover(HoverEventFilter::Click),
            RefAny::new(ToggleData {
                expanded: menu.expanded.clone(),
                path: path.to_vec(),
            }),
            Callback::from_ptr(submenu_toggled).to_core(),
        );
    } else if enabled {
        if let Some(callback) = item.callback.as_option() {
            row = row.with_callback(
                EventFilter::Hover(HoverEventFilter::Click),
                RefAny::new(PickData {
                    callback: callback.clone(),
                }),
                Callback::from_ptr(item_picked).to_core(),
            );
        }
    }
    row
}

/// The rows of one level of a menu, the unfolded submenus in their place.
fn level_rows(
    items: &[MenuItem],
    path: &mut Vec<usize>,
    menu: &MenuOverlay,
    skin: &ComboBoxSkin,
    too_deep: &mut bool,
) -> Vec<Dom> {
    let icons = level_has_icons(items);
    let mut rows = Vec::new();
    for (index, item) in items.iter().enumerate() {
        match item {
            MenuItem::Separator => rows.push(
                Dom::create_div()
                    .with_ids_and_classes(alloc::vec![class(MENU_SEPARATOR_CLASS)].into())
                    .with_css(SEPARATOR_STYLE),
            ),
            // A line break is for the horizontal menu of a bar.
            MenuItem::BreakLine => {}
            MenuItem::String(string) => {
                path.push(index);
                let has_children = !string.children.as_ref().is_empty();
                let depth = path.len() - 1;
                let unfolded = has_children
                    && menu
                        .expanded
                        .clone()
                        .downcast_ref::<ExpandedSubmenus>()
                        .is_some_and(|folds| folds.is_open(path));
                let state = has_children.then_some(unfolded);
                let row = string_row(string, path, menu, skin, icons, state, depth);
                if unfolded && depth + 1 < MAX_SUBMENU_DEPTH {
                    let inner = level_rows(string.children.as_ref(), path, menu, skin, too_deep);
                    rows.push(
                        Dom::create_div()
                            .with_child(row)
                            .with_child(
                                Dom::create_div()
                                    .with_ids_and_classes(alloc::vec![class(MENU_SUBMENU_CLASS)].into())
                                    .with_css(SUBMENU_STYLE)
                                    .with_children(DomVec::from_vec(inner)),
                            ),
                    );
                } else {
                    if unfolded {
                        *too_deep = true;
                    }
                    rows.push(row);
                }
                path.pop();
            }
        }
    }
    rows
}

/// The `<transient-window>` that shows one menu.
fn menu_node(menu: &MenuOverlay, id: &str, skin: &ComboBoxSkin, too_deep: &mut bool) -> Dom {
    let mut path = Vec::new();
    let rows = level_rows(menu.menu.items.as_ref(), &mut path, menu, skin, too_deep);
    let panel = Dom::create_div()
        .with_ids_and_classes(alloc::vec![class(MENU_CLASS)].into())
        .with_css_props(styled(skin.list.as_ref(), PANEL_STYLE))
        // A list you pick from: it leaves the keyboard where it is.
        .with_accessibility_info(AccessibilityInfo {
            role: AccessibilityRole::MenuPopup,
            ..Default::default()
        })
        .with_children(DomVec::from_vec(rows));
    let config = TransientWindowConfig::opened()
        .with_anchor(TransientAnchor::Bottom)
        .with_dismiss(TransientDismiss::Outside);
    Dom::create_from_data(NodeData::create_node(NodeType::TransientWindow(config)))
        .with_ids_and_classes(
            alloc::vec![
                class(MENU_OVERLAY_CLASS),
                IdOrClass::Id(AzString::from(String::from(id))),
            ]
            .into(),
        )
        .with_child(panel)
}

/// Is a node of `dom`'s root children the menu `id`'s?
fn has_menu_node(dom: &Dom, id: &str) -> bool {
    dom.children.as_ref().iter().any(|child| {
        child
            .root
            .get_ids_and_classes()
            .iter()
            .any(|c| c.as_id() == Some(id))
    })
}

/// Adds the `<transient-window>` of every open menu to the end of `dom`, if it is not there yet.
/// Returns how many it added. A window that opens real windows has no menus here: the caller does
/// not run the pass for it.
pub fn present_menu_overlays(dom: &mut Dom, menus: &MenuOverlays) -> usize {
    if menus.is_empty() {
        return 0;
    }
    dom.expand_flat();
    let skin = skin_of(OptionUiTheme::None);
    let mut too_deep = false;
    let mut added = 0;
    for menu in menus.menus() {
        let id = alloc::format!("{MENU_ID_PREFIX}{}", menu.id);
        if has_menu_node(dom, &id) {
            continue;
        }
        dom.add_child(menu_node(menu, &id, &skin, &mut too_deep));
        added += 1;
    }
    if too_deep {
        azul_core::diagnostics::emit(alloc::format!(
            "[azul][warn] a menu nests its submenus deeper than {MAX_SUBMENU_DEPTH} levels: the \
             items below that are not shown"
        ));
    }
    added
}

#[cfg(test)]
#[path = "menu_overlay_dom_test.rs"]
mod menu_overlay_dom_test;
