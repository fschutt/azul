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
    #[must_use]
    pub const fn new(start: SvgPoint, end: SvgPoint) -> Self {
        Self { start, end }
    }

    /// Computes the inward-facing normal vector for this line.
    ///
    /// The normal points 90 degrees to the right of the line direction.
    /// Returns `None` if the line has zero length.
    #[must_use]
    pub fn inwards_normal(&self) -> Option<SvgPoint> {
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
    #[must_use]
    pub fn outwards_normal(&self) -> Option<SvgPoint> {
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
    #[must_use]
    pub const fn get_start(&self) -> SvgPoint {
        self.start
    }
    /// Returns the end point of the line.
    #[must_use]
    pub const fn get_end(&self) -> SvgPoint {
        self.end
    }

    /// Returns the parametric `t` value (0.0–1.0) at the given arc-length offset.
    #[must_use]
    pub fn get_t_at_offset(&self, offset: f64) -> f64 {
        offset / self.get_length()
    }

    /// Returns the tangent vector of the line.
    /// For a line, the tangent is constant (same direction everywhere),
    /// so no `t` parameter is needed.
    #[must_use]
    pub fn get_tangent_vector_at_t(&self) -> SvgVector {
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
    #[must_use]
    pub fn get_x_at_t(&self, t: f64) -> f64 {
        f64::from(self.start.x) + (f64::from(self.end.x) - f64::from(self.start.x)) * t
    }

    /// Returns the Y coordinate at parametric position `t` (0.0 = start, 1.0 = end).
    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
    #[must_use]
    pub fn get_y_at_t(&self, t: f64) -> f64 {
        f64::from(self.start.y) + (f64::from(self.end.y) - f64::from(self.start.y)) * t
    }

    /// Returns the Euclidean length of the line segment.
    #[must_use]
    pub fn get_length(&self) -> f64 {
        let dx = self.end.x - self.start.x;
        let dy = self.end.y - self.start.y;
        f64::from(libm::hypotf(dx, dy))
    }

    /// Returns the axis-aligned bounding rectangle of this line segment.
    #[must_use]
    pub fn get_bounds(&self) -> SvgRect {
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
    #[must_use]
    pub const fn line(l: SvgLine) -> Self {
        Self::Line(l)
    }

    /// Creates a quadratic curve path element from a `SvgQuadraticCurve`
    #[inline]
    #[must_use]
    pub const fn quadratic_curve(qc: SvgQuadraticCurve) -> Self {
        Self::QuadraticCurve(qc)
    }

    /// Creates a cubic curve path element from a `SvgCubicCurve`
    #[inline]
    #[must_use]
    pub const fn cubic_curve(cc: SvgCubicCurve) -> Self {
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
    #[must_use]
    pub const fn get_start(&self) -> SvgPoint {
        match self {
            Self::Line(l) => l.get_start(),
            Self::QuadraticCurve(qc) => qc.get_start(),
            Self::CubicCurve(cc) => cc.get_start(),
        }
    }
    /// Returns the end point of this path element.
    #[must_use]
    pub const fn get_end(&self) -> SvgPoint {
        match self {
            Self::Line(l) => l.get_end(),
            Self::QuadraticCurve(qc) => qc.get_end(),
            Self::CubicCurve(cc) => cc.get_end(),
        }
    }
    /// Returns the axis-aligned bounding rectangle of this path element.
    #[must_use]
    pub fn get_bounds(&self) -> SvgRect {
        match self {
            Self::Line(l) => l.get_bounds(),
            Self::QuadraticCurve(qc) => qc.get_bounds(),
            Self::CubicCurve(cc) => cc.get_bounds(),
        }
    }
    /// Returns the arc length of this path element.
    #[must_use]
    pub fn get_length(&self) -> f64 {
        match self {
            Self::Line(l) => l.get_length(),
            Self::QuadraticCurve(qc) => qc.get_length(),
            Self::CubicCurve(cc) => cc.get_length(),
        }
    }
    /// Returns the parametric `t` value at the given arc-length offset.
    #[must_use]
    pub fn get_t_at_offset(&self, offset: f64) -> f64 {
        match self {
            Self::Line(l) => l.get_t_at_offset(offset),
            Self::QuadraticCurve(qc) => qc.get_t_at_offset(offset),
            Self::CubicCurve(cc) => cc.get_t_at_offset(offset),
        }
    }
    /// Returns the normalized tangent vector at parametric position `t`.
    #[must_use]
    pub fn get_tangent_vector_at_t(&self, t: f64) -> SvgVector {
        match self {
            Self::Line(l) => l.get_tangent_vector_at_t(),
            Self::QuadraticCurve(qc) => qc.get_tangent_vector_at_t(t),
            Self::CubicCurve(cc) => cc.get_tangent_vector_at_t(t),
        }
    }
    /// Returns the X coordinate at parametric position `t`.
    #[must_use]
    pub fn get_x_at_t(&self, t: f64) -> f64 {
        match self {
            Self::Line(l) => l.get_x_at_t(t),
            Self::QuadraticCurve(qc) => qc.get_x_at_t(t),
            Self::CubicCurve(cc) => cc.get_x_at_t(t),
        }
    }
    /// Returns the Y coordinate at parametric position `t`.
    #[must_use]
    pub fn get_y_at_t(&self, t: f64) -> f64 {
        match self {
            Self::Line(l) => l.get_y_at_t(t),
            Self::QuadraticCurve(qc) => qc.get_y_at_t(t),
            Self::CubicCurve(cc) => cc.get_y_at_t(t),
        }
    }
}

impl_vec!(
    SvgPathElement,
    SvgPathElementVec,
    SvgPathElementVecDestructor,
    SvgPathElementVecDestructorType,
    SvgPathElementVecSlice,
    OptionSvgPathElement
);
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
    #[must_use]
    pub const fn create(items: SvgPathElementVec) -> Self {
        Self { items }
    }

    /// Returns the start point of the first element, or `None` if the path is empty.
    #[must_use]
    pub fn get_start(&self) -> Option<SvgPoint> {
        self.items.as_ref().first().map(SvgPathElement::get_start)
    }

    /// Returns the end point of the last element, or `None` if the path is empty.
    #[must_use]
    pub fn get_end(&self) -> Option<SvgPoint> {
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
    #[must_use]
    pub fn is_closed(&self) -> bool {
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
    #[must_use]
    pub fn get_bounds(&self) -> SvgRect {
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

/// A 2D affine transform as SVG writes it: `matrix(a b c d e f)` maps
/// `(x, y)` to `(a x + c y + e, b x + d y + f)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SvgAffine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub e: f64,
    pub f: f64,
}

impl SvgAffine {
    /// No transform.
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    /// A move by `(tx, ty)`.
    #[must_use]
    pub const fn translate(tx: f64, ty: f64) -> Self {
        Self {
            e: tx,
            f: ty,
            ..Self::IDENTITY
        }
    }

    /// This transform, then `outer` (`outer . self`).
    #[must_use]
    pub fn then(&self, outer: &Self) -> Self {
        Self {
            a: outer.a.mul_add(self.a, outer.c * self.b),
            b: outer.b.mul_add(self.a, outer.d * self.b),
            c: outer.a.mul_add(self.c, outer.c * self.d),
            d: outer.b.mul_add(self.c, outer.d * self.d),
            e: outer.a.mul_add(self.e, outer.c * self.f) + outer.e,
            f: outer.b.mul_add(self.e, outer.d * self.f) + outer.f,
        }
    }

    /// Where `(x, y)` lands.
    #[must_use]
    pub fn apply(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.a.mul_add(x, self.c * y) + self.e,
            self.b.mul_add(x, self.d * y) + self.f,
        )
    }

    /// Whether it leaves everything where it is.
    #[must_use]
    pub fn is_identity(&self) -> bool {
        *self == Self::IDENTITY
    }

    /// An `<svg>`'s user space -> its box: the viewBox `(min_x, min_y,
    /// width, height)` scaled onto a box `box_width` x `box_height` (each
    /// axis on its own, as the clip masks map it), the box's origin at 0.
    /// Identity for an empty viewBox.
    #[must_use]
    pub fn view_box_mapping(
        view_box: (f32, f32, f32, f32),
        box_width: f32,
        box_height: f32,
    ) -> Self {
        let (min_x, min_y, vb_w, vb_h) = view_box;
        if !(vb_w > 0.0 && vb_h > 0.0) {
            return Self::IDENTITY;
        }
        let (sx, sy) = (f64::from(box_width / vb_w), f64::from(box_height / vb_h));
        Self {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            e: -f64::from(min_x) * sx,
            f: -f64::from(min_y) * sy,
        }
    }

    /// How much it scales a length, on average over directions: the square
    /// root of its area scale (a stroke's width under it).
    #[must_use]
    pub fn length_scale(&self) -> f64 {
        self.a.mul_add(self.d, -(self.b * self.c)).abs().sqrt()
    }
}

/// Parse an SVG `transform` attribute: a LIST of transform functions, separated
/// by whitespace and / or commas.
///
/// The functions are `matrix(a b c d e f)`, `translate(tx [ty])`,
/// `scale(sx [sy])`, `rotate(angle [cx cy])`, `skewX(angle)` and
/// `skewY(angle)`. The list applies RIGHT TO LEFT (SVG 1.1 7.6:
/// `translate(8) scale(2)` scales first, then moves). An empty list, or one
/// with an unknown function or junk, is no transform (identity). The result
/// maps the element's user space into its parent's.
#[must_use]
pub fn parse_svg_transform(s: &str) -> SvgAffine {
    let is_separator = |c: char| c == ',' || c.is_ascii_whitespace();
    // `None` until the first function: a one-function list is that function
    // EXACTLY (no multiply by identity, which turns +-inf into NaN).
    let mut result: Option<SvgAffine> = None;
    let mut rest = s;
    loop {
        rest = rest.trim_start_matches(is_separator);
        if rest.is_empty() {
            break;
        }
        let Some(open) = rest.find('(') else {
            return SvgAffine::IDENTITY;
        };
        let Some(len) = rest[open..].find(')') else {
            return SvgAffine::IDENTITY;
        };
        let close = open + len;
        let args: Vec<f64> = rest[open + 1..close]
            .split(is_separator)
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect();
        let Some(function) = svg_transform_function(rest[..open].trim(), &args) else {
            return SvgAffine::IDENTITY;
        };
        // Each later function applies BEFORE the ones already read.
        result = Some(result.map_or(function, |so_far| function.then(&so_far)));
        rest = &rest[close + 1..];
    }
    result.unwrap_or(SvgAffine::IDENTITY)
}

/// One SVG transform function `name(args)`; `None` for an unknown name or a
/// `matrix` without exactly six numbers.
fn svg_transform_function(name: &str, args: &[f64]) -> Option<SvgAffine> {
    let arg = |i: usize, default: f64| args.get(i).copied().unwrap_or(default);
    let custom = |a, b, c, d| SvgAffine {
        a,
        b,
        c,
        d,
        e: 0.0,
        f: 0.0,
    };
    Some(match name {
        "matrix" => {
            if args.len() != 6 {
                return None;
            }
            SvgAffine {
                a: args[0],
                b: args[1],
                c: args[2],
                d: args[3],
                e: args[4],
                f: args[5],
            }
        }
        "translate" => SvgAffine::translate(arg(0, 0.0), arg(1, 0.0)),
        "scale" => {
            let sx = arg(0, 1.0);
            custom(sx, 0.0, 0.0, arg(1, sx))
        }
        "rotate" => {
            let angle = arg(0, 0.0).to_radians();
            let (sin_a, cos_a) = (libm::sin(angle), libm::cos(angle));
            let rotation = custom(cos_a, sin_a, -sin_a, cos_a);
            if args.len() >= 3 {
                // rotate(a cx cy) = translate(cx cy) rotate(a) translate(-cx -cy)
                let (cx, cy) = (args[1], args[2]);
                SvgAffine::translate(-cx, -cy)
                    .then(&rotation)
                    .then(&SvgAffine::translate(cx, cy))
            } else {
                rotation
            }
        }
        "skewX" => custom(1.0, 0.0, libm::tan(arg(0, 0.0).to_radians()), 1.0),
        "skewY" => custom(1.0, libm::tan(arg(0, 0.0).to_radians()), 0.0, 1.0),
        _ => return None,
    })
}

impl SvgMultiPolygon {
    /// The same shape mapped through `m` (exact: an affine map takes a line
    /// to a line and a Bezier curve to the curve of its mapped controls).
    #[must_use]
    pub fn transformed(&self, m: &SvgAffine) -> Self {
        let point = |p: SvgPoint| {
            let (x, y) = m.apply(f64::from(p.x), f64::from(p.y));
            #[allow(clippy::cast_possible_truncation)] // user units, f32 like the input
            SvgPoint {
                x: x as f32,
                y: y as f32,
            }
        };
        let rings = self
            .rings
            .iter()
            .map(|ring| SvgPath {
                items: ring
                    .items
                    .iter()
                    .map(|item| match item {
                        SvgPathElement::Line(l) => SvgPathElement::Line(SvgLine {
                            start: point(l.start),
                            end: point(l.end),
                        }),
                        SvgPathElement::QuadraticCurve(q) => {
                            SvgPathElement::QuadraticCurve(SvgQuadraticCurve {
                                start: point(q.start),
                                ctrl: point(q.ctrl),
                                end: point(q.end),
                            })
                        }
                        SvgPathElement::CubicCurve(c) => SvgPathElement::CubicCurve(SvgCubicCurve {
                            start: point(c.start),
                            ctrl_1: point(c.ctrl_1),
                            ctrl_2: point(c.ctrl_2),
                            end: point(c.end),
                        }),
                    })
                    .collect::<Vec<_>>()
                    .into(),
            })
            .collect::<Vec<_>>();
        Self {
            rings: rings.into(),
        }
    }
}

impl SvgMultiPolygon {
    /// How finely a curve is subdivided when the path is treated as an area.
    ///
    /// Sixteen segments per curve keeps the error under a tenth of a pixel for
    /// any curve small enough to be clicked on, and the cost is a handful of
    /// multiplies on a path that is already bounded by the shape it draws.
    const FLATTEN_STEPS: usize = 16;

    /// Is `(x, y)` inside this path, by the NONZERO winding rule?
    ///
    /// What a shape's own geometry means for input: an SVG element occupies a
    /// rectangular BOX in layout, but the thing the user sees and aims at is
    /// the path. Hit-testing the box makes the transparent corners of a
    /// circular button clickable and, worse, makes them SHADOW whatever is
    /// behind them - which is exactly the complaint a clip-path is supposed to
    /// answer.
    ///
    /// Coordinates are in the path's own space; the caller maps the pointer
    /// into it. Open subpaths are treated as closed, as SVG does when filling.
    #[must_use]
    pub fn contains_point(&self, x: f32, y: f32) -> bool {
        let mut winding = 0i32;
        for ring in self.rings.as_ref() {
            let points = ring.flatten_to_points(Self::FLATTEN_STEPS);
            if points.len() < 2 {
                continue;
            }
            for i in 0..points.len() {
                let a = points[i];
                let b = points[(i + 1) % points.len()];
                // Standard nonzero crossing test: count upward crossings to
                // the right of the point positively, downward negatively.
                if a.y <= y {
                    if b.y > y && cross_sign(a, b, x, y) > 0.0 {
                        winding += 1;
                    }
                } else if b.y <= y && cross_sign(a, b, x, y) < 0.0 {
                    winding -= 1;
                }
            }
        }
        winding != 0
    }

    /// Creates a new `SvgMultiPolygon` from a vector of paths (rings)
    /// NOTE: If a ring represents a hole, simply reverse the order of points
    #[inline]
    #[must_use]
    pub const fn create(rings: SvgPathVec) -> Self {
        Self { rings }
    }

    /// Returns the axis-aligned bounding rectangle of all rings in this multi-polygon.
    #[must_use]
    pub fn get_bounds(&self) -> SvgRect {
        // Seed from the FIRST item found in ANY ring, not specifically rings[0].items[0]:
        // an empty first ring used to make the old seed-or-bail return SvgRect::default()
        // and silently drop every later ring's geometry.
        let mut bounds: Option<SvgRect> = None;
        for ring in &self.rings {
            for item in &ring.items {
                let item_bounds = item.get_bounds();
                match &mut bounds {
                    Some(b) => b.union_with(&item_bounds),
                    None => bounds = Some(item_bounds),
                }
            }
        }
        // Empty polygon (no items in any ring) has zero-sized bounds at origin.
        bounds.unwrap_or_default()
    }
}

impl_vec!(
    SvgPath,
    SvgPathVec,
    SvgPathVecDestructor,
    SvgPathVecDestructorType,
    SvgPathVecSlice,
    OptionSvgPath
);
impl_vec_debug!(SvgPath, SvgPathVec);
impl_vec_clone!(SvgPath, SvgPathVec, SvgPathVecDestructor);
impl_vec_partialeq!(SvgPath, SvgPathVec);
impl_vec_partialord!(SvgPath, SvgPathVec);

impl_vec!(
    SvgMultiPolygon,
    SvgMultiPolygonVec,
    SvgMultiPolygonVecDestructor,
    SvgMultiPolygonVecDestructorType,
    SvgMultiPolygonVecSlice,
    OptionSvgMultiPolygon
);
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

impl_vec!(
    SvgSimpleNode,
    SvgSimpleNodeVec,
    SvgSimpleNodeVecDestructor,
    SvgSimpleNodeVecDestructorType,
    SvgSimpleNodeVecSlice,
    OptionSvgSimpleNode
);
impl_vec_debug!(SvgSimpleNode, SvgSimpleNodeVec);
impl_vec_clone!(SvgSimpleNode, SvgSimpleNodeVec, SvgSimpleNodeVecDestructor);
impl_vec_partialeq!(SvgSimpleNode, SvgSimpleNodeVec);
impl_vec_partialord!(SvgSimpleNode, SvgSimpleNodeVec);

impl SvgSimpleNode {
    /// Returns the axis-aligned bounding rectangle of this node.
    // Same-body arms dispatch on differently-typed bindings (SvgPath vs SvgCircle),
    // so the identical `a.get_bounds()` bodies cannot be combined into one or-pattern.
    #[allow(clippy::match_same_arms)]
    #[must_use]
    pub fn get_bounds(&self) -> SvgRect {
        match self {
            Self::Path(a) => a.get_bounds(),
            Self::Circle(a) => a.get_bounds(),
            Self::Rect(a) => *a,
            Self::CircleHole(a) => a.get_bounds(),
            Self::RectHole(a) => *a,
        }
    }
    /// Returns `true` if this node represents a closed shape.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        match self {
            Self::Path(a) => a.is_closed(),
            Self::Circle(_) | Self::Rect(_) | Self::CircleHole(_) | Self::RectHole(_) => true,
        }
    }
}

impl SvgNode {
    /// Returns the axis-aligned bounding rectangle of this SVG node.
    #[must_use]
    pub fn get_bounds(&self) -> SvgRect {
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
    #[must_use]
    pub fn is_closed(&self) -> bool {
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

/// Which side of the segment `a -> b` the point `(x, y)` is on.
///
/// Positive = left. Used by the winding test; a separate function because the
/// sign convention is the whole subtlety and naming it makes the two branches
/// above readable.
#[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma
fn cross_sign(a: SvgPoint, b: SvgPoint, x: f32, y: f32) -> f32 {
    (b.x - a.x) * (y - a.y) - (x - a.x) * (b.y - a.y)
}

impl SvgPath {
    /// Flatten this ring into a polyline, subdividing every curve into
    /// `steps` segments.
    ///
    /// Deliberately not de-duplicating shared endpoints: the winding test
    /// walks the list cyclically and a repeated point contributes a
    /// zero-length segment, which crosses nothing.
    #[must_use]
    // The curve arms bind a/b/cc/d and the parameter t: those single letters
    // ARE the notation the Bezier basis formulas below are read in.
    #[allow(clippy::many_single_char_names)]
    pub fn flatten_to_points(&self, steps: usize) -> alloc::vec::Vec<SvgPoint> {
        use azul_css::props::basic::SvgPoint;

        let steps = steps.max(1);
        let mut out: alloc::vec::Vec<SvgPoint> = alloc::vec::Vec::new();
        let mut push = |p: SvgPoint| {
            if out.last().is_none_or(|last| {
                (last.x - p.x).abs() > f32::EPSILON || (last.y - p.y).abs() > f32::EPSILON
            }) {
                out.push(p);
            }
        };
        for item in self.items.as_ref() {
            match item {
                SvgPathElement::Line(l) => {
                    push(l.start);
                    push(l.end);
                }
                SvgPathElement::QuadraticCurve(q) => {
                    push(q.start);
                    for i in 1..=steps {
                        #[allow(clippy::cast_precision_loss)] // steps is <= 64
                        let t = i as f32 / steps as f32;
                        let inv = 1.0 - t;
                        push(SvgPoint {
                            x: (t * t).mul_add(
                                q.end.x,
                                (inv * inv).mul_add(q.start.x, 2.0 * inv * t * q.ctrl.x),
                            ),
                            y: (t * t).mul_add(
                                q.end.y,
                                (inv * inv).mul_add(q.start.y, 2.0 * inv * t * q.ctrl.y),
                            ),
                        });
                    }
                }
                SvgPathElement::CubicCurve(c) => {
                    push(c.start);
                    for i in 1..=steps {
                        #[allow(clippy::cast_precision_loss)] // steps is <= 64
                        let t = i as f32 / steps as f32;
                        let inv = 1.0 - t;
                        let (a, b, cc, d) = (
                            inv * inv * inv,
                            3.0 * inv * inv * t,
                            3.0 * inv * t * t,
                            t * t * t,
                        );
                        push(SvgPoint {
                            x: d.mul_add(
                                c.end.x,
                                cc.mul_add(c.ctrl_2.x, a.mul_add(c.start.x, b * c.ctrl_1.x)),
                            ),
                            y: d.mul_add(
                                c.end.y,
                                cc.mul_add(c.ctrl_2.y, a.mul_add(c.start.y, b * c.ctrl_1.y)),
                            ),
                        });
                    }
                }
            }
        }
        out
    }
}

impl SvgCircle {
    /// Returns `true` if the given point lies inside the circle.
    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
    #[must_use]
    pub fn contains_point(&self, x: f32, y: f32) -> bool {
        let x_diff = libm::fabsf(x - self.center_x);
        let y_diff = libm::fabsf(y - self.center_y);
        (x_diff * x_diff) + (y_diff * y_diff) < (self.radius * self.radius)
    }
    /// Returns the axis-aligned bounding rectangle of this circle.
    #[must_use]
    pub fn get_bounds(&self) -> SvgRect {
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

impl_vec!(
    TessellatedSvgNode,
    TessellatedSvgNodeVec,
    TessellatedSvgNodeVecDestructor,
    TessellatedSvgNodeVecDestructorType,
    TessellatedSvgNodeVecSlice,
    OptionTessellatedSvgNode
);
impl_vec_debug!(TessellatedSvgNode, TessellatedSvgNodeVec);
impl_vec_partialord!(TessellatedSvgNode, TessellatedSvgNodeVec);
impl_vec_clone!(
    TessellatedSvgNode,
    TessellatedSvgNodeVec,
    TessellatedSvgNodeVecDestructor
);
impl_vec_partialeq!(TessellatedSvgNode, TessellatedSvgNodeVec);

impl TessellatedSvgNode {
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }
}

impl TessellatedSvgNodeVec {
    #[must_use]
    pub fn get_ref(&self) -> TessellatedSvgNodeVecRef {
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
    #[must_use]
    pub const fn as_slice(&self) -> &[TessellatedSvgNode] {
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

impl_vec!(
    TessellatedColoredSvgNode,
    TessellatedColoredSvgNodeVec,
    TessellatedColoredSvgNodeVecDestructor,
    TessellatedColoredSvgNodeVecDestructorType,
    TessellatedColoredSvgNodeVecSlice,
    OptionTessellatedColoredSvgNode
);
impl_vec_debug!(TessellatedColoredSvgNode, TessellatedColoredSvgNodeVec);
impl_vec_partialord!(TessellatedColoredSvgNode, TessellatedColoredSvgNodeVec);
impl_vec_clone!(
    TessellatedColoredSvgNode,
    TessellatedColoredSvgNodeVec,
    TessellatedColoredSvgNodeVecDestructor
);
impl_vec_partialeq!(TessellatedColoredSvgNode, TessellatedColoredSvgNodeVec);

impl TessellatedColoredSvgNode {
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }
}

impl TessellatedColoredSvgNodeVec {
    #[must_use]
    pub fn get_ref(&self) -> TessellatedColoredSvgNodeVecRef {
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
    #[must_use]
    pub const fn as_slice(&self) -> &[TessellatedColoredSvgNode] {
        unsafe { core::slice::from_raw_parts(self.ptr, self.len) }
    }
}

impl_vec!(
    SvgVertex,
    SvgVertexVec,
    SvgVertexVecDestructor,
    SvgVertexVecDestructorType,
    SvgVertexVecSlice,
    OptionSvgVertex
);
impl_vec_debug!(SvgVertex, SvgVertexVec);
impl_vec_partialord!(SvgVertex, SvgVertexVec);
impl_vec_clone!(SvgVertex, SvgVertexVec, SvgVertexVecDestructor);
impl_vec_partialeq!(SvgVertex, SvgVertexVec);

impl_vec!(
    SvgColoredVertex,
    SvgColoredVertexVec,
    SvgColoredVertexVecDestructor,
    SvgColoredVertexVecDestructorType,
    SvgColoredVertexVecSlice,
    OptionSvgColoredVertex
);
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
        uniform_type: UniformType::FloatVec2([target_size.width as f32, target_size.height as f32]),
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
    #[must_use]
    pub fn new(node: &TessellatedSvgNode, gl: GlContextPtr) -> Self {
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
    #[allow(clippy::needless_pass_by_value)] // owned azul value taken by value (public API /
                                             // ownership-transfer convention)
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
    #[must_use]
    pub fn new(node: &TessellatedColoredSvgNode, gl: GlContextPtr) -> Self {
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
    #[allow(clippy::needless_pass_by_value)] // owned azul value taken by value (public API /
                                             // ownership-transfer convention)
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
    #[must_use]
    pub const fn get_antialias(&self) -> bool {
        match self {
            Self::Fill(f) => f.anti_alias,
            Self::Stroke(s) => s.anti_alias,
        }
    }
    #[must_use]
    pub const fn get_high_quality_aa(&self) -> bool {
        match self {
            Self::Fill(f) => f.high_quality_aa,
            Self::Stroke(s) => s.high_quality_aa,
        }
    }
    #[must_use]
    pub const fn get_transform(&self) -> SvgTransform {
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
#[allow(variant_size_differences)]
// repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size
// disparity accepted
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
        use self::SvgParseError::{
            ElementsLimitReached, InvalidSize, MalformedGZip, NoParserAvailable, NotAnUtf8Str,
            ParsingFailed,
        };
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
#[path = "svg_test.rs"]
mod svg_test;
