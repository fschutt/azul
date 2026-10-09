//! Generic icon provider system for Azul
//!
//! This module defines a generic, callback-based icon resolution infrastructure.
//! The actual parsing/loading implementations live in `azul-layout`.
//!
//! # Architecture
//!
//! The icon system is fully generic using RefAny:
//!
//! 1. `IconProviderHandle` - stores icons in nested map: pack_name → (icon_name → RefAny)
//! 2. The resolver callback turns (icon_data, original_dom) into a StyledDom
//! 3. Differentiation between Image/Font/SVG/etc. is via RefAny::downcast
//! 4. Supports any icon source: images, fonts, SVGs, animated icons, etc.
//!
//! # Resolution Flow
//!
//! 1. User creates Icon nodes: `Dom::create_icon("home")`
//! 2. Before layout, `resolve_icons_in_styled_dom()` is called
//! 3. Each Icon node is looked up across all packs (first match wins)
//! 4. The resolver callback is invoked with the found RefAny data + original DOM
//! 5. The callback returns a StyledDom subtree that replaces the icon node
//!
//! # Caching
//!
//! Resolution results are CACHED on the [`SharedIconProvider`], keyed by
//! (icon spec, the original icon node's full `NodeData`, its `StyledNode`),
//! and flushed when the `SystemStyle` changes. The engine calls
//! `resolve_icons_in_styled_dom` on EVERY DOM regeneration — during a Wayland
//! drag-resize that is one call per pixel of mouse movement (373 in a measured
//! 5-second drag), and each un-cached resolution runs `StyledDom::create`'s
//! full single-node cascade whose output is then thrown away by the host's
//! own cascade recompute. ~66 ribbon icons × 373 regenerations ≈ 24 600
//! throwaway cascades per drag, all yielding bit-identical results
//!.
//!
//! The cache stores the resolver's output DECONSTRUCTED into exactly the
//! fields the replacement consumes (node type, inline style, accessibility,
//! styled node), so a hit is four field clones — no `Dom`, no `StyledDom`,
//! no cascade, no `CssPropertyCache`, not even the single-node extraction of
//! the original.
//!
//! Correctness notes:
//! - The KEY includes the whole original `NodeData` + `StyledNode`, because a custom resolver may
//!   read anything from `original_icon_dom` (the default one copies inline styles and accessibility
//!   info). Same name with different inline styles → separate entries; a hover-state flip on the
//!   node → different `StyledNode` → re-resolve.
//! - The icon SET and the resolver are frozen once the provider is shared (`App::run` consumes the
//!   handle; `SharedIconProvider` exposes no registration), so registration invalidation cannot be
//!   needed post-share.
//! - "Animated icons" remain compatible: animation is carried by the DATA the resolver returns
//!   (e.g. an image-callback node that animates per frame), not by re-resolving per frame —
//!   re-resolution only ever happened on DOM regeneration anyway.
//!
//! # Custom Resolvers
//!
//! Users can provide custom C callbacks for complete control:
//!
//! ```c
//! AzStyledDom my_resolver(
//!     AzRefAny* icon_data,           // NULL if icon not found
//!     AzStyledDom* original_icon_dom, // Contains icon_name, styles, a11y
//!     AzSystemStyle* system_style
//! ) {
//!     // Custom resolution logic - icon_data contains your registered data
//!     return create_my_icon_dom(...);
//! }
//! ```

use alloc::{
    boxed::Box,
    collections::BTreeMap,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use core::{fmt, mem::ManuallyDrop};
#[cfg(feature = "std")]
use std::sync::Mutex;

#[cfg(not(feature = "std"))]
use self::nostd_lock::Mutex;

/// Minimal `no_std` spinlock that mirrors the slice of the `std::sync::Mutex`
/// API actually used by this module (`new` + `lock` returning a `Result`).
#[cfg(not(feature = "std"))]
mod nostd_lock {
    use core::{
        cell::UnsafeCell,
        ops::{Deref, DerefMut},
        sync::atomic::{AtomicBool, Ordering},
    };

    pub struct Mutex<T> {
        locked: AtomicBool,
        data: UnsafeCell<T>,
    }

    unsafe impl<T: Send> Send for Mutex<T> {}
    unsafe impl<T: Send> Sync for Mutex<T> {}

    pub struct MutexGuard<'a, T> {
        lock: &'a Mutex<T>,
    }

    impl<T> Mutex<T> {
        pub fn new(data: T) -> Self {
            Mutex {
                locked: AtomicBool::new(false),
                data: UnsafeCell::new(data),
            }
        }

        /// Returns `Ok(guard)` to mirror `std::sync::Mutex::lock`. Never poisons.
        pub fn lock(&self) -> Result<MutexGuard<'_, T>, core::convert::Infallible> {
            while self
                .locked
                .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_err()
            {
                core::hint::spin_loop();
            }
            Ok(MutexGuard { lock: self })
        }
    }

    impl<'a, T> Deref for MutexGuard<'a, T> {
        type Target = T;
        fn deref(&self) -> &T {
            unsafe { &*self.lock.data.get() }
        }
    }

    impl<'a, T> DerefMut for MutexGuard<'a, T> {
        fn deref_mut(&mut self) -> &mut T {
            unsafe { &mut *self.lock.data.get() }
        }
    }

    impl<'a, T> Drop for MutexGuard<'a, T> {
        fn drop(&mut self) {
            self.lock.locked.store(false, Ordering::Release);
        }
    }

    // Mirror `std::sync::Mutex: Debug` so containers can derive Debug. Does not
    // lock (the spinlock has no `try_lock`, and locking in `fmt` could deadlock).
    impl<T> core::fmt::Debug for Mutex<T> {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.debug_struct("Mutex").finish_non_exhaustive()
        }
    }
}

use azul_css::{
    dynamic_selector::{DynamicSelector, DynamicSelectorContext},
    props::basic::color::ColorU,
    system::SystemStyle,
    AzString, OptionString,
};

use crate::{
    dom::{Dom, NodeData, NodeType},
    refany::{OptionRefAny, RefAny},
    styled_dom::StyledDom,
};

// Type name constants for RefAny-based icon type detection in debug output
const IMAGE_ICON_DATA_TYPE_NAME: &str = "ImageIconData";
const FONT_ICON_DATA_TYPE_NAME: &str = "FontIconData";

// Icon Resolver Callback

/// Callback type for resolving icon data to a `StyledDom`.
///
/// Parameters:
/// - `icon_data`: The `RefAny` data from the icon pack (cloned, or None if not found)
/// - `original_icon_dom`: The original icon node's `StyledDom` (contains inline styles, a11y info,
///   `icon_name`)
/// - `system_style`: Current system style (theme, colors, etc.)
///
/// Returns: A `StyledDom` that will replace the icon node.
/// The resolver should copy relevant styles from `original_icon_dom` to the result.
/// Return an empty `StyledDom` to show a placeholder or nothing.
///
/// Note: `icon_name` is accessible via `original_icon_dom.node_data[0].get_node_type()` →
/// `NodeType::Icon(name)`
pub type IconResolverCallbackType = extern "C" fn(
    icon_data: OptionRefAny,
    original_icon_node: &NodeData,
    system_style: &SystemStyle,
) -> Dom;

/// Default resolver: an empty div, i.e. the icon renders as nothing.
#[must_use]
pub extern "C" fn default_icon_resolver(
    _icon_data: OptionRefAny,
    _original_icon_node: &NodeData,
    _system_style: &SystemStyle,
) -> Dom {
    Dom::create_div()
}

// Icon metadata
//
// What the ARTWORK can honour, as opposed to what the system asks for
// (`IconStyleOptions`: grayscale, tint, inherit the text colour). The default
// resolver combines the two - request x capability - and never guesses from
// the kind of icon: a tint on `Mask` artwork is `flood(tint) composite(in)`,
// on `CurrentColor` it is the `color`, on `Palette` the palette is remapped,
// on `None` only the variant for the mode is picked.
// (scripts/ideas/RICING_LAYERS_AND_STOPTHEMINGMYAPP_2026_09_29.md 8.1 / 8.2)

