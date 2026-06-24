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
#[derive(Debug, Default, PartialEq, Clone)]
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
    [Debug, Clone, PartialEq, Eq, PartialOrd]
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
    #[must_use] pub fn is_empty(&self) -> bool {
        self.rules.as_ref().is_empty()
    }

    #[must_use] pub fn new(rules: Vec<CssRuleBlock>) -> Self {
        Self {
            rules: rules.into(),
        }
    }

    #[cfg(feature = "parser")]
    // takes the owned C-ABI `AzString` by value by FFI ownership-transfer convention,
    // even though only a string slice is read here.
    #[allow(clippy::needless_pass_by_value)]
    #[must_use] pub fn from_string(s: AzString) -> Self {
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
    #[must_use] pub fn parse_inline(style: &str) -> Self {
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
    // takes the owned C-ABI `AzString` by value by FFI ownership-transfer convention,
    // even though only a string slice is read here.
    #[allow(clippy::needless_pass_by_value)]
    #[must_use] pub fn from_string_with_warnings(
        s: AzString,
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
// PartialOrd delegates to the length-based Ord so the two agree (the derived
// field-wise PartialOrd diverged from this manual Ord).
impl PartialOrd for Css {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
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
        // Build via an explicit push loop rather than `.into_iter().map(|p| CssRuleBlock {
        // declarations: vec![...], ... }).collect()`. On the web/remill lift, constructing a
        // complex struct with nested Vecs *inside* a mapped+collected closure drops every
        // element (AzButton's inline container style came back with 0 rules even though the
        // source Vec had props), whereas the identical construction in a plain loop body lifts
        // correctly — same pattern `NodeData::add_css_property` already relies on. Native
        // behavior is byte-identical.
        let owned = props.into_library_owned_vec();
        let mut rules: Vec<CssRuleBlock> = Vec::with_capacity(owned.len());
        for p in owned {
            rules.push(CssRuleBlock {
                path: CssPath { selectors: Vec::new().into() },
                declarations: alloc::vec![CssDeclaration::Static(p.property)].into(),
                conditions: p.apply_if,
                priority: rule_priority::INLINE,
            });
        }
        Self { rules: rules.into() }
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
    #[must_use] pub const fn new_static(prop: CssProperty) -> Self {
        Self::Static(prop)
    }

    #[must_use] pub const fn new_dynamic(prop: DynamicCssProperty) -> Self {
        Self::Dynamic(prop)
    }

    /// Returns the type of the property (i.e. the CSS key as a typed enum)
    #[must_use] pub const fn get_type(&self) -> CssPropertyType {
        use self::CssDeclaration::{Static, Dynamic};
        match self {
            Static(s) => s.get_type(),
            Dynamic(d) => d.default_value.get_type(),
        }
    }

    /// Determines if the property will be inherited (applied to the children)
    /// during the recursive application of the style on the DOM tree
    #[must_use] pub const fn is_inheritable(&self) -> bool {
        use self::CssDeclaration::{Static, Dynamic};
        match self {
            Static(s) => s.get_type().is_inheritable(),
            Dynamic(d) => d.is_inheritable(),
        }
    }

    /// Returns whether this rule affects only styling properties or layout
    /// properties (that could trigger a re-layout)
    #[must_use] pub const fn can_trigger_relayout(&self) -> bool {
        use self::CssDeclaration::{Static, Dynamic};
        match self {
            Static(s) => s.get_type().can_trigger_relayout(),
            Dynamic(d) => d.can_trigger_relayout(),
        }
    }

    #[must_use] pub fn to_str(&self) -> String {
        use self::CssDeclaration::{Static, Dynamic};
        match self {
            Static(s) => format!("{s:?}"),
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
/// Azul will register a dynamic property with the key "`my_dynamic_property_id`"
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
/// static reference.
///
/// Used to reduce enum size for large CSS property payloads
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
        Self::Boxed(Box::into_raw(Box::new(value)))
    }

    /// Return a reference to the inner value.
    ///
    /// # Safety invariant
    /// The inner pointer must be non-null. This is guaranteed by [`heap`](Self::heap)
    /// and the `Static` constructor (which should always point to valid data).
    #[inline]
    #[must_use] pub fn as_ref(&self) -> &T {
        match self {
            Self::Boxed(ptr) => unsafe {
                debug_assert!(!ptr.is_null(), "BoxOrStatic::Boxed contained a null pointer");
                &**ptr
            },
            Self::Static(ptr) => unsafe {
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
            Self::Boxed(ptr) => unsafe { &mut **ptr },
            Self::Static(_) => panic!("Cannot mutate a static BoxOrStatic value"),
        }
    }

    /// Consume self and return the inner value.
    #[inline]
    #[must_use] pub fn into_inner(self) -> T where T: Clone {
        let val = self.as_ref().clone();
        // Don't double-free: std::mem::forget prevents Drop from running
        core::mem::forget(self);
        val
    }
}

