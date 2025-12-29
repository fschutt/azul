//! Wayland clipboard integration
//!
//! Uses x11-clipboard crate which also supports Wayland via the same API

use std::time::Duration;

use azul_layout::managers::clipboard::ClipboardManager;
use x11_clipboard::Clipboard;

use crate::{log_debug, log_error, log_info, log_warn, log_trace};
use super::super::super::common::debug_server::LogCategory;

/// Synchronize clipboard manager content to Wayland system clipboard
///
/// This is called after user callbacks to commit clipboard changes.
/// If the clipboard manager has pending copy content, it's written to
/// the Wayland clipboard.
pub fn sync_clipboard(clipboard_manager: &mut ClipboardManager) {
    // Check if there's pending content to copy
    if let Some(content) = clipboard_manager.get_copy_content() {
        // Write to Wayland clipboard
        if let Err(e) = write_to_clipboard(&content.plain_text) {
            log_error!(LogCategory::Resources, "[Wayland Clipboard] Failed to write: {:?}", e);
        }
    }

    // Clear the clipboard manager after sync
    clipboard_manager.clear();
}

/// Read content from Wayland system clipboard
///
/// Returns the clipboard text content if available.
pub fn get_clipboard_content() -> Option<String> {
    read_from_clipboard().ok()
}

/// Write string to Wayland clipboard
pub fn write_to_clipboard(text: &str) -> Result<(), ClipboardError> {
    let clipboard = Clipboard::new().map_err(|_| ClipboardError::InitFailed)?;

    clipboard
        .store(
            clipboard.setter.atoms.clipboard,
            clipboard.setter.atoms.utf8_string,
            text,
        )
        .map_err(|_| ClipboardError::WriteFailed)
}

/// Read string from Wayland clipboard
fn read_from_clipboard() -> Result<String, ClipboardError> {
    let clipboard = Clipboard::new().map_err(|_| ClipboardError::InitFailed)?;

    let data = clipboard
        .load(
            clipboard.getter.atoms.clipboard,
            clipboard.getter.atoms.utf8_string,
            clipboard.getter.atoms.property,
            Duration::from_secs(3),
        )
        .map_err(|_| ClipboardError::ReadFailed)?;

    String::from_utf8(data).map_err(|_| ClipboardError::EncodingError)
}

#[derive(Debug)]
enum ClipboardError {
    InitFailed,
    WriteFailed,
    ReadFailed,
    EncodingError,
}
