//! File drag & drop management
//!
//! Manages hovered files (drag-and-drop).

use azul_css::AzString;

/// Manager for cursor state and hovered file tracking
#[derive(Debug, Clone, PartialEq)]
pub struct FileDropManager {
    /// File being hovered during drag-and-drop operation
    pub hovered_file: Option<AzString>,
    /// File that was dropped (cleared after one frame)
    pub dropped_file: Option<AzString>,
}

impl Default for FileDropManager {
    fn default() -> Self {
        Self::new()
    }
}

impl FileDropManager {
    /// Create a new cursor manager
    pub fn new() -> Self {
        Self {
            hovered_file: None,
            dropped_file: None,
        }
    }

    /// Set the currently hovered file during drag operation
    pub fn set_hovered_file(&mut self, file: Option<AzString>) {
        self.hovered_file = file;
    }

    /// Get the currently hovered file
    pub fn get_hovered_file(&self) -> Option<&AzString> {
        self.hovered_file.as_ref()
    }

    /// Set the dropped file (should be cleared after one frame)
    pub fn set_dropped_file(&mut self, file: Option<AzString>) {
        self.dropped_file = file;
    }

    /// Get and clear the dropped file (one-shot event)
    pub fn take_dropped_file(&mut self) -> Option<AzString> {
        self.dropped_file.take()
    }

    /// Clear all state
    pub fn clear(&mut self) {
        self.hovered_file = None;
        self.dropped_file = None;
    }
}
