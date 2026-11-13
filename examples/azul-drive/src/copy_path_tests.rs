//! Copy path with nothing selected copies the open folder's path, as the design's ribbon
//! offers it (the command is not greyed while a folder is open).

use crate::actions::keys_to_copy;

#[test]
fn copy_path_without_a_selection_copies_the_open_folders_path() {
    assert_eq!(keys_to_copy(Vec::new(), "docs/projects/"), vec![String::from("docs/projects/")]);
    assert_eq!(
        keys_to_copy(vec![String::from("docs/a.txt")], "docs/"),
        vec![String::from("docs/a.txt")],
        "a selection is copied as it is"
    );
}
