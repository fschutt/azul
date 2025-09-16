//! solver3/fc/table.rs
//!
//! Implements the table formatting context for the solver3 layout engine.
//! This module handles the layout of elements with `display: table` and its related
//! values, following the principles of the W3C HTML 4.01 Specification on Tables.
//! See: https://www.w3.org/TR/html4/struct/tables.html

use std::cmp::max;
use azul_core::{
    app_resources::RendererResources, dom::{NodeId, NodeType}, id_tree::NodeDataContainer, styled_dom::{StyledDom, StyledNode}, ui_solver::FormattingContext, window::{LogicalPosition, LogicalRect, LogicalSize}
};
use azul_css::{
    CssPropertyValue, LayoutBorderCollapse, LayoutBox, LayoutDebugMessage, LayoutMargin, TableLayoutAlgorithm
};

use crate::solver3::{
    fc::layout_formatting_context,
    layout_tree::{LayoutNode, LayoutTree},
    Result,
};

// --- Core Data Structures ---

/// Represents a single cell within the table grid.
#[derive(Debug, Clone)]
struct TableCell {
    /// The index of the LayoutNode corresponding to this cell (a TD or TH).
    node_index: usize,
    /// The number of rows this cell spans.
    rowspan: usize,
    /// The number of columns this cell spans.
    colspan: usize,
    /// The intrinsic min-content width of the cell's contents. Used in auto layout.
    min_content_width: f32,
    /// The intrinsic max-content width of the cell's contents. Used in auto layout.
    max_content_width: f32,
}

/// An intermediate representation of the table's structure, accounting for spans.
/// This grid is the foundation for all sizing and positioning calculations.
#[derive(Debug)]
struct TableGrid {
    /// The cells of the table, organized into a 2D grid.
    /// `None` indicates a slot that is occupied by another cell's rowspan or colspan.
    cells: Vec<Vec<Option<TableCell>>>,
    /// The final calculated width for each column.
    column_widths: Vec<f32>,
    /// The final calculated height for each row.
    row_heights: Vec<f32>,
    /// The number of rows in the grid.
    num_rows: usize,
    /// The number of columns in the grid.
    num_columns: usize,
    /// The table layout algorithm to use (`fixed` or `auto`).
    layout_algorithm: TableLayoutAlgorithm,
    /// Whether the borders are collapsed or separated.
    border_collapse: bool,
    /// Horizontal spacing between cells (for border-separate model).
    h_spacing: f32,
    /// Vertical spacing between cells (for border-separate model).
    v_spacing: f32,
}

