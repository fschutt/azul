use alloc::vec::Vec;
use core::{
    ops::{Index, IndexMut},
    slice::Iter,
};

pub use self::node_id::NodeId;
use crate::styled_dom::NodeHierarchyItem;
pub type NodeDepths = Vec<(usize, NodeId)>;
#[cfg(not(feature = "std"))]
use alloc::string::ToString;

// Since private fields are module-based, this prevents any module from accessing
// `NodeId.index` directly. To get the correct node index is by using `NodeId::index()`,
// which subtracts 1 from the ID (because of Option<NodeId> optimizations)
mod node_id {

    use alloc::vec::Vec;
    use core::{
        fmt,
        num::NonZeroUsize,
        ops::{Add, AddAssign},
    };

    /// A node identifier within a particular `Arena`.
    #[derive(Copy, Clone, PartialOrd, Ord, PartialEq, Eq, Hash)]
    pub struct NodeId {
        index: NonZeroUsize,
    }

    impl NodeId {
        pub const ZERO: NodeId = NodeId {
            index: unsafe { NonZeroUsize::new_unchecked(1) },
        };

        /// **NOTE**: In debug mode, it panics on overflow, since having a
        /// pointer that is zero is undefined behaviour (it would basically be
        /// cast to a `None`), which is incorrect, so we rather panic on overflow
        /// to prevent that.
        ///
        /// To trigger an overflow however, you'd need more that 4 billion DOM nodes -
        /// it is more likely that you run out of RAM before you do that. The only thing
        /// that could lead to an overflow would be a bug. Therefore, overflow-checking is
        /// disabled in release mode.
        #[inline(always)]
        pub const fn new(value: usize) -> Self {
            NodeId {
                index: unsafe { NonZeroUsize::new_unchecked(value.saturating_add(1)) },
            }
        }

        pub const fn from_usize(value: usize) -> Option<Self> {
            match value {
                0 => None,
                i => Some(NodeId::new(i - 1)),
            }
        }

        pub const fn into_usize(val: &Option<Self>) -> usize {
            match val {
                None => 0,
                Some(s) => s.index.get(),
            }
        }

        #[inline(always)]
        pub fn index(&self) -> usize {
            self.index.get() - 1
        }

        /// Return an iterator of references to this node’s children.
        #[inline]
        pub fn range(start: Self, end: Self) -> Vec<NodeId> {
            (start.index()..end.index())
                .map(|u| NodeId::new(u))
                .collect()
        }
    }

    impl Add<usize> for NodeId {
        type Output = NodeId;
        #[inline(always)]
        fn add(self, other: usize) -> NodeId {
            NodeId::new(self.index() + other)
        }
    }

    impl AddAssign<usize> for NodeId {
        #[inline(always)]
        fn add_assign(&mut self, other: usize) {
            *self = *self + other;
        }
    }

    impl fmt::Display for NodeId {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "{}", self.index())
        }
    }

    impl fmt::Debug for NodeId {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "NodeId({})", self.index())
        }
    }
}

/// Hierarchical information about a node (stores the indicies of the parent / child nodes).
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

// Node that initializes a Dom
pub const ROOT_NODE: Node = Node {
    parent: None,
    previous_sibling: None,
    next_sibling: None,
    last_child: None,
};

impl Node {
    pub const ROOT: Node = ROOT_NODE;

    #[inline]
    pub const fn has_parent(&self) -> bool {
        self.parent.is_some()
    }
    #[inline]
    pub const fn has_previous_sibling(&self) -> bool {
        self.previous_sibling.is_some()
    }
    #[inline]
    pub const fn has_next_sibling(&self) -> bool {
        self.next_sibling.is_some()
    }
    #[inline]
    pub const fn has_first_child(&self) -> bool {
        self.last_child.is_some() /* last_child and first_child are always set together */
    }
    #[inline]
    pub const fn has_last_child(&self) -> bool {
        self.last_child.is_some()
    }

