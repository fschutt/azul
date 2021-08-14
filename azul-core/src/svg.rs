//! SVG rendering module

use crate::{
    gl::{
        VertexLayout, Texture, GlContextPtr, IndexBufferFormat,
        VertexLayoutDescription, VertexAttribute,
        VertexAttributeType, VertexBuffer,
    },
    ui_solver::{ComputedTransform3D, RotationMode},
    window::PhysicalSizeU32,
};
use alloc::string::String;
use azul_css::{
    StyleTransformOrigin, U32Vec,
    StyleTransformVec, ColorU, ColorF,
};
use core::fmt;
use crate::xml::XmlError;
use azul_css::{AzString, OptionAzString, StringVec, OptionLayoutSize, OptionColorU};
pub use azul_css::{SvgCubicCurve, SvgPoint};

const DEFAULT_MITER_LIMIT: f32 = 4.0;
const DEFAULT_LINE_WIDTH: f32 = 1.0;
const DEFAULT_TOLERANCE: f32 = 0.1;

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

impl_vec!(SvgPathElement, SvgPathElementVec, SvgPathElementVecDestructor);
impl_vec_debug!(SvgPathElement, SvgPathElementVec);
impl_vec_clone!(SvgPathElement, SvgPathElementVec, SvgPathElementVecDestructor);
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

impl_vec!(SvgPath, SvgPathVec, SvgPathVecDestructor);
impl_vec_debug!(SvgPath, SvgPathVec);
impl_vec_clone!(SvgPath, SvgPathVec, SvgPathVecDestructor);
impl_vec_partialeq!(SvgPath, SvgPathVec);
impl_vec_partialord!(SvgPath, SvgPathVec);

impl_vec!(SvgMultiPolygon, SvgMultiPolygonVec, SvgMultiPolygonVecDestructor);
impl_vec_debug!(SvgMultiPolygon, SvgMultiPolygonVec);
impl_vec_clone!(SvgMultiPolygon, SvgMultiPolygonVec, SvgMultiPolygonVecDestructor);
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
        let x_diff = libm::fabsf(x - self.center_x);
        let y_diff = libm::fabsf(y - self.center_y);
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

impl VertexLayoutDescription for SvgVertex {
    fn get_description() -> VertexLayout {
        VertexLayout {
            fields: vec![
                VertexAttribute {
                    name: String::from("vAttrXY").into(),
                    layout_location: Some(0).into(), // crate::gl::OptionUsize::None,
                    attribute_type: VertexAttributeType::Float,
                    item_count: 2,
                },
            ].into()
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct TessellatedSvgNode {
    pub vertices: SvgVertexVec,
    pub indices: U32Vec,
}

impl Default for TessellatedSvgNode {
    fn default() -> Self {
        Self {
            vertices: Vec::new().into(),
            indices: Vec::new().into(),
        }
    }
}

impl_vec!(TessellatedSvgNode, TessellatedSvgNodeVec, TessellatedSvgNodeVecDestructor);
impl_vec_debug!(TessellatedSvgNode, TessellatedSvgNodeVec);
impl_vec_partialord!(TessellatedSvgNode, TessellatedSvgNodeVec);
impl_vec_clone!(TessellatedSvgNode, TessellatedSvgNodeVec, TessellatedSvgNodeVecDestructor);
impl_vec_partialeq!(TessellatedSvgNode, TessellatedSvgNodeVec);

impl TessellatedSvgNode {
    pub fn empty() -> Self {
        Self::default()
    }
}

impl TessellatedSvgNodeVec {
    pub fn get_ref(&self) -> TessellatedSvgNodeVecRef {
        let slice = self.as_ref();
        TessellatedSvgNodeVecRef {
            ptr: slice.as_ptr(),
            len: slice.len(),
        }
    }
}

impl fmt::Debug for TessellatedSvgNodeVecRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.as_slice().fmt(f)
    }
}

// C ABI wrapper over &[TessellatedSvgNode]
#[repr(C)]
pub struct TessellatedSvgNodeVecRef {
    pub ptr: *const TessellatedSvgNode,
    pub len: usize,
}

impl Clone for TessellatedSvgNodeVecRef {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr,
            len: self.len,
        }
    }
}

impl TessellatedSvgNodeVecRef {
    pub fn as_slice<'a>(&'a self) -> &'a [TessellatedSvgNode] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl_vec!(SvgVertex, SvgVertexVec, SvgVertexVecDestructor);
impl_vec_debug!(SvgVertex, SvgVertexVec);
impl_vec_partialord!(SvgVertex, SvgVertexVec);
impl_vec_clone!(SvgVertex, SvgVertexVec, SvgVertexVecDestructor);
impl_vec_partialeq!(SvgVertex, SvgVertexVec);

