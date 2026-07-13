//! SVG rendering and path tessellation.
//!
//! This module provides functionality for parsing, manipulating, and rendering SVG paths.
//! It includes:
//!
//! - **Path tessellation**: Converts SVG paths into triangle meshes for GPU rendering
//! - **Stroke generation**: Creates stroked paths with various line join and cap styles
//! - **Transform support**: Applies CSS transforms to SVG elements
//! - **Style parsing**: Handles SVG fill, stroke, opacity, and other attributes
//!
//! The module uses Lyon for geometric tessellation and generates vertex/index buffers
//! that can be uploaded to WebRender for hardware-accelerated rendering.

use alloc::{
    string::{String, ToString},
    vec::Vec,
};
use core::fmt;

use azul_css::{
    props::{
        basic::{
            ColorF, ColorU, OptionColorU, OptionLayoutSize, PixelValue, SvgCubicCurve, SvgPoint,
            SvgQuadraticCurve, SvgRect, SvgVector,
        },
        style::{StyleTransform, StyleTransformOrigin, StyleTransformVec},
    },
    AzString, OptionString, StringVec, U32Vec,
};

use crate::{
    geom::PhysicalSizeU32,
    gl::{
        GlContextPtr, GlShader, IndexBufferFormat, Texture, Uniform, UniformType, VertexAttribute,
        VertexAttributeType, VertexBuffer, VertexLayout, VertexLayoutDescription,
    },
    transform::{ComputedTransform3D, RotationMode},
    xml::XmlError,
};

/// Default miter limit for stroke joins (ratio of miter length to stroke width)
const DEFAULT_MITER_LIMIT: f32 = 4.0;
/// Default stroke width in pixels
const DEFAULT_LINE_WIDTH: f32 = 1.0;
/// Default tessellation tolerance in pixels (smaller = more vertices, higher quality)
const DEFAULT_TOLERANCE: f32 = 0.1;

/// Represents the dimensions of an SVG viewport or element.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgSize {
    /// Width in SVG user units
    pub width: f32,
    /// Height in SVG user units
    pub height: f32,
}

/// A line segment in 2D space.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgLine {
    /// Start point of the line
    pub start: SvgPoint,
    /// End point of the line
    pub end: SvgPoint,
}

impl SvgLine {
    /// Creates a new line segment from start to end point
    #[inline]
    #[must_use] pub const fn new(start: SvgPoint, end: SvgPoint) -> Self {
        Self { start, end }
    }

    /// Computes the inward-facing normal vector for this line.
    ///
    /// The normal points 90 degrees to the right of the line direction.
    /// Returns `None` if the line has zero length.
    #[must_use] pub fn inwards_normal(&self) -> Option<SvgPoint> {
        let dx = self.end.x - self.start.x;
        let dy = self.end.y - self.start.y;
        let edge_length = dx.hypot(dy);
        let x = -dy / edge_length;
        let y = dx / edge_length;

        if x.is_finite() && y.is_finite() {
            Some(SvgPoint { x, y })
        } else {
            None
        }
    }

    /// Computes the outward-facing normal vector for this line (opposite of `inwards_normal`).
    #[must_use] pub fn outwards_normal(&self) -> Option<SvgPoint> {
        let inwards = self.inwards_normal()?;
        Some(SvgPoint {
            x: -inwards.x,
            y: -inwards.y,
        })
    }

    /// Reverses the direction of the line by swapping start and end points.
    pub const fn reverse(&mut self) {
        core::mem::swap(&mut self.start, &mut self.end);
    }
    /// Returns the start point of the line.
    #[must_use] pub const fn get_start(&self) -> SvgPoint {
        self.start
    }
    /// Returns the end point of the line.
    #[must_use] pub const fn get_end(&self) -> SvgPoint {
        self.end
    }

    /// Returns the parametric `t` value (0.0–1.0) at the given arc-length offset.
    #[must_use] pub fn get_t_at_offset(&self, offset: f64) -> f64 {
        offset / self.get_length()
    }

    /// Returns the tangent vector of the line.
    /// For a line, the tangent is constant (same direction everywhere),
    /// so no `t` parameter is needed.
    #[must_use] pub fn get_tangent_vector_at_t(&self) -> SvgVector {
        let dx = self.end.x - self.start.x;
        let dy = self.end.y - self.start.y;
        SvgVector {
            x: f64::from(dx),
            y: f64::from(dy),
        }
        .normalize()
    }

    /// Returns the X coordinate at parametric position `t` (0.0 = start, 1.0 = end).
    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
    #[must_use] pub fn get_x_at_t(&self, t: f64) -> f64 {
        f64::from(self.start.x) + (f64::from(self.end.x) - f64::from(self.start.x)) * t
    }

    /// Returns the Y coordinate at parametric position `t` (0.0 = start, 1.0 = end).
    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
    #[must_use] pub fn get_y_at_t(&self, t: f64) -> f64 {
        f64::from(self.start.y) + (f64::from(self.end.y) - f64::from(self.start.y)) * t
    }

    /// Returns the Euclidean length of the line segment.
    #[must_use] pub fn get_length(&self) -> f64 {
        let dx = self.end.x - self.start.x;
        let dy = self.end.y - self.start.y;
        f64::from(libm::hypotf(dx, dy))
    }

    /// Returns the axis-aligned bounding rectangle of this line segment.
    #[must_use] pub fn get_bounds(&self) -> SvgRect {
        let min_x = self.start.x.min(self.end.x);
        let max_x = self.start.x.max(self.end.x);

        let min_y = self.start.y.min(self.end.y);
        let max_y = self.start.y.max(self.end.y);

        let width = (max_x - min_x).abs();
        let height = (max_y - min_y).abs();

        SvgRect {
            width,
            height,
            x: min_x,
            y: min_y,
            radius_top_left: 0.0,
            radius_top_right: 0.0,
            radius_bottom_left: 0.0,
            radius_bottom_right: 0.0,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C, u8)]
pub enum SvgPathElement {
    Line(SvgLine),
    QuadraticCurve(SvgQuadraticCurve),
    CubicCurve(SvgCubicCurve),
}

impl_option!(
    SvgPathElement,
    OptionSvgPathElement,
    [Debug, Copy, Clone, PartialEq, PartialOrd]
);

impl SvgPathElement {
    /// Creates a line path element from a `SvgLine`
    #[inline]
    #[must_use] pub const fn line(l: SvgLine) -> Self {
        Self::Line(l)
    }

    /// Creates a quadratic curve path element from a `SvgQuadraticCurve`
    #[inline]
    #[must_use] pub const fn quadratic_curve(qc: SvgQuadraticCurve) -> Self {
        Self::QuadraticCurve(qc)
    }

    /// Creates a cubic curve path element from a `SvgCubicCurve`
    #[inline]
    #[must_use] pub const fn cubic_curve(cc: SvgCubicCurve) -> Self {
        Self::CubicCurve(cc)
    }

    /// Sets the end point of this path element.
    pub const fn set_last(&mut self, point: SvgPoint) {
        match self {
            Self::Line(l) => l.end = point,
            Self::QuadraticCurve(qc) => qc.end = point,
            Self::CubicCurve(cc) => cc.end = point,
        }
    }

    /// Sets the start point of this path element.
    pub const fn set_first(&mut self, point: SvgPoint) {
        match self {
            Self::Line(l) => l.start = point,
            Self::QuadraticCurve(qc) => qc.start = point,
            Self::CubicCurve(cc) => cc.start = point,
        }
    }

    /// Reverses the direction of this path element.
    pub const fn reverse(&mut self) {
        match self {
            Self::Line(l) => l.reverse(),
            Self::QuadraticCurve(qc) => qc.reverse(),
            Self::CubicCurve(cc) => cc.reverse(),
        }
    }
    /// Returns the start point of this path element.
    #[must_use] pub const fn get_start(&self) -> SvgPoint {
        match self {
            Self::Line(l) => l.get_start(),
            Self::QuadraticCurve(qc) => qc.get_start(),
            Self::CubicCurve(cc) => cc.get_start(),
        }
    }
    /// Returns the end point of this path element.
    #[must_use] pub const fn get_end(&self) -> SvgPoint {
        match self {
            Self::Line(l) => l.get_end(),
            Self::QuadraticCurve(qc) => qc.get_end(),
            Self::CubicCurve(cc) => cc.get_end(),
        }
    }
    /// Returns the axis-aligned bounding rectangle of this path element.
    #[must_use] pub fn get_bounds(&self) -> SvgRect {
        match self {
            Self::Line(l) => l.get_bounds(),
            Self::QuadraticCurve(qc) => qc.get_bounds(),
            Self::CubicCurve(cc) => cc.get_bounds(),
        }
    }
    /// Returns the arc length of this path element.
    #[must_use] pub fn get_length(&self) -> f64 {
        match self {
            Self::Line(l) => l.get_length(),
            Self::QuadraticCurve(qc) => qc.get_length(),
            Self::CubicCurve(cc) => cc.get_length(),
        }
    }
    /// Returns the parametric `t` value at the given arc-length offset.
    #[must_use] pub fn get_t_at_offset(&self, offset: f64) -> f64 {
        match self {
            Self::Line(l) => l.get_t_at_offset(offset),
            Self::QuadraticCurve(qc) => qc.get_t_at_offset(offset),
            Self::CubicCurve(cc) => cc.get_t_at_offset(offset),
        }
    }
    /// Returns the normalized tangent vector at parametric position `t`.
    #[must_use] pub fn get_tangent_vector_at_t(&self, t: f64) -> SvgVector {
        match self {
            Self::Line(l) => l.get_tangent_vector_at_t(),
            Self::QuadraticCurve(qc) => qc.get_tangent_vector_at_t(t),
            Self::CubicCurve(cc) => cc.get_tangent_vector_at_t(t),
        }
    }
    /// Returns the X coordinate at parametric position `t`.
    #[must_use] pub fn get_x_at_t(&self, t: f64) -> f64 {
        match self {
            Self::Line(l) => l.get_x_at_t(t),
            Self::QuadraticCurve(qc) => qc.get_x_at_t(t),
            Self::CubicCurve(cc) => cc.get_x_at_t(t),
        }
    }
    /// Returns the Y coordinate at parametric position `t`.
    #[must_use] pub fn get_y_at_t(&self, t: f64) -> f64 {
        match self {
            Self::Line(l) => l.get_y_at_t(t),
            Self::QuadraticCurve(qc) => qc.get_y_at_t(t),
            Self::CubicCurve(cc) => cc.get_y_at_t(t),
        }
    }
}

impl_vec!(SvgPathElement, SvgPathElementVec, SvgPathElementVecDestructor, SvgPathElementVecDestructorType, SvgPathElementVecSlice, OptionSvgPathElement);
impl_vec_debug!(SvgPathElement, SvgPathElementVec);
impl_vec_clone!(
    SvgPathElement,
    SvgPathElementVec,
    SvgPathElementVecDestructor
);
impl_vec_partialeq!(SvgPathElement, SvgPathElementVec);
impl_vec_partialord!(SvgPathElement, SvgPathElementVec);

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgPath {
    pub items: SvgPathElementVec,
}

impl_option!(
    SvgPath,
    OptionSvgPath,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd]
);

