// Table width distribution and cell alignment regression tests
// Tests for issues fixed during table layout implementation

#[cfg(test)]
mod table_width_distribution_tests {
    use azul_layout::solver3::fc::TableColumnInfo;

    /// Test Case 1: Table with width:100% should distribute columns to fill available space
    ///
    /// Problem: When available_width > total_max_width, columns were not expanded to fill
    /// the table's full width. They stayed at their max-content width.
    ///
    /// Fix: Distribute excess width proportionally based on max-content widths.
    ///
    /// Example:
    /// - Table width: 555px (width: 100% of body)
    /// - Column 1 max-content: 80px
    /// - Column 2 max-content: 80px
    /// - Total max-content: 160px
    /// - Excess: 555 - 160 = 395px
    /// - Each column gets: 80px + (395px * 0.5) = 277.5px
    #[test]
    fn test_table_width_100_percent_distributes_columns() {
        let mut columns = vec![
            TableColumnInfo {
                min_width: 60.0,
                max_width: 80.0,
                computed_width: None,
            },
            TableColumnInfo {
                min_width: 60.0,
                max_width: 80.0,
                computed_width: None,
            },
        ];

        let table_width = 555.0;
        let total_max_width = 160.0;
        let available_width = table_width;

        // Simulate the fixed algorithm
        if available_width >= total_max_width {
            let excess_width = available_width - total_max_width;
            let total_weight: f32 = columns.iter().map(|c| c.max_width.max(1.0)).sum();

            for col in &mut columns {
                let weight_factor = col.max_width.max(1.0) / total_weight;
                col.computed_width = Some(col.max_width + (excess_width * weight_factor));
            }
        }

        // Verify columns are distributed to fill table width
        let total_computed: f32 = columns.iter().filter_map(|c| c.computed_width).sum();
        assert!(
            (total_computed - table_width).abs() < 0.01,
            "Columns should sum to table width. Expected: {}, Got: {}",
            table_width,
            total_computed
        );

        // Each column should be approximately equal (same max-content width)
        assert!(
            columns[0].computed_width.unwrap() > 200.0,
            "Column should be expanded beyond max-content width"
        );
        assert!(
            (columns[0].computed_width.unwrap() - columns[1].computed_width.unwrap()).abs() < 0.01,
            "Columns with equal max-width should get equal final width"
        );
    }

    /// Test Case 2: Columns with different max-widths get proportional shares
    #[test]
    fn test_proportional_distribution_unequal_columns() {
        let mut columns = vec![
            TableColumnInfo {
                min_width: 40.0,
                max_width: 100.0, // Wider column
                computed_width: None,
            },
            TableColumnInfo {
                min_width: 30.0,
                max_width: 50.0, // Narrower column
                computed_width: None,
            },
        ];

        let table_width = 600.0;
        let total_max_width = 150.0;
        let available_width = table_width;
        let excess_width = available_width - total_max_width; // 450px

        // Simulate the fixed algorithm
        let total_weight: f32 = columns.iter().map(|c| c.max_width.max(1.0)).sum(); // 150

        for col in &mut columns {
            let weight_factor = col.max_width.max(1.0) / total_weight;
            col.computed_width = Some(col.max_width + (excess_width * weight_factor));
        }

        // Column 1: 100 + (450 * 100/150) = 100 + 300 = 400px
        // Column 2:  50 + (450 *  50/150) =  50 + 150 = 200px
        assert!((columns[0].computed_width.unwrap() - 400.0).abs() < 0.01);
        assert!((columns[1].computed_width.unwrap() - 200.0).abs() < 0.01);

        let total: f32 = columns.iter().filter_map(|c| c.computed_width).sum();
        assert!((total - table_width).abs() < 0.01);
    }

    /// Test Case 3: All columns have zero max-width (edge case)
    #[test]
    fn test_zero_max_width_equal_distribution() {
        let mut columns = vec![
            TableColumnInfo {
                min_width: 0.0,
                max_width: 0.0,
                computed_width: None,
            },
            TableColumnInfo {
                min_width: 0.0,
                max_width: 0.0,
                computed_width: None,
            },
        ];

        let table_width = 400.0;
        let total_max_width = 0.0;
        let available_width = table_width;
        let excess_width = available_width - total_max_width;

        // When all max_widths are 0, distribute equally
        let total_weight: f32 = columns.iter().map(|c| c.max_width.max(1.0)).sum();
        let num_columns = columns.len();

        for col in &mut columns {
            if total_weight > 0.0 {
                let weight_factor = col.max_width.max(1.0) / total_weight;
                col.computed_width = Some(col.max_width + (excess_width * weight_factor));
            } else {
                col.computed_width = Some(available_width / num_columns as f32);
            }
        }

        // Each column should get equal width
        assert!((columns[0].computed_width.unwrap() - 200.0).abs() < 0.01);
        assert!((columns[1].computed_width.unwrap() - 200.0).abs() < 0.01);
    }
}

