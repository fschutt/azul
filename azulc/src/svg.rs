use core::fmt;
use azul_core::{
    svg::*,
    app_resources::{RawImage, RawImageFormat},
};
use azul_css::{OptionI16, OptionU16, U8Vec, OptionAzString, OptionColorU, AzString, StringVec, ColorU};
use owned_ttf_parser::Font as TTFFont;
use lyon::{
    tessellation::{
        FillOptions, FillAttributes, StrokeAttributes, BuffersBuilder,
        FillTessellator, VertexBuffers, StrokeTessellator, StrokeOptions,
        basic_shapes::{
            fill_circle, stroke_circle, fill_rounded_rectangle,
            stroke_rounded_rectangle, BorderRadii
        },
    },
    math::Point,
    path::Path,
    geom::euclid::{Point2D, Rect, Size2D, UnknownUnit},
};
use crate::xml::XmlError;
use alloc::boxed::Box;

pub enum c_void { }

const GL_RESTART_INDEX: u32 = core::u32::MAX;

pub type GlyphId = u16;


fn translate_svg_line_join(e: SvgLineJoin) -> lyon::tessellation::LineJoin {
    use azul_core::svg::SvgLineJoin::*;
    match e {
        Miter => lyon::tessellation::LineJoin::Miter,
        MiterClip => lyon::tessellation::LineJoin::MiterClip,
        Round => lyon::tessellation::LineJoin::Round,
        Bevel => lyon::tessellation::LineJoin::Bevel,
    }
}

fn translate_svg_line_cap(e: SvgLineCap) -> lyon::tessellation::LineCap {
    use azul_core::svg::SvgLineCap::*;
    match e {
        Butt => lyon::tessellation::LineCap::Butt,
        Square => lyon::tessellation::LineCap::Square,
        Round => lyon::tessellation::LineCap::Round,
    }
}

fn translate_svg_stroke_style(e: SvgStrokeStyle) -> lyon::tessellation::StrokeOptions {
    let target = lyon::tessellation::StrokeOptions::tolerance(e.get_tolerance())
        .with_start_cap(translate_svg_line_cap(e.start_cap))
        .with_end_cap(translate_svg_line_cap(e.end_cap))
        .with_line_join(translate_svg_line_join(e.line_join))
        .with_line_width(e.get_line_width())
        .with_miter_limit(e.get_miter_limit());

    if !e.apply_line_width {
        target.dont_apply_line_width()
    } else {
        target
    }
}

fn svg_multipolygon_to_lyon_path(polygon: &SvgMultiPolygon) -> Path {

    let mut builder = Path::builder();

    for p in polygon.rings.as_ref().iter() {
        if p.items.as_ref().is_empty() {
            continue;
        }

        let start_item = p.items.as_ref()[0];
        let first_point = Point2D::new(start_item.get_start().x, -(start_item.get_start().y));

        builder.move_to(first_point);

        for q in p.items.as_ref().iter().rev() /* NOTE: REVERSE ITERATOR */ {
            match q {
                SvgPathElement::Line(l) => {
                    builder.line_to(Point2D::new(l.end.x, -(l.end.y)));
                },
                SvgPathElement::QuadraticCurve(qc) => {
                    builder.quadratic_bezier_to(
                        Point2D::new(qc.ctrl.x, -(qc.ctrl.y)),
                        Point2D::new(qc.end.x, -(qc.end.y))
                    );
                },
                SvgPathElement::CubicCurve(cc) => {
                    builder.cubic_bezier_to(
                        Point2D::new(cc.ctrl_1.x, -(cc.ctrl_1.y)),
                        Point2D::new(cc.ctrl_2.x, -(cc.ctrl_2.y)),
                        Point2D::new(cc.end.x, -(cc.end.y))
                    );
                },
            }
        }

        if p.is_closed() {
            builder.close();
        }
    }

    builder.build()
}

fn svg_path_to_lyon_path_events(path: &SvgPath) -> Path {

    let mut builder = Path::builder();

    if !path.items.as_ref().is_empty() {

        let start_item = path.items.as_ref()[0];
        let first_point = Point2D::new(start_item.get_start().x, -(start_item.get_start().y));

        builder.move_to(first_point);

        for p in path.items.as_ref().iter() {
            match p {
                SvgPathElement::Line(l) => {
                    builder.line_to(Point2D::new(l.end.x, -(l.end.y)));
                },
                SvgPathElement::QuadraticCurve(qc) => {
                    builder.quadratic_bezier_to(
                        Point2D::new(qc.ctrl.x, -(qc.ctrl.y)),
                        Point2D::new(qc.end.x, -(qc.end.y))
                    );
                },
                SvgPathElement::CubicCurve(cc) => {
                    builder.cubic_bezier_to(
                        Point2D::new(cc.ctrl_1.x, -(cc.ctrl_1.y)),
                        Point2D::new(cc.ctrl_2.x, -(cc.ctrl_2.y)),
                        Point2D::new(cc.end.x, -(cc.end.y))
                    );
                },
            }
        }

        if path.is_closed() {
            builder.close();
        }
    }

    builder.build()
}

#[inline]
fn vertex_buffers_to_tesselated_cpu_node(v: VertexBuffers<SvgVertex, u32>) -> TesselatedCPUSvgNode {
    TesselatedCPUSvgNode {
        vertices: v.vertices.into(),
        indices: v.indices.into(),
    }
}


pub fn tesselate_multi_polygon_fill(polygon: &SvgMultiPolygon, fill_style: SvgFillStyle) -> TesselatedCPUSvgNode {

    let polygon = svg_multipolygon_to_lyon_path(polygon);

    let mut geometry = VertexBuffers::new();
    let mut tessellator = FillTessellator::new();

    let tess_result = tessellator.tessellate_path(
        &polygon,
        &FillOptions::tolerance(fill_style.get_tolerance()),
        &mut BuffersBuilder::new(&mut geometry, |pos: Point, _: FillAttributes| {
            let xy_arr = pos.to_array();
            SvgVertex { x: xy_arr[0], y: xy_arr[1] }
        })
    );

    if let Err(_) = tess_result {
        TesselatedCPUSvgNode::empty()
    } else {
        vertex_buffers_to_tesselated_cpu_node(geometry)
    }
}

