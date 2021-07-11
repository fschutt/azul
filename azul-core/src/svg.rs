//! SVG rendering module

use crate::gl::{
    VertexLayout,
    VertexLayoutDescription, VertexAttribute,
    VertexAttributeType, VertexBuffer,
};
use alloc::string::String;
use azul_css::U32Vec;
use core::fmt;
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
pub struct TesselatedSvgNode {
    pub vertices: SvgVertexVec,
    pub indices: U32Vec,
}

impl_vec!(TesselatedSvgNode, TesselatedSvgNodeVec, TesselatedSvgNodeVecDestructor);
impl_vec_debug!(TesselatedSvgNode, TesselatedSvgNodeVec);
impl_vec_partialord!(TesselatedSvgNode, TesselatedSvgNodeVec);
impl_vec_clone!(TesselatedSvgNode, TesselatedSvgNodeVec, TesselatedSvgNodeVecDestructor);
impl_vec_partialeq!(TesselatedSvgNode, TesselatedSvgNodeVec);

impl TesselatedSvgNode {
    pub fn empty() -> Self {
        Self::default()
    }
}

impl TesselatedSvgNodeVec {
    pub fn get_ref(&self) -> TesselatedSvgNodeVecRef {
        let slice = self.as_ref();
        TesselatedSvgNodeVecRef {
            ptr: slice.as_ptr(),
            len: slice.len(),
        }
    }
}

impl fmt::Debug for TesselatedSvgNodeVecRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.as_slice().fmt(f)
    }
}

// C ABI wrapper over &[TesselatedSvgNode]
#[repr(C)]
pub struct TesselatedSvgNodeVecRef {
    pub ptr: *const TesselatedSvgNode,
    pub len: usize,
}

impl Clone for TesselatedSvgNodeVecRef {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr,
            len: self.len,
        }
    }
}

impl TesselatedSvgNodeVecRef {
    pub fn as_slice<'a>(&'a self) -> &'a [TesselatedSvgNode] {
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
pub struct TesselatedGPUSvgNode {
    pub vertex_index_buffer: VertexBuffer,
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
    /// Default value: `true`.
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