/// The colour MODE (light or dark background) an icon's artwork was drawn
/// for. A mode, not a theme: themes are `flat` / `flora` / user themes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum IconDesignedFor {
    /// Reads on either background (the default).
    #[default]
    Any,
    /// Drawn for a light background: dark ink.
    Light,
    /// Drawn for a dark background: light ink.
    Dark,
}

impl IconDesignedFor {
    /// Does artwork drawn for this mode read unchanged in the given mode?
    #[must_use]
    pub const fn suits(self, dark: bool) -> bool {
        match self {
            Self::Any => true,
            Self::Light => !dark,
            Self::Dark => dark,
        }
    }

    /// `light` / `dark` / `any` (ASCII case-insensitive), as the remap
    /// format spells it.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        let name = name.trim();
        if name.eq_ignore_ascii_case("any") {
            Some(Self::Any)
        } else if name.eq_ignore_ascii_case("light") {
            Some(Self::Light)
        } else if name.eq_ignore_ascii_case("dark") {
            Some(Self::Dark)
        } else {
            None
        }
    }
}

/// Alternative artwork for other modes, each an icon SPEC (the same
/// comma-separated fallback chain `Dom::create_icon` takes).
///
/// The default resolver redirects to the variant for the current mode by
/// resolving the `<icon>` to that spec, so a variant is any registered icon:
/// another image, a font glyph, an SVG, a DOM.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub struct IconVariants {
    /// The artwork for the light mode.
    pub light: OptionString,
    /// The artwork for the dark mode.
    pub dark: OptionString,
    /// The artwork when the user asks for high contrast; beats the mode's.
    pub high_contrast: OptionString,
}

impl IconVariants {
    /// The variant spec for a mode: the high-contrast one first when high
    /// contrast is asked for and one is given, then the mode's own. `None`
    /// means "draw the icon itself". An empty or blank spec counts as absent.
    #[must_use]
    pub fn pick(&self, dark: bool, high_contrast: bool) -> Option<&AzString> {
        fn given(spec: &OptionString) -> Option<&AzString> {
            spec.as_ref().filter(|s| !s.as_str().trim().is_empty())
        }
        if high_contrast {
            if let Some(spec) = given(&self.high_contrast) {
                return Some(spec);
            }
        }
        if dark {
            given(&self.dark)
        } else {
            given(&self.light)
        }
    }
}

/// One entry of a palette remap: paint `from` is drawn as `to`. `to` may be
/// a `system:` colour token (`SystemColorRef::to_color_token`), resolved
/// against the mode the icon is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct IconColorMapping {
    pub from: ColorU,
    pub to: ColorU,
}

impl_option!(
    IconColorMapping,
    OptionIconColorMapping,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl_vec!(
    IconColorMapping,
    IconColorMappingVec,
    IconColorMappingVecDestructor,
    IconColorMappingVecDestructorType,
    IconColorMappingVecSlice,
    OptionIconColorMapping
);
impl_vec_clone!(
    IconColorMapping,
    IconColorMappingVec,
    IconColorMappingVecDestructor
);
impl_vec_debug!(IconColorMapping, IconColorMappingVec);
impl_vec_partialeq!(IconColorMapping, IconColorMappingVec);

/// One colour per mode. Either may be a `system:` colour token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct IconModeColors {
    pub light: ColorU,
    pub dark: ColorU,
}

impl IconModeColors {
    /// The same colour in both modes.
    #[must_use]
    pub const fn same(color: ColorU) -> Self {
        Self {
            light: color,
            dark: color,
        }
    }

    /// The colour for the given mode.
    #[must_use]
    pub const fn for_mode(&self, dark: bool) -> ColorU {
        if dark {
            self.dark
        } else {
            self.light
        }
    }
}

/// HOW an icon's artwork may be recoloured - the capability half of
/// "request x capability". The resolver never guesses it from the kind.
#[derive(Debug, Clone, PartialEq)]
#[repr(C, u8)]
pub enum IconRecolor {
    /// The artwork follows the cascaded `color` of the `<icon>` node, like a
    /// font glyph (the font default). A tint request becomes that colour.
    CurrentColor,
    /// Monochrome ink on alpha: a tint (or, drawn for the other mode, the
    /// text colour) is flooded through the artwork's own alpha
    /// (`flood(c) composite(in)`). Without a request it is drawn as it is.
    Mask,
    /// Multi-colour artwork whose listed paints are swapped when drawn
    /// (SVG). Tints are ignored.
    Palette(IconColorMappingVec),
    /// An explicit colour per mode: beats both the CSS `color` and a tint
    /// request (the remap file's `recolor: "#e6e6e6"`, design 9.1 pitfall 10).
    Fixed(IconModeColors),
    /// Never recoloured (the image default): full-colour artwork gets
    /// `variants`, never a tint.
    None,
}

/// Metadata of a registered icon: which mode its artwork was drawn for,
/// its variants for other modes, and how it may be recoloured.
///
/// Carried by the registered data itself (`ImageIconData::meta`,
/// `FontIconData::meta`, `SvgIconData::meta` in `azul_layout::icon`), so a
/// custom resolver reads it where it reads the artwork.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct IconMeta {
    pub variants: IconVariants,
    pub recolor: IconRecolor,
    pub designed_for: IconDesignedFor,
    /// The artwork is one colour on alpha, so flooding it through its alpha
    /// recolours it without losing detail. Required for `CurrentColor` and
    /// `Fixed` on raster artwork; `Mask` implies it.
    pub monochrome: bool,
}

impl Default for IconMeta {
    /// The conservative default: [`Self::for_image`], never recoloured.
    fn default() -> Self {
        Self::for_image()
    }
}

impl IconMeta {
    /// The default for font icons: the glyph IS the text colour.
    #[must_use]
    pub fn for_font() -> Self {
        Self {
            variants: IconVariants::default(),
            recolor: IconRecolor::CurrentColor,
            designed_for: IconDesignedFor::Any,
            monochrome: true,
        }
    }

    /// The default for images: full-colour artwork, never recoloured.
    #[must_use]
    pub fn for_image() -> Self {
        Self {
            variants: IconVariants::default(),
            recolor: IconRecolor::None,
            designed_for: IconDesignedFor::Any,
            monochrome: false,
        }
    }

    /// Monochrome ink on alpha that a tint may flood (a symbolic PNG).
    #[must_use]
    pub fn for_mask() -> Self {
        Self {
            variants: IconVariants::default(),
            recolor: IconRecolor::Mask,
            designed_for: IconDesignedFor::Any,
            monochrome: true,
        }
    }

    /// Is the artwork an alpha mask a colour can be flooded through?
    #[must_use]
    pub const fn is_mask_artwork(&self) -> bool {
        self.monochrome || matches!(self.recolor, IconRecolor::Mask)
    }

    #[must_use]
    pub fn with_variants(mut self, variants: IconVariants) -> Self {
        self.variants = variants;
        self
    }

    #[must_use]
    pub fn with_light_variant(mut self, spec: impl Into<AzString>) -> Self {
        self.variants.light = OptionString::Some(spec.into());
        self
    }

    #[must_use]
    pub fn with_dark_variant(mut self, spec: impl Into<AzString>) -> Self {
        self.variants.dark = OptionString::Some(spec.into());
        self
    }

    #[must_use]
    pub fn with_high_contrast_variant(mut self, spec: impl Into<AzString>) -> Self {
        self.variants.high_contrast = OptionString::Some(spec.into());
        self
    }

    #[must_use]
    pub fn with_recolor(mut self, recolor: IconRecolor) -> Self {
        self.recolor = recolor;
        self
    }

    #[must_use]
    pub fn with_designed_for(mut self, designed_for: IconDesignedFor) -> Self {
        self.designed_for = designed_for;
        self
    }

    #[must_use]
    pub fn with_monochrome(mut self, monochrome: bool) -> Self {
        self.monochrome = monochrome;
        self
    }
}

// Icon Provider Inner (single mutex)

