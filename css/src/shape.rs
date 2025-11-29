//! CSS Shape data structures for shape-inside, shape-outside, and clip-path
//!
//! These types are C-compatible (repr(C)) for use across FFI boundaries.

use crate::corety::{AzString, OptionF32};

/// A 2D point for shape coordinates
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
#[repr(C)]
pub struct LayoutPoint {
    pub x: f32,
    pub y: f32,
}

impl LayoutPoint {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub const fn zero() -> Self {
        Self { x: 0.0, y: 0.0 }
    }
}

impl Eq for LayoutPoint {}

impl Ord for LayoutPoint {
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

impl core::hash::Hash for LayoutPoint {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.x.to_bits().hash(state);
        self.y.to_bits().hash(state);
    }
}

impl_vec!(LayoutPoint, LayoutPointVec, LayoutPointVecDestructor);
impl_vec_debug!(LayoutPoint, LayoutPointVec);
impl_vec_partialord!(LayoutPoint, LayoutPointVec);
impl_vec_ord!(LayoutPoint, LayoutPointVec);
impl_vec_clone!(LayoutPoint, LayoutPointVec, LayoutPointVecDestructor);
impl_vec_partialeq!(LayoutPoint, LayoutPointVec);
impl_vec_eq!(LayoutPoint, LayoutPointVec);
impl_vec_hash!(LayoutPoint, LayoutPointVec);

/// A circle shape defined by center point and radius
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct ShapeCircle {
    pub center: LayoutPoint,
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
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct ShapeEllipse {
    pub center: LayoutPoint,
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
    pub points: LayoutPointVec,
}

/// An inset rectangle with optional border radius
/// Defined by insets from the reference box edges
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct ShapeInset {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
    pub border_radius: OptionF32,
}

impl Eq for ShapeInset {}
impl core::hash::Hash for ShapeInset {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.top.to_bits().hash(state);
        self.right.to_bits().hash(state);
        self.bottom.to_bits().hash(state);
        self.left.to_bits().hash(state);
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
        match self.top.partial_cmp(&other.top) {
            Some(core::cmp::Ordering::Equal) | None => match self.right.partial_cmp(&other.right) {
                Some(core::cmp::Ordering::Equal) | None => {
                    match self.bottom.partial_cmp(&other.bottom) {
                        Some(core::cmp::Ordering::Equal) | None => {
                            match self.left.partial_cmp(&other.left) {
                                Some(core::cmp::Ordering::Equal) | None => {
                                    self.border_radius.cmp(&other.border_radius)
                                }
                                Some(other) => other,
                            }
                        }
                        Some(other) => other,
                    }
                }
                Some(other) => other,
            },
            Some(other) => other,
        }
    }
}

/// An SVG-like path (for future use)
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct ShapePath {
    pub data: AzString,
}

/// Represents a CSS shape for shape-inside, shape-outside, and clip-path.
/// Used for both text layout (shape-inside/outside) and rendering clipping (clip-path).
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum Shape {
    Circle(ShapeCircle),
    Ellipse(ShapeEllipse),
    Polygon(ShapePolygon),
    Inset(ShapeInset),
    Path(ShapePath),
}

impl Eq for Shape {}

impl core::hash::Hash for Shape {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        match self {
            Shape::Circle(c) => c.hash(state),
            Shape::Ellipse(e) => e.hash(state),
            Shape::Polygon(p) => p.hash(state),
            Shape::Inset(i) => i.hash(state),
            Shape::Path(p) => p.hash(state),
        }
    }
}

impl PartialOrd for Shape {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Shape {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        match (self, other) {
            (Shape::Circle(a), Shape::Circle(b)) => a.cmp(b),
            (Shape::Ellipse(a), Shape::Ellipse(b)) => a.cmp(b),
            (Shape::Polygon(a), Shape::Polygon(b)) => a.cmp(b),
            (Shape::Inset(a), Shape::Inset(b)) => a.cmp(b),
            (Shape::Path(a), Shape::Path(b)) => a.cmp(b),
            // Different variants: use discriminant ordering
            (Shape::Circle(_), _) => core::cmp::Ordering::Less,
            (_, Shape::Circle(_)) => core::cmp::Ordering::Greater,
            (Shape::Ellipse(_), _) => core::cmp::Ordering::Less,
            (_, Shape::Ellipse(_)) => core::cmp::Ordering::Greater,
            (Shape::Polygon(_), _) => core::cmp::Ordering::Less,
            (_, Shape::Polygon(_)) => core::cmp::Ordering::Greater,
            (Shape::Inset(_), Shape::Path(_)) => core::cmp::Ordering::Less,
            (Shape::Path(_), Shape::Inset(_)) => core::cmp::Ordering::Greater,
        }
    }
}

impl Shape {
    /// Creates a circle shape at the given position with the given radius
    pub fn circle(center: LayoutPoint, radius: f32) -> Self {
        Shape::Circle(ShapeCircle { center, radius })
    }

