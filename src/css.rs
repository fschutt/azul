//! CSS parsing and styling

#[cfg(debug_assertions)]
use std::io::Error as IoError;
use std::{
    collections::BTreeMap,
    num::ParseIntError,
};
use {
    css_parser::{ParsedCssProperty, CssParsingError},
    error::CssSyntaxError,
    traits::Layout,
    ui_description::{UiDescription, StyledNode},
    dom::{NodeTypePath, NodeData, NodeTypePathParseError},
    ui_state::UiState,
    id_tree::NodeId,
};

/// CSS mimicking the OS-native look - Windows: `styles/native_windows.css`
#[cfg(target_os="windows")]
pub const NATIVE_CSS: &str = concat!(
    include_str!("styles/native_windows.css"),
    include_str!("styles/shared/table.css"),
);

/// CSS mimicking the OS-native look - Linux: `styles/native_windows.css`
#[cfg(target_os="linux")]
pub const NATIVE_CSS: &str = concat!(
    include_str!("styles/native_linux.css"),
    include_str!("styles/shared/table.css"),
);

/// CSS mimicking the OS-native look - Mac: `styles/native_macos.css`
#[cfg(target_os="macos")]
pub const NATIVE_CSS: &str = concat!(
    include_str!("styles/native_macos.css"),
    include_str!("styles/shared/table.css"),
);

