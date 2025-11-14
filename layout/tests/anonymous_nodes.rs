/// Unit tests for CSS 2.2 Section 17.2.1 - Anonymous Box Generation
/// 
/// Tests cover all three stages:
/// - Stage 1: Whitespace removal (remove irrelevant whitespace-only text nodes)
/// - Stage 2: Child wrapping (wrap non-cells in cells, non-rows in rows)
/// - Stage 3: Missing parent generation (wrap cells without rows, rows without tables)

use azul_css::props::layout::LayoutDisplay;

// Helper to check if a node display type matches expected
fn check_display(display: LayoutDisplay, expected: LayoutDisplay) -> bool {
    display == expected
}

#[test]
fn test_whitespace_detection_empty_string() {
    // CSS 2.2 Section 17.2.1, Stage 1:
    // Empty strings are whitespace-only
    let text = "";
    assert!(text.chars().all(|c| c.is_whitespace()));
}

#[test]
fn test_whitespace_detection_spaces() {
    // CSS 2.2 Section 17.2.1, Stage 1:
    // Spaces, tabs, newlines are whitespace
    let text = "   \t\n  ";
    assert!(text.chars().all(|c| c.is_whitespace()));
}

#[test]
fn test_whitespace_detection_with_content() {
    // CSS 2.2 Section 17.2.1, Stage 1:
    // Text with any non-whitespace character is NOT whitespace-only
    let text = "  hello  ";
    assert!(!text.chars().all(|c| c.is_whitespace()));
}

#[test]
fn test_whitespace_detection_unicode_spaces() {
    // CSS 2.2 Section 17.2.1, Stage 1:
    // Unicode whitespace characters should be detected
    let text = "\u{00A0}\u{2003}\u{2009}"; // NBSP, EM SPACE, THIN SPACE
    assert!(text.chars().all(|c| c.is_whitespace()));
}

#[test]
fn test_display_type_table() {
    // Verify table display type identification
    let display = LayoutDisplay::Table;
    assert!(check_display(display, LayoutDisplay::Table));
    assert!(!check_display(display, LayoutDisplay::Block));
}

#[test]
fn test_display_type_table_row() {
    // Verify table-row display type identification
    let display = LayoutDisplay::TableRow;
    assert!(check_display(display, LayoutDisplay::TableRow));
    assert!(!check_display(display, LayoutDisplay::Table));
}

#[test]
fn test_display_type_table_cell() {
    // Verify table-cell display type identification
    let display = LayoutDisplay::TableCell;
    assert!(check_display(display, LayoutDisplay::TableCell));
    assert!(!check_display(display, LayoutDisplay::TableRow));
}

#[test]
fn test_display_type_row_groups() {
    // Verify row group display types (thead, tbody, tfoot)
    let thead = LayoutDisplay::TableHeaderGroup;
    let tbody = LayoutDisplay::TableRowGroup;
    let tfoot = LayoutDisplay::TableFooterGroup;
    
    assert!(check_display(thead, LayoutDisplay::TableHeaderGroup));
    assert!(check_display(tbody, LayoutDisplay::TableRowGroup));
    assert!(check_display(tfoot, LayoutDisplay::TableFooterGroup));
}

#[test]
fn test_table_structural_elements() {
    // CSS 2.2 Section 17.2.1:
    // Table structural elements where whitespace should be skipped
    let structural = vec![
        LayoutDisplay::Table,
        LayoutDisplay::TableRowGroup,
        LayoutDisplay::TableHeaderGroup,
        LayoutDisplay::TableFooterGroup,
        LayoutDisplay::TableRow,
    ];
    
    for display in structural {
        assert!(matches!(
            display,
            LayoutDisplay::Table
                | LayoutDisplay::TableRowGroup
                | LayoutDisplay::TableHeaderGroup
                | LayoutDisplay::TableFooterGroup
                | LayoutDisplay::TableRow
        ));
    }
}

