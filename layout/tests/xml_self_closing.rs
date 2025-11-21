use azul_layout::xml::{parse_xml_string, XmlNodeChild};

// Helper function to extract element from XmlNodeChild
fn as_element(child: &XmlNodeChild) -> &azul_core::xml::XmlNode {
    match child {
        XmlNodeChild::Element(node) => node,
        _ => panic!("Expected element node"),
    }
}

#[test]
fn test_self_closing_tags() {
    // Test that self-closing tags like <header/> are parsed correctly
    let xml = r#"
        <html>
            <body>
                <header/>
                <div>Content</div>
                <footer/>
            </body>
        </html>
    "#;
    
    let result = parse_xml_string(xml).unwrap();
    assert_eq!(result.len(), 1);
    
    let html = as_element(&result[0]);
    assert_eq!(html.node_type.as_str(), "html");
    assert_eq!(html.children.as_ref().len(), 1);
    
    let body = as_element(&html.children.as_ref()[0]);
    assert_eq!(body.node_type.as_str(), "body");
    assert_eq!(body.children.as_ref().len(), 3);
    
    // Check that all three children were parsed
    assert_eq!(as_element(&body.children.as_ref()[0]).node_type.as_str(), "header");
    assert_eq!(as_element(&body.children.as_ref()[1]).node_type.as_str(), "div");
    assert_eq!(as_element(&body.children.as_ref()[2]).node_type.as_str(), "footer");
}

#[test]
fn test_self_closing_with_attributes() {
    // Test that self-closing tags with attributes work
    let xml = r#"
        <html>
            <body>
                <header exclude-pages="1"/>
                <div class="content">Text</div>
            </body>
        </html>
    "#;
    
    let result = parse_xml_string(xml).unwrap();
    let html = as_element(&result[0]);
    let body = as_element(&html.children.as_ref()[0]);
    let header = as_element(&body.children.as_ref()[0]);
    
    assert_eq!(header.node_type.as_str(), "header");
    assert_eq!(header.attributes.as_ref().len(), 1);
    assert_eq!(header.attributes.as_ref()[0].key.as_str(), "exclude-pages");
    assert_eq!(header.attributes.as_ref()[0].value.as_str(), "1");
    assert_eq!(header.children.as_ref().len(), 0);
}

#[test]
fn test_mixed_self_closing_and_regular() {
    // Test mixing self-closing and regular tags
    let xml = r#"
        <html>
            <head>
                <style>/* CSS */</style>
            </head>
            <body>
                <header/>
                <div>
                    <p>Text</p>
                    <br/>
                    <span>More text</span>
                </div>
                <footer/>
            </body>
        </html>
    "#;
    
    let result = parse_xml_string(xml).unwrap();
    let html = as_element(&result[0]);
    
    // Should have head and body
    assert_eq!(html.children.as_ref().len(), 2);
    let head = as_element(&html.children.as_ref()[0]);
    let body = as_element(&html.children.as_ref()[1]);
    
    assert_eq!(head.node_type.as_str(), "head");
    assert_eq!(body.node_type.as_str(), "body");
    
    // Body should have header, div, footer
    assert_eq!(body.children.as_ref().len(), 3);
    assert_eq!(as_element(&body.children.as_ref()[0]).node_type.as_str(), "header");
    assert_eq!(as_element(&body.children.as_ref()[1]).node_type.as_str(), "div");
    assert_eq!(as_element(&body.children.as_ref()[2]).node_type.as_str(), "footer");
}

#[test]
fn test_html5_auto_close_list_items() {
    // Test HTML5 auto-closing behavior for <li> elements
    let xml = r#"
        <html>
            <body>
                <ul>
                    <li>Item 1
                    <li>Item 2
                    <li>Item 3
                </ul>
            </body>
        </html>
    "#;
    
    let result = parse_xml_string(xml).unwrap();
    let html = as_element(&result[0]);
    let body = as_element(&html.children.as_ref()[0]);
    let ul = as_element(&body.children.as_ref()[0]);
    
    // Should have 3 list items, each auto-closed
    assert_eq!(ul.children.as_ref().len(), 3);
    assert_eq!(as_element(&ul.children.as_ref()[0]).node_type.as_str(), "li");
    assert_eq!(as_element(&ul.children.as_ref()[1]).node_type.as_str(), "li");
    assert_eq!(as_element(&ul.children.as_ref()[2]).node_type.as_str(), "li");
    
    // First item should have text child
    let first_li = as_element(&ul.children.as_ref()[0]);
    assert!(first_li.children.as_ref().len() > 0);
}

