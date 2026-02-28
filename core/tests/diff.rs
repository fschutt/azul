
extern crate alloc;

use azul_core::FastHashMap;
use azul_core::dom::{NodeData, DomId};
use azul_core::id::NodeId;
use azul_core::geom::LogicalRect;
use azul_core::diff::{reconcile_dom, reconcile_cursor_position, transfer_states, create_migration_map, NodeMove};
use azul_core::task::Instant;

#[test]
fn test_simple_mount() {
    let old_data: Vec<NodeData> = vec![];
    let new_data = vec![NodeData::create_div()];
    
    let old_layout = FastHashMap::default();
    let mut new_layout = FastHashMap::default();
    new_layout.insert(NodeId::new(0), LogicalRect::zero());
    
    let result = reconcile_dom(
        &old_data,
        &new_data,
        &old_layout,
        &new_layout,
        DomId { inner: 0 },
        Instant::now(),
    );
    
    // No mount event because no callback is registered
    assert!(result.events.is_empty());
    assert!(result.node_moves.is_empty());
}

#[test]
fn test_identical_nodes_match() {
    let div = NodeData::create_div();
    let old_data = vec![div.clone()];
    let new_data = vec![div.clone()];
    
    let mut old_layout = FastHashMap::default();
    old_layout.insert(NodeId::new(0), LogicalRect::zero());
    let mut new_layout = FastHashMap::default();
    new_layout.insert(NodeId::new(0), LogicalRect::zero());
    
    let result = reconcile_dom(
        &old_data,
        &new_data,
        &old_layout,
        &new_layout,
        DomId { inner: 0 },
        Instant::now(),
    );
    
    // Should match by hash, no lifecycle events
    assert!(result.events.is_empty());
    assert_eq!(result.node_moves.len(), 1);
    assert_eq!(result.node_moves[0].old_node_id, NodeId::new(0));
    assert_eq!(result.node_moves[0].new_node_id, NodeId::new(0));
}

#[test]
fn test_reorder_by_hash() {
    use azul_css::AzString;
    
    let mut div_a = NodeData::create_div();
    div_a.add_class(AzString::from("a"));
    let mut div_b = NodeData::create_div();
    div_b.add_class(AzString::from("b"));
    
    // Old: [A, B], New: [B, A]
    let old_data = vec![div_a.clone(), div_b.clone()];
    let new_data = vec![div_b.clone(), div_a.clone()];
    
    let mut old_layout = FastHashMap::default();
    old_layout.insert(NodeId::new(0), LogicalRect::zero());
    old_layout.insert(NodeId::new(1), LogicalRect::zero());
    
    let mut new_layout = FastHashMap::default();
    new_layout.insert(NodeId::new(0), LogicalRect::zero());
    new_layout.insert(NodeId::new(1), LogicalRect::zero());
    
    let result = reconcile_dom(
        &old_data,
        &new_data,
        &old_layout,
        &new_layout,
        DomId { inner: 0 },
        Instant::now(),
    );
    
    // Both should match by hash (reorder detected)
    assert!(result.events.is_empty());
    assert_eq!(result.node_moves.len(), 2);
    
    // B (old index 1) -> B (new index 0)
    assert!(result.node_moves.iter().any(|m| 
        m.old_node_id == NodeId::new(1) && m.new_node_id == NodeId::new(0)
    ));
    // A (old index 0) -> A (new index 1)
    assert!(result.node_moves.iter().any(|m| 
        m.old_node_id == NodeId::new(0) && m.new_node_id == NodeId::new(1)
    ));
}

// ========== EDGE CASE TESTS ==========

#[test]
fn test_empty_to_empty() {
    let old_data: Vec<NodeData> = vec![];
    let new_data: Vec<NodeData> = vec![];
    
    let result = reconcile_dom(
        &old_data,
        &new_data,
        &FastHashMap::default(),
        &FastHashMap::default(),
        DomId { inner: 0 },
        Instant::now(),
    );
    
    assert!(result.events.is_empty());
    assert!(result.node_moves.is_empty());
}

#[test]
fn test_all_nodes_removed() {
    let old_data = vec![
        NodeData::create_div(),
        NodeData::create_div(),
        NodeData::create_div(),
    ];
    let new_data: Vec<NodeData> = vec![];
    
    let mut old_layout = FastHashMap::default();
    for i in 0..3 {
        old_layout.insert(NodeId::new(i), LogicalRect::zero());
    }
    
    let result = reconcile_dom(
        &old_data,
        &new_data,
        &old_layout,
        &FastHashMap::default(),
        DomId { inner: 0 },
        Instant::now(),
    );
    
    // No events because no callbacks, but no node moves either
    assert!(result.events.is_empty());
    assert!(result.node_moves.is_empty());
}

#[test]
fn test_all_nodes_added() {
    let old_data: Vec<NodeData> = vec![];
    let new_data = vec![
        NodeData::create_div(),
        NodeData::create_div(),
        NodeData::create_div(),
    ];
    
    let mut new_layout = FastHashMap::default();
    for i in 0..3 {
        new_layout.insert(NodeId::new(i), LogicalRect::zero());
    }
    
    let result = reconcile_dom(
        &old_data,
        &new_data,
        &FastHashMap::default(),
        &new_layout,
        DomId { inner: 0 },
        Instant::now(),
    );
    
    // No events because no callbacks, but no node moves (all are new)
    assert!(result.events.is_empty());
    assert!(result.node_moves.is_empty());
}

