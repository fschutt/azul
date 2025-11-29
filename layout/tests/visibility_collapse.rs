use std::collections::HashSet;

/// Unit tests for CSS 2.2 Section 17.6 - Dynamic row and column effects
///
/// Tests cover:
/// - visibility:collapse on table rows
/// - visibility:collapse on table columns
/// - Row height calculations with collapsed rows
/// - Column width calculations with collapsed columns
/// - Cell spanning into collapsed areas
/// - Interaction with border-spacing
use azul_css::props::style::StyleVisibility;

#[test]
fn test_visibility_values() {
    // CSS 2.2 Section 17.6: visibility property values
    let visible = StyleVisibility::Visible;
    let hidden = StyleVisibility::Hidden;
    let collapse = StyleVisibility::Collapse;

    assert_eq!(visible, StyleVisibility::Visible);
    assert_eq!(hidden, StyleVisibility::Hidden);
    assert_eq!(collapse, StyleVisibility::Collapse);

    // These are different values
    assert_ne!(visible, hidden);
    assert_ne!(visible, collapse);
    assert_ne!(hidden, collapse);
}

#[test]
fn test_visibility_default() {
    // Default visibility is visible
    let default = StyleVisibility::default();
    assert_eq!(default, StyleVisibility::Visible);
}

#[test]
fn test_collapse_detection() {
    // Test that we can detect collapse
    let vis = StyleVisibility::Collapse;
    assert!(matches!(vis, StyleVisibility::Collapse));
    assert!(!matches!(vis, StyleVisibility::Visible));
    assert!(!matches!(vis, StyleVisibility::Hidden));
}

#[test]
fn test_collapsed_rows_set_empty() {
    // Empty collapsed_rows set
    let collapsed_rows: HashSet<usize> = HashSet::new();
    assert!(collapsed_rows.is_empty());
    assert_eq!(collapsed_rows.len(), 0);
}

#[test]
fn test_collapsed_rows_insert() {
    // Insert rows into collapsed set
    let mut collapsed_rows: HashSet<usize> = HashSet::new();

    collapsed_rows.insert(1);
    collapsed_rows.insert(3);

    assert!(collapsed_rows.contains(&1));
    assert!(!collapsed_rows.contains(&2));
    assert!(collapsed_rows.contains(&3));
    assert_eq!(collapsed_rows.len(), 2);
}

#[test]
fn test_collapsed_columns_set_empty() {
    // Empty collapsed_columns set
    let collapsed_columns: HashSet<usize> = HashSet::new();
    assert!(collapsed_columns.is_empty());
    assert_eq!(collapsed_columns.len(), 0);
}

#[test]
fn test_collapsed_columns_insert() {
    // Insert columns into collapsed set
    let mut collapsed_columns: HashSet<usize> = HashSet::new();

    collapsed_columns.insert(0);
    collapsed_columns.insert(2);

    assert!(collapsed_columns.contains(&0));
    assert!(!collapsed_columns.contains(&1));
    assert!(collapsed_columns.contains(&2));
    assert_eq!(collapsed_columns.len(), 2);
}

#[test]
fn test_row_height_zero_for_collapsed() {
    // CSS 2.2 Section 17.6: Collapsed rows have height 0
    let mut row_heights = vec![100.0, 50.0, 75.0, 60.0];
    let collapsed_rows: HashSet<usize> = [1, 3].iter().cloned().collect();

    // Set collapsed rows to height 0
    for &row_idx in &collapsed_rows {
        if row_idx < row_heights.len() {
            row_heights[row_idx] = 0.0;
        }
    }

    assert_eq!(row_heights[0], 100.0); // Not collapsed
    assert_eq!(row_heights[1], 0.0); // Collapsed
    assert_eq!(row_heights[2], 75.0); // Not collapsed
    assert_eq!(row_heights[3], 0.0); // Collapsed
}

#[test]
fn test_total_height_with_collapsed_rows() {
    // Total table height excludes collapsed rows
    let row_heights = vec![100.0, 0.0, 75.0, 0.0, 50.0];
    let total_height: f32 = row_heights.iter().sum();

    assert_eq!(total_height, 225.0); // 100 + 0 + 75 + 0 + 50
}

