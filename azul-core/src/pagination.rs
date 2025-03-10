use alloc::{collections::BTreeMap, vec::Vec};

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

    // Process each page
    for page_idx in 0..num_pages {
        let page_start = page_idx as f32 * page_height;
        let page_end = page_start + page_height;

        // Find visible nodes on this page
        let mut visible_nodes = BTreeMap::new();
        for i in 0..rects.len() {
            let node_id = NodeId::new(i);
            let r = &rects[node_id];
            let node_top = r.position.get_static_offset().y;
            let node_bottom = node_top + r.size.height;

            if node_bottom < page_start || node_top > page_end {
                continue;
            }

            // Clone and rebase rectangle for this page
            let mut new_rect = r.clone();
            new_rect.position.translate_vertical(-page_start);

            visible_nodes.insert(
                node_id,
                PaginatedNode {
                    id: node_id,
                    rect: LogicalRect {
                        origin: new_rect.position.get_static_offset(),
                        size: new_rect.size,
                    },
                    children: Vec::new(),
                },
            );
        }

        // If no nodes on page, skip
        if visible_nodes.is_empty() {
            continue;
        }

        // Establish parent-child relationships
        let mut nodes_map = BTreeMap::new();

        // First pass: collect all visible nodes
        for (id, node) in visible_nodes {
            nodes_map.insert(id, node);
        }

        // Second pass: establish parent-child relationships
        let mut root = None;
        let mut nodes_to_process = nodes_map.clone();

        while !nodes_to_process.is_empty() {
            let mut processed = Vec::new();

            for (&id, node) in &nodes_to_process {
                if id == NodeId::ZERO {
                    // Root node
                    root = Some(id);
                    processed.push(id);
                    continue;
                }

                // Find parent
                if let Some(parent_id) = node_hierarchy[id].parent_id() {
                    if nodes_map.contains_key(&parent_id) {
                        // Parent is visible on this page
                        let nid = nodes_map[&id].clone();
                        if let Some(parent_node) = nodes_map.get_mut(&parent_id) {
                            parent_node.children.push(nid);
                            processed.push(id);
                        }
                    } else {
                        // Parent not visible, make it a root if no root yet
                        if root.is_none() {
                            root = Some(id);
                        }
                        processed.push(id);
                    }
                } else {
                    // No parent, must be root
                    root = Some(id);
                    processed.push(id);
                }
            }

            // Remove processed nodes
            for id in processed.iter() {
                nodes_to_process.remove(id);
            }

            // Break if no progress made (safety check)
            if processed.is_empty() && !nodes_to_process.is_empty() {
                break;
            }
        }

        pages.push(PaginatedPage {
            root: root.map(|id| nodes_map[&id].clone()),
            nodes: nodes_map,
        });
    }

    pages
}

#[cfg(test)]
mod pagination_tests {
    use azul_css::LayoutPosition;

    use crate::{
        id_tree::{NodeDataContainer, NodeDataContainerRef, NodeId},
        pagination::{PaginatedPage, paginate_layout_result},
        styled_dom::NodeHierarchyItem,
        ui_solver::{PositionedRectangle, ResolvedOffsets, StyleBoxShadowOffsets},
        window::{LogicalPosition, LogicalSize},
    };

    fn create_test_node_hierarchy(node_count: usize) -> NodeDataContainer<NodeHierarchyItem> {
        let mut nodes = Vec::with_capacity(node_count);
        for i in 0..node_count {
            let mut node = NodeHierarchyItem::zeroed();
            if i > 0 {
                // Set parent for non-root nodes
                node.parent = 1; // Parent is root (id: 0)
            }
            nodes.push(node);
        }
        NodeDataContainer::new(nodes)
    }

    fn create_test_rects(
        config: &[(f32, f32, f32, f32)],
    ) -> NodeDataContainer<PositionedRectangle> {
        let mut rects = Vec::with_capacity(config.len());

        for &(x, y, width, height) in config {
            let rect = PositionedRectangle {
                position: crate::ui_solver::PositionInfo::Static(
                    crate::ui_solver::PositionInfoInner {
                        x_offset: x,
                        y_offset: y,
                        static_x_offset: x,
                        static_y_offset: y,
                    },
                ),
                size: LogicalSize::new(width, height),
                padding: ResolvedOffsets::zero(),
                margin: ResolvedOffsets::zero(),
                border_widths: ResolvedOffsets::zero(),
                box_shadow: StyleBoxShadowOffsets::default(),
                box_sizing: azul_css::LayoutBoxSizing::BorderBox,
                overflow_x: azul_css::LayoutOverflow::Auto,
                overflow_y: azul_css::LayoutOverflow::Auto,
                resolved_text_layout_options: None,
            };
            rects.push(rect);
        }

        NodeDataContainer::new(rects)
    }

