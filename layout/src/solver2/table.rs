use azul_core::{dom::{NodeId, NodeType}, id_tree::{NodeDataContainerRef, NodeDataContainerRefMut}, styled_dom::StyledDom, ui_solver::{FormattingContext, IntrinsicSizes, PositionInfo, PositionInfoInner, PositionedRectangle}, window::{LogicalPosition, LogicalRect, LogicalSize}};
use azul_css::{LayoutDebugMessage, LayoutWidth, LayoutBorderCollapse};

use super::layout::{calculate_border, calculate_margin, calculate_padding, layout_node_recursive};

/// Represents a table cell with span information
#[derive(Debug, Clone, PartialEq, PartialOrd)]
struct TableCell {
    node_id: NodeId,
    row_span: usize,
    col_span: usize,
}

/// Represents a table's column and row structure
#[derive(Debug, Clone, PartialEq, PartialOrd)]
struct TableGrid {
    rows: usize,
    columns: usize,
    column_widths: Vec<f32>,
    row_heights: Vec<f32>,
    cells: Vec<Vec<Option<TableCell>>>,
    layout_type: TableLayoutType,
}

/// Table layout algorithm type
#[derive(Debug, Clone, PartialEq, PartialOrd)]
enum TableLayoutType {
    /// Fixed layout (widths determined by the first row)
    Fixed,
    /// Auto layout (widths determined by cell contents)
    Auto,
}

impl TableGrid {
    /// Builds a table grid from a table element
    fn from_table(table_id: NodeId, styled_dom: &StyledDom, available_width: f32) -> Self {
        let node_hierarchy = styled_dom.node_hierarchy.as_container();
        let css_property_cache = styled_dom.get_css_property_cache();
        let node_data = &styled_dom.node_data.as_container()[table_id];
        let styled_node_state = &styled_dom.styled_nodes.as_container()[table_id].state;
        
        // Determine table layout algorithm
        let layout_type = css_property_cache
            .get_table_layout(node_data, &table_id, styled_node_state)
            .and_then(|tl| tl.get_property().copied())
            .unwrap_or(TableLayoutType::Auto);
        
        let layout_type = match layout_type {
            TableLayoutType::Auto => TableLayoutType::Auto,
            TableLayoutType::Fixed => TableLayoutType::Fixed,
        };
        
        // First pass: count rows and columns, accounting for rowspan/colspan
        let mut rows_count = 0;
        let mut max_columns = 0;
        let mut row_spans: Vec<Vec<usize>> = Vec::new();
        
        // Process each row / row group
        for semantic_node_id in find_semantic_table_rows_or_row_groups(table_id, styled_dom) {
            match styled_dom.node_data.as_container()[semantic_node_id].get_node_type() {
                NodeType::Tr => {
                    process_row(semantic_node_id, &mut rows_count, &mut max_columns, &mut row_spans, styled_dom);
                },
                NodeType::THead | NodeType::TBody | NodeType::TFoot => {
                    for row_id in find_semantic_table_rows_or_row_groups(semantic_node_id, styled_dom) {
                        // Ensure we only process Tr elements within row groups
                        if let NodeType::Tr = styled_dom.node_data.as_container()[row_id].get_node_type() {
                            process_row(row_id, &mut rows_count, &mut max_columns, &mut row_spans, styled_dom);
                        }
                    }
                },
                _ => {} // Anonymous blocks are handled by find_semantic_table_rows_or_row_groups
            }
        }
        
        // Create the grid
        let mut grid = TableGrid {
            rows: rows_count,
            columns: max_columns,
            column_widths: vec![0.0; max_columns],
            row_heights: vec![0.0; rows_count],
            cells: vec![vec![None; max_columns]; rows_count],
            layout_type,
        };
        
        // Second pass: fill the grid with cell references and handle colspan/rowspan
        let mut current_row = 0;
        
        for semantic_node_id in find_semantic_table_rows_or_row_groups(table_id, styled_dom) {
            match styled_dom.node_data.as_container()[semantic_node_id].get_node_type() {
                NodeType::Tr => {
                    grid.add_row_cells(semantic_node_id, current_row, styled_dom);
                    current_row += 1;
                },
                NodeType::THead | NodeType::TBody | NodeType::TFoot => {
                    for row_id in find_semantic_table_rows_or_row_groups(semantic_node_id, styled_dom) {
                        // Ensure we only process Tr elements within row groups
                        if let NodeType::Tr = styled_dom.node_data.as_container()[row_id].get_node_type() {
                            grid.add_row_cells(row_id, current_row, styled_dom);
                            current_row += 1;
                        }
                    }
                },
                _ => {} // Anonymous blocks are handled by find_semantic_table_rows_or_row_groups
            }
        }
        
        // Calculate column widths based on layout type
        match grid.layout_type {
            TableLayoutType::Fixed => grid.calculate_fixed_column_widths(available_width, styled_dom),
            TableLayoutType::Auto => grid.calculate_auto_column_widths(styled_dom),
        }
        
        grid
    }
    
