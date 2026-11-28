use super::*;

fn file(name: &str) -> Entry {
    Entry {
        key: name.to_string(),
        name: name.to_string(),
        is_folder: false,
        size: Some(1),
        modified: None,
        etag: None,
        known: true,
    }
}

fn folder(name: &str) -> Entry {
    Entry {
        key: format!("{name}/"),
        is_folder: true,
        size: None,
        ..file(name)
    }
}

/// The Share row is for a file of a drive on this computer, where the platform has a share sheet.
#[test]
fn the_share_row_is_for_a_local_file_where_the_platform_shares() {
    assert!(row_shown(true, &file("a.pdf"), false));
    assert!(!row_shown(false, &file("a.pdf"), false), "no share sheet: no row");
    assert!(!row_shown(true, &file("a.pdf"), true), "a cloud file has no path to hand over");
    assert!(!row_shown(true, &folder("src"), false), "a folder is not a file");
}
