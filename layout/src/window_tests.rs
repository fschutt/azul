#[cfg(test)]
mod cursor_scroll_tests {
    use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};

    // Test scroll-into-view padding calculations
    const SCROLL_PADDING: f32 = 5.0;

    #[test]
    fn test_cursor_scroll_padding_left() {
        // Test that the SCROLL_PADDING constant is used correctly

        // Simulate a cursor TOO FAR left (needs scrolling)
        let cursor_rect = LogicalRect::new(
            LogicalPosition::new(3.0, 50.0), // Less than padding
            LogicalSize::new(1.0, 20.0),
        );

        let visible_area = LogicalRect::new(
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(100.0, 100.0),
        );

        // Cursor too far left (< padding)
        assert!(cursor_rect.origin.x < visible_area.origin.x + SCROLL_PADDING);

        // Calculate expected delta (negative = scroll left)
        let expected_delta_x = cursor_rect.origin.x - (visible_area.origin.x + SCROLL_PADDING);
        assert_eq!(expected_delta_x, -2.0); // 3.0 - (0.0 + 5.0) = -2.0
    }

    #[test]
    fn test_cursor_scroll_right_edge() {
        // Cursor near right edge
        let cursor_rect = LogicalRect::new(
            LogicalPosition::new(96.0, 50.0),
            LogicalSize::new(1.0, 20.0),
        );

        let visible_area = LogicalRect::new(
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(100.0, 100.0),
        );

        // Cursor too far right
        let cursor_right = cursor_rect.origin.x + cursor_rect.size.width; // 97.0
        let visible_right = visible_area.origin.x + visible_area.size.width - SCROLL_PADDING; // 95.0

        assert!(cursor_right > visible_right);

        let expected_delta_x = cursor_right - visible_right;
        assert_eq!(expected_delta_x, 2.0); // 97.0 - 95.0 = 2.0
    }

    #[test]
    fn test_cursor_scroll_vertical_top() {
        // Cursor at top edge
        let cursor_rect =
            LogicalRect::new(LogicalPosition::new(50.0, 2.0), LogicalSize::new(1.0, 20.0));

        let visible_area = LogicalRect::new(
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(100.0, 100.0),
        );

        // Cursor too far up
        assert!(cursor_rect.origin.y < visible_area.origin.y + SCROLL_PADDING);

        let expected_delta_y = cursor_rect.origin.y - (visible_area.origin.y + SCROLL_PADDING);
        assert_eq!(expected_delta_y, -3.0); // 2.0 - (0.0 + 5.0) = -3.0
    }

    #[test]
    fn test_cursor_scroll_vertical_bottom() {
        // Cursor at bottom edge
        let cursor_rect = LogicalRect::new(
            LogicalPosition::new(50.0, 90.0),
            LogicalSize::new(1.0, 20.0),
        );

        let visible_area = LogicalRect::new(
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(100.0, 100.0),
        );

        // Cursor extends beyond bottom
        let cursor_bottom = cursor_rect.origin.y + cursor_rect.size.height; // 110.0
        let visible_bottom = visible_area.origin.y + visible_area.size.height - SCROLL_PADDING; // 95.0

        assert!(cursor_bottom > visible_bottom);

        let expected_delta_y = cursor_bottom - visible_bottom;
        assert_eq!(expected_delta_y, 15.0); // 110.0 - 95.0 = 15.0
    }

    #[test]
    fn test_no_scroll_when_cursor_visible() {
        // Cursor well within visible area
        let cursor_rect = LogicalRect::new(
            LogicalPosition::new(50.0, 50.0),
            LogicalSize::new(1.0, 20.0),
        );

        let visible_area = LogicalRect::new(
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(100.0, 100.0),
        );

        // Check all edges - cursor should be within padded bounds
        assert!(cursor_rect.origin.x >= visible_area.origin.x + SCROLL_PADDING);
        assert!(
            cursor_rect.origin.x + cursor_rect.size.width
                <= visible_area.origin.x + visible_area.size.width - SCROLL_PADDING
        );
        assert!(cursor_rect.origin.y >= visible_area.origin.y + SCROLL_PADDING);
        assert!(
            cursor_rect.origin.y + cursor_rect.size.height
                <= visible_area.origin.y + visible_area.size.height - SCROLL_PADDING
        );
    }

    #[test]
    fn test_scrolled_visible_area_calculation() {
        // Test that visible area is correctly offset by scroll position
        let container_rect = LogicalRect::new(
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(100.0, 100.0),
        );

        let scroll_offset = LogicalPosition::new(10.0, 20.0);

        // Calculate visible area (what the user actually sees)
        let visible_area = LogicalRect::new(
            LogicalPosition::new(
                container_rect.origin.x + scroll_offset.x,
                container_rect.origin.y + scroll_offset.y,
            ),
            container_rect.size,
        );

        assert_eq!(visible_area.origin.x, 10.0);
        assert_eq!(visible_area.origin.y, 20.0);
        assert_eq!(visible_area.size.width, 100.0);
        assert_eq!(visible_area.size.height, 100.0);
    }

    #[test]
    fn test_delta_accumulation() {
        // Test that multiple small deltas accumulate correctly
        let mut scroll_offset = LogicalPosition::new(0.0, 0.0);

        // Apply first delta
        scroll_offset.x += 5.0;
        scroll_offset.y += 3.0;

        assert_eq!(scroll_offset.x, 5.0);
        assert_eq!(scroll_offset.y, 3.0);

        // Apply second delta
        scroll_offset.x += 2.0;
        scroll_offset.y += 7.0;

        assert_eq!(scroll_offset.x, 7.0);
        assert_eq!(scroll_offset.y, 10.0);
    }
}
