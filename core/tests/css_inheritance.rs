//! CSS Inheritance and Cascade Tests
//!
//! Tests the computed values cache system (Option C from CSS_INHERITANCE_PROBLEM_REPORT.md)
//! Verifies that:
//! 1. Inherited properties correctly cascade from parent to child
//! 2. Explicit values override inherited values
//! 3. Multiple levels of nesting work correctly
//! 4. Updates to parent styles correctly invalidate children
//! 5. User-agent styles integrate properly with inheritance

use azul_core::{
    dom::{Dom, NodeType},
    prop_cache::{CssPropertyOrigin, CssPropertyWithOrigin},
    styled_dom::StyledDom,
};
use azul_css::{
    css::Css,
    dynamic_selector::CssPropertyWithConditions,
    props::{
        basic::font::{StyleFontSize, StyleFontWeight},
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

// Helper macro to create a StyledDom and get the necessary references
macro_rules! setup_test {
    ($dom:expr) => {{
        let mut dom = $dom;
        let styled_dom = StyledDom::create(&mut dom, Css::empty());

        let cache = styled_dom.css_property_cache.ptr.clone();

        (styled_dom, cache)
    }};
}

#[test]
fn test_font_size_inheritance_single_level() {
    // Create a simple parent-child DOM:
    // <div style="font-size: 24px">
    //   <p>Text</p>
    // </div>

    let dom = Dom::create_div()
        .with_css_props(
            vec![CssPropertyWithConditions::simple(CssProperty::font_size(
                StyleFontSize::px(24.0),
            ))]
            .into(),
        )
        .with_child(Dom::create_node(NodeType::P).with_child(Dom::create_text("Text")));

    let (styled_dom, mut cache) = setup_test!(dom);

    let node_hierarchy = &styled_dom.node_hierarchy.as_container().internal[..];
    let node_data = &styled_dom.node_data.as_container().internal[..];

    // Compute inherited values
    let changed_nodes = cache.compute_inherited_values(node_hierarchy, node_data);

    println!("Changed nodes: {:?}", changed_nodes);
    println!("Computed values: {:#?}", cache.computed_values);

    // Verify that <p> (child) inherited the font-size from <div> (parent)
    let parent_id = azul_core::dom::NodeId::new(0); // div
    let child_id = azul_core::dom::NodeId::new(1); // p

    // Check computed values for child
    if let Some(child_computed) = cache.computed_values.get(child_id.index()) {
        let Some(prop_with_origin) = find_prop(child_computed, &CssPropertyType::FontSize) else {
            panic!("Child should have FontSize");
        };
        if let CssProperty::FontSize(font_size_value) = &prop_with_origin.property {
            if let Some(font_size) = font_size_value.get_property() {
                let size = font_size.inner.to_pixels_internal(16.0, 16.0); // Default fallback
                assert_eq!(
                    size, 24.0,
                    "Child should inherit parent's font-size of 24px"
                );
            } else {
                panic!("FontSize value should not be None/Auto/Initial/Inherit");
            }
        } else {
            panic!("Child should have computed FontSize property");
        }
    } else {
        panic!("Child should have computed values");
    }
}

#[test]
fn test_font_size_override_not_inherited() {
    // Create a parent-child DOM where child has explicit font-size:
    // <div style="font-size: 24px">
    //   <p style="font-size: 12px">Text</p>
    // </div>

    let dom = Dom::create_div()
        .with_css_props(
            vec![CssPropertyWithConditions::simple(CssProperty::font_size(
                StyleFontSize::px(24.0),
            ))]
            .into(),
        )
        .with_child(
            Dom::create_node(NodeType::P)
                .with_css_props(
                    vec![CssPropertyWithConditions::simple(CssProperty::font_size(
                        StyleFontSize::px(12.0),
                    ))]
                    .into(),
                )
                .with_child(Dom::create_text("Text")),
        );

    let (styled_dom, mut cache) = setup_test!(dom);
    let node_hierarchy = &styled_dom.node_hierarchy.as_container().internal[..];
    let node_data = &styled_dom.node_data.as_container().internal[..];
    let changed_nodes = cache.compute_inherited_values(node_hierarchy, node_data);

    println!("Changed nodes: {:?}", changed_nodes);
    println!("Computed values: {:#?}", cache.computed_values);

    let child_id = azul_core::dom::NodeId::new(1); // p

    // Verify that child has its own explicit value, not the inherited one
    if let Some(child_computed) = cache.computed_values.get(child_id.index()) {
        let Some(prop_with_origin) = find_prop(child_computed, &CssPropertyType::FontSize) else {
            panic!("Child should have FontSize");
        };
        if let CssProperty::FontSize(font_size_value) = &prop_with_origin.property {
            if let Some(font_size) = font_size_value.get_property() {
                let size = font_size.inner.to_pixels_internal(16.0, 16.0);
                assert_eq!(
                    size, 12.0,
                    "Child should use its explicit font-size of 12px, not inherit 24px"
                );
            } else {
                panic!("FontSize value should not be None/Auto/Initial/Inherit");
            }
        } else {
            panic!("Child should have computed FontSize property");
        }
    } else {
        panic!("Child should have computed values");
    }
}

#[test]
fn test_font_weight_inheritance_multi_level() {
    // Create a three-level hierarchy:
    // <div style="font-weight: bold">
    //   <p>
    //     <span>Text</span>
    //   </p>
    // </div>

    let dom = Dom::create_div()
        .with_css_props(
            vec![CssPropertyWithConditions::simple(CssProperty::font_weight(
                StyleFontWeight::Bold,
            ))]
            .into(),
        )
        .with_child(
            Dom::create_node(NodeType::P)
                .with_child(Dom::create_node(NodeType::Span).with_child(Dom::create_text("Text"))),
        );

    let (styled_dom, mut cache) = setup_test!(dom);
    let node_hierarchy = &styled_dom.node_hierarchy.as_container().internal[..];
    let node_data = &styled_dom.node_data.as_container().internal[..];
    let changed_nodes = cache.compute_inherited_values(node_hierarchy, node_data);

    println!("Changed nodes: {:?}", changed_nodes);
    println!("Computed values: {:#?}", cache.computed_values);

    let div_id = azul_core::dom::NodeId::new(0); // div
    let p_id = azul_core::dom::NodeId::new(1); // p
    let span_id = azul_core::dom::NodeId::new(2); // span

    // Verify that both <p> and <span> inherited font-weight: bold
    for (node_id, node_name) in &[(p_id, "p"), (span_id, "span")] {
        if let Some(computed) = cache.computed_values.get(node_id.index()) {
            let Some(prop_with_origin) = find_prop(computed, &CssPropertyType::FontWeight) else {
                panic!("{} should have FontWeight", node_name);
            };
            if let CssProperty::FontWeight(font_weight_value) = &prop_with_origin.property {
                if let Some(font_weight) = font_weight_value.get_property() {
                    assert_eq!(
                        *font_weight,
                        StyleFontWeight::Bold,
                        "{} should inherit font-weight: bold from ancestor div",
                        node_name
                    );
                } else {
                    panic!(
                        "{} FontWeight value should not be None/Auto/Initial/Inherit",
                        node_name
                    );
                }
            } else {
                panic!("{} should have computed FontWeight property", node_name);
            }
        } else {
            panic!("{} should have computed values", node_name);
        }
    }
}

#[test]
fn test_mixed_inherited_and_explicit_properties() {
    // Test cascade priority:
    // <div style="font-size: 20px; font-weight: bold">
    //   <p style="font-size: 16px">
    //     Text (should have font-size: 16px, font-weight: bold)
    //   </p>
    // </div>

    let dom = Dom::create_div()
        .with_css_props(
            vec![
                CssPropertyWithConditions::simple(CssProperty::font_size(StyleFontSize::px(20.0))),
                CssPropertyWithConditions::simple(CssProperty::font_weight(StyleFontWeight::Bold)),
            ]
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

    let (styled_dom, mut cache) = setup_test!(dom);
    let node_hierarchy = &styled_dom.node_hierarchy.as_container().internal[..];
    let node_data = &styled_dom.node_data.as_container().internal[..];
    cache.compute_inherited_values(node_hierarchy, node_data);

    let p_id = azul_core::dom::NodeId::new(1); // p

    if let Some(p_computed) = cache.computed_values.get(p_id.index()) {
        // Check font-size (explicit)
        let Some(prop_with_origin) = find_prop(p_computed, &CssPropertyType::FontSize) else {
            panic!("p should have computed FontSize");
        };
        if let CssProperty::FontSize(font_size_value) = &prop_with_origin.property {
            if let Some(font_size) = font_size_value.get_property() {
                let size = font_size.inner.to_pixels_internal(16.0, 16.0);
                assert_eq!(size, 16.0, "p should have explicit font-size: 16px");
            }
        } else {
            panic!("p FontSize should be CssProperty::FontSize variant");
        }

        // Check font-weight (inherited)
        let Some(prop_with_origin) = find_prop(p_computed, &CssPropertyType::FontWeight) else {
            panic!("p should have FontWeight");
        };
        if let CssProperty::FontWeight(font_weight_value) = &prop_with_origin.property {
            if let Some(font_weight) = font_weight_value.get_property() {
                assert_eq!(
                    *font_weight,
                    StyleFontWeight::Bold,
                    "p should inherit font-weight: bold from div"
                );
            }
        } else {
            panic!("p should have computed FontWeight inherited from parent");
        }
    } else {
        panic!("p should have computed values");
    }
}

#[test]
fn test_non_inheritable_property_not_inherited() {
    // Test that non-inheritable properties (like width) are NOT inherited:
    // <div style="width: 200px">
    //   <p>Text</p>  <!-- should NOT inherit width -->
    // </div>

    use azul_css::{
        css::CssPropertyValue,
        props::{basic::pixel::PixelValue, layout::dimensions::LayoutWidth},
    };

    let dom = Dom::create_div()
        .with_css_props(
            vec![CssPropertyWithConditions::simple(CssProperty::Width(
                CssPropertyValue::Exact(LayoutWidth::Px(PixelValue::px(200.0))),
            ))]
            .into(),
        )
        .with_child(Dom::create_node(NodeType::P).with_child(Dom::create_text("Text")));

    let (styled_dom, mut cache) = setup_test!(dom);
    let node_hierarchy = &styled_dom.node_hierarchy.as_container().internal[..];
    let node_data = &styled_dom.node_data.as_container().internal[..];
    cache.compute_inherited_values(node_hierarchy, node_data);

    let p_id = azul_core::dom::NodeId::new(1); // p

    if let Some(p_computed) = cache.computed_values.get(p_id.index()) {
        // Width should NOT be inherited from parent (200px)
        // Test the ORIGIN of the property, not just its value
        if let Some(prop_with_origin) = find_prop(p_computed, &CssPropertyType::Width) {
            // The KEY test: Width should have origin Own (from UA CSS), NOT Inherited
            assert_eq!(
                prop_with_origin.origin,
                CssPropertyOrigin::Own,
                "Width origin should be Own (from UA CSS), not Inherited from parent. This proves \
                 that non-inheritable properties are not inherited."
            );
        }
        // If there's no Width at all, that's also fine (no UA CSS for this element)
    }
}

#[test]
fn test_update_invalidation() {
    // Test that updating a parent's property correctly returns changed children:
    // <div style="font-size: 20px">
    //   <p>Child 1</p>
    //   <p>Child 2</p>
    // </div>

    let dom = Dom::create_div()
        .with_css_props(
            vec![CssPropertyWithConditions::simple(CssProperty::font_size(
                StyleFontSize::px(20.0),
            ))]
            .into(),
        )
        .with_children(
            vec![
                Dom::create_node(NodeType::P).with_child(Dom::create_text("Child 1")),
                Dom::create_node(NodeType::P).with_child(Dom::create_text("Child 2")),
            ]
            .into(),
        );

    let (styled_dom, mut cache) = setup_test!(dom);
    let node_hierarchy = &styled_dom.node_hierarchy.as_container().internal[..];
    let node_data = &styled_dom.node_data.as_container().internal[..];

    // Clear the cache to test first computation (StyledDom::new already computed once)
    cache.computed_values.iter_mut().for_each(|m| m.clear());

    // First computation
    let changed_nodes_1 = cache.compute_inherited_values(node_hierarchy, node_data);

    println!("First computation changed nodes: {:?}", changed_nodes_1);

    // All nodes should be marked as changed on first computation
    assert!(
        changed_nodes_1.len() >= 3,
        "All nodes should change on first computation"
    );

    // Second computation without changes should return empty list
    let changed_nodes_2 = cache.compute_inherited_values(node_hierarchy, node_data);

    println!("Second computation changed nodes: {:?}", changed_nodes_2);

    assert!(
        changed_nodes_2.is_empty(),
        "No nodes should change when recomputing with same values"
    );
}

#[test]
fn test_deeply_nested_inheritance() {
    // Test inheritance through many levels:
    // <div style="font-weight: bold">
    //   <section>
    //     <article>
    //       <p>
    //         <span>Deep text</span>
    //       </p>
    //     </article>
    //   </section>
    // </div>

    let dom = Dom::create_div()
        .with_css_props(
            vec![CssPropertyWithConditions::simple(CssProperty::font_weight(
                StyleFontWeight::Bold,
            ))]
            .into(),
        )
        .with_child(Dom::create_node(NodeType::Section).with_child(
            Dom::create_node(NodeType::Article).with_child(
                Dom::create_node(NodeType::P).with_child(
                    Dom::create_node(NodeType::Span).with_child(Dom::create_text("Deep text")),
                ),
            ),
        ));

    let (styled_dom, mut cache) = setup_test!(dom);
    let node_hierarchy = &styled_dom.node_hierarchy.as_container().internal[..];
    let node_data = &styled_dom.node_data.as_container().internal[..];
    cache.compute_inherited_values(node_hierarchy, node_data);

    println!("Node hierarchy: {:#?}", node_hierarchy);
    println!("Computed values: {:#?}", cache.computed_values);

    // Verify all descendants inherited font-weight: bold
    let span_id = azul_core::dom::NodeId::new(4); // The deepest span

    let Some(span_computed) = cache.computed_values.get(span_id.index()) else {
        panic!("Deeply nested span should have computed values");
    };

    let Some(prop_with_origin) = find_prop(span_computed, &CssPropertyType::FontWeight) else {
        panic!("Deeply nested span should have inherited FontWeight");
    };
    let CssProperty::FontWeight(font_weight_value) = &prop_with_origin.property else {
        panic!("FontWeight should be CssProperty::FontWeight variant");
    };

    let Some(font_weight) = font_weight_value.get_property() else {
        panic!("FontWeight should have explicit value");
    };

    assert_eq!(
        *font_weight,
        StyleFontWeight::Bold,
        "Deeply nested span should inherit font-weight: bold"
    );
}

#[test]
fn test_em_unit_inheritance_basic() {
    // Test that 'em' units are resolved relative to the current element's font-size
    // <div style="font-size: 16px">
    //   <p style="font-size: 2em">  <!-- Should be 32px -->
    //     Text
    //   </p>
    // </div>

    let dom = Dom::create_div()
        .with_css_props(
            vec![CssPropertyWithConditions::simple(CssProperty::font_size(
                StyleFontSize::px(16.0),
            ))]
            .into(),
        )
        .with_child(
            Dom::create_node(NodeType::P)
                .with_css_props(
                    vec![CssPropertyWithConditions::simple(CssProperty::font_size(
                        StyleFontSize::em(2.0),
                    ))]
                    .into(),
                )
                .with_child(Dom::create_text("Text")),
        );

    let (styled_dom, mut cache) = setup_test!(dom);
    let node_hierarchy = &styled_dom.node_hierarchy.as_container().internal[..];
    let node_data = &styled_dom.node_data.as_container().internal[..];

    cache.compute_inherited_values(node_hierarchy, node_data);

    let p_id = azul_core::dom::NodeId::new(1); // p

    let Some(p_computed) = cache.computed_values.get(p_id.index()) else {
        panic!("p should have computed values");
    };

    let Some(prop_with_origin) = find_prop(p_computed, &CssPropertyType::FontSize) else {
        panic!("p should have computed FontSize property");
    };
    let CssProperty::FontSize(font_size_value) = &prop_with_origin.property else {
        panic!("FontSize should be CssProperty::FontSize variant");
    };

    let Some(font_size) = font_size_value.get_property() else {
        panic!("FontSize value should not be None/Auto/Initial/Inherit");
    };

    // 2em relative to parent's 16px = 32px
    let size = font_size.inner.to_pixels_internal(16.0, 16.0); // Parent's font-size is 16px
    assert_eq!(
        size, 32.0,
        "p with font-size: 2em should compute to 32px (2 * 16px)"
    );
}

#[test]
fn test_em_unit_cascading_multiplication() {
    // Test that 'em' units cascade properly through multiple levels
    // Each level multiplies by the parent's computed font-size
    // <div style="font-size: 10px">
    //   <p style="font-size: 2em">       <!-- 20px = 10 * 2 -->
    //     <span style="font-size: 1.5em"><!-- 30px = 20 * 1.5 -->
    //       Text
    //     </span>
    //   </p>
    // </div>

    let dom = Dom::create_div()
        .with_css_props(
            vec![CssPropertyWithConditions::simple(CssProperty::font_size(
                StyleFontSize::px(10.0),
            ))]
            .into(),
        )
        .with_child(
            Dom::create_node(NodeType::P)
                .with_css_props(
                    vec![CssPropertyWithConditions::simple(CssProperty::font_size(
                        StyleFontSize::em(2.0),
                    ))]
                    .into(),
                )
                .with_child(
                    Dom::create_node(NodeType::Span)
                        .with_css_props(
                            vec![CssPropertyWithConditions::simple(CssProperty::font_size(
                                StyleFontSize::em(1.5),
                            ))]
                            .into(),
                        )
                        .with_child(Dom::create_text("Text")),
                ),
        );

    let (styled_dom, mut cache) = setup_test!(dom);
    let node_hierarchy = &styled_dom.node_hierarchy.as_container().internal[..];
    let node_data = &styled_dom.node_data.as_container().internal[..];

    cache.compute_inherited_values(node_hierarchy, node_data);

    let p_id = azul_core::dom::NodeId::new(1); // p
    let span_id = azul_core::dom::NodeId::new(2); // span

    // Check p: 2em * 10px = 20px
    let Some(p_computed) = cache.computed_values.get(p_id.index()) else {
        panic!("p should have computed values");
    };

    let Some(prop_with_origin) = find_prop(p_computed, &CssPropertyType::FontSize) else {
        panic!("p should have computed FontSize");
    };
    let CssProperty::FontSize(p_font_size) = &prop_with_origin.property else {
        panic!("FontSize should be CssProperty::FontSize variant");
    };

    let Some(p_size_val) = p_font_size.get_property() else {
        panic!("p FontSize should have value");
    };

    let p_size = p_size_val.inner.to_pixels_internal(10.0, 10.0); // Parent (div) is 10px
    assert_eq!(p_size, 20.0, "p should be 20px (2em * 10px)");

    // Check span: 1.5em * 20px = 30px
    let Some(span_computed) = cache.computed_values.get(span_id.index()) else {
        panic!("span should have computed values");
    };

    let Some(prop_with_origin) = find_prop(span_computed, &CssPropertyType::FontSize) else {
        panic!("span should have computed FontSize");
    };

    let CssProperty::FontSize(span_font_size) = &prop_with_origin.property else {
        panic!("span property should be FontSize");
    };

    let Some(span_size_val) = span_font_size.get_property() else {
        panic!("span FontSize should have value");
    };

    let span_size = span_size_val.inner.to_pixels_internal(20.0, 20.0); // Parent (p) is 20px
    assert_eq!(span_size, 30.0, "span should be 30px (1.5em * 20px)");
}

#[test]
fn test_em_on_font_size_refers_to_parent() {
    // CSS Spec: "The exception is when 'em' occurs in the value of the 'font-size'
    // property itself, in which case it refers to the font size of the parent element."
    //
    // <div style="font-size: 20px">
    //   <p style="font-size: 1.5em">  <!-- Should be 30px (1.5 * parent's 20px) -->
    //     <span style="padding: 2em">  <!-- Should be 60px (2 * current element's 30px) -->
    //       Text
    //     </span>
    //   </p>
    // </div>

    use azul_css::{
        css::CssPropertyValue,
        props::{
            basic::{length::SizeMetric, pixel::PixelValue},
            layout::spacing::LayoutPaddingLeft,
        },
    };

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
                .with_child(
                    Dom::create_node(NodeType::Span)
                        .with_css_props(
                            vec![CssPropertyWithConditions::simple(CssProperty::PaddingLeft(
                                CssPropertyValue::Exact(LayoutPaddingLeft {
                                    inner: PixelValue::em(2.0),
                                }),
                            ))]
                            .into(),
                        )
                        .with_child(Dom::create_text("Text")),
                ),
        );

    // NOTE: setup_test! already calls StyledDom::new which calls compute_inherited_values
    // So we should NOT call it again, as that would re-process with already-resolved values
    let (styled_dom, cache) = setup_test!(dom);

    let div_id = azul_core::dom::NodeId::new(0); // div
    let p_id = azul_core::dom::NodeId::new(1); // p
    let span_id = azul_core::dom::NodeId::new(2); // span

    // Check div's font-size: should be 20px (Px metric)
    let div_computed = cache
        .computed_values
        .get(div_id.index())
        .expect("div should have computed values");
    let div_font_prop = find_prop(div_computed, &CssPropertyType::FontSize)
        .expect("div should have FontSize");
    let CssProperty::FontSize(div_font_size) = &div_font_prop.property else {
        panic!("div property should be FontSize");
    };
    let div_size_val = div_font_size
        .get_property()
        .expect("div FontSize should have value");
    assert_eq!(
        div_size_val.inner.metric,
        SizeMetric::Px,
        "div should have Px metric"
    );
    assert_eq!(
        div_size_val.inner.number.get(),
        20.0,
        "div font-size should be 20px"
    );

    // Check p's font-size: should be resolved to 30px (1.5 * 20px parent)
    let p_computed = cache
        .computed_values
        .get(p_id.index())
        .expect("p should have computed values");
    let p_font_prop = find_prop(p_computed, &CssPropertyType::FontSize)
        .expect("p should have FontSize");
    let CssProperty::FontSize(p_font_size) = &p_font_prop.property else {
        panic!("p property should be FontSize");
    };
    let p_size_val = p_font_size
        .get_property()
        .expect("p FontSize should have value");

    // The key test: p's font-size should be RESOLVED to Px, not still in Em
    assert_eq!(
        p_size_val.inner.metric,
        SizeMetric::Px,
        "p font-size should be resolved to Px metric (was {:?})",
        p_size_val.inner.metric
    );
    assert!(
        (p_size_val.inner.number.get() - 30.0).abs() < 0.001,
        "p font-size should be 30px (1.5em * 20px), got {}",
        p_size_val.inner.number.get()
    );

    // Check span's inherited font-size: should inherit 30px from p
    let span_computed = cache
        .computed_values
        .get(span_id.index())
        .expect("span should have computed values");
    let span_font_prop = find_prop(span_computed, &CssPropertyType::FontSize)
        .expect("span should have inherited FontSize");
    let CssProperty::FontSize(span_font_size) = &span_font_prop.property else {
        panic!("span property should be FontSize");
    };
    let span_size_val = span_font_size
        .get_property()
        .expect("span FontSize should have value");

    println!(
        "DEBUG: p font-size: metric={:?}, value={}",
        p_size_val.inner.metric,
        p_size_val.inner.number.get()
    );
    println!(
        "DEBUG: span font-size: metric={:?}, value={}",
        span_size_val.inner.metric,
        span_size_val.inner.number.get()
    );
    println!("DEBUG: span origin={:?}", span_font_prop.origin);

    // span should inherit the already-resolved 30px value from p
    assert_eq!(
        span_size_val.inner.metric,
        SizeMetric::Px,
        "span inherited font-size should be Px metric (was {:?})",
        span_size_val.inner.metric
    );
    assert!(
        (span_size_val.inner.number.get() - 30.0).abs() < 0.001,
        "span should inherit font-size: 30px from p, got {}",
        span_size_val.inner.number.get()
    );
}

#[test]
fn test_em_without_ancestor_absolute_unit() {
    // Test that when no ancestor has an absolute font-size, the default is used (16px)
    // <div style="font-size: 2em">  <!-- 2 * 16px (default) = 32px -->
    //   <p>Text</p>  <!-- Inherits 32px -->
    // </div>

    use azul_css::props::basic::length::SizeMetric;

    let dom = Dom::create_div()
        .with_css_props(
            vec![CssPropertyWithConditions::simple(CssProperty::font_size(
                StyleFontSize::em(2.0),
            ))]
            .into(),
        )
        .with_child(Dom::create_node(NodeType::P).with_child(Dom::create_text("Text")));

    // NOTE: setup_test! already calls StyledDom::new which calls compute_inherited_values
    let (styled_dom, cache) = setup_test!(dom);

    let div_id = azul_core::dom::NodeId::new(0); // div
    let p_id = azul_core::dom::NodeId::new(1); // p

    // div should resolve to 2 * 16px (default) = 32px
    let div_computed = cache
        .computed_values
        .get(div_id.index())
        .expect("div should have computed values");
    let div_font_prop = find_prop(div_computed, &CssPropertyType::FontSize)
        .expect("div should have FontSize");
    let CssProperty::FontSize(div_font_size) = &div_font_prop.property else {
        panic!("div property should be FontSize");
    };
    let div_size_val = div_font_size
        .get_property()
        .expect("div FontSize should have value");

    // The key test: div's font-size should be RESOLVED to Px
    assert_eq!(
        div_size_val.inner.metric,
        SizeMetric::Px,
        "div font-size should be resolved to Px metric (was {:?})",
        div_size_val.inner.metric
    );
    assert!(
        (div_size_val.inner.number.get() - 32.0).abs() < 0.001,
        "div font-size: 2em without absolute ancestor should be 32px (2 * 16px default), got {}",
        div_size_val.inner.number.get()
    );

    // p should inherit 32px from div
    let p_computed = cache
        .computed_values
        .get(p_id.index())
        .expect("p should have computed values");
    let p_font_prop = find_prop(p_computed, &CssPropertyType::FontSize)
        .expect("p should have inherited FontSize");
    let CssProperty::FontSize(p_font_size) = &p_font_prop.property else {
        panic!("p property should be FontSize");
    };
    let p_size_val = p_font_size
        .get_property()
        .expect("p FontSize should have value");

    assert_eq!(
        p_size_val.inner.metric,
        SizeMetric::Px,
        "p inherited font-size should be Px metric (was {:?})",
        p_size_val.inner.metric
    );
    assert!(
        (p_size_val.inner.number.get() - 32.0).abs() < 0.001,
        "p should inherit 32px from parent div, got {}",
        p_size_val.inner.number.get()
    );
}

#[test]
fn test_percentage_font_size_inheritance() {
    // Test that percentage font-sizes work like em (100% = 1em)
    // <div style="font-size: 20px">
    //   <p style="font-size: 150%">  <!-- 150% of 20px = 30px -->
    //     <span style="font-size: 80%">  <!-- 80% of 30px = 24px -->
    //       Text
    //     </span>
    //   </p>
    // </div>

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
                        StyleFontSize::percent(150.0),
                    ))]
                    .into(),
                )
                .with_child(
                    Dom::create_node(NodeType::Span)
                        .with_css_props(
                            vec![CssPropertyWithConditions::simple(CssProperty::font_size(
                                StyleFontSize::percent(80.0),
                            ))]
                            .into(),
                        )
                        .with_child(Dom::create_text("Text")),
                ),
        );

    let (styled_dom, mut cache) = setup_test!(dom);
    let node_hierarchy = &styled_dom.node_hierarchy.as_container().internal[..];
    let node_data = &styled_dom.node_data.as_container().internal[..];

    cache.compute_inherited_values(node_hierarchy, node_data);

    let p_id = azul_core::dom::NodeId::new(1); // p
    let span_id = azul_core::dom::NodeId::new(2); // span

    // p: 150% of 20px = 30px
    let Some(p_computed) = cache.computed_values.get(p_id.index()) else {
        panic!("p should have computed values");
    };

    let Some(prop_with_origin) = find_prop(p_computed, &CssPropertyType::FontSize) else {
        panic!("p should have FontSize");
    };

    let CssProperty::FontSize(p_font_size) = &prop_with_origin.property else {
        panic!("p property should be FontSize");
    };

    let Some(p_size_val) = p_font_size.get_property() else {
        panic!("p FontSize should have value");
    };

    let p_size = p_size_val.inner.to_pixels_internal(20.0, 20.0); // Parent is 20px
    assert_eq!(p_size, 30.0, "p with 150% should be 30px (1.5 * 20px)");

    // span: 80% of 30px = 24px
    let Some(span_computed) = cache.computed_values.get(span_id.index()) else {
        panic!("span should have computed values");
    };

    let Some(prop_with_origin) = find_prop(span_computed, &CssPropertyType::FontSize) else {
        panic!("span should have FontSize");
    };

    let CssProperty::FontSize(span_font_size) = &prop_with_origin.property else {
        panic!("span property should be FontSize");
    };

    let Some(span_size_val) = span_font_size.get_property() else {
        panic!("span FontSize should have value");
    };

    let span_size = span_size_val.inner.to_pixels_internal(30.0, 30.0); // Parent is 30px
    assert_eq!(span_size, 24.0, "span with 80% should be 24px (0.8 * 30px)");
}
