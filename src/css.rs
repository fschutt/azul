//! CSS parsing and styling
use {
    FastHashMap,
    traits::IntoParsedCssProperty,
    css_parser::{ParsedCssProperty, CssParsingError},
    errors::CssSyntaxError,
};

#[cfg(target_os="windows")]
const NATIVE_CSS_WINDOWS: &str = include_str!("styles/native_windows.css");
#[cfg(target_os="linux")]
const NATIVE_CSS_LINUX: &str = include_str!("styles/native_linux.css");
#[cfg(target_os="macos")]
const NATIVE_CSS_MACOS: &str = include_str!("styles/native_macos.css");

/// All the keys that, when changed, can trigger a re-layout
const RELAYOUT_RULES: [&str; 13] = [
    "border", "width", "height", "min-width", "min-height", "max-width", "max-height",
    "direction", "wrap", "justify-content", "align-items", "align-content",
    "order"
];

/// Wrapper for a `Vec<CssRule>` - the CSS is immutable at runtime, it can only be
/// created once. Animations / conditional styling is implemented using dynamic fields
#[derive(Debug, Clone, PartialEq)]
pub struct Css {
    pub(crate) rules: Vec<CssRule>,
    /// The dynamic properties that have to be overridden for this frame
    ///
    /// - `String`: The ID of the dynamic property
    /// - `ParsedCssProperty`: What to override it with
    pub(crate) dynamic_css_overrides: FastHashMap<String, ParsedCssProperty>,
    /// Has the CSS changed in a way where it needs a re-layout?
    ///
    /// Ex. if only a background color has changed, we need to redraw, but we
    /// don't need to re-layout the frame
    pub(crate) needs_relayout: bool,
}

/// Fake CSS that can be changed by the user
#[derive(Debug, Default, Clone)]
pub struct FakeCss {
    pub dynamic_css_overrides: FastHashMap<String, ParsedCssProperty>,
}

impl FakeCss {
    /// Set a dynamic CSS property for the duration of one frame
    pub fn set_dynamic_property<'a, S, T>(&mut self, id: S, css_value: T)
    -> Result<(), CssParsingError<'a>>
    where S: Into<String>,
          T: IntoParsedCssProperty<'a>,
    {
        let value = css_value.into_parsed_css_property()?;
        self.dynamic_css_overrides.insert(id.into(), value);
        Ok(())
    }

    /// Library-internal only: clear the dynamic overrides
    ///
    /// Is usually invoked at the end of the frame, to get a clean slate
    pub(crate) fn clear(&mut self) {
        self.dynamic_css_overrides = FastHashMap::default();
    }
}

/// Error that can happen during the parsing of a CSS value
#[derive(Debug, Clone, PartialEq)]
pub enum CssParseError<'a> {
    /// A hard error in the CSS syntax
    ParseError(CssSyntaxError),
    /// Braces are not balanced properly
    UnclosedBlock,
    /// Invalid syntax, such as `#div { #div: "my-value" }`
    MalformedCss,
    /// Error parsing dynamic CSS property, such as
    /// `#div { width: {{ my_id }} /* no default case */ }`
    DynamicCssParseError(DynamicCssParseError<'a>),
    /// Error during parsing the value of a field
    /// (Css is parsed eagerly, directly converted to strongly typed values
    /// as soon as possible)
    UnexpectedValue(CssParsingError<'a>),
}

impl<'a> From<CssParsingError<'a>> for CssParseError<'a> {
    fn from(e: CssParsingError<'a>) -> Self {
        CssParseError::UnexpectedValue(e)
    }
}

impl<'a> From<DynamicCssParseError<'a>> for CssParseError<'a> {
    fn from(e: DynamicCssParseError<'a>) -> Self {
        CssParseError::DynamicCssParseError(e)
    }
}

/// Rule that applies to some "path" in the CSS, i.e.
/// `div#myid.myclass -> ("justify-content", "center")`
///
/// The CSS rule is currently not cascaded, use `Css::new_from_str()`
/// to do the cascading.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CssRule {
    /// `div` (`*` by default)
    pub html_type: String,
    /// `#myid` (`None` by default)
    pub id: Option<String>,
    /// `.myclass .myotherclass` (vec![] by default)
    pub classes: Vec<String>,
    /// `("justify-content", "center")`
    pub declaration: (String, CssDeclaration),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CssDeclaration {
    Static(ParsedCssProperty),
    Dynamic(DynamicCssProperty),
}

/// A `CssProperty` is a type of CSS Rule,
/// but the contents of the rule is dynamic.
///
/// Azul has "dynamic properties", i.e.:
///
/// ```no_run,ignore
/// #my_div {
///    padding: {{ my_dynamic_property_id | 400px }};
/// }
/// ```
///
/// At runtime the CSS is immutable (which is a performance optimization - if we
/// can assume that the CSS never changes at runtime), we can do some optimizations on it.
/// Also it leads to cleaner code, since both animations and conditional CSS styling
/// now use the same API.
///
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct DynamicCssProperty {
    pub(crate) dynamic_id: String,
    pub(crate) default: ParsedCssProperty,
}

