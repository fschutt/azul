//! Node tree data structures and hierarchy management.
//!
//! This module provides the core data structures for managing DOM-like tree hierarchies:
//!
//! - `NodeId`: Type-safe node identifiers with Option<NodeId> optimization
//! - `NodeHierarchy`: Parent-child relationships between nodes
//! - `NodeDataContainer`: Generic storage for node data with efficient indexing
//!
//! # Memory Layout
//!
//! `NodeId` stores a plain `usize` index internally. For FFI structs that need
//! `Option<NodeId>`, a manual 1-based encoding is used (0 = None, n > 0 = Some(n-1)).
//!
//! # Performance
//!
//! - Node lookups are O(1) via direct array indexing
//! - Parent/child traversal is O(1) via pre-computed indices
//! - No heap allocations after initial tree construction

use alloc::vec::Vec;
use core::{
    ops::{Index, IndexMut},
    slice::Iter,
};

pub use self::node_id::NodeId;
use crate::styled_dom::NodeHierarchyItem;

/// Type alias for depth-first traversal results: (depth, `node_id`) pairs
pub type NodeDepths = Vec<(usize, NodeId)>;

// Simple FFI-safe NodeId - just a wrapper around usize
pub mod node_id {

    use alloc::vec::Vec;
    use core::{
        fmt,
        ops::{Add, AddAssign},
    };

    /// A type-safe identifier for a node within a DOM tree.
    ///
    /// `NodeId` is FFI-safe (`#[repr(C)]`) and stores a **zero-based** index internally.
    /// Use `NodeId::index()` to get the array index for direct node access.
    ///
    /// # Zero-based indexing
    ///
    /// - `NodeId::new(0)` → first node (index 0)
    /// - `NodeId::new(5)` → sixth node (index 5)
    /// - Use `node_id.index()` to get the array index
    ///
    /// # FFI Encoding (for `Option<NodeId>`)
    ///
    /// When storing `Option<NodeId>` in FFI structs (like `NodeHierarchyItem`),
    /// we use a **1-based encoding** to represent None:
    ///
    /// - `0` means `None` (no node)
    /// - `n > 0` means `Some(NodeId(n - 1))`
    ///
    /// Use [`NodeId::from_usize`] to decode and [`NodeId::into_raw`] to encode.
    /// See also: [`crate::styled_dom::NodeHierarchyItemId`] for the FFI wrapper type.
    ///
    /// # Warning
    ///
    /// **Never manually construct raw usize values for node hierarchy fields!**
    /// Always use the provided `from_usize`/`into_raw` functions to avoid
    /// off-by-one errors that can cause index-out-of-bounds panics.
    ///
    #[repr(C)]
    #[derive(Copy, Clone, PartialOrd, Ord, PartialEq, Eq, Hash)]
    pub struct NodeId {
        // Private field to prevent direct manipulation.
        // Use NodeId::new() to create, NodeId::index() to read.
        inner: usize,
    }

    impl NodeId {
        /// The zero/first node ID (index 0).
        pub const ZERO: Self = Self { inner: 0 };

        /// Creates a new `NodeId` from a zero-based index.
        #[inline]
        #[must_use] pub const fn new(value: usize) -> Self {
            Self { inner: value }
        }

        /// Decodes a raw `usize` to `Option<NodeId>` using 1-based encoding.
        ///
        /// This is the inverse of [`NodeId::into_usize`].
        ///
        /// - `0` → `None` (no node)
        /// - `n > 0` → `Some(NodeId(n - 1))`
        ///
        /// # Warning
        ///
        /// This function is for decoding values stored in FFI structs like
        /// `NodeHierarchyItem`. Do not use raw usize values directly - always
        /// decode them first!
        #[inline]
        #[must_use] pub const fn from_usize(value: usize) -> Option<Self> {
            match value {
                0 => None,
                i => Some(Self { inner: i - 1 }),
            }
        }

        /// Encodes `Option<NodeId>` to a raw `usize` for storage in FFI structs.
        ///
        /// - `None` → `0`
        /// - `Some(NodeId(n))` → `n + 1`
        ///
        /// The returned value uses **1-based encoding**! A value of `0` means "no node",
        /// NOT "node at index 0". Use [`NodeId::from_usize`] to decode.
        ///
        #[inline]
        #[must_use] pub const fn into_raw(val: &Option<Self>) -> usize {
            match val {
                None => 0,
                Some(s) => s.inner + 1,
            }
        }

        /// Returns the **zero-based** index of this node.
        ///
        /// This is the actual array index where the node data is stored.
        #[inline]
        #[must_use] pub const fn index(&self) -> usize {
            self.inner
        }
    }

    impl From<usize> for NodeId {
        fn from(val: usize) -> Self {
            Self::new(val)
        }
    }

    impl From<NodeId> for usize {
        fn from(val: NodeId) -> Self {
            val.inner
        }
    }

    impl Add<usize> for NodeId {
        type Output = Self;
        /// AUDIT: saturating add. A raw `self.inner + other` could overflow
        /// (debug panic / release wrap to a bogus small index that then aliases
        /// a real node). `NodeId` indices are bounded by the arena length, so a
        /// saturation to `usize::MAX` is an obviously-invalid index that fails
        /// loudly at the next bounds-checked access rather than silently aliasing.
        #[inline]
        fn add(self, other: usize) -> Self {
            Self::new(self.inner.saturating_add(other))
        }
    }

    impl AddAssign<usize> for NodeId {
        /// AUDIT: saturating add — see [`Add`] impl above.
        #[inline]
        fn add_assign(&mut self, other: usize) {
            *self = *self + other;
        }
    }

    impl fmt::Display for NodeId {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "{}", self.inner)
        }
    }

    impl fmt::Debug for NodeId {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            write!(f, "NodeId({})", self.inner)
        }
    }
}

/// Hierarchical information about a node (stores the indices of the parent / child nodes).
#[derive(Debug, Default, Copy, Clone, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub struct Node {
    pub parent: Option<NodeId>,
    pub previous_sibling: Option<NodeId>,
    pub next_sibling: Option<NodeId>,
    pub last_child: Option<NodeId>,
    // NOTE: first_child can be calculated on the fly:
    //
    //   - if last_child is None, first_child is None
    //   - if last_child is Some, first_child is parent_index + 1
    //
    // This makes the "Node" struct take up 4 registers instead of 5
    //
    // pub first_child: Option<NodeId>,
}

