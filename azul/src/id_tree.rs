use std::{
    ops::{Index, IndexMut},
    slice::{Iter, IterMut},
};
use dom::NodeData;

pub use self::node_id::NodeId;

// Since private fields are module-based, this prevents any module from accessing
// `NodeId.index` directly. To get the correct node index is by using `NodeId::index()`,
// which subtracts 1 from the ID (because of Option<NodeId> optimizations)
mod node_id {

    use std::{
        fmt,
        num::NonZeroUsize,
        ops::{Add, AddAssign},
    };

    pub(crate) const ROOT_NODE_ID: NodeId = NodeId { index: unsafe { NonZeroUsize::new_unchecked(1) } };

    /// A node identifier within a particular `Arena`.
    #[derive(Copy, Clone, PartialOrd, Ord, PartialEq, Eq, Hash)]
    pub struct NodeId {
        index: NonZeroUsize,
    }

    impl NodeId {
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
        pub(crate) fn new(value: usize) -> Self {
            NodeId { index: unsafe { NonZeroUsize::new_unchecked(value + 1) } }
        }

        #[inline(always)]
        pub fn index(&self) -> usize {
            self.index.get() - 1
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
    pub first_child: Option<NodeId>,
    pub last_child: Option<NodeId>,
}

pub(crate) use self::node_id::ROOT_NODE_ID;

// Node that initializes a Dom
pub(crate) const ROOT_NODE: Node = Node {
    parent: None,
    previous_sibling: None,
    next_sibling: None,
    first_child: None,
    last_child: None,
};

impl Node {
    #[inline]
    pub fn has_parent(&self) -> bool { self.parent.is_some() }
    #[inline]
    pub fn has_previous_sibling(&self) -> bool { self.previous_sibling.is_some() }
    #[inline]
    pub fn has_next_sibling(&self) -> bool { self.next_sibling.is_some() }
    #[inline]
    pub fn has_first_child(&self) -> bool { self.first_child.is_some() }
    #[inline]
    pub fn has_last_child(&self) -> bool { self.last_child.is_some() }
}

#[derive(Debug, Default, Clone, PartialEq, Hash, Eq)]
pub struct Arena<T> {
    pub(crate) node_layout: NodeHierarchy,
    pub(crate) node_data: NodeDataContainer<T>,
}

/// The hierarchy of nodes is stored separately from the actual node content in order
/// to save on memory, since the hierarchy can be re-used across several DOM trees even
/// if the content changes.
#[derive(Debug, Default, Clone, PartialEq, Hash, Eq)]
pub struct NodeHierarchy {
    pub(crate) internal: Vec<Node>,
}

impl NodeHierarchy {

    #[inline]
    pub const fn new(data: Vec<Node>) -> Self {
        Self {
            internal: data,
        }
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.internal.len()
    }

    #[inline]
    pub fn get(&self, id: NodeId) -> Option<&Node> {
        self.internal.get(id.index())
    }

