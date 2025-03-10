use alloc::boxed::Box;
use core::fmt;

#[cfg(not(feature = "svg"))]
pub use azul_core::svg::*;
// re-export everything except for Svg and SvgXmlNode
#[cfg(feature = "svg")]
pub use azul_core::svg::{
    FontDatabase,
    ImageRendering,
    Indent,
    OptionSvgDashPattern,
    ResultSvgSvgParseError,
    ResultSvgXmlNodeSvgParseError,
    ShapeRendering,
    SvgCircle,
    SvgCubicCurve,
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
    SvgPoint,
    SvgQuadraticCurve,
    SvgRect,
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
    SvgVector,
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
    c_void,
};
use azul_core::{
    app_resources::{RawImage, RawImageFormat},
    gl::{GlContextPtr, Texture},
    window::PhysicalSizeU32,
};
use azul_css::{
    AzString, ColorU, LayoutSize, OptionAzString, OptionColorU, OptionI16, OptionLayoutSize,
    OptionU16, StringVec, U8Vec,
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
extern crate tiny_skia;

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
    use lyon::path::{Winding, traits::PathBuilder};

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
                    &Rect::new(Point::new(c.x, c.y), Size2D::new(c.width, c.height)),
                    Winding::Positive,
                );
            }
            SvgSimpleNode::RectHole(c) => {
                builder.add_rectangle(
                    &Rect::new(Point::new(c.x, c.y), Size2D::new(c.width, c.height)),
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

pub fn svg_multi_polygon_intersection(a: &SvgMultiPolygon, b: &SvgMultiPolygon) -> SvgMultiPolygon {
    use geo::{BooleanOps, Coord};

    let a = svg_multi_polygon_to_geo(a);
    let b = svg_multi_polygon_to_geo(b);

    let u = a.intersection(&b);

    geo_to_svg_multipolygon(u)
}

pub fn svg_multi_polygon_difference(a: &SvgMultiPolygon, b: &SvgMultiPolygon) -> SvgMultiPolygon {
    use geo::{BooleanOps, Coord};

    let a = svg_multi_polygon_to_geo(a);
    let b = svg_multi_polygon_to_geo(b);

    let u = a.difference(&b);

    geo_to_svg_multipolygon(u)
}

pub fn svg_multi_polygon_xor(a: &SvgMultiPolygon, b: &SvgMultiPolygon) -> SvgMultiPolygon {
    use geo::{BooleanOps, Coord};

    let a = svg_multi_polygon_to_geo(a);
    let b = svg_multi_polygon_to_geo(b);

    let u = a.xor(&b);

    geo_to_svg_multipolygon(u)
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
        SvgNode::Rect(a) => a.contains_point(point.x, point.y),
        SvgNode::MultiShape(a) => a.as_ref().iter().any(|e| match e {
            SvgSimpleNode::Path(a) => {
                if !a.is_closed() {
                    return false;
                }
                path_contains_point(a, point, fill_rule, tolerance)
            }
            SvgSimpleNode::Circle(a) => a.contains_point(point.x, point.y),
            SvgSimpleNode::Rect(a) => a.contains_point(point.x, point.y),
            SvgSimpleNode::CircleHole(a) => !a.contains_point(point.x, point.y),
            SvgSimpleNode::RectHole(a) => !a.contains_point(point.x, point.y),
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
fn get_radii(r: &SvgRect) -> Rect<f32, UnknownUnit> {
    let rect = Rect::new(Point2D::new(r.x, r.y), Size2D::new(r.width, r.height));
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
pub fn allocate_clipmask_texture(gl_context: GlContextPtr, size: PhysicalSizeU32) -> Texture {
    use azul_core::gl::TextureFlags;

    let textures = gl_context.gen_textures(1);
    let texture_id = textures.get(0).unwrap();

    Texture::new(
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

/// Applies an FXAA filter to the texture
pub fn apply_fxaa(texture: &mut Texture) -> Option<()> {
    // TODO
    Some(())
}

pub fn render_tessellated_node_gpu(texture: &mut Texture, node: &TessellatedSvgNode) -> Option<()> {
    use std::mem;

    use azul_core::gl::{GLuint, GlVoidPtrConst, VertexAttributeType};
    use gl_context_loader::gl;

    const INDEX_TYPE: GLuint = gl::UNSIGNED_INT;

    if texture.format != RawImageFormat::R8 {
        return None;
    }

    let texture_size = texture.size;
    let gl_context = &texture.gl_context;
    let fxaa_shader = gl_context.get_fxaa_shader();
    let svg_shader = gl_context.get_svg_shader();

    // start: save the OpenGL state
    let mut current_multisample = [0_u8];
    let mut current_index_buffer = [0_i32];
    let mut current_vertex_array = [0_i32];
    let mut current_vertex_buffer = [0_i32];
    let mut current_vertex_array_object = [0_i32];
    let mut current_program = [0_i32];
    let mut current_framebuffers = [0_i32];
    let mut current_texture_2d = [0_i32];
    let mut current_primitive_restart_enabled = [0_u8];

    gl_context.get_boolean_v(gl::MULTISAMPLE, (&mut current_multisample[..]).into());
    gl_context.get_integer_v(gl::VERTEX_ARRAY, (&mut current_vertex_array[..]).into());
    gl_context.get_integer_v(
        gl::ARRAY_BUFFER_BINDING,
        (&mut current_vertex_buffer[..]).into(),
    );
    gl_context.get_integer_v(
        gl::ELEMENT_ARRAY_BUFFER_BINDING,
        (&mut current_index_buffer[..]).into(),
    );
    gl_context.get_integer_v(gl::CURRENT_PROGRAM, (&mut current_program[..]).into());
    gl_context.get_integer_v(
        gl::VERTEX_ARRAY_BINDING,
        (&mut current_vertex_array_object[..]).into(),
    );
    gl_context.get_integer_v(gl::FRAMEBUFFER, (&mut current_framebuffers[..]).into());
    gl_context.get_integer_v(gl::TEXTURE_2D, (&mut current_texture_2d[..]).into());
    gl_context.get_boolean_v(
        gl::PRIMITIVE_RESTART_FIXED_INDEX,
        (&mut current_primitive_restart_enabled[..]).into(),
    );

    // stage 1: upload vertices / indices to GPU

    let vertex_array_object = gl_context.gen_vertex_arrays(1);
    let vertex_array_object = vertex_array_object.get(0)?;

    let vertex_buffer_id = gl_context.gen_buffers(1);
    let vertex_buffer_id = vertex_buffer_id.get(0)?;

    let index_buffer_id = gl_context.gen_buffers(1);
    let index_buffer_id = index_buffer_id.get(0)?;

    gl_context.bind_vertex_array(*vertex_array_object);
    gl_context.bind_buffer(gl::ARRAY_BUFFER, *vertex_buffer_id);
    gl_context.buffer_data_untyped(
        gl::ARRAY_BUFFER,
        (mem::size_of::<SvgVertex>() * node.vertices.len()) as isize,
        GlVoidPtrConst {
            ptr: &node.vertices as *const _ as *const std::ffi::c_void,
            run_destructor: true,
        },
        gl::STATIC_DRAW,
    );

    gl_context.bind_buffer(gl::ELEMENT_ARRAY_BUFFER, *index_buffer_id);
    gl_context.buffer_data_untyped(
        gl::ELEMENT_ARRAY_BUFFER,
        (mem::size_of::<u32>() * node.indices.len()) as isize,
        GlVoidPtrConst {
            ptr: &node.indices as *const _ as *const std::ffi::c_void,
            run_destructor: true,
        },
        gl::STATIC_DRAW,
    );

    // stage 2: set up the data description
    let vertex_type = VertexAttributeType::Float;
    let vertex_count = 2;
    let stride = vertex_type.get_mem_size() * vertex_count;
    let offset = 0;
    let vertices_are_normalized = false;

    let vertex_attrib_location = gl_context.get_attrib_location(svg_shader, "vAttrXY".into());
    gl_context.vertex_attrib_pointer(
        vertex_attrib_location as u32,
        vertex_count as i32,
        vertex_type.get_gl_id(),
        vertices_are_normalized,
        stride as i32,
        offset as u32,
    );
    gl_context.enable_vertex_attrib_array(vertex_attrib_location as u32);

    // stage 3: draw

    gl_context.bind_texture(gl::TEXTURE_2D, texture.texture_id);
    gl_context.tex_image_2d(
        gl::TEXTURE_2D,
        0,
        gl::R8 as i32,
        texture_size.width as i32,
        texture_size.height as i32,
        0,
        gl::RED,
        gl::UNSIGNED_BYTE,
        None.into(),
    );
    gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
    gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
    gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
    gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);

    let framebuffers = gl_context.gen_framebuffers(1);
    let framebuffer_id = framebuffers.get(0)?;
    gl_context.bind_framebuffer(gl::FRAMEBUFFER, *framebuffer_id);

    gl_context.framebuffer_texture_2d(
        gl::FRAMEBUFFER,
        gl::COLOR_ATTACHMENT0,
        gl::TEXTURE_2D,
        texture.texture_id,
        0,
    );
    gl_context.draw_buffers([gl::COLOR_ATTACHMENT0][..].into());
    gl_context.viewport(0, 0, texture_size.width as i32, texture_size.height as i32);

    debug_assert!(
        gl_context.check_frame_buffer_status(gl::FRAMEBUFFER) == gl::FRAMEBUFFER_COMPLETE
    );

    gl_context.use_program(svg_shader);
    gl_context.disable(gl::MULTISAMPLE);

    let bbox_uniform_location = gl_context.get_uniform_location(svg_shader, "vBboxSize".into());

    gl_context.clear_color(0.0, 0.0, 0.0, 1.0);
    gl_context.clear(gl::COLOR_BUFFER_BIT);
    gl_context.bind_vertex_array(*vertex_buffer_id);
    gl_context.bind_buffer(gl::ELEMENT_ARRAY_BUFFER, *index_buffer_id);
    gl_context.uniform_2f(
        bbox_uniform_location,
        texture_size.width as f32,
        texture_size.height as f32,
    );
    gl_context.draw_elements(gl::TRIANGLES, node.indices.len() as i32, INDEX_TYPE, 0);

    // stage 4: cleanup - reset the OpenGL state
    if u32::from(current_multisample[0]) == gl::TRUE {
        gl_context.enable(gl::MULTISAMPLE);
    }
    if u32::from(current_primitive_restart_enabled[0]) == gl::FALSE {
        gl_context.disable(gl::PRIMITIVE_RESTART_FIXED_INDEX);
    }
    gl_context.bind_vertex_array(current_vertex_array_object[0] as u32);
    gl_context.bind_framebuffer(gl::FRAMEBUFFER, current_framebuffers[0] as u32);
    gl_context.bind_texture(gl::TEXTURE_2D, current_texture_2d[0] as u32);
    gl_context.bind_buffer(gl::ELEMENT_ARRAY_BUFFER, current_index_buffer[0] as u32);
    gl_context.bind_buffer(gl::ARRAY_BUFFER, current_vertex_buffer[0] as u32);
    gl_context.use_program(current_program[0] as u32);

    // delete resources
    gl_context.delete_framebuffers(framebuffers.as_ref().into());
    gl_context.delete_vertex_arrays(([current_vertex_array_object[0] as u32])[..].into());
    gl_context.delete_buffers(([*vertex_buffer_id, *index_buffer_id])[..].into());

    Some(())
}

#[cfg(feature = "svg")]
pub fn render_node_clipmask_cpu(
    image: &mut RawImage,
    node: &SvgNode,
    style: SvgStyle,
) -> Option<()> {
    use azul_core::app_resources::RawImageData;
    use tiny_skia::{
        FillRule as SkFillRule, LineCap as SkLineCap, LineJoin as SkLineJoin, Paint as SkPaint,
        Path as SkPath, PathBuilder as SkPathBuilder, Pixmap as SkPixmap, Rect as SkRect,
        Stroke as SkStroke, StrokeDash as SkStrokeDash, Transform as SkTransform,
    };

    fn tiny_skia_translate_node(node: &SvgNode) -> Option<SkPath> {
        macro_rules! build_path {
            ($path_builder:expr, $p:expr) => {{
                if $p.items.as_ref().is_empty() {
                    return None;
                }

                let start = $p.items.as_ref()[0].get_start();
                $path_builder.move_to(start.x, start.y);

                for path_element in $p.items.as_ref() {
                    match path_element {
                        SvgPathElement::Line(l) => {
                            $path_builder.line_to(l.end.x, l.end.y);
                        }
                        SvgPathElement::QuadraticCurve(qc) => {
                            $path_builder.quad_to(qc.ctrl.x, qc.ctrl.y, qc.end.x, qc.end.y);
                        }
                        SvgPathElement::CubicCurve(cc) => {
                            $path_builder.cubic_to(
                                cc.ctrl_1.x,
                                cc.ctrl_1.y,
                                cc.ctrl_2.x,
                                cc.ctrl_2.y,
                                cc.end.x,
                                cc.end.y,
                            );
                        }
                    }
                }

                if $p.is_closed() {
                    $path_builder.close();
                }
            }};
        }

        match node {
            SvgNode::MultiPolygonCollection(mpc) => {
                let mut path_builder = SkPathBuilder::new();
                for mp in mpc.iter() {
                    for p in mp.rings.iter() {
                        build_path!(path_builder, p);
                    }
                }
                path_builder.finish()
            }
            SvgNode::MultiPolygon(mp) => {
                let mut path_builder = SkPathBuilder::new();
                for p in mp.rings.iter() {
                    build_path!(path_builder, p);
                }
                path_builder.finish()
            }
            SvgNode::Path(p) => {
                let mut path_builder = SkPathBuilder::new();
                build_path!(path_builder, p);
                path_builder.finish()
            }
            SvgNode::Circle(c) => SkPathBuilder::from_circle(c.center_x, c.center_y, c.radius),
            SvgNode::Rect(r) => {
                // TODO: rounded edges!
                Some(SkPathBuilder::from_rect(SkRect::from_xywh(
                    r.x, r.y, r.width, r.height,
                )?))
            }
            // TODO: test?
            SvgNode::MultiShape(ms) => {
                let mut path_builder = SkPathBuilder::new();
                for p in ms.as_ref() {
                    match p {
                        SvgSimpleNode::Path(p) => {
                            build_path!(path_builder, p);
                        }
                        SvgSimpleNode::Rect(r) => {
                            path_builder.push_rect(r.x, r.y, r.width, r.height);
                        }
                        SvgSimpleNode::Circle(c) => {
                            path_builder.push_circle(c.center_x, c.center_y, c.radius);
                        }
                        SvgSimpleNode::CircleHole(c) => {
                            path_builder.push_circle(c.center_x, c.center_y, c.radius);
                        }
                        SvgSimpleNode::RectHole(r) => {
                            path_builder.push_rect(r.x, r.y, r.width, r.height);
                        }
                    }
                }
                path_builder.finish()
            }
        }
    }

    let mut paint = SkPaint::default();
    paint.set_color_rgba8(255, 255, 255, 255);
    paint.anti_alias = style.get_antialias();
    paint.force_hq_pipeline = style.get_high_quality_aa();

    let transform = style.get_transform();
    let transform = SkTransform {
        sx: transform.sx,
        kx: transform.kx,
        ky: transform.ky,
        sy: transform.sy,
        tx: transform.tx,
        ty: transform.ty,
    };

    let mut pixmap = SkPixmap::new(image.width as u32, image.height as u32)?;
    let path = tiny_skia_translate_node(node)?;
    let clip_mask = None;

    match style {
        SvgStyle::Fill(fs) => {
            pixmap.fill_path(
                &path,
                &paint,
                match fs.fill_rule {
                    SvgFillRule::Winding => SkFillRule::Winding,
                    SvgFillRule::EvenOdd => SkFillRule::EvenOdd,
                },
                transform,
                clip_mask,
            )?;
        }
        SvgStyle::Stroke(ss) => {
            let stroke = SkStroke {
                width: ss.line_width,
                miter_limit: ss.miter_limit,
                line_cap: match ss.start_cap {
                    // TODO: end_cap?
                    SvgLineCap::Butt => SkLineCap::Butt,
                    SvgLineCap::Square => SkLineCap::Square,
                    SvgLineCap::Round => SkLineCap::Round,
                },
                line_join: match ss.line_join {
                    SvgLineJoin::Miter | SvgLineJoin::MiterClip => SkLineJoin::Miter,
                    SvgLineJoin::Round => SkLineJoin::Round,
                    SvgLineJoin::Bevel => SkLineJoin::Bevel,
                },
                dash: ss.dash_pattern.as_ref().and_then(|d| {
                    SkStrokeDash::new(
                        vec![
                            d.length_1, d.gap_1, d.length_2, d.gap_2, d.length_3, d.gap_3,
                        ],
                        d.offset,
                    )
                }),
            };
            pixmap.stroke_path(&path, &paint, &stroke, transform, clip_mask)?;
        }
    }

    // RGBA to red channel
    let red_channel = pixmap
        .take()
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
pub struct SvgXmlNode {
    node: Box<usvg::Node>, // usvg::Node
    pub run_destructor: bool,
}

#[cfg(feature = "svg")]
impl Clone for SvgXmlNode {
    fn clone(&self) -> Self {
        Self {
            node: self.node.clone(),
            run_destructor: true,
        }
    }
}

#[cfg(feature = "svg")]
impl Drop for SvgXmlNode {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}

#[cfg(not(feature = "svg"))]
pub use azul_core::svg::SvgXmlNode;

#[cfg(feature = "svg")]
fn svgxmlnode_new(node: usvg::Node) -> SvgXmlNode {
    SvgXmlNode {
        node: Box::new(node),
        run_destructor: true,
    }
}

#[cfg(feature = "svg")]
pub fn svgxmlnode_parse(
    svg_file_data: &[u8],
    options: SvgParseOptions,
) -> Result<SvgXmlNode, SvgParseError> {
    let svg = svg_parse(svg_file_data, options)?;
    Ok(svg_root(&svg))
}

#[cfg(not(feature = "svg"))]
pub fn svgxmlnode_parse(
    svg_file_data: &[u8],
    options: SvgParseOptions,
) -> Result<SvgXmlNode, SvgParseError> {
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
pub struct Svg {
    tree: Box<usvg::Tree>, // *mut usvg::Tree,
    pub run_destructor: bool,
}

#[cfg(feature = "svg")]
impl Clone for Svg {
    fn clone(&self) -> Self {
        Self {
            tree: self.tree.clone(),
            run_destructor: true,
        }
    }
}

#[cfg(feature = "svg")]
impl Drop for Svg {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}

#[cfg(not(feature = "svg"))]
pub use azul_core::svg::Svg;

#[cfg(feature = "svg")]
impl fmt::Debug for Svg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        svg_to_string(&self, SvgXmlOptions::default()).fmt(f)
    }
}

#[cfg(feature = "svg")]
fn svg_new(tree: usvg::Tree) -> Svg {
    Svg {
        tree: Box::new(tree),
        run_destructor: true,
    }
}

/// NOTE: SVG file data may be Zlib compressed
#[cfg(feature = "svg")]
pub fn svg_parse(svg_file_data: &[u8], options: SvgParseOptions) -> Result<Svg, SvgParseError> {
    let rtree = usvg::Tree::from_data(
        svg_file_data,
        &translate_to_usvg_parseoptions(options).to_ref(),
    )
    .map_err(translate_usvg_svgparserror)?;

    Ok(svg_new(rtree))
}

#[cfg(not(feature = "svg"))]
pub fn svg_parse(svg_file_data: &[u8], options: SvgParseOptions) -> Result<Svg, SvgParseError> {
    Err(SvgParseError::NoParserAvailable)
}

#[cfg(feature = "svg")]
pub fn svg_root(s: &Svg) -> SvgXmlNode {
    svgxmlnode_new(s.tree.root())
}

#[cfg(not(feature = "svg"))]
pub fn svg_root(s: &Svg) -> SvgXmlNode {
    SvgXmlNode {
        node: core::ptr::null_mut(),
        run_destructor: false,
    }
}

#[cfg(feature = "svg")]
pub fn svg_render(s: &Svg, options: SvgRenderOptions) -> Option<RawImage> {
    use azul_core::app_resources::RawImageData;
    use tiny_skia::Pixmap;

    let root = s.tree.root();
    let (target_width, target_height) = svgrenderoptions_get_width_height_node(&options, &root)?;

    if target_height == 0 || target_width == 0 {
        return None;
    }

    let mut pixmap = Pixmap::new(target_width, target_height)?;

    pixmap.fill(
        options
            .background_color
            .into_option()
            .map(translate_color)
            .unwrap_or(tiny_skia::Color::TRANSPARENT),
    );

    let _ = resvg::render_node(
        &s.tree,
        &s.tree.root(),
        translate_fit_to(options.fit),
        translate_transform(options.transform),
        pixmap.as_mut(),
    )?;

    Some(RawImage {
        tag: Vec::new().into(),
        pixels: RawImageData::U8(pixmap.take().into()),
        width: target_width as usize,
        height: target_height as usize,
        premultiplied_alpha: true,
        data_format: RawImageFormat::RGBA8,
    })
}

#[cfg(not(feature = "svg"))]
pub fn svg_render(s: &Svg, options: SvgRenderOptions) -> Option<RawImage> {
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
pub fn svg_to_string(s: &Svg, options: SvgXmlOptions) -> String {
    s.tree.to_string(&translate_to_usvg_xmloptions(options))
}

#[cfg(not(feature = "svg"))]
pub fn svg_to_string(s: &Svg, options: SvgXmlOptions) -> String {
    String::new()
}

#[cfg(feature = "svg")]
fn svgrenderoptions_get_width_height_node(
    s: &SvgRenderOptions,
    node: &usvg::Node,
) -> Option<(u32, u32)> {
    match s.target_size.as_ref() {
        None => {
            use usvg::NodeExt;
            let bbox = node.calculate_bbox()?;
            let size = usvg::Size::new(bbox.width(), bbox.height())?.to_screen_size();
            Some((size.width(), size.height()))
        }
        Some(s) => Some((s.width as u32, s.height as u32)),
    }
}

#[cfg(feature = "svg")]
fn translate_transform(e: SvgRenderTransform) -> tiny_skia::Transform {
    tiny_skia::Transform {
        sx: e.sx,
        kx: e.kx,
        ky: e.ky,
        sy: e.sy,
        tx: e.tx,
        ty: e.ty,
    }
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
fn translate_color(i: ColorU) -> tiny_skia::Color {
    tiny_skia::Color::from_rgba8(i.r, i.g, i.b, i.a)
}

#[cfg(feature = "svg")]
#[allow(dead_code)]
const fn translate_fit_to(i: SvgFitTo) -> usvg::FitTo {
    match i {
        SvgFitTo::Original => usvg::FitTo::Original,
        SvgFitTo::Width(w) => usvg::FitTo::Width(w),
        SvgFitTo::Height(h) => usvg::FitTo::Height(h),
        SvgFitTo::Zoom(z) => usvg::FitTo::Zoom(z),
    }
}

#[cfg(feature = "svg")]
fn translate_to_usvg_parseoptions(e: SvgParseOptions) -> usvg::Options {
    let mut options = usvg::Options {
        // path: e.relative_image_path.into_option().map(|e| { let p: String = e.clone().into();
        // PathBuf::from(p) }),
        dpi: e.dpi as f64,
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
        keep_named_groups: e.keep_named_groups,
        ..usvg::Options::default()
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
fn translate_to_usvg_xmloptions(f: SvgXmlOptions) -> usvg::XmlOptions {
    usvg::XmlOptions {
        id_prefix: None,
        writer_opts: xmlwriter::Options {
            use_single_quote: f.use_single_quote,
            indent: translate_xmlwriter_indent(f.indent),
            attributes_indent: translate_xmlwriter_indent(f.attributes_indent),
        },
    }
}

#[cfg(feature = "svg")]
fn translate_usvg_svgparserror(e: usvg::Error) -> SvgParseError {
    use crate::xml::translate_roxmltree_error;
    match e {
        usvg::Error::ElementsLimitReached => SvgParseError::ElementsLimitReached,
        usvg::Error::NotAnUtf8Str => SvgParseError::NotAnUtf8Str,
        usvg::Error::MalformedGZip => SvgParseError::MalformedGZip,
        usvg::Error::InvalidSize => SvgParseError::InvalidSize,
        usvg::Error::ParsingFailed(e) => SvgParseError::ParsingFailed(translate_roxmltree_error(e)),
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
