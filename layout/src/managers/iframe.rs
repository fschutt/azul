//! IFrame lifecycle management for layout
//!
//! This module provides:
//! - IFrame re-invocation logic for lazy loading
//! - WebRender PipelineId tracking
//! - Nested DOM ID management

use alloc::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};

use azul_core::{
    callbacks::{EdgeType, IFrameCallbackReason},
    dom::{DomId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    hit_test::PipelineId,
};

use crate::managers::scroll_state::ScrollManager;

static NEXT_PIPELINE_ID: AtomicUsize = AtomicUsize::new(1);

/// Distance in pixels from edge that triggers edge-scrolled callback
const EDGE_THRESHOLD: f32 = 200.0;

/// Manages IFrame lifecycle, including re-invocation and PipelineId generation
///
/// Tracks which IFrames have been invoked, assigns unique DOM IDs to nested
/// IFrames, and determines when IFrames need to be re-invoked (e.g., when
/// the container bounds expand or the user scrolls near an edge).
#[derive(Debug, Clone, Default)]
pub struct IFrameManager {
    /// Per-IFrame state keyed by (parent DomId, NodeId of iframe element)
    states: BTreeMap<(DomId, NodeId), IFrameState>,
    /// WebRender PipelineId for each IFrame
    pipeline_ids: BTreeMap<(DomId, NodeId), PipelineId>,
    /// Counter for generating unique nested DOM IDs
    next_dom_id: usize,
}

/// Internal state for a single IFrame instance
///
/// Tracks invocation status, content dimensions, and edge triggers
/// to determine when the IFrame callback needs to be re-invoked.
#[derive(Debug, Clone)]
struct IFrameState {
    /// Content size reported by IFrame callback (actual rendered size)
    iframe_scroll_size: Option<LogicalSize>,
    /// Virtual scroll size for infinite scroll scenarios
    iframe_virtual_scroll_size: Option<LogicalSize>,
    /// Whether the IFrame has ever been invoked
    iframe_was_invoked: bool,
    /// Whether invoked for current container expansion
    invoked_for_current_expansion: bool,
    /// Whether invoked for current edge scroll event
    invoked_for_current_edge: bool,
    /// Which edges have already triggered callbacks
    last_edge_triggered: EdgeFlags,
    /// Unique DOM ID assigned to this IFrame's content
    nested_dom_id: DomId,
    /// Last known layout bounds of the IFrame container
    last_bounds: LogicalRect,
}

/// Flags indicating which scroll edges have been triggered
///
/// Used to prevent repeated edge-scroll callbacks for the same edge
/// until the user scrolls away and back.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EdgeFlags {
    /// Near top edge
    pub top: bool,
    /// Near bottom edge
    pub bottom: bool,
    /// Near left edge
    pub left: bool,
    /// Near right edge
    pub right: bool,
}

impl IFrameManager {
    /// Creates a new IFrameManager with no tracked IFrames
    pub fn new() -> Self {
        Self {
            next_dom_id: 1, // 0 is root
            ..Default::default()
        }
    }

    /// Called at the start of each frame (currently a no-op)
    pub fn begin_frame(&mut self) {
        // Nothing to do here for now, but good practice for stateful managers
    }

    /// Gets or creates a unique nested DOM ID for an IFrame
    ///
    /// Returns the existing DOM ID if the IFrame was previously registered,
    /// otherwise allocates a new unique ID and initializes the IFrame state.
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

