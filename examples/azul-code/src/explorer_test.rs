use super::*;

#[test]
fn the_tree_builds_the_rows_in_view_and_a_screen_either_side() {
    // 10,000 rows, a 440 px view (20 rows) at the top: 41 rows built.
    assert_eq!(window_of(10_000, 0.0, 440.0), (0, 41));
    // Scrolled to row 5,000: from a screen above to a screen below.
    let y = 5_000.0 * ROW_HEIGHT;
    let (first, end) = window_of(10_000, y, 440.0);
    assert_eq!((first, end), (4_980, 5_041));
    assert!(end - first <= 61, "never more than three screens");
    assert_eq!(window_of(3, 0.0, 440.0), (0, 3), "a short tree: all of it");
    assert_eq!(window_of(0, 0.0, 440.0), (0, 0));
    assert_eq!(window_of(10, 9_999.0, 440.0), (10, 10), "scrolled past the end: nothing");
}
