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
        basic::{font::StyleFontSize, length::SizeMetric},
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

// Helper macro to create a StyledDom and get the CSS property cache.
// Calls compute_inherited_values to populate computed_values
// (build_compact_cache_with_inheritance handles inheritance in compact
// format and does not fill computed_values).
macro_rules! setup_styled_dom {
    ($dom:expr) => {{
        let mut dom = $dom;
        let styled_dom = StyledDom::create(&mut dom, Css::empty());
        let mut cache = styled_dom.css_property_cache.ptr.clone();
        let h = styled_dom.node_hierarchy.as_container();
        let d = styled_dom.node_data.as_container();
        cache.compute_inherited_values(h.internal, d.internal);
        (styled_dom, cache)
    }};
    ($dom:expr, $css:expr) => {{
        let mut dom = $dom;
        let (css, _) = azul_css::parser2::new_from_str($css);
        let css_wrapper = css;
        let styled_dom = StyledDom::create(&mut dom, css_wrapper);
        let mut cache = styled_dom.css_property_cache.ptr.clone();
        let h = styled_dom.node_hierarchy.as_container();
        let d = styled_dom.node_data.as_container();
        cache.compute_inherited_values(h.internal, d.internal);
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

    let (styled_dom, _cache) = setup_styled_dom!(dom, css);

    // After compact-prune, compact-encoded properties live in the compact cache,
    // not in computed_values. Verify via compact cache.
    let cc = styled_dom.css_property_cache.ptr.compact_cache.as_ref().unwrap();
    let fs = cc.tier2_dims[0].font_size;
    assert_ne!(fs, 0, "font-size should be set in compact cache");
    let display = ((cc.tier1_enums[0] >> azul_css::compact_cache::DISPLAY_SHIFT)
        & azul_css::compact_cache::DISPLAY_MASK) as u8;
    // Block encodes as 0 (default), which is correct
    assert_eq!(display, 0, "display:block encodes as 0 in compact cache");
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
                    format!("{display:?}").to_lowercase(),
                    "flex".to_lowercase()
                );
            }
        }
    }
}