impl Node {
    pub const ROOT: Self = Self {
        parent: None,
        previous_sibling: None,
        next_sibling: None,
        last_child: None,
    };

    #[inline]
    #[must_use] pub const fn has_parent(&self) -> bool {
        self.parent.is_some()
    }
    #[inline]
    #[must_use] pub const fn has_previous_sibling(&self) -> bool {
        self.previous_sibling.is_some()
    }
    #[inline]
    #[must_use] pub const fn has_next_sibling(&self) -> bool {
        self.next_sibling.is_some()
    }
    #[inline]
    #[must_use] pub const fn has_first_child(&self) -> bool {
        self.last_child.is_some() /* last_child and first_child are always set together */
    }
    #[inline]
    #[must_use] pub const fn has_last_child(&self) -> bool {
        self.last_child.is_some()
    }

    #[inline]
    #[must_use] pub fn get_first_child(&self, current_node_id: NodeId) -> Option<NodeId> {
        // last_child and first_child are always set together
        self.last_child.map(|_| current_node_id + 1)
    }
}

/// The hierarchy of nodes is stored separately from the actual node content in order
/// to save on memory, since the hierarchy can be re-used across several DOM trees even
/// if the content changes.
#[derive(Debug, Default, Clone, PartialEq, Hash, Eq, PartialOrd, Ord)]
pub struct NodeHierarchy {
    pub internal: Vec<Node>,
}

impl NodeHierarchy {
    #[inline]
    #[must_use] pub const fn new(data: Vec<Node>) -> Self {
        Self { internal: data }
    }

    #[inline]
    #[must_use] pub fn as_ref(&self) -> NodeHierarchyRef<'_> {
        NodeHierarchyRef {
            internal: &self.internal[..],
        }
    }

}

/// The hierarchy of nodes is stored separately from the actual node content in order
/// to save on memory, since the hierarchy can be re-used across several DOM trees even
/// if the content changes.
#[derive(Debug, PartialEq, Hash, Eq)]
pub struct NodeHierarchyRef<'a> {
    pub internal: &'a [Node],
}

impl<'a> NodeHierarchyRef<'a> {
    #[inline]
    #[must_use] pub const fn from_slice(data: &'a [Node]) -> Self {
        NodeHierarchyRef { internal: data }
    }

    #[inline]
    #[must_use] pub const fn len(&self) -> usize {
        self.internal.len()
    }

    #[inline]
    #[must_use] pub const fn is_empty(&self) -> bool {
        self.internal.is_empty()
    }

    #[inline]
    #[must_use] pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.internal.get(id.index())
    }

    #[inline]
    #[must_use] pub const fn linear_iter(&self) -> LinearIterator {
        LinearIterator {
            arena_len: self.len(),
            position: 0,
        }
    }

    /// Returns the `(depth, NodeId)` of all parent nodes (i.e. nodes that have a
    /// `first_child`), in depth sorted order, (i.e. `NodeId(0)` with a depth of 0) is
    /// the first element.
    ///
    /// Runtime: O(n) max
    // the `.drain(..)` calls intentionally empty current/next_children to REUSE
    // their allocations across the BFS levels; `into_iter()` would move them.
    #[allow(clippy::iter_with_drain)]
    #[must_use] pub fn get_parents_sorted_by_depth(&self) -> NodeDepths {
        // AUDIT: an empty hierarchy has no root node — indexing `internal[0]`
        // (via `self[root]` below) would panic. Bail out early.
        if self.is_empty() {
            return Vec::new();
        }

        let root = NodeId::new(0);
        let mut non_leaf_nodes = Vec::new();

        // AUDIT: a childless root (e.g. a single-node DOM) is a LEAF, not a
        // parent. The old code seeded `current_children` with the root and
        // unconditionally pushed it into `non_leaf_nodes`, mislabeling it as a
        // parent. Only descend (and only emit the root) when it actually has a
        // first child.
        if !self[root].has_first_child() {
            return non_leaf_nodes;
        }

        let mut current_children = vec![(0, root)];
        let mut next_children = Vec::new();
        let mut depth = 1_usize;

        loop {
            for id in &current_children {
                for child_id in id.1.children(self).filter(|id| self[*id].has_first_child()) {
                    next_children.push((depth, child_id));
                }
            }

            non_leaf_nodes.extend(&mut current_children.drain(..));

            if next_children.is_empty() {
                break;
            }
            current_children.extend(&mut next_children.drain(..));
            depth += 1;
        }

        non_leaf_nodes
    }

}

#[derive(Debug, Clone, PartialEq, Hash, Eq, PartialOrd, Ord)]
pub struct NodeDataContainer<T> {
    pub internal: Vec<T>,
}

impl<T> From<Vec<T>> for NodeDataContainer<T> {
    fn from(v: Vec<T>) -> Self {
        Self { internal: v }
    }
}

#[derive(Debug, PartialEq, Hash, Eq, PartialOrd, Ord)]
pub struct NodeDataContainerRef<'a, T> {
    pub internal: &'a [T],
}

#[derive(Debug, PartialEq, Hash, Eq, PartialOrd, Ord)]
pub struct NodeDataContainerRefMut<'a, T> {
    pub internal: &'a mut [T],
}

impl<T> Default for NodeDataContainer<T> {
    fn default() -> Self {
        Self {
            internal: Vec::new(),
        }
    }
}

impl Index<NodeId> for NodeHierarchyRef<'_> {
    type Output = Node;

    #[inline]
    fn index(&self, node_id: NodeId) -> &Node {
        &self.internal[node_id.index()]
    }
}

impl<T> NodeDataContainer<T> {
    #[inline]
    #[must_use] pub const fn new(data: Vec<T>) -> Self {
        Self { internal: data }
    }

    #[inline]
    #[must_use] pub const fn is_empty(&self) -> bool {
        self.internal.is_empty()
    }

    #[inline]
    #[must_use] pub fn as_ref(&self) -> NodeDataContainerRef<'_, T> {
        NodeDataContainerRef {
            internal: &self.internal[..],
        }
    }

    #[inline]
    pub fn as_ref_mut(&mut self) -> NodeDataContainerRefMut<'_, T> {
        NodeDataContainerRefMut {
            internal: &mut self.internal[..],
        }
    }

    #[inline]
    #[must_use] pub const fn len(&self) -> usize {
        self.internal.len()
    }
}

