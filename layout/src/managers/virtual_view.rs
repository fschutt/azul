//! `VirtualView` lifecycle management for layout
//!
//! This module provides:
//! - `VirtualView` re-invocation logic for lazy loading
//! - Nested DOM ID management

use alloc::collections::BTreeMap;

use azul_core::{
    callbacks::{EdgeType, VirtualViewCallbackReason},
    dom::{DomId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
};

use crate::managers::scroll_state::ScrollManager;

/// Distance in pixels from edge that triggers edge-scrolled callback
const EDGE_THRESHOLD: f32 = 200.0;

/// Manages `VirtualView` lifecycle, including re-invocation
///
/// Tracks which `VirtualViews` have been invoked, assigns unique DOM IDs to nested
/// virtual views, and determines when `VirtualViews` need to be re-invoked (e.g., when
/// the container bounds expand or the user scrolls near an edge).
#[derive(Debug, Clone, Default)]
pub struct VirtualViewManager {
    /// Per-`VirtualView` state keyed by (parent `DomId`, `NodeId` of virtualized view element)
    states: BTreeMap<(DomId, NodeId), VirtualViewState>,
    /// Counter for generating unique nested DOM IDs
    next_dom_id: usize,
    /// MWA-C-virtual_view: queue-time callback reasons, consumed by the very
    /// next `check_reinvoke` for the same view (set by
    /// `process_virtual_view_updates` right before the invoke). Replaces the
    /// `force_reinvoke` clear-flag trick that collapsed every delivered
    /// reason to `InitialRender`.
    reason_overrides: Vec<((DomId, NodeId), VirtualViewCallbackReason)>,
}

/// Internal state for a single `VirtualView` instance
///
/// Tracks invocation status, content dimensions, and edge triggers
/// to determine when the `VirtualView` callback needs to be re-invoked.
#[derive(Debug, Clone)]
struct VirtualViewState {
    /// WHAT IS MATERIALIZED RIGHT NOW, in VIRTUAL space.
    ///
    /// `origin` = where this window of content begins in the document (the
    /// `scroll_offset` the callback reported); `size` = how much it covers.
    /// This is the rect the content is PLACED by:
    /// `content is drawn at container.origin + (materialized.origin - scroll_offset)`.
    ///
    /// Deliberately a rect, not a loose position + size: the origin and the
    /// extent are one fact about one window and drift apart the moment they
    /// are stored separately (which is how the offset ended up write-only).
    materialized: Option<LogicalRect>,
    /// THE WHOLE DOCUMENT, in VIRTUAL space — the app's current best estimate
    /// (`virtual_scroll_size`), which background pagination refines over time.
    ///
    /// ONLY the scrollbar reads this. Placement above does not, which is the
    /// property that lets the estimate change without the content jumping:
    /// the user sees the thumb resize and nothing else move.
    virtual_rect: Option<LogicalRect>,
    /// Whether the `VirtualView` has ever been invoked
    virtual_view_was_invoked: bool,
    /// Whether invoked for current container expansion
    invoked_for_current_expansion: bool,
    /// Whether invoked for current edge scroll event
    invoked_for_current_edge: bool,
    /// Which edges have already triggered callbacks
    last_edge_triggered: EdgeFlags,
    /// Unique DOM ID assigned to this `VirtualView`'s content
    nested_dom_id: DomId,
    /// The `VirtualView`'s own on-screen box (the viewport), window coords.
    /// `size` is the scrollport the other two rects are compared against.
    container: LogicalRect,
    /// Scroll offset captured at `InitialRender`. Edge-scroll callbacks only fire
    /// once the user has scrolled away from this resting position — being at an
    /// edge from the very start (e.g. the top/left edge at offset 0) is the
    /// initial position, not a scroll-to-edge event.
    initial_scroll_offset: LogicalPosition,
}

/// Flags indicating which scroll edges have been triggered
///
/// Used to prevent repeated edge-scroll callbacks for the same edge
/// until the user scrolls away and back.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
#[allow(clippy::struct_excessive_bools)] // one independent bool per box edge (top/bottom/left/right)
struct EdgeFlags {
    /// Near top edge
    top: bool,
    /// Near bottom edge
    bottom: bool,
    /// Near left edge
    left: bool,
    /// Near right edge
    right: bool,
}

impl VirtualViewManager {
    /// Creates a new `VirtualViewManager` with no tracked `VirtualViews`
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_dom_id: 1, // 0 is root
            ..Default::default()
        }
    }

    /// Number of tracked `VirtualView` states. Used by `AZ_E2E_TEST` to watch growth.
    #[must_use]
    pub fn debug_counts(&self) -> usize {
        self.states.len()
    }

    /// MWA-C-virtual_view: stage the reason the next invoke of this view
    /// should deliver to the user callback (consumed by `check_reinvoke`).
    pub fn set_reason_override(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        reason: VirtualViewCallbackReason,
    ) {
        self.reason_overrides
            .retain(|((d, n), _)| !(*d == dom_id && *n == node_id));
        self.reason_overrides.push(((dom_id, node_id), reason));
    }

    /// Gets or creates a unique nested DOM ID for a `VirtualView`
    ///
    /// Returns the existing DOM ID if the `VirtualView` was previously registered,
    /// otherwise allocates a new unique ID and initializes the `VirtualView` state.
    pub fn get_or_create_nested_dom_id(&mut self, dom_id: DomId, node_id: NodeId) -> DomId {
        let key = (dom_id, node_id);

        // Check if already exists
        if let Some(state) = self.states.get(&key) {
            return state.nested_dom_id;
        }

        // Create new nested DOM ID
        let nested_dom_id = DomId {
            inner: self.next_dom_id,
        };
        self.next_dom_id += 1;

        self.states
            .insert(key, VirtualViewState::new(nested_dom_id));
        nested_dom_id
    }

    /// Gets the nested DOM ID for a `VirtualView` if it exists
    #[must_use]
    pub fn get_nested_dom_id(&self, dom_id: DomId, node_id: NodeId) -> Option<DomId> {
        self.states.get(&(dom_id, node_id)).map(|s| s.nested_dom_id)
    }

    /// Returns whether the `VirtualView` has ever been invoked
    #[must_use]
    pub fn was_virtual_view_invoked(&self, dom_id: DomId, node_id: NodeId) -> bool {
        self.states
            .get(&(dom_id, node_id))
            .is_some_and(|s| s.virtual_view_was_invoked)
    }

    /// Updates the `VirtualView`'s content size information
    ///
    /// Called after the `VirtualView` callback returns to record the actual content
    /// dimensions. If the new size is larger than previously recorded, clears
    /// the expansion flag to allow `BoundsExpanded` re-invocation.
    /// The sizes the view's LAST invoke declared (`scroll_size`,
    /// `virtual_scroll_size`) — the reinvoke signal feeds these back so the
    /// callback's page math sees its own declared virtual extent (#16).
    #[must_use]
    pub fn get_declared_sizes(
        &self,
        dom_id: DomId,
        node_id: NodeId,
    ) -> (Option<LogicalSize>, Option<LogicalSize>) {
        self.states
            .get(&(dom_id, node_id))
            .map_or((None, None), |s| {
                (
                    s.materialized.map(|m| m.size),
                    s.virtual_rect.map(|v| v.size),
                )
            })
    }

    /// Record what the callback just materialized, as RECTS in virtual space.
    ///
    /// `window_origin` is where this window of content begins in the document
    /// (the callback's `scroll_offset`) — the piece that used to be dropped,
    /// which is why content could never be placed and the view never scrolled.
    pub fn update_virtual_view_info(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        window_origin: LogicalPosition,
        scroll_size: LogicalSize,
        virtual_scroll_size: LogicalSize,
    ) -> Option<()> {
        let state = self.states.get_mut(&(dom_id, node_id))?;

        // Reset expansion flag if the materialized window grew
        if let Some(old) = state.materialized {
            if scroll_size.width > old.size.width || scroll_size.height > old.size.height {
                state.invoked_for_current_expansion = false;
            }
        }
        state.materialized = Some(LogicalRect::new(window_origin, scroll_size));
        // The document estimate lives at the virtual origin; only its SIZE is
        // the app's (refinable) claim. Changing it must move the scrollbar and
        // nothing else — placement reads `materialized`, never this.
        state.virtual_rect = Some(LogicalRect::new(
            LogicalPosition::zero(),
            virtual_scroll_size,
        ));

        Some(())
    }

    /// Where the materialized window sits in virtual space, if anything is
    /// materialized. The renderer places content at
    /// `container.origin + (window_origin - scroll_offset)`.
    #[must_use]
    pub fn materialized_window_origin(
        &self,
        dom_id: DomId,
        node_id: NodeId,
    ) -> Option<LogicalPosition> {
        self.states
            .get(&(dom_id, node_id))
            .and_then(|s| s.materialized)
            .map(|m| m.origin)
    }

    /// Marks a `VirtualView` as invoked for a specific reason
    ///
    /// Updates internal state flags based on the callback reason to prevent
    /// duplicate callbacks for the same trigger condition.
    pub fn mark_invoked(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        reason: VirtualViewCallbackReason,
    ) -> Option<()> {
        let state = self.states.get_mut(&(dom_id, node_id))?;

        state.virtual_view_was_invoked = true;
        match reason {
            VirtualViewCallbackReason::BoundsExpanded => state.invoked_for_current_expansion = true,
            VirtualViewCallbackReason::EdgeScrolled(edge) => {
                state.invoked_for_current_edge = true;
                state.last_edge_triggered = edge.into();
            }
            _ => {}
        }

        Some(())
    }

    /// Reset invocation flags for ALL tracked `VirtualViews`
    ///
    /// After `layout_results.clear()`, the child DOMs no longer exist in memory.
    /// This method ensures `check_reinvoke()` returns `InitialRender` for every
    /// `VirtualView`, so the callbacks re-run and re-populate `layout_results`.
    ///
    /// Called from `layout_and_generate_display_list()` after clearing layout results.
    pub fn reset_all_invocation_flags(&mut self) {
        for state in self.states.values_mut() {
            state.virtual_view_was_invoked = false;
            state.invoked_for_current_expansion = false;
            state.invoked_for_current_edge = false;
            state.last_edge_triggered = EdgeFlags::default();
        }
    }

    /// Force a `VirtualView` to be re-invoked on the next layout pass
    ///
    /// Clears all invocation flags, causing `check_reinvoke()` to return `InitialRender`.
    /// Used by `trigger_virtual_view_rerender()` to manually refresh `VirtualView` content.
    pub fn force_reinvoke(&mut self, dom_id: DomId, node_id: NodeId) -> Option<()> {
        let state = self.states.get_mut(&(dom_id, node_id))?;

        state.virtual_view_was_invoked = false;
        state.invoked_for_current_expansion = false;
        state.invoked_for_current_edge = false;

        Some(())
    }

    /// `(DomId, NodeId)` of every `VirtualView` registered so far (invoked at
    /// least once). Used to re-invoke *all* views after a shared-dataset change
    /// arrives out-of-band (e.g. a background tile-fetch writeback) without
    /// needing to know which node the data belongs to.
    /// Which `VirtualView` HOSTS a nested dom: the inverse of
    /// [`Self::get_nested_dom_id`].
    ///
    /// A nested dom's display list is 0-relative — the rasteriser composites
    /// it at `host_bounds.origin + content_offset` — so every geometry
    /// accessor that must answer in WINDOW space has to walk back up through
    /// its hosts. Without this there was no way to ask "where does this dom
    /// actually sit", and a caret rect inside a `VirtualView` was handed to the
    /// platform IME as if the host were at the window origin.
    #[must_use]
    pub fn host_of_nested_dom(&self, nested: DomId) -> Option<(DomId, NodeId)> {
        self.states
            .iter()
            .find(|(_, state)| state.nested_dom_id == nested)
            .map(|((dom_id, node_id), _)| (*dom_id, *node_id))
    }

    #[must_use]
    pub fn all_view_keys(&self) -> Vec<(DomId, NodeId)> {
        self.states.keys().copied().collect()
    }

    /// Checks whether a `VirtualView` needs to be re-invoked and returns the reason
    ///
    /// Returns `Some(reason)` if the `VirtualView` callback should be invoked:
    /// - `InitialRender`: `VirtualView` has never been invoked
    /// - `BoundsExpanded`: Container grew larger than content
    /// - `EdgeScrolled`: User scrolled near an edge (for lazy loading)
    ///
    /// Returns `None` if no re-invocation is needed.
    pub fn check_reinvoke(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        scroll_manager: &ScrollManager,
        layout_bounds: LogicalRect,
    ) -> Option<VirtualViewCallbackReason> {
        // MWA-C-virtual_view: a staged reason override wins (set by
        // process_virtual_view_updates immediately before the invoke). The
        // old force_reinvoke path cleared was_invoked instead, which
        // collapsed EVERY queued re-invocation to InitialRender at delivery
        // time — user callbacks could never see EdgeScrolled/BoundsExpanded/
        // DomRecreated (the latter had zero producers at all).
        if let Some(pos) = self
            .reason_overrides
            .iter()
            .position(|((d, n), _)| *d == dom_id && *n == node_id)
        {
            let (_, reason) = self.reason_overrides.remove(pos);
            return Some(reason);
        }

        let state = self.states.entry((dom_id, node_id)).or_insert_with(|| {
            let nested_dom_id = DomId {
                inner: self.next_dom_id,
            };
            self.next_dom_id += 1;
            VirtualViewState::new(nested_dom_id)
        });

        if !state.virtual_view_was_invoked {
            // Remember where we started, so edge callbacks fire on scroll-to-edge,
            // not for the edge we happen to rest on at the initial position.
            state.initial_scroll_offset = scroll_manager
                .get_current_offset(dom_id, node_id)
                .unwrap_or_default();
            return Some(VirtualViewCallbackReason::InitialRender);
        }

        // Check for bounds expansion
        if layout_bounds.size.width > state.container.size.width
            || layout_bounds.size.height > state.container.size.height
        {
            state.invoked_for_current_expansion = false;
        }
        state.container = layout_bounds;

        let scroll_offset = scroll_manager
            .get_current_offset(dom_id, node_id)
            .unwrap_or_default();

        state.check_reinvoke_condition(scroll_offset, layout_bounds.size)
    }

    /// Returns debug info for all tracked `VirtualViews`
    ///
    /// Each entry contains: (`parent_dom_id`, `parent_node_id`, `nested_dom_id`,
    /// `scroll_size`, `virtual_scroll_size`, `was_invoked`, `last_bounds`)
    #[must_use]
    pub fn get_all_virtual_view_infos(&self) -> Vec<VirtualViewDebugInfo> {
        self.states
            .iter()
            .map(|((dom_id, node_id), state)| VirtualViewDebugInfo {
                parent_dom_id: dom_id.inner,
                parent_node_id: node_id.index(),
                nested_dom_id: state.nested_dom_id.inner,
                scroll_size_width: state.materialized.map(|m| m.size.width),
                scroll_size_height: state.materialized.map(|m| m.size.height),
                virtual_scroll_size_width: state.virtual_rect.map(|v| v.size.width),
                virtual_scroll_size_height: state.virtual_rect.map(|v| v.size.height),
                was_invoked: state.virtual_view_was_invoked,
                last_bounds_x: state.container.origin.x,
                last_bounds_y: state.container.origin.y,
                last_bounds_width: state.container.size.width,
                last_bounds_height: state.container.size.height,
            })
            .collect()
    }
}

