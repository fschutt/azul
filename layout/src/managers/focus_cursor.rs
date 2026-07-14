//! Focus and tab navigation management.
//!
//! Manages keyboard focus, tab navigation, and programmatic focus changes
//! with a recursive event system for focus/blur callbacks (max depth: 5).

use alloc::collections::BTreeMap;

use azul_core::{
    callbacks::{FocusTarget, FocusTargetPath},
    dom::{DomId, DomNodeId, NodeId},
    style::matches_html_element,
    styled_dom::NodeHierarchyItemId,
    window::UpdateFocusWarning,
};

use crate::window::DomLayoutResult;

/// Information about a pending contenteditable focus that needs cursor initialization
/// after layout is complete (W3C "flag and defer" pattern).
///
/// This is set during focus event handling and consumed after layout pass.
#[derive(Copy, Debug, Clone, PartialEq, Eq)]
pub struct PendingContentEditableFocus {
    /// The DOM where the contenteditable element is
    pub dom_id: DomId,
    /// The contenteditable container node that received focus
    pub container_node_id: NodeId,
    /// The text node where the cursor should be placed (often a child of the container)
    pub text_node_id: NodeId,
}

/// Manager for keyboard focus and tab navigation
///
/// Note: Text cursor management is now handled by the separate `CursorManager`.
///
/// The `FocusManager` only tracks which node has focus, while `CursorManager`
/// tracks the cursor position within that node (if it's contenteditable).
///
/// ## W3C Focus/Selection Model
///
/// The W3C model maintains a strict separation between **keyboard focus** and **selection**:
///
/// 1. **Focus** lands on the contenteditable container (`document.activeElement`)
/// 2. **Selection/Cursor** is placed in a descendant text node (`Selection.focusNode`)
///
/// This separation requires a "flag and defer" pattern:
/// - During focus event: Set `cursor_needs_initialization = true`
/// - After layout pass: Call `finalize_pending_focus_changes()` to actually initialize the cursor
///
/// This is necessary because cursor positioning requires text layout information,
/// which isn't available during the focus event handling phase.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FocusManager {
    /// Currently focused node (if any)
    pub focused_node: Option<DomNodeId>,
    /// Pending focus request from callback
    pub pending_focus_request: Option<FocusTarget>,
    
    // --- W3C "flag and defer" pattern fields ---
    
    /// Flag indicating that cursor initialization is pending (set during focus, consumed after layout)
    pub cursor_needs_initialization: bool,
    /// Information about the pending contenteditable focus
    pub pending_contenteditable_focus: Option<PendingContentEditableFocus>,
}

impl Default for FocusManager {
    fn default() -> Self {
        Self::new()
    }
}

impl FocusManager {
    /// Create a new focus manager
    #[must_use] pub const fn new() -> Self {
        Self {
            focused_node: None,
            pending_focus_request: None,
            cursor_needs_initialization: false,
            pending_contenteditable_focus: None,
        }
    }

    /// Get the currently focused node
    #[must_use] pub const fn get_focused_node(&self) -> Option<&DomNodeId> {
        self.focused_node.as_ref()
    }

    /// Set the focused node directly (used by event system)
    ///
    /// Note: Cursor initialization/clearing is now handled by `CursorManager`.
    /// The event system should check if the newly focused node is contenteditable
    /// and call `CursorManager::initialize_cursor_at_end()` if needed.
    pub const fn set_focused_node(&mut self, node: Option<DomNodeId>) {
        self.focused_node = node;
    }

    /// Request a focus change (to be processed by event system)
    pub fn request_focus_change(&mut self, target: FocusTarget) {
        self.pending_focus_request = Some(target);
    }

    /// Take the pending focus request (one-shot)
    pub const fn take_focus_request(&mut self) -> Option<FocusTarget> {
        self.pending_focus_request.take()
    }

    /// Clear focus
    pub const fn clear_focus(&mut self) {
        self.focused_node = None;
    }

    /// Check if a specific node has focus
    #[must_use] pub fn has_focus(&self, node: &DomNodeId) -> bool {
        self.focused_node.as_ref() == Some(node)
    }
    
    // --- W3C "flag and defer" pattern methods ---
    
    /// Mark that cursor initialization is needed for a contenteditable element.
    ///
    /// This is called during focus event handling. The actual cursor initialization
    /// happens later in `finalize_pending_focus_changes()` after layout is complete.
    ///
    /// # W3C Conformance
    ///
    /// In the W3C model, when focus lands on a contenteditable element:
    /// 1. The focus event fires on the container element
    /// 2. The browser's editing engine modifies the Selection to place a caret
    /// 3. The Selection's anchorNode/focusNode point to the child text node
    ///
    /// Since we need layout information to position the cursor, we defer step 2+3.
    pub const fn set_pending_contenteditable_focus(
        &mut self,
        dom_id: DomId,
        container_node_id: NodeId,
        text_node_id: NodeId,
    ) {
        self.cursor_needs_initialization = true;
        self.pending_contenteditable_focus = Some(PendingContentEditableFocus {
            dom_id,
            container_node_id,
            text_node_id,
        });
    }
    
    /// Clear the pending contenteditable focus (when focus moves away or is cleared).
    pub const fn clear_pending_contenteditable_focus(&mut self) {
        self.cursor_needs_initialization = false;
        self.pending_contenteditable_focus = None;
    }
    
    /// Take the pending contenteditable focus (consumes the flag).
    ///
    /// Returns `Some(info)` if cursor initialization is pending, `None` otherwise.
    /// After calling this, `cursor_needs_initialization` is set to `false`.
    pub const fn take_pending_contenteditable_focus(&mut self) -> Option<PendingContentEditableFocus> {
        if self.cursor_needs_initialization {
            self.cursor_needs_initialization = false;
            self.pending_contenteditable_focus.take()
        } else {
            None
        }
    }
    
    /// Check if cursor initialization is pending.
    #[must_use] pub const fn needs_cursor_initialization(&self) -> bool {
        self.cursor_needs_initialization
    }

}

impl crate::managers::NodeIdRemap for FocusManager {
    /// Remap the focused node AND the pending contenteditable focus.
    ///
    /// Focus on an unmounted node is CLEARED (not kept — the index now denotes a
    /// different element).
    fn remap_node_ids(&mut self, dom_id: DomId, map: &crate::managers::NodeIdMap) {
        // 1. currently focused node
        if let Some(focused) = self.focused_node {
            if focused.dom == dom_id {
                match focused
                    .node
                    .into_crate_internal()
                    .and_then(|old| map.resolve(old))
                {
                    Some(new_id) => {
                        self.focused_node = Some(DomNodeId {
                            dom: dom_id,
                            node: NodeHierarchyItemId::from_crate_internal(Some(new_id)),
                        });
                    }
                    None => self.focused_node = None,
                }
            }
        }

        // 2. pending contenteditable focus (set during focus handling, consumed
        //    after layout — a DOM rebuild can land in between).
        if let Some(ref mut pending) = self.pending_contenteditable_focus {
            if pending.dom_id != dom_id {
                return;
            }
            match (
                map.resolve(pending.container_node_id),
                map.resolve(pending.text_node_id),
            ) {
                (Some(container), Some(text)) => {
                    pending.container_node_id = container;
                    pending.text_node_id = text;
                }
                _ => {
                    self.pending_contenteditable_focus = None;
                    self.cursor_needs_initialization = false;
                }
            }
        }
    }
}

