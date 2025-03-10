use crate::id_tree::{NodeDataContainer, NodeId};
use crate::styled_dom::NodeHierarchyItem;
use crate::ui_solver::PositionedRectangle;
use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// Recursive node structure for pagination
#[derive(Debug, Clone)]
pub struct PaginatedNode {
    /// Original NodeId
    pub id: NodeId,
    /// Bounding box for this node on this page
    pub rect: PositionedRectangle,
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
    let max_height = (0..rects.len()).map(|i| {
        let r = &rects[NodeId::new(i)];
        r.position.get_static_offset().y + r.size.height
    }).fold(0.0, f32::max);
    
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
            
            visible_nodes.insert(node_id, PaginatedNode {
                id: node_id,
                rect: new_rect,
                children: Vec::new(),
            });
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
