//! Types and methods used to describe the style of an application
use alloc::{string::String, vec::Vec};
use core::fmt;

use crate::{
    dynamic_selector::DynamicSelectorVec,
    props::property::{format_static_css_prop, CssProperty, CssPropertyType},
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

impl_vec!(Stylesheet, StylesheetVec, StylesheetVecDestructor, StylesheetVecDestructorType, StylesheetVecSlice, OptionStylesheet);
impl_vec_mut!(Stylesheet, StylesheetVec);
impl_vec_debug!(Stylesheet, StylesheetVec);
impl_vec_partialord!(Stylesheet, StylesheetVec);
impl_vec_clone!(Stylesheet, StylesheetVec, StylesheetVecDestructor);
impl_vec_partialeq!(Stylesheet, StylesheetVec);

impl_option!(
    Css,
    OptionCss,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd]
);

impl_vec!(Css, CssVec, CssVecDestructor, CssVecDestructorType, CssVecSlice, OptionCss);
impl_vec_mut!(Css, CssVec);
impl_vec_debug!(Css, CssVec);
impl_vec_partialord!(Css, CssVec);
impl_vec_clone!(Css, CssVec, CssVecDestructor);
impl_vec_partialeq!(Css, CssVec);

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
        crate::parser2::new_from_str(s.as_str()).0
    }

    #[cfg(feature = "parser")]
    pub fn from_string_with_warnings(
        s: crate::AzString,
    ) -> (Self, Vec<crate::parser2::CssParseWarnMsgOwned>) {
        let (css, warnings) = crate::parser2::new_from_str(s.as_str());
        (
            css,
            warnings
                .into_iter()
                .map(|w| crate::parser2::CssParseWarnMsgOwned {
                    warning: w.warning.to_contained(),
                    location: w.location,
                })
                .collect(),
        )
    }
}

#[derive(Debug, Default, PartialEq, PartialOrd, Clone)]
#[repr(C)]
pub struct Stylesheet {
    /// The style rules making up the document - for example, de-duplicated CSS rules
    pub rules: CssRuleBlockVec,
}

impl_option!(
    Stylesheet,
    OptionStylesheet,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd]
);

impl_vec!(CssRuleBlock, CssRuleBlockVec, CssRuleBlockVecDestructor, CssRuleBlockVecDestructorType, CssRuleBlockVecSlice, OptionCssRuleBlock);
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

impl_option!(
    CssDeclaration,
    OptionCssDeclaration,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

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

/// A value that is either heap-allocated (parsed at runtime) or a compile-time
/// static reference. Used to reduce enum size for large CSS property payloads
/// by storing them behind a pointer instead of inline.
///
/// - Size: 1 (tag) + 7 (padding) + 8 (pointer) = **16 bytes** on 64-bit
/// - `Static` variant: no allocation, just a `*const T` pointer to static data
/// - `Boxed` variant: heap-allocated via `Box::into_raw`, freed on Drop
#[repr(C, u8)]
pub enum BoxOrStatic<T> {
    /// Heap-allocated (parsed at runtime). Owned — freed on Drop.
    Boxed(*mut T),
    /// Compile-time constant (e.g. from `const` CSS defaults). Not freed.
    Static(*const T),
}

impl<T> BoxOrStatic<T> {
    /// Allocate `value` on the heap and return a `Boxed` variant.
    #[inline]
    pub fn heap(value: T) -> Self {
        BoxOrStatic::Boxed(Box::into_raw(Box::new(value)))
    }

    /// Return a reference to the inner value.
    #[inline]
    pub fn as_ref(&self) -> &T {
        match self {
            BoxOrStatic::Boxed(ptr) => unsafe { &**ptr },
            BoxOrStatic::Static(ptr) => unsafe { &**ptr },
        }
    }

    /// Return a mutable reference to the inner value (only for Boxed).
    /// Panics if called on Static.
    #[inline]
    pub fn as_mut(&mut self) -> &mut T {
        match self {
            BoxOrStatic::Boxed(ptr) => unsafe { &mut **ptr },
            BoxOrStatic::Static(_) => panic!("Cannot mutate a static BoxOrStatic value"),
        }
    }

    /// Consume self and return the inner value.
    #[inline]
    pub fn into_inner(self) -> T where T: Clone {
        let val = self.as_ref().clone();
        // Don't double-free: std::mem::forget prevents Drop from running
        core::mem::forget(self);
        val
    }
}

impl<T> Drop for BoxOrStatic<T> {
    fn drop(&mut self) {
        if let BoxOrStatic::Boxed(ptr) = self {
            if !ptr.is_null() {
                unsafe { drop(Box::from_raw(*ptr)); }
                *ptr = core::ptr::null_mut();
            }
        }
    }
}

impl<T: Clone> Clone for BoxOrStatic<T> {
    fn clone(&self) -> Self {
        match self {
            BoxOrStatic::Boxed(ptr) => {
                let val = unsafe { &**ptr }.clone();
                BoxOrStatic::Boxed(Box::into_raw(Box::new(val)))
            }
            BoxOrStatic::Static(ptr) => BoxOrStatic::Static(*ptr),
        }
    }
}

impl<T: core::fmt::Debug> core::fmt::Debug for BoxOrStatic<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl<T: core::fmt::Display> core::fmt::Display for BoxOrStatic<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl<T: PartialEq> PartialEq for BoxOrStatic<T> {
    fn eq(&self, other: &Self) -> bool {
        self.as_ref() == other.as_ref()
    }
}

impl<T: Eq> Eq for BoxOrStatic<T> {}

impl<T: core::hash::Hash> core::hash::Hash for BoxOrStatic<T> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.as_ref().hash(state)
    }
}

