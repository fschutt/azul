//! High-level types and functions related to CSS parsing
use std::{
    num::ParseIntError,
    fmt,
    collections::HashMap,
};
pub use azul_simplecss::Error as CssSyntaxError;
use azul_simplecss::Tokenizer;

use crate::css_parser;
pub use crate::css_parser::CssParsingError;
use azul_css::{
    Css, CssDeclaration, Stylesheet, DynamicCssProperty,
    CssPropertyType, CssRuleBlock, CssPath, CssPathSelector,
    CssNthChildSelector, CssPathPseudoSelector, CssNthChildSelector::*,
    NodeTypePath, NodeTypePathParseError, CombinedCssPropertyType, CssKeyMap,
};

/// Error that can happen during the parsing of a CSS value
#[derive(Debug, Clone, PartialEq)]
pub struct CssParseError<'a> {
    pub css_string: &'a str,
    pub error: CssParseErrorInner<'a>,
    pub location: (ErrorLocation, ErrorLocation),
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
    NodeTypePath(NodeTypePathParseError<'a>),
    /// A certain property has an unknown key, for example: `alsdfkj: 500px` = `unknown CSS key "alsdfkj: 500px"`
    UnknownPropertyKey(&'a str, &'a str),
    /// `var()` can't be used on properties that expand to multiple values, since they would be ambigouus
    /// and degrade performance - for example `margin: var(--blah)` would be ambigouus because it's not clear
    /// when setting the variable, whether all sides should be set, instead, you have to use `margin-top: var(--blah)`,
    /// `margin-bottom: var(--baz)` in order to work around this limitation.
    VarOnShorthandProperty { key: CombinedCssPropertyType, value: &'a str },
}

impl_display!{ CssParseErrorInner<'a>, {
    ParseError(e) => format!("Parse Error: {:?}", e),
    UnclosedBlock => "Unclosed block",
    MalformedCss => "Malformed Css",
    DynamicCssParseError(e) => format!("{}", e),
    PseudoSelectorParseError(e) => format!("Failed to parse pseudo-selector: {}", e),
    NodeTypePath(e) => format!("Failed to parse CSS selector path: {}", e),
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
impl_from! { NodeTypePathParseError<'a>, CssParseErrorInner::NodeTypePath }
impl_from! { CssPseudoSelectorParseError<'a>, CssParseErrorInner::PseudoSelectorParseError }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CssPseudoSelectorParseError<'a> {
    EmptyNthChild,
    UnknownSelector(&'a str, Option<&'a str>),
    InvalidNthChildPattern(&'a str),
    InvalidNthChild(ParseIntError),
}

impl<'a> From<ParseIntError> for CssPseudoSelectorParseError<'a> {
    fn from(e: ParseIntError) -> Self { CssPseudoSelectorParseError::InvalidNthChild(e) }
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

/// Error that can happen during `css_parser::parse_key_value_pair`
#[derive(Debug, Clone, PartialEq)]
pub enum DynamicCssParseError<'a> {
    /// The brace contents aren't valid, i.e. `var(asdlfkjasf)`
    InvalidBraceContents(&'a str),
    /// Unexpected value when parsing the string
    UnexpectedValue(CssParsingError<'a>),
}

impl_display!{ DynamicCssParseError<'a>, {
    InvalidBraceContents(e) => format!("Invalid contents of var() function: var({})", e),
    UnexpectedValue(e) => format!("{}", e),
}}

impl<'a> From<CssParsingError<'a>> for DynamicCssParseError<'a> {
    fn from(e: CssParsingError<'a>) -> Self {
        DynamicCssParseError::UnexpectedValue(e)
    }
}

/// "selector" contains the actual selector such as "nth-child" while "value" contains
/// an optional value - for example "nth-child(3)" would be: selector: "nth-child", value: "3".
fn pseudo_selector_from_str<'a>(selector: &'a str, value: Option<&'a str>)
-> Result<CssPathPseudoSelector, CssPseudoSelectorParseError<'a>>
{
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
        },
        _ => {
            Err(CssPseudoSelectorParseError::UnknownSelector(selector, value))
        },
    }
}

/// Parses the inner value of the `:nth-child` selector, including numbers and patterns.
///
/// I.e.: `"2n+3"` -> `Pattern { repeat: 2, offset: 3 }`
fn parse_nth_child_selector<'a>(value: &'a str) -> Result<CssNthChildSelector, CssPseudoSelectorParseError<'a>> {

    let value = value.trim();

    if value.is_empty() {
        return Err(CssPseudoSelectorParseError::EmptyNthChild);
    }

    if let Ok(number) = value.parse::<usize>() {
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
fn parse_nth_child_pattern<'a>(value: &'a str) -> Result<CssNthChildSelector, CssPseudoSelectorParseError<'a>> {

    let value = value.trim();

    if value.is_empty() {
        return Err(CssPseudoSelectorParseError::EmptyNthChild);
    }

    // TODO: Test for "+"
    let repeat = value.split("n").next()
        .ok_or(CssPseudoSelectorParseError::InvalidNthChildPattern(value))?
        .trim()
        .parse::<usize>()?;

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
                offset_string.parse::<usize>()?
            }
        },
        None => 0,
    };

    Ok(Pattern { repeat, offset })
}

