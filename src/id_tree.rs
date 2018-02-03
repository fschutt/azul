//! ID-based node tree

use std::mem;
use std::ops::{Index, IndexMut};
use std::fmt;

/// A node identifier within a particular `Arena`.
#[derive(PartialOrd, Ord, PartialEq, Eq, Copy, Clone, Debug, Hash)]
pub struct NodeId {
    index: usize,  // FIXME: use NonZero to optimize the size of Option<NodeId>
}

#[derive(Clone)]
pub struct Node<T> {
    // Keep these private (with read-only accessors) so that we can keep them consistent.
    // E.g. the parent of a node’s child is that node.
    parent: Option<NodeId>,
    previous_sibling: Option<NodeId>,
    next_sibling: Option<NodeId>,
    first_child: Option<NodeId>,
    last_child: Option<NodeId>,
    pub data: T,
}

impl<T> Node<T> {
    pub fn print_node(&self) {
        println!("Node {{ 
    parent: {:?}, 
    previous_sibling: {:?}, 
    next_sibling: {:?}, 
    first_child: {:?}, 
    last_child: {:?}, 
    data: <not printable> 
}}", 
        self.parent, 
        self.previous_sibling, 
        self.next_sibling, 
        self.first_child, 
        self.last_child)
    }
}
impl<T: fmt::Debug> Node<T> {
    pub fn print_node_debug(&self) {
        println!("Node {{ 
    parent: {:?}, 
    previous_sibling: {:?}, 
    next_sibling: {:?}, 
    first_child: {:?}, 
    last_child: {:?}, 
    data: {:?} 
}}", 
        self.parent, 
        self.previous_sibling, 
        self.next_sibling, 
        self.first_child, 
        self.last_child, 
        self.data)
    }
}

#[derive(Clone)]
pub struct Arena<T> {
    nodes: Vec<Node<T>>,
}

impl<T> Arena<T> {
    pub fn new() -> Arena<T> {
        Arena {
            nodes: Vec::new(),
        }
    }

    /// Create a new node from its associated data.
    pub fn new_node(&mut self, data: T) -> NodeId {
        let next_index = self.nodes.len();
        self.nodes.push(Node {
            parent: None,
            first_child: None,
            last_child: None,
            previous_sibling: None,
            next_sibling: None,
            data: data,
        });
        NodeId {
            index: next_index,
        }
    }

    // Useful for debugging - returns how many 
    // nodes there are in the arena
    pub fn nodes_len(&self) -> usize {
        self.nodes.len()
    }
}

impl<T: fmt::Debug> Arena<T> {
    pub fn print_arena_debug(&self) {
        for node in &self.nodes {
            node.print_node_debug();
        }
    }
}

trait GetPairMut<T> {
    /// Get mutable references to two distinct nodes
    ///
    /// Panic
    /// -----
    ///
    /// Panics if the two given IDs are the same.
    fn get_pair_mut(&mut self, a: usize, b: usize, same_index_error_message: &'static str)
                    -> (&mut T, &mut T);
}

impl<T> GetPairMut<T> for Vec<T> {
    fn get_pair_mut(&mut self, a: usize, b: usize, same_index_error_message: &'static str)
                    -> (&mut T, &mut T) {
        if a == b {
            panic!(same_index_error_message)
        }
        unsafe {
            let self2 = mem::transmute_copy::<&mut Vec<T>, &mut Vec<T>>(&self);
            (&mut self[a], &mut self2[b])
        }
    }
}

impl<T> Index<NodeId> for Arena<T> {
    type Output = Node<T>;

    fn index(&self, node: NodeId) -> &Node<T> {
        &self.nodes[node.index]
    }
}

impl<T> IndexMut<NodeId> for Arena<T> {
    fn index_mut(&mut self, node: NodeId) -> &mut Node<T> {
        &mut self.nodes[node.index]
    }
}


impl<T> Node<T> {
    /// Return the ID of the parent node, unless this node is the root of the tree.
    pub fn parent(&self) -> Option<NodeId> { self.parent }

