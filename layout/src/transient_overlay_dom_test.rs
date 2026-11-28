//! The Dom pass of a window with one surface: what the OS would give a window of its own.
//!
//! Pure functions of the DOM: no window, no layout. What the layout does with the result is the
//! integration test `a_window_with_one_surface_shows_its_popups_inside_itself`.

use azul_core::{
    a11y::{AccessibilityInfo, AccessibilityRole},
    dom::{AttributeType, Dom, IdOrClass, NodeData, NodeType},
    transient::{TransientAnchor, TransientDock, TransientWindowConfig},
};
use azul_css::props::{layout::LayoutPosition, property::CssProperty};

use super::{
    present_transient_windows, SHEET_CLASS, SHEET_CLOSE_CLASS, SHEET_CONTENT_CLASS,
    SHEET_TITLE_CLASS,
};
use crate::transient::WindowPresentation;

fn popup(anchor: TransientAnchor, label: Option<&str>, children: Vec<Dom>) -> Dom {
    let config = TransientWindowConfig::opened().with_anchor(anchor);
    let mut node = Dom::create_from_data(NodeData::create_node(NodeType::TransientWindow(config)));
    if let Some(label) = label {
        node = node.with_attribute(AttributeType::AriaLabel(label.into()));
    }
    for child in children {
        node = node.with_child(child);
    }
    node
}

fn named(class: &str) -> Dom {
    Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class(class.into())].into())
}

fn classes(dom: &Dom) -> Vec<String> {
    dom.root
        .get_ids_and_classes()
        .as_ref()
        .iter()
        .filter_map(|c| c.as_class().map(str::to_owned))
        .collect()
}

fn has_class(dom: &Dom, class: &str) -> bool {
    classes(dom).iter().any(|c| c == class)
}

fn has_inline(dom: &Dom, wanted: impl Fn(&CssProperty) -> bool) -> bool {
    dom.root
        .get_style()
        .iter_inline_properties()
        .any(|(property, conditions)| conditions.as_ref().is_empty() && wanted(property))
}

fn page_with(popup: Dom) -> Dom {
    Dom::create_body().with_child(Dom::create_div().with_child(popup))
}

/// The popup node of a page made by [`page_with`].
fn popup_of(page: &Dom) -> &Dom {
    &page.children.as_ref()[0].children.as_ref()[0]
}

/// A window that opens windows is not touched: the OS does the framing.
#[test]
fn a_window_that_opens_windows_keeps_its_popups_as_the_app_made_them() {
    let page = page_with(popup(TransientAnchor::Dialog, Some("Delete?"), vec![named("panel")]));
    let mut presented = page.clone();
    assert_eq!(present_transient_windows(&mut presented, WindowPresentation::Windows), 0);
    assert!(presented == page);
}

/// Every popup is a layer over the page: fixed, above everything.
#[test]
fn a_popup_of_a_one_surface_window_is_a_fixed_layer_over_the_page() {
    let mut page = page_with(popup(TransientAnchor::Bottom, None, vec![named("panel")]));
    assert_eq!(present_transient_windows(&mut page, WindowPresentation::OneSurface), 1);
    let node = popup_of(&page);
    assert!(has_inline(node, |p| matches!(
        p,
        CssProperty::Position(v) if v.get_property() == Some(&LayoutPosition::Fixed)
    )));
    assert!(has_inline(node, |p| matches!(p, CssProperty::ZIndex(_))), "above the page");
    assert!(has_inline(node, |p| matches!(p, CssProperty::OverflowY(_))), "scrolls what does not fit");
}

/// The layer takes the pointer: a node without a handler or a cursor is not hit-tested, and a press on
/// the scrim of a modal would then reach the page under it.
#[test]
fn every_layer_is_a_target_of_the_pointer() {
    for anchor in [TransientAnchor::Bottom, TransientAnchor::Dialog] {
        let mut page = page_with(popup(anchor, Some("Delete?"), vec![named("panel")]));
        present_transient_windows(&mut page, WindowPresentation::OneSurface);
        assert!(
            has_inline(popup_of(&page), |p| matches!(p, CssProperty::Cursor(_))),
            "{anchor:?}: the layer declares a cursor, so it is hit-tested"
        );
    }
}

/// The `env()` references a node's own style carries: the names of the window's insets its declarations are read from
/// (`env:keyboard-inset-height`). They are dynamic declarations of the stylesheet parser; the property parser drops them.
fn env_references(dom: &Dom) -> Vec<String> {
    use azul_css::css::CssDeclaration;
    dom.root
        .get_style()
        .rules
        .as_ref()
        .iter()
        .flat_map(|rule| rule.declarations.as_ref().iter())
        .filter_map(|declaration| match declaration {
            CssDeclaration::Dynamic(dynamic) => Some(dynamic.dynamic_id.as_str().to_owned()),
            _ => None,
        })
        .collect()
}

