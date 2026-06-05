//! Wayland clipboard integration
//!
//! Currently relies on the `x11-clipboard` crate, which requires an X11
//! connection (XWayland). On pure Wayland sessions without XWayland,
//! `Clipboard::new()` will fail and clipboard operations will be unavailable.
//!
//! `sync_clipboard` is called from `wayland/mod.rs` after user callbacks
//! to commit pending clipboard changes to the system clipboard.

use std::time::Duration;

use azul_layout::managers::clipboard::ClipboardManager;

/// Timeout for clipboard read operations.
const CLIPBOARD_READ_TIMEOUT: Duration = Duration::from_secs(3);
use x11_clipboard::Clipboard;

use super::super::super::common::debug_server::LogCategory;
use crate::{log_debug, log_error, log_info, log_trace, log_warn};

/// Process-wide persistent clipboard owner — same rationale as the X11 backend
/// (`x11/clipboard.rs`): `x11_clipboard::Clipboard` spawns a thread that OWNS
/// the selection, so the copied content only survives while that `Clipboard`
/// stays alive. Creating + dropping one per copy (the previous behaviour here)
/// killed the owner thread and lost the selection immediately — Ctrl+C appeared
/// to do nothing and Ctrl+V pasted stale content. Keep ONE alive for the
/// process. NOTE: this is still the XWayland fallback; native `wl_data_device`
/// (for pure-Wayland sessions) is task #7 and not yet implemented.
fn clipboard() -> Option<std::sync::MutexGuard<'static, Option<Clipboard>>> {
    static CLIPBOARD: std::sync::OnceLock<std::sync::Mutex<Option<Clipboard>>> =
        std::sync::OnceLock::new();
    let m = CLIPBOARD.get_or_init(|| std::sync::Mutex::new(Clipboard::new().ok()));
    m.lock().ok()
}

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
            log_error!(
                LogCategory::Resources,
                "[Wayland Clipboard] Failed to write: {:?}",
                e
            );
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
pub(crate) fn write_to_clipboard(text: &str) -> Result<(), ClipboardError> {
    let guard = clipboard().ok_or(ClipboardError::InitFailed)?;
    let clipboard = guard.as_ref().ok_or(ClipboardError::InitFailed)?;

    clipboard
        .store(
            clipboard.setter.atoms.clipboard,
            clipboard.setter.atoms.utf8_string,
            text.as_bytes(),
        )
        .map_err(|_| ClipboardError::WriteFailed)
}

/// Read string from Wayland clipboard
fn read_from_clipboard() -> Result<String, ClipboardError> {
    let guard = clipboard().ok_or(ClipboardError::InitFailed)?;
    let clipboard = guard.as_ref().ok_or(ClipboardError::InitFailed)?;

    let data = clipboard
        .load(
            clipboard.getter.atoms.clipboard,
            clipboard.getter.atoms.utf8_string,
            clipboard.getter.atoms.property,
            CLIPBOARD_READ_TIMEOUT,
        )
        .map_err(|_| ClipboardError::ReadFailed)?;

    String::from_utf8(data).map_err(|_| ClipboardError::EncodingError)
}

#[derive(Debug)]
pub(crate) enum ClipboardError {
    InitFailed,
    WriteFailed,
    ReadFailed,
    EncodingError,
}