    /// Return the ID of the first child of this node, unless it has no child.
    pub fn first_child(&self) -> Option<NodeId> { self.first_child }

    /// Return the ID of the last child of this node, unless it has no child.
    pub fn last_child(&self) -> Option<NodeId> { self.last_child }

    /// Return the ID of the previous sibling of this node, unless it is a first child.
    pub fn previous_sibling(&self) -> Option<NodeId> { self.previous_sibling }

    /// Return the ID of the previous sibling of this node, unless it is a first child.
    pub fn next_sibling(&self) -> Option<NodeId> { self.next_sibling }
}


impl NodeId {
    /// Return an iterator of references to this node and its ancestors.
    ///
    /// Call `.next().unwrap()` once on the iterator to skip the node itself.
    pub fn ancestors<T>(self, arena: &Arena<T>) -> Ancestors<T> {
        Ancestors {
            arena: arena,
            node: Some(self),
        }
    }

    /// Return an iterator of references to this node and the siblings before it.
    ///
    /// Call `.next().unwrap()` once on the iterator to skip the node itself.
    pub fn preceding_siblings<T>(self, arena: &Arena<T>) -> PrecedingSiblings<T> {
        PrecedingSiblings {
            arena: arena,
            node: Some(self),
        }
    }

    /// Return an iterator of references to this node and the siblings after it.
    ///
    /// Call `.next().unwrap()` once on the iterator to skip the node itself.
    pub fn following_siblings<T>(self, arena: &Arena<T>) -> FollowingSiblings<T> {
        FollowingSiblings {
            arena: arena,
            node: Some(self),
        }
    }

    /// Return an iterator of references to this node’s children.
    pub fn children<T>(self, arena: &Arena<T>) -> Children<T> {
        Children {
            arena: arena,
            node: arena[self].first_child,
        }
    }

    /// Return an iterator of references to this node’s children, in reverse order.
    pub fn reverse_children<T>(self, arena: &Arena<T>) -> ReverseChildren<T> {
        ReverseChildren {
            arena: arena,
            node: arena[self].last_child,
        }
    }

    /// Return an iterator of references to this node and its descendants, in tree order.
    ///
    /// Parent nodes appear before the descendants.
    /// Call `.next().unwrap()` once on the iterator to skip the node itself.
    pub fn descendants<T>(self, arena: &Arena<T>) -> Descendants<T> {
        Descendants(self.traverse(arena))
    }

    /// Return an iterator of references to this node and its descendants, in tree order.
    pub fn traverse<T>(self, arena: &Arena<T>) -> Traverse<T> {
        Traverse {
            arena: arena,
            root: self,
            next: Some(NodeEdge::Start(self)),
        }
    }

    /// Return an iterator of references to this node and its descendants, in tree order.
    pub fn reverse_traverse<T>(self, arena: &Arena<T>) -> ReverseTraverse<T> {
        ReverseTraverse {
            arena: arena,
            root: self,
            next: Some(NodeEdge::End(self)),
        }
    }

    /// Detach a node from its parent and siblings. Children are not affected.
    pub fn detach<T>(self, arena: &mut Arena<T>) {
        let (parent, previous_sibling, next_sibling) = {
            let node = &mut arena[self];
            (node.parent.take(), node.previous_sibling.take(), node.next_sibling.take())
        };

        if let Some(next_sibling) = next_sibling {
            arena[next_sibling].previous_sibling = previous_sibling;
        } else if let Some(parent) = parent {
            arena[parent].last_child = previous_sibling;
        }

        if let Some(previous_sibling) = previous_sibling {
            arena[previous_sibling].next_sibling = next_sibling;
        } else if let Some(parent) = parent {
            arena[parent].first_child = next_sibling;
        }
    }

