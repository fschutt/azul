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

use std::{
    ops::Deref,
    collections::BTreeMap,
};
use {
    ui_solver::RectConstraintVariables,
    id_tree::{NodeId, Arena},
    traits::Layout,
    dom::NodeData,
};

/// We keep the tree from the previous re-layout. Then, when a re-layout
/// is required, we re-hash all the nodes, insert and remove the necessary
/// constraints and detect changes between the frames
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

    pub(crate) fn update<T: Layout>(&mut self, new_root: NodeId, new_nodes_arena: &Arena<NodeData<T>>) -> DomChangeSet {

        if let Some(_previous_root) = self.previous_layout.root {
            let new_tree = new_nodes_arena.transform(|data, _| data.calculate_node_data_hash());
            let changeset = Self::update_tree_inner(&self.previous_layout.arena, &new_tree);
            self.previous_layout.arena = new_tree;
            changeset
        } else {
            // initialize arena
            self.previous_layout.arena = new_nodes_arena.transform(|data, _| data.calculate_node_data_hash());
            self.previous_layout.root = Some(new_root);
            DomChangeSet {
                added_nodes: self.previous_layout.arena.get_all_node_ids(),
            }
        }
    }

    fn update_tree_inner(previous_arena: &Arena<DomHash>, next_arena: &Arena<DomHash>) -> DomChangeSet {

        let mut previous_iter = previous_arena.nodes.iter();
        let mut next_iter = next_arena.nodes.iter().enumerate();
        let mut changeset = DomChangeSet::empty();

        while let Some((next_idx, next_hash)) = next_iter.next() {
            if let Some(old_hash) = previous_iter.next() {
                if old_hash.data != next_hash.data {
                    changeset.added_nodes.insert(NodeId::new(next_idx), next_hash.data);
                }
            } else {
                // println!("chrildren: no old hash, but subtree has to be added: {:?}!", new_next_id);
                changeset.added_nodes.insert(NodeId::new(next_idx), next_hash.data);
            }
        }

        changeset
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub(crate) struct HashedDomTree {
    pub(crate) arena: Arena<DomHash>,
    pub(crate) root: Option<NodeId>,
}

/// Calculated hash of a DOM node, used for querying attributes of the DOM node
#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub struct DomHash(pub u64);

#[derive(Debug, Clone, Hash, PartialEq, Eq, Ord, PartialOrd)]
pub(crate) struct DomNodeHash {
    /// The hash of the node itself
    pub(crate) self_hash: DomHash,
    /// The self_hash-es of the children, in their correct order
    pub(crate) children_hash: Vec<DomHash>,
}

#[derive(Debug)]
pub(crate) struct EditVariableCache {
    pub(crate) map: BTreeMap<DomHash, (bool, RectConstraintVariables)>
}

impl EditVariableCache {

    pub(crate) fn empty() -> Self {
        Self {
            map: BTreeMap::new(),
        }
    }

    pub(crate) fn initialize_new_rectangles(&mut self, rects: &DomChangeSet) {
        use std::collections::btree_map::Entry::*;

        for dom_hash in rects.added_nodes.values() {

            let map_entry = self.map.entry(*dom_hash);
            match map_entry {
                Occupied(e) => {
                    e.into_mut().0 = true;
                },
                Vacant(e) => {
                    let rect = RectConstraintVariables::default();
                    e.insert((true, rect));
                }
            }
        }
    }

    /// Last step of the caching algorithm:
    /// Remove all edit variables where the `bool` is set to false
    ///
    /// TODO: Right now there is nothing that sets the edit variables to false,
    /// so right now this function does nothing to the cache
    pub(crate) fn remove_unused_variables(&mut self) {

        let mut to_be_removed = Vec::<DomHash>::new();

        for (key, &(active, _)) in &self.map {
            if !active {
                to_be_removed.push(*key);
            }
        }

        for hash in &to_be_removed {
            self.map.remove(hash);
        }
    }
}

/*
#[test]
fn test_domhash_stability() {
    let mut edit_variable_cache = EditVariableCache::empty();
    let mut nodes = BTreeMap::new();

    nodes.insert(NodeId::new(0), DomHash(0));
    nodes.insert(NodeId::new(1), DomHash(1));

    edit_variable_cache.initialize_new_rectangles(&DomChangeSet {
        added_nodes: nodes
    });
    edit_variable_cache.remove_unused_variables();

    // The nodes weren't used, so they should all be removed
    assert!(edit_variable_cache.map.is_empty());
}
*/