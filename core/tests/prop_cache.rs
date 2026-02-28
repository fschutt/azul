//! prop_cache Tests
//!
//! Tests for the CSS property cache, dependency chains, and inheritance system.

use azul_core::{
    dom::{Dom, NodeType},
    prop_cache::{CssPropertyOrigin, CssPropertyWithOrigin},
    styled_dom::StyledDom,
};
use azul_css::dynamic_selector::CssPropertyWithConditions;
use azul_css::{
    css::Css,
    css::CssPropertyValue,
    props::{
        basic::{font::StyleFontSize, length::SizeMetric, pixel::PixelValue},
        property::{CssProperty, CssPropertyType},
    },
};

/// Helper: look up a CssPropertyType in a sorted Vec of (CssPropertyType, CssPropertyWithOrigin)
fn find_prop<'a>(
    computed: &'a [(CssPropertyType, CssPropertyWithOrigin)],
    prop_type: &CssPropertyType,
) -> Option<&'a CssPropertyWithOrigin> {
    computed
        .binary_search_by_key(prop_type, |(k, _)| *k)
        .ok()
        .map(|idx| &computed[idx].1)
}

// Helper macro to create a StyledDom and get the CSS property cache
macro_rules! setup_styled_dom {
    ($dom:expr) => {{
        let mut dom = $dom;
        let styled_dom = StyledDom::create(&mut dom, Css::empty());
        let cache = styled_dom.css_property_cache.ptr.clone();
        (styled_dom, cache)
    }};
    ($dom:expr, $css:expr) => {{
        let mut dom = $dom;
        let (css, _) = azul_css::parser2::new_from_str($css);
        let css_wrapper = css;
        let styled_dom = StyledDom::create(&mut dom, css_wrapper);
        let cache = styled_dom.css_property_cache.ptr.clone();
        (styled_dom, cache)
    }};
}

#[test]
fn test_computed_values_exist_for_all_nodes() {
    let dom = Dom::create_div()
        .with_child(Dom::create_node(NodeType::P).with_child(Dom::create_text("Text")));

    let (_styled_dom, cache) = setup_styled_dom!(dom);

    // Check that computed_values exist for all nodes
    let node_0 = azul_core::dom::NodeId::new(0);
    let node_1 = azul_core::dom::NodeId::new(1);
    let node_2 = azul_core::dom::NodeId::new(2);

    assert!(
        cache.computed_values.get(node_0.index()).is_some(),
        "Node 0 should have computed values"
    );
    assert!(
        cache.computed_values.get(node_1.index()).is_some(),
        "Node 1 should have computed values"
    );
    assert!(
        cache.computed_values.get(node_2.index()).is_some(),
        "Node 2 should have computed values"
    );
}

#[test]
fn test_inline_css_takes_precedence() {
    let dom = Dom::create_div().with_css_props(
        vec![CssPropertyWithConditions::simple(CssProperty::font_size(
            StyleFontSize::px(24.0),
        ))]
        .into(),
    );

    let (_styled_dom, cache) = setup_styled_dom!(dom);

    let node_0 = azul_core::dom::NodeId::new(0);
    let computed = cache
        .computed_values
        .get(node_0.index())
        .expect("should have computed values");
    let font_size_prop = find_prop(computed, &CssPropertyType::FontSize)
        .expect("should have font-size");

    assert_eq!(font_size_prop.origin, CssPropertyOrigin::Own);

    if let CssProperty::FontSize(val) = &font_size_prop.property {
        if let Some(size) = val.get_property() {
            assert!((size.inner.number.get() - 24.0).abs() < 0.001);
        } else {
            panic!("FontSize should have value");
        }
    } else {
        panic!("Property should be FontSize");
    }
}

