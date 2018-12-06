//! ID-based node tree

use std::{
    ops::{Index, IndexMut},
    collections::BTreeMap,
};

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
        #[cfg_attr(not(debug_assertions), inline(always))]
        pub(crate) fn new(value: usize) -> Self {

            #[cfg(debug_assertions)] {
                let (new_value, has_overflown) = value.overflowing_add(1);
                if has_overflown {
                    panic!("Overflow when creating DOM Node with ID {}", value);
                } else {
                    NodeId { index: NonZeroUsize::new(new_value).unwrap() }
                }
            }

            #[cfg(not(debug_assertions))] {
                NodeId { index: unsafe { NonZeroUsize::new_unchecked(value + 1) } }
            }
        }

        #[inline]
        pub fn index(&self) -> usize {
            self.index.get() - 1
        }
    }

    impl Add<usize> for NodeId {
        type Output = NodeId;
        fn add(self, other: usize) -> NodeId {
            NodeId::new(self.index() + other)
        }
    }

    impl AddAssign<usize> for NodeId {
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

#[derive(Debug, Copy, Clone, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub struct Node {
    pub parent: Option<NodeId>,
    pub previous_sibling: Option<NodeId>,
    pub next_sibling: Option<NodeId>,
    pub first_child: Option<NodeId>,
    pub last_child: Option<NodeId>,
}

#[derive(Debug, Default, Clone, PartialEq, Hash, Eq)]
pub struct Arena<T> {
    pub(crate) node_layout: NodeHierarchy,
    pub(crate) node_data: NodeDataContainer<T>,
}

#[derive(Debug, Default, Clone, PartialEq, Hash, Eq)]
pub struct NodeHierarchy {
    pub internal: Vec<Node>,
}

impl NodeHierarchy {
    pub fn new(data: Vec<Node>) -> Self {
        Self {
            internal: data,
        }
    }

    pub fn len(&self) -> usize {
        self.internal.len()
    }

    pub fn linear_iter(&self) -> LinearIterator {
        LinearIterator {
            arena_len: self.len(),
            position: 0,
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Hash, Eq)]
pub struct NodeDataContainer<T> {
    pub internal: Vec<T>,
}

impl Index<NodeId> for NodeHierarchy {
    type Output = Node;

    fn index(&self, node_id: NodeId) -> &Node {
        #[cfg(debug_assertions)] {
            self.internal.get(node_id.index()).unwrap()
        } #[cfg(not(debug_assertions))] {
            unsafe { self.internal.get_unchecked(node_id.index()) }
        }
    }
}

impl IndexMut<NodeId> for NodeHierarchy {
    fn index_mut(&mut self, node_id: NodeId) -> &mut Node {
        #[cfg(debug_assertions)] {
            self.internal.get_mut(node_id.index()).unwrap()
        } #[cfg(not(debug_assertions))] {
            unsafe { self.internal.get_unchecked_mut(node_id.index()) }
        }
    }
}

impl<T> NodeDataContainer<T> {
    pub fn new(data: Vec<T>) -> Self {
        Self {
            internal: data,
        }
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

    pub fn linear_iter(&self) -> LinearIterator {
        LinearIterator {
            arena_len: self.len(),
            position: 0,
        }
    }
}

impl<T> Index<NodeId> for NodeDataContainer<T> {
    type Output = T;

    fn index(&self, node_id: NodeId) -> &T {
        #[cfg(debug_assertions)] {
            self.internal.get(node_id.index()).unwrap()
        } #[cfg(not(debug_assertions))] {
            unsafe { self.internal.get_unchecked(node_id.index()) }
        }
    }
}

impl<T> IndexMut<NodeId> for NodeDataContainer<T> {
    fn index_mut(&mut self, node_id: NodeId) -> &mut T {
        #[cfg(debug_assertions)] {
            self.internal.get_mut(node_id.index()).unwrap()
        } #[cfg(not(debug_assertions))] {
            unsafe { self.internal.get_unchecked_mut(node_id.index()) }
        }
    }
}

