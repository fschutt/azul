use crate::id_tree::{NodeHierarchy, NodeDataContainer, NodeId};
use crate::dom::NodeData;
use crate::ui_solver::PositionedRectangle;
use alloc::collections::BTreeMap;

/// The per-page output: a partial `NodeHierarchy` plus the 
/// subset of `PositionedRectangle`s that appear on that page. 
/// Both arrays map 1:1 by index.
pub struct PaginatedPage {
    pub node_hierarchy: NodeHierarchy,
    pub page_rects: NodeDataContainer<PositionedRectangle>,
    /// Maps from "original node id" -> "this page's node id" (or None if not present)
    pub old_to_new_id_map: BTreeMap<NodeId, NodeId>,
}

/// Break a single large LayoutResult into multiple "pages" by y-coordinate.
pub fn paginate_layout_result(
    node_hierarchy: &NodeHierarchy,
    node_data: &NodeDataContainer<NodeData>,
    rects: &NodeDataContainer<PositionedRectangle>,
    page_height: f32,
) -> Vec<PaginatedPage>
{
    let mut pages = Vec::new();

    // 1) Find total height of the entire layout from the root bounding box
    //    For a multi-child root, pick the max bounding box, or track from the actual "root" node.
    //    Example: The root is NodeId(0):
    let total_height = rects.internal[0].size.height;

    // compute how many pages we need
    let num_pages =
        (total_height / page_height).ceil() as usize;

    // We'll BFS from the root for each page, building a partial hierarchy
    // This is a naive approach that visits the entire tree once per page.
    // If performance is an issue, you can do a single pass to partition everything.

    for page_index in 0..num_pages {
        let page_start_y = page_index as f32 * page_height;
        let page_end_y = page_start_y + page_height;

        // We'll keep new arrays for the partial NodeHierarchy
        let mut new_nodes = Vec::<crate::id_tree::Node>::new();
        let mut new_rects = Vec::<PositionedRectangle>::new();

        // We also need a map from old NodeId -> new NodeId
        let mut old_to_new_id_map = BTreeMap::<NodeId, NodeId>::new();

        // BFS queue
        let mut queue = vec![NodeId::new(0)];

        while let Some(cur_id) = queue.pop() {
            let r = &rects.internal[cur_id.index()];
            let node_top = r.position.get_static_offset().y;
            let node_bottom = node_top + r.size.height;

            // If the node is entirely outside this page's y-range, skip
            if node_bottom < page_start_y || node_top > page_end_y {
                continue;
            }

            // Otherwise, we want to keep it. Create a new Node entry, plus a new rect entry
            // We have to replicate the parent's / siblings indices, but in new indices.

            // If we have NOT yet assigned a new ID, we create one
            let new_id = match old_to_new_id_map.get(&cur_id) {
                Some(nid) => *nid,
                None => {
                    let new_idx = new_nodes.len();
                    // Insert a placeholder Node
                    new_nodes.push(crate::id_tree::ROOT_NODE);
                    new_rects.push(PositionedRectangle::default());
                    let new_id = NodeId::new(new_idx);
                    old_to_new_id_map.insert(cur_id, new_id);
                    new_id
                }
            };

            // Fill out new_node & new_rect
            // copy the old Node
            let old_node = node_hierarchy.internal[cur_id.index()];
            // We'll fix up the parent / sibling pointers AFTER BFS
            // so for now store them in a temporary structure
            new_nodes[new_id.index()] = crate::id_tree::Node {
                parent: None,
                previous_sibling: None,
                next_sibling: None,
                last_child: None,
            };

            // Copy the old bounding box, optionally rebase "top" so that it starts at 0
            let mut new_rect = r.clone();
            // Example: rebase so that page Y=0 is oldY=page_start_y
            let offset_amount = page_start_y;
            new_rect.position
                .translate_vertical(-offset_amount);

            new_rects[new_id.index()] = new_rect;

            // BFS into the children: we only push them if they're not fully outside
            // We do not decide whether to skip them *yet*, we do that once we pop them from the queue
            if let Some(first_child) = old_node.get_first_child(cur_id) {
                // push all siblings
                let mut c = first_child;
                while let Some(n) = Some(c) {
                    queue.push(n);
                    let c_node = node_hierarchy.internal[c.index()];
                    if let Some(ns) = c_node.next_sibling {
                        c = ns;
                    } else {
                        break;
                    }
                }
            }
        }

        // 2) Now fix up the parent / sibling pointers in new_nodes
        //    We only keep them if the parent's old ID is in old_to_new_id_map
        for (old_id, new_id) in &old_to_new_id_map {
            let old_node = node_hierarchy.internal[old_id.index()];

            let old_parent = old_node.parent;
            let old_prev = old_node.previous_sibling;
            let old_next = old_node.next_sibling;
            let old_last_child = old_node.last_child;

            let new_parent = old_parent
                .and_then(|pid| old_to_new_id_map.get(&pid))
                .copied();
            let new_prev = old_prev
                .and_then(|pid| old_to_new_id_map.get(&pid))
                .copied();
            let new_next = old_next
                .and_then(|pid| old_to_new_id_map.get(&pid))
                .copied();
            let new_last_child = old_last_child
                .and_then(|pid| old_to_new_id_map.get(&pid))
                .copied();

            new_nodes[new_id.index()].parent = new_parent;
            new_nodes[new_id.index()].previous_sibling = new_prev;
            new_nodes[new_id.index()].next_sibling = new_next;
            new_nodes[new_id.index()].last_child = new_last_child;
        }

        // Create final NodeHierarchy + Container
        let partial_hierarchy = NodeHierarchy {
            internal: new_nodes,
        };
        let partial_rects = NodeDataContainer {
            internal: new_rects,
        };

        pages.push(PaginatedPage {
            node_hierarchy: partial_hierarchy,
            page_rects: partial_rects,
            old_to_new_id_map,
        });
    }

    pages
}