impl<T: PartialOrd> PartialOrd for BoxOrStatic<T> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.as_ref().partial_cmp(other.as_ref())
    }
}

impl<T: Ord> Ord for BoxOrStatic<T> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.as_ref().cmp(other.as_ref())
    }
}

impl<T> core::ops::Deref for BoxOrStatic<T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &T {
        self.as_ref()
    }
}

impl<T: Default> Default for BoxOrStatic<T> {
    fn default() -> Self {
        BoxOrStatic::heap(T::default())
    }
}

impl<T: PrintAsCssValue> PrintAsCssValue for BoxOrStatic<T> {
    fn print_as_css_value(&self) -> String {
        self.as_ref().print_as_css_value()
    }
}

// Safety: BoxOrStatic<T> is Send if T is Send
unsafe impl<T: Send + 'static> Send for BoxOrStatic<T> {}
// Safety: BoxOrStatic<T> is Sync if T is Sync
unsafe impl<T: Sync + 'static> Sync for BoxOrStatic<T> {}

/// Type alias: `BoxOrStatic<StyleBoxShadow>` — used by codegen for FFI monomorphization.
pub type BoxOrStaticStyleBoxShadow = BoxOrStatic<crate::props::style::box_shadow::StyleBoxShadow>;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C, u8)] // necessary for ABI stability
pub enum CssPropertyValue<T> {
    Auto,
    None,
    Initial,
    Inherit,
    Revert,
    Unset,
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
            CssPropertyValue::Revert => format!("revert"),
            CssPropertyValue::Unset => format!("unset"),
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
            Revert => write!(f, "revert"),
            Unset => write!(f, "unset"),
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
            CssPropertyValue::Revert => CssPropertyValue::Revert,
            CssPropertyValue::Unset => CssPropertyValue::Unset,
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

    #[inline]
    pub fn is_revert(&self) -> bool {
        match self {
            CssPropertyValue::Revert => true,
            _ => false,
        }
    }

    #[inline]
    pub fn is_unset(&self) -> bool {
        match self {
            CssPropertyValue::Unset => true,
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
            CssPropertyValue::None
            | CssPropertyValue::Inherit
            | CssPropertyValue::Revert
            | CssPropertyValue::Unset => None,
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
///
/// The `conditions` field contains @media/@lang/etc. conditions that must ALL be
/// satisfied for this rule block to apply (from enclosing @-rule blocks).
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct CssRuleBlock {
    /// The css path (full selector) of the style ruleset
    pub path: CssPath,
    /// `"justify-content: center"` =>
    /// `CssDeclaration::Static(CssProperty::JustifyContent(LayoutJustifyContent::Center))`
    pub declarations: CssDeclarationVec,
    /// Conditions from enclosing @-rules (@media, @lang, etc.) that must ALL be
    /// satisfied for this rule block to apply. Empty = unconditional.
    pub conditions: DynamicSelectorVec,
}

impl_option!(
    CssRuleBlock,
    OptionCssRuleBlock,
    copy = false,
    [Debug, Clone, PartialEq, PartialOrd]
);

impl PartialOrd for CssRuleBlock {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        // Compare by path and declarations only, conditions are not ordered
        match self.path.partial_cmp(&other.path) {
            Some(core::cmp::Ordering::Equal) => self.declarations.partial_cmp(&other.declarations),
            ord => ord,
        }
    }
}

impl_vec!(CssDeclaration, CssDeclarationVec, CssDeclarationVecDestructor, CssDeclarationVecDestructorType, CssDeclarationVecSlice, OptionCssDeclaration);
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
            conditions: DynamicSelectorVec::from_const_slice(&[]),
        }
    }

    pub fn with_conditions(
        path: CssPath,
        declarations: Vec<CssDeclaration>,
        conditions: Vec<crate::dynamic_selector::DynamicSelector>,
    ) -> Self {
        Self {
            path,
            declarations: declarations.into(),
            conditions: conditions.into(),
        }
    }
}

pub type CssContentGroup<'a> = Vec<&'a CssPathSelector>;

