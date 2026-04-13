//! Tests for focus and tab navigation management

use azul_core::{
    dom::{DomId, DomNodeId, NodeId},
    styled_dom::NodeHierarchyItemId,
};
use azul_layout::{
    callbacks::FocusUpdateRequest,
    managers::focus_cursor::FocusManager,
    window::LayoutWindow,
};

/// Helper to create a minimal FcFontCache for testing
fn create_test_font_cache() -> rust_fontconfig::FcFontCache {
    rust_fontconfig::FcFontCache::default()
}

#[test]
fn test_focus_manager_basic_operations() {
    let mut manager = FocusManager::new();

    // Initially no focus
    assert_eq!(manager.get_focused_node(), None);

    // Set focus to a node
    let node1 = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(1))),
    };
    manager.set_focused_node(Some(node1.clone()));
    assert_eq!(manager.get_focused_node(), Some(&node1));

    // Change focus to another node
    let node2 = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(2))),
    };
    manager.set_focused_node(Some(node2.clone()));
    assert_eq!(manager.get_focused_node(), Some(&node2));

    // Clear focus
    manager.set_focused_node(None);
    assert_eq!(manager.get_focused_node(), None);
}

#[test]
fn test_focus_update_request_enum() {
    // Test FocusNode variant
    let node = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(5))),
    };
    let req = FocusUpdateRequest::FocusNode(node.clone());
    assert!(req.is_change());
    assert_eq!(req.to_focused_node(), Some(Some(node)));

    // Test ClearFocus variant
    let req = FocusUpdateRequest::ClearFocus;
    assert!(req.is_change());
    assert_eq!(req.to_focused_node(), Some(None));

    // Test NoChange variant
    let req = FocusUpdateRequest::NoChange;
    assert!(!req.is_change());
    assert_eq!(req.to_focused_node(), None);
}

#[test]
fn test_focus_update_request_from_optional() {
    let node = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(3))),
    };

    // Some(Some(node)) -> FocusNode
    let req = FocusUpdateRequest::from_optional(Some(Some(node.clone())));
    assert!(matches!(req, FocusUpdateRequest::FocusNode(_)));
    assert!(req.is_change());

    // Some(None) -> ClearFocus
    let req = FocusUpdateRequest::from_optional(Some(None));
    assert!(matches!(req, FocusUpdateRequest::ClearFocus));
    assert!(req.is_change());

    // None -> NoChange
    let req = FocusUpdateRequest::from_optional(None);
    assert!(matches!(req, FocusUpdateRequest::NoChange));
    assert!(!req.is_change());
}

#[test]
fn test_focus_manager_with_layout_window() {
    // Test that FocusManager integrates correctly with LayoutWindow
    let fc_cache = create_test_font_cache();
    let mut layout_window = LayoutWindow::new(fc_cache).expect("Failed to create LayoutWindow");

    // Initially no focus
    assert_eq!(layout_window.focus_manager.get_focused_node(), None);

    // Set focus
    let node = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(1))),
    };
    layout_window
        .focus_manager
        .set_focused_node(Some(node.clone()));

    // Verify focus was set
    assert_eq!(layout_window.focus_manager.get_focused_node(), Some(&node));

    // Clear focus
    layout_window.focus_manager.set_focused_node(None);
    assert_eq!(layout_window.focus_manager.get_focused_node(), None);
}

#[test]
fn test_recursive_focus_change_detection() {
    // This test simulates the recursive focus change detection
    // that happens in process_window_events_recursive_v2

    let mut focus_manager = FocusManager::new();
    let mut recursion_count = 0;
    const MAX_RECURSION: usize = 5;

    let nodes = vec![
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(0))),
        },
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(1))),
        },
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(2))),
        },
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(3))),
        },
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(4))),
        },
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(5))),
        },
    ];

    // Simulate initial focus
    focus_manager.set_focused_node(Some(nodes[0].clone()));

    // Simulate recursive focus changes (as would happen in callbacks)
    for i in 1..nodes.len() {
        if recursion_count >= MAX_RECURSION {
            break;
        }

        let old_focus = focus_manager.get_focused_node().copied();
        focus_manager.set_focused_node(Some(nodes[i].clone()));
        let new_focus = focus_manager.get_focused_node();

        // Verify focus changed
        assert_ne!(old_focus.as_ref(), new_focus);

        recursion_count += 1;
    }

    // Verify we hit the recursion limit
    assert_eq!(recursion_count, MAX_RECURSION);

    // Verify final focus state
    assert_eq!(
        focus_manager.get_focused_node(),
        Some(&nodes[MAX_RECURSION])
    );
}

