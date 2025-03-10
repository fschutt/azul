use crate::id_tree::{NodeHierarchy, NodeDataContainer, NodeId};
use crate::dom::{Node, NodeData};
use crate::styled_dom::NodeHierarchyItem;
use crate::ui_solver::PositionedRectangle;
use alloc::collections::BTreeMap;

/// The per-page output: a partial `NodeHierarchy` plus the 
/// subset of `PositionedRectangle`s that appear on that page. 
/// Both arrays map 1:1 by index.
#[derive(Debug)]
pub struct PaginatedPage {
    /// A newly-built array of NodeHierarchyItem, one entry per retained node,
    /// in the same relative order as `page_rects`.
    pub hierarchy: NodeDataContainer<NodeHierarchyItem>,

    /// The bounding boxes, parallel to `page_node_hierarchy`.
    pub rects: NodeDataContainer<PositionedRectangle>,

    /// Maps "original NodeId" -> "this page's NodeId" so you can look up
    /// which nodes ended up in this page.
    pub old_to_new_id_map: BTreeMap<NodeId, NodeId>,
}

/// Break a single large LayoutResult into multiple "pages" by y-coordinate.
pub fn paginate_layout_result<'a>(
    node_hierarchy: &crate::id_tree::NodeDataContainerRef<'a, NodeHierarchyItem>,
    rects: &crate::id_tree::NodeDataContainerRef<'a, PositionedRectangle>,
    page_height: f32,
) -> Vec<PaginatedPage>
{
    let mut pages = Vec::new();

    // Calculate total document height by finding the maximum bottom edge of any node
    let mut total_height = 0.0_f32;
    for i in 0..rects.len() {
        let node_id = NodeId::new(i);
        let r = &rects[node_id];
        let node_bottom = r.position.get_static_offset().y + r.size.height;
        total_height = total_height.max(node_bottom);
    }
    
    // Calculate number of pages based on total content height
    let num_pages = (total_height / page_height).ceil() as usize;

    // We'll do a BFS for each page
    for page_idx in 0..num_pages {
        let page_start = page_idx as f32 * page_height;
        let page_end = page_start + page_height;

        // We'll build arrays for the partial result:
        let mut page_node_hierarchy = Vec::<NodeHierarchyItem>::new();
        let mut page_rects_array = Vec::<PositionedRectangle>::new();

        // Map from "old NodeId" to "new NodeId" in these arrays
        let mut old_to_new_id_map = BTreeMap::<NodeId, NodeId>::new();

        // BFS queue
        let mut queue = Vec::new();
        queue.push(NodeId::ZERO);

        while let Some(cur_id) = queue.pop() {
            let r = &rects[cur_id];
            let node_top = r.position.get_static_offset().y;
            let node_bottom = node_top + r.size.height;

            // If the node is completely above or below, skip it
            if node_bottom < page_start || node_top > page_end {
                continue;
            }

            // This node belongs in this page. If we haven't already assigned
            // a "new" ID for it, do so now
            let new_id = match old_to_new_id_map.get(&cur_id) {
                Some(&already_there) => already_there,
                None => {
                    let new_idx = page_node_hierarchy.len();

                    // push placeholders
                    page_node_hierarchy.push(NodeHierarchyItem {
                        parent: 0,
                        previous_sibling: 0,
                        next_sibling: 0,
                        last_child: 0,
                    });
                    page_rects_array.push(PositionedRectangle::default());

                    let new_id = NodeId::new(new_idx);
                    old_to_new_id_map.insert(cur_id, new_id);
                    new_id
                }
            };

            // fill out the partial node data
            let mut new_rect = r.clone();

            // Rebase the `y` so that each page starts at zero:
            let offset_amount = page_start;
            new_rect.position.translate_vertical(-offset_amount);

            page_rects_array[new_id.index()] = new_rect;

            // BFS push children
            let old_node = &node_hierarchy[cur_id];
            if let Some(first_child) = old_node.first_child_id(cur_id) {
                // traverse siblings
                let mut c = first_child;
                loop {
                    queue.push(c);
                    let sibling_node = &node_hierarchy[c];
                    if let Some(next_sib) = sibling_node.next_sibling_id() {
                        c = next_sib;
                    } else {
                        break;
                    }
                }
            }
        } // while BFS

        // 2) fix up parent/sibling pointers in `page_node_hierarchy`
        for (old_id, &new_id) in &old_to_new_id_map {
            let old_item = &node_hierarchy[*old_id];

            let parent = old_item.parent_id();
            let new_parent = parent.and_then(|pid| old_to_new_id_map.get(&pid)).copied();

            let prev = old_item.previous_sibling_id();
            let new_prev = prev.and_then(|pid| old_to_new_id_map.get(&pid)).copied();

            let next = old_item.next_sibling_id();
            let new_next = next.and_then(|pid| old_to_new_id_map.get(&pid)).copied();

            let last_child = old_item.last_child_id();
            let new_last_child = last_child
                .and_then(|pid| old_to_new_id_map.get(&pid)).copied();

            page_node_hierarchy[new_id.index()] = NodeHierarchyItem {
                parent: new_parent.map(|nid| nid.index()+1).unwrap_or(0),
                previous_sibling: new_prev.map(|nid| nid.index()+1).unwrap_or(0),
                next_sibling: new_next.map(|nid| nid.index()+1).unwrap_or(0),
                last_child: new_last_child.map(|nid| nid.index()+1).unwrap_or(0),
            };
        }

        pages.push(PaginatedPage {
            hierarchy: NodeDataContainer::new(page_node_hierarchy),
            rects: NodeDataContainer::new(page_rects_array),
            old_to_new_id_map,
        });
    } // for each page

    pages
}

