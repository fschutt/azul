//! A seek bar's hover tip shows while the pointer is over its trough and
//! hides when it leaves, by the widget's own CSS (`.track:hover > .tip`):
//! a display set from a callback outranks the cascade on every later
//! rebuild, so the tip would stay as the last callback left it.

use azul_core::{
    dom::{Dom, NodeId},
    styled_dom::{HoverChange, StyledDom},
};
use azul_css::props::{
    layout::LayoutDisplay,
    property::{CssProperty, CssPropertyType},
};
use azul_layout::widgets::{seek_bar::SeekBar, themes::UiTheme};

const TRACK: &str = "__azul-native-seek-bar-track";
const TIP: &str = "__azul-native-seek-bar-hover";

fn classed(sd: &StyledDom, class: &str) -> NodeId {
    let nodes = sd.node_data.as_container();
    (0..nodes.len())
        .map(NodeId::new)
        .find(|n| {
            nodes[*n]
                .get_ids_and_classes()
                .as_ref()
                .iter()
                .any(|c| c.as_class() == Some(class))
        })
        .unwrap_or_else(|| panic!("no .{class}"))
}

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

fn display(sd: &StyledDom, node: NodeId) -> Option<LayoutDisplay> {
    let cache = sd.get_css_property_cache();
    let node_data = &sd.node_data.as_container()[node];
    let state = &sd.styled_nodes.as_container()[node].styled_node_state;
    match cache.get_property(node_data, &node, state, &CssPropertyType::Display) {
        Some(CssProperty::Display(v)) => v.get_property().copied(),
        _ => None,
    }
}

#[test]
fn a_seek_bars_hover_tip_shows_while_the_pointer_is_over_its_trough() {
    let mut dom =
        Dom::create_body().with_child(SeekBar::create(30.0, 200.0).with_theme(UiTheme::Flat).dom());
    let mut sd = StyledDom::create(&mut dom, azul_css::css::Css::empty());
    let (track, tip) = (classed(&sd, TRACK), classed(&sd, TIP));
    assert_eq!(display(&sd, tip), Some(LayoutDisplay::None), "hidden at rest");

    let over = chain(&sd, track);
    let _ = sd.restyle_on_state_change(
        None,
        Some(HoverChange {
            left_nodes: Vec::new(),
            entered_nodes: over.clone(),
        }),
        None,
    );
    assert_eq!(display(&sd, tip), Some(LayoutDisplay::Flex), "shown over the trough");

    let _ = sd.restyle_on_state_change(
        None,
        Some(HoverChange {
            left_nodes: over,
            entered_nodes: Vec::new(),
        }),
        None,
    );
    assert_eq!(display(&sd, tip), Some(LayoutDisplay::None), "hidden again when it leaves");
}
