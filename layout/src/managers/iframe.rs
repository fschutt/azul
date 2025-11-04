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

/// Manages IFrame lifecycle, including re-invocation and PipelineId generation
#[derive(Debug, Clone, Default)]
pub struct IFrameManager {
    states: BTreeMap<(DomId, NodeId), IFrameState>,
    pipeline_ids: BTreeMap<(DomId, NodeId), PipelineId>,
    next_dom_id: usize,
}

#[derive(Debug, Clone)]
struct IFrameState {
    iframe_scroll_size: Option<LogicalSize>,
    iframe_virtual_scroll_size: Option<LogicalSize>,
    iframe_was_invoked: bool,
    invoked_for_current_expansion: bool,
    invoked_for_current_edge: bool,
    last_edge_triggered: EdgeFlags,
    nested_dom_id: DomId,
    last_bounds: LogicalRect,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct EdgeFlags {
    pub top: bool,
    pub bottom: bool,
    pub left: bool,
    pub right: bool,
}

impl IFrameManager {
    pub fn new() -> Self {
        Self {
            next_dom_id: 1, // 0 is root
            ..Default::default()
        }
    }

    pub fn begin_frame(&mut self) {
        // Nothing to do here for now, but good practice for stateful managers
    }

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

    pub fn get_nested_dom_id(&self, dom_id: DomId, node_id: NodeId) -> Option<DomId> {
        self.states.get(&(dom_id, node_id)).map(|s| s.nested_dom_id)
    }

    pub fn get_or_create_pipeline_id(&mut self, dom_id: DomId, node_id: NodeId) -> PipelineId {
        *self
            .pipeline_ids
            .entry((dom_id, node_id))
            .or_insert_with(|| PipelineId(dom_id.inner as u32, node_id.index() as u32))
    }

    pub fn was_iframe_invoked(&self, dom_id: DomId, node_id: NodeId) -> bool {
        self.states
            .get(&(dom_id, node_id))
            .map(|s| s.iframe_was_invoked)
            .unwrap_or(false)
    }

    pub fn update_iframe_info(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        scroll_size: LogicalSize,
        virtual_scroll_size: LogicalSize,
    ) {
        let state = self.states.entry((dom_id, node_id)).or_insert_with(|| {
            let nested_dom_id = DomId {
                inner: self.next_dom_id,
            };
            self.next_dom_id += 1;
            IFrameState::new(nested_dom_id)
        });

        if let Some(old_size) = state.iframe_scroll_size {
            if scroll_size.width > old_size.width || scroll_size.height > old_size.height {
                state.invoked_for_current_expansion = false;
            }
        }
        state.iframe_scroll_size = Some(scroll_size);
        state.iframe_virtual_scroll_size = Some(virtual_scroll_size);
    }

    pub fn mark_invoked(&mut self, dom_id: DomId, node_id: NodeId, reason: IFrameCallbackReason) {
        if let Some(state) = self.states.get_mut(&(dom_id, node_id)) {
            state.iframe_was_invoked = true;
            match reason {
                IFrameCallbackReason::BoundsExpanded => state.invoked_for_current_expansion = true,
                IFrameCallbackReason::EdgeScrolled(edge) => {
                    state.invoked_for_current_edge = true;
                    state.last_edge_triggered = edge.into();
                }
                _ => {}
            }
        }
    }

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
}

impl IFrameState {
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