    /// Add cells from a row to the grid, handling colspan and rowspan
    fn add_row_cells(&mut self, row_id: NodeId, row_index: usize, styled_dom: &StyledDom) {
        let css_property_cache = styled_dom.get_css_property_cache();
        let mut col_index = 0;
        
        for cell_id_semantic in find_semantic_table_cells(row_id, styled_dom) {
            // Ensure the found node is actually a Td or Th.
            // This should be guaranteed by find_semantic_table_cells, but it's good practice to check.
            if matches!(
                styled_dom.node_data.as_container()[cell_id_semantic].get_node_type(),
                NodeType::Td | NodeType::Th
            ) {
                // Skip cells that are already occupied by previous rowspan
                while col_index < self.columns && self.cells[row_index][col_index].is_some() {
                    col_index += 1;
                }
                
                if col_index >= self.columns {
                    break; // No more space in this row
                }
                
                // Get colspan and rowspan attributes
                let node_data = &styled_dom.node_data.as_container()[cell_id_semantic];
                let styled_node_state = &styled_dom.styled_nodes.as_container()[cell_id_semantic].state;
                
                let col_span = css_property_cache
                    .get_colspan(node_data, &cell_id_semantic, styled_node_state)
                    .and_then(|cs| cs.get_property().copied())
                    .unwrap_or(1)
                    .max(1);
                
                let row_span = css_property_cache
                    .get_rowspan(node_data, &cell_id_semantic, styled_node_state)
                    .and_then(|rs| rs.get_property().copied())
                    .unwrap_or(1)
                    .max(1);
                
                // Limit spans to available space
                let col_span = col_span.min(self.columns - col_index);
                let row_span = row_span.min(self.rows - row_index);
                
                // Create the cell entry
                let cell = TableCell {
                    node_id: cell_id_semantic,
                    row_span,
                    col_span,
                };
                
                // Place the cell and mark spanned cells
                self.cells[row_index][col_index] = Some(cell);
                
                // Mark cells covered by rowspan/colspan as occupied
                for r in 0..row_span {
                    for c in 0..col_span {
                        if r == 0 && c == 0 {
                            continue; // Skip the primary cell
                        }
                        
                        if row_index + r < self.rows && col_index + c < self.columns {
                            // Mark as spanned (None indicates it's occupied by another cell's span)
                            self.cells[row_index + r][col_index + c] = None;
                        }
                    }
                }
                
                // Move to next available column
                col_index += col_span;
            }
        }
    }
    
