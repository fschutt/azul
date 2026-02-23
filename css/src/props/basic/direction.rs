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
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                DirectionCorner::Right => "right",
                DirectionCorner::Left => "left",
                DirectionCorner::Top => "top",
                DirectionCorner::Bottom => "bottom",
                DirectionCorner::TopRight => "top right",
                DirectionCorner::TopLeft => "top left",
                DirectionCorner::BottomRight => "bottom right",
                DirectionCorner::BottomLeft => "bottom left",
            }
        )
    }
}

impl PrintAsCssValue for DirectionCorner {
    fn print_as_css_value(&self) -> String {
        format!("{}", self)
    }
}

impl DirectionCorner {
    pub const fn opposite(&self) -> Self {
        use self::DirectionCorner::*;
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

    pub const fn combine(&self, other: &Self) -> Option<Self> {
        use self::DirectionCorner::*;
        match (*self, *other) {
            (Right, Top) | (Top, Right) => Some(TopRight),
            (Left, Top) | (Top, Left) => Some(TopLeft),
            (Right, Bottom) | (Bottom, Right) => Some(BottomRight),
            (Left, Bottom) | (Bottom, Left) => Some(BottomLeft),
            _ => None,
        }
    }

    pub const fn to_point(&self, rect: &LayoutRect) -> LayoutPoint {
        use self::DirectionCorner::*;
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct DirectionCorners {
    pub dir_from: DirectionCorner,
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
        Direction::FromTo(DirectionCorners {
            dir_from: DirectionCorner::Top,
            dir_to: DirectionCorner::Bottom,
        })
    }
}

impl PrintAsCssValue for Direction {
    fn print_as_css_value(&self) -> String {
        match self {
            Direction::Angle(a) => format!("{}", a),
            Direction::FromTo(d) => format!("to {}", d.dir_to), // simplified "from X to Y"
        }
    }
}

impl Direction {
    pub fn to_points(&self, rect: &LayoutRect) -> (LayoutPoint, LayoutPoint) {
        match self {
            Direction::Angle(angle_value) => {
                // NOTE: This implementation is complex and seems to have issues in the original
                // code. It is copied here as-is for the refactoring.
                let deg = -angle_value.to_degrees();
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
            Direction::FromTo(ft) => (ft.dir_from.to_point(rect), ft.dir_to.to_point(rect)),
        }
    }
}

// -- Parser

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum CssDirectionCornerParseError<'a> {
    InvalidDirection(&'a str),
}

impl_display! { CssDirectionCornerParseError<'a>, {
    InvalidDirection(val) => format!("Invalid direction: \"{}\"", val),
}}

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum CssDirectionCornerParseErrorOwned {
    InvalidDirection(AzString),
}

impl<'a> CssDirectionCornerParseError<'a> {
    pub fn to_contained(&self) -> CssDirectionCornerParseErrorOwned {
        match self {
            CssDirectionCornerParseError::InvalidDirection(s) => {
                CssDirectionCornerParseErrorOwned::InvalidDirection(s.to_string().into())
            }
        }
    }
}

impl CssDirectionCornerParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssDirectionCornerParseError<'a> {
        match self {
            CssDirectionCornerParseErrorOwned::InvalidDirection(s) => {
                CssDirectionCornerParseError::InvalidDirection(s.as_str())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
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

impl<'a> From<ParseFloatError> for CssDirectionParseError<'a> {
    fn from(e: ParseFloatError) -> Self {
        CssDirectionParseError::ParseFloat(e)
    }
}
impl_from! { CssDirectionCornerParseError<'a>, CssDirectionParseError::CornerError }
impl_from! { CssAngleValueParseError<'a>, CssDirectionParseError::AngleError }

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum CssDirectionParseErrorOwned {
    Error(AzString),
    InvalidArguments(AzString),
    ParseFloat(crate::props::basic::error::ParseFloatError),
    CornerError(CssDirectionCornerParseErrorOwned),
    AngleError(CssAngleValueParseErrorOwned),
}

impl<'a> CssDirectionParseError<'a> {
    pub fn to_contained(&self) -> CssDirectionParseErrorOwned {
        match self {
            CssDirectionParseError::Error(s) => CssDirectionParseErrorOwned::Error(s.to_string().into()),
            CssDirectionParseError::InvalidArguments(s) => {
                CssDirectionParseErrorOwned::InvalidArguments(s.to_string().into())
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
    pub fn to_shared<'a>(&'a self) -> CssDirectionParseError<'a> {
        match self {
            CssDirectionParseErrorOwned::Error(s) => CssDirectionParseError::Error(s.as_str()),
            CssDirectionParseErrorOwned::InvalidArguments(s) => {
                CssDirectionParseError::InvalidArguments(s.as_str())
            }
            CssDirectionParseErrorOwned::ParseFloat(e) => {
                CssDirectionParseError::ParseFloat(e.to_std())
            }
            CssDirectionParseErrorOwned::CornerError(e) => {
                CssDirectionParseError::CornerError(e.to_shared())
            }
            CssDirectionParseErrorOwned::AngleError(e) => {
                CssDirectionParseError::AngleError(e.to_shared())
            }
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_direction_corner<'a>(
    input: &'a str,
) -> Result<DirectionCorner, CssDirectionCornerParseError<'a>> {
    match input {
        "right" => Ok(DirectionCorner::Right),
        "left" => Ok(DirectionCorner::Left),
        "top" => Ok(DirectionCorner::Top),
        "bottom" => Ok(DirectionCorner::Bottom),
        _ => Err(CssDirectionCornerParseError::InvalidDirection(input)),
    }
}

#[cfg(feature = "parser")]
pub fn parse_direction<'a>(input: &'a str) -> Result<Direction, CssDirectionParseError<'a>> {
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

    let mut components = input_iter.collect::<Vec<_>>();
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