    // Helper to count nodes on each page
    fn count_nodes_per_page(pages: &[PaginatedPage]) -> Vec<usize> {
        pages.iter().map(|page| page.nodes.len()).collect()
    }

    // Helper to find nodes that appear on multiple pages
    fn find_nodes_on_multiple_pages(pages: &[PaginatedPage]) -> Vec<NodeId> {
        let mut node_occurrences = std::collections::HashMap::new();

        for page in pages {
            for (&node_id, _) in &page.nodes {
                *node_occurrences.entry(node_id).or_insert(0) += 1;
            }
        }

        node_occurrences
            .into_iter()
            .filter(|(_, count)| *count > 1)
            .map(|(node_id, _)| node_id)
            .collect()
    }

    #[test]
    fn test_basic_pagination() {
        // Create a hierarchy with 3 nodes
        let node_hierarchy = create_test_node_hierarchy(3);

        // Create rectangles for all nodes - each 50 units tall and stacked vertically
        let rect_config = vec![
            (0.0, 0.0, 100.0, 50.0),   // Root node at top
            (0.0, 50.0, 100.0, 50.0),  // Child 1 in middle
            (0.0, 100.0, 100.0, 50.0), // Child 2 at bottom
        ];
        let rects = create_test_rects(&rect_config);

        // Paginate with a page height of 75.0 (should create 2 pages)
        let pages = paginate_layout_result(&node_hierarchy.as_ref(), &rects.as_ref(), 75.0);

        // Verify we got 2 pages
        assert_eq!(pages.len(), 2, "Should produce 2 pages");

        // First page should have nodes 0 and 1
        assert!(pages[0].nodes.contains_key(&NodeId::new(0)));
        assert!(pages[0].nodes.contains_key(&NodeId::new(1)));
        assert_eq!(pages[0].nodes.len(), 2);

        // Second page should have node 2
        assert!(pages[1].nodes.contains_key(&NodeId::new(2)));
        assert_eq!(pages[1].nodes.len(), 1);

        // Verify positions are adjusted
        // First page: positions remain as defined
        assert_eq!(pages[0].nodes[&NodeId::new(0)].rect.origin.y, 0.0);
        assert_eq!(pages[0].nodes[&NodeId::new(1)].rect.origin.y, 50.0);

        // Second page: positions adjusted by page height
        assert_eq!(pages[1].nodes[&NodeId::new(2)].rect.origin.y, 25.0); // 100 - 75
    }

    #[test]
    fn test_node_on_multiple_pages() {
        // Create a hierarchy with 3 nodes
        let node_hierarchy = create_test_node_hierarchy(3);

        // Create rectangles - one tall node spanning multiple pages
        let rect_config = vec![
            (0.0, 0.0, 100.0, 250.0),  // Root node spans all pages
            (0.0, 50.0, 100.0, 50.0),  // Child 1 on first page
            (0.0, 150.0, 100.0, 50.0), // Child 2 on second page
        ];
        let rects = create_test_rects(&rect_config);

        // Paginate with a page height of 100.0 (should create 3 pages)
        let pages = paginate_layout_result(&node_hierarchy.as_ref(), &rects.as_ref(), 100.0);

        // Verify we got 3 pages
        assert_eq!(pages.len(), 3, "Should produce 3 pages");

        // Find nodes appearing on multiple pages
        let multi_page_nodes = find_nodes_on_multiple_pages(&pages);

        // The root node should appear on all pages
        assert!(multi_page_nodes.contains(&NodeId::new(0)));
        assert_eq!(multi_page_nodes.len(), 1);

        // The root node should appear on all 3 pages
        assert!(pages[0].nodes.contains_key(&NodeId::new(0)));
        assert!(pages[1].nodes.contains_key(&NodeId::new(0)));
        assert!(pages[2].nodes.contains_key(&NodeId::new(0)));

        // Check positions on each page
        // Page 1: position is original
        assert_eq!(pages[0].nodes[&NodeId::new(0)].rect.origin.y, 0.0);
        // Page 2: position is adjusted by 100.0
        assert_eq!(pages[1].nodes[&NodeId::new(0)].rect.origin.y, -100.0);
        // Page 3: position is adjusted by 200.0
        assert_eq!(pages[2].nodes[&NodeId::new(0)].rect.origin.y, -200.0);
    }