    #[inline]
    pub(crate) fn get_first_child(&self, current_node_id: NodeId) -> Option<NodeId> {
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
    #[inline(always)]
    pub const fn new(data: Vec<Node>) -> Self {
        Self { internal: data }
    }

    #[inline(always)]
    pub fn as_ref<'a>(&'a self) -> NodeHierarchyRef<'a> {
        NodeHierarchyRef {
            internal: &self.internal[..],
        }
    }

    #[inline(always)]
    pub fn as_ref_mut<'a>(&'a mut self) -> NodeHierarchyRefMut<'a> {
        NodeHierarchyRefMut {
            internal: &mut self.internal[..],
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

#[derive(Debug, PartialEq, Hash, Eq)]
pub struct NodeHierarchyRefMut<'a> {
    pub internal: &'a mut [Node],
}

impl<'a> NodeHierarchyRef<'a> {
    #[inline(always)]
    pub fn from_slice(data: &'a [Node]) -> NodeHierarchyRef<'a> {
        NodeHierarchyRef { internal: data }
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.internal.len()
    }

    #[inline(always)]
    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.internal.get(id.index())
    }

    #[inline(always)]
    pub fn linear_iter(&self) -> LinearIterator {
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
    pub fn get_parents_sorted_by_depth(&self) -> NodeDepths {
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
            } else {
                current_children.extend(&mut next_children.drain(..));
                depth += 1;
            }
        }

        non_leaf_nodes
    }

    /// Returns the number of all subtree items - runtime O(1)
    #[inline]
    pub fn subtree_len(&self, parent_id: NodeId) -> usize {
        let self_item_index = parent_id.index();
        let next_item_index = match self[parent_id].next_sibling {
            None => self.len(),
            Some(s) => s.index(),
        };
        next_item_index - self_item_index - 1
    }

    /// Returns the index in the parent node of a certain NodeId
    /// (starts at 0, i.e. the first node has the index of 0).
    #[inline]
    pub fn get_index_in_parent(&self, node_id: NodeId) -> usize {
        node_id.preceding_siblings(&self).count() - 1
    }
}

impl<'a> NodeHierarchyRefMut<'a> {
    pub fn from_slice(data: &'a mut [Node]) -> NodeHierarchyRefMut<'a> {
        NodeHierarchyRefMut { internal: data }
    }
}

#[derive(Debug, Clone, PartialEq, Hash, Eq, PartialOrd, Ord)]
pub struct NodeDataContainer<T> {
    pub internal: Vec<T>,
}

impl<T> From<Vec<T>> for NodeDataContainer<T> {
    fn from(v: Vec<T>) -> NodeDataContainer<T> {
        NodeDataContainer { internal: v }
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

impl<'a, T> NodeDataContainerRefMut<'a, T> {
    pub fn as_borrowing_ref<'b>(&'b self) -> NodeDataContainerRef<'b, T> {
        NodeDataContainerRef {
            internal: &*self.internal,
        }
    }
}

impl<T> Default for NodeDataContainer<T> {
    fn default() -> Self {
        Self {
            internal: Vec::new(),
        }
    }
}

impl<'a> Index<NodeId> for NodeHierarchyRef<'a> {
    type Output = Node;

    #[inline(always)]
    fn index(&self, node_id: NodeId) -> &Node {
        &self.internal[node_id.index()]
    }
}

impl<'a> Index<NodeId> for NodeHierarchyRefMut<'a> {
    type Output = Node;

    #[inline(always)]
    fn index(&self, node_id: NodeId) -> &Node {
        &self.internal[node_id.index()]
    }
}

impl<'a> IndexMut<NodeId> for NodeHierarchyRefMut<'a> {
    #[inline(always)]
    fn index_mut(&mut self, node_id: NodeId) -> &mut Node {
        &mut self.internal[node_id.index()]
    }
}

impl<T> NodeDataContainer<T> {
    #[inline(always)]
    pub const fn new(data: Vec<T>) -> Self {
        Self { internal: data }
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.internal.len() == 0
    }