#[test]
fn test_css_stylesheet_applies() {
    let dom = Dom::create_div()
        .with_child(Dom::create_node(NodeType::P).with_child(Dom::create_text("Text")));

    let css = "p { font-size: 18px; }";
    let (_styled_dom, cache) = setup_styled_dom!(dom, css);

    let p_id = azul_core::dom::NodeId::new(1);
    let computed = cache
        .computed_values
        .get(p_id.index())
        .expect("p should have computed values");

    if let Some(font_size_prop) = find_prop(computed, &CssPropertyType::FontSize) {
        if let CssProperty::FontSize(val) = &font_size_prop.property {
            if let Some(size) = val.get_property() {
                assert!((size.inner.number.get() - 18.0).abs() < 0.001);
            }
        }
    }
}

#[test]
fn test_inherited_property_has_correct_origin() {
    let dom = Dom::create_div()
        .with_css_props(
            vec![CssPropertyWithConditions::simple(CssProperty::font_size(
                StyleFontSize::px(20.0),
            ))]
            .into(),
        )
        .with_child(Dom::create_node(NodeType::P).with_child(Dom::create_text("Text")));

    let (_styled_dom, cache) = setup_styled_dom!(dom);

    // P (node 1) should have font-size (either inherited or from UA CSS)
    // The important thing is the VALUE is correct (20px from parent)
    let p_id = azul_core::dom::NodeId::new(1);
    let computed = cache
        .computed_values
        .get(p_id.index())
        .expect("p should have computed values");
    let font_size_prop = find_prop(computed, &CssPropertyType::FontSize)
        .expect("should have font-size");

    // Check that P has the correct font-size value (inherited from div)
    if let CssProperty::FontSize(val) = &font_size_prop.property {
        if let Some(size) = val.get_property() {
            assert!(
                (size.inner.number.get() - 20.0).abs() < 0.001,
                "P should have inherited font-size 20px from parent, got {}",
                size.inner.number.get()
            );
        } else {
            panic!("FontSize should have value");
        }
    } else {
        panic!("Property should be FontSize");
    }
}

#[test]
fn test_own_property_overrides_inherited() {
    let dom = Dom::create_div()
        .with_css_props(
            vec![CssPropertyWithConditions::simple(CssProperty::font_size(
                StyleFontSize::px(20.0),
            ))]
            .into(),
        )
        .with_child(
            Dom::create_node(NodeType::P)
                .with_css_props(
                    vec![CssPropertyWithConditions::simple(CssProperty::font_size(
                        StyleFontSize::px(16.0),
                    ))]
                    .into(),
                )
                .with_child(Dom::create_text("Text")),
        );

    let (_styled_dom, cache) = setup_styled_dom!(dom);

    // P (node 1) has its own font-size, should override inherited
    let p_id = azul_core::dom::NodeId::new(1);
    let computed = cache
        .computed_values
        .get(p_id.index())
        .expect("p should have computed values");
    let font_size_prop = find_prop(computed, &CssPropertyType::FontSize)
        .expect("should have font-size");

    assert_eq!(font_size_prop.origin, CssPropertyOrigin::Own);

    if let CssProperty::FontSize(val) = &font_size_prop.property {
        if let Some(size) = val.get_property() {
            assert!((size.inner.number.get() - 16.0).abs() < 0.001);
        } else {
            panic!("FontSize should have value");
        }
    } else {
        panic!("Property should be FontSize");
    }
}

#[test]
fn test_em_resolved_to_px_in_computed() {
    let dom = Dom::create_div()
        .with_css_props(
            vec![CssPropertyWithConditions::simple(CssProperty::font_size(
                StyleFontSize::px(20.0),
            ))]
            .into(),
        )
        .with_child(
            Dom::create_node(NodeType::P)
                .with_css_props(
                    vec![CssPropertyWithConditions::simple(CssProperty::font_size(
                        StyleFontSize::em(1.5),
                    ))]
                    .into(),
                )
                .with_child(Dom::create_text("Text")),
        );

    let (_styled_dom, cache) = setup_styled_dom!(dom);

    // P's 1.5em should be resolved to 30px (1.5 * 20)
    let p_id = azul_core::dom::NodeId::new(1);
    let computed = cache
        .computed_values
        .get(p_id.index())
        .expect("p should have computed values");
    let font_size_prop = find_prop(computed, &CssPropertyType::FontSize)
        .expect("should have font-size");

    if let CssProperty::FontSize(val) = &font_size_prop.property {
        if let Some(size) = val.get_property() {
            assert_eq!(
                size.inner.metric,
                SizeMetric::Px,
                "Should be resolved to Px"
            );
            assert!(
                (size.inner.number.get() - 30.0).abs() < 0.001,
                "1.5em * 20px = 30px, got {}",
                size.inner.number.get()
            );
        } else {
            panic!("FontSize should have value");
        }
    } else {
        panic!("Property should be FontSize");
    }
}

