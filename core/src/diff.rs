//! DOM Reconciliation Module
//!
//! This module provides the reconciliation algorithm that compares two DOM trees
//! and generates lifecycle events. It uses stable keys and content hashing to
//! identify moves vs. mounts/unmounts.
//!
//! The reconciliation strategy is:
//! 1. **Stable Key Match:** If `.with_key()` is used, it's an absolute match (O(1)).
//! 2. **CSS ID Match:** If no key, use the CSS ID as key.
//! 3. **Structural Key Match:** nth-of-type-within-parent + parent's key (recursive).
//! 4. **Hash Match (Content Match):** Check for identical `DomNodeHash`.
//! 5. **Structural Hash Match:** For text nodes, match by structural hash (ignoring content).
//! 6. **Fallback:** Anything not matched is a `Mount` (new) or `Unmount` (old leftovers).

use alloc::{collections::BTreeMap, collections::VecDeque, string::String, vec::Vec};
use core::hash::Hash;

use azul_css::props::property::{CssPropertyType, RelayoutScope};

use crate::{
    dom::{DomId, DomNodeHash, DomNodeId, NodeData, NodeType, IdOrClass},
    events::{
        ComponentEventFilter, EventData, EventFilter, EventPhase, EventSource, EventType,
        LifecycleEventData, LifecycleReason, SyntheticEvent,
    },
    geom::LogicalRect,
    id::NodeId,
    styled_dom::{ChangedCssProperty, NodeHierarchyItemId, NodeHierarchyItem, RestyleResult, StyledNodeState},
    task::Instant,
    FastHashMap,
};

// ============================================================================
// NodeChangeSet — granular per-node change flags
// ============================================================================

/// Bit flags describing what changed about a node between old and new DOM.
/// Multiple flags can be set simultaneously. Uses manual bit manipulation
/// instead of bitflags crate to avoid adding a dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct NodeChangeSet {
    pub bits: u32,
}

impl NodeChangeSet {
    // --- Changes that affect LAYOUT (need relayout + repaint) ---

    /// Node type changed entirely (e.g., Text → Image).
    pub const NODE_TYPE_CHANGED: u32    = 0b0000_0000_0000_0001;
    /// Text content changed (for Text nodes).
    pub const TEXT_CONTENT: u32         = 0b0000_0000_0000_0010;
    /// CSS IDs or classes changed (may cause restyle → relayout).
    pub const IDS_AND_CLASSES: u32      = 0b0000_0000_0000_0100;
    /// Inline CSS properties changed that affect layout.
    pub const INLINE_STYLE_LAYOUT: u32  = 0b0000_0000_0000_1000;
    /// Children added, removed, or reordered.
    pub const CHILDREN_CHANGED: u32     = 0b0000_0000_0001_0000;
    /// Image source changed (may affect intrinsic size).
    pub const IMAGE_CHANGED: u32        = 0b0000_0000_0010_0000;
    /// Contenteditable flag changed.
    pub const CONTENTEDITABLE: u32      = 0b0000_0000_0100_0000;
    /// Tab index changed.
    pub const TAB_INDEX: u32            = 0b0000_0000_1000_0000;

    // --- Changes that affect PAINT only (no relayout needed) ---

    /// Inline CSS properties changed that affect paint only.
    pub const INLINE_STYLE_PAINT: u32   = 0b0000_0001_0000_0000;
    /// Styled node state changed (hover, active, focus, etc.).
    pub const STYLED_STATE: u32         = 0b0000_0010_0000_0000;

    // --- Changes that affect NEITHER layout nor paint ---

    /// Callbacks changed (new RefAny, different event handlers).
    pub const CALLBACKS: u32            = 0b0000_0100_0000_0000;
    /// Dataset changed.
    pub const DATASET: u32              = 0b0000_1000_0000_0000;
    /// Accessibility info changed.
    pub const ACCESSIBILITY: u32        = 0b0001_0000_0000_0000;

    // --- Composite masks ---

    /// Any change that requires a layout pass.
    pub const AFFECTS_LAYOUT: u32 = Self::NODE_TYPE_CHANGED
        | Self::TEXT_CONTENT
        | Self::IDS_AND_CLASSES
        | Self::INLINE_STYLE_LAYOUT
        | Self::CHILDREN_CHANGED
        | Self::IMAGE_CHANGED
        | Self::CONTENTEDITABLE;

    /// Any change that requires a paint/display-list update (but not layout).
    pub const AFFECTS_PAINT: u32 = Self::INLINE_STYLE_PAINT
        | Self::STYLED_STATE;

    pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    pub const fn is_empty(&self) -> bool {
        self.bits == 0
    }

    pub const fn contains(&self, flag: u32) -> bool {
        (self.bits & flag) == flag
    }

    pub const fn intersects(&self, mask: u32) -> bool {
        (self.bits & mask) != 0
    }

    pub fn insert(&mut self, flag: u32) {
        self.bits |= flag;
    }

    /// Returns true if no visual change occurred (only callbacks/dataset/a11y).
    pub const fn is_visually_unchanged(&self) -> bool {
        !self.intersects(Self::AFFECTS_LAYOUT) && !self.intersects(Self::AFFECTS_PAINT)
    }

    /// Returns true if layout is needed.
    pub const fn needs_layout(&self) -> bool {
        self.intersects(Self::AFFECTS_LAYOUT)
    }

    /// Returns true if paint is needed (but not necessarily layout).
    pub const fn needs_paint(&self) -> bool {
        self.intersects(Self::AFFECTS_PAINT)
    }
}

impl core::ops::BitOrAssign for NodeChangeSet {
    fn bitor_assign(&mut self, rhs: Self) {
        self.bits |= rhs.bits;
    }
}

impl core::ops::BitOr for NodeChangeSet {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self { bits: self.bits | rhs.bits }
    }
}

/// Extended diff result that includes per-node change information.
#[derive(Debug, Clone)]
pub struct ExtendedDiffResult {
    /// Original diff result (lifecycle events + node moves).
    pub diff: DiffResult,
    /// Per-node change report for matched (moved) nodes.
    /// Each entry: (old_node_id, new_node_id, what_changed).
    /// Only contains entries for nodes that were matched.
    pub node_changes: Vec<(NodeId, NodeId, NodeChangeSet)>,
}

impl Default for ExtendedDiffResult {
    fn default() -> Self {
        Self {
            diff: DiffResult::default(),
            node_changes: Vec::new(),
        }
    }
}

