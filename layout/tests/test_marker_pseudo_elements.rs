/// Tests for ::marker pseudo-element generation and layout tree structure
/// 
/// Verifies that:
/// 1. ::marker pseudo-elements are created as first child of list-items
/// 2. Markers reference the same DOM node as their parent list-item
/// 3. Layout tree structure matches browser implementations

#[cfg(test)]
mod tests {
    use azul_core::dom::NodeType;
    use azul_core::styled_dom::StyledDom;
    use azul_css::Css;
    use azul_layout::solver3::layout_tree::{generate_layout_tree, PseudoElement};
    use azul_layout::text3::cache::InMemoryFontDb;
    use azul_layout::font::FontCache;
    use azul_layout::solver3::LayoutContext;
    use std::collections::BTreeMap;

    fn create_test_tree(html: &str, css: &str) -> Result<azul_layout::solver3::LayoutTree<azul_layout::font::parsed::ParsedFont>, azul_layout::solver3::LayoutError> {
        // Parse HTML
        let dom = azul_core::dom::Dom::from_html(html).expect("Failed to parse HTML");
        
        // Parse CSS
        let css = Css::from_string(css).expect("Failed to parse CSS");
        
        // Create StyledDom
        let styled_dom = StyledDom::new(dom, css);
        
        // Create font manager
        let font_db = InMemoryFontDb::new();
        let mut font_manager = FontCache::new(font_db);
        
        // Create context
        let mut debug_messages = None;
        let mut counters = BTreeMap::new();
        let selections = BTreeMap::new();
        
        let mut ctx = LayoutContext {
            styled_dom: &styled_dom,
            font_manager: &font_manager,
            selections: &selections,
            debug_messages: &mut debug_messages,
            counters: &mut counters,
            viewport_size: azul_core::geom::LogicalSize::new(800.0, 600.0),
        };
        
        // Generate layout tree
        generate_layout_tree(&mut ctx)
    }

    #[test]
    fn test_marker_pseudo_element_created_for_list_items() {
        let html = r#"<ol><li>First</li><li>Second</li></ol>"#;
        let tree = create_test_tree(html, "").expect("Failed to create tree");
        
        // Count ::marker pseudo-elements
        let marker_count = tree.nodes.iter()
            .filter(|node| node.pseudo_element == Some(PseudoElement::Marker))
            .count();
        
        assert_eq!(marker_count, 2, "Should have 2 ::marker pseudo-elements");
    }

    #[test]
    fn test_marker_is_first_child_of_list_item() {
        let html = r#"<ol><li>Item</li></ol>"#;
        let tree = create_test_tree(html, "").expect("Failed to create tree");
        
        // Find a list-item that has children
        let list_item = tree.nodes.iter()
            .find(|node| {
                !node.children.is_empty() 
                    && node.pseudo_element.is_none()
                    && node.dom_node_id.is_some()
            })
            .expect("Should find list item");
        
        // First child should be ::marker
        let first_child = &tree.nodes[list_item.children[0]];
        assert_eq!(
            first_child.pseudo_element,
            Some(PseudoElement::Marker),
            "First child must be ::marker"
        );
    }

    #[test]
    fn test_marker_references_same_dom_node_as_list_item() {
        let html = r#"<ol><li>Item</li></ol>"#;
        let tree = create_test_tree(html, "").expect("Failed to create tree");
        
        // Find list-item
        let (li_idx, list_item) = tree.nodes.iter()
            .enumerate()
            .find(|(_, node)| {
                !node.children.is_empty()
                    && node.pseudo_element.is_none()
                    && node.dom_node_id.is_some()
            })
            .expect("Should find list item");
        
        let li_dom_id = list_item.dom_node_id.expect("List item has DOM ID");
        
        // Get ::marker
        let marker = &tree.nodes[list_item.children[0]];
        assert_eq!(marker.pseudo_element, Some(PseudoElement::Marker));
        
        // Marker should reference same DOM node
        assert_eq!(
            marker.dom_node_id,
            Some(li_dom_id),
            "::marker should reference same DOM node as list-item"
        );
    }

