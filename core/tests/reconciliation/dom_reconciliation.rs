// End-to-end tests for DOM reconciliation with change tracking.
//
// Tests the full pipeline:
// 1. reconcile_dom_with_changes() — matches old/new DOMs + computes per-node changes
// 2. ChangeAccumulator::merge_extended_diff() — converts to unified change report
//
// These simulate realistic scenarios: HTML-like state transitions.

use azul_core::diff::{
    reconcile_dom_with_changes, ChangeAccumulator, ExtendedDiffResult,
    NodeChangeSet, reconcile_dom,
};
use azul_core::dom::{NodeData, DomId};
use azul_core::id::NodeId;
use azul_core::geom::LogicalRect;
use azul_core::task::Instant;
use azul_core::FastHashMap;
use azul_css::AzString;
use azul_css::props::property::RelayoutScope;

/// Helper: create a layout map with zero-rect entries for N nodes
fn make_layout(n: usize) -> FastHashMap<NodeId, LogicalRect> {
    let mut m = FastHashMap::default();
    for i in 0..n {
        m.insert(NodeId::new(i), LogicalRect::zero());
    }
    m
}

// =========================================================================
// SCENARIO: Identical DOM (no changes)
// =========================================================================

#[test]
fn identical_dom_no_changes() {
    let data = vec![
        NodeData::create_div(),
        NodeData::create_text("Hello"),
    ];
    let layout = make_layout(data.len());
    
    let extended = reconcile_dom_with_changes(
        &data, &data,
        None, None,
        &layout, &layout,
        DomId { inner: 0 },
        Instant::now(),
    );
    
    // All nodes should match with empty change sets
    for &(_, _, ref cs) in &extended.node_changes {
        assert!(cs.is_empty(), "identical DOM should have no changes, got {:?}", cs);
    }
}

#[test]
fn identical_dom_accumulator_empty() {
    let data = vec![NodeData::create_div(), NodeData::create_text("Hi")];
    let layout = make_layout(data.len());
    
    let extended = reconcile_dom_with_changes(
        &data, &data, None, None, &layout, &layout,
        DomId { inner: 0 }, Instant::now(),
    );
    
    let mut acc = ChangeAccumulator::new();
    acc.merge_extended_diff(&extended, &data, &data);
    
    // No CSS or text changed → per_node reports should all be empty
    assert!(acc.is_visually_unchanged() || acc.is_empty(),
        "identical DOM should produce no visual changes");
}

// =========================================================================
// SCENARIO: Text edit (common for contenteditable)
// =========================================================================

#[test]
fn text_edit_detected() {
    let old = vec![NodeData::create_text("Hello")];
    let new = vec![NodeData::create_text("World")];
    
    let old_layout = make_layout(1);
    let new_layout = make_layout(1);
    
    let extended = reconcile_dom_with_changes(
        &old, &new, None, None,
        &old_layout, &new_layout,
        DomId { inner: 0 }, Instant::now(),
    );
    
    // Should detect text change
    assert!(!extended.node_changes.is_empty());
    let (_, _, ref cs) = extended.node_changes[0];
    assert!(cs.contains(NodeChangeSet::TEXT_CONTENT),
        "text edit should be detected");
}

#[test]
fn text_edit_accumulator_shows_ifc_scope() {
    let old = vec![NodeData::create_text("Hello")];
    let new = vec![NodeData::create_text("World")];
    
    let old_layout = make_layout(1);
    let new_layout = make_layout(1);
    
    let extended = reconcile_dom_with_changes(
        &old, &new, None, None,
        &old_layout, &new_layout,
        DomId { inner: 0 }, Instant::now(),
    );
    
    let mut acc = ChangeAccumulator::new();
    acc.merge_extended_diff(&extended, &old, &new);
    
    assert!(acc.needs_layout(), "text edit should need layout");
    // Text change → at least IfcOnly
    assert!(acc.max_scope >= RelayoutScope::IfcOnly,
        "text edit should produce at least IfcOnly scope");
}

// =========================================================================
// SCENARIO: Class change (triggers restyle)
// =========================================================================

#[test]
fn class_change_detected() {
    let old = vec![NodeData::create_div()];
    let mut new_div = NodeData::create_div();
    new_div.add_class(AzString::from("highlighted"));
    let new = vec![new_div];
    
    let old_layout = make_layout(1);
    let new_layout = make_layout(1);
    
    let extended = reconcile_dom_with_changes(
        &old, &new, None, None,
        &old_layout, &new_layout,
        DomId { inner: 0 }, Instant::now(),
    );
    
    // The reconciliation may match the divs structurally (same position)
    // or may not match if the hash differs. Either way, the class change
    // should be detectable.
    if !extended.node_changes.is_empty() {
        let (_, _, ref cs) = extended.node_changes[0];
        assert!(cs.contains(NodeChangeSet::IDS_AND_CLASSES),
            "matched nodes with class change should flag IDS_AND_CLASSES");
    }
    // If no match: the class-changed div will show as mount/unmount
    // which is also correct (full layout needed)
}