impl<T> Drop for BoxOrStatic<T> {
    fn drop(&mut self) {
        if let Self::Boxed(ptr) = self {
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
            Self::Boxed(ptr) => {
                let val = unsafe { &**ptr }.clone();
                Self::Boxed(Box::into_raw(Box::new(val)))
            }
            Self::Static(ptr) => Self::Static(*ptr),
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for BoxOrStatic<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.as_ref().fmt(f)
    }
}

impl<T: fmt::Display> fmt::Display for BoxOrStatic<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
        self.as_ref().hash(state);
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
        Self::heap(T::default())
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

/// Type alias: `BoxOrStatic<AzString>` — used by `NodeType::Text` and `NodeType::Icon`.
pub type BoxOrStaticString = BoxOrStatic<AzString>;

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
            Self::Auto => "auto".to_string(),
            Self::None => "none".to_string(),
            Self::Initial => "initial".to_string(),
            Self::Inherit => "inherit".to_string(),
            Self::Revert => "revert".to_string(),
            Self::Unset => "unset".to_string(),
            Self::Exact(e) => e.print_as_css_value(),
        }
    }
}

impl<T: fmt::Display> fmt::Display for CssPropertyValue<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use self::CssPropertyValue::{Auto, None, Initial, Inherit, Revert, Unset, Exact};
        match self {
            Auto => write!(f, "auto"),
            None => write!(f, "none"),
            Initial => write!(f, "initial"),
            Inherit => write!(f, "inherit"),
            Revert => write!(f, "revert"),
            Unset => write!(f, "unset"),
            Exact(e) => write!(f, "{e}"),
        }
    }
}

impl<T> From<T> for CssPropertyValue<T> {
    fn from(c: T) -> Self {
        Self::Exact(c)
    }
}

impl<T> CssPropertyValue<T> {
    /// Transforms a `CssPropertyValue<T>` into a `CssPropertyValue<U>` by applying a mapping
    /// function
    #[inline]
    pub fn map_property<F: Fn(T) -> U, U>(self, map_fn: F) -> CssPropertyValue<U> {
        match self {
            Self::Exact(c) => CssPropertyValue::Exact(map_fn(c)),
            Self::Auto => CssPropertyValue::Auto,
            Self::None => CssPropertyValue::None,
            Self::Initial => CssPropertyValue::Initial,
            Self::Inherit => CssPropertyValue::Inherit,
            Self::Revert => CssPropertyValue::Revert,
            Self::Unset => CssPropertyValue::Unset,
        }
    }

    #[inline]
    pub const fn get_property(&self) -> Option<&T> {
        match self {
            Self::Exact(c) => Some(c),
            _ => None,
        }
    }

    #[inline]
    pub fn get_property_owned(self) -> Option<T> {
        match self {
            Self::Exact(c) => Some(c),
            _ => None,
        }
    }

    #[inline]
    pub const fn is_auto(&self) -> bool {
        matches!(self, Self::Auto)
    }

    #[inline]
    pub const fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    #[inline]
    pub const fn is_initial(&self) -> bool {
        matches!(self, Self::Initial)
    }

    #[inline]
    pub const fn is_inherit(&self) -> bool {
        matches!(self, Self::Inherit)
    }

    #[inline]
    pub const fn is_revert(&self) -> bool {
        matches!(self, Self::Revert)
    }

    #[inline]
    pub const fn is_unset(&self) -> bool {
        matches!(self, Self::Unset)
    }
}

impl<T: Default> CssPropertyValue<T> {
    #[inline]
    pub fn get_property_or_default(self) -> Option<T> {
        match self {
            Self::Auto | Self::Initial => Some(T::default()),
            Self::Exact(c) => Some(c),
            Self::None
            | Self::Inherit
            | Self::Revert
            | Self::Unset => None,
        }
    }
}

impl<T: Default> Default for CssPropertyValue<T> {
    #[inline]
    fn default() -> Self {
        Self::Exact(T::default())
    }
}

impl DynamicCssProperty {
    #[must_use] pub const fn is_inheritable(&self) -> bool {
        // Dynamic style properties should not be inheritable,
        // since that could lead to bugs - you set a property in Rust, suddenly
        // the wrong UI component starts to react because it was inherited.
        false
    }

    #[must_use] pub const fn can_trigger_relayout(&self) -> bool {
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

    /// Reserved for direct-rule runtime overrides.
    ///
    /// Today the
    /// `prop_cache` handles runtime overrides via
    /// `user_overridden_properties`; this slot is reserved so a
    /// future "push a `CssRuleBlock` at runtime" path stays above
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
    [Debug, Clone, PartialEq, Eq, PartialOrd]
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
    #[must_use] pub fn new(path: CssPath, declarations: Vec<CssDeclaration>) -> Self {
        Self {
            path,
            declarations: declarations.into(),
            conditions: DynamicSelectorVec::from_const_slice(&[]),
            priority: rule_priority::AUTHOR,
        }
    }

    #[must_use] pub fn with_conditions(
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
    /// Icon element - resolved to actual content by `IconProvider`
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

