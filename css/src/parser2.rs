//! High-level types and functions related to CSS parsing.
//!
//! Main entry point: [`new_from_str`] parses a CSS string into a [`Css`] value
//! plus a list of recoverable warnings. Errors are downgraded to warnings so
//! that partially-valid CSS still produces usable output.
//!
//! Supports `@media`, `@theme`, `@os`, `@lang`, and `@container`
//! at-rules, CSS nesting, CSS variables (`var(--name, default)`), and
//! comma-separated selector lists. Tokenisation is delegated to `azul_simplecss`.
//!
//! Most error types come in borrowed/owned pairs (e.g. `CssParseError<'a>` /
//! `CssParseErrorOwned`) so they can be returned across the FFI boundary.
use alloc::{collections::BTreeMap, string::ToString, vec::Vec};
use core::{fmt, num::ParseIntError};

pub use azul_simplecss::Error as SimplecssError;
use azul_simplecss::Tokenizer;

/// FFI-safe position of a CSS syntax error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct CssSyntaxErrorPos {
    pub row: usize,
    pub col: usize,
}

impl From<azul_simplecss::ErrorPos> for CssSyntaxErrorPos {
    fn from(p: azul_simplecss::ErrorPos) -> Self {
        Self { row: p.row, col: p.col }
    }
}

/// FFI-safe wrapper for invalid advance details in CSS syntax errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct CssSyntaxInvalidAdvance {
    pub expected: isize,
    pub total: usize,
    pub pos: CssSyntaxErrorPos,
}

/// FFI-safe CSS syntax error type, mirrors `azul_simplecss::Error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C, u8)]
pub enum CssSyntaxError {
    UnexpectedEndOfStream(CssSyntaxErrorPos),
    InvalidAdvance(CssSyntaxInvalidAdvance),
    UnsupportedToken(CssSyntaxErrorPos),
    UnknownToken(CssSyntaxErrorPos),
}

impl From<SimplecssError> for CssSyntaxError {
    fn from(e: SimplecssError) -> Self {
        match e {
            SimplecssError::UnexpectedEndOfStream(pos) => Self::UnexpectedEndOfStream(pos.into()),
            SimplecssError::InvalidAdvance { expected, total, pos } => Self::InvalidAdvance(CssSyntaxInvalidAdvance { expected, total, pos: pos.into() }),
            SimplecssError::UnsupportedToken(pos) => Self::UnsupportedToken(pos.into()),
            SimplecssError::UnknownToken(pos) => Self::UnknownToken(pos.into()),
        }
    }
}

pub use crate::props::property::CssParsingError;
use crate::{
    corety::{AzString, OptionString},
    css::{
        AttributeMatchOp, Css, CssAttributeSelector, CssDeclaration, CssNthChildSelector, CssPath,
        CssPathPseudoSelector, CssPathSelector, CssRuleBlock, DynamicCssProperty, NodeTypeTag,
        NodeTypeTagParseError, NodeTypeTagParseErrorOwned,
    },
    dynamic_selector::{
        BoolCondition, DynamicSelector, DynamicSelectorVec, LanguageCondition, MediaType,
        MinMaxRange, OrientationType, OsCondition, ThemeCondition, parse_os_version,
    },
    props::{
        basic::parse::parse_parentheses,
        property::{
            parse_combined_css_property, parse_css_property, CombinedCssPropertyType, CssKeyMap,
            CssParsingErrorOwned, CssPropertyType,
        },
    },
};

/// Error that can happen during the parsing of a CSS value
#[derive(Debug, Clone, PartialEq)]
pub struct CssParseError<'a> {
    pub css_string: &'a str,
    pub error: CssParseErrorInner<'a>,
    pub location: ErrorLocationRange,
}

/// Owned version of `CssParseError`, without references.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct CssParseErrorOwned {
    pub css_string: AzString,
    pub error: CssParseErrorInnerOwned,
    pub location: ErrorLocationRange,
}

impl CssParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> CssParseErrorOwned {
        CssParseErrorOwned {
            css_string: self.css_string.to_string().into(),
            error: self.error.to_contained(),
            location: self.location,
        }
    }
}

impl CssParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> CssParseError<'_> {
        CssParseError {
            css_string: self.css_string.as_str(),
            error: self.error.to_shared(),
            location: self.location,
        }
    }
}

/// Clamps a byte offset into `s` so it is in-bounds AND on a UTF-8 char boundary,
/// rounding DOWN to the nearest boundary.
///
/// Error locations are recorded as raw byte offsets and are reachable from public,
/// unvalidated fields, so they cannot be fed to a slice directly: an out-of-range or
/// mid-character offset panics. Every error-reporting path routes through here.
#[must_use]
fn clamp_to_char_boundary(s: &str, pos: usize) -> usize {
    let mut pos = pos.min(s.len());
    while pos > 0 && !s.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}

impl<'a> CssParseError<'a> {
    /// Returns the string between the (start, end) location
    #[must_use] pub fn get_error_string(&self) -> &'a str {
        let (start, end) = (self.location.start.original_pos, self.location.end.original_pos);
        // `location` is a pub field on a pub struct and `CssParseErrorOwned::to_shared`
        // rebuilds one without revalidating, so start/end are NOT trustworthy: they can
        // sit past the end, be reversed, or land inside a multi-byte char. A raw slice
        // panics on all three -- while merely *displaying* an error. Clamp instead.
        let start = clamp_to_char_boundary(self.css_string, start);
        let end = clamp_to_char_boundary(self.css_string, end);
        let (start, end) = (start.min(end), start.max(end));
        self.css_string[start..end].trim()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CssParseErrorInner<'a> {
    /// A hard error in the CSS syntax
    ParseError(CssSyntaxError),
    /// Braces are not balanced properly
    UnclosedBlock,
    /// Invalid syntax, such as `#div { #div: "my-value" }`
    MalformedCss,
    /// Error parsing dynamic CSS property, such as
    /// `#div { width: {{ my_id }} /* no default case */ }`
    DynamicCssParseError(DynamicCssParseError<'a>),
    /// Error while parsing a pseudo selector (like `:aldkfja`)
    PseudoSelectorParseError(CssPseudoSelectorParseError<'a>),
    /// The path has to be either `*`, `div`, `p` or something like that
    NodeTypeTag(NodeTypeTagParseError<'a>),
    /// A certain property has an unknown key, for example: `alsdfkj: 500px` = `unknown CSS key
    /// "alsdfkj: 500px"`
    UnknownPropertyKey(&'a str, &'a str),
    /// `var()` can't be used on properties that expand to multiple values, since they would be
    /// ambiguous and degrade performance - for example `margin: var(--blah)` would be ambiguous
    /// because it's not clear when setting the variable, whether all sides should be set,
    /// instead, you have to use `margin-top: var(--blah)`, `margin-bottom: var(--baz)` in order
    /// to work around this limitation.
    VarOnShorthandProperty {
        key: CombinedCssPropertyType,
        value: &'a str,
    },
}

/// Wrapper for `UnknownPropertyKey` error.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct UnknownPropertyKeyError {
    pub key: AzString,
    pub value: AzString,
}

/// Wrapper for `VarOnShorthandProperty` error.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct VarOnShorthandPropertyError {
    pub key: CombinedCssPropertyType,
    pub value: AzString,
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum CssParseErrorInnerOwned {
    ParseError(CssSyntaxError),
    UnclosedBlock,
    MalformedCss,
    DynamicCssParseError(DynamicCssParseErrorOwned),
    PseudoSelectorParseError(CssPseudoSelectorParseErrorOwned),
    NodeTypeTag(NodeTypeTagParseErrorOwned),
    UnknownPropertyKey(UnknownPropertyKeyError),
    VarOnShorthandProperty(VarOnShorthandPropertyError),
}

impl CssParseErrorInner<'_> {
    #[must_use] pub fn to_contained(&self) -> CssParseErrorInnerOwned {
        match self {
            CssParseErrorInner::ParseError(e) => CssParseErrorInnerOwned::ParseError(*e),
            CssParseErrorInner::UnclosedBlock => CssParseErrorInnerOwned::UnclosedBlock,
            CssParseErrorInner::MalformedCss => CssParseErrorInnerOwned::MalformedCss,
            CssParseErrorInner::DynamicCssParseError(e) => {
                CssParseErrorInnerOwned::DynamicCssParseError(e.to_contained())
            }
            CssParseErrorInner::PseudoSelectorParseError(e) => {
                CssParseErrorInnerOwned::PseudoSelectorParseError(e.to_contained())
            }
            CssParseErrorInner::NodeTypeTag(e) => {
                CssParseErrorInnerOwned::NodeTypeTag(e.to_contained())
            }
            CssParseErrorInner::UnknownPropertyKey(a, b) => {
                CssParseErrorInnerOwned::UnknownPropertyKey(UnknownPropertyKeyError { key: (*a).to_string().into(), value: (*b).to_string().into() })
            }
            CssParseErrorInner::VarOnShorthandProperty { key, value } => {
                CssParseErrorInnerOwned::VarOnShorthandProperty(VarOnShorthandPropertyError {
                    key: *key,
                    value: (*value).to_string().into(),
                })
            }
        }
    }
}

impl CssParseErrorInnerOwned {
    #[must_use] pub fn to_shared(&self) -> CssParseErrorInner<'_> {
        match self {
            Self::ParseError(e) => CssParseErrorInner::ParseError(*e),
            Self::UnclosedBlock => CssParseErrorInner::UnclosedBlock,
            Self::MalformedCss => CssParseErrorInner::MalformedCss,
            Self::DynamicCssParseError(e) => {
                CssParseErrorInner::DynamicCssParseError(e.to_shared())
            }
            Self::PseudoSelectorParseError(e) => {
                CssParseErrorInner::PseudoSelectorParseError(e.to_shared())
            }
            Self::NodeTypeTag(e) => {
                CssParseErrorInner::NodeTypeTag(e.to_shared())
            }
            Self::UnknownPropertyKey(e) => {
                CssParseErrorInner::UnknownPropertyKey(e.key.as_str(), e.value.as_str())
            }
            Self::VarOnShorthandProperty(e) => {
                CssParseErrorInner::VarOnShorthandProperty {
                    key: e.key,
                    value: e.value.as_str(),
                }
            }
        }
    }
}

impl_display! { CssParseErrorInner<'a>, {
    ParseError(e) => format!("Parse Error: {:?}", e),
    UnclosedBlock => "Unclosed block",
    MalformedCss => "Malformed Css",
    DynamicCssParseError(e) => format!("{}", e),
    PseudoSelectorParseError(e) => format!("Failed to parse pseudo-selector: {}", e),
    NodeTypeTag(e) => format!("Failed to parse CSS selector path: {}", e),
    UnknownPropertyKey(k, v) => format!("Unknown CSS key: \"{}: {}\"", k, v),
    VarOnShorthandProperty { key, value } => format!(
        "Error while parsing: \"{}: {};\": var() cannot be used on shorthand properties - use `{}-top` or `{}-x` as the key instead: ",
        key, value, key, key
    ),
}}

impl From<CssSyntaxError> for CssParseErrorInner<'_> {
    fn from(e: CssSyntaxError) -> Self {
        CssParseErrorInner::ParseError(e)
    }
}

impl From<SimplecssError> for CssParseErrorInner<'_> {
    fn from(e: SimplecssError) -> Self {
        CssParseErrorInner::ParseError(CssSyntaxError::from(e))
    }
}

impl_from! { DynamicCssParseError<'a>, CssParseErrorInner::DynamicCssParseError }
impl_from! { NodeTypeTagParseError<'a>, CssParseErrorInner::NodeTypeTag }
impl_from! { CssPseudoSelectorParseError<'a>, CssParseErrorInner::PseudoSelectorParseError }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CssPseudoSelectorParseError<'a> {
    EmptyNthChild,
    UnknownSelector(&'a str, Option<&'a str>),
    InvalidNthChildPattern(&'a str),
    InvalidNthChild(ParseIntError),
}

impl From<ParseIntError> for CssPseudoSelectorParseError<'_> {
    fn from(e: ParseIntError) -> Self {
        CssPseudoSelectorParseError::InvalidNthChild(e)
    }
}

impl_display! { CssPseudoSelectorParseError<'a>, {
    EmptyNthChild => format!("\
        Empty :nth-child() selector - nth-child() must at least take a number, \
        a pattern (such as \"2n+3\") or the values \"even\" or \"odd\"."
    ),
    UnknownSelector(selector, value) => {
        let format_str = value
            .as_ref()
            .map_or_else(|| (*selector).to_string(), |v| format!("{selector}({v})"));
        format!("Invalid or unknown CSS pseudo-selector: ':{format_str}'")
    },
    InvalidNthChildPattern(selector) => format!(
        "Invalid pseudo-selector :{} - value has to be a \
        number, \"even\" or \"odd\" or a pattern such as \"2n+3\"", selector
    ),
    InvalidNthChild(e) => format!("Invalid :nth-child pseudo-selector: ':{}'", e),
}}

/// Wrapper for `UnknownSelector` error.
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct UnknownSelectorError {
    pub selector: AzString,
    pub suggestion: OptionString,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum CssPseudoSelectorParseErrorOwned {
    EmptyNthChild,
    UnknownSelector(UnknownSelectorError),
    InvalidNthChildPattern(AzString),
    InvalidNthChild(crate::props::basic::error::ParseIntError),
}

impl CssPseudoSelectorParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> CssPseudoSelectorParseErrorOwned {
        match self {
            CssPseudoSelectorParseError::EmptyNthChild => {
                CssPseudoSelectorParseErrorOwned::EmptyNthChild
            }
            CssPseudoSelectorParseError::UnknownSelector(a, b) => {
                CssPseudoSelectorParseErrorOwned::UnknownSelector(UnknownSelectorError {
                    selector: (*a).to_string().into(),
                    suggestion: b.map(|s| AzString::from(s.to_string())).into(),
                })
            }
            CssPseudoSelectorParseError::InvalidNthChildPattern(s) => {
                CssPseudoSelectorParseErrorOwned::InvalidNthChildPattern((*s).to_string().into())
            }
            CssPseudoSelectorParseError::InvalidNthChild(e) => {
                CssPseudoSelectorParseErrorOwned::InvalidNthChild(e.clone().into())
            }
        }
    }
}

impl CssPseudoSelectorParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> CssPseudoSelectorParseError<'_> {
        match self {
            Self::EmptyNthChild => {
                CssPseudoSelectorParseError::EmptyNthChild
            }
            Self::UnknownSelector(e) => {
                CssPseudoSelectorParseError::UnknownSelector(e.selector.as_str(), e.suggestion.as_ref().map(AzString::as_str))
            }
            Self::InvalidNthChildPattern(s) => {
                CssPseudoSelectorParseError::InvalidNthChildPattern(s)
            }
            Self::InvalidNthChild(e) => {
                CssPseudoSelectorParseError::InvalidNthChild(e.to_std())
            }
        }
    }
}

/// Error that can happen during `css_parser::parse_key_value_pair`
#[derive(Debug, Clone, PartialEq)]
pub enum DynamicCssParseError<'a> {
    /// The brace contents aren't valid, i.e. `var(asdlfkjasf)`
    InvalidBraceContents(&'a str),
    /// Unexpected value when parsing the string
    UnexpectedValue(CssParsingError<'a>),
}

impl_display! { DynamicCssParseError<'a>, {
    InvalidBraceContents(e) => format!("Invalid contents of var() function: var({})", e),
    UnexpectedValue(e) => format!("{}", e),
}}

impl<'a> From<CssParsingError<'a>> for DynamicCssParseError<'a> {
    fn from(e: CssParsingError<'a>) -> Self {
        DynamicCssParseError::UnexpectedValue(e)
    }
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum DynamicCssParseErrorOwned {
    InvalidBraceContents(AzString),
    UnexpectedValue(CssParsingErrorOwned),
}

impl DynamicCssParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> DynamicCssParseErrorOwned {
        match self {
            DynamicCssParseError::InvalidBraceContents(s) => {
                DynamicCssParseErrorOwned::InvalidBraceContents((*s).to_string().into())
            }
            DynamicCssParseError::UnexpectedValue(e) => {
                DynamicCssParseErrorOwned::UnexpectedValue(e.to_contained())
            }
        }
    }
}

impl DynamicCssParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> DynamicCssParseError<'_> {
        match self {
            Self::InvalidBraceContents(s) => {
                DynamicCssParseError::InvalidBraceContents(s)
            }
            Self::UnexpectedValue(e) => {
                DynamicCssParseError::UnexpectedValue(e.to_shared())
            }
        }
    }
}