impl CssRule {
    pub fn needs_relayout(&self) -> bool {
        // RELAYOUT_RULES.iter().any(|r| self.declaration.0 == *r)
        // TODO
        true
    }
}

impl Css {

    /// Creates an empty set of CSS rules
    pub fn empty() -> Self {
        Self {
            rules: Vec::new(),
            needs_relayout: false,
            dynamic_css_overrides: FastHashMap::default(),
        }
    }

    /// Parses a CSS string (single-threaded) and returns the parsed rules
    pub fn new_from_str<'a>(css_string: &'a str) -> Result<Self, CssParseError<'a>> {
        use simplecss::{Tokenizer, Token};
        use std::collections::HashSet;

        let mut tokenizer = Tokenizer::new(css_string);

        let mut block_nesting = 0_usize;
        let mut css_rules = Vec::<CssRule>::new();

        // TODO: For now, rules may not be nested, otherwise, this won't work
        // TODO: This could be more efficient. We don't even need to clone the
        // strings, but this is just a quick-n-dirty CSS parser
        // This will also use up a lot of memory, since the strings get duplicated

        let mut parser_in_block = false;
        let mut current_type = "*";
        let mut current_id = None;
        let mut current_classes = HashSet::<&str>::new();

        'css_parse_loop: loop {
            let tokenize_result = tokenizer.parse_next();
            match tokenize_result {
                Ok(token) => {
                    match token {
                        Token::EndOfStream => {
                            break 'css_parse_loop;
                        },
                        Token::BlockStart => {
                            parser_in_block = true;
                            block_nesting += 1;
                        },
                        Token::BlockEnd => {
                            block_nesting -= 1;
                            parser_in_block = false;
                            current_type = "*";
                            current_id = None;
                            current_classes = HashSet::<&str>::new();
                        },
                        Token::TypeSelector(div_type) => {
                            if parser_in_block {
                                return Err(CssParseError::MalformedCss);
                            }
                            current_type = div_type;
                        },
                        Token::IdSelector(id) => {
                            if parser_in_block {
                                return Err(CssParseError::MalformedCss);
                            }
                            current_id = Some(id.to_string());
                        }
                        Token::ClassSelector(class) => {
                            if parser_in_block {
                                return Err(CssParseError::MalformedCss);
                            }
                            current_classes.insert(class);
                        }
                        Token::Declaration(key, val) => {
                            if !parser_in_block {
                                return Err(CssParseError::MalformedCss);
                            }

                            // see if the Declaration is static or dynamic
                            //
                            // css_val = "center" | "{{ my_dynamic_id | center }}"
                            let css_decl = determine_static_or_dynamic_css_property(key, val)?;
                            let mut css_rule = CssRule {
                                html_type: current_type.to_string(),
                                id: current_id.clone(),
                                classes: current_classes.iter().map(|e| e.to_string()).collect::<Vec<String>>(),
                                declaration: (key.to_string(), css_decl),
                            };
                            // IMPORTANT!
                            css_rule.classes.sort();
                            css_rules.push(css_rule);
                        },
                        _ => { }
                    }
                },
                Err(e) => {
                    return Err(CssParseError::ParseError(e));
                }
            }
        }

        // non-even number of blocks
        if block_nesting != 0 {
            return Err(CssParseError::UnclosedBlock);
        }

        Ok(Self {
            rules: css_rules,
            // force re-layout for the first frame
            needs_relayout: true,
            dynamic_css_overrides: FastHashMap::default(),
        })
    }

    /// Returns the native style for the OS
    #[cfg(target_os="windows")]
    pub fn native() -> Self {
        Self::new_from_str(NATIVE_CSS_WINDOWS).unwrap()
    }

    /// Returns the native style for the OS
    #[cfg(target_os="linux")]
    pub fn native() -> Self {
        Self::new_from_str(NATIVE_CSS_LINUX).unwrap()
    }

    /// Returns the native style for the OS
    #[cfg(target_os="macos")]
    pub fn native() -> Self {
        Self::new_from_str(NATIVE_CSS_MACOS).unwrap()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DynamicCssParseError<'a> {
    UnclosedBraces,
    /// There is a valid dynamic css property, but no default case
    NoDefaultCase,
    /// The dynamic CSS property has no ID, i.e. `[[ 400px ]]`
    NoId,
    /// The ID may not start with a number or be a CSS property itself
    InvalidId,
    /// Dynamic css property braces are empty, i.e. `[[ ]]`
    EmptyBraces,
    /// Unexpected value when parsing the string
    UnexpectedValue(CssParsingError<'a>),
}

impl<'a> From<CssParsingError<'a>> for DynamicCssParseError<'a> {
    fn from(e: CssParsingError<'a>) -> Self {
        DynamicCssParseError::UnexpectedValue(e)
    }
}

