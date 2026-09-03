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
    /// Whether the callback has been invoked since the container last grew.
    /// Set by EVERY invocation (each one is shown the current bounds),
    /// cleared when [`VirtualViewManager::check_reinvoke`] sees a larger
    /// container than the one recorded — that, and only that, is a
    /// `BoundsExpanded`.
    invoked_for_current_expansion: bool,
    /// The scroll-driven demand (`EdgeScrolled` / `ScrollBeyondContent`) the
    /// CURRENT materialized window has already been asked to answer.
    ///
    /// This is the documented "fires once per edge approach; the flag clears
    /// once the scroll moves away" latch, and it is keyed on the ANSWER, not
    /// on the edge: it is set when the callback is invoked for a scroll
    /// demand, dropped by [`VirtualViewState::check_reinvoke_condition`] as
    /// soon as the geometry no longer demands anything, and dropped by
    /// [`VirtualViewManager::update_virtual_view_info`] when the callback
    /// materializes a different window (a new window is a new question). So a
    /// callback that answers an edge by materializing the same window again
    /// is asked exactly once per approach, and one that materializes too
    /// little is asked again until the edge is out of reach — while the old
    /// per-edge memory (`last_edge_triggered`) was never cleared by scrolling
    /// at all, so every edge fired at most ONCE per full relayout and a
    /// document scrolled past its second page showed bare background.
    served_scroll_demand: Option<VirtualViewCallbackReason>,
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