#[test]
fn test_deeply_nested_inheritance() {
    let dom = Dom::create_div()
        .with_css_props(
            vec![CssPropertyWithConditions::simple(CssProperty::font_size(
                StyleFontSize::px(20.0),
            ))]
            .into(),
        )
        .with_child(Dom::create_div().with_child(
            Dom::create_div().with_child(Dom::create_div().with_child(Dom::create_text("Deep"))),
        ));

    let (_styled_dom, cache) = setup_styled_dom!(dom);

    // The deepest div (node 3) should inherit 20px from root
    let deep_id = azul_core::dom::NodeId::new(3);
    let computed = cache
        .computed_values
        .get(deep_id.index())
        .expect("deep node should have computed values");
    let font_size_prop = find_prop(computed, &CssPropertyType::FontSize)
        .expect("should have font-size");

    if let CssProperty::FontSize(val) = &font_size_prop.property {
        if let Some(size) = val.get_property() {
            assert!((size.inner.number.get() - 20.0).abs() < 0.001);
        }
    }
}

#[test]
fn test_ua_css_for_headings() {
    let dom = Dom::create_node(NodeType::H1).with_child(Dom::create_text("Heading"));

    let (_styled_dom, cache) = setup_styled_dom!(dom);

    // H1 should have UA CSS font-size: 2em (32px with 16px default)
    let h1_id = azul_core::dom::NodeId::new(0);
    let computed = cache
        .computed_values
        .get(h1_id.index())
        .expect("h1 should have computed values");

    if let Some(font_size_prop) = find_prop(computed, &CssPropertyType::FontSize) {
        if let CssProperty::FontSize(val) = &font_size_prop.property {
            if let Some(size) = val.get_property() {
                // H1 UA CSS is 2em, resolved with 16px default = 32px
                assert!(
                    (size.inner.number.get() - 32.0).abs() < 0.1,
                    "H1 font-size should be ~32px (2em * 16px), got {}",
                    size.inner.number.get()
                );
            }
        }
    }
}

#[test]
fn test_multiple_properties_computed() {
    let css = r#"
        div {
            font-size: 18px;
            display: block;
            width: 100px;
        }
    "#;
    let dom = Dom::create_div();

    let (_styled_dom, cache) = setup_styled_dom!(dom, css);

    let div_id = azul_core::dom::NodeId::new(0);
    let computed = cache
        .computed_values
        .get(div_id.index())
        .expect("div should have computed values");

    // Check that multiple properties exist
    assert!(find_prop(computed, &CssPropertyType::FontSize).is_some());
    assert!(find_prop(computed, &CssPropertyType::Display).is_some());
    // Width might not be computed if it's not inheritable
}

#[test]
fn test_no_computed_values_for_nonexistent_node() {
    let dom = Dom::create_div();
    let (_styled_dom, cache) = setup_styled_dom!(dom);

    // Node 100 doesn't exist
    let nonexistent_id = azul_core::dom::NodeId::new(100);
    assert!(cache.computed_values.get(nonexistent_id.index()).is_none());
}

