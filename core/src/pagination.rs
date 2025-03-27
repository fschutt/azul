use alloc::{
    collections::{BTreeMap, BTreeSet},
    vec::Vec,
};

use crate::{
    id_tree::{NodeDataContainer, NodeId},
    styled_dom::NodeHierarchyItem,
    ui_solver::PositionedRectangle,
    window::LogicalRect,
};

/// Recursive node structure for pagination
#[derive(Debug, Clone)]
pub struct PaginatedNode {
    /// Original NodeId
    pub id: NodeId,
    /// Bounding box for this node on this page
    pub rect: LogicalRect,
    /// Children of this node on this page
    pub children: Vec<PaginatedNode>,
}

/// Page output with recursive tree structure
#[derive(Debug)]
pub struct PaginatedPage {
    /// Root node of the page
    pub root: Option<PaginatedNode>,

    /// Maps original NodeId -> this page's node
    pub nodes: BTreeMap<NodeId, PaginatedNode>,
}

/// Paginate a layout result into multiple pages
pub fn paginate_layout_result<'a>(
    node_hierarchy: &crate::id_tree::NodeDataContainerRef<'a, NodeHierarchyItem>,
    rects: &crate::id_tree::NodeDataContainerRef<'a, PositionedRectangle>,
    page_height: f32,
) -> Vec<PaginatedPage> {
    let mut pages = Vec::new();

    // Calculate total document height
    let max_height = (0..rects.len())
        .map(|i| {
            let r = &rects[NodeId::new(i)];
            r.position.get_static_offset().y + r.size.height
        })
        .fold(0.0, f32::max);

    // Calculate number of pages
    let num_pages = (max_height / page_height).ceil() as usize;
    if num_pages == 0 {
        return pages;
    }

    // Step 1: Build node visibility map for each page
    let mut page_node_sets: Vec<BTreeSet<NodeId>> = Vec::with_capacity(num_pages);
    for page_idx in 0..num_pages {
        page_node_sets.push(BTreeSet::new());
    }

    // First, identify nodes that are geometrically visible on each page
    for node_id in (0..rects.len()).map(NodeId::new) {
        let r = &rects[node_id];
        let node_top = r.position.get_static_offset().y;
        let node_bottom = node_top + r.size.height;

        // Find all pages this node appears on
        for page_idx in 0..num_pages {
            let page_start = page_idx as f32 * page_height;
            let page_end = page_start + page_height;

            // Node is at least partially visible on this page
            if !(node_bottom <= page_start || node_top >= page_end) {
                page_node_sets[page_idx].insert(node_id);
            }
        }
    }

    // Step 2: For each page, ensure hierarchy consistency by adding all ancestors
    for page_idx in 0..num_pages {
        let mut complete_set = page_node_sets[page_idx].clone();
        let mut ancestors_to_add = Vec::new();

        // Collect all ancestors for visible nodes
        for &node_id in &page_node_sets[page_idx] {
            let mut current = node_id;
            while let Some(parent_id) = node_hierarchy[current].parent_id() {
                if !complete_set.contains(&parent_id) {
                    ancestors_to_add.push(parent_id);
                    complete_set.insert(parent_id);
                }
                current = parent_id;
            }
        }

        // Add all ancestors to the page node set
        for ancestor in ancestors_to_add {
            page_node_sets[page_idx].insert(ancestor);
        }
    }

    // Step 3: Build pages with precise hierarchy
    for page_idx in 0..num_pages {
        let page_start = page_idx as f32 * page_height;
        let page_end = page_start + page_height;

        // Skip empty pages
        if page_node_sets[page_idx].is_empty() {
            continue;
        }

        let mut nodes_map = BTreeMap::new();
        let root_id = NodeId::new(0);

        // Build the root node if it's visible on this page
        if page_node_sets[page_idx].contains(&root_id) {
            let root_node = build_paginated_node(
                root_id,
                page_start,
                page_end,
                node_hierarchy,
                rects,
                &page_node_sets[page_idx],
                &mut nodes_map,
            );

            pages.push(PaginatedPage {
                root: Some(root_node),
                nodes: nodes_map,
            });
        } else {
            // If the root isn't visible, find the highest visible ancestors
            let visible_roots = find_visible_roots(&page_node_sets[page_idx], node_hierarchy);

            // Build each visible root
            for &root_id in &visible_roots {
                let root_node = build_paginated_node(
                    root_id,
                    page_start,
                    page_end,
                    node_hierarchy,
                    rects,
                    &page_node_sets[page_idx],
                    &mut nodes_map,
                );

                // The first one becomes the page root
                if nodes_map.len() == 1 {
                    pages.push(PaginatedPage {
                        root: Some(root_node),
                        nodes: nodes_map.clone(),
                    });
                } else {
                    // Add to the existing page
                    if let Some(page) = pages.last_mut() {
                        page.nodes.insert(root_id, root_node);
                    }
                }
            }
        }
    }

    pages
}

