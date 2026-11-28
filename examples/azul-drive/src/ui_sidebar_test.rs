use std::collections::{HashMap, HashSet};

use azul_appkit::l10n::t_label;

use super::*;
use crate::model::Pinned;

fn standard() -> Vec<Favorite> {
    vec![
        Favorite {
            name: "Documents".into(),
            prefix: "Documents/".into(),
            icon: "description",
        },
        Favorite {
            name: "Pictures".into(),
            prefix: "Pictures/".into(),
            icon: "image",
        },
    ]
}

fn drives() -> Vec<DriveRow<'static>> {
    vec![
        DriveRow {
            id: "home",
            name: "Home",
            icon: "home",
            local: true,
        },
        DriveRow {
            id: "bucket-1",
            name: "Bucket",
            icon: "cloud",
            local: false,
        },
    ]
}

/// The rows of a source list with every section open, the Home drive open on one folder and the
/// pins given.
fn rows_with(pins: &[Pinned], place_open: bool) -> Vec<Row> {
    let standard = standard();
    let mut expanded = HashSet::new();
    let mut loaded = HashMap::new();
    if place_open {
        expanded.insert(("home".to_string(), String::new()));
        loaded.insert(
            ("home".to_string(), String::new()),
            vec!["Code/".to_string(), "Empty/".to_string()],
        );
        loaded.insert(("home".to_string(), "Empty/".to_string()), Vec::new());
    }
    let src = Sources {
        open: [true, true, true],
        standard: &standard,
        pins,
        drives: drives(),
        expanded: &expanded,
        loaded: &loaded,
    };
    rows(&src)
}

fn facts<'a>(place: &'a Place, busy: usize) -> Facts<'a> {
    Facts {
        place,
        cloud: vec![
            None,
            Some(("sync", String::from("Syncing"), busy)),
        ],
    }
}

/// The source list is three sections in their order, each with the rows under its title; a
/// row's tag is its place in the list the events resolve it in.
#[test]
fn the_rows_group_into_the_three_sections_with_their_places_as_tags() {
    let rows = rows_with(&[], false);
    let place = Place::QuickAccess;
    let sections = sections_of(&rows, &facts(&place, 0));
    let titles: Vec<&str> = sections.iter().map(|s| s.title).collect();
    assert_eq!(
        titles,
        ["azdrive-side-favorites", "azdrive-side-locations", "azdrive-side-cloud"]
    );
    let sizes: Vec<usize> = sections.iter().map(|s| s.rows.len()).collect();
    assert_eq!(sizes, [3, 2, 2], "Quick access + two folders; This PC + Home; Bucket + Add drive");
    for section in &sections {
        for row in &section.rows {
            assert_eq!(
                rows[row.tag].label, row.label,
                "the tag names the row in the list it came from"
            );
        }
    }
    assert_eq!(
        sections.iter().map(|s| s.section).collect::<Vec<_>>(),
        [Section::Favorites, Section::Locations, Section::Cloud]
    );
    assert!(sections.iter().all(|s| s.open), "every section is open");
}

/// The place the window shows is the selected row, and only it.
#[test]
fn the_open_place_is_the_selected_row() {
    let rows = rows_with(&[], false);
    let place = Place::folder("home", "Documents/");
    let sections = sections_of(&rows, &facts(&place, 0));
    let selected: Vec<&str> = sections
        .iter()
        .flat_map(|s| &s.rows)
        .filter(|r| r.selected)
        .map(|r| r.label.as_str())
        .collect();
    assert_eq!(selected, ["azdrive-standard-documents"]);
}

/// A drive and a folder that open carry their triangle (open or closed); a folder listed empty
/// and a place that never opens carry none.
#[test]
fn a_drive_and_its_folders_carry_the_triangle_a_leaf_does_not() {
    let rows = rows_with(&[], true);
    let place = Place::ThisPc;
    let sections = sections_of(&rows, &facts(&place, 0));
    let locations = &sections[1].rows;
    let by_label = |label: &str| locations.iter().find(|r| r.label == label).expect(label);
    assert_eq!(by_label("azdrive-this-pc").disclosure, None, "This PC does not open in the list");
    assert_eq!(by_label("Home").disclosure, Some(true), "Home is open");
    assert_eq!(by_label("Code").disclosure, Some(false), "a folder not listed yet may open");
    assert_eq!(by_label("Empty").disclosure, None, "a folder listed empty has no triangle");
    assert_eq!(by_label("Code").depth, 1);
}

/// Only the top level is the rail's: the folders under a drive stay out of it.
#[test]
fn the_rail_takes_the_top_level_only() {
    let rows = rows_with(&[], true);
    let place = Place::ThisPc;
    let sections = sections_of(&rows, &facts(&place, 0));
    let locations = &sections[1].rows;
    for row in locations {
        assert_eq!(row.in_rail, row.depth == 0, "{}", row.label);
    }
    assert!(locations.iter().any(|r| r.depth == 1), "the test has a folder under a drive");
}

