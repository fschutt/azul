//! High-level types and functions related to CSS parsing
use alloc::{collections::BTreeMap, string::ToString, vec::Vec};
use core::{fmt, num::ParseIntError};

pub use azul_simplecss::Error as SimplecssError;
use azul_simplecss::Tokenizer;

/// FFI-safe position of a CSS syntax error.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct CssSyntaxErrorPos {
    pub row: usize,
    pub col: usize,
}

impl From<azul_simplecss::ErrorPos> for CssSyntaxErrorPos {
    fn from(p: azul_simplecss::ErrorPos) -> Self {
        CssSyntaxErrorPos { row: p.row, col: p.col }
    }
}

/// FFI-safe wrapper for invalid advance details in CSS syntax errors.
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(C)]
pub struct CssSyntaxInvalidAdvance {
    pub expected: isize,
    pub total: usize,
    pub pos: CssSyntaxErrorPos,
}

/// FFI-safe CSS syntax error type, mirrors azul_simplecss::Error.
#[derive(Debug, Clone, Copy, PartialEq)]
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
            SimplecssError::UnexpectedEndOfStream(pos) => CssSyntaxError::UnexpectedEndOfStream(pos.into()),
            SimplecssError::InvalidAdvance { expected, total, pos } => CssSyntaxError::InvalidAdvance(CssSyntaxInvalidAdvance { expected, total, pos: pos.into() }),
            SimplecssError::UnsupportedToken(pos) => CssSyntaxError::UnsupportedToken(pos.into()),
            SimplecssError::UnknownToken(pos) => CssSyntaxError::UnknownToken(pos.into()),
        }
    }
}

pub use crate::props::property::CssParsingError;
use crate::{
    corety::{AzString, OptionString},
    css::{
        Css, CssDeclaration, CssNthChildSelector, CssPath, CssPathPseudoSelector, CssPathSelector,
        CssRuleBlock, DynamicCssProperty, NodeTypeTag, NodeTypeTagParseError,
        NodeTypeTagParseErrorOwned, Stylesheet,
    },
    dynamic_selector::{
        DynamicSelector, DynamicSelectorVec, LanguageCondition, MediaType, MinMaxRange,
        OrientationType, OsCondition,
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

/// Owned version of CssParseError, without references.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct CssParseErrorOwned {
    pub css_string: AzString,
    pub error: CssParseErrorInnerOwned,
    pub location: ErrorLocationRange,
}

impl<'a> CssParseError<'a> {
    pub fn to_contained(&self) -> CssParseErrorOwned {
        CssParseErrorOwned {
            css_string: self.css_string.to_string().into(),
            error: self.error.to_contained(),
            location: self.location.clone(),
        }
    }
}

impl CssParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssParseError<'a> {
        CssParseError {
            css_string: self.css_string.as_str(),
            error: self.error.to_shared(),
            location: self.location.clone(),
        }
    }
}

impl<'a> CssParseError<'a> {
    /// Returns the string between the (start, end) location
    pub fn get_error_string(&self) -> &'a str {
        let (start, end) = (self.location.start.original_pos, self.location.end.original_pos);
        let s = &self.css_string[start..end];
        s.trim()
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
    /// ambigouus and degrade performance - for example `margin: var(--blah)` would be ambigouus
    /// because it's not clear when setting the variable, whether all sides should be set,
    /// instead, you have to use `margin-top: var(--blah)`, `margin-bottom: var(--baz)` in order
    /// to work around this limitation.
    VarOnShorthandProperty {
        key: CombinedCssPropertyType,
        value: &'a str,
    },
}

/// Wrapper for UnknownPropertyKey error.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct UnknownPropertyKeyError {
    pub key: AzString,
    pub value: AzString,
}

/// Wrapper for VarOnShorthandProperty error.
#[derive(Debug, Clone, PartialEq)]
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

impl<'a> CssParseErrorInner<'a> {
    pub fn to_contained(&self) -> CssParseErrorInnerOwned {
        match self {
            CssParseErrorInner::ParseError(e) => CssParseErrorInnerOwned::ParseError(e.clone()),
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
                CssParseErrorInnerOwned::UnknownPropertyKey(UnknownPropertyKeyError { key: a.to_string().into(), value: b.to_string().into() })
            }
            CssParseErrorInner::VarOnShorthandProperty { key, value } => {
                CssParseErrorInnerOwned::VarOnShorthandProperty(VarOnShorthandPropertyError {
                    key: key.clone(),
                    value: value.to_string().into(),
                })
            }
        }
    }
}