pub fn tesselate_multi_polygon_stroke(polygon: &SvgMultiPolygon, stroke_style: SvgStrokeStyle) -> TesselatedCPUSvgNode {

    let stroke_options: StrokeOptions = translate_svg_stroke_style(stroke_style);
    let polygon = svg_multipolygon_to_lyon_path(polygon);

    let mut stroke_geometry = VertexBuffers::new();
    let mut stroke_tess = StrokeTessellator::new();

    let tess_result = stroke_tess.tessellate_path(
        &polygon,
        &stroke_options,
        &mut BuffersBuilder::new(&mut stroke_geometry, |pos: Point, _: StrokeAttributes| {
            let xy_arr = pos.to_array();
            SvgVertex { x: xy_arr[0], y: xy_arr[1] }
        }),
    );

    if let Err(_) = tess_result {
        TesselatedCPUSvgNode::empty()
    } else {
        vertex_buffers_to_tesselated_cpu_node(stroke_geometry)
    }
}

pub fn tesselate_path_fill(path: &SvgPath, fill_style: SvgFillStyle) -> TesselatedCPUSvgNode {

    let polygon = svg_path_to_lyon_path_events(path);

    let mut geometry = VertexBuffers::new();
    let mut tessellator = FillTessellator::new();

    let tess_result = tessellator.tessellate_path(
        &polygon,
        &FillOptions::tolerance(fill_style.get_tolerance()),
        &mut BuffersBuilder::new(&mut geometry, |pos: Point, _: FillAttributes| {
            let xy_arr = pos.to_array();
            SvgVertex { x: xy_arr[0], y: xy_arr[1] }
        })
    );

    if let Err(_) = tess_result {
        TesselatedCPUSvgNode::empty()
    } else {
        vertex_buffers_to_tesselated_cpu_node(geometry)
    }
}

pub fn tesselate_path_stroke(path: &SvgPath, stroke_style: SvgStrokeStyle) -> TesselatedCPUSvgNode {

    let stroke_options: StrokeOptions = translate_svg_stroke_style(stroke_style);
    let polygon = svg_path_to_lyon_path_events(path);

    let mut stroke_geometry = VertexBuffers::new();
    let mut stroke_tess = StrokeTessellator::new();

    let tess_result = stroke_tess.tessellate_path(
        &polygon,
        &stroke_options,
        &mut BuffersBuilder::new(&mut stroke_geometry, |pos: Point, _: StrokeAttributes| {
            let xy_arr = pos.to_array();
            SvgVertex { x: xy_arr[0], y: xy_arr[1] }
        }),
    );

    if let Err(_) = tess_result {
        TesselatedCPUSvgNode::empty()
    } else {
        vertex_buffers_to_tesselated_cpu_node(stroke_geometry)
    }
}

pub fn tesselate_circle_fill(c: &SvgCircle, fill_style: SvgFillStyle) -> TesselatedCPUSvgNode {
    let center = Point2D::new(c.center_x, c.center_y);

    let mut geometry = VertexBuffers::new();

    let tess_result = fill_circle(
        center,
        c.radius,
        &FillOptions::tolerance(fill_style.get_tolerance()),
        &mut BuffersBuilder::new(&mut geometry, |pos: Point| {
            let xy_arr = pos.to_array();
            SvgVertex { x: xy_arr[0], y: xy_arr[1] }
        }
    ));

    if let Err(_) = tess_result {
        TesselatedCPUSvgNode::empty()
    } else {
        vertex_buffers_to_tesselated_cpu_node(geometry)
    }
}

pub fn tesselate_circle_stroke(c: &SvgCircle, stroke_style: SvgStrokeStyle) -> TesselatedCPUSvgNode {

    let stroke_options: StrokeOptions = translate_svg_stroke_style(stroke_style);
    let center = Point2D::new(c.center_x, c.center_y);

    let mut stroke_geometry = VertexBuffers::new();

    let tess_result = stroke_circle(
        center,
        c.radius,
        &stroke_options,
        &mut BuffersBuilder::new(&mut stroke_geometry, |pos: Point, _: StrokeAttributes| {
            let xy_arr = pos.to_array();
            SvgVertex { x: xy_arr[0], y: xy_arr[1] }
        }
    ));


    if let Err(_) = tess_result {
        TesselatedCPUSvgNode::empty()
    } else {
        vertex_buffers_to_tesselated_cpu_node(stroke_geometry)
    }
}

fn get_radii(r: &SvgRect) -> (Rect<f32, UnknownUnit>, BorderRadii) {
    let rect = Rect::new(Point2D::new(r.x, r.y), Size2D::new(r.width, r.height));
    let radii = BorderRadii {
        top_left: r.radius_top_left,
        top_right: r.radius_top_right,
        bottom_left: r.radius_bottom_left,
        bottom_right: r.radius_bottom_right
    };
    (rect, radii)
}

pub fn tesselate_rect_fill(r: &SvgRect, fill_style: SvgFillStyle) -> TesselatedCPUSvgNode {
    let (rect, radii) = get_radii(&r);
    let mut geometry = VertexBuffers::new();

    let tess_result = fill_rounded_rectangle(
        &rect,
        &radii,
        &FillOptions::tolerance(fill_style.get_tolerance()),
        &mut BuffersBuilder::new(&mut geometry, |pos: Point| {
            let xy_arr = pos.to_array();
            SvgVertex { x: xy_arr[0], y: xy_arr[1] }
        }
    ));

    if let Err(_) = tess_result {
        TesselatedCPUSvgNode::empty()
    } else {
        vertex_buffers_to_tesselated_cpu_node(geometry)
    }
}

