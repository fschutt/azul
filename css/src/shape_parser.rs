//! CSS Shape parsing for shape-inside, shape-outside, and clip-path
//!
//! Supports CSS Shapes Level 1 & 2 syntax:
//! - `circle(radius at x y)`
//! - `ellipse(rx ry at x y)`
//! - `polygon(x1 y1, x2 y2, ...)`
//! - `inset(top right bottom left [round radius])`
//! - `path(svg-path-data)`

use crate::shape::{CssShape, ShapePoint};

/// Error type for shape parsing failures
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShapeParseError {
    /// Unknown shape function
    UnknownFunction(alloc::string::String),
    /// Missing required parameter
    MissingParameter(alloc::string::String),
    /// Invalid numeric value
    InvalidNumber(alloc::string::String),
    /// Invalid syntax
    InvalidSyntax(alloc::string::String),
    /// Empty input
    EmptyInput,
}

impl core::fmt::Display for ShapeParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        match self {
            ShapeParseError::UnknownFunction(func) => {
                write!(f, "Unknown shape function: {}", func)
            }
            ShapeParseError::MissingParameter(param) => {
                write!(f, "Missing required parameter: {}", param)
            }
            ShapeParseError::InvalidNumber(num) => {
                write!(f, "Invalid numeric value: {}", num)
            }
            ShapeParseError::InvalidSyntax(msg) => {
                write!(f, "Invalid syntax: {}", msg)
            }
            ShapeParseError::EmptyInput => {
                write!(f, "Empty input")
            }
        }
    }
}

/// Parses a CSS shape value
pub fn parse_shape(input: &str) -> Result<CssShape, ShapeParseError> {
    let input = input.trim();

    if input.is_empty() {
        return Err(ShapeParseError::EmptyInput);
    }

    // Extract function name and arguments
    let (func_name, args) = parse_function(input)?;

    match func_name.as_str() {
        "circle" => parse_circle(&args),
        "ellipse" => parse_ellipse(&args),
        "polygon" => parse_polygon(&args),
        "inset" => parse_inset(&args),
        "path" => parse_path(&args),
        _ => Err(ShapeParseError::UnknownFunction(func_name)),
    }
}

/// Extracts function name and arguments from "func(args)"
fn parse_function(
    input: &str,
) -> Result<(alloc::string::String, alloc::string::String), ShapeParseError> {
    let open_paren = input
        .find('(')
        .ok_or_else(|| ShapeParseError::InvalidSyntax("Missing opening parenthesis".into()))?;

    let close_paren = input
        .rfind(')')
        .ok_or_else(|| ShapeParseError::InvalidSyntax("Missing closing parenthesis".into()))?;

    if close_paren <= open_paren {
        return Err(ShapeParseError::InvalidSyntax("Invalid parentheses".into()));
    }

    let func_name = input[..open_paren].trim().to_string();
    let args = input[open_paren + 1..close_paren].trim().to_string();

    Ok((func_name, args))
}

/// Parses a circle: `circle(radius at x y)` or `circle(radius)`
///
/// Examples:
/// - `circle(50px)` - circle at origin with radius 50px
/// - `circle(50px at 100px 100px)` - circle at (100, 100) with radius 50px
/// - `circle(50%)` - circle with radius 50% of container
fn parse_circle(args: &str) -> Result<CssShape, ShapeParseError> {
    let parts: Vec<&str> = args.split_whitespace().collect();

    if parts.is_empty() {
        return Err(ShapeParseError::MissingParameter("radius".into()));
    }

    let radius = parse_length(parts[0])?;

    let center = if parts.len() >= 4 && parts[1] == "at" {
        let x = parse_length(parts[2])?;
        let y = parse_length(parts[3])?;
        ShapePoint::new(x, y)
    } else {
        ShapePoint::zero() // Default to origin
    };

    Ok(CssShape::circle(center, radius))
}

/// Parses an ellipse: `ellipse(rx ry at x y)` or `ellipse(rx ry)`
///
/// Examples:
/// - `ellipse(50px 75px)` - ellipse at origin
/// - `ellipse(50px 75px at 100px 100px)` - ellipse at (100, 100)
fn parse_ellipse(args: &str) -> Result<CssShape, ShapeParseError> {
    let parts: Vec<&str> = args.split_whitespace().collect();

    if parts.len() < 2 {
        return Err(ShapeParseError::MissingParameter(
            "radius_x and radius_y".into(),
        ));
    }

    let radius_x = parse_length(parts[0])?;
    let radius_y = parse_length(parts[1])?;

    let center = if parts.len() >= 5 && parts[2] == "at" {
        let x = parse_length(parts[3])?;
        let y = parse_length(parts[4])?;
        ShapePoint::new(x, y)
    } else {
        ShapePoint::zero()
    };

    Ok(CssShape::ellipse(center, radius_x, radius_y))
}

