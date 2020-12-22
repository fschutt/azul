//! SVG rendering module

#[cfg(feature = "opengl")]
use crate::gl::{
    GlShader, VertexLayout, GlContextPtr,
    VertexLayoutDescription, VertexAttribute,
    VertexAttributeType, GlApiVersion, VertexBuffer,
    GlShaderCreateError,
};

#[cfg(feature = "opengl")]
static mut SVG_SHADER: Option<SvgShader> = None;

const SHADER_VERSION_GL: &str = "#version 150";
const SHADER_VERSION_GLES: &str = "#version 300 es";

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

#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgPoint {
    pub x: f32,
    pub y: f32,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgSize {
    pub width: f32,
    pub height: f32,
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

impl SvgPathElement {
    pub fn get_start(&self) -> SvgPoint {
        match self {
            SvgPathElement::Line(l) => l.start,
            SvgPathElement::QuadraticCurve(qc) => qc.start,
            SvgPathElement::CubicCurve(cc) => cc.start,
        }
    }
    pub fn get_end(&self) -> SvgPoint {
        match self {
            SvgPathElement::Line(l) => l.end,
            SvgPathElement::QuadraticCurve(qc) => qc.end,
            SvgPathElement::CubicCurve(cc) => cc.end,
        }
    }
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

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgMultiPolygon {
    /// NOTE: If a ring represends a hole, simply reverse the order of points
    pub rings: SvgPathVec,
}

unsafe impl Send for SvgMultiPolygon { }
unsafe impl Sync for SvgMultiPolygon { }

impl_vec!(SvgPath, SvgPathVec);
impl_vec_debug!(SvgPath, SvgPathVec);
impl_vec_clone!(SvgPath, SvgPathVec);
impl_vec_partialeq!(SvgPath, SvgPathVec);
impl_vec_partialord!(SvgPath, SvgPathVec);

impl_vec!(SvgMultiPolygon, SvgMultiPolygonVec);
impl_vec_debug!(SvgMultiPolygon, SvgMultiPolygonVec);
impl_vec_clone!(SvgMultiPolygon, SvgMultiPolygonVec);
impl_vec_partialeq!(SvgMultiPolygon, SvgMultiPolygonVec);
impl_vec_partialord!(SvgMultiPolygon, SvgMultiPolygonVec);

impl SvgPath {
    pub fn is_closed(&self) -> bool {
        let first = self.items.as_ref().first();
        let last = self.items.as_ref().last();
        match (first, last) {
            (Some(f), Some(l)) => (f.get_start() == l.get_end()),
            _ => false,
        }
    }
}

/// One `SvgNode` corresponds to one SVG `<path></path>` element
#[derive(Debug, Clone, PartialOrd, PartialEq)]
#[repr(C, u8)]
pub enum SvgNode {
    /// Multiple multipolygons, merged to one CPU buf for efficient drawing
    MultiPolygonCollection(SvgMultiPolygonVec),
    MultiPolygon(SvgMultiPolygon),
    Path(SvgPath),
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
    pub radius_top_left: f32,
    pub radius_top_right: f32,
    pub radius_bottom_left: f32,
    pub radius_bottom_right: f32,
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
            ].into()
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, PartialOrd)]
#[repr(C)]
pub struct TesselatedCPUSvgNode {
    pub vertices: SvgVertexVec,
    pub indices: U32Vec,
}

unsafe impl Send for TesselatedCPUSvgNode { }
unsafe impl Sync for TesselatedCPUSvgNode { }