impl CssParseErrorInnerOwned {
    pub fn to_shared<'a>(&'a self) -> CssParseErrorInner<'a> {
        match self {
            CssParseErrorInnerOwned::ParseError(e) => CssParseErrorInner::ParseError(e.clone()),
            CssParseErrorInnerOwned::UnclosedBlock => CssParseErrorInner::UnclosedBlock,
            CssParseErrorInnerOwned::MalformedCss => CssParseErrorInner::MalformedCss,
            CssParseErrorInnerOwned::DynamicCssParseError(e) => {
                CssParseErrorInner::DynamicCssParseError(e.to_shared())
            }
            CssParseErrorInnerOwned::PseudoSelectorParseError(e) => {
                CssParseErrorInner::PseudoSelectorParseError(e.to_shared())
            }
            CssParseErrorInnerOwned::NodeTypeTag(e) => {
                CssParseErrorInner::NodeTypeTag(e.to_shared())
            }
            CssParseErrorInnerOwned::UnknownPropertyKey(e) => {
                CssParseErrorInner::UnknownPropertyKey(e.key.as_str(), e.value.as_str())
            }
            CssParseErrorInnerOwned::VarOnShorthandProperty(e) => {
                CssParseErrorInner::VarOnShorthandProperty {
                    key: e.key.clone(),
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

impl<'a> From<CssSyntaxError> for CssParseErrorInner<'a> {
    fn from(e: CssSyntaxError) -> Self {
        CssParseErrorInner::ParseError(e)
    }
}

impl<'a> From<SimplecssError> for CssParseErrorInner<'a> {
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

impl<'a> From<ParseIntError> for CssPseudoSelectorParseError<'a> {
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
        let format_str = match value {
            Some(v) => format!("{}({})", selector, v),
            None => format!("{}", selector),
        };
        format!("Invalid or unknown CSS pseudo-selector: ':{}'", format_str)
    },
    InvalidNthChildPattern(selector) => format!(
        "Invalid pseudo-selector :{} - value has to be a \
        number, \"even\" or \"odd\" or a pattern such as \"2n+3\"", selector
    ),
    InvalidNthChild(e) => format!("Invalid :nth-child pseudo-selector: ':{}'", e),
}}

/// Wrapper for UnknownSelector error.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct UnknownSelectorError {
    pub selector: AzString,
    pub suggestion: OptionString,
}

#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum CssPseudoSelectorParseErrorOwned {
    EmptyNthChild,
    UnknownSelector(UnknownSelectorError),
    InvalidNthChildPattern(AzString),
    InvalidNthChild(crate::props::basic::error::ParseIntError),
}

impl<'a> CssPseudoSelectorParseError<'a> {
    pub fn to_contained(&self) -> CssPseudoSelectorParseErrorOwned {
        match self {
            CssPseudoSelectorParseError::EmptyNthChild => {
                CssPseudoSelectorParseErrorOwned::EmptyNthChild
            }
            CssPseudoSelectorParseError::UnknownSelector(a, b) => {
                CssPseudoSelectorParseErrorOwned::UnknownSelector(UnknownSelectorError {
                    selector: a.to_string().into(),
                    suggestion: b.map(|s| AzString::from(s.to_string())).into(),
                })
            }
            CssPseudoSelectorParseError::InvalidNthChildPattern(s) => {
                CssPseudoSelectorParseErrorOwned::InvalidNthChildPattern(s.to_string().into())
            }
            CssPseudoSelectorParseError::InvalidNthChild(e) => {
                CssPseudoSelectorParseErrorOwned::InvalidNthChild(e.clone().into())
            }
        }
    }
}

