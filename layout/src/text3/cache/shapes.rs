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

#[derive(Debug, Clone)]
pub enum ShapeBoundary {
    Rectangle(Rect),
    Circle { center: Point, radius: f32 },
    Ellipse { center: Point, radii: Size },
    Polygon { points: Vec<Point> },
    Path { segments: Vec<PathSegment> },
}

impl ShapeBoundary {
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
            Self::Polygon { points } => points.hash(state),
            Self::Path { segments } => segments.hash(state),
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
            (Self::Polygon { points: p1 }, Self::Polygon { points: p2 }) => p1 == p2,
            (Self::Path { segments: s1 }, Self::Path { segments: s2 }) => s1 == s2,
            _ => false,
        }
    }
}
impl Eq for ShapeBoundary {}

impl ShapeBoundary {
    /// Converts a CSS shape (from azul-css) to a layout engine `ShapeBoundary`
    ///
    /// # Arguments
    /// * `css_shape` - The parsed CSS shape from azul-css
    /// * `reference_box` - The containing box for resolving coordinates (from layout solver)
    ///
    /// # Returns
    /// A `ShapeBoundary` ready for use in the text layout engine
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse
                                     // routine (one branch per case)
    pub fn from_css_shape(
        css_shape: &azul_css::shape::CssShape,
        reference_box: Rect,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Self {
        use azul_css::shape::CssShape;

        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "[ShapeBoundary::from_css_shape] Input CSS shape: {css_shape:?}"
            )));
            msgs.push(LayoutDebugMessage::info(format!(
                "[ShapeBoundary::from_css_shape] Reference box: {reference_box:?}"
            )));
        }

        let result = match css_shape {
            CssShape::Circle(circle) => {
                let center = Point {
                    x: reference_box.x + circle.center.x,
                    y: reference_box.y + circle.center.y,
                };
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Circle - CSS center: ({}, {}), radius: {}",
                        circle.center.x, circle.center.y, circle.radius
                    )));
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Circle - Absolute center: ({}, {}), \
                         radius: {}",
                        center.x, center.y, circle.radius
                    )));
                }
                Self::Circle {
                    center,
                    radius: circle.radius,
                }
            }

            CssShape::Ellipse(ellipse) => {
                let center = Point {
                    x: reference_box.x + ellipse.center.x,
                    y: reference_box.y + ellipse.center.y,
                };
                let radii = Size {
                    width: ellipse.radius_x,
                    height: ellipse.radius_y,
                };
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Ellipse - center: ({}, {}), radii: ({}, \
                         {})",
                        center.x, center.y, radii.width, radii.height
                    )));
                }
                Self::Ellipse { center, radii }
            }

            CssShape::Polygon(polygon) => {
                let points = polygon
                    .points
                    .as_ref()
                    .iter()
                    .map(|pt| Point {
                        x: reference_box.x + pt.x,
                        y: reference_box.y + pt.y,
                    })
                    .collect();
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Polygon - {} points",
                        polygon.points.as_ref().len()
                    )));
                }
                Self::Polygon { points }
            }

            CssShape::Inset(inset) => {
                // Inset defines distances from reference box edges
                let x = reference_box.x + inset.inset_left;
                let y = reference_box.y + inset.inset_top;
                let width = reference_box.width - inset.inset_left - inset.inset_right;
                let height = reference_box.height - inset.inset_top - inset.inset_bottom;

                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Inset - insets: ({}, {}, {}, {})",
                        inset.inset_top, inset.inset_right, inset.inset_bottom, inset.inset_left
                    )));
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Inset - resulting rect: x={x}, y={y}, \
                         w={width}, h={height}"
                    )));
                }

                Self::Rectangle(Rect {
                    x,
                    y,
                    width: width.max(0.0),
                    height: height.max(0.0),
                })
            }

            CssShape::Path(path) => {
                // CSS `path()` value: `path.data` is a raw SVG path `d=""` string in the
                // reference-box coordinate system (origin at the reference box's top-left).
                // Parse + flatten it into `Vec<PathSegment>` (curves sampled to line
                // segments) so the scanline code in `get_shape_horizontal_spans` can
                // intersect it per line, exactly like `polygon`.
                let segments = azul_core::path_parser::parse_svg_path_d(path.data.as_str())
                    .map_or_else(
                        |_| Vec::new(),
                        |multipolygon| flatten_svg_to_path_segments(&multipolygon, reference_box),
                    );
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Path - parsed {} flattened segments",
                        segments.len()
                    )));
                }
                if segments.is_empty() {
                    // Unparseable / empty path: fall back to the reference rectangle so a
                    // shape-inside container does not collapse to zero usable space.
                    Self::Rectangle(reference_box)
                } else {
                    Self::Path { segments }
                }
            }
        };

        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "[ShapeBoundary::from_css_shape] Result: {result:?}"
            )));
        }
        result
    }
}
