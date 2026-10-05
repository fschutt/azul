//! Focus and tab navigation management.
//!
//! Manages keyboard focus, tab navigation, and programmatic focus changes
//! with a recursive event system for focus/blur callbacks (max depth: 5).

use alloc::collections::{BTreeMap, BTreeSet};

use azul_core::{
    callbacks::{FocusDirection, FocusTarget, FocusTargetPath},
    dom::{DomId, DomNodeId, NodeId},
    geom::{LogicalPosition, LogicalRect},
    style::matches_html_element,
    styled_dom::NodeHierarchyItemId,
    transform::ComputedTransform3D,
    window::UpdateFocusWarning,
};
use azul_css::props::style::spatial_nav::{
    StyleSpatialNavigationAction, StyleSpatialNavigationFunction,
};

use crate::{managers::scroll_state::ScrollNodeInfo, window::DomLayoutResult};

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
    /// Nodes whose focus a DOM rebuild dropped, waiting to be told.
    /// See [`FocusManager::take_focus_lost_to_unmount`].
    pub focus_lost_to_unmount: Vec<DomNodeId>,
    /// Pending focus request from callback
    pub pending_focus_request: Option<FocusTarget>,

    /// Whether the CURRENT focus should be indicated visually - the W3C
    /// `:focus-visible` rule.
    ///
    /// Focus and its INDICATION are different questions. A browser rings a
    /// control focused by Tab and does NOT ring one focused by a click or by
    /// `autofocus`: a form that opens with its first field ringed looks like
    /// the user pressed Tab when they did not. The focus ring reads this;
    /// hit-testing, activation and the a11y tree read `focused_node` and do
    /// not care how focus arrived.
    ///
    /// Set by the route that MOVES focus: keyboard navigation sets it, a
    /// pointer click and `autofocus` clear it.
    pub focus_is_visible: bool,

    // --- W3C "flag and defer" pattern fields ---
    /// Flag indicating that cursor initialization is pending (set during focus, consumed after
    /// layout)
    pub cursor_needs_initialization: bool,
    /// Information about the pending contenteditable focus
    pub pending_contenteditable_focus: Option<PendingContentEditableFocus>,

    // --- focus-before-first-layout retry queue ---
    /// A [`FocusTarget`] that could not be resolved because no layout existed
    /// yet, kept so it can be re-resolved as soon as one does.
    ///
    /// `resolve_focus_target` used to answer `Ok(None)` with empty
    /// `layout_results`, and every caller read that as "clear focus" — so a programmatic
    /// `set_focus` issued from a `create` callback (which runs BEFORE the first
    /// layout) vanished without a trace, and apps papered over it with a
    /// short timer. Drained by
    /// `LayoutWindow::finalize_pending_focus_changes`, which runs after the
    /// layout pass.
    pub deferred_focus_target: Option<FocusTarget>,
    /// How many times the pending contenteditable focus has been re-armed for
    /// want of a text layout. Bounded so a node that will never have an inline
    /// layout cannot re-arm forever.
    pub pending_focus_retries: u8,

    /// The OTHER seats' focused nodes (9b-ii-a-i-d): seat 0, the primary, IS
    /// `focused_node`; a second seat (X11 MPX master pair, a second Wayland
    /// `wl_seat`) keeps its own here, so two people can edit two fields of
    /// one window - the user's ruling. Absent = that seat focuses nothing.
    /// Key events of a seat target ITS node; click-to-focus and the Tab walk
    /// move ITS entry. The text-edit session, the `:focus` restyle and the
    /// a11y focus stay the primary's (follow-ups, see the ledger).
    pub seat_focus: BTreeMap<u64, DomNodeId>,
}

/// How many times [`FocusManager::pending_contenteditable_focus`] may be put
/// back because the text layout was not available yet.
pub const MAX_PENDING_FOCUS_RETRIES: u8 = 2;

/// Outcome of resolving a [`FocusTarget`] — the answer says WHY there is no
/// node, not just that there is none.
///
/// The old shape (`Resolved(Option<DomNodeId>)`) conflated three different
/// answers in `None`: "the app asked to clear focus", "the selector matched
/// nothing", and "the tab order is empty" — and every caller applied all of
/// them as a clear. A focus request that merely MISSED (a selector against a
/// `VirtualView` that had not materialized yet, a Tab press with nothing
/// tabbable) therefore DESTROYED the current focus/caret as a side effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusResolution {
    /// The target resolved to this node — apply it.
    Resolved(DomNodeId),
    /// The app explicitly asked for no focus (`FocusTarget::NoFocus`) —
    /// apply the clear.
    ClearRequested,
    /// The target matched nothing (unmatched selector, empty tab order,
    /// no layout to search). NOT a clear: callers keep the current focus.
    NotFound,
    /// No layout existed yet, so the target was retained on the
    /// [`FocusManager`] and will be re-resolved after the first layout pass.
    /// Callers must leave the current focus alone.
    Deferred,
}

impl Default for FocusManager {
    fn default() -> Self {
        Self::new()
    }
}

impl FocusManager {
    /// Create a new focus manager
    #[must_use]
    pub const fn new() -> Self {
        Self {
            focused_node: None,
            focus_lost_to_unmount: Vec::new(),
            pending_focus_request: None,
            focus_is_visible: false,
            cursor_needs_initialization: false,
            pending_contenteditable_focus: None,
            deferred_focus_target: None,
            pending_focus_retries: 0,
            seat_focus: BTreeMap::new(),
        }
    }

    /// The node seat `seat_id` focuses (9b-ii-a-i-d): the primary's is
    /// `focused_node`, every other seat's its own entry.
    #[must_use]
    pub fn focused_node_for(&self, seat_id: u64) -> Option<DomNodeId> {
        if seat_id == azul_core::window::PRIMARY_POINTER_SEAT {
            self.focused_node
        } else {
            self.seat_focus.get(&seat_id).copied()
        }
    }

    /// Move seat `seat_id`'s focus; `None` clears it. The primary seat is
    /// `set_focused_node`.
    pub fn set_focused_node_for(&mut self, seat_id: u64, node: Option<DomNodeId>) {
        if seat_id == azul_core::window::PRIMARY_POINTER_SEAT {
            self.focused_node = node;
        } else {
            match node {
                Some(n) => {
                    self.seat_focus.insert(seat_id, n);
                }
                None => {
                    self.seat_focus.remove(&seat_id);
                }
            }
        }
    }

    /// Whether seat `seat_id` focuses `node`.
    #[must_use]
    pub fn has_focus_for(&self, seat_id: u64, node: &DomNodeId) -> bool {
        self.focused_node_for(seat_id).as_ref() == Some(node)
    }

    /// Every seat focusing `node`, the primary as seat 0.
    #[must_use]
    pub fn seats_focusing(&self, node: &DomNodeId) -> Vec<u64> {
        let mut seats = Vec::new();
        if self.focused_node.as_ref() == Some(node) {
            seats.push(azul_core::window::PRIMARY_POINTER_SEAT);
        }
        seats.extend(
            self.seat_focus
                .iter()
                .filter(|(_, n)| *n == node)
                .map(|(seat, _)| *seat),
        );
        seats
    }

    /// Get the currently focused node
    #[must_use]
    pub const fn get_focused_node(&self) -> Option<&DomNodeId> {
        self.focused_node.as_ref()
    }

    /// Set the focused node directly (used by event system)
    ///
    /// Note: Cursor initialization/clearing is now handled by `CursorManager`.
    /// The event system should check if the newly focused node is contenteditable
    /// and call `CursorManager::initialize_cursor_at_end()` if needed.
    /// Move focus and say whether it should be INDICATED (see
    /// [`Self::focus_is_visible`]): `true` for keyboard navigation, `false`
    /// for a pointer click or `autofocus`.
    pub const fn set_focused_node_with_visibility(
        &mut self,
        node: Option<DomNodeId>,
        visible: bool,
    ) {
        self.focused_node = node;
        self.focus_is_visible = visible;
    }

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
    #[must_use]
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
    pub const fn set_pending_contenteditable_focus(
        &mut self,
        dom_id: DomId,
        container_node_id: NodeId,
        text_node_id: NodeId,
    ) {
        self.cursor_needs_initialization = true;
        self.pending_focus_retries = 0;
        self.pending_contenteditable_focus = Some(PendingContentEditableFocus {
            dom_id,
            container_node_id,
            text_node_id,
        });
    }

    /// Put a just-taken pending contenteditable focus BACK because the text
    /// layout it needs did not exist yet. Returns `false` once
    /// [`MAX_PENDING_FOCUS_RETRIES`] is exhausted, in which case the caller
    /// must seed the cursor with whatever it has.
    pub const fn rearm_pending_contenteditable_focus(
        &mut self,
        pending: PendingContentEditableFocus,
    ) -> bool {
        if self.pending_focus_retries >= MAX_PENDING_FOCUS_RETRIES {
            return false;
        }
        self.pending_focus_retries += 1;
        self.cursor_needs_initialization = true;
        self.pending_contenteditable_focus = Some(pending);
        true
    }

    /// Put a just-taken pending contenteditable focus BACK because the whole
    /// DOM's layout result is ABSENT - the funnel takes it out while the pass
    /// runs, the same transient absence `caret_editable_is_focused` tolerates.
    /// Unlike [`Self::rearm_pending_contenteditable_focus`] this does NOT
    /// spend a retry: mid-pass absence is not a failed attempt, and burning
    /// the bounded budget on it locked in the (0,0)+Trailing fallback - the
    /// device caret landing MID-TEXT ('4|2') when tabbing into a filled
    /// `NumberInput` (2026-08-31).
    pub const fn rearm_pending_contenteditable_focus_transient(
        &mut self,
        pending: PendingContentEditableFocus,
    ) {
        self.cursor_needs_initialization = true;
        self.pending_contenteditable_focus = Some(pending);
    }

    /// Clear the pending contenteditable focus (when focus moves away or is cleared).
    pub const fn clear_pending_contenteditable_focus(&mut self) {
        self.cursor_needs_initialization = false;
        self.pending_focus_retries = 0;
        self.pending_contenteditable_focus = None;
    }

    /// Take the pending contenteditable focus (consumes the flag).
    ///
    /// Returns `Some(info)` if cursor initialization is pending, `None` otherwise.
    /// After calling this, `cursor_needs_initialization` is set to `false`.
    pub const fn take_pending_contenteditable_focus(
        &mut self,
    ) -> Option<PendingContentEditableFocus> {
        if self.cursor_needs_initialization {
            self.cursor_needs_initialization = false;
            self.pending_contenteditable_focus.take()
        } else {
            None
        }
    }

    /// Check if cursor initialization is pending.
    #[must_use]
    pub const fn needs_cursor_initialization(&self) -> bool {
        self.cursor_needs_initialization
    }

    // --- focus-before-first-layout retry queue ---

    /// Retain a focus target that had no layout to resolve against.
    ///
    /// A later request replaces an earlier one: only the most recent
    /// programmatic focus can win once layout arrives, exactly as it would if
    /// both had been resolvable immediately.
    pub fn defer_focus_target(&mut self, target: FocusTarget) {
        self.deferred_focus_target = Some(target);
    }

    /// Whether a focus target is waiting for the first layout.
    #[must_use]
    pub const fn has_deferred_focus_target(&self) -> bool {
        self.deferred_focus_target.is_some()
    }

    /// Take the deferred focus target (one-shot).
    pub const fn take_deferred_focus_target(&mut self) -> Option<FocusTarget> {
        self.deferred_focus_target.take()
    }
}

impl FocusManager {
    /// The nodes whose focus this manager dropped because a DOM rebuild did
    /// not carry them over, drained by whoever is in a position to tell them.
    ///
    /// Clearing the focus is right - the arena index now denotes a different
    /// element - but doing it as a plain field write was not: an app that
    /// commits a text field, closes a popup or validates on blur heard
    /// nothing at all when its focused node was unmounted, and no line was
    /// logged either, which is why this cost a live reconcile session to
    /// find.
    pub fn take_focus_lost_to_unmount(&mut self) -> Vec<DomNodeId> {
        core::mem::take(&mut self.focus_lost_to_unmount)
    }

    fn record_focus_lost_to_unmount(&mut self, lost: DomNodeId) {
        self.focus_lost_to_unmount.push(lost);
    }
}

