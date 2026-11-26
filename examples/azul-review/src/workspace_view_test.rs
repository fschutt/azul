//! The workspace's files as the whiteboard's jump asks for them: any file of the folder, read when asked, a big one by its
//! text. A `MemSource` stands for the disk.

use super::*;
use crate::symbols_source::MemSource;

fn view_of(files: &[(&str, Vec<u8>)]) -> (WorkspaceView, Arc<MemSource>) {
    let mut source = MemSource::new();
    for (path, bytes) in files {
        source.put(path, bytes.clone());
    }
    let source = Arc::new(source);
    (WorkspaceView::new(source.clone(), PathBuf::from("/work")), source)
}

fn text(lines: usize) -> Vec<u8> {
    (0..lines).flat_map(|i| format!("fn f{i}() {{\t}}\n").into_bytes()).collect()
}

#[test]
fn a_file_is_counted_and_its_lines_given_as_the_page_shows_them() {
    let (view, _) = view_of(&[("src/a.rs", text(100))]);
    assert_eq!(view.line_count("src/a.rs"), Some(100));
    assert_eq!(view.lines("src/a.rs", 1, 2), Some(vec!["fn f0() {    }".to_string(), "fn f1() {    }".to_string()]));
    assert_eq!(view.lines("src/a.rs", 100, 100).map(|l| l.len()), Some(1));
    assert_eq!(view.lines("src/a.rs", 99, 101), None, "shorter than that");
    assert_eq!(view.lines("src/a.rs", 0, 5), None, "lines count from 1");
    assert_eq!(view.lines("src/a.rs", 5, 4), None);
}

#[test]
fn a_file_that_is_not_there_or_is_no_text_is_none() {
    let (view, _) = view_of(&[("bin", b"\x7fELF\0\0".to_vec())]);
    assert_eq!(view.line_count("nothing.rs"), None);
    assert_eq!(view.file_hash("nothing.rs"), None);
    assert_eq!(view.lines("nothing.rs", 1, 1), None);
    assert_eq!(view.line_count("bin"), None);
    assert_eq!(view.line_count("../outside.rs"), None, "a path that leaves the workspace");
}

#[test]
fn the_hash_of_a_file_is_the_hash_of_its_lines_whether_they_are_held_or_made_from_its_text() {
    let small = text(50);
    let big_lines = code::WHOLE_LIMIT / 12 + 100;
    let big = text(big_lines);
    assert!(big.len() > code::WHOLE_LIMIT);
    let (view, _) = view_of(&[("small.rs", small.clone()), ("big.rs", big.clone())]);
    assert_eq!(
        view.file_hash("small.rs"),
        Some(crate::whiteboard::lines_hash(&code::lines_of(&small).expect("text")))
    );
    assert_eq!(
        view.file_hash("big.rs"),
        Some(crate::whiteboard::lines_hash(&code::lines_of(&big).expect("text"))),
        "one number for the same lines, however they are held"
    );
    assert_eq!(view.line_count("big.rs"), Some(u32::try_from(big_lines).expect("fits")));
    assert_eq!(view.lines("big.rs", 3, 3), Some(vec!["fn f2() {    }".to_string()]));
}

#[test]
fn a_file_is_read_once_for_the_questions_asked_of_it_in_a_row() {
    let (view, source) = view_of(&[("a.rs", text(10)), ("b.rs", text(10))]);
    view.line_count("a.rs");
    view.lines("a.rs", 1, 3);
    view.file_hash("a.rs");
    assert_eq!(source.reads(), vec!["a.rs".to_string()]);
    view.line_count("b.rs");
    view.line_count("a.rs");
    assert_eq!(source.reads(), vec!["a.rs".to_string(), "b.rs".to_string(), "a.rs".to_string()], "one file is kept");
}
