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

#[cfg(test)]
mod pagination_tests {
    use alloc::{collections::BTreeSet, vec::Vec};

    use crate::{
        id_tree::{NodeDataContainer, NodeId},
        pagination::paginate_layout_result,
        styled_dom::NodeHierarchyItem,
        ui_solver::{
            PositionInfo, PositionInfoInner, PositionedRectangle, ResolvedOffsets,
            StyleBoxShadowOffsets,
        },
        window::LogicalSize,
    };

    /// Minimal helper: creates a NodeHierarchyItem with the given parent
    /// and sets no siblings or child pointers (so the pagination code infers them).
    /// Creates a NodeDataContainer of length `count` such that:
    ///
    /// - Node 0 is the root (no parent).
    /// - Nodes 1..(count-1) are all children of node 0.
    /// - They form a sibling chain: node 1's next sibling is node 2, node 2's next sibling is node
    ///   3, etc.
    /// - This ensures that `first_child_id(NodeId(0))` => NodeId(1) and
    ///   `next_sibling_id(NodeId(i))` => NodeId(i+1).
    ///
    /// That way, your pagination code can discover all nodes correctly.
    fn create_test_node_hierarchy(count: usize) -> NodeDataContainer<NodeHierarchyItem> {
        // (unchanged) sets up node0 with last_child = (count) etc...
        // that part is correct so your “build_paginated_node” can discover siblings
        #![allow(unused_mut)]
        let mut items = vec![NodeHierarchyItem::zeroed(); count];
        if count == 0 {
            return NodeDataContainer::new(items);
        }
        // Node0 is root
        items[0].parent = 0;
        items[0].previous_sibling = 0;
        items[0].next_sibling = 0;

        if count > 1 {
            items[0].last_child = count;
        }
        for i in 1..count {
            items[i].parent = 1;
            items[i].last_child = 0;
            if i == 1 {
                items[i].previous_sibling = 0;
            } else {
                items[i].previous_sibling = i as usize;
            }
            if i == count - 1 {
                items[i].next_sibling = 0;
            } else {
                items[i].next_sibling = (i + 2) as usize;
            }
        }
        NodeDataContainer::new(items)
    }

    fn create_rects(config: &[(f32, f32, f32, f32)]) -> NodeDataContainer<PositionedRectangle> {
        let mut out = Vec::new();
        for &(x, y, w, h) in config {
            let rect = PositionedRectangle {
                position: PositionInfo::Static(PositionInfoInner {
                    x_offset: x,
                    y_offset: y,
                    static_x_offset: x,
                    static_y_offset: y,
                }),
                size: LogicalSize::new(w, h),
                padding: ResolvedOffsets::zero(),
                margin: ResolvedOffsets::zero(),
                border_widths: ResolvedOffsets::zero(),
                box_shadow: StyleBoxShadowOffsets::default(),
                box_sizing: azul_css::LayoutBoxSizing::BorderBox,
                overflow_x: azul_css::LayoutOverflow::Auto,
                overflow_y: azul_css::LayoutOverflow::Auto,
                resolved_text_layout_options: None,
            };
            out.push(rect);
        }
        NodeDataContainer::new(out)
    }

    /// Collects the set of node-IDs on a page
    fn page_node_ids(page: &crate::pagination::PaginatedPage) -> BTreeSet<NodeId> {
        page.nodes.keys().copied().collect()
    }

    #[test]
    fn test_basic_pagination() {
        // 3 stacked: node0 => y=0..50, node1 => y=50..100, node2 => y=100..150
        let hier = create_test_node_hierarchy(3);
        let rects = create_rects(&[
            (0.0, 0.0, 100.0, 50.0),
            (0.0, 50.0, 100.0, 50.0),
            (0.0, 100.0, 100.0, 50.0),
        ]);

        let pages = paginate_layout_result(&hier.as_ref(), &rects.as_ref(), 75.0);
        assert_eq!(pages.len(), 2);

        let p0 = page_node_ids(&pages[0]);
        let want0 = BTreeSet::from([NodeId::new(0), NodeId::new(1)]);
        assert_eq!(p0, want0, "page0 node set");

        // Include node0 as it's the parent of visible nodes
        let p1 = page_node_ids(&pages[1]);
        let want1 = BTreeSet::from([NodeId::new(0), NodeId::new(1), NodeId::new(2)]);
        assert_eq!(p1, want1, "page1 node set");
    }

