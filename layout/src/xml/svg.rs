//! SVG tessellation, rendering, and geometric operations.
//!
//! This module provides:
//! - **Tessellation** of SVG primitives (paths, circles, rects, multi-polygons)
//!   via the lyon tessellation library (behind the `svg` feature flag).
//! - **CPU clip-mask rendering** via the agg-rust rasterizer (`render_node_clipmask_cpu`).
//! - **FXAA post-processing** for GPU-rendered textures (`apply_fxaa`).
//! - **Boolean polygon operations** (union, intersection, difference, XOR)
//!   on `SvgMultiPolygon` shapes via agg scanline boolean algebra.
//! - **SVG parsing and rendering** (`svg_parse`, `svg_render`) using an
//!   XML parser and the agg-rust rendering pipeline.

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

/// Kappa constant for approximating a circle with 4 cubic Bezier curves: 4/3 * (sqrt(2) - 1).
const CIRCLE_BEZIER_KAPPA: f64 = 0.552_284_749_8;

/// Default render size (width, height) when no target size is specified for SVG rendering.
const DEFAULT_SVG_RENDER_SIZE: (u32, u32) = (800, 600);

#[cfg(feature = "svg")]
const fn translate_svg_line_join(e: SvgLineJoin) -> lyon::tessellation::LineJoin {
    use azul_core::svg::SvgLineJoin::{Bevel, Miter, MiterClip, Round};
    match e {
        Miter => lyon::tessellation::LineJoin::Miter,
        MiterClip => lyon::tessellation::LineJoin::MiterClip,
        Round => lyon::tessellation::LineJoin::Round,
        Bevel => lyon::tessellation::LineJoin::Bevel,
    }
}

#[cfg(feature = "svg")]
const fn translate_svg_line_cap(e: SvgLineCap) -> lyon::tessellation::LineCap {
    use azul_core::svg::SvgLineCap::{Butt, Round, Square};
    match e {
        Butt => lyon::tessellation::LineCap::Butt,
        Square => lyon::tessellation::LineCap::Square,
        Round => lyon::tessellation::LineCap::Round,
    }
}

