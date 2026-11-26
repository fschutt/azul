//! A resolved conversation as one patch file.
//!
//! What the reviewer took of an agent's suggestions is a diff kept in memory and in the session archive, and the files are
//! never written. Once a conversation is resolved it is also written as ONE `.patch` file: a small header that says which
//! conversation it is, how it ended, over which revision of the workspace (the git hash, when there is one), when, what the
//! reviewer said, what the agent made of it and which files it touches, and then, where something was taken, a unified diff
//! against the lines of the file. A conversation that took nothing is the header alone, so that every resolved conversation is
//! one file and the files are the whole record: the sequence of these patches over a base hash is the review's own sequence
//! of revisions, and what the whiteboard's snapshots (their WHY) and the HTML export read.
//!
//! # The file
//!
//! `review/patches-<16 hex digits of the workspace folder>/<YYYYMMDD>-<HHMMSS>-c<conversation>.patch` in the data root (the
//! time is UTC, [`file_name`]). One conversation has one file: resolved again (opened again, decided again), it writes a new
//! file and the old one is removed. To find the file of conversation 7 list the folder for names ending `-c7.patch`.
//!
//! # The header
//!
//! Lines of `Key: value`, one line each (a value's line breaks are spaces, a value is at most 500 characters), ending at the
//! first empty line. The first line is the format, `AzReview-Patch: 1`; a reader that does not know the number reads nothing,
//! and a reader ignores keys it does not know. In this order:
//!
//! ```text
//! AzReview-Patch: 1
//! Conversation: 7                      the conversation's number in the review (a whole number)
//! Outcome: accepted                    accepted | rejected | answered | closed
//! Base: 3833f10608241be94251152c...    the workspace's git revision, or "none" (no repository, or not read yet)
//! Time: 2025-11-26T05:40:12Z           when it was resolved, ISO 8601 UTC
//! Said: rename step three to ...       what the reviewer said, as it was heard (the dictation, then each follow-up after " / ")
//! Understood: rename step_3 to ...     what the words came to against the workspace's names (only where they were read as names)
//! Rationale: One line goes in.         what the agent said, answer after answer (separated by " / ")
//! Files: src/lib.rs, src/main.rs       the files the diff is about, or "none"
//! Left-out: line 3: the words "9" ...  zero or more: a suggestion that was taken and could not be put in the diff, and why
//! ```
//!
//! `Outcome` is how the conversation ended: `accepted` (the reviewer took at least one suggestion: a diff follows), `rejected`
//! (suggestions were made and none was taken), `answered` (the agent answered in words and suggested nothing), `closed` (the
//! reviewer closed it with nothing said back). The empty line is followed by the diff (`diff --git a/<path> b/<path>`, `---`,
//! `+++`, hunks with three lines of context), which `patch -p1` and `git apply` take; a header alone is no patch to them.
//!
//! [`read_header`] reads a header back (the whiteboard's snapshots call it for their WHY).
//!
//! This file is pure: lines in, text out. It reads no file and runs no program.

use crate::agent_review::{Pos, Suggestion};

/// The lines of context around a hunk.
pub const CONTEXT: usize = 3;
/// The most lines of a region the line diff compares (a bigger region is all removed and all added).
const MAX_REGION: usize = 600;
/// The longest line of a header, characters.
const MAX_HEADER_CHARS: usize = 500;
/// The most suggestions a header says were left out.
const MAX_LEFT_OUT: usize = 20;
/// The most lines of a file a patch is made against.
pub const MAX_FILE_LINES: usize = 5_000_000;

/// A diff of one file: its text, whether anything changed, and what was left out and why.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Diff {
    /// `--- a/<path>`, `+++ b/<path>` and the hunks; empty where nothing changed.
    pub text: String,
    pub changed: bool,
    /// Suggestions that could not be applied to the file as it is (the words are not there any more, a column is inside a
    /// tab, two are over the same words), each in a sentence that starts with the line.
    pub left_out: Vec<String>,
}

/// How a conversation ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// The reviewer took at least one suggestion: it has a patch.
    Accepted,
    /// There were suggestions and the reviewer took none.
    Rejected,
    /// The agent answered in words and suggested nothing.
    Answered,
    /// The reviewer closed it with no answer.
    Closed,
}