impl crate::managers::NodeIdRemap for FocusManager {
    /// Remap the focused node AND the pending contenteditable focus.
    ///
    /// Focus on an unmounted node is CLEARED (not kept — the index now denotes a
    /// different element).
    fn remap_node_ids(&mut self, dom_id: DomId, map: &crate::managers::NodeIdMap) {
        // 0. the other seats' focus (9b-ii-a-i-d): the same rule as the
        // primary's - follow the node, clear on an unmounted one.
        let mut seats_lost: Vec<DomNodeId> = Vec::new();
        self.seat_focus.retain(|_, focused| {
            if focused.dom != dom_id {
                return true;
            }
            match focused
                .node
                .into_crate_internal()
                .and_then(|old| map.resolve(old))
            {
                Some(new_id) => {
                    focused.node = NodeHierarchyItemId::from_crate_internal(Some(new_id));
                    true
                }
                None => {
                    seats_lost.push(*focused);
                    false
                }
            }
        });
        for lost in seats_lost {
            self.record_focus_lost_to_unmount(lost);
        }
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
                    None => {
                        self.focused_node = None;
                        self.record_focus_lost_to_unmount(focused);
                    }
                }
            }
        }

        // 2. pending contenteditable focus (set during focus handling, consumed after layout — a
        //    DOM rebuild can land in between).
        if let Some(ref mut pending) = self.pending_contenteditable_focus {
            if pending.dom_id != dom_id {
                return;
            }
            if let (Some(container), Some(text)) = (
                map.resolve(pending.container_node_id),
                map.resolve(pending.text_node_id),
            ) {
                pending.container_node_id = container;
                pending.text_node_id = text;
            } else {
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
/// API but are NEVER part of the Tab order. The previous linear `NodeId` walk
/// both ignored positive-tabindex ordering and tabbed onto tabindex=-1 nodes.
fn collect_tab_order(
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    out_of_scope: &BTreeSet<DomId>,
) -> Vec<DomNodeId> {
    use azul_core::dom::TabIndex;
    let mut positive: Vec<(u32, DomNodeId)> = Vec::new();
    let mut auto: Vec<DomNodeId> = Vec::new();
    for (dom_id, layout) in layout_results {
        // A DOM that is not part of THIS window's focus scope contributes no
        // tab stops. A transient popup is a separate OS window whose content
        // DOM the PARENT lays out, so without this the parent's Tab order
        // walked straight into the popup - and kept doing so after the popup
        // closed, which stranded the keyboard there (device report,
        // 2026-09-01). Inside the popup's own window that same content is the
        // ROOT dom and hosts no popups, so it stays fully tabbable there.
        if out_of_scope.contains(dom_id) {
            continue;
        }
        // A `<transient-window>`'s SUBTREE is not rendered by this window: it
        // is the template the popup's own window shows, extracted into its own
        // DOM when the popup opens. The parent's copy is never on screen, so
        // it can hold no tab stops - Tab landed on it anyway (a ColorInput has
        // ONE real stop, the swatch, and EIGHT under its transient node), and
        // then arrow keys did nothing because the focused node was the
        // parent's dead copy rather than the live control the popup displays
        // (device report, 2026-09-01).
        let node_data = layout.styled_dom.node_data.as_container();
        let hierarchy = layout.styled_dom.node_hierarchy.as_container();
        let transient_roots: Vec<NodeId> = (0..node_data.len())
            .map(NodeId::new)
            .filter(|n| {
                node_data.get(*n).is_some_and(|nd| {
                    matches!(
                        nd.get_node_type(),
                        azul_core::dom::NodeType::TransientWindow(_)
                    )
                })
            })
            .collect();
        let under_transient = |mut n: NodeId| -> bool {
            if transient_roots.is_empty() {
                return false;
            }
            let mut guard = 0usize;
            while let Some(parent) = hierarchy
                .get(n)
                .and_then(azul_core::styled_dom::NodeHierarchyItem::parent_id)
            {
                guard += 1;
                if guard > 65_536 {
                    break;
                }
                if transient_roots.contains(&parent) {
                    return true;
                }
                n = parent;
            }
            false
        };
        for index in 0..node_data.len() {
            let node_id = NodeId::new(index);
            let Some(nd) = node_data.get(node_id) else {
                continue;
            };
            if !nd.is_focusable() {
                continue;
            }
            if under_transient(node_id) {
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
fn order_tab_entries(mut positive: Vec<(u32, DomNodeId)>, auto: Vec<DomNodeId>) -> Vec<DomNodeId> {
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
    #[allow(clippy::trivially_copy_pass_by_ref)] // <=8B Copy param kept by-ref intentionally (hot
                                                 // pixel/coord path or to avoid churning call sites
                                                 // for a perf-neutral change)
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
#[allow(clippy::trivially_copy_pass_by_ref)] // <=8B Copy param kept by-ref intentionally (hot
                                             // pixel/coord path or to avoid churning call sites for
                                             // a perf-neutral change)
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

/// The child DOMs `host` mounts through `VirtualView` items, transitively, in
/// display-list order.
///
/// Read off the host's display list — the same place
/// `headless::resolve_virtual_view_placements` reads it — because that is the
/// only host→child link available to a function that is handed nothing but
/// `layout_results`. Bounded by the number of layout results, so a cyclic or
/// self-referential item cannot spin.
fn nested_dom_ids(layout_results: &BTreeMap<DomId, DomLayoutResult>, host: DomId) -> Vec<DomId> {
    use crate::solver3::display_list::DisplayListItem;

    let mut out: Vec<DomId> = Vec::new();
    let mut queue: Vec<DomId> = vec![host];
    let mut head = 0usize;
    let limit = layout_results.len().saturating_add(1);
    while head < queue.len() && head < limit {
        let current = queue[head];
        head += 1;
        let Some(lr) = layout_results.get(&current) else {
            continue;
        };
        for item in &lr.display_list.items {
            if let DisplayListItem::VirtualView { child_dom_id, .. } = item {
                if *child_dom_id != host && !out.contains(child_dom_id) {
                    out.push(*child_dom_id);
                    queue.push(*child_dom_id);
                }
            }
        }
    }
    out
}

/// Resolve a `FocusTarget`, or QUEUE it when there is no layout to resolve
/// against yet.
///
/// This is the entry point every focus-changing caller should use.
/// [`resolve_focus_target`] used to be unable to tell "nothing matched" from "nothing exists
/// yet": both are `Ok(None)`, and callers apply that as "clear focus". A
/// `set_focus` issued from a `create` callback — which runs before the first
/// layout — was therefore dropped on the floor.
///
/// # Errors
///
/// Returns an `UpdateFocusWarning` if the focus target cannot be resolved.
pub fn resolve_focus_target_or_defer(
    focus_manager: &mut FocusManager,
    focus_target: &FocusTarget,
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    out_of_scope: &BTreeSet<DomId>,
) -> Result<FocusResolution, UpdateFocusWarning> {
    // `NoFocus` means the app WANTS focus cleared; that is answerable without
    // any layout and must not be queued (it would then fire later, clearing a
    // focus the app had meanwhile set).
    if layout_results.is_empty() && !matches!(focus_target, FocusTarget::NoFocus) {
        focus_manager.defer_focus_target(focus_target.clone());
        return Ok(FocusResolution::Deferred);
    }

    let current_focus = focus_manager.get_focused_node().copied();
    resolve_focus_target(focus_target, layout_results, current_focus, out_of_scope)
}

/// Resolve a `FocusTarget` to a [`FocusResolution`] (never `Deferred` —
/// that variant belongs to [`resolve_focus_target_or_defer`], which parks
/// targets that arrive before the first layout).
///
/// Layout only: a `Directional` target is searched over the STATIC geometry
/// (no scroll offsets, no transforms). A caller that has the live window uses
/// [`resolve_focus_target_in`] with the window's
/// [`SpatialNavigationEnv`] - `LayoutWindow::resolve_focus_target_live`.
///
/// # Errors
///
/// Returns an `UpdateFocusWarning` if the focus target names an invalid
/// dom/node.
pub fn resolve_focus_target(
    focus_target: &FocusTarget,
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    current_focus: Option<DomNodeId>,
    out_of_scope: &BTreeSet<DomId>,
) -> Result<FocusResolution, UpdateFocusWarning> {
    resolve_focus_target_in(
        &SpatialNavigationEnv::layout_only(layout_results, out_of_scope),
        focus_target,
        current_focus,
    )
}

/// [`resolve_focus_target`] against a [`SpatialNavigationEnv`]: the same
/// answer for every target but `Directional`, which searches the PAINTED
/// geometry the env describes.
///
/// # Errors
///
/// Returns an `UpdateFocusWarning` if the focus target names an invalid
/// dom/node.
pub fn resolve_focus_target_in(
    env: &SpatialNavigationEnv<'_>,
    focus_target: &FocusTarget,
    current_focus: Option<DomNodeId>,
) -> Result<FocusResolution, UpdateFocusWarning> {
    use azul_core::callbacks::FocusTarget::{
        Directional, First, Id, Last, Next, NoFocus, Path, Previous,
    };

    let layout_results = env.layout_results;
    let out_of_scope = env.out_of_scope;

    // The explicit clear is answerable without any layout and must stay
    // distinguishable from every kind of miss below.
    if matches!(focus_target, NoFocus) {
        return Ok(FocusResolution::ClearRequested);
    }
    if layout_results.is_empty() {
        return Ok(FocusResolution::NotFound);
    }

    let ctx = FocusSearchContext::new(layout_results);

    match focus_target {
        Path(FocusTargetPath { dom, css_path }) => {
            let layout = ctx.get_layout(dom)?;
            if let Some(found) = find_first_matching_focusable_node(layout, dom, css_path) {
                return Ok(FocusResolution::Resolved(found));
            }
            // A `VirtualView` mounts its callback's DOM as a SEPARATE layout
            // result under its own DomId. To the app that subtree is part of
            // the same document — it wrote it, it gets its edit callbacks —
            // but a selector resolved against the HOST dom alone could never
            // see it, so `set_focus_to_path` matched nothing and the caller's
            // focus request was applied as "clear focus".
            //
            // That is why an app whose editable content lives in a VirtualView
            // (AzWriter's page canvas) opened with no caret at all: its
            // startup focus targeted `.mw-doc`, which is a class on the page
            // content root inside the nested dom.
            for nested in nested_dom_ids(layout_results, *dom) {
                let Some(nested_layout) = layout_results.get(&nested) else {
                    continue;
                };
                if let Some(found) =
                    find_first_matching_focusable_node(nested_layout, &nested, css_path)
                {
                    return Ok(FocusResolution::Resolved(found));
                }
            }
            Ok(FocusResolution::NotFound)
        }

        // Programmatic focus by id. A node that can hold focus (tabindex -1
        // included: focusable, just not tabbable) takes it. One that cannot -
        // the wrapper an app named with `.with_id` around a widget's inner
        // field (a search TextInput's row) - hands it to its first focusable
        // descendant in document order, like HTML's `delegatesFocus`: the app
        // can only name the root `dom()` returned, never the field inside.
        // With no focusable descendant the node itself is focused, as before.
        Id(dom_node_id) => {
            let layout = ctx.get_layout(&dom_node_id.dom)?;
            let node_data = layout.styled_dom.node_data.as_container();
            let Some(node) = dom_node_id
                .node
                .into_crate_internal()
                .filter(|n| node_data.get(*n).is_some())
            else {
                return Err(UpdateFocusWarning::FocusInvalidNodeId(dom_node_id.node));
            };
            if node_data[node].is_focusable() {
                return Ok(FocusResolution::Resolved(*dom_node_id));
            }
            let hierarchy = layout.styled_dom.node_hierarchy.as_container();
            let delegate = (node.index() + 1..node_data.len())
                .map(NodeId::new)
                // Pre-order: the subtree is contiguous, so the first node
                // outside it ends the search.
                .take_while(|d| d.get_nearest_matching_parent(&hierarchy, |p| p == node).is_some())
                .find(|d| node_data[*d].is_focusable());
            Ok(FocusResolution::Resolved(delegate.map_or(*dom_node_id, |d| {
                DomNodeId {
                    dom: dom_node_id.dom,
                    node: NodeHierarchyItemId::from_crate_internal(Some(d)),
                }
            })))
        }

        // MWA-C-focus_cursor: sequential navigation goes through the W3C tab
        // order (positive tabindex ascending, then document order; -1
        // excluded) instead of the old raw-NodeId walk.
        Previous => Ok(next_in_tab_order(
            &collect_tab_order(layout_results, out_of_scope),
            current_focus,
            false,
        )
        .map_or(FocusResolution::NotFound, FocusResolution::Resolved)),

        // Directional (spatial) navigation: the css-nav-1 engine below. Same
        // candidate pool as Tab — `collect_tab_order` already honours
        // tabindex=-1, transient windows and out-of-scope DOMs, and a node
        // that cannot be tabbed to should not be reachable with an arrow key
        // either. Spatial navigation CONTAINERS (9a-i-b, 9a-i-b-i) are
        // searched innermost first, then outward, then the document.
        Directional(dir) => Ok(directional_focus_target(env, current_focus, *dir)
            .map_or(FocusResolution::NotFound, FocusResolution::Resolved)),

        Next => Ok(next_in_tab_order(
            &collect_tab_order(layout_results, out_of_scope),
            current_focus,
            true,
        )
        .map_or(FocusResolution::NotFound, FocusResolution::Resolved)),

        First => Ok(collect_tab_order(layout_results, out_of_scope)
            .first()
            .copied()
            .map_or(FocusResolution::NotFound, FocusResolution::Resolved)),

        Last => Ok(collect_tab_order(layout_results, out_of_scope)
            .last()
            .copied()
            .map_or(FocusResolution::NotFound, FocusResolution::Resolved)),

        // Handled by the early return above; kept for match exhaustiveness.
        NoFocus => Ok(FocusResolution::ClearRequested),
    }
}

// Trait Implementations for Event Filtering

impl azul_core::events::FocusManagerQuery for FocusManager {
    fn get_focused_node_for_seat(&self, seat_id: u64) -> Option<DomNodeId> {
        self.focused_node_for(seat_id)
    }

    fn get_focused_node_id(&self) -> Option<DomNodeId> {
        self.focused_node
    }
}

#[cfg(test)]
mod tab_order_tests {
    use super::*;

    #[test]
    fn a_transient_rearm_spends_no_retry_budget() {
        let mut fm = FocusManager::default();
        fm.set_pending_contenteditable_focus(DomId { inner: 0 }, NodeId::new(1), NodeId::new(2));
        for _ in 0..5 {
            let pending = fm.take_pending_contenteditable_focus().expect("armed");
            fm.rearm_pending_contenteditable_focus_transient(pending);
        }
        // The bounded rearm still has its FULL budget afterwards.
        let pending = fm.take_pending_contenteditable_focus().expect("armed");
        assert!(
            fm.rearm_pending_contenteditable_focus(pending),
            "retry 1 must be available"
        );
        let pending = fm.take_pending_contenteditable_focus().expect("armed");
        assert!(
            fm.rearm_pending_contenteditable_focus(pending),
            "retry 2 must be available"
        );
        let pending = fm.take_pending_contenteditable_focus().expect("armed");
        assert!(
            !fm.rearm_pending_contenteditable_focus(pending),
            "budget is 2"
        );
    }

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
        assert_eq!(
            next_in_tab_order(&order, Some(nid(0, 6)), true),
            Some(nid(0, 1))
        );
        assert_eq!(
            next_in_tab_order(&order, Some(nid(0, 1)), false),
            Some(nid(0, 6))
        );
        assert_eq!(
            next_in_tab_order(&order, Some(nid(0, 4)), true),
            Some(nid(0, 6))
        );
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
        assert_eq!(
            next_in_tab_order(&order, Some(nid(0, 5)), true),
            Some(nid(0, 6))
        );
        assert_eq!(
            next_in_tab_order(&order, Some(nid(0, 5)), false),
            Some(nid(0, 4))
        );
        // Past the last stop: wraps to first / last respectively.
        assert_eq!(
            next_in_tab_order(&order, Some(nid(0, 9)), true),
            Some(nid(0, 1))
        );
        assert_eq!(
            next_in_tab_order(&order, Some(nid(0, 0)), false),
            Some(nid(0, 6))
        );
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

    /// `DomLayoutResult` whose layout tree MIRRORS the DOM
    /// (`LayoutTree::mirroring_dom`): one unsized box per node, parented like
    /// the DOM - the structure the spatial-navigation container chain reads
    /// (`ScrollChain`) - and no geometry, so no real layout (and no font) is
    /// needed.
    fn layout_result(styled_dom: StyledDom) -> DomLayoutResult {
        let layout_tree = LayoutTree::mirroring_dom(&styled_dom);
        DomLayoutResult {
            styled_dom,
            layout_tree,
            calculated_positions: Vec::new(),
            viewport: LogicalRect::zero(),
            display_list: std::sync::Arc::new(DisplayList::default()),
            scroll_ids: HashMap::new(),
            scroll_id_to_node_id: HashMap::new(),
        }
    }

    /// A host display list that mounts `child` through a `VirtualView` item —
    /// the only host→child link `resolve_focus_target` can read.
    fn virtual_view_display_list(child: DomId) -> DisplayList {
        use azul_core::geom::{LogicalPosition, LogicalSize};

        use crate::solver3::display_list::{DisplayListItem, WindowLogicalRect};

        let bounds =
            WindowLogicalRect::new(LogicalPosition::zero(), LogicalSize::new(100.0, 100.0));
        let mut dl = DisplayList::default();
        dl.items.push(DisplayListItem::VirtualView {
            child_dom_id: child,
            bounds,
            clip_rect: bounds,
            content_offset: LogicalPosition::zero(),
        });
        dl
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

    /// A `<body>` with two buttons, then a contained panel holding two more.
    ///
    /// Flat pre-order indices: 0 body, 1 button, 2 button, 3 panel (the
    /// container), 4 button, 5 button.
    fn contain_fixture(contained: bool) -> StyledDom {
        use azul_css::{
            css::CssPropertyValue,
            props::{property::CssProperty, style::spatial_nav::StyleSpatialNavigationContain},
        };

        let mut panel = azul_core::dom::NodeData::create_div();
        if contained {
            panel.upsert_inline_css_property(CssProperty::SpatialNavigationContain(
                CssPropertyValue::Exact(StyleSpatialNavigationContain::Contain),
            ));
        }
        StyledDom::create_from_dom(
            Dom::create_body()
                .with_child(Dom::create_node(NodeType::Button))
                .with_child(Dom::create_node(NodeType::Button))
                .with_child(
                    Dom::create_from_data(panel)
                        .with_child(Dom::create_node(NodeType::Button))
                        .with_child(Dom::create_node(NodeType::Button)),
                ),
        )
    }

    /// `contain` marks the panel and nothing else, and it is found from a
    /// node INSIDE it - which is the lookup the resolver actually makes.
    #[test]
    fn contain_names_the_panel_and_only_when_declared() {
        let contained = window(vec![(dom(0), contain_fixture(true))]);
        let panel = nid(0, 3);
        let inside = nid(0, 4);

        assert_eq!(
            spatial_navigation_container(&contained, inside),
            Some(panel),
            "a node inside the panel must resolve to the panel",
        );
        // Self-inclusive: the panel is its own container.
        assert_eq!(spatial_navigation_container(&contained, panel), Some(panel));
        // A sibling OUTSIDE the panel is in no container.
        assert_eq!(spatial_navigation_container(&contained, nid(0, 1)), None);

        // Without the declaration a PLAIN div is still no container: `auto`
        // makes a container of a scroll container and of nothing else.
        let plain = window(vec![(dom(0), contain_fixture(false))]);
        assert_eq!(spatial_navigation_container(&plain, inside), None);
    }

    /// [`contain_fixture`]'s shape with `overflow-y` on the panel instead of
    /// `contain`, so the panel is a container under `auto` only if it is a
    /// scroll container.
    fn scroll_fixture(overflow: azul_css::props::layout::overflow::LayoutOverflow) -> StyledDom {
        use azul_css::{css::CssPropertyValue, props::property::CssProperty};

        let mut panel = azul_core::dom::NodeData::create_div();
        panel.upsert_inline_css_property(CssProperty::OverflowY(CssPropertyValue::Exact(overflow)));
        StyledDom::create_from_dom(
            Dom::create_body()
                .with_child(Dom::create_node(NodeType::Button))
                .with_child(Dom::create_node(NodeType::Button))
                .with_child(
                    Dom::create_from_data(panel)
                        .with_child(Dom::create_node(NodeType::Button))
                        .with_child(Dom::create_node(NodeType::Button)),
                ),
        )
    }

    /// `auto` (the initial value) makes a container of a SCROLL CONTAINER -
    /// `css-nav-1`, per the user's ruling (9a-i-b-i) - and `visible` does not.
    #[test]
    fn auto_makes_a_container_of_a_scroll_container_only() {
        use azul_css::props::layout::overflow::LayoutOverflow;

        let panel = nid(0, 3);
        let inside = nid(0, 4);
        for scrolling in [
            LayoutOverflow::Auto,
            LayoutOverflow::Scroll,
            LayoutOverflow::Hidden,
        ] {
            let w = window(vec![(dom(0), scroll_fixture(scrolling))]);
            assert_eq!(
                spatial_navigation_container(&w, inside),
                Some(panel),
                "overflow: {scrolling:?} is a scroll container, so a container under auto"
            );
        }
        let w = window(vec![(dom(0), scroll_fixture(LayoutOverflow::Visible))]);
        assert_eq!(spatial_navigation_container(&w, inside), None);
    }

    /// The CHAIN is innermost-first and lists every container on the way up:
    /// body > `contain` panel > scrolling inner box > button.
    ///
    /// Flat pre-order indices: 0 body, 1 outer (contain), 2 inner (overflow),
    /// 3 button.
    #[test]
    fn the_container_chain_runs_innermost_first_to_the_outermost() {
        use azul_css::{
            css::CssPropertyValue,
            props::{
                layout::overflow::LayoutOverflow, property::CssProperty,
                style::spatial_nav::StyleSpatialNavigationContain,
            },
        };

        let mut outer = azul_core::dom::NodeData::create_div();
        outer.upsert_inline_css_property(CssProperty::SpatialNavigationContain(
            CssPropertyValue::Exact(StyleSpatialNavigationContain::Contain),
        ));
        let mut inner = azul_core::dom::NodeData::create_div();
        inner.upsert_inline_css_property(CssProperty::OverflowY(CssPropertyValue::Exact(
            LayoutOverflow::Auto,
        )));
        let sd = StyledDom::create_from_dom(Dom::create_body().with_child(
            Dom::create_from_data(outer).with_child(
                Dom::create_from_data(inner).with_child(Dom::create_node(NodeType::Button)),
            ),
        ));
        let w = window(vec![(dom(0), sd)]);
        assert_eq!(
            spatial_navigation_containers(&w, nid(0, 3)),
            vec![nid(0, 2), nid(0, 1)],
            "innermost (the scroll box) first, then the contain panel; the body is implicit"
        );
        assert_eq!(spatial_navigation_containers(&w, nid(0, 0)), Vec::new());
    }

    /// Containment is a SUBTREE test, not a "same parent" test, and it must
    /// never say yes across DOMs.
    #[test]
    fn is_within_covers_self_descendants_and_rejects_other_trees() {
        let w = window(vec![
            (dom(0), contain_fixture(true)),
            (dom(1), contain_fixture(true)),
        ]);
        let panel = nid(0, 3);

        assert!(is_within(&w, panel, panel), "a container contains itself");
        assert!(is_within(&w, nid(0, 4), panel));
        assert!(is_within(&w, nid(0, 5), panel));
        assert!(!is_within(&w, nid(0, 1), panel), "a sibling is not inside");
        assert!(
            !is_within(&w, nid(0, 0), panel),
            "an ancestor is not inside"
        );

        // Same index, different DOM: a VirtualView page is a separate tree and
        // treating it as contained would let the filter admit everything.
        assert!(!is_within(&w, nid(1, 4), panel));
    }

    /// The pool an arrow searches narrows to the container, and WIDENS AGAIN
    /// when nothing inside answers - the spec's move-to-the-parent-container
    /// step. Without the widening, an arrow at the edge of a panel would die
    /// there instead of escaping.
    #[test]
    fn a_contained_arrow_prefers_the_panel_but_can_still_escape() {
        let w = window(vec![(dom(0), contain_fixture(true))]);
        let inside = nid(0, 4);

        let resolved = resolve_focus_target(
            &FocusTarget::Directional(azul_core::callbacks::FocusDirection::Down),
            &w,
            Some(inside),
            &BTreeSet::new(),
        );
        // This fixture has no laid-out boxes, so the engine falls back to
        // "the first candidate of the innermost container that has one" -
        // which is exactly what makes the POOL observable here.
        let Ok(FocusResolution::Resolved(target)) = resolved else {
            panic!("expected a resolution, got {resolved:?}");
        };
        assert!(
            is_within(&w, target, nid(0, 3)),
            "a contained search must land inside the panel, got {target:?}",
        );

        // From OUTSIDE the panel there is no container, so the whole document
        // is in scope and the first tab stop wins.
        let outside = nid(0, 1);
        let resolved = resolve_focus_target(
            &FocusTarget::Directional(azul_core::callbacks::FocusDirection::Down),
            &w,
            Some(outside),
            &BTreeSet::new(),
        );
        let Ok(FocusResolution::Resolved(target)) = resolved else {
            panic!("expected a resolution, got {resolved:?}");
        };
        assert!(
            !is_within(&w, target, nid(0, 3)) || target == nid(0, 3),
            "an uncontained search must not be confined to the panel, got {target:?}",
        );
    }

    fn tab_order_of(fixture: StyledDom) -> Vec<DomNodeId> {
        collect_tab_order(&window(vec![(dom(0), fixture)]), &BTreeSet::new())
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
    fn seat_focus_is_independent_of_the_primary() {
        // 9b-ii-a-i-d: seat 0 IS `focused_node`; seat 7 keeps its own entry.
        let mut fm = FocusManager::new();
        fm.set_focused_node_for(azul_core::window::PRIMARY_POINTER_SEAT, Some(nid(0, 1)));
        fm.set_focused_node_for(7, Some(nid(0, 2)));
        assert_eq!(fm.get_focused_node(), Some(&nid(0, 1)));
        assert_eq!(fm.focused_node_for(0), Some(nid(0, 1)));
        assert_eq!(fm.focused_node_for(7), Some(nid(0, 2)));
        assert_eq!(
            fm.focused_node_for(8),
            None,
            "an unknown seat focuses nothing"
        );
        assert!(fm.has_focus_for(7, &nid(0, 2)));
        assert!(!fm.has_focus_for(7, &nid(0, 1)));
        assert_eq!(fm.seats_focusing(&nid(0, 2)), vec![7]);
        // Clearing the seat leaves the primary alone, and the other way round.
        fm.set_focused_node_for(7, None);
        assert_eq!(fm.focused_node_for(7), None);
        assert_eq!(fm.focused_node_for(0), Some(nid(0, 1)));
        fm.clear_focus();
        fm.set_focused_node_for(7, Some(nid(0, 3)));
        assert_eq!(fm.focused_node_for(0), None);
        assert_eq!(fm.focused_node_for(7), Some(nid(0, 3)));
    }

    #[test]
    fn seat_focus_follows_a_remap_and_clears_on_an_unmounted_node() {
        let mut fm = FocusManager::new();
        fm.set_focused_node_for(7, Some(nid(0, 5)));
        fm.set_focused_node_for(9, Some(nid(0, 6)));
        fm.remap_node_ids(
            dom(0),
            &NodeIdMap::from_pairs([(NodeId::new(5), NodeId::new(2))]),
        );
        assert_eq!(fm.focused_node_for(7), Some(nid(0, 2)), "followed the node");
        assert_eq!(fm.focused_node_for(9), None, "unmounted: cleared, not kept");
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
        assert_eq!(
            doc_order_key(&nid(usize::MAX, usize::MAX - 1)),
            (usize::MAX, usize::MAX - 1)
        );
    }

    // ==================================================================
    // next_in_tab_order
    // ==================================================================

    #[test]
    fn next_in_tab_order_single_entry_wraps_onto_itself() {
        // `(0 + 1) % 1 == 0` — must terminate on itself, not loop or panic.
        let order = vec![nid(0, 1)];
        assert_eq!(
            next_in_tab_order(&order, Some(nid(0, 1)), true),
            Some(nid(0, 1))
        );
        assert_eq!(
            next_in_tab_order(&order, Some(nid(0, 1)), false),
            Some(nid(0, 1))
        );
    }

    #[test]
    fn next_in_tab_order_duplicate_entries_resolve_to_first_position() {
        // `position()` finds the FIRST occurrence, so a duplicated tab stop makes
        // the trailing copy unreachable by stepping. Pin it (a dedup in
        // `collect_tab_order` would change this).
        let order = vec![nid(0, 1), nid(0, 2), nid(0, 1)];
        assert_eq!(
            next_in_tab_order(&order, Some(nid(0, 1)), true),
            Some(nid(0, 2))
        );
        // Backward from index 0 wraps to the last element.
        assert_eq!(
            next_in_tab_order(&order, Some(nid(0, 1)), false),
            Some(nid(0, 1))
        );
    }

    #[test]
    fn next_in_tab_order_unknown_current_uses_cross_dom_document_order() {
        // Current node lives in DOM 1; the tab order is split across DOM 0 and 2.
        let order = vec![nid(0, 5), nid(2, 1)];
        assert_eq!(
            next_in_tab_order(&order, Some(nid(1, 0)), true),
            Some(nid(2, 1))
        );
        assert_eq!(
            next_in_tab_order(&order, Some(nid(1, 9)), false),
            Some(nid(0, 5))
        );
    }

    #[test]
    fn next_in_tab_order_unknown_current_past_both_ends_wraps() {
        let order = vec![nid(1, 2), nid(1, 4)];
        // Nothing greater -> wrap to first.
        assert_eq!(
            next_in_tab_order(&order, Some(nid(9, 9)), true),
            Some(nid(1, 2))
        );
        // Nothing smaller -> wrap to last.
        assert_eq!(
            next_in_tab_order(&order, Some(nid(0, 0)), false),
            Some(nid(1, 4))
        );
    }

    #[test]
    fn next_in_tab_order_null_current_node_is_deterministic() {
        // The sentinel keys as (dom, 0); it is not in the order, so the re-entry
        // path runs. It must produce a stable answer, not panic.
        let order = vec![nid(0, 1), nid(0, 3)];
        assert_eq!(
            next_in_tab_order(&order, Some(null_nid(0)), true),
            Some(nid(0, 1))
        );
        assert_eq!(
            next_in_tab_order(&order, Some(null_nid(0)), false),
            Some(nid(0, 3))
        );
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
        assert_eq!(
            collect_tab_order(&BTreeMap::new(), &BTreeSet::new()),
            Vec::new()
        );
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
        assert!(
            !order.contains(&nid(0, 4)),
            "tabindex=-1 must not be a tab stop"
        );
        // ...and the plain, non-focusable div is absent too.
        assert!(!order.contains(&nid(0, 1)));
        // ...while the body itself is never a tab stop.
        assert!(!order.contains(&nid(0, 0)));
    }

    #[test]
    fn collect_tab_order_huge_tabindex_truncates_at_28_bits() {
        // `NodeFlags` packs the tabindex into 28 bits, so:
        //   * tabindex = u32::MAX  -> stored as 2^28-1  -> still POSITIVE
        //   * tabindex = 1 << 28   -> stored as 0       -> demoted to the AUTO bucket (0 is not >
        //     0)
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
            "u32::MAX stays positive (sorts after tabindex=1); 1<<28 truncates to 0 and falls \
             back into the auto bucket behind every positive"
        );
    }

    /// A TRANSIENT POPUP's content DOM contributes NO tab stops to the
    /// parent window's order.
    ///
    /// The parent lays the popup's content out and keeps it in
    /// `layout_results`, so without an explicit scope the parent's Tab walked
    /// straight into the popup - and kept walking into it after the popup had
    /// CLOSED, which stranded the keyboard there: Tab appeared to stop
    /// working entirely (device report, 2026-09-01). Inside the popup's own
    /// window that same content is the ROOT dom and hosts no popups, so its
    /// scope is empty and it stays fully tabbable there.
    /// A `<transient-window>`'s SUBTREE holds no tab stops in the parent.
    ///
    /// The popup's controls live in the PARENT's DOM as children of the
    /// transient node - a ColorInput has ONE real stop (the swatch) and EIGHT
    /// under its transient node - and that copy is never rendered by the
    /// parent window. Tab landed on it anyway, and arrow keys then did
    /// nothing, because the focused node was the parent's dead copy rather
    /// than the live control the popup window shows (device report,
    /// 2026-09-01).
    #[test]
    fn collect_tab_order_skips_everything_under_a_transient_window() {
        use azul_core::dom::{Dom, NodeType, TabIndex};

        let stop = || {
            let mut d = Dom::create_div();
            d.set_tab_index(TabIndex::Auto);
            d
        };
        // body > [ real stop, transient-window > [ hidden stop, hidden stop ] ]
        let sd = StyledDom::create_from_dom(
            Dom::create_body().with_child(stop()).with_child(
                Dom::create_node(NodeType::TransientWindow(Default::default()))
                    .with_child(stop())
                    .with_child(stop()),
            ),
        );

        let order = collect_tab_order(&window(vec![(dom(0), sd)]), &BTreeSet::new());
        assert_eq!(
            order,
            vec![nid(0, 1)],
            "only the node OUTSIDE the transient subtree is a tab stop, got {order:?}",
        );
    }

    #[test]
    fn collect_tab_order_skips_doms_outside_this_windows_focus_scope() {
        let results = window(vec![(dom(0), tab_fixture()), (dom(1), tab_fixture())]);

        let both = collect_tab_order(&results, &BTreeSet::new());
        assert!(
            both.iter().any(|n| n.dom == dom(1)),
            "premise: with an empty scope BOTH doms contribute stops",
        );

        // Dom 1 is a popup hosted by this window.
        let popup: BTreeSet<DomId> = [dom(1)].into_iter().collect();
        let parent_only = collect_tab_order(&results, &popup);
        assert!(
            parent_only.iter().all(|n| n.dom == dom(0)),
            "the popup's dom must contribute no tab stops to its parent: {parent_only:?}",
        );
        assert!(
            !parent_only.is_empty(),
            "and the parent's own stops must survive",
        );
    }

    #[test]
    fn collect_tab_order_tab_order_is_global_across_doms() {
        // A positive-tabindex node in DOM 1 must outrank an auto node in DOM 0:
        // the tab order is a single sequence over all DOMs, not per-DOM chunks.
        let order = collect_tab_order(
            &window(vec![(dom(0), tab_fixture()), (dom(1), tab_fixture())]),
            &BTreeSet::new(),
        );

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
        // even a structurally invalid target resolves to `Ok(FocusResolution::NotFound)` — never
        // Err, never a panic.
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
        ];
        for t in targets {
            assert_eq!(
                resolve_focus_target(&t, &empty, Some(nid(0, 1)), &BTreeSet::new()),
                Ok(FocusResolution::NotFound),
                "empty window must short-circuit {t:?}"
            );
        }
        // The explicit clear stays answerable — and distinguishable — even
        // with no layout at all.
        assert_eq!(
            resolve_focus_target(
                &FocusTarget::NoFocus,
                &empty,
                Some(nid(0, 1)),
                &BTreeSet::new()
            ),
            Ok(FocusResolution::ClearRequested),
        );
    }

    #[test]
    fn resolve_focus_target_id_rejects_unknown_dom() {
        let results = window(vec![(dom(0), tab_fixture())]);
        assert_eq!(
            resolve_focus_target(
                &FocusTarget::Id(nid(1, 2)),
                &results,
                None,
                &BTreeSet::new()
            ),
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
                resolve_focus_target(&FocusTarget::Id(target), &results, None, &BTreeSet::new()),
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
            resolve_focus_target(&FocusTarget::Id(target), &results, None, &BTreeSet::new()),
            Err(UpdateFocusWarning::FocusInvalidNodeId(target.node))
        );
    }

    /// Programmatic focus by id: a focusable node takes it (tabindex=-1 too:
    /// focusable, just not tabbable); one that cannot hold focus hands it to
    /// its first focusable descendant (HTML's `delegatesFocus`), and keeps it
    /// only when it has none.
    #[test]
    fn resolve_focus_target_id_delegates_an_unfocusable_node_to_its_first_focusable_descendant() {
        // 0 body { 1 div, 2 button, 3 tabindex=2, 4 tabindex=-1, ... }
        let results = window(vec![(dom(0), tab_fixture())]);
        let resolve = |n| {
            resolve_focus_target(
                &FocusTarget::Id(nid(0, n)),
                &results,
                None,
                &BTreeSet::new(),
            )
        };
        // The body cannot hold focus: its first focusable descendant (the
        // button, not the plain div before it) takes it.
        assert_eq!(resolve(0), Ok(FocusResolution::Resolved(nid(0, 2))));
        assert_eq!(resolve(4), Ok(FocusResolution::Resolved(nid(0, 4))));
        // A plain div with no focusable descendant keeps the focus.
        assert_eq!(resolve(1), Ok(FocusResolution::Resolved(nid(0, 1))));
    }

    /// The search for a delegate stays inside the named node's subtree: a
    /// focusable node AFTER it in document order is not its descendant.
    #[test]
    fn resolve_focus_target_id_never_delegates_to_a_node_outside_the_subtree() {
        // 0 body { 1 panel { 2 div }, 3 button }
        let sd = StyledDom::create_from_dom(
            Dom::create_body()
                .with_child(Dom::create_div().with_child(Dom::create_div()))
                .with_child(Dom::create_node(NodeType::Button)),
        );
        let results = window(vec![(dom(0), sd)]);
        assert_eq!(
            resolve_focus_target(
                &FocusTarget::Id(nid(0, 1)),
                &results,
                None,
                &BTreeSet::new()
            ),
            Ok(FocusResolution::Resolved(nid(0, 1)))
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
            resolve_focus_target(&target, &results, None, &BTreeSet::new()),
            Err(UpdateFocusWarning::FocusInvalidDomId(dom(3)))
        );
    }

    #[test]
    fn resolve_focus_target_path_with_no_match_is_ok_none_not_err() {
        // NOTE: the doc comment on `find_first_matching_focusable_node` advertises
        // `Err(_)` for an unmatchable path, but the implementation returns
        // `Ok(FocusResolution::NotFound)`. Pin the IMPLEMENTED behaviour (a miss is not an error);
        // the doc comment is what is wrong here.
        let results = window(vec![(dom(0), tab_fixture())]);
        let target = FocusTarget::Path(FocusTargetPath {
            dom: dom(0),
            css_path: class_path("no-such-class"),
        });
        assert_eq!(
            resolve_focus_target(&target, &results, None, &BTreeSet::new()),
            Ok(FocusResolution::NotFound)
        );
    }

    /// A `VirtualView` mounts its DOM under its OWN DomId, so a selector the
    /// app resolves against the host dom must still find it — otherwise an app
    /// whose editable content lives in a VirtualView can never focus it by
    /// path, and the request lands as "clear focus".
    ///
    /// This is AzWriter's startup caret: `set_focus_to_path(ROOT, ".mw-doc")`,
    /// where `.mw-doc` is on the page content root inside the nested dom.
    #[test]
    fn resolve_focus_target_path_reaches_into_a_virtual_views_nested_dom() {
        let host = StyledDom::create_from_dom(Dom::create_body().with_child(Dom::create_div()));
        let mut page = Dom::create_div();
        page.set_contenteditable(true);
        let nested =
            StyledDom::create_from_dom(Dom::create_body().with_child(page.with_ids_and_classes(
                vec![azul_core::dom::IdOrClass::Class("mw-doc".into())].into(),
            )));

        let mut results = window(vec![(dom(0), host), (dom(1), nested)]);
        results.get_mut(&dom(0)).unwrap().display_list =
            std::sync::Arc::new(virtual_view_display_list(dom(1)));

        let target = FocusTarget::Path(FocusTargetPath {
            dom: dom(0),
            css_path: class_path("mw-doc"),
        });
        assert_eq!(
            resolve_focus_target(&target, &results, None, &BTreeSet::new()),
            Ok(FocusResolution::Resolved(nid(1, 1))),
            "the selector must resolve into the dom the VirtualView mounted; answering None here \
             is applied by every caller as 'clear focus', which is why the editor opened with no \
             caret"
        );
    }

    #[test]
    fn resolve_focus_target_path_still_prefers_the_host_dom_over_a_nested_one() {
        // Both doms carry `.target`; the host's match wins, so adding the
        // nested search cannot steal a focus that already resolved.
        let host = StyledDom::create_from_dom(
            Dom::create_body().with_child(
                Dom::create_div()
                    .with_tab_index(TabIndex::Auto)
                    .with_ids_and_classes(
                        vec![azul_core::dom::IdOrClass::Class("target".into())].into(),
                    ),
            ),
        );
        let nested = StyledDom::create_from_dom(
            Dom::create_body().with_child(
                Dom::create_div()
                    .with_tab_index(TabIndex::Auto)
                    .with_ids_and_classes(
                        vec![azul_core::dom::IdOrClass::Class("target".into())].into(),
                    ),
            ),
        );
        let mut results = window(vec![(dom(0), host), (dom(1), nested)]);
        results.get_mut(&dom(0)).unwrap().display_list =
            std::sync::Arc::new(virtual_view_display_list(dom(1)));

        let target = FocusTarget::Path(FocusTargetPath {
            dom: dom(0),
            css_path: class_path("target"),
        });
        assert_eq!(
            resolve_focus_target(&target, &results, None, &BTreeSet::new()),
            Ok(FocusResolution::Resolved(nid(0, 1)))
        );
    }

    #[test]
    fn resolve_focus_target_no_focus_is_an_explicit_clear() {
        let results = window(vec![(dom(0), tab_fixture())]);
        assert_eq!(
            resolve_focus_target(
                &FocusTarget::NoFocus,
                &results,
                Some(nid(0, 5)),
                &BTreeSet::new()
            ),
            Ok(FocusResolution::ClearRequested),
            "the app's explicit clear must stay distinguishable from a miss"
        );
    }

    #[test]
    fn resolve_focus_target_first_and_last_are_the_tab_order_ends() {
        let results = window(vec![(dom(0), tab_fixture())]);
        // Tab order is [5, 3, 2, 6, 7].
        assert_eq!(
            resolve_focus_target(&FocusTarget::First, &results, None, &BTreeSet::new()),
            Ok(FocusResolution::Resolved(nid(0, 5))),
            "First must be the lowest positive tabindex, not document node 0"
        );
        assert_eq!(
            resolve_focus_target(&FocusTarget::Last, &results, None, &BTreeSet::new()),
            Ok(FocusResolution::Resolved(nid(0, 7)))
        );
        // `current_focus` must not influence First/Last.
        assert_eq!(
            resolve_focus_target(
                &FocusTarget::First,
                &results,
                Some(nid(0, 7)),
                &BTreeSet::new()
            ),
            Ok(FocusResolution::Resolved(nid(0, 5)))
        );
    }

    #[test]
    fn resolve_focus_target_first_and_last_on_unfocusable_dom_are_none() {
        let sd = StyledDom::create_from_dom(Dom::create_body().with_child(Dom::create_div()));
        let results = window(vec![(dom(0), sd)]);
        assert_eq!(
            resolve_focus_target(&FocusTarget::First, &results, None, &BTreeSet::new()),
            Ok(FocusResolution::NotFound)
        );
        assert_eq!(
            resolve_focus_target(&FocusTarget::Last, &results, None, &BTreeSet::new()),
            Ok(FocusResolution::NotFound)
        );
        assert_eq!(
            resolve_focus_target(&FocusTarget::Next, &results, None, &BTreeSet::new()),
            Ok(FocusResolution::NotFound)
        );
        assert_eq!(
            resolve_focus_target(
                &FocusTarget::Previous,
                &results,
                Some(nid(0, 0)),
                &BTreeSet::new()
            ),
            Ok(FocusResolution::NotFound)
        );
    }

    #[test]
    fn resolve_focus_target_next_and_previous_wrap_around_the_tab_order() {
        let results = window(vec![(dom(0), tab_fixture())]);
        // Tab order [5, 3, 2, 6, 7]: stepping off either end wraps.
        assert_eq!(
            resolve_focus_target(
                &FocusTarget::Next,
                &results,
                Some(nid(0, 7)),
                &BTreeSet::new()
            ),
            Ok(FocusResolution::Resolved(nid(0, 5)))
        );
        assert_eq!(
            resolve_focus_target(
                &FocusTarget::Previous,
                &results,
                Some(nid(0, 5)),
                &BTreeSet::new()
            ),
            Ok(FocusResolution::Resolved(nid(0, 7)))
        );
        // ...and step normally in the middle.
        assert_eq!(
            resolve_focus_target(
                &FocusTarget::Next,
                &results,
                Some(nid(0, 3)),
                &BTreeSet::new()
            ),
            Ok(FocusResolution::Resolved(nid(0, 2)))
        );
        assert_eq!(
            resolve_focus_target(
                &FocusTarget::Previous,
                &results,
                Some(nid(0, 2)),
                &BTreeSet::new()
            ),
            Ok(FocusResolution::Resolved(nid(0, 3)))
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
            resolve_focus_target(
                &FocusTarget::Previous,
                &results,
                Some(nid(0, 4)),
                &BTreeSet::new()
            ),
            Ok(FocusResolution::Resolved(nid(0, 3)))
        );
        // Focus on the plain, non-focusable div (index 1): Tab goes to the next
        // tab stop in DOCUMENT order (node 2, the button) — not to the tab
        // order's first entry (node 5).
        assert_eq!(
            resolve_focus_target(
                &FocusTarget::Next,
                &results,
                Some(nid(0, 1)),
                &BTreeSet::new()
            ),
            Ok(FocusResolution::Resolved(nid(0, 2)))
        );
    }

    #[test]
    fn resolve_focus_target_next_from_a_stale_removed_node_never_panics() {
        // Focus left over from a previous DOM whose node index no longer exists:
        // resolution must still yield a valid tab stop rather than panic.
        let results = window(vec![(dom(0), tab_fixture())]);
        let stale = nid(0, 9_999);
        assert_eq!(
            resolve_focus_target(&FocusTarget::Next, &results, Some(stale), &BTreeSet::new()),
            Ok(FocusResolution::Resolved(nid(0, 5))),
            "no tab stop past node 9999 -> wrap to the first"
        );
        // A stale node in a DOM that isn't even mounted.
        let alien = nid(5, 1);
        assert_eq!(
            resolve_focus_target(&FocusTarget::Next, &results, Some(alien), &BTreeSet::new()),
            Ok(FocusResolution::Resolved(nid(0, 5)))
        );
        assert_eq!(
            resolve_focus_target(
                &FocusTarget::Previous,
                &results,
                Some(alien),
                &BTreeSet::new()
            ),
            Ok(FocusResolution::Resolved(nid(0, 7)))
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
            vec![(NodeId::new(3), NodeId::new(10))], // text node unmapped
            vec![(NodeId::new(4), NodeId::new(11))], // container unmapped
            vec![],                                  // neither survives
        ] {
            let mut fm = FocusManager::new();
            fm.set_pending_contenteditable_focus(dom(0), NodeId::new(3), NodeId::new(4));
            fm.remap_node_ids(dom(0), &NodeIdMap::from_pairs(pairs));

            assert!(!fm.needs_cursor_initialization());
            assert_eq!(fm.pending_contenteditable_focus, None);
        }
    }
}

/// Every spatial navigation container on the way up from `from`, INNERMOST
/// FIRST - the chain `css-nav-1` searches outward through (9a-i-b-i).
///
/// A node is a container when it declares `spatial-navigation-contain:
/// contain`, or - under `auto`, the initial value - when it is a SCROLL
/// CONTAINER (`overflow` other than `visible`/`clip` on either axis). That is
/// the spec's rule, and the user's ruling (2026-09-03) after 9a-i-a had held
/// it back. The document itself is the outermost container and is not
/// listed: the caller falls back to the whole candidate pool after the chain,
/// so an arrow at the edge of the innermost box still escapes it.
///
/// A scroll container counts only when `from` is painted in it: a link of
/// `from`'s `ScrollChain` (by CONTAINING BLOCK). An `absolute` box escapes
/// a non-positioned scroll box, a `fixed` box every one; the DOM walk
/// searched such a box inside a scroller it is not clipped or scrolled by.
/// `contain` is the author's grouping and stays a DOM-ancestor rule; the
/// walk up the DOM only orders the two (every chain link is a DOM ancestor).
///
/// Self-inclusive: `contain` on the focused node, or a focused scroll
/// container, counts as its own innermost container.
fn spatial_navigation_containers(
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    from: DomNodeId,
) -> Vec<DomNodeId> {
    use azul_core::spaces::Inclusivity;
    use azul_css::props::style::spatial_nav::StyleSpatialNavigationContain;

    use crate::solver3::{
        getters::{get_spatial_navigation_contain, MultiValue},
        scroll_chain::{is_css_scroll_container, ScrollChain},
    };

    let mut chain = Vec::new();
    let Some(lr) = layout_results.get(&from.dom) else {
        return chain;
    };
    let hierarchy = lr.styled_dom.node_hierarchy.as_container();
    let states = lr.styled_dom.styled_nodes.as_container();
    let Some(mut node) = from.node.into_crate_internal() else {
        return chain;
    };
    // The scroll containers `from` is painted in.
    let scrolled_in: Vec<NodeId> = ScrollChain::of_node(
        &lr.layout_tree,
        &lr.styled_dom,
        &lr.scroll_ids,
        node,
        Inclusivity::SelfAndAncestors,
    )
    .map(|scroll_chain| {
        scroll_chain
            .links
            .iter()
            .map(|link| link.node)
            .filter(|n| is_css_scroll_container(&lr.styled_dom, *n))
            .collect()
    })
    .unwrap_or_default();
    // Bounded by the node count: a corrupt hierarchy whose parent chain loops
    // must not hang the event loop, and a valid chain can never be longer.
    for _ in 0..hierarchy.internal.len().saturating_add(1) {
        if let Some(sn) = states.get(node) {
            let state = &sn.styled_node_state;
            let is_container = match get_spatial_navigation_contain(&lr.styled_dom, node, state) {
                MultiValue::Exact(StyleSpatialNavigationContain::Contain) => true,
                // `auto`, and unset (whose initial value is `auto`): a
                // container exactly when the box is a scroll container
                // `from` is painted in.
                _ => scrolled_in.contains(&node),
            };
            if is_container {
                chain.push(DomNodeId {
                    dom: from.dom,
                    node: NodeHierarchyItemId::from_crate_internal(Some(node)),
                });
            }
        }
        match hierarchy
            .get(node)
            .and_then(azul_core::styled_dom::NodeHierarchyItem::parent_id)
        {
            Some(parent) => node = parent,
            None => break,
        }
    }
    chain
}

/// The INNERMOST spatial navigation container of `from`, if any - the first
/// entry of [`spatial_navigation_containers`].
fn spatial_navigation_container(
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    from: DomNodeId,
) -> Option<DomNodeId> {
    spatial_navigation_containers(layout_results, from)
        .first()
        .copied()
}

/// Is `candidate` inside `container` (or the container itself)?
///
/// Cross-DOM candidates are never inside: a containing panel and a
/// `VirtualView`'s child document are different trees, and treating a node in
/// another DOM as contained would let the filter admit everything.
fn is_within(
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    candidate: DomNodeId,
    container: DomNodeId,
) -> bool {
    if candidate.dom != container.dom {
        return false;
    }
    let (Some(cand), Some(cont)) = (
        candidate.node.into_crate_internal(),
        container.node.into_crate_internal(),
    ) else {
        return false;
    };
    if cand == cont {
        return true;
    }
    let Some(lr) = layout_results.get(&candidate.dom) else {
        return false;
    };
    let hierarchy = lr.styled_dom.node_hierarchy.as_container();
    cand.get_nearest_matching_parent(&hierarchy, |n| n == cont)
        .is_some()
}

// ===========================================================================
// CSS Spatial Navigation Level 1 - the engine
// ===========================================================================
//
// ONE engine answers every spatial question, so no two of them can disagree:
//
// - what an ARROW KEY does: [`spatial_navigation_steps`], read by the keyboard default action
//   (focus, scroll, or nothing);
// - where a programmatic directional move lands: [`directional_focus_target`], behind
//   `FocusTarget::Directional` and the gamepad D-pad.
//
// Both search the same candidate pool (the Tab order), through the same
// container chain, with the same selection rule, over the same PAINTED
// geometry.

/// Everything a spatial-navigation search reads besides the layout.
///
/// css-nav-1 §8.4: "All geometrical operations ... work on the result of CSS
/// layout, including all graphical transformations". `layout_results` alone
/// is the STATIC geometry. Where a box is on screen also depends on the
/// scroll offset of every scroll container above it and on transforms, and
/// those live in the window's managers, so they are handed in here.
///
/// `LayoutWindow::with_spatial_navigation_env` builds the live one.
/// [`SpatialNavigationEnv::layout_only`] is the model for a caller that has
/// nothing but layout results: every scroll offset is zero, no transform
/// applies, and a box "can scroll" forward exactly when layout gave it a bar
/// on that axis.
#[derive(Clone, Copy)]
pub struct SpatialNavigationEnv<'a> {
    /// Every DOM laid out in the window.
    pub layout_results: &'a BTreeMap<DomId, DomLayoutResult>,
    /// DOMs outside this window's focus scope (an open popup's content). They
    /// hold no candidates, exactly as for Tab.
    pub out_of_scope: &'a BTreeSet<DomId>,
    /// The live scroll state of a scroll container (`None` for a node the
    /// scroll manager does not track). `None` for the whole env means
    /// layout-only: every offset is zero.
    pub scroll_info: Option<&'a dyn Fn(DomId, NodeId) -> Option<ScrollNodeInfo>>,
    /// The transform the renderer currently applies to a node.
    pub transform: &'a dyn Fn(DomId, NodeId) -> Option<ComputedTransform3D>,
}

fn no_transform(_dom: DomId, _node: NodeId) -> Option<ComputedTransform3D> {
    None
}

impl<'a> SpatialNavigationEnv<'a> {
    /// The layout-only model: unscrolled, untransformed, and a box can scroll
    /// forward on an axis exactly when layout gave it a bar there.
    #[must_use]
    pub fn layout_only(
        layout_results: &'a BTreeMap<DomId, DomLayoutResult>,
        out_of_scope: &'a BTreeSet<DomId>,
    ) -> Self {
        Self {
            layout_results,
            out_of_scope,
            scroll_info: None,
            transform: &no_transform,
        }
    }

    /// The current scroll offset of `node`, if it is a tracked scroll
    /// container. Always `None` in the layout-only model.
    fn scroll_offset(&self, dom: DomId, node: NodeId) -> Option<LogicalPosition> {
        let info_of = self.scroll_info?;
        info_of(dom, node).map(|info| info.current_offset)
    }
}

/// What the spatial navigation steps (css-nav-1 §8.3) decide for one arrow
/// press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpatialNavigationOutcome {
    /// Move the focus to this node. css-nav-1 fires `navbeforefocus` at the
    /// focused element just before; azul has no such event yet.
    Focus(DomNodeId),
    /// Directionally scroll this container. The focus stays where it is.
    Scroll(DomNodeId),
    /// No container had a candidate or could scroll. css-nav-1 fires
    /// `navnotarget` at each container on the way up; azul has no such
    /// event yet.
    NoTarget,
}

/// Which semantics a run of the steps follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StepsMode {
    /// The arrow-key default action: each container's
    /// `spatial-navigation-action` decides whether only VISIBLE candidates
    /// count and whether the container scrolls.
    Keyboard,
    /// A programmatic directional move (`FocusTarget::Directional`, the
    /// D-pad): as if every container said `spatial-navigation-action: focus`,
    /// so any candidate counts, visible or not, and nothing ever scrolls.
    FocusOnly,
}

/// A rect as its four edges, the form css-nav-1's candidate rules are
/// written in.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Edges {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl Edges {
    fn of(r: LogicalRect) -> Self {
        Self {
            left: r.origin.x,
            top: r.origin.y,
            right: r.origin.x + r.size.width,
            bottom: r.origin.y + r.size.height,
        }
    }

    fn width(&self) -> f32 {
        self.right - self.left
    }

    fn height(&self) -> f32 {
        self.bottom - self.top
    }

    /// Do the two boxes share any area? Touching edges do not.
    fn overlaps(&self, other: &Self) -> bool {
        self.left < other.right
            && other.left < self.right
            && self.top < other.bottom
            && other.top < self.bottom
    }

    /// The common area, or `None` when there is none.
    fn intersect(&self, other: &Self) -> Option<Self> {
        let common = Self {
            left: self.left.max(other.left),
            top: self.top.max(other.top),
            right: self.right.min(other.right),
            bottom: self.bottom.min(other.bottom),
        };
        (common.left < common.right && common.top < common.bottom).then_some(common)
    }
}

/// Length of the overlap of two 1-D spans (0 when they are apart).
fn span_overlap(a0: f32, a1: f32, b0: f32, b1: f32) -> f32 {
    (a1.min(b1) - a0.max(b0)).max(0.0)
}

/// Gap between two 1-D spans (0 when they overlap or touch).
fn span_gap(a0: f32, a1: f32, b0: f32, b1: f32) -> f32 {
    if b0 >= a1 {
        b0 - a1
    } else if a0 >= b1 {
        a0 - b1
    } else {
        0.0
    }
}

/// How far a "beyond" candidate may reach back over the origin's far edge
/// and still count as past it: layout rounding, not a design tolerance.
const EDGE_EPSILON: f32 = 0.01;

/// Painted, window-space geometry for one search.
///
/// A node's rect is its static layout rect with every ancestor scroll offset
/// and transform applied (`headless::node_rect_to_screen`, the helper the
/// a11y tree and menu anchoring use), lifted out of a nested DOM by its
/// `VirtualView` placement (`headless::nested_dom_window_origin`). The
/// per-DOM lift is computed once per search, not once per candidate.
struct SpatialGeometry<'a> {
    env: SpatialNavigationEnv<'a>,
    dom_origins: BTreeMap<DomId, LogicalPosition>,
}

impl<'a> SpatialGeometry<'a> {
    fn new(env: SpatialNavigationEnv<'a>) -> Self {
        let mut dom_origins = BTreeMap::new();
        if env.layout_results.len() > 1 {
            let scroll = |dom: DomId, node: NodeId| env.scroll_offset(dom, node);
            for dom_id in env.layout_results.keys() {
                if *dom_id == DomId::ROOT_ID {
                    continue;
                }
                if let Some(origin) = crate::headless::nested_dom_window_origin(
                    env.layout_results,
                    *dom_id,
                    &scroll,
                    env.transform,
                ) {
                    dom_origins.insert(*dom_id, origin);
                }
            }
        }
        Self { env, dom_origins }
    }

    /// Where `node`'s border box is painted, in window space. `None` for a
    /// node with no box (never laid out, or unmounted).
    fn rect(&self, node: DomNodeId) -> Option<LogicalRect> {
        let lr = self.env.layout_results.get(&node.dom)?;
        let nid = node.node.into_crate_internal()?;
        let idx = *lr.layout_tree.dom_to_layout.get(&nid)?.first()?;
        let size = lr.layout_tree.get(idx)?.used_size?;
        let origin = lr.calculated_positions.get(idx.index()).copied()?;
        let env = self.env;
        let scroll = |dom: DomId, n: NodeId| env.scroll_offset(dom, n);
        let mut painted = crate::headless::node_rect_to_screen(
            lr,
            node.dom,
            idx.index(),
            LogicalRect { origin, size },
            &scroll,
            env.transform,
        );
        if let Some(lift) = self.dom_origins.get(&node.dom) {
            painted.origin.x += lift.x;
            painted.origin.y += lift.y;
        }
        Some(painted)
    }
}

/// The root node of `node`'s DOM: the "document" of css-nav-1, the outermost
/// spatial navigation container of that DOM.
const fn dom_root_of(node: DomNodeId) -> DomNodeId {
    DomNodeId {
        dom: node.dom,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(0))),
    }
}

