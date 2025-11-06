//! Clipboard Manager
//!
//! Manages clipboard content flow between the system clipboard and the application.
//!
//! ## Architecture
//!
//! The clipboard manager acts as a bridge between system clipboard operations and user callbacks:
//!
//! 1. **Paste Flow**: System clipboard → ClipboardManager → User Callback → TextInputManager
//!    - When Ctrl+V is pressed, event_v2 reads system clipboard and calls `set_paste_content()`
//!    - User's On::Paste callback can inspect content via `get_clipboard_content()`
//!    - User can modify/block paste by not calling the default paste action
//!    - After callback, content is cleared for next operation
//!
//! 2. **Copy Flow**: Selection → User Callback → ClipboardManager → System clipboard
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

    // ===== Paste Operations (System → Application) =====

    /// Set content from system clipboard (called by event handler before paste callbacks)
    ///
    /// This is called by the event loop when Ctrl+V is pressed, BEFORE firing user callbacks.
    /// The content becomes available to user callbacks via `get_clipboard_content()`.
    pub fn set_paste_content(&mut self, content: ClipboardContent) {
        self.pending_paste_content = Some(content);
    }

    /// Get content available for pasting (called by user callbacks)
    ///
    /// Returns the content from the system clipboard if a paste operation is in progress.
    /// This is typically called from On::Paste callbacks to inspect what will be pasted.
    ///
    /// Returns `None` if no paste operation is active.
    pub fn get_paste_content(&self) -> Option<&ClipboardContent> {
        self.pending_paste_content.as_ref()
    }

    // ===== Copy Operations (Application → System) =====

    /// Set content to be copied to system clipboard (called by user callbacks)
    ///
    /// This is called by user callbacks (via `CallbackInfo::set_copy_content()`) to override
    /// the default clipboard content. If not set, the default selected text is used.
    pub fn set_copy_content(&mut self, content: ClipboardContent) {
        self.pending_copy_content = Some(content);
    }

    /// Get content to be copied to system clipboard (called by event handler after callbacks)
    ///
    /// Returns the overridden content if user callback set it, otherwise `None`.
    /// The event loop should use this if available, otherwise fall back to selected text.
    pub fn get_copy_content(&self) -> Option<&ClipboardContent> {
        self.pending_copy_content.as_ref()
    }

    /// Take the copy content, consuming it (for event handler after writing to clipboard)
    pub fn take_copy_content(&mut self) -> Option<ClipboardContent> {
        self.pending_copy_content.take()
    }

    // ===== Lifecycle Management =====

    /// Clear all pending clipboard content
    ///
    /// This should be called after clipboard operations are complete to reset the manager
    /// for the next operation. Typically called by the platform's `sync_clipboard()` method.
    pub fn clear(&mut self) {
        self.pending_paste_content = None;
        self.pending_copy_content = None;
    }

    /// Clear only paste content (after paste operation completes)
    pub fn clear_paste(&mut self) {
        self.pending_paste_content = None;
    }

    /// Clear only copy content (after copy operation completes)
    pub fn clear_copy(&mut self) {
        self.pending_copy_content = None;
    }

    /// Check if there's pending paste content
    pub fn has_paste_content(&self) -> bool {
        self.pending_paste_content.is_some()
    }

    /// Check if there's pending copy content
    pub fn has_copy_content(&self) -> bool {
        self.pending_copy_content.is_some()
    }
}
