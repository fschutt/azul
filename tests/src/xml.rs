#[cfg(test)]
use azul_core::xml::{
    compile_body_node_to_rust_code, compile_component, compile_components_to_rust_code,
    format_args_dynamic, get_body_node, get_item, normalize_casing, parse_component_arguments,
    prepare_string, ComponentArgumentVec, ComponentParseError, XmlComponentMap, XmlNode,
};
#[cfg(test)]
use azul_layout::xml::parse_xml_string;

#[test]
fn test_compile_dom_1() {
    use azul_core::callbacks::Dummy;

    // Test the output of a certain component
    fn test_component_source_code(input: &str, component_name: &str, expected: &str) {
        let mut component_map = XmlComponentMap::<Dummy>::default();
        let root_nodes = parse_xml_string(input).unwrap();
        get_xml_components(&root_nodes, &mut component_map).unwrap();
        let body_node = get_body_node(&root_nodes).unwrap();
        let components = compile_components_to_rust_code(&component_map).unwrap();
        let (searched_component_source, searched_component_args) =
            components.get(component_name).unwrap();
        let component_string = compile_component(
            component_name,
            searched_component_args,
            searched_component_source,
        );

        // TODO!
        // assert_eq!(component_string, expected);
    }

    fn test_app_source_code(input: &str, expected: &str) {
        let mut component_map = XmlComponentMap::default();
        let root_nodes = parse_xml_string(input).unwrap();
        get_xml_components(&root_nodes, &mut component_map).unwrap();
        let body_node = get_body_node(&root_nodes).unwrap();
        let app_source = compile_body_node_to_rust_code(&body_node, &component_map).unwrap();

        // TODO!
        // assert_eq!(app_source, expected);
    }

    let s1 = r#"
        <component name="test">
            <div id="a" class="b"></div>
        </component>

        <body>
            <Test />
        </body>
    "#;
    let s1_expected = r#"
        fn test() -> StyledDom {
            Dom::create_div().with_id("a").with_class("b")
        }
    "#;

    test_component_source_code(&s1, "test", &s1_expected);
}

#[test]
fn test_format_args_dynamic() {
    let mut variables = ComponentArgumentVec::new();
    variables.push(("a".to_string(), "value1".to_string()));
    variables.push(("b".to_string(), "value2".to_string()));
    assert_eq!(
        format_args_dynamic("hello {a}, {b}{{ {c} }}", &variables),
        String::from("hello value1, value2{ {c} }"),
    );
    assert_eq!(
        format_args_dynamic("hello {{a}, {b}{{ {c} }}", &variables),
        String::from("hello {a}, value2{ {c} }"),
    );
    assert_eq!(
        format_args_dynamic("hello {{{{{{{ a   }}, {b}{{ {c} }}", &variables),
        String::from("hello {{{{{{ a   }, value2{ {c} }"),
    );
}

#[test]
fn test_normalize_casing() {
    assert_eq!(normalize_casing("abcDef"), String::from("abc_def"));
    assert_eq!(normalize_casing("abc_Def"), String::from("abc_def"));
    assert_eq!(normalize_casing("abc-Def"), String::from("abc_def"));
    assert_eq!(normalize_casing("abc-def"), String::from("abc_def"));
    assert_eq!(normalize_casing("AbcDef"), String::from("abc_def"));
    assert_eq!(normalize_casing("Abc-Def"), String::from("abc_def"));
    assert_eq!(normalize_casing("Abc_Def"), String::from("abc_def"));
    assert_eq!(normalize_casing("aBc_Def"), String::from("a_bc_def")); // wrong, but whatever
    assert_eq!(
        normalize_casing("StartScreen"),
        String::from("start_screen")
    );
}

#[test]
fn test_parse_component_arguments() {
    let mut args_1_expected = ComponentArgumentVec::new();
    args_1_expected.push(("selected_date".to_string(), "DateTime".to_string()));
    args_1_expected.push(("minimum_date".to_string(), "DateTime".to_string()));
    args_1_expected.push(("grid_visible".to_string(), "bool".to_string()));

    // Everything OK
    assert_eq!(
        parse_component_arguments(
            "gridVisible: bool, selectedDate: DateTime, minimumDate: DateTime"
        ),
        Ok(args_1_expected)
    );

    // Missing type for selectedDate
    assert_eq!(
        parse_component_arguments("gridVisible: bool, selectedDate: , minimumDate: DateTime"),
        Err(ComponentParseError::MissingType(
            1,
            "selectedDate".to_string().into(),
        ))
    );

    // Missing name for first argument
    assert_eq!(
        parse_component_arguments(": bool, selectedDate: DateTime, minimumDate: DateTime"),
        Err(ComponentParseError::MissingName(0))
    );

    // Missing comma after DateTime
    assert_eq!(
        parse_component_arguments(
            "gridVisible: bool, selectedDate: DateTime  minimumDate: DateTime"
        ),
        Err(ComponentParseError::WhiteSpaceInComponentType(
            1,
            "selectedDate".to_string().into(),
            "DateTime  minimumDate".to_string().into(),
        ))
    );

    // Missing colon after gridVisible
    assert_eq!(
        parse_component_arguments(
            "gridVisible: bool, selectedDate DateTime, minimumDate: DateTime"
        ),
        Err(ComponentParseError::WhiteSpaceInComponentName(
            1,
            "selectedDate DateTime".to_string().into(),
        ))
    );
}