#[test]
fn test_html5_paragraph_auto_close() {
    // Test that <p> auto-closes when encountering block elements
    let xml = r#"
        <html>
            <body>
                <p>First paragraph
                <div>This div auto-closes the paragraph</div>
                <p>Second paragraph
                <h1>This heading auto-closes the paragraph</h1>
            </body>
        </html>
    "#;
    
    let result = parse_xml_string(xml).unwrap();
    let html = as_element(&result[0]);
    let body = as_element(&html.children.as_ref()[0]);
    
    // Should have 4 children: p, div, p, h1
    assert_eq!(body.children.as_ref().len(), 4);
    assert_eq!(as_element(&body.children.as_ref()[0]).node_type.as_str(), "p");
    assert_eq!(as_element(&body.children.as_ref()[1]).node_type.as_str(), "div");
    assert_eq!(as_element(&body.children.as_ref()[2]).node_type.as_str(), "p");
    assert_eq!(as_element(&body.children.as_ref()[3]).node_type.as_str(), "h1");
}

#[test]
fn test_html5_optional_closing_tags() {
    // Test lenient parsing - missing closing tags should be tolerated
    let xml = r#"
        <html>
            <body>
                <div>
                    <p>Paragraph 1
                    <p>Paragraph 2
                </div>
            </body>
        </html>
    "#;
    
    let result = parse_xml_string(xml).unwrap();
    let html = as_element(&result[0]);
    let body = as_element(&html.children.as_ref()[0]);
    let div = as_element(&body.children.as_ref()[0]);
    
    // Should have 2 paragraphs
    assert_eq!(div.children.as_ref().len(), 2);
    assert_eq!(as_element(&div.children.as_ref()[0]).node_type.as_str(), "p");
    assert_eq!(as_element(&div.children.as_ref()[1]).node_type.as_str(), "p");
}

#[test]
fn test_html5_table_auto_close() {
    // Test table element auto-closing
    // Note: Due to how auto-closing works, the second <tr> closes the first one
    // when it's encountered, creating a nested structure
    let xml = r#"
        <html>
            <body>
                <table>
                    <tr>
                        <td>Cell 1
                        <td>Cell 2
                    </tr>
                    <tr>
                        <td>Cell 3
                        <td>Cell 4
                    </tr>
                </table>
            </body>
        </html>
    "#;
    
    let result = parse_xml_string(xml).unwrap();
    let html = as_element(&result[0]);
    let body = as_element(&html.children.as_ref()[0]);
    let table = as_element(&body.children.as_ref()[0]);
    
    // Should have 2 rows
    assert_eq!(table.children.as_ref().len(), 2, "Table should have 2 rows");
    let row1 = as_element(&table.children.as_ref()[0]);
    let row2 = as_element(&table.children.as_ref()[1]);
    
    // Each row should have 2 cells
    assert_eq!(row1.children.as_ref().len(), 2, "First row should have 2 cells");
    assert_eq!(row2.children.as_ref().len(), 2, "Second row should have 2 cells");
}

#[test]
fn test_html5_void_elements_with_wrong_closing() {
    // Test that void elements tolerate incorrect closing tags
    let xml = r#"
        <html>
            <body>
                <img src="test.png"></img>
                <br></br>
                <hr></hr>
                <input type="text"></input>
            </body>
        </html>
    "#;
    
    let result = parse_xml_string(xml).unwrap();
    let html = as_element(&result[0]);
    let body = as_element(&html.children.as_ref()[0]);
    
    // Should have 4 void elements
    assert_eq!(body.children.as_ref().len(), 4);
    assert_eq!(as_element(&body.children.as_ref()[0]).node_type.as_str(), "img");
    assert_eq!(as_element(&body.children.as_ref()[1]).node_type.as_str(), "br");
    assert_eq!(as_element(&body.children.as_ref()[2]).node_type.as_str(), "hr");
    assert_eq!(as_element(&body.children.as_ref()[3]).node_type.as_str(), "input");
}

#[test]
fn test_header_without_closing_tag_lenient() {
    // HTML5-lite: Missing closing tags are now tolerated (lenient parsing)
    let xml = r#"
        <html>
            <body>
                <header>
                    <div>Page number</div>
                    <hr/>
                <footer>Bottom</footer>
            </body>
        </html>
    "#;
    
    // This should now succeed with lenient parsing
    let result = parse_xml_string(xml);
    assert!(result.is_ok(), "Should succeed with lenient HTML5 parsing");
    
    let nodes = result.unwrap();
    let html = as_element(&nodes[0]);
    let body = as_element(&html.children.as_ref()[0]);
    
    // With lenient parsing, missing </header> means header stays open
    // until </body> or an explicit closing tag
    // The footer should still be parsed
    assert!(body.children.as_ref().len() >= 1, "Should have at least one child");
}