impl SvgPath {
    /// Creates a new `SvgPath` from a vector of path elements
    #[inline]
    #[must_use] pub const fn create(items: SvgPathElementVec) -> Self {
        Self { items }
    }

    /// Returns the start point of the first element, or `None` if the path is empty.
    #[must_use] pub fn get_start(&self) -> Option<SvgPoint> {
        self.items.as_ref().first().map(SvgPathElement::get_start)
    }

    /// Returns the end point of the last element, or `None` if the path is empty.
    #[must_use] pub fn get_end(&self) -> Option<SvgPoint> {
        self.items.as_ref().last().map(SvgPathElement::get_end)
    }

    /// Closes the path by appending a line from the last point to the first point, if needed.
    pub fn close(&mut self) {
        let Some(first) = self.items.as_ref().first() else {
            return;
        };
        let Some(last) = self.items.as_ref().last() else {
            return;
        };
        if first.get_start() != last.get_end() {
            let mut elements = self.items.as_slice().to_vec();
            elements.push(SvgPathElement::Line(SvgLine {
                start: last.get_end(),
                end: first.get_start(),
            }));
            self.items = elements.into();
        }
    }

    /// Returns `true` if the path's first start point equals its last end point.
    #[must_use] pub fn is_closed(&self) -> bool {
        let first = self.items.as_ref().first();
        let last = self.items.as_ref().last();
        match (first, last) {
            (Some(f), Some(l)) => (f.get_start() == l.get_end()),
            _ => false,
        }
    }

    /// Reverses the order and direction of all elements in the path.
    pub fn reverse(&mut self) {
        // swap self.items with a default vec
        let mut vec = SvgPathElementVec::from_const_slice(&[]);
        core::mem::swap(&mut vec, &mut self.items);
        let mut vec = vec.into_library_owned_vec();

        // reverse the order of items in the vec
        vec.reverse();

        // reverse the order inside the item itself
        // i.e. swap line.start and line.end
        for item in &mut vec {
            item.reverse();
        }

        // swap back
        let mut vec = SvgPathElementVec::from_vec(vec);
        core::mem::swap(&mut vec, &mut self.items);
    }

    /// Joins another path onto the end of this one, interpolating the join point.
    pub fn join_with(&mut self, mut path: Self) -> Option<()> {
        let self_last_point = self.items.as_ref().last()?.get_end();
        let other_start_point = path.items.as_ref().first()?.get_start();
        let interpolated_join_point = SvgPoint {
            x: f32::midpoint(self_last_point.x, other_start_point.x),
            y: f32::midpoint(self_last_point.y, other_start_point.y),
        };

        // swap self.items with a default vec
        let mut vec = SvgPathElementVec::from_const_slice(&[]);
        core::mem::swap(&mut vec, &mut self.items);
        let mut vec = vec.into_library_owned_vec();

        let mut other = SvgPathElementVec::from_const_slice(&[]);
        core::mem::swap(&mut other, &mut path.items);
        let mut other = other.into_library_owned_vec();

        let vec_len = vec.len() - 1;
        vec.get_mut(vec_len)?.set_last(interpolated_join_point);
        other.get_mut(0)?.set_first(interpolated_join_point);
        vec.append(&mut other);

        // swap back
        let mut vec = SvgPathElementVec::from_vec(vec);
        core::mem::swap(&mut vec, &mut self.items);

        Some(())
    }
    /// Returns the axis-aligned bounding rectangle of the entire path.
    #[must_use] pub fn get_bounds(&self) -> SvgRect {
        let mut first_bounds = match self.items.as_ref().first() {
            Some(s) => s.get_bounds(),
            None => return SvgRect::default(),
        };

        for mp in self.items.as_ref().iter().skip(1) {
            let mp_bounds = mp.get_bounds();
            first_bounds.union_with(&mp_bounds);
        }

        first_bounds
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgMultiPolygon {
    /// NOTE: If a ring represents a hole, simply reverse the order of points
    pub rings: SvgPathVec,
}

impl_option!(
    SvgMultiPolygon,
    OptionSvgMultiPolygon,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd]
);

impl SvgMultiPolygon {
    /// Creates a new `SvgMultiPolygon` from a vector of paths (rings)
    /// NOTE: If a ring represents a hole, simply reverse the order of points
    #[inline]
    #[must_use] pub const fn create(rings: SvgPathVec) -> Self {
        Self { rings }
    }

    /// Returns the axis-aligned bounding rectangle of all rings in this multi-polygon.
    #[must_use] pub fn get_bounds(&self) -> SvgRect {
        let Some(mut first_bounds) = self
            .rings
            .get(0)
            .and_then(|b| b.items.get(0).map(SvgPathElement::get_bounds))
        else {
            // Empty polygon has zero-sized bounds at origin
            return SvgRect::default();
        };

        for ring in &self.rings {
            for item in &ring.items {
                first_bounds.union_with(&item.get_bounds());
            }
        }

        first_bounds
    }
}

impl_vec!(SvgPath, SvgPathVec, SvgPathVecDestructor, SvgPathVecDestructorType, SvgPathVecSlice, OptionSvgPath);
impl_vec_debug!(SvgPath, SvgPathVec);
impl_vec_clone!(SvgPath, SvgPathVec, SvgPathVecDestructor);
impl_vec_partialeq!(SvgPath, SvgPathVec);
impl_vec_partialord!(SvgPath, SvgPathVec);

impl_vec!(SvgMultiPolygon, SvgMultiPolygonVec, SvgMultiPolygonVecDestructor, SvgMultiPolygonVecDestructorType, SvgMultiPolygonVecSlice, OptionSvgMultiPolygon);
impl_vec_debug!(SvgMultiPolygon, SvgMultiPolygonVec);
impl_vec_clone!(
    SvgMultiPolygon,
    SvgMultiPolygonVec,
    SvgMultiPolygonVecDestructor
);
impl_vec_partialeq!(SvgMultiPolygon, SvgMultiPolygonVec);
impl_vec_partialord!(SvgMultiPolygon, SvgMultiPolygonVec);

/// One `SvgNode` corresponds to one SVG `<path></path>` element
#[derive(Debug, Clone, PartialOrd, PartialEq)]
#[repr(C, u8)]
pub enum SvgNode {
    /// Multiple multipolygons, merged to one CPU buf for efficient drawing
    MultiPolygonCollection(SvgMultiPolygonVec),
    MultiPolygon(SvgMultiPolygon),
    MultiShape(SvgSimpleNodeVec),
    Path(SvgPath),
    Circle(SvgCircle),
    Rect(SvgRect),
}

/// One `SvgSimpleNode` is either a path, a rect or a circle
#[derive(Debug, Clone, PartialOrd, PartialEq)]
#[repr(C, u8)]
pub enum SvgSimpleNode {
    Path(SvgPath),
    Circle(SvgCircle),
    Rect(SvgRect),
    CircleHole(SvgCircle),
    RectHole(SvgRect),
}

impl_option!(
    SvgSimpleNode,
    OptionSvgSimpleNode,
    copy = false,
    [Debug, Clone, PartialOrd, PartialEq]
);

impl_vec!(SvgSimpleNode, SvgSimpleNodeVec, SvgSimpleNodeVecDestructor, SvgSimpleNodeVecDestructorType, SvgSimpleNodeVecSlice, OptionSvgSimpleNode);
impl_vec_debug!(SvgSimpleNode, SvgSimpleNodeVec);
impl_vec_clone!(SvgSimpleNode, SvgSimpleNodeVec, SvgSimpleNodeVecDestructor);
impl_vec_partialeq!(SvgSimpleNode, SvgSimpleNodeVec);
impl_vec_partialord!(SvgSimpleNode, SvgSimpleNodeVec);

impl SvgSimpleNode {
    /// Returns the axis-aligned bounding rectangle of this node.
    // Same-body arms dispatch on differently-typed bindings (SvgPath vs SvgCircle),
    // so the identical `a.get_bounds()` bodies cannot be combined into one or-pattern.
    #[allow(clippy::match_same_arms)]
    #[must_use] pub fn get_bounds(&self) -> SvgRect {
        match self {
            Self::Path(a) => a.get_bounds(),
            Self::Circle(a) => a.get_bounds(),
            Self::Rect(a) => *a,
            Self::CircleHole(a) => a.get_bounds(),
            Self::RectHole(a) => *a,
        }
    }
    /// Returns `true` if this node represents a closed shape.
    #[must_use] pub fn is_closed(&self) -> bool {
        match self {
            Self::Path(a) => a.is_closed(),
            Self::Circle(_) | Self::Rect(_) | Self::CircleHole(_) | Self::RectHole(_) => true,
        }
    }
}

impl SvgNode {
    /// Returns the axis-aligned bounding rectangle of this SVG node.
    #[must_use] pub fn get_bounds(&self) -> SvgRect {
        match self {
            Self::MultiPolygonCollection(a) => {
                let mut first_mp_bounds = match a.get(0) {
                    Some(s) => s.get_bounds(),
                    None => return SvgRect::default(),
                };
                for mp in a.iter().skip(1) {
                    let mp_bounds = mp.get_bounds();
                    first_mp_bounds.union_with(&mp_bounds);
                }

                first_mp_bounds
            }
            Self::MultiPolygon(a) => a.get_bounds(),
            Self::MultiShape(a) => {
                let mut first_mp_bounds = match a.get(0) {
                    Some(s) => s.get_bounds(),
                    None => return SvgRect::default(),
                };
                for mp in a.iter().skip(1) {
                    let mp_bounds = mp.get_bounds();
                    first_mp_bounds.union_with(&mp_bounds);
                }

                first_mp_bounds
            }
            Self::Path(a) => a.get_bounds(),
            Self::Circle(a) => a.get_bounds(),
            Self::Rect(a) => *a,
        }
    }
    /// Returns `true` if all sub-paths in this node are closed.
    #[must_use] pub fn is_closed(&self) -> bool {
        match self {
            Self::MultiPolygonCollection(a) => {
                for mp in a {
                    for p in mp.rings.as_ref() {
                        if !p.is_closed() {
                            return false;
                        }
                    }
                }

                true
            }
            Self::MultiPolygon(a) => {
                for p in a.rings.as_ref() {
                    if !p.is_closed() {
                        return false;
                    }
                }

                true
            }
            Self::MultiShape(a) => {
                for p in a.as_ref() {
                    if !p.is_closed() {
                        return false;
                    }
                }

                true
            }
            Self::Path(a) => a.is_closed(),
            Self::Circle(_) | Self::Rect(_) => true,
        }
    }
}

/// An SVG node paired with its visual style (fill or stroke).
#[derive(Debug, Clone, PartialOrd, PartialEq)]
#[repr(C)]
pub struct SvgStyledNode {
    pub geometry: SvgNode,
    pub style: SvgStyle,
}

/// A 2D vertex used in tessellated SVG geometry.
#[derive(Debug, Copy, Clone, PartialOrd, PartialEq)]
#[repr(C)]
pub struct SvgVertex {
    pub x: f32,
    pub y: f32,
}

impl_option!(
    SvgVertex,
    OptionSvgVertex,
    [Debug, Copy, Clone, PartialOrd, PartialEq]
);

impl VertexLayoutDescription for SvgVertex {
    fn get_description() -> VertexLayout {
        VertexLayout {
            fields: vec![VertexAttribute {
                va_name: String::from("vAttrXY").into(),
                layout_location: None.into(),
                attribute_type: VertexAttributeType::Float,
                item_count: 2,
            }]
            .into(),
        }
    }
}