pub fn tesselate_rect_stroke(r: &SvgRect, stroke_style: SvgStrokeStyle) -> TesselatedCPUSvgNode {

    let stroke_options: StrokeOptions = translate_svg_stroke_style(stroke_style);
    let (rect, radii) = get_radii(&r);

    let mut stroke_geometry = VertexBuffers::new();

    let tess_result = stroke_rounded_rectangle(
        &rect,
        &radii,
        &stroke_options,
        &mut BuffersBuilder::new(&mut stroke_geometry, |pos: Point, _: StrokeAttributes| {
            let xy_arr = pos.to_array();
            SvgVertex { x: xy_arr[0], y: xy_arr[1] }
        }
    ));

    if let Err(_) = tess_result {
        TesselatedCPUSvgNode::empty()
    } else {
        vertex_buffers_to_tesselated_cpu_node(stroke_geometry)
    }
}

/// Tesselate the path using lyon
pub fn tesselate_styled_node(node: &SvgStyledNode) -> TesselatedCPUSvgNode {
    match node.style {
        SvgStyle::Fill(fs) => {
            tesselate_node_fill(&node.geometry, fs)
        },
        SvgStyle::Stroke(ss) => {
            tesselate_node_stroke(&node.geometry, ss)
        }
    }
}

pub fn tesselate_node_fill(node: &SvgNode, fs: SvgFillStyle) -> TesselatedCPUSvgNode {
    use rayon::prelude::*;
    match &node {
        SvgNode::MultiPolygonCollection(ref mpc) => {
            let tesselated_multipolygons = mpc.as_ref().par_iter().map(|mp| tesselate_multi_polygon_fill(mp, fs)).collect::<Vec<_>>();
            let mut all_vertices = Vec::new();
            let mut all_indices = Vec::new();
            for TesselatedCPUSvgNode { vertices, indices } in tesselated_multipolygons {
                let mut vertices: Vec<SvgVertex> = vertices.into_library_owned_vec();
                let mut indices: Vec<u32> = indices.into_library_owned_vec();
                all_vertices.append(&mut vertices);
                all_indices.append(&mut indices);
                all_indices.push(GL_RESTART_INDEX);
            }
            TesselatedCPUSvgNode { vertices: all_vertices.into(), indices: all_indices.into() }
        },
        SvgNode::MultiPolygon(ref mp) => tesselate_multi_polygon_fill(mp, fs),
        SvgNode::Path(ref p) => tesselate_path_fill(p, fs),
        SvgNode::Circle(ref c) => tesselate_circle_fill(c, fs),
        SvgNode::Rect(ref r) => tesselate_rect_fill(r, fs),
    }
}

pub fn tesselate_node_stroke(node: &SvgNode, ss: SvgStrokeStyle) -> TesselatedCPUSvgNode {
    use rayon::prelude::*;
    match &node {
        SvgNode::MultiPolygonCollection(ref mpc) => {
            let tesselated_multipolygons = mpc.as_ref().par_iter().map(|mp| tesselate_multi_polygon_stroke(mp, ss)).collect::<Vec<_>>();
            let mut all_vertices = Vec::new();
            let mut all_indices = Vec::new();
            for TesselatedCPUSvgNode { vertices, indices } in tesselated_multipolygons {
                let mut vertices: Vec<SvgVertex> = vertices.into_library_owned_vec();
                let mut indices: Vec<u32> = indices.into_library_owned_vec();
                all_vertices.append(&mut vertices);
                all_indices.append(&mut indices);
                all_indices.push(GL_RESTART_INDEX);
            }
            TesselatedCPUSvgNode { vertices: all_vertices.into(), indices: all_indices.into() }
        },
        SvgNode::MultiPolygon(ref mp) => tesselate_multi_polygon_stroke(mp, ss),
        SvgNode::Path(ref p) => tesselate_path_stroke(p, ss),
        SvgNode::Circle(ref c) => tesselate_circle_stroke(c, ss),
        SvgNode::Rect(ref r) => tesselate_rect_stroke(r, ss),
    }
}

// ---------------------------- SVG RENDERING

#[repr(C)]
pub struct SvgXmlNode {
    ptr: *mut c_void, // usvg::Node
}

impl Clone for SvgXmlNode {
    fn clone(&self) -> Self {
        Self::new(self.get_node().clone())
    }
}

impl fmt::Debug for SvgXmlNode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.get_node().fmt(f)
    }
}

impl SvgXmlNode {

    fn new(node: usvg::Node) -> Self { Self { ptr: Box::into_raw(Box::new(node)) as *mut c_void } }
    fn get_node<'a>(&'a self) -> &'a usvg::Node { unsafe { &*(self.ptr as *mut usvg::Node) } }

    pub fn parse(svg_file_data: &[u8], options: SvgParseOptions) -> Result<SvgXmlNode, SvgParseError> {
        let svg = Svg::parse(svg_file_data, options)?;
        Ok(svg.root())
    }

    /*
    pub fn from_xml(xml: Xml) -> Result<Self, SvgParseError> {
        // https://github.com/RazrFalcon/resvg/issues/308
        Ok(Svg::new(xml.into_tree()))
    }

    pub fn render_to_image(&self, options: SvgRenderOptions) -> Option<RawImage> {
        resvg::render_node(self.get_node(), translate_fit_to(options.fit), options.background_color.into_option().map(translate_color)).map(translate_image)
    }
    */
}

impl Drop for SvgXmlNode {
    fn drop(&mut self) { let _ = unsafe { Box::from_raw(self.ptr as *mut usvg::Node) }; }
}

#[repr(C)]
pub struct Svg {
    ptr: *mut c_void, // *mut usvg::Tree,
}

impl Clone for Svg {
    fn clone(&self) -> Self {
        Self::new(self.get_tree().clone())
    }
}

impl fmt::Debug for Svg {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.to_string(SvgXmlOptions::default()).fmt(f)
    }
}

impl PartialOrd for Svg {
    fn partial_cmp(&self, rhs: &Self) -> Option<core::cmp::Ordering> {
        self.to_string(SvgXmlOptions::default()).partial_cmp(&rhs.to_string(SvgXmlOptions::default()))
    }
}

