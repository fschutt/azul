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
/// If the clipboard manager has pending copy content, it's written to
/// the Wayland clipboard.
///
/// TODO(superplan): this flush path is now redundant — the copy/cut/paste
/// shortcuts and the `SetCopyContent`/`SetCutContent` callbacks both write to
/// the OS clipboard directly through `common/event.rs`
/// (`set_system_clipboard` → `write_to_clipboard`), so no run loop calls
/// `sync_clipboard`. The macOS + Windows backends already dropped their dead
/// copies; this one (plus the `wayland/mod.rs` + `linux/mod.rs` `sync_clipboard`
/// wrappers, owned by another group) should be removed in a follow-up.
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

// --- Native wl_data_device clipboard (MWA-B3) ---

/// Text we currently offer on the native Wayland selection. `Some` = we own
/// the selection: `events::data_source_send` serves the pasting client from
/// here, and `events::data_source_cancelled` clears it when another client
/// takes the selection over.
static NATIVE_COPY: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// The text served to pasting clients while we own the selection.
pub(super) fn native_copy_text() -> Option<String> {
    NATIVE_COPY.lock().ok().and_then(|g| g.clone())
}

/// Ownership lost (source cancelled) — stop serving / short-circuiting reads.
pub(super) fn clear_native_copy() {
    if let Ok(mut g) = NATIVE_COPY.lock() {
        *g = None;
    }
}

/// Run `f` against a live `WaylandWindow` from the (main-thread) Linux window
/// registry. The clipboard entry points are free functions called from the
/// shared event pipeline on the main thread, so the raw registry pointer is
/// valid for the duration of the call.
fn with_wayland_window<R>(f: impl FnOnce(&mut super::WaylandWindow) -> R) -> Option<R> {
    for id in crate::desktop::shell2::linux::registry::get_all_window_ids() {
        let Some(ptr) = (unsafe { crate::desktop::shell2::linux::registry::get_window(id) })
        else {
            continue;
        };
        let win = unsafe { &mut *ptr };
        if let crate::desktop::shell2::linux::LinuxWindow::Wayland(w) = win {
            return Some(f(w));
        }
    }
    None
}

/// Write string to Wayland clipboard
pub(crate) fn write_to_clipboard(text: &str) -> Result<(), ClipboardError> {
    // MWA-B3: native wl_data_device first — works on pure Wayland sessions
    // (no XWayland). Park the text, then take the seat selection; pasting
    // clients pull it through data_source_send.
    if let Ok(mut g) = NATIVE_COPY.lock() {
        *g = Some(text.to_owned());
    }
    if with_wayland_window(|w| w.wayland_set_selection()) == Some(true) {
        log_debug!(
            LogCategory::Resources,
            "[Wayland Clipboard] native wl_data_source selection taken"
        );
        return Ok(());
    }
    clear_native_copy();

    // XWayland fallback (x11-clipboard) — pre-existing path.
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
    // MWA-B3: if we own the selection, answer locally (a receive() on our
    // own offer would deadlock the single-threaded event loop: the send
    // event that serves it can't dispatch while we block on the pipe).
    if let Some(text) = native_copy_text() {
        return Ok(text);
    }
    // Native path: another client's offer, received through a pipe.
    if let Some(Some(text)) = with_wayland_window(|w| w.read_wayland_selection()) {
        return Ok(text);
    }

    // XWayland fallback (x11-clipboard) — pre-existing path.
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
