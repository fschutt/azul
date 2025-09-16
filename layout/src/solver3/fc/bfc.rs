//! solver3/fc/bfc.rs
//!
//! Block Formatting Context - stacks block-level boxes vertically

use azul_core::{
    styled_dom::StyledDom,
    window::{LogicalPosition, LogicalSize},
};
use azul_css::LayoutDebugMessage;

use super::{
    calculate_collapsed_margins, check_scrollbar_necessity, FormattingContextManager,
    LayoutConstraints, LayoutResult, OverflowBehavior,
};
use crate::solver3::{layout_tree::LayoutTree, LayoutError, Result};

/// Block layout manager that stacks children vertically
pub struct BlockLayoutManager {
    current_y: f32,
    max_width: f32,
}

impl BlockLayoutManager {
    pub fn new() -> Self {
        Self {
            current_y: 0.0,
            max_width: 0.0,
        }
    }
}

impl FormattingContextManager for BlockLayoutManager {
    fn layout(
        &mut self,
        tree: &mut LayoutTree,
        node_index: usize,
        constraints: &LayoutConstraints,
        styled_dom: &StyledDom,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<LayoutResult> {
        let node = tree.get(node_index).ok_or(LayoutError::InvalidTree)?;
        let children = node.children.clone();

        if children.is_empty() {
            return Ok(LayoutResult {
                positions: Vec::new(),
                overflow_size: None,
                baseline_offset: 0.0,
            });
        }

        debug_log(
            debug_messages,
            &format!("BFC: Laying out {} children", children.len()),
        );

        let mut positions = Vec::new();
        self.current_y = 0.0;
        self.max_width = 0.0;

        // Stack children vertically with margin collapsing
        let mut prev_bottom_margin = 0.0;

        for &child_index in &children {
            let child_position = self.layout_child(
                tree,
                child_index,
                constraints,
                styled_dom,
                prev_bottom_margin,
                debug_messages,
            )?;

            positions.push((child_index, child_position.position));

            // Update tracking for next child
            self.current_y = child_position.bottom_edge;
            self.max_width = self.max_width.max(child_position.width);
            prev_bottom_margin = child_position.bottom_margin;
        }

        // Check for overflow and scrollbars
        let content_size = LogicalSize::new(self.max_width, self.current_y);
        let container_size = constraints.available_size;

        let scrollbar_info = check_scrollbar_necessity(
            content_size,
            container_size,
            OverflowBehavior::Auto, // TODO: Get from CSS
            OverflowBehavior::Auto,
        );

        // If scrollbars are needed, re-layout with reduced available width
        if scrollbar_info.needs_vertical && !scrollbar_info.needs_horizontal {
            debug_log(debug_messages, "BFC: Re-laying out with vertical scrollbar");
            return self.relayout_with_scrollbars(
                tree,
                node_index,
                constraints,
                styled_dom,
                scrollbar_info.scrollbar_width,
                debug_messages,
            );
        }

        let overflow_size = if content_size.width > container_size.width
            || content_size.height > container_size.height
        {
            Some(content_size)
        } else {
            None
        };

        debug_log(
            debug_messages,
            &format!("BFC: Layout complete, content size: {:?}", content_size),
        );

        Ok(LayoutResult {
            positions,
            overflow_size,
            baseline_offset: 0.0, // TODO: Calculate proper baseline
        })
    }
}

impl BlockLayoutManager {
    fn layout_child(
        &mut self,
        tree: &mut LayoutTree,
        child_index: usize,
        constraints: &LayoutConstraints,
        styled_dom: &StyledDom,
        prev_bottom_margin: f32,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<ChildLayoutResult> {
        let child = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
        let child_size = child.used_size.unwrap_or(LogicalSize::zero());

        // Get margins from CSS (simplified)
        let margins = get_margins(styled_dom, child.dom_node_id);

        // Apply margin collapsing
        let collapsed_top = calculate_collapsed_margins(
            prev_bottom_margin,
            margins.top,
            true, // Adjacent margins collapse
        );

        let y_position = self.current_y + collapsed_top;
        let position = LogicalPosition::new(margins.left, y_position);

        // Recursively layout child's formatting context if needed
        if !child.children.is_empty() {
            let child_constraints = LayoutConstraints {
                available_size: LogicalSize::new(
                    constraints.available_size.width - margins.horizontal(),
                    f32::INFINITY, // Block children can expand vertically
                ),
                ..constraints.clone()
            };

            super::layout_formatting_context(
                tree,
                child_index,
                &child_constraints,
                styled_dom,
                &mut azul_core::app_resources::RendererResources::default(), // TODO: Pass properly
                debug_messages,
            )?;
        }

        Ok(ChildLayoutResult {
            position,
            width: child_size.width + margins.horizontal(),
            bottom_edge: y_position + child_size.height + margins.bottom,
            bottom_margin: margins.bottom,
        })
    }

    fn relayout_with_scrollbars(
        &mut self,
        tree: &mut LayoutTree,
        node_index: usize,
        constraints: &LayoutConstraints,
        styled_dom: &StyledDom,
        scrollbar_width: f32,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<LayoutResult> {
        // Create reduced constraints
        let reduced_constraints = LayoutConstraints {
            available_size: LogicalSize::new(
                constraints.available_size.width - scrollbar_width,
                constraints.available_size.height,
            ),
            ..constraints.clone()
        };

        // Re-run layout with reduced width
        self.layout(
            tree,
            node_index,
            &reduced_constraints,
            styled_dom,
            debug_messages,
        )
    }
}

struct ChildLayoutResult {
    position: LogicalPosition,
    width: f32,
    bottom_edge: f32,
    bottom_margin: f32,
}

#[derive(Debug, Clone, Copy, Default)]
struct Margins {
    top: f32,
    right: f32,
    bottom: f32,
    left: f32,
}

impl Margins {
    fn horizontal(&self) -> f32 {
        self.left + self.right
    }

    fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

fn get_margins(styled_dom: &StyledDom, node_id: Option<azul_core::dom::NodeId>) -> Margins {
    // Simplified margin extraction - real implementation would parse CSS
    let node_id = match node_id {
        Some(id) => id,
        None => return Margins::default(),
    };

    // TODO: Extract actual margins from CSS properties
    Margins {
        top: 0.0,
        right: 0.0,
        bottom: 0.0,
        left: 0.0,
    }
}

fn debug_log(debug_messages: &mut Option<Vec<LayoutDebugMessage>>, message: &str) {
    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: message.into(),
            location: "bfc".into(),
        });
    }
}
