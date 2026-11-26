//! What is kept of the review in the data root: written, read back, tolerant of what it was not made to meet. No disk.

use super::*;
use crate::progress::{Book, MAX_PAGES};

fn book() -> Book {
    let mut book = Book::default();
    let lib = book.entry("src/lib.rs").expect("room");
    for p in [0, 1, 2, 7] {
        lib.seen.mark(p);
    }
    book.entry("README.md").expect("room").held = true;
    book.entry("nothing.rs").expect("room");
    book
}

#[test]
fn a_book_is_written_and_read_back_whole_without_the_files_nothing_is_known_of() {
    let text = to_json(&book(), "/work/proj");
    let back = from_json(text.as_bytes()).expect("what was written reads back");
    assert_eq!(back.len(), 2, "a file nothing is known of is not kept");
    assert_eq!(back.get("src/lib.rs").map(|e| e.seen.ranges()), Some(vec![(0, 2), (7, 7)]));
    assert!(back.get("README.md").is_some_and(|e| e.held && e.seen.count() == 0));
    assert!(back.get("nothing.rs").is_none());
}

#[test]
fn what_is_written_names_its_format_and_its_folder() {
    let text = to_json(&book(), "/work/proj");
    assert!(text.contains("\"format\":\"azreview-progress/1\""), "{text}");
    assert!(text.contains("\"root\":\"/work/proj\""), "{text}");
    assert!(text.contains("[[0,2],[7,7]]"), "the ranges, short: {text}");
}

#[test]
fn a_path_with_quotes_and_control_characters_comes_back_as_it_was() {
    let mut book = Book::default();
    book.entry("odd \"name\"\\\n\u{fc}.rs").expect("room").seen.mark(1);
    let back = from_json(to_json(&book, "/w").as_bytes()).expect("reads back");
    assert!(back.get("odd \"name\"\\\n\u{fc}.rs").is_some());
}