/// The sheet of a dialog stands on the soft keyboard: the scrim reads the keyboard's height as its bottom
/// padding (`env(keyboard-inset-height)`), so a field of the dialog is never hidden by the keyboard it asked for.
#[test]
fn the_scrim_leaves_room_for_the_keyboard() {
    let mut page = page_with(popup(TransientAnchor::Dialog, Some("Sign in"), vec![named("panel")]));
    present_transient_windows(&mut page, WindowPresentation::OneSurface);
    assert!(
        env_references(popup_of(&page)).iter().any(|id| id == "env:keyboard-inset-height"),
        "the scrim's bottom padding is the keyboard's height: {:?}",
        env_references(popup_of(&page))
    );
}

/// ... and the card keeps clear of the home indicator (`env(safe-area-inset-bottom)`), a sheet's bottom edge being the window's.
#[test]
fn the_sheet_keeps_clear_of_the_home_indicator() {
    let mut page = page_with(popup(TransientAnchor::Dialog, Some("Sign in"), vec![named("panel")]));
    present_transient_windows(&mut page, WindowPresentation::OneSurface);
    let card = &popup_of(&page).children.as_ref()[0];
    assert!(has_class(card, SHEET_CLASS));
    assert!(
        env_references(card).iter().any(|id| id == "env:safe-area-inset-bottom"),
        "the card's bottom padding is the inset: {:?}",
        env_references(card)
    );
}

/// A sheet on a phone, a centred card from 600 px of width: the scrim and the card each carry the rule that flips at the
/// breakpoint (a `@media (min-width: 600px)` block of their style, kept as a conditional rule).
#[test]
fn the_scrim_and_the_card_carry_the_rule_of_the_600_px_breakpoint() {
    let mut page = page_with(popup(TransientAnchor::Dialog, Some("Sign in"), vec![named("panel")]));
    present_transient_windows(&mut page, WindowPresentation::OneSurface);
    let conditional = |dom: &Dom| {
        dom.root
            .get_style()
            .rules
            .as_ref()
            .iter()
            .any(|rule| !rule.conditions.as_ref().is_empty() && !rule.declarations.as_ref().is_empty())
    };
    assert!(conditional(popup_of(&page)), "the scrim centres the card from 600 px");
    assert!(conditional(&popup_of(&page).children.as_ref()[0]), "the card is no full-width sheet from 600 px");
}

/// A popover is what its panel says: no chrome of the engine's.
#[test]
fn an_anchored_popup_gets_no_sheet() {
    let mut page = page_with(popup(TransientAnchor::Bottom, Some("Details"), vec![named("panel")]));
    present_transient_windows(&mut page, WindowPresentation::OneSurface);
    let node = popup_of(&page);
    assert_eq!(node.children.as_ref().len(), 1);
    assert!(has_class(&node.children.as_ref()[0], "panel"), "its content, as it was");
}

/// A window the OS would have framed (a dialog) gets the frame: a sheet with the title the title
/// bar was, the content, and the close box - last in tree order, as the dialog's own.
#[test]
fn a_dialog_gets_a_sheet_with_a_title_the_content_and_a_close_box() {
    let mut page = page_with(popup(TransientAnchor::Dialog, Some("Delete?"), vec![named("panel")]));
    assert_eq!(present_transient_windows(&mut page, WindowPresentation::OneSurface), 1);
    let node = popup_of(&page);
    assert_eq!(node.children.as_ref().len(), 1, "the sheet is the one child of the scrim");
    let sheet = &node.children.as_ref()[0];
    assert!(has_class(sheet, SHEET_CLASS));
    let parts: Vec<&Dom> = sheet.children.as_ref().iter().collect();
    assert_eq!(parts.len(), 3, "title, content, close box");
    assert!(has_class(parts[0], SHEET_TITLE_CLASS));
    assert!(has_class(parts[1], SHEET_CONTENT_CLASS));
    assert!(has_class(parts[2], SHEET_CLOSE_CLASS));
    assert!(
        has_class(&parts[1].children.as_ref()[0], "panel"),
        "what the app made sits in the content"
    );
    assert!(
        !parts[2].root.get_callbacks().as_ref().is_empty(),
        "the close box answers a press"
    );
}

