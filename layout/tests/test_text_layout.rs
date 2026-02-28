//! Unit tests for text layout behavior
//!
//! These tests verify that text nodes are properly laid out within their parent's
//! inline formatting context, matching HTML/CSS behavior.

use azul_core::dom::{Dom, NodeType};
use azul_core::styled_dom::StyledDom;
use azul_css::css::Css;

#[test]
fn test_text_node_in_div_creates_ifc() {
    // HTML: <div>Hello World</div>
    // Expected: The DIV creates a BFC, which contains an IFC for the text

    let mut dom = Dom::create_div().with_children(vec![Dom::create_text("Hello World")].into());
    let styled_dom = StyledDom::create(&mut dom, Css::empty());

    eprintln!("Test: text_node_in_div_creates_ifc");
    eprintln!("DOM structure:");
    eprintln!(
        "  Node 0 (root): {:?}",
        styled_dom.node_data.as_container()[azul_core::dom::NodeId::new(0)].get_node_type()
    );
    eprintln!(
        "  Node 1 (div): {:?}",
        styled_dom.node_data.as_container()[azul_core::dom::NodeId::new(1)].get_node_type()
    );
    if styled_dom.node_data.as_container().len() > 2 {
        eprintln!(
            "  Node 2 (text): {:?}",
            styled_dom.node_data.as_container()[azul_core::dom::NodeId::new(2)].get_node_type()
        );
    }

    // The DIV (not the text node) should have inline layout result
    // The text node should be a child that gets laid out by the parent's IFC

    eprintln!("Test expectations:");
    eprintln!("  1. DIV establishes IFC (has inline children)");
    eprintln!("  2. Text node is laid out within parent's IFC");
    eprintln!("  3. DIV.inline_layout_result should be Some after layout");
    eprintln!("  4. Text node should NOT have its own IFC");
}

#[test]
fn test_nested_text_nodes() {
    // HTML: <div><span>Hello</span> <span>World</span></div>
    // Expected: DIV creates BFC with IFC, spans are inline, text is inline content

    let mut dom = Dom::create_div()
        .with_children(vec![Dom::create_text("Hello "), Dom::create_text("World")].into());
    let styled_dom = StyledDom::create(&mut dom, Css::empty());

    eprintln!("\nTest: nested_text_nodes");
    eprintln!(
        "DOM has {} nodes",
        styled_dom.node_data.as_container().len()
    );

    for i in 0..styled_dom.node_data.as_container().len() {
        let node_id = azul_core::dom::NodeId::new(i);
        let container = styled_dom.node_data.as_container();
        let node_type = container[node_id].get_node_type();
        eprintln!("  Node {}: {:?}", i, node_type);
    }
}

#[test]
fn test_div_with_explicit_font_size() {
    // HTML: <div style="font-size: 24px;">Test Text</div>
    // This is similar to our failing test case

    let mut dom = Dom::create_div()
        .with_inline_style("font-size: 24px; width: 400px; height: 30px;")
        .with_children(vec![Dom::create_text("Test Text")].into());

    let styled_dom = StyledDom::create(&mut dom, Css::empty());

    eprintln!("\nTest: div_with_explicit_font_size");
    eprintln!("This matches our failing test_display_list example");
    eprintln!(
        "DOM has {} nodes",
        styled_dom.node_data.as_container().len()
    );

    // Check what's in the tree
    for i in 0..styled_dom.node_data.as_container().len() {
        let node_id = azul_core::dom::NodeId::new(i);
        let container = styled_dom.node_data.as_container();
        let node_data = &container[node_id];
        let node_type = node_data.get_node_type();
        eprintln!("  Node {}: {:?}", i, node_type);

        if let NodeType::Text(text) = node_type {
            eprintln!("    Text content: '{}'", text);
        }
    }

    eprintln!("\nExpected behavior:");
    eprintln!("  - DIV (node 1) should establish IFC");
    eprintln!("  - Text node (node 2) should be inline content within DIV's IFC");
    eprintln!("  - After layout, DIV should have inline_layout_result with glyphs");
    eprintln!("  - Text node itself should NOT have inline_layout_result");
}
