//! solver3/formatting_contexts/flex.rs
//! Flexbox layout manager (stub implementation)

use azul_core::{styled_dom::StyledDom, window::LogicalPosition};
use azul_css::LayoutDebugMessage;

use super::{FormattingContextManager, LayoutConstraints, LayoutResult};
use crate::solver3::{layout_tree::LayoutTree, LayoutError, Result};

pub struct FlexLayoutManager;

impl FlexLayoutManager {
    pub fn new() -> Self {
        Self
    }
}

impl FormattingContextManager for FlexLayoutManager {
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

        debug_log(
            debug_messages,
            &format!("Flex: Stub layout of {} flex items", children.len()),
        );

        // Stub: Position children in a row
        let mut positions = Vec::new();
        let mut current_x = 0.0;

        for &child_index in &children {
            let child = tree.get(child_index).ok_or(LayoutError::InvalidTree)?;
            let child_size = child.used_size.unwrap_or_default();

            positions.push((child_index, LogicalPosition::new(current_x, 0.0)));
            current_x += child_size.width;
        }

        Ok(LayoutResult {
            positions,
            overflow_size: None,
            baseline_offset: 0.0,
        })
    }
}

fn debug_log(debug_messages: &mut Option<Vec<LayoutDebugMessage>>, message: &str) {
    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: message.into(),
            location: "flex".into(),
        });
    }
}
