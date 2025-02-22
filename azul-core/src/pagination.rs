use crate::id_tree::{NodeHierarchy, NodeDataContainer, NodeId};
use crate::dom::{Node, NodeData};
use crate::styled_dom::NodeHierarchyItem;
use crate::ui_solver::PositionedRectangle;
use alloc::collections::BTreeMap;

/// The per-page output: a partial `NodeHierarchy` plus the 
/// subset of `PositionedRectangle`s that appear on that page. 
/// Both arrays map 1:1 by index.
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

    // 1) compute total height from the root bounding box (NodeId(0)).
    //    If your actual root node is different, adjust accordingly.
    //    This is a minimal example.
    let total_height = rects[NodeId::ZERO].size.height;
    let num_pages = (total_height / page_height).ceil() as usize;

    // We'll do a BFS for each page. You can optimize to do it once, but for clarity we do repeated BFS.
    for page_idx in 0..num_pages {
        let page_start = page_idx as f32 * page_height;
        let page_end   = page_start + page_height;

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
            let node_top    = r.position.get_static_offset().y;
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
            // for BFS, we won't fix the parent/sibling pointers yet, that happens later
            // we do want to store the bounding box though
            let mut new_rect = r.clone();

            // Optionally rebase the `y` so that each page starts at zero:
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

            let prev  = old_item.previous_sibling_id();
            let new_prev = prev.and_then(|pid| old_to_new_id_map.get(&pid)).copied();

            let next  = old_item.next_sibling_id();
            let new_next = next.and_then(|pid| old_to_new_id_map.get(&pid)).copied();

            let last_child = old_item.last_child_id();
            let new_last_child = last_child
                .and_then(|pid| old_to_new_id_map.get(&pid)).copied();

            page_node_hierarchy[new_id.index()] = NodeHierarchyItem {
                parent:         new_parent.map(|nid| nid.index()+1).unwrap_or(0),
                previous_sibling: new_prev.map(|nid| nid.index()+1).unwrap_or(0),
                next_sibling:     new_next.map(|nid| nid.index()+1).unwrap_or(0),
                last_child:       new_last_child.map(|nid| nid.index()+1).unwrap_or(0),
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
