//! `text-wrap` decides how a paragraph is broken into lines: `wrap` and `stable` take all the
//! words that fit on each line, `nowrap` keeps the text on one line, `balance` breaks the paragraph
//! as a whole so that its lines are as even as the words allow (the last line counts as much as the
//! others), `pretty` breaks it as a whole and leaves the last line as long as it falls.
//!
//! The face is `Azul Mock Mono` (10 px a glyph at 20 px) in a window 100 px wide: a line holds ten
//! glyphs. The text of a line is read back from its glyph ids; the spaces at the end of a line hang
//! and are not told.

use std::time::{Duration, Instant};

use crate::text_rig::{block, laid_out, lay_out, line_texts, text, window, MONO};

/// "aa bb cc dd ee ff gg" breaks greedily into `aa bb cc`, `dd ee ff` and `gg`: a last line of
/// one word.
const WORDS: &str = "aa bb cc dd ee ff gg";

fn broken(css: &str, input: &str) -> Vec<String> {
    let (_lw, dl) = laid_out(MONO, css, input, 100.0);
    line_texts(&dl).into_iter().map(|line| line.trim_end().to_string()).collect()
}

#[test]
fn the_premise_is_a_last_line_of_one_word() {
    assert_eq!(broken("", WORDS), ["aa bb cc", "dd ee ff", "gg"]);
    assert_eq!(broken("text-wrap: wrap", WORDS), ["aa bb cc", "dd ee ff", "gg"]);
    assert_eq!(broken("text-wrap: stable", WORDS), ["aa bb cc", "dd ee ff", "gg"]);
}

#[test]
fn nowrap_keeps_the_text_on_one_line() {
    assert_eq!(broken("text-wrap: nowrap", WORDS), [WORDS]);
    // a `white-space` that does not wrap wins over every `text-wrap`
    assert_eq!(broken("white-space: nowrap; text-wrap: balance", WORDS), [WORDS]);
    assert_eq!(broken("white-space: pre; text-wrap: pretty", WORDS), [WORDS]);
}

/// The lines of a balanced paragraph are about as long as each other: as many as the greedy
/// breaker makes, the last one longer than a single word.
#[test]
fn balance_makes_the_last_line_as_long_as_the_others() {
    let lines = broken("text-wrap: balance", WORDS);
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert!(lines[2].len() >= 5, "the last line is no orphan: {lines:?}");
    let longest = lines.iter().map(String::len).max().unwrap_or(0);
    let shortest = lines.iter().map(String::len).min().unwrap_or(0);
    assert!(longest - shortest <= 3, "lines of 5 to 8 glyphs: {lines:?}");
    // every word stands on a line whole, in order
    assert_eq!(lines.join(" "), WORDS);
}

/// `pretty` breaks the paragraph as a whole but leaves its last line as long as it falls.
#[test]
fn pretty_leaves_the_last_line_as_it_falls() {
    assert_eq!(broken("text-wrap: pretty", WORDS), ["aa bb cc", "dd ee ff", "gg"]);
}

/// A paragraph that is more than ten lines long is broken as `wrap` is: balancing is for headings.
#[test]
fn a_balanced_paragraph_of_more_than_ten_lines_is_broken_as_wrap_is() {
    let long = "aa bb cc dd ee ff gg ".repeat(10);
    let wrap = broken("", &long);
    assert!(wrap.len() > 10, "{}", wrap.len());
    assert_eq!(broken("text-wrap: balance", &long), wrap);
}

#[test]
fn the_value_is_inherited() {
    let mut lw = window();
    let dom = block(MONO, "text-wrap: balance", vec![block(MONO, "", vec![text(WORDS)])]);
    let dl = lay_out(&mut lw, dom, 100.0);
    let lines: Vec<String> = line_texts(&dl).into_iter().map(|l| l.trim_end().to_string()).collect();
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert!(lines[2].len() >= 5, "the inner block is balanced: {lines:?}");
}

/// A paragraph of twenty thousand words is laid out in bounded time under every value: the
/// whole-paragraph breaker looks at every earlier break for each break, so its work is capped
/// (a longer paragraph is broken as `wrap` is).
#[test]
fn a_paragraph_of_twenty_thousand_words_is_laid_out_in_bounded_time() {
    let words = "ab ".repeat(20_000);
    for value in ["balance", "pretty"] {
        let start = Instant::now();
        let (_lw, dl) = laid_out(MONO, &format!("text-wrap: {value}"), &words, 400.0);
        assert!(!line_texts(&dl).is_empty());
        let took = start.elapsed();
        assert!(took < Duration::from_secs(60), "{value}: {took:?}");
    }
}
