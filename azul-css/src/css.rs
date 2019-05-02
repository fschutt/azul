//! Types and methods used to describe the style of an application
use crate::css_properties::{CssProperty, CssPropertyType};
use std::fmt;

/// Css stylesheet - contains a parsed CSS stylesheet in "rule blocks",
/// i.e. blocks of key-value pairs associated with a selector path.
#[derive(Debug, Default, PartialEq, Clone)]
pub struct Css {
    /// One CSS stylesheet can hold more than one sub-stylesheet:
    /// For example, when overriding native styles, the `.sort_by_specificy()` function
    /// should not mix the two stylesheets during sorting.
    pub stylesheets: Vec<Stylesheet>,
}

#[derive(Debug, Default, PartialEq, Clone)]
pub struct Stylesheet {
    /// The style rules making up the document - for example, de-duplicated CSS rules
    pub rules: Vec<CssRuleBlock>,
}

impl From<Vec<CssRuleBlock>> for Stylesheet {
    fn from(rules: Vec<CssRuleBlock>) -> Self {
        Self { rules }
    }
}

/// Contains one parsed `key: value` pair, static or dynamic
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CssDeclaration {
    /// Static key-value pair, such as `width: 500px`
    Static(CssProperty),
    /// Dynamic key-value pair with default value, such as `width: [[ my_id | 500px ]]`
    Dynamic(DynamicCssProperty),
}

impl CssDeclaration {

    /// Returns the type of the property (i.e. the CSS key as a typed enum)
    pub fn get_type(&self) -> CssPropertyType {
        use css::CssDeclaration::*;
        match self {
            Static(s) => s.get_type(),
            Dynamic(d) => d.property_type,
        }
    }

    /// Determines if the property will be inherited (applied to the children)
    /// during the recursive application of the style on the DOM tree
    pub fn is_inheritable(&self) -> bool {
        use self::CssDeclaration::*;
        match self {
            Static(s) => s.get_type().is_inheritable(),
            Dynamic(d) => d.is_inheritable(),
        }
    }

    /// Returns whether this rule affects only styling properties or layout
    /// properties (that could trigger a re-layout)
    pub fn can_trigger_relayout(&self) -> bool {
        use self::CssDeclaration::*;
        match self {
            Static(s) => s.get_type().can_trigger_relayout(),
            Dynamic(d) => d.can_trigger_relayout(),
        }
    }
}

/// A `DynamicCssProperty` is a type of css property that can be changed on possibly
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
/// At runtime the style is immutable (which is a performance optimization - if we
/// can assume that the property never changes at runtime), we can do some optimizations on it.
/// Dynamic style properties can also be used for animations and conditional styles
/// (i.e. `hover`, `focus`, etc.), thereby leading to cleaner code, since all of these
/// special cases now use one single API.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DynamicCssProperty {
    /// Key for this property
    pub property_type: CssPropertyType,
    /// The stringified ID of this property, i.e. the `"my_id"` in `width: [[ my_id | 500px ]]`.
    pub dynamic_id: String,
    /// Default value, used if the css property isn't overridden in this frame
    /// i.e. the `500px` in `width: [[ my_id | 500px ]]`. If this value is set to `auto`,
    /// the css property will not exist if it isn't overriden. An example where this is
    /// useful is when you want to say something like this:
    ///
    /// `width: [[ 400px | auto ]];`
    ///
    /// "If I set this property to width: 400px, then use exactly 400px. Otherwise use whatever the default width is."
    /// If this property wouldn't exist, you could only set the default to "0px" or something like
    /// that, meaning that if you don't override the property, then you'd set it to 0px - which is
    /// different from `auto`, since `auto` has its width determined by how much space there is
    /// available in the parent.
    pub default: CssPropertyValue<CssProperty>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CssPropertyValue<T> {
    Auto,
    None,
    Initial,
    Inherit,
    Exact(T),
}

impl<T> From<T> for CssPropertyValue<T> {
    fn from(c: T) -> Self { CssPropertyValue::Exact(c) }
}

impl<T> CssPropertyValue<T> {
    pub fn get_property(&self) -> Option<&T> {
        match self {
            CssPropertyValue::Exact(c) => Some(c),
            _ => None,
        }
    }
}

impl DynamicCssProperty {
    pub fn is_inheritable(&self) -> bool {
        // Dynamic style properties should not be inheritable,
        // since that could lead to bugs - you set a property in Rust, suddenly
        // the wrong UI component starts to react because it was inherited.
        false
    }

    pub fn can_trigger_relayout(&self) -> bool {
        self.property_type.can_trigger_relayout()
    }
}

