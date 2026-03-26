/// Tests for compact cache correctness: verify that CSS properties
/// are correctly resolved through the cascade pipeline.

use azul_core::dom::{Dom, NodeType};
use azul_core::styled_dom::StyledDom;
use azul_css::css::Css;
use azul_css::compact_cache::*;

/// Helper: create StyledDom from Dom + CSS string
fn styled(dom: Dom, css_str: &str) -> StyledDom {
    let css = if css_str.is_empty() {
        Css::empty()
    } else {
        Css::from_string(css_str.into())
    };
    let mut dom = dom;
    StyledDom::create(&mut dom, css)
}

/// Read display from compact cache for a given node index
fn get_display(styled: &StyledDom, idx: usize) -> u8 {
    let cache = styled.css_property_cache.ptr.compact_cache.as_ref().unwrap();
    ((cache.tier1_enums[idx] >> DISPLAY_SHIFT) & DISPLAY_MASK) as u8
}

/// Read font_size from compact cache
fn get_font_size_u32(styled: &StyledDom, idx: usize) -> u32 {
    styled.css_property_cache.ptr.compact_cache.as_ref().unwrap().tier2_dims[idx].font_size
}

/// Read font_weight from compact tier1
fn get_font_weight(styled: &StyledDom, idx: usize) -> u8 {
    let cache = styled.css_property_cache.ptr.compact_cache.as_ref().unwrap();
    ((cache.tier1_enums[idx] >> FONT_WEIGHT_SHIFT) & FONT_WEIGHT_MASK) as u8
}

// ===================================================================
// UA CSS defaults
// ===================================================================

#[test]
fn test_h1_display_block_from_ua() {
    // H1 should get display:block from UA CSS even without any author CSS
    let dom = Dom::create_html().with_child(
        Dom::create_body().with_child(
            Dom::create_node(NodeType::H1).with_child(Dom::create_text("heading"))
        )
    );
    let s = styled(dom, "");
    // node 0 = Html, 1 = Body, 2 = H1, 3 = text
    assert_eq!(get_display(&s, 0), 0, "Html should be display:block (0)");
    assert_eq!(get_display(&s, 1), 0, "Body should be display:block (0)");
    assert_eq!(get_display(&s, 2), 0, "H1 should be display:block (0)");
    assert_eq!(get_display(&s, 3), 1, "Text should be display:inline (1)");
}

#[test]
fn test_div_block_span_inline_from_ua() {
    let dom = Dom::create_html().with_child(
        Dom::create_body()
            .with_child(Dom::create_div().with_child(Dom::create_text("div")))
            .with_child(Dom::create_node(NodeType::Span).with_child(Dom::create_text("span")))
    );
    let s = styled(dom, "");
    // 0=Html, 1=Body, 2=Div, 3=text, 4=Span, 5=text
    assert_eq!(get_display(&s, 2), 0, "Div should be display:block (0)");
    assert_eq!(get_display(&s, 4), 1, "Span should be display:inline (1)");
}

#[test]
fn test_h1_bold_from_ua() {
    let dom = Dom::create_html().with_child(
        Dom::create_body().with_child(
            Dom::create_node(NodeType::H1).with_child(Dom::create_text("h"))
        )
    );
    let s = styled(dom, "");
    // font_weight bold = style_font_weight_to_u8(Bold) = 7
    let h1_fw = get_font_weight(&s, 2);
    assert_eq!(h1_fw, 7, "H1 should have font-weight:bold (7) from UA, got {}", h1_fw);
}

#[test]
fn test_h1_font_size_from_ua() {
    // H1 should get font-size: 2em from UA CSS
    let dom = Dom::create_html().with_child(
        Dom::create_body().with_child(
            Dom::create_node(NodeType::H1).with_child(Dom::create_text("h"))
        )
    );
    let s = styled(dom, "");
    let h1_fs = get_font_size_u32(&s, 2);
    // 2em = PixelValue { metric: Em, number: FloatValue(2000) }
    // encode_pixel_value_u32: value_bits = (2000 as i32 as u32) << 4, metric = Em = 2
    assert_ne!(h1_fs, 0, "H1 font-size should not be 0 (unset)");
    // Decode: metric = h1_fs & 0xF, value = (h1_fs >> 4) as i32
    let metric = h1_fs & 0xF;
    let value = (h1_fs >> 4) as i32;
    assert_eq!(metric, 2, "H1 font-size metric should be Em (2), got {}", metric);
    assert_eq!(value, 2000, "H1 font-size value should be 2000 (2.0em), got {}", value);
}

// ===================================================================
// Author CSS overrides
// ===================================================================