    /// Append a new child to this node, after existing children.
    pub fn append<T>(self, new_child: NodeId, arena: &mut Arena<T>) {
        new_child.detach(arena);
        let last_child_opt;
        {
            let (self_borrow, new_child_borrow) = arena.nodes.get_pair_mut(
                self.index, new_child.index, "Can not append a node to itself");
            new_child_borrow.parent = Some(self);
            last_child_opt = mem::replace(&mut self_borrow.last_child, Some(new_child));
            if let Some(last_child) = last_child_opt {
                new_child_borrow.previous_sibling = Some(last_child);
            } else {
                debug_assert!(self_borrow.first_child.is_none());
                self_borrow.first_child = Some(new_child);
            }
        }
        if let Some(last_child) = last_child_opt {
            debug_assert!(arena[last_child].next_sibling.is_none());
            arena[last_child].next_sibling = Some(new_child);
        }
    }

    /// Prepend a new child to this node, before existing children.
    pub fn prepend<T>(self, new_child: NodeId, arena: &mut Arena<T>) {
        new_child.detach(arena);
        let first_child_opt;
        {
            let (self_borrow, new_child_borrow) = arena.nodes.get_pair_mut(
                self.index, new_child.index, "Can not prepend a node to itself");
            new_child_borrow.parent = Some(self);
            first_child_opt = mem::replace(&mut self_borrow.first_child, Some(new_child));
            if let Some(first_child) = first_child_opt {
                new_child_borrow.next_sibling = Some(first_child);
            } else {
                debug_assert!(&self_borrow.first_child.is_none());
                self_borrow.last_child = Some(new_child);
            }
        }
        if let Some(first_child) = first_child_opt {
            debug_assert!(arena[first_child].previous_sibling.is_none());
            arena[first_child].previous_sibling = Some(new_child);
        }
    }

    /// Insert a new sibling after this node.
    pub fn insert_after<T>(self, new_sibling: NodeId, arena: &mut Arena<T>) {
        new_sibling.detach(arena);
        let next_sibling_opt;
        let parent_opt;
        {
            let (self_borrow, new_sibling_borrow) = arena.nodes.get_pair_mut(
                self.index, new_sibling.index, "Can not insert a node after itself");
            parent_opt = self_borrow.parent;
            new_sibling_borrow.parent = parent_opt;
            new_sibling_borrow.previous_sibling = Some(self);
            next_sibling_opt = mem::replace(&mut self_borrow.next_sibling, Some(new_sibling));
            if let Some(next_sibling) = next_sibling_opt {
                new_sibling_borrow.next_sibling = Some(next_sibling);
            }
        }
        if let Some(next_sibling) = next_sibling_opt {
            debug_assert!(arena[next_sibling].previous_sibling.unwrap() == self);
            arena[next_sibling].previous_sibling = Some(new_sibling);
        } else if let Some(parent) = parent_opt {
            debug_assert!(arena[parent].last_child.unwrap() == self);
            arena[parent].last_child = Some(new_sibling);
        }
    }

