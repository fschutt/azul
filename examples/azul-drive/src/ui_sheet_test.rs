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
    let rows = item_rows(&file("report.pdf"), false, false);
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
    let cloud = item_rows(&file("a.txt"), true, false);
    assert_eq!(cloud[1].act, ItemAct::Do(Action::Download));
    assert_eq!(cloud[1].label, "azdrive-menu-download");
    let local = item_rows(&file("a.txt"), false, false);
    assert_eq!(local[1].act, ItemAct::Do(Action::Open));
    let folder = item_rows(&folder("src"), true, false);
    assert_eq!(folder[1].act, ItemAct::Do(Action::Open), "a folder is opened, never downloaded");
}

/// Every row names an icon (the sheet draws it first) and Move to goes through the dialog that
/// asks where (a menu would have no row to hang from once the sheet is closed).
#[test]
fn every_row_has_an_icon_and_move_asks_where() {
    for row in item_rows(&file("a"), false, false) {
        assert!(!row.icon.is_empty(), "{}", row.label);
    }
    let rows = item_rows(&file("a"), false, false);
    assert_eq!(
        rows[3].act,
        ItemAct::Do(Action::ChooseLocation(TransferKind::Move))
    );
}

/// Where the platform has a share sheet a local file's sheet has a Share row after Open; a cloud file's, a
/// folder's and a platform's without a share sheet have none.
#[test]
fn a_local_file_has_a_share_row_after_open_where_the_platform_shares() {
    let with = item_rows(&file("a.pdf"), false, true);
    let labels = labels(&with);
    assert_eq!(labels[1], "azdrive-menu-open");
    assert_eq!(labels[2], "azdrive-sheet-share");
    assert_eq!(with[2].act, ItemAct::Share);
    assert!(!with[2].danger);
    assert!(!labels.iter().any(|l| *l == "azdrive-sheet-share") == false);
    assert_eq!(item_rows(&file("a.pdf"), true, true).len(), 7, "a cloud file: no Share");
    assert_eq!(item_rows(&folder("src"), false, true).len(), 7, "a folder: no Share");
    assert_eq!(item_rows(&file("a.pdf"), false, false).len(), 7, "no share sheet: no Share");
    assert_eq!(with.len(), 8);
}