/// MWA-C-focus_cursor: W3C sequential focus order over all DOMs.
///
/// Ordering: nodes with a positive `tabindex` (`TabIndex::OverrideInParent(n)`,
/// n >= 1) come first, ascending by n (stable sort, so document order breaks
/// ties); then all remaining keyboard-focusable nodes (`Auto`,
/// `OverrideInParent(0)`, implicit focusables) in document order.
/// `TabIndex::NoKeyboardFocus` (tabindex=-1) nodes stay focusable by click /
/// API but are NEVER part of the Tab order. The previous linear `NodeId` walk
/// both ignored positive-tabindex ordering and tabbed onto tabindex=-1 nodes.
fn collect_tab_order(layout_results: &BTreeMap<DomId, DomLayoutResult>) -> Vec<DomNodeId> {
    use azul_core::dom::TabIndex;
    let mut positive: Vec<(u32, DomNodeId)> = Vec::new();
    let mut auto: Vec<DomNodeId> = Vec::new();
    for (dom_id, layout) in layout_results {
        let node_data = layout.styled_dom.node_data.as_container();
        for index in 0..node_data.len() {
            let node_id = NodeId::new(index);
            let Some(nd) = node_data.get(node_id) else {
                continue;
            };
            if !nd.is_focusable() {
                continue;
            }
            let dom_node = FocusSearchContext::make_dom_node_id(*dom_id, node_id);
            match nd.get_tab_index() {
                Some(TabIndex::NoKeyboardFocus) => {}
                Some(TabIndex::OverrideInParent(n)) if n > 0 => positive.push((n, dom_node)),
                _ => auto.push(dom_node),
            }
        }
    }
    order_tab_entries(positive, auto)
}

/// Pure merge of the two tab-order sections (split out for unit testing).
fn order_tab_entries(
    mut positive: Vec<(u32, DomNodeId)>,
    auto: Vec<DomNodeId>,
) -> Vec<DomNodeId> {
    positive.sort_by_key(|(n, _)| *n); // stable: document order within equal n
    positive.into_iter().map(|(_, id)| id).chain(auto).collect()
}

/// Document-order key for a node (DOM index, then arena index) — used to
/// re-enter the tab order from a node that is not itself tab-focusable.
fn doc_order_key(id: &DomNodeId) -> (usize, usize) {
    (
        id.dom.inner,
        id.node.into_crate_internal().map_or(0, |n| n.index()),
    )
}

/// Pick the next / previous entry in `order` relative to `current`.
///
/// If `current` is a tab stop, steps with wrap-around. If it is not (no focus
/// yet, or focus sits on a tabindex=-1 / removed node), forward picks the
/// first tab stop after it in document order (wrapping to the first entry),
/// backward symmetrically.
fn next_in_tab_order(
    order: &[DomNodeId],
    current: Option<DomNodeId>,
    forward: bool,
) -> Option<DomNodeId> {
    if order.is_empty() {
        return None;
    }
    let Some(cur) = current else {
        return if forward {
            order.first().copied()
        } else {
            order.last().copied()
        };
    };
    if let Some(pos) = order.iter().position(|x| *x == cur) {
        let len = order.len();
        let next = if forward {
            (pos + 1) % len
        } else {
            (pos + len - 1) % len
        };
        return Some(order[next]);
    }
    let cur_key = doc_order_key(&cur);
    let candidate = if forward {
        order
            .iter()
            .filter(|x| doc_order_key(x) > cur_key)
            .min_by_key(|x| doc_order_key(x))
    } else {
        order
            .iter()
            .filter(|x| doc_order_key(x) < cur_key)
            .max_by_key(|x| doc_order_key(x))
    };
    candidate.copied().or_else(|| {
        if forward {
            order.first().copied()
        } else {
            order.last().copied()
        }
    })
}

/// Context for focus-target resolution (`Path` / `Id` lookups).
///
/// MWA-C-focus_cursor: the old linear-walk machinery (`SearchDirection`,
/// `search_focusable_node`, `get_*_start`) was replaced by the W3C tab order
/// built in `collect_tab_order`; only the layout lookup helpers remain.
struct FocusSearchContext<'a> {
    /// Reference to all DOM layouts in the window
    layout_results: &'a BTreeMap<DomId, DomLayoutResult>,
}

impl<'a> FocusSearchContext<'a> {
    /// Create a new search context from layout results.
    const fn new(layout_results: &'a BTreeMap<DomId, DomLayoutResult>) -> Self {
        Self { layout_results }
    }

    /// Get the layout for a DOM ID, or return an error if invalid.
    #[allow(clippy::trivially_copy_pass_by_ref)] // <=8B Copy param kept by-ref intentionally (hot pixel/coord path or to avoid churning call sites for a perf-neutral change)
    fn get_layout(&self, dom_id: &DomId) -> Result<&'a DomLayoutResult, UpdateFocusWarning> {
        self.layout_results
            .get(dom_id)
            .ok_or_else(|| UpdateFocusWarning::FocusInvalidDomId(*dom_id))
    }

    /// Construct a `DomNodeId` from DOM and node IDs.
    const fn make_dom_node_id(dom_id: DomId, node_id: NodeId) -> DomNodeId {
        DomNodeId {
            dom: dom_id,
            node: NodeHierarchyItemId::from_crate_internal(Some(node_id)),
        }
    }
}

/// Find the first focusable node matching a CSS path selector.
///
/// Iterates through all nodes in the DOM in document order (index 0..n),
/// and returns the first node that:
///
/// 1. Matches the CSS path selector
/// 2. Is focusable (has `tabindex` or is naturally focusable)
///
/// # Returns
///
/// * `Ok(Some(node))` - Found a matching focusable node
/// * `Ok(None)` - No matching focusable node exists
/// * `Err(_)` - CSS path could not be matched (malformed selector)
#[allow(clippy::trivially_copy_pass_by_ref)] // <=8B Copy param kept by-ref intentionally (hot pixel/coord path or to avoid churning call sites for a perf-neutral change)
fn find_first_matching_focusable_node(
    layout: &DomLayoutResult,
    dom_id: &DomId,
    css_path: &azul_css::css::CssPath,
) -> Option<DomNodeId> {
    let styled_dom = &layout.styled_dom;
    let node_hierarchy = styled_dom.node_hierarchy.as_container();
    let node_data = styled_dom.node_data.as_container();
    let cascade_info = styled_dom.cascade_info.as_container();

    // Iterate through all nodes in document order
    let matching_node = (0..node_data.len())
        .map(NodeId::new)
        .filter(|&node_id| {
            // Check if node matches the CSS path (no pseudo-selector requirement)
            matches_html_element(
                css_path,
                node_id,
                &node_hierarchy,
                &node_data,
                &cascade_info,
                None, // No expected pseudo-selector ending like :hover/:focus
            )
        })
        .find(|&node_id| {
            // Among matching nodes, find first that is focusable
            node_data[node_id].is_focusable()
        });

    matching_node.map(|node_id| DomNodeId {
        dom: *dom_id,
        node: NodeHierarchyItemId::from_crate_internal(Some(node_id)),
    })
}