    /// Creates an ellipse shape
    pub fn ellipse(center: LayoutPoint, radius_x: f32, radius_y: f32) -> Self {
        Shape::Ellipse(ShapeEllipse {
            center,
            radius_x,
            radius_y,
        })
    }

    /// Creates a polygon from a list of points
    pub fn polygon(points: LayoutPointVec) -> Self {
        Shape::Polygon(ShapePolygon { points })
    }

    /// Creates an inset rectangle
    pub fn inset(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Shape::Inset(ShapeInset {
            top,
            right,
            bottom,
            left,
            border_radius: OptionF32::None,
        })
    }

    /// Creates an inset rectangle with rounded corners
    pub fn inset_rounded(top: f32, right: f32, bottom: f32, left: f32, radius: f32) -> Self {
        Shape::Inset(ShapeInset {
            top,
            right,
            bottom,
            left,
            border_radius: OptionF32::Some(radius),
        })
    }
}

impl_option!(Shape, OptionShape, copy = false, [Debug, Clone, PartialEq]);

/// A line segment representing available horizontal space at a given y-position.
/// Used for line breaking within shaped containers.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct LineSegment {
    /// The x-coordinate where this segment starts
    pub start_x: f32,

    /// The width of this segment
    pub width: f32,

    /// Priority for choosing between overlapping segments (higher = preferred)
    pub priority: i32,
}

impl LineSegment {
    /// Creates a new line segment
    pub const fn new(start_x: f32, width: f32) -> Self {
        Self {
            start_x,
            width,
            priority: 0,
        }
    }

    /// Returns the end x-coordinate of this segment
    #[inline]
    pub fn end_x(&self) -> f32 {
        self.start_x + self.width
    }

    /// Returns true if this segment overlaps with another segment
    pub fn overlaps(&self, other: &Self) -> bool {
        self.start_x < other.end_x() && other.start_x < self.end_x()
    }

    /// Computes the intersection of two segments, if any
    pub fn intersection(&self, other: &Self) -> Option<Self> {
        let start = self.start_x.max(other.start_x);
        let end = self.end_x().min(other.end_x());

        if start < end {
            Some(Self {
                start_x: start,
                width: end - start,
                priority: self.priority.max(other.priority),
            })
        } else {
            None
        }
    }
}

impl_vec!(LineSegment, LineSegmentVec, LineSegmentVecDestructor);
impl_vec_debug!(LineSegment, LineSegmentVec);
impl_vec_clone!(LineSegment, LineSegmentVec, LineSegmentVecDestructor);
impl_vec_partialeq!(LineSegment, LineSegmentVec);

/// A 2D rectangle for shape bounding boxes and reference boxes
#[derive(Debug, Copy, Clone, PartialEq)]
#[repr(C)]
pub struct LayoutRect {
    pub origin: LayoutPoint,
    pub width: f32,
    pub height: f32,
}

impl LayoutRect {
    pub const fn new(origin: LayoutPoint, width: f32, height: f32) -> Self {
        Self {
            origin,
            width,
            height,
        }
    }

    pub const fn zero() -> Self {
        Self {
            origin: LayoutPoint::zero(),
            width: 0.0,
            height: 0.0,
        }
    }
}

impl_option!(
    LayoutRect,
    OptionLayoutRect,
    [Debug, Copy, Clone, PartialEq]
);

impl Shape {
    /// Computes the bounding box of this shape
    pub fn bounding_box(&self) -> LayoutRect {
        match self {
            Shape::Circle(ShapeCircle { center, radius }) => LayoutRect {
                origin: LayoutPoint::new(center.x - radius, center.y - radius),
                width: radius * 2.0,
                height: radius * 2.0,
            },

            Shape::Ellipse(ShapeEllipse {
                center,
                radius_x,
                radius_y,
            }) => LayoutRect {
                origin: LayoutPoint::new(center.x - radius_x, center.y - radius_y),
                width: radius_x * 2.0,
                height: radius_y * 2.0,
            },

            Shape::Polygon(ShapePolygon { points }) => {
                if points.as_ref().is_empty() {
                    return LayoutRect::zero();
                }

                let first = points.as_ref()[0];
                let mut min_x = first.x;
                let mut min_y = first.y;
                let mut max_x = first.x;
                let mut max_y = first.y;

                for point in points.as_ref().iter().skip(1) {
                    min_x = min_x.min(point.x);
                    min_y = min_y.min(point.y);
                    max_x = max_x.max(point.x);
                    max_y = max_y.max(point.y);
                }

                LayoutRect {
                    origin: LayoutPoint::new(min_x, min_y),
                    width: max_x - min_x,
                    height: max_y - min_y,
                }
            }

            Shape::Inset(ShapeInset {
                top,
                right,
                bottom,
                left,
                ..
            }) => {
                // For inset, we need the reference box to compute actual bounds
                // For now, return a placeholder that indicates the insets
                LayoutRect {
                    origin: LayoutPoint::new(*left, *top),
                    width: 0.0, // Will be computed relative to container
                    height: 0.0,
                }
            }

            Shape::Path(_) => {
                // Path bounding box computation requires parsing the path
                // For now, return zero rect
                LayoutRect::zero()
            }
        }
    }

