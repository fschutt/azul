//! `hanging-punctuation` lets a mark hang outside the line it ends or starts: the mark takes no
//! room on the line - it is not counted for the fit of the line or for its alignment - and stands
//! beyond the edge of the line box (CSS Text 3, 8.2).
//!
//! The face is `Azul Mock Mono` at 20 px: every ASCII glyph is 10 px wide and its glyph id is the
//! code point minus 0x1F.
//!
//! - `first`: an opening bracket or quote at the start of the FIRST line of the block hangs on its
//!   start side;
//! - `last`: a closing bracket or quote at the end of the LAST line hangs on the end side;
//! - `force-end`: a stop or comma at the end of a line always hangs;
//! - `allow-end`: a stop or comma at the end of a line hangs only if it does not otherwise fit.
//!
//! A mark inside a line, and a letter at the end of one, never hangs.

use crate::text_rig::{lines, laid_out, line_texts, Painted, MONO};
use azul_layout::solver3::display_list::DisplayList;

/// The pens of the glyphs of line `n` (from the top), in reading order, with the characters they
/// draw; the spaces are left out (a line may or may not paint the space it ends in).
fn line(dl: &DisplayList, n: usize) -> Vec<(char, f32)> {
    let all = lines(dl);
    assert!(n < all.len(), "the page has {} lines: {:?}", all.len(), line_texts(dl));
    let mut glyphs: Vec<Painted> = all[n].clone();
    glyphs.sort_by(|a, b| a.x.total_cmp(&b.x));
    glyphs
        .iter()
        .map(|g| (char::from_u32(g.id + 0x1F).unwrap_or('?'), g.x))
        .filter(|(c, _)| *c != ' ')
        .collect()
}

fn pens(css: &str, text: &str, width: f32) -> DisplayList {
    laid_out(MONO, css, text, width).1
}

fn assert_line(dl: &DisplayList, n: usize, expected: &[(char, f32)], what: &str) {
    let found = line(dl, n);
    let same = found.len() == expected.len()
        && found
            .iter()
            .zip(expected)
            .all(|(f, e)| f.0 == e.0 && (f.1 - e.1).abs() < 0.01);
    assert!(same, "{what}: line {n} is {found:?}, expected {expected:?} (all: {:?})", line_texts(dl));
}

/// Without the property nothing hangs: the guard of every case below.
#[test]
fn nothing_hangs_by_default() {
    let dl = pens("", "(ab)", 200.0);
    assert_line(&dl, 0, &[('(', 0.0), ('a', 10.0), ('b', 20.0), (')', 30.0)], "default");
    let dl = pens("text-align: right", "ab, cd", 50.0);
    assert_line(&dl, 0, &[('a', 20.0), ('b', 30.0), (',', 40.0)], "right, default");
    let dl = pens("hanging-punctuation: none; text-align: right", "ab, cd", 50.0);
    assert_line(&dl, 0, &[('a', 20.0), ('b', 30.0), (',', 40.0)], "right, none");
}

/// `first`: the opening bracket stands before the start of the line, the text after it starts at
/// the edge.
#[test]
fn first_hangs_an_opening_bracket_or_quote_of_the_first_line() {
    for (open, text) in [('(', "(ab cd"), ('[', "[ab cd"), ('{', "{ab cd"), ('"', "\"ab cd"), ('\'', "'ab cd")] {
        let dl = pens("hanging-punctuation: first", text, 200.0);
        let found = line(&dl, 0);
        assert_eq!(found[0].0, open, "{text}: {found:?}");
        assert!((found[0].1 + 10.0).abs() < 0.01, "{text}: the mark hangs 10 px before the line: {found:?}");
        assert!((found[1].1 - 0.0).abs() < 0.01, "{text}: the text starts at the edge: {found:?}");
        assert!((found[2].1 - 10.0).abs() < 0.01, "{text}: {found:?}");
    }
}

/// A closing bracket does not hang at the start, an opening bracket in the middle of the line
/// never hangs, and neither does the opening bracket of a later line.
#[test]
fn first_hangs_only_an_opening_mark_at_the_start_of_the_first_line() {
    let dl = pens("hanging-punctuation: first", ")ab", 200.0);
    assert_line(&dl, 0, &[(')', 0.0), ('a', 10.0), ('b', 20.0)], "a closing bracket");
    let dl = pens("hanging-punctuation: first", "ab (cd", 200.0);
    assert_line(&dl, 0, &[('a', 0.0), ('b', 10.0), ('(', 30.0), ('c', 40.0), ('d', 50.0)], "mid line");
    // "ab (cd" in a box of 30 px: "ab" and then "(cd" on the second line, which does not hang
    let dl = pens("hanging-punctuation: first", "ab (cd", 30.0);
    assert_line(&dl, 0, &[('a', 0.0), ('b', 10.0)], "the first line");
    assert_line(&dl, 1, &[('(', 0.0), ('c', 10.0), ('d', 20.0)], "the second line");
}

