/// Test to verify that font-family CSS property is correctly parsed and accessible
///
/// This test reproduces the bug where `cache.get_font_family()` returns None
/// even when font-family is explicitly set in CSS.
use azul_core::{
    dom::{Dom, IdOrClass},
    styled_dom::StyledDom,
};
use azul_css::css::Css;

#[test]
fn test_font_family_parsing_simple() {
    // RED TEST: This should fail initially, showing the bug

    let css_str = r#"
        .test-body { 
            font-family: Arial, sans-serif; 
        }
    "#;

    let (css, _errors) = azul_css::parser2::new_from_str(css_str);

    // Create a simple DOM with a div element
    let mut dom =
        Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("test-body".into())].into());

    // Apply CSS to create StyledDom
    let styled_dom = StyledDom::create(&mut dom, css);

    // Get the root node
    let node_id = azul_core::id::NodeId::ZERO;
    let node_data = &styled_dom.node_data.as_container()[node_id];
    let node_state = &styled_dom.styled_nodes.as_container()[node_id].styled_node_state;
    let cache = &styled_dom.css_property_cache.ptr;

    // Try to get font-family property
    let font_family = cache.get_font_family(node_data, &node_id, node_state);

    eprintln!(
        "font_family result: {:?}",
        font_family.as_ref().map(|v| format!("{:?}", v))
    );

    // This should NOT be None - we explicitly set font-family in CSS
    assert!(
        font_family.is_some(),
        "font-family should be found in CSS cache"
    );

    let families = font_family
        .and_then(|v| v.get_property())
        .expect("font-family property should exist");

    eprintln!("families count: {}", families.len());
    for i in 0..families.len() {
        if let Some(fam) = families.get(i) {
            eprintln!("  Family {}: {:?}", i, fam.as_string());
        }
    }

    // Should have 2 families: Arial and sans-serif
    assert_eq!(families.len(), 2, "Should have 2 font families");

    let arial = families.get(0).expect("Should have first family");
    let arial_string = arial.as_string();
    assert!(
        arial_string.contains("Arial") || arial_string.contains("arial"),
        "First font should be Arial, got: {}",
        arial_string
    );

    let sans_serif = families.get(1).expect("Should have second family");
    let sans_string = sans_serif.as_string();
    assert!(
        sans_string.contains("sans-serif"),
        "Second font should be sans-serif, got: {}",
        sans_string
    );
}

#[test]
fn test_font_family_parsing_quoted() {
    // Test with quoted font names like "Times New Roman"

    let css_str = r#"
        .heading { 
            font-family: "Times New Roman", Georgia, serif; 
        }
    "#;

    let (css, _errors) = azul_css::parser2::new_from_str(css_str);

    // Create DOM with a div that has the "heading" class
    let mut dom = Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("heading".into())].into());

    let styled_dom = StyledDom::create(&mut dom, css);

    let node_id = azul_core::id::NodeId::ZERO;
    let node_data = &styled_dom.node_data.as_container()[node_id];
    let node_state = &styled_dom.styled_nodes.as_container()[node_id].styled_node_state;
    let cache = &styled_dom.css_property_cache.ptr;

    let font_family = cache.get_font_family(node_data, &node_id, node_state);

    eprintln!(
        "quoted test - font_family: {:?}",
        font_family.as_ref().map(|v| format!("{:?}", v))
    );

    assert!(
        font_family.is_some(),
        "font-family should be found for .heading class"
    );

    let families = font_family
        .and_then(|v| v.get_property())
        .expect("font-family property should exist");

    eprintln!("quoted test - families count: {}", families.len());

    assert_eq!(families.len(), 3, "Should have 3 font families");
}

#[test]
fn test_font_family_parsing_japanese() {
    // Test with Japanese font names like in the recipe.html

    let css_str = r#"
        .recipe-body { 
            font-family: "NotoSansJP", sans-serif; 
        }
    "#;

    let (css, _errors) = azul_css::parser2::new_from_str(css_str);

    let mut dom =
        Dom::create_div().with_ids_and_classes(vec![IdOrClass::Class("recipe-body".into())].into());

    let styled_dom = StyledDom::create(&mut dom, css);

    let node_id = azul_core::id::NodeId::ZERO;
    let node_data = &styled_dom.node_data.as_container()[node_id];
    let node_state = &styled_dom.styled_nodes.as_container()[node_id].styled_node_state;
    let cache = &styled_dom.css_property_cache.ptr;

    let font_family = cache.get_font_family(node_data, &node_id, node_state);

    eprintln!(
        "japanese test - font_family: {:?}",
        font_family.as_ref().map(|v| format!("{:?}", v))
    );

    assert!(
        font_family.is_some(),
        "font-family should be found with Japanese font name"
    );

    let families = font_family
        .and_then(|v| v.get_property())
        .expect("font-family property should exist");

    eprintln!("japanese test - families count: {}", families.len());
    for i in 0..families.len() {
        if let Some(fam) = families.get(i) {
            eprintln!("  Family {}: {:?}", i, fam.as_string());
        }
    }

    assert_eq!(families.len(), 2, "Should have 2 font families");

    let noto = families.get(0).expect("Should have first family");
    let noto_string = noto.as_string();
    assert!(
        noto_string.contains("NotoSansJP") || noto_string.contains("notosansjp"),
        "First font should be NotoSansJP, got: {}",
        noto_string
    );
}