fn is_dom_root(node: DomNodeId) -> bool {
    node.node.into_crate_internal().is_some_and(|n| n.index() == 0)
}

/// `spatial-navigation-action` of `node` itself (not inherited), `auto` when
/// unset or unresolvable.
fn action_of(env: &SpatialNavigationEnv<'_>, node: DomNodeId) -> StyleSpatialNavigationAction {
    use crate::solver3::getters::{get_spatial_navigation_action, MultiValue};

    let Some(lr) = env.layout_results.get(&node.dom) else {
        return StyleSpatialNavigationAction::Auto;
    };
    let Some(n) = node.node.into_crate_internal() else {
        return StyleSpatialNavigationAction::Auto;
    };
    let states = lr.styled_dom.styled_nodes.as_container();
    let Some(sn) = states.get(n) else {
        return StyleSpatialNavigationAction::Auto;
    };
    match get_spatial_navigation_action(&lr.styled_dom, n, &sn.styled_node_state) {
        MultiValue::Exact(action) => action,
        _ => StyleSpatialNavigationAction::Auto,
    }
}

/// `spatial-navigation-function` of the container `node`, `normal` when
/// unset or unresolvable.
fn function_of(env: &SpatialNavigationEnv<'_>, node: DomNodeId) -> StyleSpatialNavigationFunction {
    use crate::solver3::getters::{get_spatial_navigation_function, MultiValue};

    let Some(lr) = env.layout_results.get(&node.dom) else {
        return StyleSpatialNavigationFunction::Normal;
    };
    let Some(n) = node.node.into_crate_internal() else {
        return StyleSpatialNavigationFunction::Normal;
    };
    let states = lr.styled_dom.styled_nodes.as_container();
    let Some(sn) = states.get(n) else {
        return StyleSpatialNavigationFunction::Normal;
    };
    match get_spatial_navigation_function(&lr.styled_dom, n, &sn.styled_node_state) {
        MultiValue::Exact(function) => function,
        _ => StyleSpatialNavigationFunction::Normal,
    }
}