#[test]
fn test_keyed_node_match() {
    // Create two nodes with the same key but different content
    let mut old_node = NodeData::create_div();
    old_node.set_key("my-key");
    
    let mut new_node = NodeData::create_div();
    new_node.set_key("my-key");
    new_node.add_class(azul_css::AzString::from("updated"));
    
    let old_data = vec![old_node];
    let new_data = vec![new_node];
    
    let mut old_layout = FastHashMap::default();
    old_layout.insert(NodeId::new(0), LogicalRect::zero());
    let mut new_layout = FastHashMap::default();
    new_layout.insert(NodeId::new(0), LogicalRect::zero());
    
    let result = reconcile_dom(
        &old_data,
        &new_data,
        &old_layout,
        &new_layout,
        DomId { inner: 0 },
        Instant::now(),
    );
    
    // Should match by key even though hash is different
    assert_eq!(result.node_moves.len(), 1);
    assert_eq!(result.node_moves[0].old_node_id, NodeId::new(0));
    assert_eq!(result.node_moves[0].new_node_id, NodeId::new(0));
}

#[test]
fn test_keyed_reorder() {
    let mut node_a = NodeData::create_div();
    node_a.set_key("key-a");
    let mut node_b = NodeData::create_div();
    node_b.set_key("key-b");
    let mut node_c = NodeData::create_div();
    node_c.set_key("key-c");
    
    // Old: [A, B, C], New: [C, B, A]
    let old_data = vec![node_a.clone(), node_b.clone(), node_c.clone()];
    let new_data = vec![node_c.clone(), node_b.clone(), node_a.clone()];
    
    let mut old_layout = FastHashMap::default();
    let mut new_layout = FastHashMap::default();
    for i in 0..3 {
        old_layout.insert(NodeId::new(i), LogicalRect::zero());
        new_layout.insert(NodeId::new(i), LogicalRect::zero());
    }
    
    let result = reconcile_dom(
        &old_data,
        &new_data,
        &old_layout,
        &new_layout,
        DomId { inner: 0 },
        Instant::now(),
    );
    
    assert_eq!(result.node_moves.len(), 3);
    
    // C: old[2] -> new[0]
    assert!(result.node_moves.iter().any(|m| 
        m.old_node_id == NodeId::new(2) && m.new_node_id == NodeId::new(0)
    ));
    // B: old[1] -> new[1] (unchanged position)
    assert!(result.node_moves.iter().any(|m| 
        m.old_node_id == NodeId::new(1) && m.new_node_id == NodeId::new(1)
    ));
    // A: old[0] -> new[2]
    assert!(result.node_moves.iter().any(|m| 
        m.old_node_id == NodeId::new(0) && m.new_node_id == NodeId::new(2)
    ));
}

#[test]
fn test_identical_nodes_fifo() {
    // 5 identical divs - should match FIFO
    let div = NodeData::create_div();
    let old_data = vec![div.clone(), div.clone(), div.clone()];
    let new_data = vec![div.clone(), div.clone()]; // Remove last one
    
    let mut old_layout = FastHashMap::default();
    let mut new_layout = FastHashMap::default();
    for i in 0..3 {
        old_layout.insert(NodeId::new(i), LogicalRect::zero());
    }
    for i in 0..2 {
        new_layout.insert(NodeId::new(i), LogicalRect::zero());
    }
    
    let result = reconcile_dom(
        &old_data,
        &new_data,
        &old_layout,
        &new_layout,
        DomId { inner: 0 },
        Instant::now(),
    );
    
    // First two should match (FIFO), third is unmounted
    assert_eq!(result.node_moves.len(), 2);
    assert!(result.node_moves.iter().any(|m| 
        m.old_node_id == NodeId::new(0) && m.new_node_id == NodeId::new(0)
    ));
    assert!(result.node_moves.iter().any(|m| 
        m.old_node_id == NodeId::new(1) && m.new_node_id == NodeId::new(1)
    ));
}

#[test]
fn test_insert_at_beginning() {
    use azul_css::AzString;
    
    let mut div_a = NodeData::create_div();
    div_a.add_class(AzString::from("a"));
    let mut div_b = NodeData::create_div();
    div_b.add_class(AzString::from("b"));
    let mut div_new = NodeData::create_div();
    div_new.add_class(AzString::from("new"));
    
    // Old: [A, B], New: [NEW, A, B]
    let old_data = vec![div_a.clone(), div_b.clone()];
    let new_data = vec![div_new.clone(), div_a.clone(), div_b.clone()];
    
    let mut old_layout = FastHashMap::default();
    let mut new_layout = FastHashMap::default();
    for i in 0..2 {
        old_layout.insert(NodeId::new(i), LogicalRect::zero());
    }
    for i in 0..3 {
        new_layout.insert(NodeId::new(i), LogicalRect::zero());
    }
    
    let result = reconcile_dom(
        &old_data,
        &new_data,
        &old_layout,
        &new_layout,
        DomId { inner: 0 },
        Instant::now(),
    );
    
    // A and B should be matched (moved), NEW is mounted (but no callback)
    assert_eq!(result.node_moves.len(), 2);
    
    // A: old[0] -> new[1]
    assert!(result.node_moves.iter().any(|m| 
        m.old_node_id == NodeId::new(0) && m.new_node_id == NodeId::new(1)
    ));
    // B: old[1] -> new[2]
    assert!(result.node_moves.iter().any(|m| 
        m.old_node_id == NodeId::new(1) && m.new_node_id == NodeId::new(2)
    ));
}