/// Compare two matched `NodeData` instances field-by-field and return
/// a `NodeChangeSet` describing what changed.
pub fn compute_node_changes(
    old_node: &NodeData,
    new_node: &NodeData,
    old_styled_state: Option<&StyledNodeState>,
    new_styled_state: Option<&StyledNodeState>,
) -> NodeChangeSet {
    let mut changes = NodeChangeSet::empty();

    // 1. Node type discriminant
    if core::mem::discriminant(old_node.get_node_type())
        != core::mem::discriminant(new_node.get_node_type())
    {
        changes.insert(NodeChangeSet::NODE_TYPE_CHANGED);
        return changes; // everything else is irrelevant
    }

    // 2. Content-specific comparison (same discriminant)
    match (old_node.get_node_type(), new_node.get_node_type()) {
        (NodeType::Text(old_text), NodeType::Text(new_text)) => {
            if old_text.as_str() != new_text.as_str() {
                changes.insert(NodeChangeSet::TEXT_CONTENT);
            }
        }
        (NodeType::Image(old_img), NodeType::Image(new_img)) => {
            // Use Hash-based comparison (pointer identity for decoded images,
            // callback identity for callback images)
            use core::hash::Hasher;
            use highway::{HighwayHash, HighwayHasher, Key};
            let hash_img = |img: &crate::resources::ImageRef| -> u64 {
                let mut h = HighwayHasher::new(Key([0; 4]));
                img.hash(&mut h);
                h.finalize64()
            };
            if hash_img(old_img) != hash_img(new_img) {
                changes.insert(NodeChangeSet::IMAGE_CHANGED);
            }
        }
        _ => {} // Same non-content type → no content change
    }

    // 3. IDs and classes
    if old_node.ids_and_classes.as_ref() != new_node.ids_and_classes.as_ref() {
        changes.insert(NodeChangeSet::IDS_AND_CLASSES);
    }

    // 4. Inline CSS properties — classify into layout-affecting vs paint-only
    let old_props = old_node.css_props.as_ref();
    let new_props = new_node.css_props.as_ref();
    if old_props != new_props {
        let mut has_layout = false;
        let mut has_paint = false;

        // Build a map of property type → value for old props
        let mut old_map = FastHashMap::default();
        for prop in old_props.iter() {
            old_map.insert(
                prop.property.get_type(),
                prop,
            );
        }

        // Check new props against old
        let mut seen_types = FastHashMap::default();
        for prop in new_props.iter() {
            let prop_type = prop.property.get_type();
            seen_types.insert(prop_type, ());
            match old_map.get(&prop_type) {
                Some(old_prop) if **old_prop == *prop => {} // unchanged
                _ => {
                    // Changed or new property — use relayout_scope for classification
                    // Pass node_is_ifc_member=true conservatively
                    use azul_css::props::property::RelayoutScope;
                    let scope = prop_type.relayout_scope(true);
                    if scope != RelayoutScope::None {
                        has_layout = true;
                    } else {
                        has_paint = true;
                    }
                }
            }
        }

        // Check for removed properties
        for (prop_type, _) in old_map.iter() {
            if !seen_types.contains_key(prop_type) {
                use azul_css::props::property::RelayoutScope;
                let scope = prop_type.relayout_scope(true);
                if scope != RelayoutScope::None {
                    has_layout = true;
                } else {
                    has_paint = true;
                }
            }
        }

        if has_layout {
            changes.insert(NodeChangeSet::INLINE_STYLE_LAYOUT);
        }
        if has_paint {
            changes.insert(NodeChangeSet::INLINE_STYLE_PAINT);
        }
    }

    // 5. Callbacks
    {
        let old_cbs = old_node.callbacks.as_ref();
        let new_cbs = new_node.callbacks.as_ref();
        if old_cbs.len() != new_cbs.len() {
            changes.insert(NodeChangeSet::CALLBACKS);
        } else {
            for (o, n) in old_cbs.iter().zip(new_cbs.iter()) {
                if o.event != n.event || o.callback != n.callback {
                    changes.insert(NodeChangeSet::CALLBACKS);
                    break;
                }
            }
        }
    }

    // 6. Dataset
    if old_node.get_dataset() != new_node.get_dataset() {
        changes.insert(NodeChangeSet::DATASET);
    }

    // 7. Contenteditable
    if old_node.is_contenteditable() != new_node.is_contenteditable() {
        changes.insert(NodeChangeSet::CONTENTEDITABLE);
    }

    // 8. Tab index
    if old_node.get_tab_index() != new_node.get_tab_index() {
        changes.insert(NodeChangeSet::TAB_INDEX);
    }

    // 9. Styled node state (hover, active, focused, etc.)
    if old_styled_state != new_styled_state {
        changes.insert(NodeChangeSet::STYLED_STATE);
    }

    changes
}

/// Represents a mapping between a node in the old DOM and the new DOM.
#[derive(Debug, Clone, Copy)]
pub struct NodeMove {
    /// The NodeId in the old DOM array
    pub old_node_id: NodeId,
    /// The NodeId in the new DOM array
    pub new_node_id: NodeId,
}

/// The result of a DOM diff, containing lifecycle events and node mappings.
#[derive(Debug, Clone)]
pub struct DiffResult {
    /// Lifecycle events generated by the diff (Mount, Unmount, Resize, Update)
    pub events: Vec<SyntheticEvent>,
    /// Maps Old NodeId -> New NodeId for state migration (focus, scroll, etc.)
    pub node_moves: Vec<NodeMove>,
}

impl Default for DiffResult {
    fn default() -> Self {
        Self {
            events: Vec::new(),
            node_moves: Vec::new(),
        }
    }
}

/// Calculate the reconciliation key for a node using the priority hierarchy:
/// 1. Explicit key (set via `.with_key()`)
/// 2. CSS ID (set via `.with_id("my-id")`)
/// 3. Structural key: nth-of-type-within-parent + parent's reconciliation key
///
/// The structural key prevents incorrect matching when nodes are inserted
/// before existing nodes (e.g., prepending items to a list).
///
/// # Arguments
/// * `node_data` - Slice of all node data
/// * `hierarchy` - Slice of node hierarchy (parent/child relationships)
/// * `node_id` - The node to calculate the key for
///
/// # Returns
/// A 64-bit key that uniquely identifies this node's logical position in the tree.
pub fn calculate_reconciliation_key(
    node_data: &[NodeData],
    hierarchy: &[NodeHierarchyItem],
    node_id: NodeId,
) -> u64 {
    use highway::{HighwayHash, HighwayHasher, Key};
    
    let node = &node_data[node_id.index()];
    
    // Priority 1: Explicit key
    if let Some(key) = node.get_key() {
        return key;
    }
    
    // Priority 2: CSS ID
    for id_or_class in node.ids_and_classes.as_ref().iter() {
        if let IdOrClass::Id(id) = id_or_class {
            let mut hasher = HighwayHasher::new(Key([0; 4]));
            id.as_str().hash(&mut hasher);
            return hasher.finalize64();
        }
    }
    
    // Priority 3: Structural key = nth-of-type-within-parent + parent key
    let mut hasher = HighwayHasher::new(Key([0; 4]));
    
    // Hash node type discriminant and classes (nth-of-type logic)
    core::mem::discriminant(node.get_node_type()).hash(&mut hasher);
    for id_or_class in node.ids_and_classes.as_ref().iter() {
        if let IdOrClass::Class(class) = id_or_class {
            class.as_str().hash(&mut hasher);
        }
    }
    
    // Calculate sibling index (nth-of-type within parent)
    if let Some(hierarchy_item) = hierarchy.get(node_id.index()) {
        if let Some(parent_id) = hierarchy_item.parent_id() {
            // Count siblings of same type before this node
            let mut sibling_index: usize = 0;
            let parent_hierarchy = &hierarchy[parent_id.index()];
            
            // Walk siblings from first child to this node
            let mut current = parent_hierarchy.first_child_id(parent_id);
            while let Some(sibling_id) = current {
                if sibling_id == node_id {
                    break;
                }
                // Check if sibling has same type/classes
                let sibling = &node_data[sibling_id.index()];
                if core::mem::discriminant(sibling.get_node_type()) 
                    == core::mem::discriminant(node.get_node_type()) 
                {
                    sibling_index += 1;
                }
                current = hierarchy[sibling_id.index()].next_sibling_id();
            }
            
            sibling_index.hash(&mut hasher);
            
            // Recursively include parent's key
            let parent_key = calculate_reconciliation_key(node_data, hierarchy, parent_id);
            parent_key.hash(&mut hasher);
        }
    }
    
    hasher.finalize64()
}