    /// Calculate column widths for fixed layout tables
    fn calculate_fixed_column_widths(&mut self, available_width: f32, styled_dom: &StyledDom) {
        let css_property_cache = styled_dom.get_css_property_cache();
        
        // For fixed layout, try to use explicit column widths first
        let mut specified_widths = vec![None; self.columns];
        let mut specified_total = 0.0;
        let mut unspecified_count = self.columns;
        
        // Check for col elements with explicit widths
        for row in 0..self.rows {
            for col in 0..self.columns {
                if let Some(cell) = &self.cells[row][col] {
                    // Only consider cells in the first row
                    if row > 0 || cell.col_span != 1 {
                        continue;
                    }
                    
                    let node_data = &styled_dom.node_data.as_container()[cell.node_id];
                    let styled_node_state = &styled_dom.styled_nodes.as_container()[cell.node_id].state;
                    
                    // Check if cell has explicit width
                    if let Some(width_prop) = css_property_cache.get_width(node_data, &cell.node_id, styled_node_state) {
                        if let Some(width) = width_prop.get_property().map(|w| {
                            w.inner.to_pixels(available_width)
                        }) {
                            specified_widths[col] = Some(width);
                            specified_total += width;
                            unspecified_count -= 1;
                        }
                    }
                }
            }
            
            // Only process the first row with cells for fixed layout
            if unspecified_count < self.columns {
                break;
            }
        }
        
        // Distribute remaining width
        let remaining_width = (available_width - specified_total).max(0.0);
        let default_column_width = if unspecified_count > 0 {
            remaining_width / unspecified_count as f32
        } else {
            0.0
        };
        
        // Set final column widths
        for col in 0..self.columns {
            self.column_widths[col] = specified_widths[col].unwrap_or(default_column_width);
        }
    }
    