#[derive(Debug, PartialEq, PartialOrd)]
#[repr(C)]
pub struct TessellatedGPUSvgNode {
    pub vertex_index_buffer: VertexBuffer,
}

impl TessellatedGPUSvgNode {

    /// Uploads the tesselated SVG node to GPU memory
    pub fn new(node: &TessellatedSvgNode, gl: GlContextPtr) -> Self {
        let svg_shader_id = gl.ptr.svg_shader;
        Self {
            vertex_index_buffer: VertexBuffer::new(
                gl,
                svg_shader_id,
                node.vertices.as_ref(),
                node.indices.as_ref(),
                IndexBufferFormat::Triangles
            )
        }
    }

    /// Draw the vertex buffer to the texture with the given color and transform
    ///
    /// Will resize the texture if necessary.
    pub fn draw(
        &self,
        texture: &mut Texture,
        target_size: PhysicalSizeU32,
        color: ColorU,
        transforms: StyleTransformVec
    ) -> bool {
        use crate::gl::{GlShader, Uniform, UniformType};
        use azul_css::PixelValue;

        let transform_origin = StyleTransformOrigin {
            x: PixelValue::px(target_size.width as f32 / 2.0),
            y: PixelValue::px(target_size.height as f32 / 2.0),
        };

        let computed_transform = ComputedTransform3D::from_style_transform_vec(
            transforms.as_ref(),
            &transform_origin,
            target_size.width as f32,
            target_size.height as f32,
            RotationMode::ForWebRender
        );

        let color: ColorF = color.into();

        // uniforms for the SVG shader
        let uniforms = [
            Uniform {
                name: "vBboxSize".into(),
                uniform_type: UniformType::FloatVec2([target_size.width as f32, target_size.height as f32])
            },
            Uniform {
                name: "vTransformMatrix".into(),
                uniform_type: UniformType::Matrix4 {
                    transpose: false,
                    matrix: unsafe { core::mem::transmute(computed_transform.m) }
                }
            },
            Uniform {
                name: "fDrawColor".into(),
                uniform_type: UniformType::FloatVec4([color.r, color.g, color.b, color.a])
            },
        ];

        GlShader::draw(
            texture.gl_context.ptr.svg_shader,
            texture,
            &[(&self.vertex_index_buffer, &uniforms[..])]
        );

        true
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C, u8)]
pub enum SvgStyle {
    Fill(SvgFillStyle),
    Stroke(SvgStrokeStyle),
}

impl SvgStyle {
    pub fn get_antialias(&self) -> bool {
        match self {
            SvgStyle::Fill(f) => f.anti_alias,
            SvgStyle::Stroke(s) => s.anti_alias,
        }
    }
    pub fn get_high_quality_aa(&self) -> bool {
        match self {
            SvgStyle::Fill(f) => f.high_quality_aa,
            SvgStyle::Stroke(s) => s.high_quality_aa,
        }
    }
    pub fn get_transform(&self) -> SvgTransform {
        match self {
            SvgStyle::Fill(f) => f.transform,
            SvgStyle::Stroke(s) => s.transform,
        }
    }
}
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub enum SvgFillRule {
    Winding,
    EvenOdd,
}

impl Default for SvgFillRule {
    fn default() -> Self { SvgFillRule::Winding }
}

#[derive(Default, Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgTransform {
    pub sx: f32,
    pub kx: f32,
    pub ky: f32,
    pub sy: f32,
    pub tx: f32,
    pub ty: f32,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
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
    pub miter_limit: f32,
    /// Maximum allowed distance to the path when building an approximation.
    ///
    /// See [Flattening and tolerance](index.html#flattening-and-tolerance).
    /// Default value: `StrokeOptions::DEFAULT_TOLERANCE`.
    pub tolerance: f32,
    /// Whether to use the "winding" or "even / odd" fill rule when tesselating the path
    pub fill_rule: SvgFillRule,
    /// Whether to apply a transform to the points in the path (warning: will be done on the CPU - expensive)
    pub transform: SvgTransform,
    /// Whether the fill is intended to be anti-aliased (default: true)
    pub anti_alias: bool,
    /// Whether the anti-aliasing has to be of high quality (default: false)
    pub high_quality_aa: bool,
}

