//! Focus and tab navigation management.
//!
//! Manages keyboard focus, tab navigation, and programmatic focus changes
//! with a recursive event system for focus/blur callbacks (max depth: 5).

use alloc::{collections::BTreeMap, vec::Vec};

use azul_core::{
    callbacks::{FocusTarget, FocusTargetPath},
    dom::{DomId, DomNodeId, NodeId},
    style::matches_html_element,
    styled_dom::NodeHierarchyItemId,
};

use crate::window::DomLayoutResult;

/// CSS path for selecting elements (placeholder - needs proper implementation)
pub type CssPathString = alloc::string::String;

/// Information about a pending contenteditable focus that needs cursor initialization
/// after layout is complete (W3C "flag and defer" pattern).
///
/// This is set during focus event handling and consumed after layout pass.
#[derive(Debug, Clone, PartialEq)]
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
#[derive(Debug, Clone, PartialEq)]
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
    pub fn new() -> Self {
        Self {
            focused_node: None,
            pending_focus_request: None,
            cursor_needs_initialization: false,
            pending_contenteditable_focus: None,
        }
    }

    /// Get the currently focused node
    pub fn get_focused_node(&self) -> Option<&DomNodeId> {
        self.focused_node.as_ref()
    }

    /// Set the focused node directly (used by event system)
    ///
    /// Note: Cursor initialization/clearing is now handled by `CursorManager`.
    /// The event system should check if the newly focused node is contenteditable
    /// and call `CursorManager::initialize_cursor_at_end()` if needed.
    pub fn set_focused_node(&mut self, node: Option<DomNodeId>) {
        self.focused_node = node;
    }

    /// Request a focus change (to be processed by event system)
    pub fn request_focus_change(&mut self, target: FocusTarget) {
        self.pending_focus_request = Some(target);
    }

    /// Take the pending focus request (one-shot)
    pub fn take_focus_request(&mut self) -> Option<FocusTarget> {
        self.pending_focus_request.take()
    }

    /// Clear focus
    pub fn clear_focus(&mut self) {
        self.focused_node = None;
    }

    /// Check if a specific node has focus
    pub fn has_focus(&self, node: &DomNodeId) -> bool {
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
    pub fn set_pending_contenteditable_focus(
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
    pub fn clear_pending_contenteditable_focus(&mut self) {
        self.cursor_needs_initialization = false;
        self.pending_contenteditable_focus = None;
    }
    
    /// Take the pending contenteditable focus (consumes the flag).
    ///
    /// Returns `Some(info)` if cursor initialization is pending, `None` otherwise.
    /// After calling this, `cursor_needs_initialization` is set to `false`.
    pub fn take_pending_contenteditable_focus(&mut self) -> Option<PendingContentEditableFocus> {
        if self.cursor_needs_initialization {
            self.cursor_needs_initialization = false;
            self.pending_contenteditable_focus.take()
        } else {
            None
        }
    }
    
    /// Check if cursor initialization is pending.
    pub fn needs_cursor_initialization(&self) -> bool {
        self.cursor_needs_initialization
    }
}

/// Direction for cursor navigation
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CursorNavigationDirection {
    /// Move cursor up one line
    Up,
    /// Move cursor down one line
    Down,
    /// Move cursor left one character
    Left,
    /// Move cursor right one character
    Right,
    /// Move cursor to start of current line
    LineStart,
    /// Move cursor to end of current line
    LineEnd,
    /// Move cursor to start of document
    DocumentStart,
    /// Move cursor to end of document
    DocumentEnd,
}

/// Result of a cursor movement operation
#[derive(Debug, Clone)]
pub enum CursorMovementResult {
    /// Cursor moved within the same text node
    MovedWithinNode(azul_core::selection::TextCursor),
    /// Cursor moved to a different text node
    MovedToNode {
        dom_id: DomId,
        node_id: NodeId,
        cursor: azul_core::selection::TextCursor,
    },
    /// Cursor is at a boundary and cannot move further
    AtBoundary {
        boundary: crate::text3::cache::TextBoundary,
        cursor: azul_core::selection::TextCursor,
    },
}

/// Error returned when cursor navigation cannot find a valid destination.
///
/// This occurs when attempting to move the cursor (e.g., arrow keys in a
/// contenteditable element) but no valid target position exists, such as
/// when already at the start/end of text content.
#[derive(Debug, Clone)]
pub struct NoCursorDestination {
    /// Human-readable explanation of why navigation failed
    pub reason: String,
}

/// Warning/error type for focus resolution failures.
///
/// Returned by `resolve_focus_target` when the requested focus target
/// cannot be resolved to a valid focusable node.
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateFocusWarning {
    /// The specified DOM ID does not exist in the layout results
    FocusInvalidDomId(DomId),
    /// The specified node ID does not exist within its DOM
    FocusInvalidNodeId(NodeHierarchyItemId),
    /// CSS path selector did not match any focusable node (includes the path for debugging)
    CouldNotFindFocusNode(String),
}

/// Direction for searching focusable nodes in the DOM tree.
///
/// Used by `search_focusable_node` to traverse nodes either forward
/// (towards higher indices / next DOM) or backward (towards lower indices / previous DOM).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum SearchDirection {
    /// Search forward: increment node index, move to next DOM when at end
    Forward,
    /// Search backward: decrement node index, move to previous DOM when at start
    Backward,
}

