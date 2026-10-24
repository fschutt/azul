//! `list-style-type: <string>` (CSS Lists 3 s3.2): the string is the marker,
//! as it is, with no number and no suffix.

use azul_core::{dom::NodeType, id::NodeId, styled_dom::StyledDom};

fn styled(markup: &str) -> StyledDom {
    let parsed = azul_layout::xml::parse_xml(markup).expect("parses");
    StyledDom::create_from_dom(azul_layout::xml::dom_from_parsed_xml(parsed))
}

/// The text of the `::marker` of the first node with `class`.
fn marker_text(styled: &StyledDom, class: &str) -> String {
    let nodes = styled.node_data.as_container();
    let hierarchy = styled.node_hierarchy.as_container();
    let host = (0..styled.node_data.len())
        .map(NodeId::new)
        .find(|id| nodes[*id].has_class(class))
        .unwrap_or_else(|| panic!("no .{class}"));
    let marker = hierarchy[host].first_child_id(host).expect("a first child");
    assert!(
        matches!(nodes[marker].get_node_type(), NodeType::Marker),
        "an inline list item starts with its ::marker"
    );
    let text = hierarchy[marker].first_child_id(marker).expect("the marker's text");
    match nodes[text].get_node_type() {
        NodeType::Text(s) => s.as_ref().as_str().to_string(),
        other => panic!("not text: {other:?}"),
    }
}

#[test]
fn a_string_list_style_type_is_the_marker_text() {
    let styled = styled(
        r#"<style>
            li { display: inline list-item; }
            .arrow { list-style-type: "→ "; }
            .escaped { list-style-type: '\2605  '; }
        </style>
        <ul><li class="arrow">a</li><li class="escaped">b</li></ul>"#,
    );
    assert_eq!(marker_text(&styled, "arrow"), "→ ");
    assert_eq!(marker_text(&styled, "escaped"), "\u{2605} ");
}