#[test]
fn test_insert_in_middle() {
    use azul_css::AzString;
    
    let mut div_a = NodeData::create_div();
    div_a.add_class(AzString::from("a"));
    let mut div_b = NodeData::create_div();
    div_b.add_class(AzString::from("b"));
    let mut div_new = NodeData::create_div();
    div_new.add_class(AzString::from("new"));
    
    // Old: [A, B], New: [A, NEW, B]
    let old_data = vec![div_a.clone(), div_b.clone()];
    let new_data = vec![div_a.clone(), div_new.clone(), div_b.clone()];
    
    let mut old_layout = FastHashMap::default();
    let mut new_layout = FastHashMap::default();
    for i in 0..2 {
        old_layout.insert(NodeId::new(i), LogicalRect::zero());
    }
    for i in 0..3 {
        new_layout.insert(NodeId::new(i), LogicalRect::zero());
    }
    
    let result = reconcile_dom(
        &old_data,
        &new_data,
        &old_layout,
        &new_layout,
        DomId { inner: 0 },
        Instant::now(),
    );
    
    assert_eq!(result.node_moves.len(), 2);
    
    // A: old[0] -> new[0] (same position)
    assert!(result.node_moves.iter().any(|m| 
        m.old_node_id == NodeId::new(0) && m.new_node_id == NodeId::new(0)
    ));
    // B: old[1] -> new[2]
    assert!(result.node_moves.iter().any(|m| 
        m.old_node_id == NodeId::new(1) && m.new_node_id == NodeId::new(2)
    ));
}

#[test]
fn test_remove_from_middle() {
    use azul_css::AzString;
    
    let mut div_a = NodeData::create_div();
    div_a.add_class(AzString::from("a"));
    let mut div_b = NodeData::create_div();
    div_b.add_class(AzString::from("b"));
    let mut div_c = NodeData::create_div();
    div_c.add_class(AzString::from("c"));
    
    // Old: [A, B, C], New: [A, C]
    let old_data = vec![div_a.clone(), div_b.clone(), div_c.clone()];
    let new_data = vec![div_a.clone(), div_c.clone()];
    
    let mut old_layout = FastHashMap::default();
    let mut new_layout = FastHashMap::default();
    for i in 0..3 {
        old_layout.insert(NodeId::new(i), LogicalRect::zero());
    }
    for i in 0..2 {
        new_layout.insert(NodeId::new(i), LogicalRect::zero());
    }
    
    let result = reconcile_dom(
        &old_data,
        &new_data,
        &old_layout,
        &new_layout,
        DomId { inner: 0 },
        Instant::now(),
    );
    
    // A and C matched, B unmounted (no callback so no event)
    assert_eq!(result.node_moves.len(), 2);
    
    // A: old[0] -> new[0]
    assert!(result.node_moves.iter().any(|m| 
        m.old_node_id == NodeId::new(0) && m.new_node_id == NodeId::new(0)
    ));
    // C: old[2] -> new[1]
    assert!(result.node_moves.iter().any(|m| 
        m.old_node_id == NodeId::new(2) && m.new_node_id == NodeId::new(1)
    ));
}

#[test]
fn test_mixed_keyed_and_unkeyed() {
    use azul_css::AzString;
    
    let mut keyed = NodeData::create_div();
    keyed.set_key("my-key");
    keyed.add_class(AzString::from("keyed"));
    
    let mut unkeyed = NodeData::create_div();
    unkeyed.add_class(AzString::from("unkeyed"));
    
    // Old: [keyed, unkeyed], New: [unkeyed, keyed]
    let old_data = vec![keyed.clone(), unkeyed.clone()];
    let new_data = vec![unkeyed.clone(), keyed.clone()];
    
    let mut old_layout = FastHashMap::default();
    let mut new_layout = FastHashMap::default();
    for i in 0..2 {
        old_layout.insert(NodeId::new(i), LogicalRect::zero());
        new_layout.insert(NodeId::new(i), LogicalRect::zero());
    }
    
    let result = reconcile_dom(
        &old_data,
        &new_data,
        &old_layout,
        &new_layout,
        DomId { inner: 0 },
        Instant::now(),
    );
    
    assert_eq!(result.node_moves.len(), 2);
    
    // keyed: old[0] -> new[1] (matched by key)
    assert!(result.node_moves.iter().any(|m| 
        m.old_node_id == NodeId::new(0) && m.new_node_id == NodeId::new(1)
    ));
    // unkeyed: old[1] -> new[0] (matched by hash)
    assert!(result.node_moves.iter().any(|m| 
        m.old_node_id == NodeId::new(1) && m.new_node_id == NodeId::new(0)
    ));
}

#[test]
fn test_duplicate_keys() {
    // Two nodes with the same key - first one wins
    let mut node1 = NodeData::create_div();
    node1.set_key("duplicate");
    node1.add_class(azul_css::AzString::from("first"));
    
    let mut node2 = NodeData::create_div();
    node2.set_key("duplicate");
    node2.add_class(azul_css::AzString::from("second"));
    
    let old_data = vec![node1.clone()];
    let new_data = vec![node2.clone()];
    
    let mut old_layout = FastHashMap::default();
    old_layout.insert(NodeId::new(0), LogicalRect::zero());
    let mut new_layout = FastHashMap::default();
    new_layout.insert(NodeId::new(0), LogicalRect::zero());
    
    let result = reconcile_dom(
        &old_data,
        &new_data,
        &old_layout,
        &new_layout,
        DomId { inner: 0 },
        Instant::now(),
    );
    
    // Should match by key
    assert_eq!(result.node_moves.len(), 1);
}

#[test]
fn test_key_not_in_old() {
    // New node has a key that didn't exist in old
    let old_div = NodeData::create_div();
    
    let mut new_div = NodeData::create_div();
    new_div.set_key("new-key");
    
    let old_data = vec![old_div];
    let new_data = vec![new_div];
    
    let mut old_layout = FastHashMap::default();
    old_layout.insert(NodeId::new(0), LogicalRect::zero());
    let mut new_layout = FastHashMap::default();
    new_layout.insert(NodeId::new(0), LogicalRect::zero());
    
    let result = reconcile_dom(
        &old_data,
        &new_data,
        &old_layout,
        &new_layout,
        DomId { inner: 0 },
        Instant::now(),
    );
    
    // Key doesn't match, so new keyed node is mount, old unkeyed is unmount
    // No events because no callbacks
    assert!(result.node_moves.is_empty()); // No match by key or hash
}

