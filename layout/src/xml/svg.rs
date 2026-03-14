use alloc::boxed::Box;
use core::fmt;

#[cfg(not(feature = "svg"))]
pub use azul_core::svg::*;
// re-export everything except for Svg and SvgXmlNode
#[cfg(feature = "svg")]
pub use azul_core::svg::{
    c_void,
    FontDatabase,
    ImageRendering,
    Indent,
    OptionSvgDashPattern,
    ResultSvgSvgParseError,
    ResultSvgXmlNodeSvgParseError,
    ShapeRendering,
    SvgCircle,
    SvgColoredVertex,
    SvgColoredVertexVec,
    SvgColoredVertexVecDestructor,
    SvgDashPattern,
    SvgFillRule,
    SvgFillStyle,
    SvgFitTo,
    SvgLine,
    SvgLineCap,
    SvgLineJoin,
    SvgMultiPolygon,
    SvgMultiPolygonVec,
    SvgMultiPolygonVecDestructor,
    SvgNode,
    SvgParseError,
    SvgParseOptions,
    SvgPath,
    SvgPathElement,
    SvgPathElementVec,
    SvgPathElementVecDestructor,
    SvgPathVec,
    SvgPathVecDestructor,
    SvgRenderOptions,
    SvgRenderTransform,

    SvgSimpleNode,
    SvgSimpleNodeVec,
    SvgSimpleNodeVecDestructor,
    SvgSize,
    SvgStrokeStyle,
    SvgStyle,
    SvgStyledNode,
    SvgTransform,
    SvgVertex,
    SvgVertexVec,
    SvgVertexVecDestructor,
    SvgXmlOptions,
    TessellatedColoredSvgNode,
    TessellatedColoredSvgNodeVec,
    TessellatedColoredSvgNodeVecDestructor,
    // SvgXmlNode, Svg
    TessellatedGPUSvgNode,
    TessellatedSvgNode,
    TessellatedSvgNodeVec,
    TessellatedSvgNodeVecDestructor,
    TessellatedSvgNodeVecRef,
    TextRendering,
};
use azul_core::{
    geom::PhysicalSizeU32,
    gl::{GlContextPtr, Texture},
    resources::{RawImage, RawImageFormat},
};
#[cfg(feature = "svg")]
pub use azul_css::props::basic::animation::{
    SvgCubicCurve, SvgPoint, SvgQuadraticCurve, SvgRect, SvgVector,
};
use azul_css::{
    impl_result, impl_result_inner,
    props::basic::{ColorU, LayoutSize, OptionColorU, OptionLayoutSize},
    AzString, OptionI16, OptionString, OptionU16, StringVec, U8Vec,
};
#[cfg(feature = "svg")]
use lyon::{
    geom::euclid::{Point2D, Rect, Size2D, UnknownUnit},
    math::Point,
    path::Path,
    tessellation::{
        BuffersBuilder, FillOptions, FillTessellator, FillVertex, StrokeOptions, StrokeTessellator,
        StrokeVertex, VertexBuffers,
    },
};

use crate::xml::XmlError;

#[cfg(feature = "svg")]
extern crate agg_rust;

use azul_core::gl::GL_RESTART_INDEX;

#[cfg(feature = "svg")]
fn translate_svg_line_join(e: SvgLineJoin) -> lyon::tessellation::LineJoin {
    use azul_core::svg::SvgLineJoin::*;
    match e {
        Miter => lyon::tessellation::LineJoin::Miter,
        MiterClip => lyon::tessellation::LineJoin::MiterClip,
        Round => lyon::tessellation::LineJoin::Round,
        Bevel => lyon::tessellation::LineJoin::Bevel,
    }
}

#[cfg(feature = "svg")]
fn translate_svg_line_cap(e: SvgLineCap) -> lyon::tessellation::LineCap {
    use azul_core::svg::SvgLineCap::*;
    match e {
        Butt => lyon::tessellation::LineCap::Butt,
        Square => lyon::tessellation::LineCap::Square,
        Round => lyon::tessellation::LineCap::Round,
    }
}

#[cfg(feature = "svg")]
fn translate_svg_stroke_style(e: SvgStrokeStyle) -> lyon::tessellation::StrokeOptions {
    lyon::tessellation::StrokeOptions::tolerance(e.tolerance)
        .with_start_cap(translate_svg_line_cap(e.start_cap))
        .with_end_cap(translate_svg_line_cap(e.end_cap))
        .with_line_join(translate_svg_line_join(e.line_join))
        .with_line_width(e.line_width)
        .with_miter_limit(e.miter_limit)
    // TODO: e.apply_line_width - not present in lyon 17!
}

#[cfg(feature = "svg")]
fn svg_multipolygon_to_lyon_path(polygon: &SvgMultiPolygon) -> Path {
    let mut builder = Path::builder();

    for p in polygon.rings.as_ref().iter() {
        if p.items.as_ref().is_empty() {
            continue;
        }

        let start_item = p.items.as_ref()[0];
        let first_point = Point2D::new(start_item.get_start().x, start_item.get_start().y);

        builder.begin(first_point);

        for q in p.items.as_ref().iter().rev()
        /* NOTE: REVERSE ITERATOR */
        {
            match q {
                SvgPathElement::Line(l) => {
                    builder.line_to(Point2D::new(l.end.x, l.end.y));
                }
                SvgPathElement::QuadraticCurve(qc) => {
                    builder.quadratic_bezier_to(
                        Point2D::new(qc.ctrl.x, qc.ctrl.y),
                        Point2D::new(qc.end.x, qc.end.y),
                    );
                }
                SvgPathElement::CubicCurve(cc) => {
                    builder.cubic_bezier_to(
                        Point2D::new(cc.ctrl_1.x, cc.ctrl_1.y),
                        Point2D::new(cc.ctrl_2.x, cc.ctrl_2.y),
                        Point2D::new(cc.end.x, cc.end.y),
                    );
                }
            }
        }

        builder.end(p.is_closed());
    }

    builder.build()
}

#[cfg(feature = "svg")]
fn svg_multi_shape_to_lyon_path(polygon: &[SvgSimpleNode]) -> Path {
    use lyon::{
        geom::Box2D,
        path::{traits::PathBuilder, Winding},
    };

    let mut builder = Path::builder();

    for p in polygon.iter() {
        match p {
            SvgSimpleNode::Path(p) => {
                let items = p.items.as_ref();
                if p.items.as_ref().is_empty() {
                    continue;
                }

                let start_item = p.items.as_ref()[0];
                let first_point = Point2D::new(start_item.get_start().x, start_item.get_start().y);

                builder.begin(first_point);

                for q in p.items.as_ref().iter().rev()
                /* NOTE: REVERSE ITERATOR */
                {
                    match q {
                        SvgPathElement::Line(l) => {
                            builder.line_to(Point2D::new(l.end.x, l.end.y));
                        }
                        SvgPathElement::QuadraticCurve(qc) => {
                            builder.quadratic_bezier_to(
                                Point2D::new(qc.ctrl.x, qc.ctrl.y),
                                Point2D::new(qc.end.x, qc.end.y),
                            );
                        }
                        SvgPathElement::CubicCurve(cc) => {
                            builder.cubic_bezier_to(
                                Point2D::new(cc.ctrl_1.x, cc.ctrl_1.y),
                                Point2D::new(cc.ctrl_2.x, cc.ctrl_2.y),
                                Point2D::new(cc.end.x, cc.end.y),
                            );
                        }
                    }
                }

                builder.end(p.is_closed());
            }
            SvgSimpleNode::Circle(c) => {
                builder.add_circle(
                    Point::new(c.center_x, c.center_y),
                    c.radius,
                    Winding::Positive,
                );
            }
            SvgSimpleNode::CircleHole(c) => {
                builder.add_circle(
                    Point::new(c.center_x, c.center_y),
                    c.radius,
                    Winding::Negative,
                );
            }
            SvgSimpleNode::Rect(c) => {
                builder.add_rectangle(
                    &Box2D::from_origin_and_size(
                        Point::new(c.x, c.y),
                        Size2D::new(c.width, c.height),
                    ),
                    Winding::Positive,
                );
            }
            SvgSimpleNode::RectHole(c) => {
                builder.add_rectangle(
                    &Box2D::from_origin_and_size(
                        Point::new(c.x, c.y),
                        Size2D::new(c.width, c.height),
                    ),
                    Winding::Negative,
                );
            }
        }
    }

    builder.build()
}

pub fn raw_line_intersection(p: &SvgLine, q: &SvgLine) -> Option<SvgPoint> {
    let p_min_x = p.start.x.min(p.end.x);
    let p_min_y = p.start.y.min(p.end.y);
    let p_max_x = p.start.x.max(p.end.x);
    let p_max_y = p.start.y.max(p.end.y);

    let q_min_x = q.start.x.min(q.end.x);
    let q_min_y = q.start.y.min(q.end.y);
    let q_max_x = q.start.x.max(q.end.x);
    let q_max_y = q.start.y.max(q.end.y);

    let int_min_x = p_min_x.max(q_min_x);
    let int_max_x = p_max_x.min(q_max_x);
    let int_min_y = p_min_y.max(q_min_y);
    let int_max_y = p_max_y.min(q_max_y);

    let two = 2.0;
    let mid_x = (int_min_x + int_max_x) / two;
    let mid_y = (int_min_y + int_max_y) / two;

    // condition ordinate values by subtracting midpoint
    let p1x = p.start.x - mid_x;
    let p1y = p.start.y - mid_y;
    let p2x = p.end.x - mid_x;
    let p2y = p.end.y - mid_y;
    let q1x = q.start.x - mid_x;
    let q1y = q.start.y - mid_y;
    let q2x = q.end.x - mid_x;
    let q2y = q.end.y - mid_y;

    // unrolled computation using homogeneous coordinates eqn
    let px = p1y - p2y;
    let py = p2x - p1x;
    let pw = p1x * p2y - p2x * p1y;

    let qx = q1y - q2y;
    let qy = q2x - q1x;
    let qw = q1x * q2y - q2x * q1y;

    let xw = py * qw - qy * pw;
    let yw = qx * pw - px * qw;
    let w = px * qy - qx * py;

    let x_int = xw / w;
    let y_int = yw / w;

    // check for parallel lines
    if (x_int.is_nan() || x_int.is_infinite()) || (y_int.is_nan() || y_int.is_infinite()) {
        None
    } else {
        // de-condition intersection point
        Some(SvgPoint {
            x: x_int + mid_x,
            y: y_int + mid_y,
        })
    }
}

/// By-value wrapper for raw_line_intersection (for FFI)
pub fn raw_line_intersection_byval(p: &SvgLine, q: SvgLine) -> Option<SvgPoint> {
    raw_line_intersection(p, &q)
}

