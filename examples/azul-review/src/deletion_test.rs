//! The words a slash takes away: the words of a line, which of them a slash crosses, and the ranges the page draws in the
//! background's colour. The page is the one the fixtures are drawn on (`stroke_fixtures`): line 1 at y 32, a line 15 px
//! high, a column 6.6 px wide, and the word of ten characters of line 12 at x 300 to 366.

use super::*;
use crate::{agent_review::PageGrid, model::InkPoint, slash::Line, stroke_fixtures as fx};

/// Where the code of the page is: column 4 at x 300, so that the word of ten characters starts where the fixtures put it.
fn grid() -> PageGrid {
    PageGrid { first_line: 1, origin_x: 300.0 - 4.0 * 6.6, origin_y: 32.0, line_height: 15.0, char_width: 6.6 }
}

/// 46 lines; lines 12 and 13 hold words at the places the fixtures are drawn.
fn page() -> Vec<String> {
    let mut lines: Vec<String> = (1..=46).map(|i| format!("fn f{i}() {{}}")).collect();
    lines[11] = "    abcdefghij klm = 1;".to_string();
    lines[12] = "    lmnopqrstu vwx = 2;".to_string();
    lines
}

/// The slash a stroke from (x0, y) to (x1, y) is.
fn slash(x0: f32, x1: f32, y: f32) -> Line {
    Line { x0, x1, y, duration_ms: 110 }
}

fn texts(hit: &[Deleted]) -> Vec<&str> {
    hit.iter().map(|d| d.text.as_str()).collect()
}

fn tokens_of(line: &str) -> Vec<&str> {
    tokens(line).iter().map(|t| &line[t.start..t.end]).collect()
}

// ---- the words of a line ----

#[test]
fn a_line_is_words_and_runs_of_signs_between_its_blanks() {
    assert_eq!(
        tokens_of("    let x = foo(bar, 1);"),
        vec!["let", "x", "=", "foo", "(", "bar", ",", "1", ");"]
    );
    assert_eq!(tokens_of("a::b->c"), vec!["a", "::", "b", "->", "c"]);
    assert_eq!(tokens_of("snake_case_name2 + 0x1F"), vec!["snake_case_name2", "+", "0x1F"]);
    assert!(tokens("").is_empty());
    assert!(tokens("      ").is_empty());
}

#[test]
fn a_word_knows_its_column_and_its_width_in_columns_not_in_bytes() {
    let t = tokens("    abcdefghij klm");
    assert_eq!((t[0].start, t[0].end, t[0].col, t[0].cols), (4, 14, 4, 10));
    assert_eq!((t[1].start, t[1].end, t[1].col, t[1].cols), (15, 18, 15, 3));

    // é is two bytes and one column, → three bytes and one column
    let line = "x = héllo→wörld";
    let t = tokens(line);
    let found: Vec<(&str, usize, usize)> = t.iter().map(|t| (&line[t.start..t.end], t.col, t.cols)).collect();
    assert_eq!(found, vec![("x", 0, 1), ("=", 2, 1), ("héllo", 4, 5), ("→", 9, 1), ("wörld", 10, 5)]);
    assert!(line.is_char_boundary(t[2].start) && line.is_char_boundary(t[2].end));
}

// ---- which words a slash crosses ----

#[test]
fn a_quick_slash_through_a_word_takes_that_word_and_says_where_it_was() {
    let f = fx::slash();
    let line = Line { x0: 296.0, x1: 372.0, y: 204.5, duration_ms: f.samples.last().map_or(0, |s| s.t_ms) };
    let hit = words_hit(&line, &grid(), &page());
    assert_eq!(hit, vec![Deleted { line: 12, start: 4, end: 14, text: "abcdefghij".to_string() }]);
}

#[test]
fn a_slash_across_several_words_takes_each_word_it_covers_by_sixty_percent() {
    // the long slash covers x 300 to 460: every word of the line, the signs too
    let hit = words_hit(&slash(300.0, 460.0, 205.0), &grid(), &page());
    assert_eq!(texts(&hit), vec!["abcdefghij", "klm", "=", "1", ";"]);
    assert!(hit.iter().all(|d| d.line == 12));

    // the word "klm" is x 372.6 to 392.4: 55 percent covered is left, 65 percent is taken
    assert_eq!(texts(&words_hit(&slash(330.0, 383.49, 205.0), &grid(), &page())), vec!["abcdefghij"]);
    assert_eq!(texts(&words_hit(&slash(330.0, 385.47, 205.0), &grid(), &page())), vec!["abcdefghij", "klm"]);
}