impl<'a, T: 'a> NodeDataContainerRefMut<'a, T> {
    #[inline]
    pub const fn from_slice(data: &'a mut [T]) -> Self {
        NodeDataContainerRefMut { internal: data }
    }
}

impl<'a, T: 'a> NodeDataContainerRefMut<'a, T> {
    #[inline]
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut T> {
        self.internal.get_mut(id.index())
    }
}

impl<'a, T: Send + 'a> NodeDataContainerRef<'a, T> {
    pub fn transform_nodeid_optional<U: Send, F>(
        &self,
        closure: F,
    ) -> NodeDataContainer<U>
    where
        F: Send + Sync + Fn(NodeId) -> Option<U>,
    {
        let len = self.len();
        NodeDataContainer {
            internal: (0..len)
                .filter_map(|node_id| closure(NodeId::new(node_id)))
                .collect::<Vec<U>>(),
        }
    }
}

impl<'a, T> IntoIterator for &NodeDataContainerRef<'a, T> {
    type Item = &'a T;
    type IntoIter = Iter<'a, T>;
    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.internal.iter()
    }
}

impl<'a, T: 'a> NodeDataContainerRef<'a, T> {
    #[inline]
    pub const fn from_slice(data: &'a [T]) -> Self {
        NodeDataContainerRef { internal: data }
    }

    #[inline]
    #[must_use] pub const fn len(&self) -> usize {
        self.internal.len()
    }

    #[inline]
    #[must_use] pub const fn is_empty(&self) -> bool {
        self.internal.is_empty()
    }

    #[inline]
    #[must_use] pub fn get(&self, id: NodeId) -> Option<&T> {
        self.internal.get(id.index())
    }

    #[inline]
    pub fn iter(&self) -> Iter<'_, T> {
        self.internal.iter()
    }

    #[inline]
    #[must_use] pub const fn linear_iter(&self) -> LinearIterator {
        LinearIterator {
            arena_len: self.len(),
            position: 0,
        }
    }
}

impl<T> Index<NodeId> for NodeDataContainerRef<'_, T> {
    type Output = T;

    #[inline]
    fn index(&self, node_id: NodeId) -> &T {
        &self.internal[node_id.index()]
    }
}

impl<T> Index<NodeId> for NodeDataContainerRefMut<'_, T> {
    type Output = T;

    #[inline]
    fn index(&self, node_id: NodeId) -> &T {
        &self.internal[node_id.index()]
    }
}

impl<T> IndexMut<NodeId> for NodeDataContainerRefMut<'_, T> {
    #[inline]
    fn index_mut(&mut self, node_id: NodeId) -> &mut T {
        &mut self.internal[node_id.index()]
    }
}

impl NodeId {
    /// Return an iterator of references to this node and the siblings before it.
    ///
    /// Call `.next().unwrap()` once on the iterator to skip the node itself.
    #[inline]
    #[must_use] pub const fn preceding_siblings<'a>(
        self,
        node_hierarchy: &'a NodeHierarchyRef<'a>,
    ) -> PrecedingSiblings<'a> {
        PrecedingSiblings {
            node_hierarchy,
            node: Some(self),
        }
    }

    /// Return an iterator of references to this node's children.
    #[inline]
    #[must_use] pub fn children<'a>(self, node_hierarchy: &'a NodeHierarchyRef<'a>) -> Children<'a> {
        Children {
            node_hierarchy,
            node: node_hierarchy[self].get_first_child(self),
        }
    }
}

macro_rules! impl_node_iterator {
    ($name:ident, $next:expr) => {
        impl Iterator for $name<'_> {
            type Item = NodeId;

            fn next(&mut self) -> Option<NodeId> {
                match self.node.take() {
                    Some(node) => {
                        self.node = $next(&self.node_hierarchy[node]);
                        Some(node)
                    }
                    None => None,
                }
            }
        }
    };
}

/// An linear iterator, does not respect the DOM in any way,
/// it just iterates over the nodes like a Vec
#[derive(Debug, Clone)]
pub struct LinearIterator {
    arena_len: usize,
    position: usize,
}

impl Iterator for LinearIterator {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        if self.arena_len < 1 || self.position > (self.arena_len - 1) {
            None
        } else {
            let new_id = Some(NodeId::new(self.position));
            self.position += 1;
            new_id
        }
    }
}

/// An iterator of references to the siblings before a given node.
#[derive(Debug)]
pub struct PrecedingSiblings<'a> {
    node_hierarchy: &'a NodeHierarchyRef<'a>,
    node: Option<NodeId>,
}

impl_node_iterator!(PrecedingSiblings, |node: &Node| node.previous_sibling);

/// Special iterator for using `NodeDataContainerRef`<AzNode> instead of `NodeHierarchy`
#[derive(Debug)]
pub struct AzChildren<'a> {
    node_hierarchy: &'a NodeDataContainerRef<'a, NodeHierarchyItem>,
    node: Option<NodeId>,
}

impl Iterator for AzChildren<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        match self.node.take() {
            Some(node) => {
                self.node = self.node_hierarchy[node].next_sibling_id();
                Some(node)
            }
            None => None,
        }
    }
}

/// Special iterator for using `NodeDataContainerRef`<AzNode> instead of `NodeHierarchy`
#[derive(Debug)]
pub struct AzReverseChildren<'a> {
    node_hierarchy: &'a NodeDataContainerRef<'a, NodeHierarchyItem>,
    node: Option<NodeId>,
}

impl Iterator for AzReverseChildren<'_> {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        match self.node.take() {
            Some(node) => {
                self.node = self.node_hierarchy[node].previous_sibling_id();
                Some(node)
            }
            None => None,
        }
    }
}