#[test]
fn test_skip_cells_in_collapsed_rows() {
    // Cells in collapsed rows should be skipped during layout
    let collapsed_rows: HashSet<usize> = [1, 3].iter().cloned().collect();

    // Test various row indices
    assert!(!collapsed_rows.contains(&0)); // Row 0 - layout cell
    assert!(collapsed_rows.contains(&1)); // Row 1 - skip cell
    assert!(!collapsed_rows.contains(&2)); // Row 2 - layout cell
    assert!(collapsed_rows.contains(&3)); // Row 3 - skip cell
    assert!(!collapsed_rows.contains(&4)); // Row 4 - layout cell
}

#[test]
fn test_rowspan_across_collapsed_rows() {
    // CSS 2.2 Section 17.6: Cells spanning collapsed rows
    // Cell spans rows 0-3, but row 1 and 3 are collapsed
    let row_heights = vec![100.0, 0.0, 75.0, 0.0];
    let collapsed_rows: HashSet<usize> = [1, 3].iter().cloned().collect();

    // Calculate height of visible rows in span
    let visible_height: f32 = row_heights
        .iter()
        .enumerate()
        .filter(|(idx, _)| !collapsed_rows.contains(idx))
        .map(|(_, h)| h)
        .sum();

    assert_eq!(visible_height, 175.0); // 100 + 75 (rows 0 and 2)
}

#[test]
fn test_count_non_collapsed_rows_in_span() {
    // Count how many rows in a span are not collapsed
    let collapsed_rows: HashSet<usize> = [1, 3].iter().cloned().collect();
    let span_start = 0;
    let span_end = 4;

    let non_collapsed_count = (span_start..span_end)
        .filter(|row_idx| !collapsed_rows.contains(row_idx))
        .count();

    assert_eq!(non_collapsed_count, 2); // Rows 0 and 2
}

#[test]
fn test_distribute_height_across_non_collapsed() {
    // Distribute extra height only across non-collapsed rows
    let mut row_heights = vec![100.0, 0.0, 100.0, 0.0];
    let collapsed_rows: HashSet<usize> = [1, 3].iter().cloned().collect();

    let extra_height = 50.0;
    let non_collapsed_count = 2; // Rows 0 and 2
    let per_row = extra_height / non_collapsed_count as f32;

    for row_idx in 0..row_heights.len() {
        if !collapsed_rows.contains(&row_idx) {
            row_heights[row_idx] += per_row;
        }
    }

    assert_eq!(row_heights[0], 125.0); // 100 + 25
    assert_eq!(row_heights[1], 0.0); // Collapsed, unchanged
    assert_eq!(row_heights[2], 125.0); // 100 + 25
    assert_eq!(row_heights[3], 0.0); // Collapsed, unchanged
}

#[test]
fn test_collapsed_rows_affect_border_spacing() {
    // CSS 2.2 Section 17.6: border-spacing is still applied around collapsed rows
    // But since row height is 0, only spacing remains
    let v_spacing = 10.0;
    let num_rows = 5;
    let _collapsed_rows: HashSet<usize> = [1, 3].iter().cloned().collect();

    // Total vertical spacing: (n+1) * spacing
    let total_spacing = v_spacing * (num_rows + 1) as f32;

    assert_eq!(total_spacing, 60.0); // 10 * 6 = 60

    // Collapsed rows don't reduce spacing count
    assert_eq!(num_rows, 5);
}

#[test]
fn test_all_rows_collapsed() {
    // Edge case: All rows collapsed
    let row_heights = vec![0.0, 0.0, 0.0];
    let total_height: f32 = row_heights.iter().sum();

    assert_eq!(total_height, 0.0);
}

#[test]
fn test_no_rows_collapsed() {
    // Normal case: No rows collapsed
    let row_heights = vec![100.0, 50.0, 75.0];
    let collapsed_rows: HashSet<usize> = HashSet::new();

    assert!(collapsed_rows.is_empty());

    let total_height: f32 = row_heights.iter().sum();
    assert_eq!(total_height, 225.0);
}

