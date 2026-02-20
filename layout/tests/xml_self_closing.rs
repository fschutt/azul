/// Tests for XML/HTML5-lite parser self-closing tags, void elements,
/// auto-close behavior, and lenient parsing. Verifies that the parser
/// handles both strict XML (`<br/>`) and HTML5-like (`<br>`) syntax.
use azul_layout::xml::{parse_xml_string, XmlNodeChild};

/// Extract the XmlNode from an Element variant (panics on Text nodes).
fn as_element(child: &XmlNodeChild) -> &azul_core::xml::XmlNode {
    match child {
        XmlNodeChild::Element(node) => node,
        _ => panic!("Expected element node, got text node"),
    }
}

/// Filter children to only Element nodes (skips whitespace text nodes).
fn element_children(children: &[XmlNodeChild]) -> Vec<&azul_core::xml::XmlNode> {
    children
        .iter()
        .filter_map(|c| match c {
            XmlNodeChild::Element(node) => Some(node),
            _ => None,
        })
        .collect()
}

/// Verifies that explicit self-closing tags like `<header/>` parse correctly
/// and produce empty element nodes in the tree.
#[test]
fn test_self_closing_tags() {
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
    let html = as_element(&result[0]);
    assert_eq!(html.node_type.as_str(), "html");
    let html_elems = element_children(html.children.as_ref());
    assert_eq!(html_elems.len(), 1);

    let body = html_elems[0];
    assert_eq!(body.node_type.as_str(), "body");
    let body_elems = element_children(body.children.as_ref());
    assert_eq!(body_elems.len(), 3);

    assert_eq!(body_elems[0].node_type.as_str(), "header");
    assert_eq!(body_elems[1].node_type.as_str(), "div");
    assert_eq!(body_elems[2].node_type.as_str(), "footer");
}

/// Verifies that self-closing tags preserve their attributes correctly.
#[test]
fn test_self_closing_with_attributes() {
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
    let body = element_children(html.children.as_ref())[0];
    let header = element_children(body.children.as_ref())[0];

    assert_eq!(header.node_type.as_str(), "header");
    assert_eq!(header.attributes.as_ref().len(), 1);
    assert_eq!(header.attributes.as_ref()[0].key.as_str(), "exclude-pages");
    assert_eq!(header.attributes.as_ref()[0].value.as_str(), "1");
    assert_eq!(header.children.as_ref().len(), 0);
}

/// Verifies that self-closing (`<br/>`) and regular (`<div>...</div>`)
/// tags can be mixed freely in the same parent.
#[test]
fn test_mixed_self_closing_and_regular() {
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
    let html_elems = element_children(html.children.as_ref());
    assert_eq!(html_elems.len(), 2);

    let head = html_elems[0];
    let body = html_elems[1];
    assert_eq!(head.node_type.as_str(), "head");
    assert_eq!(body.node_type.as_str(), "body");

    let body_elems = element_children(body.children.as_ref());
    assert_eq!(body_elems.len(), 3);
    assert_eq!(body_elems[0].node_type.as_str(), "header");
    assert_eq!(body_elems[1].node_type.as_str(), "div");
    assert_eq!(body_elems[2].node_type.as_str(), "footer");
}

/// Verifies HTML5 auto-closing: `<li>` elements auto-close when
/// a sibling `<li>` is encountered, without explicit `</li>`.
#[test]
fn test_html5_auto_close_list_items() {
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
    let body = element_children(html.children.as_ref())[0];
    let ul = element_children(body.children.as_ref())[0];

    // Parser auto-closes <li> when encountering the next <li>
    let li_items = element_children(ul.children.as_ref());
    assert_eq!(li_items.len(), 3, "Should have 3 list items");
    assert_eq!(li_items[0].node_type.as_str(), "li");
    assert_eq!(li_items[1].node_type.as_str(), "li");
    assert_eq!(li_items[2].node_type.as_str(), "li");

    // Each <li> should contain its text
    assert!(li_items[0].children.as_ref().len() > 0, "First li should have text");
}

/// Verifies that `<p>` auto-closes when encountering block-level elements
/// like `<div>` or `<h1>`, per HTML5 spec.
#[test]
fn test_html5_paragraph_auto_close() {
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
    let body = element_children(html.children.as_ref())[0];

    // Should have 4 element children: p, div, p, h1
    let body_elems = element_children(body.children.as_ref());
    assert_eq!(body_elems.len(), 4, "body should have p, div, p, h1");
    assert_eq!(body_elems[0].node_type.as_str(), "p");
    assert_eq!(body_elems[1].node_type.as_str(), "div");
    assert_eq!(body_elems[2].node_type.as_str(), "p");
    assert_eq!(body_elems[3].node_type.as_str(), "h1");
}

/// Verifies that `<p>` auto-closes when another `<p>` is encountered
/// inside the same parent container, without explicit `</p>`.
#[test]
fn test_html5_optional_closing_tags() {
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
    let body = element_children(html.children.as_ref())[0];
    let div = element_children(body.children.as_ref())[0];

    // Should have 2 paragraphs (auto-closed)
    let div_elems = element_children(div.children.as_ref());
    assert_eq!(div_elems.len(), 2, "div should have 2 paragraphs");
    assert_eq!(div_elems[0].node_type.as_str(), "p");
    assert_eq!(div_elems[1].node_type.as_str(), "p");
}

