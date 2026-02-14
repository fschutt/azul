//! DOM Manipulation Tests
//!
//! Tests for DOM tree construction, node management, and related operations.

use azul_core::dom::{Dom, NodeType};
use azul_css::dynamic_selector::CssPropertyWithConditions;
use azul_css::props::{basic::font::StyleFontSize, property::CssProperty};

#[test]
fn test_dom_div_creation() {
    let dom = Dom::create_div();
    assert!(matches!(dom.root.node_type, NodeType::Div));
}

#[test]
fn test_dom_body_creation() {
    let dom = Dom::create_body();
    assert!(matches!(dom.root.node_type, NodeType::Body));
}

#[test]
fn test_dom_text_creation() {
    let dom = Dom::create_text("Hello World");
    assert!(matches!(dom.root.node_type, NodeType::Text(_)));
}

#[test]
fn test_dom_with_child() {
    let dom = Dom::create_div().with_child(Dom::create_text("Child"));
    assert_eq!(dom.children.len(), 1);
}

#[test]
fn test_dom_with_multiple_children() {
    let dom = Dom::create_div()
        .with_child(Dom::create_text("First"))
        .with_child(Dom::create_text("Second"))
        .with_child(Dom::create_text("Third"));
    assert_eq!(dom.children.len(), 3);
}

#[test]
fn test_dom_with_children_vec() {
    let dom = Dom::create_div().with_children(
        vec![
            Dom::create_text("One"),
            Dom::create_text("Two"),
            Dom::create_text("Three"),
        ]
        .into(),
    );
    assert_eq!(dom.children.len(), 3);
}

#[test]
fn test_dom_nested_structure() {
    let dom =
        Dom::create_div().with_child(Dom::create_div().with_child(Dom::create_text("Nested")));
    assert_eq!(dom.children.len(), 1);
}

#[test]
fn test_dom_deeply_nested() {
    let dom = Dom::create_div().with_child(Dom::create_div().with_child(
        Dom::create_div().with_child(Dom::create_div().with_child(Dom::create_text("Deep"))),
    ));
    assert_eq!(dom.children.len(), 1);
}

#[test]
fn test_dom_node_types() {
    let div = Dom::create_div();
    assert!(matches!(div.root.node_type, NodeType::Div));

    let p = Dom::create_node(NodeType::P);
    assert!(matches!(p.root.node_type, NodeType::P));

    let span = Dom::create_node(NodeType::Span);
    assert!(matches!(span.root.node_type, NodeType::Span));

    let h1 = Dom::create_node(NodeType::H1);
    assert!(matches!(h1.root.node_type, NodeType::H1));
}

#[test]
fn test_dom_with_inline_css() {
    let dom = Dom::create_div().with_css_props(
        vec![CssPropertyWithConditions::simple(CssProperty::font_size(
            StyleFontSize::px(16.0),
        ))]
        .into(),
    );
    assert_eq!(dom.root.css_props.len(), 1);
}

#[test]
fn test_dom_with_multiple_inline_css() {
    let dom = Dom::create_div().with_css_props(
        vec![
            CssPropertyWithConditions::simple(CssProperty::font_size(StyleFontSize::px(16.0))),
            CssPropertyWithConditions::simple(CssProperty::font_size(StyleFontSize::px(18.0))),
        ]
        .into(),
    );
    assert_eq!(dom.root.css_props.len(), 2);
}

#[test]
fn test_dom_empty_children() {
    let dom = Dom::create_div();
    assert!(dom.children.is_empty());
}

#[test]
fn test_dom_with_empty_children_vec() {
    let dom = Dom::create_div().with_children(vec![].into());
    assert!(dom.children.is_empty());
}

#[test]
fn test_dom_mixed_node_types() {
    let dom = Dom::create_div()
        .with_child(Dom::create_node(NodeType::P).with_child(Dom::create_text("Paragraph")))
        .with_child(Dom::create_node(NodeType::Span).with_child(Dom::create_text("Span")))
        .with_child(Dom::create_node(NodeType::H1).with_child(Dom::create_text("Heading")));
    assert_eq!(dom.children.len(), 3);
}

#[test]
fn test_dom_text_content() {
    let text = "Test content";
    let dom = Dom::create_text(text);
    if let NodeType::Text(content) = &dom.root.node_type {
        assert_eq!(content.as_str(), text);
    } else {
        panic!("Expected Text node");
    }
}