#[test]
fn test_css_pseudo_selector_parse() {

    use self::CssPathPseudoSelector::*;
    use self::CssPseudoSelectorParseError::*;

    let ok_res = [
        (("first", None), First),
        (("last", None), Last),
        (("hover", None), Hover),
        (("active", None), Active),
        (("focus", None), Focus),
        (("nth-child", Some("4")), NthChild(Number(4))),
        (("nth-child", Some("even")), NthChild(Even)),
        (("nth-child", Some("odd")), NthChild(Odd)),
        (("nth-child", Some("5n")), NthChild(Pattern { repeat: 5, offset: 0 })),
        (("nth-child", Some("2n+3")), NthChild(Pattern { repeat: 2, offset: 3 })),
    ];

    let err = [
        (("asdf", None), UnknownSelector("asdf", None)),
        (("", None), UnknownSelector("", None)),
        (("nth-child", Some("2n+")), InvalidNthChildPattern("2n+")),
        // Can't test for ParseIntError because the fields are private.
        // This is an example on why you shouldn't use std::error::Error!
    ];

    for ((selector, val), a) in &ok_res {
        assert_eq!(pseudo_selector_from_str(selector, *val), Ok(*a));
    }

    for ((selector, val), e) in &err {
        assert_eq!(pseudo_selector_from_str(selector, *val), Err(e.clone()));
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
        write!(f, "    start: line {}:{}\r\n    end: line {}:{}\r\n    text: \"{}\"\r\n    reason: {}",
            start_location.0, start_location.1,
            end_location.0, end_location.1,
            self.get_error_string(),
            self.error,
        )
    }
}

pub fn new_from_str<'a>(css_string: &'a str) -> Result<Css, CssParseError<'a>> {
    let mut tokenizer = Tokenizer::new(css_string);
    let (stylesheet, _warnings) = new_from_str_inner(css_string, &mut tokenizer)?;
    Ok(Css { stylesheets: vec![stylesheet] })
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
    NodeTypePath(NodeTypePathParseError<'a>),
    /// Error while parsing a pseudo selector (like `:aldkfja`)
    PseudoSelectorParseError(CssPseudoSelectorParseError<'a>),
}

impl_from! { NodeTypePathParseError<'a>, CssPathParseError::NodeTypePath }
impl_from! { CssPseudoSelectorParseError<'a>, CssPathParseError::PseudoSelectorParseError }

impl<'a> From<CssSyntaxError> for CssPathParseError<'a> {
    fn from(e: CssSyntaxError) -> Self {
        CssPathParseError::SyntaxError(e)
    }
}

/// Parses a CSS path from a string (only the path,.no commas allowed)
///
/// ```rust
/// # extern crate azul_css;
/// # extern crate azul_css_parser;
/// # use azul_css_parser::parse_css_path;
/// # use azul_css::{
/// #     CssPathSelector::*, CssPathPseudoSelector::*, CssPath,
/// #     NodeTypePath::*, CssNthChildSelector::*
/// # };
///
/// assert_eq!(
///     parse_css_path("* div #my_id > .class:nth-child(2)"),
///     Ok(CssPath { selectors: vec![
///          Global,
///          Type(Div),
///          Children,
///          Id("my_id".to_string()),
///          DirectChildren,
///          Class("class".to_string()),
///          PseudoSelector(NthChild(Number(2))),
///     ]})
/// );
/// ```
pub fn parse_css_path<'a>(input: &'a str) -> Result<CssPath, CssPathParseError<'a>> {

    use azul_simplecss::{Token, Combinator};

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
            },
            Token::TypeSelector(div_type) => {
                selectors.push(CssPathSelector::Type(NodeTypePath::from_str(div_type)?));
            },
            Token::IdSelector(id) => {
                selectors.push(CssPathSelector::Id(id.to_string()));
            },
            Token::ClassSelector(class) => {
                selectors.push(CssPathSelector::Class(class.to_string()));
            },
            Token::Combinator(Combinator::GreaterThan) => {
                selectors.push(CssPathSelector::DirectChildren);
            },
            Token::Combinator(Combinator::Space) => {
                selectors.push(CssPathSelector::Children);
            },
            Token::PseudoClass { selector, value } => {
                selectors.push(CssPathSelector::PseudoSelector(pseudo_selector_from_str(selector, value)?));
            },
            Token::EndOfStream => {
                break;
            }
            _ => {
                return Err(CssPathParseError::InvalidTokenEncountered(input));
            }
        }
    }

    if !selectors.is_empty() {
        Ok(CssPath { selectors })
    } else {
        Err(CssPathParseError::EmptyPath)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnparsedCssRuleBlock<'a> {
    /// The css path (full selector) of the style ruleset
    pub path: CssPath,
    /// `"justify-content" => "center"`
    pub declarations: HashMap<&'a str, (&'a str, (ErrorLocation, ErrorLocation))>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CssParseWarnMsg<'a> {
    warning: CssParseWarnMsgInner<'a>,
    location: (ErrorLocation, ErrorLocation),
}

