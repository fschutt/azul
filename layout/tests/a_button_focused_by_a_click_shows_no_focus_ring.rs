//! A button focused by a click shows no focus ring.
//!
//! The user's report: clicking a button (or the switch, toggle-broken.png)
//! leaves the focus ring around it. Browsers ring a control only when its
//! focus should be visible - keyboard focus, `:focus-visible` - and a click
//! focuses it without a ring. The themes declared their command rings under
//! `:focus`, which a click raises as well. A pointer focus is the plain
//! focus restyle (`restyle_on_state_change` with a `FocusChange`): after it
//! the button's ring slots read what they read at rest.

use azul_core::{
    dom::{Dom, NodeId},
    styled_dom::{FocusChange, StyledDom},
};
use azul_css::props::property::{CssProperty, CssPropertyType};
use azul_layout::widgets::{button::Button, switch::Switch, themes::UiTheme};

/// The properties a theme draws a ring with: the four shadow slots and the
/// four border colours.
const RING_SLOTS: [CssPropertyType; 8] = [
    CssPropertyType::BoxShadowLeft,
    CssPropertyType::BoxShadowRight,
    CssPropertyType::BoxShadowTop,
    CssPropertyType::BoxShadowBottom,
    CssPropertyType::BorderTopColor,
    CssPropertyType::BorderRightColor,
    CssPropertyType::BorderBottomColor,
    CssPropertyType::BorderLeftColor,
];

fn first_with_class(sd: &StyledDom, class: &str) -> NodeId {
    let node_data = sd.node_data.as_container();
    (0..node_data.len())
        .map(NodeId::new)
        .find(|n| {
            node_data[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| c.as_class() == Some(class))
        })
        .expect("the control is in the DOM")
}

fn ring_slots(sd: &StyledDom, node: NodeId) -> Vec<Option<CssProperty>> {
    let cache = sd.get_css_property_cache();
    let node_data = &sd.node_data.as_container()[node];
    let state = &sd.styled_nodes.as_container()[node].styled_node_state;
    RING_SLOTS
        .iter()
        .map(|ty| cache.get_property(node_data, &node, state, ty).cloned())
        .collect()
}

fn assert_a_click_shows_no_ring(what: &str, control: Dom, class: &str) {
    let mut dom = Dom::create_body().with_child(control);
    let mut sd = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let node = first_with_class(&sd, class);
    let at_rest = ring_slots(&sd, node);
    let _ = sd.restyle_on_state_change(
        Some(FocusChange {
            lost_focus: None,
            gained_focus: Some(node),
        }),
        None,
        None,
    );
    assert!(
        sd.styled_nodes.as_container()[node].styled_node_state.focused,
        "harness: {what} is focused"
    );
    assert_eq!(
        ring_slots(&sd, node),
        at_rest,
        "{what}: a click focused it and a ring appeared"
    );
}

#[test]
fn a_flora_button_focused_by_a_click_shows_no_focus_ring() {
    assert_a_click_shows_no_ring(
        "a flora button",
        Button::create("Save".into()).with_theme(UiTheme::Flora).dom(),
        "__azul-native-button",
    );
}

#[test]
fn a_flat_button_focused_by_a_click_shows_no_focus_ring() {
    assert_a_click_shows_no_ring(
        "a flat button",
        Button::create("Save".into()).with_theme(UiTheme::Flat).dom(),
        "__azul-native-button",
    );
}

#[test]
fn a_flora_switch_focused_by_a_click_shows_no_focus_ring() {
    assert_a_click_shows_no_ring(
        "a flora switch",
        Switch::create(false).with_theme(UiTheme::Flora).dom(),
        "__azul-native-switch",
    );
}