impl SearchDirection {
    /// Compute the next node index in this direction.
    ///
    /// Uses saturating arithmetic to avoid overflow/underflow.
    fn step_node(&self, index: usize) -> usize {
        match self {
            Self::Forward => index.saturating_add(1),
            Self::Backward => index.saturating_sub(1),
        }
    }

    /// Advance the DOM ID in this direction (mutates in place).
    fn step_dom(&self, dom_id: &mut DomId) {
        match self {
            Self::Forward => dom_id.inner += 1,
            Self::Backward => dom_id.inner -= 1,
        }
    }

    /// Check if we've hit a node boundary and need to switch DOMs.
    ///
    /// Returns `true` if:
    ///
    /// - Backward: at min node and current < start (wrapped around)
    /// - Forward: at max node and current > start (wrapped around)
    fn is_at_boundary(&self, current: NodeId, start: NodeId, min: NodeId, max: NodeId) -> bool {
        match self {
            Self::Backward => current == min && current < start,
            Self::Forward => current == max && current > start,
        }
    }

    /// Check if we've hit a DOM boundary (first or last DOM in the layout).
    fn is_at_dom_boundary(&self, dom_id: DomId, min: DomId, max: DomId) -> bool {
        match self {
            Self::Backward => dom_id == min,
            Self::Forward => dom_id == max,
        }
    }

    /// Get the starting node ID when entering a new DOM.
    ///
    /// - Forward: start at first node (index 0)
    /// - Backward: start at last node
    fn initial_node_for_next_dom(&self, layout: &DomLayoutResult) -> NodeId {
        match self {
            Self::Forward => NodeId::ZERO,
            Self::Backward => NodeId::new(layout.styled_dom.node_data.len() - 1),
        }
    }
}

/// Context for focusable node search operations.
///
/// Holds shared state and provides helper methods for traversing
/// the DOM tree to find focusable nodes. This avoids passing
/// multiple parameters through the search functions.
struct FocusSearchContext<'a> {
    /// Reference to all DOM layouts in the window
    layout_results: &'a BTreeMap<DomId, DomLayoutResult>,
    /// First DOM ID (always `ROOT_ID`)
    min_dom_id: DomId,
    /// Last DOM ID in the layout results
    max_dom_id: DomId,
}