#[derive(Debug, Clone, PartialEq)]
pub enum CssParseWarnMsgInner<'a> {
    /// Key "blah" isn't (yet) supported, so the parser didn't attempt to parse the value at all
    UnsupportedKeyValuePair { key: &'a str, value: &'a str },
}

/// Parses a CSS string (single-threaded) and returns the parsed rules in blocks
///
/// May return "warning" messages, i.e. messages that just serve as a warning,
/// instead of being actual errors. These warnings may be ignored by the caller,
/// but can be useful for debugging.
fn new_from_str_inner<'a>(css_string: &'a str, tokenizer: &mut Tokenizer<'a>)
-> Result<(Stylesheet, Vec<CssParseWarnMsg<'a>>), CssParseError<'a>> {

    use azul_simplecss::{Token, Combinator};

    let mut css_blocks = Vec::new();

    // Used for error checking / checking for closed braces
    let mut parser_in_block = false;
    let mut block_nesting = 0_usize;

    // Current css paths (i.e. `div#id, .class, p` are stored here -
    // when the block is finished, all `current_rules` gets duplicated with
    // one path corresponding to one set of rules each).
    let mut current_paths = Vec::new();
    // Current CSS declarations
    let mut current_rules = HashMap::<&str, (&str, (ErrorLocation, ErrorLocation))>::new();
    // Keep track of the current path during parsing
    let mut last_path = Vec::new();

    let mut last_error_location = ErrorLocation { original_pos: 0 };

    loop {

        let token = tokenizer.parse_next().map_err(|e| CssParseError {
            css_string,
            error: e.into(),
            location: (last_error_location, get_error_location(tokenizer))
        })?;

        macro_rules! check_parser_is_outside_block {() => {
            if parser_in_block {
                return Err(CssParseError {
                    css_string,
                    error: CssParseErrorInner::MalformedCss,
                    location: (last_error_location, get_error_location(tokenizer)),
                });
            }
        }}

        macro_rules! check_parser_is_inside_block {() => {
            if !parser_in_block {
                return Err(CssParseError {
                    css_string,
                    error: CssParseErrorInner::MalformedCss,
                    location: (last_error_location, get_error_location(tokenizer)),
                });
            }
        }}

        match token {
            Token::BlockStart => {
                check_parser_is_outside_block!();
                parser_in_block = true;
                block_nesting += 1;
                current_paths.push(last_path.clone());
                last_path.clear();
            },
            Token::Comma => {
                check_parser_is_outside_block!();
                current_paths.push(last_path.clone());
                last_path.clear();
            },
            Token::BlockEnd => {

                block_nesting -= 1;
                check_parser_is_inside_block!();
                parser_in_block = false;

                css_blocks.extend(current_paths.drain(..).map(|path| {
                    UnparsedCssRuleBlock {
                        path: CssPath { selectors: path },
                        declarations: current_rules.clone(),
                    }
                }));

                current_rules.clear();
                last_path.clear(); // technically unnecessary, but just to be sure
            },

            // tokens that adjust the last_path
            Token::UniversalSelector => {
                check_parser_is_outside_block!();
                last_path.push(CssPathSelector::Global);
            },
            Token::TypeSelector(div_type) => {
                check_parser_is_outside_block!();
                last_path.push(CssPathSelector::Type(NodeTypePath::from_str(div_type).map_err(|e| {
                    CssParseError {
                        css_string,
                        error: e.into(),
                        location: (last_error_location, get_error_location(tokenizer)),
                    }
                })?));
            },
            Token::IdSelector(id) => {
                check_parser_is_outside_block!();
                last_path.push(CssPathSelector::Id(id.to_string()));
            },
            Token::ClassSelector(class) => {
                check_parser_is_outside_block!();
                last_path.push(CssPathSelector::Class(class.to_string()));
            },
            Token::Combinator(Combinator::GreaterThan) => {
                check_parser_is_outside_block!();
                last_path.push(CssPathSelector::DirectChildren);
            },
            Token::Combinator(Combinator::Space) => {
                check_parser_is_outside_block!();
                last_path.push(CssPathSelector::Children);
            },
            Token::PseudoClass { selector, value } => {
                check_parser_is_outside_block!();
                last_path.push(CssPathSelector::PseudoSelector(pseudo_selector_from_str(selector, value).map_err(|e| {
                    CssParseError {
                        css_string,
                        error: e.into(),
                        location: (last_error_location, get_error_location(tokenizer)),
                    }
                })?));
            },
            Token::Declaration(key, val) => {
                check_parser_is_inside_block!();
                current_rules.insert(key, (val, (last_error_location, get_error_location(tokenizer))));
            },
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
            },
            _ => {
                // attributes, lang-attributes and @keyframes are not supported
            }
        }

        last_error_location = get_error_location(tokenizer);
    }

    unparsed_css_blocks_to_stylesheet(css_blocks, css_string)
}