/// Determine if a Css property is static (immutable) or if it can change
/// during the runtime of the program
fn determine_static_or_dynamic_css_property<'a>(key: &'a str, value: &'a str)
-> Result<CssDeclaration, DynamicCssParseError<'a>>
{
    let key = key.trim();
    let value = value.trim();

    const START_BRACE: &str = "[[";
    const END_BRACE: &str = "]]";

    let is_starting_with_braces = value.starts_with(START_BRACE);
    let is_ending_with_braces = value.ends_with(END_BRACE);

    match (is_starting_with_braces, is_ending_with_braces) {
        (true, false) | (false, true) => {
            Err(DynamicCssParseError::UnclosedBraces)
        },
        (true, true) => {

            use std::char;

            // "[[ id | 400px ]]" => "id | 400px"
            let value = value.trim_left_matches(START_BRACE);
            let value = value.trim_right_matches(END_BRACE);
            let value = value.trim();

            let mut pipe_split = value.splitn(2, "|");
            let dynamic_id = pipe_split.next();
            let default_case = pipe_split.next();

            // note: dynamic_id will always be Some(), which is why the
            let (default_case, dynamic_id) = match (default_case, dynamic_id) {
                (Some(default), Some(id)) => (default, id),
                (None, Some(id)) => {
                    if id.trim().is_empty() {
                        return Err(DynamicCssParseError::EmptyBraces);
                    } else if ParsedCssProperty::from_kv(key, id).is_ok() {
                        // if there is an ID, but the ID is a CSS value
                        return Err(DynamicCssParseError::NoId);
                    } else {
                        return Err(DynamicCssParseError::NoDefaultCase);
                    }
                },
                (None, None) | (Some(_), None) => unreachable!(), // iterator would be broken if this happened
            };

            let dynamic_id = dynamic_id.trim();
            let default_case = default_case.trim();

            match (dynamic_id.is_empty(), default_case.is_empty()) {
                (true, true) => return Err(DynamicCssParseError::EmptyBraces),
                (true, false) => return Err(DynamicCssParseError::NoId),
                (false, true) => return Err(DynamicCssParseError::NoDefaultCase),
                (false, false) => { /* everything OK */ }
            }

            if dynamic_id.starts_with(char::is_numeric) ||
               ParsedCssProperty::from_kv(key, dynamic_id).is_ok() {
                return Err(DynamicCssParseError::InvalidId);
            }

            let default_case_parsed = ParsedCssProperty::from_kv(key, default_case)?;

            Ok(CssDeclaration::Dynamic(DynamicCssProperty {
                dynamic_id: dynamic_id.to_string(),
                default: default_case_parsed,
            }))
        },
        (false, false) => {
            Ok(CssDeclaration::Static(ParsedCssProperty::from_kv(key, value)?))
        }
    }
}

#[test]
fn test_detect_static_or_dynamic_property() {
    use css_parser::{TextAlignmentHorz, InvalidValueErr};
    assert_eq!(
        determine_static_or_dynamic_css_property("text-align", " center   "),
        Ok(CssDeclaration::Static(ParsedCssProperty::TextAlign(TextAlignmentHorz::Center)))
    );

    assert_eq!(
        determine_static_or_dynamic_css_property("text-align", "[[    400px ]]"),
        Err(DynamicCssParseError::NoDefaultCase)
    );

    assert_eq!(determine_static_or_dynamic_css_property("text-align", "[[  400px"),
        Err(DynamicCssParseError::UnclosedBraces)
    );

    assert_eq!(
        determine_static_or_dynamic_css_property("text-align", "[[  400px | center ]]"),
        Err(DynamicCssParseError::InvalidId)
    );

    assert_eq!(
        determine_static_or_dynamic_css_property("text-align", "[[  hello | center ]]"),
        Ok(CssDeclaration::Dynamic(DynamicCssProperty {
            default: ParsedCssProperty::TextAlign(TextAlignmentHorz::Center),
            dynamic_id: String::from("hello"),
        }))
    );

    assert_eq!(
        determine_static_or_dynamic_css_property("text-align", "[[  abc | hello ]]"),
        Err(DynamicCssParseError::UnexpectedValue(
            CssParsingError::InvalidValueErr(InvalidValueErr("hello"))
        ))
    );

    assert_eq!(
        determine_static_or_dynamic_css_property("text-align", "[[ ]]"),
        Err(DynamicCssParseError::EmptyBraces)
    );
    assert_eq!(
        determine_static_or_dynamic_css_property("text-align", "[[]]"),
        Err(DynamicCssParseError::EmptyBraces)
    );


    assert_eq!(
        determine_static_or_dynamic_css_property("text-align", "[[ center ]]"),
        Err(DynamicCssParseError::NoId)
    );

    assert_eq!(
        determine_static_or_dynamic_css_property("text-align", "[[ hello |  ]]"),
        Err(DynamicCssParseError::NoDefaultCase)
    );

    // debatable if this is a suitable error for this case:
    assert_eq!(
        determine_static_or_dynamic_css_property("text-align", "[[ |  ]]"),
        Err(DynamicCssParseError::EmptyBraces)
    );
}