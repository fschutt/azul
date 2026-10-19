//! A `::marker` rule (CSS Lists 3 s3.1, CSS Pseudo 4 s2.3) styles the marker
//! of the list items it selects: the marker inherits from its item and the
//! rule's declarations apply on top, for an inline list item's marker (a
//! node in its line) and a block list item's alike (the list item lays the
//! marker out; its node takes no place in the item's content).

use azul_core::{
    dom::NodeType,
    id::NodeId,
    styled_dom::StyledDom,
};
use azul_css::props::{
    basic::color::ColorU,
    layout::LayoutDisplay,
    property::{CssProperty, CssPropertyType},
};

const RED: ColorU = ColorU { r: 255, g: 0, b: 0, a: 255 };

fn styled(markup: &str) -> StyledDom {
    let parsed = azul_layout::xml::parse_xml(markup).expect("parses");
    StyledDom::create_from_dom(azul_layout::xml::dom_from_parsed_xml(parsed))
}

/// The `::marker` node of the first node with `class`.
fn marker_of(styled: &StyledDom, class: &str) -> NodeId {
    let nodes = styled.node_data.as_container();
    let hierarchy = styled.node_hierarchy.as_container();
    let host = (0..styled.node_data.len())
        .map(NodeId::new)
        .find(|id| nodes[*id].has_class(class))
        .unwrap_or_else(|| panic!("no .{class}"));
    let first = hierarchy[host].first_child_id(host).expect("a first child");
    assert!(
        matches!(nodes[first].get_node_type(), NodeType::Marker),
        ".{class} has a ::marker node"
    );
    first
}

fn color(styled: &StyledDom, node: NodeId) -> Option<ColorU> {
    let state = styled.styled_nodes.as_container()[node].styled_node_state;
    let data = &styled.node_data.as_container()[node];
    match styled
        .css_property_cache
        .ptr
        .get_property(data, &node, &state, &CssPropertyType::TextColor)
    {
        Some(CssProperty::TextColor(v)) => v.get_property().map(|c| c.inner),
        _ => None,
    }
}

#[test]
fn a_marker_rule_colours_an_inline_list_items_marker() {
    let styled = styled(
        r#"<style>
            li { display: inline list-item; }
            li::marker { color: #ff0000; }
        </style>
        <ul><li class="a">x</li></ul>"#,
    );
    assert_eq!(color(&styled, marker_of(&styled, "a")), Some(RED));
}

#[test]
fn a_marker_rule_gives_a_block_list_item_a_marker_node_outside_its_content() {
    let styled = styled(
        r#"<style>
            li::marker { color: #ff0000; }
        </style>
        <ol><li class="a">x</li></ol>"#,
    );
    let marker = marker_of(&styled, "a");
    assert_eq!(color(&styled, marker), Some(RED));
    assert_eq!(
        azul_layout::solver3::layout_tree::get_display_type(&styled, marker),
        LayoutDisplay::None,
        "the block list item lays its marker out itself"
    );
}

#[test]
fn a_marker_selector_prints_as_css_writes_it() {
    let css = azul_css::parser2::new_from_str("li::marker { color: red; }").0;
    let rule = &css.rules.as_ref()[0];
    assert_eq!(rule.path.to_string(), "li::marker");
}
