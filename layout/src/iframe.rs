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

use crate::scroll::ScrollManager;

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
        let state = self.states.entry((dom_id, node_id)).or_insert_with(|| {
            IFrameState::new(DomId {
                inner: self.next_dom_id,
            })
        });
        if state.nested_dom_id.inner == 0 {
            // new entry
            self.next_dom_id += 1;
        }
        state.nested_dom_id
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