impl fmt::Display for NodeTypeTagParseError<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self {
            NodeTypeTagParseError::Invalid(e) => write!(f, "Invalid node type: {e}"),
        }
    }
}

/// Owned version of [`NodeTypeTagParseError`] for storage across lifetime boundaries.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum NodeTypeTagParseErrorOwned {
    Invalid(AzString),
}

impl NodeTypeTagParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> NodeTypeTagParseErrorOwned {
        match self {
            NodeTypeTagParseError::Invalid(s) => NodeTypeTagParseErrorOwned::Invalid((*s).to_string().into()),
        }
    }
}

impl NodeTypeTagParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> NodeTypeTagParseError<'_> {
        match self {
            Self::Invalid(s) => NodeTypeTagParseError::Invalid(s),
        }
    }
}

/// Parses the node type from a CSS string such as `"div"` => `NodeTypeTag::Div`
impl NodeTypeTag {
    pub fn from_str(css_key: &str) -> Result<Self, NodeTypeTagParseError<'_>> {
        match css_key {
            // Document structure
            "html" => Ok(Self::Html),
            "head" => Ok(Self::Head),
            "body" => Ok(Self::Body),

            // Block-level elements
            "div" => Ok(Self::Div),
            "p" => Ok(Self::P),
            "article" => Ok(Self::Article),
            "section" => Ok(Self::Section),
            "nav" => Ok(Self::Nav),
            "aside" => Ok(Self::Aside),
            "header" => Ok(Self::Header),
            "footer" => Ok(Self::Footer),
            "main" => Ok(Self::Main),
            "figure" => Ok(Self::Figure),
            "figcaption" => Ok(Self::FigCaption),

            // Headings
            "h1" => Ok(Self::H1),
            "h2" => Ok(Self::H2),
            "h3" => Ok(Self::H3),
            "h4" => Ok(Self::H4),
            "h5" => Ok(Self::H5),
            "h6" => Ok(Self::H6),

            // Inline text
            "br" => Ok(Self::Br),
            "hr" => Ok(Self::Hr),
            "pre" => Ok(Self::Pre),
            "blockquote" => Ok(Self::BlockQuote),
            "address" => Ok(Self::Address),
            "details" => Ok(Self::Details),
            "summary" => Ok(Self::Summary),
            "dialog" => Ok(Self::Dialog),

            // Lists
            "ul" => Ok(Self::Ul),
            "ol" => Ok(Self::Ol),
            "li" => Ok(Self::Li),
            "dl" => Ok(Self::Dl),
            "dt" => Ok(Self::Dt),
            "dd" => Ok(Self::Dd),
            "menu" => Ok(Self::Menu),
            "menuitem" => Ok(Self::MenuItem),
            "dir" => Ok(Self::Dir),

            // Tables
            "table" => Ok(Self::Table),
            "caption" => Ok(Self::Caption),
            "thead" => Ok(Self::THead),
            "tbody" => Ok(Self::TBody),
            "tfoot" => Ok(Self::TFoot),
            "tr" => Ok(Self::Tr),
            "th" => Ok(Self::Th),
            "td" => Ok(Self::Td),
            "colgroup" => Ok(Self::ColGroup),
            "col" => Ok(Self::Col),

            // Forms
            "form" => Ok(Self::Form),
            "fieldset" => Ok(Self::FieldSet),
            "legend" => Ok(Self::Legend),
            "label" => Ok(Self::Label),
            "input" => Ok(Self::Input),
            "button" => Ok(Self::Button),
            "select" => Ok(Self::Select),
            "optgroup" => Ok(Self::OptGroup),
            "option" => Ok(Self::SelectOption),
            "textarea" => Ok(Self::TextArea),
            "output" => Ok(Self::Output),
            "progress" => Ok(Self::Progress),
            "meter" => Ok(Self::Meter),
            "datalist" => Ok(Self::DataList),

            // Inline elements
            "span" => Ok(Self::Span),
            "a" => Ok(Self::A),
            "em" => Ok(Self::Em),
            "strong" => Ok(Self::Strong),
            "b" => Ok(Self::B),
            "i" => Ok(Self::I),
            "u" => Ok(Self::U),
            "s" => Ok(Self::S),
            "mark" => Ok(Self::Mark),
            "del" => Ok(Self::Del),
            "ins" => Ok(Self::Ins),
            "code" => Ok(Self::Code),
            "samp" => Ok(Self::Samp),
            "kbd" => Ok(Self::Kbd),
            "var" => Ok(Self::Var),
            "cite" => Ok(Self::Cite),
            "dfn" => Ok(Self::Dfn),
            "abbr" => Ok(Self::Abbr),
            "acronym" => Ok(Self::Acronym),
            "q" => Ok(Self::Q),
            "time" => Ok(Self::Time),
            "sub" => Ok(Self::Sub),
            "sup" => Ok(Self::Sup),
            "small" => Ok(Self::Small),
            "big" => Ok(Self::Big),
            "bdo" => Ok(Self::Bdo),
            "bdi" => Ok(Self::Bdi),
            "wbr" => Ok(Self::Wbr),
            "ruby" => Ok(Self::Ruby),
            "rt" => Ok(Self::Rt),
            "rtc" => Ok(Self::Rtc),
            "rp" => Ok(Self::Rp),
            "data" => Ok(Self::Data),

            // Embedded content
            "canvas" => Ok(Self::Canvas),
            "object" => Ok(Self::Object),
            "param" => Ok(Self::Param),
            "embed" => Ok(Self::Embed),
            "audio" => Ok(Self::Audio),
            "video" => Ok(Self::Video),
            "source" => Ok(Self::Source),
            "track" => Ok(Self::Track),
            "map" => Ok(Self::Map),
            "area" => Ok(Self::Area),
            "svg" => Ok(Self::Svg),

            // SVG shape elements
            "path" => Ok(Self::SvgPath),
            "circle" => Ok(Self::SvgCircle),
            "rect" => Ok(Self::SvgRect),
            "ellipse" => Ok(Self::SvgEllipse),
            "line" => Ok(Self::SvgLine),
            "polygon" => Ok(Self::SvgPolygon),
            "polyline" => Ok(Self::SvgPolyline),
            "g" => Ok(Self::SvgG),

            // SVG container elements
            "defs" => Ok(Self::SvgDefs),
            "symbol" => Ok(Self::SvgSymbol),
            "use" => Ok(Self::SvgUse),
            "switch" => Ok(Self::SvgSwitch),

            // SVG text elements
            "svg:text" => Ok(Self::SvgText),
            "tspan" => Ok(Self::SvgTspan),
            "textpath" => Ok(Self::SvgTextPath),

            // SVG paint server elements
            "lineargradient" => Ok(Self::SvgLinearGradient),
            "radialgradient" => Ok(Self::SvgRadialGradient),
            "stop" => Ok(Self::SvgStop),
            "pattern" => Ok(Self::SvgPattern),

            // SVG clipping/masking elements
            "clippath" => Ok(Self::SvgClipPathElement),
            "mask" => Ok(Self::SvgMask),

            // SVG filter elements
            "filter" => Ok(Self::SvgFilter),
            "feblend" => Ok(Self::SvgFeBlend),
            "fecolormatrix" => Ok(Self::SvgFeColorMatrix),
            "fecomponenttransfer" => Ok(Self::SvgFeComponentTransfer),
            "fecomposite" => Ok(Self::SvgFeComposite),
            "feconvolvematrix" => Ok(Self::SvgFeConvolveMatrix),
            "fediffuselighting" => Ok(Self::SvgFeDiffuseLighting),
            "fedisplacementmap" => Ok(Self::SvgFeDisplacementMap),
            "fedistantlight" => Ok(Self::SvgFeDistantLight),
            "fedropshadow" => Ok(Self::SvgFeDropShadow),
            "feflood" => Ok(Self::SvgFeFlood),
            "fefuncr" => Ok(Self::SvgFeFuncR),
            "fefuncg" => Ok(Self::SvgFeFuncG),
            "fefuncb" => Ok(Self::SvgFeFuncB),
            "fefunca" => Ok(Self::SvgFeFuncA),
            "fegaussianblur" => Ok(Self::SvgFeGaussianBlur),
            "feimage" => Ok(Self::SvgFeImage),
            "femerge" => Ok(Self::SvgFeMerge),
            "femergenode" => Ok(Self::SvgFeMergeNode),
            "femorphology" => Ok(Self::SvgFeMorphology),
            "feoffset" => Ok(Self::SvgFeOffset),
            "fepointlight" => Ok(Self::SvgFePointLight),
            "fespecularlighting" => Ok(Self::SvgFeSpecularLighting),
            "fespotlight" => Ok(Self::SvgFeSpotLight),
            "fetile" => Ok(Self::SvgFeTile),
            "feturbulence" => Ok(Self::SvgFeTurbulence),

            // SVG marker/image elements
            "image" | "svg:image" => Ok(Self::SvgImage),
            "svg:marker" => Ok(Self::SvgMarker),
            "foreignobject" => Ok(Self::SvgForeignObject),

            // SVG descriptive elements
            "svg:title" => Ok(Self::SvgTitle),
            "svg:a" => Ok(Self::SvgA),
            "svg:style" => Ok(Self::SvgStyle),
            "svg:script" => Ok(Self::SvgScript),
            "desc" => Ok(Self::SvgDesc),
            "metadata" => Ok(Self::SvgMetadata),
            "view" => Ok(Self::SvgView),

            // SVG animation elements
            "animate" => Ok(Self::SvgAnimate),
            "animatemotion" => Ok(Self::SvgAnimateMotion),
            "animatetransform" => Ok(Self::SvgAnimateTransform),
            "set" => Ok(Self::SvgSet),
            "mpath" => Ok(Self::SvgMpath),

            // Metadata
            "title" => Ok(Self::Title),
            "meta" => Ok(Self::Meta),
            "link" => Ok(Self::Link),
            "script" => Ok(Self::Script),
            "style" => Ok(Self::Style),
            "base" => Ok(Self::Base),

            // Special
            "img" => Ok(Self::Img),
            "virtual-view" | "iframe" => Ok(Self::VirtualView),
            "icon" => Ok(Self::Icon),
            "geolocation-probe" => Ok(Self::GeolocationProbe),

            // Pseudo-elements (usually prefixed with ::)
            "before" | "::before" => Ok(Self::Before),
            "after" | "::after" => Ok(Self::After),
            "marker" | "::marker" => Ok(Self::Marker),
            "placeholder" | "::placeholder" => Ok(Self::Placeholder),

            other => Err(NodeTypeTagParseError::Invalid(other)),
        }
    }
}

