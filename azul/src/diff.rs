//! Compares and diffs two DOM trees - necessary for tracking stateful events
//! such as user focus and scroll states across frames

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct DomRange {
    pub start: DomNodeInfo,
    pub end: DomNodeInfo,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct DomNodeInfo {
    pub hash: DomHash,
    pub id: NodeId,
}

impl DomRange {
    /// Is `other` a subtree of `self`? - Assumes that the DOM was
    /// constructed in a linear order, i.e. the child being within
    /// the parents start / end bounds
    pub fn contains(&self, other: &Self) -> bool {
        other.start.id.index() >= self.start.id.index() &&
        other.end.id.index() <= self.end.id.index()
    }

    /// Compares two DOM ranges without looking at the DOM hashes
    pub fn equals_range(&self, other: &Self) -> bool {
        other.start == self.start &&
        other.end == self.end
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq, Hash)]
pub struct DomDiff {
    added_nodes: Vec<DomRange>,
    removed_nodes: Vec<DomRange>,
}

pub(crate) fn diff_dom_tree(old: &Dom<DomHash>, new: Dom<DomHash>) -> DomDiff {
    DomDiff::default()
}