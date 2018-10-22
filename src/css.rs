//! CSS parsing and styling

#[cfg(debug_assertions)]
use std::io::Error as IoError;
use std::{
    collections::BTreeMap,
    ops::{Deref, DerefMut},
};
use {
    FastHashMap,
    traits::IntoParsedCssProperty,
    css_parser::{ParsedCssProperty, CssParsingError},
    error::CssSyntaxError,
    id_tree::{NodeId, Arena},
    traits::Layout,
    ui_description::{UiDescription, StyledNode, CssConstraintList},
    dom::NodeData,
    ui_state::UiState,
};

/// CSS mimicking the OS-native look - Windows: `styles/native_windows.css`
#[cfg(target_os="windows")]
pub const NATIVE_CSS: &str = include_str!("styles/native_windows.css");
/// CSS mimicking the OS-native look - Linux: `styles/native_windows.css`
#[cfg(target_os="linux")]
pub const NATIVE_CSS: &str = include_str!("styles/native_linux.css");
/// CSS mimicking the OS-native look - Mac: `styles/native_macos.css`
#[cfg(target_os="macos")]
pub const NATIVE_CSS: &str = include_str!("styles/native_macos.css");

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
    /// Path to hot-reload the CSS file from
    #[cfg(debug_assertions)]
    pub hot_reload_path: Option<String>,
    /// When hot-reloading, should the CSS file be appended to the built-in, native styles
    /// (equivalent to `NATIVE_CSS + include_str!(hot_reload_path)`)? Default: false
    #[cfg(debug_assertions)]
    pub hot_reload_override_native: bool,
    /// The CSS rules making up the document
    pub rules: Vec<CssRule>,
    /// The dynamic properties that have to be overridden for this frame
    ///
    /// - `String`: The ID of the dynamic property
    /// - `ParsedCssProperty`: What to override it with
    pub dynamic_css_overrides: DynamicCssOverrideList,
    /// Has the CSS changed in a way where it needs a re-layout? - default:
    /// `true` in order to force a re-layout on the first frame
    ///
    /// Ex. if only a background color has changed, we need to redraw, but we
    /// don't need to re-layout the frame.
    pub needs_relayout: bool,
}

#[derive(Default, Debug, Clone, PartialEq)]
pub struct DynamicCssOverrideList {
    pub inner: FastHashMap<String, ParsedCssProperty>,
}

