//! Types and methods used to describe the style of an application.
//!
//! This module defines the core CSS data model:
//!
//! - [`Css`] contains one or more [`Stylesheet`]s, each holding [`CssRuleBlock`]s.
//! - A [`CssRuleBlock`] pairs a [`CssPath`] (selector) with [`CssDeclaration`]s (properties).
//! - [`CssPropertyValue<T>`] wraps individual property values with CSS keywords
//!   (`auto`, `inherit`, `initial`, etc.).
//! - [`BoxOrStatic<T>`] is a smart-pointer enum for heap-allocated or static CSS values.
//! - [`NodeTypeTag`] enumerates all recognized HTML/SVG element types for selector matching.
use alloc::{string::String, vec::Vec};
use core::fmt;

use crate::{
    corety::OptionString,
    dynamic_selector::DynamicSelectorVec,
    props::property::{CssProperty, CssPropertyType},
    AzString,
};

/// Css stylesheet - contains a parsed CSS stylesheet in "rule blocks",
/// i.e. blocks of key-value pairs associated with a selector path.
///
/// Layer separation (UA / system / author / inline / runtime) is encoded
/// per-rule via `CssRuleBlock::priority`; see [`rule_priority`] for the
/// slot allocation. There is no separate `Stylesheet` wrapper — to merge
/// two CSS sources, concatenate their `rules` and re-sort.
#[derive(Debug, Default, PartialEq, PartialOrd, Clone)]
#[repr(C)]
pub struct Css {
    /// All rule blocks, in source order. Sort by `(priority, specificity)`
    /// via `sort_by_specificity` to put them in cascade order.
    pub rules: CssRuleBlockVec,
}

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

impl_vec!(CssRuleBlock, CssRuleBlockVec, CssRuleBlockVecDestructor, CssRuleBlockVecDestructorType, CssRuleBlockVecSlice, OptionCssRuleBlock);
impl_vec_mut!(CssRuleBlock, CssRuleBlockVec);
impl_vec_debug!(CssRuleBlock, CssRuleBlockVec);
impl_vec_partialord!(CssRuleBlock, CssRuleBlockVec);
impl_vec_clone!(CssRuleBlock, CssRuleBlockVec, CssRuleBlockVecDestructor);
impl_vec_partialeq!(CssRuleBlock, CssRuleBlockVec);

impl Css {
    pub fn is_empty(&self) -> bool {
        self.rules.as_ref().is_empty()
    }

    pub fn new(rules: Vec<CssRuleBlock>) -> Self {
        Self {
            rules: rules.into(),
        }
    }

    #[cfg(feature = "parser")]
    pub fn from_string(s: crate::AzString) -> Self {
        crate::parser2::new_from_str(s.as_str()).0
    }

    /// Parse inline-style CSS (bare properties, pseudo blocks, @-rule blocks)
    /// and return a `Css` whose rules carry `rule_priority::INLINE`.
    ///
    /// Wraps the input in `* { ... }` so the main CSS parser can handle bare
    /// properties at the top level. Pseudo and at-rule blocks like
    /// `:hover { color: red; }` or `@os(linux) { font-size: 14px; }` work
    /// directly via CSS nesting.
    #[cfg(feature = "parser")]
    pub fn parse_inline(style: &str) -> Self {
        use alloc::string::ToString;
        let mut wrapped = String::with_capacity(style.len() + 6);
        wrapped.push_str("* {\n");
        wrapped.push_str(style);
        wrapped.push_str("\n}");
        let (mut css, _warnings) = crate::parser2::new_from_str(&wrapped);
        for rule in css.rules.as_mut() {
            rule.priority = rule_priority::INLINE;
        }
        css
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

impl From<Vec<CssRuleBlock>> for Css {
    fn from(rules: Vec<CssRuleBlock>) -> Self {
        Self {
            rules: rules.into(),
        }
    }
}

// NodeData derives Eq + Ord and carries `Css` as its inline style. Provide
// length-based ordering so the derives keep working — the same pattern the
// previous `CssPropertyWithConditionsVec` used.
impl Eq for Css {}
impl Ord for Css {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.rules.as_ref().len().cmp(&other.rules.as_ref().len())
    }
}
impl Eq for CssRuleBlock {}
impl Ord for CssRuleBlock {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        // Match the existing PartialOrd: path first, then declarations.
        // Priority is intentionally not in the sort key — it's a layer label,
        // not a comparison primitive for callers.
        self.path.cmp(&other.path).then_with(|| self.declarations.cmp(&other.declarations))
    }
}