    #[inline(always)]
    pub fn as_ref<'a>(&'a self) -> NodeDataContainerRef<'a, T> {
        NodeDataContainerRef {
            internal: &self.internal[..],
        }
    }

    #[inline(always)]
    pub fn as_ref_mut<'a>(&'a mut self) -> NodeDataContainerRefMut<'a, T> {
        NodeDataContainerRefMut {
            internal: &mut self.internal[..],
        }
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.internal.len()
    }
}

impl<'a, T: 'a> NodeDataContainerRefMut<'a, T> {
    #[inline(always)]
    pub fn from_slice(data: &'a mut [T]) -> NodeDataContainerRefMut<'a, T> {
        NodeDataContainerRefMut { internal: data }
    }
}

impl<'a, T: 'a> NodeDataContainerRefMut<'a, T> {
    #[inline(always)]
    pub fn get_mut(&mut self, id: NodeId) -> Option<&mut T> {
        self.internal.get_mut(id.index())
    }
    #[inline(always)]
    pub fn get_mut_extended_lifetime(&'a mut self, id: NodeId) -> Option<&'a mut T> {
        self.internal.get_mut(id.index())
    }
}

impl<'a, T: Send + 'a> NodeDataContainerRefMut<'a, T> {
    pub fn transform_multithread<U: Send, F: Send + Sync>(
        &mut self,
        closure: F,
    ) -> NodeDataContainer<U>
    where
        F: Fn(&mut T, NodeId) -> U,
    {
        NodeDataContainer {
            internal: self
                .internal
                .iter_mut()
                .enumerate()
                .map(|(node_id, node)| closure(node, NodeId::new(node_id)))
                .collect::<Vec<U>>(),
        }
    }

    pub fn transform_multithread_optional<U: Send, F: Send + Sync>(&mut self, closure: F) -> Vec<U>
    where
        F: Fn(&mut T, NodeId) -> Option<U>,
    {
        self.internal
            .iter_mut()
            .enumerate()
            .filter_map(|(node_id, node)| closure(node, NodeId::new(node_id)))
            .collect::<Vec<U>>()
    }
}

impl<'a, T: Send + 'a> NodeDataContainerRef<'a, T> {
    pub fn transform_nodeid<U: Send, F: Send + Sync>(&self, closure: F) -> NodeDataContainer<U>
    where
        F: Fn(NodeId) -> U,
    {
        let len = self.len();
        NodeDataContainer {
            internal: (0..len)
                .into_iter()
                .map(|node_id| closure(NodeId::new(node_id)))
                .collect::<Vec<U>>(),
        }
    }

    pub fn transform_nodeid_multithreaded_optional<U: Send, F: Send + Sync>(
        &self,
        closure: F,
    ) -> NodeDataContainer<U>
    where
        F: Fn(NodeId) -> Option<U>,
    {
        let len = self.len();
        NodeDataContainer {
            internal: (0..len)
                .into_iter()
                .filter_map(|node_id| closure(NodeId::new(node_id)))
                .collect::<Vec<U>>(),
        }
    }
}

impl<'a, T: 'a> NodeDataContainerRef<'a, T> {
    #[inline(always)]
    pub fn get_extended_lifetime(&self, id: NodeId) -> Option<&'a T> {
        self.internal.get(id.index())
    }

    #[inline(always)]
    pub fn from_slice(data: &'a [T]) -> NodeDataContainerRef<'a, T> {
        NodeDataContainerRef { internal: data }
    }

    #[inline(always)]
    pub fn len(&self) -> usize {
        self.internal.len()
    }

    pub fn transform_singlethread<U, F>(&self, mut closure: F) -> NodeDataContainer<U>
    where
        F: FnMut(&T, NodeId) -> U,
    {
        // TODO if T: Send (which is usually the case), then we could use rayon here!
        NodeDataContainer {
            internal: self
                .internal
                .iter()
                .enumerate()
                .map(|(node_id, node)| closure(node, NodeId::new(node_id)))
                .collect(),
        }
    }

    #[inline(always)]
    pub fn get(&self, id: NodeId) -> Option<&T> {
        self.internal.get(id.index())
    }

    #[inline(always)]
    pub fn iter(&self) -> Iter<T> {
        self.internal.iter()
    }

    #[inline(always)]
    pub fn linear_iter(&self) -> LinearIterator {
        LinearIterator {
            arena_len: self.len(),
            position: 0,
        }
    }
}

