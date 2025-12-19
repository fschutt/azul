//! DOM Manipulation Tests
//!
//! Tests for DOM tree construction, node management, and related operations.

use azul_core::dom::{Dom, NodeDataInlineCssProperty, NodeType};
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
    let dom = Dom::text("Hello World");
    assert!(matches!(dom.root.node_type, NodeType::Text(_)));
}

#[test]
fn test_dom_with_child() {
    let dom = Dom::create_div().with_child(Dom::text("Child"));
    assert_eq!(dom.children.len(), 1);
}

#[test]
fn test_dom_with_multiple_children() {
    let dom = Dom::create_div()
        .with_child(Dom::text("First"))
        .with_child(Dom::text("Second"))
        .with_child(Dom::text("Third"));
    assert_eq!(dom.children.len(), 3);
}

#[test]
fn test_dom_with_children_vec() {
    let dom = Dom::create_div()
        .with_children(vec![Dom::text("One"), Dom::text("Two"), Dom::text("Three")].into());
    assert_eq!(dom.children.len(), 3);
}

#[test]
fn test_dom_nested_structure() {
    let dom = Dom::create_div().with_child(Dom::create_div().with_child(Dom::text("Nested")));
    assert_eq!(dom.children.len(), 1);
}

#[test]
fn test_dom_deeply_nested() {
    let dom = Dom::create_div().with_child(
        Dom::create_div().with_child(Dom::create_div().with_child(Dom::create_div().with_child(Dom::text("Deep")))),
    );
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
    let dom = Dom::create_div().with_inline_css_props(
        vec![NodeDataInlineCssProperty::Normal(CssProperty::font_size(
            StyleFontSize::px(16.0),
        ))]
        .into(),
    );
    assert_eq!(dom.root.inline_css_props.len(), 1);
}

#[test]
fn test_dom_with_multiple_inline_css() {
    let dom = Dom::create_div().with_inline_css_props(
        vec![
            NodeDataInlineCssProperty::Normal(CssProperty::font_size(StyleFontSize::px(16.0))),
            NodeDataInlineCssProperty::Normal(CssProperty::font_size(StyleFontSize::px(18.0))),
        ]
        .into(),
    );
    assert_eq!(dom.root.inline_css_props.len(), 2);
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
        .with_child(Dom::create_node(NodeType::P).with_child(Dom::text("Paragraph")))
        .with_child(Dom::create_node(NodeType::Span).with_child(Dom::text("Span")))
        .with_child(Dom::create_node(NodeType::H1).with_child(Dom::text("Heading")));
    assert_eq!(dom.children.len(), 3);
}

#[test]
fn test_dom_text_content() {
    let text = "Test content";
    let dom = Dom::text(text);
    if let NodeType::Text(content) = &dom.root.node_type {
        assert_eq!(content.as_str(), text);
    } else {
        panic!("Expected Text node");
    }
}

#[test]
fn test_dom_unicode_text() {
    let text = "日本語テスト 🎉 مرحبا";
    let dom = Dom::text(text);
    if let NodeType::Text(content) = &dom.root.node_type {
        assert_eq!(content.as_str(), text);
    } else {
        panic!("Expected Text node");
    }
}

#[test]
fn test_dom_empty_text() {
    let dom = Dom::text("");
    if let NodeType::Text(content) = &dom.root.node_type {
        assert!(content.as_str().is_empty());
    } else {
        panic!("Expected Text node");
    }
}

#[test]
fn test_dom_very_long_text() {
    let text = "a".repeat(10000);
    let dom = Dom::text(text.clone());
    if let NodeType::Text(content) = &dom.root.node_type {
        assert_eq!(content.as_str().len(), 10000);
    } else {
        panic!("Expected Text node");
    }
}

#[test]
fn test_dom_whitespace_text() {
    let text = "   \n\t\r\n   ";
    let dom = Dom::text(text);
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
                .with_child(Dom::create_node(NodeType::Td).with_child(Dom::text("Cell 1")))
                .with_child(Dom::create_node(NodeType::Td).with_child(Dom::text("Cell 2"))),
        )
        .with_child(
            Dom::create_node(NodeType::Tr)
                .with_child(Dom::create_node(NodeType::Td).with_child(Dom::text("Cell 3")))
                .with_child(Dom::create_node(NodeType::Td).with_child(Dom::text("Cell 4"))),
        );
    assert_eq!(dom.children.len(), 2);
}

#[test]
fn test_dom_list_structure() {
    let dom = Dom::create_node(NodeType::Ul)
        .with_child(Dom::create_node(NodeType::Li).with_child(Dom::text("Item 1")))
        .with_child(Dom::create_node(NodeType::Li).with_child(Dom::text("Item 2")))
        .with_child(Dom::create_node(NodeType::Li).with_child(Dom::text("Item 3")));
    assert_eq!(dom.children.len(), 3);
}

#[test]
fn test_dom_form_structure() {
    let dom = Dom::create_node(NodeType::Form)
        .with_child(Dom::create_node(NodeType::Input))
        .with_child(Dom::create_node(NodeType::Button).with_child(Dom::text("Submit")));
    assert_eq!(dom.children.len(), 2);
}

#[test]
fn test_dom_semantic_elements() {
    let dom = Dom::create_node(NodeType::Article)
        .with_child(Dom::create_node(NodeType::Header).with_child(Dom::text("Title")))
        .with_child(Dom::create_node(NodeType::Section).with_child(Dom::text("Content")))
        .with_child(Dom::create_node(NodeType::Footer).with_child(Dom::text("Footer")));
    assert_eq!(dom.children.len(), 3);
}

#[test]
fn test_dom_all_heading_levels() {
    let dom = Dom::create_div()
        .with_child(Dom::create_node(NodeType::H1).with_child(Dom::text("H1")))
        .with_child(Dom::create_node(NodeType::H2).with_child(Dom::text("H2")))
        .with_child(Dom::create_node(NodeType::H3).with_child(Dom::text("H3")))
        .with_child(Dom::create_node(NodeType::H4).with_child(Dom::text("H4")))
        .with_child(Dom::create_node(NodeType::H5).with_child(Dom::text("H5")))
        .with_child(Dom::create_node(NodeType::H6).with_child(Dom::text("H6")));
    assert_eq!(dom.children.len(), 6);
}

#[test]
fn test_dom_inline_elements() {
    let dom = Dom::create_node(NodeType::P)
        .with_child(Dom::text("Normal "))
        .with_child(Dom::create_node(NodeType::Strong).with_child(Dom::text("bold")))
        .with_child(Dom::text(" and "))
        .with_child(Dom::create_node(NodeType::Em).with_child(Dom::text("italic")))
        .with_child(Dom::text(" text"));
    assert_eq!(dom.children.len(), 5);
}

#[test]
fn test_dom_many_children() {
    let children: Vec<Dom> = (0..100)
        .map(|i| Dom::text(format!("Child {}", i)))
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
            Dom::text("Leaf")
        } else {
            Dom::create_div().with_child(create_deep(depth - 1))
        }
    }

    let dom = create_deep(20);
    assert_eq!(dom.children.len(), 1);
}