    /// Calculate column widths for auto layout tables
    fn calculate_auto_column_widths(&mut self, styled_dom: &StyledDom) {
        let css_property_cache = styled_dom.get_css_property_cache();
        
        // Two-pass algorithm: 
        // 1. Calculate min/max content width for each column
        // 2. Distribute available space
        
        // Initialize with min/max content widths
        let mut min_content_widths = vec![0.0_f32; self.columns];
        let mut max_content_widths = vec![0.0_f32; self.columns];
        
        // First pass: gather min/max content widths
        for row in 0..self.rows {
            for col in 0..self.columns {
                if let Some(cell) = &self.cells[row][col] {
                    if cell.col_span == 1 {
                        // Get intrinsic widths for single-column cells
                        let node_data = &styled_dom.node_data.as_container()[cell.node_id];
                        let styled_node_state = &styled_dom.styled_nodes.as_container()[cell.node_id].state;
                        
                        // Get intrinsic sizes if possible
                        if let Some(intrinsic_sizes) = styled_dom.get_intrinsic_sizes(cell.node_id) {
                            min_content_widths[col] = min_content_widths[col].max(intrinsic_sizes.min_content_width);
                            max_content_widths[col] = max_content_widths[col].max(intrinsic_sizes.max_content_width);
                        } else {
                            // Use explicit width if provided
                            if let Some(width_prop) = css_property_cache.get_width(node_data, &cell.node_id, styled_node_state) {
                                if let Some(width) = width_prop.get_property().and_then(|w| {
                                    w.inner.to_pixels_no_percent()
                                }) {
                                    min_content_widths[col] = min_content_widths[col].max(width);
                                    max_content_widths[col] = max_content_widths[col].max(width);
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // Handle cells with colspan > 1
        for row in 0..self.rows {
            for col in 0..self.columns {
                if let Some(cell) = &self.cells[row][col] {
                    if cell.col_span > 1 {
                        // Get intrinsic widths for multi-column cells
                        if let Some(intrinsic_sizes) = styled_dom.get_intrinsic_sizes(cell.node_id) {
                            let min_content = intrinsic_sizes.min_content_width;
                            let max_content = intrinsic_sizes.max_content_width;
                            
                            // Calculate how much width we already have in spanned columns
                            let spanned_min_width: f32 = (col..(col + cell.col_span))
                                .map(|c| min_content_widths[c])
                                .sum();
                            
                            let spanned_max_width: f32 = (col..(col + cell.col_span))
                                .map(|c| max_content_widths[c])
                                .sum();
                            
                            // If the cell requires more width, distribute it evenly
                            if min_content > spanned_min_width {
                                let extra_width = (min_content - spanned_min_width) / cell.col_span as f32;
                                for c in col..(col + cell.col_span) {
                                    min_content_widths[c] += extra_width;
                                }
                            }
                            
                            if max_content > spanned_max_width {
                                let extra_width = (max_content - spanned_max_width) / cell.col_span as f32;
                                for c in col..(col + cell.col_span) {
                                    max_content_widths[c] += extra_width;
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // Set column widths to their max content width
        // In a real implementation, you would consider available space and
        // potentially distribute excess width proportionally
        for col in 0..self.columns {
            self.column_widths[col] = max_content_widths[col];
        }
    }
}

/// Process a row to determine its contribution to the grid structure
fn process_row(
    row_id: NodeId, 
    rows_count: &mut usize, 
    max_columns: &mut usize,
    row_spans: &mut Vec<Vec<usize>>,
    styled_dom: &StyledDom
) {
    let css_property_cache = styled_dom.get_css_property_cache();
    
    // Add any missing row span vectors
    while row_spans.len() <= *rows_count {
        row_spans.push(Vec::new());
    }
    
    let mut col_index = 0;
    let mut spanned_columns = 0;
    
    // Account for active rowspans from previous rows
    for (i, span) in row_spans[*rows_count].iter().enumerate() {
        if i >= col_index {
            spanned_columns += span;
            col_index = i + span;
        }
    }
    
    // Count cells in this row
    for cell_id_semantic in find_semantic_table_cells(row_id, styled_dom) {
        // Ensure the found node is actually a Td or Th.
        if matches!(
            styled_dom.node_data.as_container()[cell_id_semantic].get_node_type(),
            NodeType::Td | NodeType::Th
        ) {
            // Get colspan and rowspan
            let node_data = &styled_dom.node_data.as_container()[cell_id_semantic];
            let styled_node_state = &styled_dom.styled_nodes.as_container()[cell_id_semantic].state;
            
            let col_span = css_property_cache
                .get_colspan(node_data, &cell_id_semantic, styled_node_state)
                .and_then(|cs| cs.get_property().copied())
                .unwrap_or(1)
                .max(1);
            
            let row_span = css_property_cache
                .get_rowspan(node_data, &cell_id_semantic, styled_node_state)
                .and_then(|rs| rs.get_property().copied())
                .unwrap_or(1)
                .max(1);
            
            // Skip columns occupied by previous rowspans
            while col_index < row_spans[*rows_count].len() && row_spans[*rows_count][col_index] > 0 {
                col_index += 1;
            }
            
            // Track this cell's rowspan for future rows
            if row_span > 1 {
                for r in 1..row_span {
                    let target_row = *rows_count + r;
                    
                    // Ensure we have entries for all affected rows
                    while row_spans.len() <= target_row {
                        row_spans.push(Vec::new());
                    }
                    
                    // Ensure the vector is long enough
                    while row_spans[target_row].len() <= col_index {
                        row_spans[target_row].push(0);
                    }
                    
                    // Mark columns as spanned
                    for c in 0..col_span {
                        if col_index + c < row_spans[target_row].len() {
                            row_spans[target_row][col_index + c] = 1;
                        } else {
                            row_spans[target_row].push(1);
                        }
                    }
                }
            }
            
            // Move to next position
            col_index += col_span;
        }
    }
    
    // Update max columns
    *max_columns = (*max_columns).max(col_index);
    
    // Move to next row
    *rows_count += 1;
}

/// Finds table cells (Td or Th), including those nested in anonymous blocks.
fn find_semantic_table_cells(
    row_node_id: NodeId,
    styled_dom: &StyledDom,
) -> Vec<NodeId> {
    let mut result = Vec::new();
    let node_hierarchy = styled_dom.node_hierarchy.as_container();
    let node_data_container = styled_dom.node_data.as_container();

    for child_id in row_node_id.az_children(&node_hierarchy) {
        let child_node_data = &node_data_container[child_id];
        match child_node_data.get_node_type() {
            NodeType::Td | NodeType::Th => {
                result.push(child_id);
            }
            _ => {
                if child_node_data.is_anonymous() {
                    result.extend(find_semantic_table_cells(child_id, styled_dom));
                }
            }
        }
    }
    result
}

/// Finds table rows or row groups, including those nested in anonymous blocks.
fn find_semantic_table_rows_or_row_groups(
    parent_node_id: NodeId,
    styled_dom: &StyledDom,
) -> Vec<NodeId> {
    let mut result = Vec::new();
    let node_hierarchy = styled_dom.node_hierarchy.as_container();
    let node_data_container = styled_dom.node_data.as_container();

    for child_id in parent_node_id.az_children(&node_hierarchy) {
        let child_node_data = &node_data_container[child_id];
        match child_node_data.get_node_type() {
            NodeType::Tr | NodeType::THead | NodeType::TBody | NodeType::TFoot => {
                result.push(child_id);
            }
            _ => {
                if child_node_data.is_anonymous() {
                    result.extend(find_semantic_table_rows_or_row_groups(child_id, styled_dom));
                }
            }
        }
    }
    result
}

/// Handles table layout within the overall layout process
pub fn layout_table(
    table_id: NodeId,
    positioned_rects: &mut NodeDataContainerRefMut<PositionedRectangle>,
    styled_dom: &StyledDom,
    formatting_contexts: &NodeDataContainerRef<FormattingContext>,
    intrinsic_sizes: &NodeDataContainerRef<IntrinsicSizes>,
    available_space: LogicalRect,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> LogicalSize {
    // Get table borders and padding
    let css_property_cache = styled_dom.get_css_property_cache();
    let node_data = &styled_dom.node_data.as_container()[table_id];
    let styled_node_state = &styled_dom.styled_nodes.as_container()[table_id].state;
    
    let padding = calculate_padding(table_id, styled_dom, available_space);
    let border = calculate_border(table_id, styled_dom, available_space);
    let margin = calculate_margin(table_id, styled_dom, available_space);
    
    // Calculate available area for table content
    let content_origin = LogicalPosition::new(
        available_space.origin.x + margin.left + border.left + padding.left,
        available_space.origin.y + margin.top + border.top + padding.top
    );
    
    let content_width = available_space.size.width - 
        margin.left - margin.right - 
        border.left - border.right - 
        padding.left - padding.right;
    
    let content_height = available_space.size.height - 
        margin.top - margin.bottom - 
        border.top - border.bottom - 
        padding.top - padding.bottom;
    
    let content_space = LogicalRect::new(
        content_origin,
        LogicalSize::new(content_width, content_height)
    );
    
    // Get border collapse mode
    let border_collapse = css_property_cache
        .get_border_collapse(node_data, &table_id, styled_node_state)
        .and_then(|bc| bc.get_property().copied())
        .unwrap_or(LayoutBorderCollapse::Separate);
    
    // Calculate table caption height if present
    let mut caption_height = 0.0;
    let node_hierarchy = styled_dom.node_hierarchy.as_container();
    
    for child_id in table_id.az_children(&node_hierarchy) {
        if let NodeType::Caption = styled_dom.node_data.as_container()[child_id].get_node_type() {
            // Layout the caption
            let caption_space = LogicalRect::new(
                content_origin,
                LogicalSize::new(content_width, content_height)
            );
            
            let caption_size = layout_node_recursive(
                child_id,
                positioned_rects,
                styled_dom,
                formatting_contexts,
                intrinsic_sizes,
                caption_space,
                debug_messages,
            );
            
            caption_height = caption_size.height;
            break;
        }
    }
    
    // Adjust available height for the table content
    let table_content_space = LogicalRect::new(
        LogicalPosition::new(content_origin.x, content_origin.y + caption_height),
        LogicalSize::new(content_width, content_height - caption_height)
    );
    
    // Build the table grid
    let grid = TableGrid::from_table(table_id, styled_dom, content_width);
    
    // Calculate table spacing
    let (h_spacing, v_spacing) = if border_collapse == LayoutBorderCollapse::Collapse {
        (0.0, 0.0) // In collapsed mode, borders overlap
    } else {
        // Get border spacing values
        let h_space = css_property_cache
            .get_border_spacing_horizontal(node_data, &table_id, styled_node_state)
            .and_then(|bs| bs.get_property().copied())
            .unwrap_or(0.0);
        
        let v_space = css_property_cache
            .get_border_spacing_vertical(node_data, &table_id, styled_node_state)
            .and_then(|bs| bs.get_property().copied())
            .unwrap_or(0.0);
        
        (h_space, v_space)
    };
    
    // Layout each cell and determine row heights
    let mut row_heights = vec![0.0; grid.rows];
    let mut y_position = table_content_space.origin.y;
    
    // First pass: layout cells and determine row heights
    for row in 0..grid.rows {
        let mut max_row_height = 0.0_f32;
        let mut x_position = table_content_space.origin.x;
        
        for col in 0..grid.columns {
            if let Some(cell) = &grid.cells[row][col] {
                // Skip cells that are spanning from previous rows or columns
                if cell.node_id == NodeId::ZERO {
                    continue;
                }
                
                // Calculate cell width (sum width of all spanned columns)
                let cell_width = (col..(col + cell.col_span))
                    .map(|c| grid.column_widths[c])
                    .sum::<f32>() + h_spacing * (cell.col_span as f32 - 1.0);
                
                // Create space for this cell
                let cell_space = LogicalRect::new(
                    LogicalPosition::new(x_position, y_position),
                    LogicalSize::new(cell_width, 0.0) // Height will be determined by content
                );
                
                // Layout the cell
                let cell_size = layout_node_recursive(
                    cell.node_id,
                    positioned_rects,
                    styled_dom,
                    formatting_contexts,
                    intrinsic_sizes,
                    cell_space,
                    debug_messages,
                );
                
                // Row height should accommodate the tallest cell
                max_row_height = max_row_height.max(cell_size.height / cell.row_span as f32);
            }
            
            // Move to next column
            x_position += grid.column_widths[col] + h_spacing;
        }
        
        row_heights[row] = max_row_height;
        y_position += max_row_height + v_spacing;
    }
    
    // Second pass: position cells according to final dimensions
    y_position = table_content_space.origin.y;
    
    for row in 0..grid.rows {
        let mut x_position = table_content_space.origin.x;
        
        // Calculate row height including any rowspans
        let row_height = row_heights[row];
        
        for col in 0..grid.columns {
            if let Some(cell) = &grid.cells[row][col] {
                // Skip placeholder cells
                if cell.node_id == NodeId::ZERO {
                    continue;
                }
                
                // Calculate cell dimensions
                let cell_width = (col..(col + cell.col_span))
                    .map(|c| grid.column_widths[c])
                    .sum::<f32>() + h_spacing * (cell.col_span as f32 - 1.0);
                
                let cell_height = (row..(row + cell.row_span))
                    .map(|r| row_heights[r])
                    .sum::<f32>() + v_spacing * (cell.row_span as f32 - 1.0);
                
                // Update the cell's position and size
                let mut cell_rect = positioned_rects[cell.node_id].clone();
                cell_rect.position = PositionInfo::Static(PositionInfoInner {
                    x_offset: 0.0,
                    y_offset: 0.0,
                    static_x_offset: x_position,
                    static_y_offset: y_position,
                });
                cell_rect.size = LogicalSize::new(cell_width, cell_height);
                positioned_rects[cell.node_id] = cell_rect;
            }
            
            // Move to next column
            x_position += grid.column_widths[col] + h_spacing;
        }
        
        // Move to next row
        y_position += row_height + v_spacing;
    }
    
    // Calculate final table dimensions
    let table_width = grid.column_widths.iter().sum::<f32>() + 
        h_spacing * (grid.columns as f32 + 1.0);
    
    let table_height = caption_height + 
        row_heights.iter().sum::<f32>() + 
        v_spacing * (grid.rows as f32 + 1.0);
    
    // Return the total size including borders and padding
    LogicalSize::new(
        table_width + padding.left + padding.right + border.left + border.right,
        table_height + padding.top + padding.bottom + border.top + border.bottom
    )
}