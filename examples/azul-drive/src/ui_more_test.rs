use super::*;

fn action_of(rows: &[SheetRow], label: &str) -> Action {
    rows.iter()
        .find(|r| r.label == label)
        .unwrap_or_else(|| panic!("no row {label}"))
        .action
        .clone()
}

/// What File held is all there: the menu runs from New tab to Close, the lines between its
/// groups.
#[test]
fn the_menu_has_what_file_held_in_its_order() {
    let rows = rows();
    let labels: Vec<&str> = rows
        .iter()
        .map(|row| match row {
            Row::Item { label, .. } | Row::Group { label, .. } => *label,
            Row::Separator => "-",
        })
        .collect();
    assert_eq!(
        labels,
        [
            "azdrive-loctab-new",
            "azdrive-file-new-window",
            "azdrive-file-terminal-here",
            "-",
            "azdrive-file-delete-history",
            "-",
            "azdrive-file-shortcuts",
            "azdrive-backstage-options",
            "azdrive-file-about",
            "-",
            "azdrive-ribbon-close",
        ]
    );
}

/// The phone's sheet is the same list flat: a group's ways one row each, no lines, each row an
/// icon and the action it ran as a menu entry.
#[test]
fn the_phones_sheet_is_the_list_flat() {
    let sheet = sheet_rows();
    let labels: Vec<&str> = sheet.iter().map(|r| r.label).collect();
    assert_eq!(
        labels,
        [
            "azdrive-loctab-new",
            "azdrive-file-new-window",
            "azdrive-file-terminal",
            "azdrive-file-azterm",
            "azdrive-file-recent-places",
            "azdrive-file-back-forward",
            "azdrive-file-everything",
            "azdrive-file-shortcuts",
            "azdrive-backstage-options",
            "azdrive-file-about",
            "azdrive-ribbon-close",
        ]
    );
    for row in &sheet {
        assert!(!row.icon.is_empty(), "{}", row.label);
    }
    assert_eq!(action_of(&sheet, "azdrive-loctab-new"), Action::NewTab);
    assert_eq!(action_of(&sheet, "azdrive-ribbon-close"), Action::CloseWindow);
    assert_eq!(action_of(&sheet, "azdrive-backstage-options"), Action::Options);
}

/// The terminal and the history each come in their ways: the system's terminal and AzTerm, and
/// the three things Delete history can forget.
#[test]
fn a_group_runs_each_of_its_ways() {
    let sheet = sheet_rows();
    assert_eq!(
        action_of(&sheet, "azdrive-file-terminal"),
        Action::OpenTerminal { azterm: false }
    );
    assert_eq!(
        action_of(&sheet, "azdrive-file-azterm"),
        Action::OpenTerminal { azterm: true }
    );
    assert_eq!(
        action_of(&sheet, "azdrive-file-recent-places"),
        Action::ClearHistory {
            recent: true,
            back_forward: false
        }
    );
    assert_eq!(
        action_of(&sheet, "azdrive-file-back-forward"),
        Action::ClearHistory {
            recent: false,
            back_forward: true
        }
    );
    assert_eq!(
        action_of(&sheet, "azdrive-file-everything"),
        Action::ClearHistory {
            recent: true,
            back_forward: true
        }
    );
}