/// "selector" contains the actual selector such as "nth-child" while "value" contains
/// an optional value - for example "nth-child(3)" would be: selector: "nth-child", value: "3".
/// # Errors
///
/// Returns an error if `selector` (with optional `value`) is not a recognized CSS pseudo-selector.
pub fn pseudo_selector_from_str<'a>(
    selector: &'a str,
    value: Option<&'a str>,
) -> Result<CssPathPseudoSelector, CssPseudoSelectorParseError<'a>> {
    match selector {
        "first" => Ok(CssPathPseudoSelector::First),
        "last" => Ok(CssPathPseudoSelector::Last),
        "hover" => Ok(CssPathPseudoSelector::Hover),
        "active" => Ok(CssPathPseudoSelector::Active),
        "focus" => Ok(CssPathPseudoSelector::Focus),
        "dragging" => Ok(CssPathPseudoSelector::Dragging),
        "drag-over" => Ok(CssPathPseudoSelector::DragOver),
        "nth-child" => {
            let value = value.ok_or(CssPseudoSelectorParseError::EmptyNthChild)?;
            let parsed = parse_nth_child_selector(value)?;
            Ok(CssPathPseudoSelector::NthChild(parsed))
        }
        "lang" => {
            let lang_value = value.ok_or(CssPseudoSelectorParseError::UnknownSelector(
                selector, value,
            ))?;
            // Remove quotes if present
            let lang_value = lang_value
                .trim()
                .trim_start_matches('"')
                .trim_end_matches('"')
                .trim_start_matches('\'')
                .trim_end_matches('\'')
                .trim();
            Ok(CssPathPseudoSelector::Lang(AzString::from(
                lang_value.to_string(),
            )))
        }
        _ => Err(CssPseudoSelectorParseError::UnknownSelector(
            selector, value,
        )),
    }
}

/// Parses the inner content of an attribute selector token (the text between `[` and `]`).
///
/// Returns `None` if the input is malformed (empty name, unterminated quote, etc).
#[must_use] pub fn parse_attribute_selector(input: &str) -> Option<CssAttributeSelector> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }

    // Find the operator (the longest match wins): try the compound operators
    // first (in order), then the bare `=`, otherwise it is an existence check.
    let compound_ops: [(&str, AttributeMatchOp); 5] = [
        ("~=", AttributeMatchOp::Includes),
        ("|=", AttributeMatchOp::DashMatch),
        ("^=", AttributeMatchOp::Prefix),
        ("$=", AttributeMatchOp::Suffix),
        ("*=", AttributeMatchOp::Substring),
    ];
    let (op, op_pos): (AttributeMatchOp, Option<usize>) = compound_ops
        .iter()
        .find_map(|(pat, op)| s.find(pat).map(|i| (*op, Some(i))))
        .or_else(|| s.find('=').map(|i| (AttributeMatchOp::Eq, Some(i))))
        .unwrap_or((AttributeMatchOp::Exists, None));

    let (name, value) = match op_pos {
        None => (s, None),
        Some(i) => {
            let name = s[..i].trim();
            let op_len = if matches!(op, AttributeMatchOp::Eq) { 1 } else { 2 };
            let raw_value = s[i + op_len..].trim();
            let unquoted = strip_attribute_quotes(raw_value)?;
            (name, Some(unquoted))
        }
    };

    if name.is_empty() {
        return None;
    }
    // Reject names that contain whitespace or quotes.
    if name.chars().any(|c| c.is_whitespace() || c == '"' || c == '\'') {
        return None;
    }

    Some(CssAttributeSelector {
        name: name.to_string().into(),
        op,
        value: value
            .map_or_else(|| OptionString::None, |v| OptionString::Some(v.to_string().into())),
    })
}

/// Strips matching surrounding `"` or `'` from a value. If the value is unquoted,
/// returns it unchanged. Returns `None` if quoting is unbalanced.
fn strip_attribute_quotes(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    if bytes.len() >= 2 {
        let first = bytes[0];
        let last = bytes[bytes.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return Some(&s[1..s.len() - 1]);
        }
        if first == b'"' || first == b'\'' || last == b'"' || last == b'\'' {
            // Unbalanced quote.
            return None;
        }
    } else if bytes.len() == 1 && (bytes[0] == b'"' || bytes[0] == b'\'') {
        return None;
    }
    Some(s)
}

/// Parses the inner value of the `:nth-child` selector, including numbers and patterns.
///
/// I.e.: `"2n+3"` -> `Pattern { repeat: 2, offset: 3 }`
fn parse_nth_child_selector(
    value: &str,
) -> Result<CssNthChildSelector, CssPseudoSelectorParseError<'_>> {
    let value = value.trim();

    if value.is_empty() {
        return Err(CssPseudoSelectorParseError::EmptyNthChild);
    }

    if let Ok(number) = value.parse::<u32>() {
        return Ok(CssNthChildSelector::Number(number));
    }

    // If the value is not a number
    match value {
        "even" => Ok(CssNthChildSelector::Even),
        "odd" => Ok(CssNthChildSelector::Odd),
        _ => parse_nth_child_pattern(value),
    }
}

/// Parses the pattern between the braces of a "nth-child" (such as "2n+3").
fn parse_nth_child_pattern(
    value: &str,
) -> Result<CssNthChildSelector, CssPseudoSelectorParseError<'_>> {
    use crate::css::CssNthChildPattern;

    let value = value.trim();

    if value.is_empty() {
        return Err(CssPseudoSelectorParseError::EmptyNthChild);
    }

    // TODO: Test for "+"
    let repeat = value
        .split('n')
        .next()
        .ok_or(CssPseudoSelectorParseError::InvalidNthChildPattern(value))?
        .trim()
        .parse::<u32>()?;

    // In a "2n+3" form, the first .next() yields the "2n", the second .next() yields the "3"
    let mut offset_iterator = value.split('+');

    // has to succeed, since the string is verified to not be empty
    offset_iterator.next().unwrap();

    let offset = match offset_iterator.next() {
        Some(offset_string) => {
            let offset_string = offset_string.trim();
            if offset_string.is_empty() {
                return Err(CssPseudoSelectorParseError::InvalidNthChildPattern(value));
            }
            offset_string.parse::<u32>()?
        }
        None => 0,
    };

    Ok(CssNthChildSelector::Pattern(CssNthChildPattern {
        pattern_repeat: repeat,
        offset,
    }))
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct ErrorLocation {
    pub original_pos: usize,
}

/// FFI-safe replacement for `(ErrorLocation, ErrorLocation)` tuple.
/// Represents a range (start..end) in the source text.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct ErrorLocationRange {
    pub start: ErrorLocation,
    pub end: ErrorLocation,
}

impl ErrorLocation {
    /// Given an error location, returns the (line, column)
    #[must_use] pub fn get_line_column_from_error(&self, css_string: &str) -> (usize, usize) {
        // `original_pos` is a pub field and, at Token::EndOfStream, `get_error_location`
        // records it as exactly `css_string.len()` -- so `- 1` lands INSIDE the final
        // character whenever the stylesheet ends in a multi-byte char, and an
        // out-of-range value is trivially constructible. Both used to panic here, i.e.
        // simply Display-ing a parse error on Unicode CSS would abort.
        let error_location =
            clamp_to_char_boundary(css_string, self.original_pos.saturating_sub(1));
        let (mut line_number, mut total_characters) = (0, 0);

        for line in css_string[0..error_location].lines() {
            line_number += 1;
            total_characters += line.chars().count();
        }

        // Rust doesn't count "\n" as a character, so we have to add the line number count on top
        let total_characters = total_characters + line_number;
        let column_pos = error_location - total_characters.saturating_sub(2);

        (line_number, column_pos)
    }
}

impl fmt::Display for CssParseError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let start_location = self.location.start.get_line_column_from_error(self.css_string);
        let end_location = self.location.end.get_line_column_from_error(self.css_string);
        write!(
            f,
            "    start: line {}:{}\r\n    end: line {}:{}\r\n    text: \"{}\"\r\n    reason: {}",
            start_location.0,
            start_location.1,
            end_location.0,
            end_location.1,
            self.get_error_string(),
            self.error,
        )
    }
}

/// Parses a CSS string into a [`Css`] value and a list of recoverable warnings.
///
/// Never panics. Syntax errors and unsupported properties are collected as
/// [`CssParseWarnMsg`] items rather than causing a hard failure, so the caller
/// always receives a (possibly empty) stylesheet.
#[must_use] pub fn new_from_str(css_string: &str) -> (Css, Vec<CssParseWarnMsg<'_>>) {
    let mut tokenizer = Tokenizer::new(css_string);
    let (rules, warnings) = new_from_str_inner(css_string, &mut tokenizer);

    (
        Css { rules: rules.into() },
        warnings,
    )
}

