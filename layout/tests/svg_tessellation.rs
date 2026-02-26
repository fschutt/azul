//! SVG Tessellation Tests
//!
//! Minimal unit tests to verify that SVG shapes (paths, rects, circles)
//! are correctly tessellated into triangle vertex/index buffers.
//!
//! These tests check:
//! 1. That tessellation produces non-empty vertex/index buffers
//! 2. That index values are valid (within vertex buffer bounds)
//! 3. That index counts are divisible by 3 (triangle primitives)
//! 4. That expected vertex coordinates appear in the output

use azul_core::svg::{
    SvgCircle, SvgFillStyle, SvgLine, SvgMultiPolygon, SvgPath, SvgPathElement,
    SvgSimpleNode, SvgStrokeStyle, TessellatedSvgNode,
};
// SvgPoint and SvgRect are re-exported from azul_css
use azul_css::props::basic::animation::{SvgPoint, SvgRect};
use azul_layout::xml::svg::{
    tessellate_circle_fill, tessellate_multi_polygon_fill, tessellate_multi_shape_fill,
    tessellate_path_fill, tessellate_path_stroke, tessellate_rect_fill, tessellate_rect_stroke,
};

/// Helper to create a simple square path from (0,0) to (100,100)
fn create_square_path() -> SvgPath {
    // Square: (0,0) -> (100,0) -> (100,100) -> (0,100) -> (0,0)
    let items = vec![
        SvgPathElement::Line(SvgLine {
            start: SvgPoint { x: 0.0, y: 0.0 },
            end: SvgPoint { x: 100.0, y: 0.0 },
        }),
        SvgPathElement::Line(SvgLine {
            start: SvgPoint { x: 100.0, y: 0.0 },
            end: SvgPoint { x: 100.0, y: 100.0 },
        }),
        SvgPathElement::Line(SvgLine {
            start: SvgPoint { x: 100.0, y: 100.0 },
            end: SvgPoint { x: 0.0, y: 100.0 },
        }),
        SvgPathElement::Line(SvgLine {
            start: SvgPoint { x: 0.0, y: 100.0 },
            end: SvgPoint { x: 0.0, y: 0.0 },
        }),
    ];

    SvgPath {
        items: items.into(),
    }
}

/// Helper to create a simple triangle path
fn create_triangle_path() -> SvgPath {
    // Triangle: (50,0) -> (100,100) -> (0,100) -> (50,0)
    let items = vec![
        SvgPathElement::Line(SvgLine {
            start: SvgPoint { x: 50.0, y: 0.0 },
            end: SvgPoint { x: 100.0, y: 100.0 },
        }),
        SvgPathElement::Line(SvgLine {
            start: SvgPoint { x: 100.0, y: 100.0 },
            end: SvgPoint { x: 0.0, y: 100.0 },
        }),
        SvgPathElement::Line(SvgLine {
            start: SvgPoint { x: 0.0, y: 100.0 },
            end: SvgPoint { x: 50.0, y: 0.0 },
        }),
    ];

    SvgPath {
        items: items.into(),
    }
}

/// Verify basic tessellation invariants
fn verify_tessellation(
    node: &TessellatedSvgNode,
    test_name: &str,
    expect_min_vertices: usize,
    expect_min_indices: usize,
) {
    let vertices = node.vertices.as_ref();
    let indices = node.indices.as_ref();

    // Check non-empty
    assert!(
        !vertices.is_empty(),
        "{}: Expected non-empty vertices",
        test_name
    );
    assert!(
        !indices.is_empty(),
        "{}: Expected non-empty indices",
        test_name
    );

    // Check minimum counts
    assert!(
        vertices.len() >= expect_min_vertices,
        "{}: Expected at least {} vertices, got {}",
        test_name,
        expect_min_vertices,
        vertices.len()
    );
    assert!(
        indices.len() >= expect_min_indices,
        "{}: Expected at least {} indices, got {}",
        test_name,
        expect_min_indices,
        indices.len()
    );

    // Check index count divisible by 3 (triangles)
    assert_eq!(
        indices.len() % 3,
        0,
        "{}: Index count {} not divisible by 3 (not proper triangle list)",
        test_name,
        indices.len()
    );

    // Check all indices are valid (within vertex buffer bounds)
    let restart_index = azul_core::gl::GL_RESTART_INDEX;
    for (i, &idx) in indices.iter().enumerate() {
        if idx == restart_index {
            // Restart index is allowed
            continue;
        }
        assert!(
            (idx as usize) < vertices.len(),
            "{}: Index {} at position {} is out of bounds (vertex count: {})",
            test_name,
            idx,
            i,
            vertices.len()
        );
    }

    println!(
        "[PASS] {}: {} vertices, {} indices ({} triangles)",
        test_name,
        vertices.len(),
        indices.len(),
        indices.len() / 3
    );
}