/// A 3D vertex with per-vertex RGBA color, used in multi-colored SVG tessellation.
#[derive(Debug, Copy, Clone, PartialOrd, PartialEq)]
#[repr(C)]
pub struct SvgColoredVertex {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

impl_option!(
    SvgColoredVertex,
    OptionSvgColoredVertex,
    [Debug, Copy, Clone, PartialOrd, PartialEq]
);

impl VertexLayoutDescription for SvgColoredVertex {
    fn get_description() -> VertexLayout {
        VertexLayout {
            fields: vec![
                VertexAttribute {
                    va_name: String::from("vAttrXY").into(),
                    layout_location: None.into(),
                    attribute_type: VertexAttributeType::Float,
                    item_count: 3,
                },
                VertexAttribute {
                    va_name: String::from("vColor").into(),
                    layout_location: None.into(),
                    attribute_type: VertexAttributeType::Float,
                    item_count: 4,
                },
            ]
            .into(),
        }
    }
}

/// A circle defined by center coordinates and radius.
#[derive(Debug, Copy, Clone, PartialOrd, PartialEq)]
#[repr(C)]
pub struct SvgCircle {
    pub center_x: f32,
    pub center_y: f32,
    pub radius: f32,
}

impl SvgCircle {
    /// Returns `true` if the given point lies inside the circle.
    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
    #[must_use] pub fn contains_point(&self, x: f32, y: f32) -> bool {
        let x_diff = libm::fabsf(x - self.center_x);
        let y_diff = libm::fabsf(y - self.center_y);
        (x_diff * x_diff) + (y_diff * y_diff) < (self.radius * self.radius)
    }
    /// Returns the axis-aligned bounding rectangle of this circle.
    #[must_use] pub fn get_bounds(&self) -> SvgRect {
        SvgRect {
            width: self.radius * 2.0,
            height: self.radius * 2.0,
            x: self.center_x - self.radius,
            y: self.center_y - self.radius,
            radius_top_left: 0.0,
            radius_top_right: 0.0,
            radius_bottom_left: 0.0,
            radius_bottom_right: 0.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct TessellatedSvgNode {
    pub vertices: SvgVertexVec,
    pub indices: U32Vec,
}

impl_option!(
    TessellatedSvgNode,
    OptionTessellatedSvgNode,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd]
);

impl Default for TessellatedSvgNode {
    fn default() -> Self {
        Self {
            vertices: Vec::new().into(),
            indices: Vec::new().into(),
        }
    }
}

impl_vec!(TessellatedSvgNode, TessellatedSvgNodeVec, TessellatedSvgNodeVecDestructor, TessellatedSvgNodeVecDestructorType, TessellatedSvgNodeVecSlice, OptionTessellatedSvgNode);
impl_vec_debug!(TessellatedSvgNode, TessellatedSvgNodeVec);
impl_vec_partialord!(TessellatedSvgNode, TessellatedSvgNodeVec);
impl_vec_clone!(
    TessellatedSvgNode,
    TessellatedSvgNodeVec,
    TessellatedSvgNodeVecDestructor
);
impl_vec_partialeq!(TessellatedSvgNode, TessellatedSvgNodeVec);

impl TessellatedSvgNode {
    #[must_use] pub fn empty() -> Self {
        Self::default()
    }
}

impl TessellatedSvgNodeVec {
    #[must_use] pub fn get_ref(&self) -> TessellatedSvgNodeVecRef {
        let slice = self.as_ref();
        TessellatedSvgNodeVecRef {
            ptr: slice.as_ptr(),
            len: slice.len(),
        }
    }
}

impl fmt::Debug for TessellatedSvgNodeVecRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
    #[must_use] pub const fn as_slice(&self) -> &[TessellatedSvgNode] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct TessellatedColoredSvgNode {
    pub vertices: SvgColoredVertexVec,
    pub indices: U32Vec,
}

impl_option!(
    TessellatedColoredSvgNode,
    OptionTessellatedColoredSvgNode,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd]
);

impl Default for TessellatedColoredSvgNode {
    fn default() -> Self {
        Self {
            vertices: Vec::new().into(),
            indices: Vec::new().into(),
        }
    }
}

impl_vec!(TessellatedColoredSvgNode, TessellatedColoredSvgNodeVec, TessellatedColoredSvgNodeVecDestructor, TessellatedColoredSvgNodeVecDestructorType, TessellatedColoredSvgNodeVecSlice, OptionTessellatedColoredSvgNode);
impl_vec_debug!(TessellatedColoredSvgNode, TessellatedColoredSvgNodeVec);
impl_vec_partialord!(TessellatedColoredSvgNode, TessellatedColoredSvgNodeVec);
impl_vec_clone!(
    TessellatedColoredSvgNode,
    TessellatedColoredSvgNodeVec,
    TessellatedColoredSvgNodeVecDestructor
);
impl_vec_partialeq!(TessellatedColoredSvgNode, TessellatedColoredSvgNodeVec);

impl TessellatedColoredSvgNode {
    #[must_use] pub fn empty() -> Self {
        Self::default()
    }
}

impl TessellatedColoredSvgNodeVec {
    #[must_use] pub fn get_ref(&self) -> TessellatedColoredSvgNodeVecRef {
        let slice = self.as_ref();
        TessellatedColoredSvgNodeVecRef {
            ptr: slice.as_ptr(),
            len: slice.len(),
        }
    }
}

impl fmt::Debug for TessellatedColoredSvgNodeVecRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_slice().fmt(f)
    }
}

// C ABI wrapper over &[TessellatedColoredSvgNode]
#[repr(C)]
pub struct TessellatedColoredSvgNodeVecRef {
    pub ptr: *const TessellatedColoredSvgNode,
    pub len: usize,
}

impl Clone for TessellatedColoredSvgNodeVecRef {
    fn clone(&self) -> Self {
        Self {
            ptr: self.ptr,
            len: self.len,
        }
    }
}

impl TessellatedColoredSvgNodeVecRef {
    #[must_use] pub const fn as_slice(&self) -> &[TessellatedColoredSvgNode] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl_vec!(SvgVertex, SvgVertexVec, SvgVertexVecDestructor, SvgVertexVecDestructorType, SvgVertexVecSlice, OptionSvgVertex);
impl_vec_debug!(SvgVertex, SvgVertexVec);
impl_vec_partialord!(SvgVertex, SvgVertexVec);
impl_vec_clone!(SvgVertex, SvgVertexVec, SvgVertexVecDestructor);
impl_vec_partialeq!(SvgVertex, SvgVertexVec);

impl_vec!(SvgColoredVertex, SvgColoredVertexVec, SvgColoredVertexVecDestructor, SvgColoredVertexVecDestructorType, SvgColoredVertexVecSlice, OptionSvgColoredVertex);
impl_vec_debug!(SvgColoredVertex, SvgColoredVertexVec);
impl_vec_partialord!(SvgColoredVertex, SvgColoredVertexVec);
impl_vec_clone!(
    SvgColoredVertex,
    SvgColoredVertexVec,
    SvgColoredVertexVecDestructor
);
impl_vec_partialeq!(SvgColoredVertex, SvgColoredVertexVec);

/// Computes the bbox size and transform matrix uniforms shared by SVG draw methods.
///
/// Converts `StyleTransform` list into column-major `[f32; 16]` for OpenGL,
/// and packages it along with the bbox size uniform.
// target_size is physical pixel dimensions (u32); GL uniforms are f32. Pixel
// counts are always well within f32's exact-integer range (2^24), so the
// precision loss the lint warns about cannot occur for any real render target.
#[allow(clippy::cast_precision_loss)]
fn compute_svg_transform_uniforms(
    target_size: PhysicalSizeU32,
    transforms: &[StyleTransform],
) -> (Uniform, Uniform) {
    let transform_origin = StyleTransformOrigin {
        x: PixelValue::px(target_size.width as f32 / 2.0),
        y: PixelValue::px(target_size.height as f32 / 2.0),
    };

    let computed_transform = ComputedTransform3D::from_style_transform_vec(
        transforms,
        &transform_origin,
        target_size.width as f32,
        target_size.height as f32,
        RotationMode::ForWebRender,
    );

    // NOTE: OpenGL draws are column-major, while ComputedTransform3D
    // is row-major! Need to transpose the matrix!
    let m = computed_transform.get_column_major().m;
    let matrix: [f32; 16] = core::array::from_fn(|i| m[i / 4][i % 4]);

    let bbox_uniform = Uniform {
        uniform_name: "vBboxSize".into(),
        uniform_type: UniformType::FloatVec2([
            target_size.width as f32,
            target_size.height as f32,
        ]),
    };

    let transform_uniform = Uniform {
        uniform_name: "vTransformMatrix".into(),
        uniform_type: UniformType::Matrix4 {
            transpose: false,
            matrix,
        },
    };

    (bbox_uniform, transform_uniform)
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
#[repr(C)]
pub struct TessellatedGPUSvgNode {
    pub vertex_index_buffer: VertexBuffer,
}

impl TessellatedGPUSvgNode {
    /// Uploads the tesselated SVG node to GPU memory
    #[must_use] pub fn new(node: &TessellatedSvgNode, gl: GlContextPtr) -> Self {
        let svg_shader_id = gl.ptr.svg_shader;
        Self {
            vertex_index_buffer: VertexBuffer::new(
                gl,
                svg_shader_id,
                node.vertices.as_ref(),
                node.indices.as_ref(),
                IndexBufferFormat::Triangles,
            ),
        }
    }

    /// Draw the vertex buffer to the texture with the given color and transform
    #[allow(clippy::needless_pass_by_value)] // owned azul value taken by value (public API / ownership-transfer convention)
    pub fn draw(
        &self,
        texture: &mut Texture,
        target_size: PhysicalSizeU32,
        color: ColorU,
        transforms: StyleTransformVec,
    ) -> bool {
        let (bbox_uniform, transform_uniform) =
            compute_svg_transform_uniforms(target_size, transforms.as_ref());

        let color: ColorF = color.into();

        let uniforms = [
            bbox_uniform,
            Uniform {
                uniform_name: "fDrawColor".into(),
                uniform_type: UniformType::FloatVec4([color.r, color.g, color.b, color.a]),
            },
            transform_uniform,
        ];

        GlShader::draw(
            texture.gl_context.ptr.svg_shader,
            texture,
            &[(&self.vertex_index_buffer, &uniforms[..])],
        );

        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd)]
#[repr(C)]
pub struct TessellatedColoredGPUSvgNode {
    pub vertex_index_buffer: VertexBuffer,
}

impl TessellatedColoredGPUSvgNode {
    /// Uploads the tesselated SVG node to GPU memory
    #[must_use] pub fn new(node: &TessellatedColoredSvgNode, gl: GlContextPtr) -> Self {
        let svg_shader_id = gl.ptr.svg_multicolor_shader;
        Self {
            vertex_index_buffer: VertexBuffer::new(
                gl,
                svg_shader_id,
                node.vertices.as_ref(),
                node.indices.as_ref(),
                IndexBufferFormat::Triangles,
            ),
        }
    }