/// Is `node` a CSS scroll container (`overflow` other than `visible`/`clip`
/// on either axis)? The same test `spatial_navigation_containers` uses
/// (`scroll_chain::is_css_scroll_container`).
fn is_css_scroll_container(env: &SpatialNavigationEnv<'_>, node: DomNodeId) -> bool {
    let Some(lr) = env.layout_results.get(&node.dom) else {
        return false;
    };
    let Some(n) = node.node.into_crate_internal() else {
        return false;
    };
    lr.styled_dom.styled_nodes.as_container().get(n).is_some()
        && crate::solver3::scroll_chain::is_css_scroll_container(&lr.styled_dom, n)
}

/// css-nav-1 Appendix A, "can be manually scrolled" in `dir`: a scroll
/// container whose overflow on that axis is not `hidden`, and which is not at
/// its scroll boundary in `dir`.
///
/// A DOM's root node stands for the viewport, which scrolls although the root
/// element itself is usually `overflow: visible`; only an explicit `hidden`
/// stops it.
///
/// Live env: the scroll manager's offset against its maximum. Layout-only:
/// every offset is zero, so a box can scroll Down/Right exactly when layout
/// gave it a bar on that axis, and never Up/Left.
fn can_manually_scroll(env: &SpatialNavigationEnv<'_>, node: DomNodeId, dir: FocusDirection) -> bool {
    use azul_css::props::layout::overflow::LayoutOverflow;

    use crate::solver3::getters::{get_overflow_x, get_overflow_y, MultiValue};

    /// Offsets closer than this to a boundary count as AT it.
    const BOUNDARY_EPSILON: f32 = 0.5;

    let Some(lr) = env.layout_results.get(&node.dom) else {
        return false;
    };
    let Some(n) = node.node.into_crate_internal() else {
        return false;
    };
    let states = lr.styled_dom.styled_nodes.as_container();
    let Some(sn) = states.get(n) else {
        return false;
    };
    let state = &sn.styled_node_state;
    let horizontal = matches!(dir, FocusDirection::Left | FocusDirection::Right);
    let axis = if horizontal {
        get_overflow_x(&lr.styled_dom, n, state)
    } else {
        get_overflow_y(&lr.styled_dom, n, state)
    };
    let user_scrollable = if n.index() == 0 {
        !matches!(axis, MultiValue::Exact(LayoutOverflow::Hidden))
    } else {
        axis.allows_user_scrolling()
    };
    if !user_scrollable {
        return false;
    }

    if let Some(info_of) = env.scroll_info {
        let Some(info) = info_of(node.dom, n) else {
            return false;
        };
        return match dir {
            FocusDirection::Up => info.current_offset.y > BOUNDARY_EPSILON,
            FocusDirection::Down => info.current_offset.y < info.max_scroll_y - BOUNDARY_EPSILON,
            FocusDirection::Left => info.current_offset.x > BOUNDARY_EPSILON,
            FocusDirection::Right => info.current_offset.x < info.max_scroll_x - BOUNDARY_EPSILON,
        };
    }

    let Some(idx) = lr
        .layout_tree
        .dom_to_layout
        .get(&n)
        .and_then(|v| v.first().copied())
    else {
        return false;
    };
    let Some(bars) = lr
        .layout_tree
        .warm(idx)
        .and_then(|w| w.scrollbar_info.as_ref())
    else {
        return false;
    };
    match dir {
        FocusDirection::Down => bars.needs_vertical,
        FocusDirection::Right => bars.needs_horizontal,
        FocusDirection::Up | FocusDirection::Left => false,
    }
}

