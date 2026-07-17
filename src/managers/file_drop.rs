//! **File** drag & drop management
//!
//! Manages hovered files (drag-and-drop).

use alloc::vec::Vec;

use azul_css::AzString;

/// Manager for file drop state and hovered file tracking.
///
/// MWA-B7: stores ALL files of a drag/drop (multi-file drops were silently
/// truncated to the first path at every OS ingress site — the manager could
/// only hold one). The single-file accessors remain as first-element views
/// for existing callers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileDropManager {
    /// Files being hovered during the drag operation (empty = no hover).
    hovered_files: Vec<AzString>,
    /// Files that were dropped (cleared after one frame).
    dropped_files: Vec<AzString>,
    /// One-shot flag set when a hover ends without a drop (a non-empty →
    /// empty transition). Read by `determine_all_events` to emit
    /// `EventType::FileHoverCancel`, then cleared by the platform drag
    /// handler (one-shot, mirrors the dropped-files reset).
    hover_cancelled: bool,
}

impl FileDropManager {
    /// Create a new file drop manager
    #[must_use] pub const fn new() -> Self {
        Self {
            hovered_files: Vec::new(),
            dropped_files: Vec::new(),
            hover_cancelled: false,
        }
    }

    /// Set ALL currently hovered files (MWA-B7). An empty vec behaves like a
    /// drag-leave (latches the hover-cancel flag).
    pub fn set_hovered_files(&mut self, files: Vec<AzString>) {
        if files.is_empty() {
            if !self.hovered_files.is_empty() {
                self.hover_cancelled = true;
            }
            self.hovered_files.clear();
        } else {
            self.hovered_files = files;
        }
    }

    /// Single-file compatibility shim over [`set_hovered_files`](Self::set_hovered_files).
    ///
    /// Platform backends call this with `Some(path)` on drag-enter
    /// (macOS `draggingEntered`, Windows OLE `IDropTarget::DragEnter`) and
    /// `None` on drag-leave (`draggingExited` / `DragLeave`). A `Some` -> `None`
    /// transition latches [`FileDropManager::hover_was_cancelled`] so the
    /// `FileHoverCancel` event can fire.
    pub fn set_hovered_file(&mut self, file: Option<AzString>) {
        match file {
            Some(f) => self.set_hovered_files(alloc::vec![f]),
            None => self.set_hovered_files(Vec::new()),
        }
    }

    /// Whether a hover ended without a drop since the last
    /// [`FileDropManager::clear_hover_cancelled`] (one-shot).
    #[must_use] pub const fn hover_was_cancelled(&self) -> bool {
        self.hover_cancelled
    }

    /// Clear the one-shot hover-cancel flag. Called by the platform drag
    /// handler after `determine_all_events` has emitted the `FileHoverCancel`
    /// event (mirrors the dropped-files reset after `FileDrop`).
    pub const fn clear_hover_cancelled(&mut self) {
        self.hover_cancelled = false;
    }

    /// First hovered file (single-file view; use
    /// [`get_hovered_files`](Self::get_hovered_files) for the full list).
    #[must_use] pub fn get_hovered_file(&self) -> Option<&AzString> {
        self.hovered_files.first()
    }

    /// ALL currently hovered files (MWA-B7).
    #[must_use] pub fn get_hovered_files(&self) -> &[AzString] {
        &self.hovered_files
    }

    /// First dropped file (single-file view; use
    /// [`get_dropped_files`](Self::get_dropped_files) for the full list).
    #[must_use] pub fn get_dropped_file(&self) -> Option<&AzString> {
        self.dropped_files.first()
    }

    /// ALL files of the drop this frame (MWA-B7; one-shot, cleared by the
    /// platform handler after event processing).
    #[must_use] pub fn get_dropped_files(&self) -> &[AzString] {
        &self.dropped_files
    }

    /// Set ALL dropped files (MWA-B7; cleared after one frame).
    pub fn set_dropped_files(&mut self, files: Vec<AzString>) {
        self.dropped_files = files;
    }

    /// Single-file compatibility shim over [`set_dropped_files`](Self::set_dropped_files).
    pub fn set_dropped_file(&mut self, file: Option<AzString>) {
        match file {
            Some(f) => self.dropped_files = alloc::vec![f],
            None => self.dropped_files.clear(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &str) -> AzString {
        AzString::from(v.to_string())
    }

    #[test]
    fn multi_file_drop_keeps_every_path() {
        let mut m = FileDropManager::new();
        m.set_dropped_files(vec![s("/a"), s("/b"), s("/c")]);
        assert_eq!(m.get_dropped_files().len(), 3);
        assert_eq!(m.get_dropped_file().map(AzString::as_str), Some("/a"));
        m.set_dropped_file(None);
        assert!(m.get_dropped_files().is_empty());
    }

    #[test]
    fn hover_cancel_latches_on_empty_transition() {
        let mut m = FileDropManager::new();
        m.set_hovered_files(vec![s("/a"), s("/b")]);
        assert_eq!(m.get_hovered_files().len(), 2);
        assert!(!m.hover_was_cancelled());
        m.set_hovered_files(Vec::new());
        assert!(m.hover_was_cancelled());
        m.clear_hover_cancelled();
        assert!(!m.hover_was_cancelled());
    }
}