impl NodeId {
    /// Traverse up through the hierarchy until a node matching the predicate is found.
    ///
    /// Necessary to resolve the last positioned (= relative)
    /// element of an absolute node.
    pub fn get_nearest_matching_parent<'a, F>(
        self,
        node_hierarchy: &'a NodeDataContainerRef<'a, NodeHierarchyItem>,
        predicate: F,
    ) -> Option<Self>
    where
        F: Fn(Self) -> bool,
    {
        // AUDIT: guard against (a) an out-of-bounds `self` and (b) a cycle in a
        // corrupt hierarchy (a `parent_id` that points back down into a
        // descendant). Use checked `get` and cap the walk at the node count —
        // a valid parent chain can never be longer than the number of nodes.
        let node_count = node_hierarchy.internal.len();
        let mut current_node = node_hierarchy.internal.get(self.index())?.parent_id()?;
        for _ in 0..node_count {
            if predicate(current_node) {
                return Some(current_node);
            }
            current_node = node_hierarchy.internal.get(current_node.index())?.parent_id()?;
        }
        None
    }

    /// Return the children of this node (necessary for parallel iteration over children)
    #[inline]
    #[must_use] pub fn az_children_collect<'a>(
        self,
        node_hierarchy: &'a NodeDataContainerRef<'a, NodeHierarchyItem>,
    ) -> Vec<Self> {
        self.az_children(node_hierarchy).collect()
    }

    /// Return an iterator of references to this node's children.
    #[inline]
    #[must_use] pub fn az_children<'a>(
        self,
        node_hierarchy: &'a NodeDataContainerRef<'a, NodeHierarchyItem>,
    ) -> AzChildren<'a> {
        AzChildren {
            node_hierarchy,
            node: node_hierarchy[self].first_child_id(self),
        }
    }

    /// Return an iterator of references to this node's children.
    #[inline]
    #[must_use] pub fn az_reverse_children<'a>(
        self,
        node_hierarchy: &'a NodeDataContainerRef<'a, NodeHierarchyItem>,
    ) -> AzReverseChildren<'a> {
        AzReverseChildren {
            node_hierarchy,
            node: node_hierarchy[self].last_child_id(),
        }
    }
}

/// An iterator of references to the children of a given node.
#[derive(Debug)]
pub struct Children<'a> {
    node_hierarchy: &'a NodeHierarchyRef<'a>,
    node: Option<NodeId>,
}

impl_node_iterator!(Children, |node: &Node| node.next_sibling);

#[cfg(test)]
mod audit_tests {
    use super::*;
    use crate::styled_dom::NodeHierarchyItem;

    #[test]
    fn parents_by_depth_empty_hierarchy() {
        let h = NodeHierarchy::new(Vec::new());
        assert!(h.as_ref().get_parents_sorted_by_depth().is_empty());
    }

    #[test]
    fn parents_by_depth_single_childless_root() {
        // A single-node DOM: the root is a LEAF, not a parent.
        let h = NodeHierarchy::new(vec![Node::ROOT]);
        assert!(h.as_ref().get_parents_sorted_by_depth().is_empty());
    }

    #[test]
    fn parents_by_depth_root_with_child() {
        let root = Node {
            parent: None,
            previous_sibling: None,
            next_sibling: None,
            last_child: Some(NodeId::new(1)),
        };
        let child = Node {
            parent: Some(NodeId::new(0)),
            ..Node::ROOT
        };
        let h = NodeHierarchy::new(vec![root, child]);
        let parents = h.as_ref().get_parents_sorted_by_depth();
        assert_eq!(parents, vec![(0, NodeId::new(0))]);
    }

    fn item(parent: Option<usize>) -> NodeHierarchyItem {
        NodeHierarchyItem {
            parent: parent.map_or(0, |p| p + 1),
            previous_sibling: 0,
            next_sibling: 0,
            last_child: 0,
        }
    }

    #[test]
    fn nearest_matching_parent_cycle_terminates() {
        // node1.parent = 2, node2.parent = 1 — cyclic, must not hang.
        let items = vec![item(None), item(Some(2)), item(Some(1))];
        let cont = NodeDataContainerRef { internal: &items };
        let r = NodeId::new(1).get_nearest_matching_parent(&cont, |_| false);
        assert_eq!(r, None);
    }

    #[test]
    fn nearest_matching_parent_finds_match() {
        // 0 <- 1 <- 2 ; from 2, find the root (index 0).
        let items = vec![item(None), item(Some(0)), item(Some(1))];
        let cont = NodeDataContainerRef { internal: &items };
        let r = NodeId::new(2).get_nearest_matching_parent(&cont, |n| n == NodeId::new(0));
        assert_eq!(r, Some(NodeId::new(0)));
    }

    #[test]
    fn node_id_add_saturates() {
        assert_eq!(NodeId::new(5) + 3, NodeId::new(8));
        assert_eq!(NodeId::new(usize::MAX) + 1, NodeId::new(usize::MAX));
        let mut n = NodeId::new(usize::MAX);
        n += 10;
        assert_eq!(n, NodeId::new(usize::MAX));
    }
}

#[cfg(test)]
#[allow(clippy::pedantic, clippy::nursery)]
mod autotest_generated {
    use core::sync::atomic::{AtomicUsize, Ordering};

    use super::*;
    use crate::styled_dom::NodeHierarchyItem;

    // ---------------------------------------------------------------------
    // helpers
    // ---------------------------------------------------------------------

    /// A well-formed 5-node tree (children are always contiguous after the
    /// parent, as the `first_child = parent + 1` design requires):
    ///
    /// ```text
    /// 0 ── 1 ── 2
    ///  │    └── 3
    ///  └── 4
    /// ```
    fn tree_5() -> Vec<Node> {
        vec![
            Node {
                parent: None,
                previous_sibling: None,
                next_sibling: None,
                last_child: Some(NodeId::new(4)),
            },
            Node {
                parent: Some(NodeId::new(0)),
                previous_sibling: None,
                next_sibling: Some(NodeId::new(4)),
                last_child: Some(NodeId::new(3)),
            },
            Node {
                parent: Some(NodeId::new(1)),
                previous_sibling: None,
                next_sibling: Some(NodeId::new(3)),
                last_child: None,
            },
            Node {
                parent: Some(NodeId::new(1)),
                previous_sibling: Some(NodeId::new(2)),
                next_sibling: None,
                last_child: None,
            },
            Node {
                parent: Some(NodeId::new(0)),
                previous_sibling: Some(NodeId::new(1)),
                next_sibling: None,
                last_child: None,
            },
        ]
    }

    fn items(nodes: &[Node]) -> Vec<NodeHierarchyItem> {
        nodes.iter().copied().map(NodeHierarchyItem::from).collect()
    }

    // ---------------------------------------------------------------------
    // NodeId::new / index / ZERO  (constructor + getter)
    // ---------------------------------------------------------------------