#[test]
fn test_large_list_reorder() {
    use azul_css::AzString;
    
    // Create 100 unique nodes
    let nodes: Vec<NodeData> = (0..100).map(|i| {
        let mut node = NodeData::create_div();
        node.add_class(AzString::from(format!("item-{}", i)));
        node
    }).collect();
    
    // Reverse the order
    let old_data = nodes.clone();
    let new_data: Vec<NodeData> = nodes.into_iter().rev().collect();
    
    let mut old_layout = FastHashMap::default();
    let mut new_layout = FastHashMap::default();
    for i in 0..100 {
        old_layout.insert(NodeId::new(i), LogicalRect::zero());
        new_layout.insert(NodeId::new(i), LogicalRect::zero());
    }
    
    let result = reconcile_dom(
        &old_data,
        &new_data,
        &old_layout,
        &new_layout,
        DomId { inner: 0 },
        Instant::now(),
    );
    
    // All 100 nodes should be matched (just reordered)
    assert_eq!(result.node_moves.len(), 100);
    assert!(result.events.is_empty());
}

#[test]
fn test_migration_map() {
    use azul_css::AzString;
    
    let mut div_a = NodeData::create_div();
    div_a.add_class(AzString::from("a"));
    let mut div_b = NodeData::create_div();
    div_b.add_class(AzString::from("b"));
    
    let old_data = vec![div_a.clone(), div_b.clone()];
    let new_data = vec![div_b.clone(), div_a.clone()];
    
    let mut old_layout = FastHashMap::default();
    let mut new_layout = FastHashMap::default();
    for i in 0..2 {
        old_layout.insert(NodeId::new(i), LogicalRect::zero());
        new_layout.insert(NodeId::new(i), LogicalRect::zero());
    }
    
    let result = reconcile_dom(
        &old_data,
        &new_data,
        &old_layout,
        &new_layout,
        DomId { inner: 0 },
        Instant::now(),
    );
    
    let migration = create_migration_map(&result.node_moves);
    
    // old[0] (A) -> new[1]
    assert_eq!(migration.get(&NodeId::new(0)), Some(&NodeId::new(1)));
    // old[1] (B) -> new[0]
    assert_eq!(migration.get(&NodeId::new(1)), Some(&NodeId::new(0)));
}

#[test]
fn test_different_node_types() {
    // Different node types should not match by hash
    let div = NodeData::create_div();
    let span = NodeData::create_node(azul_core::dom::NodeType::Span);
    
    let old_data = vec![div];
    let new_data = vec![span];
    
    let mut old_layout = FastHashMap::default();
    old_layout.insert(NodeId::new(0), LogicalRect::zero());
    let mut new_layout = FastHashMap::default();
    new_layout.insert(NodeId::new(0), LogicalRect::zero());
    
    let result = reconcile_dom(
        &old_data,
        &new_data,
        &old_layout,
        &new_layout,
        DomId { inner: 0 },
        Instant::now(),
    );
    
    // Should not match (different types = different hashes)
    assert!(result.node_moves.is_empty());
}

#[test]
fn test_text_nodes() {
    use azul_css::AzString;
    
    let text_a = NodeData::create_text(AzString::from("Hello"));
    let text_b = NodeData::create_text(AzString::from("World"));
    let text_a_copy = NodeData::create_text(AzString::from("Hello"));
    
    // Old: ["Hello", "World"], New: ["World", "Hello"]
    let old_data = vec![text_a.clone(), text_b.clone()];
    let new_data = vec![text_b.clone(), text_a_copy.clone()];
    
    let mut old_layout = FastHashMap::default();
    let mut new_layout = FastHashMap::default();
    for i in 0..2 {
        old_layout.insert(NodeId::new(i), LogicalRect::zero());
        new_layout.insert(NodeId::new(i), LogicalRect::zero());
    }
    
    let result = reconcile_dom(
        &old_data,
        &new_data,
        &old_layout,
        &new_layout,
        DomId { inner: 0 },
        Instant::now(),
    );
    
    // Should match by content hash
    assert_eq!(result.node_moves.len(), 2);
}

#[test]
fn test_shuffle_three() {
    use azul_css::AzString;
    
    let mut a = NodeData::create_div();
    a.add_class(AzString::from("a"));
    let mut b = NodeData::create_div();
    b.add_class(AzString::from("b"));
    let mut c = NodeData::create_div();
    c.add_class(AzString::from("c"));
    
    // Old: [A, B, C], New: [B, C, A]
    let old_data = vec![a.clone(), b.clone(), c.clone()];
    let new_data = vec![b.clone(), c.clone(), a.clone()];
    
    let mut old_layout = FastHashMap::default();
    let mut new_layout = FastHashMap::default();
    for i in 0..3 {
        old_layout.insert(NodeId::new(i), LogicalRect::zero());
        new_layout.insert(NodeId::new(i), LogicalRect::zero());
    }
    
    let result = reconcile_dom(
        &old_data,
        &new_data,
        &old_layout,
        &new_layout,
        DomId { inner: 0 },
        Instant::now(),
    );
    
    assert_eq!(result.node_moves.len(), 3);
    
    // A: old[0] -> new[2]
    assert!(result.node_moves.iter().any(|m| 
        m.old_node_id == NodeId::new(0) && m.new_node_id == NodeId::new(2)
    ));
    // B: old[1] -> new[0]
    assert!(result.node_moves.iter().any(|m| 
        m.old_node_id == NodeId::new(1) && m.new_node_id == NodeId::new(0)
    ));
    // C: old[2] -> new[1]
    assert!(result.node_moves.iter().any(|m| 
        m.old_node_id == NodeId::new(2) && m.new_node_id == NodeId::new(1)
    ));
}