#[cfg(test)]
mod tests {
    
    use super::*;
    use crate::id_tree::{NodeDataContainer, NodeId};
    use crate::styled_dom::NodeHierarchyItem;
    use crate::window::{LogicalPosition, LogicalSize};
    use crate::ui_solver::{PositionInfo, PositionInfoInner, PositionedRectangle};

    #[test]
    fn test_pagination_basic() {
        // Create test data
        let mut hierarchy = Vec::new();
        let mut rects = Vec::new();
        
        // Root node
        hierarchy.push(NodeHierarchyItem {
            parent: 0,
            previous_sibling: 0,
            next_sibling: 0,
            last_child: 3, // Last child is node 3
        });
        
        // Add three child nodes
        for i in 1..4 {
            hierarchy.push(NodeHierarchyItem {
                parent: 1, // Child of root
                previous_sibling: if i > 1 { i } else { 0 },
                next_sibling: if i < 3 { i + 1 } else { 0 },
                last_child: 0,
            });
        }
        
        // Root rectangle - only reports 200px height but content extends beyond
        rects.push(PositionedRectangle {
            size: LogicalSize::new(100.0, 200.0),
            position: PositionInfo::Static(PositionInfoInner {
                x_offset: 0.0,
                y_offset: 0.0,
                static_x_offset: 0.0,
                static_y_offset: 0.0,
            }),
            ..Default::default()
        });
        
        // Child on page 1
        rects.push(PositionedRectangle {
            size: LogicalSize::new(100.0, 80.0),
            position: PositionInfo::Static(PositionInfoInner {
                x_offset: 0.0,
                y_offset: 0.0,
                static_x_offset: 0.0,
                static_y_offset: 10.0,
            }),
            ..Default::default()
        });
        
        // Child on page 2
        rects.push(PositionedRectangle {
            size: LogicalSize::new(100.0, 80.0),
            position: PositionInfo::Static(PositionInfoInner {
                x_offset: 0.0,
                y_offset: 0.0,
                static_x_offset: 0.0,
                static_y_offset: 120.0,
            }),
            ..Default::default()
        });
        
        // Child on page 3 (extends beyond root's height)
        rects.push(PositionedRectangle {
            size: LogicalSize::new(100.0, 80.0),
            position: PositionInfo::Static(PositionInfoInner {
                x_offset: 0.0,
                y_offset: 0.0,
                static_x_offset: 0.0,
                static_y_offset: 250.0, // Beyond root's reported height
            }),
            ..Default::default()
        });
        
        let hierarchy_container = NodeDataContainer::new(hierarchy);
        let rects_container = NodeDataContainer::new(rects);
        
        let page_height = 100.0;
        let pages = paginate_layout_result(
            &hierarchy_container.as_ref(),
            &rects_container.as_ref(),
            page_height
        );
        
        // Should create 4 pages (since content extends to y=330)
        assert_eq!(pages.len(), 4, "Expected 4 pages but got {}", pages.len());
        
        // Each page should have appropriate nodes
        assert!(pages[0].old_to_new_id_map.contains_key(&NodeId::new(1)), 
                "Page 1 should contain node 1");
        assert!(pages[1].old_to_new_id_map.contains_key(&NodeId::new(2)), 
                "Page 2 should contain node 2");
        assert!(pages[2].old_to_new_id_map.contains_key(&NodeId::new(3)), 
                "Page 3 should contain node 3");
    }

