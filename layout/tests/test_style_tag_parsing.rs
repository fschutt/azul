use azul_layout::xml::parse_xml_string;
use azul_core::xml::XmlNodeChild;

fn as_element(child: &XmlNodeChild) -> &azul_core::xml::XmlNode {
    match child {
        XmlNodeChild::Element(e) => e,
        XmlNodeChild::Text(_) => panic!("Expected element, got text"),
    }
}

fn as_text(child: &XmlNodeChild) -> &str {
    match child {
        XmlNodeChild::Text(t) => t.as_str(),
        XmlNodeChild::Element(_) => panic!("Expected text, got element"),
    }
}

#[test]
fn test_style_tag_parsing_simple() {
    let xml = r#"
<html>
<head>
<style>body { color: red; }</style>
</head>
<body></body>
</html>
"#;
    
    let result = parse_xml_string(xml).unwrap();
    eprintln!("Parsed {} root nodes", result.len());
    
    let html = as_element(&result[0]);
    eprintln!("HTML node has {} children", html.children.as_ref().len());
    assert_eq!(html.node_type.as_str(), "html");
    
    let head = as_element(&html.children.as_ref()[0]);
    eprintln!("HEAD node type: {}", head.node_type.as_str());
    eprintln!("HEAD has {} children", head.children.as_ref().len());
    assert_eq!(head.node_type.as_str(), "head");
    
    // Head should have 1 child: <style>
    assert!(head.children.as_ref().len() > 0, "HEAD should have children (style tag)");
    
    let style = as_element(&head.children.as_ref()[0]);
    eprintln!("STYLE node type: {}", style.node_type.as_str());
    eprintln!("STYLE has {} children", style.children.as_ref().len());
    assert_eq!(style.node_type.as_str(), "style");
    
    // Style should have 1 text child with the CSS
    assert_eq!(style.children.as_ref().len(), 1, "STYLE should have 1 text child");
    let css_text = as_text(&style.children.as_ref()[0]);
    eprintln!("CSS text: '{}'", css_text);
    assert_eq!(css_text, "body { color: red; }");
}

#[test]
fn test_style_tag_parsing_with_quotes() {
    let xml = r#"
<html>
<head>
<style>body { font-family: Arial, sans-serif; }</style>
</head>
<body></body>
</html>
"#;
    
    let result = parse_xml_string(xml).unwrap();
    let html = as_element(&result[0]);
    let head = as_element(&html.children.as_ref()[0]);
    
    eprintln!("HEAD has {} children", head.children.as_ref().len());
    assert!(head.children.as_ref().len() > 0, "HEAD should have style child");
    
    let style = as_element(&head.children.as_ref()[0]);
    assert_eq!(style.node_type.as_str(), "style");
    
    assert_eq!(style.children.as_ref().len(), 1);
    let css_text = as_text(&style.children.as_ref()[0]);
    eprintln!("CSS text: '{}'", css_text);
    assert!(css_text.contains("font-family"), "CSS should contain font-family");
}
