/// Test to verify that inline span elements preserve text node structure
/// 
/// Issue: When parsing HTML like `<p>Before <span>inline</span> after</p>`,
/// the text nodes "Before ", "inline", and " after" should be preserved as
/// separate nodes in the DOM hierarchy.
/// 
/// Expected structure:
/// ```
/// <p>
///   ├─ TextNode: "Before "
///   ├─ <span>
///   │   └─ TextNode: "inline"
///   └─ TextNode: " after"
/// ```
///
/// Current bug: All text outside the span gets merged into a single node,
/// losing the whitespace and structure.

use azul_core::dom::{Dom, NodeType};

#[test]
fn test_manual_dom_structure() {
    // First, verify that manually creating the DOM structure works correctly
    
    let dom = Dom::p().with_children(vec![
        Dom::create_text("Before "),
        Dom::create_node(NodeType::Span).with_children(vec![
            Dom::create_text("inline")
        ].into()),
        Dom::create_text(" after")
    ].into());
    
    // Verify structure
    assert_eq!(dom.children.as_ref().len(), 3, "Paragraph should have 3 children");
    
    // Check first child (text before span)
    match &dom.children.as_ref()[0].root.node_type {
        NodeType::Text(text) => {
            assert_eq!(text.as_str(), "Before ", "First text node should be 'Before '");
        }
        other => panic!("Expected Text node, got {:?}", other),
    }
    
    // Check second child (span)
    match &dom.children.as_ref()[1].root.node_type {
        NodeType::Span => {
            assert_eq!(
                dom.children.as_ref()[1].children.as_ref().len(),
                1,
                "Span should have 1 child"
            );
            
            // Check span's text child
            match &dom.children.as_ref()[1].children.as_ref()[0].root.node_type {
                NodeType::Text(text) => {
                    assert_eq!(text.as_str(), "inline", "Span's text should be 'inline'");
                }
                other => panic!("Expected Text node in span, got {:?}", other),
            }
        }
        other => panic!("Expected Span node, got {:?}", other),
    }
    
    // Check third child (text after span)
    match &dom.children.as_ref()[2].root.node_type {
        NodeType::Text(text) => {
            assert_eq!(text.as_str(), " after", "Third text node should be ' after'");
        }
        other => panic!("Expected Text node, got {:?}", other),
    }
    
    println!("✓ Manual DOM structure test passed");
    println!("  DOM has {} children", dom.children.as_ref().len());
    println!("  Structure: TextNode -> Span -> TextNode");
}

#[cfg(feature = "xml")]
#[test]
fn test_parsed_inline_span_structure() {
    use azul_layout::xml::{domxml_from_str, ComponentMap};
    
    let html = r#"
<!DOCTYPE html>
<html>
<body>
    <p>Before <span class="highlight">inline</span> after</p>
</body>
</html>
"#;
    
    let component_map = ComponentMap::with_builtin();
    let dom_xml = domxml_from_str(html, &component_map);
    let styled_dom = dom_xml.parsed_dom;
    
    // Debug: Print the DOM structure
    println!("\n=== Parsed DOM Structure ===");
    println!("Root node count: {}", styled_dom.node_data.as_ref().len());
    
    for i in 0..styled_dom.node_data.as_ref().len() {
        let node_id = azul_core::dom::NodeId::new(i);
        let node_type = &styled_dom.node_data.as_ref()[node_id].node_type;
        let hier = &styled_dom.node_hierarchy.as_ref()[node_id];
        
        println!("  Node {}: {:?}", i, node_type);
        if let Some(parent) = hier.parent_id() {
            println!("    Parent: {:?}", parent);
        }
        
        // Print text content if it's a text node
        if let NodeType::Text(ref text) = node_type {
            println!("    Text: '{}'", text.as_str());
        }
    }
    
    // Find the paragraph node
    let p_node_id = styled_dom.node_data.as_ref()
        .iter()
        .position(|n| matches!(n.node_type, NodeType::P))
        .expect("Should have a <p> node");
    
    println!("\n=== Paragraph Node Analysis ===");
    println!("Paragraph is at NodeId({})", p_node_id);
    
    // Get children of paragraph
    let p_node = azul_core::dom::NodeId::new(p_node_id);
    let mut p_children = Vec::new();
    for child_id in p_node.az_children(&styled_dom.node_hierarchy.as_ref()) {
        p_children.push(child_id);
    }
    
    println!("Paragraph has {} children", p_children.len());
    for (i, child_id) in p_children.iter().enumerate() {
        let node_type = &styled_dom.node_data.as_ref()[*child_id].node_type;
        println!("  Child {}: {:?}", i, node_type);
        if let NodeType::Text(ref text) = node_type {
            println!("    Text: '{}'", text.as_str());
        }
    }
    
    // ASSERTION: Paragraph should have 3 children (or at least the span should exist)
    // This test will FAIL if the bug exists (text nodes merged)
    // It will PASS if text nodes are preserved correctly
    
    // Find span child
    let span_exists = p_children.iter().any(|&child_id| {
        matches!(styled_dom.node_data.as_ref()[child_id].node_type, NodeType::Span)
    });
    
    assert!(span_exists, "Paragraph should contain a <span> element");
    
    // Count text nodes
    let text_node_count = p_children.iter().filter(|&&child_id| {
        matches!(styled_dom.node_data.as_ref()[child_id].node_type, NodeType::Text(_))
    }).count();
    
    println!("\nText nodes in paragraph: {}", text_node_count);
    println!("Expected: 2 (one before span, one after span)");
    
    // This assertion will show us the actual bug
    assert_eq!(
        text_node_count, 2,
        "Expected 2 text nodes (before and after span), but found {}. \
         This indicates the parser is merging text nodes incorrectly.",
        text_node_count
    );
}
