//! The breadcrumb of a path deeper than the room.

use super::*;

#[test]
fn a_path_that_fits_shows_every_folder() {
    assert_eq!(crumb_plan(&["layout", "src"], 3), [(1, "layout".to_string()), (2, "src".to_string())]);
    assert!(crumb_plan(&[], 3).is_empty());
}

#[test]
fn a_deeper_path_keeps_the_last_folders_and_a_key_for_the_rest_that_goes_to_the_last_of_them() {
    let plan = crumb_plan(&["layout", "src", "solver3", "deep"], 2);
    assert_eq!(plan, [(2, "…".to_string()), (3, "solver3".to_string()), (4, "deep".to_string())]);
    // the key for the folders left out goes to the folder just before the first one shown
    let names = ["a", "b", "c", "d", "e"];
    let plan = crumb_plan(&names, 3);
    assert_eq!(plan[0], (2, "…".to_string()));
    assert_eq!(plan.len(), 4);
}

#[test]
fn a_room_of_nothing_still_shows_the_last_folder() {
    let plan = crumb_plan(&["a", "b", "c"], 0);
    assert_eq!(plan.last(), Some(&(3, "c".to_string())));
}