    /// Insert a new sibling before this node.
    pub fn insert_before<T>(self, new_sibling: NodeId, arena: &mut Arena<T>) {
        new_sibling.detach(arena);
        let previous_sibling_opt;
        let parent_opt;
        {
            let (self_borrow, new_sibling_borrow) = arena.nodes.get_pair_mut(
                self.index, new_sibling.index, "Can not insert a node before itself");
            parent_opt = self_borrow.parent;
            new_sibling_borrow.parent = parent_opt;
            new_sibling_borrow.next_sibling = Some(self);
            previous_sibling_opt = mem::replace(&mut self_borrow.previous_sibling, Some(new_sibling));
            if let Some(previous_sibling) = previous_sibling_opt {
                new_sibling_borrow.previous_sibling = Some(previous_sibling);
            }
        }
        if let Some(previous_sibling) = previous_sibling_opt {
            debug_assert!(arena[previous_sibling].next_sibling.unwrap() == self);
            arena[previous_sibling].next_sibling = Some(new_sibling);
        } else if let Some(parent) = parent_opt {
            debug_assert!(arena[parent].first_child.unwrap() == self);
            arena[parent].first_child = Some(new_sibling);
        }
    }
}


macro_rules! impl_node_iterator {
    ($name: ident, $next: expr) => {
        impl<'a, T> Iterator for $name<'a, T> {
            type Item = NodeId;

            fn next(&mut self) -> Option<NodeId> {
                match self.node.take() {
                    Some(node) => {
                        self.node = $next(&self.arena[node]);
                        Some(node)
                    }
                    None => None
                }
            }
        }
    }
}

/// An iterator of references to the ancestors a given node.
pub struct Ancestors<'a, T: 'a> {
    arena: &'a Arena<T>,
    node: Option<NodeId>,
}

impl_node_iterator!(Ancestors, |node: &Node<T>| node.parent);

/// An iterator of references to the siblings before a given node.
pub struct PrecedingSiblings<'a, T: 'a> {
    arena: &'a Arena<T>,
    node: Option<NodeId>,
}

impl_node_iterator!(PrecedingSiblings, |node: &Node<T>| node.previous_sibling);

/// An iterator of references to the siblings after a given node.
pub struct FollowingSiblings<'a, T: 'a> {
    arena: &'a Arena<T>,
    node: Option<NodeId>,
}

impl_node_iterator!(FollowingSiblings, |node: &Node<T>| node.next_sibling);

/// An iterator of references to the children of a given node.
pub struct Children<'a, T: 'a> {
    arena: &'a Arena<T>,
    node: Option<NodeId>,
}

impl_node_iterator!(Children, |node: &Node<T>| node.next_sibling);

/// An iterator of references to the children of a given node, in reverse order.
pub struct ReverseChildren<'a, T: 'a> {
    arena: &'a Arena<T>,
    node: Option<NodeId>,
}

impl_node_iterator!(ReverseChildren, |node: &Node<T>| node.previous_sibling);


/// An iterator of references to a given node and its descendants, in tree order.
pub struct Descendants<'a, T: 'a>(Traverse<'a, T>);

impl<'a, T> Iterator for Descendants<'a, T> {
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
pub struct Traverse<'a, T: 'a> {
    arena: &'a Arena<T>,
    root: NodeId,
    next: Option<NodeEdge<NodeId>>,
}

impl<'a, T> Iterator for Traverse<'a, T> {
    type Item = NodeEdge<NodeId>;

    fn next(&mut self) -> Option<NodeEdge<NodeId>> {
        match self.next.take() {
            Some(item) => {
                self.next = match item {
                    NodeEdge::Start(node) => {
                        match self.arena[node].first_child {
                            Some(first_child) => Some(NodeEdge::Start(first_child)),
                            None => Some(NodeEdge::End(node.clone()))
                        }
                    }
                    NodeEdge::End(node) => {
                        if node == self.root {
                            None
                        } else {
                            match self.arena[node].next_sibling {
                                Some(next_sibling) => Some(NodeEdge::Start(next_sibling)),
                                None => match self.arena[node].parent {
                                    Some(parent) => Some(NodeEdge::End(parent)),

                                    // `node.parent()` here can only be `None`
                                    // if the tree has been modified during iteration,
                                    // but silently stoping iteration
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
pub struct ReverseTraverse<'a, T: 'a> {
    arena: &'a Arena<T>,
    root: NodeId,
    next: Option<NodeEdge<NodeId>>,
}

impl<'a, T> Iterator for ReverseTraverse<'a, T> {
    type Item = NodeEdge<NodeId>;

    fn next(&mut self) -> Option<NodeEdge<NodeId>> {
        match self.next.take() {
            Some(item) => {
                self.next = match item {
                    NodeEdge::End(node) => {
                        match self.arena[node].last_child {
                            Some(last_child) => Some(NodeEdge::End(last_child)),
                            None => Some(NodeEdge::Start(node.clone()))
                        }
                    }
                    NodeEdge::Start(node) => {
                        if node == self.root {
                            None
                        } else {
                            match self.arena[node].previous_sibling {
                                Some(previous_sibling) => Some(NodeEdge::End(previous_sibling)),
                                None => match self.arena[node].parent {
                                    Some(parent) => Some(NodeEdge::Start(parent)),

                                    // `node.parent()` here can only be `None`
                                    // if the tree has been modified during iteration,
                                    // but silently stoping iteration
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