/// Returns the location of where the parser is currently in the document
fn get_error_location(tokenizer: &Tokenizer<'_>) -> ErrorLocation {
    ErrorLocation {
        original_pos: tokenizer.pos(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CssPathParseError<'a> {
    EmptyPath,
    /// Invalid item encountered in string (for example a "{", "}")
    InvalidTokenEncountered(&'a str),
    UnexpectedEndOfStream(&'a str),
    SyntaxError(CssSyntaxError),
    /// The path has to be either `*`, `div`, `p` or something like that
    NodeTypeTag(NodeTypeTagParseError<'a>),
    /// Error while parsing a pseudo selector (like `:aldkfja`)
    PseudoSelectorParseError(CssPseudoSelectorParseError<'a>),
}

impl_from! { NodeTypeTagParseError<'a>, CssPathParseError::NodeTypeTag }
impl_from! { CssPseudoSelectorParseError<'a>, CssPathParseError::PseudoSelectorParseError }

impl From<CssSyntaxError> for CssPathParseError<'_> {
    fn from(e: CssSyntaxError) -> Self {
        CssPathParseError::SyntaxError(e)
    }
}

impl From<SimplecssError> for CssPathParseError<'_> {
    fn from(e: SimplecssError) -> Self {
        CssPathParseError::SyntaxError(CssSyntaxError::from(e))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CssPathParseErrorOwned {
    EmptyPath,
    InvalidTokenEncountered(AzString),
    UnexpectedEndOfStream(AzString),
    SyntaxError(CssSyntaxError),
    NodeTypeTag(NodeTypeTagParseErrorOwned),
    PseudoSelectorParseError(CssPseudoSelectorParseErrorOwned),
}

impl CssPathParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> CssPathParseErrorOwned {
        match self {
            CssPathParseError::EmptyPath => CssPathParseErrorOwned::EmptyPath,
            CssPathParseError::InvalidTokenEncountered(s) => {
                CssPathParseErrorOwned::InvalidTokenEncountered((*s).to_string().into())
            }
            CssPathParseError::UnexpectedEndOfStream(s) => {
                CssPathParseErrorOwned::UnexpectedEndOfStream((*s).to_string().into())
            }
            CssPathParseError::SyntaxError(e) => CssPathParseErrorOwned::SyntaxError(*e),
            CssPathParseError::NodeTypeTag(e) => {
                CssPathParseErrorOwned::NodeTypeTag(e.to_contained())
            }
            CssPathParseError::PseudoSelectorParseError(e) => {
                CssPathParseErrorOwned::PseudoSelectorParseError(e.to_contained())
            }
        }
    }
}

impl CssPathParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> CssPathParseError<'_> {
        match self {
            Self::EmptyPath => CssPathParseError::EmptyPath,
            Self::InvalidTokenEncountered(s) => {
                CssPathParseError::InvalidTokenEncountered(s)
            }
            Self::UnexpectedEndOfStream(s) => {
                CssPathParseError::UnexpectedEndOfStream(s)
            }
            Self::SyntaxError(e) => CssPathParseError::SyntaxError(*e),
            Self::NodeTypeTag(e) => CssPathParseError::NodeTypeTag(e.to_shared()),
            Self::PseudoSelectorParseError(e) => {
                CssPathParseError::PseudoSelectorParseError(e.to_shared())
            }
        }
    }
}

/// Parses a CSS path from a string (only the path,.no commas allowed)
///
/// ```rust
/// # extern crate azul_css;
/// # use azul_css::parser2::parse_css_path;
/// # use azul_css::css::{
/// #     CssPathSelector::*, CssPathPseudoSelector::*, CssPath,
/// #     NodeTypeTag::*, CssNthChildSelector::*
/// # };
///
/// assert_eq!(
///     parse_css_path("* div #my_id > .class:nth-child(2)"),
///     Ok(CssPath {
///         selectors: vec![
///             Global,
///             Type(Div),
///             Children,
///             Id("my_id".to_string().into()),
///             DirectChildren,
///             Class("class".to_string().into()),
///             PseudoSelector(NthChild(Number(2))),
///         ]
///         .into()
///     })
/// );
/// ```
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `css-path` value.
pub fn parse_css_path(input: &str) -> Result<CssPath, CssPathParseError<'_>> {
    use azul_simplecss::{Combinator, Token};

    let input = input.trim();
    if input.is_empty() {
        return Err(CssPathParseError::EmptyPath);
    }

    let mut tokenizer = Tokenizer::new(input);
    let mut selectors = Vec::new();

    loop {
        let token = tokenizer.parse_next()?;
        match token {
            Token::UniversalSelector => {
                selectors.push(CssPathSelector::Global);
            }
            Token::TypeSelector(div_type) => match NodeTypeTag::from_str(div_type) {
                // An unknown type selector must invalidate the whole path (Selectors L4:
                // an invalid simple selector invalidates the selector), not be silently
                // dropped — dropping it left a dangling combinator that matched every
                // descendant of the previous selector.
                Ok(nt) => selectors.push(CssPathSelector::Type(nt)),
                Err(e) => return Err(CssPathParseError::NodeTypeTag(e)),
            },
            Token::IdSelector(id) => {
                selectors.push(CssPathSelector::Id(id.to_string().into()));
            }
            Token::ClassSelector(class) => {
                selectors.push(CssPathSelector::Class(class.to_string().into()));
            }
            Token::Combinator(Combinator::GreaterThan) => {
                selectors.push(CssPathSelector::DirectChildren);
            }
            Token::Combinator(Combinator::Space) => {
                selectors.push(CssPathSelector::Children);
            }
            Token::Combinator(Combinator::Plus) => {
                selectors.push(CssPathSelector::AdjacentSibling);
            }
            Token::Combinator(Combinator::Tilde) => {
                selectors.push(CssPathSelector::GeneralSibling);
            }
            Token::PseudoClass { selector, value } => {
                selectors.push(CssPathSelector::PseudoSelector(pseudo_selector_from_str(
                    selector, value,
                )?));
            }
            Token::EndOfStream => {
                break;
            }
            _ => {
                return Err(CssPathParseError::InvalidTokenEncountered(input));
            }
        }
    }

    if selectors.is_empty() {
        Err(CssPathParseError::EmptyPath)
    } else {
        Ok(CssPath {
            selectors: selectors.into(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnparsedCssRuleBlock<'a> {
    /// The css path (full selector) of the style ruleset
    pub path: CssPath,
    /// `"justify-content" => "center"`
    pub declarations: BTreeMap<&'a str, (&'a str, ErrorLocationRange)>,
    /// Conditions from enclosing @-rules (@media, @lang, etc.)
    pub conditions: Vec<DynamicSelector>,
}

/// Owned version of `UnparsedCssRuleBlock`, with `BTreeMap` of Strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnparsedCssRuleBlockOwned {
    pub path: CssPath,
    pub declarations: BTreeMap<String, (String, ErrorLocationRange)>,
    pub conditions: Vec<DynamicSelector>,
}

impl UnparsedCssRuleBlock<'_> {
    #[must_use] pub fn to_contained(&self) -> UnparsedCssRuleBlockOwned {
        UnparsedCssRuleBlockOwned {
            path: self.path.clone(),
            declarations: self
                .declarations
                .iter()
                .map(|(k, (v, loc))| ((*k).to_string(), ((*v).to_string(), *loc)))
                .collect(),
            conditions: self.conditions.clone(),
        }
    }
}

impl UnparsedCssRuleBlockOwned {
    #[must_use] pub fn to_shared(&self) -> UnparsedCssRuleBlock<'_> {
        UnparsedCssRuleBlock {
            path: self.path.clone(),
            declarations: self
                .declarations
                .iter()
                .map(|(k, (v, loc))| (k.as_str(), (v.as_str(), *loc)))
                .collect(),
            conditions: self.conditions.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CssParseWarnMsg<'a> {
    pub warning: CssParseWarnMsgInner<'a>,
    pub location: ErrorLocationRange,
}

/// Owned version of `CssParseWarnMsg`, where warning is the owned type.
#[derive(Debug, Clone, PartialEq)]
pub struct CssParseWarnMsgOwned {
    pub warning: CssParseWarnMsgInnerOwned,
    pub location: ErrorLocationRange,
}

impl CssParseWarnMsg<'_> {
    #[must_use] pub fn to_contained(&self) -> CssParseWarnMsgOwned {
        CssParseWarnMsgOwned {
            warning: self.warning.to_contained(),
            location: self.location,
        }
    }
}

impl CssParseWarnMsgOwned {
    #[must_use] pub fn to_shared(&self) -> CssParseWarnMsg<'_> {
        CssParseWarnMsg {
            warning: self.warning.to_shared(),
            location: self.location,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CssParseWarnMsgInner<'a> {
    /// Key "blah" isn't (yet) supported, so the parser didn't attempt to parse the value at all
    UnsupportedKeyValuePair { key: &'a str, value: &'a str },
    /// A CSS parse error that was encountered but recovered from
    ParseError(CssParseErrorInner<'a>),
    /// A rule was skipped due to an error
    SkippedRule {
        selector: Option<&'a str>,
        error: CssParseErrorInner<'a>,
    },
    /// A declaration was skipped due to an error
    SkippedDeclaration {
        key: &'a str,
        value: &'a str,
        error: CssParseErrorInner<'a>,
    },
    /// Malformed block structure (mismatched braces, etc.)
    MalformedStructure { message: &'a str },
}

#[derive(Debug, Clone, PartialEq)]
pub enum CssParseWarnMsgInnerOwned {
    UnsupportedKeyValuePair {
        key: String,
        value: String,
    },
    ParseError(CssParseErrorInnerOwned),
    SkippedRule {
        selector: Option<String>,
        error: CssParseErrorInnerOwned,
    },
    SkippedDeclaration {
        key: String,
        value: String,
        error: CssParseErrorInnerOwned,
    },
    MalformedStructure {
        message: String,
    },
}

impl CssParseWarnMsgInner<'_> {
    #[must_use] pub fn to_contained(&self) -> CssParseWarnMsgInnerOwned {
        match self {
            Self::UnsupportedKeyValuePair { key, value } => {
                CssParseWarnMsgInnerOwned::UnsupportedKeyValuePair {
                    key: (*key).to_string(),
                    value: (*value).to_string(),
                }
            }
            Self::ParseError(e) => CssParseWarnMsgInnerOwned::ParseError(e.to_contained()),
            Self::SkippedRule { selector, error } => CssParseWarnMsgInnerOwned::SkippedRule {
                selector: selector.map(std::string::ToString::to_string),
                error: error.to_contained(),
            },
            Self::SkippedDeclaration { key, value, error } => {
                CssParseWarnMsgInnerOwned::SkippedDeclaration {
                    key: (*key).to_string(),
                    value: (*value).to_string(),
                    error: error.to_contained(),
                }
            }
            Self::MalformedStructure { message } => CssParseWarnMsgInnerOwned::MalformedStructure {
                message: (*message).to_string(),
            },
        }
    }
}

impl CssParseWarnMsgInnerOwned {
    #[must_use] pub fn to_shared(&self) -> CssParseWarnMsgInner<'_> {
        match self {
            Self::UnsupportedKeyValuePair { key, value } => {
                CssParseWarnMsgInner::UnsupportedKeyValuePair { key, value }
            }
            Self::ParseError(e) => CssParseWarnMsgInner::ParseError(e.to_shared()),
            Self::SkippedRule { selector, error } => CssParseWarnMsgInner::SkippedRule {
                selector: selector.as_deref(),
                error: error.to_shared(),
            },
            Self::SkippedDeclaration { key, value, error } => {
                CssParseWarnMsgInner::SkippedDeclaration {
                    key,
                    value,
                    error: error.to_shared(),
                }
            }
            Self::MalformedStructure { message } => {
                CssParseWarnMsgInner::MalformedStructure { message }
            }
        }
    }
}

impl_display! { CssParseWarnMsgInner<'a>, {
    UnsupportedKeyValuePair { key, value } => format!("Unsupported CSS property: \"{}: {}\"", key, value),
    ParseError(e) => format!("Parse error (recoverable): {}", e),
    SkippedRule { selector, error } => {
        let sel = selector.unwrap_or("unknown");
        format!("Skipped rule for selector '{sel}': {error}")
    },
    SkippedDeclaration { key, value, error } => format!("Skipped declaration '{}:{}': {}", key, value, error),
    MalformedStructure { message } => format!("Malformed CSS structure: {}", message),
}}

/// Parses @media conditions from the content following "@media"
/// Returns a list of `DynamicSelectors` for the conditions
fn parse_media_conditions(content: &str) -> Vec<DynamicSelector> {
    let mut conditions = Vec::new();
    let content = content.trim();

    // Handle simple media types: "screen", "print", "all"
    if content.eq_ignore_ascii_case("screen") {
        conditions.push(DynamicSelector::Media(MediaType::Screen));
        return conditions;
    }
    if content.eq_ignore_ascii_case("print") {
        conditions.push(DynamicSelector::Media(MediaType::Print));
        return conditions;
    }
    if content.eq_ignore_ascii_case("all") {
        conditions.push(DynamicSelector::Media(MediaType::All));
        return conditions;
    }

    // Parse more complex media queries like "(min-width: 800px)" or "screen and (max-width: 600px)"
    // Split by "and" for compound queries
    for part in content.split(" and ") {
        let part = part.trim();

        // Skip media type keywords in compound queries
        if part.eq_ignore_ascii_case("screen")
            || part.eq_ignore_ascii_case("print")
            || part.eq_ignore_ascii_case("all")
        {
            if part.eq_ignore_ascii_case("screen") {
                conditions.push(DynamicSelector::Media(MediaType::Screen));
            } else if part.eq_ignore_ascii_case("print") {
                conditions.push(DynamicSelector::Media(MediaType::Print));
            } else if part.eq_ignore_ascii_case("all") {
                conditions.push(DynamicSelector::Media(MediaType::All));
            }
            continue;
        }

        // Parse parenthesized conditions like "(min-width: 800px)"
        if let Some(inner) = part.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
            if let Some(selector) = parse_media_feature(inner) {
                conditions.push(selector);
            }
        }
    }

    conditions
}

/// Parses a single media feature like "min-width: 800px"
fn parse_media_feature(feature: &str) -> Option<DynamicSelector> {
    let parts: Vec<&str> = feature.splitn(2, ':').collect();
    if parts.len() != 2 {
        // Handle features without values like "orientation: portrait"
        return None;
    }

    let key = parts[0].trim();
    let value = parts[1].trim();

    match key.to_lowercase().as_str() {
        "min-width" => {
            if let Some(px) = parse_px_value(value) {
                return Some(DynamicSelector::ViewportWidth(MinMaxRange::new(
                    Some(px),
                    None,
                )));
            }
        }
        "max-width" => {
            if let Some(px) = parse_px_value(value) {
                return Some(DynamicSelector::ViewportWidth(MinMaxRange::new(
                    None,
                    Some(px),
                )));
            }
        }
        "min-height" => {
            if let Some(px) = parse_px_value(value) {
                return Some(DynamicSelector::ViewportHeight(MinMaxRange::new(
                    Some(px),
                    None,
                )));
            }
        }
        "max-height" => {
            if let Some(px) = parse_px_value(value) {
                return Some(DynamicSelector::ViewportHeight(MinMaxRange::new(
                    None,
                    Some(px),
                )));
            }
        }
        "orientation" => {
            if value.eq_ignore_ascii_case("portrait") {
                return Some(DynamicSelector::Orientation(OrientationType::Portrait));
            } else if value.eq_ignore_ascii_case("landscape") {
                return Some(DynamicSelector::Orientation(OrientationType::Landscape));
            }
        }
        "prefers-color-scheme" => {
            if value.eq_ignore_ascii_case("dark") {
                return Some(DynamicSelector::Theme(ThemeCondition::Dark));
            } else if value.eq_ignore_ascii_case("light") {
                return Some(DynamicSelector::Theme(ThemeCondition::Light));
            }
        }
        "prefers-reduced-motion" => {
            if value.eq_ignore_ascii_case("reduce") {
                return Some(DynamicSelector::PrefersReducedMotion(BoolCondition::True));
            } else if value.eq_ignore_ascii_case("no-preference") {
                return Some(DynamicSelector::PrefersReducedMotion(BoolCondition::False));
            }
        }
        "prefers-contrast" | "prefers-high-contrast" => {
            if value.eq_ignore_ascii_case("more") || value.eq_ignore_ascii_case("high") || value.eq_ignore_ascii_case("active") {
                return Some(DynamicSelector::PrefersHighContrast(BoolCondition::True));
            } else if value.eq_ignore_ascii_case("no-preference") || value.eq_ignore_ascii_case("none") {
                return Some(DynamicSelector::PrefersHighContrast(BoolCondition::False));
            }
        }
        "aspect-ratio" => {
            if let Some(ratio) = parse_ratio_value(value) {
                return Some(DynamicSelector::AspectRatio(MinMaxRange::new(Some(ratio), Some(ratio))));
            }
        }
        "min-aspect-ratio" => {
            if let Some(ratio) = parse_ratio_value(value) {
                return Some(DynamicSelector::AspectRatio(MinMaxRange::new(Some(ratio), None)));
            }
        }
        "max-aspect-ratio" => {
            if let Some(ratio) = parse_ratio_value(value) {
                return Some(DynamicSelector::AspectRatio(MinMaxRange::new(None, Some(ratio))));
            }
        }
        _ => {}
    }

    None
}

/// Parses a pixel value like "800px" and returns the numeric value
fn parse_px_value(value: &str) -> Option<f32> {
    let value = value.trim();
    value
        .strip_suffix("px")
        .map_or_else(
            // Try parsing as a bare number
            || value.parse::<f32>().ok(),
            |num_str| num_str.trim().parse::<f32>().ok(),
        )
        // `str::parse::<f32>` accepts "NaN"/"inf"/"infinity"; the CSS <number-token>
        // grammar does not (CSS Syntax L3 §4.3.6 — digits, no keywords). Letting a NaN
        // through is not merely lax: `MinMaxRange` encodes "no bound" AS NaN, so
        // `@media (min-width: NaN)` would silently become an unconditional match
        // instead of an invalid feature. Reject non-finite at the source.
        .filter(|v| v.is_finite())
}

/// Parses a ratio value like "16/9" or "1.777" and returns it as f32
fn parse_ratio_value(value: &str) -> Option<f32> {
    let value = value.trim();
    if let Some((num, den)) = value.split_once('/') {
        let num: f32 = num.trim().parse().ok()?;
        let den: f32 = den.trim().parse().ok()?;
        if den == 0.0 { return None; }
        // Same NaN-sentinel hazard as parse_px_value: "inf/inf" and "1/NaN" both parse,
        // and a NaN ratio reads back out of MinMaxRange as "no bound".
        Some(num / den).filter(|r| r.is_finite())
    } else {
        value.parse::<f32>().ok().filter(|r| r.is_finite())
    }
}

/// Parses @container conditions from the content following "@container"
/// Format: @container (min-width: 400px) or @container sidebar (min-width: 400px)
fn parse_container_conditions(content: &str) -> Vec<DynamicSelector> {
    let mut conditions = Vec::new();
    let content = content.trim();

    // Check if there's a container name before the parenthesized condition
    // e.g., "sidebar (min-width: 400px)" or just "(min-width: 400px)"
    let (name_part, query_part) = if content.starts_with('(') {
        (None, content)
    } else if let Some(paren_idx) = content.find('(') {
        let name = content[..paren_idx].trim();
        if name.is_empty() {
            (None, content)
        } else {
            (Some(name), &content[paren_idx..])
        }
    } else {
        // No parentheses - might be just a container name
        if !content.is_empty() {
            conditions.push(DynamicSelector::ContainerName(AzString::from(content.to_string())));
        }
        return conditions;
    };

    if let Some(name) = name_part {
        conditions.push(DynamicSelector::ContainerName(AzString::from(name.to_string())));
    }

    // Parse the parenthesized query parts
    for part in query_part.split(" and ") {
        let part = part.trim();
        if let Some(inner) = part.strip_prefix('(').and_then(|s| s.strip_suffix(')')) {
            if let Some(selector) = parse_container_feature(inner) {
                conditions.push(selector);
            }
        }
    }

    conditions
}

/// Parses a single container query feature like "min-width: 400px"
fn parse_container_feature(feature: &str) -> Option<DynamicSelector> {
    let (key, value) = feature.split_once(':')?;
    let key = key.trim();
    let value = value.trim();

    match key.to_lowercase().as_str() {
        "min-width" => {
            parse_px_value(value).map(|px| DynamicSelector::ContainerWidth(MinMaxRange::new(Some(px), None)))
        }
        "max-width" => {
            parse_px_value(value).map(|px| DynamicSelector::ContainerWidth(MinMaxRange::new(None, Some(px))))
        }
        "min-height" => {
            parse_px_value(value).map(|px| DynamicSelector::ContainerHeight(MinMaxRange::new(Some(px), None)))
        }
        "max-height" => {
            parse_px_value(value).map(|px| DynamicSelector::ContainerHeight(MinMaxRange::new(None, Some(px))))
        }
        "aspect-ratio" => {
            parse_ratio_value(value).map(|r| DynamicSelector::AspectRatio(MinMaxRange::new(Some(r), Some(r))))
        }
        "min-aspect-ratio" => {
            parse_ratio_value(value).map(|r| DynamicSelector::AspectRatio(MinMaxRange::new(Some(r), None)))
        }
        "max-aspect-ratio" => {
            parse_ratio_value(value).map(|r| DynamicSelector::AspectRatio(MinMaxRange::new(None, Some(r))))
        }
        _ => None,
    }
}

/// Parses @theme condition from the content following "@theme"
/// Format: @theme(dark) or @theme dark
fn parse_theme_condition(content: &str) -> Option<DynamicSelector> {
    let content = content.trim();
    let inner = content
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(content)
        .trim();
    let inner = inner
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| inner.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
        .unwrap_or(inner)
        .trim();

    match inner.to_lowercase().as_str() {
        "dark" => Some(DynamicSelector::Theme(ThemeCondition::Dark)),
        "light" => Some(DynamicSelector::Theme(ThemeCondition::Light)),
        _ => None,
    }
}

/// Parses @lang condition from the content following "@lang"
/// Format: @lang("de-DE") or @lang(de-DE)
fn parse_lang_condition(content: &str) -> Option<DynamicSelector> {
    let content = content.trim();

    // Remove parentheses and quotes
    let lang = content
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(content)
        .trim();

    let lang = lang
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| lang.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
        .unwrap_or(lang)
        .trim();

    if lang.is_empty() {
        return None;
    }

    // Use Prefix matching by default (e.g., "de" matches "de-DE", "de-AT")
    Some(DynamicSelector::Language(LanguageCondition::Prefix(
        AzString::from(lang.to_string()),
    )))
}

/// Parses a CSS string (single-threaded) and returns the parsed rules in blocks
///
/// May return "warning" messages, i.e. messages that just serve as a warning,
/// instead of being actual errors. These warnings may be ignored by the caller,
/// but can be useful for debugging.
// Beyond this CSS nesting depth, get_parent_paths clones the ever-growing
// ancestor path every level (parse becomes O(depth^2) — a hang on adversarial
// input like `div{` x 10_000), so deeper rules keep only their own local
// selector. No realistic stylesheet nests this deep; this bounds parse time.
const MAX_NESTING_DEPTH: usize = 1024;

#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose CSS parser/formatter/dispatch table (one branch per property/variant)
fn new_from_str_inner<'a>(
    css_string: &'a str,
    tokenizer: &mut Tokenizer<'a>,
) -> (Vec<CssRuleBlock>, Vec<CssParseWarnMsg<'a>>) {
    use azul_simplecss::{Combinator, Token};

    // Stack entry for nested selectors: accumulated parent paths + the current
    // declarations at this nesting level.
    struct NestingLevel<'a> {
        paths: Vec<Vec<CssPathSelector>>,
        declarations: BTreeMap<&'a str, (&'a str, ErrorLocationRange)>,
        depth: usize,
    }

    // Helper: get parent paths from nesting stack (if any)
    fn get_parent_paths(nesting_stack: &[NestingLevel<'_>]) -> Vec<Vec<CssPathSelector>> {
        nesting_stack
            .last()
            .map_or_else(Vec::new, |parent| parent.paths.clone())
    }

    // Helper: combine parent path with child selector for nesting
    // For .button { :hover { } } -> .button:hover
    // For .outer { .inner { } } -> .outer .inner (with Children combinator)
    fn combine_paths(
        parent_paths: &[Vec<CssPathSelector>],
        child_path: &[CssPathSelector],
        is_pseudo_only: bool,
    ) -> Vec<Vec<CssPathSelector>> {
        if parent_paths.is_empty() {
            vec![child_path.to_vec()]
        } else {
            parent_paths
                .iter()
                .map(|parent| {
                    let mut combined = parent.clone();
                    if !is_pseudo_only && !child_path.is_empty() {
                        // Add implicit descendant combinator for non-pseudo selectors
                        combined.push(CssPathSelector::Children);
                    }
                    combined.extend(child_path.iter().cloned());
                    combined
                })
                .collect()
        }
    }

    let mut css_blocks = Vec::new();
    let mut warnings = Vec::new();

    let mut block_nesting = 0_usize;
    let mut last_path: Vec<CssPathSelector> = Vec::new();
    let mut last_error_location = ErrorLocation { original_pos: 0 };

    // Stack for tracking @-rule conditions (e.g., @media, @lang, @os)
    // Each entry contains the conditions and the nesting level where they were introduced
    let mut at_rule_stack: Vec<(Vec<DynamicSelector>, usize)> = Vec::new();
    // Pending @-rule that needs to be combined with AtStr tokens
    let mut pending_at_rule: Option<&str> = None;
    // Collect multiple AtStr tokens (e.g., "screen", "(min-width: 800px)" for compound media queries)
    let mut pending_at_str_parts: Vec<String> = Vec::new();

    // Stack for nested selectors
    // Each entry: (parent_paths, declarations, nesting_level)
    // parent_paths: all accumulated paths at this level (for comma-separated selectors)
    // declarations: current declarations at this level
    let mut nesting_stack: Vec<NestingLevel<'a>> = Vec::new();
    // Current accumulated paths before BlockStart
    let mut current_paths: Vec<Vec<CssPathSelector>> = Vec::new();
    // Current declarations at current level
    let mut current_declarations: BTreeMap<&str, (&str, ErrorLocationRange)> = BTreeMap::new();

    // Safety: limit maximum iterations to prevent infinite loops
    // A reasonable limit is 10x the input length (each char could produce at most a few tokens)
    let max_iterations = css_string.len().saturating_mul(10).max(1000);
    let mut iterations = 0_usize;
    let mut last_position = 0_usize;
    let mut stuck_count = 0_usize;

    loop {
        // Safety check 1: Maximum iterations
        iterations += 1;
        if iterations > max_iterations {
            warnings.push(CssParseWarnMsg {
                warning: CssParseWarnMsgInner::MalformedStructure {
                    message: "Parser iteration limit exceeded - possible infinite loop",
                },
                location: ErrorLocationRange { start: last_error_location, end: get_error_location(tokenizer) },
            });
            break;
        }

        // Safety check 2: Detect if parser is stuck (position not advancing)
        let current_position = tokenizer.pos();
        if current_position == last_position {
            stuck_count += 1;
            if stuck_count > 10 {
                warnings.push(CssParseWarnMsg {
                    warning: CssParseWarnMsgInner::MalformedStructure {
                        message: "Parser stuck - position not advancing",
                    },
                    location: ErrorLocationRange { start: last_error_location, end: get_error_location(tokenizer) },
                });
                break;
            }
        } else {
            stuck_count = 0;
            last_position = current_position;
        }

        let token = match tokenizer.parse_next() {
            Ok(token) => token,
            Err(e) => {
                let error_location = get_error_location(tokenizer);
                // An unclosed block that still contains a declaration makes the
                // tokenizer raise UnexpectedEndOfStream while scanning past the last `;`
                // for the missing `}`, BEFORE the loop ever reaches Token::EndOfStream.
                // Emit the same dedicated "unclosed blocks" diagnostic that arm would,
                // rather than a generic parse error, when we're still inside a block.
                let warning = if block_nesting != 0 {
                    CssParseWarnMsgInner::MalformedStructure {
                        message: "Unclosed blocks at end of file",
                    }
                } else {
                    CssParseWarnMsgInner::ParseError(e.into())
                };
                warnings.push(CssParseWarnMsg {
                    warning,
                    location: ErrorLocationRange { start: last_error_location, end: error_location },
                });
                // On error, break to avoid infinite loop - the tokenizer may be stuck
                break;
            }
        };

        macro_rules! warn_and_continue {
            ($warning:expr) => {{
                warnings.push(CssParseWarnMsg {
                    warning: $warning,
                    location: ErrorLocationRange { start: last_error_location, end: get_error_location(tokenizer) },
                });
                continue;
            }};
        }

        match token {
            Token::AtRule(rule_name) => {
                // Store the @-rule name to combine with the following AtStr tokens
                pending_at_rule = Some(rule_name);
                pending_at_str_parts.clear();
            }
            Token::AtStr(content) => {
                // Collect AtStr tokens until we see BlockStart
                if pending_at_rule.is_some() {
                    // Skip "and" keyword, it's just a separator
                    if !content.eq_ignore_ascii_case("and") {
                        pending_at_str_parts.push(content.to_string());
                    }
                }
            }
            Token::BlockStart => {
                // Process pending @-rule with all collected AtStr parts
                if let Some(rule_name) = pending_at_rule.take() {
                    let combined_content = pending_at_str_parts.join(" and ");
                    pending_at_str_parts.clear();
                    
                    let conditions = match rule_name.to_lowercase().as_str() {
                        "media" => parse_media_conditions(&combined_content),
                        "lang" => parse_lang_condition(&combined_content).into_iter().collect(),
                        "os" => crate::dynamic_selector::parse_os_at_rule_content(&combined_content).unwrap_or_default(),
                        "theme" => parse_theme_condition(&combined_content).into_iter().collect(),
                        "container" => parse_container_conditions(&combined_content),
                        _ => {
                            // Unknown @-rule, ignore
                            Vec::new()
                        }
                    };

                    if !conditions.is_empty() {
                        // Push conditions to stack, will be applied to nested rules
                        at_rule_stack.push((conditions, block_nesting + 1));
                    }
                }

                block_nesting += 1;

                // If we have a selector, push current state onto nesting stack
                if !current_paths.is_empty() || !last_path.is_empty() {
                    // Finalize current_paths with last_path
                    if !last_path.is_empty() {
                        current_paths.push(last_path.clone());
                        last_path.clear();
                    }

                    // Get parent paths and combine with current paths. Beyond
                    // MAX_NESTING_DEPTH, stop combining with the ancestor chain to
                    // bound the O(depth^2) path-cloning (see the const's doc above).
                    let combined_paths: Vec<Vec<CssPathSelector>> = if block_nesting > MAX_NESTING_DEPTH {
                        std::mem::take(&mut current_paths)
                    } else {
                        let parent_paths = get_parent_paths(&nesting_stack);
                        if parent_paths.is_empty() {
                            current_paths.clone()
                        } else {
                            // Combine each parent path with each current path
                            let mut result = Vec::new();
                            for parent in &parent_paths {
                                for child in &current_paths {
                                    // Check if child starts with pseudo-selector
                                    let is_pseudo_only = child.first().is_some_and(|s| matches!(s, CssPathSelector::PseudoSelector(_)));
                                    let mut combined = parent.clone();
                                    if !is_pseudo_only && !child.is_empty() {
                                        combined.push(CssPathSelector::Children);
                                    }
                                    combined.extend(child.iter().cloned());
                                    result.push(combined);
                                }
                            }
                            result
                        }
                    };

                    // Push to nesting stack
                    nesting_stack.push(NestingLevel {
                        paths: combined_paths,
                        declarations: std::mem::take(&mut current_declarations),
                        depth: block_nesting,
                    });
                    current_paths.clear();
                }
            }
            Token::Comma => {
                // Comma separates selectors
                if !last_path.is_empty() {
                    current_paths.push(last_path.clone());
                    last_path.clear();
                }
            }
            Token::BlockEnd => {
                if block_nesting == 0 {
                    warn_and_continue!(CssParseWarnMsgInner::MalformedStructure {
                        message: "Block end without matching block start"
                    });
                }

                // Collect all conditions from the current @-rule stack
                let current_conditions: Vec<DynamicSelector> = at_rule_stack
                    .iter()
                    .flat_map(|(conds, _)| conds.iter().cloned())
                    .collect();

                // Pop @-rule conditions that are at this nesting level
                while let Some((_, level)) = at_rule_stack.last() {
                    if *level >= block_nesting {
                        at_rule_stack.pop();
                    } else {
                        break;
                    }
                }

                block_nesting = block_nesting.saturating_sub(1);

                // Pop from nesting stack if we have one
                if let Some(level) = nesting_stack.pop() {
                    // Emit CSS blocks for all paths at this level
                    if !level.paths.is_empty() && !current_declarations.is_empty() {
                        css_blocks.extend(level.paths.iter().map(|path| UnparsedCssRuleBlock {
                            path: CssPath {
                                selectors: path.clone().into(),
                            },
                            declarations: current_declarations.clone(),
                            conditions: current_conditions.clone(),
                        }));
                    }
                    // Restore parent declarations
                    current_declarations = level.declarations;
                }

                last_path.clear();
                current_paths.clear();
            }
            Token::UniversalSelector => {
                last_path.push(CssPathSelector::Global);
            }
            Token::TypeSelector(div_type) => {
                match NodeTypeTag::from_str(div_type) {
                    Ok(nt) => last_path.push(CssPathSelector::Type(nt)),
                    Err(e) => {
                        warn_and_continue!(CssParseWarnMsgInner::SkippedRule {
                            selector: Some(div_type),
                            error: e.into(),
                        });
                    }
                }
            }
            Token::IdSelector(id) => {
                last_path.push(CssPathSelector::Id(id.to_string().into()));
            }
            Token::ClassSelector(class) => {
                last_path.push(CssPathSelector::Class(class.to_string().into()));
            }
            Token::Combinator(Combinator::GreaterThan) => {
                last_path.push(CssPathSelector::DirectChildren);
            }
            Token::Combinator(Combinator::Space) => {
                last_path.push(CssPathSelector::Children);
            }
            Token::Combinator(Combinator::Plus) => {
                last_path.push(CssPathSelector::AdjacentSibling);
            }
            Token::Combinator(Combinator::Tilde) => {
                last_path.push(CssPathSelector::GeneralSibling);
            }
            Token::PseudoClass { selector, value } | Token::DoublePseudoClass { selector, value } => {
                match pseudo_selector_from_str(selector, value) {
                    Ok(ps) => last_path.push(CssPathSelector::PseudoSelector(ps)),
                    Err(e) => {
                        warn_and_continue!(CssParseWarnMsgInner::SkippedRule {
                            selector: Some(selector),
                            error: e.into(),
                        });
                    }
                }
            }
            Token::AttributeSelector(attr) => {
                if let Some(sel) = parse_attribute_selector(attr) { last_path.push(CssPathSelector::Attribute(sel)) } else { warn_and_continue!(CssParseWarnMsgInner::MalformedStructure {
                    message: "Malformed attribute selector, rule skipped",
                }) }
            }
            Token::Declaration(key, val) => {
                current_declarations.insert(
                    key,
                    (val, ErrorLocationRange { start: last_error_location, end: get_error_location(tokenizer) }),
                );
            }
            Token::EndOfStream => {
                if block_nesting != 0 {
                    warnings.push(CssParseWarnMsg {
                        warning: CssParseWarnMsgInner::MalformedStructure {
                            message: "Unclosed blocks at end of file",
                        },
                        location: ErrorLocationRange { start: last_error_location, end: get_error_location(tokenizer) },
                    });
                }
                break;
            }
            _ => { /* Ignore unsupported tokens */ }
        }

        last_error_location = get_error_location(tokenizer);
    }

    // Process the collected CSS blocks and convert warnings
    let (stylesheet, mut block_warnings) = css_blocks_to_stylesheet(css_blocks, css_string);
    warnings.append(&mut block_warnings);

    (stylesheet, warnings)
}