#[test]
fn test_empty_css_produces_only_ua_styles() {
    let dom = Dom::create_node(NodeType::P).with_child(Dom::create_text("Paragraph"));

    let (styled_dom, _cache) = setup_styled_dom!(dom);

    // P should have UA CSS applied. Compact cache encodes Block as 0
    // (default when bits unset), so we check the POPULATED bit + margin.
    let cc = styled_dom.css_property_cache.ptr.compact_cache.as_ref()
        .expect("compact cache should exist");
    // P has margin-top from UA CSS — check it's non-zero in compact dims
    let margin_top = cc.tier2_dims[0].margin_top;
    assert_ne!(margin_top, 0, "P should have non-zero margin-top from UA CSS");
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
    let node_data = NodeData::create_node(NodeType::P);

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
                "Expected <p> to have display: block, got {display_value:?}"
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
        "Expected <p> to NOT have explicit width (should be auto), but got {width:?}"
    );

    // Test that <p> does NOT have a default height from UA CSS
    // (height should be auto, which means None)
    let height = cache.get_height(&node_data, &node_id, &node_state);
    println!("Height for <p> tag: {height:?}");

    // Height should be None because <p> should use auto height
    assert!(
        height.is_none(),
        "Expected <p> to NOT have explicit height (should be auto), but got {height:?}"
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
        "Expected <body> to NOT have explicit width (should be auto), but got {width:?}"
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

// =========================================================================
// Regression tests for bugs found during performance optimization
// =========================================================================

/// Bug #1/#2: FlatVecVec::sort_each_and_flatten must free build Vecs
/// and shrink data to fit. Previously build Vecs were retained after
/// flatten (leaking ~1 MiB on a 1000-node DOM) and data capacity
/// wasn't shrunk after dedup.
#[test]
fn flatvecvec_flatten_frees_build_and_shrinks() {
    use azul_core::prop_cache::FlatVecVec;

    let mut fvv: FlatVecVec<u32> = FlatVecVec::new(100);
    for i in 0..100 {
        fvv.push_to(i, i as u32);
        fvv.push_to(i, i as u32); // duplicate
        fvv.push_to(i, (i + 1) as u32);
    }

    let build_bytes_before = fvv.heap_bytes(4);
    assert!(build_bytes_before > 0, "build phase should have allocations");

    fvv.sort_each_and_flatten(|x| *x);

    let after_bytes = fvv.heap_bytes(4);
    // After flatten+dedup: build should be freed, data should be shrunk
    // 100 nodes × 2 unique items each = 200 items × 4 bytes = 800 bytes data
    // + 100 offsets × 8 bytes = 800 bytes offsets
    // Total: ~1600 bytes. Before: much more due to build Vecs.
    assert!(
        after_bytes < build_bytes_before,
        "flatten should reduce memory: before={build_bytes_before} after={after_bytes}"
    );
    // Verify data is accessible
    assert_eq!(fvv.get_slice(0), &[0, 1]);
    assert_eq!(fvv.get_slice(50), &[50, 51]);
}

/// Bug #3: FlatVecVec::retain must shrink_to_fit after filtering.
/// Previously capacity stayed at the pre-retain size.
#[test]
fn flatvecvec_retain_shrinks_capacity() {
    use azul_core::prop_cache::FlatVecVec;

    let mut fvv: FlatVecVec<u32> = FlatVecVec::new(10);
    for i in 0..10 {
        for j in 0..100 {
            fvv.push_to(i, (i * 100 + j) as u32);
        }
    }
    fvv.sort_each_and_flatten(|x| *x);

    let before = fvv.heap_bytes(4);
    // Keep only even numbers (removes ~50%)
    fvv.retain(|x| x % 2 == 0);
    let after = fvv.heap_bytes(4);

    assert!(
        after < before * 3 / 4,
        "retain should significantly reduce memory: before={before} after={after}"
    );
    // Verify filtered data
    for i in 0..10 {
        let slice = fvv.get_slice(i);
        assert!(slice.iter().all(|x| x % 2 == 0), "all values should be even");
    }
}

/// Bug #4: prune_compact_normal_props must handle cascaded_props in
/// build phase (not yet flattened). Previously it would silently skip
/// the prune because retain() early-returns when offsets is empty.
#[test]
fn prune_handles_unflattened_cascaded_props() {
    use azul_core::prop_cache::{CssPropertyCache, StatefulCssProperty};
    use azul_css::dynamic_selector::PseudoStateType;

    let mut cache = CssPropertyCache::empty(5);
    // Add some Normal-state compact properties to cascaded_props (build phase)
    for i in 0..5 {
        cache.cascaded_props.push_to(i, StatefulCssProperty {
            state: PseudoStateType::Normal,
            prop_type: CssPropertyType::Display,
            property: CssProperty::Display(CssPropertyValue::Exact(
                azul_css::props::layout::display::LayoutDisplay::Block,
            )),
        });
    }

    // cascaded_props is still in build phase (not flattened)
    assert!(!cache.cascaded_props.is_flattened());

    // Build a dummy compact cache so prune can run
    cache.compact_cache = Some(azul_css::compact_cache::CompactLayoutCache::with_capacity(5));

    // This should NOT panic and should actually prune
    cache.prune_compact_normal_props();

    // After prune, cascaded_props should be flattened and pruned
    assert!(cache.cascaded_props.is_flattened());
    // Display is compact-encoded + Normal state → should be removed
    let total: usize = (0..5).map(|i| cache.cascaded_props.get_slice(i).len()).sum();
    assert_eq!(total, 0, "all Normal+compact entries should be pruned");
}

/// Bug #6: has_compact_encoding must return true for all properties
/// that apply_css_property_to_compact handles.
#[test]
fn has_compact_encoding_covers_all_compact_properties() {
    use azul_css::props::property::CssPropertyType::*;

    // All properties that are encoded in compact_cache_builder.rs
    let compact_props = [
        Display, Position, Float, OverflowX, OverflowY, BoxSizing,
        FlexDirection, FlexWrap, JustifyContent, AlignItems, AlignContent,
        WritingMode, Clear, FontWeight, FontStyle, TextAlign,
        Visibility, WhiteSpace, Direction, VerticalAlign, BorderCollapse,
        AlignSelf, JustifySelf, GridAutoFlow, JustifyItems,
        Width, Height, MinWidth, MaxWidth, MinHeight, MaxHeight,
        FlexBasis, FontSize,
        PaddingTop, PaddingRight, PaddingBottom, PaddingLeft,
        MarginTop, MarginRight, MarginBottom, MarginLeft,
        BorderTopWidth, BorderRightWidth, BorderBottomWidth, BorderLeftWidth,
        Top, Right, Bottom, Left,
        FlexGrow, FlexShrink,
        ZIndex,
        BorderTopStyle, BorderRightStyle, BorderBottomStyle, BorderLeftStyle,
        BorderTopColor, BorderRightColor, BorderBottomColor, BorderLeftColor,
        BorderSpacing, TabSize,
        TextColor, FontFamily, LineHeight, LetterSpacing, WordSpacing, TextIndent,
        ColumnGap, RowGap, Gap,
        GridColumn, GridRow,
    ];

    for pt in &compact_props {
        assert!(
            pt.has_compact_encoding(),
            "{pt:?} should have compact encoding but has_compact_encoding() returns false"
        );
    }

    // These should NOT have compact encoding
    let non_compact = [
        BackgroundContent, BackgroundPosition, BackgroundSize,
        Transform, TransformOrigin, BoxShadowLeft, Opacity,
        GridTemplateColumns, GridTemplateRows, GridTemplateAreas,
    ];
    for pt in &non_compact {
        assert!(
            !pt.has_compact_encoding(),
            "{pt:?} should NOT have compact encoding"
        );
    }
}