pub fn svg_path_offset(p: &SvgPath, distance: f32, join: SvgLineJoin, cap: SvgLineCap) -> SvgPath {
    if distance == 0.0 {
        return p.clone();
    }

    let mut items = p.items.as_slice().to_vec();
    if let Some(mut first) = items.first() {
        items.push(first.clone());
    }

    let mut items = items
        .iter()
        .map(|l| match l {
            SvgPathElement::Line(q) => {
                let normal = match q.outwards_normal() {
                    Some(s) => SvgPoint {
                        x: s.x * distance,
                        y: s.y * distance,
                    },
                    None => return l.clone(),
                };

                SvgPathElement::Line(SvgLine {
                    start: SvgPoint {
                        x: q.start.x + normal.x,
                        y: q.start.y + normal.y,
                    },
                    end: SvgPoint {
                        x: q.end.x + normal.x,
                        y: q.end.y + normal.y,
                    },
                })
            }
            SvgPathElement::QuadraticCurve(q) => {
                let n1 = match (SvgLine {
                    start: q.start.clone(),
                    end: q.ctrl.clone(),
                }
                .outwards_normal())
                {
                    Some(s) => SvgPoint {
                        x: s.x * distance,
                        y: s.y * distance,
                    },
                    None => return l.clone(),
                };

                let n2 = match (SvgLine {
                    start: q.ctrl.clone(),
                    end: q.end.clone(),
                }
                .outwards_normal())
                {
                    Some(s) => SvgPoint {
                        x: s.x * distance,
                        y: s.y * distance,
                    },
                    None => return l.clone(),
                };

                let nl1 = SvgLine {
                    start: SvgPoint {
                        x: q.start.x + n1.x,
                        y: q.start.y + n1.y,
                    },
                    end: SvgPoint {
                        x: q.ctrl.x + n1.x,
                        y: q.ctrl.y + n1.y,
                    },
                };

                let nl2 = SvgLine {
                    start: SvgPoint {
                        x: q.ctrl.x + n2.x,
                        y: q.ctrl.y + n2.y,
                    },
                    end: SvgPoint {
                        x: q.end.x + n2.x,
                        y: q.end.y + n2.y,
                    },
                };

                let nctrl = match raw_line_intersection(&nl1, &nl2) {
                    Some(s) => s,
                    None => return l.clone(),
                };

                SvgPathElement::QuadraticCurve(SvgQuadraticCurve {
                    start: nl1.start,
                    ctrl: nctrl,
                    end: nl2.end,
                })
            }
            SvgPathElement::CubicCurve(q) => {
                let n1 = match (SvgLine {
                    start: q.start.clone(),
                    end: q.ctrl_1.clone(),
                }
                .outwards_normal())
                {
                    Some(s) => SvgPoint {
                        x: s.x * distance,
                        y: s.y * distance,
                    },
                    None => return l.clone(),
                };

                let n2 = match (SvgLine {
                    start: q.ctrl_1.clone(),
                    end: q.ctrl_2.clone(),
                }
                .outwards_normal())
                {
                    Some(s) => SvgPoint {
                        x: s.x * distance,
                        y: s.y * distance,
                    },
                    None => return l.clone(),
                };

                let n3 = match (SvgLine {
                    start: q.ctrl_2.clone(),
                    end: q.end.clone(),
                }
                .outwards_normal())
                {
                    Some(s) => SvgPoint {
                        x: s.x * distance,
                        y: s.y * distance,
                    },
                    None => return l.clone(),
                };

                let nl1 = SvgLine {
                    start: SvgPoint {
                        x: q.start.x + n1.x,
                        y: q.start.y + n1.y,
                    },
                    end: SvgPoint {
                        x: q.ctrl_1.x + n1.x,
                        y: q.ctrl_1.y + n1.y,
                    },
                };

                let nl2 = SvgLine {
                    start: SvgPoint {
                        x: q.ctrl_1.x + n2.x,
                        y: q.ctrl_1.y + n2.y,
                    },
                    end: SvgPoint {
                        x: q.ctrl_2.x + n2.x,
                        y: q.ctrl_2.y + n2.y,
                    },
                };

                let nl3 = SvgLine {
                    start: SvgPoint {
                        x: q.ctrl_2.x + n3.x,
                        y: q.ctrl_2.y + n3.y,
                    },
                    end: SvgPoint {
                        x: q.end.x + n3.x,
                        y: q.end.y + n3.y,
                    },
                };

                let nctrl_1 = match raw_line_intersection(&nl1, &nl2) {
                    Some(s) => s,
                    None => return l.clone(),
                };

                let nctrl_2 = match raw_line_intersection(&nl2, &nl3) {
                    Some(s) => s,
                    None => return l.clone(),
                };

                SvgPathElement::CubicCurve(SvgCubicCurve {
                    start: nl1.start,
                    ctrl_1: nctrl_1,
                    ctrl_2: nctrl_2,
                    end: nl3.end,
                })
            }
        })
        .collect::<Vec<_>>();

    for i in 0..items.len().saturating_sub(2) {
        let a_end_line = match items[i] {
            SvgPathElement::Line(q) => q.clone(),
            SvgPathElement::QuadraticCurve(q) => SvgLine {
                start: q.ctrl.clone(),
                end: q.end.clone(),
            },
            SvgPathElement::CubicCurve(q) => SvgLine {
                start: q.ctrl_2.clone(),
                end: q.end.clone(),
            },
        };

        let b_start_line = match items[i + 1] {
            SvgPathElement::Line(q) => q.clone(),
            SvgPathElement::QuadraticCurve(q) => SvgLine {
                start: q.ctrl.clone(),
                end: q.start.clone(),
            },
            SvgPathElement::CubicCurve(q) => SvgLine {
                start: q.ctrl_1.clone(),
                end: q.start.clone(),
            },
        };

        if let Some(intersect_pt) = raw_line_intersection(&a_end_line, &b_start_line) {
            items[i].set_last(intersect_pt.clone());
            items[i + 1].set_first(intersect_pt);
        }
    }

    items.pop();

    SvgPath {
        items: items.into(),
    }
}

fn shorten_line_end_by(line: SvgLine, distance: f32) -> SvgLine {
    let dx = line.end.x - line.start.x;
    let dy = line.end.y - line.start.y;
    let dt = (dx * dx + dy * dy).sqrt();
    let dt_short = dt - distance;

    SvgLine {
        start: line.start,
        end: SvgPoint {
            x: line.start.x + (dt_short / dt) * (dx / dt),
            y: line.start.y + (dt_short / dt) * (dx / dt),
        },
    }
}

fn shorten_line_start_by(line: SvgLine, distance: f32) -> SvgLine {
    let dx = line.end.x - line.start.x;
    let dy = line.end.y - line.start.y;
    let dt = (dx * dx + dy * dy).sqrt();
    let dt_short = dt - distance;

    SvgLine {
        start: SvgPoint {
            x: line.start.x + (1.0 - (dt_short / dt)) * (dx / dt),
            y: line.start.y + (1.0 - (dt_short / dt)) * (dx / dt),
        },
        end: line.end,
    }
}

// Creates a "bevel"
pub fn svg_path_bevel(p: &SvgPath, distance: f32) -> SvgPath {
    let mut items = p.items.as_slice().to_vec();

    // duplicate first & last items
    let first = items.first().cloned();
    let last = items.last().cloned();
    if let Some(first) = first {
        items.push(first);
    }
    items.reverse();
    if let Some(last) = last {
        items.push(last);
    }
    items.reverse();

    let mut final_items = Vec::new();
    for i in 0..items.len() {
        let a = items[i].clone();
        let b = items[i + 1].clone();
        match (a, b) {
            (SvgPathElement::Line(a), SvgPathElement::Line(b)) => {
                let a_short = shorten_line_end_by(a, distance);
                let b_short = shorten_line_start_by(b, distance);
                final_items.push(SvgPathElement::Line(a_short));
                final_items.push(SvgPathElement::CubicCurve(SvgCubicCurve {
                    start: a_short.end,
                    ctrl_1: a.end,
                    ctrl_2: b.start,
                    end: b_short.start,
                }));
                final_items.push(SvgPathElement::Line(b_short));
            }
            (other_a, other_b) => {
                final_items.push(other_a);
                final_items.push(other_b);
            }
        }
    }

    // remove first & last items again
    final_items.pop();
    final_items.reverse();
    final_items.pop();
    final_items.reverse();

    SvgPath {
        items: final_items.into(),
    }
}

fn svg_multi_polygon_to_geo(poly: &SvgMultiPolygon) -> geo::MultiPolygon {
    use geo::{Coord, Intersects, Winding};

    let linestrings = poly
        .rings
        .iter()
        .map(|p| {
            let mut p = p.clone();

            if !p.is_closed() {
                p.close();
            }

            let mut coords = p
                .items
                .iter()
                .flat_map(|p| {
                    match p {
                        SvgPathElement::Line(l) => vec![
                            Coord {
                                x: l.start.x as f64,
                                y: l.start.y as f64,
                            },
                            Coord {
                                x: l.end.x as f64,
                                y: l.end.y as f64,
                            },
                        ],
                        SvgPathElement::QuadraticCurve(l) => vec![
                            Coord {
                                x: l.start.x as f64,
                                y: l.start.y as f64,
                            },
                            Coord {
                                x: l.ctrl.x as f64,
                                y: l.ctrl.y as f64,
                            },
                            Coord {
                                x: l.end.x as f64,
                                y: l.end.y as f64,
                            },
                        ],
                        SvgPathElement::CubicCurve(l) => vec![
                            Coord {
                                x: l.start.x as f64,
                                y: l.start.y as f64,
                            },
                            Coord {
                                x: l.ctrl_1.x as f64,
                                y: l.ctrl_1.y as f64,
                            },
                            Coord {
                                x: l.ctrl_2.x as f64,
                                y: l.ctrl_2.y as f64,
                            },
                            Coord {
                                x: l.end.x as f64,
                                y: l.end.y as f64,
                            },
                        ],
                    }
                    .into_iter()
                })
                .collect::<Vec<_>>();

            coords.dedup();

            geo::LineString::new(coords)
        })
        .collect::<Vec<_>>();

    let exterior_polys = linestrings
        .iter()
        .filter(|ls| ls.is_cw())
        .cloned()
        .collect::<Vec<geo::LineString<_>>>();
    let mut interior_polys = linestrings
        .iter()
        .filter(|ls| ls.is_ccw())
        .cloned()
        .map(|p| Some(p))
        .collect::<Vec<_>>();

    let ext_int_matched = exterior_polys
        .iter()
        .map(|p| {
            let mut interiors = Vec::new();
            let p_poly = geo::Polygon::new(p.clone(), Vec::new());
            for i in interior_polys.iter_mut() {
                let cloned = match i.as_ref() {
                    Some(s) => s.clone(),
                    None => continue,
                };

                if geo::Polygon::new(cloned.clone(), Vec::new()).intersects(&p_poly) {
                    interiors.push(cloned);
                    *i = None;
                }
            }
            geo::Polygon::new(p.clone(), interiors)
        })
        .collect::<Vec<geo::Polygon<_>>>();

    geo::MultiPolygon(ext_int_matched)
}

fn linestring_to_svg_path(ls: geo::LineString<f64>) -> SvgPath {
    // TODO: bezier curves?
    SvgPath {
        items: ls
            .0
            .windows(2)
            .map(|a| {
                SvgPathElement::Line(SvgLine {
                    start: SvgPoint {
                        x: a[0].x as f32,
                        y: a[0].y as f32,
                    },
                    end: SvgPoint {
                        x: a[1].x as f32,
                        y: a[1].y as f32,
                    },
                })
            })
            .collect::<Vec<_>>()
            .into(),
    }
}

fn geo_to_svg_multipolygon(poly: geo::MultiPolygon<f64>) -> SvgMultiPolygon {
    use geo::Winding;
    SvgMultiPolygon {
        rings: poly
            .0
            .into_iter()
            .flat_map(|s| {
                let mut exterior = s.exterior().clone();
                let mut interiors = s.interiors().to_vec();
                exterior.make_cw_winding();
                for i in interiors.iter_mut() {
                    i.make_ccw_winding();
                }
                interiors.push(exterior);
                interiors.reverse();
                interiors.into_iter()
            })
            .map(|s| linestring_to_svg_path(s))
            .collect::<Vec<_>>()
            .into(),
    }
}

// TODO: produces wrong results for curve curve intersection
pub fn svg_multi_polygon_union(a: &SvgMultiPolygon, b: &SvgMultiPolygon) -> SvgMultiPolygon {
    use geo::{BooleanOps, Coord};

    let a = svg_multi_polygon_to_geo(a);
    let b = svg_multi_polygon_to_geo(b);

    let u = a.union(&b);

    geo_to_svg_multipolygon(u)
}

