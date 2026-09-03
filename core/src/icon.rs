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

use azul_css::{system::SystemStyle, AzString};

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
#[must_use]
pub extern "C" fn default_icon_resolver(
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
    #[must_use]
    pub fn lookup_spec(&self, spec: &str) -> Option<RefAny> {
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
    #[must_use]
    pub fn new() -> Self {
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
        let pack = self.inner.icons.entry(pack_name.to_string()).or_default();
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
            let pack = inner.icons.entry(pack_name.to_string()).or_default();
            pack.insert(icon_name.to_lowercase(), data);
        }
        if let Ok(mut cache) = self.cache.lock() {
            cache.entries.clear();
            cache.total = 0;
            // The next batch re-validates against whatever style it carries.
            cache.system_style = None;
        }
    }

    /// Flush the cache if `system_style` differs from the one its entries
    /// were resolved under. Called ONCE per `resolve_icons_in_styled_dom`
    /// batch, not per icon, so the `SystemStyle` comparison is per-frame.
    fn validate_cache_for_style(&self, system_style: &SystemStyle) {
        let Ok(mut cache) = self.cache.lock() else {
            return;
        };
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

    /// Resolve an icon to a `StyledDom` using the registered callback
    #[must_use]
    pub fn resolve(
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
    #[must_use]
    fn resolve_cached(
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

/// Resolve every `<icon>` in a user `Dom` and cascade it - the two halves of
/// "a `Dom` the application handed us becomes a `StyledDom`", as one call.
///
/// The halves were separate, and that is exactly how a path came to skip one:
/// three call sites ran `resolve_icons_in_dom` and then
/// `StyledDom::create_from_dom`, while a fourth - the DOM a VirtualView
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
    mut dom: Dom,
    provider: &SharedIconProvider,
    system_style: &SystemStyle,
) -> StyledDom {
    resolve_icons_in_dom(&mut dom, provider, system_style);
    StyledDom::create_from_dom(dom)
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

impl_option!(IconProviderHandle, OptionIconProviderHandle, [Clone]);

#[cfg(test)]
#[path = "icon_test.rs"]
mod icon_test;
