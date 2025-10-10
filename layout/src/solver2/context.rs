//! Determines the CSS formatting context for each item

use azul_core::{
    dom::NodeType, id_tree::NodeDataContainer, styled_dom::StyledDom, ui_solver::FormattingContext,
};
use azul_css::props::{
    layout::{LayoutDisplay, LayoutFloat, LayoutOverflow, LayoutPosition},
    property::CssProperty,
};

/// Determines the formatting context for each node in the DOM
pub fn determine_formatting_contexts(
    styled_dom: &StyledDom,
) -> NodeDataContainer<FormattingContext> {
    let css_property_cache = styled_dom.get_css_property_cache();
    let node_data_container = styled_dom.node_data.as_container();
    let styled_nodes = styled_dom.styled_nodes.as_container();

    // Transform each node to determine its formatting context
    node_data_container.transform_singlethread(|node_data, node_id| {
        let styled_node_state = &styled_nodes[node_id].state;

        // Default display based on node type
        let default_display = node_data.get_node_type().get_default_display();

        // Get relevant CSS properties
        let display = css_property_cache
            .get_display(node_data, &node_id, styled_node_state)
            .and_then(|p| p.get_property().copied())
            .unwrap_or(default_display); // Use node-specific default

        // Rest of the function remains the same
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
                let establishes_new_context = overflow_x != LayoutOverflow::Visible
                    || overflow_y != LayoutOverflow::Visible
                    || position == LayoutPosition::Relative; // Positioned elements establish a BFC

                FormattingContext::Block {
                    establishes_new_context,
                }
            }
            LayoutDisplay::Inline => FormattingContext::Inline,
            LayoutDisplay::InlineBlock => FormattingContext::InlineBlock,
            LayoutDisplay::Flex => FormattingContext::Flex,
            _ => FormattingContext::Block {
                establishes_new_context: false,
            }, // Default to block
        }
    })
}