/// Convert a flat list of `CssPropertyWithConditions` (the legacy inline-CSS form)
/// into a `Css`. Each property becomes a single-declaration `CssRuleBlock` with
/// `priority = INLINE`, an empty path (implicitly `:scope` — applies to the node it
/// lives on), and the original conditions intact.
///
/// This bridge lets widget code that built `&[CssPropertyWithConditions]` arrays
/// keep working through `.into()` while the storage on `NodeData` is the unified
/// `Css` type.
impl From<crate::dynamic_selector::CssPropertyWithConditionsVec> for Css {
    fn from(props: crate::dynamic_selector::CssPropertyWithConditionsVec) -> Self {
        let rules: Vec<CssRuleBlock> = props.into_library_owned_vec().into_iter().map(|p| {
            CssRuleBlock {
                path: CssPath { selectors: Vec::new().into() },
                declarations: alloc::vec![CssDeclaration::Static(p.property)].into(),
                conditions: p.apply_if,
                priority: rule_priority::INLINE,
            }
        }).collect();
        Css { rules: rules.into() }
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
    ///
    /// # Safety invariant
    /// The inner pointer must be non-null. This is guaranteed by [`heap`](Self::heap)
    /// and the `Static` constructor (which should always point to valid data).
    #[inline]
    pub fn as_ref(&self) -> &T {
        match self {
            BoxOrStatic::Boxed(ptr) => unsafe {
                debug_assert!(!ptr.is_null(), "BoxOrStatic::Boxed contained a null pointer");
                &**ptr
            },
            BoxOrStatic::Static(ptr) => unsafe {
                debug_assert!(!ptr.is_null(), "BoxOrStatic::Static contained a null pointer");
                &**ptr
            },
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

/// Type alias: `BoxOrStatic<AzString>` — used by NodeType::Text and NodeType::Icon.
pub type BoxOrStaticString = BoxOrStatic<crate::AzString>;

/// A CSS property value that may be an explicit value or a CSS-wide keyword
/// (`auto`, `none`, `initial`, `inherit`, `revert`, `unset`).
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

/// Trait for types that can format themselves as a CSS property value string.
pub trait PrintAsCssValue {
    fn print_as_css_value(&self) -> String;
}

impl<T: PrintAsCssValue> CssPropertyValue<T> {
    pub fn get_css_value_fmt(&self) -> String {
        match self {
            CssPropertyValue::Auto => "auto".to_string(),
            CssPropertyValue::None => "none".to_string(),
            CssPropertyValue::Initial => "initial".to_string(),
            CssPropertyValue::Inherit => "inherit".to_string(),
            CssPropertyValue::Revert => "revert".to_string(),
            CssPropertyValue::Unset => "unset".to_string(),
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
        matches!(self, CssPropertyValue::Auto)
    }

    #[inline]
    pub fn is_none(&self) -> bool {
        matches!(self, CssPropertyValue::None)
    }

    #[inline]
    pub fn is_initial(&self) -> bool {
        matches!(self, CssPropertyValue::Initial)
    }

    #[inline]
    pub fn is_inherit(&self) -> bool {
        matches!(self, CssPropertyValue::Inherit)
    }

    #[inline]
    pub fn is_revert(&self) -> bool {
        matches!(self, CssPropertyValue::Revert)
    }

    #[inline]
    pub fn is_unset(&self) -> bool {
        matches!(self, CssPropertyValue::Unset)
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

/// Layer priority for `CssRuleBlock`. Lower numbers cascade first;
/// higher numbers override earlier layers at the same specificity.
///
/// `u8` leaves 256 slots, so a new layer can be inserted between any
/// two existing slots without renumbering consumers. The gaps between
/// named slots are intentional — fill them with custom intermediate
/// layers if/when `@layer` lands.
pub mod rule_priority {
    /// User-Agent / framework defaults. Widget code that emits its
    /// own default CSS uses this. Lowest priority — anything else
    /// overrides it.
    pub const UA: u8 = 0;

    /// Stylesheets the host system reports (system fonts, theme CSS
    /// derived from `SystemStyle`). One step above UA so they win
    /// against framework defaults but lose against anything the app
    /// author writes.
    pub const SYSTEM: u8 = 10;

    /// Default for parser-produced rules: the app author's CSS.
    /// Everything coming out of `Css::from_string` lives here.
    pub const AUTHOR: u8 = 20;

    /// Inline `style="..."` / `NodeData::set_css(...)` rules — used
    /// once the inline-vs-component unification (separate plan) folds
    /// inline storage into the same Vec.
    pub const INLINE: u8 = 30;

    /// Reserved for direct-rule runtime overrides. Today the
    /// prop_cache handles runtime overrides via
    /// `user_overridden_properties`; this slot is reserved so a
    /// future "push a CssRuleBlock at runtime" path stays above
    /// inline. Used only when a callback writes a full rule, not a
    /// single property.
    pub const RUNTIME: u8 = 50;
}

/// One block of rules that applies a bunch of rules to a "path" in the style, i.e.
/// `div#myid.myclass -> { ("justify-content", "center") }`
///
/// The `conditions` field contains @media/@lang/etc. conditions that must ALL be
/// satisfied for this rule block to apply (from enclosing @-rule blocks).
#[derive(Debug, Default, Clone, PartialEq)]
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
    /// Layer priority. See [`rule_priority`] for slot allocation.
    /// `0` = UA / framework, `20` = author CSS (default), higher = wins.
    /// Sort key combined with selector specificity in `sort_by_specificity`.
    pub priority: u8,
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
            priority: rule_priority::AUTHOR,
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
            priority: rule_priority::AUTHOR,
        }
    }
}

/// A group of CSS path selectors, used during selector matching.
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
    /// SVG `<path>` element.
    SvgPath,
    /// SVG `<circle>` element.
    SvgCircle,
    /// SVG `<rect>` element.
    SvgRect,
    /// SVG `<ellipse>` element.
    SvgEllipse,
    /// SVG `<line>` element.
    SvgLine,
    /// SVG `<polygon>` element.
    SvgPolygon,
    /// SVG `<polyline>` element.
    SvgPolyline,
    /// SVG `<g>` group element.
    SvgG,

    // SVG container elements
    /// SVG `<defs>` element.
    SvgDefs,
    /// SVG `<symbol>` element.
    SvgSymbol,
    /// SVG `<use>` element.
    SvgUse,
    /// SVG `<switch>` element.
    SvgSwitch,

    // SVG text elements
    /// SVG `<text>` element.
    SvgText,
    /// SVG `<tspan>` element.
    SvgTspan,
    /// SVG `<textPath>` element.
    SvgTextPath,

    // SVG paint server elements
    /// SVG `<linearGradient>` element.
    SvgLinearGradient,
    /// SVG `<radialGradient>` element.
    SvgRadialGradient,
    /// SVG `<stop>` element.
    SvgStop,
    /// SVG `<pattern>` element.
    SvgPattern,

    // SVG clipping/masking elements
    /// SVG `<clipPath>` element.
    SvgClipPathElement,
    /// SVG `<mask>` element.
    SvgMask,

    // SVG filter elements
    /// SVG `<filter>` element.
    SvgFilter,
    /// SVG `<feBlend>` element.
    SvgFeBlend,
    /// SVG `<feColorMatrix>` element.
    SvgFeColorMatrix,
    /// SVG `<feComponentTransfer>` element.
    SvgFeComponentTransfer,
    /// SVG `<feComposite>` element.
    SvgFeComposite,
    /// SVG `<feConvolveMatrix>` element.
    SvgFeConvolveMatrix,
    /// SVG `<feDiffuseLighting>` element.
    SvgFeDiffuseLighting,
    /// SVG `<feDisplacementMap>` element.
    SvgFeDisplacementMap,
    /// SVG `<feDistantLight>` element.
    SvgFeDistantLight,
    /// SVG `<feDropShadow>` element.
    SvgFeDropShadow,
    /// SVG `<feFlood>` element.
    SvgFeFlood,
    /// SVG `<feFuncR>` element.
    SvgFeFuncR,
    /// SVG `<feFuncG>` element.
    SvgFeFuncG,
    /// SVG `<feFuncB>` element.
    SvgFeFuncB,
    /// SVG `<feFuncA>` element.
    SvgFeFuncA,
    /// SVG `<feGaussianBlur>` element.
    SvgFeGaussianBlur,
    /// SVG `<feImage>` element.
    SvgFeImage,
    /// SVG `<feMerge>` element.
    SvgFeMerge,
    /// SVG `<feMergeNode>` element.
    SvgFeMergeNode,
    /// SVG `<feMorphology>` element.
    SvgFeMorphology,
    /// SVG `<feOffset>` element.
    SvgFeOffset,
    /// SVG `<fePointLight>` element.
    SvgFePointLight,
    /// SVG `<feSpecularLighting>` element.
    SvgFeSpecularLighting,
    /// SVG `<feSpotLight>` element.
    SvgFeSpotLight,
    /// SVG `<feTile>` element.
    SvgFeTile,
    /// SVG `<feTurbulence>` element.
    SvgFeTurbulence,

    // SVG marker/image elements
    /// SVG `<marker>` element.
    SvgMarker,
    /// SVG `<image>` element.
    SvgImage,
    /// SVG `<foreignObject>` element.
    SvgForeignObject,

    // SVG descriptive elements
    /// SVG `<title>` element.
    SvgTitle,
    /// SVG `<desc>` element.
    SvgDesc,
    /// SVG `<metadata>` element.
    SvgMetadata,
    /// SVG `<a>` element.
    SvgA,
    /// SVG `<view>` element.
    SvgView,
    /// SVG `<style>` element.
    SvgStyle,
    /// SVG `<script>` element.
    SvgScript,

    // SVG animation elements
    /// SVG `<animate>` element.
    SvgAnimate,
    /// SVG `<animateMotion>` element.
    SvgAnimateMotion,
    /// SVG `<animateTransform>` element.
    SvgAnimateTransform,
    /// SVG `<set>` element.
    SvgSet,
    /// SVG `<mpath>` element.
    SvgMpath,

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
    VirtualView,
    /// Icon element - resolved to actual content by IconProvider
    Icon,
    /// Invisible probe — `NodeType::GeolocationProbe`. Zero-size in
    /// layout, skipped in the display list. CSS tag: `geolocation-probe`.
    GeolocationProbe,

    // Pseudo-elements
    Before,
    After,
    Marker,
    Placeholder,
}

/// Error returned when a CSS tag name string cannot be mapped to a [`NodeTypeTag`].
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

/// Owned version of [`NodeTypeTagParseError`] for storage across lifetime boundaries.
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
    pub fn from_str(css_key: &str) -> Result<Self, NodeTypeTagParseError<'_>> {
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

            // SVG shape elements
            "path" => Ok(NodeTypeTag::SvgPath),
            "circle" => Ok(NodeTypeTag::SvgCircle),
            "rect" => Ok(NodeTypeTag::SvgRect),
            "ellipse" => Ok(NodeTypeTag::SvgEllipse),
            "line" => Ok(NodeTypeTag::SvgLine),
            "polygon" => Ok(NodeTypeTag::SvgPolygon),
            "polyline" => Ok(NodeTypeTag::SvgPolyline),
            "g" => Ok(NodeTypeTag::SvgG),

            // SVG container elements
            "defs" => Ok(NodeTypeTag::SvgDefs),
            "symbol" => Ok(NodeTypeTag::SvgSymbol),
            "use" => Ok(NodeTypeTag::SvgUse),
            "switch" => Ok(NodeTypeTag::SvgSwitch),

            // SVG text elements
            "svg:text" => Ok(NodeTypeTag::SvgText),
            "tspan" => Ok(NodeTypeTag::SvgTspan),
            "textpath" => Ok(NodeTypeTag::SvgTextPath),

            // SVG paint server elements
            "lineargradient" => Ok(NodeTypeTag::SvgLinearGradient),
            "radialgradient" => Ok(NodeTypeTag::SvgRadialGradient),
            "stop" => Ok(NodeTypeTag::SvgStop),
            "pattern" => Ok(NodeTypeTag::SvgPattern),

            // SVG clipping/masking elements
            "clippath" => Ok(NodeTypeTag::SvgClipPathElement),
            "mask" => Ok(NodeTypeTag::SvgMask),

            // SVG filter elements
            "filter" => Ok(NodeTypeTag::SvgFilter),
            "feblend" => Ok(NodeTypeTag::SvgFeBlend),
            "fecolormatrix" => Ok(NodeTypeTag::SvgFeColorMatrix),
            "fecomponenttransfer" => Ok(NodeTypeTag::SvgFeComponentTransfer),
            "fecomposite" => Ok(NodeTypeTag::SvgFeComposite),
            "feconvolvematrix" => Ok(NodeTypeTag::SvgFeConvolveMatrix),
            "fediffuselighting" => Ok(NodeTypeTag::SvgFeDiffuseLighting),
            "fedisplacementmap" => Ok(NodeTypeTag::SvgFeDisplacementMap),
            "fedistantlight" => Ok(NodeTypeTag::SvgFeDistantLight),
            "fedropshadow" => Ok(NodeTypeTag::SvgFeDropShadow),
            "feflood" => Ok(NodeTypeTag::SvgFeFlood),
            "fefuncr" => Ok(NodeTypeTag::SvgFeFuncR),
            "fefuncg" => Ok(NodeTypeTag::SvgFeFuncG),
            "fefuncb" => Ok(NodeTypeTag::SvgFeFuncB),
            "fefunca" => Ok(NodeTypeTag::SvgFeFuncA),
            "fegaussianblur" => Ok(NodeTypeTag::SvgFeGaussianBlur),
            "feimage" => Ok(NodeTypeTag::SvgFeImage),
            "femerge" => Ok(NodeTypeTag::SvgFeMerge),
            "femergenode" => Ok(NodeTypeTag::SvgFeMergeNode),
            "femorphology" => Ok(NodeTypeTag::SvgFeMorphology),
            "feoffset" => Ok(NodeTypeTag::SvgFeOffset),
            "fepointlight" => Ok(NodeTypeTag::SvgFePointLight),
            "fespecularlighting" => Ok(NodeTypeTag::SvgFeSpecularLighting),
            "fespotlight" => Ok(NodeTypeTag::SvgFeSpotLight),
            "fetile" => Ok(NodeTypeTag::SvgFeTile),
            "feturbulence" => Ok(NodeTypeTag::SvgFeTurbulence),

            // SVG marker/image elements
            "image" | "svg:image" => Ok(NodeTypeTag::SvgImage),
            "svg:marker" => Ok(NodeTypeTag::SvgMarker),
            "foreignobject" => Ok(NodeTypeTag::SvgForeignObject),

            // SVG descriptive elements
            "svg:title" => Ok(NodeTypeTag::SvgTitle),
            "svg:a" => Ok(NodeTypeTag::SvgA),
            "svg:style" => Ok(NodeTypeTag::SvgStyle),
            "svg:script" => Ok(NodeTypeTag::SvgScript),
            "desc" => Ok(NodeTypeTag::SvgDesc),
            "metadata" => Ok(NodeTypeTag::SvgMetadata),
            "view" => Ok(NodeTypeTag::SvgView),

            // SVG animation elements
            "animate" => Ok(NodeTypeTag::SvgAnimate),
            "animatemotion" => Ok(NodeTypeTag::SvgAnimateMotion),
            "animatetransform" => Ok(NodeTypeTag::SvgAnimateTransform),
            "set" => Ok(NodeTypeTag::SvgSet),
            "mpath" => Ok(NodeTypeTag::SvgMpath),

            // Metadata
            "title" => Ok(NodeTypeTag::Title),
            "meta" => Ok(NodeTypeTag::Meta),
            "link" => Ok(NodeTypeTag::Link),
            "script" => Ok(NodeTypeTag::Script),
            "style" => Ok(NodeTypeTag::Style),
            "base" => Ok(NodeTypeTag::Base),

            // Special
            "img" => Ok(NodeTypeTag::Img),
            "virtual-view" | "iframe" => Ok(NodeTypeTag::VirtualView),
            "icon" => Ok(NodeTypeTag::Icon),
            "geolocation-probe" => Ok(NodeTypeTag::GeolocationProbe),

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
            NodeTypeTag::SvgPath => write!(f, "path"),
            NodeTypeTag::SvgCircle => write!(f, "circle"),
            NodeTypeTag::SvgRect => write!(f, "rect"),
            NodeTypeTag::SvgEllipse => write!(f, "ellipse"),
            NodeTypeTag::SvgLine => write!(f, "line"),
            NodeTypeTag::SvgPolygon => write!(f, "polygon"),
            NodeTypeTag::SvgPolyline => write!(f, "polyline"),
            NodeTypeTag::SvgG => write!(f, "g"),

            // SVG container elements
            NodeTypeTag::SvgDefs => write!(f, "defs"),
            NodeTypeTag::SvgSymbol => write!(f, "symbol"),
            NodeTypeTag::SvgUse => write!(f, "use"),
            NodeTypeTag::SvgSwitch => write!(f, "switch"),

            // SVG text elements
            NodeTypeTag::SvgText => write!(f, "svg:text"),
            NodeTypeTag::SvgTspan => write!(f, "tspan"),
            NodeTypeTag::SvgTextPath => write!(f, "textpath"),

            // SVG paint server elements
            NodeTypeTag::SvgLinearGradient => write!(f, "lineargradient"),
            NodeTypeTag::SvgRadialGradient => write!(f, "radialgradient"),
            NodeTypeTag::SvgStop => write!(f, "stop"),
            NodeTypeTag::SvgPattern => write!(f, "pattern"),

            // SVG clipping/masking elements
            NodeTypeTag::SvgClipPathElement => write!(f, "clippath"),
            NodeTypeTag::SvgMask => write!(f, "mask"),

            // SVG filter elements
            NodeTypeTag::SvgFilter => write!(f, "filter"),
            NodeTypeTag::SvgFeBlend => write!(f, "feblend"),
            NodeTypeTag::SvgFeColorMatrix => write!(f, "fecolormatrix"),
            NodeTypeTag::SvgFeComponentTransfer => write!(f, "fecomponenttransfer"),
            NodeTypeTag::SvgFeComposite => write!(f, "fecomposite"),
            NodeTypeTag::SvgFeConvolveMatrix => write!(f, "feconvolvematrix"),
            NodeTypeTag::SvgFeDiffuseLighting => write!(f, "fediffuselighting"),
            NodeTypeTag::SvgFeDisplacementMap => write!(f, "fedisplacementmap"),
            NodeTypeTag::SvgFeDistantLight => write!(f, "fedistantlight"),
            NodeTypeTag::SvgFeDropShadow => write!(f, "fedropshadow"),
            NodeTypeTag::SvgFeFlood => write!(f, "feflood"),
            NodeTypeTag::SvgFeFuncR => write!(f, "fefuncr"),
            NodeTypeTag::SvgFeFuncG => write!(f, "fefuncg"),
            NodeTypeTag::SvgFeFuncB => write!(f, "fefuncb"),
            NodeTypeTag::SvgFeFuncA => write!(f, "fefunca"),
            NodeTypeTag::SvgFeGaussianBlur => write!(f, "fegaussianblur"),
            NodeTypeTag::SvgFeImage => write!(f, "feimage"),
            NodeTypeTag::SvgFeMerge => write!(f, "femerge"),
            NodeTypeTag::SvgFeMergeNode => write!(f, "femergenode"),
            NodeTypeTag::SvgFeMorphology => write!(f, "femorphology"),
            NodeTypeTag::SvgFeOffset => write!(f, "feoffset"),
            NodeTypeTag::SvgFePointLight => write!(f, "fepointlight"),
            NodeTypeTag::SvgFeSpecularLighting => write!(f, "fespecularlighting"),
            NodeTypeTag::SvgFeSpotLight => write!(f, "fespotlight"),
            NodeTypeTag::SvgFeTile => write!(f, "fetile"),
            NodeTypeTag::SvgFeTurbulence => write!(f, "feturbulence"),

            // SVG marker/image elements
            NodeTypeTag::SvgMarker => write!(f, "svg:marker"),
            NodeTypeTag::SvgImage => write!(f, "svg:image"),
            NodeTypeTag::SvgForeignObject => write!(f, "foreignobject"),

            // SVG descriptive elements
            NodeTypeTag::SvgTitle => write!(f, "svg:title"),
            NodeTypeTag::SvgDesc => write!(f, "desc"),
            NodeTypeTag::SvgMetadata => write!(f, "metadata"),
            NodeTypeTag::SvgA => write!(f, "svg:a"),
            NodeTypeTag::SvgView => write!(f, "view"),
            NodeTypeTag::SvgStyle => write!(f, "svg:style"),
            NodeTypeTag::SvgScript => write!(f, "svg:script"),

            // SVG animation elements
            NodeTypeTag::SvgAnimate => write!(f, "animate"),
            NodeTypeTag::SvgAnimateMotion => write!(f, "animatemotion"),
            NodeTypeTag::SvgAnimateTransform => write!(f, "animatetransform"),
            NodeTypeTag::SvgSet => write!(f, "set"),
            NodeTypeTag::SvgMpath => write!(f, "mpath"),

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
            NodeTypeTag::VirtualView => write!(f, "virtual-view"),
            NodeTypeTag::Icon => write!(f, "icon"),
            NodeTypeTag::GeolocationProbe => write!(f, "geolocation-probe"),

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

    /// Prepend a `Root(range)` scope selector (push_front), confining this rule to
    /// the subtree `range`. Used to scope inline (`with_css`/`set_css`) css to its
    /// owning node's subtree so it cannot leak to the whole tree (#47). Because
    /// there is no combinator between `Root(range)` and the original leading `*`
    /// wrapper, they compound: a bare-decl rule `[Global]` becomes
    /// `[Root(range), Global]` (= the subtree), and a nested selector keeps
    /// matching, now confined to the subtree.
    pub fn push_front_scope(&mut self, range: CssScopeRange) {
        let mut selectors = Vec::with_capacity(self.selectors.as_ref().len() + 1);
        selectors.push(CssPathSelector::Root(range));
        selectors.extend(self.selectors.as_ref().iter().cloned());
        self.selectors = selectors.into();
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

/// Inclusive range of flat `NodeId`s describing a node's subtree `[start, end]`
/// (`end = start + estimated_total_children`, since the flat arena lays subtrees
/// out contiguously). Carried by [`CssPathSelector::Root`] to scope inline css to
/// a subtree, and is the unit of future parallel per-subtree cascading.
/// `repr(C)` for FFI / api.json codegen.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct CssScopeRange {
    /// First flat NodeId of the subtree (the owning node itself).
    pub start: usize,
    /// Last flat NodeId of the subtree, inclusive (`start` for a leaf).
    pub end: usize,
}

impl CssScopeRange {
    /// True if `node` (a flat NodeId index) is inside this subtree range.
    #[inline]
    pub fn contains(&self, node: usize) -> bool {
        self.start <= node && node <= self.end
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C, u8)]
#[derive(Default)]
pub enum CssPathSelector {
    /// Represents the `*` selector
    #[default]
    Global,
    /// Scope marker carrying a node's **subtree range** `[start, end]` (inclusive
    /// flat `NodeId`s; `end = start + estimated_total_children`). Matches a node
    /// iff `start <= node <= end`. Synthesized at flatten time and `push_front`-ed
    /// onto every inline (`with_css`/`set_css`) rule's path, so the rule compounds
    /// with the `parse_inline` `*` wrapper (`[Root(s,e), Global, …]`) and is scoped
    /// to that node's subtree instead of leaking to the whole tree (#47). Because
    /// the flat arena lays subtrees out contiguously, this range is also the unit
    /// of future parallel per-subtree cascading.
    Root(CssScopeRange),
    /// `div`, `p`, etc.
    Type(NodeTypeTag),
    /// `.something`
    Class(AzString),
    /// `#something`
    Id(AzString),
    /// `:something`
    PseudoSelector(CssPathPseudoSelector),
    /// `[attr]`, `[attr="value"]`, `[attr~="value"]`, etc.
    Attribute(CssAttributeSelector),
    /// Represents the `>` selector (direct child)
    DirectChildren,
    /// Represents the ` ` selector (descendant)
    Children,
    /// Represents the `+` selector (adjacent sibling)
    AdjacentSibling,
    /// Represents the `~` selector (general sibling)
    GeneralSibling,
}

/// Attribute selector (`[attr]`, `[attr="value"]`, ...).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct CssAttributeSelector {
    pub name: AzString,
    pub op: AttributeMatchOp,
    pub value: OptionString,
}

impl Default for CssAttributeSelector {
    fn default() -> Self {
        Self {
            name: AzString::default(),
            op: AttributeMatchOp::Exists,
            value: OptionString::None,
        }
    }
}

/// Operator that compares an attribute value against a target string.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub enum AttributeMatchOp {
    /// `[attr]` — attribute is present (any value).
    Exists,
    /// `[attr="value"]` — attribute equals value exactly.
    Eq,
    /// `[attr~="value"]` — value is one of the whitespace-separated words.
    Includes,
    /// `[attr|="value"]` — value equals exactly OR begins with value followed by `-`.
    DashMatch,
    /// `[attr^="value"]` — value starts with the given prefix.
    Prefix,
    /// `[attr$="value"]` — value ends with the given suffix.
    Suffix,
    /// `[attr*="value"]` — value contains the given substring.
    Substring,
}

impl Default for AttributeMatchOp {
    fn default() -> Self {
        AttributeMatchOp::Exists
    }
}

impl_option!(
    CssPathSelector,
    OptionCssPathSelector,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);


impl fmt::Display for CssPathSelector {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::CssPathSelector::*;
        match &self {
            Global => write!(f, "*"),
            Root(r) => write!(f, ":root({}..={})", r.start, r.end),
            Type(n) => write!(f, "{}", n),
            Class(c) => write!(f, ".{}", c),
            Id(i) => write!(f, "#{}", i),
            PseudoSelector(p) => write!(f, ":{}", p),
            Attribute(a) => write!(f, "{}", a),
            DirectChildren => write!(f, ">"),
            Children => write!(f, " "),
            AdjacentSibling => write!(f, "+"),
            GeneralSibling => write!(f, "~"),
        }
    }
}

