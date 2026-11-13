//! CSS basic shapes for `shape-inside`, `shape-outside` and `clip-path`.
//!
//! A shape keeps its lengths as written - pixels, points, ems, percentages -
//! because a percentage or a relative length means nothing until the box it
//! is measured against is known. [`CssShape::resolve`] measures them against
//! a [`ShapeReference`] at layout time.
//!
//! These types are C-compatible (repr(C)) for use across FFI boundaries.

use alloc::{string::String, vec::Vec};

use crate::{
    corety::AzString,
    props::basic::{
        length::SizeMetric,
        pixel::{OptionPixelValue, PixelValue},
    },
};

/// Which points the rings of a polygon or a path enclose: the `<fill-rule>`
/// of `polygon()` and `path()`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(C)]
pub enum ShapeFillRule {
    /// Inside where the rings around a point wind a non-zero number of times:
    /// a ring drawn the other way round inside another cuts a hole.
    #[default]
    NonZero,
    /// Inside where an odd number of ring edges lie on either side of a
    /// point: every ring inside another cuts a hole.
    EvenOdd,
}

impl ShapeFillRule {
    /// The CSS keyword.
    #[must_use]
    pub const fn as_css_keyword(self) -> &'static str {
        match self {
            Self::NonZero => "nonzero",
            Self::EvenOdd => "evenodd",
        }
    }
}

/// A vertex of a `polygon()`: two `<length-percentage>`s, the percentages of
/// the reference box's width and height.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(C)]
pub struct ShapePoint {
    pub x: PixelValue,
    pub y: PixelValue,
}

impl_option!(
    ShapePoint,
    OptionShapePoint,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd]
);

impl ShapePoint {
    #[must_use]
    pub const fn new(x: PixelValue, y: PixelValue) -> Self {
        Self { x, y }
    }

    /// The point `x`, `y` in pixels.
    #[must_use]
    pub fn px(x: f32, y: f32) -> Self {
        Self::new(PixelValue::px(x), PixelValue::px(y))
    }

    #[must_use]
    pub const fn zero() -> Self {
        Self::new(PixelValue::zero(), PixelValue::zero())
    }
}

impl_vec!(
    ShapePoint,
    ShapePointVec,
    ShapePointVecDestructor,
    ShapePointVecDestructorType,
    ShapePointVecSlice,
    OptionShapePoint
);
impl_vec_debug!(ShapePoint, ShapePointVec);
impl_vec_partialord!(ShapePoint, ShapePointVec);
impl_vec_ord!(ShapePoint, ShapePointVec);
impl_vec_clone!(ShapePoint, ShapePointVec, ShapePointVecDestructor);
impl_vec_partialeq!(ShapePoint, ShapePointVec);
impl_vec_eq!(ShapePoint, ShapePointVec);
impl_vec_hash!(ShapePoint, ShapePointVec);

/// One coordinate of a `<position>`: a length or percentage from the start
/// edge of the reference box (its left or top), or from its end edge (its
/// right or bottom), as in `right 10px bottom 20%`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(C)]
pub struct ShapeOffset {
    /// Whether the offset is measured from the end edge.
    pub from_end: bool,
    /// How far from that edge.
    pub offset: PixelValue,
}

impl ShapeOffset {
    /// `offset` from the start edge.
    #[must_use]
    pub const fn from_start(offset: PixelValue) -> Self {
        Self {
            from_end: false,
            offset,
        }
    }

    /// `offset` from the end edge.
    #[must_use]
    pub const fn from_end(offset: PixelValue) -> Self {
        Self {
            from_end: true,
            offset,
        }
    }
}

/// The centre of a `circle()` or an `ellipse()`: a `<position>` in the
/// reference box, its centre unless said otherwise.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct ShapePosition {
    pub x: ShapeOffset,
    pub y: ShapeOffset,
}

impl Default for ShapePosition {
    fn default() -> Self {
        Self::center()
    }
}

impl ShapePosition {
    /// The centre of the reference box, `50% 50%`.
    #[must_use]
    pub const fn center() -> Self {
        Self {
            x: ShapeOffset::from_start(PixelValue::const_percent(50)),
            y: ShapeOffset::from_start(PixelValue::const_percent(50)),
        }
    }