#[test]
fn the_word_under_the_middle_of_a_short_slash_is_taken_whatever_its_length() {
    // 26 px across a word of 66: 39 percent, but the middle of the stroke is in it
    let hit = words_hit(&slash(320.0, 346.0, 205.0), &grid(), &page());
    assert_eq!(texts(&hit), vec!["abcdefghij"]);
}

#[test]
fn a_slash_that_only_touches_the_next_word_leaves_it() {
    let hit = words_hit(&slash(300.0, 380.0, 205.0), &grid(), &page());
    assert_eq!(texts(&hit), vec!["abcdefghij"], "klm is 37 percent covered");
}

#[test]
fn a_slash_whose_middle_is_in_a_gap_takes_only_the_words_it_covers_enough() {
    // 360 to 384: the middle (372) is the blank between the words; "abcdefghij" is 9 percent covered, "klm" 58 percent
    assert!(words_hit(&slash(360.0, 384.0, 205.0), &grid(), &page()).is_empty());
    // 360 to 390: the middle (375) is in "klm"
    assert_eq!(texts(&words_hit(&slash(360.0, 390.0, 205.0), &grid(), &page())), vec!["klm"]);
}

#[test]
fn the_line_is_the_one_the_middle_of_the_stroke_lies_in() {
    let at = |y: f32| texts(&words_hit(&slash(300.0, 366.0, y), &grid(), &page())).join(",");
    assert_eq!(at(198.0), "abcdefghij", "line 12 spans y 197 to 212");
    assert_eq!(at(211.9), "abcdefghij");
    assert_eq!(at(212.0), "lmnopqrstu", "line 13 begins at 212");
    assert_eq!(at(226.0), "lmnopqrstu");
}

#[test]
fn nothing_is_taken_where_there_is_no_text() {
    // right of the text of the line, in the page's padding above the code, below the last line
    assert!(words_hit(&slash(500.0, 560.0, 205.0), &grid(), &page()).is_empty());
    assert!(words_hit(&slash(300.0, 366.0, 20.0), &grid(), &page()).is_empty());
    assert!(words_hit(&slash(300.0, 366.0, 32.0 + 46.0 * 15.0 + 1.0), &grid(), &page()).is_empty());
    // left of the code (the gutter with the numbers)
    assert!(words_hit(&slash(100.0, 150.0, 205.0), &grid(), &page()).is_empty());
    // a line with nothing on it
    let mut lines = page();
    lines[11] = String::new();
    assert!(words_hit(&slash(300.0, 366.0, 205.0), &grid(), &lines).is_empty());
}

#[test]
fn a_page_further_on_counts_its_lines_from_its_first() {
    let grid = PageGrid { first_line: 47, ..grid() };
    let hit = words_hit(&slash(300.0, 366.0, 204.5), &grid, &page());
    assert_eq!(hit[0].line, 47 + 11);
}

#[test]
fn what_is_taken_is_what_the_line_says_between_its_bytes() {
    let lines = page();
    for d in words_hit(&slash(300.0, 460.0, 205.0), &grid(), &lines) {
        assert_eq!(&lines[d.line - 1][d.start..d.end], d.text);
    }
}

#[test]
fn a_line_longer_than_the_page_is_wide_is_read_as_far_as_the_slash_reaches_and_no_further() {
    let mut lines = page();
    lines[11] = format!("{}{}", " ".repeat(4), "x".repeat(10_000));
    let hit = words_hit(&slash(300.0, 366.0, 205.0), &grid(), &lines);
    assert_eq!(hit.len(), 1);
    assert_eq!((hit[0].start, hit[0].end), (4, 10_004), "the word is the whole run, however long");
}

#[test]
fn numbers_that_are_no_numbers_take_nothing() {
    let bad = Line { x0: f32::NAN, x1: 366.0, y: 205.0, duration_ms: 110 };
    assert!(words_hit(&bad, &grid(), &page()).is_empty());
    let bad = Line { x0: 300.0, x1: 366.0, y: f32::INFINITY, duration_ms: 110 };
    assert!(words_hit(&bad, &grid(), &page()).is_empty());
    let flat = PageGrid { char_width: 0.0, ..grid() };
    assert!(words_hit(&slash(300.0, 366.0, 205.0), &flat, &page()).is_empty());
}