fn unparsed_css_blocks_to_stylesheet<'a>(css_blocks: Vec<UnparsedCssRuleBlock<'a>>, css_string: &'a str)
-> Result<(Stylesheet, Vec<CssParseWarnMsg<'a>>), CssParseError<'a>> {

    // Actually parse the properties (TODO: this could be done in parallel and in a separate function)
    let css_key_map = azul_css::get_css_key_map();

    let mut warnings = Vec::new();

    let parsed_css_blocks = css_blocks.into_iter().map(|unparsed_css_block| {

        let mut declarations = Vec::<CssDeclaration>::new();

        for (unparsed_css_key, (unparsed_css_value, location)) in unparsed_css_block.declarations {
            parse_css_declaration(
                unparsed_css_key,
                unparsed_css_value,
                location,
                &css_key_map,
                &mut warnings,
                &mut declarations,
            ).map_err(|e| CssParseError {
                css_string,
                error: e.into(),
                location,
            })?;
        }

        Ok(CssRuleBlock {
            path: unparsed_css_block.path,
            declarations,
        })
    }).collect::<Result<Vec<CssRuleBlock>, CssParseError>>()?;

    Ok((parsed_css_blocks.into(), warnings))
}

fn parse_css_declaration<'a>(
    unparsed_css_key: &'a str,
    unparsed_css_value: &'a str,
    location: (ErrorLocation, ErrorLocation),
    css_key_map: &CssKeyMap,
    warnings: &mut Vec<CssParseWarnMsg<'a>>,
    declarations: &mut Vec<CssDeclaration>,
) -> Result<(), CssParseErrorInner<'a>> {

    use self::CssParseErrorInner::*;
    use self::CssParseWarnMsgInner::*;

    if let Some(combined_key) = CombinedCssPropertyType::from_str(unparsed_css_key, &css_key_map) {
        if let Some(css_var) = check_if_value_is_css_var(unparsed_css_value) {
            // margin: var(--my-variable);
            return Err(VarOnShorthandProperty { key: combined_key, value: unparsed_css_value });
        } else {
            // margin: 10px;
            let parsed_css_properties =
                css_parser::parse_combined_css_property(combined_key, unparsed_css_value)
                .map_err(|e| DynamicCssParseError(e.into()))?;

            declarations.extend(parsed_css_properties.into_iter().map(|val| CssDeclaration::Static(val)));
        }
    } else if let Some(normal_key) = CssPropertyType::from_str(unparsed_css_key, css_key_map) {
        if let Some(css_var) = check_if_value_is_css_var(unparsed_css_value) {
            // margin-left: var(--my-variable);
            let (css_var_id, css_var_default) = css_var?;
            let parsed_default_value =
                css_parser::parse_css_property(normal_key, css_var_default)
                .map_err(|e| DynamicCssParseError(e.into()))?;

            declarations.push(CssDeclaration::Dynamic(DynamicCssProperty {
                dynamic_id: css_var_id.to_string(),
                default_value: parsed_default_value,
            }));
        } else {
            // margin-left: 10px;
            let parsed_css_value =
                css_parser::parse_css_property(normal_key, unparsed_css_value)
                .map_err(|e| DynamicCssParseError(e.into()))?;

            declarations.push(CssDeclaration::Static(parsed_css_value));
        }
    } else {
        // asldfkjasdf: 10px;
        warnings.push(CssParseWarnMsg {
            warning: UnsupportedKeyValuePair { key: unparsed_css_key, value: unparsed_css_value },
            location,
        });
    }

    Ok(())
}

