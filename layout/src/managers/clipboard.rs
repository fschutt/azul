//! Clipboard Manager
//!
//! Manages clipboard content flow between the system clipboard and 
//! the application.
//!
//! ## Architecture
//!
//! The clipboard manager acts as a bridge between system clipboard 
//! operations and user callbacks:
//!
//! 1. **Paste Flow**: System clipboard → ClipboardManager → User Callback → TextInputManager
//! 
//!    - When Ctrl+V is pressed, event_v2 reads system clipboard and calls `set_paste_content()`
//!    - User's On::Paste callback can inspect content via `get_clipboard_content()`
//!    - User can modify/block paste by not calling the default paste action
//!    - After callback, content is cleared for next operation
//!
//! 2. **Copy Flow**: Selection → User Callback → ClipboardManager → System clipboard
//! 
//!    - When Ctrl+C is pressed, user's On::Copy callback fires
//!    - Callback can inspect selected content and override via `set_copy_content()`
//!    - event_v2 calls `get_copy_content()` to get final content (override or default)
//!    - Content is written to system clipboard via platform sync
//!    - After callback, content is cleared for next operation
//!
//! 3. **Cut Flow**: Same as Copy + delete selection

use crate::managers::selection::ClipboardContent;

/// Manages clipboard content flow between system clipboard and application
///
/// This manager temporarily holds clipboard content during clipboard operations,
/// allowing user callbacks to inspect and modify content before it's committed
/// to the system clipboard or pasted into the document.
#[derive(Debug, Clone, Default)]
pub struct ClipboardManager {
    /// Content from system clipboard when paste is triggered
    /// Available to user callbacks via `CallbackInfo::get_clipboard_content()`
    pending_paste_content: Option<ClipboardContent>,

    /// Content to be written to system clipboard after copy/cut
    /// Set by user callbacks via `CallbackInfo::set_copy_content()`
    pending_copy_content: Option<ClipboardContent>,
}

impl ClipboardManager {
    
    /// Create a new empty clipboard manager
    pub fn new() -> Self {
        Self {
            pending_paste_content: None,
            pending_copy_content: None,
        }
    }

    // Paste Operations (System → Application)

    /// Sets content from the system clipboard (called before paste callbacks).
    pub fn set_paste_content(&mut self, content: ClipboardContent) {
        self.pending_paste_content = Some(content);
    }

    /// Returns the pending paste content, if any.
    pub fn get_paste_content(&self) -> Option<&ClipboardContent> {
        self.pending_paste_content.as_ref()
    }

    // Copy Operations (Application → System)

    /// Sets content to be copied to the system clipboard.
    pub fn set_copy_content(&mut self, content: ClipboardContent) {
        self.pending_copy_content = Some(content);
    }

    /// Returns the pending copy content, if any.
    pub fn get_copy_content(&self) -> Option<&ClipboardContent> {
        self.pending_copy_content.as_ref()
    }

    /// Takes the copy content, consuming it.
    pub fn take_copy_content(&mut self) -> Option<ClipboardContent> {
        self.pending_copy_content.take()
    }

    // Lifecycle Management

    /// Clears all pending clipboard content.
    pub fn clear(&mut self) {
        self.pending_paste_content = None;
        self.pending_copy_content = None;
    }

    /// Clears only paste content.
    pub fn clear_paste(&mut self) {
        self.pending_paste_content = None;
    }

    /// Clears only copy content.
    pub fn clear_copy(&mut self) {
        self.pending_copy_content = None;
    }

    /// Returns `true` if there's pending paste content.
    pub fn has_paste_content(&self) -> bool {
        self.pending_paste_content.is_some()
    }

    /// Returns `true` if there's pending copy content.
    pub fn has_copy_content(&self) -> bool {
        self.pending_copy_content.is_some()
    }
}