    /// Draw the vertex buffer to the texture with the given color and transform
    #[allow(clippy::needless_pass_by_value)] // owned azul value taken by value (public API / ownership-transfer convention)
    pub fn draw(
        &self,
        texture: &mut Texture,
        target_size: PhysicalSizeU32,
        transforms: StyleTransformVec,
    ) -> bool {
        let (bbox_uniform, transform_uniform) =
            compute_svg_transform_uniforms(target_size, transforms.as_ref());

        // two separately-named GL uniforms collected into the draw-call array;
        // not a tuple->array conversion.
        #[allow(clippy::tuple_array_conversions)]
        let uniforms = [bbox_uniform, transform_uniform];

        GlShader::draw(
            texture.gl_context.ptr.svg_multicolor_shader,
            texture,
            &[(&self.vertex_index_buffer, &uniforms[..])],
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
    #[must_use] pub const fn get_antialias(&self) -> bool {
        match self {
            Self::Fill(f) => f.anti_alias,
            Self::Stroke(s) => s.anti_alias,
        }
    }
    #[must_use] pub const fn get_high_quality_aa(&self) -> bool {
        match self {
            Self::Fill(f) => f.high_quality_aa,
            Self::Stroke(s) => s.high_quality_aa,
        }
    }
    #[must_use] pub const fn get_transform(&self) -> SvgTransform {
        match self {
            Self::Fill(f) => f.transform,
            Self::Stroke(s) => s.transform,
        }
    }
}
/// SVG fill rule for determining the interior of a shape.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd)]
#[repr(C)]
#[derive(Default)]
pub enum SvgFillRule {
    #[default]
    Winding,
    EvenOdd,
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
    /// Whether to apply a transform to the points in the path (warning: will be done on the CPU -
    /// expensive)
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
    /// Dash pattern
    pub dash_pattern: OptionSvgDashPattern,
    /// Whether to apply a transform to the points in the path (warning: will be done on the CPU -
    /// expensive)
    pub transform: SvgTransform,
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
    /// Whether the fill is intended to be anti-aliased (default: true)
    pub anti_alias: bool,
    /// Whether the anti-aliasing has to be of high quality (default: false)
    pub high_quality_aa: bool,
}

impl Default for SvgStrokeStyle {
    fn default() -> Self {
        Self {
            dash_pattern: OptionSvgDashPattern::None,
            transform: SvgTransform::default(),
            start_cap: SvgLineCap::default(),
            end_cap: SvgLineCap::default(),
            line_join: SvgLineJoin::default(),
            line_width: DEFAULT_LINE_WIDTH,
            miter_limit: DEFAULT_MITER_LIMIT,
            tolerance: DEFAULT_TOLERANCE,
            apply_line_width: true,
            anti_alias: true,
            high_quality_aa: false,
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

impl_option!(
    SvgDashPattern,
    OptionSvgDashPattern,
    [Debug, Copy, Clone, PartialEq, PartialOrd]
);

/// The shape used at the end of open sub-paths when they are stroked.
#[derive(Debug, Copy, Clone, PartialEq, Hash, Eq, PartialOrd, Ord)]
#[repr(C)]
#[derive(Default)]
pub enum SvgLineCap {
    #[default]
    Butt,
    Square,
    Round,
}


/// The shape used at the corners of stroked paths.
#[derive(Debug, Copy, Clone, PartialEq, Hash, Eq, PartialOrd, Ord)]
#[repr(C)]
#[derive(Default)]
pub enum SvgLineJoin {
    #[default]
    Miter,
    MiterClip,
    Round,
    Bevel,
}


pub use core::ffi::c_void;

#[derive(Debug, Clone)]
#[repr(C)]
pub struct SvgXmlNode {
    pub node: *const c_void, // usvg::Node
    pub run_destructor: bool,
}

#[derive(Debug, Clone)]
#[repr(C)]
pub struct Svg {
    pub tree: *const c_void, // *mut usvg::Tree,
    pub run_destructor: bool,
}

/// SVG `shape-rendering` property controlling quality vs speed tradeoffs.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub enum ShapeRendering {
    OptimizeSpeed,
    CrispEdges,
    GeometricPrecision,
}

/// SVG `image-rendering` property controlling image quality vs speed.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub enum ImageRendering {
    OptimizeQuality,
    OptimizeSpeed,
}

/// SVG `text-rendering` property controlling text quality vs speed.
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Eq, Ord, Hash)]
#[repr(C)]
pub enum TextRendering {
    OptimizeSpeed,
    OptimizeLegibility,
    GeometricPrecision,
}

/// Font database source for SVG text rendering.
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
    pub transform: SvgRenderTransform,
}

#[derive(Debug, Default, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgRenderTransform {
    pub sx: f32,
    pub kx: f32,
    pub ky: f32,
    pub sy: f32,
    pub tx: f32,
    pub ty: f32,
}

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C, u8)]
#[derive(Default)]
pub enum SvgFitTo {
    #[default]
    Original,
    Width(u32),
    Height(u32),
    Zoom(f32),
}


#[derive(Debug, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct SvgParseOptions {
    /// SVG image path. Used to resolve relative image paths.
    pub relative_image_path: OptionString,
    /// Default font family. Will be used when no font-family attribute is set in the SVG. Default:
    /// Times New Roman
    pub default_font_family: AzString,
    /// A list of languages. Will be used to resolve a systemLanguage conditional attribute.
    /// Format: en, en-US. Default: [en]
    pub languages: StringVec,
    /// Target DPI. Impact units conversion. Default: 96.0
    pub dpi: f32,
    /// A default font size. Will be used when no font-size attribute is set in the SVG. Default:
    /// 12
    pub font_size: f32,
    /// Specifies the default shape rendering method. Will be used when an SVG element's
    /// shape-rendering property is set to auto. Default: `GeometricPrecision`
    pub shape_rendering: ShapeRendering,
    /// Specifies the default text rendering method. Will be used when an SVG element's
    /// text-rendering property is set to auto. Default: `OptimizeLegibility`
    pub text_rendering: TextRendering,
    /// Specifies the default image rendering method. Will be used when an SVG element's
    /// image-rendering property is set to auto. Default: `OptimizeQuality`
    pub image_rendering: ImageRendering,
    /// When empty, text elements will be skipped. Default: `System`
    pub fontdb: FontDatabase,
    /// Keep named groups. If set to true, all non-empty groups with id attribute will not be
    /// removed. Default: false
    pub keep_named_groups: bool,
}

impl Default for SvgParseOptions {
    fn default() -> Self {
        let lang_vec: Vec<AzString> = vec![String::from("en").into()];
        Self {
            relative_image_path: OptionString::None,
            default_font_family: "Times New Roman".to_string().into(),
            languages: lang_vec.into(),
            dpi: 96.0,
            font_size: 12.0,
            shape_rendering: ShapeRendering::GeometricPrecision,
            text_rendering: TextRendering::OptimizeLegibility,
            image_rendering: ImageRendering::OptimizeQuality,
            fontdb: FontDatabase::System,
            keep_named_groups: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd)]
#[repr(C)]
pub struct SvgXmlOptions {
    pub use_single_quote: bool,
    pub indent: Indent,
    pub attributes_indent: Indent,
}

impl Default for SvgXmlOptions {
    fn default() -> Self {
        Self {
            use_single_quote: false,
            indent: Indent::Spaces(2),
            attributes_indent: Indent::Spaces(2),
        }
    }
}
#[allow(variant_size_differences)] // repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size disparity accepted
#[derive(Debug, PartialEq, Eq, PartialOrd, Clone)]
#[repr(C, u8)]
pub enum SvgParseError {
    NoParserAvailable,
    ElementsLimitReached,
    NotAnUtf8Str,
    MalformedGZip,
    InvalidSize,
    ParsingFailed(XmlError),
}

impl fmt::Display for SvgParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use self::SvgParseError::{NoParserAvailable, ElementsLimitReached, NotAnUtf8Str, MalformedGZip, InvalidSize, ParsingFailed};
        match self {
            NoParserAvailable => write!(
                f,
                "Library was compiled without SVG support (no parser available)"
            ),
            ElementsLimitReached => write!(f, "Error parsing SVG: Elements limit reached"),
            NotAnUtf8Str => write!(f, "Error parsing SVG: Not an UTF-8 String"),
            MalformedGZip => write!(
                f,
                "Error parsing SVG: SVG is compressed with a malformed GZIP compression"
            ),
            InvalidSize => write!(f, "Error parsing SVG: Invalid size"),
            ParsingFailed(e) => write!(f, "Error parsing SVG: Parsing SVG as XML failed: {e}"),
        }
    }
}

impl_result!(
    SvgXmlNode,
    SvgParseError,
    ResultSvgXmlNodeSvgParseError,
    copy = false,
    [Debug, Clone]
);
impl_result!(
    Svg,
    SvgParseError,
    ResultSvgSvgParseError,
    copy = false,
    [Debug, Clone]
);

/// Indentation style for SVG XML serialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd)]
#[repr(C, u8)]
pub enum Indent {
    None,
    Spaces(u8),
    Tabs,
}

#[cfg(test)]
#[allow(clippy::pedantic, clippy::nursery, clippy::float_cmp)]
mod autotest_generated {
    use super::*;

    // ---------------------------------------------------------------- helpers

    fn pt(x: f32, y: f32) -> SvgPoint {
        SvgPoint { x, y }
    }

    fn line_el(x1: f32, y1: f32, x2: f32, y2: f32) -> SvgPathElement {
        SvgPathElement::line(SvgLine::new(pt(x1, y1), pt(x2, y2)))
    }

    fn make_path(items: Vec<SvgPathElement>) -> SvgPath {
        SvgPath::create(SvgPathElementVec::from_vec(items))
    }

    fn quad() -> SvgQuadraticCurve {
        SvgQuadraticCurve {
            start: pt(0.0, 0.0),
            ctrl: pt(5.0, 10.0),
            end: pt(10.0, 0.0),
        }
    }

    fn cubic() -> SvgCubicCurve {
        SvgCubicCurve {
            start: pt(0.0, 0.0),
            ctrl_1: pt(0.0, 10.0),
            ctrl_2: pt(10.0, 10.0),
            end: pt(10.0, 0.0),
        }
    }

    /// `true` if `outer` fully contains `inner` (used to check bounding-box invariants).
    fn rect_contains(outer: &SvgRect, inner: &SvgRect) -> bool {
        outer.x <= inner.x
            && outer.y <= inner.y
            && outer.x + outer.width >= inner.x + inner.width
            && outer.y + outer.height >= inner.y + inner.height
    }

    // ------------------------------------------------------- SvgLine :: basic

    #[test]
    fn svgline_new_keeps_fields_including_extreme_values() {
        let l = SvgLine::new(pt(1.0, 2.0), pt(3.0, 4.0));
        assert_eq!(l.get_start(), pt(1.0, 2.0));
        assert_eq!(l.get_end(), pt(3.0, 4.0));

        let extreme = SvgLine::new(pt(f32::MIN, f32::MAX), pt(f32::MAX, f32::MIN));
        assert_eq!(extreme.start.x, f32::MIN);
        assert_eq!(extreme.end.x, f32::MAX);

        // NaN survives construction untouched (no normalization happens)
        let nan = SvgLine::new(pt(f32::NAN, 0.0), pt(0.0, f32::NAN));
        assert!(nan.get_start().x.is_nan());
        assert!(nan.get_end().y.is_nan());
    }

    #[test]
    fn svgline_reverse_is_an_involution() {
        let orig = SvgLine::new(pt(-1.5, 2.5), pt(7.0, -9.0));
        let mut l = orig;
        l.reverse();
        assert_eq!(l.get_start(), orig.get_end());
        assert_eq!(l.get_end(), orig.get_start());
        l.reverse();
        assert_eq!(l, orig);
    }

    #[test]
    fn svgline_reverse_does_not_panic_on_extreme_values() {
        let mut l = SvgLine::new(pt(f32::INFINITY, f32::NAN), pt(f32::NEG_INFINITY, f32::MAX));
        l.reverse();
        assert!(l.get_start().x.is_infinite() && l.get_start().x < 0.0);
        assert!(l.get_end().y.is_nan());
    }