/// Precompute reconciliation keys for all nodes in a DOM tree.
///
/// This should be called once before reconciliation to compute stable keys
/// for all nodes. Keys are computed using the hierarchy:
/// 1. Explicit key → 2. CSS ID → 3. Structural key (nth-of-type + parent key)
///
/// # Returns
/// A map from NodeId to its reconciliation key.
pub fn precompute_reconciliation_keys(
    node_data: &[NodeData],
    hierarchy: &[NodeHierarchyItem],
) -> FastHashMap<NodeId, u64> {
    let mut keys = FastHashMap::default();
    for idx in 0..node_data.len() {
        let node_id = NodeId::new(idx);
        let key = calculate_reconciliation_key(node_data, hierarchy, node_id);
        keys.insert(node_id, key);
    }
    keys
}

/// Calculates the difference between two DOM frames and generates lifecycle events.
///
/// This is the main entry point for DOM reconciliation. It compares the old and new
/// DOM trees and produces:
/// - Mount events for new nodes
/// - Unmount events for removed nodes
/// - Resize events for nodes whose bounds changed
/// - Update events for nodes whose content changed (when matched by key)
///
/// # Arguments
/// * `old_node_data` - Node data from the previous frame
/// * `new_node_data` - Node data from the current frame
/// * `old_layout` - Layout bounds from the previous frame
/// * `new_layout` - Layout bounds from the current frame
/// * `dom_id` - The DOM identifier
/// * `timestamp` - Current timestamp for events
pub fn reconcile_dom(
    old_node_data: &[NodeData],
    new_node_data: &[NodeData],
    old_layout: &FastHashMap<NodeId, LogicalRect>,
    new_layout: &FastHashMap<NodeId, LogicalRect>,
    dom_id: DomId,
    timestamp: Instant,
) -> DiffResult {
    let mut result = DiffResult::default();

    // --- STEP 1: INDEX THE OLD DOM ---
    // Create lookups to find old nodes by Key or by Hash.
    // 
    // IMPORTANT: We use TWO hash indexes:
    // 1. Content Hash (calculate_node_data_hash) - for exact matching including text content
    // 2. Structural Hash (calculate_structural_hash) - for text nodes where content may change
    //
    // This allows Text("Hello") to match Text("Hello World") as a structural match,
    // preserving cursor/selection state during text editing.

    let mut old_keyed: FastHashMap<u64, NodeId> = FastHashMap::default();
    let mut old_hashed: FastHashMap<DomNodeHash, VecDeque<NodeId>> = FastHashMap::default();
    let mut old_structural: FastHashMap<DomNodeHash, VecDeque<NodeId>> = FastHashMap::default();
    let mut old_nodes_consumed = vec![false; old_node_data.len()];

    for (idx, node) in old_node_data.iter().enumerate() {
        let id = NodeId::new(idx);

        if let Some(key) = node.get_key() {
            // Priority 1: Explicit Key
            old_keyed.insert(key, id);
        } else {
            // Priority 2: Content Hash (exact match)
            let hash = node.calculate_node_data_hash();
            old_hashed.entry(hash).or_default().push_back(id);
            
            // Priority 3: Structural Hash (for text node matching)
            let structural_hash = node.calculate_structural_hash();
            old_structural.entry(structural_hash).or_default().push_back(id);
        }
    }

    // --- STEP 2: ITERATE NEW DOM AND CLAIM MATCHES ---

    for (new_idx, new_node) in new_node_data.iter().enumerate() {
        let new_id = NodeId::new(new_idx);
        let mut matched_old_id = None;

        // A. Try Match by Key
        if let Some(key) = new_node.get_key() {
            if let Some(&old_id) = old_keyed.get(&key) {
                if !old_nodes_consumed[old_id.index()] {
                    matched_old_id = Some(old_id);
                }
            }
        }
        // B. Try Match by Content Hash first (exact match - The "Automagic" Reordering)
        else {
            let hash = new_node.calculate_node_data_hash();

            // Get the queue of old nodes with this identical content
            if let Some(queue) = old_hashed.get_mut(&hash) {
                // Find first non-consumed node in queue
                while let Some(old_id) = queue.front() {
                    if !old_nodes_consumed[old_id.index()] {
                        matched_old_id = Some(*old_id);
                        queue.pop_front();
                        break;
                    } else {
                        queue.pop_front();
                    }
                }
            }
            
            // C. If no exact match, try Structural Hash (for text nodes with changed content)
            if matched_old_id.is_none() {
                let structural_hash = new_node.calculate_structural_hash();
                if let Some(queue) = old_structural.get_mut(&structural_hash) {
                    while let Some(old_id) = queue.front() {
                        if !old_nodes_consumed[old_id.index()] {
                            matched_old_id = Some(*old_id);
                            queue.pop_front();
                            break;
                        } else {
                            queue.pop_front();
                        }
                    }
                }
            }
        }

        // --- STEP 3: PROCESS MATCH OR MOUNT ---

        if let Some(old_id) = matched_old_id {
            // FOUND A MATCH (It might be at a different index, but it's the "same" node)

            old_nodes_consumed[old_id.index()] = true;
            result.node_moves.push(NodeMove {
                old_node_id: old_id,
                new_node_id: new_id,
            });

            // Check for Resize
            let old_rect = old_layout.get(&old_id).copied().unwrap_or(LogicalRect::zero());
            let new_rect = new_layout.get(&new_id).copied().unwrap_or(LogicalRect::zero());

            if old_rect.size != new_rect.size {
                // Fire Resize Event
                if has_resize_callback(new_node) {
                    result.events.push(create_lifecycle_event(
                        EventType::Resize,
                        new_id,
                        dom_id,
                        &timestamp,
                        LifecycleEventData {
                            reason: LifecycleReason::Resize,
                            previous_bounds: Some(old_rect),
                            current_bounds: new_rect,
                        },
                    ));
                }
            }

            // If matched by Key, the content might have changed, so we should check hash equality.
            if new_node.get_key().is_some() {
                let old_hash = old_node_data[old_id.index()].calculate_node_data_hash();
                let new_hash = new_node.calculate_node_data_hash();

                if old_hash != new_hash && has_update_callback(new_node) {
                    result.events.push(create_lifecycle_event(
                        EventType::Update,
                        new_id,
                        dom_id,
                        &timestamp,
                        LifecycleEventData {
                            reason: LifecycleReason::Update,
                            previous_bounds: Some(old_rect),
                            current_bounds: new_rect,
                        },
                    ));
                }
            }
        } else {
            // NO MATCH FOUND -> MOUNT (New Node)
            if has_mount_callback(new_node) {
                let bounds = new_layout.get(&new_id).copied().unwrap_or(LogicalRect::zero());
                result.events.push(create_lifecycle_event(
                    EventType::Mount,
                    new_id,
                    dom_id,
                    &timestamp,
                    LifecycleEventData {
                        reason: LifecycleReason::InitialMount,
                        previous_bounds: None,
                        current_bounds: bounds,
                    },
                ));
            }
        }
    }

    // --- STEP 4: CLEANUP (UNMOUNTS) ---
    // Any old node that wasn't claimed is effectively destroyed.

    for (old_idx, consumed) in old_nodes_consumed.iter().enumerate() {
        if !consumed {
            let old_id = NodeId::new(old_idx);
            let old_node = &old_node_data[old_idx];

            if has_unmount_callback(old_node) {
                let bounds = old_layout.get(&old_id).copied().unwrap_or(LogicalRect::zero());
                result.events.push(create_lifecycle_event(
                    EventType::Unmount,
                    old_id,
                    dom_id,
                    &timestamp,
                    LifecycleEventData {
                        reason: LifecycleReason::InitialMount, // Context implies unmount
                        previous_bounds: Some(bounds),
                        current_bounds: LogicalRect::zero(),
                    },
                ));
            }
        }
    }

    result
}

