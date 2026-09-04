//! Accessibility Manager for integrating with `accesskit`.
//!
//! This module provides the `A11yManager` which:
//!
//! - Maintains the accessibility tree state
//! - Generates `TreeUpdate`s after each layout pass
//! - Handles `ActionRequest`s from assistive technologies
//!
//! The manager translates between Azul's internal DOM representation and
//! the platform-agnostic `accesskit` tree format.

#[cfg(feature = "a11y")]
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[cfg(feature = "a11y")]
use accesskit::{Action, ActionRequest, Node, NodeId as A11yNodeId, Rect, Role, Tree, TreeUpdate};
use azul_core::{
    dom::{
        AccessibilityAction, AccessibilityInfo, AccessibilityRole, AccessibilityState, DomId,
        DomNodeId, NodeData, NodeId, NodeType, TextSelectionStartEnd,
    },
    geom::{LogicalPosition, LogicalSize},
    styled_dom::NodeHierarchyItem,
};
use azul_css::AzString;

use crate::{solver3::layout_tree::LayoutNodeHot, window::DomLayoutResult};

/// Is this DOM node exposed to assistive technology?
///
/// ONE definition, deliberately. It used to be an inline condition inside
/// [`A11yManager::update_tree`], which is fine while accesskit is the only
/// consumer — but accesskit ships no `UIKit` and no Android backend, so the iOS
/// and Android bridges have to build their element lists themselves, and the
/// E2E `accessibility_action` op has to decide whether the node a test is
/// activating is one a screen reader could ever reach. Three consumers guessing
/// separately is three different answers to "can a screen reader see this?".
///
/// Included: anything carrying explicit `AccessibilityInfo`, anything
/// contenteditable, anything focusable, and every node type that is not pure
/// metadata (`<head>`, `<meta>`, `<script>`, …) or a pseudo-element.
#[must_use]
pub fn is_exposed_to_accessibility(node_data: &NodeData) -> bool {
    node_data.get_accessibility_info().is_some()
        || node_data.is_contenteditable()
        || node_data.is_focusable()
        || !matches!(
            node_data.node_type,
            NodeType::Head
                | NodeType::Meta
                | NodeType::Link
                | NodeType::Script
                | NodeType::Style
                | NodeType::Base
                | NodeType::Before
                | NodeType::After
                | NodeType::Marker
                | NodeType::Placeholder
                | NodeType::Source
                | NodeType::Track
                | NodeType::Param
                | NodeType::Col
                | NodeType::ColGroup
                | NodeType::Wbr
                | NodeType::Rp
                | NodeType::Rtc
                | NodeType::Bdo
                | NodeType::Bdi
                | NodeType::Data
                | NodeType::Map
                | NodeType::Area
                | NodeType::VirtualView
        )
}

/// Does this declaration actually say what KIND of control the node is?
///
/// ONE definition, deliberately — three surfaces resolve a node's role (the
/// accesskit tree's `A11yManager::build_node`, its no-layout fallback in
/// `A11yManager::update_tree`, and the iOS/Android `A11ySnapshot`) and they
/// must not disagree about it.
///
/// `AccessibilityRole::Unknown` is what `AccessibilityInfo::default()` hands
/// out, and it is the value `AccessibilityInfo::assign` treats as "not
/// specified" when it merges a patch. It therefore means *the declaration is
/// silent about the kind of control*, NOT *this control is of an unknown
/// kind* — so it must never outrank the element's own type.
///
/// It used to. Because `build_node` selected the role as "if there is any
/// `AccessibilityInfo` at all, its `role` wins", the one-field form the engine
/// itself recommends —
///
/// ```ignore
/// slider.dom().with_accessibility_name("Volume")
/// ```
///
/// — replaced the node's real role with `Role::Unknown`, which `VoiceOver` skips
/// exactly the way it skips `GenericContainer`. Naming a control DELETED it
/// from the accessibility tree, so the more accessibility an app declared the
/// less of it a screen reader could reach: 25 of `AzWidgets`' 691 nodes were
/// `Unknown` for this reason alone.
#[must_use]
pub const fn accessibility_role_is_specified(role: &AccessibilityRole) -> bool {
    !matches!(role, AccessibilityRole::Unknown)
}

/// Cursor/selection info passed to the a11y tree builder.
/// Used to set `text_selection` on contenteditable nodes so screen readers
/// can announce the cursor position and selection range.
#[cfg(feature = "a11y")]
#[derive(Debug, Clone, Copy)]
pub struct CursorA11yInfo {
    pub dom_id: DomId,
    pub node_id: NodeId,
    /// Byte offset of the selection anchor (start of selection, or cursor pos if no range)
    pub anchor_offset: usize,
    /// Byte offset of the selection focus (end of selection, or same as anchor for cursor)
    pub focus_offset: usize,
}

/// Why [`A11yManager::publish`] refused a `TreeUpdate`.
///
/// Each variant is one invariant `accesskit_consumer::tree::State::update`
/// enforces with a `panic!` — and the release build is `panic = "abort"`, so
/// the `catch_unwind` the shells wrap the adapter in cannot save the process.
/// An update that would trip one of these is never handed over; the caller
/// rebuilds the full tree (incremental path) or keeps the last good state
/// (full path). This is the 2026-08-29 `AzWriter` crash class: focus parked in a
/// DOM that two `RefreshDom` relayouts had rebuilt.
#[cfg(feature = "a11y")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum A11yUpdateError {
    /// An incremental update (no `tree`) arrived before any full tree.
    NoTreeYet,
    /// A child id names neither a delivered node nor a node in this update.
    UnknownChild {
        parent: A11yNodeId,
        child: A11yNodeId,
    },
    /// The update lists one child under two parents (or twice under one).
    DuplicateChild(A11yNodeId),
    /// An update node is neither the root, nor already delivered, nor the
    /// child of another update node — the consumer cannot place it.
    OrphanNode(A11yNodeId),
    /// The root the update names is not in the resulting tree.
    RootMissing(A11yNodeId),
    /// `focus` names no node in the resulting tree (tree.rs:75).
    FocusNotInTree(A11yNodeId),
}

/// The accessibility tree as the platform adapter currently holds it: every
/// node id with its child list. [`Self::apply`] replays the consumer's merge
/// rules so a bad update is caught HERE, in safe code, instead of aborting
/// the process inside the adapter.
#[cfg(feature = "a11y")]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct A11yTreeMirror {
    pub root: Option<A11yNodeId>,
    pub children: BTreeMap<A11yNodeId, Vec<A11yNodeId>>,
}

#[cfg(feature = "a11y")]
impl A11yTreeMirror {
    /// Apply `update` exactly the way `accesskit_consumer` would and return
    /// the resulting tree — or the invariant the consumer would have
    /// panicked on.
    ///
    /// Merge rules mirrored from `tree.rs::State::update`:
    /// - `tree: Some` re-roots; `tree: None` keeps the delivered root
    ///   (and needs one).
    /// - every child an update node lists must exist (delivered or in the
    ///   update); a child listed twice is a duplicate.
    /// - an update node must be the root, already delivered, or listed as a
    ///   child by another update node.
    /// - nodes an updated parent no longer lists become unreachable and are
    ///   dropped (with their subtrees) — the consumer removes them silently,
    ///   which is why a focus on such a node is the classic abort.
    /// - root and focus must survive in the reachable set.
    pub fn apply(&self, update: &TreeUpdate) -> Result<Self, A11yUpdateError> {
        let root = match (&update.tree, self.root) {
            (Some(t), _) => t.root,
            (None, Some(r)) => r,
            (None, None) => return Err(A11yUpdateError::NoTreeYet),
        };
        let update_ids: BTreeSet<A11yNodeId> = update.nodes.iter().map(|(id, _)| *id).collect();

        let mut seen_children = BTreeSet::new();
        for (id, node) in &update.nodes {
            for &child in node.children() {
                if !seen_children.insert(child) {
                    return Err(A11yUpdateError::DuplicateChild(child));
                }
                if !self.children.contains_key(&child) && !update_ids.contains(&child) {
                    return Err(A11yUpdateError::UnknownChild {
                        parent: *id,
                        child,
                    });
                }
            }
        }
        for id in &update_ids {
            if *id != root && !self.children.contains_key(id) && !seen_children.contains(id) {
                return Err(A11yUpdateError::OrphanNode(*id));
            }
        }

        let mut merged = self.children.clone();
        for (id, node) in &update.nodes {
            merged.insert(*id, node.children().to_vec());
        }
        if !merged.contains_key(&root) {
            return Err(A11yUpdateError::RootMissing(root));
        }
        let mut reachable = BTreeSet::new();
        let mut stack = vec![root];
        while let Some(n) = stack.pop() {
            if reachable.insert(n) {
                if let Some(cs) = merged.get(&n) {
                    stack.extend(cs.iter().copied());
                }
            }
        }
        merged.retain(|id, _| reachable.contains(id));
        if !merged.contains_key(&update.focus) {
            return Err(A11yUpdateError::FocusNotInTree(update.focus));
        }
        Ok(Self {
            root: Some(root),
            children: merged,
        })
    }
}

/// Manager for accessibility tree state and updates.
///
/// The `A11yManager` sits within `LayoutWindow` and is responsible for:
///
/// 1. Maintaining the current accessibility tree state
/// 2. Generating `TreeUpdate`s by comparing layout results with the stored tree
/// 3. Translating `ActionRequest`s from screen readers into synthetic Azul events
#[cfg(feature = "a11y")]
#[derive(Debug)]
pub struct A11yManager {
    /// The root node ID of the accessibility tree (represents the window).
    pub root_id: A11yNodeId,
    /// The current accessibility tree state.
    pub tree: Option<Tree>,
    /// The last generated tree update (for platform adapter consumption).
    pub last_tree_update: Option<TreeUpdate>,
    /// Whether the full tree has been sent to the platform adapter at least once.
    /// After initialization, incremental updates can use `tree: None`.
    pub tree_initialized: bool,
    /// The tree as the platform adapter currently holds it — advanced by
    /// [`Self::take_pending`] when a shell hands `last_tree_update` over.
    pub delivered: A11yTreeMirror,
    /// `delivered` as it will be once `last_tree_update` is handed over.
    pub pending_post: Option<A11yTreeMirror>,
    /// The last update [`Self::publish`] refused (diagnostics, tests).
    pub last_rejection: Option<A11yUpdateError>,
    /// Scroll offsets moved since the last full rebuild — bounds and
    /// `scroll_x/y` in the delivered tree are stale until one runs.
    pub scroll_dirty: bool,
    /// When the last scroll-driven rebuild ran (see
    /// [`Self::scroll_rebuild_due`]).
    pub last_scroll_rebuild: Option<std::time::Instant>,
}

#[cfg(feature = "a11y")]
impl Default for A11yManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "a11y")]
impl A11yManager {
    /// Creates a new `A11yManager` with an empty tree containing only a root window node.
    #[must_use]
    pub const fn new() -> Self {
        let root_id = A11yNodeId(0);
        Self {
            root_id,
            tree: None,
            last_tree_update: None,
            tree_initialized: false,
            delivered: A11yTreeMirror {
                root: None,
                children: BTreeMap::new(),
            },
            pending_post: None,
            last_rejection: None,
            scroll_dirty: false,
            last_scroll_rebuild: None,
        }
    }

    /// THE ONE WAY an update reaches `last_tree_update`.
    ///
    /// Folds `update` into whatever is still parked (a full update
    /// supersedes; an incremental one merges node-by-node so the slot always
    /// holds ONE coherent update — replacing a parked full tree with a later
    /// incremental used to drop the full tree on the floor), then replays
    /// the consumer's merge rules against the tree the adapter actually
    /// holds. Refused updates leave the slot exactly as it was and are
    /// recorded in `last_rejection`; the caller decides (incremental →
    /// rebuild the full tree; full → keep the last good state).
    pub fn publish(&mut self, update: TreeUpdate) -> Result<(), A11yUpdateError> {
        let prev = self.last_tree_update.take();
        let merged = Self::merge_pending(prev.clone(), update);
        match self.delivered.apply(&merged) {
            Ok(post) => {
                if merged.tree.is_some() {
                    self.tree_initialized = true;
                }
                self.pending_post = Some(post);
                self.last_tree_update = Some(merged);
                Ok(())
            }
            Err(e) => {
                self.last_tree_update = prev;
                self.last_rejection = Some(e);
                Err(e)
            }
        }
    }

    /// Fold `new` into a still-parked update (see [`Self::publish`]).
    fn merge_pending(prev: Option<TreeUpdate>, new: TreeUpdate) -> TreeUpdate {
        match prev {
            None => new,
            Some(_) if new.tree.is_some() => new,
            Some(mut parked) => {
                for (id, node) in new.nodes {
                    if let Some(slot) = parked.nodes.iter_mut().find(|(i, _)| *i == id) {
                        slot.1 = node;
                    } else {
                        parked.nodes.push((id, node));
                    }
                }
                parked.focus = new.focus;
                parked
            }
        }
    }

    /// Hand the parked update to a platform adapter and advance
    /// `delivered` to the tree the adapter will hold afterwards. Every shell
    /// drains the slot through this — a raw `last_tree_update.take()` would
    /// leave the mirror behind and the next incremental update would be
    /// validated against the wrong tree.
    pub fn take_pending(&mut self) -> Option<TreeUpdate> {
        let update = self.last_tree_update.take()?;
        if let Some(post) = self.pending_post.take() {
            self.delivered = post;
        }
        Some(update)
    }

    /// A scroll offset changed: bounds and `scroll_x/y` in the delivered tree
    /// are stale. The fast scroll path never re-lays out, so nothing else
    /// would ever rebuild the tree — screen readers saw pre-scroll rects.
    pub const fn mark_scroll_dirty(&mut self) {
        self.scroll_dirty = true;
    }

    /// Whether a scroll-driven full rebuild should run NOW: dirty, and at
    /// least `min_interval` since the previous one. Clears the flag and
    /// stamps the time when it answers `true`; a throttled `false` keeps the
    /// flag so the next tick re-asks — the final state of a glide lands
    /// within one interval of the last offset change.
    pub fn scroll_rebuild_due(
        &mut self,
        now: std::time::Instant,
        min_interval: std::time::Duration,
    ) -> bool {
        if !self.scroll_dirty {
            return false;
        }
        let due = self
            .last_scroll_rebuild
            .is_none_or(|last| now.saturating_duration_since(last) >= min_interval);
        if due {
            self.scroll_dirty = false;
            self.last_scroll_rebuild = Some(now);
        }
        due
    }

