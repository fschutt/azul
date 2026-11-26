//! The words a slash takes away.
//!
//! A deletion is a diff, kept in memory and in the session archive, never written to the source file (an export is a
//! separate, explicit action): the file, the line, the bytes of the word, what it said, when, and the stroke that did it.
//! The page then draws the word in the colour of the page and leaves its place as it is - no reflow, no change of any
//! page break: [`hidden_ranges`] says which bytes of which line, [`segments`] cuts the line at them.
//!
//! Which words a slash crosses ([`words_hit`]) is decided by one rule, said here once:
//!
//! 1. the slash lies on the LINE its middle height is in;
//! 2. a word is a run of letters, digits and `_`, or a run of other signs, between blanks (`foo(bar, 1);` is `foo`, `(`,
//!    `bar`, `,`, `1` and `);`);
//! 3. the word under the MIDDLE of the stroke is taken, whatever its length (a short slash through the middle of a long
//!    word takes it);
//! 4. every other word is taken when the stroke covers at least [`COVER`] (60 percent) of its width, so a stroke that
//!    only touches the next word leaves it, and a slash across a stretch of a line takes each word it crosses.
//!
//! No window, no azul: `deletion_test.rs` runs it on the page the fixtures are drawn on.

use crate::{agent_review::PageGrid, model::InkPoint, slash::Line};

/// How much of a word a slash must cover for the word to be taken with the one under its middle.
pub const COVER: f32 = 0.6;
/// How many deletions a file keeps: far more than anyone makes by hand; past it a slash is ink like any other.
pub const MAX_DELETIONS: usize = 5_000;
/// How many samples of a slash are kept with it (a quick stroke has a few dozen).
pub const MAX_POINTS: usize = 512;
/// How many bytes of a word are kept with it, to tell it from another when the file has changed (the word's place is
/// kept whole).
pub const MAX_TEXT_BYTES: usize = 256;

/// A word of a line: where it is, in bytes of the line and in columns of the page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub start: usize,
    pub end: usize,
    /// The column it starts in (a character is a column) and how many it takes.
    pub col: usize,
    pub cols: usize,
}

/// A character of an identifier or a number.
fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The words of `line`: runs of letters, digits and `_`, and runs of other signs, between blanks.
#[must_use]
pub fn tokens(line: &str) -> Vec<Token> {
    let mut out = Vec::new();
    // the run being read: where it began, the column, and whether it is made of word characters
    let mut open: Option<(usize, usize, bool)> = None;
    let mut col = 0;
    let close = |out: &mut Vec<Token>, open: &mut Option<(usize, usize, bool)>, end: usize, col: usize| {
        if let Some((start, from, _)) = open.take() {
            out.push(Token { start, end, col: from, cols: col - from });
        }
    };
    for (i, c) in line.char_indices() {
        if c.is_whitespace() {
            close(&mut out, &mut open, i, col);
        } else {
            let word = is_word_char(c);
            match open {
                Some((_, _, w)) if w == word => {}
                _ => {
                    close(&mut out, &mut open, i, col);
                    open = Some((i, col, word));
                }
            }
        }
        col += 1;
    }
    close(&mut out, &mut open, line.len(), col);
    out
}

/// A word a slash took: its place in the file and what it said.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Deleted {
    /// The line of the file, the first being 1.
    pub line: usize,
    /// The bytes of the word in the line.
    pub start: usize,
    pub end: usize,
    /// What it said (its first [`MAX_TEXT_BYTES`] bytes): the file may have changed since.
    pub text: String,
}