impl<'a> FocusSearchContext<'a> {
    /// Create a new search context from layout results.
    fn new(layout_results: &'a BTreeMap<DomId, DomLayoutResult>) -> Self {
        Self {
            layout_results,
            min_dom_id: DomId::ROOT_ID,
            max_dom_id: DomId {
                inner: layout_results.len() - 1,
            },
        }
    }

    /// Get the layout for a DOM ID, or return an error if invalid.
    fn get_layout(&self, dom_id: &DomId) -> Result<&'a DomLayoutResult, UpdateFocusWarning> {
        self.layout_results
            .get(dom_id)
            .ok_or_else(|| UpdateFocusWarning::FocusInvalidDomId(dom_id.clone()))
    }

    /// Validate that a node exists in the given layout.
    ///
    /// Returns an error if the node ID is out of bounds or the DOM is empty.
    fn validate_node(
        &self,
        layout: &DomLayoutResult,
        node_id: NodeId,
        dom_id: DomId,
    ) -> Result<(), UpdateFocusWarning> {
        let is_valid = layout
            .styled_dom
            .node_data
            .as_container()
            .get(node_id)
            .is_some();
        if !is_valid {
            return Err(UpdateFocusWarning::FocusInvalidNodeId(
                NodeHierarchyItemId::from_crate_internal(Some(node_id)),
            ));
        }
        if layout.styled_dom.node_data.is_empty() {
            return Err(UpdateFocusWarning::FocusInvalidDomId(dom_id));
        }
        Ok(())
    }

    /// Get the valid node ID range for a layout: `(min, max)`.
    fn node_bounds(&self, layout: &DomLayoutResult) -> (NodeId, NodeId) {
        (
            NodeId::ZERO,
            NodeId::new(layout.styled_dom.node_data.len() - 1),
        )
    }

    /// Check if a node can receive keyboard focus.
    fn is_focusable(&self, layout: &DomLayoutResult, node_id: NodeId) -> bool {
        layout.styled_dom.node_data.as_container()[node_id].is_focusable()
    }

    /// Construct a `DomNodeId` from DOM and node IDs.
    fn make_dom_node_id(&self, dom_id: DomId, node_id: NodeId) -> DomNodeId {
        DomNodeId {
            dom: dom_id,
            node: NodeHierarchyItemId::from_crate_internal(Some(node_id)),
        }
    }
}

/// Search for the next focusable node in a given direction.
///
/// Traverses nodes within the current DOM, then moves to adjacent DOMs
/// if no focusable node is found. Returns `Ok(None)` if no focusable
/// node exists in the entire layout in the given direction.
///
/// # Termination guarantee
///
/// The function is guaranteed to terminate because:
///
/// - The inner loop advances `node_id` by 1 each iteration (via `step_node`)
/// - When hitting a node boundary, we either return `None` (at DOM boundary) or move to the next
///   DOM and break to the outer loop
/// - The outer loop only continues when we switch DOMs, which is bounded by the finite number of
///   DOMs in `layout_results`
/// - Each DOM is visited at most once per search direction
///
/// # Returns
///
/// * `Ok(Some(node))` - Found a focusable node
/// * `Ok(None)` - No focusable node exists in the search direction
/// * `Err(_)` - Invalid DOM or node ID encountered
fn search_focusable_node(
    ctx: &FocusSearchContext,
    mut dom_id: DomId,
    mut node_id: NodeId,
    direction: SearchDirection,
) -> Result<Option<DomNodeId>, UpdateFocusWarning> {
    loop {
        let layout = ctx.get_layout(&dom_id)?;
        ctx.validate_node(layout, node_id, dom_id)?;

        let (min_node, max_node) = ctx.node_bounds(layout);

        loop {
            let next_node = NodeId::new(direction.step_node(node_id.index()))
                .max(min_node)
                .min(max_node);

            // If we couldn't make progress (next_node == node_id due to clamping),
            // we've hit the boundary of this DOM
            if next_node == node_id {
                if direction.is_at_dom_boundary(dom_id, ctx.min_dom_id, ctx.max_dom_id) {
                    return Ok(None); // Reached end of all DOMs
                }
                direction.step_dom(&mut dom_id);
                let next_layout = ctx.get_layout(&dom_id)?;
                node_id = direction.initial_node_for_next_dom(next_layout);
                break; // Continue outer loop with new DOM
            }

            // Check for focusable node (we made progress, so this is a different node)
            if ctx.is_focusable(layout, next_node) {
                return Ok(Some(ctx.make_dom_node_id(dom_id, next_node)));
            }

            // Detect if we've hit the boundary (at min/max node)
            let at_boundary = direction.is_at_boundary(next_node, node_id, min_node, max_node);

            if at_boundary {
                if direction.is_at_dom_boundary(dom_id, ctx.min_dom_id, ctx.max_dom_id) {
                    return Ok(None); // Reached end of all DOMs
                }
                direction.step_dom(&mut dom_id);
                let next_layout = ctx.get_layout(&dom_id)?;
                node_id = direction.initial_node_for_next_dom(next_layout);
                break; // Continue outer loop with new DOM
            }

            node_id = next_node;
        }
    }
}

