use crate::{
    key::{
        check_path_key, check_path_key_on, check_path_prefix, folder_trail, last_segment,
        parent_prefix, safe_file_name, without_twins, System,
    },
    DriveError,
};

fn refused(key: &str) -> bool {
    matches!(check_path_key(key), Err(DriveError::InvalidKey { .. }))
}

#[test]
fn a_key_with_a_parent_segment_is_refused() {
    assert!(refused(".."));
    assert!(refused("../etc/passwd"));
    assert!(refused("mail/../../etc/passwd"));
    assert!(refused("mail/.."));
}

#[test]
fn a_key_with_a_dot_segment_is_refused() {
    assert!(refused("."));
    assert!(refused("./mail"));
    assert!(refused("mail/./inbox"));
}

#[test]
fn an_absolute_key_is_refused() {
    assert!(refused("/etc/passwd"));
    assert!(refused("\\\\server\\share"));
    assert!(refused("C:/Windows/win.ini"));
    // `c:evil` is relative to drive C's current folder: a path that leaves the root where a key
    // becomes a path on Windows, and a plain name on macOS and Linux.
    assert!(check_path_key_on("c:evil", System::Windows).is_err());
}

#[test]
fn a_key_with_a_backslash_or_nul_is_refused() {
    assert!(refused("mail\\..\\..\\secret"));
    assert!(refused("mail/a\0b"));
}

#[test]
fn an_empty_key_or_an_empty_segment_is_refused() {
    assert!(refused(""));
    assert!(refused("mail//inbox"));
    assert!(refused("mail/inbox/"));
}

#[test]
fn ordinary_keys_pass() {
    for key in [
        "mail/inbox/0001.eml",
        "a b/c.txt",
        "..hidden",
        "mail/.config",
        "notes...txt",
        "\u{00e4}rger/\u{1f600}.txt",
    ] {
        assert_eq!(check_path_key(key), Ok(()), "{key}");
    }
}

#[test]
fn a_folder_prefix_ends_with_the_delimiter() {
    assert_eq!(check_path_prefix(""), Ok(()));
    assert_eq!(check_path_prefix("mail/"), Ok(()));
    assert_eq!(check_path_prefix("mail/inbox/"), Ok(()));
    assert_eq!(check_path_prefix("mail/in"), Ok(()));
    assert!(check_path_prefix("../").is_err());
    assert!(check_path_prefix("mail/../").is_err());
    assert!(check_path_prefix("/").is_err());
}

#[test]
fn parent_prefix_walks_up_one_folder() {
    assert_eq!(parent_prefix("mail/inbox/"), "mail/");
    assert_eq!(parent_prefix("mail/"), "");
    assert_eq!(parent_prefix(""), "");
}

#[test]
fn last_segment_names_files_and_folders() {
    assert_eq!(last_segment("mail/inbox/0001.eml"), "0001.eml");
    assert_eq!(last_segment("mail/inbox/"), "inbox");
    assert_eq!(last_segment("readme.txt"), "readme.txt");
    assert_eq!(last_segment(""), "");
}

#[test]
fn the_folder_trail_lists_every_folder_down_to_the_prefix() {
    assert_eq!(folder_trail(""), Vec::<(String, String)>::new());
    assert_eq!(
        folder_trail("mail/inbox/"),
        vec![
            ("mail".to_string(), "mail/".to_string()),
            ("inbox".to_string(), "mail/inbox/".to_string()),
        ]
    );
}

#[test]
fn a_download_name_never_leaves_the_folder() {
    assert_eq!(
        safe_file_name("../../etc/passwd"),
        Some("passwd".to_string())
    );
    assert_eq!(
        safe_file_name("mail/inbox/0001.eml"),
        Some("0001.eml".to_string())
    );
    assert_eq!(safe_file_name("mail/inbox/"), Some("inbox".to_string()));
    assert_eq!(safe_file_name("mail/.."), None);
    assert_eq!(safe_file_name("mail/."), None);
    assert_eq!(safe_file_name(""), None);
    assert_eq!(safe_file_name("a/b\\c.txt"), Some("b_c.txt".to_string()));
    assert_eq!(safe_file_name("con:x?.txt"), Some("con_x_.txt".to_string()));
}

/// How APFS and NTFS compare names, for a test: case ignored (Unicode forms are the caller's).
fn lower(name: &str) -> String {
    name.to_lowercase()
}

#[test]
fn keys_a_disk_would_take_for_one_name_get_names_of_their_own_in_the_order_given() {
    let names = without_twins(
        &[
            "Report.txt",
            "report.txt",
            "REPORT.TXT",
            "notes/Report.txt",
            "other.md",
        ],
        &lower,
    );
    assert_eq!(
        names,
        [
            "Report.txt",
            "report (2).txt",
            "REPORT (3).TXT",
            "notes/Report.txt",
            "other.md"
        ]
    );
    // Keys that are different stay as they are.
    let same = ["a.txt", "b.txt", "dir/a.txt", ".env"];
    assert_eq!(without_twins(&same, &lower), same);
    // A name made for a twin is free: it is not another key's (the first number that is).
    let names = without_twins(&["a.txt", "A.txt", "a (2).txt", "A (2).txt"], &lower);
    let folds: std::collections::BTreeSet<String> = names.iter().map(|n| lower(n)).collect();
    assert_eq!(folds.len(), 4, "{names:?}");
    assert_eq!(names[0], "a.txt");
    assert_eq!(names[1], "A (2).txt");
    // No extension, a hidden file, a folder in the name: the number goes before the extension of
    // the last name only.
    let names = without_twins(
        &[
            "Makefile",
            "makefile",
            ".ENV",
            ".env",
            "v1.2/Doc.tar.gz",
            "v1.2/doc.tar.gz",
        ],
        &lower,
    );
    assert_eq!(
        names,
        [
            "Makefile",
            "makefile (2)",
            ".ENV",
            ".env (2)",
            "v1.2/Doc.tar.gz",
            "v1.2/doc.tar (2).gz"
        ]
    );
    // The same call gives the same names.
    let again = without_twins(&["Report.txt", "report.txt"], &lower);
    assert_eq!(again, without_twins(&["Report.txt", "report.txt"], &lower));
}