/// css-nav-1 "find focusable areas ... visibleOnly": is any part of `rect`
/// (the painted rect of `node`) on screen?
///
/// Clipped by the scrollport of EVERY scroll container the node's box is
/// painted in - its `ScrollChain`, by containing block (an `absolute` box is
/// not clipped by a non-positioned scroller it escapes, a `fixed` box by
/// none) - and, for the window's own DOM, by the viewport: the spec's example
/// of `focusableAreas` "recursively finds focusable areas" inside nested
/// containers and drops the ones outside their scrollports. A nested DOM is
/// not clipped by its `VirtualView` host here (see the audit's open list).
fn is_visible(
    env: &SpatialNavigationEnv<'_>,
    geom: &SpatialGeometry<'_>,
    node: DomNodeId,
    rect: LogicalRect,
) -> bool {
    use azul_core::spaces::Inclusivity;

    use crate::solver3::scroll_chain::{is_css_scroll_container, ScrollChain};

    let Some(lr) = env.layout_results.get(&node.dom) else {
        return false;
    };
    let Some(start) = node.node.into_crate_internal() else {
        return false;
    };
    let mut seen = Edges::of(rect);
    if node.dom == DomId::ROOT_ID {
        let viewport = lr.viewport;
        if viewport.size.width > 0.0 && viewport.size.height > 0.0 {
            match seen.intersect(&Edges::of(viewport)) {
                Some(common) => seen = common,
                None => return false,
            }
        }
    }
    let Some(scroll_chain) = ScrollChain::of_node(
        &lr.layout_tree,
        &lr.styled_dom,
        &lr.scroll_ids,
        start,
        Inclusivity::AncestorsOnly,
    ) else {
        return true;
    };
    for link in &scroll_chain.links {
        if !is_css_scroll_container(&lr.styled_dom, link.node) {
            continue;
        }
        let port = geom.rect(DomNodeId {
            dom: node.dom,
            node: NodeHierarchyItemId::from_crate_internal(Some(link.node)),
        });
        if let Some(port) = port {
            match seen.intersect(&Edges::of(port)) {
                Some(common) => seen = common,
                None => return false,
            }
        }
    }
    true
}

