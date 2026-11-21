//! A pressed application button sinks its curved sides with its face: no
//! seam where the middle meets the curves.
//!
//! Flora cuts the ribbon's application button as an Australis tab: the
//! middle is the button's own box, its two S-curved sides are child nodes
//! hung off it. Pressed, the middle turns to the sunken stone (its deep
//! colour and the sunken rig, the bottom shadow band included) while the
//! curves kept the raised stone: a press drew the sunken face between two
//! raised feet, and the bottom band stopped where the curves began. The
//! curves are drawn as part of the button, as a browser draws `::before` /
//! `::after`, so they take its `:active` and paint the pressed face too.

use azul_core::{
    dom::{Dom, NodeId},
    styled_dom::{ActiveChange, StyledDom},
};
use azul_css::props::{
    property::{CssProperty, CssPropertyType},
    style::StyleBackgroundContent,
};
use azul_layout::widgets::{
    ribbon::{Ribbon, RibbonAppButton, RibbonTab, RibbonTabVec},
    themes::UiTheme,
};

fn with_class(sd: &StyledDom, class: &str) -> Vec<NodeId> {
    let nodes = sd.node_data.as_container();
    (0..nodes.len())
        .map(NodeId::new)
        .filter(|n| {
            nodes[*n]
                .get_ids_and_classes()
                .iter()
                .any(|c| c.as_class() == Some(class))
        })
        .collect()
}

fn is_inside(sd: &StyledDom, node: NodeId, ancestor: NodeId) -> bool {
    let hierarchy = sd.node_hierarchy.as_container();
    let mut cur = hierarchy[node].parent_id();
    while let Some(n) = cur {
        if n == ancestor {
            return true;
        }
        cur = hierarchy[n].parent_id();
    }
    false
}

fn layers(sd: &StyledDom, node: NodeId) -> Vec<StyleBackgroundContent> {
    let cache = sd.get_css_property_cache();
    let node_data = &sd.node_data.as_container()[node];
    let state = &sd.styled_nodes.as_container()[node].styled_node_state;
    match cache.get_property(node_data, &node, state, &CssPropertyType::BackgroundContent) {
        Some(CssProperty::BackgroundContent(v)) => {
            v.get_property().map(|l| l.as_ref().to_vec()).unwrap_or_default()
        }
        _ => Vec::new(),
    }
}

/// The pressed node and every ancestor: the chain the shell activates.
fn chain(sd: &StyledDom, node: NodeId) -> Vec<NodeId> {
    let hierarchy = sd.node_hierarchy.as_container();
    let mut out = vec![node];
    let mut cur = hierarchy[node].parent_id();
    while let Some(n) = cur {
        out.push(n);
        cur = hierarchy[n].parent_id();
    }
    out
}

#[test]
fn a_pressed_app_button_sinks_its_curves_with_its_face() {
    let ribbon = Ribbon::new(RibbonTabVec::from_vec(vec![
        RibbonTab::new("HOME".into()),
        RibbonTab::new("INSERT".into()),
    ]))
    .with_app_button(RibbonAppButton::new("FILE".into()))
    .with_theme(UiTheme::Flora);
    let mut dom = Dom::create_body().with_child(ribbon.dom());
    let mut sd = StyledDom::create(&mut dom, azul_css::css::Css::empty());

    let button = with_class(&sd, "__azul-native-ribbon-appbutton")[0];
    let fills: Vec<NodeId> = with_class(&sd, "__azul-native-tab-curve-fill")
        .into_iter()
        .filter(|n| is_inside(&sd, *n, button))
        .collect();
    assert_eq!(fills.len(), 2, "the button's two curves");

    // The middle's face lies over the rolled metal: its stone is the layer
    // after the metal, its last layer the rig's bottom band.
    let middle_at_rest = layers(&sd, button);
    let rest_fill = layers(&sd, fills[0]);
    assert_eq!(
        rest_fill.first(),
        middle_at_rest.get(1),
        "premise: at rest a curve holds the middle's stone"
    );

    let _ = sd.restyle_on_state_change(
        None,
        None,
        Some(ActiveChange {
            deactivated: Vec::new(),
            activated: chain(&sd, button),
        }),
    );
    let middle_pressed = layers(&sd, button);
    assert_ne!(middle_pressed, middle_at_rest, "premise: the middle sinks");
    for fill in fills {
        let pressed_fill = layers(&sd, fill);
        assert_eq!(
            pressed_fill.first(),
            middle_pressed.get(1),
            "a pressed curve holds the middle's sunken stone"
        );
        assert!(
            middle_pressed.last().is_some_and(|band| pressed_fill.contains(band)),
            "the sunken rig's bottom band runs on under the curve: {pressed_fill:?}"
        );
    }
}