impl<T> Arena<T> {

    pub fn new() -> Arena<T> {
        Self::with_capacity(0)
    }

    pub fn with_capacity(cap: usize) -> Arena<T> {
        Arena {
            node_layout: NodeHierarchy { internal: Vec::with_capacity(cap) },
            node_data: NodeDataContainer { internal: Vec::<T>::with_capacity(cap) },
        }
    }

    /// Create a new node from its associated data.
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
    pub fn len(&self) -> usize {
        self.node_layout.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Return an iterator over the indices in the internal arenas Vec<T>
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
    pub fn append_arena(&mut self, other: &mut Arena<T>) {
        self.node_layout.internal.append(&mut other.node_layout.internal);
        self.node_data.internal.append(&mut other.node_data.internal);
    }

    /// Transform keeps the relative order of parents / children
    /// but transforms an Arena<T> into an Arena<U>, by running the closure on each of the
    /// items. The `NodeId` for the root is then valid for the newly created `Arena<U>`, too.
    pub(crate) fn transform<U, F>(&self, closure: F) -> Arena<U> where F: Fn(&T, NodeId) -> U {
        // TODO if T: Send (which is usually the case), then we could use rayon here!
        Arena {
            node_layout: self.node_layout.clone(),
            node_data: self.node_data.transform(closure),
        }
    }

    pub(crate) fn node_info_ref(&self, node_id: &NodeId) -> Option<&Node> {
        self.node_layout.internal.get(node_id.index())
    }

    pub(crate) fn node_data_ref(&self, node_id: &NodeId) -> Option<&T> {
        self.node_data.internal.get(node_id.index())
    }

    pub(crate) fn node_info_mut(&self, node_id: &NodeId) -> Option<&Node> {
        self.node_layout.internal.get(node_id.index())
    }

    pub(crate) fn node_data_mut(&self, node_id: &NodeId) -> Option<&T> {
        self.node_data.internal.get(node_id.index())
    }

    pub(crate) fn get_node_hierarchy(&self) -> &NodeHierarchy {
        &self.node_layout
    }

    pub(crate) fn get_node_data(&self) -> &NodeDataContainer<T> {
        &self.node_data
    }

    /// Prints the debug version of the arena, without printing the actual arena
    pub(crate) fn print_tree<F: Fn(&T) -> String + Copy>(&self, format_cb: F) -> String {
        let mut s = String::new();
        if self.len() > 0 {
            self.print_tree_recursive(format_cb, &mut s, NodeId::new(0), 0);
        }
        s
    }

    fn print_tree_recursive<F: Fn(&T) -> String + Copy>(&self, format_cb: F, string: &mut String, current_node_id: NodeId, indent: usize) {
        let node = &self.node_layout[current_node_id];
        let tabs = String::from("\t|").repeat(indent);
        string.push_str(&format!("{}-- {}: {}\n", tabs, current_node_id.index(), format_cb(&self.node_data[current_node_id])));

        if let Some(first_child) = node.first_child {
            self.print_tree_recursive(format_cb, string, first_child, indent + 1);
        }

        if let Some(next_sibling) = node.next_sibling {
            self.print_tree_recursive(format_cb, string, next_sibling, indent);
        }
    }
}

impl<T: Copy> Arena<T> {
    #[inline]
    pub fn get_all_node_ids(&self) -> BTreeMap<NodeId, T> {
        use std::iter::FromIterator;
        BTreeMap::from_iter(self.node_data.internal.iter().enumerate().map(|(i, node)|
            (NodeId::new(i), *node)
        ))
    }
}

impl NodeId {

    /// Return an iterator of references to this node and its ancestors.
    ///
    /// Call `.next().unwrap()` once on the iterator to skip the node itself.
    pub fn ancestors(self, node_layout: &NodeHierarchy) -> Ancestors {
        Ancestors {
            node_layout,
            node: Some(self),
        }
    }