/// The candidate pool of every spatial search: the Tab order (tabindex −1,
/// `<transient-window>` subtrees and out-of-scope DOMs already excluded - the
/// spec's "remove negative tabindex" step), re-sorted into DOCUMENT order,
/// which is the order css-nav-1 breaks ties in.
fn spatial_candidate_pool(env: &SpatialNavigationEnv<'_>) -> Vec<DomNodeId> {
    let mut pool = collect_tab_order(env.layout_results, env.out_of_scope);
    pool.sort_by_key(doc_order_key);
    pool
}

/// css-nav-1 "find focusable areas within `container`", with their painted
/// rects: the pool's DESCENDANTS of `container` (not the container itself),
/// minus `exclude`, and - when `visible_only` - only the ones partly on
/// screen. A node with no box is never a candidate.
fn candidates_in(
    env: &SpatialNavigationEnv<'_>,
    geom: &SpatialGeometry<'_>,
    pool: &[DomNodeId],
    container: DomNodeId,
    exclude: DomNodeId,
    visible_only: bool,
) -> Vec<(DomNodeId, LogicalRect)> {
    pool.iter()
        .copied()
        .filter(|c| *c != exclude && *c != container)
        .filter(|c| is_within(env.layout_results, *c, container))
        .filter_map(|c| {
            let r = geom.rect(c)?;
            (!visible_only || is_visible(env, geom, c, r)).then_some((c, r))
        })
        .collect()
}

