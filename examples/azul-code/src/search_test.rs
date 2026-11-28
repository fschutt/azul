use super::*;

fn found(line: usize, start: usize, end: usize) -> Found {
    Found { line, start, end }
}

#[test]
fn find_all_reads_every_line_and_honours_case_and_whole_words() {
    let b = TextBuffer::from_text("let a = 1;\nlet A = 2;\nrental rent");
    let any = TextMatch::default();
    assert_eq!(find_all(&b, "a", any), vec![found(0, 4, 5), found(1, 4, 5), found(2, 4, 5)]);
    let case = TextMatch {
        match_case: true,
        ..any
    };
    assert_eq!(find_all(&b, "A", case), vec![found(1, 4, 5)]);
    let whole = TextMatch {
        whole_word: true,
        ..any
    };
    assert_eq!(find_all(&b, "rent", whole), vec![found(2, 7, 11)]);
    assert!(find_all(&b, "1;\nlet", any).is_empty(), "one line at a time");
    assert!(find_all(&b, "", any).is_empty());
}

#[test]
fn the_next_match_comes_after_the_caret_and_wraps() {
    let list = vec![found(0, 4, 5), found(1, 4, 5)];
    assert_eq!(next_after(&list, Pos::new(0, 0)), Some(0));
    assert_eq!(next_after(&list, Pos::new(0, 5)), Some(1));
    assert_eq!(next_after(&list, Pos::new(1, 5)), Some(0), "round past the end");
    assert_eq!(previous_before(&list, Pos::new(1, 4)), Some(0));
    assert_eq!(previous_before(&list, Pos::new(0, 0)), Some(1), "round past the start");
    assert_eq!(next_after(&[], Pos::new(0, 0)), None);
}

#[test]
fn replace_all_is_one_step_of_edits_last_first() {
    let mut b = TextBuffer::from_text("rent and Rent\nno rent");
    let edits = replace_all(&b, "rent", "lease", TextMatch::default());
    assert_eq!(edits.len(), 3);
    assert!(edits.windows(2).all(|w| w[0].start > w[1].start), "last in the text first");
    b.apply(&edits);
    assert_eq!(b.text(), "lease and lease\nno lease");
    b.undo();
    assert_eq!(b.text(), "rent and Rent\nno rent", "one undo step");
}

#[test]
fn a_long_lines_preview_is_a_window_around_the_match() {
    let line = format!("{}needle{}", "a".repeat(100), "b".repeat(400));
    let (preview, s, e) = preview_of(&line, 100, 106);
    assert!(preview.starts_with('\u{2026}') && preview.ends_with('\u{2026}'));
    assert_eq!(&preview[s..e], "needle");
    assert!(preview.chars().count() <= PREVIEW_CHARS + 2);
    let (short, s, e) = preview_of("  x = needle;", 6, 12);
    assert_eq!((short.as_str(), &"  x = needle;"[6..12]), ("x = needle;", "needle"));
    assert_eq!(&short[s..e], "needle");
}

#[test]
fn go_to_line_reads_a_line_and_a_column() {
    assert_eq!(go_to_line("3", 10), Some(Pos::new(2, 0)));
    assert_eq!(go_to_line(" 3:4 ", 10), Some(Pos::new(2, 3)));
    assert_eq!(go_to_line("99", 10), Some(Pos::new(9, 0)), "past the end: the last line");
    assert_eq!(go_to_line("0", 10), Some(Pos::new(0, 0)));
    assert_eq!(go_to_line("x", 10), None);
    assert_eq!(go_to_line("", 10), None);
}
