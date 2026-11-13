//! The parser of CSS basic shapes (CSS Shapes 1, 3.1) for `shape-inside`,
//! `shape-outside` and `clip-path`:
//!
//! - `circle( <shape-radius>? [ at <position> ]? )`
//! - `ellipse( [ <shape-radius>{2} ]? [ at <position> ]? )`
//! - `inset( <length-percentage>{1,4} [ round <length-percentage> ]? )`
//! - `polygon( <fill-rule>? , [ <length-percentage> <length-percentage> ]# )`
//! - `path( <fill-rule>? , <string> )`
//!
//! Lengths keep their units and percentages; they are measured against the
//! reference box when the shape is laid out (`CssShape::resolve`). Function
//! names and keywords are matched without regard to case, as CSS does.
//!
//! Two departures from the grammar, both on the lenient side: a number
//! without a unit is read as pixels (CSS allows that for zero only), and
//! `round` takes one radius for all four corners (CSS allows a whole
//! `border-radius`; more than one value is refused rather than misread).

use alloc::{
    string::{String, ToString},
    vec::Vec,
};

use crate::{
    props::basic::{length::SizeMetric, pixel::PixelValue},
    shape::{
        CssShape, ShapeFillRule, ShapeOffset, ShapePoint, ShapePosition, ShapeRadius,
    },
};

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

/// One token of a shape function's arguments: a word (a number, a length, a
/// keyword) or a quoted string, its quotes and escapes removed.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Token {
    Word(String),
    Str(String),
}

impl Token {
    /// The word, if this is one.
    fn word(&self) -> Option<&str> {
        match self {
            Self::Word(w) => Some(w),
            Self::Str(_) => None,
        }
    }

    /// Whether this is the keyword `keyword`, in any case.
    fn is_keyword(&self, keyword: &str) -> bool {
        self.word().is_some_and(|w| w.eq_ignore_ascii_case(keyword))
    }
}

/// Parses a CSS basic shape.
///
/// # Errors
///
/// Returns an error if `input` is not one of the basic shapes this module
/// reads (see the module documentation).
pub fn parse_shape(input: &str) -> Result<CssShape, ShapeParseError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(ShapeParseError::EmptyInput);
    }
    let open = input
        .find('(')
        .ok_or_else(|| ShapeParseError::InvalidSyntax("Missing opening parenthesis".into()))?;
    if !input.ends_with(')') {
        return Err(ShapeParseError::InvalidSyntax(
            "Missing closing parenthesis".into(),
        ));
    }
    let name = input[..open].trim();
    let groups = tokenize(&input[open + 1..input.len() - 1])?;
    match name.to_ascii_lowercase().as_str() {
        "circle" => parse_circle(&single_group(groups, "circle")?),
        "ellipse" => parse_ellipse(&single_group(groups, "ellipse")?),
        "inset" => parse_inset(&single_group(groups, "inset")?),
        "polygon" => parse_polygon(groups),
        "path" => parse_path(groups),
        _ => Err(ShapeParseError::UnknownFunction(name.to_string())),
    }
}