    #[test]
    fn node_id_new_roundtrips_index_at_boundaries() {
        for v in [0_usize, 1, 2, 42, usize::MAX - 1, usize::MAX] {
            assert_eq!(NodeId::new(v).index(), v, "index() must echo new()");
        }
    }

    #[test]
    fn node_id_zero_matches_new_zero() {
        assert_eq!(NodeId::ZERO, NodeId::new(0));
        assert_eq!(NodeId::ZERO.index(), 0);
    }

    #[test]
    fn node_id_usize_conversions_are_lossless() {
        for v in [0_usize, 7, usize::MAX] {
            let id: NodeId = v.into();
            let back: usize = id.into();
            assert_eq!(back, v);
        }
    }

    #[test]
    fn node_id_ordering_follows_index() {
        assert!(NodeId::new(0) < NodeId::new(1));
        assert!(NodeId::new(1) < NodeId::new(usize::MAX));
        assert_eq!(NodeId::new(3), NodeId::new(3));
    }

    // ---------------------------------------------------------------------
    // NodeId::from_usize / into_raw  (1-based FFI encoding round-trip)
    // ---------------------------------------------------------------------

    #[test]
    fn from_usize_zero_is_none_and_shifts_by_one() {
        assert_eq!(NodeId::from_usize(0), None);
        assert_eq!(NodeId::from_usize(1), Some(NodeId::new(0)));
        assert_eq!(NodeId::from_usize(2), Some(NodeId::new(1)));
        // usize::MAX must NOT overflow the `i - 1` decode.
        assert_eq!(
            NodeId::from_usize(usize::MAX),
            Some(NodeId::new(usize::MAX - 1))
        );
    }

    #[test]
    fn into_raw_encodes_none_as_zero() {
        assert_eq!(NodeId::into_raw(&None), 0);
        assert_eq!(NodeId::into_raw(&Some(NodeId::new(0))), 1);
        assert_eq!(NodeId::into_raw(&Some(NodeId::new(41))), 42);
        // Largest index that survives the +1 encode without overflowing.
        assert_eq!(NodeId::into_raw(&Some(NodeId::new(usize::MAX - 1))), usize::MAX);
    }

    #[test]
    fn encode_decode_roundtrip_is_identity() {
        // decode(encode(x)) == x
        for x in [
            None,
            Some(NodeId::new(0)),
            Some(NodeId::new(1)),
            Some(NodeId::new(9_999)),
            Some(NodeId::new(usize::MAX - 1)),
        ] {
            assert_eq!(NodeId::from_usize(NodeId::into_raw(&x)), x, "decode(encode({x:?}))");
        }
        // encode(decode(n)) == n
        for n in [0_usize, 1, 2, 12_345, usize::MAX] {
            assert_eq!(NodeId::into_raw(&NodeId::from_usize(n)), n, "encode(decode({n}))");
        }
    }

    /// BOUNDARY: `NodeId::new(usize::MAX)` is the one index that cannot be
    /// encoded — `into_raw` computes `inner + 1`, which overflows. Unlike the
    /// `Add`/`AddAssign` impls (which were deliberately made saturating), this
    /// add is unchecked: debug builds panic, release builds wrap to `0`, i.e.
    /// the `None` encoding. Either way the node is lost; assert that it never
    /// silently produces some *other* valid-looking node id.
    #[cfg(feature = "std")]
    #[test]
    fn into_raw_at_usize_max_never_yields_a_bogus_node() {
        let encoded = std::panic::catch_unwind(|| NodeId::into_raw(&Some(NodeId::new(usize::MAX))));
        match encoded {
            // debug: overflow check fires — loud failure, acceptable.
            Err(_) => {}
            // release: wraps to 0 == the "no node" encoding.
            Ok(raw) => {
                assert_eq!(raw, 0, "wrapped encode must not alias a real node id");
                assert_eq!(NodeId::from_usize(raw), None);
            }
        }
    }

    // ---------------------------------------------------------------------
    // NodeId Display / Debug  (serializer)
    // ---------------------------------------------------------------------

    #[test]
    fn node_id_display_and_debug_are_well_formed() {
        assert_eq!(alloc::format!("{}", NodeId::new(0)), "0");
        assert_eq!(alloc::format!("{}", NodeId::new(7)), "7");
        assert_eq!(alloc::format!("{:?}", NodeId::new(7)), "NodeId(7)");
        // Extreme values must render without panicking and stay non-empty.
        let max = alloc::format!("{}", NodeId::new(usize::MAX));
        assert_eq!(max, alloc::format!("{}", usize::MAX));
        assert!(!max.is_empty());
        assert_eq!(
            alloc::format!("{:?}", NodeId::new(usize::MAX)),
            alloc::format!("NodeId({})", usize::MAX)
        );
    }

    // ---------------------------------------------------------------------
    // Node predicates + get_first_child
    // ---------------------------------------------------------------------

    #[test]
    fn node_root_and_default_have_no_relations() {
        for n in [Node::ROOT, Node::default()] {
            assert!(!n.has_parent());
            assert!(!n.has_previous_sibling());
            assert!(!n.has_next_sibling());
            assert!(!n.has_first_child());
            assert!(!n.has_last_child());
            assert_eq!(n.get_first_child(NodeId::new(0)), None);
        }
        assert_eq!(Node::default(), Node::ROOT);
    }

    #[test]
    fn node_predicates_report_each_populated_field() {
        let full = Node {
            parent: Some(NodeId::new(1)),
            previous_sibling: Some(NodeId::new(2)),
            next_sibling: Some(NodeId::new(3)),
            last_child: Some(NodeId::new(4)),
        };
        assert!(full.has_parent());
        assert!(full.has_previous_sibling());
        assert!(full.has_next_sibling());
        assert!(full.has_first_child());
        assert!(full.has_last_child());
    }

    /// INVARIANT: `has_first_child()` and `has_last_child()` read the same
    /// field, so they can never disagree — child-presence is all-or-nothing.
    #[test]
    fn has_first_child_always_agrees_with_has_last_child() {
        for last_child in [None, Some(NodeId::new(0)), Some(NodeId::new(usize::MAX))] {
            let n = Node {
                last_child,
                ..Node::ROOT
            };
            assert_eq!(n.has_first_child(), n.has_last_child());
            assert_eq!(n.has_first_child(), last_child.is_some());
        }
    }

