//! The words the recogniser is told to favour: the identifiers of the page, the file, its folder, what was
//! said lately and the project's own names, within the limit the speech layer takes. No window, no azul.

use super::*;
use crate::symbols::{Caps, Index, Kind, RawDef, Scanned};

fn put(idx: &mut Index, file: &str, defs: Vec<RawDef>) {
    idx.put_scanned(file, Scanned { size: 100, lines: 100, defs, ..Scanned::default() });
}

fn index() -> Index {
    let mut idx = Index::new(Caps::default());
    put(
        &mut idx,
        "crates/css/src/error.rs",
        vec![
            RawDef::new("CssError", Kind::Struct, 10).lines(5).public(),
            RawDef::new("CssParseError", Kind::Enum, 30).lines(12).public(),
            RawDef::new("parse_css", Kind::Fn, 50).lines(6).public(),
            RawDef::new("error", Kind::Fn, 70),
            RawDef::new("ab", Kind::Fn, 80),
        ],
    );
    put(
        &mut idx,
        "crates/css/src/props.rs",
        vec![RawDef::new("PropertyCache", Kind::Struct, 5).public(), RawDef::new("hidden_helper", Kind::Fn, 20), RawDef::new("works_on_a_dog", Kind::Fn, 30).test()],
    );
    put(&mut idx, "crates/layout/src/widgets/data_table.rs", vec![RawDef::new("DataTableCell", Kind::Struct, 254).lines(9).public(), RawDef::new("scroll_manager", Kind::Fn, 300).public()]);
    put(&mut idx, "crates/layout/src/managers/scroll_state.rs", vec![RawDef::new("ScrollManager", Kind::Struct, 609).lines(123).public()]);
    put(&mut idx, "crates/css/Cargo.toml", vec![RawDef::new("azul-css", Kind::Package, 2), RawDef::new("serde", Kind::Dependency, 9)]);
    idx
}

fn ctx<'a>(page: &'a str, file: Option<&'a str>, recent: &'a [String]) -> VocabContext<'a> {
    VocabContext { page_text: page, file, recent }
}

#[test]
fn the_identifiers_of_the_page_come_first_then_the_file_then_what_was_said_then_the_folder_then_the_project() {
    let idx = index();
    let recent = vec!["ScrollManager".to_string()];
    let v = build(&idx, &ctx("fn show(e: CssParseError) { /* PropertyCache */ }", Some("crates/css/src/error.rs"), &recent), &VocabOptions { keywords: false, ..VocabOptions::default() });
    let pos = |name: &str| v.words.iter().position(|w| w == name).unwrap_or_else(|| panic!("{name} is not in {:?}", v.words));
    assert!(pos("CssParseError") < pos("CssError"), "the page before the file");
    assert!(pos("CssError") < pos("ScrollManager"), "the file before what was said");
    assert!(pos("ScrollManager") < pos("PropertyCache") || pos("PropertyCache") < 3, "{:?}", v.words);
    assert!(pos("PropertyCache") < pos("DataTableCell"), "the folder before the project: {:?}", v.words);
    assert!(v.from_page >= 2 && v.from_file >= 2 && v.from_recent == 1);
}

#[test]
fn an_everyday_word_a_too_short_name_and_a_name_in_tests_are_not_in_the_list_unless_the_page_has_it() {
    let idx = index();
    let v = build(&idx, &ctx("", Some("crates/css/src/props.rs"), &[]), &VocabOptions { keywords: false, ..VocabOptions::default() });
    for bad in ["error", "ab", "works_on_a_dog"] {
        assert!(!v.words.iter().any(|w| w == bad), "{bad} in {:?}", v.words);
    }
    // The page being reviewed has a test: the reviewer may talk about it.
    let v = build(&idx, &ctx("fn works_on_a_dog() {}", Some("crates/css/src/props.rs"), &[]), &VocabOptions { keywords: false, ..VocabOptions::default() });
    assert!(v.words.iter().any(|w| w == "works_on_a_dog"), "{:?}", v.words);
}

#[test]
fn a_name_is_given_as_it_is_written_once_whatever_its_other_spellings() {
    let idx = index();
    let v = build(&idx, &ctx("", None, &[]), &VocabOptions { keywords: false, ..VocabOptions::default() });
    let sm: Vec<&String> = v.words.iter().filter(|w| w.to_lowercase().replace('_', "") == "scrollmanager").collect();
    assert_eq!(sm, ["ScrollManager"], "{:?}", v.words);
    let mut lower: Vec<String> = v.words.iter().map(|w| w.to_lowercase()).collect();
    lower.sort();
    let before = lower.len();
    lower.dedup();
    assert_eq!(before, lower.len());
}