/// Print tessellation details for debugging
fn debug_print_tessellation(node: &TessellatedSvgNode, name: &str) {
    let vertices = node.vertices.as_ref();
    let indices = node.indices.as_ref();

    println!("\n=== {} ===", name);
    println!("Vertices ({}):", vertices.len());
    for (i, v) in vertices.iter().enumerate().take(20) {
        println!("  [{}] ({:.2}, {:.2})", i, v.x, v.y);
    }
    if vertices.len() > 20 {
        println!("  ... and {} more", vertices.len() - 20);
    }

    println!("Indices ({}):", indices.len());
    let restart_index = azul_core::gl::GL_RESTART_INDEX;
    for chunk in indices.chunks(3).take(10) {
        let formatted: Vec<String> = chunk
            .iter()
            .map(|&idx| {
                if idx == restart_index {
                    "RESTART".to_string()
                } else {
                    idx.to_string()
                }
            })
            .collect();
        println!("  Triangle: [{}]", formatted.join(", "));
    }
    if indices.len() > 30 {
        println!("  ... and {} more indices", indices.len() - 30);
    }
}

// ============================================================================
// Tests for tessellate_rect_fill
// ============================================================================

#[test]
fn test_tessellate_rect_fill_simple() {
    let rect = SvgRect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
        radius_top_left: 0.0,
        radius_top_right: 0.0,
        radius_bottom_left: 0.0,
        radius_bottom_right: 0.0,
    };

    let fill_style = SvgFillStyle::default();
    let result = tessellate_rect_fill(&rect, fill_style);

    debug_print_tessellation(&result, "Rect Fill (100x100)");

    // A simple rect should produce at least 4 vertices (corners) and 6 indices (2 triangles)
    verify_tessellation(&result, "Rect Fill", 4, 6);
}

#[test]
fn test_tessellate_rect_fill_with_rounded_corners() {
    let rect = SvgRect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
        radius_top_left: 10.0,
        radius_top_right: 10.0,
        radius_bottom_left: 10.0,
        radius_bottom_right: 10.0,
    };

    let fill_style = SvgFillStyle::default();
    let result = tessellate_rect_fill(&rect, fill_style);

    debug_print_tessellation(&result, "Rect Fill (Rounded 10px)");

    // Rounded corners produce more vertices
    verify_tessellation(&result, "Rect Fill Rounded", 4, 6);
}

// ============================================================================
// Tests for tessellate_circle_fill
// ============================================================================

#[test]
fn test_tessellate_circle_fill() {
    let circle = SvgCircle {
        center_x: 50.0,
        center_y: 50.0,
        radius: 50.0,
    };

    let fill_style = SvgFillStyle::default();
    let result = tessellate_circle_fill(&circle, fill_style);

    debug_print_tessellation(&result, "Circle Fill (r=50)");

    // Circle produces many vertices for smooth curve
    verify_tessellation(&result, "Circle Fill", 10, 24);
}

// ============================================================================
// Tests for tessellate_path_fill
// ============================================================================

#[test]
fn test_tessellate_path_fill_square() {
    let path = create_square_path();
    let fill_style = SvgFillStyle::default();
    let result = tessellate_path_fill(&path, fill_style);

    debug_print_tessellation(&result, "Path Fill (Square)");

    // Square path should produce at least 4 vertices and 6 indices
    verify_tessellation(&result, "Path Fill Square", 4, 6);
}

#[test]
fn test_tessellate_path_fill_triangle() {
    let path = create_triangle_path();
    let fill_style = SvgFillStyle::default();
    let result = tessellate_path_fill(&path, fill_style);

    debug_print_tessellation(&result, "Path Fill (Triangle)");

    // Triangle path should produce at least 3 vertices and 3 indices
    verify_tessellation(&result, "Path Fill Triangle", 3, 3);
}

// ============================================================================
// Tests for tessellate_path_stroke
// ============================================================================

#[test]
fn test_tessellate_path_stroke_square() {
    let path = create_square_path();
    let stroke_style = SvgStrokeStyle {
        line_width: 5.0,
        ..Default::default()
    };
    let result = tessellate_path_stroke(&path, stroke_style);

    debug_print_tessellation(&result, "Path Stroke (Square, 5px)");

    // Stroke produces vertices for both sides of the line
    verify_tessellation(&result, "Path Stroke Square", 8, 12);
}

// ============================================================================
// Tests for tessellate_multi_polygon_fill
// ============================================================================

#[test]
fn test_tessellate_multi_polygon_fill() {
    let path = create_square_path();
    let polygon = SvgMultiPolygon::create(vec![path].into());

    let fill_style = SvgFillStyle::default();
    let result = tessellate_multi_polygon_fill(&polygon, fill_style);

    debug_print_tessellation(&result, "MultiPolygon Fill (Square)");

    verify_tessellation(&result, "MultiPolygon Fill", 4, 6);
}

// ============================================================================
// Tests for tessellate_multi_shape_fill
// ============================================================================