impl Outcome {
    /// The outcome a word stands for (the header's `Outcome`).
    #[must_use]
    pub fn parse(word: &str) -> Option<Self> {
        [Self::Accepted, Self::Rejected, Self::Answered, Self::Closed].into_iter().find(|o| o.name() == word.trim())
    }

    /// The word the header keeps.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
            Self::Answered => "answered",
            Self::Closed => "closed",
        }
    }
}

/// What a header says.
#[derive(Debug, Clone, Copy)]
pub struct Header<'a> {
    pub conversation: u64,
    pub outcome: Outcome,
    /// The workspace's revision (a git hash) when the conversation was resolved; none where there is no repository.
    pub base: Option<&'a str>,
    /// When it was resolved, Unix ms.
    pub time_unix_ms: u64,
    /// What the reviewer said, as it was heard.
    pub said: &'a str,
    /// What the words came to against the workspace's names, where they were read as names.
    pub understood: Option<&'a str>,
    /// What the agent said.
    pub rationale: &'a str,
    /// The suggestions that were taken and could not be put in the diff, each in a sentence.
    pub left_out: &'a [String],
}

/// The lines of a file as the file has them (a trailing `\r` is part of a line), with where each begins in the file's text:
/// every line, the last too, is followed by `\n` there.
struct File<'a> {
    lines: &'a [String],
    /// Where line `i` (0-based) begins; one more: where the text ends.
    starts: Vec<usize>,
    /// Most lines end with `\r\n`: what is added ends that way too.
    crlf: bool,
}

/// One change to the text of a file: the bytes `from..to` become `text`.
struct Edit {
    from: usize,
    to: usize,
    /// The lines (0-based, `first..last_excl`) the edit changes; none for lines put between two.
    first: usize,
    last_excl: usize,
    /// The line the sentences about it name.
    line: usize,
    text: String,
}

/// A line of the file as the page shows it: the line end gone, a tab four spaces (the agent's columns count them so).
fn shown(raw: &str) -> String {
    raw.trim_end_matches('\r').replace('\t', "    ")
}

/// Where each byte column of the shown line is in the raw line: `Some(raw offset)` at the start of every letter and of every
/// tab and at the end of the line; none inside a letter and inside the four columns of a tab.
fn column_map(raw: &str) -> Vec<Option<usize>> {
    let body = raw.trim_end_matches('\r');
    let mut map: Vec<Option<usize>> = Vec::with_capacity(body.len() + 4);
    for (at, ch) in body.char_indices() {
        if ch == '\t' {
            map.extend([Some(at), None, None, None]);
        } else {
            map.push(Some(at));
            map.extend(std::iter::repeat_n(None, ch.len_utf8() - 1));
        }
    }
    map.push(Some(body.len()));
    map
}

impl<'a> File<'a> {
    fn new(lines: &'a [String]) -> Self {
        let mut starts = Vec::with_capacity(lines.len() + 1);
        let mut at = 0;
        for line in lines {
            starts.push(at);
            at += line.len() + 1;
        }
        starts.push(at);
        let crlf = lines.iter().filter(|l| l.ends_with('\r')).count() * 2 > lines.len();
        Self { lines, starts, crlf }
    }

    /// The file's text: every line followed by `\n`.
    fn text(&self) -> String {
        let mut text = String::with_capacity(self.starts.last().copied().unwrap_or(0));
        for line in self.lines {
            text.push_str(line);
            text.push('\n');
        }
        text
    }

    /// The byte of the file that column `col` of line `number` (1-based) is, as the page shows the line; none inside a letter
    /// or a tab and past the end of the line.
    fn offset(&self, number: usize, col: usize) -> Option<usize> {
        let within = (*column_map(&self.lines[number - 1]).get(col)?)?;
        Some(self.starts[number - 1] + within)
    }

