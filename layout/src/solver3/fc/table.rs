//! solver3/formatting_contexts/table.rs
//! CSS Table layout manager (stub implementation)

use azul_core::{styled_dom::StyledDom, window::LogicalPosition};
use azul_css::LayoutDebugMessage;

use super::{FormattingContextManager, LayoutConstraints, LayoutResult};
use crate::solver3::{layout_tree::LayoutTree, LayoutError, Result};

pub struct TableLayoutManager {
    row_height: f32,
    column_widths: Vec<f32>,
}

impl TableLayoutManager {
    pub fn new() -> Self {
        Self {
            row_height: 30.0,
            column_widths: Vec::new(),
        }
    }
}

impl FormattingContextManager for TableLayoutManager {
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
            &format!("Table: Stub layout of {} table rows", children.len()),
        );

        // Stub: Position children as table rows stacked vertically
        let mut positions = Vec::new();

        for (i, &child_index) in children.iter().enumerate() {
            let y = i as f32 * self.row_height;
            positions.push((child_index, LogicalPosition::new(0.0, y)));

            // For table cells within rows, position them horizontally
            if let Some(row_node) = tree.get(child_index) {
                self.layout_table_row(tree, child_index, constraints, y, debug_messages)?;
            }
        }

        Ok(LayoutResult {
            positions,
            overflow_size: None,
            baseline_offset: 0.0,
        })
    }
}

impl TableLayoutManager {
    fn layout_table_row(
        &mut self,
        tree: &mut LayoutTree,
        row_index: usize,
        constraints: &LayoutConstraints,
        row_y: f32,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<()> {
        let row_node = tree.get(row_index).ok_or(LayoutError::InvalidTree)?;
        let cells = row_node.children.clone();

        if cells.is_empty() {
            return Ok(());
        }

        // Distribute available width equally among cells (simplified)
        let cell_width = constraints.available_size.width / cells.len() as f32;

        for (i, &cell_index) in cells.iter().enumerate() {
            let x = i as f32 * cell_width;

            // Position the cell
            if let Some(cell_node) = tree.get_mut(cell_index) {
                cell_node.position = Some(LogicalPosition::new(x, row_y));
            }
        }

        debug_log(
            debug_messages,
            &format!("Table: Positioned {} cells in row", cells.len()),
        );

        Ok(())
    }
}

fn debug_log(debug_messages: &mut Option<Vec<LayoutDebugMessage>>, message: &str) {
    if let Some(messages) = debug_messages {
        messages.push(LayoutDebugMessage {
            message: message.into(),
            location: "table".into(),
        });
    }
}