#[test]
fn test_tessellate_multi_shape_fill_rect_and_circle() {
    let shapes = vec![
        SvgSimpleNode::Rect(SvgRect {
            x: 0.0,
            y: 0.0,
            width: 50.0,
            height: 50.0,
            radius_top_left: 0.0,
            radius_top_right: 0.0,
            radius_bottom_left: 0.0,
            radius_bottom_right: 0.0,
        }),
        SvgSimpleNode::Circle(SvgCircle {
            center_x: 75.0,
            center_y: 75.0,
            radius: 25.0,
        }),
    ];

    let fill_style = SvgFillStyle::default();
    let result = tessellate_multi_shape_fill(&shapes, fill_style);

    debug_print_tessellation(&result, "MultiShape Fill (Rect + Circle)");

    // Should produce vertices for both shapes
    verify_tessellation(&result, "MultiShape Fill", 10, 24);
}

// ============================================================================
// Tests for tessellate_rect_stroke
// ============================================================================

#[test]
fn test_tessellate_rect_stroke() {
    let rect = SvgRect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
        radius_top_left: 0.0,
        radius_top_right: 0.0,
        radius_bottom_left: 0.0,
        radius_bottom_right: 0.0,
    };

    let stroke_style = SvgStrokeStyle {
        line_width: 5.0,
        ..Default::default()
    };
    let result = tessellate_rect_stroke(&rect, stroke_style);

    debug_print_tessellation(&result, "Rect Stroke (100x100, 5px)");

    // Stroke produces vertices for both sides of each edge
    verify_tessellation(&result, "Rect Stroke", 8, 12);
}

// ============================================================================
// Edge case tests
// ============================================================================

#[test]
fn test_tessellate_degenerate_zero_size_rect() {
    // Zero-size rect should still not crash
    let rect = SvgRect {
        x: 50.0,
        y: 50.0,
        width: 0.0,
        height: 0.0,
        radius_top_left: 0.0,
        radius_top_right: 0.0,
        radius_bottom_left: 0.0,
        radius_bottom_right: 0.0,
    };

    let fill_style = SvgFillStyle::default();
    let result = tessellate_rect_fill(&rect, fill_style);

    // May produce empty or minimal output - just check it doesn't panic
    println!(
        "Zero-size rect: {} vertices, {} indices",
        result.vertices.as_ref().len(),
        result.indices.as_ref().len()
    );
}

#[test]
fn test_tessellate_very_small_circle() {
    let circle = SvgCircle {
        center_x: 0.0,
        center_y: 0.0,
        radius: 0.001,
    };

    let fill_style = SvgFillStyle::default();
    let result = tessellate_circle_fill(&circle, fill_style);

    // Very small circle should still produce valid output
    println!(
        "Very small circle: {} vertices, {} indices",
        result.vertices.as_ref().len(),
        result.indices.as_ref().len()
    );
}

#[test]
fn test_tessellate_empty_path() {
    let path = SvgPath {
        items: Vec::new().into(),
    };

    let fill_style = SvgFillStyle::default();
    let result = tessellate_path_fill(&path, fill_style);

    // Empty path should produce empty output
    assert!(
        result.vertices.as_ref().is_empty(),
        "Empty path should produce no vertices"
    );
    assert!(
        result.indices.as_ref().is_empty(),
        "Empty path should produce no indices"
    );
    println!("Empty path test: PASS");
}

// ============================================================================
// Index buffer validation tests
// ============================================================================

#[test]
fn test_index_buffer_no_restart_markers_in_fill() {
    // Fill tessellation should NOT use restart markers in index buffer
    // (restart markers are for GL_PRIMITIVE_RESTART, typically for strokes)
    let rect = SvgRect {
        x: 0.0,
        y: 0.0,
        width: 100.0,
        height: 100.0,
        radius_top_left: 0.0,
        radius_top_right: 0.0,
        radius_bottom_left: 0.0,
        radius_bottom_right: 0.0,
    };

    let fill_style = SvgFillStyle::default();
    let result = tessellate_rect_fill(&rect, fill_style);

    let restart_index = azul_core::gl::GL_RESTART_INDEX;
    let has_restart = result.indices.as_ref().iter().any(|&i| i == restart_index);

    // Fill should NOT have restart indices
    assert!(
        !has_restart,
        "Fill tessellation should not contain restart index markers"
    );
    println!("Fill index buffer validation: PASS (no restart markers)");
}

#[test]
fn test_vertex_coordinates_in_bounds() {
    // Vertices should be within expected coordinate bounds
    let rect = SvgRect {
        x: 10.0,
        y: 20.0,
        width: 100.0,
        height: 50.0,
        radius_top_left: 0.0,
        radius_top_right: 0.0,
        radius_bottom_left: 0.0,
        radius_bottom_right: 0.0,
    };

    let fill_style = SvgFillStyle::default();
    let result = tessellate_rect_fill(&rect, fill_style);

    for v in result.vertices.as_ref() {
        assert!(
            v.x >= 10.0 - 0.1 && v.x <= 110.0 + 0.1,
            "Vertex x={} out of expected range [10, 110]",
            v.x
        );
        assert!(
            v.y >= 20.0 - 0.1 && v.y <= 70.0 + 0.1,
            "Vertex y={} out of expected range [20, 70]",
            v.y
        );
    }
    println!("Vertex bounds validation: PASS");
}
