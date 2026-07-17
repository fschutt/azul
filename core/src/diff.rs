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

use alloc::{collections::BTreeMap, collections::VecDeque, string::{String, ToString}, vec::Vec};
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
    OrderedMap,
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

    /// Callbacks changed (new `RefAny`, different event handlers).
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

    #[must_use] pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    #[must_use] pub const fn is_empty(&self) -> bool {
        self.bits == 0
    }

    #[must_use] pub const fn contains(&self, flag: u32) -> bool {
        (self.bits & flag) == flag
    }

    #[must_use] pub const fn intersects(&self, mask: u32) -> bool {
        (self.bits & mask) != 0
    }

    pub const fn insert(&mut self, flag: u32) {
        self.bits |= flag;
    }

    /// Returns true if no visual change occurred (only callbacks/dataset/a11y).
    #[must_use] pub const fn is_visually_unchanged(&self) -> bool {
        !self.intersects(Self::AFFECTS_LAYOUT) && !self.intersects(Self::AFFECTS_PAINT)
    }

    /// Returns true if layout is needed.
    #[must_use] pub const fn needs_layout(&self) -> bool {
        self.intersects(Self::AFFECTS_LAYOUT)
    }

    /// Returns true if paint is needed (but not necessarily layout).
    #[must_use] pub const fn needs_paint(&self) -> bool {
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
#[derive(Default)]
pub struct ExtendedDiffResult {
    /// Original diff result (lifecycle events + node moves).
    pub diff: DiffResult,
    /// Per-node change report for matched (moved) nodes.
    /// Each entry: (`old_node_id`, `new_node_id`, `what_changed`).
    /// Only contains entries for nodes that were matched.
    pub node_changes: Vec<(NodeId, NodeId, NodeChangeSet)>,
}