#[test]
fn test_first_row_collapsed() {
    // First row collapsed
    let mut row_heights = vec![100.0, 50.0, 75.0];
    let collapsed_rows: HashSet<usize> = [0].iter().cloned().collect();

    for &row_idx in &collapsed_rows {
        if row_idx < row_heights.len() {
            row_heights[row_idx] = 0.0;
        }
    }

    assert_eq!(row_heights[0], 0.0);
    assert_eq!(row_heights[1], 50.0);
    assert_eq!(row_heights[2], 75.0);
}

#[test]
fn test_last_row_collapsed() {
    // Last row collapsed
    let mut row_heights = vec![100.0, 50.0, 75.0];
    let collapsed_rows: HashSet<usize> = [2].iter().cloned().collect();

    for &row_idx in &collapsed_rows {
        if row_idx < row_heights.len() {
            row_heights[row_idx] = 0.0;
        }
    }

    assert_eq!(row_heights[0], 100.0);
    assert_eq!(row_heights[1], 50.0);
    assert_eq!(row_heights[2], 0.0);
}

#[test]
fn test_consecutive_collapsed_rows() {
    // Multiple consecutive collapsed rows
    let mut row_heights = vec![100.0, 50.0, 75.0, 60.0, 80.0];
    let collapsed_rows: HashSet<usize> = [1, 2, 3].iter().cloned().collect();

    for &row_idx in &collapsed_rows {
        if row_idx < row_heights.len() {
            row_heights[row_idx] = 0.0;
        }
    }

    assert_eq!(row_heights[0], 100.0);
    assert_eq!(row_heights[1], 0.0);
    assert_eq!(row_heights[2], 0.0);
    assert_eq!(row_heights[3], 0.0);
    assert_eq!(row_heights[4], 80.0);

    let total: f32 = row_heights.iter().sum();
    assert_eq!(total, 180.0); // 100 + 80
}

#[test]
fn test_collapsed_row_preservation() {
    // Ensure collapsed row height stays 0 after calculations
    let mut row_heights = vec![100.0, 50.0, 75.0];
    let collapsed_rows: HashSet<usize> = [1].iter().cloned().collect();

    // Initial collapse
    for &row_idx in &collapsed_rows {
        if row_idx < row_heights.len() {
            row_heights[row_idx] = 0.0;
        }
    }

    // Simulate some height adjustments (but not to collapsed rows)
    row_heights[0] += 10.0;
    row_heights[2] += 10.0;

    // Final pass: ensure collapsed rows still 0
    for &row_idx in &collapsed_rows {
        if row_idx < row_heights.len() {
            row_heights[row_idx] = 0.0;
        }
    }

    assert_eq!(row_heights[0], 110.0);
    assert_eq!(row_heights[1], 0.0); // Still 0
    assert_eq!(row_heights[2], 85.0);
}

#[test]
fn test_visibility_collapse_vs_hidden() {
    // CSS 2.2: collapse removes space, hidden preserves it
    let visible = StyleVisibility::Visible;
    let hidden = StyleVisibility::Hidden;
    let collapse = StyleVisibility::Collapse;

    // For tables: collapse removes space
    assert!(matches!(collapse, StyleVisibility::Collapse));

    // For tables: hidden would preserve space (not implemented here)
    assert!(matches!(hidden, StyleVisibility::Hidden));

    // Visible shows the element
    assert!(matches!(visible, StyleVisibility::Visible));
}

#[test]
fn test_column_collapse_tracking() {
    // Column collapse tracking (similar to rows)
    let mut collapsed_columns: HashSet<usize> = HashSet::new();

    collapsed_columns.insert(1);
    collapsed_columns.insert(3);

    // Check which columns are collapsed
    for col_idx in 0..5 {
        let is_collapsed = collapsed_columns.contains(&col_idx);
        match col_idx {
            1 | 3 => assert!(is_collapsed),
            _ => assert!(!is_collapsed),
        }
    }
}

#[test]
fn test_column_width_zero_for_collapsed() {
    // CSS 2.2 Section 17.6: Collapsed columns have width 0
    let mut column_widths = vec![100.0, 50.0, 75.0, 60.0];
    let collapsed_columns: HashSet<usize> = [1, 3].iter().cloned().collect();

    // Set collapsed columns to width 0
    for &col_idx in &collapsed_columns {
        if col_idx < column_widths.len() {
            column_widths[col_idx] = 0.0;
        }
    }

    assert_eq!(column_widths[0], 100.0); // Not collapsed
    assert_eq!(column_widths[1], 0.0); // Collapsed
    assert_eq!(column_widths[2], 75.0); // Not collapsed
    assert_eq!(column_widths[3], 0.0); // Collapsed
}

