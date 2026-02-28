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

#[cfg(feature = "std")]
use std::sync::Mutex;

#[cfg(not(feature = "std"))]
use spin::Mutex;

use azul_css::{AzString, system::SystemStyle};

use crate::{
    debug::DebugLog,
    dom::{Dom, NodeType},
    refany::{OptionRefAny, RefAny},
    styled_dom::StyledDom,
};

// Icon Resolver Callback

/// Callback type for resolving icon data to a StyledDom.
///
/// Parameters:
/// - `icon_data`: The RefAny data from the icon pack (cloned, or None if not found)
/// - `original_icon_dom`: The original icon node's StyledDom (contains inline styles, a11y info, icon_name)
/// - `system_style`: Current system style (theme, colors, etc.)
///
/// Returns: A StyledDom that will replace the icon node.
/// The resolver should copy relevant styles from original_icon_dom to the result.
/// Return an empty StyledDom to show a placeholder or nothing.
///
/// Note: icon_name is accessible via `original_icon_dom.node_data[0].get_node_type()` → `NodeType::Icon(name)`
pub type IconResolverCallbackType = extern "C" fn(
    icon_data: OptionRefAny,
    original_icon_dom: &StyledDom,
    system_style: &SystemStyle,
) -> StyledDom;

/// Default resolver that returns an empty StyledDom (shows placeholder)
pub extern "C" fn default_icon_resolver(
    _icon_data: OptionRefAny,
    _original_icon_dom: &StyledDom,
    _system_style: &SystemStyle,
) -> StyledDom {
    // Default: return empty DOM (icon won't be visible)
    StyledDom::default()
}

// Icon Provider Inner (single mutex)

/// Inner data for IconProviderHandle - all fields behind single mutex
#[derive(Clone)]
pub struct IconProviderInner {
    /// Nested map: pack_name → (icon_name → RefAny)
    /// Differentiation between Image/Font/SVG is via RefAny::downcast
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

/// Icon provider stored in AppConfig.
///
/// This is a Box<IconProviderInner> for C FFI compatibility.
/// When App::run() is called, it gets converted to Arc<Mutex<IconProviderInner>>
/// and cloned to each window.
///
/// Icons are stored in a nested map: pack_name → (icon_name → RefAny)
/// This allows:
/// - Multiple packs with different sources (app-images, material-icons, etc.)
/// - Easy unregistration of entire packs
/// - First-match-wins lookup across all packs
#[repr(C)]
pub struct IconProviderHandle {
    /// Boxed inner data - Box<T> is repr(C) compatible (single pointer)
    pub inner: Box<IconProviderInner>,
}

impl Clone for IconProviderHandle {
    fn clone(&self) -> Self {
        Self { inner: Box::new((*self.inner).clone()) }
    }
}

impl fmt::Debug for IconProviderHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let pack_count = self.inner.icons.len();
        let icon_count: usize = self.inner.icons.values().map(|p| p.len()).sum();
        
        f.debug_struct("IconProviderHandle")
            .field("pack_count", &pack_count)
            .field("icon_count", &icon_count)
            .finish()
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
    /// Note: The default resolver in core crate returns an empty StyledDom.
    /// Use `set_resolver()` to set a proper resolver from the layout crate,
    /// or use `with_resolver()` to create with a custom resolver.
    pub fn new() -> Self {
        Self {
            inner: Box::new(IconProviderInner {
                icons: BTreeMap::new(),
                resolver: default_icon_resolver,
            })
        }
    }

    /// Create with a custom resolver callback
    pub fn with_resolver(resolver: IconResolverCallbackType) -> Self {
        Self {
            inner: Box::new(IconProviderInner {
                icons: BTreeMap::new(),
                resolver,
            })
        }
    }
    
    /// Convert this handle into an Arc<Mutex<IconProviderInner>> for use in windows.
    /// 
    /// This consumes the Box and creates an Arc. Called by App::run() to create
    /// the shared icon provider that gets cloned to each window.
    pub fn into_shared(self) -> Arc<Mutex<IconProviderInner>> {
        Arc::new(Mutex::new(*self.inner))
    }

    /// Set the resolver callback
    pub fn set_resolver(&mut self, resolver: IconResolverCallbackType) {
        self.inner.resolver = resolver;
    }

