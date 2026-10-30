//! Geometry for text: rects, sizes and points, paths, and the shapes text flows inside or around.

use std::{
    cmp::Ordering,
    collections::{
        hash_map::{DefaultHasher, HashMap},
        BTreeSet, HashSet,
    },
    hash::{Hash, Hasher},
    mem::discriminant,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};
use azul_core::{
    dom::NodeId,
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::ImageRef,
    selection::{CursorAffinity, SelectionRange, TextCursor},
    ui_solver::GlyphInstance,
};
use azul_css::{
    corety::LayoutDebugMessage,
    props::{basic::ColorU, style::StyleBackgroundContent},
};
#[cfg(feature = "text_layout_hyphenation")]
use hyphenation::{Hyphenator, Language as HyphenationLanguage, Load, Standard};
use rust_fontconfig::{
    FcFontCache, FcPattern, FcStretch, FcWeight, FontId, PatternMatch, UnicodeRange,
};
use smallvec::{smallvec, SmallVec};
use unicode_bidi::{BidiInfo, Level, TextSource};
use unicode_segmentation::UnicodeSegmentation;
use crate::text3::script::{script_to_language, Language, Script};
#[allow(clippy::wildcard_imports)]
// the text layout cache's items, re-exported from the sibling modules by mod.rs
use super::*;

// Path and shape definitions
#[derive(Copy, Debug, Clone, PartialOrd)]
pub enum PathSegment {
    MoveTo(Point),
    LineTo(Point),
    CurveTo {
        control1: Point,
        control2: Point,
        end: Point,
    },
    QuadTo {
        control: Point,
        end: Point,
    },
    Arc {
        center: Point,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
    },
    Close,
}

// PathSegment
impl Hash for PathSegment {
    #[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
    #[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input
                                      // variant (or cross-type bindings that can't merge)
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash the enum variant's discriminant first to distinguish them
        discriminant(self).hash(state);

        match self {
            Self::MoveTo(p) => p.hash(state),
            Self::LineTo(p) => p.hash(state),
            Self::CurveTo {
                control1,
                control2,
                end,
            } => {
                control1.hash(state);
                control2.hash(state);
                end.hash(state);
            }
            Self::QuadTo { control, end } => {
                control.hash(state);
                end.hash(state);
            }
            Self::Arc {
                center,
                radius,
                start_angle,
                end_angle,
            } => {
                center.hash(state);
                (radius.round() as isize).hash(state);
                (start_angle.round() as isize).hash(state);
                (end_angle.round() as isize).hash(state);
            }
            Self::Close => {} // No data to hash
        }
    }
}

impl PartialEq for PathSegment {
    #[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
    #[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input
                                      // variant (or cross-type bindings that can't merge)
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::MoveTo(a), Self::MoveTo(b)) => a == b,
            (Self::LineTo(a), Self::LineTo(b)) => a == b,
            (
                Self::CurveTo {
                    control1: c1a,
                    control2: c2a,
                    end: ea,
                },
                Self::CurveTo {
                    control1: c1b,
                    control2: c2b,
                    end: eb,
                },
            ) => c1a == c1b && c2a == c2b && ea == eb,
            (
                Self::QuadTo {
                    control: ca,
                    end: ea,
                },
                Self::QuadTo {
                    control: cb,
                    end: eb,
                },
            ) => ca == cb && ea == eb,
            (
                Self::Arc {
                    center: ca,
                    radius: ra,
                    start_angle: sa_a,
                    end_angle: ea_a,
                },
                Self::Arc {
                    center: cb,
                    radius: rb,
                    start_angle: sa_b,
                    end_angle: ea_b,
                },
            ) => ca == cb && round_eq(*ra, *rb) && round_eq(*sa_a, *sa_b) && round_eq(*ea_a, *ea_b),
            (Self::Close, Self::Close) => true,
            _ => false, // Variants are different
        }
    }
}

impl Eq for PathSegment {}