    #[test]
    fn test_empty_layout() {
        // Create an empty hierarchy
        let node_hierarchy = create_test_node_hierarchy(0);
        let rects = create_test_rects(&[]);

        // Paginate with any page height
        let pages = paginate_layout_result(&node_hierarchy.as_ref(), &rects.as_ref(), 100.0);

        // Should result in no pages
        assert_eq!(pages.len(), 0, "Empty layout should produce no pages");
    }

    #[test]
    fn test_single_page_layout() {
        // Create a hierarchy with 3 nodes
        let node_hierarchy = create_test_node_hierarchy(3);

        // Create rectangles all fitting on one page
        let rect_config = vec![
            (0.0, 0.0, 100.0, 30.0),
            (0.0, 30.0, 100.0, 30.0),
            (0.0, 60.0, 100.0, 30.0),
        ];
        let rects = create_test_rects(&rect_config);

        // Paginate with a page height of 100.0 (should create 1 page)
        let pages = paginate_layout_result(&node_hierarchy.as_ref(), &rects.as_ref(), 100.0);

        // Verify we got 1 page
        assert_eq!(pages.len(), 1, "Should produce 1 page");

        // All nodes should be on the first page
        assert_eq!(pages[0].nodes.len(), 3);
        assert!(pages[0].nodes.contains_key(&NodeId::new(0)));
        assert!(pages[0].nodes.contains_key(&NodeId::new(1)));
        assert!(pages[0].nodes.contains_key(&NodeId::new(2)));
    }

    #[test]
    fn test_partially_visible_node() {
        // Create a hierarchy with 3 nodes
        let node_hierarchy = create_test_node_hierarchy(3);

        // Create rectangles with a node that crosses page boundary
        let rect_config = vec![
            (0.0, 0.0, 100.0, 50.0),   // Node 0: fully on page 1
            (0.0, 75.0, 100.0, 50.0),  // Node 1: crosses page boundary
            (0.0, 150.0, 100.0, 50.0), // Node 2: fully on page 2
        ];
        let rects = create_test_rects(&rect_config);

        // Paginate with a page height of 100.0 (should create 2 pages)
        let pages = paginate_layout_result(&node_hierarchy.as_ref(), &rects.as_ref(), 100.0);

        // Verify we got 2 pages
        assert_eq!(pages.len(), 2, "Should produce 2 pages");

        // Node 1 should appear on both pages
        assert!(pages[0].nodes.contains_key(&NodeId::new(1)));
        assert!(pages[1].nodes.contains_key(&NodeId::new(1)));

        // Check the multi-page nodes
        let multi_page_nodes = find_nodes_on_multiple_pages(&pages);
        assert!(multi_page_nodes.contains(&NodeId::new(1)));
    }

    #[test]
    fn test_large_document_pagination() {
        // Create a larger hierarchy
        let node_count = 20;
        let node_hierarchy = create_test_node_hierarchy(node_count);

        // Create rectangles spread evenly, 30 units tall each
        let mut rect_config = Vec::with_capacity(node_count);
        for i in 0..node_count {
            rect_config.push((0.0, i as f32 * 30.0, 100.0, 30.0));
        }
        let rects = create_test_rects(&rect_config);

        // Paginate with a page height of 100.0
        let pages = paginate_layout_result(&node_hierarchy.as_ref(), &rects.as_ref(), 100.0);

        // Calculate expected number of pages
        let total_height = (node_count as f32) * 30.0;
        let expected_pages = (total_height / 100.0).ceil() as usize;

        assert_eq!(
            pages.len(),
            expected_pages,
            "Should produce the expected number of pages"
        );

        // Verify distribution of nodes across pages
        let nodes_per_page = count_nodes_per_page(&pages);

        // Most pages should have about 3-4 nodes (100/30 ≈ 3.33)
        for count in &nodes_per_page[..nodes_per_page.len() - 1] {
            assert!(
                *count >= 3 && *count <= 4,
                "Most pages should have 3-4 nodes"
            );
        }
    }

