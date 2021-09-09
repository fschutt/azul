use core::fmt;
use azul_core::{
    app_resources::{RawImage, RawImageFormat},
    gl::{Texture, GlContextPtr},
    window::PhysicalSizeU32,
};
use azul_css::{
    OptionI16, OptionU16, U8Vec, OptionAzString,
    OptionColorU, AzString, StringVec, ColorU,
    OptionLayoutSize, LayoutSize,
};
#[cfg(feature = "svg")]
use lyon::{
    tessellation::{
        FillOptions, BuffersBuilder,
        FillVertex, StrokeVertex,
        FillTessellator, VertexBuffers,
        StrokeTessellator, StrokeOptions,
    },
    math::Point,
    path::Path,
    geom::euclid::{Point2D, Rect, Size2D, UnknownUnit},
};
use crate::xml::XmlError;
use alloc::boxed::Box;
#[cfg(not(feature = "svg"))]
pub use azul_core::svg::*;

// re-export everything except for Svg and SvgXmlNode
#[cfg(feature = "svg")]
pub use azul_core::svg::{
    SvgSize, SvgLine, SvgQuadraticCurve, SvgPath, SvgMultiPolygon,
    SvgStyledNode, SvgVertex, SvgCircle, SvgRect, TessellatedSvgNode,
    TessellatedSvgNodeVecRef, TessellatedGPUSvgNode, SvgTransform,
    SvgFillStyle, SvgStrokeStyle, SvgDashPattern, SvgRenderOptions,
    SvgParseOptions, SvgXmlOptions, SvgPathElement, SvgNode,
    SvgStyle, SvgFillRule, SvgLineCap, SvgLineJoin, c_void,
    ShapeRendering, ImageRendering, TextRendering, FontDatabase,
    SvgFitTo, SvgParseError, Indent, SvgVector,

    SvgPoint, SvgCubicCurve, TessellatedSvgNodeVec,
    SvgMultiPolygonVec, SvgPathVec,
    SvgPathElementVec, SvgVertexVec,
    TessellatedSvgNodeVecDestructor,
    SvgMultiPolygonVecDestructor, SvgPathVecDestructor,
    SvgPathElementVecDestructor, SvgVertexVecDestructor,
    OptionSvgDashPattern, ResultSvgXmlNodeSvgParseError,
    ResultSvgSvgParseError,

    // SvgXmlNode, Svg
};

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

        for q in p.items.as_ref().iter().rev() /* NOTE: REVERSE ITERATOR */ {
            match q {
                SvgPathElement::Line(l) => {
                    builder.line_to(Point2D::new(l.end.x, l.end.y));
                },
                SvgPathElement::QuadraticCurve(qc) => {
                    builder.quadratic_bezier_to(
                        Point2D::new(qc.ctrl.x, qc.ctrl.y),
                        Point2D::new(qc.end.x, qc.end.y)
                    );
                },
                SvgPathElement::CubicCurve(cc) => {
                    builder.cubic_bezier_to(
                        Point2D::new(cc.ctrl_1.x, cc.ctrl_1.y),
                        Point2D::new(cc.ctrl_2.x, cc.ctrl_2.y),
                        Point2D::new(cc.end.x, cc.end.y)
                    );
                },
            }
        }

        builder.end(p.is_closed());
    }

    builder.build()
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
                },
                SvgPathElement::QuadraticCurve(qc) => {
                    builder.quadratic_bezier_to(
                        Point2D::new(qc.ctrl.x, qc.ctrl.y),
                        Point2D::new(qc.end.x, qc.end.y)
                    );
                },
                SvgPathElement::CubicCurve(cc) => {
                    builder.cubic_bezier_to(
                        Point2D::new(cc.ctrl_1.x, cc.ctrl_1.y),
                        Point2D::new(cc.ctrl_2.x, cc.ctrl_2.y),
                        Point2D::new(cc.end.x, cc.end.y)
                    );
                },
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
pub fn tessellate_multi_polygon_fill(polygon: &SvgMultiPolygon, fill_style: SvgFillStyle) -> TessellatedSvgNode {

    let polygon = svg_multipolygon_to_lyon_path(polygon);

    let mut geometry = VertexBuffers::new();
    let mut tessellator = FillTessellator::new();

    let tess_result = tessellator.tessellate_path(
        &polygon,
        &FillOptions::tolerance(fill_style.tolerance),
        &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex| {
            let xy_arr = vertex.position();
            SvgVertex { x: xy_arr.x, y: xy_arr.y }
        })
    );

    if let Err(_) = tess_result {
        TessellatedSvgNode::empty()
    } else {
        vertex_buffers_to_tessellated_cpu_node(geometry)
    }
}

