//! CSS property types for direction (for gradients).

use alloc::string::String;
use core::{fmt, num::ParseFloatError};
use crate::corety::AzString;

use crate::props::{
    basic::{
        angle::{
            parse_angle_value, AngleValue, CssAngleValueParseError, CssAngleValueParseErrorOwned,
        },
        geometry::{LayoutPoint, LayoutRect},
    },
    formatter::PrintAsCssValue,
};

/// Corner or side of a rectangle, used to specify CSS gradient directions
/// (e.g. `to top right`).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum DirectionCorner {
    Right,
    Left,
    Top,
    Bottom,
    TopRight,
    TopLeft,
    BottomRight,
    BottomLeft,
}

impl fmt::Display for DirectionCorner {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Self::Right => "right",
                Self::Left => "left",
                Self::Top => "top",
                Self::Bottom => "bottom",
                Self::TopRight => "top right",
                Self::TopLeft => "top left",
                Self::BottomRight => "bottom right",
                Self::BottomLeft => "bottom left",
            }
        )
    }
}

impl PrintAsCssValue for DirectionCorner {
    fn print_as_css_value(&self) -> String {
        format!("{self}")
    }
}

impl DirectionCorner {
    #[must_use] pub const fn opposite(&self) -> Self {
        use self::DirectionCorner::{Right, Left, Top, Bottom, TopRight, BottomLeft, TopLeft, BottomRight};
        match *self {
            Right => Left,
            Left => Right,
            Top => Bottom,
            Bottom => Top,
            TopRight => BottomLeft,
            BottomLeft => TopRight,
            TopLeft => BottomRight,
            BottomRight => TopLeft,
        }
    }

    #[must_use] pub const fn combine(&self, other: &Self) -> Option<Self> {
        use self::DirectionCorner::{Right, Top, TopRight, Left, TopLeft, Bottom, BottomRight, BottomLeft};
        match (*self, *other) {
            (Right, Top) | (Top, Right) => Some(TopRight),
            (Left, Top) | (Top, Left) => Some(TopLeft),
            (Right, Bottom) | (Bottom, Right) => Some(BottomRight),
            (Left, Bottom) | (Bottom, Left) => Some(BottomLeft),
            _ => None,
        }
    }

    #[must_use] pub const fn to_point(&self, rect: &LayoutRect) -> LayoutPoint {
        use self::DirectionCorner::{Right, Left, Top, Bottom, TopRight, TopLeft, BottomRight, BottomLeft};
        match *self {
            Right => LayoutPoint {
                x: rect.size.width,
                y: rect.size.height / 2,
            },
            Left => LayoutPoint {
                x: 0,
                y: rect.size.height / 2,
            },
            Top => LayoutPoint {
                x: rect.size.width / 2,
                y: 0,
            },
            Bottom => LayoutPoint {
                x: rect.size.width / 2,
                y: rect.size.height,
            },
            TopRight => LayoutPoint {
                x: rect.size.width,
                y: 0,
            },
            TopLeft => LayoutPoint { x: 0, y: 0 },
            BottomRight => LayoutPoint {
                x: rect.size.width,
                y: rect.size.height,
            },
            BottomLeft => LayoutPoint {
                x: 0,
                y: rect.size.height,
            },
        }
    }
}

/// A pair of corners representing the start and end of a CSS gradient direction.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct DirectionCorners {
    /// The corner or side from which the gradient starts.
    pub dir_from: DirectionCorner,
    /// The corner or side at which the gradient ends.
    pub dir_to: DirectionCorner,
}

/// CSS direction (necessary for gradients). Can either be a fixed angle or
/// a direction ("to right" / "to left", etc.).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum Direction {
    Angle(AngleValue),
    FromTo(DirectionCorners),
}

impl Default for Direction {
    fn default() -> Self {
        Self::FromTo(DirectionCorners {
            dir_from: DirectionCorner::Top,
            dir_to: DirectionCorner::Bottom,
        })
    }
}

impl PrintAsCssValue for Direction {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::Angle(a) => format!("{a}"),
            Self::FromTo(d) => format!("to {}", d.dir_to), // simplified "from X to Y"
        }
    }
}

impl Direction {
    #[must_use] pub fn to_points(&self, rect: &LayoutRect) -> (LayoutPoint, LayoutPoint) {
        match self {
            Self::Angle(angle_value) => {
                // Convert the angle to start/end points on the rectangle.
                // Normalize to [0, 360) so negative angles and angles >= 360 fall
                // into the same quadrant branches below (rem_euclid is always >= 0).
                let deg = (-angle_value.to_degrees()).rem_euclid(360.0);
                let width_half = crate::cast::isize_to_f32(rect.size.width) / 2.0;
                let height_half = crate::cast::isize_to_f32(rect.size.height) / 2.0;
                let hypotenuse_len = libm::hypotf(width_half, height_half);
                let angle_to_corner = libm::atanf(height_half / width_half).to_degrees();
                let corner_angle = if deg < 90.0 {
                    90.0 - angle_to_corner
                } else if deg < 180.0 {
                    90.0 + angle_to_corner
                } else if deg < 270.0 {
                    270.0 - angle_to_corner
                } else {
                    270.0 + angle_to_corner
                };
                let angle_diff = corner_angle - deg;
                let line_length = libm::fabsf(hypotenuse_len * libm::cosf(angle_diff.to_radians()));
                let dx = libm::sinf(deg.to_radians()) * line_length;
                let dy = libm::cosf(deg.to_radians()) * line_length;
                (
                    LayoutPoint::new(
                        crate::cast::f32_to_isize(libm::roundf(width_half - dx)),
                        crate::cast::f32_to_isize(libm::roundf(height_half + dy)),
                    ),
                    LayoutPoint::new(
                        crate::cast::f32_to_isize(libm::roundf(width_half + dx)),
                        crate::cast::f32_to_isize(libm::roundf(height_half - dy)),
                    ),
                )
            }
            Self::FromTo(ft) => (ft.dir_from.to_point(rect), ft.dir_to.to_point(rect)),
        }
    }
}

// -- Parser

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CssDirectionCornerParseError<'a> {
    InvalidDirection(&'a str),
}

impl_display! { CssDirectionCornerParseError<'a>, {
    InvalidDirection(val) => format!("Invalid direction: \"{}\"", val),
}}

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum CssDirectionCornerParseErrorOwned {
    InvalidDirection(AzString),
}

impl CssDirectionCornerParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> CssDirectionCornerParseErrorOwned {
        match self {
            CssDirectionCornerParseError::InvalidDirection(s) => {
                CssDirectionCornerParseErrorOwned::InvalidDirection((*s).to_string().into())
            }
        }
    }
}

