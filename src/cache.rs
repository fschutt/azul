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

/// We keep the tree from the previous re-layout. Then, when a re-layout is required,
/// we re-hash all the nodes, insert the 
pub(crate) struct DomTreeCache {
    pub(crate) previous_layout: HashedDomTree,
}

pub(crate) struct DomChangeSet {
    // todo: calculate the constraints that have to be updated
    added_nodes: BTreeMap<NodeId, DomHash>
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

    pub(crate) fn update<T: LayoutScreen>(&mut self, root: NodeId, new_arena: &Arena<NodeData<T>>) -> DomChangeSet {
        
        use std::hash::Hash;
        
        println!("DomTreeCache::update()");
        
        let mut new_iterator = root.following_siblings(new_arena);
        let new_next = new_iterator.next();
        
        let mut changeset = DomChangeSet {
            added_nodes: BTreeMap::new(),
        };

        if let Some(previous_root) = self.previous_layout.root {
            let mut old_iterator = previous_root.following_siblings(new_arena);
            let old_next = old_iterator.next();
            
            loop {
                
                if old_next.is_none() && new_next.is_none() {
                    // both old and new node have the same length
                    break;
                } else if old_next.is_none() && new_next.is_some() {
                    // new node was pushed as a child
                    let new_next = new_next.unwrap();
                    let new_hash = new_arena[new_next].data.calculate_node_data_hash();
                    changeset.added_nodes.insert(new_next, new_hash);
                } else if old_next.is_some() && new_next.is_none() {
                    // node was removed as a child
                    // mark node as inactive
                    println!("node was removed as a child");
                }

                let old_next = old_next.unwrap();
                let new_next = new_next.unwrap();
                let old_hash = self.previous_layout.arena[old_next].data;
                let new_hash = new_arena[new_next].data.calculate_node_data_hash();

                if old_hash != new_hash {
                    // hashes differ
                    println!("hashes differ");
                }
            }
        } else {
            // initialize tree
            println!("initialize tree");
        }
        changeset

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
        for dom_hash in rects.added_nodes.values() {
            println!("adding rectangle!!!");
            let rect = DisplayRect::default();
            rect.add_to_solver(solver);
            self.map.insert(*dom_hash, (true, rect));
        }
    }

    /// Last step of the caching algorithm: 
    /// Remove all edit variables where the `bool` is set to false
    pub(crate) fn remove_unused_variables(&mut self, solver: &mut Solver) {
        
        let mut to_be_removed = Vec::<DomHash>::new();
        
        for (key, &(active, variable_rect)) in &self.map {
            if !active {
                println!("removing rectangle!!!");
                variable_rect.remove_from_solver(solver);
                to_be_removed.push(*key);  
            }
        }

        for hash in &to_be_removed {
            self.map.remove(hash);
        }
    }
}