fn css_blocks_to_stylesheet<'a>(
    css_blocks: Vec<UnparsedCssRuleBlock<'a>>,
    css_string: &'a str,
) -> (Vec<CssRuleBlock>, Vec<CssParseWarnMsg<'a>>) {
    let css_key_map = crate::props::property::get_css_key_map();
    let mut warnings = Vec::new();
    let mut parsed_css_blocks = Vec::new();

    for unparsed_css_block in css_blocks {
        let mut declarations = Vec::<CssDeclaration>::new();

        for (unparsed_css_key, (unparsed_css_value, location)) in &unparsed_css_block.declarations {
            match parse_declaration_resilient(
                unparsed_css_key,
                unparsed_css_value,
                *location,
                &css_key_map,
            ) {
                Ok(decls) => declarations.extend(decls),
                Err(e) => {
                    warnings.push(CssParseWarnMsg {
                        warning: CssParseWarnMsgInner::SkippedDeclaration {
                            key: unparsed_css_key,
                            value: unparsed_css_value,
                            error: e,
                        },
                        location: *location,
                    });
                }
            }
        }

        parsed_css_blocks.push(CssRuleBlock {
            path: unparsed_css_block.path,
            declarations: declarations.into(),
            conditions: unparsed_css_block.conditions.into(),
            priority: crate::css::rule_priority::AUTHOR,
        });
    }

    (parsed_css_blocks, warnings)
}

fn parse_declaration_resilient<'a>(
    unparsed_css_key: &'a str,
    unparsed_css_value: &'a str,
    location: ErrorLocationRange,
    css_key_map: &CssKeyMap,
) -> Result<Vec<CssDeclaration>, CssParseErrorInner<'a>> {
    let mut declarations = Vec::new();

    if let Some(combined_key) = CombinedCssPropertyType::from_str(unparsed_css_key, css_key_map) {
        if check_if_value_is_css_var(unparsed_css_value).is_some() {
            return Err(CssParseErrorInner::VarOnShorthandProperty {
                key: combined_key,
                value: unparsed_css_value,
            });
        }

        // Attempt to parse combined properties, continue with what succeeds
        match parse_combined_css_property(combined_key, unparsed_css_value) {
            Ok(parsed_props) => {
                declarations.extend(parsed_props.into_iter().map(CssDeclaration::Static));
            }
            Err(e) => return Err(CssParseErrorInner::DynamicCssParseError(e.into())),
        }
    } else if let Some(normal_key) = CssPropertyType::from_str(unparsed_css_key, css_key_map) {
        if let Some(css_var) = check_if_value_is_css_var(unparsed_css_value) {
            let (css_var_id, css_var_default) = css_var?;
            match parse_css_property(normal_key, css_var_default) {
                Ok(parsed_default) => {
                    declarations.push(CssDeclaration::Dynamic(DynamicCssProperty {
                        dynamic_id: css_var_id.to_string().into(),
                        default_value: parsed_default,
                    }));
                }
                Err(e) => return Err(CssParseErrorInner::DynamicCssParseError(e.into())),
            }
        } else {
            match parse_css_property(normal_key, unparsed_css_value) {
                Ok(parsed_value) => {
                    declarations.push(CssDeclaration::Static(parsed_value));
                }
                Err(e) => return Err(CssParseErrorInner::DynamicCssParseError(e.into())),
            }
        }
    } else {
        return Err(CssParseErrorInner::UnknownPropertyKey(
            unparsed_css_key,
            unparsed_css_value,
        ));
    }

    Ok(declarations)
}

/// Parses a single CSS key-value declaration, appending results to `declarations`.
///
/// Unknown property keys are downgraded to warnings (pushed into `warnings`)
/// rather than causing a hard error, so callers can continue processing the
/// remaining declarations in a rule block.
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `css-declaration` value.
pub fn parse_css_declaration<'a>(
    unparsed_css_key: &'a str,
    unparsed_css_value: &'a str,
    location: ErrorLocationRange,
    css_key_map: &CssKeyMap,
    warnings: &mut Vec<CssParseWarnMsg<'a>>,
    declarations: &mut Vec<CssDeclaration>,
) -> Result<(), CssParseErrorInner<'a>> {
    match parse_declaration_resilient(unparsed_css_key, unparsed_css_value, location, css_key_map) {
        Ok(mut decls) => {
            declarations.append(&mut decls);
            Ok(())
        }
        Err(e) => {
            if let CssParseErrorInner::UnknownPropertyKey(key, val) = &e {
                warnings.push(CssParseWarnMsg {
                    warning: CssParseWarnMsgInner::UnsupportedKeyValuePair { key, value: val },
                    location,
                });
                Ok(()) // Continue processing despite unknown property
            } else {
                Err(e) // Propagate other errors
            }
        }
    }
}

fn check_if_value_is_css_var(
    unparsed_css_value: &str,
) -> Option<Result<(&str, &str), CssParseErrorInner<'_>>> {
    const DEFAULT_VARIABLE_DEFAULT: &str = "none";

    let (_, brace_contents) = parse_parentheses(unparsed_css_value, &["var"]).ok()?;

    // value is a CSS variable, i.e. var(--main-bg-color)
    Some(match parse_css_variable_brace_contents(brace_contents) {
        Some((variable_id, default_value)) => Ok((
            variable_id,
            default_value.unwrap_or(DEFAULT_VARIABLE_DEFAULT),
        )),
        None => Err(DynamicCssParseError::InvalidBraceContents(brace_contents).into()),
    })
}

/// Parses the brace contents of a css var, i.e.:
///
/// ```no_run,ignore
/// "--main-bg-col, blue" => (Some("main-bg-col"), Some("blue"))
/// "--main-bg-col"       => (Some("main-bg-col"), None)
/// ```
fn parse_css_variable_brace_contents(input: &str) -> Option<(&str, Option<&str>)> {
    let input = input.trim();

    let mut split_comma_iter = input.splitn(2, ',');
    let var_name = split_comma_iter.next()?;
    let var_name = var_name.trim();

    if !var_name.starts_with("--") {
        return None; // no proper CSS variable name
    }

    Some((&var_name[2..], split_comma_iter.next()))
}

#[cfg(test)]
#[allow(
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    unused_qualifications,
    single_use_lifetimes
)]
mod autotest_generated {
    use super::*;
    use crate::css::CssNthChildPattern;

    // ---------------------------------------------------------------------
    // helpers
    // ---------------------------------------------------------------------

