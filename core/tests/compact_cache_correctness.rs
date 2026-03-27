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
    // font_weight bold = style_font_weight_to_u8(Bold) = 6
    let h1_fw = get_font_weight(&s, 2);
    assert_eq!(h1_fw, 6, "H1 should have font-weight:bold (6) from UA, got {}", h1_fw);
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
    assert_eq!(body_fw, 6, "Body should have font-weight:bold (6)");
    assert_eq!(div_fw, 6, "Div should inherit font-weight:bold (6) from body, got {}", div_fw);
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

// ===================================================================
// Round-trip: CSS → compact cache → getter reads
// Tests that the compact builder encodes values correctly AND the
// layout engine's getters decode them back to the original values.
// ===================================================================

/// Helper: create a styled dom with a body child div that has the given CSS
fn styled_div_with_css(css: &str) -> StyledDom {
    use azul_core::dom::IdOrClass;
    let dom = Dom::create_html().with_child(
        Dom::create_body().with_child(
            Dom::create_div()
                .with_ids_and_classes(vec![IdOrClass::Class("t".into())].into())
        )
    );
    styled(dom, css)
}

/// Read a compact i16 field and decode to f32 px value (÷10)
fn read_compact_i16_as_px(styled: &StyledDom, idx: usize, field: &str) -> f32 {
    let cc = styled.css_property_cache.ptr.compact_cache.as_ref().unwrap();
    let d = &cc.tier2_dims[idx];
    let raw = match field {
        "padding_top" => d.padding_top,
        "padding_bottom" => d.padding_bottom,
        "padding_left" => d.padding_left,
        "padding_right" => d.padding_right,
        "margin_top" => d.margin_top,
        "margin_bottom" => d.margin_bottom,
        "margin_left" => d.margin_left,
        "margin_right" => d.margin_right,
        "border_top_width" => d.border_top_width,
        "border_bottom_width" => d.border_bottom_width,
        "border_left_width" => d.border_left_width,
        "border_right_width" => d.border_right_width,
        "top" => d.top,
        "bottom" => d.bottom,
        "left" => d.left,
        "right" => d.right,
        _ => panic!("Unknown field: {}", field),
    };
    raw as f32 / 10.0
}

/// Read a compact u32 width/height field and decode to f32 px value
fn read_compact_u32_as_px(styled: &StyledDom, idx: usize, field: &str) -> f32 {
    let cc = styled.css_property_cache.ptr.compact_cache.as_ref().unwrap();
    let d = &cc.tier2_dims[idx];
    let raw = match field {
        "width" => d.width,
        "height" => d.height,
        _ => panic!("Unknown field: {}", field),
    };
    // Decode: lower 4 bits = metric, upper 28 bits = value
    let metric = raw & 0xF;
    let value = (raw >> 4) as i32;
    if metric == 0 { // Px
        value as f32 / 1000.0
    } else {
        panic!("Expected Px metric (0), got {}", metric);
    }
}

#[test]
fn test_roundtrip_padding() {
    let s = styled_div_with_css(".t { padding: 15px; }");
    // node 0=Html, 1=Body, 2=Div
    assert_eq!(read_compact_i16_as_px(&s, 2, "padding_top"), 15.0, "padding-top");
    assert_eq!(read_compact_i16_as_px(&s, 2, "padding_bottom"), 15.0, "padding-bottom");
    assert_eq!(read_compact_i16_as_px(&s, 2, "padding_left"), 15.0, "padding-left");
    assert_eq!(read_compact_i16_as_px(&s, 2, "padding_right"), 15.0, "padding-right");
}

#[test]
fn test_roundtrip_margin() {
    let s = styled_div_with_css(".t { margin: 10px; }");
    assert_eq!(read_compact_i16_as_px(&s, 2, "margin_top"), 10.0, "margin-top");
    assert_eq!(read_compact_i16_as_px(&s, 2, "margin_bottom"), 10.0, "margin-bottom");
    assert_eq!(read_compact_i16_as_px(&s, 2, "margin_left"), 10.0, "margin-left");
    assert_eq!(read_compact_i16_as_px(&s, 2, "margin_right"), 10.0, "margin-right");
}