    #[test]
    fn test_parent_child_relationships() {
        // Create a hierarchy with parent-child relationships
        let mut node_hierarchy = create_test_node_hierarchy(5);

        // Set up parent-child relationships
        // 0 is root
        // 1 and 3 are children of 0
        // 2 is a child of 1
        // 4 is a child of 3
        node_hierarchy.internal[1].parent = 1; // 1's parent is 0
        node_hierarchy.internal[2].parent = 2; // 2's parent is 1
        node_hierarchy.internal[3].parent = 1; // 3's parent is 0
        node_hierarchy.internal[4].parent = 4; // 4's parent is 3

        // Create rectangles spread vertically
        let rect_config = vec![
            (0.0, 0.0, 100.0, 200.0),  // Node 0: spans all
            (10.0, 10.0, 80.0, 90.0),  // Node 1: page 1
            (20.0, 50.0, 60.0, 40.0),  // Node 2: page 1
            (10.0, 110.0, 80.0, 90.0), // Node 3: crosses pages 1-2
            (20.0, 150.0, 60.0, 40.0), // Node 4: page 2
        ];
        let rects = create_test_rects(&rect_config);

        // Paginate with a page height of 100.0
        let pages = paginate_layout_result(&node_hierarchy.as_ref(), &rects.as_ref(), 100.0);

        // Verify parent-child relationships are preserved on each page

        // Page 1 should have nodes 0, 1, 2, and 3
        let page1 = &pages[0];
        assert!(page1.nodes.contains_key(&NodeId::new(0)));
        assert!(page1.nodes.contains_key(&NodeId::new(1)));
        assert!(page1.nodes.contains_key(&NodeId::new(2)));
        assert!(page1.nodes.contains_key(&NodeId::new(3)));

        // Check children of node 0 on page 1
        if let Some(root) = &page1.root {
            if root.id == NodeId::new(0) {
                let root_children: Vec<_> = root.children.iter().map(|child| child.id).collect();

                // Node 0 should have nodes 1 and 3 as children
                assert!(root_children.contains(&NodeId::new(1)));
                assert!(root_children.contains(&NodeId::new(3)));

                // Node 1 should have node 2 as a child
                let node1 = root
                    .children
                    .iter()
                    .find(|child| child.id == NodeId::new(1));
                if let Some(node1) = node1 {
                    assert_eq!(node1.children.len(), 1);
                    assert_eq!(node1.children[0].id, NodeId::new(2));
                } else {
                    panic!("Node 1 not found as child of root on page 1");
                }
            }
        }

        // Page 2 should have nodes 0, 3, and 4
        let page2 = &pages[1];
        assert!(page2.nodes.contains_key(&NodeId::new(0)));
        assert!(page2.nodes.contains_key(&NodeId::new(3)));
        assert!(page2.nodes.contains_key(&NodeId::new(4)));

        // Check children of node 3 on page 2
        if let Some(root) = &page2.root {
            // Find node 3 in the hierarchy
            let node3 = if root.id == NodeId::new(3) {
                Some(root)
            } else if root.id == NodeId::new(0) {
                root.children
                    .iter()
                    .find(|child| child.id == NodeId::new(3))
            } else {
                None
            };

            if let Some(node3) = node3 {
                // Node 3 should have node 4 as a child
                assert_eq!(node3.children.len(), 1);
                assert_eq!(node3.children[0].id, NodeId::new(4));
            } else {
                panic!("Node 3 not found on page 2");
            }
        }
    }

    #[test]
    fn test_exact_page_boundaries() {
        // Create a hierarchy with 4 nodes
        let node_hierarchy = create_test_node_hierarchy(4);

        // Create rectangles that exactly align with page boundaries
        let rect_config = vec![
            (0.0, 0.0, 100.0, 100.0),   // Node 0: exactly page 1
            (0.0, 100.0, 100.0, 100.0), // Node 1: exactly page 2
            (0.0, 200.0, 100.0, 100.0), // Node 2: exactly page 3
            (0.0, 0.0, 100.0, 300.0),   // Node 3: spans all pages
        ];
        let rects = create_test_rects(&rect_config);

        // Paginate with a page height of 100.0
        let pages = paginate_layout_result(&node_hierarchy.as_ref(), &rects.as_ref(), 100.0);

        // Verify page count
        assert_eq!(pages.len(), 3, "Should produce 3 pages");

        // Node 3 should appear on all pages
        for page in &pages {
            assert!(page.nodes.contains_key(&NodeId::new(3)));
        }

        // Check specific page contents
        assert!(pages[0].nodes.contains_key(&NodeId::new(0)));
        assert!(pages[1].nodes.contains_key(&NodeId::new(1)));
        assert!(pages[2].nodes.contains_key(&NodeId::new(2)));

        // Verify positions are properly adjusted
        assert_eq!(pages[0].nodes[&NodeId::new(0)].rect.origin.y, 0.0);
        assert_eq!(pages[1].nodes[&NodeId::new(1)].rect.origin.y, 0.0); // 100-100
        assert_eq!(pages[2].nodes[&NodeId::new(2)].rect.origin.y, 0.0); // 200-200
    }
}
