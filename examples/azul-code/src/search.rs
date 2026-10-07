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

/// Every match of `needle` in `text` (a whole file; LF, CRLF or CR line
/// breaks, counted as the buffer a file opens in counts them), top to
/// bottom, at most `max`. A file without the needle is passed over in one
/// scan before any line is split.
#[must_use]
pub fn find_in_text(text: &str, needle: &str, how: TextMatch, max: usize) -> Vec<Hit> {
    let mut out = Vec::new();
    if needle.is_empty() || needle.contains('\n') || max == 0 {
        return out;
    }
    // The buffer a file opens in leaves the BOM out and reads CRLF and a
    // lone CR as one break: so do the lines and offsets here.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let normalized;
    let text = if text.contains('\r') {
        normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        normalized.as_str()
    } else {
        text
    };
    // Character by character, as the matcher compares (`str::to_lowercase`
    // would apply the final-sigma rule the matcher does not).
    let lower = |s: &str| s.chars().flat_map(char::to_lowercase).collect::<String>();
    let holds = if how.match_case {
        text.contains(needle)
    } else {
        lower(text).contains(&lower(needle))
    };
    if !holds {
        return out;
    }
    for (line, content) in text.split('\n').enumerate() {
        for (start, end) in matches(content, needle, how) {
            let (preview, preview_start, preview_end) = preview_of(content, start, end);
            out.push(Hit {
                line,
                start,
                end,
                preview,
                preview_start,
                preview_end,
            });
            if out.len() >= max {
                return out;
            }
        }
    }
    out
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
    fn a_search_of_a_files_text_finds_every_line_with_its_preview() {
        let text = "fn main() {\r\n    let picked = 7;\r\n    picked + 1\r\n}\r\n";
        let hits = find_in_text(text, "picked", TextMatch::default(), 100);
        assert_eq!(hits.len(), 2);
        assert_eq!((hits[0].line, hits[0].start, hits[0].end), (1, 8, 14));
        assert_eq!(hits[0].preview, "let picked = 7;", "the indentation and the CR left out");
        assert_eq!(&hits[0].preview[hits[0].preview_start..hits[0].preview_end], "picked");
        assert_eq!(hits[1].found(), Found { line: 2, start: 4, end: 10 });
        assert!(find_in_text(text, "PICKED", TextMatch { match_case: true, ..TextMatch::default() }, 100).is_empty());
        assert_eq!(find_in_text(text, "PICKED", TextMatch::default(), 100).len(), 2, "any case");
        assert_eq!(find_in_text(text, "picked", TextMatch::default(), 1).len(), 1, "at most max");
        assert!(find_in_text(text, "absent", TextMatch::default(), 100).is_empty());
        assert!(find_in_text(text, "", TextMatch::default(), 100).is_empty());
        // A lone CR is a break, as in the buffer the file opens in; a BOM is no column.
        let mixed = "\u{feff}a\rpicked\r\nx picked";
        let lines: Vec<(usize, usize)> = find_in_text(mixed, "picked", TextMatch::default(), 100)
            .iter()
            .map(|h| (h.line, h.start))
            .collect();
        assert_eq!(lines, vec![(1, 0), (2, 2)]);
        let buffer = TextBuffer::from_text(mixed);
        assert_eq!(buffer.line(1), "picked");
        assert_eq!(buffer.line(2), "x picked");
    }

    #[test]
    fn a_long_lines_preview_is_a_window_around_the_match() {
        let line = format!("{}needle{}", "a".repeat(100), "b".repeat(400));
        let (preview, s, e) = preview_of(&line, 100, 106);
        assert!(preview.starts_with('\u{2026}') && preview.ends_with('\u{2026}'));
        assert_eq!(&preview[s..e], "needle");
        assert!(preview.chars().count() <= PREVIEW_CHARS + 2);
        let (short, s, e) = preview_of("  x = needle;", 6, 12);
        assert_eq!((short.as_str(), &"  x = needle;"[6..12]), ("x = needle;", "needle"));
        assert_eq!(&short[s..e], "needle");
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