impl fmt::Display for CssAttributeSelector {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match (&self.op, self.value.as_ref()) {
            (AttributeMatchOp::Exists, _) => write!(f, "[{}]", self.name),
            (op, Some(v)) => write!(f, "[{}{}=\"{}\"]", self.name, op.symbol_prefix(), v),
            (op, None) => write!(f, "[{}{}=\"\"]", self.name, op.symbol_prefix()),
        }
    }
}

impl AttributeMatchOp {
    /// Returns the prefix character for the `=` operator (e.g. `~` for `~=`).
    /// `Eq` returns `""`, `Exists` is unused (no `=` printed at all).
    pub fn symbol_prefix(&self) -> &'static str {
        match self {
            AttributeMatchOp::Exists => "",
            AttributeMatchOp::Eq => "",
            AttributeMatchOp::Includes => "~",
            AttributeMatchOp::DashMatch => "|",
            AttributeMatchOp::Prefix => "^",
            AttributeMatchOp::Suffix => "$",
            AttributeMatchOp::Substring => "*",
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

/// Selector for the `:nth-child()` CSS pseudo-class.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C, u8)]
pub enum CssNthChildSelector {
    Number(u32),
    Even,
    Odd,
    Pattern(CssNthChildPattern),
}

/// Pattern for `:nth-child(An+B)` selectors, where `pattern_repeat` is A and `offset` is B.
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
    /// Creates a new, empty CSS.
    pub fn empty() -> Self {
        Default::default()
    }

    /// Sort the rules by `(priority, specificity)` so they apply in cascade order.
    /// Lower-priority rules sort first; ties break by selector specificity.
    /// This preserves layer identity (UA / SYSTEM / AUTHOR / INLINE / RUNTIME)
    /// without needing a separate `Stylesheet` boundary.
    pub fn sort_by_specificity(&mut self) {
        self.rules.as_mut().sort_by(|a, b| {
            a.priority.cmp(&b.priority)
                .then_with(|| get_specificity(&a.path).cmp(&get_specificity(&b.path)))
        });
    }

    pub fn rules<'a>(&'a self) -> core::slice::Iter<'a, CssRuleBlock> {
        self.rules.as_ref().iter()
    }

    /// Iterate `(property, conditions)` pairs as if this were a flat list of
    /// `CssPropertyWithConditions`. Each `Static` declaration yields one item,
    /// sharing the conditions of its enclosing rule. `Dynamic` declarations
    /// are skipped (matching the previous inline-CSS behaviour).
    ///
    /// Used by cascade and diff code that walks per-property to keep the
    /// flat-iteration shape after the inline-vs-component unification.
    pub fn iter_inline_properties<'a>(
        &'a self,
    ) -> impl Iterator<
        Item = (
            &'a crate::props::property::CssProperty,
            &'a DynamicSelectorVec,
        ),
    > + 'a {
        self.rules.as_ref().iter().flat_map(|r| {
            r.declarations.as_ref().iter().filter_map(move |d| match d {
                CssDeclaration::Static(p) => Some((p, &r.conditions)),
                CssDeclaration::Dynamic(_) => None,
            })
        })
    }
}