/// Creates a lifecycle event with all necessary fields.
fn create_lifecycle_event(
    event_type: EventType,
    node_id: NodeId,
    dom_id: DomId,
    timestamp: &Instant,
    data: LifecycleEventData,
) -> SyntheticEvent {
    let dom_node_id = DomNodeId {
        dom: dom_id,
        node: NodeHierarchyItemId::from_crate_internal(Some(node_id)),
    };
    SyntheticEvent {
        event_type,
        source: EventSource::Lifecycle,
        phase: EventPhase::Target,
        target: dom_node_id,
        current_target: dom_node_id,
        timestamp: timestamp.clone(),
        data: EventData::Lifecycle(data),
        stopped: false,
        stopped_immediate: false,
        prevented_default: false,
    }
}

/// Check if the node has an AfterMount callback registered.
fn has_mount_callback(node: &NodeData) -> bool {
    node.get_callbacks().iter().any(|cb| {
        matches!(
            cb.event,
            EventFilter::Component(ComponentEventFilter::AfterMount)
        )
    })
}

/// Check if the node has a BeforeUnmount callback registered.
fn has_unmount_callback(node: &NodeData) -> bool {
    node.get_callbacks().iter().any(|cb| {
        matches!(
            cb.event,
            EventFilter::Component(ComponentEventFilter::BeforeUnmount)
        )
    })
}

/// Check if the node has a NodeResized callback registered.
fn has_resize_callback(node: &NodeData) -> bool {
    node.get_callbacks().iter().any(|cb| {
        matches!(
            cb.event,
            EventFilter::Component(ComponentEventFilter::NodeResized)
        )
    })
}

/// Check if the node has any lifecycle callback that would respond to updates.
fn has_update_callback(node: &NodeData) -> bool {
    // For now, we use Selected as a placeholder for "update" events
    // This could be extended to a dedicated UpdateCallback in the future
    node.get_callbacks().iter().any(|cb| {
        matches!(
            cb.event,
            EventFilter::Component(ComponentEventFilter::Selected)
        )
    })
}

/// Migrate state (focus, scroll, etc.) from old node IDs to new node IDs.
///
/// This function should be called after reconciliation to update any state
/// that references old NodeIds to use the new NodeIds.
///
/// # Example
/// ```rust,ignore
/// let diff = reconcile_dom(...);
/// let migration_map = create_migration_map(&diff.node_moves);
/// 
/// // Migrate focus
/// if let Some(current_focus) = focus_manager.focused_node {
///     if let Some(&new_id) = migration_map.get(&current_focus) {
///         focus_manager.focused_node = Some(new_id);
///     } else {
///         // Focused node was unmounted, clear focus
///         focus_manager.focused_node = None;
///     }
/// }
/// ```
pub fn create_migration_map(node_moves: &[NodeMove]) -> FastHashMap<NodeId, NodeId> {
    let mut map = FastHashMap::default();
    for m in node_moves {
        map.insert(m.old_node_id, m.new_node_id);
    }
    map
}

/// Executes state migration between the old DOM and the new DOM based on diff results.
///
/// This iterates through matched nodes. If a match has BOTH a merge callback AND a dataset,
/// it executes the callback to transfer state from the old node to the new node.
///
/// This must be called **before** the old DOM is dropped, because we need to access its data.
///
/// # Arguments
/// * `old_node_data` - Mutable reference to the old DOM's node data (source of heavy state)
/// * `new_node_data` - Mutable reference to the new DOM's node data (target for heavy state)
/// * `node_moves` - The matched nodes from the reconciliation diff
///
/// # Example
/// ```rust,ignore
/// let diff_result = reconcile_dom(&old_data, &new_data, ...);
/// 
/// // Execute state migration BEFORE old_dom is dropped
/// transfer_states(&mut old_data, &mut new_data, &diff_result.node_moves);
/// 
/// // Now safe to drop old_dom - heavy resources have been transferred
/// drop(old_dom);
/// ```
pub fn transfer_states(
    old_node_data: &mut [NodeData],
    new_node_data: &mut [NodeData],
    node_moves: &[NodeMove],
) {
    use crate::refany::OptionRefAny;

    for movement in node_moves {
        let old_idx = movement.old_node_id.index();
        let new_idx = movement.new_node_id.index();

        // Bounds check
        if old_idx >= old_node_data.len() || new_idx >= new_node_data.len() {
            continue;
        }

        // 1. Check if the NEW node has requested a merge callback
        let merge_callback = match new_node_data[new_idx].get_merge_callback() {
            Some(cb) => cb,
            None => continue, // No merge callback, skip
        };

        // 2. Check if BOTH nodes have datasets
        // We need to temporarily take the datasets to satisfy borrow checker
        let old_dataset = old_node_data[old_idx].take_dataset();
        let new_dataset = new_node_data[new_idx].take_dataset();

        match (new_dataset, old_dataset) {
            (Some(new_data), Some(old_data)) => {
                // 3. EXECUTE THE MERGE CALLBACK
                // The callback receives both datasets and returns the merged result
                let merged = (merge_callback.cb)(new_data, old_data);
                
                // 4. Store the merged result back in the new node
                new_node_data[new_idx].set_dataset(OptionRefAny::Some(merged));
            }
            (new_ds, old_ds) => {
                // One or both datasets missing - restore what we had
                if let Some(ds) = new_ds {
                    new_node_data[new_idx].set_dataset(OptionRefAny::Some(ds));
                }
                if let Some(ds) = old_ds {
                    old_node_data[old_idx].set_dataset(OptionRefAny::Some(ds));
                }
            }
        }
    }
}