/// Splits a function's arguments into their comma-separated groups of
/// tokens. Strings are read with their escapes; a parenthesis outside a
/// string is refused, as no function nests here.
fn tokenize(args: &str) -> Result<Vec<Vec<Token>>, ShapeParseError> {
    let mut groups = alloc::vec![Vec::new()];
    let mut word = String::new();
    let mut chars = args.chars();
    let end_word = |word: &mut String, groups: &mut Vec<Vec<Token>>| {
        if !word.is_empty() {
            if let Some(group) = groups.last_mut() {
                group.push(Token::Word(core::mem::take(word)));
            }
        }
    };
    while let Some(ch) = chars.next() {
        match ch {
            '"' | '\'' => {
                end_word(&mut word, &mut groups);
                let mut text = String::new();
                let mut closed = false;
                while let Some(c) = chars.next() {
                    match c {
                        '\\' => {
                            if let Some(escaped) = chars.next() {
                                text.push(escaped);
                            }
                        }
                        c if c == ch => {
                            closed = true;
                            break;
                        }
                        c => text.push(c),
                    }
                }
                if !closed {
                    return Err(ShapeParseError::InvalidSyntax(
                        "Unterminated string".into(),
                    ));
                }
                if let Some(group) = groups.last_mut() {
                    group.push(Token::Str(text));
                }
            }
            ',' => {
                end_word(&mut word, &mut groups);
                groups.push(Vec::new());
            }
            '(' | ')' => {
                return Err(ShapeParseError::InvalidSyntax(alloc::format!(
                    "Unexpected '{ch}' in the shape's arguments"
                )));
            }
            c if c.is_whitespace() => end_word(&mut word, &mut groups),
            c => word.push(c),
        }
    }
    end_word(&mut word, &mut groups);
    if groups.len() == 1 && groups[0].is_empty() {
        groups.clear();
    }
    if groups.iter().any(Vec::is_empty) {
        return Err(ShapeParseError::InvalidSyntax(
            "Empty argument between commas".into(),
        ));
    }
    Ok(groups)
}

/// The one comma-free group of tokens `function` takes.
fn single_group(
    mut groups: Vec<Vec<Token>>,
    function: &str,
) -> Result<Vec<Token>, ShapeParseError> {
    match groups.len() {
        0 => Ok(Vec::new()),
        1 => Ok(groups.remove(0)),
        _ => Err(ShapeParseError::InvalidSyntax(alloc::format!(
            "{function}() takes no commas"
        ))),
    }
}

/// `circle( <shape-radius>? [ at <position> ]? )`: the closest side around
/// the centre where they are left out.
fn parse_circle(tokens: &[Token]) -> Result<CssShape, ShapeParseError> {
    let at = tokens.iter().position(|t| t.is_keyword("at"));
    let radii = &tokens[..at.unwrap_or(tokens.len())];
    let radius = match radii {
        [] => ShapeRadius::ClosestSide,
        [radius] => parse_radius(radius)?,
        _ => {
            return Err(ShapeParseError::InvalidSyntax(
                "circle() takes one radius".into(),
            ))
        }
    };
    let center = match at {
        Some(at) => parse_position(&tokens[at + 1..])?,
        None => ShapePosition::center(),
    };
    Ok(CssShape::circle(center, radius))
}

/// `ellipse( [ <shape-radius>{2} ]? [ at <position> ]? )`.
fn parse_ellipse(tokens: &[Token]) -> Result<CssShape, ShapeParseError> {
    let at = tokens.iter().position(|t| t.is_keyword("at"));
    let radii = &tokens[..at.unwrap_or(tokens.len())];
    let (radius_x, radius_y) = match radii {
        [] => (ShapeRadius::ClosestSide, ShapeRadius::ClosestSide),
        [x, y] => (parse_radius(x)?, parse_radius(y)?),
        _ => {
            return Err(ShapeParseError::MissingParameter(
                "ellipse() takes two radii or none".into(),
            ))
        }
    };
    let center = match at {
        Some(at) => parse_position(&tokens[at + 1..])?,
        None => ShapePosition::center(),
    };
    Ok(CssShape::ellipse(center, radius_x, radius_y))
}

