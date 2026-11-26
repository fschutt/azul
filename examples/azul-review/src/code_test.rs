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
    let file = SourceFile::new(PathBuf::from("f.rs"), "f.rs".to_string(), (0..100).map(|i| format!("l{i}")).collect());
    assert_eq!(file.page_count(), 3);
    let (first, lines) = file.page(0);
    assert_eq!((first, lines.len()), (1, 46));
    let (first, lines) = file.page(2);
    assert_eq!((first, lines.len()), (93, 8));
    let (first, lines) = file.page(9);
    assert_eq!((first, lines.len()), (415, 0), "past the end: no lines");
    assert_eq!(SourceFile::new(PathBuf::from("e.rs"), "e.rs".to_string(), Vec::new()).page_count(), 1, "an empty file is a page");
}

// ---- a text too big to hold as lines: its bytes and where each line starts ----

/// Text that has every way a line can end and begin.
fn awkward() -> Vec<u8> {
    let mut text = Vec::new();
    text.extend_from_slice(b"fn a() {\n\tlet x = 1;\r\n\n\n    // caf\xc3\xa9 \xe2\x86\x92 end\n");
    text.extend_from_slice(b"bad \xff byte\n");
    text.extend_from_slice(b"no end");
    text
}

#[test]
fn a_big_text_gives_the_lines_the_same_bytes_give_when_they_are_held_as_lines() {
    for text in [awkward(), b"".to_vec(), b"\n".to_vec(), b"one".to_vec(), b"one\n".to_vec(), b"\n\ntwo\n\n".to_vec(), b"a\r\nb\r\n".to_vec()] {
        let held = lines_of(&text).expect("text");
        let big = BigText::new(text.clone());
        assert_eq!(big.line_count(), held.len(), "{:?}", String::from_utf8_lossy(&text));
        for (i, line) in held.iter().enumerate() {
            assert_eq!(&big.line(i), line, "line {i} of {:?}", String::from_utf8_lossy(&text));
        }
        assert_eq!(big.line(held.len()), "", "past the end: nothing");
    }
}

#[test]
fn the_pages_of_a_big_file_are_the_pages_of_the_same_file_held_as_lines() {
    let mut text = String::new();
    for i in 0..230 {
        text.push_str(&format!("line {i}\t{}\n", "x".repeat(i % 7)));
    }
    let held = SourceFile::new(PathBuf::from("a.rs"), "a.rs".to_string(), lines_of(text.as_bytes()).expect("text"));
    let big = SourceFile::big(PathBuf::from("a.rs"), "a.rs".to_string(), BigText::new(text.into_bytes()));
    assert_eq!(big.page_count(), held.page_count());
    assert_eq!(big.line_count(), 230);
    for page in 0..held.page_count() + 2 {
        let (a, a_lines) = held.page(page);
        let (b, b_lines) = big.page(page);
        assert_eq!((a, a_lines.as_ref()), (b, b_lines.as_ref()), "page {page}");
    }
    assert!(big.lines.is_empty(), "a big file holds no lines of its own");
}

#[test]
fn a_big_text_holds_its_bytes_and_four_bytes_a_line_and_no_more() {
    let text: Vec<u8> = (0..50_000).flat_map(|i| format!("pub fn step_{i}() {{}}\n").into_bytes()).collect();
    let size = text.len();
    let big = BigText::new(text);
    assert_eq!(big.line_count(), 50_000);
    assert!(big.heap_bytes() <= size + 4 * 50_001 + 64, "{} bytes for {size} of text", big.heap_bytes());
    assert_eq!(big.line(49_999), "pub fn step_49999() {}");
}

#[test]
fn a_text_larger_than_what_a_line_start_can_say_is_refused_and_not_wrapped() {
    assert!(BigText::try_new(vec![b'\n'; 16]).is_some());
    assert_eq!(BigText::MAX_BYTES, u32::MAX as usize);
}

#[test]
fn text_is_held_as_lines_up_to_the_limit_and_as_a_big_text_above_it() {
    let small = vec![b'a'; WHOLE_LIMIT];
    assert!(matches!(open_bytes(PathBuf::from("a"), "a".into(), small), Ok(f) if f.big.is_none()));
    let over = vec![b'a'; WHOLE_LIMIT + 1];
    assert!(matches!(open_bytes(PathBuf::from("a"), "a".into(), over), Ok(f) if f.big.is_some()));
    assert_eq!(open_bytes(PathBuf::from("a"), "a".into(), b"\x7fELF\0".to_vec()).err(), Some(NotShown::Binary));
}

#[test]
fn the_page_a_line_is_on_and_how_many_lines_a_page_has_without_making_them() {
    assert_eq!(SourceFile::page_of_line(0), 0, "before the first line is the first page");
    assert_eq!(SourceFile::page_of_line(1), 0);
    assert_eq!(SourceFile::page_of_line(46), 0, "the last line of the first page");
    assert_eq!(SourceFile::page_of_line(47), 1);
    assert_eq!(SourceFile::page_of_line(138), 2);
    let file = SourceFile::new(PathBuf::from("f.rs"), "f.rs".to_string(), (0..100).map(|i| format!("l{i}")).collect());
    assert_eq!((file.lines_on_page(0), file.lines_on_page(1), file.lines_on_page(2), file.lines_on_page(3)), (46, 46, 8, 0));
    // the same numbers as the lines the page makes
    for page in 0..4 {
        assert_eq!(file.lines_on_page(page), file.page(page).1.len(), "page {page}");
    }
}