#[test]
fn test_dom_unicode_text() {
    let text = "日本語テスト 🎉 مرحبا";
    let dom = Dom::create_text(text);
    if let NodeType::Text(content) = &dom.root.node_type {
        assert_eq!(content.as_str(), text);
    } else {
        panic!("Expected Text node");
    }
}

#[test]
fn test_dom_empty_text() {
    let dom = Dom::create_text("");
    if let NodeType::Text(content) = &dom.root.node_type {
        assert!(content.as_str().is_empty());
    } else {
        panic!("Expected Text node");
    }
}

#[test]
fn test_dom_very_long_text() {
    let text = "a".repeat(10000);
    let dom = Dom::create_text(text.clone());
    if let NodeType::Text(content) = &dom.root.node_type {
        assert_eq!(content.as_str().len(), 10000);
    } else {
        panic!("Expected Text node");
    }
}

#[test]
fn test_dom_whitespace_text() {
    let text = "   \n\t\r\n   ";
    let dom = Dom::create_text(text);
    if let NodeType::Text(content) = &dom.root.node_type {
        assert_eq!(content.as_str(), text);
    } else {
        panic!("Expected Text node");
    }
}

#[test]
fn test_dom_table_structure() {
    let dom = Dom::create_node(NodeType::Table)
        .with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(Dom::create_node(NodeType::Td).with_child(Dom::create_text("Cell 1")))
                .with_child(Dom::create_node(NodeType::Td).with_child(Dom::create_text("Cell 2"))),
        )
        .with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(Dom::create_node(NodeType::Td).with_child(Dom::create_text("Cell 3")))
                .with_child(Dom::create_node(NodeType::Td).with_child(Dom::create_text("Cell 4"))),
        );
    assert_eq!(dom.children.len(), 2);
}

#[test]
fn test_dom_list_structure() {
    let dom = Dom::create_node(NodeType::Ul)
        .with_child(Dom::create_node(NodeType::Li).with_child(Dom::create_text("Item 1")))
        .with_child(Dom::create_node(NodeType::Li).with_child(Dom::create_text("Item 2")))
        .with_child(Dom::create_node(NodeType::Li).with_child(Dom::create_text("Item 3")));
    assert_eq!(dom.children.len(), 3);
}

#[test]
fn test_dom_form_structure() {
    let dom = Dom::create_node(NodeType::Form)
        .with_child(Dom::create_node(NodeType::Input))
        .with_child(Dom::create_node(NodeType::Button).with_child(Dom::create_text("Submit")));
    assert_eq!(dom.children.len(), 2);
}

#[test]
fn test_dom_semantic_elements() {
    let dom = Dom::create_node(NodeType::Article)
        .with_child(Dom::create_node(NodeType::Header).with_child(Dom::create_text("Title")))
        .with_child(Dom::create_node(NodeType::Section).with_child(Dom::create_text("Content")))
        .with_child(Dom::create_node(NodeType::Footer).with_child(Dom::create_text("Footer")));
    assert_eq!(dom.children.len(), 3);
}

#[test]
fn test_dom_all_heading_levels() {
    let dom = Dom::create_div()
        .with_child(Dom::create_node(NodeType::H1).with_child(Dom::create_text("H1")))
        .with_child(Dom::create_node(NodeType::H2).with_child(Dom::create_text("H2")))
        .with_child(Dom::create_node(NodeType::H3).with_child(Dom::create_text("H3")))
        .with_child(Dom::create_node(NodeType::H4).with_child(Dom::create_text("H4")))
        .with_child(Dom::create_node(NodeType::H5).with_child(Dom::create_text("H5")))
        .with_child(Dom::create_node(NodeType::H6).with_child(Dom::create_text("H6")));
    assert_eq!(dom.children.len(), 6);
}

#[test]
fn test_dom_inline_elements() {
    let dom = Dom::create_node(NodeType::P)
        .with_child(Dom::create_text("Normal "))
        .with_child(Dom::create_node(NodeType::Strong).with_child(Dom::create_text("bold")))
        .with_child(Dom::create_text(" and "))
        .with_child(Dom::create_node(NodeType::Em).with_child(Dom::create_text("italic")))
        .with_child(Dom::create_text(" text"));
    assert_eq!(dom.children.len(), 5);
}

