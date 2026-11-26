//! The helpers the screens share that are not DOM: shortening a path to its line.

use super::*;

#[test]
fn a_short_text_is_left_alone() {
    assert_eq!(shorten_middle("src/lib.rs", 30), "src/lib.rs");
    assert_eq!(shorten_middle("exactly ten", 11), "exactly ten");
    assert_eq!(shorten_middle("", 10), "");
}

#[test]
fn a_long_path_loses_its_middle_and_keeps_its_start_and_its_file_name() {
    let path = "examples/azul-review/src/solver3/layout_tree_builder.rs";
    let short = shorten_middle(path, 32);
    assert_eq!(short.chars().count(), 32, "{short}");
    assert!(short.starts_with("examples/"), "{short}");
    assert!(short.ends_with("builder.rs"), "{short}");
    assert!(short.contains('…'));
}

#[test]
fn shortening_counts_characters_not_bytes() {
    let short = shorten_middle("ünïcödé/dïrëctörý/fïlé_wïth_ümläüts.rs", 20);
    assert_eq!(short.chars().count(), 20, "{short}");
}

#[test]
fn a_limit_too_small_to_shorten_into_leaves_the_text() {
    assert_eq!(shorten_middle("a long text here", 3), "a long text here");
    assert_eq!(shorten_middle("a long text here", 0), "a long text here");
}