    /// The point `x`, `y` px from the reference box's top left corner.
    #[must_use]
    pub fn px(x: f32, y: f32) -> Self {
        Self {
            x: ShapeOffset::from_start(PixelValue::px(x)),
            y: ShapeOffset::from_start(PixelValue::px(y)),
        }
    }
}

#[allow(variant_size_differences)]
// repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size
// disparity accepted
/// The radius of a `circle()`, or one radius of an `ellipse()`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(C, u8)]
pub enum ShapeRadius {
    /// A length or a percentage: of the box's diagonal over the square root
    /// of two for a circle, of its width or height for an ellipse.
    Length(PixelValue),
    /// Out to the side of the reference box closest to the centre.
    #[default]
    ClosestSide,
    /// Out to the side of the reference box farthest from the centre.
    FarthestSide,
}

/// A `circle()`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(C)]
pub struct ShapeCircle {
    pub center: ShapePosition,
    pub radius: ShapeRadius,
}

/// An `ellipse()`.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
#[repr(C)]
pub struct ShapeEllipse {
    pub center: ShapePosition,
    pub radius_x: ShapeRadius,
    pub radius_y: ShapeRadius,
}

/// A `polygon()`: one ring of vertices, filled by its fill rule.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct ShapePolygon {
    pub fill_rule: ShapeFillRule,
    pub points: ShapePointVec,
}

/// An `inset()`: a rectangle the given distances inside the reference box's
/// edges, its corners rounded by one radius when `round` gives one.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct ShapeInset {
    pub inset_top: PixelValue,
    pub inset_right: PixelValue,
    pub inset_bottom: PixelValue,
    pub inset_left: PixelValue,
    pub border_radius: OptionPixelValue,
}

/// A `path()`: SVG path data in the reference box's pixels, its rings
/// filled by the fill rule.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct ShapePath {
    pub fill_rule: ShapeFillRule,
    pub data: AzString,
}

/// A CSS basic shape, as `shape-inside`, `shape-outside` and `clip-path` take
/// it.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C, u8)]
pub enum CssShape {
    Circle(ShapeCircle),
    Ellipse(ShapeEllipse),
    Polygon(ShapePolygon),
    Inset(ShapeInset),
    Path(ShapePath),
}

impl_option!(
    CssShape,
    OptionCssShape,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

/// What the lengths of a shape are measured against: its reference box, and
/// the font and viewport sizes relative lengths take.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct ShapeReference {
    /// The width of the reference box, in px.
    pub width: f32,
    /// The height of the reference box, in px.
    pub height: f32,
    /// The element's font size, what an `em` is.
    pub font_size: f32,
    /// The root element's font size, what a `rem` is.
    pub root_font_size: f32,
    /// The viewport's width, what `100vw` is.
    pub viewport_width: f32,
    /// The viewport's height, what `100vh` is.
    pub viewport_height: f32,
}

impl ShapeReference {
    /// A `width` x `height` reference box whose relative lengths take the
    /// initial font size (16px) and a viewport of the box's size.
    #[must_use]
    pub const fn of_box(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            font_size: 16.0,
            root_font_size: 16.0,
            viewport_width: width,
            viewport_height: height,
        }
    }

    /// `value` in px, a percentage being of `percent_of`.
    #[must_use]
    pub fn length(&self, value: PixelValue, percent_of: f32) -> f32 {
        // The product before the division keeps a whole percentage of a whole
        // size whole: 30% of 100px is 30px, not 30.000002px.
        let number = value.number.get();
        match value.metric {
            SizeMetric::Percent => number * percent_of / 100.0,
            SizeMetric::Em => number * self.font_size,
            SizeMetric::Rem => number * self.root_font_size,
            SizeMetric::Vw => number * self.viewport_width / 100.0,
            SizeMetric::Vh => number * self.viewport_height / 100.0,
            SizeMetric::Vmin => number * self.viewport_width.min(self.viewport_height) / 100.0,
            SizeMetric::Vmax => number * self.viewport_width.max(self.viewport_height) / 100.0,
            SizeMetric::Px | SizeMetric::Pt | SizeMetric::In | SizeMetric::Cm | SizeMetric::Mm => {
                value.to_pixels_internal(0.0, 0.0, 0.0)
            }
        }
    }

    /// Where `offset` lies along an axis `extent` long.
    fn offset(&self, offset: ShapeOffset, extent: f32) -> f32 {
        let from_start = self.length(offset.offset, extent);
        if offset.from_end {
            extent - from_start
        } else {
            from_start
        }
    }

    /// Where `position` lies in the box.
    fn position(&self, position: ShapePosition) -> (f32, f32) {
        (
            self.offset(position.x, self.width),
            self.offset(position.y, self.height),
        )
    }
}

