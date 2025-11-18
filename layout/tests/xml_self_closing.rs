use azul_layout::xml::parse_xml_string;

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
    
    let html = &result[0];
    assert_eq!(html.node_type.as_str(), "html");
    assert_eq!(html.children.as_ref().len(), 1);
    
    let body = &html.children.as_ref()[0];
    assert_eq!(body.node_type.as_str(), "body");
    assert_eq!(body.children.as_ref().len(), 3);
    
    // Check that all three children were parsed
    assert_eq!(body.children.as_ref()[0].node_type.as_str(), "header");
    assert_eq!(body.children.as_ref()[1].node_type.as_str(), "div");
    assert_eq!(body.children.as_ref()[2].node_type.as_str(), "footer");
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
    let html = &result[0];
    let body = &html.children.as_ref()[0];
    let header = &body.children.as_ref()[0];
    
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
    let html = &result[0];
    
    // Should have head and body
    assert_eq!(html.children.as_ref().len(), 2);
    let head = &html.children.as_ref()[0];
    let body = &html.children.as_ref()[1];
    
    assert_eq!(head.node_type.as_str(), "head");
    assert_eq!(body.node_type.as_str(), "body");
    
    // Body should have header, div, footer
    assert_eq!(body.children.as_ref().len(), 3);
    assert_eq!(body.children.as_ref()[0].node_type.as_str(), "header");
    assert_eq!(body.children.as_ref()[1].node_type.as_str(), "div");
    assert_eq!(body.children.as_ref()[2].node_type.as_str(), "footer");
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
    let html = &result[0];
    let body = &html.children.as_ref()[0];
    let ul = &body.children.as_ref()[0];
    
    // Should have 3 list items, each auto-closed
    assert_eq!(ul.children.as_ref().len(), 3);
    assert_eq!(ul.children.as_ref()[0].node_type.as_str(), "li");
    assert_eq!(ul.children.as_ref()[1].node_type.as_str(), "li");
    assert_eq!(ul.children.as_ref()[2].node_type.as_str(), "li");
    
    // First item should have text
    assert!(ul.children.as_ref()[0].text.as_ref().is_some());
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
    let html = &result[0];
    let body = &html.children.as_ref()[0];
    
    // Should have 4 children: p, div, p, h1
    assert_eq!(body.children.as_ref().len(), 4);
    assert_eq!(body.children.as_ref()[0].node_type.as_str(), "p");
    assert_eq!(body.children.as_ref()[1].node_type.as_str(), "div");
    assert_eq!(body.children.as_ref()[2].node_type.as_str(), "p");
    assert_eq!(body.children.as_ref()[3].node_type.as_str(), "h1");
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
    let html = &result[0];
    let body = &html.children.as_ref()[0];
    let div = &body.children.as_ref()[0];
    
    // Should have 2 paragraphs
    assert_eq!(div.children.as_ref().len(), 2);
    assert_eq!(div.children.as_ref()[0].node_type.as_str(), "p");
    assert_eq!(div.children.as_ref()[1].node_type.as_str(), "p");
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
    let html = &result[0];
    let body = &html.children.as_ref()[0];
    let table = &body.children.as_ref()[0];
    
    // Should have 2 rows
    assert_eq!(table.children.as_ref().len(), 2, "Table should have 2 rows");
    let row1 = &table.children.as_ref()[0];
    let row2 = &table.children.as_ref()[1];
    
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
    let html = &result[0];
    let body = &html.children.as_ref()[0];
    
    // Should have 4 void elements
    assert_eq!(body.children.as_ref().len(), 4);
    assert_eq!(body.children.as_ref()[0].node_type.as_str(), "img");
    assert_eq!(body.children.as_ref()[1].node_type.as_str(), "br");
    assert_eq!(body.children.as_ref()[2].node_type.as_str(), "hr");
    assert_eq!(body.children.as_ref()[3].node_type.as_str(), "input");
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
    let html = &nodes[0];
    let body = &html.children.as_ref()[0];
    
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
    let html = &result[0];
    let body = &html.children.as_ref()[0];
    
    // Should have 5 children: img, br, hr, input, div
    assert_eq!(body.children.as_ref().len(), 5);
    assert_eq!(body.children.as_ref()[0].node_type.as_str(), "img");
    assert_eq!(body.children.as_ref()[1].node_type.as_str(), "br");
    assert_eq!(body.children.as_ref()[2].node_type.as_str(), "hr");
    assert_eq!(body.children.as_ref()[3].node_type.as_str(), "input");
    assert_eq!(body.children.as_ref()[4].node_type.as_str(), "div");
}
