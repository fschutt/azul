//! High-level types and functions related to CSS parsing
use alloc::{collections::BTreeMap, string::ToString, vec::Vec};
use core::{fmt, num::ParseIntError};

pub use azul_simplecss::Error as CssSyntaxError;
use azul_simplecss::Tokenizer;

pub use crate::parser::{CssParsingError, CssParsingErrorOwned};
use crate::{
    AzString, CombinedCssPropertyType, Css, CssDeclaration, CssKeyMap, CssNthChildSelector,
    CssNthChildSelector::*, CssPath, CssPathPseudoSelector, CssPathSelector, CssPropertyType,
    CssRuleBlock, DynamicCssProperty, NodeTypeTag, NodeTypeTagParseError,
    NodeTypeTagParseErrorOwned, Stylesheet,
};

#[derive(Debug, Default, PartialEq, PartialOrd, Clone)]
#[repr(transparent)]
pub struct CssApiWrapper {
    pub css: Css,
}

impl From<Css> for CssApiWrapper {
    fn from(value: Css) -> Self {
        Self { css: value }
    }
}

impl CssApiWrapper {
    pub fn empty() -> Self {
        Self { css: Css::empty() }
    }

    pub fn from_string(s: AzString) -> Self {
        Self {
            css: crate::parser::new_from_str(s.as_str()).unwrap_or_default(),
        }
    }
}

/// Error that can happen during the parsing of a CSS value
#[derive(Debug, Clone, PartialEq)]
pub struct CssParseError<'a> {
    pub css_string: &'a str,
    pub error: CssParseErrorInner<'a>,
    pub location: (ErrorLocation, ErrorLocation),
}

/// Owned version of CssParseError, without references.
#[derive(Debug, Clone, PartialEq)]
pub struct CssParseErrorOwned {
    pub css_string: String,
    pub error: CssParseErrorInnerOwned,
    pub location: (ErrorLocation, ErrorLocation),
}

impl<'a> CssParseError<'a> {
    pub fn to_contained(&self) -> CssParseErrorOwned {
        CssParseErrorOwned {
            css_string: self.css_string.to_string(),
            error: self.error.to_contained(),
            location: self.location.clone(),
        }
    }
}

impl CssParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssParseError<'a> {
        CssParseError {
            css_string: &self.css_string,
            error: self.error.to_shared(),
            location: self.location.clone(),
        }
    }
}

