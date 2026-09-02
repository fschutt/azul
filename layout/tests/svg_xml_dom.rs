//! Tests that SVG elements parsed from XML end up with the correct DOM structure:
//! correct NodeTypes, correct clip paths, correct tree hierarchy.

use azul_core::dom::{Dom, NodeType, SvgNodeData};
use azul_core::svg::SvgPathElement;
use azul_layout::xml::DomXmlExt;

/// Helper: parse XML string → StyledDom, then collect (depth, NodeType) pairs
/// by walking the flat node_data array.
fn node_types_from_xml(xml: &str) -> Vec<NodeType> {
    let styled = Dom::from_xml_string(xml);
    let container = styled.node_data.as_container();
    (0..container.len())
        .map(|i| {
            container
                .get(azul_core::id::NodeId::new(i))
                .unwrap()
                .get_node_type()
                .clone()
        })
        .collect()
}

/// Helper: parse XML and return SVG data for all nodes that have it.
fn svg_data_from_xml(xml: &str) -> Vec<(usize, SvgNodeData)> {
    let styled = Dom::from_xml_string(xml);
    let container = styled.node_data.as_container();
    let mut result = Vec::new();
    for i in 0..container.len() {
        let nd = container.get(azul_core::id::NodeId::new(i)).unwrap();
        if let Some(data) = nd.get_svg_data() {
            result.push((i, data.clone()));
        }
    }
    result
}

// ============================================================================
// NodeType tests — SVG elements get the correct NodeType variants
// ============================================================================

