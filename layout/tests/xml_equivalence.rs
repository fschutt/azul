use azul_core::dom::{AttributeType, Dom, NodeData, NodeType, TabIndex};
use azul_layout::xml::{dom_from_parsed_xml, parse_xml, parse_xml_to_fast_dom};

fn find_div_in_dom(dom: &Dom) -> Option<&NodeData> {
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

/// `<p data-l10n="k" data-l10n-name="Alice">` is a `<p>` whose one text child is
/// the key `k`, marked localizable, with the arguments on the `<p>` - in the
/// tree builder AND in the streaming FastDom parser. Both used to turn the
/// `<p>` itself into a text node; the streaming parser then hung the
/// element's markup content under that TEXT node.
#[test]
fn a_data_l10n_element_keeps_its_tag_in_both_xml_paths() {
    let xml = r#"<body><p data-l10n="welcome-message" data-l10n-userName="Alice"></p></body>"#;

    // Path 1 (Core/Dom)
    let dom = dom_from_parsed_xml(parse_xml(xml).expect("core parses"));
    fn find_p(dom: &Dom) -> Option<&Dom> {
        if dom.root.node_type == NodeType::P {
            return Some(dom);
        }
        dom.children.as_slice().iter().find_map(find_p)
    }
    let p = find_p(&dom).expect("the <p> survives the core path");
    assert_eq!(p.children.as_slice().len(), 1, "one child: the key");
    match &p.children.as_slice()[0].root.node_type {
        NodeType::Text(t) => {
            assert!(t.as_ref().is_localizable());
            assert_eq!(t.as_ref().as_str(), "welcome-message");
        }
        other => panic!("core: expected the key text, got {other:?}"),
    }
    assert!(p.root.fluent_args.is_some(), "core: args stay on the <p>");

    // Path 2 (Layout/FastDom)
    let fast = parse_xml_to_fast_dom(xml).expect("layout parses");
    let nodes = fast.node_data.as_ref();
    let p_idx = nodes
        .iter()
        .position(|n| n.node_type == NodeType::P)
        .expect("the <p> survives the streaming path");
    assert!(nodes[p_idx].fluent_args.is_some(), "layout: args stay on the <p>");
    let key = &nodes[p_idx + 1];
    let parent = fast.node_hierarchy.as_ref()[p_idx + 1].parent_id();
    assert_eq!(parent.map(|id| id.index()), Some(p_idx), "layout: the key is the <p>'s child");
    match &key.node_type {
        NodeType::Text(t) => {
            assert!(t.as_ref().is_localizable());
            assert_eq!(t.as_ref().as_str(), "welcome-message");
        }
        other => panic!("layout: expected the key text, got {other:?}"),
    }
}