    #[inline]
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
    pub fn get_parents_sorted_by_depth(&self) -> Vec<(usize, NodeId)> {

        let mut non_leaf_nodes = Vec::new();
        let mut current_children = vec![(0, NodeId::new(0))];
        let mut next_children = Vec::new();
        let mut depth = 1;

        loop {

            for id in &current_children {
                for child_id in id.1.children(self).filter(|id| self[*id].first_child.is_some()) {
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

    /// Returns the index in the parent node of a certain NodeId
    /// (starts at 0, i.e. the first node has the index of 0).
    pub fn get_index_in_parent(&self, node_id: NodeId) -> usize {
        node_id.preceding_siblings(&self).count() - 1
    }
}

#[derive(Debug, Default, Clone, PartialEq, Hash, Eq)]
pub struct NodeDataContainer<T> {
    pub(crate) internal: Vec<T>,
}

impl Index<NodeId> for NodeHierarchy {
    type Output = Node;

    #[inline]
    fn index(&self, node_id: NodeId) -> &Node {
        unsafe { self.internal.get_unchecked(node_id.index()) }
    }
}

impl IndexMut<NodeId> for NodeHierarchy {

    #[inline]
    fn index_mut(&mut self, node_id: NodeId) -> &mut Node {
        unsafe { self.internal.get_unchecked_mut(node_id.index()) }
    }
}

impl<T> NodeDataContainer<T> {

    #[inline]
    pub const fn new(data: Vec<T>) -> Self {
        Self { internal: data }
    }

    pub fn len(&self) -> usize { self.internal.len() }

    pub fn transform<U, F>(&self, closure: F) -> NodeDataContainer<U> where F: Fn(&T, NodeId) -> U {
        // TODO if T: Send (which is usually the case), then we could use rayon here!
        NodeDataContainer {
            internal: self.internal.iter().enumerate().map(|(node_id, node)| closure(node, NodeId::new(node_id))).collect(),
        }
    }

    pub fn get(&self, id: NodeId) -> Option<&T> {
        self.internal.get(id.index())
    }

    pub fn iter(&self) -> Iter<T> {
        self.internal.iter()
    }

    pub fn iter_mut(&mut self) -> IterMut<T> {
        self.internal.iter_mut()
    }

    pub fn linear_iter(&self) -> LinearIterator {
        LinearIterator {
            arena_len: self.len(),
            position: 0,
        }
    }
}

impl<T> Index<NodeId> for NodeDataContainer<T> {
    type Output = T;

    #[inline]
    fn index(&self, node_id: NodeId) -> &T {
        unsafe { self.internal.get_unchecked(node_id.index()) }
    }
}

impl<T> IndexMut<NodeId> for NodeDataContainer<T> {

    #[inline]
    fn index_mut(&mut self, node_id: NodeId) -> &mut T {
        unsafe { self.internal.get_unchecked_mut(node_id.index()) }
    }
}

impl<T> Arena<T> {

    #[inline]
    pub fn new() -> Arena<T> {
        // NOTE: This is a separate function, since Vec::new() is a const fn (so this function doesn't allocate)
        Arena {
            node_layout: NodeHierarchy { internal: Vec::new() },
            node_data: NodeDataContainer { internal: Vec::<T>::new() },
        }
    }

    #[inline]
    pub fn with_capacity(cap: usize) -> Arena<T> {
        Arena {
            node_layout: NodeHierarchy { internal: Vec::with_capacity(cap) },
            node_data: NodeDataContainer { internal: Vec::<T>::with_capacity(cap) },
        }
    }

    /// Create a new node from its associated data.
    #[inline]
    pub(crate) fn new_node(&mut self, data: T) -> NodeId {
        let next_index = self.node_layout.len();
        self.node_layout.internal.push(Node {
            parent: None,
            first_child: None,
            last_child: None,
            previous_sibling: None,
            next_sibling: None,
        });
        self.node_data.internal.push(data);
        NodeId::new(next_index)
    }

    // Returns how many nodes there are in the arena
    #[inline]
    pub fn len(&self) -> usize {
        self.node_layout.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Return an iterator over the indices in the internal arenas Vec<T>
    #[inline]
    pub fn linear_iter(&self) -> LinearIterator {
        LinearIterator {
            arena_len: self.node_layout.len(),
            position: 0,
        }
    }

    /// Appends another arena to the end of the current arena
    /// (by simply appending the two Vec of nodes)
    /// Can potentially mess up internal IDs, only use this if you
    /// know what you're doing
    #[inline]
    pub fn append_arena(&mut self, other: &mut Arena<T>) {
        self.node_layout.internal.append(&mut other.node_layout.internal);
        self.node_data.internal.append(&mut other.node_data.internal);
    }

    /// Transform keeps the relative order of parents / children
    /// but transforms an Arena<T> into an Arena<U>, by running the closure on each of the
    /// items. The `NodeId` for the root is then valid for the newly created `Arena<U>`, too.
    #[inline]
    pub(crate) fn transform<U, F>(&self, closure: F) -> Arena<U> where F: Fn(&T, NodeId) -> U {
        // TODO if T: Send (which is usually the case), then we could use rayon here!
        Arena {
            node_layout: self.node_layout.clone(),
            node_data: self.node_data.transform(closure),
        }
    }
}

impl<T> Arena<NodeData<T>> {

    /// Prints the debug version of the arena, without printing the actual arena
    pub(crate) fn print_tree<F: Fn(&NodeData<T>) -> String + Copy>(&self, format_cb: F) -> String {
        let mut s = String::new();
        if self.len() > 0 {
            self.print_tree_recursive(format_cb, &mut s, NodeId::new(0), 0);
        }
        s
    }

    fn print_tree_recursive<F: Fn(&NodeData<T>) -> String + Copy>(&self, format_cb: F, string: &mut String, current_node_id: NodeId, indent: usize) {
        let node = &self.node_layout[current_node_id];
        let tabs = String::from("    ").repeat(indent);
        string.push_str(&format!("{}{}\n", tabs, format_cb(&self.node_data[current_node_id])));

        if let Some(first_child) = node.first_child {
            self.print_tree_recursive(format_cb, string, first_child, indent + 1);
            if node.last_child.is_some() {
                string.push_str(&format!("{}</{}>\n", tabs, self.node_data[current_node_id].node_type.get_path()));
            }
        }

        if let Some(next_sibling) = node.next_sibling {
            self.print_tree_recursive(format_cb, string, next_sibling, indent);
        }
    }
}

impl NodeId {

    /// Return an iterator of references to this node and its ancestors.
    ///
    /// Call `.next().unwrap()` once on the iterator to skip the node itself.
    #[inline]
    pub const fn ancestors(self, node_layout: &NodeHierarchy) -> Ancestors {
        Ancestors {
            node_layout,
            node: Some(self),
        }
    }

    /// Return an iterator of references to this node and the siblings before it.
    ///
    /// Call `.next().unwrap()` once on the iterator to skip the node itself.
    #[inline]
    pub const fn preceding_siblings(self, node_layout: &NodeHierarchy) -> PrecedingSiblings {
        PrecedingSiblings {
            node_layout,
            node: Some(self),
        }
    }

    /// Return an iterator of references to this node and the siblings after it.
    ///
    /// Call `.next().unwrap()` once on the iterator to skip the node itself.
    #[inline]
    pub const fn following_siblings(self, node_layout: &NodeHierarchy) -> FollowingSiblings {
        FollowingSiblings {
            node_layout,
            node: Some(self),
        }
    }

    /// Return an iterator of references to this node’s children.
    #[inline]
    pub fn children(self, node_layout: &NodeHierarchy) -> Children {
        Children {
            node_layout,
            node: node_layout[self].first_child,
        }
    }

    /// Return an iterator of references to this node’s children, in reverse order.
    #[inline]
    pub fn reverse_children(self, node_layout: &NodeHierarchy) -> ReverseChildren {
        ReverseChildren {
            node_layout,
            node: node_layout[self].last_child,
        }
    }

    /// Return an iterator of references to this node and its descendants, in tree order.
    ///
    /// Parent nodes appear before the descendants.
    /// Call `.next().unwrap()` once on the iterator to skip the node itself.
    #[inline]
    pub const fn descendants(self, node_layout: &NodeHierarchy) -> Descendants {
        Descendants(self.traverse(node_layout))
    }

    /// Return an iterator of references to this node and its descendants, in tree order.
    #[inline]
    pub const fn traverse(self, node_layout: &NodeHierarchy) -> Traverse {
        Traverse {
            node_layout,
            root: self,
            next: Some(NodeEdge::Start(self)),
        }
    }

    /// Return an iterator of references to this node and its descendants, in tree order.
    #[inline]
    pub const fn reverse_traverse(self, node_layout: &NodeHierarchy) -> ReverseTraverse {
        ReverseTraverse {
            node_layout,
            root: self,
            next: Some(NodeEdge::End(self)),
        }
    }
}


macro_rules! impl_node_iterator {
    ($name: ident, $next: expr) => {
        impl<'a> Iterator for $name<'a> {
            type Item = NodeId;

            fn next(&mut self) -> Option<NodeId> {
                match self.node.take() {
                    Some(node) => {
                        self.node = $next(&self.node_layout[node]);
                        Some(node)
                    }
                    None => None
                }
            }
        }
    }
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
        if self.arena_len < 1 || self.position > (self.arena_len - 1){
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
    node_layout: &'a NodeHierarchy,
    node: Option<NodeId>,
}

impl_node_iterator!(Ancestors, |node: &Node| node.parent);

/// An iterator of references to the siblings before a given node.
pub struct PrecedingSiblings<'a> {
    node_layout: &'a NodeHierarchy,
    node: Option<NodeId>,
}

impl_node_iterator!(PrecedingSiblings, |node: &Node| node.previous_sibling);

/// An iterator of references to the siblings after a given node.
pub struct FollowingSiblings<'a> {
    node_layout: &'a NodeHierarchy,
    node: Option<NodeId>,
}

impl_node_iterator!(FollowingSiblings, |node: &Node| node.next_sibling);

/// An iterator of references to the children of a given node.
pub struct Children<'a> {
    node_layout: &'a NodeHierarchy,
    node: Option<NodeId>,
}

impl_node_iterator!(Children, |node: &Node| node.next_sibling);

/// An iterator of references to the children of a given node, in reverse order.
pub struct ReverseChildren<'a> {
    node_layout: &'a NodeHierarchy,
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
                None => return None
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
    node_layout: &'a NodeHierarchy,
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
                        match self.node_layout[node].first_child {
                            Some(first_child) => Some(NodeEdge::Start(first_child)),
                            None => Some(NodeEdge::End(node.clone()))
                        }
                    }
                    NodeEdge::End(node) => {
                        if node == self.root {
                            None
                        } else {
                            match self.node_layout[node].next_sibling {
                                Some(next_sibling) => Some(NodeEdge::Start(next_sibling)),
                                None => match self.node_layout[node].parent {
                                    Some(parent) => Some(NodeEdge::End(parent)),

                                    // `node.parent()` here can only be `None`
                                    // if the tree has been modified during iteration,
                                    // but silently stopping iteration
                                    // seems a more sensible behavior than panicking.
                                    None => None
                                }
                            }
                        }
                    }
                };
                Some(item)
            }
            None => None
        }
    }
}