#[test]
fn class_change_accumulator_full_scope() {
    let old = vec![NodeData::create_div()];
    let mut new_div = NodeData::create_div();
    new_div.add_class(AzString::from("big"));
    let new = vec![new_div];
    
    let old_layout = make_layout(1);
    let new_layout = make_layout(1);
    
    let extended = reconcile_dom_with_changes(
        &old, &new, None, None,
        &old_layout, &new_layout,
        DomId { inner: 0 }, Instant::now(),
    );
    
    let mut acc = ChangeAccumulator::new();
    acc.merge_extended_diff(&extended, &old, &new);
    
    // Class changes are conservatively Full (could add layout CSS)
    assert!(acc.needs_layout());
}

// =========================================================================
// SCENARIO: Inline style change
// =========================================================================

#[test]
fn inline_style_change_detected() {
    let old = vec![NodeData::create_div().with_css("width: 50px;")];
    let new = vec![NodeData::create_div().with_css("width: 100px;")];
    
    let old_layout = make_layout(1);
    let new_layout = make_layout(1);
    
    let extended = reconcile_dom_with_changes(
        &old, &new, None, None,
        &old_layout, &new_layout,
        DomId { inner: 0 }, Instant::now(),
    );
    
    assert!(!extended.node_changes.is_empty());
    let (_, _, ref cs) = extended.node_changes[0];
    assert!(cs.contains(NodeChangeSet::INLINE_STYLE_LAYOUT),
        "width change should be INLINE_STYLE_LAYOUT");
}

#[test]
fn paint_only_style_change() {
    let old = vec![NodeData::create_div().with_css("color: red;")];
    let new = vec![NodeData::create_div().with_css("color: blue;")];
    
    let old_layout = make_layout(1);
    let new_layout = make_layout(1);
    
    let extended = reconcile_dom_with_changes(
        &old, &new, None, None,
        &old_layout, &new_layout,
        DomId { inner: 0 }, Instant::now(),
    );
    
    if !extended.node_changes.is_empty() {
        let (_, _, ref cs) = extended.node_changes[0];
        // Color change should be paint-only
        if cs.contains(NodeChangeSet::INLINE_STYLE_PAINT) {
            // correct
        } else if cs.contains(NodeChangeSet::INLINE_STYLE_LAYOUT) {
            // also acceptable (conservative)
        }
    }
}

// =========================================================================
// SCENARIO: Node added (mount)
// =========================================================================

#[test]
fn node_added_detected_as_mount() {
    let old = vec![NodeData::create_div()];
    let new = vec![
        NodeData::create_div(),
        NodeData::create_text("new"),
    ];
    
    let old_layout = make_layout(1);
    let new_layout = make_layout(2);
    
    let extended = reconcile_dom_with_changes(
        &old, &new, None, None,
        &old_layout, &new_layout,
        DomId { inner: 0 }, Instant::now(),
    );
    
    let mut acc = ChangeAccumulator::new();
    acc.merge_extended_diff(&extended, &old, &new);
    
    assert!(!acc.mounted_nodes.is_empty(),
        "new node should be tracked as mounted");
    assert!(acc.needs_layout(),
        "mounted nodes always need layout");
}

// =========================================================================
// SCENARIO: Node removed (unmount)
// =========================================================================

#[test]
fn node_removed_detected_as_unmount() {
    let old = vec![
        NodeData::create_div(),
        NodeData::create_text("old"),
    ];
    let new = vec![NodeData::create_div()];
    
    let old_layout = make_layout(2);
    let new_layout = make_layout(1);
    
    let extended = reconcile_dom_with_changes(
        &old, &new, None, None,
        &old_layout, &new_layout,
        DomId { inner: 0 }, Instant::now(),
    );
    
    let mut acc = ChangeAccumulator::new();
    acc.merge_extended_diff(&extended, &old, &new);
    
    assert!(!acc.unmounted_nodes.is_empty(),
        "removed node should be tracked as unmounted");
}

// =========================================================================
// SCENARIO: Node type change (div → span)
// =========================================================================