impl Default for SvgFillStyle {
    fn default() -> Self {
        Self {
            line_join: SvgLineJoin::Miter,
            miter_limit: DEFAULT_MITER_LIMIT,
            tolerance: DEFAULT_TOLERANCE,
            fill_rule: SvgFillRule::default(),
            transform: SvgTransform::default(),
            anti_alias: true,
            high_quality_aa: false,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
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
    pub line_width: f32,
    /// See the SVG specification.
    ///
    /// Must be greater than or equal to 1.0.
    /// Default value: `StrokeOptions::DEFAULT_MITER_LIMIT`.
    pub miter_limit: f32,
    /// Maximum allowed distance to the path when building an approximation.
    ///
    /// See [Flattening and tolerance](index.html#flattening-and-tolerance).
    /// Default value: `StrokeOptions::DEFAULT_TOLERANCE`.
    pub tolerance: f32,
    /// Apply line width
    ///
    /// When set to false, the generated vertices will all be positioned in the centre
    /// of the line. The width can be applied later on (eg in a vertex shader) by adding
    /// the vertex normal multiplied by the line with to each vertex position.
    ///
    /// Default value: `true`. NOTE: currently unused!
    pub apply_line_width: bool,
    /// Whether to apply a transform to the points in the path (warning: will be done on the CPU - expensive)
    pub transform: SvgTransform,
    /// Whether the fill is intended to be anti-aliased (default: true)
    pub anti_alias: bool,
    /// Whether the anti-aliasing has to be of high quality (default: false)
    pub high_quality_aa: bool,
}

impl Default for SvgStrokeStyle {
    fn default() -> Self {
        Self {
            start_cap: SvgLineCap::default(),
            end_cap: SvgLineCap::default(),
            line_join: SvgLineJoin::default(),
            dash_pattern: OptionSvgDashPattern::None,
            line_width: DEFAULT_LINE_WIDTH,
            miter_limit: DEFAULT_MITER_LIMIT,
            tolerance: DEFAULT_TOLERANCE,
            apply_line_width: true,
            anti_alias: true,
            high_quality_aa: false,
            transform: SvgTransform::default(),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgDashPattern {
    pub offset: f32,
    pub length_1: f32,
    pub gap_1: f32,
    pub length_2: f32,
    pub gap_2: f32,
    pub length_3: f32,
    pub gap_3: f32,
}

impl_option!(SvgDashPattern, OptionSvgDashPattern, [Debug, Copy, Clone, PartialEq, PartialOrd]);

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

#[allow(non_camel_case_types)]
pub enum c_void { }

pub type GlyphId = u16;

#[derive(Clone, Debug)]
#[repr(C)]
pub struct SvgXmlNode {
    pub node: *const c_void, // usvg::Node
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct Svg {
    tree: *const c_void, // *mut usvg::Tree,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub enum ShapeRendering {
    OptimizeSpeed,
    CrispEdges,
    GeometricPrecision,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub enum ImageRendering {
    OptimizeQuality,
    OptimizeSpeed,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub enum TextRendering {
    OptimizeSpeed,
    OptimizeLegibility,
    GeometricPrecision,
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
    pub target_size: OptionLayoutSize,
    pub background_color: OptionColorU,
    pub fit: SvgFitTo,
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

#[derive(Debug, Clone, PartialEq, PartialOrd)]
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


#[derive(Debug, PartialEq, PartialOrd, Clone)]
#[repr(C, u8)]
pub enum SvgParseError {
    NoParserAvailable,
    InvalidFileSuffix,
    FileOpenFailed,
    NotAnUtf8Str,
    MalformedGZip,
    InvalidSize,
    ParsingFailed(XmlError),
}

impl fmt::Display for SvgParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::SvgParseError::*;
        match self {
            NoParserAvailable => write!(f, "Library was compiled without SVG support (no parser available)"),
            InvalidFileSuffix => write!(f, "Error parsing SVG: Invalid file suffix"),
            FileOpenFailed => write!(f, "Error parsing SVG: Failed to open file"),
            NotAnUtf8Str => write!(f, "Error parsing SVG: Not an UTF-8 String"),
            MalformedGZip => write!(f, "Error parsing SVG: SVG is compressed with a malformed GZIP compression"),
            InvalidSize => write!(f, "Error parsing SVG: Invalid size"),
            ParsingFailed(e) => write!(f, "Error parsing SVG: Parsing SVG as XML failed: {}", e),
        }
    }
}

impl_result!(SvgXmlNode, SvgParseError, ResultSvgXmlNodeSvgParseError, copy = false, [Debug, Clone]);
impl_result!(Svg, SvgParseError, ResultSvgSvgParseError, copy = false, [Debug, Clone]);

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C, u8)]
pub enum Indent {
    None,
    Spaces(u8),
    Tabs,
}