#[test]
fn test_auto_close_void_tags() {
    // Test that we auto-close known void/empty HTML elements
    // These are tags that should be treated as self-closing even without />
    let xml = r#"
        <html>
            <body>
                <img src="test.png">
                <br>
                <hr>
                <input type="text">
                <div>Content</div>
            </body>
        </html>
    "#;
    
    let result = parse_xml_string(xml).unwrap();
    let html = as_element(&result[0]);
    let body = as_element(&html.children.as_ref()[0]);
    
    // Should have 5 children: img, br, hr, input, div
    assert_eq!(body.children.as_ref().len(), 5);
    assert_eq!(as_element(&body.children.as_ref()[0]).node_type.as_str(), "img");
    assert_eq!(as_element(&body.children.as_ref()[1]).node_type.as_str(), "br");
    assert_eq!(as_element(&body.children.as_ref()[2]).node_type.as_str(), "hr");
    assert_eq!(as_element(&body.children.as_ref()[3]).node_type.as_str(), "input");
    assert_eq!(as_element(&body.children.as_ref()[4]).node_type.as_str(), "div");
}

#[test]
fn test_inline_span_text_node_structure() {
    // This test verifies that inline spans preserve separate text nodes
    // Issue: Text nodes before/after inline elements were being merged
    
    let xml = r#"
        <html>
            <body>
                <p>Text before <span class="highlight">inline text</span> text after.</p>
            </body>
        </html>
    "#;
    
    let result = parse_xml_string(xml).expect("Should parse XML successfully");
    assert_eq!(result.len(), 1, "Should have one root node");
    
    let html = match &result[0] {
        XmlNodeChild::Element(node) => node,
        _ => panic!("Expected element node"),
    };
    assert_eq!(html.node_type.as_str(), "html");
    
    let body = match &html.children.as_ref()[0] {
        XmlNodeChild::Element(node) => node,
        _ => panic!("Expected element node"),
    };
    assert_eq!(body.node_type.as_str(), "body");
    assert_eq!(body.children.as_ref().len(), 1, "Body should have 1 child (p)");
    
    let p = match &body.children.as_ref()[0] {
        XmlNodeChild::Element(node) => node,
        _ => panic!("Expected element node"),
    };
    assert_eq!(p.node_type.as_str(), "p");
    
    println!("\n=== Paragraph Children ===");
    println!("Paragraph has {} children", p.children.as_ref().len());
    for (i, child) in p.children.as_ref().iter().enumerate() {
        match child {
            XmlNodeChild::Element(node) => {
                println!("  Child {}: Element node_type='{}'", i, node.node_type.as_str());
            }
            XmlNodeChild::Text(text) => {
                println!("  Child {}: Text node text='{}'", i, text.as_str());
            }
        }
    }
    
    // The paragraph should have 3 children:
    // 1. Text node: "Text before "
    // 2. Span element with child text node "inline text"
    // 3. Text node: " text after."
    
    // THIS IS THE KEY TEST: If the parser incorrectly merges text nodes,
    // we'll see only 2 children (one big text node + span), or the text
    // will be malformed
    
    assert_eq!(
        p.children.as_ref().len(), 
        3,
        "Paragraph should have exactly 3 children: [TextNode, Span, TextNode]. \
         Found {} children. This indicates text nodes are being merged incorrectly.",
        p.children.as_ref().len()
    );
    
    // Verify first child is text node with correct content
    match &p.children.as_ref()[0] {
        XmlNodeChild::Text(text) => {
            assert_eq!(
                text.as_str(),
                "Text before ",
                "First child should be text node 'Text before ' (with trailing space)"
            );
        }
        XmlNodeChild::Element(_) => panic!("First child should be a text node"),
    }
    
    // Verify second child is span
    match &p.children.as_ref()[1] {
        XmlNodeChild::Element(span) => {
            assert_eq!(
                span.node_type.as_str(),
                "span",
                "Second child should be <span> element"
            );
            assert_eq!(
                span.children.as_ref().len(),
                1,
                "Span should have 1 child (its text node)"
            );
            
            // Verify span's text content
            match &span.children.as_ref()[0] {
                XmlNodeChild::Text(text) => {
                    assert_eq!(
                        text.as_str(),
                        "inline text",
                        "Span's text node should contain 'inline text'"
                    );
                }
                XmlNodeChild::Element(_) => panic!("Span's child should be a text node"),
            }
        }
        XmlNodeChild::Text(_) => panic!("Second child should be an element (span)"),
    }
    
    // Verify third child is text node with correct content
    match &p.children.as_ref()[2] {
        XmlNodeChild::Text(text) => {
            assert_eq!(
                text.as_str(),
                " text after.",
                "Third child should be text node ' text after.' (with leading space)"
            );
        }
        XmlNodeChild::Element(_) => panic!("Third child should be a text node"),
    }
    
    println!("✓ Test passed: Inline span text node structure is correct");
}

