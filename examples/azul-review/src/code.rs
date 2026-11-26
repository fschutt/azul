//! The file under review as the pages show it: its lines, in pages of [`LINES_PER_PAGE`].
//!
//! What the workspace holds is the tree's (`tree.rs`), read through a `FileSource` (`symbols_source.rs`); this is only
//! the content of the one file that is open.

use std::{borrow::Cow, path::PathBuf, sync::Arc};

pub const LINES_PER_PAGE: usize = 46;

/// The largest file that is shown, bytes: a bigger one is not read (the window says why).
pub const MAX_OPEN_BYTES: usize = 64 * 1024 * 1024;

/// The largest file held as a list of lines, bytes. A bigger one is held as its bytes and the start of each line
/// ([`BigText`]): four bytes a line instead of a string (24 bytes and an allocation, whatever the line is), and the lines
/// of the page on the screen are made when the page is.
pub const WHOLE_LIMIT: usize = 8 * 1024 * 1024;

/// A text that is too big to hold as lines: its bytes, and where each line starts. The memory it takes is the file's
/// size and four bytes a line, whatever the lines are.
#[derive(Clone, Default)]
pub struct BigText {
    bytes: Vec<u8>,
    /// The offset of each line, and one more: where the last one ends. `starts.len() - 1` lines.
    starts: Vec<u32>,
}

impl std::fmt::Debug for BigText {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "BigText({} bytes, {} lines)", self.bytes.len(), self.line_count())
    }
}

impl BigText {
    /// The longest text a line start can say (a `u32` offset).
    pub const MAX_BYTES: usize = u32::MAX as usize;

    /// The text of `bytes`; `None` where it is longer than [`Self::MAX_BYTES`].
    #[must_use]
    pub fn try_new(bytes: Vec<u8>) -> Option<Self> {
        if bytes.len() > Self::MAX_BYTES {
            return None;
        }
        Some(Self::build(bytes))
    }

    /// The text of `bytes`, which must be no longer than [`Self::MAX_BYTES`] (a longer one has no lines).
    #[must_use]
    pub fn new(bytes: Vec<u8>) -> Self {
        Self::try_new(bytes).unwrap_or_default()
    }

    fn build(bytes: Vec<u8>) -> Self {
        let mut starts = Vec::with_capacity(bytes.len() / 32 + 2);
        starts.push(0);
        for (i, b) in bytes.iter().enumerate() {
            if *b == b'\n' {
                starts.push(u32::try_from(i + 1).unwrap_or(u32::MAX));
            }
        }
        // a last line that has no end of its own
        if starts.last().is_some_and(|&last| (last as usize) < bytes.len()) {
            starts.push(u32::try_from(bytes.len()).unwrap_or(u32::MAX));
        }
        Self { bytes, starts }
    }

    #[must_use]
    pub fn line_count(&self) -> usize {
        self.starts.len().saturating_sub(1)
    }

    /// Line `i` as a page shows it (as `lines_of` makes it); nothing past the last.
    #[must_use]
    pub fn line(&self, i: usize) -> String {
        let (Some(&a), Some(&b)) = (self.starts.get(i), self.starts.get(i + 1)) else {
            return String::new();
        };
        let mut line = &self.bytes[a as usize..b as usize];
        if line.last() == Some(&b'\n') {
            line = &line[..line.len() - 1];
            if line.last() == Some(&b'\r') {
                line = &line[..line.len() - 1];
            }
        }
        String::from_utf8_lossy(line).replace('\t', "    ")
    }

    /// The memory the text holds, bytes.
    #[must_use]
    pub fn heap_bytes(&self) -> usize {
        self.bytes.len() + self.starts.len() * 4
    }
}

/// The file under review: its lines, or - for a file too big to hold them - its text.
#[derive(Debug, Clone)]
pub struct SourceFile {
    pub path: PathBuf,
    pub display: String,
    /// The lines of a file held as lines (empty for a big one).
    pub lines: Vec<String>,
    /// The text of a file too big to hold as lines.
    pub big: Option<Arc<BigText>>,
}

impl SourceFile {
    /// A file held as lines.
    #[must_use]
    pub fn new(path: PathBuf, display: String, lines: Vec<String>) -> Self {
        Self { path, display, lines, big: None }
    }

    /// A file held as its text.
    #[must_use]
    pub fn big(path: PathBuf, display: String, text: BigText) -> Self {
        Self { path, display, lines: Vec::new(), big: Some(Arc::new(text)) }
    }

    /// How many lines the file has.
    #[must_use]
    pub fn line_count(&self) -> usize {
        self.big.as_ref().map_or(self.lines.len(), |b| b.line_count())
    }

    pub fn page_count(&self) -> usize {
        self.line_count().div_ceil(LINES_PER_PAGE).max(1)
    }

    /// The number of the first line of page `page` (the first line of the file is 1).
    #[must_use]
    pub const fn first_line_of(page: usize) -> usize {
        page * LINES_PER_PAGE + 1
    }

    /// The first line of `page` and its lines (none past the end of the file): borrowed where the file is held as
    /// lines, made from the text where it is big.
    pub fn page(&self, page: usize) -> (usize, Cow<'_, [String]>) {
        let start = page * LINES_PER_PAGE;
        let first = start + 1;
        let total = self.line_count();
        if start >= total {
            return (first, Cow::Borrowed(&[]));
        }
        let end = (start + LINES_PER_PAGE).min(total);
        match &self.big {
            None => (first, Cow::Borrowed(&self.lines[start..end])),
            Some(text) => (first, Cow::Owned((start..end).map(|i| text.line(i)).collect())),
        }
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

/// The file `path` (shown as `display`) of these bytes: held as lines up to [`WHOLE_LIMIT`], as its text above it. The
/// bytes of a binary file are refused.
///
/// # Errors
/// The file holds a NUL byte in its first 8 KiB, or is longer than a text can say.
pub fn open_bytes(path: PathBuf, display: String, bytes: Vec<u8>) -> Result<SourceFile, NotShown> {
    if bytes[..bytes.len().min(8192)].contains(&0) {
        return Err(NotShown::Binary);
    }
    if bytes.len() <= WHOLE_LIMIT {
        let lines = lines_of(&bytes)?;
        return Ok(SourceFile::new(path, display, lines));
    }
    let size = bytes.len() as u64;
    BigText::try_new(bytes)
        .map(|text| SourceFile::big(path, display, text))
        .ok_or(NotShown::TooLarge(size))
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