    #[test]
    fn get_first_child_is_self_plus_one_and_saturates() {
        let parent = Node {
            last_child: Some(NodeId::new(9)),
            ..Node::ROOT
        };
        assert_eq!(parent.get_first_child(NodeId::new(0)), Some(NodeId::new(1)));
        assert_eq!(parent.get_first_child(NodeId::new(5)), Some(NodeId::new(6)));
        // Extreme id: the `+ 1` is saturating, so this must not panic/wrap. It
        // yields an obviously-invalid id that fails loudly at the next lookup.
        assert_eq!(
            parent.get_first_child(NodeId::new(usize::MAX)),
            Some(NodeId::new(usize::MAX))
        );
        // A leaf has no first child regardless of how extreme the id is.
        assert_eq!(Node::ROOT.get_first_child(NodeId::new(usize::MAX)), None);
    }

    // ---------------------------------------------------------------------
    // NodeHierarchy / NodeHierarchyRef
    // ---------------------------------------------------------------------

    #[test]
    fn hierarchy_new_preserves_len_and_contents() {
        let h = NodeHierarchy::new(tree_5());
        assert_eq!(h.as_ref().len(), 5);
        assert!(!h.as_ref().is_empty());
        assert_eq!(h.as_ref().get(NodeId::new(0)), Some(&tree_5()[0]));
        assert_eq!(h.as_ref().internal, &tree_5()[..]);
    }

    #[test]
    fn empty_hierarchy_is_empty_everywhere() {
        let h = NodeHierarchy::new(Vec::new());
        let r = h.as_ref();
        assert_eq!(r.len(), 0);
        assert!(r.is_empty());
        assert_eq!(r.get(NodeId::new(0)), None);
        assert_eq!(r.linear_iter().count(), 0);
        assert!(r.get_parents_sorted_by_depth().is_empty());

        let default = NodeHierarchy::default();
        assert!(default.as_ref().is_empty());
    }

    #[test]
    fn hierarchy_ref_from_slice_matches_len_and_emptiness() {
        let empty: [Node; 0] = [];
        assert_eq!(NodeHierarchyRef::from_slice(&empty).len(), 0);
        assert!(NodeHierarchyRef::from_slice(&empty).is_empty());

        let nodes = tree_5();
        let r = NodeHierarchyRef::from_slice(&nodes);
        assert_eq!(r.len(), 5);
        assert!(!r.is_empty());
    }

    /// `get()` is the bounds-checked accessor: an out-of-range id must return
    /// `None`, never panic and never read out of bounds.
    #[test]
    fn hierarchy_ref_get_out_of_bounds_returns_none() {
        let nodes = tree_5();
        let r = NodeHierarchyRef::from_slice(&nodes);
        assert!(r.get(NodeId::new(4)).is_some());
        assert_eq!(r.get(NodeId::new(5)), None);
        assert_eq!(r.get(NodeId::new(usize::MAX)), None);
    }

    /// `Index` (unlike `get`) is unchecked-by-contract: it must fail loudly
    /// rather than silently hand back an unrelated node.
    #[test]
    #[should_panic]
    fn hierarchy_ref_index_out_of_bounds_panics() {
        let nodes = tree_5();
        let r = NodeHierarchyRef::from_slice(&nodes);
        let _ = &r[NodeId::new(5)];
    }

    #[test]
    fn hierarchy_linear_iter_walks_every_index_in_order() {
        let nodes = tree_5();
        let r = NodeHierarchyRef::from_slice(&nodes);
        let ids: Vec<NodeId> = r.linear_iter().collect();
        assert_eq!(
            ids,
            (0..5).map(NodeId::new).collect::<Vec<_>>(),
            "linear_iter must yield 0..len exactly once, in order"
        );
    }

    /// The `arena_len < 1` guard exists because `arena_len - 1` would underflow
    /// on an empty arena; check the 0- and 1-element boundaries explicitly.
    #[test]
    fn linear_iter_len_zero_and_one_boundaries() {
        let empty: [Node; 0] = [];
        assert_eq!(
            NodeHierarchyRef::from_slice(&empty).linear_iter().next(),
            None
        );

        let one = [Node::ROOT];
        let mut it = NodeHierarchyRef::from_slice(&one).linear_iter();
        assert_eq!(it.next(), Some(NodeId::new(0)));
        assert_eq!(it.next(), None);
        // Exhausted iterators stay exhausted.
        assert_eq!(it.next(), None);
    }

    #[test]
    fn get_parents_sorted_by_depth_is_depth_ordered_and_leaf_free() {
        let h = NodeHierarchy::new(tree_5());
        let parents = h.as_ref().get_parents_sorted_by_depth();
        // Only 0 and 1 have children; 2/3/4 are leaves and must not appear.
        assert_eq!(parents, vec![(0, NodeId::new(0)), (1, NodeId::new(1))]);
        // Depths must be non-decreasing.
        assert!(parents.windows(2).all(|w| w[0].0 <= w[1].0));
    }

    // ---------------------------------------------------------------------
    // NodeId::children / preceding_siblings  (NodeHierarchyRef iterators)
    // ---------------------------------------------------------------------

    #[test]
    fn children_yields_direct_children_only() {
        let nodes = tree_5();
        let r = NodeHierarchyRef::from_slice(&nodes);
        assert_eq!(
            NodeId::new(0).children(&r).collect::<Vec<_>>(),
            vec![NodeId::new(1), NodeId::new(4)]
        );
        assert_eq!(
            NodeId::new(1).children(&r).collect::<Vec<_>>(),
            vec![NodeId::new(2), NodeId::new(3)]
        );
        // Leaves have no children.
        assert_eq!(NodeId::new(2).children(&r).count(), 0);
        assert_eq!(NodeId::new(4).children(&r).count(), 0);
    }

    #[test]
    #[should_panic]
    fn children_of_out_of_bounds_node_panics_loudly() {
        let nodes = tree_5();
        let r = NodeHierarchyRef::from_slice(&nodes);
        // `children()` indexes the hierarchy directly — an id past the end must
        // abort rather than fabricate a child list.
        let _ = NodeId::new(99).children(&r);
    }

