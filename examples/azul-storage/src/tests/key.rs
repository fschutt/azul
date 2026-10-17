use crate::{
    key::{
        check_path_key, check_path_prefix, folder_trail, last_segment, parent_prefix,
        safe_file_name,
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
    assert!(refused("c:evil"));
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
