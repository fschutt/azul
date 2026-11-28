use super::*;

fn english() {
    crate::l10n::in_english();
}

fn names(trail: &[(String, Place)]) -> Vec<&str> {
    trail.iter().map(|(name, _)| name.as_str()).collect()
}

/// A folder's trail starts at This PC, goes through its drive and every folder, the open folder
/// last; a crumb carries the place a click on it goes to.
#[test]
fn a_folders_trail_runs_from_this_pc_through_the_drive_to_the_open_folder() {
    english();
    let place = Place::folder("home", "Documents/Reports/");
    let trail = trail_of(&place, "Home");
    assert_eq!(names(&trail), ["This PC", "Home", "Documents", "Reports"]);
    assert_eq!(trail[0].1, Place::ThisPc);
    assert_eq!(trail[1].1, Place::folder("home", ""));
    assert_eq!(trail[2].1, Place::folder("home", "Documents/"));
    assert_eq!(trail[3].1, place, "the last crumb is the open place");
}

/// A drive's root has This PC over it; This PC and Quick access have no parents.
#[test]
fn a_drives_root_has_this_pc_over_it_and_the_overviews_have_none() {
    english();
    let root = trail_of(&Place::folder("home", ""), "Home");
    assert_eq!(names(&root), ["This PC", "Home"]);
    assert_eq!(names(&trail_of(&Place::ThisPc, "This PC")), ["This PC"]);
    assert_eq!(names(&trail_of(&Place::QuickAccess, "Quick access")), ["Quick access"]);
}

fn drives() -> Vec<(String, String)> {
    vec![("home".to_string(), "Home".to_string())]
}

const ITEMS: [(&str, &str, bool); 4] = [
    ("Documents", "Documents/", true),
    ("Downloads", "Downloads/", true),
    ("notes.txt", "notes.txt", false),
    ("Notes-old.txt", "Notes-old.txt", false),
];

/// Nothing typed closes the field and goes nowhere.
#[test]
fn an_empty_field_goes_nowhere() {
    assert_eq!(goal_of("", &drives(), &ITEMS), Goal::Stay);
    assert_eq!(goal_of("   ", &drives(), &ITEMS), Goal::Stay);
}

/// A path is read as the old path box read it: the drive by its name without regard to case, the
/// folders after it; "This PC" is the overview.
#[test]
fn a_typed_path_goes_to_its_place() {
    english();
    assert_eq!(
        goal_of("home/Documents", &drives(), &ITEMS),
        Goal::Place(Place::folder("home", "Documents/"))
    );
    assert_eq!(goal_of("This PC", &drives(), &ITEMS), Goal::Place(Place::ThisPc));
    assert_eq!(
        goal_of("Home", &drives(), &ITEMS),
        Goal::Place(Place::folder("home", "")),
        "a path wins over a name"
    );
}

/// A name of an item of the folder: the exact one without regard to case, a folder opens and a
/// file is selected.
#[test]
fn an_items_name_goes_to_the_item() {
    assert_eq!(
        goal_of("documents", &drives(), &ITEMS),
        Goal::Folder("Documents/".to_string())
    );
    assert_eq!(
        goal_of("NOTES.TXT", &drives(), &ITEMS),
        Goal::File("notes.txt".to_string()),
        "the exact name, though another holds it too"
    );
}

/// A part of a name goes to the one item that holds it; with several there is no one, and with
/// none there is nothing.
#[test]
fn a_part_of_a_name_goes_to_the_one_item_that_holds_it() {
    assert_eq!(
        goal_of("down", &drives(), &ITEMS),
        Goal::Folder("Downloads/".to_string())
    );
    assert_eq!(goal_of("do", &drives(), &ITEMS), Goal::Nowhere, "two folders hold it");
    assert_eq!(goal_of("zzz", &drives(), &ITEMS), Goal::Nowhere);
    assert_eq!(goal_of("a", &drives(), &[]), Goal::Nowhere, "an empty folder has no item");
}

/// The field reports Escape with the text it held and a lost keyboard with none: no text while
/// text was typed is a click elsewhere.
#[test]
fn a_report_without_text_while_text_was_typed_is_a_click_elsewhere() {
    assert!(cancel_is_blur("", "doc"));
    assert!(!cancel_is_blur("doc", "doc"), "Escape carries the text");
    assert!(!cancel_is_blur("", ""), "nothing was narrowed: nothing to keep");
    assert!(!cancel_is_blur("", "  "));
}