#[cfg(test)]
mod root_scope_tests {
    use super::*;

    #[test]
    fn scope_range_contains() {
        let r = CssScopeRange { start: 3, end: 7 };
        assert!(r.contains(3) && r.contains(5) && r.contains(7));
        assert!(!r.contains(2) && !r.contains(8));
        // leaf: start == end matches only itself
        let leaf = CssScopeRange { start: 4, end: 4 };
        assert!(leaf.contains(4));
        assert!(!leaf.contains(3) && !leaf.contains(5));
    }

    #[test]
    fn push_front_scope_compounds_with_wrapper() {
        // a bare-decl `set_css` path is `[Global]` (the parse_inline `*` wrapper).
        let range = CssScopeRange { start: 5, end: 9 };
        let mut p = CssPath::new(vec![CssPathSelector::Global]);
        p.push_front_scope(range);
        assert_eq!(
            p.selectors.as_ref(),
            &[CssPathSelector::Root(range), CssPathSelector::Global][..]
        );
        // a nested selector path stays intact behind the scope prefix.
        let mut p2 = CssPath::new(vec![
            CssPathSelector::Global,
            CssPathSelector::Children,
            CssPathSelector::Class("foo".to_string().into()),
        ]);
        p2.push_front_scope(range);
        assert_eq!(p2.selectors.as_ref()[0], CssPathSelector::Root(range));
        assert_eq!(p2.selectors.as_ref().len(), 4);
    }