#[derive(Debug, Default, Clone, Copy)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl PartialEq for Rect {
    fn eq(&self, other: &Self) -> bool {
        round_eq(self.x, other.x)
            && round_eq(self.y, other.y)
            && round_eq(self.width, other.width)
            && round_eq(self.height, other.height)
    }
}
impl Eq for Rect {}

impl Hash for Rect {
    #[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
    fn hash<H: Hasher>(&self, state: &mut H) {
        // The order in which you hash the fields matters.
        // A consistent order is crucial.
        (self.x.round() as isize).hash(state);
        (self.y.round() as isize).hash(state);
        (self.width.round() as isize).hash(state);
        (self.height.round() as isize).hash(state);
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl PartialOrd for Size {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Size {
    #[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
    fn cmp(&self, other: &Self) -> Ordering {
        (self.width.round() as isize)
            .cmp(&(other.width.round() as isize))
            .then_with(|| (self.height.round() as isize).cmp(&(other.height.round() as isize)))
    }
}

// Size
impl Hash for Size {
    #[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.width.round() as isize).hash(state);
        (self.height.round() as isize).hash(state);
    }
}
impl PartialEq for Size {
    fn eq(&self, other: &Self) -> bool {
        round_eq(self.width, other.width) && round_eq(self.height, other.height)
    }
}
impl Eq for Size {}

impl Size {
    #[must_use]
    pub const fn zero() -> Self {
        Self::new(0.0, 0.0)
    }
    #[must_use]
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialOrd)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

// Point
impl Hash for Point {
    #[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.x.round() as isize).hash(state);
        (self.y.round() as isize).hash(state);
    }
}

impl PartialEq for Point {
    fn eq(&self, other: &Self) -> bool {
        round_eq(self.x, other.x) && round_eq(self.y, other.y)
    }
}

impl Eq for Point {}

#[derive(Debug, Clone, PartialOrd)]
pub enum ShapeDefinition {
    Rectangle {
        size: Size,
        corner_radius: Option<f32>,
    },
    Circle {
        radius: f32,
    },
    Ellipse {
        radii: Size,
    },
    Polygon {
        points: Vec<Point>,
    },
    Path {
        segments: Vec<PathSegment>,
    },
}

// ShapeDefinition
impl Hash for ShapeDefinition {
    #[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        match self {
            Self::Rectangle {
                size,
                corner_radius,
            } => {
                size.hash(state);
                corner_radius.map(|r| r.round() as isize).hash(state);
            }
            Self::Circle { radius } => {
                (radius.round() as isize).hash(state);
            }
            Self::Ellipse { radii } => {
                radii.hash(state);
            }
            Self::Polygon { points } => {
                // Since Point implements Hash, we can hash the Vec directly.
                points.hash(state);
            }
            Self::Path { segments } => {
                // Same for Vec<PathSegment>
                segments.hash(state);
            }
        }
    }
}

impl PartialEq for ShapeDefinition {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                Self::Rectangle {
                    size: s1,
                    corner_radius: r1,
                },
                Self::Rectangle {
                    size: s2,
                    corner_radius: r2,
                },
            ) => {
                s1 == s2
                    && match (r1, r2) {
                        (None, None) => true,
                        (Some(v1), Some(v2)) => round_eq(*v1, *v2),
                        _ => false,
                    }
            }
            (Self::Circle { radius: r1 }, Self::Circle { radius: r2 }) => round_eq(*r1, *r2),
            (Self::Ellipse { radii: r1 }, Self::Ellipse { radii: r2 }) => r1 == r2,
            (Self::Polygon { points: p1 }, Self::Polygon { points: p2 }) => p1 == p2,
            (Self::Path { segments: s1 }, Self::Path { segments: s2 }) => s1 == s2,
            _ => false,
        }
    }
}
impl Eq for ShapeDefinition {}