    #[test]
    fn test_pagination_overlapping_nodes() {
        // Test nodes that span across page boundaries
        let mut hierarchy = Vec::new();
        let mut rects = Vec::new();
        
        // Root node
        hierarchy.push(NodeHierarchyItem {
            parent: 0,
            previous_sibling: 0,
            next_sibling: 0,
            last_child: 2, // Last child is node 2
        });
        
        // Two child nodes
        for i in 1..3 {
            hierarchy.push(NodeHierarchyItem {
                parent: 1, // Child of root
                previous_sibling: if i > 1 { i } else { 0 },
                next_sibling: if i < 2 { i + 1 } else { 0 },
                last_child: 0,
            });
        }
        
        // Root rectangle
        rects.push(PositionedRectangle {
            size: LogicalSize::new(100.0, 200.0),
            position: PositionInfo::Static(PositionInfoInner {
                x_offset: 0.0,
                y_offset: 0.0,
                static_x_offset: 0.0,
                static_y_offset: 0.0,
            }),
            ..Default::default()
        });
        
        // Node that spans pages 1 and 2
        rects.push(PositionedRectangle {
            size: LogicalSize::new(100.0, 120.0), // Tall enough to cross page boundary
            position: PositionInfo::Static(PositionInfoInner {
                x_offset: 0.0,
                y_offset: 0.0,
                static_x_offset: 0.0,
                static_y_offset: 50.0, // Starts in page 1, extends into page 2
            }),
            ..Default::default()
        });
        
        // Node that spans pages 2 and 3
        rects.push(PositionedRectangle {
            size: LogicalSize::new(100.0, 120.0),
            position: PositionInfo::Static(PositionInfoInner {
                x_offset: 0.0,
                y_offset: 0.0,
                static_x_offset: 0.0,
                static_y_offset: 150.0, // Starts in page 2, extends into page 3
            }),
            ..Default::default()
        });
        
        let hierarchy_container = NodeDataContainer::new(hierarchy);
        let rects_container = NodeDataContainer::new(rects);
        
        let page_height = 100.0;
        let pages = paginate_layout_result(
            &hierarchy_container.as_ref(),
            &rects_container.as_ref(),
            page_height
        );
        
        // Should have 3 pages (content extends to y=270)
        assert_eq!(pages.len(), 3);
        
        // Node 1 should appear on both pages 1 and 2
        assert!(pages[0].old_to_new_id_map.contains_key(&NodeId::new(1)), 
                "Node 1 should be on page 1");
        assert!(pages[1].old_to_new_id_map.contains_key(&NodeId::new(1)), 
                "Node 1 should also be on page 2");
        
        // Node 2 should appear on both pages 2 and 3
        assert!(pages[1].old_to_new_id_map.contains_key(&NodeId::new(2)), 
                "Node 2 should be on page 2");
        assert!(pages[2].old_to_new_id_map.contains_key(&NodeId::new(2)), 
                "Node 2 should also be on page 3");
    }