/// Main entry point for laying out a `display: table` element.
///
/// This function is called by the formatting context dispatcher when it encounters a table.
/// It orchestrates the entire multi-pass table layout algorithm.
pub fn layout_table(
    tree: &mut LayoutTree,
    styled_dom: &StyledDom,
    node_index: usize,
    constraints: LogicalRect,
    _debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<LogicalSize> {
    let table_node = tree.get(node_index).and_then(|n| n.dom_node_id).map(|id| &styled_dom.nodes[id]);

    // Phase 0: Extract table-wide properties from the styled DOM.
    let layout_algorithm = get_table_layout_algorithm(table_node);
    let (border_collapse, h_spacing, v_spacing) = get_border_model(table_node);

    // Phase 1: Grid Construction
    // This phase builds the fundamental grid structure from the layout tree,
    // resolving row and column spans.
    // W3C Spec § 11.2.4.3: "Calculating the number of columns in a table"
    // The grid construction implicitly determines the number of rows and columns.
    let mut grid = build_grid(
        tree,
        styled_dom,
        node_index,
        layout_algorithm,
        border_collapse,
        h_spacing,
        v_spacing,
    )?;

    // Phase 2: Column Width Calculation
    // This is the most complex phase, with different logic for 'fixed' and 'auto' layouts.
    // W3C Spec § 11.2.4.4: "Calculating the width of columns"
    match grid.layout_algorithm {
        TableLayoutAlgorithm::Fixed => {
            calculate_fixed_column_widths(&mut grid, tree, styled_dom, constraints.size.width)?;
        }
        TableLayoutAlgorithm::Auto => {
            calculate_auto_column_widths(&mut grid, tree, styled_dom, constraints.size.width)?;
        }
    }

    // Phase 3: Row Height Calculation and Cell Content Layout
    // With column widths determined, we can now lay out the contents of each cell to find the
    // necessary height for each row.
    calculate_row_heights(&mut grid, tree, styled_dom, constraints.size)?;

    // Phase 4: Final Positioning
    // With all dimensions known, perform the final pass to set the absolute position
    // of each cell within the table's formatting context.
    position_cells(&grid, tree, constraints.origin)?;

    // Finally, calculate the total size of the table including spacing and borders.
    let total_width = grid.column_widths.iter().sum::<f32>()
        + grid.h_spacing * (grid.num_columns.saturating_sub(1) as f32);
    let total_height = grid.row_heights.iter().sum::<f32>()
        + grid.v_spacing * (grid.num_rows.saturating_sub(1) as f32);

    Ok(LogicalSize::new(total_width, total_height))
}

// --- Phase 1: Grid Construction ---

/// Builds the `TableGrid` by traversing the layout tree and placing cells.
fn build_grid(
    tree: &LayoutTree,
    styled_dom: &StyledDom,
    table_node_index: usize,
    layout_algorithm: TableLayoutAlgorithm,
    border_collapse: bool,
    h_spacing: f32,
    v_spacing: f32,
) -> Result<TableGrid> {
    let mut cells_grid: Vec<Vec<Option<TableCell>>> = Vec::new();
    let mut max_cols = 0;
    let mut current_row = 0;

    let table_layout_node = tree.get(table_node_index).unwrap();
    // Traverse row groups (thead, tbody, tfoot) and anonymous row groups.
    for row_group_idx in &table_layout_node.children {
        let row_group_node = tree.get(*row_group_idx).unwrap();
        // Traverse rows (tr) within the group.
        for row_idx in &row_group_node.children {
            if cells_grid.len() <= current_row {
                cells_grid.resize_with(current_row + 1, || Vec::new());
            }

            let mut current_col = 0;
            let row_node = tree.get(*row_idx).unwrap();

            // Traverse cells (td, th) within the row.
            for cell_idx in &row_node.children {
                // Find the next available slot, skipping slots occupied by previous rowspans.
                loop {
                    if cells_grid[current_row].len() > current_col && cells_grid[current_row][current_col].is_some() {
                        current_col += 1;
                    } else {
                        break;
                    }
                }

                let cell_layout_node = tree.get(*cell_idx).unwrap();
                let cell_styled_node = cell_layout_node.dom_node_id.map(|id| &styled_dom.styled_nodes.as_container()[id]);
                let rowspan = get_span_property(cell_styled_node, |s| s.rowspan).unwrap_or(1);
                let colspan = get_span_property(cell_styled_node, |s| s.colspan).unwrap_or(1);

                let cell = TableCell {
                    node_index: *cell_idx,
                    rowspan,
                    colspan,
                    min_content_width: 0.0,
                    max_content_width: 0.0,
                };
                
                // W3C Spec § 11.2.6.1: "Cells that span several rows or columns"
                // Mark all covered cells. The primary cell is at (current_row, current_col).
                // Other cells covered by its span will be marked to be skipped later.
                for r_offset in 0..rowspan {
                    let target_row = current_row + r_offset;
                    if cells_grid.len() <= target_row {
                        cells_grid.resize_with(target_row + 1, || Vec::new());
                    }
                    for c_offset in 0..colspan {
                        let target_col = current_col + c_offset;
                        if cells_grid[target_row].len() <= target_col {
                            cells_grid[target_row].resize_with(target_col + 1, || None);
                        }
                        // Only place the actual cell struct in the top-left corner of its span.
                        if r_offset == 0 && c_offset == 0 {
                            cells_grid[target_row][target_col] = Some(cell.clone());
                        } else {
                             // Mark other slots as occupied by a span.
                             cells_grid[target_row][target_col] = Some(TableCell { node_index: usize::MAX, ..cell.clone()}); // Placeholder
                        }
                    }
                }

                max_cols = max(max_cols, current_col + colspan);
                current_col += colspan;
            }
            current_row += 1;
        }
    }

    // Normalize grid width
    for row_vec in &mut cells_grid {
        row_vec.resize_with(max_cols, || None);
    }
    
    Ok(TableGrid {
        cells: cells_grid,
        column_widths: vec![0.0; max_cols],
        row_heights: vec![0.0; current_row],
        num_rows: current_row,
        num_columns: max_cols,
        layout_algorithm,
        border_collapse,
        h_spacing,
        v_spacing,
    })
}

// --- Phase 2: Column Width Calculation ---

/// Calculates column widths for `table-layout: fixed`.
fn calculate_fixed_column_widths(
    grid: &mut TableGrid,
    _tree: &LayoutTree,
    _styled_dom: &StyledDom,
    _table_width: f32,
) -> Result<()> {
    // W3C Spec § 11.2.4.4: Describes fixed, percentage, and proportional widths.
    // The algorithm is simpler because it does not depend on the content of most cells.
    // 1. Use widths from <col> elements.
    // 2. If not specified, use widths from cells in the first row.
    // 3. Distribute remaining space.
    // (Stubbed for brevity)
    if grid.num_columns > 0 {
        let equal_width = _table_width / grid.num_columns as f32;
        grid.column_widths.fill(equal_width);
    }
    Ok(())
}

/// Calculates column widths for `table-layout: auto`.
fn calculate_auto_column_widths(
    grid: &mut TableGrid,
    tree: &mut LayoutTree,
    styled_dom: &StyledDom,
    table_width: f32,
) -> Result<()> {
    // 1. Measure Intrinsic Widths of all cells.
    for r in 0..grid.num_rows {
        for c in 0..grid.num_columns {
            if let Some(mut cell) = grid.cells[r][c].clone() {
                if cell.node_index == usize::MAX { continue; } // Skip placeholder for spanned cells
                
                // For intrinsic sizing, we provide infinite available space to measure the
                // content's "natural" min and max widths.
                let unconstrained = LogicalSize::new(f32::INFINITY, f32::INFINITY);
                let (min_w, max_w) = crate::solver3::sizing::get_intrinsic_widths(
                    tree, styled_dom, cell.node_index, unconstrained,
                )?;
                cell.min_content_width = min_w;
                cell.max_content_width = max_w;
                grid.cells[r][c] = Some(cell);
            }
        }
    }

    // 2. Calculate Column Intrinsic Widths from Cell Widths.
    let mut min_col_widths = vec![0.0; grid.num_columns];
    let mut max_col_widths = vec![0.0; grid.num_columns];

    for r in 0..grid.num_rows {
        for c in 0..grid.num_columns {
            if let Some(cell) = &grid.cells[r][c] {
                if cell.node_index == usize::MAX { continue; }

                if cell.colspan == 1 {
                    min_col_widths[c] = (min_col_widths[c] as f64).max(cell.min_content_width as f64);
                    max_col_widths[c] = (max_col_widths[c] as f64).max(cell.max_content_width as f64);
                }
            }
        }
    }
    // (A full implementation would need another pass to distribute widths of colspan cells)

    // 3. Distribute Table Width.
    // This is a simplified distribution algorithm. The actual CSS spec is more complex.
    let total_max_width: f64 = max_col_widths.iter().sum();
    let table_width = table_width as f64;
    if total_max_width < table_width {
        // Distribute extra space proportionally
        let extra_space = table_width - total_max_width;
        for i in 0..grid.num_columns {
            let proportion = if total_max_width > 0.0 { max_col_widths[i] / total_max_width } else { 1.0 / grid.num_columns as f64 };
            grid.column_widths[i] = (max_col_widths[i] + extra_space * proportion) as f32;
        }
    } else {
        // Not enough space, use max-content widths (will overflow)
        grid.column_widths = max_col_widths;
    }

    Ok(())
}

// --- Phase 3: Row Height Calculation ---

/// Lays out cell content to determine the height of each row.
fn calculate_row_heights(
    grid: &mut TableGrid,
    tree: &mut LayoutTree,
    resources: &mut RendererResources,
    styled_dom: &StyledDom,
    _table_size: LogicalSize,
) -> Result<()> {
    let mut cell_heights = vec![vec![0.0; grid.num_columns]; grid.num_rows];

    // First, lay out every cell to find its content height.
    for r in 0..grid.num_rows {
        for c in 0..grid.num_columns {
            if let Some(cell) = &grid.cells[r][c] {
                if cell.node_index == usize::MAX { continue; }

                // Determine the width available to this cell's content.
                let cell_content_width: f32 = (c..c + cell.colspan)
                    .map(|i| grid.column_widths[i])
                    .sum::<f32>() + grid.h_spacing * (cell.colspan.saturating_sub(1) as f32);

                let cell_constraints = LogicalRect::new(
                    LogicalPosition::new(0.0, 0.0),
                    LogicalSize::new(cell_content_width, f32::INFINITY),
                );
                
                // Recursively lay out the cell's children.
                let cell_content_size = layout_formatting_context(
                    tree,
                    cell.node_index,
                    cell_constraints,
                    styled_dom,
                    resources,
                    &mut None,
                )?;
                cell_heights[r][c] = cell_content_size.height;
            }
        }
    }

    // Now, determine row heights from cell heights, considering rowspans.
    for r in 0..grid.num_rows {
        let mut max_height = 0.0;
        for c in 0..grid.num_columns {
            if let Some(cell) = &grid.cells[r][c] {
                 if cell.node_index == usize::MAX { continue; }
                 if cell.rowspan == 1 {
                     max_height = max_height.max(cell_heights[r][c]);
                 }
            }
        }
        grid.row_heights[r] = max_height;
    }
    // (A full implementation would need to handle distributing height for rowspan > 1)

    Ok(())
}

// --- Phase 4: Positioning ---

/// Sets the final `position` and `used_size` for each cell's `LayoutNode`.
fn position_cells(grid: &TableGrid, tree: &mut LayoutTree, table_origin: LogicalPosition) -> Result<()> {
    let mut current_y = table_origin.y;
    for r in 0..grid.num_rows {
        let mut current_x = table_origin.x;
        for c in 0..grid.num_columns {
            if let Some(cell) = &grid.cells[r][c] {
                if cell.node_index != usize::MAX {
                    let cell_width: f32 = (c..c + cell.colspan)
                        .map(|i| grid.column_widths[i])
                        .sum::<f32>() + grid.h_spacing * (cell.colspan.saturating_sub(1) as f32);
                    let cell_height: f32 = (r..r + cell.rowspan)
                        .map(|i| grid.row_heights[i])
                        .sum::<f32>() + grid.v_spacing * (cell.rowspan.saturating_sub(1) as f32);

                    let node = tree.get_mut(cell.node_index).unwrap();
                    node.position = Some(LogicalPosition::new(current_x, current_y));
                    node.used_size = Some(LogicalSize::new(cell_width, cell_height));
                    
                    // The children of the cell were already laid out relative to (0,0).
                    // Now, we need to offset their positions by the cell's final absolute position.
                    offset_child_positions(tree, cell.node_index, node.position.unwrap());
                }
            }
            if c < grid.num_columns {
                current_x += grid.column_widths[c] + grid.h_spacing;
            }
        }
        if r < grid.num_rows {
            current_y += grid.row_heights[r] + grid.v_spacing;
        }
    }
    Ok(())
}

fn offset_child_positions(tree: &mut LayoutTree, parent_index: usize, offset: LogicalPosition) {
    let parent = tree.get(parent_index).unwrap().clone(); // Clone to satisfy borrow checker
    for child_index in &parent.children {
        if let Some(child_node) = tree.get_mut(*child_index) {
            if let Some(pos) = &mut child_node.position {
                pos.x += offset.x;
                pos.y += offset.y;
            }
            offset_child_positions(tree, *child_index, offset);
        }
    }
}

// --- Helper Functions to read CSS properties ---

fn get_span_property<F>(node: Option<&StyledNode>, accessor: F) -> Option<usize>
where F: Fn(&CssPropertyValue) -> Option<usize>, {
    node.and_then(|n| n.css_properties.get(&accessor))
        .and_then(|prop| accessor(prop))
}

fn get_table_layout_algorithm(table_node: Option<&StyledNode>) -> TableLayoutAlgorithm {
    table_node
        .and_then(|n| n.css_properties.get(&|p| p.table_layout))
        .and_then(|p| p.table_layout)
        .unwrap_or(TableLayoutAlgorithm::Auto)
}

fn get_border_model(table_node: Option<&StyledNode>) -> (bool, f32, f32) {
    let border_collapse = table_node
        .and_then(|n| n.css_properties.get(&|p| p.border_collapse))
        .and_then(|p| p.border_collapse)
        .unwrap_or(LayoutBorderCollapse::Separate);

    if border_collapse == LayoutBorderCollapse::Collapse {
        (true, 0.0, 0.0)
    } else {
        let h_spacing = table_node
            .and_then(|n| n.css_properties.get(&|p| p.border_spacing_horizontal))
            .and_then(|p| p.border_spacing_horizontal)
            .unwrap_or(LayoutBox::Value(2.0)); // Default border-spacing is 2px
        let v_spacing = table_node
            .and_then(|n| n.css_properties.get(&|p| p.border_spacing_vertical))
            .and_then(|p| p.border_spacing_vertical)
            .unwrap_or(LayoutBox::Value(2.0));
        (false, h_spacing.get_value(), v_spacing.get_value())
    }
}