    /// Computes available horizontal line segments at a given y-position.
    /// Used for text layout with shape-inside.
    ///
    /// # Arguments
    /// * `y` - The vertical position to compute segments for
    /// * `margin` - Inward margin from the shape boundary
    /// * `reference_box` - The containing box for inset shapes
    ///
    /// # Returns
    /// A vector of line segments, sorted by start_x
    pub fn compute_line_segments(
        &self,
        y: f32,
        margin: f32,
        reference_box: OptionLayoutRect,
    ) -> LineSegmentVec {
        use alloc::vec::Vec;

        let segments: Vec<LineSegment> = match self {
            Shape::Circle(ShapeCircle { center, radius }) => {
                let dy = y - center.y;
                let r_with_margin = radius - margin;

                if dy.abs() > r_with_margin {
                    Vec::new() // Outside circle
                } else {
                    // Chord width at y: w = 2*sqrt(r²-dy²)
                    let half_width = (r_with_margin.powi(2) - dy.powi(2)).sqrt();

                    alloc::vec![LineSegment {
                        start_x: center.x - half_width,
                        width: 2.0 * half_width,
                        priority: 0,
                    }]
                }
            }

            Shape::Ellipse(ShapeEllipse {
                center,
                radius_x,
                radius_y,
            }) => {
                let dy = y - center.y;
                let ry_with_margin = radius_y - margin;

                if dy.abs() > ry_with_margin {
                    Vec::new() // Outside ellipse
                } else {
                    // Ellipse equation: (x/rx)² + (y/ry)² = 1
                    // Solve for x at given y: x = rx * sqrt(1 - (y/ry)²)
                    let ratio = dy / ry_with_margin;
                    let factor = (1.0 - ratio.powi(2)).sqrt();
                    let half_width = (radius_x - margin) * factor;

                    alloc::vec![LineSegment {
                        start_x: center.x - half_width,
                        width: 2.0 * half_width,
                        priority: 0,
                    }]
                }
            }

            Shape::Polygon(ShapePolygon { points }) => {
                compute_polygon_line_segments(points.as_ref(), y, margin)
            }

            Shape::Inset(ShapeInset {
                top,
                right,
                bottom,
                left,
                border_radius,
            }) => {
                let ref_box = match reference_box {
                    OptionLayoutRect::Some(r) => r,
                    OptionLayoutRect::None => LayoutRect::zero(),
                };

                let inset_top = ref_box.origin.y + top + margin;
                let inset_bottom = ref_box.origin.y + ref_box.height - bottom - margin;
                let inset_left = ref_box.origin.x + left + margin;
                let inset_right = ref_box.origin.x + ref_box.width - right - margin;

                if y < inset_top || y > inset_bottom {
                    Vec::new()
                } else {
                    // TODO: Handle border_radius for rounded corners
                    // For now, just return full width
                    alloc::vec![LineSegment {
                        start_x: inset_left,
                        width: inset_right - inset_left,
                        priority: 0,
                    }]
                }
            }

            Shape::Path(_) => {
                // Path intersection requires path parsing
                // For now, return empty
                Vec::new()
            }
        };

        segments.into()
    }
}

/// Computes line segments for a polygon at a given y-position.
/// Uses a scanline algorithm to find intersections with polygon edges.
fn compute_polygon_line_segments(
    points: &[LayoutPoint],
    y: f32,
    margin: f32,
) -> alloc::vec::Vec<LineSegment> {
    use alloc::vec::Vec;

    if points.len() < 3 {
        return Vec::new();
    }

    // Find all intersections of the horizontal line y with polygon edges
    let mut intersections = Vec::new();

    for i in 0..points.len() {
        let p1 = points[i];
        let p2 = points[(i + 1) % points.len()];

        // Check if edge crosses the scanline
        let min_y = p1.y.min(p2.y);
        let max_y = p1.y.max(p2.y);

        if y >= min_y && y < max_y {
            // Compute x-intersection using linear interpolation
            let t = (y - p1.y) / (p2.y - p1.y);
            let x = p1.x + t * (p2.x - p1.x);
            intersections.push(x);
        }
    }

    // Sort intersections
    intersections.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));

    // Pair up intersections to form segments
    // Polygon fill rule: pairs of intersections form filled regions
    let mut segments = Vec::new();

    for chunk in intersections.chunks(2) {
        if chunk.len() == 2 {
            let start = chunk[0] + margin;
            let end = chunk[1] - margin;

            if start < end {
                segments.push(LineSegment {
                    start_x: start,
                    width: end - start,
                    priority: 0,
                });
            }
        }
    }

    segments
}