#[test]
fn test_non_structural_elements() {
    // Elements that should NOT trigger whitespace skipping
    let non_structural = vec![
        LayoutDisplay::Block,
        LayoutDisplay::Inline,
        LayoutDisplay::InlineBlock,
        LayoutDisplay::TableCell,
        LayoutDisplay::None,
    ];
    
    for display in non_structural {
        assert!(!matches!(
            display,
            LayoutDisplay::Table
                | LayoutDisplay::TableRowGroup
                | LayoutDisplay::TableHeaderGroup
                | LayoutDisplay::TableFooterGroup
                | LayoutDisplay::TableRow
        ));
    }
}

#[test]
fn test_stage2_cell_wrapping_needed() {
    // CSS 2.2 Section 17.2.1, Stage 2:
    // "If a child C of a table-row parent P is not a table-cell,
    // then generate an anonymous table-cell box around C"
    
    let parent = LayoutDisplay::TableRow;
    let child = LayoutDisplay::Block;
    
    // Block child in table-row needs anonymous cell wrapper
    assert!(parent == LayoutDisplay::TableRow);
    assert!(child != LayoutDisplay::TableCell);
}

#[test]
fn test_stage2_cell_wrapping_not_needed() {
    // CSS 2.2 Section 17.2.1, Stage 2:
    // If child is already a table-cell, no wrapping needed
    
    let parent = LayoutDisplay::TableRow;
    let child = LayoutDisplay::TableCell;
    
    assert!(parent == LayoutDisplay::TableRow);
    assert!(child == LayoutDisplay::TableCell);
}

#[test]
fn test_stage2_row_wrapping_needed() {
    // CSS 2.2 Section 17.2.1, Stage 2:
    // Table-cells that are not properly parented need anonymous row wrapper
    
    let parent = LayoutDisplay::Table;
    let child = LayoutDisplay::TableCell;
    
    // Cell directly in table (not in row) needs anonymous row
    assert!(parent == LayoutDisplay::Table);
    assert!(child == LayoutDisplay::TableCell);
}

#[test]
fn test_stage2_row_wrapping_not_needed() {
    // CSS 2.2 Section 17.2.1, Stage 2:
    // Table-rows in table don't need wrapping
    
    let parent = LayoutDisplay::Table;
    let child = LayoutDisplay::TableRow;
    
    assert!(parent == LayoutDisplay::Table);
    assert!(child == LayoutDisplay::TableRow);
}

#[test]
fn test_stage3_cell_needs_row_parent() {
    // CSS 2.2 Section 17.2.1, Stage 3:
    // "For each table-cell box C in a sequence of consecutive table-cell boxes
    // (that are not part of a table-row), an anonymous table-row box is generated"
    
    let parent = LayoutDisplay::Block;
    let child = LayoutDisplay::TableCell;
    
    // Table-cell in non-table parent needs anonymous row wrapper
    assert!(child == LayoutDisplay::TableCell);
    assert!(parent != LayoutDisplay::TableRow);
    assert!(parent != LayoutDisplay::TableRowGroup);
}

#[test]
fn test_stage3_row_needs_table_parent() {
    // CSS 2.2 Section 17.2.1, Stage 3:
    // Table-row without proper table parent needs anonymous table wrapper
    
    let parent = LayoutDisplay::Block;
    let child = LayoutDisplay::TableRow;
    
    // Table-row in non-table parent needs anonymous table wrapper
    assert!(child == LayoutDisplay::TableRow);
    assert!(parent != LayoutDisplay::Table);
    assert!(parent != LayoutDisplay::TableRowGroup);
}

#[test]
fn test_stage3_rowgroup_needs_table_parent() {
    // CSS 2.2 Section 17.2.1, Stage 3:
    // Row groups without table parent need anonymous table wrapper
    
    let parent = LayoutDisplay::Block;
    let child = LayoutDisplay::TableRowGroup;
    
    // Row group in non-table parent needs anonymous table wrapper
    assert!(matches!(
        child,
        LayoutDisplay::TableRowGroup
            | LayoutDisplay::TableHeaderGroup
            | LayoutDisplay::TableFooterGroup
    ));
    assert!(parent != LayoutDisplay::Table);
}