/// Wrapper for a `Vec<CssRule>` - the CSS is immutable at runtime, it can only be
/// created once. Animations / conditional styling is implemented using dynamic fields
#[derive(Debug, Default, PartialEq, Clone)]
pub struct Css {
    /// Path to hot-reload the CSS file from
    #[cfg(debug_assertions)]
    pub hot_reload_path: Option<String>,
    /// When hot-reloading, should the CSS file be appended to the built-in, native styles
    /// (equivalent to `NATIVE_CSS + include_str!(hot_reload_path)`)? Default: false
    #[cfg(debug_assertions)]
    pub hot_reload_override_native: bool,
    /// The CSS rules making up the document - i.e the rules of the CSS sheet de-duplicated
    pub rules: Vec<CssRuleBlock>,
    /// Has the CSS changed in a way where it needs a re-layout? - default:
    /// `true` in order to force a re-layout on the first frame
    ///
    /// Ex. if only a background color has changed, we need to redraw, but we
    /// don't need to re-layout the frame.
    pub needs_relayout: bool,
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
    /// Error while parsing a pseudo selector (like `:aldkfja`)
    PseudoSelectorParseError(CssPseudoSelectorParseError<'a>),
    /// The path has to be either `*`, `div`, `p` or something like that
    NodeTypePath(NodeTypePathParseError<'a>),
}

impl_display!{ CssParseError<'a>, {
    ParseError(e) => format!("Parse Error: {:?}", e),
    UnclosedBlock => "Unclosed block",
    MalformedCss => "Malformed Css",
    DynamicCssParseError(e) => format!("Dynamic parsing error: {}", e),
    UnexpectedValue(e) => format!("Unexpected value: {}", e),
    PseudoSelectorParseError(e) => format!("Failed to parse pseudo-selector: {}", e),
    NodeTypePath(e) => format!("Failed to parse CSS selector path: {}", e),
}}

impl_from! { CssParsingError<'a>, CssParseError::UnexpectedValue }
impl_from! { DynamicCssParseError<'a>, CssParseError::DynamicCssParseError }
impl_from! { CssPseudoSelectorParseError<'a>, CssParseError::PseudoSelectorParseError }
impl_from! { NodeTypePathParseError<'a>, CssParseError::NodeTypePath }

/// Contains one parsed `key: value` pair, static or dynamic
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CssDeclaration {
    /// Static key-value pair, such as `width: 500px`
    Static(ParsedCssProperty),
    /// Dynamic key-value pair with default value, such as `width: [[ my_id | 500px ]]`
    Dynamic(DynamicCssProperty),
}

impl CssDeclaration {
    /// Determines if the property will be inherited (applied to the children)
    /// during the recursive application of the CSS on the DOM tree
    pub fn is_inheritable(&self) -> bool {
        use self::CssDeclaration::*;
        match self {
            Static(s) => s.is_inheritable(),
            Dynamic(d) => d.is_inheritable(),
        }
    }
}

/// A `DynamicCssProperty` is a type of CSS rule that can be changed on possibly
/// every frame by the Rust code - for example to implement an `On::Hover` behaviour.
///
/// The syntax for such a property looks like this:
///
/// ```no_run,ignore
/// #my_div {
///    padding: [[ my_dynamic_property_id | 400px ]];
/// }
/// ```
///
/// Azul will register a dynamic property with the key "my_dynamic_property_id"
/// and the default value of 400px. If the property gets overridden during one frame,
/// the overridden property takes precedence.
///
/// At runtime the CSS is immutable (which is a performance optimization - if we
/// can assume that the CSS never changes at runtime), we can do some optimizations on it.
/// Dynamic CSS properties can also be used for animations and conditional CSS
/// (i.e. `hover`, `focus`, etc.), thereby leading to cleaner code, since all of these
/// special cases now use one single API.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DynamicCssProperty {
    /// The stringified ID of this property, i.e. the `"my_id"` in `width: [[ my_id | 500px ]]`.
    pub dynamic_id: String,
    /// Default value, used if the CSS property isn't overridden in this frame
    /// i.e. the `500px` in `width: [[ my_id | 500px ]]`.
    pub default: DynamicCssPropertyDefault,
}

/// If this value is set to default, the CSS property will not exist if it isn't overriden.
/// An example where this is useful is when you want to say something like this:
///
/// `width: [[ 400px | auto ]];`
///
/// "If I set this property to width: 400px, then use exactly 400px. Otherwise use whatever the default width is."
/// If this property wouldn't exist, you could only set the default to "0px" or something like
/// that, meaning that if you don't override the property, then you'd set it to 0px - which is
/// different from `auto`, since `auto` has its width determined by how much space there is
/// available in the parent.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DynamicCssPropertyDefault  {
    Exact(ParsedCssProperty),
    Auto,
}

impl DynamicCssProperty {
    pub fn is_inheritable(&self) -> bool {
        // Dynamic CSS properties should not be inheritable,
        // since that could lead to bugs - you set a property in Rust, suddenly
        // the wrong UI component starts to react because it was inherited.
        false
    }
}

#[cfg(debug_assertions)]
#[derive(Debug)]
pub enum HotReloadError {
    Io(IoError, String),
    FailedToReload,
}

#[cfg(debug_assertions)]
impl_display! { HotReloadError, {
    Io(e, file) => format!("Failed to hot-reload CSS file: Io error: {} when loading file: \"{}\"", e, file),
    FailedToReload => "Failed to hot-reload CSS file",
}}

/// One block of rules that applies a bunch of rules to a "path" in the CSS, i.e.
/// `div#myid.myclass -> { ("justify-content", "center") }`
#[derive(Debug, Clone, PartialEq)]
pub struct CssRuleBlock {
    /// The path (full selector) of the CSS block
    pub path: CssPath,
    /// `"justify-content: center"` =>
    /// `CssDeclaration::Static(ParsedCssProperty::JustifyContent(LayoutJustifyContent::Center))`
    pub declarations: Vec<CssDeclaration>,
}

/// Represents a full CSS path:
/// `#div > .my_class:focus` =>
/// `[CssPathSelector::Type(NodeTypePath::Div), DirectChildren, CssPathSelector::Class("my_class"), CssPathSelector::PseudoSelector]`
#[derive(Debug, Clone, Hash, Default, PartialEq)]
pub struct CssPath {
    pub selectors: Vec<CssPathSelector>,
}

/// Has all the necessary information about the CSS path
pub struct HtmlCascadeInfo<'a, T: 'a + Layout> {
    node_data: &'a NodeData<T>,
    index_in_parent: usize,
    is_mouse_over: bool,
    is_mouse_pressed: bool,
    // parent node item count, is_hovered, is_focused, is_active
}

impl CssPath {

    /// Returns if the CSS path matches the DOM node (i.e. if the DOM node should be styled by that element)
    pub fn matches_html_element<'a, T: Layout>(&self, html_node: &HtmlCascadeInfo<'a, T>) -> bool {
        use self::CssPathSelector::*;

        let html_node_type = html_node.node_data.node_type.get_path();
        let html_classes = &html_node.node_data.classes;
        let html_ids = &html_node.node_data.ids;
        let index_in_parent = html_node.index_in_parent;

        // TODO: Later on, we'll need the full path of all selectors of all parents of this node,
        // but for now just match the current selector

        for selector in &self.selectors {
            match selector {
                Global => { },
                Type(t) => {
                    if html_node_type != *t {
                        return false;
                    }
                },
                Class(c) => {
                    if !html_classes.contains(c) {
                        return false;
                    }
                },
                Id(id) => {
                    if !html_ids.contains(id) {
                      return false;
                    }
                },
                PseudoSelector(CssPathPseudoSelector::First) => {
                    // Notice: index_in_parent is 1-indexed
                    if index_in_parent != 1 { return false; }
                },
                PseudoSelector(CssPathPseudoSelector::NthChild(x)) => {
                    if index_in_parent != *x { return false; }
                },
                DirectChildren | Children | PseudoSelector(_) => {
                    // TODO: for now
                    return false;
                },
            }
        }

        true
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum CssPathSelector {
    /// Represents the `*` selector
    Global,
    /// `div`, `p`, etc.
    Type(NodeTypePath),
    /// `.something`
    Class(String),
    /// `#something`
    Id(String),
    /// `:something`
    PseudoSelector(CssPathPseudoSelector),
    /// Represents the `>` selector
    DirectChildren,
    /// Represents the ` ` selector
    Children
}

impl Default for CssPathSelector { fn default() -> Self { CssPathSelector::Global } }

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum CssPathPseudoSelector {
    /// `:first`
    First,
    /// `:last`
    Last,
    /// `:nth-child`
    NthChild(usize),
    /// `:hover` - mouse is over element
    Hover,
    /// `:active` - mouse is pressed and over element
    Active,
    /// `:focus` - element has received focus
    Focus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CssPseudoSelectorParseError<'a> {
    UnknownSelector(&'a str),
    InvalidNthChild(ParseIntError),
    UnclosedBracesNthChild(&'a str),
}

impl<'a> From<ParseIntError> for CssPseudoSelectorParseError<'a> {
    fn from(e: ParseIntError) -> Self { CssPseudoSelectorParseError::InvalidNthChild(e) }
}

impl_display! { CssPseudoSelectorParseError<'a>, {
    UnknownSelector(e) => format!("Invalid CSS pseudo-selector: ':{}'", e),
    InvalidNthChild(e) => format!("Invalid :nth-child pseudo-selector: ':{}'", e),
    UnclosedBracesNthChild(e) => format!(":nth-child has unclosed braces: ':{}'", e),
}}

impl CssPathPseudoSelector {
    pub fn from_str<'a>(data: &'a str) -> Result<Self, CssPseudoSelectorParseError<'a>> {
        match data {
            "first" => Ok(CssPathPseudoSelector::First),
            "last" => Ok(CssPathPseudoSelector::Last),
            "hover" => Ok(CssPathPseudoSelector::Hover),
            "active" => Ok(CssPathPseudoSelector::Active),
            "focus" => Ok(CssPathPseudoSelector::Focus),
            other => {
                // TODO: move this into a seperate function
                if other.starts_with("nth-child") {
                    let mut nth_child = other.split("nth-child");
                    nth_child.next();
                    let mut nth_child_string = nth_child.next().ok_or(CssPseudoSelectorParseError::UnknownSelector(other))?;
                    nth_child_string.trim();
                    if !nth_child_string.starts_with("(") || !nth_child_string.ends_with(")") {
                        return Err(CssPseudoSelectorParseError::UnclosedBracesNthChild(other));
                    }

                    // Should the string be empty, then the `starts_with` and `ends_with` won't succeed
                    let mut nth_child_string = &nth_child_string[1..nth_child_string.len() - 1];
                    nth_child_string.trim();
                    let parsed = nth_child_string.parse::<usize>()?;
                    Ok(CssPathPseudoSelector::NthChild(parsed))
                } else {
                    Err(CssPseudoSelectorParseError::UnknownSelector(other))
                }
            },
        }
    }
}

#[test]
fn test_css_pseudo_selector_parse() {
    let ok_res = [
        ("first", CssPathPseudoSelector::First),
        ("last", CssPathPseudoSelector::Last),
        ("nth-child(4)", CssPathPseudoSelector::NthChild(4)),
        ("hover", CssPathPseudoSelector::Hover),
        ("active", CssPathPseudoSelector::Active),
        ("focus", CssPathPseudoSelector::Focus),
    ];

    let err = [
        ("asdf", CssPseudoSelectorParseError::UnknownSelector("asdf")),
        ("", CssPseudoSelectorParseError::UnknownSelector("")),
        ("nth-child(", CssPseudoSelectorParseError::UnclosedBracesNthChild("nth-child(")),
        ("nth-child)", CssPseudoSelectorParseError::UnclosedBracesNthChild("nth-child)")),
        // Can't test for ParseIntError because the fields are private.
        // This is an example on why you shouldn't use std::error::Error!
    ];

    for (s, a) in &ok_res {
        assert_eq!(CssPathPseudoSelector::from_str(s), Ok(*a));
    }

    for (s, e) in &err {
        assert_eq!(CssPathPseudoSelector::from_str(s), Err(e.clone()));
    }
}

impl Css {

    /// Parses a CSS string (single-threaded) and returns the parsed rules in blocks
    pub fn new_from_str<'a>(css_string: &'a str) -> Result<Self, CssParseError<'a>> {
        use simplecss::{Tokenizer, Token, Combinator};

        let mut tokenizer = Tokenizer::new(css_string);

        let mut css_blocks = Vec::new();

        // Used for error checking / checking for closed braces
        let mut parser_in_block = false;
        let mut block_nesting = 0_usize;

        // Current css paths (i.e. `div#id, .class, p` are stored here -
        // when the block is finished, all `current_rules` gets duplicated with
        // one path corresponding to one set of rules each).
        let mut current_paths = Vec::new();
        // Current CSS declarations
        let mut current_rules = Vec::new();
        // Keep track of the current path during parsing
        let mut last_path = Vec::new();

        loop {
            let tokenize_result = tokenizer.parse_next();
            match tokenize_result {
                Ok(token) => {
                    match token {
                        Token::BlockStart => {
                            if parser_in_block {
                                // multi-nested CSS blocks are currently not supported
                                return Err(CssParseError::MalformedCss);
                            }
                            parser_in_block = true;
                            block_nesting += 1;
                            current_paths.push(last_path.clone());
                            last_path.clear();
                        },
                        Token::Comma => {
                            current_paths.push(last_path.clone());
                            last_path.clear();
                        },
                        Token::BlockEnd => {
                            block_nesting -= 1;
                            if !parser_in_block {
                                return Err(CssParseError::MalformedCss);
                            }
                            parser_in_block = false;
                            for path in current_paths.drain(..) {
                                css_blocks.push(CssRuleBlock {
                                    path: CssPath { selectors: path },
                                    declarations: current_rules.clone(),
                                })
                            }
                            current_rules.clear();
                            last_path.clear(); // technically unnecessary, but just to be sure
                        },

                        // tokens that adjust the last_path
                        Token::UniversalSelector => {
                            if parser_in_block {
                                return Err(CssParseError::MalformedCss);
                            }
                            last_path.push(CssPathSelector::Global);
                        },
                        Token::TypeSelector(div_type) => {
                            if parser_in_block {
                                return Err(CssParseError::MalformedCss);
                            }
                            last_path.push(CssPathSelector::Type(NodeTypePath::from_str(div_type)?));
                        },
                        Token::IdSelector(id) => {
                            if parser_in_block {
                                return Err(CssParseError::MalformedCss);
                            }
                            last_path.push(CssPathSelector::Id(id.to_string()));
                        },
                        Token::ClassSelector(class) => {
                            if parser_in_block {
                                return Err(CssParseError::MalformedCss);
                            }
                            last_path.push(CssPathSelector::Class(class.to_string()));
                        },
                        Token::Combinator(Combinator::GreaterThan) => {
                            if parser_in_block {
                                return Err(CssParseError::MalformedCss);
                            }
                            last_path.push(CssPathSelector::DirectChildren);
                        },
                        Token::Combinator(Combinator::Space) => {
                            if parser_in_block {
                                return Err(CssParseError::MalformedCss);
                            }
                            last_path.push(CssPathSelector::Children);
                        },
                        Token::PseudoClass(pseudo_class) => {
                            if parser_in_block {
                                return Err(CssParseError::MalformedCss);
                            }
                            last_path.push(CssPathSelector::PseudoSelector(CssPathPseudoSelector::from_str(pseudo_class)?));
                        },
                        Token::Declaration(key, val) => {
                            if !parser_in_block {
                                return Err(CssParseError::MalformedCss);
                            }
                            current_rules.push(determine_static_or_dynamic_css_property(key, val)?);
                        },
                        Token::EndOfStream => {
                            break;
                        },
                        _ => {
                            // attributes, lang-attributes and @keyframes are not supported
                        }
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

        let mut css = Self {
            #[cfg(debug_assertions)]
            hot_reload_path: None,
            #[cfg(debug_assertions)]
            hot_reload_override_native: false,
            rules: css_blocks,
            // force re-layout for the first frame
            needs_relayout: true,
        };

        css.sort_by_specificity();

        Ok(css)
    }

    /// Sort the CSS rules by their weight, so that the rules are applied in the correct order
    pub fn sort_by_specificity(&mut self) {
        self.rules.sort_by(|a, b| get_specificity(&a.path).cmp(&get_specificity(&b.path)));
    }

    /// Returns the native style for the OS
    pub fn native() -> Self {
        Self::new_from_str(NATIVE_CSS).unwrap()
    }

    /// Same as `new_from_str`, but applies the OS-native styles first, before
    /// applying the user styles on top.
    pub fn override_native<'a>(css_string: &'a str) -> Result<Self, CssParseError<'a>> {
        let parsed = Self::new_from_str(css_string)?;
        let mut native = Self::native();
        native.merge(parsed);
        Ok(native)
    }

    // Combines two parsed stylesheets into one, appending the rules of
    // `other` after the rules of `self`. Overrides `self.hot_reload_path` with
    // `other.hot_reload_path`
    pub fn merge(&mut self, mut other: Self) {
        self.rules.append(&mut other.rules);
        self.needs_relayout = self.needs_relayout || other.needs_relayout;

        #[cfg(debug_assertions)] {
            self.hot_reload_path = other.hot_reload_path;
            self.hot_reload_override_native = other.hot_reload_override_native;
        }
    }

    /// **NOTE**: Only available in debug mode, can crash if the file isn't found
    #[cfg(debug_assertions)]
    pub fn hot_reload(file_path: &str) -> Result<Self, HotReloadError>  {
        use std::fs;
        let initial_css = fs::read_to_string(&file_path).map_err(|e| HotReloadError::Io(e, file_path.to_string()))?;
        let mut css = match Self::new_from_str(&initial_css) {
            Ok(o) => o,
            Err(e) => panic!("Hot reload CSS: Parsing error in file {}:\n{}\n", file_path, e),
        };
        css.hot_reload_path = Some(file_path.into());

        Ok(css)
    }

    /// Same as `hot_reload`, but applies the OS-native styles first, before
    /// applying the user styles on top.
    #[cfg(debug_assertions)]
    pub fn hot_reload_override_native(file_path: &str) -> Result<Self, HotReloadError> {
        use std::fs;
        let initial_css = fs::read_to_string(&file_path).map_err(|e| HotReloadError::Io(e, file_path.to_string()))?;
        let mut css = match Self::override_native(&initial_css) {
            Ok(o) => o,
            Err(e) => panic!("Hot reload CSS: Parsing error in file {}:\n{}\n", file_path, e),
        };
        css.hot_reload_path = Some(file_path.into());
        css.hot_reload_override_native = true;

        Ok(css)
    }

    #[cfg(debug_assertions)]
    pub(crate) fn reload_css(&mut self) {

        use std::fs;

        let file_path = if let Some(f) = &self.hot_reload_path {
            f.clone()
        } else {
            #[cfg(feature = "logging")] {
               error!("No file to hot-reload the CSS from!");
            }
            return;
        };

        #[allow(unused_variables)]
        let reloaded_css = match fs::read_to_string(&file_path) {
            Ok(o) => o,
            Err(e) => {
                #[cfg(feature = "logging")] {
                    error!("Failed to hot-reload \"{}\":\r\n{}\n", file_path, e);
                }
                return;
            },
        };

        let target_css = if self.hot_reload_override_native {
            format!("{}\r\n{}\n", NATIVE_CSS, reloaded_css)
        } else {
            reloaded_css
        };

        #[allow(unused_variables)]
        let mut css = match Self::new_from_str(&target_css) {
            Ok(o) => o,
            Err(e) => {
                #[cfg(feature = "logging")] {
                    error!("Failed to reload - parse error \"{}\":\r\n{}\n", file_path, e);
                }
                return;
            },
        };