#[test]
fn test_xml_get_item() {
    // <a>
    //     <b/>
    //     <c/>
    //     <d/>
    //     <e/>
    // </a>
    // <f>
    //     <g>
    //         <h/>
    //     </g>
    //     <i/>
    // </f>
    // <j/>

    let mut tree = XmlNode::create("component").with_children(vec![
        XmlNode::create("a").with_children(vec![
            XmlNode::create("b"),
            XmlNode::create("c"),
            XmlNode::create("d"),
            XmlNode::create("e"),
        ]),
        XmlNode::create("f").with_children(vec![
            XmlNode::create("g").with_children(vec![XmlNode::create("h")]),
            XmlNode::create("i"),
        ]),
        XmlNode::create("j"),
    ]);

    assert_eq!(
        get_item(&[], &mut tree).unwrap().node_type.as_str(),
        "component"
    );
    assert_eq!(get_item(&[0], &mut tree).unwrap().node_type.as_str(), "a");
    assert_eq!(
        get_item(&[0, 0], &mut tree).unwrap().node_type.as_str(),
        "b"
    );
    assert_eq!(
        get_item(&[0, 1], &mut tree).unwrap().node_type.as_str(),
        "c"
    );
    assert_eq!(
        get_item(&[0, 2], &mut tree).unwrap().node_type.as_str(),
        "d"
    );
    assert_eq!(
        get_item(&[0, 3], &mut tree).unwrap().node_type.as_str(),
        "e"
    );
    assert_eq!(get_item(&[1], &mut tree).unwrap().node_type.as_str(), "f");
    assert_eq!(
        get_item(&[1, 0], &mut tree).unwrap().node_type.as_str(),
        "g"
    );
    assert_eq!(
        get_item(&[1, 0, 0], &mut tree).unwrap().node_type.as_str(),
        "h"
    );
    assert_eq!(
        get_item(&[1, 1], &mut tree).unwrap().node_type.as_str(),
        "i"
    );
    assert_eq!(get_item(&[2], &mut tree).unwrap().node_type.as_str(), "j");

    assert_eq!(get_item(&[123213], &mut tree), None);
    assert_eq!(get_item(&[0, 1, 2], &mut tree), None);
}

#[test]
fn test_prepare_string_1() {
    let input1 = r#"Test"#;
    let output = prepare_string(input1);
    assert_eq!(output, String::from("Test"));
}

#[test]
fn test_prepare_string_2() {
    let input1 = r#"
    Hello,
    123


    Test Test2

    Test3




    Test4
    "#;

    let output = prepare_string(input1);
    assert_eq!(output, String::from("Hello, 123\nTest Test2\nTest3\nTest4"));
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
    
    let html = &result[0];
    assert_eq!(html.node_type.as_str(), "html");
    
    let body = &html.children.as_ref()[0];
    assert_eq!(body.node_type.as_str(), "body");
    assert_eq!(body.children.as_ref().len(), 1, "Body should have 1 child (p)");
    
    let p = &body.children.as_ref()[0];
    assert_eq!(p.node_type.as_str(), "p");
    
    println!("\n=== Paragraph Children ===");
    println!("Paragraph has {} children", p.children.as_ref().len());
    for (i, child) in p.children.as_ref().iter().enumerate() {
        println!("  Child {}: node_type='{}', text={:?}", 
            i, 
            child.node_type.as_str(),
            child.text.as_ref().map(|t| t.as_str())
        );
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
    let child0 = &p.children.as_ref()[0];
    assert_eq!(
        child0.text.as_ref().map(|t| t.as_str()),
        Some("Text before "),
        "First child should be text node 'Text before ' (with trailing space)"
    );
    
    // Verify second child is span
    let child1 = &p.children.as_ref()[1];
    assert_eq!(
        child1.node_type.as_str(),
        "span",
        "Second child should be <span> element"
    );
    assert_eq!(
        child1.children.as_ref().len(),
        1,
        "Span should have 1 child (its text node)"
    );
    
    // Verify span's text content
    let span_text = &child1.children.as_ref()[0];
    assert_eq!(
        span_text.text.as_ref().map(|t| t.as_str()),
        Some("inline text"),
        "Span's text should be 'inline text'"
    );
    
    // Verify third child is text node with correct content
    let child2 = &p.children.as_ref()[2];
    assert_eq!(
        child2.text.as_ref().map(|t| t.as_str()),
        Some(" text after."),
        "Third child should be text node ' text after.' (with leading space)"
    );
    
    println!("✓ Test passed: Inline span text node structure is correct");
}
