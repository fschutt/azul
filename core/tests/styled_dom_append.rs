//! StyledDom Append Child Tests
//!
//! Tests for the append_child functionality that verify no index underflow occurs.
//! The critical invariant is that node hierarchy fields use 0 = None encoding:
//! - parent, previous_sibling, next_sibling, last_child all use 1-based encoding
//! - When these fields are 0, it means "no node" (None)
//! - When > 0, the actual NodeId index is (value - 1)

use azul_core::dom::Dom;
use azul_css::css::Css;

fn empty_css() -> Css {
    Css::empty()
}

// ==================== Basic Append Tests ====================

#[test]
fn test_append_single_child() {
    // Create a parent with one existing child
    let parent = Dom::create_div()
        .with_child(Dom::create_text("existing"))
        .style(empty_css());
    
    let initial_len = parent.node_hierarchy.as_ref().len();
    
    // Create child to append
    let child = Dom::create_text("appended").style(empty_css());
    let child_len = child.node_hierarchy.as_ref().len();
    
    let mut combined = parent;
    combined.append_child(child);
    
    // Verify lengths are correct
    assert_eq!(
        combined.node_hierarchy.as_ref().len(), 
        initial_len + child_len,
        "Combined length should be parent + child"
    );
}

#[test]
fn test_append_multiple_children() {
    let mut parent = Dom::create_div().style(empty_css());
    
    // Append three children
    parent.append_child(Dom::create_text("first").style(empty_css()));
    parent.append_child(Dom::create_text("second").style(empty_css()));
    parent.append_child(Dom::create_text("third").style(empty_css()));
    
    // Parent (1) + 3 children = 4 nodes
    assert_eq!(parent.node_hierarchy.as_ref().len(), 4);
}

#[test]
fn test_append_nested_child() {
    let parent = Dom::create_div().style(empty_css());
    
    // Create a nested child structure
    let nested = Dom::create_div()
        .with_child(Dom::create_div()
            .with_child(Dom::create_text("deeply nested")))
        .style(empty_css());
    
    let mut combined = parent;
    combined.append_child(nested);
    
    // All node indices should be valid
    for (i, node) in combined.node_hierarchy.as_ref().iter().enumerate() {
        // Verify parent index is valid if present
        if let Some(parent_id) = node.parent_id() {
            assert!(
                parent_id.index() < combined.node_hierarchy.as_ref().len(),
                "Node {} has invalid parent index {} (len={})",
                i, parent_id.index(), combined.node_hierarchy.as_ref().len()
            );
        }
        
        // Verify last_child index is valid if present
        if let Some(last_child_id) = node.last_child_id() {
            assert!(
                last_child_id.index() < combined.node_hierarchy.as_ref().len(),
                "Node {} has invalid last_child index {} (len={})",
                i, last_child_id.index(), combined.node_hierarchy.as_ref().len()
            );
        }
        
        // Verify previous_sibling index is valid if present
        if let Some(prev_id) = node.previous_sibling_id() {
            assert!(
                prev_id.index() < combined.node_hierarchy.as_ref().len(),
                "Node {} has invalid previous_sibling index {} (len={})",
                i, prev_id.index(), combined.node_hierarchy.as_ref().len()
            );
        }
        
        // Verify next_sibling index is valid if present
        if let Some(next_id) = node.next_sibling_id() {
            assert!(
                next_id.index() < combined.node_hierarchy.as_ref().len(),
                "Node {} has invalid next_sibling index {} (len={})",
                i, next_id.index(), combined.node_hierarchy.as_ref().len()
            );
        }
    }
}

// ==================== Underflow Prevention Tests ====================

#[test]
fn test_no_underflow_on_empty_parent() {
    // Edge case: parent has no children yet
    let parent = Dom::create_div().style(empty_css());
    let child = Dom::create_text("child").style(empty_css());
    
    let mut combined = parent;
    combined.append_child(child);
    
    // Should not panic and all indices should be valid
    for (i, node) in combined.node_hierarchy.as_ref().iter().enumerate() {
        if let Some(last_child_id) = node.last_child_id() {
            assert!(
                last_child_id.index() < combined.node_hierarchy.as_ref().len(),
                "Node {} last_child underflow: index {} >= len {}",
                i, last_child_id.index(), combined.node_hierarchy.as_ref().len()
            );
        }
    }
}

