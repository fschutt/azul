//! Types and methods used to describe the style of an application
use alloc::{string::String, vec::Vec};
use core::fmt;

use crate::{
    css_properties::{CssProperty, CssPropertyType},
    AzString,
};

/// Css stylesheet - contains a parsed CSS stylesheet in "rule blocks",
/// i.e. blocks of key-value pairs associated with a selector path.
#[derive(Debug, Default, PartialEq, PartialOrd, Clone)]
#[repr(C)]
pub struct Css {
    /// One CSS stylesheet can hold more than one sub-stylesheet:
    /// For example, when overriding native styles, the `.sort_by_specificy()` function
    /// should not mix the two stylesheets during sorting.
    pub stylesheets: StylesheetVec,
}

impl_vec!(Stylesheet, StylesheetVec, StylesheetVecDestructor);
impl_vec_mut!(Stylesheet, StylesheetVec);
impl_vec_debug!(Stylesheet, StylesheetVec);
impl_vec_partialord!(Stylesheet, StylesheetVec);
impl_vec_clone!(Stylesheet, StylesheetVec, StylesheetVecDestructor);
impl_vec_partialeq!(Stylesheet, StylesheetVec);

impl Css {
    pub fn is_empty(&self) -> bool {
        self.stylesheets.iter().all(|s| s.rules.as_ref().is_empty())
    }

    pub fn new(stylesheets: Vec<Stylesheet>) -> Self {
        Self {
            stylesheets: stylesheets.into(),
        }
    }

    #[cfg(feature = "parser")]
    pub fn from_string(s: crate::AzString) -> Self {
        crate::parser::new_from_str(s.as_str()).0
    }
}

#[derive(Debug, Default, PartialEq, PartialOrd, Clone)]
#[repr(C)]
pub struct Stylesheet {
    /// The style rules making up the document - for example, de-duplicated CSS rules
    pub rules: CssRuleBlockVec,
}

impl_vec!(CssRuleBlock, CssRuleBlockVec, CssRuleBlockVecDestructor);
impl_vec_mut!(CssRuleBlock, CssRuleBlockVec);
impl_vec_debug!(CssRuleBlock, CssRuleBlockVec);
impl_vec_partialord!(CssRuleBlock, CssRuleBlockVec);
impl_vec_clone!(CssRuleBlock, CssRuleBlockVec, CssRuleBlockVecDestructor);
impl_vec_partialeq!(CssRuleBlock, CssRuleBlockVec);

impl Stylesheet {
    pub fn new(rules: Vec<CssRuleBlock>) -> Self {
        Self {
            rules: rules.into(),
        }
    }
}

impl From<Vec<CssRuleBlock>> for Stylesheet {
    fn from(rules: Vec<CssRuleBlock>) -> Self {
        Self {
            rules: rules.into(),
        }
    }
}

/// Contains one parsed `key: value` pair, static or dynamic
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C, u8)]
pub enum CssDeclaration {
    /// Static key-value pair, such as `width: 500px`
    Static(CssProperty),
    /// Dynamic key-value pair with default value, such as `width: [[ my_id | 500px ]]`
    Dynamic(DynamicCssProperty),
}

impl CssDeclaration {
    pub const fn new_static(prop: CssProperty) -> Self {
        CssDeclaration::Static(prop)
    }

    pub const fn new_dynamic(prop: DynamicCssProperty) -> Self {
        CssDeclaration::Dynamic(prop)
    }