    #[test]
    fn preceding_siblings_starts_with_self_then_walks_backwards() {
        let nodes = tree_5();
        let r = NodeHierarchyRef::from_slice(&nodes);
        assert_eq!(
            NodeId::new(3).preceding_siblings(&r).collect::<Vec<_>>(),
            vec![NodeId::new(3), NodeId::new(2)],
            "the iterator includes the node itself first"
        );
        // A first-born has only itself.
        assert_eq!(
            NodeId::new(2).preceding_siblings(&r).collect::<Vec<_>>(),
            vec![NodeId::new(2)]
        );
    }

    /// ADVERSARIAL: a corrupt hierarchy whose `previous_sibling` points at the
    /// node itself makes the iterator cycle forever. It must not panic — but a
    /// caller that `collect()`s it would hang, so only ever take a bounded
    /// prefix from an untrusted hierarchy.
    #[test]
    fn preceding_siblings_on_self_cycle_repeats_without_panicking() {
        let nodes = vec![
            Node::ROOT,
            Node {
                parent: Some(NodeId::new(0)),
                previous_sibling: Some(NodeId::new(1)), // points at itself
                next_sibling: None,
                last_child: None,
            },
        ];
        let r = NodeHierarchyRef::from_slice(&nodes);
        let first_4: Vec<NodeId> = NodeId::new(1).preceding_siblings(&r).take(4).collect();
        assert_eq!(first_4, vec![NodeId::new(1); 4]);
    }

    // ---------------------------------------------------------------------
    // NodeDataContainer / Ref / RefMut
    // ---------------------------------------------------------------------

    #[test]
    fn data_container_new_and_len_track_the_vec() {
        let c = NodeDataContainer::new(vec![10_u32, 20, 30]);
        assert_eq!(c.len(), 3);
        assert!(!c.is_empty());
        assert_eq!(c.as_ref().len(), 3);
        assert_eq!(c.as_ref().get(NodeId::new(2)), Some(&30));

        let empty: NodeDataContainer<u32> = NodeDataContainer::new(Vec::new());
        assert_eq!(empty.len(), 0);
        assert!(empty.is_empty());
        assert!(empty.as_ref().is_empty());
        assert_eq!(empty.as_ref().get(NodeId::new(0)), None);

        // Default and From<Vec<T>> agree with the explicit constructor.
        assert!(NodeDataContainer::<u32>::default().is_empty());
        assert_eq!(NodeDataContainer::from(vec![1_u32, 2]).len(), 2);
    }

    #[test]
    fn data_container_ref_get_out_of_bounds_returns_none() {
        let data = [1_u8, 2, 3];
        let r = NodeDataContainerRef::from_slice(&data);
        assert_eq!(r.get(NodeId::new(0)), Some(&1));
        assert_eq!(r.get(NodeId::new(3)), None);
        assert_eq!(r.get(NodeId::new(usize::MAX)), None);
    }

    #[test]
    #[should_panic]
    fn data_container_ref_index_out_of_bounds_panics() {
        let data = [1_u8, 2, 3];
        let r = NodeDataContainerRef::from_slice(&data);
        let _ = &r[NodeId::new(3)];
    }

    #[test]
    fn data_container_ref_iterators_cover_all_elements() {
        let data = [1_u8, 2, 3];
        let r = NodeDataContainerRef::from_slice(&data);
        assert_eq!(r.iter().copied().collect::<Vec<_>>(), vec![1, 2, 3]);
        assert_eq!((&r).into_iter().copied().collect::<Vec<_>>(), vec![1, 2, 3]);
        assert_eq!(r.linear_iter().count(), 3);

        let empty: [u8; 0] = [];
        let e = NodeDataContainerRef::from_slice(&empty);
        assert_eq!(e.len(), 0);
        assert!(e.is_empty());
        assert_eq!(e.iter().count(), 0);
        assert_eq!(e.linear_iter().next(), None);
    }

    #[test]
    fn data_container_ref_mut_get_mut_is_bounds_checked() {
        let mut c = NodeDataContainer::new(vec![1_u32, 2, 3]);
        let mut m = c.as_ref_mut();
        assert_eq!(m.get_mut(NodeId::new(3)), None);
        assert_eq!(m.get_mut(NodeId::new(usize::MAX)), None);
        *m.get_mut(NodeId::new(1)).unwrap() = 99;
        m[NodeId::new(2)] = 7;
        assert_eq!(m[NodeId::new(2)], 7);
        assert_eq!(c.internal, vec![1, 99, 7]);
    }

    #[test]
    fn data_container_ref_mut_from_slice_on_empty_slice() {
        let mut empty: [u32; 0] = [];
        let mut m = NodeDataContainerRefMut::from_slice(&mut empty);
        assert_eq!(m.get_mut(NodeId::new(0)), None);
        assert!(m.internal.is_empty());
    }

    #[test]
    #[should_panic]
    fn data_container_ref_mut_index_out_of_bounds_panics() {
        let mut data = [1_u8, 2];
        let m = NodeDataContainerRefMut::from_slice(&mut data);
        let _ = &m[NodeId::new(2)];
    }

    // ---------------------------------------------------------------------
    // transform_nodeid_optional
    // ---------------------------------------------------------------------

    #[test]
    fn transform_nodeid_optional_maps_every_index() {
        let data = [0_u32; 4];
        let r = NodeDataContainerRef::from_slice(&data);
        let out = r.transform_nodeid_optional(|id| Some(id.index() as u32 * 10));
        assert_eq!(out.internal, vec![0, 10, 20, 30]);
        assert_eq!(out.len(), r.len());
    }

    #[test]
    fn transform_nodeid_optional_empty_input_never_calls_the_closure() {
        let empty: [u32; 0] = [];
        let r = NodeDataContainerRef::from_slice(&empty);
        let calls = AtomicUsize::new(0);
        let out = r.transform_nodeid_optional(|_| {
            calls.fetch_add(1, Ordering::SeqCst);
            Some(1_u32)
        });
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(out.is_empty());
    }