impl ShapeDefinition {
    /// Calculates the bounding box size for the shape.
    #[must_use]
    pub fn get_size(&self) -> Size {
        match self {
            // The size is explicitly defined.
            Self::Rectangle { size, .. } => *size,

            // The bounding box of a circle is a square with sides equal to the diameter.
            Self::Circle { radius } => {
                let diameter = radius * 2.0;
                Size::new(diameter, diameter)
            }

            // The bounding box of an ellipse has width and height equal to twice its radii.
            Self::Ellipse { radii } => Size::new(radii.width * 2.0, radii.height * 2.0),

            // For a polygon, we must find the min/max coordinates to get the bounds.
            Self::Polygon { points } => calculate_bounding_box_size(points),

            // For a path, we find the bounding box of all its anchor and control points.
            //
            // NOTE: This is a common and fast approximation. The true bounding box of
            // bezier curves can be slightly smaller than the box containing their control
            // points. For pixel-perfect results, one would need to calculate the
            // curve's extrema.
            Self::Path { segments } => {
                let mut points = Vec::new();
                let mut current_pos = Point { x: 0.0, y: 0.0 };

                for segment in segments {
                    match segment {
                        PathSegment::MoveTo(p) | PathSegment::LineTo(p) => {
                            points.push(*p);
                            current_pos = *p;
                        }
                        PathSegment::QuadTo { control, end } => {
                            points.push(current_pos);
                            points.push(*control);
                            points.push(*end);
                            current_pos = *end;
                        }
                        PathSegment::CurveTo {
                            control1,
                            control2,
                            end,
                        } => {
                            points.push(current_pos);
                            points.push(*control1);
                            points.push(*control2);
                            points.push(*end);
                            current_pos = *end;
                        }
                        PathSegment::Arc {
                            center,
                            radius,
                            start_angle,
                            end_angle,
                        } => {
                            // 1. Calculate and add the arc's start and end points to the list.
                            let start_point = Point {
                                x: center.x + radius * start_angle.cos(),
                                y: center.y + radius * start_angle.sin(),
                            };
                            let end_point = Point {
                                x: center.x + radius * end_angle.cos(),
                                y: center.y + radius * end_angle.sin(),
                            };
                            points.push(start_point);
                            points.push(end_point);

                            // 2. Normalize the angles to handle cases where the arc crosses the
                            //    0-radian line.
                            // This ensures we can iterate forward from a start to an end angle.
                            let mut normalized_end = *end_angle;
                            #[allow(clippy::while_float)]
                            // intentional bounded float loop (angle-wrap / pixel-step); an integer
                            // counter would be artificial
                            while normalized_end < *start_angle {
                                normalized_end += 2.0 * std::f32::consts::PI;
                            }

                            // 3. Find the first cardinal point (multiples of PI/2) at or after the
                            //    start angle.
                            let mut check_angle = (*start_angle / std::f32::consts::FRAC_PI_2)
                                .ceil()
                                * std::f32::consts::FRAC_PI_2;

                            // 4. Iterate through all cardinal points that fall within the arc's
                            //    sweep and add them.
                            // These points define the maximum extent of the arc's bounding box.
                            #[allow(clippy::while_float)]
                            // intentional bounded float loop (angle-wrap / pixel-step); an integer
                            // counter would be artificial
                            while check_angle < normalized_end {
                                points.push(Point {
                                    x: center.x + radius * check_angle.cos(),
                                    y: center.y + radius * check_angle.sin(),
                                });
                                check_angle += std::f32::consts::FRAC_PI_2;
                            }

                            // 5. The end of the arc is the new current position for subsequent path
                            //    segments.
                            current_pos = end_point;
                        }
                        PathSegment::Close => {
                            // No new points are added for closing the path
                        }
                    }
                }
                calculate_bounding_box_size(&points)
            }
        }
    }
}

/// Helper function to calculate the size of the bounding box enclosing a set of points.
pub(super) fn calculate_bounding_box_size(points: &[Point]) -> Size {
    if points.is_empty() {
        return Size::zero();
    }

    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;

    for point in points {
        min_x = min_x.min(point.x);
        max_x = max_x.max(point.x);
        min_y = min_y.min(point.y);
        max_y = max_y.max(point.y);
    }

    // Handle case where points might be collinear or a single point
    if min_x > max_x || min_y > max_y {
        return Size::zero();
    }

    Size::new(max_x - min_x, max_y - min_y)
}

