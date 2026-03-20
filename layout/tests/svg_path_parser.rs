//! Tests for the SVG path `d=""` parser.

use azul_core::svg::{SvgLine, SvgMultiPolygon, SvgPath, SvgPathElement};
use azul_core::svg_path_parser::{parse_svg_path_d, svg_circle_to_paths, svg_rect_to_path, SvgPathParseError};
use azul_css::props::basic::{SvgCubicCurve, SvgPoint, SvgQuadraticCurve};

fn approx_eq(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

fn pt_eq(a: SvgPoint, x: f32, y: f32) -> bool {
    approx_eq(a.x, x) && approx_eq(a.y, y)
}

fn rings(mp: &SvgMultiPolygon) -> &[SvgPath] {
    mp.rings.as_ref()
}

fn items(path: &SvgPath) -> &[SvgPathElement] {
    path.items.as_ref()
}

#[test]
fn test_simple_triangle() {
    let mp = parse_svg_path_d("M 0,0 L 100,0 L 100,100 Z").unwrap();
    assert_eq!(rings(&mp).len(), 1);
    let elems = items(&rings(&mp)[0]);
    assert_eq!(elems.len(), 3); // 2 lines + closing line
    // All should be lines
    for e in elems {
        assert!(matches!(e, SvgPathElement::Line(_)));
    }
}

#[test]
fn test_relative_commands() {
    let mp = parse_svg_path_d("m 10,10 l 50,0 l 0,50 z").unwrap();
    assert_eq!(rings(&mp).len(), 1);
    let elems = items(&rings(&mp)[0]);
    // Should produce: line(10,10->60,10), line(60,10->60,60), closing line(60,60->10,10)
    assert_eq!(elems.len(), 3);
    if let SvgPathElement::Line(l) = &elems[0] {
        assert!(pt_eq(l.start, 10.0, 10.0));
        assert!(pt_eq(l.end, 60.0, 10.0));
    } else {
        panic!("expected line");
    }
    if let SvgPathElement::Line(l) = &elems[1] {
        assert!(pt_eq(l.start, 60.0, 10.0));
        assert!(pt_eq(l.end, 60.0, 60.0));
    } else {
        panic!("expected line");
    }
}

#[test]
fn test_horizontal_vertical() {
    let mp = parse_svg_path_d("M 0,0 H 100 V 100 H 0 Z").unwrap();
    assert_eq!(rings(&mp).len(), 1);
    let elems = items(&rings(&mp)[0]);
    assert_eq!(elems.len(), 4); // 3 H/V lines + closing line
    for e in elems {
        assert!(matches!(e, SvgPathElement::Line(_)));
    }
    if let SvgPathElement::Line(l) = &elems[0] {
        assert!(pt_eq(l.start, 0.0, 0.0));
        assert!(pt_eq(l.end, 100.0, 0.0));
    } else {
        panic!();
    }
    if let SvgPathElement::Line(l) = &elems[1] {
        assert!(pt_eq(l.start, 100.0, 0.0));
        assert!(pt_eq(l.end, 100.0, 100.0));
    } else {
        panic!();
    }
}

#[test]
fn test_cubic_bezier() {
    let mp = parse_svg_path_d("M 0,0 C 10,20 30,40 50,50").unwrap();
    assert_eq!(rings(&mp).len(), 1);
    let elems = items(&rings(&mp)[0]);
    assert_eq!(elems.len(), 1);
    if let SvgPathElement::CubicCurve(c) = &elems[0] {
        assert!(pt_eq(c.start, 0.0, 0.0));
        assert!(pt_eq(c.ctrl_1, 10.0, 20.0));
        assert!(pt_eq(c.ctrl_2, 30.0, 40.0));
        assert!(pt_eq(c.end, 50.0, 50.0));
    } else {
        panic!("expected cubic");
    }
}

#[test]
fn test_smooth_cubic() {
    let mp = parse_svg_path_d("M 0,0 C 10,20 30,40 50,50 S 70,80 90,90").unwrap();
    let elems = items(&rings(&mp)[0]);
    assert_eq!(elems.len(), 2);
    // Second curve should have reflected ctrl1
    if let SvgPathElement::CubicCurve(c) = &elems[1] {
        // reflected ctrl1 = 2*(50,50) - (30,40) = (70,60)
        assert!(pt_eq(c.ctrl_1, 70.0, 60.0));
        assert!(pt_eq(c.ctrl_2, 70.0, 80.0));
        assert!(pt_eq(c.end, 90.0, 90.0));
    } else {
        panic!("expected cubic");
    }
}

#[test]
fn test_quadratic_bezier() {
    let mp = parse_svg_path_d("M 0,0 Q 50,100 100,0").unwrap();
    let elems = items(&rings(&mp)[0]);
    assert_eq!(elems.len(), 1);
    if let SvgPathElement::QuadraticCurve(q) = &elems[0] {
        assert!(pt_eq(q.start, 0.0, 0.0));
        assert!(pt_eq(q.ctrl, 50.0, 100.0));
        assert!(pt_eq(q.end, 100.0, 0.0));
    } else {
        panic!("expected quadratic");
    }
}

#[test]
fn test_smooth_quadratic() {
    let mp = parse_svg_path_d("M 0,0 Q 50,100 100,0 T 200,0").unwrap();
    let elems = items(&rings(&mp)[0]);
    assert_eq!(elems.len(), 2);
    if let SvgPathElement::QuadraticCurve(q) = &elems[1] {
        // reflected ctrl = 2*(100,0) - (50,100) = (150,-100)
        assert!(pt_eq(q.ctrl, 150.0, -100.0));
        assert!(pt_eq(q.end, 200.0, 0.0));
    } else {
        panic!("expected quadratic");
    }
}

#[test]
fn test_arc_basic() {
    let mp = parse_svg_path_d("M 0,0 A 25,25 0 0,1 50,0").unwrap();
    let elems = items(&rings(&mp)[0]);
    // Arc should produce at least one cubic curve
    assert!(!elems.is_empty());
    for e in elems {
        assert!(matches!(e, SvgPathElement::CubicCurve(_)));
    }
    // Last element should end at (50,0)
    let last = elems.last().unwrap();
    assert!(pt_eq(last.get_end(), 50.0, 0.0));
}

#[test]
fn test_multiple_subpaths() {
    let mp = parse_svg_path_d("M 0,0 L 10,10 M 20,20 L 30,30").unwrap();
    assert_eq!(rings(&mp).len(), 2);
    assert_eq!(items(&rings(&mp)[0]).len(), 1);
    assert_eq!(items(&rings(&mp)[1]).len(), 1);
}

#[test]
fn test_implicit_lineto() {
    // After M, implicit coordinates are treated as L
    let mp = parse_svg_path_d("M 0,0 10,10 20,0").unwrap();
    let elems = items(&rings(&mp)[0]);
    assert_eq!(elems.len(), 2);
    for e in elems {
        assert!(matches!(e, SvgPathElement::Line(_)));
    }
}

#[test]
fn test_empty_path() {
    assert_eq!(parse_svg_path_d(""), Err(SvgPathParseError::EmptyPath));
    assert_eq!(parse_svg_path_d("  "), Err(SvgPathParseError::EmptyPath));
}

#[test]
fn test_close_adds_line() {
    // Z should add a closing line when current != subpath_start
    let mp = parse_svg_path_d("M 0,0 L 100,0 L 100,100 Z").unwrap();
    let elems = items(&rings(&mp)[0]);
    assert_eq!(elems.len(), 3); // 2 L + closing line from Z
    if let SvgPathElement::Line(l) = &elems[2] {
        assert!(pt_eq(l.start, 100.0, 100.0));
        assert!(pt_eq(l.end, 0.0, 0.0));
    } else {
        panic!("expected closing line");
    }
}

#[test]
fn test_close_no_extra() {
    // Z should NOT add a closing line when current == subpath_start
    let mp = parse_svg_path_d("M 0,0 L 100,0 L 100,100 L 0,0 Z").unwrap();
    let elems = items(&rings(&mp)[0]);
    assert_eq!(elems.len(), 3); // 3 L, no extra close
}

#[test]
fn test_circle_to_paths() {
    let path = svg_circle_to_paths(50.0, 50.0, 25.0);
    let elems = path.items.as_ref();
    assert_eq!(elems.len(), 4);
    for e in elems {
        assert!(matches!(e, SvgPathElement::CubicCurve(_)));
    }
    // First curve starts at top center
    assert!(pt_eq(elems[0].get_start(), 50.0, 25.0));
}

#[test]
fn test_rect_to_path() {
    let path = svg_rect_to_path(10.0, 20.0, 100.0, 50.0, 0.0, 0.0);
    let elems = path.items.as_ref();
    assert_eq!(elems.len(), 4);
    for e in elems {
        assert!(matches!(e, SvgPathElement::Line(_)));
    }
}

#[test]
fn test_rect_rounded() {
    let path = svg_rect_to_path(0.0, 0.0, 100.0, 100.0, 10.0, 10.0);
    let elems = path.items.as_ref();
    // 4 lines + 4 curves = 8 elements
    assert_eq!(elems.len(), 8);
    let lines = elems.iter().filter(|e| matches!(e, SvgPathElement::Line(_))).count();
    let curves = elems.iter().filter(|e| matches!(e, SvgPathElement::CubicCurve(_))).count();
    assert_eq!(lines, 4);
    assert_eq!(curves, 4);
}

#[test]
fn test_compact_numbers() {
    // SVG allows compact notation: no separator needed between numbers when unambiguous
    let mp = parse_svg_path_d("M0,0L100,0L100,100Z").unwrap();
    assert_eq!(rings(&mp).len(), 1);
    assert_eq!(items(&rings(&mp)[0]).len(), 3);
}

#[test]
fn test_negative_coords() {
    let mp = parse_svg_path_d("M -10,-20 L -30,-40").unwrap();
    let elems = items(&rings(&mp)[0]);
    if let SvgPathElement::Line(l) = &elems[0] {
        assert!(pt_eq(l.start, -10.0, -20.0));
        assert!(pt_eq(l.end, -30.0, -40.0));
    } else {
        panic!("expected line");
    }
}
