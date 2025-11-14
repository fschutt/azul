// Table layout support for generating anonymous table elements
// Based on CSS 2.1 Section 17.2.1: Anonymous table objects

use crate::dom::{Dom, NodeData, NodeType};
use alloc::vec::Vec;

/// Generates anonymous table elements according to CSS table layout rules.
/// 
/// This function ensures that:
/// - Table elements have proper table-row and table-cell children
/// - Non-table children of tables are wrapped in anonymous table-row and table-cell boxes
/// - The resulting DOM tree is suitable for table layout algorithms
///
/// Returns a tuple of (processed_dom, total_node_count) where total_node_count
/// includes all nodes in the processed tree (original + generated anonymous nodes).
pub fn generate_anonymous_table_elements(dom: Dom) -> (Dom, usize) {
    let mut total_count = 1; // Start with 1 for the root node
    let processed = generate_anonymous_table_elements_recursive(dom, &mut total_count);
    (processed, total_count)
}

fn generate_anonymous_table_elements_recursive(mut dom: Dom, node_count: &mut usize) -> Dom {
    // First, recursively process all children
    let mut processed_children = Vec::with_capacity(dom.children.as_ref().len());
    
    for child_dom in dom.children.into_library_owned_vec() {
        processed_children.push(generate_anonymous_table_elements_recursive(child_dom, node_count));
    }
    
    dom.children = processed_children.into();
    
    // Now apply table-specific wrapping rules based on the current node type
    match dom.root.node_type {
        NodeType::Table => {
            // Table children must be table-row-group, table-header-group, table-footer-group, 
            // table-row, or table-caption
            // Wrap any other children in anonymous table-row and table-cell
            let mut new_children = Vec::new();
            let mut pending_non_table_children = Vec::new();
            
            for child in dom.children.into_library_owned_vec() {
                let child_type = child.root.node_type.clone();
                
                match child_type {
                    NodeType::Caption | NodeType::THead | NodeType::TBody | 
                    NodeType::TFoot | NodeType::Tr => {
                        // These are valid table children
                        // First, wrap any pending non-table children
                        if !pending_non_table_children.is_empty() {
                            let wrapped = wrap_in_anonymous_table_row_and_cell(
                                pending_non_table_children,
                                node_count
                            );
                            new_children.push(wrapped);
                            pending_non_table_children = Vec::new();
                        }
                        new_children.push(child);
                    }
                    _ => {
                        // Non-table child, needs wrapping
                        pending_non_table_children.push(child);
                    }
                }
            }
            
            // Wrap any remaining non-table children
            if !pending_non_table_children.is_empty() {
                let wrapped = wrap_in_anonymous_table_row_and_cell(
                    pending_non_table_children,
                    node_count
                );
                new_children.push(wrapped);
            }
            
            dom.children = new_children.into();
        }
        
        NodeType::Tr => {
            // Table-row children must be table-cell (Td, Th)
            // Wrap any other children in anonymous table-cell
            let mut new_children = Vec::new();
            let mut pending_non_cell_children = Vec::new();
            
            for child in dom.children.into_library_owned_vec() {
                let child_type = child.root.node_type.clone();
                
                match child_type {
                    NodeType::Td | NodeType::Th => {
                        // Valid table-row children
                        if !pending_non_cell_children.is_empty() {
                            let wrapped = wrap_in_anonymous_table_cell(
                                pending_non_cell_children,
                                node_count
                            );
                            new_children.push(wrapped);
                            pending_non_cell_children = Vec::new();
                        }
                        new_children.push(child);
                    }
                    _ => {
                        // Non-cell child, needs wrapping
                        pending_non_cell_children.push(child);
                    }
                }
            }
            
            // Wrap any remaining non-cell children
            if !pending_non_cell_children.is_empty() {
                let wrapped = wrap_in_anonymous_table_cell(
                    pending_non_cell_children,
                    node_count
                );
                new_children.push(wrapped);
            }
            
            dom.children = new_children.into();
        }
        
        _ => {
            // For non-table elements, no special wrapping needed
        }
    }
    
    // Update estimated_total_children
    let mut total = 0;
    for child in dom.children.as_ref().iter() {
        total += 1 + child.estimated_total_children;
    }
    dom.estimated_total_children = total;
    
    dom
}