/// Parses a polygon: `polygon([fill-rule,] x1 y1, x2 y2, ...)`
///
/// Examples:
/// - `polygon(0% 0%, 100% 0%, 100% 100%, 0% 100%)` - rectangle
/// - `polygon(50% 0%, 100% 50%, 50% 100%, 0% 50%)` - diamond
/// - `polygon(nonzero, 0 0, 100 0, 100 100)` - with fill rule
fn parse_polygon(args: &str) -> Result<CssShape, ShapeParseError> {
    let args = args.trim();

    // Check for optional fill-rule
    let point_str = if args.starts_with("nonzero,") || args.starts_with("evenodd,") {
        // Skip fill-rule for now (not used in line segment computation)
        let comma = args.find(',').unwrap();
        &args[comma + 1..]
    } else {
        args
    };

    // Split by comma to get coordinate pairs
    let pairs: Vec<&str> = point_str.split(',').map(|s| s.trim()).collect();

    if pairs.is_empty() {
        return Err(ShapeParseError::MissingParameter(
            "at least one point".into(),
        ));
    }

    let mut points = alloc::vec::Vec::new();

    for pair in pairs {
        let coords: Vec<&str> = pair.split_whitespace().collect();

        if coords.len() < 2 {
            return Err(ShapeParseError::InvalidSyntax(format!(
                "Expected x y pair, got: {}",
                pair
            )));
        }

        let x = parse_length(coords[0])?;
        let y = parse_length(coords[1])?;

        points.push(ShapePoint::new(x, y));
    }

    if points.len() < 3 {
        return Err(ShapeParseError::InvalidSyntax(
            "Polygon must have at least 3 points".into(),
        ));
    }

    Ok(CssShape::polygon(points.into()))
}

/// Parses an inset: `inset(top right bottom left [round radius])`
///
/// Examples:
/// - `inset(10px)` - all sides 10px
/// - `inset(10px 20px)` - top/bottom 10px, left/right 20px
/// - `inset(10px 20px 30px)` - top 10px, left/right 20px, bottom 30px
/// - `inset(10px 20px 30px 40px)` - individual sides
/// - `inset(10px round 5px)` - with border radius
fn parse_inset(args: &str) -> Result<CssShape, ShapeParseError> {
    let args = args.trim();

    // Check for optional "round" keyword for border radius
    let (inset_str, border_radius) = if let Some(round_pos) = args.find("round") {
        let insets = args[..round_pos].trim();
        let radius_str = args[round_pos + 5..].trim();
        let radius = parse_length(radius_str)?;
        (insets, Some(radius))
    } else {
        (args, None)
    };

    let values: Vec<&str> = inset_str.split_whitespace().collect();

    if values.is_empty() {
        return Err(ShapeParseError::MissingParameter("inset values".into()));
    }

    // Parse insets using CSS shorthand rules (same as margin/padding)
    let (top, right, bottom, left) = match values.len() {
        1 => {
            let all = parse_length(values[0])?;
            (all, all, all, all)
        }
        2 => {
            let vertical = parse_length(values[0])?;
            let horizontal = parse_length(values[1])?;
            (vertical, horizontal, vertical, horizontal)
        }
        3 => {
            let top = parse_length(values[0])?;
            let horizontal = parse_length(values[1])?;
            let bottom = parse_length(values[2])?;
            (top, horizontal, bottom, horizontal)
        }
        4 => {
            let top = parse_length(values[0])?;
            let right = parse_length(values[1])?;
            let bottom = parse_length(values[2])?;
            let left = parse_length(values[3])?;
            (top, right, bottom, left)
        }
        _ => {
            return Err(ShapeParseError::InvalidSyntax(
                "Too many inset values (max 4)".into(),
            ));
        }
    };

    if let Some(radius) = border_radius {
        Ok(CssShape::inset_rounded(top, right, bottom, left, radius))
    } else {
        Ok(CssShape::inset(top, right, bottom, left))
    }
}

/// Parses a path: `path("svg-path-data")`
///
/// Example:
/// - `path("M 0 0 L 100 0 L 100 100 Z")`
fn parse_path(args: &str) -> Result<CssShape, ShapeParseError> {
    use crate::corety::AzString;

    let args = args.trim();

    // Path data should be quoted
    if !args.starts_with('"') || !args.ends_with('"') {
        return Err(ShapeParseError::InvalidSyntax(
            "Path data must be quoted".into(),
        ));
    }

    let path_data = AzString::from(&args[1..args.len() - 1]);

    Ok(CssShape::Path(crate::shape::ShapePath { data: path_data }))
}