/// Calculate a stable key for a contenteditable node using the hierarchy:
///
/// 1. **Explicit Key** - If `.with_key()` was called, use that
/// 2. **CSS ID** - If the node has a CSS ID (e.g., `#my-editor`), hash that
/// 3. **Structural Key** - Hash of `(nth-of-type, parent_key)` recursively
///
/// The structural key prevents shifting when elements are inserted before siblings.
/// For example, in `<div><p>A</p><p contenteditable>B</p></div>`, if we insert
/// a new `<p>` at the start, the contenteditable `<p>` becomes nth-child(3) but
/// its nth-of-type stays stable (it's still the 2nd `<p>`).
///
/// # Arguments
/// * `node_data` - All nodes in the DOM
/// * `hierarchy` - Parent-child relationships
/// * `node_id` - The node to calculate the key for
///
/// # Returns
/// A stable u64 key for the node
pub fn calculate_contenteditable_key(
    node_data: &[NodeData],
    hierarchy: &[crate::styled_dom::NodeHierarchyItem],
    node_id: NodeId,
) -> u64 {
    use highway::{HighwayHash, HighwayHasher, Key};
    use crate::dom::IdOrClass;
    
    let node = &node_data[node_id.index()];
    
    // Priority 1: Explicit key (from .with_key())
    if let Some(explicit_key) = node.get_key() {
        return explicit_key;
    }
    
    // Priority 2: CSS ID
    for id_or_class in node.get_ids_and_classes().as_ref().iter() {
        if let IdOrClass::Id(id) = id_or_class {
            let mut hasher = HighwayHasher::new(Key([1; 4])); // Different seed for ID keys
            hasher.append(id.as_str().as_bytes());
            return hasher.finalize64();
        }
    }
    
    // Priority 3: Structural key = (nth-of-type, classes, parent_key)
    let mut hasher = HighwayHasher::new(Key([2; 4])); // Different seed for structural keys
    
    // Get parent and calculate its key recursively
    let parent_key = if let Some(parent_id) = hierarchy.get(node_id.index()).and_then(|h| h.parent_id()) {
        calculate_contenteditable_key(node_data, hierarchy, parent_id)
    } else {
        0u64 // Root node
    };
    hasher.append(&parent_key.to_le_bytes());
    
    // Calculate nth-of-type (count siblings of same node type before this one)
    // We compare discriminants directly without hashing
    let node_discriminant = core::mem::discriminant(node.get_node_type());
    let nth_of_type = if let Some(parent_id) = hierarchy.get(node_id.index()).and_then(|h| h.parent_id()) {
        // Count siblings with same node type that come before this node
        let mut count = 0u32;
        let mut sibling_id = hierarchy.get(parent_id.index()).and_then(|h| h.first_child_id(parent_id));
        while let Some(sib_id) = sibling_id {
            if sib_id == node_id {
                break;
            }
            let sibling_discriminant = core::mem::discriminant(node_data[sib_id.index()].get_node_type());
            if sibling_discriminant == node_discriminant {
                count += 1;
            }
            sibling_id = hierarchy.get(sib_id.index()).and_then(|h| h.next_sibling_id());
        }
        count
    } else {
        0
    };
    
    hasher.append(&nth_of_type.to_le_bytes());
    
    // Hash the node type using its Debug representation as a stable identifier
    // This works because NodeType implements Debug
    #[cfg(feature = "std")]
    {
        let type_str = format!("{:?}", node_discriminant);
        hasher.append(type_str.as_bytes());
    }
    #[cfg(not(feature = "std"))]
    {
        // For no_std, use the memory representation of the discriminant
        // NodeType variants are numbered 0..N, and discriminant stores this
        let discriminant_bytes: [u8; core::mem::size_of::<core::mem::Discriminant<crate::dom::NodeType>>()] = 
            unsafe { core::mem::transmute(node_discriminant) };
        hasher.append(&discriminant_bytes);
    }
    
    // Also hash the classes for additional stability
    for id_or_class in node.get_ids_and_classes().as_ref().iter() {
        if let IdOrClass::Class(class) = id_or_class {
            hasher.append(class.as_str().as_bytes());
        }
    }
    
    hasher.finalize64()
}

/// Reconcile cursor byte position when text content changes.
///
/// This function maps a cursor position from old text to new text, preserving
/// the cursor's logical position as much as possible:
///
/// 1. If cursor is in unchanged prefix → stays at same byte offset
/// 2. If cursor is in unchanged suffix → adjusts by length difference
/// 3. If cursor is in changed region → places at end of new content
///
/// # Arguments
/// * `old_text` - The previous text content
/// * `new_text` - The new text content
/// * `old_cursor_byte` - Cursor byte offset in old text
///
/// # Returns
/// The reconciled cursor byte offset in new text
///
/// # Example
/// ```rust,ignore
/// let old_text = "Hello";
/// let new_text = "Hello World";
/// let old_cursor = 5; // cursor at end of "Hello"
/// let new_cursor = reconcile_cursor_position(old_text, new_text, old_cursor);
/// assert_eq!(new_cursor, 5); // cursor stays at same position (prefix unchanged)
/// ```
pub fn reconcile_cursor_position(
    old_text: &str,
    new_text: &str,
    old_cursor_byte: usize,
) -> usize {
    // If texts are equal, cursor is unchanged
    if old_text == new_text {
        return old_cursor_byte;
    }
    
    // Empty old text - place cursor at end of new text
    if old_text.is_empty() {
        return new_text.len();
    }
    
    // Empty new text - place cursor at 0
    if new_text.is_empty() {
        return 0;
    }
    
    // Find common prefix (how many bytes from the start are identical)
    let common_prefix_bytes = old_text
        .bytes()
        .zip(new_text.bytes())
        .take_while(|(a, b)| a == b)
        .count();
    
    // If cursor was in the unchanged prefix, it stays at the same byte offset
    if old_cursor_byte <= common_prefix_bytes {
        return old_cursor_byte.min(new_text.len());
    }
    
    // Find common suffix (how many bytes from the end are identical)
    let common_suffix_bytes = old_text
        .bytes()
        .rev()
        .zip(new_text.bytes().rev())
        .take_while(|(a, b)| a == b)
        .count();
    
    // Calculate where the suffix starts in old and new text
    let old_suffix_start = old_text.len().saturating_sub(common_suffix_bytes);
    let new_suffix_start = new_text.len().saturating_sub(common_suffix_bytes);
    
    // If cursor was in the unchanged suffix, adjust by length difference
    if old_cursor_byte >= old_suffix_start {
        let offset_from_end = old_text.len() - old_cursor_byte;
        return new_text.len().saturating_sub(offset_from_end);
    }
    
    // Cursor was in the changed region - place at end of inserted content
    // This handles insertions (cursor moves with new text) and deletions (cursor at edit point)
    new_suffix_start
}

