//! DOM tree to CSS style tree cascading.
//!
//! Implements CSS selector matching (`matches_html_element`) and cascade-info
//! construction (`construct_html_cascade_tree`). Used by `styled_dom` and
//! `prop_cache` to resolve which CSS rules apply to each DOM node.

use alloc::vec::Vec;

use azul_css::css::{
    AttributeMatchOp, CssAttributeSelector, CssContentGroup, CssNthChildSelector,
    CssNthChildSelector::*, CssPath, CssPathPseudoSelector, CssPathSelector,
};

use crate::{
    dom::NodeData,
    id::{NodeDataContainer, NodeDataContainerRef, NodeHierarchyRef, NodeId},
    styled_dom::NodeHierarchyItem,
};

/// Has all the necessary information about the style CSS path
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub struct CascadeInfo {
    pub index_in_parent: u32,
    pub is_last_child: bool,
}

impl_option!(
    CascadeInfo,
    OptionCascadeInfo,
    [Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash]
);

impl_vec!(CascadeInfo, CascadeInfoVec, CascadeInfoVecDestructor, CascadeInfoVecDestructorType, CascadeInfoVecSlice, OptionCascadeInfo);
impl_vec_mut!(CascadeInfo, CascadeInfoVec);
impl_vec_debug!(CascadeInfo, CascadeInfoVec);
impl_vec_partialord!(CascadeInfo, CascadeInfoVec);
impl_vec_clone!(CascadeInfo, CascadeInfoVec, CascadeInfoVecDestructor);
impl_vec_partialeq!(CascadeInfo, CascadeInfoVec);

impl CascadeInfoVec {
    pub fn as_container<'a>(&'a self) -> NodeDataContainerRef<'a, CascadeInfo> {
        NodeDataContainerRef {
            internal: self.as_ref(),
        }
    }
}

