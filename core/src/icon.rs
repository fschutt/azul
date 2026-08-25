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
//! - The KEY includes the whole original `NodeData` + `StyledNode`, because a
//!   custom resolver may read anything from `original_icon_dom` (the default
//!   one copies inline styles and accessibility info). Same name with
//!   different inline styles → separate entries; a hover-state flip on the
//!   node → different `StyledNode` → re-resolve.
//! - The icon SET and the resolver are frozen once the provider is shared
//!   (`App::run` consumes the handle; `SharedIconProvider` exposes no
//!   registration), so registration invalidation cannot be needed post-share.
//! - "Animated icons" remain compatible: animation is carried by the DATA the
//!   resolver returns (e.g. an image-callback node that animates per frame),
//!   not by re-resolving per frame — re-resolution only ever happened on DOM
//!   regeneration anyway.
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
use core::fmt;
use core::mem::ManuallyDrop;

#[cfg(feature = "std")]
use std::sync::Mutex;

#[cfg(not(feature = "std"))]
use self::nostd_lock::Mutex;

/// Minimal `no_std` spinlock that mirrors the slice of the `std::sync::Mutex`
/// API actually used by this module (`new` + `lock` returning a `Result`).
#[cfg(not(feature = "std"))]
mod nostd_lock {
    use core::cell::UnsafeCell;
    use core::ops::{Deref, DerefMut};
    use core::sync::atomic::{AtomicBool, Ordering};

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
            Mutex { locked: AtomicBool::new(false), data: UnsafeCell::new(data) }
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

use azul_css::{AzString, system::SystemStyle};

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
/// - `original_icon_dom`: The original icon node's `StyledDom` (contains inline styles, a11y info, `icon_name`)
/// - `system_style`: Current system style (theme, colors, etc.)
///
/// Returns: A `StyledDom` that will replace the icon node.
/// The resolver should copy relevant styles from `original_icon_dom` to the result.
/// Return an empty `StyledDom` to show a placeholder or nothing.
///
/// Note: `icon_name` is accessible via `original_icon_dom.node_data[0].get_node_type()` → `NodeType::Icon(name)`
pub type IconResolverCallbackType = extern "C" fn(
    icon_data: OptionRefAny,
    original_icon_node: &NodeData,
    system_style: &SystemStyle,
) -> Dom;

/// Default resolver: an empty div, i.e. the icon renders as nothing.
#[must_use] pub extern "C" fn default_icon_resolver(
    _icon_data: OptionRefAny,
    _original_icon_node: &NodeData,
    _system_style: &SystemStyle,
) -> Dom {
    Dom::create_div()
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
}

impl Default for IconProviderInner {
    fn default() -> Self {
        Self {
            icons: BTreeMap::new(),
            resolver: default_icon_resolver,
        }
    }
}

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
    /// Resolves an icon SPEC to registered icon data.
    ///
    /// A spec is a comma-separated fallback list of entries, each either a
    /// bare icon name (`"content_copy"`, searched across all packs in
    /// registration order, first match wins) or a pack-qualified name
    /// (`"material-icons:save"`, searched only in that pack). The first
    /// entry that resolves wins, so markup can express per-platform
    /// fallbacks: `<icon>ios:open_menu,kde:three-lines,menu</icon>`.
    /// Icon names are case-insensitive; pack names are case-sensitive.
    #[must_use] pub fn lookup_spec(&self, spec: &str) -> Option<RefAny> {
        // Verbatim first: a registered name is always found as-is (names may
        // legally contain ':', ',' or whitespace). The spec syntax below only
        // applies when nothing is registered under the literal name.
        let verbatim = spec.to_lowercase();
        if let Some(data) = self.icons.values().find_map(|pack| pack.get(&verbatim)) {
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
            let found = pack.map_or_else(
                || self.icons.values().find_map(|pack| pack.get(&name_lower)),
                |p| self.icons.get(p).and_then(|pack| pack.get(&name_lower)),
            );
            if let Some(data) = found {
                return Some(data.clone());
            }
        }
        None
    }
}

