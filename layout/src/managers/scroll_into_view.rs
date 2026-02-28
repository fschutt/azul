//! Scroll-into-view implementation
//!
//! Provides W3C CSSOM View Module compliant scroll-into-view functionality.
//! This module contains the core primitive `scroll_rect_into_view` which all
//! higher-level scroll-into-view APIs build upon.
//!
//! # Architecture
//!
//! The core principle is that all scroll-into-view operations reduce to scrolling
//! a rectangle into the visible area of its scroll container ancestry:
//!
//! - `scroll_rect_into_view`: Core primitive - scroll any rect into view
//! - `scroll_node_into_view`: Scroll a DOM node's bounding rect into view
//! - `scroll_cursor_into_view`: Scroll a text cursor position into view
//!
//! # W3C Compliance
//!
//! This implementation follows the W3C CSSOM View Module specification:
//! - ScrollLogicalPosition: start, center, end, nearest
//! - ScrollBehavior: auto, instant, smooth
//! - Proper scroll ancestor chain traversal

use alloc::vec::Vec;

use azul_core::{
    dom::{DomId, DomNodeId, NodeId},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    styled_dom::NodeHierarchyItemId,
    task::{Duration, Instant},
};
use azul_css::props::layout::LayoutOverflow;

use crate::{
    managers::scroll_state::ScrollManager,
    solver3::getters::{get_overflow_x, get_overflow_y, MultiValue},
    window::DomLayoutResult,
};

// Re-export types from core for public API
pub use azul_core::events::{ScrollIntoViewBehavior, ScrollIntoViewOptions, ScrollLogicalPosition};

/// Calculated scroll adjustment for one scroll container
#[derive(Debug, Clone)]
pub struct ScrollAdjustment {
    /// The scroll container that needs adjustment
    pub scroll_container_dom_id: DomId,
    pub scroll_container_node_id: NodeId,
    /// The scroll delta to apply
    pub delta: LogicalPosition,
    /// The scroll behavior to use
    pub behavior: ScrollIntoViewBehavior,
}

/// Information about a scrollable ancestor
#[derive(Debug, Clone)]
struct ScrollableAncestor {
    dom_id: DomId,
    node_id: NodeId,
    /// The visible rect of the scroll container (content area)
    visible_rect: LogicalRect,
    /// Whether horizontal scroll is enabled
    scroll_x: bool,
    /// Whether vertical scroll is enabled
    scroll_y: bool,
}

// ============================================================================
// Core API: scroll_rect_into_view
// ============================================================================

/// Core function: scroll a rect into the visible area of its scroll containers
///
/// This is the ONLY scroll-into-view primitive. All higher-level APIs call this.
///
/// # Arguments
///
/// * `target_rect` - The rectangle to make visible (in absolute coordinates)
/// * `target_dom_id` - The DOM containing the target node
/// * `target_node_id` - The target node (used for finding scroll ancestors)
/// * `layout_results` - Layout data for all DOMs
/// * `scroll_manager` - Current scroll state
/// * `options` - How to scroll (alignment and animation)
/// * `now` - Current timestamp for animation
///
/// # Returns
///
/// A vector of scroll adjustments for each scroll container in the ancestry chain.
/// The adjustments are ordered from innermost (closest to target) to outermost.
pub fn scroll_rect_into_view(
    target_rect: LogicalRect,
    target_dom_id: DomId,
    target_node_id: NodeId,
    layout_results: &alloc::collections::BTreeMap<DomId, DomLayoutResult>,
    scroll_manager: &mut ScrollManager,
    options: ScrollIntoViewOptions,
    now: Instant,
) -> Vec<ScrollAdjustment> {
    let mut adjustments = Vec::new();
    
    // Find scrollable ancestors from target to root
    let scroll_ancestors = find_scrollable_ancestors(
        target_dom_id,
        target_node_id,
        layout_results,
        scroll_manager,
    );
    
    if scroll_ancestors.is_empty() {
        return adjustments;
    }
    
    // Transform target_rect relative to each scroll container and calculate deltas
    let mut current_rect = target_rect;
    
    for ancestor in scroll_ancestors {
        // Calculate the scroll delta based on options
        let delta = calculate_scroll_delta(
            current_rect,
            ancestor.visible_rect,
            options.block,
            options.inline_axis,
            ancestor.scroll_x,
            ancestor.scroll_y,
        );
        
        // Only add adjustment if there's actual scrolling to do
        if delta.x.abs() > 0.5 || delta.y.abs() > 0.5 {
            // Resolve scroll behavior
            let behavior = resolve_scroll_behavior(
                options.behavior,
                ancestor.dom_id,
                ancestor.node_id,
                layout_results,
            );
            
            // Apply the scroll adjustment
            apply_scroll_adjustment(
                scroll_manager,
                ancestor.dom_id,
                ancestor.node_id,
                delta,
                behavior,
                now.clone(),
            );
            
            adjustments.push(ScrollAdjustment {
                scroll_container_dom_id: ancestor.dom_id,
                scroll_container_node_id: ancestor.node_id,
                delta,
                behavior,
            });
            
            // Adjust current_rect for next iteration (relative to new scroll position)
            current_rect.origin.x -= delta.x;
            current_rect.origin.y -= delta.y;
        }
    }
    
    adjustments
}

