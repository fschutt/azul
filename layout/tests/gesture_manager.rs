use azul_layout::managers::gesture::*;

fn make_instant(millis: u64) -> CoreInstant {
    // For testing, we can use the milliseconds as ticks
    CoreInstant::Tick(azul_core::task::SystemTick {
        tick_counter: millis,
    })
}

#[test]
fn test_drag_detection() {
    let mut manager = GestureAndDragManager::new();

    // Start session at (0, 0)
    manager.start_input_session(LogicalPosition::new(0.0, 0.0), make_instant(0), 0x01);

    // Not a drag yet (distance too small)
    assert!(manager.detect_drag().is_none());

    // Move to (10, 10) - should be detected as drag
    manager.record_input_sample(LogicalPosition::new(10.0, 10.0), make_instant(100), 0x01);

    let drag = manager.detect_drag().unwrap();
    assert_eq!(drag.start_position, LogicalPosition::new(0.0, 0.0));
    assert_eq!(drag.current_position, LogicalPosition::new(10.0, 10.0));
    assert!(drag.direct_distance > 14.0); // sqrt(10^2 + 10^2) ≈ 14.14
    assert_eq!(drag.duration_ms, 100);
}

#[test]
fn test_double_click_detection() {
    let mut manager = GestureAndDragManager::new();

    // First click at (5, 5)
    manager.start_input_session(LogicalPosition::new(5.0, 5.0), make_instant(0), 0x01);
    manager.end_current_session();

    // Second click at (6, 6) after 200ms - should be double click
    manager.start_input_session(LogicalPosition::new(6.0, 6.0), make_instant(200), 0x01);
    manager.end_current_session(); // Need to end the second session too

    assert!(manager.detect_double_click());
}

#[test]
fn test_long_press_detection() {
    let mut manager = GestureAndDragManager::new();

    // Start press at (10, 10)
    manager.start_input_session(LogicalPosition::new(10.0, 10.0), make_instant(0), 0x01);

    // Record another sample after 600ms (exceeds threshold)
    manager.record_input_sample(LogicalPosition::new(10.0, 10.0), make_instant(600), 0x01);

    let long_press = manager.detect_long_press().unwrap();
    assert_eq!(long_press.position, LogicalPosition::new(10.0, 10.0));
    assert_eq!(long_press.duration_ms, 600);
    assert!(!long_press.callback_invoked);

    // Mark callback as invoked
    manager.mark_long_press_callback_invoked(long_press.session_id);

    let long_press2 = manager.detect_long_press().unwrap();
    assert!(long_press2.callback_invoked);
}

#[test]
fn test_session_cleanup() {
    let mut manager = GestureAndDragManager::new();

    // Create an old session
    manager.start_input_session(LogicalPosition::new(0.0, 0.0), make_instant(0), 0x01);
    manager.end_current_session();

    assert_eq!(manager.session_count(), 1);

    // Clear sessions that are older than 2000ms
    manager.clear_old_sessions(make_instant(3000));

    assert_eq!(manager.session_count(), 0);
}