/// Inner data for `IconProviderHandle` - all fields behind single mutex
#[derive(Debug, Clone)]
pub struct IconProviderInner {
    /// Nested map: `pack_name` → (`icon_name` → `RefAny`)
    /// Differentiation between Image/Font/SVG is via `RefAny::downcast`
    pub icons: BTreeMap<String, BTreeMap<String, RefAny>>,
    /// The resolver callback
    pub resolver: IconResolverCallbackType,
    /// Pack names in the order they were first registered: the lookup order
    /// among packs of equal rank. A pack removed and registered again goes
    /// to the back.
    pub pack_order: Vec<String>,
    /// Pack ranks, lower searched first - a theme's index in the theme
    /// chain, so `icons/xyz/pink/` beats `icons/xyz/` beats the app's own
    /// packs. Packs without a rank are searched after every ranked pack.
    pub pack_ranks: BTreeMap<String, u32>,
    /// Pack conditions: a pack listed here takes part in the search for a
    /// bare icon name only while all its terms hold under the lookup's live
    /// context - the flora theme's icon pack under `theme=flora`
    /// ([`IconProviderHandle::set_pack_condition`]). Like a rank, a setting
    /// about the pack NAME.
    pub pack_conditions: BTreeMap<String, Vec<IconRuleCondition>>,
    /// Remap rules, icon name (lowercase) -> its rules in the order they were
    /// added. See [`IconRemapRule`].
    pub remap: BTreeMap<String, Vec<IconRemapRule>>,
    /// The application's name, which `app=` rule terms compare against.
    pub app_name: String,
}

impl Default for IconProviderInner {
    fn default() -> Self {
        Self {
            icons: BTreeMap::new(),
            resolver: default_icon_resolver,
            pack_order: Vec::new(),
            pack_ranks: BTreeMap::new(),
            pack_conditions: BTreeMap::new(),
            remap: BTreeMap::new(),
            app_name: String::new(),
        }
    }
}

// Icon remap rules
//
// The user's per-name rules (`~/.azul/icons/remap.json`, and one table per
// theme in `~/.azul/icons/<theme>/remap.json`): "draw `material/home` as
// this file when `theme=monokai,mode=dark`". Design:
// scripts/ideas/RICING_LAYERS_AND_STOPTHEMINGMYAPP_2026_09_29.md section 8.

/// One term of a rule's `apply-if` (the comma is AND).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IconRuleCondition {
    /// A term in the dynamic-selector vocabulary CSS conditions use:
    /// `theme=<app theme>` (chain membership, like `@theme(name)`),
    /// `theme=light|dark` / `mode=light|dark` (the mode), `os=<@os content>`,
    /// `contrast=high|normal`. Matched by `DynamicSelector::matches`, the
    /// same matcher the cascade uses.
    Selector(DynamicSelector),
    /// `app=<name>`: the application's name (the executable's).
    App(String),
    /// A term nobody understands, kept for diagnostics. Never matches: a
    /// typo must not turn a rule unconditional.
    Never(String),
}

impl IconRuleCondition {
    /// Does this term hold under `context` for the application `app_name`?
    #[must_use]
    pub fn matches(&self, context: &DynamicSelectorContext, app_name: &str) -> bool {
        match self {
            Self::Selector(selector) => selector.matches(context),
            Self::App(app) => app.eq_ignore_ascii_case(app_name),
            Self::Never(_) => false,
        }
    }
}

/// Parse an `apply-if` string: comma-separated `key=value` terms, all of
/// which must hold. See [`IconRuleCondition`] for the keys. An empty string
/// is no condition at all.
#[must_use]
pub fn parse_icon_apply_if(apply_if: &str) -> Vec<IconRuleCondition> {
    use azul_css::dynamic_selector::{parse_os_at_rule_content, BoolCondition, ThemeCondition};

    let mode = |value: &str| {
        if value.eq_ignore_ascii_case("light") {
            Some(azul_css::dynamic_selector::ModeCondition::Light)
        } else if value.eq_ignore_ascii_case("dark") {
            Some(azul_css::dynamic_selector::ModeCondition::Dark)
        } else {
            None
        }
    };
    let mut conditions = Vec::new();
    for term in apply_if.split(',') {
        let term = term.trim();
        if term.is_empty() {
            continue;
        }
        let never = || IconRuleCondition::Never(term.to_string());
        let Some((key, value)) = term.split_once('=') else {
            conditions.push(never());
            continue;
        };
        let value = value.trim();
        match key.trim().to_ascii_lowercase().as_str() {
            // `light` / `dark` are reserved for the mode (design 9.1
            // pitfall 5); any other name is an app theme in the chain.
            "theme" => conditions.push(IconRuleCondition::Selector(mode(value).map_or_else(
                || DynamicSelector::Theme(ThemeCondition::Custom(AzString::from(value))),
                DynamicSelector::Mode,
            ))),
            "mode" => conditions.push(mode(value).map_or_else(never, |m| {
                IconRuleCondition::Selector(DynamicSelector::Mode(m))
            })),
            "os" => match parse_os_at_rule_content(value) {
                Some(selectors) => conditions
                    .extend(selectors.into_iter().map(IconRuleCondition::Selector)),
                None => conditions.push(never()),
            },
            "contrast" => {
                let wanted = match value.to_ascii_lowercase().as_str() {
                    "high" | "more" => Some(BoolCondition::True),
                    "normal" | "no-preference" | "low" | "less" => Some(BoolCondition::False),
                    _ => None,
                };
                conditions.push(wanted.map_or_else(never, |w| {
                    IconRuleCondition::Selector(DynamicSelector::PrefersHighContrast(w))
                }));
            }
            "app" => conditions.push(IconRuleCondition::App(value.to_string())),
            _ => conditions.push(never()),
        }
    }
    conditions
}

/// One per-name remap rule: while every condition holds, the name is drawn
/// as `target`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconRemapRule {
    /// The theme whose directory the rule came from (`xyz:pink` for
    /// `icons/xyz/pink/`), `None` for the global table. A theme's rules apply
    /// only while that theme is in the chain, and rank by its place there;
    /// the global table ranks after every theme.
    pub theme: Option<String>,
    /// All must hold ([`parse_icon_apply_if`]).
    pub conditions: Vec<IconRuleCondition>,
    /// The icon spec the name is drawn as - a pack-qualified name the loader
    /// registered the rule's file under, or any spec.
    pub target: String,
}

/// The rank of a pack that was given none: after every ranked pack.
pub const ICON_PACK_UNRANKED: u32 = u32::MAX;

// Icon Provider Handle

/// Icon provider stored in `AppConfig`.
///
/// This is a Box<IconProviderInner> for C FFI compatibility.
/// When `App::run()` is called, it gets converted to Arc<Mutex<IconProviderInner>>
/// and cloned to each window.
///
/// Icons are stored in a nested map: `pack_name` → (`icon_name` → `RefAny`)
/// This allows:
/// - Multiple packs with different sources (app-images, material-icons, etc.)
/// - Easy unregistration of entire packs
/// - First-match-wins lookup across all packs
#[repr(C)]
pub struct IconProviderHandle {
    /// Boxed inner data - Box<T> is repr(C) compatible (single pointer).
    /// `ManuallyDrop` so the Box is freed ONLY by our `Drop` (gated on
    /// `run_destructor`), never by drop-glue. The codegen Az wrapper nests an
    /// `AzIconProviderHandle` field (in `AzAppConfig`) whose own `Drop` re-runs
    /// `_delete` -> `drop_in_place::<IconProviderHandle>` on the SAME bytes; with
    /// a bare `Box` the glue freed it a second time -> double free. Same
    /// convention as `GlContextPtr` / `CssPropertyCachePtr`.
    pub inner: ManuallyDrop<Box<IconProviderInner>>,
    pub run_destructor: bool,
}

impl Clone for IconProviderHandle {
    fn clone(&self) -> Self {
        Self {
            inner: ManuallyDrop::new(Box::new((**self.inner).clone())),
            run_destructor: true,
        }
    }
}

impl Drop for IconProviderHandle {
    fn drop(&mut self) {
        // First drop (run_destructor still true) frees the Box and clears the flag
        // in the shared bytes; the codegen's redundant second drop sees false -> no-op.
        if self.run_destructor {
            self.run_destructor = false;
            unsafe {
                ManuallyDrop::drop(&mut self.inner);
            }
        }
    }
}