    /// Return an iterator of references to this node and the siblings before it.
    ///
    /// Call `.next().unwrap()` once on the iterator to skip the node itself.
    pub fn preceding_siblings(self, node_layout: &NodeHierarchy) -> PrecedingSiblings {
        PrecedingSiblings {
            node_layout,
            node: Some(self),
        }
    }

    /// Return an iterator of references to this node and the siblings after it.
    ///
    /// Call `.next().unwrap()` once on the iterator to skip the node itself.
    pub fn following_siblings(self, node_layout: &NodeHierarchy) -> FollowingSiblings {
        FollowingSiblings {
            node_layout,
            node: Some(self),
        }
    }

    /// Return an iterator of references to this node’s children.
    pub fn children(self, node_layout: &NodeHierarchy) -> Children {
        Children {
            node_layout,
            node: node_layout[self].first_child,
        }
    }

    /// Return an iterator of references to this node’s children, in reverse order.
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
    pub fn descendants(self, node_layout: &NodeHierarchy) -> Descendants {
        Descendants(self.traverse(node_layout))
    }

    /// Return an iterator of references to this node and its descendants, in tree order.
    pub fn traverse(self, node_layout: &NodeHierarchy) -> Traverse {
        Traverse {
            node_layout,
            root: self,
            next: Some(NodeEdge::Start(self)),
        }
    }

    /// Return an iterator of references to this node and its descendants, in tree order.
    pub fn reverse_traverse(self, node_layout: &NodeHierarchy) -> ReverseTraverse {
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

#[cfg(test)]
mod id_tree_tests {
    use super::*;

    #[test]
    fn drop_allocator() {
        use std::cell::Cell;

        struct DropTracker<'a>(&'a Cell<u32>);
        impl<'a> Drop for DropTracker<'a> {
            fn drop(&mut self) {
                self.0.set(&self.0.get() + 1);
            }
        }

        let drop_counter = Cell::new(0);
        {
            let mut new_counter = 0;
            let arena = &mut Arena::new();
            macro_rules! new {
                () => {
                    {
                        new_counter += 1;
                        arena.new_node((new_counter, DropTracker(&drop_counter)))
                    }
                }
            };

            let a = new!();  // 1
            a.append(new!(), arena);  // 2
            a.append(new!(), arena);  // 3
            a.prepend(new!(), arena);  // 4
            let b = new!();  // 5
            b.append(a, arena);
            a.insert_before(new!(), arena);  // 6
            a.insert_before(new!(), arena);  // 7
            a.insert_after(new!(), arena);  // 8
            a.insert_after(new!(), arena);  // 9
            let c = new!();  // 10
            b.append(c, arena);

            assert_eq!(drop_counter.get(), 0);
            arena[c].previous_sibling().unwrap().detach(arena);
            assert_eq!(drop_counter.get(), 0);

            assert_eq!(b.descendants(arena).map(|node| arena[node].data.0).collect::<Vec<_>>(), [
                5, 6, 7, 1, 4, 2, 3, 9, 10
            ]);
        }

        assert_eq!(drop_counter.get(), 10);
    }


    #[test]
    fn children_ordering() {

        let arena = &mut Arena::new();
        let root = arena.new_node("".to_string());

        root.append(arena.new_node("b".to_string()), arena);
        root.prepend(arena.new_node("a".to_string()), arena);
        root.append(arena.new_node("c".to_string()), arena);

        let children = root.children(arena).map(|node| &*arena[node].data).collect::<Vec<&str>>();
        let reverse_children = root.reverse_children(arena).map(|node| &*arena[node].data).collect::<Vec<&str>>();

        assert_eq!(children, vec!["a", "b", "c"]);
        assert_eq!(reverse_children, vec!["c", "b", "a"]);
    }
}
