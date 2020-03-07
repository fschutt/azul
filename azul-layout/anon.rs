#![allow(unused_assignments)]
#![allow(unused_variables)]

//! Module that handles the construction on `AnonDom`, a `Dom` that holds
//! "anonymous" nodes (that group two following "inline" texts into one "anonymous" block).

use std::collections::BTreeMap;
use crate::{
    RectContent,
    style::{Style, Overflow, Display, PositionType},
};
use azul_core::{
    id_tree::{NodeDataContainer, NodeHierarchy, NodeId, NodeDepths, Node},
    traits::GetTextLayout,
};

pub(crate) type OriginalNodeId = NodeId;
pub(crate) type AnonNodeId = NodeId;

/// Same as the original DOM, but with anonymous nodes added to the original nodes.
///
/// Each box must contain only block children, or only inline children. When an DOM element
/// contains a mix of block and inline children, the layout engine inserts anonymous boxes to
/// separate the two types. (These boxes are "anonymous" because they aren't associated with
/// nodes in the DOM tree.)
#[derive(Debug, Clone)]
pub(crate) struct AnonDom {
    pub(crate) anon_node_hierarchy: NodeHierarchy,
    pub(crate) anon_node_data: NodeDataContainer<AnonNode>,
    pub(crate) original_node_id_mapping: BTreeMap<OriginalNodeId, AnonNodeId>,
    pub(crate) reverse_node_id_mapping: BTreeMap<AnonNodeId, OriginalNodeId>,
}

#[derive(Debug, Copy, Clone)]
pub(crate) enum AnonNode {
    /// Node that doesn't have a correspondent in the DOM tree,
    /// but still behaves like display: block. Is always a parent of one or
    /// more display:inline items
    AnonStyle,
    /// Non-inline DOM node (block / flex, etc.)
    BlockNode(Style),
    /// Inline node or text. Note that the style.display may still be "block",
    /// on text nodes the "display" property is ignored, since texts are always
    /// laid out as inline items.
    InlineNode(Style),
}

impl AnonNode {

    pub(crate) fn get_style(&self) -> &Style {
        use self::AnonNode::*;
        use crate::style::DEFAULT_STYLE;
        match &self {
            AnonStyle => &DEFAULT_STYLE,
            BlockNode(s) | InlineNode(s) => s,
        }
    }

    pub(crate) fn get_position_type(&self) -> PositionType {
        use self::AnonNode::*;
        match self {
         AnonStyle => PositionType::Static,
         BlockNode(s) | InlineNode(s) => s.position_type,
        }
    }

    pub(crate) fn get_display(&self) -> Display {
        use self::AnonNode::*;
        match self {
         AnonStyle => Display::Block,
         BlockNode(s) | InlineNode(s) => s.display,
        }
    }

