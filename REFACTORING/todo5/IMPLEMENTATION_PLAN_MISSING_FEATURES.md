# Implementation Plan: Missing Table Layout Features

## Current Status Analysis

### ✅ ALREADY IMPLEMENTED: Anonymous Node Generation (Partially)

**File:** `azul/layout/src/solver3/layout_tree.rs`

**What's Already Done:**
- ✅ `AnonymousBoxType` enum with all table types (TableWrapper, TableRowGroup, TableRow, TableCell)
- ✅ `create_anonymous_node()` helper function
- ✅ `process_table_children()` - wraps direct table-cell children in anonymous table-rows
- ✅ `process_table_row_group_children()` - delegates to table children processing
- ✅ `process_table_row_children()` - wraps non-cell children in anonymous cells
- ✅ `process_block_children()` - creates anonymous inline wrappers for mixed content
- ✅ List item marker generation

**What's Missing:**
- ⚠️ **Anonymous Table Wrapper**: When a table-cell or table-row appears outside a table context
- ⚠️ **Anonymous Row Group (tbody)**: Not currently generated (tables jump directly to rows)
- ⚠️ **Whitespace Handling**: The spec requires collapsing whitespace-only text nodes between table elements
- ⚠️ **Complex Nesting**: Missing parent wrappers (e.g., row-group → row → cell hierarchy)

**Why It's Not Complete:**

According to CSS 2.2 Section 17.2.1, the algorithm has 3 stages:

1. **Stage 1**: Remove irrelevant whitespace (only whitespace text nodes between table elements)
2. **Stage 2**: Generate missing child wrappers (if table-row has non-cell children → wrap in cell)
3. **Stage 3**: Generate missing parents (if cell appears without row → create row wrapper)

**Current Implementation:** Only does Stage 2 (child wrapping). Missing Stages 1 and 3.

---

## Feature 1: Complete Anonymous Node Generation

### Priority: HIGH (Foundation for proper table layout)

### Implementation Steps:

#### Step 1.1: Add Whitespace Detection
```rust
// In layout_tree.rs

/// Checks if a DOM node is whitespace-only (for table anonymous box generation)
fn is_whitespace_only_text(styled_dom: &StyledDom, node_id: NodeId) -> bool {
    let node_data = styled_dom.node_data.as_container().get(node_id).unwrap();
    if let NodeType::Text(text) = node_data.get_node_type() {
        // Check if the text contains only whitespace characters
        text.chars().all(|c| c.is_whitespace())
    } else {
        false
    }
}

/// Determines if a node should be skipped in table structure generation
/// (whitespace-only text nodes between table elements)
fn should_skip_for_table_structure(
    styled_dom: &StyledDom,
    node_id: NodeId,
    parent_display: LayoutDisplay,
) -> bool {
    // Only skip whitespace text nodes when parent is a table structural element
    matches!(
        parent_display,
        LayoutDisplay::Table
            | LayoutDisplay::TableRowGroup
            | LayoutDisplay::TableHeaderGroup
            | LayoutDisplay::TableFooterGroup
            | LayoutDisplay::TableRow
    ) && is_whitespace_only_text(styled_dom, node_id)
}
```

#### Step 1.2: Implement Stage 3 (Missing Parent Generation)
```rust
// Add to LayoutTreeBuilder

/// Wraps a node in necessary parent anonymous boxes to satisfy table structure
fn ensure_table_parent_wrapper(
    &mut self,
    styled_dom: &StyledDom,
    child_id: NodeId,
    parent_idx: usize,
    parent_display: LayoutDisplay,
) -> Result<usize> {
    let child_display = get_display_type(styled_dom, child_id);
    
    match (parent_display, child_display) {
        // Cell without row → create anonymous row
        (LayoutDisplay::Table | LayoutDisplay::TableRowGroup, LayoutDisplay::TableCell) => {
            let anon_row = self.create_anonymous_node(
                parent_idx,
                AnonymousBoxType::TableRow,
                FormattingContext::TableRow,
            );
            self.process_node(styled_dom, child_id, Some(anon_row))
        }
        
        // Row without row-group (going into table directly) → optionally create tbody
        // Note: This is debatable - CSS 2.2 doesn't strictly require it
        // Many browsers skip this optimization
        _ => {
            // Default: process normally
            self.process_node(styled_dom, child_id, Some(parent_idx))
        }
    }
}
```