    /// Sum of every ANCESTOR scroll container's current offset — what
    /// translates a node's static layout position to where it is on screen.
    /// A scroller's own box does not move when it scrolls; its content does,
    /// so the walk starts at the parent.
    /// The accumulated scroll offset of every scrollable ANCESTOR of a node.
    ///
    /// Layout rects are in CONTENT space; anything drawn or reported in
    /// VIEWPORT space (the a11y tree's bounds, the focus ring) has to
    /// subtract this or it lands where the node would be if nothing were
    /// scrolled. `pub(crate)` because the focus ring needs the very same
    /// answer - one projection, not two that can disagree.
    pub(crate) fn ancestor_scroll_offset(
        dom_id: DomId,
        node_hierarchy: &[NodeHierarchyItem],
        dom_idx: usize,
        scroll_manager: &crate::managers::scroll_state::ScrollManager,
    ) -> LogicalPosition {
        let mut acc = LogicalPosition::zero();
        let mut cur = node_hierarchy.get(dom_idx).and_then(NodeHierarchyItem::parent_id);
        let mut guard = 0usize;
        while let Some(parent) = cur {
            guard += 1;
            if guard > 65_536 {
                break;
            }
            if let Some(off) = scroll_manager.get_current_offset(dom_id, parent) {
                acc.x += off.x;
                acc.y += off.y;
            }
            cur = node_hierarchy.get(parent.index()).and_then(NodeHierarchyItem::parent_id);
        }
        acc
    }

    /// Force the collected child lists to satisfy accesskit's `TreeUpdate`
    /// invariants, so a malformed a11y tree can never abort the process (the
    /// release build is `panic = "abort"`, so the shell's `catch_unwind` around
    /// `update_if_active` cannot catch accesskit's consumer panic).
    ///
    /// Given `node_ids` (every node present in the update, in order), the
    /// `root_id`, and the raw `root_children` + `parent_children_map`, this:
    /// - drops any child that names no present node, is its own parent, or was
    ///   ALREADY claimed by another parent (accesskit forbids a node having two
    ///   parents — a GLOBAL duplicate, tree.rs:225), and clears the child list of
    ///   a parent that is itself not present;
    /// - re-hangs every exposed non-root node that no parent claimed off the root,
    ///   so every node is reachable (accesskit tree.rs:307).
    ///
    /// Root's children win a tie (they are processed first). The result is a
    /// forest rooted at `root_id` with each node reachable exactly once.
    fn enforce_child_invariants(
        node_ids: &[A11yNodeId],
        root_id: A11yNodeId,
        root_children: &mut Vec<A11yNodeId>,
        parent_children_map: &mut HashMap<A11yNodeId, Vec<A11yNodeId>>,
    ) {
        let valid: std::collections::HashSet<A11yNodeId> = node_ids.iter().copied().collect();
        let mut claimed: std::collections::HashSet<A11yNodeId> = std::collections::HashSet::new();
        root_children.retain(|c| valid.contains(c) && *c != root_id && claimed.insert(*c));
        for (parent, children) in parent_children_map.iter_mut() {
            if !valid.contains(parent) {
                children.clear();
                continue;
            }
            let p = *parent;
            children.retain(|c| valid.contains(c) && *c != p && claimed.insert(*c));
        }
        for id in node_ids {
            if *id != root_id && !claimed.contains(id) {
                claimed.insert(*id);
                root_children.push(*id);
            }
        }
    }

    /// Updates the accessibility tree based on the current layout state.
    ///
    /// This should be called after each layout pass to synchronize the
    /// accessibility tree with the visual representation.
    #[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/font/fixed-point/debug-marker cast
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    #[must_use]
    pub fn update_tree(
        root_id: A11yNodeId,
        layout_results: &BTreeMap<DomId, DomLayoutResult>,
        scroll_manager: &crate::managers::scroll_state::ScrollManager,
        window_title: &AzString,
        window_size: LogicalSize,
        focused_node: Option<DomNodeId>,
        hidpi_factor: f32,
        dirty_text_overrides: &BTreeMap<(DomId, NodeId), String>,
        cursor_info: Option<CursorA11yInfo>,
    ) -> TreeUpdate {
        let mut nodes = Vec::new();
        let mut root_children = Vec::new();

        // Map from (DomId, NodeId) to A11yNodeId for building parent-child relationships
        let mut node_id_map: HashMap<(u32, u32), A11yNodeId> = HashMap::new();

        // Map to collect children for each parent
        let mut parent_children_map: HashMap<A11yNodeId, Vec<A11yNodeId>> = HashMap::new();

        // Create root window node and add it to the nodes list
        let mut root_node = Node::new(Role::Window);
        root_node.set_label(window_title.as_str());
        nodes.push((root_id, root_node));

        for (dom_id, layout_result) in layout_results {
            let styled_dom = &layout_result.styled_dom;
            let node_hierarchy = styled_dom.node_hierarchy.as_ref();
            let node_data_slice = styled_dom.node_data.as_ref();

            // First pass: Create a11y nodes for each DOM node
            for (dom_idx, node_data) in node_data_slice.iter().enumerate() {
                let a11y_info = node_data.get_accessibility_info();

                // Include every node that has a meaningful role — see
                // `is_exposed_to_accessibility`, which is the single definition
                // shared with every other a11y surface.
                let should_create_node = is_exposed_to_accessibility(node_data);

                if !should_create_node {
                    continue;
                }

                // Generate stable A11yNodeId: offset by 1 to avoid collision with root_id(0)
                let a11y_node_id = Self::encode_a11y_node_id(dom_id.inner, dom_idx);

                // Get layout info: absolute position from calculated_positions,
                // size from layout node. Uses dom_to_layout to map DOM → layout index.
                let dom_node_id = NodeId::new(dom_idx);
                let layout_info = layout_result
                    .layout_tree
                    .dom_to_layout
                    .get(&dom_node_id)
                    .and_then(|indices| indices.first())
                    .and_then(|&layout_idx| {
                        let hot = layout_result.layout_tree.get(layout_idx)?;
                        let abs_pos = layout_result
                            .calculated_positions
                            .get(layout_idx.index())
                            .copied();
                        Some((hot, layout_idx, abs_pos))
                    });

                // Screen position = static layout position minus every
                // ancestor scroller's offset. Bounds used to be the
                // unscrolled layout rects, so after any scroll VoiceOver's
                // cursor rectangles sat where the content had been.
                let ancestor_scroll = Self::ancestor_scroll_offset(
                    *dom_id,
                    node_hierarchy,
                    dom_idx,
                    scroll_manager,
                );
                let layout_info = layout_info.map(|(hot, idx, pos)| {
                    (
                        hot,
                        idx,
                        pos.map(|p| LogicalPosition {
                            x: p.x - ancestor_scroll.x,
                            y: p.y - ancestor_scroll.y,
                        }),
                    )
                });

                let a11y_info_ref = a11y_info;
                let mut node = if let Some((layout_node, _layout_idx, abs_pos)) = layout_info {
                    Self::build_node(
                        node_data,
                        layout_node,
                        abs_pos,
                        a11y_info_ref,
                        hidpi_factor,
                        window_size,
                    )
                } else {
                    // Same rule as `build_node` (which this branch stands in
                    // for when the node has no layout yet): only a SPECIFIED
                    // role overrides the element's own type.
                    let role = match a11y_info_ref {
                        Some(info) if accessibility_role_is_specified(&info.role) => {
                            Self::map_role(&info.role)
                        }
                        _ => Self::node_type_to_role(&node_data.node_type),
                    };
                    let mut builder = Node::new(role);
                    if let NodeType::Text(text) = &node_data.node_type {
                        builder.set_label(text.as_str());
                    }
                    builder
                };

                // MWA-B10: advertise the scroll surface. The INBOUND handler
                // (LayoutWindow::process_accessibility_action) has handled
                // ScrollUp/Down/Left/Right/SetScrollOffset/ScrollIntoView all
                // along — but the tree never declared any scroll action or
                // offset, so screen readers had nothing to invoke.
                if let Some((offset, max_x, max_y)) =
                    scroll_manager.a11y_scroll_info(*dom_id, NodeId::new(dom_idx))
                {
                    node.set_scroll_x(f64::from(offset.x));
                    node.set_scroll_x_min(0.0);
                    node.set_scroll_x_max(f64::from(max_x));
                    node.set_scroll_y(f64::from(offset.y));
                    node.set_scroll_y_min(0.0);
                    node.set_scroll_y_max(f64::from(max_y));
                    node.set_clips_children();
                    if max_y > 0.0 {
                        node.add_action(Action::ScrollUp);
                        node.add_action(Action::ScrollDown);
                    }
                    if max_x > 0.0 {
                        node.add_action(Action::ScrollLeft);
                        node.add_action(Action::ScrollRight);
                    }
                    node.add_action(Action::SetScrollOffset);
                }

                // Collect child text and promote to this node's label or value.
                // Only do this when all children are text nodes — if the node has
                // interactive children (links, buttons, inputs), DON'T set a group
                // label, so VoiceOver navigates into the children individually.
                //
                // For edited contenteditable nodes, dirty_text_overrides has the
                // current text (from the relayout path) instead of the stale
                // StyledDom text.
                {
                    let hierarchy_item = &node_hierarchy[dom_idx];
                    let dom_node_id_key = (*dom_id, NodeId::new(dom_idx));

                    // Use dirty text override if this node was edited since last RefreshDom
                    let (text_content, has_non_text_children) =
                        dirty_text_overrides.get(&dom_node_id_key).map_or_else(
                            || {
                                let mut text = String::new();
                                let mut has_non_text = false;

                                let mut child = hierarchy_item.first_child_id(NodeId::new(dom_idx));
                                while let Some(child_id) = child {
                                    if let Some(child_data) = node_data_slice.get(child_id.index())
                                    {
                                        if let NodeType::Text(t) = &child_data.node_type {
                                            if !text.is_empty() {
                                                text.push(' ');
                                            }
                                            text.push_str(t.as_str());
                                        } else {
                                            has_non_text = true;
                                        }
                                    }
                                    if child_id.index() >= node_hierarchy.len() {
                                        break;
                                    }
                                    child = node_hierarchy[child_id.index()].next_sibling_id();
                                }
                                (text, has_non_text)
                            },
                            |override_text| (override_text.clone(), false),
                        );

                    if !text_content.is_empty() {
                        if node_data.is_contenteditable()
                            || matches!(node_data.node_type, NodeType::TextArea | NodeType::Input)
                        {
                            node.set_value(text_content.as_str());
                            // Add text editing actions for contenteditable/input nodes
                            node.add_action(Action::SetTextSelection);
                            node.add_action(Action::ReplaceSelectedText);
                            node.add_action(Action::SetValue);

                            // If cursor/selection is in this node, expose to screen readers
                            if let Some(ref ci) = cursor_info {
                                if ci.dom_id == *dom_id && ci.node_id == NodeId::new(dom_idx) {
                                    let char_lengths: Vec<u8> =
                                        text_content.chars().map(|c| c.len_utf16() as u8).collect();
                                    node.set_character_lengths(char_lengths.clone());

                                    let byte_to_char_idx = |byte_off: usize| -> usize {
                                        text_content
                                            .char_indices()
                                            .take_while(|(b, _)| *b < byte_off)
                                            .count()
                                            .min(char_lengths.len())
                                    };

                                    let anchor_idx = byte_to_char_idx(ci.anchor_offset);
                                    let focus_idx = byte_to_char_idx(ci.focus_offset);

                                    node.set_text_selection(accesskit::TextSelection {
                                        anchor: accesskit::TextPosition {
                                            node: a11y_node_id,
                                            character_index: anchor_idx,
                                        },
                                        focus: accesskit::TextPosition {
                                            node: a11y_node_id,
                                            character_index: focus_idx,
                                        },
                                    });
                                }
                            }
                        } else if !has_non_text_children {
                            // Only promote text when there are NO interactive children.
                            // Otherwise VoiceOver reads the label instead of navigating children.
                            node.set_label(text_content.as_str());
                        }
                    }
                }

                node_id_map.insert((dom_id.inner as u32, dom_idx as u32), a11y_node_id);
                nodes.push((a11y_node_id, node));
            }

            // Second pass: Build parent-child relationships using DOM hierarchy
            for (dom_idx, _) in node_data_slice.iter().enumerate() {
                let a11y_node_id = match node_id_map.get(&(dom_id.inner as u32, dom_idx as u32)) {
                    Some(id) => *id,
                    None => continue,
                };

                let hierarchy_item = &node_hierarchy[dom_idx];

                // Walk up the DOM tree to find the nearest accessible ancestor.
                // parent_id() decodes the 1-based encoding: 0 = None, n+1 = Some(NodeId(n))
                let mut current_parent = hierarchy_item.parent_id();
                let mut accessible_parent_id = None;
                let mut iterations = 0;

                while let Some(parent_node_id) = current_parent {
                    iterations += 1;
                    if iterations > 10_000 {
                        break;
                    }

                    let parent_idx = parent_node_id.index();
                    if let Some(parent_a11y_id) =
                        node_id_map.get(&(dom_id.inner as u32, parent_idx as u32))
                    {
                        accessible_parent_id = Some(*parent_a11y_id);
                        break;
                    }
                    if parent_idx >= node_hierarchy.len() {
                        break;
                    }
                    current_parent = node_hierarchy[parent_idx].parent_id();
                }

                if let Some(parent_id) = accessible_parent_id {
                    parent_children_map
                        .entry(parent_id)
                        .or_default()
                        .push(a11y_node_id);
                } else {
                    root_children.push(a11y_node_id);
                }
            }
        }

        // A11Y CONSISTENCY GUARD. accesskit's tree consumer PANICS on a
        // malformed `TreeUpdate` — a focus/child naming no node (tree.rs:75), a
        // node claimed as a child by two parents (:225, a GLOBAL "duplicate
        // child"), or a node reachable from no parent at all (:307). Because the
        // release build is `panic = "abort"`, the `catch_unwind` around
        // `update_if_active` in the macOS shell CANNOT save it: the process
        // aborts. This merges the MAIN window's DOM with EVERY transient-window
        // child DOM into one tree, which is exactly where a dangling/duplicate/
        // orphan reference slips in (a focused node from a DOM that just
        // regenerated, a subtree whose accessible parent wasn't exposed). Enforce
        // the invariants here so a bad tree degrades gracefully — the whole
        // "a11y update aborts the app" bug class cannot recur.
        let node_ids: Vec<A11yNodeId> = nodes.iter().map(|(id, _)| *id).collect();
        Self::enforce_child_invariants(
            &node_ids,
            root_id,
            &mut root_children,
            &mut parent_children_map,
        );

        // Third pass: Set children on all nodes (including root)
        for (node_id, node) in &mut nodes {
            if *node_id == root_id {
                // Root window node gets top-level DOM nodes as children
                node.set_children(root_children.clone());
            } else if let Some(children) = parent_children_map.get(node_id) {
                node.set_children(children.clone());
            }
        }

        // Set focus to the currently focused DOM node (from FocusManager).
        // If no node is focused, fall back to the first visible content node.
        // VoiceOver navigates to the focused element on activation.
        let focus = focused_node
            .and_then(|dom_node_id| {
                let dom_idx = dom_node_id.node.into_crate_internal()?.index();
                node_id_map
                    .get(&(dom_node_id.dom.inner as u32, dom_idx as u32))
                    .copied()
            })
            .unwrap_or_else(|| {
                // Fallback: first non-container node
                nodes
                    .iter()
                    .find(|(id, node)| {
                        *id != root_id
                            && !matches!(node.role(), Role::GenericContainer | Role::Window)
                    })
                    .map_or(root_id, |(id, _)| *id)
            });

        // Focus MUST name a node in the update (accesskit tree.rs:75). The
        // fallback above normally guarantees this, but a `focused_node` mapped
        // from a DOM that regenerated between the focus write and this build can
        // resolve to an id that was filtered out — degrade to the root instead of
        // aborting.
        let focus = if node_ids.contains(&focus) {
            focus
        } else {
            root_id
        };

        TreeUpdate {
            nodes,
            tree: Some(Tree::new(root_id)),
            focus,
            tree_id: accesskit::TreeId::ROOT,
        }
    }