    #[test]
    fn test_single_page_layout() {
        // 3 nodes all fit in a single page
        let hierarchy = create_test_node_hierarchy(3);
        let rects = create_rects(&[
            (0.0, 0.0, 100.0, 30.0),
            (0.0, 30.0, 100.0, 30.0),
            (0.0, 60.0, 100.0, 30.0),
        ]);
        // page height=100 => only 1 page
        let pages = paginate_layout_result(&hierarchy.as_ref(), &rects.as_ref(), 100.0);
        // everything should appear on page0
        assert_eq!(pages.len(), 1);
        let p0_ids = page_node_ids(&pages[0]);
        let expected = BTreeSet::from([NodeId::new(0), NodeId::new(1), NodeId::new(2)]);
        assert_eq!(p0_ids, expected, "all 3 nodes on the single page");
    }

    #[test]
    fn test_node_on_multiple_pages() {
        // Node0 is tall (y=0..250) => spans multiple pages
        // Node1 => y=50..100, Node2 => y=150..200
        let hierarchy = create_test_node_hierarchy(3);
        let rects = create_rects(&[
            (0.0, 0.0, 100.0, 250.0),  // node0 => big
            (0.0, 50.0, 100.0, 50.0),  // node1
            (0.0, 150.0, 100.0, 50.0), // node2
        ]);
        // page height=100 => likely 3 pages: [0..100, 100..200, 200..300]
        let pages = paginate_layout_result(&hierarchy.as_ref(), &rects.as_ref(), 100.0);
        assert_eq!(pages.len(), 3);

        // page0 => y=0..100 => node0 partially, node1.
        let p0_ids = page_node_ids(&pages[0]);
        // since node1 is y=50..100, it also belongs to page0
        let want0 = BTreeSet::from([NodeId::new(0), NodeId::new(1)]);
        assert_eq!(p0_ids, want0, "page0 nodes");

        // page1 => y=100..200 => node0 partially, node1 partially if it extends 50..100?
        // actually node1 is 50..100 => it doesn't overlap y=100..200, so maybe no node1
        // node2 => 150..200 => yes
        let p1_ids = page_node_ids(&pages[1]);
        let want1 = BTreeSet::from([NodeId::new(0), NodeId::new(2)]);
        assert_eq!(p1_ids, want1, "page1 nodes");

        // page2 => y=200..300 => node0 partially => maybe node2 if 150..200 intersects 200..300? no
        let p2_ids = page_node_ids(&pages[2]);
        let want2 = BTreeSet::from([NodeId::new(0)]);
        assert_eq!(p2_ids, want2, "page2 nodes");
    }

    #[test]
    fn test_parent_child_relationships() {
        // 5 nodes:
        // node0 => y=0..200 => big parent
        // node1 => y=10..100 => child
        // node2 => y=50..90 => child of node1
        // node3 => y=110..200 => child of node0, partially in 1st page at 110..100?? Actually that
        // is out-of-range => 110..100 is invalid. So it doesn't appear on page0
        // node4 => y=150..190 => child of node3 => only on page1
        //
        // page0 => y=0..100 => node0, node1, node2
        // page1 => y=100..200 => node0, node3, node4
        // The old test incorrectly placed node3 on page0. We'll fix that now.
        let mut items = create_test_node_hierarchy(5);
        // Force the parent references you want:
        // node1 => parent= node0
        items.internal[1].parent = 1;
        // node2 => parent= node1
        items.internal[2].parent = 2;
        // node3 => parent= node0
        items.internal[3].parent = 1;
        // node4 => parent= node3
        items.internal[4].parent = 4;

        let r = create_rects(&[
            (0.0, 0.0, 100.0, 200.0), /* node0 => y=0..200 => partial on page0 =>0..100, partial
                                       * on page1 =>100..200 */
            (10.0, 10.0, 80.0, 90.0),  // node1 => y=10..100 => page0
            (20.0, 50.0, 60.0, 40.0),  // node2 => y=50..90 => page0
            (10.0, 110.0, 80.0, 90.0), // node3 => y=110..200 => only on page1
            (20.0, 150.0, 60.0, 40.0), // node4 => y=150..190 => only on page1
        ]);
        // page height=100
        let pages = paginate_layout_result(&items.as_ref(), &r.as_ref(), 100.0);
        assert_eq!(pages.len(), 2);

        // page0 => y=0..100 => node0, node1, node2
        // node3 => starts at y=110 => not on page0
        // node4 => y=150 => not on page0
        let p0 = page_node_ids(&pages[0]);
        let want0 = BTreeSet::from([NodeId::new(0), NodeId::new(1), NodeId::new(2)]);
        assert_eq!(
            p0, want0,
            "page0 nodes (child node3 not included if it’s out of geometry)"
        );

        // page1 => y=100..200 => node0 partial, node3 => y=110..200 => node4 => y=150..190
        let p1 = page_node_ids(&pages[1]);
        let want1 = BTreeSet::from([NodeId::new(0), NodeId::new(3), NodeId::new(4)]);
        assert_eq!(p1, want1, "page1 nodes");
    }