#[test]
fn test_svg_container_node_type() {
    let types = node_types_from_xml(r#"<html><body><svg></svg></body></html>"#);
    assert!(
        types.contains(&NodeType::Svg),
        "Should contain Svg node type, got: {types:?}"
    );
}

#[test]
fn test_svg_path_node_type() {
    let types = node_types_from_xml(
        r#"<html><body><svg><path d="M 0,0 L 10,10"></path></svg></body></html>"#,
    );
    assert!(
        types.contains(&NodeType::SvgPath),
        "Should contain SvgPath, got: {types:?}"
    );
}

#[test]
fn test_svg_circle_node_type() {
    let types = node_types_from_xml(
        r#"<html><body><svg><circle cx="50" cy="50" r="25"></circle></svg></body></html>"#,
    );
    assert!(
        types.contains(&NodeType::SvgCircle),
        "Should contain SvgCircle, got: {types:?}"
    );
}

#[test]
fn test_svg_rect_node_type() {
    let types = node_types_from_xml(
        r#"<html><body><svg><rect x="0" y="0" width="100" height="50"></rect></svg></body></html>"#,
    );
    assert!(
        types.contains(&NodeType::SvgRect),
        "Should contain SvgRect, got: {types:?}"
    );
}

#[test]
fn test_svg_ellipse_node_type() {
    let types = node_types_from_xml(
        r#"<html><body><svg><ellipse cx="50" cy="50" rx="30" ry="20"></ellipse></svg></body></html>"#,
    );
    assert!(
        types.contains(&NodeType::SvgEllipse),
        "Should contain SvgEllipse, got: {types:?}"
    );
}

#[test]
fn test_svg_line_node_type() {
    let types = node_types_from_xml(
        r#"<html><body><svg><line x1="0" y1="0" x2="100" y2="100"></line></svg></body></html>"#,
    );
    assert!(
        types.contains(&NodeType::SvgLine),
        "Should contain SvgLine, got: {types:?}"
    );
}

#[test]
fn test_svg_polygon_node_type() {
    let types = node_types_from_xml(
        r#"<html><body><svg><polygon points="0,0 50,100 100,0"></polygon></svg></body></html>"#,
    );
    assert!(
        types.contains(&NodeType::SvgPolygon),
        "Should contain SvgPolygon, got: {types:?}"
    );
}

#[test]
fn test_svg_polyline_node_type() {
    let types = node_types_from_xml(
        r#"<html><body><svg><polyline points="0,0 50,50 100,0"></polyline></svg></body></html>"#,
    );
    assert!(
        types.contains(&NodeType::SvgPolyline),
        "Should contain SvgPolyline, got: {types:?}"
    );
}

#[test]
fn test_svg_g_node_type() {
    let types = node_types_from_xml(
        r#"<html><body><svg><g><path d="M 0,0 L 10,10"></path></g></svg></body></html>"#,
    );
    assert!(
        types.contains(&NodeType::SvgG),
        "Should contain SvgG, got: {types:?}"
    );
    assert!(
        types.contains(&NodeType::SvgPath),
        "Group child should be SvgPath, got: {types:?}"
    );
}

// ============================================================================
// Clip path tests — SVG shapes get SvgClip attached
// ============================================================================

#[test]
fn test_svg_path_has_clip() {
    let data = svg_data_from_xml(
        r#"<html><body><svg><path d="M 0,0 L 100,0 L 100,100 Z"></path></svg></body></html>"#,
    );
    assert_eq!(data.len(), 1, "path should produce exactly one clip");
    match &data[0].1 {
        SvgNodeData::Path(mp) => {
            let rings = mp.rings.as_ref();
            assert_eq!(rings.len(), 1, "triangle should have 1 ring");
            // M 0,0 L 100,0 L 100,100 Z → 2 lines + 1 closing line = 3 elements
            assert_eq!(rings[0].items.as_ref().len(), 3);
        }
        _ => panic!("Expected Path variant"),
    }
}

#[test]
fn test_svg_circle_has_clip() {
    let data = svg_data_from_xml(
        r#"<html><body><svg><circle cx="50" cy="50" r="25"></circle></svg></body></html>"#,
    );
    assert_eq!(data.len(), 1);
    match &data[0].1 {
        SvgNodeData::Path(mp) => {
            let rings = mp.rings.as_ref();
            assert_eq!(rings.len(), 1, "circle → 1 ring");
            // Circle approximated as 4 cubic beziers
            assert_eq!(rings[0].items.as_ref().len(), 4);
            for item in rings[0].items.as_ref() {
                assert!(matches!(item, SvgPathElement::CubicCurve(_)));
            }
        }
        _ => panic!("Expected Path variant"),
    }
}

#[test]
fn test_svg_rect_has_clip() {
    let data = svg_data_from_xml(
        r#"<html><body><svg><rect x="10" y="20" width="100" height="50"></rect></svg></body></html>"#,
    );
    assert_eq!(data.len(), 1);
    match &data[0].1 {
        SvgNodeData::Path(mp) => {
            let rings = mp.rings.as_ref();
            assert_eq!(rings.len(), 1);
            // Simple rect (no rx/ry) → 4 lines
            assert_eq!(rings[0].items.as_ref().len(), 4);
            for item in rings[0].items.as_ref() {
                assert!(matches!(item, SvgPathElement::Line(_)));
            }
        }
        _ => panic!("Expected Path variant"),
    }
}

#[test]
fn test_svg_rect_rounded_has_clip() {
    let data = svg_data_from_xml(
        r#"<html><body><svg><rect x="0" y="0" width="100" height="100" rx="10"></rect></svg></body></html>"#,
    );
    assert_eq!(data.len(), 1);
    match &data[0].1 {
        SvgNodeData::Path(mp) => {
            let rings = mp.rings.as_ref();
            assert_eq!(rings.len(), 1);
            // Rounded rect: 4 lines + 4 cubic curves = 8 elements
            assert_eq!(rings[0].items.as_ref().len(), 8);
        }
        _ => panic!("Expected Path variant"),
    }
}

#[test]
fn test_svg_polygon_has_clip() {
    let data = svg_data_from_xml(
        r#"<html><body><svg><polygon points="0,0 100,0 100,100 0,100"></polygon></svg></body></html>"#,
    );
    assert_eq!(data.len(), 1);
    match &data[0].1 {
        SvgNodeData::Path(mp) => {
            let rings = mp.rings.as_ref();
            assert_eq!(rings.len(), 1);
            // 4 points → 3 lines between consecutive + 1 closing line = 4 lines
            assert_eq!(rings[0].items.as_ref().len(), 4);
        }
        _ => panic!("Expected Path variant"),
    }
}

#[test]
fn test_svg_ellipse_has_clip() {
    let data = svg_data_from_xml(
        r#"<html><body><svg><ellipse cx="50" cy="50" rx="30" ry="20"></ellipse></svg></body></html>"#,
    );
    assert_eq!(data.len(), 1);
    match &data[0].1 {
        SvgNodeData::Path(mp) => {
            let rings = mp.rings.as_ref();
            assert_eq!(rings.len(), 1);
            // Ellipse → 4 cubic beziers
            assert_eq!(rings[0].items.as_ref().len(), 4);
        }
        _ => panic!("Expected Path variant"),
    }
}

// ============================================================================
// Hierarchy tests — SVG children are nested correctly
// ============================================================================

#[test]
fn test_svg_no_clip_outside_svg() {
    // Shapes outside <svg> should NOT get clip paths (they're just div-like nodes)
    let data = svg_data_from_xml(
        r#"<html><body><div><path d="M 0,0 L 10,10"></path></div></body></html>"#,
    );
    // "path" outside <svg> is treated as an unknown element → falls through to Div
    // with no SVG parsing, so no clip
    assert_eq!(data.len(), 0, "path outside <svg> should not have clip");
}

#[test]
fn test_multiple_shapes_in_svg() {
    let data = svg_data_from_xml(
        r#"<html><body><svg>
            <circle cx="10" cy="10" r="5"></circle>
            <rect x="20" y="20" width="30" height="30"></rect>
            <path d="M 0,0 L 50,50"></path>
        </svg></body></html>"#,
    );
    assert_eq!(data.len(), 3, "3 shapes → 3 clips, got {}", data.len());
}

#[test]
fn test_shapes_inside_g_get_clips() {
    let data = svg_data_from_xml(
        r#"<html><body><svg><g>
            <circle cx="10" cy="10" r="5"></circle>
            <rect x="20" y="20" width="30" height="30"></rect>
        </g></svg></body></html>"#,
    );
    assert_eq!(data.len(), 2, "shapes inside <g> should still get clips");
}

#[test]
fn test_zero_radius_circle_no_clip() {
    let data = svg_data_from_xml(
        r#"<html><body><svg><circle cx="50" cy="50" r="0"></circle></svg></body></html>"#,
    );
    assert_eq!(data.len(), 0, "zero-radius circle should produce no clip");
}

#[test]
fn test_zero_size_rect_no_clip() {
    let data = svg_data_from_xml(
        r#"<html><body><svg><rect x="0" y="0" width="0" height="50"></rect></svg></body></html>"#,
    );
    assert_eq!(data.len(), 0, "zero-width rect should produce no clip");
}

// ============================================================================
// Sizing — an <svg> is a replaced element and must carry its own dimensions
// ============================================================================

/// Width (or height) in logical px declared on the node's inline style.
fn declared_px(
    styled: &azul_core::styled_dom::StyledDom,
    node: usize,
    want_width: bool,
) -> Option<f32> {
    use azul_css::{
        props::layout::{LayoutHeight, LayoutWidth},
        props::property::CssProperty,
    };
    let container = styled.node_data.as_container();
    let nd = container.get(azul_core::id::NodeId::new(node))?;
    nd.style.rules.as_ref().iter().find_map(|rule| {
        rule.declarations.as_ref().iter().find_map(|decl| {
            let azul_css::css::CssDeclaration::Static(prop) = decl else {
                return None;
            };
            match (prop, want_width) {
                (CssProperty::Width(w), true) => match w.get_property() {
                    Some(LayoutWidth::Px(v)) => Some(v.to_pixels_internal(0.0, 16.0, 1.0)),
                    _ => None,
                },
                (CssProperty::Height(h), false) => match h.get_property() {
                    Some(LayoutHeight::Px(v)) => Some(v.to_pixels_internal(0.0, 16.0, 1.0)),
                    _ => None,
                },
                _ => None,
            }
        })
    })
}

fn svg_node(styled: &azul_core::styled_dom::StyledDom) -> usize {
    let container = styled.node_data.as_container();
    (0..container.len())
        .find(|i| {
            *container
                .get(azul_core::id::NodeId::new(*i))
                .unwrap()
                .get_node_type()
                == NodeType::Svg
        })
        .expect("the document has an <svg>")
}

/// THE regression: an `<svg>` with only a `viewBox` laid out 0x0, took no
/// space, and everything inside it clipped away to nothing. SVG's own default
/// sizing rule says the viewBox IS the intrinsic size when width/height are
/// absent.
#[test]
fn an_svg_takes_its_intrinsic_size_from_the_view_box() {
    let styled = Dom::from_xml_string(
        r#"<html><body><svg viewBox="0 0 24 18"><path d="M 0,0 L 1,1"></path></svg></body></html>"#,
    );
    let n = svg_node(&styled);
    assert_eq!(declared_px(&styled, n, true), Some(24.0));
    assert_eq!(declared_px(&styled, n, false), Some(18.0));
}

/// Stated dimensions outrank the viewBox - the viewBox is the coordinate
/// system, not necessarily the size.
#[test]
fn stated_dimensions_outrank_the_view_box() {
    let styled = Dom::from_xml_string(
        r#"<html><body><svg viewBox="0 0 24 18" width="48" height="36"></svg></body></html>"#,
    );
    let n = svg_node(&styled);
    assert_eq!(declared_px(&styled, n, true), Some(48.0));
    assert_eq!(declared_px(&styled, n, false), Some(36.0));

    // ... and a bare CSS unit is as common in the wild as a bare number.
    let styled = Dom::from_xml_string(
        r#"<html><body><svg viewBox="0 0 24 18" width="48px" height="36px"></svg></body></html>"#,
    );
    let n = svg_node(&styled);
    assert_eq!(declared_px(&styled, n, true), Some(48.0));
}

/// An `<svg>` that states nothing declares nothing - inventing a size would be
/// worse than letting CSS decide.
#[test]
fn an_svg_with_no_size_and_no_view_box_declares_neither() {
    let styled = Dom::from_xml_string(r#"<html><body><svg></svg></body></html>"#);
    let n = svg_node(&styled);
    assert_eq!(declared_px(&styled, n, true), None);
    assert_eq!(declared_px(&styled, n, false), None);
}

/// The viewBox is the element's user-space coordinate system and has to be
/// RECORDED, not just measured: `SvgNodeData::ViewBox` existed as a variant
/// that nothing ever produced.
#[test]
fn the_view_box_is_recorded_on_the_node() {
    let styled = Dom::from_xml_string(
        r#"<html><body><svg viewBox="-1 -2 24 18"></svg></body></html>"#,
    );
    let container = styled.node_data.as_container();
    let n = svg_node(&styled);
    let data = container
        .get(azul_core::id::NodeId::new(n))
        .unwrap()
        .get_svg_data()
        .cloned();
    assert_eq!(
        data,
        Some(azul_core::dom::SvgNodeData::ViewBox {
            min_x: -1.0,
            min_y: -2.0,
            width: 24.0,
            height: 18.0,
        })
    );
}

/// A malformed viewBox must be REJECTED rather than half-read: taking the
/// first four of five numbers places the art somewhere nobody asked for.
#[test]
fn a_malformed_view_box_is_rejected_rather_than_guessed() {
    for bad in ["0 0 24", "0 0 24 18 9", "0 0 0 18", "a b c d", ""] {
        let xml = format!(r#"<html><body><svg viewBox="{bad}"></svg></body></html>"#);
        let styled = Dom::from_xml_string(&xml);
        let n = svg_node(&styled);
        assert_eq!(
            declared_px(&styled, n, true),
            None,
            "viewBox={bad:?} must not produce a size"
        );
    }
}