    /// MWA-B10: outbound twin of `map_accesskit_action` — declares a node's
    /// supported actions in the tree (payload-carrying variants map to their
    /// action KIND; the payload only exists on inbound requests).
    const fn map_action_to_accesskit(action: &AccessibilityAction) -> Action {
        use azul_core::a11y::AccessibilityAction as A;
        match action {
            A::Default => Action::Click,
            A::Focus => Action::Focus,
            A::Blur => Action::Blur,
            A::Collapse => Action::Collapse,
            A::Expand => Action::Expand,
            A::ScrollIntoView => Action::ScrollIntoView,
            A::Increment => Action::Increment,
            A::Decrement => Action::Decrement,
            A::ShowContextMenu => Action::ShowContextMenu,
            A::HideTooltip => Action::HideTooltip,
            A::ShowTooltip => Action::ShowTooltip,
            A::ScrollUp => Action::ScrollUp,
            A::ScrollDown => Action::ScrollDown,
            A::ScrollLeft => Action::ScrollLeft,
            A::ScrollRight => Action::ScrollRight,
            A::ReplaceSelectedText(_) => Action::ReplaceSelectedText,
            A::ScrollToPoint(_) => Action::ScrollToPoint,
            A::SetScrollOffset(_) => Action::SetScrollOffset,
            A::SetTextSelection(_) => Action::SetTextSelection,
            A::SetSequentialFocusNavigationStartingPoint => {
                Action::SetSequentialFocusNavigationStartingPoint
            }
            A::SetValue(_) | A::SetNumericValue(_) => Action::SetValue,
            A::CustomAction(_) => Action::CustomAction,
        }
    }

    /// Builds an accesskit Node from Azul's `NodeData` and layout information.
    #[allow(clippy::cast_sign_loss)] // bounded graphics/coord/font/fixed-point/debug-marker cast
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    fn build_node(
        node_data: &NodeData,
        layout_node: &LayoutNodeHot,
        abs_pos: Option<LogicalPosition>,
        a11y_info: Option<&AccessibilityInfo>,
        hidpi_factor: f32,
        window_size: LogicalSize,
    ) -> Node {
        // Set role based on NodeType or AccessibilityInfo. A DECLARED role
        // wins; an `Unknown` one is not a declaration at all — see
        // `accessibility_role_is_specified`, which is why naming a control no
        // longer erases its role.
        let role = if node_data.is_contenteditable() {
            Role::MultilineTextInput
        } else {
            match a11y_info {
                Some(info) if accessibility_role_is_specified(&info.role) => {
                    Self::map_role(&info.role)
                }
                _ => Self::node_type_to_role(&node_data.node_type),
            }
        };

        let mut builder = Node::new(role);

        // Set HTML tag name for screen readers that use it
        let tag = node_data.node_type.get_path().to_string();
        if !tag.is_empty() {
            builder.set_html_tag(tag.as_str());
        }

        // === Label and Value ===
        // Priority: explicit a11y info > DOM attributes > text content
        if let Some(info) = a11y_info {
            if let Some(name) = info.accessibility_name.as_option() {
                builder.set_label(name.as_str());
            }
            if let Some(value) = info.accessibility_value.as_option() {
                builder.set_value(value.as_str());
            }
            if let Some(desc) = info.description.as_option() {
                builder.set_description(desc.as_str());
            }
        }

        // DOM attribute overrides
        if let Some(label) = node_data.get_accessible_label() {
            builder.set_label(label);
        }
        if let Some(value) = node_data.get_accessible_value() {
            builder.set_value(value);
        }
        // Text node: set as label
        if let NodeType::Text(text) = &node_data.node_type {
            builder.set_label(text.as_str());
        }

        // === States from AccessibilityInfo ===
        if let Some(info) = a11y_info {
            for state in info.states.as_ref() {
                match state {
                    AccessibilityState::Unavailable => {
                        builder.set_disabled();
                    }
                    AccessibilityState::Readonly => {
                        builder.set_read_only();
                    }
                    AccessibilityState::CheckedTrue => {
                        builder.set_toggled(accesskit::Toggled::True);
                    }
                    AccessibilityState::CheckedFalse => {
                        builder.set_toggled(accesskit::Toggled::False);
                    }
                    AccessibilityState::Expanded => {
                        builder.set_expanded(true);
                    }
                    AccessibilityState::Collapsed => {
                        builder.set_expanded(false);
                    }
                    AccessibilityState::Focusable => {
                        builder.add_action(Action::Focus);
                    }
                    AccessibilityState::Selected => {
                        builder.set_selected(true);
                    }
                    AccessibilityState::Busy => {
                        builder.set_busy();
                    }
                    AccessibilityState::Offscreen => {
                        builder.set_hidden();
                    }
                    _ => {}
                }
            }
        }

        // MWA-B10: declare user-supplied supported actions — the public
        // AccessibilityInfo.supported_actions field was never read, so
        // API-declared actions never reached assistive technology.
        if let Some(info) = a11y_info {
            for action in info.supported_actions.as_ref() {
                builder.add_action(Self::map_action_to_accesskit(action));
            }
        }

        // MWA-B10: every content node can be scrolled into view — the
        // inbound handler implements it; declaring it lets screen readers
        // use it for navigation.
        builder.add_action(Action::ScrollIntoView);

        // === Heading level ===
        match &node_data.node_type {
            NodeType::H1 => {
                builder.set_level(1);
            }
            NodeType::H2 => {
                builder.set_level(2);
            }
            NodeType::H3 => {
                builder.set_level(3);
            }
            NodeType::H4 => {
                builder.set_level(4);
            }
            NodeType::H5 => {
                builder.set_level(5);
            }
            NodeType::H6 => {
                builder.set_level(6);
            }
            _ => {}
        }

        // Wire up HTML attributes to accesskit properties
        for attr in node_data.attributes().as_ref() {
            match attr {
                azul_core::dom::AttributeType::AriaLabel(s) => {
                    builder.set_label(s.as_str());
                }
                azul_core::dom::AttributeType::Title(s) | azul_core::dom::AttributeType::Alt(s) => {
                    builder.set_description(s.as_str());
                }
                azul_core::dom::AttributeType::Placeholder(s) => {
                    builder.set_placeholder(s.as_str());
                }
                azul_core::dom::AttributeType::Value(s) => {
                    builder.set_value(s.as_str());
                }
                azul_core::dom::AttributeType::Disabled => {
                    builder.set_disabled();
                }
                azul_core::dom::AttributeType::Readonly => {
                    builder.set_read_only();
                }
                azul_core::dom::AttributeType::CheckedTrue => {
                    builder.set_toggled(accesskit::Toggled::True);
                }
                azul_core::dom::AttributeType::CheckedFalse => {
                    builder.set_toggled(accesskit::Toggled::False);
                }
                azul_core::dom::AttributeType::Required => {
                    builder.set_required();
                }
                azul_core::dom::AttributeType::Hidden => {
                    builder.set_hidden();
                }
                azul_core::dom::AttributeType::Lang(s) => {
                    builder.set_language(s.as_str());
                }
                azul_core::dom::AttributeType::ColSpan(n) => {
                    builder.set_column_span(*n as usize);
                }
                azul_core::dom::AttributeType::RowSpan(n) => {
                    builder.set_row_span(*n as usize);
                }
                _ => {}
            }
        }

        // Set bounds: absolute position, offset by padding+border, scaled to physical pixels,
        // clipped to window viewport so VoiceOver highlights don't extend off-screen.
        if let (Some(pos), Some(size)) = (abs_pos, layout_node.used_size) {
            let bp = layout_node.box_props.unpack();
            let pad_left = bp.padding.left + bp.border.left;
            let pad_top = bp.padding.top + bp.border.top;
            let pad_right = bp.padding.right + bp.border.right;
            let pad_bottom = bp.padding.bottom + bp.border.bottom;

            let s = f64::from(hidpi_factor);
            let ww = f64::from(window_size.width) * s;
            let wh = f64::from(window_size.height) * s;

            let x0 = (f64::from(pos.x + pad_left) * s).max(0.0).min(ww);
            let y0 = (f64::from(pos.y + pad_top) * s).max(0.0).min(wh);
            let x1 = (f64::from(pos.x + size.width - pad_right) * s)
                .max(0.0)
                .min(ww);
            let y1 = (f64::from(pos.y + size.height - pad_bottom) * s)
                .max(0.0)
                .min(wh);

            if x1 > x0 && y1 > y0 {
                builder.set_bounds(Rect { x0, y0, x1, y1 });
            }
        }

        // Add supported actions based on the DOM node's own properties.
        // VoiceOver uses these to determine what the user can do with the element.
        if node_data.is_focusable() || node_data.is_contenteditable() {
            builder.add_action(Action::Focus);
        }
        if node_data.has_activation_behavior() {
            builder.add_action(Action::Click);
        }

        // ARIA relations + live-region from AccessibilityInfo. aria-labelledby /
        // aria-describedby reference another node; encode its id the SAME way the
        // tree walk does (encode_a11y_node_id) so the relation resolves to a real
        // node. is_live_region maps to accesskit's Live property. These were all
        // previously dropped (screen readers got no labelled-by/described-by
        // relations and no live-region announcements).
        if let Some(info) = a11y_info {
            if let azul_core::dom::OptionDomNodeId::Some(target) = info.labelled_by {
                if let Some(id) = Self::a11y_node_id_for(&target) {
                    builder.push_labelled_by(id);
                }
            }
            if let azul_core::dom::OptionDomNodeId::Some(target) = info.described_by {
                if let Some(id) = Self::a11y_node_id_for(&target) {
                    builder.push_described_by(id);
                }
            }
            if info.is_live_region {
                builder.set_live(accesskit::Live::Polite);
            }
        }

        // MWA-C-a11y: aria-live="polite|assertive" HTML attribute — arrives
        // as AriaProperty/Custom (no parsing existed; live regions were only
        // reachable through the explicit AccessibilityInfo.is_live_region
        // flag, so HTML-defined live regions never announced).
        for attr in node_data.attributes() {
            let (name, value) = match attr {
                azul_core::dom::AttributeType::AriaProperty(nv)
                | azul_core::dom::AttributeType::Custom(nv) => {
                    (nv.attr_name.as_str(), nv.value.as_str())
                }
                _ => continue,
            };
            if name.eq_ignore_ascii_case("aria-live") {
                match value.to_ascii_lowercase().as_str() {
                    "polite" => builder.set_live(accesskit::Live::Polite),
                    "assertive" => builder.set_live(accesskit::Live::Assertive),
                    _ => builder.set_live(accesskit::Live::Off),
                }
            }
        }

        builder
    }

    /// Encode a `(DomId.inner, node index)` pair into the stable `A11yNodeId` used
    /// throughout the tree (offset by 1 so it never collides with `root_id` 0).
    /// Shared by the tree walk and the aria-labelledby/-describedby relation
    /// mapping, so a relation always resolves to the node the walk emitted.
    const fn encode_a11y_node_id(dom_inner: usize, node_idx: usize) -> A11yNodeId {
        A11yNodeId(((dom_inner as u64) << 32) | ((node_idx as u64) + 1))
    }

    /// Map an aria-labelledby/-describedby target `DomNodeId` to its `A11yNodeId`,
    /// or `None` if the node id can't be resolved.
    fn a11y_node_id_for(target: &DomNodeId) -> Option<A11yNodeId> {
        let idx = target.node.into_crate_internal()?.index();
        Some(Self::encode_a11y_node_id(target.dom.inner, idx))
    }