/// Get the text content from a NodeData if it's a Text node.
///
/// Returns the text string if the node is `NodeType::Text`, otherwise `None`.
pub fn get_node_text_content(node: &NodeData) -> Option<&str> {
    if let crate::dom::NodeType::Text(ref text) = node.get_node_type() {
        Some(text.as_str())
    } else {
        None
    }
}

// ============================================================================
// ChangeAccumulator — unifies all change input paths
// ============================================================================

/// Text change info for cursor/selection reconciliation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextChange {
    pub old_text: String,
    pub new_text: String,
}

/// Per-node change report combining multiple information sources.
#[derive(Debug, Clone, Default)]
pub struct NodeChangeReport {
    /// Bitflags from DOM-level field comparison.
    pub change_set: NodeChangeSet,

    /// Highest RelayoutScope from any CSS property that changed on this node.
    /// This is more granular than NodeChangeSet's binary LAYOUT/PAINT split.
    ///
    /// - `None` → repaint only (color, opacity, transform)
    /// - `IfcOnly` → reshape text in the containing IFC
    /// - `SizingOnly` → recompute this node's intrinsic size
    /// - `Full` → full subtree relayout (display, position, float, etc.)
    pub relayout_scope: RelayoutScope,

    /// Individual CSS properties that changed (for fine-grained cache invalidation).
    /// Empty if the change was structural (text content, node type, etc.)
    pub changed_css_properties: Vec<CssPropertyType>,

    /// If text content changed, the old and new text for cursor reconciliation.
    pub text_change: Option<TextChange>,
}

impl NodeChangeReport {
    /// Returns the DirtyFlag level needed for this change report.
    /// Maps RelayoutScope + NodeChangeSet → a simple tri-state.
    pub fn needs_layout(&self) -> bool {
        self.change_set.needs_layout() || self.relayout_scope > RelayoutScope::None
    }

    pub fn needs_paint(&self) -> bool {
        self.change_set.needs_paint()
    }

    pub fn is_visually_unchanged(&self) -> bool {
        self.change_set.is_visually_unchanged() && self.relayout_scope == RelayoutScope::None
    }
}

/// Unified change report that merges information from all three change paths:
///
/// 1. **DOM reconciliation** (`compute_node_changes` after `reconcile_dom`)
/// 2. **CSS restyle** (`restyle_on_state_change` for hover/focus/active)
/// 3. **Runtime edits** (`words_changed`, `css_properties_changed`, `images_changed`)
///
/// This is the single source of truth for "what work needs to happen this frame".
#[derive(Debug, Clone, Default)]
pub struct ChangeAccumulator {
    /// Per-node change info. Key is the new-DOM NodeId.
    pub per_node: BTreeMap<NodeId, NodeChangeReport>,

    /// Maximum RelayoutScope across all changed nodes.
    /// Quick check: if this is `None`, we can skip layout entirely.
    pub max_scope: RelayoutScope,

    /// Nodes that are newly mounted (no old counterpart).
    /// These always need full layout.
    pub mounted_nodes: Vec<NodeId>,

    /// Nodes that were unmounted (no new counterpart).
    /// Used for cleanup (remove from scroll/focus/cursor managers).
    pub unmounted_nodes: Vec<NodeId>,
}

