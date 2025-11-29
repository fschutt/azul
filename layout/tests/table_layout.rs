// Table layout unit tests
// Tests for CSS 2.2 Chapter 17 Table Layout implementation

use azul_css::props::{basic::ColorU, style::BorderStyle};
use azul_layout::solver3::fc::{BorderInfo, BorderSource, TableCellInfo, TableColumnInfo};

// ==================== Border Conflict Resolution Tests ====================

#[test]
fn test_border_conflict_hidden_wins() {
    // CSS 2.2: 'hidden' suppresses all borders
    let hidden = BorderInfo {
        width: 1.0,
        style: BorderStyle::Hidden,
        color: ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        },
        source: BorderSource::Cell,
    };

    let solid = BorderInfo {
        width: 5.0,
        style: BorderStyle::Solid,
        color: ColorU {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
        source: BorderSource::Cell,
    };

    let result = BorderInfo::resolve_conflict(&hidden, &solid);
    assert!(result.is_none()); // Hidden suppresses all borders

    let result2 = BorderInfo::resolve_conflict(&solid, &hidden);
    assert!(result2.is_none());
}

#[test]
fn test_border_conflict_none_loses() {
    // 'none' has lowest priority
    let none = BorderInfo {
        width: 0.0,
        style: BorderStyle::None,
        color: ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        },
        source: BorderSource::Cell,
    };

    let solid = BorderInfo {
        width: 1.0,
        style: BorderStyle::Solid,
        color: ColorU {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
        source: BorderSource::Cell,
    };

    let result = BorderInfo::resolve_conflict(&none, &solid);
    assert!(result.is_some());
    let border = result.unwrap();
    assert_eq!(border.style, BorderStyle::Solid);
    assert_eq!(border.width, 1.0);
}