#[test]
fn test_activate_node_drag_with_hit_test() {
    use alloc::collections::BTreeMap;

    use azul_core::hit_test::{HitTest, HitTestItem};

    let mut manager = GestureAndDragManager::new();

    // Start drag session
    manager.start_input_session(LogicalPosition::new(0.0, 0.0), make_instant(0), 0x01);
    manager.record_input_sample(LogicalPosition::new(15.0, 15.0), make_instant(100), 0x01);

    // Verify drag is detected
    assert!(manager.detect_drag().is_some());

    // Create mock hit test
    let mut hit_test = HitTest {
        regular_hit_test_nodes: BTreeMap::new(),
        scroll_hit_test_nodes: BTreeMap::new(),
        scrollbar_hit_test_nodes: BTreeMap::new(),
    };

    hit_test.regular_hit_test_nodes.insert(
        NodeId::new(1),
        HitTestItem {
            point_in_viewport: LogicalPosition::new(0.0, 0.0),
            point_relative_to_item: LogicalPosition::new(0.0, 0.0),
            is_focusable: false,
            is_iframe_hit: None,
        },
    );

    // Activate node drag with hit test
    let drag_data = DragData {
        data: BTreeMap::new(),
        effect_allowed: DragEffect::Copy,
    };

    manager.activate_node_drag(
        DomId { inner: 0 },
        NodeId::new(1),
        drag_data,
        Some(hit_test.clone()),
    );

    // Verify node drag was activated with hit test
    let node_drag = manager.get_node_drag().unwrap();
    assert_eq!(node_drag.dom_id, DomId { inner: 0 });
    assert_eq!(node_drag.node_id, NodeId::new(1));
    assert!(node_drag.start_hit_test.is_some());

    let saved_hit_test = node_drag.start_hit_test.as_ref().unwrap();
    assert_eq!(saved_hit_test.regular_hit_test_nodes.len(), 1);
}

#[test]
fn test_activate_window_drag_with_hit_test() {
    use alloc::collections::BTreeMap;

    use azul_core::{
        hit_test::{HitTest, HitTestItem},
        window::WindowPosition,
    };

    let mut manager = GestureAndDragManager::new();

    // Start drag session
    manager.start_input_session(LogicalPosition::new(10.0, 10.0), make_instant(0), 0x01);
    manager.record_input_sample(LogicalPosition::new(25.0, 25.0), make_instant(100), 0x01);

    // Create mock hit test (simulating titlebar hit)
    let mut hit_test = HitTest {
        regular_hit_test_nodes: BTreeMap::new(),
        scroll_hit_test_nodes: BTreeMap::new(),
        scrollbar_hit_test_nodes: BTreeMap::new(),
    };

    hit_test.regular_hit_test_nodes.insert(
        NodeId::new(99), // Titlebar node ID
        HitTestItem {
            point_in_viewport: LogicalPosition::new(10.0, 10.0),
            point_relative_to_item: LogicalPosition::new(10.0, 10.0),
            is_focusable: false,
            is_iframe_hit: None,
        },
    );

    // Activate window drag with hit test
    manager.activate_window_drag(
        WindowPosition::Initialized(azul_core::geom::PhysicalPositionI32::new(100, 100)),
        Some(hit_test.clone()),
    );

    // Verify window drag was activated with hit test
    let window_drag = manager.get_window_drag().unwrap();
    assert!(window_drag.start_hit_test.is_some());

    let saved_hit_test = window_drag.start_hit_test.as_ref().unwrap();
    assert_eq!(saved_hit_test.regular_hit_test_nodes.len(), 1);
    assert!(saved_hit_test
        .regular_hit_test_nodes
        .contains_key(&NodeId::new(99)));
}

#[test]
fn test_update_hit_test_methods() {
    use alloc::collections::BTreeMap;

    use azul_core::hit_test::HitTest;

    let mut manager = GestureAndDragManager::new();

    // Start drag and activate node drag
    manager.start_input_session(LogicalPosition::new(0.0, 0.0), make_instant(0), 0x01);
    manager.record_input_sample(LogicalPosition::new(20.0, 20.0), make_instant(100), 0x01);

    let drag_data = DragData {
        data: BTreeMap::new(),
        effect_allowed: DragEffect::Copy,
    };

    manager.activate_node_drag(
        DomId { inner: 0 },
        NodeId::new(1),
        drag_data,
        None, // Start with no hit test
    );

    assert!(manager.get_node_drag().unwrap().start_hit_test.is_none());

    // Update hit test later
    let hit_test = HitTest {
        regular_hit_test_nodes: BTreeMap::new(),
        scroll_hit_test_nodes: BTreeMap::new(),
        scrollbar_hit_test_nodes: BTreeMap::new(),
    };

    manager.update_node_drag_hit_test(Some(hit_test));

    // Verify hit test was updated
    assert!(manager.get_node_drag().unwrap().start_hit_test.is_some());
}
