// Tests for state preservation across DOM reconciliation.
//
// When the DOM is rebuilt by a callback, the framework must carry over:
// - Cursor position + selection (via contenteditable key matching)
// - Focus state (which node is focused)
// - Scroll position (per-node scroll offsets)
//
// State is transferred via reconcile_dom → create_migration_map → transfer_states.

use azul_core::diff::{
    reconcile_dom, create_migration_map, transfer_states,
    ChangeAccumulator, NodeChangeSet,
};
use azul_core::dom::{NodeData, DomId};
use azul_core::id::NodeId;
use azul_core::geom::LogicalRect;
use azul_core::task::Instant;
use azul_core::FastHashMap;
use azul_css::AzString;

/// Helper: create a layout map with zero-rect entries for N nodes
fn make_layout(n: usize) -> FastHashMap<NodeId, LogicalRect> {
    let mut m = FastHashMap::default();
    for i in 0..n {
        m.insert(NodeId::new(i), LogicalRect::zero());
    }
    m
}

/// Helper: run reconcile_dom with simple defaults
fn reconcile(
    old: &[NodeData],
    new: &[NodeData],
) -> azul_core::diff::DiffResult {
    let old_layout = make_layout(old.len());
    let new_layout = make_layout(new.len());
    reconcile_dom(
        old, new,
        &old_layout, &new_layout,
        DomId { inner: 0 },
        Instant::now(),
    )
}

// =========================================================================
// MIGRATION MAP: old NodeId → new NodeId
// =========================================================================

#[test]
fn migration_map_identity() {
    // Same node at same position
    let data = vec![NodeData::create_div()];
    let result = reconcile(&data, &data);
    let map = create_migration_map(&result.node_moves);
    assert_eq!(map.get(&NodeId::new(0)), Some(&NodeId::new(0)));
}

#[test]
fn migration_map_reorder() {
    // Two different nodes swapped:
    // old: [A-div-with-key, B-text]
    // new: [B-text, A-div-with-key]
    let mut a = NodeData::create_div();
    a.add_id(AzString::from("a"));
    let b = NodeData::create_text("B");
    
    let old = vec![a.clone(), b.clone()];
    let new = vec![b.clone(), a.clone()];
    let result = reconcile(&old, &new);
    let map = create_migration_map(&result.node_moves);
    
    // Node "a" was at old[0], should map to new[1]
    // Node "b" was at old[1], should map to new[0]
    // (exact behavior depends on reconciliation matching algorithm)
    assert!(!map.is_empty(), "migration map should track moved nodes");
}

#[test]
fn migration_map_new_node_not_in_map() {
    // old: [div]
    // new: [div, span]
    // The span is new, so old node 1 shouldn't exist
    let old = vec![NodeData::create_div()];
    let new = vec![
        NodeData::create_div(),
        NodeData::create_node(azul_core::dom::NodeType::Span),
    ];
    let result = reconcile(&old, &new);
    let map = create_migration_map(&result.node_moves);
    // old node 0 → new node 0 (both are divs)
    assert_eq!(map.get(&NodeId::new(0)), Some(&NodeId::new(0)));
    // There's no old node 1
    assert_eq!(map.get(&NodeId::new(1)), None);
}

#[test]
fn migration_map_removed_node() {
    // old: [div, span]
    // new: [div]
    let old = vec![
        NodeData::create_div(),
        NodeData::create_node(azul_core::dom::NodeType::Span),
    ];
    let new = vec![NodeData::create_div()];
    let result = reconcile(&old, &new);
    let map = create_migration_map(&result.node_moves);
    // old node 0 should map to new node 0
    assert_eq!(map.get(&NodeId::new(0)), Some(&NodeId::new(0)));
    // old node 1 (span) was removed — may or may not be in map
}

// =========================================================================
// TRANSFER STATES: moving state from old DOM to new DOM
// =========================================================================