#[cfg(not(feature = "svg"))]
pub fn tessellate_multi_polygon_fill(polygon: &SvgMultiPolygon, fill_style: SvgFillStyle) -> TessellatedSvgNode {
    TessellatedSvgNode::default()
}

#[cfg(feature = "svg")]
pub fn polygon_contains_point(polygon: &SvgMultiPolygon, point: SvgPoint, fill_rule: SvgFillRule, tolerance: f32) -> bool {
    use lyon::{math::Point as LyonPoint, path::FillRule as LyonFillRule};
    use lyon::algorithms::hit_test::hit_test_path;
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
pub fn polygon_contains_point(polygon: &SvgMultiPolygon, point: SvgPoint, tolerance: f32, fill_rule: SvgFillRule) -> bool {
    false
}

#[cfg(feature = "svg")]
pub fn tessellate_multi_polygon_stroke(polygon: &SvgMultiPolygon, stroke_style: SvgStrokeStyle) -> TessellatedSvgNode {

    let stroke_options: StrokeOptions = translate_svg_stroke_style(stroke_style);
    let polygon = svg_multipolygon_to_lyon_path(polygon);

    let mut stroke_geometry = VertexBuffers::new();
    let mut stroke_tess = StrokeTessellator::new();

    let tess_result = stroke_tess.tessellate_path(
        &polygon,
        &stroke_options,
        &mut BuffersBuilder::new(&mut stroke_geometry, |vertex: StrokeVertex| {
            let xy_arr = vertex.position();
            SvgVertex { x: xy_arr.x, y: xy_arr.y }
        }),
    );

    if let Err(_) = tess_result {
        TessellatedSvgNode::empty()
    } else {
        vertex_buffers_to_tessellated_cpu_node(stroke_geometry)
    }
}

#[cfg(not(feature = "svg"))]
pub fn tessellate_multi_polygon_stroke(polygon: &SvgMultiPolygon, stroke_style: SvgStrokeStyle) -> TessellatedSvgNode {
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
            SvgVertex { x: xy_arr.x, y: xy_arr.y }
        })
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
            SvgVertex { x: xy_arr.x, y: xy_arr.y }
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
            SvgVertex { x: xy_arr.x, y: xy_arr.y }
        }
    ));

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
            SvgVertex { x: xy_arr.x, y: xy_arr.y }
        }
    ));


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
            SvgVertex { x: xy_arr.x, y: xy_arr.y }
        }
    ));

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
            SvgVertex { x: xy_arr.x, y: xy_arr.y }
        }
    ));

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
        SvgStyle::Fill(fs) => {
            tessellate_node_fill(&node.geometry, fs)
        },
        SvgStyle::Stroke(ss) => {
            tessellate_node_stroke(&node.geometry, ss)
        }
    }
}

#[cfg(not(feature = "svg"))]
pub fn tessellate_styled_node(node: &SvgStyledNode) -> TessellatedSvgNode {
    TessellatedSvgNode::default()
}

#[cfg(feature = "svg")]
pub fn tessellate_line_stroke(svgline: &SvgLine, stroke_style: SvgStrokeStyle) -> TessellatedSvgNode {
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
            SvgVertex { x: xy_arr.x, y: xy_arr.y }
        }),
    );

    if let Err(_) = tess_result {
        TessellatedSvgNode::empty()
    } else {
        vertex_buffers_to_tessellated_cpu_node(stroke_geometry)
    }
}

#[cfg(not(feature = "svg"))]
pub fn tessellate_line_stroke(svgline: &SvgLine, stroke_style: SvgStrokeStyle) -> TessellatedSvgNode {
    TessellatedSvgNode::default()
}