/// Parses a CSS length value (px, %, em, etc.)
///
/// For now, only handles px and % values.
/// TODO: Handle em, rem, vh, vw, etc. (requires layout context)
fn parse_length(s: &str) -> Result<f32, ShapeParseError> {
    let s = s.trim();

    if s.ends_with("px") {
        let num_str = &s[..s.len() - 2];
        num_str
            .parse::<f32>()
            .map_err(|_| ShapeParseError::InvalidNumber(s.to_string()))
    } else if s.ends_with('%') {
        let num_str = &s[..s.len() - 1];
        let percent = num_str
            .parse::<f32>()
            .map_err(|_| ShapeParseError::InvalidNumber(s.to_string()))?;
        // TODO: Percentage values need container size to resolve
        // For now, treat as raw value (will need context later)
        Ok(percent)
    } else {
        // Try to parse as unitless number (treat as px)
        s.parse::<f32>()
            .map_err(|_| ShapeParseError::InvalidNumber(s.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        corety::OptionF32,
        shape::{ShapeCircle, ShapeEllipse, ShapeInset, ShapePath, ShapePolygon},
    };

    #[test]
    fn test_parse_circle() {
        let shape = parse_shape("circle(50px at 100px 100px)").unwrap();
        match shape {
            CssShape::Circle(ShapeCircle { center, radius }) => {
                assert_eq!(radius, 50.0);
                assert_eq!(center.x, 100.0);
                assert_eq!(center.y, 100.0);
            }
            _ => panic!("Expected Circle"),
        }
    }

    #[test]
    fn test_parse_circle_no_position() {
        let shape = parse_shape("circle(50px)").unwrap();
        match shape {
            CssShape::Circle(ShapeCircle { center, radius }) => {
                assert_eq!(radius, 50.0);
                assert_eq!(center.x, 0.0);
                assert_eq!(center.y, 0.0);
            }
            _ => panic!("Expected Circle"),
        }
    }

    #[test]
    fn test_parse_ellipse() {
        let shape = parse_shape("ellipse(50px 75px at 100px 100px)").unwrap();
        match shape {
            CssShape::Ellipse(ShapeEllipse {
                center,
                radius_x,
                radius_y,
            }) => {
                assert_eq!(radius_x, 50.0);
                assert_eq!(radius_y, 75.0);
                assert_eq!(center.x, 100.0);
                assert_eq!(center.y, 100.0);
            }
            _ => panic!("Expected Ellipse"),
        }
    }

    #[test]
    fn test_parse_polygon_rectangle() {
        let shape = parse_shape("polygon(0px 0px, 100px 0px, 100px 100px, 0px 100px)").unwrap();
        match shape {
            CssShape::Polygon(ShapePolygon { points }) => {
                assert_eq!(points.as_ref().len(), 4);
                assert_eq!(points.as_ref()[0].x, 0.0);
                assert_eq!(points.as_ref()[0].y, 0.0);
                assert_eq!(points.as_ref()[2].x, 100.0);
                assert_eq!(points.as_ref()[2].y, 100.0);
            }
            _ => panic!("Expected Polygon"),
        }
    }

    #[test]
    fn test_parse_polygon_star() {
        // 5-pointed star
        let shape = parse_shape(
            "polygon(50px 0px, 61px 35px, 98px 35px, 68px 57px, 79px 91px, 50px 70px, 21px 91px, \
             32px 57px, 2px 35px, 39px 35px)",
        )
        .unwrap();
        match shape {
            CssShape::Polygon(ShapePolygon { points }) => {
                assert_eq!(points.as_ref().len(), 10); // 5-pointed star has 10 vertices
            }
            _ => panic!("Expected Polygon"),
        }
    }

    #[test]
    fn test_parse_inset() {
        let shape = parse_shape("inset(10px 20px 30px 40px)").unwrap();
        match shape {
            CssShape::Inset(ShapeInset {
                inset_top,
                inset_right,
                inset_bottom,
                inset_left,
                border_radius,
            }) => {
                assert_eq!(inset_top, 10.0);
                assert_eq!(inset_right, 20.0);
                assert_eq!(inset_bottom, 30.0);
                assert_eq!(inset_left, 40.0);
                assert!(matches!(border_radius, OptionF32::None));
            }
            _ => panic!("Expected Inset"),
        }
    }

    #[test]
    fn test_parse_inset_rounded() {
        let shape = parse_shape("inset(10px round 5px)").unwrap();
        match shape {
            CssShape::Inset(ShapeInset {
                inset_top,
                inset_right,
                inset_bottom,
                inset_left,
                border_radius,
            }) => {
                assert_eq!(inset_top, 10.0);
                assert_eq!(inset_right, 10.0);
                assert_eq!(inset_bottom, 10.0);
                assert_eq!(inset_left, 10.0);
                assert!(matches!(border_radius, OptionF32::Some(r) if r == 5.0));
            }
            _ => panic!("Expected Inset"),
        }
    }

    #[test]
    fn test_parse_path() {
        let shape = parse_shape(r#"path("M 0 0 L 100 0 L 100 100 Z")"#).unwrap();
        match shape {
            CssShape::Path(ShapePath { data }) => {
                assert_eq!(data.as_str(), "M 0 0 L 100 0 L 100 100 Z");
            }
            _ => panic!("Expected Path"),
        }
    }

    #[test]
    fn test_invalid_function() {
        let result = parse_shape("unknown(50px)");
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_input() {
        let result = parse_shape("");
        assert!(matches!(result, Err(ShapeParseError::EmptyInput)));
    }
}
