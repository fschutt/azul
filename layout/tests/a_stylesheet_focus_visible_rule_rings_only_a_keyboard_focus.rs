//! A stylesheet's `:focus-visible` rule applies to a keyboard focus and not
//! to a click.
//!
//! Browsers ring a control only when its focus should be seen: reached with
//! Tab, not clicked. The engine tracks that (`styled_node_state.focus_visible`)
//! and the widgets' inline styles match it, but the stylesheet parser knew no
//! `:focus-visible`, so an app's own CSS could only ring on `:focus` - which
//! a click raises as well.

use azul_core::{
    dom::{Dom, IdOrClass, NodeId, TabIndex},
    styled_dom::{FocusChange, StyledDom},
};
use azul_css::props::{
    basic::ColorU,
    property::{CssProperty, CssPropertyType},
};

const CSS: &str = "
    .field { background-color: #ffffff; }
    .field:focus-visible { background-color: #ff0000; }
";

fn background(sd: &StyledDom, node: NodeId) -> Option<ColorU> {
    let cache = sd.get_css_property_cache();
    let node_data = &sd.node_data.as_container()[node];
    let state = &sd.styled_nodes.as_container()[node].styled_node_state;
    match cache.get_property(node_data, &node, state, &CssPropertyType::BackgroundContent) {
        Some(CssProperty::BackgroundContent(v)) => v
            .get_property()
            .and_then(|layers| layers.as_ref().first().cloned())
            .and_then(|layer| match layer {
                azul_css::props::style::StyleBackgroundContent::Color(c) => Some(c),
                _ => None,
            }),
        _ => None,
    }
}

#[test]
fn a_stylesheet_focus_visible_rule_rings_only_a_keyboard_focus() {
    let (css, warnings) = azul_css::parser2::new_from_str(CSS);
    assert!(warnings.is_empty(), "the stylesheet parses: {warnings:?}");
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_ids_and_classes(vec![IdOrClass::Class("field".into())].into())
            .with_tab_index(TabIndex::Auto),
    );
    let mut sd = StyledDom::create(&mut dom, css);
    let field = NodeId::new(1);
    let white = Some(ColorU::rgb(255, 255, 255));
    assert_eq!(background(&sd, field), white, "at rest");

    // A click focuses the field: its focus is not seen.
    let _ = sd.restyle_on_state_change(
        Some(FocusChange {
            lost_focus: None,
            gained_focus: Some(field),
        }),
        None,
        None,
    );
    assert_eq!(background(&sd, field), white, "a click rings nothing");

    // The same focus reached with the keyboard is seen.
    let _ = sd.restyle_on_focus_visibility(field, true);
    assert_eq!(
        background(&sd, field),
        Some(ColorU::rgb(255, 0, 0)),
        "a keyboard focus takes the :focus-visible rule"
    );
}