/// One block of rules that applies a bunch of rules to a "path" in the style, i.e.
/// `div#myid.myclass -> { ("justify-content", "center") }`
#[derive(Debug, Clone, PartialEq)]
pub struct CssRuleBlock {
    /// The css path (full selector) of the style ruleset
    pub path: CssPath,
    /// `"justify-content: center"` =>
    /// `CssDeclaration::Static(CssProperty::JustifyContent(LayoutJustifyContent::Center))`
    pub declarations: Vec<CssDeclaration>,
}

pub type CssContentGroup<'a> = Vec<&'a CssPathSelector>;

/// Signifies the type (i.e. the discriminant value) of a DOM node
/// without carrying any of its associated data
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NodeTypePath {
    Div,
    P,
    Img,
    Texture,
    IFrame,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NodeTypePathParseError<'a> {
    Invalid(&'a str),
}

impl<'a> fmt::Display for NodeTypePathParseError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self {
            NodeTypePathParseError::Invalid(e) => write!(f, "Invalid node type: {}", e),
        }
    }
}

const NODE_TYPE_PATH_MAP: [(NodeTypePath, &'static str); 5] = [
    (NodeTypePath::Div, "div"),
    (NodeTypePath::P, "p"),
    (NodeTypePath::Img, "img"),
    (NodeTypePath::Texture, "texture"),
    (NodeTypePath::IFrame, "iframe"),
];

/// Parses the node type from a CSS string such as `"div"` => `NodeTypePath::Div`
impl NodeTypePath {
    pub fn from_str(css_key: &str) -> Result<Self, NodeTypePathParseError> {
        NODE_TYPE_PATH_MAP.iter()
        .find(|(_, k)| css_key == *k)
        .and_then(|(v, _)| Some(*v))
        .ok_or(NodeTypePathParseError::Invalid(css_key))
    }
}

impl fmt::Display for NodeTypePath {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        let display_string = NODE_TYPE_PATH_MAP.iter()
            .find(|(v, _)| *self == *v)
            .and_then(|(_, k)| Some(*k))
            .unwrap();

        write!(f, "{}", display_string)?;
        Ok(())
    }
}

/// Represents a full CSS path (i.e. the "div#id.class" selector belonging to
///  a CSS "content group" (the following key-value block)).
///
/// ```no_run,ignore
/// "#div > .my_class:focus" ==
/// [
///   CssPathSelector::Type(NodeTypePath::Div),
///   CssPathSelector::PseudoSelector(CssPathPseudoSelector::LimitChildren),
///   CssPathSelector::Class("my_class"),
///   CssPathSelector::PseudoSelector(CssPathPseudoSelector::Focus),
/// ]
#[derive(Clone, Hash, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct CssPath {
    pub selectors: Vec<CssPathSelector>,
}

impl fmt::Display for CssPath {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        for selector in &self.selectors {
            write!(f, "{}", selector)?;
        }
        Ok(())
    }
}

impl fmt::Debug for CssPath {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(f, "{}", self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
    Children,
}

impl Default for CssPathSelector {
    fn default() -> Self {
        CssPathSelector::Global
    }
}

impl fmt::Display for CssPathSelector {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::CssPathSelector::*;
        match &self {
            Global => write!(f, "*"),
            Type(n) => write!(f, "{}", n),
            Class(c) => write!(f, ".{}", c),
            Id(i) => write!(f, "#{}", i),
            PseudoSelector(p) => write!(f, ":{}", p),
            DirectChildren => write!(f, ">"),
            Children => write!(f, " "),
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CssPathPseudoSelector {
    /// `:first`
    First,
    /// `:last`
    Last,
    /// `:nth-child`
    NthChild(CssNthChildSelector),
    /// `:hover` - mouse is over element
    Hover,
    /// `:active` - mouse is pressed and over element
    Active,
    /// `:focus` - element has received focus
    Focus,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CssNthChildSelector {
    Number(usize),
    Even,
    Odd,
    Pattern { repeat: usize, offset: usize },
}

impl fmt::Display for CssNthChildSelector {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::CssNthChildSelector::*;
        match &self {
            Number(u) => write!(f, "{}", u),
            Even => write!(f, "even"),
            Odd => write!(f, "odd"),
            Pattern { repeat, offset } => write!(f, "{}n + {}", repeat, offset),
        }
    }
}

impl fmt::Display for CssPathPseudoSelector {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::CssPathPseudoSelector::*;
        match &self {
            First => write!(f, "first"),
            Last => write!(f, "last"),
            NthChild(u) => write!(f, "nth-child({})", u),
            Hover => write!(f, "hover"),
            Active => write!(f, "active"),
            Focus => write!(f, "focus"),
        }
    }
}

impl Css {

    /// Creates a new, empty CSS with no stylesheets
    pub fn new() -> Self {
        Default::default()
    }

    pub fn append(&mut self, css: Self) {
        for stylesheet in css.stylesheets {
            self.append_stylesheet(stylesheet);
        }
    }

    pub fn append_stylesheet(&mut self, styles: Stylesheet) {
        self.stylesheets.push(styles);
    }