#[derive(Debug, Clone, PartialOrd)]
pub struct Stroke {
    pub color: ColorU,
    pub width: f32,
    pub dash_pattern: Option<Vec<f32>>,
}

// Stroke
impl Hash for Stroke {
    #[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.color.hash(state);
        (self.width.round() as isize).hash(state);

        // Manual hashing for Option<Vec<f32>>
        match &self.dash_pattern {
            None => 0u8.hash(state), // Hash a discriminant for None
            Some(pattern) => {
                1u8.hash(state); // Hash a discriminant for Some
                pattern.len().hash(state); // Hash the length
                for &val in pattern {
                    (val.round() as isize).hash(state); // Hash each rounded value
                }
            }
        }
    }
}

impl PartialEq for Stroke {
    fn eq(&self, other: &Self) -> bool {
        if self.color != other.color || !round_eq(self.width, other.width) {
            return false;
        }
        match (&self.dash_pattern, &other.dash_pattern) {
            (None, None) => true,
            (Some(p1), Some(p2)) => {
                p1.len() == p2.len() && p1.iter().zip(p2.iter()).all(|(a, b)| round_eq(*a, *b))
            }
            _ => false,
        }
    }
}

impl Eq for Stroke {}

// Helper function to round f32 for comparison
#[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
pub(super) const fn round_eq(a: f32, b: f32) -> bool {
    (a.round() as isize) == (b.round() as isize)
}

/// Which points the rings of a polygon or a path enclose: the `fill-rule` of
/// CSS `polygon()` and `path()` (SVG's rule of the same name).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FillRule {
    /// A point is inside where the rings around it wind a non-zero number of
    /// times, so a ring drawn the other way round inside another cuts a hole
    /// and one drawn the same way does not.
    #[default]
    NonZero,
    /// A point is inside where an odd number of ring edges lie on either side
    /// of it, so every ring inside another cuts a hole.
    EvenOdd,
}

#[derive(Debug, Clone)]
pub enum ShapeBoundary {
    Rectangle(Rect),
    Circle {
        center: Point,
        radius: f32,
    },
    Ellipse {
        center: Point,
        radii: Size,
    },
    Polygon {
        points: Vec<Point>,
        fill_rule: FillRule,
    },
    Path {
        segments: Vec<PathSegment>,
        fill_rule: FillRule,
    },
}

impl ShapeBoundary {
    /// The shape moved by `dx`, `dy`.
    #[must_use]
    pub fn translated(&self, dx: f32, dy: f32) -> Self {
        let at = |p: &Point| Point {
            x: p.x + dx,
            y: p.y + dy,
        };
        match self {
            Self::Rectangle(rect) => Self::Rectangle(Rect {
                x: rect.x + dx,
                y: rect.y + dy,
                ..*rect
            }),
            Self::Circle { center, radius } => Self::Circle {
                center: at(center),
                radius: *radius,
            },
            Self::Ellipse { center, radii } => Self::Ellipse {
                center: at(center),
                radii: *radii,
            },
            Self::Polygon { points, fill_rule } => Self::Polygon {
                points: points.iter().map(at).collect(),
                fill_rule: *fill_rule,
            },
            Self::Path {
                segments,
                fill_rule,
            } => Self::Path {
                segments: segments
                    .iter()
                    .map(|segment| match segment {
                        PathSegment::MoveTo(p) => PathSegment::MoveTo(at(p)),
                        PathSegment::LineTo(p) => PathSegment::LineTo(at(p)),
                        PathSegment::CurveTo {
                            control1,
                            control2,
                            end,
                        } => PathSegment::CurveTo {
                            control1: at(control1),
                            control2: at(control2),
                            end: at(end),
                        },
                        PathSegment::QuadTo { control, end } => PathSegment::QuadTo {
                            control: at(control),
                            end: at(end),
                        },
                        PathSegment::Arc {
                            center,
                            radius,
                            start_angle,
                            end_angle,
                        } => PathSegment::Arc {
                            center: at(center),
                            radius: *radius,
                            start_angle: *start_angle,
                            end_angle: *end_angle,
                        },
                        PathSegment::Close => PathSegment::Close,
                    })
                    .collect(),
                fill_rule: *fill_rule,
            },
        }
    }