    /// The change a range makes: the words in it become `new`. The words the range holds now are checked against `old`.
    fn range_edit(&self, start: &Pos, end: &Pos, old: &str, new: &str) -> Result<Edit, String> {
        let line = start.line;
        if start.line == 0 || end.line < start.line || end.line > self.lines.len() {
            return Err(format!("line {line}: the range is not on the file as it is"));
        }
        let mut now = String::new();
        for number in start.line..=end.line {
            let text = shown(&self.lines[number - 1]);
            let from = if number == start.line { start.col } else { 0 };
            let to = if number == end.line { end.col } else { text.len() };
            let Some(piece) = text.get(from..to) else {
                return Err(format!("line {line}: the range is not on the file as it is"));
            };
            if number > start.line {
                now.push('\n');
            }
            now.push_str(piece);
        }
        if now != old {
            return Err(format!("line {line}: the words {old:?} are not what the file says there any more"));
        }
        let (Some(from), Some(to)) = (self.offset(start.line, start.col), self.offset(end.line, end.col)) else {
            return Err(format!("line {line}: the range begins or ends inside a tab"));
        };
        let text = if self.crlf { new.replace("\r\n", "\n").replace('\n', "\r\n") } else { new.to_string() };
        Ok(Edit { from, to, first: start.line - 1, last_excl: end.line, line, text })
    }

    /// The change a suggestion makes, or why it cannot be made.
    fn edit_of(&self, suggestion: &Suggestion) -> Result<Edit, String> {
        match suggestion {
            Suggestion::InsertLines { after_line, lines } => {
                if *after_line > self.lines.len() || lines.is_empty() {
                    return Err(format!("line {after_line}: the file has no such line"));
                }
                let end = if self.crlf { "\r\n" } else { "\n" };
                let text: String = lines.iter().map(|l| format!("{}{end}", l.trim_end_matches('\r'))).collect();
                let at = self.starts[*after_line];
                Ok(Edit { from: at, to: at, first: *after_line, last_excl: *after_line, line: *after_line, text })
            }
            Suggestion::ReplaceRange { start, end, old, text } => self.range_edit(start, end, old, text),
            Suggestion::RemoveRange { start, end, old } => self.range_edit(start, end, old, ""),
        }
    }
}

/// The diff of the lines `old` and `new` (a region around some changes): `(' ' | '-' | '+', line)` in order, by the longest
/// common subsequence; a region too big for that is all removed and all added.
fn line_diff<'a>(old: &[&'a str], new: &[&'a str]) -> Vec<(char, &'a str)> {
    if old.len() > MAX_REGION || new.len() > MAX_REGION {
        return old.iter().map(|l| ('-', *l)).chain(new.iter().map(|l| ('+', *l))).collect();
    }
    let (n, m) = (old.len(), new.len());
    // table[i][j]: the length of the longest common subsequence of old[i..] and new[j..]
    let mut table = vec![vec![0usize; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i][j] = if old[i] == new[j] { table[i + 1][j + 1] + 1 } else { table[i + 1][j].max(table[i][j + 1]) };
        }
    }
    let mut out = Vec::with_capacity(n + m);
    let (mut i, mut j) = (0, 0);
    while i < n && j < m {
        if old[i] == new[j] {
            out.push((' ', old[i]));
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            out.push(('-', old[i]));
            i += 1;
        } else {
            out.push(('+', new[j]));
            j += 1;
        }
    }
    out.extend(old[i..].iter().map(|l| ('-', *l)));
    out.extend(new[j..].iter().map(|l| ('+', *l)));
    out
}

