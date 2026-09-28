use azul_core::dom::{AttributeType, Dom, NodeData, NodeType, TabIndex};
use azul_layout::xml::{dom_from_parsed_xml, parse_xml, parse_xml_to_fast_dom};

fn find_div_in_dom<'a>(dom: &'a Dom) -> Option<&'a NodeData> {
    if dom.root.node_type == NodeType::Div {
        return Some(&dom.root);
    }
    for child in dom.children.as_slice() {
        if let Some(res) = find_div_in_dom(child) {
            return Some(res);
        }
    }
    None
}

fn compare_xml_node(xml: &str, expected_contenteditable: bool, expected_tabindex: Option<TabIndex>, has_autofocus: bool, expected_placeholder: Option<&str>) {
    // Path 1 (Core/Dom)
    let parsed_xml = parse_xml(xml).expect("Failed to parse XML in core");
    let dom = dom_from_parsed_xml(parsed_xml);
    let core_node = find_div_in_dom(&dom).expect("Could not find div in Dom");

    // Path 2 (Layout/FastDom)
    let layout_fast_dom = parse_xml_to_fast_dom(xml).expect("Failed to parse XML in layout");
    let layout_node = layout_fast_dom.node_data.as_ref().iter().find(|n| n.node_type == NodeType::Div).unwrap();

    assert_eq!(core_node.node_type, layout_node.node_type);

    assert_eq!(core_node.is_contenteditable(), layout_node.is_contenteditable(), "XML: {}", xml);
    assert_eq!(core_node.is_contenteditable(), expected_contenteditable, "XML: {}", xml);

    assert_eq!(core_node.get_tab_index(), layout_node.get_tab_index(), "XML: {}", xml);
    assert_eq!(core_node.get_tab_index(), expected_tabindex, "XML: {}", xml);

    let core_attrs = core_node.attributes().as_slice();
    let layout_attrs = layout_node.attributes().as_slice();

    let check_attr = |attr: AttributeType| {
        let in_core = core_attrs.contains(&attr);
        let in_layout = layout_attrs.contains(&attr);
        assert_eq!(
            in_core, in_layout,
            "Attribute {:?} mismatch on XML: {}! core={}, layout={}",
            attr, xml, in_core, in_layout
        );
    };

    check_attr(AttributeType::Autofocus);
    if has_autofocus {
        assert!(core_attrs.contains(&AttributeType::Autofocus), "XML: {}", xml);
    }
    
    if let Some(ph) = expected_placeholder {
        check_attr(AttributeType::Placeholder(ph.into()));
        assert!(core_attrs.contains(&AttributeType::Placeholder(ph.into())), "XML: {}", xml);
    }
}

#[test]
fn test_core_and_layout_xml_parsing_equivalence_permutations() {
    compare_xml_node(
        r#"<div contenteditable="true" autofocus="true" placeholder="Enter text here" tabindex="0">hello</div>"#,
        true,
        Some(TabIndex::Auto),
        true,
        Some("Enter text here"),
    );

    compare_xml_node(
        r#"<div contenteditable="false" tabindex="-1">hello</div>"#,
        false,
        Some(TabIndex::NoKeyboardFocus),
        false,
        None,
    );

    compare_xml_node(
        r#"<div focusable="true">hello</div>"#,
        false,
        Some(TabIndex::Auto),
        false,
        None,
    );

    compare_xml_node(
        r#"<div focusable="false">hello</div>"#,
        false,
        Some(TabIndex::NoKeyboardFocus),
        false,
        None,
    );

    compare_xml_node(
        r#"<div tabindex="5">hello</div>"#,
        false,
        Some(TabIndex::OverrideInParent(5)),
        false,
        None,
    );

    compare_xml_node(
        r#"<div autofocus="true" placeholder="blank">hello</div>"#,
        false,
        None,
        true,
        Some("blank"),
    );
}
