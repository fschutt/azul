//! The top-edge rule: a scroll moves the selection and the Preview, a choice holds them.

use super::*;

/// Rows of a mail-like list: `None` is a group header, `Some(n)` an item (the key is a number).
fn rows() -> Vec<Option<u32>> {
    vec![None, Some(30), Some(29), Some(28), None, Some(20), Some(19)]
}

#[test]
fn the_top_edge_is_the_first_visible_item_and_a_header_holds_none() {
    let rows: Vec<Option<u32>> = vec![None, Some(30), Some(29), None, Some(20)];
    let key = |first| top_item(&rows, first).map(|(_, key)| key);
    assert_eq!(key(0), Some(30), "the header is skipped");
    assert_eq!(key(1), Some(30));
    assert_eq!(key(2), Some(29));
    assert_eq!(key(3), Some(20), "the header at the top edge: the item under it");
    assert_eq!(key(5), None, "past the last row");
    assert_eq!(key(usize::MAX), None);
    assert_eq!(top_item::<u32>(&[], 0), None);
}

#[test]
fn a_choice_holds_the_preview_until_the_top_edge_has_really_moved() {
    // no choice holds it: it follows
    assert!(follows_top_edge(None, 0));
    assert!(follows_top_edge(None, 100));
    // a choice at top row 10: the list nudging itself to keep the row in view is not the reader's
    assert!(!follows_top_edge(Some(10), 10));
    assert!(!follows_top_edge(Some(10), 11));
    assert!(!follows_top_edge(Some(10), 9));
    // two rows or more, either way, is a scroll
    assert!(follows_top_edge(Some(10), 12));
    assert!(follows_top_edge(Some(10), 8));
    assert!(follows_top_edge(Some(10), usize::MAX));
    assert!(follows_top_edge(Some(usize::MAX), 0));
}

#[test]
fn scrolling_moves_the_selection_and_the_preview_to_the_item_at_the_top_edge() {
    let rows = rows();
    assert_eq!(follow(&rows, 2, None, Some(30)), Follow::Move { row: 2, key: 29 });
    assert_eq!(follow(&rows, 3, None, Some(29)), Follow::Move { row: 3, key: 28 });
}

#[test]
fn a_header_at_the_top_edge_hands_it_to_the_first_item_under_it() {
    let rows = rows();
    assert_eq!(follow(&rows, 4, None, Some(28)), Follow::Move { row: 5, key: 20 });
    assert_eq!(follow(&rows, 0, None, None), Follow::Move { row: 1, key: 30 });
}

#[test]
fn a_scroll_that_leaves_the_same_item_at_the_top_changes_nothing() {
    let rows = rows();
    assert_eq!(follow(&rows, 1, None, Some(30)), Follow::Keep);
    assert_eq!(follow(&rows, 0, None, Some(30)), Follow::Keep, "the header over it: the same item");
}

#[test]
fn a_scroll_past_the_last_item_changes_nothing() {
    let rows = rows();
    assert_eq!(follow(&rows, 7, None, Some(19)), Follow::Keep);
    assert_eq!(follow(&rows, usize::MAX, None, None), Follow::Keep);
    assert_eq!(follow::<u32>(&[], 0, None, None), Follow::Keep);
}

#[test]
fn a_choice_holds_the_selection_and_the_preview_until_the_list_is_scrolled_two_rows() {
    let rows = rows();
    // a choice at top row 1 shows item 29 (row 2); the list nudges itself by one row: still held
    assert_eq!(follow(&rows, 2, Some(1), Some(29)), Follow::Keep);
    assert_eq!(follow(&rows, 0, Some(1), Some(29)), Follow::Keep);
    // the reader scrolls on: the selection and the Preview follow again
    assert_eq!(follow(&rows, 3, Some(1), Some(29)), Follow::Move { row: 3, key: 28 });
    assert_eq!(follow(&rows, 5, Some(1), Some(29)), Follow::Move { row: 5, key: 20 });
}

#[test]
fn a_choice_of_a_row_in_view_makes_the_next_scroll_the_readers() {
    let rows = rows();
    let mut list = ListScroll::new();
    assert_eq!(list.scrolled(&rows, 0, 6, None), Follow::Move { row: 1, key: 30 }, "the first scroll follows");
    list.chose(2);
    assert!(!list.swallow, "a row in view needs no scroll");
    assert_eq!(list.pinned_top, Some(0));
    // the reader scrolls three rows: the choice is let go and the top edge decides
    assert_eq!(list.scrolled(&rows, 3, 7, Some(29)), Follow::Move { row: 3, key: 28 });
    assert_eq!(list.pinned_top, None);
}

#[test]
fn the_keys_past_the_edge_scroll_the_list_and_that_scroll_does_not_take_the_row_away() {
    let rows = rows();
    let mut list = ListScroll::new();
    let _ = list.scrolled(&rows, 0, 3, Some(30));
    // the arrow keys chose row 5, out of view: the list will scroll it in
    list.chose(5);
    assert!(list.swallow, "row 5 is past the rows in view 0..3");
    assert_eq!(list.scrolled(&rows, 3, 6, Some(20)), Follow::Keep, "the selection's own scroll");
    assert!(!list.swallow, "swallowed once");
    assert_eq!(list.pinned_top, Some(3), "the choice holds at the place the list scrolled to");
    // a nudge of one row is still the choice's
    assert_eq!(list.scrolled(&rows, 4, 7, Some(20)), Follow::Keep);
    // the reader scrolls on by two rows: the top edge decides again
    assert_eq!(list.scrolled(&rows, 5, 7, Some(29)), Follow::Move { row: 5, key: 20 });
    assert_eq!(list.pinned_top, None);
}

#[test]
fn a_list_nobody_has_scrolled_counts_every_row_as_in_view() {
    let mut list = ListScroll::new();
    list.chose(500);
    assert!(!list.swallow);
    list.chose(0);
    assert!(!list.swallow);
}

#[test]
fn a_choice_above_the_rows_in_view_is_the_selections_scroll_too() {
    let rows = rows();
    let mut list = ListScroll::new();
    let _ = list.scrolled(&rows, 3, 6, Some(28));
    list.chose(1);
    assert!(list.swallow);
    assert_eq!(list.scrolled(&rows, 1, 4, Some(30)), Follow::Keep);
}

#[test]
fn another_list_forgets_the_choice_and_the_rows_in_view() {
    let rows = rows();
    let mut list = ListScroll::new();
    let _ = list.scrolled(&rows, 3, 6, Some(28));
    list.chose(4);
    list.reset();
    assert_eq!(list, ListScroll::default());
    assert_eq!(list.pinned_top, None);
    assert_eq!(list.end, usize::MAX);
}