impl<'a, T> Index<NodeId> for NodeDataContainerRef<'a, T> {
    type Output = T;

    #[inline(always)]
    fn index(&self, node_id: NodeId) -> &T {
        &self.internal[node_id.index()]
    }
}

impl<'a, T> Index<NodeId> for NodeDataContainerRefMut<'a, T> {
    type Output = T;

    #[inline(always)]
    fn index(&self, node_id: NodeId) -> &T {
        &self.internal[node_id.index()]
    }
}

impl<'a, T> IndexMut<NodeId> for NodeDataContainerRefMut<'a, T> {
    #[inline(always)]
    fn index_mut(&mut self, node_id: NodeId) -> &mut T {
        &mut self.internal[node_id.index()]
    }
}

impl NodeId {
    /// Return an iterator of references to this node and its ancestors.
    ///
    /// Call `.next().unwrap()` once on the iterator to skip the node itself.
    #[inline]
    pub const fn ancestors<'a>(self, node_hierarchy: &'a NodeHierarchyRef<'a>) -> Ancestors<'a> {
        Ancestors {
            node_hierarchy,
            node: Some(self),
        }
    }

    /// Return an iterator of references to this node and the siblings before it.
    ///
    /// Call `.next().unwrap()` once on the iterator to skip the node itself.
    #[inline]
    pub const fn preceding_siblings<'a>(
        self,
        node_hierarchy: &'a NodeHierarchyRef<'a>,
    ) -> PrecedingSiblings<'a> {
        PrecedingSiblings {
            node_hierarchy,
            node: Some(self),
        }
    }

    /// Return an iterator of references to this node and the siblings after it.
    ///
    /// Call `.next().unwrap()` once on the iterator to skip the node itself.
    #[inline]
    pub const fn following_siblings<'a>(
        self,
        node_hierarchy: &'a NodeHierarchyRef<'a>,
    ) -> FollowingSiblings<'a> {
        FollowingSiblings {
            node_hierarchy,
            node: Some(self),
        }
    }

    /// Return an iterator of references to this node’s children.
    #[inline]
    pub fn children<'a>(self, node_hierarchy: &'a NodeHierarchyRef<'a>) -> Children<'a> {
        Children {
            node_hierarchy,
            node: node_hierarchy[self].get_first_child(self),
        }
    }

    /// Return an iterator of references to this node’s children, in reverse order.
    #[inline]
    pub fn reverse_children<'a>(
        self,
        node_hierarchy: &'a NodeHierarchyRef<'a>,
    ) -> ReverseChildren<'a> {
        ReverseChildren {
            node_hierarchy,
            node: node_hierarchy[self].last_child,
        }
    }

    /// Return an iterator of references to this node and its descendants, in tree order.
    ///
    /// Parent nodes appear before the descendants.
    /// Call `.next().unwrap()` once on the iterator to skip the node itself.
    #[inline]
    pub const fn descendants<'a>(
        self,
        node_hierarchy: &'a NodeHierarchyRef<'a>,
    ) -> Descendants<'a> {
        Descendants(self.traverse(node_hierarchy))
    }

    /// Return an iterator of references to this node and its descendants, in tree order.
    #[inline]
    pub const fn traverse<'a>(self, node_hierarchy: &'a NodeHierarchyRef<'a>) -> Traverse<'a> {
        Traverse {
            node_hierarchy,
            root: self,
            next: Some(NodeEdge::Start(self)),
        }
    }

    /// Return an iterator of references to this node and its descendants, in tree order.
    #[inline]
    pub const fn reverse_traverse<'a>(
        self,
        node_hierarchy: &'a NodeHierarchyRef<'a>,
    ) -> ReverseTraverse<'a> {
        ReverseTraverse {
            node_hierarchy,
            root: self,
            next: Some(NodeEdge::End(self)),
        }
    }
}

