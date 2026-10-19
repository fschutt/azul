//! A rule for a state (`.b:hover`) takes part in the cascade like any other
//! rule: while the node is in that state it competes with the resting rules
//! by the cascade order - priority, theme rank, specificity, source order -
//! and wins only where it ranks higher (CSS Cascade 4 s6, as Chrome does).
//! The state tiers used to decide first: any `:hover` rule beat every resting
//! rule, a rice of higher priority and the node's own style included, and
//! `:focus` beat `:hover` whatever their order.
//!
//! What a node inherits comes after everything it declares itself, and an
//! unscoped `*` rule stands where the cascade order puts it: above an
//! element's presentational hints, and above author rules when its priority
//! is higher.

use azul_core::{
    dom::{Dom, NodeType},
    id::NodeId,
    styled_dom::{FocusChange, HoverChange, StyledDom},
};
use azul_css::{
    css::{rule_priority, Css, CssRuleBlock},
    props::{
        basic::color::ColorU,
        property::{CssProperty, CssPropertyType},
        style::StyleBackgroundContent,
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

fn styled_markup(markup: &str) -> StyledDom {
    let parsed = azul_layout::xml::parse_xml(markup).expect("parses");
    StyledDom::create_from_dom(azul_layout::xml::dom_from_parsed_xml(parsed))
}

fn with_class(styled: &StyledDom, class: &str) -> NodeId {
    (0..styled.node_data.len())
        .map(NodeId::new)
        .find(|id| styled.node_data.as_container()[*id].has_class(class))
        .unwrap_or_else(|| panic!("no .{class}"))
}

fn the_table(styled: &StyledDom) -> NodeId {
    (0..styled.node_data.len())
        .map(NodeId::new)
        .find(|id| matches!(styled.node_data.as_container()[*id].get_node_type(), NodeType::Table))
        .expect("no <table>")
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

fn hover(styled: &mut StyledDom, nodes: Vec<NodeId>) {
    let _ = styled.restyle_on_state_change(
        None,
        Some(HoverChange {
            left_nodes: Vec::new(),
            entered_nodes: nodes,
        }),
        None,
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
    hover(&mut styled, vec![b]);
    assert!(styled.styled_nodes.as_container()[b].styled_node_state.hover);
    assert_eq!(background(&styled, b), Some(GREEN));
}

/// The node's own style ranks above every author rule (the `style`
/// attribute beats a `:hover` rule in Chrome).
#[test]
fn a_hover_rule_loses_to_the_nodes_own_resting_style() {
    let css = sheet(&[(".b:hover { background: #ff0000; }", rule_priority::AUTHOR)]);
    let mut dom = Dom::create_body()
        .with_child(Dom::create_div().with_class("b".into()).with_css("background: #00ff00;"));
    let mut styled = StyledDom::create(&mut dom, css);
    let b = with_class(&styled, "b");
    hover(&mut styled, vec![b]);
    assert_eq!(background(&styled, b), Some(GREEN));
}

/// `#list .b` is more specific than `.b:hover`: the row keeps its colour
/// under the pointer, as in Chrome.
#[test]
fn a_more_specific_resting_rule_beats_a_hover_rule() {
    let mut styled = styled_markup(
        r#"<style>
            #list .b { background: #00ff00; }
            .b:hover { background: #ff0000; }
        </style>
        <div id="list"><div class="b">row</div></div>"#,
    );
    let b = with_class(&styled, "b");
    hover(&mut styled, vec![b]);
    assert_eq!(background(&styled, b), Some(GREEN));
}

/// A focused field under the pointer: `:focus` and `:hover` rules of the
/// same specificity, so the later one wins - here the hover.
#[test]
fn of_two_states_the_rule_later_in_the_cascade_wins() {
    let mut styled = styled_markup(
        r#"<style>
            .b:focus { background: #ff0000; }
            .b:hover { background: #00ff00; }
        </style>
        <div class="b" tabindex="0">field</div>"#,
    );
    let b = with_class(&styled, "b");
    let _ = styled.restyle_on_state_change(
        Some(FocusChange {
            lost_focus: None,
            gained_focus: Some(b),
        }),
        Some(HoverChange {
            left_nodes: Vec::new(),
            entered_nodes: vec![b],
        }),
        None,
    );
    let state = styled.styled_nodes.as_container()[b].styled_node_state;
    assert!(state.focused && state.hover, "harness: both states raised");
    assert_eq!(background(&styled, b), Some(GREEN));
}

/// A colour inherited from a hovered parent only fills in what the child
/// does not declare: the child's own `color` stays.
#[test]
fn a_value_inherited_from_a_hovered_parent_yields_to_the_childs_own_rule() {
    let mut styled = styled_markup(
        r#"<style>
            .p:hover { color: #ff0000; }
            .c { color: #00ff00; }
        </style>
        <div class="p"><div class="c">text</div></div>"#,
    );
    let p = with_class(&styled, "p");
    let c = with_class(&styled, "c");
    hover(&mut styled, vec![p, c]);
    assert_eq!(color(&styled, p), Some(RED), "harness: the parent is hovered");
    assert_eq!(color(&styled, c), Some(GREEN));
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

/// A presentational hint (`<table bgcolor>`) is the weakest author rule: a
/// `*` rule beats it.
#[test]
fn a_star_rule_beats_a_presentational_hint() {
    let styled = styled_markup(
        r#"<style>* { background: #00ff00; }</style>
        <table bgcolor="#ff0000"><tr><td>x</td></tr></table>"#,
    );
    let table = the_table(&styled);
    assert_eq!(background(&styled, table), Some(GREEN));
}
