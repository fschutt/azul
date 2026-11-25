//! `text-align-last`, `text-justify` and `text-indent` place the words of a line: the last line of
//! a justified block is aligned as `text-align-last` says, `text-justify` chooses between spreading
//! the room over the spaces and over every letter, `text-indent` shifts the first line.
//!
//! The face is `Azul Mock Mono` (10 px a glyph at 20 px) in a window 100 px wide: a line holds ten
//! glyphs, and `aaaa bbbb cc dd` breaks into `aaaa bbbb` and `cc dd`.

use crate::text_rig::{laid_out, lines, Painted, MONO};

const TEXT: &str = "aaaa bbbb cc dd";

/// The glyphs of each line of `TEXT` under `css`, spaces left out, each line from the left.
fn words(css: &str) -> Vec<Vec<Painted>> {
    let (_lw, dl) = laid_out(MONO, css, TEXT, 100.0);
    lines(&dl)
        .into_iter()
        .map(|mut line| {
            line.retain(|g| g.id != 1); // the space glyph of the face
            line.sort_by(|a, b| a.x.total_cmp(&b.x));
            line
        })
        .collect()
}

fn xs(line: &[Painted]) -> Vec<f32> {
    line.iter().map(|g| (g.x * 100.0).round() / 100.0).collect()
}

fn assert_xs(css: &str, line: usize, expected: &[f32]) {
    let all = words(css);
    let got = xs(&all[line]);
    assert_eq!(got.len(), expected.len(), "`{css}` line {line}: {got:?}");
    for (pen, want) in got.iter().zip(expected) {
        assert!((pen - want).abs() < 0.05, "`{css}` line {line}: {got:?}, not {expected:?}");
    }
}

#[test]
fn the_premise_is_two_lines_from_the_left() {
    let all = words("");
    assert_eq!(all.len(), 2);
    assert_xs("", 0, &[0.0, 10.0, 20.0, 30.0, 50.0, 60.0, 70.0, 80.0]);
    assert_xs("", 1, &[0.0, 10.0, 30.0, 40.0]);
}

/// `text-align: justify` spreads every line but the last over the box; the last is aligned by
/// `text-align-last`, which is `start` where it says `auto`.
#[test]
fn text_align_last_places_the_last_line_of_a_justified_block() {
    let justify = "text-align: justify";
    // the first line fills the box: the room (10 px) goes to the one space
    assert_xs(justify, 0, &[0.0, 10.0, 20.0, 30.0, 60.0, 70.0, 80.0, 90.0]);
    // the last line is not spread
    assert_xs(justify, 1, &[0.0, 10.0, 30.0, 40.0]);
    assert_xs(&format!("{justify}; text-align-last: auto"), 1, &[0.0, 10.0, 30.0, 40.0]);
    assert_xs(&format!("{justify}; text-align-last: left"), 1, &[0.0, 10.0, 30.0, 40.0]);
    // 50 px of text in a box of 100
    assert_xs(&format!("{justify}; text-align-last: right"), 1, &[50.0, 60.0, 80.0, 90.0]);
    assert_xs(&format!("{justify}; text-align-last: center"), 1, &[25.0, 35.0, 55.0, 65.0]);
    // the room of the last line goes to its space
    assert_xs(&format!("{justify}; text-align-last: justify"), 1, &[0.0, 10.0, 80.0, 90.0]);
}

/// `text-justify` chooses what the room is spread over. The first line `aaaa bbbb` has 10 px of
/// room: inter-word puts them all in the one space, inter-character spreads them over the nine
/// glyphs, `none` spreads nothing (the line is aligned, wherever that puts it).
#[test]
fn text_justify_chooses_the_spaces_or_the_letters() {
    // (the distance from the last `a` to the first `b`, the distance between two letters of a
    // word, the pen of the last `b` relative to the first `a`)
    let first_line = |css: &str| {
        let line = &words(css)[0];
        (line[4].x - line[3].x, line[1].x - line[0].x, line[7].x - line[0].x)
    };
    let (gap, pitch, span) = first_line("text-align: justify; text-justify: inter-word");
    assert!((gap - 30.0).abs() < 0.05 && (pitch - 10.0).abs() < 0.05, "inter-word: {gap} {pitch}");
    assert!((span - 90.0).abs() < 0.05, "and it fills the box: {span}");
    let (gap, pitch, span) = first_line("text-align: justify");
    assert!((gap - 30.0).abs() < 0.05 && (pitch - 10.0).abs() < 0.05, "auto is inter-word: {gap} {pitch}");
    assert!((span - 90.0).abs() < 0.05);

    let (_gap, pitch, span) = first_line("text-align: justify; text-justify: inter-character");
    assert!(pitch > 11.0, "inter-character: the letters move apart: {pitch}");
    assert!((span - 90.0).abs() < 0.05, "and it fills the box: {span}");

    let (gap, pitch, span) = first_line("text-align: justify; text-justify: none");
    assert!(
        (gap - 20.0).abs() < 0.05 && (pitch - 10.0).abs() < 0.05,
        "none: nothing is spread: {gap} {pitch}"
    );
    assert!((span - 80.0).abs() < 0.05, "the line keeps its own width: {span}");
}

#[test]
fn text_indent_shifts_the_first_line() {
    // 30 px of the first line are the indent: `aaaa` is all that fits after it
    let all = words("text-indent: 30px");
    assert_eq!(xs(&all[0]), [30.0, 40.0, 50.0, 60.0]);
    assert_eq!(all[1][0].x, 0.0, "the second line is not indented");
}

#[test]
fn a_hanging_indent_shifts_every_line_but_the_first() {
    let all = words("text-indent: 30px hanging");
    assert_eq!(all[0][0].x, 0.0, "the first line is not indented");
    assert_eq!(all[1][0].x, 30.0, "the others are");
}

/// The pen of the first glyph of each line of `text` under `css`.
fn starts(css: &str, text: &str) -> Vec<f32> {
    let (_lw, dl) = laid_out(MONO, css, text, 100.0);
    lines(&dl)
        .into_iter()
        .map(|line| line.iter().map(|g| g.x).fold(f32::MAX, f32::min))
        .collect()
}

fn assert_starts(css: &str, text: &str, expected: &[f32]) {
    let got = starts(css, text);
    assert_eq!(got.len(), expected.len(), "`{css}`: {got:?}");
    for (pen, want) in got.iter().zip(expected) {
        assert!((pen - want).abs() < 0.05, "`{css}`: {got:?}, not {expected:?}");
    }
}

/// `each-line` indents the line after a forced break as well as the first line.
#[test]
fn each_line_indents_the_line_after_a_forced_break_too() {
    let pre_line = "white-space: pre-line; text-indent: 30px";
    assert_starts(pre_line, "aa\nbb", &[30.0, 0.0]);
    assert_starts(&format!("{pre_line} each-line"), "aa\nbb", &[30.0, 30.0]);
}

/// A line that ends in a forced break is aligned by `text-align-last` like the last line.
#[test]
fn text_align_last_also_places_the_line_before_a_forced_break() {
    let css = "white-space: pre-line; text-align: justify; text-align-last: right";
    // `aa` is 20 px of the 100: the line before the break and the last line both stand at the right
    assert_starts(css, "aa\nbb", &[80.0, 80.0]);
    assert_starts("white-space: pre-line; text-align: justify", "aa\nbb", &[0.0, 0.0]);
}