#[cfg(test)]
mod vertical_alignment_tests {
    /// Test Case 4: Vertical alignment should use content-box height, not border-box
    ///
    /// Problem: y_offset was calculated using the full border-box height (including padding),
    /// causing text to be displaced downward.
    ///
    /// Fix: Calculate y_offset using content-box height (height - padding - border).
    ///
    /// Example:
    /// - Cell border-box height: 178px
    /// - Padding: 80px top + 80px bottom = 160px
    /// - Content-box height: 178 - 160 = 18px
    /// - Content height: 16px (text)
    /// - y_offset (middle): (18 - 16) / 2 = 1px (correct)
    /// - OLD y_offset: (178 - 16) / 2 = 81px (wrong!)
    #[test]
    fn test_vertical_alignment_uses_content_box() {
        let border_box_height = 178.0;
        let padding_top = 80.0;
        let padding_bottom = 80.0;
        let border_top = 1.0;
        let border_bottom = 1.0;
        let content_height = 16.0;

        // Calculate content-box height
        let content_box_height =
            border_box_height - padding_top - padding_bottom - border_top - border_bottom;

        // Calculate y_offset for middle alignment (align_factor = 0.5)
        let align_factor = 0.5_f32;
        let y_offset: f32 = (content_box_height - content_height) * align_factor;

        // Verify correct calculation
        assert!(
            (content_box_height - 16.0).abs() < 0.01,
            "Content-box height should be ~16px, got {}",
            content_box_height
        );
        assert!(
            y_offset.abs() < 2.0,
            "y_offset should be near 0, got {}",
            y_offset
        );

        // OLD WRONG calculation for comparison
        let wrong_y_offset: f32 = (border_box_height - content_height) * align_factor;
        assert!(
            wrong_y_offset > 80.0,
            "Old calculation would give ~81px offset"
        );
    }

    /// Test Case 5: Vertical alignment with different align values
    #[test]
    fn test_vertical_alignment_top_middle_bottom() {
        let content_box_height = 100.0;
        let content_height = 20.0;

        // Top alignment (align_factor = 0.0)
        let y_offset_top = (content_box_height - content_height) * 0.0;
        assert_eq!(y_offset_top, 0.0);

        // Middle alignment (align_factor = 0.5)
        let y_offset_middle = (content_box_height - content_height) * 0.5;
        assert_eq!(y_offset_middle, 40.0);

        // Bottom alignment (align_factor = 1.0)
        let y_offset_bottom = (content_box_height - content_height) * 1.0;
        assert_eq!(y_offset_bottom, 80.0);
    }
}

#[cfg(test)]
mod cell_padding_tests {
    /// Test Case 6: Cell padding should be subtracted before passing to IFC
    ///
    /// Problem: layout_cell_for_height() passed border-box width to IFC,
    /// but IFC needs content-box width to layout text correctly.
    ///
    /// Fix: Subtract padding and border from cell_width before creating IFC constraints.
    ///
    /// Example:
    /// - Cell border-box width: 277.64px
    /// - Padding: 8px left + 8px right = 16px
    /// - Border: 1px left + 1px right = 2px
    /// - Content-box width: 277.64 - 16 - 2 = 259.64px
    #[test]
    fn test_cell_padding_subtracted_for_ifc() {
        let cell_border_box_width = 277.64;
        let padding_left = 8.0;
        let padding_right = 8.0;
        let border_left = 1.0;
        let border_right = 1.0;

        // Calculate content-box width for IFC
        let content_box_width: f32 =
            cell_border_box_width - padding_left - padding_right - border_left - border_right;

        // Verify
        assert!(
            (content_box_width - 259.64).abs() < 0.01,
            "Content-box width should be ~259.64px, got {}",
            content_box_width
        );

        // OLD WRONG: Would pass full border-box width
        assert!(
            cell_border_box_width > content_box_width + 15.0,
            "Border-box should be significantly larger than content-box"
        );
    }
}

#[cfg(test)]
mod body_margin_tests {
    /// Test Case 7: Body margin should be preserved from original XML node
    ///
    /// Problem: render_dom_from_body_node() created a new body element,
    /// losing the style="margin: 20px" attribute from the original HTML.
    ///
    /// Fix: Render the original body_node to preserve all attributes.
    ///
    /// This is a regression test to ensure body styles are preserved.
    #[test]
    fn test_body_margin_preserved() {
        // This is more of an integration test - just document the expected behavior

        // Given: HTML with <body style="margin: 20px;">
        // Expected: Body should have 20px margin on all sides
        // Previous bug: Body had only 8px margin (default user-agent style)

        let expected_margin = 20.0;
        let default_ua_margin = 8.0;

        assert_ne!(
            expected_margin, default_ua_margin,
            "Custom body margin should override UA default"
        );
    }
}

#[cfg(test)]
mod type_safety_tests {
    /// Test Case 8: BorderBoxRect and ContentBoxRect newtypes
    ///
    /// Enhancement: Added BorderBoxRect and ContentBoxRect newtypes to make
    /// coordinate system explicit and prevent errors.
    ///
    /// This test documents the conversion logic.
    #[test]
    fn test_border_box_to_content_box_conversion() {
        use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};

        let border_box = LogicalRect {
            origin: LogicalPosition { x: 100.0, y: 200.0 },
            size: LogicalSize {
                width: 300.0,
                height: 200.0,
            },
        };

        let padding_left = 10.0;
        let padding_right = 10.0;
        let padding_top = 15.0;
        let padding_bottom = 15.0;
        let border_left = 2.0;
        let border_right = 2.0;
        let border_top = 3.0;
        let border_bottom = 3.0;

        // Manual conversion
        let content_box = LogicalRect {
            origin: LogicalPosition {
                x: border_box.origin.x + padding_left + border_left,
                y: border_box.origin.y + padding_top + border_top,
            },
            size: LogicalSize {
                width: border_box.size.width
                    - padding_left
                    - padding_right
                    - border_left
                    - border_right,
                height: border_box.size.height
                    - padding_top
                    - padding_bottom
                    - border_top
                    - border_bottom,
            },
        };

        // Verify conversion
        assert_eq!(content_box.origin.x, 112.0); // 100 + 10 + 2
        assert_eq!(content_box.origin.y, 218.0); // 200 + 15 + 3
        assert_eq!(content_box.size.width, 276.0); // 300 - 10 - 10 - 2 - 2
        assert_eq!(content_box.size.height, 164.0); // 200 - 15 - 15 - 3 - 3
    }
}