    /// Register a single icon in a pack (creates pack if needed)
    pub fn register_icon(&mut self, pack_name: &str, icon_name: &str, data: RefAny) {
        let pack = self.inner.icons
            .entry(pack_name.to_string())
            .or_insert_with(BTreeMap::new);
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

    /// Look up an icon across all packs (first match wins)
    pub fn lookup(&self, icon_name: &str) -> Option<RefAny> {
        let icon_name_lower = icon_name.to_lowercase();
        for pack in self.inner.icons.values() {
            if let Some(data) = pack.get(&icon_name_lower) {
                return Some(data.clone());
            }
        }
        None
    }

    /// Check if an icon exists in any pack
    pub fn has_icon(&self, icon_name: &str) -> bool {
        let icon_name_lower = icon_name.to_lowercase();
        self.inner.icons.values().any(|p| p.contains_key(&icon_name_lower))
    }

    /// List all pack names
    pub fn list_packs(&self) -> Vec<String> {
        self.inner.icons.keys().cloned().collect()
    }

    /// List all icon names in a specific pack
    pub fn list_icons_in_pack(&self, pack_name: &str) -> Vec<String> {
        self.inner.icons.get(pack_name)
            .map(|pack| pack.keys().cloned().collect())
            .unwrap_or_default()
    }

    /// Debug lookup: returns detailed info about an icon's RefAny contents
    pub fn debug_lookup(&self, icon_name: &str) -> AzString {
        let icon_name_lower = icon_name.to_lowercase();
        
        let mut result = format!("Debug lookup for icon '{}' (normalized: '{}'):\n", icon_name, icon_name_lower);
        
        // Report registered packs
        result.push_str(&format!("  Total packs: {}\n", self.inner.icons.len()));
        for (pack_name, pack) in self.inner.icons.iter() {
            result.push_str(&format!("    Pack '{}': {} icons\n", pack_name, pack.len()));
            for (name, _) in pack.iter() {
                result.push_str(&format!("      - {}\n", name));
            }
        }
        
        // Find the icon
        let mut found_in_pack: Option<&str> = None;
        let mut refany: Option<&RefAny> = None;
        for (pack_name, pack) in self.inner.icons.iter() {
            if let Some(data) = pack.get(&icon_name_lower) {
                found_in_pack = Some(pack_name);
                refany = Some(data);
                break;
            }
        }
        
        match (found_in_pack, refany) {
            (Some(pack), Some(data)) => {
                result.push_str(&format!("\n  FOUND in pack '{}'\n", pack));
                let type_name = data.get_type_name();
                result.push_str(&format!("  RefAny type_name: '{}'\n", type_name.as_str()));
                
                let debug_info = data.sharing_info.debug_get_refcount_copied();
                result.push_str(&format!("  RefAny size: {} bytes\n", debug_info._internal_layout_size));
                
                let type_str = type_name.as_str();
                if type_str.contains("ImageIconData") {
                    result.push_str("  RefAny type: ImageIconData (image-based icon)\n");
                } else if type_str.contains("FontIconData") {
                    result.push_str("  RefAny type: FontIconData (font-based icon)\n");
                } else {
                    result.push_str(&format!("  RefAny type: UNKNOWN ('{}')\n", type_str));
                }
            }
            _ => {
                result.push_str(&format!("\n  NOT FOUND in any pack\n"));
            }
        }
        
        AzString::from(result)
    }
}

/// Thread-safe icon provider for use in windows.
/// 
/// This is created from IconProviderHandle::into_shared() in App::run()
/// and cloned to each window.
#[derive(Clone)]
pub struct SharedIconProvider {
    inner: Arc<Mutex<IconProviderInner>>,
}

impl SharedIconProvider {
    /// Create from an IconProviderHandle (consumes the handle)
    pub fn from_handle(handle: IconProviderHandle) -> Self {
        Self { inner: handle.into_shared() }
    }
    