#[test]
fn test_focus_clear_then_set() {
    let mut focus_manager = FocusManager::new();

    // Set initial focus
    let node1 = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(1))),
    };
    focus_manager.set_focused_node(Some(node1.clone()));
    assert_eq!(focus_manager.get_focused_node(), Some(&node1));

    // Clear focus
    focus_manager.set_focused_node(None);
    assert_eq!(focus_manager.get_focused_node(), None);

    // Set focus again to different node
    let node2 = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(2))),
    };
    focus_manager.set_focused_node(Some(node2.clone()));
    assert_eq!(focus_manager.get_focused_node(), Some(&node2));
}

#[test]
fn test_focus_update_request_conversion_edge_cases() {
    // Test with ROOT node
    let root_node = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::ZERO)),
    };
    let req = FocusUpdateRequest::FocusNode(root_node.clone());
    assert!(req.is_change());
    assert_eq!(req.to_focused_node(), Some(Some(root_node)));

    // Test multiple conversions
    let req1 = FocusUpdateRequest::ClearFocus;
    let opt1 = req1.to_focused_node();
    let req2 = FocusUpdateRequest::from_optional(opt1);
    assert!(matches!(req2, FocusUpdateRequest::ClearFocus));

    // Test round-trip NoChange
    let req1 = FocusUpdateRequest::NoChange;
    let opt1 = req1.to_focused_node();
    let req2 = FocusUpdateRequest::from_optional(opt1);
    assert!(matches!(req2, FocusUpdateRequest::NoChange));
}

#[test]
fn test_recursion_depth_limit_enforcement() {
    // Test that enforces the MAX_EVENT_RECURSION_DEPTH = 5 limit
    const MAX_DEPTH: usize = 5;
    let mut focus_manager = FocusManager::new();
    let mut depth = 0;

    // Generate nodes for depth+1 to exceed limit
    let nodes: Vec<DomNodeId> = (0..=MAX_DEPTH + 2)
        .map(|i| DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(i))),
        })
        .collect();

    // Set initial focus
    focus_manager.set_focused_node(Some(nodes[0].clone()));

    // Simulate recursive focus changes with depth tracking
    for i in 1..nodes.len() {
        if depth >= MAX_DEPTH {
            // In real code, event_v2.rs would stop recursion here
            break;
        }

        let old_focus = focus_manager.get_focused_node().copied();
        focus_manager.set_focused_node(Some(nodes[i].clone()));
        let new_focus = focus_manager.get_focused_node();

        if old_focus.as_ref() != new_focus {
            depth += 1;
        }
    }

    // Verify we stopped at MAX_DEPTH
    assert_eq!(depth, MAX_DEPTH);

    // Verify final focus is at depth MAX_DEPTH (node at index MAX_DEPTH)
    assert_eq!(focus_manager.get_focused_node(), Some(&nodes[MAX_DEPTH]));

    // Verify we didn't process nodes beyond MAX_DEPTH
    assert_ne!(
        focus_manager.get_focused_node(),
        Some(&nodes[MAX_DEPTH + 1])
    );
}

#[test]
fn test_focus_update_request_equality() {
    let node1 = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(1))),
    };
    let node2 = DomNodeId {
        dom: DomId::ROOT_ID,
        node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(2))),
    };

    // Test equality for FocusNode
    let req1 = FocusUpdateRequest::FocusNode(node1.clone());
    let req2 = FocusUpdateRequest::FocusNode(node1.clone());
    let req3 = FocusUpdateRequest::FocusNode(node2.clone());
    assert_eq!(req1, req2);
    assert_ne!(req1, req3);

    // Test equality for ClearFocus
    let req1 = FocusUpdateRequest::ClearFocus;
    let req2 = FocusUpdateRequest::ClearFocus;
    assert_eq!(req1, req2);

    // Test equality for NoChange
    let req1 = FocusUpdateRequest::NoChange;
    let req2 = FocusUpdateRequest::NoChange;
    assert_eq!(req1, req2);

    // Test inequality across variants
    let req1 = FocusUpdateRequest::FocusNode(node1.clone());
    let req2 = FocusUpdateRequest::ClearFocus;
    let req3 = FocusUpdateRequest::NoChange;
    assert_ne!(req1, req2);
    assert_ne!(req2, req3);
    assert_ne!(req1, req3);
}