impl TesselatedCPUSvgNode {
    pub fn empty() -> Self {
        Self::default()
    }
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

#[cfg(feature = "opengl")]
#[derive(Debug, PartialEq, PartialOrd)]
#[repr(C)]
pub struct TesselatedGPUSvgNode {
    pub vertex_index_buffer: VertexBuffer,
}

#[cfg(feature = "opengl")]
#[derive(Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[repr(C)]
pub struct SvgShader {
    pub program: GlShader,
}

#[cfg(feature = "opengl")]
impl SvgShader {
    pub fn new(gl_context: &GlContextPtr) -> Result<Self, GlShaderCreateError> {
        let current_gl_api = GlApiVersion::get(gl_context);
        let vertex_source_prefixed = prefix_gl_version(SVG_VERTEX_SHADER, current_gl_api);
        let fragment_source_prefixed = prefix_gl_version(SVG_FRAGMENT_SHADER, current_gl_api);
        let program = GlShader::new(gl_context, &vertex_source_prefixed, &fragment_source_prefixed)?;
        Ok(Self { program })
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
    pub miter_limit: usize,

    /// Maximum allowed distance to the path when building an approximation.
    ///
    /// See [Flattening and tolerance](index.html#flattening-and-tolerance).
    /// Default value: `StrokeOptions::DEFAULT_TOLERANCE`.
    pub tolerance: usize,
}

impl SvgFillStyle {
    /// NOTE: Getters and setters are necessary here, because the line width, miter limit, etc.
    /// are all normalized to fit into a usize
    pub fn with_miter_limit(mut self, miter_limit: f32) -> Self { self.set_miter_limit(miter_limit); self }
    pub fn set_miter_limit(&mut self, miter_limit: f32) { self.miter_limit = (miter_limit * SVG_LINE_PRECISION) as usize; }
    pub fn get_miter_limit(&self) -> f32 { self.miter_limit as f32 / SVG_LINE_PRECISION }
    pub fn with_tolerance(mut self, tolerance: f32) -> Self { self.set_tolerance(tolerance); self }
    pub fn set_tolerance(&mut self, tolerance: f32) { self.tolerance = (tolerance * SVG_LINE_PRECISION) as usize; }
    pub fn get_tolerance(&self) -> f32 { self.tolerance as f32 / SVG_LINE_PRECISION }
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

#[cfg(feature = "opengl")] mod internal_2 {

    use super::*;
    use crate::gl::{Uniform, Texture};
    use crate::window::{LogicalSize, PhysicalSizeU32};
    use gleam::gl;

    /// NOTE: may not be called from more than 1 thread
    pub fn get_svg_shader(gl_context: &GlContextPtr) -> Option<&SvgShader> {
        if let Some(s) = unsafe { SVG_SHADER.as_ref() } {
            Some(s)
        } else {
            let svg_shader = match SvgShader::new(gl_context) {
                Ok(o) => o,
                Err(_e) => {
                    #[cfg(feature = "logging")] { error!("could not compile SVG shader: {}", _e); }
                    return None;
                },
            };
            unsafe {
                SVG_SHADER = Some(svg_shader);
                SVG_SHADER.as_ref()
            }
        }
    }

    pub fn upload_tesselated_path_to_gpu(gl_context: &GlContextPtr, cpu_data: &TesselatedCPUSvgNode) -> Option<TesselatedGPUSvgNode> {
        use crate::gl::IndexBufferFormat;
        let shader = get_svg_shader(gl_context)?;
        let vertex_index_buffer = VertexBuffer::new(&shader.program, cpu_data.vertices.as_ref(), cpu_data.indices.as_ref(), IndexBufferFormat::Triangles);
        Some(TesselatedGPUSvgNode { vertex_index_buffer })
    }

    pub fn draw_gpu_buf_to_texture(buf: TesselatedGPUSvgNode, svg_size: LogicalSize, texture_size: PhysicalSizeU32) -> Option<Texture> {
        use azul_css::ColorU;
        let shader = get_svg_shader(&buf.vertex_index_buffer.vao.gl_context)?;
        let shader = &shader.program;

        buf.vertex_index_buffer.vao.gl_context.enable(gl::PRIMITIVE_RESTART_FIXED_INDEX);
        let uniforms = build_uniforms(svg_size);
        let texture = shader.draw(&[(&buf, &uniforms)], Some(ColorU::TRANSPARENT), texture_size);
        buf.vertex_index_buffer.vao.gl_context.disable(gl::PRIMITIVE_RESTART_FIXED_INDEX);
        Some(texture)
    }

    fn build_uniforms(bbox_size: LogicalSize) -> Vec<Uniform> {
        use crate::gl::UniformType::*;
        vec! [
            // Vertex shader
            Uniform::new(String::from("vBboxSize"), FloatVec2([bbox_size.width, bbox_size.height])),
        ]
    }
}

#[cfg(feature = "opengl")] pub use self::internal_2::*;