/// Debug info for a single `VirtualView`, returned by `get_all_virtual_view_infos`
#[derive(Copy, Debug, Clone)]
pub struct VirtualViewDebugInfo {
    pub parent_dom_id: usize,
    pub parent_node_id: usize,
    pub nested_dom_id: usize,
    pub scroll_size_width: Option<f32>,
    pub scroll_size_height: Option<f32>,
    pub virtual_scroll_size_width: Option<f32>,
    pub virtual_scroll_size_height: Option<f32>,
    pub was_invoked: bool,
    pub last_bounds_x: f32,
    pub last_bounds_y: f32,
    pub last_bounds_width: f32,
    pub last_bounds_height: f32,
}

impl VirtualViewState {
    /// Creates a new `VirtualViewState` with the given nested DOM ID
    fn new(nested_dom_id: DomId) -> Self {
        Self {
            materialized: None,
            virtual_rect: None,
            virtual_view_was_invoked: false,
            invoked_for_current_expansion: false,
            invoked_for_current_edge: false,
            last_edge_triggered: EdgeFlags::default(),
            nested_dom_id,
            container: LogicalRect::zero(),
            initial_scroll_offset: LogicalPosition::zero(),
        }
    }

    /// Determines if the `VirtualView` callback should be re-invoked based on
    /// scroll position
    ///
    /// Checks two conditions:
    /// 1. Container bounds expanded beyond content size
    /// 2. User scrolled within `EDGE_THRESHOLD` pixels of an edge (for lazy loading)
    fn check_reinvoke_condition(
        &self,
        current_offset: LogicalPosition,
        container_size: LogicalSize,
    ) -> Option<VirtualViewCallbackReason> {
        // Nothing is materialized yet — nothing to be near the edge OF.
        let materialized = self.materialized?;
        // The document estimate; falls back to the materialized window when
        // the app reports no virtual extent (a VirtualView used as a plain
        // windowed view: then materialized IS the document).
        let virtual_rect = self.virtual_rect.unwrap_or(materialized);

        // Check 1: Container grew larger than the materialized content — the
        // window no longer fills the viewport, so ask for more.
        if !self.invoked_for_current_expansion
            && (container_size.width > materialized.size.width
                || container_size.height > materialized.size.height)
        {
            return Some(VirtualViewCallbackReason::BoundsExpanded);
        }

        // Check 2: the user scrolled near an edge of WHAT IS MATERIALIZED.
        //
        // This is the rule that makes a VirtualView scrollable rather than
        // merely lazy-loadable. The old test compared a VIRTUAL-space offset
        // against the MATERIALIZED window's size — two different spaces — so
        // it only ever fired at the absolute top/bottom of the document, and
        // a document scrolled in the middle never re-materialized at all.
        //
        // Both rects are in virtual space, so the comparison is honest: the
        // visible window is `[current_offset, current_offset + container]`,
        // and we re-invoke when it comes within EDGE_THRESHOLD of the edge of
        // `materialized` — but only when the document actually extends past
        // that edge, otherwise we would spin at the ends forever.
        let vis_min_x = current_offset.x;
        let vis_min_y = current_offset.y;
        let vis_max_x = current_offset.x + container_size.width;
        let vis_max_y = current_offset.y + container_size.height;

        let mat_min_x = materialized.origin.x;
        let mat_min_y = materialized.origin.y;
        let mat_max_x = materialized.origin.x + materialized.size.width;
        let mat_max_y = materialized.origin.y + materialized.size.height;

        let doc_min_x = virtual_rect.origin.x;
        let doc_min_y = virtual_rect.origin.y;
        let doc_max_x = virtual_rect.origin.x + virtual_rect.size.width;
        let doc_max_y = virtual_rect.origin.y + virtual_rect.size.height;

        let current_edges = EdgeFlags {
            // More document above what we materialized, and the view is near
            // the materialized window's top.
            top: mat_min_y > doc_min_y && (vis_min_y - mat_min_y) <= EDGE_THRESHOLD,
            bottom: mat_max_y < doc_max_y && (mat_max_y - vis_max_y) <= EDGE_THRESHOLD,
            left: mat_min_x > doc_min_x && (vis_min_x - mat_min_x) <= EDGE_THRESHOLD,
            right: mat_max_x < doc_max_x && (mat_max_x - vis_max_x) <= EDGE_THRESHOLD,
        };

        // Only treat an edge as "scrolled to" once the user has actually moved
        // from the resting position captured at InitialRender — sitting at the
        // initial top/left edge from the start is not an edge-scroll event.
        let has_scrolled = current_offset != self.initial_scroll_offset;

        // Trigger edge callback if near an edge that hasn't been triggered yet
        // Prioritize bottom/right edges (common infinite scroll directions)
        if has_scrolled && !self.invoked_for_current_edge && current_edges.any() {
            if current_edges.bottom && !self.last_edge_triggered.bottom {
                return Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom));
            }
            if current_edges.right && !self.last_edge_triggered.right {
                return Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Right));
            }
            if current_edges.top && !self.last_edge_triggered.top {
                return Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Top));
            }
            if current_edges.left && !self.last_edge_triggered.left {
                return Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Left));
            }
        }

        None
    }
}