    fn check_reinvoke_condition(
        &mut self,
        current_offset: LogicalPosition,
        container_size: LogicalSize,
    ) -> Option<IFrameCallbackReason> {
        let Some(scroll_size) = self.iframe_scroll_size else {
            return None;
        };

        if !self.invoked_for_current_expansion
            && (container_size.width > scroll_size.width
                || container_size.height > scroll_size.height)
        {
            return Some(IFrameCallbackReason::BoundsExpanded);
        }

        const EDGE_THRESHOLD: f32 = 200.0;
        let scrollable_width = scroll_size.width > container_size.width;
        let scrollable_height = scroll_size.height > container_size.height;

        let current_edges = EdgeFlags {
            top: scrollable_height && current_offset.y <= EDGE_THRESHOLD,
            bottom: scrollable_height
                && (scroll_size.height - container_size.height - current_offset.y)
                    <= EDGE_THRESHOLD,
            left: scrollable_width && current_offset.x <= EDGE_THRESHOLD,
            right: scrollable_width
                && (scroll_size.width - container_size.width - current_offset.x) <= EDGE_THRESHOLD,
        };

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managers::scroll_state::ScrollManager;
    use azul_core::task::{Duration, Instant, SystemTick, SystemTickDiff};
    use azul_core::events::EasingFunction;

    fn test_instant() -> Instant {
        #[cfg(feature = "std")]
        {
            Instant::System(std::time::Instant::now().into())
        }
        #[cfg(not(feature = "std"))]
        {
            Instant::Tick(SystemTick { tick_counter: 0 })
        }
    }

    fn test_duration_zero() -> Duration {
        #[cfg(feature = "std")]
        {
            Duration::System(std::time::Duration::from_secs(0).into())
        }
        #[cfg(not(feature = "std"))]
        {
            Duration::Tick(SystemTickDiff { tick_diff: 0 })
        }
    }

    #[test]
    fn test_iframe_manager_initial_render() {
        let mut iframe_mgr = IFrameManager::new();
        let mut scroll_mgr = ScrollManager::new();
        let now = test_instant();

        let parent_dom = DomId { inner: 0 };
        let node_id = NodeId::new(5);
        let bounds = LogicalRect::new(
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(800.0, 600.0),
        );

        // First check_reinvoke should return InitialRender
        let reason = iframe_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
        assert_eq!(reason, Some(IFrameCallbackReason::InitialRender));

        // Second check without marking invoked should still return InitialRender
        let reason = iframe_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
        assert_eq!(reason, Some(IFrameCallbackReason::InitialRender));

        // Mark as invoked
        iframe_mgr.mark_invoked(parent_dom, node_id, IFrameCallbackReason::InitialRender);

        // Now it should return None (no re-invocation needed)
        let reason = iframe_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
        assert_eq!(reason, None);
    }

    #[test]
    fn test_iframe_manager_bounds_expanded() {
        let mut iframe_mgr = IFrameManager::new();
        let mut scroll_mgr = ScrollManager::new();
        let now = test_instant();

        let parent_dom = DomId { inner: 0 };
        let node_id = NodeId::new(5);

        // Initial render with small bounds
        let small_bounds = LogicalRect::new(
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(400.0, 300.0),
        );
        
        let reason = iframe_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, small_bounds);
        assert_eq!(reason, Some(IFrameCallbackReason::InitialRender));
        
        iframe_mgr.mark_invoked(parent_dom, node_id, IFrameCallbackReason::InitialRender);

        // Update with scroll sizes from the callback
        iframe_mgr.update_iframe_info(
            parent_dom,
            node_id,
            LogicalSize::new(400.0, 300.0),
            LogicalSize::new(400.0, 300.0),
        );

        // Expand bounds (width increases)
        let expanded_bounds = LogicalRect::new(
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(800.0, 300.0),
        );
        
        let reason = iframe_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, expanded_bounds);
        assert_eq!(reason, Some(IFrameCallbackReason::BoundsExpanded));

        // Mark as invoked for expansion
        iframe_mgr.mark_invoked(parent_dom, node_id, IFrameCallbackReason::BoundsExpanded);

        // Same bounds again should return None
        let reason = iframe_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, expanded_bounds);
        assert_eq!(reason, None);

        // Expand height as well
        let more_expanded_bounds = LogicalRect::new(
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(800.0, 600.0),
        );
        
