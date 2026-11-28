use super::*;
use crate::{buffer::TextBuffer, highlight::{Grammar, Highlighter}};

#[test]
fn huge_rs_has_a_hundred_thousand_lines_and_reads_back() {
    let text = huge_rs(HUGE_LINES);
    let b = TextBuffer::from_text(&text);
    assert_eq!(b.line_count(), HUGE_LINES + 1, "the last line is the empty one after the break");
    assert_eq!(b.line(90_002), "pub fn f9000(x: u64) -> u64 {");
    assert_eq!(prefix(), "code/sample/");
    let keys: Vec<String> = jobs()
        .into_iter()
        .map(|j| match j {
            FileJob::Put { key, .. } => key,
            other => panic!("{other:?}"),
        })
        .collect();
    assert!(keys.contains(&"code/sample/huge.rs".to_string()));
}

#[test]
fn the_sample_sources_highlight_as_rust() {
    let b = TextBuffer::from_text(MAIN_RS);
    let mut h = Highlighter::new(Grammar::load(), "main.rs", "");
    let text_of = |i: usize| b.line(i);
    for line in 0..b.line_count() {
        assert!(h.line_spans(line, &text_of).is_some());
    }
    assert_eq!(h.language(), "Rust");
}