/// By-value wrapper for svg_multi_polygon_union (for FFI)
pub fn svg_multi_polygon_union_byval(a: &SvgMultiPolygon, b: SvgMultiPolygon) -> SvgMultiPolygon {
    svg_multi_polygon_union(a, &b)
}

pub fn svg_multi_polygon_intersection(a: &SvgMultiPolygon, b: &SvgMultiPolygon) -> SvgMultiPolygon {
    use geo::{BooleanOps, Coord};

    let a = svg_multi_polygon_to_geo(a);
    let b = svg_multi_polygon_to_geo(b);

    let u = a.intersection(&b);

    geo_to_svg_multipolygon(u)
}

/// By-value wrapper for svg_multi_polygon_intersection (for FFI)
pub fn svg_multi_polygon_intersection_byval(
    a: &SvgMultiPolygon,
    b: SvgMultiPolygon,
) -> SvgMultiPolygon {
    svg_multi_polygon_intersection(a, &b)
}

pub fn svg_multi_polygon_difference(a: &SvgMultiPolygon, b: &SvgMultiPolygon) -> SvgMultiPolygon {
    use geo::{BooleanOps, Coord};

    let a = svg_multi_polygon_to_geo(a);
    let b = svg_multi_polygon_to_geo(b);

    let u = a.difference(&b);

    geo_to_svg_multipolygon(u)
}

/// By-value wrapper for svg_multi_polygon_difference (for FFI)
pub fn svg_multi_polygon_difference_byval(
    a: &SvgMultiPolygon,
    b: SvgMultiPolygon,
) -> SvgMultiPolygon {
    svg_multi_polygon_difference(a, &b)
}

pub fn svg_multi_polygon_xor(a: &SvgMultiPolygon, b: &SvgMultiPolygon) -> SvgMultiPolygon {
    use geo::{BooleanOps, Coord};

    let a = svg_multi_polygon_to_geo(a);
    let b = svg_multi_polygon_to_geo(b);

    let u = a.xor(&b);

    geo_to_svg_multipolygon(u)
}

/// By-value wrapper for svg_multi_polygon_xor (for FFI)
pub fn svg_multi_polygon_xor_byval(a: &SvgMultiPolygon, b: SvgMultiPolygon) -> SvgMultiPolygon {
    svg_multi_polygon_xor(a, &b)
}

#[cfg(feature = "svg")]
fn svg_path_to_lyon_path_events(path: &SvgPath) -> Path {
    let mut builder = Path::builder();

    if !path.items.as_ref().is_empty() {
        let start_item = path.items.as_ref()[0];
        let first_point = Point2D::new(start_item.get_start().x, start_item.get_start().y);

        builder.begin(first_point);

        for p in path.items.as_ref().iter() {
            match p {
                SvgPathElement::Line(l) => {
                    builder.line_to(Point2D::new(l.end.x, l.end.y));
                }
                SvgPathElement::QuadraticCurve(qc) => {
                    builder.quadratic_bezier_to(
                        Point2D::new(qc.ctrl.x, qc.ctrl.y),
                        Point2D::new(qc.end.x, qc.end.y),
                    );
                }
                SvgPathElement::CubicCurve(cc) => {
                    builder.cubic_bezier_to(
                        Point2D::new(cc.ctrl_1.x, cc.ctrl_1.y),
                        Point2D::new(cc.ctrl_2.x, cc.ctrl_2.y),
                        Point2D::new(cc.end.x, cc.end.y),
                    );
                }
            }
        }

        builder.end(path.is_closed());
    }

    builder.build()
}

#[cfg(feature = "svg")]
#[inline]
fn vertex_buffers_to_tessellated_cpu_node(v: VertexBuffers<SvgVertex, u32>) -> TessellatedSvgNode {
    TessellatedSvgNode {
        vertices: v.vertices.into(),
        indices: v.indices.into(),
    }
}

#[cfg(feature = "svg")]
pub fn tessellate_multi_polygon_fill(
    polygon: &SvgMultiPolygon,
    fill_style: SvgFillStyle,
) -> TessellatedSvgNode {
    let polygon = svg_multipolygon_to_lyon_path(polygon);

    let mut geometry = VertexBuffers::new();
    let mut tessellator = FillTessellator::new();

    let tess_result = tessellator.tessellate_path(
        &polygon,
        &FillOptions::tolerance(fill_style.tolerance),
        &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex| {
            let xy_arr = vertex.position();
            SvgVertex {
                x: xy_arr.x,
                y: xy_arr.y,
            }
        }),
    );

    if let Err(_) = tess_result {
        TessellatedSvgNode::empty()
    } else {
        vertex_buffers_to_tessellated_cpu_node(geometry)
    }
}

#[cfg(not(feature = "svg"))]
pub fn tessellate_multi_polygon_fill(
    polygon: &SvgMultiPolygon,
    fill_style: SvgFillStyle,
) -> TessellatedSvgNode {
    TessellatedSvgNode::default()
}

#[cfg(feature = "svg")]
pub fn tessellate_multi_shape_fill(
    ms: &[SvgSimpleNode],
    fill_style: SvgFillStyle,
) -> TessellatedSvgNode {
    let polygon = svg_multi_shape_to_lyon_path(ms);

    let mut geometry = VertexBuffers::new();
    let mut tessellator = FillTessellator::new();

    let tess_result = tessellator.tessellate_path(
        &polygon,
        &FillOptions::tolerance(fill_style.tolerance),
        &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex| {
            let xy_arr = vertex.position();
            SvgVertex {
                x: xy_arr.x,
                y: xy_arr.y,
            }
        }),
    );

    if let Err(_) = tess_result {
        TessellatedSvgNode::empty()
    } else {
        vertex_buffers_to_tessellated_cpu_node(geometry)
    }
}

#[cfg(not(feature = "svg"))]
pub fn tessellate_multi_shape_fill(
    ms: &[SvgMultiPolygon],
    fill_style: SvgFillStyle,
) -> TessellatedSvgNode {
    TessellatedSvgNode::default()
}

pub fn svg_node_contains_point(
    node: &SvgNode,
    point: SvgPoint,
    fill_rule: SvgFillRule,
    tolerance: f32,
) -> bool {
    match node {
        SvgNode::MultiPolygonCollection(a) => a
            .as_ref()
            .iter()
            .any(|e| polygon_contains_point(e, point, fill_rule, tolerance)),
        SvgNode::MultiPolygon(a) => polygon_contains_point(a, point, fill_rule, tolerance),
        SvgNode::Path(a) => {
            if !a.is_closed() {
                return false;
            }
            path_contains_point(a, point, fill_rule, tolerance)
        }
        SvgNode::Circle(a) => a.contains_point(point.x, point.y),
        SvgNode::Rect(a) => a.contains_point(point),
        SvgNode::MultiShape(a) => a.as_ref().iter().any(|e| match e {
            SvgSimpleNode::Path(a) => {
                if !a.is_closed() {
                    return false;
                }
                path_contains_point(a, point, fill_rule, tolerance)
            }
            SvgSimpleNode::Circle(a) => a.contains_point(point.x, point.y),
            SvgSimpleNode::Rect(a) => a.contains_point(point),
            SvgSimpleNode::CircleHole(a) => !a.contains_point(point.x, point.y),
            SvgSimpleNode::RectHole(a) => !a.contains_point(point),
        }),
    }
}

#[cfg(feature = "svg")]
pub fn path_contains_point(
    path: &SvgPath,
    point: SvgPoint,
    fill_rule: SvgFillRule,
    tolerance: f32,
) -> bool {
    use lyon::{
        algorithms::hit_test::hit_test_path, math::Point as LyonPoint,
        path::FillRule as LyonFillRule,
    };
    let path = svg_path_to_lyon_path_events(path);
    let fill_rule = match fill_rule {
        SvgFillRule::Winding => LyonFillRule::NonZero,
        SvgFillRule::EvenOdd => LyonFillRule::EvenOdd,
    };
    let point = LyonPoint::new(point.x, point.y);
    hit_test_path(&point, path.iter(), fill_rule, tolerance)
}

#[cfg(not(feature = "svg"))]
pub fn path_contains_point(
    path: &SvgPath,
    point: SvgPoint,
    fill_rule: SvgFillRule,
    tolerance: f32,
) -> bool {
    false
}

#[cfg(feature = "svg")]
pub fn polygon_contains_point(
    polygon: &SvgMultiPolygon,
    point: SvgPoint,
    fill_rule: SvgFillRule,
    tolerance: f32,
) -> bool {
    use lyon::{
        algorithms::hit_test::hit_test_path, math::Point as LyonPoint,
        path::FillRule as LyonFillRule,
    };
    polygon.rings.iter().any(|path| {
        let path = svg_path_to_lyon_path_events(&path);
        let fill_rule = match fill_rule {
            SvgFillRule::Winding => LyonFillRule::NonZero,
            SvgFillRule::EvenOdd => LyonFillRule::EvenOdd,
        };
        let point = LyonPoint::new(point.x, point.y);
        hit_test_path(&point, path.iter(), fill_rule, tolerance)
    })
}

#[cfg(not(feature = "svg"))]
pub fn polygon_contains_point(
    polygon: &SvgMultiPolygon,
    point: SvgPoint,
    fill_rule: SvgFillRule,
    tolerance: f32,
) -> bool {
    false
}

#[cfg(feature = "svg")]
pub fn tessellate_multi_shape_stroke(
    ms: &[SvgSimpleNode],
    stroke_style: SvgStrokeStyle,
) -> TessellatedSvgNode {
    let stroke_options: StrokeOptions = translate_svg_stroke_style(stroke_style);
    let polygon = svg_multi_shape_to_lyon_path(ms);

    let mut stroke_geometry = VertexBuffers::new();
    let mut stroke_tess = StrokeTessellator::new();

    let tess_result = stroke_tess.tessellate_path(
        &polygon,
        &stroke_options,
        &mut BuffersBuilder::new(&mut stroke_geometry, |vertex: StrokeVertex| {
            let xy_arr = vertex.position();
            SvgVertex {
                x: xy_arr.x,
                y: xy_arr.y,
            }
        }),
    );

    if let Err(_) = tess_result {
        TessellatedSvgNode::empty()
    } else {
        vertex_buffers_to_tessellated_cpu_node(stroke_geometry)
    }
}

#[cfg(not(feature = "svg"))]
pub fn tessellate_multi_shape_stroke(
    polygon: &[SvgSimpleNode],
    stroke_style: SvgStrokeStyle,
) -> TessellatedSvgNode {
    TessellatedSvgNode::default()
}

#[cfg(feature = "svg")]
pub fn tessellate_multi_polygon_stroke(
    polygon: &SvgMultiPolygon,
    stroke_style: SvgStrokeStyle,
) -> TessellatedSvgNode {
    let stroke_options: StrokeOptions = translate_svg_stroke_style(stroke_style);
    let polygon = svg_multipolygon_to_lyon_path(polygon);

    let mut stroke_geometry = VertexBuffers::new();
    let mut stroke_tess = StrokeTessellator::new();

    let tess_result = stroke_tess.tessellate_path(
        &polygon,
        &stroke_options,
        &mut BuffersBuilder::new(&mut stroke_geometry, |vertex: StrokeVertex| {
            let xy_arr = vertex.position();
            SvgVertex {
                x: xy_arr.x,
                y: xy_arr.y,
            }
        }),
    );

    if let Err(_) = tess_result {
        TessellatedSvgNode::empty()
    } else {
        vertex_buffers_to_tessellated_cpu_node(stroke_geometry)
    }
}