// ============================================================================
// Higher-Level APIs
// ============================================================================

/// Scroll a DOM node's bounding rect into view
///
/// This is a convenience wrapper around `scroll_rect_into_view` that
/// automatically gets the node's bounding rect from layout results.
pub fn scroll_node_into_view(
    node_id: DomNodeId,
    layout_results: &alloc::collections::BTreeMap<DomId, DomLayoutResult>,
    scroll_manager: &mut ScrollManager,
    options: ScrollIntoViewOptions,
    now: Instant,
) -> Vec<ScrollAdjustment> {
    // Get node's bounding rect from layout
    let target_rect = match get_node_rect(node_id, layout_results) {
        Some(rect) => rect,
        None => return Vec::new(),
    };
    
    let internal_node_id = match node_id.node.into_crate_internal() {
        Some(nid) => nid,
        None => return Vec::new(),
    };
    
    // Call the core rect-based API
    scroll_rect_into_view(
        target_rect,
        node_id.dom,
        internal_node_id,
        layout_results,
        scroll_manager,
        options,
        now,
    )
}

/// Scroll a text cursor position into view
///
/// This requires the cursor's visual rect (from text layout) and transforms
/// it to absolute coordinates before scrolling.
///
/// # Arguments
///
/// * `cursor_rect` - The cursor's rect in node-local coordinates
/// * `node_id` - The contenteditable node containing the cursor
/// * `layout_results` - Layout data
/// * `scroll_manager` - Scroll state
/// * `options` - Scroll options
/// * `now` - Current timestamp
pub fn scroll_cursor_into_view(
    cursor_rect: LogicalRect,
    node_id: DomNodeId,
    layout_results: &alloc::collections::BTreeMap<DomId, DomLayoutResult>,
    scroll_manager: &mut ScrollManager,
    options: ScrollIntoViewOptions,
    now: Instant,
) -> Vec<ScrollAdjustment> {
    // Get node's position to transform cursor_rect to absolute coordinates
    let node_rect = match get_node_rect(node_id, layout_results) {
        Some(rect) => rect,
        None => return Vec::new(),
    };
    
    // Transform cursor rect to absolute coordinates
    let absolute_cursor_rect = LogicalRect {
        origin: LogicalPosition {
            x: node_rect.origin.x + cursor_rect.origin.x,
            y: node_rect.origin.y + cursor_rect.origin.y,
        },
        size: cursor_rect.size,
    };
    
    let internal_node_id = match node_id.node.into_crate_internal() {
        Some(nid) => nid,
        None => return Vec::new(),
    };
    
    // Call the core rect-based API
    scroll_rect_into_view(
        absolute_cursor_rect,
        node_id.dom,
        internal_node_id,
        layout_results,
        scroll_manager,
        options,
        now,
    )
}