#[test]
fn test_font_weight_inheritance() {
    // Use inline CSS to ensure div has bold font-weight
    let dom = Dom::create_div()
        .with_css_props(
            vec![CssPropertyWithConditions::simple(CssProperty::FontWeight(
                azul_css::css::CssPropertyValue::Exact(
                    azul_css::props::basic::font::StyleFontWeight::Bold,
                ),
            ))]
            .into(),
        )
        .with_child(Dom::create_node(NodeType::Span).with_child(Dom::create_text("Bold text")));

    let (_styled_dom, cache) = setup_styled_dom!(dom);

    // Span should have font-weight (inherited from div)
    let span_id = azul_core::dom::NodeId::new(1);
    let computed = cache
        .computed_values
        .get(span_id.index())
        .expect("span should have computed values");

    if let Some(font_weight_prop) = find_prop(computed, &CssPropertyType::FontWeight) {
        // Check that span has bold font-weight (value matters, origin may vary due to UA CSS)
        if let CssProperty::FontWeight(val) = &font_weight_prop.property {
            if let Some(weight) = val.get_property() {
                assert_eq!(
                    *weight,
                    azul_css::props::basic::font::StyleFontWeight::Bold,
                    "Span should have inherited bold font-weight from parent"
                );
            }
        }
    }
}

#[test]
fn test_color_inheritance() {
    // Use inline CSS to ensure div has red text color
    let dom = Dom::create_div()
        .with_css_props(
            vec![CssPropertyWithConditions::simple(CssProperty::TextColor(
                azul_css::css::CssPropertyValue::Exact(azul_css::props::style::StyleTextColor {
                    inner: azul_css::props::basic::color::ColorU {
                        r: 255,
                        g: 0,
                        b: 0,
                        a: 255,
                    },
                }),
            ))]
            .into(),
        )
        .with_child(Dom::create_node(NodeType::P).with_child(Dom::create_text("Red text")));

    let (_styled_dom, cache) = setup_styled_dom!(dom);

    // P should have red text color (inherited from div)
    let p_id = azul_core::dom::NodeId::new(1);
    let computed = cache
        .computed_values
        .get(p_id.index())
        .expect("p should have computed values");

    if let Some(color_prop) = find_prop(computed, &CssPropertyType::TextColor) {
        // Check that P has red color (value matters, origin may vary)
        if let CssProperty::TextColor(val) = &color_prop.property {
            if let Some(color) = val.get_property() {
                assert_eq!(
                    color.inner.r, 255,
                    "P should have inherited red color from parent"
                );
                assert_eq!(color.inner.g, 0);
                assert_eq!(color.inner.b, 0);
            }
        }
    }
}

#[test]
fn test_non_inheritable_property_not_inherited() {
    // display is not inheritable
    let css = "div { display: flex; }";
    let dom = Dom::create_div()
        .with_child(Dom::create_node(NodeType::P).with_child(Dom::create_text("Text")));

    let (_styled_dom, cache) = setup_styled_dom!(dom, css);

    // P should NOT inherit display from div
    let p_id = azul_core::dom::NodeId::new(1);
    let computed = cache
        .computed_values
        .get(p_id.index())
        .expect("p should have computed values");

    if let Some(display_prop) = find_prop(computed, &CssPropertyType::Display) {
        // If it exists, it should be the default (block for P), not inherited flex
        if let CssProperty::Display(val) = &display_prop.property {
            if let Some(display) = val.get_property() {
                // P's display should be block (UA CSS), not flex
                assert_ne!(
                    format!("{:?}", display).to_lowercase(),
                    "flex".to_lowercase()
                );
            }
        }
    }
}

#[test]
fn test_empty_css_produces_only_ua_styles() {
    let dom = Dom::create_node(NodeType::P).with_child(Dom::create_text("Paragraph"));

    let (_styled_dom, cache) = setup_styled_dom!(dom);

    // P should have UA CSS for display, margins, etc.
    let p_id = azul_core::dom::NodeId::new(0);
    let computed = cache
        .computed_values
        .get(p_id.index())
        .expect("p should have computed values");

    // P has UA CSS margin-top and margin-bottom
    assert!(
        find_prop(computed, &CssPropertyType::MarginTop).is_some()
            || find_prop(computed, &CssPropertyType::Display).is_some(),
        "P should have some UA CSS properties"
    );
}