impl ChangeAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if no changes were detected at all.
    pub fn is_empty(&self) -> bool {
        self.per_node.is_empty() && self.mounted_nodes.is_empty() && self.unmounted_nodes.is_empty()
    }

    /// Returns true if layout work is needed (any node has scope > None).
    pub fn needs_layout(&self) -> bool {
        self.max_scope > RelayoutScope::None
            || !self.mounted_nodes.is_empty()
            || self.per_node.values().any(|r| r.needs_layout())
    }

    /// Returns true if only paint work is needed (no layout).
    pub fn needs_paint_only(&self) -> bool {
        !self.needs_layout() && self.per_node.values().any(|r| r.needs_paint())
    }

    /// Returns true if only non-visual changes occurred (callbacks, dataset, a11y).
    pub fn is_visually_unchanged(&self) -> bool {
        self.mounted_nodes.is_empty()
            && self.unmounted_nodes.is_empty()
            && self.max_scope == RelayoutScope::None
            && self.per_node.values().all(|r| r.is_visually_unchanged())
    }

    /// Add a node change from DOM reconciliation (Path A).
    pub fn add_dom_change(
        &mut self,
        new_node_id: NodeId,
        change_set: NodeChangeSet,
        relayout_scope: RelayoutScope,
        text_change: Option<TextChange>,
        changed_css_properties: Vec<CssPropertyType>,
    ) {
        if relayout_scope > self.max_scope {
            self.max_scope = relayout_scope;
        }

        let report = self.per_node.entry(new_node_id).or_default();
        report.change_set |= change_set;
        if relayout_scope > report.relayout_scope {
            report.relayout_scope = relayout_scope;
        }
        if text_change.is_some() {
            report.text_change = text_change;
        }
        report.changed_css_properties.extend(changed_css_properties);
    }

    /// Add a text change (from runtime edit or DOM reconciliation).
    pub fn add_text_change(
        &mut self,
        node_id: NodeId,
        old_text: String,
        new_text: String,
    ) {
        let scope = RelayoutScope::IfcOnly;
        if scope > self.max_scope {
            self.max_scope = scope;
        }

        let report = self.per_node.entry(node_id).or_default();
        report.change_set.insert(NodeChangeSet::TEXT_CONTENT);
        if scope > report.relayout_scope {
            report.relayout_scope = scope;
        }
        report.text_change = Some(TextChange { old_text, new_text });
    }

    /// Add a CSS property change (from runtime edit or restyle).
    pub fn add_css_change(
        &mut self,
        node_id: NodeId,
        prop_type: CssPropertyType,
        scope: RelayoutScope,
    ) {
        if scope > self.max_scope {
            self.max_scope = scope;
        }

        let report = self.per_node.entry(node_id).or_default();
        if scope > RelayoutScope::None {
            report.change_set.insert(NodeChangeSet::INLINE_STYLE_LAYOUT);
        } else {
            report.change_set.insert(NodeChangeSet::INLINE_STYLE_PAINT);
        }
        if scope > report.relayout_scope {
            report.relayout_scope = scope;
        }
        report.changed_css_properties.push(prop_type);
    }

    /// Add an image change (from runtime edit or DOM reconciliation).
    pub fn add_image_change(
        &mut self,
        node_id: NodeId,
        scope: RelayoutScope,
    ) {
        if scope > self.max_scope {
            self.max_scope = scope;
        }

        let report = self.per_node.entry(node_id).or_default();
        report.change_set.insert(NodeChangeSet::IMAGE_CHANGED);
        if scope > report.relayout_scope {
            report.relayout_scope = scope;
        }
    }

    /// Add a mounted (new) node.
    pub fn add_mount(&mut self, node_id: NodeId) {
        self.mounted_nodes.push(node_id);
    }

    /// Add an unmounted (removed) node.
    pub fn add_unmount(&mut self, node_id: NodeId) {
        self.unmounted_nodes.push(node_id);
    }

    /// Merge a `RestyleResult` (from `restyle_on_state_change()`) into this accumulator.
    ///
    /// This is the bridge between Path B (restyle) and the unified change pipeline.
    /// Each `ChangedCssProperty` is classified via `relayout_scope()` to determine
    /// whether it affects layout or only paint.
    pub fn merge_restyle_result(&mut self, restyle: &crate::styled_dom::RestyleResult) {
        for (node_id, changed_props) in &restyle.changed_nodes {
            for changed in changed_props {
                let prop_type = changed.current_prop.get_type();
                let scope = prop_type.relayout_scope(true); // conservative
                self.add_css_change(*node_id, prop_type, scope);
            }
        }
    }

    /// Populate this accumulator from an `ExtendedDiffResult` + the old/new DOM data.
    ///
    /// This converts per-node `NodeChangeSet` flags into full `NodeChangeReport`s
    /// with `RelayoutScope` classification.
    pub fn merge_extended_diff(
        &mut self,
        extended: &ExtendedDiffResult,
        old_node_data: &[NodeData],
        new_node_data: &[NodeData],
    ) {
        for &(old_id, new_id, ref change_set) in &extended.node_changes {
            if change_set.is_empty() {
                continue;
            }

            // Determine RelayoutScope from the change flags
            let scope = self.classify_change_scope(change_set, new_node_data, new_id);

            // Extract text change info if TEXT_CONTENT flag is set
            let text_change = if change_set.contains(NodeChangeSet::TEXT_CONTENT) {
                let old_text = get_node_text_content(&old_node_data[old_id.index()])
                    .unwrap_or("")
                    .to_string();
                let new_text = get_node_text_content(&new_node_data[new_id.index()])
                    .unwrap_or("")
                    .to_string();
                Some(TextChange { old_text, new_text })
            } else {
                None
            };

            self.add_dom_change(new_id, *change_set, scope, text_change, Vec::new());
        }

        // Track mounts: new nodes that didn't match anything in old
        let matched_new: alloc::collections::BTreeSet<usize> = extended
            .diff
            .node_moves
            .iter()
            .map(|m| m.new_node_id.index())
            .collect();

        for idx in 0..new_node_data.len() {
            if !matched_new.contains(&idx) {
                self.add_mount(NodeId::new(idx));
            }
        }

        // Track unmounts: old nodes that didn't match anything in new
        let matched_old: alloc::collections::BTreeSet<usize> = extended
            .diff
            .node_moves
            .iter()
            .map(|m| m.old_node_id.index())
            .collect();

        for idx in 0..old_node_data.len() {
            if !matched_old.contains(&idx) {
                self.add_unmount(NodeId::new(idx));
            }
        }
    }

    /// Classify a NodeChangeSet into the appropriate RelayoutScope.
    fn classify_change_scope(
        &self,
        change_set: &NodeChangeSet,
        new_node_data: &[NodeData],
        new_node_id: NodeId,
    ) -> RelayoutScope {
        // NODE_TYPE_CHANGED or CHILDREN_CHANGED → Full
        if change_set.contains(NodeChangeSet::NODE_TYPE_CHANGED)
            || change_set.contains(NodeChangeSet::CHILDREN_CHANGED)
        {
            return RelayoutScope::Full;
        }

        // IDS_AND_CLASSES → Full (conservative: class change may add layout-affecting CSS)
        if change_set.contains(NodeChangeSet::IDS_AND_CLASSES) {
            return RelayoutScope::Full;
        }

        // INLINE_STYLE_LAYOUT → could be IfcOnly, SizingOnly, or Full
        // We need to check individual properties for the exact scope.
        // For now, we use SizingOnly as a conservative default since
        // the individual property scopes were already checked in compute_node_changes.
        if change_set.contains(NodeChangeSet::INLINE_STYLE_LAYOUT) {
            // Walk the inline CSS properties to find the max scope
            let new_node = &new_node_data[new_node_id.index()];
            let mut max_scope = RelayoutScope::None;
            for prop in new_node.css_props.as_ref().iter() {
                let scope = prop.property.get_type().relayout_scope(true);
                if scope > max_scope {
                    max_scope = scope;
                }
            }
            return if max_scope == RelayoutScope::None {
                RelayoutScope::SizingOnly // conservative fallback
            } else {
                max_scope
            };
        }

        // TEXT_CONTENT → IfcOnly (reshape text, may cascade)
        if change_set.contains(NodeChangeSet::TEXT_CONTENT) {
            return RelayoutScope::IfcOnly;
        }

        // IMAGE_CHANGED → SizingOnly (intrinsic size may change)
        if change_set.contains(NodeChangeSet::IMAGE_CHANGED) {
            return RelayoutScope::SizingOnly;
        }

        // CONTENTEDITABLE → SizingOnly
        if change_set.contains(NodeChangeSet::CONTENTEDITABLE) {
            return RelayoutScope::SizingOnly;
        }

        // Paint-only or no-visual changes
        if change_set.intersects(NodeChangeSet::AFFECTS_PAINT) {
            return RelayoutScope::None;
        }

        RelayoutScope::None
    }
}

/// Perform a full reconciliation with change detection.
///
/// This combines `reconcile_dom()` + `compute_node_changes()` into a single
/// pass that produces an `ExtendedDiffResult` with per-node change flags.
///
/// The `ChangeAccumulator` can then be populated from the result via
/// `accumulator.merge_extended_diff()`.
pub fn reconcile_dom_with_changes(
    old_node_data: &[NodeData],
    new_node_data: &[NodeData],
    old_styled_nodes: Option<&[StyledNodeState]>,
    new_styled_nodes: Option<&[StyledNodeState]>,
    old_layout: &FastHashMap<NodeId, LogicalRect>,
    new_layout: &FastHashMap<NodeId, LogicalRect>,
    dom_id: DomId,
    timestamp: Instant,
) -> ExtendedDiffResult {
    // Step 1: Run standard reconciliation
    let diff = reconcile_dom(
        old_node_data,
        new_node_data,
        old_layout,
        new_layout,
        dom_id,
        timestamp,
    );

    // Step 2: For each matched pair, compute what changed
    let mut node_changes = Vec::new();
    for node_move in &diff.node_moves {
        let old_nd = &old_node_data[node_move.old_node_id.index()];
        let new_nd = &new_node_data[node_move.new_node_id.index()];

        let old_state = old_styled_nodes.and_then(|s| s.get(node_move.old_node_id.index()));
        let new_state = new_styled_nodes.and_then(|s| s.get(node_move.new_node_id.index()));

        let changes = compute_node_changes(old_nd, new_nd, old_state, new_state);
        node_changes.push((node_move.old_node_id, node_move.new_node_id, changes));
    }

    ExtendedDiffResult { diff, node_changes }
}

