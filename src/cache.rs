//! DOM cache for library-internal use
//!
//! # Diffing the DOM
//!
//! Changes in the DOM can happen in three ways:
//!
//! - An element changes its content An element is pushed as a child The order / childs of an element
//! - are restructured
//!
//! In order for the caching to be effective, we need to solve the  problem of only adding
//! EditVariable-s if needed. In order to do that, we need two elements for each DOM node:
//!
//! - The self-hash (the hash of the current DOM node, including hashing the content)
//! - The hashes of the individual children (like a `Vec<DomHash>`), in their correct order
//!
//! For detecting these changes, we build an `Arena<DomHash>` (empty on startup) and a
//! `HashMap<(DomHash, bool) -> LayoutRect>`. The latter stores all active EditVariables.  Whenever we
//! insert or remove from the HashMap, we also remove the variables from the solver
//!
//! When a re-layout is required, we hash the nodes from the UiDescription, starting from the root. Each
//! time we go to the next sibling / next child, this change is also reflected by going through the
//! `Arena<DomHash>`. For each node, we calculate the self-hash of the node and compare it with the hash
//! in that position in the `Arena<DomHash>`. If the hash does not exist in the `Arena<DomHash>`, we
//! insert it in the `HashMap<(DomHash, bool)`, create a new LayoutRect and add the variables to the
//! solver. We set the `bool` to true to indicate, that this hash is currently active in the DOM and
//! should not be removed. Then we add the hash to the `Arena<DomHash>`.
//!
//! If there is a hash, but the hashes differ, this means that either the order of the current siblings
//! were  changed or the actual contents of the node were changed. So we look up the hash in the
//! `HashMap<(DomHash, bool)>`.  If we can find it, this means that we already have EditVariables in the
//! solver corresponding to the node and the node was simply reordered.
//! If we can't find it, it's either a completely new DOM element or the contents of the node have changed.
//!
//! Lastly, we go through the `HashMap<(DomHash, bool)>` and remove the edit variables if the `bool` is false,
//! meaning that the variable was not present in the current DOM tree, so leaving the variables in the solver
//! would be garbage.

use std::collections::BTreeMap;
use constraints::DisplayRect;
use cassowary::Solver;
use id_tree::{NodeId, Arena};
use traits::LayoutScreen;
use dom::NodeData;
use std::ops::Deref;

