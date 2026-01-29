//! Tests for cursor management

use azul_core::{
    dom::{DomId, NodeId},
    selection::{CursorAffinity, GraphemeClusterId, TextCursor},
};
use azul_layout::managers::cursor::{CursorLocation, CursorManager};

#[test]
fn test_cursor_manager_basic_operations() {
    let mut manager = CursorManager::new();

    // Initially no cursor
    assert_eq!(manager.get_cursor(), None);
    assert_eq!(manager.get_cursor_location(), None);
    assert!(!manager.has_cursor());

    // Set cursor
    let cursor = TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: 5,
        },
        affinity: CursorAffinity::Leading,
    };
    let location = CursorLocation::new(DomId::ROOT_ID, NodeId::new(1));

    manager.set_cursor(Some(cursor.clone()), Some(location.clone()));

    assert_eq!(manager.get_cursor(), Some(&cursor));
    assert_eq!(manager.get_cursor_location(), Some(&location));
    assert!(manager.has_cursor());

    // Clear cursor
    manager.clear();

    assert_eq!(manager.get_cursor(), None);
    assert_eq!(manager.get_cursor_location(), None);
    assert!(!manager.has_cursor());
}

#[test]
fn test_initialize_cursor_at_start() {
    let mut manager = CursorManager::new();

    manager.initialize_cursor_at_start(DomId::ROOT_ID, NodeId::new(5));

    assert!(manager.has_cursor());
    let cursor = manager.get_cursor().unwrap();
    assert_eq!(cursor.cluster_id.source_run, 0);
    assert_eq!(cursor.cluster_id.start_byte_in_run, 0);
    assert_eq!(cursor.affinity, CursorAffinity::Trailing);

    let location = manager.get_cursor_location().unwrap();
    assert_eq!(location.dom_id, DomId::ROOT_ID);
    assert_eq!(location.node_id, NodeId::new(5));
}

#[test]
fn test_move_cursor_to() {
    let mut manager = CursorManager::new();

    // Initial position
    let cursor1 = TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: 0,
        },
        affinity: CursorAffinity::Leading,
    };
    manager.move_cursor_to(cursor1.clone(), DomId::ROOT_ID, NodeId::new(1));

    assert_eq!(manager.get_cursor(), Some(&cursor1));
    assert_eq!(
        manager.get_cursor_location(),
        Some(&CursorLocation::new(DomId::ROOT_ID, NodeId::new(1)))
    );

    // Move to new position
    let cursor2 = TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 1,
            start_byte_in_run: 10,
        },
        affinity: CursorAffinity::Trailing,
    };
    manager.move_cursor_to(cursor2.clone(), DomId::ROOT_ID, NodeId::new(2));

    assert_eq!(manager.get_cursor(), Some(&cursor2));
    assert_eq!(
        manager.get_cursor_location(),
        Some(&CursorLocation::new(DomId::ROOT_ID, NodeId::new(2)))
    );
}

#[test]
fn test_is_cursor_in_node() {
    let mut manager = CursorManager::new();

    // No cursor initially
    assert!(!manager.is_cursor_in_node(DomId::ROOT_ID, NodeId::new(1)));

    // Set cursor in node 1
    manager.initialize_cursor_at_start(DomId::ROOT_ID, NodeId::new(1));

    assert!(manager.is_cursor_in_node(DomId::ROOT_ID, NodeId::new(1)));
    assert!(!manager.is_cursor_in_node(DomId::ROOT_ID, NodeId::new(2)));
    assert!(!manager.is_cursor_in_node(DomId { inner: 1 }, NodeId::new(1)));
}

#[test]
fn test_cursor_affinity() {
    let mut manager = CursorManager::new();

    // Leading affinity
    let cursor_leading = TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: 5,
        },
        affinity: CursorAffinity::Leading,
    };
    manager.set_cursor(
        Some(cursor_leading.clone()),
        Some(CursorLocation::new(DomId::ROOT_ID, NodeId::new(1))),
    );

    assert_eq!(
        manager.get_cursor().unwrap().affinity,
        CursorAffinity::Leading
    );

    // Trailing affinity
    let cursor_trailing = TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 0,
            start_byte_in_run: 5,
        },
        affinity: CursorAffinity::Trailing,
    };
    manager.set_cursor(
        Some(cursor_trailing.clone()),
        Some(CursorLocation::new(DomId::ROOT_ID, NodeId::new(1))),
    );

    assert_eq!(
        manager.get_cursor().unwrap().affinity,
        CursorAffinity::Trailing
    );
}

#[test]
fn test_cursor_manager_clear_resets_all_state() {
    let mut manager = CursorManager::new();

    // Set cursor with location
    let cursor = TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: 2,
            start_byte_in_run: 15,
        },
        affinity: CursorAffinity::Leading,
    };
    let location = CursorLocation::with_key(DomId { inner: 3 }, NodeId::new(42), 0);
    manager.set_cursor(Some(cursor), Some(location));

    // Verify state is set
    assert!(manager.has_cursor());
    assert!(manager.get_cursor().is_some());
    assert!(manager.get_cursor_location().is_some());

    // Clear
    manager.clear();

    // Verify all state is cleared
    assert!(!manager.has_cursor());
    assert!(manager.get_cursor().is_none());
    assert!(manager.get_cursor_location().is_none());
}

#[test]
fn test_cursor_location_equality() {
    let loc1 = CursorLocation::new(DomId::ROOT_ID, NodeId::new(5));
    let loc2 = CursorLocation::new(DomId::ROOT_ID, NodeId::new(5));
    let loc3 = CursorLocation::new(DomId::ROOT_ID, NodeId::new(6));
    let loc4 = CursorLocation::new(DomId { inner: 1 }, NodeId::new(5));

    assert_eq!(loc1, loc2);
    assert_ne!(loc1, loc3);
    assert_ne!(loc1, loc4);
    assert_ne!(loc3, loc4);
}