#[test]
fn test_roundtrip_width_height() {
    let s = styled_div_with_css(".t { width: 200px; height: 100px; }");
    assert_eq!(read_compact_u32_as_px(&s, 2, "width"), 200.0, "width");
    assert_eq!(read_compact_u32_as_px(&s, 2, "height"), 100.0, "height");
}

#[test]
fn test_roundtrip_border_width() {
    let s = styled_div_with_css(".t { border: 3px solid red; }");
    assert_eq!(read_compact_i16_as_px(&s, 2, "border_top_width"), 3.0, "border-top-width");
    assert_eq!(read_compact_i16_as_px(&s, 2, "border_bottom_width"), 3.0, "border-bottom-width");
    assert_eq!(read_compact_i16_as_px(&s, 2, "border_left_width"), 3.0, "border-left-width");
    assert_eq!(read_compact_i16_as_px(&s, 2, "border_right_width"), 3.0, "border-right-width");
}

#[test]
fn test_roundtrip_position_offsets() {
    let s = styled_div_with_css(".t { position: absolute; top: 10px; left: 20px; bottom: 30px; right: 40px; }");
    let cc = s.css_property_cache.ptr.compact_cache.as_ref().unwrap();
    let t1 = cc.tier1_enums[2];
    let pos = ((t1 >> POSITION_SHIFT) & POSITION_MASK) as u8;
    // position: absolute = 2 in new encoding (Static=0, Relative=1, Absolute=2)
    assert_eq!(pos, 2, "position should be absolute (2), got {}", pos);
    assert_eq!(read_compact_i16_as_px(&s, 2, "top"), 10.0, "top");
    assert_eq!(read_compact_i16_as_px(&s, 2, "left"), 20.0, "left");
    assert_eq!(read_compact_i16_as_px(&s, 2, "bottom"), 30.0, "bottom");
    assert_eq!(read_compact_i16_as_px(&s, 2, "right"), 40.0, "right");
}

#[test]
fn test_roundtrip_display_flex() {
    let s = styled_div_with_css(".t { display: flex; flex-direction: column; flex-wrap: wrap; justify-content: center; align-items: center; }");
    let cc = s.css_property_cache.ptr.compact_cache.as_ref().unwrap();
    let t1 = cc.tier1_enums[2];
    let display = ((t1 >> DISPLAY_SHIFT) & DISPLAY_MASK) as u8;
    assert_eq!(display, layout_display_to_u8(azul_css::props::layout::LayoutDisplay::Flex), "display:flex");
    let flex_dir = ((t1 >> FLEX_DIRECTION_SHIFT) & FLEX_DIR_MASK) as u8;
    assert_eq!(flex_dir, layout_flex_direction_to_u8(azul_css::props::layout::LayoutFlexDirection::Column), "flex-direction:column");
    let flex_wrap = ((t1 >> FLEX_WRAP_SHIFT) & FLEX_WRAP_MASK) as u8;
    assert_eq!(flex_wrap, layout_flex_wrap_to_u8(azul_css::props::layout::LayoutFlexWrap::Wrap), "flex-wrap:wrap");
}

#[test]
fn test_roundtrip_overflow_visible_default() {
    // No overflow set → should default to visible (0)
    let s = styled_div_with_css(".t { width: 100px; }");
    let cc = s.css_property_cache.ptr.compact_cache.as_ref().unwrap();
    let t1 = cc.tier1_enums[2];
    let ox = ((t1 >> OVERFLOW_X_SHIFT) & OVERFLOW_MASK) as u8;
    let oy = ((t1 >> OVERFLOW_Y_SHIFT) & OVERFLOW_MASK) as u8;
    assert_eq!(ox, 0, "overflow-x should default to visible (0)");
    assert_eq!(oy, 0, "overflow-y should default to visible (0)");
}