impl fmt::Display for NodeTypeTag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // Document structure
            Self::Html => write!(f, "html"),
            Self::Head => write!(f, "head"),
            Self::Body => write!(f, "body"),

            // Block elements
            Self::Div => write!(f, "div"),
            Self::P => write!(f, "p"),
            Self::Article => write!(f, "article"),
            Self::Section => write!(f, "section"),
            Self::Nav => write!(f, "nav"),
            Self::Aside => write!(f, "aside"),
            Self::Header => write!(f, "header"),
            Self::Footer => write!(f, "footer"),
            Self::Main => write!(f, "main"),
            Self::Figure => write!(f, "figure"),
            Self::FigCaption => write!(f, "figcaption"),

            // Headings
            Self::H1 => write!(f, "h1"),
            Self::H2 => write!(f, "h2"),
            Self::H3 => write!(f, "h3"),
            Self::H4 => write!(f, "h4"),
            Self::H5 => write!(f, "h5"),
            Self::H6 => write!(f, "h6"),

            // Text formatting
            Self::Br => write!(f, "br"),
            Self::Hr => write!(f, "hr"),
            Self::Pre => write!(f, "pre"),
            Self::BlockQuote => write!(f, "blockquote"),
            Self::Address => write!(f, "address"),
            Self::Details => write!(f, "details"),
            Self::Summary => write!(f, "summary"),
            Self::Dialog => write!(f, "dialog"),

            // List elements
            Self::Ul => write!(f, "ul"),
            Self::Ol => write!(f, "ol"),
            Self::Li => write!(f, "li"),
            Self::Dl => write!(f, "dl"),
            Self::Dt => write!(f, "dt"),
            Self::Dd => write!(f, "dd"),
            Self::Menu => write!(f, "menu"),
            Self::MenuItem => write!(f, "menuitem"),
            Self::Dir => write!(f, "dir"),

            // Table elements
            Self::Table => write!(f, "table"),
            Self::Caption => write!(f, "caption"),
            Self::THead => write!(f, "thead"),
            Self::TBody => write!(f, "tbody"),
            Self::TFoot => write!(f, "tfoot"),
            Self::Tr => write!(f, "tr"),
            Self::Th => write!(f, "th"),
            Self::Td => write!(f, "td"),
            Self::ColGroup => write!(f, "colgroup"),
            Self::Col => write!(f, "col"),

            // Form elements
            Self::Form => write!(f, "form"),
            Self::FieldSet => write!(f, "fieldset"),
            Self::Legend => write!(f, "legend"),
            Self::Label => write!(f, "label"),
            Self::Input => write!(f, "input"),
            Self::Button => write!(f, "button"),
            Self::Select => write!(f, "select"),
            Self::OptGroup => write!(f, "optgroup"),
            Self::SelectOption => write!(f, "option"),
            Self::TextArea => write!(f, "textarea"),
            Self::Output => write!(f, "output"),
            Self::Progress => write!(f, "progress"),
            Self::Meter => write!(f, "meter"),
            Self::DataList => write!(f, "datalist"),

            // Inline elements
            Self::Span => write!(f, "span"),
            Self::A => write!(f, "a"),
            Self::Em => write!(f, "em"),
            Self::Strong => write!(f, "strong"),
            Self::B => write!(f, "b"),
            Self::I => write!(f, "i"),
            Self::U => write!(f, "u"),
            Self::S => write!(f, "s"),
            Self::Mark => write!(f, "mark"),
            Self::Del => write!(f, "del"),
            Self::Ins => write!(f, "ins"),
            Self::Code => write!(f, "code"),
            Self::Samp => write!(f, "samp"),
            Self::Kbd => write!(f, "kbd"),
            Self::Var => write!(f, "var"),
            Self::Cite => write!(f, "cite"),
            Self::Dfn => write!(f, "dfn"),
            Self::Abbr => write!(f, "abbr"),
            Self::Acronym => write!(f, "acronym"),
            Self::Q => write!(f, "q"),
            Self::Time => write!(f, "time"),
            Self::Sub => write!(f, "sub"),
            Self::Sup => write!(f, "sup"),
            Self::Small => write!(f, "small"),
            Self::Big => write!(f, "big"),
            Self::Bdo => write!(f, "bdo"),
            Self::Bdi => write!(f, "bdi"),
            Self::Wbr => write!(f, "wbr"),
            Self::Ruby => write!(f, "ruby"),
            Self::Rt => write!(f, "rt"),
            Self::Rtc => write!(f, "rtc"),
            Self::Rp => write!(f, "rp"),
            Self::Data => write!(f, "data"),

            // Embedded content
            Self::Canvas => write!(f, "canvas"),
            Self::Object => write!(f, "object"),
            Self::Param => write!(f, "param"),
            Self::Embed => write!(f, "embed"),
            Self::Audio => write!(f, "audio"),
            Self::Video => write!(f, "video"),
            Self::Source => write!(f, "source"),
            Self::Track => write!(f, "track"),
            Self::Map => write!(f, "map"),
            Self::Area => write!(f, "area"),
            Self::Svg => write!(f, "svg"),
            Self::SvgPath => write!(f, "path"),
            Self::SvgCircle => write!(f, "circle"),
            Self::SvgRect => write!(f, "rect"),
            Self::SvgEllipse => write!(f, "ellipse"),
            Self::SvgLine => write!(f, "line"),
            Self::SvgPolygon => write!(f, "polygon"),
            Self::SvgPolyline => write!(f, "polyline"),
            Self::SvgG => write!(f, "g"),

            // SVG container elements
            Self::SvgDefs => write!(f, "defs"),
            Self::SvgSymbol => write!(f, "symbol"),
            Self::SvgUse => write!(f, "use"),
            Self::SvgSwitch => write!(f, "switch"),

            // SVG text elements
            Self::SvgText => write!(f, "svg:text"),
            Self::SvgTspan => write!(f, "tspan"),
            Self::SvgTextPath => write!(f, "textpath"),

            // SVG paint server elements
            Self::SvgLinearGradient => write!(f, "lineargradient"),
            Self::SvgRadialGradient => write!(f, "radialgradient"),
            Self::SvgStop => write!(f, "stop"),
            Self::SvgPattern => write!(f, "pattern"),

            // SVG clipping/masking elements
            Self::SvgClipPathElement => write!(f, "clippath"),
            Self::SvgMask => write!(f, "mask"),

            // SVG filter elements
            Self::SvgFilter => write!(f, "filter"),
            Self::SvgFeBlend => write!(f, "feblend"),
            Self::SvgFeColorMatrix => write!(f, "fecolormatrix"),
            Self::SvgFeComponentTransfer => write!(f, "fecomponenttransfer"),
            Self::SvgFeComposite => write!(f, "fecomposite"),
            Self::SvgFeConvolveMatrix => write!(f, "feconvolvematrix"),
            Self::SvgFeDiffuseLighting => write!(f, "fediffuselighting"),
            Self::SvgFeDisplacementMap => write!(f, "fedisplacementmap"),
            Self::SvgFeDistantLight => write!(f, "fedistantlight"),
            Self::SvgFeDropShadow => write!(f, "fedropshadow"),
            Self::SvgFeFlood => write!(f, "feflood"),
            Self::SvgFeFuncR => write!(f, "fefuncr"),
            Self::SvgFeFuncG => write!(f, "fefuncg"),
            Self::SvgFeFuncB => write!(f, "fefuncb"),
            Self::SvgFeFuncA => write!(f, "fefunca"),
            Self::SvgFeGaussianBlur => write!(f, "fegaussianblur"),
            Self::SvgFeImage => write!(f, "feimage"),
            Self::SvgFeMerge => write!(f, "femerge"),
            Self::SvgFeMergeNode => write!(f, "femergenode"),
            Self::SvgFeMorphology => write!(f, "femorphology"),
            Self::SvgFeOffset => write!(f, "feoffset"),
            Self::SvgFePointLight => write!(f, "fepointlight"),
            Self::SvgFeSpecularLighting => write!(f, "fespecularlighting"),
            Self::SvgFeSpotLight => write!(f, "fespotlight"),
            Self::SvgFeTile => write!(f, "fetile"),
            Self::SvgFeTurbulence => write!(f, "feturbulence"),

            // SVG marker/image elements
            Self::SvgMarker => write!(f, "svg:marker"),
            Self::SvgImage => write!(f, "svg:image"),
            Self::SvgForeignObject => write!(f, "foreignobject"),

            // SVG descriptive elements
            Self::SvgTitle => write!(f, "svg:title"),
            Self::SvgDesc => write!(f, "desc"),
            Self::SvgMetadata => write!(f, "metadata"),
            Self::SvgA => write!(f, "svg:a"),
            Self::SvgView => write!(f, "view"),
            Self::SvgStyle => write!(f, "svg:style"),
            Self::SvgScript => write!(f, "svg:script"),

            // SVG animation elements
            Self::SvgAnimate => write!(f, "animate"),
            Self::SvgAnimateMotion => write!(f, "animatemotion"),
            Self::SvgAnimateTransform => write!(f, "animatetransform"),
            Self::SvgSet => write!(f, "set"),
            Self::SvgMpath => write!(f, "mpath"),

            // Metadata
            Self::Title => write!(f, "title"),
            Self::Meta => write!(f, "meta"),
            Self::Link => write!(f, "link"),
            Self::Script => write!(f, "script"),
            Self::Style => write!(f, "style"),
            Self::Base => write!(f, "base"),

            // Content elements
            Self::Text => write!(f, "text"),
            Self::Img => write!(f, "img"),
            Self::VirtualView => write!(f, "virtual-view"),
            Self::Icon => write!(f, "icon"),
            Self::GeolocationProbe => write!(f, "geolocation-probe"),

            // Pseudo-elements
            Self::Before => write!(f, "::before"),
            Self::After => write!(f, "::after"),
            Self::Marker => write!(f, "::marker"),
            Self::Placeholder => write!(f, "::placeholder"),
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
    #[must_use] pub fn new(selectors: Vec<CssPathSelector>) -> Self {
        Self {
            selectors: selectors.into(),
        }
    }

    /// Prepend a `Root` scope selector (`push_front`) confining this rule to the owner
    /// node `start` (whose subtree spans the inclusive flat ids `[start, end]`).
    /// Two cases (#47 leak fix + descendant-selector support):
    ///
    /// - A **bare `*` rule** (the `parse_inline` wrapper for a `with_css`/`set_css`
    ///   bare-declaration string) is scoped **node-only** (`[start, start]`):
    ///   inline-style semantics — it applies to the OWNER only and must not leak to
    ///   descendants or siblings. `[Root([s,s]), Global]` matches `s` only.
    /// - A rule with a **real selector** (`.menu-item`, `div`, a descendant chain —
    ///   from `add_component_css` / a component stylesheet) is scoped to the whole
    ///   **subtree** (`[start, end]`), so its selectors match within the owner's
    ///   subtree (e.g. a menu container's `.menu-item` children). `[Root([s,e]),
    ///   Class(x)]` matches any node in `[s,e]` that also matches `.x`.
    pub fn push_front_scope(&mut self, start: usize, end: usize) {
        let is_bare_global = self.selectors.as_ref().len() == 1
            && matches!(self.selectors.as_ref().first(), Some(CssPathSelector::Global));
        let range = if is_bare_global {
            CssScopeRange { start, end: start }
        } else {
            CssScopeRange { start, end }
        };
        let mut selectors = Vec::with_capacity(self.selectors.as_ref().len() + 1);
        selectors.push(CssPathSelector::Root(range));
        selectors.extend(self.selectors.as_ref().iter().cloned());
        self.selectors = selectors.into();
    }
}