macro_rules! impl_node_iterator {
    ($name:ident, $next:expr) => {
        impl<'a> Iterator for $name<'a> {
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
#[derive(Debug, Copy, Clone)]
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

/// An iterator of references to the ancestors a given node.
pub struct Ancestors<'a> {
    node_hierarchy: &'a NodeHierarchyRef<'a>,
    node: Option<NodeId>,
}

impl_node_iterator!(Ancestors, |node: &Node| node.parent);

/// An iterator of references to the siblings before a given node.
pub struct PrecedingSiblings<'a> {
    node_hierarchy: &'a NodeHierarchyRef<'a>,
    node: Option<NodeId>,
}

impl_node_iterator!(PrecedingSiblings, |node: &Node| node.previous_sibling);

/// An iterator of references to the siblings after a given node.
pub struct FollowingSiblings<'a> {
    node_hierarchy: &'a NodeHierarchyRef<'a>,
    node: Option<NodeId>,
}

impl_node_iterator!(FollowingSiblings, |node: &Node| node.next_sibling);

/// Special iterator for using NodeDataContainerRef<AzNode> instead of NodeHierarchy
pub struct AzChildren<'a> {
    node_hierarchy: &'a NodeDataContainerRef<'a, NodeHierarchyItem>,
    node: Option<NodeId>,
}

impl<'a> Iterator for AzChildren<'a> {
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

/// Special iterator for using NodeDataContainerRef<AzNode> instead of NodeHierarchy
pub struct AzReverseChildren<'a> {
    node_hierarchy: &'a NodeDataContainerRef<'a, NodeHierarchyItem>,
    node: Option<NodeId>,
}

impl<'a> Iterator for AzReverseChildren<'a> {
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
    // Traverse up through the hierarchy a node matching the predicate is found
    //
    // Necessary to resolve the last positioned (= relative)
    // element of an absolute ndoe
    pub fn get_nearest_matching_parent<'a, F>(
        self,
        node_hierarchy: &'a NodeDataContainerRef<'a, NodeHierarchyItem>,
        predicate: F,
    ) -> Option<NodeId>
    where
        F: Fn(NodeId) -> bool,
    {
        let mut current_node = node_hierarchy[self].parent_id()?;
        loop {
            match predicate(current_node) {
                true => {
                    return Some(current_node);
                }
                false => {
                    current_node = node_hierarchy[current_node].parent_id()?;
                }
            }
        }
    }

    /// Return the children of this node (necessary for parallel iteration over children)
    #[inline]
    pub fn az_children_collect<'a>(
        self,
        node_hierarchy: &'a NodeDataContainerRef<'a, NodeHierarchyItem>,
    ) -> Vec<NodeId> {
        self.az_children(node_hierarchy).collect()
    }

    /// Return an iterator of references to this node’s children.
    #[inline]
    pub fn az_children<'a>(
        self,
        node_hierarchy: &'a NodeDataContainerRef<'a, NodeHierarchyItem>,
    ) -> AzChildren<'a> {
        AzChildren {
            node_hierarchy,
            node: node_hierarchy[self].first_child_id(self),
        }
    }

    /// Return an iterator of references to this node’s children.
    #[inline]
    pub fn az_reverse_children<'a>(
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

/// An iterator of references to the children of a given node, in reverse order.
pub struct ReverseChildren<'a> {
    node_hierarchy: &'a NodeHierarchyRef<'a>,
    node: Option<NodeId>,
}

impl_node_iterator!(ReverseChildren, |node: &Node| node.previous_sibling);