/// The text after the hung mark is placed by the alignment as if the mark were not there: centred
/// (20 of 100 px: the text from 40, the mark at 30) and flush right.
#[test]
fn a_hung_mark_takes_no_part_in_the_alignment() {
    let dl = pens("hanging-punctuation: first; text-align: center", "(ab", 100.0);
    assert_line(&dl, 0, &[('(', 30.0), ('a', 40.0), ('b', 50.0)], "centre");
    let dl = pens("hanging-punctuation: first; text-align: right", "(ab", 100.0);
    assert_line(&dl, 0, &[('(', 70.0), ('a', 80.0), ('b', 90.0)], "right");
}

/// `last`: the closing bracket of the last line hangs on the end side; one that ends an earlier
/// line does not.
#[test]
fn last_hangs_a_closing_bracket_or_quote_of_the_last_line() {
    for text in ["ab)", "ab]", "ab}", "ab\"", "ab'"] {
        let dl = pens("hanging-punctuation: last; text-align: right", text, 100.0);
        let found = line(&dl, 0);
        assert!((found[0].1 - 80.0).abs() < 0.01, "{text}: the text is flush right without the mark: {found:?}");
        assert!((found[2].1 - 100.0).abs() < 0.01, "{text}: the mark hangs beyond the edge: {found:?}");
    }
    // two lines of 40 px: "xx)" and "yy)"; only the last hangs
    let dl = pens("hanging-punctuation: last; text-align: right", "xx) yy)", 40.0);
    assert_line(&dl, 0, &[('x', 10.0), ('x', 20.0), (')', 30.0)], "the first line");
    assert_line(&dl, 1, &[('y', 20.0), ('y', 30.0), (')', 40.0)], "the last line");
    // an opening bracket at the start is `first`'s
    let dl = pens("hanging-punctuation: last", "(ab", 200.0);
    assert_line(&dl, 0, &[('(', 0.0), ('a', 10.0), ('b', 20.0)], "last does not hang an opening mark");
}

/// `force-end`: the comma that ends the line hangs whether or not it fits.
#[test]
fn force_end_hangs_the_stop_or_comma_that_ends_a_line() {
    // "ab," fits in 50 px, and hangs all the same: 20 px of text flush right
    let dl = pens("hanging-punctuation: force-end; text-align: right", "ab, cd", 50.0);
    assert_line(&dl, 0, &[('a', 30.0), ('b', 40.0), (',', 50.0)], "right");
    let dl = pens("hanging-punctuation: force-end; text-align: center", "ab, cd", 50.0);
    assert_line(&dl, 0, &[('a', 15.0), ('b', 25.0), (',', 35.0)], "centre");
    // the full stop, and the line after it
    let dl = pens("hanging-punctuation: force-end; text-align: right", "ab. cd", 50.0);
    assert_line(&dl, 0, &[('a', 30.0), ('b', 40.0), ('.', 50.0)], "a full stop");
    assert_line(&dl, 1, &[('c', 30.0), ('d', 40.0)], "the second line");
    // a comma inside the line stays where it is; a letter at the end does not hang
    let dl = pens("hanging-punctuation: force-end; text-align: right", "a,b", 100.0);
    assert_line(&dl, 0, &[('a', 70.0), (',', 80.0), ('b', 90.0)], "a comma inside the line");
}

/// `allow-end`: the comma hangs only if it does not otherwise fit.
#[test]
fn allow_end_hangs_the_stop_or_comma_that_does_not_fit() {
    // it fits in 50 px: it stays inside the line
    let dl = pens("hanging-punctuation: allow-end; text-align: right", "ab, cd", 50.0);
    assert_line(&dl, 0, &[('a', 20.0), ('b', 30.0), (',', 40.0)], "it fits");
    // "ab," is 30 px and the box 25: the comma hangs and the text of 20 px is flush right
    let dl = pens("hanging-punctuation: allow-end; text-align: right", "ab, cd", 25.0);
    assert_line(&dl, 0, &[('a', 5.0), ('b', 15.0), (',', 25.0)], "it does not fit");
    // the same box without the property: the line overflows and starts at the edge
    let dl = pens("text-align: right", "ab, cd", 25.0);
    assert_line(&dl, 0, &[('a', 0.0), ('b', 10.0), (',', 20.0)], "overflow");
    // `force-end` hangs it there too
    let dl = pens("hanging-punctuation: force-end; text-align: right", "ab, cd", 25.0);
    assert_line(&dl, 0, &[('a', 5.0), ('b', 15.0), (',', 25.0)], "force-end");
}

/// The keywords combine: `first` and `last` and `force-end` together.
#[test]
fn the_keywords_combine() {
    let dl = pens("hanging-punctuation: first force-end last", "(ab,", 100.0);
    assert_line(&dl, 0, &[('(', -10.0), ('a', 0.0), ('b', 10.0), (',', 20.0)], "left");
}
