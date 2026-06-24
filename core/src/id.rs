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
        #[inline]
        fn add(self, other: usize) -> Self {
            Self::new(self.inner + other)
        }
    }

    impl AddAssign<usize> for NodeId {
        #[inline]
        fn add_assign(&mut self, other: usize) {
            *self = *self + other;
        }
    }

    impl fmt::Display for NodeId {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "{}", self.inner)
        }
    }

    impl fmt::Debug for NodeId {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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
        let mut non_leaf_nodes = Vec::new();
        let mut current_children = vec![(0, NodeId::new(0))];
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
    pub fn iter(&self) -> Iter<T> {
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
pub struct PrecedingSiblings<'a> {
    node_hierarchy: &'a NodeHierarchyRef<'a>,
    node: Option<NodeId>,
}

impl_node_iterator!(PrecedingSiblings, |node: &Node| node.previous_sibling);

/// Special iterator for using `NodeDataContainerRef`<AzNode> instead of `NodeHierarchy`
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
        let mut current_node = node_hierarchy[self].parent_id()?;
        loop {
            if predicate(current_node) {
                return Some(current_node);
            }
            current_node = node_hierarchy[current_node].parent_id()?;
        }
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
pub struct Children<'a> {
    node_hierarchy: &'a NodeHierarchyRef<'a>,
    node: Option<NodeId>,
}

impl_node_iterator!(Children, |node: &Node| node.next_sibling);