#[test]
fn test_border_conflict_width_priority() {
    // Wider borders win
    let thin = BorderInfo {
        width: 1.0,
        style: BorderStyle::Solid,
        color: ColorU {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
        source: BorderSource::Cell,
    };

    let thick = BorderInfo {
        width: 5.0,
        style: BorderStyle::Solid,
        color: ColorU {
            r: 0,
            g: 255,
            b: 0,
            a: 255,
        },
        source: BorderSource::Cell,
    };

    let result = BorderInfo::resolve_conflict(&thin, &thick);
    assert!(result.is_some());
    let border = result.unwrap();
    assert_eq!(border.width, 5.0);
    assert_eq!(border.color.g, 255); // Should be green
}

#[test]
fn test_border_conflict_style_priority() {
    // double > solid > dashed > dotted > ridge > outset > groove > inset
    let solid = BorderInfo {
        width: 2.0,
        style: BorderStyle::Solid,
        color: ColorU {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
        source: BorderSource::Cell,
    };

    let double = BorderInfo {
        width: 2.0,
        style: BorderStyle::Double,
        color: ColorU {
            r: 0,
            g: 255,
            b: 0,
            a: 255,
        },
        source: BorderSource::Cell,
    };

    let result = BorderInfo::resolve_conflict(&solid, &double);
    assert!(result.is_some());
    let border = result.unwrap();
    assert_eq!(border.style, BorderStyle::Double);
    assert_eq!(border.color.g, 255);
}

#[test]
fn test_border_conflict_style_priority_dashed_vs_dotted() {
    let dashed = BorderInfo {
        width: 2.0,
        style: BorderStyle::Dashed,
        color: ColorU {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
        source: BorderSource::Cell,
    };

    let dotted = BorderInfo {
        width: 2.0,
        style: BorderStyle::Dotted,
        color: ColorU {
            r: 0,
            g: 255,
            b: 0,
            a: 255,
        },
        source: BorderSource::Cell,
    };

    let result = BorderInfo::resolve_conflict(&dashed, &dotted);
    assert!(result.is_some());
    let border = result.unwrap();
    assert_eq!(border.style, BorderStyle::Dashed);
}

#[test]
fn test_border_conflict_source_priority() {
    // cell > row > rowgroup > column > columngroup > table
    let cell_border = BorderInfo {
        width: 2.0,
        style: BorderStyle::Solid,
        color: ColorU {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
        source: BorderSource::Cell,
    };

    let row_border = BorderInfo {
        width: 2.0,
        style: BorderStyle::Solid,
        color: ColorU {
            r: 0,
            g: 255,
            b: 0,
            a: 255,
        },
        source: BorderSource::Row,
    };

    let result = BorderInfo::resolve_conflict(&cell_border, &row_border);
    assert!(result.is_some());
    let border = result.unwrap();
    assert_eq!(border.source, BorderSource::Cell);
    assert_eq!(border.color.r, 255); // Red (cell color)
}

#[test]
fn test_border_conflict_source_priority_full_hierarchy() {
    // Test all source priorities
    let borders = vec![
        (
            BorderSource::Table,
            ColorU {
                r: 10,
                g: 0,
                b: 0,
                a: 255,
            },
        ),
        (
            BorderSource::ColumnGroup,
            ColorU {
                r: 20,
                g: 0,
                b: 0,
                a: 255,
            },
        ),
        (
            BorderSource::Column,
            ColorU {
                r: 30,
                g: 0,
                b: 0,
                a: 255,
            },
        ),
        (
            BorderSource::RowGroup,
            ColorU {
                r: 40,
                g: 0,
                b: 0,
                a: 255,
            },
        ),
        (
            BorderSource::Row,
            ColorU {
                r: 50,
                g: 0,
                b: 0,
                a: 255,
            },
        ),
        (
            BorderSource::Cell,
            ColorU {
                r: 60,
                g: 0,
                b: 0,
                a: 255,
            },
        ),
    ];

    // Cell should win against all others
    let cell = BorderInfo {
        width: 2.0,
        style: BorderStyle::Solid,
        color: ColorU {
            r: 60,
            g: 0,
            b: 0,
            a: 255,
        },
        source: BorderSource::Cell,
    };

    for (source, color) in borders.iter() {
        if *source == BorderSource::Cell {
            continue;
        }
        let other = BorderInfo {
            width: 2.0,
            style: BorderStyle::Solid,
            color: *color,
            source: *source,
        };
        let result = BorderInfo::resolve_conflict(&cell, &other);
        assert!(result.is_some());
        let border = result.unwrap();
        assert_eq!(border.source, BorderSource::Cell);
        assert_eq!(border.color.r, 60);
    }

    // Row should win against lower priorities
    let row = BorderInfo {
        width: 2.0,
        style: BorderStyle::Solid,
        color: ColorU {
            r: 50,
            g: 0,
            b: 0,
            a: 255,
        },
        source: BorderSource::Row,
    };

    let table = BorderInfo {
        width: 2.0,
        style: BorderStyle::Solid,
        color: ColorU {
            r: 10,
            g: 0,
            b: 0,
            a: 255,
        },
        source: BorderSource::Table,
    };

    let result = BorderInfo::resolve_conflict(&row, &table);
    assert!(result.is_some());
    let border = result.unwrap();
    assert_eq!(border.source, BorderSource::Row);
    assert_eq!(border.color.r, 50);
}

#[test]
fn test_border_conflict_complex_scenario() {
    // Test a complex scenario: different width, style, and source
    // Scenario: Cell has thin dashed border, Row has thick solid border
    // Width wins first, so Row wins
    let cell = BorderInfo {
        width: 1.0,
        style: BorderStyle::Dashed,
        color: ColorU {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
        source: BorderSource::Cell,
    };

    let row = BorderInfo {
        width: 3.0,
        style: BorderStyle::Solid,
        color: ColorU {
            r: 0,
            g: 255,
            b: 0,
            a: 255,
        },
        source: BorderSource::Row,
    };

    let result = BorderInfo::resolve_conflict(&cell, &row);
    assert!(result.is_some());
    let border = result.unwrap();
    assert_eq!(border.width, 3.0);
    assert_eq!(border.style, BorderStyle::Solid);
    assert_eq!(border.source, BorderSource::Row);
}

// ==================== TableColumnInfo Tests ====================

#[test]
fn test_table_column_info_creation() {
    let col = TableColumnInfo {
        min_width: 50.0,
        max_width: 200.0,
        computed_width: Some(100.0),
    };

    assert_eq!(col.min_width, 50.0);
    assert_eq!(col.max_width, 200.0);
    assert_eq!(col.computed_width, Some(100.0));
}

#[test]
fn test_table_column_info_width_constraints() {
    // Test that we can create column info with various constraints
    let cols = vec![
        TableColumnInfo {
            min_width: 0.0,
            max_width: f32::INFINITY,
            computed_width: None,
        },
        TableColumnInfo {
            min_width: 100.0,
            max_width: 100.0, // Fixed width
            computed_width: Some(100.0),
        },
        TableColumnInfo {
            min_width: 50.0,
            max_width: 300.0,
            computed_width: Some(150.0),
        },
    ];

    assert_eq!(cols[0].min_width, 0.0);
    assert!(cols[0].max_width.is_infinite());
    assert_eq!(cols[1].min_width, cols[1].max_width);
    assert!(cols[2].computed_width.unwrap() >= cols[2].min_width);
    assert!(cols[2].computed_width.unwrap() <= cols[2].max_width);
}

// ==================== TableCellInfo Tests ====================

#[test]
fn test_table_cell_info_single_cell() {
    let cell = TableCellInfo {
        node_index: 0,
        column: 0,
        colspan: 1,
        row: 0,
        rowspan: 1,
    };

    assert_eq!(cell.node_index, 0);
    assert_eq!(cell.column, 0);
    assert_eq!(cell.colspan, 1);
    assert_eq!(cell.row, 0);
    assert_eq!(cell.rowspan, 1);
}

#[test]
fn test_table_cell_info_colspan() {
    let cell = TableCellInfo {
        node_index: 0,
        column: 1,
        colspan: 3, // Spans columns 1, 2, 3
        row: 0,
        rowspan: 1,
    };

    assert_eq!(cell.column, 1);
    assert_eq!(cell.colspan, 3);
    // Cell occupies columns 1, 2, 3
    assert_eq!(cell.column + cell.colspan, 4);
}

#[test]
fn test_table_cell_info_rowspan() {
    let cell = TableCellInfo {
        node_index: 0,
        column: 0,
        colspan: 1,
        row: 2,
        rowspan: 4, // Spans rows 2, 3, 4, 5
    };

    assert_eq!(cell.row, 2);
    assert_eq!(cell.rowspan, 4);
    // Cell occupies rows 2, 3, 4, 5
    assert_eq!(cell.row + cell.rowspan, 6);
}

#[test]
fn test_table_cell_info_complex_span() {
    // Test a cell that spans both columns and rows
    let cell = TableCellInfo {
        node_index: 5,
        column: 1,
        colspan: 2,
        row: 0,
        rowspan: 3,
    };

    // Cell at (1,0) spanning to (2,2) inclusive
    assert_eq!(cell.column, 1);
    assert_eq!(cell.colspan, 2);
    assert_eq!(cell.row, 0);
    assert_eq!(cell.rowspan, 3);
}

// ==================== Border Source Ordering Tests ====================

#[test]
fn test_border_source_ordering() {
    // Test that BorderSource enum has correct ordering
    assert!(BorderSource::Table < BorderSource::ColumnGroup);
    assert!(BorderSource::ColumnGroup < BorderSource::Column);
    assert!(BorderSource::Column < BorderSource::RowGroup);
    assert!(BorderSource::RowGroup < BorderSource::Row);
    assert!(BorderSource::Row < BorderSource::Cell);
}

#[test]
fn test_border_source_as_priority() {
    // Verify the priority values
    assert_eq!(BorderSource::Table as u8, 0);
    assert_eq!(BorderSource::ColumnGroup as u8, 1);
    assert_eq!(BorderSource::Column as u8, 2);
    assert_eq!(BorderSource::RowGroup as u8, 3);
    assert_eq!(BorderSource::Row as u8, 4);
    assert_eq!(BorderSource::Cell as u8, 5);
}

// ==================== Border Style Priority Tests ====================

#[test]
fn test_border_style_priority_full_hierarchy() {
    // Test full style hierarchy: double > solid > dashed > dotted > ridge > outset > groove > inset
    let styles = vec![
        (
            BorderStyle::Double,
            ColorU {
                r: 1,
                g: 0,
                b: 0,
                a: 255,
            },
        ),
        (
            BorderStyle::Solid,
            ColorU {
                r: 2,
                g: 0,
                b: 0,
                a: 255,
            },
        ),
        (
            BorderStyle::Dashed,
            ColorU {
                r: 3,
                g: 0,
                b: 0,
                a: 255,
            },
        ),
        (
            BorderStyle::Dotted,
            ColorU {
                r: 4,
                g: 0,
                b: 0,
                a: 255,
            },
        ),
        (
            BorderStyle::Ridge,
            ColorU {
                r: 5,
                g: 0,
                b: 0,
                a: 255,
            },
        ),
        (
            BorderStyle::Outset,
            ColorU {
                r: 6,
                g: 0,
                b: 0,
                a: 255,
            },
        ),
        (
            BorderStyle::Groove,
            ColorU {
                r: 7,
                g: 0,
                b: 0,
                a: 255,
            },
        ),
        (
            BorderStyle::Inset,
            ColorU {
                r: 8,
                g: 0,
                b: 0,
                a: 255,
            },
        ),
    ];

    // Double should win against all others
    let double = BorderInfo {
        width: 2.0,
        style: BorderStyle::Double,
        color: ColorU {
            r: 1,
            g: 0,
            b: 0,
            a: 255,
        },
        source: BorderSource::Cell,
    };

    for (style, color) in styles.iter().skip(1) {
        let other = BorderInfo {
            width: 2.0,
            style: *style,
            color: *color,
            source: BorderSource::Cell,
        };
        let result = BorderInfo::resolve_conflict(&double, &other);
        assert!(result.is_some());
        let border = result.unwrap();
        assert_eq!(border.style, BorderStyle::Double);
        assert_eq!(border.color.r, 1);
    }
}

#[test]
fn test_border_style_ridge_vs_groove() {
    // ridge > groove
    let ridge = BorderInfo {
        width: 2.0,
        style: BorderStyle::Ridge,
        color: ColorU {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
        source: BorderSource::Cell,
    };

    let groove = BorderInfo {
        width: 2.0,
        style: BorderStyle::Groove,
        color: ColorU {
            r: 0,
            g: 255,
            b: 0,
            a: 255,
        },
        source: BorderSource::Cell,
    };

    let result = BorderInfo::resolve_conflict(&ridge, &groove);
    assert!(result.is_some());
    let border = result.unwrap();
    assert_eq!(border.style, BorderStyle::Ridge);
}

#[test]
fn test_border_style_outset_vs_inset() {
    // outset > inset
    let outset = BorderInfo {
        width: 2.0,
        style: BorderStyle::Outset,
        color: ColorU {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
        source: BorderSource::Cell,
    };

    let inset = BorderInfo {
        width: 2.0,
        style: BorderStyle::Inset,
        color: ColorU {
            r: 0,
            g: 255,
            b: 0,
            a: 255,
        },
        source: BorderSource::Cell,
    };

    let result = BorderInfo::resolve_conflict(&outset, &inset);
    assert!(result.is_some());
    let border = result.unwrap();
    assert_eq!(border.style, BorderStyle::Outset);
}

// ==================== Edge Cases ====================

#[test]
fn test_border_conflict_same_border() {
    // Identical borders should return the same border
    let border = BorderInfo {
        width: 2.0,
        style: BorderStyle::Solid,
        color: ColorU {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
        source: BorderSource::Cell,
    };

    let result = BorderInfo::resolve_conflict(&border, &border);
    assert!(result.is_some());
    let res = result.unwrap();
    assert_eq!(res.width, 2.0);
    assert_eq!(res.style, BorderStyle::Solid);
    assert_eq!(res.color.r, 255);
    assert_eq!(res.source, BorderSource::Cell);
}

#[test]
fn test_border_conflict_zero_width() {
    // Zero-width borders
    let zero = BorderInfo {
        width: 0.0,
        style: BorderStyle::Solid,
        color: ColorU {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
        source: BorderSource::Cell,
    };

    let normal = BorderInfo {
        width: 2.0,
        style: BorderStyle::Solid,
        color: ColorU {
            r: 0,
            g: 255,
            b: 0,
            a: 255,
        },
        source: BorderSource::Cell,
    };

    let result = BorderInfo::resolve_conflict(&zero, &normal);
    assert!(result.is_some());
    let border = result.unwrap();
    assert_eq!(border.width, 2.0);
}

#[test]
fn test_border_conflict_very_thick_border() {
    // Very thick borders should still work
    let thin = BorderInfo {
        width: 1.0,
        style: BorderStyle::Solid,
        color: ColorU {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
        source: BorderSource::Cell,
    };

    let very_thick = BorderInfo {
        width: 100.0,
        style: BorderStyle::Solid,
        color: ColorU {
            r: 0,
            g: 255,
            b: 0,
            a: 255,
        },
        source: BorderSource::Cell,
    };

    let result = BorderInfo::resolve_conflict(&thin, &very_thick);
    assert!(result.is_some());
    let border = result.unwrap();
    assert_eq!(border.width, 100.0);
}

// ==================== Integration Scenarios ====================

#[test]
fn test_realistic_table_scenario_1() {
    // Scenario: Table has 1px border, Cell has 3px border
    // Width wins first, so 3px wins, but Cell also has higher priority source
    let table_border = BorderInfo {
        width: 1.0,
        style: BorderStyle::Solid,
        color: ColorU {
            r: 128,
            g: 128,
            b: 128,
            a: 255,
        }, // Gray
        source: BorderSource::Table,
    };

    let cell_border = BorderInfo {
        width: 3.0,
        style: BorderStyle::Solid,
        color: ColorU {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        }, // Red
        source: BorderSource::Cell,
    };

    let result = BorderInfo::resolve_conflict(&table_border, &cell_border);
    assert!(result.is_some());
    let border = result.unwrap();
    assert_eq!(border.width, 3.0);
    assert_eq!(border.color.r, 255); // Red
    assert_eq!(border.source, BorderSource::Cell);
}

#[test]
fn test_realistic_table_scenario_2() {
    // Scenario: Row has 2px solid border, Cell has 2px dashed border
    // Same width, solid > dashed, but cell has higher source priority
    // Width is same (2px), style priority: solid > dashed
    let row_border = BorderInfo {
        width: 2.0,
        style: BorderStyle::Solid,
        color: ColorU {
            r: 0,
            g: 0,
            b: 255,
            a: 255,
        }, // Blue
        source: BorderSource::Row,
    };

    let cell_border = BorderInfo {
        width: 2.0,
        style: BorderStyle::Dashed,
        color: ColorU {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        }, // Red
        source: BorderSource::Cell,
    };

    let result = BorderInfo::resolve_conflict(&row_border, &cell_border);
    assert!(result.is_some());
    let border = result.unwrap();
    // Same width, so style matters: solid > dashed
    assert_eq!(border.style, BorderStyle::Solid);
    assert_eq!(border.color.b, 255); // Blue (row color)
}

#[test]
fn test_realistic_table_scenario_3() {
    // Scenario: Hidden border on row, thick border on cell
    // Hidden should suppress everything
    let row_border = BorderInfo {
        width: 1.0,
        style: BorderStyle::Hidden,
        color: ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        },
        source: BorderSource::Row,
    };

    let cell_border = BorderInfo {
        width: 10.0,
        style: BorderStyle::Double,
        color: ColorU {
            r: 255,
            g: 0,
            b: 0,
            a: 255,
        },
        source: BorderSource::Cell,
    };

    let result = BorderInfo::resolve_conflict(&row_border, &cell_border);
    assert!(result.is_none()); // Hidden suppresses all borders
}

#[test]
fn test_table_cell_grid_layout() {
    // Simulate a 3x3 table grid
    let cells = vec![
        // Row 0
        TableCellInfo {
            node_index: 0,
            column: 0,
            colspan: 1,
            row: 0,
            rowspan: 1,
        },
        TableCellInfo {
            node_index: 1,
            column: 1,
            colspan: 1,
            row: 0,
            rowspan: 1,
        },
        TableCellInfo {
            node_index: 2,
            column: 2,
            colspan: 1,
            row: 0,
            rowspan: 1,
        },
        // Row 1
        TableCellInfo {
            node_index: 3,
            column: 0,
            colspan: 1,
            row: 1,
            rowspan: 1,
        },
        TableCellInfo {
            node_index: 4,
            column: 1,
            colspan: 1,
            row: 1,
            rowspan: 1,
        },
        TableCellInfo {
            node_index: 5,
            column: 2,
            colspan: 1,
            row: 1,
            rowspan: 1,
        },
        // Row 2
        TableCellInfo {
            node_index: 6,
            column: 0,
            colspan: 1,
            row: 2,
            rowspan: 1,
        },
        TableCellInfo {
            node_index: 7,
            column: 1,
            colspan: 1,
            row: 2,
            rowspan: 1,
        },
        TableCellInfo {
            node_index: 8,
            column: 2,
            colspan: 1,
            row: 2,
            rowspan: 1,
        },
    ];

    // Verify all cells are in correct positions
    for (i, cell) in cells.iter().enumerate() {
        let expected_row = i / 3;
        let expected_col = i % 3;
        assert_eq!(cell.row, expected_row);
        assert_eq!(cell.column, expected_col);
        assert_eq!(cell.node_index, i);
    }
}

#[test]
fn test_table_cell_grid_with_colspan() {
    // 2x2 table where first cell spans 2 columns
    let cells = vec![
        // Row 0: One cell spanning columns 0-1
        TableCellInfo {
            node_index: 0,
            column: 0,
            colspan: 2,
            row: 0,
            rowspan: 1,
        },
        // Row 1: Two normal cells
        TableCellInfo {
            node_index: 1,
            column: 0,
            colspan: 1,
            row: 1,
            rowspan: 1,
        },
        TableCellInfo {
            node_index: 2,
            column: 1,
            colspan: 1,
            row: 1,
            rowspan: 1,
        },
    ];

    // First cell spans columns 0 and 1
    assert_eq!(cells[0].column, 0);
    assert_eq!(cells[0].colspan, 2);

    // Second row has normal cells
    assert_eq!(cells[1].column, 0);
    assert_eq!(cells[2].column, 1);
}

#[test]
fn test_table_cell_grid_with_rowspan() {
    // 2x2 table where first cell spans 2 rows
    let cells = vec![
        // Column 0: One cell spanning rows 0-1
        TableCellInfo {
            node_index: 0,
            column: 0,
            colspan: 1,
            row: 0,
            rowspan: 2,
        },
        // Column 1: Two normal cells
        TableCellInfo {
            node_index: 1,
            column: 1,
            colspan: 1,
            row: 0,
            rowspan: 1,
        },
        TableCellInfo {
            node_index: 2,
            column: 1,
            colspan: 1,
            row: 1,
            rowspan: 1,
        },
    ];

    // First cell spans rows 0 and 1
    assert_eq!(cells[0].row, 0);
    assert_eq!(cells[0].rowspan, 2);

    // Column 1 has normal cells
    assert_eq!(cells[1].row, 0);
    assert_eq!(cells[2].row, 1);
}