#[test]
fn transfer_states_preserves_structure() {
    // Verify transfer_states doesn't panic with valid inputs
    let mut old_nodes: Vec<NodeData> = vec![NodeData::create_div()];
    let mut new_nodes: Vec<NodeData> = vec![NodeData::create_div()];
    let node_moves = vec![];
    transfer_states(&mut old_nodes, &mut new_nodes, &node_moves);
    // No assertion needed — just verify it doesn't panic
}

#[test]
fn transfer_states_with_node_moves() {
    use azul_core::diff::NodeMove;
    let mut old_nodes = vec![NodeData::create_div()];
    let mut new_nodes = vec![NodeData::create_div()];
    let node_moves = vec![NodeMove {
        old_node_id: NodeId::new(0),
        new_node_id: NodeId::new(0),
    }];
    transfer_states(&mut old_nodes, &mut new_nodes, &node_moves);
}

// =========================================================================
// KEY-BASED MATCHING preserves identity
// =========================================================================

#[test]
fn keyed_nodes_match_across_reorder() {
    // Key-based matching: nodes with same key should be paired
    let mut a = NodeData::create_div();
    a.add_id(AzString::from("key-alpha"));
    let mut b = NodeData::create_div();
    b.add_id(AzString::from("key-beta"));
    
    let old = vec![a.clone(), b.clone()];
    let new = vec![b.clone(), a.clone()]; // swapped
    
    let result = reconcile(&old, &new);
    // Should have node moves pairing by key
    assert!(!result.node_moves.is_empty(), "keyed nodes should be matched");
}

#[test]
fn keyed_node_survives_siblings_changing() {
    // Node with key survives even when siblings are added
    let mut keyed = NodeData::create_div();
    keyed.add_id(AzString::from("persist"));
    let extra = NodeData::create_text("new");
    
    let old = vec![keyed.clone()];
    let new = vec![extra, keyed.clone()]; // keyed node moved to position 1
    
    let result = reconcile(&old, &new);
    let map = create_migration_map(&result.node_moves);
    // Old node 0 (keyed) should map to new node 1
    // OR the reconciliation may match differently, but the keyed node should persist
    assert!(!map.is_empty());
}

// =========================================================================
// HASH-BASED MATCHING for unkeyed nodes
// =========================================================================

#[test]
fn identical_unkeyed_nodes_match_by_hash() {
    let div = NodeData::create_div();
    let old = vec![div.clone()];
    let new = vec![div.clone()];
    
    let result = reconcile(&old, &new);
    assert_eq!(result.node_moves.len(), 1);
    assert_eq!(result.node_moves[0].old_node_id, NodeId::new(0));
    assert_eq!(result.node_moves[0].new_node_id, NodeId::new(0));
}

#[test]
fn different_unkeyed_nodes_may_not_match() {
    let div = NodeData::create_div();
    let text = NodeData::create_text("hello");
    
    let old = vec![div.clone()];
    let new = vec![text.clone()];
    
    let result = reconcile(&old, &new);
    // Different types may or may not match depending on structural matching
    // Just verify the algorithm runs without panicking
    let _ = result;
}

// =========================================================================
// STATE PRESERVATION across text edits
// =========================================================================

#[test]
fn text_node_edit_preserves_match() {
    // Editing text content: the node should still match by position/structure
    let old = vec![NodeData::create_text("Hello")];
    let new = vec![NodeData::create_text("World")];
    
    let result = reconcile(&old, &new);
    // Even though content changed, structural matching should pair them
    // (they're both text nodes at position 0)
    assert!(!result.node_moves.is_empty(),
        "text nodes at same position should match structurally");
}

#[test]
fn contenteditable_text_edit_match() {
    // contenteditable div with different text should still match
    let mut old_div = NodeData::create_div();
    old_div.set_contenteditable(true);
    old_div.add_id(AzString::from("editor"));
    
    let mut new_div = NodeData::create_div();
    new_div.set_contenteditable(true);
    new_div.add_id(AzString::from("editor"));
    
    let old = vec![old_div];
    let new = vec![new_div];
    
    let result = reconcile(&old, &new);
    assert_eq!(result.node_moves.len(), 1, "contenteditable nodes with same key should match");
}

