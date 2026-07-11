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
    dom::{Dom, NodeType},
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
    original_icon_dom: &StyledDom,
    system_style: &SystemStyle,
) -> StyledDom;

/// Default resolver that returns an empty `StyledDom` (shows placeholder)
#[must_use] pub extern "C" fn default_icon_resolver(
    _icon_data: OptionRefAny,
    _original_icon_dom: &StyledDom,
    _system_style: &SystemStyle,
) -> StyledDom {
    // Default: return empty DOM (icon won't be visible)
    StyledDom::default()
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

    /// Look up an icon across all packs (first match wins)
    #[must_use] pub fn lookup(&self, icon_name: &str) -> Option<RefAny> {
        self.lookup_with_pack(icon_name).map(|(_, data)| data.clone())
    }

    /// Check if an icon exists in any pack
    #[must_use] pub fn has_icon(&self, icon_name: &str) -> bool {
        let icon_name_lower = icon_name.to_lowercase();
        self.inner.icons.values().any(|p| p.contains_key(&icon_name_lower))
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
}

impl SharedIconProvider {
    /// Create from an `IconProviderHandle` (consumes the handle)
    #[must_use] pub fn from_handle(handle: IconProviderHandle) -> Self {
        Self { inner: handle.into_shared() }
    }
    
    /// Resolve an icon to a `StyledDom` using the registered callback
    #[must_use] pub fn resolve(
        &self, 
        original_icon_dom: &StyledDom,
        icon_name: &str,
        system_style: &SystemStyle,
    ) -> StyledDom {
        let (resolver, lookup_result) = {
            let Ok(guard) = self.inner.lock() else {
                return StyledDom::default();
            };
            
            let resolver = guard.resolver;
            let icon_name_lower = icon_name.to_lowercase();
            
            let lookup_result = guard.icons.values()
                .find_map(|pack| pack.get(&icon_name_lower).cloned());
            
            (resolver, lookup_result)
        };
        
        resolver(lookup_result.into(), original_icon_dom, system_style)
    }
    
    /// Look up an icon across all packs
    #[must_use] pub fn lookup(&self, icon_name: &str) -> Option<RefAny> {
        let icon_name_lower = icon_name.to_lowercase();
        self.inner.lock().ok().and_then(|guard| {
            for pack in guard.icons.values() {
                if let Some(data) = pack.get(&icon_name_lower) {
                    return Some(data.clone());
                }
            }
            None
        })
    }
    
    /// Check if an icon exists
    #[must_use] pub fn has_icon(&self, icon_name: &str) -> bool {
        let icon_name_lower = icon_name.to_lowercase();
        self.inner.lock()
            .map(|guard| guard.icons.values().any(|p| p.contains_key(&icon_name_lower)))
            .unwrap_or(false)
    }
}

// Icon Resolution in StyledDom

/// Collected icon node info for replacement
struct CollectedIcon {
    /// Index in the `node_data` array
    node_idx: usize,
    /// The icon name
    icon_name: AzString,
}

/// Replacement result after resolving an icon
struct IconReplacement {
    /// Index of the icon node to replace
    node_idx: usize,
    /// The resolved `StyledDom` (may be empty, single node, or multi-node tree)
    replacement: StyledDom,
}

/// Collect all Icon nodes from the `StyledDom`
fn collect_icon_nodes(styled_dom: &StyledDom) -> Vec<CollectedIcon> {
    let mut icons = Vec::new();
    
    let node_data = styled_dom.node_data.as_ref();
    for (idx, node) in node_data.iter().enumerate() {
        if let NodeType::Icon(icon_name) = node.get_node_type() {
            icons.push(CollectedIcon {
                node_idx: idx,
                icon_name: icon_name.clone_self(),
            });
        }
    }
    
    icons
}

