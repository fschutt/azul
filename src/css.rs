//! CSS parsing and styling
use std::ops::Add;
use css_parser::{ParsedCssProperty, CssParsingError};

#[cfg(target_os="windows")]
const NATIVE_CSS_WINDOWS: &str = include_str!("../assets/native_windows.css");
#[cfg(target_os="linux")]
const NATIVE_CSS_LINUX: &str = include_str!("../assets/native_linux.css");
#[cfg(target_os="macos")]
const NATIVE_CSS_MACOS: &str = include_str!("../assets/native_macos.css");

/// All the keys that, when changed, can trigger a re-layout
const RELAYOUT_RULES: [&str; 13] = [
    "border", "width", "height", "min-width", "min-height", "max-width", "max-height",
    "direction", "wrap", "justify-content", "align-items", "align-content",
    "order"
];

/// Wrapper for a `Vec<CssRule>` - the CSS is immutable at runtime, it can only be
/// created once. Animations / conditional styling is implemented using dynamic fields (see ``)
#[derive(Debug, Clone, PartialEq)]
pub struct Css<'a> {
    // NOTE: Each time the rules are modified, the `dirty` flag
    // has to be set accordingly for the CSS to update!
    pub(crate) rules: Vec<CssRule<'a>>,
    /*
    /// The dynamic properties that have to be set for this frame
    rules_to_change: FastHashMap<String, ParsedCssProperty>,
    */
    pub(crate) is_dirty: bool,
    /// Has the CSS changed in a way where it needs a re-layout?
    ///
    /// Ex. if only a background color has changed, we need to redraw, but we
    /// don't need to re-layout the frame
    pub(crate) needs_relayout: bool,
}

/// Error that can happen during the parsing of a CSS value
#[derive(Debug, Clone, PartialEq)]
pub enum CssParseError<'a> {
    /// A hard error in the CSS syntax
    ParseError(::simplecss::Error),
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
/// The CSS rule is currently not cascaded, use `Css::new_from_string()`
/// to do the cascading.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CssRule<'a> {
    /// `div` (`*` by default)
    pub html_type: String,
    /// `#myid` (`None` by default)
    pub id: Option<String>,
    /// `.myclass .myotherclass` (vec![] by default)
    pub classes: Vec<String>,
    /// `("justify-content", "center")`
    pub declaration: (String, CssDeclaration<'a>),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CssDeclaration<'a> {
    Static(ParsedCssProperty<'a>),
    Dynamic(DynamicCssProperty<'a>),
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
pub(crate) struct DynamicCssProperty<'a> {
    default: ParsedCssProperty<'a>,
    dynamic_id: String,
}

impl<'a> CssRule<'a> {
    pub fn needs_relayout(&self) -> bool {
        // RELAYOUT_RULES.iter().any(|r| self.declaration.0 == *r)
        // TODO
        true
    }
}

impl<'a> Css<'a> {

    /// Creates an empty set of CSS rules
    pub fn empty() -> Self {
        Self {
            rules: Vec::new(),
            is_dirty: false,
            needs_relayout: false,
        }
    }

    /// Parses a CSS string (single-threaded) and returns the parsed rules
    pub fn new_from_string(css_string: &'a str) -> Result<Self, CssParseError> {
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
                            println!("blockend!");
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
                            println!("declaration: key - {}\t\t| val - {}", key, val);

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
            // force repaint for the first frame
            is_dirty: true,
            // force re-layout for the first frame
            needs_relayout: true,
        })
    }

    /// Returns the native style for the OS
    #[cfg(target_os="windows")]
    pub fn native() -> Self {
        Self::new_from_string(NATIVE_CSS_WINDOWS).unwrap()
    }

    /// Returns the native style for the OS
    #[cfg(target_os="linux")]
    pub fn native() -> Self {
        Self::new_from_string(NATIVE_CSS_LINUX).unwrap()
    }

    /// Returns the native style for the OS
    #[cfg(target_os="macos")]
    pub fn native() -> Self {
        Self::new_from_string(NATIVE_CSS_MACOS).unwrap()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum DynamicCssParseError<'a> {
    UnclosedBraces,
    /// There is a valid dynamic css property, but no default case
    NoDefaultCase,
    /// The ID may not start with a number or be a CSS property itself
    InvalidId,
    /// The "default" ID has to be the second ID, not the first one.
    DefaultCaseNotSecond,
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
-> Result<CssDeclaration<'a>, DynamicCssParseError<'a>>
{
    // TODO: dynamic css declarations
    Ok(CssDeclaration::Static(ParsedCssProperty::from_kv(key, value)?))
}

#[test]
fn test_detect_static_or_dynamic_property() {
    use css_parser::TextAlignment;
    assert_eq!(
        determine_static_or_dynamic_css_property("text-align", " center   "),
        Ok(CssDeclaration::Static(ParsedCssProperty::TextAlign(TextAlignment::Center)))
    );

    assert_eq!(
        determine_static_or_dynamic_css_property("text-align", "{{    400px }}"),
        Err(DynamicCssParseError::NoDefaultCase)
    );

    assert_eq!(determine_static_or_dynamic_css_property("text-align", "{{  400px"),
        Err(DynamicCssParseError::UnclosedBraces)
    );

    assert_eq!(
        determine_static_or_dynamic_css_property("text-align", "{{  400px | 500px }}"),
        Err(DynamicCssParseError::InvalidId)
    );

    assert_eq!(
        determine_static_or_dynamic_css_property("text-align", "{{  hello | 500px }}"),
        Err(DynamicCssParseError::InvalidId)
    );

    assert_eq!(
        determine_static_or_dynamic_css_property("text-align", "{{  500px | hello }}"),
        Err(DynamicCssParseError::InvalidId)
    );
}