/// The sheet is the dialog for a screen reader, named by its title - unless the content brings a
/// dialog of its own (the widget's panel).
#[test]
fn the_sheet_is_the_dialog_unless_the_content_is_one() {
    let mut plain = page_with(popup(TransientAnchor::Dialog, Some("Delete?"), vec![named("panel")]));
    present_transient_windows(&mut plain, WindowPresentation::OneSurface);
    let sheet = &popup_of(&plain).children.as_ref()[0];
    assert!(matches!(
        sheet.root.get_accessibility_info().map(|a| a.role),
        Some(AccessibilityRole::Dialog)
    ));

    let own = named("panel").with_accessibility_info(AccessibilityInfo {
        role: AccessibilityRole::Dialog,
        ..Default::default()
    });
    let mut widget = page_with(popup(TransientAnchor::Dialog, Some("Delete?"), vec![own]));
    present_transient_windows(&mut widget, WindowPresentation::OneSurface);
    let sheet = &popup_of(&widget).children.as_ref()[0];
    assert!(
        sheet.root.get_accessibility_info().is_none(),
        "one dialog, not a dialog in a dialog"
    );
}

/// A dialog window the app asked to have no title bar has no header either.
#[test]
fn a_window_without_a_title_bar_has_no_header() {
    let node = popup(TransientAnchor::Dialog, Some("Delete?"), vec![named("panel")])
        .with_attribute(AttributeType::Titlebar(false));
    let mut page = page_with(node);
    present_transient_windows(&mut page, WindowPresentation::OneSurface);
    let sheet = &popup_of(&page).children.as_ref()[0];
    let parts: Vec<&Dom> = sheet.children.as_ref().iter().collect();
    assert_eq!(parts.len(), 1, "the content alone");
    assert!(has_class(parts[0], SHEET_CONTENT_CLASS));
}

/// A popover that asks for a title bar gets the header: the rule is the OS window's.
#[test]
fn a_popover_that_asks_for_a_title_bar_gets_the_header() {
    let node = popup(TransientAnchor::Viewport, Some("Colour"), vec![named("panel")])
        .with_attribute(AttributeType::Titlebar(true));
    let mut page = page_with(node);
    present_transient_windows(&mut page, WindowPresentation::OneSurface);
    let sheet = &popup_of(&page).children.as_ref()[0];
    assert_eq!(sheet.children.as_ref().len(), 3);
}

/// The pass runs where the DOM is made and again where it is styled: the second finds the sheet.
#[test]
fn the_pass_is_idempotent() {
    let mut once = page_with(popup(TransientAnchor::Dialog, Some("Delete?"), vec![named("panel")]));
    present_transient_windows(&mut once, WindowPresentation::OneSurface);
    let mut twice = once.clone();
    present_transient_windows(&mut twice, WindowPresentation::OneSurface);
    // The style the layer carries is upserted, the sheet is not made again.
    assert_eq!(popup_of(&twice).children.as_ref().len(), 1);
    assert_eq!(popup_of(&twice).children.as_ref()[0].children.as_ref().len(), 3);
    assert_eq!(
        classes(&popup_of(&twice).children.as_ref()[0]),
        classes(&popup_of(&once).children.as_ref()[0])
    );
}

/// An inline panel (`dock="inline"`) is content of the page already, a popup a window the app tore
/// off is not one: only `dock="popup"` is presented.
#[test]
fn an_inline_dock_is_left_in_the_page() {
    let config = TransientWindowConfig::opened().with_dock(TransientDock::Inline);
    let inline = Dom::create_from_data(NodeData::create_node(NodeType::TransientWindow(config)))
        .with_child(named("panel"));
    let mut page = page_with(inline);
    let before = page.clone();
    assert_eq!(present_transient_windows(&mut page, WindowPresentation::OneSurface), 0);
    assert!(page == before);
}

/// A dropdown opened from a dialog is a popup inside the dialog's content: both are presented.
#[test]
fn a_popup_inside_a_dialog_is_presented_too() {
    let inner = popup(TransientAnchor::Bottom, None, vec![named("list")]);
    let mut page = page_with(popup(
        TransientAnchor::Dialog,
        Some("Settings"),
        vec![Dom::create_div().with_child(inner)],
    ));
    assert_eq!(present_transient_windows(&mut page, WindowPresentation::OneSurface), 2);
}

/// The scrim of a modal is the theme's dim: a background of the node itself.
#[test]
fn the_scrim_dims_the_page_in_the_colour_of_the_theme() {
    let mut page = page_with(popup(TransientAnchor::Dialog, Some("Delete?"), vec![named("panel")]));
    present_transient_windows(&mut page, WindowPresentation::OneSurface);
    assert!(has_inline(popup_of(&page), |p| matches!(p, CssProperty::BackgroundContent(_))));
}
