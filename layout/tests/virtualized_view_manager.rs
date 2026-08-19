//! Tests for VirtualView lifecycle management

use azul_core::{
    callbacks::{EdgeType, VirtualViewCallbackReason},
    dom::{DomId, NodeId},
    events::EasingFunction,
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    task::{Duration, Instant},
};
use azul_layout::managers::{virtual_view::VirtualViewManager, scroll_state::ScrollManager};

fn test_instant() -> Instant {
    #[cfg(feature = "std")]
    {
        Instant::now()
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
fn test_virtual_view_manager_initial_render() {
    let mut virtual_view_mgr = VirtualViewManager::new();
    let scroll_mgr = ScrollManager::new();
    let _now = test_instant();

    let parent_dom = DomId { inner: 0 };
    let node_id = NodeId::new(5);
    let bounds = LogicalRect::new(
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(800.0, 600.0),
    );

    // First check_reinvoke should return InitialRender
    let reason = virtual_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
    assert_eq!(reason, Some(VirtualViewCallbackReason::InitialRender));

    // Second check without marking invoked should still return InitialRender
    let reason = virtual_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
    assert_eq!(reason, Some(VirtualViewCallbackReason::InitialRender));

    // Mark as invoked
    virtual_view_mgr.mark_invoked(parent_dom, node_id, VirtualViewCallbackReason::InitialRender);

    // Now it should return None (no re-invocation needed)
    let reason = virtual_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
    assert_eq!(reason, None);
}

#[test]
fn test_virtual_view_manager_bounds_expanded() {
    let mut virtual_view_mgr = VirtualViewManager::new();
    let scroll_mgr = ScrollManager::new();
    let _now = test_instant();

    let parent_dom = DomId { inner: 0 };
    let node_id = NodeId::new(5);

    // Initial render with small bounds
    let small_bounds = LogicalRect::new(
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(400.0, 300.0),
    );

    let reason = virtual_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, small_bounds);
    assert_eq!(reason, Some(VirtualViewCallbackReason::InitialRender));

    virtual_view_mgr.mark_invoked(parent_dom, node_id, VirtualViewCallbackReason::InitialRender);

    // Update with scroll sizes from the callback
    virtual_view_mgr.update_virtual_view_info(
        parent_dom,
        node_id,
        azul_core::geom::LogicalPosition::zero(),
        LogicalSize::new(400.0, 300.0),
        LogicalSize::new(400.0, 300.0),
    );

    // Expand bounds (width increases)
    let expanded_bounds = LogicalRect::new(
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(800.0, 300.0),
    );

    let reason = virtual_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, expanded_bounds);
    assert_eq!(reason, Some(VirtualViewCallbackReason::BoundsExpanded));

    // Mark as invoked for expansion
    virtual_view_mgr.mark_invoked(parent_dom, node_id, VirtualViewCallbackReason::BoundsExpanded);

    // Same bounds again should return None
    let reason = virtual_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, expanded_bounds);
    assert_eq!(reason, None);

    // Expand height as well
    let more_expanded_bounds = LogicalRect::new(
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(800.0, 600.0),
    );

    let reason = virtual_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, more_expanded_bounds);
    assert_eq!(reason, Some(VirtualViewCallbackReason::BoundsExpanded));
}

#[test]
fn test_virtual_view_manager_edge_scrolled_bottom() {
    let mut virtual_view_mgr = VirtualViewManager::new();
    let mut scroll_mgr = ScrollManager::new();
    let now = test_instant();

    let parent_dom = DomId { inner: 0 };
    let node_id = NodeId::new(5);
    let bounds = LogicalRect::new(
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(800.0, 600.0),
    );

    // Initial render
    let reason = virtual_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
    assert_eq!(reason, Some(VirtualViewCallbackReason::InitialRender));
    virtual_view_mgr.mark_invoked(parent_dom, node_id, VirtualViewCallbackReason::InitialRender);

    // The callback materialized the first 2000px of a 10000px document. The
    // edge is the edge of that WINDOW, not of the document: re-invoking is how
    // the app is asked to materialize more.
    virtual_view_mgr.update_virtual_view_info(
        parent_dom,
        node_id,
        azul_core::geom::LogicalPosition::zero(),
        LogicalSize::new(800.0, 2000.0),
        LogicalSize::new(800.0, 10_000.0),
    );

    // Initialize scroll state — the scrollbar spans the whole document
    scroll_mgr.update_node_bounds(
        parent_dom,
        node_id,
        bounds,
        LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(800.0, 10_000.0)),
        now.clone(),
    );

    // No edge yet (scroll at top)
    let reason = virtual_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
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
    let reason = virtual_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
    assert_eq!(
        reason,
        Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom))
    );

    // Mark as invoked for this edge
    virtual_view_mgr.mark_invoked(
        parent_dom,
        node_id,
        VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom),
    );

    // Same scroll position should not trigger again
    let reason = virtual_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
    assert_eq!(reason, None);
}