        css.hot_reload_path = self.hot_reload_path.clone();
        css.hot_reload_override_native = self.hot_reload_override_native;

        *self = css;
    }
}

fn get_specificity(path: &CssPath) -> (usize, usize, usize) {
    // http://www.w3.org/TR/selectors/#specificity
    let id_count = path.selectors.iter().filter(|x|     if let CssPathSelector::Id(_) = x {     true } else { false }).count();
    let class_count = path.selectors.iter().filter(|x|  if let CssPathSelector::Class(_) = x {  true } else { false }).count();
    let div_count = path.selectors.iter().filter(|x|    if let CssPathSelector::Type(_) = x {   true } else { false }).count();
    (id_count, class_count, div_count)
}

/// Error that can happen during `ParsedCssProperty::from_kv`
#[derive(Debug, Clone, PartialEq)]
pub enum DynamicCssParseError<'a> {
    /// The braces of a dynamic CSS property aren't closed or unbalanced, i.e. ` [[ `
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

impl_display!{ DynamicCssParseError<'a>, {
    UnclosedBraces => "The braces of a dynamic CSS property aren't closed or unbalanced, i.e. ` [[ `",
    NoDefaultCase => "There is a valid dynamic css property, but no default case",
    NoId => "The dynamic CSS property has no ID, i.e. [[ 400px ]]",
    InvalidId => "The ID may not start with a number or be a CSS property itself",
    EmptyBraces => "Dynamic css property braces are empty, i.e. `[[ ]]`",
    UnexpectedValue(e) => format!("Unexpected value: {}", e),
}}

