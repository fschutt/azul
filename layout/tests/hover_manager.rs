//! Tests for hover state management

use azul_layout::managers::hover::{HoverManager, InputPointId};
use azul_layout::hit_test::FullHitTest;

const MAX_HOVER_HISTORY: usize = 5;

#[test]
fn test_hover_manager_push_and_get() {
    let mut manager = HoverManager::new();
    let mouse_id = InputPointId::Mouse;

    assert_eq!(manager.frame_count(&mouse_id), 0);

    let hit1 = FullHitTest::empty(None);
    manager.push_hit_test(mouse_id, hit1.clone());

    assert_eq!(manager.frame_count(&mouse_id), 1);
    assert_eq!(manager.get_current(&mouse_id), Some(&hit1));
    assert_eq!(manager.get_current_mouse(), Some(&hit1));
}

#[test]
fn test_hover_manager_multi_touch() {
    let mut manager = HoverManager::new();
    let mouse_id = InputPointId::Mouse;
    let touch1_id = InputPointId::Touch(1);
    let touch2_id = InputPointId::Touch(2);

    let hit_mouse = FullHitTest::empty(None);
    let hit_touch1 = FullHitTest::empty(None);
    let hit_touch2 = FullHitTest::empty(None);

    manager.push_hit_test(mouse_id, hit_mouse.clone());
    manager.push_hit_test(touch1_id, hit_touch1.clone());
    manager.push_hit_test(touch2_id, hit_touch2.clone());

    assert_eq!(manager.get_active_input_points().len(), 3);
    assert_eq!(manager.get_current(&mouse_id), Some(&hit_mouse));
    assert_eq!(manager.get_current(&touch1_id), Some(&hit_touch1));
    assert_eq!(manager.get_current(&touch2_id), Some(&hit_touch2));
}

#[test]
fn test_hover_manager_frame_limit() {
    let mut manager = HoverManager::new();
    let mouse_id = InputPointId::Mouse;

    // Push 7 frames (more than MAX_HOVER_HISTORY = 5)
    for _ in 0..7 {
        let hit = FullHitTest::empty(None);
        manager.push_hit_test(mouse_id, hit);
    }

    // Should only keep the last 5
    assert_eq!(manager.frame_count(&mouse_id), MAX_HOVER_HISTORY);
}

#[test]
fn test_remove_input_point() {
    let mut manager = HoverManager::new();
    let touch_id = InputPointId::Touch(1);

    manager.push_hit_test(touch_id, FullHitTest::empty(None));
    assert_eq!(manager.frame_count(&touch_id), 1);

    manager.remove_input_point(&touch_id);
    assert_eq!(manager.frame_count(&touch_id), 0);
    assert_eq!(manager.get_current(&touch_id), None);
}

#[test]
fn test_gesture_history_check() {
    let mut manager = HoverManager::new();
    let mouse_id = InputPointId::Mouse;

    assert!(!manager.has_sufficient_history_for_gestures(&mouse_id));
    assert!(!manager.any_has_sufficient_history_for_gestures());

    manager.push_hit_test(mouse_id, FullHitTest::empty(None));
    assert!(!manager.has_sufficient_history_for_gestures(&mouse_id));

    manager.push_hit_test(mouse_id, FullHitTest::empty(None));
    assert!(manager.has_sufficient_history_for_gestures(&mouse_id));
    assert!(manager.any_has_sufficient_history_for_gestures());
}
