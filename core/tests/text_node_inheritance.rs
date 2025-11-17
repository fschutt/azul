//! Test to verify that text nodes inherit CSS properties from their parent elements
//!
//! This is critical for proper text rendering - text nodes themselves have no CSS properties,
//! but must inherit font-weight, font-size, color, etc. from their parent.

use azul_core::{
    dom::{Dom, NodeType, NodeId},
    styled_dom::StyledDom,
};
use azul_css::{
    parser2::CssApiWrapper,
    props::{
        basic::font::StyleFontWeight,
        property::{CssProperty, CssPropertyType},
    },
};

#[test]
fn test_text_node_inherits_font_weight_from_h1() {
    println!("\n=== Test: Text Node Inherits font-weight from H1 ===");
    
    // Create: <body><h1>Bold Text</h1></body>
    // H1 has UA CSS font-weight: bold
    // Text node should inherit this
    let mut dom = Dom::body()
        .with_child(
            Dom::new(NodeType::H1)
                .with_child(Dom::text("Bold Text"))
        );
    
    let styled_dom = StyledDom::new(&mut dom, CssApiWrapper::empty());
    let mut cache = styled_dom.css_property_cache.ptr.clone();
    
    let node_hierarchy = &styled_dom.node_hierarchy.as_container().internal[..];
    let node_data = &styled_dom.node_data.as_container().internal[..];
    
    // Compute inherited values
    cache.compute_inherited_values(node_hierarchy, node_data);
    
    let body_id = NodeId::new(0);
    let h1_id = NodeId::new(1);
    let text_id = NodeId::new(2);
    
    println!("\nNode structure:");
    println!("  Node 0: Body");
    println!("  Node 1: H1");
    println!("  Node 2: Text('Bold Text')");
    
    // Check H1 has font-weight from UA CSS
    println!("\nChecking H1 (parent element):");
    if let Some(h1_computed) = cache.computed_values.get(&h1_id) {
        if let Some(CssProperty::FontWeight(weight_value)) = h1_computed.get(&CssPropertyType::FontWeight) {
            if let Some(weight) = weight_value.get_property() {
                println!("  H1 font-weight: {:?}", weight);
                assert_eq!(*weight, StyleFontWeight::Bold, "H1 should have font-weight: bold from UA CSS");
            }
        }
    } else {
        println!("  H1 has no computed values (UA CSS applied during layout)");
    }
    
    // CRITICAL: Check if text node inherited the font-weight
    println!("\nChecking Text Node (child of H1):");
    if let Some(text_computed) = cache.computed_values.get(&text_id) {
        if let Some(CssProperty::FontWeight(weight_value)) = text_computed.get(&CssPropertyType::FontWeight) {
            if let Some(weight) = weight_value.get_property() {
                println!("  ✓ Text node has font-weight: {:?}", weight);
                assert_eq!(*weight, StyleFontWeight::Bold, "Text node should inherit font-weight: bold from H1");
                println!("  ✓ SUCCESS: Text node correctly inherits from parent!");
            } else {
                println!("  ✗ Text node has font-weight property but value is None/Auto/etc");
            }
        } else {
            println!("  ✗ PROBLEM: Text node has NO font-weight property!");
            println!("  → Text nodes are not inheriting properties from parent");
            println!("  → This causes text in H1 to NOT be bold in rendered output");
            panic!("Text node must inherit font-weight from parent H1");
        }
    } else {
        println!("  ✗ PROBLEM: Text node has NO computed values at all!");
        println!("  → Inheritance is not reaching text nodes");
        println!("  → compute_inherited_values() may skip text nodes");
        panic!("Text node must have computed values with inherited properties");
    }
}

#[test]
fn test_text_node_inherits_from_parent_with_explicit_style() {
    println!("\n=== Test: Text Node Inherits Explicit Parent Style ===");
    
    // Create: <div style="font-weight: 900">Heavy Text</div>
    let mut dom = Dom::body()
        .with_child(
            Dom::div()
                .with_inline_style("font-weight: 900;")
                .with_child(Dom::text("Heavy Text"))
        );
    
    let styled_dom = StyledDom::new(&mut dom, CssApiWrapper::empty());
    let mut cache = styled_dom.css_property_cache.ptr.clone();
    
    let node_hierarchy = &styled_dom.node_hierarchy.as_container().internal[..];
    let node_data = &styled_dom.node_data.as_container().internal[..];
    
    cache.compute_inherited_values(node_hierarchy, node_data);
    
    let div_id = NodeId::new(1);
    let text_id = NodeId::new(2);
    
    println!("\nDIV should have font-weight: 900 (explicit)");
    if let Some(div_computed) = cache.computed_values.get(&div_id) {
        if let Some(CssProperty::FontWeight(weight_value)) = div_computed.get(&CssPropertyType::FontWeight) {
            if let Some(weight) = weight_value.get_property() {
                println!("  ✓ DIV font-weight: {:?}", weight);
            }
        }
    }
    
    println!("\nText node should inherit font-weight: 900 from DIV");
    if let Some(text_computed) = cache.computed_values.get(&text_id) {
        if let Some(CssProperty::FontWeight(weight_value)) = text_computed.get(&CssPropertyType::FontWeight) {
            if let Some(weight) = weight_value.get_property() {
                println!("  ✓ Text node font-weight: {:?}", weight);
                assert_eq!(*weight, StyleFontWeight::W900, "Text should inherit 900 weight");
            } else {
                panic!("Text node has font-weight but no value");
            }
        } else {
            panic!("Text node missing font-weight property (not inherited)");
        }
    } else {
        panic!("Text node has no computed values");
    }
}