    pub fn sort_by_specificity(&mut self) {
        for stylesheet in &mut self.stylesheets {
            stylesheet.sort_by_specificity()
        }
    }

    pub fn rules<'a>(&'a self) -> RuleIterator<'a> {
        RuleIterator {
            current_stylesheet: 0,
            current_rule: 0,
            css: self,
        }
    }
}

pub struct RuleIterator<'a> {
    current_stylesheet: usize,
    current_rule: usize,
    css: &'a Css,
}

impl<'a> Iterator for RuleIterator<'a> {
    type Item = &'a CssRuleBlock;
    fn next(&mut self) -> Option<&'a CssRuleBlock> {
        let current_stylesheet = self.css.stylesheets.get(self.current_stylesheet)?;
        match current_stylesheet.rules.get(self.current_rule) {
            Some(s) => {
                self.current_rule += 1;
                Some(s)
            },
            None => {
                self.current_rule = 0;
                self.current_stylesheet += 1;
                self.next()
            }
        }
    }
}


impl Stylesheet {

    /// Creates a new stylesheet with no style rules.
    pub fn new() -> Self {
        Default::default()
    }

    /// Sort the style rules by their weight, so that the rules are applied in the correct order.
    /// Should always be called when a new style is loaded from an external source.
    pub fn sort_by_specificity(&mut self) {
        self.rules.sort_by(|a, b| get_specificity(&a.path).cmp(&get_specificity(&b.path)));
    }
}

/// Returns specificity of the given css path. Further information can be found on
/// [the w3 website](http://www.w3.org/TR/selectors/#specificity).
fn get_specificity(path: &CssPath) -> (usize, usize, usize, usize) {
    let id_count = path.selectors.iter().filter(|x|     if let CssPathSelector::Id(_) = x {     true } else { false }).count();
    let class_count = path.selectors.iter().filter(|x|  if let CssPathSelector::Class(_) = x {  true } else { false }).count();
    let div_count = path.selectors.iter().filter(|x|    if let CssPathSelector::Type(_) = x {   true } else { false }).count();
    (id_count, class_count, div_count, path.selectors.len())
}

#[test]
fn test_specificity() {
    use self::CssPathSelector::*;
    assert_eq!(get_specificity(&CssPath { selectors: vec![Id("hello".into())] }), (1, 0, 0, 1));
    assert_eq!(get_specificity(&CssPath { selectors: vec![Class("hello".into())] }), (0, 1, 0, 1));
    assert_eq!(get_specificity(&CssPath { selectors: vec![Type(NodeTypePath::Div)] }), (0, 0, 1, 1));
    assert_eq!(get_specificity(&CssPath { selectors: vec![Id("hello".into()), Type(NodeTypePath::Div)] }), (1, 0, 1, 2));
}

// Assert that order of the style items is correct (in order of CSS path specificity, lowest-to-highest)
#[test]
fn test_specificity_sort() {
    use self::CssPathSelector::*;
    use crate::NodeTypePath::*;

    let mut input_style = Stylesheet {
        rules: vec![
            // Rules are sorted from lowest-specificity to highest specificity
            CssRuleBlock { path: CssPath { selectors: vec![Global] }, declarations: Vec::new() },
            CssRuleBlock { path: CssPath { selectors: vec![Global, Type(Div), Class("my_class".into()), Id("my_id".into())] }, declarations: Vec::new() },
            CssRuleBlock { path: CssPath { selectors: vec![Global, Type(Div), Id("my_id".into())] }, declarations: Vec::new() },
            CssRuleBlock { path: CssPath { selectors: vec![Global, Id("my_id".into())] }, declarations: Vec::new() },
            CssRuleBlock { path: CssPath { selectors: vec![Type(Div), Class("my_class".into()), Class("specific".into()), Id("my_id".into())] }, declarations: Vec::new() },
        ],
    };

    input_style.sort_by_specificity();

    let expected_style = Stylesheet {
        rules: vec![
            // Rules are sorted from lowest-specificity to highest specificity
            CssRuleBlock { path: CssPath { selectors: vec![Global] }, declarations: Vec::new() },
            CssRuleBlock { path: CssPath { selectors: vec![Global, Id("my_id".into())] }, declarations: Vec::new() },
            CssRuleBlock { path: CssPath { selectors: vec![Global, Type(Div), Id("my_id".into())] }, declarations: Vec::new() },
            CssRuleBlock { path: CssPath { selectors: vec![Global, Type(Div), Class("my_class".into()), Id("my_id".into())] }, declarations: Vec::new() },
            CssRuleBlock { path: CssPath { selectors: vec![Type(Div), Class("my_class".into()), Class("specific".into()), Id("my_id".into())] }, declarations: Vec::new() },
        ],
    };

    assert_eq!(input_style, expected_style);
}