#[cfg(feature = "svg")]
pub fn tessellate_cubiccurve_stroke(svgcubiccurve: &SvgCubicCurve, stroke_style: SvgStrokeStyle) -> TessellatedSvgNode {
    let stroke_options: StrokeOptions = translate_svg_stroke_style(stroke_style);

    let mut builder = Path::builder();
    builder.begin(Point2D::new(svgcubiccurve.start.x, svgcubiccurve.start.y));
    builder.cubic_bezier_to(
        Point2D::new(svgcubiccurve.ctrl_1.x, svgcubiccurve.ctrl_1.y),
        Point2D::new(svgcubiccurve.ctrl_2.x, svgcubiccurve.ctrl_2.y),
        Point2D::new(svgcubiccurve.end.x, svgcubiccurve.end.y)
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
            SvgVertex { x: xy_arr.x, y: xy_arr.y }
        }),
    );

    if let Err(_) = tess_result {
        TessellatedSvgNode::empty()
    } else {
        vertex_buffers_to_tessellated_cpu_node(stroke_geometry)
    }
}

#[cfg(not(feature = "svg"))]
pub fn tessellate_cubiccurve_stroke(svgline: &SvgCubicCurve, stroke_style: SvgStrokeStyle) -> TessellatedSvgNode {
    TessellatedSvgNode::default()
}

#[cfg(feature = "svg")]
pub fn tessellate_quadraticcurve_stroke(svgquadraticcurve: &SvgQuadraticCurve, stroke_style: SvgStrokeStyle) -> TessellatedSvgNode {
    let stroke_options: StrokeOptions = translate_svg_stroke_style(stroke_style);

    let mut builder = Path::builder();
    builder.begin(Point2D::new(svgquadraticcurve.start.x, svgquadraticcurve.start.y));
    builder.quadratic_bezier_to(
        Point2D::new(svgquadraticcurve.ctrl.x, svgquadraticcurve.ctrl.y),
        Point2D::new(svgquadraticcurve.end.x, svgquadraticcurve.end.y)
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
            SvgVertex { x: xy_arr.x, y: xy_arr.y }
        }),
    );

    if let Err(_) = tess_result {
        TessellatedSvgNode::empty()
    } else {
        vertex_buffers_to_tessellated_cpu_node(stroke_geometry)
    }
}

#[cfg(not(feature = "svg"))]
pub fn tessellate_quadraticcurve_stroke(svgquadraticcurve: &SvgQuadraticCurve, stroke_style: SvgStrokeStyle) -> TessellatedSvgNode {
    TessellatedSvgNode::default()
}

#[cfg(feature = "svg")]
pub fn tessellate_svgpathelement_stroke(svgpathelement: &SvgPathElement, stroke_style: SvgStrokeStyle) -> TessellatedSvgNode {
    match svgpathelement {
        SvgPathElement::Line(l) => tessellate_line_stroke(l, stroke_style),
        SvgPathElement::QuadraticCurve(l) => tessellate_quadraticcurve_stroke(l, stroke_style),
        SvgPathElement::CubicCurve(l) => tessellate_cubiccurve_stroke(l, stroke_style),
    }
}

#[cfg(not(feature = "svg"))]
pub fn tessellate_svgpathelement_stroke(svgpathelement: &SvgPathElement, stroke_style: SvgStrokeStyle) -> TessellatedSvgNode {
    TessellatedSvgNode::default()
}

