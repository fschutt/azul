//! A folder in the navigation tree with no folder in it shows no expander, as in the design:
//! its parent's listing (on this computer) already says which of its folders are leaves.

use azul_storage::testing::TempDir;

use crate::jobs::has_subfolder;

#[test]
fn a_folder_with_only_files_is_a_leaf_and_one_with_a_folder_is_not() {
    let tmp = TempDir::new("tree-leaves");
    let leaf = tmp.path().join("leaf");
    std::fs::create_dir_all(&leaf).unwrap();
    std::fs::write(leaf.join("notes.txt"), b"words").unwrap();
    assert!(!has_subfolder(&leaf), "files only");
    let branch = tmp.path().join("branch");
    std::fs::create_dir_all(branch.join("inner")).unwrap();
    assert!(has_subfolder(&branch));
    let hidden = tmp.path().join("hidden");
    std::fs::create_dir_all(hidden.join(".git")).unwrap();
    assert!(has_subfolder(&hidden), "a hidden folder may show: the expander stays");
    assert!(!has_subfolder(&tmp.path().join("gone")), "a folder that is gone holds nothing");
}