    /// CONTRACT TRAP: `None` results are *filtered out*, so the output is
    /// COMPACTED — it is shorter than the input and its positions no longer
    /// line up with the `NodeId`s that produced them. Indexing the result by a
    /// `NodeId` therefore reads the wrong element (or goes out of bounds).
    /// Pinned here so the aliasing behaviour can't change silently.
    #[test]
    fn transform_nodeid_optional_compacts_and_breaks_nodeid_alignment() {
        let data = [0_u32; 5];
        let r = NodeDataContainerRef::from_slice(&data);
        // Keep only even node ids: 0, 2, 4.
        let out = r.transform_nodeid_optional(|id| {
            if id.index() % 2 == 0 {
                Some(id.index() as u32)
            } else {
                None
            }
        });
        assert_eq!(out.len(), 3, "output is compacted, NOT padded to the input len");
        assert_eq!(out.internal, vec![0, 2, 4]);
        // Position 1 holds node 2's value — the result is not index-aligned.
        assert_eq!(out.as_ref().get(NodeId::new(1)), Some(&2));
        assert_eq!(out.as_ref().get(NodeId::new(4)), None);

        // All-None closure yields an empty container rather than panicking.
        let none_out = r.transform_nodeid_optional(|_| -> Option<u32> { None });
        assert!(none_out.is_empty());
    }

    // ---------------------------------------------------------------------
    // NodeId::az_children / az_reverse_children / az_children_collect
    // ---------------------------------------------------------------------

    #[test]
    fn az_children_walks_forwards_and_reverse_walks_backwards() {
        let nodes = tree_5();
        let it = items(&nodes);
        let h = NodeDataContainerRef::from_slice(&it);

        assert_eq!(
            NodeId::new(0).az_children_collect(&h),
            vec![NodeId::new(1), NodeId::new(4)]
        );
        assert_eq!(
            NodeId::new(1).az_children(&h).collect::<Vec<_>>(),
            vec![NodeId::new(2), NodeId::new(3)]
        );
        assert_eq!(
            NodeId::new(1).az_reverse_children(&h).collect::<Vec<_>>(),
            vec![NodeId::new(3), NodeId::new(2)],
            "reverse iteration starts at last_child and walks previous_sibling"
        );
        // Leaves yield nothing in either direction.
        assert_eq!(NodeId::new(2).az_children(&h).count(), 0);
        assert_eq!(NodeId::new(2).az_reverse_children(&h).count(), 0);
        assert!(NodeId::new(3).az_children_collect(&h).is_empty());
    }

    #[test]
    #[should_panic]
    fn az_children_of_out_of_bounds_node_panics_loudly() {
        let nodes = tree_5();
        let it = items(&nodes);
        let h = NodeDataContainerRef::from_slice(&it);
        let _ = NodeId::new(5).az_children(&h);
    }

    /// ADVERSARIAL: a `next_sibling` that points back at the node itself makes
    /// `az_children` an infinite iterator. It must not panic, but
    /// `az_children_collect` on such a hierarchy would allocate until OOM —
    /// so only a bounded prefix is taken here.
    #[test]
    fn az_children_on_sibling_cycle_repeats_without_panicking() {
        let nodes = vec![
            Node {
                last_child: Some(NodeId::new(1)),
                ..Node::ROOT
            },
            Node {
                parent: Some(NodeId::new(0)),
                previous_sibling: None,
                next_sibling: Some(NodeId::new(1)), // points at itself
                last_child: None,
            },
        ];
        let it = items(&nodes);
        let h = NodeDataContainerRef::from_slice(&it);
        let prefix: Vec<NodeId> = NodeId::new(0).az_children(&h).take(5).collect();
        assert_eq!(prefix, vec![NodeId::new(1); 5]);
    }

    // ---------------------------------------------------------------------
    // NodeId::get_nearest_matching_parent
    // ---------------------------------------------------------------------

    #[test]
    fn nearest_matching_parent_skips_non_matching_ancestors() {
        let nodes = tree_5();
        let it = items(&nodes);
        let h = NodeDataContainerRef::from_slice(&it);
        // From node 2, the first ancestor is 1, then the root 0.
        assert_eq!(
            NodeId::new(2).get_nearest_matching_parent(&h, |_| true),
            Some(NodeId::new(1))
        );
        assert_eq!(
            NodeId::new(2).get_nearest_matching_parent(&h, |n| n == NodeId::new(0)),
            Some(NodeId::new(0))
        );
        // The root has no parent at all.
        assert_eq!(
            NodeId::new(0).get_nearest_matching_parent(&h, |_| true),
            None
        );
        // Nothing matches -> None, and the walk terminates.
        assert_eq!(
            NodeId::new(3).get_nearest_matching_parent(&h, |_| false),
            None
        );
    }

    #[test]
    fn nearest_matching_parent_out_of_bounds_self_returns_none() {
        let nodes = tree_5();
        let it = items(&nodes);
        let h = NodeDataContainerRef::from_slice(&it);
        assert_eq!(
            NodeId::new(5).get_nearest_matching_parent(&h, |_| true),
            None
        );
        assert_eq!(
            NodeId::new(usize::MAX).get_nearest_matching_parent(&h, |_| true),
            None
        );
    }

    #[test]
    fn nearest_matching_parent_self_parent_cycle_terminates() {
        // node 1 is its own parent — the node-count cap must break the loop.
        let nodes = vec![
            Node::ROOT,
            Node {
                parent: Some(NodeId::new(1)),
                ..Node::ROOT
            },
        ];
        let it = items(&nodes);
        let h = NodeDataContainerRef::from_slice(&it);
        assert_eq!(
            NodeId::new(1).get_nearest_matching_parent(&h, |_| false),
            None
        );
    }

    /// ADVERSARIAL: a corrupt `parent` index that points past the end of the
    /// arena. The lookup must not panic. Note the predicate is still called
    /// with that out-of-bounds id, so a permissive predicate hands the caller
    /// back an id that will panic when used to index the arena — callers must
    /// bounds-check the returned id, not assume it is valid.
    #[test]
    fn nearest_matching_parent_out_of_bounds_ancestor_does_not_panic() {
        let nodes = vec![
            Node::ROOT,
            Node {
                parent: Some(NodeId::new(99)), // dangling
                ..Node::ROOT
            },
        ];
        let it = items(&nodes);
        let h = NodeDataContainerRef::from_slice(&it);

        // Rejecting predicate: the dangling id fails the `get()` and yields None.
        assert_eq!(
            NodeId::new(1).get_nearest_matching_parent(&h, |_| false),
            None
        );
        // Accepting predicate: the dangling id is returned as-is.
        assert_eq!(
            NodeId::new(1).get_nearest_matching_parent(&h, |_| true),
            Some(NodeId::new(99))
        );
        assert!(h.get(NodeId::new(99)).is_none(), "and it is NOT a valid index");
    }
}