/// The "inside area" of a search origin: its border box, or the viewport when
/// the origin is the window's document.
fn inside_area_of(env: &SpatialNavigationEnv<'_>, origin: DomNodeId, rect: LogicalRect) -> LogicalRect {
    if origin.dom == DomId::ROOT_ID && is_dom_root(origin) {
        if let Some(lr) = env.layout_results.get(&origin.dom) {
            let viewport = lr.viewport;
            if viewport.size.width > 0.0 && viewport.size.height > 0.0 {
                return viewport;
            }
        }
    }
    rect
}

/// css-nav-1 §8.4 "select the best candidate", over painted rects.
///
/// `candidates` must be in document order: every tie goes to the earlier one
/// (the spec's further tie-break, CSS painting order, is not applied).
///
/// 1. INSIDERS - candidates overlapping the origin's inside area and lying further along `dir`
///    than its start edge (a child of a focused box) - win first, nearest start edge first.
/// 2. Otherwise only candidates entirely BEYOND the origin's far edge count (edges, not centres:
///    a box below but far to the side is still below), and `function` picks among them.
///
/// The spec's single-candidate shortcut is applied after the directional
/// filter, not before: a lone candidate ABOVE the focus is no answer to Down.
fn select_best_candidate(
    origin: LogicalRect,
    inside_area: LogicalRect,
    candidates: &[(DomNodeId, LogicalRect)],
    dir: FocusDirection,
    function: StyleSpatialNavigationFunction,
) -> Option<DomNodeId> {
    let o = Edges::of(origin);
    let area = Edges::of(inside_area);

    let mut best_insider: Option<(f32, DomNodeId)> = None;
    for (node, rect) in candidates {
        let c = Edges::of(*rect);
        let further = match dir {
            FocusDirection::Down => c.top > o.top,
            FocusDirection::Up => c.bottom < o.bottom,
            FocusDirection::Right => c.left > o.left,
            FocusDirection::Left => c.right < o.right,
        };
        if !(further && c.overlaps(&area)) {
            continue;
        }
        let key = match dir {
            FocusDirection::Down => c.top - area.top,
            FocusDirection::Up => area.bottom - c.bottom,
            FocusDirection::Right => c.left - area.left,
            FocusDirection::Left => area.right - c.right,
        };
        if best_insider.is_none_or(|(b, _)| key < b) {
            best_insider = Some((key, *node));
        }
    }
    if let Some((_, node)) = best_insider {
        return Some(node);
    }

    let beyond: Vec<(DomNodeId, Edges)> = candidates
        .iter()
        .map(|(node, rect)| (*node, Edges::of(*rect)))
        .filter(|(_, c)| {
            !c.overlaps(&o)
                && match dir {
                    FocusDirection::Down => c.top >= o.bottom - EDGE_EPSILON,
                    FocusDirection::Up => c.bottom <= o.top + EDGE_EPSILON,
                    FocusDirection::Right => c.left >= o.right - EDGE_EPSILON,
                    FocusDirection::Left => c.right <= o.left + EDGE_EPSILON,
                }
        })
        .collect();

    match function {
        StyleSpatialNavigationFunction::Normal => {
            let mut best: Option<(f32, DomNodeId)> = None;
            for (node, c) in &beyond {
                let d = normal_distance(&o, c, dir);
                if best.is_none_or(|(b, _)| d < b) {
                    best = Some((d, *node));
                }
            }
            best.map(|(_, node)| node)
        }
        StyleSpatialNavigationFunction::Grid => grid_pick(&o, &beyond, dir),
    }
}

/// Distance ALONG `dir` from the origin's far edge to the candidate's near
/// edge (never negative).
fn along_gap(o: &Edges, c: &Edges, dir: FocusDirection) -> f32 {
    let gap = match dir {
        FocusDirection::Down => c.top - o.bottom,
        FocusDirection::Up => o.top - c.bottom,
        FocusDirection::Right => c.left - o.right,
        FocusDirection::Left => o.left - c.right,
    };
    gap.max(0.0)
}

/// `(gap, projected overlap, origin's cross size)` on the axis ORTHOGONAL to
/// `dir`.
fn cross_axis(o: &Edges, c: &Edges, dir: FocusDirection) -> (f32, f32, f32) {
    if matches!(dir, FocusDirection::Left | FocusDirection::Right) {
        (
            span_gap(o.top, o.bottom, c.top, c.bottom),
            span_overlap(o.top, o.bottom, c.top, c.bottom),
            o.height(),
        )
    } else {
        (
            span_gap(o.left, o.right, c.left, c.right),
            span_overlap(o.left, o.right, c.left, c.right),
            o.width(),
        )
    }
}

/// css-nav-1 §8.4 "find the shortest distance", for a candidate beyond the
/// origin (so the two never overlap and the `sqrt(Overlap)` term is 0):
///
/// `distance = euclidean + displacement - alignment`, where
/// - `euclidean` is between the closest points P1, P2 of the two boxes;
/// - `displacement = (cross-axis gap + orthogonalBias) * orthogonalWeight`, `orthogonalBias` half
///   the origin's cross size, `orthogonalWeight` 30 for Left/Right and 2 for Up/Down;
/// - `alignment = (projected overlap / origin's cross size) * 5`.
///
/// The weights are the spec's own, "determined experimentally" against its
/// UX test cases. This replaced azul's `along + 3 * cross` on centre deltas,
/// whose comment wrongly credited the 3 to the spec.
fn normal_distance(o: &Edges, c: &Edges, dir: FocusDirection) -> f32 {
    const ALIGN_WEIGHT: f32 = 5.0;
    let horizontal = matches!(dir, FocusDirection::Left | FocusDirection::Right);
    let along = along_gap(o, c, dir);
    let (cross_gap, projected, cross_size) = cross_axis(o, c, dir);
    let euclidean = along.hypot(cross_gap);
    let orthogonal_weight = if horizontal { 30.0 } else { 2.0 };
    let displacement = (cross_gap + cross_size / 2.0) * orthogonal_weight;
    let alignment = if cross_size > 0.0 {
        projected / cross_size * ALIGN_WEIGHT
    } else {
        0.0
    };
    euclidean + displacement - alignment
}

/// css-nav-1 §9.3 `spatial-navigation-function: grid`.
///
/// ALIGNED candidates (their projection on the cross axis overlaps the
/// origin's) win: nearest along the axis, ties to the one whose centre is
/// closest to the origin's on the cross axis (the spec's "minimum amount of
/// alignment" read as the least MIS-alignment). Only when nothing is aligned:
/// nearest along the axis, ties to the smallest cross-axis gap. Remaining ties
/// go to document order.
fn grid_pick(o: &Edges, beyond: &[(DomNodeId, Edges)], dir: FocusDirection) -> Option<DomNodeId> {
    /// Two distances this close are a tie.
    const TIE: f32 = 0.01;

    let horizontal = matches!(dir, FocusDirection::Left | FocusDirection::Right);
    let centre_offset = |c: &Edges| -> f32 {
        if horizontal {
            ((c.top + c.bottom) - (o.top + o.bottom)).abs() / 2.0
        } else {
            ((c.left + c.right) - (o.left + o.right)).abs() / 2.0
        }
    };
    // Lexicographic on (primary, tie-break), with a tolerance on both.
    let better = |a: (f32, f32), b: (f32, f32)| {
        a.0 < b.0 - TIE || ((a.0 - b.0).abs() <= TIE && a.1 < b.1 - TIE)
    };

    let mut aligned: Option<((f32, f32), DomNodeId)> = None;
    let mut any: Option<((f32, f32), DomNodeId)> = None;
    for (node, c) in beyond {
        let along = along_gap(o, c, dir);
        let (cross_gap, projected, _) = cross_axis(o, c, dir);
        if projected > 0.0 {
            let key = (along, centre_offset(c));
            if aligned.is_none_or(|(k, _)| better(key, k)) {
                aligned = Some((key, *node));
            }
        }
        let key = (along, cross_gap);
        if any.is_none_or(|(k, _)| better(key, k)) {
            any = Some((key, *node));
        }
    }
    aligned.or(any).map(|(_, node)| node)
}

/// css-nav-1 §8.3 "spatial navigation steps" for one arrow press from
/// `origin`: what the arrow-key default action does.
///
/// 1. `origin` itself is a scroll container (or its DOM's root, the document): unless its
///    `spatial-navigation-action` is `focus`, it SCROLLS while it can; then its own candidates are
///    searched.
/// 2. Then every spatial navigation container from the innermost outward, the DOM's root last. In
///    each: the best candidate (VISIBLE ones only, unless the container says `focus`) wins; with
///    none, a container that can still scroll that way SCROLLS (unless it says `focus`); otherwise
///    the search moves out (`navnotarget`).
/// 3. azul extension: when the whole DOM is exhausted, candidates in the window's OTHER DOMs
///    (VirtualView content, or the host of the DOM the focus is in). This stands in for the spec's
///    nested-browsing-context step.
///
/// `spatial-navigation-function` is read off each container searched.
#[must_use]
pub fn spatial_navigation_steps(
    env: &SpatialNavigationEnv<'_>,
    origin: DomNodeId,
    dir: FocusDirection,
) -> SpatialNavigationOutcome {
    run_spatial_navigation_steps(env, origin, dir, StepsMode::Keyboard)
}

/// Where a programmatic directional focus move from `current` lands:
/// `FocusTarget::Directional` and the gamepad D-pad.
///
/// First the arrow-key steps. When they would SCROLL (or find nothing), the
/// same search again as if every container said `focus` - any candidate,
/// visible or not - because a focus move cannot scroll, and a D-pad whose next
/// item is scrolled out of view must still reach it (focus scrolls it in).
/// With nothing focused, the first tab stop: an arrow wakes the UI the way a
/// TV remote wakes a menu.
#[must_use]
pub fn directional_focus_target(
    env: &SpatialNavigationEnv<'_>,
    current: Option<DomNodeId>,
    dir: FocusDirection,
) -> Option<DomNodeId> {
    let Some(origin) = current else {
        return collect_tab_order(env.layout_results, env.out_of_scope)
            .first()
            .copied();
    };
    match run_spatial_navigation_steps(env, origin, dir, StepsMode::Keyboard) {
        SpatialNavigationOutcome::Focus(node) => Some(node),
        SpatialNavigationOutcome::Scroll(_) | SpatialNavigationOutcome::NoTarget => {
            match run_spatial_navigation_steps(env, origin, dir, StepsMode::FocusOnly) {
                SpatialNavigationOutcome::Focus(node) => Some(node),
                SpatialNavigationOutcome::Scroll(_) | SpatialNavigationOutcome::NoTarget => None,
            }
        }
    }
}

// ---------------------------------------------------------------------------
// The css-nav-1 JS API (§5.2), over the same engine
// ---------------------------------------------------------------------------

/// Is `node` a spatial navigation container: `contain`, a scroll container
/// (under `auto`), or its DOM's root (the document)?
fn is_spatial_navigation_container(env: &SpatialNavigationEnv<'_>, node: DomNodeId) -> bool {
    is_dom_root(node)
        || spatial_navigation_containers(env.layout_results, node)
            .first()
            .is_some_and(|innermost| *innermost == node)
}

/// css-nav-1 `element.getSpatialNavigationContainer()`: the nearest ANCESTOR
/// of `node` that is a spatial navigation container - never `node` itself -
/// or the document (its DOM's root node) when the nearest container is the
/// viewport. `None` only for a node that does not exist.
#[must_use]
pub fn get_spatial_navigation_container(
    env: &SpatialNavigationEnv<'_>,
    node: DomNodeId,
) -> Option<DomNodeId> {
    let lr = env.layout_results.get(&node.dom)?;
    let n = node.node.into_crate_internal()?;
    if lr.styled_dom.node_data.as_container().get(n).is_none() {
        return None;
    }
    Some(
        spatial_navigation_containers(env.layout_results, node)
            .into_iter()
            .find(|c| *c != node)
            .unwrap_or_else(|| dom_root_of(node)),
    )
}

/// css-nav-1 `element.focusableAreas({ mode })`: the focusable areas whose
/// node is a DESCENDANT of `node`, in document order. `Visible` keeps the ones
/// at least partly inside every scrollport above them; `All` keeps every one
/// that has a box. The pool is the spatial one (the Tab order: tabindex −1,
/// popup content and out-of-scope DOMs excluded).
#[must_use]
pub fn focusable_areas(
    env: &SpatialNavigationEnv<'_>,
    node: DomNodeId,
    mode: azul_core::callbacks::FocusableAreaSearchMode,
) -> Vec<DomNodeId> {
    let geom = SpatialGeometry::new(*env);
    let visible_only = mode == azul_core::callbacks::FocusableAreaSearchMode::Visible;
    spatial_candidate_pool(env)
        .into_iter()
        .filter(|c| *c != node && is_within(env.layout_results, *c, node))
        .filter(|c| {
            geom.rect(*c)
                .is_some_and(|r| !visible_only || is_visible(env, &geom, *c, r))
        })
        .collect()
}

