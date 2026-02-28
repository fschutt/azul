//! Tests for VirtualizedView lifecycle management

use azul_core::{
    callbacks::{EdgeType, VirtualizedViewCallbackReason},
    dom::{DomId, NodeId},
    events::EasingFunction,
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    task::{Duration, Instant, SystemTick, SystemTickDiff},
};
use azul_layout::managers::{virtualized_view::VirtualizedViewManager, scroll_state::ScrollManager};

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
fn test_virtualized_view_manager_initial_render() {
    let mut virtualized_view_mgr = VirtualizedViewManager::new();
    let scroll_mgr = ScrollManager::new();
    let _now = test_instant();

    let parent_dom = DomId { inner: 0 };
    let node_id = NodeId::new(5);
    let bounds = LogicalRect::new(
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(800.0, 600.0),
    );

    // First check_reinvoke should return InitialRender
    let reason = virtualized_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
    assert_eq!(reason, Some(VirtualizedViewCallbackReason::InitialRender));

    // Second check without marking invoked should still return InitialRender
    let reason = virtualized_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
    assert_eq!(reason, Some(VirtualizedViewCallbackReason::InitialRender));

    // Mark as invoked
    virtualized_view_mgr.mark_invoked(parent_dom, node_id, VirtualizedViewCallbackReason::InitialRender);

    // Now it should return None (no re-invocation needed)
    let reason = virtualized_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
    assert_eq!(reason, None);
}

#[test]
fn test_virtualized_view_manager_bounds_expanded() {
    let mut virtualized_view_mgr = VirtualizedViewManager::new();
    let scroll_mgr = ScrollManager::new();
    let _now = test_instant();

    let parent_dom = DomId { inner: 0 };
    let node_id = NodeId::new(5);

    // Initial render with small bounds
    let small_bounds = LogicalRect::new(
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(400.0, 300.0),
    );

    let reason = virtualized_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, small_bounds);
    assert_eq!(reason, Some(VirtualizedViewCallbackReason::InitialRender));

    virtualized_view_mgr.mark_invoked(parent_dom, node_id, VirtualizedViewCallbackReason::InitialRender);

    // Update with scroll sizes from the callback
    virtualized_view_mgr.update_virtualized_view_info(
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

    let reason = virtualized_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, expanded_bounds);
    assert_eq!(reason, Some(VirtualizedViewCallbackReason::BoundsExpanded));

    // Mark as invoked for expansion
    virtualized_view_mgr.mark_invoked(parent_dom, node_id, VirtualizedViewCallbackReason::BoundsExpanded);

    // Same bounds again should return None
    let reason = virtualized_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, expanded_bounds);
    assert_eq!(reason, None);

    // Expand height as well
    let more_expanded_bounds = LogicalRect::new(
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(800.0, 600.0),
    );

    let reason = virtualized_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, more_expanded_bounds);
    assert_eq!(reason, Some(VirtualizedViewCallbackReason::BoundsExpanded));
}