#### Step 1.3: Update process_table_children to Handle Whitespace
```rust
fn process_table_children(
    &mut self,
    styled_dom: &StyledDom,
    parent_dom_id: NodeId,
    parent_idx: usize,
) -> Result<()> {
    let parent_display = get_display_type(styled_dom, parent_dom_id);
    let mut row_children = Vec::new();
    
    for child_id in parent_dom_id.az_children(&styled_dom.node_hierarchy.as_container()) {
        // Skip whitespace-only text nodes (Stage 1)
        if should_skip_for_table_structure(styled_dom, child_id, parent_display) {
            continue;
        }
        
        let child_display = get_display_type(styled_dom, child_id);
        
        if child_display == LayoutDisplay::TableCell {
            row_children.push(child_id);
        } else {
            // Flush accumulated cells into anonymous row
            if !row_children.is_empty() {
                let anon_row_idx = self.create_anonymous_node(
                    parent_idx,
                    AnonymousBoxType::TableRow,
                    FormattingContext::TableRow,
                );
                for cell_id in row_children.drain(..) {
                    self.process_node(styled_dom, cell_id, Some(anon_row_idx))?;
                }
            }
            
            // Process non-cell child (could be row, row-group, caption, etc.)
            self.ensure_table_parent_wrapper(styled_dom, child_id, parent_idx, parent_display)?;
        }
    }
    
    // Flush remaining cells
    if !row_children.is_empty() {
        let anon_row_idx = self.create_anonymous_node(
            parent_idx,
            AnonymousBoxType::TableRow,
            FormattingContext::TableRow,
        );
        for cell_id in row_children {
            self.process_node(styled_dom, cell_id, Some(anon_row_idx))?;
        }
    }
    
    Ok(())
}
```

#### Step 1.4: Add Tests
```rust
// In tests/table_layout.rs or tests/anonymous_nodes.rs

#[test]
fn test_anonymous_row_generation() {
    // <div display="table">
    //   <div display="table-cell">Cell 1</div>
    //   <div display="table-cell">Cell 2</div>
    // </div>
    // Should create: table → (anon row) → cell, cell
}

#[test]
fn test_anonymous_cell_generation() {
    // <div display="table-row">
    //   <div>Text</div>
    // </div>
    // Should create: row → (anon cell) → text
}

#[test]
fn test_whitespace_skipping() {
    // <div display="table">
    //   [whitespace]
    //   <div display="table-row">...</div>
    //   [whitespace]
    // </div>
    // Whitespace should be ignored
}
```

**Estimated Effort:** 3-4 hours
**Files to Modify:** `layout/src/solver3/layout_tree.rs`, `layout/tests/anonymous_nodes.rs`
**Risk Level:** LOW (builds on existing infrastructure)

---

## Feature 2: Caption Positioning (`caption-side`)

### Priority: MEDIUM (Visual feature, property already exists)

### Implementation Steps:

#### Step 2.1: Detect Caption in Table Layout
```rust
// In solver3/fc.rs, layout_table_fc function

pub fn layout_table_fc<T, Q>(
    /* ... existing params ... */
) -> LayoutResult {
    // ... existing table layout code ...
    
    // NEW: Handle caption positioning
    let caption_side = get_caption_side_property(node_index, node_data, cache);
    let mut caption_node = None;
    let mut caption_height = 0.0;
    
    // Find caption child (if any)
    for &child_idx in children.iter() {
        let child_display = get_display_type_from_layout_node(tree, child_idx);
        if child_display == LayoutDisplay::TableCaption {
            caption_node = Some(child_idx);
            break;
        }
    }
    
    // If caption exists, layout it first
    if let Some(caption_idx) = caption_node {
        // Layout caption with table's available width
        let caption_constraints = LayoutConstraints {
            available_size: LogicalSize {
                inline: available_width,
                block: None, // Caption height is auto
            },
            writing_mode: constraints.writing_mode,
            bfc_state: None,
            text_align: constraints.text_align,
        };
        
        let caption_result = calculate_layout_for_subtree(
            tree,
            caption_idx,
            caption_constraints,
            ctx,
        )?;
        
        caption_height = caption_result.content_size.block;
    }
    
    // ... existing table layout algorithm ...
    
    // Adjust table Y position based on caption
    let table_y = match caption_side {
        StyleCaptionSide::Top => caption_height,
        StyleCaptionSide::Bottom => 0.0,
    };
    
    // Position caption
    if let Some(caption_idx) = caption_node {
        let caption_y = match caption_side {
            StyleCaptionSide::Top => 0.0,
            StyleCaptionSide::Bottom => table_height,
        };
        
        tree.nodes[caption_idx].relative_position = Some(LogicalPosition {
            inline: 0.0,
            block: caption_y,
        });
    }
    
    // Final table wrapper size includes caption
    let total_height = table_height + caption_height;
    
    LayoutResult {
        content_size: LogicalSize {
            inline: table_width,
            block: total_height,
        },
        baseline: None, // Tables don't contribute to baseline
    }
}
```

#### Step 2.2: Add Helper Function
```rust
// In solver3/fc.rs

fn get_caption_side_property(
    node_index: usize,
    node_data: &NodeData,
    cache: &Box<CssPropertyCache>,
) -> StyleCaptionSide {
    cache
        .get_caption_side(node_data, node_index, node_data.get_state())
        .unwrap_or(StyleCaptionSide::Top) // Default is top
}
```

#### Step 2.3: Tests
```rust
#[test]
fn test_caption_top_positioning() {
    // Caption with caption-side: top should appear above table
}

#[test]
fn test_caption_bottom_positioning() {
    // Caption with caption-side: bottom should appear below table
}
```

**Estimated Effort:** 2-3 hours
**Files to Modify:** `layout/src/solver3/fc.rs`
**Risk Level:** LOW (straightforward positioning)

---

## Feature 3: Empty Cell Detection (`empty-cells`)

### Priority: LOW (Rendering optimization, only affects separated borders)

### Implementation Steps:

#### Step 3.1: Add Empty Cell Detection
```rust
// In solver3/fc.rs or new module

/// Checks if a table cell is empty (contains no visible content)
fn is_cell_empty(
    tree: &LayoutTree,
    cell_idx: usize,
) -> bool {
    let cell_node = &tree.nodes[cell_idx];
    
    // If cell has no children, it's empty
    if cell_node.children.is_empty() {
        return true;
    }
    
    // Check if all children are whitespace-only text
    for &child_idx in &cell_node.children {
        let child = &tree.nodes[child_idx];
        
        // If it has inline layout result, check if it's only whitespace
        if let Some(ref layout) = child.inline_layout_result {
            // Check if layout has any visible glyphs
            if !layout.words.is_empty() {
                return false; // Has visible content
            }
        } else {
            // Non-text content exists
            return false;
        }
    }
    
    true
}
```

#### Step 3.2: Use in Rendering (Future - when implementing rendering)
```rust
// This is a rendering concern, not layout
// Would be implemented in the rendering pipeline

fn should_render_cell_border(
    cell: &Cell,
    empty_cells: StyleEmptyCells,
    border_collapse: StyleBorderCollapse,
) -> bool {
    // empty-cells only applies to separated border model
    if border_collapse == StyleBorderCollapse::Collapse {
        return true;
    }
    
    // Check if cell is empty and empty-cells is hide
    if empty_cells == StyleEmptyCells::Hide && is_cell_empty(tree, cell_idx) {
        return false; // Don't render border/background
    }
    
    true
}
```