impl CssPseudoSelectorParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssPseudoSelectorParseError<'a> {
        match self {
            CssPseudoSelectorParseErrorOwned::EmptyNthChild => {
                CssPseudoSelectorParseError::EmptyNthChild
            }
            CssPseudoSelectorParseErrorOwned::UnknownSelector(e) => {
                CssPseudoSelectorParseError::UnknownSelector(e.selector.as_str(), e.suggestion.as_ref().map(|s| s.as_str()))
            }
            CssPseudoSelectorParseErrorOwned::InvalidNthChildPattern(s) => {
                CssPseudoSelectorParseError::InvalidNthChildPattern(s)
            }
            CssPseudoSelectorParseErrorOwned::InvalidNthChild(e) => {
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

impl<'a> DynamicCssParseError<'a> {
    pub fn to_contained(&self) -> DynamicCssParseErrorOwned {
        match self {
            DynamicCssParseError::InvalidBraceContents(s) => {
                DynamicCssParseErrorOwned::InvalidBraceContents(s.to_string().into())
            }
            DynamicCssParseError::UnexpectedValue(e) => {
                DynamicCssParseErrorOwned::UnexpectedValue(e.to_contained())
            }
        }
    }
}

impl DynamicCssParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> DynamicCssParseError<'a> {
        match self {
            DynamicCssParseErrorOwned::InvalidBraceContents(s) => {
                DynamicCssParseError::InvalidBraceContents(s)
            }
            DynamicCssParseErrorOwned::UnexpectedValue(e) => {
                DynamicCssParseError::UnexpectedValue(e.to_shared())
            }
        }
    }
}

/// "selector" contains the actual selector such as "nth-child" while "value" contains
/// an optional value - for example "nth-child(3)" would be: selector: "nth-child", value: "3".
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

/// Parses the inner value of the `:nth-child` selector, including numbers and patterns.
///
/// I.e.: `"2n+3"` -> `Pattern { repeat: 2, offset: 3 }`
fn parse_nth_child_selector<'a>(
    value: &'a str,
) -> Result<CssNthChildSelector, CssPseudoSelectorParseError<'a>> {
    let value = value.trim();

    if value.is_empty() {
        return Err(CssPseudoSelectorParseError::EmptyNthChild);
    }

    if let Ok(number) = value.parse::<u32>() {
        return Ok(CssNthChildSelector::Number(number));
    }

    // If the value is not a number
    match value.as_ref() {
        "even" => Ok(CssNthChildSelector::Even),
        "odd" => Ok(CssNthChildSelector::Odd),
        other => parse_nth_child_pattern(value),
    }
}

