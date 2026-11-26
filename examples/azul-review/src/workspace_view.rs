//! The workspace's files as the whiteboard's jump asks for them (`whiteboard_jump::FileView`): any file of the folder,
//! read through the `FileSource` when it is asked for, as the page shows its lines; a big file by its text, so asking for
//! a hundred lines of a 60 MB file holds the file and makes a hundred strings.
//!
//! One file is kept: the questions about a file come together (is it as it was, where are its lines), and a second file
//! takes the place of the first.

use std::{cell::RefCell, path::PathBuf, sync::Arc};

use crate::{
    code::{self, SourceFile},
    symbols_source::FileSource,
    whiteboard_jump::FileView,
};

/// The files of a workspace for the jump.
pub struct WorkspaceView {
    source: Arc<dyn FileSource>,
    root: PathBuf,
    /// The last file asked for: its path and what was read.
    kept: RefCell<Option<(String, Option<Arc<SourceFile>>)>>,
}

impl WorkspaceView {
    #[must_use]
    pub fn new(source: Arc<dyn FileSource>, root: PathBuf) -> Self {
        Self { source, root, kept: RefCell::new(None) }
    }

    /// The file `file` (a workspace-relative path) as it is now; `None` when it is not there, is no text or is too big.
    fn read(&self, file: &str) -> Option<Arc<SourceFile>> {
        if let Some((path, read)) = &*self.kept.borrow() {
            if path == file {
                return read.clone();
            }
        }
        let read = self
            .source
            .read(file, code::MAX_OPEN_BYTES)
            .ok()
            .and_then(|bytes| code::open_bytes(self.root.join(file), file.to_string(), bytes).ok())
            .map(Arc::new);
        *self.kept.borrow_mut() = Some((file.to_string(), read.clone()));
        read
    }
}

/// FNV-1a as `whiteboard::lines_hash` has it, one line at a time (a big file is never held as a list of lines).
fn fold(mut h: u64, line: &str) -> u64 {
    for b in line.bytes().chain(std::iter::once(b'\n')) {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// What a file hashes to as the whiteboard keeps it (`whiteboard::lines_hash` of its lines; of its text a line at a time when
/// it is too big to be held as lines): what a cutout says of the file it comes from, and what a jump compares the file to.
#[must_use]
pub fn hash_of_file(f: &SourceFile) -> u64 {
    match &f.big {
        None => crate::whiteboard::lines_hash(&f.lines),
        Some(text) => (0..text.line_count()).fold(0xcbf2_9ce4_8422_2325, |h, i| fold(h, &text.line(i))),
    }
}

impl FileView for WorkspaceView {
    fn file_hash(&self, file: &str) -> Option<u64> {
        let f = self.read(file)?;
        Some(hash_of_file(&f))
    }

    fn line_count(&self, file: &str) -> Option<u32> {
        let f = self.read(file)?;
        u32::try_from(f.line_count()).ok()
    }

    fn lines(&self, file: &str, first: u32, last: u32) -> Option<Vec<String>> {
        let f = self.read(file)?;
        let (first, last) = (first as usize, last as usize);
        if first == 0 || last < first || last > f.line_count() {
            return None;
        }
        Some(match &f.big {
            None => f.lines[first - 1..last].to_vec(),
            Some(text) => (first - 1..last).map(|i| text.line(i)).collect(),
        })
    }
}

#[cfg(test)]
#[path = "workspace_view_test.rs"]
mod workspace_view_test;