/// Helper function to find the roots of visible nodes (nodes with no visible parents)
fn find_visible_roots(
    visible_nodes: &BTreeSet<NodeId>,
    node_hierarchy: &crate::id_tree::NodeDataContainerRef<NodeHierarchyItem>,
) -> Vec<NodeId> {
    let mut roots = Vec::new();

    for &node_id in visible_nodes {
        // Check if any parent is visible
        let mut has_visible_parent = false;
        let mut current = node_id;

        while let Some(parent_id) = node_hierarchy[current].parent_id() {
            if visible_nodes.contains(&parent_id) {
                has_visible_parent = true;
                break;
            }
            current = parent_id;
        }

        if !has_visible_parent {
            roots.push(node_id);
        }
    }

    roots
}

/// Build a paginated node and its children
fn build_paginated_node(
    node_id: NodeId,
    page_start: f32,
    page_end: f32,
    node_hierarchy: &crate::id_tree::NodeDataContainerRef<NodeHierarchyItem>,
    rects: &crate::id_tree::NodeDataContainerRef<PositionedRectangle>,
    visible_nodes: &BTreeSet<NodeId>,
    nodes_map: &mut BTreeMap<NodeId, PaginatedNode>,
) -> PaginatedNode {
    // If the node is already in the map, return a clone
    if let Some(existing) = nodes_map.get(&node_id) {
        return existing.clone();
    }

    let rect = &rects[node_id];
    let node_top = rect.position.get_static_offset().y;
    let node_bottom = node_top + rect.size.height;

    // Calculate visible portion of the node on this page
    let visible_top = node_top.max(page_start);
    let visible_bottom = node_bottom.min(page_end);
    let visible_height = visible_bottom - visible_top;

    // Create a copy of the rectangle with adjusted position and height
    let mut new_rect = rect.clone();
    if node_top < page_start || node_bottom > page_end {
        // Node is partially visible - adjust height and y position
        new_rect.size.height = visible_height;
        new_rect.position.translate_vertical(page_start - node_top);
    } else {
        // Node is fully visible - just adjust y position
        new_rect.position.translate_vertical(-page_start);
    }

    // Create the paginated node
    let mut paginated_node = PaginatedNode {
        id: node_id,
        rect: LogicalRect {
            origin: new_rect.position.get_static_offset(),
            size: new_rect.size,
        },
        children: Vec::new(),
    };

    // Add to map early to break potential cycles
    nodes_map.insert(node_id, paginated_node.clone());

    // Collect children that are visible on this page
    let mut child_id_opt = node_hierarchy[node_id].first_child_id(node_id);
    while let Some(child_id) = child_id_opt {
        if visible_nodes.contains(&child_id) {
            let child_node = build_paginated_node(
                child_id,
                page_start,
                page_end,
                node_hierarchy,
                rects,
                visible_nodes,
                nodes_map,
            );

            paginated_node.children.push(child_node.clone());
            nodes_map.insert(child_id, child_node);
        }

        // Move to next sibling
        child_id_opt = node_hierarchy[child_id].next_sibling_id();
    }

    // Update the map with the complete node
    nodes_map.insert(node_id, paginated_node.clone());

    paginated_node
}