impl fmt::Debug for IconProviderHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let pack_count = self.inner.icons.len();
        let icon_count: usize = self.inner.icons.values().map(BTreeMap::len).sum();

        f.debug_struct("IconProviderHandle")
            .field("pack_count", &pack_count)
            .field("icon_count", &icon_count)
            .finish_non_exhaustive()
    }
}

impl Default for IconProviderHandle {
    fn default() -> Self {
        Self::new()
    }
}

impl IconProviderInner {
    /// Insert (or replace) one icon. A pack seen for the first time joins
    /// the lookup order at the back; re-registering into a pack that already
    /// exists does not move it.
    pub fn insert_icon(&mut self, pack_name: &str, icon_name: &str, data: RefAny) {
        if !self.icons.contains_key(pack_name) {
            self.pack_order.retain(|p| p != pack_name);
            self.pack_order.push(pack_name.to_string());
        }
        self.icons
            .entry(pack_name.to_string())
            .or_default()
            .insert(icon_name.to_lowercase(), data);
    }

    /// Remove a whole pack, and its place in the registration order (its
    /// rank, a setting about the pack NAME, stays).
    pub fn remove_pack(&mut self, pack_name: &str) {
        self.icons.remove(pack_name);
        self.pack_order.retain(|p| p != pack_name);
    }

    /// The packs in lookup order: rank first (lower first, unranked last),
    /// then registration order, then name (only packs inserted around
    /// [`Self::insert_icon`] have no registration position).
    #[must_use]
    pub fn packs_in_lookup_order(&self) -> Vec<(&str, &BTreeMap<String, RefAny>)> {
        let mut packs: Vec<(u32, usize, &str, &BTreeMap<String, RefAny>)> = self
            .icons
            .iter()
            .map(|(name, pack)| {
                let rank = self
                    .pack_ranks
                    .get(name)
                    .copied()
                    .unwrap_or(ICON_PACK_UNRANKED);
                let registered = self
                    .pack_order
                    .iter()
                    .position(|p| p == name)
                    .unwrap_or(usize::MAX);
                (rank, registered, name.as_str(), pack)
            })
            .collect();
        packs.sort_by(|a, b| (a.0, a.1, a.2).cmp(&(b.0, b.1, b.2)));
        packs
            .into_iter()
            .map(|(_, _, name, pack)| (name, pack))
            .collect()
    }

    /// Does the pack `pack_name` take part in the search for a bare name
    /// under `context`? A pack without a condition always does; one with a
    /// condition ([`IconProviderHandle::set_pack_condition`]) only while all
    /// its terms hold - and never without a context to hold under.
    fn pack_takes_part(&self, pack_name: &str, context: Option<&DynamicSelectorContext>) -> bool {
        match self.pack_conditions.get(pack_name) {
            None => true,
            Some(conditions) => context
                .is_some_and(|ctx| conditions.iter().all(|c| c.matches(ctx, &self.app_name))),
        }
    }

    /// The first pack, in lookup order, that has `name_lower` and takes part
    /// under `context`.
    fn find_in_packs(
        &self,
        name_lower: &str,
        context: Option<&DynamicSelectorContext>,
    ) -> Option<(&str, &RefAny)> {
        self.packs_in_lookup_order()
            .into_iter()
            .filter(|(pack_name, _)| self.pack_takes_part(pack_name, context))
            .find_map(|(pack_name, pack)| pack.get(name_lower).map(|d| (pack_name, d)))
    }

    /// Resolves an icon SPEC to registered icon data.
    ///
    /// A spec is a comma-separated fallback list of entries, each either a
    /// bare icon name (`"content_copy"`, searched across all packs in lookup
    /// order - rank, then registration order - first match wins) or a
    /// pack-qualified name (`"material-icons:save"`, searched only in that
    /// pack). The first entry that resolves wins, so markup can express
    /// per-platform fallbacks: `<icon>ios:open_menu,kde:three-lines,menu</icon>`.
    /// Icon names are case-insensitive; pack names are case-sensitive.
    ///
    /// No window context: a pack with a condition takes no part in the
    /// search ([`Self::lookup_spec_in_context`] evaluates it).
    #[must_use]
    pub fn lookup_spec(&self, spec: &str) -> Option<RefAny> {
        self.lookup_spec_with(spec, None)
    }

    /// [`Self::lookup_spec`] with the packs' conditions evaluated under
    /// `context` (`None`: the conditional packs sit out).
    fn lookup_spec_with(
        &self,
        spec: &str,
        context: Option<&DynamicSelectorContext>,
    ) -> Option<RefAny> {
        // Verbatim first: a registered name is always found as-is (names may
        // legally contain ':', ',' or whitespace). The spec syntax below only
        // applies when nothing is registered under the literal name.
        let verbatim = spec.to_lowercase();
        if let Some((_, data)) = self.find_in_packs(&verbatim, context) {
            return Some(data.clone());
        }

        for entry in spec.split(',') {
            let entry = entry.trim();
            if entry.is_empty() {
                continue;
            }
            let (pack, name) = match entry.split_once(':') {
                Some((p, n)) => (Some(p.trim()), n.trim()),
                None => (None, entry),
            };
            let name_lower = name.to_lowercase();
            // A pack-qualified entry names its pack: a condition (which
            // decides whether the pack joins the SEARCH) does not apply.
            let found = pack.map_or_else(
                || self.find_in_packs(&name_lower, context).map(|(_, d)| d),
                |p| self.icons.get(p).and_then(|pack| pack.get(&name_lower)),
            );
            if let Some(data) = found {
                return Some(data.clone());
            }
        }
        None
    }

    /// Add a remap rule for `icon_name` (case-insensitive), after the rules
    /// it already has.
    pub fn add_remap_rule(&mut self, icon_name: &str, rule: IconRemapRule) {
        self.remap
            .entry(icon_name.trim().to_lowercase())
            .or_default()
            .push(rule);
    }

    /// The data the first applicable remap rule for `name_lower` draws it
    /// as. Rules are ranked by their theme's place in `context`'s chain (the
    /// global table after every theme; a theme not in the chain contributes
    /// nothing), then by the order they were added; the first whose
    /// conditions all hold AND whose target resolves wins.
    fn remapped(&self, name_lower: &str, context: &DynamicSelectorContext) -> Option<RefAny> {
        let rules = self.remap.get(name_lower)?;
        let chain = context.theme_chain.as_ref();
        let mut ranked: Vec<(usize, usize, &IconRemapRule)> = rules
            .iter()
            .enumerate()
            .filter_map(|(index, rule)| {
                let rank = match &rule.theme {
                    None => chain.len(),
                    Some(theme) => chain.iter().position(|live| live.as_str() == theme.as_str())?,
                };
                Some((rank, index, rule))
            })
            .collect();
        ranked.sort_by_key(|(rank, index, _)| (*rank, *index));
        ranked
            .into_iter()
            .filter(|(_, _, rule)| {
                rule.conditions
                    .iter()
                    .all(|c| c.matches(context, &self.app_name))
            })
            .find_map(|(_, _, rule)| self.lookup_spec_with(&rule.target, Some(context)))
    }

    /// [`Self::lookup_spec`] with the remap rules applied first, evaluated
    /// against `context` - the live window's - at this lookup, as are the
    /// packs' conditions (the flora theme's pack sits out under flat).
    ///
    /// Remap first, then the spec's own fallback list: the spec is the
    /// app's statement (`ios:open_menu,kde:three-lines,menu`), the rules the
    /// user's, so a rule for ANY entry (tried in the spec's order) beats the
    /// app's chain, and an unmapped spec still falls through it.
    #[must_use]
    pub fn lookup_spec_in_context(
        &self,
        spec: &str,
        context: &DynamicSelectorContext,
    ) -> Option<RefAny> {
        if !self.remap.is_empty() {
            let verbatim = spec.trim().to_lowercase();
            if let Some(data) = self.remapped(&verbatim, context) {
                return Some(data);
            }
            for entry in spec.split(',') {
                let entry = entry.trim();
                if entry.is_empty() {
                    continue;
                }
                if let Some(data) = self.remapped(&entry.to_lowercase(), context) {
                    return Some(data);
                }
            }
        }
        self.lookup_spec_with(spec, Some(context))
    }
}