impl fmt::Display for CssPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for selector in self.selectors.as_ref() {
            write!(f, "{selector}")?;
        }
        Ok(())
    }
}

impl fmt::Debug for CssPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self}")
    }
}

/// Inclusive range of flat `NodeId`s describing a node's subtree `[start, end]`
/// (`end = start + estimated_total_children`, since the flat arena lays subtrees
/// out contiguously).
///
/// Carried by [`CssPathSelector::Root`] to scope inline css to
/// a subtree, and is the unit of future parallel per-subtree cascading.
/// `repr(C)` for FFI / api.json codegen.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(C)]
pub struct CssScopeRange {
    /// First flat `NodeId` of the subtree (the owning node itself).
    pub start: usize,
    /// Last flat `NodeId` of the subtree, inclusive (`start` for a leaf).
    pub end: usize,
}

impl CssScopeRange {
    /// True if `node` (a flat `NodeId` index) is inside this subtree range.
    #[inline]
    #[must_use] pub const fn contains(&self, node: usize) -> bool {
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
#[derive(Default)]
pub enum AttributeMatchOp {
    /// `[attr]` — attribute is present (any value).
    #[default]
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


impl_option!(
    CssPathSelector,
    OptionCssPathSelector,
    copy = false,
    [Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);


impl fmt::Display for CssPathSelector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use self::CssPathSelector::{Global, Root, Type, Class, Id, PseudoSelector, Attribute, DirectChildren, Children, AdjacentSibling, GeneralSibling};
        match &self {
            Global => write!(f, "*"),
            Root(r) => write!(f, ":root({}..={})", r.start, r.end),
            Type(n) => write!(f, "{n}"),
            Class(c) => write!(f, ".{c}"),
            Id(i) => write!(f, "#{i}"),
            PseudoSelector(p) => write!(f, ":{p}"),
            Attribute(a) => write!(f, "{a}"),
            DirectChildren => write!(f, ">"),
            Children => write!(f, " "),
            AdjacentSibling => write!(f, "+"),
            GeneralSibling => write!(f, "~"),
        }
    }
}

impl fmt::Display for CssAttributeSelector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
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
    #[must_use] pub const fn symbol_prefix(&self) -> &'static str {
        match self {
            Self::Exists | Self::Eq => "",
            Self::Includes => "~",
            Self::DashMatch => "|",
            Self::Prefix => "^",
            Self::Suffix => "$",
            Self::Substring => "*",
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
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use self::CssNthChildSelector::{Number, Even, Odd, Pattern};
        match &self {
            Number(u) => write!(f, "{u}"),
            Even => write!(f, "even"),
            Odd => write!(f, "odd"),
            Pattern(p) => write!(f, "{}n + {}", p.pattern_repeat, p.offset),
        }
    }
}