/// The words of `lines` (the lines of a page, the first being line `grid.first_line`, drawn as `grid` says) that `slash`
/// takes: by the rule at the top of this file.
#[must_use]
pub fn words_hit(slash: &Line, grid: &PageGrid, lines: &[String]) -> Vec<Deleted> {
    let numbers = [slash.x0, slash.x1, slash.y, grid.origin_x, grid.origin_y, grid.line_height, grid.char_width];
    if !numbers.iter().all(|n| n.is_finite()) || grid.line_height <= 0.0 || grid.char_width <= 0.0 {
        return Vec::new();
    }
    let row = ((slash.y - grid.origin_y) / grid.line_height).floor();
    if row < 0.0 || row >= lines.len() as f32 {
        return Vec::new();
    }
    let row = row as usize;
    let text = &lines[row];
    let (x0, x1) = (slash.x0.min(slash.x1), slash.x0.max(slash.x1));
    let middle = (x0 + x1) / 2.0;
    let mut out = Vec::new();
    for token in tokens(text) {
        let left = grid.origin_x + token.col as f32 * grid.char_width;
        let right = left + token.cols as f32 * grid.char_width;
        let covered = (x1.min(right) - x0.max(left)).max(0.0) / (right - left);
        if covered >= COVER || (middle >= left && middle < right) {
            out.push(Deleted {
                line: grid.first_line + row,
                start: token.start,
                end: token.end,
                text: head(&text[token.start..token.end], MAX_TEXT_BYTES).to_string(),
            });
        }
    }
    out
}

/// The first `max` bytes of `text`, cut at a character.
fn head(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// One slash: the stroke that was drawn (kept for the agent: where, how hard, when) and the words it took.
#[derive(Debug, Clone, PartialEq)]
pub struct Deletion {
    /// The stroke's id (ids of strokes and deletions are one series).
    pub id: u64,
    pub page: usize,
    /// The wall-clock moment the stroke began, Unix ms (0: not known).
    pub started_unix_ms: u64,
    pub points: Vec<InkPoint>,
    pub words: Vec<Deleted>,
}

/// The byte ranges of each line of a page that the deletions hide, in order and joined where they touch. A word whose
/// line the file no longer says as it did (the bytes are not the word) is left visible.
#[must_use]
pub fn hidden_ranges(deletions: &[Deletion], first_line: usize, lines: &[String]) -> Vec<Vec<(usize, usize)>> {
    let mut out: Vec<Vec<(usize, usize)>> = vec![Vec::new(); lines.len()];
    for word in deletions.iter().flat_map(|d| d.words.iter()) {
        let Some(row) = word.line.checked_sub(first_line) else {
            continue;
        };
        let Some(text) = lines.get(row) else {
            continue;
        };
        if text.get(word.start..word.end).is_some_and(|now| now.starts_with(&word.text)) && word.start < word.end {
            out[row].push((word.start, word.end));
        }
    }
    for ranges in &mut out {
        ranges.sort_unstable();
        let mut joined: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
        for &(start, end) in ranges.iter() {
            match joined.last_mut() {
                Some(last) if start <= last.1 => last.1 = last.1.max(end),
                _ => joined.push((start, end)),
            }
        }
        *ranges = joined;
    }
    out
}

/// A piece of a line, as the page draws it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    pub start: usize,
    pub end: usize,
    /// Drawn in the colour of the page.
    pub hidden: bool,
}

/// `line` cut at `hidden` (byte ranges, in order): the pieces are the line again, one after the other. A range that is
/// not on the characters of the line is left out.
#[must_use]
pub fn segments(line: &str, hidden: &[(usize, usize)]) -> Vec<Segment> {
    let len = line.len();
    let mut out = Vec::new();
    let mut at = 0;
    for &(start, end) in hidden {
        let (start, end) = (start.max(at), end.min(len));
        if start >= end || !line.is_char_boundary(start) || !line.is_char_boundary(end) {
            continue;
        }
        if start > at {
            out.push(Segment { start: at, end: start, hidden: false });
        }
        out.push(Segment { start, end, hidden: true });
        at = end;
    }
    if at < len {
        out.push(Segment { start: at, end: len, hidden: false });
    }
    out
}

#[cfg(test)]
#[path = "deletion_test.rs"]
mod deletion_test;