impl IconProviderHandle {
    /// Create a new empty icon provider with the default (no-op) resolver.
    ///
    /// Note: The default resolver in core crate returns an empty `StyledDom`.
    /// Use `set_resolver()` to set a proper resolver from the layout crate,
    /// or use `with_resolver()` to create with a custom resolver.
    #[must_use]
    pub fn new() -> Self {
        Self::with_resolver(default_icon_resolver)
    }

    /// Create with a custom resolver callback
    pub fn with_resolver(resolver: IconResolverCallbackType) -> Self {
        Self {
            inner: ManuallyDrop::new(Box::new(IconProviderInner {
                resolver,
                ..IconProviderInner::default()
            })),
            run_destructor: true,
        }
    }

    /// Convert this handle into an Arc<Mutex<IconProviderInner>> for use in windows.
    ///
    /// This consumes the Box and creates an Arc. Called by `App::run()` to create
    /// the shared icon provider that gets cloned to each window.
    pub(crate) fn into_shared(mut self) -> Arc<Mutex<IconProviderInner>> {
        // Take the Box out and disarm our Drop so it doesn't free the moved-out
        // allocation (ManuallyDrop::take leaves `inner` logically uninitialized).
        let inner = unsafe { ManuallyDrop::take(&mut self.inner) };
        self.run_destructor = false;
        Arc::new(Mutex::new(*inner))
    }

    /// Set the resolver callback
    pub fn set_resolver(&mut self, resolver: IconResolverCallbackType) {
        self.inner.resolver = resolver;
    }

    /// Register a single icon in a pack (creates pack if needed).
    ///
    /// Note: `pack_name` is case-sensitive, while `icon_name` is normalized to lowercase.
    pub fn register_icon(&mut self, pack_name: &str, icon_name: &str, data: RefAny) {
        self.inner.insert_icon(pack_name, icon_name, data);
    }

    /// Unregister a single icon from a pack
    pub fn unregister_icon(&mut self, pack_name: &str, icon_name: &str) {
        let now_empty = self.inner.icons.get_mut(pack_name).is_some_and(|pack| {
            pack.remove(&icon_name.to_lowercase());
            pack.is_empty()
        });
        if now_empty {
            self.inner.remove_pack(pack_name);
        }
    }

    /// Unregister an entire icon pack
    pub fn unregister_pack(&mut self, pack_name: &str) {
        self.inner.remove_pack(pack_name);
    }

    /// Rank a pack: packs are searched in rank order (lower first), packs of
    /// equal rank in registration order, and packs without a rank after
    /// every ranked one. The user's theme packs rank by their theme's place
    /// in the theme chain, so they beat the app's packs although the app
    /// registered first.
    pub fn set_pack_rank(&mut self, pack_name: &str, rank: u32) {
        self.inner.pack_ranks.insert(pack_name.to_string(), rank);
    }

    /// Let a pack take part in the search for a bare icon name only while
    /// `apply_if` holds ([`parse_icon_apply_if`]: `theme=flora`, `mode=dark`,
    /// `os=...`, `contrast=high`, `app=...`; the comma is AND). It is
    /// evaluated at every lookup against the window's live context, so a
    /// theme switch brings the pack in or takes it out with the next frame.
    /// The flora theme's own icons, searched first under flora and not at
    /// all under any other theme: `set_pack_rank(pack, 0)` and
    /// `set_pack_condition(pack, "theme=flora")`.
    ///
    /// A pack-qualified spec (`pack:name`) still reaches the pack, the
    /// user's remap rules still come first, and a lookup without a window
    /// context (`lookup`, `has_icon`) passes the pack by. A term nobody
    /// understands never holds: a typo keeps the pack out instead of making
    /// it unconditional. An empty `apply_if` removes the condition; like the
    /// rank, the condition belongs to the pack NAME.
    pub fn set_pack_condition(&mut self, pack_name: &str, apply_if: &str) {
        let conditions = parse_icon_apply_if(apply_if);
        if conditions.is_empty() {
            self.inner.pack_conditions.remove(pack_name);
        } else {
            self.inner
                .pack_conditions
                .insert(pack_name.to_string(), conditions);
        }
    }

    /// Add a global remap rule: while `apply_if` holds
    /// ([`parse_icon_apply_if`]; empty = always), `icon_name` is drawn as
    /// `target_spec`. Rules for one name are tried in the order they were
    /// added; the first that applies and resolves wins. Evaluated at lookup
    /// against the live window context, so `mode=dark` follows a mode switch.
    pub fn add_icon_remap_rule(&mut self, icon_name: &str, apply_if: &str, target_spec: &str) {
        self.inner.add_remap_rule(
            icon_name,
            IconRemapRule {
                theme: None,
                conditions: parse_icon_apply_if(apply_if),
                target: target_spec.to_string(),
            },
        );
    }

    /// [`Self::add_icon_remap_rule`] for a rule of the THEME `theme`
    /// (`xyz:pink`): it applies only while that theme is in the window's
    /// theme chain, and beats the rules of the themes after it in the chain
    /// and the global table - whatever order the rules were added in.
    pub fn add_theme_icon_remap_rule(
        &mut self,
        theme: &str,
        icon_name: &str,
        apply_if: &str,
        target_spec: &str,
    ) {
        self.inner.add_remap_rule(
            icon_name,
            IconRemapRule {
                theme: Some(theme.to_string()),
                conditions: parse_icon_apply_if(apply_if),
                target: target_spec.to_string(),
            },
        );
    }

    /// The application's name, which `app=` rule terms compare against.
    pub fn set_app_name(&mut self, app_name: &str) {
        self.inner.app_name = app_name.to_string();
    }

    /// Look up an icon across all packs in lookup order, returning the pack
    /// name and data reference (first match wins; without a window context,
    /// so a conditional pack sits out)
    fn lookup_with_pack(&self, icon_name: &str) -> Option<(&str, &RefAny)> {
        self.inner.find_in_packs(&icon_name.to_lowercase(), None)
    }

    /// Look up an icon by spec (bare name, `pack:name`, or a comma-separated
    /// fallback list of either form; first match wins).
    #[must_use]
    pub fn lookup(&self, icon_name: &str) -> Option<RefAny> {
        self.inner.lookup_spec(icon_name)
    }

    /// Check if an icon spec resolves in any pack
    #[must_use]
    pub fn has_icon(&self, icon_name: &str) -> bool {
        self.inner.lookup_spec(icon_name).is_some()
    }

    /// List all pack names
    #[must_use]
    pub fn list_packs(&self) -> Vec<String> {
        self.inner.icons.keys().cloned().collect()
    }

