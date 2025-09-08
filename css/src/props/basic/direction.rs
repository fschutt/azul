//! CSS direction and corner value types

use alloc::{format, string::String};
use core::fmt;

use crate::{error::CssDirectionParseError, props::formatter::FormatAsCssValue};

/// Basic direction for gradients and other directional properties
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum Direction {
    /// Top direction
    Top,
    /// Bottom direction
    Bottom,
    /// Left direction
    Left,
    /// Right direction
    Right,
    /// Top-left direction
    TopLeft,
    /// Top-right direction
    TopRight,
    /// Bottom-left direction
    BottomLeft,
    /// Bottom-right direction
    BottomRight,
}

/// Corner specification for properties like border-radius
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum DirectionCorner {
    /// Top-left corner
    TopLeft,
    /// Top-right corner
    TopRight,
    /// Bottom-left corner
    BottomLeft,
    /// Bottom-right corner
    BottomRight,
}

/// Multiple corners specification
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct DirectionCorners {
    pub top_left: bool,
    pub top_right: bool,
    pub bottom_left: bool,
    pub bottom_right: bool,
}

impl Default for Direction {
    fn default() -> Self {
        Direction::Top
    }
}

impl Default for DirectionCorner {
    fn default() -> Self {
        DirectionCorner::TopLeft
    }
}

impl Default for DirectionCorners {
    fn default() -> Self {
        Self {
            top_left: false,
            top_right: false,
            bottom_left: false,
            bottom_right: false,
        }
    }
}

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::Direction::*;
        match self {
            Top => write!(f, "top"),
            Bottom => write!(f, "bottom"),
            Left => write!(f, "left"),
            Right => write!(f, "right"),
            TopLeft => write!(f, "top left"),
            TopRight => write!(f, "top right"),
            BottomLeft => write!(f, "bottom left"),
            BottomRight => write!(f, "bottom right"),
        }
    }
}

impl fmt::Display for DirectionCorner {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::DirectionCorner::*;
        match self {
            TopLeft => write!(f, "top-left"),
            TopRight => write!(f, "top-right"),
            BottomLeft => write!(f, "bottom-left"),
            BottomRight => write!(f, "bottom-right"),
        }
    }
}

impl FormatAsCssValue for Direction {
    fn format_as_css_value(&self) -> String {
        self.to_string()
    }
}

impl FormatAsCssValue for DirectionCorner {
    fn format_as_css_value(&self) -> String {
        self.to_string()
    }
}

impl FormatAsCssValue for DirectionCorners {
    fn format_as_css_value(&self) -> String {
        let mut parts = alloc::vec::Vec::new();
        if self.top_left {
            parts.push("top-left");
        }
        if self.top_right {
            parts.push("top-right");
        }
        if self.bottom_left {
            parts.push("bottom-left");
        }
        if self.bottom_right {
            parts.push("bottom-right");
        }
        parts.join(" ")
    }
}

impl DirectionCorners {
    pub fn all() -> Self {
        Self {
            top_left: true,
            top_right: true,
            bottom_left: true,
            bottom_right: true,
        }
    }

    pub fn none() -> Self {
        Self::default()
    }

    pub fn from_corner(corner: DirectionCorner) -> Self {
        let mut result = Self::none();
        match corner {
            DirectionCorner::TopLeft => result.top_left = true,
            DirectionCorner::TopRight => result.top_right = true,
            DirectionCorner::BottomLeft => result.bottom_left = true,
            DirectionCorner::BottomRight => result.bottom_right = true,
        }
        result
    }
}

/// Parse a direction value
#[cfg(feature = "parser")]
pub fn parse_direction<'a>(input: &'a str) -> Result<Direction, CssDirectionParseError<'a>> {
    let input_trimmed = input.trim().to_lowercase();
    match input_trimmed.as_str() {
        "top" => Ok(Direction::Top),
        "bottom" => Ok(Direction::Bottom),
        "left" => Ok(Direction::Left),
        "right" => Ok(Direction::Right),
        "top left" | "left top" => Ok(Direction::TopLeft),
        "top right" | "right top" => Ok(Direction::TopRight),
        "bottom left" | "left bottom" => Ok(Direction::BottomLeft),
        "bottom right" | "right bottom" => Ok(Direction::BottomRight),
        _ => Err(CssDirectionParseError::InvalidDirection(input)),
    }
}

/// Parse a corner specification
#[cfg(feature = "parser")]
pub fn parse_direction_corner<'a>(
    input: &'a str,
) -> Result<DirectionCorner, CssDirectionParseError<'a>> {
    let input_trimmed = input.trim().to_lowercase();
    match input_trimmed.as_str() {
        "top-left" | "top left" => Ok(DirectionCorner::TopLeft),
        "top-right" | "top right" => Ok(DirectionCorner::TopRight),
        "bottom-left" | "bottom left" => Ok(DirectionCorner::BottomLeft),
        "bottom-right" | "bottom right" => Ok(DirectionCorner::BottomRight),
        _ => Err(CssDirectionParseError::InvalidCorner(input)),
    }
}