#[test]
fn test_nested_text_node_inheritance() {
    println!("\n=== Test: Deeply Nested Text Node Inheritance ===");
    
    // Create: <body><div><span><strong>Very Bold</strong></span></div></body>
    // Each level might have different inherited properties
    let mut dom = Dom::body()
        .with_inline_style("font-size: 16px;")
        .with_child(
            Dom::div()
                .with_child(
                    Dom::new(NodeType::Span)
                        .with_child(
                            Dom::new(NodeType::Strong)
                                .with_inline_style("font-weight: bold;")
                                .with_child(Dom::text("Very Bold"))
                        )
                )
        );
    
    let styled_dom = StyledDom::new(&mut dom, CssApiWrapper::empty());
    let mut cache = styled_dom.css_property_cache.ptr.clone();
    
    let node_hierarchy = &styled_dom.node_hierarchy.as_container().internal[..];
    let node_data = &styled_dom.node_data.as_container().internal[..];
    
    cache.compute_inherited_values(node_hierarchy, node_data);
    
    // Text node is deeply nested: body → div → span → strong → text
    let text_id = NodeId::new(4);
    
    println!("\nText node is at depth 4, should inherit:");
    println!("  - font-size: 16px (from body)");
    println!("  - font-weight: bold (from strong)");
    
    if let Some(text_computed) = cache.computed_values.get(&text_id) {
        let has_font_size = text_computed.contains_key(&CssPropertyType::FontSize);
        let has_font_weight = text_computed.contains_key(&CssPropertyType::FontWeight);
        
        println!("\nText node computed values:");
        println!("  Has FontSize: {}", has_font_size);
        println!("  Has FontWeight: {}", has_font_weight);
        
        if has_font_size && has_font_weight {
            println!("  ✓ Text node has inherited properties");
        } else {
            println!("  ✗ Text node missing inherited properties");
            panic!("Deeply nested text node must inherit all properties");
        }
    } else {
        panic!("Deeply nested text node has no computed values");
    }
}

#[test]
fn test_multiple_text_nodes_same_parent() {
    println!("\n=== Test: Multiple Text Nodes in Same Parent ===");
    
    // Create: <h1>First Bold</h1><h1>Second Bold</h1>
    let mut dom = Dom::body()
        .with_child(
            Dom::new(NodeType::H1)
                .with_child(Dom::text("First Bold"))
        )
        .with_child(
            Dom::new(NodeType::H1)
                .with_child(Dom::text("Second Bold"))
        );
    
    let styled_dom = StyledDom::new(&mut dom, CssApiWrapper::empty());
    let mut cache = styled_dom.css_property_cache.ptr.clone();
    
    let node_hierarchy = &styled_dom.node_hierarchy.as_container().internal[..];
    let node_data = &styled_dom.node_data.as_container().internal[..];
    
    cache.compute_inherited_values(node_hierarchy, node_data);
    
    let text1_id = NodeId::new(2);
    let text2_id = NodeId::new(4);
    
    println!("\nBoth text nodes should inherit font-weight: bold");
    
    let text1_has_bold = cache.computed_values.get(&text1_id)
        .and_then(|cv| cv.get(&CssPropertyType::FontWeight))
        .is_some();
    
    let text2_has_bold = cache.computed_values.get(&text2_id)
        .and_then(|cv| cv.get(&CssPropertyType::FontWeight))
        .is_some();
    
    println!("  Text node 1 has FontWeight: {}", text1_has_bold);
    println!("  Text node 2 has FontWeight: {}", text2_has_bold);
    
    assert!(text1_has_bold, "First text node should inherit font-weight");
    assert!(text2_has_bold, "Second text node should inherit font-weight");
    
    println!("  ✓ Both text nodes inherit correctly");
}