#[cfg(not(feature = "svg"))]
pub fn tessellate_multi_polygon_stroke(
    polygon: &SvgMultiPolygon,
    stroke_style: SvgStrokeStyle,
) -> TessellatedSvgNode {
    TessellatedSvgNode::default()
}

#[cfg(feature = "svg")]
pub fn tessellate_path_fill(path: &SvgPath, fill_style: SvgFillStyle) -> TessellatedSvgNode {
    let polygon = svg_path_to_lyon_path_events(path);

    let mut geometry = VertexBuffers::new();
    let mut tessellator = FillTessellator::new();

    let tess_result = tessellator.tessellate_path(
        &polygon,
        &FillOptions::tolerance(fill_style.tolerance),
        &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex| {
            let xy_arr = vertex.position();
            SvgVertex {
                x: xy_arr.x,
                y: xy_arr.y,
            }
        }),
    );

    if let Err(_) = tess_result {
        TessellatedSvgNode::empty()
    } else {
        vertex_buffers_to_tessellated_cpu_node(geometry)
    }
}

#[cfg(not(feature = "svg"))]
pub fn tessellate_path_fill(path: &SvgPath, fill_style: SvgFillStyle) -> TessellatedSvgNode {
    TessellatedSvgNode::default()
}

#[cfg(feature = "svg")]
pub fn tessellate_path_stroke(path: &SvgPath, stroke_style: SvgStrokeStyle) -> TessellatedSvgNode {
    let stroke_options: StrokeOptions = translate_svg_stroke_style(stroke_style);
    let polygon = svg_path_to_lyon_path_events(path);

    let mut stroke_geometry = VertexBuffers::new();
    let mut stroke_tess = StrokeTessellator::new();

    let tess_result = stroke_tess.tessellate_path(
        &polygon,
        &stroke_options,
        &mut BuffersBuilder::new(&mut stroke_geometry, |vertex: StrokeVertex| {
            let xy_arr = vertex.position();
            SvgVertex {
                x: xy_arr.x,
                y: xy_arr.y,
            }
        }),
    );

    if let Err(_) = tess_result {
        TessellatedSvgNode::empty()
    } else {
        vertex_buffers_to_tessellated_cpu_node(stroke_geometry)
    }
}

#[cfg(not(feature = "svg"))]
pub fn tessellate_path_stroke(path: &SvgPath, stroke_style: SvgStrokeStyle) -> TessellatedSvgNode {
    TessellatedSvgNode::default()
}

#[cfg(feature = "svg")]
pub fn tessellate_circle_fill(c: &SvgCircle, fill_style: SvgFillStyle) -> TessellatedSvgNode {
    let center = Point2D::new(c.center_x, c.center_y);

    let mut geometry = VertexBuffers::new();
    let mut tesselator = FillTessellator::new();
    let tess_result = tesselator.tessellate_circle(
        center,
        c.radius,
        &FillOptions::tolerance(fill_style.tolerance),
        &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex| {
            let xy_arr = vertex.position();
            SvgVertex {
                x: xy_arr.x,
                y: xy_arr.y,
            }
        }),
    );

    if let Err(_) = tess_result {
        TessellatedSvgNode::empty()
    } else {
        vertex_buffers_to_tessellated_cpu_node(geometry)
    }
}

#[cfg(not(feature = "svg"))]
pub fn tessellate_circle_fill(c: &SvgCircle, fill_style: SvgFillStyle) -> TessellatedSvgNode {
    TessellatedSvgNode::default()
}

#[cfg(feature = "svg")]
pub fn tessellate_circle_stroke(c: &SvgCircle, stroke_style: SvgStrokeStyle) -> TessellatedSvgNode {
    let stroke_options: StrokeOptions = translate_svg_stroke_style(stroke_style);
    let center = Point2D::new(c.center_x, c.center_y);

    let mut stroke_geometry = VertexBuffers::new();
    let mut tesselator = StrokeTessellator::new();

    let tess_result = tesselator.tessellate_circle(
        center,
        c.radius,
        &stroke_options,
        &mut BuffersBuilder::new(&mut stroke_geometry, |vertex: StrokeVertex| {
            let xy_arr = vertex.position();
            SvgVertex {
                x: xy_arr.x,
                y: xy_arr.y,
            }
        }),
    );

    if let Err(_) = tess_result {
        TessellatedSvgNode::empty()
    } else {
        vertex_buffers_to_tessellated_cpu_node(stroke_geometry)
    }
}

#[cfg(not(feature = "svg"))]
pub fn tessellate_circle_stroke(c: &SvgCircle, stroke_style: SvgStrokeStyle) -> TessellatedSvgNode {
    TessellatedSvgNode::default()
}

// TODO: radii not respected on latest version of lyon
#[cfg(feature = "svg")]
fn get_radii(r: &SvgRect) -> lyon::geom::Box2D<f32> {
    let rect = lyon::geom::Box2D::from_origin_and_size(
        Point2D::new(r.x, r.y),
        Size2D::new(r.width, r.height),
    );
    /*
    let radii = BorderRadii {
        top_left: r.radius_top_left,
        top_right: r.radius_top_right,
        bottom_left: r.radius_bottom_left,
        bottom_right: r.radius_bottom_right
    };*/
    rect
}

#[cfg(feature = "svg")]
pub fn tessellate_rect_fill(r: &SvgRect, fill_style: SvgFillStyle) -> TessellatedSvgNode {
    let rect = get_radii(&r);
    let mut geometry = VertexBuffers::new();
    let mut tesselator = FillTessellator::new();

    let tess_result = tesselator.tessellate_rectangle(
        &rect,
        &FillOptions::tolerance(fill_style.tolerance),
        &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex| {
            let xy_arr = vertex.position();
            SvgVertex {
                x: xy_arr.x,
                y: xy_arr.y,
            }
        }),
    );

    if let Err(_) = tess_result {
        TessellatedSvgNode::empty()
    } else {
        vertex_buffers_to_tessellated_cpu_node(geometry)
    }
}

#[cfg(not(feature = "svg"))]
pub fn tessellate_rect_fill(r: &SvgRect, fill_style: SvgFillStyle) -> TessellatedSvgNode {
    TessellatedSvgNode::default()
}

#[cfg(feature = "svg")]
pub fn tessellate_rect_stroke(r: &SvgRect, stroke_style: SvgStrokeStyle) -> TessellatedSvgNode {
    let stroke_options: StrokeOptions = translate_svg_stroke_style(stroke_style);
    let rect = get_radii(&r);

    let mut stroke_geometry = VertexBuffers::new();
    let mut tesselator = StrokeTessellator::new();

    let tess_result = tesselator.tessellate_rectangle(
        &rect,
        &stroke_options,
        &mut BuffersBuilder::new(&mut stroke_geometry, |vertex: StrokeVertex| {
            let xy_arr = vertex.position();
            SvgVertex {
                x: xy_arr.x,
                y: xy_arr.y,
            }
        }),
    );

    if let Err(_) = tess_result {
        TessellatedSvgNode::empty()
    } else {
        vertex_buffers_to_tessellated_cpu_node(stroke_geometry)
    }
}

#[cfg(not(feature = "svg"))]
pub fn tessellate_rect_stroke(r: &SvgRect, stroke_style: SvgStrokeStyle) -> TessellatedSvgNode {
    TessellatedSvgNode::default()
}

/// Tessellate the path using lyon
#[cfg(feature = "svg")]
pub fn tessellate_styled_node(node: &SvgStyledNode) -> TessellatedSvgNode {
    match node.style {
        SvgStyle::Fill(fs) => tessellate_node_fill(&node.geometry, fs),
        SvgStyle::Stroke(ss) => tessellate_node_stroke(&node.geometry, ss),
    }
}

#[cfg(not(feature = "svg"))]
pub fn tessellate_styled_node(node: &SvgStyledNode) -> TessellatedSvgNode {
    TessellatedSvgNode::default()
}

#[cfg(feature = "svg")]
pub fn tessellate_line_stroke(
    svgline: &SvgLine,
    stroke_style: SvgStrokeStyle,
) -> TessellatedSvgNode {
    let stroke_options: StrokeOptions = translate_svg_stroke_style(stroke_style);

    let mut builder = Path::builder();
    builder.begin(Point2D::new(svgline.start.x, svgline.start.y));
    builder.line_to(Point2D::new(svgline.end.x, svgline.end.y));
    builder.end(/* closed */ false);
    let path = builder.build();

    let mut stroke_geometry = VertexBuffers::new();
    let mut stroke_tess = StrokeTessellator::new();

    let tess_result = stroke_tess.tessellate_path(
        &path,
        &stroke_options,
        &mut BuffersBuilder::new(&mut stroke_geometry, |vertex: StrokeVertex| {
            let xy_arr = vertex.position();
            SvgVertex {
                x: xy_arr.x,
                y: xy_arr.y,
            }
        }),
    );

    if let Err(_) = tess_result {
        TessellatedSvgNode::empty()
    } else {
        vertex_buffers_to_tessellated_cpu_node(stroke_geometry)
    }
}

#[cfg(not(feature = "svg"))]
pub fn tessellate_line_stroke(
    svgline: &SvgLine,
    stroke_style: SvgStrokeStyle,
) -> TessellatedSvgNode {
    TessellatedSvgNode::default()
}

#[cfg(feature = "svg")]
pub fn tessellate_cubiccurve_stroke(
    svgcubiccurve: &SvgCubicCurve,
    stroke_style: SvgStrokeStyle,
) -> TessellatedSvgNode {
    let stroke_options: StrokeOptions = translate_svg_stroke_style(stroke_style);

    let mut builder = Path::builder();
    builder.begin(Point2D::new(svgcubiccurve.start.x, svgcubiccurve.start.y));
    builder.cubic_bezier_to(
        Point2D::new(svgcubiccurve.ctrl_1.x, svgcubiccurve.ctrl_1.y),
        Point2D::new(svgcubiccurve.ctrl_2.x, svgcubiccurve.ctrl_2.y),
        Point2D::new(svgcubiccurve.end.x, svgcubiccurve.end.y),
    );
    builder.end(/* closed */ false);
    let path = builder.build();

    let mut stroke_geometry = VertexBuffers::new();
    let mut stroke_tess = StrokeTessellator::new();

    let tess_result = stroke_tess.tessellate_path(
        &path,
        &stroke_options,
        &mut BuffersBuilder::new(&mut stroke_geometry, |vertex: StrokeVertex| {
            let xy_arr = vertex.position();
            SvgVertex {
                x: xy_arr.x,
                y: xy_arr.y,
            }
        }),
    );

    if let Err(_) = tess_result {
        TessellatedSvgNode::empty()
    } else {
        vertex_buffers_to_tessellated_cpu_node(stroke_geometry)
    }
}

#[cfg(not(feature = "svg"))]
pub fn tessellate_cubiccurve_stroke(
    svgline: &SvgCubicCurve,
    stroke_style: SvgStrokeStyle,
) -> TessellatedSvgNode {
    TessellatedSvgNode::default()
}

#[cfg(feature = "svg")]
pub fn tessellate_quadraticcurve_stroke(
    svgquadraticcurve: &SvgQuadraticCurve,
    stroke_style: SvgStrokeStyle,
) -> TessellatedSvgNode {
    let stroke_options: StrokeOptions = translate_svg_stroke_style(stroke_style);

    let mut builder = Path::builder();
    builder.begin(Point2D::new(
        svgquadraticcurve.start.x,
        svgquadraticcurve.start.y,
    ));
    builder.quadratic_bezier_to(
        Point2D::new(svgquadraticcurve.ctrl.x, svgquadraticcurve.ctrl.y),
        Point2D::new(svgquadraticcurve.end.x, svgquadraticcurve.end.y),
    );
    builder.end(/* closed */ false);
    let path = builder.build();

    let mut stroke_geometry = VertexBuffers::new();
    let mut stroke_tess = StrokeTessellator::new();

    let tess_result = stroke_tess.tessellate_path(
        &path,
        &stroke_options,
        &mut BuffersBuilder::new(&mut stroke_geometry, |vertex: StrokeVertex| {
            let xy_arr = vertex.position();
            SvgVertex {
                x: xy_arr.x,
                y: xy_arr.y,
            }
        }),
    );

    if let Err(_) = tess_result {
        TessellatedSvgNode::empty()
    } else {
        vertex_buffers_to_tessellated_cpu_node(stroke_geometry)
    }
}

