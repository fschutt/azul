//! The Preview follows the table's top edge, a choice holds it, the next scroll lets go.

use super::*;

#[test]
fn a_scroll_moves_the_selection_to_the_row_at_the_top_edge() {
    let mut follow = Follow::new();
    assert_eq!(follow.scrolled(5, Some(0)), Effect::Select(5));
    assert_eq!(follow.top, 5);
    assert_eq!(follow.scrolled(9, Some(5)), Effect::Select(9));
}

#[test]
fn the_row_already_shown_at_the_top_edge_changes_nothing() {
    let mut follow = Follow::new();
    assert_eq!(follow.scrolled(5, Some(5)), Effect::Keep);
    assert_eq!(follow.scrolled(0, Some(0)), Effect::Keep);
}

#[test]
fn a_choice_holds_the_selection_until_the_table_is_scrolled_on() {
    let mut follow = Follow::new();
    follow.chose(10);
    // a table that nudges itself to keep the chosen row in view moves its top edge by one: still the choice
    assert_eq!(follow.scrolled(11, Some(14)), Effect::Keep);
    assert_eq!(follow.pinned_top, Some(10), "the choice is held");
    // the reader scrolls on: the choice lets go and the Preview follows the edge again
    assert_eq!(follow.scrolled(15, Some(14)), Effect::Select(15));
    assert_eq!(follow.pinned_top, None);
    assert_eq!(follow.scrolled(16, Some(15)), Effect::Select(16));
}

#[test]
fn scrolling_back_up_past_the_slack_also_lets_go() {
    let mut follow = Follow::new();
    follow.chose(10);
    assert_eq!(follow.scrolled(8, Some(12)), Effect::Select(8));
    assert!(follows_top_edge(None, 3));
}

#[test]
fn the_slack_is_two_positions() {
    assert!(!follows_top_edge(Some(10), 10));
    assert!(!follows_top_edge(Some(10), 11));
    assert!(!follows_top_edge(Some(10), 9));
    assert!(follows_top_edge(Some(10), 12));
    assert!(follows_top_edge(Some(10), 8));
    assert!(follows_top_edge(None, 10));
}

#[test]
fn a_rebuilt_table_holds_no_choice_of_the_old_rows() {
    let mut follow = Follow::new();
    follow.chose(4);
    follow.scrolled(40, Some(4));
    follow.reset();
    assert_eq!(follow, Follow::new());
}

#[test]
fn the_first_build_selects_the_first_row_when_there_is_one() {
    assert_eq!(first_selection(12), Some(0));
    assert_eq!(first_selection(1), Some(0));
    assert_eq!(first_selection(0), None);
}