impl fmt::Display for CssPathPseudoSelector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use self::CssPathPseudoSelector::{First, Last, NthChild, Hover, Active, Focus, Lang, Backdrop, Dragging, DragOver};
        match &self {
            First => write!(f, "first"),
            Last => write!(f, "last"),
            NthChild(u) => write!(f, "nth-child({u})"),
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
    #[must_use] pub fn empty() -> Self {
        Self::default()
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

    pub fn rules(&self) -> core::slice::Iter<'_, CssRuleBlock> {
        self.rules.as_ref().iter()
    }

    /// Iterate `(property, conditions)` pairs as if this were a flat list of
    /// `CssPropertyWithConditions`. Each `Static` declaration yields one item,
    /// sharing the conditions of its enclosing rule. `Dynamic` declarations
    /// are skipped (matching the previous inline-CSS behaviour).
    ///
    /// Used by cascade and diff code that walks per-property to keep the
    /// flat-iteration shape after the inline-vs-component unification.
    pub fn iter_inline_properties(
        &self,
    ) -> impl Iterator<
        Item = (
            &CssProperty,
            &DynamicSelectorVec,
        ),
    > + '_ {
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
        // a bare-decl `set_css` path is `[Global]` (the parse_inline `*` wrapper) and
        // is scoped NODE-ONLY ([start, start]) so it applies to the owner only.
        let mut p = CssPath::new(vec![CssPathSelector::Global]);
        p.push_front_scope(5, 9);
        assert_eq!(
            p.selectors.as_ref(),
            &[
                CssPathSelector::Root(CssScopeRange { start: 5, end: 5 }),
                CssPathSelector::Global
            ][..]
        );
        // a path with a real selector is SUBTREE-scoped ([start, end]).
        let subtree = CssScopeRange { start: 5, end: 9 };
        let mut p2 = CssPath::new(vec![
            CssPathSelector::Global,
            CssPathSelector::Children,
            CssPathSelector::Class("foo".to_string().into()),
        ]);
        p2.push_front_scope(5, 9);
        assert_eq!(p2.selectors.as_ref()[0], CssPathSelector::Root(subtree));
        assert_eq!(p2.selectors.as_ref().len(), 4);
    }

    #[test]
    fn root_display_roundtrips() {
        let s = CssPathSelector::Root(CssScopeRange { start: 2, end: 6 });
        assert_eq!(format!("{s}"), ":root(2..=6)");
    }

    #[test]
    fn parse_inline_keeps_layout_and_style_decls() {
        // set_css("width: 200px; height: 100px; background: red") must keep all
        // three declarations (layout + style) as Static props in the parsed rule.
        let css = Css::parse_inline("width: 200px; height: 100px; background: red");
        let mut types = Vec::new();
        for r in css.rules.as_ref() {
            for d in r.declarations.as_ref() {
                if let CssDeclaration::Static(p) = d {
                    types.push(alloc::format!("{:?}", p.get_type()));
                }
            }
        }
        println!("INLINE PROP TYPES: {types:?}");
        assert!(
            types.iter().any(|t| t.contains("width")),
            "width must survive parse_inline as a Static decl; got {types:?}"
        );
        assert!(
            types.iter().any(|t| t.contains("height")),
            "height must survive parse_inline; got {types:?}"
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
#[must_use] pub fn get_specificity(path: &CssPath) -> (usize, usize, usize, usize) {
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