/// Compare two matched `NodeData` instances field-by-field and return
/// a `NodeChangeSet` describing what changed.
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose parser/builder/dispatch (one branch per input variant)
#[must_use] pub fn compute_node_changes(
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
            let hash_img = |img: &crate::resources::ImageRef| -> u64 {
                let mut h = crate::hash::DefaultHasher::new();
                img.hash(&mut h);
                h.finish()
            };
            if hash_img(old_img) != hash_img(new_img) {
                changes.insert(NodeChangeSet::IMAGE_CHANGED);
            }
        }
        _ => {} // Same non-content type → no content change
    }

    // 3. IDs and classes (now stored in attributes as AttributeType::Id/Class)
    {
        use crate::dom::AttributeType;
        let old_ids_classes: Vec<_> = old_node.attributes().as_ref().iter()
            .filter(|a| matches!(a, AttributeType::Id(_) | AttributeType::Class(_)))
            .collect();
        let new_ids_classes: Vec<_> = new_node.attributes().as_ref().iter()
            .filter(|a| matches!(a, AttributeType::Id(_) | AttributeType::Class(_)))
            .collect();
        if old_ids_classes != new_ids_classes {
            changes.insert(NodeChangeSet::IDS_AND_CLASSES);
        }
    }

    // 4. Inline CSS properties — classify into layout-affecting vs paint-only.
    // After the inline-vs-component unification, inline CSS is stored as a `Css`
    // with rule blocks; iterate it via the `(property, conditions)` flat view to
    // keep the per-property compare semantics this code was written for.
    if old_node.style != new_node.style {
        let mut has_layout = false;
        let mut has_paint = false;

        // Classify a changed/added/removed property into the layout vs paint bucket.
        #[allow(clippy::items_after_statements)]
        fn mark(prop_type: CssPropertyType, has_layout: &mut bool, has_paint: &mut bool) {
            if prop_type.relayout_scope(true) == RelayoutScope::None {
                *has_paint = true;
            } else {
                *has_layout = true;
            }
        }

        // AUDIT: key the diff by (prop_type, conditions), NOT prop_type alone.
        // A node can carry the same property under different conditions (e.g.
        // `color: red` and `color: blue` scoped to `:hover`); keying by
        // prop_type collapsed them into one map slot, so a change to one
        // conditional variant could be silently dropped. Match each new
        // property against an old entry with the SAME prop_type AND the same
        // conditions, and mark any old entry left unmatched as removed.
        let old_props: Vec<(CssPropertyType, _, _)> = old_node
            .style
            .iter_inline_properties()
            .map(|(prop, conds)| (prop.get_type(), prop, conds))
            .collect();
        let mut old_matched = vec![false; old_props.len()];

        for (prop, conds) in new_node.style.iter_inline_properties() {
            let prop_type = prop.get_type();
            // Find an as-yet-unmatched old entry with the same (type, conditions).
            let mut found_unchanged = false;
            for (i, (old_type, old_prop, old_conds)) in old_props.iter().enumerate() {
                if old_matched[i]
                    || *old_type != prop_type
                    || old_conds.as_slice() != conds.as_slice()
                {
                    continue;
                }
                old_matched[i] = true;
                if *old_prop == prop {
                    found_unchanged = true;
                }
                break;
            }
            // Unchanged only when we matched an old (type, conditions) slot whose
            // value is identical; otherwise the property was added or changed.
            if !found_unchanged {
                mark(prop_type, &mut has_layout, &mut has_paint);
            }
        }

        // Check for removed properties (old (type, conditions) slots never matched)
        for (i, (old_type, _, _)) in old_props.iter().enumerate() {
            if !old_matched[i] {
                mark(*old_type, &mut has_layout, &mut has_paint);
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
        if old_cbs.len() == new_cbs.len() {
            for (o, n) in old_cbs.iter().zip(new_cbs.iter()) {
                if o.event != n.event || o.callback != n.callback {
                    changes.insert(NodeChangeSet::CALLBACKS);
                    break;
                }
            }
        } else {
            changes.insert(NodeChangeSet::CALLBACKS);
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

/// Calculate the reconciliation key for a node using the priority hierarchy:
/// 1. Explicit key (set via `.with_key()`)
/// 2. CSS ID (set via `.with_id("my-id")`)
/// 3. Structural key: nth-of-type-within-parent + parent's reconciliation key
///
/// The structural key prevents incorrect matching when nodes are inserted
/// before existing nodes (e.g., prepending items to a list) and allows
/// keyless nodes to be matched across frames when their logical position
/// and type are stable (even if content changed — which then fires an
/// `Update` lifecycle event, see `reconcile_dom`).
///
/// When `hierarchy` is empty (or this node has no entry), the structural
/// key degrades to `discriminant(node_type) + classes` — parent/nth-of-type
/// context simply drops out. This lets callers that don't track hierarchy
/// (tests, flat-DOM scenarios) still benefit from explicit-key and CSS-ID
/// matching without divergent behavior.
#[must_use] pub fn calculate_reconciliation_key(
    node_data: &[NodeData],
    hierarchy: &[NodeHierarchyItem],
    node_id: NodeId,
) -> u64 {
    use core::hash::Hasher;

    let n = node_data.len();

    // Terminal (parent-independent) key for a node: Priority 1 explicit key,
    // else Priority 2 CSS ID, else `None` (structural — needs the parent chain).
    let terminal_key = |nid: NodeId| -> Option<u64> {
        let node = &node_data[nid.index()];
        // Priority 1: Explicit key
        if let Some(key) = node.get_key() {
            return Some(key);
        }
        // Priority 2: CSS ID
        for attr in node.attributes().as_ref() {
            if let Some(id) = attr.as_id() {
                let mut hasher = crate::hash::DefaultHasher::new();
                id.hash(&mut hasher);
                return Some(hasher.finish());
            }
        }
        None
    };

    // Fast path: the node itself has an explicit key or CSS ID.
    if let Some(key) = terminal_key(node_id) {
        return key;
    }

    // Priority 3: structural key, computed ITERATIVELY up the parent chain.
    //
    // AUDIT: the previous implementation recursed once per ancestor with no
    // depth cap and no cycle guard, so a deep DOM overflowed the stack and a
    // corrupt (cyclic) hierarchy recursed forever — and `precompute_*` calls
    // this once per node. Walk upward instead, bounded by the node count.
    //
    // Collect the structural chain from `node_id` upward. The walk stops at:
    //   - the root (a node with no parent) — structural base is just
    //     `discriminant + classes`,
    //   - a terminal (explicit-key / CSS-ID) ancestor, whose key seeds the fold, or
    //   - `n` iterations (a valid parent chain is at most `n` long, so exceeding
    //     that means the hierarchy is cyclic/corrupt — stop).
    let mut chain: Vec<NodeId> = Vec::new();
    let mut seed_parent_key: Option<u64> = None;
    let mut cur = node_id;
    for _ in 0..n {
        if cur.index() >= n {
            break;
        }
        chain.push(cur);
        match hierarchy.get(cur.index()).and_then(NodeHierarchyItem::parent_id) {
            None => break,
            Some(parent) => {
                if let Some(k) = terminal_key(parent) {
                    seed_parent_key = Some(k);
                    break;
                }
                cur = parent;
            }
        }
    }

    // Fold from the topmost ancestor down to `node_id`. `parent_key` threads the
    // accumulated key of the level above (identical to the old recursion, just
    // unrolled bottom-up).
    let mut parent_key: Option<u64> = seed_parent_key;
    for &nid in chain.iter().rev() {
        let node = &node_data[nid.index()];
        let mut hasher = crate::hash::DefaultHasher::new();

        core::mem::discriminant(node.get_node_type()).hash(&mut hasher);
        for attr in node.attributes().as_ref() {
            if let Some(class) = attr.as_class() {
                class.hash(&mut hasher);
            }
        }

        if let Some(parent_id) =
            hierarchy.get(nid.index()).and_then(NodeHierarchyItem::parent_id)
        {
            // nth-of-type: count same-discriminant siblings before `nid`.
            let mut sibling_index: usize = 0;
            let mut current = hierarchy
                .get(parent_id.index())
                .and_then(|h| h.first_child_id(parent_id));
            while let Some(sibling_id) = current {
                if sibling_id == nid {
                    break;
                }
                let sibling = &node_data[sibling_id.index()];
                if core::mem::discriminant(sibling.get_node_type())
                    == core::mem::discriminant(node.get_node_type())
                {
                    sibling_index += 1;
                }
                current = hierarchy
                    .get(sibling_id.index())
                    .and_then(NodeHierarchyItem::next_sibling_id);
            }

            sibling_index.hash(&mut hasher);
            parent_key.unwrap_or(0).hash(&mut hasher);
        }

        parent_key = Some(hasher.finish());
    }

    parent_key.unwrap_or(0)
}

/// Precompute reconciliation keys for every node in a DOM tree.
///
/// Called once per side (old/new) at the start of `reconcile_dom`. Returns a
/// vector indexed by node index (`keys[node_id.index()]`) so lookup during
/// reconciliation is O(1).
#[must_use] pub fn precompute_reconciliation_keys(
    node_data: &[NodeData],
    hierarchy: &[NodeHierarchyItem],
) -> Vec<u64> {
    (0..node_data.len())
        .map(|idx| calculate_reconciliation_key(node_data, hierarchy, NodeId::new(idx)))
        .collect()
}

/// Represents a mapping between a node in the old DOM and the new DOM.
#[derive(Debug, Clone, Copy)]
pub struct NodeMove {
    /// The `NodeId` in the old DOM array
    pub old_node_id: NodeId,
    /// The `NodeId` in the new DOM array
    pub new_node_id: NodeId,
}

/// The result of a DOM diff, containing lifecycle events and node mappings.
#[derive(Debug, Clone)]
#[derive(Default)]
pub struct DiffResult {
    /// Lifecycle events generated by the diff (Mount, Unmount, Resize, Update)
    pub events: Vec<SyntheticEvent>,
    /// Maps Old `NodeId` -> New `NodeId` for state migration (focus, scroll, etc.)
    pub node_moves: Vec<NodeMove>,
}


/// Calculates the difference between two DOM frames and generates lifecycle events.
///
/// This is the main entry point for DOM reconciliation. It compares the old and new
/// DOM trees and produces:
/// - Mount events for new nodes
/// - Unmount events for removed nodes
/// - Resize events for nodes whose bounds changed
/// - Update events for nodes whose logical position is stable but content changed
///
/// # Matching priority
/// For every node, the reconciliation key (`calculate_reconciliation_key`) encodes
/// Priority 1 (`.with_key()`), Priority 2 (CSS ID), and Priority 3 (structural key:
/// nth-of-type + parent key). The tiers are then tried in order:
///
/// 1. **Reconciliation key** — matches logical identity, may fire Update on content change.
/// 2. **Content hash** — exact match including content; catches pure reorders of anonymous nodes.
/// 3. **Structural hash** — matches node type + attrs ignoring text content; for text-edit cases.
///
/// # Arguments
/// * `old_node_data` / `new_node_data` - Per-node data for each frame
/// * `old_hierarchy` / `new_hierarchy` - Parent/sibling pointers. Pass `&[]` if unavailable;
///   the structural-key branch of the reconciliation key degrades gracefully.
/// * `old_layout` / `new_layout` - Layout bounds used to detect Resize events
/// * `dom_id` - The DOM identifier
/// * `timestamp` - Current timestamp for events
#[allow(clippy::needless_pass_by_value)] // owned azul value taken by value (public API / ownership-transfer convention)
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose parser/builder/dispatch (one branch per input variant)
#[must_use] pub fn reconcile_dom(
    old_node_data: &[NodeData],
    new_node_data: &[NodeData],
    old_hierarchy: &[NodeHierarchyItem],
    new_hierarchy: &[NodeHierarchyItem],
    old_layout: &OrderedMap<NodeId, LogicalRect>,
    new_layout: &OrderedMap<NodeId, LogicalRect>,
    dom_id: DomId,
    timestamp: Instant,
) -> DiffResult {
    // Helper: pop the first non-consumed NodeId from a queue.
    fn pop_first_unconsumed(
        queue: &mut VecDeque<NodeId>,
        consumed: &[bool],
    ) -> Option<NodeId> {
        while let Some(&old_id) = queue.front() {
            queue.pop_front();
            if !consumed[old_id.index()] {
                return Some(old_id);
            }
        }
        None
    }

    let mut result = DiffResult::default();

    // --- STEP 1: INDEX THE OLD DOM ---
    //
    // Three tiers, in priority order:
    //   Tier 1: reconciliation key (.with_key() / CSS ID / structural key)
    //   Tier 2: content hash (exact node_data hash — matches pure reorders)
    //   Tier 3: structural hash (discriminant + attrs, ignores text — matches text edits)
    //
    // Each tier is keyed with a `VecDeque<NodeId>` because all three can legitimately
    // collide (two sibling divs produce the same structural key, two identical nodes
    // produce the same content hash, etc.); we consume in document order on match.

    let old_rec_keys = precompute_reconciliation_keys(old_node_data, old_hierarchy);
    // AUDIT: precompute NEW keys too so the Tier-2/Tier-3 keyless tiers can be
    // gated on parent-key agreement (see STEP 2). Also lets Tier 1 look the key
    // up instead of recomputing it per node.
    let new_rec_keys = precompute_reconciliation_keys(new_node_data, new_hierarchy);

    // Reconciliation key of a node's PARENT (`None` for a root or when the
    // hierarchy is unavailable). Used to keep keyless matches from migrating
    // focus/scroll/dataset state across different parents.
    let old_parent_key = |old_id: NodeId| -> Option<u64> {
        old_hierarchy
            .get(old_id.index())
            .and_then(NodeHierarchyItem::parent_id)
            .map(|p| old_rec_keys[p.index()])
    };

    let mut old_by_rec_key: OrderedMap<u64, VecDeque<NodeId>> = OrderedMap::default();
    let mut old_hashed: OrderedMap<DomNodeHash, VecDeque<NodeId>> = OrderedMap::default();
    let mut old_structural: OrderedMap<DomNodeHash, VecDeque<NodeId>> = OrderedMap::default();
    let mut old_nodes_consumed = vec![false; old_node_data.len()];

    for (idx, node) in old_node_data.iter().enumerate() {
        let id = NodeId::new(idx);
        old_by_rec_key.entry(old_rec_keys[idx]).or_default().push_back(id);

        let hash = node.calculate_node_data_hash();
        old_hashed.entry(hash).or_default().push_back(id);

        let structural_hash = node.calculate_structural_hash();
        old_structural.entry(structural_hash).or_default().push_back(id);
    }

    // --- STEP 2: ITERATE NEW DOM AND CLAIM MATCHES ---

    for (new_idx, new_node) in new_node_data.iter().enumerate() {
        let new_id = NodeId::new(new_idx);
        let mut matched_old_id = None;
        let mut matched_by_rec_key = false;
        let has_explicit_key = new_node.get_key().is_some();

        // Tier 1: Reconciliation key (explicit `.with_key()`, CSS ID, or structural key)
        let new_rec_key = new_rec_keys[new_idx];
        if let Some(queue) = old_by_rec_key.get_mut(&new_rec_key) {
            if let Some(old_id) = pop_first_unconsumed(queue, &old_nodes_consumed) {
                matched_old_id = Some(old_id);
                matched_by_rec_key = true;
            }
        }

        // AUDIT: parent-key of the new node. The keyless Tier-2/Tier-3 tiers are
        // only allowed to claim an old node whose parent's reconciliation key
        // agrees — otherwise two structurally-identical nodes under DIFFERENT
        // parents would match and migrate focus/scroll/dataset state to an
        // unrelated subtree. When either hierarchy is unavailable this is `None`
        // on both sides, so the gate is a no-op (flat-DOM behavior preserved).
        let new_parent_key: Option<u64> = new_hierarchy
            .get(new_idx)
            .and_then(NodeHierarchyItem::parent_id)
            .map(|p| new_rec_keys[p.index()]);

        // An explicit `.with_key()` is a strong, intentional identity marker: if it
        // doesn't match anything in the old DOM we treat the new node as genuinely
        // new (Mount), rather than falling through to coarser content/structural
        // tiers and silently matching an unrelated node.
        if !has_explicit_key && matched_old_id.is_none() {
            // Tier 2: Content hash (exact match — catches pure reorders)
            let hash = new_node.calculate_node_data_hash();
            if let Some(queue) = old_hashed.get_mut(&hash) {
                if let Some(pos) = queue.iter().position(|&old_id| {
                    !old_nodes_consumed[old_id.index()]
                        && old_parent_key(old_id) == new_parent_key
                }) {
                    matched_old_id = queue.remove(pos);
                }
            }

            // Tier 3: Structural hash (text-node fallback — ignores text content)
            if matched_old_id.is_none() {
                let structural_hash = new_node.calculate_structural_hash();
                if let Some(queue) = old_structural.get_mut(&structural_hash) {
                    if let Some(pos) = queue.iter().position(|&old_id| {
                        !old_nodes_consumed[old_id.index()]
                            && old_parent_key(old_id) == new_parent_key
                    }) {
                        matched_old_id = queue.remove(pos);
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

            // Fire Update when the node was matched by logical identity (reconciliation
            // key: explicit .with_key(), CSS ID, or structural key) but its content hash
            // differs. Tier-2/Tier-3 matches by definition don't carry an Update — a
            // content-hash match is content-identical, and a structural-hash match is
            // a text edit handled by cursor/text reconciliation elsewhere.
            if matched_by_rec_key {
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
                        reason: LifecycleReason::Unmount,
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

/// Check if the node has an `AfterMount` callback registered.
fn has_mount_callback(node: &NodeData) -> bool {
    node.get_callbacks().iter().any(|cb| {
        matches!(
            cb.event,
            EventFilter::Component(ComponentEventFilter::AfterMount)
        )
    })
}

/// Check if the node has a `BeforeUnmount` callback registered.
fn has_unmount_callback(node: &NodeData) -> bool {
    node.get_callbacks().iter().any(|cb| {
        matches!(
            cb.event,
            EventFilter::Component(ComponentEventFilter::BeforeUnmount)
        )
    })
}

/// Check if the node has a `NodeResized` callback registered.
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
    node.get_callbacks().iter().any(|cb| {
        matches!(
            cb.event,
            EventFilter::Component(ComponentEventFilter::Updated)
        )
    })
}

/// Migrate state (focus, scroll, etc.) from old node IDs to new node IDs.
///
/// This function should be called after reconciliation to update any state
/// that references old `NodeIds` to use the new `NodeIds`.
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
#[must_use] pub fn create_migration_map(node_moves: &[NodeMove]) -> OrderedMap<NodeId, NodeId> {
    let mut map = OrderedMap::default();
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
        let Some(merge_callback) = new_node_data[new_idx].get_merge_callback() else {
            continue; // No merge callback, skip
        };

        // 2. Check if BOTH nodes have datasets
        // We need to temporarily take the datasets to satisfy borrow checker
        let old_dataset = old_node_data[old_idx].take_dataset();
        let new_dataset = new_node_data[new_idx].take_dataset();

        match (new_dataset, old_dataset) {
            (Some(new_data), Some(old_data)) => {
                // The fresh DOM's dataset allocation. A widget builds its dataset,
                // its VirtualView content `refany`, AND its event-callback
                // `refany`s from clones of ONE `RefAny` — so every one shares THIS
                // allocation (`RefAny::clone` shares `sharing_info`; only the
                // per-clone `instance_id` differs). The merge below keeps the
                // PERSISTENT (old) allocation (e.g. MapWidget shares its tile cache
                // so background fetch threads keep writing into it), so every clone
                // of the fresh one is now orphaned and must be re-pointed — or the
                // widget fragments across two caches: the VirtualView rendered an
                // empty clone (blank/grey tiles) while the live data sat in the
                // dataset, and pan/zoom mutated yet a third copy. Identity = the
                // shared `RefCountInner` pointer (`sharing_info.ptr`).
                let orphan_alloc = new_data.sharing_info.ptr as usize;

                // 3. EXECUTE THE MERGE CALLBACK
                // The callback receives both datasets and returns the merged result
                let merged = (merge_callback.cb)(new_data, old_data);

                // 4. Store the merged result back in the new node
                new_node_data[new_idx].set_dataset(OptionRefAny::Some(merged.clone()));

                // 5. UNIFY: re-point every refany across the NEW DOM that was a
                // clone of the now-discarded fresh dataset onto the merged result,
                // so the whole widget reads ONE cache. Covers VirtualView content
                // refanys + event-callback refanys + any node's dataset cloned
                // from the same source. (Generalises the old special-case that
                // only re-pointed a VirtualView ON the merge node itself — the
                // MapWidget puts its VirtualView in a CHILD and its pan/zoom
                // callbacks on the parent, which that case missed.)
                for nd in new_node_data.iter_mut() {
                    if let Some(vv) = nd.get_virtual_view_node() {
                        if vv.refany.sharing_info.ptr as usize == orphan_alloc {
                            vv.refany = merged.clone();
                        }
                    }
                    for cb in nd.callbacks.as_mut().iter_mut() {
                        if cb.refany.sharing_info.ptr as usize == orphan_alloc {
                            cb.refany = merged.clone();
                        }
                    }
                    let ds_is_orphan = nd
                        .get_dataset()
                        .is_some_and(|ds| ds.sharing_info.ptr as usize == orphan_alloc);
                    if ds_is_orphan {
                        nd.set_dataset(OptionRefAny::Some(merged.clone()));
                    }
                }
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
#[must_use] pub fn calculate_contenteditable_key(
    node_data: &[NodeData],
    hierarchy: &[NodeHierarchyItem],
    node_id: NodeId,
) -> u64 {
    use core::hash::Hasher;

    let n = node_data.len();

    // Terminal (parent-independent) key: Priority 1 explicit key, else
    // Priority 2 CSS ID, else `None` (structural — needs the parent chain).
    let terminal_key = |nid: NodeId| -> Option<u64> {
        let node = &node_data[nid.index()];
        // Priority 1: Explicit key (from .with_key())
        if let Some(explicit_key) = node.get_key() {
            return Some(explicit_key);
        }
        // Priority 2: CSS ID
        for attr in node.attributes().as_ref() {
            if let Some(id) = attr.as_id() {
                let mut hasher = crate::hash::DefaultHasher::new(); // Different seed for ID keys
                hasher.write(id.as_bytes());
                return Some(hasher.finish());
            }
        }
        None
    };

    // Fast path: the node itself has an explicit key or CSS ID.
    if let Some(key) = terminal_key(node_id) {
        return key;
    }

    // Priority 3: structural key, computed ITERATIVELY up the parent chain.
    //
    // AUDIT: replaces unbounded parent-chain recursion (stack overflow on deep
    // DOMs, infinite recursion on a cyclic hierarchy). Same fold as the old
    // recursion, unrolled bottom-up and bounded by the node count.
    let mut chain: Vec<NodeId> = Vec::new();
    let mut seed_parent_key: Option<u64> = None;
    let mut cur = node_id;
    for _ in 0..n {
        if cur.index() >= n {
            break;
        }
        chain.push(cur);
        match hierarchy.get(cur.index()).and_then(NodeHierarchyItem::parent_id) {
            None => break,
            Some(parent) => {
                if let Some(k) = terminal_key(parent) {
                    seed_parent_key = Some(k);
                    break;
                }
                cur = parent;
            }
        }
    }

    // Fold from the topmost ancestor down to `node_id`. Unlike the
    // reconciliation key, the contenteditable structural key ALWAYS writes a
    // `parent_key` (0 at the root) and an `nth_of_type` (0 at the root), so the
    // per-level hashing is unconditional — preserve that exactly.
    let mut parent_key: u64 = seed_parent_key.unwrap_or(0);
    for &nid in chain.iter().rev() {
        let node = &node_data[nid.index()];
        let mut hasher = crate::hash::DefaultHasher::new(); // Different seed for structural keys

        let node_parent = hierarchy.get(nid.index()).and_then(NodeHierarchyItem::parent_id);

        // parent_key: 0 at the root, else the accumulated key of the level above.
        let level_parent_key = if node_parent.is_some() { parent_key } else { 0 };
        hasher.write(&level_parent_key.to_le_bytes());

        // nth-of-type: count same-discriminant siblings before `nid`.
        let node_discriminant = core::mem::discriminant(node.get_node_type());
        let nth_of_type = node_parent.map_or(0u32, |parent_id| {
            let mut count = 0u32;
            let mut sibling_id = hierarchy
                .get(parent_id.index())
                .and_then(|h| h.first_child_id(parent_id));
            while let Some(sib_id) = sibling_id {
                if sib_id == nid {
                    break;
                }
                let sibling_discriminant =
                    core::mem::discriminant(node_data[sib_id.index()].get_node_type());
                if sibling_discriminant == node_discriminant {
                    count += 1;
                }
                sibling_id = hierarchy
                    .get(sib_id.index())
                    .and_then(NodeHierarchyItem::next_sibling_id);
            }
            count
        });
        hasher.write(&nth_of_type.to_le_bytes());

        // Hash the node type discriminant (Discriminant<T> implements Hash)
        node_discriminant.hash(&mut hasher);

        // Also hash the classes for additional stability
        for attr in node.attributes().as_ref() {
            if let Some(class) = attr.as_class() {
                hasher.write(class.as_bytes());
            }
        }

        parent_key = hasher.finish();
    }

    parent_key
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
#[must_use] pub fn reconcile_cursor_position(
    old_text: &str,
    new_text: &str,
    old_cursor_byte: usize,
) -> usize {
    // AUDIT: every returned offset is snapped DOWN to the nearest UTF-8 char
    // boundary in `new_text` (and clamped to its length). The prefix/suffix
    // scans below compare byte-by-byte and can land mid-codepoint, so a raw
    // return value could later panic when used to slice `new_text` as a `str`.
    let snap = |offset: usize| -> usize {
        let mut o = offset.min(new_text.len());
        while o > 0 && !new_text.is_char_boundary(o) {
            o -= 1;
        }
        o
    };

    // If texts are equal, cursor is unchanged
    if old_text == new_text {
        return snap(old_cursor_byte);
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
        return snap(old_cursor_byte);
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
        // saturating: an out-of-range cursor (> old_text.len()) must clamp to the
        // end of the new text like every other path here, not underflow-panic.
        let offset_from_end = old_text.len().saturating_sub(old_cursor_byte);
        return snap(new_text.len().saturating_sub(offset_from_end));
    }

    // Cursor was in the changed region - place at end of inserted content
    // This handles insertions (cursor moves with new text) and deletions (cursor at edit point)
    snap(new_suffix_start)
}

/// Get the text content from a `NodeData` if it's a Text node.
///
/// Returns the text string if the node is `NodeType::Text`, otherwise `None`.
#[must_use] pub fn get_node_text_content(node: &NodeData) -> Option<&str> {
    if let NodeType::Text(ref text) = node.get_node_type() {
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
    /// The text content before the change.
    pub old_text: String,
    /// The text content after the change.
    pub new_text: String,
}

/// Per-node change report combining multiple information sources.
#[derive(Debug, Clone, Default)]
pub struct NodeChangeReport {
    /// Bitflags from DOM-level field comparison.
    pub change_set: NodeChangeSet,

    /// Highest `RelayoutScope` from any CSS property that changed on this node.
    /// This is more granular than `NodeChangeSet`'s binary LAYOUT/PAINT split.
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
    /// Returns the `DirtyFlag` level needed for this change report.
    /// Maps `RelayoutScope` + `NodeChangeSet` → a simple tri-state.
    #[must_use] pub fn needs_layout(&self) -> bool {
        self.change_set.needs_layout() || self.relayout_scope > RelayoutScope::None
    }

    #[must_use] pub const fn needs_paint(&self) -> bool {
        self.change_set.needs_paint()
    }

    #[must_use] pub fn is_visually_unchanged(&self) -> bool {
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
    /// Per-node change info. Key is the new-DOM `NodeId`.
    pub per_node: BTreeMap<NodeId, NodeChangeReport>,

    /// Maximum `RelayoutScope` across all changed nodes.
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
    #[must_use] pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if no changes were detected at all.
    #[must_use] pub fn is_empty(&self) -> bool {
        self.per_node.is_empty() && self.mounted_nodes.is_empty() && self.unmounted_nodes.is_empty()
    }

    /// Returns true if layout work is needed (any node has scope > None).
    #[must_use] pub fn needs_layout(&self) -> bool {
        self.max_scope > RelayoutScope::None
            || !self.mounted_nodes.is_empty()
            || self.per_node.values().any(NodeChangeReport::needs_layout)
    }

    /// Returns true if only paint work is needed (no layout).
    #[must_use] pub fn needs_paint_only(&self) -> bool {
        !self.needs_layout() && self.per_node.values().any(NodeChangeReport::needs_paint)
    }

    /// Returns true if only non-visual changes occurred (callbacks, dataset, a11y).
    #[must_use] pub fn is_visually_unchanged(&self) -> bool {
        self.mounted_nodes.is_empty()
            && self.unmounted_nodes.is_empty()
            && self.max_scope == RelayoutScope::None
            && self.per_node.values().all(NodeChangeReport::is_visually_unchanged)
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
            let scope = Self::classify_change_scope(*change_set, new_node_data, new_id);

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

    /// Classify a `NodeChangeSet` into the appropriate `RelayoutScope`.
    fn classify_change_scope(
        change_set: NodeChangeSet,
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
            for (prop, _conds) in new_node.style.iter_inline_properties() {
                let scope = prop.get_type().relayout_scope(true);
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
#[must_use] pub fn reconcile_dom_with_changes(
    old_node_data: &[NodeData],
    new_node_data: &[NodeData],
    old_hierarchy: &[NodeHierarchyItem],
    new_hierarchy: &[NodeHierarchyItem],
    old_styled_nodes: Option<&[StyledNodeState]>,
    new_styled_nodes: Option<&[StyledNodeState]>,
    old_layout: &OrderedMap<NodeId, LogicalRect>,
    new_layout: &OrderedMap<NodeId, LogicalRect>,
    dom_id: DomId,
    timestamp: Instant,
) -> ExtendedDiffResult {
    // Step 1: Run standard reconciliation
    let diff = reconcile_dom(
        old_node_data,
        new_node_data,
        old_hierarchy,
        new_hierarchy,
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
#[derive(Default)]
pub struct NodeDataFingerprint {
    /// Hash of `node_type` (Text content, Image ref, Div, etc.)
    pub content_hash: u64,
    /// Hash of `styled_node_state` (hover, focus, active bits)
    pub state_hash: u64,
    /// Hash of inline CSS properties
    pub inline_css_hash: u64,
    /// Hash of `ids_and_classes`
    pub ids_classes_hash: u64,
    /// Hash of callbacks (event types + function pointers)
    pub callbacks_hash: u64,
    /// Hash of other attributes (contenteditable, `tab_index`, dataset)
    pub attrs_hash: u64,
}


impl NodeDataFingerprint {
    /// Compute a fingerprint from a node's data and styled state.
    #[must_use] pub fn compute(node: &NodeData, styled_state: Option<&StyledNodeState>) -> Self {
        use core::hash::Hasher;
        use core::hash::Hash;

        // Content hash
        let content_hash = {
            let mut h = crate::hash::DefaultHasher::new();
            node.get_node_type().hash(&mut h);
            h.finish()
        };

        // State hash
        let state_hash = {
            let mut h = crate::hash::DefaultHasher::new();
            if let Some(state) = styled_state {
                state.hash(&mut h);
            }
            h.finish()
        };

        // Inline CSS hash — full CssProperty value (matches the legacy
        // CssPropertyWithConditions::hash that hashed both property and the
        // condition vec length).
        let inline_css_hash = {
            let mut h = crate::hash::DefaultHasher::new();
            for (prop, conds) in node.style.iter_inline_properties() {
                prop.hash(&mut h);
                conds.as_slice().len().hash(&mut h);
            }
            h.finish()
        };

        // IDs and classes hash (now stored in attributes)
        let ids_classes_hash = {
            let mut h = crate::hash::DefaultHasher::new();
            for attr in node.attributes().as_ref() {
                match attr {
                    crate::dom::AttributeType::Id(s) => {
                        crate::dom::IdOrClass::Id(s.clone()).hash(&mut h);
                    }
                    crate::dom::AttributeType::Class(s) => {
                        crate::dom::IdOrClass::Class(s.clone()).hash(&mut h);
                    }
                    _ => {}
                }
            }
            h.finish()
        };

        // Callbacks hash
        let callbacks_hash = {
            let mut h = crate::hash::DefaultHasher::new();
            for cb in node.callbacks.as_ref() {
                cb.event.hash(&mut h);
                cb.callback.hash(&mut h);
            }
            h.finish()
        };

        // Attributes hash
        let attrs_hash = {
            let mut h = crate::hash::DefaultHasher::new();
            node.is_contenteditable().hash(&mut h);
            node.flags.hash(&mut h);
            node.get_dataset().hash(&mut h);
            h.finish()
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

    /// Returns a quick `NodeChangeSet` by comparing two fingerprints.
    /// This is O(1) — just comparing 6 u64s.
    ///
    /// The result is *conservative*: if a field hash differs, we set the
    /// broadest applicable flag. For precise classification (e.g., which
    /// CSS properties changed and their `relayout_scope()`), the caller
    /// should fall back to `compute_node_changes()` for changed nodes.
    #[must_use] pub const fn diff(&self, other: &Self) -> NodeChangeSet {
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
    #[must_use] pub fn is_identical(&self, other: &Self) -> bool {
        self == other
    }

    /// Quick check: could this change affect layout?
    #[must_use] pub const fn might_affect_layout(&self, other: &Self) -> bool {
        self.content_hash != other.content_hash
            || self.inline_css_hash != other.inline_css_hash
            || self.ids_classes_hash != other.ids_classes_hash
            || self.attrs_hash != other.attrs_hash
    }

    /// Quick check: could this change affect visuals at all?
    #[must_use] pub const fn might_affect_visuals(&self, other: &Self) -> bool {
        self.content_hash != other.content_hash
            || self.state_hash != other.state_hash
            || self.inline_css_hash != other.inline_css_hash
            || self.ids_classes_hash != other.ids_classes_hash
    }
}

#[cfg(test)]
mod audit_tests {
    use super::*;
    use crate::dom::NodeData;
    use crate::styled_dom::NodeHierarchyItem;

    // Build a NodeHierarchyItem from optional 0-based indices (encoded 1-based).
    fn hitem(
        parent: Option<usize>,
        prev: Option<usize>,
        next: Option<usize>,
        last_child: Option<usize>,
    ) -> NodeHierarchyItem {
        NodeHierarchyItem {
            parent: parent.map_or(0, |p| p + 1),
            previous_sibling: prev.map_or(0, |p| p + 1),
            next_sibling: next.map_or(0, |p| p + 1),
            last_child: last_child.map_or(0, |p| p + 1),
        }
    }

    // A deep parent chain that would overflow the stack with the old recursion.
    #[test]
    fn reconciliation_key_deep_chain_no_overflow() {
        let build = |n: usize| -> (Vec<NodeData>, Vec<NodeHierarchyItem>) {
            let node_data = (0..n).map(|_| NodeData::create_div()).collect();
            let hierarchy = (0..n)
                .map(|i| {
                    hitem(
                        if i == 0 { None } else { Some(i - 1) },
                        None,
                        None,
                        if i + 1 < n { Some(i + 1) } else { None },
                    )
                })
                .collect();
            (node_data, hierarchy)
        };

        // A very deep linear chain: the OLD recursion overflowed the stack here.
        // A single-node key walk is O(depth) and must complete without recursing.
        let n = 100_000usize;
        let (node_data, hierarchy) = build(n);
        let _ = calculate_reconciliation_key(&node_data, &hierarchy, NodeId::new(n - 1));
        let _ = calculate_contenteditable_key(&node_data, &hierarchy, NodeId::new(n - 1));

        // Whole-DOM precompute calls the per-node walk once per node, so over a
        // *linear* chain it is O(n²) — that only bites a pathological 100k-deep
        // DOM (never a real tree). Exercise the whole-DOM path over a modest
        // chain; correctness is covered by `reconciliation_key_single_node` and
        // `reconciliation_key_distinguishes_siblings`.
        let m = 2_000usize;
        let (nd, hi) = build(m);
        let keys = precompute_reconciliation_keys(&nd, &hi);
        assert_eq!(keys.len(), m);
    }

    // A cyclic (corrupt) hierarchy must terminate, not hang.
    #[test]
    fn reconciliation_key_cycle_terminates() {
        let node_data = vec![
            NodeData::create_div(),
            NodeData::create_div(),
            NodeData::create_div(),
        ];
        // node1.parent = 2, node2.parent = 1 — a cycle not involving root 0.
        let hierarchy = vec![
            hitem(None, None, None, None),
            hitem(Some(2), None, None, None),
            hitem(Some(1), None, None, None),
        ];
        let _ = calculate_reconciliation_key(&node_data, &hierarchy, NodeId::new(1));
        let _ = calculate_contenteditable_key(&node_data, &hierarchy, NodeId::new(1));
    }

    #[test]
    fn reconciliation_key_single_node() {
        let node_data = vec![NodeData::create_div()];
        let hierarchy = vec![hitem(None, None, None, None)];
        let direct = calculate_reconciliation_key(&node_data, &hierarchy, NodeId::new(0));
        let pre = precompute_reconciliation_keys(&node_data, &hierarchy)[0];
        assert_eq!(direct, pre);
    }

    #[test]
    fn reconciliation_key_distinguishes_siblings() {
        // root 0 with two div children 1 and 2 — nth-of-type must differ.
        let node_data = vec![NodeData::create_div(); 3];
        let hierarchy = vec![
            hitem(None, None, None, Some(2)),    // root: first_child=1, last_child=2
            hitem(Some(0), None, Some(2), None), // child 1
            hitem(Some(0), Some(1), None, None), // child 2
        ];
        let k1 = calculate_reconciliation_key(&node_data, &hierarchy, NodeId::new(1));
        let k2 = calculate_reconciliation_key(&node_data, &hierarchy, NodeId::new(2));
        assert_ne!(k1, k2);
    }

    #[test]
    fn cursor_offsets_are_always_char_boundaries() {
        // "héllo": h=0, é=1..3 (2 bytes), l=3, l=4, o=5 (len 6).
        let old = "héllo";
        let new = "héllo wörld"; // ö is multibyte too
        for c in 0..=old.len() {
            let r = reconcile_cursor_position(old, new, c);
            assert!(
                new.is_char_boundary(r),
                "cursor {c} mapped to non-char-boundary offset {r} in {new:?}",
            );
            assert!(r <= new.len());
        }
        // Deletion inside a multibyte suffix must not split a codepoint.
        let r = reconcile_cursor_position("aömega", "bömega", 3);
        assert!("bömega".is_char_boundary(r));
    }

    #[test]
    fn cursor_prefix_unchanged_stays_put() {
        assert_eq!(reconcile_cursor_position("Hello", "Hello World", 5), 5);
    }

    #[test]
    fn cursor_empty_cases() {
        assert_eq!(reconcile_cursor_position("", "abc", 0), 3);
        assert_eq!(reconcile_cursor_position("abc", "", 2), 0);
        assert_eq!(reconcile_cursor_position("abc", "abc", 2), 2);
    }
}

// ============================================================================
// Autotest: adversarial unit tests
// ============================================================================
//
// Generated against the autotest task spec for `core/src/diff.rs`. Strategy per
// category:
//
//   * numeric      -> 0 / MIN / MAX / overflow / NaN / saturation
//   * "parser"-ish -> malformed, huge, boundary and unicode text input
//                     (`reconcile_cursor_position` is the byte-offset parser here)
//   * round-trip   -> precompute == per-node compute, fingerprint == recompute,
//                     BitOr == BitOrAssign
//   * getters /    -> invariants hold on default, empty and extreme instances
//     predicates
//
// The module is inline (not `core/tests/`) because `has_*_callback`,
// `create_lifecycle_event` and `ChangeAccumulator::classify_change_scope` are
// private to this module.
#[cfg(test)]
mod autotest_generated {
    use super::*;

    use azul_css::{
        css::CssPropertyValue,
        props::{layout::LayoutWidth, property::CssProperty},
    };

    use crate::{
        callbacks::CoreCallback,
        dom::{DatasetMergeCallbackType, TabIndex},
        geom::{LogicalPosition, LogicalSize},
        refany::{OptionRefAny, RefAny},
        resources::{ImageRef, RawImageFormat},
    };

    // ---------------------------------------------------------------- helpers

    // `CoreCallback::cb` is a raw `usize` fn-pointer slot. `reconcile_dom` only
    // ever inspects `CoreCallbackData::event`, never calls through the pointer,
    // so `0` is a safe sentinel (same convention as
    // `core/tests/reconciliation/deep_reconciliation.rs`).
    fn noop_callback() -> CoreCallback {
        CoreCallback {
            cb: 0usize,
            ctx: OptionRefAny::None,
        }
    }

    fn with_cb(mut nd: NodeData, filter: ComponentEventFilter) -> NodeData {
        nd.add_callback(
            EventFilter::Component(filter),
            RefAny::new(0u32),
            noop_callback(),
        );
        nd
    }

    // Build a NodeHierarchyItem from optional 0-based indices (encoded 1-based).
    fn hitem(
        parent: Option<usize>,
        prev: Option<usize>,
        next: Option<usize>,
        last_child: Option<usize>,
    ) -> NodeHierarchyItem {
        NodeHierarchyItem {
            parent: parent.map_or(0, |p| p + 1),
            previous_sibling: prev.map_or(0, |p| p + 1),
            next_sibling: next.map_or(0, |p| p + 1),
            last_child: last_child.map_or(0, |p| p + 1),
        }
    }

    fn rect(w: f32, h: f32) -> LogicalRect {
        LogicalRect::new(LogicalPosition::new(0.0, 0.0), LogicalSize::new(w, h))
    }

    fn layout_of(entries: &[(usize, LogicalRect)]) -> OrderedMap<NodeId, LogicalRect> {
        let mut m = OrderedMap::default();
        for (idx, r) in entries {
            m.insert(NodeId::new(*idx), *r);
        }
        m
    }

    fn no_layout() -> OrderedMap<NodeId, LogicalRect> {
        OrderedMap::default()
    }

    // Flat diff: empty hierarchies exercise the documented "degrade gracefully"
    // path of the structural reconciliation key.
    fn diff_flat(old: &[NodeData], new: &[NodeData]) -> DiffResult {
        reconcile_dom(
            old,
            new,
            &[],
            &[],
            &no_layout(),
            &no_layout(),
            DomId::ROOT_ID,
            Instant::now(),
        )
    }

    fn count_events(r: &DiffResult, t: EventType) -> usize {
        r.events.iter().filter(|e| e.event_type == t).count()
    }

    fn id_node(id: &str) -> NodeData {
        NodeData::create_div().with_ids_and_classes(vec![IdOrClass::Id(id.into())].into())
    }

    fn class_node(class: &str) -> NodeData {
        NodeData::create_div().with_ids_and_classes(vec![IdOrClass::Class(class.into())].into())
    }

    // A representative unicode torture corpus: multi-byte, combining marks, RTL,
    // ZWJ emoji sequences, CJK, and a lone BOM.
    const UNICODE_SAMPLES: &[&str] = &[
        "",
        "a",
        "héllo",
        "e\u{0301}galite\u{0301}",   // combining acute accents
        "مرحبا بالعالم",             // RTL Arabic
        "👨‍👩‍👧‍👦 family",           // ZWJ emoji sequence
        "日本語のテキスト",
        "\u{feff}bom-prefixed",
        "🇩🇪🇫🇷🇯🇵",                 // regional indicator pairs
        "mixed 漢字 and ascii ✅",
    ];

    // ========================================================================
    // NodeChangeSet — constructor / predicates / numeric bit ops
    // ========================================================================

    #[test]
    fn autotest_changeset_empty_is_a_neutral_element() {
        let e = NodeChangeSet::empty();
        assert_eq!(e.bits, 0);
        assert!(e.is_empty());
        assert!(!e.needs_layout());
        assert!(!e.needs_paint());
        assert!(e.is_visually_unchanged());
        // `Default` must agree with `empty()`.
        assert_eq!(NodeChangeSet::default(), e);
        // Neutral under BitOr in both directions.
        let mut some = NodeChangeSet::empty();
        some.insert(NodeChangeSet::TEXT_CONTENT);
        assert_eq!((some | e).bits, some.bits);
        assert_eq!((e | some).bits, some.bits);
    }

    #[test]
    fn autotest_changeset_contains_zero_is_vacuously_true() {
        // `contains` is an ALL-bits test: `(bits & 0) == 0` holds for every
        // value, including the empty set. Pin the semantics so a future rewrite
        // to `(bits & flag) != 0` (an ANY-bits test) is caught.
        assert!(NodeChangeSet::empty().contains(0));
        let mut s = NodeChangeSet::empty();
        s.insert(NodeChangeSet::CALLBACKS);
        assert!(s.contains(0));
        assert!(NodeChangeSet { bits: u32::MAX }.contains(0));
    }

    #[test]
    fn autotest_changeset_intersects_zero_is_always_false() {
        // `intersects` is an ANY-bits test: masking with 0 can never be non-zero.
        assert!(!NodeChangeSet::empty().intersects(0));
        assert!(!NodeChangeSet { bits: u32::MAX }.intersects(0));
    }

    #[test]
    fn autotest_changeset_contains_is_all_bits_intersects_is_any_bits() {
        let mut s = NodeChangeSet::empty();
        s.insert(NodeChangeSet::TEXT_CONTENT);

        let both = NodeChangeSet::TEXT_CONTENT | NodeChangeSet::IMAGE_CHANGED;
        assert!(!s.contains(both), "contains() must require ALL bits");
        assert!(s.intersects(both), "intersects() must require ANY bit");
        assert!(s.contains(NodeChangeSet::TEXT_CONTENT));
    }

    #[test]
    fn autotest_changeset_insert_min_max_and_idempotent() {
        // MIN (0) is a no-op.
        let mut s = NodeChangeSet::empty();
        s.insert(0);
        assert!(s.is_empty());

        // MAX must not panic and must saturate to "all bits set".
        let mut s = NodeChangeSet::empty();
        s.insert(u32::MAX);
        assert_eq!(s.bits, u32::MAX);
        // Inserting again is idempotent (OR, not ADD -> cannot overflow).
        s.insert(u32::MAX);
        assert_eq!(s.bits, u32::MAX);
        s.insert(NodeChangeSet::TEXT_CONTENT);
        assert_eq!(s.bits, u32::MAX);

        // With every bit set, all defined flags are present.
        assert!(s.contains(NodeChangeSet::NODE_TYPE_CHANGED));
        assert!(s.contains(NodeChangeSet::AFFECTS_LAYOUT));
        assert!(s.contains(NodeChangeSet::AFFECTS_PAINT));
        assert!(s.needs_layout());
        assert!(s.needs_paint());
        assert!(!s.is_visually_unchanged());
        assert!(!s.is_empty());
    }

    #[test]
    fn autotest_changeset_undefined_high_bits_trigger_no_work() {
        // Bits outside every defined flag must not be interpreted as layout or
        // paint work — but the set is still non-empty.
        let s = NodeChangeSet {
            bits: 0b1000_0000_0000_0000_0000_0000_0000_0000,
        };
        assert!(!s.is_empty());
        assert!(!s.needs_layout());
        assert!(!s.needs_paint());
        assert!(s.is_visually_unchanged());
    }

    #[test]
    fn autotest_changeset_layout_and_paint_masks_are_disjoint() {
        // A single flag must never mean "relayout AND repaint" — the two
        // composite masks partition the visual flags.
        assert_eq!(
            NodeChangeSet::AFFECTS_LAYOUT & NodeChangeSet::AFFECTS_PAINT,
            0,
            "AFFECTS_LAYOUT and AFFECTS_PAINT must not overlap",
        );

        for flag in [
            NodeChangeSet::NODE_TYPE_CHANGED,
            NodeChangeSet::TEXT_CONTENT,
            NodeChangeSet::IDS_AND_CLASSES,
            NodeChangeSet::INLINE_STYLE_LAYOUT,
            NodeChangeSet::CHILDREN_CHANGED,
            NodeChangeSet::IMAGE_CHANGED,
            NodeChangeSet::CONTENTEDITABLE,
            NodeChangeSet::INLINE_STYLE_PAINT,
            NodeChangeSet::STYLED_STATE,
            NodeChangeSet::CALLBACKS,
            NodeChangeSet::DATASET,
            NodeChangeSet::ACCESSIBILITY,
        ] {
            let mut s = NodeChangeSet::empty();
            s.insert(flag);
            assert!(!(s.needs_layout() && s.needs_paint()), "flag {flag:#b} is both");
            // `is_visually_unchanged` is exactly "neither layout nor paint".
            assert_eq!(
                s.is_visually_unchanged(),
                !s.needs_layout() && !s.needs_paint(),
                "is_visually_unchanged() disagrees with needs_layout/needs_paint for {flag:#b}",
            );
        }
    }

    #[test]
    fn autotest_changeset_nonvisual_flags_are_visually_unchanged() {
        let mut s = NodeChangeSet::empty();
        s.insert(NodeChangeSet::CALLBACKS);
        s.insert(NodeChangeSet::DATASET);
        s.insert(NodeChangeSet::ACCESSIBILITY);
        s.insert(NodeChangeSet::TAB_INDEX); // TAB_INDEX is in neither mask
        assert!(!s.is_empty());
        assert!(s.is_visually_unchanged());
        assert!(!s.needs_layout());
        assert!(!s.needs_paint());
    }

    #[test]
    fn autotest_changeset_bitor_matches_bitorassign() {
        // Round-trip: the two operators must agree, and BitOr must be
        // commutative + idempotent for arbitrary (including undefined) bits.
        for (a, b) in [
            (0u32, 0u32),
            (0, u32::MAX),
            (u32::MAX, u32::MAX),
            (NodeChangeSet::TEXT_CONTENT, NodeChangeSet::STYLED_STATE),
            (0xDEAD_BEEF, 0x0BAD_F00D),
        ] {
            let (sa, sb) = (NodeChangeSet { bits: a }, NodeChangeSet { bits: b });

            let by_operator = sa | sb;
            assert_eq!(by_operator.bits, a | b);

            let mut by_assign = sa;
            by_assign |= sb;
            assert_eq!(by_assign, by_operator);

            assert_eq!((sb | sa).bits, by_operator.bits, "BitOr must commute");
            assert_eq!((by_operator | by_operator).bits, by_operator.bits);
        }
    }

    // ========================================================================
    // NodeChangeReport — getters / predicates
    // ========================================================================

    #[test]
    fn autotest_change_report_default_is_inert() {
        let r = NodeChangeReport::default();
        assert!(!r.needs_layout());
        assert!(!r.needs_paint());
        assert!(r.is_visually_unchanged());
        assert_eq!(r.relayout_scope, RelayoutScope::None);
        assert!(r.changed_css_properties.is_empty());
        assert!(r.text_change.is_none());
    }

    #[test]
    fn autotest_change_report_scope_alone_forces_layout() {
        // An empty change_set with a non-None scope must still request layout:
        // `needs_layout()` ORs the two sources.
        let r = NodeChangeReport { relayout_scope: RelayoutScope::IfcOnly, ..Default::default() };
        assert!(r.needs_layout());
        assert!(!r.needs_paint());
        assert!(!r.is_visually_unchanged());
    }

    #[test]
    fn autotest_change_report_paint_flag_does_not_force_layout() {
        let mut r = NodeChangeReport::default();
        r.change_set.insert(NodeChangeSet::STYLED_STATE);
        assert!(!r.needs_layout());
        assert!(r.needs_paint());
        assert!(!r.is_visually_unchanged());
    }

    // ========================================================================
    // reconcile_cursor_position — the byte-offset "parser": unicode + boundary
    // ========================================================================

    #[test]
    fn autotest_cursor_result_is_always_a_valid_slice_index() {
        // The core safety invariant: whatever comes back must be <= new.len()
        // AND land on a char boundary, or a later `&new_text[..cursor]` panics.
        // Sweep every in-range cursor over every pair of the unicode corpus.
        for old in UNICODE_SAMPLES {
            for new in UNICODE_SAMPLES {
                for cursor in 0..=old.len() {
                    let r = reconcile_cursor_position(old, new, cursor);
                    assert!(
                        r <= new.len(),
                        "cursor {cursor} in {old:?} -> {r} exceeds len of {new:?}",
                    );
                    assert!(
                        new.is_char_boundary(r),
                        "cursor {cursor} in {old:?} -> {r} splits a codepoint in {new:?}",
                    );
                    // Must be usable as a real slice index.
                    let _ = &new[..r];
                }
            }
        }
    }

    #[test]
    fn autotest_cursor_is_deterministic() {
        // Same inputs must always give the same answer (no hashing / iteration
        // order leaking into the result).
        for old in UNICODE_SAMPLES {
            for new in UNICODE_SAMPLES {
                let a = reconcile_cursor_position(old, new, old.len() / 2);
                let b = reconcile_cursor_position(old, new, old.len() / 2);
                assert_eq!(a, b);
            }
        }
    }

    #[test]
    fn autotest_cursor_zero_stays_zero_when_texts_differ_at_byte_zero() {
        // cursor 0 <= common_prefix (0) -> snap(0) == 0.
        assert_eq!(reconcile_cursor_position("abc", "xyz", 0), 0);
        assert_eq!(reconcile_cursor_position("日本", "中国", 0), 0);
    }

    #[test]
    fn autotest_cursor_identical_text_clamps_to_len() {
        // Equal texts short-circuit to `snap(cursor)`, which clamps to len and
        // snaps down to a char boundary — so even an absurd cursor is safe here.
        assert_eq!(reconcile_cursor_position("abc", "abc", usize::MAX), 3);
        assert_eq!(reconcile_cursor_position("héllo", "héllo", usize::MAX), 6);
        // Snapping down: byte 2 is mid-'é' (bytes 1..3) -> snaps to 1.
        assert_eq!(reconcile_cursor_position("héllo", "héllo", 2), 1);
    }

    #[test]
    fn autotest_cursor_empty_sides_are_documented_constants() {
        // Empty old  -> end of new. Empty new -> 0. Both empty -> 0 (equal-text path).
        for new in UNICODE_SAMPLES {
            assert_eq!(reconcile_cursor_position("", new, 0), new.len());
            assert_eq!(reconcile_cursor_position("", new, usize::MAX), new.len());
        }
        for old in UNICODE_SAMPLES {
            if old.is_empty() {
                continue; // equal-text path, covered above
            }
            assert_eq!(reconcile_cursor_position(old, "", 0), 0);
            assert_eq!(reconcile_cursor_position(old, "", old.len()), 0);
        }
    }

    #[test]
    fn autotest_cursor_appended_text_keeps_prefix_cursor() {
        // Pure append: any cursor inside the common prefix is untouched.
        let old = "Hello";
        let new = "Hello, World";
        for cursor in 0..=old.len() {
            assert_eq!(reconcile_cursor_position(old, new, cursor), cursor);
        }
    }

    #[test]
    fn autotest_cursor_deleted_tail_clamps_into_new_text() {
        // Pure truncation: a cursor past the end of the new text must land at
        // the new end, never beyond it.
        let old = "Hello, World";
        let new = "Hello";
        assert_eq!(reconcile_cursor_position(old, new, old.len()), new.len());
        assert_eq!(reconcile_cursor_position(old, new, 5), 5);
    }

    #[test]
    fn autotest_cursor_multibyte_insert_before_cursor_shifts_by_suffix_rule() {
        // Insert a 2-byte 'ö' at the front; a cursor sitting in the (unchanged)
        // suffix must keep its distance from the END of the string.
        let old = "mega";
        let new = "ömega";
        let r = reconcile_cursor_position(old, new, 4); // end of old
        assert_eq!(r, new.len());
        assert!(new.is_char_boundary(r));
    }

    #[test]
    fn autotest_cursor_huge_inputs_do_not_hang_or_panic() {
        // 200k-byte strings: the prefix/suffix scans are linear, so this must
        // complete quickly and stay in-bounds.
        let old: String = std::iter::repeat_n('a', 200_000).collect();
        let mut new = old.clone();
        new.push_str("tail");

        let r = reconcile_cursor_position(&old, &new, old.len());
        assert!(r <= new.len());
        assert!(new.is_char_boundary(r));

        // Huge multibyte string: every returned offset must still be a boundary.
        let old_u: String = std::iter::repeat_n('é', 50_000).collect();
        let new_u: String = std::iter::repeat_n('é', 49_999).collect();
        let r = reconcile_cursor_position(&old_u, &new_u, old_u.len());
        assert!(r <= new_u.len());
        assert!(new_u.is_char_boundary(r));
    }

    // KNOWN BUG (reported, not fixed here — this module may only touch tests).
    //
    // `reconcile_cursor_position` underflows when the caller passes a cursor
    // byte offset PAST the end of `old_text`:
    //
    //     diff.rs:1163  let offset_from_end = old_text.len() - old_cursor_byte;
    //
    // Reaching it needs: old != new, both non-empty, cursor > common_prefix and
    // cursor >= old_suffix_start. With ("abc", "abd", usize::MAX) the guard at
    // :1145 does not fire (MAX > prefix 2), old_suffix_start is 3, and
    // `3 - usize::MAX` panics with "attempt to subtract with overflow" in debug
    // (silently wraps in release, then `snap()` clamps the garbage — so release
    // hides it). Every other exit from this function is clamped via `snap()`;
    // this one is not. Fix: `old_text.len().saturating_sub(old_cursor_byte)`.
    //
    // Kept as an #[ignore]d executable repro asserting the CORRECT behavior, so
    // it does not fail the suite but flips to green once the fix lands.
    #[test]
    fn autotest_cursor_out_of_range_cursor_must_saturate_not_underflow() {
        // Expected: clamp to the end of the new text, exactly like every other path.
        assert_eq!(reconcile_cursor_position("abc", "abd", usize::MAX), 3);
        assert_eq!(reconcile_cursor_position("abc", "abd", 99), 3);
        assert_eq!(reconcile_cursor_position("héllo", "héllx", usize::MAX), 6);
    }

    // ========================================================================
    // get_node_text_content — round-trip
    // ========================================================================

    #[test]
    fn autotest_text_content_round_trips_unicode() {
        for s in UNICODE_SAMPLES {
            let node = NodeData::create_text(*s);
            assert_eq!(
                get_node_text_content(&node),
                Some(*s),
                "create_text -> get_node_text_content must round-trip {s:?}",
            );
        }
    }

    #[test]
    fn autotest_text_content_is_none_for_non_text_nodes() {
        assert_eq!(get_node_text_content(&NodeData::create_div()), None);
        assert_eq!(get_node_text_content(&NodeData::create_body()), None);
        assert_eq!(get_node_text_content(&NodeData::create_br()), None);
        let img = NodeData::create_image(ImageRef::null_image(
            1,
            1,
            RawImageFormat::RGBA8,
            Vec::new(),
        ));
        assert_eq!(get_node_text_content(&img), None);
    }

    // ========================================================================
    // has_*_callback predicates
    // ========================================================================

    #[test]
    fn autotest_callback_predicates_all_false_without_callbacks() {
        let n = NodeData::create_div();
        assert!(!has_mount_callback(&n));
        assert!(!has_unmount_callback(&n));
        assert!(!has_resize_callback(&n));
        assert!(!has_update_callback(&n));
    }

    #[test]
    fn autotest_callback_predicates_are_mutually_exclusive_per_filter() {
        // Each predicate must recognise exactly its own ComponentEventFilter.
        let cases = [
            (ComponentEventFilter::AfterMount, [true, false, false, false]),
            (ComponentEventFilter::BeforeUnmount, [false, true, false, false]),
            (ComponentEventFilter::NodeResized, [false, false, true, false]),
            (ComponentEventFilter::Updated, [false, false, false, true]),
            // A Component filter that none of the four predicates handle.
            (ComponentEventFilter::Selected, [false, false, false, false]),
            (ComponentEventFilter::DefaultAction, [false, false, false, false]),
        ];

        for (filter, expected) in cases {
            let n = with_cb(NodeData::create_div(), filter);
            let got = [
                has_mount_callback(&n),
                has_unmount_callback(&n),
                has_resize_callback(&n),
                has_update_callback(&n),
            ];
            assert_eq!(got, expected, "predicate mismatch for {filter:?}");
        }
    }

    #[test]
    fn autotest_callback_predicates_find_target_among_many() {
        // The target callback is last of several — `any()` must still find it.
        let mut n = NodeData::create_div();
        for f in [
            ComponentEventFilter::Selected,
            ComponentEventFilter::DefaultAction,
            ComponentEventFilter::NodeResized,
        ] {
            n = with_cb(n, f);
        }
        assert!(has_resize_callback(&n));
        assert!(!has_mount_callback(&n));
    }

    // ========================================================================
    // create_lifecycle_event (private)
    // ========================================================================

    #[test]
    fn autotest_lifecycle_event_fields_are_wired_consistently() {
        let ts = Instant::now();
        let ev = create_lifecycle_event(
            EventType::Mount,
            NodeId::new(1_000_000),
            DomId::ROOT_ID,
            &ts,
            LifecycleEventData {
                reason: LifecycleReason::InitialMount,
                previous_bounds: None,
                current_bounds: rect(1.0, 2.0),
            },
        );

        assert_eq!(ev.event_type, EventType::Mount);
        assert_eq!(ev.source, EventSource::Lifecycle);
        assert_eq!(ev.phase, EventPhase::Target);
        // A lifecycle event is delivered at its target, so the two must agree.
        assert_eq!(ev.target, ev.current_target);
        assert_eq!(
            ev.target.node.into_crate_internal(),
            Some(NodeId::new(1_000_000)),
            "NodeId must survive the 1-based NodeHierarchyItemId encoding",
        );
        assert!(!ev.stopped);
        assert!(!ev.stopped_immediate);
        assert!(!ev.prevented_default);

        let EventData::Lifecycle(data) = &ev.data else {
            panic!("expected EventData::Lifecycle, got {:?}", ev.data);
        };
        assert!(data.previous_bounds.is_none());
        assert_eq!(data.current_bounds, rect(1.0, 2.0));
    }

    // ========================================================================
    // compute_node_changes
    // ========================================================================

    #[test]
    fn autotest_compute_changes_identical_nodes_report_nothing() {
        let a = NodeData::create_text("same");
        let b = NodeData::create_text("same");
        let changes = compute_node_changes(&a, &b, None, None);
        assert!(
            changes.is_empty(),
            "identical nodes must produce no change flags, got {:#b}",
            changes.bits,
        );
        assert!(changes.is_visually_unchanged());
    }

    #[test]
    fn autotest_compute_changes_node_type_change_short_circuits_everything() {
        // The documented early-return: when the discriminant changes, NOTHING
        // else is inspected — even though these two nodes ALSO differ in
        // classes, callbacks, inline CSS, tab index and contenteditable, and
        // sit in different styled states.
        let old = NodeData::create_div();
        let new = with_cb(
            NodeData::create_text("now a text node")
                .with_ids_and_classes(vec![IdOrClass::Class("brand-new".into())].into())
                .with_css("width: 10px")
                .with_tab_index(TabIndex::NoKeyboardFocus)
                .with_contenteditable(true),
            ComponentEventFilter::AfterMount,
        );

        let hovered = StyledNodeState {
            hover: true,
            ..StyledNodeState::default()
        };
        let changes = compute_node_changes(
            &old,
            &new,
            Some(&StyledNodeState::default()),
            Some(&hovered),
        );
        assert_eq!(
            changes.bits,
            NodeChangeSet::NODE_TYPE_CHANGED,
            "a node-type change must be reported alone (early return)",
        );
    }

    #[test]
    fn autotest_compute_changes_text_content_unicode() {
        for (i, s) in UNICODE_SAMPLES.iter().enumerate() {
            let old = NodeData::create_text(*s);

            // Same text -> no TEXT_CONTENT flag.
            let same = NodeData::create_text(*s);
            assert!(
                !compute_node_changes(&old, &same, None, None)
                    .contains(NodeChangeSet::TEXT_CONTENT),
                "identical text {s:?} must not report TEXT_CONTENT",
            );

            // Different text -> TEXT_CONTENT flag.
            let other = UNICODE_SAMPLES[(i + 1) % UNICODE_SAMPLES.len()];
            if other == *s {
                continue;
            }
            let changed = NodeData::create_text(other);
            assert!(
                compute_node_changes(&old, &changed, None, None)
                    .contains(NodeChangeSet::TEXT_CONTENT),
                "{s:?} -> {other:?} must report TEXT_CONTENT",
            );
        }
    }

    #[test]
    fn autotest_compute_changes_paint_only_css_never_sets_layout() {
        // `color` is RelayoutScope::None -> paint bucket only.
        let old = NodeData::create_div().with_css("color: red");
        let new = NodeData::create_div().with_css("color: blue");
        let changes = compute_node_changes(&old, &new, None, None);

        assert!(changes.contains(NodeChangeSet::INLINE_STYLE_PAINT));
        assert!(
            !changes.contains(NodeChangeSet::INLINE_STYLE_LAYOUT),
            "a paint-only property must not request relayout",
        );
        assert!(changes.needs_paint());
        assert!(!changes.needs_layout());
    }

    #[test]
    fn autotest_compute_changes_sizing_css_sets_layout_not_paint() {
        // `width` is RelayoutScope::SizingOnly -> layout bucket.
        let old = NodeData::create_div().with_css("width: 10px");
        let new = NodeData::create_div().with_css("width: 20px");
        let changes = compute_node_changes(&old, &new, None, None);

        assert!(changes.contains(NodeChangeSet::INLINE_STYLE_LAYOUT));
        assert!(!changes.contains(NodeChangeSet::INLINE_STYLE_PAINT));
        assert!(changes.needs_layout());
    }

    #[test]
    fn autotest_compute_changes_detects_removed_property() {
        // Regression guard for the AUDIT note at diff.rs:270 — a property that
        // exists only on the OLD node (i.e. was removed) must still be marked.
        let old = NodeData::create_div().with_css("color: red");
        let new = NodeData::create_div();
        let changes = compute_node_changes(&old, &new, None, None);
        assert!(
            changes.contains(NodeChangeSet::INLINE_STYLE_PAINT),
            "removing an inline property must be reported, got {:#b}",
            changes.bits,
        );
    }

    #[test]
    fn autotest_compute_changes_detects_added_property() {
        let old = NodeData::create_div();
        let new = NodeData::create_div().with_css("width: 5px");
        let changes = compute_node_changes(&old, &new, None, None);
        assert!(changes.contains(NodeChangeSet::INLINE_STYLE_LAYOUT));
    }

    #[test]
    fn autotest_compute_changes_ids_and_classes() {
        let changes = compute_node_changes(&class_node("a"), &class_node("b"), None, None);
        assert!(changes.contains(NodeChangeSet::IDS_AND_CLASSES));

        // Same classes -> no flag.
        let changes = compute_node_changes(&class_node("a"), &class_node("a"), None, None);
        assert!(!changes.contains(NodeChangeSet::IDS_AND_CLASSES));
    }

    #[test]
    fn autotest_compute_changes_styled_state() {
        let n = NodeData::create_div();
        let calm = StyledNodeState::default();
        let hovered = StyledNodeState {
            hover: true,
            ..StyledNodeState::default()
        };

        let changes = compute_node_changes(&n, &n, Some(&calm), Some(&hovered));
        assert!(changes.contains(NodeChangeSet::STYLED_STATE));
        assert!(changes.needs_paint());
        assert!(!changes.needs_layout());

        // Same state -> no flag; and None/None -> no flag.
        assert!(!compute_node_changes(&n, &n, Some(&calm), Some(&calm))
            .contains(NodeChangeSet::STYLED_STATE));
        assert!(!compute_node_changes(&n, &n, None, None).contains(NodeChangeSet::STYLED_STATE));

        // None vs Some(default) are *different* inputs and must be reported.
        assert!(compute_node_changes(&n, &n, None, Some(&calm))
            .contains(NodeChangeSet::STYLED_STATE));
    }

    #[test]
    fn autotest_compute_changes_tab_index_and_contenteditable() {
        let plain = NodeData::create_div();

        let editable = NodeData::create_div().with_contenteditable(true);
        let changes = compute_node_changes(&plain, &editable, None, None);
        assert!(changes.contains(NodeChangeSet::CONTENTEDITABLE));
        assert!(changes.needs_layout(), "CONTENTEDITABLE is in AFFECTS_LAYOUT");

        let tabbed = NodeData::create_div().with_tab_index(TabIndex::OverrideInParent(3));
        let changes = compute_node_changes(&plain, &tabbed, None, None);
        assert!(changes.contains(NodeChangeSet::TAB_INDEX));
        // TAB_INDEX is in neither composite mask -> no visual work.
        assert!(changes.is_visually_unchanged());
    }

    #[test]
    fn autotest_compute_changes_callbacks_count_and_identity() {
        let plain = NodeData::create_div();
        let one = with_cb(NodeData::create_div(), ComponentEventFilter::AfterMount);

        // Different callback counts.
        let changes = compute_node_changes(&plain, &one, None, None);
        assert!(changes.contains(NodeChangeSet::CALLBACKS));
        assert!(changes.is_visually_unchanged(), "callbacks are not a visual change");

        // Same count, different event filter.
        let other = with_cb(NodeData::create_div(), ComponentEventFilter::BeforeUnmount);
        let changes = compute_node_changes(&one, &other, None, None);
        assert!(changes.contains(NodeChangeSet::CALLBACKS));

        // Same count, same filter -> no flag (cb pointer 0 == 0).
        let same = with_cb(NodeData::create_div(), ComponentEventFilter::AfterMount);
        let changes = compute_node_changes(&one, &same, None, None);
        assert!(!changes.contains(NodeChangeSet::CALLBACKS));
    }

    #[test]
    fn autotest_compute_changes_image_identity_is_by_image_id() {
        // `ImageRef` hashes its process-unique `id`: shallow clones share it,
        // every fresh `null_image()` gets a new one.
        let img = ImageRef::null_image(4, 4, RawImageFormat::RGBA8, Vec::new());
        let same = NodeData::create_image(img.clone());
        let also_same = NodeData::create_image(img.clone());
        assert!(
            !compute_node_changes(&same, &also_same, None, None)
                .contains(NodeChangeSet::IMAGE_CHANGED),
            "two nodes holding clones of the SAME ImageRef must not report a change",
        );

        // A distinct allocation, even with identical pixels/dimensions, is a
        // different image as far as reconciliation is concerned.
        let other = NodeData::create_image(ImageRef::null_image(
            4,
            4,
            RawImageFormat::RGBA8,
            Vec::new(),
        ));
        let changes = compute_node_changes(&same, &other, None, None);
        assert!(changes.contains(NodeChangeSet::IMAGE_CHANGED));
        assert!(changes.needs_layout(), "IMAGE_CHANGED is in AFFECTS_LAYOUT");
    }

    // ========================================================================
    // calculate_reconciliation_key / precompute_reconciliation_keys
    // ========================================================================

    #[test]
    fn autotest_rec_key_empty_node_data_is_safe() {
        assert!(precompute_reconciliation_keys(&[], &[]).is_empty());
    }

    #[test]
    fn autotest_rec_key_precompute_matches_per_node_calculation() {
        // Round-trip: the O(1)-lookup table must agree with the direct call for
        // every node — the whole point of precomputing.
        let node_data = vec![
            NodeData::create_div(),
            class_node("row"),
            NodeData::create_text("leaf"),
            id_node("footer"),
        ];
        let hierarchy = vec![
            hitem(None, None, None, Some(3)),
            hitem(Some(0), None, Some(2), None),
            hitem(Some(0), Some(1), Some(3), None),
            hitem(Some(0), Some(2), None, None),
        ];

        let keys = precompute_reconciliation_keys(&node_data, &hierarchy);
        assert_eq!(keys.len(), node_data.len());
        for (i, k) in keys.iter().enumerate() {
            assert_eq!(
                *k,
                calculate_reconciliation_key(&node_data, &hierarchy, NodeId::new(i)),
                "precomputed key for node {i} disagrees with the direct call",
            );
        }
    }

    #[test]
    fn autotest_rec_key_explicit_key_beats_css_id_and_node_type() {
        // Priority 1 is absolute: it ignores the CSS ID, the classes, the node
        // type and the position in the tree.
        let bare = NodeData::create_div().with_key(7u32);
        let decorated = NodeData::create_text("totally different")
            .with_key(7u32)
            .with_ids_and_classes(
                vec![IdOrClass::Id("hero".into()), IdOrClass::Class("x".into())].into(),
            );

        let a = calculate_reconciliation_key(&[bare], &[], NodeId::new(0));
        let b = calculate_reconciliation_key(&[decorated], &[], NodeId::new(0));
        assert_eq!(a, b, "an explicit .with_key() must dominate every other input");
    }

    #[test]
    fn autotest_rec_key_css_id_used_when_no_explicit_key() {
        let same_a = calculate_reconciliation_key(&[id_node("hero")], &[], NodeId::new(0));
        let same_b = calculate_reconciliation_key(&[id_node("hero")], &[], NodeId::new(0));
        let other = calculate_reconciliation_key(&[id_node("footer")], &[], NodeId::new(0));

        assert_eq!(same_a, same_b, "the CSS-ID key must be stable");
        assert_ne!(same_a, other, "different CSS IDs must produce different keys");
    }

    #[test]
    fn autotest_rec_key_classes_participate_in_the_structural_key() {
        let a = calculate_reconciliation_key(&[class_node("alpha")], &[], NodeId::new(0));
        let b = calculate_reconciliation_key(&[class_node("beta")], &[], NodeId::new(0));
        assert_ne!(a, b, "classes must feed the structural key");
    }

    #[test]
    fn autotest_rec_key_node_type_participates_in_the_structural_key() {
        let div = calculate_reconciliation_key(&[NodeData::create_div()], &[], NodeId::new(0));
        let txt =
            calculate_reconciliation_key(&[NodeData::create_text("x")], &[], NodeId::new(0));
        assert_ne!(div, txt, "the node-type discriminant must feed the structural key");
    }

    #[test]
    fn autotest_rec_key_hierarchy_shorter_than_node_data_is_safe() {
        // A truncated / absent hierarchy must degrade to the documented
        // "discriminant + classes" key instead of panicking.
        let node_data = vec![NodeData::create_div(), class_node("a"), id_node("b")];

        let with_none = precompute_reconciliation_keys(&node_data, &[]);
        let with_short = precompute_reconciliation_keys(&node_data, &[hitem(None, None, None, None)]);

        assert_eq!(with_none.len(), 3);
        assert_eq!(with_short.len(), 3);
        // Node 0 is a root either way, so both spellings must agree on it.
        assert_eq!(with_none[0], with_short[0]);
    }

    #[test]
    fn autotest_rec_key_identical_leaves_under_different_parents_differ() {
        // The parent chain must be folded in, otherwise keyless nodes under
        // unrelated parents would collide and migrate state across subtrees.
        //
        //   0 root
        //   ├── 1 (#left)   ── 3 div
        //   └── 2 (#right)  ── 4 div
        let node_data = vec![
            NodeData::create_div(),
            id_node("left"),
            id_node("right"),
            NodeData::create_div(),
            NodeData::create_div(),
        ];
        let hierarchy = vec![
            hitem(None, None, None, Some(2)),       // 0: children 1,2
            hitem(Some(0), None, Some(2), Some(3)), // 1: child 3
            hitem(Some(0), Some(1), None, Some(4)), // 2: child 4
            hitem(Some(1), None, None, None),       // 3
            hitem(Some(2), None, None, None),       // 4
        ];

        let k3 = calculate_reconciliation_key(&node_data, &hierarchy, NodeId::new(3));
        let k4 = calculate_reconciliation_key(&node_data, &hierarchy, NodeId::new(4));
        assert_ne!(k3, k4, "identical leaves under different parents must not share a key");
    }

    // ========================================================================
    // calculate_contenteditable_key
    // ========================================================================

    #[test]
    fn autotest_contenteditable_key_is_deterministic_and_honours_explicit_keys() {
        let node_data = vec![NodeData::create_div().with_key(99u64)];
        let a = calculate_contenteditable_key(&node_data, &[], NodeId::new(0));
        let b = calculate_contenteditable_key(&node_data, &[], NodeId::new(0));
        assert_eq!(a, b, "must be deterministic");

        // Priority 1 is shared with the reconciliation key: for an explicitly
        // keyed node both functions return the SAME value.
        assert_eq!(
            a,
            calculate_reconciliation_key(&node_data, &[], NodeId::new(0)),
            "explicit keys must be identical across both key functions",
        );
    }

    #[test]
    fn autotest_contenteditable_key_distinguishes_nth_of_type() {
        // <div><p>A</p><p contenteditable>B</p></div> — the two same-type
        // siblings must not collide (nth-of-type is folded in).
        let node_data = vec![
            NodeData::create_div(),
            NodeData::create_text("A"),
            NodeData::create_text("B"),
        ];
        let hierarchy = vec![
            hitem(None, None, None, Some(2)),
            hitem(Some(0), None, Some(2), None),
            hitem(Some(0), Some(1), None, None),
        ];

        let k1 = calculate_contenteditable_key(&node_data, &hierarchy, NodeId::new(1));
        let k2 = calculate_contenteditable_key(&node_data, &hierarchy, NodeId::new(2));
        assert_ne!(k1, k2, "same-type siblings must differ by nth-of-type");
    }

    #[test]
    fn autotest_contenteditable_key_empty_hierarchy_is_safe() {
        let node_data = vec![NodeData::create_div(), class_node("editor")];
        for i in 0..node_data.len() {
            let k = calculate_contenteditable_key(&node_data, &[], NodeId::new(i));
            assert_eq!(k, calculate_contenteditable_key(&node_data, &[], NodeId::new(i)));
        }
    }

    // ========================================================================
    // reconcile_dom
    // ========================================================================

    #[test]
    fn autotest_reconcile_empty_to_empty_is_a_no_op() {
        let r = diff_flat(&[], &[]);
        assert!(r.events.is_empty());
        assert!(r.node_moves.is_empty());
    }

    #[test]
    fn autotest_reconcile_mount_and_unmount_need_a_callback_to_fire() {
        // Without an AfterMount callback the node still mounts — it just fires
        // no event. Same for unmount. The events are opt-in.
        let silent_new = vec![NodeData::create_div()];
        let r = diff_flat(&[], &silent_new);
        assert!(r.events.is_empty(), "no callback -> no event");
        assert!(r.node_moves.is_empty());

        let loud_new = vec![with_cb(NodeData::create_div(), ComponentEventFilter::AfterMount)];
        let r = diff_flat(&[], &loud_new);
        assert_eq!(count_events(&r, EventType::Mount), 1);

        let loud_old = vec![with_cb(
            NodeData::create_div(),
            ComponentEventFilter::BeforeUnmount,
        )];
        let r = diff_flat(&loud_old, &[]);
        assert_eq!(count_events(&r, EventType::Unmount), 1);
        assert!(r.node_moves.is_empty());
    }

    #[test]
    fn autotest_reconcile_node_moves_are_a_bijection() {
        // 50 indistinguishable divs on both sides: every old node must be
        // claimed exactly once and every new node must claim at most one old
        // node. A queue bug (double-consume) would break this immediately.
        let old: Vec<NodeData> = (0..50).map(|_| NodeData::create_div()).collect();
        let new: Vec<NodeData> = (0..50).map(|_| NodeData::create_div()).collect();

        let r = diff_flat(&old, &new);
        assert_eq!(r.node_moves.len(), 50);

        let mut seen_old = [false; 50];
        let mut seen_new = [false; 50];
        for m in &r.node_moves {
            assert!(!seen_old[m.old_node_id.index()], "old node claimed twice");
            assert!(!seen_new[m.new_node_id.index()], "new node matched twice");
            seen_old[m.old_node_id.index()] = true;
            seen_new[m.new_node_id.index()] = true;
        }
        assert!(seen_old.iter().all(|b| *b), "every old node must be claimed");
        assert!(seen_new.iter().all(|b| *b), "every new node must be matched");
        assert!(r.events.is_empty(), "no lifecycle callbacks -> no events");
    }

    #[test]
    fn autotest_reconcile_surplus_new_nodes_mount_and_surplus_old_unmount() {
        // 50 old, 60 new -> 50 matches + 10 mounts, no unmounts.
        let old: Vec<NodeData> = (0..50).map(|_| NodeData::create_div()).collect();
        let new: Vec<NodeData> = (0..60)
            .map(|_| with_cb(NodeData::create_div(), ComponentEventFilter::AfterMount))
            .collect();

        let r = diff_flat(&old, &new);
        assert_eq!(r.node_moves.len(), 50);
        assert_eq!(count_events(&r, EventType::Mount), 10);
        assert_eq!(count_events(&r, EventType::Unmount), 0);

        // 50 old, 40 new -> 40 matches + 10 unmounts.
        let old: Vec<NodeData> = (0..50)
            .map(|_| with_cb(NodeData::create_div(), ComponentEventFilter::BeforeUnmount))
            .collect();
        let new: Vec<NodeData> = (0..40).map(|_| NodeData::create_div()).collect();

        let r = diff_flat(&old, &new);
        assert_eq!(r.node_moves.len(), 40);
        assert_eq!(count_events(&r, EventType::Unmount), 10);
        assert_eq!(count_events(&r, EventType::Mount), 0);
    }

    #[test]
    fn autotest_reconcile_explicit_key_mismatch_mounts_instead_of_guessing() {
        // The documented rule: an explicit `.with_key()` that finds no partner
        // must NOT fall through to the content/structural tiers, even though
        // the two nodes are otherwise byte-identical.
        let old = vec![with_cb(
            NodeData::create_text("same content").with_key(1u32),
            ComponentEventFilter::BeforeUnmount,
        )];
        let new = vec![with_cb(
            NodeData::create_text("same content").with_key(2u32),
            ComponentEventFilter::AfterMount,
        )];

        let r = diff_flat(&old, &new);
        assert!(
            r.node_moves.is_empty(),
            "keys 1 and 2 must not match, got {:?}",
            r.node_moves,
        );
        assert_eq!(count_events(&r, EventType::Mount), 1);
        assert_eq!(count_events(&r, EventType::Unmount), 1);
    }

    #[test]
    fn autotest_reconcile_update_fires_only_on_rec_key_match_with_changed_content() {
        // Same key, changed text, Updated callback present -> Update event.
        let old = vec![NodeData::create_text("v1").with_key(1u32)];
        let new = vec![with_cb(
            NodeData::create_text("v2").with_key(1u32),
            ComponentEventFilter::Updated,
        )];

        let r = diff_flat(&old, &new);
        assert_eq!(r.node_moves.len(), 1, "the key must match across frames");
        assert_eq!(count_events(&r, EventType::Update), 1);

        // Same key, SAME content -> no Update. Both frames must be byte-identical
        // for this: `NodeData::hash` folds in the callback events too (dom.rs:1579),
        // so the Updated handler has to be present on BOTH sides — otherwise the
        // hashes differ for the callback alone and we'd be testing nothing.
        let stable = with_cb(
            NodeData::create_text("v1").with_key(1u32),
            ComponentEventFilter::Updated,
        );
        let old = vec![stable.clone()];
        let new = vec![stable];

        let r = diff_flat(&old, &new);
        assert_eq!(r.node_moves.len(), 1);
        assert_eq!(
            count_events(&r, EventType::Update),
            0,
            "unchanged content must not fire Update",
        );
    }

    #[test]
    fn autotest_reconcile_update_requires_the_callback() {
        // Content changed under a stable key, but no Updated callback -> silent.
        let old = vec![NodeData::create_text("v1").with_key(1u32)];
        let new = vec![NodeData::create_text("v2").with_key(1u32)];
        let r = diff_flat(&old, &new);
        assert_eq!(r.node_moves.len(), 1);
        assert!(r.events.is_empty());
    }

    #[test]
    fn autotest_reconcile_missing_layout_entries_default_to_zero_rect() {
        // Neither side has layout data: `unwrap_or(LogicalRect::zero())` means
        // the sizes compare equal, so no Resize fires and nothing panics.
        let old = vec![NodeData::create_div()];
        let new = vec![with_cb(
            NodeData::create_div(),
            ComponentEventFilter::NodeResized,
        )];

        let r = diff_flat(&old, &new);
        assert_eq!(r.node_moves.len(), 1);
        assert_eq!(
            count_events(&r, EventType::Resize),
            0,
            "zero-vs-zero bounds must not be treated as a resize",
        );
    }

    #[test]
    fn autotest_reconcile_resize_fires_with_previous_and_current_bounds() {
        let old = vec![NodeData::create_div()];
        let new = vec![with_cb(
            NodeData::create_div(),
            ComponentEventFilter::NodeResized,
        )];

        let r = reconcile_dom(
            &old,
            &new,
            &[],
            &[],
            &layout_of(&[(0, rect(100.0, 50.0))]),
            &layout_of(&[(0, rect(100.0, 80.0))]),
            DomId::ROOT_ID,
            Instant::now(),
        );

        assert_eq!(count_events(&r, EventType::Resize), 1);
        let EventData::Lifecycle(data) = &r.events[0].data else {
            panic!("resize event must carry EventData::Lifecycle");
        };
        assert_eq!(data.reason, LifecycleReason::Resize);
        assert_eq!(data.previous_bounds, Some(rect(100.0, 50.0)));
        assert_eq!(data.current_bounds, rect(100.0, 80.0));
    }

    #[test]
    fn autotest_reconcile_resize_ignores_pure_translation() {
        // Only `size` is compared — moving a node must not fire Resize.
        let old = vec![NodeData::create_div()];
        let new = vec![with_cb(
            NodeData::create_div(),
            ComponentEventFilter::NodeResized,
        )];

        let moved = LogicalRect::new(LogicalPosition::new(999.0, 999.0), LogicalSize::new(10.0, 10.0));
        let r = reconcile_dom(
            &old,
            &new,
            &[],
            &[],
            &layout_of(&[(0, rect(10.0, 10.0))]),
            &layout_of(&[(0, moved)]),
            DomId::ROOT_ID,
            Instant::now(),
        );
        assert_eq!(count_events(&r, EventType::Resize), 0);
    }

    #[test]
    fn autotest_reconcile_nan_bounds_do_not_fire_a_resize_every_frame() {
        // NUMERIC EDGE — the sharpest one in this file.
        //
        // The Resize check is `old_rect.size != new_rect.size`. With a DERIVED
        // f32 `PartialEq` this would be catastrophic: `NaN != NaN` is true, so a
        // node whose layout solved to NaN would be reported as "resized" on
        // EVERY frame forever, firing an endless Resize-callback storm on a
        // completely static layout.
        //
        // `LogicalSize` dodges that with a hand-written `PartialEq` that runs
        // both operands through `geom::quantize()`, which maps every NaN to the
        // single sentinel `i64::MIN` (geom.rs:218) — so all NaNs compare EQUAL.
        // This test pins that: revert `LogicalSize` to `#[derive(PartialEq)]`
        // and it goes red.
        let old = vec![NodeData::create_div()];
        let new = vec![with_cb(
            NodeData::create_div(),
            ComponentEventFilter::NodeResized,
        )];

        let nan = rect(f32::NAN, f32::NAN);
        let r = reconcile_dom(
            &old,
            &new,
            &[],
            &[],
            &layout_of(&[(0, nan)]),
            &layout_of(&[(0, nan)]),
            DomId::ROOT_ID,
            Instant::now(),
        );
        assert_eq!(r.node_moves.len(), 1);
        assert_eq!(
            count_events(&r, EventType::Resize),
            0,
            "an unchanged NaN size must not be reported as a resize",
        );

        // Infinities are likewise stable against themselves (they saturate to
        // i64::MAX / i64::MIN under quantize()).
        let inf = rect(f32::INFINITY, f32::NEG_INFINITY);
        let r = reconcile_dom(
            &old,
            &new,
            &[],
            &[],
            &layout_of(&[(0, inf)]),
            &layout_of(&[(0, inf)]),
            DomId::ROOT_ID,
            Instant::now(),
        );
        assert_eq!(
            count_events(&r, EventType::Resize),
            0,
            "infinite-but-equal bounds must not be treated as a resize",
        );

        // But a NaN -> real transition IS a genuine resize, and must still fire.
        let r = reconcile_dom(
            &old,
            &new,
            &[],
            &[],
            &layout_of(&[(0, nan)]),
            &layout_of(&[(0, rect(10.0, 20.0))]),
            DomId::ROOT_ID,
            Instant::now(),
        );
        assert_eq!(
            count_events(&r, EventType::Resize),
            1,
            "NaN -> a real size is a real resize",
        );
    }

    #[test]
    fn autotest_reconcile_extreme_bounds_do_not_panic() {
        // f32 MIN/MAX/subnormal bounds must flow through the Resize comparison
        // without arithmetic surprises (the code only compares, never subtracts).
        let old = vec![NodeData::create_div()];
        let new = vec![with_cb(
            NodeData::create_div(),
            ComponentEventFilter::NodeResized,
        )];

        for (a, b) in [
            (rect(f32::MIN, f32::MAX), rect(f32::MAX, f32::MIN)),
            (rect(f32::MIN_POSITIVE, 0.0), rect(0.0, f32::MIN_POSITIVE)),
            (rect(-0.0, 0.0), rect(0.0, -0.0)), // IEEE: -0.0 == 0.0
        ] {
            let r = reconcile_dom(
                &old,
                &new,
                &[],
                &[],
                &layout_of(&[(0, a)]),
                &layout_of(&[(0, b)]),
                DomId::ROOT_ID,
                Instant::now(),
            );
            assert_eq!(r.node_moves.len(), 1);
        }
    }

    #[test]
    fn autotest_reconcile_keyless_tiers_respect_the_parent_key_gate() {
        // Regression guard for the AUDIT note at diff.rs:601. Two structurally
        // identical leaves live under DIFFERENT parents. The content-hash and
        // structural-hash tiers must not match them across parents, or focus /
        // scroll / dataset state migrates into an unrelated subtree.
        //
        // old:  0 root ── 1 (#left)  ── 2 "leaf"
        // new:  0 root ── 1 (#right) ── 2 "leaf"
        let old_nd = vec![
            NodeData::create_div(),
            id_node("left"),
            NodeData::create_text("leaf"),
        ];
        let old_hier = vec![
            hitem(None, None, None, Some(1)),
            hitem(Some(0), None, None, Some(2)),
            hitem(Some(1), None, None, None),
        ];

        let new_nd = vec![
            NodeData::create_div(),
            id_node("right"),
            NodeData::create_text("leaf"),
        ];
        let new_hier = old_hier.clone();

        let r = reconcile_dom(
            &old_nd,
            &new_nd,
            &old_hier,
            &new_hier,
            &no_layout(),
            &no_layout(),
            DomId::ROOT_ID,
            Instant::now(),
        );

        // The leaf (index 2) must NOT be matched: its parent's reconciliation
        // key differs (#left vs #right), so both keyless tiers are gated off.
        let leaf_matched = r
            .node_moves
            .iter()
            .any(|m| m.new_node_id.index() == 2 && m.old_node_id.index() == 2);
        assert!(
            !leaf_matched,
            "a leaf must not migrate across parents; moves = {:?}",
            r.node_moves,
        );
    }

    // ========================================================================
    // create_migration_map
    // ========================================================================

    #[test]
    fn autotest_migration_map_empty_and_large() {
        assert!(create_migration_map(&[]).is_empty());

        let moves: Vec<NodeMove> = (0..1000)
            .map(|i| NodeMove {
                old_node_id: NodeId::new(i),
                new_node_id: NodeId::new(i * 2),
            })
            .collect();
        let map = create_migration_map(&moves);
        assert_eq!(map.len(), 1000);
        assert_eq!(map.get(&NodeId::new(999)), Some(&NodeId::new(1998)));
    }

    #[test]
    fn autotest_migration_map_duplicate_old_id_keeps_the_last_write() {
        // The map is a BTreeMap, so a repeated old id overwrites. Pin it: a
        // silent "first wins" flip would strand focus on a stale node.
        let moves = vec![
            NodeMove {
                old_node_id: NodeId::new(0),
                new_node_id: NodeId::new(5),
            },
            NodeMove {
                old_node_id: NodeId::new(0),
                new_node_id: NodeId::new(9),
            },
        ];
        let map = create_migration_map(&moves);
        assert_eq!(map.len(), 1);
        assert_eq!(map.get(&NodeId::new(0)), Some(&NodeId::new(9)));
    }

    #[test]
    fn autotest_migration_map_round_trips_a_real_diff() {
        let old: Vec<NodeData> = (0..8).map(|_| NodeData::create_div()).collect();
        let new: Vec<NodeData> = (0..8).map(|_| NodeData::create_div()).collect();
        let r = diff_flat(&old, &new);

        let map = create_migration_map(&r.node_moves);
        assert_eq!(map.len(), r.node_moves.len());
        for m in &r.node_moves {
            assert_eq!(map.get(&m.old_node_id), Some(&m.new_node_id));
        }
    }

    // ========================================================================
    // transfer_states
    // ========================================================================

    #[allow(dead_code)]
    struct TestState(u32);

    // Keeps the PERSISTENT (old) allocation, discarding the fresh one — the
    // real-world case (MapWidget's tile cache is written by background threads).
    extern "C" fn merge_keep_old(_new_data: RefAny, old_data: RefAny) -> RefAny {
        old_data
    }

    #[test]
    fn autotest_transfer_states_out_of_range_moves_are_skipped() {
        // The bounds guard must swallow a corrupt NodeMove instead of indexing
        // out of bounds.
        let mut old = vec![NodeData::create_div()];
        let mut new = vec![NodeData::create_div()];

        let moves = vec![
            NodeMove {
                old_node_id: NodeId::new(5), // out of range
                new_node_id: NodeId::new(0),
            },
            NodeMove {
                old_node_id: NodeId::new(0),
                new_node_id: NodeId::new(7), // out of range
            },
            NodeMove {
                old_node_id: NodeId::new(usize::MAX),
                new_node_id: NodeId::new(usize::MAX),
            },
        ];

        transfer_states(&mut old, &mut new, &moves); // must not panic
        assert!(new[0].get_dataset().is_none());
    }

    #[test]
    fn autotest_transfer_states_without_merge_callback_leaves_datasets_intact() {
        let mut old = vec![NodeData::create_div()];
        old[0].set_dataset(OptionRefAny::Some(RefAny::new(TestState(1))));

        let mut new = vec![NodeData::create_div()];
        new[0].set_dataset(OptionRefAny::Some(RefAny::new(TestState(2))));

        let old_ptr = old[0].get_dataset().unwrap().sharing_info.ptr as usize;
        let new_ptr = new[0].get_dataset().unwrap().sharing_info.ptr as usize;

        transfer_states(
            &mut old,
            &mut new,
            &[NodeMove {
                old_node_id: NodeId::new(0),
                new_node_id: NodeId::new(0),
            }],
        );

        // No merge callback -> early `continue`, both datasets stay where they were.
        assert_eq!(
            old[0].get_dataset().unwrap().sharing_info.ptr as usize,
            old_ptr,
        );
        assert_eq!(
            new[0].get_dataset().unwrap().sharing_info.ptr as usize,
            new_ptr,
        );
    }

    #[test]
    fn autotest_transfer_states_with_one_missing_dataset_restores_both_sides() {
        // Merge callback present, but the OLD node has no dataset -> the
        // `(new_ds, old_ds)` arm must put the taken dataset back.
        let mut old = vec![NodeData::create_div()];
        let mut new = vec![NodeData::create_div()];
        new[0].set_merge_callback(merge_keep_old as DatasetMergeCallbackType);
        new[0].set_dataset(OptionRefAny::Some(RefAny::new(TestState(2))));

        let new_ptr = new[0].get_dataset().unwrap().sharing_info.ptr as usize;

        transfer_states(
            &mut old,
            &mut new,
            &[NodeMove {
                old_node_id: NodeId::new(0),
                new_node_id: NodeId::new(0),
            }],
        );

        assert!(old[0].get_dataset().is_none());
        assert_eq!(
            new[0].get_dataset().unwrap().sharing_info.ptr as usize,
            new_ptr,
            "the fresh dataset must be restored, not dropped",
        );
    }

    #[test]
    fn autotest_transfer_states_repoints_orphaned_callback_refanys() {
        // The unification rule (diff.rs:909): a widget builds its dataset AND
        // its callback refanys from clones of ONE RefAny. When the merge keeps
        // the OLD allocation, every clone of the FRESH one is orphaned and must
        // be re-pointed at the merged result — otherwise the widget fragments
        // across two caches (the MapWidget grey-tile bug).
        let fresh = RefAny::new(TestState(1));
        let fresh_ptr = fresh.sharing_info.ptr as usize;

        let mut new0 = NodeData::create_div();
        new0.set_merge_callback(merge_keep_old as DatasetMergeCallbackType);
        new0.set_dataset(OptionRefAny::Some(fresh.clone()));
        // A callback on the SAME node, holding a clone of the fresh allocation.
        new0.add_callback(
            EventFilter::Component(ComponentEventFilter::Selected),
            fresh.clone(),
            noop_callback(),
        );

        // A *sibling* node whose callback also clones the fresh allocation —
        // the generalised sweep must reach it too, not just the merge node.
        let mut new1 = NodeData::create_div();
        new1.add_callback(
            EventFilter::Component(ComponentEventFilter::Selected),
            fresh.clone(),
            noop_callback(),
        );

        let persistent = RefAny::new(TestState(2));
        let persistent_ptr = persistent.sharing_info.ptr as usize;
        assert_ne!(fresh_ptr, persistent_ptr, "test setup: allocations must differ");

        let mut old = vec![NodeData::create_div()];
        old[0].set_dataset(OptionRefAny::Some(persistent));

        let mut new = vec![new0, new1];

        transfer_states(
            &mut old,
            &mut new,
            &[NodeMove {
                old_node_id: NodeId::new(0),
                new_node_id: NodeId::new(0),
            }],
        );

        // The merged dataset is the PERSISTENT allocation.
        assert_eq!(
            new[0].get_dataset().unwrap().sharing_info.ptr as usize,
            persistent_ptr,
            "the merge must keep the persistent allocation",
        );
        // The old node's dataset was moved into the merge result.
        assert!(old[0].get_dataset().is_none());

        // Both orphaned callback refanys — on the merge node AND on the sibling
        // — must now point at the merged allocation.
        for (i, nd) in new.iter().enumerate() {
            for cb in nd.callbacks.as_ref() {
                assert_eq!(
                    cb.refany.sharing_info.ptr as usize,
                    persistent_ptr,
                    "node {i}: an orphaned callback refany was not re-pointed",
                );
            }
        }
    }

    // ========================================================================
    // ChangeAccumulator
    // ========================================================================

    #[test]
    fn autotest_accumulator_new_is_empty_and_inert() {
        let a = ChangeAccumulator::new();
        assert!(a.is_empty());
        assert!(!a.needs_layout());
        assert!(!a.needs_paint_only());
        assert!(a.is_visually_unchanged());
        assert_eq!(a.max_scope, RelayoutScope::None);
        // `new()` and `default()` must agree.
        let d = ChangeAccumulator::default();
        assert_eq!(a.is_empty(), d.is_empty());
        assert_eq!(a.max_scope, d.max_scope);
    }

    #[test]
    fn autotest_accumulator_mount_forces_layout_unmount_does_not() {
        let mut a = ChangeAccumulator::new();
        a.add_mount(NodeId::new(0));
        assert!(!a.is_empty());
        assert!(a.needs_layout(), "a mounted node always needs layout");
        assert!(!a.needs_paint_only());
        assert!(!a.is_visually_unchanged());

        // An unmount alone is NOT layout work here (the node is gone); it only
        // breaks `is_visually_unchanged`. Pin the asymmetry.
        let mut a = ChangeAccumulator::new();
        a.add_unmount(NodeId::new(0));
        assert!(!a.is_empty());
        assert!(!a.needs_layout());
        assert!(!a.is_visually_unchanged());
    }

    #[test]
    fn autotest_accumulator_css_change_routes_paint_vs_layout_by_scope() {
        // scope == None -> paint bucket.
        let mut a = ChangeAccumulator::new();
        a.add_css_change(NodeId::new(0), CssPropertyType::TextColor, RelayoutScope::None);
        assert!(!a.needs_layout());
        assert!(a.needs_paint_only());
        assert!(!a.is_visually_unchanged());
        assert_eq!(a.max_scope, RelayoutScope::None);
        assert!(a.per_node[&NodeId::new(0)]
            .change_set
            .contains(NodeChangeSet::INLINE_STYLE_PAINT));

        // scope > None -> layout bucket.
        let mut a = ChangeAccumulator::new();
        a.add_css_change(NodeId::new(0), CssPropertyType::Width, RelayoutScope::SizingOnly);
        assert!(a.needs_layout());
        assert!(!a.needs_paint_only(), "layout work subsumes paint-only");
        assert_eq!(a.max_scope, RelayoutScope::SizingOnly);
        assert!(a.per_node[&NodeId::new(0)]
            .change_set
            .contains(NodeChangeSet::INLINE_STYLE_LAYOUT));
    }

    #[test]
    fn autotest_accumulator_max_scope_is_monotone() {
        // Once escalated, the scope must never be lowered by a later, weaker
        // change — otherwise a Full relayout gets silently downgraded.
        let mut a = ChangeAccumulator::new();
        a.add_css_change(NodeId::new(0), CssPropertyType::Display, RelayoutScope::Full);
        assert_eq!(a.max_scope, RelayoutScope::Full);

        a.add_css_change(NodeId::new(0), CssPropertyType::TextColor, RelayoutScope::None);
        assert_eq!(a.max_scope, RelayoutScope::Full, "max_scope must not regress");
        assert_eq!(
            a.per_node[&NodeId::new(0)].relayout_scope,
            RelayoutScope::Full,
            "per-node scope must not regress either",
        );

        a.add_css_change(NodeId::new(1), CssPropertyType::Width, RelayoutScope::SizingOnly);
        assert_eq!(a.max_scope, RelayoutScope::Full);
        assert_eq!(
            a.per_node[&NodeId::new(1)].relayout_scope,
            RelayoutScope::SizingOnly,
            "a different node keeps its own, lower scope",
        );
    }

    #[test]
    fn autotest_accumulator_text_change_is_ifc_scoped_and_unicode_safe() {
        for s in UNICODE_SAMPLES {
            let mut a = ChangeAccumulator::new();
            a.add_text_change(NodeId::new(0), String::new(), (*s).to_string());

            let report = &a.per_node[&NodeId::new(0)];
            assert!(report.change_set.contains(NodeChangeSet::TEXT_CONTENT));
            assert_eq!(report.relayout_scope, RelayoutScope::IfcOnly);
            assert_eq!(
                report.text_change,
                Some(TextChange {
                    old_text: String::new(),
                    new_text: (*s).to_string(),
                }),
            );
            assert!(a.needs_layout());
            assert_eq!(a.max_scope, RelayoutScope::IfcOnly);
        }
    }

    #[test]
    fn autotest_accumulator_add_dom_change_accumulates_and_never_clears_text() {
        let node = NodeId::new(0);
        let mut a = ChangeAccumulator::new();

        a.add_dom_change(
            node,
            NodeChangeSet {
                bits: NodeChangeSet::TEXT_CONTENT,
            },
            RelayoutScope::IfcOnly,
            Some(TextChange {
                old_text: "a".to_string(),
                new_text: "b".to_string(),
            }),
            vec![CssPropertyType::Width],
        );

        // A second call with text_change == None must NOT wipe the first one.
        a.add_dom_change(
            node,
            NodeChangeSet {
                bits: NodeChangeSet::STYLED_STATE,
            },
            RelayoutScope::None,
            None,
            vec![CssPropertyType::TextColor],
        );

        let report = &a.per_node[&node];
        assert!(report.change_set.contains(NodeChangeSet::TEXT_CONTENT));
        assert!(
            report.change_set.contains(NodeChangeSet::STYLED_STATE),
            "flags must be OR-accumulated across calls",
        );
        assert_eq!(report.relayout_scope, RelayoutScope::IfcOnly);
        assert!(
            report.text_change.is_some(),
            "a None text_change must not erase a previously recorded one",
        );
        assert_eq!(
            report.changed_css_properties,
            vec![CssPropertyType::Width, CssPropertyType::TextColor],
            "changed properties must be appended, not replaced",
        );
    }

    #[test]
    fn autotest_accumulator_image_change() {
        let mut a = ChangeAccumulator::new();
        a.add_image_change(NodeId::new(0), RelayoutScope::SizingOnly);
        assert!(a.per_node[&NodeId::new(0)]
            .change_set
            .contains(NodeChangeSet::IMAGE_CHANGED));
        assert!(a.needs_layout());
        assert_eq!(a.max_scope, RelayoutScope::SizingOnly);
    }

    #[test]
    fn autotest_accumulator_merge_empty_restyle_result_is_a_no_op() {
        let mut a = ChangeAccumulator::new();
        a.merge_restyle_result(&RestyleResult::default());
        assert!(a.is_empty());
        assert!(a.is_visually_unchanged());
    }

    #[test]
    fn autotest_accumulator_merge_restyle_result_classifies_by_property() {
        let prop = CssProperty::Width(CssPropertyValue::Exact(LayoutWidth::const_px(100)));
        let changed = ChangedCssProperty {
            previous_state: StyledNodeState::default(),
            previous_prop: prop.clone(),
            current_state: StyledNodeState::default(),
            current_prop: prop,
        };

        let mut restyle = RestyleResult::default();
        restyle
            .changed_nodes
            .insert(NodeId::new(3), vec![changed]);

        let mut a = ChangeAccumulator::new();
        a.merge_restyle_result(&restyle);

        // `width` -> SizingOnly -> layout bucket.
        assert!(!a.is_empty());
        assert!(a.needs_layout());
        assert_eq!(a.max_scope, RelayoutScope::SizingOnly);
        let report = &a.per_node[&NodeId::new(3)];
        assert!(report.change_set.contains(NodeChangeSet::INLINE_STYLE_LAYOUT));
        assert_eq!(report.changed_css_properties, vec![CssPropertyType::Width]);
    }

    #[test]
    fn autotest_accumulator_merge_extended_diff_counts_mounts_and_unmounts() {
        // No node_moves at all -> every new node mounted, every old node unmounted.
        let old_nd = vec![NodeData::create_div(), NodeData::create_div()];
        let new_nd = vec![
            NodeData::create_div(),
            NodeData::create_div(),
            NodeData::create_div(),
        ];

        let mut a = ChangeAccumulator::new();
        a.merge_extended_diff(&ExtendedDiffResult::default(), &old_nd, &new_nd);

        assert_eq!(a.mounted_nodes.len(), 3);
        assert_eq!(a.unmounted_nodes.len(), 2);
        assert!(!a.is_empty());
        assert!(a.needs_layout(), "mounted nodes always need layout");
        assert!(!a.is_visually_unchanged());
    }

    #[test]
    fn autotest_accumulator_merge_extended_diff_on_empty_doms_is_empty() {
        let mut a = ChangeAccumulator::new();
        a.merge_extended_diff(&ExtendedDiffResult::default(), &[], &[]);
        assert!(a.is_empty());
    }

    #[test]
    fn autotest_accumulator_merge_extended_diff_skips_empty_change_sets() {
        // A matched node with NO changes must not create a per_node entry.
        let old_nd = vec![NodeData::create_div()];
        let new_nd = vec![NodeData::create_div()];

        let extended = ExtendedDiffResult {
            diff: DiffResult {
                events: Vec::new(),
                node_moves: vec![NodeMove {
                    old_node_id: NodeId::new(0),
                    new_node_id: NodeId::new(0),
                }],
            },
            node_changes: vec![(NodeId::new(0), NodeId::new(0), NodeChangeSet::empty())],
        };

        let mut a = ChangeAccumulator::new();
        a.merge_extended_diff(&extended, &old_nd, &new_nd);

        assert!(a.per_node.is_empty(), "an empty change set must be skipped");
        assert!(a.mounted_nodes.is_empty());
        assert!(a.unmounted_nodes.is_empty());
        assert!(a.is_empty());
    }

    #[test]
    fn autotest_accumulator_merge_extended_diff_extracts_text_change() {
        let old_nd = vec![NodeData::create_text("héllo")];
        let new_nd = vec![NodeData::create_text("héllo wörld")];

        let extended = ExtendedDiffResult {
            diff: DiffResult {
                events: Vec::new(),
                node_moves: vec![NodeMove {
                    old_node_id: NodeId::new(0),
                    new_node_id: NodeId::new(0),
                }],
            },
            node_changes: vec![(
                NodeId::new(0),
                NodeId::new(0),
                NodeChangeSet {
                    bits: NodeChangeSet::TEXT_CONTENT,
                },
            )],
        };

        let mut a = ChangeAccumulator::new();
        a.merge_extended_diff(&extended, &old_nd, &new_nd);

        let report = &a.per_node[&NodeId::new(0)];
        assert_eq!(
            report.text_change,
            Some(TextChange {
                old_text: "héllo".to_string(),
                new_text: "héllo wörld".to_string(),
            }),
            "TEXT_CONTENT must carry the old/new text for cursor reconciliation",
        );
        assert_eq!(report.relayout_scope, RelayoutScope::IfcOnly);
    }

    // ========================================================================
    // ChangeAccumulator::classify_change_scope (private)
    // ========================================================================

    #[test]
    fn autotest_classify_scope_maps_each_flag_to_its_documented_scope() {
        let nodes = vec![NodeData::create_div()];
        let id = NodeId::new(0);

        let classify = |bits: u32| {
            ChangeAccumulator::classify_change_scope(NodeChangeSet { bits }, &nodes, id)
        };

        assert_eq!(classify(0), RelayoutScope::None, "empty -> no work");
        assert_eq!(classify(NodeChangeSet::NODE_TYPE_CHANGED), RelayoutScope::Full);
        assert_eq!(classify(NodeChangeSet::CHILDREN_CHANGED), RelayoutScope::Full);
        assert_eq!(classify(NodeChangeSet::IDS_AND_CLASSES), RelayoutScope::Full);
        assert_eq!(classify(NodeChangeSet::TEXT_CONTENT), RelayoutScope::IfcOnly);
        assert_eq!(classify(NodeChangeSet::IMAGE_CHANGED), RelayoutScope::SizingOnly);
        assert_eq!(classify(NodeChangeSet::CONTENTEDITABLE), RelayoutScope::SizingOnly);
        assert_eq!(classify(NodeChangeSet::STYLED_STATE), RelayoutScope::None);
        assert_eq!(classify(NodeChangeSet::INLINE_STYLE_PAINT), RelayoutScope::None);
        // Non-visual flags -> no work.
        assert_eq!(classify(NodeChangeSet::CALLBACKS), RelayoutScope::None);
        assert_eq!(classify(NodeChangeSet::DATASET), RelayoutScope::None);
        assert_eq!(classify(NodeChangeSet::TAB_INDEX), RelayoutScope::None);
    }

    #[test]
    fn autotest_classify_scope_precedence_is_widest_first() {
        let nodes = vec![NodeData::create_div()];
        let id = NodeId::new(0);

        // NODE_TYPE_CHANGED wins over everything below it.
        let bits = NodeChangeSet::NODE_TYPE_CHANGED
            | NodeChangeSet::TEXT_CONTENT
            | NodeChangeSet::IMAGE_CHANGED
            | NodeChangeSet::STYLED_STATE;
        assert_eq!(
            ChangeAccumulator::classify_change_scope(NodeChangeSet { bits }, &nodes, id),
            RelayoutScope::Full,
        );

        // TEXT_CONTENT (IfcOnly) wins over IMAGE_CHANGED (SizingOnly) — pinning
        // the documented order, even though IfcOnly < SizingOnly.
        let bits = NodeChangeSet::TEXT_CONTENT | NodeChangeSet::IMAGE_CHANGED;
        assert_eq!(
            ChangeAccumulator::classify_change_scope(NodeChangeSet { bits }, &nodes, id),
            RelayoutScope::IfcOnly,
        );
    }

    #[test]
    fn autotest_classify_scope_inline_layout_walks_the_nodes_own_css() {
        // With a sizing property on the node, the scope comes from the property.
        let nodes = vec![NodeData::create_div().with_css("width: 100px")];
        assert_eq!(
            ChangeAccumulator::classify_change_scope(
                NodeChangeSet {
                    bits: NodeChangeSet::INLINE_STYLE_LAYOUT,
                },
                &nodes,
                NodeId::new(0),
            ),
            RelayoutScope::SizingOnly,
        );

        // A `display` change is a full relayout.
        let nodes = vec![NodeData::create_div().with_css("display: flex")];
        assert_eq!(
            ChangeAccumulator::classify_change_scope(
                NodeChangeSet {
                    bits: NodeChangeSet::INLINE_STYLE_LAYOUT,
                },
                &nodes,
                NodeId::new(0),
            ),
            RelayoutScope::Full,
        );

        // No inline CSS at all (the property was REMOVED, so the new node has
        // nothing to walk): the conservative SizingOnly fallback must kick in
        // rather than silently reporting "no layout work".
        let nodes = vec![NodeData::create_div()];
        assert_eq!(
            ChangeAccumulator::classify_change_scope(
                NodeChangeSet {
                    bits: NodeChangeSet::INLINE_STYLE_LAYOUT,
                },
                &nodes,
                NodeId::new(0),
            ),
            RelayoutScope::SizingOnly,
            "an INLINE_STYLE_LAYOUT change must never classify as 'no layout'",
        );
    }

    // ========================================================================
    // reconcile_dom_with_changes
    // ========================================================================

    #[test]
    fn autotest_reconcile_with_changes_on_empty_doms() {
        let r = reconcile_dom_with_changes(
            &[],
            &[],
            &[],
            &[],
            None,
            None,
            &no_layout(),
            &no_layout(),
            DomId::ROOT_ID,
            Instant::now(),
        );
        assert!(r.diff.events.is_empty());
        assert!(r.diff.node_moves.is_empty());
        assert!(r.node_changes.is_empty());
    }

    #[test]
    fn autotest_reconcile_with_changes_reports_one_entry_per_move() {
        let old = vec![NodeData::create_text("v1").with_key(1u32)];
        let new = vec![NodeData::create_text("v2").with_key(1u32)];

        let r = reconcile_dom_with_changes(
            &old,
            &new,
            &[],
            &[],
            None,
            None,
            &no_layout(),
            &no_layout(),
            DomId::ROOT_ID,
            Instant::now(),
        );

        assert_eq!(r.diff.node_moves.len(), 1);
        assert_eq!(
            r.node_changes.len(),
            r.diff.node_moves.len(),
            "there must be exactly one change entry per matched pair",
        );

        let (old_id, new_id, changes) = &r.node_changes[0];
        assert_eq!(*old_id, NodeId::new(0));
        assert_eq!(*new_id, NodeId::new(0));
        assert!(changes.contains(NodeChangeSet::TEXT_CONTENT));
    }

    #[test]
    fn autotest_reconcile_with_changes_tolerates_short_styled_state_slices() {
        // `old_styled_nodes` / `new_styled_nodes` are indexed with `.get()`, so a
        // slice shorter than the DOM must degrade to `None`, not panic.
        let old = vec![NodeData::create_div(), NodeData::create_div()];
        let new = vec![NodeData::create_div(), NodeData::create_div()];
        let short = [StyledNodeState::default()]; // 1 entry for 2 nodes

        let r = reconcile_dom_with_changes(
            &old,
            &new,
            &[],
            &[],
            Some(&short[..]),
            Some(&short[..]),
            &no_layout(),
            &no_layout(),
            DomId::ROOT_ID,
            Instant::now(),
        );
        assert_eq!(r.node_changes.len(), 2);
        // Both sides see the same (present-or-absent) state, so no STYLED_STATE.
        for (_, _, changes) in &r.node_changes {
            assert!(!changes.contains(NodeChangeSet::STYLED_STATE));
        }
    }

    #[test]
    fn autotest_reconcile_with_changes_feeds_the_accumulator() {
        // End-to-end: reconcile -> ExtendedDiffResult -> ChangeAccumulator.
        let old = vec![NodeData::create_text("before").with_key(1u32)];
        let new = vec![NodeData::create_text("after").with_key(1u32)];

        let extended = reconcile_dom_with_changes(
            &old,
            &new,
            &[],
            &[],
            None,
            None,
            &no_layout(),
            &no_layout(),
            DomId::ROOT_ID,
            Instant::now(),
        );

        let mut acc = ChangeAccumulator::new();
        acc.merge_extended_diff(&extended, &old, &new);

        assert!(!acc.is_empty());
        assert!(acc.needs_layout(), "a text edit needs (IFC) layout");
        assert!(!acc.is_visually_unchanged());
        assert!(acc.mounted_nodes.is_empty());
        assert!(acc.unmounted_nodes.is_empty());

        let report = &acc.per_node[&NodeId::new(0)];
        assert_eq!(report.relayout_scope, RelayoutScope::IfcOnly);
        assert_eq!(
            report.text_change,
            Some(TextChange {
                old_text: "before".to_string(),
                new_text: "after".to_string(),
            }),
        );
    }

    // ========================================================================
    // NodeDataFingerprint
    // ========================================================================

    #[test]
    fn autotest_fingerprint_default_and_self_comparison_are_inert() {
        let d = NodeDataFingerprint::default();
        assert!(d.is_identical(&d));
        assert!(d.diff(&d).is_empty());
        assert!(!d.might_affect_layout(&d));
        assert!(!d.might_affect_visuals(&d));
        assert_eq!(d, NodeDataFingerprint::default());
    }

    #[test]
    fn autotest_fingerprint_is_a_pure_function_of_its_inputs() {
        // Round-trip / determinism: recomputing from equal inputs must give an
        // identical fingerprint (no address or allocation identity leaking in).
        let state = StyledNodeState::default();
        for s in UNICODE_SAMPLES {
            let a = NodeDataFingerprint::compute(&NodeData::create_text(*s), Some(&state));
            let b = NodeDataFingerprint::compute(&NodeData::create_text(*s), Some(&state));
            assert_eq!(a, b, "fingerprint of {s:?} is not deterministic");
            assert!(a.is_identical(&b));
            assert!(a.diff(&b).is_empty());
        }
    }

    #[test]
    fn autotest_fingerprint_diff_is_symmetric() {
        let a = NodeDataFingerprint::compute(&NodeData::create_text("a"), None);
        let b = NodeDataFingerprint::compute(&class_node("x").with_css("width: 1px"), None);

        assert_eq!(a.diff(&b), b.diff(&a), "diff must be symmetric");
        assert_eq!(
            a.might_affect_layout(&b),
            b.might_affect_layout(&a),
            "might_affect_layout must be symmetric",
        );
        assert_eq!(a.might_affect_visuals(&b), b.might_affect_visuals(&a));
    }

    #[test]
    fn autotest_fingerprint_text_change_is_layout_and_visual() {
        let a = NodeDataFingerprint::compute(&NodeData::create_text("one"), None);
        let b = NodeDataFingerprint::compute(&NodeData::create_text("two"), None);

        assert!(!a.is_identical(&b));
        let changes = a.diff(&b);
        // Conservative by design: content_hash cannot tell text from image.
        assert!(changes.contains(NodeChangeSet::TEXT_CONTENT));
        assert!(changes.contains(NodeChangeSet::IMAGE_CHANGED));
        assert!(a.might_affect_layout(&b));
        assert!(a.might_affect_visuals(&b));
    }

    #[test]
    fn autotest_fingerprint_styled_state_is_visual_but_not_layout() {
        // The sharpest invariant of the fast path: a :hover flip must never be
        // able to trigger relayout.
        let node = NodeData::create_div();
        let calm = StyledNodeState::default();
        let hovered = StyledNodeState {
            hover: true,
            ..StyledNodeState::default()
        };

        let a = NodeDataFingerprint::compute(&node, Some(&calm));
        let b = NodeDataFingerprint::compute(&node, Some(&hovered));

        assert!(!a.is_identical(&b));
        assert!(a.diff(&b).contains(NodeChangeSet::STYLED_STATE));
        assert!(
            !a.might_affect_layout(&b),
            "a styled-state change must not be able to request layout",
        );
        assert!(a.might_affect_visuals(&b));
    }

    #[test]
    fn autotest_fingerprint_callback_change_is_neither_layout_nor_visual() {
        let plain = NodeData::create_div();
        let with_handler = with_cb(NodeData::create_div(), ComponentEventFilter::AfterMount);

        let a = NodeDataFingerprint::compute(&plain, None);
        let b = NodeDataFingerprint::compute(&with_handler, None);

        assert!(!a.is_identical(&b), "the callback list must be fingerprinted");
        assert!(a.diff(&b).contains(NodeChangeSet::CALLBACKS));
        assert!(
            !a.might_affect_layout(&b),
            "swapping an event handler must not trigger relayout",
        );
        assert!(
            !a.might_affect_visuals(&b),
            "swapping an event handler must not trigger a repaint",
        );
    }

    #[test]
    fn autotest_fingerprint_ids_classes_and_inline_css_are_layout_relevant() {
        let base = NodeDataFingerprint::compute(&NodeData::create_div(), None);

        let classes = NodeDataFingerprint::compute(&class_node("banner"), None);
        assert!(base.diff(&classes).contains(NodeChangeSet::IDS_AND_CLASSES));
        assert!(base.might_affect_layout(&classes));
        assert!(base.might_affect_visuals(&classes));

        let styled =
            NodeDataFingerprint::compute(&NodeData::create_div().with_css("width: 3px"), None);
        assert!(base.diff(&styled).contains(NodeChangeSet::INLINE_STYLE_LAYOUT));
        assert!(base.might_affect_layout(&styled));
        assert!(base.might_affect_visuals(&styled));
    }

    #[test]
    fn autotest_fingerprint_attrs_change_flags_tab_index_and_contenteditable() {
        let base = NodeDataFingerprint::compute(&NodeData::create_div(), None);
        let editable =
            NodeDataFingerprint::compute(&NodeData::create_div().with_contenteditable(true), None);

        let changes = base.diff(&editable);
        assert!(changes.contains(NodeChangeSet::TAB_INDEX));
        assert!(changes.contains(NodeChangeSet::CONTENTEDITABLE));
        assert!(
            base.might_affect_layout(&editable),
            "attrs_hash feeds might_affect_layout",
        );
        assert!(
            !base.might_affect_visuals(&editable),
            "attrs_hash is deliberately NOT part of might_affect_visuals",
        );
    }

    #[test]
    fn autotest_fingerprint_agrees_with_compute_node_changes_on_unchanged_nodes() {
        // Tier 1 (fingerprint) must never claim "changed" where Tier 2
        // (compute_node_changes) says "unchanged" — that would defeat the whole
        // two-tier fast path.
        let state = StyledNodeState::default();
        let samples = vec![
            NodeData::create_div(),
            NodeData::create_text("hello 🌍"),
            class_node("row"),
            id_node("main"),
            NodeData::create_div().with_css("color: red"),
            NodeData::create_div().with_contenteditable(true),
            with_cb(NodeData::create_div(), ComponentEventFilter::AfterMount),
        ];

        for node in &samples {
            let clone = node.clone();

            let fp_a = NodeDataFingerprint::compute(node, Some(&state));
            let fp_b = NodeDataFingerprint::compute(&clone, Some(&state));
            assert!(
                fp_a.is_identical(&fp_b),
                "a cloned node must fingerprint identically",
            );
            assert!(fp_a.diff(&fp_b).is_empty());

            let tier2 = compute_node_changes(node, &clone, Some(&state), Some(&state));
            assert!(
                tier2.is_empty(),
                "compute_node_changes must agree that a clone is unchanged, got {:#b}",
                tier2.bits,
            );
        }
    }
}
