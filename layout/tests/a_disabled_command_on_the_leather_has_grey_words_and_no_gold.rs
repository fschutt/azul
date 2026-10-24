//! A disabled command on flora's leather is the quiet pill the design draws:
//! a neutral edge where an enabled command has its gold, and grey words and
//! icon, so the eye sees at once what cannot act (AzDrive's Paste with
//! nothing copied).

use azul_core::{
    dom::{Dom, NodeId, NodeType},
    styled_dom::StyledDom,
};
use azul_css::props::{
    basic::color::ColorU,
    property::{CssProperty, CssPropertyType},
};
use azul_layout::widgets::{
    ribbon::{Ribbon, RibbonButton, RibbonGroup, RibbonItem, RibbonTab, RibbonTabVec},
    tabs::TabStyle,
    themes::UiTheme,
};

/// The design's `--distx` by day.
const GREY: ColorU = ColorU::rgb(163, 160, 153);
/// The quiet pill's neutral edge by day.
const QUIET_EDGE: ColorU = ColorU::new(150, 144, 130, 115);

fn classed(sd: &StyledDom, class: &str) -> Vec<NodeId> {
    let nodes = sd.node_data.as_container();
    (0..nodes.len())
        .map(NodeId::new)
        .filter(|n| {
            nodes[*n]
                .get_ids_and_classes()
                .as_ref()
                .iter()
                .any(|c| c.as_class() == Some(class))
        })
        .collect()
}

/// The text and icon nodes inside `node`.
fn ink_nodes(sd: &StyledDom, node: NodeId) -> Vec<NodeId> {
    let hierarchy = sd.node_hierarchy.as_container();
    let nodes = sd.node_data.as_container();
    let mut out = Vec::new();
    let mut stack = vec![node];
    while let Some(n) = stack.pop() {
        if matches!(nodes[n].get_node_type(), NodeType::Text(_) | NodeType::Icon(_)) {
            out.push(n);
        }
        let mut child = hierarchy[n].first_child_id(n);
        while let Some(c) = child {
            stack.push(c);
            child = hierarchy[c].next_sibling_id();
        }
    }
    out
}

fn ink(sd: &StyledDom, node: NodeId) -> ColorU {
    let cache = sd.get_css_property_cache();
    let node_data = &sd.node_data.as_container()[node];
    let state = &sd.styled_nodes.as_container()[node].styled_node_state;
    cache.get_text_color_or_default(node_data, &node, state).inner
}

fn top_edge(sd: &StyledDom, node: NodeId) -> Option<ColorU> {
    let cache = sd.get_css_property_cache();
    let node_data = &sd.node_data.as_container()[node];
    let state = &sd.styled_nodes.as_container()[node].styled_node_state;
    match cache.get_property(node_data, &node, state, &CssPropertyType::BorderTopColor) {
        Some(CssProperty::BorderTopColor(v)) => v.get_property().map(|c| c.inner),
        _ => None,
    }
}

fn ribbon() -> StyledDom {
    let group = RibbonGroup::new("Clipboard".into()).with_items(
        vec![
            RibbonItem::SmallButton(RibbonButton::new("content_copy".into(), "Copy".into())),
            RibbonItem::SmallButton(
                RibbonButton::new("content_paste".into(), "Paste shortcut".into())
                    .with_disabled("Copy or cut something first".into()),
            ),
        ]
        .into(),
    );
    let mut dom = Dom::create_body().with_child(
        Ribbon::new(RibbonTabVec::from_vec(vec![RibbonTab::new("HOME".into()).with_group(group)]))
            .with_theme(UiTheme::Flora)
            .with_tab_style(TabStyle::Pills)
            .with_available_width(1280.0)
            .dom_desktop(),
    );
    StyledDom::create(&mut dom, azul_css::css::Css::empty())
}

#[test]
fn a_disabled_command_on_the_leather_has_grey_words_and_no_gold() {
    let sd = ribbon();
    let buttons = classed(&sd, "__azul-native-button");
    assert_eq!(buttons.len(), 2, "the two commands");
    let (enabled, disabled) = (buttons[0], buttons[1]);
    assert_ne!(top_edge(&sd, enabled), Some(QUIET_EDGE), "premise: the enabled one has its gold");
    assert_eq!(top_edge(&sd, disabled), Some(QUIET_EDGE), "the disabled one has the neutral edge");
    let words = ink_nodes(&sd, disabled);
    assert!(!words.is_empty(), "the disabled command has words and an icon");
    for n in words {
        assert_eq!(ink(&sd, n), GREY, "{n:?} of the disabled command is grey");
    }
    for n in ink_nodes(&sd, enabled) {
        assert_ne!(ink(&sd, n), GREY, "{n:?} of the enabled command is not grey");
    }
}