    /// Calls `visit` with every height at which the shape's horizontal extent
    /// can change its course: a rectangle's top and bottom, a circle's or an
    /// ellipse's top, centre and bottom, every vertex of a polygon or a path.
    pub fn for_each_turning_height(&self, visit: &mut impl FnMut(f32)) {
        match self {
            Self::Rectangle(rect) => {
                visit(rect.y);
                visit(rect.y + rect.height);
            }
            Self::Circle { center, radius } => {
                visit(center.y - radius);
                visit(center.y);
                visit(center.y + radius);
            }
            Self::Ellipse { center, radii } => {
                visit(center.y - radii.height);
                visit(center.y);
                visit(center.y + radii.height);
            }
            Self::Polygon { points, .. } => points.iter().for_each(|p| visit(p.y)),
            Self::Path { segments, .. } => {
                for segment in segments {
                    match segment {
                        PathSegment::MoveTo(p)
                        | PathSegment::LineTo(p)
                        | PathSegment::CurveTo { end: p, .. }
                        | PathSegment::QuadTo { end: p, .. } => visit(p.y),
                        PathSegment::Arc { center, radius, .. } => {
                            visit(center.y - radius);
                            visit(center.y + radius);
                        }
                        PathSegment::Close => {}
                    }
                }
            }
        }
    }

    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
    #[must_use]
    pub fn inflate(&self, margin: f32) -> Self {
        if margin == 0.0 {
            return self.clone();
        }
        match self {
            Self::Rectangle(rect) => Self::Rectangle(Rect {
                x: rect.x - margin,
                y: rect.y - margin,
                width: (rect.width + margin * 2.0).max(0.0),
                height: (rect.height + margin * 2.0).max(0.0),
            }),
            Self::Circle { center, radius } => Self::Circle {
                center: *center,
                radius: radius + margin,
            },
            // For simplicity, Polygon and Path inflation is not implemented here.
            // A full implementation would require a geometry library to offset the path.
            _ => self.clone(),
        }
    }
}

// ShapeBoundary
impl Hash for ShapeBoundary {
    #[allow(clippy::cast_possible_truncation)] // bounded pixel/coord/colour/glyph cast
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        match self {
            Self::Rectangle(rect) => rect.hash(state),
            Self::Circle { center, radius } => {
                center.hash(state);
                (radius.round() as isize).hash(state);
            }
            Self::Ellipse { center, radii } => {
                center.hash(state);
                radii.hash(state);
            }
            Self::Polygon { points, fill_rule } => {
                points.hash(state);
                fill_rule.hash(state);
            }
            Self::Path {
                segments,
                fill_rule,
            } => {
                segments.hash(state);
                fill_rule.hash(state);
            }
        }
    }
}
impl PartialEq for ShapeBoundary {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Rectangle(r1), Self::Rectangle(r2)) => r1 == r2,
            (
                Self::Circle {
                    center: c1,
                    radius: r1,
                },
                Self::Circle {
                    center: c2,
                    radius: r2,
                },
            ) => c1 == c2 && round_eq(*r1, *r2),
            (
                Self::Ellipse {
                    center: c1,
                    radii: r1,
                },
                Self::Ellipse {
                    center: c2,
                    radii: r2,
                },
            ) => c1 == c2 && r1 == r2,
            (
                Self::Polygon {
                    points: p1,
                    fill_rule: f1,
                },
                Self::Polygon {
                    points: p2,
                    fill_rule: f2,
                },
            ) => p1 == p2 && f1 == f2,
            (
                Self::Path {
                    segments: s1,
                    fill_rule: f1,
                },
                Self::Path {
                    segments: s2,
                    fill_rule: f2,
                },
            ) => s1 == s2 && f1 == f2,
            _ => false,
        }
    }
}
impl Eq for ShapeBoundary {}