#[cfg(not(feature = "svg"))]
pub fn tessellate_quadraticcurve_stroke(
    svgquadraticcurve: &SvgQuadraticCurve,
    stroke_style: SvgStrokeStyle,
) -> TessellatedSvgNode {
    TessellatedSvgNode::default()
}

#[cfg(feature = "svg")]
pub fn tessellate_svgpathelement_stroke(
    svgpathelement: &SvgPathElement,
    stroke_style: SvgStrokeStyle,
) -> TessellatedSvgNode {
    match svgpathelement {
        SvgPathElement::Line(l) => tessellate_line_stroke(l, stroke_style),
        SvgPathElement::QuadraticCurve(l) => tessellate_quadraticcurve_stroke(l, stroke_style),
        SvgPathElement::CubicCurve(l) => tessellate_cubiccurve_stroke(l, stroke_style),
    }
}

#[cfg(not(feature = "svg"))]
pub fn tessellate_svgpathelement_stroke(
    svgpathelement: &SvgPathElement,
    stroke_style: SvgStrokeStyle,
) -> TessellatedSvgNode {
    TessellatedSvgNode::default()
}

#[cfg(feature = "svg")]
pub fn join_tessellated_nodes(nodes: &[TessellatedSvgNode]) -> TessellatedSvgNode {
    let mut index_offset = 0;

    // note: can not be parallelized!
    let all_index_offsets = nodes
        .as_ref()
        .iter()
        .map(|t| {
            let i = index_offset;
            index_offset += t.vertices.len();
            i
        })
        .collect::<Vec<_>>();

    let all_vertices = nodes
        .as_ref()
        .iter()
        .flat_map(|t| t.vertices.clone().into_library_owned_vec())
        .collect::<Vec<_>>();

    let all_indices = nodes
        .as_ref()
        .iter()
        .enumerate()
        .flat_map(|(buffer_index, t)| {
            // since the vertex buffers are now joined,
            // offset the indices by the vertex buffers lengths
            // encountered so far
            let vertex_buffer_offset: u32 = all_index_offsets
                .get(buffer_index)
                .copied()
                .unwrap_or(0)
                .min(core::u32::MAX as usize) as u32;

            let mut indices = t.indices.clone().into_library_owned_vec();
            if vertex_buffer_offset != 0 {
                indices.iter_mut().for_each(|i| {
                    if *i != GL_RESTART_INDEX {
                        *i += vertex_buffer_offset;
                    }
                });
            }

            indices.push(GL_RESTART_INDEX);

            indices
        })
        .collect::<Vec<_>>();

    TessellatedSvgNode {
        vertices: all_vertices.into(),
        indices: all_indices.into(),
    }
}

#[cfg(feature = "svg")]
pub fn join_tessellated_colored_nodes(
    nodes: &[TessellatedColoredSvgNode],
) -> TessellatedColoredSvgNode {
    let mut index_offset = 0;

    // note: can not be parallelized!
    let all_index_offsets = nodes
        .as_ref()
        .iter()
        .map(|t| {
            let i = index_offset;
            index_offset += t.vertices.len();
            i
        })
        .collect::<Vec<_>>();

    let all_vertices = nodes
        .as_ref()
        .iter()
        .flat_map(|t| t.vertices.clone().into_library_owned_vec())
        .collect::<Vec<_>>();

    let all_indices = nodes
        .as_ref()
        .iter()
        .enumerate()
        .flat_map(|(buffer_index, t)| {
            // since the vertex buffers are now joined,
            // offset the indices by the vertex buffers lengths
            // encountered so far
            let vertex_buffer_offset: u32 = all_index_offsets
                .get(buffer_index)
                .copied()
                .unwrap_or(0)
                .min(core::u32::MAX as usize) as u32;

            let mut indices = t.indices.clone().into_library_owned_vec();
            if vertex_buffer_offset != 0 {
                indices.iter_mut().for_each(|i| {
                    if *i != GL_RESTART_INDEX {
                        *i += vertex_buffer_offset;
                    }
                });
            }

            indices.push(GL_RESTART_INDEX);

            indices
        })
        .collect::<Vec<_>>();

    TessellatedColoredSvgNode {
        vertices: all_vertices.into(),
        indices: all_indices.into(),
    }
}

#[cfg(not(feature = "svg"))]
pub fn join_tessellated_nodes(nodes: &[TessellatedSvgNode]) -> TessellatedSvgNode {
    TessellatedSvgNode::default()
}

#[cfg(not(feature = "svg"))]
pub fn join_tessellated_colored_nodes(
    nodes: &[TessellatedColoredSvgNode],
) -> TessellatedColoredSvgNode {
    TessellatedColoredSvgNode::default()
}

#[cfg(feature = "svg")]
pub fn tessellate_node_fill(node: &SvgNode, fs: SvgFillStyle) -> TessellatedSvgNode {
    match &node {
        SvgNode::MultiPolygonCollection(ref mpc) => {
            let tessellated_multipolygons = mpc
                .as_ref()
                .iter()
                .map(|mp| tessellate_multi_polygon_fill(mp, fs))
                .collect::<Vec<_>>();
            join_tessellated_nodes(&tessellated_multipolygons)
        }
        SvgNode::MultiPolygon(ref mp) => tessellate_multi_polygon_fill(mp, fs),
        SvgNode::Path(ref p) => tessellate_path_fill(p, fs),
        SvgNode::Circle(ref c) => tessellate_circle_fill(c, fs),
        SvgNode::Rect(ref r) => tessellate_rect_fill(r, fs),
        SvgNode::MultiShape(ref r) => tessellate_multi_shape_fill(r.as_ref(), fs),
    }
}

#[cfg(not(feature = "svg"))]
pub fn tessellate_node_fill(node: &SvgNode, fs: SvgFillStyle) -> TessellatedSvgNode {
    TessellatedSvgNode::default()
}

#[cfg(feature = "svg")]
pub fn tessellate_node_stroke(node: &SvgNode, ss: SvgStrokeStyle) -> TessellatedSvgNode {
    match &node {
        SvgNode::MultiPolygonCollection(ref mpc) => {
            let tessellated_multipolygons = mpc
                .as_ref()
                .iter()
                .map(|mp| tessellate_multi_polygon_stroke(mp, ss))
                .collect::<Vec<_>>();
            let mut all_vertices = Vec::new();
            let mut all_indices = Vec::new();
            for TessellatedSvgNode { vertices, indices } in tessellated_multipolygons {
                let mut vertices: Vec<SvgVertex> = vertices.into_library_owned_vec();
                let mut indices: Vec<u32> = indices.into_library_owned_vec();
                all_vertices.append(&mut vertices);
                all_indices.append(&mut indices);
                all_indices.push(GL_RESTART_INDEX);
            }
            TessellatedSvgNode {
                vertices: all_vertices.into(),
                indices: all_indices.into(),
            }
        }
        SvgNode::MultiPolygon(ref mp) => tessellate_multi_polygon_stroke(mp, ss),
        SvgNode::Path(ref p) => tessellate_path_stroke(p, ss),
        SvgNode::Circle(ref c) => tessellate_circle_stroke(c, ss),
        SvgNode::Rect(ref r) => tessellate_rect_stroke(r, ss),
        SvgNode::MultiShape(ms) => tessellate_multi_shape_stroke(ms.as_ref(), ss),
    }
}

#[cfg(not(feature = "svg"))]
pub fn tessellate_node_stroke(node: &SvgNode, ss: SvgStrokeStyle) -> TessellatedSvgNode {
    TessellatedSvgNode::default()
}

// NOTE: This is a separate step both in order to reuse GPU textures
// and also because texture allocation is heavy and can be offloaded to a different thread
pub fn allocate_clipmask_texture(
    gl_context: GlContextPtr,
    size: PhysicalSizeU32,
    _background: ColorU,
) -> Texture {
    use azul_core::gl::TextureFlags;

    let textures = gl_context.gen_textures(1);
    let texture_id = textures.get(0).unwrap();

    Texture::create(
        *texture_id,
        TextureFlags {
            is_opaque: true,
            is_video_texture: false,
        },
        size,
        ColorU::TRANSPARENT,
        gl_context,
        RawImageFormat::R8,
    )
}

/// Applies an FXAA filter to the texture using the pre-compiled FXAA shader.
///
/// Renders a fullscreen quad with the FXAA fragment shader, reading from
/// the input texture and writing to a temporary texture, then swaps the
/// texture IDs so the caller gets the post-FXAA result.
pub fn apply_fxaa(texture: &mut Texture) -> Option<()> {
    apply_fxaa_with_config(texture, azul_core::gl_fxaa::FxaaConfig::enabled())
}