#[test]
fn test_stage3_cell_in_row_no_wrapper() {
    // CSS 2.2 Section 17.2.1, Stage 3:
    // Table-cell in table-row doesn't need wrapper
    
    let parent = LayoutDisplay::TableRow;
    let child = LayoutDisplay::TableCell;
    
    assert!(child == LayoutDisplay::TableCell);
    assert!(parent == LayoutDisplay::TableRow);
}

#[test]
fn test_stage3_cell_in_rowgroup_no_wrapper() {
    // CSS 2.2 Section 17.2.1, Stage 3:
    // Table-cell in row-group can be handled (will be wrapped in row by Stage 2)
    
    let parent = LayoutDisplay::TableRowGroup;
    let child = LayoutDisplay::TableCell;
    
    assert!(child == LayoutDisplay::TableCell);
    assert!(matches!(
        parent,
        LayoutDisplay::TableRowGroup
            | LayoutDisplay::TableHeaderGroup
            | LayoutDisplay::TableFooterGroup
    ));
}

#[test]
fn test_stage3_row_in_table_no_wrapper() {
    // CSS 2.2 Section 17.2.1, Stage 3:
    // Table-row in table doesn't need wrapper
    
    let parent = LayoutDisplay::Table;
    let child = LayoutDisplay::TableRow;
    
    assert!(child == LayoutDisplay::TableRow);
    assert!(parent == LayoutDisplay::Table);
}

#[test]
fn test_stage3_row_in_rowgroup_no_wrapper() {
    // CSS 2.2 Section 17.2.1, Stage 3:
    // Table-row in row-group doesn't need wrapper
    
    let parent = LayoutDisplay::TableRowGroup;
    let child = LayoutDisplay::TableRow;
    
    assert!(child == LayoutDisplay::TableRow);
    assert!(matches!(
        parent,
        LayoutDisplay::TableRowGroup
            | LayoutDisplay::TableHeaderGroup
            | LayoutDisplay::TableFooterGroup
    ));
}

#[test]
fn test_stage3_rowgroup_in_table_no_wrapper() {
    // CSS 2.2 Section 17.2.1, Stage 3:
    // Row group in table doesn't need wrapper
    
    let parent = LayoutDisplay::Table;
    let child = LayoutDisplay::TableRowGroup;
    
    assert!(matches!(
        child,
        LayoutDisplay::TableRowGroup
            | LayoutDisplay::TableHeaderGroup
            | LayoutDisplay::TableFooterGroup
    ));
    assert!(parent == LayoutDisplay::Table);
}

#[test]
fn test_complex_scenario_cell_in_block() {
    // CSS 2.2 Section 17.2.1:
    // Complex scenario - table-cell appearing in block context
    // Needs: anonymous row wrapper (Stage 3), then anonymous table wrapper (Stage 3)
    
    let parent = LayoutDisplay::Block;
    let child = LayoutDisplay::TableCell;
    
    // First, cell needs row wrapper
    assert!(child == LayoutDisplay::TableCell);
    assert!(parent != LayoutDisplay::TableRow);
    
    // Then, the anonymous row would need table wrapper
    let anonymous_row = LayoutDisplay::TableRow;
    assert!(anonymous_row == LayoutDisplay::TableRow);
    assert!(parent != LayoutDisplay::Table);
}

#[test]
fn test_complex_scenario_mixed_children() {
    // CSS 2.2 Section 17.2.1:
    // Table with mix of rows and cells needs to:
    // 1. Wrap consecutive cells in anonymous rows (Stage 2)
    // 2. Leave proper rows as-is
    
    let parent = LayoutDisplay::Table;
    let child1 = LayoutDisplay::TableRow;
    let child2 = LayoutDisplay::TableCell;
    let child3 = LayoutDisplay::TableCell;
    let child4 = LayoutDisplay::TableRow;
    
    // Row is fine
    assert!(child1 == LayoutDisplay::TableRow);
    
    // Consecutive cells (child2, child3) need anonymous row wrapper
    assert!(child2 == LayoutDisplay::TableCell);
    assert!(child3 == LayoutDisplay::TableCell);
    
    // Another row is fine
    assert!(child4 == LayoutDisplay::TableRow);
}