    /// List all icon names in a specific pack
    #[must_use]
    pub fn list_icons_in_pack(&self, pack_name: &str) -> Vec<String> {
        self.inner
            .icons
            .get(pack_name)
            .map(|pack| pack.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Debug lookup: returns detailed info about an icon's `RefAny` contents
    #[allow(clippy::used_underscore_binding)] // intentional `_`-prefix (FFI/api.json pub field, or cfg-gated binding); access is deliberate
    #[must_use]
    pub fn debug_lookup(&self, icon_name: &str) -> AzString {
        use core::fmt::Write;

        let icon_name_lower = icon_name.to_lowercase();

        let mut result =
            format!("Debug lookup for icon '{icon_name}' (normalized: '{icon_name_lower}'):\n");

        // Report registered packs
        let _ = writeln!(result, "  Total packs: {}", self.inner.icons.len());
        for (pack_name, pack) in &self.inner.icons {
            let _ = writeln!(result, "    Pack '{}': {} icons", pack_name, pack.len());
            for name in pack.keys() {
                let _ = writeln!(result, "      - {name}");
            }
        }

        // Find the icon using shared lookup helper
        match self.lookup_with_pack(icon_name) {
            Some((pack, data)) => {
                let _ = writeln!(result, "\n  FOUND in pack '{pack}'");
                let type_name = data.get_type_name();
                let _ = writeln!(result, "  RefAny type_name: '{}'", type_name.as_str());

                let debug_info = data.sharing_info.debug_get_refcount_copied();
                let _ = writeln!(
                    result,
                    "  RefAny size: {} bytes",
                    debug_info._internal_layout_size
                );

                let type_str = type_name.as_str();
                if type_str.contains(IMAGE_ICON_DATA_TYPE_NAME) {
                    result.push_str("  RefAny type: ImageIconData (image-based icon)\n");
                } else if type_str.contains(FONT_ICON_DATA_TYPE_NAME) {
                    result.push_str("  RefAny type: FontIconData (font-based icon)\n");
                } else {
                    let _ = writeln!(result, "  RefAny type: UNKNOWN ('{type_str}')");
                }
            }
            None => {
                result.push_str("\n  NOT FOUND in any pack\n");
            }
        }

        AzString::from(result)
    }
}

/// Thread-safe icon provider for use in windows.
///
/// This is created from `IconProviderHandle::into_shared()` in `App::run()`
/// and cloned to each window.
#[derive(Debug, Clone)]
pub struct SharedIconProvider {
    inner: Arc<Mutex<IconProviderInner>>,
    /// Resolution cache — see the module-level `# Caching` section. Shared by
    /// every clone of this provider (all windows), like `inner`.
    cache: Arc<Mutex<IconResolutionCache>>,
}

/// Hard cap on cached resolutions. A frame's live icon set is typically a few
/// dozen; the cap only matters when specs vary without bound (adversarial or
/// generated names). Policy on overflow is FLUSH-ALL: the next frame re-fills
/// with the live set, so a pathological producer degrades to today's uncached
/// behaviour instead of growing without limit.
const ICON_CACHE_CAP: usize = 512;

/// One cached resolution. `original`/`original_styled` are the KEY (together
/// with the spec, the map key one level up); `resolution` is the value.
#[derive(Debug)]
struct IconCacheEntry {
    /// The icon node as it was BEFORE resolution. Two `<icon>` nodes with the
    /// same spec but different inline styles resolve differently, so the node
    /// itself is part of the key.
    original: NodeData,
    /// The resolved replacement, spliced in whole. A `Dom` rather than a
    /// flattened single node: an icon may be an arbitrary styled subtree.
    resolution: Dom,
}

/// See the module-level `# Caching` section.
#[derive(Debug, Default)]
struct IconResolutionCache {
    /// The `SystemStyle` every entry was resolved under. A mismatch flushes:
    /// resolvers read the style (theme, tint, grayscale), so entries from
    /// another style are wrong, not merely stale.
    system_style: Option<SystemStyle>,
    /// The rule context every entry was looked up under
    /// ([`icon_rule_context`]): remap rules read the mode, the theme chain,
    /// the OS and the contrast preference, so another context is another
    /// lookup. Holds ONLY what rules read, so a viewport change - every frame
    /// of a drag-resize - does not flush.
    rule_context: Option<DynamicSelectorContext>,
    /// spec → entries with that spec (usually exactly one; more when the same
    /// icon name appears with different inline styles).
    entries: BTreeMap<String, Vec<IconCacheEntry>>,
    /// Total entry count across all specs (the map holds vecs, so `len()` of
    /// the map alone cannot enforce [`ICON_CACHE_CAP`]).
    total: usize,
}

impl SharedIconProvider {
    /// Create from an `IconProviderHandle` (consumes the handle)
    #[must_use]
    pub fn from_handle(handle: IconProviderHandle) -> Self {
        Self {
            inner: handle.into_shared(),
            cache: Arc::new(Mutex::new(IconResolutionCache::default())),
        }
    }

    /// Register (or REPLACE) one icon on a live shared provider.
    ///
    /// The registration path that exists after startup. `IconProviderHandle`
    /// is consumed by [`Self::from_handle`], so a pack built once at
    /// `App::create` could never be refreshed - and it has to be: a pack whose
    /// artwork depends on the OS theme (the desktop's own icons, tinted with
    /// the palette) is WRONG the moment the theme flips, and re-reading it is
    /// the only way to get the dark variant.
    ///
    /// Flushes the resolution cache: entries there hold the Dom the OLD
    /// artwork resolved to, and serving those back would make the
    /// re-registration invisible.
    pub fn register_icon(&self, pack_name: &str, icon_name: &str, data: RefAny) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.insert_icon(pack_name, icon_name, data);
        }
        if let Ok(mut cache) = self.cache.lock() {
            cache.entries.clear();
            cache.total = 0;
            // The next batch re-validates against whatever it carries.
            cache.system_style = None;
            cache.rule_context = None;
        }
    }

    /// Flush the cache if the style or the rule context differs from the one
    /// its entries were resolved under. Called ONCE per resolution pass, not
    /// per icon, so the comparison is per-frame.
    fn validate_cache(&self, system_style: &SystemStyle, rule_context: &DynamicSelectorContext) {
        let Ok(mut cache) = self.cache.lock() else {
            return;
        };
        let same_style = cache.system_style.as_ref() == Some(system_style);
        let same_rules = cache.rule_context.as_ref() == Some(rule_context);
        if !(same_style && same_rules) {
            cache.entries.clear();
            cache.total = 0;
            cache.system_style = Some(system_style.clone());
            cache.rule_context = Some(rule_context.clone());
        }
    }

    /// Cache hit test. `None` = miss (resolve for real, then
    /// [`Self::store_resolution`]).
    fn cached_resolution(&self, spec: &str, node: &NodeData) -> Option<Dom> {
        let cache = self.cache.lock().ok()?;
        cache
            .entries
            .get(spec)?
            .iter()
            .find_map(|e| (e.original == *node).then(|| e.resolution.clone()))
    }

    /// Insert a freshly-resolved entry, flushing everything first if the cap
    /// is reached (see [`ICON_CACHE_CAP`]).
    fn store_resolution(&self, spec: &str, node: &NodeData, resolution: &Dom) {
        let Ok(mut cache) = self.cache.lock() else {
            return;
        };
        if cache.total >= ICON_CACHE_CAP {
            cache.entries.clear();
            cache.total = 0;
        }
        cache
            .entries
            .entry(spec.to_string())
            .or_default()
            .push(IconCacheEntry {
                original: node.clone(),
                resolution: resolution.clone(),
            });
        cache.total += 1;
    }

    /// Resolve an icon to a `StyledDom` using the registered callback, the
    /// remap rules evaluated against the context `system_style` alone implies
    /// (no window). See [`Self::resolve_in_context`].
    #[must_use]
    pub fn resolve(
        &self,
        original_icon_node: &NodeData,
        icon_name: &str,
        system_style: &SystemStyle,
    ) -> Dom {
        let rule_context = icon_rule_context(system_style, None);
        self.resolve_in_context(original_icon_node, icon_name, system_style, &rule_context)
    }

    /// Resolve an icon: remap rules evaluated against `rule_context` (the
    /// live window's), then the spec's own fallback list, then the resolver.
    #[must_use]
    pub fn resolve_in_context(
        &self,
        original_icon_node: &NodeData,
        icon_name: &str,
        system_style: &SystemStyle,
        rule_context: &DynamicSelectorContext,
    ) -> Dom {
        let (resolver, lookup_result) = {
            let Ok(guard) = self.inner.lock() else {
                return Dom::create_div();
            };

            let resolver = guard.resolver;
            let lookup_result = guard.lookup_spec_in_context(icon_name, rule_context);

            (resolver, lookup_result)
        };

        resolver(lookup_result.into(), original_icon_node, system_style)
    }

    /// [`Self::resolve_in_context`], memoised on `(spec, icon node)`.
    ///
    /// Neither the system style nor the rule context is part of the key: a
    /// change to either clears the whole cache once per pass
    /// (`validate_cache`), which is cheaper than carrying them in every entry.
    #[must_use]
    fn resolve_cached(
        &self,
        original_icon_node: &NodeData,
        icon_name: &str,
        system_style: &SystemStyle,
        rule_context: &DynamicSelectorContext,
    ) -> Dom {
        if let Some(hit) = self.cached_resolution(icon_name, original_icon_node) {
            return hit;
        }
        let resolved =
            self.resolve_in_context(original_icon_node, icon_name, system_style, rule_context);
        self.store_resolution(icon_name, original_icon_node, &resolved);
        resolved
    }