impl IconProviderHandle {
    /// Create a new empty icon provider with the default (no-op) resolver.
    /// 
    /// Note: The default resolver in core crate returns an empty `StyledDom`.
    /// Use `set_resolver()` to set a proper resolver from the layout crate,
    /// or use `with_resolver()` to create with a custom resolver.
    #[must_use] pub fn new() -> Self {
        Self {
            inner: ManuallyDrop::new(Box::new(IconProviderInner {
                icons: BTreeMap::new(),
                resolver: default_icon_resolver,
            })),
            run_destructor: true,
        }
    }

    /// Create with a custom resolver callback
    pub fn with_resolver(resolver: IconResolverCallbackType) -> Self {
        Self {
            inner: ManuallyDrop::new(Box::new(IconProviderInner {
                icons: BTreeMap::new(),
                resolver,
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
        let pack = self.inner.icons
            .entry(pack_name.to_string())
            .or_default();
        pack.insert(icon_name.to_lowercase(), data);
    }

    /// Unregister a single icon from a pack
    pub fn unregister_icon(&mut self, pack_name: &str, icon_name: &str) {
        if let Some(pack) = self.inner.icons.get_mut(pack_name) {
            pack.remove(&icon_name.to_lowercase());
            if pack.is_empty() {
                self.inner.icons.remove(pack_name);
            }
        }
    }

    /// Unregister an entire icon pack
    pub fn unregister_pack(&mut self, pack_name: &str) {
        self.inner.icons.remove(pack_name);
    }

    /// Look up an icon across all packs, returning the pack name and data reference (first match wins)
    fn lookup_with_pack(&self, icon_name: &str) -> Option<(&str, &RefAny)> {
        let icon_name_lower = icon_name.to_lowercase();
        for (pack_name, pack) in &self.inner.icons {
            if let Some(data) = pack.get(&icon_name_lower) {
                return Some((pack_name.as_str(), data));
            }
        }
        None
    }

    /// Look up an icon by spec (bare name, `pack:name`, or a comma-separated
    /// fallback list of either form; first match wins).
    #[must_use] pub fn lookup(&self, icon_name: &str) -> Option<RefAny> {
        self.inner.lookup_spec(icon_name)
    }

    /// Check if an icon spec resolves in any pack
    #[must_use] pub fn has_icon(&self, icon_name: &str) -> bool {
        self.inner.lookup_spec(icon_name).is_some()
    }

    /// List all pack names
    #[must_use] pub fn list_packs(&self) -> Vec<String> {
        self.inner.icons.keys().cloned().collect()
    }

    /// List all icon names in a specific pack
    #[must_use] pub fn list_icons_in_pack(&self, pack_name: &str) -> Vec<String> {
        self.inner.icons.get(pack_name)
            .map(|pack| pack.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Debug lookup: returns detailed info about an icon's `RefAny` contents
    #[allow(clippy::used_underscore_binding)] // intentional `_`-prefix (FFI/api.json pub field, or cfg-gated binding); access is deliberate
    #[must_use] pub fn debug_lookup(&self, icon_name: &str) -> AzString {
        use core::fmt::Write;

        let icon_name_lower = icon_name.to_lowercase();

        let mut result = format!("Debug lookup for icon '{icon_name}' (normalized: '{icon_name_lower}'):\n");

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
                let _ = writeln!(result, "  RefAny size: {} bytes", debug_info._internal_layout_size);

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
    /// spec → entries with that spec (usually exactly one; more when the same
    /// icon name appears with different inline styles).
    entries: BTreeMap<String, Vec<IconCacheEntry>>,
    /// Total entry count across all specs (the map holds vecs, so `len()` of
    /// the map alone cannot enforce [`ICON_CACHE_CAP`]).
    total: usize,
}

impl SharedIconProvider {
    /// Create from an `IconProviderHandle` (consumes the handle)
    #[must_use] pub fn from_handle(handle: IconProviderHandle) -> Self {
        Self {
            inner: handle.into_shared(),
            cache: Arc::new(Mutex::new(IconResolutionCache::default())),
        }
    }

    /// Flush the cache if `system_style` differs from the one its entries
    /// were resolved under. Called ONCE per `resolve_icons_in_styled_dom`
    /// batch, not per icon, so the `SystemStyle` comparison is per-frame.
    fn validate_cache_for_style(&self, system_style: &SystemStyle) {
        let Ok(mut cache) = self.cache.lock() else { return };
        match &cache.system_style {
            Some(cached) if cached == system_style => {}
            _ => {
                cache.entries.clear();
                cache.total = 0;
                cache.system_style = Some(system_style.clone());
            }
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
        let Ok(mut cache) = self.cache.lock() else { return };
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
    
    /// Resolve an icon to a `StyledDom` using the registered callback
    #[must_use] pub fn resolve(
        &self,
        original_icon_node: &NodeData,
        icon_name: &str,
        system_style: &SystemStyle,
    ) -> Dom {
        let (resolver, lookup_result) = {
            let Ok(guard) = self.inner.lock() else {
                return Dom::create_div();
            };

            let resolver = guard.resolver;
            let lookup_result = guard.lookup_spec(icon_name);

            (resolver, lookup_result)
        };

        resolver(lookup_result.into(), original_icon_node, system_style)
    }

    /// [`Self::resolve`], memoised on `(spec, icon node)`.
    ///
    /// The system style is not part of the key: a change to it clears the whole
    /// cache once per pass (`validate_cache_for_style`), which is cheaper than
    /// carrying it in every entry.
    #[must_use] fn resolve_cached(
        &self,
        original_icon_node: &NodeData,
        icon_name: &str,
        system_style: &SystemStyle,
    ) -> Dom {
        if let Some(hit) = self.cached_resolution(icon_name, original_icon_node) {
            return hit;
        }
        let resolved = self.resolve(original_icon_node, icon_name, system_style);
        self.store_resolution(icon_name, original_icon_node, &resolved);
        resolved
    }

    /// Look up an icon by spec (bare name, `pack:name`, or a comma-separated
    /// fallback list of either form; first match wins)
    #[must_use] pub fn lookup(&self, icon_name: &str) -> Option<RefAny> {
        self.inner.lock().ok().and_then(|guard| guard.lookup_spec(icon_name))
    }

    /// Check if an icon spec resolves
    #[must_use] pub fn has_icon(&self, icon_name: &str) -> bool {
        self.inner.lock()
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
/// * **Any icon that is not one node was impossible.** Registering a styled
///   `Dom` as an icon could not work, because only the root survived.
/// * **A stale property cache.** Rewriting a node's inline `style` after the
///   cascade left the precomputed per-node arrays describing the PRE-resolution
///   node. For a font icon that hid `font-family: StyleFontFamily::Ref(face)` -
///   the only place that face is named - from font collection, so shaping fell
///   back to a face with no glyph at the icon's private-use codepoint and drew
///   `.notdef`. It needed an explicit cache rebuild to paper over.
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
    // A SystemStyle change (theme flip, tint, grayscale) invalidates every
    // cached resolution. Checked once per pass, not once per icon.
    provider.validate_cache_for_style(system_style);
    resolve_icons_in_dom_inner(dom, provider, system_style);
}