// =========================================================================
// MERGE CALLBACK / STATE MIGRATION TESTS
// =========================================================================

use azul_core::refany::{RefAny, OptionRefAny};
use azul_core::dom::DatasetMergeCallbackType;
use alloc::sync::Arc;
use core::cell::RefCell;

/// Test data simulating a video player with a heavy decoder handle
struct VideoPlayerState {
    url: alloc::string::String,
    decoder_handle: Option<u64>, // Simulates heavy resource (e.g., FFmpeg handle)
}

/// Simple merge callback that transfers decoder_handle from old to new
extern "C" fn merge_video_state(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
    // Get mutable access to new, immutable to old
    if let Some(mut new_guard) = new_data.downcast_mut::<VideoPlayerState>() {
        if let Some(old_guard) = old_data.downcast_ref::<VideoPlayerState>() {
            // Transfer heavy resource
            new_guard.decoder_handle = old_guard.decoder_handle;
        }
    }
    new_data
}

#[test]
fn test_transfer_states_basic() {
    // Scenario: Video player moves from index 0 to index 1
    // The decoder handle should be preserved
    
    let old_state = VideoPlayerState {
        url: "movie.mp4".into(),
        decoder_handle: Some(12345),
    };
    let new_state = VideoPlayerState {
        url: "movie.mp4".into(),
        decoder_handle: None, // Fresh state, no handle yet
    };
    
    let mut old_node = NodeData::create_div();
    old_node.set_dataset(OptionRefAny::Some(RefAny::new(old_state)));
    
    let mut new_node = NodeData::create_div();
    new_node.set_dataset(OptionRefAny::Some(RefAny::new(new_state)));
    new_node.set_merge_callback(merge_video_state as DatasetMergeCallbackType);
    
    let mut old_data = vec![old_node];
    let mut new_data = vec![new_node];
    
    let moves = vec![NodeMove {
        old_node_id: NodeId::new(0),
        new_node_id: NodeId::new(0),
    }];
    
    // Execute state migration
    transfer_states(&mut old_data, &mut new_data, &moves);
    
    // Verify the handle was transferred
    if let Some(dataset) = new_data[0].get_dataset_mut() {
        let guard = dataset.downcast_ref::<VideoPlayerState>().unwrap();
        assert_eq!(guard.decoder_handle, Some(12345));
    } else {
        panic!("Dataset should exist");
    }
}

#[test]
fn test_transfer_states_no_callback_no_transfer() {
    // If no merge callback is set, nothing should happen
    
    let old_state = VideoPlayerState {
        url: "movie.mp4".into(),
        decoder_handle: Some(99999),
    };
    let new_state = VideoPlayerState {
        url: "movie.mp4".into(),
        decoder_handle: None,
    };
    
    let mut old_node = NodeData::create_div();
    old_node.set_dataset(OptionRefAny::Some(RefAny::new(old_state)));
    
    let mut new_node = NodeData::create_div();
    new_node.set_dataset(OptionRefAny::Some(RefAny::new(new_state)));
    // NO merge callback set!
    
    let mut old_data = vec![old_node];
    let mut new_data = vec![new_node];
    
    let moves = vec![NodeMove {
        old_node_id: NodeId::new(0),
        new_node_id: NodeId::new(0),
    }];
    
    transfer_states(&mut old_data, &mut new_data, &moves);
    
    // Handle should NOT be transferred (no callback)
    if let Some(dataset) = new_data[0].get_dataset_mut() {
        let guard = dataset.downcast_ref::<VideoPlayerState>().unwrap();
        assert_eq!(guard.decoder_handle, None); // Still None!
    } else {
        panic!("Dataset should exist");
    }
}

#[test]
fn test_transfer_states_no_old_dataset() {
    // If old node has no dataset, merge should not crash
    
    let new_state = VideoPlayerState {
        url: "movie.mp4".into(),
        decoder_handle: None,
    };
    
    let old_node = NodeData::create_div(); // No dataset
    
    let mut new_node = NodeData::create_div();
    new_node.set_dataset(OptionRefAny::Some(RefAny::new(new_state)));
    new_node.set_merge_callback(merge_video_state as DatasetMergeCallbackType);
    
    let mut old_data = vec![old_node];
    let mut new_data = vec![new_node];
    
    let moves = vec![NodeMove {
        old_node_id: NodeId::new(0),
        new_node_id: NodeId::new(0),
    }];
    
    // Should not panic
    transfer_states(&mut old_data, &mut new_data, &moves);
    
    // New node should still have its dataset (unmodified)
    if let Some(dataset) = new_data[0].get_dataset_mut() {
        let guard = dataset.downcast_ref::<VideoPlayerState>().unwrap();
        assert_eq!(guard.decoder_handle, None);
    } else {
        panic!("Dataset should still exist");
    }
}

#[test]
fn test_transfer_states_no_new_dataset() {
    // If new node has merge callback but no dataset, nothing should happen
    
    let old_state = VideoPlayerState {
        url: "movie.mp4".into(),
        decoder_handle: Some(77777),
    };
    
    let mut old_node = NodeData::create_div();
    old_node.set_dataset(OptionRefAny::Some(RefAny::new(old_state)));
    
    let mut new_node = NodeData::create_div();
    // No dataset on new node!
    new_node.set_merge_callback(merge_video_state as DatasetMergeCallbackType);
    
    let mut old_data = vec![old_node];
    let mut new_data = vec![new_node];
    
    let moves = vec![NodeMove {
        old_node_id: NodeId::new(0),
        new_node_id: NodeId::new(0),
    }];
    
    // Should not panic
    transfer_states(&mut old_data, &mut new_data, &moves);
    
    // New node should still have no dataset
    assert!(new_data[0].get_dataset().is_none());
}