#[test]
fn test_virtual_view_manager_edge_scrolled_right() {
    let mut virtual_view_mgr = VirtualViewManager::new();
    let mut scroll_mgr = ScrollManager::new();
    let now = test_instant();

    let parent_dom = DomId { inner: 0 };
    let node_id = NodeId::new(7);
    let bounds = LogicalRect::new(
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(800.0, 600.0),
    );

    // Initial render
    let reason = virtual_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
    assert_eq!(reason, Some(VirtualViewCallbackReason::InitialRender));
    virtual_view_mgr.mark_invoked(parent_dom, node_id, VirtualViewCallbackReason::InitialRender);

    // The callback materialized the leftmost 3000px of a 10000px-wide document
    virtual_view_mgr.update_virtual_view_info(
        parent_dom,
        node_id,
        azul_core::geom::LogicalPosition::zero(),
        LogicalSize::new(3000.0, 600.0),
        LogicalSize::new(10_000.0, 600.0),
    );

    // Initialize scroll state — the scrollbar spans the whole document
    scroll_mgr.update_node_bounds(
        parent_dom,
        node_id,
        bounds,
        LogicalRect::new(LogicalPosition::zero(), LogicalSize::new(10_000.0, 600.0)),
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
    let reason = virtual_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
    assert_eq!(
        reason,
        Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Right))
    );
}

#[test]
fn test_virtual_view_manager_nested_dom_ids() {
    let mut virtual_view_mgr = VirtualViewManager::new();

    let parent_dom = DomId { inner: 0 };
    let node1 = NodeId::new(1);
    let node2 = NodeId::new(2);
    let node3 = NodeId::new(3);

    // Create nested DOM IDs
    let child1 = virtual_view_mgr.get_or_create_nested_dom_id(parent_dom, node1);
    let child2 = virtual_view_mgr.get_or_create_nested_dom_id(parent_dom, node2);
    let child3 = virtual_view_mgr.get_or_create_nested_dom_id(parent_dom, node3);

    // Should be unique
    assert_ne!(child1, child2);
    assert_ne!(child2, child3);
    assert_ne!(child1, child3);

    // Should be consistent (same result when called again)
    assert_eq!(
        child1,
        virtual_view_mgr.get_or_create_nested_dom_id(parent_dom, node1)
    );
    assert_eq!(
        child2,
        virtual_view_mgr.get_or_create_nested_dom_id(parent_dom, node2)
    );

    // get_nested_dom_id should return existing IDs
    assert_eq!(
        virtual_view_mgr.get_nested_dom_id(parent_dom, node1),
        Some(child1)
    );
    assert_eq!(
        virtual_view_mgr.get_nested_dom_id(parent_dom, node2),
        Some(child2)
    );

    // Non-existent should return None
    let nonexistent = NodeId::new(999);
    assert_eq!(virtual_view_mgr.get_nested_dom_id(parent_dom, nonexistent), None);
}

#[test]
fn test_virtual_view_manager_was_invoked_tracking() {
    let mut virtual_view_mgr = VirtualViewManager::new();
    let scroll_mgr = ScrollManager::new();

    let parent_dom = DomId { inner: 0 };
    let node_id = NodeId::new(5);
    let bounds = LogicalRect::new(
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(800.0, 600.0),
    );

    // Initially not invoked
    assert!(!virtual_view_mgr.was_virtual_view_invoked(parent_dom, node_id));

    // Check reinvoke to create state
    virtual_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);

    // Still not invoked until we mark it
    assert!(!virtual_view_mgr.was_virtual_view_invoked(parent_dom, node_id));

    // Mark as invoked
    virtual_view_mgr.mark_invoked(parent_dom, node_id, VirtualViewCallbackReason::InitialRender);

    // Now it should be invoked
    assert!(virtual_view_mgr.was_virtual_view_invoked(parent_dom, node_id));
}

