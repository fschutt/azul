//! Determines the CSS formatting context for each item

use azul_core::{id_tree::NodeDataContainer, styled_dom::StyledDom};
use azul_css::{LayoutDisplay, LayoutFloat, LayoutOverflow, LayoutPosition, CssProperty};

/// Represents the CSS formatting context for an element
#[derive(Debug, Clone, PartialEq)]
pub enum FormattingContext {
    /// Block-level formatting context
    Block {
        /// Whether this element establishes a new block formatting context
        establishes_new_context: bool
    },
    /// Inline-level formatting context
    Inline,
    /// Inline-block (participates in an IFC but creates a BFC)
    InlineBlock,
    /// Flex formatting context
    Flex,
    /// Float (left or right)
    Float(LayoutFloat),
    /// Absolutely positioned (out of flow)
    OutOfFlow(LayoutPosition),
    /// No formatting context (display: none)
    None,
}

/// Determines the formatting context for each node in the DOM
pub fn determine_formatting_contexts(styled_dom: &StyledDom) -> NodeDataContainer<FormattingContext> {
    let css_property_cache = styled_dom.get_css_property_cache();
    let node_data_container = styled_dom.node_data.as_container();
    let styled_nodes = styled_dom.styled_nodes.as_container();
    
    // Transform each node to determine its formatting context
    node_data_container.transform_singlethread(|node_data, node_id| {
        let styled_node_state = &styled_nodes[node_id].state;
        
        // Get relevant CSS properties
        let display = css_property_cache
            .get_display(node_data, &node_id, styled_node_state)
            .and_then(|p| p.get_property().copied())
            .unwrap_or(LayoutDisplay::Block);
        
        let position = css_property_cache
            .get_position(node_data, &node_id, styled_node_state)
            .and_then(|p| p.get_property().copied())
            .unwrap_or(LayoutPosition::Static);
        
        let float = css_property_cache
            .get_float(node_data, &node_id, styled_node_state)
            .and_then(|p| p.get_property().copied())
            .unwrap_or(LayoutFloat::None);
        
        let overflow_x = css_property_cache
            .get_overflow_x(node_data, &node_id, styled_node_state)
            .and_then(|p| p.get_property().copied())
            .unwrap_or(LayoutOverflow::Visible);
        
        let overflow_y = css_property_cache
            .get_overflow_y(node_data, &node_id, styled_node_state)
            .and_then(|p| p.get_property().copied())
            .unwrap_or(LayoutOverflow::Visible);
        
        // Apply the CSS rules for determining formatting context
        
        // 1. Check for display: none
        if display == LayoutDisplay::None {
            return FormattingContext::None;
        }
        
        // 2. Check position property (highest precedence)
        if position == LayoutPosition::Absolute || position == LayoutPosition::Fixed {
            return FormattingContext::OutOfFlow(position);
        }
        
        // 3. Check float property
        if float != LayoutFloat::None {
            return FormattingContext::Float(float);
        }
        
        // 4. Determine context based on display property
        match display {
            LayoutDisplay::Block => {
                // Determine if it establishes a new BFC
                let establishes_new_context = 
                    overflow_x != LayoutOverflow::Visible || 
                    overflow_y != LayoutOverflow::Visible ||
                    position == LayoutPosition::Relative; // Positioned elements establish a BFC
                
                FormattingContext::Block { establishes_new_context }
            },
            LayoutDisplay::Inline => FormattingContext::Inline,
            LayoutDisplay::InlineBlock => FormattingContext::InlineBlock,
            LayoutDisplay::Flex => FormattingContext::Flex,
            _ => FormattingContext::Block { establishes_new_context: false }, // Default to block
        }
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use azul_css::{
        CssPropertyType, CssPropertyValue, LayoutDisplay, LayoutFloat, LayoutOverflow, LayoutPosition
    };
    use azul_core::{
        dom::{Node, NodeData, NodeId},
        styled_dom::{CssPropertyCache, CssPropertyCachePtr, NodeHierarchyItem, StyledDom, StyledNode, StyledNodeState},
        window::LogicalSize,
    };
    
    fn create_test_dom(properties: Vec<(NodeId, CssPropertyType, CssProperty)>) -> StyledDom {
        // Create a minimal StyledDom for testing
        let mut styled_dom = StyledDom::default();
        
        // Add nodes to the DOM
        styled_dom.node_data = (0..properties.len() + 1).map(|_| NodeData::default()).collect::<Vec<_>>().into();
        
        styled_dom.styled_nodes = (0..properties.len() + 1).map(|_| StyledNode::default()).collect::<Vec<_>>().into();
        
        // Set up basic hierarchy using Node::ROOT for the first node
        let mut node_hierarchy = vec![];
        
        // Root node - properly initialized
        // If we have child nodes, set the last_child to point to the last one
        let root_node = Node {
            parent: None, 
            previous_sibling: None,
            next_sibling: None,
            last_child: if properties.is_empty() { None } else { Some(NodeId::new(properties.len())) }
        };
        node_hierarchy.push(NodeHierarchyItem::from(root_node));
        
        for i in 1..=properties.len() {
            // Create a Node and convert it to NodeHierarchyItem
            let node = azul_core::id_tree::Node {
                parent: Some(NodeId::new(0)),
                previous_sibling: if i > 1 { Some(NodeId::new(i-1)) } else { None },
                next_sibling: if i < properties.len() { Some(NodeId::new(i+1)) } else { None },
                last_child: None,
            };
            node_hierarchy.push(NodeHierarchyItem::from(node));
        }
        
        // Convert Vec<NodeHierarchyItem> to NodeHierarchyItemVec
        styled_dom.node_hierarchy = node_hierarchy.into();
        
        // Apply the CSS properties
        let mut property_cache = CssPropertyCache::default();
        property_cache.node_count = properties.len() + 1;
        
        for (node_id, property_type, property_value) in properties {
            // Insert properties directly into the normal properties map
            property_cache.css_normal_props
                .entry(node_id)
                .or_insert_with(|| BTreeMap::new())
                .insert(property_type, property_value);
        }
        
        // Convert CssPropertyCache to CssPropertyCachePtr
        styled_dom.css_property_cache = CssPropertyCachePtr::new(property_cache);
        
        styled_dom
    }
    #[test]
    fn test_display_block() {
        // Create a DOM with a block element
        let styled_dom = create_test_dom(vec![
            (NodeId::new(1), CssPropertyType::Display, CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::Block)))
        ]);
        
        let formatting_contexts = determine_formatting_contexts(&styled_dom);
        
        // Root is default block
        assert_eq!(
            formatting_contexts.as_ref()[NodeId::new(0)], 
            FormattingContext::Block { establishes_new_context: false }
        );
        
        // Node 1 should be block
        assert_eq!(
            formatting_contexts.as_ref()[NodeId::new(1)], 
            FormattingContext::Block { establishes_new_context: false }
        );
    }
    
    #[test]
    fn test_display_inline() {
        let styled_dom = create_test_dom(vec![
            (NodeId::new(1), CssPropertyType::Display, CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::Inline)))
        ]);
        
        let formatting_contexts = determine_formatting_contexts(&styled_dom);
        
        assert_eq!(
            formatting_contexts.as_ref()[NodeId::new(1)], 
            FormattingContext::Inline
        );
    }
    
    #[test]
    fn test_display_inline_block() {
        let styled_dom = create_test_dom(vec![
            (NodeId::new(1), CssPropertyType::Display, CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::InlineBlock)))
        ]);
        
        let formatting_contexts = determine_formatting_contexts(&styled_dom);
        
        assert_eq!(
            formatting_contexts.as_ref()[NodeId::new(1)], 
            FormattingContext::InlineBlock
        );
    }
    
    #[test]
    fn test_display_flex() {
        let styled_dom = create_test_dom(vec![
            (NodeId::new(1), CssPropertyType::Display, CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::Flex)))
        ]);
        
        let formatting_contexts = determine_formatting_contexts(&styled_dom);
        
        assert_eq!(
            formatting_contexts.as_ref()[NodeId::new(1)], 
            FormattingContext::Flex
        );
    }
    
    #[test]
    fn test_display_none() {
        let styled_dom = create_test_dom(vec![
            (NodeId::new(1), CssPropertyType::Display, CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::None)))
        ]);
        
        let formatting_contexts = determine_formatting_contexts(&styled_dom);
        
        assert_eq!(
            formatting_contexts.as_ref()[NodeId::new(1)], 
            FormattingContext::None
        );
    }
    
    #[test]
    fn test_position_absolute() {
        let styled_dom = create_test_dom(vec![
            (NodeId::new(1), CssPropertyType::Position, CssProperty::Position(CssPropertyValue::Exact(LayoutPosition::Absolute)))
        ]);
        
        let formatting_contexts = determine_formatting_contexts(&styled_dom);
        
        assert_eq!(
            formatting_contexts.as_ref()[NodeId::new(1)], 
            FormattingContext::OutOfFlow(LayoutPosition::Absolute)
        );
    }
    
    #[test]
    fn test_position_fixed() {
        let styled_dom = create_test_dom(vec![
            (NodeId::new(1), CssPropertyType::Position, CssProperty::Position(CssPropertyValue::Exact(LayoutPosition::Fixed)))
        ]);
        
        let formatting_contexts = determine_formatting_contexts(&styled_dom);
        
        assert_eq!(
            formatting_contexts.as_ref()[NodeId::new(1)], 
            FormattingContext::OutOfFlow(LayoutPosition::Fixed)
        );
    }
    
    #[test]
    fn test_position_relative() {
        let styled_dom = create_test_dom(vec![
            (NodeId::new(1), CssPropertyType::Position, CssProperty::Position(CssPropertyValue::Exact(LayoutPosition::Relative)))
        ]);
        
        let formatting_contexts = determine_formatting_contexts(&styled_dom);
        
        // Relative positioning establishes a new BFC
        assert_eq!(
            formatting_contexts.as_ref()[NodeId::new(1)], 
            FormattingContext::Block { establishes_new_context: true }
        );
    }
    
    #[test]
    fn test_float_left() {
        let styled_dom = create_test_dom(vec![
            (NodeId::new(1), CssPropertyType::Float, CssProperty::Float(CssPropertyValue::Exact(LayoutFloat::Left)))
        ]);
        
        let formatting_contexts = determine_formatting_contexts(&styled_dom);
        
        assert_eq!(
            formatting_contexts.as_ref()[NodeId::new(1)], 
            FormattingContext::Float(LayoutFloat::Left)
        );
    }
    
    #[test]
    fn test_float_right() {
        let styled_dom = create_test_dom(vec![
            (NodeId::new(1), CssPropertyType::Float, CssProperty::Float(CssPropertyValue::Exact(LayoutFloat::Right)))
        ]);
        
        let formatting_contexts = determine_formatting_contexts(&styled_dom);
        
        assert_eq!(
            formatting_contexts.as_ref()[NodeId::new(1)], 
            FormattingContext::Float(LayoutFloat::Right)
        );
    }
    
    #[test]
    fn test_overflow_non_visible() {
        let styled_dom = create_test_dom(vec![
            (NodeId::new(1), CssPropertyType::OverflowX, CssProperty::OverflowX(CssPropertyValue::Exact(LayoutOverflow::Auto)))
        ]);
        
        let formatting_contexts = determine_formatting_contexts(&styled_dom);
        
        // Non-visible overflow establishes a new BFC
        assert_eq!(
            formatting_contexts.as_ref()[NodeId::new(1)], 
            FormattingContext::Block { establishes_new_context: true }
        );
    }
    
    #[test]
    fn test_overflow_only_y_non_visible() {
        let styled_dom = create_test_dom(vec![
            (NodeId::new(1), CssPropertyType::OverflowX, CssProperty::OverflowX(CssPropertyValue::Exact(LayoutOverflow::Visible))),
            (NodeId::new(1), CssPropertyType::OverflowY, CssProperty::OverflowY(CssPropertyValue::Exact(LayoutOverflow::Scroll)))
        ]);
        
        let formatting_contexts = determine_formatting_contexts(&styled_dom);
        
        // Even if only one overflow is non-visible, it establishes a new BFC
        assert_eq!(
            formatting_contexts.as_ref()[NodeId::new(1)], 
            FormattingContext::Block { establishes_new_context: true }
        );
    }
    
    #[test]
    fn test_precedence() {
        // Test precedence: position > float > display
        let styled_dom = create_test_dom(vec![
            (NodeId::new(1), CssPropertyType::Display, CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::Flex))),
            (NodeId::new(1), CssPropertyType::Float, CssProperty::Float(CssPropertyValue::Exact(LayoutFloat::Left))),
            (NodeId::new(1), CssPropertyType::Position, CssProperty::Position(CssPropertyValue::Exact(LayoutPosition::Absolute)))
        ]);
        
        let formatting_contexts = determine_formatting_contexts(&styled_dom);
        
        // Position: absolute wins
        assert_eq!(
            formatting_contexts.as_ref()[NodeId::new(1)], 
            FormattingContext::OutOfFlow(LayoutPosition::Absolute)
        );
        
        // Test float > display
        let styled_dom = create_test_dom(vec![
            (NodeId::new(1), CssPropertyType::Display, CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::Flex))),
            (NodeId::new(1), CssPropertyType::Float, CssProperty::Float(CssPropertyValue::Exact(LayoutFloat::Left)))
        ]);
        
        let formatting_contexts = determine_formatting_contexts(&styled_dom);
        
        // Float: left wins over display: flex
        assert_eq!(
            formatting_contexts.as_ref()[NodeId::new(1)], 
            FormattingContext::Float(LayoutFloat::Left)
        );
    }
    
    #[test]
    fn test_complex_tree() {
        // Test a more complex tree with mixed formatting contexts
        let styled_dom = create_test_dom(vec![
            // Node 1: Block
            (NodeId::new(1), CssPropertyType::Display, CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::Block))),
            // Node 2: Inline
            (NodeId::new(2), CssPropertyType::Display, CssProperty::Display(CssPropertyValue::Exact(LayoutDisplay::Inline))),
            // Node 3: Floated
            (NodeId::new(3), CssPropertyType::Float, CssProperty::Float(CssPropertyValue::Exact(LayoutFloat::Right))),
            // Node 4: Absolute
            (NodeId::new(4), CssPropertyType::Position, CssProperty::Position(CssPropertyValue::Exact(LayoutPosition::Absolute))),
            // Node 5: Block with new BFC
            (NodeId::new(5), CssPropertyType::OverflowX, CssProperty::OverflowX(CssPropertyValue::Exact(LayoutOverflow::Auto)))
        ]);
        
        let formatting_contexts = determine_formatting_contexts(&styled_dom);
        
        assert_eq!(formatting_contexts.as_ref()[NodeId::new(1)], FormattingContext::Block { establishes_new_context: false });
        assert_eq!(formatting_contexts.as_ref()[NodeId::new(2)], FormattingContext::Inline);
        assert_eq!(formatting_contexts.as_ref()[NodeId::new(3)], FormattingContext::Float(LayoutFloat::Right));
        assert_eq!(formatting_contexts.as_ref()[NodeId::new(4)], FormattingContext::OutOfFlow(LayoutPosition::Absolute));
        assert_eq!(formatting_contexts.as_ref()[NodeId::new(5)], FormattingContext::Block { establishes_new_context: true });
    }
}