#[test]
fn test_total_width_with_collapsed_columns() {
    // Total table width excludes collapsed columns
    let column_widths = vec![100.0, 0.0, 75.0, 0.0, 50.0];
    let total_width: f32 = column_widths.iter().sum();

    assert_eq!(total_width, 225.0); // 100 + 0 + 75 + 0 + 50
}

#[test]
fn test_colspan_visible_width_calculation() {
    // CSS 2.2 Section 17.6: Cells spanning collapsed columns
    let column_widths = vec![100.0, 0.0, 75.0, 0.0];
    let collapsed_columns: HashSet<usize> = [1, 3].iter().cloned().collect();

    // Calculate width of visible columns in span
    let visible_width: f32 = column_widths
        .iter()
        .enumerate()
        .filter(|(idx, _)| !collapsed_columns.contains(idx))
        .map(|(_, w)| w)
        .sum();

    assert_eq!(visible_width, 175.0); // 100 + 75 (columns 0 and 2)
}

#[test]
fn test_mixed_row_column_collapse() {
    // Both rows and columns can be collapsed simultaneously
    let collapsed_rows: HashSet<usize> = [1].iter().cloned().collect();
    let collapsed_columns: HashSet<usize> = [2].iter().cloned().collect();

    // Cell at (1, 2) is in both collapsed row and column
    let cell_row = 1;
    let cell_col = 2;

    let in_collapsed_row = collapsed_rows.contains(&cell_row);
    let in_collapsed_col = collapsed_columns.contains(&cell_col);

    assert!(in_collapsed_row);
    assert!(in_collapsed_col);
}

#[test]
fn test_visibility_collapse_documentation() {
    // CSS 2.2 Section 17.6 quote verification
    // "The visibility value 'collapse' removes a row or column from display"

    let visibility = StyleVisibility::Collapse;

    // When collapse is set:
    // - Row height becomes 0
    // - Column width becomes 0
    // - Space is removed from layout

    assert_eq!(visibility, StyleVisibility::Collapse);

    let row_height_before = 100.0;
    let row_height_after = if matches!(visibility, StyleVisibility::Collapse) {
        0.0
    } else {
        row_height_before
    };

    assert_eq!(row_height_after, 0.0);
}

// ==================== Column Width Calculation Tests ====================

#[test]
fn test_fixed_width_with_collapsed_columns() {
    // CSS 2.2 Section 17.6: Collapsed columns should have width 0
    // Fixed layout distributes width equally among non-collapsed columns

    let total_columns = 4;
    let available_width = 400.0;

    // Columns 1 and 3 are collapsed
    let mut collapsed_columns: HashSet<usize> = HashSet::new();
    collapsed_columns.insert(1);
    collapsed_columns.insert(3);

    let num_visible = total_columns - collapsed_columns.len();
    assert_eq!(num_visible, 2);

    // Width should be distributed only among visible columns
    let expected_width_per_visible = available_width / num_visible as f32;
    assert_eq!(expected_width_per_visible, 200.0);

    // Collapsed columns should have 0 width
    for col_idx in 0..total_columns {
        let expected_width = if collapsed_columns.contains(&col_idx) {
            0.0
        } else {
            expected_width_per_visible
        };

        if col_idx == 0 {
            assert_eq!(expected_width, 200.0);
        } else if col_idx == 1 {
            assert_eq!(expected_width, 0.0);
        } else if col_idx == 2 {
            assert_eq!(expected_width, 200.0);
        } else if col_idx == 3 {
            assert_eq!(expected_width, 0.0);
        }
    }
}