    #[test]
    fn test_pagination_y_rebasing() {
        // Test y-coordinate rebasing for each page
        let mut hierarchy = Vec::new();
        let mut rects = Vec::new();
        
        // Root and two children
        hierarchy.push(NodeHierarchyItem { parent: 0, previous_sibling: 0, next_sibling: 0, last_child: 2 });
        hierarchy.push(NodeHierarchyItem { parent: 1, previous_sibling: 0, next_sibling: 2, last_child: 0 });
        hierarchy.push(NodeHierarchyItem { parent: 1, previous_sibling: 1, next_sibling: 0, last_child: 0 });
        
        // Root rectangle
        rects.push(PositionedRectangle {
            size: LogicalSize::new(100.0, 250.0),
            position: PositionInfo::Static(PositionInfoInner {
                x_offset: 0.0, y_offset: 0.0, static_x_offset: 0.0, static_y_offset: 0.0
            }),
            ..Default::default()
        });
        
        // Page 1 node
        rects.push(PositionedRectangle {
            size: LogicalSize::new(100.0, 50.0),
            position: PositionInfo::Static(PositionInfoInner {
                x_offset: 0.0, y_offset: 0.0, static_x_offset: 0.0, static_y_offset: 25.0
            }),
            ..Default::default()
        });
        
        // Page 2 node
        rects.push(PositionedRectangle {
            size: LogicalSize::new(100.0, 50.0),
            position: PositionInfo::Static(PositionInfoInner {
                x_offset: 0.0, y_offset: 0.0, static_x_offset: 0.0, static_y_offset: 125.0
            }),
            ..Default::default()
        });
        
        let hierarchy_container = NodeDataContainer::new(hierarchy);
        let rects_container = NodeDataContainer::new(rects);
        
        let page_height = 100.0;
        let pages = paginate_layout_result(
            &hierarchy_container.as_ref(),
            &rects_container.as_ref(),
            page_height
        );
        
        // Verify y-coordinate rebasing
        let node1_page1_id = pages[0].old_to_new_id_map[&NodeId::new(1)];
        let node1_y = pages[0].rects.as_ref()[node1_page1_id].position.get_static_offset().y;
        assert_eq!(node1_y, 25.0, "Node 1 should be at y=25 on page 1");
        
        let node2_page2_id = pages[1].old_to_new_id_map[&NodeId::new(2)];
        let node2_y = pages[1].rects.as_ref()[node2_page2_id].position.get_static_offset().y;
        assert_eq!(node2_y, 25.0, "Node 2 should be at y=25 on page 2 (125-100)");
    }

    #[test]
    fn test_pagination_empty_document() {
        // Test with an empty document (just root node)
        let mut hierarchy = Vec::new();
        let mut rects = Vec::new();
        
        // Root node only
        hierarchy.push(NodeHierarchyItem { 
            parent: 0, previous_sibling: 0, next_sibling: 0, last_child: 0 
        });
        
        // Root rectangle
        rects.push(PositionedRectangle {
            size: LogicalSize::new(100.0, 50.0),
            position: PositionInfo::Static(PositionInfoInner {
                x_offset: 0.0, y_offset: 0.0, static_x_offset: 0.0, static_y_offset: 0.0
            }),
            ..Default::default()
        });
        
        let hierarchy_container = NodeDataContainer::new(hierarchy);
        let rects_container = NodeDataContainer::new(rects);
        
        let page_height = 100.0;
        let pages = paginate_layout_result(
            &hierarchy_container.as_ref(),
            &rects_container.as_ref(),
            page_height
        );
        
        // Should have 1 page
        assert_eq!(pages.len(), 1, "Empty document should have 1 page");
        assert!(pages[0].old_to_new_id_map.contains_key(&NodeId::new(0)), 
                "Page should contain root node");
    }
}
