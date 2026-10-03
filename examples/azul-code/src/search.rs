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
mod tests {
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
    fn go_to_line_reads_a_line_and_a_column() {
        assert_eq!(go_to_line("3", 10), Some(Pos::new(2, 0)));
        assert_eq!(go_to_line(" 3:4 ", 10), Some(Pos::new(2, 3)));
        assert_eq!(go_to_line("99", 10), Some(Pos::new(9, 0)), "past the end: the last line");
        assert_eq!(go_to_line("0", 10), Some(Pos::new(0, 0)));
        assert_eq!(go_to_line("x", 10), None);
        assert_eq!(go_to_line("", 10), None);
    }
}