/// `inset( <length-percentage>{1,4} [ round <length-percentage> ]? )`, the
/// insets expanded as `margin` expands its values.
fn parse_inset(tokens: &[Token]) -> Result<CssShape, ShapeParseError> {
    let round = tokens.iter().position(|t| t.is_keyword("round"));
    let insets = tokens[..round.unwrap_or(tokens.len())]
        .iter()
        .map(parse_length_percentage)
        .collect::<Result<Vec<_>, _>>()?;
    let (top, right, bottom, left) = match insets.as_slice() {
        [] => return Err(ShapeParseError::MissingParameter("inset values".into())),
        [all] => (*all, *all, *all, *all),
        [vertical, horizontal] => (*vertical, *horizontal, *vertical, *horizontal),
        [top, horizontal, bottom] => (*top, *horizontal, *bottom, *horizontal),
        [top, right, bottom, left] => (*top, *right, *bottom, *left),
        _ => {
            return Err(ShapeParseError::InvalidSyntax(
                "Too many inset values (max 4)".into(),
            ))
        }
    };
    match round {
        None => Ok(CssShape::inset(top, right, bottom, left)),
        Some(at) => match &tokens[at + 1..] {
            [radius] => Ok(CssShape::inset_rounded(
                top,
                right,
                bottom,
                left,
                parse_length_percentage(radius)?,
            )),
            [] => Err(ShapeParseError::MissingParameter("round radius".into())),
            _ => Err(ShapeParseError::InvalidSyntax(
                "round takes one radius for all corners".into(),
            )),
        },
    }
}