/// Applies FXAA with custom configuration parameters.
pub fn apply_fxaa_with_config(
    texture: &mut Texture,
    config: azul_core::gl_fxaa::FxaaConfig,
) -> Option<()> {
    use std::mem;

    use azul_core::gl::{GLuint, GlVoidPtrConst, VertexAttributeType};
    use gl_context_loader::gl;

    if !config.enabled || texture.size.width == 0 || texture.size.height == 0 {
        return Some(());
    }

    // FXAA only works on RGBA8 textures
    if texture.format != RawImageFormat::RGBA8 {
        return Some(());
    }

    let texture_size = texture.size;
    let gl_context = &texture.gl_context;
    let fxaa_shader = gl_context.get_fxaa_shader();
    let w = texture_size.width as f32;
    let h = texture_size.height as f32;

    // Save GL state
    let mut current_program = [0_i32];
    let mut current_framebuffers = [0_i32];
    let mut current_texture_2d = [0_i32];
    let mut current_vertex_array_object = [0_i32];
    let mut current_vertex_buffer = [0_i32];
    let mut current_index_buffer = [0_i32];
    let mut current_active_texture = [0_i32];
    let mut current_blend_enabled = [0_u8];
    let mut current_viewport = [0_i32; 4];

    gl_context.get_integer_v(gl::CURRENT_PROGRAM, (&mut current_program[..]).into());
    gl_context.get_integer_v(gl::FRAMEBUFFER, (&mut current_framebuffers[..]).into());
    gl_context.get_integer_v(gl::TEXTURE_2D, (&mut current_texture_2d[..]).into());
    gl_context.get_integer_v(
        gl::VERTEX_ARRAY_BINDING,
        (&mut current_vertex_array_object[..]).into(),
    );
    gl_context.get_integer_v(
        gl::ARRAY_BUFFER_BINDING,
        (&mut current_vertex_buffer[..]).into(),
    );
    gl_context.get_integer_v(
        gl::ELEMENT_ARRAY_BUFFER_BINDING,
        (&mut current_index_buffer[..]).into(),
    );
    gl_context.get_integer_v(
        gl::ACTIVE_TEXTURE,
        (&mut current_active_texture[..]).into(),
    );
    gl_context.get_boolean_v(gl::BLEND, (&mut current_blend_enabled[..]).into());
    gl_context.get_integer_v(gl::VIEWPORT, (&mut current_viewport[..]).into());

    // 1. Create temporary output texture
    let temp_textures = gl_context.gen_textures(1);
    let temp_tex_id = *temp_textures.get(0)?;
    gl_context.bind_texture(gl::TEXTURE_2D, temp_tex_id);
    gl_context.tex_image_2d(
        gl::TEXTURE_2D,
        0,
        gl::RGBA as i32,
        texture_size.width as i32,
        texture_size.height as i32,
        0,
        gl::RGBA,
        gl::UNSIGNED_BYTE,
        None.into(),
    );
    gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as i32);
    gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as i32);
    gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
    gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);

    // 2. Create FBO targeting the temp texture
    let fbos = gl_context.gen_framebuffers(1);
    let fbo_id = *fbos.get(0)?;
    gl_context.bind_framebuffer(gl::FRAMEBUFFER, fbo_id);
    gl_context.framebuffer_texture_2d(
        gl::FRAMEBUFFER,
        gl::COLOR_ATTACHMENT0,
        gl::TEXTURE_2D,
        temp_tex_id,
        0,
    );
    gl_context.draw_buffers([gl::COLOR_ATTACHMENT0][..].into());

    debug_assert!(
        gl_context.check_frame_buffer_status(gl::FRAMEBUFFER) == gl::FRAMEBUFFER_COMPLETE
    );

    // 3. Create fullscreen quad VAO/VBO/IBO
    // Vertices in [-1, 1] range; the FXAA vertex shader converts to [0, 1] UVs
    let quad_vertices: [f32; 8] = [
        -1.0, -1.0, // bottom-left
         1.0, -1.0, // bottom-right
         1.0,  1.0, // top-right
        -1.0,  1.0, // top-left
    ];
    let quad_indices: [u32; 6] = [0, 1, 2, 0, 2, 3];

    let vaos = gl_context.gen_vertex_arrays(1);
    let vao_id = *vaos.get(0)?;
    gl_context.bind_vertex_array(vao_id);

    let vbos = gl_context.gen_buffers(1);
    let vbo_id = *vbos.get(0)?;
    gl_context.bind_buffer(gl::ARRAY_BUFFER, vbo_id);
    gl_context.buffer_data_untyped(
        gl::ARRAY_BUFFER,
        (mem::size_of::<f32>() * quad_vertices.len()) as isize,
        GlVoidPtrConst {
            ptr: quad_vertices.as_ptr() as *const std::ffi::c_void,
            run_destructor: true,
        },
        gl::STATIC_DRAW,
    );

    let ibos = gl_context.gen_buffers(1);
    let ibo_id = *ibos.get(0)?;
    gl_context.bind_buffer(gl::ELEMENT_ARRAY_BUFFER, ibo_id);
    gl_context.buffer_data_untyped(
        gl::ELEMENT_ARRAY_BUFFER,
        (mem::size_of::<u32>() * quad_indices.len()) as isize,
        GlVoidPtrConst {
            ptr: quad_indices.as_ptr() as *const std::ffi::c_void,
            run_destructor: true,
        },
        gl::STATIC_DRAW,
    );

    // Set up vertex attribute for vAttrXY (location 0, bound at shader compilation)
    let vertex_type = VertexAttributeType::Float;
    let stride = vertex_type.get_mem_size() * 2; // 2 floats per vertex (x, y)
    gl_context.vertex_attrib_pointer(0, 2, vertex_type.get_gl_id(), false, stride as i32, 0);
    gl_context.enable_vertex_attrib_array(0);

    // 4. Render FXAA pass
    gl_context.use_program(fxaa_shader);
    gl_context.viewport(0, 0, texture_size.width as i32, texture_size.height as i32);
    gl_context.disable(gl::BLEND); // FXAA reads exact colors, blending would corrupt output

    // Bind input texture to GL_TEXTURE0
    gl_context.active_texture(gl::TEXTURE0);
    gl_context.bind_texture(gl::TEXTURE_2D, texture.texture_id);

    // Set uniforms
    let u_texture = gl_context.get_uniform_location(fxaa_shader, "uTexture");
    gl_context.uniform_1i(u_texture, 0);

    let u_texel_size = gl_context.get_uniform_location(fxaa_shader, "uTexelSize");
    gl_context.uniform_2f(u_texel_size, 1.0 / w, 1.0 / h);

    let u_edge_threshold =
        gl_context.get_uniform_location(fxaa_shader, "uEdgeThreshold");
    gl_context.uniform_1f(u_edge_threshold, config.edge_threshold);

    let u_edge_threshold_min =
        gl_context.get_uniform_location(fxaa_shader, "uEdgeThresholdMin");
    gl_context.uniform_1f(u_edge_threshold_min, config.edge_threshold_min);

    // Draw the fullscreen quad
    gl_context.draw_elements(gl::TRIANGLES, 6, gl::UNSIGNED_INT, 0);

    // 5. Swap texture IDs: the temp texture now has the FXAA result.
    // We swap so the caller's texture_id points to the anti-aliased result,
    // and the old texture_id gets cleaned up.
    let old_texture_id = texture.texture_id;
    texture.texture_id = temp_tex_id;
    // Delete the old texture (which was the input)
    gl_context.delete_textures((&[old_texture_id])[..].into());

    // 6. Cleanup: delete FBO, quad buffers
    gl_context.delete_framebuffers((&[fbo_id])[..].into());
    gl_context.disable_vertex_attrib_array(0);
    gl_context.delete_vertex_arrays((&[vao_id])[..].into());
    gl_context.delete_buffers((&[vbo_id, ibo_id])[..].into());

    // Restore GL state
    gl_context.bind_framebuffer(gl::FRAMEBUFFER, current_framebuffers[0] as u32);
    gl_context.bind_texture(gl::TEXTURE_2D, current_texture_2d[0] as u32);
    gl_context.bind_vertex_array(current_vertex_array_object[0] as u32);
    gl_context.bind_buffer(gl::ELEMENT_ARRAY_BUFFER, current_index_buffer[0] as u32);
    gl_context.bind_buffer(gl::ARRAY_BUFFER, current_vertex_buffer[0] as u32);
    gl_context.use_program(current_program[0] as u32);
    gl_context.active_texture(current_active_texture[0] as u32);
    gl_context.viewport(
        current_viewport[0],
        current_viewport[1],
        current_viewport[2],
        current_viewport[3],
    );
    if u32::from(current_blend_enabled[0]) == gl::TRUE {
        gl_context.enable(gl::BLEND);
    }

    Some(())
}

#[cfg(feature = "svg")]
pub fn render_node_clipmask_cpu(
    image: &mut RawImage,
    node: &SvgNode,
    style: SvgStyle,
) -> Option<()> {
    use azul_core::resources::RawImageData;
    use agg_rust::{
        basics::{FillingRule, VertexSource, PATH_FLAGS_NONE},
        path_storage::PathStorage,
        color::Rgba8,
        conv_stroke::ConvStroke,
        conv_transform::ConvTransform,
        math_stroke::{LineCap, LineJoin},
        pixfmt_rgba::{PixfmtRgba32, PixelFormat},
        rasterizer_scanline_aa::RasterizerScanlineAa,
        renderer_base::RendererBase,
        renderer_scanline::render_scanlines_aa_solid,
        rendering_buffer::RowAccessor,
        scanline_u::ScanlineU8,
        trans_affine::TransAffine,
    };

    fn agg_translate_node(node: &SvgNode) -> Option<PathStorage> {
        macro_rules! build_path {
            ($path:expr, $p:expr) => {{
                if $p.items.as_ref().is_empty() {
                    return None;
                }

                let start = $p.items.as_ref()[0].get_start();
                $path.move_to(start.x as f64, start.y as f64);

                for path_element in $p.items.as_ref() {
                    match path_element {
                        SvgPathElement::Line(l) => {
                            $path.line_to(l.end.x as f64, l.end.y as f64);
                        }
                        SvgPathElement::QuadraticCurve(qc) => {
                            $path.curve3(
                                qc.ctrl.x as f64, qc.ctrl.y as f64,
                                qc.end.x as f64, qc.end.y as f64,
                            );
                        }
                        SvgPathElement::CubicCurve(cc) => {
                            $path.curve4(
                                cc.ctrl_1.x as f64, cc.ctrl_1.y as f64,
                                cc.ctrl_2.x as f64, cc.ctrl_2.y as f64,
                                cc.end.x as f64, cc.end.y as f64,
                            );
                        }
                    }
                }

                if $p.is_closed() {
                    $path.close_polygon(PATH_FLAGS_NONE);
                }
            }};
        }

        let mut path = PathStorage::new();
        match node {
            SvgNode::MultiPolygonCollection(mpc) => {
                for mp in mpc.iter() {
                    for p in mp.rings.iter() {
                        build_path!(path, p);
                    }
                }
            }
            SvgNode::MultiPolygon(mp) => {
                for p in mp.rings.iter() {
                    build_path!(path, p);
                }
            }
            SvgNode::Path(p) => {
                build_path!(path, p);
            }
            SvgNode::Circle(c) => {
                // Approximate circle with 4 cubic beziers
                let cx = c.center_x as f64;
                let cy = c.center_y as f64;
                let r = c.radius as f64;
                let k = 0.5522847498; // 4/3 * (sqrt(2) - 1)
                let kr = k * r;
                path.move_to(cx + r, cy);
                path.curve4(cx + r, cy + kr, cx + kr, cy + r, cx, cy + r);
                path.curve4(cx - kr, cy + r, cx - r, cy + kr, cx - r, cy);
                path.curve4(cx - r, cy - kr, cx - kr, cy - r, cx, cy - r);
                path.curve4(cx + kr, cy - r, cx + r, cy - kr, cx + r, cy);
                path.close_polygon(PATH_FLAGS_NONE);
            }
            SvgNode::Rect(r) => {
                let x = r.x as f64;
                let y = r.y as f64;
                let w = r.width as f64;
                let h = r.height as f64;
                path.move_to(x, y);
                path.line_to(x + w, y);
                path.line_to(x + w, y + h);
                path.line_to(x, y + h);
                path.close_polygon(PATH_FLAGS_NONE);
            }
            SvgNode::MultiShape(ms) => {
                for p in ms.as_ref() {
                    match p {
                        SvgSimpleNode::Path(p) => {
                            build_path!(path, p);
                        }
                        SvgSimpleNode::Rect(r) => {
                            let x = r.x as f64;
                            let y = r.y as f64;
                            let w = r.width as f64;
                            let h = r.height as f64;
                            path.move_to(x, y);
                            path.line_to(x + w, y);
                            path.line_to(x + w, y + h);
                            path.line_to(x, y + h);
                            path.close_polygon(PATH_FLAGS_NONE);
                        }
                        SvgSimpleNode::Circle(c) | SvgSimpleNode::CircleHole(c) => {
                            let cx = c.center_x as f64;
                            let cy = c.center_y as f64;
                            let r = c.radius as f64;
                            let k = 0.5522847498_f64;
                            let kr = k * r;
                            path.move_to(cx + r, cy);
                            path.curve4(cx + r, cy + kr, cx + kr, cy + r, cx, cy + r);
                            path.curve4(cx - kr, cy + r, cx - r, cy + kr, cx - r, cy);
                            path.curve4(cx - r, cy - kr, cx - kr, cy - r, cx, cy - r);
                            path.curve4(cx + kr, cy - r, cx + r, cy - kr, cx + r, cy);
                            path.close_polygon(PATH_FLAGS_NONE);
                        }
                        SvgSimpleNode::RectHole(r) => {
                            let x = r.x as f64;
                            let y = r.y as f64;
                            let w = r.width as f64;
                            let h = r.height as f64;
                            path.move_to(x, y);
                            path.line_to(x + w, y);
                            path.line_to(x + w, y + h);
                            path.line_to(x, y + h);
                            path.close_polygon(PATH_FLAGS_NONE);
                        }
                    }
                }
            }
        }
        if path.total_vertices() == 0 {
            return None;
        }
        Some(path)
    }

    let w = image.width as u32;
    let h = image.height as u32;
    if w == 0 || h == 0 {
        return None;
    }

    let transform_data = style.get_transform();
    let transform = TransAffine::new_custom(
        transform_data.sx as f64,
        transform_data.ky as f64,
        transform_data.kx as f64,
        transform_data.sy as f64,
        transform_data.tx as f64,
        transform_data.ty as f64,
    );

    let mut agg_path = agg_translate_node(node)?;
    let white = Rgba8::new(255, 255, 255, 255);

    // Create pixel buffer and render
    let mut buf = vec![0u8; (w as usize) * (h as usize) * 4];
    let stride = (w * 4) as i32;
    let mut ra = unsafe { RowAccessor::new_with_buf(buf.as_mut_ptr(), w, h, stride) };
    let mut pf = PixfmtRgba32::new(&mut ra);
    let mut rb = RendererBase::new(pf);
    let mut ras = RasterizerScanlineAa::new();
    let mut sl = ScanlineU8::new();

    match style {
        SvgStyle::Fill(fs) => {
            ras.filling_rule(match fs.fill_rule {
                SvgFillRule::Winding => FillingRule::NonZero,
                SvgFillRule::EvenOdd => FillingRule::EvenOdd,
            });
            if transform.is_identity(0.0001) {
                ras.add_path(&mut agg_path, 0);
            } else {
                let mut transformed = ConvTransform::new(&mut agg_path, transform);
                ras.add_path(&mut transformed, 0);
            }
            render_scanlines_aa_solid(&mut ras, &mut sl, &mut rb, &white);
        }
        SvgStyle::Stroke(ss) => {
            let mut stroke = ConvStroke::new(agg_path);
            stroke.set_width(ss.line_width as f64);
            stroke.set_miter_limit(ss.miter_limit as f64);
            stroke.set_line_cap(match ss.start_cap {
                SvgLineCap::Butt => LineCap::Butt,
                SvgLineCap::Square => LineCap::Square,
                SvgLineCap::Round => LineCap::Round,
            });
            stroke.set_line_join(match ss.line_join {
                SvgLineJoin::Miter | SvgLineJoin::MiterClip => LineJoin::Miter,
                SvgLineJoin::Round => LineJoin::Round,
                SvgLineJoin::Bevel => LineJoin::Bevel,
            });
            if transform.is_identity(0.0001) {
                ras.add_path(&mut stroke, 0);
            } else {
                let mut transformed = ConvTransform::new(&mut stroke, transform);
                ras.add_path(&mut transformed, 0);
            }
            render_scanlines_aa_solid(&mut ras, &mut sl, &mut rb, &white);
        }
    }

    // Extract red channel from RGBA buffer
    let red_channel = buf
        .chunks_exact(4)
        .map(|r| r[0])
        .collect::<Vec<_>>();

    image.premultiplied_alpha = true;
    image.pixels = RawImageData::U8(red_channel.into());
    image.data_format = RawImageFormat::R8;

    Some(())
}