/// The placement rule, end to end: content sits at
/// `container.origin + (materialized.origin - scroll_offset)`.
///
/// This is the arithmetic `LayoutWindow::virtual_view_content_offset` performs,
/// driven through the two real managers it reads.
#[test]
fn placement_follows_the_materialized_window_not_the_document_estimate() {
    let mut virtual_view_mgr = VirtualViewManager::new();
    let mut scroll_mgr = ScrollManager::new();
    let now = test_instant();

    let parent_dom = DomId { inner: 0 };
    let node_id = NodeId::new(5);
    let container = LogicalRect::new(
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(600.0, 400.0),
    );

    virtual_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, container);

    // The callback rendered a 600x400 window starting 300px into the document,
    // and estimated the whole document at 30000px tall.
    virtual_view_mgr.update_virtual_view_info(
        parent_dom,
        node_id,
        LogicalPosition::new(0.0, 300.0),
        LogicalSize::new(600.0, 400.0),
        LogicalSize::new(600.0, 30_000.0),
    );

    let placement = |vv: &VirtualViewManager, sm: &ScrollManager| {
        let origin = vv
            .materialized_window_origin(parent_dom, node_id)
            .unwrap_or_else(LogicalPosition::zero);
        let offset = sm
            .get_current_offset(parent_dom, node_id)
            .unwrap_or_else(LogicalPosition::zero);
        LogicalPosition::new(origin.x - offset.x, origin.y - offset.y)
    };

    // Looking at exactly where the window starts: content sits flush.
    scroll_mgr.set_scroll_position_unclamped(
        parent_dom,
        node_id,
        LogicalPosition::new(0.0, 300.0),
        now.clone(),
    );
    assert_eq!(placement(&virtual_view_mgr, &scroll_mgr), LogicalPosition::zero());

    // Scrolling 50px further down shifts the same content 50px up. Before the
    // offset reached the display list this was always zero, which is precisely
    // why a VirtualView could not be scrolled.
    scroll_mgr.set_scroll_position_unclamped(
        parent_dom,
        node_id,
        LogicalPosition::new(0.0, 350.0),
        now.clone(),
    );
    assert_eq!(
        placement(&virtual_view_mgr, &scroll_mgr),
        LogicalPosition::new(0.0, -50.0)
    );

    // Background pagination now reports the document is really 12000px, not
    // 30000. The estimate is the scrollbar's input alone: the materialized
    // window is untouched, so not one pixel may move.
    let before = placement(&virtual_view_mgr, &scroll_mgr);
    virtual_view_mgr.update_virtual_view_info(
        parent_dom,
        node_id,
        LogicalPosition::new(0.0, 300.0),
        LogicalSize::new(600.0, 400.0),
        LogicalSize::new(600.0, 12_000.0),
    );
    assert_eq!(
        placement(&virtual_view_mgr, &scroll_mgr),
        before,
        "refining the document estimate must move the scrollbar and nothing else"
    );
}

/// The edge callback exists to ask the app for MORE content. When the
/// materialized window already covers the whole document there is nothing left
/// to ask for, so scrolling to the bottom must stay quiet.
///
/// The old rule compared the scroll offset against the materialized *size* with
/// no such gate, and woke the app up on every scroll to the end of a fully
/// materialized view.
#[test]
fn a_fully_materialized_document_never_reports_an_edge() {
    let mut virtual_view_mgr = VirtualViewManager::new();
    let mut scroll_mgr = ScrollManager::new();
    let now = test_instant();

    let parent_dom = DomId { inner: 0 };
    let node_id = NodeId::new(11);
    let bounds = LogicalRect::new(
        LogicalPosition::new(0.0, 0.0),
        LogicalSize::new(800.0, 600.0),
    );
    let document = LogicalSize::new(800.0, 2000.0);

    virtual_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds);
    virtual_view_mgr.mark_invoked(parent_dom, node_id, VirtualViewCallbackReason::InitialRender);

    // materialized == document: the app rendered everything there is
    virtual_view_mgr.update_virtual_view_info(
        parent_dom,
        node_id,
        LogicalPosition::zero(),
        document,
        document,
    );
    scroll_mgr.update_node_bounds(
        parent_dom,
        node_id,
        bounds,
        LogicalRect::new(LogicalPosition::zero(), document),
        now.clone(),
    );

    // Hard against the bottom of the document
    scroll_mgr.scroll_to(
        parent_dom,
        node_id,
        LogicalPosition::new(0.0, 1400.0),
        test_duration_zero(),
        EasingFunction::Linear,
        now.clone(),
    );
    scroll_mgr.tick(now.clone());

    assert_eq!(
        virtual_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds),
        None,
        "nothing left to materialize, so no callback"
    );

    // Publishing a taller estimate is the app saying there IS more beyond the
    // window — the same scroll position must now ask for it.
    virtual_view_mgr.update_virtual_view_info(
        parent_dom,
        node_id,
        LogicalPosition::zero(),
        document,
        LogicalSize::new(800.0, 10_000.0),
    );
    assert_eq!(
        virtual_view_mgr.check_reinvoke(parent_dom, node_id, &scroll_mgr, bounds),
        Some(VirtualViewCallbackReason::EdgeScrolled(EdgeType::Bottom))
    );
}