// ============================================================================
// Helper Functions
// ============================================================================

/// Find all scrollable ancestors from a node to the root
///
/// Returns ancestors ordered from innermost (closest to target) to outermost (root).
fn find_scrollable_ancestors(
    dom_id: DomId,
    node_id: NodeId,
    layout_results: &alloc::collections::BTreeMap<DomId, DomLayoutResult>,
    scroll_manager: &ScrollManager,
) -> Vec<ScrollableAncestor> {
    let mut ancestors = Vec::new();
    
    let layout_result = match layout_results.get(&dom_id) {
        Some(lr) => lr,
        None => return ancestors,
    };
    
    let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
    let styled_nodes = layout_result.styled_dom.styled_nodes.as_container();
    
    // Walk up the DOM tree from parent of target node
    let mut current = node_hierarchy.get(node_id).and_then(|h| h.parent_id());
    
    while let Some(current_node_id) = current {
        // Check if this node is scrollable
        if let Some(ancestor) = check_if_scrollable(
            dom_id,
            current_node_id,
            layout_result,
            scroll_manager,
        ) {
            ancestors.push(ancestor);
        }
        
        // Move to parent
        current = node_hierarchy.get(current_node_id).and_then(|h| h.parent_id());
    }
    
    ancestors
}

/// Check if a node is scrollable and return its scroll info
fn check_if_scrollable(
    dom_id: DomId,
    node_id: NodeId,
    layout_result: &DomLayoutResult,
    scroll_manager: &ScrollManager,
) -> Option<ScrollableAncestor> {
    let styled_nodes = layout_result.styled_dom.styled_nodes.as_container();
    let styled_node = styled_nodes.get(node_id)?;
    
    let overflow_x = get_overflow_x(
        &layout_result.styled_dom,
        node_id,
        &styled_node.styled_node_state,
    );
    let overflow_y = get_overflow_y(
        &layout_result.styled_dom,
        node_id,
        &styled_node.styled_node_state,
    );
    
    let scroll_x = overflow_x.is_scroll();
    let scroll_y = overflow_y.is_scroll();
    
    // If neither axis is scrollable, skip this node
    if !scroll_x && !scroll_y {
        return None;
    }
    
    // Check if the scroll manager has scroll state for this node
    // (which means it actually has overflowing content)
    let scroll_state = scroll_manager.get_scroll_state(dom_id, node_id)?;
    
    // Check if content actually overflows
    let has_overflow_x = scroll_state.content_rect.size.width > scroll_state.container_rect.size.width;
    let has_overflow_y = scroll_state.content_rect.size.height > scroll_state.container_rect.size.height;
    
    if !has_overflow_x && !has_overflow_y {
        return None;
    }
    
    // Get the visible rect (container rect minus current scroll offset)
    let visible_rect = LogicalRect {
        origin: LogicalPosition {
            x: scroll_state.container_rect.origin.x + scroll_state.current_offset.x,
            y: scroll_state.container_rect.origin.y + scroll_state.current_offset.y,
        },
        size: scroll_state.container_rect.size,
    };
    
    Some(ScrollableAncestor {
        dom_id,
        node_id,
        visible_rect,
        scroll_x: scroll_x && has_overflow_x,
        scroll_y: scroll_y && has_overflow_y,
    })
}

/// Calculate the scroll delta needed to bring target into view within container
fn calculate_scroll_delta(
    target: LogicalRect,
    container: LogicalRect,
    block: ScrollLogicalPosition,
    inline: ScrollLogicalPosition,
    scroll_x_enabled: bool,
    scroll_y_enabled: bool,
) -> LogicalPosition {
    LogicalPosition {
        x: if scroll_x_enabled {
            calculate_axis_delta(
                target.origin.x,
                target.size.width,
                container.origin.x,
                container.size.width,
                inline,
            )
        } else {
            0.0
        },
        y: if scroll_y_enabled {
            calculate_axis_delta(
                target.origin.y,
                target.size.height,
                container.origin.y,
                container.size.height,
                block,
            )
        } else {
            0.0
        },
    }
}