impl ShapeBoundary {
    /// The shape `css_shape` measured against `reference` (its reference box
    /// and the sizes its relative lengths take), placed with the box's top
    /// left corner at `origin`.
    ///
    /// An inset rounded by `round` follows its rounded corners. A path whose
    /// data cannot be read takes the whole reference box: a shape that means
    /// nothing cuts nothing.
    #[must_use]
    pub fn from_css_shape(
        css_shape: &azul_css::shape::CssShape,
        origin: Point,
        reference: &azul_css::shape::ShapeReference,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Self {
        use azul_css::shape::ResolvedShape;

        let at = |(x, y): (f32, f32)| Point {
            x: origin.x + x,
            y: origin.y + y,
        };
        let result = match css_shape.resolve(reference) {
            ResolvedShape::Circle { center, radius } => Self::Circle {
                center: at(center),
                radius: radius.max(0.0),
            },
            ResolvedShape::Ellipse {
                center,
                radius_x,
                radius_y,
            } => Self::Ellipse {
                center: at(center),
                radii: Size::new(radius_x.max(0.0), radius_y.max(0.0)),
            },
            ResolvedShape::Polygon { fill_rule, points } => Self::Polygon {
                points: points.into_iter().map(at).collect(),
                fill_rule: fill_rule.into(),
            },
            ResolvedShape::Inset {
                x,
                y,
                width,
                height,
                radius,
            } => {
                let rect = Rect {
                    x: origin.x + x,
                    y: origin.y + y,
                    width: width.max(0.0),
                    height: height.max(0.0),
                };
                match radius.map(|r| r.min(rect.width / 2.0).min(rect.height / 2.0)) {
                    Some(r) if r > 0.0 => Self::Polygon {
                        points: rounded_rect_points(rect, r),
                        fill_rule: FillRule::NonZero,
                    },
                    _ => Self::Rectangle(rect),
                }
            }
            ResolvedShape::Path { fill_rule, data } => {
                let box_rect = Rect {
                    x: origin.x,
                    y: origin.y,
                    width: reference.width,
                    height: reference.height,
                };
                let segments = azul_core::path_parser::parse_svg_path_d(data).map_or_else(
                    |_| Vec::new(),
                    |multipolygon| flatten_svg_to_path_segments(&multipolygon, box_rect),
                );
                if segments.is_empty() {
                    Self::Rectangle(box_rect)
                } else {
                    Self::Path {
                        segments,
                        fill_rule: fill_rule.into(),
                    }
                }
            }
        };
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "[ShapeBoundary::from_css_shape] {css_shape:?} in {reference:?} at {origin:?}: \
                 {result:?}"
            )));
        }
        result
    }
}

impl From<azul_css::shape::ShapeFillRule> for FillRule {
    fn from(rule: azul_css::shape::ShapeFillRule) -> Self {
        match rule {
            azul_css::shape::ShapeFillRule::NonZero => Self::NonZero,
            azul_css::shape::ShapeFillRule::EvenOdd => Self::EvenOdd,
        }
    }
}

/// The outline of `rect` with its corners rounded by `radius`, each quarter
/// circle sampled at eight points, clockwise from the top edge.
fn rounded_rect_points(rect: Rect, radius: f32) -> Vec<Point> {
    const STEPS: u16 = 8;
    let corners = [
        // centre of the corner's circle, and the angle its arc starts at
        (rect.x + rect.width - radius, rect.y + radius, -core::f32::consts::FRAC_PI_2),
        (rect.x + rect.width - radius, rect.y + rect.height - radius, 0.0),
        (rect.x + radius, rect.y + rect.height - radius, core::f32::consts::FRAC_PI_2),
        (rect.x + radius, rect.y + radius, core::f32::consts::PI),
    ];
    let mut points = Vec::with_capacity(corners.len() * usize::from(STEPS + 1));
    for (cx, cy, start) in corners {
        for step in 0..=STEPS {
            let angle = (f32::from(step) / f32::from(STEPS))
                .mul_add(core::f32::consts::FRAC_PI_2, start);
            points.push(Point {
                x: radius.mul_add(angle.cos(), cx),
                y: radius.mul_add(angle.sin(), cy),
            });
        }
    }
    points
}
