//! Windows clipboard integration
//!
//! Uses the `clipboard-win` crate to interface with the Windows clipboard API.
//!
//! - [`sync_clipboard`]: writes pending copy content from the [`ClipboardManager`]
//!   to the system clipboard and clears the manager state.
//! - [`get_clipboard_content`]: reads the current text content from the system clipboard.

use azul_layout::managers::clipboard::ClipboardManager;

/// Synchronize clipboard manager content to Windows system clipboard
///
/// This is called after user callbacks to commit clipboard changes.
/// If the clipboard manager has pending copy content, it's written to
/// the Windows clipboard via clipboard-win.
pub fn sync_clipboard(clipboard_manager: &mut ClipboardManager) {
    if let Some(content) = clipboard_manager.get_copy_content() {
        if write_to_clipboard(&content.plain_text).is_ok() {
            clipboard_manager.clear();
        }
    } else {
        clipboard_manager.clear();
    }
}

/// Write text to the Windows system clipboard
pub fn write_to_clipboard(text: &str) -> Result<(), ()> {
    use clipboard_win::{formats, set_clipboard};
    set_clipboard(formats::Unicode, text).map_err(|_| ())
}

/// Read content from Windows system clipboard
///
/// Returns the clipboard text content if available.
pub fn get_clipboard_content() -> Option<String> {
    use clipboard_win::{formats, get_clipboard};

    get_clipboard::<String, _>(formats::Unicode).ok()
}
