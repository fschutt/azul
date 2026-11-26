//! What the suggestions of the agents do to the lines of a page.
//!
//! A suggestion that removes words shows them GREYED AND STRUCK while it waits, and in the COLOUR OF THE PAGE once the
//! reviewer took it (the space stays: nothing reflows, exactly as a slash leaves it); one that replaces words does that to
//! the old ones and shows the new ones after them, YELLOW while it waits and GREEN once taken; a refused suggestion leaves
//! the line as it was. Lines an agent would insert are listed with where they go ([`inserts_of`]).
//!
//! A suggestion names its text (`old`) and the page is checked against it: a range whose text is not what the suggestion
//! said any more (the file changed since) marks nothing. Pure: lines and numbers, no window.

use std::collections::BTreeMap;

use crate::{
    agent_review::{Pos, Suggestion},
    code::SourceFile,
    conversation::Decision,
    model::Note,
};

/// How a piece of a line is drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Look {
    /// The ink of the code.
    Plain,
    /// The colour of the page: words that were taken (by a slash or an accepted suggestion).
    Hidden,
    /// Grey and struck: words a suggestion that waits would take.
    Struck,
    /// New words a suggestion that waits would put there.
    Yellow,
    /// New words of a suggestion the reviewer took.
    Green,
}

/// A piece of a line and how it is drawn: `from` is where it starts in the line of the file (none for new words, which are
/// not in the file).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Piece {
    pub text: String,
    pub look: Look,
    pub from: Option<usize>,
}

/// New words shown after byte `at` of a line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Added {
    pub at: usize,
    pub text: String,
    /// The reviewer took the suggestion that puts them there.
    pub accepted: bool,
}

/// The marks on one line of a page.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LineMarks {
    /// Byte ranges a suggestion that waits would take, in order and joined where they touch.
    pub struck: Vec<(usize, usize)>,
    /// Byte ranges the reviewer took with a suggestion, in order and joined where they touch.
    pub hidden: Vec<(usize, usize)>,
    pub added: Vec<Added>,
}

/// Lines an agent would insert after a line of the file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Insert {
    /// The line they go after (0: before the first).
    pub after_line: usize,
    pub rows: Vec<String>,
    /// The reviewer took the suggestion.
    pub accepted: bool,
    /// Whose suggestion it is.
    pub conversation: u64,
}

/// The text of the range `start` to `end` as `lines` say it now (the first of them is line `first_line`), with the byte
/// ranges of each line it covers; none where a line is not on the page or a column is not a letter's start.
fn range_in(first_line: usize, lines: &[String], start: &Pos, end: &Pos) -> Option<(String, Vec<(usize, usize, usize)>)> {
    if start.line == 0 || end.line < start.line {
        return None;
    }
    let mut text = String::new();
    let mut covered = Vec::new();
    for number in start.line..=end.line {
        let row = number.checked_sub(first_line).filter(|r| *r < lines.len())?;
        let line = &lines[row];
        let from = if number == start.line { start.col } else { 0 };
        let to = if number == end.line { end.col } else { line.len() };
        let piece = line.get(from..to)?;
        if number > start.line {
            text.push('\n');
        }
        text.push_str(piece);
        covered.push((row, from, to));
    }
    Some((text, covered))
}

/// Joins ranges that touch or overlap, in order.
fn joined(ranges: &mut Vec<(usize, usize)>) {
    ranges.sort_unstable();
    let mut out: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
    for &(start, end) in ranges.iter() {
        match out.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => out.push((start, end)),
        }
    }
    *ranges = out;
}

/// What the suggestions of `notes` do to the `lines` of a page (the first of them is line `first_line` of the file): the
/// marks on each line.
#[must_use]
pub fn marks_of(notes: &[Note], first_line: usize, lines: &[String]) -> Vec<LineMarks> {
    let mut out = vec![LineMarks::default(); lines.len()];
    for note in notes {
        for proposal in &note.talk.proposals {
            let accepted = match proposal.decision {
                Decision::Rejected => continue,
                Decision::Accepted => true,
                Decision::Waiting => false,
            };
            let (start, end, old, new) = match &proposal.suggestion {
                Suggestion::InsertLines { .. } => continue,
                Suggestion::RemoveRange { start, end, old } => (start, end, old, None),
                Suggestion::ReplaceRange { start, end, old, text } => (start, end, old, Some(text)),
            };
            let Some((now, covered)) = range_in(first_line, lines, start, end) else {
                continue;
            };
            if now != *old {
                continue;
            }
            for &(row, from, to) in &covered {
                if from < to {
                    let marks = &mut out[row];
                    let list = if accepted { &mut marks.hidden } else { &mut marks.struck };
                    list.push((from, to));
                }
            }
            if let (Some(text), Some(&(row, _, to))) = (new, covered.last()) {
                if !text.is_empty() {
                    out[row].added.push(Added { at: to, text: text.clone(), accepted });
                }
            }
        }
    }
    for marks in &mut out {
        joined(&mut marks.struck);
        joined(&mut marks.hidden);
    }
    out
}

