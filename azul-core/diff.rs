#![allow(unused_variables)]
#![allow(dead_code)]

use std::{collections::BTreeMap, marker::PhantomData};
use {
    id_tree::{NodeId, NodeHierarchy},
    dom::{Dom, NodeData},
};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct DomRange<F: FrameMarker> {
    pub start: DomNode<F>,
    pub end: DomNode<F>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct OldState { }

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct NewState { }

pub(crate) trait FrameMarker { }

impl FrameMarker for OldState { }
impl FrameMarker for NewState { }

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct DomNode<F: FrameMarker> {
    pub(crate) id: NodeId,
    pub(crate) marker: PhantomData<F>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum DomChange {
    Added(DomRange<NewState>),
    Removed(DomRange<OldState>),
}

#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DomDiff {
    /// What the actual changes nodes (not trees / subtrees) were in this diff, in order of appearance
    pub(crate) changed_nodes: Vec<DomChange>,
    /// Which items simply need updating in terms of image source?
    pub(crate) only_replace_images: Vec<NodeId>,
    /// Which nodes / subtrees need re-styling?
    pub(crate) need_restyling: Vec<DomRange<NewState>>,
    /// Which nodes need a re-layout?
    /// For example A would be:
    pub(crate) need_relayout: Vec<DomRange<NewState>>,
}

type TreeDepth = usize;
type ParentNodeId = NodeId;
type LeafNodeId = NodeId;

impl<F: FrameMarker + PartialEq> DomRange<F> {

    /// Is `other` a subtree of `self`? - Assumes that the DOM was
    /// constructed in a linear order, i.e. the child being within
    /// the parents start / end bounds
    pub fn contains(&self, other: &Self) -> bool {
        other.start.id.index() >= self.start.id.index() &&
        other.end.id.index() <= self.end.id.index()
    }

    /// Compares two DOM ranges *without* looking at the DOM hashes (not equivalent to `==`)
    pub fn equals_range(&self, other: &Self) -> bool {
        other.start == self.start &&
        other.end == self.end
    }
}

// In order to test two DOM nodes for "equality", you'd need to
// test if the node type, the classes and the ids are the same.
// The rest of the attributes can be ignored, since they are not
// used by the CSS engine.
//
// Right now the CSS doesn't support adjacent modifiers ("+" selectors),
// which will make this algorithm a bit more complex (but should be
// solvable with adjacency lists).
//
// for each leaf node (sorted by depth):
//     - if the node has only changed its position:
//         - node doesn't need restyle (but may need re-layout when "+" selectors are implemented)
//     - else insert it it
//     - add the end added / removed nodes
//
// for each parent node (sorted by depth, bubble up):
//     - if the parent has changed:
//
//     - ask if that child has affected that parents layout
//          - if yes, set the parent to be restyled
//     - ask if that child affects its siblings layout
//          - if yes, add the siblings to the changeset
//

const NODE_CHANGED_NOTHING: u8  = 0x01;
const NODE_CHANGED_TYPE: u8     = 0x02;
const NODE_CHANGED_CLASSES: u8  = 0x04;
const NODE_CHANGED_IDS: u8      = 0x08;

fn node_needs_restyle<T>(old: &NodeData<T>, new: &NodeData<T>) -> u8 {
    let mut result = NODE_CHANGED_NOTHING;

    if old.get_node_type() != new.get_node_type() {
        result &= NODE_CHANGED_TYPE;
    }

    if old.get_classes() != new.get_classes() {
        result &= NODE_CHANGED_CLASSES;
    }

    if old.get_ids() != new.get_ids() {
        result &= NODE_CHANGED_CLASSES;
    }

    result
}

fn get_leaf_nodes_by_depth<T: FrameMarker>(hierarchy: &NodeHierarchy)
-> BTreeMap<TreeDepth, BTreeMap<ParentNodeId, Vec<DomNode<T>>>>
{
    let mut map = BTreeMap::new();
    let parent_nodes = hierarchy.get_parents_sorted_by_depth();

    for (parent_depth, parent_id) in parent_nodes {
        for child_id in parent_id.children(hierarchy).filter(|child| hierarchy[*child].first_child.is_some()) {
            map.entry(parent_depth + 1).or_insert_with(|| BTreeMap::new())
               .entry(parent_id).or_insert_with(|| Vec::new())
               .push(DomNode { id: child_id, marker: PhantomData });
        }
    }

    map
}

pub(crate) fn diff_dom_tree<T>(old: &Dom<T>, new:Dom<T>) -> DomDiff {

    // TODO!

    // let old_leaf_nodes = get_leaf_nodes_by_depth(&old.arena.node_layout, OldState { });
    // let new_leaf_nodes = get_leaf_nodes_by_depth(&new.arena.node_layout, NewState { });

    // depth -> parents (in order) -> [leaf children]

    DomDiff::default()
}