/// An iterator of references to a given node and its descendants, in reverse tree order.
pub struct ReverseTraverse<'a> {
    node_layout: &'a NodeHierarchy,
    root: NodeId,
    next: Option<NodeEdge<NodeId>>,
}

impl<'a> Iterator for ReverseTraverse<'a> {
    type Item = NodeEdge<NodeId>;

    fn next(&mut self) -> Option<NodeEdge<NodeId>> {
        match self.next.take() {
            Some(item) => {
                self.next = match item {
                    NodeEdge::End(node) => {
                        match self.node_layout[node].last_child {
                            Some(last_child) => Some(NodeEdge::End(last_child)),
                            None => Some(NodeEdge::Start(node.clone()))
                        }
                    }
                    NodeEdge::Start(node) => {
                        if node == self.root {
                            None
                        } else {
                            match self.node_layout[node].previous_sibling {
                                Some(previous_sibling) => Some(NodeEdge::End(previous_sibling)),
                                None => match self.node_layout[node].parent {
                                    Some(parent) => Some(NodeEdge::Start(parent)),

                                    // `node.parent()` here can only be `None`
                                    // if the tree has been modified during iteration,
                                    // but silently stopping iteration
                                    // seems a more sensible behavior than panicking.
                                    None => None
                                }
                            }
                        }
                    }
                };
                Some(item)
            }
            None => None
        }
    }
}