    #[test]
    fn test_multiple_list_items_each_have_marker() {
        let html = r#"<ul><li>A</li><li>B</li><li>C</li></ul>"#;
        let tree = create_test_tree(html, "").expect("Failed to create tree");
        
        // Find all list items (nodes with children, not pseudo-elements)
        let list_items: Vec<_> = tree.nodes.iter()
            .filter(|node| {
                !node.children.is_empty()
                    && node.pseudo_element.is_none()
                    && node.dom_node_id.is_some()
            })
            .collect();
        
        assert!(list_items.len() >= 3, "Should have at least 3 list items");
        
        // Each should have ::marker as first child
        for item in list_items.iter().take(3) {
            let first_child = &tree.nodes[item.children[0]];
            assert_eq!(
                first_child.pseudo_element,
                Some(PseudoElement::Marker),
                "Each list item should have ::marker as first child"
            );
        }
    }

    #[test]
    fn test_nested_lists_have_separate_markers() {
        let html = r#"
            <ol>
                <li>Parent 1
                    <ol>
                        <li>Child 1.1</li>
                        <li>Child 1.2</li>
                    </ol>
                </li>
                <li>Parent 2</li>
            </ol>
        "#;
        let tree = create_test_tree(html, "").expect("Failed to create tree");
        
        let marker_count = tree.nodes.iter()
            .filter(|node| node.pseudo_element == Some(PseudoElement::Marker))
            .count();
        
        // Should have markers for: Parent 1, Parent 2, Child 1.1, Child 1.2
        assert_eq!(marker_count, 4, "Should have 4 ::marker pseudo-elements total");
    }

    #[test]
    fn test_marker_not_created_for_non_list_items() {
        let html = r#"<div>Normal</div><p>Paragraph</p><span>Span</span>"#;
        let tree = create_test_tree(html, "").expect("Failed to create tree");
        
        let marker_count = tree.nodes.iter()
            .filter(|node| node.pseudo_element == Some(PseudoElement::Marker))
            .count();
        
        assert_eq!(marker_count, 0, "Non-list-items should not have ::marker");
    }

    #[test]
    fn test_layout_tree_structure_matches_spec() {
        // Verify structure:
        // ol
        //   └── li
        //       ├── ::marker (first child)
        //       └── content
        
        let html = r#"<ol><li>Item 1</li></ol>"#;
        let tree = create_test_tree(html, "").expect("Failed to create tree");
        
        // Find list item with children
        let (li_idx, li_node) = tree.nodes.iter()
            .enumerate()
            .find(|(_, node)| {
                !node.children.is_empty()
                    && node.pseudo_element.is_none()
                    && node.dom_node_id.is_some()
            })
            .expect("Should find list item");
        
        // First child MUST be ::marker
        let first_child_idx = li_node.children[0];
        let first_child = &tree.nodes[first_child_idx];
        
        assert_eq!(
            first_child.pseudo_element,
            Some(PseudoElement::Marker),
            "First child MUST be ::marker per CSS Lists spec"
        );
        
        // Verify parent relationship
        assert_eq!(
            first_child.parent,
            Some(li_idx),
            "::marker parent should be list-item"
        );
        
        // Verify DOM reference
        assert_eq!(
            first_child.dom_node_id,
            li_node.dom_node_id,
            "::marker should reference same DOM node for style inheritance"
        );
    }

    #[test]
    fn test_marker_is_not_anonymous_box() {
        let html = r#"<ol><li>Item</li></ol>"#;
        let tree = create_test_tree(html, "").expect("Failed to create tree");
        
        // Find ::marker
        let marker = tree.nodes.iter()
            .find(|node| node.pseudo_element == Some(PseudoElement::Marker))
            .expect("Should find marker");
        
        // Pseudo-elements are NOT anonymous boxes
        assert_eq!(
            marker.is_anonymous, false,
            "::marker is a pseudo-element, not an anonymous box"
        );
        assert_eq!(
            marker.anonymous_type, None,
            "::marker should not have anonymous_type"
        );
    }
}
