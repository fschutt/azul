/// Test HTML-style tag parsing with body selector
use azul_core::{
    dom::{Dom, IdOrClass, NodeType},
    styled_dom::StyledDom,
};
use azul_css::parser2::CssApiWrapper;

#[test]
fn test_html_style_tag_with_body_selector() {
    // Test that CSS with body selector works

    let css_str = r#"
        body { 
            font-family: "NotoSansJP", sans-serif; 
            padding: 20px;
        }
    "#;

    let (css, _errors) = azul_css::parser2::new_from_str(css_str);
    let css_wrapper = CssApiWrapper::from(css);

    // Create a body element
    let mut dom = Dom::create_body();

    let styled_dom = StyledDom::create(&mut dom, css_wrapper);

    // Get the root node (body)
    let node_id = azul_core::id::NodeId::ZERO;
    let node_data = &styled_dom.node_data.as_container()[node_id];
    let node_state = &styled_dom.styled_nodes.as_container()[node_id].styled_node_state;
    let cache = &styled_dom.css_property_cache.ptr;

    // Try to get font-family property
    let font_family = cache.get_font_family(node_data, &node_id, node_state);

    eprintln!(
        "body selector test - font_family: {:?}",
        font_family.as_ref().map(|v| format!("{:?}", v))
    );

    assert!(
        font_family.is_some(),
        "font-family should be found for body selector"
    );

    let families = font_family
        .and_then(|v| v.get_property())
        .expect("font-family property should exist");

    eprintln!("body selector test - families count: {}", families.len());
    for i in 0..families.len() {
        if let Some(fam) = families.get(i) {
            eprintln!("  Family {}: {:?}", i, fam.as_string());
        }
    }

    assert_eq!(families.len(), 2, "Should have 2 font families");
}

#[test]
fn test_html_vs_body_node_type() {
    // Check if there's a difference between body and div for CSS matching

    let css_str = r#"
        body { 
            font-family: Arial, sans-serif; 
        }
    "#;

    let (css, _errors) = azul_css::parser2::new_from_str(css_str);
    let css_wrapper = CssApiWrapper::from(css);

    // Test with actual body node
    let mut dom_body = Dom::create_body();
    let styled_dom_body = StyledDom::create(&mut dom_body, css_wrapper.clone());

    let node_id = azul_core::id::NodeId::ZERO;
    let node_data = &styled_dom_body.node_data.as_container()[node_id];
    let node_state = &styled_dom_body.styled_nodes.as_container()[node_id].styled_node_state;
    let cache = &styled_dom_body.css_property_cache.ptr;

    let font_family_body = cache.get_font_family(node_data, &node_id, node_state);

    eprintln!("NodeType for body: {:?}", node_data.node_type);
    eprintln!("Body font_family: {:?}", font_family_body.is_some());

    // Test with div (shouldn't match body selector)
    let mut dom_div = Dom::create_div();
    let styled_dom_div = StyledDom::create(&mut dom_div, css_wrapper);

    let node_data_div = &styled_dom_div.node_data.as_container()[node_id];
    let node_state_div = &styled_dom_div.styled_nodes.as_container()[node_id].styled_node_state;
    let cache_div = &styled_dom_div.css_property_cache.ptr;

    let font_family_div = cache_div.get_font_family(node_data_div, &node_id, node_state_div);

    eprintln!("NodeType for div: {:?}", node_data_div.node_type);
    eprintln!("Div font_family: {:?}", font_family_div.is_some());

    // Body should match body selector
    assert!(
        font_family_body.is_some(),
        "Body should have font-family from CSS"
    );

    // Div should NOT match body selector
    // (unless there's inheritance, which font-family does have)
    eprintln!("Note: Div may inherit font-family if inheritance is implemented");
}