/// Resolve a `FocusTarget` to an actual `DomNodeId`
/// # Errors
///
/// Returns an `UpdateFocusWarning` if the focus target cannot be resolved.
pub fn resolve_focus_target(
    focus_target: &FocusTarget,
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    current_focus: Option<DomNodeId>,
) -> Result<Option<DomNodeId>, UpdateFocusWarning> {
    use azul_core::callbacks::FocusTarget::{Path, Id, Previous, Next, First, Last, NoFocus};

    if layout_results.is_empty() {
        return Ok(None);
    }

    let ctx = FocusSearchContext::new(layout_results);

    match focus_target {
        Path(FocusTargetPath { dom, css_path }) => {
            let layout = ctx.get_layout(dom)?;
            Ok(find_first_matching_focusable_node(layout, dom, css_path))
        }

        Id(dom_node_id) => {
            let layout = ctx.get_layout(&dom_node_id.dom)?;
            let is_valid = dom_node_id
                .node
                .into_crate_internal()
                .is_some_and(|n| layout.styled_dom.node_data.as_container().get(n).is_some());

            if is_valid {
                Ok(Some(*dom_node_id))
            } else {
                Err(UpdateFocusWarning::FocusInvalidNodeId(
                    dom_node_id.node,
                ))
            }
        }

        // MWA-C-focus_cursor: sequential navigation goes through the W3C tab
        // order (positive tabindex ascending, then document order; -1
        // excluded) instead of the old raw-NodeId walk.
        Previous => Ok(next_in_tab_order(
            &collect_tab_order(layout_results),
            current_focus,
            false,
        )),

        Next => Ok(next_in_tab_order(
            &collect_tab_order(layout_results),
            current_focus,
            true,
        )),

        First => Ok(collect_tab_order(layout_results).first().copied()),

        Last => Ok(collect_tab_order(layout_results).last().copied()),

        NoFocus => Ok(None),
    }
}

// Trait Implementations for Event Filtering

impl azul_core::events::FocusManagerQuery for FocusManager {
    fn get_focused_node_id(&self) -> Option<DomNodeId> {
        self.focused_node
    }
}

#[cfg(test)]
mod tab_order_tests {
    use super::*;

    fn nid(dom: usize, node: usize) -> DomNodeId {
        FocusSearchContext::make_dom_node_id(DomId { inner: dom }, NodeId::new(node))
    }

    #[test]
    fn positive_tabindex_sorts_first_ascending_then_document_order() {
        // Document order: n3 (tabindex=2), n5 (auto), n7 (tabindex=1), n9 (auto)
        let order = order_tab_entries(
            vec![(2, nid(0, 3)), (1, nid(0, 7))],
            vec![nid(0, 5), nid(0, 9)],
        );
        assert_eq!(order, vec![nid(0, 7), nid(0, 3), nid(0, 5), nid(0, 9)]);
    }

    #[test]
    fn equal_positive_tabindex_keeps_document_order() {
        let order = order_tab_entries(vec![(1, nid(0, 2)), (1, nid(0, 8))], vec![]);
        assert_eq!(order, vec![nid(0, 2), nid(0, 8)]);
    }

    #[test]
    fn next_wraps_and_previous_wraps() {
        let order = vec![nid(0, 1), nid(0, 4), nid(0, 6)];
        assert_eq!(next_in_tab_order(&order, Some(nid(0, 6)), true), Some(nid(0, 1)));
        assert_eq!(next_in_tab_order(&order, Some(nid(0, 1)), false), Some(nid(0, 6)));
        assert_eq!(next_in_tab_order(&order, Some(nid(0, 4)), true), Some(nid(0, 6)));
    }

    #[test]
    fn no_focus_starts_at_ends() {
        let order = vec![nid(0, 1), nid(0, 4)];
        assert_eq!(next_in_tab_order(&order, None, true), Some(nid(0, 1)));
        assert_eq!(next_in_tab_order(&order, None, false), Some(nid(0, 4)));
    }

    #[test]
    fn non_tab_stop_focus_reenters_in_document_order() {
        // Focus sits on a tabindex=-1 node (0,5): Tab goes to the next tab
        // stop in document order (0,6); Shift+Tab to the previous one (0,4).
        let order = vec![nid(0, 1), nid(0, 4), nid(0, 6)];
        assert_eq!(next_in_tab_order(&order, Some(nid(0, 5)), true), Some(nid(0, 6)));
        assert_eq!(next_in_tab_order(&order, Some(nid(0, 5)), false), Some(nid(0, 4)));
        // Past the last stop: wraps to first / last respectively.
        assert_eq!(next_in_tab_order(&order, Some(nid(0, 9)), true), Some(nid(0, 1)));
        assert_eq!(next_in_tab_order(&order, Some(nid(0, 0)), false), Some(nid(0, 6)));
    }

    #[test]
    fn empty_order_yields_none() {
        assert_eq!(next_in_tab_order(&[], Some(nid(0, 1)), true), None);
        assert_eq!(next_in_tab_order(&[], None, false), None);
    }
}

#[cfg(test)]
mod autotest_generated {
    use std::collections::HashMap;

    use azul_core::{
        dom::{Dom, NodeType, TabIndex},
        geom::LogicalRect,
        styled_dom::StyledDom,
    };
    use azul_css::css::{CssPath, CssPathSelector};

    use super::*;
    use crate::{
        managers::{NodeIdMap, NodeIdRemap},
        solver3::{display_list::DisplayList, layout_tree::LayoutTree},
    };

    // ------------------------------------------------------------------
    // Fixtures
    // ------------------------------------------------------------------

    fn dom(inner: usize) -> DomId {
        DomId { inner }
    }

    fn nid(dom_idx: usize, node: usize) -> DomNodeId {
        FocusSearchContext::make_dom_node_id(dom(dom_idx), NodeId::new(node))
    }

    /// A `DomNodeId` whose node slot is the "no node" sentinel (`inner == 0`).
    fn null_nid(dom_idx: usize) -> DomNodeId {
        DomNodeId {
            dom: dom(dom_idx),
            node: NodeHierarchyItemId::from_crate_internal(None),
        }
    }

