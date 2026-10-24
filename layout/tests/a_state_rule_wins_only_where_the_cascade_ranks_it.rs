//! Priority ranks above the state: a rule for a state (`.b:hover`) wins over
//! the resting value only at the same or a higher `rule_priority`. A rice at
//! `priority: app` re-skins the app's own DOM and keeps its value under the
//! pointer, and the node's own style (`rule_priority::INLINE`) beats an
//! author's `:hover` rule, as the `style` attribute does in Chrome. The state
//! tiers used to decide first, whatever the priority. At one priority the
//! stronger state still wins (the widgets' looks are written for that).
//!
//! An unscoped `*` rule stands where its priority puts it too: above an
//! element's presentational hints, and above the app's rules when it comes
//! from a rice of a higher priority.

use azul_core::{
    dom::{Dom, NodeType},
    id::NodeId,
    styled_dom::{HoverChange, StyledDom},
};
use azul_css::{
    css::{rule_priority, Css, CssRuleBlock},
    dynamic_selector::CssPropertyWithConditions,
    props::{
        basic::color::ColorU,
        property::{CssProperty, CssPropertyType},
        style::{StyleBackgroundContent, StyleTextColor},
    },
};

const RED: ColorU = ColorU { r: 255, g: 0, b: 0, a: 255 };
const GREEN: ColorU = ColorU { r: 0, g: 255, b: 0, a: 255 };

/// The rules of `source`, each at `priority`.
fn rules_at(source: &str, priority: u8) -> Vec<CssRuleBlock> {
    let mut rules = azul_css::parser2::new_from_str(source)
        .0
        .rules
        .into_library_owned_vec();
    for rule in &mut rules {
        rule.priority = priority;
    }
    rules
}

fn sheet(parts: &[(&str, u8)]) -> Css {
    Css::new(
        parts
            .iter()
            .flat_map(|(source, priority)| rules_at(source, *priority))
            .collect(),
    )
}

fn with_class(styled: &StyledDom, class: &str) -> NodeId {
    (0..styled.node_data.len())
        .map(NodeId::new)
        .find(|id| styled.node_data.as_container()[*id].has_class(class))
        .unwrap_or_else(|| panic!("no .{class}"))
}

fn background(styled: &StyledDom, node: NodeId) -> Option<ColorU> {
    let state = styled.styled_nodes.as_container()[node].styled_node_state;
    azul_layout::solver3::getters::get_background_contents(styled, node, &state)
        .into_iter()
        .find_map(|bg| match bg {
            StyleBackgroundContent::Color(c) => Some(c),
            _ => None,
        })
}

fn color(styled: &StyledDom, node: NodeId) -> Option<ColorU> {
    let state = styled.styled_nodes.as_container()[node].styled_node_state;
    let node_data = &styled.node_data.as_container()[node];
    match styled.css_property_cache.ptr.get_property(
        node_data,
        &node,
        &state,
        &CssPropertyType::TextColor,
    ) {
        Some(CssProperty::TextColor(v)) => v.get_property().map(|c| c.inner),
        _ => None,
    }
}

fn hover(styled: &mut StyledDom, node: NodeId) {
    let _ = styled.restyle_on_state_change(
        None,
        Some(HoverChange {
            left_nodes: Vec::new(),
            entered_nodes: vec![node],
        }),
        None,
    );
    assert!(
        styled.styled_nodes.as_container()[node].styled_node_state.hover,
        "harness: hovered"
    );
}

/// A rice at `priority: app` paints `.b` green at rest; the app's own sheet
/// paints it red under the pointer. The rice ranks higher, so it stays green.
#[test]
fn a_hover_rule_loses_to_a_resting_rule_of_higher_priority() {
    let css = sheet(&[
        (".b:hover { background: #ff0000; }", rule_priority::AUTHOR),
        (".b { background: #00ff00; }", rule_priority::APP),
    ]);
    let mut dom = Dom::create_body().with_child(Dom::create_div().with_class("b".into()));
    let mut styled = StyledDom::create(&mut dom, css);
    let b = with_class(&styled, "b");
    assert_eq!(background(&styled, b), Some(GREEN), "harness: resting");
    hover(&mut styled, b);
    assert_eq!(background(&styled, b), Some(GREEN));
}

/// At one priority the hover rule still wins: the state ranks the rules of a
/// priority.
#[test]
fn a_hover_rule_beats_a_resting_rule_of_its_own_priority() {
    let css = sheet(&[
        (".b:hover { background: #ff0000; }", rule_priority::APP),
        (".b { background: #00ff00; }", rule_priority::APP),
    ]);
    let mut dom = Dom::create_body().with_child(Dom::create_div().with_class("b".into()));
    let mut styled = StyledDom::create(&mut dom, css);
    let b = with_class(&styled, "b");
    hover(&mut styled, b);
    assert_eq!(background(&styled, b), Some(RED));
}

/// The node's own resting style ranks above every author rule, a `:hover`
/// one included (the `style` attribute in Chrome).
#[test]
fn a_hover_rule_loses_to_the_nodes_own_resting_style() {
    let css = sheet(&[(".b:hover { color: #ff0000; }", rule_priority::AUTHOR)]);
    let own = CssPropertyWithConditions::simple(CssProperty::const_text_color(StyleTextColor {
        inner: GREEN,
    }));
    let mut dom = Dom::create_body().with_child(
        Dom::create_div()
            .with_class("b".into())
            .with_css_props(vec![own].into()),
    );
    let mut styled = StyledDom::create(&mut dom, css);
    let b = with_class(&styled, "b");
    hover(&mut styled, b);
    assert_eq!(color(&styled, b), Some(GREEN));
}

/// A `*` rule of a rice at `priority: app` re-skins the whole app: it ranks
/// above the app's own rules, however specific.
#[test]
fn a_star_rule_of_higher_priority_beats_an_author_rule() {
    let css = sheet(&[
        ("* { background: #00ff00; }", rule_priority::APP),
        (".b { background: #ff0000; }", rule_priority::AUTHOR),
    ]);
    let mut dom = Dom::create_body().with_child(Dom::create_div().with_class("b".into()));
    let styled = StyledDom::create(&mut dom, css);
    let b = with_class(&styled, "b");
    assert_eq!(background(&styled, b), Some(GREEN));
}

/// A presentational hint (`<table bgcolor>`) is the weakest author
/// declaration: a `*` rule of the app beats it.
#[test]
fn a_star_rule_beats_a_presentational_hint() {
    let parsed = azul_layout::xml::parse_xml(
        r##"<style>* { background: #00ff00; }</style>
        <table bgcolor="#ff0000"><tr><td>x</td></tr></table>"##,
    )
    .expect("parses");
    let styled = StyledDom::create_from_dom(azul_layout::xml::dom_from_parsed_xml(parsed));
    let table = (0..styled.node_data.len())
        .map(NodeId::new)
        .find(|id| matches!(styled.node_data.as_container()[*id].get_node_type(), NodeType::Table))
        .expect("the table");
    assert_eq!(background(&styled, table), Some(GREEN));
}