    /// Maps an HTML `NodeType` to an accesskit `Role`.
    ///
    /// Every role used here must pass accesskit's `common_filter` (i.e. NOT be
    /// `GenericContainer` or `TextRun`) or `VoiceOver` will skip the node entirely.
    /// Use `Group` for structural containers, `Paragraph` for text blocks, `Label`
    /// for inline text, and semantic roles for everything else.
    // Exhaustive NodeType -> accessibility Role mapping table; many node types share
    // a Role, but one-arm-per-NodeType is intentional for readability/maintainability.
    #[allow(clippy::match_same_arms)]
    const fn node_type_to_role(node_type: &NodeType) -> Role {
        match node_type {
            // === Text content ===
            NodeType::Text(_) => Role::Label,
            NodeType::P => Role::Paragraph,
            NodeType::Pre => Role::Code,
            NodeType::BlockQuote => Role::Blockquote,
            NodeType::Code => Role::Code,
            NodeType::Em | NodeType::I => Role::Emphasis,
            NodeType::Strong | NodeType::B => Role::Strong,
            NodeType::Mark => Role::Mark,
            NodeType::Del => Role::ContentDeletion,
            NodeType::Ins => Role::ContentInsertion,
            NodeType::Abbr | NodeType::Acronym => Role::Abbr,
            NodeType::Q => Role::Blockquote,
            NodeType::Time => Role::Time,
            NodeType::Cite | NodeType::Dfn | NodeType::Var | NodeType::Samp | NodeType::Kbd => {
                Role::Label
            }
            NodeType::Small
            | NodeType::Big
            | NodeType::Sub
            | NodeType::Sup
            | NodeType::U
            | NodeType::S => Role::Label,
            NodeType::Ruby => Role::Ruby,
            NodeType::Rt => Role::RubyAnnotation,
            NodeType::Br => Role::LineBreak,
            NodeType::Hr => Role::Splitter,

            // === Structural containers ===
            // Group (not GenericContainer) so VoiceOver can navigate into them
            NodeType::Body => Role::Group,
            NodeType::Div => Role::Group,
            NodeType::Span => Role::Group,
            NodeType::Html => Role::Group,

            // === Semantic sections ===
            NodeType::Article => Role::Article,
            NodeType::Section => Role::Section,
            NodeType::Nav => Role::Navigation,
            NodeType::Main => Role::Main,
            NodeType::Header => Role::Header,
            NodeType::Footer => Role::Footer,
            NodeType::Aside => Role::Complementary,
            NodeType::Address => Role::Group,
            NodeType::Figure => Role::Figure,
            NodeType::FigCaption => Role::FigureCaption,
            NodeType::Details => Role::Details,
            NodeType::Summary => Role::DisclosureTriangle,
            NodeType::Dialog => Role::Dialog,

            // === Headings ===
            NodeType::H1
            | NodeType::H2
            | NodeType::H3
            | NodeType::H4
            | NodeType::H5
            | NodeType::H6 => Role::Heading,

            // === Lists ===
            NodeType::Ul | NodeType::Ol | NodeType::Dir => Role::List,
            NodeType::Li => Role::ListItem,
            NodeType::Dl => Role::DescriptionList,
            NodeType::Dt => Role::Term,
            NodeType::Dd => Role::Definition,
            NodeType::Menu => Role::Menu,
            NodeType::MenuItem => Role::MenuItem,

            // === Tables ===
            NodeType::Table => Role::Table,
            NodeType::Caption => Role::Caption,
            NodeType::THead | NodeType::TBody | NodeType::TFoot => Role::RowGroup,
            NodeType::Tr => Role::Row,
            NodeType::Th => Role::ColumnHeader,
            NodeType::Td => Role::Cell,
            NodeType::ColGroup | NodeType::Col => Role::GenericContainer,

            // === Forms ===
            NodeType::Form => Role::Form,
            NodeType::FieldSet => Role::Group,
            NodeType::Legend => Role::Legend,
            NodeType::Label => Role::Label,
            NodeType::Input => Role::TextInput,
            NodeType::Button => Role::Button,
            NodeType::Select => Role::ComboBox,
            NodeType::OptGroup => Role::Group,
            NodeType::SelectOption => Role::ListBoxOption,
            NodeType::TextArea => Role::MultilineTextInput,
            NodeType::Output => Role::Status,
            NodeType::Progress => Role::ProgressIndicator,
            NodeType::Meter => Role::Meter,
            NodeType::DataList => Role::ListBox,

            // === Links ===
            NodeType::A => Role::Link,

            // === Embedded content ===
            NodeType::Image(_) => Role::Image,
            NodeType::Icon(_) => Role::Image,
            NodeType::Canvas => Role::Canvas,
            NodeType::Audio => Role::Audio,
            NodeType::Video => Role::Video,
            NodeType::Svg => Role::SvgRoot,
            NodeType::Object | NodeType::Embed => Role::EmbeddedObject,

            // === Everything else: Group (visible to VoiceOver) ===
            _ => Role::Group,
        }
    }

    /// Maps Azul's `AccessibilityRole` to accesskit's Role.
    // Exhaustive AccessibilityRole -> AccessKit Role mapping table (see node_type_to_role).
    #[allow(clippy::match_same_arms)]
    #[allow(clippy::trivially_copy_pass_by_ref)] // <=8B Copy param kept by-ref intentionally (hot pixel/coord path or to avoid churning call sites for a perf-neutral change)
    const fn map_role(role: &AccessibilityRole) -> Role {
        match role {
            AccessibilityRole::TitleBar => Role::TitleBar,
            AccessibilityRole::MenuBar => Role::MenuBar,
            AccessibilityRole::ScrollBar => Role::ScrollBar,
            AccessibilityRole::Grip => Role::Splitter,
            AccessibilityRole::Sound => Role::Audio,
            AccessibilityRole::Cursor => Role::Caret,
            AccessibilityRole::Caret => Role::Caret,
            AccessibilityRole::Alert => Role::Alert,
            AccessibilityRole::Window => Role::Window,
            AccessibilityRole::Client => Role::GenericContainer,
            AccessibilityRole::MenuPopup => Role::Menu,
            AccessibilityRole::MenuItem => Role::MenuItem,
            AccessibilityRole::Tooltip => Role::Tooltip,
            AccessibilityRole::Application => Role::Application,
            AccessibilityRole::Document => Role::Document,
            AccessibilityRole::Pane => Role::Pane,
            AccessibilityRole::Chart => Role::Figure,
            AccessibilityRole::Dialog => Role::Dialog,
            AccessibilityRole::Border => Role::GenericContainer,
            AccessibilityRole::Grouping => Role::Group,
            AccessibilityRole::Separator => Role::GenericContainer,
            AccessibilityRole::Toolbar => Role::Toolbar,
            AccessibilityRole::StatusBar => Role::Status,
            AccessibilityRole::Table => Role::Table,
            AccessibilityRole::ColumnHeader => Role::ColumnHeader,
            AccessibilityRole::RowHeader => Role::RowHeader,
            AccessibilityRole::Column => Role::GenericContainer, // No Column in accesskit 0.17
            AccessibilityRole::Row => Role::Row,
            AccessibilityRole::Cell => Role::Cell,
            AccessibilityRole::Link => Role::Link,
            AccessibilityRole::HelpBalloon => Role::Tooltip,
            AccessibilityRole::Character => Role::GenericContainer,
            AccessibilityRole::List => Role::List,
            AccessibilityRole::ListItem => Role::ListItem,
            AccessibilityRole::Outline => Role::Tree,
            AccessibilityRole::OutlineItem => Role::TreeItem,
            AccessibilityRole::PageTab => Role::Tab,
            AccessibilityRole::PropertyPage => Role::TabPanel,
            AccessibilityRole::Indicator => Role::Meter,
            AccessibilityRole::Graphic => Role::Image,
            // StaticText -> Label in accesskit 0.17
            AccessibilityRole::StaticText => Role::Label,
            AccessibilityRole::Text => Role::TextInput,
            AccessibilityRole::PushButton => Role::Button,
            AccessibilityRole::CheckButton => Role::CheckBox,
            AccessibilityRole::RadioButton => Role::RadioButton,
            AccessibilityRole::ComboBox => Role::ComboBox,
            AccessibilityRole::DropList => Role::ListBox,
            AccessibilityRole::ProgressBar => Role::ProgressIndicator,
            AccessibilityRole::Dial => Role::Meter,
            AccessibilityRole::HotkeyField => Role::TextInput,
            AccessibilityRole::Slider => Role::Slider,
            AccessibilityRole::SpinButton => Role::SpinButton,
            AccessibilityRole::Diagram => Role::Figure,
            AccessibilityRole::Animation => Role::GenericContainer,
            AccessibilityRole::Equation => Role::Math,
            AccessibilityRole::ButtonDropdown => Role::Button,
            // No MenuButton in accesskit 0.17
            AccessibilityRole::ButtonMenu => Role::Button,
            AccessibilityRole::ButtonDropdownGrid => Role::Button,
            AccessibilityRole::Whitespace => Role::GenericContainer,
            AccessibilityRole::PageTabList => Role::TabList,
            AccessibilityRole::Clock => Role::Timer,
            AccessibilityRole::SplitButton => Role::Button,
            AccessibilityRole::IpAddress => Role::TextInput,
            AccessibilityRole::Unknown => Role::Unknown,
            AccessibilityRole::Nothing => Role::GenericContainer,
        }
    }
}

/// Decodes an `A11yNodeId` back into its `(DomId, NodeId)` components.
///
/// The `A11yNodeId` encodes both values in a single u64:
/// - Upper 32 bits: `DomId` (which DOM tree the node belongs to)
/// - Lower 32 bits: `NodeId + 1` (index within that DOM tree, offset by 1 to avoid
///   colliding with the accesskit root node id, matching the encoding in `update_tree`)
#[cfg(feature = "a11y")]
#[must_use]
pub const fn decode_a11y_node_id(a11y_node_id: A11yNodeId) -> (DomId, NodeId) {
    let raw = a11y_node_id.0;
    let dom_id = DomId {
        inner: (raw >> 32) as usize,
    };
    let node_id = NodeId::new(((raw & 0xFFFF_FFFF).wrapping_sub(1)) as usize);
    (dom_id, node_id)
}

/// Maps an accesskit `ActionRequest` to an Azul `AccessibilityAction`.
///
/// Returns `None` if the action requires data that was not provided or is invalid.
#[cfg(feature = "a11y")]
#[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/font/fixed-point/debug-marker cast
#[must_use]
pub fn map_accesskit_action(request: ActionRequest) -> Option<AccessibilityAction> {
    use azul_css::{props::basic::FloatValue, AzString};

    let action = match request.action {
        Action::Click => AccessibilityAction::Default,
        Action::Focus => AccessibilityAction::Focus,
        Action::Blur => AccessibilityAction::Blur,
        Action::Collapse => AccessibilityAction::Collapse,
        Action::Expand => AccessibilityAction::Expand,
        Action::ScrollIntoView => AccessibilityAction::ScrollIntoView,
        Action::Increment => AccessibilityAction::Increment,
        Action::Decrement => AccessibilityAction::Decrement,
        Action::ShowContextMenu => AccessibilityAction::ShowContextMenu,
        Action::HideTooltip => AccessibilityAction::HideTooltip,
        Action::ShowTooltip => AccessibilityAction::ShowTooltip,
        Action::ScrollUp => AccessibilityAction::ScrollUp,
        Action::ScrollDown => AccessibilityAction::ScrollDown,
        Action::ScrollLeft => AccessibilityAction::ScrollLeft,
        Action::ScrollRight => AccessibilityAction::ScrollRight,
        Action::SetSequentialFocusNavigationStartingPoint => {
            AccessibilityAction::SetSequentialFocusNavigationStartingPoint
        }
        Action::ReplaceSelectedText => {
            let accesskit::ActionData::Value(value) = request.data? else {
                return None;
            };
            AccessibilityAction::ReplaceSelectedText(AzString::from(value.as_ref()))
        }
        Action::ScrollToPoint => {
            let accesskit::ActionData::ScrollToPoint(point) = request.data? else {
                return None;
            };
            AccessibilityAction::ScrollToPoint(LogicalPosition {
                x: point.x as f32,
                y: point.y as f32,
            })
        }
        Action::SetScrollOffset => {
            let accesskit::ActionData::SetScrollOffset(point) = request.data? else {
                return None;
            };
            AccessibilityAction::SetScrollOffset(LogicalPosition {
                x: point.x as f32,
                y: point.y as f32,
            })
        }
        Action::SetTextSelection => {
            let accesskit::ActionData::SetTextSelection(selection) = request.data? else {
                return None;
            };
            AccessibilityAction::SetTextSelection(TextSelectionStartEnd {
                selection_start: selection.anchor.character_index,
                selection_end: selection.focus.character_index,
            })
        }
        Action::SetValue => match request.data? {
            accesskit::ActionData::Value(value) => {
                AccessibilityAction::SetValue(AzString::from(value.as_ref()))
            }
            accesskit::ActionData::NumericValue(value) => {
                AccessibilityAction::SetNumericValue(FloatValue::new(value as f32))
            }
            _ => return None,
        },
        Action::CustomAction => {
            let accesskit::ActionData::CustomAction(id) = request.data? else {
                return None;
            };
            AccessibilityAction::CustomAction(id)
        }
    };

    Some(action)
}

/// Stub implementation when accessibility feature is disabled.
#[cfg(not(feature = "a11y"))]
#[derive(Debug)]
pub struct A11yManager {
    _private: (),
}

#[cfg(not(feature = "a11y"))]
impl A11yManager {
    /// Creates a new stub `A11yManager` (no-op when accessibility is disabled).
    pub fn new() -> Self {
        Self { _private: () }
    }
}

#[cfg(all(test, feature = "a11y"))]
mod a11y_relation_tests {
    use super::A11yManager;
    use accesskit::NodeId as A11yNodeId;

    /// The a11y node-id encoding must stay in lockstep with the tree walk:
    /// `(dom.inner << 32) | (idx + 1)`. `labelled_by/described_by` relations encode
    /// their targets the same way, so any drift here would point a relation at
    /// the wrong (or a nonexistent) node.
    #[test]
    fn a11y_node_id_encoding_is_stable_and_offset() {
        assert_eq!(A11yManager::encode_a11y_node_id(0, 0), A11yNodeId(1));
        assert_eq!(A11yManager::encode_a11y_node_id(0, 5), A11yNodeId(6));
        assert_eq!(
            A11yManager::encode_a11y_node_id(2, 3),
            A11yNodeId((2u64 << 32) | 4)
        );
        // Never collides with the root window node (id 0).
        assert_ne!(A11yManager::encode_a11y_node_id(0, 0), A11yNodeId(0));
    }
}