/// Signifies the type of a DOM node without carrying any associated data
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum NodeTypeTag {
    // Document structure
    Html,
    Head,
    Body,

    // Block-level elements
    Div,
    P,
    Article,
    Section,
    Nav,
    Aside,
    Header,
    Footer,
    Main,
    Figure,
    FigCaption,

    // Headings
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,

    // Inline text
    Br,
    Hr,
    Pre,
    BlockQuote,
    Address,
    Details,
    Summary,
    Dialog,

    // Lists
    Ul,
    Ol,
    Li,
    Dl,
    Dt,
    Dd,
    Menu,
    MenuItem,
    Dir,

    // Tables
    Table,
    Caption,
    THead,
    TBody,
    TFoot,
    Tr,
    Th,
    Td,
    ColGroup,
    Col,

    // Forms
    Form,
    FieldSet,
    Legend,
    Label,
    Input,
    Button,
    Select,
    OptGroup,
    SelectOption,
    TextArea,
    Output,
    Progress,
    Meter,
    DataList,

    // Inline elements
    Span,
    A,
    Em,
    Strong,
    B,
    I,
    U,
    S,
    Mark,
    Del,
    Ins,
    Code,
    Samp,
    Kbd,
    Var,
    Cite,
    Dfn,
    Abbr,
    Acronym,
    Q,
    Time,
    Sub,
    Sup,
    Small,
    Big,
    Bdo,
    Bdi,
    Wbr,
    Ruby,
    Rt,
    Rtc,
    Rp,
    Data,

    // Embedded content
    Canvas,
    Object,
    Param,
    Embed,
    Audio,
    Video,
    Source,
    Track,
    Map,
    Area,
    Svg,

    // Metadata
    Title,
    Meta,
    Link,
    Script,
    Style,
    Base,

    // Special
    Text,
    Img,
    VirtualizedView,
    /// Icon element - resolved to actual content by IconProvider
    Icon,

    // Pseudo-elements
    Before,
    After,
    Marker,
    Placeholder,
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
#[repr(C, u8)]
pub enum NodeTypeTagParseErrorOwned {
    Invalid(AzString),
}

impl<'a> NodeTypeTagParseError<'a> {
    pub fn to_contained(&self) -> NodeTypeTagParseErrorOwned {
        match self {
            NodeTypeTagParseError::Invalid(s) => NodeTypeTagParseErrorOwned::Invalid(s.to_string().into()),
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
            // Document structure
            "html" => Ok(NodeTypeTag::Html),
            "head" => Ok(NodeTypeTag::Head),
            "body" => Ok(NodeTypeTag::Body),

            // Block-level elements
            "div" => Ok(NodeTypeTag::Div),
            "p" => Ok(NodeTypeTag::P),
            "article" => Ok(NodeTypeTag::Article),
            "section" => Ok(NodeTypeTag::Section),
            "nav" => Ok(NodeTypeTag::Nav),
            "aside" => Ok(NodeTypeTag::Aside),
            "header" => Ok(NodeTypeTag::Header),
            "footer" => Ok(NodeTypeTag::Footer),
            "main" => Ok(NodeTypeTag::Main),
            "figure" => Ok(NodeTypeTag::Figure),
            "figcaption" => Ok(NodeTypeTag::FigCaption),

            // Headings
            "h1" => Ok(NodeTypeTag::H1),
            "h2" => Ok(NodeTypeTag::H2),
            "h3" => Ok(NodeTypeTag::H3),
            "h4" => Ok(NodeTypeTag::H4),
            "h5" => Ok(NodeTypeTag::H5),
            "h6" => Ok(NodeTypeTag::H6),

            // Inline text
            "br" => Ok(NodeTypeTag::Br),
            "hr" => Ok(NodeTypeTag::Hr),
            "pre" => Ok(NodeTypeTag::Pre),
            "blockquote" => Ok(NodeTypeTag::BlockQuote),
            "address" => Ok(NodeTypeTag::Address),
            "details" => Ok(NodeTypeTag::Details),
            "summary" => Ok(NodeTypeTag::Summary),
            "dialog" => Ok(NodeTypeTag::Dialog),

            // Lists
            "ul" => Ok(NodeTypeTag::Ul),
            "ol" => Ok(NodeTypeTag::Ol),
            "li" => Ok(NodeTypeTag::Li),
            "dl" => Ok(NodeTypeTag::Dl),
            "dt" => Ok(NodeTypeTag::Dt),
            "dd" => Ok(NodeTypeTag::Dd),
            "menu" => Ok(NodeTypeTag::Menu),
            "menuitem" => Ok(NodeTypeTag::MenuItem),
            "dir" => Ok(NodeTypeTag::Dir),

            // Tables
            "table" => Ok(NodeTypeTag::Table),
            "caption" => Ok(NodeTypeTag::Caption),
            "thead" => Ok(NodeTypeTag::THead),
            "tbody" => Ok(NodeTypeTag::TBody),
            "tfoot" => Ok(NodeTypeTag::TFoot),
            "tr" => Ok(NodeTypeTag::Tr),
            "th" => Ok(NodeTypeTag::Th),
            "td" => Ok(NodeTypeTag::Td),
            "colgroup" => Ok(NodeTypeTag::ColGroup),
            "col" => Ok(NodeTypeTag::Col),

            // Forms
            "form" => Ok(NodeTypeTag::Form),
            "fieldset" => Ok(NodeTypeTag::FieldSet),
            "legend" => Ok(NodeTypeTag::Legend),
            "label" => Ok(NodeTypeTag::Label),
            "input" => Ok(NodeTypeTag::Input),
            "button" => Ok(NodeTypeTag::Button),
            "select" => Ok(NodeTypeTag::Select),
            "optgroup" => Ok(NodeTypeTag::OptGroup),
            "option" => Ok(NodeTypeTag::SelectOption),
            "textarea" => Ok(NodeTypeTag::TextArea),
            "output" => Ok(NodeTypeTag::Output),
            "progress" => Ok(NodeTypeTag::Progress),
            "meter" => Ok(NodeTypeTag::Meter),
            "datalist" => Ok(NodeTypeTag::DataList),

            // Inline elements
            "span" => Ok(NodeTypeTag::Span),
            "a" => Ok(NodeTypeTag::A),
            "em" => Ok(NodeTypeTag::Em),
            "strong" => Ok(NodeTypeTag::Strong),
            "b" => Ok(NodeTypeTag::B),
            "i" => Ok(NodeTypeTag::I),
            "u" => Ok(NodeTypeTag::U),
            "s" => Ok(NodeTypeTag::S),
            "mark" => Ok(NodeTypeTag::Mark),
            "del" => Ok(NodeTypeTag::Del),
            "ins" => Ok(NodeTypeTag::Ins),
            "code" => Ok(NodeTypeTag::Code),
            "samp" => Ok(NodeTypeTag::Samp),
            "kbd" => Ok(NodeTypeTag::Kbd),
            "var" => Ok(NodeTypeTag::Var),
            "cite" => Ok(NodeTypeTag::Cite),
            "dfn" => Ok(NodeTypeTag::Dfn),
            "abbr" => Ok(NodeTypeTag::Abbr),
            "acronym" => Ok(NodeTypeTag::Acronym),
            "q" => Ok(NodeTypeTag::Q),
            "time" => Ok(NodeTypeTag::Time),
            "sub" => Ok(NodeTypeTag::Sub),
            "sup" => Ok(NodeTypeTag::Sup),
            "small" => Ok(NodeTypeTag::Small),
            "big" => Ok(NodeTypeTag::Big),
            "bdo" => Ok(NodeTypeTag::Bdo),
            "bdi" => Ok(NodeTypeTag::Bdi),
            "wbr" => Ok(NodeTypeTag::Wbr),
            "ruby" => Ok(NodeTypeTag::Ruby),
            "rt" => Ok(NodeTypeTag::Rt),
            "rtc" => Ok(NodeTypeTag::Rtc),
            "rp" => Ok(NodeTypeTag::Rp),
            "data" => Ok(NodeTypeTag::Data),

            // Embedded content
            "canvas" => Ok(NodeTypeTag::Canvas),
            "object" => Ok(NodeTypeTag::Object),
            "param" => Ok(NodeTypeTag::Param),
            "embed" => Ok(NodeTypeTag::Embed),
            "audio" => Ok(NodeTypeTag::Audio),
            "video" => Ok(NodeTypeTag::Video),
            "source" => Ok(NodeTypeTag::Source),
            "track" => Ok(NodeTypeTag::Track),
            "map" => Ok(NodeTypeTag::Map),
            "area" => Ok(NodeTypeTag::Area),
            "svg" => Ok(NodeTypeTag::Svg),

            // Metadata
            "title" => Ok(NodeTypeTag::Title),
            "meta" => Ok(NodeTypeTag::Meta),
            "link" => Ok(NodeTypeTag::Link),
            "script" => Ok(NodeTypeTag::Script),
            "style" => Ok(NodeTypeTag::Style),
            "base" => Ok(NodeTypeTag::Base),

            // Special
            "img" => Ok(NodeTypeTag::Img),
            "virtualized-view" | "iframe" => Ok(NodeTypeTag::VirtualizedView),
            "icon" => Ok(NodeTypeTag::Icon),

            // Pseudo-elements (usually prefixed with ::)
            "before" | "::before" => Ok(NodeTypeTag::Before),
            "after" | "::after" => Ok(NodeTypeTag::After),
            "marker" | "::marker" => Ok(NodeTypeTag::Marker),
            "placeholder" | "::placeholder" => Ok(NodeTypeTag::Placeholder),

            other => Err(NodeTypeTagParseError::Invalid(other)),
        }
    }
}

impl fmt::Display for NodeTypeTag {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            // Document structure
            NodeTypeTag::Html => write!(f, "html"),
            NodeTypeTag::Head => write!(f, "head"),
            NodeTypeTag::Body => write!(f, "body"),

            // Block elements
            NodeTypeTag::Div => write!(f, "div"),
            NodeTypeTag::P => write!(f, "p"),
            NodeTypeTag::Article => write!(f, "article"),
            NodeTypeTag::Section => write!(f, "section"),
            NodeTypeTag::Nav => write!(f, "nav"),
            NodeTypeTag::Aside => write!(f, "aside"),
            NodeTypeTag::Header => write!(f, "header"),
            NodeTypeTag::Footer => write!(f, "footer"),
            NodeTypeTag::Main => write!(f, "main"),
            NodeTypeTag::Figure => write!(f, "figure"),
            NodeTypeTag::FigCaption => write!(f, "figcaption"),

            // Headings
            NodeTypeTag::H1 => write!(f, "h1"),
            NodeTypeTag::H2 => write!(f, "h2"),
            NodeTypeTag::H3 => write!(f, "h3"),
            NodeTypeTag::H4 => write!(f, "h4"),
            NodeTypeTag::H5 => write!(f, "h5"),
            NodeTypeTag::H6 => write!(f, "h6"),

            // Text formatting
            NodeTypeTag::Br => write!(f, "br"),
            NodeTypeTag::Hr => write!(f, "hr"),
            NodeTypeTag::Pre => write!(f, "pre"),
            NodeTypeTag::BlockQuote => write!(f, "blockquote"),
            NodeTypeTag::Address => write!(f, "address"),
            NodeTypeTag::Details => write!(f, "details"),
            NodeTypeTag::Summary => write!(f, "summary"),
            NodeTypeTag::Dialog => write!(f, "dialog"),

            // List elements
            NodeTypeTag::Ul => write!(f, "ul"),
            NodeTypeTag::Ol => write!(f, "ol"),
            NodeTypeTag::Li => write!(f, "li"),
            NodeTypeTag::Dl => write!(f, "dl"),
            NodeTypeTag::Dt => write!(f, "dt"),
            NodeTypeTag::Dd => write!(f, "dd"),
            NodeTypeTag::Menu => write!(f, "menu"),
            NodeTypeTag::MenuItem => write!(f, "menuitem"),
            NodeTypeTag::Dir => write!(f, "dir"),

            // Table elements
            NodeTypeTag::Table => write!(f, "table"),
            NodeTypeTag::Caption => write!(f, "caption"),
            NodeTypeTag::THead => write!(f, "thead"),
            NodeTypeTag::TBody => write!(f, "tbody"),
            NodeTypeTag::TFoot => write!(f, "tfoot"),
            NodeTypeTag::Tr => write!(f, "tr"),
            NodeTypeTag::Th => write!(f, "th"),
            NodeTypeTag::Td => write!(f, "td"),
            NodeTypeTag::ColGroup => write!(f, "colgroup"),
            NodeTypeTag::Col => write!(f, "col"),

            // Form elements
            NodeTypeTag::Form => write!(f, "form"),
            NodeTypeTag::FieldSet => write!(f, "fieldset"),
            NodeTypeTag::Legend => write!(f, "legend"),
            NodeTypeTag::Label => write!(f, "label"),
            NodeTypeTag::Input => write!(f, "input"),
            NodeTypeTag::Button => write!(f, "button"),
            NodeTypeTag::Select => write!(f, "select"),
            NodeTypeTag::OptGroup => write!(f, "optgroup"),
            NodeTypeTag::SelectOption => write!(f, "option"),
            NodeTypeTag::TextArea => write!(f, "textarea"),
            NodeTypeTag::Output => write!(f, "output"),
            NodeTypeTag::Progress => write!(f, "progress"),
            NodeTypeTag::Meter => write!(f, "meter"),
            NodeTypeTag::DataList => write!(f, "datalist"),

            // Inline elements
            NodeTypeTag::Span => write!(f, "span"),
            NodeTypeTag::A => write!(f, "a"),
            NodeTypeTag::Em => write!(f, "em"),
            NodeTypeTag::Strong => write!(f, "strong"),
            NodeTypeTag::B => write!(f, "b"),
            NodeTypeTag::I => write!(f, "i"),
            NodeTypeTag::U => write!(f, "u"),
            NodeTypeTag::S => write!(f, "s"),
            NodeTypeTag::Mark => write!(f, "mark"),
            NodeTypeTag::Del => write!(f, "del"),
            NodeTypeTag::Ins => write!(f, "ins"),
            NodeTypeTag::Code => write!(f, "code"),
            NodeTypeTag::Samp => write!(f, "samp"),
            NodeTypeTag::Kbd => write!(f, "kbd"),
            NodeTypeTag::Var => write!(f, "var"),
            NodeTypeTag::Cite => write!(f, "cite"),
            NodeTypeTag::Dfn => write!(f, "dfn"),
            NodeTypeTag::Abbr => write!(f, "abbr"),
            NodeTypeTag::Acronym => write!(f, "acronym"),
            NodeTypeTag::Q => write!(f, "q"),
            NodeTypeTag::Time => write!(f, "time"),
            NodeTypeTag::Sub => write!(f, "sub"),
            NodeTypeTag::Sup => write!(f, "sup"),
            NodeTypeTag::Small => write!(f, "small"),
            NodeTypeTag::Big => write!(f, "big"),
            NodeTypeTag::Bdo => write!(f, "bdo"),
            NodeTypeTag::Bdi => write!(f, "bdi"),
            NodeTypeTag::Wbr => write!(f, "wbr"),
            NodeTypeTag::Ruby => write!(f, "ruby"),
            NodeTypeTag::Rt => write!(f, "rt"),
            NodeTypeTag::Rtc => write!(f, "rtc"),
            NodeTypeTag::Rp => write!(f, "rp"),
            NodeTypeTag::Data => write!(f, "data"),

            // Embedded content
            NodeTypeTag::Canvas => write!(f, "canvas"),
            NodeTypeTag::Object => write!(f, "object"),
            NodeTypeTag::Param => write!(f, "param"),
            NodeTypeTag::Embed => write!(f, "embed"),
            NodeTypeTag::Audio => write!(f, "audio"),
            NodeTypeTag::Video => write!(f, "video"),
            NodeTypeTag::Source => write!(f, "source"),
            NodeTypeTag::Track => write!(f, "track"),
            NodeTypeTag::Map => write!(f, "map"),
            NodeTypeTag::Area => write!(f, "area"),
            NodeTypeTag::Svg => write!(f, "svg"),

            // Metadata
            NodeTypeTag::Title => write!(f, "title"),
            NodeTypeTag::Meta => write!(f, "meta"),
            NodeTypeTag::Link => write!(f, "link"),
            NodeTypeTag::Script => write!(f, "script"),
            NodeTypeTag::Style => write!(f, "style"),
            NodeTypeTag::Base => write!(f, "base"),

            // Content elements
            NodeTypeTag::Text => write!(f, "text"),
            NodeTypeTag::Img => write!(f, "img"),
            NodeTypeTag::VirtualizedView => write!(f, "virtualized-view"),
            NodeTypeTag::Icon => write!(f, "icon"),

            // Pseudo-elements
            NodeTypeTag::Before => write!(f, "::before"),
            NodeTypeTag::After => write!(f, "::after"),
            NodeTypeTag::Marker => write!(f, "::marker"),
            NodeTypeTag::Placeholder => write!(f, "::placeholder"),
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

impl_vec!(CssPathSelector, CssPathSelectorVec, CssPathSelectorVecDestructor, CssPathSelectorVecDestructorType, CssPathSelectorVecSlice, OptionCssPathSelector);
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
    /// Represents the `>` selector (direct child)
    DirectChildren,
    /// Represents the ` ` selector (descendant)
    Children,
    /// Represents the `+` selector (adjacent sibling)
    AdjacentSibling,
    /// Represents the `~` selector (general sibling)
    GeneralSibling,
}

impl_option!(
    CssPathSelector,
    OptionCssPathSelector,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

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
            AdjacentSibling => write!(f, "+"),
            GeneralSibling => write!(f, "~"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
    /// `:lang(de)` - element matches language
    Lang(AzString),
    /// `:backdrop` - window is not focused (GTK compatibility)
    Backdrop,
    /// `:dragging` - element is currently being dragged
    Dragging,
    /// `:drag-over` - a dragged element is over this drop target
    DragOver,
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
    pub pattern_repeat: u32,
    pub offset: u32,
}

impl fmt::Display for CssNthChildSelector {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::CssNthChildSelector::*;
        match &self {
            Number(u) => write!(f, "{}", u),
            Even => write!(f, "even"),
            Odd => write!(f, "odd"),
            Pattern(p) => write!(f, "{}n + {}", p.pattern_repeat, p.offset),
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
            Lang(lang) => write!(f, "lang({})", lang.as_str()),
            Backdrop => write!(f, "backdrop"),
            Dragging => write!(f, "dragging"),
            DragOver => write!(f, "drag-over"),
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
pub fn get_specificity(path: &CssPath) -> (usize, usize, usize, usize) {
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

// --- Formatting ---

// High-level CSS to Rust code generation

pub fn css_to_rust_code(css: &Css) -> String {
    let mut output = String::new();

    output.push_str("const CSS: Css = Css {\r\n");
    output.push_str("\tstylesheets: [\r\n");

    for stylesheet in css.stylesheets.iter() {
        output.push_str("\t\tStylesheet {\r\n");
        output.push_str("\t\t\trules: [\r\n");

        for block in stylesheet.rules.iter() {
            output.push_str("\t\t\t\tCssRuleBlock: {\r\n");
            output.push_str(&format!(
                "\t\t\t\t\tpath: {},\r\n",
                print_block_path(&block.path, 5)
            ));

            output.push_str("\t\t\t\t\tdeclarations: [\r\n");

            for declaration in block.declarations.iter() {
                output.push_str(&format!(
                    "\t\t\t\t\t\t{},\r\n",
                    print_declaration(declaration, 6)
                ));
            }

            output.push_str("\t\t\t\t\t]\r\n");

            output.push_str("\t\t\t\t},\r\n");
        }

        output.push_str("\t\t\t]\r\n");
        output.push_str("\t\t},\r\n");
    }

    output.push_str("\t]\r\n");
    output.push_str("};");

    let output = output.replace("\t", "    ");

    output
}

pub fn format_node_type(n: &NodeTypeTag) -> &'static str {
    match n {
        // Document structure
        NodeTypeTag::Html => "NodeTypeTag::Html",
        NodeTypeTag::Head => "NodeTypeTag::Head",
        NodeTypeTag::Body => "NodeTypeTag::Body",

        // Block elements
        NodeTypeTag::Div => "NodeTypeTag::Div",
        NodeTypeTag::P => "NodeTypeTag::P",
        NodeTypeTag::Article => "NodeTypeTag::Article",
        NodeTypeTag::Section => "NodeTypeTag::Section",
        NodeTypeTag::Nav => "NodeTypeTag::Nav",
        NodeTypeTag::Aside => "NodeTypeTag::Aside",
        NodeTypeTag::Header => "NodeTypeTag::Header",
        NodeTypeTag::Footer => "NodeTypeTag::Footer",
        NodeTypeTag::Main => "NodeTypeTag::Main",
        NodeTypeTag::Figure => "NodeTypeTag::Figure",
        NodeTypeTag::FigCaption => "NodeTypeTag::FigCaption",

        // Headings
        NodeTypeTag::H1 => "NodeTypeTag::H1",
        NodeTypeTag::H2 => "NodeTypeTag::H2",
        NodeTypeTag::H3 => "NodeTypeTag::H3",
        NodeTypeTag::H4 => "NodeTypeTag::H4",
        NodeTypeTag::H5 => "NodeTypeTag::H5",
        NodeTypeTag::H6 => "NodeTypeTag::H6",

        // Text formatting
        NodeTypeTag::Br => "NodeTypeTag::Br",
        NodeTypeTag::Hr => "NodeTypeTag::Hr",
        NodeTypeTag::Pre => "NodeTypeTag::Pre",
        NodeTypeTag::BlockQuote => "NodeTypeTag::BlockQuote",
        NodeTypeTag::Address => "NodeTypeTag::Address",
        NodeTypeTag::Details => "NodeTypeTag::Details",
        NodeTypeTag::Summary => "NodeTypeTag::Summary",
        NodeTypeTag::Dialog => "NodeTypeTag::Dialog",

        // List elements
        NodeTypeTag::Ul => "NodeTypeTag::Ul",
        NodeTypeTag::Ol => "NodeTypeTag::Ol",
        NodeTypeTag::Li => "NodeTypeTag::Li",
        NodeTypeTag::Dl => "NodeTypeTag::Dl",
        NodeTypeTag::Dt => "NodeTypeTag::Dt",
        NodeTypeTag::Dd => "NodeTypeTag::Dd",
        NodeTypeTag::Menu => "NodeTypeTag::Menu",
        NodeTypeTag::MenuItem => "NodeTypeTag::MenuItem",
        NodeTypeTag::Dir => "NodeTypeTag::Dir",

        // Table elements
        NodeTypeTag::Table => "NodeTypeTag::Table",
        NodeTypeTag::Caption => "NodeTypeTag::Caption",
        NodeTypeTag::THead => "NodeTypeTag::THead",
        NodeTypeTag::TBody => "NodeTypeTag::TBody",
        NodeTypeTag::TFoot => "NodeTypeTag::TFoot",
        NodeTypeTag::Tr => "NodeTypeTag::Tr",
        NodeTypeTag::Th => "NodeTypeTag::Th",
        NodeTypeTag::Td => "NodeTypeTag::Td",
        NodeTypeTag::ColGroup => "NodeTypeTag::ColGroup",
        NodeTypeTag::Col => "NodeTypeTag::Col",

        // Form elements
        NodeTypeTag::Form => "NodeTypeTag::Form",
        NodeTypeTag::FieldSet => "NodeTypeTag::FieldSet",
        NodeTypeTag::Legend => "NodeTypeTag::Legend",
        NodeTypeTag::Label => "NodeTypeTag::Label",
        NodeTypeTag::Input => "NodeTypeTag::Input",
        NodeTypeTag::Button => "NodeTypeTag::Button",
        NodeTypeTag::Select => "NodeTypeTag::Select",
        NodeTypeTag::OptGroup => "NodeTypeTag::OptGroup",
        NodeTypeTag::SelectOption => "NodeTypeTag::SelectOption",
        NodeTypeTag::TextArea => "NodeTypeTag::TextArea",
        NodeTypeTag::Output => "NodeTypeTag::Output",
        NodeTypeTag::Progress => "NodeTypeTag::Progress",
        NodeTypeTag::Meter => "NodeTypeTag::Meter",
        NodeTypeTag::DataList => "NodeTypeTag::DataList",

        // Inline elements
        NodeTypeTag::Span => "NodeTypeTag::Span",
        NodeTypeTag::A => "NodeTypeTag::A",
        NodeTypeTag::Em => "NodeTypeTag::Em",
        NodeTypeTag::Strong => "NodeTypeTag::Strong",
        NodeTypeTag::B => "NodeTypeTag::B",
        NodeTypeTag::I => "NodeTypeTag::I",
        NodeTypeTag::U => "NodeTypeTag::U",
        NodeTypeTag::S => "NodeTypeTag::S",
        NodeTypeTag::Mark => "NodeTypeTag::Mark",
        NodeTypeTag::Del => "NodeTypeTag::Del",
        NodeTypeTag::Ins => "NodeTypeTag::Ins",
        NodeTypeTag::Code => "NodeTypeTag::Code",
        NodeTypeTag::Samp => "NodeTypeTag::Samp",
        NodeTypeTag::Kbd => "NodeTypeTag::Kbd",
        NodeTypeTag::Var => "NodeTypeTag::Var",
        NodeTypeTag::Cite => "NodeTypeTag::Cite",
        NodeTypeTag::Dfn => "NodeTypeTag::Dfn",
        NodeTypeTag::Abbr => "NodeTypeTag::Abbr",
        NodeTypeTag::Acronym => "NodeTypeTag::Acronym",
        NodeTypeTag::Q => "NodeTypeTag::Q",
        NodeTypeTag::Time => "NodeTypeTag::Time",
        NodeTypeTag::Sub => "NodeTypeTag::Sub",
        NodeTypeTag::Sup => "NodeTypeTag::Sup",
        NodeTypeTag::Small => "NodeTypeTag::Small",
        NodeTypeTag::Big => "NodeTypeTag::Big",
        NodeTypeTag::Bdo => "NodeTypeTag::Bdo",
        NodeTypeTag::Bdi => "NodeTypeTag::Bdi",
        NodeTypeTag::Wbr => "NodeTypeTag::Wbr",
        NodeTypeTag::Ruby => "NodeTypeTag::Ruby",
        NodeTypeTag::Rt => "NodeTypeTag::Rt",
        NodeTypeTag::Rtc => "NodeTypeTag::Rtc",
        NodeTypeTag::Rp => "NodeTypeTag::Rp",
        NodeTypeTag::Data => "NodeTypeTag::Data",

        // Embedded content
        NodeTypeTag::Canvas => "NodeTypeTag::Canvas",
        NodeTypeTag::Object => "NodeTypeTag::Object",
        NodeTypeTag::Param => "NodeTypeTag::Param",
        NodeTypeTag::Embed => "NodeTypeTag::Embed",
        NodeTypeTag::Audio => "NodeTypeTag::Audio",
        NodeTypeTag::Video => "NodeTypeTag::Video",
        NodeTypeTag::Source => "NodeTypeTag::Source",
        NodeTypeTag::Track => "NodeTypeTag::Track",
        NodeTypeTag::Map => "NodeTypeTag::Map",
        NodeTypeTag::Area => "NodeTypeTag::Area",
        NodeTypeTag::Svg => "NodeTypeTag::Svg",

        // Metadata
        NodeTypeTag::Title => "NodeTypeTag::Title",
        NodeTypeTag::Meta => "NodeTypeTag::Meta",
        NodeTypeTag::Link => "NodeTypeTag::Link",
        NodeTypeTag::Script => "NodeTypeTag::Script",
        NodeTypeTag::Style => "NodeTypeTag::Style",
        NodeTypeTag::Base => "NodeTypeTag::Base",

        // Content elements
        NodeTypeTag::Text => "NodeTypeTag::Text",
        NodeTypeTag::Img => "NodeTypeTag::Img",
        NodeTypeTag::VirtualizedView => "NodeTypeTag::VirtualizedView",
        NodeTypeTag::Icon => "NodeTypeTag::Icon",

        // Pseudo-elements
        NodeTypeTag::Before => "NodeTypeTag::Before",
        NodeTypeTag::After => "NodeTypeTag::After",
        NodeTypeTag::Marker => "NodeTypeTag::Marker",
        NodeTypeTag::Placeholder => "NodeTypeTag::Placeholder",
    }
}

pub fn print_block_path(path: &CssPath, tabs: usize) -> String {
    let t = String::from("    ").repeat(tabs);
    let t1 = String::from("    ").repeat(tabs + 1);

    format!(
        "CssPath {{\r\n{}selectors: {}\r\n{}}}",
        t1,
        format_selectors(path.selectors.as_ref(), tabs + 1),
        t
    )
}

pub fn format_selectors(selectors: &[CssPathSelector], tabs: usize) -> String {
    let t = String::from("    ").repeat(tabs);
    let t1 = String::from("    ").repeat(tabs + 1);

    let selectors_formatted = selectors
        .iter()
        .map(|s| format!("{}{},", t1, format_single_selector(s, tabs + 1)))
        .collect::<Vec<String>>()
        .join("\r\n");

    format!("vec![\r\n{}\r\n{}].into()", selectors_formatted, t)
}

pub fn format_single_selector(p: &CssPathSelector, _tabs: usize) -> String {
    match p {
        CssPathSelector::Global => format!("CssPathSelector::Global"),
        CssPathSelector::Type(ntp) => format!("CssPathSelector::Type({})", format_node_type(ntp)),
        CssPathSelector::Class(class) => {
            format!("CssPathSelector::Class(String::from({:?}))", class)
        }
        CssPathSelector::Id(id) => format!("CssPathSelector::Id(String::from({:?}))", id),
        CssPathSelector::PseudoSelector(cps) => format!(
            "CssPathSelector::PseudoSelector({})",
            format_pseudo_selector_type(cps)
        ),
        CssPathSelector::DirectChildren => format!("CssPathSelector::DirectChildren"),
        CssPathSelector::Children => format!("CssPathSelector::Children"),
        CssPathSelector::AdjacentSibling => format!("CssPathSelector::AdjacentSibling"),
        CssPathSelector::GeneralSibling => format!("CssPathSelector::GeneralSibling"),
    }
}

pub fn format_pseudo_selector_type(p: &CssPathPseudoSelector) -> String {
    match p {
        CssPathPseudoSelector::First => format!("CssPathPseudoSelector::First"),
        CssPathPseudoSelector::Last => format!("CssPathPseudoSelector::Last"),
        CssPathPseudoSelector::NthChild(n) => format!(
            "CssPathPseudoSelector::NthChild({})",
            format_nth_child_selector(n)
        ),
        CssPathPseudoSelector::Hover => format!("CssPathPseudoSelector::Hover"),
        CssPathPseudoSelector::Active => format!("CssPathPseudoSelector::Active"),
        CssPathPseudoSelector::Focus => format!("CssPathPseudoSelector::Focus"),
        CssPathPseudoSelector::Backdrop => format!("CssPathPseudoSelector::Backdrop"),
        CssPathPseudoSelector::Lang(lang) => format!(
            "CssPathPseudoSelector::Lang(AzString::from_const_str(\"{}\"))",
            lang.as_str()
        ),
        CssPathPseudoSelector::Dragging => format!("CssPathPseudoSelector::Dragging"),
        CssPathPseudoSelector::DragOver => format!("CssPathPseudoSelector::DragOver"),
    }
}

pub fn format_nth_child_selector(n: &CssNthChildSelector) -> String {
    match n {
        CssNthChildSelector::Number(num) => format!("CssNthChildSelector::Number({})", num),
        CssNthChildSelector::Even => format!("CssNthChildSelector::Even"),
        CssNthChildSelector::Odd => format!("CssNthChildSelector::Odd"),
        CssNthChildSelector::Pattern(CssNthChildPattern {
            pattern_repeat,
            offset,
        }) => format!(
            "CssNthChildSelector::Pattern(CssNthChildPattern {{ pattern_repeat: {}, offset: {} }})",
            pattern_repeat, offset
        ),
    }
}

pub fn print_declaration(decl: &CssDeclaration, tabs: usize) -> String {
    match decl {
        CssDeclaration::Static(s) => format!(
            "CssDeclaration::Static({})",
            format_static_css_prop(s, tabs)
        ),
        CssDeclaration::Dynamic(d) => format!(
            "CssDeclaration::Dynamic({})",
            format_dynamic_css_prop(d, tabs)
        ),
    }
}

pub fn format_dynamic_css_prop(decl: &DynamicCssProperty, tabs: usize) -> String {
    let t = String::from("    ").repeat(tabs);
    format!(
        "DynamicCssProperty {{\r\n{}    dynamic_id: {:?},\r\n{}    default_value: {},\r\n{}}}",
        t,
        decl.dynamic_id,
        t,
        format_static_css_prop(&decl.default_value, tabs + 1),
        t
    )
}