#[cfg(feature = "svg")]
fn translate_svg_stroke_style(e: SvgStrokeStyle) -> StrokeOptions {
    StrokeOptions::tolerance(e.tolerance)
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

    for p in polygon.rings.as_ref() {
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

    for p in polygon {
        match p {
            SvgSimpleNode::Path(p) => {
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

#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
#[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
#[must_use]
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

/// By-value wrapper for `raw_line_intersection` (for FFI)
#[must_use]
pub fn raw_line_intersection_byval(p: &SvgLine, q: SvgLine) -> Option<SvgPoint> {
    raw_line_intersection(p, &q)
}

#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
#[must_use]
pub fn svg_path_offset(p: &SvgPath, distance: f32, join: SvgLineJoin, cap: SvgLineCap) -> SvgPath {
    if distance == 0.0 {
        return p.clone();
    }

    let mut items = p.items.as_slice().to_vec();
    if let Some(mut first) = items.first() {
        items.push(*first);
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
                    None => return *l,
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
                    start: q.start,
                    end: q.ctrl,
                }
                .outwards_normal())
                {
                    Some(s) => SvgPoint {
                        x: s.x * distance,
                        y: s.y * distance,
                    },
                    None => return *l,
                };

                let n2 = match (SvgLine {
                    start: q.ctrl,
                    end: q.end,
                }
                .outwards_normal())
                {
                    Some(s) => SvgPoint {
                        x: s.x * distance,
                        y: s.y * distance,
                    },
                    None => return *l,
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

                let Some(nctrl) = raw_line_intersection(&nl1, &nl2) else {
                    return *l;
                };

                SvgPathElement::QuadraticCurve(SvgQuadraticCurve {
                    start: nl1.start,
                    ctrl: nctrl,
                    end: nl2.end,
                })
            }
            SvgPathElement::CubicCurve(q) => {
                let n1 = match (SvgLine {
                    start: q.start,
                    end: q.ctrl_1,
                }
                .outwards_normal())
                {
                    Some(s) => SvgPoint {
                        x: s.x * distance,
                        y: s.y * distance,
                    },
                    None => return *l,
                };

                let n2 = match (SvgLine {
                    start: q.ctrl_1,
                    end: q.ctrl_2,
                }
                .outwards_normal())
                {
                    Some(s) => SvgPoint {
                        x: s.x * distance,
                        y: s.y * distance,
                    },
                    None => return *l,
                };

                let n3 = match (SvgLine {
                    start: q.ctrl_2,
                    end: q.end,
                }
                .outwards_normal())
                {
                    Some(s) => SvgPoint {
                        x: s.x * distance,
                        y: s.y * distance,
                    },
                    None => return *l,
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

                let Some(nctrl_1) = raw_line_intersection(&nl1, &nl2) else {
                    return *l;
                };

                let Some(nctrl_2) = raw_line_intersection(&nl2, &nl3) else {
                    return *l;
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
            SvgPathElement::Line(q) => q,
            SvgPathElement::QuadraticCurve(q) => SvgLine {
                start: q.ctrl,
                end: q.end,
            },
            SvgPathElement::CubicCurve(q) => SvgLine {
                start: q.ctrl_2,
                end: q.end,
            },
        };

        let b_start_line = match items[i + 1] {
            SvgPathElement::Line(q) => q,
            SvgPathElement::QuadraticCurve(q) => SvgLine {
                start: q.ctrl,
                end: q.start,
            },
            SvgPathElement::CubicCurve(q) => SvgLine {
                start: q.ctrl_1,
                end: q.start,
            },
        };

        if let Some(intersect_pt) = raw_line_intersection(&a_end_line, &b_start_line) {
            items[i].set_last(intersect_pt);
            items[i + 1].set_first(intersect_pt);
        }
    }

    items.pop();

    SvgPath {
        items: items.into(),
    }
}

#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
fn shorten_line_end_by(line: SvgLine, distance: f32) -> SvgLine {
    let dx = line.end.x - line.start.x;
    let dy = line.end.y - line.start.y;
    let dt = dx.hypot(dy);
    let dt_short = dt - distance;

    SvgLine {
        start: line.start,
        end: SvgPoint {
            x: line.start.x + (dt_short / dt) * dx,
            y: line.start.y + (dt_short / dt) * dy,
        },
    }
}

#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
fn shorten_line_start_by(line: SvgLine, distance: f32) -> SvgLine {
    let dx = line.end.x - line.start.x;
    let dy = line.end.y - line.start.y;
    let dt = dx.hypot(dy);
    let dt_short = dt - distance;

    SvgLine {
        start: SvgPoint {
            x: line.start.x + (1.0 - dt_short / dt) * dx,
            y: line.start.y + (1.0 - dt_short / dt) * dy,
        },
        end: line.end,
    }
}

// Creates a "bevel"
#[must_use]
pub fn svg_path_bevel(p: &SvgPath, distance: f32) -> SvgPath {
    let mut items = p.items.as_slice().to_vec();

    // duplicate first & last items
    let first = items.first().copied();
    let last = items.last().copied();
    if let Some(first) = first {
        items.push(first);
    }
    items.reverse();
    if let Some(last) = last {
        items.push(last);
    }
    items.reverse();

    let mut final_items = Vec::new();
    for i in 0..items.len().saturating_sub(1) {
        let a = items[i];
        let b = items[i + 1];
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

#[cfg(feature = "svg")]
fn svg_path_to_lyon_path_events(path: &SvgPath) -> Path {
    let mut builder = Path::builder();

    if !path.items.as_ref().is_empty() {
        let start_item = path.items.as_ref()[0];
        let first_point = Point2D::new(start_item.get_start().x, start_item.get_start().y);

        builder.begin(first_point);

        for p in path.items.as_ref() {
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
#[must_use]
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
        &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex<'_>| {
            let xy_arr = vertex.position();
            SvgVertex {
                x: xy_arr.x,
                y: xy_arr.y,
            }
        }),
    );

    if tess_result.is_err() {
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
#[must_use]
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
        &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex<'_>| {
            let xy_arr = vertex.position();
            SvgVertex {
                x: xy_arr.x,
                y: xy_arr.y,
            }
        }),
    );

    if tess_result.is_err() {
        TessellatedSvgNode::empty()
    } else {
        vertex_buffers_to_tessellated_cpu_node(geometry)
    }
}

#[cfg(not(feature = "svg"))]
pub fn tessellate_multi_shape_fill(
    ms: &[SvgSimpleNode],
    fill_style: SvgFillStyle,
) -> TessellatedSvgNode {
    TessellatedSvgNode::default()
}

#[must_use]
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
#[must_use]
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
#[must_use]
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
        let path = svg_path_to_lyon_path_events(path);
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
#[must_use]
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
        &mut BuffersBuilder::new(&mut stroke_geometry, |vertex: StrokeVertex<'_, '_>| {
            let xy_arr = vertex.position();
            SvgVertex {
                x: xy_arr.x,
                y: xy_arr.y,
            }
        }),
    );

    if tess_result.is_err() {
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
#[must_use]
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
        &mut BuffersBuilder::new(&mut stroke_geometry, |vertex: StrokeVertex<'_, '_>| {
            let xy_arr = vertex.position();
            SvgVertex {
                x: xy_arr.x,
                y: xy_arr.y,
            }
        }),
    );

    if tess_result.is_err() {
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
#[must_use]
pub fn tessellate_path_fill(path: &SvgPath, fill_style: SvgFillStyle) -> TessellatedSvgNode {
    let polygon = svg_path_to_lyon_path_events(path);

    let mut geometry = VertexBuffers::new();
    let mut tessellator = FillTessellator::new();

    let tess_result = tessellator.tessellate_path(
        &polygon,
        &FillOptions::tolerance(fill_style.tolerance),
        &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex<'_>| {
            let xy_arr = vertex.position();
            SvgVertex {
                x: xy_arr.x,
                y: xy_arr.y,
            }
        }),
    );

    if tess_result.is_err() {
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
#[must_use]
pub fn tessellate_path_stroke(path: &SvgPath, stroke_style: SvgStrokeStyle) -> TessellatedSvgNode {
    let stroke_options: StrokeOptions = translate_svg_stroke_style(stroke_style);
    let polygon = svg_path_to_lyon_path_events(path);

    let mut stroke_geometry = VertexBuffers::new();
    let mut stroke_tess = StrokeTessellator::new();

    let tess_result = stroke_tess.tessellate_path(
        &polygon,
        &stroke_options,
        &mut BuffersBuilder::new(&mut stroke_geometry, |vertex: StrokeVertex<'_, '_>| {
            let xy_arr = vertex.position();
            SvgVertex {
                x: xy_arr.x,
                y: xy_arr.y,
            }
        }),
    );

    if tess_result.is_err() {
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
#[must_use]
pub fn tessellate_circle_fill(c: &SvgCircle, fill_style: SvgFillStyle) -> TessellatedSvgNode {
    let center = Point2D::new(c.center_x, c.center_y);

    let mut geometry = VertexBuffers::new();
    let mut tesselator = FillTessellator::new();
    let tess_result = tesselator.tessellate_circle(
        center,
        c.radius,
        &FillOptions::tolerance(fill_style.tolerance),
        &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex<'_>| {
            let xy_arr = vertex.position();
            SvgVertex {
                x: xy_arr.x,
                y: xy_arr.y,
            }
        }),
    );

    if tess_result.is_err() {
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
#[must_use]
pub fn tessellate_circle_stroke(c: &SvgCircle, stroke_style: SvgStrokeStyle) -> TessellatedSvgNode {
    let stroke_options: StrokeOptions = translate_svg_stroke_style(stroke_style);
    let center = Point2D::new(c.center_x, c.center_y);

    let mut stroke_geometry = VertexBuffers::new();
    let mut tesselator = StrokeTessellator::new();

    let tess_result = tesselator.tessellate_circle(
        center,
        c.radius,
        &stroke_options,
        &mut BuffersBuilder::new(&mut stroke_geometry, |vertex: StrokeVertex<'_, '_>| {
            let xy_arr = vertex.position();
            SvgVertex {
                x: xy_arr.x,
                y: xy_arr.y,
            }
        }),
    );

    if tess_result.is_err() {
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
    /*
    let radii = BorderRadii {
        top_left: r.radius_top_left,
        top_right: r.radius_top_right,
        bottom_left: r.radius_bottom_left,
        bottom_right: r.radius_bottom_right
    };*/
    lyon::geom::Box2D::from_origin_and_size(Point2D::new(r.x, r.y), Size2D::new(r.width, r.height))
}

#[cfg(feature = "svg")]
#[must_use]
pub fn tessellate_rect_fill(r: &SvgRect, fill_style: SvgFillStyle) -> TessellatedSvgNode {
    let rect = get_radii(r);
    let mut geometry = VertexBuffers::new();
    let mut tesselator = FillTessellator::new();

    let tess_result = tesselator.tessellate_rectangle(
        &rect,
        &FillOptions::tolerance(fill_style.tolerance),
        &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex<'_>| {
            let xy_arr = vertex.position();
            SvgVertex {
                x: xy_arr.x,
                y: xy_arr.y,
            }
        }),
    );

    if tess_result.is_err() {
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
#[must_use]
pub fn tessellate_rect_stroke(r: &SvgRect, stroke_style: SvgStrokeStyle) -> TessellatedSvgNode {
    let stroke_options: StrokeOptions = translate_svg_stroke_style(stroke_style);
    let rect = get_radii(r);

    let mut stroke_geometry = VertexBuffers::new();
    let mut tesselator = StrokeTessellator::new();

    let tess_result = tesselator.tessellate_rectangle(
        &rect,
        &stroke_options,
        &mut BuffersBuilder::new(&mut stroke_geometry, |vertex: StrokeVertex<'_, '_>| {
            let xy_arr = vertex.position();
            SvgVertex {
                x: xy_arr.x,
                y: xy_arr.y,
            }
        }),
    );

    if tess_result.is_err() {
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
#[must_use]
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
#[must_use]
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
        &mut BuffersBuilder::new(&mut stroke_geometry, |vertex: StrokeVertex<'_, '_>| {
            let xy_arr = vertex.position();
            SvgVertex {
                x: xy_arr.x,
                y: xy_arr.y,
            }
        }),
    );

    if tess_result.is_err() {
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
#[must_use]
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
        &mut BuffersBuilder::new(&mut stroke_geometry, |vertex: StrokeVertex<'_, '_>| {
            let xy_arr = vertex.position();
            SvgVertex {
                x: xy_arr.x,
                y: xy_arr.y,
            }
        }),
    );

    if tess_result.is_err() {
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
#[must_use]
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
        &mut BuffersBuilder::new(&mut stroke_geometry, |vertex: StrokeVertex<'_, '_>| {
            let xy_arr = vertex.position();
            SvgVertex {
                x: xy_arr.x,
                y: xy_arr.y,
            }
        }),
    );

    if tess_result.is_err() {
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
#[must_use]
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
#[allow(clippy::cast_possible_truncation)] // bounded layout/render numeric cast
#[must_use]
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
                for i in &mut indices {
                    if *i != GL_RESTART_INDEX {
                        *i += vertex_buffer_offset;
                    }
                }
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
#[allow(clippy::cast_possible_truncation)] // bounded layout/render numeric cast
#[must_use]
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
                for i in &mut indices {
                    if *i != GL_RESTART_INDEX {
                        *i += vertex_buffer_offset;
                    }
                }
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
#[must_use]
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
#[must_use]
pub fn tessellate_node_stroke(node: &SvgNode, ss: SvgStrokeStyle) -> TessellatedSvgNode {
    match &node {
        SvgNode::MultiPolygonCollection(ref mpc) => {
            let tessellated_multipolygons = mpc
                .as_ref()
                .iter()
                .map(|mp| tessellate_multi_polygon_stroke(mp, ss))
                .collect::<Vec<_>>();
            join_tessellated_nodes(&tessellated_multipolygons)
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
/// # Panics
///
/// Panics if the GL driver returned no texture id.
#[must_use]
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
    apply_fxaa_with_config(texture, &azul_core::gl_fxaa::FxaaConfig::enabled())
}

/// Applies FXAA with custom configuration parameters.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)] // bounded layout/render numeric cast
#[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
pub fn apply_fxaa_with_config(
    texture: &mut Texture,
    config: &azul_core::gl_fxaa::FxaaConfig,
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
    gl_context.get_integer_v(gl::ACTIVE_TEXTURE, (&mut current_active_texture[..]).into());
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
        1.0, 1.0, // top-right
        -1.0, 1.0, // top-left
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
        (size_of::<f32>() * quad_vertices.len()) as isize,
        GlVoidPtrConst {
            ptr: quad_vertices.as_ptr().cast::<c_void>(),
            run_destructor: true,
        },
        gl::STATIC_DRAW,
    );

    let ibos = gl_context.gen_buffers(1);
    let ibo_id = *ibos.get(0)?;
    gl_context.bind_buffer(gl::ELEMENT_ARRAY_BUFFER, ibo_id);
    gl_context.buffer_data_untyped(
        gl::ELEMENT_ARRAY_BUFFER,
        (size_of::<u32>() * quad_indices.len()) as isize,
        GlVoidPtrConst {
            ptr: quad_indices.as_ptr().cast::<c_void>(),
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

    let u_edge_threshold = gl_context.get_uniform_location(fxaa_shader, "uEdgeThreshold");
    gl_context.uniform_1f(u_edge_threshold, config.edge_threshold);

    let u_edge_threshold_min = gl_context.get_uniform_location(fxaa_shader, "uEdgeThresholdMin");
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
#[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)] // bounded layout/render numeric cast
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
pub fn render_node_clipmask_cpu(
    image: &mut RawImage,
    node: &SvgNode,
    style: SvgStyle,
) -> Option<()> {
    use agg_rust::{
        basics::{FillingRule, VertexSource, PATH_FLAGS_NONE},
        color::Rgba8,
        conv_stroke::ConvStroke,
        conv_transform::ConvTransform,
        math_stroke::{LineCap, LineJoin},
        path_storage::PathStorage,
        pixfmt_rgba::{PixelFormat, PixfmtRgba32},
        rasterizer_scanline_aa::RasterizerScanlineAa,
        renderer_base::RendererBase,
        renderer_scanline::render_scanlines_aa_solid,
        rendering_buffer::RowAccessor,
        scanline_u::ScanlineU8,
        trans_affine::TransAffine,
    };
    use azul_core::resources::RawImageData;

    #[allow(clippy::many_single_char_names)] // domain-standard coordinate/geometry/short-lived names
    #[allow(clippy::match_same_arms)]
    // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    fn agg_translate_node(node: &SvgNode) -> Option<PathStorage> {
        macro_rules! build_path {
            ($path:expr, $p:expr) => {{
                if $p.items.as_ref().is_empty() {
                    return None;
                }

                let start = $p.items.as_ref()[0].get_start();
                $path.move_to(f64::from(start.x), f64::from(start.y));

                for path_element in $p.items.as_ref() {
                    match path_element {
                        SvgPathElement::Line(l) => {
                            $path.line_to(f64::from(l.end.x), f64::from(l.end.y));
                        }
                        SvgPathElement::QuadraticCurve(qc) => {
                            $path.curve3(
                                f64::from(qc.ctrl.x),
                                f64::from(qc.ctrl.y),
                                f64::from(qc.end.x),
                                f64::from(qc.end.y),
                            );
                        }
                        SvgPathElement::CubicCurve(cc) => {
                            $path.curve4(
                                f64::from(cc.ctrl_1.x),
                                f64::from(cc.ctrl_1.y),
                                f64::from(cc.ctrl_2.x),
                                f64::from(cc.ctrl_2.y),
                                f64::from(cc.end.x),
                                f64::from(cc.end.y),
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
                for mp in mpc {
                    for p in &mp.rings {
                        build_path!(path, p);
                    }
                }
            }
            SvgNode::MultiPolygon(mp) => {
                for p in &mp.rings {
                    build_path!(path, p);
                }
            }
            SvgNode::Path(p) => {
                build_path!(path, p);
            }
            SvgNode::Circle(c) => {
                // Approximate circle with 4 cubic beziers
                let cx = f64::from(c.center_x);
                let cy = f64::from(c.center_y);
                let r = f64::from(c.radius);
                let k = CIRCLE_BEZIER_KAPPA;
                let kr = k * r;
                path.move_to(cx + r, cy);
                path.curve4(cx + r, cy + kr, cx + kr, cy + r, cx, cy + r);
                path.curve4(cx - kr, cy + r, cx - r, cy + kr, cx - r, cy);
                path.curve4(cx - r, cy - kr, cx - kr, cy - r, cx, cy - r);
                path.curve4(cx + kr, cy - r, cx + r, cy - kr, cx + r, cy);
                path.close_polygon(PATH_FLAGS_NONE);
            }
            SvgNode::Rect(r) => {
                let x = f64::from(r.x);
                let y = f64::from(r.y);
                let w = f64::from(r.width);
                let h = f64::from(r.height);
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
                            let x = f64::from(r.x);
                            let y = f64::from(r.y);
                            let w = f64::from(r.width);
                            let h = f64::from(r.height);
                            path.move_to(x, y);
                            path.line_to(x + w, y);
                            path.line_to(x + w, y + h);
                            path.line_to(x, y + h);
                            path.close_polygon(PATH_FLAGS_NONE);
                        }
                        SvgSimpleNode::Circle(c) | SvgSimpleNode::CircleHole(c) => {
                            let cx = f64::from(c.center_x);
                            let cy = f64::from(c.center_y);
                            let r = f64::from(c.radius);
                            let k = CIRCLE_BEZIER_KAPPA;
                            let kr = k * r;
                            path.move_to(cx + r, cy);
                            path.curve4(cx + r, cy + kr, cx + kr, cy + r, cx, cy + r);
                            path.curve4(cx - kr, cy + r, cx - r, cy + kr, cx - r, cy);
                            path.curve4(cx - r, cy - kr, cx - kr, cy - r, cx, cy - r);
                            path.curve4(cx + kr, cy - r, cx + r, cy - kr, cx + r, cy);
                            path.close_polygon(PATH_FLAGS_NONE);
                        }
                        SvgSimpleNode::RectHole(r) => {
                            let x = f64::from(r.x);
                            let y = f64::from(r.y);
                            let w = f64::from(r.width);
                            let h = f64::from(r.height);
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
        f64::from(transform_data.sx),
        f64::from(transform_data.ky),
        f64::from(transform_data.kx),
        f64::from(transform_data.sy),
        f64::from(transform_data.tx),
        f64::from(transform_data.ty),
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
            stroke.set_width(f64::from(ss.line_width));
            stroke.set_miter_limit(f64::from(ss.miter_limit));
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
    let red_channel = buf.chunks_exact(4).map(|r| r[0]).collect::<Vec<_>>();

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

// ============================================================================
// Boolean operations on SvgMultiPolygon — via agg scanline boolean algebra
// ============================================================================

/// Rasterize an `SvgMultiPolygon` into an agg `RasterizerScanlineAa`.
fn rasterize_multi_polygon(
    mp: &SvgMultiPolygon,
) -> agg_rust::rasterizer_scanline_aa::RasterizerScanlineAa {
    use agg_rust::{
        basics::{FillingRule, PATH_FLAGS_NONE},
        path_storage::PathStorage,
        rasterizer_scanline_aa::RasterizerScanlineAa,
    };

    let mut ras = RasterizerScanlineAa::new();
    ras.filling_rule(FillingRule::NonZero);

    let mut path = PathStorage::new();
    for ring in mp.rings.as_ref() {
        let mut first = true;
        for item in ring.items.as_ref() {
            match item {
                SvgPathElement::Line(l) => {
                    if first {
                        path.move_to(f64::from(l.start.x), f64::from(l.start.y));
                        first = false;
                    }
                    path.line_to(f64::from(l.end.x), f64::from(l.end.y));
                }
                SvgPathElement::QuadraticCurve(q) => {
                    if first {
                        path.move_to(f64::from(q.start.x), f64::from(q.start.y));
                        first = false;
                    }
                    path.curve3(
                        f64::from(q.ctrl.x),
                        f64::from(q.ctrl.y),
                        f64::from(q.end.x),
                        f64::from(q.end.y),
                    );
                }
                SvgPathElement::CubicCurve(c) => {
                    if first {
                        path.move_to(f64::from(c.start.x), f64::from(c.start.y));
                        first = false;
                    }
                    path.curve4(
                        f64::from(c.ctrl_1.x),
                        f64::from(c.ctrl_1.y),
                        f64::from(c.ctrl_2.x),
                        f64::from(c.ctrl_2.y),
                        f64::from(c.end.x),
                        f64::from(c.end.y),
                    );
                }
            }
        }
        path.close_polygon(PATH_FLAGS_NONE);
    }
    ras.add_path(&mut path, 0);
    ras
}

/// Extract polygon contours from a `ScanlineStorageAa` by tracing
/// horizontal span edges across consecutive scanlines.
///
/// For each row, we collect the solid spans (coverage > 128). Then we
/// trace left/right boundaries of connected span groups into closed
/// polygons (go down on the left edge, come back up on the right edge).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)] // bounded layout/render numeric cast
fn storage_to_multi_polygon(
    storage: &mut agg_rust::scanline_storage_aa::ScanlineStorageAa,
) -> SvgMultiPolygon {
    use agg_rust::rasterizer_scanline_aa::Scanline;
    use azul_css::props::basic::SvgPoint;

    // Collect solid spans per row
    let mut rows: Vec<(i32, Vec<(i32, i32)>)> = Vec::new(); // (y, [(x_start, x_end)])

    let mut sl = agg_rust::scanline_u::ScanlineU8::new();
    if storage.rewind_scanlines() {
        sl.reset(storage.min_x(), storage.max_x());
        while storage.sweep_scanline(&mut sl) {
            let y = Scanline::y(&sl);
            let mut row_spans: Vec<(i32, i32)> = Vec::new();
            for span in sl.begin() {
                // Span with positive len: per-pixel coverage
                let len = span.len;
                if len <= 0 {
                    continue;
                }
                // Check if any pixel in the span has enough coverage
                let covers = sl.covers();
                let mut x_start = None;
                for j in 0..len as usize {
                    let cov = covers.get(span.cover_offset + j).copied().unwrap_or(0);
                    if cov > 128 {
                        if x_start.is_none() {
                            x_start = Some(span.x + j as i32);
                        }
                    } else if let Some(xs) = x_start.take() {
                        row_spans.push((xs, span.x + j as i32));
                    }
                }
                if let Some(xs) = x_start {
                    row_spans.push((xs, span.x + len));
                }
            }
            if !row_spans.is_empty() {
                rows.push((y, row_spans));
            }
        }
    }

    if rows.is_empty() {
        return SvgMultiPolygon {
            rings: SvgPathVec::from_const_slice(&[]),
        };
    }

    // Simple contour extraction: for each row, create horizontal line segments.
    // Then connect consecutive rows into closed polygons.
    // This produces axis-aligned polygons (staircase approximation).
    let mut rings = Vec::new();

    for (y, spans) in &rows {
        let yf = *y as f32;
        for &(x0, x1) in spans {
            let x0f = x0 as f32;
            let x1f = x1 as f32;
            // Create a small horizontal rectangle for this span
            let elements = vec![
                SvgPathElement::Line(SvgLine::new(
                    SvgPoint { x: x0f, y: yf },
                    SvgPoint { x: x1f, y: yf },
                )),
                SvgPathElement::Line(SvgLine::new(
                    SvgPoint { x: x1f, y: yf },
                    SvgPoint {
                        x: x1f,
                        y: yf + 1.0,
                    },
                )),
                SvgPathElement::Line(SvgLine::new(
                    SvgPoint {
                        x: x1f,
                        y: yf + 1.0,
                    },
                    SvgPoint {
                        x: x0f,
                        y: yf + 1.0,
                    },
                )),
                SvgPathElement::Line(SvgLine::new(
                    SvgPoint {
                        x: x0f,
                        y: yf + 1.0,
                    },
                    SvgPoint { x: x0f, y: yf },
                )),
            ];
            rings.push(SvgPath {
                items: SvgPathElementVec::from_vec(elements),
            });
        }
    }

    SvgMultiPolygon {
        rings: SvgPathVec::from_vec(rings),
    }
}

/// Perform a boolean operation on two `SvgMultiPolygon` shapes using agg scanline algebra.
fn svg_bool_op(
    a: &SvgMultiPolygon,
    b: &SvgMultiPolygon,
    op: agg_rust::scanline_boolean_algebra::SBoolOp,
) -> SvgMultiPolygon {
    use agg_rust::{
        scanline_boolean_algebra::sbool_combine_shapes_aa, scanline_storage_aa::ScanlineStorageAa,
        scanline_u::ScanlineU8,
    };

    let mut ras1 = rasterize_multi_polygon(a);
    let mut ras2 = rasterize_multi_polygon(b);

    let mut sl1 = ScanlineU8::new();
    let mut sl2 = ScanlineU8::new();
    let mut sl_result = ScanlineU8::new();
    let mut storage1 = ScanlineStorageAa::new();
    let mut storage2 = ScanlineStorageAa::new();
    let mut storage_result = ScanlineStorageAa::new();

    sbool_combine_shapes_aa(
        op,
        &mut ras1,
        &mut ras2,
        &mut sl1,
        &mut sl2,
        &mut sl_result,
        &mut storage1,
        &mut storage2,
        &mut storage_result,
    );

    storage_to_multi_polygon(&mut storage_result)
}

#[must_use]
pub fn svg_multi_polygon_union(a: &SvgMultiPolygon, b: &SvgMultiPolygon) -> SvgMultiPolygon {
    svg_bool_op(a, b, agg_rust::scanline_boolean_algebra::SBoolOp::Or)
}

// FFI by-value variant: `b` is taken owned to mirror the exported api.json signature.
#[allow(clippy::needless_pass_by_value)]
#[must_use]
pub fn svg_multi_polygon_union_byval(a: &SvgMultiPolygon, b: SvgMultiPolygon) -> SvgMultiPolygon {
    svg_multi_polygon_union(a, &b)
}

#[must_use]
pub fn svg_multi_polygon_intersection(a: &SvgMultiPolygon, b: &SvgMultiPolygon) -> SvgMultiPolygon {
    svg_bool_op(a, b, agg_rust::scanline_boolean_algebra::SBoolOp::And)
}

// FFI by-value variant: `b` is taken owned to mirror the exported api.json signature.
#[allow(clippy::needless_pass_by_value)]
#[must_use]
pub fn svg_multi_polygon_intersection_byval(
    a: &SvgMultiPolygon,
    b: SvgMultiPolygon,
) -> SvgMultiPolygon {
    svg_multi_polygon_intersection(a, &b)
}

#[must_use]
pub fn svg_multi_polygon_difference(a: &SvgMultiPolygon, b: &SvgMultiPolygon) -> SvgMultiPolygon {
    svg_bool_op(a, b, agg_rust::scanline_boolean_algebra::SBoolOp::AMinusB)
}

// FFI by-value variant: `b` is taken owned to mirror the exported api.json signature.
#[allow(clippy::needless_pass_by_value)]
#[must_use]
pub fn svg_multi_polygon_difference_byval(
    a: &SvgMultiPolygon,
    b: SvgMultiPolygon,
) -> SvgMultiPolygon {
    svg_multi_polygon_difference(a, &b)
}

#[must_use]
pub fn svg_multi_polygon_xor(a: &SvgMultiPolygon, b: &SvgMultiPolygon) -> SvgMultiPolygon {
    svg_bool_op(a, b, agg_rust::scanline_boolean_algebra::SBoolOp::Xor)
}

// FFI by-value variant: `b` is taken owned to mirror the exported api.json signature.
#[allow(clippy::needless_pass_by_value)]
#[must_use]
pub fn svg_multi_polygon_xor_byval(a: &SvgMultiPolygon, b: SvgMultiPolygon) -> SvgMultiPolygon {
    svg_multi_polygon_xor(a, &b)
}

// ============================================================================
// SVG Rendering — ParsedSvg wraps the XML tree, no usvg
// ============================================================================

/// Parsed SVG document — wraps the XML node tree.
///
/// Previously wrapped `usvg::Tree`; now stores our own `XmlNode` tree parsed via xmlparser.
/// Rendering uses the agg-rust pipeline in `cpurender::render_svg_to_png()`.
#[derive(Debug, Clone)]
#[repr(C)]
pub struct ParsedSvgXmlNode {
    pub run_destructor: bool,
}

impl Drop for ParsedSvgXmlNode {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}

/// # Errors
///
/// Returns a `SvgParseError` if the input is not valid SVG.
pub fn svgxmlnode_parse(
    svg_file_data: &[u8],
    _options: SvgParseOptions,
) -> Result<ParsedSvgXmlNode, SvgParseError> {
    // Verify we can parse the XML
    let s = core::str::from_utf8(svg_file_data).map_err(|_| SvgParseError::NotAnUtf8Str)?;
    let _nodes = crate::xml::parse_xml_string(s).map_err(|_| SvgParseError::NoParserAvailable)?;
    Ok(ParsedSvgXmlNode {
        run_destructor: true,
    })
}

/// Parsed SVG document. Stores the raw SVG bytes for deferred rendering.
#[derive(Clone)]
#[repr(C)]
pub struct ParsedSvg {
    pub svg_data: azul_css::U8Vec,
    pub run_destructor: bool,
}

impl Drop for ParsedSvg {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}

impl_result!(
    ParsedSvg,
    SvgParseError,
    ResultParsedSvgSvgParseError,
    copy = false,
    [Debug, Clone]
);

impl From<ParsedSvg> for azul_core::svg::Svg {
    fn from(_parsed: ParsedSvg) -> Self {
        Self {
            tree: core::ptr::null(),
            run_destructor: false,
        }
    }
}

impl fmt::Debug for ParsedSvg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "ParsedSvg({} bytes)", self.svg_data.as_ref().len())
    }
}

impl ParsedSvg {
    /// # Errors
    ///
    /// Returns a `SvgParseError` if the input is not valid SVG.
    pub fn from_string(
        svg_string: &str,
        parse_options: SvgParseOptions,
    ) -> Result<Self, SvgParseError> {
        svg_parse(svg_string.as_bytes(), parse_options)
    }

    /// # Errors
    ///
    /// Returns a `SvgParseError` if the input is not valid SVG.
    pub fn from_bytes(
        svg_bytes: &[u8],
        parse_options: SvgParseOptions,
    ) -> Result<Self, SvgParseError> {
        svg_parse(svg_bytes, parse_options)
    }

    #[must_use]
    pub const fn get_root(&self) -> ParsedSvgXmlNode {
        svg_root(self)
    }

    #[must_use]
    pub fn render(&self, options: SvgRenderOptions) -> Option<RawImage> {
        svg_render(self, options)
    }

    #[must_use]
    pub fn to_string(&self, _options: SvgXmlOptions) -> String {
        String::from_utf8_lossy(self.svg_data.as_ref()).into_owned()
    }
}

/// Parse SVG data into a `ParsedSvg` (validates XML, stores bytes for deferred rendering).
/// # Errors
///
/// Returns a `SvgParseError` if the input is not valid SVG.
pub fn svg_parse(
    svg_file_data: &[u8],
    _options: SvgParseOptions,
) -> Result<ParsedSvg, SvgParseError> {
    // Validate that it's parseable XML
    let s = core::str::from_utf8(svg_file_data).map_err(|_| SvgParseError::NotAnUtf8Str)?;
    let _nodes = crate::xml::parse_xml_string(s).map_err(|_| SvgParseError::NoParserAvailable)?;
    Ok(ParsedSvg {
        svg_data: svg_file_data.to_vec().into(),
        run_destructor: true,
    })
}

#[must_use]
pub const fn svg_root(s: &ParsedSvg) -> ParsedSvgXmlNode {
    ParsedSvgXmlNode {
        run_destructor: true,
    }
}

/// Render a `ParsedSvg` to a `RawImage` using the agg-rust pipeline.
///
/// Requires the `cpurender` feature (the agg-rust + png rasterization pipeline).
/// Without it, SVG parsing/layout still work but rasterizing yields `None`.
#[cfg(feature = "cpurender")]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded layout/render numeric cast
#[must_use]
pub fn svg_render(s: &ParsedSvg, options: SvgRenderOptions) -> Option<RawImage> {
    use azul_core::resources::RawImageData;

    let (target_width, target_height) = options
        .target_size
        .as_ref()
        .map_or(DEFAULT_SVG_RENDER_SIZE, |s| {
            (s.width as u32, s.height as u32)
        });

    if target_width == 0 || target_height == 0 {
        return None;
    }

    let png_data =
        crate::cpurender::render_svg_to_png(s.svg_data.as_ref(), target_width, target_height)
            .ok()?;

    // Decode PNG back to raw RGBA (TODO: render_svg_to_rgba to avoid PNG round-trip)
    let decoder = png::Decoder::new(std::io::Cursor::new(&png_data));
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0u8; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    buf.truncate(info.buffer_size());

    Some(RawImage {
        tag: Vec::new().into(),
        pixels: RawImageData::U8(buf.into()),
        width: info.width as usize,
        height: info.height as usize,
        premultiplied_alpha: false,
        data_format: RawImageFormat::RGBA8,
    })
}

/// `cpurender`-less stub: SVG rasterization needs the agg-rust pipeline, so
/// without that feature there is nothing to render to. Parsing and layout are
/// unaffected — only the raster output is unavailable.
#[cfg(not(feature = "cpurender"))]
pub fn svg_render(_s: &ParsedSvg, _options: SvgRenderOptions) -> Option<RawImage> {
    // The caller asked for pixels and can NEVER get any from this build —
    // a permanent None is otherwise indistinguishable from a bad SVG.
    static ANNOUNCE: std::sync::Once = std::sync::Once::new();
    ANNOUNCE.call_once(|| {
        eprintln!(
            "[azul][svg] svg_render called, but this build has no `cpurender` \
             feature — SVG rasterization always returns None (parsing/layout are \
             unaffected). Rebuild azul-layout with the `cpurender` feature"
        );
    });
    None
}

#[must_use]
pub fn svg_to_string(s: &ParsedSvg, _options: SvgXmlOptions) -> String {
    String::from_utf8_lossy(s.svg_data.as_ref()).into_owned()
}

// ============================================================================
// Lyon tessellation (kept — no usvg dependency)
// ============================================================================

/// Trait for tessellating `SvgMultiPolygon` shapes
pub trait SvgMultiPolygonTessellation {
    fn tessellate_fill(&self, fill_style: SvgFillStyle) -> TessellatedSvgNode;
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

// ============================================================================
// Adversarial unit tests
// ============================================================================
//
// The whole module is `#[cfg(feature = "svg")]`, so the `#[cfg(not(feature =
// "svg"))]` stubs above are never compiled and are therefore not exercised here.
// `cpurender` is a separate flag and is gated per-test.

#[cfg(test)]
mod autotest_generated {
    use azul_core::resources::RawImageData;

    use super::*;

    // ------------------------------------------------------------------
    // helpers
    // ------------------------------------------------------------------

    fn pt(x: f32, y: f32) -> SvgPoint {
        SvgPoint { x, y }
    }

    fn ln(x0: f32, y0: f32, x1: f32, y1: f32) -> SvgLine {
        SvgLine {
            start: pt(x0, y0),
            end: pt(x1, y1),
        }
    }

    fn mk_path(items: Vec<SvgPathElement>) -> SvgPath {
        SvgPath {
            items: SvgPathElementVec::from_vec(items),
        }
    }

    fn empty_path() -> SvgPath {
        SvgPath {
            items: SvgPathElementVec::from_const_slice(&[]),
        }
    }

    /// A closed, axis-aligned 10x10 square whose lower-left corner is at `d`.
    fn square_at(d: f32) -> SvgPath {
        mk_path(vec![
            SvgPathElement::Line(ln(d, d, d + 10.0, d)),
            SvgPathElement::Line(ln(d + 10.0, d, d + 10.0, d + 10.0)),
            SvgPathElement::Line(ln(d + 10.0, d + 10.0, d, d + 10.0)),
            SvgPathElement::Line(ln(d, d + 10.0, d, d)),
        ])
    }

    fn square_path() -> SvgPath {
        square_at(0.0)
    }

    fn polygon_of(rings: Vec<SvgPath>) -> SvgMultiPolygon {
        SvgMultiPolygon {
            rings: SvgPathVec::from_vec(rings),
        }
    }

    fn square_polygon() -> SvgMultiPolygon {
        polygon_of(vec![square_path()])
    }

    fn empty_polygon() -> SvgMultiPolygon {
        SvgMultiPolygon {
            rings: SvgPathVec::from_const_slice(&[]),
        }
    }

    fn rect(x: f32, y: f32, w: f32, h: f32) -> SvgRect {
        SvgRect {
            width: w,
            height: h,
            x,
            y,
            ..SvgRect::default()
        }
    }

    /// `SvgTransform::default()` is the **zero** matrix (every field is `0.0`),
    /// which collapses all geometry onto the origin — tests that want a real
    /// 1:1 mapping must build the identity by hand.
    const fn identity_transform() -> SvgTransform {
        SvgTransform {
            sx: 1.0,
            kx: 0.0,
            ky: 0.0,
            sy: 1.0,
            tx: 0.0,
            ty: 0.0,
        }
    }

    fn tess(vertices: &[(f32, f32)], indices: &[u32]) -> TessellatedSvgNode {
        TessellatedSvgNode {
            vertices: vertices
                .iter()
                .map(|&(x, y)| SvgVertex { x, y })
                .collect::<Vec<_>>()
                .into(),
            indices: indices.to_vec().into(),
        }
    }

    fn mask_image(w: usize, h: usize) -> RawImage {
        RawImage {
            pixels: RawImageData::U8(vec![0u8; w * h].into()),
            width: w,
            height: h,
            premultiplied_alpha: false,
            data_format: RawImageFormat::R8,
            tag: Vec::new().into(),
        }
    }

    fn mask_bytes(img: &RawImage) -> Vec<u8> {
        match &img.pixels {
            RawImageData::U8(v) => v.as_ref().to_vec(),
            _ => panic!("a clip mask must always come back as 8-bit data"),
        }
    }

    /// Every index must address a vertex that actually exists.
    fn assert_indices_in_range(t: &TessellatedSvgNode) {
        let verts = t.vertices.as_ref().len();
        for i in t.indices.as_ref() {
            if *i == GL_RESTART_INDEX {
                continue;
            }
            assert!(
                (*i as usize) < verts,
                "index {i} points past a {verts}-vertex buffer"
            );
        }
    }

    const MINIMAL_SVG: &[u8] =
        br#"<svg viewBox="0 0 8 8"><rect x="0" y="0" width="8" height="8" fill="red"/></svg>"#;

    // ==================================================================
    // translate_svg_line_join / translate_svg_line_cap
    // ==================================================================

    #[test]
    fn translate_svg_line_join_maps_every_variant() {
        use lyon::tessellation::LineJoin as L;
        assert_eq!(translate_svg_line_join(SvgLineJoin::Miter), L::Miter);
        assert_eq!(
            translate_svg_line_join(SvgLineJoin::MiterClip),
            L::MiterClip
        );
        assert_eq!(translate_svg_line_join(SvgLineJoin::Round), L::Round);
        assert_eq!(translate_svg_line_join(SvgLineJoin::Bevel), L::Bevel);
        assert_eq!(translate_svg_line_join(SvgLineJoin::default()), L::Miter);
    }

    #[test]
    fn translate_svg_line_cap_maps_every_variant() {
        use lyon::tessellation::LineCap as C;
        assert_eq!(translate_svg_line_cap(SvgLineCap::Butt), C::Butt);
        assert_eq!(translate_svg_line_cap(SvgLineCap::Square), C::Square);
        assert_eq!(translate_svg_line_cap(SvgLineCap::Round), C::Round);
        assert_eq!(translate_svg_line_cap(SvgLineCap::default()), C::Butt);
    }

    // ==================================================================
    // translate_svg_stroke_style
    // ==================================================================

    #[test]
    fn translate_svg_stroke_style_carries_every_field_across() {
        let s = SvgStrokeStyle {
            start_cap: SvgLineCap::Round,
            end_cap: SvgLineCap::Square,
            line_join: SvgLineJoin::Bevel,
            line_width: 3.5,
            miter_limit: 7.25,
            tolerance: 0.25,
            ..SvgStrokeStyle::default()
        };
        let o = translate_svg_stroke_style(s);
        assert_eq!(o.start_cap, lyon::tessellation::LineCap::Round);
        assert_eq!(o.end_cap, lyon::tessellation::LineCap::Square);
        assert_eq!(o.line_join, lyon::tessellation::LineJoin::Bevel);
        assert!((o.line_width - 3.5).abs() < 1e-6, "{o:?}");
        assert!((o.miter_limit - 7.25).abs() < 1e-6, "{o:?}");
        assert!((o.tolerance - 0.25).abs() < 1e-6, "{o:?}");
    }

    #[test]
    fn translate_svg_stroke_style_accepts_the_azul_default() {
        let o = translate_svg_stroke_style(SvgStrokeStyle::default());
        assert!(o.miter_limit >= 1.0, "the default must clear lyon's assert");
    }

    // FINDING (pinned): lyon's `StrokeOptions::with_miter_limit` is a *hard*
    // `assert!(limit >= 1.0)` (not a debug assert), so an `SvgStrokeStyle` whose
    // miter_limit is below 1.0 aborts every stroke entry point instead of being
    // clamped. `SvgStrokeStyle` accepts such a value without complaint.
    #[test]
    #[should_panic(expected = "limit")]
    fn translate_svg_stroke_style_miter_limit_below_one_panics() {
        let s = SvgStrokeStyle {
            miter_limit: 0.0,
            ..SvgStrokeStyle::default()
        };
        let _ = translate_svg_stroke_style(s);
    }

    #[test]
    #[should_panic(expected = "limit")]
    fn translate_svg_stroke_style_nan_miter_limit_panics() {
        let s = SvgStrokeStyle {
            miter_limit: f32::NAN,
            ..SvgStrokeStyle::default()
        };
        let _ = translate_svg_stroke_style(s);
    }

    // ==================================================================
    // raw_line_intersection / raw_line_intersection_byval
    // ==================================================================

    #[test]
    fn raw_line_intersection_crossing_diagonals_meet_in_the_middle() {
        let i = raw_line_intersection(&ln(0.0, 0.0, 10.0, 10.0), &ln(0.0, 10.0, 10.0, 0.0))
            .expect("crossing diagonals intersect");
        assert!((i.x - 5.0).abs() < 1e-4, "{i:?}");
        assert!((i.y - 5.0).abs() < 1e-4, "{i:?}");
    }

    #[test]
    fn raw_line_intersection_parallel_lines_are_none() {
        assert_eq!(
            raw_line_intersection(&ln(0.0, 0.0, 10.0, 0.0), &ln(0.0, 5.0, 10.0, 5.0)),
            None
        );
    }

    #[test]
    fn raw_line_intersection_identical_lines_are_none() {
        // Collinear overlap has infinitely many solutions -> 0/0 -> NaN -> None.
        let l = ln(0.0, 0.0, 10.0, 0.0);
        assert_eq!(raw_line_intersection(&l, &l), None);
    }

    #[test]
    fn raw_line_intersection_zero_length_lines_are_none() {
        assert_eq!(
            raw_line_intersection(&ln(1.0, 1.0, 1.0, 1.0), &ln(0.0, 0.0, 2.0, 2.0)),
            None
        );
        assert_eq!(
            raw_line_intersection(&ln(0.0, 0.0, 0.0, 0.0), &ln(0.0, 0.0, 0.0, 0.0)),
            None
        );
    }

    #[test]
    fn raw_line_intersection_never_leaks_nan_for_extreme_inputs() {
        let extremes = [
            ln(f32::NAN, 0.0, 1.0, 1.0),
            ln(0.0, f32::NAN, 1.0, 1.0),
            ln(f32::INFINITY, f32::INFINITY, 1.0, 1.0),
            ln(f32::NEG_INFINITY, 0.0, f32::INFINITY, 0.0),
            ln(f32::MAX, f32::MAX, f32::MIN, f32::MIN),
            ln(f32::MIN_POSITIVE, 0.0, -f32::MIN_POSITIVE, 0.0),
            ln(0.0, 0.0, 0.0, 0.0),
            ln(-1.0, -1.0, 1.0, 1.0),
        ];
        for p in &extremes {
            for q in &extremes {
                if let Some(i) = raw_line_intersection(p, q) {
                    assert!(
                        !i.x.is_nan() && !i.y.is_nan(),
                        "NaN escaped for {p:?} x {q:?} -> {i:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn raw_line_intersection_byval_matches_the_by_ref_form() {
        let p = ln(0.0, 0.0, 10.0, 10.0);
        let q = ln(0.0, 10.0, 10.0, 0.0);
        assert_eq!(
            raw_line_intersection_byval(&p, q),
            raw_line_intersection(&p, &q)
        );
        let horizontal = ln(0.0, 0.0, 10.0, 0.0);
        let parallel = ln(0.0, 5.0, 10.0, 5.0);
        assert_eq!(raw_line_intersection_byval(&horizontal, parallel), None);
    }

    // ==================================================================
    // shorten_line_end_by / shorten_line_start_by
    // ==================================================================

    #[test]
    fn shorten_line_end_by_trims_the_end_only() {
        let out = shorten_line_end_by(ln(0.0, 0.0, 10.0, 0.0), 4.0);
        assert_eq!(out.start, pt(0.0, 0.0));
        assert!((out.end.x - 6.0).abs() < 1e-3, "{out:?}");
        assert!(out.end.y.abs() < 1e-3, "{out:?}");
    }

    #[test]
    fn shorten_line_end_by_zero_is_the_identity() {
        let l = ln(1.0, 2.0, 11.0, 2.0);
        let out = shorten_line_end_by(l, 0.0);
        assert!((out.end.x - 11.0).abs() < 1e-3, "{out:?}");
        assert!((out.end.y - 2.0).abs() < 1e-3, "{out:?}");
    }

    #[test]
    fn shorten_line_end_by_more_than_the_length_overshoots_past_the_start() {
        // 20 taken off a 10-long line puts the end 10 units *behind* the start.
        let out = shorten_line_end_by(ln(0.0, 0.0, 10.0, 0.0), 20.0);
        assert!((out.end.x + 10.0).abs() < 1e-3, "{out:?}");
    }

    #[test]
    fn shorten_line_end_by_negative_distance_extends_the_line() {
        let out = shorten_line_end_by(ln(0.0, 0.0, 10.0, 0.0), -5.0);
        assert!((out.end.x - 15.0).abs() < 1e-3, "{out:?}");
    }

    #[test]
    fn shorten_line_end_by_degenerate_segment_yields_nan() {
        // LATENT DEFECT (pinned): a zero-length segment gives dt == 0, so
        // (dt - distance) / dt is -inf and -inf * 0.0 == NaN. A fix that returns
        // the line unchanged will flip this test loudly rather than silently.
        let out = shorten_line_end_by(ln(5.0, 5.0, 5.0, 5.0), 1.0);
        assert!(out.end.x.is_nan() && out.end.y.is_nan(), "{out:?}");
        assert_eq!(out.start, pt(5.0, 5.0), "the start is never touched");
    }

    #[test]
    fn shorten_line_end_by_non_finite_distance_does_not_panic() {
        for d in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MAX,
            f32::MIN,
        ] {
            let out = shorten_line_end_by(ln(0.0, 0.0, 10.0, 0.0), d);
            assert_eq!(out.start, pt(0.0, 0.0), "distance {d}: start must survive");
        }
    }

    #[test]
    fn shorten_line_start_by_trims_the_start_only() {
        let out = shorten_line_start_by(ln(0.0, 0.0, 10.0, 0.0), 4.0);
        assert_eq!(out.end, pt(10.0, 0.0));
        assert!((out.start.x - 4.0).abs() < 1e-3, "{out:?}");
        assert!(out.start.y.abs() < 1e-3, "{out:?}");
    }

    #[test]
    fn shorten_line_start_by_zero_is_the_identity() {
        let out = shorten_line_start_by(ln(2.0, 3.0, 12.0, 3.0), 0.0);
        assert!((out.start.x - 2.0).abs() < 1e-3, "{out:?}");
        assert!((out.start.y - 3.0).abs() < 1e-3, "{out:?}");
    }

    #[test]
    fn shorten_line_start_by_degenerate_segment_yields_nan() {
        // Same latent defect as `shorten_line_end_by`, mirrored onto the start.
        let out = shorten_line_start_by(ln(-3.0, 7.0, -3.0, 7.0), 2.0);
        assert!(out.start.x.is_nan() && out.start.y.is_nan(), "{out:?}");
        assert_eq!(out.end, pt(-3.0, 7.0));
    }

    // ==================================================================
    // svg_path_offset
    // ==================================================================

    #[test]
    fn svg_path_offset_zero_distance_returns_the_input_unchanged() {
        let p = square_path();
        assert_eq!(
            svg_path_offset(&p, 0.0, SvgLineJoin::Miter, SvgLineCap::Butt),
            p
        );
        // -0.0 == 0.0 under IEEE-754, so the early-out must fire for it too.
        assert_eq!(
            svg_path_offset(&p, -0.0, SvgLineJoin::Miter, SvgLineCap::Butt),
            p
        );
    }

    #[test]
    fn svg_path_offset_empty_path_stays_empty() {
        // The `items.pop()` at the end must not underflow on an empty vec.
        let out = svg_path_offset(&empty_path(), 5.0, SvgLineJoin::Round, SvgLineCap::Round);
        assert!(out.items.as_ref().is_empty());
    }

    #[test]
    fn svg_path_offset_single_line_moves_along_its_outwards_normal() {
        // (0,0)->(10,0) has inwards normal (0, 1), so outwards is (0, -1).
        let p = mk_path(vec![SvgPathElement::Line(ln(0.0, 0.0, 10.0, 0.0))]);
        let out = svg_path_offset(&p, 5.0, SvgLineJoin::Miter, SvgLineCap::Butt);
        assert_eq!(out.items.as_ref().len(), 1);
        match out.items.as_ref()[0] {
            SvgPathElement::Line(l) => {
                assert!(l.start.x.abs() < 1e-4, "{l:?}");
                assert!((l.end.x - 10.0).abs() < 1e-4, "{l:?}");
                assert!((l.start.y + 5.0).abs() < 1e-4, "{l:?}");
                assert!((l.end.y + 5.0).abs() < 1e-4, "{l:?}");
            }
            other => panic!("expected a line, got {other:?}"),
        }
    }

    #[test]
    fn svg_path_offset_preserves_the_item_count_for_any_distance() {
        let p = square_path();
        for d in [
            1.0_f32,
            -1.0,
            1e-30,
            1e30,
            f32::MAX,
            f32::MIN,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            let out = svg_path_offset(&p, d, SvgLineJoin::Miter, SvgLineCap::Butt);
            assert_eq!(
                out.items.as_ref().len(),
                p.items.as_ref().len(),
                "distance {d} changed the element count"
            );
        }
    }

    #[test]
    fn svg_path_offset_degenerate_segments_are_passed_through_unchanged() {
        // A zero-length line has no normal, so the element must come back as-is.
        let p = mk_path(vec![SvgPathElement::Line(ln(4.0, 4.0, 4.0, 4.0))]);
        assert_eq!(
            svg_path_offset(&p, 9.0, SvgLineJoin::Bevel, SvgLineCap::Square),
            p
        );
    }

    #[test]
    fn svg_path_offset_ignores_its_join_and_cap_arguments() {
        // Documented gap (pinned): neither parameter is ever read; the function
        // only offsets the segments and re-intersects the neighbours.
        let p = square_path();
        let a = svg_path_offset(&p, 3.0, SvgLineJoin::Miter, SvgLineCap::Butt);
        let b = svg_path_offset(&p, 3.0, SvgLineJoin::Round, SvgLineCap::Round);
        assert_eq!(a, b);
    }

    #[test]
    fn svg_path_offset_handles_curve_elements() {
        let p = mk_path(vec![
            SvgPathElement::QuadraticCurve(SvgQuadraticCurve {
                start: pt(0.0, 0.0),
                ctrl: pt(5.0, 10.0),
                end: pt(10.0, 0.0),
            }),
            SvgPathElement::CubicCurve(SvgCubicCurve {
                start: pt(10.0, 0.0),
                ctrl_1: pt(7.0, -5.0),
                ctrl_2: pt(3.0, -5.0),
                end: pt(0.0, 0.0),
            }),
        ]);
        let out = svg_path_offset(&p, 2.0, SvgLineJoin::Miter, SvgLineCap::Butt);
        assert_eq!(out.items.as_ref().len(), 2);
    }

    // ==================================================================
    // svg_path_bevel
    // ==================================================================

    #[test]
    fn svg_path_bevel_empty_path_stays_empty() {
        // Two unconditional `pop()`s on an empty vec must not underflow.
        assert!(svg_path_bevel(&empty_path(), 2.0).items.as_ref().is_empty());
    }

    #[test]
    fn svg_path_bevel_single_line_expands_to_four_elements() {
        // The single element is duplicated at both ends (3 items -> 2 pairs ->
        // 6 pushes), then one element is trimmed off each side.
        let p = mk_path(vec![SvgPathElement::Line(ln(0.0, 0.0, 10.0, 0.0))]);
        let out = svg_path_bevel(&p, 2.0);
        assert_eq!(out.items.as_ref().len(), 4);
        for e in out.items.as_ref() {
            let (s, t) = (e.get_start(), e.get_end());
            assert!(
                s.x.is_finite() && s.y.is_finite() && t.x.is_finite() && t.y.is_finite(),
                "{e:?}"
            );
        }
    }

    #[test]
    fn svg_path_bevel_non_line_pairs_are_passed_straight_through() {
        let p = mk_path(vec![
            SvgPathElement::Line(ln(0.0, 0.0, 10.0, 0.0)),
            SvgPathElement::CubicCurve(SvgCubicCurve {
                start: pt(10.0, 0.0),
                ctrl_1: pt(12.0, 0.0),
                ctrl_2: pt(14.0, 2.0),
                end: pt(14.0, 4.0),
            }),
        ]);
        // No (Line, Line) pair survives the duplication, so every pair takes the
        // 2-push fall-through arm: 3 pairs -> 6 pushes -> 4 after trimming.
        assert_eq!(svg_path_bevel(&p, 1.0).items.as_ref().len(), 4);
    }

    #[test]
    fn svg_path_bevel_zero_distance_keeps_every_coordinate_finite() {
        for e in svg_path_bevel(&square_path(), 0.0).items.as_ref() {
            let (s, t) = (e.get_start(), e.get_end());
            assert!(
                s.x.is_finite() && s.y.is_finite() && t.x.is_finite() && t.y.is_finite(),
                "{e:?}"
            );
        }
    }

    #[test]
    fn svg_path_bevel_extreme_distances_do_not_panic() {
        let p = square_path();
        for d in [
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MAX,
            -f32::MAX,
            1e-30,
        ] {
            assert!(
                !svg_path_bevel(&p, d).items.as_ref().is_empty(),
                "distance {d} produced an empty path"
            );
        }
    }

    #[test]
    fn svg_path_bevel_degenerate_segments_do_not_panic() {
        let p = mk_path(vec![
            SvgPathElement::Line(ln(1.0, 1.0, 1.0, 1.0)),
            SvgPathElement::Line(ln(1.0, 1.0, 1.0, 1.0)),
        ]);
        // NaN coordinates leak out of `shorten_line_*_by` here, but nothing panics.
        assert!(!svg_path_bevel(&p, 3.0).items.as_ref().is_empty());
    }

    // ==================================================================
    // svg_node_contains_point / path_contains_point / polygon_contains_point
    // ==================================================================

    #[test]
    fn svg_node_contains_point_rect_excludes_its_own_border() {
        let node = SvgNode::Rect(rect(0.0, 0.0, 10.0, 10.0));
        assert!(svg_node_contains_point(
            &node,
            pt(5.0, 5.0),
            SvgFillRule::Winding,
            0.1
        ));
        assert!(!svg_node_contains_point(
            &node,
            pt(15.0, 5.0),
            SvgFillRule::Winding,
            0.1
        ));
        // `>` / `<`, not `>=` / `<=`: the border itself is outside.
        assert!(!svg_node_contains_point(
            &node,
            pt(0.0, 0.0),
            SvgFillRule::Winding,
            0.1
        ));
        assert!(!svg_node_contains_point(
            &node,
            pt(10.0, 10.0),
            SvgFillRule::Winding,
            0.1
        ));
    }

    #[test]
    fn svg_node_contains_point_circle_excludes_its_own_rim() {
        let node = SvgNode::Circle(SvgCircle {
            center_x: 0.0,
            center_y: 0.0,
            radius: 5.0,
        });
        assert!(svg_node_contains_point(
            &node,
            pt(0.0, 0.0),
            SvgFillRule::Winding,
            0.1
        ));
        assert!(!svg_node_contains_point(
            &node,
            pt(5.0, 0.0),
            SvgFillRule::Winding,
            0.1
        ));
        assert!(!svg_node_contains_point(
            &node,
            pt(100.0, 100.0),
            SvgFillRule::Winding,
            0.1
        ));
    }

    #[test]
    fn svg_node_contains_point_nan_and_infinite_points_are_outside() {
        let r = SvgNode::Rect(rect(0.0, 0.0, 10.0, 10.0));
        let c = SvgNode::Circle(SvgCircle {
            center_x: 0.0,
            center_y: 0.0,
            radius: 5.0,
        });
        for p in [
            pt(f32::NAN, f32::NAN),
            pt(f32::NAN, 5.0),
            pt(5.0, f32::NAN),
            pt(f32::INFINITY, f32::INFINITY),
            pt(f32::NEG_INFINITY, 0.0),
        ] {
            assert!(
                !svg_node_contains_point(&r, p, SvgFillRule::Winding, 0.1),
                "rect / {p:?}"
            );
            assert!(
                !svg_node_contains_point(&c, p, SvgFillRule::EvenOdd, 0.1),
                "circle / {p:?}"
            );
        }
    }

    #[test]
    fn svg_node_contains_point_empty_geometry_is_never_hit() {
        for node in [
            SvgNode::MultiPolygonCollection(SvgMultiPolygonVec::from_const_slice(&[])),
            SvgNode::MultiShape(SvgSimpleNodeVec::from_const_slice(&[])),
            SvgNode::MultiPolygon(empty_polygon()),
            SvgNode::Path(empty_path()),
        ] {
            assert!(
                !svg_node_contains_point(&node, pt(0.0, 0.0), SvgFillRule::Winding, 0.1),
                "{node:?}"
            );
        }
    }

    #[test]
    fn svg_node_contains_point_open_paths_short_circuit_to_false() {
        let open = mk_path(vec![
            SvgPathElement::Line(ln(0.0, 0.0, 10.0, 0.0)),
            SvgPathElement::Line(ln(10.0, 0.0, 10.0, 10.0)),
        ]);
        assert!(!svg_node_contains_point(
            &SvgNode::Path(open.clone()),
            pt(5.0, 5.0),
            SvgFillRule::Winding,
            0.1
        ));
        assert!(!svg_node_contains_point(
            &SvgNode::MultiShape(SvgSimpleNodeVec::from_vec(vec![SvgSimpleNode::Path(open)])),
            pt(5.0, 5.0),
            SvgFillRule::Winding,
            0.1
        ));
    }

    #[test]
    fn svg_node_contains_point_closed_square_path_is_hit() {
        let node = SvgNode::Path(square_path());
        assert!(svg_node_contains_point(
            &node,
            pt(5.0, 5.0),
            SvgFillRule::Winding,
            0.1
        ));
        assert!(!svg_node_contains_point(
            &node,
            pt(50.0, 5.0),
            SvgFillRule::Winding,
            0.1
        ));
    }

    #[test]
    fn svg_node_contains_point_lone_hole_reports_everything_outside_it() {
        // Pinned semantics: a MultiShape made only of holes inverts, so every
        // point *outside* the hole counts as a hit.
        let hole = SvgCircle {
            center_x: 0.0,
            center_y: 0.0,
            radius: 5.0,
        };
        let node =
            SvgNode::MultiShape(SvgSimpleNodeVec::from_vec(vec![SvgSimpleNode::CircleHole(
                hole,
            )]));
        assert!(!svg_node_contains_point(
            &node,
            pt(0.0, 0.0),
            SvgFillRule::Winding,
            0.1
        ));
        assert!(svg_node_contains_point(
            &node,
            pt(100.0, 100.0),
            SvgFillRule::Winding,
            0.1
        ));
    }

    #[test]
    fn path_contains_point_square_positive_and_negative_controls() {
        let sq = square_path();
        assert!(path_contains_point(
            &sq,
            pt(5.0, 5.0),
            SvgFillRule::Winding,
            0.1
        ));
        assert!(path_contains_point(
            &sq,
            pt(5.0, 5.0),
            SvgFillRule::EvenOdd,
            0.1
        ));
        assert!(!path_contains_point(
            &sq,
            pt(-1.0, 5.0),
            SvgFillRule::Winding,
            0.1
        ));
        assert!(!path_contains_point(
            &sq,
            pt(5.0, 100.0),
            SvgFillRule::EvenOdd,
            0.1
        ));
    }

    #[test]
    fn path_contains_point_empty_path_is_never_hit() {
        assert!(!path_contains_point(
            &empty_path(),
            pt(0.0, 0.0),
            SvgFillRule::Winding,
            0.1
        ));
    }

    #[test]
    fn path_contains_point_tolerance_is_irrelevant_for_straight_edges() {
        // A line-only path is never flattened, so even 0 / negative / huge
        // tolerances must give the same answer instead of hanging.
        let sq = square_path();
        for t in [0.0_f32, 1e-6, 1.0, 1e6, -1.0] {
            assert!(
                path_contains_point(&sq, pt(5.0, 5.0), SvgFillRule::Winding, t),
                "tolerance {t}"
            );
            assert!(
                !path_contains_point(&sq, pt(-50.0, 5.0), SvgFillRule::Winding, t),
                "tolerance {t}"
            );
        }
    }

    #[test]
    fn polygon_contains_point_square_ring() {
        let poly = square_polygon();
        assert!(polygon_contains_point(
            &poly,
            pt(5.0, 5.0),
            SvgFillRule::Winding,
            0.1
        ));
        assert!(!polygon_contains_point(
            &poly,
            pt(-5.0, 5.0),
            SvgFillRule::Winding,
            0.1
        ));
    }

    #[test]
    fn polygon_contains_point_without_rings_is_false() {
        assert!(!polygon_contains_point(
            &empty_polygon(),
            pt(0.0, 0.0),
            SvgFillRule::Winding,
            0.1
        ));
        // A ring that carries no elements must not be treated as a hit either.
        assert!(!polygon_contains_point(
            &polygon_of(vec![empty_path()]),
            pt(0.0, 0.0),
            SvgFillRule::Winding,
            0.1
        ));
    }

    // ==================================================================
    // lyon path conversion helpers
    // ==================================================================

    #[test]
    fn svg_multipolygon_to_lyon_path_of_an_empty_polygon_is_empty() {
        assert_eq!(
            svg_multipolygon_to_lyon_path(&empty_polygon())
                .iter()
                .count(),
            0
        );
    }

    #[test]
    fn svg_multipolygon_to_lyon_path_skips_rings_without_items() {
        let only_empty = polygon_of(vec![empty_path(), empty_path()]);
        assert_eq!(svg_multipolygon_to_lyon_path(&only_empty).iter().count(), 0);
        // …and an empty ring must not change the result of a real one.
        let mixed = polygon_of(vec![empty_path(), square_path()]);
        assert_eq!(
            svg_multipolygon_to_lyon_path(&mixed).iter().count(),
            svg_multipolygon_to_lyon_path(&square_polygon())
                .iter()
                .count()
        );
    }

    #[test]
    fn svg_multi_shape_to_lyon_path_of_an_empty_slice_is_empty() {
        assert_eq!(svg_multi_shape_to_lyon_path(&[]).iter().count(), 0);
    }

    #[test]
    fn svg_path_to_lyon_path_events_of_an_empty_path_is_empty() {
        assert_eq!(
            svg_path_to_lyon_path_events(&empty_path()).iter().count(),
            0
        );
    }

    // ==================================================================
    // vertex_buffers_to_tessellated_cpu_node
    // ==================================================================

    #[test]
    fn vertex_buffers_to_tessellated_cpu_node_moves_both_buffers_verbatim() {
        let mut vb: VertexBuffers<SvgVertex, u32> = VertexBuffers::new();
        vb.vertices.push(SvgVertex { x: 1.0, y: 2.0 });
        vb.vertices.push(SvgVertex { x: 3.0, y: 4.0 });
        vb.indices.extend_from_slice(&[0, 1, 0]);
        let t = vertex_buffers_to_tessellated_cpu_node(vb);
        assert_eq!(t.vertices.as_ref().len(), 2);
        assert_eq!(t.indices.as_ref(), &[0u32, 1, 0][..]);
        assert!((t.vertices.as_ref()[1].x - 3.0).abs() < 1e-6);
    }

    #[test]
    fn vertex_buffers_to_tessellated_cpu_node_of_empty_buffers_is_empty() {
        let t = vertex_buffers_to_tessellated_cpu_node(VertexBuffers::<SvgVertex, u32>::new());
        assert!(t.vertices.as_ref().is_empty());
        assert!(t.indices.as_ref().is_empty());
    }

    // ==================================================================
    // tessellation
    // ==================================================================

    #[test]
    fn tessellate_path_fill_of_a_square_produces_whole_triangles() {
        let t = tessellate_path_fill(&square_path(), SvgFillStyle::default());
        assert!(!t.vertices.as_ref().is_empty());
        assert_eq!(t.indices.as_ref().len() % 3, 0);
        assert_indices_in_range(&t);
    }

    #[test]
    fn tessellate_path_fill_and_stroke_of_an_empty_path_are_empty() {
        let f = tessellate_path_fill(&empty_path(), SvgFillStyle::default());
        assert!(f.vertices.as_ref().is_empty() && f.indices.as_ref().is_empty());
        let s = tessellate_path_stroke(&empty_path(), SvgStrokeStyle::default());
        assert!(s.vertices.as_ref().is_empty() && s.indices.as_ref().is_empty());
    }

    #[test]
    fn tessellate_circle_fill_zero_radius_is_empty() {
        let t = tessellate_circle_fill(
            &SvgCircle {
                center_x: 0.0,
                center_y: 0.0,
                radius: 0.0,
            },
            SvgFillStyle::default(),
        );
        assert!(t.vertices.as_ref().is_empty());
    }

    #[test]
    fn tessellate_circle_fill_negative_radius_matches_the_positive_one() {
        let mk = |r: f32| {
            tessellate_circle_fill(
                &SvgCircle {
                    center_x: 1.0,
                    center_y: 2.0,
                    radius: r,
                },
                SvgFillStyle::default(),
            )
        };
        let positive = mk(5.0);
        assert!(!positive.vertices.as_ref().is_empty());
        assert_eq!(
            positive,
            mk(-5.0),
            "lyon takes |radius|; the sign must not matter"
        );
    }

    #[test]
    fn tessellate_rect_fill_always_emits_exactly_two_triangles() {
        for r in [
            rect(0.0, 0.0, 10.0, 10.0),
            rect(0.0, 0.0, 0.0, 0.0),     // fully degenerate
            rect(5.0, 5.0, -10.0, -10.0), // inverted
            rect(-f32::MAX, -f32::MAX, f32::MAX, f32::MAX), // extreme but finite
        ] {
            let t = tessellate_rect_fill(&r, SvgFillStyle::default());
            assert_eq!(t.vertices.as_ref().len(), 4, "{r:?}");
            assert_eq!(t.indices.as_ref().len(), 6, "{r:?}");
            assert_indices_in_range(&t);
        }
    }

    #[test]
    fn tessellate_rect_stroke_of_a_real_rect_produces_geometry() {
        let t = tessellate_rect_stroke(&rect(0.0, 0.0, 10.0, 10.0), SvgStrokeStyle::default());
        assert!(!t.vertices.as_ref().is_empty());
        assert_indices_in_range(&t);
    }

    #[test]
    fn get_radii_maps_origin_and_size_onto_a_box() {
        let b = get_radii(&rect(1.0, 2.0, 4.0, 6.0));
        assert!((b.min.x - 1.0).abs() < 1e-6, "{b:?}");
        assert!((b.min.y - 2.0).abs() < 1e-6, "{b:?}");
        assert!((b.max.x - 5.0).abs() < 1e-6, "{b:?}");
        assert!((b.max.y - 8.0).abs() < 1e-6, "{b:?}");
    }

    #[test]
    fn get_radii_ignores_the_corner_radii() {
        // Matches the source TODO: "radii not respected on latest version of lyon".
        let base = rect(1.0, 2.0, 4.0, 6.0);
        let rounded = SvgRect {
            radius_top_left: 3.0,
            radius_top_right: 4.0,
            radius_bottom_left: 5.0,
            radius_bottom_right: 9.0,
            ..base
        };
        let (a, b) = (get_radii(&base), get_radii(&rounded));
        assert!((a.min.x - b.min.x).abs() < 1e-6 && (a.min.y - b.min.y).abs() < 1e-6);
        assert!((a.max.x - b.max.x).abs() < 1e-6 && (a.max.y - b.max.y).abs() < 1e-6);
    }

    #[test]
    fn get_radii_of_a_negative_size_rect_produces_an_inverted_box() {
        let b = get_radii(&rect(5.0, 5.0, -3.0, -4.0));
        assert!(b.max.x < b.min.x, "{b:?}");
        assert!(b.max.y < b.min.y, "{b:?}");
    }

    #[test]
    fn tessellate_multi_polygon_fill_of_an_empty_polygon_is_empty() {
        let t = tessellate_multi_polygon_fill(&empty_polygon(), SvgFillStyle::default());
        assert!(t.vertices.as_ref().is_empty());
    }

    #[test]
    fn tessellate_multi_polygon_fill_skips_empty_rings() {
        let with_empty = polygon_of(vec![empty_path(), square_path()]);
        assert_eq!(
            tessellate_multi_polygon_fill(&with_empty, SvgFillStyle::default()),
            tessellate_multi_polygon_fill(&square_polygon(), SvgFillStyle::default()),
        );
    }

    #[test]
    fn tessellate_multi_shape_fill_of_an_empty_slice_is_empty() {
        let t = tessellate_multi_shape_fill(&[], SvgFillStyle::default());
        assert!(t.vertices.as_ref().is_empty());
    }

    #[test]
    fn tessellate_multi_shape_fill_of_a_plain_shape_produces_geometry() {
        let ms = [
            SvgSimpleNode::Circle(SvgCircle {
                center_x: 40.0,
                center_y: 40.0,
                radius: 5.0,
            }),
            SvgSimpleNode::Rect(rect(60.0, 0.0, 5.0, 5.0)),
        ];
        let t = tessellate_multi_shape_fill(&ms, SvgFillStyle::default());
        assert!(!t.vertices.as_ref().is_empty());
        assert_indices_in_range(&t);
    }

    #[test]
    fn tessellate_multi_shape_fill_handles_every_simple_node_kind() {
        let ms = [
            SvgSimpleNode::Path(square_path()),
            SvgSimpleNode::Circle(SvgCircle {
                center_x: 40.0,
                center_y: 40.0,
                radius: 5.0,
            }),
            SvgSimpleNode::CircleHole(SvgCircle {
                center_x: 40.0,
                center_y: 40.0,
                radius: 2.0,
            }),
            SvgSimpleNode::Rect(rect(60.0, 0.0, 5.0, 5.0)),
            SvgSimpleNode::RectHole(rect(61.0, 1.0, 2.0, 2.0)),
        ];
        assert_indices_in_range(&tessellate_multi_shape_fill(&ms, SvgFillStyle::default()));
        assert_indices_in_range(&tessellate_multi_shape_stroke(
            &ms,
            SvgStrokeStyle::default(),
        ));
    }

    #[test]
    fn tessellate_multi_polygon_stroke_of_an_empty_polygon_is_empty() {
        let t = tessellate_multi_polygon_stroke(&empty_polygon(), SvgStrokeStyle::default());
        assert!(t.vertices.as_ref().is_empty());
    }

    #[test]
    fn tessellate_styled_node_dispatches_on_the_style() {
        let geo = SvgNode::Rect(rect(0.0, 0.0, 10.0, 10.0));
        let fill = SvgStyledNode {
            geometry: geo.clone(),
            style: SvgStyle::Fill(SvgFillStyle::default()),
        };
        let stroke = SvgStyledNode {
            geometry: geo.clone(),
            style: SvgStyle::Stroke(SvgStrokeStyle::default()),
        };
        assert_eq!(
            tessellate_styled_node(&fill),
            tessellate_node_fill(&geo, SvgFillStyle::default())
        );
        assert_eq!(
            tessellate_styled_node(&stroke),
            tessellate_node_stroke(&geo, SvgStrokeStyle::default())
        );
    }

    #[test]
    fn tessellate_node_fill_of_a_collection_is_the_join_of_its_parts() {
        let mp = square_polygon();
        let node = SvgNode::MultiPolygonCollection(SvgMultiPolygonVec::from_vec(vec![
            mp.clone(),
            mp.clone(),
        ]));
        let one = tessellate_multi_polygon_fill(&mp, SvgFillStyle::default());
        assert_eq!(
            tessellate_node_fill(&node, SvgFillStyle::default()),
            join_tessellated_nodes(&[one.clone(), one])
        );
    }

    #[test]
    fn tessellate_node_fill_of_an_empty_collection_is_empty() {
        let node = SvgNode::MultiPolygonCollection(SvgMultiPolygonVec::from_const_slice(&[]));
        let t = tessellate_node_fill(&node, SvgFillStyle::default());
        assert!(t.vertices.as_ref().is_empty() && t.indices.as_ref().is_empty());
    }

    #[test]
    fn tessellate_svgpathelement_stroke_matches_the_per_kind_helpers() {
        let ss = SvgStrokeStyle::default();
        let l = ln(0.0, 0.0, 10.0, 10.0);
        assert_eq!(
            tessellate_svgpathelement_stroke(&SvgPathElement::Line(l), ss),
            tessellate_line_stroke(&l, ss)
        );
        let q = SvgQuadraticCurve {
            start: pt(0.0, 0.0),
            ctrl: pt(5.0, 10.0),
            end: pt(10.0, 0.0),
        };
        assert_eq!(
            tessellate_svgpathelement_stroke(&SvgPathElement::QuadraticCurve(q), ss),
            tessellate_quadraticcurve_stroke(&q, ss)
        );
        let c = SvgCubicCurve {
            start: pt(0.0, 0.0),
            ctrl_1: pt(3.0, 10.0),
            ctrl_2: pt(7.0, -10.0),
            end: pt(10.0, 0.0),
        };
        assert_eq!(
            tessellate_svgpathelement_stroke(&SvgPathElement::CubicCurve(c), ss),
            tessellate_cubiccurve_stroke(&c, ss)
        );
    }

    #[test]
    fn tessellate_line_stroke_of_a_zero_length_line_does_not_panic() {
        let t = tessellate_line_stroke(&ln(3.0, 3.0, 3.0, 3.0), SvgStrokeStyle::default());
        assert_indices_in_range(&t);
    }

    // ==================================================================
    // join_tessellated_nodes / join_tessellated_colored_nodes
    // ==================================================================

    #[test]
    fn join_tessellated_nodes_of_nothing_is_empty() {
        let t = join_tessellated_nodes(&[]);
        assert!(t.vertices.as_ref().is_empty());
        assert!(t.indices.as_ref().is_empty());
    }

    #[test]
    fn join_tessellated_nodes_offsets_and_terminates_each_buffer() {
        let a = tess(&[(0.0, 0.0), (1.0, 0.0)], &[0, 1]);
        let b = tess(&[(2.0, 0.0), (3.0, 0.0)], &[0, 1]);
        let j = join_tessellated_nodes(&[a, b]);
        assert_eq!(j.vertices.as_ref().len(), 4);
        assert_eq!(
            j.indices.as_ref(),
            &[0u32, 1, GL_RESTART_INDEX, 2, 3, GL_RESTART_INDEX][..]
        );
        assert_indices_in_range(&j);
    }

    #[test]
    fn join_tessellated_nodes_leaves_existing_restart_markers_unshifted() {
        let a = tess(&[(0.0, 0.0)], &[0]);
        let b = tess(&[(1.0, 0.0), (2.0, 0.0)], &[0, GL_RESTART_INDEX, 1]);
        let j = join_tessellated_nodes(&[a, b]);
        assert_eq!(
            j.indices.as_ref(),
            &[
                0u32,
                GL_RESTART_INDEX,
                1,
                GL_RESTART_INDEX,
                2,
                GL_RESTART_INDEX
            ][..]
        );
    }

    #[test]
    fn join_tessellated_nodes_single_node_is_only_terminated() {
        let j = join_tessellated_nodes(&[tess(&[(0.0, 0.0), (1.0, 1.0)], &[0, 1, 0])]);
        assert_eq!(j.indices.as_ref(), &[0u32, 1, 0, GL_RESTART_INDEX][..]);
    }

    #[test]
    fn join_tessellated_nodes_can_alias_an_index_onto_the_restart_marker() {
        // FINDING (pinned): the offset is applied with a bare `+=` and is only
        // skipped for indices that *already* equal GL_RESTART_INDEX, so a large
        // index can be shifted onto the sentinel and silently turn into a
        // primitive-restart marker. (Anything past u32::MAX additionally
        // overflow-panics in debug builds.)
        let a = tess(&[(0.0, 0.0)], &[0]);
        let b = tess(&[(1.0, 0.0)], &[GL_RESTART_INDEX - 1]);
        let j = join_tessellated_nodes(&[a, b]);
        assert_eq!(
            j.indices.as_ref(),
            &[0u32, GL_RESTART_INDEX, GL_RESTART_INDEX, GL_RESTART_INDEX][..]
        );
    }

    #[test]
    fn join_tessellated_colored_nodes_of_nothing_is_empty() {
        let t = join_tessellated_colored_nodes(&[]);
        assert!(t.vertices.as_ref().is_empty());
        assert!(t.indices.as_ref().is_empty());
    }

    #[test]
    fn join_tessellated_colored_nodes_offsets_like_the_plain_variant() {
        fn colored(xs: &[f32], idx: &[u32]) -> TessellatedColoredSvgNode {
            TessellatedColoredSvgNode {
                vertices: xs
                    .iter()
                    .map(|&x| SvgColoredVertex {
                        x,
                        y: 0.0,
                        z: 0.0,
                        r: 1.0,
                        g: 0.0,
                        b: 0.0,
                        a: 1.0,
                    })
                    .collect::<Vec<_>>()
                    .into(),
                indices: idx.to_vec().into(),
            }
        }
        let j =
            join_tessellated_colored_nodes(&[colored(&[0.0, 1.0], &[0, 1]), colored(&[2.0], &[0])]);
        assert_eq!(j.vertices.as_ref().len(), 3);
        assert_eq!(
            j.indices.as_ref(),
            &[0u32, 1, GL_RESTART_INDEX, 2, GL_RESTART_INDEX][..]
        );
    }

    // ==================================================================
    // boolean operations (agg scanline algebra)
    // ==================================================================

    #[test]
    fn storage_to_multi_polygon_of_an_untouched_storage_is_empty() {
        let mut storage = agg_rust::scanline_storage_aa::ScanlineStorageAa::new();
        assert!(storage_to_multi_polygon(&mut storage)
            .rings
            .as_ref()
            .is_empty());
    }

    #[test]
    fn svg_multi_polygon_boolean_ops_on_two_empties_are_empty() {
        let e = empty_polygon();
        assert!(svg_multi_polygon_union(&e, &e).rings.as_ref().is_empty());
        assert!(svg_multi_polygon_intersection(&e, &e)
            .rings
            .as_ref()
            .is_empty());
        assert!(svg_multi_polygon_difference(&e, &e)
            .rings
            .as_ref()
            .is_empty());
        assert!(svg_multi_polygon_xor(&e, &e).rings.as_ref().is_empty());
    }

    #[test]
    fn svg_multi_polygon_union_of_a_square_with_itself_keeps_the_square() {
        let sq = square_polygon();
        let out = svg_multi_polygon_union(&sq, &sq);
        assert!(
            !out.rings.as_ref().is_empty(),
            "a shape unioned with itself must not vanish"
        );
        let b = out.get_bounds();
        assert!(b.width > 0.0 && b.height > 0.0, "{b:?}");
    }

    #[test]
    fn svg_multi_polygon_intersection_of_disjoint_squares_is_empty() {
        let a = square_polygon();
        let b = polygon_of(vec![square_at(100.0)]);
        assert!(svg_multi_polygon_intersection(&a, &b)
            .rings
            .as_ref()
            .is_empty());
    }

    #[test]
    fn svg_multi_polygon_self_cancelling_ops_are_empty() {
        let sq = square_polygon();
        assert!(
            svg_multi_polygon_difference(&sq, &sq)
                .rings
                .as_ref()
                .is_empty(),
            "A minus A must be empty"
        );
        assert!(
            svg_multi_polygon_xor(&sq, &sq).rings.as_ref().is_empty(),
            "A xor A must be empty"
        );
    }

    #[test]
    fn svg_multi_polygon_ops_accept_curve_rings() {
        let ring = mk_path(vec![
            SvgPathElement::QuadraticCurve(SvgQuadraticCurve {
                start: pt(0.0, 0.0),
                ctrl: pt(5.0, 12.0),
                end: pt(10.0, 0.0),
            }),
            SvgPathElement::CubicCurve(SvgCubicCurve {
                start: pt(10.0, 0.0),
                ctrl_1: pt(7.0, -6.0),
                ctrl_2: pt(3.0, -6.0),
                end: pt(0.0, 0.0),
            }),
        ]);
        let mp = polygon_of(vec![ring]);
        assert!(!svg_multi_polygon_union(&mp, &mp).rings.as_ref().is_empty());
    }

    #[test]
    fn svg_multi_polygon_byval_wrappers_match_the_by_ref_forms() {
        let a = square_polygon();
        let b = polygon_of(vec![square_at(5.0)]);
        assert_eq!(
            svg_multi_polygon_union_byval(&a, b.clone()),
            svg_multi_polygon_union(&a, &b)
        );
        assert_eq!(
            svg_multi_polygon_intersection_byval(&a, b.clone()),
            svg_multi_polygon_intersection(&a, &b)
        );
        assert_eq!(
            svg_multi_polygon_difference_byval(&a, b.clone()),
            svg_multi_polygon_difference(&a, &b)
        );
        assert_eq!(
            svg_multi_polygon_xor_byval(&a, b.clone()),
            svg_multi_polygon_xor(&a, &b)
        );
    }

    // ==================================================================
    // render_node_clipmask_cpu
    // ==================================================================

    #[test]
    fn render_node_clipmask_cpu_zero_sized_image_is_none() {
        let node = SvgNode::Rect(rect(0.0, 0.0, 4.0, 4.0));
        let style = SvgStyle::Fill(SvgFillStyle {
            transform: identity_transform(),
            ..SvgFillStyle::default()
        });
        assert_eq!(
            render_node_clipmask_cpu(&mut mask_image(0, 4), &node, style),
            None
        );
        assert_eq!(
            render_node_clipmask_cpu(&mut mask_image(4, 0), &node, style),
            None
        );
        assert_eq!(
            render_node_clipmask_cpu(&mut mask_image(0, 0), &node, style),
            None
        );
    }

    #[test]
    fn render_node_clipmask_cpu_geometry_less_nodes_are_none() {
        let style = SvgStyle::Fill(SvgFillStyle {
            transform: identity_transform(),
            ..SvgFillStyle::default()
        });
        for node in [
            SvgNode::Path(empty_path()),
            SvgNode::MultiPolygon(empty_polygon()),
            SvgNode::MultiShape(SvgSimpleNodeVec::from_const_slice(&[])),
            SvgNode::MultiPolygonCollection(SvgMultiPolygonVec::from_const_slice(&[])),
        ] {
            assert_eq!(
                render_node_clipmask_cpu(&mut mask_image(4, 4), &node, style),
                None,
                "{node:?}"
            );
        }
    }

    #[test]
    fn render_node_clipmask_cpu_writes_a_single_channel_mask() {
        let node = SvgNode::Rect(rect(0.0, 0.0, 4.0, 4.0));
        let style = SvgStyle::Fill(SvgFillStyle {
            transform: identity_transform(),
            ..SvgFillStyle::default()
        });
        let mut img = mask_image(4, 4);
        assert_eq!(render_node_clipmask_cpu(&mut img, &node, style), Some(()));
        assert_eq!(img.data_format, RawImageFormat::R8);
        assert!(img.premultiplied_alpha);
        let bytes = mask_bytes(&img);
        assert_eq!(bytes.len(), 16, "one byte per pixel");
        assert!(
            bytes.iter().any(|&b| b > 0),
            "a rect covering the whole image must leave coverage"
        );
    }

    #[test]
    fn render_node_clipmask_cpu_default_style_transform_is_not_the_identity() {
        // FINDING (pinned): `SvgTransform::default()` is the *zero* matrix, so a
        // style built straight from `SvgFillStyle::default()` collapses all
        // geometry onto the origin instead of drawing it 1:1.
        let node = SvgNode::Rect(rect(0.0, 0.0, 4.0, 4.0));
        let mut zeroed = mask_image(4, 4);
        let mut identity = mask_image(4, 4);
        assert_eq!(
            render_node_clipmask_cpu(&mut zeroed, &node, SvgStyle::Fill(SvgFillStyle::default())),
            Some(())
        );
        assert_eq!(
            render_node_clipmask_cpu(
                &mut identity,
                &node,
                SvgStyle::Fill(SvgFillStyle {
                    transform: identity_transform(),
                    ..SvgFillStyle::default()
                })
            ),
            Some(())
        );
        assert_ne!(mask_bytes(&zeroed), mask_bytes(&identity));
    }

    #[test]
    fn render_node_clipmask_cpu_stroke_style_renders_without_panicking() {
        let node = SvgNode::Circle(SvgCircle {
            center_x: 8.0,
            center_y: 8.0,
            radius: 4.0,
        });
        let style = SvgStyle::Stroke(SvgStrokeStyle {
            transform: identity_transform(),
            line_width: 2.0,
            ..SvgStrokeStyle::default()
        });
        let mut img = mask_image(16, 16);
        assert_eq!(render_node_clipmask_cpu(&mut img, &node, style), Some(()));
        assert_eq!(mask_bytes(&img).len(), 256);
    }

    #[test]
    fn render_node_clipmask_cpu_geometry_far_outside_the_image_is_clipped_not_crashed() {
        let node = SvgNode::Rect(rect(-1.0e6, -1.0e6, 10.0, 10.0));
        let style = SvgStyle::Fill(SvgFillStyle {
            transform: identity_transform(),
            ..SvgFillStyle::default()
        });
        let mut img = mask_image(4, 4);
        assert_eq!(render_node_clipmask_cpu(&mut img, &node, style), Some(()));
        assert!(mask_bytes(&img).iter().all(|&b| b == 0));
    }

    // ==================================================================
    // svgxmlnode_parse / svg_parse / ParsedSvg
    // ==================================================================

    #[test]
    fn svg_parse_valid_minimal_input_keeps_the_bytes_verbatim() {
        let p = svg_parse(MINIMAL_SVG, SvgParseOptions::default()).expect("positive control");
        assert_eq!(p.svg_data.as_ref(), MINIMAL_SVG);
        assert!(p.run_destructor);
    }

    #[test]
    fn svg_parse_invalid_utf8_is_rejected() {
        for bad in [
            &[0xFFu8, 0xFE, 0x00][..],
            &[0x80][..],
            &[0xED, 0xA0, 0x80][..], // encoded surrogate
            &[0xC0, 0x80][..],       // overlong NUL
        ] {
            assert_eq!(
                svg_parse(bad, SvgParseOptions::default()).err(),
                Some(SvgParseError::NotAnUtf8Str),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn svg_parse_unclosed_elements_are_rejected() {
        for bad in [&b"<svg"[..], &b"<svg>"[..], &b"<svg><g>"[..]] {
            assert_eq!(
                svg_parse(bad, SvgParseOptions::default()).err(),
                Some(SvgParseError::NoParserAvailable),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn svg_parse_accepts_input_that_is_not_svg_at_all() {
        // PINNED leniency: `svg_parse` only checks that the bytes are UTF-8 and
        // tokenize as XML — it never requires an <svg> root, so empty,
        // whitespace-only and plain-text input all come back Ok.
        for lenient in [
            &b""[..],
            &b"   "[..],
            &b"\t\n\r "[..],
            &b"garbage"[..],
            &b"</svg>"[..],
            &b"<html><body/></html>"[..],
        ] {
            let p = svg_parse(lenient, SvgParseOptions::default())
                .unwrap_or_else(|e| panic!("{lenient:?} was rejected with {e:?}"));
            assert_eq!(p.svg_data.as_ref(), lenient);
        }
    }

    #[test]
    fn svg_parse_garbage_never_panics_and_never_invents_data() {
        for data in [
            &b"<<<<<<"[..],
            &b"\x00\x01\x02\x03"[..],
            &b"{\"json\": true}"[..],
            &b"<svg <<>>"[..],
            &b"&&&;;;"[..],
            &b"<!--"[..],
            &b"<?xml"[..],
            &b"<!DOCTYPE"[..],
        ] {
            match svg_parse(data, SvgParseOptions::default()) {
                Ok(p) => assert_eq!(p.svg_data.as_ref(), data, "{data:?}"),
                Err(e) => assert_eq!(e, SvgParseError::NoParserAvailable, "{data:?}"),
            }
        }
    }

    #[test]
    fn svg_parse_boundary_numeric_attributes_are_kept_as_opaque_strings() {
        let src = concat!(
            r#"<svg width="1e400" height="-0" a="9223372036854775807" "#,
            r#"b="NaN" c="inf" d="0"><rect width="0"/></svg>"#
        );
        let p = ParsedSvg::from_string(src, SvgParseOptions::default()).expect("parse");
        assert_eq!(p.to_string(SvgXmlOptions::default()), src);
    }

    #[test]
    fn svg_parse_unicode_payloads_survive_untouched() {
        for s in [
            "<svg><text>\u{1F600}</text></svg>",
            "<svg><text>e\u{0301}\u{0328}</text></svg>",
            "<svg id=\"\u{65E5}\u{672C}\u{8A9E}\"/>",
            "<svg>\u{200b}\u{1F600}</svg>",
        ] {
            let p = ParsedSvg::from_string(s, SvgParseOptions::default())
                .unwrap_or_else(|e| panic!("{s:?} -> {e:?}"));
            assert_eq!(p.to_string(SvgXmlOptions::default()), s);
        }
    }

    #[test]
    fn svg_parse_leading_and_trailing_junk_is_tolerated_and_preserved() {
        let src = "  <svg/>  trailing;garbage";
        let p = ParsedSvg::from_string(src, SvgParseOptions::default()).expect("lenient parse");
        // The stored bytes are never trimmed — to_string gives back the original.
        assert_eq!(p.to_string(SvgXmlOptions::default()), src);
    }

    #[test]
    fn svg_parse_extremely_long_input_does_not_hang() {
        let long = format!("<svg>{}</svg>", "a".repeat(1_000_000));
        let p = ParsedSvg::from_string(&long, SvgParseOptions::default())
            .expect("1 MB of text must parse");
        assert_eq!(p.svg_data.as_ref().len(), long.len());
    }

    #[test]
    fn svg_parse_many_sibling_elements_does_not_hang() {
        let mut s = String::from("<svg>");
        for _ in 0..50_000 {
            s.push_str("<g/>");
        }
        s.push_str("</svg>");
        assert!(ParsedSvg::from_string(&s, SvgParseOptions::default()).is_ok());
    }

    #[test]
    fn svg_parse_deeply_nested_input_does_not_stack_overflow() {
        // The tokenizer loop is iterative, but the XmlNode tree it builds is torn
        // down recursively — run on a big stack so a genuinely linear-depth drop
        // is proven safe instead of coin-flipping on the 2 MiB default test stack.
        let child = std::thread::Builder::new()
            .stack_size(128 * 1024 * 1024)
            .spawn(|| {
                const DEPTH: usize = 10_000;
                let mut s = String::from("<svg>");
                for _ in 0..DEPTH {
                    s.push_str("<g>");
                }
                for _ in 0..DEPTH {
                    s.push_str("</g>");
                }
                s.push_str("</svg>");
                ParsedSvg::from_string(&s, SvgParseOptions::default()).is_ok()
            })
            .expect("spawn");
        assert!(child
            .join()
            .expect("10k-deep nesting must not overflow the stack"));
    }

    #[test]
    fn svgxmlnode_parse_mirrors_svg_parse_acceptance() {
        assert!(svgxmlnode_parse(MINIMAL_SVG, SvgParseOptions::default()).is_ok());
        assert!(svgxmlnode_parse(b"", SvgParseOptions::default()).is_ok());
        assert_eq!(
            svgxmlnode_parse(&[0xFF, 0xFE], SvgParseOptions::default()).err(),
            Some(SvgParseError::NotAnUtf8Str)
        );
        assert_eq!(
            svgxmlnode_parse(b"<svg>", SvgParseOptions::default()).err(),
            Some(SvgParseError::NoParserAvailable)
        );
        let node = svgxmlnode_parse(MINIMAL_SVG, SvgParseOptions::default()).expect("parse");
        assert!(node.run_destructor);
    }

    #[test]
    fn parsed_svg_round_trips_through_to_string_and_back() {
        let src = r#"<svg viewBox="0 0 8 8"><rect width="8" height="8"/></svg>"#;
        let once = ParsedSvg::from_string(src, SvgParseOptions::default()).expect("parse");
        let text = once.to_string(SvgXmlOptions::default());
        assert_eq!(text, src);
        let twice = ParsedSvg::from_string(&text, SvgParseOptions::default()).expect("re-parse");
        assert_eq!(
            twice.to_string(SvgXmlOptions::default()),
            text,
            "serialisation must be idempotent"
        );
        assert_eq!(svg_to_string(&twice, SvgXmlOptions::default()), text);
    }

    #[test]
    fn parsed_svg_from_bytes_and_from_string_agree() {
        let a = ParsedSvg::from_bytes(MINIMAL_SVG, SvgParseOptions::default()).expect("bytes");
        let b = ParsedSvg::from_string(
            core::str::from_utf8(MINIMAL_SVG).expect("ascii"),
            SvgParseOptions::default(),
        )
        .expect("string");
        assert_eq!(a.svg_data.as_ref(), b.svg_data.as_ref());
    }

    #[test]
    fn parsed_svg_to_string_replaces_invalid_utf8_instead_of_panicking() {
        // `ParsedSvg` is a plain #[repr(C)] FFI struct, so it can hold bytes that
        // `svg_parse` would have rejected; `to_string` must stay lossy, not panic.
        let p = ParsedSvg {
            svg_data: vec![0xFFu8, 0xFE].into(),
            run_destructor: true,
        };
        assert_eq!(
            p.to_string(SvgXmlOptions::default()),
            "\u{FFFD}\u{FFFD}",
            "each invalid byte becomes one replacement char"
        );
        assert_eq!(
            svg_to_string(&p, SvgXmlOptions::default()),
            p.to_string(SvgXmlOptions::default())
        );
    }

    #[test]
    fn parsed_svg_to_string_of_an_empty_document_is_empty() {
        let p = ParsedSvg {
            svg_data: Vec::new().into(),
            run_destructor: true,
        };
        assert!(p.to_string(SvgXmlOptions::default()).is_empty());
        assert_eq!(format!("{p:?}"), "ParsedSvg(0 bytes)");
    }

    #[test]
    fn parsed_svg_debug_reports_the_byte_length() {
        let p = ParsedSvg {
            svg_data: vec![1u8, 2, 3].into(),
            run_destructor: true,
        };
        assert_eq!(format!("{p:?}"), "ParsedSvg(3 bytes)");
    }

    #[test]
    fn svg_root_and_get_root_agree() {
        let p = svg_parse(MINIMAL_SVG, SvgParseOptions::default()).expect("parse");
        assert!(p.get_root().run_destructor);
        assert!(svg_root(&p).run_destructor);
        // …and stay well-defined for a document with no elements at all.
        let empty = svg_parse(b"", SvgParseOptions::default()).expect("lenient parse");
        assert!(empty.get_root().run_destructor);
    }

    // ==================================================================
    // svg_render
    // ==================================================================

    #[test]
    fn svg_render_zero_target_size_is_none() {
        let p = svg_parse(MINIMAL_SVG, SvgParseOptions::default()).expect("parse");
        for size in [
            LayoutSize::new(0, 0),
            LayoutSize::new(0, 8),
            LayoutSize::new(8, 0),
        ] {
            let opts = SvgRenderOptions {
                target_size: OptionLayoutSize::Some(size),
                ..SvgRenderOptions::default()
            };
            assert!(p.render(opts).is_none(), "{size:?}");
            assert!(svg_render(&p, opts).is_none(), "{size:?}");
        }
    }

    #[cfg(feature = "cpurender")]
    #[test]
    fn svg_render_produces_an_image_of_the_requested_size() {
        let p = svg_parse(MINIMAL_SVG, SvgParseOptions::default()).expect("parse");
        let opts = SvgRenderOptions {
            target_size: OptionLayoutSize::Some(LayoutSize::new(8, 8)),
            ..SvgRenderOptions::default()
        };
        let img = p.render(opts).expect("a minimal <svg> must rasterize");
        assert_eq!((img.width, img.height), (8, 8));
        assert_eq!(img.data_format, RawImageFormat::RGBA8);
        assert!(!img.premultiplied_alpha);
    }

    #[cfg(feature = "cpurender")]
    #[test]
    fn svg_render_of_a_document_without_an_svg_root_is_none() {
        // `svg_parse` accepts it, but rasterization has nothing to draw.
        let p =
            svg_parse(b"<html><body/></html>", SvgParseOptions::default()).expect("lenient parse");
        let opts = SvgRenderOptions {
            target_size: OptionLayoutSize::Some(LayoutSize::new(4, 4)),
            ..SvgRenderOptions::default()
        };
        assert!(p.render(opts).is_none());
    }

    #[cfg(feature = "cpurender")]
    #[test]
    fn svg_render_one_by_one_target_is_the_smallest_valid_size() {
        let p = svg_parse(MINIMAL_SVG, SvgParseOptions::default()).expect("parse");
        let opts = SvgRenderOptions {
            target_size: OptionLayoutSize::Some(LayoutSize::new(1, 1)),
            ..SvgRenderOptions::default()
        };
        let img = p.render(opts).expect("1x1 must still rasterize");
        assert_eq!((img.width, img.height), (1, 1));
    }
}
