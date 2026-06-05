//! X11 clipboard integration using x11-clipboard crate
//!
//! This module provides clipboard synchronization between azul-layout's ClipboardManager
//! and the X11 system clipboard (both PRIMARY and CLIPBOARD selections).

use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use azul_layout::managers::clipboard::ClipboardManager;
use x11_clipboard::Clipboard;

use super::super::super::common::debug_server::LogCategory;
use crate::log_error;

/// Process-wide persistent X11 clipboard owner.
///
/// CRITICAL: `x11_clipboard::Clipboard` spawns a background thread that OWNS the
/// X selection; the copied content only persists while that `Clipboard` (and its
/// thread) stays alive. The previous code created a fresh `Clipboard` per copy
/// and dropped it on return — which closed `_drop_fd`, exited the owner thread,
/// and lost the selection immediately. The symptom: Ctrl+C appeared to do
/// nothing and a following Ctrl+V pasted stale/foreign clipboard contents.
///
/// Keeping ONE instance alive for the whole process fixes that: `store()`
/// updates the content the live owner thread serves. `Clipboard` is `Send`
/// (its background thread already moves an `Arc<Context>`), so a `static Mutex`
/// is sound.
fn clipboard() -> Option<MutexGuard<'static, Option<Clipboard>>> {
    static CLIPBOARD: OnceLock<Mutex<Option<Clipboard>>> = OnceLock::new();
    let m = CLIPBOARD.get_or_init(|| Mutex::new(Clipboard::new().ok()));
    m.lock().ok()
}

/// Synchronize clipboard manager content to X11 system clipboard
///
/// This is called after user callbacks to commit clipboard changes.
/// If the clipboard manager has pending copy content, it's written to
/// both the CLIPBOARD and PRIMARY X11 selections.
pub fn sync_clipboard(clipboard_manager: &mut ClipboardManager) {
    // Check if there's pending content to copy
    if let Some(content) = clipboard_manager.get_copy_content() {
        // Write to X11 clipboard
        if let Err(e) = write_to_clipboard(&content.plain_text) {
            log_error!(LogCategory::Resources, "Failed to sync clipboard to X11: {e}");
        }
    }

    // Clear the clipboard manager after sync
    clipboard_manager.clear();
}

/// Write text to the X11 clipboard
///
/// Writes to both CLIPBOARD (Ctrl+C/V) and PRIMARY (middle-click) selections,
/// using the persistent owner so the content survives this call.
/// Returns Ok(()) if successful, Err if clipboard access fails.
pub fn write_to_clipboard(text: &str) -> Result<(), ClipboardError> {
    let guard = clipboard().ok_or(ClipboardError::InitFailed)?;
    let clipboard = guard.as_ref().ok_or(ClipboardError::InitFailed)?;

    // Store to CLIPBOARD selection (Ctrl+C/V)
    clipboard
        .store(
            clipboard.setter.atoms.clipboard,
            clipboard.setter.atoms.utf8_string,
            text.as_bytes(),
        )
        .map_err(|_| ClipboardError::WriteFailed)?;

    // Also store to PRIMARY selection (middle-click paste)
    clipboard
        .store(
            clipboard.setter.atoms.primary,
            clipboard.setter.atoms.utf8_string,
            text.as_bytes(),
        )
        .map_err(|_| ClipboardError::WriteFailed)?;

    Ok(())
}

/// Read content from X11 system clipboard
///
/// Attempts to read from CLIPBOARD selection first, falls back to PRIMARY.
/// Returns the clipboard text content if available.
///
/// Note: Each selection read uses a 3-second timeout, so this function may
/// block for up to 6 seconds in the worst case (both selections time out).
pub fn get_clipboard_content() -> Option<String> {
    let guard = clipboard()?;
    let clipboard = guard.as_ref()?;
    let timeout = Duration::from_secs(3);

    // Try CLIPBOARD first (Ctrl+C/V)
    if let Ok(data) = clipboard.load(
        clipboard.getter.atoms.clipboard,
        clipboard.getter.atoms.utf8_string,
        clipboard.getter.atoms.property,
        timeout,
    ) {
        if let Ok(s) = String::from_utf8(data) {
            if !s.is_empty() {
                return Some(s);
            }
        }
    }

    // Fall back to PRIMARY (middle-click)
    if let Ok(data) = clipboard.load(
        clipboard.getter.atoms.primary,
        clipboard.getter.atoms.utf8_string,
        clipboard.getter.atoms.property,
        timeout,
    ) {
        if let Ok(s) = String::from_utf8(data) {
            if !s.is_empty() {
                return Some(s);
            }
        }
    }

    None
}

#[derive(Debug, Copy, Clone)]
pub enum ClipboardError {
    InitFailed,
    WriteFailed,
}

impl std::fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ClipboardError::InitFailed => write!(f, "failed to initialize X11 clipboard"),
            ClipboardError::WriteFailed => write!(f, "failed to write to X11 clipboard"),
        }
    }
}