    /// Look up an icon by spec (bare name, `pack:name`, or a comma-separated
    /// fallback list of either form; first match wins)
    #[must_use]
    pub fn lookup(&self, icon_name: &str) -> Option<RefAny> {
        self.inner
            .lock()
            .ok()
            .and_then(|guard| guard.lookup_spec(icon_name))
    }

    /// Check if an icon spec resolves
    #[must_use]
    pub fn has_icon(&self, icon_name: &str) -> bool {
        self.inner
            .lock()
            .map(|guard| guard.lookup_spec(icon_name).is_some())
            .unwrap_or(false)
    }
}

// Icon Resolution in the Dom tree

/// How many times an icon may resolve to another icon before we stop.
///
/// Chains are legitimate - restyling an existing icon by registering a `Dom`
/// that contains it is the obvious way to do it - but a resolver is user code,
/// so a cycle has to terminate. Direct self-reference is caught exactly; this
/// bounds everything longer.
const MAX_ICON_INDIRECTION: usize = 8;

/// Replace every `NodeType::Icon` node in `dom` with whatever the registered
/// resolver returns for it.
///
/// # Why this runs on a `Dom`, BEFORE the cascade
///
/// This used to run on a `StyledDom`, after the cascade, and it is worth
/// recording why that was wrong - the shape of the old code is still visible in
/// the git history and in several comments elsewhere.
///
/// A `StyledDom` is a FLAT ARENA in DFS order: a node's first child is the next
/// index. So a replacement's children could not be attached to the icon node
/// after the fact without inserting mid-arena and shifting every index after
/// them. The old code therefore flattened every replacement down to its ROOT
/// node's `node_type` / `style` / `accessibility` plus a single glyph character
/// threaded into a text leaf, and threw the rest away - including the whole
/// `CssPropertyCache` that the resolver's own cascade had just built. Its own
/// comment said so: "everything else in the returned `StyledDom` ... was always
/// discarded".
///
/// That cost three things:
///
/// * **A wasted cascade per icon**, whose result was discarded.
/// * **Any icon that is not one node was impossible.** Registering a styled `Dom` as an icon could
///   not work, because only the root survived.
/// * **A stale property cache.** Rewriting a node's inline `style` after the cascade left the
///   precomputed per-node arrays describing the PRE-resolution node. For a font icon that hid
///   `font-family: StyleFontFamily::Ref(face)` - the only place that face is named - from font
///   collection, so shaping fell back to a face with no glyph at the icon's private-use codepoint
///   and drew `.notdef`. It needed an explicit cache rebuild to paper over.
///
/// Running on the `Dom` removes all three by construction. A `Dom` is a real
/// tree (`root` + `children` + its own `css`), so a replacement is spliced whole;
/// nothing is cascaded twice because the cascade has not happened yet; and there
/// is no property cache to invalidate. An icon is now free to be an arbitrary
/// styled subtree, which is what makes "register a `Dom` as an icon" work -
/// including the colour it should be, which travels with the icon rather than
/// having to be threaded through every call site as a tint parameter.
pub fn resolve_icons_in_dom(
    dom: &mut Dom,
    provider: &SharedIconProvider,
    system_style: &SystemStyle,
) {
    resolve_icons_in_dom_with_context(dom, provider, system_style, None);
}

/// [`resolve_icons_in_dom`] under a window's selector context.
///
/// The context decides two things the `SystemStyle` alone cannot:
///
/// * the MODE the icons are drawn in - the window's (an app pinned dark on a light desktop, the
///   `AZ_THEME` pin), handed to the resolver as the style's `theme`, which is where resolvers read
///   it ([`style_in_window_mode`]);
/// * what remap rules see: `apply-if` is evaluated at LOOKUP against this live context (mode,
///   theme chain, OS, contrast), not once at startup, so a light -> dark switch swaps the artwork
///   on the very next pass.
///
/// `None` (no window yet, a tray, the client-side decorations) uses the
/// context the style implies.
pub fn resolve_icons_in_dom_with_context(
    dom: &mut Dom,
    provider: &SharedIconProvider,
    system_style: &SystemStyle,
    context: Option<&DynamicSelectorContext>,
) {
    let rule_context = icon_rule_context(system_style, context);
    let in_window_mode = style_in_window_mode(system_style, &rule_context);
    let system_style = in_window_mode.as_ref().unwrap_or(system_style);
    // A change of either (theme flip, tint, grayscale, theme chain)
    // invalidates every cached resolution. Checked once per pass, not once
    // per icon.
    provider.validate_cache(system_style, &rule_context);
    resolve_icons_in_dom_inner(dom, provider, system_style, &rule_context);
}

/// The part of a selector context icon remap rules read - mode, theme
/// chain, OS, desktop, contrast - taken from the live `context` when there is
/// one; everything else at what `system_style` alone implies. Keeping the
/// rest fixed is what lets the resolution cache compare contexts per frame
/// without a resize (a new viewport every frame) flushing it.
fn icon_rule_context(
    system_style: &SystemStyle,
    context: Option<&DynamicSelectorContext>,
) -> DynamicSelectorContext {
    let mut rules = DynamicSelectorContext::from_system_style(system_style);
    if let Some(live) = context {
        rules.os = live.os;
        rules.os_version = live.os_version;
        rules.desktop_env = live.desktop_env;
        rules.de_version = live.de_version;
        rules.mode = live.mode;
        rules.theme_chain = live.theme_chain.clone();
        rules.prefers_high_contrast = live.prefers_high_contrast;
        rules.system_colors = live.system_colors;
    }
    rules
}

/// `system_style` in the mode (and contrast) `context` evaluates, when that
/// differs from the style's own: the palette of that mode, `theme` set to
/// it. `None` when the style already matches, the usual case.
fn style_in_window_mode(
    system_style: &SystemStyle,
    context: &DynamicSelectorContext,
) -> Option<SystemStyle> {
    use azul_css::{dynamic_selector::ThemeCondition, system::DarkLightMode};

    let mode = context.mode;
    if system_style.mode == mode
        && system_style.prefers_high_contrast == context.prefers_high_contrast
    {
        return None;
    }
    let mut in_mode = system_style.clone();
    in_mode.colors = system_style.colors_for_theme(mode);
    in_mode.mode = mode;
    in_mode.prefers_high_contrast = context.prefers_high_contrast;
    Some(in_mode)
}

/// Resolve every `<icon>` in a user `Dom` and cascade it - the two halves of
/// "a `Dom` the application handed us becomes a `StyledDom`", as one call.
///
/// The halves were separate, and that is exactly how a path came to skip one:
/// three call sites ran `resolve_icons_in_dom` and then
/// `StyledDom::create_from_dom`, while a fourth - the DOM a `VirtualView`
/// callback returns - ran only the cascade. An `<icon>` inside a virtual view
/// therefore never resolved, and nothing downstream would ever resolve it
/// later, so it stayed an empty node for the life of the view.
///
/// The order is not interchangeable and is not obvious from either name:
/// resolution MUST precede the cascade, because a replacement is a SUBTREE and
/// `StyledDom` is a flat arena in DFS order - splicing one in afterwards would
/// mean inserting mid-arena and shifting every index after it, which is what
/// used to flatten every icon down to its root node. Giving the pair a single
/// name is what stops the next caller from re-deriving that.
#[must_use]
pub fn styled_dom_resolving_icons(
    dom: Dom,
    provider: &SharedIconProvider,
    system_style: &SystemStyle,
) -> StyledDom {
    styled_dom_resolving_icons_with_context(dom, provider, system_style, None)
}

/// [`styled_dom_resolving_icons`], cascading under a known window context.
///
/// The context reaches the first cascade (`StyledDom::create_with_context`),
/// so the funnel's later context offer is a no-op rather than a re-cascade.
#[must_use]
pub fn styled_dom_resolving_icons_with_context(
    dom: Dom,
    provider: &SharedIconProvider,
    system_style: &SystemStyle,
    context: Option<DynamicSelectorContext>,
) -> StyledDom {
    styled_dom_resolving_icons_with_user_sheets(dom, provider, system_style, context, &[])
}