/// We keep the tree from the previous re-layout. Then, when a re-layout is required,
/// we re-hash all the nodes, insert the
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) struct DomTreeCache {
    pub(crate) previous_layout: HashedDomTree,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DomChangeSet {
    // TODO: calculate the constraints that have to be updated
    pub(crate) added_nodes: BTreeMap<NodeId, DomHash>,
}

impl DomChangeSet {
    pub(crate) fn empty() -> Self {
        Self {
            added_nodes: BTreeMap::new(),
        }
    }
}

impl Deref for DomChangeSet {
    type Target = BTreeMap<NodeId, DomHash>;

    fn deref(&self) -> &Self::Target {
        &self.added_nodes
    }
}

impl DomTreeCache {

    pub(crate) fn empty() -> Self {
        Self {
            previous_layout: HashedDomTree {
                arena: Arena::<DomHash>::new(),
                root: None,
            },
        }
    }

    pub(crate) fn update<T: LayoutScreen>(&mut self, new_root: NodeId, new_nodes_arena: &Arena<NodeData<T>>) -> DomChangeSet {

        use std::hash::Hash;

        if let Some(previous_root) = self.previous_layout.root {
            // let mut changeset = DomChangeSet::empty();
            let new_tree = new_nodes_arena.transform(|data| data.calculate_node_data_hash());
            // Self::update_tree_inner(previous_root, &self.previous_layout.arena, new_root, &new_nodes_arena, &mut changeset);
            let changeset = Self::update_tree_inner_2(&self.previous_layout.arena, &new_tree);
            self.previous_layout.arena = new_tree;
            changeset
        } else {
            // initialize arena
            use std::iter::FromIterator;
            self.previous_layout.arena = new_nodes_arena.transform(|data| data.calculate_node_data_hash());
            self.previous_layout.root = Some(new_root);
            DomChangeSet {
                added_nodes: self.previous_layout.arena.get_all_node_ids(),
            }
        }
    }

    fn update_tree_inner_2(previous_arena: &Arena<DomHash>, next_arena: &Arena<DomHash>) -> DomChangeSet {

        let mut previous_iter = previous_arena.nodes.iter();
        let mut next_iter = next_arena.nodes.iter().enumerate();
        let mut changeset = DomChangeSet::empty();

        while let Some((next_idx, next_hash)) = next_iter.next() {
            if let Some(old_hash) = previous_iter.next() {
                if old_hash.data != next_hash.data {
                    changeset.added_nodes.insert(NodeId { index: next_idx }, next_hash.data);
                }
            } else {
                // println!("chrildren: no old hash, but subtree has to be added: {:?}!", new_next_id);
                changeset.added_nodes.insert(NodeId { index: next_idx }, next_hash.data);
            }
        }
/*
        loop {
            match (previous_iter.next(), next_iter.next().enumerate()) {
                (None, None) => {
                    // println!("chrildren: old has no children, new has no children!");
                    break;
                },
                (Some(_), None) => {
                    prev = previous_iter.next();
                },
                (None, Some(next_hash)) => {
                    // println!("chrildren: no old hash, but subtree has to be added: {:?}!", new_next_id);
                    // TODO: add subtree
                    changeset.added_nodes.insert(NodeId { index: next_idx }, next_hash.data);
                    next = next_iter.next();
                    next_idx += 1;
                },
                (Some(old_hash), Some(next_hash)) => {
                    if old_hash.data != next_hash.data {
                        changeset.added_nodes.insert(NodeId { index: next_idx }, next_hash.data);
                    }
                    next = next_iter.next();
                    next_idx += 1;
                }
            }
        }
*/
        changeset
    }

    fn update_tree_inner<T>(previous_root: NodeId,
                            previous_hash_arena: &Arena<DomHash>,
                            current_root: NodeId,
                            current_dom_arena: &Arena<NodeData<T>>,
                            changeset: &mut DomChangeSet)
    where T: LayoutScreen
    {
        let mut old_child_iterator = previous_root.children(previous_hash_arena);
        let mut new_child_iterator = current_root.children(previous_hash_arena);

        // children first
        loop {
            // skip the root node itself, although it wouldn't be necessary here
            // old_child_iterator.next();
            // new_child_iterator.next();
            let old_child_next = old_child_iterator.next();
            let new_child_next = new_child_iterator.next();

            match (old_child_next, new_child_next) {
                (None, None) => {
                    // println!("chrildren: old has no children, new has no children!");
                    break;
                },
                (Some(old_hash), None) => {
                    // meaning, the whole subtree should be removed
                    // println!("chrildren: old has children at id: {:?}, new has children at id:", old_hash);
                },
                (None, Some(new_next_id)) => {
                    // println!("chrildren: no old hash, but subtree has to be added: {:?}!", new_next_id);
                    // TODO: add subtree
                },
                (Some(old_hash_id), Some(new_next_node_id)) => {
                    let old_hash = previous_hash_arena[old_hash_id].data;
                    let new_hash = current_dom_arena[new_next_node_id].data.calculate_node_data_hash();

                    if old_hash == new_hash {
                        // println!("chrildren: children are the same!");
                    } else {
                        // hashes differ
                        // println!("chrildren: children are different!");
                        // changeset.added_nodes.insert(new_next_node_id, new_hash);
                    }
                }
            }
        }

        let mut old_iterator = previous_root.following_siblings(previous_hash_arena);
        let mut new_iterator = current_root.following_siblings(current_dom_arena);

        // now iterate over siblings
        loop {
            let old_next = old_iterator.next();
            let new_next = new_iterator.next();

            match (old_next, new_next) {
                (None, None) => {
                    // both old and new node have the same length
                    break;
                },
                (None, Some(new_next_node_id)) => {
                    // new node was pushed as a child
                    let new_hash = current_dom_arena[new_next_node_id].data.calculate_node_data_hash();
                    changeset.added_nodes.insert(new_next_node_id, new_hash);
                    // println!("siblings: node was added as a child!");
                },
                (Some(old_hash_id), None) => {
                    // node was removed as a child
                    // mark node as inactive
                    let old_hash = previous_hash_arena[old_hash_id].data;
                    // println!("siblings: node was removed as a child: {:?}", old_hash);
                },
                (Some(old_hash_id), Some(new_next_node_id)) => {
                    let old_hash = previous_hash_arena[old_hash_id].data;
                    let new_hash = current_dom_arena[new_next_node_id].data.calculate_node_data_hash();

                    if old_hash == new_hash {
                        // println!("siblings: hashes are the same!");
                    } else {
                        // hashes differ
                        // println!("siblings: hashes differ");
                        changeset.added_nodes.insert(new_next_node_id, new_hash);
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) struct HashedDomTree {
    pub(crate) arena: Arena<DomHash>,
    pub(crate) root: Option<NodeId>,
}

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub(crate) struct DomHash(pub u64);

#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub(crate) struct DomNodeHash {
    /// The hash of the node itself
    pub(crate) self_hash: DomHash,
    /// The self_hash-es of the children, in their correct order
    pub(crate) children_hash: Vec<DomHash>,
}

#[derive(Debug)]
pub(crate) struct EditVariableCache {
    pub(crate) map: BTreeMap<DomHash, (bool, DisplayRect)>
}

impl EditVariableCache {
    pub(crate) fn empty() -> Self {
        Self {
            map: BTreeMap::new(),
        }
    }

    pub(crate) fn initialize_new_rectangles(&mut self, solver: &mut Solver, rects: &DomChangeSet) {
        use std::collections::btree_map::Entry::*;

        for dom_hash in rects.added_nodes.values() {

            let map_entry = self.map.entry(*dom_hash);
            match map_entry {
                Occupied(e) => {
                    e.into_mut().0 = true;
                },
                Vacant(e) => {
                    let rect = DisplayRect::default();
                    rect.add_to_solver(solver);
                    e.insert((true, rect));
                }
            }
        }
    }

    /// Last step of the caching algorithm:
    /// Remove all edit variables where the `bool` is set to false
    pub(crate) fn remove_unused_variables(&mut self, solver: &mut Solver) {

        let mut to_be_removed = Vec::<DomHash>::new();

        for (key, &(active, variable_rect)) in &self.map {
            if !active {
                variable_rect.remove_from_solver(solver);
                to_be_removed.push(*key);
            }
        }

        for hash in &to_be_removed {
            self.map.remove(hash);
        }
    }
}