/// `polygon( <fill-rule>? , [ <length-percentage> <length-percentage> ]# )`.
fn parse_polygon(groups: Vec<Vec<Token>>) -> Result<CssShape, ShapeParseError> {
    let (fill_rule, points) = split_fill_rule(groups);
    if points.is_empty() {
        return Err(ShapeParseError::MissingParameter(
            "at least one point".into(),
        ));
    }
    let points = points
        .iter()
        .map(|pair| match pair.as_slice() {
            [x, y] => Ok(ShapePoint::new(
                parse_length_percentage(x)?,
                parse_length_percentage(y)?,
            )),
            _ => Err(ShapeParseError::InvalidSyntax(alloc::format!(
                "Expected an x y pair, got {} values",
                pair.len()
            ))),
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(CssShape::polygon(fill_rule, points.into()))
}

/// `path( <fill-rule>? , <string> )`.
fn parse_path(groups: Vec<Vec<Token>>) -> Result<CssShape, ShapeParseError> {
    let (fill_rule, rest) = split_fill_rule(groups);
    match rest.as_slice() {
        [data] => match data.as_slice() {
            [Token::Str(data)] => Ok(CssShape::path(fill_rule, data)),
            _ => Err(ShapeParseError::InvalidSyntax(
                "Path data must be quoted".into(),
            )),
        },
        [] => Err(ShapeParseError::MissingParameter("path data".into())),
        _ => Err(ShapeParseError::InvalidSyntax(
            "path() takes a fill rule and one string".into(),
        )),
    }
}

/// The fill rule a polygon's or a path's first argument names, and the
/// arguments after it; `nonzero` where the first argument is no fill rule.
fn split_fill_rule(mut groups: Vec<Vec<Token>>) -> (ShapeFillRule, Vec<Vec<Token>>) {
    let rule = match groups.first().map(Vec::as_slice) {
        Some([only]) if only.is_keyword("nonzero") => Some(ShapeFillRule::NonZero),
        Some([only]) if only.is_keyword("evenodd") => Some(ShapeFillRule::EvenOdd),
        _ => None,
    };
    match rule {
        Some(rule) => {
            groups.remove(0);
            (rule, groups)
        }
        None => (ShapeFillRule::NonZero, groups),
    }
}

/// A `<shape-radius>`: a length or percentage, `closest-side` or
/// `farthest-side`.
fn parse_radius(token: &Token) -> Result<ShapeRadius, ShapeParseError> {
    if token.is_keyword("closest-side") {
        Ok(ShapeRadius::ClosestSide)
    } else if token.is_keyword("farthest-side") {
        Ok(ShapeRadius::FarthestSide)
    } else {
        let length = parse_length_percentage(token)?;
        if length.number.get() < 0.0 {
            return Err(ShapeParseError::InvalidNumber(alloc::format!(
                "a radius cannot be negative: {length}"
            )));
        }
        Ok(ShapeRadius::Length(length))
    }
}

/// Which axis a position keyword names, and where along it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Edge {
    /// `left` or `right`.
    Horizontal(ShapeOffset),
    /// `top` or `bottom`.
    Vertical(ShapeOffset),
    /// `center`, either axis.
    Center,
}

/// The position keyword `token`, if it is one.
fn edge(token: &Token) -> Option<Edge> {
    let word = token.word()?.to_ascii_lowercase();
    let start = ShapeOffset::from_start(PixelValue::const_percent(0));
    let end = ShapeOffset::from_start(PixelValue::const_percent(100));
    match word.as_str() {
        "left" => Some(Edge::Horizontal(start)),
        "right" => Some(Edge::Horizontal(end)),
        "top" => Some(Edge::Vertical(start)),
        "bottom" => Some(Edge::Vertical(end)),
        "center" => Some(Edge::Center),
        _ => None,
    }
}

/// A `<position>` of one, two or four tokens (CSS Values 4).
fn parse_position(tokens: &[Token]) -> Result<ShapePosition, ShapeParseError> {
    let center = ShapeOffset::from_start(PixelValue::const_percent(50));
    let length = |t: &Token| parse_length_percentage(t).map(ShapeOffset::from_start);
    match tokens {
        [] => Err(ShapeParseError::MissingParameter(
            "a position after at".into(),
        )),
        [one] => Ok(match edge(one) {
            Some(Edge::Horizontal(x)) => ShapePosition { x, y: center },
            Some(Edge::Vertical(y)) => ShapePosition { x: center, y },
            Some(Edge::Center) => ShapePosition::center(),
            None => ShapePosition {
                x: length(one)?,
                y: center,
            },
        }),
        [first, second] => {
            let (first_edge, second_edge) = (edge(first), edge(second));
            // Two keywords may come in either order: `top left` is `left top`.
            let swapped = first_edge.is_some()
                && second_edge.is_some()
                && (matches!(first_edge, Some(Edge::Vertical(_)))
                    || matches!(second_edge, Some(Edge::Horizontal(_))));
            let ((horizontal, horizontal_edge), (vertical, vertical_edge)) = if swapped {
                ((second, second_edge), (first, first_edge))
            } else {
                ((first, first_edge), (second, second_edge))
            };
            let x = match horizontal_edge {
                Some(Edge::Horizontal(x)) => x,
                Some(Edge::Center) => center,
                Some(Edge::Vertical(_)) => {
                    return Err(ShapeParseError::InvalidSyntax(
                        "a vertical keyword cannot come before a length".into(),
                    ))
                }
                None => length(horizontal)?,
            };
            let y = match vertical_edge {
                Some(Edge::Vertical(y)) => y,
                Some(Edge::Center) => center,
                Some(Edge::Horizontal(_)) => {
                    return Err(ShapeParseError::InvalidSyntax(
                        "a horizontal keyword cannot come after a length".into(),
                    ))
                }
                None => length(vertical)?,
            };
            Ok(ShapePosition { x, y })
        }
        [a, a_offset, b, b_offset] => {
            let offset_from = |keyword: &Token, offset: &Token| -> Result<Edge, ShapeParseError> {
                let distance = parse_length_percentage(offset)?;
                let word = keyword.word().unwrap_or_default().to_ascii_lowercase();
                match word.as_str() {
                    "left" => Ok(Edge::Horizontal(ShapeOffset::from_start(distance))),
                    "right" => Ok(Edge::Horizontal(ShapeOffset::from_end(distance))),
                    "top" => Ok(Edge::Vertical(ShapeOffset::from_start(distance))),
                    "bottom" => Ok(Edge::Vertical(ShapeOffset::from_end(distance))),
                    _ => Err(ShapeParseError::InvalidSyntax(
                        "a four-value position names an edge before each offset".into(),
                    )),
                }
            };
            match (offset_from(a, a_offset)?, offset_from(b, b_offset)?) {
                (Edge::Horizontal(x), Edge::Vertical(y)) | (Edge::Vertical(y), Edge::Horizontal(x)) => {
                    Ok(ShapePosition { x, y })
                }
                _ => Err(ShapeParseError::InvalidSyntax(
                    "a four-value position names one horizontal and one vertical edge".into(),
                )),
            }
        }
        _ => Err(ShapeParseError::InvalidSyntax(alloc::format!(
            "a position has one, two or four values, not {}",
            tokens.len()
        ))),
    }
}

/// The units a `<length-percentage>` may carry, longest first where one
/// ends in another (`rem` before `em`, `vmin` before `in`).
const UNITS: [(&str, SizeMetric); 12] = [
    ("px", SizeMetric::Px),
    ("rem", SizeMetric::Rem),
    ("em", SizeMetric::Em),
    ("pt", SizeMetric::Pt),
    ("vmax", SizeMetric::Vmax),
    ("vmin", SizeMetric::Vmin),
    ("vw", SizeMetric::Vw),
    ("vh", SizeMetric::Vh),
    ("in", SizeMetric::In),
    ("mm", SizeMetric::Mm),
    ("cm", SizeMetric::Cm),
    ("%", SizeMetric::Percent),
];

/// A `<length-percentage>`: a number and one of the units of [`UNITS`], or
/// a bare number, read as pixels.
fn parse_length_percentage(token: &Token) -> Result<PixelValue, ShapeParseError> {
    let Some(word) = token.word() else {
        return Err(ShapeParseError::InvalidNumber(
            "a string is no length".into(),
        ));
    };
    let lower = word.to_ascii_lowercase();
    let (number, metric) = UNITS
        .iter()
        .find_map(|(unit, metric)| lower.strip_suffix(unit).map(|n| (n, *metric)))
        .unwrap_or((lower.as_str(), SizeMetric::Px));
    if !is_css_number(number) {
        return Err(ShapeParseError::InvalidNumber(word.to_string()));
    }
    number
        .parse::<f32>()
        .ok()
        .filter(|n| n.is_finite())
        .map(|n| PixelValue::from_metric(metric, n))
        .ok_or_else(|| ShapeParseError::InvalidNumber(word.to_string()))
}

/// Whether `text` is a CSS `<number>`: a sign, digits with a decimal point
/// somewhere, an exponent - and not Rust's `inf` or `NaN`.
fn is_css_number(text: &str) -> bool {
    let text = text.strip_prefix(['+', '-']).unwrap_or(text);
    let (mantissa, exponent) = text
        .split_once(['e', 'E'])
        .map_or((text, None), |(mantissa, exponent)| (mantissa, Some(exponent)));
    let digits_and_point = |s: &str| {
        let points = s.chars().filter(|&c| c == '.').count();
        points <= 1 && s.chars().any(|c| c.is_ascii_digit()) && s.chars().all(|c| c.is_ascii_digit() || c == '.')
    };
    let exponent_ok = exponent.is_none_or(|e| {
        let e = e.strip_prefix(['+', '-']).unwrap_or(e);
        !e.is_empty() && e.chars().all(|c| c.is_ascii_digit())
    });
    digits_and_point(mantissa) && exponent_ok
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        props::basic::pixel::OptionPixelValue,
        shape::{ShapeCircle, ShapeEllipse, ShapeInset, ShapePath, ShapePolygon},
    };

    fn circle(input: &str) -> ShapeCircle {
        match parse_shape(input) {
            Ok(CssShape::Circle(c)) => c,
            other => panic!("{input}: expected a circle, got {other:?}"),
        }
    }

    #[test]
    fn a_circle_without_arguments_is_the_closest_side_around_the_centre() {
        let c = circle("circle()");
        assert_eq!(c.radius, ShapeRadius::ClosestSide);
        assert_eq!(c.center, ShapePosition::center());
    }

    #[test]
    fn a_circle_keeps_its_radius_and_position_with_their_units() {
        let c = circle("circle(50% at 10pt 2em)");
        assert_eq!(c.radius, ShapeRadius::Length(PixelValue::percent(50.0)));
        assert_eq!(c.center.x, ShapeOffset::from_start(PixelValue::pt(10.0)));
        assert_eq!(c.center.y, ShapeOffset::from_start(PixelValue::em(2.0)));
        assert_eq!(circle("CIRCLE(Farthest-Side)").radius, ShapeRadius::FarthestSide);
    }

    #[test]
    fn a_position_reads_keywords_in_either_order_and_offsets_from_edges() {
        let percent = |p: f32| ShapeOffset::from_start(PixelValue::percent(p));
        let at = |pos: &str| circle(&alloc::format!("circle(5px at {pos})")).center;
        assert_eq!(at("left"), ShapePosition { x: percent(0.0), y: percent(50.0) });
        assert_eq!(at("bottom"), ShapePosition { x: percent(50.0), y: percent(100.0) });
        assert_eq!(at("top right"), ShapePosition { x: percent(100.0), y: percent(0.0) });
        assert_eq!(at("right top"), ShapePosition { x: percent(100.0), y: percent(0.0) });
        assert_eq!(at("center bottom"), ShapePosition { x: percent(50.0), y: percent(100.0) });
        assert_eq!(
            at("right 10px bottom 20%"),
            ShapePosition {
                x: ShapeOffset::from_end(PixelValue::px(10.0)),
                y: ShapeOffset::from_end(PixelValue::percent(20.0)),
            }
        );
        assert!(parse_shape("circle(5px at 10px left)").is_err());
        assert!(parse_shape("circle(5px at 1px 2px 3px)").is_err());
    }

    #[test]
    fn an_ellipse_takes_two_radii_or_none() {
        match parse_shape("ellipse(50px 75% at 100px 100px)") {
            Ok(CssShape::Ellipse(ShapeEllipse {
                radius_x, radius_y, ..
            })) => {
                assert_eq!(radius_x, ShapeRadius::Length(PixelValue::px(50.0)));
                assert_eq!(radius_y, ShapeRadius::Length(PixelValue::percent(75.0)));
            }
            other => panic!("{other:?}"),
        }
        assert!(matches!(parse_shape("ellipse()"), Ok(CssShape::Ellipse(_))));
        assert!(parse_shape("ellipse(50px)").is_err());
    }

    #[test]
    fn an_inset_expands_like_a_margin_and_takes_one_round_radius() {
        let inset = |input: &str| match parse_shape(input) {
            Ok(CssShape::Inset(i)) => i,
            other => panic!("{input}: {other:?}"),
        };
        let i = inset("inset(10px 5%)");
        assert_eq!(
            (i.inset_top, i.inset_right, i.inset_bottom, i.inset_left),
            (
                PixelValue::px(10.0),
                PixelValue::percent(5.0),
                PixelValue::px(10.0),
                PixelValue::percent(5.0)
            )
        );
        let ShapeInset { border_radius, .. } = inset("inset(1px 2px 3px 4px round 2pt)");
        assert_eq!(border_radius, OptionPixelValue::Some(PixelValue::pt(2.0)));
        assert!(parse_shape("inset(1px round 2px 3px)").is_err());
        assert!(parse_shape("inset(1px 2px 3px 4px 5px)").is_err());
    }

    #[test]
    fn a_polygon_takes_a_fill_rule_and_points_of_any_unit() {
        match parse_shape("polygon(evenodd, 0 0, 50% 0, 100pt 2in)") {
            Ok(CssShape::Polygon(ShapePolygon { fill_rule, points })) => {
                assert_eq!(fill_rule, ShapeFillRule::EvenOdd);
                assert_eq!(points.as_ref().len(), 3);
                assert_eq!(points.as_ref()[1].x, PixelValue::percent(50.0));
                assert_eq!(points.as_ref()[2].x, PixelValue::pt(100.0));
                assert_eq!(points.as_ref()[2].y, PixelValue::from_metric(SizeMetric::In, 2.0));
            }
            other => panic!("{other:?}"),
        }
        match parse_shape("polygon(0 0, 10px 0, 0 10px)") {
            Ok(CssShape::Polygon(ShapePolygon { fill_rule, .. })) => {
                assert_eq!(fill_rule, ShapeFillRule::NonZero, "nonzero is the initial rule");
            }
            other => panic!("{other:?}"),
        }
        assert!(parse_shape("polygon(evenodd)").is_err());
        assert!(parse_shape("polygon(0 0 0, 1 1)").is_err());
    }

    #[test]
    fn a_path_takes_a_fill_rule_and_a_string_in_either_quote() {
        let path = |input: &str| match parse_shape(input) {
            Ok(CssShape::Path(p)) => p,
            other => panic!("{input}: {other:?}"),
        };
        let ShapePath { fill_rule, data } = path("path(evenodd, 'M0 0H10V10Z M2 2H4V4Z')");
        assert_eq!(fill_rule, ShapeFillRule::EvenOdd);
        assert_eq!(data.as_str(), "M0 0H10V10Z M2 2H4V4Z");
        let ShapePath { fill_rule, data } = path("path(\"M 0 0 L 1 1 Z\")");
        assert_eq!(fill_rule, ShapeFillRule::NonZero);
        assert_eq!(data.as_str(), "M 0 0 L 1 1 Z");
        assert_eq!(path(r#"path("a\"b")"#).data.as_str(), "a\"b");
        assert!(parse_shape("path(M 0 0)").is_err());
        assert!(parse_shape("path(\"unterminated)").is_err());
        assert!(parse_shape("path(\")").is_err());
    }

    #[test]
    fn a_length_is_a_css_number_with_a_unit_or_none() {
        for bad in ["circle(NaNpx)", "circle(infpx)", "circle(1..2px)", "circle(px)", "circle(1e)"] {
            assert!(parse_shape(bad).is_err(), "{bad}");
        }
        assert_eq!(circle("circle(1.5e1px)").radius, ShapeRadius::Length(PixelValue::px(15.0)));
        assert_eq!(circle("circle(.5)").radius, ShapeRadius::Length(PixelValue::px(0.5)));
        assert!(parse_shape("circle(-1px)").is_err(), "a radius is not negative");
    }

    #[test]
    fn malformed_shapes_name_what_is_wrong() {
        assert_eq!(parse_shape("  "), Err(ShapeParseError::EmptyInput));
        assert!(matches!(parse_shape("star(1px)"), Err(ShapeParseError::UnknownFunction(n)) if n == "star"));
        assert!(matches!(parse_shape("circle 1px"), Err(ShapeParseError::InvalidSyntax(_))));
        assert!(matches!(parse_shape("circle(1px"), Err(ShapeParseError::InvalidSyntax(_))));
        assert!(matches!(parse_shape("circle(calc(1px))"), Err(ShapeParseError::InvalidSyntax(_))));
        assert!(matches!(parse_shape("polygon(0 0,, 1 1)"), Err(ShapeParseError::InvalidSyntax(_))));
    }

    #[test]
    fn every_shape_prints_back_to_itself() {
        for input in [
            "circle(closest-side at 50% 50%)",
            "circle(10px at right 5pt bottom 20%)",
            "ellipse(1em 2rem at 3px 4px)",
            "polygon(evenodd, 0px 0px, 100% 0px, 50% 3pt)",
            "inset(1px 2px 3px 4px round 5px)",
            "path(evenodd, \"M 0 0 L 1 1 Z\")",
        ] {
            let shape = parse_shape(input).expect(input);
            let printed = shape.print_as_css_value();
            assert_eq!(parse_shape(&printed).as_ref(), Ok(&shape), "{input} printed as {printed}");
        }
    }
}