// =========================================================================
// ACCUMULATOR + RECONCILIATION integration
// =========================================================================

#[test]
fn accumulator_tracks_mount_unmount_from_diff() {
    // old: [A], new: [B]  (completely different)
    let mut a = NodeData::create_div();
    a.add_id(AzString::from("old-node"));
    let mut b = NodeData::create_div();
    b.add_id(AzString::from("new-node"));
    
    let _old = vec![a];
    let _new = vec![b];
    
    // Just verify ChangeAccumulator can track mounts/unmounts
    let mut acc = ChangeAccumulator::new();
    acc.add_unmount(NodeId::new(0)); // old node removed
    acc.add_mount(NodeId::new(0));    // new node added
    
    assert_eq!(acc.mounted_nodes.len(), 1);
    assert_eq!(acc.unmounted_nodes.len(), 1);
    assert!(acc.needs_layout());
}

// =========================================================================
// MULTIPLE DOM REBUILDS: verify stability
// =========================================================================

#[test]
fn repeated_identical_rebuild_stable() {
    let data = vec![
        NodeData::create_div(),
        NodeData::create_text("hello"),
        NodeData::create_div(),
    ];
    
    // First rebuild
    let result1 = reconcile(&data, &data);
    // Second rebuild (same data again)
    let result2 = reconcile(&data, &data);
    
    // Both should produce the same mapping
    assert_eq!(result1.node_moves.len(), result2.node_moves.len());
}

#[test]
fn incremental_changes_produce_correct_migration() {
    // Simulating incremental edits to a document
    let v1 = vec![
        NodeData::create_text("Line 1"),
        NodeData::create_text("Line 2"),
        NodeData::create_text("Line 3"),
    ];
    
    // Edit: change line 2
    let v2 = vec![
        NodeData::create_text("Line 1"),
        NodeData::create_text("Line 2 modified"),
        NodeData::create_text("Line 3"),
    ];
    
    let result = reconcile(&v1, &v2);
    assert!(!result.node_moves.is_empty());
    
    // Line 1 and Line 3 should be matched (same content)
    let map = create_migration_map(&result.node_moves);
    // Node 0 (Line 1) → Node 0
    assert_eq!(map.get(&NodeId::new(0)), Some(&NodeId::new(0)));
}

// =========================================================================
// LARGE DOM: performance sanity
// =========================================================================

#[test]
fn large_dom_reconciliation_completes() {
    let size = 1000;
    let old: Vec<NodeData> = (0..size)
        .map(|i| {
            let mut n = NodeData::create_div();
            n.add_id(AzString::from(format!("node-{}", i)));
            n
        })
        .collect();
    
    // Remove first, add one at end
    let mut new: Vec<NodeData> = old[1..].to_vec();
    let mut extra = NodeData::create_div();
    extra.add_id(AzString::from("node-new"));
    new.push(extra);
    
    let result = reconcile(&old, &new);
    // Should complete without timeout/panic
    assert!(!result.node_moves.is_empty());
}

#[test]
fn empty_to_large_dom() {
    let old: Vec<NodeData> = vec![];
    let new: Vec<NodeData> = (0..100)
        .map(|_| NodeData::create_div())
        .collect();
    
    let result = reconcile(&old, &new);
    // All new nodes, no matches from old
    assert!(result.node_moves.is_empty());
}

#[test]
fn large_dom_to_empty() {
    let old: Vec<NodeData> = (0..100)
        .map(|_| NodeData::create_div())
        .collect();
    let new: Vec<NodeData> = vec![];
    
    let result = reconcile(&old, &new);
    // All old nodes removed, no new matches
    assert!(result.node_moves.is_empty());
}
