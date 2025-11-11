//! X11 clipboard integration using x11-clipboard crate
//!
//! This module provides clipboard synchronization between azul-layout's ClipboardManager
//! and the X11 system clipboard (both PRIMARY and CLIPBOARD selections).

use std::time::Duration;

use azul_layout::managers::clipboard::ClipboardManager;
use x11_clipboard::Clipboard;

/// Synchronize clipboard manager content to X11 system clipboard
///
/// This is called after user callbacks to commit clipboard changes.
/// If the clipboard manager has pending copy content, it's written to
/// both the CLIPBOARD and PRIMARY X11 selections.
pub fn sync_clipboard(clipboard_manager: &mut ClipboardManager) {
    // Check if there's pending content to copy
    if let Some(content) = clipboard_manager.get_copy_content() {
        // Write to X11 clipboard
        let _ = write_to_clipboard(&content.plain_text);
    }

    // Clear the clipboard manager after sync
    clipboard_manager.clear();
}

/// Write text to the X11 clipboard
///
/// Writes to both CLIPBOARD (Ctrl+C/V) and PRIMARY (middle-click) selections.
/// Returns Ok(()) if successful, Err if clipboard access fails.
pub fn write_to_clipboard(text: &str) -> Result<(), Box<dyn std::error::Error>> {
    let clipboard = Clipboard::new()?;

    // Store to CLIPBOARD selection (Ctrl+C/V)
    clipboard
        .store(
            clipboard.setter.atoms.clipboard,
            clipboard.setter.atoms.utf8_string,
            text.as_bytes(),
        )
        .map_err(|e| format!("Failed to write to CLIPBOARD: {:?}", e))?;

    // Also store to PRIMARY selection (middle-click paste)
    clipboard
        .store(
            clipboard.setter.atoms.primary,
            clipboard.setter.atoms.utf8_string,
            text.as_bytes(),
        )
        .map_err(|e| format!("Failed to write to PRIMARY: {:?}", e))?;

    Ok(())
}

/// Read content from X11 system clipboard
///
/// Attempts to read from CLIPBOARD selection first, falls back to PRIMARY.
/// Returns the clipboard text content if available.
pub fn get_clipboard_content() -> Option<String> {
    let clipboard = Clipboard::new().ok()?;
    let timeout = Duration::from_secs(3);

    // Try CLIPBOARD first (Ctrl+C/V)
    if let Ok(data) = clipboard.load(
        clipboard.getter.atoms.clipboard,
        clipboard.getter.atoms.utf8_string,
        clipboard.getter.atoms.property,
        timeout,
    ) {
        return String::from_utf8(data).ok();
    }

    // Fall back to PRIMARY (middle-click)
    if let Ok(data) = clipboard.load(
        clipboard.getter.atoms.primary,
        clipboard.getter.atoms.utf8_string,
        clipboard.getter.atoms.property,
        timeout,
    ) {
        return String::from_utf8(data).ok();
    }

    None
}