    pub(crate) fn get_overflow_x(&self) -> Overflow {
        use self::AnonNode::*;
        match self {
         AnonStyle => Overflow::Auto,
         BlockNode(s) | InlineNode(s) => s.overflow,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn is_inline(&self) -> bool {
        use self::AnonNode::*;
        match self {
            AnonStyle | BlockNode(_) => false,
            InlineNode(_) => true,
        }
    }
}

impl AnonDom {

    pub(crate) fn new<T: GetTextLayout>(
        node_hierarchy: &NodeHierarchy,
        node_styles: &NodeDataContainer<Style>,
        node_depths: &NodeDepths,
        rect_contents: &BTreeMap<NodeId, RectContent<T>>,
    ) -> Self {

        use self::AnonNode::*;

        // Worst case scenario is that every node needs an anonymous block.
        // Pre-allocate 2x the nodes to avoid recursion
        let mut new_nodes = vec![AnonNode::AnonStyle; node_hierarchy.len() * 2];
        let mut new_node_hierarchy = vec![Node::ROOT; node_hierarchy.len() * 2];
        let mut original_node_id_mapping = BTreeMap::new();
        let mut reverse_node_id_mapping = BTreeMap::new();

        original_node_id_mapping.insert(NodeId::ZERO, NodeId::ZERO);
        reverse_node_id_mapping.insert(NodeId::ZERO, NodeId::ZERO);

        let mut num_anon_nodes = 0;

        // Count how many anonymous nodes need to be inserted in order
        // to correct the "next sibling" count
        let anon_nodes_count = count_all_anon_nodes(node_hierarchy, node_styles, node_depths, rect_contents);

        for (_depth, parent_id) in node_depths {

            let children_ids = parent_id.children(node_hierarchy).collect::<Vec<NodeId>>();
            let children_count = children_ids.len();

            if children_count == 0 {
                continue;
            }

            let num_inline_children = children_ids.iter().filter(|child_id| is_inline_node(&node_styles[**child_id], &rect_contents, child_id)).count();
            let num_block_children = children_count - num_inline_children;
            let all_children_are_inline = num_inline_children == children_count;
            let all_children_are_block = num_block_children == children_count;

            // Add the node data of the parent to the DOM
            let parent_node_style = &node_styles[*parent_id];
            let old_parent_node = node_hierarchy[*parent_id];
            let parent_is_inline_node = is_inline_node(&parent_node_style, &rect_contents, parent_id);

            original_node_id_mapping.insert(*parent_id, *parent_id + num_anon_nodes);
            reverse_node_id_mapping.insert(*parent_id + num_anon_nodes, *parent_id);

            new_nodes[(*parent_id + num_anon_nodes).index()] =
                if parent_is_inline_node { InlineNode(*parent_node_style) } else { BlockNode(*parent_node_style) };

            let anon_node_count_all_children = anon_nodes_count.get(parent_id).cloned().unwrap_or(0);

            new_node_hierarchy[(*parent_id + num_anon_nodes).index()] = Node {
                parent: old_parent_node.parent.as_ref().and_then(|p| original_node_id_mapping.get(p).copied()),
                previous_sibling: old_parent_node.previous_sibling.as_ref().and_then(|s| original_node_id_mapping.get(s).copied()),
                next_sibling: old_parent_node.next_sibling.map(|n| n + num_anon_nodes + anon_node_count_all_children),
                first_child: old_parent_node.first_child.map(|n| n + num_anon_nodes),
                last_child: old_parent_node.last_child.map(|n| n + num_anon_nodes + anon_node_count_all_children),
            };

            if all_children_are_inline || all_children_are_block {

                for child_id in children_ids.iter() {

                    let child_node_style = &node_styles[*child_id];
                    let old_child_node = node_hierarchy[*child_id];
                    let child_node_count_all_children = anon_nodes_count.get(child_id).copied().unwrap_or(0);

                    original_node_id_mapping.insert(*child_id, *child_id + num_anon_nodes);
                    reverse_node_id_mapping.insert(*child_id + num_anon_nodes, *child_id);

                    new_nodes[(*child_id + num_anon_nodes).index()] =
                        if all_children_are_block { BlockNode(*child_node_style) } else { InlineNode(*child_node_style) };

                    new_node_hierarchy[(*child_id + num_anon_nodes).index()] = Node {
                        parent: old_child_node.parent.as_ref().and_then(|p| original_node_id_mapping.get(p).copied()),
                        previous_sibling: old_child_node.previous_sibling.as_ref().and_then(|s| original_node_id_mapping.get(s).copied()),
                        next_sibling: old_child_node.next_sibling.map(|n| n + num_anon_nodes + child_node_count_all_children),
                        first_child: old_child_node.first_child.map(|n| n + num_anon_nodes),
                        last_child: old_child_node.last_child.map(|n| n + num_anon_nodes + child_node_count_all_children),
                    };
                }

            } else {

                // Mixed inline / block content: Need to insert anonymous nodes +
                // fix their parent / child relationships

                let mut last_anon_node = None;

                macro_rules! start_anonymous_node {($id:expr) => ({
                    let node_count_anon_children = anon_nodes_count.get($id).copied().unwrap_or(0);
                    last_anon_node = Some(($id, num_anon_nodes, node_count_anon_children));
                    num_anon_nodes += 1;
                })}

                macro_rules! end_anonymous_node {($id:expr) => ({
                    if let Some((last_anon_node, num_anon_nodes_before, _)) = last_anon_node {
                        let old_node = node_hierarchy[*last_anon_node];
                        new_node_hierarchy[last_anon_node.index() + num_anon_nodes_before] = Node {
                            parent: old_node.parent.as_ref().and_then(|p| original_node_id_mapping.get(p).copied()),
                            previous_sibling: old_node.previous_sibling.as_ref().and_then(|s| original_node_id_mapping.get(s).copied()),
                            next_sibling: old_node.next_sibling.map(|_| $id + num_anon_nodes),
                            first_child: Some(*last_anon_node + num_anon_nodes_before + 1),
                            last_child: Some($id + (num_anon_nodes - 1)),
                        };
                    }

                    last_anon_node = None;
                })}

                let mut last_child_is_inline_node = {
                    let first_child_id = &children_ids[0];
                    is_inline_node(&node_styles[*first_child_id], &rect_contents, first_child_id)
                };

                if last_child_is_inline_node { // first node is inline node
                    start_anonymous_node!(&children_ids[0]);
                }

                // Mixed content: How many anonymous nodes are needed?
                for (child_idx, child_id) in children_ids.iter().enumerate() {

                    let child_node_style = node_styles[*child_id];
                    let child_node = node_hierarchy[*child_id];
                    let current_child_is_inline_node = is_inline_node(&child_node_style, rect_contents, child_id);
                    let next_child_is_inline_node = if let Some(next_node_id) = children_ids.get(child_idx + 1) {
                        is_inline_node(&node_styles[*next_node_id], rect_contents, next_node_id)
                    } else {
                        false
                    };

                    let mut anon_node_tree_was_ended = None;

                    // inline content follows a block
                    if current_child_is_inline_node && !last_child_is_inline_node {
                        start_anonymous_node!(child_id);
                    } else if !current_child_is_inline_node && last_child_is_inline_node {
                        anon_node_tree_was_ended = last_anon_node
                        .map(|(last_anon_node, num_anon_nodes_before, _)| {
                            (*last_anon_node, num_anon_nodes_before)
                        });
                        end_anonymous_node!(*child_id);
                    }

                    original_node_id_mapping.insert(*child_id, *child_id + num_anon_nodes);
                    reverse_node_id_mapping.insert(*child_id + num_anon_nodes, *child_id);

                    new_nodes[(*child_id + num_anon_nodes).index()] =
                        if current_child_is_inline_node { InlineNode(child_node_style) } else { BlockNode(child_node_style) };

                    let node_count_anon_children = anon_nodes_count.get(child_id).copied().unwrap_or(0);
                    new_node_hierarchy[(*child_id + num_anon_nodes).index()] = Node {
                        parent:
                            if let Some((last_anon, num_anon_nodes_before, _)) = last_anon_node {
                                Some(*last_anon + num_anon_nodes_before)
                            } else {
                                child_node.parent.as_ref().and_then(|p| original_node_id_mapping.get(p).copied())
                            },
                        previous_sibling:
                            if let Some((last_anon_node, num_anon_nodes_before)) = anon_node_tree_was_ended {
                                Some(last_anon_node + num_anon_nodes_before)
                            } else {
                                child_node.previous_sibling.map(|n| n + num_anon_nodes)
                            },
                        next_sibling:
                            if current_child_is_inline_node && !next_child_is_inline_node {
                                None
                            } else {
                                child_node.next_sibling.map(|n| n + num_anon_nodes + node_count_anon_children)
                            },
                        first_child: child_node.first_child.map(|n| n + num_anon_nodes),
                        last_child: child_node.last_child.map(|n| n + num_anon_nodes + node_count_anon_children),
                    };

                    last_child_is_inline_node = current_child_is_inline_node;
                }

                if let Some(last) = children_ids.last() {
                    end_anonymous_node!(*last);
                }
            }
        }

        let total_nodes = node_hierarchy.len() + num_anon_nodes;
        new_nodes.truncate(total_nodes);
        new_node_hierarchy.truncate(total_nodes);

        Self {
            anon_node_hierarchy: NodeHierarchy::new(new_node_hierarchy),
            anon_node_data: NodeDataContainer::new(new_nodes),
            original_node_id_mapping,
            reverse_node_id_mapping,
        }
    }
}

// For each parent node, holds the amount of anonymous children nodes
fn count_all_anon_nodes<T: GetTextLayout>(
    node_hierarchy: &NodeHierarchy,
    node_styles: &NodeDataContainer<Style>,
    node_depths: &NodeDepths,
    rect_contents: &BTreeMap<NodeId, RectContent<T>>,
) -> BTreeMap<NodeId, usize> {
    let mut anon_nodes_by_depth = BTreeMap::new();
    let mut sum_anon_nodes = BTreeMap::new();

    let max_depth_level = match node_depths.last() {
        Some((s, _)) => *s,
        None => return anon_nodes_by_depth,
    };

    for (depth, parent_id) in node_depths.iter().rev() {

        let anon_nodes_direct_children = count_anon_nodes_direct_children(parent_id, node_hierarchy, node_styles, rect_contents);

        let current_node_all_anon_children = if *depth == max_depth_level {
            anon_nodes_direct_children
        } else {
            anon_nodes_direct_children +
                ((depth + 1)..max_depth_level)
                .map(|d| sum_anon_nodes.get(&d).copied().unwrap_or(0))
                .sum::<usize>()
        };

        anon_nodes_by_depth.insert(*parent_id, current_node_all_anon_children);
        *sum_anon_nodes.entry(depth).or_insert(0) += anon_nodes_direct_children;
    }

    anon_nodes_by_depth
}

fn count_anon_nodes_direct_children<T: GetTextLayout>(
    node_id: &NodeId,
    node_hierarchy: &NodeHierarchy,
    node_styles: &NodeDataContainer<Style>,
    rect_contents: &BTreeMap<NodeId, RectContent<T>>,
) -> usize {

    let children_ids = node_id.children(node_hierarchy).collect::<Vec<NodeId>>();
    let num_inline_children = children_ids
        .iter()
        .filter(|child_id| is_inline_node(&node_styles[**child_id], &rect_contents, child_id))
        .count();

    let children_count = children_ids.len();
    let num_block_children = children_count - num_inline_children;
    let all_children_are_inline = num_inline_children == children_count;
    let all_children_are_block = num_block_children == children_count;

    let mut anon_node_count = 0;

    if all_children_are_block || all_children_are_inline {
        // If all children are blocks or inlines, there are no anon blocks necessary
        return anon_node_count;
    }

    let first_child_id = match &node_hierarchy[*node_id].first_child {
        None => return anon_node_count,
        Some(s) => s,
    };

    let mut last_child_is_inline_node = is_inline_node(&node_styles[*first_child_id], rect_contents, first_child_id);

    if last_child_is_inline_node {
        anon_node_count += 1
    };

    for child_id in children_ids.iter() {
        let current_child_is_inline_node = is_inline_node(&node_styles[*child_id], &rect_contents, child_id);
        if current_child_is_inline_node && !last_child_is_inline_node {
            anon_node_count += 1;
        }
        last_child_is_inline_node = current_child_is_inline_node;
    }

    anon_node_count
}

fn is_inline_node<T: GetTextLayout>(s: &Style, rect_contents: &BTreeMap<NodeId, RectContent<T>>, node_id: &NodeId) -> bool {
    s.display == Display::InlineBlock ||
    // Is the item a text line? Texts are always laid out as display: inline, no matter what
    rect_contents.get(node_id).map(|c| c.is_text()) == Some(true)
}

#[test]
fn test_anon_dom() {

    use azul_core::{
        dom::Dom,
        ui_state::UiState,
        ui_description::UiDescription,
        ui_solver::{ResolvedTextLayoutOptions, InlineTextLayout, InlineTextLine},
        display_list::DisplayList,
    };
    use azul_css::{
        Css, Stylesheet, CssRuleBlock, CssPath, CssDeclaration,
        CssPathSelector, CssProperty, LayoutDisplay,
        LayoutRect, LayoutSize, LayoutPoint,
    };
    use crate::GetStyle;

    struct Mock;

    struct FakeTextMetricsProvider { }

    impl GetTextLayout for FakeTextMetricsProvider {
        // Fake text metrict provider that just returns a 10x10 rect for every text
        fn get_text_layout(&mut self, _: &ResolvedTextLayoutOptions) -> InlineTextLayout {
            InlineTextLayout::new(vec![
                InlineTextLine::new(LayoutRect::new(LayoutPoint::zero(), LayoutSize::new(10.0, 10.0)), 0, 0)
            ])
        }
    }

    let dom: Dom<Mock> = Dom::body()
        .with_child(Dom::label("first"))
        .with_child(Dom::label("second"))
        .with_child(Dom::div());

    let css = Css::empty();

    let mut ui_state = UiState::new(dom, None);
    let ui_description = UiDescription::new(&mut ui_state, &css, &None, &BTreeMap::new(), false);
    let display_list = DisplayList::new(&ui_description, &ui_state);
    let node_styles = display_list.rectangles.transform(|t, _| t.get_style());

    let mut rect_contents = BTreeMap::new();
    rect_contents.insert(NodeId::new(1), RectContent::Text(FakeTextMetricsProvider { }));
    rect_contents.insert(NodeId::new(2), RectContent::Text(FakeTextMetricsProvider { }));

    let anon_dom = AnonDom::new(
        &ui_state.get_dom().arena.node_hierarchy,
        &node_styles,
        &ui_state.get_dom().arena.node_hierarchy.get_parents_sorted_by_depth(),
        &rect_contents,
    );

    let expected_anon_hierarchy = NodeHierarchy::new(
        vec![
            // Node 0: root node (body):
            Node {
                parent: None,
                first_child: Some(NodeId::new(1)),
                last_child: Some(NodeId::new(4)),
                previous_sibling: None,
                next_sibling: None,
            },
            // Node 1 (anonymous node, parent of the two inline texts):
            Node {
                parent: Some(NodeId::new(0)),
                first_child: Some(NodeId::new(2)),
                last_child: Some(NodeId::new(3)),
                previous_sibling: None,
                next_sibling: Some(NodeId::new(4)),
            },
            // Node 2 (inline text "first"):
            Node {
                parent: Some(NodeId::new(1)),
                first_child: None,
                last_child: None,
                previous_sibling: None,
                next_sibling: Some(NodeId::new(3)),
            },
            // Node 3 (inline text "second"):
            Node {
                parent: Some(NodeId::new(1)),
                first_child: None,
                last_child: None,
                previous_sibling: Some(NodeId::new(2)),
                next_sibling: None,
            },
            // Node 4 (div block with id "third"):
            Node {
                parent: Some(NodeId::new(0)),
                first_child: None,
                last_child: None,
                previous_sibling: Some(NodeId::new(1)),
                next_sibling: None,
            },
        ]
    );

    if anon_dom.anon_node_hierarchy != expected_anon_hierarchy {
        panic!(
            "\r\n\r\nexpected:\r\n{:#?}\r\n\r\n----\r\n\r\ngot:\r\n{:#?}\r\n\r\n",
            expected_anon_hierarchy.internal,
            anon_dom.anon_node_hierarchy.internal,
        );
    }
}