#[cfg(not(feature = "svg"))]
pub fn render_node_clipmask_cpu(
    image: &mut RawImage,
    node: &SvgNode,
    style: SvgStyle,
) -> Option<()> {
    None
}

// ---------------------------- SVG RENDERING

#[cfg(feature = "svg")]
#[derive(Debug)]
#[repr(C)]
pub struct ParsedSvgXmlNode {
    node: Box<usvg::Group>, // usvg::Node
    pub run_destructor: bool,
}

#[cfg(feature = "svg")]
impl Clone for ParsedSvgXmlNode {
    fn clone(&self) -> Self {
        Self {
            node: self.node.clone(),
            run_destructor: true,
        }
    }
}

#[cfg(feature = "svg")]
impl Drop for ParsedSvgXmlNode {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}

#[cfg(not(feature = "svg"))]
pub use azul_core::svg::SvgXmlNode;

#[cfg(feature = "svg")]
fn svgxmlnode_new(node: usvg::Group) -> ParsedSvgXmlNode {
    ParsedSvgXmlNode {
        node: Box::new(node),
        run_destructor: true,
    }
}

#[cfg(feature = "svg")]
pub fn svgxmlnode_parse(
    svg_file_data: &[u8],
    options: SvgParseOptions,
) -> Result<ParsedSvgXmlNode, SvgParseError> {
    let svg = svg_parse(svg_file_data, options)?;
    Ok(svg_root(&svg))
}

#[cfg(not(feature = "svg"))]
pub fn svgxmlnode_parse(
    svg_file_data: &[u8],
    options: SvgParseOptions,
) -> Result<ParsedSvgXmlNode, SvgParseError> {
    Err(SvgParseError::NoParserAvailable)
}

/*
#[cfg(feature = "svg")]
pub fn svgxmlnode_from_xml(xml: Xml) -> Result<Self, SvgParseError> {
    // https://github.com/RazrFalcon/resvg/issues/308
    Ok(Svg::new(xml.into_tree()))
}
*/

#[cfg(feature = "svg")]
#[repr(C)]
pub struct ParsedSvg {
    tree: Box<usvg::Tree>, // *mut usvg::Tree,
    pub run_destructor: bool,
}

#[cfg(feature = "svg")]
impl Clone for ParsedSvg {
    fn clone(&self) -> Self {
        Self {
            tree: self.tree.clone(),
            run_destructor: true,
        }
    }
}

#[cfg(feature = "svg")]
impl Drop for ParsedSvg {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}

#[cfg(feature = "svg")]
impl_result!(
    ParsedSvg,
    SvgParseError,
    ResultParsedSvgSvgParseError,
    copy = false,
    [Debug, Clone]
);

#[cfg(not(feature = "svg"))]
pub use azul_core::svg::Svg;

#[cfg(feature = "svg")]
impl From<ParsedSvg> for azul_core::svg::Svg {
    fn from(mut parsed: ParsedSvg) -> Self {
        // Use ManuallyDrop to prevent the ParsedSvg destructor from running
        // while still allowing us to move out the tree
        let mut parsed = core::mem::ManuallyDrop::new(parsed);
        // Take ownership of the tree by replacing it with a dummy
        let tree = unsafe { core::ptr::read(&parsed.tree) };
        let tree_ptr = Box::into_raw(tree) as *const azul_core::svg::c_void;
        Self {
            tree: tree_ptr,
            run_destructor: true,
        }
    }
}

#[cfg(feature = "svg")]
impl fmt::Debug for ParsedSvg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        svg_to_string(&self, SvgXmlOptions::default()).fmt(f)
    }
}

#[cfg(feature = "svg")]
impl ParsedSvg {
    /// Parses an SVG from a string
    pub fn from_string(
        svg_string: &str,
        parse_options: SvgParseOptions,
    ) -> Result<Self, SvgParseError> {
        svg_parse(svg_string.as_bytes(), parse_options)
    }

    /// Parses an SVG from bytes
    pub fn from_bytes(
        svg_bytes: &[u8],
        parse_options: SvgParseOptions,
    ) -> Result<Self, SvgParseError> {
        svg_parse(svg_bytes, parse_options)
    }

    /// Returns the root XML node of the SVG
    pub fn get_root(&self) -> ParsedSvgXmlNode {
        svg_root(self)
    }

    /// Renders the SVG to a raw image
    pub fn render(&self, options: SvgRenderOptions) -> Option<RawImage> {
        svg_render(self, options)
    }

    /// Converts the SVG back to a string
    pub fn to_string(&self, options: SvgXmlOptions) -> String {
        svg_to_string(self, options)
    }
}

#[cfg(feature = "svg")]
fn svg_new(tree: usvg::Tree) -> ParsedSvg {
    ParsedSvg {
        tree: Box::new(tree),
        run_destructor: true,
    }
}

/// NOTE: SVG file data may be Zlib compressed
#[cfg(feature = "svg")]
pub fn svg_parse(
    svg_file_data: &[u8],
    options: SvgParseOptions,
) -> Result<ParsedSvg, SvgParseError> {
    let rtree = usvg::Tree::from_data(svg_file_data, &translate_to_usvg_parseoptions(options))
        .map_err(translate_usvg_svgparserror)?;

    Ok(svg_new(rtree))
}

#[cfg(not(feature = "svg"))]
pub fn svg_parse(
    svg_file_data: &[u8],
    options: SvgParseOptions,
) -> Result<ParsedSvg, SvgParseError> {
    Err(SvgParseError::NoParserAvailable)
}

#[cfg(feature = "svg")]
pub fn svg_root(s: &ParsedSvg) -> ParsedSvgXmlNode {
    svgxmlnode_new(s.tree.root().clone())
}

#[cfg(not(feature = "svg"))]
pub fn svg_root(s: &ParsedSvg) -> ParsedSvgXmlNode {
    ParsedSvgXmlNode {
        node: core::ptr::null_mut(),
        run_destructor: false,
    }
}

#[cfg(feature = "svg")]
pub fn svg_render(s: &ParsedSvg, options: SvgRenderOptions) -> Option<RawImage> {
    use azul_core::resources::RawImageData;

    let root = s.tree.root();
    let (target_width, target_height) = svgrenderoptions_get_width_height_node(&options, &root)?;

    if target_height == 0 || target_width == 0 {
        return None;
    }

    // Walk the usvg tree and render paths using agg-rust
    let mut buf = vec![0u8; (target_width as usize) * (target_height as usize) * 4];

    // Fill background
    if let Some(bg) = options.background_color.into_option() {
        for chunk in buf.chunks_exact_mut(4) {
            chunk[0] = bg.r;
            chunk[1] = bg.g;
            chunk[2] = bg.b;
            chunk[3] = bg.a;
        }
    }

    // Render the usvg tree using agg-rust
    render_usvg_tree_to_buffer(&s.tree, &mut buf, target_width, target_height, options.transform);

    Some(RawImage {
        tag: Vec::new().into(),
        pixels: RawImageData::U8(buf.into()),
        width: target_width as usize,
        height: target_height as usize,
        premultiplied_alpha: true,
        data_format: RawImageFormat::RGBA8,
    })
}