/// Verifies that `<td>` elements auto-close when encountering a sibling
/// `<td>`, and `<tr>` elements contain the correct cell structure.
#[test]
fn test_html5_table_auto_close() {
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
    let body = element_children(html.children.as_ref())[0];
    let table = element_children(body.children.as_ref())[0];

    let rows = element_children(table.children.as_ref());
    assert_eq!(rows.len(), 2, "Table should have 2 rows");

    let row1_cells = element_children(rows[0].children.as_ref());
    let row2_cells = element_children(rows[1].children.as_ref());
    assert_eq!(row1_cells.len(), 2, "First row should have 2 cells");
    assert_eq!(row2_cells.len(), 2, "Second row should have 2 cells");
}

/// Verifies that void elements (`<img>`, `<br>`, etc.) tolerate incorrect
/// explicit closing tags like `</img>` without breaking the tree.
#[test]
fn test_html5_void_elements_with_wrong_closing() {
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
    let body = element_children(html.children.as_ref())[0];

    let body_elems = element_children(body.children.as_ref());
    assert_eq!(body_elems.len(), 4);
    assert_eq!(body_elems[0].node_type.as_str(), "img");
    assert_eq!(body_elems[1].node_type.as_str(), "br");
    assert_eq!(body_elems[2].node_type.as_str(), "hr");
    assert_eq!(body_elems[3].node_type.as_str(), "input");
}

/// Verifies lenient parsing: a `<header>` without explicit `</header>`
/// stays open until its parent closes. The `<footer>` becomes a child
/// of `<header>` since the parser has no auto-close rule for header->footer.
#[test]
fn test_header_without_closing_tag_lenient() {
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

    let result = parse_xml_string(xml);
    assert!(result.is_ok(), "Should succeed with lenient HTML5 parsing");

    let nodes = result.unwrap();
    let html = as_element(&nodes[0]);
    let body = element_children(html.children.as_ref())[0];

    // With lenient parsing, <header> stays open (no auto-close rule for
    // header->footer), so footer becomes a child of header.
    let body_elems = element_children(body.children.as_ref());
    assert!(body_elems.len() >= 1, "Should have at least one element child");

    let header = body_elems[0];
    assert_eq!(header.node_type.as_str(), "header");
    let header_elems = element_children(header.children.as_ref());
    // header contains: div, hr, footer
    assert!(header_elems.len() >= 2, "Header should contain div, hr (and footer as child)");
}

/// Verifies that HTML5 void elements (`<br>`, `<hr>`, `<img>`, `<input>`)
/// are auto-closed even without `/>` and never consume children.
#[test]
fn test_auto_close_void_tags() {
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
    let body = element_children(html.children.as_ref())[0];

    let body_elems = element_children(body.children.as_ref());
    assert_eq!(body_elems.len(), 5, "body should have img, br, hr, input, div");
    assert_eq!(body_elems[0].node_type.as_str(), "img");
    assert_eq!(body_elems[1].node_type.as_str(), "br");
    assert_eq!(body_elems[2].node_type.as_str(), "hr");
    assert_eq!(body_elems[3].node_type.as_str(), "input");
    assert_eq!(body_elems[4].node_type.as_str(), "div");
}

/// Verifies that text nodes around inline elements (`<span>`) are preserved
/// as separate text nodes and not merged. Critical for correct text layout
/// where "Text before " and " text after." must remain distinct nodes.
#[test]
fn test_inline_span_text_node_structure() {
    let xml = r#"
        <html>
            <body>
                <p>Text before <span class="highlight">inline text</span> text after.</p>
            </body>
        </html>
    "#;

    let result = parse_xml_string(xml).expect("Should parse XML successfully");
    assert_eq!(result.len(), 1, "Should have one root node");

    let html = as_element(&result[0]);
    assert_eq!(html.node_type.as_str(), "html");

    let body = element_children(html.children.as_ref())[0];
    assert_eq!(body.node_type.as_str(), "body");
    let body_elems = element_children(body.children.as_ref());
    assert_eq!(body_elems.len(), 1, "Body should have 1 element child (p)");

    let p = body_elems[0];
    assert_eq!(p.node_type.as_str(), "p");

    // The paragraph should have exactly 3 children:
    // [0] Text: "Text before "
    // [1] Element: <span> with text "inline text"
    // [2] Text: " text after."
    assert_eq!(
        p.children.as_ref().len(),
        3,
        "Paragraph should have [TextNode, Span, TextNode], found {} children",
        p.children.as_ref().len()
    );

    match &p.children.as_ref()[0] {
        XmlNodeChild::Text(text) => {
            assert_eq!(text.as_str(), "Text before ");
        }
        XmlNodeChild::Element(_) => panic!("First child should be a text node"),
    }

    match &p.children.as_ref()[1] {
        XmlNodeChild::Element(span) => {
            assert_eq!(span.node_type.as_str(), "span");
            assert_eq!(span.children.as_ref().len(), 1);
            match &span.children.as_ref()[0] {
                XmlNodeChild::Text(text) => {
                    assert_eq!(text.as_str(), "inline text");
                }
                XmlNodeChild::Element(_) => panic!("Span's child should be a text node"),
            }
        }
        XmlNodeChild::Text(_) => panic!("Second child should be an element (span)"),
    }

    match &p.children.as_ref()[2] {
        XmlNodeChild::Text(text) => {
            assert_eq!(text.as_str(), " text after.");
        }
        XmlNodeChild::Element(_) => panic!("Third child should be a text node"),
    }
}
