//! A descendant rule nested in a node's own style (`with_css(".a .b { .. }")`) matches a `.b`
//! inside an `.a` of that node's subtree, and not a node that carries both classes.
//!
//! The tokenizer used to skip the whitespace between two selectors of a nested rule, so the
//! rule became `* .a.b`: it styled the node with both classes and missed the real descendant.

use azul_core::{
    dom::{Dom, NodeId},
    styled_dom::StyledDom,
};
use azul_css::{
    css::CssPropertyValue,
    props::{
        layout::LayoutWidth,
        property::{CssProperty, CssPropertyType},
    },
};

/// Tree (flat ids):
///   body[0]  with_css(".a .b { width: 10px; }")
///     div.a[1]
///       div[2]
///         div.b[3]   inside an .a: matches
///     div.a.b[4]     both classes, no .a above it: does not match
#[test]
fn a_nested_descendant_rule_in_with_css_matches_only_descendants() {
    let dom = Dom::create_body()
        .with_css(".a .b { width: 10px; }")
        .with_child(
            Dom::create_div().with_class("a".into()).with_child(
                Dom::create_div().with_child(Dom::create_div().with_class("b".into())),
            ),
        )
        .with_child(Dom::create_div().with_class("a".into()).with_class("b".into()));
    let styled_dom = StyledDom::create_from_dom(dom);

    let cache = styled_dom.get_css_property_cache();
    let nd = styled_dom.node_data.as_container();
    let sn = styled_dom.styled_nodes.as_container();
    let ten = CssProperty::Width(CssPropertyValue::Exact(LayoutWidth::px(10.0)));
    let has_width = |i: usize| {
        let id = NodeId::new(i);
        cache.get_property(&nd[id], &id, &sn[id].styled_node_state, &CssPropertyType::Width)
            == Some(&ten)
    };
    assert!(has_width(3), "the .b inside an .a takes the rule");
    assert!(!has_width(4), "a node with both classes and no .a above it does not");
    assert!(!has_width(1) && !has_width(2), "the .a and the plain div do not");
}
