//! The lines of a file as the pages show them.

use super::*;

#[test]
fn text_is_read_as_the_pages_show_it_tabs_are_four_spaces_and_a_line_end_is_one_either_way() {
    let lines = lines_of(b"fn a() {\n\tlet x = 1;\r\n}\n").expect("text");
    assert_eq!(lines, vec!["fn a() {", "    let x = 1;", "}"]);
    assert_eq!(lines_of(b"").expect("empty"), Vec::<String>::new());
    assert_eq!(lines_of(b"one").expect("no end"), vec!["one"]);
    assert_eq!(lines_of(b"\n\n").expect("blank lines"), vec!["", ""]);
}

#[test]
fn text_that_is_not_utf8_is_read_with_its_bad_bytes_replaced_not_refused() {
    let lines = lines_of(b"caf\xe9 au lait\nok\n").expect("latin-1 text");
    assert_eq!(lines.len(), 2);
    assert!(lines[0].starts_with("caf") && lines[0].ends_with("au lait"), "{lines:?}");
}

#[test]
fn a_file_with_a_nul_byte_near_its_top_is_no_text() {
    assert_eq!(lines_of(b"\x7fELF\0\0\0\0"), Err(NotShown::Binary));
    let mut late = vec![b'a'; 9_000];
    late.push(0);
    assert!(lines_of(&late).is_ok(), "only the first 8 KiB are looked at");
}

#[test]
fn a_page_is_forty_six_lines_and_the_last_one_is_what_is_left() {
    let file = SourceFile {
        path: PathBuf::from("f.rs"),
        display: "f.rs".to_string(),
        lines: (0..100).map(|i| format!("l{i}")).collect(),
    };
    assert_eq!(file.page_count(), 3);
    let (first, lines) = file.page(0);
    assert_eq!((first, lines.len()), (1, 46));
    let (first, lines) = file.page(2);
    assert_eq!((first, lines.len()), (93, 8));
    let (first, lines) = file.page(9);
    assert_eq!((first, lines.len()), (415, 0), "past the end: no lines");
    assert_eq!(SourceFile { lines: Vec::new(), ..file }.page_count(), 1, "an empty file is a page");
}