// ============================================================================
// NodeDataFingerprint — multi-field hash for fast change detection
// ============================================================================

/// Per-node hash broken into independent fields for fast change detection.
///
/// Instead of a single u64 hash (which loses all granularity), this stores
/// separate hashes per field category. Comparing two fingerprints is O(1)
/// (6 integer comparisons) and immediately tells us WHICH category changed,
/// avoiding the more expensive `compute_node_changes()` for unchanged nodes.
///
/// Two-tier strategy:
/// - **Tier 1** (this struct): O(1) per node, identifies which categories changed.
/// - **Tier 2** (`compute_node_changes`): O(n) per changed field, does field-by-field
///   comparison only for nodes that Tier 1 identified as changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeDataFingerprint {
    /// Hash of node_type (Text content, Image ref, Div, etc.)
    pub content_hash: u64,
    /// Hash of styled_node_state (hover, focus, active bits)
    pub state_hash: u64,
    /// Hash of inline CSS properties
    pub inline_css_hash: u64,
    /// Hash of ids_and_classes
    pub ids_classes_hash: u64,
    /// Hash of callbacks (event types + function pointers)
    pub callbacks_hash: u64,
    /// Hash of other attributes (contenteditable, tab_index, dataset)
    pub attrs_hash: u64,
}

impl Default for NodeDataFingerprint {
    fn default() -> Self {
        Self {
            content_hash: 0,
            state_hash: 0,
            inline_css_hash: 0,
            ids_classes_hash: 0,
            callbacks_hash: 0,
            attrs_hash: 0,
        }
    }
}

impl NodeDataFingerprint {
    /// Compute a fingerprint from a node's data and styled state.
    pub fn compute(node: &NodeData, styled_state: Option<&StyledNodeState>) -> Self {
        use highway::{HighwayHash, HighwayHasher, Key};
        use core::hash::Hash;

        // Content hash
        let content_hash = {
            let mut h = HighwayHasher::new(Key([1; 4]));
            node.get_node_type().hash(&mut h);
            h.finalize64()
        };

        // State hash
        let state_hash = {
            let mut h = HighwayHasher::new(Key([2; 4]));
            if let Some(state) = styled_state {
                state.hash(&mut h);
            }
            h.finalize64()
        };

        // Inline CSS hash
        let inline_css_hash = {
            let mut h = HighwayHasher::new(Key([3; 4]));
            for prop in node.css_props.as_ref().iter() {
                prop.hash(&mut h);
            }
            h.finalize64()
        };

        // IDs and classes hash
        let ids_classes_hash = {
            let mut h = HighwayHasher::new(Key([4; 4]));
            for id_or_class in node.ids_and_classes.as_ref().iter() {
                id_or_class.hash(&mut h);
            }
            h.finalize64()
        };

        // Callbacks hash
        let callbacks_hash = {
            let mut h = HighwayHasher::new(Key([5; 4]));
            for cb in node.callbacks.as_ref().iter() {
                cb.event.hash(&mut h);
                cb.callback.hash(&mut h);
            }
            h.finalize64()
        };

        // Attributes hash
        let attrs_hash = {
            let mut h = HighwayHasher::new(Key([6; 4]));
            node.is_contenteditable().hash(&mut h);
            node.flags.hash(&mut h);
            node.get_dataset().hash(&mut h);
            h.finalize64()
        };

        Self {
            content_hash,
            state_hash,
            inline_css_hash,
            ids_classes_hash,
            callbacks_hash,
            attrs_hash,
        }
    }

    /// Returns a quick NodeChangeSet by comparing two fingerprints.
    /// This is O(1) — just comparing 6 u64s.
    ///
    /// The result is *conservative*: if a field hash differs, we set the
    /// broadest applicable flag. For precise classification (e.g., which
    /// CSS properties changed and their `relayout_scope()`), the caller
    /// should fall back to `compute_node_changes()` for changed nodes.
    pub fn diff(&self, other: &NodeDataFingerprint) -> NodeChangeSet {
        let mut changes = NodeChangeSet::empty();

        if self.content_hash != other.content_hash {
            // Could be TEXT_CONTENT, IMAGE_CHANGED, or NODE_TYPE_CHANGED
            // We set both TEXT_CONTENT and IMAGE_CHANGED conservatively;
            // compute_node_changes() will refine this.
            changes.insert(NodeChangeSet::TEXT_CONTENT);
            changes.insert(NodeChangeSet::IMAGE_CHANGED);
        }

        if self.state_hash != other.state_hash {
            changes.insert(NodeChangeSet::STYLED_STATE);
        }

        if self.inline_css_hash != other.inline_css_hash {
            // Conservative: inline CSS could affect layout or paint.
            // compute_node_changes() checks relayout_scope() per property.
            changes.insert(NodeChangeSet::INLINE_STYLE_LAYOUT);
        }

        if self.ids_classes_hash != other.ids_classes_hash {
            changes.insert(NodeChangeSet::IDS_AND_CLASSES);
        }

        if self.callbacks_hash != other.callbacks_hash {
            changes.insert(NodeChangeSet::CALLBACKS);
        }

        if self.attrs_hash != other.attrs_hash {
            changes.insert(NodeChangeSet::TAB_INDEX);
            changes.insert(NodeChangeSet::CONTENTEDITABLE);
        }

        changes
    }

    /// Returns true if the fingerprint is identical (no changes at all).
    pub fn is_identical(&self, other: &NodeDataFingerprint) -> bool {
        self == other
    }

    /// Quick check: could this change affect layout?
    pub fn might_affect_layout(&self, other: &NodeDataFingerprint) -> bool {
        self.content_hash != other.content_hash
            || self.inline_css_hash != other.inline_css_hash
            || self.ids_classes_hash != other.ids_classes_hash
            || self.attrs_hash != other.attrs_hash
    }

    /// Quick check: could this change affect visuals at all?
    pub fn might_affect_visuals(&self, other: &NodeDataFingerprint) -> bool {
        self.content_hash != other.content_hash
            || self.state_hash != other.state_hash
            || self.inline_css_hash != other.inline_css_hash
            || self.ids_classes_hash != other.ids_classes_hash
    }
}