#[test]
fn test_hierarchy_fields_use_one_based_encoding() {
    // This test verifies the encoding invariant directly
    let parent = Dom::create_div()
        .with_child(Dom::create_text("child"))
        .style(empty_css());
    
    // The root should have a last_child
    let root = parent.root.into_crate_internal().expect("should have root");
    let root_node = &parent.node_hierarchy.as_ref()[root.index()];
    
    // last_child should be > 0 (meaning Some in 1-based encoding)
    // If it were 0, that would mean None (no children), which is wrong
    assert!(
        root_node.last_child != 0,
        "Root with children should have last_child != 0"
    );
    
    // The child's parent field should also be > 0
    // If parent were 0, it would mean no parent (orphan), which is wrong
    if let Some(child_id) = root_node.last_child_id() {
        let child_node = &parent.node_hierarchy.as_ref()[child_id.index()];
        assert!(
            child_node.parent != 0,
            "Child should have parent != 0"
        );
    }
}

#[test]
fn test_none_fields_are_zero() {
    // A leaf node (text) should have last_child = 0 (None)
    let leaf = Dom::create_text("leaf").style(empty_css());
    
    let root = leaf.root.into_crate_internal().expect("should have root");
    let root_node = &leaf.node_hierarchy.as_ref()[root.index()];
    
    // Leaf node has no children, so last_child should be 0 (None)
    assert_eq!(
        root_node.last_child, 0,
        "Leaf node should have last_child = 0 (None)"
    );
    
    // Root has no parent, so parent should be 0 (None)
    assert_eq!(
        root_node.parent, 0,
        "Root node should have parent = 0 (None)"
    );
}

// ==================== Regression Tests ====================

#[test]
fn test_regression_index_2_with_len_2() {
    // This reproduces the original bug:
    // "index out of bounds: the len is 2 but the index is 2"
    // 
    // The bug occurred when last_child was set incorrectly,
    // causing an attempt to access index 2 in a 2-element array (indices 0, 1)
    
    // Create a DOM with 2 nodes: root + 1 child
    let parent = Dom::create_div()
        .with_child(Dom::create_text("child"))
        .style(empty_css());
    
    assert_eq!(parent.node_hierarchy.as_ref().len(), 2);
    
    // Now append another child
    let child2 = Dom::create_text("child2").style(empty_css());
    
    let mut combined = parent;
    combined.append_child(child2);
    
    // After append, len should be 3
    assert_eq!(combined.node_hierarchy.as_ref().len(), 3);
    
    // All last_child pointers should be valid
    for (i, node) in combined.node_hierarchy.as_ref().iter().enumerate() {
        if let Some(lc) = node.last_child_id() {
            assert!(
                lc.index() < combined.node_hierarchy.as_ref().len(),
                "BUG REGRESSION: Node {} has last_child index {} but len is {}",
                i, lc.index(), combined.node_hierarchy.as_ref().len()
            );
        }
    }
}

#[test]
fn test_xml_like_append_sequence() {
    // Simulates what XML parsing does: creates body, then appends children one by one
    let mut body = Dom::create_body().style(empty_css());
    
    // Append several children like XML parser would do
    for i in 0..5 {
        let child = Dom::create_div()
            .with_child(Dom::create_text(format!("text {}", i)))
            .style(empty_css());
        body.append_child(child);
    }
    
    // Verify all indices are valid
    let len = body.node_hierarchy.as_ref().len();
    for (i, node) in body.node_hierarchy.as_ref().iter().enumerate() {
        if let Some(id) = node.parent_id() {
            assert!(id.index() < len, "Node {} parent invalid", i);
        }
        if let Some(id) = node.last_child_id() {
            assert!(id.index() < len, "Node {} last_child invalid", i);
        }
        if let Some(id) = node.previous_sibling_id() {
            assert!(id.index() < len, "Node {} previous_sibling invalid", i);
        }
        if let Some(id) = node.next_sibling_id() {
            assert!(id.index() < len, "Node {} next_sibling invalid", i);
        }
    }
}