    /// Returns the type of the property (i.e. the CSS key as a typed enum)
    pub fn get_type(&self) -> CssPropertyType {
        use self::CssDeclaration::*;
        match self {
            Static(s) => s.get_type(),
            Dynamic(d) => d.default_value.get_type(),
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

    pub fn to_str(&self) -> String {
        use self::CssDeclaration::*;
        match self {
            Static(s) => format!("{:?}", s),
            Dynamic(d) => format!("var(--{}, {:?})", d.dynamic_id, d.default_value),
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
///    padding: var(--my_dynamic_property_id, 400px);
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
#[repr(C)]
pub struct DynamicCssProperty {
    /// The stringified ID of this property, i.e. the `"my_id"` in `width: var(--my_id, 500px)`.
    pub dynamic_id: AzString,
    /// Default values for this properties - one single value can control multiple properties!
    pub default_value: CssProperty,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C, u8)] // necessary for ABI stability
pub enum CssPropertyValue<T> {
    Auto,
    None,
    Initial,
    Inherit,
    Exact(T),
}

pub trait PrintAsCssValue {
    fn print_as_css_value(&self) -> String;
}

impl<T: PrintAsCssValue> CssPropertyValue<T> {
    pub fn get_css_value_fmt(&self) -> String {
        match self {
            CssPropertyValue::Auto => format!("auto"),
            CssPropertyValue::None => format!("none"),
            CssPropertyValue::Initial => format!("initial"),
            CssPropertyValue::Inherit => format!("inherit"),
            CssPropertyValue::Exact(e) => e.print_as_css_value(),
        }
    }
}

impl<T: fmt::Display> fmt::Display for CssPropertyValue<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::CssPropertyValue::*;
        match self {
            Auto => write!(f, "auto"),
            None => write!(f, "none"),
            Initial => write!(f, "initial"),
            Inherit => write!(f, "inherit"),
            Exact(e) => write!(f, "{}", e),
        }
    }
}

impl<T> From<T> for CssPropertyValue<T> {
    fn from(c: T) -> Self {
        CssPropertyValue::Exact(c)
    }
}

impl<T> CssPropertyValue<T> {
    /// Transforms a `CssPropertyValue<T>` into a `CssPropertyValue<U>` by applying a mapping
    /// function
    #[inline]
    pub fn map_property<F: Fn(T) -> U, U>(self, map_fn: F) -> CssPropertyValue<U> {
        match self {
            CssPropertyValue::Exact(c) => CssPropertyValue::Exact(map_fn(c)),
            CssPropertyValue::Auto => CssPropertyValue::Auto,
            CssPropertyValue::None => CssPropertyValue::None,
            CssPropertyValue::Initial => CssPropertyValue::Initial,
            CssPropertyValue::Inherit => CssPropertyValue::Inherit,
        }
    }

    #[inline]
    pub fn get_property(&self) -> Option<&T> {
        match self {
            CssPropertyValue::Exact(c) => Some(c),
            _ => None,
        }
    }

    #[inline]
    pub fn get_property_owned(self) -> Option<T> {
        match self {
            CssPropertyValue::Exact(c) => Some(c),
            _ => None,
        }
    }

    #[inline]
    pub fn is_auto(&self) -> bool {
        match self {
            CssPropertyValue::Auto => true,
            _ => false,
        }
    }

    #[inline]
    pub fn is_none(&self) -> bool {
        match self {
            CssPropertyValue::None => true,
            _ => false,
        }
    }

    #[inline]
    pub fn is_initial(&self) -> bool {
        match self {
            CssPropertyValue::Initial => true,
            _ => false,
        }
    }

    #[inline]
    pub fn is_inherit(&self) -> bool {
        match self {
            CssPropertyValue::Inherit => true,
            _ => false,
        }
    }
}

impl<T: Default> CssPropertyValue<T> {
    #[inline]
    pub fn get_property_or_default(self) -> Option<T> {
        match self {
            CssPropertyValue::Auto | CssPropertyValue::Initial => Some(T::default()),
            CssPropertyValue::Exact(c) => Some(c),
            CssPropertyValue::None | CssPropertyValue::Inherit => None,
        }
    }
}

impl<T: Default> Default for CssPropertyValue<T> {
    #[inline]
    fn default() -> Self {
        CssPropertyValue::Exact(T::default())
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
        self.default_value.get_type().can_trigger_relayout()
    }
}

/// One block of rules that applies a bunch of rules to a "path" in the style, i.e.
/// `div#myid.myclass -> { ("justify-content", "center") }`
#[derive(Debug, Clone, PartialOrd, PartialEq)]
#[repr(C)]
pub struct CssRuleBlock {
    /// The css path (full selector) of the style ruleset
    pub path: CssPath,
    /// `"justify-content: center"` =>
    /// `CssDeclaration::Static(CssProperty::JustifyContent(LayoutJustifyContent::Center))`
    pub declarations: CssDeclarationVec,
}

impl_vec!(
    CssDeclaration,
    CssDeclarationVec,
    CssDeclarationVecDestructor
);
impl_vec_mut!(CssDeclaration, CssDeclarationVec);
impl_vec_debug!(CssDeclaration, CssDeclarationVec);
impl_vec_partialord!(CssDeclaration, CssDeclarationVec);
impl_vec_ord!(CssDeclaration, CssDeclarationVec);
impl_vec_clone!(
    CssDeclaration,
    CssDeclarationVec,
    CssDeclarationVecDestructor
);
impl_vec_partialeq!(CssDeclaration, CssDeclarationVec);
impl_vec_eq!(CssDeclaration, CssDeclarationVec);
impl_vec_hash!(CssDeclaration, CssDeclarationVec);

impl CssRuleBlock {
    pub fn new(path: CssPath, declarations: Vec<CssDeclaration>) -> Self {
        Self {
            path,
            declarations: declarations.into(),
        }
    }
}

pub type CssContentGroup<'a> = Vec<&'a CssPathSelector>;

/// Signifies the type (i.e. the discriminant value) of a DOM node
/// without carrying any of its associated data
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum NodeTypeTag {
    Body,
    Div,
    Br,
    P,
    Img,
    IFrame,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NodeTypeTagParseError<'a> {
    Invalid(&'a str),
}

impl<'a> fmt::Display for NodeTypeTagParseError<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self {
            NodeTypeTagParseError::Invalid(e) => write!(f, "Invalid node type: {}", e),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum NodeTypeTagParseErrorOwned {
    Invalid(String),
}

impl<'a> NodeTypeTagParseError<'a> {
    pub fn to_contained(&self) -> NodeTypeTagParseErrorOwned {
        match self {
            NodeTypeTagParseError::Invalid(s) => NodeTypeTagParseErrorOwned::Invalid(s.to_string()),
        }
    }
}

impl NodeTypeTagParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> NodeTypeTagParseError<'a> {
        match self {
            NodeTypeTagParseErrorOwned::Invalid(s) => NodeTypeTagParseError::Invalid(s),
        }
    }
}

/// Parses the node type from a CSS string such as `"div"` => `NodeTypeTag::Div`
impl NodeTypeTag {
    pub fn from_str(css_key: &str) -> Result<Self, NodeTypeTagParseError> {
        match css_key {
            "body" => Ok(NodeTypeTag::Body),
            "div" => Ok(NodeTypeTag::Div),
            "p" => Ok(NodeTypeTag::P),
            "img" => Ok(NodeTypeTag::Img),
            other => Err(NodeTypeTagParseError::Invalid(other)),
        }
    }
}

impl fmt::Display for NodeTypeTag {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            NodeTypeTag::Body => write!(f, "body"),
            NodeTypeTag::Div => write!(f, "div"),
            NodeTypeTag::Br => write!(f, "br"),
            NodeTypeTag::P => write!(f, "p"),
            NodeTypeTag::Img => write!(f, "img"),
            NodeTypeTag::IFrame => write!(f, "iframe"),
        }
    }
}

