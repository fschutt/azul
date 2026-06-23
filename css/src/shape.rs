//! CSS Shape data structures for shape-inside, shape-outside, and clip-path
//!
//! These types are C-compatible (repr(C)) for use across FFI boundaries.

use alloc::string::String;
use crate::corety::{AzString, OptionF32};

/// Compares two f32 values for ordering, treating NaN as equal.
fn cmp_f32(a: f32, b: f32) -> core::cmp::Ordering {
    a.partial_cmp(&b).unwrap_or(core::cmp::Ordering::Equal)
}

/// A 2D point for shape coordinates (using f32 for precision)
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct ShapePoint {
    pub x: f32,
    pub y: f32,
}

impl_option!(
    ShapePoint,
    OptionShapePoint,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd]
);

impl ShapePoint {
    #[must_use] pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    #[must_use] pub const fn zero() -> Self {
        Self { x: 0.0, y: 0.0 }
    }
}

impl Eq for ShapePoint {}

impl Ord for ShapePoint {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        match self.x.partial_cmp(&other.x) {
            Some(core::cmp::Ordering::Equal) => self
                .y
                .partial_cmp(&other.y)
                .unwrap_or(core::cmp::Ordering::Equal),
            other => other.unwrap_or(core::cmp::Ordering::Equal),
        }
    }
}

impl core::hash::Hash for ShapePoint {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.x.to_bits().hash(state);
        self.y.to_bits().hash(state);
    }
}

impl_vec!(ShapePoint, ShapePointVec, ShapePointVecDestructor, ShapePointVecDestructorType, ShapePointVecSlice, OptionShapePoint);
impl_vec_debug!(ShapePoint, ShapePointVec);
impl_vec_partialord!(ShapePoint, ShapePointVec);
impl_vec_ord!(ShapePoint, ShapePointVec);
impl_vec_clone!(ShapePoint, ShapePointVec, ShapePointVecDestructor);
impl_vec_partialeq!(ShapePoint, ShapePointVec);
impl_vec_eq!(ShapePoint, ShapePointVec);
impl_vec_hash!(ShapePoint, ShapePointVec);

/// A circle shape defined by center point and radius
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct ShapeCircle {
    pub center: ShapePoint,
    pub radius: f32,
}

impl Eq for ShapeCircle {}
impl core::hash::Hash for ShapeCircle {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.center.hash(state);
        self.radius.to_bits().hash(state);
    }
}
impl PartialOrd for ShapeCircle {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ShapeCircle {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        match self.center.cmp(&other.center) {
            core::cmp::Ordering::Equal => self
                .radius
                .partial_cmp(&other.radius)
                .unwrap_or(core::cmp::Ordering::Equal),
            other => other,
        }
    }
}

/// An ellipse shape defined by center point and two radii
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct ShapeEllipse {
    pub center: ShapePoint,
    pub radius_x: f32,
    pub radius_y: f32,
}

impl Eq for ShapeEllipse {}
impl core::hash::Hash for ShapeEllipse {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.center.hash(state);
        self.radius_x.to_bits().hash(state);
        self.radius_y.to_bits().hash(state);
    }
}
impl PartialOrd for ShapeEllipse {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ShapeEllipse {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        match self.center.cmp(&other.center) {
            core::cmp::Ordering::Equal => match self.radius_x.partial_cmp(&other.radius_x) {
                Some(core::cmp::Ordering::Equal) | None => self
                    .radius_y
                    .partial_cmp(&other.radius_y)
                    .unwrap_or(core::cmp::Ordering::Equal),
                Some(other) => other,
            },
            other => other,
        }
    }
}

/// A polygon shape defined by a list of points (in clockwise order)
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct ShapePolygon {
    pub points: ShapePointVec,
}

/// An inset rectangle with optional border radius
/// Defined by insets from the reference box edges
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct ShapeInset {
    pub inset_top: f32,
    pub inset_right: f32,
    pub inset_bottom: f32,
    pub inset_left: f32,
    pub border_radius: OptionF32,
}

impl Eq for ShapeInset {}
impl core::hash::Hash for ShapeInset {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.inset_top.to_bits().hash(state);
        self.inset_right.to_bits().hash(state);
        self.inset_bottom.to_bits().hash(state);
        self.inset_left.to_bits().hash(state);
        self.border_radius.hash(state);
    }
}
impl PartialOrd for ShapeInset {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ShapeInset {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        cmp_f32(self.inset_top, other.inset_top)
            .then_with(|| cmp_f32(self.inset_right, other.inset_right))
            .then_with(|| cmp_f32(self.inset_bottom, other.inset_bottom))
            .then_with(|| cmp_f32(self.inset_left, other.inset_left))
            .then_with(|| self.border_radius.cmp(&other.border_radius))
    }
}

/// An SVG-like path for shape definitions.
/// TODO: path parsing is not yet implemented — `data` is stored but not interpreted.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct ShapePath {
    pub data: AzString,
}

/// Represents a CSS shape for shape-inside, shape-outside, and clip-path.
/// Used for both text layout (shape-inside/outside) and rendering clipping (clip-path).
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum CssShape {
    Circle(ShapeCircle),
    Ellipse(ShapeEllipse),
    Polygon(ShapePolygon),
    Inset(ShapeInset),
    Path(ShapePath),
}

impl Eq for CssShape {}

impl core::hash::Hash for CssShape {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        match self {
            Self::Circle(c) => c.hash(state),
            Self::Ellipse(e) => e.hash(state),
            Self::Polygon(p) => p.hash(state),
            Self::Inset(i) => i.hash(state),
            Self::Path(p) => p.hash(state),
        }
    }
}

