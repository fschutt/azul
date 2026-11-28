//! Which files of an app's source the key check reads: not the tests, in either spelling.

use std::fs;

use azul_storage::testing::TempDir;

use super::*;

const EN: &str = "azx-one = One\n";
const DE: &str = "azx-one = Eins\n";

fn tree(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new("l10n-check-files");
    for (name, text) in files {
        fs::write(dir.path().join(name), text).expect("a file of the source");
    }
    dir
}

#[test]
fn a_key_the_source_names_is_found() {
    let dir = tree(&[("lib.rs", "let a = \"azx-one\";\n"), ("ui.rs", "let b = \"azx-two\";\n")]);
    let problems = check(dir.path(), &["azx"], EN, DE);
    assert_eq!(problems, vec!["azx-two: not in en".to_string(), "azx-two: not in de".to_string()]);
}

#[test]
fn the_files_of_the_tests_are_not_read_in_either_spelling() {
    // a test names a half of a key (`starts_with("azx-")`) or a key that does not exist on purpose
    let dir = tree(&[
        ("lib.rs", "let a = \"azx-one\";\n"),
        ("flow_test.rs", "assert!(name.starts_with(\"azx-chip-\"));\n"),
        ("ui_tests.rs", "assert!(name.starts_with(\"azx-style-\"));\n"),
    ]);
    assert_eq!(check(dir.path(), &["azx"], EN, DE), Vec::<String>::new());
}

#[test]
fn an_inline_test_module_is_left_out_and_a_declared_test_file_is_not_the_end() {
    let text = "let a = \"azx-one\";\n#[cfg(test)]\n#[path = \"x_test.rs\"]\nmod x_test;\nlet b = \"azx-two\";\n#[cfg(test)]\nmod tests {\n    let c = \"azx-three\";\n}\n";
    let keys = keys_in_source(text, &["azx"]);
    assert_eq!(keys.into_iter().collect::<Vec<_>>(), vec!["azx-one".to_string(), "azx-two".to_string()]);
}
