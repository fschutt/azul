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

    /// Remap `NodeIds` in pending contenteditable focus after DOM reconciliation.
    ///
    /// This handles the edge case where a DOM rebuild happens between setting
    /// pending focus and consuming it after layout.
    pub fn remap_pending_focus_node_ids(
        &mut self,
        dom_id: DomId,
        node_id_map: &BTreeMap<NodeId, NodeId>,
    ) {
        if let Some(ref mut pending) = self.pending_contenteditable_focus {
            if pending.dom_id != dom_id {
                return;
            }
            if let Some(&new_id) = node_id_map.get(&pending.container_node_id) { pending.container_node_id = new_id } else {
                self.pending_contenteditable_focus = None;
                self.cursor_needs_initialization = false;
                return;
            }
            if let Some(&new_id) = node_id_map.get(&pending.text_node_id) { pending.text_node_id = new_id } else {
                self.pending_contenteditable_focus = None;
                self.cursor_needs_initialization = false;
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
/// API but are NEVER part of the Tab order. The previous linear NodeId walk
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
/// MWA-C-focus_cursor: the old linear-walk machinery (SearchDirection,
/// search_focusable_node, get_*_start) was replaced by the W3C tab order
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