/// Parses the pattern between the braces of a "nth-child" (such as "2n+3").
fn parse_nth_child_pattern<'a>(
    value: &'a str,
) -> Result<CssNthChildSelector, CssPseudoSelectorParseError<'a>> {
    use crate::css::CssNthChildPattern;

    let value = value.trim();

    if value.is_empty() {
        return Err(CssPseudoSelectorParseError::EmptyNthChild);
    }

    // TODO: Test for "+"
    let repeat = value
        .split("n")
        .next()
        .ok_or(CssPseudoSelectorParseError::InvalidNthChildPattern(value))?
        .trim()
        .parse::<u32>()?;

    // In a "2n+3" form, the first .next() yields the "2n", the second .next() yields the "3"
    let mut offset_iterator = value.split("+");

    // has to succeed, since the string is verified to not be empty
    offset_iterator.next().unwrap();

    let offset = match offset_iterator.next() {
        Some(offset_string) => {
            let offset_string = offset_string.trim();
            if offset_string.is_empty() {
                return Err(CssPseudoSelectorParseError::InvalidNthChildPattern(value));
            } else {
                offset_string.parse::<u32>()?
            }
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
    pub fn get_line_column_from_error(&self, css_string: &str) -> (usize, usize) {
        let error_location = self.original_pos.saturating_sub(1);
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

impl<'a> fmt::Display for CssParseError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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

pub fn new_from_str<'a>(css_string: &'a str) -> (Css, Vec<CssParseWarnMsg<'a>>) {
    let mut tokenizer = Tokenizer::new(css_string);
    let (stylesheet, warnings) = match new_from_str_inner(css_string, &mut tokenizer) {
        Ok((stylesheet, warnings)) => (stylesheet, warnings),
        Err(error) => {
            let warning = CssParseWarnMsg {
                warning: CssParseWarnMsgInner::ParseError(error.error),
                location: error.location,
            };
            (Stylesheet::default(), vec![warning])
        }
    };

    (
        Css {
            stylesheets: vec![stylesheet].into(),
        },
        warnings,
    )
}

/// Returns the location of where the parser is currently in the document
fn get_error_location(tokenizer: &Tokenizer) -> ErrorLocation {
    ErrorLocation {
        original_pos: tokenizer.pos(),
    }
}

#[derive(Debug, Clone, PartialEq)]
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

impl<'a> From<CssSyntaxError> for CssPathParseError<'a> {
    fn from(e: CssSyntaxError) -> Self {
        CssPathParseError::SyntaxError(e)
    }
}

impl<'a> From<SimplecssError> for CssPathParseError<'a> {
    fn from(e: SimplecssError) -> Self {
        CssPathParseError::SyntaxError(CssSyntaxError::from(e))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum CssPathParseErrorOwned {
    EmptyPath,
    InvalidTokenEncountered(AzString),
    UnexpectedEndOfStream(AzString),
    SyntaxError(CssSyntaxError),
    NodeTypeTag(NodeTypeTagParseErrorOwned),
    PseudoSelectorParseError(CssPseudoSelectorParseErrorOwned),
}

impl<'a> CssPathParseError<'a> {
    pub fn to_contained(&self) -> CssPathParseErrorOwned {
        match self {
            CssPathParseError::EmptyPath => CssPathParseErrorOwned::EmptyPath,
            CssPathParseError::InvalidTokenEncountered(s) => {
                CssPathParseErrorOwned::InvalidTokenEncountered(s.to_string().into())
            }
            CssPathParseError::UnexpectedEndOfStream(s) => {
                CssPathParseErrorOwned::UnexpectedEndOfStream(s.to_string().into())
            }
            CssPathParseError::SyntaxError(e) => CssPathParseErrorOwned::SyntaxError(e.clone()),
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
    pub fn to_shared<'a>(&'a self) -> CssPathParseError<'a> {
        match self {
            CssPathParseErrorOwned::EmptyPath => CssPathParseError::EmptyPath,
            CssPathParseErrorOwned::InvalidTokenEncountered(s) => {
                CssPathParseError::InvalidTokenEncountered(s)
            }
            CssPathParseErrorOwned::UnexpectedEndOfStream(s) => {
                CssPathParseError::UnexpectedEndOfStream(s)
            }
            CssPathParseErrorOwned::SyntaxError(e) => CssPathParseError::SyntaxError(e.clone()),
            CssPathParseErrorOwned::NodeTypeTag(e) => CssPathParseError::NodeTypeTag(e.to_shared()),
            CssPathParseErrorOwned::PseudoSelectorParseError(e) => {
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
pub fn parse_css_path<'a>(input: &'a str) -> Result<CssPath, CssPathParseError<'a>> {
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
            Token::TypeSelector(div_type) => {
                if let Ok(nt) = NodeTypeTag::from_str(div_type) {
                    selectors.push(CssPathSelector::Type(nt));
                }
            }
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

    if !selectors.is_empty() {
        Ok(CssPath {
            selectors: selectors.into(),
        })
    } else {
        Err(CssPathParseError::EmptyPath)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnparsedCssRuleBlock<'a> {
    /// The css path (full selector) of the style ruleset
    pub path: CssPath,
    /// `"justify-content" => "center"`
    pub declarations: BTreeMap<&'a str, (&'a str, ErrorLocationRange)>,
    /// Conditions from enclosing @-rules (@media, @lang, etc.)
    pub conditions: Vec<DynamicSelector>,
}

/// Owned version of UnparsedCssRuleBlock, with BTreeMap of Strings.
#[derive(Debug, Clone, PartialEq)]
pub struct UnparsedCssRuleBlockOwned {
    pub path: CssPath,
    pub declarations: BTreeMap<String, (String, ErrorLocationRange)>,
    pub conditions: Vec<DynamicSelector>,
}

impl<'a> UnparsedCssRuleBlock<'a> {
    pub fn to_contained(&self) -> UnparsedCssRuleBlockOwned {
        UnparsedCssRuleBlockOwned {
            path: self.path.clone(),
            declarations: self
                .declarations
                .iter()
                .map(|(k, (v, loc))| (k.to_string(), (v.to_string(), loc.clone())))
                .collect(),
            conditions: self.conditions.clone(),
        }
    }
}

impl UnparsedCssRuleBlockOwned {
    pub fn to_shared<'a>(&'a self) -> UnparsedCssRuleBlock<'a> {
        UnparsedCssRuleBlock {
            path: self.path.clone(),
            declarations: self
                .declarations
                .iter()
                .map(|(k, (v, loc))| (k.as_str(), (v.as_str(), loc.clone())))
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

/// Owned version of CssParseWarnMsg, where warning is the owned type.
#[derive(Debug, Clone, PartialEq)]
pub struct CssParseWarnMsgOwned {
    pub warning: CssParseWarnMsgInnerOwned,
    pub location: ErrorLocationRange,
}

impl<'a> CssParseWarnMsg<'a> {
    pub fn to_contained(&self) -> CssParseWarnMsgOwned {
        CssParseWarnMsgOwned {
            warning: self.warning.to_contained(),
            location: self.location.clone(),
        }
    }
}

impl CssParseWarnMsgOwned {
    pub fn to_shared<'a>(&'a self) -> CssParseWarnMsg<'a> {
        CssParseWarnMsg {
            warning: self.warning.to_shared(),
            location: self.location.clone(),
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

impl<'a> CssParseWarnMsgInner<'a> {
    pub fn to_contained(&self) -> CssParseWarnMsgInnerOwned {
        match self {
            Self::UnsupportedKeyValuePair { key, value } => {
                CssParseWarnMsgInnerOwned::UnsupportedKeyValuePair {
                    key: key.to_string(),
                    value: value.to_string(),
                }
            }
            Self::ParseError(e) => CssParseWarnMsgInnerOwned::ParseError(e.to_contained()),
            Self::SkippedRule { selector, error } => CssParseWarnMsgInnerOwned::SkippedRule {
                selector: selector.map(|s| s.to_string()),
                error: error.to_contained(),
            },
            Self::SkippedDeclaration { key, value, error } => {
                CssParseWarnMsgInnerOwned::SkippedDeclaration {
                    key: key.to_string(),
                    value: value.to_string(),
                    error: error.to_contained(),
                }
            }
            Self::MalformedStructure { message } => CssParseWarnMsgInnerOwned::MalformedStructure {
                message: message.to_string(),
            },
        }
    }
}

impl CssParseWarnMsgInnerOwned {
    pub fn to_shared<'a>(&'a self) -> CssParseWarnMsgInner<'a> {
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
        format!("Skipped rule for selector '{}': {}", sel, error)
    },
    SkippedDeclaration { key, value, error } => format!("Skipped declaration '{}:{}': {}", key, value, error),
    MalformedStructure { message } => format!("Malformed CSS structure: {}", message),
}}

/// Parses @media conditions from the content following "@media"
/// Returns a list of DynamicSelectors for the conditions
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
        _ => {}
    }

    None
}

/// Parses a pixel value like "800px" and returns the numeric value
fn parse_px_value(value: &str) -> Option<f32> {
    let value = value.trim();
    if let Some(num_str) = value.strip_suffix("px") {
        num_str.trim().parse::<f32>().ok()
    } else {
        // Try parsing as a bare number
        value.parse::<f32>().ok()
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

/// Parses @os condition from the content following "@os"
/// Format: @os(linux) or @os("windows") or @os(macos)
fn parse_os_condition(content: &str) -> Option<DynamicSelector> {
    let content = content.trim();

    // Remove parentheses and quotes
    let os = content
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(content)
        .trim();

    let os = os
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .or_else(|| os.strip_prefix('\'').and_then(|s| s.strip_suffix('\'')))
        .unwrap_or(os)
        .trim();

    if os.is_empty() {
        return None;
    }

    // Parse OS name (case-insensitive)
    let condition = match os.to_lowercase().as_str() {
        "linux" => OsCondition::Linux,
        "windows" | "win" => OsCondition::Windows,
        "macos" | "mac" | "osx" => OsCondition::MacOS,
        "ios" => OsCondition::IOS,
        "android" => OsCondition::Android,
        "apple" => OsCondition::Apple, // macOS + iOS
        "web" | "wasm" => OsCondition::Web,
        "*" | "any" | "all" => OsCondition::Any,
        _ => return None, // Unknown OS
    };

    Some(DynamicSelector::Os(condition))
}

/// Parses a CSS string (single-threaded) and returns the parsed rules in blocks
///
/// May return "warning" messages, i.e. messages that just serve as a warning,
/// instead of being actual errors. These warnings may be ignored by the caller,
/// but can be useful for debugging.
fn new_from_str_inner<'a>(
    css_string: &'a str,
    tokenizer: &mut Tokenizer<'a>,
) -> Result<(Stylesheet, Vec<CssParseWarnMsg<'a>>), CssParseError<'a>> {
    use azul_simplecss::{Combinator, Token};

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
    struct NestingLevel<'a> {
        paths: Vec<Vec<CssPathSelector>>,
        declarations: BTreeMap<&'a str, (&'a str, ErrorLocationRange)>,
        nesting_level: usize,
    }
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
                warnings.push(CssParseWarnMsg {
                    warning: CssParseWarnMsgInner::ParseError(e.into()),
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

        // Helper: get parent paths from nesting stack (if any)
        fn get_parent_paths(nesting_stack: &[NestingLevel<'_>]) -> Vec<Vec<CssPathSelector>> {
            if let Some(parent) = nesting_stack.last() {
                parent.paths.clone()
            } else {
                Vec::new()
            }
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
                        "os" => parse_os_condition(&combined_content).into_iter().collect(),
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

                    // Get parent paths and combine with current paths
                    let parent_paths = get_parent_paths(&nesting_stack);
                    let combined_paths: Vec<Vec<CssPathSelector>> = if parent_paths.is_empty() {
                        current_paths.clone()
                    } else {
                        // Combine each parent path with each current path
                        let mut result = Vec::new();
                        for parent in &parent_paths {
                            for child in &current_paths {
                                // Check if child starts with pseudo-selector
                                let is_pseudo_only = child.first().map(|s| matches!(s, CssPathSelector::PseudoSelector(_))).unwrap_or(false);
                                let mut combined = parent.clone();
                                if !is_pseudo_only && !child.is_empty() {
                                    combined.push(CssPathSelector::Children);
                                }
                                combined.extend(child.iter().cloned());
                                result.push(combined);
                            }
                        }
                        result
                    };

                    // Push to nesting stack
                    nesting_stack.push(NestingLevel {
                        paths: combined_paths,
                        declarations: std::mem::take(&mut current_declarations),
                        nesting_level: block_nesting,
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
                // Parse attribute selector - for now just store as-is
                // TODO: properly parse attribute selectors
                last_path.push(CssPathSelector::Class(format!("[{}]", attr).into()));
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

    Ok((stylesheet, warnings))
}

fn css_blocks_to_stylesheet<'a>(
    css_blocks: Vec<UnparsedCssRuleBlock<'a>>,
    css_string: &'a str,
) -> (Stylesheet, Vec<CssParseWarnMsg<'a>>) {
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
            path: unparsed_css_block.path.into(),
            declarations: declarations.into(),
            conditions: unparsed_css_block.conditions.into(),
        });
    }

    (
        Stylesheet {
            rules: parsed_css_blocks.into(),
        },
        warnings,
    )
}

fn parse_declaration_resilient<'a>(
    unparsed_css_key: &'a str,
    unparsed_css_value: &'a str,
    location: ErrorLocationRange,
    css_key_map: &CssKeyMap,
) -> Result<Vec<CssDeclaration>, CssParseErrorInner<'a>> {
    let mut declarations = Vec::new();

    if let Some(combined_key) = CombinedCssPropertyType::from_str(unparsed_css_key, css_key_map) {
        if let Some(css_var) = check_if_value_is_css_var(unparsed_css_value) {
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

fn unparsed_css_blocks_to_stylesheet<'a>(
    css_blocks: Vec<UnparsedCssRuleBlock<'a>>,
    css_string: &'a str,
) -> Result<(Stylesheet, Vec<CssParseWarnMsg<'a>>), CssParseError<'a>> {
    // Actually parse the properties
    let css_key_map = crate::props::property::get_css_key_map();

    let mut warnings = Vec::new();

    let parsed_css_blocks = css_blocks
        .into_iter()
        .map(|unparsed_css_block| {
            let mut declarations = Vec::<CssDeclaration>::new();

            for (unparsed_css_key, (unparsed_css_value, location)) in
                unparsed_css_block.declarations
            {
                parse_css_declaration(
                    unparsed_css_key,
                    unparsed_css_value,
                    location,
                    &css_key_map,
                    &mut warnings,
                    &mut declarations,
                )
                .map_err(|e| CssParseError {
                    css_string,
                    error: e.into(),
                    location,
                })?;
            }

            Ok(CssRuleBlock {
                path: unparsed_css_block.path.into(),
                declarations: declarations.into(),
                conditions: unparsed_css_block.conditions.into(),
            })
        })
        .collect::<Result<Vec<CssRuleBlock>, CssParseError>>()?;

    Ok((parsed_css_blocks.into(), warnings))
}

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

fn check_if_value_is_css_var<'a>(
    unparsed_css_value: &'a str,
) -> Option<Result<(&'a str, &'a str), CssParseErrorInner<'a>>> {
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
fn parse_css_variable_brace_contents<'a>(input: &'a str) -> Option<(&'a str, Option<&'a str>)> {
    let input = input.trim();

    let mut split_comma_iter = input.splitn(2, ",");
    let var_name = split_comma_iter.next()?;
    let var_name = var_name.trim();

    if !var_name.starts_with("--") {
        return None; // no proper CSS variable name
    }

    Some((&var_name[2..], split_comma_iter.next()))
}