#[test]
fn the_list_has_at_most_the_words_the_speech_layer_takes_and_says_how_many_it_cut() {
    let mut idx = Index::new(Caps::default());
    let defs: Vec<RawDef> = (0..300).map(|i| RawDef::new(&format!("SomeLongName{i}Thing"), Kind::Struct, i + 1).public()).collect();
    put(&mut idx, "a.rs", defs);
    let v = build(&idx, &ctx("", Some("a.rs"), &[]), &VocabOptions { max_words: 50, ..VocabOptions::default() });
    assert_eq!(v.words.len(), 50);
    assert!(v.cut >= 250, "{}", v.cut);
    assert!(KEYWORDS.iter().all(|k| v.words.iter().any(|w| w == k)), "the keywords keep their place: {:?}", &v.words[v.words.len() - 12..]);
    let v = build(&idx, &ctx("", Some("a.rs"), &[]), &VocabOptions { max_words: 0, ..VocabOptions::default() });
    assert!(v.words.is_empty());
}

#[test]
fn the_keywords_of_the_code_are_there_once_and_last() {
    let idx = index();
    let v = build(&idx, &ctx("", None, &[]), &VocabOptions::default());
    let tail = &v.words[v.words.len() - KEYWORDS.len()..];
    assert_eq!(tail, KEYWORDS);
    assert_eq!(v.words.iter().filter(|w| *w == "enum").count(), 1);
    let none = build(&idx, &ctx("", None, &[]), &VocabOptions { keywords: false, ..VocabOptions::default() });
    assert!(!none.words.iter().any(|w| w == "enum"));
}

#[test]
fn the_names_of_the_packages_and_what_they_depend_on_are_in_the_list_as_written() {
    let idx = index();
    let v = build(&idx, &ctx("", None, &[]), &VocabOptions { keywords: false, ..VocabOptions::default() });
    assert!(v.words.iter().any(|w| w == "azul-css"), "{:?}", v.words);
    assert!(v.words.iter().any(|w| w == "serde"), "{:?}", v.words);
}

#[test]
fn a_name_longer_than_a_keyword_may_be_is_left_out() {
    let mut idx = Index::new(Caps::default());
    let long = format!("Very{}Name", "Long".repeat(30));
    put(&mut idx, "a.rs", vec![RawDef::new(&long, Kind::Struct, 1).public(), RawDef::new("ShortOne", Kind::Struct, 5).public()]);
    let v = build(&idx, &ctx("", Some("a.rs"), &[]), &VocabOptions { keywords: false, max_chars: 64, ..VocabOptions::default() });
    assert_eq!(v.words, ["ShortOne"]);
}

#[test]
fn the_same_input_gives_the_same_list_and_an_empty_index_gives_the_keywords() {
    let idx = index();
    let a = build(&idx, &ctx("CssError ScrollManager", Some("crates/css/src/error.rs"), &["PropertyCache".to_string()]), &VocabOptions::default());
    let b = build(&idx, &ctx("CssError ScrollManager", Some("crates/css/src/error.rs"), &["PropertyCache".to_string()]), &VocabOptions::default());
    assert_eq!(a, b);
    let empty = build(&Index::new(Caps::default()), &ctx("", None, &[]), &VocabOptions::default());
    assert_eq!(empty.words, KEYWORDS);
}

#[test]
fn hostile_pages_are_bounded() {
    let idx = index();
    let big = "word CssError_x ".repeat(500_000);
    let started = std::time::Instant::now();
    let v = build(&idx, &ctx(&big, Some("crates/css/src/error.rs"), &[]), &VocabOptions::default());
    assert!(v.words.len() <= 200);
    let _ = build(&idx, &ctx(&"x".repeat(2_000_000), None, &[]), &VocabOptions::default());
    let _ = build(&idx, &ctx("ü ß 日本語 😀 \u{0}", None, &["".to_string(), "x".repeat(100_000)]), &VocabOptions::default());
    assert!(started.elapsed() < std::time::Duration::from_secs(3), "{:?}", started.elapsed());
}

#[test]
fn a_page_full_of_names_leaves_room_for_the_file_the_folder_and_the_project() {
    let idx = index();
    let page: String = (0..300).map(|i| format!("ThingNumber{i}Holder ")).collect();
    let v = build(&idx, &ctx(&page, Some("crates/css/src/error.rs"), &[]), &VocabOptions { keywords: false, ..VocabOptions::default() });
    assert!(v.from_page <= 60, "{}", v.from_page);
    assert!(v.words.iter().any(|w| w == "CssError"), "the file's own names are in: {:?}", &v.words[..v.words.len().min(80)]);
    assert!(v.words.iter().any(|w| w == "DataTableCell"), "and the project's");
}

#[test]
fn a_name_with_underscores_on_the_page_is_taken_when_the_index_defines_it_and_not_for_its_shape_alone() {
    let idx = index();
    let v = build(&idx, &ctx("use some_module_path::{hidden_helper, wildcard_imports};", None, &[]), &VocabOptions { keywords: false, ..VocabOptions::default() });
    assert!(v.words.iter().any(|w| w == "hidden_helper"), "{:?}", v.words);
    assert!(!v.words.iter().any(|w| w == "wildcard_imports" || w == "some_module_path"), "{:?}", v.words);
}
