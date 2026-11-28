use super::*;
use crate::model::Selection;

fn select(key: &str) -> Step {
    Step::Select(key.to_string())
}

/// Scrolling moves the selection to the row at the top edge, and the preview - which shows the
/// selected row - with it: both are the same row at every step of the scroll.
#[test]
fn scrolling_moves_the_selection_and_the_preview_together() {
    let mut follow = Follow::default();
    let mut selected: Vec<String> = vec![String::from("a")];
    for (offset, top) in [(40.0, "b"), (95.0, "c"), (210.0, "e")] {
        let now: Vec<&str> = selected.iter().map(String::as_str).collect();
        match follow.scrolled(offset, Some(top), &now) {
            Step::Select(key) => selected = vec![key],
            Step::Nothing => panic!("the list scrolled to {offset}: the selection stays at {now:?}"),
        }
        let now: Vec<&str> = selected.iter().map(String::as_str).collect();
        assert_eq!(now, [top], "the top-edge row is the selected row");
        assert_eq!(
            subject(now.first().copied(), Some("a")),
            Some(top),
            "the preview shows the selected row, not the first one the view has"
        );
    }
}

/// A scroll that moved the list while no row was selected selects the top row.
#[test]
fn a_scroll_selects_the_top_row_when_nothing_is() {
    let mut follow = Follow::default();
    assert_eq!(follow.scrolled(30.0, Some("b"), &[]), select("b"));
}

/// The row at the top edge that is the selection already is left as it is.
#[test]
fn the_selected_top_row_stays() {
    let mut follow = Follow::default();
    assert_eq!(follow.scrolled(30.0, Some("b"), &["b"]), Step::Nothing);
}

/// A tap chooses a row by itself and pins it: reports of the offset the list already had - the end
/// of the gesture that tapped, a scroll event that moved nothing - leave it where it is.
#[test]
fn a_tap_pins_its_row_until_the_list_scrolls() {
    let mut follow = Follow::default();
    // the list rests at 120 with "c" at its top edge, the selection followed it
    assert_eq!(follow.scrolled(120.0, Some("c"), &["a"]), select("c"));
    // a tap on "e": the selection is "e"; the list did not move
    assert_eq!(follow.scrolled(120.0, Some("c"), &["e"]), Step::Nothing);
    assert_eq!(follow.scrolled(120.2, Some("c"), &["e"]), Step::Nothing, "a hair's breadth is no scroll");
    assert_eq!(follow.scrolled(120.0, Some("c"), &["e"]), Step::Nothing);
}

/// Scrolling again releases the pin: the top-edge row is the selected row once more.
#[test]
fn scrolling_again_releases_the_pin() {
    let mut follow = Follow::default();
    assert_eq!(follow.scrolled(120.0, Some("c"), &["a"]), select("c"));
    assert_eq!(follow.scrolled(120.0, Some("c"), &["e"]), Step::Nothing, "pinned");
    assert_eq!(follow.scrolled(160.0, Some("d"), &["e"]), select("d"), "released");
}

/// A key moves the selection and scrolls the list to show it: that scroll is the app's own and the
/// selection stays where the key put it; the user's scroll after it follows again.
#[test]
fn a_scroll_the_app_made_itself_moves_nothing() {
    let mut follow = Follow::default();
    follow.app_scrolled(300.0);
    // the report of that scroll: the top-edge row is another one than the key's selection
    assert_eq!(follow.scrolled(300.0, Some("f"), &["k"]), Step::Nothing);
    // a second report at the same offset (the end of the scroll) is no scroll either
    assert_eq!(follow.scrolled(300.0, Some("f"), &["k"]), Step::Nothing);
    // the user scrolls on from there: the selection follows
    assert_eq!(follow.scrolled(340.0, Some("g"), &["k"]), select("g"));
}

/// If the list is somewhere else than the app put it, the user took it from there.
#[test]
fn a_list_somewhere_else_than_the_app_put_it_is_the_users() {
    let mut follow = Follow::default();
    follow.app_scrolled(300.0);
    assert_eq!(follow.scrolled(520.0, Some("p"), &["k"]), select("p"));
}

/// Check boxes: a selection of several rows is the user's choice, a scroll leaves it alone.
#[test]
fn a_selection_of_several_rows_is_left_alone() {
    let mut follow = Follow::default();
    assert_eq!(follow.scrolled(90.0, Some("c"), &["a", "b"]), Step::Nothing);
    assert_eq!(follow.scrolled(190.0, Some("e"), &["a", "b", "x"]), Step::Nothing);
    // the preview then shows the first row checked
    assert_eq!(subject(["a", "b"].first().copied(), Some("e")), Some("a"));
}

/// A folder with no row, and an offset that is no number, move nothing.
#[test]
fn no_row_and_no_number_move_nothing() {
    let mut follow = Follow::default();
    assert_eq!(follow.scrolled(50.0, None, &["a"]), Step::Nothing);
    assert_eq!(follow.scrolled(f32::NAN, Some("b"), &["a"]), Step::Nothing);
    assert_eq!(follow.scrolled(f32::INFINITY, Some("b"), &["a"]), Step::Nothing);
    follow.app_scrolled(f32::NAN);
    assert_eq!(follow, Follow { last: 50.0, own: None });
}

/// The selection a scroll leaves is exactly the top row, so Delete, Move to and Rename act on the
/// row the user sees highlighted; they take the selection as it is, as they always did.
#[test]
fn what_a_scroll_selects_is_one_row_for_the_actions_to_act_on() {
    let mut selection = Selection::default();
    selection.click("a");
    let mut follow = Follow::default();
    let now = selection.keys();
    let now: Vec<&str> = now.iter().map(String::as_str).collect();
    if let Step::Select(key) = follow.scrolled(75.0, Some("c"), &now) {
        selection.click(&key);
    }
    assert_eq!(selection.keys(), ["c"], "exactly the top row: nothing of the earlier choice is left");
    assert_eq!(selection.len(), 1);
}

/// The preview's own content is asked for when the scroll has settled, never for a row it passes.
#[test]
fn the_preview_loads_when_the_scroll_settles_not_while_it_runs() {
    assert!(!load_due(false));
    assert!(load_due(true));
}

/// A phone follows its scroll; a desktop's wheel leaves the selection alone.
#[test]
fn a_phone_follows_its_scroll_and_a_desktop_does_not() {
    assert!(follows_scroll(true));
    assert!(!follows_scroll(false));
}

/// With no row selected the preview shows the first row of the view; with none at all, nothing.
#[test]
fn the_preview_shows_the_first_row_until_a_row_is_selected() {
    assert_eq!(subject(None, Some("a")), Some("a"));
    assert_eq!(subject(Some("c"), Some("a")), Some("c"));
    assert_eq!(subject::<&str>(None, None), None);
}
