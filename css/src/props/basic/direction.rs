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
                let width_half = rect.size.width as f32 / 2.0;
                let height_half = rect.size.height as f32 / 2.0;
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
                        libm::roundf(width_half - dx) as isize,
                        libm::roundf(height_half + dy) as isize,
                    ),
                    LayoutPoint::new(
                        libm::roundf(width_half + dx) as isize,
                        libm::roundf(height_half - dy) as isize,
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