    // ----------------------------------------------------- SvgLine :: normals

    #[test]
    fn svgline_inwards_normal_is_unit_length_and_90deg_right() {
        // horizontal line pointing +x -> normal points -y (90deg to the right in SVG coords)
        let l = SvgLine::new(pt(0.0, 0.0), pt(10.0, 0.0));
        let n = l.inwards_normal().expect("non-degenerate line has a normal");
        assert!((n.x - 0.0).abs() < 1e-6);
        assert!((n.y - 1.0).abs() < 1e-6);
        let len = (n.x * n.x + n.y * n.y).sqrt();
        assert!((len - 1.0).abs() < 1e-6, "normal must be unit length");
    }

    #[test]
    fn svgline_outwards_normal_is_the_negated_inwards_normal() {
        let l = SvgLine::new(pt(3.0, -4.0), pt(-7.0, 11.0));
        let i = l.inwards_normal().expect("non-degenerate line has a normal");
        let o = l.outwards_normal().expect("non-degenerate line has a normal");
        assert!((i.x + o.x).abs() < 1e-6);
        assert!((i.y + o.y).abs() < 1e-6);
    }

    #[test]
    fn svgline_normals_are_none_for_zero_length_line() {
        // division by a zero edge length must not produce a bogus point
        let l = SvgLine::new(pt(5.0, 5.0), pt(5.0, 5.0));
        assert_eq!(l.inwards_normal(), None);
        assert_eq!(l.outwards_normal(), None);
    }

    #[test]
    fn svgline_normals_are_none_for_nan_and_infinite_coords() {
        let nan = SvgLine::new(pt(f32::NAN, 0.0), pt(1.0, 1.0));
        assert_eq!(nan.inwards_normal(), None);
        assert_eq!(nan.outwards_normal(), None);

        // dy/hypot == inf/inf == NaN -> not finite -> None
        let inf = SvgLine::new(pt(f32::NEG_INFINITY, f32::NEG_INFINITY), pt(1.0, 1.0));
        assert_eq!(inf.inwards_normal(), None);
        assert_eq!(inf.outwards_normal(), None);
    }

    #[test]
    fn svgline_inwards_normal_on_overflowing_line_stays_defined() {
        // dx/dy overflow f32 -> hypot is +inf; the result must be either None
        // or finite, never a silent NaN/inf leaking into the point.
        let l = SvgLine::new(pt(-f32::MAX, -f32::MAX), pt(f32::MAX, f32::MAX));
        match l.inwards_normal() {
            None => {}
            Some(n) => assert!(
                n.x.is_finite() && n.y.is_finite(),
                "inwards_normal returned a non-finite point: {n:?}"
            ),
        }
    }

    // ---------------------------------------------------- SvgLine :: numerics

    #[test]
    fn svgline_get_x_y_at_t_hit_the_endpoints_exactly() {
        let l = SvgLine::new(pt(2.0, -3.0), pt(12.0, 17.0));
        assert_eq!(l.get_x_at_t(0.0), 2.0);
        assert_eq!(l.get_y_at_t(0.0), -3.0);
        assert_eq!(l.get_x_at_t(1.0), 12.0);
        assert_eq!(l.get_y_at_t(1.0), 17.0);
        assert!((l.get_x_at_t(0.5) - 7.0).abs() < 1e-9);
        assert!((l.get_y_at_t(0.5) - 7.0).abs() < 1e-9);
    }

    #[test]
    fn svgline_get_x_at_t_extrapolates_for_out_of_range_t() {
        // t is NOT clamped to [0, 1] - document the extrapolating behaviour
        let l = SvgLine::new(pt(0.0, 0.0), pt(10.0, 10.0));
        assert!((l.get_x_at_t(-1.0) - -10.0).abs() < 1e-9);
        assert!((l.get_y_at_t(2.0) - 20.0).abs() < 1e-9);
    }

    #[test]
    fn svgline_get_x_y_at_t_nan_and_inf_do_not_panic() {
        let l = SvgLine::new(pt(0.0, 0.0), pt(10.0, 10.0));
        assert!(l.get_x_at_t(f64::NAN).is_nan());
        assert!(l.get_y_at_t(f64::NAN).is_nan());
        assert!(l.get_x_at_t(f64::INFINITY).is_infinite());
        assert!(l.get_y_at_t(f64::NEG_INFINITY).is_infinite());

        // 0-length in x: (end.x - start.x) == 0, so 0 * inf == NaN
        let vertical = SvgLine::new(pt(4.0, 0.0), pt(4.0, 10.0));
        assert!(vertical.get_x_at_t(f64::INFINITY).is_nan());
    }

    #[test]
    fn svgline_get_x_at_t_at_f32_extremes_saturates_to_inf_not_panic() {
        let l = SvgLine::new(pt(-f32::MAX, 0.0), pt(f32::MAX, 0.0));
        // f64 has the range to hold 2 * f32::MAX, so this must stay finite
        assert!(l.get_x_at_t(0.5).abs() < 1e-9);
        assert!(l.get_x_at_t(1.0).is_finite());
        assert!(l.get_x_at_t(f64::MAX).is_infinite());
    }

    #[test]
    fn svgline_get_length_is_euclidean_and_direction_independent() {
        let l = SvgLine::new(pt(0.0, 0.0), pt(3.0, 4.0));
        assert!((l.get_length() - 5.0).abs() < 1e-6);
        let mut r = l;
        r.reverse();
        assert!((r.get_length() - l.get_length()).abs() < 1e-9);
        assert_eq!(SvgLine::new(pt(1.0, 1.0), pt(1.0, 1.0)).get_length(), 0.0);
    }

    #[test]
    fn svgline_get_length_overflow_and_nan_are_defined() {
        // dx overflows f32 -> +inf, hypot(+inf, 0) == +inf
        let huge = SvgLine::new(pt(-f32::MAX, 0.0), pt(f32::MAX, 0.0));
        let len = huge.get_length();
        assert!(len.is_infinite() && len > 0.0);

        let nan = SvgLine::new(pt(f32::NAN, 0.0), pt(0.0, 0.0));
        assert!(nan.get_length().is_nan());
    }