impl Deref for DynamicCssOverrideList {
    type Target = FastHashMap<String, ParsedCssProperty>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for DynamicCssOverrideList {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

/// Fake CSS containing the dynamic CSS properties for this frame -
/// can be changed by the user to override styles if needed
#[derive(Debug, Default, Clone)]
pub struct FakeCss {
    pub dynamic_css_overrides: DynamicCssOverrideList,
}

impl FakeCss {
    /// Set a dynamic CSS property for the duration of one frame. You can
    /// access the dynamic property on a window via `app_state.windows[event.window].css`.
    ///
    /// You can set dynamic properties from either a string or directly, however,
    /// setting them directly avoids re-parsing the string:
    ///
    /// ```rust
    /// # use azul::prelude::*;
    /// let mut fake_css = FakeCss::default();
    /// fake_css.set_dynamic_property("my_id", ("width", "500px")).unwrap();
    /// fake_css.set_dynamic_property("my_id", ParsedCssProperty::Width(LayoutWidth(PixelValue::px(500.0)))).unwrap();
    /// ```
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
        self.dynamic_css_overrides.clear();
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

impl_display!{ CssParseError<'a>, {
    ParseError(e) => format!("Parse Error: {:?}", e),
    UnclosedBlock => "Unclosed block",
    MalformedCss => "Malformed Css",
    DynamicCssParseError(e) => format!("Dynamic parsing error: {}", e),
    UnexpectedValue(e) => format!("Unexpected value: {}", e),
}}

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
pub struct CssRule {
    /// `div` (`*` by default)
    pub html_type: String,
    /// `#myid` (`None` by default)
    pub id: Option<String>,
    /// `.myclass .myotherclass` (vec![] by default)
    pub classes: Vec<String>,
    /// `("justify-content", "center")`
    pub declaration: (String, CssDeclaration),
}

/// Contains one parsed `key: value` pair, static or dynamic
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
pub struct DynamicCssProperty {
    /// The stringified ID of this property, i.e. the `"my_id"` in `width: [[ my_id | 500px ]]`.
    pub dynamic_id: String,
    /// Default value, used if the CSS property isn't overridden in this frame
    /// i.e. the `500px` in `width: [[ my_id | 500px ]]`.
    pub default: ParsedCssProperty,
}

impl DynamicCssProperty {
    pub fn is_inheritable(&self) -> bool {
        // Since the overridden value has to have the same enum type
        // we can just check if the default value is inheritable
        self.default.is_inheritable()
    }
}

impl CssRule {
    pub fn needs_relayout(&self) -> bool {
        // RELAYOUT_RULES.iter().any(|r| self.declaration.0 == *r)
        // TODO
        true
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

impl Css {

    /// Creates an empty set of CSS rules
    pub fn empty() -> Self {
        Self {
            #[cfg(debug_assertions)]
            hot_reload_path: None,
            #[cfg(debug_assertions)]
            hot_reload_override_native: false,
            rules: Vec::new(),
            needs_relayout: false,
            dynamic_css_overrides: DynamicCssOverrideList::default(),
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
        let mut current_pseudo_selector = None;

        loop {
            let tokenize_result = tokenizer.parse_next();
            match tokenize_result {
                Ok(token) => {
                    match token {
                        Token::EndOfStream => {
                            break;
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
                            current_pseudo_selector = None;
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
                            // ignore any :hover, :focus, etc. for now
                            if current_pseudo_selector.is_some() {
                                continue;
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
                        Token::PseudoClass(pseudo_class) => {
                            if parser_in_block {
                                return Err(CssParseError::MalformedCss);
                            }
                            current_pseudo_selector = Some(pseudo_class);
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
            #[cfg(debug_assertions)]
            hot_reload_path: None,
            #[cfg(debug_assertions)]
            hot_reload_override_native: false,
            rules: css_rules,
            // force re-layout for the first frame
            needs_relayout: true,
            dynamic_css_overrides: DynamicCssOverrideList::default(),
        })
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
        for (id, property) in other.dynamic_css_overrides.inner {
            self.dynamic_css_overrides.insert(id, property);
        }
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

        let mut parsed_css = match Self::new_from_str(&target_css) {
            Ok(o) => o,
            Err(e) => {
                #[cfg(feature = "logging")] {
                    error!("Failed to reload - parse error \"{}\":\r\n{}\n", file_path, e);
                }
                return;
            },
        };

        parsed_css.hot_reload_path = self.hot_reload_path.clone();
        parsed_css.dynamic_css_overrides = self.dynamic_css_overrides.clone();
        parsed_css.hot_reload_override_native = self.hot_reload_override_native;

        *self = parsed_css;
    }
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

/// CSS rules, sorted and grouped by priority
#[derive(Debug, Default, Clone)]
pub struct ParsedCss {
    pub(crate) pure_global_rules: Vec<CssRule>,
    pub(crate) pure_div_rules: Vec<CssRule>,
    pub(crate) pure_class_rules: Vec<CssRule>,
    pub(crate) pure_id_rules: Vec<CssRule>,
}

impl ParsedCss {
    /// Takes a `Css` struct and groups the types by their priority.
    pub fn from_css(css: &Css) -> Self {

        // Parse the CSS nodes cascading by their importance
        // 1. global rules
        // 2. div-type ("html { }") specific rules
        // 3. class-based rules
        // 4. ID-based rules

        /*
            CssRule { html_type: "div", id: Some("main"), classes: [], declaration: ("direction", "row") }
            CssRule { html_type: "div", id: Some("main"), classes: [], declaration: ("justify-content", "center") }
            CssRule { html_type: "div", id: Some("main"), classes: [], declaration: ("align-items", "center") }
            CssRule { html_type: "div", id: Some("main"), classes: [], declaration: ("align-content", "center") }
        */

        // note: the following passes can be done in parallel ...

        // Global rules
        // * {
        //    background-color: blue;
        // }
        let pure_global_rules: Vec<CssRule> = css.rules.iter().cloned().filter(|rule|
            rule.html_type == "*" && rule.id.is_none() && rule.classes.is_empty()
        ).collect();

        // Pure-div-type specific rules
        // button {
        //    justify-content: center;
        // }
        let pure_div_rules: Vec<CssRule> = css.rules.iter().cloned().filter(|rule|
            rule.html_type != "*" && rule.id.is_none() && rule.classes.is_empty()
        ).collect();

        // Pure-class rules
        // NOTE: These classes are sorted alphabetically and are not duplicated
        //
        // .something .otherclass {
        //    text-color: red;
        // }
        let pure_class_rules: Vec<CssRule> = css.rules.iter().cloned().filter(|rule|
            rule.id.is_none() && !rule.classes.is_empty()
        ).collect();

        // Pure-id rules
        // #something {
        //    background-color: red;
        // }
        let pure_id_rules: Vec<CssRule> = css.rules.iter().cloned().filter(|rule|
            rule.id.is_some() && rule.classes.is_empty()
        ).collect();

        Self {
            pure_global_rules: pure_global_rules,
            pure_div_rules: pure_div_rules,
            pure_class_rules: pure_class_rules,
            pure_id_rules: pure_id_rules,
        }
    }
}

/// Represents the z-index as defined by the stacking order
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct ZIndex(pub u32);

impl Default for ZIndex { fn default() -> Self { ZIndex(0) }}

pub(crate) fn match_dom_css_selectors<T: Layout>(
    ui_state: &UiState<T>,
    parsed_css: &ParsedCss,
    dynamic_css_overrides: &DynamicCssOverrideList,
    parent_z_level: ZIndex)
-> UiDescription<T>
{
    let root = ui_state.dom.root;
    let arena_borrow = &*ui_state.dom.arena.borrow();

    let mut root_constraints = CssConstraintList::default();
    let mut styled_nodes = BTreeMap::<NodeId, StyledNode>::new();

    for global_rule in &parsed_css.pure_global_rules {
        root_constraints.push_rule(global_rule);
    }

    let sibling_iterator = root.following_siblings(arena_borrow);
    // skip the root node itself, see documentation for `following_siblings` in id_tree.rs
    // sibling_iterator.next().unwrap();

    for sibling in sibling_iterator {
        styled_nodes.append(&mut match_dom_css_selectors_inner(sibling, arena_borrow, parsed_css, &root_constraints, parent_z_level));
    }

    UiDescription {
        // note: this clone is neccessary, otherwise,
        // we wouldn't be able to update the UiState
        //
        // WARNING: The UIState can modify the `arena` with its copy of the Rc !
        // Be careful about appending things to the arena, since that could modify
        // the UiDescription without you knowing!
        ui_descr_arena: ui_state.dom.arena.clone(),
        ui_descr_root: root,
        styled_nodes: styled_nodes,
        default_style_of_node: StyledNode::default(),
        dynamic_css_overrides: dynamic_css_overrides.clone(),
    }
}

fn match_dom_css_selectors_inner<T: Layout>(
    root: NodeId,
    arena: &Arena<NodeData<T>>,
    parsed_css: &ParsedCss,
    parent_constraints: &CssConstraintList,
    parent_z_level: ZIndex)
-> BTreeMap<NodeId, StyledNode>
{
    let mut styled_nodes = BTreeMap::<NodeId, StyledNode>::new();

    let mut current_constraints = CssConstraintList {
        list: parent_constraints.list.iter().filter(|prop| prop.is_inheritable()).cloned().collect(),
    };

    cascade_constraints(&arena[root].data, &mut current_constraints, parsed_css);

    let current_node = StyledNode {
        z_level: parent_z_level,
        css_constraints: current_constraints,
    };

    // DFS tree
    for child in root.children(arena) {
        styled_nodes.append(&mut match_dom_css_selectors_inner(child, arena, parsed_css, &current_node.css_constraints, ZIndex(parent_z_level.0 + 1)));
    }

    styled_nodes.insert(root, current_node);
    styled_nodes
}

/// Cascade the rules, put them into the list
#[allow(unused_variables)]
fn cascade_constraints<T: Layout>(
    node: &NodeData<T>,
    list: &mut CssConstraintList,
    parsed_css: &ParsedCss)
{
    for div_rule in &parsed_css.pure_div_rules {
        if *node.node_type.get_css_id() == div_rule.html_type {
            list.push_rule(div_rule);
        }
    }

    let mut node_classes: Vec<&String> = node.classes.iter().map(|x| x).collect();
    node_classes.sort();
    node_classes.dedup_by(|a, b| *a == *b);

    // for all classes that this node has
    for class_rule in &parsed_css.pure_class_rules {
        // NOTE: class_rule is sorted and de-duplicated
        // If the selector matches, the node classes must be identical
        let mut should_insert_rule = true;
        if class_rule.classes.len() != node_classes.len() {
            should_insert_rule = false;
        } else {
            for i in 0..class_rule.classes.len() {
                // we verified that the length of the two classes is the same
                if *node_classes[i] != class_rule.classes[i] {
                    should_insert_rule = false;
                    break;
                }
            }
        }

        if should_insert_rule {
            list.push_rule(class_rule);
        }
    }

    // first attribute for "id = something"
    let node_id = &node.id;

    if let Some(ref node_id) = *node_id {
        // if the node has an ID
        for id_rule in &parsed_css.pure_id_rules {
            if *id_rule.id.as_ref().unwrap() == *node_id {
                list.push_rule(id_rule);
            }
        }
    }

    // TODO: all the mixed rules
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
            default: ParsedCssProperty::TextAlign(StyleTextAlignmentHorz::Center),
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