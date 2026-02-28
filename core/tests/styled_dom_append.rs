//! StyledDom Append Child Tests
//!
//! Tests for the append_child functionality that verify no index underflow occurs.
//! The critical invariant is that node hierarchy fields use 0 = None encoding:
//! - parent, previous_sibling, next_sibling, last_child all use 1-based encoding
//! - When these fields are 0, it means "no node" (None)
//! - When > 0, the actual NodeId index is (value - 1)

use azul_core::dom::Dom;
use azul_core::styled_dom::StyledDom;
use azul_css::css::Css;

fn empty_css() -> Css {
    Css::empty()
}

fn style_dom(mut dom: Dom) -> StyledDom {
    StyledDom::create(&mut dom, empty_css())
}

// ==================== Basic Append Tests ====================

#[test]
fn test_append_single_child() {
    let parent = style_dom(Dom::create_div()
        .with_child(Dom::create_text("existing")));

    let initial_len = parent.node_hierarchy.as_ref().len();

    let child = style_dom(Dom::create_text("appended"));
    let child_len = child.node_hierarchy.as_ref().len();

    let mut combined = parent;
    combined.append_child(child);

    assert_eq!(
        combined.node_hierarchy.as_ref().len(),
        initial_len + child_len,
        "Combined length should be parent + child"
    );
}

#[test]
fn test_append_multiple_children() {
    let mut parent = style_dom(Dom::create_div());

    parent.append_child(style_dom(Dom::create_text("first")));
    parent.append_child(style_dom(Dom::create_text("second")));
    parent.append_child(style_dom(Dom::create_text("third")));

    assert_eq!(parent.node_hierarchy.as_ref().len(), 4);
}

#[test]
fn test_append_nested_child() {
    let parent = style_dom(Dom::create_div());

    let nested = style_dom(Dom::create_div()
        .with_child(Dom::create_div().with_child(Dom::create_text("deeply nested"))));

    let mut combined = parent;
    combined.append_child(nested);

    for (i, node) in combined.node_hierarchy.as_ref().iter().enumerate() {
        if let Some(parent_id) = node.parent_id() {
            assert!(
                parent_id.index() < combined.node_hierarchy.as_ref().len(),
                "Node {} has invalid parent index {} (len={})",
                i, parent_id.index(), combined.node_hierarchy.as_ref().len()
            );
        }
        if let Some(last_child_id) = node.last_child_id() {
            assert!(
                last_child_id.index() < combined.node_hierarchy.as_ref().len(),
                "Node {} has invalid last_child index {} (len={})",
                i, last_child_id.index(), combined.node_hierarchy.as_ref().len()
            );
        }
        if let Some(prev_id) = node.previous_sibling_id() {
            assert!(
                prev_id.index() < combined.node_hierarchy.as_ref().len(),
                "Node {} has invalid previous_sibling index {} (len={})",
                i, prev_id.index(), combined.node_hierarchy.as_ref().len()
            );
        }
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
    let parent = style_dom(Dom::create_div());
    let child = style_dom(Dom::create_text("child"));

    let mut combined = parent;
    combined.append_child(child);

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
    let parent = style_dom(Dom::create_div()
        .with_child(Dom::create_text("child")));

    let root = parent.root.into_crate_internal().expect("should have root");
    let root_node = &parent.node_hierarchy.as_ref()[root.index()];

    assert!(
        root_node.last_child != 0,
        "Root with children should have last_child != 0"
    );

    if let Some(child_id) = root_node.last_child_id() {
        let child_node = &parent.node_hierarchy.as_ref()[child_id.index()];
        assert!(child_node.parent != 0, "Child should have parent != 0");
    }
}

#[test]
fn test_none_fields_are_zero() {
    let leaf = style_dom(Dom::create_text("leaf"));

    let root = leaf.root.into_crate_internal().expect("should have root");
    let root_node = &leaf.node_hierarchy.as_ref()[root.index()];

    assert_eq!(root_node.last_child, 0, "Leaf node should have last_child = 0 (None)");
    assert_eq!(root_node.parent, 0, "Root node should have parent = 0 (None)");
}

// ==================== Regression Tests ====================

#[test]
fn test_regression_index_2_with_len_2() {
    let parent = style_dom(Dom::create_div()
        .with_child(Dom::create_text("child")));

    assert_eq!(parent.node_hierarchy.as_ref().len(), 2);

    let child2 = style_dom(Dom::create_text("child2"));

    let mut combined = parent;
    combined.append_child(child2);

    assert_eq!(combined.node_hierarchy.as_ref().len(), 3);

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
    let mut body = style_dom(Dom::create_body());

    for i in 0..5 {
        let child = style_dom(Dom::create_div()
            .with_child(Dom::create_text(format!("text {}", i))));
        body.append_child(child);
    }

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