    /// `DomLayoutResult` with an empty layout tree — every function under test
    /// here reads only `styled_dom`, so no real layout (and no font) is needed.
    fn layout_result(styled_dom: StyledDom) -> DomLayoutResult {
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
            display_list: DisplayList::default(),
            scroll_ids: HashMap::new(),
            scroll_id_to_node_id: HashMap::new(),
        }
    }

    fn window(entries: Vec<(DomId, StyledDom)>) -> BTreeMap<DomId, DomLayoutResult> {
        entries
            .into_iter()
            .map(|(id, sd)| (id, layout_result(sd)))
            .collect()
    }

    /// Flat (pre-order) indices of [`tab_fixture`]:
    ///
    /// | idx | node                    | focusable | tab bucket        |
    /// |-----|-------------------------|-----------|-------------------|
    /// | 0   | body                    | no        | —                 |
    /// | 1   | div (plain)             | no        | —                 |
    /// | 2   | button                  | yes       | auto              |
    /// | 3   | div `tabindex=2`        | yes       | positive (n=2)    |
    /// | 4   | div `tabindex=-1`       | yes       | EXCLUDED          |
    /// | 5   | div `tabindex=1`        | yes       | positive (n=1)    |
    /// | 6   | div `tabindex=0`        | yes       | auto              |
    /// | 7   | textarea                | yes       | auto              |
    ///
    /// => tab order `[5, 3, 2, 6, 7]`.
    fn tab_fixture() -> StyledDom {
        StyledDom::create_from_dom(
            Dom::create_body()
                .with_child(Dom::create_div())
                .with_child(Dom::create_node(NodeType::Button))
                .with_child(Dom::create_div().with_tab_index(TabIndex::OverrideInParent(2)))
                .with_child(Dom::create_div().with_tab_index(TabIndex::NoKeyboardFocus))
                .with_child(Dom::create_div().with_tab_index(TabIndex::OverrideInParent(1)))
                .with_child(Dom::create_div().with_tab_index(TabIndex::OverrideInParent(0)))
                .with_child(Dom::create_node(NodeType::TextArea)),
        )
    }

    fn tab_order_of(fixture: StyledDom) -> Vec<DomNodeId> {
        collect_tab_order(&window(vec![(dom(0), fixture)]))
    }

    fn class_path(class: &str) -> CssPath {
        CssPath {
            selectors: vec![CssPathSelector::Class(class.to_string().into())].into(),
        }
    }

    // ==================================================================
    // FocusManager — constructor / getters / predicates
    // ==================================================================

    #[test]
    fn focus_manager_new_matches_default_and_is_fully_empty() {
        let fm = FocusManager::new();
        assert_eq!(fm, FocusManager::default());
        assert_eq!(fm.get_focused_node(), None);
        assert!(!fm.needs_cursor_initialization());
        assert_eq!(fm.pending_focus_request, None);
        assert_eq!(fm.pending_contenteditable_focus, None);
        // A default instance must answer every query without panicking.
        assert!(!fm.has_focus(&nid(0, 0)));
        assert!(!fm.has_focus(&null_nid(0)));
    }

    #[test]
    fn focus_manager_set_get_clear_focus_roundtrip() {
        let mut fm = FocusManager::new();
        fm.set_focused_node(Some(nid(0, 3)));
        assert_eq!(fm.get_focused_node(), Some(&nid(0, 3)));
        assert!(fm.has_focus(&nid(0, 3)));

        // Explicitly setting `None` is equivalent to clearing.
        fm.set_focused_node(None);
        assert_eq!(fm.get_focused_node(), None);

        fm.set_focused_node(Some(nid(0, 3)));
        fm.clear_focus();
        assert_eq!(fm.get_focused_node(), None);
        assert!(!fm.has_focus(&nid(0, 3)));
        // Clearing twice is idempotent, not a panic.
        fm.clear_focus();
        assert_eq!(fm.get_focused_node(), None);
    }

    #[test]
    fn focus_manager_has_focus_discriminates_both_dom_and_node() {
        let mut fm = FocusManager::new();
        fm.set_focused_node(Some(nid(1, 4)));

        assert!(fm.has_focus(&nid(1, 4)));
        // Same node index, different DOM — must NOT be treated as focused.
        assert!(!fm.has_focus(&nid(0, 4)));
        assert!(!fm.has_focus(&nid(2, 4)));
        // Same DOM, different node index.
        assert!(!fm.has_focus(&nid(1, 3)));
        assert!(!fm.has_focus(&nid(1, 5)));
        // The "no node" sentinel must not alias node 0.
        assert!(!fm.has_focus(&null_nid(1)));
    }

    #[test]
    fn focus_manager_has_focus_on_null_node_sentinel_is_exact() {
        // Focusing the sentinel itself: it matches only the sentinel, and in
        // particular is NOT confused with real node index 0 (whose encoded
        // `NodeHierarchyItemId` is 1, not 0).
        let mut fm = FocusManager::new();
        fm.set_focused_node(Some(null_nid(0)));
        assert!(fm.has_focus(&null_nid(0)));
        assert!(!fm.has_focus(&nid(0, 0)));
        assert!(!fm.has_focus(&null_nid(1)));
    }

    #[test]
    fn focus_manager_focus_survives_extreme_node_index() {
        // `NodeHierarchyItemId` encodes `Some(n)` as `n + 1`, so `usize::MAX`
        // itself would overflow the encoding. `usize::MAX - 1` is the largest
        // representable node and must round-trip cleanly.
        let extreme = nid(usize::MAX, usize::MAX - 1);
        let mut fm = FocusManager::new();
        fm.set_focused_node(Some(extreme));
        assert!(fm.has_focus(&extreme));
        assert_eq!(
            fm.get_focused_node()
                .and_then(|n| n.node.into_crate_internal())
                .map(|n| n.index()),
            Some(usize::MAX - 1)
        );
    }

    // ==================================================================
    // FocusManager — pending focus request (one-shot)
    // ==================================================================

    #[test]
    fn focus_manager_take_focus_request_is_one_shot() {
        let mut fm = FocusManager::new();
        // Taking from a fresh manager yields None rather than panicking.
        assert_eq!(fm.take_focus_request(), None);

        fm.request_focus_change(FocusTarget::Next);
        assert_eq!(fm.take_focus_request(), Some(FocusTarget::Next));
        // Consumed — a second take must not replay the request.
        assert_eq!(fm.take_focus_request(), None);
        assert_eq!(fm.take_focus_request(), None);
    }

    #[test]
    fn focus_manager_request_focus_change_overwrites_pending_request() {
        // The field holds a single slot: a second request silently REPLACES the
        // first (requests are not queued). Pin that, so a change to queueing
        // semantics is a deliberate, visible break.
        let mut fm = FocusManager::new();
        fm.request_focus_change(FocusTarget::First);
        fm.request_focus_change(FocusTarget::Last);
        fm.request_focus_change(FocusTarget::NoFocus);
        assert_eq!(fm.take_focus_request(), Some(FocusTarget::NoFocus));
        assert_eq!(fm.take_focus_request(), None);
    }

    #[test]
    fn focus_manager_request_focus_change_accepts_every_variant() {
        let path = FocusTarget::Path(FocusTargetPath {
            dom: dom(usize::MAX),
            css_path: class_path("nonexistent"),
        });
        let targets = vec![
            FocusTarget::Id(nid(0, 0)),
            FocusTarget::Id(null_nid(usize::MAX)),
            path,
            FocusTarget::Previous,
            FocusTarget::Next,
            FocusTarget::First,
            FocusTarget::Last,
            FocusTarget::NoFocus,
        ];
        for t in targets {
            let mut fm = FocusManager::new();
            fm.request_focus_change(t.clone());
            assert_eq!(fm.take_focus_request(), Some(t));
        }
    }

    // ==================================================================
    // FocusManager — W3C "flag and defer" contenteditable state
    // ==================================================================

    #[test]
    fn focus_manager_pending_contenteditable_set_then_take_is_one_shot() {
        let mut fm = FocusManager::new();
        assert!(!fm.needs_cursor_initialization());
        assert_eq!(fm.take_pending_contenteditable_focus(), None);

        fm.set_pending_contenteditable_focus(dom(2), NodeId::new(7), NodeId::new(9));
        assert!(fm.needs_cursor_initialization());

        assert_eq!(
            fm.take_pending_contenteditable_focus(),
            Some(PendingContentEditableFocus {
                dom_id: dom(2),
                container_node_id: NodeId::new(7),
                text_node_id: NodeId::new(9),
            })
        );
        // Flag consumed; a second take must not replay the pending focus.
        assert!(!fm.needs_cursor_initialization());
        assert_eq!(fm.take_pending_contenteditable_focus(), None);
    }

    #[test]
    fn focus_manager_set_pending_contenteditable_overwrites_and_accepts_extremes() {
        let mut fm = FocusManager::new();
        fm.set_pending_contenteditable_focus(dom(0), NodeId::new(1), NodeId::new(2));
        // Extreme ids (and container == text, i.e. a degenerate self-reference)
        // must be stored verbatim without panicking.
        fm.set_pending_contenteditable_focus(
            dom(usize::MAX),
            NodeId::new(usize::MAX),
            NodeId::new(usize::MAX),
        );
        assert_eq!(
            fm.take_pending_contenteditable_focus(),
            Some(PendingContentEditableFocus {
                dom_id: dom(usize::MAX),
                container_node_id: NodeId::new(usize::MAX),
                text_node_id: NodeId::new(usize::MAX),
            })
        );
    }

    #[test]
    fn focus_manager_clear_pending_contenteditable_clears_flag_and_value() {
        let mut fm = FocusManager::new();
        fm.set_pending_contenteditable_focus(dom(0), NodeId::new(1), NodeId::new(2));
        fm.clear_pending_contenteditable_focus();

        assert!(!fm.needs_cursor_initialization());
        assert_eq!(fm.pending_contenteditable_focus, None);
        assert_eq!(fm.take_pending_contenteditable_focus(), None);
        // Clearing an already-clear manager is idempotent.
        fm.clear_pending_contenteditable_focus();
        assert!(!fm.needs_cursor_initialization());
    }

    #[test]
    fn focus_manager_take_pending_without_flag_strands_the_value() {
        // Both fields are `pub`, so the flag and the value can be desynced by a
        // direct field write. `take_pending_contenteditable_focus` gates purely
        // on the FLAG, so a value written without the flag is never handed out
        // and is left stranded in the manager. Pin the (safe, non-panicking)
        // behaviour: no cursor is initialised, and the stale value survives.
        let mut fm = FocusManager::new();
        fm.pending_contenteditable_focus = Some(PendingContentEditableFocus {
            dom_id: dom(0),
            container_node_id: NodeId::new(1),
            text_node_id: NodeId::new(2),
        });

        assert!(!fm.needs_cursor_initialization());
        assert_eq!(fm.take_pending_contenteditable_focus(), None);
        assert!(fm.pending_contenteditable_focus.is_some());
    }

    #[test]
    fn focus_manager_flag_without_value_take_returns_none_and_clears_flag() {
        // The mirror-image desync: flag set, value absent. `take` must report
        // "nothing to do" AND drop the flag, so the caller cannot spin on a
        // permanently-pending initialisation.
        let mut fm = FocusManager::new();
        fm.cursor_needs_initialization = true;

        assert!(fm.needs_cursor_initialization());
        assert_eq!(fm.take_pending_contenteditable_focus(), None);
        assert!(!fm.needs_cursor_initialization());
    }

    #[test]
    fn focus_manager_clear_focus_does_not_clear_pending_cursor_state() {
        // `clear_focus` touches ONLY `focused_node`: the deferred contenteditable
        // cursor request deliberately survives it (callers must call
        // `clear_pending_contenteditable_focus` themselves). Pin this, since a
        // silent change would either leak a cursor into an unfocused node or
        // drop a legitimate deferred cursor.
        let mut fm = FocusManager::new();
        fm.set_focused_node(Some(nid(0, 4)));
        fm.set_pending_contenteditable_focus(dom(0), NodeId::new(4), NodeId::new(5));

        fm.clear_focus();

        assert_eq!(fm.get_focused_node(), None);
        assert!(fm.needs_cursor_initialization());
        assert!(fm.pending_contenteditable_focus.is_some());
    }

    // ==================================================================
    // order_tab_entries / doc_order_key
    // ==================================================================

    #[test]
    fn order_tab_entries_empty_inputs_yield_empty() {
        assert_eq!(order_tab_entries(Vec::new(), Vec::new()), Vec::new());
    }

    #[test]
    fn order_tab_entries_u32_max_sorts_after_smaller_positives() {
        // No overflow / wrap: u32::MAX is just a very large key and must land
        // last among the positives, never first.
        let order = order_tab_entries(
            vec![
                (u32::MAX, nid(0, 1)),
                (1, nid(0, 2)),
                (u32::MAX - 1, nid(0, 3)),
            ],
            vec![nid(0, 4)],
        );
        assert_eq!(
            order,
            vec![nid(0, 2), nid(0, 3), nid(0, 1), nid(0, 4)],
            "u32::MAX must sort last among positives, and all positives before auto"
        );
    }

    #[test]
    fn order_tab_entries_positive_always_precedes_auto() {
        // Even the largest possible positive tabindex outranks every auto entry.
        let order = order_tab_entries(vec![(u32::MAX, nid(9, 99))], vec![nid(0, 0), nid(0, 1)]);
        assert_eq!(order[0], nid(9, 99));
        assert_eq!(order.len(), 3);
    }

    #[test]
    fn order_tab_entries_does_not_deduplicate() {
        // Duplicates are preserved verbatim (the function is a pure merge, not a
        // set builder) — a duplicated entry must not silently vanish.
        let order = order_tab_entries(
            vec![(1, nid(0, 1)), (1, nid(0, 1))],
            vec![nid(0, 2), nid(0, 2)],
        );
        assert_eq!(order, vec![nid(0, 1), nid(0, 1), nid(0, 2), nid(0, 2)]);
    }

    #[test]
    fn doc_order_key_null_node_collides_with_node_index_zero() {
        // `doc_order_key` maps the "no node" sentinel to arena index 0, so it is
        // indistinguishable from real node 0 within the same DOM. Pin the
        // collision: `next_in_tab_order`'s re-entry search relies on this key,
        // and a focus sitting on the sentinel therefore re-enters as if it sat
        // on node 0.
        assert_eq!(doc_order_key(&null_nid(0)), (0, 0));
        assert_eq!(doc_order_key(&nid(0, 0)), (0, 0));
        assert_eq!(doc_order_key(&null_nid(0)), doc_order_key(&nid(0, 0)));
    }

    #[test]
    fn doc_order_key_is_dom_major_then_arena_index() {
        assert_eq!(doc_order_key(&nid(3, 7)), (3, 7));
        // DOM index dominates: a huge node index in DOM 0 still precedes node 0
        // of DOM 1.
        assert!(doc_order_key(&nid(0, usize::MAX - 1)) < doc_order_key(&nid(1, 0)));
        assert_eq!(doc_order_key(&nid(usize::MAX, usize::MAX - 1)), (usize::MAX, usize::MAX - 1));
    }

    // ==================================================================
    // next_in_tab_order
    // ==================================================================

    #[test]
    fn next_in_tab_order_single_entry_wraps_onto_itself() {
        // `(0 + 1) % 1 == 0` — must terminate on itself, not loop or panic.
        let order = vec![nid(0, 1)];
        assert_eq!(next_in_tab_order(&order, Some(nid(0, 1)), true), Some(nid(0, 1)));
        assert_eq!(next_in_tab_order(&order, Some(nid(0, 1)), false), Some(nid(0, 1)));
    }

    #[test]
    fn next_in_tab_order_duplicate_entries_resolve_to_first_position() {
        // `position()` finds the FIRST occurrence, so a duplicated tab stop makes
        // the trailing copy unreachable by stepping. Pin it (a dedup in
        // `collect_tab_order` would change this).
        let order = vec![nid(0, 1), nid(0, 2), nid(0, 1)];
        assert_eq!(next_in_tab_order(&order, Some(nid(0, 1)), true), Some(nid(0, 2)));
        // Backward from index 0 wraps to the last element.
        assert_eq!(next_in_tab_order(&order, Some(nid(0, 1)), false), Some(nid(0, 1)));
    }

    #[test]
    fn next_in_tab_order_unknown_current_uses_cross_dom_document_order() {
        // Current node lives in DOM 1; the tab order is split across DOM 0 and 2.
        let order = vec![nid(0, 5), nid(2, 1)];
        assert_eq!(next_in_tab_order(&order, Some(nid(1, 0)), true), Some(nid(2, 1)));
        assert_eq!(next_in_tab_order(&order, Some(nid(1, 9)), false), Some(nid(0, 5)));
    }

    #[test]
    fn next_in_tab_order_unknown_current_past_both_ends_wraps() {
        let order = vec![nid(1, 2), nid(1, 4)];
        // Nothing greater -> wrap to first.
        assert_eq!(next_in_tab_order(&order, Some(nid(9, 9)), true), Some(nid(1, 2)));
        // Nothing smaller -> wrap to last.
        assert_eq!(next_in_tab_order(&order, Some(nid(0, 0)), false), Some(nid(1, 4)));
    }

    #[test]
    fn next_in_tab_order_null_current_node_is_deterministic() {
        // The sentinel keys as (dom, 0); it is not in the order, so the re-entry
        // path runs. It must produce a stable answer, not panic.
        let order = vec![nid(0, 1), nid(0, 3)];
        assert_eq!(next_in_tab_order(&order, Some(null_nid(0)), true), Some(nid(0, 1)));
        assert_eq!(next_in_tab_order(&order, Some(null_nid(0)), false), Some(nid(0, 3)));
    }

    #[test]
    fn next_in_tab_order_empty_order_is_none_for_every_input() {
        assert_eq!(next_in_tab_order(&[], None, true), None);
        assert_eq!(next_in_tab_order(&[], None, false), None);
        assert_eq!(next_in_tab_order(&[], Some(null_nid(0)), true), None);
        assert_eq!(
            next_in_tab_order(&[], Some(nid(usize::MAX, usize::MAX - 1)), false),
            None
        );
    }

    // ==================================================================
    // collect_tab_order
    // ==================================================================

    #[test]
    fn collect_tab_order_empty_window_is_empty() {
        assert_eq!(collect_tab_order(&BTreeMap::new()), Vec::new());
    }

    #[test]
    fn collect_tab_order_dom_without_focusables_is_empty() {
        let sd = StyledDom::create_from_dom(
            Dom::create_body()
                .with_child(Dom::create_div())
                .with_child(Dom::create_div()),
        );
        assert_eq!(tab_order_of(sd), Vec::new());
    }

    #[test]
    fn collect_tab_order_positives_first_then_document_order_minus_one_excluded() {
        // See `tab_fixture` doc comment for the expected layout.
        assert_eq!(
            tab_order_of(tab_fixture()),
            vec![nid(0, 5), nid(0, 3), nid(0, 2), nid(0, 6), nid(0, 7)],
            "tabindex=1 then tabindex=2, then auto nodes in document order"
        );
    }

    #[test]
    fn collect_tab_order_excludes_tabindex_minus_one_though_it_is_focusable() {
        // The tabindex=-1 node (index 4) is click/API focusable but must NEVER be
        // a tab stop.
        let order = tab_order_of(tab_fixture());
        assert!(!order.contains(&nid(0, 4)), "tabindex=-1 must not be a tab stop");
        // ...and the plain, non-focusable div is absent too.
        assert!(!order.contains(&nid(0, 1)));
        // ...while the body itself is never a tab stop.
        assert!(!order.contains(&nid(0, 0)));
    }

    #[test]
    fn collect_tab_order_huge_tabindex_truncates_at_28_bits() {
        // `NodeFlags` packs the tabindex into 28 bits, so:
        //   * tabindex = u32::MAX  -> stored as 2^28-1  -> still POSITIVE
        //   * tabindex = 1 << 28   -> stored as 0       -> demoted to the AUTO
        //                                                  bucket (0 is not > 0)
        // The truncation is silent, so pin the observable ordering consequence.
        //
        // Document order: 1 = u32::MAX, 2 = 1<<28, 3 = tabindex 1, 4 = button.
        let sd = StyledDom::create_from_dom(
            Dom::create_body()
                .with_child(Dom::create_div().with_tab_index(TabIndex::OverrideInParent(u32::MAX)))
                .with_child(Dom::create_div().with_tab_index(TabIndex::OverrideInParent(1 << 28)))
                .with_child(Dom::create_div().with_tab_index(TabIndex::OverrideInParent(1)))
                .with_child(Dom::create_node(NodeType::Button)),
        );

        assert_eq!(
            tab_order_of(sd),
            vec![nid(0, 3), nid(0, 1), nid(0, 2), nid(0, 4)],
            "u32::MAX stays positive (sorts after tabindex=1); 1<<28 truncates to 0 and \
             falls back into the auto bucket behind every positive"
        );
    }

    #[test]
    fn collect_tab_order_tab_order_is_global_across_doms() {
        // A positive-tabindex node in DOM 1 must outrank an auto node in DOM 0:
        // the tab order is a single sequence over all DOMs, not per-DOM chunks.
        let order = collect_tab_order(&window(vec![
            (dom(0), tab_fixture()),
            (dom(1), tab_fixture()),
        ]));

        assert_eq!(
            order,
            vec![
                // positives, ascending; ties broken by DOM then document order
                nid(0, 5),
                nid(1, 5),
                nid(0, 3),
                nid(1, 3),
                // autos, in DOM order then document order
                nid(0, 2),
                nid(0, 6),
                nid(0, 7),
                nid(1, 2),
                nid(1, 6),
                nid(1, 7),
            ]
        );
    }

    // ==================================================================
    // FocusSearchContext
    // ==================================================================

    #[test]
    fn focus_search_context_get_layout_hit_and_miss() {
        let results = window(vec![(dom(0), tab_fixture())]);
        let ctx = FocusSearchContext::new(&results);

        // `DomLayoutResult` is not `PartialEq`, so compare on the error side only.
        assert!(ctx.get_layout(&dom(0)).is_ok());
        assert_eq!(
            ctx.get_layout(&dom(1)).err(),
            Some(UpdateFocusWarning::FocusInvalidDomId(dom(1)))
        );
        assert_eq!(
            ctx.get_layout(&dom(usize::MAX)).err(),
            Some(UpdateFocusWarning::FocusInvalidDomId(dom(usize::MAX)))
        );
    }

    #[test]
    fn focus_search_context_new_on_empty_map_never_resolves() {
        let empty = BTreeMap::new();
        let ctx = FocusSearchContext::new(&empty);
        assert_eq!(
            ctx.get_layout(&dom(0)).err(),
            Some(UpdateFocusWarning::FocusInvalidDomId(dom(0)))
        );
    }

    #[test]
    fn make_dom_node_id_round_trips_including_boundary_index() {
        // encode == decode for 0, a mid value, and the largest encodable index
        // (`usize::MAX` itself would overflow `NodeHierarchyItemId`'s n+1 encoding).
        for idx in [0usize, 1, 42, usize::MAX - 1] {
            let d = FocusSearchContext::make_dom_node_id(dom(7), NodeId::new(idx));
            assert_eq!(d.dom, dom(7));
            assert_eq!(d.node.into_crate_internal(), Some(NodeId::new(idx)));
            assert_eq!(doc_order_key(&d), (7, idx));
        }
    }

    // ==================================================================
    // find_first_matching_focusable_node
    // ==================================================================

    #[test]
    fn find_first_matching_skips_matching_but_unfocusable_nodes() {
        // Node 1 matches `.target` but is NOT focusable; node 2 matches AND is
        // focusable. The first *focusable* match must win.
        let sd = StyledDom::create_from_dom(
            Dom::create_body()
                .with_child(Dom::create_div().with_class("target".to_string().into()))
                .with_child(
                    Dom::create_div()
                        .with_class("target".to_string().into())
                        .with_tab_index(TabIndex::Auto),
                ),
        );
        let results = window(vec![(dom(0), sd)]);
        let layout = results.get(&dom(0)).unwrap();

        assert_eq!(
            find_first_matching_focusable_node(layout, &dom(0), &class_path("target")),
            Some(nid(0, 2))
        );
    }

    #[test]
    fn find_first_matching_empty_css_path_matches_nothing() {
        // A `CssPath` with zero selectors must not vacuously match every node.
        let results = window(vec![(dom(0), tab_fixture())]);
        let layout = results.get(&dom(0)).unwrap();
        let empty = CssPath {
            selectors: Vec::<CssPathSelector>::new().into(),
        };

        assert_eq!(
            find_first_matching_focusable_node(layout, &dom(0), &empty),
            None
        );
    }

    #[test]
    fn find_first_matching_unicode_class_matches_and_misses_cleanly() {
        // Non-ASCII / astral-plane class names must compare by exact string, with
        // no panic and no byte-index slicing surprises.
        let class = "クラス-día-🎯";
        let sd = StyledDom::create_from_dom(
            Dom::create_body().with_child(
                Dom::create_div()
                    .with_class(class.to_string().into())
                    .with_tab_index(TabIndex::Auto),
            ),
        );
        let results = window(vec![(dom(0), sd)]);
        let layout = results.get(&dom(0)).unwrap();

        assert_eq!(
            find_first_matching_focusable_node(layout, &dom(0), &class_path(class)),
            Some(nid(0, 1))
        );
        // A near-miss (same prefix, different suffix) must NOT match.
        assert_eq!(
            find_first_matching_focusable_node(layout, &dom(0), &class_path("クラス-día-🎲")),
            None
        );
        // A huge class name that cannot exist in the DOM also just misses.
        let huge = "x".repeat(10_000);
        assert_eq!(
            find_first_matching_focusable_node(layout, &dom(0), &class_path(&huge)),
            None
        );
    }

    // ==================================================================
    // resolve_focus_target
    // ==================================================================

    #[test]
    fn resolve_focus_target_empty_window_short_circuits_every_variant() {
        // The `layout_results.is_empty()` guard runs BEFORE any validation, so
        // even a structurally invalid target resolves to `Ok(None)` — never Err,
        // never a panic.
        let empty = BTreeMap::new();
        let targets = vec![
            FocusTarget::Id(nid(usize::MAX, 0)),
            FocusTarget::Id(null_nid(0)),
            FocusTarget::Path(FocusTargetPath {
                dom: dom(usize::MAX),
                css_path: class_path("nope"),
            }),
            FocusTarget::Previous,
            FocusTarget::Next,
            FocusTarget::First,
            FocusTarget::Last,
            FocusTarget::NoFocus,
        ];
        for t in targets {
            assert_eq!(
                resolve_focus_target(&t, &empty, Some(nid(0, 1))),
                Ok(None),
                "empty window must short-circuit {t:?}"
            );
        }
    }

    #[test]
    fn resolve_focus_target_id_rejects_unknown_dom() {
        let results = window(vec![(dom(0), tab_fixture())]);
        assert_eq!(
            resolve_focus_target(&FocusTarget::Id(nid(1, 2)), &results, None),
            Err(UpdateFocusWarning::FocusInvalidDomId(dom(1)))
        );
    }

    #[test]
    fn resolve_focus_target_id_rejects_out_of_range_node() {
        // The fixture has 8 nodes (0..=7); anything past the end must be a
        // `FocusInvalidNodeId` error, not a panic and not a silent focus.
        let results = window(vec![(dom(0), tab_fixture())]);
        for idx in [8usize, 9, 1_000_000, usize::MAX - 1] {
            let target = nid(0, idx);
            assert_eq!(
                resolve_focus_target(&FocusTarget::Id(target), &results, None),
                Err(UpdateFocusWarning::FocusInvalidNodeId(target.node)),
                "node {idx} is out of range and must be rejected"
            );
        }
    }

    #[test]
    fn resolve_focus_target_id_rejects_null_node_sentinel() {
        let results = window(vec![(dom(0), tab_fixture())]);
        let target = null_nid(0);
        assert_eq!(
            resolve_focus_target(&FocusTarget::Id(target), &results, None),
            Err(UpdateFocusWarning::FocusInvalidNodeId(target.node))
        );
    }

    #[test]
    fn resolve_focus_target_id_accepts_valid_but_unfocusable_node() {
        // `Id` checks only that the node EXISTS — programmatic focus deliberately
        // bypasses the focusability check (unlike `Path` and the tab order).
        // Node 0 is the body and node 4 is tabindex=-1: both resolve.
        let results = window(vec![(dom(0), tab_fixture())]);
        assert_eq!(
            resolve_focus_target(&FocusTarget::Id(nid(0, 0)), &results, None),
            Ok(Some(nid(0, 0)))
        );
        assert_eq!(
            resolve_focus_target(&FocusTarget::Id(nid(0, 4)), &results, None),
            Ok(Some(nid(0, 4)))
        );
    }

    #[test]
    fn resolve_focus_target_path_rejects_unknown_dom() {
        let results = window(vec![(dom(0), tab_fixture())]);
        let target = FocusTarget::Path(FocusTargetPath {
            dom: dom(3),
            css_path: class_path("target"),
        });
        assert_eq!(
            resolve_focus_target(&target, &results, None),
            Err(UpdateFocusWarning::FocusInvalidDomId(dom(3)))
        );
    }

    #[test]
    fn resolve_focus_target_path_with_no_match_is_ok_none_not_err() {
        // NOTE: the doc comment on `find_first_matching_focusable_node` advertises
        // `Err(_)` for an unmatchable path, but the implementation returns
        // `Ok(None)`. Pin the IMPLEMENTED behaviour (a miss is not an error);
        // the doc comment is what is wrong here.
        let results = window(vec![(dom(0), tab_fixture())]);
        let target = FocusTarget::Path(FocusTargetPath {
            dom: dom(0),
            css_path: class_path("no-such-class"),
        });
        assert_eq!(resolve_focus_target(&target, &results, None), Ok(None));
    }

    #[test]
    fn resolve_focus_target_no_focus_is_always_none() {
        let results = window(vec![(dom(0), tab_fixture())]);
        assert_eq!(
            resolve_focus_target(&FocusTarget::NoFocus, &results, Some(nid(0, 5))),
            Ok(None)
        );
    }

    #[test]
    fn resolve_focus_target_first_and_last_are_the_tab_order_ends() {
        let results = window(vec![(dom(0), tab_fixture())]);
        // Tab order is [5, 3, 2, 6, 7].
        assert_eq!(
            resolve_focus_target(&FocusTarget::First, &results, None),
            Ok(Some(nid(0, 5))),
            "First must be the lowest positive tabindex, not document node 0"
        );
        assert_eq!(
            resolve_focus_target(&FocusTarget::Last, &results, None),
            Ok(Some(nid(0, 7)))
        );
        // `current_focus` must not influence First/Last.
        assert_eq!(
            resolve_focus_target(&FocusTarget::First, &results, Some(nid(0, 7))),
            Ok(Some(nid(0, 5)))
        );
    }

    #[test]
    fn resolve_focus_target_first_and_last_on_unfocusable_dom_are_none() {
        let sd = StyledDom::create_from_dom(Dom::create_body().with_child(Dom::create_div()));
        let results = window(vec![(dom(0), sd)]);
        assert_eq!(resolve_focus_target(&FocusTarget::First, &results, None), Ok(None));
        assert_eq!(resolve_focus_target(&FocusTarget::Last, &results, None), Ok(None));
        assert_eq!(resolve_focus_target(&FocusTarget::Next, &results, None), Ok(None));
        assert_eq!(
            resolve_focus_target(&FocusTarget::Previous, &results, Some(nid(0, 0))),
            Ok(None)
        );
    }

    #[test]
    fn resolve_focus_target_next_and_previous_wrap_around_the_tab_order() {
        let results = window(vec![(dom(0), tab_fixture())]);
        // Tab order [5, 3, 2, 6, 7]: stepping off either end wraps.
        assert_eq!(
            resolve_focus_target(&FocusTarget::Next, &results, Some(nid(0, 7))),
            Ok(Some(nid(0, 5)))
        );
        assert_eq!(
            resolve_focus_target(&FocusTarget::Previous, &results, Some(nid(0, 5))),
            Ok(Some(nid(0, 7)))
        );
        // ...and step normally in the middle.
        assert_eq!(
            resolve_focus_target(&FocusTarget::Next, &results, Some(nid(0, 3))),
            Ok(Some(nid(0, 2)))
        );
        assert_eq!(
            resolve_focus_target(&FocusTarget::Previous, &results, Some(nid(0, 2))),
            Ok(Some(nid(0, 3)))
        );
    }

    #[test]
    fn resolve_focus_target_next_from_a_non_tab_stop_reenters_in_document_order() {
        let results = window(vec![(dom(0), tab_fixture())]);
        // Focus sits on the tabindex=-1 node (index 4), which is NOT in the tab
        // order. Shift+Tab must fall back to document order and land on node 3
        // (the nearest preceding tab stop by DOCUMENT position), NOT on the tab
        // order's neighbour of any element.
        assert_eq!(
            resolve_focus_target(&FocusTarget::Previous, &results, Some(nid(0, 4))),
            Ok(Some(nid(0, 3)))
        );
        // Focus on the plain, non-focusable div (index 1): Tab goes to the next
        // tab stop in DOCUMENT order (node 2, the button) — not to the tab
        // order's first entry (node 5).
        assert_eq!(
            resolve_focus_target(&FocusTarget::Next, &results, Some(nid(0, 1))),
            Ok(Some(nid(0, 2)))
        );
    }

    #[test]
    fn resolve_focus_target_next_from_a_stale_removed_node_never_panics() {
        // Focus left over from a previous DOM whose node index no longer exists:
        // resolution must still yield a valid tab stop rather than panic.
        let results = window(vec![(dom(0), tab_fixture())]);
        let stale = nid(0, 9_999);
        assert_eq!(
            resolve_focus_target(&FocusTarget::Next, &results, Some(stale)),
            Ok(Some(nid(0, 5))),
            "no tab stop past node 9999 -> wrap to the first"
        );
        // A stale node in a DOM that isn't even mounted.
        let alien = nid(5, 1);
        assert_eq!(
            resolve_focus_target(&FocusTarget::Next, &results, Some(alien)),
            Ok(Some(nid(0, 5)))
        );
        assert_eq!(
            resolve_focus_target(&FocusTarget::Previous, &results, Some(alien)),
            Ok(Some(nid(0, 7)))
        );
    }

    // ==================================================================
    // NodeIdRemap
    // ==================================================================

    #[test]
    fn remap_rewrites_the_focused_node() {
        let mut fm = FocusManager::new();
        fm.set_focused_node(Some(nid(0, 5)));
        fm.remap_node_ids(
            dom(0),
            &NodeIdMap::from_pairs([(NodeId::new(5), NodeId::new(2))]),
        );
        assert_eq!(fm.get_focused_node(), Some(&nid(0, 2)));
    }

    #[test]
    fn remap_clears_focus_on_an_unmounted_node() {
        // The node vanished from the rebuilt DOM: keeping the stale index would
        // silently focus a DIFFERENT element, so focus must be dropped.
        let mut fm = FocusManager::new();
        fm.set_focused_node(Some(nid(0, 5)));
        fm.remap_node_ids(
            dom(0),
            &NodeIdMap::from_pairs([(NodeId::new(1), NodeId::new(1))]),
        );
        assert_eq!(fm.get_focused_node(), None);
    }

    #[test]
    fn remap_leaves_other_doms_untouched() {
        let mut fm = FocusManager::new();
        fm.set_focused_node(Some(nid(1, 5)));
        // Remapping DOM 0 must not disturb focus that lives in DOM 1.
        fm.remap_node_ids(
            dom(0),
            &NodeIdMap::from_pairs([(NodeId::new(5), NodeId::new(2))]),
        );
        assert_eq!(fm.get_focused_node(), Some(&nid(1, 5)));
    }

    #[test]
    fn remap_rewrites_pending_contenteditable_focus() {
        let mut fm = FocusManager::new();
        fm.set_pending_contenteditable_focus(dom(0), NodeId::new(3), NodeId::new(4));
        fm.remap_node_ids(
            dom(0),
            &NodeIdMap::from_pairs([
                (NodeId::new(3), NodeId::new(10)),
                (NodeId::new(4), NodeId::new(11)),
            ]),
        );
        assert!(fm.needs_cursor_initialization());
        assert_eq!(
            fm.take_pending_contenteditable_focus(),
            Some(PendingContentEditableFocus {
                dom_id: dom(0),
                container_node_id: NodeId::new(10),
                text_node_id: NodeId::new(11),
            })
        );
    }

    #[test]
    fn remap_partially_resolvable_pending_focus_drops_it_entirely() {
        // Container survives the rebuild but the text node does not (or vice
        // versa): keeping half of the pair would place a cursor in the wrong
        // node, so BOTH the value and the flag must be dropped.
        for pairs in [
            vec![(NodeId::new(3), NodeId::new(10))],  // text node unmapped
            vec![(NodeId::new(4), NodeId::new(11))],  // container unmapped
            vec![],                                   // neither survives
        ] {
            let mut fm = FocusManager::new();
            fm.set_pending_contenteditable_focus(dom(0), NodeId::new(3), NodeId::new(4));
            fm.remap_node_ids(dom(0), &NodeIdMap::from_pairs(pairs));

            assert!(!fm.needs_cursor_initialization());
            assert_eq!(fm.pending_contenteditable_focus, None);
        }
    }
}