/// A shape measured in px from its reference box's top left corner.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedShape<'a> {
    Circle {
        center: (f32, f32),
        radius: f32,
    },
    Ellipse {
        center: (f32, f32),
        radius_x: f32,
        radius_y: f32,
    },
    Polygon {
        fill_rule: ShapeFillRule,
        points: Vec<(f32, f32)>,
    },
    /// The rectangle `x`, `y`, `width` x `height`; a width or height the
    /// insets overlap is negative.
    Inset {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        radius: Option<f32>,
    },
    Path {
        fill_rule: ShapeFillRule,
        data: &'a str,
    },
}

impl CssShape {
    /// A circle of `radius` around `center`.
    #[must_use]
    pub const fn circle(center: ShapePosition, radius: ShapeRadius) -> Self {
        Self::Circle(ShapeCircle { center, radius })
    }

    /// An ellipse of the radii `radius_x` and `radius_y` around `center`.
    #[must_use]
    pub const fn ellipse(center: ShapePosition, radius_x: ShapeRadius, radius_y: ShapeRadius) -> Self {
        Self::Ellipse(ShapeEllipse {
            center,
            radius_x,
            radius_y,
        })
    }

    /// A polygon through `points`, filled by `fill_rule`.
    #[must_use]
    pub const fn polygon(fill_rule: ShapeFillRule, points: ShapePointVec) -> Self {
        Self::Polygon(ShapePolygon { fill_rule, points })
    }

    /// The rectangle the given distances inside the reference box.
    #[must_use]
    pub const fn inset(top: PixelValue, right: PixelValue, bottom: PixelValue, left: PixelValue) -> Self {
        Self::Inset(ShapeInset {
            inset_top: top,
            inset_right: right,
            inset_bottom: bottom,
            inset_left: left,
            border_radius: OptionPixelValue::None,
        })
    }

    /// The rectangle the given distances inside the reference box, its
    /// corners rounded by `radius`.
    #[must_use]
    pub const fn inset_rounded(
        top: PixelValue,
        right: PixelValue,
        bottom: PixelValue,
        left: PixelValue,
        radius: PixelValue,
    ) -> Self {
        Self::Inset(ShapeInset {
            inset_top: top,
            inset_right: right,
            inset_bottom: bottom,
            inset_left: left,
            border_radius: OptionPixelValue::Some(radius),
        })
    }

    /// The shape of the SVG path `data`, filled by `fill_rule`.
    #[must_use]
    pub fn path(fill_rule: ShapeFillRule, data: &str) -> Self {
        Self::Path(ShapePath {
            fill_rule,
            data: AzString::from(data),
        })
    }