#[test]
fn test_transfer_states_reorder_preserves_handles() {
    // Scenario: Two video players swap positions
    // Each should keep its own decoder handle
    
    let state_a = VideoPlayerState { url: "a.mp4".into(), decoder_handle: Some(111) };
    let state_b = VideoPlayerState { url: "b.mp4".into(), decoder_handle: Some(222) };
    
    let mut old_a = NodeData::create_div();
    old_a.set_key("player-a");
    old_a.set_dataset(OptionRefAny::Some(RefAny::new(state_a)));
    
    let mut old_b = NodeData::create_div();
    old_b.set_key("player-b");
    old_b.set_dataset(OptionRefAny::Some(RefAny::new(state_b)));
    
    // New order: B, A (swapped)
    let new_state_b = VideoPlayerState { url: "b.mp4".into(), decoder_handle: None };
    let new_state_a = VideoPlayerState { url: "a.mp4".into(), decoder_handle: None };
    
    let mut new_b = NodeData::create_div();
    new_b.set_key("player-b");
    new_b.set_dataset(OptionRefAny::Some(RefAny::new(new_state_b)));
    new_b.set_merge_callback(merge_video_state as DatasetMergeCallbackType);
    
    let mut new_a = NodeData::create_div();
    new_a.set_key("player-a");
    new_a.set_dataset(OptionRefAny::Some(RefAny::new(new_state_a)));
    new_a.set_merge_callback(merge_video_state as DatasetMergeCallbackType);
    
    let mut old_data = vec![old_a, old_b];  // [A@0, B@1]
    let mut new_data = vec![new_b, new_a];  // [B@0, A@1]
    
    // Moves from reconciliation: old_a(0)->new(1), old_b(1)->new(0)
    let moves = vec![
        NodeMove { old_node_id: NodeId::new(0), new_node_id: NodeId::new(1) }, // A
        NodeMove { old_node_id: NodeId::new(1), new_node_id: NodeId::new(0) }, // B
    ];
    
    transfer_states(&mut old_data, &mut new_data, &moves);
    
    // new[0] should be B with handle 222
    if let Some(ds) = new_data[0].get_dataset_mut() {
        let guard = ds.downcast_ref::<VideoPlayerState>().unwrap();
        assert_eq!(guard.url, "b.mp4");
        assert_eq!(guard.decoder_handle, Some(222));
    } else {
        panic!("B dataset missing");
    }
    
    // new[1] should be A with handle 111
    if let Some(ds) = new_data[1].get_dataset_mut() {
        let guard = ds.downcast_ref::<VideoPlayerState>().unwrap();
        assert_eq!(guard.url, "a.mp4");
        assert_eq!(guard.decoder_handle, Some(111));
    } else {
        panic!("A dataset missing");
    }
}

#[test]
fn test_transfer_states_out_of_bounds() {
    // Invalid node moves should be skipped gracefully
    
    let state = VideoPlayerState { url: "test.mp4".into(), decoder_handle: Some(123) };
    
    let mut old_node = NodeData::create_div();
    old_node.set_dataset(OptionRefAny::Some(RefAny::new(state)));
    
    let mut old_data = vec![old_node];
    let mut new_data: Vec<NodeData> = vec![]; // Empty!
    
    let moves = vec![NodeMove {
        old_node_id: NodeId::new(0),
        new_node_id: NodeId::new(999), // Out of bounds
    }];
    
    // Should not panic
    transfer_states(&mut old_data, &mut new_data, &moves);
}

/// Test with a more complex type that uses interior mutability
struct WebGLContext {
    texture_ids: alloc::vec::Vec<u32>,
    shader_program: Option<u32>,
}

extern "C" fn merge_webgl_context(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
    if let Some(mut new_guard) = new_data.downcast_mut::<WebGLContext>() {
        if let Some(old_guard) = old_data.downcast_ref::<WebGLContext>() {
            // Transfer all GL resources
            new_guard.texture_ids = old_guard.texture_ids.clone();
            new_guard.shader_program = old_guard.shader_program;
        }
    }
    new_data
}

#[test]
fn test_transfer_states_complex_type() {
    let old_ctx = WebGLContext {
        texture_ids: vec![1, 2, 3, 4, 5],
        shader_program: Some(42),
    };
    let new_ctx = WebGLContext {
        texture_ids: vec![],
        shader_program: None,
    };
    
    let mut old_node = NodeData::create_div();
    old_node.set_dataset(OptionRefAny::Some(RefAny::new(old_ctx)));
    
    let mut new_node = NodeData::create_div();
    new_node.set_dataset(OptionRefAny::Some(RefAny::new(new_ctx)));
    new_node.set_merge_callback(merge_webgl_context as DatasetMergeCallbackType);
    
    let mut old_data = vec![old_node];
    let mut new_data = vec![new_node];
    
    let moves = vec![NodeMove {
        old_node_id: NodeId::new(0),
        new_node_id: NodeId::new(0),
    }];
    
    transfer_states(&mut old_data, &mut new_data, &moves);
    
    if let Some(ds) = new_data[0].get_dataset_mut() {
        let guard = ds.downcast_ref::<WebGLContext>().unwrap();
        assert_eq!(guard.texture_ids, vec![1, 2, 3, 4, 5]);
        assert_eq!(guard.shader_program, Some(42));
    } else {
        panic!("Dataset missing");
    }
}