impl EdgeFlags {
    /// Returns true if any edge flag is set
    #[allow(clippy::trivially_copy_pass_by_ref)] // <=8B Copy param kept by-ref intentionally (hot pixel/coord path or to avoid churning call sites for a perf-neutral change)
    const fn any(&self) -> bool {
        self.top || self.bottom || self.left || self.right
    }
}

impl From<EdgeType> for EdgeFlags {
    fn from(edge: EdgeType) -> Self {
        match edge {
            EdgeType::Top => Self {
                top: true,
                ..Default::default()
            },
            EdgeType::Bottom => Self {
                bottom: true,
                ..Default::default()
            },
            EdgeType::Left => Self {
                left: true,
                ..Default::default()
            },
            EdgeType::Right => Self {
                right: true,
                ..Default::default()
            },
        }
    }
}

impl crate::managers::NodeIdRemap for VirtualViewManager {
    /// Remap the `(DomId, NodeId)` keys of every tracked `VirtualView`.
    ///
    /// A `VirtualView` whose host node was unmounted has its state dropped —
    /// including the `nested_dom_id` binding, which would otherwise resurface
    /// on whatever node inherited the index (rendering the *wrong* nested DOM
    /// into it) and leak forever.
    fn remap_node_ids(&mut self, dom: DomId, map: &crate::managers::NodeIdMap) {
        crate::managers::remap_dom_keys(&mut self.states, dom, map);

        self.reason_overrides.retain_mut(|((d, node_id), _)| {
            if *d != dom {
                return true;
            }
            map.resolve(*node_id).is_some_and(|new_id| {
                *node_id = new_id;
                true
            })
        });
    }
}

// ============================================================================
// Adversarial unit tests (autotest fleet)
//
// Hostile inputs for every category in the task file: constructors (extreme
// args + post-construction invariants), getters/predicates (defined value on a
// default/empty instance), and the numeric decision functions
// (`check_reinvoke` / `check_reinvoke_condition` / `update_virtual_view_info`)
// under NaN / ±inf / f32::MAX / negative-overscroll / zero and at the exact
// EDGE_THRESHOLD boundary.
//
// An inline module can reach the private `states` / `next_dom_id` /
// `reason_overrides` fields and the private `VirtualViewState`, so the flag
// invariants are asserted directly rather than inferred.
//
// Every assertion documents the *actual* behavior — nothing is weakened to
// make it pass. Where the actual behavior looks wrong (stale `last_edge_triggered`
// suppressing repeat bottom-edge loads; NaN poisoning the growth check;
// `Default` handing out DomId 0), the test pins the current behavior and says so
// in a comment.
// ============================================================================
#[cfg(all(test, feature = "std"))]
mod autotest_generated {
    #![allow(clippy::float_cmp)] // deterministic inputs: exact float compares are intended

    use std::collections::BTreeSet;

    use azul_core::task::{Instant, SystemTick};

    use super::*;

    // ---------------------------------------------------------------- helpers

    const DOM: DomId = DomId::ROOT_ID;
    const DOM1: DomId = DomId { inner: 1 };
    const DOM_MAX: DomId = DomId { inner: usize::MAX };

    fn n(i: usize) -> NodeId {
        NodeId::new(i)
    }

    fn sz(width: f32, height: f32) -> LogicalSize {
        LogicalSize::new(width, height)
    }

    fn pos(x: f32, y: f32) -> LogicalPosition {
        LogicalPosition::new(x, y)
    }

    fn rect(width: f32, height: f32) -> LogicalRect {
        LogicalRect::new(LogicalPosition::zero(), sz(width, height))
    }

    /// Deterministic tick-clock instant — no wall clock, no flakiness.
    fn at(t: u64) -> Instant {
        Instant::Tick(SystemTick::new(t))
    }

    /// A `ScrollManager` reporting exactly `(x, y)` for `(dom, node)`.
    /// Unclamped, so overscroll / absurd offsets survive to `check_reinvoke`.
    fn scrolled(dom: DomId, node: NodeId, x: f32, y: f32) -> ScrollManager {
        let mut sm = ScrollManager::new();
        sm.set_scroll_position_unclamped(dom, node, pos(x, y), at(0));
        sm
    }

    fn st(m: &VirtualViewManager, dom: DomId, node: NodeId) -> &VirtualViewState {
        m.states.get(&(dom, node)).expect("state must exist")
    }

    /// How much document the manager-level fixture leaves UNMATERIALIZED below
    /// its window. A view whose materialized window IS the whole document is
    /// fully loaded, and by the edge rule nothing may fire for it — so a
    /// fixture that wants to observe an edge has to leave something to load.
    const FIXTURE_DOC_TAIL: f32 = 1000.0;

    /// Steady state: view `(DOM, n(1))` created, invoked once, `scroll`
    /// materialized at the document's top-left corner — with the document
    /// estimate `FIXTURE_DOC_TAIL` px taller than what is materialized, i.e. a
    /// real virtual view that still has content to load BELOW the window (and,
    /// deliberately, none above it and none to either side: the window spans
    /// the document's full width, as a vertically-scrolling list does).
    /// `initial_scroll_offset` stays at the (0, 0) default, so any nonzero offset
    /// counts as "the user has scrolled".
    fn ready_view(scroll: LogicalSize) -> VirtualViewManager {
        let mut m = VirtualViewManager::new();
        m.get_or_create_nested_dom_id(DOM, n(1));
        m.mark_invoked(DOM, n(1), VirtualViewCallbackReason::InitialRender)
            .expect("view exists");
        m.update_virtual_view_info(
            DOM,
            n(1),
            LogicalPosition::zero(),
            scroll,
            sz(scroll.width, scroll.height + FIXTURE_DOC_TAIL),
        )
        .expect("view exists");
        m
    }

    /// The general fixture for driving the private `check_reinvoke_condition`
    /// directly: an already-invoked state whose materialized window is `mat`
    /// and whose document estimate is `doc`, BOTH in virtual space. Spelling
    /// the two rects out is the point — the rule is entirely about where the
    /// window sits inside the document, and every "why did/didn't this fire?"
    /// answer is read off these two rects.
    fn windowed_state(mat: LogicalRect, doc: LogicalRect) -> VirtualViewState {
        let mut s = VirtualViewState::new(DomId { inner: 7 });
        s.virtual_view_was_invoked = true;
        s.materialized = Some(mat);
        s.virtual_rect = Some(doc);
        s
    }

    /// How far into the document the fixture's materialized window starts.
    /// The document extends this far ABOVE and BELOW it, so both the top and
    /// the bottom edge of the window have more document past them — which is
    /// what the edge rule is about. `vpos` maps a window-relative y (what the
    /// tests reason in) into virtual space.
    const FIXTURE_WINDOW_ORIGIN_Y: f32 = 1000.0;

    /// Same, for the x axis of the both-axes fixture `invoked_state_2d`.
    const FIXTURE_WINDOW_ORIGIN_X: f32 = 1000.0;

    fn vpos(y: f32) -> LogicalPosition {
        pos(0.0, y + FIXTURE_WINDOW_ORIGIN_Y)
    }

    /// A view windowed on the Y AXIS ONLY: a `scroll`-sized window parked
    /// 1000 px down a document that is 1000 px taller than the window at each
    /// end, and exactly as WIDE as the window.
    ///
    /// HORIZONTAL JUDGEMENT (deliberate, not an oversight): left/right can
    /// never fire on this fixture, because `mat_min_x == doc_min_x` and
    /// `mat_max_x == doc_max_x` — there is no document to either side, so
    /// there is nothing to load and silence is the correct answer. That is the
    /// shape a `VirtualView` actually ships in (a vertically-scrolling list),
    /// and it keeps the vertical assertions free of cross-axis noise: any edge
    /// these tests observe is unambiguously the one they aimed at.
    /// `invoked_state_2d` is the both-axes fixture, used wherever left/right is
    /// itself the property under test.
    fn invoked_state(scroll: LogicalSize) -> VirtualViewState {
        windowed_state(
            LogicalRect::new(pos(0.0, FIXTURE_WINDOW_ORIGIN_Y), scroll),
            LogicalRect::new(
                LogicalPosition::zero(),
                sz(scroll.width, FIXTURE_WINDOW_ORIGIN_Y * 2.0 + scroll.height),
            ),
        )
    }

    /// The same idea on BOTH axes: a `scroll`-sized window parked 1000 px into
    /// a document that extends 1000 px past it on all four sides. Left/right
    /// are exactly symmetric with top/bottom here, which is what makes edge
    /// priority and the horizontal threshold observable at all.
    fn invoked_state_2d(scroll: LogicalSize) -> VirtualViewState {
        windowed_state(
            LogicalRect::new(
                pos(FIXTURE_WINDOW_ORIGIN_X, FIXTURE_WINDOW_ORIGIN_Y),
                scroll,
            ),
            LogicalRect::new(
                LogicalPosition::zero(),
                sz(
                    FIXTURE_WINDOW_ORIGIN_X * 2.0 + scroll.width,
                    FIXTURE_WINDOW_ORIGIN_Y * 2.0 + scroll.height,
                ),
            ),
        )
    }

    // `Option`-returning mutators (the crate denies `unused_must_use`): these
    // wrappers also assert that the view actually existed.
    fn mark(m: &mut VirtualViewManager, dom: DomId, node: NodeId, r: VirtualViewCallbackReason) {
        m.mark_invoked(dom, node, r).expect("view exists");
    }

    fn set_sizes(
        m: &mut VirtualViewManager,
        dom: DomId,
        node: NodeId,
        scroll: LogicalSize,
        virt: LogicalSize,
    ) {
        m.update_virtual_view_info(dom, node, LogicalPosition::zero(), scroll, virt)
            .expect("view exists");
    }

    // ------------------------------------------------------- constructors

    #[test]
    fn new_is_empty_and_reserves_dom_id_zero_for_root() {
        let m = VirtualViewManager::new();

        assert_eq!(m.debug_counts(), 0);
        assert!(m.all_view_keys().is_empty());
        assert!(m.get_all_virtual_view_infos().is_empty());
        assert!(m.reason_overrides.is_empty());
        // 0 is the root DOM — nested ids start at 1.
        assert_eq!(m.next_dom_id, 1);

        // Getters on the empty instance are defined, not panicking.
        assert_eq!(m.get_nested_dom_id(DOM, n(0)), None);
        assert_eq!(m.get_nested_dom_id(DOM_MAX, n(usize::MAX)), None);
        assert!(!m.was_virtual_view_invoked(DOM, n(0)));
        assert!(!m.was_virtual_view_invoked(DOM_MAX, n(usize::MAX)));
    }