/// An iterator of references to a given node and its descendants, in tree order.
pub struct Descendants<'a>(Traverse<'a>);

impl<'a> Iterator for Descendants<'a> {
    type Item = NodeId;

    fn next(&mut self) -> Option<NodeId> {
        loop {
            match self.0.next() {
                Some(NodeEdge::Start(node)) => return Some(node),
                Some(NodeEdge::End(_)) => {}
                None => return None,
            }
        }
    }
}

#[derive(Debug, Clone)]
pub enum NodeEdge<T> {
    /// Indicates that start of a node that has children.
    /// Yielded by `Traverse::next` before the node’s descendants.
    /// In HTML or XML, this corresponds to an opening tag like `<div>`
    Start(T),

    /// Indicates that end of a node that has children.
    /// Yielded by `Traverse::next` after the node’s descendants.
    /// In HTML or XML, this corresponds to a closing tag like `</div>`
    End(T),
}

impl<T> NodeEdge<T> {
    pub fn inner_value(self) -> T {
        use self::NodeEdge::*;
        match self {
            Start(t) => t,
            End(t) => t,
        }
    }
}

/// An iterator of references to a given node and its descendants, in tree order.
pub struct Traverse<'a> {
    node_hierarchy: &'a NodeHierarchyRef<'a>,
    root: NodeId,
    next: Option<NodeEdge<NodeId>>,
}

impl<'a> Iterator for Traverse<'a> {
    type Item = NodeEdge<NodeId>;

    fn next(&mut self) -> Option<NodeEdge<NodeId>> {
        match self.next.take() {
            Some(item) => {
                self.next = match item {
                    NodeEdge::Start(node) => {
                        match self.node_hierarchy[node].get_first_child(node) {
                            Some(first_child) => Some(NodeEdge::Start(first_child)),
                            None => Some(NodeEdge::End(node.clone())),
                        }
                    }
                    NodeEdge::End(node) => {
                        if node == self.root {
                            None
                        } else {
                            match self.node_hierarchy[node].next_sibling {
                                Some(next_sibling) => Some(NodeEdge::Start(next_sibling)),
                                None => match self.node_hierarchy[node].parent {
                                    Some(parent) => Some(NodeEdge::End(parent)),

                                    // `node.parent()` here can only be `None`
                                    // if the tree has been modified during iteration,
                                    // but silently stopping iteration
                                    // seems a more sensible behavior than panicking.
                                    None => None,
                                },
                            }
                        }
                    }
                };
                Some(item)
            }
            None => None,
        }
    }
}

/// An iterator of references to a given node and its descendants, in reverse tree order.
pub struct ReverseTraverse<'a> {
    node_hierarchy: &'a NodeHierarchyRef<'a>,
    root: NodeId,
    next: Option<NodeEdge<NodeId>>,
}

impl<'a> Iterator for ReverseTraverse<'a> {
    type Item = NodeEdge<NodeId>;

    fn next(&mut self) -> Option<NodeEdge<NodeId>> {
        match self.next.take() {
            Some(item) => {
                self.next = match item {
                    NodeEdge::End(node) => match self.node_hierarchy[node].last_child {
                        Some(last_child) => Some(NodeEdge::End(last_child)),
                        None => Some(NodeEdge::Start(node.clone())),
                    },
                    NodeEdge::Start(node) => {
                        if node == self.root {
                            None
                        } else {
                            match self.node_hierarchy[node].previous_sibling {
                                Some(previous_sibling) => Some(NodeEdge::End(previous_sibling)),
                                None => match self.node_hierarchy[node].parent {
                                    Some(parent) => Some(NodeEdge::Start(parent)),

                                    // `node.parent()` here can only be `None`
                                    // if the tree has been modified during iteration,
                                    // but silently stopping iteration
                                    // seems a more sensible behavior than panicking.
                                    None => None,
                                },
                            }
                        }
                    }
                };
                Some(item)
            }
            None => None,
        }
    }
}