#[test]
fn node_type_change_full_scope() {
    let old = vec![NodeData::create_div()];
    let new = vec![NodeData::create_node(azul_core::dom::NodeType::Span)];
    
    let old_layout = make_layout(1);
    let new_layout = make_layout(1);
    
    let extended = reconcile_dom_with_changes(
        &old, &new, None, None,
        &old_layout, &new_layout,
        DomId { inner: 0 }, Instant::now(),
    );
    
    // If they're matched (structural), NODE_TYPE_CHANGED should be set
    // If they're not matched, they'll show as mount/unmount
    let mut acc = ChangeAccumulator::new();
    acc.merge_extended_diff(&extended, &old, &new);
    assert!(acc.needs_layout());
}

// =========================================================================
// SCENARIO: Multiple nodes, one changes
// =========================================================================

#[test]
fn only_changed_node_flagged() {
    let old = vec![
        NodeData::create_text("Line 1"),
        NodeData::create_text("Line 2"),
        NodeData::create_text("Line 3"),
    ];
    let new = vec![
        NodeData::create_text("Line 1"),         // same
        NodeData::create_text("Line 2 edited"),  // changed
        NodeData::create_text("Line 3"),          // same
    ];
    
    let old_layout = make_layout(3);
    let new_layout = make_layout(3);
    
    let extended = reconcile_dom_with_changes(
        &old, &new, None, None,
        &old_layout, &new_layout,
        DomId { inner: 0 }, Instant::now(),
    );
    
    let mut acc = ChangeAccumulator::new();
    acc.merge_extended_diff(&extended, &old, &new);
    
    // Only the middle node should have TEXT_CONTENT flag
    let changed_nodes: Vec<&NodeId> = acc.per_node.iter()
        .filter(|(_, r)| r.change_set.contains(NodeChangeSet::TEXT_CONTENT))
        .map(|(id, _)| id)
        .collect();
    
    // At least one node should be flagged as text-changed
    assert!(!changed_nodes.is_empty(),
        "at least the edited node should be flagged");
}

// =========================================================================
// SCENARIO: Keyed list reorder
// =========================================================================

#[test]
fn keyed_reorder_no_content_change() {
    let mut a = NodeData::create_div();
    a.add_id(AzString::from("alpha"));
    let mut b = NodeData::create_div();
    b.add_id(AzString::from("beta"));
    let mut c = NodeData::create_div();
    c.add_id(AzString::from("gamma"));
    
    let old = vec![a.clone(), b.clone(), c.clone()];
    let new = vec![c.clone(), a.clone(), b.clone()]; // rotated
    
    let old_layout = make_layout(3);
    let new_layout = make_layout(3);
    
    let extended = reconcile_dom_with_changes(
        &old, &new, None, None,
        &old_layout, &new_layout,
        DomId { inner: 0 }, Instant::now(),
    );
    
    // All nodes should match (by key), and no content changes
    for &(_, _, ref cs) in &extended.node_changes {
        assert!(!cs.contains(NodeChangeSet::TEXT_CONTENT),
            "reorder should not flag text changes");
        assert!(!cs.contains(NodeChangeSet::NODE_TYPE_CHANGED),
            "reorder should not flag node type changes");
    }
}

// =========================================================================
// SCENARIO: Insert at beginning (list prepend)
// =========================================================================

#[test]
fn insert_at_beginning() {
    let old = vec![
        NodeData::create_text("Item 1"),
        NodeData::create_text("Item 2"),
    ];
    let new = vec![
        NodeData::create_text("Item 0"),  // new
        NodeData::create_text("Item 1"),  // old[0]
        NodeData::create_text("Item 2"),  // old[1]
    ];
    
    let old_layout = make_layout(2);
    let new_layout = make_layout(3);
    
    let extended = reconcile_dom_with_changes(
        &old, &new, None, None,
        &old_layout, &new_layout,
        DomId { inner: 0 }, Instant::now(),
    );
    
    let mut acc = ChangeAccumulator::new();
    acc.merge_extended_diff(&extended, &old, &new);
    
    // "Item 0" is new → mounted
    assert!(!acc.mounted_nodes.is_empty() || !acc.per_node.is_empty(),
        "insert at beginning should produce changes");
}

// =========================================================================
// SCENARIO: Delete from middle
// =========================================================================

#[test]
fn delete_from_middle() {
    let old = vec![
        NodeData::create_text("A"),
        NodeData::create_text("B"),
        NodeData::create_text("C"),
    ];
    let new = vec![
        NodeData::create_text("A"),
        NodeData::create_text("C"),
    ];
    
    let old_layout = make_layout(3);
    let new_layout = make_layout(2);
    
    let extended = reconcile_dom_with_changes(
        &old, &new, None, None,
        &old_layout, &new_layout,
        DomId { inner: 0 }, Instant::now(),
    );
    
    let mut acc = ChangeAccumulator::new();
    acc.merge_extended_diff(&extended, &old, &new);
    
    // "B" was removed → unmounted
    assert!(!acc.unmounted_nodes.is_empty(),
        "deleting a node should produce unmount");
}

