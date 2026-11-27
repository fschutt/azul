//! The ids a text entry of the overlay holds: its key, and the text node of every run. They are those of
//! the DOM the edit was typed in; a rebuilt DOM renumbers its nodes, so they follow the remap of the
//! reconciliation, and one that did not follow (a page that shrank before a typed field: E22, the layout
//! pass indexed a node one past the end of the DOM) is made harmless before the display list reads it.

use std::sync::Arc;

use azul_core::dom::{DomId, NodeId};

use super::*;
use crate::managers::{NodeIdMap, NodeIdRemap};

fn dom(n: usize) -> DomId {
    DomId { inner: n }
}

fn run(text: &str, source: Option<usize>) -> StyledRun {
    StyledRun {
        text: Arc::from(text),
        style: Arc::new(Default::default()),
        logical_start_byte: 0,
        source_node_id: source.map(NodeId::new),
    }
}

fn text(text: &str, source: Option<usize>) -> InlineContent {
    InlineContent::Text(run(text, source))
}

fn entry(content: Vec<InlineContent>) -> DirtyTextNode {
    DirtyTextNode {
        content,
        cursor: None,
        needs_ancestor_relayout: false,
        revision: 1,
        typed_over: None,
    }
}

/// The text nodes the runs of an entry name, in order (markers and rubies' runs included).
fn sources(overlay: &ContentOverlay, dom_id: DomId, node: usize) -> Vec<Option<usize>> {
    let mut named = Vec::new();
    let mut content = overlay
        .text_for_node(dom_id, NodeId::new(node))
        .expect("the entry is there")
        .content
        .clone();
    for_each_run_mut(&mut content, &mut |run| named.push(run.source_node_id.map(|node| node.index())));
    named
}

#[test]
fn the_runs_of_an_edit_follow_their_text_nodes_through_a_remap() {
    let mut overlay = ContentOverlay::default();
    overlay.set_text(
        dom(0),
        NodeId::new(3),
        entry(vec![text("Ada", Some(5)), text("x", Some(5)), text("gone", Some(9)), text("g", None)]),
    );

    // A page that lost a node before the field: 3 became 2, its text node 5 became 4, 9 is gone.
    let map = NodeIdMap::from_pairs([(NodeId::new(3), NodeId::new(2)), (NodeId::new(5), NodeId::new(4))]);
    overlay.remap_node_ids(dom(0), &map);

    assert!(overlay.text_for_node(dom(0), NodeId::new(3)).is_none(), "the key moved");
    assert_eq!(
        sources(&overlay, dom(0), 2),
        vec![Some(4), Some(4), None, None],
        "the runs follow their nodes; a run whose node is gone names none"
    );
}

#[test]
fn the_runs_of_a_marker_and_of_a_ruby_follow_their_nodes_too() {
    let mut overlay = ContentOverlay::default();
    overlay.set_text(
        dom(0),
        NodeId::new(1),
        entry(vec![
            InlineContent::Marker { run: run("*", Some(6)), position_outside: true },
            InlineContent::Ruby {
                base: vec![text("base", Some(7))],
                text: vec![text("ruby", Some(8))],
                style: Arc::new(Default::default()),
            },
        ]),
    );
    let map = NodeIdMap::from_pairs([
        (NodeId::new(1), NodeId::new(1)),
        (NodeId::new(6), NodeId::new(2)),
        (NodeId::new(7), NodeId::new(3)),
    ]);
    overlay.remap_node_ids(dom(0), &map);
    assert_eq!(sources(&overlay, dom(0), 1), vec![Some(2), Some(3), None]);
}

#[test]
fn the_remap_of_one_dom_leaves_the_runs_of_another_alone() {
    let mut overlay = ContentOverlay::default();
    overlay.set_text(dom(1), NodeId::new(3), entry(vec![text("Ada", Some(5))]));
    let map = NodeIdMap::from_pairs([(NodeId::new(5), NodeId::new(4))]);
    overlay.remap_node_ids(dom(0), &map);
    assert_eq!(sources(&overlay, dom(1), 3), vec![Some(5)]);
}

#[test]
fn an_entry_outside_the_dom_is_dropped_and_a_run_outside_it_names_no_node() {
    let mut overlay = ContentOverlay::default();
    // A DOM of 5 nodes (0..5): the field 3 is in it; a text node 5 and a node 900 are not.
    overlay.set_text(dom(0), NodeId::new(3), entry(vec![text("a", Some(4)), text("b", Some(5)), text("c", Some(900))]));
    overlay.set_text(dom(0), NodeId::new(7), entry(vec![text("d", Some(4))]));
    // Another DOM, with ids of its own, is not this one's business.
    overlay.set_text(dom(1), NodeId::new(7), entry(vec![text("e", Some(800))]));

    let dropped = overlay.drop_text_outside(dom(0), 5);

    assert_eq!(dropped, vec![NodeId::new(7)], "the entry of a node the DOM does not have is dropped");
    assert!(overlay.text_for_node(dom(0), NodeId::new(7)).is_none());
    assert_eq!(sources(&overlay, dom(0), 3), vec![Some(4), None, None]);
    assert_eq!(sources(&overlay, dom(1), 7), vec![Some(800)]);
}

#[test]
fn nothing_is_dropped_from_a_dom_that_holds_all_the_ids() {
    let mut overlay = ContentOverlay::default();
    overlay.set_text(dom(0), NodeId::new(3), entry(vec![text("a", Some(4))]));
    assert!(overlay.drop_text_outside(dom(0), 5).is_empty());
    assert_eq!(sources(&overlay, dom(0), 3), vec![Some(4)]);
}