impl<'a> CssParseError<'a> {
    /// Returns the string between the (start, end) location
    pub fn get_error_string(&self) -> &'a str {
        let (start, end) = (self.location.0.original_pos, self.location.1.original_pos);
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

#[derive(Debug, Clone, PartialEq)]
pub enum CssParseErrorInnerOwned {
    ParseError(CssSyntaxError),
    UnclosedBlock,
    MalformedCss,
    DynamicCssParseError(DynamicCssParseErrorOwned),
    PseudoSelectorParseError(CssPseudoSelectorParseErrorOwned),
    NodeTypeTag(NodeTypeTagParseErrorOwned),
    UnknownPropertyKey(String, String),
    VarOnShorthandProperty {
        key: CombinedCssPropertyType,
        value: String,
    },
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
                CssParseErrorInnerOwned::UnknownPropertyKey(a.to_string(), b.to_string())
            }
            CssParseErrorInner::VarOnShorthandProperty { key, value } => {
                CssParseErrorInnerOwned::VarOnShorthandProperty {
                    key: key.clone(),
                    value: value.to_string(),
                }
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
            CssParseErrorInnerOwned::UnknownPropertyKey(a, b) => {
                CssParseErrorInner::UnknownPropertyKey(a, b)
            }
            CssParseErrorInnerOwned::VarOnShorthandProperty { key, value } => {
                CssParseErrorInner::VarOnShorthandProperty {
                    key: key.clone(),
                    value,
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

#[derive(Debug, Clone, PartialEq)]
pub enum CssPseudoSelectorParseErrorOwned {
    EmptyNthChild,
    UnknownSelector(String, Option<String>),
    InvalidNthChildPattern(String),
    InvalidNthChild(ParseIntError),
}

impl<'a> CssPseudoSelectorParseError<'a> {
    pub fn to_contained(&self) -> CssPseudoSelectorParseErrorOwned {
        match self {
            CssPseudoSelectorParseError::EmptyNthChild => {
                CssPseudoSelectorParseErrorOwned::EmptyNthChild
            }
            CssPseudoSelectorParseError::UnknownSelector(a, b) => {
                CssPseudoSelectorParseErrorOwned::UnknownSelector(
                    a.to_string(),
                    b.map(|s| s.to_string()),
                )
            }
            CssPseudoSelectorParseError::InvalidNthChildPattern(s) => {
                CssPseudoSelectorParseErrorOwned::InvalidNthChildPattern(s.to_string())
            }
            CssPseudoSelectorParseError::InvalidNthChild(e) => {
                CssPseudoSelectorParseErrorOwned::InvalidNthChild(e.clone())
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
            CssPseudoSelectorParseErrorOwned::UnknownSelector(a, b) => {
                CssPseudoSelectorParseError::UnknownSelector(a, b.as_deref())
            }
            CssPseudoSelectorParseErrorOwned::InvalidNthChildPattern(s) => {
                CssPseudoSelectorParseError::InvalidNthChildPattern(s)
            }
            CssPseudoSelectorParseErrorOwned::InvalidNthChild(e) => {
                CssPseudoSelectorParseError::InvalidNthChild(e.clone())
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
pub enum DynamicCssParseErrorOwned {
    InvalidBraceContents(String),
    UnexpectedValue(CssParsingErrorOwned),
}

impl<'a> DynamicCssParseError<'a> {
    pub fn to_contained(&self) -> DynamicCssParseErrorOwned {
        match self {
            DynamicCssParseError::InvalidBraceContents(s) => {
                DynamicCssParseErrorOwned::InvalidBraceContents(s.to_string())
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
fn pseudo_selector_from_str<'a>(
    selector: &'a str,
    value: Option<&'a str>,
) -> Result<CssPathPseudoSelector, CssPseudoSelectorParseError<'a>> {
    match selector {
        "first" => Ok(CssPathPseudoSelector::First),
        "last" => Ok(CssPathPseudoSelector::Last),
        "hover" => Ok(CssPathPseudoSelector::Hover),
        "active" => Ok(CssPathPseudoSelector::Active),
        "focus" => Ok(CssPathPseudoSelector::Focus),
        "nth-child" => {
            let value = value.ok_or(CssPseudoSelectorParseError::EmptyNthChild)?;
            let parsed = parse_nth_child_selector(value)?;
            Ok(CssPathPseudoSelector::NthChild(parsed))
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
        return Ok(Number(number));
    }

    // If the value is not a number
    match value.as_ref() {
        "even" => Ok(Even),
        "odd" => Ok(Odd),
        other => parse_nth_child_pattern(value),
    }
}

/// Parses the pattern between the braces of a "nth-child" (such as "2n+3").
fn parse_nth_child_pattern<'a>(
    value: &'a str,
) -> Result<CssNthChildSelector, CssPseudoSelectorParseError<'a>> {
    use crate::CssNthChildPattern;

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

    Ok(Pattern(CssNthChildPattern { repeat, offset }))
}

#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ErrorLocation {
    pub original_pos: usize,
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
        let start_location = self.location.0.get_line_column_from_error(self.css_string);
        let end_location = self.location.1.get_line_column_from_error(self.css_string);
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

pub fn new_from_str<'a>(css_string: &'a str) -> Result<Css, CssParseError<'a>> {
    let mut tokenizer = Tokenizer::new(css_string);
    let (stylesheet, _warnings) = new_from_str_inner(css_string, &mut tokenizer)?;
    Ok(Css {
        stylesheets: vec![stylesheet].into(),
    })
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

#[derive(Debug, Clone, PartialEq)]
pub enum CssPathParseErrorOwned {
    EmptyPath,
    InvalidTokenEncountered(String),
    UnexpectedEndOfStream(String),
    SyntaxError(CssSyntaxError),
    NodeTypeTag(NodeTypeTagParseErrorOwned),
    PseudoSelectorParseError(CssPseudoSelectorParseErrorOwned),
}

impl<'a> CssPathParseError<'a> {
    pub fn to_contained(&self) -> CssPathParseErrorOwned {
        match self {
            CssPathParseError::EmptyPath => CssPathParseErrorOwned::EmptyPath,
            CssPathParseError::InvalidTokenEncountered(s) => {
                CssPathParseErrorOwned::InvalidTokenEncountered(s.to_string())
            }
            CssPathParseError::UnexpectedEndOfStream(s) => {
                CssPathParseErrorOwned::UnexpectedEndOfStream(s.to_string())
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
/// # use azul_css::parser::parse_css_path;
/// # use azul_css::{
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
                selectors.push(CssPathSelector::Type(NodeTypeTag::from_str(div_type)?));
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
    pub declarations: BTreeMap<&'a str, (&'a str, (ErrorLocation, ErrorLocation))>,
}

/// Owned version of UnparsedCssRuleBlock, with BTreeMap of Strings.
#[derive(Debug, Clone, PartialEq)]
pub struct UnparsedCssRuleBlockOwned {
    pub path: CssPath,
    pub declarations: BTreeMap<String, (String, (ErrorLocation, ErrorLocation))>,
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
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CssParseWarnMsg<'a> {
    warning: CssParseWarnMsgInner<'a>,
    location: (ErrorLocation, ErrorLocation),
}

/// Owned version of CssParseWarnMsg, where warning is the owned type.
#[derive(Debug, Clone, PartialEq)]
pub struct CssParseWarnMsgOwned {
    warning: CssParseWarnMsgInnerOwned,
    location: (ErrorLocation, ErrorLocation),
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
}

#[derive(Debug, Clone, PartialEq)]
pub enum CssParseWarnMsgInnerOwned {
    UnsupportedKeyValuePair { key: String, value: String },
}

impl<'a> CssParseWarnMsgInner<'a> {
    pub fn to_contained(&self) -> CssParseWarnMsgInnerOwned {
        match self {
            CssParseWarnMsgInner::UnsupportedKeyValuePair { key, value } => {
                CssParseWarnMsgInnerOwned::UnsupportedKeyValuePair {
                    key: key.to_string(),
                    value: value.to_string(),
                }
            }
        }
    }
}

impl CssParseWarnMsgInnerOwned {
    pub fn to_shared<'a>(&'a self) -> CssParseWarnMsgInner<'a> {
        match self {
            CssParseWarnMsgInnerOwned::UnsupportedKeyValuePair { key, value } => {
                CssParseWarnMsgInner::UnsupportedKeyValuePair { key, value }
            }
        }
    }
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

    // Used for error checking / checking for closed braces
    let mut parser_in_block = false;
    let mut block_nesting = 0_usize;

    // Current css paths (i.e. `div#id, .class, p` are stored here -
    // when the block is finished, all `current_rules` gets duplicated with
    // one path corresponding to one set of rules each).
    let mut current_paths = Vec::new();
    // Current CSS declarations
    let mut current_rules = BTreeMap::<&str, (&str, (ErrorLocation, ErrorLocation))>::new();
    // Keep track of the current path during parsing
    let mut last_path = Vec::new();

    let mut last_error_location = ErrorLocation { original_pos: 0 };

    loop {
        let token = tokenizer.parse_next().map_err(|e| CssParseError {
            css_string,
            error: e.into(),
            location: (last_error_location, get_error_location(tokenizer)),
        })?;

        macro_rules! check_parser_is_outside_block {
            () => {
                if parser_in_block {
                    return Err(CssParseError {
                        css_string,
                        error: CssParseErrorInner::MalformedCss,
                        location: (last_error_location, get_error_location(tokenizer)),
                    });
                }
            };
        }

        macro_rules! check_parser_is_inside_block {
            () => {
                if !parser_in_block {
                    return Err(CssParseError {
                        css_string,
                        error: CssParseErrorInner::MalformedCss,
                        location: (last_error_location, get_error_location(tokenizer)),
                    });
                }
            };
        }

        match token {
            Token::BlockStart => {
                check_parser_is_outside_block!();
                parser_in_block = true;
                block_nesting += 1;
                current_paths.push(last_path.clone());
                last_path.clear();
            }
            Token::Comma => {
                check_parser_is_outside_block!();
                current_paths.push(last_path.clone());
                last_path.clear();
            }
            Token::BlockEnd => {
                block_nesting -= 1;
                check_parser_is_inside_block!();
                parser_in_block = false;

                css_blocks.extend(current_paths.drain(..).map(|path| UnparsedCssRuleBlock {
                    path: CssPath {
                        selectors: path.into(),
                    },
                    declarations: current_rules.clone(),
                }));

                current_rules.clear();
                last_path.clear(); // technically unnecessary, but just to be sure
            }

            // tokens that adjust the last_path
            Token::UniversalSelector => {
                check_parser_is_outside_block!();
                last_path.push(CssPathSelector::Global);
            }
            Token::TypeSelector(div_type) => {
                check_parser_is_outside_block!();
                last_path.push(CssPathSelector::Type(
                    NodeTypeTag::from_str(div_type).map_err(|e| CssParseError {
                        css_string,
                        error: e.into(),
                        location: (last_error_location, get_error_location(tokenizer)),
                    })?,
                ));
            }
            Token::IdSelector(id) => {
                check_parser_is_outside_block!();
                last_path.push(CssPathSelector::Id(id.to_string().into()));
            }
            Token::ClassSelector(class) => {
                check_parser_is_outside_block!();
                last_path.push(CssPathSelector::Class(class.to_string().into()));
            }
            Token::Combinator(Combinator::GreaterThan) => {
                check_parser_is_outside_block!();
                last_path.push(CssPathSelector::DirectChildren);
            }
            Token::Combinator(Combinator::Space) => {
                check_parser_is_outside_block!();
                last_path.push(CssPathSelector::Children);
            }
            Token::PseudoClass { selector, value } => {
                check_parser_is_outside_block!();
                last_path.push(CssPathSelector::PseudoSelector(
                    pseudo_selector_from_str(selector, value).map_err(|e| CssParseError {
                        css_string,
                        error: e.into(),
                        location: (last_error_location, get_error_location(tokenizer)),
                    })?,
                ));
            }
            Token::Declaration(key, val) => {
                check_parser_is_inside_block!();
                current_rules.insert(
                    key,
                    (val, (last_error_location, get_error_location(tokenizer))),
                );
            }
            Token::EndOfStream => {
                // uneven number of open / close braces
                if block_nesting != 0 {
                    return Err(CssParseError {
                        css_string,
                        error: CssParseErrorInner::UnclosedBlock,
                        location: (last_error_location, get_error_location(tokenizer)),
                    });
                }

                break;
            }
            _ => {
                // attributes, lang-attributes and @keyframes are not supported
            }
        }

        last_error_location = get_error_location(tokenizer);
    }

    unparsed_css_blocks_to_stylesheet(css_blocks, css_string)
}

fn unparsed_css_blocks_to_stylesheet<'a>(
    css_blocks: Vec<UnparsedCssRuleBlock<'a>>,
    css_string: &'a str,
) -> Result<(Stylesheet, Vec<CssParseWarnMsg<'a>>), CssParseError<'a>> {
    // Actually parse the properties (TODO: this could be done in parallel and in a separate
    // function)
    let css_key_map = crate::get_css_key_map();

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
            })
        })
        .collect::<Result<Vec<CssRuleBlock>, CssParseError>>()?;

    Ok((parsed_css_blocks.into(), warnings))
}

pub fn parse_css_declaration<'a>(
    unparsed_css_key: &'a str,
    unparsed_css_value: &'a str,
    location: (ErrorLocation, ErrorLocation),
    css_key_map: &CssKeyMap,
    warnings: &mut Vec<CssParseWarnMsg<'a>>,
    declarations: &mut Vec<CssDeclaration>,
) -> Result<(), CssParseErrorInner<'a>> {
    use self::{CssParseErrorInner::*, CssParseWarnMsgInner::*};

    if let Some(combined_key) = CombinedCssPropertyType::from_str(unparsed_css_key, &css_key_map) {
        if let Some(css_var) = check_if_value_is_css_var(unparsed_css_value) {
            // margin: var(--my-variable);
            return Err(VarOnShorthandProperty {
                key: combined_key,
                value: unparsed_css_value,
            });
        } else {
            // margin: 10px;
            let parsed_css_properties =
                crate::parser::parse_combined_css_property(combined_key, unparsed_css_value)
                    .map_err(|e| DynamicCssParseError(e.into()))?;

            declarations.extend(
                parsed_css_properties
                    .into_iter()
                    .map(|val| CssDeclaration::Static(val)),
            );
        }
    } else if let Some(normal_key) = CssPropertyType::from_str(unparsed_css_key, css_key_map) {
        if let Some(css_var) = check_if_value_is_css_var(unparsed_css_value) {
            // margin-left: var(--my-variable);
            let (css_var_id, css_var_default) = css_var?;
            let parsed_default_value =
                crate::parser::parse_css_property(normal_key, css_var_default)
                    .map_err(|e| DynamicCssParseError(e.into()))?;

            declarations.push(CssDeclaration::Dynamic(DynamicCssProperty {
                dynamic_id: css_var_id.to_string().into(),
                default_value: parsed_default_value,
            }));
        } else {
            // margin-left: 10px;
            let parsed_css_value =
                crate::parser::parse_css_property(normal_key, unparsed_css_value)
                    .map_err(|e| DynamicCssParseError(e.into()))?;

            declarations.push(CssDeclaration::Static(parsed_css_value));
        }
    } else {
        // asldfkjasdf: 10px;
        warnings.push(CssParseWarnMsg {
            warning: UnsupportedKeyValuePair {
                key: unparsed_css_key,
                value: unparsed_css_value,
            },
            location,
        });
    }

    Ok(())
}

fn check_if_value_is_css_var<'a>(
    unparsed_css_value: &'a str,
) -> Option<Result<(&'a str, &'a str), CssParseErrorInner<'a>>> {
    const DEFAULT_VARIABLE_DEFAULT: &str = "none";

    let (_, brace_contents) =
        crate::parser::parse_parentheses(unparsed_css_value, &["var"]).ok()?;

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
