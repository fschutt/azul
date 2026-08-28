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

use alloc::{
    collections::BTreeMap,
    collections::VecDeque,
    string::{String, ToString},
    vec::Vec,
};
use core::hash::Hash;

use azul_css::props::property::{CssPropertyType, RelayoutScope};

use crate::{
    dom::{DomId, DomNodeHash, DomNodeId, IdOrClass, NodeData, NodeType},
    events::{
        ComponentEventFilter, EventData, EventFilter, EventPhase, EventSource, EventType,
        LifecycleEventData, LifecycleReason, SyntheticEvent,
    },
    geom::LogicalRect,
    id::NodeId,
    refany::RefAny,
    styled_dom::{
        ChangedCssProperty, NodeHierarchyItem, NodeHierarchyItemId, RestyleResult, StyledNodeState,
    },
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
    pub const NODE_TYPE_CHANGED: u32 = 0b0000_0000_0000_0001;
    /// Text content changed (for Text nodes).
    pub const TEXT_CONTENT: u32 = 0b0000_0000_0000_0010;
    /// CSS IDs or classes changed (may cause restyle → relayout).
    pub const IDS_AND_CLASSES: u32 = 0b0000_0000_0000_0100;
    /// Inline CSS properties changed that affect layout.
    pub const INLINE_STYLE_LAYOUT: u32 = 0b0000_0000_0000_1000;
    /// Children added, removed, or reordered.
    pub const CHILDREN_CHANGED: u32 = 0b0000_0000_0001_0000;
    /// Image source changed (may affect intrinsic size).
    pub const IMAGE_CHANGED: u32 = 0b0000_0000_0010_0000;
    /// Contenteditable flag changed.
    pub const CONTENTEDITABLE: u32 = 0b0000_0000_0100_0000;
    /// Tab index changed.
    pub const TAB_INDEX: u32 = 0b0000_0000_1000_0000;

    // --- Changes that affect PAINT only (no relayout needed) ---

    /// Inline CSS properties changed that affect paint only.
    pub const INLINE_STYLE_PAINT: u32 = 0b0000_0001_0000_0000;
    /// Styled node state changed (hover, active, focus, etc.).
    pub const STYLED_STATE: u32 = 0b0000_0010_0000_0000;

    // --- Changes that affect NEITHER layout nor paint ---

    /// Callbacks changed (new `RefAny`, different event handlers).
    pub const CALLBACKS: u32 = 0b0000_0100_0000_0000;
    /// Dataset changed.
    pub const DATASET: u32 = 0b0000_1000_0000_0000;
    /// Accessibility info changed.
    pub const ACCESSIBILITY: u32 = 0b0001_0000_0000_0000;

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
    pub const AFFECTS_PAINT: u32 = Self::INLINE_STYLE_PAINT | Self::STYLED_STATE;

    #[must_use]
    pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.bits == 0
    }

    #[must_use]
    pub const fn contains(&self, flag: u32) -> bool {
        (self.bits & flag) == flag
    }

    #[must_use]
    pub const fn intersects(&self, mask: u32) -> bool {
        (self.bits & mask) != 0
    }

    pub const fn insert(&mut self, flag: u32) {
        self.bits |= flag;
    }

    /// Returns true if no visual change occurred (only callbacks/dataset/a11y).
    #[must_use]
    pub const fn is_visually_unchanged(&self) -> bool {
        !self.intersects(Self::AFFECTS_LAYOUT) && !self.intersects(Self::AFFECTS_PAINT)
    }

    /// Returns true if layout is needed.
    #[must_use]
    pub const fn needs_layout(&self) -> bool {
        self.intersects(Self::AFFECTS_LAYOUT)
    }

    /// Returns true if paint is needed (but not necessarily layout).
    #[must_use]
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
        Self {
            bits: self.bits | rhs.bits,
        }
    }
}

