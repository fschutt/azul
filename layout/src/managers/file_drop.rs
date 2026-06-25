//! **File** drag & drop management
//!
//! Manages hovered files (drag-and-drop).

use azul_css::AzString;

/// Manager for file drop state and hovered file tracking
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDropManager {
    /// File being hovered during drag-and-drop operation
    hovered_file: Option<AzString>,
    /// File that was dropped (cleared after one frame)
    dropped_file: Option<AzString>,
    /// One-shot flag set when a hover ends without a drop (a `Some` -> `None`
    /// transition through [`FileDropManager::set_hovered_file`]). Read by
    /// `determine_all_events` to emit `EventType::FileHoverCancel`, then
    /// cleared by the platform drag handler (one-shot, mirrors `dropped_file`).
    hover_cancelled: bool,
}

impl Default for FileDropManager {
    fn default() -> Self {
        Self::new()
    }
}

impl FileDropManager {
    /// Create a new file drop manager
    #[must_use] pub const fn new() -> Self {
        Self {
            hovered_file: None,
            dropped_file: None,
            hover_cancelled: false,
        }
    }

    /// Set the currently hovered file during drag operation.
    ///
    /// Platform backends call this with `Some(path)` on drag-enter
    /// (macOS `draggingEntered`, Windows OLE `IDropTarget::DragEnter`) and
    /// `None` on drag-leave (`draggingExited` / `DragLeave`). A `Some` -> `None`
    /// transition latches [`FileDropManager::hover_was_cancelled`] so the
    /// `FileHoverCancel` event can fire.
    pub fn set_hovered_file(&mut self, file: Option<AzString>) {
        if file.is_none() && self.hovered_file.is_some() {
            self.hover_cancelled = true;
        }
        self.hovered_file = file;
    }

    /// Whether a hover ended without a drop since the last
    /// [`FileDropManager::clear_hover_cancelled`] (one-shot).
    #[must_use] pub const fn hover_was_cancelled(&self) -> bool {
        self.hover_cancelled
    }

    /// Clear the one-shot hover-cancel flag. Called by the platform drag
    /// handler after `determine_all_events` has emitted the `FileHoverCancel`
    /// event (mirrors the `set_dropped_file(None)` reset after `FileDrop`).
    pub const fn clear_hover_cancelled(&mut self) {
        self.hover_cancelled = false;
    }

    /// Get the currently hovered file
    #[must_use] pub const fn get_hovered_file(&self) -> Option<&AzString> {
        self.hovered_file.as_ref()
    }

    /// Get the currently dropped file
    #[must_use] pub const fn get_dropped_file(&self) -> Option<&AzString> {
        self.dropped_file.as_ref()
    }

    /// Set the dropped file (should be cleared after one frame)
    pub fn set_dropped_file(&mut self, file: Option<AzString>) {
        self.dropped_file = file;
    }
}