    /// The shape measured against `reference` (CSS Shapes 1, 3.1 basic
    /// shapes).
    ///
    /// A polygon's or a position's percentages are of the box's width
    /// horizontally and of its height vertically; a circle's radius
    /// percentage is of the box's diagonal over the square root of two, and
    /// `closest-side` / `farthest-side` reach to the box side closest to or
    /// farthest from the centre (for an ellipse, along each axis alone); an
    /// inset's top and bottom percentages are of the height, its left and
    /// right ones and its corner radius of the width.
    #[must_use]
    pub fn resolve(&self, reference: &ShapeReference) -> ResolvedShape<'_> {
        let (width, height) = (reference.width, reference.height);
        match self {
            Self::Circle(circle) => {
                let center = reference.position(circle.center);
                let (cx, cy) = center;
                let sides = [cx.abs(), (width - cx).abs(), cy.abs(), (height - cy).abs()];
                let radius = match circle.radius {
                    ShapeRadius::Length(length) => reference.length(
                        length,
                        width.hypot(height) / core::f32::consts::SQRT_2,
                    ),
                    ShapeRadius::ClosestSide => sides.into_iter().fold(f32::MAX, f32::min),
                    ShapeRadius::FarthestSide => sides.into_iter().fold(0.0, f32::max),
                };
                ResolvedShape::Circle { center, radius }
            }
            Self::Ellipse(ellipse) => {
                let center = reference.position(ellipse.center);
                let axis = |radius: ShapeRadius, at: f32, extent: f32| match radius {
                    ShapeRadius::Length(length) => reference.length(length, extent),
                    ShapeRadius::ClosestSide => at.abs().min((extent - at).abs()),
                    ShapeRadius::FarthestSide => at.abs().max((extent - at).abs()),
                };
                ResolvedShape::Ellipse {
                    center,
                    radius_x: axis(ellipse.radius_x, center.0, width),
                    radius_y: axis(ellipse.radius_y, center.1, height),
                }
            }
            Self::Polygon(polygon) => ResolvedShape::Polygon {
                fill_rule: polygon.fill_rule,
                points: polygon
                    .points
                    .as_ref()
                    .iter()
                    .map(|p| (reference.length(p.x, width), reference.length(p.y, height)))
                    .collect(),
            },
            Self::Inset(inset) => {
                let top = reference.length(inset.inset_top, height);
                let right = reference.length(inset.inset_right, width);
                let bottom = reference.length(inset.inset_bottom, height);
                let left = reference.length(inset.inset_left, width);
                ResolvedShape::Inset {
                    x: left,
                    y: top,
                    width: width - left - right,
                    height: height - top - bottom,
                    radius: inset
                        .border_radius
                        .into_option()
                        .map(|r| reference.length(r, width)),
                }
            }
            Self::Path(path) => ResolvedShape::Path {
                fill_rule: path.fill_rule,
                data: path.data.as_str(),
            },
        }
    }

    /// The shape as CSS text that parses back to it.
    #[must_use]
    pub fn print_as_css_value(&self) -> String {
        use alloc::format;
        let fill_rule = |rule: ShapeFillRule| match rule {
            ShapeFillRule::NonZero => String::new(),
            ShapeFillRule::EvenOdd => String::from("evenodd, "),
        };
        match self {
            Self::Circle(ShapeCircle { center, radius }) => {
                format!(
                    "circle({} at {})",
                    print_radius(*radius),
                    print_position(*center)
                )
            }
            Self::Ellipse(ShapeEllipse {
                center,
                radius_x,
                radius_y,
            }) => format!(
                "ellipse({} {} at {})",
                print_radius(*radius_x),
                print_radius(*radius_y),
                print_position(*center)
            ),
            Self::Polygon(ShapePolygon { fill_rule: rule, points }) => {
                let pts: Vec<String> = points
                    .as_ref()
                    .iter()
                    .map(|p| format!("{} {}", p.x, p.y))
                    .collect();
                format!("polygon({}{})", fill_rule(*rule), pts.join(", "))
            }
            Self::Inset(ShapeInset {
                inset_top,
                inset_right,
                inset_bottom,
                inset_left,
                border_radius,
            }) => {
                let base = format!("inset({inset_top} {inset_right} {inset_bottom} {inset_left}");
                match border_radius {
                    OptionPixelValue::Some(r) => format!("{base} round {r})"),
                    OptionPixelValue::None => format!("{base})"),
                }
            }
            Self::Path(ShapePath {
                fill_rule: rule,
                data,
            }) => {
                let mut quoted = String::with_capacity(data.as_str().len() + 2);
                quoted.push('"');
                for ch in data.as_str().chars() {
                    if matches!(ch, '"' | '\\') {
                        quoted.push('\\');
                    }
                    quoted.push(ch);
                }
                quoted.push('"');
                format!("path({}{quoted})", fill_rule(*rule))
            }
        }
    }

    /// The Rust code that builds this shape.
    #[cfg(feature = "codegen")]
    #[must_use]
    pub fn format_as_rust_code(&self) -> String {
        use alloc::format;

        use crate::codegen::format::format_pixel_value as px;
        let radius = |r: ShapeRadius| match r {
            ShapeRadius::Length(l) => format!("ShapeRadius::Length({})", px(&l)),
            ShapeRadius::ClosestSide => String::from("ShapeRadius::ClosestSide"),
            ShapeRadius::FarthestSide => String::from("ShapeRadius::FarthestSide"),
        };
        let offset = |o: ShapeOffset| {
            format!(
                "ShapeOffset {{ from_end: {}, offset: {} }}",
                o.from_end,
                px(&o.offset)
            )
        };
        let position = |p: ShapePosition| {
            format!(
                "ShapePosition {{ x: {}, y: {} }}",
                offset(p.x),
                offset(p.y)
            )
        };
        let fill_rule = |rule: ShapeFillRule| match rule {
            ShapeFillRule::NonZero => "ShapeFillRule::NonZero",
            ShapeFillRule::EvenOdd => "ShapeFillRule::EvenOdd",
        };
        match self {
            Self::Circle(c) => format!(
                "CssShape::Circle(ShapeCircle {{ center: {}, radius: {} }})",
                position(c.center),
                radius(c.radius)
            ),
            Self::Ellipse(e) => format!(
                "CssShape::Ellipse(ShapeEllipse {{ center: {}, radius_x: {}, radius_y: {} }})",
                position(e.center),
                radius(e.radius_x),
                radius(e.radius_y)
            ),
            Self::Polygon(p) => {
                let pts: Vec<String> = p
                    .points
                    .as_ref()
                    .iter()
                    .map(|pt| format!("ShapePoint::new({}, {})", px(&pt.x), px(&pt.y)))
                    .collect();
                format!(
                    "CssShape::Polygon(ShapePolygon {{ fill_rule: {}, points: vec![{}].into() }})",
                    fill_rule(p.fill_rule),
                    pts.join(", ")
                )
            }
            Self::Inset(i) => {
                let br = match i.border_radius {
                    OptionPixelValue::Some(r) => format!("OptionPixelValue::Some({})", px(&r)),
                    OptionPixelValue::None => String::from("OptionPixelValue::None"),
                };
                format!(
                    "CssShape::Inset(ShapeInset {{ inset_top: {}, inset_right: {}, inset_bottom: \
                     {}, inset_left: {}, border_radius: {br} }})",
                    px(&i.inset_top),
                    px(&i.inset_right),
                    px(&i.inset_bottom),
                    px(&i.inset_left)
                )
            }
            Self::Path(p) => format!(
                "CssShape::Path(ShapePath {{ fill_rule: {}, data: AzString::from_const_str({:?}) }})",
                fill_rule(p.fill_rule),
                p.data.as_str()
            ),
        }
    }
}