#[cfg(all(test, feature = "a11y"))]
#[allow(clippy::float_cmp, clippy::too_many_lines)]
mod autotest_generated {
    use std::collections::BTreeMap;

    use accesskit::{ActionData, Live, Point, TextPosition, TextSelection, Toggled, TreeId};
    use azul_core::{
        dom::{AttributeNameValue, AttributeType, FormattingContext, OptionDomNodeId},
        styled_dom::NodeHierarchyItemId,
        window::OptionVirtualKeyCodeCombo,
    };
    use azul_css::{css::BoxOrStatic, props::basic::FloatValue, OptionString};

    use super::*;
    use crate::{managers::scroll_state::ScrollManager, solver3::geometry::PackedBoxProps};

    // ---------------------------------------------------------------------
    // fixtures
    // ---------------------------------------------------------------------

    /// A `LayoutNodeHot` with the given used size and packed box props.
    /// `PackedBoxProps` edges are `[top, right, bottom, left]` in tenths of a pixel.
    fn hot(used_size: Option<LogicalSize>, padding: [i16; 4], border: [i16; 4]) -> LayoutNodeHot {
        LayoutNodeHot {
            box_props: PackedBoxProps {
                padding,
                border,
                ..PackedBoxProps::default()
            },
            dom_node_id: Some(NodeId::new(0)),
            used_size,
            formatting_context: FormattingContext::Block {
                establishes_new_context: false,
            },
            parent: None,
        }
    }

    fn plain_hot() -> LayoutNodeHot {
        hot(Some(LogicalSize::new(100.0, 50.0)), [0; 4], [0; 4])
    }

    fn info(role: AccessibilityRole) -> AccessibilityInfo {
        AccessibilityInfo {
            accessibility_name: OptionString::None,
            accessibility_value: OptionString::None,
            description: OptionString::None,
            accelerator: OptionVirtualKeyCodeCombo::None,
            default_action: OptionString::None,
            states: Vec::<AccessibilityState>::new().into(),
            supported_actions: Vec::<AccessibilityAction>::new().into(),
            labelled_by: OptionDomNodeId::None,
            described_by: OptionDomNodeId::None,
            role,
            is_live_region: false,
        }
    }