impl<'a> From<CssParsingError<'a>> for DynamicCssParseError<'a> {
    fn from(e: CssParsingError<'a>) -> Self {
        DynamicCssParseError::UnexpectedValue(e)
    }
}

const START_BRACE: &str = "[[";
const END_BRACE: &str = "]]";

/// Determine if a Css property is static (immutable) or if it can change
/// during the runtime of the program
fn determine_static_or_dynamic_css_property<'a>(key: &'a str, value: &'a str)
-> Result<CssDeclaration, DynamicCssParseError<'a>>
{
    let key = key.trim();
    let value = value.trim();

    let is_starting_with_braces = value.starts_with(START_BRACE);
    let is_ending_with_braces = value.ends_with(END_BRACE);

    match (is_starting_with_braces, is_ending_with_braces) {
        (true, false) | (false, true) => {
            Err(DynamicCssParseError::UnclosedBraces)
        },
        (true, true) => {
            parse_dynamic_css_property(key, value).and_then(|val| Ok(CssDeclaration::Dynamic(val)))
        },
        (false, false) => {
            Ok(CssDeclaration::Static(ParsedCssProperty::from_kv(key, value)?))
        }
    }
}

fn parse_dynamic_css_property<'a>(key: &'a str, value: &'a str) -> Result<DynamicCssProperty, DynamicCssParseError<'a>> {

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

    let default_case_parsed = match default_case {
        "auto" => DynamicCssPropertyDefault::Auto,
        other => DynamicCssPropertyDefault::Exact(ParsedCssProperty::from_kv(key, other)?),
    };

    Ok(DynamicCssProperty {
        dynamic_id: dynamic_id.to_string(),
        default: default_case_parsed,
    })
}

