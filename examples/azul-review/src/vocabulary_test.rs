//! The words of the page being reviewed, for the speech recogniser's keyword biasing and for the agent's request: the
//! identifiers and file names the person is about to say, which a recogniser trained on speech does not know.

use super::*;

fn lines(text: &str) -> Vec<String> {
    text.lines().map(str::to_string).collect()
}

#[test]
fn the_identifiers_of_the_page_are_its_words_most_used_first() {
    let page = lines("pub fn parse_header(input: &str) -> Header {\n    let header = parse_header(input);\n    header\n}");
    let words = vocabulary(&page, "src/parser.rs", 50);
    assert!(words.contains(&"parse_header".to_string()), "{words:?}");
    assert!(words.contains(&"Header".to_string()) && words.contains(&"header".to_string()), "{words:?}");
    let place = |w: &str| words.iter().position(|x| x == w).expect(w);
    assert!(place("parse_header") < place("input") || place("header") < place("input"), "{words:?}");
}

#[test]
fn the_keywords_of_the_language_are_there_because_they_are_on_the_page() {
    let words = vocabulary(&lines("enum Mode { Fast }\nasync fn run() { work().await }"), "a.rs", 50);
    for keyword in ["enum", "async", "await"] {
        assert!(words.contains(&keyword.to_string()), "{keyword} in {words:?}");
    }
}

#[test]
fn the_file_name_and_its_folders_are_words_too() {
    let words = vocabulary(&lines("fn a() {}"), "src/azul_review/ink_layer.rs", 50);
    for word in ["ink_layer", "azul_review"] {
        assert!(words.contains(&word.to_string()), "{word} in {words:?}");
    }
}

#[test]
fn short_words_and_numbers_are_left_out() {
    let words = vocabulary(&lines("let x = 5; let ab = 12345; let abc = 1;"), "a.rs", 50);
    assert!(!words.iter().any(|w| w == "x" || w == "ab" || w == "5" || w == "12345"), "{words:?}");
    assert!(words.contains(&"abc".to_string()), "{words:?}");
}

#[test]
fn the_list_is_capped_and_says_nothing_twice() {
    let page: Vec<String> = (0..500).map(|i| format!("fn name{i}_{} () {{}}", "x".repeat(i % 7 + 3))).collect();
    let words = vocabulary(&page, "big.rs", 200);
    assert_eq!(words.len(), 200);
    let mut seen = std::collections::BTreeSet::new();
    assert!(words.iter().all(|w| seen.insert(w.clone())), "a word twice");
    assert_eq!(vocabulary(&page, "big.rs", 0), Vec::<String>::new());
}

#[test]
fn a_word_longer_than_a_word_is_left_out_and_the_same_page_gives_the_same_list() {
    let long = "q".repeat(300);
    let page = lines(&format!("fn {long}() {{}}\nfn short_one() {{}}"));
    let words = vocabulary(&page, "a.rs", 50);
    assert!(!words.iter().any(|w| w.len() > MAX_WORD_CHARS), "{words:?}");
    assert_eq!(words, vocabulary(&page, "a.rs", 50), "deterministic");
}

#[test]
fn a_page_with_no_words_has_only_its_file_name() {
    assert_eq!(vocabulary(&[], "", 50), Vec::<String>::new());
    assert_eq!(vocabulary(&[], "notes.md", 50), vec!["notes".to_string()]);
}