    #[test]
    fn root_display_roundtrips() {
        let s = CssPathSelector::Root(CssScopeRange { start: 2, end: 6 });
        assert_eq!(format!("{}", s), ":root(2..=6)");
    }

    #[test]
    fn parse_inline_keeps_layout_and_style_decls() {
        // set_css("width: 200px; height: 100px; background: red") must keep all
        // three declarations (layout + style) as Static props in the parsed rule.
        let css = Css::parse_inline("width: 200px; height: 100px; background: red");
        let mut types = Vec::new();
        for r in css.rules.as_ref() {
            for d in r.declarations.as_ref() {
                if let crate::css::CssDeclaration::Static(p) = d {
                    types.push(alloc::format!("{:?}", p.get_type()));
                }
            }
        }
        println!("INLINE PROP TYPES: {:?}", types);
        assert!(
            types.iter().any(|t| t.contains("width")),
            "width must survive parse_inline as a Static decl; got {:?}",
            types
        );
        assert!(
            types.iter().any(|t| t.contains("height")),
            "height must survive parse_inline; got {:?}",
            types
        );
    }
}

#[cfg(test)]
mod priority_sort_tests {
    use super::*;
    use crate::css::rule_priority;

    fn rule_with(priority: u8, selectors: Vec<CssPathSelector>) -> CssRuleBlock {
        CssRuleBlock {
            path: CssPath { selectors: selectors.into() },
            declarations: Vec::new().into(),
            conditions: DynamicSelectorVec::from_const_slice(&[]),
            priority,
        }
    }