/// A cloud drive says its state with a glyph at the row's end and, while transfers run, their
/// count as the badge; a drive on this computer has neither.
#[test]
fn a_cloud_drive_has_its_state_and_its_transfers_as_a_badge() {
    let rows = rows_with(&[], false);
    let place = Place::ThisPc;
    let busy = sections_of(&rows, &facts(&place, 3));
    let cloud = &busy[2].rows;
    let bucket = cloud.iter().find(|r| r.label == "Bucket").expect("the bucket");
    assert_eq!(bucket.status, Some(("sync", String::from("Syncing"))));
    assert_eq!(bucket.badge, "3");
    let quiet = sections_of(&rows, &facts(&place, 0));
    let bucket = quiet[2].rows.iter().find(|r| r.label == "Bucket").expect("the bucket");
    assert_eq!(bucket.badge, "", "no transfers, no count");
    assert!(bucket.status.is_some(), "the state stays");
    let home = quiet[1].rows.iter().find(|r| r.label == "Home").expect("home");
    assert_eq!((home.badge.as_str(), home.status.clone()), ("", None));
}

/// A folder takes dropped items and Quick access pins them; This PC, a section and "Add drive"
/// take none.
#[test]
fn folders_and_quick_access_take_drops() {
    let rows = rows_with(&[], false);
    let place = Place::ThisPc;
    let sections = sections_of(&rows, &facts(&place, 0));
    let takes = |label: &str| {
        sections
            .iter()
            .flat_map(|s| &s.rows)
            .find(|r| r.label == label)
            .map(|r| r.accepts_drops)
            .expect(label)
    };
    assert!(takes("azdrive-quick-access"));
    assert!(takes("azdrive-standard-documents"));
    assert!(takes("Home"));
    assert!(!takes("azdrive-this-pc"));
    assert!(!takes("azdrive-side-add-drive"));
}

/// A closed section keeps its title and shows no rows (the list leaves them out), and rows
/// before any title are no one's.
#[test]
fn a_closed_section_has_no_rows_and_stray_rows_belong_to_no_section() {
    let standard = standard();
    let expanded = HashSet::new();
    let loaded = HashMap::new();
    let src = Sources {
        open: [false, true, false],
        standard: &standard,
        pins: &[],
        drives: drives(),
        expanded: &expanded,
        loaded: &loaded,
    };
    let rows = rows(&src);
    let place = Place::ThisPc;
    let sections = sections_of(&rows, &facts(&place, 0));
    assert_eq!(sections.len(), 3);
    assert!(!sections[0].open && sections[0].rows.is_empty());
    assert!(sections[1].open && !sections[1].rows.is_empty());
    assert!(!sections[2].open && sections[2].rows.is_empty());
    // rows that come before any title belong to no section: they are dropped, not misplaced
    let stray = &rows[2..4];
    assert!(stray.iter().all(|r| !matches!(r.kind, RowKind::Section(_))));
    assert!(sections_of(stray, &facts(&place, 0)).is_empty());
}

/// The widget's section index is the order of [`Section`]; past the last there is none.
#[test]
fn a_section_title_of_the_widget_is_found_by_its_place() {
    assert_eq!(section_at(0), Some(Section::Favorites));
    assert_eq!(section_at(1), Some(Section::Locations));
    assert_eq!(section_at(2), Some(Section::Cloud));
    assert_eq!(section_at(3), None);
    assert_eq!(section_at(usize::MAX), None);
}

/// The settings keep the state the widget asked for, all three.
#[test]
fn the_widgets_states_are_the_settings_states() {
    assert_eq!(mode_of(SidebarState::Expanded), SidebarMode::Expanded);
    assert_eq!(mode_of(SidebarState::Rail), SidebarMode::Rail);
    assert_eq!(mode_of(SidebarState::Handle), SidebarMode::Handle);
}

/// A menu entry has the icon of what it does; removing a drive is the one shown in red.
#[test]
fn the_sheet_icons_say_what_an_entry_does() {
    assert_eq!(entry_icon(&SideAction::Go(Place::ThisPc)), "folder_open");
    assert_eq!(entry_icon(&SideAction::Unpin(0)), "push_pin");
    assert_eq!(entry_icon(&SideAction::Eject(String::from("x"))), "eject");
    assert_eq!(entry_icon(&SideAction::AddDrive), "more_horiz", "anything else is the plain dots");
    let label = |key: &str| {
        crate::l10n::in_english();
        t_label(key)
    };
    assert_eq!(label("azdrive-side-unpin"), "Remove from Favorites");
}