    #[test]
    fn test_exact_page_boundaries() {
        // 4 nodes: node0 => y=0..100, node1 => y=100..200, node2 => y=200..300
        // node3 => y=0..300 => tall root
        let h = create_test_node_hierarchy(4);
        let r = create_rects(&[
            (0.0, 0.0, 100.0, 100.0),   // node0
            (0.0, 100.0, 100.0, 100.0), // node1
            (0.0, 200.0, 100.0, 100.0), // node2
            (0.0, 0.0, 100.0, 300.0),   // node3 => spans all
        ]);
        let pages = paginate_layout_result(&h.as_ref(), &r.as_ref(), 100.0);
        assert_eq!(pages.len(), 3);

        let p0 = page_node_ids(&pages[0]);
        let want0 = BTreeSet::from([NodeId::new(0), NodeId::new(3)]);
        assert_eq!(p0, want0);

        // Include node0 as it's the parent of node1
        let p1 = page_node_ids(&pages[1]);
        let want1 = BTreeSet::from([NodeId::new(0), NodeId::new(1), NodeId::new(3)]);
        assert_eq!(p1, want1);

        // Include node0 as it's the parent of node2
        let p2 = page_node_ids(&pages[2]);
        let want2 = BTreeSet::from([NodeId::new(0), NodeId::new(2), NodeId::new(3)]);
        assert_eq!(p2, want2);
    }

    #[test]
    fn test_partially_visible_node() {
        // node0 => y=0..50
        // node1 => y=75..125 => crosses boundary at 100
        // node2 => y=150..200 => only on 2nd page
        let h = create_test_node_hierarchy(3);
        let r = create_rects(&[
            (0.0, 0.0, 100.0, 50.0),
            (0.0, 75.0, 100.0, 50.0),
            (0.0, 150.0, 100.0, 50.0),
        ]);
        let pages = paginate_layout_result(&h.as_ref(), &r.as_ref(), 100.0);
        assert_eq!(pages.len(), 2);

        let p0 = page_node_ids(&pages[0]);
        let want0 = BTreeSet::from([NodeId::new(0), NodeId::new(1)]);
        assert_eq!(p0, want0, "page0 node set includes partial node1");

        // Include node0 as it's the parent of all nodes
        let p1 = page_node_ids(&pages[1]);
        let want1 = BTreeSet::from([NodeId::new(0), NodeId::new(1), NodeId::new(2)]);
        assert_eq!(
            p1, want1,
            "page1 node set includes partial node1 plus node2"
        );
    }

    #[test]
    fn test_large_document_pagination() {
        // 20 nodes => each 30 tall => total 600 => page height=100 => 6 pages
        let n = 20;
        let hierarchy = create_test_node_hierarchy(n);
        let mut cfg = Vec::with_capacity(n);
        for i in 0..n {
            cfg.push((0.0, i as f32 * 30.0, 100.0, 30.0));
        }
        let rects = create_rects(&cfg);
        let pages = paginate_layout_result(&hierarchy.as_ref(), &rects.as_ref(), 100.0);

        // total height= 20*30=600 => ceil(600/100)=6 pages
        assert_eq!(pages.len(), 6);

        // check distribution (which nodes appear on each page).
        // page0 => y=0..100 => node0..node3 inclusive (since node3 => y=90..120 partial => no,
        // wait, 3 => y=90..120 => partial => yes) Actually let's do it systematically:
        // node i => y=(i*30)..(i*30+30).
        // page j => y=(j*100)..(j*100+100).

        let mut page_nodes = vec![BTreeSet::new(); 6];
        for (page_index, page) in pages.iter().enumerate() {
            page_nodes[page_index] = page_node_ids(page);
        }

        // We can do a quick check: node i belongs to page floor((i*30)/100) and page
        // floor(((i*30)+29)/100) if partial overlap, etc. We'll just check that each node
        // is on at least one page, possibly two if it crosses a boundary at multiples of 100.
        // We'll do a sanity check that no page is empty except maybe the last if the total lines up
        // exactly.
        for i in 0..6 {
            assert!(!page_nodes[i].is_empty(), "page{i} shouldn't be empty here");
        }

        // We won't do exact membership sets for brevity, but if you want:
        //  - page0 => node0..node3 or node4
        //  - page1 => node3..node6 or node7
        // etc. The main point is that no negative coordinates are tested and partial membership is
        // allowed.

        // done
    }
}