impl CssDirectionCornerParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> CssDirectionCornerParseError<'_> {
        match self {
            Self::InvalidDirection(s) => {
                CssDirectionCornerParseError::InvalidDirection(s.as_str())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CssDirectionParseError<'a> {
    Error(&'a str),
    InvalidArguments(&'a str),
    ParseFloat(ParseFloatError),
    CornerError(CssDirectionCornerParseError<'a>),
    AngleError(CssAngleValueParseError<'a>),
}

impl_display! {CssDirectionParseError<'a>, {
    Error(e) => e,
    InvalidArguments(val) => format!("Invalid arguments: \"{}\"", val),
    ParseFloat(e) => format!("Invalid value: {}", e),
    CornerError(e) => format!("Invalid corner value: {}", e),
    AngleError(e) => format!("Invalid angle value: {}", e),
}}

impl From<ParseFloatError> for CssDirectionParseError<'_> {
    fn from(e: ParseFloatError) -> Self {
        CssDirectionParseError::ParseFloat(e)
    }
}
impl_from! { CssDirectionCornerParseError<'a>, CssDirectionParseError::CornerError }
impl_from! { CssAngleValueParseError<'a>, CssDirectionParseError::AngleError }

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum CssDirectionParseErrorOwned {
    Error(AzString),
    InvalidArguments(AzString),
    ParseFloat(crate::props::basic::error::ParseFloatError),
    CornerError(CssDirectionCornerParseErrorOwned),
    AngleError(CssAngleValueParseErrorOwned),
}

impl CssDirectionParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> CssDirectionParseErrorOwned {
        match self {
            CssDirectionParseError::Error(s) => CssDirectionParseErrorOwned::Error((*s).to_string().into()),
            CssDirectionParseError::InvalidArguments(s) => {
                CssDirectionParseErrorOwned::InvalidArguments((*s).to_string().into())
            }
            CssDirectionParseError::ParseFloat(e) => {
                CssDirectionParseErrorOwned::ParseFloat(e.clone().into())
            }
            CssDirectionParseError::CornerError(e) => {
                CssDirectionParseErrorOwned::CornerError(e.to_contained())
            }
            CssDirectionParseError::AngleError(e) => {
                CssDirectionParseErrorOwned::AngleError(e.to_contained())
            }
        }
    }
}

impl CssDirectionParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> CssDirectionParseError<'_> {
        match self {
            Self::Error(s) => CssDirectionParseError::Error(s.as_str()),
            Self::InvalidArguments(s) => {
                CssDirectionParseError::InvalidArguments(s.as_str())
            }
            Self::ParseFloat(e) => {
                CssDirectionParseError::ParseFloat(e.to_std())
            }
            Self::CornerError(e) => {
                CssDirectionParseError::CornerError(e.to_shared())
            }
            Self::AngleError(e) => {
                CssDirectionParseError::AngleError(e.to_shared())
            }
        }
    }
}

#[cfg(feature = "parser")]
fn parse_direction_corner(
    input: &str,
) -> Result<DirectionCorner, CssDirectionCornerParseError<'_>> {
    match input {
        "right" => Ok(DirectionCorner::Right),
        "left" => Ok(DirectionCorner::Left),
        "top" => Ok(DirectionCorner::Top),
        "bottom" => Ok(DirectionCorner::Bottom),
        _ => Err(CssDirectionCornerParseError::InvalidDirection(input)),
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `direction` value.
pub fn parse_direction(input: &str) -> Result<Direction, CssDirectionParseError<'_>> {
    let mut input_iter = input.split_whitespace();
    let first_input = input_iter
        .next()
        .ok_or(CssDirectionParseError::Error(input))?;

    if let Ok(angle) = parse_angle_value(first_input) {
        return Ok(Direction::Angle(angle));
    }

    if first_input != "to" {
        return Err(CssDirectionParseError::InvalidArguments(input));
    }

    let components = input_iter.collect::<Vec<_>>();
    if components.is_empty() || components.len() > 2 {
        return Err(CssDirectionParseError::InvalidArguments(input));
    }

    let first_corner = parse_direction_corner(components[0])?;
    let end = if components.len() == 2 {
        let second_corner = parse_direction_corner(components[1])?;
        first_corner
            .combine(&second_corner)
            .ok_or(CssDirectionParseError::InvalidArguments(input))?
    } else {
        first_corner
    };

    Ok(Direction::FromTo(DirectionCorners {
        dir_from: end.opposite(),
        dir_to: end,
    }))
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;
    use crate::props::basic::angle::AngleValue;

    #[test]
    fn test_parse_direction_angle() {
        assert_eq!(
            parse_direction("45deg").unwrap(),
            Direction::Angle(AngleValue::deg(45.0))
        );
        assert_eq!(
            parse_direction("  -0.25turn  ").unwrap(),
            Direction::Angle(AngleValue::turn(-0.25))
        );
    }

    #[test]
    fn test_parse_direction_corners() {
        assert_eq!(
            parse_direction("to right").unwrap(),
            Direction::FromTo(DirectionCorners {
                dir_from: DirectionCorner::Left,
                dir_to: DirectionCorner::Right,
            })
        );
        assert_eq!(
            parse_direction("to top left").unwrap(),
            Direction::FromTo(DirectionCorners {
                dir_from: DirectionCorner::BottomRight,
                dir_to: DirectionCorner::TopLeft,
            })
        );
        assert_eq!(
            parse_direction("to left top").unwrap(),
            Direction::FromTo(DirectionCorners {
                dir_from: DirectionCorner::BottomRight,
                dir_to: DirectionCorner::TopLeft,
            })
        );
    }

    #[test]
    fn test_parse_direction_errors() {
        assert!(parse_direction("").is_err());
        assert!(parse_direction("to").is_err());
        assert!(parse_direction("right").is_err());
        assert!(parse_direction("to center").is_err());
        assert!(parse_direction("to top right bottom").is_err());
        assert!(parse_direction("to top top").is_err());
    }
}

#[cfg(test)]
mod autotest_generated {
    // Angles/coordinates are compared against exact literals that the code under
    // test can reproduce bit-for-bit (or with an explicit ±1 pixel tolerance).
    #![allow(clippy::float_cmp, clippy::too_many_lines)]

    use alloc::collections::BTreeSet;

    use super::*;
    use crate::props::basic::geometry::LayoutSize;

    // ---- helpers -----------------------------------------------------------

    const ALL_CORNERS: [DirectionCorner; 8] = [
        DirectionCorner::Right,
        DirectionCorner::Left,
        DirectionCorner::Top,
        DirectionCorner::Bottom,
        DirectionCorner::TopRight,
        DirectionCorner::TopLeft,
        DirectionCorner::BottomRight,
        DirectionCorner::BottomLeft,
    ];

    const SIDES: [DirectionCorner; 4] = [
        DirectionCorner::Right,
        DirectionCorner::Left,
        DirectionCorner::Top,
        DirectionCorner::Bottom,
    ];

    const DIAGONALS: [DirectionCorner; 4] = [
        DirectionCorner::TopRight,
        DirectionCorner::TopLeft,
        DirectionCorner::BottomRight,
        DirectionCorner::BottomLeft,
    ];

    fn rect(w: isize, h: isize) -> LayoutRect {
        LayoutRect::new(LayoutPoint::zero(), LayoutSize::new(w, h))
    }

    fn rect_at(x: isize, y: isize, w: isize, h: isize) -> LayoutRect {
        LayoutRect::new(LayoutPoint::new(x, y), LayoutSize::new(w, h))
    }

    /// Canonical direction: `to <corner>`, i.e. starting at the opposite corner.
    fn canonical(dir_to: DirectionCorner) -> Direction {
        Direction::FromTo(DirectionCorners {
            dir_from: dir_to.opposite(),
            dir_to,
        })
    }

    fn assert_near(actual: LayoutPoint, expected: LayoutPoint, tol: isize) {
        assert!(
            (actual.x - expected.x).abs() <= tol && (actual.y - expected.y).abs() <= tol,
            "expected {expected:?} (±{tol}), got {actual:?}"
        );
    }

    // ---- const-evaluability of the `const fn`s -----------------------------

    const CONST_RECT: LayoutRect =
        LayoutRect::new(LayoutPoint::new(7, 9), LayoutSize::new(200, 100));
    const CONST_OPPOSITE: DirectionCorner = DirectionCorner::TopRight.opposite();
    const CONST_COMBINED: Option<DirectionCorner> =
        DirectionCorner::Right.combine(&DirectionCorner::Top);
    const CONST_POINT: LayoutPoint = DirectionCorner::Right.to_point(&CONST_RECT);

    #[test]
    fn const_fns_evaluate_at_compile_time() {
        assert_eq!(CONST_OPPOSITE, DirectionCorner::BottomLeft);
        assert_eq!(CONST_COMBINED, Some(DirectionCorner::TopRight));
        // to_point() is rect-local: the origin (7, 9) is not added.
        assert_eq!(CONST_POINT, LayoutPoint::new(200, 50));
    }

    // ---- DirectionCorner: Display / PrintAsCssValue (serializer) ------------

    #[test]
    fn corner_display_exact_values_and_wellformed() {
        let expected = [
            (DirectionCorner::Right, "right"),
            (DirectionCorner::Left, "left"),
            (DirectionCorner::Top, "top"),
            (DirectionCorner::Bottom, "bottom"),
            (DirectionCorner::TopRight, "top right"),
            (DirectionCorner::TopLeft, "top left"),
            (DirectionCorner::BottomRight, "bottom right"),
            (DirectionCorner::BottomLeft, "bottom left"),
        ];
        for (corner, want) in expected {
            let printed = format!("{corner}");
            assert_eq!(printed, want);
            // PrintAsCssValue must agree with Display.
            assert_eq!(corner.print_as_css_value(), printed);
            // Well-formed: non-empty, ASCII, lowercase, no stray whitespace.
            assert!(!printed.is_empty());
            assert!(printed.is_ascii());
            assert_eq!(printed, printed.to_lowercase());
            assert_eq!(printed, printed.trim());
        }
    }

    #[test]
    fn corner_display_is_injective() {
        let printed: BTreeSet<String> = ALL_CORNERS.iter().map(|c| format!("{c}")).collect();
        assert_eq!(printed.len(), ALL_CORNERS.len());
    }

    #[test]
    fn direction_default_and_display_do_not_panic() {
        // no_panic_default: Default::default() serializes.
        assert_eq!(
            Direction::default(),
            Direction::FromTo(DirectionCorners {
                dir_from: DirectionCorner::Top,
                dir_to: DirectionCorner::Bottom,
            })
        );
        assert_eq!(Direction::default().print_as_css_value(), "to bottom");

        // edge_values: NaN / infinite angles must still serialize without panic.
        for v in [
            0.0_f32,
            -0.0,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            f32::MAX,
            f32::MIN,
            f32::MIN_POSITIVE,
        ] {
            let s = Direction::Angle(AngleValue::deg(v)).print_as_css_value();
            assert!(!s.is_empty(), "empty serialization for {v}");
            assert!(s.ends_with("deg"), "unexpected serialization: {s}");
            // The fixed-point encoding must never leak NaN/inf into the output.
            assert!(!s.contains("NaN"), "NaN leaked into CSS output: {s}");
            assert!(!s.contains("inf"), "inf leaked into CSS output: {s}");
        }
    }

    // ---- DirectionCorner::opposite (getter) --------------------------------

    #[test]
    fn opposite_known_values() {
        assert_eq!(DirectionCorner::Right.opposite(), DirectionCorner::Left);
        assert_eq!(DirectionCorner::Left.opposite(), DirectionCorner::Right);
        assert_eq!(DirectionCorner::Top.opposite(), DirectionCorner::Bottom);
        assert_eq!(DirectionCorner::Bottom.opposite(), DirectionCorner::Top);
        assert_eq!(
            DirectionCorner::TopRight.opposite(),
            DirectionCorner::BottomLeft
        );
        assert_eq!(
            DirectionCorner::BottomLeft.opposite(),
            DirectionCorner::TopRight
        );
        assert_eq!(
            DirectionCorner::TopLeft.opposite(),
            DirectionCorner::BottomRight
        );
        assert_eq!(
            DirectionCorner::BottomRight.opposite(),
            DirectionCorner::TopLeft
        );
    }

    #[test]
    fn opposite_is_an_involution_and_a_bijection() {
        let mut images = BTreeSet::new();
        for c in ALL_CORNERS {
            // No fixed points: a corner is never its own opposite.
            assert_ne!(c.opposite(), c, "{c} is its own opposite");
            // Involution.
            assert_eq!(c.opposite().opposite(), c, "opposite² != id for {c}");
            // A side maps to a side, a diagonal maps to a diagonal.
            assert_eq!(
                SIDES.contains(&c),
                SIDES.contains(&c.opposite()),
                "{c} changed class under opposite()"
            );
            images.insert(c.opposite());
        }
        assert_eq!(images.len(), 8, "opposite() is not a bijection");
    }

    // ---- DirectionCorner::combine (other) ----------------------------------

    #[test]
    fn combine_exhaustive_over_all_64_pairs() {
        let mut some_count = 0_usize;
        for a in ALL_CORNERS {
            for b in ALL_CORNERS {
                let r = a.combine(&b);

                // Commutative.
                assert_eq!(r, b.combine(&a), "combine({a}, {b}) is not commutative");

                match r {
                    Some(c) => {
                        some_count += 1;
                        // Only perpendicular side pairs may combine, and the
                        // result is always a diagonal.
                        assert!(SIDES.contains(&a) && SIDES.contains(&b));
                        assert!(DIAGONALS.contains(&c), "combine({a}, {b}) = {c}, not a corner");
                        assert_ne!(a, b);
                        assert_ne!(a.opposite(), b, "opposite sides must not combine");
                        // The corner name must mention both inputs.
                        let name = format!("{c}");
                        assert!(name.contains(&format!("{a}")) && name.contains(&format!("{b}")));
                        // Combining the opposites yields the opposite corner.
                        assert_eq!(a.opposite().combine(&b.opposite()), Some(c.opposite()));
                    }
                    None => {
                        assert!(
                            !SIDES.contains(&a)
                                || !SIDES.contains(&b)
                                || a == b
                                || a.opposite() == b,
                            "combine({a}, {b}) unexpectedly returned None"
                        );
                    }
                }
            }
        }
        // Exactly the 4 corners × 2 orderings.
        assert_eq!(some_count, 8);
    }

    #[test]
    fn combine_degenerate_pairs_are_none() {
        for c in ALL_CORNERS {
            assert_eq!(c.combine(&c), None, "{c} combined with itself");
            assert_eq!(c.combine(&c.opposite()), None, "{c} combined with opposite");
        }
        assert_eq!(
            DirectionCorner::Top.combine(&DirectionCorner::Bottom),
            None
        );
        assert_eq!(DirectionCorner::Left.combine(&DirectionCorner::Right), None);
        // A diagonal never combines with anything.
        for d in DIAGONALS {
            for c in ALL_CORNERS {
                assert_eq!(d.combine(&c), None, "{d} combined with {c}");
            }
        }
    }

    // ---- DirectionCorner::to_point (numeric) -------------------------------

    #[test]
    fn to_point_zero_rect_is_origin() {
        let r = rect(0, 0);
        for c in ALL_CORNERS {
            assert_eq!(c.to_point(&r), LayoutPoint::zero(), "corner {c}");
        }
    }

    #[test]
    fn to_point_known_values() {
        let r = rect(200, 100);
        assert_eq!(
            DirectionCorner::Right.to_point(&r),
            LayoutPoint::new(200, 50)
        );
        assert_eq!(DirectionCorner::Left.to_point(&r), LayoutPoint::new(0, 50));
        assert_eq!(DirectionCorner::Top.to_point(&r), LayoutPoint::new(100, 0));
        assert_eq!(
            DirectionCorner::Bottom.to_point(&r),
            LayoutPoint::new(100, 100)
        );
        assert_eq!(
            DirectionCorner::TopRight.to_point(&r),
            LayoutPoint::new(200, 0)
        );
        assert_eq!(DirectionCorner::TopLeft.to_point(&r), LayoutPoint::new(0, 0));
        assert_eq!(
            DirectionCorner::BottomRight.to_point(&r),
            LayoutPoint::new(200, 100)
        );
        assert_eq!(
            DirectionCorner::BottomLeft.to_point(&r),
            LayoutPoint::new(0, 100)
        );
    }

    #[test]
    fn to_point_ignores_rect_origin() {
        // to_point() works in rect-local space: a far-away (even negative)
        // origin must not shift the result.
        let local = rect(200, 100);
        for offset in [(0, 0), (1000, -500), (isize::MIN, isize::MAX)] {
            let moved = rect_at(offset.0, offset.1, 200, 100);
            for c in ALL_CORNERS {
                assert_eq!(
                    c.to_point(&moved),
                    c.to_point(&local),
                    "corner {c} shifted by origin {offset:?}"
                );
            }
        }
    }

    #[test]
    fn to_point_opposite_corners_sum_to_the_full_extent() {
        // p(c) + p(opposite(c)) == (width, height) for even extents.
        for (w, h) in [(200_isize, 100_isize), (2, 2), (0, 0), (-40, -60)] {
            let r = rect(w, h);
            for c in ALL_CORNERS {
                let p = c.to_point(&r);
                let q = c.opposite().to_point(&r);
                assert_eq!(p.x + q.x, w, "x-sum for {c} in {w}x{h}");
                assert_eq!(p.y + q.y, h, "y-sum for {c} in {w}x{h}");
            }
        }
    }

    #[test]
    fn to_point_odd_and_negative_extents_truncate_toward_zero() {
        // Rust integer division truncates toward zero: 3/2 == 1, -3/2 == -1.
        let r = rect(3, 3);
        assert_eq!(DirectionCorner::Top.to_point(&r), LayoutPoint::new(1, 0));
        assert_eq!(DirectionCorner::Right.to_point(&r), LayoutPoint::new(3, 1));

        let neg = rect(-3, -3);
        assert_eq!(DirectionCorner::Top.to_point(&neg), LayoutPoint::new(-1, 0));
        assert_eq!(
            DirectionCorner::Right.to_point(&neg),
            LayoutPoint::new(-3, -1)
        );
        assert_eq!(
            DirectionCorner::BottomLeft.to_point(&neg),
            LayoutPoint::new(0, -3)
        );
    }

    #[test]
    fn to_point_isize_extremes_do_not_overflow() {
        // Halving isize::MIN / isize::MAX is the only arithmetic here; neither
        // can overflow (the divisor is a constant 2), so no debug-panic.
        let max = rect(isize::MAX, isize::MAX);
        assert_eq!(
            DirectionCorner::Right.to_point(&max),
            LayoutPoint::new(isize::MAX, isize::MAX / 2)
        );
        assert_eq!(
            DirectionCorner::Bottom.to_point(&max),
            LayoutPoint::new(isize::MAX / 2, isize::MAX)
        );
        assert_eq!(
            DirectionCorner::BottomRight.to_point(&max),
            LayoutPoint::new(isize::MAX, isize::MAX)
        );

        let min = rect(isize::MIN, isize::MIN);
        assert_eq!(
            DirectionCorner::Right.to_point(&min),
            LayoutPoint::new(isize::MIN, isize::MIN / 2)
        );
        assert_eq!(
            DirectionCorner::Top.to_point(&min),
            LayoutPoint::new(isize::MIN / 2, 0)
        );

        // Mixed extremes: every corner stays inside the rect's own bounds.
        let mixed = rect(isize::MAX, isize::MIN);
        for c in ALL_CORNERS {
            let p = c.to_point(&mixed);
            assert!(p.x == 0 || p.x == isize::MAX || p.x == isize::MAX / 2);
            assert!(p.y == 0 || p.y == isize::MIN || p.y == isize::MIN / 2);
        }
    }

    // ---- Direction::to_points (numeric) ------------------------------------

    #[test]
    fn to_points_fromto_delegates_to_to_point() {
        let r = rect(200, 100);
        for from in ALL_CORNERS {
            for to in ALL_CORNERS {
                let d = Direction::FromTo(DirectionCorners {
                    dir_from: from,
                    dir_to: to,
                });
                assert_eq!(d.to_points(&r), (from.to_point(&r), to.to_point(&r)));
            }
        }
    }

    #[test]
    fn to_points_angle_zero_deg_runs_bottom_to_top() {
        // CSS: 0deg == "to top", so the gradient starts at the bottom edge.
        let r = rect(100, 100);
        let (start, end) = Direction::Angle(AngleValue::deg(0.0)).to_points(&r);
        assert_near(start, LayoutPoint::new(50, 100), 1);
        assert_near(end, LayoutPoint::new(50, 0), 1);
    }

    #[test]
    fn to_points_angle_180_deg_runs_top_to_bottom() {
        // CSS: 180deg == "to bottom".
        let r = rect(100, 100);
        let (start, end) = Direction::Angle(AngleValue::deg(180.0)).to_points(&r);
        assert_near(start, LayoutPoint::new(50, 0), 1);
        assert_near(end, LayoutPoint::new(50, 100), 1);
    }

    #[test]
    fn to_points_angle_90_deg_is_horizontal_across_the_full_width() {
        let r = rect(100, 100);
        let (start, end) = Direction::Angle(AngleValue::deg(90.0)).to_points(&r);
        // Both endpoints sit on the horizontal midline, at the two extremes.
        assert!((start.y - 50).abs() <= 1, "start.y = {}", start.y);
        assert!((end.y - 50).abs() <= 1, "end.y = {}", end.y);
        let mut xs = [start.x, end.x];
        xs.sort_unstable();
        assert!(xs[0].abs() <= 1 && (xs[1] - 100).abs() <= 1, "xs = {xs:?}");
        assert_ne!(start, end);
    }

    #[test]
    fn to_points_angle_is_symmetric_about_the_rect_center() {
        // start = center - d, end = center + d (mod rounding), for *every* angle
        // and metric — so the endpoints must always straddle the center.
        let r = rect(200, 100);
        let mut angles = Vec::new();
        let mut deg = -720.0_f32;
        while deg <= 720.0 {
            angles.push(AngleValue::deg(deg));
            deg += 15.0;
        }
        angles.extend([
            AngleValue::rad(1.5),
            AngleValue::rad(-3.0),
            AngleValue::grad(100.0),
            AngleValue::grad(-400.0),
            AngleValue::turn(0.25),
            AngleValue::turn(-2.5),
            AngleValue::percent(50.0),
            AngleValue::percent(-125.0),
        ]);

        for a in angles {
            let (start, end) = Direction::Angle(a).to_points(&r);
            assert!(
                (start.x + end.x - 200).abs() <= 1,
                "x not centered for {a}: {start:?} / {end:?}"
            );
            assert!(
                (start.y + end.y - 100).abs() <= 1,
                "y not centered for {a}: {start:?} / {end:?}"
            );
            // The half-length is the corner projected onto the gradient line, so
            // it can never exceed the center-to-corner distance (~111.8 here).
            // (The endpoints themselves may fall outside a non-square box.)
            // |start - end| == 2 * L <= 2 * hypot(100, 50) == 223.6 (+ rounding).
            let len_sq = (start.x - end.x).pow(2) + (start.y - end.y).pow(2);
            assert!(
                len_sq <= 4 * (100 * 100 + 50 * 50) + 1000,
                "gradient line longer than the rect diagonal for {a}: {start:?} / {end:?}"
            );
        }
    }

    #[test]
    fn to_points_zero_sized_rect_yields_origin_not_nan() {
        // width_half == height_half == 0 => atan(0/0) == NaN propagates through
        // the whole computation; the f32 -> isize cast saturates NaN to 0, so
        // the result must be (0,0)/(0,0) rather than a panic or garbage.
        let r = rect(0, 0);
        for a in [
            AngleValue::deg(0.0),
            AngleValue::deg(45.0),
            AngleValue::deg(-137.5),
            AngleValue::turn(0.75),
        ] {
            let (start, end) = Direction::Angle(a).to_points(&r);
            assert_eq!(start, LayoutPoint::zero(), "start for {a}");
            assert_eq!(end, LayoutPoint::zero(), "end for {a}");
        }
    }

    #[test]
    fn to_points_degenerate_axis_rects_do_not_panic() {
        // Zero width => height/width == +-inf => atan(inf) == 90deg. Must not panic.
        let thin = rect(0, 100);
        let (s, e) = Direction::Angle(AngleValue::deg(0.0)).to_points(&thin);
        assert_eq!(s.x, 0);
        assert_eq!(e.x, 0);
        assert!((s.y + e.y - 100).abs() <= 1);

        let flat = rect(100, 0);
        let (s, e) = Direction::Angle(AngleValue::deg(90.0)).to_points(&flat);
        assert_eq!(s.y, 0);
        assert_eq!(e.y, 0);
        assert!((s.x + e.x - 100).abs() <= 1);
    }

    #[test]
    fn to_points_nan_angle_collapses_to_zero_degrees() {
        // FloatValue::new(NaN) -> f32_to_isize(NaN) == 0, so a NaN angle is
        // silently the same value as 0deg. Assert the collapse (no NaN escapes).
        let nan = AngleValue::deg(f32::NAN);
        assert!(nan.to_degrees().is_finite());
        assert_eq!(nan.to_degrees(), 0.0);
        assert_eq!(nan, AngleValue::deg(0.0));

        let r = rect(100, 100);
        assert_eq!(
            Direction::Angle(nan).to_points(&r),
            Direction::Angle(AngleValue::deg(0.0)).to_points(&r)
        );
    }

    #[test]
    fn to_points_infinite_angle_saturates_and_stays_finite() {
        for v in [f32::INFINITY, f32::NEG_INFINITY, f32::MAX, f32::MIN] {
            let a = AngleValue::deg(v);
            let deg = a.to_degrees();
            assert!(deg.is_finite(), "non-finite degrees for {v}");
            assert!((0.0..=360.0).contains(&deg), "{v} normalized to {deg}");

            let r = rect(200, 100);
            let first = Direction::Angle(a).to_points(&r);
            // Deterministic (no NaN-dependent branch flapping).
            assert_eq!(first, Direction::Angle(a).to_points(&r));
        }
    }

    #[test]
    fn to_points_isize_extreme_rects_do_not_panic() {
        for (w, h) in [
            (isize::MAX, isize::MAX),
            (isize::MIN, isize::MIN),
            (isize::MAX, isize::MIN),
            (isize::MIN, 1),
            (-1, isize::MAX),
        ] {
            let r = rect(w, h);
            for a in [
                AngleValue::deg(45.0),
                AngleValue::deg(0.0),
                AngleValue::deg(270.0),
            ] {
                let d = Direction::Angle(a);
                // The f32 round-trip saturates instead of panicking; only assert
                // that the computation terminates and is deterministic.
                assert_eq!(d.to_points(&r), d.to_points(&r), "{w}x{h} @ {a}");
            }
            // The FromTo path on extreme rects is exact.
            let d = canonical(DirectionCorner::BottomRight);
            assert_eq!(d.to_points(&r).1, LayoutPoint::new(w, h));
        }
    }

    // ---- parse_direction_corner (parser, private) ---------------------------

    #[cfg(feature = "parser")]
    #[test]
    fn parse_corner_valid_minimal() {
        assert_eq!(parse_direction_corner("right"), Ok(DirectionCorner::Right));
        assert_eq!(parse_direction_corner("left"), Ok(DirectionCorner::Left));
        assert_eq!(parse_direction_corner("top"), Ok(DirectionCorner::Top));
        assert_eq!(parse_direction_corner("bottom"), Ok(DirectionCorner::Bottom));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_corner_rejects_untrimmed_cased_and_diagonal_input() {
        // parse_direction_corner does NOT trim and is case-sensitive; every one
        // of these must be a clean Err carrying the input verbatim.
        for bad in [
            "", " ", "   ", "\t", "\n", "\r\n", " right", "right ", "right\n", "Right", "RIGHT",
            "rIgHt", "top right", "top-right", "topright", "right;", "right)", "center", "start",
            "end", "to", "to right",
        ] {
            assert_eq!(
                parse_direction_corner(bad),
                Err(CssDirectionCornerParseError::InvalidDirection(bad)),
                "input {bad:?} was not rejected verbatim"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_corner_garbage_and_unicode_do_not_panic() {
        for bad in [
            "!@#$%^&*()",
            "\u{0}",
            "\u{0}right",
            "right\u{0}",
            "\u{1F600}",
            "to \u{1F600}",
            "ri\u{0301}ght",   // combining acute on 'i'
            "\u{0440}ight",    // Cyrillic 'р'
            "\u{200b}right",   // zero-width space (not Rust whitespace)
            "right\u{200b}",
            "\u{feff}right",   // BOM
            "\u{202e}right",   // RTL override
            "ｒｉｇｈｔ",       // fullwidth
            "\u{a0}right",     // NBSP (Rust whitespace, but not trimmed here)
        ] {
            assert_eq!(
                parse_direction_corner(bad),
                Err(CssDirectionCornerParseError::InvalidDirection(bad)),
                "unicode input {bad:?} was not rejected"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_corner_boundary_numbers_are_rejected() {
        for bad in [
            "0",
            "-0",
            "9223372036854775807",
            "-9223372036854775808",
            "1e400",
            "1e-400",
            "NaN",
            "inf",
            "-inf",
        ] {
            assert!(
                parse_direction_corner(bad).is_err(),
                "numeric input {bad:?} parsed as a corner"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_corner_extremely_long_input_is_rejected_quickly() {
        let long = "a".repeat(1_000_000);
        assert!(parse_direction_corner(&long).is_err());

        let repeated = "right".repeat(200_000);
        assert!(parse_direction_corner(&repeated).is_err());

        // Deeply "nested" input: no recursion in the matcher, so no stack blowup.
        let nested = "(".repeat(100_000);
        assert!(parse_direction_corner(&nested).is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_corner_error_round_trips_through_owned() {
        let long = "top".repeat(10_000);
        for bad in ["", "bogus", "\u{1F600}", long.as_str()] {
            let Err(err) = parse_direction_corner(bad) else {
                panic!("{bad:?} unexpectedly parsed");
            };
            let owned = err.to_contained();
            assert_eq!(owned.to_shared(), err);
        }
    }

    // ---- parse_direction (parser) -------------------------------------------

    #[cfg(feature = "parser")]
    #[test]
    fn parse_direction_empty_and_whitespace_only() {
        for empty in ["", " ", "   ", "\t", "\n", "\r\n", "\t \n \r", "\u{a0}", "\u{3000}"] {
            assert!(
                matches!(
                    parse_direction(empty),
                    Err(CssDirectionParseError::Error(_))
                ),
                "whitespace input {empty:?} did not yield Error"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_direction_valid_minimal_angles() {
        assert_eq!(
            parse_direction("45deg").unwrap(),
            Direction::Angle(AngleValue::deg(45.0))
        );
        // A bare number is degrees.
        assert_eq!(
            parse_direction("0").unwrap(),
            Direction::Angle(AngleValue::deg(0.0))
        );
        assert_eq!(
            parse_direction("1.5rad").unwrap(),
            Direction::Angle(AngleValue::rad(1.5))
        );
        assert_eq!(
            parse_direction("100grad").unwrap(),
            Direction::Angle(AngleValue::grad(100.0))
        );
        assert_eq!(
            parse_direction("50%").unwrap(),
            Direction::Angle(AngleValue::percent(50.0))
        );
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_direction_all_corner_spellings() {
        let cases = [
            ("to right", DirectionCorner::Right),
            ("to left", DirectionCorner::Left),
            ("to top", DirectionCorner::Top),
            ("to bottom", DirectionCorner::Bottom),
            ("to top right", DirectionCorner::TopRight),
            ("to right top", DirectionCorner::TopRight),
            ("to top left", DirectionCorner::TopLeft),
            ("to left top", DirectionCorner::TopLeft),
            ("to bottom right", DirectionCorner::BottomRight),
            ("to right bottom", DirectionCorner::BottomRight),
            ("to bottom left", DirectionCorner::BottomLeft),
            ("to left bottom", DirectionCorner::BottomLeft),
        ];
        for (input, dir_to) in cases {
            let parsed = parse_direction(input).unwrap();
            assert_eq!(parsed, canonical(dir_to), "input {input:?}");
            // Invariant: a parsed FromTo always starts at the opposite corner.
            let Direction::FromTo(ft) = parsed else {
                panic!("{input:?} did not parse to FromTo");
            };
            assert_eq!(ft.dir_from, ft.dir_to.opposite());
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_direction_surrounding_whitespace_is_ignored() {
        let expect = canonical(DirectionCorner::Right);
        for input in ["to right", "  to right  ", "\tto\nright\r", "to    right"] {
            assert_eq!(parse_direction(input).unwrap(), expect, "input {input:?}");
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_direction_error_classification() {
        assert!(matches!(
            parse_direction(""),
            Err(CssDirectionParseError::Error(_))
        ));
        // Missing "to" keyword.
        assert!(matches!(
            parse_direction("right"),
            Err(CssDirectionParseError::InvalidArguments(_))
        ));
        // "to" with no corner.
        assert!(matches!(
            parse_direction("to"),
            Err(CssDirectionParseError::InvalidArguments(_))
        ));
        // Too many components (checked before the corners are parsed).
        assert!(matches!(
            parse_direction("to top right bottom"),
            Err(CssDirectionParseError::InvalidArguments(_))
        ));
        // Unknown corner.
        assert!(matches!(
            parse_direction("to center"),
            Err(CssDirectionParseError::CornerError(
                CssDirectionCornerParseError::InvalidDirection("center")
            ))
        ));
        // Non-combinable corner pairs.
        for bad in [
            "to top top",
            "to top bottom",
            "to bottom top",
            "to left right",
            "to right left",
            "to left left",
        ] {
            assert!(
                matches!(
                    parse_direction(bad),
                    Err(CssDirectionParseError::InvalidArguments(_))
                ),
                "input {bad:?} was accepted or misclassified"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_direction_is_case_sensitive() {
        // NOTE: CSS keywords are case-insensitive; this parser is not. Asserting
        // the current (stricter) behavior so a future relaxation is a visible change.
        for bad in ["TO RIGHT", "To Right", "to RIGHT", "TO right", "45DEG"] {
            assert!(parse_direction(bad).is_err(), "{bad:?} was accepted");
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_direction_leading_trailing_junk_is_rejected() {
        for bad in [
            "45deg;",
            "45deg;garbage",
            "to right;",
            "to;right",
            "to right)",
            "(45deg)",
            "to right,",
            "-->45deg",
        ] {
            assert!(parse_direction(bad).is_err(), "{bad:?} was accepted");
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_direction_ignores_tokens_after_a_leading_angle() {
        // Leniency: once the first token parses as an angle, the rest of the
        // input is dropped on the floor instead of being rejected.
        let expect = Direction::Angle(AngleValue::deg(45.0));
        assert_eq!(parse_direction("45deg garbage").unwrap(), expect);
        assert_eq!(parse_direction("45deg to right").unwrap(), expect);
        assert_eq!(parse_direction("45deg 90deg").unwrap(), expect);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_direction_garbage_and_unicode_do_not_panic() {
        for bad in [
            "!@#$%^&*()",
            "\u{0}",
            "\u{1F600}",
            "to \u{1F600}",
            "45\u{00b0}",   // degree sign, not a CSS unit
            "to right\u{200b}",
            "\u{feff}to right",
            "ｔｏ right",
            "to\u{200b}right",
            "\u{202e}to right",
            "deg",
            "%",
            "-",
            "+",
            ".",
            "e",
            "todeg",
        ] {
            // Must never panic; whatever comes back must be a well-formed value.
            match parse_direction(bad) {
                Ok(Direction::Angle(a)) => {
                    assert!(a.to_degrees().is_finite(), "{bad:?} -> non-finite angle");
                }
                Ok(Direction::FromTo(ft)) => {
                    assert_eq!(ft.dir_from, ft.dir_to.opposite(), "{bad:?}");
                }
                Err(_) => {}
            }
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_direction_unicode_whitespace_separator_is_wellformed() {
        // U+00A0 is Unicode White_Space (so `split_whitespace` splits on it) but
        // is NOT CSS whitespace. Accept either outcome; just pin down that the
        // result can't be some third thing.
        if let Ok(d) = parse_direction("to\u{a0}right") {
            assert_eq!(d, canonical(DirectionCorner::Right));
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_direction_boundary_numbers_never_yield_nan_or_inf() {
        for input in [
            "0",
            "-0",
            "0deg",
            "-0deg",
            "360deg",
            "-360deg",
            "9223372036854775807",
            "-9223372036854775808deg",
            "340282350000000000000000000000000000000deg", // ~f32::MAX
            "1e400",       // parses to f32 inf
            "-1e400deg",
            "1e-400deg",   // underflows to 0
            "NaN",
            "nan deg",
            "inf",
            "-infinity",
            "0.0000001turn",
            "-99999999rad",
        ] {
            match parse_direction(input) {
                Ok(Direction::Angle(a)) => {
                    let deg = a.to_degrees();
                    assert!(deg.is_finite(), "{input:?} produced non-finite {deg}");
                    assert!(
                        (0.0..=360.0).contains(&deg),
                        "{input:?} normalized outside [0,360]: {deg}"
                    );
                    assert!(
                        a.to_degrees_raw().is_finite(),
                        "{input:?} produced non-finite raw degrees"
                    );
                    // And it must survive geometry without producing garbage.
                    let (s, e) = Direction::Angle(a).to_points(&rect(200, 100));
                    assert!((s.x + e.x - 200).abs() <= 1, "{input:?}: {s:?}/{e:?}");
                    assert!((s.y + e.y - 100).abs() <= 1, "{input:?}: {s:?}/{e:?}");
                }
                Ok(Direction::FromTo(_)) => panic!("{input:?} parsed as a corner direction"),
                Err(_) => {}
            }
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_direction_nan_string_silently_becomes_zero_degrees() {
        // "NaN" is a valid f32 literal, so the angle path accepts it; the
        // fixed-point encoding then clamps it to 0. No NaN may escape.
        let parsed = parse_direction("NaN").unwrap();
        assert_eq!(parsed, Direction::Angle(AngleValue::deg(0.0)));
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_direction_extremely_long_input_terminates() {
        // 1M bytes of garbage: rejected without hanging.
        let garbage = "x".repeat(1_000_000);
        assert!(parse_direction(&garbage).is_err());

        // 1M bytes of whitespace: no token at all.
        let blank = " ".repeat(1_000_000);
        assert!(matches!(
            parse_direction(&blank),
            Err(CssDirectionParseError::Error(_))
        ));

        // A huge component list must be rejected (len > 2), not truncated.
        let many = format!("to {}", "top ".repeat(50_000));
        assert!(matches!(
            parse_direction(&many),
            Err(CssDirectionParseError::InvalidArguments(_))
        ));

        // A 100k-digit number: overflows f32 to inf, which the fixed-point
        // encoding saturates -- must still be finite downstream.
        let huge_number = format!("{}deg", "1".repeat(100_000));
        if let Ok(Direction::Angle(a)) = parse_direction(&huge_number) {
            assert!(a.to_degrees().is_finite());
        }

        // Deeply nested brackets: no recursion, so no stack overflow.
        let nested = format!("to {}", "(".repeat(100_000));
        assert!(parse_direction(&nested).is_err());
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_direction_round_trips_all_eight_canonical_corners() {
        for dir_to in ALL_CORNERS {
            let d = canonical(dir_to);
            let printed = d.print_as_css_value();
            assert_eq!(printed, format!("to {dir_to}"));
            assert_eq!(
                parse_direction(&printed).unwrap(),
                d,
                "round-trip failed for {printed:?}"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_direction_round_trips_angles_of_every_metric() {
        // Values are exact multiples of the 1/1000 fixed-point step, so the
        // encode -> print -> parse cycle must be lossless.
        for a in [
            AngleValue::deg(45.0),
            AngleValue::deg(-90.0),
            AngleValue::deg(0.0),
            AngleValue::deg(359.999),
            AngleValue::rad(1.5),
            AngleValue::rad(-3.125),
            AngleValue::grad(100.0),
            AngleValue::turn(-0.25),
            AngleValue::turn(2.0),
            AngleValue::percent(50.0),
        ] {
            let d = Direction::Angle(a);
            let printed = d.print_as_css_value();
            assert_eq!(
                parse_direction(&printed).unwrap(),
                d,
                "round-trip failed for {printed:?}"
            );
        }
    }

    #[cfg(feature = "parser")]
    #[test]
    fn parse_direction_round_trips_the_default() {
        let d = Direction::default();
        assert_eq!(parse_direction(&d.print_as_css_value()).unwrap(), d);
    }

    #[cfg(feature = "parser")]
    #[test]
    fn print_as_css_value_is_lossy_for_non_canonical_fromto() {
        // print_as_css_value() only encodes dir_to, so a FromTo whose dir_from is
        // not the opposite of dir_to cannot survive a round-trip. Pin the loss.
        let weird = Direction::FromTo(DirectionCorners {
            dir_from: DirectionCorner::Top,
            dir_to: DirectionCorner::Right,
        });
        assert_eq!(weird.print_as_css_value(), "to right");
        let reparsed = parse_direction("to right").unwrap();
        assert_ne!(reparsed, weird, "dir_from unexpectedly survived the round-trip");
        assert_eq!(reparsed, canonical(DirectionCorner::Right));
    }

    // ---- error conversions (getters) ---------------------------------------

    #[test]
    fn corner_error_to_contained_and_to_shared_round_trip() {
        let long = "x".repeat(100_000);
        for s in ["", " ", "bogus", "\u{1F600}\u{0301}", "\u{0}", long.as_str()] {
            let err = CssDirectionCornerParseError::InvalidDirection(s);
            let owned = err.to_contained();
            assert_eq!(owned.to_shared(), err, "round-trip failed for {s:?}");
            // The payload is preserved byte-for-byte.
            let CssDirectionCornerParseErrorOwned::InvalidDirection(payload) = &owned;
            assert_eq!(payload.as_str(), s);
        }
    }

    #[test]
    fn direction_error_to_contained_and_to_shared_round_trip_all_variants() {
        let empty_float_err = "".parse::<f32>().unwrap_err();
        let invalid_float_err = "x".parse::<f32>().unwrap_err();

        let errors = [
            CssDirectionParseError::Error("boom"),
            CssDirectionParseError::Error(""),
            CssDirectionParseError::InvalidArguments("to nowhere"),
            CssDirectionParseError::InvalidArguments("\u{1F600}"),
            CssDirectionParseError::ParseFloat(empty_float_err),
            CssDirectionParseError::ParseFloat(invalid_float_err),
            CssDirectionParseError::CornerError(CssDirectionCornerParseError::InvalidDirection(
                "nope",
            )),
            CssDirectionParseError::AngleError(CssAngleValueParseError::EmptyString),
            CssDirectionParseError::AngleError(CssAngleValueParseError::InvalidAngle("zzz")),
        ];

        for err in errors {
            let owned = err.to_contained();
            assert_eq!(owned.to_shared(), err, "round-trip failed for {err:?}");
            // to_contained() must be idempotent through the shared form.
            assert_eq!(owned.to_shared().to_contained(), owned);
        }
    }

    #[test]
    fn direction_error_from_impls_pick_the_right_variant() {
        let float_err: CssDirectionParseError<'_> = "x".parse::<f32>().unwrap_err().into();
        assert!(matches!(float_err, CssDirectionParseError::ParseFloat(_)));

        let corner_err: CssDirectionParseError<'_> =
            CssDirectionCornerParseError::InvalidDirection("q").into();
        assert!(matches!(corner_err, CssDirectionParseError::CornerError(_)));

        let angle_err: CssDirectionParseError<'_> = CssAngleValueParseError::EmptyString.into();
        assert!(matches!(angle_err, CssDirectionParseError::AngleError(_)));
    }

    #[test]
    fn error_display_is_non_empty_and_keeps_the_offending_input() {
        let corner = CssDirectionCornerParseError::InvalidDirection("bogus");
        let printed = format!("{corner}");
        assert!(printed.contains("bogus"), "display lost the input: {printed}");

        for err in [
            CssDirectionParseError::Error("boom"),
            CssDirectionParseError::InvalidArguments("to nowhere"),
            CssDirectionParseError::ParseFloat("x".parse::<f32>().unwrap_err()),
            CssDirectionParseError::CornerError(corner),
            CssDirectionParseError::AngleError(CssAngleValueParseError::EmptyString),
        ] {
            assert!(!format!("{err}").is_empty(), "empty display for {err:?}");
        }

        // Unicode / empty payloads must not panic the formatter.
        for s in ["", "\u{1F600}", "\u{0}"] {
            let e = CssDirectionCornerParseError::InvalidDirection(s);
            let _ = format!("{e}");
            let _ = format!("{:?}", e.to_contained());
        }
    }
}
