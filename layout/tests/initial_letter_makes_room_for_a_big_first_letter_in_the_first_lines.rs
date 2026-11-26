//! `initial-letter: N` makes room for a first letter that spans N lines: the lines beside it start
//! after it, the lines below it start at the edge again. (The room is all the engine makes: the
//! letter is not drawn bigger or moved into it yet.)
//!
//! Glyphs here are Azul Mock Mono at 20 px: 10 px wide, in 20 px line boxes (see
//! `common/mono_lines.rs`), in a paragraph 200 px wide. The letter's box is `N * 20 * 0.7 + 4` px
//! wide.

use crate::mono_lines::{page_glyphs, LINE};

/// Plenty of short words.
const WORDS: &str = "aa bb cc dd ee ff gg hh ii jj kk ll mm nn oo pp qq rr ss tt uu vv ww xx yy zz \
                     aa bb cc dd ee ff gg hh ii jj kk ll mm nn oo pp qq rr ss tt uu vv ww xx yy zz \
                     aa bb cc dd ee ff gg hh ii jj kk ll mm nn oo pp qq rr ss tt uu vv ww xx yy zz";

/// The glyphs of the lines whose top is in `from..to` lines of the paragraph's top: their x.
fn lines_x(css: &str, from: usize, to: usize) -> Vec<f32> {
    page_glyphs(
        &format!(".p {{ width: 200px; {css} }}"),
        &format!("<p class=\"p\">{WORDS}</p>"),
    )
    .into_iter()
    .filter(|&(_, top)| top > from as f32 * LINE - 0.01 && top < to as f32 * LINE - 0.01)
    .map(|(x, _)| x)
    .collect()
}

#[test]
fn without_an_initial_letter_every_line_starts_at_the_edge() {
    for line in 0..4 {
        let xs = lines_x("", line, line + 1);
        assert!(!xs.is_empty(), "line {line} has glyphs");
        assert!(xs.iter().copied().fold(f32::MAX, f32::min) < 0.01, "line {line}: {xs:?}");
    }
}

#[test]
fn the_lines_beside_the_letter_start_after_it_and_the_lines_below_start_at_the_edge() {
    let css = "initial-letter: 3";
    // the letter is 3 * 20 * 0.7 + 4 = 46 px wide
    for line in 1..3 {
        let xs = lines_x(css, line, line + 1);
        assert!(!xs.is_empty(), "line {line} has glyphs");
        let first = xs.iter().copied().fold(f32::MAX, f32::min);
        assert!(first > 40.0, "line {line} starts after the letter: {first}");
    }
    for line in 3..5 {
        let xs = lines_x(css, line, line + 1);
        assert!(!xs.is_empty(), "line {line} has glyphs");
        let first = xs.iter().copied().fold(f32::MAX, f32::min);
        assert!(first < 0.01, "line {line} is below the letter: {first}");
    }
}

/// The size is the number of lines the letter spans.
#[test]
fn the_letter_spans_as_many_lines_as_it_says() {
    let beside = |css: &str| -> usize {
        (1..6)
            .filter(|&line| {
                lines_x(css, line, line + 1).iter().copied().fold(f32::MAX, f32::min) > 20.0
            })
            .count()
    };
    assert_eq!(beside("initial-letter: 2"), 1, "the second line");
    assert_eq!(beside("initial-letter: 3"), 2);
    assert_eq!(beside("initial-letter: 4"), 3);
}