impl PartialOrd for CssShape {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CssShape {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        match (self, other) {
            (Self::Circle(a), Self::Circle(b)) => a.cmp(b),
            (Self::Ellipse(a), Self::Ellipse(b)) => a.cmp(b),
            (Self::Polygon(a), Self::Polygon(b)) => a.cmp(b),
            (Self::Inset(a), Self::Inset(b)) => a.cmp(b),
            (Self::Path(a), Self::Path(b)) => a.cmp(b),
            // Different variants: use discriminant ordering
            (Self::Circle(_), _) => core::cmp::Ordering::Less,
            (_, Self::Circle(_)) => core::cmp::Ordering::Greater,
            (Self::Ellipse(_), _) => core::cmp::Ordering::Less,
            (_, Self::Ellipse(_)) => core::cmp::Ordering::Greater,
            (Self::Polygon(_), _) => core::cmp::Ordering::Less,
            (_, Self::Polygon(_)) => core::cmp::Ordering::Greater,
            (Self::Inset(_), Self::Path(_)) => core::cmp::Ordering::Less,
            (Self::Path(_), Self::Inset(_)) => core::cmp::Ordering::Greater,
        }
    }
}

impl CssShape {
    /// Creates a circle shape at the given position with the given radius
    #[must_use] pub const fn circle(center: ShapePoint, radius: f32) -> Self {
        Self::Circle(ShapeCircle { center, radius })
    }

    /// Creates an ellipse shape
    #[must_use] pub const fn ellipse(center: ShapePoint, radius_x: f32, radius_y: f32) -> Self {
        Self::Ellipse(ShapeEllipse {
            center,
            radius_x,
            radius_y,
        })
    }

    /// Creates a polygon from a list of points
    #[must_use] pub const fn polygon(points: ShapePointVec) -> Self {
        Self::Polygon(ShapePolygon { points })
    }

    /// Creates an inset rectangle
    #[must_use] pub const fn inset(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self::Inset(ShapeInset {
            inset_top: top,
            inset_right: right,
            inset_bottom: bottom,
            inset_left: left,
            border_radius: OptionF32::None,
        })
    }

    /// Creates an inset rectangle with rounded corners
    #[must_use] pub const fn inset_rounded(top: f32, right: f32, bottom: f32, left: f32, radius: f32) -> Self {
        Self::Inset(ShapeInset {
            inset_top: top,
            inset_right: right,
            inset_bottom: bottom,
            inset_left: left,
            border_radius: OptionF32::Some(radius),
        })
    }

    #[must_use] pub fn print_as_css_value(&self) -> String {
        use alloc::format;
        match self {
            Self::Circle(ShapeCircle { center, radius }) => {
                format!("circle({}px at {}px {}px)", radius, center.x, center.y)
            }
            Self::Ellipse(ShapeEllipse { center, radius_x, radius_y }) => {
                format!("ellipse({}px {}px at {}px {}px)", radius_x, radius_y, center.x, center.y)
            }
            Self::Polygon(ShapePolygon { points }) => {
                let pts: Vec<String> = points.as_ref().iter()
                    .map(|p| format!("{}px {}px", p.x, p.y))
                    .collect();
                format!("polygon({})", pts.join(", "))
            }
            Self::Inset(ShapeInset { inset_top, inset_right, inset_bottom, inset_left, border_radius }) => {
                let base = format!("inset({inset_top}px {inset_right}px {inset_bottom}px {inset_left}px");
                match border_radius {
                    OptionF32::Some(r) => format!("{base} round {r}px)"),
                    OptionF32::None => format!("{base})"),
                }
            }
            Self::Path(ShapePath { data }) => {
                format!("path(\"{}\")", data.as_str())
            }
        }
    }

    #[must_use] pub fn format_as_rust_code(&self) -> String {
        use alloc::format;
        match self {
            Self::Circle(ShapeCircle { center, radius }) => {
                format!(
                    "CssShape::Circle(ShapeCircle {{ center: ShapePoint::new({}_f32, {}_f32), radius: {}_f32 }})",
                    center.x, center.y, radius
                )
            }
            Self::Ellipse(ShapeEllipse { center, radius_x, radius_y }) => {
                format!(
                    "CssShape::Ellipse(ShapeEllipse {{ center: ShapePoint::new({}_f32, {}_f32), radius_x: {}_f32, radius_y: {}_f32 }})",
                    center.x, center.y, radius_x, radius_y
                )
            }
            Self::Polygon(ShapePolygon { points }) => {
                let pts: Vec<String> = points.as_ref().iter()
                    .map(|p| format!("ShapePoint::new({}_f32, {}_f32)", p.x, p.y))
                    .collect();
                format!("CssShape::Polygon(ShapePolygon {{ points: vec![{}].into() }})", pts.join(", "))
            }
            Self::Inset(ShapeInset { inset_top, inset_right, inset_bottom, inset_left, border_radius }) => {
                let br = match border_radius {
                    OptionF32::Some(r) => format!("OptionF32::Some({r}_f32)"),
                    OptionF32::None => String::from("OptionF32::None"),
                };
                format!(
                    "CssShape::Inset(ShapeInset {{ inset_top: {inset_top}_f32, inset_right: {inset_right}_f32, inset_bottom: {inset_bottom}_f32, inset_left: {inset_left}_f32, border_radius: {br} }})"
                )
            }
            Self::Path(ShapePath { data }) => {
                format!("CssShape::Path(ShapePath {{ data: AzString::from_const_str(\"{}\") }})", data.as_str())
            }
        }
    }
}

impl_option!(
    CssShape,
    OptionCssShape,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