/// Represents a full CSS path (i.e. the "div#id.class" selector belonging to
///  a CSS "content group" (the following key-value block)).
///
/// ```no_run,ignore
/// "#div > .my_class:focus" ==
/// [
///   CssPathSelector::Type(NodeTypeTag::Div),
///   CssPathSelector::PseudoSelector(CssPathPseudoSelector::LimitChildren),
///   CssPathSelector::Class("my_class"),
///   CssPathSelector::PseudoSelector(CssPathPseudoSelector::Focus),
/// ]
#[derive(Clone, Hash, Default, PartialEq, Eq, PartialOrd, Ord)]
#[repr(C)]
pub struct CssPath {
    pub selectors: CssPathSelectorVec,
}

impl_vec!(
    CssPathSelector,
    CssPathSelectorVec,
    CssPathSelectorVecDestructor
);
impl_vec_debug!(CssPathSelector, CssPathSelectorVec);
impl_vec_partialord!(CssPathSelector, CssPathSelectorVec);
impl_vec_ord!(CssPathSelector, CssPathSelectorVec);
impl_vec_clone!(
    CssPathSelector,
    CssPathSelectorVec,
    CssPathSelectorVecDestructor
);
impl_vec_partialeq!(CssPathSelector, CssPathSelectorVec);
impl_vec_eq!(CssPathSelector, CssPathSelectorVec);
impl_vec_hash!(CssPathSelector, CssPathSelectorVec);