    /// Resolve an icon to a StyledDom using the registered callback
    pub fn resolve(
        &self, 
        original_icon_dom: &StyledDom,
        icon_name: &str,
        system_style: &SystemStyle,
    ) -> StyledDom {
        let (resolver, lookup_result) = {
            let guard = match self.inner.lock() {
                Ok(g) => g,
                Err(_) => return StyledDom::default(),
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
    pub fn lookup(&self, icon_name: &str) -> Option<RefAny> {
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
    pub fn has_icon(&self, icon_name: &str) -> bool {
        let icon_name_lower = icon_name.to_lowercase();
        self.inner.lock()
            .map(|guard| guard.icons.values().any(|p| p.contains_key(&icon_name_lower)))
            .unwrap_or(false)
    }
}

// Icon Resolution in StyledDom

/// Collected icon node info for replacement
struct CollectedIcon {
    /// Index in the node_data array
    node_idx: usize,
    /// The icon name
    icon_name: AzString,
}

/// Replacement result after resolving an icon
struct IconReplacement {
    /// Index of the icon node to replace
    node_idx: usize,
    /// The resolved StyledDom (may be empty, single node, or multi-node tree)
    replacement: StyledDom,
}

/// Collect all Icon nodes from the StyledDom
fn collect_icon_nodes(styled_dom: &StyledDom) -> Vec<CollectedIcon> {
    let mut icons = Vec::new();
    
    let node_data = styled_dom.node_data.as_ref();
    for (idx, node) in node_data.iter().enumerate() {
        if let NodeType::Icon(icon_name) = node.get_node_type() {
            icons.push(CollectedIcon {
                node_idx: idx,
                icon_name: icon_name.clone(),
            });
        }
    }
    
    icons
}

/// Generate accessibility label from icon name
fn generate_a11y_label(icon_name: &str) -> AzString {
    AzString::from(format!("{} icon", icon_name.replace('_', " ").replace('-', " ")))
}

/// Extract a single-node StyledDom from a parent StyledDom at the given index.
/// This creates a minimal StyledDom containing just that node for the resolver.
fn extract_single_node_styled_dom(styled_dom: &StyledDom, node_idx: usize) -> StyledDom {
    use crate::dom::{NodeDataVec, DomId};
    use crate::styled_dom::{
        StyledNodeVec, NodeHierarchyItemIdVec, TagIdToNodeIdMappingVec,
        NodeHierarchyItemVec, NodeHierarchyItemId, ParentWithNodeDepthVec, ParentWithNodeDepth,
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
        root: styled_dom.root.clone(),
        node_hierarchy: styled_dom.node_hierarchy.clone(),
        node_data: NodeDataVec::from_vec(vec![single_node]),
        styled_nodes: StyledNodeVec::from_vec(vec![single_styled]),
        cascade_info: CascadeInfoVec::from_vec(vec![CascadeInfo { index_in_parent: 0, is_last_child: true }]),
        nodes_with_window_callbacks: NodeHierarchyItemIdVec::from_vec(Vec::new()),
        nodes_with_not_callbacks: NodeHierarchyItemIdVec::from_vec(Vec::new()),
        nodes_with_datasets: NodeHierarchyItemIdVec::from_vec(Vec::new()),
        tag_ids_to_node_ids: TagIdToNodeIdMappingVec::from_vec(Vec::new()),
        non_leaf_nodes: ParentWithNodeDepthVec::from_vec(vec![ParentWithNodeDepth {
            depth: 0,
            node_id: styled_dom.root.clone(),
        }]),
        css_property_cache: CssPropertyCachePtr::new(CssPropertyCache::empty(1)),
        dom_id: DomId::ROOT_ID,
    }
}

/// Resolve all collected icons to their StyledDom representations
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

/// Apply a single-node replacement (fast path: swap NodeType and copy properties)
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
            
            // Copy CSS properties from replacement
            node.css_props = replacement_root.get_css_props().clone();
            
            // Copy accessibility info if present
            if let Some(a11y) = replacement_root.get_accessibility_info() {
                node.set_accessibility_info(*a11y.clone());
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
    replacement: StyledDom,
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
    apply_single_node_replacement(styled_dom, node_idx, &replacement);
    
    if replacement_len > 1 {
        // TODO: Full subtree splicing requires inserting nodes into arrays
        #[cfg(debug_assertions)]
        eprintln!(
            "Warning: Icon replacement has {} nodes, only root node used.",
            replacement_len
        );
    }
}

/// Resolve all Icon nodes in a StyledDom to their actual content.
///
/// This function:
/// 1. Collects all Icon nodes from the StyledDom
/// 2. Resolves each icon via the provider's callback (passing original icon DOM)
/// 3. Applies replacements (single-node fast path or multi-node splicing)
///
/// This should be called after StyledDom creation but before layout.
pub fn resolve_icons_in_styled_dom(
    styled_dom: &mut StyledDom,
    provider: &SharedIconProvider,
    system_style: &SystemStyle,
) {
    resolve_icons_in_styled_dom_with_log(styled_dom, provider, system_style, None)
}

/// Same as `resolve_icons_in_styled_dom` but with optional debug logging
pub fn resolve_icons_in_styled_dom_with_log(
    styled_dom: &mut StyledDom,
    provider: &SharedIconProvider,
    system_style: &SystemStyle,
    mut debug_log: Option<&mut DebugLog>,
) {
    use crate::log_debug;
    
    // Step 1: Collect all icon nodes
    let icons = collect_icon_nodes(styled_dom);
    
    if icons.is_empty() {
        if let Some(ref mut log) = debug_log {
            log_debug!(log, Icon, "No icon nodes found in StyledDom");
        }
        return;
    }
    
    if let Some(ref mut log) = debug_log {
        log_debug!(log, Icon, "Found {} icon nodes to resolve", icons.len());
        for icon in &icons {
            let has_icon = provider.has_icon(icon.icon_name.as_str());
            log_debug!(log, Icon, "  - Icon '{}' at node {}: registered={}", 
                icon.icon_name.as_str(), icon.node_idx, has_icon);
        }
    }
    
    // Step 2: Resolve all icons to their StyledDom representations
    // Note: We pass styled_dom to extract each icon's original node
    let replacements = resolve_collected_icons(&icons, styled_dom, provider, system_style);
    
    if let Some(ref mut log) = debug_log {
        for replacement in &replacements {
            let node_count = replacement.replacement.node_data.as_ref().len();
            let node_type = replacement.replacement.node_data.as_ref()
                .first()
                .map(|n| format!("{:?}", n.get_node_type()))
                .unwrap_or_else(|| "empty".to_string());
            log_debug!(log, Icon, "  - Replacement at {}: {} nodes, root type: {}", 
                replacement.node_idx, node_count, node_type);
        }
    }
    
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
                replacement.replacement
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