        self.states.insert(key, IFrameState::new(nested_dom_id));
        nested_dom_id
    }

    /// Gets the nested DOM ID for an IFrame if it exists
    pub fn get_nested_dom_id(&self, dom_id: DomId, node_id: NodeId) -> Option<DomId> {
        self.states.get(&(dom_id, node_id)).map(|s| s.nested_dom_id)
    }

    /// Gets or creates a WebRender PipelineId for an IFrame
    ///
    /// PipelineIds are used by WebRender to identify distinct rendering contexts.
    pub fn get_or_create_pipeline_id(&mut self, dom_id: DomId, node_id: NodeId) -> PipelineId {
        *self
            .pipeline_ids
            .entry((dom_id, node_id))
            .or_insert_with(|| PipelineId(dom_id.inner as u32, node_id.index() as u32))
    }

    /// Returns whether the IFrame has ever been invoked
    pub fn was_iframe_invoked(&self, dom_id: DomId, node_id: NodeId) -> bool {
        self.states
            .get(&(dom_id, node_id))
            .map(|s| s.iframe_was_invoked)
            .unwrap_or(false)
    }

    /// Returns the virtual scroll size for an IFrame (if set by the callback)
    pub fn get_virtual_scroll_size(&self, dom_id: DomId, node_id: NodeId) -> Option<LogicalSize> {
        self.states
            .get(&(dom_id, node_id))
            .and_then(|s| s.iframe_virtual_scroll_size)
    }

    /// Returns the scroll size for an IFrame (actual content size, if set by the callback)
    pub fn get_scroll_size(&self, dom_id: DomId, node_id: NodeId) -> Option<LogicalSize> {
        self.states
            .get(&(dom_id, node_id))
            .and_then(|s| s.iframe_scroll_size)
    }

    /// Updates the IFrame's content size information
    ///
    /// Called after the IFrame callback returns to record the actual content
    /// dimensions. If the new size is larger than previously recorded, clears
    /// the expansion flag to allow BoundsExpanded re-invocation.
    pub fn update_iframe_info(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        scroll_size: LogicalSize,
        virtual_scroll_size: LogicalSize,
    ) -> Option<()> {
        let state = self.states.get_mut(&(dom_id, node_id))?;

        // Reset expansion flag if content grew
        if let Some(old_size) = state.iframe_scroll_size {
            if scroll_size.width > old_size.width || scroll_size.height > old_size.height {
                state.invoked_for_current_expansion = false;
            }
        }
        state.iframe_scroll_size = Some(scroll_size);
        state.iframe_virtual_scroll_size = Some(virtual_scroll_size);

        Some(())
    }

    /// Marks an IFrame as invoked for a specific reason
    ///
    /// Updates internal state flags based on the callback reason to prevent
    /// duplicate callbacks for the same trigger condition.
    pub fn mark_invoked(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        reason: IFrameCallbackReason,
    ) -> Option<()> {
        let state = self.states.get_mut(&(dom_id, node_id))?;

        state.iframe_was_invoked = true;
        match reason {
            IFrameCallbackReason::BoundsExpanded => state.invoked_for_current_expansion = true,
            IFrameCallbackReason::EdgeScrolled(edge) => {
                state.invoked_for_current_edge = true;
                state.last_edge_triggered = edge.into();
            }
            _ => {}
        }

        Some(())
    }

    /// Reset invocation flags for ALL tracked IFrames
    ///
    /// After `layout_results.clear()`, the child DOMs no longer exist in memory.
    /// This method ensures `check_reinvoke()` returns `InitialRender` for every
    /// IFrame, so the callbacks re-run and re-populate `layout_results`.
    ///
    /// Called from `layout_and_generate_display_list()` after clearing layout results.
    pub fn reset_all_invocation_flags(&mut self) {
        for state in self.states.values_mut() {
            state.iframe_was_invoked = false;
            state.invoked_for_current_expansion = false;
            state.invoked_for_current_edge = false;
            state.last_edge_triggered = EdgeFlags::default();
        }
    }

    /// Force an IFrame to be re-invoked on the next layout pass
    ///
    /// Clears all invocation flags, causing check_reinvoke() to return InitialRender.
    /// Used by trigger_iframe_rerender() to manually refresh IFrame content.
    pub fn force_reinvoke(&mut self, dom_id: DomId, node_id: NodeId) -> Option<()> {
        let state = self.states.get_mut(&(dom_id, node_id))?;

        state.iframe_was_invoked = false;
        state.invoked_for_current_expansion = false;
        state.invoked_for_current_edge = false;

        Some(())
    }

    /// Checks whether an IFrame needs to be re-invoked and returns the reason
    ///
    /// Returns `Some(reason)` if the IFrame callback should be invoked:
    /// - `InitialRender`: IFrame has never been invoked
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
    ) -> Option<IFrameCallbackReason> {
        let state = self.states.entry((dom_id, node_id)).or_insert_with(|| {
            let nested_dom_id = DomId {
                inner: self.next_dom_id,
            };
            self.next_dom_id += 1;
            IFrameState::new(nested_dom_id)
        });

        if !state.iframe_was_invoked {
            return Some(IFrameCallbackReason::InitialRender);
        }

        // Check for bounds expansion
        if layout_bounds.size.width > state.last_bounds.size.width
            || layout_bounds.size.height > state.last_bounds.size.height
        {
            state.invoked_for_current_expansion = false;
        }
        state.last_bounds = layout_bounds;

        let scroll_offset = scroll_manager
            .get_current_offset(dom_id, node_id)
            .unwrap_or_default();

        state.check_reinvoke_condition(scroll_offset, layout_bounds.size)
    }

    /// Returns debug info for all tracked IFrames
    ///
    /// Each entry contains: (parent_dom_id, parent_node_id, nested_dom_id,
    /// scroll_size, virtual_scroll_size, was_invoked, last_bounds)
    pub fn get_all_iframe_infos(&self) -> alloc::vec::Vec<IFrameDebugInfo> {
        self.states
            .iter()
            .map(|((dom_id, node_id), state)| IFrameDebugInfo {
                parent_dom_id: dom_id.inner,
                parent_node_id: node_id.index(),
                nested_dom_id: state.nested_dom_id.inner,
                scroll_size_width: state.iframe_scroll_size.map(|s| s.width),
                scroll_size_height: state.iframe_scroll_size.map(|s| s.height),
                virtual_scroll_size_width: state.iframe_virtual_scroll_size.map(|s| s.width),
                virtual_scroll_size_height: state.iframe_virtual_scroll_size.map(|s| s.height),
                was_invoked: state.iframe_was_invoked,
                last_bounds_x: state.last_bounds.origin.x,
                last_bounds_y: state.last_bounds.origin.y,
                last_bounds_width: state.last_bounds.size.width,
                last_bounds_height: state.last_bounds.size.height,
            })
            .collect()
    }
}