impl CssPath {
    pub fn new(selectors: Vec<CssPathSelector>) -> Self {
        Self {
            selectors: selectors.into(),
        }
    }
}

impl fmt::Display for CssPath {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for selector in self.selectors.as_ref() {
            write!(f, "{}", selector)?;
        }
        Ok(())
    }
}

impl fmt::Debug for CssPath {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C, u8)]
pub enum CssPathSelector {
    /// Represents the `*` selector
    Global,
    /// `div`, `p`, etc.
    Type(NodeTypeTag),
    /// `.something`
    Class(AzString),
    /// `#something`
    Id(AzString),
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
#[repr(C, u8)]
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
#[repr(C, u8)]
pub enum CssNthChildSelector {
    Number(u32),
    Even,
    Odd,
    Pattern(CssNthChildPattern),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct CssNthChildPattern {
    pub repeat: u32,
    pub offset: u32,
}

impl fmt::Display for CssNthChildSelector {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::CssNthChildSelector::*;
        match &self {
            Number(u) => write!(f, "{}", u),
            Even => write!(f, "even"),
            Odd => write!(f, "odd"),
            Pattern(p) => write!(f, "{}n + {}", p.repeat, p.offset),
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
    pub fn empty() -> Self {
        Default::default()
    }

    pub fn sort_by_specificity(&mut self) {
        self.stylesheets
            .as_mut()
            .iter_mut()
            .for_each(|s| s.sort_by_specificity());
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
            }
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
    pub fn empty() -> Self {
        Default::default()
    }

    /// Sort the style rules by their weight, so that the rules are applied in the correct order.
    /// Should always be called when a new style is loaded from an external source.
    pub fn sort_by_specificity(&mut self) {
        self.rules
            .as_mut()
            .sort_by(|a, b| get_specificity(&a.path).cmp(&get_specificity(&b.path)));
    }
}

/// Returns specificity of the given css path. Further information can be found on
/// [the w3 website](http://www.w3.org/TR/selectors/#specificity).
fn get_specificity(path: &CssPath) -> (usize, usize, usize, usize) {
    let id_count = path
        .selectors
        .iter()
        .filter(|x| {
            if let CssPathSelector::Id(_) = x {
                true
            } else {
                false
            }
        })
        .count();
    let class_count = path
        .selectors
        .iter()
        .filter(|x| {
            if let CssPathSelector::Class(_) = x {
                true
            } else {
                false
            }
        })
        .count();
    let div_count = path
        .selectors
        .iter()
        .filter(|x| {
            if let CssPathSelector::Type(_) = x {
                true
            } else {
                false
            }
        })
        .count();
    (id_count, class_count, div_count, path.selectors.len())
}
