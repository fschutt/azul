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
    /// Unknown shape function — the string contains the unrecognized function name
    UnknownFunction(String),
    /// Missing required parameter — the string names the expected parameter
    MissingParameter(String),
    /// Invalid numeric value — the string contains the unparseable token
    InvalidNumber(String),
    /// Invalid syntax — the string contains a description of what went wrong
    InvalidSyntax(String),
    /// Empty input string was provided
    EmptyInput,
}

impl core::fmt::Display for ShapeParseError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::UnknownFunction(func) => {
                write!(f, "Unknown shape function: {func}")
            }
            Self::MissingParameter(param) => {
                write!(f, "Missing required parameter: {param}")
            }
            Self::InvalidNumber(num) => {
                write!(f, "Invalid numeric value: {num}")
            }
            Self::InvalidSyntax(msg) => {
                write!(f, "Invalid syntax: {msg}")
            }
            Self::EmptyInput => {
                write!(f, "Empty input")
            }
        }
    }
}

/// Parses a CSS shape value
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `shape` value.
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
) -> Result<(String, String), ShapeParseError> {
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
/// Note: the optional fill-rule (`nonzero` or `evenodd`) is parsed but
/// currently ignored — the scanline rasterizer always uses even-odd fill.
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
    let pairs: Vec<&str> = point_str.split(',').map(str::trim).collect();

    if pairs.is_empty() {
        return Err(ShapeParseError::MissingParameter(
            "at least one point".into(),
        ));
    }

    let mut points = Vec::new();

    for pair in pairs {
        let coords: Vec<&str> = pair.split_whitespace().collect();

        if coords.len() < 2 {
            return Err(ShapeParseError::InvalidSyntax(format!(
                "Expected x y pair, got: {pair}"
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

    border_radius.map_or_else(
        || Ok(CssShape::inset(top, right, bottom, left)),
        |radius| Ok(CssShape::inset_rounded(top, right, bottom, left, radius)),
    )
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

    if let Some(num_str) = s.strip_suffix("px") {
        num_str
            .parse::<f32>()
            .map_err(|_| ShapeParseError::InvalidNumber(s.to_string()))
    } else if let Some(num_str) = s.strip_suffix('%') {
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
    // Tests assert that parsed values equal the exact source literals.
    #![allow(clippy::float_cmp)]
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

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::unreadable_literal)]
mod autotest_generated {
    //! Adversarial tests for the shape parser.
    //!
    //! Tests named `bug_*` assert the *correct* behavior and currently FAIL —
    //! they document a genuine defect, not a broken test.
    //!
    //! Tests named `current_*` pin down behavior that is deliberately lenient
    //! or spec-divergent today; they exist so a future tightening is a visible,
    //! intentional change rather than a silent one.

    use std::panic::{catch_unwind, AssertUnwindSafe};

    use super::*;
    use crate::{
        corety::OptionF32,
        shape::{ShapeCircle, ShapeEllipse, ShapeInset, ShapePath, ShapePolygon},
    };

    // ---- helpers ----------------------------------------------------------

    fn circle_of(shape: &CssShape) -> ShapeCircle {
        match shape {
            CssShape::Circle(c) => *c,
            other => panic!("expected Circle, got {other:?}"),
        }
    }

    fn ellipse_of(shape: &CssShape) -> ShapeEllipse {
        match shape {
            CssShape::Ellipse(e) => *e,
            other => panic!("expected Ellipse, got {other:?}"),
        }
    }

    fn inset_of(shape: &CssShape) -> ShapeInset {
        match shape {
            CssShape::Inset(i) => *i,
            other => panic!("expected Inset, got {other:?}"),
        }
    }

    fn polygon_points(shape: &CssShape) -> Vec<ShapePoint> {
        match shape {
            CssShape::Polygon(ShapePolygon { points }) => points.as_ref().to_vec(),
            other => panic!("expected Polygon, got {other:?}"),
        }
    }

    fn path_data(shape: &CssShape) -> String {
        match shape {
            CssShape::Path(ShapePath { data }) => data.as_str().to_string(),
            other => panic!("expected Path, got {other:?}"),
        }
    }

    fn radius_of(input: &str) -> f32 {
        circle_of(&parse_shape(input).unwrap()).radius
    }

    // =======================================================================
    // GENUINE BUGS — these assertions fail today.
    // =======================================================================

    #[test]
    fn bug_parse_path_bare_quote_char_panics_on_reversed_slice() {
        // `path(")` -> parse_function yields args == "\"" (a single byte).
        // In parse_path, `starts_with('"')` and `ends_with('"')` are BOTH true
        // for that one character, so the "is quoted" guard passes and the body
        // evaluates `&args[1..args.len() - 1]` == `&args[1..0]`, which panics:
        //   "slice index starts at 1 but ends at 0"
        // Correct behavior: reject it as unquoted/invalid syntax.
        // Fix: require `args.len() >= 2` alongside the two quote checks.
        let parsed = catch_unwind(AssertUnwindSafe(|| parse_shape("path(\")")));
        assert!(
            parsed.is_ok(),
            "parse_shape(r#\"path(\")\"#) panicked instead of returning Err: parse_path slices \
             args[1..len-1] on a 1-char arg"
        );
        assert!(matches!(
            parsed.unwrap(),
            Err(ShapeParseError::InvalidSyntax(_))
        ));
    }

    #[test]
    fn bug_parse_path_direct_single_quote_arg_panics() {
        // Same defect reached through the private fn, with the argument already
        // isolated: a lone `"` is not a quoted string and must be an Err.
        let parsed = catch_unwind(AssertUnwindSafe(|| parse_path("\"")));
        assert!(
            parsed.is_ok(),
            "parse_path(\"\\\"\") panicked instead of returning Err"
        );
        assert!(parsed.unwrap().is_err());
    }

    // =======================================================================
    // parse_shape — dispatch, framing, hostile inputs
    // =======================================================================

    #[test]
    fn shape_empty_and_whitespace_only_input_is_empty_input_err() {
        assert!(matches!(parse_shape(""), Err(ShapeParseError::EmptyInput)));
        assert!(matches!(
            parse_shape("   "),
            Err(ShapeParseError::EmptyInput)
        ));
        assert!(matches!(
            parse_shape("\t\n\r  \x0c"),
            Err(ShapeParseError::EmptyInput)
        ));
        // U+00A0 NO-BREAK SPACE has White_Space=yes, so str::trim removes it too.
        assert!(parse_shape("\u{00a0}\u{2003}").is_err());
    }

    #[test]
    fn shape_garbage_input_is_rejected_without_panicking() {
        for garbage in [
            "!!!",
            ";;;;",
            "\0\0\0",
            "\u{7}\u{1b}[0m",
            "circle",
            "circle 50px",
            "()",
            "(",
            ")",
            ")(",
            ")circle(",
            "((((",
            "))))",
            "-",
            "50px",
            "{}",
            "circle{50px}",
            "circle[50px]",
            "<circle r=\"50\"/>",
        ] {
            let parsed = catch_unwind(AssertUnwindSafe(|| parse_shape(garbage)));
            assert!(parsed.is_ok(), "parse_shape({garbage:?}) panicked");
            assert!(
                parsed.unwrap().is_err(),
                "parse_shape({garbage:?}) unexpectedly succeeded"
            );
        }
    }

    #[test]
    fn shape_missing_parens_report_which_one_is_missing() {
        assert!(matches!(
            parse_shape("circle 50px"),
            Err(ShapeParseError::InvalidSyntax(ref m)) if m.contains("opening")
        ));
        assert!(matches!(
            parse_shape("circle(50px"),
            Err(ShapeParseError::InvalidSyntax(ref m)) if m.contains("closing")
        ));
        // Closing paren before the opening one must not produce a reversed slice.
        assert!(matches!(
            parse_shape(")circle("),
            Err(ShapeParseError::InvalidSyntax(ref m)) if m.contains("Invalid parentheses")
        ));
    }

    #[test]
    fn shape_unknown_function_names_are_reported_verbatim() {
        assert!(matches!(
            parse_shape("unknown(50px)"),
            Err(ShapeParseError::UnknownFunction(ref f)) if f == "unknown"
        ));
        // Empty function name: "()" is not EmptyInput, it is an unknown "" function.
        assert!(matches!(
            parse_shape("()"),
            Err(ShapeParseError::UnknownFunction(ref f)) if f.is_empty()
        ));
    }

    #[test]
    fn current_shape_function_names_are_case_sensitive() {
        // NOTE: CSS function names are ASCII case-insensitive per spec, so
        // `CIRCLE(50px)` / `Circle(50px)` should parse. They do not today.
        // Pinned so that adding case-folding is a deliberate change.
        assert!(matches!(
            parse_shape("CIRCLE(50px)"),
            Err(ShapeParseError::UnknownFunction(ref f)) if f == "CIRCLE"
        ));
        assert!(parse_shape("Inset(10px)").is_err());
    }

    #[test]
    fn current_shape_trailing_junk_after_the_last_paren_is_silently_dropped() {
        // parse_function slices between the FIRST '(' and the LAST ')', so
        // anything after the closing paren is discarded rather than rejected.
        // The task spec allows "rejected OR trimmed deterministically" — this is
        // the deterministic-drop branch. Pinned to catch an accidental change.
        assert_eq!(radius_of("circle(50px) garbage"), 50.0);
        assert_eq!(radius_of("circle(50px);garbage"), 50.0);
        assert_eq!(radius_of("circle(50px)!!!"), 50.0);
        // ...but leading junk becomes part of the function name and IS rejected.
        assert!(matches!(
            parse_shape("junk circle(50px)"),
            Err(ShapeParseError::UnknownFunction(ref f)) if f == "junk circle"
        ));
    }

    #[test]
    fn shape_surrounding_whitespace_is_trimmed_before_dispatch() {
        assert_eq!(radius_of("   circle(50px)   "), 50.0);
        assert_eq!(radius_of("\n\tcircle( 50px )\n"), 50.0);
    }

    #[test]
    fn shape_extra_closing_paren_inside_args_is_rejected_not_ignored() {
        // rfind(')') takes the LAST paren, so the inner one lands in the args.
        assert!(matches!(
            parse_shape("circle(50px))"),
            Err(ShapeParseError::InvalidNumber(ref n)) if n == "50px)"
        ));
        assert!(parse_shape("circle((50px)").is_err());
    }

    #[test]
    fn shape_unicode_input_does_not_panic_or_split_a_codepoint() {
        for input in [
            "\u{1F600}",
            "circle(\u{1F600})",
            "\u{1F600}(50px)",
            "cercle\u{301}(50px)",       // combining acute on the name
            "circle(50px\u{200b})",      // zero-width space glued to the unit
            "circle(\u{FF15}\u{FF10}px)", // fullwidth digits
            "円(50px)",
            "\u{202e}circle(50px)",  // RTL override
            "polygon(\u{1F4A9} \u{1F4A9}, 0 0, 1 1)",
            "path(\u{1F600})",
            "inset(\u{1F600} round \u{1F600})",
            "ellipse(\u{1F600} \u{1F600})",
        ] {
            let parsed = catch_unwind(AssertUnwindSafe(|| parse_shape(input)));
            assert!(parsed.is_ok(), "parse_shape({input:?}) panicked");
            assert!(
                parsed.unwrap().is_err(),
                "parse_shape({input:?}) unexpectedly succeeded"
            );
        }
    }

    #[test]
    fn shape_multibyte_name_slices_on_a_char_boundary() {
        // func_name = input[..open_paren]: the byte index of '(' must never land
        // mid-codepoint. A 4-byte emoji directly before '(' is the tight case.
        assert!(matches!(
            parse_shape("\u{1F600}(50px)"),
            Err(ShapeParseError::UnknownFunction(ref f)) if f == "\u{1F600}"
        ));
    }

    #[test]
    fn shape_deeply_nested_parens_do_not_stack_overflow() {
        // The parser is iterative (find/rfind), not recursive — 10k nesting
        // levels must terminate with a plain Err, not blow the stack.
        let depth = 10_000;
        let nested = format!("{}{}", "(".repeat(depth), ")".repeat(depth));
        assert!(matches!(
            parse_shape(&nested),
            Err(ShapeParseError::UnknownFunction(ref f)) if f.is_empty()
        ));

        let nested_circle = format!("circle{}50px{}", "(".repeat(5_000), ")".repeat(5_000));
        assert!(matches!(
            parse_shape(&nested_circle),
            Err(ShapeParseError::InvalidNumber(_))
        ));
    }

    #[test]
    fn shape_extremely_long_input_does_not_hang() {
        // 1M bytes with no '(' — must fail fast on the framing check.
        let long_garbage = "x".repeat(1_000_000);
        assert!(matches!(
            parse_shape(&long_garbage),
            Err(ShapeParseError::InvalidSyntax(_))
        ));

        // 50k-digit number: f32::from_str must saturate to +inf, not hang/panic.
        let huge = format!("circle({}px)", "9".repeat(50_000));
        let radius = radius_of(&huge);
        assert!(radius.is_infinite() && radius.is_sign_positive());
    }

    #[test]
    fn shape_parsing_is_deterministic() {
        let input = "polygon(0px 0px, 100px 0px, 50px 100px)";
        assert_eq!(parse_shape(input).unwrap(), parse_shape(input).unwrap());
        assert_eq!(parse_shape("!!!").unwrap_err(), parse_shape("!!!").unwrap_err());
    }

    #[test]
    fn shape_minimal_valid_input_per_function() {
        assert_eq!(circle_of(&parse_shape("circle(1)").unwrap()).radius, 1.0);
        assert_eq!(ellipse_of(&parse_shape("ellipse(1 2)").unwrap()).radius_y, 2.0);
        assert_eq!(polygon_points(&parse_shape("polygon(0 0,1 0,0 1)").unwrap()).len(), 3);
        assert_eq!(inset_of(&parse_shape("inset(0)").unwrap()).inset_top, 0.0);
        assert_eq!(path_data(&parse_shape("path(\"\")").unwrap()), "");
    }

    // =======================================================================
    // parse_function
    // =======================================================================

    #[test]
    fn function_empty_and_whitespace_input_is_err() {
        assert!(parse_function("").is_err());
        assert!(parse_function("   \t\n").is_err());
    }

    #[test]
    fn function_splits_and_trims_name_and_args() {
        let (name, args) = parse_function("  circle  (  50px  )  ").unwrap();
        assert_eq!(name, "circle");
        assert_eq!(args, "50px");

        // Empty function, empty args.
        let (name, args) = parse_function("()").unwrap();
        assert!(name.is_empty() && args.is_empty());
    }

    #[test]
    fn function_uses_first_open_and_last_close_paren() {
        let (name, args) = parse_function("a(b(c))").unwrap();
        assert_eq!(name, "a");
        assert_eq!(args, "b(c)", "rfind(')') must take the outermost close paren");
    }

    #[test]
    fn function_rejects_reversed_and_missing_parens() {
        assert!(matches!(
            parse_function(")("),
            Err(ShapeParseError::InvalidSyntax(ref m)) if m.contains("Invalid parentheses")
        ));
        assert!(parse_function("no parens here").is_err());
        assert!(parse_function("circle(").is_err());
        assert!(parse_function("circle)").is_err());
    }

    #[test]
    fn function_handles_pathological_lengths_without_panicking() {
        let long = "y".repeat(1_000_000);
        assert!(parse_function(&long).is_err());

        // 1M-char argument body: extraction is a slice, so this must be cheap.
        let long_args = format!("f({})", "z".repeat(1_000_000));
        let (name, args) = parse_function(&long_args).unwrap();
        assert_eq!(name, "f");
        assert_eq!(args.len(), 1_000_000);
    }

    #[test]
    fn function_multibyte_args_round_trip_intact() {
        let (name, args) = parse_function("f(\u{1F600}\u{0301})").unwrap();
        assert_eq!(name, "f");
        assert_eq!(args, "\u{1F600}\u{0301}");
    }

    // =======================================================================
    // parse_circle
    // =======================================================================

    #[test]
    fn circle_empty_or_whitespace_args_report_the_missing_radius() {
        assert!(matches!(
            parse_circle(""),
            Err(ShapeParseError::MissingParameter(ref p)) if p == "radius"
        ));
        assert!(matches!(
            parse_circle("  \t\n "),
            Err(ShapeParseError::MissingParameter(_))
        ));
    }

    #[test]
    fn circle_garbage_radius_is_an_invalid_number() {
        assert!(matches!(
            parse_circle("abc"),
            Err(ShapeParseError::InvalidNumber(ref n)) if n == "abc"
        ));
        assert!(parse_circle("at 10px 10px").is_err());
        assert!(parse_circle("50px at 10px abc").is_err());
    }

    #[test]
    fn current_circle_ignores_a_malformed_at_clause_instead_of_erroring() {
        // The `at` branch needs >= 4 parts AND parts[1] == "at" (lowercase); if
        // either check fails the position is silently dropped and the shape
        // still parses. CSS would reject these. Pinned as current behavior.
        let truncated = circle_of(&parse_circle("50px at 100px").unwrap());
        assert_eq!(truncated.center, ShapePoint::zero());

        let wrong_case = circle_of(&parse_circle("50px AT 100px 100px").unwrap());
        assert_eq!(wrong_case.center, ShapePoint::zero());

        let bad_keyword = circle_of(&parse_circle("50px on 100px 100px").unwrap());
        assert_eq!(bad_keyword.center, ShapePoint::zero());

        // Trailing extra parts past the `at x y` triple are dropped too.
        let extra = circle_of(&parse_circle("50px at 1px 2px 3px 4px").unwrap());
        assert_eq!(extra.center, ShapePoint::new(1.0, 2.0));
    }

    #[test]
    fn current_circle_accepts_a_negative_radius() {
        // CSS rejects negative radii; the parser passes them straight through.
        assert_eq!(radius_of("circle(-50px)"), -50.0);
        // -0.0 must keep its sign bit rather than collapse to +0.0.
        let neg_zero = radius_of("circle(-0px)");
        assert!(neg_zero == 0.0 && neg_zero.is_sign_negative());
    }

    #[test]
    fn circle_non_finite_and_saturating_radii_do_not_panic() {
        // f32::from_str accepts NaN/inf spellings, so they survive into the shape.
        assert!(radius_of("circle(NaN)").is_nan());
        assert!(radius_of("circle(NaNpx)").is_nan());
        assert!(radius_of("circle(inf)").is_infinite());
        assert!(radius_of("circle(infinitypx)").is_infinite());
        assert!(radius_of("circle(-inf)").is_sign_negative());

        // Overflow saturates to inf, underflow flushes to zero — no panic, no wrap.
        assert!(radius_of("circle(1e39px)").is_infinite());
        assert_eq!(radius_of("circle(1e-46px)"), 0.0);
        assert_eq!(radius_of("circle(9223372036854775807px)"), 9223372036854775807.0_f32);
    }

    #[test]
    fn circle_percentages_are_currently_kept_as_raw_numbers() {
        // TODO in the source: percentages need a container size. Until then a
        // "50%" radius is indistinguishable from "50px".
        assert_eq!(radius_of("circle(50%)"), 50.0);
        assert_eq!(radius_of("circle(50%)"), radius_of("circle(50px)"));
    }

    // =======================================================================
    // parse_ellipse
    // =======================================================================

    #[test]
    fn ellipse_requires_two_radii() {
        assert!(matches!(
            parse_ellipse(""),
            Err(ShapeParseError::MissingParameter(ref p)) if p.contains("radius_x")
        ));
        assert!(matches!(
            parse_ellipse("50px"),
            Err(ShapeParseError::MissingParameter(_))
        ));
        assert!(parse_ellipse("50px abc").is_err());
    }

    #[test]
    fn current_ellipse_ignores_a_truncated_at_clause() {
        // Needs >= 5 parts; "50px 75px at 100px" is 4, so the centre is dropped.
        let truncated = ellipse_of(&parse_ellipse("50px 75px at 100px").unwrap());
        assert_eq!(truncated.center, ShapePoint::zero());

        // 5 parts but no `at` keyword: extras are dropped, still Ok.
        let no_keyword = ellipse_of(&parse_ellipse("1px 2px 3px 4px 5px").unwrap());
        assert_eq!(no_keyword.center, ShapePoint::zero());
        assert_eq!((no_keyword.radius_x, no_keyword.radius_y), (1.0, 2.0));
    }

    #[test]
    fn ellipse_valid_input_maps_radii_and_centre_in_order() {
        let e = ellipse_of(&parse_shape("ellipse(50px 75px at 10px 20px)").unwrap());
        assert_eq!(e.radius_x, 50.0);
        assert_eq!(e.radius_y, 75.0);
        assert_eq!(e.center, ShapePoint::new(10.0, 20.0));
    }

    #[test]
    fn ellipse_non_finite_radii_do_not_panic() {
        let e = ellipse_of(&parse_shape("ellipse(NaN inf)").unwrap());
        assert!(e.radius_x.is_nan());
        assert!(e.radius_y.is_infinite());
    }

    // =======================================================================
    // parse_polygon
    // =======================================================================

    #[test]
    fn polygon_empty_and_whitespace_args_are_rejected() {
        // NB: `"".split(',')` yields one empty element, so `pairs` is never
        // empty and the MissingParameter branch is unreachable — the error
        // surfaces as InvalidSyntax from the x/y pair check instead.
        assert!(matches!(
            parse_polygon(""),
            Err(ShapeParseError::InvalidSyntax(_))
        ));
        assert!(matches!(
            parse_polygon("   \t "),
            Err(ShapeParseError::InvalidSyntax(_))
        ));
    }

    #[test]
    fn polygon_needs_at_least_three_points() {
        assert!(matches!(
            parse_polygon("0 0"),
            Err(ShapeParseError::InvalidSyntax(ref m)) if m.contains("at least 3 points")
        ));
        assert!(matches!(
            parse_polygon("0 0, 1 1"),
            Err(ShapeParseError::InvalidSyntax(ref m)) if m.contains("at least 3 points")
        ));
        assert_eq!(polygon_points(&parse_polygon("0 0, 1 1, 2 2").unwrap()).len(), 3);
    }

    #[test]
    fn polygon_malformed_pairs_are_rejected() {
        // Lone coordinate, trailing comma, doubled comma, missing y.
        assert!(parse_polygon("0 0, 1 1, 2").is_err());
        assert!(parse_polygon("0 0, 1 1, 2 2,").is_err());
        assert!(parse_polygon("0 0,, 1 1, 2 2").is_err());
        assert!(parse_polygon(",0 0, 1 1, 2 2").is_err());
        assert!(parse_polygon("0 0, 1 1, abc def").is_err());
    }

    #[test]
    fn polygon_fill_rule_prefix_is_stripped_only_when_comma_attached() {
        let nonzero = polygon_points(&parse_polygon("nonzero, 0 0, 1 0, 1 1").unwrap());
        assert_eq!(nonzero.len(), 3);
        assert_eq!(nonzero[0], ShapePoint::zero());

        let evenodd = polygon_points(&parse_polygon("evenodd, 0 0, 1 0, 1 1").unwrap());
        assert_eq!(evenodd.len(), 3);

        // The prefix check is `starts_with("nonzero,")` — a space before the
        // comma defeats it, and the keyword then fails as a coordinate.
        assert!(matches!(
            parse_polygon("nonzero , 0 0, 1 0, 1 1"),
            Err(ShapeParseError::InvalidSyntax(_))
        ));
        // No comma at all: "nonzero" is read as an x coordinate.
        assert!(matches!(
            parse_polygon("nonzero 0 0, 1 0, 1 1"),
            Err(ShapeParseError::InvalidNumber(ref n)) if n == "nonzero"
        ));
        // Fill rule with no points after it.
        assert!(parse_polygon("nonzero,").is_err());
    }

    #[test]
    fn current_polygon_ignores_extra_coordinates_in_a_pair() {
        // Only coords[0] and coords[1] are read; a stray third value is dropped.
        let points = polygon_points(&parse_polygon("0 0 999, 1 1 999, 2 2 999").unwrap());
        assert_eq!(points.len(), 3);
        assert_eq!(points[2], ShapePoint::new(2.0, 2.0));
    }

    #[test]
    fn current_polygon_mixes_units_silently() {
        // px, %, and unitless all collapse to the same raw f32 today.
        let points = polygon_points(&parse_polygon("0% 0px, 100 0%, 50px 100").unwrap());
        assert_eq!(points[1], ShapePoint::new(100.0, 0.0));
        assert_eq!(points[2], ShapePoint::new(50.0, 100.0));
    }

    #[test]
    fn polygon_non_finite_coordinates_do_not_panic() {
        let points = polygon_points(&parse_polygon("NaN 0, inf 1, -inf 2").unwrap());
        assert!(points[0].x.is_nan());
        assert!(points[1].x.is_infinite() && points[1].x.is_sign_positive());
        assert!(points[2].x.is_infinite() && points[2].x.is_sign_negative());
    }

    #[test]
    fn polygon_with_twenty_thousand_points_does_not_hang() {
        let mut args = String::with_capacity(20_000 * 10);
        for i in 0..20_000 {
            if i > 0 {
                args.push(',');
            }
            args.push_str("1px 2px");
        }
        let points = polygon_points(&parse_polygon(&args).unwrap());
        assert_eq!(points.len(), 20_000);
        assert_eq!(points[19_999], ShapePoint::new(1.0, 2.0));
    }

    // =======================================================================
    // parse_inset
    // =======================================================================

    #[test]
    fn inset_empty_args_report_missing_values() {
        assert!(matches!(
            parse_inset(""),
            Err(ShapeParseError::MissingParameter(ref p)) if p.contains("inset values")
        ));
        assert!(matches!(
            parse_inset("   "),
            Err(ShapeParseError::MissingParameter(_))
        ));
    }

    #[test]
    fn inset_shorthand_expansion_follows_the_margin_rules() {
        let one = inset_of(&parse_inset("10px").unwrap());
        assert_eq!(
            (one.inset_top, one.inset_right, one.inset_bottom, one.inset_left),
            (10.0, 10.0, 10.0, 10.0)
        );

        let two = inset_of(&parse_inset("10px 20px").unwrap());
        assert_eq!(
            (two.inset_top, two.inset_right, two.inset_bottom, two.inset_left),
            (10.0, 20.0, 10.0, 20.0)
        );

        let three = inset_of(&parse_inset("10px 20px 30px").unwrap());
        assert_eq!(
            (three.inset_top, three.inset_right, three.inset_bottom, three.inset_left),
            (10.0, 20.0, 30.0, 20.0)
        );

        let four = inset_of(&parse_inset("10px 20px 30px 40px").unwrap());
        assert_eq!(
            (four.inset_top, four.inset_right, four.inset_bottom, four.inset_left),
            (10.0, 20.0, 30.0, 40.0)
        );
    }

    #[test]
    fn inset_rejects_more_than_four_values() {
        assert!(matches!(
            parse_inset("1px 2px 3px 4px 5px"),
            Err(ShapeParseError::InvalidSyntax(ref m)) if m.contains("max 4")
        ));
        // Long runs must hit the same guard, not allocate their way through.
        let many = ["1px"; 1_000].join(" ");
        assert!(matches!(
            parse_inset(&many),
            Err(ShapeParseError::InvalidSyntax(_))
        ));
    }

    #[test]
    fn inset_round_keyword_splits_insets_from_the_radius() {
        let rounded = inset_of(&parse_inset("10px 20px round 5px").unwrap());
        assert_eq!(rounded.inset_top, 10.0);
        assert_eq!(rounded.inset_right, 20.0);
        assert!(matches!(rounded.border_radius, OptionF32::Some(r) if r == 5.0));

        let plain = inset_of(&parse_inset("10px").unwrap());
        assert!(matches!(plain.border_radius, OptionF32::None));
    }

    #[test]
    fn inset_malformed_round_clauses_are_rejected() {
        // "round" with no radius after it.
        assert!(matches!(
            parse_inset("10px round"),
            Err(ShapeParseError::InvalidNumber(ref n)) if n.is_empty()
        ));
        // "round" with no insets before it: the radius parses, then the value
        // list comes up empty.
        assert!(matches!(
            parse_inset("round 5px"),
            Err(ShapeParseError::MissingParameter(_))
        ));
        // A second "round" lands inside the radius token and fails to parse.
        assert!(parse_inset("10px round 5px round 6px").is_err());
        // `find("round")` is a substring search, so "roundup" also triggers the
        // split — and then fails on the leftover "up".
        assert!(matches!(
            parse_inset("10px roundup"),
            Err(ShapeParseError::InvalidNumber(ref n)) if n == "up"
        ));
    }

    #[test]
    fn inset_non_finite_and_negative_values_do_not_panic() {
        let nan = inset_of(&parse_inset("NaN").unwrap());
        assert!(nan.inset_top.is_nan() && nan.inset_left.is_nan());

        let huge = inset_of(&parse_inset("1e39px round 1e39px").unwrap());
        assert!(huge.inset_top.is_infinite());
        assert!(matches!(huge.border_radius, OptionF32::Some(r) if r.is_infinite()));

        // Negative insets/radii are accepted (CSS rejects a negative radius).
        let negative = inset_of(&parse_inset("-10px round -5px").unwrap());
        assert_eq!(negative.inset_top, -10.0);
        assert!(matches!(negative.border_radius, OptionF32::Some(r) if r == -5.0));
    }

    // =======================================================================
    // parse_path
    // =======================================================================

    #[test]
    fn path_requires_double_quotes_on_both_ends() {
        for unquoted in [
            "",
            "   ",
            "M 0 0",
            "\"M 0 0",
            "M 0 0\"",
            "'M 0 0'", // single quotes are not accepted
            "`M 0 0`",
        ] {
            let parsed = catch_unwind(AssertUnwindSafe(|| parse_path(unquoted)));
            assert!(parsed.is_ok(), "parse_path({unquoted:?}) panicked");
            assert!(
                matches!(parsed.unwrap(), Err(ShapeParseError::InvalidSyntax(ref m)) if m.contains("quoted")),
                "parse_path({unquoted:?}) should be an unquoted-syntax error"
            );
        }
    }

    #[test]
    fn path_strips_exactly_one_quote_from_each_end() {
        assert_eq!(path_data(&parse_path("\"\"").unwrap()), "");
        assert_eq!(path_data(&parse_path("\"M 0 0 Z\"").unwrap()), "M 0 0 Z");
        // Inner quotes are preserved verbatim.
        assert_eq!(path_data(&parse_path("\"a\"b\"").unwrap()), "a\"b");
    }

    #[test]
    fn current_path_data_is_stored_without_validation() {
        // ShapePath's doc says the data is stored but not interpreted, so any
        // garbage inside the quotes round-trips untouched.
        assert_eq!(path_data(&parse_shape("path(\"not svg at all\")").unwrap()), "not svg at all");
        assert_eq!(path_data(&parse_shape("path(\"\u{1F600}\")").unwrap()), "\u{1F600}");
        assert_eq!(path_data(&parse_shape("path(\"M 0 0 L NaN inf\")").unwrap()), "M 0 0 L NaN inf");
    }

    #[test]
    fn path_multibyte_content_slices_on_char_boundaries() {
        // args[1..len-1] indexes bytes: a 4-byte emoji adjacent to each quote is
        // the case that would split a codepoint if the bounds were wrong.
        let data = path_data(&parse_path("\"\u{1F600}\u{0301}\u{1F600}\"").unwrap());
        assert_eq!(data, "\u{1F600}\u{0301}\u{1F600}");
    }

    #[test]
    fn path_with_a_megabyte_of_data_does_not_hang() {
        let inner = "L 1 1 ".repeat(150_000);
        let arg = format!("\"{inner}\"");
        assert_eq!(path_data(&parse_path(&arg).unwrap()), inner);
    }

    // =======================================================================
    // parse_length
    // =======================================================================

    #[test]
    fn length_empty_and_whitespace_are_invalid_numbers() {
        assert!(matches!(
            parse_length(""),
            Err(ShapeParseError::InvalidNumber(ref n)) if n.is_empty()
        ));
        assert!(matches!(
            parse_length("  \t\n "),
            Err(ShapeParseError::InvalidNumber(ref n)) if n.is_empty()
        ));
        // Bare units with no number.
        assert!(matches!(
            parse_length("px"),
            Err(ShapeParseError::InvalidNumber(ref n)) if n == "px"
        ));
        assert!(parse_length("%").is_err());
    }

    #[test]
    fn length_accepts_px_percent_and_unitless() {
        assert_eq!(parse_length("50px").unwrap(), 50.0);
        assert_eq!(parse_length("50%").unwrap(), 50.0);
        assert_eq!(parse_length("50").unwrap(), 50.0);
        assert_eq!(parse_length("  50px  ").unwrap(), 50.0);
        assert_eq!(parse_length("+5px").unwrap(), 5.0);
        assert_eq!(parse_length(".5px").unwrap(), 0.5);
        assert_eq!(parse_length("5.px").unwrap(), 5.0);
        assert_eq!(parse_length("5e2px").unwrap(), 500.0);
    }

    #[test]
    fn current_length_rejects_uppercase_units_and_other_css_units() {
        // CSS units are case-insensitive and em/rem/vh/vw/pt are all legal; the
        // source has a TODO for the latter. Both are rejected today.
        for unsupported in ["50PX", "50Px", "1em", "1rem", "1vh", "1vw", "1pt", "1cm", "1fr"] {
            assert!(
                parse_length(unsupported).is_err(),
                "parse_length({unsupported:?}) unexpectedly succeeded"
            );
        }
    }

    #[test]
    fn length_rejects_non_css_numeric_syntax() {
        for bad in [
            "1_000px", "0x10px", "1,5px", "50px50px", "50%%", "50%px", "--5px", "5 0px", "50 px",
            "1e", "e5", "0b101", "١٠px", // arabic-indic digits
        ] {
            let parsed = catch_unwind(AssertUnwindSafe(|| parse_length(bad)));
            assert!(parsed.is_ok(), "parse_length({bad:?}) panicked");
            assert!(
                parsed.unwrap().is_err(),
                "parse_length({bad:?}) unexpectedly succeeded"
            );
        }
    }

    #[test]
    fn length_saturates_on_overflow_and_flushes_on_underflow() {
        assert!(parse_length("1e39").unwrap().is_infinite());
        assert!(parse_length("1e39px").unwrap().is_sign_positive());
        assert!(parse_length("-1e39px").unwrap().is_sign_negative());
        assert_eq!(parse_length("1e-46px").unwrap(), 0.0);
        assert_eq!(parse_length("-1e-46px").unwrap(), -0.0);
        // f32 boundary values survive exactly.
        assert_eq!(parse_length(&format!("{}px", f32::MAX)).unwrap(), f32::MAX);
        assert_eq!(parse_length(&format!("{}px", f32::MIN)).unwrap(), f32::MIN);
        assert_eq!(
            parse_length(&format!("{}px", f32::MIN_POSITIVE)).unwrap(),
            f32::MIN_POSITIVE
        );
        // A double-precision value beyond f32 range clamps to inf, not to a wrap.
        assert!(parse_length(&format!("{}px", f64::MAX)).unwrap().is_infinite());
    }

    #[test]
    fn current_length_accepts_nan_and_infinity_spellings() {
        // f32::from_str parses "NaN"/"inf"/"infinity" (case-insensitively), so
        // these reach the shape structs as non-finite radii/coordinates. CSS has
        // no such tokens — a future tightening should reject them here.
        assert!(parse_length("NaN").unwrap().is_nan());
        assert!(parse_length("nan").unwrap().is_nan());
        assert!(parse_length("NaNpx").unwrap().is_nan());
        assert!(parse_length("-NaN%").unwrap().is_nan());
        assert!(parse_length("inf").unwrap().is_infinite());
        assert!(parse_length("infinity").unwrap().is_infinite());
        assert!(parse_length("INFpx").unwrap().is_infinite());
        assert!(parse_length("-inf").unwrap().is_sign_negative());
    }

    #[test]
    fn length_preserves_the_sign_of_negative_zero() {
        let negative_zero = parse_length("-0px").unwrap();
        assert!(negative_zero == 0.0 && negative_zero.is_sign_negative());
        assert!(parse_length("0px").unwrap().is_sign_positive());
    }

    #[test]
    fn length_handles_huge_digit_strings_without_hanging() {
        // 100k digits: the float parser's slow path is bounded, and the result
        // saturates rather than wrapping or panicking.
        let huge = format!("{}px", "9".repeat(100_000));
        assert!(parse_length(&huge).unwrap().is_infinite());

        // 100k leading zeros still denote 1.0.
        let padded = format!("{}1px", "0".repeat(100_000));
        assert_eq!(parse_length(&padded).unwrap(), 1.0);

        // 1M non-numeric chars must fail fast.
        let garbage = "q".repeat(1_000_000);
        assert!(parse_length(&garbage).is_err());
    }

    #[test]
    fn length_error_payload_is_the_trimmed_input() {
        assert!(matches!(
            parse_length("  bogus  "),
            Err(ShapeParseError::InvalidNumber(ref n)) if n == "bogus"
        ));
        // The unit stays in the payload so the message shows what the user wrote.
        assert!(matches!(
            parse_length("bogus px"),
            Err(ShapeParseError::InvalidNumber(ref n)) if n == "bogus px"
        ));
    }

    // =======================================================================
    // ShapeParseError::fmt (Display)
    // =======================================================================

    #[test]
    fn error_display_is_non_empty_and_names_the_variant() {
        let cases = [
            (ShapeParseError::UnknownFunction("blob".into()), "Unknown shape function", "blob"),
            (ShapeParseError::MissingParameter("radius".into()), "Missing required parameter", "radius"),
            (ShapeParseError::InvalidNumber("12abc".into()), "Invalid numeric value", "12abc"),
            (ShapeParseError::InvalidSyntax("bad parens".into()), "Invalid syntax", "bad parens"),
        ];
        for (err, prefix, payload) in cases {
            let rendered = err.to_string();
            assert!(rendered.starts_with(prefix), "{rendered:?} lacks prefix {prefix:?}");
            assert!(rendered.contains(payload), "{rendered:?} lost its payload {payload:?}");
        }
        assert_eq!(ShapeParseError::EmptyInput.to_string(), "Empty input");
    }

    #[test]
    fn error_display_survives_hostile_payloads() {
        // Empty, brace-laden (no format re-interpretation), unicode, control
        // chars and a 100k payload must all render without panicking.
        let payloads = [
            String::new(),
            "{}{0}{name}%s%n".to_string(),
            "\u{1F600}\u{0301}\u{202e}".to_string(),
            "\0\u{7}\n\t".to_string(),
            "x".repeat(100_000),
        ];
        for payload in payloads {
            for err in [
                ShapeParseError::UnknownFunction(payload.clone()),
                ShapeParseError::MissingParameter(payload.clone()),
                ShapeParseError::InvalidNumber(payload.clone()),
                ShapeParseError::InvalidSyntax(payload.clone()),
            ] {
                let rendered = catch_unwind(AssertUnwindSafe(|| err.to_string()));
                assert!(rendered.is_ok(), "Display panicked on payload {payload:?}");
                let rendered = rendered.unwrap();
                assert!(!rendered.is_empty());
                assert!(
                    rendered.ends_with(&payload),
                    "payload must be emitted literally, not re-formatted"
                );
            }
        }
    }

    #[test]
    fn error_variants_render_distinctly_and_compare_by_value() {
        let same_payload = "x".to_string();
        let unknown = ShapeParseError::UnknownFunction(same_payload.clone());
        let missing = ShapeParseError::MissingParameter(same_payload.clone());
        assert_ne!(unknown, missing);
        assert_ne!(unknown.to_string(), missing.to_string());
        assert_eq!(unknown, ShapeParseError::UnknownFunction(same_payload));
        assert_eq!(unknown.clone(), unknown);
        // Debug must not be empty either (derive(Debug)).
        assert!(!format!("{unknown:?}").is_empty());
    }

    // =======================================================================
    // Round-trip: print_as_css_value -> parse_shape
    // =======================================================================

    fn assert_round_trips(shape: &CssShape) {
        let printed = shape.print_as_css_value();
        let reparsed = parse_shape(&printed)
            .unwrap_or_else(|e| panic!("{printed:?} did not re-parse: {e}"));
        assert_eq!(
            &reparsed, shape,
            "round-trip changed the shape via {printed:?}"
        );
        // Printing the re-parsed value must be a fixed point.
        assert_eq!(reparsed.print_as_css_value(), printed);
    }

    #[test]
    fn round_trip_every_shape_variant() {
        assert_round_trips(&CssShape::circle(ShapePoint::new(10.0, 20.0), 50.0));
        assert_round_trips(&CssShape::ellipse(ShapePoint::new(1.5, -2.5), 3.25, 4.75));
        assert_round_trips(&CssShape::polygon(
            vec![
                ShapePoint::new(0.0, 0.0),
                ShapePoint::new(100.0, 0.0),
                ShapePoint::new(50.0, 100.0),
            ]
            .into(),
        ));
        assert_round_trips(&CssShape::inset(1.0, 2.0, 3.0, 4.0));
        assert_round_trips(&CssShape::inset_rounded(1.0, 2.0, 3.0, 4.0, 5.0));
        assert_round_trips(&CssShape::Path(ShapePath {
            data: crate::corety::AzString::from("M 0 0 L 100 0 Z"),
        }));
    }

    #[test]
    fn round_trip_survives_extreme_and_negative_numbers() {
        assert_round_trips(&CssShape::circle(
            ShapePoint::new(f32::MIN, f32::MAX),
            f32::MIN_POSITIVE,
        ));
        assert_round_trips(&CssShape::inset(-0.0, -1.5, 1e-30, 1e30));
        assert_round_trips(&CssShape::ellipse(
            ShapePoint::new(-0.000_001, 123_456.79),
            0.1,
            0.2,
        ));
    }

    #[test]
    fn round_trip_of_non_finite_values_preserves_them() {
        // "{}" prints inf as "inf", which parse_length happily reads back — so
        // an inf radius survives a print/parse cycle instead of erroring out.
        let printed = CssShape::circle(ShapePoint::zero(), f32::INFINITY).print_as_css_value();
        assert_eq!(printed, "circle(infpx at 0px 0px)");
        assert!(circle_of(&parse_shape(&printed).unwrap()).radius.is_infinite());

        // NaN != NaN, so assert_round_trips can't be used — check field-wise.
        let printed = CssShape::circle(ShapePoint::zero(), f32::NAN).print_as_css_value();
        assert!(circle_of(&parse_shape(&printed).unwrap()).radius.is_nan());
    }

    #[test]
    fn round_trip_of_a_path_containing_quotes_and_parens() {
        // parse_function takes the LAST ')' and parse_path strips only the outer
        // quotes, so both survive an embedded ')' and an embedded '"'.
        for data in ["M 0 0)", ")", "a\"b", "", "M 0 0 L 1 1 Z"] {
            assert_round_trips(&CssShape::Path(ShapePath {
                data: crate::corety::AzString::from(data),
            }));
        }
    }

    #[test]
    fn current_round_trip_is_asymmetric_for_degenerate_polygons() {
        // The printer will happily emit a 0/1/2-point polygon, but the parser
        // requires >= 3 points — so these shapes cannot survive a CSS round-trip.
        for count in 0..3 {
            let points: Vec<ShapePoint> =
                (0..count).map(|i| ShapePoint::new(i as f32, i as f32)).collect();
            let printed = CssShape::polygon(points.into()).print_as_css_value();
            assert!(
                parse_shape(&printed).is_err(),
                "{printed:?} should not re-parse (fewer than 3 points)"
            );
        }
    }

    #[test]
    fn round_trip_from_the_css_source_side_is_a_fixed_point() {
        // parse -> print -> parse must converge for author-written CSS.
        for source in [
            "circle(50px at 100px 100px)",
            "ellipse(50px 75px at 10px 20px)",
            "polygon(0px 0px, 100px 0px, 100px 100px, 0px 100px)",
            "inset(10px 20px 30px 40px)",
            "inset(10px round 5px)",
            "path(\"M 0 0 L 100 0 L 100 100 Z\")",
        ] {
            let first = parse_shape(source).unwrap();
            let second = parse_shape(&first.print_as_css_value()).unwrap();
            assert_eq!(first, second, "{source:?} is not a parse/print fixed point");
        }
    }
}
