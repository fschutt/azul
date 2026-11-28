#[test]
fn a_tree_rows_id_is_its_key_spelled_for_a_selector() {
    assert_eq!(super::tree_row_name("src/"), "__azcode_tree-src-");
    assert_eq!(super::tree_row_name("src/main.rs"), "__azcode_tree-src-main-rs");
    assert_eq!(super::tree_row_name("Cargo.toml"), "__azcode_tree-Cargo-toml");
    assert_eq!(super::tree_row_name("a b/ü.rs"), "__azcode_tree-a-b---rs");
}