/// Returns if the style CSS path matches the DOM node (i.e. if the DOM node should be styled by
/// that element)
pub fn matches_html_element(
    css_path: &CssPath,
    node_id: NodeId,
    node_hierarchy: &NodeDataContainerRef<NodeHierarchyItem>,
    node_data: &NodeDataContainerRef<NodeData>,
    html_node_tree: &NodeDataContainerRef<CascadeInfo>,
    expected_path_ending: Option<CssPathPseudoSelector>,
) -> bool {
    use self::CssGroupSplitReason::*;

    if css_path.selectors.is_empty() {
        return false;
    }

    // Skip anonymous nodes - they are not part of the original DOM tree
    // and should not participate in CSS selector matching
    if node_data[node_id].is_anonymous() {
        return false;
    }

    // Collect all selector groups (processed right-to-left from the CSS path).
    let groups: Vec<(CssContentGroup<'_>, CssGroupSplitReason)> =
        CssGroupIterator::new(css_path.selectors.as_ref()).collect();

    if groups.is_empty() {
        return false;
    }

    // The rightmost group must match the target node directly.
    let (ref first_group, first_reason) = groups[0];
    let is_last_content_group = groups.len() == 1;
    if !selector_group_matches(
        first_group,
        &html_node_tree[node_id],
        &node_data[node_id],
        node_id,
        &expected_path_ending,
        is_last_content_group,
    ) {
        return false;
    }

    // Navigate from the target node upward/sideways through the DOM,
    // matching each remaining selector group with its combinator.
    let mut current_node = node_id;

    for (group_idx, (content_group, _reason)) in groups.iter().enumerate().skip(1) {
        // The combinator comes from the PREVIOUS group's reason
        let combinator = groups[group_idx - 1].1;
        let is_last = group_idx == groups.len() - 1;

        match combinator {
            DirectChildren => {
                // Parent must match directly (child combinator `>`)
                let parent = find_non_anonymous_parent(current_node, node_hierarchy, node_data);
                match parent {
                    Some(p) if selector_group_matches(
                        content_group, &html_node_tree[p], &node_data[p], p,
                        &expected_path_ending, is_last,
                    ) => { current_node = p; }
                    _ => return false,
                }
            }
            Children => {
                // Search up ancestor chain for a match (descendant combinator ` `)
                let mut ancestor = find_non_anonymous_parent(current_node, node_hierarchy, node_data);
                let mut found = false;
                while let Some(anc) = ancestor {
                    if selector_group_matches(
                        content_group, &html_node_tree[anc], &node_data[anc], anc,
                        &expected_path_ending, is_last,
                    ) {
                        current_node = anc;
                        found = true;
                        break;
                    }
                    ancestor = find_non_anonymous_parent(anc, node_hierarchy, node_data);
                }
                if !found {
                    return false;
                }
            }
            AdjacentSibling => {
                // Immediate previous sibling must match (adjacent sibling `+`)
                let sibling = find_non_anonymous_prev_sibling(current_node, node_hierarchy, node_data);
                match sibling {
                    Some(s) if selector_group_matches(
                        content_group, &html_node_tree[s], &node_data[s], s,
                        &expected_path_ending, is_last,
                    ) => { current_node = s; }
                    _ => return false,
                }
            }
            GeneralSibling => {
                // Search previous siblings for a match (general sibling `~`)
                let mut sibling = find_non_anonymous_prev_sibling(current_node, node_hierarchy, node_data);
                let mut found = false;
                while let Some(sib) = sibling {
                    if selector_group_matches(
                        content_group, &html_node_tree[sib], &node_data[sib], sib,
                        &expected_path_ending, is_last,
                    ) {
                        current_node = sib;
                        found = true;
                        break;
                    }
                    sibling = find_non_anonymous_prev_sibling(sib, node_hierarchy, node_data);
                }
                if !found {
                    return false;
                }
            }
        }
    }

    true
}

/// Find the first non-anonymous parent of a node.
fn find_non_anonymous_parent(
    node_id: NodeId,
    node_hierarchy: &NodeDataContainerRef<NodeHierarchyItem>,
    node_data: &NodeDataContainerRef<NodeData>,
) -> Option<NodeId> {
    let mut next = node_hierarchy[node_id].parent_id();
    while let Some(n) = next {
        if !node_data[n].is_anonymous() {
            return Some(n);
        }
        next = node_hierarchy[n].parent_id();
    }
    None
}

/// Find the first non-anonymous previous sibling of a node.
fn find_non_anonymous_prev_sibling(
    node_id: NodeId,
    node_hierarchy: &NodeDataContainerRef<NodeHierarchyItem>,
    node_data: &NodeDataContainerRef<NodeData>,
) -> Option<NodeId> {
    let mut next = node_hierarchy[node_id].previous_sibling_id();
    while let Some(n) = next {
        if !node_data[n].is_anonymous() {
            return Some(n);
        }
        next = node_hierarchy[n].previous_sibling_id();
    }
    None
}

/// A CSS group is a group of css selectors in a path that specify the rule that a
/// certain node has to match, i.e. "div.main.foo" has to match three requirements:
///
/// - the node has to be of type div
/// - the node has to have the class "main"
/// - the node has to have the class "foo"
///
/// If any of these requirements are not met, the CSS block is discarded.
///
/// The CssGroupIterator splits the CSS path into semantic blocks, i.e.:
///
/// "body > .foo.main > #baz" will be split into ["body", ".foo.main" and "#baz"]
pub struct CssGroupIterator<'a> {
    pub css_path: &'a [CssPathSelector],
    current_idx: usize,
    last_reason: CssGroupSplitReason,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CssGroupSplitReason {
    /// ".foo .main" - match any children
    Children,
    /// ".foo > .main" - match only direct children
    DirectChildren,
    /// ".foo + .main" - match adjacent sibling (immediately preceding)
    AdjacentSibling,
    /// ".foo ~ .main" - match general sibling (any preceding sibling)
    GeneralSibling,
}

impl<'a> CssGroupIterator<'a> {
    pub fn new(css_path: &'a [CssPathSelector]) -> Self {
        let initial_len = css_path.len();
        Self {
            css_path,
            current_idx: initial_len,
            last_reason: CssGroupSplitReason::Children,
        }
    }
}

impl<'a> Iterator for CssGroupIterator<'a> {
    type Item = (CssContentGroup<'a>, CssGroupSplitReason);

    fn next(&mut self) -> Option<(CssContentGroup<'a>, CssGroupSplitReason)> {
        use self::CssPathSelector::*;

        let mut new_idx = self.current_idx;

        if new_idx == 0 {
            return None;
        }

        let mut current_path = Vec::new();

        while new_idx != 0 {
            match self.css_path.get(new_idx - 1)? {
                Children => {
                    self.last_reason = CssGroupSplitReason::Children;
                    break;
                }
                DirectChildren => {
                    self.last_reason = CssGroupSplitReason::DirectChildren;
                    break;
                }
                AdjacentSibling => {
                    self.last_reason = CssGroupSplitReason::AdjacentSibling;
                    break;
                }
                GeneralSibling => {
                    self.last_reason = CssGroupSplitReason::GeneralSibling;
                    break;
                }
                other => current_path.push(other),
            }
            new_idx -= 1;
        }

        // NOTE: Order inside of a ContentGroup is not important
        // for matching elements, only important for testing
        #[cfg(test)]
        current_path.reverse();

        if new_idx == 0 {
            if current_path.is_empty() {
                None
            } else {
                // Last element of path
                self.current_idx = 0;
                Some((current_path, self.last_reason))
            }
        } else {
            // skip the "Children | DirectChildren" element itself
            self.current_idx = new_idx - 1;
            Some((current_path, self.last_reason))
        }
    }
}

pub fn construct_html_cascade_tree(
    node_hierarchy: &NodeHierarchyRef,
    node_depths_sorted: &[(usize, NodeId)],
    node_data: &NodeDataContainerRef<NodeData>,
) -> NodeDataContainer<CascadeInfo> {
    let mut nodes = (0..node_hierarchy.len())
        .map(|_| CascadeInfo {
            index_in_parent: 0,
            is_last_child: false,
        })
        .collect::<Vec<_>>();

    for (_depth, parent_id) in node_depths_sorted {
        // Per CSS Selectors Level 4 §13: "Standalone text and other non-element
        // nodes are not counted when calculating the position of an element in
        // the list of children of its parent."
        //
        // We count only element siblings when computing index_in_parent.
        let element_index_in_parent = parent_id
            .preceding_siblings(node_hierarchy)
            .filter(|sib_id| !node_data[*sib_id].is_text_node())
            .count();

        let parent_html_matcher = CascadeInfo {
            index_in_parent: (element_index_in_parent.saturating_sub(1)) as u32,
            // Necessary for :last selectors — find last element sibling
            is_last_child: {
                let mut is_last_element = true;
                let mut next = node_hierarchy[*parent_id].next_sibling;
                while let Some(sib_id) = next {
                    if !node_data[sib_id].is_text_node() {
                        is_last_element = false;
                        break;
                    }
                    next = node_hierarchy[sib_id].next_sibling;
                }
                is_last_element
            },
        };

        nodes[parent_id.index()] = parent_html_matcher;

        // Count only element children for index_in_parent
        let mut element_idx: u32 = 0;
        for child_id in parent_id.children(node_hierarchy) {
            let is_text = node_data[child_id].is_text_node();

            // Find whether this is the last element child (skip trailing text nodes)
            let is_last_element_child = if is_text {
                false
            } else {
                let mut is_last = true;
                let mut next = node_hierarchy[child_id].next_sibling;
                while let Some(sib_id) = next {
                    if !node_data[sib_id].is_text_node() {
                        is_last = false;
                        break;
                    }
                    next = node_hierarchy[sib_id].next_sibling;
                }
                is_last
            };

            let child_html_matcher = CascadeInfo {
                index_in_parent: element_idx,
                is_last_child: is_last_element_child,
            };

            nodes[child_id.index()] = child_html_matcher;

            if !is_text {
                element_idx += 1;
            }
        }
    }

    NodeDataContainer { internal: nodes }
}

/// Checks whether the last selector in `path` matches the given pseudo-selector `target`.
///
/// Known limitation: this only inspects the final selector in the path, so compound
/// selectors like `div:hover:first-child` may not be filtered correctly when `target`
/// is `None` — only the very last pseudo-selector is tested.
#[inline]
pub fn rule_ends_with(path: &CssPath, target: Option<CssPathPseudoSelector>) -> bool {
    // Helper to check if a pseudo-selector is "interactive" (requires user interaction state)
    // vs "structural" (based on DOM structure only)
    fn is_interactive_pseudo(p: &CssPathPseudoSelector) -> bool {
        matches!(
            p,
            CssPathPseudoSelector::Hover
                | CssPathPseudoSelector::Active
                | CssPathPseudoSelector::Focus
                | CssPathPseudoSelector::Backdrop
                | CssPathPseudoSelector::Dragging
                | CssPathPseudoSelector::DragOver
        )
    }

    match target {
        None => match path.selectors.as_ref().last() {
            None => false,
            Some(q) => match q {
                // Only reject interactive pseudo-selectors (hover, active, focus)
                // Structural pseudo-selectors (nth-child, first, last) should be allowed
                CssPathSelector::PseudoSelector(p) => !is_interactive_pseudo(p),
                _ => true,
            },
        },
        Some(s) => match path.selectors.as_ref().last() {
            None => false,
            Some(q) => match q {
                CssPathSelector::PseudoSelector(q) => *q == s,
                _ => false,
            },
        },
    }
}

/// Matches a single group of CSS selectors against a DOM node.
///
/// Returns true if all selectors in the group match the given node.
/// Combinator selectors (>, +, ~, space) should not appear in the group.
fn selector_group_matches(
    selectors: &[&CssPathSelector],
    html_node: &CascadeInfo,
    node_data: &NodeData,
    node_id: NodeId,
    expected_path_ending: &Option<CssPathPseudoSelector>,
    is_last_content_group: bool,
) -> bool {
    selectors.iter().all(|selector| {
        match_single_selector(
            selector,
            html_node,
            node_data,
            node_id,
            expected_path_ending,
            is_last_content_group,
        )
    })
}

/// Matches a single CSS selector against a DOM node.
fn match_single_selector(
    selector: &CssPathSelector,
    html_node: &CascadeInfo,
    node_data: &NodeData,
    node_id: NodeId,
    expected_path_ending: &Option<CssPathPseudoSelector>,
    is_last_content_group: bool,
) -> bool {
    use self::CssPathSelector::*;

    match selector {
        Global => true,
        // `Root(range)` (scope marker, #47): matches iff this node's flat id is in
        // the owning subtree's range. push_front-ed onto inline (with_css) rules so
        // they scope to a subtree instead of leaking to the whole tree.
        Root(range) => range.contains(node_id.index()),
        Type(t) => node_data.get_node_type().get_path() == *t,
        Class(c) => node_data.has_class(c.as_str()),
        Id(id) => node_data.has_id(id.as_str()),
        PseudoSelector(p) => {
            match_pseudo_selector(p, html_node, expected_path_ending, is_last_content_group)
        }
        Attribute(a) => match_attribute_selector(a, node_data),
        DirectChildren | Children | AdjacentSibling | GeneralSibling => false,
    }
}

/// Matches an attribute selector (`[name]`, `[name="v"]`, `[name~="v"]`, ...) against a node.
///
/// Some attributes (notably `class`) are stored as multiple separate entries in
/// `node_data.attributes()` rather than a single space-joined string. We collect
/// every matching value and treat the matcher as "any value satisfies the op",
/// so that `[class~="primary"]` matches a node with classes `foo primary bar`.
fn match_attribute_selector(sel: &CssAttributeSelector, node_data: &NodeData) -> bool {
    let name = sel.name.as_str();
    let target = sel.value.as_ref().map(|v| v.as_str());

    let check = |actual: &str| -> bool {
        match (&sel.op, target) {
            (AttributeMatchOp::Exists, _) => true,
            (AttributeMatchOp::Eq, Some(t)) => actual == t,
            (AttributeMatchOp::Includes, Some(t)) => {
                if t.is_empty() || t.contains(char::is_whitespace) {
                    return false;
                }
                actual.split_whitespace().any(|word| word == t)
            }
            (AttributeMatchOp::DashMatch, Some(t)) => {
                actual == t || actual.starts_with(&alloc::format!("{}-", t))
            }
            (AttributeMatchOp::Prefix, Some(t)) => !t.is_empty() && actual.starts_with(t),
            (AttributeMatchOp::Suffix, Some(t)) => !t.is_empty() && actual.ends_with(t),
            (AttributeMatchOp::Substring, Some(t)) => !t.is_empty() && actual.contains(t),
            // Operator with a missing value (parser should reject these — be defensive).
            (_, None) => false,
        }
    };

    for attr in node_data.attributes().iter() {
        if attr.name() != name {
            continue;
        }
        if check(attr.value().as_str()) {
            return true;
        }
    }

    false
}

/// Matches a pseudo-selector (:first, :last, :nth-child, :hover, etc.) against a node.
fn match_pseudo_selector(
    pseudo: &CssPathPseudoSelector,
    html_node: &CascadeInfo,
    expected_path_ending: &Option<CssPathPseudoSelector>,
    is_last_content_group: bool,
) -> bool {
    match pseudo {
        CssPathPseudoSelector::First => match_first_child(html_node),
        CssPathPseudoSelector::Last => match_last_child(html_node),
        CssPathPseudoSelector::NthChild(pattern) => match_nth_child(html_node, pattern),
        CssPathPseudoSelector::Hover => match_interactive_pseudo(
            &CssPathPseudoSelector::Hover,
            expected_path_ending,
            is_last_content_group,
        ),
        CssPathPseudoSelector::Active => match_interactive_pseudo(
            &CssPathPseudoSelector::Active,
            expected_path_ending,
            is_last_content_group,
        ),
        CssPathPseudoSelector::Focus => match_interactive_pseudo(
            &CssPathPseudoSelector::Focus,
            expected_path_ending,
            is_last_content_group,
        ),
        CssPathPseudoSelector::Backdrop => match_interactive_pseudo(
            &CssPathPseudoSelector::Backdrop,
            expected_path_ending,
            is_last_content_group,
        ),
        CssPathPseudoSelector::Dragging => match_interactive_pseudo(
            &CssPathPseudoSelector::Dragging,
            expected_path_ending,
            is_last_content_group,
        ),
        CssPathPseudoSelector::DragOver => match_interactive_pseudo(
            &CssPathPseudoSelector::DragOver,
            expected_path_ending,
            is_last_content_group,
        ),
        CssPathPseudoSelector::Lang(lang) => {
            // :lang() is matched via DynamicSelector at runtime, not during CSS cascade
            // During cascade, we just check if this is the expected ending
            if let Some(expected) = expected_path_ending {
                if let CssPathPseudoSelector::Lang(expected_lang) = expected {
                    return lang == expected_lang;
                }
            }
            // If not specifically looking for :lang, it doesn't match structurally
            false
        }
    }
}

/// Returns true if the node is the first child of its parent.
fn match_first_child(html_node: &CascadeInfo) -> bool {
    html_node.index_in_parent == 0
}

/// Returns true if the node is the last child of its parent.
fn match_last_child(html_node: &CascadeInfo) -> bool {
    html_node.is_last_child
}

/// Matches :nth-child(n), :nth-child(even), :nth-child(odd), or :nth-child(An+B) patterns.
fn match_nth_child(html_node: &CascadeInfo, pattern: &CssNthChildSelector) -> bool {
    use azul_css::css::CssNthChildPattern;

    // nth-child is 1-indexed, index_in_parent is 0-indexed
    let index = html_node.index_in_parent + 1;

    match pattern {
        Number(n) => index == *n,
        Even => index % 2 == 0,
        Odd => index % 2 == 1,
        Pattern(CssNthChildPattern {
            pattern_repeat,
            offset,
        }) => {
            if *pattern_repeat == 0 {
                index == *offset
            } else {
                index >= *offset && ((index - offset) % pattern_repeat == 0)
            }
        }
    }
}

/// Matches interactive pseudo-selectors (:hover, :active, :focus).
/// These only apply if they appear in the last content group of the CSS path.
fn match_interactive_pseudo(
    pseudo: &CssPathPseudoSelector,
    expected_path_ending: &Option<CssPathPseudoSelector>,
    is_last_content_group: bool,
) -> bool {
    is_last_content_group && expected_path_ending.as_ref() == Some(pseudo)
}