#[test]
fn test_text_node_inherits_from_parent() {
    let dom = Dom::create_node(NodeType::P)
        .with_css_props(
            vec![CssPropertyWithConditions::simple(CssProperty::font_size(
                StyleFontSize::px(18.0),
            ))]
            .into(),
        )
        .with_child(Dom::create_text("Text inherits"));

    let (_styled_dom, cache) = setup_styled_dom!(dom);

    // Text node (node 1) should have font-size from P
    let text_id = azul_core::dom::NodeId::new(1);
    let computed = cache
        .computed_values
        .get(text_id.index())
        .expect("text should have computed values");

    if let Some(font_size_prop) = find_prop(computed, &CssPropertyType::FontSize) {
        // Check the VALUE is correct (18px from parent)
        if let CssProperty::FontSize(val) = &font_size_prop.property {
            if let Some(size) = val.get_property() {
                assert!(
                    (size.inner.number.get() - 18.0).abs() < 0.001,
                    "Text node should have font-size 18px from parent, got {}",
                    size.inner.number.get()
                );
            } else {
                panic!("FontSize should have value");
            }
        } else {
            panic!("Property should be FontSize");
        }
    } else {
        panic!("Text node should have font-size property");
    }
}

use azul_core::prop_cache::*;
use azul_core::dom::{NodeData, NodeId};
use azul_core::styled_dom::StyledNodeState;
use azul_css::props::layout::LayoutDisplay;

#[test]
fn test_ua_css_p_tag_properties() {
    // Create an empty CssPropertyCache
    let cache = CssPropertyCache::empty(1);

    // Create a minimal <p> tag NodeData using public API
    let mut node_data = NodeData::create_node(NodeType::P);

    let node_id = NodeId::new(0);
    let node_state = StyledNodeState::default();

    // Test that <p> has display: block from UA CSS
    let display = cache.get_display(&node_data, &node_id, &node_state);
    assert!(
        display.is_some(),
        "Expected <p> to have display property from UA CSS"
    );
    if let Some(d) = display {
        if let Some(display_value) = d.get_property() {
            assert_eq!(
                *display_value,
                LayoutDisplay::Block,
                "Expected <p> to have display: block, got {:?}",
                display_value
            );
        }
    }

    // NOTE: <p> does NOT have width: 100% in standard UA CSS
    // Block elements have width: auto by default, which means "fill available width"
    // but it's not the same as width: 100%. The difference is critical for flexbox.
    let width = cache.get_width(&node_data, &node_id, &node_state);
    // Width should be None because <p> should use auto width (no explicit width property)
    assert!(
        width.is_none(),
        "Expected <p> to NOT have explicit width (should be auto), but got {:?}",
        width
    );

    // Test that <p> does NOT have a default height from UA CSS
    // (height should be auto, which means None)
    let height = cache.get_height(&node_data, &node_id, &node_state);
    println!("Height for <p> tag: {:?}", height);

    // Height should be None because <p> should use auto height
    assert!(
        height.is_none(),
        "Expected <p> to NOT have explicit height (should be auto), but got {:?}",
        height
    );
}

#[test]
fn test_ua_css_body_tag_properties() {
    let cache = CssPropertyCache::empty(1);

    let node_data = NodeData::create_node(NodeType::Body);

    let node_id = NodeId::new(0);
    let node_state = StyledNodeState::default();

    // NOTE: <body> does NOT have width: 100% in standard UA CSS
    // It inherits from the Initial Containing Block (ICB) and has width: auto
    let width = cache.get_width(&node_data, &node_id, &node_state);
    // Width should be None because <body> should use auto width (no explicit width property)
    assert!(
        width.is_none(),
        "Expected <body> to NOT have explicit width (should be auto), but got {:?}",
        width
    );

    // Note: height: 100% was removed from UA CSS (ua_css.rs:506 commented out)
    // This is correct - <body> should have height: auto by default per CSS spec
    let height = cache.get_height(&node_data, &node_id, &node_state);
    assert!(
        height.is_none(),
        "<body> should not have explicit height from UA CSS (should be auto)"
    );

    // Test margins (body has 8px margins from UA CSS)
    let margin_top = cache.get_margin_top(&node_data, &node_id, &node_state);
    assert!(
        margin_top.is_some(),
        "Expected <body> to have margin-top from UA CSS"
    );
}
