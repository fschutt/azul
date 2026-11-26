//! The file under review as the pages show it: its lines, in pages of [`LINES_PER_PAGE`].
//!
//! What the workspace holds is the tree's (`tree.rs`), read through a `FileSource` (`symbols_source.rs`); this is only
//! the content of the one file that is open.

use std::path::PathBuf;

pub const LINES_PER_PAGE: usize = 46;

/// The largest file that is shown, bytes: a bigger one is not read (the tree says why).
pub const MAX_OPEN_BYTES: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct SourceFile {
    pub path: PathBuf,
    pub display: String,
    pub lines: Vec<String>,
}

impl SourceFile {
    pub fn page_count(&self) -> usize {
        self.lines.len().div_ceil(LINES_PER_PAGE).max(1)
    }

    pub fn page(&self, page: usize) -> (usize, &[String]) {
        let start = page * LINES_PER_PAGE;
        if start >= self.lines.len() {
            return (start + 1, &[]);
        }
        let end = (start + LINES_PER_PAGE).min(self.lines.len());
        (start + 1, &self.lines[start..end])
    }
}

/// Why a file is not shown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotShown {
    /// It holds a NUL byte: it is no text.
    Binary,
    /// It is over [`MAX_OPEN_BYTES`] (the number of bytes it has).
    TooLarge(u64),
    /// It could not be read (what the system said).
    Unreadable(String),
}

/// The lines of the bytes of a file as a page shows them: text that is not UTF-8 is read with its bad bytes replaced, a
/// tab is four spaces (a column is a character), `\r\n` ends a line as `\n` does. The bytes of a binary file are refused.
///
/// # Errors
/// The file holds a NUL byte in its first 8 KiB.
pub fn lines_of(bytes: &[u8]) -> Result<Vec<String>, NotShown> {
    if bytes[..bytes.len().min(8192)].contains(&0) {
        return Err(NotShown::Binary);
    }
    Ok(String::from_utf8_lossy(bytes)
        .lines()
        .map(|l| l.replace('\t', "    "))
        .collect())
}

#[cfg(test)]
#[path = "code_test.rs"]
mod code_test;