/// Calculate scroll delta for a single axis
pub fn calculate_axis_delta(
    target_start: f32,
    target_size: f32,
    container_start: f32,
    container_size: f32,
    position: ScrollLogicalPosition,
) -> f32 {
    let target_end = target_start + target_size;
    let container_end = container_start + container_size;
    
    match position {
        ScrollLogicalPosition::Start => {
            // Align target start with container start
            target_start - container_start
        }
        ScrollLogicalPosition::End => {
            // Align target end with container end
            target_end - container_end
        }
        ScrollLogicalPosition::Center => {
            // Center target in container
            let target_center = target_start + target_size / 2.0;
            let container_center = container_start + container_size / 2.0;
            target_center - container_center
        }
        ScrollLogicalPosition::Nearest => {
            // Minimum scroll to make target fully visible
            if target_start < container_start {
                // Target is above/left of visible area - scroll up/left
                target_start - container_start
            } else if target_end > container_end {
                // Target is below/right of visible area
                if target_size <= container_size {
                    // Target fits, align end with container end
                    target_end - container_end
                } else {
                    // Target doesn't fit, align start with container start
                    target_start - container_start
                }
            } else {
                // Target is already fully visible
                0.0
            }
        }
    }
}

/// Resolve scroll behavior based on options and CSS properties
fn resolve_scroll_behavior(
    requested: ScrollIntoViewBehavior,
    _dom_id: DomId,
    _node_id: NodeId,
    _layout_results: &alloc::collections::BTreeMap<DomId, DomLayoutResult>,
) -> ScrollIntoViewBehavior {
    match requested {
        ScrollIntoViewBehavior::Auto => {
            // TODO: Check CSS scroll-behavior property on the scroll container
            // For now, default to instant
            ScrollIntoViewBehavior::Instant
        }
        other => other,
    }
}

/// Apply a scroll adjustment to the scroll manager
fn apply_scroll_adjustment(
    scroll_manager: &mut ScrollManager,
    dom_id: DomId,
    node_id: NodeId,
    delta: LogicalPosition,
    behavior: ScrollIntoViewBehavior,
    now: Instant,
) {
    use azul_core::events::EasingFunction;
    use azul_core::task::SystemTimeDiff;
    
    let current = scroll_manager
        .get_current_offset(dom_id, node_id)
        .unwrap_or_default();
    
    let new_position = LogicalPosition {
        x: current.x + delta.x,
        y: current.y + delta.y,
    };
    
    match behavior {
        ScrollIntoViewBehavior::Instant | ScrollIntoViewBehavior::Auto => {
            scroll_manager.set_scroll_position(dom_id, node_id, new_position, now);
        }
        ScrollIntoViewBehavior::Smooth => {
            // Use smooth scroll with 300ms duration
            let duration = Duration::System(SystemTimeDiff::from_millis(300));
            scroll_manager.scroll_to(
                dom_id,
                node_id,
                new_position,
                duration,
                EasingFunction::EaseOut,
                now,
            );
        }
    }
}

/// Get a node's bounding rect from layout results
fn get_node_rect(
    node_id: DomNodeId,
    layout_results: &alloc::collections::BTreeMap<DomId, DomLayoutResult>,
) -> Option<LogicalRect> {
    let layout_result = layout_results.get(&node_id.dom)?;
    let nid = node_id.node.into_crate_internal()?;
    
    // Get position
    let layout_indices = layout_result.layout_tree.dom_to_layout.get(&nid)?;
    let layout_index = *layout_indices.first()?;
    let position = *layout_result.calculated_positions.get(layout_index)?;
    
    // Get size
    let layout_node = layout_result.layout_tree.get(layout_index)?;
    let size = layout_node.used_size?;
    
    Some(LogicalRect::new(position, size))
}