/// The unified diff of the `accepted` suggestions against the lines `raw` of the file `path` (the lines as the file has
/// them: tabs and line ends included). A suggestion that cannot be applied to the file as it is, or that is over words
/// another one (taken before it) changes, is left out with a sentence; the others make the diff.
///
/// # Errors
/// A sentence for the log when the file has more lines than a patch is made against.
pub fn diff_for(path: &str, raw: &[String], accepted: &[&Suggestion]) -> Result<Diff, String> {
    if raw.len() > MAX_FILE_LINES {
        return Err(format!("{path} has {} lines: more than a patch is made against ({MAX_FILE_LINES})", raw.len()));
    }
    let file = File::new(raw);
    let mut left_out = Vec::new();
    let mut kept: Vec<Edit> = Vec::new();
    for suggestion in accepted {
        match file.edit_of(suggestion) {
            Err(why) => left_out.push(why),
            Ok(edit) => {
                if kept.iter().any(|k| k.from < edit.to && edit.from < k.to) {
                    left_out.push(format!("line {}: the words are over words another suggestion changes", edit.line));
                } else {
                    kept.push(edit);
                }
            }
        }
    }
    // in the order of the file (two put in the same place stay in the order they were taken)
    kept.sort_by_key(|e| (e.from, e.to));

    let text = file.text();
    let mut out = String::new();
    let mut changed = false;
    // how many lines the hunks before have added (or taken)
    let mut delta: isize = 0;
    let mut at = 0;
    while at < kept.len() {
        // the changes whose contexts meet are one hunk
        let mut upto = at + 1;
        let mut region_last = kept[at].last_excl;
        while upto < kept.len() && kept[upto].first <= region_last + 2 * CONTEXT {
            region_last = region_last.max(kept[upto].last_excl);
            upto += 1;
        }
        let group = &kept[at..upto];
        at = upto;
        let region_first = group[0].first;
        let before = region_first.saturating_sub(CONTEXT);
        let after = (region_last + CONTEXT).min(raw.len());

        // the lines of the region as they are and as they become
        let mut new_text = String::new();
        let mut cursor = file.starts[region_first];
        for edit in group {
            new_text.push_str(&text[cursor..edit.from]);
            new_text.push_str(&edit.text);
            cursor = edit.to;
        }
        new_text.push_str(&text[cursor..file.starts[region_last]]);
        let mut new_lines: Vec<&str> = new_text.split('\n').collect();
        new_lines.pop();
        let old_lines: Vec<&str> = raw[region_first..region_last].iter().map(String::as_str).collect();

        let mut rows: Vec<(char, &str)> = Vec::new();
        rows.extend(raw[before..region_first].iter().map(|l| (' ', l.as_str())));
        let middle = line_diff(&old_lines, &new_lines);
        let does_change = middle.iter().any(|(sign, _)| *sign != ' ');
        rows.extend(middle);
        rows.extend(raw[region_last..after].iter().map(|l| (' ', l.as_str())));
        if !does_change {
            continue;
        }
        changed = true;

        let old_len = rows.iter().filter(|(sign, _)| *sign != '+').count();
        let new_len = rows.iter().filter(|(sign, _)| *sign != '-').count();
        let old_start = if old_len == 0 { before } else { before + 1 };
        let first_new = before.checked_add_signed(delta).unwrap_or(0);
        let new_start = if new_len == 0 { first_new } else { first_new + 1 };
        out.push_str(&format!("@@ -{old_start},{old_len} +{new_start},{new_len} @@\n"));
        for (sign, line) in rows {
            out.push(sign);
            out.push_str(line);
            out.push('\n');
        }
        delta += isize::try_from(new_lines.len()).unwrap_or(0) - isize::try_from(old_lines.len()).unwrap_or(0);
    }
    if !changed {
        return Ok(Diff { text: String::new(), changed: false, left_out });
    }
    Ok(Diff { text: format!("--- a/{path}\n+++ b/{path}\n{out}"), changed: true, left_out })
}

/// The header and the diffs of a patch: `files` are `(path, diff text)`; none for a conversation that changed nothing (the
/// header alone says how it ended).
#[must_use]
pub fn render(header: &Header<'_>, files: &[(String, String)]) -> String {
    let one_line = |text: &str| -> String {
        let joined = text.split_whitespace().collect::<Vec<_>>().join(" ");
        joined.chars().take(MAX_HEADER_CHARS).collect()
    };
    let mut out = String::from("AzReview-Patch: 1\n");
    out.push_str(&format!("Conversation: {}\n", header.conversation));
    out.push_str(&format!("Outcome: {}\n", header.outcome.name()));
    out.push_str(&format!("Base: {}\n", header.base.unwrap_or("none")));
    out.push_str(&format!("Time: {}\n", time_text(header.time_unix_ms)));
    out.push_str(&format!("Said: {}\n", one_line(header.said)));
    if let Some(understood) = header.understood.filter(|u| !u.trim().is_empty()) {
        out.push_str(&format!("Understood: {}\n", one_line(understood)));
    }
    out.push_str(&format!("Rationale: {}\n", one_line(header.rationale)));
    if files.is_empty() {
        out.push_str("Files: none\n");
    } else {
        let names: Vec<String> = files.iter().map(|(p, _)| one_line(p)).collect();
        out.push_str(&format!("Files: {}\n", names.join(", ")));
    }
    for sentence in header.left_out.iter().take(MAX_LEFT_OUT) {
        out.push_str(&format!("Left-out: {}\n", one_line(sentence)));
    }
    out.push('\n');
    for (path, diff) in files {
        out.push_str(&format!("diff --git a/{path} b/{path}\n{diff}"));
    }
    out
}