#[test]
fn test_virtualized_view_manager_edge_scrolled_bottom() {
    let mut virtualized_view_mgr = VirtualizedViewManager::new();
    let mut scroll_mgr = ScrollManager::new();
    let now = test_instant();

    let parent_dom = DomId { inner: 0 };
    let node_id = NodeId::new(5);
    let bounds = LogicalRect::new(
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(800.0, 600.0),
    );

    // Initial render
    let reason = virtualized_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
    assert_eq!(reason, Some(VirtualizedViewCallbackReason::InitialRender));
    virtualized_view_mgr.mark_invoked(parent_dom, node_id, VirtualizedViewCallbackReason::InitialRender);

    // Update with large content size (scrollable)
    virtualized_view_mgr.update_virtualized_view_info(
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
    let reason = virtualized_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
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
    let reason = virtualized_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
    assert_eq!(
        reason,
        Some(VirtualizedViewCallbackReason::EdgeScrolled(EdgeType::Bottom))
    );

    // Mark as invoked for this edge
    virtualized_view_mgr.mark_invoked(
        parent_dom,
        node_id,
        VirtualizedViewCallbackReason::EdgeScrolled(EdgeType::Bottom),
    );

    // Same scroll position should not trigger again
    let reason = virtualized_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
    assert_eq!(reason, None);
}

#[test]
fn test_virtualized_view_manager_edge_scrolled_right() {
    let mut virtualized_view_mgr = VirtualizedViewManager::new();
    let mut scroll_mgr = ScrollManager::new();
    let now = test_instant();

    let parent_dom = DomId { inner: 0 };
    let node_id = NodeId::new(7);
    let bounds = LogicalRect::new(
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(800.0, 600.0),
    );

    // Initial render
    let reason = virtualized_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
    assert_eq!(reason, Some(VirtualizedViewCallbackReason::InitialRender));
    virtualized_view_mgr.mark_invoked(parent_dom, node_id, VirtualizedViewCallbackReason::InitialRender);

    // Update with wide content (scrollable horizontally)
    virtualized_view_mgr.update_virtualized_view_info(
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
    let reason = virtualized_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
    assert_eq!(
        reason,
        Some(VirtualizedViewCallbackReason::EdgeScrolled(EdgeType::Right))
    );
}

#[test]
fn test_virtualized_view_manager_nested_dom_ids() {
    let mut virtualized_view_mgr = VirtualizedViewManager::new();

    let parent_dom = DomId { inner: 0 };
    let node1 = NodeId::new(1);
    let node2 = NodeId::new(2);
    let node3 = NodeId::new(3);

    // Create nested DOM IDs
    let child1 = virtualized_view_mgr.get_or_create_nested_dom_id(parent_dom, node1);
    let child2 = virtualized_view_mgr.get_or_create_nested_dom_id(parent_dom, node2);
    let child3 = virtualized_view_mgr.get_or_create_nested_dom_id(parent_dom, node3);

    // Should be unique
    assert_ne!(child1, child2);
    assert_ne!(child2, child3);
    assert_ne!(child1, child3);

    // Should be consistent (same result when called again)
    assert_eq!(
        child1,
        virtualized_view_mgr.get_or_create_nested_dom_id(parent_dom, node1)
    );
    assert_eq!(
        child2,
        virtualized_view_mgr.get_or_create_nested_dom_id(parent_dom, node2)
    );

    // get_nested_dom_id should return existing IDs
    assert_eq!(
        virtualized_view_mgr.get_nested_dom_id(parent_dom, node1),
        Some(child1)
    );
    assert_eq!(
        virtualized_view_mgr.get_nested_dom_id(parent_dom, node2),
        Some(child2)
    );

    // Non-existent should return None
    let nonexistent = NodeId::new(999);
    assert_eq!(virtualized_view_mgr.get_nested_dom_id(parent_dom, nonexistent), None);
}

#[test]
fn test_virtualized_view_manager_was_invoked_tracking() {
    let mut virtualized_view_mgr = VirtualizedViewManager::new();
    let scroll_mgr = ScrollManager::new();

    let parent_dom = DomId { inner: 0 };
    let node_id = NodeId::new(5);
    let bounds = LogicalRect::new(
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(800.0, 600.0),
    );

    // Initially not invoked
    assert!(!virtualized_view_mgr.was_virtualized_view_invoked(parent_dom, node_id));

    // Check reinvoke to create state
    virtualized_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);

    // Still not invoked until we mark it
    assert!(!virtualized_view_mgr.was_virtualized_view_invoked(parent_dom, node_id));

    // Mark as invoked
    virtualized_view_mgr.mark_invoked(parent_dom, node_id, VirtualizedViewCallbackReason::InitialRender);

    // Now it should be invoked
    assert!(virtualized_view_mgr.was_virtualized_view_invoked(parent_dom, node_id));
}
