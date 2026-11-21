//! A node a callback inserts at runtime (`CallbackInfo::insert_child_node`)
//! is styled as if the DOM had been created with it: the author sheet's
//! `::before` and `::after` rules give it its generated nodes, with their
//! text, without a rebuild.

use azul_core::{
    dom::{Dom, NodeType},
    id::NodeId,
    styled_dom::StyledDom,
};

fn children(styled: &StyledDom, node: NodeId) -> Vec<NodeId> {
    let hierarchy = styled.node_hierarchy.as_container();
    let mut out = Vec::new();
    let mut next = hierarchy[node].first_child_id(node);
    while let Some(c) = next {
        out.push(c);
        next = hierarchy[c].next_sibling_id();
    }
    out
}

fn text_of_generated(styled: &StyledDom, host: NodeId, kind_before: bool) -> Option<String> {
    let node = children(styled, host).into_iter().find(|c| {
        match styled.node_data.as_container()[*c].get_node_type() {
            NodeType::Before => kind_before,
            NodeType::After => !kind_before,
            _ => false,
        }
    })?;
    let text = children(styled, node);
    match styled.node_data.as_container()[*text.first()?].get_node_type() {
        NodeType::Text(s) => Some(s.as_ref().as_str().to_string()),
        _ => None,
    }
}

#[test]
fn a_node_inserted_at_runtime_gets_its_before_and_after() {
    let parsed = azul_layout::xml::parse_xml(
        r#"<style>
            .x::before { content: "+"; }
            .x::after { content: "-"; }
        </style>
        <div class="list"><p class="x">a</p></div>"#,
    )
    .expect("parses");
    let mut styled = StyledDom::create_from_dom(azul_layout::xml::dom_from_parsed_xml(parsed));
    let list = (0..styled.node_data.len())
        .map(NodeId::new)
        .find(|id| styled.node_data.as_container()[*id].has_class("list"))
        .expect("the list");
    let first = children(&styled, list)[0];
    assert_eq!(text_of_generated(&styled, first, true).as_deref(), Some("+"), "harness");

    let new_id = NodeId::new(styled.node_data.len());
    let node = Dom::create_node(NodeType::P).with_class("x".into()).root;
    assert!(styled.insert_runtime_child(list, node), "the list is on the rightmost spine");

    assert_eq!(children(&styled, list).last(), Some(&new_id));
    assert_eq!(text_of_generated(&styled, new_id, true).as_deref(), Some("+"));
    assert_eq!(text_of_generated(&styled, new_id, false).as_deref(), Some("-"));
}

#[test]
fn a_parent_off_the_rightmost_spine_takes_no_child_at_the_end() {
    let parsed = azul_layout::xml::parse_xml(
        r#"<div class="early"><p>a</p></div><div class="late"><p>b</p></div>"#,
    )
    .expect("parses");
    let mut styled = StyledDom::create_from_dom(azul_layout::xml::dom_from_parsed_xml(parsed));
    let early = (0..styled.node_data.len())
        .map(NodeId::new)
        .find(|id| styled.node_data.as_container()[*id].has_class("early"))
        .expect("the early div");
    let before = children(&styled, early).len();
    let node = Dom::create_node(NodeType::P).root;
    assert!(!styled.insert_runtime_child(early, node));
    assert_eq!(children(&styled, early).len(), before, "the early div kept its children");
}