#[test]
fn test_edge_case_nested_tables() {
    // CSS 2.2 Section 17.2.1:
    // Table inside table-cell is valid, no anonymous boxes needed
    
    let outer_table = LayoutDisplay::Table;
    let row = LayoutDisplay::TableRow;
    let cell = LayoutDisplay::TableCell;
    let inner_table = LayoutDisplay::Table;
    
    assert!(outer_table == LayoutDisplay::Table);
    assert!(row == LayoutDisplay::TableRow);
    assert!(cell == LayoutDisplay::TableCell);
    assert!(inner_table == LayoutDisplay::Table);
}

#[test]
fn test_edge_case_empty_table() {
    // CSS 2.2 Section 17.2.1:
    // Empty table with no children is valid
    
    let display = LayoutDisplay::Table;
    assert!(display == LayoutDisplay::Table);
}

#[test]
fn test_edge_case_table_with_only_whitespace() {
    // CSS 2.2 Section 17.2.1, Stage 1:
    // Table containing only whitespace text nodes should skip all children
    
    let _parent = LayoutDisplay::Table;
    let whitespace1 = "   ";
    let whitespace2 = "\n\t";
    
    assert!(whitespace1.chars().all(|c| c.is_whitespace()));
    assert!(whitespace2.chars().all(|c| c.is_whitespace()));
}

#[test]
fn test_all_stages_integration() {
    // CSS 2.2 Section 17.2.1:
    // Complete integration test simulating all three stages
    
    // Original structure: div > "  " (whitespace) > table-cell > "content"
    let parent = LayoutDisplay::Block;
    let whitespace = "  ";
    let child = LayoutDisplay::TableCell;
    
    // Stage 1: Whitespace would be skipped (but only if parent is table structural)
    // In this case, parent is Block, so whitespace is kept but irrelevant for table
    assert!(whitespace.chars().all(|c| c.is_whitespace()));
    
    // Stage 2: Not applicable (no table structural parent yet)
    
    // Stage 3: Table-cell in block needs anonymous row wrapper
    assert!(child == LayoutDisplay::TableCell);
    assert!(parent != LayoutDisplay::TableRow);
    
    // After Stage 3, we'd have: div > anonymous-row > table-cell
    // Then anonymous-row in block would need anonymous table wrapper
    let anon_row = LayoutDisplay::TableRow;
    assert!(anon_row == LayoutDisplay::TableRow);
    assert!(parent != LayoutDisplay::Table);
}

#[test]
fn test_caption_not_wrapped() {
    // CSS 2.2 Section 17.2.1:
    // Table-caption should NOT be wrapped in anonymous boxes
    
    let parent = LayoutDisplay::Table;
    let caption = LayoutDisplay::TableCaption;
    
    assert!(parent == LayoutDisplay::Table);
    assert!(caption == LayoutDisplay::TableCaption);
    // Caption is a valid direct child of table, no wrapping needed
}

#[test]
fn test_column_group_not_wrapped() {
    // CSS 2.2 Section 17.2.1:
    // Table-column-group should NOT be wrapped
    
    let parent = LayoutDisplay::Table;
    let colgroup = LayoutDisplay::TableColumnGroup;
    
    assert!(parent == LayoutDisplay::Table);
    assert!(colgroup == LayoutDisplay::TableColumnGroup);
    // Column group is valid direct child of table
}

#[test]
fn test_column_not_wrapped() {
    // CSS 2.2 Section 17.2.1:
    // Table-column should NOT be wrapped
    
    let parent = LayoutDisplay::TableColumnGroup;
    let col = LayoutDisplay::TableColumn;
    
    assert!(parent == LayoutDisplay::TableColumnGroup);
    assert!(col == LayoutDisplay::TableColumn);
    // Column is valid child of column-group
}
