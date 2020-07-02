//! SVG rendering module

#[cfg(feature = "opengl")]
use crate::gl::{
    GlShader, VertexLayout, GlContextPtr,
    VertexLayoutDescription, VertexAttribute,
    VertexAttributeType, GlApiVersion
};

static mut SVG_SHADER: Option<SvgShader> = None;

const GL_RESTART_INDEX: u32 = ::std::u32::MAX;
const SHADER_VERSION_GL: &str = "#version 150";
const SHADER_VERSION_GLES: &str = "#version 300 es";
const DEFAULT_GLYPH_TOLERANCE: f32 = 0.01;

const SVG_VERTEX_SHADER: &str = "

    precision highp float;

    #define attribute in
    #define varying out

    in vec2 vAttrXY;
    out vec4 vPosition;
    uniform vec2 vBboxSize;

    void main() {
        vPosition = vec4(vAttrXY / vBboxSize - vec2(1.0), 1.0, 1.0);
    }
";

const SVG_FRAGMENT_SHADER: &str = "

    precision highp float;

    #define attribute in
    #define varying out

    in vec4 vPosition;
    out vec4 fOutColor;

    void main() {
        fOutColor = fFillColor;
    }
";

#[cfg(feature = "opengl")]
fn prefix_gl_version(shader: &str, gl: GlApiVersion) -> String {
    match gl {
        GlApiVersion::Gl { .. } => format!("{}\n{}", SHADER_VERSION_GL, shader),
        GlApiVersion::GlEs { .. } => format!("{}\n{}", SHADER_VERSION_GLES, shader),
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgLine {
    pub start: SvgPoint,
    pub end: SvgPoint,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgQuadraticCurve {
    pub start: SvgPoint,
    pub ctrl: SvgPoint,
    pub end: SvgPoint,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgCubicCurve {
    pub start: SvgPoint,
    pub ctrl_1: SvgPoint,
    pub ctrl_2: SvgPoint,
    pub end: SvgPoint,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C, u8)]
pub enum SvgPathElement {
    Line(SvgLine),
    QuadraticCurve(SvgQuadraticCurve),
    CubicCurve(SvgCubicCurve),
}

impl_vec!(SvgPathElement, SvgPathElementVec);
impl_vec_debug!(SvgPathElement, SvgPathElementVec);
impl_vec_clone!(SvgPathElement, SvgPathElementVec);
impl_vec_partialeq!(SvgPathElement, SvgPathElementVec);
impl_vec_partialord!(SvgPathElement, SvgPathElementVec);

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgPath {
    pub items: SvgPathElementVec,
}

/// One `SvgNode` corresponds to one SVG `<path></path>` element
#[derive(Debug, Clone, PartialOrd, PartialEq)]
#[repr(C, u8)]
pub enum SvgNode {
    Polygon(SvgPath),
    Circle(SvgCircle),
    Rect(SvgRect),
}

#[derive(Debug, Clone, PartialOrd, PartialEq)]
#[repr(C)]
pub struct SvgStyledNode {
    pub geometry: SvgNode,
    pub style: SvgStyle,
}

#[derive(Debug, Copy, Clone, PartialOrd, PartialEq)]
#[repr(C)]
pub struct SvgVertex {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Copy, Clone, PartialOrd, PartialEq)]
#[repr(C)]
pub struct SvgCircle {
    pub center_x: f32,
    pub center_y: f32,
    pub radius: f32,
}

impl SvgCircle {
    pub fn contains_point(&self, x: f32, y: f32) -> bool {
        let x_diff = (x - self.center_x).abs();
        let y_diff = (y - self.center_y).abs();
        (x_diff * x_diff) + (y_diff * y_diff) < (self.radius * self.radius)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgRect {
    pub width: f32,
    pub height: f32,
    pub x: f32,
    pub y: f32,
    pub rx: f32,
    pub ry: f32,
}

impl SvgRect {
    /// Note: does not incorporate rounded edges!
    /// Origin of x and y is assumed to be the top left corner
    pub fn contains_point(&self, x: f32, y: f32) -> bool {
        x > self.x &&
        x < self.x + self.width &&
        y > self.y &&
        y < self.y + self.height
    }
}

#[cfg(feature = "opengl")]
impl VertexLayoutDescription for SvgVertex {
    fn get_description() -> VertexLayout {
        VertexLayout {
            fields: vec![
                VertexAttribute {
                    name: String::from("vAttrXY").into(),
                    layout_location: crate::gl::OptionUsize::None,
                    attribute_type: VertexAttributeType::Float,
                    item_count: 2,
                },
            ]
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct TesselatedCPUSvgNode {
    pub vertices: SvgVertexVec,
    pub indices: U32Vec,
}

impl_vec!(SvgVertex, SvgVertexVec);
impl_vec_debug!(SvgVertex, SvgVertexVec);
impl_vec_partialord!(SvgVertex, SvgVertexVec);
impl_vec_clone!(SvgVertex, SvgVertexVec);
impl_vec_partialeq!(SvgVertex, SvgVertexVec);

impl_vec!(u32, U32Vec);
impl_vec_debug!(u32, U32Vec);
impl_vec_partialord!(u32, U32Vec);
impl_vec_ord!(u32, U32Vec);
impl_vec_clone!(u32, U32Vec);
impl_vec_partialeq!(u32, U32Vec);
impl_vec_eq!(u32, U32Vec);
impl_vec_hash!(u32, U32Vec);

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct TesselatedGPUSvgNode {
    pub vertex_buffer_id: u32,
    pub index_buffer_id: i32,
    pub index_buffer_gl_type: IndexBufferType,
    pub index_buffer_len: i32,
    pub gl_context: GlContextPtr,
}

#[cfg(feature = "opengl")]
#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[repr(C)]
pub struct SvgShader {
    pub program: GlShader,
}

#[cfg(feature = "opengl")]
impl SvgShader {
    pub fn new(gl_context: GlContextPtr) -> Self {

        let current_gl_api = GlApiVersion::get(&gl_context);
        let vertex_source_prefixed = prefix_gl_version(SVG_VERTEX_SHADER, current_gl_api);
        let fragment_source_prefixed = prefix_gl_version(SVG_FRAGMENT_SHADER, current_gl_api);

        Self {
            program: GlShader::new(gl_context, &vertex_source_prefixed, &fragment_source_prefixed).unwrap(),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C, u8)]
pub enum SvgStyle {
    Fill(SvgFillStyle),
    Stroke(SvgStrokeStyle),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct SvgFillStyle {
    /// See the SVG specification.
    ///
    /// Default value: `LineJoin::Miter`.
    pub line_join: SvgLineJoin,

    /// See the SVG specification.
    ///
    /// Must be greater than or equal to 1.0.
    /// Default value: `StrokeOptions::DEFAULT_MITER_LIMIT`.
    miter_limit: usize,

    /// Maximum allowed distance to the path when building an approximation.
    ///
    /// See [Flattening and tolerance](index.html#flattening-and-tolerance).
    /// Default value: `StrokeOptions::DEFAULT_TOLERANCE`.
    tolerance: usize,
}

// similar to lyon::SvgStrokeOptions, except the
// thickness is a usize (f32 * 1000 as usize), in order
// to implement Hash
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct SvgStrokeStyle {
    /// What cap to use at the start of each sub-path.
    ///
    /// Default value: `LineCap::Butt`.
    pub start_cap: SvgLineCap,

    /// What cap to use at the end of each sub-path.
    ///
    /// Default value: `LineCap::Butt`.
    pub end_cap: SvgLineCap,

    /// See the SVG specification.
    ///
    /// Default value: `LineJoin::Miter`.
    pub line_join: SvgLineJoin,

    /// Dash pattern
    pub dash_pattern: OptionSvgDashPattern,

    /// Line width
    ///
    /// Default value: `StrokeOptions::DEFAULT_LINE_WIDTH`.
    line_width: usize,

    /// See the SVG specification.
    ///
    /// Must be greater than or equal to 1.0.
    /// Default value: `StrokeOptions::DEFAULT_MITER_LIMIT`.
    miter_limit: usize,

    /// Maximum allowed distance to the path when building an approximation.
    ///
    /// See [Flattening and tolerance](index.html#flattening-and-tolerance).
    /// Default value: `StrokeOptions::DEFAULT_TOLERANCE`.
    tolerance: usize,

    /// Apply line width
    ///
    /// When set to false, the generated vertices will all be positioned in the centre
    /// of the line. The width can be applied later on (eg in a vertex shader) by adding
    /// the vertex normal multiplied by the line with to each vertex position.
    ///
    /// Default value: `true`.
    pub apply_line_width: bool,
}

const SVG_LINE_PRECISION: f32 = 1000.0;

impl SvgStrokeStyle {
    /// NOTE: Getters and setters are necessary here, because the line width, miter limit, etc.
    /// are all normalized to fit into a usize
    pub fn with_line_width(mut self, line_width: f32) -> Self { self.set_line_width(line_width); self }
    pub fn set_line_width(&mut self, line_width: f32) { self.line_width = (line_width * SVG_LINE_PRECISION) as usize; }
    pub fn get_line_width(&self) -> f32 { self.line_width as f32 / SVG_LINE_PRECISION }
    pub fn with_miter_limit(mut self, miter_limit: f32) -> Self { self.set_miter_limit(miter_limit); self }
    pub fn set_miter_limit(&mut self, miter_limit: f32) { self.miter_limit = (miter_limit * SVG_LINE_PRECISION) as usize; }
    pub fn get_miter_limit(&self) -> f32 { self.miter_limit as f32 / SVG_LINE_PRECISION }
    pub fn with_tolerance(mut self, tolerance: f32) -> Self { self.set_tolerance(tolerance); self }
    pub fn set_tolerance(&mut self, tolerance: f32) { self.tolerance = (tolerance * SVG_LINE_PRECISION) as usize; }
    pub fn get_tolerance(&self) -> f32 { self.tolerance as f32 / SVG_LINE_PRECISION }
}

#[cfg(feature = "svg")]
impl Into<lyon::tesselation::StrokeOptions> for SvgStrokeStyle {
    fn into(self) -> lyon::tesselation::StrokeOptions {
        let target = lyon::tesselation::StrokeOptions::default()
            .with_tolerance(self.get_tolerance())
            .with_start_cap(self.start_cap.into())
            .with_end_cap(self.end_cap.into())
            .with_line_join(self.line_join.into())
            .with_line_width(self.get_line_width())
            .with_miter_limit(self.get_miter_limit());

        if !self.apply_line_width {
            target.dont_apply_line_width()
        } else {
            target
        }
    }
}

impl Default for SvgStrokeStyle {
    fn default() -> Self {
        const DEFAULT_MITER_LIMIT: f32 = 4.0;
        const DEFAULT_LINE_WIDTH: f32 = 1.0;
        const DEFAULT_TOLERANCE: f32 = 0.1;

        Self {
            start_cap: SvgLineCap::default(),
            end_cap: SvgLineCap::default(),
            line_join: SvgLineJoin::default(),
            dash_pattern: OptionSvgDashPattern::None,
            line_width: (DEFAULT_LINE_WIDTH * SVG_LINE_PRECISION) as usize,
            miter_limit: (DEFAULT_MITER_LIMIT * SVG_LINE_PRECISION) as usize,
            tolerance: (DEFAULT_TOLERANCE * SVG_LINE_PRECISION) as usize,
            apply_line_width: true,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Hash, Eq, PartialOrd, Ord)]
#[repr(C)]
pub struct SvgDashPattern {
    pub offset: usize,
    pub length_1: usize,
    pub gap_1: usize,
    pub length_2: usize,
    pub gap_2: usize,
    pub length_3: usize,
    pub gap_3: usize,
}

impl SvgDashPattern {
    #[inline] pub fn with_offset(self, value: f32) -> Self { Self { offset: ((value * SVG_LINE_PRECISION) as usize), .. self } }
    #[inline] pub fn set_offset(&mut self, value: f32) { self.offset = (value * SVG_LINE_PRECISION) as usize; }
    #[inline] pub fn get_offset(&self) -> f32 { self.offset as f32 / SVG_LINE_PRECISION }
    #[inline] pub fn with_length_1(self, value: f32) -> Self { Self { length_1: ((value * SVG_LINE_PRECISION) as usize), .. self } }
    #[inline] pub fn set_length_1(&mut self, value: f32) { self.length_1 = (value * SVG_LINE_PRECISION) as usize; }
    #[inline] pub fn get_length_1(&self) -> f32 { self.length_1 as f32 / SVG_LINE_PRECISION }
    #[inline] pub fn with_gap_1(self, value: f32) -> Self { Self { gap_1: ((value * SVG_LINE_PRECISION) as usize), .. self } }
    #[inline] pub fn set_gap_1(&mut self, value: f32) { self.gap_1 = (value * SVG_LINE_PRECISION) as usize; }
    #[inline] pub fn get_gap_1(&self) -> f32 { self.gap_1 as f32 / SVG_LINE_PRECISION }
    #[inline] pub fn with_length_2(self, value: f32) -> Self { Self { length_2: ((value * SVG_LINE_PRECISION) as usize), .. self } }
    #[inline] pub fn set_length_2(&mut self, value: f32) { self.length_2 = (value * SVG_LINE_PRECISION) as usize; }
    #[inline] pub fn get_length_2(&self) -> f32 { self.length_2 as f32 / SVG_LINE_PRECISION }
    #[inline] pub fn with_gap_2(self, value: f32) -> Self { Self { gap_2: ((value * SVG_LINE_PRECISION) as usize), .. self } }
    #[inline] pub fn set_gap_2(&mut self, value: f32) { self.gap_2 = (value * SVG_LINE_PRECISION) as usize; }
    #[inline] pub fn get_gap_2(&self) -> f32 { self.gap_2 as f32 / SVG_LINE_PRECISION }
    #[inline] pub fn with_length_3(self, value: f32) -> Self { Self { length_3: ((value * SVG_LINE_PRECISION) as usize), .. self } }
    #[inline] pub fn set_length_3(&mut self, value: f32) { self.length_3 = (value * SVG_LINE_PRECISION) as usize; }
    #[inline] pub fn get_length_3(&self) -> f32 { self.length_3 as f32 / SVG_LINE_PRECISION }
    #[inline] pub fn with_gap_3(self, value: f32) -> Self { Self { gap_3: ((value * SVG_LINE_PRECISION) as usize), .. self } }
    #[inline] pub fn set_gap_3(&mut self, value: f32) { self.gap_3 = (value * SVG_LINE_PRECISION) as usize; }
    #[inline] pub fn get_gap_3(&self) -> f32 { self.gap_3 as f32 / SVG_LINE_PRECISION }
}

impl_option!(SvgDashPattern, OptionSvgDashPattern, [Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq, Hash]);

#[derive(Debug, Copy, Clone, PartialEq, Hash, Eq, PartialOrd, Ord)]
#[repr(C)]
pub enum SvgLineCap {
    Butt,
    Square,
    Round,
}

impl Default for SvgLineCap {
    fn default() -> Self {
        SvgLineCap::Butt
    }
}

#[cfg(feature = "svg")]
impl Into<lyon::tesselation::LineCap> for SvgLineCap {
    #[inline]
    fn into(self) -> LineCap {
        use self::SvgLineCap::*;
        match self {
            Butt => lyon::tesselation::LineCap::Butt,
            Square => lyon::tesselation::LineCap::Square,
            Round => lyon::tesselation::LineCap::Round,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Hash, Eq, PartialOrd, Ord)]
#[repr(C)]
pub enum SvgLineJoin {
    Miter,
    MiterClip,
    Round,
    Bevel,
}

impl Default for SvgLineJoin {
    fn default() -> Self {
        SvgLineJoin::Miter
    }
}

#[cfg(feature = "svg")]
impl Into<lyon::tesselation::LineJoin> for SvgLineJoin {
    #[inline]
    fn into(self) -> LineJoin {
        use self::SvgLineJoin::*;
        match self {
            Miter => lyon::tesselation::LineJoin::Miter,
            MiterClip => lyon::tesselation::LineJoin::MiterClip,
            Round => lyon::tesselation::LineJoin::Round,
            Bevel => lyon::tesselation::LineJoin::Bevel,
        }
    }
}


#[cfg(feature = "svg")] {

    use lyon::{
        tessellation::{
            FillOptions, BuffersBuilder, FillVertex, FillTessellator,
            LineCap, LineJoin, StrokeTessellator, StrokeOptions, StrokeVertex,
            basic_shapes::{
                fill_circle, stroke_circle, fill_rounded_rectangle,
                stroke_rounded_rectangle, BorderRadii
            },
        },
        path::{
            default::{Builder, Path},
            builder::{PathBuilder, FlatPathBuilder},
        },
        geom::euclid::{TypedRect, TypedPoint2D, TypedSize2D, TypedVector2D, UnknownUnit},
    };

    fn svg_path_to_lyon_path_events(path: &SvgPath) -> Vec<PathEvent> {

        let v;

        if path.items.as_ref().is_empty() {
            return Vec::new();
        } else {
            let start_item = path.items.as_ref()[0];
            v = vec![PathEvent::MoveTo(Point::new(start_item.get_start().x as f32, -(start_item.get_start().y as f32)))];
        }

        v.extend(path.items.as_ref().par_iter().map(|p| match p {
            SvgPathElement::Line(l) => PathEvent::LineTo(Point::new(l.end.x as f32, -(l.end.y as f32))),
            SvgPathElement::QuadraticCurve(qc) => PathEvent::QuadraticTo(
                Point::new(qc.ctrl.x as f32, -(qc.ctrl.y as f32)),
                Point::new(qc.end.x as f32, -(qc.end.y as f32)),
            ),
            SvgPathElement::CubicCurve(cc) => PathEvent::CubicTo(
                Point::new(cc.ctrl_1.x as f32, -(cc.ctrl_1.y as f32)),
                Point::new(cc.ctrl_2.x as f32, -(cc.ctrl_2.y as f32)),
                Point::new(cc.end.x as f32, -(cc.end.y as f32)),
            ),
        });

        if path.is_closed() {
            v.push(PathEvent::Close);
        }

        v
    }

    #[inline]
    fn vertex_buffers_to_tesselated_cpu_node(v: VertexBuffers<SvgVertex, u32>) -> TesselatedCPUSvgNode {
        TesselatedCPUSvgNode {
            vertices: v.vertices.into(),
            indices: v.indices.into(),
        }
    }

    pub fn tesselate_path_fill(path: &SvgPath, fill_style: &SvgFillStyle) -> TesselatedCPUSvgNode {

        let mut geometry = VertexBuffers::new();
        let polygon = svg_path_to_lyon_path_events(path);
        let path = polygon.as_ref().unwrap();
        let mut tessellator = FillTessellator::new();

        tessellator.tessellate_path(
            path.path_iter(),
            &FillOptions::default(),
            &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex| {
                let xy_arr = vertex.position.to_array();
                SvgVertex { x: xy_arr[0], y: xy_arr[1] }
            }).with_tolerance(fill_style.get_tolerance()),
        ).unwrap();

        vertex_buffers_to_tesselated_cpu_node(geometry)
    }

    pub fn tesselate_path_stroke(path: &SvgPath, stroke_style: &SvgStrokeStyle) -> TesselatedCPUSvgNode {
        let mut stroke_geometry = VertexBuffers::new();
        let stroke_options: StrokeOptions = stroke.into();
        let polygon = svg_path_to_lyon_path_events(path);
        let path = polygon.as_ref().unwrap();

        let mut stroke_tess = StrokeTessellator::new();
        stroke_tess.tessellate_path(
            path.path_iter(),
            &stroke_options,
            &mut BuffersBuilder::new(&mut stroke_geometry, |vertex: StrokeVertex| {
                let xy_arr = vertex.position.to_array();
                SvgVertex { x: xy_arr[0], y: xy_arr[1] }
            }).with_tolerance(fill_style.get_tolerance()),
        );

        vertex_buffers_to_tesselated_cpu_node(stroke_geometry)
    }

    pub fn tesselate_circle_fill(c: &SvgCircle, fill_style: &SvgFillStyle) -> TesselatedCPUSvgNode {
        let mut geometry = VertexBuffers::new();
        let center = TypedPoint2D::new(c.center_x, c.center_y);
        fill_circle(center, c.radius, &FillOptions::default(),
            &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex| {
                let xy_arr = vertex.position.to_array();
                SvgVertex { x: xy_arr[0], y: xy_arr[1] }
            }
        ));
        vertex_buffers_to_tesselated_cpu_node(geometry)
    }

    pub fn tesselate_circle_stroke(c: &SvgCircle, stroke_style: &SvgStrokeStyle) -> TesselatedCPUSvgNode {
        let center = TypedPoint2D::new(c.center_x, c.center_y);
        let mut stroke_geometry = VertexBuffers::new();
        let stroke_options: StrokeOptions = stroke.into();
        let stroke_options = stroke_options.with_tolerance(tolerance);
        stroke_circle(center, c.radius, &stroke_options,
            &mut BuffersBuilder::new(&mut stroke_geometry, |vertex: StrokeVertex| {
                let xy_arr = vertex.position.to_array();
                SvgVertex { x: xy_arr[0], y: xy_arr[1] }
            }
        ));
        vertex_buffers_to_tesselated_cpu_node(stroke_geometry)
    }

    fn get_radii(r: &SvgRect) -> (TypedRect<f32, UnknownUnit>, BorderRadii) {
        let rect = TypedRect::new(TypedPoint2D::new(r.x, r.y), TypedSize2D::new(r.width, r.height));
        let radii = BorderRadii { top_left: r.rx, top_right: r.rx, bottom_left: r.rx, bottom_right: r.rx };
        (rect, radii)
    }

    pub fn tesselate_rect_fill(r: &SvgRect, fill_style: &SvgFillStyle) -> TesselatedCPUSvgNode {
        let mut geometry = VertexBuffers::new();
        let (rect, radii) = get_radii(&r);
        fill_rounded_rectangle(&rect, &radii, &FillOptions::default(),
            &mut BuffersBuilder::new(&mut geometry, |vertex: FillVertex| {
                let xy_arr = vertex.position.to_array();
                SvgVertex { x: xy_arr[0], y: xy_arr[1] }
            }
        ));
        vertex_buffers_to_tesselated_cpu_node(geometry)
    }

    pub fn tesselate_rect_stroke(r: &SvgRect, fill_style: &SvgStrokeStyle) -> TesselatedCPUSvgNode {
        let mut stroke_geometry = VertexBuffers::new();
        let stroke_options: StrokeOptions = stroke.into();
        let stroke_options = stroke_options.with_tolerance(tolerance);
        let (rect, radii) = get_radii(&r);
        stroke_rounded_rectangle(&rect, &radii, &stroke_options,
            &mut BuffersBuilder::new(&mut stroke_geometry, |vertex: StrokeVertex| {
                SvgVert {
                    xy: vertex.position.to_array(),
                }
            }
        ));
        vertex_buffers_to_tesselated_cpu_node(stroke_geometry)
    }

    /// Tesselate the path using lyon
    pub fn tesselate_path(node: &SvgStyledNode) -> TesselatedCPUSvgNode {
        match node.style {
            Fill(fs) => {
                match node.geometry {
                    SvgNode::Polygon(p) => tesselate_path_fill(p, fs),
                    SvgNode::Circle(c) => tesselate_circle_fill(c, fs),
                    SvgNode::Rect(r) => tesselate_rect_fill(r, fs),
                }
            },
            Stroke(ss) => {
                match node.geometry {
                    SvgNode::Polygon(p) => tesselate_path_stroke(p, ss),
                    SvgNode::Circle(c) => tesselate_circle_stroke(c, ss),
                    SvgNode::Rect(r) => tesselate_rect_stroke(r, ss),
                }
            }
        }
    }

    /// Parse an XML string using xmlparser
    pub fn parse_xml_string(string: &str) -> Xml {

    }

    /// Build a SVG model from the parsed XML using usvg
    pub fn get_simplified_svg(xml: &Xml) -> Svg {

    }

    /// Parse all paths from a font using rusttype
    pub fn parse_font(font_bytes: &[u8]) -> Vec<(GlyphId, SvgPath)> {

    }
}

#[cfg(feature = opengl)] {
    /// NOTE: may not be called from more than 1 thread
    pub fn get_svg_shader(gl_context: &GlContextPtr) -> Option<&SvgShadervgShader> {
        if let Some(s) = SVG_SHADER {
            return Some(s);
        } else {
            let svg_shader = match SvgShader::new(gl_context) {
                Ok(o) => o,
                Err(e) => {
                    #[cfg(feature = "logging")] { error!("could not compile SVG shader: {}", e); }
                    return None;
                },
            };
            unsafe { SVG_SHADER = Some(svg_shader) };
            return SVG_SHADER.as_ref();
        }
    }

    pub fn upload_tesselated_path_to_gpu(cpu_data: &TesselatedCPUSvgNode) -> TesselatedGPUSvgNode {
        let buf = VertexBuffer::new();
        let buf = VertexBuffer::new(&shader, &f.vertices, &f.indices, IndexBufferFormat::Triangles);
    }

    pub fn draw_gpu_buf_to_texture(gl_context: &GlContextPtr, buf: TesselatedGPUSvgNode, texture_size: PhysicalSize) -> Option<Texture> {
        let shader = get_svg_shader(gl_context)?;
        let shader = &shader.program;

        gl_context.enable(gl::PRIMITIVE_RESTART_FIXED_INDEX);
        let texture = shader.draw(&[(buf, build_uniforms(texture_size))], Some(ColorU::TRANSPARENT), texture_size);
        gl_context.disable(gl::PRIMITIVE_RESTART_FIXED_INDEX);
    }
}

// ---------------------------------------------- not yet ported

/*
#[cfg(feature = "svg")]
mod svg_internal {

    #[derive(Debug, Copy, Clone, PartialOrd, Ord, PartialEq, Eq, Hash)]
    struct SvgWorldPixel;

    pub type GlyphId = u32;

    /// A vectorized font holds the glyphs for a given font, but in a vector format
    #[derive(Debug, Clone, Default)]
    pub struct VectorizedFont {
        /// Glyph -> Polygon map
        glyph_polygon_map: RefCell<FastHashMap<GlyphId, VertexBuffers<SvgVert, u32>>>,
        /// Glyph -> Stroke map
        glyph_stroke_map: RefCell<FastHashMap<GlyphId, VertexBuffers<SvgVert, u32>>>,
        /// Original font bytes
        font_bytes: U8Vec,
        /// Index of the font in the Vec<u8>, usually 0
        font_index: i32,
    }

    use stb_truetype::{FontInfo, Vertex};

    impl VectorizedFont {

        /// Prepares a vectorized font from a set of bytes
        pub fn from_bytes(font_bytes: FontBytes, font_index: FontIndex) -> Self {
            Self {
                font_bytes,
                font_index,
                .. Default::default()
            }
        }

        pub fn get_fill_vertices(&self, glyphs: &[GlyphInstance]) -> Vec<VertexBuffers<SvgVert, u32>> {

            let font_info = match FontInfo::new(self.font_bytes.clone(), 0) {
                Some(s) => s,
                None => return Vec::new(),
            };

            let mut borrow_mut = self.glyph_polygon_map.borrow_mut();
            glyphs.iter().filter_map(|glyph| {
                match borrow_mut.entry(glyph.index) {
                    Occupied(o) => Some(o.get().clone()),
                    Vacant(v) => {
                        let glyph_shape = font_info.get_glyph_shape(glyph.index)?;
                        let poly = glyph_to_svg_layer_type(glyph_shape);
                        let mut path = None;
                        let polygon_verts = poly.tesselate_fill(DEFAULT_GLYPH_TOLERANCE, &mut path);
                        v.insert(polygon_verts.clone());
                        Some(polygon_verts)
                    }
                }
            }).collect()
        }

        pub fn get_stroke_vertices(&self, glyphs: &[GlyphInstance], stroke_options: &SvgStrokeOptions) -> Vec<VertexBuffers<SvgVert, u32>> {

            let font_info = match FontInfo::new(self.font_bytes.clone(), 0) {
                Some(s) => s,
                None => return Vec::new(),
            };

            let mut borrow_mut = self.glyph_stroke_map.borrow_mut();
            glyphs.iter().filter_map(|glyph| {
                match borrow_mut.entry(glyph.index) {
                    Occupied(o) => Some(o.get().clone()),
                    Vacant(v) => {
                        let glyph_shape = font_info.get_glyph_shape(glyph.index)?;
                        let poly = glyph_to_svg_layer_type(glyph_shape);
                        let mut path = None;
                        let stroke_verts = poly.tesselate_stroke(DEFAULT_GLYPH_TOLERANCE, &mut path, *stroke_options);
                        v.insert(stroke_verts.clone());
                        Some(stroke_verts)
                    }
                }
            }).collect()
        }

        pub fn get_font_bytes(&self) -> (&FontBytes, FontIndex) {
            (&self.font_bytes, self.font_index)
        }
    }

    /// Converts a `Vec<stb_truetype::Vertex>` to a `SvgLayerType::Polygon`
    fn glyph_to_svg_layer_type(vertices: Vec<Vertex>) -> SvgLayerType {
        SvgLayerType::Polygon(vertices.into_iter().map(rusttype_glyph_to_path_events).collect())
    }

    // Convert a Rusttype glyph to a Vec of PathEvents,
    // in order to turn a glyph into a polygon
    fn rusttype_glyph_to_path_events(vertex: Vertex) -> PathEvent {

        use stb_truetype::VertexType;

        // Rusttypes vertex type needs to be inverted in the Y axis
        // in order to work with lyon correctly
        match vertex.vertex_type() {
            VertexType::CurveTo =>  PathEvent::QuadraticTo(
                                        Point::new(vertex.cx as f32, -(vertex.cy as f32)),
                                        Point::new(vertex.x as f32,  -(vertex.y as f32))
                                    ),
            VertexType::MoveTo =>   PathEvent::MoveTo(Point::new(vertex.x as f32, -(vertex.y as f32))),
            VertexType::LineTo =>   PathEvent::LineTo(Point::new(vertex.x as f32, -(vertex.y as f32))),
        }
    }

    #[derive(Debug, Clone, Default)]
    pub struct VectorizedFontCache {
        /// Font -> Vectorized glyph map
        ///
        /// Needs to be wrapped in a RefCell / Rc since we want to lazy-load the
        /// fonts to keep the memory usage down
        vectorized_fonts: RefCell<FastHashMap<VectorizedFontId, Rc<VectorizedFont>>>,
    }

    impl VectorizedFontCache {

        pub fn new() -> Self {
            Self::default()
        }

        pub fn clear(&mut self) {
            self.vectorized_fonts.borrow_mut().clear();
        }

        pub fn add_font(&mut self, font: VectorizedFont) -> VectorizedFontId {
            let font_id = VectorizedFontId::new();
            self.vectorized_fonts.borrow_mut().insert(font_id, Rc::new(font));
            font_id
        }

        pub fn get_font(&self, id: &VectorizedFontId) -> Option<Rc<VectorizedFont>> {
            self.vectorized_fonts.borrow().get(&id).map(|font| font.clone())
        }

        /// Returns true if the font cache has the respective font
        pub fn has_font(&self, id: &VectorizedFontId) -> bool {
            self.vectorized_fonts.borrow().get(id).is_some()
        }

        pub fn remove_font(&mut self, id: &VectorizedFontId) {
            self.vectorized_fonts.borrow_mut().remove(id);
        }
    }

    fn build_path_from_polygon(polygon: &[PathEvent], tolerance: f32) -> Path {
        let mut builder = Builder::with_capacity(polygon.len()).flattened(tolerance);
        for event in polygon {
            builder.path_event(*event);
        }
        builder.with_svg().build()
    }

    mod svg_to_lyon {

        use lyon::{
            math::Point,
            path::PathEvent,
        };
        use usvg::{Tree, PathSegment, Color, Options, Paint, Stroke, LineCap, LineJoin, NodeKind};
        use crate::svg::{
            SvgStrokeOptions, SvgLineCap, SvgLineJoin,
            SvgLayerType, SvgStyle, SvgParseError
        };
        use azul_css::ColorU;

        pub fn parse_from<S: AsRef<str>>(svg_source: S) -> Result<Vec<(Vec<SvgLayerType>, SvgStyle)>, SvgParseError> {

            let opt = Options::default();
            let rtree = Tree::from_str(svg_source.as_ref(), &opt).unwrap();

            let mut layer_data = Vec::new();

            for node in rtree.root().descendants() {
                if let NodeKind::Path(p) = &*node.borrow() {
                    let mut style = SvgStyle::default();

                    if let Some(ref fill) = p.fill {

                        // fall back to always use color fill
                        // no gradients (yet?)
                        let color = match fill.paint {
                            Paint::Color(c) => c,
                            _ => FALLBACK_COLOR,
                        };

                        style.fill = Some(ColorU {
                            r: color.red,
                            g: color.green,
                            b: color.blue,
                            a: (fill.opacity.value() * 255.0) as u8
                        });
                    }

                    if let Some(ref stroke) = p.stroke {
                        style.stroke = Some(convert_stroke(stroke));
                    }

                    let layer = vec![SvgLayerType::Polygon(p.segments.iter().map(|e| as_event(e)).collect())];
                    layer_data.push((layer, style));
                }
            }

            Ok(layer_data)
        }

        // Map resvg::tree::PathSegment to lyon::path::PathEvent
        fn as_event(ps: &PathSegment) -> PathEvent {
            match *ps {
                PathSegment::MoveTo { x, y } => PathEvent::MoveTo(Point::new(x as f32, y as f32)),
                PathSegment::LineTo { x, y } => PathEvent::LineTo(Point::new(x as f32, y as f32)),
                PathSegment::CurveTo { x1, y1, x2, y2, x, y, } => {
                    PathEvent::CubicTo(
                        Point::new(x1 as f32, y1 as f32),
                        Point::new(x2 as f32, y2 as f32),
                        Point::new(x as f32, y as f32))
                }
                PathSegment::ClosePath => PathEvent::Close,
            }
        }

        pub const FALLBACK_COLOR: Color = Color {
            red: 0,
            green: 0,
            blue: 0,
        };

        // dissect a resvg::Stroke into a webrender::ColorU + SvgStrokeOptions
        pub fn convert_stroke(s: &Stroke) -> (ColorU, SvgStrokeOptions) {

            let color = match s.paint {
                Paint::Color(c) => c,
                _ => FALLBACK_COLOR,
            };
            let line_cap = match s.linecap {
                LineCap::Butt => SvgLineCap::Butt,
                LineCap::Square => SvgLineCap::Square,
                LineCap::Round => SvgLineCap::Round,
            };
            let line_join = match s.linejoin {
                LineJoin::Miter => SvgLineJoin::Miter,
                LineJoin::Bevel => SvgLineJoin::Bevel,
                LineJoin::Round => SvgLineJoin::Round,
            };

            let opts = SvgStrokeOptions {
                start_cap: line_cap,
                end_cap: line_cap,
                line_join,
                .. SvgStrokeOptions::default().with_line_width(s.width as f32)
            };

            (ColorU {
                r: color.red,
                g: color.green,
                b: color.blue,
                a: (s.opacity.value() * 255.0) as u8
            }, opts)
        }
    }

    #[derive(Debug, Clone)]
    pub struct Svg {
        /// Currently active layers
        pub layers: Vec<SvgLayerResource>,
        /// Pan (horizontal, vertical) in pixels
        pub pan: (f32, f32),
        /// 1.0 = default zoom
        pub zoom: f32,
        /// Whether an FXAA shader should be applied to the resulting OpenGL texture
        pub enable_fxaa: bool,
        /// Should the SVG add the current HiDPI factor to the zoom?
        pub enable_hidpi: bool,
        /// Background color (default: transparent)
        pub background_color: ColorU,
        /// Multisampling (default: 1.0) - since there is no anti-aliasing yet, simply
        /// increases the texture size that is drawn to.
        pub multisampling_factor: usize,
    }

    impl Default for Svg {
        fn default() -> Self {
            Self {
                layers: Vec::new(),
                pan: (0.0, 0.0),
                zoom: 1.0,
                enable_fxaa: false,
                enable_hidpi: true,
                background_color: ColorU::TRANSPARENT,
                multisampling_factor: 1,
            }
        }
    }

    #[derive(Debug, Clone)]
    pub enum SvgLayerResource {
        Reference((SvgLayerId, SvgStyle)),
        Direct(SvgLayerResourceDirect),
    }

    #[derive(Debug, Clone)]
    pub struct SvgLayerResourceDirect {
        pub style: SvgStyle,
        pub fill: Option<VerticesIndicesBuffer>,
        pub stroke: Option<VerticesIndicesBuffer>,
    }

    impl SvgLayerResourceDirect {
        pub fn tesselate_from_layer(data: &[SvgLayerType], style: SvgStyle) -> Self {
            tesselate_polygon_data(data, style)
        }
    }

    #[derive(Debug, Clone, Default)]
    pub struct VerticesIndicesBuffer {
        pub vertices: Vec<SvgVert>,
        pub indices: Vec<u32>,
    }

    #[derive(Debug, Copy, Clone)]
    pub struct BezierControlPoint {
        pub x: f32,
        pub y: f32,
    }

    impl BezierControlPoint {
        /// Distance of two points
        pub fn distance(&self, other: &Self) -> f32 {
            ((other.x - self.x).powi(2) + (other.y - self.y).powi(2)).sqrt()
        }

        #[inline(always)]
        pub const fn from_points((x, y): (f32, f32)) -> Self {
            BezierControlPoint { x, y }
        }
    }

    /// Bezier formula for cubic curves (start, handle 1, handle 2, end).
    ///
    /// ## Inputs
    ///
    /// - `curve`: The 4 handles of the curve
    /// - `t`: The interpolation amount - usually between 0.0 and 1.0 if the point
    ///   should be between the start and end
    ///
    /// ## Returns
    ///
    /// - `BezierControlPoint`: The calculated point which lies on the curve,
    ///    according the the bezier formula
    pub fn cubic_interpolate_bezier(curve: &[BezierControlPoint;4], t: f32) -> BezierControlPoint {
        let one_minus = 1.0 - t;
        let one_minus_square = one_minus.powi(2);
        let one_minus_cubic = one_minus.powi(3);

        let t_pow2 = t.powi(2);
        let t_pow3 = t.powi(3);

        let x =         one_minus_cubic  *             curve[0].x
                + 3.0 * one_minus_square * t         * curve[1].x
                + 3.0 * one_minus        * t_pow2    * curve[2].x
                +                          t_pow3    * curve[3].x;

        let y =         one_minus_cubic  *             curve[0].y
                + 3.0 * one_minus_square * t         * curve[1].y
                + 3.0 * one_minus        * t_pow2    * curve[2].y
                +                          t_pow3    * curve[3].y;

        BezierControlPoint { x, y }
    }

    pub fn quadratic_interpolate_bezier(curve: &[BezierControlPoint;3], t: f32) -> BezierControlPoint {
        let one_minus = 1.0 - t;
        let one_minus_square = one_minus.powi(2);

        let t_pow2 = t.powi(2);

        // TODO: Why 3.0 and not 2.0?

        let x =         one_minus_square *             curve[0].x
                + 2.0 * one_minus        * t         * curve[1].x
                + 3.0                    * t_pow2    * curve[2].x;

        let y =         one_minus_square *             curve[0].y
                + 2.0 * one_minus        * t         * curve[1].y
                + 3.0                    * t_pow2    * curve[2].y;

        BezierControlPoint { x, y }
    }

    #[derive(Debug, Copy, Clone)]
    pub struct BezierNormalVector {
        pub x: f32,
        pub y: f32,
    }

    impl BezierNormalVector {
        pub fn to_rotation(&self) -> BezierCharacterRotation {
            BezierCharacterRotation((-self.x).atan2(self.y))
        }
    }

    /// Calculates the normal vector at a certain point (perpendicular to the curve)
    pub fn cubic_bezier_normal(curve: &[BezierControlPoint;4], t: f32) -> BezierNormalVector {

        // 1. Calculate the derivative of the bezier curve
        //
        // This means, we go from 4 control points to 3 control points and redistribute
        // the weights of the control points according to the formula:
        //
        // w'0 = 3(w1-w0)
        // w'1 = 3(w2-w1)
        // w'2 = 3(w3-w2)

        let weight_1_x = 3.0 * (curve[1].x - curve[0].x);
        let weight_1_y = 3.0 * (curve[1].y - curve[0].y);

        let weight_2_x = 3.0 * (curve[2].x - curve[1].x);
        let weight_2_y = 3.0 * (curve[2].y - curve[1].y);

        let weight_3_x = 3.0 * (curve[3].x - curve[2].x);
        let weight_3_y = 3.0 * (curve[3].y - curve[2].y);

        // The first derivative of a cubic bezier curve is a quadratic bezier curve
        // Luckily, the first derivative is also the tangent vector. So all we need to do
        // is to get the quadratic bezier
        let mut tangent = quadratic_interpolate_bezier(&[
            BezierControlPoint { x: weight_1_x, y: weight_1_y },
            BezierControlPoint { x: weight_2_x, y: weight_2_y },
            BezierControlPoint { x: weight_3_x, y: weight_3_y },
        ], t);

        // We normalize the tangent to have a lenght of 1
        let tangent_length = (tangent.x.powi(2) + tangent.y.powi(2)).sqrt();
        tangent.x /= tangent_length;
        tangent.y /= tangent_length;

        // The tangent is the vector that runs "along" the curve at a specific point.
        // To get the normal (to calcuate the rotation of the characters), we need to
        // rotate the tangent vector by 90 degrees.
        //
        // Rotating by 90 degrees is very simple, as we only need to flip the x and y axis

        BezierNormalVector {
            x: -tangent.y,
            y: tangent.x,
        }
    }

    #[derive(Debug, Copy, Clone)]
    pub enum SvgTextPlacement {
        /// Text is simply layouted from left-to-right
        Unmodified,
        /// Text is rotated by X degrees
        Rotated(f32),
        /// Text is placed on a cubic bezier curve
        OnCubicBezierCurve(SampledBezierCurve),
    }

    #[derive(Debug, Clone)]
    pub struct SvgText {
        /// Font size of the text, in pixels
        pub font_size_px: f32,
        /// Font ID, such as FontId(0)
        pub font_id: VectorizedFontId,
        /// What are the words / glyphs in this text
        pub text_layout: SvgTextLayout,
        /// What is the font color & stroke (if any)?
        pub style: SvgStyle,
        /// Is the text rotated or on a curve?
        pub placement: SvgTextPlacement,
    }

    /// An axis-aligned bounding box (not rotated / skewed)
    #[derive(Debug, Copy, Clone)]
    pub struct SvgBbox(pub TypedRect<f32, SvgWorldPixel>);

    impl SvgBbox {

        /// Simple function for drawing a single bounding box
        pub fn draw_lines(&self, color: ColorU, line_width: f32) -> SvgLayerResourceDirect {
            quick_rects(&[SvgRect {
                width: self.0.size.width,
                height: self.0.size.height,
                x: self.0.origin.x,
                y: self.0.origin.y,
                rx: 0.0,
                ry: 0.0,
            }],
            Some(color),
            None,
            Some(SvgStrokeOptions::default().with_line_width(line_width))
        )}

        /// Checks if the bounding box contains a point
        pub fn contains_point(&self, x: f32, y: f32) -> bool {
            self.0.contains(&TypedPoint2D::new(x, y))
        }

        /// Translate the SvgBbox by x / y
        pub fn translate(&mut self, x: f32, y: f32) {
            self.0 = self.0.translate(&TypedVector2D::new(x, y));
        }
    }

    #[test]
    fn translate_bbox() {
        let mut bbox = SvgBbox(TypedRect::zero());
        bbox.translate(200.0, 300.0);
        assert_eq!(bbox.0.origin.x, 200.0);
        assert_eq!(bbox.0.origin.y, 300.0);
    }

    pub fn is_point_in_shape(point: (f32, f32), shape: &[(f32, f32)]) -> bool {
        if shape.len() < 3 {
            // Shape must at least have 3 points, i.e. be a triangle
            return false;
        }

        // We iterate over the shape in 2 points.
        //
        // If the mouse cursor (target point) is on the left side for all points,
        // then cursor is inside of the shape. If it appears on the right side for
        // only one point, we know that it isn't inside the target shape.
        // all() is lazy and will quit on the first result where the target is not
        // inside the shape.
        shape.iter().zip(shape.iter().skip(1)).all(|(start, end)| {
            !(side_of_point(point, *start, *end).is_sign_positive())
        })
    }

    /// Determine which side of a vector the point is on.
    ///
    /// Depending on if the result of this function is positive or negative,
    /// the target point lies either right or left to the imaginary line from (start -> end)
    #[inline]
    pub fn side_of_point(target: (f32, f32), start: (f32, f32), end: (f32, f32)) -> f32 {
        ((target.0 - start.0) * (end.1 - start.1)) -
        ((target.1 - start.1) * (end.0 - start.0))
    }

    /// Creates a text layout for a single string of text
    #[derive(Debug, Clone)]
    pub struct SvgTextLayout {
        /// The words, broken up by whitespace
        pub words: Words,
        /// Words, scaled by a certain font size (with font metrics)
        pub scaled_words: ScaledWords,
        /// Layout of the positions, word-by-word
        pub word_positions: WordPositions,
        /// Positioned and horizontally aligned glyphs
        pub layouted_glyphs: LayoutedGlyphs,
        /// At what glyphs does the line actually break (necessary for aligning content)
        pub inline_text_layout: InlineTextLayout,
    }

    /// Since the SvgText is scaled on the GPU, the font size doesn't matter here
    pub const SVG_FAKE_FONT_SIZE: f32 = 64.0;

    impl SvgTextLayout {

        /// Get the bounding box of a layouted text
        pub fn get_bbox(&self, placement: &SvgTextPlacement) -> SvgBbox {
            use self::SvgTextPlacement::*;

            // TODO: Scale by font size!

            let normal_width = self.word_positions.content_size.width;
            let normal_height = self.word_positions.content_size.height;

            SvgBbox(match placement {
                Unmodified => {
                    TypedRect::new(
                        TypedPoint2D::new(0.0, 0.0),
                        TypedSize2D::new(normal_width, normal_height)
                    )
                },
                Rotated(r) => {

                    fn rotate_point((x, y): (f32, f32), sin: f32, cos: f32) -> (f32, f32) {
                        ((x * cos) - (y * sin), (x * sin) + (y * cos))
                    }

                    let rot_radians = r.to_radians();
                    let sin = rot_radians.sin();
                    let cos = rot_radians.cos();

                    let top_left = (0.0, 0.0);
                    let top_right = (0.0 + normal_width, 0.0);
                    let bottom_right = (0.0 + normal_width, normal_height);
                    let bottom_left = (0.0, normal_height);

                    let (top_left_x, top_left_y) = rotate_point(top_left, sin, cos);
                    let (top_right_x, top_right_y) = rotate_point(top_right, sin, cos);
                    let (bottom_right_x, bottom_right_y) = rotate_point(bottom_right, sin, cos);
                    let (bottom_left_x, bottom_left_y) = rotate_point(bottom_left, sin, cos);

                    let min_x = top_left_x.min(top_right_x).min(bottom_right_x).min(bottom_left_x);
                    let max_x = top_left_x.max(top_right_x).max(bottom_right_x).max(bottom_left_x);
                    let min_y = top_left_y.min(top_right_y).min(bottom_right_y).min(bottom_left_y);
                    let max_y = top_left_y.max(top_right_y).max(bottom_right_y).max(bottom_left_y);

                    TypedRect::new(
                        TypedPoint2D::new(min_x, min_y),
                        TypedSize2D::new(max_x - min_x, max_y - min_y)
                    )
                },
                OnCubicBezierCurve(curve) => {
                    let (mut bbox, _bbox_indices) = curve.get_bbox();

                    // TODO: There should be a more sophisticated Bbox calculation here
                    // that takes the rotation of the text into account. Right now we simply
                    // add the font size to the BBox height, so that we can still select text
                    // even when the control points are aligned in a horizontal line.
                    //
                    // This is not so much about correctness as it is about simply making
                    // it work for now.

                    bbox.0.origin.y -= SVG_FAKE_FONT_SIZE;
                    bbox.0.size.height += SVG_FAKE_FONT_SIZE;
                    bbox.0
                }
            })
        }
    }

    impl SvgText {

        pub fn to_svg_layer(&self, vectorized_fonts_cache: &VectorizedFontCache) -> Option<SvgLayerResourceDirect> {

            let vectorized_font = vectorized_fonts_cache.get_font(&self.font_id)?;

            // The text contains the vertices and indices in unscaled units. This is so that the font
            // can be cached and later on be scaled and rotated on the GPU instead of the CPU.
            let mut text = match self.placement {
                SvgTextPlacement::Unmodified => {
                    normal_text(&self.text_layout, self.style, &*vectorized_font)
                },
                SvgTextPlacement::Rotated(degrees) => {
                    let mut text = normal_text(&self.text_layout, self.style, &*vectorized_font);
                    text.style.rotate(degrees);
                    text
                },
                SvgTextPlacement::OnCubicBezierCurve(curve) => {
                    text_on_curve(&self.text_layout, self.style, &*vectorized_font, &curve)
                },
            };

            // The glyphs are laid out to be 1px high, they are then later scaled to the correct font size
            text.style.scale(self.font_size_px, self.font_size_px);

            Some(text)
        }

        pub fn get_bbox(&self) -> SvgBbox {
            let mut bbox = self.text_layout.get_bbox(&self.placement);
            let translation = self.style.transform.translation.unwrap_or_default();
            bbox.translate(translation.x, translation.y);
            bbox
        }
    }

    pub fn normal_text(
        layout: &SvgTextLayout,
        text_style: SvgStyle,
        vectorized_font: &VectorizedFont,
    ) -> SvgLayerResourceDirect
    {
        let fill_vertices = text_style.fill.map(|_| {
            let fill_verts = vectorized_font.get_fill_vertices(&layout.layouted_glyphs.glyphs);
            normal_text_to_vertices(&layout.layouted_glyphs.glyphs, fill_verts)
        });

        let stroke_vertices = text_style.stroke.map(|stroke| {
            let stroke_verts = vectorized_font.get_stroke_vertices(&layout.layouted_glyphs.glyphs, &stroke.1);
            normal_text_to_vertices(&layout.layouted_glyphs.glyphs, stroke_verts)
        });

        SvgLayerResourceDirect {
            style: text_style,
            fill: fill_vertices,
            stroke: stroke_vertices,
        }
    }

    pub fn normal_text_to_vertices(
        glyph_ids: &[GlyphInstance],
        mut vertex_buffers: Vec<VertexBuffers<SvgVert, u32>>,
    ) -> VerticesIndicesBuffer
    {
        normal_text_to_vertices_inner(glyph_ids, &mut vertex_buffers);
        join_vertex_buffers(&vertex_buffers)
    }

    fn normal_text_to_vertices_inner(
        glyph_ids: &[GlyphInstance],
        vertex_buffers: &mut Vec<VertexBuffers<SvgVert, u32>>,
    ) {
        vertex_buffers.iter_mut().zip(glyph_ids).for_each(|(vertex_buf, gid)| {
            // NOTE: The gid.point has the font size already applied to it,
            // so we have to un-do the scaling for the glyph offsets, so all other scaling can be done on the GPU
            transform_vertex_buffer(&mut vertex_buf.vertices, gid.point.x / SVG_FAKE_FONT_SIZE, gid.point.y / SVG_FAKE_FONT_SIZE);
        });
    }

    pub fn text_on_curve(
        layout: &SvgTextLayout,
        text_style: SvgStyle,
        vectorized_font: &VectorizedFont,
        curve: &SampledBezierCurve
    ) -> SvgLayerResourceDirect
    {
        // NOTE: char offsets are now in unscaled glyph space!
        let (char_offsets, char_rotations) = curve.get_text_offsets_and_rotations(&layout.layouted_glyphs.glyphs, 0.0);

        let fill_vertices = text_style.fill.map(|_| {
            let fill_verts = vectorized_font.get_fill_vertices(&layout.layouted_glyphs.glyphs);
            curved_vector_text_to_vertices(&char_offsets, &char_rotations, fill_verts)
        });

        let stroke_vertices = text_style.stroke.map(|stroke| {
            let stroke_verts = vectorized_font.get_stroke_vertices(&layout.layouted_glyphs.glyphs, &stroke.1);
            curved_vector_text_to_vertices(&char_offsets, &char_rotations, stroke_verts)
        });

        SvgLayerResourceDirect {
            style: text_style,
            fill: fill_vertices,
            stroke: stroke_vertices,
        }
    }

    // Calculates the layout for one word block
    pub fn curved_vector_text_to_vertices(
        char_offsets: &[(f32, f32)],
        char_rotations: &[BezierCharacterRotation],
        mut vertex_buffers: Vec<VertexBuffers<SvgVert, u32>>,
    ) -> VerticesIndicesBuffer
    {
        vertex_buffers.iter_mut()
        .zip(char_rotations.into_iter())
        .zip(char_offsets.iter())
        .for_each(|((vertex_buf, char_rot), char_offset)| {
            let (char_offset_x, char_offset_y) = char_offset; // weird borrow issue
            // 1. Rotate individual characters inside of the word
            let (char_sin, char_cos) = (char_rot.0.sin(), char_rot.0.cos());
            rotate_vertex_buffer(&mut vertex_buf.vertices, char_sin, char_cos);
            // 2. Transform characters to their respective positions
            transform_vertex_buffer(&mut vertex_buf.vertices, *char_offset_x, *char_offset_y);
        });

        join_vertex_buffers(&vertex_buffers)
    }

    impl Svg {

        #[inline]
        pub fn with_layers(layers: Vec<SvgLayerResource>) -> Self {
            Self { layers: layers, .. Default::default() }
        }

        #[inline]
        pub fn with_pan(mut self, horz: f32, vert: f32) -> Self {
            self.pan = (horz, vert);
            self
        }

        #[inline]
        pub fn with_zoom(mut self, zoom: f32) -> Self {
            self.zoom = zoom;
            self
        }

        #[inline]
        pub fn with_hidpi_enabled(mut self, hidpi_enabled: bool) -> Self {
            self.enable_hidpi = hidpi_enabled;
            self
        }

        #[inline]
        pub fn with_background_color(mut self, color: ColorU) -> Self {
            self.background_color = color;
            self
        }

        /// Since there is no anti-aliasing yet, this will enlarge the texture that is drawn to by
        /// the factor X. Default is `1.0`, but you could for example, render to a `1.2x` texture.
        #[inline]
        pub fn with_multisampling_factor(mut self, multisampling_factor: usize) -> Self {
            self.multisampling_factor = multisampling_factor;
            self
        }

        #[inline]
        pub fn with_fxaa(mut self, enable_fxaa: bool) -> Self {
            self.enable_fxaa = enable_fxaa;
            self
        }

        /// Renders the SVG to a texture. This should be called in a callback, since
        /// during DOM construction, the items don't know how large they will be.
        ///
        /// The final texture will be width * height large. Note that width and height
        /// need to be multiplied with the current `HiDPI` factor, otherwise the texture
        /// will be blurry on HiDPI screens. This isn't done automatically.
        pub fn render_svg(
            &self,
            svg_cache: &SvgCache,
            gl_context: Rc<Gl>,
            hidpi_factor: f32,
            svg_size: LogicalSize,
        ) -> Texture {

        }
    }

    fn build_uniforms(
        bbox_size: &TypedSize2D<f32, SvgWorldPixel>,
        color: ColorU,
        z_index: f32,
        pan: (f32, f32),
        zoom: f32,
        layer_transform: &SvgTransform
    ) -> Vec<Uniform> {

        use azul_core::gl::UniformType::*;

        let color: ColorF = color.into();

        let (layer_rotation_center, layer_rotation_degrees) = layer_transform.rotation.unwrap_or_default();
        let (rotation_sin, rotation_cos) = layer_rotation_degrees.to_rotation();
        let layer_translation = layer_transform.translation.unwrap_or_default();
        let layer_scale_factor = layer_transform.scale.unwrap_or_default();

        vec! [

            // Vertex shader
            Uniform::new("vBboxSize", FloatVec2([bbox_size.width, bbox_size.height])),
            Uniform::new("vGlobalOffset", FloatVec2([pan.0, pan.1])),
            Uniform::new("vZIndex", Float(z_index)),
            Uniform::new("vZoom", Float(zoom)),
            Uniform::new("vRotationCenter", FloatVec2([layer_rotation_center.x, layer_rotation_center.y])),
            Uniform::new("vRotationSin", Float(rotation_sin)),
            Uniform::new("vRotationCos", Float(rotation_cos)),
            Uniform::new("vScaleFactor", FloatVec2([layer_scale_factor.x, layer_scale_factor.y])),
            Uniform::new("vTranslatePx", FloatVec2([layer_translation.x, layer_translation.y])),

            // Fragment shader
            Uniform::new("fFillColor", FloatVec4([color.r, color.g, color.b, color.a])),
        ]
    }
}

#[cfg(feature = "svg")]
use self::svg_internal::*;
*/