#[test]
fn test_author_css_display_flex_overrides_ua_block() {
    let dom = Dom::create_html().with_child(
        Dom::create_body().with_child(
            Dom::create_div().with_child(Dom::create_text("flex"))
        )
    );
    let s = styled(dom, "div { display: flex; }");
    // Div should now be flex (4) instead of block (1)
    assert_eq!(get_display(&s, 2), 3, "Div should be display:flex (3) from author CSS");
}

#[test]
fn test_global_star_doesnt_override_ua_display() {
    // `* { margin: 0; }` should NOT change display (it doesn't set display)
    let dom = Dom::create_html().with_child(
        Dom::create_body().with_child(
            Dom::create_node(NodeType::H1).with_child(Dom::create_text("h1"))
        )
    );
    let s = styled(dom, "* { margin: 0; padding: 0; }");
    assert_eq!(get_display(&s, 2), 0, "H1 should still be display:block after * reset");
}

// ===================================================================
// Inheritance
// ===================================================================

#[test]
fn test_font_weight_inherits_from_parent() {
    // Parent sets font-weight:bold, child should inherit
    use azul_css::dynamic_selector::CssPropertyWithConditions;
    use azul_css::props::property::CssProperty;
    use azul_css::props::basic::StyleFontWeight;

    let dom = Dom::create_html().with_child(
        Dom::create_body()
            .with_css_props(vec![
                CssPropertyWithConditions::simple(
                    CssProperty::font_weight(StyleFontWeight::Bold)
                )
            ].into())
            .with_child(Dom::create_div().with_child(Dom::create_text("child")))
    );
    let s = styled(dom, "");
    // 0=Html, 1=Body (bold), 2=Div, 3=text
    let body_fw = get_font_weight(&s, 1);
    let div_fw = get_font_weight(&s, 2);
    assert_eq!(body_fw, 7, "Body should have font-weight:bold (7)");
    assert_eq!(div_fw, 7, "Div should inherit font-weight:bold (7) from body, got {}", div_fw);
}

#[test]
fn test_display_does_not_inherit() {
    // Body is display:block, but a Span child should NOT inherit block
    let dom = Dom::create_html().with_child(
        Dom::create_body().with_child(
            Dom::create_node(NodeType::Span).with_child(Dom::create_text("span"))
        )
    );
    let s = styled(dom, "");
    assert_eq!(get_display(&s, 1), 0, "Body should be block");
    // Span gets display:inline from UA, NOT inherited block from body
    assert_eq!(get_display(&s, 2), 1, "Span should be display:inline (1), not inherited block");
}

// ===================================================================
// Inline CSS priority
// ===================================================================

#[test]
fn test_inline_css_overrides_stylesheet() {
    use azul_css::dynamic_selector::CssPropertyWithConditions;
    use azul_css::props::property::CssProperty;
    use azul_css::props::basic::pixel::PixelValue;

    // Stylesheet says div { width: 100px }, inline says width: 200px
    let dom = Dom::create_html().with_child(
        Dom::create_body().with_child(
            Dom::create_div()
                .with_css_props(vec![
                    CssPropertyWithConditions::simple(
                        CssProperty::width(azul_css::props::layout::LayoutWidth::Px(PixelValue::px(200.0)))
                    )
                ].into())
                .with_child(Dom::create_text("wide"))
        )
    );
    let s = styled(dom, "div { width: 100px; }");
    let w = s.css_property_cache.ptr.compact_cache.as_ref().unwrap().tier2_dims[2].width;
    // 200px should win over 100px (inline > stylesheet)
    // decode: metric = w & 0xF (Px=0), value = (w >> 4) as i32
    let value = (w >> 4) as i32;
    assert_eq!(value, 200000, "Width should be 200px (200000), got {} (inline should override stylesheet)", value);
}

// ===================================================================
// Background color (non-compact, paint-time property)
// ===================================================================

#[test]
fn test_background_color_via_class_selector() {
    use azul_core::dom::IdOrClass;

    let dom = Dom::create_html().with_child(
        Dom::create_body().with_child(
            Dom::create_div()
                .with_ids_and_classes(vec![IdOrClass::Class("red".into())].into())
                .with_child(Dom::create_text("red bg"))
        )
    );
    let s = styled(dom, ".red { background-color: #ff0000; }");

    // Background is non-compact — read via get_property_slow
    let cache = &s.css_property_cache.ptr;
    let node_data = &s.node_data.as_container();
    let div_id = azul_core::dom::NodeId::new(2);
    let state = azul_core::styled_dom::StyledNodeState::default();

    let bg = cache.get_background_content(&node_data[div_id], &div_id, &state);
    assert!(bg.is_some(), "Div with class 'red' should have background-color from .red {{ background-color: #ff0000 }}");
}