/// The recursive half of [`resolve_icons_in_dom`].
fn resolve_icons_in_dom_inner(
    dom: &mut Dom,
    provider: &SharedIconProvider,
    system_style: &SystemStyle,
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
        let replacement = provider.resolve_cached(&dom.root, spec.as_str(), system_style);
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
        *dom = replacement;
        seen += 1;
    }

    for child in dom.children.as_mut() {
        resolve_icons_in_dom_inner(child, provider, system_style);
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

impl_option!(
    IconProviderHandle,
    OptionIconProviderHandle,
    [Clone]
);

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::too_many_lines)]
mod autotest_generated {
    use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering};

    use super::*;
    use crate::{dom::NodeDataVec, styled_dom::StyledNodeVec};

    // Test payloads. The names `ImageIconData` / `FontIconData` are load-bearing:
    // `debug_lookup` sniffs `RefAny::get_type_name()` (i.e. `core::any::type_name`)
    // for those substrings.
    #[derive(Debug, Clone, PartialEq)]
    struct TestIconData {
        id: u32,
    }
    #[derive(Debug)]
    struct ImageIconData {
        _w: u32,
    }
    #[derive(Debug)]
    struct FontIconData {
        _codepoint: u32,
    }

    /// Empty / control / unicode / huge names, all of which are legal icon names
    /// (the API places no constraints on them).
    fn adversarial_names() -> Vec<String> {
        vec![
            String::new(),
            String::from(" "),
            String::from("   "),
            String::from("\t\n\r"),
            String::from("\0"),
            String::from("a\0b"),
            String::from("\u{1b}[0m"),
            String::from("../../etc/passwd"),
            String::from("home;garbage"),
            String::from("{\"json\":true}"),
            String::from("-0"),
            String::from("NaN"),
            String::from("inf"),
            String::from("9223372036854775807"),
            String::from("\u{1F600}"),               // emoji
            String::from("e\u{0301}\u{0301}"),       // combining marks
            String::from("\u{202e}RTL\u{202d}"),     // bidi override
            String::from("\u{130}"),                 // LATIN CAPITAL I WITH DOT ABOVE
            String::from("\u{FFFD}\u{10FFFF}"),      // replacement + max scalar
            "[".repeat(10_000),                      // deeply "nested" junk
            "x".repeat(100_000),                     // huge
        ]
    }

    // Resolvers


    extern "C" fn div_resolver(
        _icon_data: OptionRefAny,
        _original_icon_node: &NodeData,
        _system_style: &SystemStyle,
    ) -> Dom {
        Dom::create_div()
    }

    // Statics for `shared_resolve_receives_icon_data_and_original_dom` ONLY.
    // (`extern "C" fn` cannot capture, and tests run in parallel — never share
    // one recording resolver between two tests.)
    static REC_CALLS: AtomicUsize = AtomicUsize::new(0);
    static REC_SAW_DATA: AtomicBool = AtomicBool::new(false);
    static REC_SAW_ICON_NODE: AtomicBool = AtomicBool::new(false);
    static REC_NAME_LEN: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn recording_resolver(
        icon_data: OptionRefAny,
        original_icon_node: &NodeData,
        _system_style: &SystemStyle,
    ) -> Dom {
        REC_CALLS.fetch_add(1, AtomicOrdering::SeqCst);
        if matches!(icon_data, OptionRefAny::Some(_)) {
            REC_SAW_DATA.store(true, AtomicOrdering::SeqCst);
        }
        if let NodeType::Icon(name) = original_icon_node.get_node_type() {
            REC_SAW_ICON_NODE.store(true, AtomicOrdering::SeqCst);
            REC_NAME_LEN.store(name.as_ref().as_str().len(), AtomicOrdering::SeqCst);
        }
        Dom::create_div()
    }

    // Mutex (the no_std spinlock under `no_std`, `std::sync::Mutex` otherwise)

    #[test]
    fn mutex_new_then_lock_roundtrips_the_value() {
        let m = Mutex::new(42u32);
        assert_eq!(*m.lock().unwrap(), 42);
        *m.lock().unwrap() = u32::MAX;
        assert_eq!(*m.lock().unwrap(), u32::MAX);
    }

    #[test]
    fn mutex_lock_on_empty_and_large_payloads() {
        let empty: Mutex<Vec<u8>> = Mutex::new(Vec::new());
        assert!(empty.lock().unwrap().is_empty());

        let big = Mutex::new(vec![0u8; 1_000_000]);
        assert_eq!(big.lock().unwrap().len(), 1_000_000);

        // Sequential re-lock must not deadlock (guard dropped at end of statement).
        for _ in 0..1_000 {
            assert!(big.lock().is_ok());
        }
    }

    // default_icon_resolver

    #[test]
    fn default_resolver_returns_a_single_empty_div_for_none_and_some() {
        let orig = Dom::create_div().root;
        let style = SystemStyle::default();

        // The core default resolver renders nothing; azul-layout installs the
        // real one that understands image / font / Dom icon data.
        let none = default_icon_resolver(OptionRefAny::None, &orig, &style);
        assert!(matches!(none.root.get_node_type(), NodeType::Div));
        assert!(none.children.as_ref().is_empty());

        let some = default_icon_resolver(
            OptionRefAny::Some(RefAny::new(TestIconData { id: 1 })),
            &orig,
            &style,
        );
        assert!(matches!(some.root.get_node_type(), NodeType::Div));
        assert!(some.children.as_ref().is_empty());
    }

    // IconProviderHandle: construction / invariants

    #[test]
    fn new_handle_is_empty_and_all_queries_are_negative() {
        let h = IconProviderHandle::new();
        assert!(h.list_packs().is_empty());
        assert!(h.list_icons_in_pack("anything").is_empty());
        assert!(!h.has_icon("home"));
        assert!(h.lookup("home").is_none());
        assert!(h.lookup_with_pack("home").is_none());
        assert!(h.debug_lookup("home").as_str().contains("Total packs: 0"));
    }

    #[test]
    fn default_handle_matches_new_handle() {
        let a = IconProviderHandle::default();
        let b = IconProviderHandle::new();
        assert_eq!(a.list_packs(), b.list_packs());
        assert_eq!(a.has_icon(""), b.has_icon(""));
    }

    #[test]
    fn with_resolver_installs_the_callback() {
        let mut h = IconProviderHandle::with_resolver(div_resolver);
        h.register_icon("p", "home", RefAny::new(TestIconData { id: 1 }));
        let shared = SharedIconProvider::from_handle(h);
        let out = shared.resolve(&Dom::create_div().root, "home", &SystemStyle::default());
        assert!(matches!(out.root.get_node_type(), NodeType::Div));
    }

    #[test]
    fn set_resolver_overrides_the_default_resolver() {
        let mut h = IconProviderHandle::new();
        h.set_resolver(div_resolver);
        let shared = SharedIconProvider::from_handle(h);
        // Unregistered icon: resolver still runs, just with `None` data.
        let out = shared.resolve(&Dom::create_div().root, "missing", &SystemStyle::default());
        assert!(matches!(out.root.get_node_type(), NodeType::Div));
    }

    #[test]
    fn clone_of_handle_is_deep() {
        let mut a = IconProviderHandle::new();
        a.register_icon("p", "home", RefAny::new(TestIconData { id: 1 }));

        let mut b = a.clone();
        b.register_icon("p", "settings", RefAny::new(TestIconData { id: 2 }));
        b.unregister_icon("p", "home");

        assert!(a.has_icon("home"));
        assert!(!a.has_icon("settings"));
        assert!(b.has_icon("settings"));
        assert!(!b.has_icon("home"));
    }

    #[test]
    fn drop_of_clones_and_originals_is_safe() {
        // Guards the ManuallyDrop / run_destructor convention (see the type's docs).
        let mut a = IconProviderHandle::new();
        a.register_icon("p", "home", RefAny::new(TestIconData { id: 1 }));
        for _ in 0..100 {
            let c = a.clone();
            drop(c);
        }
        assert!(a.has_icon("home"));
        drop(a);
    }

    // register / unregister

    #[test]
    fn register_icon_lowercases_icon_name_but_not_pack_name() {
        let mut h = IconProviderHandle::new();
        h.register_icon("MyPack", "HoMe", RefAny::new(TestIconData { id: 1 }));

        assert_eq!(h.list_packs(), vec![String::from("MyPack")]);
        assert!(h.list_icons_in_pack("MyPack").contains(&String::from("home")));
        // Pack name is case-sensitive:
        assert!(h.list_icons_in_pack("mypack").is_empty());
        // Icon name is not:
        assert!(h.has_icon("HOME"));
        assert!(h.has_icon("home"));
        assert!(h.has_icon("hOmE"));
    }

    #[test]
    fn registering_the_same_icon_twice_overwrites_instead_of_duplicating() {
        let mut h = IconProviderHandle::new();
        h.register_icon("p", "home", RefAny::new(TestIconData { id: 1 }));
        h.register_icon("p", "HOME", RefAny::new(TestIconData { id: 2 }));

        assert_eq!(h.list_icons_in_pack("p").len(), 1);
        let mut data = h.lookup("home").expect("icon must exist");
        assert_eq!(data.downcast_ref::<TestIconData>().unwrap().id, 2);
    }

    #[test]
    fn unregister_icon_drops_the_pack_once_it_is_empty() {
        let mut h = IconProviderHandle::new();
        h.register_icon("p", "home", RefAny::new(TestIconData { id: 1 }));
        h.register_icon("p", "settings", RefAny::new(TestIconData { id: 2 }));

        h.unregister_icon("p", "HOME"); // case-insensitive on the icon name
        assert_eq!(h.list_packs(), vec![String::from("p")]);
        assert!(!h.has_icon("home"));

        h.unregister_icon("p", "settings");
        assert!(h.list_packs().is_empty(), "pack must be pruned when empty");
    }

    #[test]
    fn unregister_of_unknown_pack_or_icon_is_a_no_op() {
        let mut h = IconProviderHandle::new();
        h.register_icon("p", "home", RefAny::new(TestIconData { id: 1 }));

        h.unregister_icon("nonexistent-pack", "home");
        h.unregister_icon("p", "nonexistent-icon");
        h.unregister_pack("nonexistent-pack");
        h.unregister_pack("");
        h.unregister_icon("", "");

        assert!(h.has_icon("home"));
        assert_eq!(h.list_packs().len(), 1);
    }

    #[test]
    fn unregister_pack_removes_all_of_its_icons() {
        let mut h = IconProviderHandle::new();
        h.register_icon("p", "a", RefAny::new(TestIconData { id: 1 }));
        h.register_icon("p", "b", RefAny::new(TestIconData { id: 2 }));
        h.register_icon("q", "a", RefAny::new(TestIconData { id: 3 }));

        h.unregister_pack("p");
        assert_eq!(h.list_packs(), vec![String::from("q")]);
        // "a" still resolvable via the other pack.
        assert!(h.has_icon("a"));
        assert!(!h.has_icon("b"));
    }

    #[test]
    fn adversarial_names_roundtrip_through_register_lookup_unregister() {
        for (i, name) in adversarial_names().iter().enumerate() {
            let mut h = IconProviderHandle::new();
            h.register_icon("p", name, RefAny::new(TestIconData { id: i as u32 }));

            assert!(h.has_icon(name), "has_icon failed for name #{i}");
            let mut data = h.lookup(name).unwrap_or_else(|| panic!("lookup failed for name #{i}"));
            assert_eq!(data.downcast_ref::<TestIconData>().unwrap().id, i as u32);

            h.unregister_icon("p", name);
            assert!(!h.has_icon(name), "unregister failed for name #{i}");
            assert!(h.list_packs().is_empty());
        }
    }

    #[test]
    fn empty_pack_name_and_empty_icon_name_are_legal_keys() {
        let mut h = IconProviderHandle::new();
        h.register_icon("", "", RefAny::new(TestIconData { id: 9 }));

        assert_eq!(h.list_packs(), vec![String::new()]);
        assert_eq!(h.list_icons_in_pack(""), vec![String::new()]);
        assert!(h.has_icon(""));
        let (pack, _) = h.lookup_with_pack("").expect("empty key must be found");
        assert_eq!(pack, "");
    }

    // lookup / lookup_with_pack (parser-shaped adversarial cases)

    #[test]
    fn lookup_empty_input_returns_none() {
        let mut h = IconProviderHandle::new();
        h.register_icon("p", "home", RefAny::new(TestIconData { id: 1 }));
        assert!(h.lookup("").is_none());
        assert!(h.lookup_with_pack("").is_none());
        assert!(!h.has_icon(""));
    }

    #[test]
    fn lookup_whitespace_only_is_not_trimmed() {
        let mut h = IconProviderHandle::new();
        h.register_icon("p", "home", RefAny::new(TestIconData { id: 1 }));
        for ws in ["   ", "\t\n", "\r", "\u{a0}"] {
            assert!(h.lookup(ws).is_none(), "{ws:?} must not match");
        }
        // ...and a whitespace-only *registered* name matches only itself, verbatim.
        h.register_icon("p", "   ", RefAny::new(TestIconData { id: 2 }));
        assert!(h.lookup("   ").is_some());
        assert!(h.lookup(" ").is_none());
        assert!(h.lookup("").is_none());
    }

    #[test]
    fn lookup_garbage_returns_none_but_spec_whitespace_is_tolerated() {
        let mut h = IconProviderHandle::new();
        h.register_icon("p", "home", RefAny::new(TestIconData { id: 1 }));

        // Genuine garbage must never match 'home'.
        for junk in [
            "home;garbage",
            "home\0",
            "\0home",
            "ho\nme",
            "home/../../etc/passwd",
            "\u{1b}[31mhome\u{1b}[0m",
            "{\"icon\":\"home\"}",
        ] {
            assert!(h.lookup(junk).is_none(), "{junk:?} must not match 'home'");
            assert!(!h.has_icon(junk));
        }

        // Spec normalization: surrounding whitespace is trimmed per entry —
        // `<icon> home </icon>` markup must resolve (ligature-font model).
        for spec in [" home ", "home ", " home"] {
            assert!(h.lookup(spec).is_some(), "{spec:?} must resolve via spec trim");
            assert!(h.has_icon(spec));
        }
        assert!(h.lookup("home").is_some(), "positive control");
    }

    #[test]
    fn lookup_of_extremely_long_name_terminates_and_matches_exactly() {
        let mut h = IconProviderHandle::new();
        let long = "x".repeat(1_000_000);
        h.register_icon("p", &long, RefAny::new(TestIconData { id: 7 }));

        assert!(h.lookup(&long).is_some());
        assert!(h.has_icon(&long));
        // One char shorter -> no match, still no panic/hang.
        assert!(h.lookup(&"x".repeat(999_999)).is_none());
        // A 1M-char miss against a small map.
        let mut h2 = IconProviderHandle::new();
        h2.register_icon("p", "home", RefAny::new(TestIconData { id: 1 }));
        assert!(h2.lookup(&long).is_none());
    }

    #[test]
    fn lookup_of_boundary_numeric_strings_is_deterministic() {
        let mut h = IconProviderHandle::new();
        for (i, n) in ["0", "-0", "9223372036854775807", "-9223372036854775808", "nan", "inf"]
            .iter()
            .enumerate()
        {
            h.register_icon("p", n, RefAny::new(TestIconData { id: i as u32 }));
        }
        // Numeric-looking names are plain string keys: no numeric parsing, no coercion.
        assert!(h.lookup("0").is_some());
        assert!(h.lookup("-0").is_some());
        assert!(h.lookup("0.0").is_none());
        assert!(h.lookup("00").is_none());
        assert!(h.lookup("+0").is_none());
        assert!(h.lookup("9223372036854775808").is_none()); // i64::MAX + 1
        // ...but case folding still applies.
        assert!(h.lookup("NaN").is_some());
        assert!(h.lookup("INF").is_some());
    }

    #[test]
    fn lookup_of_unicode_names_folds_case_without_panicking() {
        let mut h = IconProviderHandle::new();
        h.register_icon("p", "\u{1F600}", RefAny::new(TestIconData { id: 1 }));
        h.register_icon("p", "\u{C4}", RefAny::new(TestIconData { id: 2 })); // Ä
        h.register_icon("p", "I", RefAny::new(TestIconData { id: 3 }));

        assert!(h.lookup("\u{1F600}").is_some(), "emoji key must round-trip");
        assert!(h.lookup("\u{E4}").is_some(), "ä must match registered Ä");
        assert!(h.lookup("i").is_some(), "I folds to i");

        // `str::to_lowercase` is full-Unicode: "İ" (U+0130) folds to TWO scalars
        // ("i" + U+0307), so the *stored key is not the registered string*.
        let mut h2 = IconProviderHandle::new();
        h2.register_icon("p", "\u{130}", RefAny::new(TestIconData { id: 4 }));
        assert!(h2.lookup("\u{130}").is_some(), "self-lookup must still work");
        let keys = h2.list_icons_in_pack("p");
        assert_eq!(keys, vec![String::from("\u{130}").to_lowercase()]);
        assert!(!keys.contains(&String::from("\u{130}")), "key is stored folded, not verbatim");
        assert!(h2.lookup("i").is_none(), "the bare ASCII 'i' must not match İ");
    }

    #[test]
    fn lookup_of_deeply_nested_input_does_not_stack_overflow() {
        let h = IconProviderHandle::new();
        // Lookup is a map probe, not a recursive-descent parse: depth is irrelevant,
        // but assert it explicitly so a future parsing implementation stays flat.
        for depth in [1_000usize, 10_000, 100_000] {
            let nested = "[".repeat(depth);
            assert!(h.lookup(&nested).is_none());
            assert!(!h.has_icon(&nested));
            assert!(h.debug_lookup(&nested).as_str().contains("NOT FOUND"));
        }
    }

    #[test]
    fn lookup_valid_minimal_positive_control_roundtrips_the_payload() {
        let mut h = IconProviderHandle::new();
        h.register_icon("p", "a", RefAny::new(TestIconData { id: 123 }));

        let mut data = h.lookup("a").expect("registered icon must be found");
        assert_eq!(*data.downcast_ref::<TestIconData>().unwrap(), TestIconData { id: 123 });
        // Wrong-type downcast must fail rather than reinterpret the bytes.
        assert!(data.downcast_ref::<u64>().is_none());
    }

    #[test]
    fn lookup_with_pack_first_match_is_the_lexicographically_first_pack() {
        let mut h = IconProviderHandle::new();
        // Register in reverse-alphabetical order: insertion order must NOT decide.
        h.register_icon("zzz", "home", RefAny::new(TestIconData { id: 26 }));
        h.register_icon("mmm", "home", RefAny::new(TestIconData { id: 13 }));
        h.register_icon("aaa", "home", RefAny::new(TestIconData { id: 1 }));

        let (pack, _) = h.lookup_with_pack("HOME").expect("must be found");
        assert_eq!(pack, "aaa", "BTreeMap order => first match is the first pack by name");

        let mut data = h.lookup("home").unwrap();
        assert_eq!(data.downcast_ref::<TestIconData>().unwrap().id, 1);

        // Removing the winner promotes the next pack in order.
        h.unregister_pack("aaa");
        let (pack, _) = h.lookup_with_pack("home").unwrap();
        assert_eq!(pack, "mmm");
    }

    // has_icon

    #[test]
    fn has_icon_true_false_and_edge_inputs() {
        let mut h = IconProviderHandle::new();
        h.register_icon("p", "home", RefAny::new(TestIconData { id: 1 }));

        assert!(h.has_icon("home"));
        assert!(!h.has_icon("definitely-not-registered"));

        for name in adversarial_names() {
            // Deterministic bool, no panic: none of these were registered.
            assert!(!h.has_icon(&name));
        }
        assert!(h.has_icon("home"), "state unchanged by the queries above");
    }

    // getters: list_packs / list_icons_in_pack

    #[test]
    fn list_packs_is_sorted_and_case_sensitive() {
        let mut h = IconProviderHandle::new();
        for p in ["zeta", "alpha", "Alpha", "mid", ""] {
            h.register_icon(p, "home", RefAny::new(TestIconData { id: 0 }));
        }
        // BTreeMap => byte-order sorted; "Alpha" != "alpha" (case-sensitive).
        assert_eq!(
            h.list_packs(),
            vec![
                String::new(),
                String::from("Alpha"),
                String::from("alpha"),
                String::from("mid"),
                String::from("zeta"),
            ]
        );
    }

    #[test]
    fn list_icons_in_pack_returns_folded_keys_and_empty_for_unknown_packs() {
        let mut h = IconProviderHandle::new();
        h.register_icon("p", "Zoom", RefAny::new(TestIconData { id: 1 }));
        h.register_icon("p", "HOME", RefAny::new(TestIconData { id: 2 }));

        assert_eq!(h.list_icons_in_pack("p"), vec![String::from("home"), String::from("zoom")]);
        assert!(h.list_icons_in_pack("P").is_empty());
        assert!(h.list_icons_in_pack("").is_empty());
        assert!(h.list_icons_in_pack(&"x".repeat(100_000)).is_empty());
    }

    // debug_lookup

    #[test]
    fn debug_lookup_reports_not_found_for_missing_icons() {
        let mut h = IconProviderHandle::new();
        h.register_icon("p", "home", RefAny::new(TestIconData { id: 1 }));

        let out = h.debug_lookup("settings");
        let s = out.as_str();
        assert!(s.contains("NOT FOUND in any pack"));
        assert!(s.contains("Total packs: 1"));
        assert!(s.contains("Pack 'p': 1 icons"));
    }

    #[test]
    fn debug_lookup_classifies_image_font_and_unknown_refany_types() {
        let mut h = IconProviderHandle::new();
        h.register_icon("p", "img", RefAny::new(ImageIconData { _w: 16 }));
        h.register_icon("p", "fnt", RefAny::new(FontIconData { _codepoint: 0xF015 }));
        h.register_icon("p", "other", RefAny::new(TestIconData { id: 1 }));

        let img = h.debug_lookup("img");
        assert!(img.as_str().contains("FOUND in pack 'p'"));
        assert!(img.as_str().contains("RefAny type: ImageIconData"));

        let fnt = h.debug_lookup("FNT"); // case-folded lookup path
        assert!(fnt.as_str().contains("RefAny type: FontIconData"));

        let other = h.debug_lookup("other");
        assert!(other.as_str().contains("RefAny type: UNKNOWN"));
    }

    #[test]
    fn debug_lookup_survives_adversarial_names() {
        let mut h = IconProviderHandle::new();
        for (i, name) in adversarial_names().iter().enumerate() {
            h.register_icon("p", name, RefAny::new(TestIconData { id: i as u32 }));
        }
        for name in adversarial_names() {
            let out = h.debug_lookup(&name);
            assert!(out.as_str().contains("FOUND in pack 'p'"), "must find {name:?}");
        }
        assert!(h.debug_lookup("never-registered").as_str().contains("NOT FOUND"));
    }

    // SharedIconProvider

    #[test]
    fn from_handle_preserves_every_registered_icon() {
        let mut h = IconProviderHandle::new();
        for i in 0..64u32 {
            h.register_icon("p", &format!("icon{i}"), RefAny::new(TestIconData { id: i }));
        }
        let shared = SharedIconProvider::from_handle(h);

        for i in 0..64u32 {
            let name = format!("ICON{i}");
            assert!(shared.has_icon(&name));
            let mut data = shared.lookup(&name).expect("must survive into_shared()");
            assert_eq!(data.downcast_ref::<TestIconData>().unwrap().id, i);
        }
        assert!(!shared.has_icon("icon64"));
        assert!(shared.lookup("").is_none());
    }

    #[test]
    fn shared_provider_lookup_and_has_icon_agree_on_adversarial_input() {
        let mut h = IconProviderHandle::new();
        h.register_icon("p", "home", RefAny::new(TestIconData { id: 1 }));
        let shared = SharedIconProvider::from_handle(h);

        for name in adversarial_names() {
            assert_eq!(
                shared.has_icon(&name),
                shared.lookup(&name).is_some(),
                "has_icon/lookup disagree for {name:?}"
            );
        }
        assert!(shared.has_icon("HoMe") && shared.lookup("HoMe").is_some());
    }

    #[test]
    fn shared_provider_clone_shares_the_same_icon_table() {
        let mut h = IconProviderHandle::new();
        h.register_icon("p", "home", RefAny::new(TestIconData { id: 1 }));
        let a = SharedIconProvider::from_handle(h);
        let b = a.clone();

        assert!(b.has_icon("home"));
        drop(a);
        assert!(b.has_icon("home"), "clone must keep the Arc alive");
        let mut data = b.lookup("home").unwrap();
        assert_eq!(data.downcast_ref::<TestIconData>().unwrap().id, 1);
    }

    #[test]
    fn shared_resolve_receives_icon_data_and_original_dom() {
        let mut h = IconProviderHandle::with_resolver(recording_resolver);
        h.register_icon("p", "home", RefAny::new(TestIconData { id: 1 }));
        let shared = SharedIconProvider::from_handle(h);

        // The resolver is handed the ICON NODE itself now, not a one-node
        // StyledDom carved out of the tree.
        let icon_node = Dom::create_icon("home").root;

        let out = shared.resolve(&icon_node, "HOME", &SystemStyle::default());

        assert!(REC_CALLS.load(AtomicOrdering::SeqCst) >= 1);
        assert!(REC_SAW_DATA.load(AtomicOrdering::SeqCst), "case-folded lookup must pass Some(data)");
        assert!(REC_SAW_ICON_NODE.load(AtomicOrdering::SeqCst), "the resolver must receive the Icon node");
        assert_eq!(REC_NAME_LEN.load(AtomicOrdering::SeqCst), "home".len());
        assert!(matches!(out.root.get_node_type(), NodeType::Div));
    }

    #[test]
    fn shared_resolve_runs_the_resolver_even_when_the_icon_is_missing() {
        let h = IconProviderHandle::with_resolver(div_resolver);
        let shared = SharedIconProvider::from_handle(h);
        // Empty name, huge name, unicode name: resolver still returns its DOM.
        let huge = "x".repeat(100_000);
        for name in ["", "\u{1F600}", huge.as_str()] {
            let out = shared.resolve(&Dom::create_div().root, name, &SystemStyle::default());
            assert!(matches!(out.root.get_node_type(), NodeType::Div));
        }
    }

    #[cfg(feature = "std")]
    #[test]
    fn shared_provider_survives_concurrent_lookups() {
        let mut h = IconProviderHandle::new();
        for i in 0..16u32 {
            h.register_icon("p", &format!("icon{i}"), RefAny::new(TestIconData { id: i }));
        }
        let shared = SharedIconProvider::from_handle(h);

        let mut threads = Vec::new();
        for _ in 0..4 {
            let s = shared.clone();
            threads.push(std::thread::spawn(move || {
                let mut hits = 0usize;
                for i in 0..200u32 {
                    let name = format!("icon{}", i % 16);
                    if s.has_icon(&name) {
                        hits += 1;
                    }
                    let mut data = s.lookup(&name).expect("registered icon");
                    assert_eq!(data.downcast_ref::<TestIconData>().unwrap().id, i % 16);
                }
                hits
            }));
        }
        for t in threads {
            assert_eq!(t.join().unwrap(), 200);
        }
        assert!(shared.has_icon("icon0"), "table intact after contention");
    }

    // resolve_icons_in_dom (end to end)

    /// Every icon node in the tree, by the path of child indices that reaches it.
    fn icon_paths(dom: &Dom) -> Vec<Vec<usize>> {
        fn walk(d: &Dom, path: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
            if matches!(d.root.get_node_type(), NodeType::Icon(_)) {
                out.push(path.clone());
            }
            for (i, c) in d.children.as_ref().iter().enumerate() {
                path.push(i);
                walk(c, path, out);
                path.pop();
            }
        }
        let mut out = Vec::new();
        walk(dom, &mut Vec::new(), &mut out);
        out
    }

    /// The node at a child path.
    fn node_at<'a>(dom: &'a Dom, path: &[usize]) -> &'a Dom {
        let mut cur = dom;
        for &i in path {
            cur = &cur.children.as_ref()[i];
        }
        cur
    }

    fn dom_with_icons(names: &[&str]) -> Dom {
        let mut body = Dom::create_body();
        for n in names {
            body = body.with_child(Dom::create_icon(AzString::from(*n)));
        }
        body
    }

    #[test]
    fn resolve_icons_in_dom_is_a_no_op_without_icons() {
        let shared = SharedIconProvider::from_handle(IconProviderHandle::with_resolver(div_resolver));
        let mut dom = Dom::create_body()
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("hi"));
        let before = dom.clone();

        resolve_icons_in_dom(&mut dom, &shared, &SystemStyle::default());

        assert_eq!(dom.children.as_ref().len(), before.children.as_ref().len());
        assert_eq!(dom.root.get_node_type(), before.root.get_node_type());
    }

    #[test]
    fn resolve_icons_in_dom_replaces_every_icon_case_insensitively() {
        let mut h = IconProviderHandle::with_resolver(div_resolver);
        h.register_icon("p", "home", RefAny::new(TestIconData { id: 1 }));
        let shared = SharedIconProvider::from_handle(h);

        // Mixed case in the DOM, lowercase in the pack, plus one unregistered icon.
        let mut dom = dom_with_icons(&["HOME", "unregistered", "HoMe"]);
        let paths = icon_paths(&dom);
        assert_eq!(paths.len(), 3);

        resolve_icons_in_dom(&mut dom, &shared, &SystemStyle::default());

        assert!(icon_paths(&dom).is_empty(), "no Icon node may survive resolution");
        for p in paths {
            assert!(matches!(node_at(&dom, &p).root.get_node_type(), NodeType::Div));
        }
    }

    #[test]
    fn resolve_icons_in_dom_with_the_default_resolver_removes_the_icon_nodes() {
        // The default resolver returns an empty div, so an icon with no
        // registered data becomes that rather than staying an Icon node.
        let shared = SharedIconProvider::from_handle(IconProviderHandle::new());
        let mut dom = dom_with_icons(&["home"]);

        resolve_icons_in_dom(&mut dom, &shared, &SystemStyle::default());

        assert!(icon_paths(&dom).is_empty());
    }

    /// The whole point of resolving on the tree: a replacement may be MORE than
    /// one node, and all of it survives. The old StyledDom splice flattened
    /// every replacement to its root plus a single glyph character, which is
    /// why registering a `Dom` as an icon was impossible.
    #[test]
    fn a_multi_node_replacement_is_spliced_in_whole() {
        extern "C" fn subtree_resolver(
            _d: OptionRefAny,
            _n: &NodeData,
            _s: &SystemStyle,
        ) -> Dom {
            Dom::create_div()
                .with_child(Dom::create_div())
                .with_child(Dom::create_div())
        }

        let shared =
            SharedIconProvider::from_handle(IconProviderHandle::with_resolver(subtree_resolver));
        let mut dom = dom_with_icons(&["home"]);
        let path = icon_paths(&dom)[0].clone();

        resolve_icons_in_dom(&mut dom, &shared, &SystemStyle::default());

        let replaced = node_at(&dom, &path);
        assert!(matches!(replaced.root.get_node_type(), NodeType::Div));
        assert_eq!(
            replaced.children.as_ref().len(),
            2,
            "both children of the replacement must survive the splice"
        );
    }

    /// REGRESSION: an icon that resolves to ANOTHER icon must keep the
    /// stylesheets attached along the way.
    ///
    /// `Dom::with_css` attaches a scoped stylesheet to `Dom::css`, NOT inline
    /// properties - so `Dom::create_icon("favorite").with_css("color: red")`,
    /// the natural way to register a recoloured icon, carries the colour in
    /// `css`. Replacing the node wholesale dropped it, and the icon rendered in
    /// the default colour while every step reported success.
    ///
    /// This is the mechanism that lets an icon carry its own appearance instead
    /// of every call site threading a tint parameter down to the renderer.
    #[test]
    fn stylesheets_survive_an_icon_resolving_to_another_icon() {
        extern "C" fn to_div(_d: OptionRefAny, _n: &NodeData, _s: &SystemStyle) -> Dom {
            Dom::create_div()
        }

        let shared = SharedIconProvider::from_handle(IconProviderHandle::with_resolver(to_div));
        let mut dom = Dom::create_body().with_child(
            Dom::create_icon("outer").with_css("color: #d7263d;"),
        );

        resolve_icons_in_dom(&mut dom, &shared, &SystemStyle::default());

        let resolved = &dom.children.as_ref()[0];
        assert!(matches!(resolved.root.get_node_type(), NodeType::Div));
        assert!(
            !resolved.css.as_ref().is_empty(),
            "the icon's own stylesheet must survive resolution; losing it is \
             how a styled icon silently renders unstyled"
        );
    }

    #[test]
    fn resolve_icons_in_dom_scales_to_many_icons() {
        let shared = SharedIconProvider::from_handle(IconProviderHandle::with_resolver(div_resolver));
        let names: Vec<String> = (0..500).map(|i| format!("icon{i}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut dom = dom_with_icons(&refs);
        let before_children = dom.children.as_ref().len();

        resolve_icons_in_dom(&mut dom, &shared, &SystemStyle::default());

        assert_eq!(dom.children.as_ref().len(), before_children);
        assert!(icon_paths(&dom).is_empty());
    }
}

/// Tests for the resolution CACHE — the reproduction of the per-regeneration
/// waste (RSS_MAP_2026_08_07.md §36c) and the properties of the fix.
///
/// The engine calls `resolve_icons_in_styled_dom` once per DOM regeneration on
/// a FRESH StyledDom each time (the layout callback rebuilds it), so "two
/// frames" here means two identically-built DOMs — exactly what a drag-resize
/// produces 373 times in five seconds.
#[cfg(test)]
#[allow(clippy::float_cmp)]
mod icon_cache_tests {
    use core::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};

    use super::*;
    use crate::refany::RefAny;

    #[derive(Debug, Clone, PartialEq)]
    struct CacheTestIconData {
        id: u32,
    }

    /// A body whose direct children are the named icons, so child index N is
    /// icon N both before and after resolution.
    fn dom_with_icons(names: &[&str]) -> Dom {
        let mut body = Dom::create_body();
        for n in names {
            body.add_child(Dom::create_icon(*n));
        }
        body
    }

    // Per-test statics: `extern "C" fn` cannot capture, and tests run in
    // parallel — never share one counter between two tests (same convention
    // as `autotest_generated::REC_*`).

    static FRAME_CALLS: AtomicUsize = AtomicUsize::new(0);
    extern "C" fn frame_counting_resolver(
        _icon_data: OptionRefAny,
        _original: &NodeData,
        _style: &SystemStyle,
    ) -> Dom {
        FRAME_CALLS.fetch_add(1, AtomicOrdering::SeqCst);
        Dom::create_div()
    }

    /// THE REPRODUCTION. Before the cache, this counted one resolver call per
    /// icon per frame — 3 icons × 2 frames = 6 calls for six bit-identical
    /// results (scaled up in production: 66 icons × 373 regenerations in one
    /// measured drag ≈ 24 600 calls, each running a full throwaway single-node
    /// cascade). With the cache: ONE call, ever, for identical inputs.
    #[test]
    fn identical_icons_across_frames_resolve_exactly_once() {
        let mut h = IconProviderHandle::with_resolver(frame_counting_resolver);
        h.register_icon("p", "home", RefAny::new(CacheTestIconData { id: 1 }));
        let shared = SharedIconProvider::from_handle(h);
        let style = SystemStyle::default();

        for frame in 0..3 {
            let mut sd = dom_with_icons(&["home", "home", "home"]);
            resolve_icons_in_dom(&mut sd, &shared, &style);
            for child in sd.children.as_ref() {
                assert!(
                    matches!(child.root.get_node_type(), NodeType::Div),
                    "frame {frame}: icon must be resolved on the cached path too"
                );
            }
        }

        assert_eq!(
            FRAME_CALLS.load(AtomicOrdering::SeqCst),
            1,
            "identical (spec, node, styled-state) must hit the cache — both \
             across frames AND across duplicates within one frame"
        );
    }

    static STYLE_VARIANT_CALLS: AtomicUsize = AtomicUsize::new(0);
    extern "C" fn style_variant_resolver(
        _icon_data: OptionRefAny,
        _original: &NodeData,
        _style: &SystemStyle,
    ) -> Dom {
        STYLE_VARIANT_CALLS.fetch_add(1, AtomicOrdering::SeqCst);
        Dom::create_div()
    }

    /// The resolver copies inline styles off the original node, so the same
    /// icon NAME with different inline styles is a different resolution and
    /// must occupy a different cache entry.
    #[test]
    fn distinct_inline_styles_are_distinct_cache_entries() {
        use azul_css::dynamic_selector::CssPropertyWithConditions;
        use azul_css::props::layout::dimensions::LayoutWidth;
        use azul_css::props::property::CssProperty;

        let shared = SharedIconProvider::from_handle(IconProviderHandle::with_resolver(
            style_variant_resolver,
        ));
        let style = SystemStyle::default();

        let build = || {
            let mut body = Dom::create_body();
            body.add_child(Dom::create_icon("home"));
            body.add_child(Dom::create_icon("home").with_css_props(
                vec![CssPropertyWithConditions::simple(CssProperty::width(
                    LayoutWidth::px(24.0),
                ))]
                .into(),
            ));
            body
        };

        let mut frame1 = build();
        resolve_icons_in_dom(&mut frame1, &shared, &style);
        assert_eq!(
            STYLE_VARIANT_CALLS.load(AtomicOrdering::SeqCst),
            2,
            "same name, different inline style => two resolutions"
        );

        let mut frame2 = build();
        resolve_icons_in_dom(&mut frame2, &shared, &style);
        assert_eq!(
            STYLE_VARIANT_CALLS.load(AtomicOrdering::SeqCst),
            2,
            "both variants must be cache hits on the second frame"
        );
    }

    static SYS_STYLE_CALLS: AtomicUsize = AtomicUsize::new(0);
    extern "C" fn sys_style_resolver(
        _icon_data: OptionRefAny,
        _original: &NodeData,
        _style: &SystemStyle,
    ) -> Dom {
        SYS_STYLE_CALLS.fetch_add(1, AtomicOrdering::SeqCst);
        Dom::create_div()
    }

    /// Resolvers read the SystemStyle (theme, tint, grayscale), so a style
    /// change must flush. The policy is flush-on-change, not per-style keying:
    /// flipping BACK re-resolves too. That trade is deliberate — a style flip
    /// is a rare, user-visible event; keying every entry by style would bloat
    /// every comparison for it.
    #[test]
    fn system_style_change_flushes_the_cache() {
        let shared =
            SharedIconProvider::from_handle(IconProviderHandle::with_resolver(sys_style_resolver));
        let style_a = SystemStyle::default();
        let mut style_b = SystemStyle::default();
        style_b.language = azul_css::AzString::from("xx-ZZ");
        assert_ne!(style_a, style_b);

        let mut sd = dom_with_icons(&["home"]);
        resolve_icons_in_dom(&mut sd, &shared, &style_a);
        assert_eq!(SYS_STYLE_CALLS.load(AtomicOrdering::SeqCst), 1);

        let mut sd = dom_with_icons(&["home"]);
        resolve_icons_in_dom(&mut sd, &shared, &style_b);
        assert_eq!(SYS_STYLE_CALLS.load(AtomicOrdering::SeqCst), 2, "style change => re-resolve");

        let mut sd = dom_with_icons(&["home"]);
        resolve_icons_in_dom(&mut sd, &shared, &style_a);
        assert_eq!(
            SYS_STYLE_CALLS.load(AtomicOrdering::SeqCst),
            3,
            "flush-on-change: flipping back re-resolves (documented policy)"
        );
    }

    static PARITY_CALLS: AtomicUsize = AtomicUsize::new(0);
    extern "C" fn parity_resolver(
        _icon_data: OptionRefAny,
        original: &NodeData,
        _style: &SystemStyle,
    ) -> Dom {
        use azul_css::dynamic_selector::CssPropertyWithConditions;
        use azul_css::props::layout::dimensions::LayoutWidth;
        use azul_css::props::property::CssProperty;
        PARITY_CALLS.fetch_add(1, AtomicOrdering::SeqCst);
        // A realistic replacement: styled text (what a font-icon resolver
        // produces), reading nothing but producing node type + props + a11y.
        let mut dom = Dom::create_text_do_not_use_without_block_level_wrapper("\u{e88a}");
        dom.root.set_css_props(
            vec![CssPropertyWithConditions::simple(CssProperty::width(LayoutWidth::px(16.0)))]
                .into(),
        );
        if let Some(a11y) = original.get_accessibility_info() {
            dom = dom.with_accessibility_info(a11y.clone());
        }
        dom
    }

    /// A cache hit must produce a node BIT-IDENTICAL to what the fresh
    /// resolver produced on frame 1 — node type, inline style, the lot.
    #[test]
    fn cached_hit_produces_an_identical_node() {
        let shared =
            SharedIconProvider::from_handle(IconProviderHandle::with_resolver(parity_resolver));
        let style = SystemStyle::default();

        let mut frame1 = dom_with_icons(&["save"]);
        resolve_icons_in_dom(&mut frame1, &shared, &style);
        assert_eq!(PARITY_CALLS.load(AtomicOrdering::SeqCst), 1);

        let mut frame2 = dom_with_icons(&["save"]);
        resolve_icons_in_dom(&mut frame2, &shared, &style);
        assert_eq!(PARITY_CALLS.load(AtomicOrdering::SeqCst), 1, "frame 2 must be a hit");

        assert_eq!(
            frame1.children.as_ref()[0].root,
            frame2.children.as_ref()[0].root,
            "cached and freshly-resolved node must be indistinguishable"
        );
    }

    static EMPTY_CALLS: AtomicUsize = AtomicUsize::new(0);
    extern "C" fn empty_resolver(
        _icon_data: OptionRefAny,
        _original: &NodeData,
        _style: &SystemStyle,
    ) -> Dom {
        EMPTY_CALLS.fetch_add(1, AtomicOrdering::SeqCst);
        Dom::create_div()
    }

    /// "Icon not found → empty div" is also a resolution and is also cached.
    #[test]
    fn empty_resolutions_are_cached_too() {
        let shared =
            SharedIconProvider::from_handle(IconProviderHandle::with_resolver(empty_resolver));
        let style = SystemStyle::default();

        for _ in 0..2 {
            let mut sd = dom_with_icons(&["missing"]);
            resolve_icons_in_dom(&mut sd, &shared, &style);
            assert!(matches!(
                sd.children.as_ref()[0].root.get_node_type(),
                NodeType::Div
            ));
        }
        assert_eq!(EMPTY_CALLS.load(AtomicOrdering::SeqCst), 1);
    }

    static SUBTREE_CALLS: AtomicUsize = AtomicUsize::new(0);
    extern "C" fn subtree_resolver(
        _icon_data: OptionRefAny,
        _original: &NodeData,
        _style: &SystemStyle,
    ) -> Dom {
        SUBTREE_CALLS.fetch_add(1, AtomicOrdering::SeqCst);
        Dom::create_body()
            .with_child(Dom::create_div())
            .with_child(Dom::create_text_do_not_use_without_block_level_wrapper("x"))
    }

    /// Multi-node replacements go through the same cache, stored as a whole
    /// `Dom` and cloned per hit - and, unlike the old arena splice, they are
    /// spliced in ENTIRELY, children and all.
    #[test]
    fn subtree_resolutions_are_cached() {
        let shared =
            SharedIconProvider::from_handle(IconProviderHandle::with_resolver(subtree_resolver));
        let style = SystemStyle::default();

        let mut first_type = None;
        for _ in 0..2 {
            let mut sd = dom_with_icons(&["multi"]);
            resolve_icons_in_dom(&mut sd, &shared, &style);
            let replaced = &sd.children.as_ref()[0];
            assert_eq!(
                replaced.children.as_ref().len(),
                2,
                "the cached subtree keeps BOTH children, not just its root"
            );
            let t = replaced.root.get_node_type().clone();
            match &first_type {
                None => first_type = Some(t),
                Some(prev) => assert_eq!(prev, &t, "cached subtree must apply identically"),
            }
        }
        assert_eq!(SUBTREE_CALLS.load(AtomicOrdering::SeqCst), 1);
    }

    static CAP_CALLS: AtomicUsize = AtomicUsize::new(0);
    extern "C" fn cap_resolver(
        _icon_data: OptionRefAny,
        _original: &NodeData,
        _style: &SystemStyle,
    ) -> Dom {
        CAP_CALLS.fetch_add(1, AtomicOrdering::SeqCst);
        Dom::create_div()
    }

    /// Unbounded distinct specs must not grow the cache without limit; the
    /// flush-all overflow policy degrades to uncached behaviour, never to
    /// unbounded memory.
    #[test]
    fn cache_is_capped_and_correct_past_the_cap() {
        let shared =
            SharedIconProvider::from_handle(IconProviderHandle::with_resolver(cap_resolver));
        let style = SystemStyle::default();

        let names: Vec<String> = (0..(ICON_CACHE_CAP + 100)).map(|i| format!("icon{i}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut sd = dom_with_icons(&refs);
        resolve_icons_in_dom(&mut sd, &shared, &style);

        assert_eq!(CAP_CALLS.load(AtomicOrdering::SeqCst), ICON_CACHE_CAP + 100);
        assert!(
            sd.children
                .as_ref()
                .iter()
                .all(|c| !matches!(c.root.get_node_type(), NodeType::Icon(_))),
            "every icon still resolved"
        );

        let cache = shared.cache.lock().unwrap();
        assert!(
            cache.total <= ICON_CACHE_CAP,
            "cap must hold: total={} cap={}",
            cache.total,
            ICON_CACHE_CAP
        );
    }
}