/// Render a usvg tree into an RGBA buffer using agg-rust.
#[cfg(feature = "svg")]
fn render_usvg_tree_to_buffer(
    tree: &usvg::Tree,
    buf: &mut [u8],
    width: u32,
    height: u32,
    transform: SvgRenderTransform,
) {
    use agg_rust::{
        basics::{FillingRule, VertexSource, PATH_FLAGS_NONE},
        path_storage::PathStorage,
        color::Rgba8,
        conv_stroke::ConvStroke,
        conv_transform::ConvTransform,
        math_stroke::{LineCap, LineJoin},
        pixfmt_rgba::{PixfmtRgba32, PixelFormat},
        rasterizer_scanline_aa::RasterizerScanlineAa,
        renderer_base::RendererBase,
        renderer_scanline::render_scanlines_aa_solid,
        rendering_buffer::RowAccessor,
        scanline_u::ScanlineU8,
        trans_affine::TransAffine,
    };

    let root_transform = TransAffine::new_custom(
        transform.sx as f64,
        transform.ky as f64,
        transform.kx as f64,
        transform.sy as f64,
        transform.tx as f64,
        transform.ty as f64,
    );

    // Walk groups recursively
    fn render_group(
        group: &usvg::Group,
        buf: &mut [u8],
        width: u32,
        height: u32,
        parent_transform: &TransAffine,
    ) {
        let gt = group.transform();
        let group_transform = {
            let mut t = TransAffine::new_custom(
                gt.sx as f64, gt.ky as f64, gt.kx as f64,
                gt.sy as f64, gt.tx as f64, gt.ty as f64,
            );
            t.premultiply(parent_transform);
            t
        };

        for child in group.children() {
            match child {
                usvg::Node::Group(ref g) => {
                    render_group(g, buf, width, height, &group_transform);
                }
                usvg::Node::Path(ref p) => {
                    // Convert usvg path to agg PathStorage
                    let mut path = PathStorage::new();
                    for seg in p.data().segments() {
                        match seg {
                            usvg::tiny_skia_path::PathSegment::MoveTo(pt) => {
                                path.move_to(pt.x as f64, pt.y as f64);
                            }
                            usvg::tiny_skia_path::PathSegment::LineTo(pt) => {
                                path.line_to(pt.x as f64, pt.y as f64);
                            }
                            usvg::tiny_skia_path::PathSegment::QuadTo(p1, p2) => {
                                path.curve3(p1.x as f64, p1.y as f64, p2.x as f64, p2.y as f64);
                            }
                            usvg::tiny_skia_path::PathSegment::CubicTo(p1, p2, p3) => {
                                path.curve4(
                                    p1.x as f64, p1.y as f64,
                                    p2.x as f64, p2.y as f64,
                                    p3.x as f64, p3.y as f64,
                                );
                            }
                            usvg::tiny_skia_path::PathSegment::Close => {
                                path.close_polygon(PATH_FLAGS_NONE);
                            }
                        }
                    }

                    // Apply fill
                    if let Some(ref fill) = p.fill() {
                        if let usvg::Paint::Color(c) = fill.paint() {
                            let color = Rgba8::new(c.red as u32, c.green as u32, c.blue as u32,
                                ((fill.opacity().get() * 255.0) as u32).min(255));
                            let rule = match fill.rule() {
                                usvg::FillRule::NonZero => FillingRule::NonZero,
                                usvg::FillRule::EvenOdd => FillingRule::EvenOdd,
                            };
                            let stride = (width * 4) as i32;
                            let mut ra = unsafe { RowAccessor::new_with_buf(buf.as_mut_ptr(), width, height, stride) };
                            let mut pf = PixfmtRgba32::new(&mut ra);
                            let mut rb = RendererBase::new(pf);
                            let mut ras = RasterizerScanlineAa::new();
                            ras.filling_rule(rule);
                            let mut transformed = ConvTransform::new(&mut path, group_transform.clone());
                            ras.add_path(&mut transformed, 0);
                            let mut sl = ScanlineU8::new();
                            render_scanlines_aa_solid(&mut ras, &mut sl, &mut rb, &color);
                        }
                    }

                    // Apply stroke
                    if let Some(ref stroke) = p.stroke() {
                        if let usvg::Paint::Color(c) = stroke.paint() {
                            let color = Rgba8::new(c.red as u32, c.green as u32, c.blue as u32,
                                ((stroke.opacity().get() * 255.0) as u32).min(255));
                            let mut conv_stroke = ConvStroke::new(path.clone());
                            conv_stroke.set_width(stroke.width().get() as f64);
                            conv_stroke.set_line_cap(match stroke.linecap() {
                                usvg::LineCap::Butt => LineCap::Butt,
                                usvg::LineCap::Round => LineCap::Round,
                                usvg::LineCap::Square => LineCap::Square,
                            });
                            conv_stroke.set_line_join(match stroke.linejoin() {
                                usvg::LineJoin::Miter | usvg::LineJoin::MiterClip => LineJoin::Miter,
                                usvg::LineJoin::Round => LineJoin::Round,
                                usvg::LineJoin::Bevel => LineJoin::Bevel,
                            });
                            conv_stroke.set_miter_limit(stroke.miterlimit().get() as f64);

                            let stride = (width * 4) as i32;
                            let mut ra = unsafe { RowAccessor::new_with_buf(buf.as_mut_ptr(), width, height, stride) };
                            let mut pf = PixfmtRgba32::new(&mut ra);
                            let mut rb = RendererBase::new(pf);
                            let mut ras = RasterizerScanlineAa::new();
                            let mut transformed = ConvTransform::new(&mut conv_stroke, group_transform.clone());
                            ras.add_path(&mut transformed, 0);
                            let mut sl = ScanlineU8::new();
                            render_scanlines_aa_solid(&mut ras, &mut sl, &mut rb, &color);
                        }
                    }
                }
                usvg::Node::Image(_) => {
                    // TODO: handle embedded raster images in SVG
                }
                usvg::Node::Text(_) => {
                    // usvg converts text to paths, so this shouldn't normally be reached
                }
            }
        }
    }

    render_group(tree.root(), buf, width, height, &root_transform);
}

#[cfg(not(feature = "svg"))]
pub fn svg_render(s: &ParsedSvg, options: SvgRenderOptions) -> Option<RawImage> {
    None
}

/*
#[cfg(feature = "svg")]
pub fn from_xml(xml: Xml) -> Result<Self, SvgParseError> {
    // https://github.com/RazrFalcon/resvg/issues/308
    Ok(Svg::new(xml.into_tree()))
}
*/

#[cfg(feature = "svg")]
pub fn svg_to_string(s: &ParsedSvg, options: SvgXmlOptions) -> String {
    s.tree.to_string(&translate_to_usvg_xmloptions(options))
}

#[cfg(not(feature = "svg"))]
pub fn svg_to_string(s: &ParsedSvg, options: SvgXmlOptions) -> String {
    String::new()
}

#[cfg(feature = "svg")]
fn svgrenderoptions_get_width_height_node(
    s: &SvgRenderOptions,
    node: &usvg::Group,
) -> Option<(u32, u32)> {
    match s.target_size.as_ref() {
        None => {
            let bbox = node.bounding_box();
            let size = usvg::Size::from_wh(bbox.width(), bbox.height())?;
            Some((
                size.width().round().max(0.0) as u32,
                size.height().round().max(0.0) as u32,
            ))
        }
        Some(s) => Some((s.width as u32, s.height as u32)),
    }
}

#[cfg(feature = "svg")]
fn translate_transform(e: SvgRenderTransform) -> agg_rust::trans_affine::TransAffine {
    agg_rust::trans_affine::TransAffine::new_custom(
        e.sx as f64,
        e.ky as f64,
        e.kx as f64,
        e.sy as f64,
        e.tx as f64,
        e.ty as f64,
    )
}

#[cfg(feature = "svg")]
fn translate_to_usvg_shaperendering(e: ShapeRendering) -> usvg::ShapeRendering {
    match e {
        ShapeRendering::OptimizeSpeed => usvg::ShapeRendering::OptimizeSpeed,
        ShapeRendering::CrispEdges => usvg::ShapeRendering::CrispEdges,
        ShapeRendering::GeometricPrecision => usvg::ShapeRendering::GeometricPrecision,
    }
}

#[cfg(feature = "svg")]
fn translate_to_usvg_imagerendering(e: ImageRendering) -> usvg::ImageRendering {
    match e {
        ImageRendering::OptimizeQuality => usvg::ImageRendering::OptimizeQuality,
        ImageRendering::OptimizeSpeed => usvg::ImageRendering::OptimizeSpeed,
    }
}

#[cfg(feature = "svg")]
fn translate_to_usvg_textrendering(e: TextRendering) -> usvg::TextRendering {
    match e {
        TextRendering::OptimizeSpeed => usvg::TextRendering::OptimizeSpeed,
        TextRendering::OptimizeLegibility => usvg::TextRendering::OptimizeLegibility,
        TextRendering::GeometricPrecision => usvg::TextRendering::GeometricPrecision,
    }
}

#[cfg(feature = "svg")]
#[allow(dead_code)]
fn translate_color(i: ColorU) -> agg_rust::color::Rgba8 {
    agg_rust::color::Rgba8::new(i.r as u32, i.g as u32, i.b as u32, i.a as u32)
}

#[cfg(feature = "svg")]
fn translate_to_usvg_parseoptions<'a>(e: SvgParseOptions) -> usvg::Options<'a> {
    use usvg::ImageHrefResolver;

    let mut options = usvg::Options {
        // path: e.relative_image_path.into_option().map(|e| { let p: String = e.clone().into();
        // PathBuf::from(p) }),
        dpi: e.dpi,
        font_family: e.default_font_family.clone().into_library_owned_string(),
        font_size: e.font_size.into(),
        languages: e
            .languages
            .as_ref()
            .iter()
            .map(|e| e.clone().into_library_owned_string())
            .collect(),
        shape_rendering: translate_to_usvg_shaperendering(e.shape_rendering),
        text_rendering: translate_to_usvg_textrendering(e.text_rendering),
        image_rendering: translate_to_usvg_imagerendering(e.image_rendering),
        resources_dir: None,                                      // TODO
        default_size: usvg::Size::from_wh(100.0, 100.0).unwrap(), // TODO
        style_sheet: None,                                        // TODO
        image_href_resolver: ImageHrefResolver::default(),        // TODO
        ..Default::default()
    };

    /*
    // only available with
    use usvg::SystemFontDB;
    use std::path::PathBuf;

    match e.fontdb {
        FontDatabase::Empty => { },
        FontDatabase::System => { options.fontdb.load_system_fonts(); },
    }
    */

    options
}

#[cfg(feature = "svg")]
fn translate_to_usvg_xmloptions(f: SvgXmlOptions) -> usvg::WriteOptions {
    usvg::WriteOptions {
        id_prefix: None,
        preserve_text: false,
        coordinates_precision: 8,
        transforms_precision: 8,
        use_single_quote: f.use_single_quote,
        indent: translate_xmlwriter_indent(f.indent),
        attributes_indent: translate_xmlwriter_indent(f.attributes_indent),
    }
}

#[cfg(feature = "svg")]
fn translate_usvg_svgparserror(e: usvg::Error) -> SvgParseError {
    match e {
        usvg::Error::ElementsLimitReached => SvgParseError::ElementsLimitReached,
        usvg::Error::NotAnUtf8Str => SvgParseError::NotAnUtf8Str,
        usvg::Error::MalformedGZip => SvgParseError::MalformedGZip,
        usvg::Error::InvalidSize => SvgParseError::InvalidSize,
        usvg::Error::ParsingFailed(e) => {
            // Note: usvg uses roxmltree 0.20, but we use 0.21, so we can't directly convert
            // Convert the error to a string representation instead
            use azul_core::xml::{XmlError, XmlTextPos};
            let error_string = format!("{:?}", e);
            SvgParseError::ParsingFailed(XmlError::UnknownToken(XmlTextPos { row: 0, col: 0 }))
        }
    }
}

#[cfg(feature = "svg")]
fn translate_xmlwriter_indent(f: Indent) -> xmlwriter::Indent {
    match f {
        Indent::None => xmlwriter::Indent::None,
        Indent::Spaces(s) => xmlwriter::Indent::Spaces(s),
        Indent::Tabs => xmlwriter::Indent::Tabs,
    }
}

/// Trait for tessellating SvgMultiPolygon shapes
pub trait SvgMultiPolygonTessellation {
    /// Tessellates the polygon with fill style, returns CPU-side vertex buffers
    fn tessellate_fill(&self, fill_style: SvgFillStyle) -> TessellatedSvgNode;
    /// Tessellates the polygon with stroke style, returns CPU-side vertex buffers
    fn tessellate_stroke(&self, stroke_style: SvgStrokeStyle) -> TessellatedSvgNode;
}

impl SvgMultiPolygonTessellation for SvgMultiPolygon {
    fn tessellate_fill(&self, fill_style: SvgFillStyle) -> TessellatedSvgNode {
        tessellate_multi_polygon_fill(self, fill_style)
    }
    fn tessellate_stroke(&self, stroke_style: SvgStrokeStyle) -> TessellatedSvgNode {
        tessellate_multi_polygon_stroke(self, stroke_style)
    }
}