/// Wraps children in an anonymous table-row, then wraps that in an anonymous table-cell
fn wrap_in_anonymous_table_row_and_cell(
    children: Vec<Dom>,
    node_count: &mut usize
) -> Dom {
    // Create anonymous table-cell with the children
    let mut anon_cell = Dom::new(NodeType::Td);
    anon_cell.root.set_anonymous(true);
    anon_cell.children = children.into();
    
    // Update estimated children count
    let mut cell_total = 0;
    for child in anon_cell.children.as_ref().iter() {
        cell_total += 1 + child.estimated_total_children;
    }
    anon_cell.estimated_total_children = cell_total;
    
    *node_count += 1; // Count the anonymous cell
    
    // Create anonymous table-row containing the cell
    let mut anon_row = Dom::new(NodeType::Tr);
    anon_row.root.set_anonymous(true);
    anon_row.children = vec![anon_cell].into();
    anon_row.estimated_total_children = 1 + cell_total;
    
    *node_count += 1; // Count the anonymous row
    
    anon_row
}

/// Wraps children in an anonymous table-cell
fn wrap_in_anonymous_table_cell(
    children: Vec<Dom>,
    node_count: &mut usize
) -> Dom {
    let mut anon_cell = Dom::new(NodeType::Td);
    anon_cell.root.set_anonymous(true);
    anon_cell.children = children.into();
    
    // Update estimated children count
    let mut total = 0;
    for child in anon_cell.children.as_ref().iter() {
        total += 1 + child.estimated_total_children;
    }
    anon_cell.estimated_total_children = total;
    
    *node_count += 1; // Count the anonymous cell
    
    anon_cell
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_table_with_direct_div_child() {
        // <table><div>text</div></table>
        // Should become: <table><tr(anon)><td(anon)><div>text</div></td></tr></table>
        let div = Dom::new(NodeType::Div)
            .with_child(Dom::text("content"));
        
        let table = Dom::new(NodeType::Table)
            .with_child(div);
        
        let (processed, total) = generate_anonymous_table_elements(table);
        
        assert_eq!(processed.root.node_type, NodeType::Table);
        assert_eq!(processed.children.as_ref().len(), 1, "Table should have 1 child (anonymous TR)");
        
        let first_child = &processed.children.as_ref()[0];
        assert_eq!(first_child.root.node_type, NodeType::Tr, "First child should be TR");
        assert!(first_child.root.is_anonymous(), "TR should be anonymous");
        
        let tr_first_child = &first_child.children.as_ref()[0];
        assert_eq!(tr_first_child.root.node_type, NodeType::Td, "TR's child should be TD");
        assert!(tr_first_child.root.is_anonymous(), "TD should be anonymous");
        
        let td_first_child = &tr_first_child.children.as_ref()[0];
        assert_eq!(td_first_child.root.node_type, NodeType::Div, "TD's child should be the original Div");
        assert!(!td_first_child.root.is_anonymous(), "Original Div should not be anonymous");
        
        // Total: 1 (table) + 1 (anon tr) + 1 (anon td) + 1 (div) + 1 (text) = 5
        assert_eq!(total, 5, "Total node count should be 5");
    }
    
    #[test]
    fn test_tr_with_direct_span_child() {
        // <tr><span>text</span></tr>
        // Should become: <tr><td(anon)><span>text</span></td></tr>
        let span = Dom::new(NodeType::Span)
            .with_child(Dom::text("content"));
        
        let tr = Dom::new(NodeType::Tr)
            .with_child(span);
        
        let (processed, _) = generate_anonymous_table_elements(tr);
        
        assert_eq!(processed.root.node_type, NodeType::Tr);
        assert_eq!(processed.children.as_ref().len(), 1, "TR should have 1 child (anonymous TD)");
        
        let first_child = &processed.children.as_ref()[0];
        assert_eq!(first_child.root.node_type, NodeType::Td, "First child should be TD");
        assert!(first_child.root.is_anonymous(), "TD should be anonymous");
        
        let td_first_child = &first_child.children.as_ref()[0];
        assert_eq!(td_first_child.root.node_type, NodeType::Span, "TD's child should be the original Span");
    }
}