impl Ord for Svg {
    fn cmp(&self, rhs: &Self) -> core::cmp::Ordering {
        self.to_string(SvgXmlOptions::default()).cmp(&rhs.to_string(SvgXmlOptions::default()))
    }
}

impl PartialEq for Svg {
    fn eq(&self, rhs: &Self) -> bool {
        self.to_string(SvgXmlOptions::default()).eq(&rhs.to_string(SvgXmlOptions::default()))
    }
}

impl Eq for Svg { }

impl core::hash::Hash for Svg {
    fn hash<H>(&self, state: &mut H) where H: core::hash::Hasher {
        self.to_string(SvgXmlOptions::default()).hash(state)
    }
}

/*
#[inline]
fn translate_image(img: resvg::Image) -> RawImage {
    let width = img.width();
    let height = img.height();
    RawImage {
        pixels: img.take().into(),
        width: width as usize,
        height: height as usize,
        data_format: RawImageFormat::RGBA8,
    }
}
*/

impl Svg {

    fn new(tree: usvg::Tree) -> Self { Self { ptr: Box::into_raw(Box::new(tree)) as *mut c_void } }
    fn get_tree<'a>(&'a self) -> &'a usvg::Tree { unsafe { &*(self.ptr as *mut usvg::Tree) } }

    /*
    pub fn from_xml(xml: Xml) -> Result<Self, SvgParseError> {
        // https://github.com/RazrFalcon/resvg/issues/308
        Ok(Svg::new(xml.into_tree()))
    }
    */

    /// NOTE: SVG file data may be Zlib compressed
    pub fn parse(svg_file_data: &[u8], options: SvgParseOptions) -> Result<Svg, SvgParseError> {
        let rtree = usvg::Tree::from_data(svg_file_data, &options.into())?;
        Ok(Svg::new(rtree))
    }

    pub fn root(&self) -> SvgXmlNode {
        SvgXmlNode::new(self.get_tree().root())
    }

    /*
    pub fn render_to_image(&self, options: SvgRenderOptions) -> Option<RawImage> {
        resvg::render(self.get_tree(), translate_fit_to(options.fit), options.background_color.into_option().map(translate_color)).map(translate_image)
    }
    */

    pub fn to_string(&self, options: SvgXmlOptions) -> String {
        self.get_tree().to_string(options.into())
    }
}