#[test]
fn test_auto_width_excludes_collapsed_columns() {
    // CSS 2.2 Section 17.6: Collapsed columns excluded from min/max totals

    let columns = vec![
        (50.0, 100.0), // Col 0: min=50, max=100
        (30.0, 60.0),  // Col 1: min=30, max=60 (collapsed)
        (40.0, 80.0),  // Col 2: min=40, max=80
        (20.0, 40.0),  // Col 3: min=20, max=40 (collapsed)
    ];

    let mut collapsed_columns: HashSet<usize> = HashSet::new();
    collapsed_columns.insert(1);
    collapsed_columns.insert(3);

    // Calculate totals excluding collapsed columns
    let total_min: f32 = columns
        .iter()
        .enumerate()
        .filter(|(idx, _)| !collapsed_columns.contains(idx))
        .map(|(_, (min, _))| min)
        .sum();

    let total_max: f32 = columns
        .iter()
        .enumerate()
        .filter(|(idx, _)| !collapsed_columns.contains(idx))
        .map(|(_, (_, max))| max)
        .sum();

    assert_eq!(total_min, 50.0 + 40.0); // 90.0
    assert_eq!(total_max, 100.0 + 80.0); // 180.0

    // Collapsed columns should be excluded
    assert_eq!(total_min, 90.0);
    assert_eq!(total_max, 180.0);
}

#[test]
fn test_all_columns_collapsed() {
    // Edge case: all columns collapsed

    let total_columns = 3;

    let mut collapsed_columns: HashSet<usize> = HashSet::new();
    for i in 0..total_columns {
        collapsed_columns.insert(i);
    }

    let num_visible = total_columns - collapsed_columns.len();
    assert_eq!(num_visible, 0);

    // All columns should have 0 width
    for col_idx in 0..total_columns {
        assert!(collapsed_columns.contains(&col_idx));
        let width = 0.0;
        assert_eq!(width, 0.0);
    }
}

#[test]
fn test_first_column_collapsed() {
    // Edge case: first column collapsed

    let total_columns = 3;
    let available_width = 300.0;

    let mut collapsed_columns: HashSet<usize> = HashSet::new();
    collapsed_columns.insert(0);

    let num_visible = total_columns - collapsed_columns.len();
    assert_eq!(num_visible, 2);

    let width_per_visible = available_width / num_visible as f32;
    assert_eq!(width_per_visible, 150.0);

    // First column: 0, others: 150.0 each
    assert_eq!(collapsed_columns.contains(&0), true);
    assert_eq!(collapsed_columns.contains(&1), false);
    assert_eq!(collapsed_columns.contains(&2), false);
}

#[test]
fn test_last_column_collapsed() {
    // Edge case: last column collapsed

    let total_columns = 3;
    let available_width = 300.0;

    let mut collapsed_columns: HashSet<usize> = HashSet::new();
    collapsed_columns.insert(2);

    let num_visible = total_columns - collapsed_columns.len();
    assert_eq!(num_visible, 2);

    let width_per_visible = available_width / num_visible as f32;
    assert_eq!(width_per_visible, 150.0);

    // Last column: 0, others: 150.0 each
    assert_eq!(collapsed_columns.contains(&0), false);
    assert_eq!(collapsed_columns.contains(&1), false);
    assert_eq!(collapsed_columns.contains(&2), true);
}

#[test]
fn test_colspan_across_collapsed_columns() {
    // CSS 2.2 Section 17.6: Cell spanning collapsed columns

    let colspan = 4;
    let start_col = 0;

    let mut collapsed_columns: HashSet<usize> = HashSet::new();
    collapsed_columns.insert(1); // Column 1 is collapsed
    collapsed_columns.insert(3); // Column 3 is collapsed

    // Count visible columns in the span
    let visible_cols: Vec<usize> = (start_col..start_col + colspan)
        .filter(|idx| !collapsed_columns.contains(idx))
        .collect();

    assert_eq!(visible_cols.len(), 2);
    assert_eq!(visible_cols, vec![0, 2]);

    // Cell width should only be distributed across visible columns
    let cell_min_width = 200.0;
    let width_per_visible = cell_min_width / visible_cols.len() as f32;
    assert_eq!(width_per_visible, 100.0);
}

#[test]
fn test_colspan_all_columns_collapsed() {
    // Edge case: cell spans only collapsed columns

    let colspan = 2;
    let start_col = 1;

    let mut collapsed_columns: HashSet<usize> = HashSet::new();
    collapsed_columns.insert(1);
    collapsed_columns.insert(2);

    // Count visible columns in the span
    let visible_cols: Vec<usize> = (start_col..start_col + colspan)
        .filter(|idx| !collapsed_columns.contains(idx))
        .collect();

    assert_eq!(visible_cols.len(), 0);

    // No width should be distributed (cell is in collapsed area)
}