    #[test]
    fn derived_default_hands_out_root_dom_id_unlike_new() {
        // HAZARD (pinned, not a live bug): `new()` skips 0 because "0 is root",
        // but the derived `Default` starts the counter at 0, so a Default-built
        // manager hands out DomId::ROOT_ID as its first *nested* DOM id. Every
        // production site builds via `new()` (LayoutWindow does not derive
        // Default), so this is only reachable by a future caller.
        assert_eq!(VirtualViewManager::default().next_dom_id, 0);
        assert_eq!(VirtualViewManager::new().next_dom_id, 1);

        let mut d = VirtualViewManager::default();
        assert_eq!(d.get_or_create_nested_dom_id(DOM, n(0)), DomId::ROOT_ID);

        let mut fresh = VirtualViewManager::new();
        assert_ne!(fresh.get_or_create_nested_dom_id(DOM, n(0)), DomId::ROOT_ID);
    }

    #[test]
    fn virtual_view_state_new_invariants_at_extreme_dom_id() {
        let s = VirtualViewState::new(DomId { inner: usize::MAX });

        assert_eq!(s.nested_dom_id.inner, usize::MAX);
        assert!(s.materialized.is_none());
        assert!(s.virtual_rect.is_none());
        assert!(!s.virtual_view_was_invoked);
        assert!(!s.invoked_for_current_expansion);
        assert!(!s.invoked_for_current_edge);
        assert_eq!(s.last_edge_triggered, EdgeFlags::default());
        assert!(!s.last_edge_triggered.any());
        assert_eq!(s.container, LogicalRect::zero());
        assert_eq!(s.initial_scroll_offset, LogicalPosition::zero());

        // A brand-new state has no content size, so it can never ask to be
        // re-invoked, however absurd the container.
        assert_eq!(
            s.check_reinvoke_condition(pos(0.0, 0.0), sz(f32::INFINITY, f32::INFINITY)),
            None
        );
    }

    // --------------------------------------------- nested DOM id allocation

    #[test]
    fn get_or_create_is_idempotent_and_unique_per_key() {
        let mut m = VirtualViewManager::new();

        let a = m.get_or_create_nested_dom_id(DOM, n(0));
        let a_again = m.get_or_create_nested_dom_id(DOM, n(0));
        assert_eq!(a, a_again, "re-registering a view must not re-allocate");
        assert_eq!(a, DomId { inner: 1 });
        assert_eq!(m.debug_counts(), 1);

        // Saturated key components must not panic and must get a fresh id.
        let b = m.get_or_create_nested_dom_id(DOM_MAX, n(usize::MAX));
        assert_eq!(b, DomId { inner: 2 });
        assert_ne!(a, b);
        assert_eq!(m.debug_counts(), 2);

        assert_eq!(m.get_nested_dom_id(DOM, n(0)), Some(a));
        assert_eq!(m.get_nested_dom_id(DOM_MAX, n(usize::MAX)), Some(b));
        assert_eq!(m.get_nested_dom_id(DOM, n(1)), None);
        assert_eq!(m.get_nested_dom_id(DOM1, n(0)), None);
    }

    #[test]
    fn nested_dom_ids_are_unique_across_many_views() {
        let mut m = VirtualViewManager::new();
        let mut seen = BTreeSet::new();

        for dom in 0..8_usize {
            for node in 0..32_usize {
                let id = m.get_or_create_nested_dom_id(DomId { inner: dom }, n(node));
                assert!(id.inner >= 1, "nested id must never collide with the root");
                assert!(
                    seen.insert(id.inner),
                    "nested DOM id {id:?} handed out twice"
                );
            }
        }

        assert_eq!(seen.len(), 8 * 32);
        assert_eq!(m.debug_counts(), 8 * 32);
        assert_eq!(m.next_dom_id, 8 * 32 + 1);
    }

    #[test]
    fn all_view_keys_is_sorted_and_matches_the_tracked_states() {
        let mut m = VirtualViewManager::new();
        assert!(m.all_view_keys().is_empty());

        // Insert in deliberately reversed order — BTreeMap must still yield
        // ascending (DomId, NodeId).
        m.get_or_create_nested_dom_id(DOM1, n(9));
        m.get_or_create_nested_dom_id(DOM1, n(2));
        m.get_or_create_nested_dom_id(DOM, n(7));

        let keys = m.all_view_keys();
        assert_eq!(keys, vec![(DOM, n(7)), (DOM1, n(2)), (DOM1, n(9))]);
        assert_eq!(keys.len(), m.debug_counts());

        let mut sorted = keys.clone();
        sorted.sort_unstable();
        assert_eq!(keys, sorted);
    }

    // ------------------------------------------------ Option-returning mutators

    #[test]
    fn mutators_return_none_for_unknown_view_and_never_insert() {
        let mut m = VirtualViewManager::new();

        assert_eq!(
            m.update_virtual_view_info(
                DOM,
                n(3),
                LogicalPosition::zero(),
                sz(1.0, 1.0),
                sz(1.0, 1.0)
            ),
            None
        );
        assert_eq!(
            m.mark_invoked(DOM, n(3), VirtualViewCallbackReason::InitialRender),
            None
        );
        assert_eq!(m.force_reinvoke(DOM, n(3)), None);
        assert_eq!(
            m.update_virtual_view_info(
                DOM_MAX,
                n(usize::MAX),
                LogicalPosition::zero(),
                sz(0.0, 0.0),
                sz(0.0, 0.0)
            ),
            None
        );

        // Unlike check_reinvoke, none of these may lazily create a state.
        assert_eq!(m.debug_counts(), 0);
        assert_eq!(m.next_dom_id, 1);
    }

    // ------------------------------------------------------ reason overrides

    #[test]
    fn set_reason_override_keeps_only_the_latest_per_key() {
        let mut m = VirtualViewManager::new();

        for _ in 0..1_000 {
            m.set_reason_override(DOM, n(2), VirtualViewCallbackReason::DomRecreated);
        }
        m.set_reason_override(DOM, n(2), VirtualViewCallbackReason::BoundsExpanded);

        // Re-staging must overwrite, not accumulate.
        assert_eq!(m.reason_overrides.len(), 1);

        let sm = ScrollManager::new();
        assert_eq!(
            m.check_reinvoke(DOM, n(2), &sm, rect(10.0, 10.0)),
            Some(VirtualViewCallbackReason::BoundsExpanded)
        );
    }

    #[test]
    fn reason_override_is_consumed_exactly_once_and_does_not_create_state() {
        let mut m = VirtualViewManager::new();
        let sm = ScrollManager::new();

        m.set_reason_override(DOM, n(2), VirtualViewCallbackReason::ScrollBeyondContent);
        assert_eq!(
            m.check_reinvoke(DOM, n(2), &sm, rect(10.0, 10.0)),
            Some(VirtualViewCallbackReason::ScrollBeyondContent)
        );

        // The override short-circuits before the entry() call, so no state yet.
        assert!(m.reason_overrides.is_empty());
        assert_eq!(m.debug_counts(), 0);

        // Second call falls through to the normal path, which *does* create it.
        assert_eq!(
            m.check_reinvoke(DOM, n(2), &sm, rect(10.0, 10.0)),
            Some(VirtualViewCallbackReason::InitialRender)
        );
        assert_eq!(m.debug_counts(), 1);
        assert_eq!(m.get_nested_dom_id(DOM, n(2)), Some(DomId { inner: 1 }));
    }