/// A radius as CSS text.
fn print_radius(radius: ShapeRadius) -> String {
    match radius {
        ShapeRadius::Length(length) => alloc::format!("{length}"),
        ShapeRadius::ClosestSide => String::from("closest-side"),
        ShapeRadius::FarthestSide => String::from("farthest-side"),
    }
}

/// A position as CSS text: two offsets, or the four-value form naming the
/// edges when one counts from an end edge.
fn print_position(position: ShapePosition) -> String {
    let ShapePosition { x, y } = position;
    if x.from_end || y.from_end {
        alloc::format!(
            "{} {} {} {}",
            if x.from_end { "right" } else { "left" },
            x.offset,
            if y.from_end { "bottom" } else { "top" },
            y.offset
        )
    } else {
        alloc::format!("{} {}", x.offset, y.offset)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)] // exact values of exactly representable sums

    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn a_percentage_of_a_polygon_point_is_of_the_reference_box() {
        let shape = CssShape::polygon(
            ShapeFillRule::NonZero,
            alloc::vec![
                ShapePoint::new(PixelValue::percent(50.0), PixelValue::percent(25.0)),
                ShapePoint::new(PixelValue::px(10.0), PixelValue::const_pt(30)),
                ShapePoint::new(PixelValue::em(2.0), PixelValue::percent(100.0)),
            ]
            .into(),
        );
        let ResolvedShape::Polygon { points, .. } = shape.resolve(&ShapeReference::of_box(200.0, 80.0))
        else {
            panic!("a polygon resolves to a polygon");
        };
        assert_eq!(points[0], (100.0, 20.0));
        assert!(close(points[1].0, 10.0) && close(points[1].1, 40.0), "30pt is 40px: {points:?}");
        assert_eq!(points[2], (32.0, 80.0), "2em at 16px");
    }

    #[test]
    fn a_circle_defaults_to_the_closest_side_around_the_centre() {
        let shape = CssShape::circle(ShapePosition::center(), ShapeRadius::ClosestSide);
        let resolved = shape.resolve(&ShapeReference::of_box(200.0, 100.0));
        assert_eq!(
            resolved,
            ResolvedShape::Circle {
                center: (100.0, 50.0),
                radius: 50.0
            }
        );
        let far = CssShape::circle(ShapePosition::center(), ShapeRadius::FarthestSide);
        assert!(matches!(
            far.resolve(&ShapeReference::of_box(200.0, 100.0)),
            ResolvedShape::Circle { radius, .. } if close(radius, 100.0)
        ));
    }

    #[test]
    fn a_circles_percentage_radius_is_of_the_normalised_diagonal() {
        let shape = CssShape::circle(
            ShapePosition::center(),
            ShapeRadius::Length(PixelValue::percent(50.0)),
        );
        // sqrt(300^2 + 400^2) / sqrt(2) = 500 / 1.41421 = 353.55
        let ResolvedShape::Circle { radius, .. } = shape.resolve(&ShapeReference::of_box(300.0, 400.0))
        else {
            panic!("a circle resolves to a circle");
        };
        assert!(close(radius, 176.7767), "{radius}");
    }

    #[test]
    fn a_position_from_the_end_edges_counts_back_from_them() {
        let shape = CssShape::circle(
            ShapePosition {
                x: ShapeOffset::from_end(PixelValue::px(10.0)),
                y: ShapeOffset::from_end(PixelValue::percent(25.0)),
            },
            ShapeRadius::Length(PixelValue::px(5.0)),
        );
        assert_eq!(
            shape.resolve(&ShapeReference::of_box(100.0, 200.0)),
            ResolvedShape::Circle {
                center: (90.0, 150.0),
                radius: 5.0
            }
        );
    }

    #[test]
    fn an_inset_takes_its_percentages_of_the_box_sides_they_cut() {
        let shape = CssShape::inset(
            PixelValue::percent(10.0),
            PixelValue::percent(10.0),
            PixelValue::px(5.0),
            PixelValue::px(20.0),
        );
        assert_eq!(
            shape.resolve(&ShapeReference::of_box(300.0, 100.0)),
            ResolvedShape::Inset {
                x: 20.0,
                y: 10.0,
                width: 250.0,
                height: 85.0,
                radius: None
            }
        );
    }

    #[test]
    fn every_shape_prints_as_css_with_its_fill_rule_and_units() {
        let path = CssShape::path(ShapeFillRule::EvenOdd, "M0 0H10V10Z");
        assert_eq!(path.print_as_css_value(), "path(evenodd, \"M0 0H10V10Z\")");
        let polygon = CssShape::polygon(
            ShapeFillRule::NonZero,
            alloc::vec![ShapePoint::new(PixelValue::percent(50.0), PixelValue::const_pt(3))].into(),
        );
        assert_eq!(polygon.print_as_css_value(), "polygon(50% 3pt)");
        let circle = CssShape::circle(ShapePosition::center(), ShapeRadius::ClosestSide);
        assert_eq!(circle.print_as_css_value(), "circle(closest-side at 50% 50%)");
    }
}
