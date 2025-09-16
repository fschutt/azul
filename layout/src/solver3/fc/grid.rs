//! solver3/formatting_contexts/grid.rs
//! CSS Grid layout manager (stub implementation)

use azul_core::{styled_dom::StyledDom, window::LogicalPosition};
use azul_css::LayoutDebugMessage;

use super::{FormattingContextManager, LayoutConstraints, LayoutResult};
use crate::solver3::{layout_tree::LayoutTree, LayoutError, Result};

pub struct GridLayoutManager {
    columns: u32,
    row_height: f32,
}

impl GridLayoutManager {
    pub fn new() -> Self {
        Self {
            columns: 2, // Default 2-column grid for stub
            row_height: 100.0,
        }
    }

    pub fn with_columns(columns: u32) -> Self {
        Self {
            columns,
            row_height: 100.0,
        }
    }
}

impl FormattingContextManager for GridLayoutManager {
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
            &format!(
                "Grid: Stub layout of {} grid items in {}x? grid",
                children.len(),
                self.columns
            ),
        );

        // Stub: Position children in a grid
        let mut positions = Vec::new();
        let cell_width = constraints.available_size.width / self.columns as f32;

        for (i, &child_index) in children.iter().enumerate() {
            let row = i / self.columns as usize;
            let col = i % self.columns as usize;
            let x = col as f32 * cell_width;
            let y = row as f32 * self.row_height;

            positions.push((child_index, LogicalPosition::new(x, y)));
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
            location: "grid".into(),
        });
    }
}