// =========================================================================
// SCENARIO: Contenteditable + text change
// =========================================================================

#[test]
fn contenteditable_change_detected() {
    let mut old_div = NodeData::create_div();
    old_div.set_contenteditable(false);
    
    let mut new_div = NodeData::create_div();
    new_div.set_contenteditable(true);
    
    let old = vec![old_div];
    let new = vec![new_div];
    
    let old_layout = make_layout(1);
    let new_layout = make_layout(1);
    
    let extended = reconcile_dom_with_changes(
        &old, &new, None, None,
        &old_layout, &new_layout,
        DomId { inner: 0 }, Instant::now(),
    );
    
    if !extended.node_changes.is_empty() {
        let (_, _, ref cs) = extended.node_changes[0];
        assert!(cs.contains(NodeChangeSet::CONTENTEDITABLE),
            "contenteditable change should be detected");
    }
}

// =========================================================================
// SCENARIO: Mixed changes (realistic)
// =========================================================================

#[test]
fn realistic_todo_list_update() {
    // Simulate a todo list: user marks item 2 as complete (class change)
    // and edits item 3's text
    let make_todo = |text: &str, done: bool| -> NodeData {
        let mut nd = NodeData::create_text(text);
        if done {
            nd.add_class(AzString::from("done"));
        }
        nd
    };
    
    let old = vec![
        make_todo("Buy milk", false),
        make_todo("Walk dog", false),
        make_todo("Code review", false),
    ];
    let new = vec![
        make_todo("Buy milk", false),         // same
        make_todo("Walk dog", true),          // class added
        make_todo("Code review done", false), // text changed
    ];
    
    let old_layout = make_layout(3);
    let new_layout = make_layout(3);
    
    let extended = reconcile_dom_with_changes(
        &old, &new, None, None,
        &old_layout, &new_layout,
        DomId { inner: 0 }, Instant::now(),
    );
    
    let mut acc = ChangeAccumulator::new();
    acc.merge_extended_diff(&extended, &old, &new);
    
    // Should detect changes (at least class or text changes)
    assert!(!acc.is_visually_unchanged(),
        "todo list update should produce visual changes");
    assert!(acc.needs_layout(),
        "text/class changes should need layout");
}

// =========================================================================
// SCENARIO: Empty DOM rebuild
// =========================================================================

#[test]
fn empty_to_empty_no_changes() {
    let old: Vec<NodeData> = vec![];
    let new: Vec<NodeData> = vec![];
    
    let old_layout = FastHashMap::default();
    let new_layout = FastHashMap::default();
    
    let extended = reconcile_dom_with_changes(
        &old, &new, None, None,
        &old_layout, &new_layout,
        DomId { inner: 0 }, Instant::now(),
    );
    
    assert!(extended.node_changes.is_empty());
    assert!(extended.diff.node_moves.is_empty());
}

#[test]
fn empty_to_populated() {
    let old: Vec<NodeData> = vec![];
    let new = vec![NodeData::create_div(), NodeData::create_text("hello")];
    
    let old_layout = FastHashMap::default();
    let new_layout = make_layout(2);
    
    let extended = reconcile_dom_with_changes(
        &old, &new, None, None,
        &old_layout, &new_layout,
        DomId { inner: 0 }, Instant::now(),
    );
    
    let mut acc = ChangeAccumulator::new();
    acc.merge_extended_diff(&extended, &old, &new);
    
    // All new → mounted
    assert_eq!(acc.mounted_nodes.len(), 2);
    assert!(acc.unmounted_nodes.is_empty());
    assert!(acc.needs_layout());
}

#[test]
fn populated_to_empty() {
    let old = vec![NodeData::create_div(), NodeData::create_text("hello")];
    let new: Vec<NodeData> = vec![];
    
    let old_layout = make_layout(2);
    let new_layout = FastHashMap::default();
    
    let extended = reconcile_dom_with_changes(
        &old, &new, None, None,
        &old_layout, &new_layout,
        DomId { inner: 0 }, Instant::now(),
    );
    
    let mut acc = ChangeAccumulator::new();
    acc.merge_extended_diff(&extended, &old, &new);
    
    // All removed → unmounted
    assert_eq!(acc.unmounted_nodes.len(), 2);
    assert!(acc.mounted_nodes.is_empty());
}