/// Which edges of the materialized window the visible window is near
/// (within `EDGE_THRESHOLD`) AND has document left to load past.
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
    /// Called after the `VirtualView` callback returns to record the actual
    /// content dimensions. The sizes are the callback's ANSWER, so they never
    /// re-arm `BoundsExpanded` — only a container that grows does that.
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

    /// Every view's last MATERIALIZED size, for the layout solver.
    ///
    /// A `VirtualView` is a replaced element, and `width: auto` on one means
    /// what it means on an `<img>`: as big as the content. The content's size
    /// is exactly what the callback reported, and this is how that report
    /// reaches sizing - it used to reach placement and scrollbar geometry
    /// only, so a view could never be sized by what it returned.
    #[must_use]
    pub fn materialized_sizes(&self) -> BTreeMap<(DomId, NodeId), LogicalSize> {
        self.states
            .iter()
            .filter_map(|(key, state)| state.materialized.map(|m| (*key, m.size)))
            .collect()
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

        let materialized = LogicalRect::new(window_origin, scroll_size);
        // A different window is a different question: whatever scroll demand
        // the previous window had been asked about is void, and the next
        // check re-evaluates the geometry against this one.
        if state.materialized != Some(materialized) {
            state.served_scroll_demand = None;
        }
        state.materialized = Some(materialized);
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
        // Every invocation presents `info.bounds`, so every invocation answers
        // the current container — `BoundsExpanded` is "the window GREW since
        // the callback last saw it" (the documented once-per-expansion), not
        // "the container is bigger than what the callback chose to
        // materialize". The latter re-fired on every check for a view whose
        // window is narrower than its viewport.
        state.invoked_for_current_expansion = true;
        // Latched against the window that is materialized RIGHT NOW (the one
        // the demand was computed from); `update_virtual_view_info` runs after
        // the callback and drops the latch again if the callback answered
        // with a different window.
        if matches!(
            reason,
            VirtualViewCallbackReason::EdgeScrolled(_) | VirtualViewCallbackReason::ScrollBeyondContent
        ) {
            state.served_scroll_demand = Some(reason);
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
            state.served_scroll_demand = None;
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
        state.served_scroll_demand = None;

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
    /// - `BoundsExpanded`: the container grew since the callback last saw it,
    ///   and is now larger than the materialized content
    /// - `ScrollBeyondContent`: the visible window left the materialized one
    ///   entirely (a jump past everything that is rendered)
    /// - `EdgeScrolled`: User scrolled near an edge (for lazy loading)
    ///
    /// Returns `None` if no re-invocation is needed — including when the
    /// current window has already been invoked for exactly the demand that
    /// is standing (see `VirtualViewState::served_scroll_demand`).
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
            // The initial render is shown these bounds too. Without recording
            // them, the first check after it compared the real container
            // against a zero one and fired `BoundsExpanded` on the first
            // scroll tick of every view whose window is narrower than its
            // viewport (a paginated document in a wide canvas, always).
            state.container = layout_bounds;
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
            served_scroll_demand: None,
            nested_dom_id,
            container: LogicalRect::zero(),
            initial_scroll_offset: LogicalPosition::zero(),
        }
    }

    /// Determines if the `VirtualView` callback should be re-invoked based on
    /// scroll position
    ///
    /// Checks, in this order:
    /// 1. Container bounds expanded beyond the materialized content
    ///    (`BoundsExpanded`, once per container growth — armed by
    ///    [`VirtualViewManager::check_reinvoke`], served by any invocation).
    /// 2. What the scroll position demands of the materialized window
    ///    ([`Self::scroll_demand`]: `ScrollBeyondContent` / `EdgeScrolled`),
    ///    minus the demand this window has already been invoked for.
    ///
    /// This is where the "fires once per edge approach" latch is CLEARED: a
    /// scroll position that demands nothing releases `served_scroll_demand`,
    /// so the next approach to the same edge fires again.
    fn check_reinvoke_condition(
        &mut self,
        current_offset: LogicalPosition,
        container_size: LogicalSize,
    ) -> Option<VirtualViewCallbackReason> {
        // Nothing is materialized yet — nothing to be near the edge OF.
        let materialized = self.materialized?;

        // Check 1: Container grew larger than the materialized content — the
        // window no longer fills the viewport, so ask for more.
        if !self.invoked_for_current_expansion
            && (container_size.width > materialized.size.width
                || container_size.height > materialized.size.height)
        {
            return Some(VirtualViewCallbackReason::BoundsExpanded);
        }

        // Check 2: the scroll position against WHAT IS MATERIALIZED.
        let Some(demand) = self.scroll_demand(current_offset, container_size) else {
            // The scroll moved away from every edge: the latch releases, and
            // the next approach is a new event.
            self.served_scroll_demand = None;
            return None;
        };

        // Already asked this window exactly this question, and it answered
        // with this same window (a different answer would have dropped the
        // latch in `update_virtual_view_info`): asking again would spin.
        if self.served_scroll_demand == Some(demand) {
            return None;
        }

        Some(demand)
    }

    /// What the scroll position demands of the materialized window, from the
    /// geometry alone — no memory of previous invocations.
    ///
    /// The visible window is `[current_offset, current_offset + container]`,
    /// in the same virtual space as `materialized` and the document estimate.
    ///
    /// * `ScrollBeyondContent`: the visible window does not overlap the
    ///   materialized one at all (a scrollbar drag or a programmatic jump
    ///   landed on pages that were never materialized) while the document
    ///   does extend there. Everything on screen would be bare background.
    /// * `EdgeScrolled(edge)`: the visible window is within `EDGE_THRESHOLD`
    ///   of an edge of the materialized window that the document extends past
    ///   (so there is something to load — otherwise the ends of every
    ///   document would demand a re-materialization forever). Priority
    ///   bottom / right / top / left, the common infinite-scroll directions
    ///   first; a callback that materializes around the offset clears all of
    ///   them at once, one that only extends the reported edge is asked about
    ///   the next one on the next check.
    ///
    /// Both require the user to have actually moved from the resting position
    /// captured at `InitialRender`: the callback was just invoked for THAT
    /// offset and materialized what it wanted for it, so sitting there is not
    /// a scroll event.
    ///
    /// The old rule compared a VIRTUAL-space offset against the MATERIALIZED
    /// window's size — two different spaces — so it only ever fired at the
    /// absolute top/bottom of the document, and a document scrolled in the
    /// middle never re-materialized at all.
    fn scroll_demand(
        &self,
        current_offset: LogicalPosition,
        container_size: LogicalSize,
    ) -> Option<VirtualViewCallbackReason> {
        let materialized = self.materialized?;
        // The document estimate; falls back to the materialized window when
        // the app reports no virtual extent (a VirtualView used as a plain
        // windowed view: then materialized IS the document).
        let virtual_rect = self.virtual_rect.unwrap_or(materialized);

        // Only treat an edge as "scrolled to" once the user has actually moved
        // from the resting position captured at InitialRender — sitting at the
        // initial top/left edge from the start is not an edge-scroll event.
        if current_offset == self.initial_scroll_offset {
            return None;
        }

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

        // Is there document past each edge of the materialized window?
        let doc_above = mat_min_y > doc_min_y;
        let doc_below = mat_max_y < doc_max_y;
        let doc_left = mat_min_x > doc_min_x;
        let doc_right = mat_max_x < doc_max_x;

        // The visible window lies entirely past an edge of the materialized
        // window, on the side the document continues on. Strict: a visible
        // window flush against the materialized edge still shows nothing of
        // it, and a viewport of zero height can never be "beyond" (`<=`/`>=`
        // on equal bounds would have made it so).
        let beyond = (doc_below && vis_min_y >= mat_max_y && vis_min_y < doc_max_y)
            || (doc_above && vis_max_y <= mat_min_y && vis_max_y > doc_min_y)
            || (doc_right && vis_min_x >= mat_max_x && vis_min_x < doc_max_x)
            || (doc_left && vis_max_x <= mat_min_x && vis_max_x > doc_min_x);
        if beyond {
            return Some(VirtualViewCallbackReason::ScrollBeyondContent);
        }

        let near = EdgeFlags {
            // More document above what we materialized, and the view is near
            // the materialized window's top.
            top: doc_above && (vis_min_y - mat_min_y) <= EDGE_THRESHOLD,
            bottom: doc_below && (mat_max_y - vis_max_y) <= EDGE_THRESHOLD,
            left: doc_left && (vis_min_x - mat_min_x) <= EDGE_THRESHOLD,
            right: doc_right && (mat_max_x - vis_max_x) <= EDGE_THRESHOLD,
        };

        if near.bottom {
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom))
        } else if near.right {
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Right))
        } else if near.top {
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Top))
        } else if near.left {
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Left))
        } else {
            None
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
// make it pass. Where the actual behavior looks wrong (NaN poisoning the
// growth check; `Default` handing out DomId 0), the test pins the current
// behavior and says so in a comment. The one bug this module used to pin
// instead of fixing — a per-edge memory that scrolling never cleared, so every
// edge fired at most once per full relayout — is fixed, and the tests below
// state the law that replaced it: an edge approach fires once, the latch
// releases when the scroll moves away or the window changes, and a repeat
// approach fires again.
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
        assert!(s.served_scroll_demand.is_none());
        assert_eq!(s.container, LogicalRect::zero());
        assert_eq!(s.initial_scroll_offset, LogicalPosition::zero());

        // A brand-new state has no content size, so it can never ask to be
        // re-invoked, however absurd the container.
        let mut s = s;
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
    fn update_virtual_view_info_never_touches_the_expansion_latch() {
        // What the callback materializes is its ANSWER to the bounds it was
        // shown, not a new question: content growth, shrinkage, infinite or
        // NaN sizes all leave `invoked_for_current_expansion` alone. (The old
        // rule cleared it on content growth, so a view that answered an
        // expansion by growing was immediately asked again.)
        let mut m = ready_view(sz(100.0, 100.0));
        mark(&mut m, DOM, n(1), VirtualViewCallbackReason::BoundsExpanded);
        assert!(st(&m, DOM, n(1)).invoked_for_current_expansion);

        for size in [
            sz(50.0, 50.0),
            sz(50.0, 50.0),
            sz(50.000_01, 50.0),
            sz(1.0e9, 1.0e9),
            sz(f32::INFINITY, 100.0),
            sz(f32::NAN, f32::NAN),
            sz(1.0e9, 1.0e9),
        ] {
            set_sizes(&mut m, DOM, n(1), size, size);
            assert!(
                st(&m, DOM, n(1)).invoked_for_current_expansion,
                "size {size:?} must not re-arm BoundsExpanded"
            );
        }

        // Only a container that GREW (check_reinvoke) or a reset re-arms it.
        m.force_reinvoke(DOM, n(1)).expect("view exists");
        assert!(!st(&m, DOM, n(1)).invoked_for_current_expansion);
    }

    // ----------------------------------------------------------- mark_invoked

    #[test]
    fn mark_invoked_serves_the_container_and_latches_only_scroll_demands() {
        // Every invocation is shown `info.bounds`, so every reason answers the
        // current expansion. Only the scroll-driven reasons latch THEMSELVES
        // — the exact demand that was served, so the same demand is
        // suppressed and a different one (another edge, or a jump beyond the
        // content) is not.
        for reason in [
            VirtualViewCallbackReason::InitialRender,
            VirtualViewCallbackReason::DomRecreated,
            VirtualViewCallbackReason::BoundsExpanded,
        ] {
            let mut m = VirtualViewManager::new();
            m.get_or_create_nested_dom_id(DOM, n(1));
            assert_eq!(m.mark_invoked(DOM, n(1), reason), Some(()));

            let s = st(&m, DOM, n(1));
            assert!(s.virtual_view_was_invoked, "{reason:?} must mark invoked");
            assert!(s.invoked_for_current_expansion, "{reason:?}");
            assert!(s.served_scroll_demand.is_none(), "{reason:?}");
            assert!(m.was_virtual_view_invoked(DOM, n(1)));
        }

        for reason in [
            VirtualViewCallbackReason::EdgeScrolled(EdgeType::Top),
            VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom),
            VirtualViewCallbackReason::EdgeScrolled(EdgeType::Left),
            VirtualViewCallbackReason::EdgeScrolled(EdgeType::Right),
            VirtualViewCallbackReason::ScrollBeyondContent,
        ] {
            let mut m = VirtualViewManager::new();
            m.get_or_create_nested_dom_id(DOM, n(1));
            mark(&mut m, DOM, n(1), reason);

            let s = st(&m, DOM, n(1));
            assert!(s.virtual_view_was_invoked);
            assert!(s.invoked_for_current_expansion, "{reason:?}");
            assert_eq!(s.served_scroll_demand, Some(reason), "{reason:?}");
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
            assert!(s.served_scroll_demand.is_none());
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
    fn force_reinvoke_yields_initial_render_and_releases_the_scroll_latch() {
        let mut m = ready_view(sz(100.0, 1000.0));
        mark(
            &mut m,
            DOM,
            n(1),
            VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom),
        );
        assert!(st(&m, DOM, n(1)).served_scroll_demand.is_some());

        assert_eq!(m.force_reinvoke(DOM, n(1)), Some(()));
        let s = st(&m, DOM, n(1));
        assert!(!s.virtual_view_was_invoked);
        assert!(!s.invoked_for_current_expansion);
        // A forced re-invoke starts the view over: it must not carry a memory
        // of an edge the NEXT materialization has never been asked about.
        // (This used to be asymmetric with reset_all_invocation_flags, which
        // is how an infinite-scroll list stopped loading after its first page.)
        assert!(s.served_scroll_demand.is_none());

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
    fn an_initial_render_serves_the_bounds_it_was_shown() {
        // A paginated document: the callback materializes pages 796 px wide
        // inside a 1400 px canvas, so the container is wider than the window
        // for the whole life of the view. That is the callback's layout
        // choice, not an expansion — the first scroll tick must NOT
        // re-materialize everything as `BoundsExpanded`. (It did: the
        // InitialRender branch returned before recording the container, so
        // the first real check saw a growth from a zero container.)
        let canvas = rect(1400.0, 900.0);
        let mut m = VirtualViewManager::new();
        let sm = scrolled(DOM, n(1), 0.0, 0.0);
        assert_eq!(
            m.check_reinvoke(DOM, n(1), &sm, canvas),
            Some(VirtualViewCallbackReason::InitialRender)
        );
        assert_eq!(st(&m, DOM, n(1)).container, canvas);
        mark(&mut m, DOM, n(1), VirtualViewCallbackReason::InitialRender);
        set_sizes(&mut m, DOM, n(1), sz(796.0, 3000.0), sz(796.0, 13_000.0));

        // First wheel tick: same canvas, 20 px in — nothing to ask.
        let sm = scrolled(DOM, n(1), 0.0, 20.0);
        assert_eq!(m.check_reinvoke(DOM, n(1), &sm, canvas), None);

        // The canvas itself growing is the documented BoundsExpanded, once.
        let wider = rect(1600.0, 900.0);
        assert_eq!(
            m.check_reinvoke(DOM, n(1), &sm, wider),
            Some(VirtualViewCallbackReason::BoundsExpanded)
        );
        mark(&mut m, DOM, n(1), VirtualViewCallbackReason::BoundsExpanded);
        assert_eq!(m.check_reinvoke(DOM, n(1), &sm, wider), None);
        // Shrinking back is not an expansion either.
        assert_eq!(m.check_reinvoke(DOM, n(1), &sm, canvas), None);
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

        // Same window, same question, already answered → no duplicate. This
        // is what keeps a callback that CHOOSES not to grow (a list at its
        // real end, say) from being hammered every frame while the user sits
        // at the edge.
        assert_eq!(m.check_reinvoke(DOM, n(1), &sm, rect(100.0, 100.0)), None);
        assert_eq!(m.check_reinvoke(DOM, n(1), &sm, rect(100.0, 100.0)), None);
    }

    #[test]
    fn an_edge_approach_fires_once_and_rearms_when_the_scroll_moves_away() {
        // The documented contract: "Fires once per edge approach. The flag
        // clears once the scroll moves away." Geometry (`ready_view`): the
        // 100x1000 window is materialized at the document's ORIGIN and the
        // document is 2000 px tall, so y=900 puts the viewport's bottom flush
        // against the window's bottom edge with 1000 px still to load below;
        // y=400 is comfortably inside.
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
        // The callback answered with the SAME window (nothing more to show).
        set_sizes(&mut m, DOM, n(1), sz(100.0, 1000.0), sz(100.0, 2000.0));
        assert_eq!(m.check_reinvoke(DOM, n(1), &bottom, bounds), None);

        // Scrolling away releases the latch ...
        assert_eq!(m.check_reinvoke(DOM, n(1), &middle, bounds), None);
        assert!(st(&m, DOM, n(1)).served_scroll_demand.is_none());

        // ... so the next approach is a new approach and fires again.
        assert_eq!(
            m.check_reinvoke(DOM, n(1), &bottom, bounds),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom))
        );
    }

    #[test]
    fn a_second_scroll_to_the_bottom_edge_fires_again_after_any_reinvoke() {
        // Regression law for the bug this module used to PIN: the old per-edge
        // memory (`last_edge_triggered`) was cleared by reset_all but not by
        // force_reinvoke, so after the first lazy-load an infinite-scroll list
        // never loaded another page until a full relayout. Both re-invoke paths
        // now start the view over; the two halves are identical and both fire.
        let bottom = scrolled(DOM, n(1), 0.0, 900.0);
        let middle = scrolled(DOM, n(1), 0.0, 400.0);
        let bounds = rect(100.0, 100.0);

        for reset_via_force in [true, false] {
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

            if reset_via_force {
                m.force_reinvoke(DOM, n(1)).expect("view exists");
            } else {
                m.reset_all_invocation_flags();
            }
            // Re-invoked while the user sits mid-list: the resting position is 400.
            assert_eq!(
                m.check_reinvoke(DOM, n(1), &middle, bounds),
                Some(VirtualViewCallbackReason::InitialRender)
            );
            mark(&mut m, DOM, n(1), VirtualViewCallbackReason::InitialRender);
            assert_eq!(st(&m, DOM, n(1)).initial_scroll_offset, pos(0.0, 400.0));

            // The user really scrolls 400 → 900: a genuine second approach.
            assert_eq!(
                m.check_reinvoke(DOM, n(1), &bottom, bounds),
                Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom)),
                "force={reset_via_force}: the second bottom-edge load must fire"
            );
        }
    }

    #[test]
    fn a_callback_that_materializes_a_new_window_is_re_evaluated_against_it() {
        // The latch remembers the ANSWER (demand + window), not the edge. When
        // the callback answers EdgeScrolled(Bottom) by materializing a window
        // further down, the latch is dropped and the standing offset is judged
        // against the NEW window: not near its edges → quiet; and when the user
        // reaches the new window's bottom edge, that is a fresh approach.
        let bounds = rect(100.0, 100.0);
        let mut m = ready_view(sz(100.0, 1000.0));
        // Document: 3000 px, window 0..1000.
        set_sizes(&mut m, DOM, n(1), sz(100.0, 1000.0), sz(100.0, 3000.0));

        let at_900 = scrolled(DOM, n(1), 0.0, 900.0);
        assert_eq!(
            m.check_reinvoke(DOM, n(1), &at_900, bounds),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom))
        );
        mark(
            &mut m,
            DOM,
            n(1),
            VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom),
        );
        // The callback re-materializes 500..1500 (a page stride down).
        m.update_virtual_view_info(
            DOM,
            n(1),
            pos(0.0, 500.0),
            sz(100.0, 1000.0),
            sz(100.0, 3000.0),
        )
        .expect("view exists");
        assert!(
            st(&m, DOM, n(1)).served_scroll_demand.is_none(),
            "a different window is a different question"
        );

        // Same offset 900: viewport 900..1000 sits 500 px above the new
        // window's bottom (1500) and 400 px below its top (500) → nothing.
        assert_eq!(m.check_reinvoke(DOM, n(1), &at_900, bounds), None);

        // 1350: viewport 1350..1450, 50 px from the new bottom → fires again.
        let at_1350 = scrolled(DOM, n(1), 0.0, 1350.0);
        assert_eq!(
            m.check_reinvoke(DOM, n(1), &at_1350, bounds),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom))
        );
        mark(
            &mut m,
            DOM,
            n(1),
            VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom),
        );

        // A callback that UNDER-DELIVERS (moves the window, but not far enough
        // to take the edge out of reach) is asked again — the demand persists
        // and the window changed, so the latch does not apply.
        m.update_virtual_view_info(
            DOM,
            n(1),
            pos(0.0, 550.0),
            sz(100.0, 1000.0),
            sz(100.0, 3000.0),
        )
        .expect("view exists");
        assert_eq!(
            m.check_reinvoke(DOM, n(1), &at_1350, bounds),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom))
        );
    }

    #[test]
    fn a_smooth_scroll_through_a_paginated_document_fires_once_per_window_advance() {
        // AzWriter's page host, driven the way the wheel physics drives it:
        // 20 px per tick, one check per tick. stride = page height + gap,
        // viewport 847 px, the callback materializes `first-1 .. first+count`
        // pages around the offset (its real math). The law under test is the
        // whole point of the refactor: the callback runs ONCE per window
        // advance — never once per tick, and never twice for the same window.
        const STRIDE: f32 = 1155.0;
        const VIEWPORT: f32 = 847.0;
        const TOTAL_PAGES: f32 = 12.0;
        let bounds = rect(600.0, VIEWPORT);
        let doc = sz(600.0, TOTAL_PAGES * STRIDE);

        // The callback's answer for a given offset (AzWriter's `pages_virtual_view`).
        let materialize = |offset: f32| -> (LogicalPosition, LogicalSize) {
            let first = ((offset / STRIDE).floor() as i64 - 1).max(0) as f32;
            let visible = (VIEWPORT / STRIDE).ceil() + 2.0;
            let count = visible.max(3.0).min(TOTAL_PAGES - first);
            (pos(0.0, first * STRIDE), sz(600.0, count * STRIDE))
        };

        let mut m = VirtualViewManager::new();
        m.get_or_create_nested_dom_id(DOM, n(1));
        mark(&mut m, DOM, n(1), VirtualViewCallbackReason::InitialRender);
        let (o, s) = materialize(0.0);
        m.update_virtual_view_info(DOM, n(1), o, s, doc).expect("view exists");

        let mut invocations = Vec::new();
        let mut offset = 0.0;
        let end = TOTAL_PAGES * STRIDE - VIEWPORT;
        while offset < end {
            offset = (offset + 20.0).min(end);
            let sm = scrolled(DOM, n(1), 0.0, offset);
            if let Some(reason) = m.check_reinvoke(DOM, n(1), &sm, bounds) {
                assert_eq!(
                    reason,
                    VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom),
                    "at offset {offset}"
                );
                mark(&mut m, DOM, n(1), reason);
                let (o, s) = materialize(offset);
                m.update_virtual_view_info(DOM, n(1), o, s, doc).expect("view exists");
                invocations.push((offset, o.y));
            }
        }

        // Every invocation moved the window (no two answers with the same
        // origin back to back), and the count is one per advance: the window
        // starts at page 0 and ends at page TOTAL-3 (the last 3-page window),
        // advancing one page per fire.
        for w in invocations.windows(2) {
            assert!(w[1].1 > w[0].1, "window must advance on every fire: {invocations:?}");
        }
        let last_first = TOTAL_PAGES - 3.0;
        assert_eq!(
            invocations.len(),
            last_first as usize,
            "one re-materialization per page advance, none per tick: {invocations:?}"
        );
        assert_eq!(invocations.last().map(|i| i.1), Some(last_first * STRIDE));

        // Scrolling back up through the whole document is the mirror image.
        let mut ups = 0usize;
        while offset > 0.0 {
            offset = (offset - 20.0).max(0.0);
            let sm = scrolled(DOM, n(1), 0.0, offset);
            if let Some(reason) = m.check_reinvoke(DOM, n(1), &sm, bounds) {
                assert_eq!(
                    reason,
                    VirtualViewCallbackReason::EdgeScrolled(EdgeType::Top),
                    "at offset {offset}"
                );
                mark(&mut m, DOM, n(1), reason);
                let (o, s) = materialize(offset);
                m.update_virtual_view_info(DOM, n(1), o, s, doc).expect("view exists");
                ups += 1;
            }
        }
        assert_eq!(ups, last_first as usize, "one re-materialization per page retreat");
        assert_eq!(st(&m, DOM, n(1)).materialized.map(|r| r.origin.y), Some(0.0));
    }

    #[test]
    fn a_jump_past_everything_materialized_is_scroll_beyond_content() {
        // "A programmatic scroll jumped the offset past the rendered
        // scroll_size" — the documented reason that had no producer. Window
        // 0..1000 of a 3000-px document; a jump to 2500 shows nothing that is
        // materialized, so the callback is asked to re-materialize there.
        let bounds = rect(100.0, 100.0);
        let mut m = ready_view(sz(100.0, 1000.0));
        set_sizes(&mut m, DOM, n(1), sz(100.0, 1000.0), sz(100.0, 3000.0));

        let far = scrolled(DOM, n(1), 0.0, 2500.0);
        assert_eq!(
            m.check_reinvoke(DOM, n(1), &far, bounds),
            Some(VirtualViewCallbackReason::ScrollBeyondContent)
        );
        mark(&mut m, DOM, n(1), VirtualViewCallbackReason::ScrollBeyondContent);

        // Until the callback answers, the same jump is not asked twice.
        assert_eq!(m.check_reinvoke(DOM, n(1), &far, bounds), None);

        // The callback materializes 2000..3000 around the new offset: viewport
        // 2500..2600 is 500 px from either edge → quiet.
        m.update_virtual_view_info(
            DOM,
            n(1),
            pos(0.0, 2000.0),
            sz(100.0, 1000.0),
            sz(100.0, 3000.0),
        )
        .expect("view exists");
        assert_eq!(m.check_reinvoke(DOM, n(1), &far, bounds), None);

        // And a jump back to the top is the same demand in the other direction.
        let top = scrolled(DOM, n(1), 0.0, 100.0);
        assert_eq!(
            m.check_reinvoke(DOM, n(1), &top, bounds),
            Some(VirtualViewCallbackReason::ScrollBeyondContent)
        );
    }

    #[test]
    fn overscroll_past_the_document_end_is_not_scroll_beyond_content() {
        // Rubber-banding past the document's real end shows no unmaterialized
        // content, so it is not a jump beyond the content — and the window is
        // flush with the document end, so it is not an edge approach either.
        let bounds = rect(100.0, 100.0);
        let mut m = ready_view(sz(100.0, 1000.0));
        // Window 2000..3000 == the document's tail.
        m.update_virtual_view_info(
            DOM,
            n(1),
            pos(0.0, 2000.0),
            sz(100.0, 1000.0),
            sz(100.0, 3000.0),
        )
        .expect("view exists");

        let overscrolled = scrolled(DOM, n(1), 0.0, 3200.0);
        assert_eq!(m.check_reinvoke(DOM, n(1), &overscrolled, bounds), None);
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
        let mut s = invoked_state(sz(100.0, 1000.0)); // window y 1000..2000 of a 0..3000 doc
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
        let mut s2 = invoked_state_2d(sz(1000.0, 1000.0)); // window 1000..2000 of a 0..3000 doc, both axes
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
    fn edge_priority_is_bottom_right_top_left_and_one_answer_per_window() {
        // Several edges have to be near AT ONCE for priority to mean anything,
        // which takes a small viewport inside a window that has document past
        // it on all four sides — hence the both-axes fixture. A 900 px viewport
        // sitting on the top-left corner of the 1000x1000 window is 0 px from
        // its top and left edges and 100 px from its bottom and right ones, so
        // all four are inside EDGE_THRESHOLD simultaneously.
        let mut s = invoked_state_2d(sz(1000.0, 1000.0));
        let container = sz(900.0, 900.0);
        let offset = pos(FIXTURE_WINDOW_ORIGIN_X, FIXTURE_WINDOW_ORIGIN_Y);
        let doc = s.virtual_rect.expect("fixture sets the document");

        assert_eq!(
            s.check_reinvoke_condition(offset, container),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom))
        );

        // The callback answered Bottom and kept the SAME window: it has said
        // what it wants to show for this position, and asking it about the
        // other three edges could only get the same answer — so nothing more
        // fires. (The old per-edge memory "drained" Right, Top, Left here: three
        // full re-materializations for no new content.)
        s.served_scroll_demand = Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom));
        assert_eq!(s.check_reinvoke_condition(offset, container), None);

        // Priority becomes observable once the callback answers each edge with
        // a window that takes THAT edge out of reach: the rect change releases
        // the latch, and the next edge in priority order is reported.
        let mut answer = |s: &mut VirtualViewState, mat: LogicalRect| {
            // What `update_virtual_view_info` does for a changed window.
            s.materialized = Some(mat);
            s.served_scroll_demand = None;
        };

        // Grow down: bottom is 600 px away → Right is next.
        answer(&mut s, LogicalRect::new(pos(1000.0, 1000.0), sz(1000.0, 1500.0)));
        assert_eq!(
            s.check_reinvoke_condition(offset, container),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Right))
        );
        s.served_scroll_demand = Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Right));
        assert_eq!(s.check_reinvoke_condition(offset, container), None);

        // Grow right → Top.
        answer(&mut s, LogicalRect::new(pos(1000.0, 1000.0), sz(1500.0, 1500.0)));
        assert_eq!(
            s.check_reinvoke_condition(offset, container),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Top))
        );

        // Grow up → Left.
        answer(&mut s, LogicalRect::new(pos(1000.0, 500.0), sz(1500.0, 2000.0)));
        assert_eq!(
            s.check_reinvoke_condition(offset, container),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Left))
        );

        // Grow left: every edge is out of reach → quiet, and the document is
        // the same one throughout.
        answer(&mut s, LogicalRect::new(pos(500.0, 500.0), sz(2000.0, 2000.0)));
        assert_eq!(s.check_reinvoke_condition(offset, container), None);
        assert_eq!(s.virtual_rect, Some(doc));
    }

    #[test]
    fn check_reinvoke_condition_at_zero_is_quiet() {
        // Zero content, zero container, zero offset: `0 > 0` is false so there
        // is no expansion, and the offset is exactly the resting
        // `initial_scroll_offset`, so no edge may fire either.
        let mut s = invoked_state(sz(0.0, 0.0));
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
        let mut s = invoked_state(sz(nan, nan));
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
        let mut s = invoked_state(sz(100.0, 1000.0));
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
        let mut s = invoked_state(sz(100.0, 1000.0));
        let container = sz(100.0, 100.0);

        // Rubber-band overscroll far above the materialized top: the top edge
        // fires, because this window has 1000 px of document above it. (The
        // bottom is ~1e9 px away, and the fixture is not windowed on x.) It is
        // NOT a jump beyond the content: the viewport lies entirely above the
        // document's own top, where there is nothing to materialize.
        assert_eq!(
            s.check_reinvoke_condition(vpos(-1.0e9), container),
            Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Top))
        );
        // Whereas a viewport parked at y -1..99 shows document (0..99) that
        // the 1000..2000 window does not cover at all — nothing materialized is
        // on screen, which is the jump-beyond-content case, not an approach.
        assert_eq!(
            s.check_reinvoke_condition(pos(-50.0, -1.0), container),
            Some(VirtualViewCallbackReason::ScrollBeyondContent)
        );

        // JUDGEMENT: a negative offset is not a top-edge event by ITSELF — the
        // sign of the offset is irrelevant, what matters is whether there is
        // document above the materialized window. With the window flush against
        // the document's top there is nothing left to load up there, so the
        // very same rubber-band overscroll is silence, not a reload loop
        // (which is precisely what a rubber-band bounce would otherwise cause,
        // once per frame, for the whole duration of the bounce).
        let mut at_doc_top = windowed_state(
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
        // detected when it is not. Totality is the guarantee; the particular
        // answer is an artefact of the garbage, recorded so a future change to
        // it is noticed rather than silently absorbed.
        //
        // What actually happens with the inverted rects: the visible window
        // (1000 down to 800) starts at or past the inverted materialized
        // "bottom" (y=900) and short of the inverted document's (y=1900), which
        // reads as "scrolled beyond the materialized content".
        let mut s = invoked_state(sz(-100.0, -100.0));
        assert_eq!(
            s.check_reinvoke_condition(vpos(0.0), sz(-200.0, -200.0)),
            Some(VirtualViewCallbackReason::ScrollBeyondContent)
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
        let mut s = invoked_state(sz(f32::MAX, f32::MAX));
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
        let mut huge = windowed_state(
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
        let mut s = invoked_state(sz(100.0, 100.0));
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

        // Already answered for this window → quiet while the same demand stands.
        s.served_scroll_demand = Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom));
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
        let mut s = invoked_state(sz(100.0, 1000.0)); // window y 1000..2000 of a 0..3000 doc
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
        let mut head = windowed_state(
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
        let mut whole = windowed_state(
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
    fn a_view_reports_the_size_it_materialized() {
        let mut m = VirtualViewManager::new();
        let (dom, node) = (DomId::ROOT_ID, NodeId::new(3));
        m.get_or_create_nested_dom_id(dom, node);

        assert!(
            m.materialized_sizes().is_empty(),
            "before the callback has run there is nothing to size from - a \
             view is sized from the OUTSIDE first, which is the only order \
             that terminates"
        );

        m.update_virtual_view_info(
            dom,
            node,
            LogicalPosition::zero(),
            LogicalSize::new(16.0, 16.0),
            LogicalSize::new(16.0, 16.0),
        );
        assert_eq!(
            m.materialized_sizes().get(&(dom, node)).copied(),
            Some(LogicalSize::new(16.0, 16.0)),
            "afterwards the view is as big as what it returned"
        );
    }

    /// Only views that ACTUALLY materialized report a size: a registered but
    /// never-invoked view must not claim 0x0, which would collapse its box.
    #[test]
    fn a_registered_but_unrendered_view_reports_nothing() {
        let mut m = VirtualViewManager::new();
        m.get_or_create_nested_dom_id(DomId::ROOT_ID, NodeId::new(1));
        m.get_or_create_nested_dom_id(DomId::ROOT_ID, NodeId::new(2));
        m.update_virtual_view_info(
            DomId::ROOT_ID,
            NodeId::new(2),
            LogicalPosition::zero(),
            LogicalSize::new(8.0, 8.0),
            LogicalSize::new(8.0, 8.0),
        );

        let sizes = m.materialized_sizes();
        assert_eq!(sizes.len(), 1, "only the one that rendered: {sizes:?}");
        assert!(sizes.contains_key(&(DomId::ROOT_ID, NodeId::new(2))));
    }
}