/// What a patch's header says, read back.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ReadHeader {
    pub conversation: u64,
    /// None for a word this version does not know.
    pub outcome: Option<Outcome>,
    /// The git revision; none for `none`.
    pub base: Option<String>,
    /// When it was resolved, as written: ISO 8601 UTC.
    pub time: String,
    pub said: String,
    pub understood: Option<String>,
    pub rationale: String,
    /// The files of the diff (none for a header alone).
    pub files: Vec<String>,
    pub left_out: Vec<String>,
}

/// The most lines of a header a reader looks at: a header is a dozen lines, a file that goes on is not one.
pub const MAX_HEADER_LINES: usize = 64;

/// The header of the text of a patch file, or `None` where the text is not one of ours (the first line is not
/// `AzReview-Patch: 1`) or says no conversation. A key it does not know is left out; the diff after the empty line is not
/// read.
#[must_use]
pub fn read_header(text: &str) -> Option<ReadHeader> {
    let mut lines = text.lines().map(|l| l.trim_end_matches('\r'));
    if lines.next()?.trim() != "AzReview-Patch: 1" {
        return None;
    }
    let mut out = ReadHeader::default();
    let mut conversation = None;
    for line in lines.take(MAX_HEADER_LINES) {
        if line.trim().is_empty() {
            break;
        }
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim();
        match key.trim() {
            "Conversation" => conversation = value.parse::<u64>().ok(),
            "Outcome" => out.outcome = Outcome::parse(value),
            "Base" => out.base = (value != "none" && !value.is_empty()).then(|| value.to_string()),
            "Time" => out.time = value.to_string(),
            "Said" => out.said = value.to_string(),
            "Understood" => out.understood = (!value.is_empty()).then(|| value.to_string()),
            "Rationale" => out.rationale = value.to_string(),
            "Files" => out.files = if value == "none" { Vec::new() } else { value.split(", ").map(str::to_string).filter(|f| !f.is_empty()).collect() },
            "Left-out" => out.left_out.push(value.to_string()),
            _ => {}
        }
    }
    out.conversation = conversation?;
    Some(out)
}

/// A Unix time in milliseconds as `(year, month, day, hour, minute, second)`, UTC (the civil calendar from a count of days,
/// in whole numbers).
fn civil(unix_ms: u64) -> (i64, i64, i64, i64, i64, i64) {
    let secs = i64::try_from(unix_ms / 1000).unwrap_or(i64::MAX / 2);
    let days = secs.div_euclid(86_400);
    let rest = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day, rest / 3_600, rest % 3_600 / 60, rest % 60)
}

/// `2025-11-26T05:40:12Z`.
#[must_use]
pub fn time_text(unix_ms: u64) -> String {
    let (y, mo, d, h, mi, s) = civil(unix_ms);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{s:02}Z")
}

/// The name of a patch: it sorts by the time and says which conversation (`20251126-054012-c7.patch`).
#[must_use]
pub fn file_name(conversation: u64, unix_ms: u64) -> String {
    let (y, mo, d, h, mi, s) = civil(unix_ms);
    format!("{y:04}{mo:02}{d:02}-{h:02}{mi:02}{s:02}-c{conversation}.patch")
}

/// The longest name of a patch file: its time, `-c`, the conversation's number and `.patch`.
pub const MAX_NAME_BYTES: usize = 64;

/// Whether `name` is the name of a patch file [`file_name`] makes (plain letters, digits, `-` and `.`, ending in `.patch`):
/// what is read back from an archive is checked before it is the name of a file to remove.
#[must_use]
pub fn is_name(name: &str) -> bool {
    name.len() <= MAX_NAME_BYTES
        && name.len() > ".patch".len()
        && name.ends_with(".patch")
        && !name.starts_with('.')
        && !name.contains("..")
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
}

#[cfg(test)]
#[path = "patch_test.rs"]
mod patch_test;