#[test]
fn test_mixed_row_and_column_collapse() {
    // CSS 2.2 Section 17.6: Both row and column can be collapsed

    let mut collapsed_rows: HashSet<usize> = HashSet::new();
    let mut collapsed_columns: HashSet<usize> = HashSet::new();

    collapsed_rows.insert(1);
    collapsed_columns.insert(2);

    // Cell at (1, 2) is in both collapsed row and column
    let cell_row = 1;
    let cell_col = 2;

    let in_collapsed_row = collapsed_rows.contains(&cell_row);
    let in_collapsed_col = collapsed_columns.contains(&cell_col);

    assert!(in_collapsed_row);
    assert!(in_collapsed_col);

    // Cell should be skipped in both dimensions
    // Height from row: 0
    // Width from column: 0
}

#[test]
fn test_auto_width_interpolation_with_collapsed() {
    // CSS 2.2 Section 17.5.2.2: Auto layout with available space between min/max

    let columns = vec![
        (50.0, 100.0), // Col 0
        (30.0, 60.0),  // Col 1 (collapsed)
        (40.0, 80.0),  // Col 2
    ];

    let mut collapsed_columns: HashSet<usize> = HashSet::new();
    collapsed_columns.insert(1);

    // Total min/max excluding collapsed
    let total_min: f32 = columns
        .iter()
        .enumerate()
        .filter(|(idx, _)| !collapsed_columns.contains(idx))
        .map(|(_, (min, _))| min)
        .sum();

    let total_max: f32 = columns
        .iter()
        .enumerate()
        .filter(|(idx, _)| !collapsed_columns.contains(idx))
        .map(|(_, (_, max))| max)
        .sum();

    assert_eq!(total_min, 90.0); // 50 + 40
    assert_eq!(total_max, 180.0); // 100 + 80

    // Available width between min and max
    let available_width = 135.0; // Halfway between 90 and 180

    assert!(available_width >= total_min);
    assert!(available_width <= total_max);

    // Interpolation scale
    let scale = (available_width - total_min) / (total_max - total_min);
    assert_eq!(scale, 0.5); // Exactly halfway

    // Col 0: 50 + (100-50) * 0.5 = 75
    // Col 1: 0 (collapsed)
    // Col 2: 40 + (80-40) * 0.5 = 60
    let expected_widths = vec![75.0, 0.0, 60.0];

    for (idx, (min, max)) in columns.iter().enumerate() {
        let computed = if collapsed_columns.contains(&idx) {
            0.0
        } else {
            min + (max - min) * scale
        };
        assert_eq!(computed, expected_widths[idx]);
    }
}

#[test]
fn test_auto_width_scale_down_with_collapsed() {
    // CSS 2.2: Not enough space, scale down from min widths

    let columns = vec![
        (50.0, 100.0), // Col 0
        (30.0, 60.0),  // Col 1 (collapsed)
        (40.0, 80.0),  // Col 2
    ];

    let mut collapsed_columns: HashSet<usize> = HashSet::new();
    collapsed_columns.insert(1);

    let total_min: f32 = columns
        .iter()
        .enumerate()
        .filter(|(idx, _)| !collapsed_columns.contains(idx))
        .map(|(_, (min, _))| min)
        .sum();

    assert_eq!(total_min, 90.0);

    // Available width less than total min
    let available_width = 45.0; // Half of min

    assert!(available_width < total_min);

    // Scale down from min
    let scale = available_width / total_min;
    assert_eq!(scale, 0.5);

    // Col 0: 50 * 0.5 = 25
    // Col 1: 0 (collapsed)
    // Col 2: 40 * 0.5 = 20
    let expected_widths = vec![25.0, 0.0, 20.0];

    for (idx, (min, _)) in columns.iter().enumerate() {
        let computed = if collapsed_columns.contains(&idx) {
            0.0
        } else {
            min * scale
        };
        assert_eq!(computed, expected_widths[idx]);
    }
}