/// Get starting position for Previous focus search
fn get_previous_start(
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    current_focus: Option<DomNodeId>,
) -> Result<(DomId, NodeId), UpdateFocusWarning> {
    let last_dom_id = DomId {
        inner: layout_results.len() - 1,
    };

    let Some(focus) = current_focus else {
        let layout = layout_results
            .get(&last_dom_id)
            .ok_or(UpdateFocusWarning::FocusInvalidDomId(last_dom_id))?;
        return Ok((
            last_dom_id,
            NodeId::new(layout.styled_dom.node_data.len() - 1),
        ));
    };

    let Some(node) = focus.node.into_crate_internal() else {
        if let Some(layout) = layout_results.get(&focus.dom) {
            return Ok((
                focus.dom,
                NodeId::new(layout.styled_dom.node_data.len() - 1),
            ));
        }
        let layout = layout_results
            .get(&last_dom_id)
            .ok_or(UpdateFocusWarning::FocusInvalidDomId(last_dom_id))?;
        return Ok((
            last_dom_id,
            NodeId::new(layout.styled_dom.node_data.len() - 1),
        ));
    };

    Ok((focus.dom, node))
}

/// Get starting position for Next focus search
fn get_next_start(
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    current_focus: Option<DomNodeId>,
) -> (DomId, NodeId) {
    let Some(focus) = current_focus else {
        return (DomId::ROOT_ID, NodeId::ZERO);
    };

    match focus.node.into_crate_internal() {
        Some(node) => (focus.dom, node),
        None if layout_results.contains_key(&focus.dom) => (focus.dom, NodeId::ZERO),
        None => (DomId::ROOT_ID, NodeId::ZERO),
    }
}