#[test]
fn test_dom_many_children() {
    let children: Vec<Dom> = (0..100)
        .map(|i| Dom::create_text(format!("Child {}", i)))
        .collect();
    let dom = Dom::create_div().with_children(children.into());
    assert_eq!(dom.children.len(), 100);
}

#[test]
fn test_dom_wide_tree() {
    // Test a wide but shallow tree
    let dom = Dom::create_div()
        .with_child(Dom::create_div())
        .with_child(Dom::create_div())
        .with_child(Dom::create_div())
        .with_child(Dom::create_div())
        .with_child(Dom::create_div())
        .with_child(Dom::create_div())
        .with_child(Dom::create_div())
        .with_child(Dom::create_div())
        .with_child(Dom::create_div())
        .with_child(Dom::create_div());
    assert_eq!(dom.children.len(), 10);
}

#[test]
fn test_dom_deep_tree() {
    // Test a deep but narrow tree
    fn create_deep(depth: usize) -> Dom {
        if depth == 0 {
            Dom::create_text("Leaf")
        } else {
            Dom::create_div().with_child(create_deep(depth - 1))
        }
    }

    let dom = create_deep(20);
    assert_eq!(dom.children.len(), 1);
}

// ============================================================================
// Tests for estimated_total_children bug in add_child
// ============================================================================

#[test]
fn test_add_child_estimated_total_children_with_nested_children() {
    // Create a parent node
    let mut parent = Dom::create_div();

    // Create a child with its own children (grandchildren)
    let mut child = Dom::create_div();
    child.add_child(Dom::create_node(NodeType::Span));
    child.add_child(Dom::create_node(NodeType::Span));

    // At this point, child should have estimated_total_children = 2
    // (one for each of its 2 children)
    assert_eq!(child.estimated_total_children, 2);

    // Now add this child (with its 2 descendants) to parent
    parent.add_child(child);

    // Expected behavior: parent should have estimated_total_children = 3
    // (the child itself + its 2 descendants)
    assert_eq!(
        parent.estimated_total_children,
        3,
        "add_child should increment estimated_total_children by (child.estimated_total_children + 1), not just 1"
    );
}

#[test]
fn test_set_children_estimated_total_children_correct() {
    // Create a parent node
    let mut parent = Dom::create_div();

    // Create a child with its own children (grandchildren)
    let mut child = Dom::create_div();
    child.add_child(Dom::create_node(NodeType::Span));
    child.add_child(Dom::create_node(NodeType::Span));

    // At this point, child should have estimated_total_children = 2
    assert_eq!(child.estimated_total_children, 2);

    // Now use set_children instead
    parent.set_children(vec![child].into());

    // This should correctly be 3 (the child + its 2 descendants)
    assert_eq!(
        parent.estimated_total_children, 3,
        "set_children correctly calculates estimated_total_children"
    );
}

#[test]
fn test_add_child_vs_set_children_consistency() {
    // Create identical structures using add_child and set_children

    // Method 1: Using add_child
    let mut parent1 = Dom::create_div();
    let mut child1 = Dom::create_div();
    child1.add_child(Dom::create_node(NodeType::Span));
    child1.add_child(Dom::create_node(NodeType::Span));
    parent1.add_child(child1);

    // Method 2: Using set_children
    let mut parent2 = Dom::create_div();
    let mut child2 = Dom::create_div();
    child2.add_child(Dom::create_node(NodeType::Span));
    child2.add_child(Dom::create_node(NodeType::Span));
    parent2.set_children(vec![child2].into());

    // These should produce identical DOM structures with identical estimated_total_children
    assert_eq!(
        parent1.estimated_total_children,
        parent2.estimated_total_children,
        "add_child and set_children should produce the same estimated_total_children for identical DOM structures"
    );
}

#[test]
fn test_add_child_node_count_matches_actual() {
    let mut parent = Dom::create_div();
    let mut child = Dom::create_div();
    child.add_child(Dom::create_node(NodeType::Span));
    child.add_child(Dom::create_node(NodeType::Span));
    parent.add_child(child);

    // node_count() = estimated_total_children + 1
    // Parent (1) + child (1) + 2 grandchildren (2) = 4 total nodes
    assert_eq!(
        parent.node_count(),
        4,
        "node_count() should return the actual total node count including all descendants"
    );
}