impl Drop for Svg {
    fn drop(&mut self) {
        let _ = unsafe { Box::from_raw(self.ptr as *mut usvg::Tree) };
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub enum ShapeRendering {
    OptimizeSpeed,
    CrispEdges,
    GeometricPrecision,
}

impl From<ShapeRendering> for usvg::ShapeRendering {
    fn from(e: ShapeRendering) -> usvg::ShapeRendering {
        match e {
            ShapeRendering::OptimizeSpeed => usvg::ShapeRendering::OptimizeSpeed,
            ShapeRendering::CrispEdges => usvg::ShapeRendering::CrispEdges,
            ShapeRendering::GeometricPrecision => usvg::ShapeRendering::GeometricPrecision,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub enum ImageRendering {
    OptimizeQuality,
    OptimizeSpeed,
}

impl From<ImageRendering> for usvg::ImageRendering {
    fn from(e: ImageRendering) -> usvg::ImageRendering {
        match e {
            ImageRendering::OptimizeQuality => usvg::ImageRendering::OptimizeQuality,
            ImageRendering::OptimizeSpeed => usvg::ImageRendering::OptimizeSpeed,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub enum TextRendering {
    OptimizeSpeed,
    OptimizeLegibility,
    GeometricPrecision,
}

impl From<TextRendering> for usvg::TextRendering {
    fn from(e: TextRendering) -> usvg::TextRendering {
        match e {
            TextRendering::OptimizeSpeed => usvg::TextRendering::OptimizeSpeed,
            TextRendering::OptimizeLegibility => usvg::TextRendering::OptimizeLegibility,
            TextRendering::GeometricPrecision => usvg::TextRendering::GeometricPrecision,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub enum FontDatabase {
    Empty,
    System,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgRenderOptions {
    pub background_color: OptionColorU,
    pub fit: SvgFitTo,
}

#[allow(dead_code)]
const fn translate_color(i: ColorU) -> usvg::Color {
    usvg::Color { red: i.r, green: i.g, blue: i.b }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C, u8)]
pub enum SvgFitTo {
    Original,
    Width(u32),
    Height(u32),
    Zoom(f32),
}

impl Default for SvgFitTo {
    fn default() -> Self { SvgFitTo::Original }
}

#[allow(dead_code)]
const fn translate_fit_to(i: SvgFitTo) -> usvg::FitTo {
    match i {
        SvgFitTo::Original => usvg::FitTo::Original,
        SvgFitTo::Width(w) => usvg::FitTo::Width(w),
        SvgFitTo::Height(h) => usvg::FitTo::Height(h),
        SvgFitTo::Zoom(z) => usvg::FitTo::Zoom(z),
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgParseOptions {
    /// SVG image path. Used to resolve relative image paths.
    pub relative_image_path: OptionAzString,
    /// Target DPI. Impact units conversion. Default: 96.0
    pub dpi: f32,
    /// Default font family. Will be used when no font-family attribute is set in the SVG. Default: Times New Roman
    pub default_font_family: AzString,
    /// A default font size. Will be used when no font-size attribute is set in the SVG. Default: 12
    pub font_size: f32,
    /// A list of languages. Will be used to resolve a systemLanguage conditional attribute. Format: en, en-US. Default: [en]
    pub languages: StringVec,
    /// Specifies the default shape rendering method. Will be used when an SVG element's shape-rendering property is set to auto. Default: GeometricPrecision
    pub shape_rendering: ShapeRendering,
    /// Specifies the default text rendering method. Will be used when an SVG element's text-rendering property is set to auto. Default: OptimizeLegibility
    pub text_rendering: TextRendering,
    /// Specifies the default image rendering method. Will be used when an SVG element's image-rendering property is set to auto. Default: OptimizeQuality
    pub image_rendering: ImageRendering,
    /// Keep named groups. If set to true, all non-empty groups with id attribute will not be removed. Default: false
    pub keep_named_groups: bool,
    /// When empty, text elements will be skipped. Default: `System`
    pub fontdb: FontDatabase,
}

impl Default for SvgParseOptions {
    fn default() -> Self {
        let lang_vec: Vec<AzString> = vec![String::from("en").into()];
        SvgParseOptions {
            relative_image_path: OptionAzString::None,
            dpi: 96.0,
            default_font_family: "Times New Roman".to_string().into(),
            font_size: 12.0,
            languages: lang_vec.into(),
            shape_rendering: ShapeRendering::GeometricPrecision,
            text_rendering: TextRendering::OptimizeLegibility,
            image_rendering: ImageRendering::OptimizeQuality,
            keep_named_groups: false,
            fontdb: FontDatabase::System,
        }
    }
}

impl From<SvgParseOptions> for usvg::Options {
    fn from(e: SvgParseOptions) -> usvg::Options {

        let mut options = usvg::Options {
            // path: e.relative_image_path.into_option().map(|e| { let p: String = e.clone().into(); PathBuf::from(p) }),
            dpi: e.dpi as f64,
            font_family: e.default_font_family.clone().into_library_owned_string(),
            font_size: e.font_size.into(),
            languages: e.languages.as_ref().iter().map(|e| e.clone().into_library_owned_string()).collect(),
            shape_rendering: e.shape_rendering.into(),
            text_rendering: e.text_rendering.into(),
            image_rendering: e.image_rendering.into(),
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
}

#[repr(C)]
pub struct SvgXmlOptions {
    pub use_single_quote: bool,
    pub indent: Indent,
    pub attributes_indent: Indent,
}

impl Default for SvgXmlOptions {
    fn default() -> Self {
        SvgXmlOptions {
            use_single_quote: false,
            indent: Indent::Spaces(2),
            attributes_indent: Indent::Spaces(2),
        }
    }
}
impl From<SvgXmlOptions> for usvg::XmlOptions {
    fn from(f: SvgXmlOptions) -> usvg::XmlOptions {
        usvg::XmlOptions {
            use_single_quote: f.use_single_quote,
            indent: f.indent.into(),
            attributes_indent: f.attributes_indent.into(),
        }
    }
}

#[derive(Debug, PartialEq, PartialOrd, Clone)]
#[repr(C, u8)]
pub enum SvgParseError {
    InvalidFileSuffix,
    FileOpenFailed,
    NotAnUtf8Str,
    MalformedGZip,
    InvalidSize,
    ParsingFailed(XmlError),
}

impl_result!(SvgXmlNode, SvgParseError, ResultSvgXmlNodeSvgParseError, copy = false, [Debug, Clone]);
impl_result!(Svg, SvgParseError, ResultSvgSvgParseError, copy = false, [Debug, PartialEq, PartialOrd, Clone]);

impl From<usvg::Error> for SvgParseError {
    fn from(e: usvg::Error) -> SvgParseError {
        match e {
            usvg::Error::InvalidFileSuffix => SvgParseError::InvalidFileSuffix,
            usvg::Error::FileOpenFailed => SvgParseError::FileOpenFailed,
            usvg::Error::NotAnUtf8Str => SvgParseError::NotAnUtf8Str,
            usvg::Error::MalformedGZip => SvgParseError::MalformedGZip,
            usvg::Error::InvalidSize => SvgParseError::InvalidSize,
            usvg::Error::ParsingFailed(e) => SvgParseError::ParsingFailed(e.into()),
        }
    }
}

#[repr(C, u8)]
pub enum Indent {
    None,
    Spaces(u8),
    Tabs,
}

impl From<Indent> for usvg::XmlIndent {
    fn from(f: Indent) -> usvg::XmlIndent {
        match f {
            Indent::None => usvg::XmlIndent::None,
            Indent::Spaces(s) => usvg::XmlIndent::Spaces(s),
            Indent::Tabs => usvg::XmlIndent::Tabs,
        }
    }
}

#[derive(Default, Debug, Clone, PartialEq, PartialOrd)]
struct FontParser {
    paths: Vec<SvgPath>,
    current_path: Vec<SvgPathElement>,
    current_pos: SvgPoint,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C, u8)]
pub enum GlyphData {
    Outline(SvgMultiPolygon),
    Image(RawImage),
    Svg(Svg),
}

impl GlyphData {

    pub fn get_outline(&self) -> Option<SvgMultiPolygon> {
        match self {
            GlyphData::Outline(p) => Some(p.clone()),
            _ => None
        }
    }

    pub fn get_emoji_image(&self) -> Option<RawImage> {
        match self {
            GlyphData::Image(p) => Some(p.clone()),
            _ => None
        }
    }

    pub fn get_emoji_svg(&self) -> Option<Svg> {
        match self {
            GlyphData::Svg(p) => Some(p.clone()),
            _ => None
        }
    }
}

#[cfg(feature = "image_loading")]
fn decode_raster_glyph_image(i: owned_ttf_parser::RasterGlyphImage) -> Option<RawImage> {
    use image_crate::GenericImage;

    let decoded = image_crate::load_from_memory_with_format(i.data, image_crate::ImageFormat::Png).ok()?;
    let mut decoded = decoded.into_rgba8();
    let sub = decoded.sub_image(i.x.max(0) as u32, i.y.max(0) as u32, i.width as u32, i.height as u32).to_image();
    let sub_width = sub.width() as usize;
    let sub_height = sub.height() as usize;
    let data: Vec<u8> = sub.into_raw();

    Some(RawImage {
        width: sub_width,
        height: sub_height,
        pixels: data.into(),
        data_format: RawImageFormat::RGBA8,
    })
}

/*
#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct Glyph {
    pub glyph_id: GlyphId,
    pub kerning: f32,
    pub metrics: GlyphMetrics,
    pub data: GlyphData,
}

impl_option!(Glyph, OptionGlyph, copy = false, [Debug, Clone, PartialEq, PartialOrd]);


impl Glyph {
    pub fn render_to_image_cpu(&self, font_size: f32) -> RawImage {
        match &self.data {
            GlyphData::Path(_) => { font.rasterize_glyph(self.glyph_id, font_size) },
            GlyphData::Image(i) => { i.clone() },
            GlyphData::Svg(p) => {
                Svg::parse(p.as_ref().into(), SvgParseOptions::default())
                .to_image(SvgRenderOptions::default())
                .unwrap_or(RawImage::null_image())
            },
        }
    }
}


impl_vec!(Glyph, GlyphVec, GlyphVecDestructor);
impl_vec_debug!(Glyph, GlyphVec);
impl_vec_partialord!(Glyph, GlyphVec);
impl_vec_clone!(Glyph, GlyphVec, GlyphVecDestructor);
impl_vec_partialeq!(Glyph, GlyphVec);
*/

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct Font {
    pub bytes: U8Vec,
    pub font_index: u32,
    pub info: FontInfo,
}
/*
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub enum PlatformId {
    Unicode,
    Macintosh,
    Iso,
    Windows,
    Custom,
}

impl From<owned_ttf_parser::PlatformId> for PlatformId {
    fn from(o: owned_ttf_parser::PlatformId) -> PlatformId {
        match o {
            owned_ttf_parser::PlatformId::Unicode => PlatformId::Unicode,
            owned_ttf_parser::PlatformId::Macintosh => PlatformId::Macintosh,
            owned_ttf_parser::PlatformId::Iso => PlatformId::Iso,
            owned_ttf_parser::PlatformId::Windows => PlatformId::Windows,
            owned_ttf_parser::PlatformId::Custom => PlatformId::Custom,
        }
    }
}

impl_option!(PlatformId, OptionPlatformId, [Debug, Clone, PartialEq, PartialOrd]);

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct FontName {
    pub platform_id: OptionPlatformId,
    pub encoding_id: u16,
    pub language_id: u16,
    pub name_id: u16,
    pub name: OptionAzString,
}

impl<'a> From<owned_ttf_parser::Name<'a>> for FontName {
    fn from(n: owned_ttf_parser::Name<'a>) -> FontName {
        FontName {
            platform_id: n.platform_id().map(|pi| pi.into()).into(),
            encoding_id: n.encoding_id(),
            language_id: n.language_id(),
            name_id: n.name_id(),
            name: n.name_utf8().map(|s| s.into()).into(),
        }
    }
}

impl_vec!(FontName, FontNameVec, FontNameVecDestructor);
impl_vec_debug!(FontName, FontNameVec);
impl_vec_partialord!(FontName, FontNameVec);
impl_vec_clone!(FontName, FontNameVec, FontNameVecDestructor);
impl_vec_partialeq!(FontName, FontNameVec);

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
#[repr(C)]
pub enum TableName {
    AxisVariations,
    CharacterToGlyphIndexMapping,
    ColorBitmapData,
    ColorBitmapLocation,
    CompactFontFormat,
    CompactFontFormat2,
    FontVariations,
    GlyphData,
    GlyphDefinition,
    GlyphVariations,
    Header,
    HorizontalHeader,
    HorizontalMetrics,
    HorizontalMetricsVariations,
    IndexToLocation,
    Kerning,
    MaximumProfile,
    MetricsVariations,
    Naming,
    PostScript,
    ScalableVectorGraphics,
    StandardBitmapGraphics,
    VerticalHeader,
    VerticalMetrics,
    VerticalMetricsVariations,
    VerticalOrigin,
    WindowsMetrics,
}

impl_vec!(TableName, TableNameVec, TableNameVecDestructor);
impl_vec_debug!(TableName, TableNameVec);
impl_vec_partialord!(TableName, TableNameVec);
impl_vec_clone!(TableName, TableNameVec, TableNameVecDestructor);
impl_vec_partialeq!(TableName, TableNameVec);
impl_vec_eq!(TableName, TableNameVec);
impl_vec_ord!(TableName, TableNameVec);
impl_vec_hash!(TableName, TableNameVec);

impl From<owned_ttf_parser::TableName> for TableName {
    fn from(e: owned_ttf_parser::TableName) -> TableName {
        match e {
            owned_ttf_parser::TableName::AxisVariations => TableName::AxisVariations,
            owned_ttf_parser::TableName::CharacterToGlyphIndexMapping => TableName::CharacterToGlyphIndexMapping,
            owned_ttf_parser::TableName::ColorBitmapData => TableName::ColorBitmapData,
            owned_ttf_parser::TableName::ColorBitmapLocation => TableName::ColorBitmapLocation,
            owned_ttf_parser::TableName::CompactFontFormat => TableName::CompactFontFormat,
            owned_ttf_parser::TableName::CompactFontFormat2 => TableName::CompactFontFormat2,
            owned_ttf_parser::TableName::FontVariations => TableName::FontVariations,
            owned_ttf_parser::TableName::GlyphData => TableName::GlyphData,
            owned_ttf_parser::TableName::GlyphDefinition => TableName::GlyphDefinition,
            owned_ttf_parser::TableName::GlyphVariations => TableName::GlyphVariations,
            owned_ttf_parser::TableName::Header => TableName::Header,
            owned_ttf_parser::TableName::HorizontalHeader => TableName::HorizontalHeader,
            owned_ttf_parser::TableName::HorizontalMetrics => TableName::HorizontalMetrics,
            owned_ttf_parser::TableName::HorizontalMetricsVariations => TableName::HorizontalMetricsVariations,
            owned_ttf_parser::TableName::IndexToLocation => TableName::IndexToLocation,
            owned_ttf_parser::TableName::Kerning => TableName::Kerning,
            owned_ttf_parser::TableName::MaximumProfile => TableName::MaximumProfile,
            owned_ttf_parser::TableName::MetricsVariations => TableName::MetricsVariations,
            owned_ttf_parser::TableName::Naming => TableName::Naming,
            owned_ttf_parser::TableName::PostScript => TableName::PostScript,
            owned_ttf_parser::TableName::ScalableVectorGraphics => TableName::ScalableVectorGraphics,
            owned_ttf_parser::TableName::StandardBitmapGraphics => TableName::StandardBitmapGraphics,
            owned_ttf_parser::TableName::VerticalHeader => TableName::VerticalHeader,
            owned_ttf_parser::TableName::VerticalMetrics => TableName::VerticalMetrics,
            owned_ttf_parser::TableName::VerticalMetricsVariations => TableName::VerticalMetricsVariations,
            owned_ttf_parser::TableName::VerticalOrigin => TableName::VerticalOrigin,
            owned_ttf_parser::TableName::WindowsMetrics => TableName::WindowsMetrics,
        }
    }
}
*/
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
#[repr(C, u8)]
pub enum FontWeight {
    Thin,
    ExtraLight,
    Light,
    Normal,
    Medium,
    SemiBold,
    Bold,
    ExtraBold,
    Black,
    Other(u16),
}

impl From<owned_ttf_parser::Weight> for FontWeight {
    fn from(e: owned_ttf_parser::Weight) -> FontWeight {
        match e {
            owned_ttf_parser::Weight::Thin => FontWeight::Thin,
            owned_ttf_parser::Weight::ExtraLight => FontWeight::ExtraLight,
            owned_ttf_parser::Weight::Light => FontWeight::Light,
            owned_ttf_parser::Weight::Normal => FontWeight::Normal,
            owned_ttf_parser::Weight::Medium => FontWeight::Medium,
            owned_ttf_parser::Weight::SemiBold => FontWeight::SemiBold,
            owned_ttf_parser::Weight::Bold => FontWeight::Bold,
            owned_ttf_parser::Weight::ExtraBold => FontWeight::ExtraBold,
            owned_ttf_parser::Weight::Black => FontWeight::Black,
            owned_ttf_parser::Weight::Other(o) => FontWeight::Other(o),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
#[repr(C)]
pub struct LineMetrics {
    pub position: i16,
    pub thickness: i16,
}

impl From<owned_ttf_parser::LineMetrics> for LineMetrics {
    fn from(e: owned_ttf_parser::LineMetrics) -> LineMetrics {
        LineMetrics { position: e.position, thickness: e.thickness }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
#[repr(C)]
pub struct ScriptMetrics {
    pub x_size: i16,
    pub y_size: i16,
    pub x_offset: i16,
    pub y_offset: i16,
}

impl From<owned_ttf_parser::ScriptMetrics> for ScriptMetrics {
    fn from(e: owned_ttf_parser::ScriptMetrics) -> ScriptMetrics {
        ScriptMetrics {
            x_size: e.x_size,
            y_size: e.y_size,
            x_offset: e.x_offset,
            y_offset: e.y_offset,
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Ord, Eq, Hash)]
#[repr(C)]
pub enum FontWidth {
    UltraCondensed,
    ExtraCondensed,
    Condensed,
    SemiCondensed,
    Normal,
    SemiExpanded,
    Expanded,
    ExtraExpanded,
    UltraExpanded,
}

impl From<owned_ttf_parser::Width> for FontWidth {
    fn from(e: owned_ttf_parser::Width) -> FontWidth {
        match e {
            owned_ttf_parser::Width::UltraCondensed => FontWidth::UltraCondensed,
            owned_ttf_parser::Width::ExtraCondensed => FontWidth::ExtraCondensed,
            owned_ttf_parser::Width::Condensed => FontWidth::Condensed,
            owned_ttf_parser::Width::SemiCondensed => FontWidth::SemiCondensed,
            owned_ttf_parser::Width::Normal => FontWidth::Normal,
            owned_ttf_parser::Width::SemiExpanded => FontWidth::SemiExpanded,
            owned_ttf_parser::Width::Expanded => FontWidth::Expanded,
            owned_ttf_parser::Width::ExtraExpanded => FontWidth::ExtraExpanded,
            owned_ttf_parser::Width::UltraExpanded => FontWidth::UltraExpanded,
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct FontInfo {
    // pub available_tables: TableNameVec,
    // pub names: FontNameVec,
    // pub family_name: OptionAzString,
    // pub post_script_name: OptionAzString,
    pub is_regular: bool,
    pub is_italic: bool,
    pub is_bold: bool,
    pub is_oblique: bool,
    pub is_variable: bool,
    pub weight: FontWeight,
    pub width: FontWidth,
    pub ascender: i16,
    pub descender: i16,
    pub height: i16,
    pub line_gap: i16,
    pub vertical_ascender: OptionI16,
    pub vertical_descender: OptionI16,
    pub vertical_height: OptionI16,
    pub vertical_line_gap: OptionI16,
    pub units_per_em: OptionU16,
    pub x_height: OptionI16,
    pub underline_metrics: OptionLineMetrics,
    pub strikeout_metrics: OptionLineMetrics,
    pub subscript_metrics: OptionScriptMetrics,
    pub superscript_metrics: OptionScriptMetrics,
    pub number_of_glyphs: u16,
}

impl_option!(LineMetrics, OptionLineMetrics, [Debug, Copy, Clone, PartialEq, PartialOrd]);
impl_option!(ScriptMetrics, OptionScriptMetrics, [Debug, Copy, Clone, PartialEq, PartialOrd]);

/// Every unit in the SVG font is unscaled, meaning it's mapped from 0.0 to 1.0,
/// so that it can simply be multiplied by the font size later on.
const FAKE_GLYPH_SCALE: f32 = 1000.0;
static ALL_TABLE_NAMES: [owned_ttf_parser::TableName;27] = [
    owned_ttf_parser::TableName::AxisVariations,
    owned_ttf_parser::TableName::CharacterToGlyphIndexMapping,
    owned_ttf_parser::TableName::ColorBitmapData,
    owned_ttf_parser::TableName::ColorBitmapLocation,
    owned_ttf_parser::TableName::CompactFontFormat,
    owned_ttf_parser::TableName::CompactFontFormat2,
    owned_ttf_parser::TableName::FontVariations,
    owned_ttf_parser::TableName::GlyphData,
    owned_ttf_parser::TableName::GlyphDefinition,
    owned_ttf_parser::TableName::GlyphVariations,
    owned_ttf_parser::TableName::Header,
    owned_ttf_parser::TableName::HorizontalHeader,
    owned_ttf_parser::TableName::HorizontalMetrics,
    owned_ttf_parser::TableName::HorizontalMetricsVariations,
    owned_ttf_parser::TableName::IndexToLocation,
    owned_ttf_parser::TableName::Kerning,
    owned_ttf_parser::TableName::MaximumProfile,
    owned_ttf_parser::TableName::MetricsVariations,
    owned_ttf_parser::TableName::Naming,
    owned_ttf_parser::TableName::PostScript,
    owned_ttf_parser::TableName::ScalableVectorGraphics,
    owned_ttf_parser::TableName::StandardBitmapGraphics,
    owned_ttf_parser::TableName::VerticalHeader,
    owned_ttf_parser::TableName::VerticalMetrics,
    owned_ttf_parser::TableName::VerticalMetricsVariations,
    owned_ttf_parser::TableName::VerticalOrigin,
    owned_ttf_parser::TableName::WindowsMetrics,
];

impl Font {

    /// Parse all paths from a font using rusttype
    pub fn parse(font_bytes: U8Vec, font_index: u32) -> Option<Font> {
        let f = TTFFont::from_data(font_bytes.as_ref(), font_index)?;

        let info = FontInfo {
            // available_tables: ALL_TABLE_NAMES.iter().filter(|tn| f.has_table(**tn)).copied().map(Into::into).collect::<Vec<_>>().into(),
            // names: f.names().map(Into::into).collect::<Vec<_>>().into(),
            // family_name: f.family_name().map(Into::into).into(),
            // post_script_name: f.post_script_name().map(Into::into).into(),
            is_regular: f.is_regular(),
            is_italic: f.is_italic(),
            is_bold: f.is_bold(),
            is_oblique: f.is_oblique(),
            is_variable: f.is_variable(),
            weight: f.weight().into(),
            width: f.width().into(),
            ascender: f.ascender(),
            descender: f.descender(),
            height: f.height(),
            line_gap: f.line_gap(),
            vertical_ascender: f.vertical_ascender().into(),
            vertical_descender: f.vertical_descender().into(),
            vertical_height: f.vertical_height().into(),
            vertical_line_gap: f.vertical_line_gap().into(),
            units_per_em: f.units_per_em().into(),
            x_height: f.x_height().into(),
            underline_metrics: f.underline_metrics().map(Into::into).into(),
            strikeout_metrics: f.strikeout_metrics().map(Into::into).into(),
            subscript_metrics: f.subscript_metrics().map(Into::into).into(),
            superscript_metrics: f.superscript_metrics().map(Into::into).into(),
            number_of_glyphs: f.number_of_glyphs(),
        };

        Some(Font { bytes: font_bytes, font_index, info }).into()
    }

    pub fn get_glyph_index(&self, c: char) -> Option<GlyphId> {
        let f = TTFFont::from_data(self.bytes.as_ref(), self.font_index)?;
        f.glyph_index(c).map(|i| i.0)
    }

    pub fn get_glyph_variation_index(&self, c: char, variation_char: char) -> Option<GlyphId> {
        let f = TTFFont::from_data(self.bytes.as_ref(), self.font_index)?;
        f.glyph_variation_index(c, variation_char).map(|i| i.0)
    }

    /*
    pub fn get_glyph(&self, gid: GlyphId, previous_glyph_id: Option<GlyphId>) -> Option<Glyph> {

        fn get_glyph_metrics(glyph: &rusttype::Glyph) -> GlyphMetrics {
            let metrics = glyph.clone().scaled(FAKE_GLYPH_SCALE_RUSTTYPE).h_metrics();
            GlyphMetrics {
                advance_width: metrics.advance_width / FAKE_GLYPH_SCALE,
                left_side_bearing: metrics.left_side_bearing / FAKE_GLYPH_SCALE
            }
        }

        fn get_glyph_data(glyph: &rusttype::Glyph, fb: &TTFFont, gid: owned_ttf_parser::GlyphId) -> Option<GlyphData> {

            // const PIXELS_PER_EM: u16 = 96;
            //
            // if let Some(svg_data) = fb.glyph_svg_image(gid) {
            //     Some(GlyphData::Svg(Svg::parse(svg_data, SvgParseOptions::default()).ok()?))
            // } else if let Some(image_data) = fb.glyph_raster_image(gid, PIXELS_PER_EM) {
            //     Some(GlyphData::Image(decode_raster_glyph_image(image_data).unwrap_or(RawImage::null_image())))
            // } else {
            //     Some(GlyphData::Outline(get_glyph_outline(glyph)?))
            // }

            None
        }

        let f = rusttype::Font::try_from_bytes_and_index(self.bytes.as_ref(), self.font_index)?;
        let g = f.glyph(rusttype::GlyphId(gid));
        let glyph_metrics = get_glyph_metrics(&g);
        let kerning = previous_glyph_id.map(|pgid| f.pair_kerning(FAKE_GLYPH_SCALE_RUSTTYPE, rusttype::GlyphId(pgid), rusttype::GlyphId(gid))).unwrap_or_default() / FAKE_GLYPH_SCALE;

        let fb = TTFFont::from_data(self.bytes.as_ref(), self.font_index)?;
        let glyph_data = get_glyph_data(&g, &fb, owned_ttf_parser::GlyphId(gid))?;

        Some(Glyph {
            glyph_id: gid,
            kerning,
            metrics: glyph_metrics,
            data: glyph_data,
        })
    }
    */

    pub fn get_glyph_name(&self, gid: GlyphId) -> Option<String> {
        let f = TTFFont::from_data(self.bytes.as_ref(), self.font_index)?;
        f.glyph_name(owned_ttf_parser::GlyphId(gid)).map(|s| s.to_string())
    }
}