/// css-nav-1 `element.spatialNavigationSearch(dir, options)`: the best
/// candidate in `dir` from `node`, by the container's
/// `spatial-navigation-function`.
///
/// The container is `options.container` if that is a spatial navigation
/// container, else its nearest container ancestor; with no container given,
/// `node`'s nearest container ancestor. The candidates are
/// `options.candidates` when given (visible or not, exactly those), else the
/// VISIBLE focusable areas of that container. Per the spec's note this does
/// NOT climb further up when the container has nothing in `dir` - that is
/// what the arrow keys' steps ([`spatial_navigation_steps`]) do.
#[must_use]
pub fn spatial_navigation_search(
    env: &SpatialNavigationEnv<'_>,
    node: DomNodeId,
    dir: FocusDirection,
    options: &azul_core::callbacks::SpatialNavigationSearchOptions,
) -> Option<DomNodeId> {
    let geom = SpatialGeometry::new(*env);
    let origin_rect = geom.rect(node)?;
    let container = match options.container.into_option() {
        Some(c) if is_spatial_navigation_container(env, c) => c,
        Some(c) => get_spatial_navigation_container(env, c)?,
        None => get_spatial_navigation_container(env, node)?,
    };
    let areas: Vec<(DomNodeId, LogicalRect)> = match options.candidates.as_ref() {
        Some(list) => list
            .iter()
            .copied()
            .filter(|c| *c != node)
            .filter_map(|c| Some((c, geom.rect(c)?)))
            .collect(),
        None => {
            let pool = spatial_candidate_pool(env);
            candidates_in(env, &geom, &pool, container, node, true)
        }
    };
    select_best_candidate(
        origin_rect,
        inside_area_of(env, node, origin_rect),
        &areas,
        dir,
        function_of(env, container),
    )
}

#[allow(clippy::too_many_lines)] // one algorithm, written in the spec's step order
fn run_spatial_navigation_steps(
    env: &SpatialNavigationEnv<'_>,
    origin: DomNodeId,
    dir: FocusDirection,
    mode: StepsMode,
) -> SpatialNavigationOutcome {
    let geom = SpatialGeometry::new(*env);
    let pool = spatial_candidate_pool(env);
    let effective_action = |node: DomNodeId| -> StyleSpatialNavigationAction {
        match mode {
            StepsMode::Keyboard => action_of(env, node),
            StepsMode::FocusOnly => StyleSpatialNavigationAction::Focus,
        }
    };
    // Every container ABOVE the origin, innermost first; the origin itself is
    // step 1's business.
    let chain: Vec<DomNodeId> = spatial_navigation_containers(env.layout_results, origin)
        .into_iter()
        .filter(|c| *c != origin)
        .collect();

    // No box to search FROM (never laid out, unmounted, or a fixture without
    // geometry): the first candidate of the innermost container that has one,
    // else of the whole pool. Keeps containment observable without geometry,
    // and never answers the origin itself.
    let Some(origin_rect) = geom.rect(origin) else {
        let first_in = |container: Option<DomNodeId>| {
            pool.iter().copied().find(|c| {
                *c != origin
                    && container.is_none_or(|k| *c != k && is_within(env.layout_results, *c, k))
            })
        };
        return chain
            .iter()
            .find_map(|k| first_in(Some(*k)))
            .or_else(|| first_in(None))
            .map_or(SpatialNavigationOutcome::NoTarget, SpatialNavigationOutcome::Focus);
    };

    // Step 1: the origin is itself a scroll container, or the document.
    if is_dom_root(origin) || is_css_scroll_container(env, origin) {
        let action = effective_action(origin);
        if action != StyleSpatialNavigationAction::Focus && can_manually_scroll(env, origin, dir) {
            return SpatialNavigationOutcome::Scroll(origin);
        }
        let visible_only = action != StyleSpatialNavigationAction::Focus;
        let inside = candidates_in(env, &geom, &pool, origin, origin, visible_only);
        if let Some(best) = select_best_candidate(
            origin_rect,
            inside_area_of(env, origin, origin_rect),
            &inside,
            dir,
            function_of(env, origin),
        ) {
            return SpatialNavigationOutcome::Focus(best);
        }
    }

    // Step 2: the container chain, the document last.
    let mut containers = chain;
    let root = dom_root_of(origin);
    if root != origin && !containers.contains(&root) {
        containers.push(root);
    }
    for container in containers {
        let action = effective_action(container);
        let visible_only = action != StyleSpatialNavigationAction::Focus;
        let candidates = candidates_in(env, &geom, &pool, container, origin, visible_only);
        if let Some(best) = select_best_candidate(
            origin_rect,
            origin_rect,
            &candidates,
            dir,
            function_of(env, container),
        ) {
            return SpatialNavigationOutcome::Focus(best);
        }
        let scrollable = is_dom_root(container) || is_css_scroll_container(env, container);
        if action != StyleSpatialNavigationAction::Focus
            && scrollable
            && can_manually_scroll(env, container, dir)
        {
            return SpatialNavigationOutcome::Scroll(container);
        }
        // `navnotarget` would fire here, at the focused element, naming
        // `container`; then the search moves out.
    }

    // Step 3 (azul extension): the window's other DOMs.
    let document_action = effective_action(root);
    let visible_only = document_action != StyleSpatialNavigationAction::Focus;
    let others: Vec<(DomNodeId, LogicalRect)> = pool
        .iter()
        .copied()
        .filter(|c| c.dom != origin.dom)
        .filter_map(|c| {
            let r = geom.rect(c)?;
            (!visible_only || is_visible(env, &geom, c, r)).then_some((c, r))
        })
        .collect();
    select_best_candidate(
        origin_rect,
        origin_rect,
        &others,
        dir,
        function_of(env, root),
    )
    .map_or(SpatialNavigationOutcome::NoTarget, SpatialNavigationOutcome::Focus)
}

#[cfg(test)]
mod spatial_selection_tests {
    //! css-nav-1 §8.4 / §9.3 candidate selection on synthetic rects - the
    //! pure half of the engine, no layout needed.

    use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
    use azul_css::props::style::spatial_nav::StyleSpatialNavigationFunction;

    use super::*;

    fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
        LogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(w, h))
    }

    fn node(n: usize) -> DomNodeId {
        FocusSearchContext::make_dom_node_id(DomId::ROOT_ID, NodeId::new(n))
    }

    fn pick(
        origin: LogicalRect,
        candidates: &[(usize, LogicalRect)],
        dir: FocusDirection,
        function: StyleSpatialNavigationFunction,
    ) -> Option<DomNodeId> {
        let c: Vec<(DomNodeId, LogicalRect)> =
            candidates.iter().map(|(n, r)| (node(*n), *r)).collect();
        select_best_candidate(origin, origin, &c, dir, function)
    }

    /// "Below" is decided by EDGES: a box whose top edge is under the focus's
    /// bottom edge is below it, however far to the side it sits.
    #[test]
    fn a_box_below_counts_as_below_however_far_to_the_side() {
        let a = rect(0.0, 0.0, 100.0, 20.0);
        let b = rect(150.0, 30.0, 100.0, 20.0);
        assert_eq!(
            pick(a, &[(2, b)], FocusDirection::Down, StyleSpatialNavigationFunction::Normal),
            Some(node(2)),
        );
    }

    /// The single-candidate shortcut comes AFTER the direction filter: a lone
    /// box above the focus is no answer to Down.
    #[test]
    fn a_lone_candidate_the_other_way_is_no_answer() {
        let a = rect(0.0, 100.0, 100.0, 20.0);
        let above = rect(0.0, 0.0, 100.0, 20.0);
        for function in [
            StyleSpatialNavigationFunction::Normal,
            StyleSpatialNavigationFunction::Grid,
        ] {
            assert_eq!(pick(a, &[(2, above)], FocusDirection::Down, function), None);
        }
    }

    /// Touching boxes do not overlap: the next row of a list whose top edge
    /// IS the focus's bottom edge is below it.
    #[test]
    fn the_next_row_of_a_list_is_below_even_when_the_edges_touch() {
        let a = rect(0.0, 0.0, 100.0, 40.0);
        let next = rect(0.0, 40.0, 100.0, 40.0);
        let after = rect(0.0, 80.0, 100.0, 40.0);
        assert_eq!(
            pick(
                a,
                &[(2, next), (3, after)],
                FocusDirection::Down,
                StyleSpatialNavigationFunction::Normal
            ),
            Some(node(2)),
        );
    }

    /// An INSIDER (a box inside the focused one, further along the axis than
    /// its start edge) wins over every box beyond it.
    #[test]
    fn a_box_inside_the_focus_wins_over_one_beyond_it() {
        let card = rect(0.0, 0.0, 200.0, 200.0);
        let inner = rect(10.0, 50.0, 50.0, 20.0);
        let below = rect(0.0, 210.0, 200.0, 20.0);
        assert_eq!(
            pick(
                card,
                &[(2, inner), (3, below)],
                FocusDirection::Down,
                StyleSpatialNavigationFunction::Normal
            ),
            Some(node(2)),
        );
    }

    /// The css-nav-1 §9.3 example in both modes: `a` on top, `b` nearer but
    /// beside the column, `c` further but straight below. `normal` takes the
    /// nearer `b` (distance 190 vs 675); `grid` takes the aligned `c`.
    #[test]
    fn normal_takes_the_nearer_box_and_grid_the_aligned_one() {
        let a = rect(0.0, 0.0, 100.0, 20.0);
        let b = rect(100.0, 110.0, 100.0, 20.0);
        let c = rect(0.0, 600.0, 100.0, 20.0);
        let candidates = [(3, b), (4, c)];
        assert_eq!(
            pick(a, &candidates, FocusDirection::Down, StyleSpatialNavigationFunction::Normal),
            Some(node(3)),
        );
        assert_eq!(
            pick(a, &candidates, FocusDirection::Down, StyleSpatialNavigationFunction::Grid),
            Some(node(4)),
        );
    }

    /// `grid` with nothing aligned: the nearest along the axis, ties to the
    /// smallest gap across it.
    #[test]
    fn grid_without_an_aligned_box_takes_the_nearest_along_the_axis() {
        let a = rect(0.0, 0.0, 100.0, 20.0);
        let far_right = rect(400.0, 50.0, 100.0, 20.0);
        let near_right = rect(150.0, 50.0, 100.0, 20.0);
        let deeper = rect(120.0, 90.0, 100.0, 20.0);
        assert_eq!(
            pick(
                a,
                &[(2, far_right), (3, near_right), (4, deeper)],
                FocusDirection::Down,
                StyleSpatialNavigationFunction::Grid
            ),
            Some(node(3)),
        );
    }

    /// Left/Right weigh drift across the axis 15x harder than Up/Down do
    /// (orthogonalWeight 30 vs 2): Right prefers the box on the same row even
    /// when a box one row down is nearer.
    #[test]
    fn right_stays_on_the_row() {
        let a = rect(0.0, 0.0, 50.0, 20.0);
        let same_row_far = rect(300.0, 0.0, 50.0, 20.0);
        let next_row_near = rect(60.0, 30.0, 50.0, 20.0);
        assert_eq!(
            pick(
                a,
                &[(2, same_row_far), (3, next_row_near)],
                FocusDirection::Right,
                StyleSpatialNavigationFunction::Normal
            ),
            Some(node(2)),
        );
    }
}

#[cfg(test)]
mod focus_lost_to_unmount_tests {
    //! Focus dropped because the DOM rebuild did not carry the node over is
    //! RECORDED, so the node it belonged to can still be told.
    //!
    //! Clearing it is right - the arena index now denotes a different element
    //! - but as a plain field write it was silent: an app that commits a text
    //! field or closes a popup on blur heard nothing when its focused node
    //! was unmounted.

    use azul_core::{
        dom::{DomId, DomNodeId, NodeId},
        styled_dom::NodeHierarchyItemId,
    };

    use super::FocusManager;
    use crate::managers::{NodeIdMap, NodeIdRemap};

    fn node(id: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(id))),
        }
    }

    #[test]
    fn focus_a_rebuild_did_not_carry_over_is_recorded_for_its_node() {
        let mut fm = FocusManager::new();
        fm.focused_node = Some(node(7));
        // The rebuild kept node 3 and dropped node 7.
        fm.remap_node_ids(
            DomId::ROOT_ID,
            &NodeIdMap::from_pairs([(NodeId::new(3), NodeId::new(3))]),
        );
        assert_eq!(fm.focused_node, None, "the index means another element now");
        assert_eq!(
            fm.take_focus_lost_to_unmount(),
            vec![node(7)],
            "and the node that lost it is named, once"
        );
        assert!(
            fm.take_focus_lost_to_unmount().is_empty(),
            "draining it twice reports it twice"
        );
    }

    #[test]
    fn focus_that_merely_moved_is_not_a_loss() {
        let mut fm = FocusManager::new();
        fm.focused_node = Some(node(7));
        fm.remap_node_ids(
            DomId::ROOT_ID,
            &NodeIdMap::from_pairs([(NodeId::new(7), NodeId::new(21))]),
        );
        assert_eq!(fm.focused_node, Some(node(21)));
        assert!(fm.take_focus_lost_to_unmount().is_empty());
    }

    #[test]
    fn every_seat_that_lost_its_node_is_named_too() {
        let mut fm = FocusManager::new();
        fm.seat_focus.insert(1, node(4));
        fm.seat_focus.insert(2, node(5));
        fm.remap_node_ids(
            DomId::ROOT_ID,
            &NodeIdMap::from_pairs([(NodeId::new(4), NodeId::new(9))]),
        );
        assert_eq!(fm.seat_focus.get(&1), Some(&node(9)));
        assert_eq!(fm.take_focus_lost_to_unmount(), vec![node(5)]);
    }
}