fn check_if_value_is_css_var<'a>(unparsed_css_value: &'a str) -> Option<Result<(&'a str, &'a str), CssParseErrorInner<'a>>> {

    const DEFAULT_VARIABLE_DEFAULT: &str = "none";

    let (_, brace_contents) = css_parser::parse_parentheses(unparsed_css_value, &["var"]).ok()?;

    // value is a CSS variable, i.e. var(--main-bg-color)
    Some(match parse_css_variable_brace_contents(brace_contents) {
        Some((variable_id, default_value)) => Ok((variable_id, default_value.unwrap_or(DEFAULT_VARIABLE_DEFAULT))),
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

#[test]
fn test_css_parse_1() {

    use azul_css::*;

    let parsed_css = new_from_str("
        div#my_id .my_class:first {
            background-color: red;
        }
    ").unwrap();


    let expected_css_rules = vec![CssRuleBlock {
        path: CssPath {
            selectors: vec![
                CssPathSelector::Type(NodeTypePath::Div),
                CssPathSelector::Id(String::from("my_id")),
                CssPathSelector::Children,
                // NOTE: This is technically wrong, the space between "#my_id"
                // and ".my_class" is important, but gets ignored for now
                CssPathSelector::Class(String::from("my_class")),
                CssPathSelector::PseudoSelector(CssPathPseudoSelector::First),
            ],
        },
        declarations: vec![CssDeclaration::Static(CssProperty::BackgroundContent(
            CssPropertyValue::Exact(StyleBackgroundContent::Color(ColorU {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            })),
        ))],
    }];

    assert_eq!(
        parsed_css,
        Css {
            stylesheets: vec![expected_css_rules.into()]
        }
    );
}

#[test]
fn test_css_simple_selector_parse() {
    use self::CssPathSelector::*;
    use azul_css::NodeTypePath;
    let css = "div#id.my_class > p .new { }";
    let parsed = vec![
        Type(NodeTypePath::Div),
        Id("id".into()),
        Class("my_class".into()),
        DirectChildren,
        Type(NodeTypePath::P),
        Children,
        Class("new".into())
    ];
    assert_eq!(new_from_str(css).unwrap(), Css {
        stylesheets: vec![Stylesheet {
            rules: vec![CssRuleBlock {
                path: CssPath { selectors: parsed },
                declarations: Vec::new(),
            }],
        }],
    });
}

#[cfg(test)]
mod stylesheet_parse {

    use azul_css::*;
    use super::*;

    fn test_css(css: &str, expected: Vec<CssRuleBlock>) {
        let css = new_from_str(css).unwrap();
        assert_eq!(css, Css { stylesheets: vec![expected.into()] });
    }

    // Tests that an element with a single class always gets the CSS element applied properly
    #[test]
    fn test_apply_css_pure_class() {
        let red = CssProperty::BackgroundContent(CssPropertyValue::Exact(
            StyleBackgroundContent::Color(ColorU {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            }),
        ));
        let blue = CssProperty::BackgroundContent(CssPropertyValue::Exact(
            StyleBackgroundContent::Color(ColorU {
                r: 0,
                g: 0,
                b: 255,
                a: 255,
            }),
        ));
        let black = CssProperty::BackgroundContent(CssPropertyValue::Exact(
            StyleBackgroundContent::Color(ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            }),
        ));

        // Simple example
        {
            let css_1 = ".my_class { background-color: red; }";
            let expected_rules = vec![
                CssRuleBlock {
                    path: CssPath { selectors: vec![CssPathSelector::Class("my_class".into())] },
                    declarations: vec![
                        CssDeclaration::Static(red.clone())
                    ],
                },
            ];
            test_css(css_1, expected_rules);
        }

        // Slightly more complex example
        {
            let css_2 = "#my_id { background-color: red; } .my_class { background-color: blue; }";
            let expected_rules = vec![
                CssRuleBlock {
                    path: CssPath { selectors: vec![CssPathSelector::Id("my_id".into())] },
                    declarations: vec![CssDeclaration::Static(red.clone())]
                },
                CssRuleBlock {
                    path: CssPath { selectors: vec![CssPathSelector::Class("my_class".into())] },
                    declarations: vec![CssDeclaration::Static(blue.clone())]
                },
            ];
            test_css(css_2, expected_rules);
        }

        // Even more complex example
        {
            let css_3 = "* { background-color: black; } .my_class#my_id { background-color: red; } .my_class { background-color: blue; }";
            let expected_rules = vec![
                CssRuleBlock {
                    path: CssPath { selectors: vec![CssPathSelector::Global] },
                    declarations: vec![CssDeclaration::Static(black.clone())]
                },
                CssRuleBlock {
                    path: CssPath { selectors: vec![CssPathSelector::Class("my_class".into()), CssPathSelector::Id("my_id".into())] },
                    declarations: vec![CssDeclaration::Static(red.clone())]
                },
                CssRuleBlock {
                    path: CssPath { selectors: vec![CssPathSelector::Class("my_class".into())] },
                    declarations: vec![CssDeclaration::Static(blue.clone())]
                },
            ];
            test_css(css_3, expected_rules);
        }
    }
}

// Assert that order of the style rules is correct (in same order as provided in CSS form)
#[test]
fn test_multiple_rules() {
    use azul_css::*;
    use self::CssPathSelector::*;

    let parsed_css = new_from_str("
        * { }
        * div.my_class#my_id { }
        * div#my_id { }
        * #my_id { }
        div.my_class.specific#my_id { }
    ").unwrap();

    let expected_rules = vec![
        // Rules are sorted by order of appearance in source string
        CssRuleBlock { path: CssPath { selectors: vec![Global] }, declarations: Vec::new() },
        CssRuleBlock { path: CssPath { selectors: vec![Global, Type(NodeTypePath::Div), Class("my_class".into()), Id("my_id".into())] }, declarations: Vec::new() },
        CssRuleBlock { path: CssPath { selectors: vec![Global, Type(NodeTypePath::Div), Id("my_id".into())] }, declarations: Vec::new() },
        CssRuleBlock { path: CssPath { selectors: vec![Global, Id("my_id".into())] }, declarations: Vec::new() },
        CssRuleBlock { path: CssPath { selectors: vec![Type(NodeTypePath::Div), Class("my_class".into()), Class("specific".into()), Id("my_id".into())] }, declarations: Vec::new() },
    ];

    assert_eq!(parsed_css, Css { stylesheets: vec![expected_rules.into()] });
}

#[test]
fn test_case_issue_93() {

    use azul_css::*;
    use self::CssPathSelector::*;

    let parsed_css = new_from_str("
        .tabwidget-tab-label {
          color: #FFFFFF;
        }

        .tabwidget-tab.active .tabwidget-tab-label {
          color: #000000;
        }

        .tabwidget-tab.active .tabwidget-tab-close {
          color: #FF0000;
        }
    ").unwrap();

    fn declaration(classes: &[CssPathSelector], color: ColorU) -> CssRuleBlock {
        CssRuleBlock {
            path: CssPath {
                selectors: classes.to_vec(),
            },
            declarations: vec![CssDeclaration::Static(CssProperty::TextColor(
                CssPropertyValue::Exact(StyleTextColor(color)),
            ))],
        }
    }

    let expected_rules = vec![
        declaration(&[Class("tabwidget-tab-label".into())], ColorU { r: 255, g: 255, b: 255, a: 255 }),
        declaration(&[Class("tabwidget-tab".into()), Class("active".into()), Children, Class("tabwidget-tab-label".into())], ColorU { r: 0, g: 0, b: 0, a: 255 }),
        declaration(&[Class("tabwidget-tab".into()), Class("active".into()), Children, Class("tabwidget-tab-close".into())], ColorU { r: 255, g: 0, b: 0, a: 255 }),
    ];

    assert_eq!(parsed_css, Css { stylesheets: vec![expected_rules.into()] });
}