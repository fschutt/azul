//! Finding the code a cutout points at again, after the code has moved on. A cutout keeps the file, the lines, a hash of them
//! and what they said when they were circled; when the code is at a newer revision the reference is re-located against the
//! files as they are now:
//!
//! - the file is as it was, or the lines at the same place are, by their hash: **unchanged** (the reference only names the new
//!   revision);
//! - the same lines are somewhere else in the file: **moved** (found by their hash: the reference points at the new place);
//! - the lines are not to be found as they were but their first and last distinctive lines are (the code between them
//!   changed): **changed** (the reference points at where they begin and end now, with what is there now);
//! - nothing of them is there, or the file is gone: **vanished** (the reference is left as it was, and is flagged).
//!
//! A reference that moved, changed or vanished is where the reason for a snapshot is usually needed, so the board says so.
//! Pure: the files are asked through [`FileView`].

use crate::{
    whiteboard::{kept_lines, lines_hash, Limits, Source},
    whiteboard_git::is_hash,
    whiteboard_hash,
    whiteboard_jump::FileView,
};

/// What became of a reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefStatus {
    Unchanged,
    Moved,
    Changed,
    Vanished,
}

impl RefStatus {
    /// The name a document writes.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Unchanged => "unchanged",
            Self::Moved => "moved",
            Self::Changed => "changed",
            Self::Vanished => "vanished",
        }
    }

    /// The status a document names; `None` for any other word.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "unchanged" => Self::Unchanged,
            "moved" => Self::Moved,
            "changed" => Self::Changed,
            "vanished" => Self::Vanished,
            _ => return None,
        })
    }

    /// Whether the board should say something: everything but "as it was".
    #[must_use]
    pub const fn is_flag(self) -> bool {
        !matches!(self, Self::Unchanged)
    }
}

/// A reference and what became of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Relocation {
    pub status: RefStatus,
    /// The lines it pointed at (first, last).
    pub from: (u32, u32),
    /// The lines it points at now (the same ones for a reference that did not move or is gone).
    pub to: (u32, u32),
    /// The reference as it is now: the new place, what the lines say there, the revision of the code now. A vanished one is
    /// as it was.
    pub source: Source,
}

/// A line that says something: at least four letters or digits (`}`, `);` and `// ---` are found everywhere).
fn distinctive(line: &str) -> bool {
    line.chars().filter(|c| c.is_alphanumeric()).count() >= 4
}

/// The reference `source` as the files are now (`rev_now`: the revision the code is at, when it is known).
#[must_use]
pub fn relocate(source: &Source, files: &dyn FileView, rev_now: Option<&str>, limits: &Limits) -> Relocation {
    let from = (source.first_line, source.last_line);
    let span = source.last_line.saturating_sub(source.first_line);
    let rev = rev_now.filter(|r| is_hash(r)).map_or_else(|| source.rev.clone(), str::to_string);
    let Some(file_hash) = files.file_hash(&source.file) else {
        return Relocation { status: RefStatus::Vanished, from, to: from, source: source.clone() };
    };
    // The reference as it is at `at` now.
    let at = |status: RefStatus, first: u32, last: u32| -> Relocation {
        let covered = files.lines(&source.file, first, last).unwrap_or_default();
        let kept = kept_lines(&covered, limits);
        Relocation {
            status,
            from,
            to: (first, last),
            source: Source {
                first_line: first,
                last_line: last,
                file_hash,
                range_hash: lines_hash(&kept),
                lines: kept,
                sha256: whiteboard_hash::lines_tagged(&covered),
                rev: rev.clone(),
                ..source.clone()
            },
        }
    };
    // Intact where it was: the file did not change, or the lines did not.
    let here = files.lines(&source.file, source.first_line, source.last_line);
    if file_hash == source.file_hash || here.as_ref().is_some_and(|now| lines_hash(&kept_lines(now, limits)) == source.range_hash) {
        let mut same = at(RefStatus::Unchanged, source.first_line, source.last_line);
        // What the lines say is what it was when it was circled: nothing is made up for a reference that did not change.
        same.source.lines.clone_from(&source.lines);
        same.source.range_hash = source.range_hash;
        if !source.sha256.is_empty() {
            same.source.sha256.clone_from(&source.sha256);
        }
        return same;
    }
    // The same lines elsewhere.
    if !source.lines.is_empty() {
        if let Some(found) = files.find_block(&source.file, &source.lines, source.first_line) {
            return at(RefStatus::Moved, found, found.saturating_add(span));
        }
    }
    // Their first and last lines that say something, elsewhere.
    let first_line = source.lines.iter().find(|l| distinctive(l));
    let last_line = source.lines.iter().rev().find(|l| distinctive(l));
    let a = first_line.and_then(|l| files.find_line(&source.file, l, source.first_line));
    let b = last_line.and_then(|l| files.find_line(&source.file, l, source.last_line));
    let count = files.line_count(&source.file).unwrap_or(u32::MAX);
    let found = match (a, b) {
        (Some(a), Some(b)) if b >= a && b - a <= span.saturating_mul(3).saturating_add(10) => Some((a, b)),
        (Some(a), _) => Some((a, a.saturating_add(span).min(count))),
        (None, Some(b)) => Some((b.saturating_sub(span).max(1), b)),
        (None, None) => None,
    };
    match found {
        Some((first, last)) => at(RefStatus::Changed, first, last.max(first)),
        None => Relocation { status: RefStatus::Vanished, from, to: from, source: source.clone() },
    }
}

#[cfg(test)]
#[path = "whiteboard_relocate_test.rs"]
mod whiteboard_relocate_test;