/// The lines the suggestions of `notes` would insert after the lines of a page (`line_count` lines from `first_line`): the
/// ones that wait and the ones that were taken. Lines inserted after the last line of a page belong to that page (it
/// grows); before the first line of the file, to the first.
#[must_use]
pub fn inserts_of(notes: &[Note], first_line: usize, line_count: usize) -> Vec<Insert> {
    let mut out = Vec::new();
    let last_line = first_line + line_count.saturating_sub(1);
    for note in notes {
        for proposal in &note.talk.proposals {
            let accepted = match proposal.decision {
                Decision::Rejected => continue,
                Decision::Accepted => true,
                Decision::Waiting => false,
            };
            let Suggestion::InsertLines { after_line, lines } = &proposal.suggestion else {
                continue;
            };
            let here = (*after_line >= first_line && *after_line <= last_line) || (first_line == 1 && *after_line == 0);
            if here && !lines.is_empty() {
                out.push(Insert { after_line: *after_line, rows: lines.clone(), accepted, conversation: note.id });
            }
        }
    }
    out.sort_by_key(|i| i.after_line);
    out
}

/// How many rows each page grows by when the lines the suggestions of `notes` would put in (the ones that wait and the ones
/// that were taken) are shown in it: the pages that grow only. `lines_on_page` says how many lines a page has; lines put after
/// a line the file does not have grow no page.
#[must_use]
pub fn extra_rows_by_page(notes: &[Note], lines_on_page: impl Fn(usize) -> usize) -> BTreeMap<usize, usize> {
    let mut grown: BTreeMap<usize, usize> = BTreeMap::new();
    for note in notes {
        for proposal in &note.talk.proposals {
            if proposal.decision == Decision::Rejected {
                continue;
            }
            let Suggestion::InsertLines { after_line, lines } = &proposal.suggestion else {
                continue;
            };
            let page = SourceFile::page_of_line(*after_line);
            let first = SourceFile::first_line_of(page);
            let last = first + lines_on_page(page).saturating_sub(1);
            let on_page = (*after_line >= first && *after_line <= last && lines_on_page(page) > 0) || (*after_line == 0 && lines_on_page(0) > 0);
            if on_page && !lines.is_empty() {
                *grown.entry(page).or_insert(0) += lines.len();
            }
        }
    }
    grown
}

/// A byte position inside `line` that is the start of a letter: `at` where it is, the next start where it is inside a
/// letter, the end of the line past it.
fn boundary(line: &str, at: usize) -> usize {
    let mut at = at.min(line.len());
    while !line.is_char_boundary(at) {
        at += 1;
    }
    at
}

/// A byte range of `line` as one a piece can have: both ends at the start of a letter, in order, not empty.
fn valid(line: &str, (start, end): (usize, usize)) -> Option<(usize, usize)> {
    (start < end && end <= line.len() && line.is_char_boundary(start) && line.is_char_boundary(end)).then_some((start, end))
}

/// The pieces a `line` is drawn in: split where a range or a place for new words begins or ends, each with its look. What the
/// reviewer slashed (`slashed`) and what the suggestions took (`marks.hidden`) are in the colour of the page; where a range
/// would be struck and is hidden too it is hidden. New words come after the piece that ends where they go. A range that
/// does not fit the line (past its end, inside a letter) is left out, and the pieces that are in the file add up to the
/// line.
#[must_use]
pub fn pieces(line: &str, slashed: &[(usize, usize)], marks: &LineMarks) -> Vec<Piece> {
    let len = line.len();
    let hidden: Vec<(usize, usize)> = slashed.iter().chain(&marks.hidden).filter_map(|r| valid(line, *r)).collect();
    let struck: Vec<(usize, usize)> = marks.struck.iter().filter_map(|r| valid(line, *r)).collect();
    let added: Vec<(usize, &Added)> = marks.added.iter().map(|a| (boundary(line, a.at), a)).collect();

    let mut cuts: Vec<usize> = vec![0, len];
    cuts.extend(hidden.iter().chain(&struck).flat_map(|&(a, b)| [a, b]));
    cuts.extend(added.iter().map(|(at, _)| *at));
    cuts.sort_unstable();
    cuts.dedup();

    let look_of = |from: usize| {
        if hidden.iter().any(|&(a, b)| a <= from && from < b) {
            Look::Hidden
        } else if struck.iter().any(|&(a, b)| a <= from && from < b) {
            Look::Struck
        } else {
            Look::Plain
        }
    };
    let new_words = |at: usize, out: &mut Vec<Piece>| {
        for (place, a) in &added {
            if *place == at {
                out.push(Piece { text: a.text.clone(), look: if a.accepted { Look::Green } else { Look::Yellow }, from: None });
            }
        }
    };
    let mut out = Vec::new();
    new_words(0, &mut out);
    for pair in cuts.windows(2) {
        let (from, to) = (pair[0], pair[1]);
        if from < to {
            out.push(Piece { text: line[from..to].to_string(), look: look_of(from), from: Some(from) });
        }
        new_words(to, &mut out);
    }
    out
}

#[cfg(test)]
#[path = "page_marks_test.rs"]
mod page_marks_test;