/// [`styled_dom_resolving_icons_with_context`] with USER-origin stylesheets -
/// the end user's rice - cascaded over the whole window
/// (`StyledDom::create_from_dom_with_user_sheets`).
#[must_use]
pub fn styled_dom_resolving_icons_with_user_sheets(
    mut dom: Dom,
    provider: &SharedIconProvider,
    system_style: &SystemStyle,
    context: Option<DynamicSelectorContext>,
    user_sheets: &[azul_css::css::Css],
) -> StyledDom {
    resolve_icons_in_dom_with_context(&mut dom, provider, system_style, context.as_ref());
    StyledDom::create_from_dom_with_user_sheets(dom, context, user_sheets)
}

/// The private dataset behind [`Dom::create_icon_view`]: the spec that view
/// renders right now. Public because the swap API downcasts it - see
/// `CallbackInfo::set_icon`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IconViewState {
    /// An icon spec, i.e. a comma-separated fallback chain exactly as
    /// `Dom::create_icon` takes ("system:titlebar-close,close").
    pub spec: AzString,
}

/// The body of [`Dom::create_icon_view`], which is where this is documented.
///
/// Not public itself: the constructor is the API, and two spellings of one
/// thing is how they drift.
#[must_use]
pub(crate) fn icon_view(spec: impl Into<AzString>) -> Dom {
    let dataset = RefAny::new(IconViewState { spec: spec.into() });
    Dom::create_virtual_view(
        dataset.clone(),
        crate::callbacks::VirtualViewCallback::create(render_icon_view),
    )
    // The SAME `RefAny` as the view's own payload, on the node: the swap API
    // reaches it through `CallbackInfo::get_dataset`, and a clone points at
    // the same data, so rewriting the spec here is what the callback reads
    // there. (The progress bar's fast path is built the same way.)
    .with_dataset(OptionRefAny::Some(dataset))
    // Lays out like the icon node it stands in for. A `VirtualView` defaults
    // to `display: block` (it exists to virtualize scrollable content) and to
    // `overflow: auto` with it - but an icon is INLINE content, and one that
    // grows a scrollbar is absurd. The view also reports the icon's MEASURED
    // size, which can exceed a box the caller sized itself (a 40px icon asked
    // to sit in a 24px button), and `auto` would answer that with a bar.
    //
    // A caller's own `with_css` is appended after this and so wins on anything
    // it states; this only fills in what the caller has no reason to think
    // about.
    .with_css("display: inline-block; overflow: hidden;")
}

/// [`icon_view`]'s callback: render the spec the dataset currently holds.
extern "C" fn render_icon_view(
    mut data: RefAny,
    info: crate::callbacks::VirtualViewCallbackInfo,
) -> crate::callbacks::VirtualViewReturn {
    use crate::geom::{LogicalPosition, LogicalRect, LogicalSize};

    let spec = match data.downcast_ref::<IconViewState>() {
        // Foreign payload: render nothing rather than lie about bounds.
        None => return crate::callbacks::VirtualViewReturn::default(),
        Some(state) => state.spec.clone(),
    };
    let dom = Dom::create_icon(spec);

    // How big the icon actually is. `measure_dom` styles through the window,
    // which resolves the icon first - so this measures the ARTWORK, not the
    // empty `<icon>` node.
    //
    // Measured against the view's own box, which is what a replaced element's
    // content is laid out in. An auto-sized view's box is the replaced-element
    // default (300x150) on the first pass and the icon's own size afterwards;
    // either is a box an icon fits in, so the measurement is the icon's
    // natural size in both. A box of ZERO is the degenerate case - a view in a
    // collapsed parent - where a real constraint would measure the icon to
    // nothing.
    let bounds = info.bounds.get_logical_size();
    let available = if bounds.width > 0.0 && bounds.height > 0.0 {
        bounds
    } else {
        LogicalSize::new(UNCONSTRAINED, UNCONSTRAINED)
    };
    let measured = info.measure_dom(dom.clone(), available);
    // A measurement of zero means there was no measure hook (or nothing to
    // draw); reporting it would collapse an auto-sized view to nothing.
    let size = if measured.width > 0.0 && measured.height > 0.0 {
        measured
    } else {
        bounds
    };

    let rect = LogicalRect::new(LogicalPosition::zero(), size);
    // An icon does not scroll, so all three rects are the same box.
    crate::callbacks::VirtualViewReturn::with_dom(dom, rect, rect)
}

/// The "no constraint" box an auto-sized icon is measured in. Large enough
/// that no icon is wrapped or clipped by it, finite so a bug cannot turn into
/// a NaN geometry.
const UNCONSTRAINED: f32 = 4096.0;

/// The recursive half of [`resolve_icons_in_dom`].
fn resolve_icons_in_dom_inner(
    dom: &mut Dom,
    provider: &SharedIconProvider,
    system_style: &SystemStyle,
    rule_context: &DynamicSelectorContext,
) {
    // An icon may resolve TO another icon - registering
    // `Dom::create_icon("favorite").with_css("color: red")` under another name
    // is the natural way to restyle an existing icon - so this iterates rather
    // than resolving once.
    //
    // Bounded two ways, because a resolver is user code and can trivially cycle:
    // a resolution that yields the SAME spec is a self-reference and stops
    // immediately, and any longer cycle stops at `MAX_ICON_INDIRECTION`. In both
    // cases the node is left as-is rather than looping forever.
    let mut seen = 0;
    while let Some(spec) = icon_spec_of(dom) {
        if seen >= MAX_ICON_INDIRECTION {
            break;
        }
        let replacement =
            provider.resolve_cached(&dom.root, spec.as_str(), system_style, rule_context);
        if icon_spec_of(&replacement).as_ref().map(AzString::as_str) == Some(spec.as_str()) {
            // Resolves to itself: replacing would spin.
            break;
        }
        // The whole node is replaced, children included: an `<icon>name</icon>`
        // carries its spec as a text child, and leaving it would render the raw
        // spec next to the resolved icon.
        //
        // Its STYLESHEETS are carried forward, though. `Dom::with_css` attaches
        // a scoped stylesheet to `Dom::css` rather than inline properties, so
        // `Dom::create_icon("favorite").with_css("color: red")` - the natural
        // way to register a recoloured icon - keeps the colour in `css`, not on
        // the node. Dropping it with the node made the replacement render in the
        // default colour and silently ignore the caller's styling.
        //
        // The replaced node's sheets go FIRST so the replacement's own
        // declarations still win on conflict.
        let mut css = dom.css.clone().into_library_owned_vec();
        let mut replacement = replacement;
        css.extend(replacement.css.clone().into_library_owned_vec());
        replacement.css = css.into();
        // And what made it THIS node of the app's tree: an icon button's
        // click, the id it is found by, its classes, its tab stop
        // (`NodeData::carry_identity_from`).
        replacement.root.carry_identity_from(&dom.root);
        *dom = replacement;
        seen += 1;
    }

    for child in dom.children.as_mut() {
        resolve_icons_in_dom_inner(child, provider, system_style, rule_context);
    }
}

/// The icon spec for a node, or `None` if it is not an icon node.
///
/// An icon with an explicit non-empty name (`Dom::create_icon("x")`) uses it
/// directly. One with an EMPTY name - the markup form `<icon>content_copy</icon>`,
/// where the tag carries no name - derives the spec from its direct text
/// children, exactly like a ligature icon font turns glyph text into an icon.
fn icon_spec_of(dom: &Dom) -> Option<AzString> {
    let NodeType::Icon(name) = dom.root.get_node_type() else {
        return None;
    };
    let name = name.as_str();
    if !name.is_empty() {
        return Some(AzString::from(name));
    }

    let mut derived = alloc::string::String::new();
    for child in dom.children.as_ref() {
        if let NodeType::Text(t) = child.root.get_node_type() {
            derived.push_str(t.as_str());
        }
    }
    let derived = derived.trim();
    if derived.is_empty() {
        None
    } else {
        Some(AzString::from(derived))
    }
}

// FFI Option Types

impl_option!(IconProviderHandle, OptionIconProviderHandle, [Clone]);

#[cfg(test)]
#[path = "icon_test.rs"]
mod icon_test;