    /// Pin the (priority, specificity) sort order. Lower priority sorts first;
    /// ties break by specificity.
    #[test]
    fn sort_by_priority_then_specificity() {
        let mut css = Css::new(vec![
            // Author rule, no specificity.
            rule_with(rule_priority::AUTHOR, vec![CssPathSelector::Global]),
            // UA rule with high specificity — must still come BEFORE any author rule.
            rule_with(rule_priority::UA, vec![
                CssPathSelector::Id("ua-id".to_string().into()),
                CssPathSelector::Class("ua-class".to_string().into()),
            ]),
            // Author rule with high specificity.
            rule_with(rule_priority::AUTHOR, vec![
                CssPathSelector::Id("a-id".to_string().into()),
            ]),
            // System rule with no specificity — must sit between UA and author.
            rule_with(rule_priority::SYSTEM, vec![CssPathSelector::Global]),
        ]);
        css.sort_by_specificity();
        let priorities: Vec<u8> = css.rules.as_ref().iter().map(|r| r.priority).collect();
        assert_eq!(
            priorities,
            vec![rule_priority::UA, rule_priority::SYSTEM, rule_priority::AUTHOR, rule_priority::AUTHOR],
            "rules must sort by layer first; specificity only breaks ties within a layer"
        );
        // Within author, the high-specificity #a-id comes after the * rule.
        let last_two_specificity: Vec<_> = css.rules.as_ref().iter()
            .filter(|r| r.priority == rule_priority::AUTHOR)
            .map(|r| get_specificity(&r.path))
            .collect();
        assert!(last_two_specificity[0] < last_two_specificity[1]);
    }
}

/// Returns specificity of the given css path. Further information can be found on
/// [the w3 website](http://www.w3.org/TR/selectors/#specificity).
pub fn get_specificity(path: &CssPath) -> (usize, usize, usize, usize) {
    let id_count = path
        .selectors
        .iter()
        .filter(|x| matches!(x, CssPathSelector::Id(_)))
        .count();
    let class_count = path
        .selectors
        .iter()
        .filter(|x| {
            matches!(
                x,
                CssPathSelector::Class(_)
                    | CssPathSelector::PseudoSelector(_)
                    | CssPathSelector::Attribute(_)
            )
        })
        .count();
    let div_count = path
        .selectors
        .iter()
        .filter(|x| matches!(x, CssPathSelector::Type(_)))
        .count();
    (id_count, class_count, div_count, path.selectors.len())
}