#[test]
fn test_transfer_states_callback_returns_old_data() {
    // Test that callback can choose to return old_data instead of new_data
    
    struct Counter {
        value: u32,
    }
    
    extern "C" fn prefer_old(_new_data: RefAny, old_data: RefAny) -> RefAny {
        // Return old_data to preserve the old state entirely
        old_data
    }
    
    let old_counter = Counter { value: 100 };
    let new_counter = Counter { value: 0 };
    
    let mut old_node = NodeData::create_div();
    old_node.set_dataset(OptionRefAny::Some(RefAny::new(old_counter)));
    
    let mut new_node = NodeData::create_div();
    new_node.set_dataset(OptionRefAny::Some(RefAny::new(new_counter)));
    new_node.set_merge_callback(prefer_old as DatasetMergeCallbackType);
    
    let mut old_data = vec![old_node];
    let mut new_data = vec![new_node];
    
    let moves = vec![NodeMove {
        old_node_id: NodeId::new(0),
        new_node_id: NodeId::new(0),
    }];
    
    transfer_states(&mut old_data, &mut new_data, &moves);
    
    // The callback returned old_data, so new node should have value=100
    if let Some(ds) = new_data[0].get_dataset_mut() {
        let guard = ds.downcast_ref::<Counter>().unwrap();
        assert_eq!(guard.value, 100);
    } else {
        panic!("Dataset missing");
    }
}

#[test]
fn test_transfer_states_multiple_nodes_partial_callbacks() {
    // Scenario: 3 nodes, only middle one has merge callback
    
    struct Simple { val: u32 }
    
    extern "C" fn merge_simple(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
        if let Some(mut new_g) = new_data.downcast_mut::<Simple>() {
            if let Some(old_g) = old_data.downcast_ref::<Simple>() {
                new_g.val = old_g.val;
            }
        }
        new_data
    }
    
    let mut old_nodes = vec![
        {
            let mut n = NodeData::create_div();
            n.set_dataset(OptionRefAny::Some(RefAny::new(Simple { val: 1 })));
            n
        },
        {
            let mut n = NodeData::create_div();
            n.set_dataset(OptionRefAny::Some(RefAny::new(Simple { val: 2 })));
            n
        },
        {
            let mut n = NodeData::create_div();
            n.set_dataset(OptionRefAny::Some(RefAny::new(Simple { val: 3 })));
            n
        },
    ];
    
    let mut new_nodes = vec![
        {
            let mut n = NodeData::create_div();
            n.set_dataset(OptionRefAny::Some(RefAny::new(Simple { val: 10 })));
            // NO callback
            n
        },
        {
            let mut n = NodeData::create_div();
            n.set_dataset(OptionRefAny::Some(RefAny::new(Simple { val: 20 })));
            n.set_merge_callback(merge_simple as DatasetMergeCallbackType); // HAS callback
            n
        },
        {
            let mut n = NodeData::create_div();
            n.set_dataset(OptionRefAny::Some(RefAny::new(Simple { val: 30 })));
            // NO callback
            n
        },
    ];
    
    let moves = vec![
        NodeMove { old_node_id: NodeId::new(0), new_node_id: NodeId::new(0) },
        NodeMove { old_node_id: NodeId::new(1), new_node_id: NodeId::new(1) },
        NodeMove { old_node_id: NodeId::new(2), new_node_id: NodeId::new(2) },
    ];
    
    transfer_states(&mut old_nodes, &mut new_nodes, &moves);
    
    // Node 0: no callback, should keep val=10
    if let Some(ds) = new_nodes[0].get_dataset_mut() {
        let g = ds.downcast_ref::<Simple>().unwrap();
        assert_eq!(g.val, 10);
    }
    
    // Node 1: has callback, should get val=2 from old
    if let Some(ds) = new_nodes[1].get_dataset_mut() {
        let g = ds.downcast_ref::<Simple>().unwrap();
        assert_eq!(g.val, 2);
    }
    
    // Node 2: no callback, should keep val=30
    if let Some(ds) = new_nodes[2].get_dataset_mut() {
        let g = ds.downcast_ref::<Simple>().unwrap();
        assert_eq!(g.val, 30);
    }
}

#[test]
fn test_transfer_states_empty_moves() {
    // No moves = no transfers
    let mut old_data: Vec<NodeData> = vec![];
    let mut new_data: Vec<NodeData> = vec![];
    let moves: Vec<NodeMove> = vec![];
    
    // Should not panic
    transfer_states(&mut old_data, &mut new_data, &moves);
}