        let reason = iframe_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, more_expanded_bounds);
        assert_eq!(reason, Some(IFrameCallbackReason::BoundsExpanded));
    }

    #[test]
    fn test_iframe_manager_edge_scrolled_bottom() {
        let mut iframe_mgr = IFrameManager::new();
        let mut scroll_mgr = ScrollManager::new();
        let now = test_instant();

        let parent_dom = DomId { inner: 0 };
        let node_id = NodeId::new(5);
        let bounds = LogicalRect::new(
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(800.0, 600.0),
        );

        // Initial render
        let reason = iframe_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
        assert_eq!(reason, Some(IFrameCallbackReason::InitialRender));
        iframe_mgr.mark_invoked(parent_dom, node_id, IFrameCallbackReason::InitialRender);

        // Update with large content size (scrollable)
        iframe_mgr.update_iframe_info(
            parent_dom,
            node_id,
            LogicalSize::new(800.0, 2000.0), // Content is taller than container
            LogicalSize::new(800.0, 2000.0),
        );

        // Initialize scroll state
        scroll_mgr.update_node_bounds(
            parent_dom,
            node_id,
            bounds,
            LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(800.0, 2000.0)),
            now.clone(),
        );

        // No edge yet (scroll at top)
        let reason = iframe_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
        assert_eq!(reason, None);

        // Scroll near bottom edge (within 200px threshold)
        let scroll_offset = LogicalPosition::new(0.0, 1300.0); // 2000 - 600 - 1300 = 100px from bottom
        scroll_mgr.scroll_to(
            parent_dom,
            node_id,
            scroll_offset,
            test_duration_zero(),
            EasingFunction::Linear,
            now.clone(),
        );
        // Tick to apply the scroll immediately (zero duration)
        scroll_mgr.tick(now.clone());

        // Should trigger bottom edge
        let reason = iframe_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
        assert_eq!(
            reason,
            Some(IFrameCallbackReason::EdgeScrolled(EdgeType::Bottom))
        );

        // Mark as invoked for this edge
        iframe_mgr.mark_invoked(
            parent_dom,
            node_id,
            IFrameCallbackReason::EdgeScrolled(EdgeType::Bottom),
        );

        // Same scroll position should not trigger again
        let reason = iframe_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
        assert_eq!(reason, None);
    }

    #[test]
    fn test_iframe_manager_edge_scrolled_right() {
        let mut iframe_mgr = IFrameManager::new();
        let mut scroll_mgr = ScrollManager::new();
        let now = test_instant();

        let parent_dom = DomId { inner: 0 };
        let node_id = NodeId::new(7);
        let bounds = LogicalRect::new(
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(800.0, 600.0),
        );

        // Initial render
        let reason = iframe_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
        assert_eq!(reason, Some(IFrameCallbackReason::InitialRender));
        iframe_mgr.mark_invoked(parent_dom, node_id, IFrameCallbackReason::InitialRender);

        // Update with wide content (scrollable horizontally)
        iframe_mgr.update_iframe_info(
            parent_dom,
            node_id,
            LogicalSize::new(3000.0, 600.0), // Content is wider than container
            LogicalSize::new(3000.0, 600.0),
        );

        // Initialize scroll state
        scroll_mgr.update_node_bounds(
            parent_dom,
            node_id,
            bounds,
            LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(3000.0, 600.0)),
            now.clone(),
        );

        // Scroll near right edge (within 200px threshold)
        let scroll_offset = LogicalPosition::new(2100.0, 0.0); // 3000 - 800 - 2100 = 100px from right
        scroll_mgr.scroll_to(
            parent_dom,
            node_id,
            scroll_offset,
            test_duration_zero(),
            EasingFunction::Linear,
            now.clone(),
        );
        // Tick to apply the scroll immediately (zero duration)
        scroll_mgr.tick(now.clone());

        // Should trigger right edge
        let reason = iframe_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
        assert_eq!(
            reason,
            Some(IFrameCallbackReason::EdgeScrolled(EdgeType::Right))
        );
    }

    #[test]
    fn test_iframe_manager_nested_dom_ids() {
        let mut iframe_mgr = IFrameManager::new();

        let parent_dom = DomId { inner: 0 };
        let node1 = NodeId::new(1);
        let node2 = NodeId::new(2);
        let node3 = NodeId::new(3);

        // Create nested DOM IDs
        let child1 = iframe_mgr.get_or_create_nested_dom_id(parent_dom, node1);
        let child2 = iframe_mgr.get_or_create_nested_dom_id(parent_dom, node2);
        let child3 = iframe_mgr.get_or_create_nested_dom_id(parent_dom, node3);

        // Should be unique
        assert_ne!(child1, child2);
        assert_ne!(child2, child3);
        assert_ne!(child1, child3);

        // Should be consistent (same result when called again)
        assert_eq!(
            child1,
            iframe_mgr.get_or_create_nested_dom_id(parent_dom, node1)
        );
        assert_eq!(
            child2,
            iframe_mgr.get_or_create_nested_dom_id(parent_dom, node2)
        );

        // get_nested_dom_id should return existing IDs
        assert_eq!(iframe_mgr.get_nested_dom_id(parent_dom, node1), Some(child1));
        assert_eq!(iframe_mgr.get_nested_dom_id(parent_dom, node2), Some(child2));
        
        // Non-existent should return None
        let nonexistent = NodeId::new(999);
        assert_eq!(iframe_mgr.get_nested_dom_id(parent_dom, nonexistent), None);
    }

    #[test]
    fn test_iframe_manager_was_invoked_tracking() {
        let mut iframe_mgr = IFrameManager::new();
        let scroll_mgr = ScrollManager::new();

        let parent_dom = DomId { inner: 0 };
        let node_id = NodeId::new(5);
        let bounds = LogicalRect::new(
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(800.0, 600.0),
        );

        // Initially not invoked
        assert!(!iframe_mgr.was_iframe_invoked(parent_dom, node_id));

        // Check reinvoke to create state
        iframe_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
        
        // Still not invoked until we mark it
        assert!(!iframe_mgr.was_iframe_invoked(parent_dom, node_id));

        // Mark as invoked
        iframe_mgr.mark_invoked(parent_dom, node_id, IFrameCallbackReason::InitialRender);

        // Now it should be invoked
        assert!(iframe_mgr.was_iframe_invoked(parent_dom, node_id));
    }
}
