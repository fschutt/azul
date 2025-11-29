extern crate azul_layout;

use azul_layout::xml::{parse_xml_string, XmlNodeChild, XmlParseError};

#[test]
fn test_no_text_duplication_in_paragraph() {
    let html = r#"<p>Text before <span>inline</span> text after.</p>"#;

    let result = parse_xml_string(html);
    assert!(result.is_ok(), "Failed to parse XML: {:?}", result.err());

    let nodes = result.unwrap();
    assert_eq!(nodes.len(), 1, "Expected 1 root node (p)");

    if let XmlNodeChild::Element(p_node) = &nodes[0] {
        assert_eq!(p_node.node_type.as_str(), "p", "Root node should be <p>");
        assert_eq!(
            p_node.children.as_ref().len(),
            3,
            "Expected 3 children in <p>"
        );

        // First child: "Text before "
        if let XmlNodeChild::Text(text1) = &p_node.children.as_ref()[0] {
            assert_eq!(
                text1.as_str(),
                "Text before ",
                "First child should be 'Text before '"
            );
        } else {
            panic!(
                "First child should be Text, got: {:?}",
                p_node.children.as_ref()[0]
            );
        }

        // Second child: <span>inline</span>
        if let XmlNodeChild::Element(span_node) = &p_node.children.as_ref()[1] {
            assert_eq!(
                span_node.node_type.as_str(),
                "span",
                "Second child should be <span>"
            );
            assert_eq!(
                span_node.children.as_ref().len(),
                1,
                "Span should have 1 text child"
            );

            if let XmlNodeChild::Text(span_text) = &span_node.children.as_ref()[0] {
                assert_eq!(span_text.as_str(), "inline", "Span text should be 'inline'");
            } else {
                panic!("Span child should be Text");
            }
        } else {
            panic!(
                "Second child should be Element(span), got: {:?}",
                p_node.children.as_ref()[1]
            );
        }

        // Third child: " text after."
        if let XmlNodeChild::Text(text2) = &p_node.children.as_ref()[2] {
            assert_eq!(
                text2.as_str(),
                " text after.",
                "Third child should be ' text after.'"
            );
        } else {
            panic!(
                "Third child should be Text, got: {:?}",
                p_node.children.as_ref()[2]
            );
        }
    } else {
        panic!("Root should be Element(p), got: {:?}", nodes[0]);
    }
}

#[test]
fn test_no_text_duplication_complex() {
    let html = r#"<p>This is <span class="highlight">important</span> text.</p>"#;

    let result = parse_xml_string(html);
    assert!(result.is_ok(), "Failed to parse XML: {:?}", result.err());

    let nodes = result.unwrap();
    assert_eq!(nodes.len(), 1, "Expected 1 root node");

    if let XmlNodeChild::Element(p_node) = &nodes[0] {
        // Collect all text content to verify no duplication
        let mut all_text = Vec::new();
        for child in p_node.children.as_ref() {
            match child {
                XmlNodeChild::Text(text) => all_text.push(text.as_str()),
                XmlNodeChild::Element(elem) => {
                    for elem_child in elem.children.as_ref() {
                        if let XmlNodeChild::Text(text) = elem_child {
                            all_text.push(text.as_str());
                        }
                    }
                }
            }
        }

        let combined = all_text.join("");
        assert_eq!(
            combined, "This is important text.",
            "Text should not be duplicated"
        );

        // Verify each text appears only once
        assert_eq!(
            all_text.iter().filter(|&&t| t == "This is ").count(),
            1,
            "'This is ' should appear exactly once"
        );
        assert_eq!(
            all_text.iter().filter(|&&t| t == "important").count(),
            1,
            "'important' should appear exactly once"
        );
        assert_eq!(
            all_text.iter().filter(|&&t| t == " text.").count(),
            1,
            "' text.' should appear exactly once"
        );
    }
}