#[test]
fn test_roundtrip_overflow_hidden() {
    let s = styled_div_with_css(".t { overflow: hidden; }");
    let cc = s.css_property_cache.ptr.compact_cache.as_ref().unwrap();
    let t1 = cc.tier1_enums[2];
    let ox = ((t1 >> OVERFLOW_X_SHIFT) & OVERFLOW_MASK) as u8;
    let oy = ((t1 >> OVERFLOW_Y_SHIFT) & OVERFLOW_MASK) as u8;
    assert_eq!(ox, layout_overflow_to_u8(azul_css::props::layout::LayoutOverflow::Hidden), "overflow-x:hidden");
    assert_eq!(oy, layout_overflow_to_u8(azul_css::props::layout::LayoutOverflow::Hidden), "overflow-y:hidden");
}

#[test]
fn test_roundtrip_text_color() {
    let s = styled_div_with_css(".t { color: #ff0000; }");
    let cc = s.css_property_cache.ptr.compact_cache.as_ref().unwrap();
    let text_color = cc.tier2b_text[2].text_color;
    // Encoded as RGBA u32: (r<<24) | (g<<16) | (b<<8) | a
    let r = (text_color >> 24) & 0xFF;
    let g = (text_color >> 16) & 0xFF;
    let b = (text_color >> 8) & 0xFF;
    let a = text_color & 0xFF;
    assert_eq!(r, 255, "red");
    assert_eq!(g, 0, "green");
    assert_eq!(b, 0, "blue");
    assert_eq!(a, 255, "alpha");
}

#[test]
fn test_roundtrip_border_color() {
    let s = styled_div_with_css(".t { border: 1px solid #00ff00; }");
    let cc = s.css_property_cache.ptr.compact_cache.as_ref().unwrap();
    let c = cc.tier2_cold[2].border_top_color;
    let r = (c >> 24) & 0xFF;
    let g = (c >> 16) & 0xFF;
    let b = (c >> 8) & 0xFF;
    assert_eq!(r, 0, "border-top-color red");
    assert_eq!(g, 255, "border-top-color green");
    assert_eq!(b, 0, "border-top-color blue");
}

#[test]
fn test_roundtrip_global_star_overrides_ua_margin() {
    // * { margin: 0; } should override body's UA margin: 8px
    let dom = Dom::create_html().with_child(Dom::create_body());
    let s = styled(dom, "* { margin: 0; }");
    // Body is node 1
    assert_eq!(read_compact_i16_as_px(&s, 1, "margin_top"), 0.0, "body margin-top should be 0 (overridden by *)");
    assert_eq!(read_compact_i16_as_px(&s, 1, "margin_left"), 0.0, "body margin-left should be 0 (overridden by *)");
}

#[test]
fn test_roundtrip_specific_overrides_global_star() {
    // body { padding: 20px; } should override * { padding: 0; }
    let dom = Dom::create_html().with_child(Dom::create_body());
    let s = styled(dom, "* { padding: 0; } body { padding: 20px; }");
    assert_eq!(read_compact_i16_as_px(&s, 1, "padding_top"), 20.0,
        "body padding-top should be 20px (body selector overrides *)");
    assert_eq!(read_compact_i16_as_px(&s, 1, "padding_bottom"), 20.0,
        "body padding-bottom should be 20px (body selector overrides *)");
}

#[test]
fn test_roundtrip_font_weight_default() {
    // No CSS → font-weight should default to Normal (0 in encoding)
    let s = styled_div_with_css(".t { width: 100px; }");
    assert_eq!(get_font_weight(&s, 2), 0, "font-weight should default to Normal (0)");
}

#[test]
fn test_roundtrip_visibility() {
    let s = styled_div_with_css(".t { visibility: hidden; }");
    let cc = s.css_property_cache.ptr.compact_cache.as_ref().unwrap();
    let t1 = cc.tier1_enums[2];
    let vis = ((t1 >> VISIBILITY_SHIFT) & VISIBILITY_MASK) as u8;
    assert_ne!(vis, 0, "visibility:hidden should not be 0 (visible)");
}

#[test]
fn test_roundtrip_z_index() {
    let s = styled_div_with_css(".t { position: relative; z-index: 5; }");
    let cc = s.css_property_cache.ptr.compact_cache.as_ref().unwrap();
    assert_eq!(cc.tier2_cold[2].z_index, 5, "z-index should be 5");
}
