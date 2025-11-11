//! Windows clipboard integration
//!
//! Uses clipboard-win crate to interface with Windows clipboard API

use azul_layout::managers::clipboard::ClipboardManager;

/// Synchronize clipboard manager content to Windows system clipboard
///
/// This is called after user callbacks to commit clipboard changes.
/// If the clipboard manager has pending copy content, it's written to
/// the Windows clipboard via clipboard-win.
pub fn sync_clipboard(clipboard_manager: &mut ClipboardManager) {
    use clipboard_win::{formats, set_clipboard};

    // Check if there's pending content to copy
    if let Some(content) = clipboard_manager.get_copy_content() {
        // Write plain text to clipboard
        let _ = set_clipboard(formats::Unicode, &content.plain_text);
    }

    // Clear the clipboard manager after sync
    clipboard_manager.clear();
}

/// Read content from Windows system clipboard
///
/// Returns the clipboard text content if available.
pub fn get_clipboard_content() -> Option<String> {
    use clipboard_win::{formats, get_clipboard};

    get_clipboard::<String, _>(formats::Unicode).ok()
}
