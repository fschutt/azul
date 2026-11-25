//! The jump from a cutout back to its lines: a double click on a cutout goes to its file and lines
//! and flashes them. This file is what is decided before the app goes: where, and whether what is
//! there is what was circled. The files are asked through [`FileView`] (a folder's files may be many
//! and kept lazily: the board never holds one), the answer is a plain [`JumpTarget`], and the app does
//! the going. No window, no azul.

use crate::whiteboard::{kept_lines, lines_hash, Board, Cutout, ItemId, Notice, LIMITS};

/// What the jump may ask of the workspace's files.
pub trait FileView {
    /// [`lines_hash`] of the whole file as it is now (`None`: the file is gone).
    fn file_hash(&self, file: &str) -> Option<u64>;
    /// How many lines the file has now.
    fn line_count(&self, file: &str) -> Option<u32>;
    /// The lines `first` to `last` (from 1, both included) as they are now, `None` when the file is gone or
    /// is shorter than that.
    fn lines(&self, file: &str, first: u32, last: u32) -> Option<Vec<String>>;
    /// Where in the file the lines `block` are now (the first line of the nearest place to `near`), when
    /// the file can look: a file kept in pieces may not, and the answer is then `None`.
    fn find_block(&self, _file: &str, _block: &[String], _near: u32) -> Option<u32> {
        None
    }
}

/// How the lines are now, against how they were when they were circled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    /// The file is as it was.
    Fresh,
    /// The file changed but the circled lines are as they were, where they were.
    FileChanged,
    /// The circled lines are somewhere else in the file now; the jump goes there.
    LinesMoved,
    /// The circled lines are not as they were and are nowhere else; the jump goes to where they were.
    LinesChanged,
    /// The file is gone; the jump has nowhere to go but the lines it was.
    FileMissing,
}

impl Freshness {
    /// The key of what the person is told, `None` when there is nothing to tell.
    #[must_use]
    pub const fn key(self) -> Option<&'static str> {
        match self {
            Self::Fresh => None,
            Self::FileChanged => Some("azreview-whiteboard-jump-file-changed"),
            Self::LinesMoved => Some("azreview-whiteboard-jump-lines-moved"),
            Self::LinesChanged => Some("azreview-whiteboard-jump-lines-changed"),
            Self::FileMissing => Some("azreview-whiteboard-jump-file-missing"),
        }
    }

    /// What is told, in English (`first` and `last`: where the jump goes).
    #[must_use]
    pub fn english(self, first: u32, last: u32) -> String {
        match self {
            Self::Fresh => String::new(),
            Self::FileChanged => "The file has changed since this was circled; these lines are as they were.".to_string(),
            Self::LinesMoved => format!("These lines moved: they are now at lines {first} to {last}."),
            Self::LinesChanged => "These lines have changed since they were circled.".to_string(),
            Self::FileMissing => "The file is no longer there.".to_string(),
        }
    }
}

/// Where a double click on a cutout goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JumpTarget {
    /// The cutout.
    pub id: ItemId,
    /// The file, relative to the workspace.
    pub file: String,
    /// The lines to flash (from 1, both included).
    pub first_line: u32,
    pub last_line: u32,
    pub freshness: Freshness,
}

impl JumpTarget {
    /// The page (from 0) of a file with `lines_per_page` lines to a page that the first line is on.
    #[must_use]
    pub fn page(&self, lines_per_page: usize) -> usize {
        if lines_per_page == 0 {
            0
        } else {
            (self.first_line.saturating_sub(1) as usize) / lines_per_page
        }
    }
}

/// The first line (from 1) of the place the lines `block` are at in `lines`, the one nearest `near`
/// (the earlier of two as near); `None` for an empty block or when they are not there.
#[must_use]
pub fn find_block_in(lines: &[String], block: &[String], near: u32) -> Option<u32> {
    if block.is_empty() || block.len() > lines.len() {
        return None;
    }
    let mut best: Option<(u32, u32)> = None;
    for start in 0..=(lines.len() - block.len()) {
        if lines[start..start + block.len()] == *block {
            let first = u32::try_from(start + 1).unwrap_or(u32::MAX);
            let d = first.abs_diff(near);
            if best.is_none_or(|(_, bd)| d < bd) {
                best = Some((first, d));
            }
        }
    }
    best.map(|(first, _)| first)
}

/// Where cutout `c` goes now, and what the lines are like.
#[must_use]
pub fn jump_target(c: &Cutout, files: &dyn FileView) -> JumpTarget {
    let s = &c.source;
    let target = |first: u32, last: u32, freshness| JumpTarget {
        id: c.id,
        file: s.file.clone(),
        first_line: first,
        last_line: last,
        freshness,
    };
    let Some(hash) = files.file_hash(&s.file) else {
        return target(s.first_line, s.last_line, Freshness::FileMissing);
    };
    if hash == s.file_hash {
        return target(s.first_line, s.last_line, Freshness::Fresh);
    }
    let intact = files
        .lines(&s.file, s.first_line, s.last_line)
        .is_some_and(|now| lines_hash(&kept_lines(&now, &LIMITS)) == s.range_hash);
    if intact {
        return target(s.first_line, s.last_line, Freshness::FileChanged);
    }
    if let Some(at) = files.find_block(&s.file, &s.lines, s.first_line) {
        let span = s.last_line.saturating_sub(s.first_line);
        return target(at, at.saturating_add(span), Freshness::LinesMoved);
    }
    // The lines are not as they were and not elsewhere: the old place, kept inside what the file has.
    let end = files.line_count(&s.file).unwrap_or(0).max(1);
    let (first, last) = (s.first_line.min(end), s.last_line.min(end));
    target(first, last.max(first), Freshness::LinesChanged)
}

/// The jump of cutout `id`: where it goes now. The board's observer hears it.
///
/// # Errors
/// There is no such cutout.
pub fn jump(board: &mut Board, id: ItemId, files: &dyn FileView) -> Result<JumpTarget, Notice> {
    let Some(cutout) = board.cutout(id) else {
        return Err(Notice::NoSuchItem { id });
    };
    let target = jump_target(cutout, files);
    board.note_jump(id, &target.file, target.first_line, target.last_line);
    Ok(target)
}

#[cfg(test)]
#[path = "whiteboard_jump_test.rs"]
mod whiteboard_jump_test;