/// Debug info for a single IFrame, returned by `get_all_iframe_infos`
#[derive(Debug, Clone)]
pub struct IFrameDebugInfo {
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

impl IFrameState {
    /// Creates a new IFrameState with the given nested DOM ID
    fn new(nested_dom_id: DomId) -> Self {
        Self {
            iframe_scroll_size: None,
            iframe_virtual_scroll_size: None,
            iframe_was_invoked: false,
            invoked_for_current_expansion: false,
            invoked_for_current_edge: false,
            last_edge_triggered: EdgeFlags::default(),
            nested_dom_id,
            last_bounds: LogicalRect::zero(),
        }
    }

    /// Determines if the IFrame callback should be re-invoked based on
    // scroll position
    ///
    /// Checks two conditions:
    /// 1. Container bounds expanded beyond content size
    /// 2. User scrolled within EDGE_THRESHOLD pixels of an edge (for lazy loading)
    fn check_reinvoke_condition(
        &mut self,
        current_offset: LogicalPosition,
        container_size: LogicalSize,
    ) -> Option<IFrameCallbackReason> {
        // Need scroll_size to determine if we can scroll at all
        let Some(scroll_size) = self.iframe_scroll_size else {
            return None;
        };

        // Check 1: Container grew larger than content - need more content
        if !self.invoked_for_current_expansion
            && (container_size.width > scroll_size.width
                || container_size.height > scroll_size.height)
        {
            return Some(IFrameCallbackReason::BoundsExpanded);
        }

        // Check 2: Edge-based lazy loading
        // Determine if scrolling is possible in each direction
        let scrollable_width = scroll_size.width > container_size.width;
        let scrollable_height = scroll_size.height > container_size.height;

        // Calculate which edges the user is currently near
        let current_edges = EdgeFlags {
            top: scrollable_height && current_offset.y <= EDGE_THRESHOLD,
            bottom: scrollable_height
                && (scroll_size.height - container_size.height - current_offset.y)
                    <= EDGE_THRESHOLD,
            left: scrollable_width && current_offset.x <= EDGE_THRESHOLD,
            right: scrollable_width
                && (scroll_size.width - container_size.width - current_offset.x) <= EDGE_THRESHOLD,
        };

        // Trigger edge callback if near an edge that hasn't been triggered yet
        // Prioritize bottom/right edges (common infinite scroll directions)
        if !self.invoked_for_current_edge && current_edges.any() {
            if current_edges.bottom && !self.last_edge_triggered.bottom {
                return Some(IFrameCallbackReason::EdgeScrolled(EdgeType::Bottom));
            }
            if current_edges.right && !self.last_edge_triggered.right {
                return Some(IFrameCallbackReason::EdgeScrolled(EdgeType::Right));
            }
        }

        None
    }
}

impl EdgeFlags {
    /// Returns true if any edge flag is set
    fn any(&self) -> bool {
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
