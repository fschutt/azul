//! Find / replace in an open file and "go to line": azul-appkit's one
//! matcher (`find::matches`: match case, whole word) run line by line over
//! the piece table, so a million-line file is searched without ever being
//! one string.

use azul_appkit::find::{matches, TextMatch};

use crate::buffer::{Edit, Pos, TextBuffer};

/// The most matches one search lists.
pub const MAX_FOUND: usize = 100_000;

/// One match: bytes `start..end` of line `line`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Found {
    pub line: usize,
    pub start: usize,
    pub end: usize,
}

impl Found {
    #[must_use]
    pub fn start_pos(&self) -> Pos {
        Pos::new(self.line, self.start)
    }

    #[must_use]
    pub fn end_pos(&self) -> Pos {
        Pos::new(self.line, self.end)
    }
}

/// Every match of `needle` in the text, top to bottom (at most
/// [`MAX_FOUND`]; a needle with a line break matches nothing).
#[must_use]
pub fn find_all(buffer: &TextBuffer, needle: &str, how: TextMatch) -> Vec<Found> {
    let mut out = Vec::new();
    if needle.is_empty() || needle.contains('\n') {
        return out;
    }
    for line in 0..buffer.line_count() {
        let text = buffer.line(line);
        for (start, end) in matches(&text, needle, how) {
            out.push(Found { line, start, end });
            if out.len() >= MAX_FOUND {
                return out;
            }
        }
    }
    out
}

/// The first match at or after `at`, round past the end.
#[must_use]
pub fn next_after(found: &[Found], at: Pos) -> Option<usize> {
    if found.is_empty() {
        return None;
    }
    let i = found.partition_point(|f| f.start_pos() < at);
    Some(if i < found.len() { i } else { 0 })
}

/// The last match before `at`, round past the start.
#[must_use]
pub fn previous_before(found: &[Found], at: Pos) -> Option<usize> {
    if found.is_empty() {
        return None;
    }
    let i = found.partition_point(|f| f.start_pos() < at);
    Some(if i > 0 { i - 1 } else { found.len() - 1 })
}

/// The edits replacing every match with `replacement`, last in the text
/// first (what `TextBuffer::apply` takes as one undo step).
#[must_use]
pub fn replace_all(buffer: &TextBuffer, needle: &str, replacement: &str, how: TextMatch) -> Vec<Edit> {
    find_all(buffer, needle, how)
        .into_iter()
        .rev()
        .map(|f| Edit::new(f.start_pos(), f.end_pos(), replacement))
        .collect()
}

/// The longest preview of a matched line (characters).
pub const PREVIEW_CHARS: usize = 160;

/// One match of a search over the folder's files: bytes `start..end` of
/// line `line` of the file, and the line as the results list shows it
/// ([`preview_of`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub line: usize,
    pub start: usize,
    pub end: usize,
    pub preview: String,
    /// The match in `preview` (bytes).
    pub preview_start: usize,
    pub preview_end: usize,
}

impl Hit {
    /// The match as the find bar's kind of match (a selection in the file).
    #[must_use]
    pub fn found(&self) -> Found {
        Found {
            line: self.line,
            start: self.start,
            end: self.end,
        }
    }
}

/// Line `line` as the results list shows a match at bytes `start..end` of
/// it: the indentation left out, a long line cut to a window around the
/// match (`…` where it was cut); the match's place in the preview.
#[must_use]
pub fn preview_of(line: &str, start: usize, end: usize) -> (String, usize, usize) {
    let indent = line.len() - line.trim_start().len();
    let start = start.max(indent).min(line.len());
    let end = end.max(start).min(line.len());
    let lead_chars = line[indent..start].chars().count();
    // At most 30 characters before the match.
    let from = if lead_chars > 30 {
        line[indent..start]
            .char_indices()
            .nth(lead_chars - 30)
            .map_or(start, |(i, _)| indent + i)
    } else {
        indent
    };
    let prefix = if from > indent { "\u{2026}" } else { "" };
    let mut preview = String::from(prefix);
    let match_start = preview.len() + (start - from);
    let match_end = match_start + (end - start);
    let rest: String = line[from..].chars().take(PREVIEW_CHARS).collect();
    let cut = rest.len() < line.len() - from;
    preview.push_str(&rest);
    // A match longer than the window ends where the window does (a char
    // boundary: never inside the ellipsis).
    let match_end = match_end.min(preview.len());
    if cut {
        preview.push('\u{2026}');
    }
    (preview, match_start.min(match_end), match_end)
}

/// "120" (line 120) or "120:5" (line 120, column 5) as a position in a text
/// of `line_count` lines (1-based input, past the end: the last line).
#[must_use]
pub fn go_to_line(input: &str, line_count: usize) -> Option<Pos> {
    let input = input.trim();
    let (line, column) = match input.split_once(':') {
        Some((l, c)) => (l.trim(), Some(c.trim())),
        None => (input, None),
    };
    let line: usize = line.parse().ok()?;
    let column: usize = match column {
        Some(c) => c.parse().ok()?,
        None => 1,
    };
    let last = line_count.max(1) - 1;
    Some(Pos::new(line.saturating_sub(1).min(last), column.saturating_sub(1)))
}

#[cfg(test)]
#[path = "search_test.rs"]
mod tests;
