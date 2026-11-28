//! The ribbon's touch chrome on a phone (`dom_mobile`, and the touch half of `dom`).
//!
//! The tab strip that holds the application button ("File") is not shown on a phone: the full-width tab
//! button and its picker take its place, and the picker listed the tabs only, so the backstage or the menu
//! the application button opens had no way in (AzWriter on an Android phone: no way to open, save or export a
//! document). The picker lists the application button first, with the same click and the same menu. Its rows
//! and the group list's are 48 px tall: Material's touch target, which is also more than Apple's 44 pt (the
//! group list's rows were 44 px).
//!
//! Not compiled by the author (house rule).

use azul_core::dom::{Dom, IdOrClass::Class, NodeType};
use azul_css::{AzString, StringVec};

use super::*;
use crate::widgets::button::ButtonOnClickCallbackType;

const FILE_ROW: &str = "__azul-native-ribbon-mobile-file";
const PICKER_ROW: &str = "__azul-native-ribbon-mobile-tab-overlay-item";
const PICKER: &str = "__azul-native-ribbon-mobile-tab-overlay";

fn has_class(node: &Dom, name: &str) -> bool {
    node.root
        .get_ids_and_classes()
        .as_ref()
        .iter()
        .any(|c| matches!(c, Class(s) if s.as_str() == name))
}

/// The first text under `node`, depth first.
fn first_text(node: &Dom) -> Option<String> {
    if let NodeType::Text(t) = node.root.get_node_type() {
        return Some(t.as_str().to_string());
    }
    node.children.as_ref().iter().find_map(first_text)
}

extern "C" fn open_the_backstage(_data: RefAny, _info: CallbackInfo) -> Update {
    Update::DoNothing
}

fn two_tabs() -> RibbonTabVec {
    RibbonTabVec::from_vec(vec![
        RibbonTab::new(AzString::from("HOME")),
        RibbonTab::new(AzString::from("VIEW")),
    ])
}

/// The picker of `dom`: the child with the picker's class.
fn picker_of(dom: &Dom) -> &Dom {
    dom.children
        .as_ref()
        .iter()
        .find(|child| has_class(child, PICKER))
        .expect("the touch chrome has a tab picker")
}

fn with_file(click: bool) -> Ribbon {
    let mut file = RibbonAppButton::new(AzString::from("FILE"));
    if click {
        file = file.with_on_click(
            RefAny::new(7u32),
            open_the_backstage as ButtonOnClickCallbackType,
        );
    }
    Ribbon::new(two_tabs()).with_app_button(file)
}

#[test]
fn the_picker_lists_the_application_button_first_with_its_click() {
    let dom = with_file(true).dom_mobile();
    let rows = picker_of(&dom).children.as_ref();
    assert_eq!(rows.len(), 3, "File, then the two tabs");
    assert!(has_class(&rows[0], FILE_ROW));
    assert!(has_class(&rows[0], PICKER_ROW), "a row of the picker in the picker's look");
    assert_eq!(first_text(&rows[0]).as_deref(), Some("FILE"));
    assert_eq!(rows[0].root.get_callbacks().as_ref().len(), 1, "the button's own click");
    assert_eq!(first_text(&rows[1]).as_deref(), Some("HOME"));
    assert_eq!(first_text(&rows[2]).as_deref(), Some("VIEW"));
    for tab_row in &rows[1..] {
        assert!(!has_class(tab_row, FILE_ROW), "the tabs are not the button");
    }
}

#[test]
fn the_adaptive_ribbon_has_the_row_too() {
    let dom = with_file(true).dom();
    let rows = picker_of(&dom).children.as_ref();
    assert_eq!(rows.len(), 3);
    assert!(has_class(&rows[0], FILE_ROW));
}

#[test]
fn a_ribbon_without_an_application_button_lists_its_tabs_only() {
    let dom = Ribbon::new(two_tabs()).dom_mobile();
    let rows = picker_of(&dom).children.as_ref();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| !has_class(row, FILE_ROW)));
}

#[test]
fn a_button_that_only_opens_a_menu_hangs_the_menu_on_its_row() {
    let file = RibbonAppButton::new(AzString::from("FILE")).with_menu(Dom::create_div());
    let dom = Ribbon::new(two_tabs()).with_app_button(file).dom_mobile();
    let rows = picker_of(&dom).children.as_ref();
    assert!(has_class(&rows[0], FILE_ROW));
    assert_eq!(rows[0].children.as_ref().len(), 2, "the label, then the menu's window");
    assert_eq!(rows[0].root.get_callbacks().as_ref().len(), 1, "the click that opens and closes it");
}

#[test]
fn the_desktop_chrome_has_no_picker_and_keeps_the_button_in_its_strip() {
    let dom = with_file(true).dom_desktop();
    assert!(dom.children.as_ref().iter().all(|child| !has_class(child, PICKER)));
    let strip = &dom.children.as_ref()[0];
    assert!(has_class(strip, "__azul-native-ribbon-tabbar"));
    assert!(strip
        .children
        .as_ref()
        .iter()
        .any(|child| has_class(child, "__azul-native-ribbon-appbutton")));
}

#[test]
fn the_rows_of_the_picker_and_of_the_group_list_are_48_px_tall() {
    let style = RibbonStyle::office_2013();
    for (what, part) in [
        ("the picker's row", style.resolved_mobile_tab_overlay_item_style()),
        ("the group list's row", style.resolved_mobile_group_list_item_style()),
        ("the tab button", style.resolved_mobile_tab_button_style()),
    ] {
        assert!(
            part.as_ref()
                .iter()
                .any(|c| c.property == P::const_height(LayoutHeight::const_px(48))),
            "{what} is 48 px tall"
        );
    }
}

/// The first node under `node` (itself included) with the class `class`, depth first.
fn find_class<'a>(node: &'a Dom, class: &str) -> Option<&'a Dom> {
    if has_class(node, class) {
        return Some(node);
    }
    node.children.as_ref().iter().find_map(|child| find_class(child, class))
}

#[test]
fn a_drop_down_in_the_ribbon_is_sized_by_the_band_not_by_the_finger_floor_of_a_page() {
    use azul_css::props::property::CssPropertyType;

    let choices = StringVec::from_vec(vec![AzString::from("Apple"), AzString::from("Pear")]);
    let group = RibbonGroup::new(AzString::from("Font")).with_item(RibbonItem::Drop(DropDown::new(choices)));
    let tab = RibbonTab::new(AzString::from("HOME")).with_group(group);
    let dom = Ribbon::new(RibbonTabVec::from_vec(vec![tab])).dom_desktop();
    let drop_down = find_class(&dom, "__azul-native-dropdown").expect("the drop-down is in the band");
    for ty in [CssPropertyType::MinHeight, CssPropertyType::MinWidth] {
        let last = drop_down
            .root
            .style
            .iter_inline_properties()
            .filter(|(p, conds)| conds.as_ref().is_empty() && p.get_type() == ty)
            .last()
            .map(|(p, _)| p.clone());
        assert!(last.is_some(), "{ty:?}: the ribbon says its own size, last");
    }
}