    #[test]
    fn svgline_get_t_at_offset_maps_arc_length_to_t() {
        let l = SvgLine::new(pt(0.0, 0.0), pt(10.0, 0.0));
        assert_eq!(l.get_t_at_offset(0.0), 0.0);
        assert!((l.get_t_at_offset(5.0) - 0.5).abs() < 1e-6);
        assert!((l.get_t_at_offset(10.0) - 1.0).abs() < 1e-6);
        // negative + past-the-end offsets are NOT clamped
        assert!((l.get_t_at_offset(-5.0) + 0.5).abs() < 1e-6);
        assert!((l.get_t_at_offset(20.0) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn svgline_get_t_at_offset_on_zero_length_line_is_nan_or_inf_not_a_panic() {
        // offset / 0.0 -- must not panic; 0/0 is NaN, x/0 is +-inf
        let l = SvgLine::new(pt(1.0, 1.0), pt(1.0, 1.0));
        assert!(l.get_t_at_offset(0.0).is_nan());
        assert!(l.get_t_at_offset(5.0).is_infinite());
        assert!(l.get_t_at_offset(-5.0).is_infinite());
        assert!(l.get_t_at_offset(f64::NAN).is_nan());
    }

    #[test]
    fn svgline_get_t_at_offset_round_trips_through_get_x_at_t() {
        let l = SvgLine::new(pt(2.0, 2.0), pt(2.0, 12.0));
        let t = l.get_t_at_offset(l.get_length());
        assert!((l.get_y_at_t(t) - 12.0).abs() < 1e-6);
        assert!((l.get_x_at_t(t) - 2.0).abs() < 1e-6);
    }

    #[test]
    fn svgline_tangent_vector_is_normalized_and_zero_for_degenerate_lines() {
        let l = SvgLine::new(pt(0.0, 0.0), pt(0.0, 5.0));
        let t = l.get_tangent_vector_at_t();
        assert!((t.x - 0.0).abs() < 1e-9);
        assert!((t.y - 1.0).abs() < 1e-9);

        // normalize() defines the zero-length case as the zero vector
        let degenerate = SvgLine::new(pt(3.0, 3.0), pt(3.0, 3.0));
        let t = degenerate.get_tangent_vector_at_t();
        assert_eq!(t, SvgVector { x: 0.0, y: 0.0 });
    }

    // ------------------------------------------------------ SvgLine :: bounds

    #[test]
    fn svgline_get_bounds_is_orientation_independent() {
        let l = SvgLine::new(pt(10.0, 20.0), pt(-5.0, 4.0));
        let mut r = l;
        r.reverse();
        assert_eq!(l.get_bounds(), r.get_bounds());

        let b = l.get_bounds();
        assert_eq!(b.x, -5.0);
        assert_eq!(b.y, 4.0);
        assert_eq!(b.width, 15.0);
        assert_eq!(b.height, 16.0);
        assert_eq!(b.radius_top_left, 0.0);
    }

    #[test]
    fn svgline_get_bounds_of_degenerate_line_is_zero_sized() {
        let b = SvgLine::new(pt(7.0, 8.0), pt(7.0, 8.0)).get_bounds();
        assert_eq!((b.x, b.y, b.width, b.height), (7.0, 8.0, 0.0, 0.0));
    }

    #[test]
    fn svgline_get_bounds_overflows_to_inf_width_without_panicking() {
        // max_x - min_x overflows f32 -> +inf rather than a wrapped/negative width
        let b = SvgLine::new(pt(-f32::MAX, 0.0), pt(f32::MAX, 1.0)).get_bounds();
        assert!(b.width.is_infinite() && b.width > 0.0);
        assert_eq!(b.x, -f32::MAX);
        assert_eq!(b.height, 1.0);
    }

    // ------------------------------------------------- SvgPathElement :: ctors

    #[test]
    fn svgpathelement_constructors_wrap_the_right_variant() {
        let l = SvgLine::new(pt(0.0, 0.0), pt(1.0, 1.0));
        assert!(matches!(SvgPathElement::line(l), SvgPathElement::Line(_)));
        assert!(matches!(
            SvgPathElement::quadratic_curve(quad()),
            SvgPathElement::QuadraticCurve(_)
        ));
        assert!(matches!(
            SvgPathElement::cubic_curve(cubic()),
            SvgPathElement::CubicCurve(_)
        ));
    }

    #[test]
    fn svgpathelement_constructors_accept_extreme_geometry() {
        let l = SvgLine::new(pt(f32::NAN, f32::INFINITY), pt(f32::MIN, f32::MAX));
        let el = SvgPathElement::line(l);
        assert!(el.get_start().x.is_nan());
        assert_eq!(el.get_end().x, f32::MIN);
    }

    // ------------------------------------------- SvgPathElement :: set / get

    #[test]
    fn svgpathelement_set_first_last_round_trip_for_every_variant() {
        let a = pt(-1.0, -2.0);
        let b = pt(3.0, 4.0);

        for mut el in [
            SvgPathElement::line(SvgLine::new(pt(0.0, 0.0), pt(1.0, 1.0))),
            SvgPathElement::quadratic_curve(quad()),
            SvgPathElement::cubic_curve(cubic()),
        ] {
            el.set_first(a);
            el.set_last(b);
            assert_eq!(el.get_start(), a);
            assert_eq!(el.get_end(), b);
        }
    }

    #[test]
    fn svgpathelement_set_first_last_accept_zero_min_max_and_nan() {
        let mut el = SvgPathElement::cubic_curve(cubic());

        el.set_first(pt(0.0, 0.0));
        el.set_last(pt(0.0, 0.0));
        assert_eq!(el.get_start(), pt(0.0, 0.0));
        assert_eq!(el.get_end(), pt(0.0, 0.0));

        el.set_first(pt(f32::MIN, f32::MIN));
        el.set_last(pt(f32::MAX, f32::MAX));
        assert_eq!(el.get_start().x, f32::MIN);
        assert_eq!(el.get_end().x, f32::MAX);

        el.set_first(pt(f32::NAN, f32::NEG_INFINITY));
        assert!(el.get_start().x.is_nan());
        assert!(el.get_start().y.is_infinite());
    }

    #[test]
    fn svgpathelement_reverse_is_an_involution_for_every_variant() {
        for el in [
            SvgPathElement::line(SvgLine::new(pt(0.0, 0.0), pt(1.0, 1.0))),
            SvgPathElement::quadratic_curve(quad()),
            SvgPathElement::cubic_curve(cubic()),
        ] {
            let mut r = el;
            r.reverse();
            assert_eq!(r.get_start(), el.get_end());
            assert_eq!(r.get_end(), el.get_start());
            r.reverse();
            assert_eq!(r, el, "reverse() applied twice must be the identity");
        }
    }

    // --------------------------------------------- SvgPathElement :: numerics

    #[test]
    fn svgpathelement_get_length_is_non_negative_for_every_variant() {
        for el in [
            SvgPathElement::line(SvgLine::new(pt(0.0, 0.0), pt(3.0, 4.0))),
            SvgPathElement::quadratic_curve(quad()),
            SvgPathElement::cubic_curve(cubic()),
        ] {
            let len = el.get_length();
            assert!(len.is_finite() && len >= 0.0, "bad length: {len}");
        }
    }

    #[test]
    fn svgpathelement_get_x_y_at_t_hit_endpoints_for_every_variant() {
        for el in [
            SvgPathElement::line(SvgLine::new(pt(0.0, 0.0), pt(10.0, 0.0))),
            SvgPathElement::quadratic_curve(quad()),
            SvgPathElement::cubic_curve(cubic()),
        ] {
            assert!((el.get_x_at_t(0.0) - f64::from(el.get_start().x)).abs() < 1e-6);
            assert!((el.get_y_at_t(0.0) - f64::from(el.get_start().y)).abs() < 1e-6);
            assert!((el.get_x_at_t(1.0) - f64::from(el.get_end().x)).abs() < 1e-6);
            assert!((el.get_y_at_t(1.0) - f64::from(el.get_end().y)).abs() < 1e-6);
        }
    }

    #[test]
    fn svgpathelement_get_x_y_at_t_nan_inf_do_not_panic() {
        for el in [
            SvgPathElement::line(SvgLine::new(pt(0.0, 0.0), pt(10.0, 0.0))),
            SvgPathElement::quadratic_curve(quad()),
            SvgPathElement::cubic_curve(cubic()),
        ] {
            assert!(el.get_x_at_t(f64::NAN).is_nan());
            assert!(el.get_y_at_t(f64::NAN).is_nan());
            // +-inf must not panic; any non-panicking value is acceptable here
            let _ = el.get_x_at_t(f64::INFINITY);
            let _ = el.get_y_at_t(f64::NEG_INFINITY);
            let _ = el.get_x_at_t(f64::MIN);
            let _ = el.get_y_at_t(f64::MAX);
        }
    }

    #[test]
    fn svgpathelement_line_tangent_ignores_t_even_when_t_is_nan() {
        // SvgLine has a constant tangent, so the `t` argument is discarded.
        let el = SvgPathElement::line(SvgLine::new(pt(0.0, 0.0), pt(0.0, 8.0)));
        let at_half = el.get_tangent_vector_at_t(0.5);
        assert_eq!(el.get_tangent_vector_at_t(f64::NAN), at_half);
        assert_eq!(el.get_tangent_vector_at_t(f64::INFINITY), at_half);
        assert_eq!(at_half, SvgVector { x: 0.0, y: 1.0 });
    }

    #[test]
    fn svgpathelement_curve_tangent_at_nan_is_nan_not_a_panic() {
        let el = SvgPathElement::quadratic_curve(quad());
        let v = el.get_tangent_vector_at_t(f64::NAN);
        assert!(v.x.is_nan() && v.y.is_nan());
    }

    #[test]
    fn svgpathelement_curve_tangent_is_unit_length_in_range() {
        for el in [
            SvgPathElement::quadratic_curve(quad()),
            SvgPathElement::cubic_curve(cubic()),
        ] {
            for t in [0.0_f64, 0.25, 0.5, 0.75, 1.0] {
                let v = el.get_tangent_vector_at_t(t);
                let len = (v.x * v.x + v.y * v.y).sqrt();
                // either the zero vector (degenerate derivative) or unit length
                assert!(
                    len.abs() < 1e-9 || (len - 1.0).abs() < 1e-6,
                    "tangent at t={t} has length {len}"
                );
            }
        }
    }

    #[test]
    fn svgpathelement_curve_t_at_offset_saturates_at_1_past_the_end() {
        // the sampling loop never triggers for an out-of-range offset,
        // so it must fall through to the final t (== 1.0), not overshoot
        for el in [
            SvgPathElement::quadratic_curve(quad()),
            SvgPathElement::cubic_curve(cubic()),
        ] {
            assert!((el.get_t_at_offset(f64::MAX) - 1.0).abs() < 1e-9);
            assert!((el.get_t_at_offset(1.0e30) - 1.0).abs() < 1e-9);
            // NaN never compares greater, so it also falls through
            assert!((el.get_t_at_offset(f64::NAN) - 1.0).abs() < 1e-9);
        }
    }

    #[test]
    fn svgpathelement_curve_t_at_offset_is_monotonic_and_in_range() {
        let el = SvgPathElement::cubic_curve(cubic());
        let len = el.get_length();
        let t_quarter = el.get_t_at_offset(len * 0.25);
        let t_half = el.get_t_at_offset(len * 0.5);
        assert!(t_quarter.is_finite() && t_half.is_finite());
        assert!((0.0..=1.0).contains(&t_quarter), "t={t_quarter}");
        assert!((0.0..=1.0).contains(&t_half), "t={t_half}");
        assert!(t_quarter <= t_half, "t must not decrease with offset");
    }

    #[test]
    fn svgpathelement_curve_t_at_offset_negative_offset_is_non_positive() {
        let el = SvgPathElement::cubic_curve(cubic());
        let t = el.get_t_at_offset(-10.0);
        assert!(t.is_finite(), "negative offset produced {t}");
        assert!(t <= 0.0, "negative offset must not map to a forward t: {t}");
    }

    #[test]
    fn svgpathelement_get_bounds_contains_both_endpoints() {
        for el in [
            SvgPathElement::line(SvgLine::new(pt(-3.0, 2.0), pt(9.0, -4.0))),
            SvgPathElement::quadratic_curve(quad()),
            SvgPathElement::cubic_curve(cubic()),
        ] {
            let b = el.get_bounds();
            let (s, e) = (el.get_start(), el.get_end());
            assert!(b.width >= 0.0 && b.height >= 0.0);
            assert!(s.x >= b.x && s.x <= b.x + b.width);
            assert!(e.x >= b.x && e.x <= b.x + b.width);
            assert!(s.y >= b.y && s.y <= b.y + b.height);
            assert!(e.y >= b.y && e.y <= b.y + b.height);
        }
    }

    // -------------------------------------------------------------- SvgPath

    #[test]
    fn svgpath_empty_getters_are_none_and_bounds_are_default() {
        let p = make_path(Vec::new());
        assert_eq!(p.get_start(), None);
        assert_eq!(p.get_end(), None);
        assert!(!p.is_closed(), "an empty path is not closed");
        assert_eq!(p.get_bounds(), SvgRect::default());
    }

    #[test]
    fn svgpath_start_and_end_come_from_first_and_last_element() {
        let p = make_path(vec![
            line_el(0.0, 0.0, 1.0, 1.0),
            line_el(1.0, 1.0, 5.0, 5.0),
            line_el(5.0, 5.0, 9.0, 2.0),
        ]);
        assert_eq!(p.get_start(), Some(pt(0.0, 0.0)));
        assert_eq!(p.get_end(), Some(pt(9.0, 2.0)));
    }

    #[test]
    fn svgpath_close_makes_is_closed_true_and_is_idempotent() {
        let mut p = make_path(vec![
            line_el(0.0, 0.0, 10.0, 0.0),
            line_el(10.0, 0.0, 10.0, 10.0),
        ]);
        assert!(!p.is_closed());
        p.close();
        assert!(p.is_closed(), "close() must establish is_closed()");
        assert_eq!(p.items.len(), 3);
        assert_eq!(p.get_end(), p.get_start());

        // closing an already-closed path must not append anything
        p.close();
        assert_eq!(p.items.len(), 3);
    }

    #[test]
    fn svgpath_close_on_empty_path_is_a_noop() {
        let mut p = make_path(Vec::new());
        p.close();
        assert_eq!(p.items.len(), 0);
        assert!(!p.is_closed());
    }

    #[test]
    fn svgpath_close_with_nan_coords_appends_once_and_does_not_panic() {
        // NaN != NaN, so the "already closed?" check can never be satisfied.
        // close() must still terminate and append exactly one element.
        let mut p = make_path(vec![line_el(f32::NAN, 0.0, 10.0, 0.0)]);
        p.close();
        assert_eq!(p.items.len(), 2);
        assert!(!p.is_closed(), "a NaN start point can never compare equal");
    }

    #[test]
    fn svgpath_is_closed_for_single_degenerate_element() {
        let p = make_path(vec![line_el(4.0, 4.0, 4.0, 4.0)]);
        assert!(p.is_closed(), "start == end for the only element");

        let open = make_path(vec![line_el(4.0, 4.0, 5.0, 4.0)]);
        assert!(!open.is_closed());
    }

    #[test]
    fn svgpath_reverse_is_an_involution_and_swaps_endpoints() {
        let orig = make_path(vec![
            line_el(0.0, 0.0, 1.0, 1.0),
            SvgPathElement::cubic_curve(cubic()),
            line_el(10.0, 0.0, 20.0, 5.0),
        ]);

        let mut p = orig.clone();
        p.reverse();
        assert_eq!(p.get_start(), orig.get_end());
        assert_eq!(p.get_end(), orig.get_start());
        assert_eq!(p.items.len(), orig.items.len());

        p.reverse();
        assert_eq!(p, orig, "reverse() applied twice must be the identity");
    }

    #[test]
    fn svgpath_reverse_on_empty_path_does_not_panic() {
        let mut p = make_path(Vec::new());
        p.reverse();
        assert_eq!(p.items.len(), 0);
    }

    #[test]
    fn svgpath_join_with_interpolates_the_join_point() {
        let mut a = make_path(vec![line_el(0.0, 0.0, 10.0, 0.0)]);
        let b = make_path(vec![line_el(20.0, 10.0, 30.0, 10.0)]);

        assert_eq!(a.join_with(b), Some(()));
        assert_eq!(a.items.len(), 2);

        // join point is the midpoint of (10,0) and (20,10)
        let mid = pt(15.0, 5.0);
        assert_eq!(a.items.as_ref()[0].get_end(), mid);
        assert_eq!(a.items.as_ref()[1].get_start(), mid);
        assert_eq!(a.get_start(), Some(pt(0.0, 0.0)));
        assert_eq!(a.get_end(), Some(pt(30.0, 10.0)));
    }

    #[test]
    fn svgpath_join_with_empty_other_returns_none_and_leaves_self_intact() {
        let mut a = make_path(vec![line_el(0.0, 0.0, 10.0, 0.0)]);
        let before = a.clone();
        assert_eq!(a.join_with(make_path(Vec::new())), None);
        assert_eq!(a, before, "a failed join must not corrupt the receiver");
    }

    #[test]
    fn svgpath_join_with_on_empty_self_returns_none_without_underflow() {
        // `vec.len() - 1` would underflow on an empty receiver; the `?` on
        // `last()` must short-circuit first.
        let mut a = make_path(Vec::new());
        let b = make_path(vec![line_el(0.0, 0.0, 1.0, 1.0)]);
        assert_eq!(a.join_with(b), None);
        assert_eq!(a.items.len(), 0);
    }

    #[test]
    fn svgpath_join_with_extreme_coords_does_not_panic() {
        let mut a = make_path(vec![line_el(0.0, 0.0, f32::MAX, f32::MAX)]);
        let b = make_path(vec![line_el(-f32::MAX, -f32::MAX, 0.0, 0.0)]);
        assert_eq!(a.join_with(b), Some(()));
        // midpoint of MAX and -MAX must not overflow to inf
        let join = a.items.as_ref()[0].get_end();
        assert!(join.x.is_finite() && join.y.is_finite(), "join: {join:?}");
    }

    #[test]
    fn svgpath_get_bounds_unions_every_element() {
        let p = make_path(vec![
            line_el(0.0, 0.0, 10.0, 0.0),
            line_el(10.0, 0.0, 10.0, 20.0),
            line_el(10.0, 20.0, -5.0, -8.0),
        ]);
        let b = p.get_bounds();
        assert_eq!(b.x, -5.0);
        assert_eq!(b.y, -8.0);
        assert_eq!(b.width, 15.0);
        assert_eq!(b.height, 28.0);

        for el in p.items.as_ref() {
            assert!(
                rect_contains(&b, &el.get_bounds()),
                "path bounds must contain every element's bounds"
            );
        }
    }

    // ------------------------------------------------------ SvgMultiPolygon

    #[test]
    fn svgmultipolygon_empty_has_default_bounds() {
        let mp = SvgMultiPolygon::create(SvgPathVec::from_vec(Vec::new()));
        assert_eq!(mp.get_bounds(), SvgRect::default());
    }

    #[test]
    fn svgmultipolygon_bounds_union_all_rings() {
        let mp = SvgMultiPolygon::create(SvgPathVec::from_vec(vec![
            make_path(vec![line_el(0.0, 0.0, 10.0, 10.0)]),
            make_path(vec![line_el(-4.0, 30.0, 2.0, 33.0)]),
        ]));
        let b = mp.get_bounds();
        assert_eq!(b.x, -4.0);
        assert_eq!(b.y, 0.0);
        assert_eq!(b.width, 14.0);
        assert_eq!(b.height, 33.0);
    }

    #[test]
    fn svgmultipolygon_bounds_must_contain_geometry_after_an_empty_first_ring() {
        // BUG: get_bounds() seeds from rings[0].items[0]; when the FIRST ring is
        // empty it bails out to SvgRect::default() and silently drops every
        // later ring's geometry.
        let mp = SvgMultiPolygon::create(SvgPathVec::from_vec(vec![
            make_path(Vec::new()),
            make_path(vec![line_el(100.0, 100.0, 200.0, 200.0)]),
        ]));
        let b = mp.get_bounds();
        let expected = SvgRect {
            width: 100.0,
            height: 100.0,
            x: 100.0,
            y: 100.0,
            ..SvgRect::default()
        };
        assert!(
            rect_contains(&b, &expected),
            "bounds {b:?} must contain the geometry of the non-empty ring {expected:?}"
        );
    }

    // ------------------------------------------------------- SvgSimpleNode

    #[test]
    fn svgsimplenode_is_closed_per_variant() {
        let circle = SvgCircle {
            center_x: 0.0,
            center_y: 0.0,
            radius: 5.0,
        };
        assert!(SvgSimpleNode::Circle(circle).is_closed());
        assert!(SvgSimpleNode::CircleHole(circle).is_closed());
        assert!(SvgSimpleNode::Rect(SvgRect::default()).is_closed());
        assert!(SvgSimpleNode::RectHole(SvgRect::default()).is_closed());

        assert!(!SvgSimpleNode::Path(make_path(vec![line_el(0.0, 0.0, 1.0, 0.0)])).is_closed());
        assert!(SvgSimpleNode::Path(make_path(vec![line_el(2.0, 2.0, 2.0, 2.0)])).is_closed());
        // an empty path is not a closed shape
        assert!(!SvgSimpleNode::Path(make_path(Vec::new())).is_closed());
    }

    #[test]
    fn svgsimplenode_get_bounds_per_variant() {
        let circle = SvgCircle {
            center_x: 10.0,
            center_y: 10.0,
            radius: 2.0,
        };
        let b = SvgSimpleNode::Circle(circle).get_bounds();
        assert_eq!((b.x, b.y, b.width, b.height), (8.0, 8.0, 4.0, 4.0));
        assert_eq!(SvgSimpleNode::CircleHole(circle).get_bounds(), b);

        let rect = SvgRect {
            width: 3.0,
            height: 4.0,
            x: 1.0,
            y: 2.0,
            ..SvgRect::default()
        };
        assert_eq!(SvgSimpleNode::Rect(rect).get_bounds(), rect);
        assert_eq!(SvgSimpleNode::RectHole(rect).get_bounds(), rect);

        // empty path -> default bounds, no panic
        assert_eq!(
            SvgSimpleNode::Path(make_path(Vec::new())).get_bounds(),
            SvgRect::default()
        );
    }

    // ------------------------------------------------------------- SvgNode

    #[test]
    fn svgnode_get_bounds_of_empty_collections_is_default() {
        assert_eq!(
            SvgNode::MultiPolygonCollection(SvgMultiPolygonVec::from_vec(Vec::new())).get_bounds(),
            SvgRect::default()
        );
        assert_eq!(
            SvgNode::MultiShape(SvgSimpleNodeVec::from_vec(Vec::new())).get_bounds(),
            SvgRect::default()
        );
        assert_eq!(
            SvgNode::Path(make_path(Vec::new())).get_bounds(),
            SvgRect::default()
        );
        assert_eq!(
            SvgNode::MultiPolygon(SvgMultiPolygon::create(SvgPathVec::from_vec(Vec::new())))
                .get_bounds(),
            SvgRect::default()
        );
    }

    #[test]
    fn svgnode_is_closed_is_vacuously_true_for_empty_collections() {
        assert!(SvgNode::MultiPolygonCollection(SvgMultiPolygonVec::from_vec(Vec::new())).is_closed());
        assert!(SvgNode::MultiShape(SvgSimpleNodeVec::from_vec(Vec::new())).is_closed());
        assert!(
            SvgNode::MultiPolygon(SvgMultiPolygon::create(SvgPathVec::from_vec(Vec::new())))
                .is_closed()
        );
        // ... but an empty *path* is still open
        assert!(!SvgNode::Path(make_path(Vec::new())).is_closed());
    }

    #[test]
    fn svgnode_is_closed_false_when_any_subpath_is_open() {
        let open = make_path(vec![line_el(0.0, 0.0, 1.0, 0.0)]);
        let mut closed = make_path(vec![
            line_el(0.0, 0.0, 1.0, 0.0),
            line_el(1.0, 0.0, 1.0, 1.0),
        ]);
        closed.close();

        let mp = SvgMultiPolygon::create(SvgPathVec::from_vec(vec![closed.clone(), open.clone()]));
        assert!(!SvgNode::MultiPolygon(mp.clone()).is_closed());
        assert!(!SvgNode::MultiPolygonCollection(SvgMultiPolygonVec::from_vec(vec![mp])).is_closed());
        assert!(!SvgNode::MultiShape(SvgSimpleNodeVec::from_vec(vec![
            SvgSimpleNode::Path(open),
            SvgSimpleNode::Circle(SvgCircle {
                center_x: 0.0,
                center_y: 0.0,
                radius: 1.0,
            }),
        ]))
        .is_closed());

        let all_closed = SvgMultiPolygon::create(SvgPathVec::from_vec(vec![closed]));
        assert!(SvgNode::MultiPolygon(all_closed).is_closed());
    }

    #[test]
    fn svgnode_get_bounds_contains_all_children() {
        let a = make_path(vec![line_el(0.0, 0.0, 10.0, 10.0)]);
        let b = make_path(vec![line_el(100.0, 100.0, 200.0, 200.0)]);
        let node = SvgNode::MultiShape(SvgSimpleNodeVec::from_vec(vec![
            SvgSimpleNode::Path(a.clone()),
            SvgSimpleNode::Path(b.clone()),
        ]));
        let bounds = node.get_bounds();
        assert!(rect_contains(&bounds, &a.get_bounds()));
        assert!(rect_contains(&bounds, &b.get_bounds()));
    }

    #[test]
    fn svgnode_rect_and_circle_bounds_are_passthrough() {
        let rect = SvgRect {
            width: 5.0,
            height: 6.0,
            x: -1.0,
            y: -2.0,
            ..SvgRect::default()
        };
        assert_eq!(SvgNode::Rect(rect).get_bounds(), rect);
        assert!(SvgNode::Rect(rect).is_closed());

        let circle = SvgCircle {
            center_x: 0.0,
            center_y: 0.0,
            radius: 1.0,
        };
        assert_eq!(SvgNode::Circle(circle).get_bounds(), circle.get_bounds());
        assert!(SvgNode::Circle(circle).is_closed());
    }

    // ----------------------------------------------------------- SvgCircle

    #[test]
    fn svgcircle_contains_point_is_strict_and_correct() {
        let c = SvgCircle {
            center_x: 0.0,
            center_y: 0.0,
            radius: 1.0,
        };
        assert!(c.contains_point(0.0, 0.0));
        assert!(c.contains_point(0.5, 0.5));
        // exactly on the perimeter -> NOT contained (strict `<`)
        assert!(!c.contains_point(1.0, 0.0));
        assert!(!c.contains_point(0.0, -1.0));
        assert!(!c.contains_point(2.0, 0.0));
        assert!(!c.contains_point(-0.8, -0.8));
    }

    #[test]
    fn svgcircle_zero_radius_contains_nothing_not_even_its_center() {
        let c = SvgCircle {
            center_x: 3.0,
            center_y: 3.0,
            radius: 0.0,
        };
        assert!(!c.contains_point(3.0, 3.0));
        assert!(!c.contains_point(0.0, 0.0));
    }

    #[test]
    fn svgcircle_negative_radius_behaves_like_its_absolute_value() {
        // r*r discards the sign, so a negative radius is NOT treated as empty.
        // Documented here so a future fix has to update this test deliberately.
        let neg = SvgCircle {
            center_x: 0.0,
            center_y: 0.0,
            radius: -2.0,
        };
        let pos = SvgCircle {
            center_x: 0.0,
            center_y: 0.0,
            radius: 2.0,
        };
        assert_eq!(neg.contains_point(0.0, 0.0), pos.contains_point(0.0, 0.0));
        assert_eq!(neg.contains_point(1.9, 0.0), pos.contains_point(1.9, 0.0));
        assert_eq!(neg.contains_point(5.0, 0.0), pos.contains_point(5.0, 0.0));
    }

    #[test]
    fn svgcircle_contains_point_nan_and_inf_are_false_not_a_panic() {
        let c = SvgCircle {
            center_x: 0.0,
            center_y: 0.0,
            radius: 1.0,
        };
        // every comparison against NaN is false
        assert!(!c.contains_point(f32::NAN, 0.0));
        assert!(!c.contains_point(0.0, f32::NAN));
        assert!(!c.contains_point(f32::INFINITY, 0.0));
        assert!(!c.contains_point(f32::NEG_INFINITY, f32::NEG_INFINITY));

        let nan_r = SvgCircle {
            center_x: 0.0,
            center_y: 0.0,
            radius: f32::NAN,
        };
        assert!(!nan_r.contains_point(0.0, 0.0));
    }

    #[test]
    fn svgcircle_contains_point_at_f32_extremes_does_not_panic() {
        let c = SvgCircle {
            center_x: 0.0,
            center_y: 0.0,
            radius: f32::MAX,
        };
        // x_diff*x_diff overflows to +inf, which is never < radius^2 (+inf)
        assert!(!c.contains_point(f32::MAX, f32::MAX));
        assert!(c.contains_point(1.0, 1.0));
        assert!(!c.contains_point(f32::MIN, 0.0));
    }

    #[test]
    fn svgcircle_get_bounds_is_the_enclosing_square() {
        let c = SvgCircle {
            center_x: 5.0,
            center_y: -5.0,
            radius: 2.5,
        };
        let b = c.get_bounds();
        assert_eq!((b.x, b.y, b.width, b.height), (2.5, -7.5, 5.0, 5.0));
        // a point just inside the bbox's left edge is still outside the circle
        assert!(!c.contains_point(b.x + 0.1, b.y + b.height / 2.0));
        assert!(c.contains_point(c.center_x, c.center_y));
    }

    #[test]
    fn svgcircle_get_bounds_with_nan_and_huge_radius_does_not_panic() {
        let nan = SvgCircle {
            center_x: 0.0,
            center_y: 0.0,
            radius: f32::NAN,
        };
        let b = nan.get_bounds();
        assert!(b.width.is_nan() && b.x.is_nan());

        let huge = SvgCircle {
            center_x: 0.0,
            center_y: 0.0,
            radius: f32::MAX,
        };
        let b = huge.get_bounds();
        // radius * 2.0 overflows f32 -> +inf (saturates, does not wrap negative)
        assert!(b.width.is_infinite() && b.width > 0.0);
        assert_eq!(b.x, -f32::MAX);
    }

    // ------------------------------------------------ Tessellated* wrappers

    #[test]
    fn tessellated_svg_node_empty_is_a_neutral_value() {
        let e = TessellatedSvgNode::empty();
        assert!(e.vertices.as_ref().is_empty());
        assert!(e.indices.as_ref().is_empty());
        assert_eq!(e, TessellatedSvgNode::default());
    }

    #[test]
    fn tessellated_colored_svg_node_empty_is_a_neutral_value() {
        let e = TessellatedColoredSvgNode::empty();
        assert!(e.vertices.as_ref().is_empty());
        assert!(e.indices.as_ref().is_empty());
        assert_eq!(e, TessellatedColoredSvgNode::default());
    }

    #[test]
    fn tessellated_svg_node_vec_ref_round_trips_through_as_slice() {
        let node = TessellatedSvgNode {
            vertices: vec![SvgVertex { x: 1.0, y: 2.0 }].into(),
            indices: vec![0_u32].into(),
        };
        let v = TessellatedSvgNodeVec::from_vec(vec![node.clone(), TessellatedSvgNode::empty()]);
        let r = v.get_ref();
        assert_eq!(r.len, 2);
        let slice = r.as_slice();
        assert_eq!(slice.len(), 2);
        assert_eq!(slice[0], node);
        assert_eq!(slice[1], TessellatedSvgNode::empty());
    }

    #[test]
    fn tessellated_svg_node_vec_ref_on_empty_vec_yields_an_empty_slice() {
        // as_slice() calls slice::from_raw_parts - an empty vec must still
        // produce a valid (dangling but aligned) pointer, never a null deref.
        let v = TessellatedSvgNodeVec::from_vec(Vec::new());
        let r = v.get_ref();
        assert_eq!(r.len, 0);
        assert!(r.as_slice().is_empty());
        assert!(!r.ptr.is_null(), "raw ptr must never be null");
    }

    #[test]
    fn tessellated_colored_svg_node_vec_ref_round_trips_through_as_slice() {
        let node = TessellatedColoredSvgNode {
            vertices: vec![SvgColoredVertex {
                x: 1.0,
                y: 2.0,
                z: 3.0,
                r: 1.0,
                g: 0.5,
                b: 0.25,
                a: 1.0,
            }]
            .into(),
            indices: vec![0_u32, 1, 2].into(),
        };
        let v = TessellatedColoredSvgNodeVec::from_vec(vec![node.clone()]);
        let r = v.get_ref();
        assert_eq!(r.len, 1);
        assert_eq!(r.as_slice().len(), 1);
        assert_eq!(r.as_slice()[0], node);
    }

    #[test]
    fn tessellated_colored_svg_node_vec_ref_on_empty_vec_yields_an_empty_slice() {
        let v = TessellatedColoredSvgNodeVec::from_vec(Vec::new());
        let r = v.get_ref();
        assert_eq!(r.len, 0);
        assert!(r.as_slice().is_empty());
        assert!(!r.ptr.is_null(), "raw ptr must never be null");
    }

    // ------------------------------------- compute_svg_transform_uniforms

    fn matrix_of(u: &Uniform) -> [f32; 16] {
        match u.uniform_type {
            UniformType::Matrix4 { transpose, matrix } => {
                assert!(!transpose, "SVG shaders expect a pre-transposed matrix");
                matrix
            }
            _ => panic!("expected a Matrix4 uniform"),
        }
    }

    fn bbox_of(u: &Uniform) -> [f32; 2] {
        match u.uniform_type {
            UniformType::FloatVec2(v) => v,
            _ => panic!("expected a FloatVec2 uniform"),
        }
    }

    #[test]
    fn compute_svg_transform_uniforms_zero_size_no_transforms_is_finite() {
        let (bbox, tf) = compute_svg_transform_uniforms(
            PhysicalSizeU32 {
                width: 0,
                height: 0,
            },
            &[],
        );
        assert_eq!(bbox.uniform_name.as_str(), "vBboxSize");
        assert_eq!(bbox_of(&bbox), [0.0, 0.0]);

        assert_eq!(tf.uniform_name.as_str(), "vTransformMatrix");
        let m = matrix_of(&tf);
        assert!(
            m.iter().all(|f| f.is_finite()),
            "a zero-sized target must not produce NaN/inf in the matrix: {m:?}"
        );
    }

    #[test]
    fn compute_svg_transform_uniforms_at_u32_max_does_not_panic() {
        let (bbox, tf) = compute_svg_transform_uniforms(
            PhysicalSizeU32 {
                width: u32::MAX,
                height: u32::MAX,
            },
            &[],
        );
        let b = bbox_of(&bbox);
        assert!(b[0].is_finite() && b[0] > 0.0);
        assert_eq!(b[0], u32::MAX as f32);
        assert_eq!(b[1], u32::MAX as f32);
        let m = matrix_of(&tf);
        assert!(m.iter().all(|f| f.is_finite()), "matrix: {m:?}");
    }

    #[test]
    fn compute_svg_transform_uniforms_translation_changes_the_matrix() {
        let size = PhysicalSizeU32 {
            width: 800,
            height: 600,
        };
        let identity = matrix_of(&compute_svg_transform_uniforms(size, &[]).1);
        let translated = matrix_of(
            &compute_svg_transform_uniforms(
                size,
                &[
                    StyleTransform::TranslateX(PixelValue::px(10.0)),
                    StyleTransform::TranslateY(PixelValue::px(-20.0)),
                ],
            )
            .1,
        );
        assert!(
            translated.iter().all(|f| f.is_finite()),
            "matrix: {translated:?}"
        );
        assert!(
            identity != translated,
            "a translation must actually alter the transform matrix"
        );
    }

    #[test]
    fn compute_svg_transform_uniforms_extreme_translation_does_not_panic() {
        let size = PhysicalSizeU32 {
            width: 1,
            height: 1,
        };
        let (bbox, tf) = compute_svg_transform_uniforms(
            size,
            &[
                StyleTransform::TranslateX(PixelValue::px(f32::MAX)),
                StyleTransform::TranslateY(PixelValue::px(-f32::MAX)),
            ],
        );
        assert_eq!(bbox_of(&bbox), [1.0, 1.0]);
        // no assertion on finiteness here: the transform itself is degenerate,
        // we only require that building the uniform does not panic
        let _ = matrix_of(&tf);
    }

    // ------------------------------------------------------------ SvgStyle

    #[test]
    fn svgstyle_getters_read_through_to_both_variants() {
        let fill = SvgStyle::Fill(SvgFillStyle::default());
        assert!(fill.get_antialias());
        assert!(!fill.get_high_quality_aa());
        assert_eq!(fill.get_transform(), SvgTransform::default());

        let stroke = SvgStyle::Stroke(SvgStrokeStyle::default());
        assert!(stroke.get_antialias());
        assert!(!stroke.get_high_quality_aa());
        assert_eq!(stroke.get_transform(), SvgTransform::default());
    }

    #[test]
    fn svgstyle_getters_reflect_non_default_fields() {
        let transform = SvgTransform {
            sx: 2.0,
            kx: 0.5,
            ky: -0.5,
            sy: 3.0,
            tx: 10.0,
            ty: -10.0,
        };
        let fill = SvgStyle::Fill(SvgFillStyle {
            anti_alias: false,
            high_quality_aa: true,
            transform,
            ..SvgFillStyle::default()
        });
        assert!(!fill.get_antialias());
        assert!(fill.get_high_quality_aa());
        assert_eq!(fill.get_transform(), transform);

        let stroke = SvgStyle::Stroke(SvgStrokeStyle {
            anti_alias: false,
            high_quality_aa: true,
            transform,
            ..SvgStrokeStyle::default()
        });
        assert!(!stroke.get_antialias());
        assert!(stroke.get_high_quality_aa());
        assert_eq!(stroke.get_transform(), transform);
    }

    // ------------------------------------------------------ SvgParseError

    #[test]
    fn svgparseerror_display_is_non_empty_for_every_variant() {
        let variants = [
            SvgParseError::NoParserAvailable,
            SvgParseError::ElementsLimitReached,
            SvgParseError::NotAnUtf8Str,
            SvgParseError::MalformedGZip,
            SvgParseError::InvalidSize,
            SvgParseError::ParsingFailed(XmlError::NoRootNode),
        ];

        for v in &variants {
            let s = v.to_string();
            assert!(!s.is_empty(), "empty Display output for {v:?}");
            assert!(
                !s.contains("\u{0}"),
                "Display output must not contain NUL bytes"
            );
        }
    }

    #[test]
    fn svgparseerror_display_of_parsing_failed_embeds_the_inner_error() {
        let inner = XmlError::NoRootNode;
        let s = SvgParseError::ParsingFailed(inner.clone()).to_string();
        assert!(s.starts_with("Error parsing SVG:"), "got: {s}");
        assert!(
            s.contains(&inner.to_string()),
            "outer message {s:?} must embed the inner XmlError message"
        );
    }
}