/// Extended diff result that includes per-node change information.
#[derive(Debug, Clone, Default)]
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
#[must_use]
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
        let old_ids_classes: Vec<_> = old_node
            .attributes()
            .as_ref()
            .iter()
            .filter(|a| matches!(a, AttributeType::Id(_) | AttributeType::Class(_)))
            .collect();
        let new_ids_classes: Vec<_> = new_node
            .attributes()
            .as_ref()
            .iter()
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
#[must_use]
pub fn calculate_reconciliation_key(
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
        match hierarchy
            .get(cur.index())
            .and_then(NodeHierarchyItem::parent_id)
        {
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

        if let Some(parent_id) = hierarchy
            .get(nid.index())
            .and_then(NodeHierarchyItem::parent_id)
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
#[must_use]
pub fn precompute_reconciliation_keys(
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
#[derive(Debug, Clone, Default)]
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
#[must_use]
pub fn reconcile_dom(
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
    fn pop_first_unconsumed(queue: &mut VecDeque<NodeId>, consumed: &[bool]) -> Option<NodeId> {
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
        old_by_rec_key
            .entry(old_rec_keys[idx])
            .or_default()
            .push_back(id);

        let hash = node.calculate_node_data_hash();
        old_hashed.entry(hash).or_default().push_back(id);

        let structural_hash = node.calculate_structural_hash();
        old_structural
            .entry(structural_hash)
            .or_default()
            .push_back(id);
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
                    !old_nodes_consumed[old_id.index()] && old_parent_key(old_id) == new_parent_key
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
            let old_rect = old_layout
                .get(&old_id)
                .copied()
                .unwrap_or(LogicalRect::zero());
            let new_rect = new_layout
                .get(&new_id)
                .copied()
                .unwrap_or(LogicalRect::zero());

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
                let bounds = new_layout
                    .get(&new_id)
                    .copied()
                    .unwrap_or(LogicalRect::zero());
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
                let bounds = old_layout
                    .get(&old_id)
                    .copied()
                    .unwrap_or(LogicalRect::zero());
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
        at_target_only: false,
    }
}

/// The event a `<transient-window>` receives when the USER closed it — an
/// outside click, or Escape — as opposed to the app flipping `open`.
///
/// Built here, next to the other lifecycle events, so it carries the same
/// `EventSource::Lifecycle` / `EventPhase::Target` shape the dispatcher
/// expects for a `ComponentEventFilter`. `bounds` is the popup's anchor
/// rect in the parent, the closest thing to "where it was".
#[must_use]
pub fn create_dismiss_event(
    node_id: NodeId,
    dom_id: DomId,
    timestamp: &Instant,
    bounds: LogicalRect,
) -> SyntheticEvent {
    create_lifecycle_event(
        EventType::Dismiss,
        node_id,
        dom_id,
        timestamp,
        LifecycleEventData {
            reason: LifecycleReason::Dismiss,
            previous_bounds: None,
            current_bounds: bounds,
        },
    )
}

/// The lifecycle event a `<transient-window>` gets on a tear-off or a dock.
///
/// `torn == true`: torn off its anchor (`bounds` = the toplevel's rect in the
/// parent). `torn == false`: docked back (`bounds` = the anchor it docked onto).
#[must_use]
pub fn create_tearoff_event(
    node_id: NodeId,
    dom_id: DomId,
    timestamp: &Instant,
    torn: bool,
    bounds: LogicalRect,
) -> SyntheticEvent {
    let (ty, reason) = if torn {
        (EventType::TearOff, LifecycleReason::TearOff)
    } else {
        (EventType::Dock, LifecycleReason::Dock)
    };
    create_lifecycle_event(
        ty,
        node_id,
        dom_id,
        timestamp,
        LifecycleEventData {
            reason,
            previous_bounds: None,
            current_bounds: bounds,
        },
    )
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
#[must_use]
pub fn create_migration_map(node_moves: &[NodeMove]) -> OrderedMap<NodeId, NodeId> {
    let mut map = OrderedMap::default();
    for m in node_moves {
        map.insert(m.old_node_id, m.new_node_id);
    }
    map
}

/// Suppression tag for the image-churn lint, honored from `AZ_SUPPRESS`.
pub const IMAGE_CHURN_SUPPRESS_TAG: &str = "image_churn";

/// Re-initialisations per second above which an image node is churning.
///
/// A widget legitimately rebuilds its image node with a placeholder now and
/// then — the first build after mount has no frame yet. Doing it dozens of
/// times a second means a LIVE image is being discarded and re-awaited on every
/// frame, which is a bug in how the node is built, not in the content.
const IMAGE_CHURN_PER_SEC: u32 = 10;

/// The framework notices, by itself, when an image node re-initialises at frame
/// rate — and says what is almost always wrong.
///
/// The symptom is a video or capture node that flickers: it holds a real frame,
/// the DOM rebuilds, the fresh node carries only a placeholder, and the live
/// image is thrown away until the next frame arrives 16-33ms later. On a
/// resizing window, which rebuilds continuously, that is a continuous flash.
///
/// The cause is almost always a missing DATASET + merge callback. Without one
/// the reconciler cannot tell that the rebuilt node is the same widget, so it
/// has nothing to carry forward — see `transfer_states`, which does carry the
/// previous frame when a merge callback exists.
///
/// Detection lives HERE, in the reconciler, because neither DOM shows it alone:
/// the old build has a frame, the new build has a placeholder, and only the
/// pair reveals the churn. No user code has to opt in.
/// Per-node churn bookkeeping: `(count, window_start, warned)`.
///
/// Shared with the tests so the detector can be asserted on directly, instead
/// of only through a message on stderr that nothing can observe.
#[cfg(feature = "std")]
type ImageChurnMap = BTreeMap<usize, (u32, std::time::Instant, bool)>;

#[cfg(feature = "std")]
fn image_churn_state() -> &'static std::sync::Mutex<ImageChurnMap> {
    use std::sync::{Mutex, OnceLock};
    static CHURN: OnceLock<Mutex<ImageChurnMap>> = OnceLock::new();
    CHURN.get_or_init(|| Mutex::new(ImageChurnMap::new()))
}

/// How many times this node has re-initialised inside the current window.
#[cfg(all(feature = "std", test))]
pub(crate) fn image_churn_count(node_index: usize) -> u32 {
    image_churn_state()
        .lock()
        .ok()
        .and_then(|m| m.get(&node_index).map(|e| e.0))
        .unwrap_or(0)
}

#[cfg(feature = "std")]
fn note_image_reinitialised(node_index: usize, carried: bool) {
    use std::{
        collections::BTreeMap,
        sync::{Mutex, OnceLock},
        time::Instant,
    };

    static SUPPRESSED: OnceLock<bool> = OnceLock::new();
    if *SUPPRESSED.get_or_init(|| {
        let v = std::env::var("AZ_SUPPRESS")
            .or_else(|_| std::env::var("AZ_SUPRESS"))
            .unwrap_or_default();
        v.split(',')
            .any(|t| t.trim().eq_ignore_ascii_case(IMAGE_CHURN_SUPPRESS_TAG))
    }) {
        return;
    }

    // Per node: how many times it re-initialised, when that window started, and
    // whether we have already said so. "The time it was last updated and how
    // much" is the whole state — no history, no allocation per event.
    let churn = image_churn_state();
    let Ok(mut map) = churn.lock() else {
        return; // a poisoned lint counter must never take the app down
    };

    let now = Instant::now();
    let entry = map.entry(node_index).or_insert((0, now, false));
    if now.duration_since(entry.1).as_secs_f32() >= 1.0 {
        *entry = (1, now, entry.2);
        return;
    }
    entry.0 += 1;

    // Warn once per node. This runs on every rebuild of a resizing window; a
    // warning per frame would bury the message it is trying to deliver.
    if entry.0 < IMAGE_CHURN_PER_SEC || entry.2 {
        return;
    }
    entry.2 = true;
    let rate = entry.0;

    if carried {
        crate::diagnostics::emit(format!(
            "[azul][image-churn] node {node_index} rebuilt its image as a \
             PLACEHOLDER {rate}x in one second. The previous frame was carried \
             forward each time, so nothing flickers — but a live image node is \
             being reconstructed every frame. If this is not a capture widget, \
             build the node once and update it through the image cache. \
             (suppress with AZ_SUPPRESS={IMAGE_CHURN_SUPPRESS_TAG})"
        ));
    } else {
        crate::diagnostics::emit(format!(
            "[azul][image-churn] node {node_index} rebuilt its image as a \
             PLACEHOLDER {rate}x in one second and the previous frame could NOT \
             be carried forward: this node has NO DATASET + merge callback, so \
             the reconciler cannot tell the rebuilt node is the same widget. The \
             live image is discarded every frame and the node falls back to its \
             placeholder until the next one arrives — a continuous flicker. If \
             this is a video or camera node, it is almost certainly missing its \
             dataset: attach one with a DatasetMergeCallback (see MapWidget / \
             ScreenCaptureWidget). \
             (suppress with AZ_SUPPRESS={IMAGE_CHURN_SUPPRESS_TAG})"
        ));
    }
}

#[cfg(not(feature = "std"))]
fn note_image_reinitialised(_node_index: usize, _carried: bool) {}

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
            // No merge callback — nothing can be carried forward. If this node
            // is an image that just reverted to a placeholder while the old
            // build held a real frame, that live frame is being DISCARDED, and
            // at frame rate it is a visible flicker. This is the "forgot the
            // dataset on a video node" case, and the framework can see it
            // without anyone asking.
            if new_node_data[new_idx].image_is_placeholder()
                && !old_node_data[old_idx].image_is_placeholder()
            {
                note_image_reinitialised(new_idx, false);
            }
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

                // 3b. CARRY THE LIVE IMAGE FORWARD.
                //
                // A merge callback ran, so this is the SAME logical widget as
                // before — the reconciler matched them and the widget asked for
                // its state to persist. A capture widget rebuilds its node with
                // a PLACEHOLDER every time (`Dom::create_image(null_image)`),
                // because the fresh widget struct has no frame yet; the live
                // frame arrives later by writeback. So on every DOM rebuild the
                // node reverted to the placeholder and stayed there until the
                // next frame landed ~16-33ms later.
                //
                // That is the flash reported when resizing a window while
                // screensharing: "the screen flickers, like it is
                // re-initializing". Nothing was re-initialising — the last frame
                // was simply thrown away and re-awaited.
                //
                // NARROW ON PURPOSE: only when the NEW image is a null/
                // placeholder image and the OLD one is not. An app that
                // deliberately swaps in a real image still wins, and one that
                // deliberately clears to a placeholder is the only case this
                // changes — which is indistinguishable from "has not produced a
                // frame yet" and is what the widget itself does every rebuild.
                if new_node_data[new_idx].image_is_placeholder()
                    && !old_node_data[old_idx].image_is_placeholder()
                {
                    if let Some(prev) = old_node_data[old_idx].get_image_ref_cloned() {
                        new_node_data[new_idx].set_image_ref(prev);
                    }
                    // Handled — but still worth saying if it happens every
                    // frame, because rebuilding a live image node at 60 Hz is
                    // work nobody asked for.
                    note_image_reinitialised(new_idx, true);
                }

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
                repoint_orphaned_refanys(new_node_data, orphan_alloc, &merged);
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

/// Re-point every `RefAny` across `node_data` that is a clone of the
/// allocation `orphan_alloc` (a dataset the merge discarded) at `merged`.
///
/// The whole widget then reads ONE state: `VirtualView` content refanys,
/// event callback refanys and datasets cloned from the same source. The
/// `MapWidget` puts its `VirtualView` in a CHILD and its pan/zoom callbacks
/// on the parent, which is why this scans the whole arena and not just the
/// merge node.
fn repoint_orphaned_refanys(node_data: &mut [NodeData], orphan_alloc: usize, merged: &RefAny) {
    use crate::refany::OptionRefAny;
    if merged.sharing_info.ptr as usize == orphan_alloc {
        return; // the merge kept the fresh allocation: nothing is orphaned
    }
    for nd in node_data.iter_mut() {
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

/// The pre-cascade fast path's half of [`transfer_states`].
///
/// When the fresh build's fingerprints equal the retained DOM's, the cascade
/// is skipped and the retained `StyledDom` is kept; the fresh build's event
/// callbacks are installed on it (they may reference new app state). That
/// left the DATASETS behind: the fresh callbacks' `RefAny`s were clones of
/// the fresh build's dataset, the retained node kept last frame's, and no
/// merge callback ever ran — so a `RefreshDom` that rebuilt an identical DOM
/// reset every stateful widget's callback state (a slider's drag died on its
/// second move) and split the widget across two allocations, the exact
/// fragmentation [`repoint_orphaned_refanys`] exists to prevent.
///
/// Same rules as `transfer_states`, with the retained node as "old" and the
/// fresh dataset as "new": merge through the node's merge callback when it
/// has one, otherwise the fresh dataset wins; then re-point everything on the
/// retained DOM that was a clone of the fresh dataset at the result. Call it
/// AFTER the fresh callbacks have been installed on `node_data`, once per
/// fresh dataset, with `idx` the node's flattened index.
pub fn merge_fresh_dataset(node_data: &mut [NodeData], idx: usize, fresh: RefAny) {
    use crate::refany::OptionRefAny;
    let Some(nd) = node_data.get_mut(idx) else {
        return;
    };
    let orphan_alloc = fresh.sharing_info.ptr as usize;
    let merge_callback = nd.get_merge_callback();
    let retained = nd.take_dataset();
    let result = match (merge_callback, retained) {
        (Some(cb), Some(old)) => (cb.cb)(fresh, old),
        _ => fresh,
    };
    nd.set_dataset(OptionRefAny::Some(result.clone()));
    repoint_orphaned_refanys(node_data, orphan_alloc, &result);
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
#[must_use]
pub fn calculate_contenteditable_key(
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
        match hierarchy
            .get(cur.index())
            .and_then(NodeHierarchyItem::parent_id)
        {
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

        let node_parent = hierarchy
            .get(nid.index())
            .and_then(NodeHierarchyItem::parent_id);

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
#[must_use]
pub fn reconcile_cursor_position(old_text: &str, new_text: &str, old_cursor_byte: usize) -> usize {
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
#[must_use]
pub fn get_node_text_content(node: &NodeData) -> Option<&str> {
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
    #[must_use]
    pub fn needs_layout(&self) -> bool {
        self.change_set.needs_layout() || self.relayout_scope > RelayoutScope::None
    }

    #[must_use]
    pub const fn needs_paint(&self) -> bool {
        self.change_set.needs_paint()
    }

    #[must_use]
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
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if no changes were detected at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.per_node.is_empty() && self.mounted_nodes.is_empty() && self.unmounted_nodes.is_empty()
    }

    /// Returns true if layout work is needed (any node has scope > None).
    #[must_use]
    pub fn needs_layout(&self) -> bool {
        self.max_scope > RelayoutScope::None
            || !self.mounted_nodes.is_empty()
            || self.per_node.values().any(NodeChangeReport::needs_layout)
    }

    /// Returns true if only paint work is needed (no layout).
    #[must_use]
    pub fn needs_paint_only(&self) -> bool {
        !self.needs_layout() && self.per_node.values().any(NodeChangeReport::needs_paint)
    }

    /// Returns true if only non-visual changes occurred (callbacks, dataset, a11y).
    #[must_use]
    pub fn is_visually_unchanged(&self) -> bool {
        self.mounted_nodes.is_empty()
            && self.unmounted_nodes.is_empty()
            && self.max_scope == RelayoutScope::None
            && self
                .per_node
                .values()
                .all(NodeChangeReport::is_visually_unchanged)
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
    pub fn add_text_change(&mut self, node_id: NodeId, old_text: String, new_text: String) {
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
    pub fn add_image_change(&mut self, node_id: NodeId, scope: RelayoutScope) {
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
#[must_use]
pub fn reconcile_dom_with_changes(
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
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
    /// Hash of the layout-relevant attributes (contenteditable, flags)
    pub attrs_hash: u64,
    /// Hash of the dataset's PRESENCE and TYPE — never its allocation.
    ///
    /// A widget's dataset is state, not layout. Most widgets allocate a fresh
    /// `RefAny` on every build, and `RefAny` hashes by pointer, so hashing the
    /// dataset into `attrs_hash` (which maps to `CONTENTEDITABLE`, a layout
    /// change) made every `with_dataset` node LAYOUT-DIRTY on every
    /// `RefreshDom`: the `TextArea` became a standalone layout root on each
    /// callback, was re-laid-out with its `min-height` while its flex
    /// container kept the old slot, and painted 64 px into a 36 px slot — over
    /// the slider beneath it. A dataset change is [`NodeChangeSet::DATASET`],
    /// which affects neither layout nor paint.
    pub dataset_hash: u64,
}

impl NodeDataFingerprint {
    /// Compute a fingerprint from a node's data and styled state.
    #[must_use]
    pub fn compute(node: &NodeData, styled_state: Option<&StyledNodeState>) -> Self {
        use core::hash::Hash;
        use core::hash::Hasher;

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

        // Attributes hash — the layout-relevant ones only
        let attrs_hash = {
            let mut h = crate::hash::DefaultHasher::new();
            node.is_contenteditable().hash(&mut h);
            node.flags.hash(&mut h);
            h.finish()
        };

        // Dataset hash: presence + type, NOT the allocation (see the field doc).
        let dataset_hash = {
            let mut h = crate::hash::DefaultHasher::new();
            match node.get_dataset() {
                Some(ds) => {
                    true.hash(&mut h);
                    ds.get_type_id().hash(&mut h);
                }
                None => false.hash(&mut h),
            }
            h.finish()
        };

        Self {
            content_hash,
            state_hash,
            inline_css_hash,
            ids_classes_hash,
            callbacks_hash,
            attrs_hash,
            dataset_hash,
        }
    }

    /// Returns a quick `NodeChangeSet` by comparing two fingerprints.
    /// This is O(1) — just comparing 7 u64s.
    ///
    /// The result is *conservative*: if a field hash differs, we set the
    /// broadest applicable flag. For precise classification (e.g., which
    /// CSS properties changed and their `relayout_scope()`), the caller
    /// should fall back to `compute_node_changes()` for changed nodes.
    #[must_use]
    pub const fn diff(&self, other: &Self) -> NodeChangeSet {
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

        if self.dataset_hash != other.dataset_hash {
            changes.insert(NodeChangeSet::DATASET);
        }

        changes
    }

    /// Returns true if the fingerprint is identical (no changes at all).
    #[must_use]
    pub fn is_identical(&self, other: &Self) -> bool {
        self == other
    }

    /// Quick check: could this change affect layout?
    #[must_use]
    pub const fn might_affect_layout(&self, other: &Self) -> bool {
        self.content_hash != other.content_hash
            || self.inline_css_hash != other.inline_css_hash
            || self.ids_classes_hash != other.ids_classes_hash
            || self.attrs_hash != other.attrs_hash
    }

    /// Quick check: could this change affect visuals at all?
    #[must_use]
    pub const fn might_affect_visuals(&self, other: &Self) -> bool {
        self.content_hash != other.content_hash
            || self.state_hash != other.state_hash
            || self.inline_css_hash != other.inline_css_hash
            || self.ids_classes_hash != other.ids_classes_hash
    }
}

// ============================================================================
// Pre-cascade DOM fingerprints (two tiers: STRUCTURE vs STYLE)
// ============================================================================

/// Two-tier fingerprints of a recursive [`crate::dom::Dom`].
///
/// Computed BEFORE the cascade, in the same pre-order the flattener
/// (`convert_dom_into_compact_dom`) assigns `NodeId`s — index `i` in each Vec
/// is flattened `NodeId(i)`.
///
/// WHY TWO TIERS (user directive 2026-08-08): "the start should just scan
/// over the `NodeHierarchy` to discover anything that changed, which is
/// iterating over a minimal array" — and css must be EXCLUDED from that
/// first equivalence, because a stylesheet can only affect the subtree it
/// is attached to:
///
/// - **structure**: hierarchy shape + node content (`node_type`, ids/classes,
///   attributes, callback EVENT types). NO css of any kind. If this tier is
///   equal, the old tree, its shaped text and its intrinsic caches are all
///   reusable — and if the style tier is ALSO equal, the previous CASCADE
///   is reusable wholesale (skip `create_from_dom` entirely).
/// - **style**: per-node inline css + (at subtree roots that carry
///   `.with_css()` sheets) the sheet content. A difference here with an
///   equal structure tier means: keep the tree, re-cascade the affected
///   subtree(s) only.
///
/// The per-node arrays exist so a mismatch NAMES the changed nodes (the
/// eventual dirty-set for scoped re-cascade / word-granular text relayout);
/// the root folds make the equal case one u64 compare per tier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DomFingerprints {
    /// Per-node structural hash, pre-order. Folds: `node_type` content
    /// (image-callback nodes hash (fn ptr, `RefAny` `type_id`) — the `RefAny`
    /// INSTANCE is rebuilt every frame by design and is transferred, not
    /// compared; mirrors `is_layout_equivalent`), ids+classes, callback
    /// event types, contenteditable/flags/dataset, and child COUNT (pre-order
    /// alone cannot distinguish `[a [b] c]` from `[a [b c]]`).
    pub structure: Vec<u64>,
    /// Per-node style hash, pre-order: inline css properties + conditions,
    /// plus the node's attached `.with_css()` sheets (path, declarations,
    /// @-conditions, priority per rule).
    pub style: Vec<u64>,
    /// Order-sensitive fold of `structure`.
    pub structure_root: u64,
    /// Order-sensitive fold of `style`.
    pub style_root: u64,
}

/// `RefAny` payloads collected during the fingerprint walk.
///
/// Transferred onto the retained DOM when the produce side is skipped. The skip path
/// keeps last frame's `StyledDom`, but callbacks/image callbacks must use the
/// freshly-created `RefAnys` (they may reference new app state) — same
/// transfer `regenerate_layout`'s equivalence branch has always done, minus
/// the cascade it used to pay to get here. Indices are flattened `NodeIds`.
#[derive(Debug, Default, Clone)]
pub struct PreCascadeTransfers {
    /// `(flattened NodeId index, fresh image callback)` for every
    /// `NodeType::Image(DecodedImage::Callback)` node.
    pub image_callbacks: Vec<(usize, crate::callbacks::CoreImageCallback)>,
    /// `(flattened NodeId index, fresh event callbacks)` for every node with
    /// a non-empty callback list.
    pub callbacks: Vec<(usize, crate::callbacks::CoreCallbackDataVec)>,
    /// `(flattened NodeId index, fresh dataset)` for every node that carries
    /// one. Merged onto the retained DOM by [`merge_fresh_dataset`] — the
    /// skip path's equivalent of `transfer_states` — so a widget's state
    /// survives an identical rebuild and its callbacks (installed from
    /// `callbacks` above) end up on the SAME allocation as its dataset.
    pub datasets: Vec<(usize, RefAny)>,
}

/// Walk a recursive [`crate::dom::Dom`] once, pre-order.
///
/// Produces both fingerprint tiers and the `RefAny` transfer list. Cost: one hash pass over
/// node data — no cascade, no allocation proportional to anything but node
/// count.
#[allow(clippy::too_many_lines)] // cohesive single-pass walker; splitting adds state-threading
#[must_use]
pub fn fingerprint_dom(dom: &crate::dom::Dom) -> (DomFingerprints, PreCascadeTransfers) {
    use core::hash::{Hash, Hasher};

    fn node_structure_hash(node: &NodeData, child_count: usize) -> u64 {
        use crate::dom::NodeType;
        use crate::resources::DecodedImage;
        use core::hash::{Hash, Hasher};
        let mut h = crate::hash::DefaultHasher::new();

        // node_type content — image-callback special case (see struct doc)
        match node.get_node_type() {
            NodeType::Image(img) => {
                match img.get_data() {
                    DecodedImage::Callback(cb) => {
                        0xB0DE_CA11u32.hash(&mut h);
                        cb.callback.cb.hash(&mut h);
                        cb.refany.get_type_id().hash(&mut h);
                    }
                    _ => {
                        // Raw / GPU images: ImageRef hashes by id — instance
                        // identity, the same strictness is_layout_equivalent's
                        // `old_img != new_img` applies.
                        node.get_node_type().hash(&mut h);
                    }
                }
            }
            other => other.hash(&mut h),
        }

        // ids + classes (order-sensitive, as worn)
        for attr in node.attributes().as_ref() {
            match attr {
                crate::dom::AttributeType::Id(s) => {
                    1u8.hash(&mut h);
                    s.hash(&mut h);
                }
                crate::dom::AttributeType::Class(s) => {
                    2u8.hash(&mut h);
                    s.hash(&mut h);
                }
                other => {
                    3u8.hash(&mut h);
                    other.hash(&mut h);
                }
            }
        }

        // callback EVENT types only — the fn ptr + RefAny are transferred,
        // not compared (is_layout_equivalent: "compare only event types")
        node.callbacks.as_ref().len().hash(&mut h);
        for cb in node.callbacks.as_ref() {
            cb.event.hash(&mut h);
        }

        // layout-relevant attributes
        node.is_contenteditable().hash(&mut h);
        node.flags.hash(&mut h);

        // hierarchy shape
        child_count.hash(&mut h);

        h.finish()
    }

    fn node_style_hash(dom: &crate::dom::Dom) -> u64 {
        use core::hash::{Hash, Hasher};
        let mut h = crate::hash::DefaultHasher::new();

        for (prop, conds) in dom.root.style.iter_inline_properties() {
            prop.hash(&mut h);
            conds.as_slice().len().hash(&mut h);
        }

        // Attached .with_css() sheets — subtree-scoped by construction, so
        // they belong to THIS node's style identity.
        dom.css.as_ref().len().hash(&mut h);
        for css in dom.css.as_ref() {
            for rule in css.rules.as_ref() {
                rule.path.hash(&mut h);
                for decl in rule.declarations.as_ref() {
                    decl.hash(&mut h);
                }
                // DynamicSelector carries f32 media thresholds and derives no
                // Hash — the Debug repr is the stable identity here (rare
                // path: only @-rule-conditioned blocks have any).
                for cond in rule.conditions.as_ref() {
                    alloc::format!("{cond:?}").hash(&mut h);
                }
                rule.priority.hash(&mut h);
            }
        }

        h.finish()
    }

    fn walk(dom: &crate::dom::Dom, fp: &mut DomFingerprints, transfers: &mut PreCascadeTransfers) {
        use crate::dom::NodeType;
        use crate::resources::DecodedImage;

        let idx = fp.structure.len();
        fp.structure
            .push(node_structure_hash(&dom.root, dom.children.as_ref().len()));
        fp.style.push(node_style_hash(dom));

        if let NodeType::Image(img) = dom.root.get_node_type() {
            if let DecodedImage::Callback(cb) = img.get_data() {
                transfers.image_callbacks.push((idx, cb.clone()));
            }
        }
        if !dom.root.callbacks.as_ref().is_empty() {
            transfers.callbacks.push((idx, dom.root.callbacks.clone()));
        }
        if let Some(ds) = dom.root.get_dataset() {
            transfers.datasets.push((idx, ds.clone()));
        }

        for child in dom.children.as_ref() {
            walk(child, fp, transfers);
        }
    }

    let mut fp = DomFingerprints {
        structure: Vec::new(),
        style: Vec::new(),
        structure_root: 0,
        style_root: 0,
    };
    let mut transfers = PreCascadeTransfers::default();
    walk(dom, &mut fp, &mut transfers);

    let mut hs = crate::hash::DefaultHasher::new();
    for v in &fp.structure {
        v.hash(&mut hs);
    }
    fp.structure_root = hs.finish();

    let mut hy = crate::hash::DefaultHasher::new();
    for v in &fp.style {
        v.hash(&mut hy);
    }
    fp.style_root = hy.finish();

    (fp, transfers)
}

#[cfg(test)]
#[path = "diff_test.rs"]
mod diff_test;