#[cfg(feature = "svg")]
pub fn join_tessellated_nodes(nodes: &[TessellatedSvgNode]) -> TessellatedSvgNode {

    use rayon::iter::IntoParallelRefIterator;
    use rayon::iter::ParallelIterator;
    use rayon::iter::IndexedParallelIterator;
    use rayon::iter::IntoParallelRefMutIterator;

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
    .par_iter()
    .flat_map(|t| t.vertices.clone().into_library_owned_vec())
    .collect::<Vec<_>>();

    let all_indices = nodes
    .as_ref()
    .par_iter()
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
            indices
            .par_iter_mut()
            .for_each(|i| { *i += vertex_buffer_offset; });
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

#[cfg(not(feature = "svg"))]
pub fn join_tessellated_nodes(nodes: &[TessellatedSvgNode]) -> TessellatedSvgNode {
    TessellatedSvgNode::default()
}

#[cfg(feature = "svg")]
pub fn tessellate_node_fill(node: &SvgNode, fs: SvgFillStyle) -> TessellatedSvgNode {
    use rayon::prelude::*;
    match &node {
        SvgNode::MultiPolygonCollection(ref mpc) => {
            let tessellated_multipolygons = mpc
                .as_ref()
                .par_iter()
                .map(|mp| tessellate_multi_polygon_fill(mp, fs))
                .collect::<Vec<_>>();
            join_tessellated_nodes(&tessellated_multipolygons)
        },
        SvgNode::MultiPolygon(ref mp) => tessellate_multi_polygon_fill(mp, fs),
        SvgNode::Path(ref p) => tessellate_path_fill(p, fs),
        SvgNode::Circle(ref c) => tessellate_circle_fill(c, fs),
        SvgNode::Rect(ref r) => tessellate_rect_fill(r, fs),
    }
}

#[cfg(not(feature = "svg"))]
pub fn tessellate_node_fill(node: &SvgNode, fs: SvgFillStyle) -> TessellatedSvgNode {
    TessellatedSvgNode::default()
}

#[cfg(feature = "svg")]
pub fn tessellate_node_stroke(node: &SvgNode, ss: SvgStrokeStyle) -> TessellatedSvgNode {
    use rayon::prelude::*;
    match &node {
        SvgNode::MultiPolygonCollection(ref mpc) => {
            let tessellated_multipolygons = mpc.as_ref().par_iter().map(|mp| tessellate_multi_polygon_stroke(mp, ss)).collect::<Vec<_>>();
            let mut all_vertices = Vec::new();
            let mut all_indices = Vec::new();
            for TessellatedSvgNode { vertices, indices } in tessellated_multipolygons {
                let mut vertices: Vec<SvgVertex> = vertices.into_library_owned_vec();
                let mut indices: Vec<u32> = indices.into_library_owned_vec();
                all_vertices.append(&mut vertices);
                all_indices.append(&mut indices);
                all_indices.push(GL_RESTART_INDEX);
            }
            TessellatedSvgNode { vertices: all_vertices.into(), indices: all_indices.into() }
        },
        SvgNode::MultiPolygon(ref mp) => tessellate_multi_polygon_stroke(mp, ss),
        SvgNode::Path(ref p) => tessellate_path_stroke(p, ss),
        SvgNode::Circle(ref c) => tessellate_circle_stroke(c, ss),
        SvgNode::Rect(ref r) => tessellate_rect_stroke(r, ss),
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

pub fn render_tessellated_node_gpu(
    texture: &mut Texture,
    node: &TessellatedSvgNode,
) -> Option<()> {

    use std::mem;
    use gl_context_loader::gl;
    use azul_core::gl::{GLuint, GlVoidPtrConst, VertexAttributeType};

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
    gl_context.get_integer_v(gl::ARRAY_BUFFER_BINDING, (&mut current_vertex_buffer[..]).into());
    gl_context.get_integer_v(gl::ELEMENT_ARRAY_BUFFER_BINDING, (&mut current_index_buffer[..]).into());
    gl_context.get_integer_v(gl::CURRENT_PROGRAM, (&mut current_program[..]).into());
    gl_context.get_integer_v(gl::VERTEX_ARRAY_BINDING, (&mut current_vertex_array_object[..]).into());
    gl_context.get_integer_v(gl::FRAMEBUFFER, (&mut current_framebuffers[..]).into());
    gl_context.get_integer_v(gl::TEXTURE_2D, (&mut current_texture_2d[..]).into());
    gl_context.get_boolean_v(gl::PRIMITIVE_RESTART_FIXED_INDEX, (&mut current_primitive_restart_enabled[..]).into());

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
        GlVoidPtrConst { ptr: &node.vertices as *const _ as *const std::ffi::c_void, run_destructor: true },
        gl::STATIC_DRAW
    );

    gl_context.bind_buffer(gl::ELEMENT_ARRAY_BUFFER, *index_buffer_id);
    gl_context.buffer_data_untyped(
        gl::ELEMENT_ARRAY_BUFFER,
        (mem::size_of::<u32>() * node.indices.len()) as isize,
        GlVoidPtrConst { ptr: &node.indices as *const _ as *const std::ffi::c_void, run_destructor: true },
        gl::STATIC_DRAW
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
    gl_context.tex_image_2d(gl::TEXTURE_2D, 0, gl::R8 as i32, texture_size.width as i32, texture_size.height as i32, 0, gl::RED, gl::UNSIGNED_BYTE, None.into());
    gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::NEAREST as i32);
    gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::NEAREST as i32);
    gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as i32);
    gl_context.tex_parameter_i(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as i32);

    let framebuffers = gl_context.gen_framebuffers(1);
    let framebuffer_id = framebuffers.get(0)?;
    gl_context.bind_framebuffer(gl::FRAMEBUFFER, *framebuffer_id);

    gl_context.framebuffer_texture_2d(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, texture.texture_id, 0);
    gl_context.draw_buffers([gl::COLOR_ATTACHMENT0][..].into());
    gl_context.viewport(0, 0, texture_size.width as i32, texture_size.height as i32);

    debug_assert!(gl_context.check_frame_buffer_status(gl::FRAMEBUFFER) == gl::FRAMEBUFFER_COMPLETE);

    gl_context.use_program(svg_shader);
    gl_context.disable(gl::MULTISAMPLE);

    let bbox_uniform_location = gl_context.get_uniform_location(svg_shader, "vBboxSize".into());

    gl_context.clear_color(0.0, 0.0, 0.0, 1.0);
    gl_context.clear(gl::COLOR_BUFFER_BIT);
    gl_context.bind_vertex_array(*vertex_buffer_id);
    gl_context.bind_buffer(gl::ELEMENT_ARRAY_BUFFER, *index_buffer_id);
    gl_context.uniform_2f(bbox_uniform_location, texture_size.width as f32, texture_size.height as f32);
    gl_context.draw_elements(gl::TRIANGLES, node.indices.len() as i32, INDEX_TYPE, 0);

    // stage 4: cleanup - reset the OpenGL state
    if u32::from(current_multisample[0]) == gl::TRUE { gl_context.enable(gl::MULTISAMPLE); }
    if u32::from(current_primitive_restart_enabled[0]) == gl::FALSE { gl_context.disable(gl::PRIMITIVE_RESTART_FIXED_INDEX); }
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

    use tiny_skia::{
        Pixmap as SkPixmap,
        Paint as SkPaint,
        Path as SkPath,
        FillRule as SkFillRule,
        PathBuilder as SkPathBuilder,
        LineJoin as SkLineJoin,
        LineCap as SkLineCap,
        Transform as SkTransform,
        Rect as SkRect,
        Stroke as SkStroke,
        StrokeDash as SkStrokeDash,
    };
    use azul_core::app_resources::RawImageData;

    fn tiny_skia_translate_node(node: &SvgNode) -> Option<SkPath> {

        macro_rules! build_path {($path_builder:expr, $p:expr) => ({
            if $p.items.as_ref().is_empty() {
                return None;
            }

            let start = $p.items.as_ref()[0].get_start();
            $path_builder.move_to(start.x, start.y);

            for path_element in $p.items.as_ref() {
                match path_element {
                    SvgPathElement::Line(l) => {
                        $path_builder.line_to(l.end.x, l.end.y);
                    },
                    SvgPathElement::QuadraticCurve(qc) => {
                        $path_builder.quad_to(
                            qc.ctrl.x, qc.ctrl.y,
                            qc.end.x, qc.end.y
                        );
                    },
                    SvgPathElement::CubicCurve(cc) => {
                        $path_builder.cubic_to(
                            cc.ctrl_1.x, cc.ctrl_1.y,
                            cc.ctrl_2.x, cc.ctrl_2.y,
                            cc.end.x, cc.end.y
                        );
                    },
                }
            }

            if $p.is_closed() {
                $path_builder.close();
            }
        })}

        match node {
            SvgNode::MultiPolygonCollection(mpc) => {
                let mut path_builder = SkPathBuilder::new();
                for mp in mpc.iter() {
                    for p in mp.rings.iter() {
                        build_path!(path_builder, p);
                    }
                }
                path_builder.finish()
            },
            SvgNode::MultiPolygon(mp) => {
                let mut path_builder = SkPathBuilder::new();
                for p in mp.rings.iter() {
                    build_path!(path_builder, p);
                }
                path_builder.finish()
            },
            SvgNode::Path(p) => {
                let mut path_builder = SkPathBuilder::new();
                build_path!(path_builder, p);
                path_builder.finish()
            },
            SvgNode::Circle(c) => {
                SkPathBuilder::from_circle(c.center_x, c.center_y, c.radius)
            },
            SvgNode::Rect(r) => {
                // TODO: rounded edges!
                Some(SkPathBuilder::from_rect(SkRect::from_xywh(r.x, r.y, r.width, r.height)?))
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
        },
        SvgStyle::Stroke(ss) => {
            let stroke = SkStroke {
                width: ss.line_width,
                miter_limit: ss.miter_limit,
                line_cap: match ss.start_cap { // TODO: end_cap?
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
                    SkStrokeDash::new(vec![
                        d.length_1,
                        d.gap_1,
                        d.length_2,
                        d.gap_2,
                        d.length_3,
                        d.gap_3,
                    ], d.offset)
                }),
            };
            pixmap.stroke_path(
                &path,
                &paint,
                &stroke,
                transform,
                clip_mask,
            )?;
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

impl Clone for SvgXmlNode {
    fn clone(&self) -> Self {
        Self {
            node: self.node.clone(),
            run_destructor: true,
        }
    }
}

impl Drop for SvgXmlNode {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}

#[cfg(not(feature = "svg"))]
pub use azul_core::svg::SvgXmlNode;

#[cfg(feature = "svg")]
fn svgxmlnode_new(node: usvg::Node) -> SvgXmlNode { SvgXmlNode { node: Box::new(node), run_destructor: true } }

#[cfg(feature = "svg")]
pub fn svgxmlnode_parse(svg_file_data: &[u8], options: SvgParseOptions) -> Result<SvgXmlNode, SvgParseError> {
    let svg = svg_parse(svg_file_data, options)?;
    Ok(svg_root(&svg))
}

#[cfg(not(feature = "svg"))]
pub fn svgxmlnode_parse(svg_file_data: &[u8], options: SvgParseOptions) -> Result<SvgXmlNode, SvgParseError> {
    Err(SvgParseError::NoParserAvailable)
}

#[cfg(feature = "svg")]
pub fn svgxmlnode_render(s: &SvgXmlNode, options: SvgRenderOptions) -> Option<RawImage> {
    use tiny_skia::Pixmap;
    use azul_core::app_resources::RawImageData;

    let (target_width, target_height) = svgrenderoptions_get_width_height_node(&options, &s.node)?;

    if target_height == 0 || target_width == 0 { return None; }

    let mut pixmap = Pixmap::new(target_width, target_height)?;
    pixmap.fill(options.background_color.into_option().map(translate_color).unwrap_or(tiny_skia::Color::TRANSPARENT));

    let _ = resvg::render_node(&s.node, translate_fit_to(options.fit), pixmap.as_mut())?;

    Some(RawImage {
        pixels: RawImageData::U8(pixmap.take().into()),
        width: target_width as usize,
        height: target_height as usize,
        premultiplied_alpha: true,
        data_format: RawImageFormat::RGBA8,
    })
}

#[cfg(not(feature = "svg"))]
pub fn svgxmlnode_render(s: &SvgXmlNode, options: SvgRenderOptions) -> Option<RawImage> {
    None
}

#[cfg(feature = "svg")]
pub fn svgxmlnode_to_string(s: &SvgXmlNode, options: SvgXmlOptions) -> String {
    use usvg::NodeExt;
    s.node.tree().to_string(translate_to_usvg_xmloptions(options))
}

#[cfg(not(feature = "svg"))]
pub fn svgxmlnode_to_string(s: &SvgXmlNode, options: SvgXmlOptions) -> String {
    String::new()
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

impl Clone for Svg {
    fn clone(&self) -> Self {
        Self {
            tree: self.tree.clone(),
            run_destructor: true,
        }
    }
}

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
fn svg_new(tree: usvg::Tree) -> Svg { Svg { tree: Box::new(tree), run_destructor: true, } }

/// NOTE: SVG file data may be Zlib compressed
#[cfg(feature = "svg")]
pub fn svg_parse(svg_file_data: &[u8], options: SvgParseOptions) -> Result<Svg, SvgParseError> {
    let rtree = usvg::Tree::from_data(svg_file_data, &translate_to_usvg_parseoptions(options))
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
    SvgXmlNode { node: core::ptr::null_mut() }
}

#[cfg(feature = "svg")]
pub fn svg_render(s: &Svg, options: SvgRenderOptions) -> Option<RawImage> {
    use tiny_skia::Pixmap;
    use azul_core::app_resources::RawImageData;

    let root = s.tree.root();
    let (target_width, target_height) = svgrenderoptions_get_width_height_node(&options, &root)?;

    if target_height == 0 || target_width == 0 { return None; }

    let mut pixmap = Pixmap::new(target_width, target_height)?;
    pixmap.fill(options.background_color.into_option().map(translate_color).unwrap_or(tiny_skia::Color::TRANSPARENT));

    let _ = resvg::render_node(&root, translate_fit_to(options.fit), pixmap.as_mut())?;

    Some(RawImage {
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
    s.tree.to_string(translate_to_usvg_xmloptions(options))
}

#[cfg(not(feature = "svg"))]
pub fn svg_to_string(s: &Svg, options: SvgXmlOptions) -> String {
    String::new()
}

#[cfg(feature = "svg")]
fn svgrenderoptions_get_width_height_node(s: &SvgRenderOptions, node: &usvg::Node) -> Option<(u32, u32)> {
    match s.target_size.as_ref() {
        None => {
            use usvg::NodeExt;
            let wh = node.calculate_bbox()?.size().to_screen_size();
            Some((wh.width(), wh.height()))
        },
        Some(s) => Some((s.width as u32, s.height as u32)),
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
        // path: e.relative_image_path.into_option().map(|e| { let p: String = e.clone().into(); PathBuf::from(p) }),
        dpi: e.dpi as f64,
        font_family: e.default_font_family.clone().into_library_owned_string(),
        font_size: e.font_size.into(),
        languages: e.languages.as_ref().iter().map(|e| e.clone().into_library_owned_string()).collect(),
        shape_rendering: translate_to_usvg_shaperendering(e.shape_rendering),
        text_rendering: translate_to_usvg_textrendering(e.text_rendering),
        image_rendering: translate_to_usvg_imagerendering(e.image_rendering),
        keep_named_groups: e.keep_named_groups,
        .. usvg::Options::default()
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
        use_single_quote: f.use_single_quote,
        indent: translate_usvg_xmlindent(f.indent),
        attributes_indent: translate_usvg_xmlindent(f.attributes_indent),
    }
}

#[cfg(feature = "svg")]
fn translate_usvg_svgparserror(e: usvg::Error) -> SvgParseError {
    use crate::xml::translate_roxmltree_error;
    match e {
        usvg::Error::InvalidFileSuffix => SvgParseError::InvalidFileSuffix,
        usvg::Error::FileOpenFailed => SvgParseError::FileOpenFailed,
        usvg::Error::NotAnUtf8Str => SvgParseError::NotAnUtf8Str,
        usvg::Error::MalformedGZip => SvgParseError::MalformedGZip,
        usvg::Error::InvalidSize => SvgParseError::InvalidSize,
        usvg::Error::ParsingFailed(e) => SvgParseError::ParsingFailed(translate_roxmltree_error(e)),
    }
}

#[cfg(feature = "svg")]
fn translate_usvg_xmlindent(f: Indent) -> usvg::XmlIndent {
    match f {
        Indent::None => usvg::XmlIndent::None,
        Indent::Spaces(s) => usvg::XmlIndent::Spaces(s),
        Indent::Tabs => usvg::XmlIndent::Tabs,
    }
}