    /// Runs `f`, converting a panic into `Err(message)` so that a *panicking*
    /// function under test produces a readable assertion failure instead of
    /// tearing down the test binary. `[profile.test] panic = "unwind"` is set
    /// in the workspace root `Cargo.toml`, so unwinding is available here.
    fn catch<R>(f: impl FnOnce() -> R) -> Result<R, String> {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)).map_err(|e| {
            e.downcast_ref::<String>().cloned().unwrap_or_else(|| {
                e.downcast_ref::<&str>()
                    .map_or_else(|| "<non-string panic payload>".to_string(), |s| (*s).to_string())
            })
        })
    }

    fn key_map() -> CssKeyMap {
        crate::props::property::get_css_key_map()
    }

    fn loc(start: usize, end: usize) -> ErrorLocationRange {
        ErrorLocationRange {
            start: ErrorLocation { original_pos: start },
            end: ErrorLocation { original_pos: end },
        }
    }

    /// A grab-bag of hostile inputs reused across the string parsers.
    const HOSTILE: &[&str] = &[
        "",
        " ",
        "   \t\n\r  ",
        "\0",
        "\u{1F600}",
        "e\u{301}\u{301}\u{301}",
        "-0",
        "0",
        "NaN",
        "inf",
        "-inf",
        "9223372036854775807",
        "-9223372036854775808",
        "18446744073709551616",
        "1e309",
        ";",
        "{}",
        "()",
        "((((",
        "))))",
        "\"",
        "'",
        "\\",
        "//",
        "/*",
        "valid;garbage",
        "  valid  ",
        "a=b=c",
        ":::",
        "--",
    ];

    // =====================================================================
    // parsers -> malformed / huge / boundary / unicode
    // =====================================================================

    // --- pseudo_selector_from_str ----------------------------------------

    #[test]
    fn pseudo_selector_from_str_valid_minimal() {
        assert_eq!(
            pseudo_selector_from_str("hover", None),
            Ok(CssPathPseudoSelector::Hover)
        );
        assert_eq!(
            pseudo_selector_from_str("first", None),
            Ok(CssPathPseudoSelector::First)
        );
        assert_eq!(
            pseudo_selector_from_str("last", None),
            Ok(CssPathPseudoSelector::Last)
        );
        assert_eq!(
            pseudo_selector_from_str("active", None),
            Ok(CssPathPseudoSelector::Active)
        );
        assert_eq!(
            pseudo_selector_from_str("focus", None),
            Ok(CssPathPseudoSelector::Focus)
        );
        assert_eq!(
            pseudo_selector_from_str("dragging", None),
            Ok(CssPathPseudoSelector::Dragging)
        );
        assert_eq!(
            pseudo_selector_from_str("drag-over", None),
            Ok(CssPathPseudoSelector::DragOver)
        );
    }

    #[test]
    fn pseudo_selector_from_str_nth_child_needs_a_value() {
        assert_eq!(
            pseudo_selector_from_str("nth-child", None),
            Err(CssPseudoSelectorParseError::EmptyNthChild)
        );
        assert_eq!(
            pseudo_selector_from_str("nth-child", Some("2")),
            Ok(CssPathPseudoSelector::NthChild(CssNthChildSelector::Number(2)))
        );
    }

    #[test]
    fn pseudo_selector_from_str_lang_strips_quotes() {
        // Both quote styles are stripped, and the inner value is trimmed.
        for v in ["de-DE", "\"de-DE\"", "'de-DE'", "  \"de-DE\"  "] {
            assert_eq!(
                pseudo_selector_from_str("lang", Some(v)),
                Ok(CssPathPseudoSelector::Lang(AzString::from("de-DE".to_string()))),
                "lang value {v:?} did not normalise to de-DE"
            );
        }
        // A `:lang` with no value is rejected rather than defaulting to "".
        assert!(pseudo_selector_from_str("lang", None).is_err());
    }

    #[test]
    fn pseudo_selector_from_str_empty_and_whitespace_are_rejected() {
        assert!(pseudo_selector_from_str("", None).is_err());
        assert!(pseudo_selector_from_str("   ", None).is_err());
        assert!(pseudo_selector_from_str("\t\n", None).is_err());
        // The selector name is matched verbatim, so a padded name is *not* accepted.
        assert!(pseudo_selector_from_str(" hover ", None).is_err());
    }

    #[test]
    fn pseudo_selector_from_str_garbage_and_unicode_never_panic() {
        for s in HOSTILE {
            for v in [None, Some(*s), Some("2"), Some("\u{1F600}")] {
                let r = catch(|| pseudo_selector_from_str(s, v));
                assert!(
                    r.is_ok(),
                    "pseudo_selector_from_str({s:?}, {v:?}) panicked: {}",
                    r.unwrap_err()
                );
                // Nothing in HOSTILE is a real pseudo-selector name.
                assert!(
                    pseudo_selector_from_str(s, v).is_err(),
                    "pseudo_selector_from_str({s:?}, {v:?}) unexpectedly succeeded"
                );
            }
        }
    }

    #[test]
    fn pseudo_selector_from_str_extremely_long_input_terminates() {
        let long = "z".repeat(200_000);
        assert!(pseudo_selector_from_str(&long, None).is_err());
        // A huge *value* on a selector that ignores values must also terminate.
        assert_eq!(
            pseudo_selector_from_str("hover", Some(&long)),
            Ok(CssPathPseudoSelector::Hover)
        );
        // A huge nth-child value is rejected, not parsed into a bogus number.
        assert!(pseudo_selector_from_str("nth-child", Some(&long)).is_err());
    }

    #[test]
    fn pseudo_selector_from_str_deeply_nested_value_does_not_stack_overflow() {
        let nested = "(".repeat(10_000);
        let r = catch(|| pseudo_selector_from_str("nth-child", Some(&nested)).is_err());
        assert_eq!(r, Ok(true), "deeply nested nth-child value was not rejected safely");
    }

    // --- parse_attribute_selector ----------------------------------------

    #[test]
    fn parse_attribute_selector_valid_minimal() {
        let sel = parse_attribute_selector("href").expect("bare attribute name must parse");
        assert_eq!(sel.name.as_str(), "href");
        assert_eq!(sel.op, AttributeMatchOp::Exists);
        assert_eq!(sel.value.clone().into_option(), None);
    }

    #[test]
    fn parse_attribute_selector_all_operators() {
        let cases: [(&str, AttributeMatchOp); 6] = [
            ("a=b", AttributeMatchOp::Eq),
            ("a~=b", AttributeMatchOp::Includes),
            ("a|=b", AttributeMatchOp::DashMatch),
            ("a^=b", AttributeMatchOp::Prefix),
            ("a$=b", AttributeMatchOp::Suffix),
            ("a*=b", AttributeMatchOp::Substring),
        ];
        for (input, expected_op) in cases {
            let sel = parse_attribute_selector(input)
                .unwrap_or_else(|| panic!("{input:?} should parse"));
            assert_eq!(sel.name.as_str(), "a", "wrong name for {input:?}");
            assert_eq!(sel.op, expected_op, "wrong op for {input:?}");
            assert_eq!(
                sel.value.clone().into_option().map(|v| v.as_str().to_string()),
                Some("b".to_string()),
                "wrong value for {input:?}"
            );
        }
    }

    #[test]
    fn parse_attribute_selector_quotes_are_stripped_and_unbalanced_rejected() {
        for input in ["a=\"b\"", "a='b'", "a=b", "  a  =  \"b\"  "] {
            let sel = parse_attribute_selector(input)
                .unwrap_or_else(|| panic!("{input:?} should parse"));
            assert_eq!(
                sel.value.clone().into_option().map(|v| v.as_str().to_string()),
                Some("b".to_string()),
                "quotes not stripped for {input:?}"
            );
        }
        // Unbalanced quoting is a hard reject, not a silent half-strip.
        for input in ["a=\"b", "a=b\"", "a='b", "a=b'", "a=\"b'", "a=\"", "a='"] {
            assert!(
                parse_attribute_selector(input).is_none(),
                "unbalanced quote {input:?} should be rejected"
            );
        }
    }

    #[test]
    fn parse_attribute_selector_empty_and_malformed_are_rejected() {
        for input in ["", "   ", "\t\n", "=", "=b", "  =b", "\"a\"", "'a'"] {
            assert!(
                parse_attribute_selector(input).is_none(),
                "{input:?} should be rejected (empty/quoted name)"
            );
        }
        // Names may not contain whitespace.
        assert!(parse_attribute_selector("a b").is_none());
        assert!(parse_attribute_selector("a b=c").is_none());
    }

    #[test]
    fn parse_attribute_selector_unicode_name_is_accepted_and_does_not_panic() {
        let sel = parse_attribute_selector("data-\u{1F600}")
            .expect("a non-ASCII attribute name has no whitespace/quotes, so it is accepted");
        assert_eq!(sel.name.as_str(), "data-\u{1F600}");

        // Multi-byte values must not be sliced mid-char.
        let sel = parse_attribute_selector("lang=\"\u{4E2D}\u{6587}\"").expect("unicode value");
        assert_eq!(
            sel.value.clone().into_option().map(|v| v.as_str().to_string()),
            Some("\u{4E2D}\u{6587}".to_string())
        );
    }

    /// Invariant: whatever comes back, the name is never empty and never contains
    /// whitespace or quotes -- those are exactly the cases the parser promises to
    /// reject. This holds regardless of how the operator split is implemented.
    #[test]
    fn parse_attribute_selector_result_invariants_hold_for_hostile_input() {
        let long = format!("a={}", "x".repeat(100_000));
        let nested = format!("a={}", "[".repeat(10_000));
        let mut inputs: Vec<&str> = HOSTILE.to_vec();
        inputs.push(&long);
        inputs.push(&nested);
        inputs.push("title=\"a~=b\"");
        inputs.push("a=b=c");
        inputs.push("[[[[]]]]");

        for input in inputs {
            let parsed = match catch(|| parse_attribute_selector(input)) {
                Ok(p) => p,
                Err(msg) => panic!("parse_attribute_selector({input:?}) panicked: {msg}"),
            };
            if let Some(sel) = parsed {
                let name = sel.name.as_str();
                assert!(!name.is_empty(), "empty name accepted for {input:?}");
                assert!(
                    !name.chars().any(|c| c.is_whitespace() || c == '"' || c == '\''),
                    "name {name:?} contains whitespace/quotes for input {input:?}"
                );
            }
        }
    }

    // --- strip_attribute_quotes (private) --------------------------------

    #[test]
    fn strip_attribute_quotes_balanced_unquoted_and_unbalanced() {
        // Balanced -> stripped.
        assert_eq!(strip_attribute_quotes("\"abc\""), Some("abc"));
        assert_eq!(strip_attribute_quotes("'abc'"), Some("abc"));
        assert_eq!(strip_attribute_quotes("\"\""), Some(""));
        assert_eq!(strip_attribute_quotes("''"), Some(""));
        // Unquoted -> unchanged.
        assert_eq!(strip_attribute_quotes("abc"), Some("abc"));
        assert_eq!(strip_attribute_quotes(""), Some(""));
        assert_eq!(strip_attribute_quotes("a"), Some("a"));
        // Unbalanced -> None.
        assert_eq!(strip_attribute_quotes("\"abc"), None);
        assert_eq!(strip_attribute_quotes("abc\""), None);
        assert_eq!(strip_attribute_quotes("'abc"), None);
        assert_eq!(strip_attribute_quotes("abc'"), None);
        assert_eq!(strip_attribute_quotes("\"abc'"), None);
        assert_eq!(strip_attribute_quotes("\""), None);
        assert_eq!(strip_attribute_quotes("'"), None);
    }

    /// The function slices with raw byte indices (`&s[1..s.len() - 1]`), so a
    /// multi-byte first/last char is the interesting boundary case. No byte of a
    /// multi-byte UTF-8 sequence can equal `"` (0x22) or `'` (0x27), so the slice
    /// must always land on a char boundary.
    #[test]
    fn strip_attribute_quotes_multibyte_boundaries_never_panic() {
        let cases = [
            "\u{1F600}",
            "\"\u{1F600}\"",
            "'\u{4E2D}\u{6587}'",
            "\u{4E2D}\u{6587}",
            "\"\u{301}\"",
            "\u{301}",
        ];
        for s in cases {
            let r = catch(|| strip_attribute_quotes(s));
            assert!(r.is_ok(), "strip_attribute_quotes({s:?}) panicked: {}", r.unwrap_err());
        }
        assert_eq!(strip_attribute_quotes("\"\u{1F600}\""), Some("\u{1F600}"));
        assert_eq!(strip_attribute_quotes("\u{1F600}"), Some("\u{1F600}"));
    }

    #[test]
    fn strip_attribute_quotes_extremely_long_input_terminates() {
        let long = "x".repeat(500_000);
        assert_eq!(strip_attribute_quotes(&long), Some(long.as_str()));
        let quoted = format!("\"{long}\"");
        assert_eq!(strip_attribute_quotes(&quoted), Some(long.as_str()));
    }

    // --- parse_nth_child_selector / parse_nth_child_pattern (private) -----

    #[test]
    fn parse_nth_child_selector_valid_minimal() {
        assert_eq!(parse_nth_child_selector("2"), Ok(CssNthChildSelector::Number(2)));
        assert_eq!(parse_nth_child_selector("0"), Ok(CssNthChildSelector::Number(0)));
        assert_eq!(parse_nth_child_selector("even"), Ok(CssNthChildSelector::Even));
        assert_eq!(parse_nth_child_selector("odd"), Ok(CssNthChildSelector::Odd));
        assert_eq!(parse_nth_child_selector("  7  "), Ok(CssNthChildSelector::Number(7)));
        assert_eq!(
            parse_nth_child_selector("2n+3"),
            Ok(CssNthChildSelector::Pattern(CssNthChildPattern {
                pattern_repeat: 2,
                offset: 3
            }))
        );
        assert_eq!(
            parse_nth_child_selector("2n"),
            Ok(CssNthChildSelector::Pattern(CssNthChildPattern {
                pattern_repeat: 2,
                offset: 0
            }))
        );
    }

    #[test]
    fn parse_nth_child_selector_empty_is_empty_nth_child_error() {
        assert_eq!(
            parse_nth_child_selector(""),
            Err(CssPseudoSelectorParseError::EmptyNthChild)
        );
        assert_eq!(
            parse_nth_child_selector("   \t\n "),
            Err(CssPseudoSelectorParseError::EmptyNthChild)
        );
        assert_eq!(
            parse_nth_child_pattern(""),
            Err(CssPseudoSelectorParseError::EmptyNthChild)
        );
    }

    /// `u32` boundaries: MAX parses, MAX+1 and negatives are rejected via
    /// `ParseIntError` rather than wrapping or panicking.
    #[test]
    fn parse_nth_child_selector_numeric_limits_saturate_into_errors() {
        assert_eq!(
            parse_nth_child_selector("4294967295"),
            Ok(CssNthChildSelector::Number(u32::MAX))
        );
        for overflow in [
            "4294967296",
            "18446744073709551616",
            "99999999999999999999999999",
            "-1",
            "-0",
        ] {
            assert!(
                parse_nth_child_selector(overflow).is_err(),
                "{overflow:?} must not parse as an nth-child index"
            );
        }
        // Huge digit runs must be rejected, not truncated -- and must terminate.
        let huge = "9".repeat(100_000);
        assert!(parse_nth_child_selector(&huge).is_err());
        let huge_repeat = format!("{}n+1", "9".repeat(100_000));
        assert!(parse_nth_child_pattern(&huge_repeat).is_err());
    }

    #[test]
    fn parse_nth_child_selector_float_and_non_finite_strings_are_rejected() {
        for v in ["NaN", "inf", "-inf", "1.5", "1e5", "0x2", "+2", " 2 n "] {
            let r = catch(|| parse_nth_child_selector(v));
            assert!(r.is_ok(), "parse_nth_child_selector({v:?}) panicked: {}", r.unwrap_err());
        }
        assert!(parse_nth_child_selector("NaN").is_err());
        assert!(parse_nth_child_selector("inf").is_err());
        assert!(parse_nth_child_selector("1.5").is_err());
    }

    #[test]
    fn parse_nth_child_pattern_malformed_offsets_are_rejected() {
        // Trailing "+" with no offset.
        assert_eq!(
            parse_nth_child_pattern("2n+"),
            Err(CssPseudoSelectorParseError::InvalidNthChildPattern("2n+"))
        );
        assert!(parse_nth_child_pattern("2n+   ").is_err());
        assert!(parse_nth_child_pattern("2n+x").is_err());
        assert!(parse_nth_child_pattern("xn+1").is_err());
        // The `.split('n').next()` / `.split('+').next().unwrap()` pair must never
        // panic, no matter what the input looks like.
        for s in HOSTILE {
            let r = catch(|| parse_nth_child_pattern(s));
            assert!(r.is_ok(), "parse_nth_child_pattern({s:?}) panicked: {}", r.unwrap_err());
        }
    }

    #[test]
    fn parse_nth_child_selector_unicode_never_panics() {
        for v in ["\u{1F600}", "\u{FF12}", "2\u{301}", "e\u{301}ven", "\u{4E2D}n+\u{6587}"] {
            let r = catch(|| parse_nth_child_selector(v));
            assert!(r.is_ok(), "parse_nth_child_selector({v:?}) panicked: {}", r.unwrap_err());
            assert!(
                parse_nth_child_selector(v).is_err(),
                "{v:?} is not a valid nth-child value"
            );
        }
    }

    // --- parse_css_path ---------------------------------------------------

    #[test]
    fn parse_css_path_valid_minimal() {
        // Positive control, mirrors the doc example on `parse_css_path`.
        assert_eq!(
            parse_css_path("* div #my_id > .class:nth-child(2)"),
            Ok(CssPath {
                selectors: vec![
                    CssPathSelector::Global,
                    CssPathSelector::Type(NodeTypeTag::from_str("div").unwrap()),
                    CssPathSelector::Children,
                    CssPathSelector::Id("my_id".to_string().into()),
                    CssPathSelector::DirectChildren,
                    CssPathSelector::Class("class".to_string().into()),
                    CssPathSelector::PseudoSelector(CssPathPseudoSelector::NthChild(
                        CssNthChildSelector::Number(2)
                    )),
                ]
                .into()
            })
        );
        assert_eq!(
            parse_css_path("div"),
            Ok(CssPath {
                selectors: vec![CssPathSelector::Type(NodeTypeTag::from_str("div").unwrap())]
                    .into()
            })
        );
    }

    #[test]
    fn parse_css_path_empty_and_whitespace_are_empty_path_errors() {
        assert_eq!(parse_css_path(""), Err(CssPathParseError::EmptyPath));
        assert_eq!(parse_css_path("   "), Err(CssPathParseError::EmptyPath));
        assert_eq!(parse_css_path("\t\r\n "), Err(CssPathParseError::EmptyPath));
        // An unknown type tag is now a hard error (Selectors L4: an invalid simple
        // selector invalidates the selector) rather than being silently dropped into an
        // empty path. See parse_css_path_unknown_type_tag_is_not_silently_dropped.
        assert!(matches!(
            parse_css_path("definitelynotatag"),
            Err(CssPathParseError::NodeTypeTag(_))
        ));
    }

    #[test]
    fn parse_css_path_garbage_and_unicode_never_panic() {
        let long = "div ".repeat(50_000);
        let brackets = "[".repeat(10_000);
        let braces = "{".repeat(10_000);
        let mut inputs: Vec<&str> = HOSTILE.to_vec();
        inputs.push(&long);
        inputs.push(&brackets);
        inputs.push(&braces);
        inputs.push("div;garbage");
        inputs.push("div }");
        inputs.push(":::::");
        inputs.push("\u{1F600} > \u{4E2D}\u{6587}");

        for input in inputs {
            let r = catch(|| parse_css_path(input));
            assert!(r.is_ok(), "parse_css_path({:.40?}) panicked: {}", input, r.unwrap_err());
        }
    }

    #[test]
    fn parse_css_path_rejects_block_tokens() {
        // `{` / `}` are not path tokens; they must not silently produce a path.
        for input in ["div { }", "div {", "}"] {
            assert!(
                parse_css_path(input).is_err(),
                "{input:?} contains block tokens and must not parse as a path"
            );
        }
    }

    #[test]
    fn parse_css_path_unknown_pseudo_selector_is_an_error() {
        assert!(parse_css_path("div:definitelynotapseudo").is_err());
        assert!(parse_css_path(".x:nth-child(notanumber)").is_err());
    }

    /// BUG (red): `parse_css_path` swallows an unknown type selector
    /// (`if let Ok(nt) = NodeTypeTag::from_str(..)` with no `else`), so
    /// `"div definitelynotatag"` parses as `[Type(Div), Children]` -- a path that
    /// ends in a dangling descendant combinator and therefore matches *every*
    /// descendant of `div`, silently widening the selector. It should either be
    /// rejected (like `new_from_str_inner`, which emits a `SkippedRule` warning)
    /// or not leave a trailing combinator behind.
    #[test]
    fn parse_css_path_unknown_type_tag_is_not_silently_dropped() {
        let parsed = parse_css_path("div definitelynotatag");
        if let Ok(path) = &parsed {
            let selectors = path.selectors.as_slice();
            assert!(
                !matches!(
                    selectors.last(),
                    Some(
                        CssPathSelector::Children
                            | CssPathSelector::DirectChildren
                            | CssPathSelector::AdjacentSibling
                            | CssPathSelector::GeneralSibling
                    )
                ),
                "BUG: the unknown type tag was dropped, leaving a dangling combinator; \
                 `div definitelynotatag` now matches every descendant of div. \
                 selectors = {selectors:?}"
            );
        }
    }

    // --- parse_media_conditions / parse_media_feature ---------------------

    #[test]
    fn parse_media_conditions_valid_minimal() {
        assert_eq!(
            parse_media_conditions("screen"),
            vec![DynamicSelector::Media(MediaType::Screen)]
        );
        assert_eq!(
            parse_media_conditions("PRINT"),
            vec![DynamicSelector::Media(MediaType::Print)]
        );
        assert_eq!(
            parse_media_conditions("all"),
            vec![DynamicSelector::Media(MediaType::All)]
        );
    }

    #[test]
    fn parse_media_conditions_parenthesised_and_compound() {
        let conds = parse_media_conditions("(min-width: 800px)");
        assert_eq!(conds.len(), 1);
        match &conds[0] {
            DynamicSelector::ViewportWidth(r) => {
                assert_eq!(r.min(), Some(800.0));
                assert_eq!(r.max(), None);
            }
            other => panic!("expected ViewportWidth, got {other:?}"),
        }

        let conds = parse_media_conditions("screen and (max-width: 600px)");
        assert_eq!(conds.len(), 2, "compound query should yield both conditions");
        assert_eq!(conds[0], DynamicSelector::Media(MediaType::Screen));
        match &conds[1] {
            DynamicSelector::ViewportWidth(r) => {
                assert_eq!(r.min(), None);
                assert_eq!(r.max(), Some(600.0));
            }
            other => panic!("expected ViewportWidth, got {other:?}"),
        }
    }

    #[test]
    fn parse_media_conditions_empty_and_garbage_yield_no_conditions() {
        for input in ["", "   ", "((((", "))))", "\u{1F600}", "and", "(", ")", "()"] {
            let r = catch(|| parse_media_conditions(input));
            match r {
                Ok(conds) => assert!(
                    conds.is_empty(),
                    "{input:?} should not produce media conditions, got {conds:?}"
                ),
                Err(msg) => panic!("parse_media_conditions({input:?}) panicked: {msg}"),
            }
        }
    }

    #[test]
    fn parse_media_conditions_extremely_long_and_deeply_nested_terminate() {
        let nested = format!("({})", "(".repeat(10_000));
        let r = catch(|| parse_media_conditions(&nested));
        assert!(r.is_ok(), "deeply nested media query panicked: {}", r.unwrap_err());

        let long = "screen and ".repeat(20_000);
        let r = catch(|| parse_media_conditions(&long));
        assert!(r.is_ok(), "very long media query panicked: {}", r.unwrap_err());
    }

    #[test]
    fn parse_media_feature_known_features() {
        assert_eq!(
            parse_media_feature("orientation: portrait"),
            Some(DynamicSelector::Orientation(OrientationType::Portrait))
        );
        assert_eq!(
            parse_media_feature("orientation: LANDSCAPE"),
            Some(DynamicSelector::Orientation(OrientationType::Landscape))
        );
        assert_eq!(
            parse_media_feature("prefers-color-scheme: dark"),
            Some(DynamicSelector::Theme(ThemeCondition::Dark))
        );
        assert_eq!(
            parse_media_feature("prefers-reduced-motion: reduce"),
            Some(DynamicSelector::PrefersReducedMotion(BoolCondition::True))
        );
        assert_eq!(
            parse_media_feature("prefers-contrast: more"),
            Some(DynamicSelector::PrefersHighContrast(BoolCondition::True))
        );
        // Keys are matched case-insensitively.
        assert!(parse_media_feature("MIN-WIDTH: 800px").is_some());
    }

    #[test]
    fn parse_media_feature_malformed_returns_none() {
        for input in [
            "",
            "   ",
            "nocolon",
            "min-width:",
            "min-width: ",
            "min-width: abc",
            ": 800px",
            "unknown-feature: 800px",
            "orientation: sideways",
            "\u{1F600}: \u{1F600}",
        ] {
            let r = catch(|| parse_media_feature(input));
            match r {
                Ok(v) => assert!(v.is_none(), "{input:?} should be rejected, got {v:?}"),
                Err(msg) => panic!("parse_media_feature({input:?}) panicked: {msg}"),
            }
        }
    }

    /// BUG (red): `MinMaxRange` uses `f32::NAN` as its "no bound" sentinel, and
    /// `parse_px_value` happily parses `"NaN"` (Rust's `f32::from_str` accepts it).
    /// So `@media (min-width: NaN)` produces a `ViewportWidth` whose `min()` is
    /// `None` -- a viewport constraint that constrains nothing and therefore
    /// matches *every* viewport, instead of the media query being rejected.
    #[test]
    fn parse_media_feature_nan_width_does_not_erase_the_constraint() {
        for feature in ["min-width: NaN", "min-width: NaNpx", "max-width: nan"] {
            match parse_media_feature(feature) {
                None => {} // acceptable: the feature was rejected outright
                Some(DynamicSelector::ViewportWidth(r)) => {
                    assert!(
                        r.min().is_some() || r.max().is_some(),
                        "BUG: {feature:?} parsed into a ViewportWidth with no bounds at all \
                         (the NaN collided with MinMaxRange's `absent` sentinel), so the \
                         media query silently matches every viewport"
                    );
                }
                Some(other) => panic!("unexpected selector for {feature:?}: {other:?}"),
            }
        }
    }

    // --- parse_px_value ---------------------------------------------------

    #[test]
    fn parse_px_value_valid_minimal() {
        assert_eq!(parse_px_value("800px"), Some(800.0));
        assert_eq!(parse_px_value("800"), Some(800.0));
        assert_eq!(parse_px_value("  800px  "), Some(800.0));
        assert_eq!(parse_px_value("0"), Some(0.0));
        assert_eq!(parse_px_value("1.5px"), Some(1.5));
        assert_eq!(parse_px_value("-10px"), Some(-10.0));
    }

    #[test]
    fn parse_px_value_malformed_returns_none() {
        for input in ["", "   ", "px", "abc", "8 0 0", "800pxx", "\u{1F600}", "800%", "--"] {
            let r = catch(|| parse_px_value(input));
            match r {
                Ok(v) => assert!(v.is_none(), "{input:?} should be rejected, got {v:?}"),
                Err(msg) => panic!("parse_px_value({input:?}) panicked: {msg}"),
            }
        }
    }

    /// f32 range boundaries: overflow saturates to +/-inf and underflow to zero
    /// (that is `f32::from_str`'s documented behaviour) -- neither may panic.
    #[test]
    fn parse_px_value_overflow_and_underflow_saturate_without_panicking() {
        assert_eq!(parse_px_value("-0"), Some(-0.0));
        assert_eq!(parse_px_value("1e-50"), Some(0.0));
        assert_eq!(parse_px_value("3.4e38px"), Some(3.4e38));

        // FIXED: parse_px_value now rejects non-finite results (see
        // parse_px_value_rejects_non_finite_values). "1e39" is valid CSS number
        // *syntax* but overflows f32 to infinity, and an infinite length is exactly
        // the non-finite value that would collide with MinMaxRange's NaN "no bound"
        // sentinel — so it is rejected rather than saturated. (Was: Some(inf).)
        assert_eq!(parse_px_value("1e39px"), None);

        let huge_digits = "9".repeat(100_000);
        let r = catch(|| parse_px_value(&huge_digits));
        assert!(r.is_ok(), "a 100k-digit number panicked: {}", r.unwrap_err());
    }

    /// BUG (red): `f32::from_str` accepts `"NaN"`, `"inf"` and `"infinity"`, none of
    /// which are valid CSS `<length>` values. Because `MinMaxRange` encodes "no
    /// bound" as `NaN`, letting a NaN through silently turns a constraint into a
    /// wildcard (see `parse_media_feature_nan_width_does_not_erase_the_constraint`).
    /// `parse_px_value` should reject non-finite values at the source.
    #[test]
    fn parse_px_value_rejects_non_finite_values() {
        for input in ["NaN", "nan", "inf", "-inf", "infinity", "NaNpx", "infpx", "-infpx"] {
            if let Some(px) = parse_px_value(input) {
                assert!(
                    px.is_finite(),
                    "BUG: parse_px_value({input:?}) returned the non-finite value {px}; \
                     a non-finite length is not valid CSS and collides with MinMaxRange's \
                     NaN `absent` sentinel"
                );
            }
        }
    }

    // --- parse_ratio_value ------------------------------------------------

    #[test]
    fn parse_ratio_value_valid_minimal() {
        let r = parse_ratio_value("16/9").expect("16/9 should parse");
        assert!((r - (16.0 / 9.0)).abs() < 1e-6, "16/9 parsed as {r}");
        let r = parse_ratio_value("1.777").expect("bare float should parse");
        assert!((r - 1.777).abs() < 1e-6);
        let r = parse_ratio_value("  16 / 9  ").expect("whitespace should be trimmed");
        assert!((r - (16.0 / 9.0)).abs() < 1e-6);
    }

    #[test]
    fn parse_ratio_value_division_by_zero_is_rejected() {
        // Both +0.0 and -0.0 denominators must be caught by the `den == 0.0` guard.
        assert_eq!(parse_ratio_value("1/0"), None);
        assert_eq!(parse_ratio_value("1/-0"), None);
        assert_eq!(parse_ratio_value("0/0"), None);
        assert_eq!(parse_ratio_value("16/0.0"), None);
    }

    #[test]
    fn parse_ratio_value_malformed_returns_none() {
        for input in ["", "   ", "/", "16/", "/9", "a/b", "1/2/3", "\u{1F600}", "16:9"] {
            let r = catch(|| parse_ratio_value(input));
            match r {
                Ok(v) => assert!(v.is_none(), "{input:?} should be rejected, got {v:?}"),
                Err(msg) => panic!("parse_ratio_value({input:?}) panicked: {msg}"),
            }
        }
    }

    /// BUG (red): the `den == 0.0` guard catches division by zero but not the
    /// non-finite operands that produce NaN anyway -- `inf/inf` and `1/NaN` both
    /// yield NaN, which then becomes MinMaxRange's "no bound" sentinel and turns
    /// an `aspect-ratio` query into a wildcard.
    #[test]
    fn parse_ratio_value_never_returns_nan() {
        for input in ["NaN", "inf/inf", "NaN/1", "1/NaN", "-inf/inf", "inf/-inf"] {
            if let Some(r) = parse_ratio_value(input) {
                assert!(
                    !r.is_nan(),
                    "BUG: parse_ratio_value({input:?}) returned NaN, which MinMaxRange \
                     reads back as `no bound` -- the aspect-ratio query silently matches \
                     everything"
                );
            }
        }
    }

    #[test]
    fn parse_ratio_value_extremely_long_input_terminates() {
        let huge = format!("{}/{}", "9".repeat(50_000), "9".repeat(50_000));
        let r = catch(|| parse_ratio_value(&huge));
        assert!(r.is_ok(), "huge ratio panicked: {}", r.unwrap_err());
    }

    // --- parse_container_conditions / parse_container_feature -------------

    #[test]
    fn parse_container_conditions_valid_minimal() {
        // Bare name.
        assert_eq!(
            parse_container_conditions("sidebar"),
            vec![DynamicSelector::ContainerName(AzString::from(
                "sidebar".to_string()
            ))]
        );

        // Anonymous query.
        let conds = parse_container_conditions("(min-width: 400px)");
        assert_eq!(conds.len(), 1);
        match &conds[0] {
            DynamicSelector::ContainerWidth(r) => {
                assert_eq!(r.min(), Some(400.0));
                assert_eq!(r.max(), None);
            }
            other => panic!("expected ContainerWidth, got {other:?}"),
        }

        // Named query -> name + condition.
        let conds = parse_container_conditions("sidebar (min-width: 400px)");
        assert_eq!(conds.len(), 2);
        assert_eq!(
            conds[0],
            DynamicSelector::ContainerName(AzString::from("sidebar".to_string()))
        );
        assert!(matches!(conds[1], DynamicSelector::ContainerWidth(_)));
    }

    #[test]
    fn parse_container_conditions_empty_and_garbage_never_panic() {
        assert!(parse_container_conditions("").is_empty());
        assert!(parse_container_conditions("   ").is_empty());

        let nested = "(".repeat(10_000);
        let long = "a".repeat(200_000);
        let mut inputs: Vec<&str> = HOSTILE.to_vec();
        inputs.push(&nested);
        inputs.push(&long);

        for input in inputs {
            let r = catch(|| parse_container_conditions(input));
            assert!(
                r.is_ok(),
                "parse_container_conditions({:.40?}) panicked: {}",
                input,
                r.unwrap_err()
            );
        }
    }

    #[test]
    fn parse_container_feature_malformed_returns_none() {
        for input in ["", "   ", "nocolon", "min-width:", "min-width: abc", "unknown: 1px"] {
            let r = catch(|| parse_container_feature(input));
            match r {
                Ok(v) => assert!(v.is_none(), "{input:?} should be rejected, got {v:?}"),
                Err(msg) => panic!("parse_container_feature({input:?}) panicked: {msg}"),
            }
        }
        assert!(parse_container_feature("min-height: 400px").is_some());
        assert!(parse_container_feature("MAX-WIDTH: 400px").is_some());
    }

    // --- parse_theme_condition / parse_lang_condition ---------------------

    #[test]
    fn parse_theme_condition_valid_minimal() {
        for input in ["dark", "(dark)", "DARK", "\"dark\"", "'dark'", "(\"dark\")", "  dark  "] {
            assert_eq!(
                parse_theme_condition(input),
                Some(DynamicSelector::Theme(ThemeCondition::Dark)),
                "theme {input:?} should resolve to Dark"
            );
        }
        assert_eq!(
            parse_theme_condition("light"),
            Some(DynamicSelector::Theme(ThemeCondition::Light))
        );
    }

    #[test]
    fn parse_theme_condition_garbage_returns_none() {
        for input in ["", "   ", "(", ")", "()", "sepia", "\u{1F600}", "dark light", "\"dark"] {
            let r = catch(|| parse_theme_condition(input));
            match r {
                Ok(v) => assert!(v.is_none(), "theme {input:?} should be rejected, got {v:?}"),
                Err(msg) => panic!("parse_theme_condition({input:?}) panicked: {msg}"),
            }
        }
    }

    #[test]
    fn parse_lang_condition_valid_minimal() {
        for input in ["de-DE", "(de-DE)", "(\"de-DE\")", "('de-DE')", "  de-DE  "] {
            assert_eq!(
                parse_lang_condition(input),
                Some(DynamicSelector::Language(LanguageCondition::Prefix(
                    AzString::from("de-DE".to_string())
                ))),
                "lang {input:?} should resolve to Prefix(de-DE)"
            );
        }
    }

    #[test]
    fn parse_lang_condition_empty_returns_none() {
        for input in ["", "   ", "()", "(  )", "( )"] {
            assert_eq!(
                parse_lang_condition(input),
                None,
                "empty lang {input:?} must not produce a condition"
            );
        }
    }

    #[test]
    fn parse_lang_condition_unicode_and_long_input_never_panic() {
        let long = "a".repeat(200_000);
        let mut inputs: Vec<&str> = HOSTILE.to_vec();
        inputs.push(&long);
        for input in inputs {
            let r = catch(|| parse_lang_condition(input));
            assert!(
                r.is_ok(),
                "parse_lang_condition({:.40?}) panicked: {}",
                input,
                r.unwrap_err()
            );
        }
    }

    // --- css variables ----------------------------------------------------

    #[test]
    fn parse_css_variable_brace_contents_valid_minimal() {
        assert_eq!(
            parse_css_variable_brace_contents("--main-bg-col"),
            Some(("main-bg-col", None))
        );
        let (name, default) = parse_css_variable_brace_contents("--main-bg-col, blue")
            .expect("var with default should parse");
        assert_eq!(name, "main-bg-col");
        // NOTE: the default is returned *untrimmed* (" blue"); `parse_css_property`
        // trims it later, so assert on the trimmed form to stay fix-stable.
        assert_eq!(default.map(str::trim), Some("blue"));
    }

    #[test]
    fn parse_css_variable_brace_contents_rejects_non_variables() {
        for input in ["", "   ", "main-bg-col", "-main-bg-col", "blue", "\u{1F600}", ","] {
            assert_eq!(
                parse_css_variable_brace_contents(input),
                None,
                "{input:?} is not a `--` prefixed CSS variable"
            );
        }
    }

    /// The function slices `&var_name[2..]` after a `starts_with("--")` check.
    /// `--` is ASCII, so byte 2 is always a char boundary even when the variable
    /// name itself is multi-byte.
    #[test]
    fn parse_css_variable_brace_contents_multibyte_name_does_not_split_a_char() {
        assert_eq!(
            parse_css_variable_brace_contents("--\u{1F600}"),
            Some(("\u{1F600}", None))
        );
        // An empty name after `--` is currently accepted; assert only that it is safe.
        let r = catch(|| parse_css_variable_brace_contents("--"));
        assert!(r.is_ok(), "`--` panicked: {}", r.unwrap_err());
    }

    #[test]
    fn check_if_value_is_css_var_recognises_var_syntax() {
        // Not a var() at all.
        assert!(check_if_value_is_css_var("100px").is_none());
        assert!(check_if_value_is_css_var("").is_none());
        assert!(check_if_value_is_css_var("calc(1px + 2px)").is_none());

        // A var() without a default falls back to "none".
        match check_if_value_is_css_var("var(--main-bg-color)") {
            Some(Ok((id, default))) => {
                assert_eq!(id, "main-bg-color");
                assert_eq!(default, "none");
            }
            other => panic!("expected Some(Ok(..)), got {other:?}"),
        }

        // A var() with a default returns it.
        match check_if_value_is_css_var("var(--w, 100px)") {
            Some(Ok((id, default))) => {
                assert_eq!(id, "w");
                assert_eq!(default.trim(), "100px");
            }
            other => panic!("expected Some(Ok(..)), got {other:?}"),
        }

        // Malformed brace contents surface as an error, not a panic and not a None.
        assert!(matches!(
            check_if_value_is_css_var("var(nonsense)"),
            Some(Err(CssParseErrorInner::DynamicCssParseError(
                DynamicCssParseError::InvalidBraceContents(_)
            )))
        ));
        assert!(matches!(
            check_if_value_is_css_var("var()"),
            Some(Err(CssParseErrorInner::DynamicCssParseError(
                DynamicCssParseError::InvalidBraceContents(_)
            )))
        ));
    }

    #[test]
    fn check_if_value_is_css_var_hostile_input_never_panics() {
        let long = format!("var(--{})", "x".repeat(100_000));
        let nested = format!("var({})", "(".repeat(10_000));
        let mut inputs: Vec<&str> = HOSTILE.to_vec();
        inputs.push(&long);
        inputs.push(&nested);
        inputs.push("var(");
        inputs.push("var)");
        inputs.push("var((--x))");

        for input in inputs {
            let r = catch(|| check_if_value_is_css_var(input).is_some());
            assert!(
                r.is_ok(),
                "check_if_value_is_css_var({:.40?}) panicked: {}",
                input,
                r.unwrap_err()
            );
        }
    }

    // --- parse_declaration_resilient / parse_css_declaration --------------

    #[test]
    fn parse_css_declaration_valid_minimal() {
        let km = key_map();
        let mut warnings = Vec::new();
        let mut declarations = Vec::new();
        let r = parse_css_declaration(
            "width",
            "100px",
            loc(0, 0),
            &km,
            &mut warnings,
            &mut declarations,
        );
        assert_eq!(r, Ok(()));
        assert_eq!(declarations.len(), 1);
        assert!(matches!(declarations[0], CssDeclaration::Static(_)));
        assert!(warnings.is_empty());
    }

    #[test]
    fn parse_css_declaration_unknown_key_is_downgraded_to_a_warning() {
        let km = key_map();
        let mut warnings = Vec::new();
        let mut declarations = Vec::new();
        // Documented contract: an unknown key is a warning, not a hard error, so the
        // caller can keep processing the rest of the block.
        let r = parse_css_declaration(
            "definitely-not-a-property",
            "1",
            loc(0, 0),
            &km,
            &mut warnings,
            &mut declarations,
        );
        assert_eq!(r, Ok(()));
        assert!(declarations.is_empty());
        assert_eq!(warnings.len(), 1);
        assert!(matches!(
            warnings[0].warning,
            CssParseWarnMsgInner::UnsupportedKeyValuePair { .. }
        ));
    }

    #[test]
    fn parse_css_declaration_bad_value_is_a_hard_error() {
        let km = key_map();
        let mut warnings = Vec::new();
        let mut declarations = Vec::new();
        let r = parse_css_declaration(
            "width",
            "definitely-not-a-length",
            loc(0, 0),
            &km,
            &mut warnings,
            &mut declarations,
        );
        assert!(r.is_err(), "a known key with an unparseable value must error");
        assert!(declarations.is_empty());
    }

    #[test]
    fn parse_declaration_resilient_var_on_shorthand_is_rejected() {
        let km = key_map();
        // `margin` is a shorthand; `var()` on it is ambiguous and must be refused.
        let r = parse_declaration_resilient("margin", "var(--m)", loc(0, 0), &km);
        assert!(
            matches!(r, Err(CssParseErrorInner::VarOnShorthandProperty { .. })),
            "expected VarOnShorthandProperty, got {r:?}"
        );
    }

    #[test]
    fn parse_declaration_resilient_var_on_normal_property_becomes_dynamic() {
        let km = key_map();
        let decls = parse_declaration_resilient("width", "var(--w, 100px)", loc(0, 0), &km)
            .expect("var() on a non-shorthand property should parse");
        assert_eq!(decls.len(), 1);
        match &decls[0] {
            CssDeclaration::Dynamic(DynamicCssProperty { dynamic_id, .. }) => {
                assert_eq!(dynamic_id.as_str(), "w");
            }
            other => panic!("expected a Dynamic declaration, got {other:?}"),
        }
    }

    #[test]
    fn parse_declaration_resilient_hostile_key_value_pairs_never_panic() {
        let km = key_map();
        let long = "x".repeat(100_000);
        let mut inputs: Vec<&str> = HOSTILE.to_vec();
        inputs.push(&long);

        for key in &inputs {
            for value in &inputs {
                let r = catch(|| parse_declaration_resilient(key, value, loc(0, 0), &km).is_ok());
                assert!(
                    r.is_ok(),
                    "parse_declaration_resilient({:.30?}, {:.30?}) panicked: {}",
                    key,
                    value,
                    r.unwrap_err()
                );
            }
        }
    }

    #[test]
    fn parse_declaration_resilient_empty_key_is_an_unknown_property() {
        let km = key_map();
        assert!(matches!(
            parse_declaration_resilient("", "", loc(0, 0), &km),
            Err(CssParseErrorInner::UnknownPropertyKey("", ""))
        ));
    }

    // =====================================================================
    // numeric -> overflow / NaN / saturation / limits
    // =====================================================================

    #[test]
    fn get_line_column_from_error_representative_values() {
        let css = "div {\n    width: 100px;\n}";
        // Position 0 and 1 both clamp to offset 0 via `saturating_sub(1)`.
        assert_eq!(
            ErrorLocation { original_pos: 0 }.get_line_column_from_error(css),
            (0, 0)
        );
        let (line, _col) = ErrorLocation { original_pos: 12 }.get_line_column_from_error(css);
        assert_eq!(line, 2, "byte 11 is on the second line");
    }

    #[test]
    fn get_line_column_from_error_empty_css_does_not_panic() {
        let r = catch(|| ErrorLocation { original_pos: 0 }.get_line_column_from_error(""));
        assert_eq!(r, Ok((0, 0)));
    }

    /// The column arithmetic (`error_location - total_characters.saturating_sub(2)`)
    /// is an unchecked subtraction; newline-heavy inputs are the worst case for it.
    #[test]
    fn get_line_column_from_error_newline_heavy_input_does_not_underflow() {
        let newlines = "\n".repeat(1_000);
        let crlf = "\r\n".repeat(1_000);
        for css in [newlines.as_str(), crlf.as_str()] {
            for pos in [1_usize, 2, 3, 500, css.len()] {
                let r = catch(|| ErrorLocation { original_pos: pos }.get_line_column_from_error(css));
                assert!(
                    r.is_ok(),
                    "get_line_column_from_error(pos={pos}) underflowed/panicked: {}",
                    r.unwrap_err()
                );
            }
        }
    }

    /// BUG (red): `css_string[0..error_location]` is an unchecked slice. An
    /// `original_pos` past the end of the string -- trivially reachable, since
    /// `ErrorLocation` is a `pub` struct with a `pub` field and the method takes an
    /// arbitrary `&str` -- panics with "byte index out of bounds" instead of
    /// clamping.
    #[test]
    fn get_line_column_from_error_out_of_range_pos_does_not_panic() {
        let css = "div {}";
        for pos in [css.len() + 2, 999, usize::MAX] {
            if let Err(msg) =
                catch(|| ErrorLocation { original_pos: pos }.get_line_column_from_error(css))
            {
                panic!(
                    "BUG: get_line_column_from_error panicked for original_pos={pos} on a \
                     {}-byte string (unchecked `css_string[0..error_location]` slice); it \
                     should clamp instead: {msg}",
                    css.len()
                );
            }
        }
    }

    /// BUG (red): the same unchecked slice also ignores UTF-8 char boundaries.
    /// `original_pos = css.len()` is exactly what `get_error_location` records at
    /// `Token::EndOfStream`, so a stylesheet whose last character is multi-byte
    /// makes `original_pos - 1` land *inside* that character and the slice panics
    /// with "byte index is not a char boundary".
    #[test]
    fn get_line_column_from_error_at_end_of_unicode_css_does_not_panic() {
        // "a\u{1F600}" is 5 bytes; the only char boundaries are 0, 1 and 5.
        let css = "a\u{1F600}";
        assert_eq!(css.len(), 5);
        let pos = css.len(); // -> error_location == 4, which is mid-emoji
        if let Err(msg) =
            catch(|| ErrorLocation { original_pos: pos }.get_line_column_from_error(css))
        {
            panic!(
                "BUG: get_line_column_from_error panicked at end-of-stream (original_pos={pos}) \
                 because the CSS ends with a multi-byte char and `original_pos - 1` splits it: \
                 {msg}"
            );
        }
    }

    // =====================================================================
    // getters / predicates -> invariants
    // =====================================================================

    #[test]
    fn get_error_string_returns_the_trimmed_slice_between_start_and_end() {
        let css = "div { width: 100px; }";
        let err = CssParseError {
            css_string: css,
            error: CssParseErrorInner::MalformedCss,
            location: loc(6, 18),
        };
        assert_eq!(err.get_error_string(), "width: 100px");

        // An empty range yields an empty string rather than panicking.
        let err = CssParseError {
            css_string: css,
            error: CssParseErrorInner::MalformedCss,
            location: loc(0, 0),
        };
        assert_eq!(err.get_error_string(), "");
    }

    /// BUG (red): `get_error_string` slices `&self.css_string[start..end]` with no
    /// validation. A location that is out of range, reversed, or lands inside a
    /// multi-byte char panics. `CssParseError` is `pub` with `pub` fields (and is
    /// rebuilt from an owned value by `CssParseErrorOwned::to_shared`, where nothing
    /// re-checks the location against the string), so this is reachable.
    #[test]
    fn get_error_string_invalid_location_does_not_panic() {
        let cases: [(&str, ErrorLocationRange, &str); 3] = [
            ("div", loc(0, 99), "end past the end of the string"),
            ("div", loc(2, 1), "reversed range (start > end)"),
            ("a\u{1F600}", loc(0, 4), "end inside a multi-byte char"),
        ];
        for (css, location, why) in cases {
            let err = CssParseError {
                css_string: css,
                error: CssParseErrorInner::MalformedCss,
                location,
            };
            if let Err(msg) = catch(|| err.get_error_string().to_string()) {
                panic!(
                    "BUG: get_error_string panicked on an invalid location ({why}) instead of \
                     returning an empty/clamped slice: {msg}"
                );
            }
        }
    }

    // --- serializer: Display for CssParseError ----------------------------

    #[test]
    fn display_of_css_parse_error_is_non_empty_and_well_formed() {
        let css = "div { width: 100px; }";
        let err = CssParseError {
            css_string: css,
            error: CssParseErrorInner::MalformedCss,
            location: loc(6, 18),
        };
        let s = format!("{err}");
        assert!(!s.is_empty());
        assert!(s.contains("start: line"), "missing start location: {s}");
        assert!(s.contains("end: line"), "missing end location: {s}");
        assert!(s.contains("width: 100px"), "missing offending text: {s}");
        assert!(s.contains("Malformed Css"), "missing reason: {s}");
    }

    #[test]
    fn display_of_css_parse_error_on_zero_value_does_not_panic() {
        let err = CssParseError {
            css_string: "",
            error: CssParseErrorInner::UnclosedBlock,
            location: ErrorLocationRange::default(),
        };
        let r = catch(|| format!("{err}"));
        match r {
            Ok(s) => assert!(!s.is_empty(), "Display produced an empty string"),
            Err(msg) => panic!("Display panicked on a zero-valued CssParseError: {msg}"),
        }
    }

    /// BUG (red): `Display for CssParseError` calls both `get_line_column_from_error`
    /// and `get_error_string`, so it inherits their unchecked slicing. Formatting the
    /// error for a stylesheet that ends in a multi-byte character panics -- i.e. the
    /// *error reporting path* itself crashes on non-ASCII CSS.
    #[test]
    fn display_of_css_parse_error_with_unicode_css_does_not_panic() {
        let css = "p{}\u{1F600}"; // 7 bytes; boundaries at 0..=3 and 7
        let err = CssParseError {
            css_string: css,
            error: CssParseErrorInner::MalformedCss,
            location: loc(0, css.len()),
        };
        if let Err(msg) = catch(|| format!("{err}")) {
            panic!(
                "BUG: Display for CssParseError panicked while formatting an error whose CSS \
                 ends in a multi-byte char (unchecked slicing in get_line_column_from_error / \
                 get_error_string): {msg}"
            );
        }
    }

    #[test]
    fn display_of_error_and_warning_inners_is_never_empty() {
        let km = key_map();
        let margin = CombinedCssPropertyType::from_str("margin", &km).expect("margin is a shorthand");

        let inners: Vec<CssParseErrorInner<'_>> = vec![
            CssParseErrorInner::ParseError(CssSyntaxError::UnknownToken(CssSyntaxErrorPos {
                row: usize::MAX,
                col: usize::MAX,
            })),
            CssParseErrorInner::UnclosedBlock,
            CssParseErrorInner::MalformedCss,
            CssParseErrorInner::DynamicCssParseError(DynamicCssParseError::InvalidBraceContents(
                "",
            )),
            CssParseErrorInner::PseudoSelectorParseError(
                CssPseudoSelectorParseError::EmptyNthChild,
            ),
            CssParseErrorInner::NodeTypeTag(NodeTypeTagParseError::Invalid("")),
            CssParseErrorInner::UnknownPropertyKey("", ""),
            CssParseErrorInner::VarOnShorthandProperty {
                key: margin,
                value: "",
            },
        ];

        for inner in &inners {
            let s = format!("{inner}");
            assert!(!s.is_empty(), "empty Display for {inner:?}");
        }

        let warnings = vec![
            CssParseWarnMsgInner::UnsupportedKeyValuePair { key: "", value: "" },
            CssParseWarnMsgInner::ParseError(CssParseErrorInner::MalformedCss),
            CssParseWarnMsgInner::SkippedRule {
                selector: None,
                error: CssParseErrorInner::UnclosedBlock,
            },
            CssParseWarnMsgInner::SkippedDeclaration {
                key: "",
                value: "",
                error: CssParseErrorInner::MalformedCss,
            },
            CssParseWarnMsgInner::MalformedStructure { message: "" },
        ];
        for w in &warnings {
            assert!(!format!("{w}").is_empty(), "empty Display for {w:?}");
        }
    }

    // =====================================================================
    // round-trip -> to_contained() == to_shared()
    // =====================================================================

    #[test]
    fn css_pseudo_selector_parse_error_round_trips() {
        let cases = vec![
            CssPseudoSelectorParseError::EmptyNthChild,
            CssPseudoSelectorParseError::UnknownSelector("blah", None),
            CssPseudoSelectorParseError::UnknownSelector("blah", Some("3")),
            CssPseudoSelectorParseError::InvalidNthChildPattern("2x+1"),
            CssPseudoSelectorParseError::InvalidNthChild(
                "x".parse::<u32>().unwrap_err(),
            ),
            CssPseudoSelectorParseError::InvalidNthChild(
                "99999999999999999999".parse::<u32>().unwrap_err(),
            ),
            // Empty / extreme payloads.
            CssPseudoSelectorParseError::UnknownSelector("", Some("")),
        ];
        for case in &cases {
            assert_eq!(
                &case.to_contained().to_shared(),
                case,
                "round-trip changed the value"
            );
        }
    }

    #[test]
    fn dynamic_css_parse_error_round_trips() {
        let simple = DynamicCssParseError::InvalidBraceContents("--x, blue");
        assert_eq!(&simple.to_contained().to_shared(), &simple);

        let empty = DynamicCssParseError::InvalidBraceContents("");
        assert_eq!(&empty.to_contained().to_shared(), &empty);

        // A real `CssParsingError` from the property parser. The nested error has its
        // own owned/shared pair, so compare via `Display` (semantics) plus the variant.
        let km = key_map();
        let width = CssPropertyType::from_str("width", &km).expect("width is a property");
        let inner = parse_css_property(width, "definitely-not-a-length")
            .expect_err("an invalid length must fail to parse");
        let wrapped = DynamicCssParseError::UnexpectedValue(inner);
        let round_tripped = wrapped.to_contained();
        let back = round_tripped.to_shared();
        assert!(matches!(back, DynamicCssParseError::UnexpectedValue(_)));
        assert_eq!(
            format!("{back}"),
            format!("{wrapped}"),
            "round-trip lost information from the nested CssParsingError"
        );
    }

    #[test]
    fn css_parse_error_inner_round_trips_for_every_variant() {
        let km = key_map();
        let margin =
            CombinedCssPropertyType::from_str("margin", &km).expect("margin is a shorthand");

        let cases = vec![
            CssParseErrorInner::ParseError(CssSyntaxError::UnexpectedEndOfStream(
                CssSyntaxErrorPos { row: 0, col: 0 },
            )),
            // Extreme numeric payloads must survive the FFI hop unchanged.
            CssParseErrorInner::ParseError(CssSyntaxError::InvalidAdvance(
                CssSyntaxInvalidAdvance {
                    expected: isize::MIN,
                    total: usize::MAX,
                    pos: CssSyntaxErrorPos {
                        row: usize::MAX,
                        col: usize::MAX,
                    },
                },
            )),
            CssParseErrorInner::ParseError(CssSyntaxError::UnsupportedToken(CssSyntaxErrorPos {
                row: 3,
                col: 7,
            })),
            CssParseErrorInner::UnclosedBlock,
            CssParseErrorInner::MalformedCss,
            CssParseErrorInner::DynamicCssParseError(DynamicCssParseError::InvalidBraceContents(
                "--x",
            )),
            CssParseErrorInner::PseudoSelectorParseError(
                CssPseudoSelectorParseError::EmptyNthChild,
            ),
            CssParseErrorInner::NodeTypeTag(NodeTypeTagParseError::Invalid("notatag")),
            CssParseErrorInner::UnknownPropertyKey("key", "value"),
            CssParseErrorInner::UnknownPropertyKey("", ""),
            CssParseErrorInner::VarOnShorthandProperty {
                key: margin,
                value: "var(--m)",
            },
        ];

        for case in &cases {
            assert_eq!(
                &case.to_contained().to_shared(),
                case,
                "round-trip changed the value for {case:?}"
            );
        }
    }

    #[test]
    fn css_parse_error_round_trips_including_unicode_payloads() {
        let css = "div { width: \u{1F600}; }";
        let err = CssParseError {
            css_string: css,
            error: CssParseErrorInner::UnknownPropertyKey("k\u{1F600}", "v\u{4E2D}"),
            location: loc(1, 2),
        };
        assert_eq!(err.to_contained().to_shared(), err);

        // Zero value.
        let err = CssParseError {
            css_string: "",
            error: CssParseErrorInner::MalformedCss,
            location: ErrorLocationRange::default(),
        };
        assert_eq!(err.to_contained().to_shared(), err);
    }

    #[test]
    fn css_path_parse_error_round_trips_for_every_variant() {
        let cases = vec![
            CssPathParseError::EmptyPath,
            CssPathParseError::InvalidTokenEncountered("{"),
            CssPathParseError::InvalidTokenEncountered(""),
            CssPathParseError::UnexpectedEndOfStream("div"),
            CssPathParseError::SyntaxError(CssSyntaxError::UnknownToken(CssSyntaxErrorPos {
                row: usize::MAX,
                col: 0,
            })),
            CssPathParseError::NodeTypeTag(NodeTypeTagParseError::Invalid("notatag")),
            CssPathParseError::PseudoSelectorParseError(
                CssPseudoSelectorParseError::InvalidNthChildPattern("2x"),
            ),
        ];
        for case in &cases {
            assert_eq!(
                &case.to_contained().to_shared(),
                case,
                "round-trip changed the value for {case:?}"
            );
        }
    }

    #[test]
    fn css_parse_warn_msg_round_trips_for_every_variant() {
        let inners = vec![
            CssParseWarnMsgInner::UnsupportedKeyValuePair {
                key: "foo",
                value: "bar",
            },
            CssParseWarnMsgInner::UnsupportedKeyValuePair { key: "", value: "" },
            CssParseWarnMsgInner::ParseError(CssParseErrorInner::MalformedCss),
            CssParseWarnMsgInner::SkippedRule {
                selector: None,
                error: CssParseErrorInner::UnclosedBlock,
            },
            CssParseWarnMsgInner::SkippedRule {
                selector: Some("div"),
                error: CssParseErrorInner::MalformedCss,
            },
            CssParseWarnMsgInner::SkippedDeclaration {
                key: "width",
                value: "\u{1F600}",
                error: CssParseErrorInner::MalformedCss,
            },
            CssParseWarnMsgInner::MalformedStructure {
                message: "unclosed",
            },
        ];

        for inner in &inners {
            assert_eq!(
                &inner.to_contained().to_shared(),
                inner,
                "round-trip changed the value for {inner:?}"
            );

            let msg = CssParseWarnMsg {
                warning: inner.clone(),
                location: loc(7, 42),
            };
            let back = msg.to_contained();
            let back = back.to_shared();
            assert_eq!(back, msg, "CssParseWarnMsg round-trip changed the value");
            assert_eq!(back.location, loc(7, 42), "location was not preserved");
        }
    }

    #[test]
    fn unparsed_css_rule_block_round_trips() {
        let mut declarations = BTreeMap::new();
        declarations.insert("width", ("100px", loc(1, 2)));
        declarations.insert("color", ("\u{1F600}", loc(3, 4)));

        let block = UnparsedCssRuleBlock {
            path: CssPath {
                selectors: vec![
                    CssPathSelector::Global,
                    CssPathSelector::Class("btn".to_string().into()),
                ]
                .into(),
            },
            declarations,
            // NB: deliberately no MinMaxRange condition here -- those store `f32::NAN`
            // as the "absent bound" sentinel, so they are not equal to themselves under
            // the derived `PartialEq` (see dynamic_selector.rs).
            conditions: vec![DynamicSelector::Media(MediaType::Screen)],
        };

        assert_eq!(block.to_contained().to_shared(), block);

        // Empty instance.
        let empty = UnparsedCssRuleBlock {
            path: CssPath {
                selectors: Vec::new().into(),
            },
            declarations: BTreeMap::new(),
            conditions: Vec::new(),
        };
        assert_eq!(empty.to_contained().to_shared(), empty);
    }

    // =====================================================================
    // other -> new_from_str / new_from_str_inner / css_blocks_to_stylesheet
    // =====================================================================

    #[test]
    fn new_from_str_valid_minimal() {
        let (css, warnings) = new_from_str("div { width: 100px; }");
        assert_eq!(css.rules.len(), 1);
        assert_eq!(css.rules.as_slice()[0].declarations.len(), 1);
        assert!(warnings.is_empty(), "unexpected warnings: {warnings:?}");
    }

    #[test]
    fn new_from_str_empty_input_yields_an_empty_stylesheet() {
        let (css, warnings) = new_from_str("");
        assert_eq!(css.rules.len(), 0);
        assert!(warnings.is_empty());

        let (css, _warnings) = new_from_str("   \n\t  ");
        assert_eq!(css.rules.len(), 0);
    }

    #[test]
    fn new_from_str_unclosed_block_warns_instead_of_failing() {
        let (css, warnings) = new_from_str("div { width: 100px;");
        assert_eq!(css.rules.len(), 0, "an unclosed block emits no rules");
        assert!(
            warnings.iter().any(|w| matches!(
                w.warning,
                CssParseWarnMsgInner::MalformedStructure { .. }
            )),
            "expected a MalformedStructure warning, got {warnings:?}"
        );
    }

    #[test]
    fn new_from_str_unknown_property_is_a_warning_not_a_dropped_rule() {
        let (css, warnings) = new_from_str("div { definitely-not-a-property: 1; width: 10px; }");
        assert_eq!(css.rules.len(), 1);
        // The unknown key is skipped but the valid declaration survives.
        assert_eq!(css.rules.as_slice()[0].declarations.len(), 1);
        assert!(!warnings.is_empty(), "the unknown key should have warned");
    }

    /// `new_from_str` documents "Never panics" -- hold it to that.
    #[test]
    fn new_from_str_hostile_input_never_panics() {
        let long_rule = "div { width: 100px; }".repeat(2_000);
        let deep_nesting = format!("{}{}", "div {".repeat(500), "}".repeat(500));
        let unbalanced_open = "{".repeat(5_000);
        let unbalanced_close = "}".repeat(5_000);
        let long_selector = format!("{} {{ width: 1px; }}", "div ".repeat(10_000));

        let mut inputs: Vec<&str> = HOSTILE.to_vec();
        inputs.push(&long_rule);
        inputs.push(&deep_nesting);
        inputs.push(&unbalanced_open);
        inputs.push(&unbalanced_close);
        inputs.push(&long_selector);
        inputs.push("div { width: \u{1F600}; }");
        inputs.push("\u{1F600} { \u{4E2D}: \u{6587}; }");
        inputs.push("div { width: 100px; /* unterminated");
        inputs.push("@media (min-width: 800px) { div { width: 1px; } }");
        inputs.push("@theme(dark) { div { width: 1px; } }");
        inputs.push("@lang(\"de-DE\") { div { width: 1px; } }");
        inputs.push("@container sidebar (min-width: 400px) { div { width: 1px; } }");
        inputs.push("@definitely-not-an-at-rule x { div { width: 1px; } }");
        inputs.push(".a { .b { :hover { width: 1px; } } }");
        inputs.push("div[data-x=\"y\"] { width: 1px; }");
        inputs.push("div[ { width: 1px; }");
        inputs.push("a, b, , c { width: 1px; }");
        inputs.push("div:nth-child(999999999999) { width: 1px; }");

        for input in inputs {
            let r = catch(|| {
                let (css, warnings) = new_from_str(input);
                (css.rules.len(), warnings.len())
            });
            assert!(
                r.is_ok(),
                "new_from_str({:.60?}) panicked despite the `Never panics` contract: {}",
                input,
                r.unwrap_err()
            );
        }
    }

    #[test]
    fn new_from_str_at_rules_attach_conditions_to_nested_rules() {
        let (css, _warnings) = new_from_str("@media screen { div { width: 1px; } }");
        assert_eq!(css.rules.len(), 1);
        let rule = &css.rules.as_slice()[0];
        assert!(
            rule.conditions
                .as_slice()
                .contains(&DynamicSelector::Media(MediaType::Screen)),
            "the @media condition was not attached: {:?}",
            rule.conditions.as_slice()
        );
    }

    #[test]
    fn new_from_str_comma_separated_selectors_emit_one_rule_each() {
        let (css, _warnings) = new_from_str("div, p { width: 1px; }");
        assert_eq!(css.rules.len(), 2, "each selector in the list gets its own rule");
    }

    #[test]
    fn new_from_str_inner_matches_new_from_str() {
        let css_string = "div { width: 100px; }";
        let mut tokenizer = Tokenizer::new(css_string);
        let (rules, warnings) = new_from_str_inner(css_string, &mut tokenizer);
        assert_eq!(rules.len(), 1);
        assert!(warnings.is_empty());
    }

    #[test]
    fn get_error_location_tracks_the_tokenizer_position() {
        let css_string = "div { width: 100px; }";
        let mut tokenizer = Tokenizer::new(css_string);
        assert_eq!(get_error_location(&tokenizer).original_pos, 0);

        let _ = tokenizer.parse_next();
        let after = get_error_location(&tokenizer).original_pos;
        assert!(after > 0, "the tokenizer position did not advance");
        assert!(
            after <= css_string.len(),
            "the tokenizer position ran past the end of the input"
        );

        // Position on an empty document is 0 and must not panic.
        let empty = Tokenizer::new("");
        assert_eq!(get_error_location(&empty).original_pos, 0);
    }

    #[test]
    fn css_blocks_to_stylesheet_parses_known_keys_and_warns_on_unknown_ones() {
        let css_string = "div { width: 100px; }";

        let mut declarations = BTreeMap::new();
        declarations.insert("width", ("100px", loc(6, 18)));
        let good = UnparsedCssRuleBlock {
            path: CssPath {
                selectors: vec![CssPathSelector::Global].into(),
            },
            declarations,
            conditions: Vec::new(),
        };

        let mut declarations = BTreeMap::new();
        declarations.insert("definitely-not-a-property", ("1", loc(0, 1)));
        let bad = UnparsedCssRuleBlock {
            path: CssPath {
                selectors: vec![CssPathSelector::Global].into(),
            },
            declarations,
            conditions: Vec::new(),
        };

        let (rules, warnings) = css_blocks_to_stylesheet(vec![good, bad], css_string);
        assert_eq!(rules.len(), 2, "both blocks are emitted");
        assert_eq!(rules[0].declarations.len(), 1);
        assert_eq!(rules[1].declarations.len(), 0, "the unknown key is dropped");
        assert_eq!(warnings.len(), 1, "the unknown key produced exactly one warning");
        assert!(matches!(
            warnings[0].warning,
            CssParseWarnMsgInner::SkippedDeclaration { .. }
        ));
    }

    #[test]
    fn css_blocks_to_stylesheet_empty_input_is_empty_output() {
        let (rules, warnings) = css_blocks_to_stylesheet(Vec::new(), "");
        assert!(rules.is_empty());
        assert!(warnings.is_empty());
    }
}
