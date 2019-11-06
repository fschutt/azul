use std::{
    fmt,
    collections::BTreeSet,
};
use crate::{
    id_tree::{NodeId, NodeDataContainer},
    dom::{CompactDom, NodeData},
};

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DomRange {
    pub start: NodeId,
    pub end: NodeId,
}

impl DomRange {

    pub fn new(start: NodeId, end: NodeId) -> Self {
        Self { start, end }
    }

    pub fn single_node(node_id: NodeId) -> Self {
        Self {
            start: node_id,
            end: node_id,
        }
    }
}

impl fmt::Debug for DomRange {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

impl fmt::Display for DomRange {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}..{}", self.start, self.end)
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DomChange {
    /// Node is present on the new DOM, but not on the old one = add node
    Added(DomRange),
    /// Node is present on the old DOM, but not on the new one = remove node
    Removed(DomRange),
}

impl fmt::Display for DomChange {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::DomChange::*;
        match self {
            Added(c) => write!(f, "+ {}", c),
            Removed(c) => write!(f, "- {}", c),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DomDiff {
    /// What the actual changes nodes (not trees / subtrees) were in this diff, in order of appearance
    pub changed_nodes: Vec<DomChange>,
}

impl DomDiff {
    pub fn new<T>(old: &CompactDom<T>, new: &CompactDom<T>) -> Self {

        // TODO: Check if old root = new root, if not, change entire tree

        let mut changes = BTreeSet::new();
        let mut visited_nodes = NodeDataContainer::new(vec![false; new.len()]);

        visited_nodes[NodeId::ZERO] = true;

        let has_root_changed = node_has_changed(
            &old.arena.node_data[NodeId::ZERO],
            &new.arena.node_data[NodeId::ZERO]
        );

        if has_root_changed == NODE_CHANGED_NOTHING {

            diff_tree_inner(NodeId::ZERO, old, new, &mut changes, &mut visited_nodes);
            add_visited_nodes(visited_nodes, &mut changes);

            Self {
                changed_nodes: optimize_changeset(changes)
            }

        } else {

            // Root changed = everything changed
            changes.insert(DomChange::Removed(DomRange {
                start: NodeId::ZERO,
                end: NodeId::new(old.len() - 1)
            }));

            changes.insert(DomChange::Added(DomRange {
                start: NodeId::ZERO,
                end: NodeId::new(new.len() - 1)
            }));

            Self {
                changed_nodes: optimize_changeset(changes)
            }
        }
    }

    /// Formats the diff into a git-like `+ Node1 / - Node3` form
    pub fn format_nicely<T>(&self, old: &CompactDom<T>, new: &CompactDom<T>) -> String {
        use self::DomChange::*;
        self.changed_nodes.iter().map(|change| {
            match change {
                Added(c) => format!("+\t{}", new.arena.node_data[c.start]),
                Removed(c) => format!("-\t{}", old.arena.node_data[c.start]),
            }
        }).collect::<Vec<String>>().join("\r\n")
    }
}

impl fmt::Display for DomDiff {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for c in self.changed_nodes.iter() {
            write!(f, "{}", c)?;
        }
        Ok(())
    }
}

impl DomRange {

    /// Is `other` a subtree of `self`? - Assumes that the DOM was
    /// constructed in a linear order, i.e. the child being within
    /// the parents start / end bounds
    pub fn contains(&self, other: &Self) -> bool {
        other.start.index() >= self.start.index() &&
        other.end.index() <= self.end.index()
    }

    /// Compares two DOM ranges *without* looking at the DOM hashes (not equivalent to `==`)
    pub fn equals_range(&self, other: &Self) -> bool {
        other.start == self.start &&
        other.end == self.end
    }
}

const NODE_CHANGED_NOTHING: u8  = 0x01;
const NODE_CHANGED_TYPE: u8     = 0x02;
const NODE_CHANGED_CLASSES: u8  = 0x04;
const NODE_CHANGED_IDS: u8      = 0x08;

// In order to test two DOM nodes for "equality", you'd need to
// test if the node type, the classes and the ids are the same.
// The rest of the attributes can be ignored, since they are not
// used by the CSS engine.
//
// NOTE: The callbacks / etc. need to be changed!
#[inline]
fn node_has_changed<T>(old: &NodeData<T>, new: &NodeData<T>) -> u8 {
    let mut result = NODE_CHANGED_NOTHING;

    if old.get_node_type() != new.get_node_type() {
        result &= NODE_CHANGED_TYPE;
    }

    if old.get_classes() != new.get_classes() {
        result &= NODE_CHANGED_CLASSES;
    }

    if old.get_ids() != new.get_ids() {
        result &= NODE_CHANGED_IDS;
    }

    result
}

fn diff_tree_inner<T>(
    old_root_id: NodeId,
    old: &CompactDom<T>,
    new: &CompactDom<T>,
    changes: &mut BTreeSet<DomChange>,
    visited_nodes: &mut NodeDataContainer<bool>,
) {
    let mut node_shift = 0_isize;

    for old_node_id in old_root_id.children(&old.arena.node_hierarchy) {

        // Node ID that corresponds to the same node in the new node tree
        let new_node_id = NodeId::new((old_node_id.index() as isize + node_shift) as usize);

        let old_node_last_child = NodeId::new(match old.arena.node_hierarchy[old_node_id].next_sibling {
            Some(s) => s.index(),
            None => old.arena.node_hierarchy.len() - 1,
        });

        match new.arena.node_data.get(new_node_id) {
            None => {
                // Couldn't find the new node in the new tree, old tree has more children than new
                changes.insert(DomChange::Removed(DomRange {
                    start: old_node_id,
                    end: old_node_last_child,
                }));
            },
            Some(new_node_data) => {

                visited_nodes[new_node_id] = true;

                let old_node_data = &old.arena.node_data[old_node_id];
                let compare_nodes = node_has_changed(old_node_data, new_node_data);

                if compare_nodes == NODE_CHANGED_NOTHING {
                    diff_tree_inner(old_node_id, old, new, changes, visited_nodes);
                } else {

                    let new_node_subtree_len = new.arena.node_hierarchy.subtree_len(new_node_id);

                    let next_node_id = match new.arena.node_hierarchy[new_node_id].next_sibling {
                        Some(s) => s.index(),
                        None => new.arena.node_hierarchy.len() - 1,
                    };

                    // remove entire old subtree, including the node itself
                    changes.insert(DomChange::Removed(DomRange {
                        start: old_node_id,
                        end: old_node_last_child,
                    }));

                    // add entire new subtree, including the node itself
                    changes.insert(DomChange::Added(DomRange {
                        start: new_node_id,
                        end: NodeId::new(next_node_id),
                    }));

                    node_shift += new_node_subtree_len as isize;

                    for n in new_node_id.index()..next_node_id {
                        visited_nodes[NodeId::new(n)] = true;
                    }
                }
            }
        }
    }
}

fn add_visited_nodes(
    visited_nodes: NodeDataContainer<bool>,
    changes: &mut BTreeSet<DomChange>,
) {
    changes.extend(
        visited_nodes
        .linear_iter()
        .filter_map(|node_id| if visited_nodes[node_id] { None } else { Some(node_id) })
        .map(|node_id| DomChange::Added(DomRange::single_node(node_id)))
    );
}

fn optimize_changeset(changes: BTreeSet<DomChange>) -> Vec<DomChange> {
    // TODO: optimize changeset into larger chunks!
    changes.into_iter().collect()
}