/// Extract a single-node `StyledDom` from a parent `StyledDom` at the given index.
/// This creates a minimal `StyledDom` containing just that node for the resolver.
fn extract_single_node_styled_dom(styled_dom: &StyledDom, node_idx: usize) -> StyledDom {
    use crate::dom::{NodeDataVec, DomId};
    use crate::id::NodeId;
    use crate::styled_dom::{
        StyledNodeVec, NodeHierarchyItemIdVec, TagIdToNodeIdMappingVec,
        NodeHierarchyItemVec, NodeHierarchyItem, NodeHierarchyItemId,
        ParentWithNodeDepthVec, ParentWithNodeDepth,
    };
    use crate::style::{CascadeInfoVec, CascadeInfo};
    use crate::prop_cache::{CssPropertyCachePtr, CssPropertyCache};
    
    let node_data = styled_dom.node_data.as_ref();
    let styled_nodes = styled_dom.styled_nodes.as_ref();
    
    if node_idx >= node_data.len() {
        return StyledDom::default();
    }
    
    // Clone the single node
    let single_node = node_data[node_idx].clone();
    let single_styled = if node_idx < styled_nodes.len() {
        styled_nodes[node_idx].clone()
    } else {
        crate::styled_dom::StyledNode::default()
    };
    
    StyledDom {
        root: NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO)),
        node_hierarchy: NodeHierarchyItemVec::from_vec(vec![NodeHierarchyItem {
            parent: 0,
            previous_sibling: 0,
            next_sibling: 0,
            last_child: 0,
        }]),
        node_data: NodeDataVec::from_vec(vec![single_node]),
        styled_nodes: StyledNodeVec::from_vec(vec![single_styled]),
        cascade_info: CascadeInfoVec::from_vec(vec![CascadeInfo { index_in_parent: 0, is_last_child: true }]),
        nodes_with_window_callbacks: NodeHierarchyItemIdVec::from_vec(Vec::new()),
        nodes_with_datasets: NodeHierarchyItemIdVec::from_vec(Vec::new()),
        tag_ids_to_node_ids: TagIdToNodeIdMappingVec::from_vec(Vec::new()),
        non_leaf_nodes: ParentWithNodeDepthVec::from_vec(Vec::new()),
        css_property_cache: CssPropertyCachePtr::new(CssPropertyCache::empty(1)),
        dom_id: DomId::ROOT_ID,
    }
}

/// Resolve all collected icons to their `StyledDom` representations
fn resolve_collected_icons(
    icons: &[CollectedIcon],
    styled_dom: &StyledDom,
    provider: &SharedIconProvider,
    system_style: &SystemStyle,
) -> Vec<IconReplacement> {
    icons.iter().map(|icon| {
        // Extract the original icon node as a StyledDom
        let original_icon_dom = extract_single_node_styled_dom(styled_dom, icon.node_idx);
        let replacement = provider.resolve(&original_icon_dom, icon.icon_name.as_str(), system_style);
        IconReplacement {
            node_idx: icon.node_idx,
            replacement,
        }
    }).collect()
}

/// Check if a replacement is a single-node replacement (fast path)
fn is_single_node_replacement(replacement: &StyledDom) -> bool {
    replacement.node_data.as_ref().len() == 1
}

/// Apply a single-node replacement (fast path: swap `NodeType` and copy properties)
fn apply_single_node_replacement(
    styled_dom: &mut StyledDom,
    node_idx: usize,
    replacement: &StyledDom,
) {
    if replacement.node_data.as_ref().is_empty() {
        // Empty replacement - convert to empty div
        let node_data = styled_dom.node_data.as_mut();
        if let Some(node) = node_data.get_mut(node_idx) {
            node.set_node_type(NodeType::Div);
        }
    } else {
        // Get the root node from the replacement and copy its properties
        let replacement_root = &replacement.node_data.as_ref()[0];
        let replacement_node_type = replacement_root.get_node_type().clone();
        
        let node_data = styled_dom.node_data.as_mut();
        if let Some(node) = node_data.get_mut(node_idx) {
            // Swap node type
            node.set_node_type(replacement_node_type);
            
            // Copy inline style from replacement
            node.set_style(replacement_root.get_style().clone());
            
            // Copy accessibility info if present
            if let Some(a11y) = replacement_root.get_accessibility_info() {
                node.set_accessibility_info(a11y.clone());
            }
        }
        
        // Also update the styled_nodes to reflect the new styling
        if let Some(replacement_styled) = replacement.styled_nodes.as_ref().first() {
            let styled_nodes = styled_dom.styled_nodes.as_mut();
            if let Some(styled) = styled_nodes.get_mut(node_idx) {
                *styled = replacement_styled.clone();
            }
        }
    }
}

