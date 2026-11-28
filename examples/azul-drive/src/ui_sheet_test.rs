use super::*;

fn file(name: &str) -> Entry {
    Entry {
        key: name.to_string(),
        name: name.to_string(),
        is_folder: false,
        size: Some(10),
        modified: None,
        etag: None,
        known: true,
    }
}

fn folder(name: &str) -> Entry {
    Entry {
        key: format!("{name}/"),
        name: name.to_string(),
        is_folder: true,
        size: None,
        modified: None,
        etag: None,
        known: true,
    }
}

fn labels(rows: &[ItemRow]) -> Vec<&'static str> {
    rows.iter().map(|r| r.label).collect()
}

/// A file's sheet is the context menu's quick actions in its order, Delete last and alone in red.
#[test]
fn a_files_sheet_runs_from_preview_to_the_red_delete() {
    let rows = item_rows(&file("report.pdf"), false);
    assert_eq!(
        labels(&rows),
        [
            "azdrive-sheet-preview",
            "azdrive-menu-open",
            "azdrive-menu-rename",
            "azdrive-ribbon-move-to",
            "azdrive-menu-copy",
            "azdrive-menu-properties",
            "azdrive-sheet-trash",
        ]
    );
    let red: Vec<&str> = rows.iter().filter(|r| r.danger).map(|r| r.label).collect();
    assert_eq!(red, ["azdrive-sheet-trash"], "only deleting is red");
    assert_eq!(rows.last().map(|r| &r.act), Some(&ItemAct::Do(Action::Delete)));
    assert_eq!(rows[0].act, ItemAct::Preview);
}

/// A file of a cloud drive downloads where a local one opens; a folder opens on either.
#[test]
fn a_cloud_file_downloads_where_a_local_one_opens() {
    let cloud = item_rows(&file("a.txt"), true);
    assert_eq!(cloud[1].act, ItemAct::Do(Action::Download));
    assert_eq!(cloud[1].label, "azdrive-menu-download");
    let local = item_rows(&file("a.txt"), false);
    assert_eq!(local[1].act, ItemAct::Do(Action::Open));
    let folder = item_rows(&folder("src"), true);
    assert_eq!(folder[1].act, ItemAct::Do(Action::Open), "a folder is opened, never downloaded");
}

/// Every row names an icon (the sheet draws it first) and Move to goes through the dialog that
/// asks where (a menu would have no row to hang from once the sheet is closed).
#[test]
fn every_row_has_an_icon_and_move_asks_where() {
    for row in item_rows(&file("a"), false) {
        assert!(!row.icon.is_empty(), "{}", row.label);
    }
    let rows = item_rows(&file("a"), false);
    assert_eq!(
        rows[3].act,
        ItemAct::Do(Action::ChooseLocation(TransferKind::Move))
    );
}