// ---- what the page draws ----

fn deletion(id: u64, words: &[(usize, usize, usize, &str)]) -> Deletion {
    Deletion {
        id,
        page: 0,
        started_unix_ms: 0,
        points: vec![InkPoint { x: 0.0, y: 0.0, pressure: 0.5, tilt_x: 0.0, tilt_y: 0.0, t_ms: 0 }],
        words: words
            .iter()
            .map(|&(line, start, end, text)| Deleted { line, start, end, text: text.to_string() })
            .collect(),
    }
}

#[test]
fn the_ranges_of_a_page_are_kept_per_line_in_order_and_joined_where_they_overlap() {
    let lines = page();
    let ranges = hidden_ranges(
        &[
            deletion(1, &[(12, 15, 18, "klm"), (12, 4, 14, "abcdefghij"), (13, 4, 14, "lmnopqrstu")]),
            deletion(2, &[(12, 10, 14, "ghij")]),
        ],
        1,
        &lines,
    );
    assert_eq!(ranges.len(), lines.len(), "a list for every line of the page");
    assert_eq!(ranges[11], vec![(4, 14), (15, 18)]);
    assert_eq!(ranges[12], vec![(4, 14)]);
    assert!(ranges[0].is_empty() && ranges[13].is_empty());
}

#[test]
fn a_word_of_a_line_the_file_no_longer_has_is_not_hidden() {
    // the file changed after the session was written: the bytes say another word now
    let lines = page();
    let ranges = hidden_ranges(&[deletion(1, &[(12, 4, 14, "zzzzzzzzzz"), (12, 15, 18, "klm")])], 1, &lines);
    assert_eq!(ranges[11], vec![(15, 18)]);
    // a range past the end of the line, or in the middle of a character
    let mut lines = page();
    lines[0] = "héllo".to_string();
    let ranges = hidden_ranges(&[deletion(1, &[(1, 2, 3, "é"), (1, 3, 99, "llo"), (999, 0, 1, "x")])], 1, &lines);
    assert!(ranges[0].is_empty(), "{:?}", ranges[0]);
}

#[test]
fn the_words_of_other_pages_are_not_on_this_one() {
    let lines = page();
    let second = (0..lines.len()).map(|_| String::new()).collect::<Vec<_>>();
    let ranges = hidden_ranges(&[deletion(1, &[(12, 4, 14, "abcdefghij")])], 47, &second);
    assert!(ranges.iter().all(Vec::is_empty));
    // the same word on the page that starts at line 1
    assert_eq!(hidden_ranges(&[deletion(1, &[(12, 4, 14, "abcdefghij")])], 1, &lines)[11], vec![(4, 14)]);
}

#[test]
fn a_line_is_cut_at_the_ranges_into_what_is_seen_and_what_is_hidden() {
    let line = "    abcdefghij klm";
    let s = segments(line, &[(4, 14)]);
    assert_eq!(
        s,
        vec![
            Segment { start: 0, end: 4, hidden: false },
            Segment { start: 4, end: 14, hidden: true },
            Segment { start: 14, end: 18, hidden: false },
        ]
    );
    assert_eq!(segments(line, &[]), vec![Segment { start: 0, end: 18, hidden: false }]);
    assert_eq!(segments(line, &[(0, 4)]), vec![Segment { start: 0, end: 4, hidden: true }, Segment { start: 4, end: 18, hidden: false }]);
    assert_eq!(segments(line, &[(15, 18)])[1], Segment { start: 15, end: 18, hidden: true });
    assert_eq!(segments(line, &[(0, 18)]), vec![Segment { start: 0, end: 18, hidden: true }]);
    assert!(segments("", &[]).is_empty());
}

#[test]
fn the_pieces_of_a_cut_line_are_the_line_again_whatever_the_ranges() {
    let line = "héllo wörld → x";
    for ranges in [vec![], vec![(0, 6)], vec![(7, 13)], vec![(0, 1), (3, 6), (14, 17)], vec![(2, 3)], vec![(0, 999)]] {
        let s = segments(line, &ranges);
        let joined: String = s.iter().map(|p| &line[p.start..p.end]).collect();
        assert_eq!(joined, line, "{ranges:?}");
        assert!(s.windows(2).all(|w| w[0].end == w[1].start), "no gap and no overlap: {s:?}");
    }
}