pub(crate) fn match_dom_css_selectors<T: Layout>(
    ui_state: &UiState<T>,
    css: &Css)
-> UiDescription<T>
{
    use ui_solver::get_non_leaf_nodes_sorted_by_depth;

    let root = ui_state.dom.root;
    let arena_borrow = &*ui_state.dom.arena.borrow();
    let non_leaf_nodes = get_non_leaf_nodes_sorted_by_depth(&arena_borrow.node_layout);

    let mut styled_nodes = BTreeMap::<NodeId, StyledNode>::new();

    for (_depth, parent_id) in non_leaf_nodes {

        // Note: starts at 1 instead of 0
        let index_in_parent = parent_id.preceding_siblings(&arena_borrow.node_layout).count();

        let parent_html_matcher = HtmlCascadeInfo {
            node_data: &arena_borrow.node_data[parent_id],
            index_in_parent: index_in_parent, // necessary for nth-child
            is_mouse_over: false, // TODO
            is_mouse_pressed: false, // TODO
        };

        let mut parent_rules = styled_nodes.get(&parent_id).cloned().unwrap_or_default();

        // Iterate through all rules in the CSS style sheet, test if the
        // This is technically O(n ^ 2), however, there are usually not that many CSS blocks,
        // so the cost of this should be insignificant.
        for applying_rule in css.rules.iter().filter(|rule| rule.path.matches_html_element(&parent_html_matcher)) {
            parent_rules.css_constraints.list.extend(applying_rule.declarations.clone());
        }

        let inheritable_rules: Vec<CssDeclaration> = parent_rules.css_constraints.list.iter().filter(|prop| prop.is_inheritable()).cloned().collect();

        // For children: inherit from parents - filter children that themselves are not parents!
        for (child_idx, child_id) in parent_id.children(&arena_borrow.node_layout).enumerate() {
            let child_node = &arena_borrow.node_layout[child_id];
            match child_node.first_child {
                None => {
                    // Style children that themselves aren't parents
                    let mut child_rules = inheritable_rules.clone();

                    let child_html_matcher = HtmlCascadeInfo {
                        node_data: &arena_borrow.node_data[child_id],
                        index_in_parent: child_idx + 1, // necessary for nth-child
                        is_mouse_over: false, // TODO
                        is_mouse_pressed: false, // TODO
                    };

                    // Iterate through all rules in the CSS style sheet, test if the
                    // This is technically O(n ^ 2), however, there are usually not that many CSS blocks,
                    // so the cost of this should be insignificant.
                    for applying_rule in css.rules.iter().filter(|rule| rule.path.matches_html_element(&child_html_matcher)) {
                        child_rules.extend(applying_rule.declarations.clone());
                    }

                    styled_nodes.insert(child_id, StyledNode { css_constraints:  CssConstraintList { list: child_rules }});
                },
                Some(_) => {
                    // For all children that themselves are parents, simply copy the inheritable rules
                    styled_nodes.insert(child_id, StyledNode { css_constraints:  CssConstraintList { list: inheritable_rules.clone() } });
                },
            }
        }

        styled_nodes.insert(parent_id, parent_rules);
    }

    UiDescription {
        // Note: this clone is necessary, otherwise,
        // we wouldn't be able to update the UiState
        //
        // WARNING: The UIState can modify the `arena` with its copy of the Rc !
        // Be careful about appending things to the arena, since that could modify
        // the UiDescription without you knowing!
        ui_descr_arena: ui_state.dom.arena.clone(),
        ui_descr_root: root,
        styled_nodes: styled_nodes,
        default_style_of_node: StyledNode::default(),
        dynamic_css_overrides: ui_state.dynamic_css_overrides.clone(),
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct CssConstraintList {
    pub(crate) list: Vec<CssDeclaration>
}

#[test]
fn test_detect_static_or_dynamic_property() {
    use css_parser::{StyleTextAlignmentHorz, InvalidValueErr};
    assert_eq!(
        determine_static_or_dynamic_css_property("text-align", " center   "),
        Ok(CssDeclaration::Static(ParsedCssProperty::TextAlign(StyleTextAlignmentHorz::Center)))
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
            default: DynamicCssPropertyDefault::Exact(ParsedCssProperty::TextAlign(StyleTextAlignmentHorz::Center)),
            dynamic_id: String::from("hello"),
        }))
    );

    assert_eq!(
        determine_static_or_dynamic_css_property("text-align", "[[  hello | auto ]]"),
        Ok(CssDeclaration::Dynamic(DynamicCssProperty {
            default: DynamicCssPropertyDefault::Auto,
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

#[test]
fn test_css_parse_1() {

    use prelude::{ColorU, StyleBackgroundColor};

    let parsed_css = Css::new_from_str("
        div#my_id .my_class:first {
            background-color: red;
        }
    ").unwrap();

    let expected_css = Css {
        rules: vec![
            CssRuleBlock {
                path: CssPath {
                    selectors: vec![
                        CssPathSelector::Type(NodeTypePath::Div),
                        CssPathSelector::Id(String::from("my_id")),
                        // NOTE: This is technically wrong, the space between "#my_id"
                        // and ".my_class" is important, but gets ignored for now
                        CssPathSelector::Children,
                        CssPathSelector::Class(String::from("my_class")),
                        CssPathSelector::PseudoSelector(CssPathPseudoSelector::First),
                    ],
                },
                declarations: vec![CssDeclaration::Static(ParsedCssProperty::BackgroundColor(StyleBackgroundColor(ColorU { r: 255, g: 0, b: 0, a: 255 })))],
            }
        ],
        needs_relayout: true,
        #[cfg(debug_assertions)]
        hot_reload_path: None,
        #[cfg(debug_assertions)]
        hot_reload_override_native: false,
    };

    assert_eq!(parsed_css, expected_css);
}

#[test]
fn test_css_simple_selector_parse() {
    use self::CssPathSelector::*;
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
    assert_eq!(Css::new_from_str(css).unwrap(), Css {
        rules: vec![CssRuleBlock {
            path: CssPath { selectors: parsed },
            declarations: Vec::new(),
        }],
        needs_relayout: true,
        #[cfg(debug_assertions)]
        hot_reload_path: None,
        #[cfg(debug_assertions)]
        hot_reload_override_native: false,
    });
}

#[cfg(test)]
mod cascade_tests {

    use prelude::*;
    use super::*;

    fn test_css(css: &str, ids: Vec<&str>, classes: Vec<&str>, expected: Vec<ParsedCssProperty>) {

        // Unimportant boilerplate
        struct Data { }

        impl Layout for Data { fn layout(&self) -> Dom<Self> { Dom::new(NodeType::Div) } }

        let css = Css::new_from_str(css).unwrap();
        let ids_str = ids.into_iter().map(|x| x.to_string()).collect();
        let class_str = classes.into_iter().map(|x| x.to_string()).collect();
        let node_data: NodeData<Data> = NodeData {
            node_type: NodeType::Div,
            ids: ids_str,
            classes: class_str,
            .. Default::default()
        };

        let test_node = HtmlCascadeInfo {
            node_data: &node_data,
            index_in_parent: 0,
            is_mouse_over: false,
            is_mouse_pressed: false,
        };

        let mut test_node_rules = Vec::new();
        for applying_rule in css.rules.iter().filter(|rule| rule.path.matches_html_element(&test_node)) {
            test_node_rules.extend(applying_rule.declarations.clone());
        }

        let expected_rules: Vec<CssDeclaration> = expected.into_iter().map(|x| CssDeclaration::Static(x)).collect();
        assert_eq!(test_node_rules, expected_rules);
    }

    // Tests that an element with a single class always gets the CSS element applied properly
    #[test]
    fn test_apply_css_pure_class() {
        let red = ParsedCssProperty::BackgroundColor(StyleBackgroundColor(ColorU { r: 255, g: 0, b: 0, a: 255 }));
        let blue = ParsedCssProperty::BackgroundColor(StyleBackgroundColor(ColorU { r: 0, g: 0, b: 255, a: 255 }));
        let black = ParsedCssProperty::BackgroundColor(StyleBackgroundColor(ColorU { r: 0, g: 0, b: 0, a: 255 }));

        // Test that single elements are applied properly
        {
            let css_1 = ".my_class { background-color: red; }";
            test_css(css_1, vec![], vec!["my_class"], vec![red.clone()]);
            test_css(css_1, vec!["my_id"], vec!["my_class"], vec![red.clone()]);
            test_css(css_1, vec!["my_id"], vec![], vec![]);
        }

        // Test that the ID overwrites the class (higher specificy)
        {
            let css_2 = "#my_id { background-color: red; } .my_class { background-color: blue; }";
            test_css(css_2, vec![], vec![], vec![]);
            test_css(css_2, vec!["my_id"], vec![], vec![red.clone()]);
            test_css(css_2, vec!["my_id"], vec!["my_class"], vec![blue.clone(), red.clone()]); // red will overwrite blue later on
            test_css(css_2, vec![], vec!["my_class"], vec![blue.clone()]);
        }

        // Global tests
        {
            let css_3 = "* { background-color: black; } .my_class#my_id { background-color: red; } .my_class { background-color: blue; }";
            test_css(css_3, vec![], vec![], vec![black.clone()]);
            test_css(css_3, vec!["my_id"], vec![], vec![black.clone()]); // note: .my_class#my_id
            test_css(css_3, vec![], vec!["my_class"], vec![black.clone(), blue.clone()]);
            test_css(css_3, vec!["my_id"], vec!["my_class"], vec![black.clone(), blue.clone(), red.clone()]);
            test_css(css_3, vec![], vec!["my_class"], vec![black.clone(), blue.clone()]);
        }
    }
}

#[test]
fn test_specificity() {
    use self::CssPathSelector::*;
    assert_eq!(get_specificity(&CssPath { selectors: vec![Id("hello".into())] }), (1, 0, 0));
    assert_eq!(get_specificity(&CssPath { selectors: vec![Class("hello".into())] }), (0, 1, 0));
    assert_eq!(get_specificity(&CssPath { selectors: vec![Type(NodeTypePath::Div)] }), (0, 0, 1));
    assert_eq!(get_specificity(&CssPath { selectors: vec![Id("hello".into()), Type(NodeTypePath::Div)] }), (1, 0, 1));
}

// Assert that order of the CSS items is correct (in order of specificity, lowest-to-highest)
#[test]
fn test_specificity_sort() {
    use prelude::*;
    use self::CssPathSelector::*;
    use dom::NodeTypePath::*;

    let parsed_css = Css::new_from_str("
        * { }
        * div.my_class#my_id { }
        * div#my_id { }
        * #my_id { }
        div.my_class.specific#my_id { }
    ").unwrap();

    let expected_css = Css {
        rules: vec![
            // Rules are sorted from lowest-specificity to highest specificity
            CssRuleBlock { path: CssPath { selectors: vec![Global] }, declarations: Vec::new() },
            CssRuleBlock { path: CssPath { selectors: vec![Global, Id("my_id".into())] }, declarations: Vec::new() },
            CssRuleBlock { path: CssPath { selectors: vec![Global, Type(Div), Id("my_id".into())] }, declarations: Vec::new() },
            CssRuleBlock { path: CssPath { selectors: vec![Global, Type(Div), Class("my_class".into()), Id("my_id".into())] }, declarations: Vec::new() },
            CssRuleBlock { path: CssPath { selectors: vec![Type(Div), Class("my_class".into()), Class("specific".into()), Id("my_id".into())] }, declarations: Vec::new() },
        ],
        needs_relayout: true,
        #[cfg(debug_assertions)]
        hot_reload_path: None,
        #[cfg(debug_assertions)]
        hot_reload_override_native: false,
    };

    assert_eq!(parsed_css, expected_css);
}