#[test]
fn test_reconcile_then_transfer_integration() {
    // Full integration test: reconcile DOM, then transfer states
    
    struct AppState { 
        name: alloc::string::String, 
        heavy_handle: Option<u64> 
    }
    
    extern "C" fn merge_app(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
        if let Some(mut new_g) = new_data.downcast_mut::<AppState>() {
            if let Some(old_g) = old_data.downcast_ref::<AppState>() {
                new_g.heavy_handle = old_g.heavy_handle;
            }
        }
        new_data
    }
    
    // OLD DOM: one node with key "main" and handle=999
    let old_state = AppState { name: "old".into(), heavy_handle: Some(999) };
    let mut old_node = NodeData::create_div();
    old_node.set_key("main");
    old_node.set_dataset(OptionRefAny::Some(RefAny::new(old_state)));
    
    // NEW DOM: same key, new state, needs merge
    let new_state = AppState { name: "new".into(), heavy_handle: None };
    let mut new_node = NodeData::create_div();
    new_node.set_key("main");
    new_node.set_dataset(OptionRefAny::Some(RefAny::new(new_state)));
    new_node.set_merge_callback(merge_app as DatasetMergeCallbackType);
    
    let mut old_data = vec![old_node];
    let mut new_data = vec![new_node];
    
    let mut old_layout = FastHashMap::default();
    old_layout.insert(NodeId::new(0), LogicalRect::zero());
    let mut new_layout = FastHashMap::default();
    new_layout.insert(NodeId::new(0), LogicalRect::zero());
    
    // Step 1: Reconcile
    let diff = reconcile_dom(
        &old_data,
        &new_data,
        &old_layout,
        &new_layout,
        DomId { inner: 0 },
        Instant::now(),
    );
    
    // Should match by key
    assert_eq!(diff.node_moves.len(), 1);
    assert_eq!(diff.node_moves[0].old_node_id, NodeId::new(0));
    assert_eq!(diff.node_moves[0].new_node_id, NodeId::new(0));
    
    // Step 2: Transfer states
    transfer_states(&mut old_data, &mut new_data, &diff.node_moves);
    
    // Verify: name should be "new", handle should be 999
    if let Some(ds) = new_data[0].get_dataset_mut() {
        let g = ds.downcast_ref::<AppState>().unwrap();
        assert_eq!(g.name, "new");
        assert_eq!(g.heavy_handle, Some(999));
    } else {
        panic!("Dataset missing");
    }
}

// ========== CURSOR RECONCILIATION TESTS ==========

#[test]
fn test_cursor_reconcile_identical_text() {
    let result = reconcile_cursor_position("Hello", "Hello", 3);
    assert_eq!(result, 3);
}

#[test]
fn test_cursor_reconcile_text_appended() {
    // Cursor at end of "Hello", text becomes "Hello World"
    let result = reconcile_cursor_position("Hello", "Hello World", 5);
    assert_eq!(result, 5); // Cursor stays at same position (in prefix)
}

#[test]
fn test_cursor_reconcile_text_prepended() {
    // Cursor at "H|ello", text becomes "Say Hello"
    // "Hello" is a common suffix, so cursor in the suffix is remapped:
    // offset_from_end = 5 - 1 = 4, new position = 9 - 4 = 5
    let result = reconcile_cursor_position("Hello", "Say Hello", 1);
    assert_eq!(result, 5); // Same relative position within the suffix "Hello"
}

#[test]
fn test_cursor_reconcile_suffix_preserved() {
    // Text: "Hello World" -> "Hi World", cursor at "World" (position 6)
    // "World" is in suffix, so cursor should adjust
    let result = reconcile_cursor_position("Hello World", "Hi World", 6);
    // In old text: "Hello World" (11 chars), cursor at 6
    // Suffix " World" (6 chars) is preserved
    // Old suffix starts at 5 (11-6), new suffix starts at 2 (8-6)
    // Cursor is at 6, which is in suffix (>= 5)
    // Offset from end: 11-6 = 5, new position: 8-5 = 3
    assert_eq!(result, 3);
}

#[test]
fn test_cursor_reconcile_empty_to_text() {
    let result = reconcile_cursor_position("", "Hello", 0);
    assert_eq!(result, 5); // Cursor at end of new text
}

#[test]
fn test_cursor_reconcile_text_to_empty() {
    let result = reconcile_cursor_position("Hello", "", 3);
    assert_eq!(result, 0); // Cursor at 0
}

#[test]
fn test_cursor_reconcile_insert_at_cursor() {
    // Typing 'X' at cursor position 3 in "Hello" -> "HelXlo"
    let result = reconcile_cursor_position("Hello", "HelXlo", 3);
    // Common prefix: "Hel" (3 bytes), cursor is at 3 (at end of prefix)
    // Should stay at 3
    assert_eq!(result, 3);
}

#[test]
fn test_structural_hash_text_nodes_match() {
    use azul_css::AzString;
    
    // Two text nodes with different content should have same structural hash
    let text_a = NodeData::create_text(AzString::from("Hello"));
    let text_b = NodeData::create_text(AzString::from("Hello World"));
    
    // Content hash should be different
    assert_ne!(text_a.calculate_node_data_hash(), text_b.calculate_node_data_hash());
    
    // Structural hash should be the same (both are Text nodes)
    assert_eq!(text_a.calculate_structural_hash(), text_b.calculate_structural_hash());
}

#[test]
fn test_structural_hash_different_types() {
    use azul_css::AzString;
    
    // Div and Text should have different structural hashes
    let div = NodeData::create_div();
    let text = NodeData::create_text(AzString::from("Hello"));
    
    assert_ne!(div.calculate_structural_hash(), text.calculate_structural_hash());
}

#[test]
fn test_text_nodes_match_by_structural_hash() {
    use azul_css::AzString;
    
    // Old DOM has Text("Hello"), new DOM has Text("Hello World")
    // They should match by structural hash
    let old_data = vec![NodeData::create_text(AzString::from("Hello"))];
    let new_data = vec![NodeData::create_text(AzString::from("Hello World"))];
    
    let mut old_layout = FastHashMap::default();
    old_layout.insert(NodeId::new(0), LogicalRect::zero());
    let mut new_layout = FastHashMap::default();
    new_layout.insert(NodeId::new(0), LogicalRect::zero());
    
    let result = reconcile_dom(
        &old_data,
        &new_data,
        &old_layout,
        &new_layout,
        DomId { inner: 0 },
        Instant::now(),
    );
    
    // Should match by structural hash (same node, just different text content)
    assert_eq!(result.node_moves.len(), 1);
    assert_eq!(result.node_moves[0].old_node_id, NodeId::new(0));
    assert_eq!(result.node_moves[0].new_node_id, NodeId::new(0));
}