#[test]
fn the_name_of_the_file_is_made_of_the_folder_so_that_each_workspace_has_its_own() {
    let a = file_name("/work/proj");
    assert_eq!(a, file_name("/work/proj"));
    assert_ne!(a, file_name("/work/other"));
    assert!(a.starts_with("progress-") && a.ends_with(".json") && a.len() == "progress-".len() + 16 + ".json".len(), "{a}");
    assert!(a.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.'), "{a}");
}

#[test]
fn what_is_no_book_is_refused_with_a_sentence_and_a_later_version_is_not_misread() {
    assert!(from_json(b"not json at all").is_err());
    assert!(from_json(b"[1,2,3]").is_err() || from_json(b"[1,2,3]").is_ok_and(|b| b.is_empty()));
    let later = br#"{"format":"azreview-progress/2","files":[]}"#;
    assert!(from_json(later).is_err(), "a format this version does not read");
    let extra = br#"{"format":"azreview-progress/1","future":{"a":1},"files":[{"path":"a.rs","seen":[[0,1]],"colour":"teal"}]}"#;
    assert_eq!(from_json(extra).expect("extra keys are fine").len(), 1);
    let no_format = br#"{"files":[{"path":"a.rs","seen":[[3,3]]}]}"#;
    assert_eq!(from_json(no_format).expect("a file without a format number").len(), 1);
}

#[test]
fn an_entry_that_cannot_be_read_is_skipped_and_the_rest_kept() {
    let text = br#"{"format":"azreview-progress/1","files":[{"seen":[[0,1]]},{"path":"ok.rs","seen":[[0,1],"bad",[5],[6,"x"],[9,9]]},7,{"path":"","seen":[[0,0]]}]}"#;
    let back = from_json(text).expect("reads");
    assert_eq!(back.len(), 1, "the one with a path");
    assert_eq!(back.get("ok.rs").map(|e| e.seen.ranges()), Some(vec![(0, 1), (9, 9)]), "and the ranges that can be read");
}

#[test]
fn what_is_read_is_bounded() {
    let mut text = String::from(r#"{"format":"azreview-progress/1","files":["#);
    for i in 0..120 {
        if i > 0 {
            text.push(',');
        }
        text.push_str(&format!(r#"{{"path":"f{i}.rs","seen":[[0,0]]}}"#));
    }
    text.push_str("]}");
    assert_eq!(from_json_capped(text.as_bytes(), 50).expect("reads").len(), 50, "the cap, not a crash");
    assert_eq!(MAX_ENTRIES, crate::progress::MAX_FILES);

    let wild = format!(r#"{{"format":"azreview-progress/1","files":[{{"path":"a.rs","seen":[[0,{}]]}}]}}"#, MAX_PAGES + 50);
    let back = from_json(wild.as_bytes()).expect("reads");
    assert!(back.get("a.rs").is_some_and(|e| e.seen.count() <= MAX_PAGES), "no page past what a file can have");

    let mut many = String::from(r#"{"format":"azreview-progress/1","files":[{"path":"a.rs","seen":["#);
    for i in 0..(MAX_RANGES + 100) {
        if i > 0 {
            many.push(',');
        }
        many.push_str(&format!("[{},{}]", i * 3, i * 3));
    }
    many.push_str("]}]}");
    let back = from_json(many.as_bytes()).expect("reads");
    assert!(back.get("a.rs").is_some_and(|e| e.seen.ranges().len() <= MAX_RANGES));
}

#[test]
fn what_was_kept_and_what_was_seen_since_are_both_kept() {
    let mut now = Book::default();
    now.entry("a.rs").expect("room").seen.mark(5);
    let mut kept = Book::default();
    kept.entry("a.rs").expect("room").seen.mark(1);
    kept.entry("a.rs").expect("room").held = true;
    kept.entry("b.rs").expect("room").seen.mark(2);
    now.merge(kept);
    assert_eq!(now.get("a.rs").map(|e| (e.seen.ranges(), e.held)), Some((vec![(1, 1), (5, 5)], true)));
    assert!(now.get("b.rs").is_some_and(|e| e.seen.is_seen(2)));
}

#[test]
fn the_place_to_come_back_to_is_kept_with_the_progress() {
    use crate::progress::Back;
    let mut book = Book::default();
    book.entry("a.rs").expect("room").seen.mark(1);
    book.jumped_from(Back { file: "src/lib.rs".to_string(), page: 2, line: 93 });
    let text = to_json(&book, "/w");
    let back = from_json(text.as_bytes()).expect("reads");
    assert_eq!(back.back(), Some(&Back { file: "src/lib.rs".to_string(), page: 2, line: 93 }));
    assert!(back.get("a.rs").is_some(), "with the files");
    // a book with no place says none
    assert!(from_json(to_json(&Book::default(), "/w").as_bytes()).expect("reads").back().is_none());
}

#[test]
fn a_place_that_cannot_be_read_is_left_out_and_the_rest_kept() {
    let text = r#"{"format":"azreview-progress/1","return":{"file":"a.rs","page":"two"},"files":[{"path":"b.rs","seen":[[0,0]]}]}"#;
    let book = from_json(text.as_bytes()).expect("reads");
    assert!(book.back().is_none());
    assert!(book.get("b.rs").is_some());
    let old = r#"{"format":"azreview-progress/1","files":[]}"#;
    assert!(from_json(old.as_bytes()).expect("reads").back().is_none(), "a file from before the place");
}

#[test]
fn what_was_kept_and_what_is_now_keep_the_place_of_now_first() {
    use crate::progress::Back;
    let mut now = Book::default();
    let mut kept = Book::default();
    kept.jumped_from(Back { file: "old.rs".to_string(), page: 0, line: 1 });
    now.merge(kept.clone());
    assert_eq!(now.back().map(|b| b.file.as_str()), Some("old.rs"), "the kept place comes back with the window");
    let mut now = Book::default();
    now.jumped_from(Back { file: "new.rs".to_string(), page: 1, line: 47 });
    now.merge(kept);
    assert_eq!(now.back().map(|b| b.file.as_str()), Some("new.rs"), "a place set since is the place");
}
