//! A node's OWN style - `NodeData::with_css`, or a `Css::parse_inline` set
//! with `with_style` / `CallbackInfo::set_node_style` - takes a nested
//! `:hover { .. }` block: it applies while the node is hovered, and only
//! then.
//!
//! The cascade reads a node's own style by its rules' CONDITIONS and never
//! matches their selectors, but the parser put a nested `:hover` into the
//! rule's SELECTOR (`*:hover`). With no condition the rule counted as a
//! resting one: `color: blue; :hover { color: red; }` stayed blue under the
//! pointer (the resting rule is emitted last), a lone `:hover { color: red;
//! }` was red at rest, and the node got no hit-test tag, so the engine never
//! even hovered it.

use azul_core::{
    dom::{Dom, NodeData, NodeId},
    styled_dom::{StyledDom, StyledNodeState},
};
use azul_css::{
    css::Css,
    props::{
        basic::color::ColorU,
        property::{CssProperty, CssPropertyType},
    },
};

/// `body > node`: the node under test.
const NODE: NodeId = NodeId::new(1);
const BLUE: ColorU = ColorU::rgb(0, 0, 255);
const RED: ColorU = ColorU::rgb(255, 0, 0);

/// The same text in every test: blue, red under the pointer.
const STYLE: &str = "color: blue; :hover { color: red; }";

fn styled(node: Dom) -> StyledDom {
    StyledDom::create_from_dom(Dom::create_body().with_child(node))
}

/// A div whose OWN style is `css`.
fn own_style(css: &str) -> Dom {
    Dom::create_from_data(NodeData::create_div().with_css(css))
}

/// The text colour the node resolves to, at rest or hovered.
fn colour(sd: &StyledDom, hover: bool) -> Option<ColorU> {
    let state = StyledNodeState {
        hover,
        ..StyledNodeState::default()
    };
    let nodes = sd.node_data.as_container();
    let property = sd.get_css_property_cache().get_property(
        &nodes[NODE],
        &NODE,
        &state,
        &CssPropertyType::TextColor,
    )?;
    match property {
        CssProperty::TextColor(v) => v.get_property().map(|c| c.inner),
        _ => None,
    }
}

#[test]
fn a_nodes_own_hover_block_applies_under_the_pointer_and_only_there() {
    let sd = styled(own_style(STYLE));
    assert_eq!(
        colour(&sd, false),
        Some(BLUE),
        "at rest: the resting colour"
    );
    assert_eq!(colour(&sd, true), Some(RED), "hovered: the :hover block");
}

#[test]
fn a_lone_hover_block_of_a_nodes_own_style_does_not_apply_at_rest() {
    let sd = styled(own_style(":hover { color: red; }"));
    assert_ne!(
        colour(&sd, false),
        Some(RED),
        "at rest the :hover block must not apply"
    );
    assert_eq!(colour(&sd, true), Some(RED), "hovered it does");
}

/// The route `CallbackInfo::set_node_style` takes: a stylesheet parsed as
/// inline style, stored as the node's own.
#[test]
fn an_inline_parsed_stylesheet_set_as_a_nodes_style_takes_its_hover_block() {
    let sd = styled(Dom::create_div().with_style(Css::parse_inline(STYLE)));
    assert_eq!(colour(&sd, false), Some(BLUE));
    assert_eq!(colour(&sd, true), Some(RED));
}

/// The engine only hovers a node that has a hit-test tag, and a node gets
/// one for a `:hover` style only if the cascade sees it.
#[test]
fn a_node_with_its_own_hover_block_is_hit_testable() {
    let sd = styled(own_style(STYLE));
    assert!(
        sd.tag_ids_to_node_ids
            .as_ref()
            .iter()
            .any(|m| m.node_id.into_crate_internal() == Some(NODE)),
        "the node declares a :hover style, so it must be hit-testable"
    );
}

/// PIN: `Dom::with_css` puts the same text into the node's SCOPED
/// stylesheet, which the cascade selector-matches: its `:hover` rule works
/// there and must keep working.
#[test]
fn a_scoped_stylesheets_hover_block_still_applies_only_when_hovered() {
    let sd = styled(Dom::create_div().with_css(STYLE));
    assert_eq!(colour(&sd, false), Some(BLUE));
    assert_eq!(colour(&sd, true), Some(RED));
}