**Estimated Effort:** 1-2 hours (layout part), rendering part deferred
**Files to Modify:** `layout/src/solver3/fc.rs` (detection only)
**Risk Level:** VERY LOW (optional optimization)

---

## Feature 4: Layered Background Painting

### Priority: LOW (Rendering feature, not layout)

### Implementation: DEFERRED TO RENDERING PIPELINE

This feature is entirely about painting order and doesn't affect layout calculations. 
It should be implemented in the rendering pipeline (display list generation).

**Future Implementation Note:**
```rust
// In rendering code (not layout)

fn paint_table_backgrounds(table_node: &Node) {
    // Layer 1: Table background
    paint_background(table_node);
    
    // Layer 2: Column group backgrounds
    for colgroup in table_node.colgroups() {
        paint_background(colgroup);
    }
    
    // Layer 3: Column backgrounds
    for col in table_node.cols() {
        paint_background(col);
    }
    
    // Layer 4: Row group backgrounds
    for rowgroup in table_node.rowgroups() {
        paint_background(rowgroup);
    }
    
    // Layer 5: Row backgrounds
    for row in table_node.rows() {
        paint_background(row);
    }
    
    // Layer 6: Cell backgrounds (topmost)
    for cell in table_node.cells() {
        paint_background(cell);
    }
}
```

**Estimated Effort:** 4-6 hours (when implementing rendering)
**Files to Modify:** Rendering pipeline (future)
**Risk Level:** LOW (well-defined layering rules)

---

## Feature 5: `visibility: collapse` Optimization

### Priority: MEDIUM (Performance optimization for dynamic tables)

### Implementation Steps:

#### Step 5.1: Detect Collapsed Rows/Columns During Structure Analysis
```rust
// In solver3/fc.rs, during table structure analysis

/// Checks if a row or column is collapsed
fn is_collapsed(
    node_index: usize,
    node_data: &NodeData,
    cache: &Box<CssPropertyCache>,
) -> bool {
    cache
        .get_visibility(node_data, node_index, node_data.get_state())
        .map(|v| v.inner == StyleVisibility::Collapse)
        .unwrap_or(false)
}

// In analyze_table_structure
for row in 0..num_rows {
    let row_node_index = /* get row node */;
    if is_collapsed(row_node_index, node_data, cache) {
        // Mark this row as collapsed
        collapsed_rows.insert(row);
    }
}
```

#### Step 5.2: Skip Collapsed Rows in Height Calculation
```rust
// In calculate_row_heights

fn calculate_row_heights<T, Q>(
    ctx: &TableLayoutContext,
    tree: &LayoutTree,
    collapsed_rows: &HashSet<usize>,
    /* ... */
) -> Vec<f32> {
    let mut row_heights = vec![0.0; ctx.num_rows];
    
    for cell_info in &ctx.cells {
        // Skip cells in collapsed rows
        if collapsed_rows.contains(&cell_info.row) {
            continue;
        }
        
        // ... rest of height calculation ...
    }
    
    // Set collapsed rows to zero height
    for &row in collapsed_rows {
        row_heights[row] = 0.0;
    }
    
    row_heights
}
```

#### Step 5.3: Skip Collapsed Columns in Width Calculation
```rust
// Similar logic for column width calculation

fn calculate_column_widths_auto<T, Q>(
    ctx: &mut TableLayoutContext,
    tree: &LayoutTree,
    collapsed_columns: &HashSet<usize>,
    available_width: f32,
    /* ... */
) -> Result<()> {
    // Skip collapsed columns when measuring
    for col_idx in 0..ctx.columns.len() {
        if collapsed_columns.contains(&col_idx) {
            ctx.columns[col_idx].computed_width = Some(0.0);
            continue;
        }
        
        // ... rest of width calculation ...
    }
    
    Ok(())
}
```