/// Apply multi-node replacement using subtree splicing
fn apply_multi_node_replacement(
    styled_dom: &mut StyledDom,
    node_idx: usize,
    replacement: &StyledDom,
) {
    let replacement_len = replacement.node_data.as_ref().len();
    if replacement_len == 0 {
        let node_data = styled_dom.node_data.as_mut();
        if let Some(node) = node_data.get_mut(node_idx) {
            node.set_node_type(NodeType::Div);
        }
        return;
    }
    
    // For now, just apply the root node (same as single-node)
    apply_single_node_replacement(styled_dom, node_idx, replacement);
    
    if replacement_len > 1 {
        // TODO: Full subtree splicing requires inserting nodes into arrays
        #[cfg(all(debug_assertions, feature = "std"))]
        eprintln!(
            "Warning: Icon replacement has {replacement_len} nodes, only root node used."
        );
    }
}

/// Resolve all Icon nodes in a `StyledDom` to their actual content.
///
/// This function:
/// 1. Collects all Icon nodes from the `StyledDom`
/// 2. Resolves each icon via the provider's callback (passing original icon DOM)
/// 3. Applies replacements (single-node fast path or multi-node splicing)
///
/// This should be called after `StyledDom` creation but before layout.
pub fn resolve_icons_in_styled_dom(
    styled_dom: &mut StyledDom,
    provider: &SharedIconProvider,
    system_style: &SystemStyle,
) {
    // Step 1: Collect all icon nodes
    let icons = collect_icon_nodes(styled_dom);

    if icons.is_empty() {
        return;
    }

    // Step 2: Resolve all icons to their StyledDom representations
    // Note: We pass styled_dom to extract each icon's original node
    let replacements = resolve_collected_icons(&icons, styled_dom, provider, system_style);

    // Step 3: Apply replacements (reverse order to preserve indices)
    for replacement in replacements.into_iter().rev() {
        if is_single_node_replacement(&replacement.replacement) ||
           replacement.replacement.node_data.as_ref().is_empty() {
            apply_single_node_replacement(
                styled_dom,
                replacement.node_idx,
                &replacement.replacement
            );
        } else {
            apply_multi_node_replacement(
                styled_dom,
                replacement.node_idx,
                &replacement.replacement
            );
        }
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

    fn styled_dom_with_icons(names: &[&str]) -> StyledDom {
        let mut body = Dom::create_body();
        for n in names {
            body.add_child(Dom::create_icon(*n));
        }
        StyledDom::create_from_dom(body)
    }

    /// A `StyledDom` with *zero* nodes — `StyledDom::default()` has one (a Body),
    /// so the truly-empty case has to be built by hand.
    fn zero_node_styled_dom() -> StyledDom {
        let mut sd = StyledDom::default();
        sd.node_data = NodeDataVec::from_vec(Vec::new());
        sd.styled_nodes = StyledNodeVec::from_vec(Vec::new());
        sd
    }

    fn node_type_at(sd: &StyledDom, idx: usize) -> NodeType {
        sd.node_data.as_ref()[idx].get_node_type().clone()
    }

    fn icon_indices(sd: &StyledDom) -> Vec<usize> {
        collect_icon_nodes(sd).iter().map(|i| i.node_idx).collect()
    }

    // Resolvers

    extern "C" fn div_resolver(
        _icon_data: OptionRefAny,
        _original_icon_dom: &StyledDom,
        _system_style: &SystemStyle,
    ) -> StyledDom {
        StyledDom::create_from_dom(Dom::create_div())
    }

    extern "C" fn zero_node_resolver(
        _icon_data: OptionRefAny,
        _original_icon_dom: &StyledDom,
        _system_style: &SystemStyle,
    ) -> StyledDom {
        zero_node_styled_dom()
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
        original_icon_dom: &StyledDom,
        _system_style: &SystemStyle,
    ) -> StyledDom {
        REC_CALLS.fetch_add(1, AtomicOrdering::SeqCst);
        if matches!(icon_data, OptionRefAny::Some(_)) {
            REC_SAW_DATA.store(true, AtomicOrdering::SeqCst);
        }
        if let Some(node) = original_icon_dom.node_data.as_ref().first() {
            if let NodeType::Icon(name) = node.get_node_type() {
                REC_SAW_ICON_NODE.store(true, AtomicOrdering::SeqCst);
                REC_NAME_LEN.store(name.as_ref().as_str().len(), AtomicOrdering::SeqCst);
            }
        }
        StyledDom::create_from_dom(Dom::create_div())
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
    fn default_resolver_returns_one_body_node_for_none_and_some() {
        let orig = StyledDom::default();
        let style = SystemStyle::default();

        let none = default_icon_resolver(OptionRefAny::None, &orig, &style);
        // NOTE: the doc calls this an "empty StyledDom", but `StyledDom::default()`
        // carries exactly one node (a Body), so the result is single-node, NOT empty.
        assert_eq!(none.node_data.as_ref().len(), 1);
        assert!(is_single_node_replacement(&none));

        let some = default_icon_resolver(
            OptionRefAny::Some(RefAny::new(TestIconData { id: 1 })),
            &orig,
            &style,
        );
        assert_eq!(some.node_data.as_ref().len(), 1);
    }

    #[test]
    fn default_resolver_no_panic_on_zero_node_original_dom() {
        let orig = zero_node_styled_dom();
        let style = SystemStyle::default();
        let out = default_icon_resolver(OptionRefAny::None, &orig, &style);
        assert_eq!(out.node_data.as_ref().len(), 1);
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
        let out = shared.resolve(&StyledDom::default(), "home", &SystemStyle::default());
        assert!(matches!(node_type_at(&out, 0), NodeType::Div));
    }

    #[test]
    fn set_resolver_overrides_the_default_resolver() {
        let mut h = IconProviderHandle::new();
        h.set_resolver(div_resolver);
        let shared = SharedIconProvider::from_handle(h);
        // Unregistered icon: resolver still runs, just with `None` data.
        let out = shared.resolve(&StyledDom::default(), "missing", &SystemStyle::default());
        assert!(matches!(node_type_at(&out, 0), NodeType::Div));
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
    fn lookup_garbage_and_leading_trailing_junk_returns_none() {
        let mut h = IconProviderHandle::new();
        h.register_icon("p", "home", RefAny::new(TestIconData { id: 1 }));

        for junk in [
            " home ",
            "home ",
            " home",
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

        let original = styled_dom_with_icons(&["home"]);
        let icon_idx = icon_indices(&original)[0];
        let single = extract_single_node_styled_dom(&original, icon_idx);

        let out = shared.resolve(&single, "HOME", &SystemStyle::default());

        assert!(REC_CALLS.load(AtomicOrdering::SeqCst) >= 1);
        assert!(REC_SAW_DATA.load(AtomicOrdering::SeqCst), "case-folded lookup must pass Some(data)");
        assert!(REC_SAW_ICON_NODE.load(AtomicOrdering::SeqCst), "node 0 of the original dom is the Icon");
        assert_eq!(REC_NAME_LEN.load(AtomicOrdering::SeqCst), "home".len());
        assert!(matches!(node_type_at(&out, 0), NodeType::Div));
    }

    #[test]
    fn shared_resolve_runs_the_resolver_even_when_the_icon_is_missing() {
        let h = IconProviderHandle::with_resolver(div_resolver);
        let shared = SharedIconProvider::from_handle(h);
        // Empty name, huge name, unicode name: resolver still returns its DOM.
        let huge = "x".repeat(100_000);
        for name in ["", "\u{1F600}", huge.as_str()] {
            let out = shared.resolve(&StyledDom::default(), name, &SystemStyle::default());
            assert!(matches!(node_type_at(&out, 0), NodeType::Div));
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

    // collect_icon_nodes

    #[test]
    fn collect_icon_nodes_is_empty_when_there_are_no_icons() {
        assert!(collect_icon_nodes(&StyledDom::default()).is_empty());
        assert!(collect_icon_nodes(&zero_node_styled_dom()).is_empty());
        assert!(collect_icon_nodes(&StyledDom::create_from_dom(Dom::create_div())).is_empty());
    }

    #[test]
    fn collect_icon_nodes_finds_every_icon_in_ascending_index_order_with_verbatim_names() {
        let names = ["HOME", "\u{1F600}", ""];
        let sd = styled_dom_with_icons(&names);
        let collected = collect_icon_nodes(&sd);

        assert_eq!(collected.len(), names.len());
        for (i, c) in collected.iter().enumerate() {
            // Node names are NOT folded at DOM-construction time (only at lookup).
            assert_eq!(c.icon_name.as_str(), names[i]);
            if i > 0 {
                assert!(c.node_idx > collected[i - 1].node_idx, "indices must ascend");
            }
        }
    }

    #[test]
    fn collect_icon_nodes_handles_a_very_long_icon_name() {
        let long = "x".repeat(100_000);
        let sd = styled_dom_with_icons(&[&long]);
        let collected = collect_icon_nodes(&sd);
        assert_eq!(collected.len(), 1);
        assert_eq!(collected[0].icon_name.as_str().len(), 100_000);
    }

    // extract_single_node_styled_dom (numeric / index boundaries)

    #[test]
    fn extract_single_node_at_index_zero() {
        let sd = styled_dom_with_icons(&["home"]);
        let out = extract_single_node_styled_dom(&sd, 0);
        assert_eq!(out.node_data.as_ref().len(), 1);
        assert_eq!(out.styled_nodes.as_ref().len(), 1);
        assert_eq!(node_type_at(&out, 0), node_type_at(&sd, 0));
    }

    #[test]
    fn extract_single_node_of_the_icon_keeps_the_icon_node_type() {
        let sd = styled_dom_with_icons(&["home"]);
        let idx = icon_indices(&sd)[0];
        let out = extract_single_node_styled_dom(&sd, idx);
        assert_eq!(out.node_data.as_ref().len(), 1);
        assert!(matches!(node_type_at(&out, 0), NodeType::Icon(_)));
    }

    #[test]
    fn extract_single_node_out_of_bounds_falls_back_to_default_without_panicking() {
        let sd = styled_dom_with_icons(&["home"]);
        let len = sd.node_data.as_ref().len();

        for idx in [len, len + 1, usize::MAX / 2, usize::MAX - 1, usize::MAX] {
            let out = extract_single_node_styled_dom(&sd, idx);
            // Falls back to StyledDom::default() -> exactly one (Body) node.
            assert_eq!(out.node_data.as_ref().len(), 1, "idx {idx} must not panic");
            assert!(!matches!(node_type_at(&out, 0), NodeType::Icon(_)));
        }
        // Zero-node input: even index 0 is out of bounds.
        let empty = zero_node_styled_dom();
        assert_eq!(extract_single_node_styled_dom(&empty, 0).node_data.as_ref().len(), 1);
    }

    #[test]
    fn extract_single_node_tolerates_styled_nodes_shorter_than_node_data() {
        let sd_full = styled_dom_with_icons(&["home"]);
        let idx = icon_indices(&sd_full)[0];

        let mut sd = sd_full;
        sd.styled_nodes = StyledNodeVec::from_vec(Vec::new()); // desynced arrays

        let out = extract_single_node_styled_dom(&sd, idx);
        assert_eq!(out.node_data.as_ref().len(), 1);
        assert_eq!(out.styled_nodes.as_ref().len(), 1, "must synthesize a default StyledNode");
        assert!(matches!(node_type_at(&out, 0), NodeType::Icon(_)));
    }

    // is_single_node_replacement

    #[test]
    fn is_single_node_replacement_true_false_and_edges() {
        assert!(is_single_node_replacement(&StyledDom::default()));
        assert!(is_single_node_replacement(&StyledDom::create_from_dom(Dom::create_div())));

        // Zero nodes is NOT "single node" (callers treat it as the empty case).
        assert!(!is_single_node_replacement(&zero_node_styled_dom()));

        let multi = StyledDom::create_from_dom(
            Dom::create_div().with_child(Dom::create_div()).with_child(Dom::create_div()),
        );
        assert!(multi.node_data.as_ref().len() > 1);
        assert!(!is_single_node_replacement(&multi));
    }

    // apply_single_node_replacement (index boundaries)

    #[test]
    fn apply_single_node_replacement_with_zero_node_dom_turns_the_icon_into_a_div() {
        let mut sd = styled_dom_with_icons(&["home"]);
        let idx = icon_indices(&sd)[0];
        let empty = zero_node_styled_dom();

        apply_single_node_replacement(&mut sd, idx, &empty);
        assert!(matches!(node_type_at(&sd, idx), NodeType::Div));
        assert!(collect_icon_nodes(&sd).is_empty());
    }

    #[test]
    fn apply_single_node_replacement_copies_the_replacement_root_node_type() {
        let mut sd = styled_dom_with_icons(&["home"]);
        let idx = icon_indices(&sd)[0];
        let before_len = sd.node_data.as_ref().len();
        let repl = StyledDom::create_from_dom(Dom::create_div());

        apply_single_node_replacement(&mut sd, idx, &repl);
        assert!(matches!(node_type_at(&sd, idx), NodeType::Div));
        assert_eq!(sd.node_data.as_ref().len(), before_len, "node count must not change");
    }

    #[test]
    fn apply_single_node_replacement_out_of_bounds_index_is_a_no_op() {
        let repl = StyledDom::create_from_dom(Dom::create_div());
        let empty = zero_node_styled_dom();

        let base = styled_dom_with_icons(&["home"]);
        let icon_idx = icon_indices(&base)[0];
        let len = base.node_data.as_ref().len();

        for idx in [len, len + 1, usize::MAX / 2, usize::MAX] {
            let mut sd = styled_dom_with_icons(&["home"]);
            apply_single_node_replacement(&mut sd, idx, &repl);
            apply_single_node_replacement(&mut sd, idx, &empty);
            assert_eq!(sd.node_data.as_ref().len(), len, "idx {idx} must not resize");
            assert!(
                matches!(node_type_at(&sd, icon_idx), NodeType::Icon(_)),
                "idx {idx} must leave the icon untouched"
            );
        }
    }

    // apply_multi_node_replacement (index boundaries)

    #[test]
    fn apply_multi_node_replacement_with_zero_node_dom_turns_the_icon_into_a_div() {
        let mut sd = styled_dom_with_icons(&["home"]);
        let idx = icon_indices(&sd)[0];

        apply_multi_node_replacement(&mut sd, idx, &zero_node_styled_dom());
        assert!(matches!(node_type_at(&sd, idx), NodeType::Div));
    }

    #[test]
    fn apply_multi_node_replacement_applies_only_the_root_and_does_not_splice() {
        let mut sd = styled_dom_with_icons(&["home"]);
        let idx = icon_indices(&sd)[0];
        let before_len = sd.node_data.as_ref().len();

        let repl = StyledDom::create_from_dom(
            Dom::create_div().with_child(Dom::create_div()).with_child(Dom::create_div()),
        );
        assert!(repl.node_data.as_ref().len() > 1);

        apply_multi_node_replacement(&mut sd, idx, &repl);

        // Documented TODO: subtree splicing is not implemented, only the root is used.
        assert!(matches!(node_type_at(&sd, idx), NodeType::Div));
        assert_eq!(sd.node_data.as_ref().len(), before_len, "children are dropped, not spliced");
    }

    #[test]
    fn apply_multi_node_replacement_out_of_bounds_index_is_a_no_op() {
        let repl = StyledDom::create_from_dom(Dom::create_div().with_child(Dom::create_div()));
        let base = styled_dom_with_icons(&["home"]);
        let icon_idx = icon_indices(&base)[0];
        let len = base.node_data.as_ref().len();

        for idx in [len, usize::MAX] {
            let mut sd = styled_dom_with_icons(&["home"]);
            apply_multi_node_replacement(&mut sd, idx, &repl);
            apply_multi_node_replacement(&mut sd, idx, &zero_node_styled_dom());
            assert_eq!(sd.node_data.as_ref().len(), len);
            assert!(matches!(node_type_at(&sd, icon_idx), NodeType::Icon(_)));
        }
    }

    // resolve_collected_icons

    #[test]
    fn resolve_collected_icons_preserves_indices_and_resolves_each_icon() {
        let mut h = IconProviderHandle::with_resolver(div_resolver);
        h.register_icon("p", "home", RefAny::new(TestIconData { id: 1 }));
        let shared = SharedIconProvider::from_handle(h);

        let sd = styled_dom_with_icons(&["home", "missing", "\u{1F600}"]);
        let icons = collect_icon_nodes(&sd);
        let replacements =
            resolve_collected_icons(&icons, &sd, &shared, &SystemStyle::default());

        assert_eq!(replacements.len(), icons.len());
        for (r, i) in replacements.iter().zip(icons.iter()) {
            assert_eq!(r.node_idx, i.node_idx);
            // The custom resolver ignores the data, so even unregistered icons resolve.
            assert!(matches!(node_type_at(&r.replacement, 0), NodeType::Div));
        }
    }

    #[test]
    fn resolve_collected_icons_with_no_icons_returns_no_replacements() {
        let shared = SharedIconProvider::from_handle(IconProviderHandle::new());
        let sd = StyledDom::default();
        let out = resolve_collected_icons(&[], &sd, &shared, &SystemStyle::default());
        assert!(out.is_empty());
    }

    // resolve_icons_in_styled_dom (end to end)

    #[test]
    fn resolve_icons_in_styled_dom_is_a_no_op_without_icons() {
        let shared = SharedIconProvider::from_handle(IconProviderHandle::with_resolver(div_resolver));
        let mut sd = StyledDom::create_from_dom(Dom::create_body().with_child(Dom::create_text("hi")));
        let before_len = sd.node_data.as_ref().len();
        let before_root = node_type_at(&sd, 0);

        resolve_icons_in_styled_dom(&mut sd, &shared, &SystemStyle::default());

        assert_eq!(sd.node_data.as_ref().len(), before_len);
        assert_eq!(node_type_at(&sd, 0), before_root);
    }

    #[test]
    fn resolve_icons_in_styled_dom_replaces_every_icon_case_insensitively() {
        let mut h = IconProviderHandle::with_resolver(div_resolver);
        h.register_icon("p", "home", RefAny::new(TestIconData { id: 1 }));
        let shared = SharedIconProvider::from_handle(h);

        // Mixed case in the DOM, lowercase in the pack, plus one unregistered icon.
        let mut sd = styled_dom_with_icons(&["HOME", "unregistered", "HoMe"]);
        let idxs = icon_indices(&sd);
        let before_len = sd.node_data.as_ref().len();

        resolve_icons_in_styled_dom(&mut sd, &shared, &SystemStyle::default());

        assert_eq!(sd.node_data.as_ref().len(), before_len);
        assert!(collect_icon_nodes(&sd).is_empty(), "no Icon node may survive resolution");
        for idx in idxs {
            assert!(matches!(node_type_at(&sd, idx), NodeType::Div));
        }
    }

    #[test]
    fn resolve_icons_in_styled_dom_with_the_default_resolver_removes_the_icon_nodes() {
        // The default resolver returns `StyledDom::default()` (one Body node), so
        // icons are replaced by that root's node type rather than being cleared.
        let shared = SharedIconProvider::from_handle(IconProviderHandle::new());
        let mut sd = styled_dom_with_icons(&["home"]);
        let idx = icon_indices(&sd)[0];

        resolve_icons_in_styled_dom(&mut sd, &shared, &SystemStyle::default());

        assert!(!matches!(node_type_at(&sd, idx), NodeType::Icon(_)));
        assert!(collect_icon_nodes(&sd).is_empty());
    }

    #[test]
    fn resolve_icons_in_styled_dom_handles_a_zero_node_replacement() {
        let shared =
            SharedIconProvider::from_handle(IconProviderHandle::with_resolver(zero_node_resolver));
        let mut sd = styled_dom_with_icons(&["home", "other"]);
        let idxs = icon_indices(&sd);
        let before_len = sd.node_data.as_ref().len();

        resolve_icons_in_styled_dom(&mut sd, &shared, &SystemStyle::default());

        assert_eq!(sd.node_data.as_ref().len(), before_len);
        for idx in idxs {
            assert!(matches!(node_type_at(&sd, idx), NodeType::Div), "empty => Div placeholder");
        }
    }

    #[test]
    fn resolve_icons_in_styled_dom_scales_to_many_icons() {
        let shared = SharedIconProvider::from_handle(IconProviderHandle::with_resolver(div_resolver));
        let names: Vec<String> = (0..500).map(|i| format!("icon{i}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let mut sd = styled_dom_with_icons(&refs);
        let before_len = sd.node_data.as_ref().len();

        resolve_icons_in_styled_dom(&mut sd, &shared, &SystemStyle::default());

        assert_eq!(sd.node_data.as_ref().len(), before_len);
        assert!(collect_icon_nodes(&sd).is_empty());
    }
}