    fn dom_node(dom: usize, idx: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId { inner: dom },
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(idx))),
        }
    }

    fn text_node(s: &str) -> NodeData {
        NodeData::create_node(NodeType::Text(BoxOrStatic::heap(AzString::from(s))))
    }

    fn request(action: Action, data: Option<ActionData>) -> ActionRequest {
        ActionRequest {
            action,
            target_tree: TreeId::ROOT,
            target_node: A11yNodeId(1),
            data,
        }
    }

    /// A `DomLayoutResult` carrying a real `StyledDom` but an EMPTY layout
    /// tree. `update_tree`'s tree SHAPE — who is whose child, what is
    /// reachable, where focus lands — is a pure function of `node_data` +
    /// `node_hierarchy`; the layout tree only supplies bounds. So this is
    /// enough to reproduce exactly the tree a real window publishes, with no
    /// solver pass and no fonts.
    fn layout_result_of(styled_dom: azul_core::styled_dom::StyledDom) -> DomLayoutResult {
        use std::{collections::HashMap, sync::Arc};

        use azul_core::geom::LogicalRect;

        use crate::solver3::{display_list::DisplayList, layout_tree::LayoutTree};

        DomLayoutResult {
            styled_dom,
            layout_tree: LayoutTree {
                nodes: Vec::new(),
                warm: Vec::new(),
                cold: Vec::new(),
                root: 0,
                dom_to_layout: BTreeMap::new(),
                children_arena: Vec::new(),
                children_offsets: Vec::new(),
                subtree_needs_intrinsic: Vec::new(),
            },
            calculated_positions: Vec::new(),
            viewport: LogicalRect::zero(),
            display_list: Arc::new(DisplayList::default()),
            scroll_ids: HashMap::new(),
            scroll_id_to_node_id: HashMap::new(),
        }
    }

    /// `update_tree` over a set of real DOMs, one per `DomId`, in id order.
    fn update_over_doms(doms: Vec<azul_core::dom::Dom>) -> TreeUpdate {
        use azul_core::styled_dom::StyledDom;

        let mut layout_results = BTreeMap::new();
        for (i, dom) in doms.into_iter().enumerate() {
            layout_results.insert(
                DomId { inner: i },
                layout_result_of(StyledDom::create_from_dom(dom)),
            );
        }
        let scroll_manager = ScrollManager::new();
        let overrides = BTreeMap::new();
        A11yManager::update_tree(
            A11yNodeId(0),
            &layout_results,
            &scroll_manager,
            &AzString::from("Azul Widget Showcase"),
            LogicalSize::new(1000.0, 700.0),
            None,
            1.0,
            &overrides,
            None,
        )
    }

    /// `update_tree` with no DOMs at all — the smallest legal input.
    fn empty_update(
        window_size: LogicalSize,
        focused_node: Option<DomNodeId>,
        hidpi_factor: f32,
        title: &str,
    ) -> TreeUpdate {
        let layout_results = BTreeMap::new();
        let scroll_manager = ScrollManager::new();
        let overrides = BTreeMap::new();
        A11yManager::update_tree(
            A11yNodeId(0),
            &layout_results,
            &scroll_manager,
            &AzString::from(title),
            window_size,
            focused_node,
            hidpi_factor,
            &overrides,
            None,
        )
    }

    // ---------------------------------------------------------------------
    // A11yManager::new / Default (constructor)
    // ---------------------------------------------------------------------

    #[test]
    fn new_starts_with_root_zero_and_uninitialized_tree() {
        let manager = A11yManager::new();
        assert_eq!(manager.root_id, A11yNodeId(0));
        assert!(manager.tree.is_none());
        assert!(manager.last_tree_update.is_none());
        assert!(
            !manager.tree_initialized,
            "a fresh manager must force a full first TreeUpdate"
        );
    }

    #[test]
    fn default_matches_new() {
        let a = A11yManager::new();
        let b = A11yManager::default();
        assert_eq!(a.root_id, b.root_id);
        assert_eq!(a.tree_initialized, b.tree_initialized);
        assert!(b.tree.is_none() && b.last_tree_update.is_none());
    }

    // ---------------------------------------------------------------------
    // encode_a11y_node_id / decode_a11y_node_id (numeric + round-trip)
    // ---------------------------------------------------------------------

    #[test]
    fn encode_decode_round_trips_over_the_representable_domain() {
        // node_idx must stay < u32::MAX so `idx + 1` cannot carry out of the
        // low 32 bits; dom_inner must stay <= u32::MAX so it cannot shift out.
        let cases: [(usize, usize); 7] = [
            (0, 0),
            (0, 1),
            (1, 0),
            (2, 3),
            (0, u32::MAX as usize - 1),
            (u32::MAX as usize, 0),
            (u32::MAX as usize, u32::MAX as usize - 1),
        ];
        for (dom, idx) in cases {
            let encoded = A11yManager::encode_a11y_node_id(dom, idx);
            let (decoded_dom, decoded_node) = decode_a11y_node_id(encoded);
            assert_eq!(decoded_dom.inner, dom, "dom round-trip for ({dom}, {idx})");
            assert_eq!(
                decoded_node.index(),
                idx,
                "idx round-trip for ({dom}, {idx})"
            );
        }
    }

    #[test]
    fn encoded_ids_never_collide_with_the_root_window_id() {
        // root_id is 0; the +1 offset is the only thing keeping node 0 of dom 0
        // from being mistaken for the window itself.
        for (dom, idx) in [(0usize, 0usize), (0, 1), (5, 0), (u32::MAX as usize, 0)] {
            assert_ne!(
                A11yManager::encode_a11y_node_id(dom, idx),
                A11yNodeId(0),
                "({dom}, {idx}) must not encode to the root id"
            );
        }
    }

    #[test]
    fn encode_is_injective_for_neighbouring_ids() {
        let a = A11yManager::encode_a11y_node_id(0, 1);
        let b = A11yManager::encode_a11y_node_id(1, 0);
        let c = A11yManager::encode_a11y_node_id(0, 2);
        assert_ne!(a, b);
        assert_ne!(a, c);
        assert_ne!(b, c);
    }

    /// Characterisation of a known-lossy boundary: `idx + 1` carries out of the
    /// low 32 bits at `idx == u32::MAX`, silently incrementing the DomId field.
    /// Not reachable with real DOMs (a 4-billion-node tree), but pinned so the
    /// encoding contract can't drift unnoticed. Does not panic.
    #[test]
    fn encode_node_idx_at_u32_max_carries_into_the_dom_field() {
        let encoded = A11yManager::encode_a11y_node_id(0, u32::MAX as usize);
        assert_eq!(encoded, A11yNodeId(1u64 << 32));
        let (dom, node) = decode_a11y_node_id(encoded);
        assert_eq!(dom.inner, 1, "dom 0 aliases onto dom 1 at the carry");
        assert_eq!(node.index(), usize::MAX);
        // Still never collides with the root window node.
        assert_ne!(encoded, A11yNodeId(0));
    }

    /// Decoding the root id itself is nonsense (root is not a DOM node), but it
    /// must not panic: the `wrapping_sub(1)` yields the usize::MAX sentinel.
    #[test]
    fn decode_of_root_id_wraps_instead_of_panicking() {
        let (dom, node) = decode_a11y_node_id(A11yNodeId(0));
        assert_eq!(dom.inner, 0);
        assert_eq!(node.index(), usize::MAX);
    }

    #[test]
    fn decode_of_u64_max_does_not_panic() {
        let (dom, node) = decode_a11y_node_id(A11yNodeId(u64::MAX));
        assert_eq!(dom.inner, u32::MAX as usize);
        assert_eq!(node.index(), u32::MAX as usize - 1);
    }

    // ---------------------------------------------------------------------
    // a11y_node_id_for
    // ---------------------------------------------------------------------

    #[test]
    fn a11y_node_id_for_resolves_to_the_same_id_the_tree_walk_emits() {
        let target = dom_node(2, 3);
        assert_eq!(
            A11yManager::a11y_node_id_for(&target),
            Some(A11yManager::encode_a11y_node_id(2, 3)),
            "a relation must resolve to the node the walk emitted"
        );
    }

    #[test]
    fn a11y_node_id_for_returns_none_for_the_none_sentinel() {
        let target = DomNodeId {
            dom: DomId { inner: 0 },
            node: NodeHierarchyItemId::NONE,
        };
        assert_eq!(A11yManager::a11y_node_id_for(&target), None);
    }

    #[test]
    fn a11y_node_id_for_handles_extreme_dom_ids_without_panicking() {
        let target = dom_node(u32::MAX as usize, 0);
        assert_eq!(
            A11yManager::a11y_node_id_for(&target),
            Some(A11yNodeId((u32::MAX as u64) << 32 | 1))
        );
    }

    // ---------------------------------------------------------------------
    // node_type_to_role
    // ---------------------------------------------------------------------

    #[test]
    fn node_type_to_role_maps_the_documented_roles() {
        let cases: [(NodeType, Role); 18] = [
            (
                NodeType::Text(BoxOrStatic::heap(AzString::from("x"))),
                Role::Label,
            ),
            (NodeType::P, Role::Paragraph),
            (NodeType::Div, Role::Group),
            (NodeType::Body, Role::Group),
            (NodeType::A, Role::Link),
            (NodeType::Button, Role::Button),
            (NodeType::H1, Role::Heading),
            (NodeType::H6, Role::Heading),
            (NodeType::Input, Role::TextInput),
            (NodeType::TextArea, Role::MultilineTextInput),
            (NodeType::Table, Role::Table),
            (NodeType::Tr, Role::Row),
            (NodeType::Td, Role::Cell),
            (NodeType::Th, Role::ColumnHeader),
            (NodeType::Ul, Role::List),
            (NodeType::Li, Role::ListItem),
            (NodeType::Br, Role::LineBreak),
            (NodeType::Hr, Role::Splitter),
        ];
        for (node_type, expected) in cases {
            assert_eq!(
                A11yManager::node_type_to_role(&node_type),
                expected,
                "{node_type:?}"
            );
        }
    }

    #[test]
    fn node_type_to_role_falls_back_to_group_for_unmapped_types() {
        // The `_ => Role::Group` arm: metadata types that never reach the tree
        // must still produce a VoiceOver-visible role rather than a filtered one.
        for node_type in [
            NodeType::Script,
            NodeType::Style,
            NodeType::Meta,
            NodeType::Head,
        ] {
            assert_eq!(A11yManager::node_type_to_role(&node_type), Role::Group);
        }
    }

    /// The doc contract: no role emitted here may be filtered out by accesskit's
    /// `common_filter` (`GenericContainer` / `TextRun`), or VoiceOver skips the
    /// node. `Col`/`ColGroup` are the sole exceptions — see the test below.
    #[test]
    fn node_type_to_role_never_emits_a_voiceover_filtered_role() {
        let node_types = [
            NodeType::Text(BoxOrStatic::heap(AzString::from("x"))),
            NodeType::P,
            NodeType::Pre,
            NodeType::BlockQuote,
            NodeType::Code,
            NodeType::Em,
            NodeType::Strong,
            NodeType::Mark,
            NodeType::Del,
            NodeType::Ins,
            NodeType::Abbr,
            NodeType::Q,
            NodeType::Time,
            NodeType::Ruby,
            NodeType::Rt,
            NodeType::Br,
            NodeType::Hr,
            NodeType::Body,
            NodeType::Div,
            NodeType::Span,
            NodeType::Html,
            NodeType::Article,
            NodeType::Section,
            NodeType::Nav,
            NodeType::Main,
            NodeType::Header,
            NodeType::Footer,
            NodeType::Aside,
            NodeType::Figure,
            NodeType::FigCaption,
            NodeType::Details,
            NodeType::Summary,
            NodeType::Dialog,
            NodeType::H1,
            NodeType::H2,
            NodeType::H3,
            NodeType::H4,
            NodeType::H5,
            NodeType::H6,
            NodeType::Ul,
            NodeType::Ol,
            NodeType::Li,
            NodeType::Dl,
            NodeType::Dt,
            NodeType::Dd,
            NodeType::Menu,
            NodeType::MenuItem,
            NodeType::Table,
            NodeType::Caption,
            NodeType::THead,
            NodeType::TBody,
            NodeType::TFoot,
            NodeType::Tr,
            NodeType::Th,
            NodeType::Td,
            NodeType::Form,
            NodeType::FieldSet,
            NodeType::Legend,
            NodeType::Label,
            NodeType::Input,
            NodeType::Button,
            NodeType::Select,
            NodeType::SelectOption,
            NodeType::TextArea,
            NodeType::Output,
            NodeType::Progress,
            NodeType::Meter,
            NodeType::DataList,
            NodeType::A,
            NodeType::Canvas,
            NodeType::Audio,
            NodeType::Video,
            NodeType::Svg,
            NodeType::Object,
            NodeType::Embed,
            NodeType::Script,
        ];
        for node_type in node_types {
            let role = A11yManager::node_type_to_role(&node_type);
            assert!(
                role != Role::GenericContainer && role != Role::TextRun,
                "{node_type:?} -> {role:?} would be skipped by VoiceOver's common_filter"
            );
        }
    }

    /// `Col`/`ColGroup` do map to the filtered `GenericContainer` role, against the
    /// function's own doc comment. Harmless today only because `update_tree` drops
    /// both node types before they can reach the tree — pinned so that stays true.
    #[test]
    fn col_and_colgroup_map_to_the_filtered_generic_container_role() {
        assert_eq!(
            A11yManager::node_type_to_role(&NodeType::Col),
            Role::GenericContainer
        );
        assert_eq!(
            A11yManager::node_type_to_role(&NodeType::ColGroup),
            Role::GenericContainer
        );
    }

    // ---------------------------------------------------------------------
    // map_role
    // ---------------------------------------------------------------------

    #[test]
    fn map_role_is_total_and_matches_the_documented_table() {
        let cases: [(AccessibilityRole, Role); 65] = [
            (AccessibilityRole::TitleBar, Role::TitleBar),
            (AccessibilityRole::MenuBar, Role::MenuBar),
            (AccessibilityRole::ScrollBar, Role::ScrollBar),
            (AccessibilityRole::Grip, Role::Splitter),
            (AccessibilityRole::Sound, Role::Audio),
            (AccessibilityRole::Cursor, Role::Caret),
            (AccessibilityRole::Caret, Role::Caret),
            (AccessibilityRole::Alert, Role::Alert),
            (AccessibilityRole::Window, Role::Window),
            (AccessibilityRole::Client, Role::GenericContainer),
            (AccessibilityRole::MenuPopup, Role::Menu),
            (AccessibilityRole::MenuItem, Role::MenuItem),
            (AccessibilityRole::Tooltip, Role::Tooltip),
            (AccessibilityRole::Application, Role::Application),
            (AccessibilityRole::Document, Role::Document),
            (AccessibilityRole::Pane, Role::Pane),
            (AccessibilityRole::Chart, Role::Figure),
            (AccessibilityRole::Dialog, Role::Dialog),
            (AccessibilityRole::Border, Role::GenericContainer),
            (AccessibilityRole::Grouping, Role::Group),
            (AccessibilityRole::Separator, Role::GenericContainer),
            (AccessibilityRole::Toolbar, Role::Toolbar),
            (AccessibilityRole::StatusBar, Role::Status),
            (AccessibilityRole::Table, Role::Table),
            (AccessibilityRole::ColumnHeader, Role::ColumnHeader),
            (AccessibilityRole::RowHeader, Role::RowHeader),
            (AccessibilityRole::Column, Role::GenericContainer),
            (AccessibilityRole::Row, Role::Row),
            (AccessibilityRole::Cell, Role::Cell),
            (AccessibilityRole::Link, Role::Link),
            (AccessibilityRole::HelpBalloon, Role::Tooltip),
            (AccessibilityRole::Character, Role::GenericContainer),
            (AccessibilityRole::List, Role::List),
            (AccessibilityRole::ListItem, Role::ListItem),
            (AccessibilityRole::Outline, Role::Tree),
            (AccessibilityRole::OutlineItem, Role::TreeItem),
            (AccessibilityRole::PageTab, Role::Tab),
            (AccessibilityRole::PropertyPage, Role::TabPanel),
            (AccessibilityRole::Indicator, Role::Meter),
            (AccessibilityRole::Graphic, Role::Image),
            (AccessibilityRole::StaticText, Role::Label),
            (AccessibilityRole::Text, Role::TextInput),
            (AccessibilityRole::PushButton, Role::Button),
            (AccessibilityRole::CheckButton, Role::CheckBox),
            (AccessibilityRole::RadioButton, Role::RadioButton),
            (AccessibilityRole::ComboBox, Role::ComboBox),
            (AccessibilityRole::DropList, Role::ListBox),
            (AccessibilityRole::ProgressBar, Role::ProgressIndicator),
            (AccessibilityRole::Dial, Role::Meter),
            (AccessibilityRole::HotkeyField, Role::TextInput),
            (AccessibilityRole::Slider, Role::Slider),
            (AccessibilityRole::SpinButton, Role::SpinButton),
            (AccessibilityRole::Diagram, Role::Figure),
            (AccessibilityRole::Animation, Role::GenericContainer),
            (AccessibilityRole::Equation, Role::Math),
            (AccessibilityRole::ButtonDropdown, Role::Button),
            (AccessibilityRole::ButtonMenu, Role::Button),
            (AccessibilityRole::ButtonDropdownGrid, Role::Button),
            (AccessibilityRole::Whitespace, Role::GenericContainer),
            (AccessibilityRole::PageTabList, Role::TabList),
            (AccessibilityRole::Clock, Role::Timer),
            (AccessibilityRole::SplitButton, Role::Button),
            (AccessibilityRole::IpAddress, Role::TextInput),
            (AccessibilityRole::Unknown, Role::Unknown),
            (AccessibilityRole::Nothing, Role::GenericContainer),
        ];
        for (role, expected) in cases {
            assert_eq!(A11yManager::map_role(&role), expected, "{role:?}");
        }
    }

    // ---------------------------------------------------------------------
    // map_action_to_accesskit / map_accesskit_action (round-trip)
    // ---------------------------------------------------------------------

    /// Every payload-free action must survive outbound -> inbound unchanged:
    /// the tree declares `map_action_to_accesskit(a)`, the screen reader sends
    /// that action back, and `map_accesskit_action` must hand back exactly `a`.
    #[test]
    fn payload_free_actions_round_trip_through_accesskit() {
        let actions = [
            AccessibilityAction::Default,
            AccessibilityAction::Focus,
            AccessibilityAction::Blur,
            AccessibilityAction::Collapse,
            AccessibilityAction::Expand,
            AccessibilityAction::ScrollIntoView,
            AccessibilityAction::Increment,
            AccessibilityAction::Decrement,
            AccessibilityAction::ShowContextMenu,
            AccessibilityAction::HideTooltip,
            AccessibilityAction::ShowTooltip,
            AccessibilityAction::ScrollUp,
            AccessibilityAction::ScrollDown,
            AccessibilityAction::ScrollLeft,
            AccessibilityAction::ScrollRight,
            AccessibilityAction::SetSequentialFocusNavigationStartingPoint,
        ];
        for action in actions {
            let outbound = A11yManager::map_action_to_accesskit(&action);
            let inbound = map_accesskit_action(request(outbound, None));
            assert_eq!(
                inbound,
                Some(action.clone()),
                "{action:?} did not survive the outbound/inbound round-trip"
            );
        }
    }

    #[test]
    fn payload_carrying_actions_map_to_their_action_kind() {
        let cases = [
            (
                AccessibilityAction::ReplaceSelectedText(AzString::from("x")),
                Action::ReplaceSelectedText,
            ),
            (
                AccessibilityAction::ScrollToPoint(LogicalPosition::new(f32::NAN, 0.0)),
                Action::ScrollToPoint,
            ),
            (
                AccessibilityAction::SetScrollOffset(LogicalPosition::new(-1.0, f32::INFINITY)),
                Action::SetScrollOffset,
            ),
            (
                AccessibilityAction::SetTextSelection(TextSelectionStartEnd {
                    selection_start: usize::MAX,
                    selection_end: 0,
                }),
                Action::SetTextSelection,
            ),
            (
                AccessibilityAction::SetValue(AzString::from("")),
                Action::SetValue,
            ),
            (
                AccessibilityAction::SetNumericValue(FloatValue::new(f32::NAN)),
                Action::SetValue,
            ),
            (
                AccessibilityAction::CustomAction(i32::MIN),
                Action::CustomAction,
            ),
        ];
        for (action, expected) in cases {
            assert_eq!(
                A11yManager::map_action_to_accesskit(&action),
                expected,
                "{action:?}"
            );
        }
    }

    #[test]
    fn data_requiring_actions_return_none_when_data_is_missing() {
        for action in [
            Action::ReplaceSelectedText,
            Action::ScrollToPoint,
            Action::SetScrollOffset,
            Action::SetTextSelection,
            Action::SetValue,
            Action::CustomAction,
        ] {
            assert_eq!(
                map_accesskit_action(request(action, None)),
                None,
                "{action:?} must reject a request with no payload"
            );
        }
    }

    #[test]
    fn data_requiring_actions_return_none_on_mismatched_payloads() {
        let mismatches = [
            (Action::ReplaceSelectedText, ActionData::NumericValue(1.0)),
            (Action::ScrollToPoint, ActionData::NumericValue(1.0)),
            (
                Action::SetScrollOffset,
                ActionData::Value(Box::from("not a point")),
            ),
            (Action::SetTextSelection, ActionData::CustomAction(3)),
            (Action::SetValue, ActionData::CustomAction(3)),
            (Action::CustomAction, ActionData::NumericValue(1.0)),
        ];
        for (action, data) in mismatches {
            assert_eq!(
                map_accesskit_action(request(action, Some(data.clone()))),
                None,
                "{action:?} must reject payload {data:?}"
            );
        }
    }

    #[test]
    fn payload_free_actions_ignore_an_unexpected_payload() {
        // ScrollIntoView takes an *optional* hint; a stray payload must not
        // turn the request into a no-op.
        assert_eq!(
            map_accesskit_action(request(
                Action::ScrollIntoView,
                Some(ActionData::NumericValue(1.0))
            )),
            Some(AccessibilityAction::ScrollIntoView)
        );
    }

    #[test]
    fn set_value_accepts_unicode_and_empty_strings() {
        for s in ["", "héllo 🎉", "a\0b", "\u{202e}rtl"] {
            let got = map_accesskit_action(request(
                Action::SetValue,
                Some(ActionData::Value(Box::from(s))),
            ));
            assert_eq!(got, Some(AccessibilityAction::SetValue(AzString::from(s))));
        }
    }

    #[test]
    fn set_numeric_value_saturates_instead_of_panicking() {
        for v in [
            0.0_f64,
            -0.0,
            f64::NAN,
            f64::INFINITY,
            f64::NEG_INFINITY,
            1e300,
            -1e300,
            f64::from(f32::MAX),
        ] {
            let got =
                map_accesskit_action(request(Action::SetValue, Some(ActionData::NumericValue(v))));
            #[allow(clippy::cast_possible_truncation)]
            let expected = AccessibilityAction::SetNumericValue(FloatValue::new(v as f32));
            assert_eq!(got, Some(expected), "NumericValue({v}) must not panic");
        }
    }

    #[test]
    fn scroll_to_point_preserves_nan_and_saturates_out_of_range_f64() {
        let got = map_accesskit_action(request(
            Action::ScrollToPoint,
            Some(ActionData::ScrollToPoint(Point {
                x: f64::NAN,
                y: 1e308,
            })),
        ));
        let Some(AccessibilityAction::ScrollToPoint(p)) = got else {
            panic!("expected ScrollToPoint, got {got:?}");
        };
        assert!(p.x.is_nan(), "NaN must pass through, not trap");
        assert!(
            p.y.is_infinite() && p.y.is_sign_positive(),
            "1e308 must saturate to +inf, got {}",
            p.y
        );
    }

    #[test]
    fn set_scroll_offset_saturates_negative_out_of_range_f64() {
        let got = map_accesskit_action(request(
            Action::SetScrollOffset,
            Some(ActionData::SetScrollOffset(Point {
                x: -1e308,
                y: f64::NEG_INFINITY,
            })),
        ));
        let Some(AccessibilityAction::SetScrollOffset(p)) = got else {
            panic!("expected SetScrollOffset, got {got:?}");
        };
        assert!(p.x.is_infinite() && p.x.is_sign_negative());
        assert!(p.y.is_infinite() && p.y.is_sign_negative());
    }

    #[test]
    fn set_text_selection_passes_through_extreme_character_indices() {
        let got = map_accesskit_action(request(
            Action::SetTextSelection,
            Some(ActionData::SetTextSelection(TextSelection {
                anchor: TextPosition {
                    node: A11yNodeId(1),
                    character_index: usize::MAX,
                },
                focus: TextPosition {
                    node: A11yNodeId(2),
                    character_index: 0,
                },
            })),
        ));
        assert_eq!(
            got,
            Some(AccessibilityAction::SetTextSelection(
                TextSelectionStartEnd {
                    selection_start: usize::MAX,
                    selection_end: 0,
                }
            )),
            "an inverted, out-of-range selection must be forwarded verbatim, not clamped"
        );
    }

    #[test]
    fn custom_action_forwards_extreme_ids() {
        for id in [i32::MIN, -1, 0, i32::MAX] {
            assert_eq!(
                map_accesskit_action(request(
                    Action::CustomAction,
                    Some(ActionData::CustomAction(id))
                )),
                Some(AccessibilityAction::CustomAction(id))
            );
        }
    }

    // ---------------------------------------------------------------------
    // build_node — bounds arithmetic (numeric)
    // ---------------------------------------------------------------------

    #[test]
    fn build_node_bounds_are_padding_inset_and_hidpi_scaled() {
        let node_data = NodeData::create_node(NodeType::Div);
        // padding 5px top/bottom, 2px left/right (packed as tenths of a px).
        let layout_node = hot(
            Some(LogicalSize::new(100.0, 50.0)),
            [50, 20, 50, 20],
            [0; 4],
        );
        let node = A11yManager::build_node(
            &node_data,
            &layout_node,
            Some(LogicalPosition::new(10.0, 20.0)),
            None,
            2.0,
            LogicalSize::new(1000.0, 1000.0),
        );
        let bounds = node.bounds().expect("in-viewport node must have bounds");
        assert_eq!(bounds.x0, 24.0); // (10 + 2) * 2
        assert_eq!(bounds.y0, 50.0); // (20 + 5) * 2
        assert_eq!(bounds.x1, 216.0); // (10 + 100 - 2) * 2
        assert_eq!(bounds.y1, 130.0); // (20 + 50 - 5) * 2
    }

    #[test]
    fn build_node_clips_bounds_to_the_window_viewport() {
        let node_data = NodeData::create_node(NodeType::Div);
        let layout_node = hot(Some(LogicalSize::new(10_000.0, 10_000.0)), [0; 4], [0; 4]);
        let node = A11yManager::build_node(
            &node_data,
            &layout_node,
            Some(LogicalPosition::new(-500.0, -500.0)),
            None,
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        let bounds = node.bounds().expect("clipped node still has bounds");
        assert_eq!(
            (bounds.x0, bounds.y0),
            (0.0, 0.0),
            "off-screen origin clamps to 0"
        );
        assert_eq!(
            (bounds.x1, bounds.y1),
            (800.0, 600.0),
            "overflowing extent clamps to the viewport"
        );
    }

    #[test]
    fn build_node_omits_bounds_when_padding_exceeds_the_used_size() {
        // Degenerate box: x1 <= x0, so accesskit must not be handed an inverted rect.
        let node_data = NodeData::create_node(NodeType::Div);
        let layout_node = hot(
            Some(LogicalSize::new(10.0, 10.0)),
            [500, 500, 500, 500],
            [0; 4],
        );
        let node = A11yManager::build_node(
            &node_data,
            &layout_node,
            Some(LogicalPosition::new(0.0, 0.0)),
            None,
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(node.bounds(), None);
    }

    #[test]
    fn build_node_omits_bounds_for_zero_and_nan_hidpi() {
        let node_data = NodeData::create_node(NodeType::Div);
        let layout_node = plain_hot();
        for hidpi in [0.0_f32, f32::NAN, -2.0] {
            let node = A11yManager::build_node(
                &node_data,
                &layout_node,
                Some(LogicalPosition::new(10.0, 20.0)),
                None,
                hidpi,
                LogicalSize::new(800.0, 600.0),
            );
            assert_eq!(
                node.bounds(),
                None,
                "hidpi {hidpi} collapses the rect; no bounds must be set"
            );
        }
    }

    #[test]
    fn build_node_never_emits_an_inverted_rect_for_hostile_geometry() {
        // The `x1 > x0 && y1 > y0` guard is the only thing between accesskit and
        // an inverted/degenerate rect. Sweep the nastiest float inputs at it.
        let node_data = NodeData::create_node(NodeType::Div);
        let sizes = [
            LogicalSize::new(0.0, 0.0),
            LogicalSize::new(f32::MAX, f32::MAX),
            LogicalSize::new(-100.0, -100.0),
            LogicalSize::new(f32::NAN, f32::NAN),
            LogicalSize::new(f32::INFINITY, f32::INFINITY),
        ];
        let positions = [
            LogicalPosition::new(0.0, 0.0),
            LogicalPosition::new(-f32::MAX, -f32::MAX),
            LogicalPosition::new(f32::NAN, 0.0),
            LogicalPosition::new(f32::INFINITY, f32::NEG_INFINITY),
        ];
        let windows = [
            LogicalSize::new(0.0, 0.0),
            LogicalSize::new(800.0, 600.0),
            LogicalSize::new(f32::MAX, f32::MAX),
            LogicalSize::new(f32::NAN, f32::NAN),
        ];
        for size in sizes {
            for pos in positions {
                for window in windows {
                    for hidpi in [1.0_f32, 0.5, 3.0, f32::INFINITY] {
                        let layout_node = hot(Some(size), [i16::MAX; 4], [i16::MIN; 4]);
                        let node = A11yManager::build_node(
                            &node_data,
                            &layout_node,
                            Some(pos),
                            None,
                            hidpi,
                            window,
                        );
                        if let Some(b) = node.bounds() {
                            assert!(
                                b.x1 > b.x0 && b.y1 > b.y0,
                                "inverted rect {b:?} for size={size:?} pos={pos:?} \
                                 window={window:?} hidpi={hidpi}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn build_node_omits_bounds_when_layout_info_is_missing() {
        let node_data = NodeData::create_node(NodeType::Div);
        // No absolute position.
        let node = A11yManager::build_node(
            &node_data,
            &plain_hot(),
            None,
            None,
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(node.bounds(), None);

        // No used size.
        let node = A11yManager::build_node(
            &node_data,
            &hot(None, [0; 4], [0; 4]),
            Some(LogicalPosition::new(0.0, 0.0)),
            None,
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(node.bounds(), None);
    }

    // ---------------------------------------------------------------------
    // build_node — roles, labels, states, actions (invariants)
    // ---------------------------------------------------------------------

    #[test]
    fn build_node_always_declares_scroll_into_view() {
        let node = A11yManager::build_node(
            &NodeData::create_node(NodeType::Div),
            &plain_hot(),
            None,
            None,
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert!(node.supports_action(Action::ScrollIntoView));
    }

    #[test]
    fn build_node_sets_the_html_tag() {
        let node = A11yManager::build_node(
            &NodeData::create_node(NodeType::Div),
            &plain_hot(),
            None,
            None,
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        let expected = NodeType::Div.get_path().to_string();
        assert_eq!(node.html_tag(), Some(expected.as_str()));
    }

    #[test]
    fn build_node_contenteditable_wins_over_a11y_role_and_gains_focus() {
        let mut node_data = NodeData::create_node(NodeType::Div);
        node_data.set_contenteditable(true);
        // Even an explicit (conflicting) role must not override editability.
        node_data.set_accessibility_info(info(AccessibilityRole::PushButton));
        let node = A11yManager::build_node(
            &node_data,
            &plain_hot(),
            None,
            node_data.get_accessibility_info(),
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(node.role(), Role::MultilineTextInput);
        assert!(node.supports_action(Action::Focus));
    }

    #[test]
    fn build_node_labels_text_nodes_including_unicode_and_empty() {
        for s in ["", "héllo 🎉", "a\u{202e}b"] {
            let node_data = text_node(s);
            let node = A11yManager::build_node(
                &node_data,
                &plain_hot(),
                None,
                None,
                1.0,
                LogicalSize::new(800.0, 600.0),
            );
            assert_eq!(node.role(), Role::Label);
            assert_eq!(node.label(), Some(s));
        }
    }

    #[test]
    fn build_node_sets_heading_levels_one_through_six() {
        let expected = [
            (NodeType::H1, 1),
            (NodeType::H2, 2),
            (NodeType::H3, 3),
            (NodeType::H4, 4),
            (NodeType::H5, 5),
            (NodeType::H6, 6),
        ];
        for (node_type, level) in expected {
            let node_data = NodeData::create_node(node_type);
            let node = A11yManager::build_node(
                &node_data,
                &plain_hot(),
                None,
                None,
                1.0,
                LogicalSize::new(800.0, 600.0),
            );
            assert_eq!(node.role(), Role::Heading);
            assert_eq!(node.level(), Some(level));
        }
    }

    #[test]
    fn build_node_maps_every_handled_accessibility_state() {
        let states = [
            AccessibilityState::Unavailable,
            AccessibilityState::Readonly,
            AccessibilityState::CheckedTrue,
            AccessibilityState::Expanded,
            AccessibilityState::Selected,
            AccessibilityState::Busy,
            AccessibilityState::Offscreen,
            AccessibilityState::Focusable,
        ];
        let mut a11y = info(AccessibilityRole::CheckButton);
        a11y.states = states.to_vec().into();
        let node_data = NodeData::create_node(NodeType::Div);
        let node = A11yManager::build_node(
            &node_data,
            &plain_hot(),
            None,
            Some(&a11y),
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(node.role(), Role::CheckBox);
        assert!(node.is_disabled());
        assert!(node.is_read_only());
        assert_eq!(node.toggled(), Some(Toggled::True));
        assert_eq!(node.is_expanded(), Some(true));
        assert_eq!(node.is_selected(), Some(true));
        assert!(node.is_busy());
        assert!(node.is_hidden());
        assert!(node.supports_action(Action::Focus));
    }

    #[test]
    fn build_node_collapsed_and_checked_false_are_distinct_from_absent() {
        let mut a11y = info(AccessibilityRole::CheckButton);
        a11y.states = vec![
            AccessibilityState::Collapsed,
            AccessibilityState::CheckedFalse,
        ]
        .into();
        let node = A11yManager::build_node(
            &NodeData::create_node(NodeType::Div),
            &plain_hot(),
            None,
            Some(&a11y),
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(node.is_expanded(), Some(false));
        assert_eq!(node.toggled(), Some(Toggled::False));
    }

    #[test]
    fn build_node_declares_api_supplied_supported_actions() {
        let mut a11y = info(AccessibilityRole::Slider);
        a11y.supported_actions = vec![
            AccessibilityAction::Increment,
            AccessibilityAction::Decrement,
            AccessibilityAction::SetValue(AzString::from("x")),
            AccessibilityAction::CustomAction(1),
        ]
        .into();
        let node = A11yManager::build_node(
            &NodeData::create_node(NodeType::Div),
            &plain_hot(),
            None,
            Some(&a11y),
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert!(node.supports_action(Action::Increment));
        assert!(node.supports_action(Action::Decrement));
        assert!(node.supports_action(Action::SetValue));
        assert!(node.supports_action(Action::CustomAction));
    }

    #[test]
    fn build_node_relations_resolve_to_walk_emitted_ids() {
        let mut a11y = info(AccessibilityRole::Text);
        a11y.labelled_by = OptionDomNodeId::Some(dom_node(2, 3));
        a11y.described_by = OptionDomNodeId::Some(dom_node(0, 0));
        a11y.is_live_region = true;
        let node = A11yManager::build_node(
            &NodeData::create_node(NodeType::Div),
            &plain_hot(),
            None,
            Some(&a11y),
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(
            node.labelled_by(),
            &[A11yManager::encode_a11y_node_id(2, 3)]
        );
        assert_eq!(
            node.described_by(),
            &[A11yManager::encode_a11y_node_id(0, 0)]
        );
        assert_eq!(node.live(), Some(Live::Polite));
    }

    #[test]
    fn build_node_drops_relations_pointing_at_the_none_sentinel() {
        let mut a11y = info(AccessibilityRole::Text);
        let unresolvable = DomNodeId {
            dom: DomId { inner: 0 },
            node: NodeHierarchyItemId::NONE,
        };
        a11y.labelled_by = OptionDomNodeId::Some(unresolvable);
        a11y.described_by = OptionDomNodeId::Some(unresolvable);
        let node = A11yManager::build_node(
            &NodeData::create_node(NodeType::Div),
            &plain_hot(),
            None,
            Some(&a11y),
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert!(
            node.labelled_by().is_empty() && node.described_by().is_empty(),
            "an unresolvable relation must be dropped, not encoded as a bogus id"
        );
    }

    // ---------------------------------------------------------------------
    // build_node — HTML attributes
    // ---------------------------------------------------------------------

    #[test]
    fn build_node_parses_aria_live_case_insensitively() {
        for (value, expected) in [
            ("polite", Live::Polite),
            ("POLITE", Live::Polite),
            ("assertive", Live::Assertive),
            ("AsSeRtIvE", Live::Assertive),
            ("off", Live::Off),
            ("", Live::Off),
            ("banana", Live::Off),
            ("🎉", Live::Off),
        ] {
            let mut node_data = NodeData::create_node(NodeType::Div);
            node_data.set_attributes(
                vec![AttributeType::AriaProperty(AttributeNameValue {
                    attr_name: AzString::from("ARIA-LIVE"),
                    value: AzString::from(value),
                })]
                .into(),
            );
            let node = A11yManager::build_node(
                &node_data,
                &plain_hot(),
                None,
                None,
                1.0,
                LogicalSize::new(800.0, 600.0),
            );
            assert_eq!(node.live(), Some(expected), "aria-live={value:?}");
        }
    }

    #[test]
    fn build_node_wires_up_html_attributes() {
        let mut node_data = NodeData::create_node(NodeType::Input);
        node_data.set_attributes(
            vec![
                AttributeType::AriaLabel(AzString::from("label")),
                AttributeType::Title(AzString::from("desc")),
                AttributeType::Placeholder(AzString::from("hint")),
                AttributeType::Value(AzString::from("val")),
                AttributeType::Disabled,
                AttributeType::Readonly,
                AttributeType::Required,
                AttributeType::Hidden,
                AttributeType::CheckedTrue,
                AttributeType::Lang(AzString::from("de")),
                AttributeType::ColSpan(2),
                AttributeType::RowSpan(3),
            ]
            .into(),
        );
        let node = A11yManager::build_node(
            &node_data,
            &plain_hot(),
            None,
            None,
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(node.label(), Some("label"));
        assert_eq!(node.description(), Some("desc"));
        assert_eq!(node.placeholder(), Some("hint"));
        assert_eq!(node.value(), Some("val"));
        assert!(node.is_disabled());
        assert!(node.is_read_only());
        assert!(node.is_required());
        assert!(node.is_hidden());
        assert_eq!(node.toggled(), Some(Toggled::True));
        assert_eq!(node.language(), Some("de"));
        assert_eq!(node.column_span(), Some(2));
        assert_eq!(node.row_span(), Some(3));
    }

    /// `colspan`/`rowspan` are `i32` in the DOM but `usize` in accesskit, and the
    /// conversion is an unchecked `as` cast. A negative span (HTML lets you write
    /// `colspan="-1"`) sign-extends into an astronomically large span instead of
    /// being rejected or clamped. Pinned here: no panic, but the value is garbage.
    #[test]
    fn build_node_negative_col_and_row_span_sign_extend_to_usize_max() {
        let mut node_data = NodeData::create_node(NodeType::Td);
        node_data
            .set_attributes(vec![AttributeType::ColSpan(-1), AttributeType::RowSpan(-1)].into());
        let node = A11yManager::build_node(
            &node_data,
            &plain_hot(),
            None,
            None,
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(node.column_span(), Some(usize::MAX));
        assert_eq!(node.row_span(), Some(usize::MAX));
    }

    #[test]
    fn build_node_zero_span_is_forwarded_unchanged() {
        let mut node_data = NodeData::create_node(NodeType::Td);
        node_data.set_attributes(vec![AttributeType::ColSpan(0)].into());
        let node = A11yManager::build_node(
            &node_data,
            &plain_hot(),
            None,
            None,
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(node.column_span(), Some(0));
    }

    #[test]
    fn build_node_dom_attributes_override_accessibility_info() {
        // Documented priority: explicit a11y info < DOM attributes.
        let mut a11y = info(AccessibilityRole::PushButton);
        a11y.accessibility_name = OptionString::Some(AzString::from("from-info"));
        a11y.accessibility_value = OptionString::Some(AzString::from("info-value"));
        let mut node_data = NodeData::create_node(NodeType::Button);
        node_data.set_attributes(
            vec![
                AttributeType::AriaLabel(AzString::from("from-attr")),
                AttributeType::Value(AzString::from("attr-value")),
            ]
            .into(),
        );
        let node = A11yManager::build_node(
            &node_data,
            &plain_hot(),
            None,
            Some(&a11y),
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(node.label(), Some("from-attr"));
        assert_eq!(node.value(), Some("attr-value"));
    }

    // ---------------------------------------------------------------------
    // update_tree (numeric / no-panic)
    // ---------------------------------------------------------------------

    #[test]
    fn update_tree_with_no_doms_emits_only_the_root_window_node() {
        let update = empty_update(LogicalSize::new(800.0, 600.0), None, 1.0, "title");
        assert_eq!(update.nodes.len(), 1);
        assert_eq!(update.nodes[0].0, A11yNodeId(0));
        assert_eq!(update.nodes[0].1.role(), Role::Window);
        assert_eq!(update.nodes[0].1.label(), Some("title"));
        assert!(update.nodes[0].1.children().is_empty());
        assert!(
            update.tree.is_some(),
            "the first update must carry the tree"
        );
        assert_eq!(update.tree_id, TreeId::ROOT);
        assert_eq!(
            update.focus,
            A11yNodeId(0),
            "with no content nodes, focus must fall back to the root"
        );
    }

    // ---------------------------------------------------------------------
    // Naming a control must not DELETE it (the AzWidgets a11y regression)
    // ---------------------------------------------------------------------

    #[test]
    fn naming_a_node_keeps_the_role_its_element_type_implies() {
        // The one-field form the engine's own lint recommends. It produces
        // `AccessibilityInfo { name: Some(..), role: Unknown, .. }`, and
        // `Unknown` is `Default`'s "not specified" — so the element's own kind
        // must survive. It used to be replaced by `Role::Unknown`, which
        // VoiceOver skips exactly the way it skips `GenericContainer`: naming a
        // control removed it from the tree.
        for (node_type, expected) in [
            (NodeType::Button, Role::Button),
            (NodeType::Div, Role::Group),
            (NodeType::A, Role::Link),
            (NodeType::Input, Role::TextInput),
            (NodeType::H1, Role::Heading),
        ] {
            let a11y = AccessibilityInfo {
                accessibility_name: OptionString::Some(AzString::from("Accent colour")),
                ..Default::default()
            };
            assert_eq!(
                a11y.role,
                AccessibilityRole::Unknown,
                "the fixture must exercise the unspecified-role case"
            );

            let node = A11yManager::build_node(
                &NodeData::create_node(node_type.clone()),
                &plain_hot(),
                None,
                Some(&a11y),
                1.0,
                LogicalSize::new(800.0, 600.0),
            );
            assert_eq!(
                node.role(),
                expected,
                "{node_type:?} lost its role to a name-only declaration"
            );
            assert_eq!(
                node.label(),
                Some("Accent colour"),
                "the name must survive too"
            );
        }
    }

    #[test]
    fn a_declared_role_still_outranks_the_element_type() {
        // The other half of the contract: an explicit role is authoritative.
        let mut a11y = info(AccessibilityRole::Slider);
        a11y.accessibility_name = OptionString::Some(AzString::from("Volume"));
        let node = A11yManager::build_node(
            &NodeData::create_node(NodeType::Div),
            &plain_hot(),
            None,
            Some(&a11y),
            1.0,
            LogicalSize::new(800.0, 600.0),
        );
        assert_eq!(node.role(), Role::Slider);
    }

    #[test]
    fn no_named_node_in_a_widget_showcase_tree_is_announced_as_unknown() {
        // Whole-tree statement of the same bug, over the shape AzWidgets
        // actually builds: a labelled section whose control carries a name and
        // nothing else. `Role::Unknown` must not appear anywhere.
        use azul_core::dom::Dom;

        let labelled = |caption: &str, control: Dom| {
            Dom::create_div()
                .with_child(Dom::create_span_with_text(caption))
                // Exactly what `examples/azul-widgets`' `labelled()` helper
                // does: the caption a sighted user reads IS the control's name.
                .with_child(control.with_accessibility_name(caption))
        };
        let body = Dom::create_body().with_child(
            Dom::create_div()
                .with_child(labelled("Accent colour", Dom::create_div()))
                .with_child(labelled("Volume", Dom::create_div()))
                .with_child(labelled("Subscribe", Dom::create_div())),
        );

        let update = update_over_doms(vec![body]);
        let unknown: Vec<_> = update
            .nodes
            .iter()
            .filter(|(_, n)| n.role() == Role::Unknown)
            .map(|(id, n)| (*id, n.label()))
            .collect();
        assert!(
            unknown.is_empty(),
            "named controls fell out of the tree as Role::Unknown: {unknown:?}"
        );
    }

    #[test]
    fn the_ios_android_snapshot_resolves_roles_the_same_way_the_accesskit_tree_does() {
        // `A11ySnapshot` is the platform-neutral twin for the two shells
        // accesskit has no backend for. It carried the identical defect, and a
        // second copy of a rule is how the two drift apart — both now go
        // through `accessibility_role_is_specified`.
        use azul_core::{dom::Dom, styled_dom::StyledDom};

        use crate::managers::{a11y_snapshot::A11ySnapshot, gpu_state::GpuStateManager};

        let mut layout_results = BTreeMap::new();
        layout_results.insert(
            DomId { inner: 0 },
            layout_result_of(StyledDom::create_from_dom(
                Dom::create_body().with_child(
                    Dom::create_button(
                        "Subscribe",
                        azul_core::a11y::SmallAriaInfo::label("Subscribe"),
                    )
                    .with_accessibility_name("Subscribe"),
                ),
            )),
        );
        let gpu = GpuStateManager::new(
            azul_core::task::Duration::from_millis(0),
            azul_core::task::Duration::from_millis(0),
        );
        let snapshot = A11ySnapshot::build(
            &layout_results,
            &ScrollManager::new(),
            &gpu,
            None,
            "t",
            LogicalSize::new(800.0, 600.0),
        );
        let unknown: Vec<_> = snapshot
            .elements
            .iter()
            .filter(|e| e.role == AccessibilityRole::Unknown)
            .map(|e| e.label.clone())
            .collect();
        assert!(
            unknown.is_empty(),
            "named elements came back with an Unknown role: {unknown:?}"
        );
    }

    #[test]
    fn a_well_formed_multi_dom_tree_passes_through_the_guard_untouched() {
        // The consistency guard exists to neutralise MALFORMED input. On a
        // well-formed one — including the main window merged with a
        // `<transient-window>` child DOM — it must be a NO-OP: every node
        // present, reachable exactly once, nothing re-hung on the root. (The
        // AzWidgets a11y regression was blamed on this guard; it is not the
        // culprit, and this pins that.)
        use azul_core::dom::Dom;

        let page = |name: &str| {
            Dom::create_body()
                .with_child(Dom::create_span_with_text(name))
                .with_child(
                    Dom::create_div()
                        .with_child(Dom::create_span_with_text("one"))
                        .with_child(Dom::create_span_with_text("two")),
                )
        };
        let update = update_over_doms(vec![page("main"), page("popup")]);

        let by_id: HashMap<A11yNodeId, &Node> =
            update.nodes.iter().map(|(id, n)| (*id, n)).collect();
        let root = by_id[&A11yNodeId(0)];
        assert_eq!(
            root.children().len(),
            2,
            "one root child per DOM, and no orphan re-hung alongside them: {:?}",
            root.children()
        );

        let mut reachable = std::collections::HashSet::new();
        let mut stack = vec![A11yNodeId(0)];
        while let Some(id) = stack.pop() {
            assert!(reachable.insert(id), "{id:?} is reachable twice");
            if let Some(n) = by_id.get(&id) {
                stack.extend(n.children().iter().copied());
            }
        }
        assert_eq!(
            reachable.len(),
            update.nodes.len(),
            "the guard dropped nodes from a well-formed tree"
        );
    }

    #[test]
    fn enforce_child_invariants_makes_a_malformed_tree_accesskit_safe() {
        use std::collections::{HashMap, HashSet};

        // Regression for the SIGABRT in accesskit_consumer::tree::State::update
        // (a mouse_up a11y flush): a malformed TreeUpdate ABORTS the process
        // under panic=abort, so the shell's catch_unwind can't save it. This one
        // input carries EVERY accesskit-fatal shape at once — a class-level guard
        // has to neutralise all of them, no matter what the multi-DOM merge emits.
        let root = A11yNodeId(0);
        let node_ids = vec![
            root,
            A11yNodeId(1),
            A11yNodeId(2),
            A11yNodeId(3),
            A11yNodeId(4),
            A11yNodeId(5),
        ];
        //   99 = child that names no present node   -> must be dropped (else tree.rs:75/307-adjacent)
        //   2  = claimed by root AND node 1          -> a node with two parents (tree.rs:225)
        //   3  = claimed by node 1 AND node 2        -> two parents again
        //   4  = its own child                       -> self-cycle
        //   88 = parent that is not a present node   -> its child list must be cleared
        //   5  = only child of missing parent 88     -> orphan; must be re-hung reachable (tree.rs:307)
        let mut root_children = vec![A11yNodeId(1), A11yNodeId(2), A11yNodeId(99)];
        let mut map: HashMap<A11yNodeId, Vec<A11yNodeId>> = HashMap::new();
        map.insert(A11yNodeId(1), vec![A11yNodeId(2), A11yNodeId(3)]);
        map.insert(A11yNodeId(2), vec![A11yNodeId(3)]);
        map.insert(A11yNodeId(4), vec![A11yNodeId(4)]);
        map.insert(A11yNodeId(88), vec![A11yNodeId(5)]);

        A11yManager::enforce_child_invariants(&node_ids, root, &mut root_children, &mut map);

        let valid: HashSet<A11yNodeId> = node_ids.iter().copied().collect();
        let mut all_children: Vec<A11yNodeId> = root_children.clone();
        for children in map.values() {
            all_children.extend(children.iter().copied());
        }

        // 1. No child names a node absent from the update.
        for c in &all_children {
            assert!(valid.contains(c), "dangling child {c:?} survived the guard");
        }
        // 2. No node is a child of two parents (accesskit's GLOBAL duplicate).
        let mut seen = HashSet::new();
        for c in &all_children {
            assert!(seen.insert(*c), "node {c:?} still has two parents");
        }
        // 3. No node is its own child.
        assert!(
            !map.get(&A11yNodeId(4))
                .is_some_and(|ch| ch.contains(&A11yNodeId(4))),
            "self-parent survived the guard"
        );
        // 4. Every present non-root node is reachable exactly once (no orphan).
        let reachable: HashSet<A11yNodeId> = all_children.iter().copied().collect();
        for id in &node_ids {
            if *id == root {
                continue;
            }
            assert!(
                reachable.contains(id),
                "node {id:?} is an orphan (unreachable from root)"
            );
        }
    }

    #[test]
    fn update_tree_survives_degenerate_window_sizes_and_hidpi() {
        let sizes = [
            LogicalSize::new(0.0, 0.0),
            LogicalSize::new(-1.0, -1.0),
            LogicalSize::new(f32::MAX, f32::MAX),
            LogicalSize::new(f32::NAN, f32::NAN),
            LogicalSize::new(f32::INFINITY, f32::NEG_INFINITY),
        ];
        for size in sizes {
            for hidpi in [0.0_f32, 1.0, -1.0, f32::NAN, f32::INFINITY, f32::MIN] {
                let update = empty_update(size, None, hidpi, "t");
                assert_eq!(update.nodes.len(), 1, "size={size:?} hidpi={hidpi}");
                assert_eq!(update.focus, A11yNodeId(0));
            }
        }
    }

    #[test]
    fn update_tree_focus_falls_back_to_root_for_an_unresolvable_focused_node() {
        // A focused node in a DOM that isn't in layout_results at all.
        let update = empty_update(
            LogicalSize::new(800.0, 600.0),
            Some(dom_node(9, 42)),
            1.0,
            "t",
        );
        assert_eq!(update.focus, A11yNodeId(0));

        // A focused node whose NodeHierarchyItemId is the None sentinel.
        let update = empty_update(
            LogicalSize::new(800.0, 600.0),
            Some(DomNodeId {
                dom: DomId { inner: 0 },
                node: NodeHierarchyItemId::NONE,
            }),
            1.0,
            "t",
        );
        assert_eq!(update.focus, A11yNodeId(0));
    }

    #[test]
    fn update_tree_preserves_unicode_and_empty_window_titles() {
        for title in ["", "Ünïcødé 🪟", "a\u{202e}b"] {
            let update = empty_update(LogicalSize::new(800.0, 600.0), None, 1.0, title);
            assert_eq!(update.nodes[0].1.label(), Some(title));
        }
    }

    #[test]
    fn update_tree_honours_a_non_zero_root_id() {
        let layout_results = BTreeMap::new();
        let scroll_manager = ScrollManager::new();
        let overrides = BTreeMap::new();
        let root = A11yNodeId(999);
        let update = A11yManager::update_tree(
            root,
            &layout_results,
            &scroll_manager,
            &AzString::from("t"),
            LogicalSize::new(800.0, 600.0),
            None,
            1.0,
            &overrides,
            None,
        );
        assert_eq!(update.nodes[0].0, root);
        assert_eq!(update.focus, root);
        assert_eq!(
            update.tree.map(|t| t.root),
            Some(root),
            "the declared tree root must match the emitted root node"
        );
    }
}