#### Step 5.4: Handle Cell Clipping for Spans
```rust
// Cells that span into collapsed rows/columns should be clipped

fn calculate_cell_size(
    cell: &TableCellInfo,
    column_widths: &[f32],
    row_heights: &[f32],
    collapsed_rows: &HashSet<usize>,
    collapsed_columns: &HashSet<usize>,
) -> LogicalSize {
    let mut width = 0.0;
    let mut height = 0.0;
    
    // Sum widths, skipping collapsed columns
    for col in cell.column..(cell.column + cell.colspan) {
        if !collapsed_columns.contains(&col) {
            width += column_widths[col];
        }
    }
    
    // Sum heights, skipping collapsed rows
    for row in cell.row..(cell.row + cell.rowspan) {
        if !collapsed_rows.contains(&row) {
            height += row_heights[row];
        }
    }
    
    LogicalSize {
        inline: width,
        block: height,
    }
}
```

#### Step 5.5: Tests
```rust
#[test]
fn test_row_visibility_collapse() {
    // Row with visibility:collapse should take zero space
}

#[test]
fn test_column_visibility_collapse() {
    // Column with visibility:collapse should take zero space
}

#[test]
fn test_cell_spanning_collapsed_row() {
    // Cell spanning into collapsed row should be clipped
}
```

**Estimated Effort:** 4-5 hours
**Files to Modify:** `layout/src/solver3/fc.rs`
**Risk Level:** MEDIUM (affects layout algorithm, need careful testing)

---

## Implementation Order (Recommended):

1. **Feature 1: Complete Anonymous Node Generation** (HIGH priority, 3-4 hours)
   - Foundation for all other features
   - Fixes structural issues with malformed tables
   
2. **Feature 2: Caption Positioning** (MEDIUM priority, 2-3 hours)
   - Visual feature, straightforward implementation
   - Property already exists
   
3. **Feature 5: `visibility: collapse`** (MEDIUM priority, 4-5 hours)
   - Performance optimization
   - Affects layout algorithm
   
4. **Feature 3: Empty Cell Detection** (LOW priority, 1-2 hours)
   - Minor optimization
   - Layout part is simple, rendering deferred
   
5. **Feature 4: Layered Background Painting** (DEFERRED)
   - Rendering concern, not layout
   - Implement when building rendering pipeline

---

## Total Estimated Time:

- **Phase 1 (High Priority):** 3-4 hours (Anonymous nodes)
- **Phase 2 (Medium Priority):** 6-8 hours (Caption + visibility:collapse)
- **Phase 3 (Low Priority):** 1-2 hours (Empty cells - layout part)
- **Phase 4 (Deferred):** 4-6 hours (Layered backgrounds - rendering)

**Total Layout Work:** 10-14 hours
**Total with Rendering:** 14-20 hours

---

## Success Criteria:

### Feature 1 (Anonymous Nodes):
- ✅ All CSS 2.2 table structure tests pass
- ✅ Whitespace-only text nodes are ignored
- ✅ Missing parents are generated correctly
- ✅ Browser compatibility tests pass

### Feature 2 (Caption):
- ✅ Caption appears above table when caption-side: top
- ✅ Caption appears below table when caption-side: bottom
- ✅ Table height includes caption height

### Feature 3 (Empty Cells):
- ✅ Empty cells are correctly detected
- ✅ API exists for rendering to check empty cell status

### Feature 5 (visibility:collapse):
- ✅ Collapsed rows take zero height
- ✅ Collapsed columns take zero width
- ✅ Cells spanning collapsed areas are clipped correctly
- ✅ Layout performance is maintained

---

## Notes:

- **Anonymous Node Generation** is the most critical feature and should be implemented first
- **Caption Positioning** is a "quick win" - easy to implement, high visual impact
- **Empty Cell Detection** is optional and can be skipped if time is limited
- **Layered Background Painting** should wait for rendering pipeline work
- **visibility:collapse** is useful but not essential for basic table functionality