/// Get starting position for Last focus search
fn get_last_start(
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
) -> Result<(DomId, NodeId), UpdateFocusWarning> {
    let last_dom_id = DomId {
        inner: layout_results.len() - 1,
    };
    let layout = layout_results
        .get(&last_dom_id)
        .ok_or(UpdateFocusWarning::FocusInvalidDomId(last_dom_id))?;
    Ok((
        last_dom_id,
        NodeId::new(layout.styled_dom.node_data.len() - 1),
    ))
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
fn find_first_matching_focusable_node(
    layout: &DomLayoutResult,
    dom_id: &DomId,
    css_path: &azul_css::css::CssPath,
) -> Result<Option<DomNodeId>, UpdateFocusWarning> {
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

    Ok(matching_node.map(|node_id| DomNodeId {
        dom: *dom_id,
        node: NodeHierarchyItemId::from_crate_internal(Some(node_id)),
    }))
}

/// Resolve a FocusTarget to an actual DomNodeId
pub fn resolve_focus_target(
    focus_target: &FocusTarget,
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    current_focus: Option<DomNodeId>,
) -> Result<Option<DomNodeId>, UpdateFocusWarning> {
    use azul_core::callbacks::FocusTarget::*;

    if layout_results.is_empty() {
        return Ok(None);
    }

    let ctx = FocusSearchContext::new(layout_results);

    match focus_target {
        Path(FocusTargetPath { dom, css_path }) => {
            let layout = ctx.get_layout(dom)?;
            find_first_matching_focusable_node(layout, dom, css_path)
        }

        Id(dom_node_id) => {
            let layout = ctx.get_layout(&dom_node_id.dom)?;
            let is_valid = dom_node_id
                .node
                .into_crate_internal()
                .map(|n| layout.styled_dom.node_data.as_container().get(n).is_some())
                .unwrap_or(false);

            if is_valid {
                Ok(Some(dom_node_id.clone()))
            } else {
                Err(UpdateFocusWarning::FocusInvalidNodeId(
                    dom_node_id.node.clone(),
                ))
            }
        }

        Previous => {
            let (dom_id, node_id) = get_previous_start(layout_results, current_focus)?;
            let result = search_focusable_node(&ctx, dom_id, node_id, SearchDirection::Backward)?;
            // Wrap around: if no previous focusable found, go to last focusable
            if result.is_none() {
                let (last_dom_id, last_node_id) = get_last_start(layout_results)?;
                // First check if the last node itself is focusable
                let last_layout = ctx.get_layout(&last_dom_id)?;
                if ctx.is_focusable(last_layout, last_node_id) {
                    Ok(Some(ctx.make_dom_node_id(last_dom_id, last_node_id)))
                } else {
                    // Otherwise search backward from last node
                    search_focusable_node(&ctx, last_dom_id, last_node_id, SearchDirection::Backward)
                }
            } else {
                Ok(result)
            }
        }

        Next => {
            let (dom_id, node_id) = get_next_start(layout_results, current_focus);
            let result = search_focusable_node(&ctx, dom_id, node_id, SearchDirection::Forward)?;
            // Wrap around: if no next focusable found, go to first focusable
            if result.is_none() {
                // First check if the first node itself is focusable
                let first_layout = ctx.get_layout(&DomId::ROOT_ID)?;
                if ctx.is_focusable(first_layout, NodeId::ZERO) {
                    Ok(Some(ctx.make_dom_node_id(DomId::ROOT_ID, NodeId::ZERO)))
                } else {
                    search_focusable_node(&ctx, DomId::ROOT_ID, NodeId::ZERO, SearchDirection::Forward)
                }
            } else {
                Ok(result)
            }
        }

        First => {
            // First check if the first node itself is focusable
            let first_layout = ctx.get_layout(&DomId::ROOT_ID)?;
            if ctx.is_focusable(first_layout, NodeId::ZERO) {
                Ok(Some(ctx.make_dom_node_id(DomId::ROOT_ID, NodeId::ZERO)))
            } else {
                search_focusable_node(&ctx, DomId::ROOT_ID, NodeId::ZERO, SearchDirection::Forward)
            }
        }

        Last => {
            let (dom_id, node_id) = get_last_start(layout_results)?;
            // First check if the last node itself is focusable
            let last_layout = ctx.get_layout(&dom_id)?;
            if ctx.is_focusable(last_layout, node_id) {
                Ok(Some(ctx.make_dom_node_id(dom_id, node_id)))
            } else {
                search_focusable_node(&ctx, dom_id, node_id, SearchDirection::Backward)
            }
        }

        NoFocus => Ok(None),
    }
}

// Trait Implementations for Event Filtering

impl azul_core::events::FocusManagerQuery for FocusManager {
    fn get_focused_node_id(&self) -> Option<azul_core::dom::DomNodeId> {
        self.focused_node
    }
}