    #[test]
    fn reason_overrides_do_not_leak_across_keys() {
        let mut m = VirtualViewManager::new();
        let sm = ScrollManager::new();

        m.set_reason_override(
            DOM,
            n(1),
            VirtualViewCallbackReason::EdgeScrolled(EdgeType::Left),
        );
        m.set_reason_override(DOM1, n(1), VirtualViewCallbackReason::DomRecreated);
        m.set_reason_override(DOM, n(2), VirtualViewCallbackReason::BoundsExpanded);
        assert_eq!(m.reason_overrides.len(), 3);

        // A different node of the same DOM must not steal DOM/n(1)'s override.
        assert_eq!(
            m.check_reinvoke(DOM, n(2), &sm, rect(1.0, 1.0)),
            Some(VirtualViewCallbackReason::BoundsExpanded)
        );
        assert_eq!(
            m.check_reinvoke(DOM1, n(1), &sm, rect(1.0, 1.0)),
            Some(VirtualViewCallbackReason::DomRecreated)
        );
        assert_eq!(
            m.check_reinvoke(DOM, n(1), &sm, rect(1.0, 1.0)),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Left))
        );
        assert!(m.reason_overrides.is_empty());
    }

    // ------------------------------------------- update_virtual_view_info (numeric)

    #[test]
    fn update_virtual_view_info_zero_and_extreme_sizes_do_not_panic() {
        let mut m = VirtualViewManager::new();
        m.get_or_create_nested_dom_id(DOM, n(1));

        for size in [
            sz(0.0, 0.0),
            sz(-0.0, -0.0),
            sz(f32::MAX, f32::MAX),
            sz(f32::MIN, f32::MIN),
            sz(f32::INFINITY, f32::NEG_INFINITY),
            sz(-1.0e30, 1.0e30),
            sz(f32::MIN_POSITIVE, f32::EPSILON),
        ] {
            assert_eq!(
                m.update_virtual_view_info(DOM, n(1), LogicalPosition::zero(), size, size),
                Some(()),
                "size {size:?} must be recorded without panicking"
            );
            assert_eq!(st(&m, DOM, n(1)).materialized.map(|r| r.size), Some(size));
            assert_eq!(st(&m, DOM, n(1)).virtual_rect.map(|r| r.size), Some(size));
        }

        // NaN is stored verbatim (no normalization, no panic).
        assert_eq!(
            m.update_virtual_view_info(
                DOM,
                n(1),
                LogicalPosition::zero(),
                sz(f32::NAN, f32::NAN),
                sz(f32::NAN, 1.0)
            ),
            Some(())
        );
        let stored = st(&m, DOM, n(1)).materialized.map(|r| r.size).unwrap();
        assert!(stored.width.is_nan() && stored.height.is_nan());
    }

    #[test]
    fn update_virtual_view_info_clears_expansion_flag_only_when_content_grows() {
        let mut m = ready_view(sz(100.0, 100.0));
        mark(&mut m, DOM, n(1), VirtualViewCallbackReason::BoundsExpanded);
        assert!(st(&m, DOM, n(1)).invoked_for_current_expansion);

        // Shrink: not a growth, flag survives.
        set_sizes(&mut m, DOM, n(1), sz(50.0, 50.0), sz(50.0, 50.0));
        assert!(st(&m, DOM, n(1)).invoked_for_current_expansion);

        // Same size: still not a growth (strict `>`).
        set_sizes(&mut m, DOM, n(1), sz(50.0, 50.0), sz(50.0, 50.0));
        assert!(st(&m, DOM, n(1)).invoked_for_current_expansion);

        // One axis grows by an epsilon: flag cleared, BoundsExpanded can re-fire.
        set_sizes(&mut m, DOM, n(1), sz(50.000_01, 50.0), sz(50.0, 50.0));
        assert!(!st(&m, DOM, n(1)).invoked_for_current_expansion);
    }

    #[test]
    fn update_virtual_view_info_infinite_growth_clears_the_flag() {
        let mut m = ready_view(sz(100.0, 100.0));
        mark(&mut m, DOM, n(1), VirtualViewCallbackReason::BoundsExpanded);

        set_sizes(
            &mut m,
            DOM,
            n(1),
            sz(f32::INFINITY, 100.0),
            sz(f32::INFINITY, 100.0),
        );
        assert!(!st(&m, DOM, n(1)).invoked_for_current_expansion);
    }

    #[test]
    fn update_virtual_view_info_nan_size_poisons_the_growth_check() {
        // PINNED QUIRK: growth is `new > old`, and every comparison against NaN
        // is false. Once a NaN content size is recorded, *no* later size — not
        // even 1e9 — is seen as growth, so `invoked_for_current_expansion` can
        // never be cleared here again. Only force_reinvoke/reset_all recover.
        let mut m = ready_view(sz(100.0, 100.0));
        mark(&mut m, DOM, n(1), VirtualViewCallbackReason::BoundsExpanded);
        assert!(st(&m, DOM, n(1)).invoked_for_current_expansion);

        // NaN is not > 100.0 → no clear (and no panic).
        set_sizes(
            &mut m,
            DOM,
            n(1),
            sz(f32::NAN, f32::NAN),
            sz(f32::NAN, f32::NAN),
        );
        assert!(st(&m, DOM, n(1)).invoked_for_current_expansion);

        // 1e9 is not > NaN either → still no clear.
        set_sizes(&mut m, DOM, n(1), sz(1.0e9, 1.0e9), sz(1.0e9, 1.0e9));
        assert!(st(&m, DOM, n(1)).invoked_for_current_expansion);

        // The recovery path still works.
        m.force_reinvoke(DOM, n(1)).expect("view exists");
        assert!(!st(&m, DOM, n(1)).invoked_for_current_expansion);
    }

    // ----------------------------------------------------------- mark_invoked

    #[test]
    fn mark_invoked_sets_only_the_flags_the_reason_owns() {
        for reason in [
            VirtualViewCallbackReason::InitialRender,
            VirtualViewCallbackReason::DomRecreated,
            VirtualViewCallbackReason::ScrollBeyondContent,
        ] {
            let mut m = VirtualViewManager::new();
            m.get_or_create_nested_dom_id(DOM, n(1));
            assert_eq!(m.mark_invoked(DOM, n(1), reason), Some(()));

            let s = st(&m, DOM, n(1));
            assert!(s.virtual_view_was_invoked, "{reason:?} must mark invoked");
            assert!(!s.invoked_for_current_expansion, "{reason:?}");
            assert!(!s.invoked_for_current_edge, "{reason:?}");
            assert_eq!(s.last_edge_triggered, EdgeFlags::default(), "{reason:?}");
            assert!(m.was_virtual_view_invoked(DOM, n(1)));
        }

        let mut m = VirtualViewManager::new();
        m.get_or_create_nested_dom_id(DOM, n(1));
        mark(&mut m, DOM, n(1), VirtualViewCallbackReason::BoundsExpanded);
        let s = st(&m, DOM, n(1));
        assert!(s.virtual_view_was_invoked);
        assert!(s.invoked_for_current_expansion);
        assert!(!s.invoked_for_current_edge);
        assert_eq!(s.last_edge_triggered, EdgeFlags::default());
    }

    #[test]
    fn edge_type_round_trips_through_mark_invoked_into_edge_flags() {
        for edge in [
            EdgeType::Top,
            EdgeType::Bottom,
            EdgeType::Left,
            EdgeType::Right,
        ] {
            let mut m = VirtualViewManager::new();
            m.get_or_create_nested_dom_id(DOM, n(1));
            mark(
                &mut m,
                DOM,
                n(1),
                VirtualViewCallbackReason::EdgeScrolled(edge),
            );

            let s = st(&m, DOM, n(1));
            assert!(s.virtual_view_was_invoked);
            assert!(s.invoked_for_current_edge);
            // encode(edge) == decode: the stored flags are exactly EdgeFlags::from(edge).
            assert_eq!(s.last_edge_triggered, EdgeFlags::from(edge), "{edge:?}");
            assert!(s.last_edge_triggered.any(), "{edge:?}");

            let f = s.last_edge_triggered;
            let set = usize::from(f.top)
                + usize::from(f.bottom)
                + usize::from(f.left)
                + usize::from(f.right);
            assert_eq!(set, 1, "{edge:?} must set exactly one flag");
            // Expansion is a different trigger and must stay untouched.
            assert!(!s.invoked_for_current_expansion, "{edge:?}");
        }
    }

    // ---------------------------------------------------- reset / force_reinvoke

    #[test]
    fn reset_all_invocation_flags_on_empty_manager_is_a_noop() {
        let mut m = VirtualViewManager::new();
        m.reset_all_invocation_flags();
        assert_eq!(m.debug_counts(), 0);
        assert_eq!(m.next_dom_id, 1);
        assert!(m.all_view_keys().is_empty());
    }

    #[test]
    fn reset_all_clears_every_flag_but_preserves_identity_sizes_and_bounds() {
        let mut m = ready_view(sz(100.0, 1000.0));
        let nested = m.get_nested_dom_id(DOM, n(1)).expect("view exists");
        m.get_or_create_nested_dom_id(DOM1, n(4));
        mark(
            &mut m,
            DOM,
            n(1),
            VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom),
        );
        mark(
            &mut m,
            DOM1,
            n(4),
            VirtualViewCallbackReason::BoundsExpanded,
        );

        // Record a non-zero last_bounds through the normal path.
        let sm = scrolled(DOM, n(1), 0.0, 900.0);
        let _ = m.check_reinvoke(DOM, n(1), &sm, rect(100.0, 100.0));
        // Overrides are a separate queue: reset must not touch them.
        m.set_reason_override(DOM, n(1), VirtualViewCallbackReason::DomRecreated);

        m.reset_all_invocation_flags();

        for (dom, node) in [(DOM, n(1)), (DOM1, n(4))] {
            let s = st(&m, dom, node);
            assert!(!s.virtual_view_was_invoked);
            assert!(!s.invoked_for_current_expansion);
            assert!(!s.invoked_for_current_edge);
            assert_eq!(s.last_edge_triggered, EdgeFlags::default());
            assert!(!m.was_virtual_view_invoked(dom, node));
        }

        // Identity, content size and bounds survive — only the flags reset.
        let s = st(&m, DOM, n(1));
        assert_eq!(s.nested_dom_id, nested);
        assert_eq!(s.materialized.map(|r| r.size), Some(sz(100.0, 1000.0)));
        assert_eq!(s.container, rect(100.0, 100.0));
        assert_eq!(m.debug_counts(), 2);
        assert_eq!(m.reason_overrides.len(), 1);
    }

    #[test]
    fn force_reinvoke_yields_initial_render_but_leaves_last_edge_triggered_set() {
        let mut m = ready_view(sz(100.0, 1000.0));
        mark(
            &mut m,
            DOM,
            n(1),
            VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom),
        );

        assert_eq!(m.force_reinvoke(DOM, n(1)), Some(()));
        let s = st(&m, DOM, n(1));
        assert!(!s.virtual_view_was_invoked);
        assert!(!s.invoked_for_current_expansion);
        assert!(!s.invoked_for_current_edge);
        // ASYMMETRY (pinned): unlike reset_all_invocation_flags, force_reinvoke
        // does NOT clear last_edge_triggered — see the bottom-edge suppression
        // test below for the consequence.
        assert_eq!(s.last_edge_triggered, EdgeFlags::from(EdgeType::Bottom));

        // The documented effect still holds: the next check is an InitialRender.
        let sm = scrolled(DOM, n(1), 0.0, 900.0);
        assert_eq!(
            m.check_reinvoke(DOM, n(1), &sm, rect(100.0, 100.0)),
            Some(VirtualViewCallbackReason::InitialRender)
        );
    }

    // -------------------------------------------------- check_reinvoke (numeric)

    #[test]
    fn check_reinvoke_creates_the_state_for_an_unknown_view() {
        let mut m = VirtualViewManager::new();
        let sm = ScrollManager::new();

        assert_eq!(
            m.check_reinvoke(DOM, n(3), &sm, rect(100.0, 100.0)),
            Some(VirtualViewCallbackReason::InitialRender)
        );
        assert_eq!(m.debug_counts(), 1);
        assert_eq!(m.get_nested_dom_id(DOM, n(3)), Some(DomId { inner: 1 }));
        assert!(!m.was_virtual_view_invoked(DOM, n(3)));

        // Re-checking without marking must keep returning InitialRender and must
        // NOT keep allocating states/ids (unbounded growth guard).
        for _ in 0..16 {
            assert_eq!(
                m.check_reinvoke(DOM, n(3), &sm, rect(100.0, 100.0)),
                Some(VirtualViewCallbackReason::InitialRender)
            );
        }
        assert_eq!(m.debug_counts(), 1);
        assert_eq!(m.next_dom_id, 2);

        // Saturated key: no panic, fresh id.
        assert_eq!(
            m.check_reinvoke(DOM_MAX, n(usize::MAX), &sm, rect(0.0, 0.0)),
            Some(VirtualViewCallbackReason::InitialRender)
        );
        assert_eq!(
            m.get_nested_dom_id(DOM_MAX, n(usize::MAX)),
            Some(DomId { inner: 2 })
        );
    }

    #[test]
    fn check_reinvoke_is_none_while_no_content_size_is_known() {
        let mut m = VirtualViewManager::new();
        m.get_or_create_nested_dom_id(DOM, n(1));
        mark(&mut m, DOM, n(1), VirtualViewCallbackReason::InitialRender);

        let sm = scrolled(DOM, n(1), 0.0, 5_000.0);
        // scroll_size is still None → the `?` bails out, whatever the bounds.
        assert_eq!(
            m.check_reinvoke(DOM, n(1), &sm, rect(f32::MAX, f32::MAX)),
            None
        );
        assert_eq!(m.check_reinvoke(DOM, n(1), &sm, rect(0.0, 0.0)), None);
    }

    #[test]
    fn check_reinvoke_bounds_expanded_fires_once_per_growth() {
        let mut m = ready_view(sz(100.0, 100.0));
        let sm = ScrollManager::new();

        assert_eq!(
            m.check_reinvoke(DOM, n(1), &sm, rect(200.0, 200.0)),
            Some(VirtualViewCallbackReason::BoundsExpanded)
        );
        mark(&mut m, DOM, n(1), VirtualViewCallbackReason::BoundsExpanded);

        // Same bounds again → already invoked for this expansion → quiet.
        assert_eq!(m.check_reinvoke(DOM, n(1), &sm, rect(200.0, 200.0)), None);
        // Shrinking is never a re-invoke trigger.
        assert_eq!(m.check_reinvoke(DOM, n(1), &sm, rect(150.0, 150.0)), None);
        // Growing past the last bounds re-arms it.
        assert_eq!(
            m.check_reinvoke(DOM, n(1), &sm, rect(300.0, 300.0)),
            Some(VirtualViewCallbackReason::BoundsExpanded)
        );
        assert_eq!(st(&m, DOM, n(1)).container, rect(300.0, 300.0));
    }

    #[test]
    fn check_reinvoke_does_not_fire_an_edge_for_the_resting_start_position() {
        // Regression guard for the initial_scroll_offset rule: a view that
        // starts at offset 0 is *at* the top edge, but that is the initial
        // position, not a scroll-to-edge event.
        let mut m = VirtualViewManager::new();
        m.get_or_create_nested_dom_id(DOM, n(1));
        let sm = scrolled(DOM, n(1), 0.0, 0.0);

        assert_eq!(
            m.check_reinvoke(DOM, n(1), &sm, rect(100.0, 100.0)),
            Some(VirtualViewCallbackReason::InitialRender)
        );
        assert_eq!(st(&m, DOM, n(1)).initial_scroll_offset, pos(0.0, 0.0));
        mark(&mut m, DOM, n(1), VirtualViewCallbackReason::InitialRender);
        set_sizes(&mut m, DOM, n(1), sz(100.0, 1000.0), sz(100.0, 1000.0));

        // Still parked at the top edge, hasn't moved → no EdgeScrolled(Top).
        assert_eq!(m.check_reinvoke(DOM, n(1), &sm, rect(100.0, 100.0)), None);
    }

    #[test]
    fn check_reinvoke_edge_scrolled_bottom_then_stays_quiet() {
        let mut m = ready_view(sz(100.0, 1000.0));
        let sm = scrolled(DOM, n(1), 0.0, 900.0);

        assert_eq!(
            m.check_reinvoke(DOM, n(1), &sm, rect(100.0, 100.0)),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom))
        );
        mark(
            &mut m,
            DOM,
            n(1),
            VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom),
        );

        // invoked_for_current_edge gates the whole edge block → no duplicate.
        assert_eq!(m.check_reinvoke(DOM, n(1), &sm, rect(100.0, 100.0)), None);
    }

    #[test]
    fn bottom_edge_is_suppressed_after_a_force_reinvoke_but_not_after_a_reset() {
        // PINNED BUG-SHAPED BEHAVIOR: force_reinvoke clears invoked_for_current_edge
        // but NOT last_edge_triggered, so a second genuine scroll-to-bottom produces
        // no EdgeScrolled(Bottom) — an infinite-scroll list stops lazy-loading after
        // the first page. reset_all_invocation_flags (which does clear the edge
        // memory) re-arms it; the two halves below are identical except for that
        // one call, which isolates the stale flag as the cause.
        //
        // Geometry (`ready_view`): the 100x1000 window is materialized at the
        // document's ORIGIN, and the document is 2000 px tall — so offsets here
        // are already virtual-space offsets and `vpos` (which is for the
        // 1000-px-down `invoked_state` window) must NOT be applied to them.
        // Scrolling to y=900 puts the viewport's bottom flush against the
        // window's bottom edge, with 1000 px of document still to load below.
        let bottom = scrolled(DOM, n(1), 0.0, 900.0);
        let middle = scrolled(DOM, n(1), 0.0, 400.0);
        let bounds = rect(100.0, 100.0);

        let mut m = ready_view(sz(100.0, 1000.0));
        assert_eq!(
            m.check_reinvoke(DOM, n(1), &bottom, bounds),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom))
        );
        mark(
            &mut m,
            DOM,
            n(1),
            VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom),
        );

        // --- half 1: the trigger_virtual_view_rerender() path -----------------
        m.force_reinvoke(DOM, n(1)).expect("view exists");
        // Re-invoked while the user sits mid-list, so the resting position is 400.
        assert_eq!(
            m.check_reinvoke(DOM, n(1), &middle, bounds),
            Some(VirtualViewCallbackReason::InitialRender)
        );
        mark(&mut m, DOM, n(1), VirtualViewCallbackReason::InitialRender);
        assert_eq!(st(&m, DOM, n(1)).initial_scroll_offset, pos(0.0, 400.0));

        // The user now really scrolls 400 → 900 (a scroll-to-edge, and the
        // invoked_for_current_edge gate is open), yet nothing fires.
        assert_eq!(
            m.check_reinvoke(DOM, n(1), &bottom, bounds),
            None,
            "stale last_edge_triggered.bottom suppresses the second bottom-edge load"
        );
        assert!(st(&m, DOM, n(1)).last_edge_triggered.bottom);

        // --- half 2: the same sequence, but through reset_all -----------------
        m.reset_all_invocation_flags();
        assert_eq!(
            m.check_reinvoke(DOM, n(1), &middle, bounds),
            Some(VirtualViewCallbackReason::InitialRender)
        );
        mark(&mut m, DOM, n(1), VirtualViewCallbackReason::InitialRender);
        assert_eq!(
            m.check_reinvoke(DOM, n(1), &bottom, bounds),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom)),
            "reset_all clears the edge memory, so the identical scroll does fire"
        );
    }

    #[test]
    fn check_reinvoke_with_nan_bounds_is_quiet_and_stores_nan() {
        let mut m = ready_view(sz(100.0, 100.0));
        let sm = ScrollManager::new();
        let nan_rect = LogicalRect::new(pos(f32::NAN, f32::NAN), sz(f32::NAN, f32::NAN));

        // Every NaN comparison is false → no expansion, no scrollable axis.
        assert_eq!(m.check_reinvoke(DOM, n(1), &sm, nan_rect), None);

        let info = m.get_all_virtual_view_infos();
        assert_eq!(info.len(), 1);
        assert!(info[0].last_bounds_x.is_nan());
        assert!(info[0].last_bounds_width.is_nan());
        assert!(info[0].last_bounds_height.is_nan());
    }

    #[test]
    fn check_reinvoke_with_infinite_bounds_reports_bounds_expanded() {
        let mut m = ready_view(sz(100.0, 100.0));
        let sm = ScrollManager::new();

        assert_eq!(
            m.check_reinvoke(DOM, n(1), &sm, rect(f32::INFINITY, f32::INFINITY)),
            Some(VirtualViewCallbackReason::BoundsExpanded)
        );
    }

    // ------------------------------- check_reinvoke_condition (private, numeric)

    #[test]
    fn edge_threshold_is_exactly_200_px_and_inclusive() {
        assert_eq!(EDGE_THRESHOLD, 200.0);

        // Every distance below is measured between the VISIBLE window
        // (`[offset, offset + container]`) and the MATERIALIZED window's edge —
        // never the document's. The document only decides whether an edge is
        // allowed to fire at all (is there anything left to load past it?).
        let s = invoked_state(sz(100.0, 1000.0)); // window y 1000..2000 of a 0..3000 doc
        let container = sz(100.0, 100.0);

        // Bottom edge: mat_max_y - vis_max_y == 2000 - 1800 == 200 → inclusive hit.
        assert_eq!(
            s.check_reinvoke_condition(vpos(700.0), container),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom))
        );
        // One px further from the bottom (201) and not near the top → quiet.
        assert_eq!(s.check_reinvoke_condition(vpos(699.0), container), None);

        // Top edge: vis_min_y - mat_min_y == 1200 - 1000 == 200 → inclusive hit.
        assert_eq!(
            s.check_reinvoke_condition(vpos(200.0), container),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Top))
        );
        // Just past it, and still 699 px from the bottom → quiet.
        assert_eq!(s.check_reinvoke_condition(vpos(201.0), container), None);

        // The horizontal axis uses the identical threshold with identical
        // inclusivity. It needs the both-axes fixture: on `invoked_state` the
        // window spans the document's full width, so left/right have nothing to
        // load and correctly never fire whatever the distance.
        let s2 = invoked_state_2d(sz(1000.0, 1000.0)); // window 1000..2000 of a 0..3000 doc, both axes
                                                       // y is parked dead centre of the window (450 px from either vertical
                                                       // edge) so that only the x axis can speak.
        let quiet_y = FIXTURE_WINDOW_ORIGIN_Y + 450.0;

        // Left edge: vis_min_x - mat_min_x == 1200 - 1000 == 200 → inclusive hit.
        assert_eq!(
            s2.check_reinvoke_condition(pos(1200.0, quiet_y), container),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Left))
        );
        assert_eq!(
            s2.check_reinvoke_condition(pos(1201.0, quiet_y), container),
            None
        );

        // Right edge: mat_max_x - vis_max_x == 2000 - 1800 == 200 → inclusive hit.
        assert_eq!(
            s2.check_reinvoke_condition(pos(1700.0, quiet_y), container),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Right))
        );
        assert_eq!(
            s2.check_reinvoke_condition(pos(1699.0, quiet_y), container),
            None
        );
    }

    #[test]
    fn edge_priority_is_bottom_right_top_left_and_drains() {
        // Several edges have to be near AT ONCE for priority to mean anything,
        // which takes a small viewport inside a window that has document past
        // it on all four sides — hence the both-axes fixture. A 900 px viewport
        // sitting on the top-left corner of the 1000x1000 window is 0 px from
        // its top and left edges and 100 px from its bottom and right ones, so
        // all four are inside EDGE_THRESHOLD simultaneously.
        let mut s = invoked_state_2d(sz(1000.0, 1000.0));
        let container = sz(900.0, 900.0);
        let offset = pos(FIXTURE_WINDOW_ORIGIN_X, FIXTURE_WINDOW_ORIGIN_Y);

        assert_eq!(
            s.check_reinvoke_condition(offset, container),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom))
        );

        s.last_edge_triggered.bottom = true;
        assert_eq!(
            s.check_reinvoke_condition(offset, container),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Right))
        );

        s.last_edge_triggered.right = true;
        assert_eq!(
            s.check_reinvoke_condition(offset, container),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Top))
        );

        s.last_edge_triggered.top = true;
        assert_eq!(
            s.check_reinvoke_condition(offset, container),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Left))
        );

        // All four drained: near an edge, but nothing left to report.
        s.last_edge_triggered.left = true;
        assert_eq!(s.check_reinvoke_condition(offset, container), None);
    }

    #[test]
    fn check_reinvoke_condition_at_zero_is_quiet() {
        // Zero content, zero container, zero offset: `0 > 0` is false so there
        // is no expansion, and the offset is exactly the resting
        // `initial_scroll_offset`, so no edge may fire either.
        let s = invoked_state(sz(0.0, 0.0));
        assert_eq!(
            s.check_reinvoke_condition(pos(0.0, 0.0), sz(0.0, 0.0)),
            None
        );

        // Zero-size content inside a real container *is* an expansion.
        assert_eq!(
            s.check_reinvoke_condition(pos(0.0, 0.0), sz(1.0, 1.0)),
            Some(VirtualViewCallbackReason::BoundsExpanded)
        );
    }

    #[test]
    fn check_reinvoke_condition_handles_nan_offset_and_nan_sizes() {
        let nan = f32::NAN;

        // JUDGEMENT (NaN). The rule is four independent comparisons, and every
        // comparison against NaN is false — so NaN does NOT poison the whole
        // decision, it silences exactly the edges whose arithmetic touches it
        // while the others still answer. Which edges those are depends on where
        // the NaN is, so each case is verified in both directions (the edge
        // that survives fires; the edge that was poisoned stays quiet) rather
        // than asserted wholesale as `None`. Nothing here may panic.

        // (1) NaN materialized/document SIZES. `bottom`/`right` are derived
        // from those sizes → NaN → false. `top`/`left` are derived from ORIGINS
        // only, so they are NaN-free: parked on the window's top edge with
        // 1000 px of document above it, Top still fires.
        let s = invoked_state(sz(nan, nan));
        assert_eq!(
            s.check_reinvoke_condition(vpos(0.0), sz(100.0, 100.0)),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Top)),
            "the top edge is computed from origins alone, so a NaN size cannot silence it"
        );
        // Far below the top, where only the (NaN-poisoned) bottom edge could
        // have fired — proof that the NaN really did silence it.
        assert_eq!(
            s.check_reinvoke_condition(vpos(5_000.0), sz(100.0, 100.0)),
            None
        );

        // (2) NaN offset AND NaN sizes: nothing is left that can compare true.
        assert_eq!(
            s.check_reinvoke_condition(pos(nan, nan), sz(nan, nan)),
            None
        );

        // (3) NaN CONTAINER size against real content. The container size only
        // enters the bottom/right distances, so Top survives again...
        let s = invoked_state(sz(100.0, 1000.0));
        assert_eq!(
            s.check_reinvoke_condition(vpos(0.0), sz(nan, nan)),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Top))
        );
        // ...and the bottom edge is the one that goes quiet, at an offset that
        // would otherwise be a dead-on bottom hit.
        assert_eq!(s.check_reinvoke_condition(vpos(900.0), sz(nan, nan)), None);

        // (4) NaN scroll OFFSET. It appears in all four distances, so every
        // edge predicate is false and NO edge fires — verified, not assumed —
        // even though `has_scrolled` is true: NaN quantizes to the dedicated
        // i64::MIN sentinel, which is != the quantized 0 of the resting offset.
        assert_ne!(
            pos(nan, nan),
            LogicalPosition::zero(),
            "a NaN offset counts as 'has scrolled', so the edge block IS entered"
        );
        assert_eq!(
            s.check_reinvoke_condition(pos(nan, nan), sz(100.0, 100.0)),
            None
        );
    }

    #[test]
    fn check_reinvoke_condition_handles_negative_overscroll_offsets() {
        let s = invoked_state(sz(100.0, 1000.0));
        let container = sz(100.0, 100.0);

        // Rubber-band overscroll far above the materialized top: the top edge
        // fires, because this window has 1000 px of document above it. (The
        // bottom is ~1e9 px away, and the fixture is not windowed on x.)
        assert_eq!(
            s.check_reinvoke_condition(vpos(-1.0e9), container),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Top))
        );
        assert_eq!(
            s.check_reinvoke_condition(pos(-50.0, -1.0), container),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Top))
        );

        // JUDGEMENT: a negative offset is not a top-edge event by ITSELF — the
        // sign of the offset is irrelevant, what matters is whether there is
        // document above the materialized window. With the window flush against
        // the document's top there is nothing left to load up there, so the
        // very same rubber-band overscroll is silence, not a reload loop
        // (which is precisely what a rubber-band bounce would otherwise cause,
        // once per frame, for the whole duration of the bounce).
        let at_doc_top = windowed_state(
            LogicalRect::new(LogicalPosition::zero(), sz(100.0, 1000.0)),
            LogicalRect::new(LogicalPosition::zero(), sz(100.0, 3000.0)),
        );
        assert_eq!(
            at_doc_top.check_reinvoke_condition(pos(0.0, -1.0e9), container),
            None
        );
        assert_eq!(
            at_doc_top.check_reinvoke_condition(pos(-1.0e9, -1.0e9), container),
            None
        );

        // Overscrolled past the bottom: still the bottom edge, no panic.
        assert_eq!(
            s.check_reinvoke_condition(vpos(1.0e9), container),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom))
        );

        // JUDGEMENT: negative container/content sizes are nonsense input — an
        // INVERTED rect, whose max lies below its min. The rule stays total on
        // them (no panic, no NaN, a deterministic answer), but it cannot be
        // *meaningful*, and pinning `None` here would pretend the inversion is
        // detected when it is not. What actually happens: the inverted window's
        // "bottom" (y=900) sits 100 px short of the inverted document's
        // (y=1900), so the bottom edge reports. Totality is the guarantee; the
        // particular edge is an artefact of the garbage, recorded so a future
        // change to it is noticed rather than silently absorbed.
        let s = invoked_state(sz(-100.0, -100.0));
        assert_eq!(
            s.check_reinvoke_condition(vpos(0.0), sz(-200.0, -200.0)),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom))
        );
    }

    #[test]
    fn check_reinvoke_condition_saturates_at_f32_extremes() {
        // JUDGEMENT (extremes): the requirement is that saturated arithmetic
        // neither panics NOR invents an edge. The second half is the subtle
        // one — at f32::MAX the distances collapse to 0 and *look* like a
        // permanent edge hit, and the only thing standing between that and an
        // infinite re-materialize loop is the "does the document actually
        // extend past this edge?" guard. Both halves are pinned below.

        // A MAX-sized window inside a document that saturates to the SAME MAX
        // (`1000 + MAX == MAX` and `2000 + MAX == MAX` in f32): the window
        // covers everything there is. Scrolled to the far end the bottom
        // distance is MAX - MAX == 0 — inside the threshold — yet nothing
        // fires, because there is nothing beyond the window to load.
        let s = invoked_state(sz(f32::MAX, f32::MAX));
        assert_eq!(
            s.check_reinvoke_condition(pos(f32::MAX, f32::MAX), sz(0.0, 0.0)),
            None,
            "a saturated distance of 0 must not fake an edge when the window covers the document"
        );

        // The mirrored extreme: -MAX is far above the window, and the 1000 px
        // of document above it survive saturation (the window's ORIGIN is
        // still 1000, the document's still 0), so the top edge does fire.
        assert_eq!(
            s.check_reinvoke_condition(pos(-f32::MAX, -f32::MAX), sz(0.0, 0.0)),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Top))
        );

        // MAX container over MAX content: `>` is strict, so nothing grew and
        // there is no BoundsExpanded — but the viewport starts exactly on the
        // window's top edge, which has document above it, so this is a Top.
        assert_eq!(
            s.check_reinvoke_condition(vpos(0.0), sz(f32::MAX, f32::MAX)),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Top))
        );

        // Saturation where the document genuinely DOES extend past the window:
        // a MAX/2 window at the document's origin inside a MAX-tall document.
        // The bottom distance underflows to -MAX/2 (the viewport is absurdly
        // far past the window) — still <= EDGE_THRESHOLD, so the bottom edge
        // reports instead of overflowing.
        let huge = windowed_state(
            LogicalRect::new(LogicalPosition::zero(), sz(f32::MAX / 2.0, f32::MAX / 2.0)),
            LogicalRect::new(LogicalPosition::zero(), sz(f32::MAX, f32::MAX)),
        );
        assert_eq!(
            huge.check_reinvoke_condition(pos(f32::MAX, f32::MAX), sz(0.0, 0.0)),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom))
        );
        // Mirrored: that window is flush with the document's top, so an -MAX
        // offset has nothing to load above it. The bottom distance
        // (MAX/2 + MAX) overflows to +inf, which reads as "very far away" —
        // not as a panic and not as an edge.
        assert_eq!(
            huge.check_reinvoke_condition(pos(-f32::MAX, -f32::MAX), sz(0.0, 0.0)),
            None
        );

        // Infinite container over finite content is an expansion...
        let s = invoked_state(sz(100.0, 100.0));
        assert_eq!(
            s.check_reinvoke_condition(vpos(0.0), sz(f32::INFINITY, f32::INFINITY)),
            Some(VirtualViewCallbackReason::BoundsExpanded)
        );
        // ...and once that expansion has been served, an infinite viewport is
        // "at" every edge at once (`mat_max - inf == -inf <= 200`), so priority
        // answers Bottom. Absurd, but deterministic and bounded: the document
        // guard is still the thing that decides, as the next case shows.
        let mut s = invoked_state(sz(100.0, 100.0));
        s.invoked_for_current_expansion = true;
        assert_eq!(
            s.check_reinvoke_condition(vpos(0.0), sz(f32::INFINITY, f32::INFINITY)),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom))
        );

        // Same infinite viewport over a FULLY materialized view: every edge is
        // 0/-inf away and every one of them is vetoed, so it goes quiet instead
        // of looping.
        let mut covered = windowed_state(
            LogicalRect::new(LogicalPosition::zero(), sz(100.0, 100.0)),
            LogicalRect::new(LogicalPosition::zero(), sz(100.0, 100.0)),
        );
        covered.invoked_for_current_expansion = true;
        assert_eq!(
            covered.check_reinvoke_condition(pos(0.0, 50.0), sz(f32::INFINITY, f32::INFINITY)),
            None
        );
    }

    #[test]
    fn check_reinvoke_condition_needs_a_real_scroll_before_any_edge_fires() {
        // The view is re-invoked while the user sits at the BOTTOM of the
        // materialized window, so the resting position is itself an edge: the
        // bottom distance is 0 from the very first check. Only movement away
        // from that resting position may fire.
        let mut s = invoked_state(sz(100.0, 1000.0));
        s.initial_scroll_offset = vpos(900.0);
        let container = sz(100.0, 100.0);

        // Parked exactly where it started (on the window's bottom edge, with
        // 1000 px of document still below it): not a scroll-to-edge.
        assert_eq!(s.check_reinvoke_condition(vpos(900.0), container), None);

        // One pixel of real movement, still within the bottom threshold → fires.
        assert_eq!(
            s.check_reinvoke_condition(vpos(899.0), container),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom))
        );

        // Already invoked for this edge event → quiet regardless of movement.
        s.invoked_for_current_edge = true;
        assert_eq!(s.check_reinvoke_condition(vpos(899.0), container), None);
    }

    #[test]
    fn a_window_scrolled_in_the_middle_of_a_document_rematerializes_at_its_edges() {
        // THE REGRESSION THIS RULE EXISTS FOR. The old rule compared a
        // VIRTUAL-space scroll offset against the MATERIALIZED window's SIZE —
        // two different coordinate spaces — so an edge could only ever fire at
        // the absolute top or bottom of the DOCUMENT, and a view parked in the
        // middle of one never asked for more content: a VirtualView could not
        // scroll. The edges that matter belong to the materialized WINDOW, and
        // they sit in the middle of the document, which is exactly what this
        // pins.
        let s = invoked_state(sz(100.0, 1000.0)); // window y 1000..2000 of a 0..3000 doc
        let container = sz(100.0, 100.0);

        // Dead centre of the window — 450 px from either of its edges, and also
        // the middle of the document: nothing to do.
        assert_eq!(s.check_reinvoke_condition(vpos(450.0), container), None);

        // On the window's BOTTOM edge while still 1100 px short of the
        // document's end. Under the old rule this was "not at the bottom" and
        // nothing loaded; the view could never grow downwards.
        assert_eq!(
            s.check_reinvoke_condition(vpos(900.0), container),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom))
        );

        // Symmetrically on the window's TOP edge, 1000 px INTO the document
        // rather than at its start — the case a paginated document hits every
        // time the user scrolls back up.
        assert_eq!(
            s.check_reinvoke_condition(vpos(0.0), container),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Top))
        );
    }

    #[test]
    fn no_edge_fires_when_the_materialized_window_already_covers_that_side() {
        // The other half of the rule: proximity alone is not enough, the
        // document has to actually extend past that edge. Without this the
        // ends of every document would re-materialize once per frame forever.
        let container = sz(100.0, 100.0);

        // Window flush against the document's TOP: its top edge has nothing
        // behind it, so resting on it is silence — while the BOTTOM edge of the
        // very same window, which does have document behind it, still fires.
        // (Same fixture, same offsets: the only difference is which side has
        // content left.)
        let head = windowed_state(
            LogicalRect::new(LogicalPosition::zero(), sz(100.0, 1000.0)),
            LogicalRect::new(LogicalPosition::zero(), sz(100.0, 3000.0)),
        );
        assert_eq!(
            head.check_reinvoke_condition(pos(0.0, 1.0), container),
            None
        );
        assert_eq!(
            head.check_reinvoke_condition(pos(0.0, 900.0), container),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom))
        );

        // Fully materialized (window == document): nothing is left to load on
        // ANY side, so no offset can produce an edge — including the corners,
        // where all four distances are 0 and every edge "looks" reachable.
        let whole = windowed_state(
            LogicalRect::new(LogicalPosition::zero(), sz(1000.0, 1000.0)),
            LogicalRect::new(LogicalPosition::zero(), sz(1000.0, 1000.0)),
        );
        for offset in [
            pos(0.0, 1.0),
            pos(1.0, 0.0),
            pos(900.0, 900.0),
            pos(500.0, 500.0),
        ] {
            assert_eq!(
                whole.check_reinvoke_condition(offset, container),
                None,
                "fully materialized: nothing to load at {offset:?}"
            );
        }
    }

    // ------------------------------------------------------- getters / predicates

    #[test]
    fn debug_counts_matches_the_number_of_tracked_views() {
        let mut m = VirtualViewManager::new();
        assert_eq!(m.debug_counts(), 0);

        for i in 0..10_usize {
            m.get_or_create_nested_dom_id(DOM, n(i));
            assert_eq!(m.debug_counts(), i + 1);
        }
        // Re-registering the same keys must not grow the map.
        for i in 0..10_usize {
            m.get_or_create_nested_dom_id(DOM, n(i));
        }
        assert_eq!(m.debug_counts(), 10);
        assert_eq!(m.debug_counts(), m.all_view_keys().len());
        assert_eq!(m.debug_counts(), m.get_all_virtual_view_infos().len());
    }

    #[test]
    fn was_virtual_view_invoked_is_false_until_marked() {
        let mut m = VirtualViewManager::new();
        assert!(!m.was_virtual_view_invoked(DOM, n(1)));

        m.get_or_create_nested_dom_id(DOM, n(1));
        assert!(
            !m.was_virtual_view_invoked(DOM, n(1)),
            "registration alone is not an invocation"
        );

        mark(&mut m, DOM, n(1), VirtualViewCallbackReason::InitialRender);
        assert!(m.was_virtual_view_invoked(DOM, n(1)));
        // A sibling node must not inherit the flag.
        assert!(!m.was_virtual_view_invoked(DOM, n(2)));
        assert!(!m.was_virtual_view_invoked(DOM1, n(1)));

        assert_eq!(m.force_reinvoke(DOM, n(1)), Some(()));
        assert!(!m.was_virtual_view_invoked(DOM, n(1)));
    }

    #[test]
    fn get_all_virtual_view_infos_reports_every_field() {
        let m = VirtualViewManager::new();
        assert!(m.get_all_virtual_view_infos().is_empty());

        let mut m = VirtualViewManager::new();
        let nested = m.get_or_create_nested_dom_id(DOM1, n(5));

        // Before any callback: sizes are None, not 0.0.
        let info = m.get_all_virtual_view_infos();
        assert_eq!(info.len(), 1);
        assert_eq!(info[0].parent_dom_id, 1);
        assert_eq!(info[0].parent_node_id, 5);
        assert_eq!(info[0].nested_dom_id, nested.inner);
        assert!(info[0].scroll_size_width.is_none());
        assert!(info[0].scroll_size_height.is_none());
        assert!(info[0].virtual_scroll_size_width.is_none());
        assert!(info[0].virtual_scroll_size_height.is_none());
        assert!(!info[0].was_invoked);
        assert_eq!(info[0].last_bounds_x, 0.0);
        assert_eq!(info[0].last_bounds_y, 0.0);
        assert_eq!(info[0].last_bounds_width, 0.0);
        assert_eq!(info[0].last_bounds_height, 0.0);

        set_sizes(&mut m, DOM1, n(5), sz(3.0, 4.0), sz(5.0, 6.0));
        mark(&mut m, DOM1, n(5), VirtualViewCallbackReason::InitialRender);

        let info = m.get_all_virtual_view_infos();
        assert_eq!(info[0].scroll_size_width, Some(3.0));
        assert_eq!(info[0].scroll_size_height, Some(4.0));
        assert_eq!(info[0].virtual_scroll_size_width, Some(5.0));
        assert_eq!(info[0].virtual_scroll_size_height, Some(6.0));
        assert!(info[0].was_invoked);

        // Infos are emitted in the same (sorted) order as all_view_keys.
        m.get_or_create_nested_dom_id(DOM, n(9));
        let keys = m.all_view_keys();
        let infos = m.get_all_virtual_view_infos();
        assert_eq!(keys.len(), infos.len());
        for (k, i) in keys.iter().zip(infos.iter()) {
            assert_eq!(k.0.inner, i.parent_dom_id);
            assert_eq!(k.1.index(), i.parent_node_id);
        }
    }

    #[test]
    fn edge_flags_any_is_the_or_of_all_four_edges() {
        assert!(!EdgeFlags::default().any());

        let mut all = EdgeFlags::default();
        for edge in [
            EdgeType::Top,
            EdgeType::Bottom,
            EdgeType::Left,
            EdgeType::Right,
        ] {
            let f = EdgeFlags::from(edge);
            assert!(f.any(), "{edge:?} alone must satisfy any()");
            all.top |= f.top;
            all.bottom |= f.bottom;
            all.left |= f.left;
            all.right |= f.right;
        }

        assert_eq!(
            all,
            EdgeFlags {
                top: true,
                bottom: true,
                left: true,
                right: true,
            }
        );
        assert!(all.any());
    }

    #[test]
    fn edge_flags_from_edge_type_sets_exactly_that_edge() {
        assert_eq!(
            EdgeFlags::from(EdgeType::Top),
            EdgeFlags {
                top: true,
                ..Default::default()
            }
        );
        assert_eq!(
            EdgeFlags::from(EdgeType::Bottom),
            EdgeFlags {
                bottom: true,
                ..Default::default()
            }
        );
        assert_eq!(
            EdgeFlags::from(EdgeType::Left),
            EdgeFlags {
                left: true,
                ..Default::default()
            }
        );
        assert_eq!(
            EdgeFlags::from(EdgeType::Right),
            EdgeFlags {
                right: true,
                ..Default::default()
            }
        );
    }
}
