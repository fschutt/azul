//! Wayland clipboard integration.
//!
//! Native `wl_data_device` / `zwp_primary_selection_v1` first — those work on
//! a pure Wayland session with no XWayland at all. When the compositor has
//! announced no selection, the XWayland path is used as a fallback, and it is
//! routed through the X11 backend's clipboard worker (`x11/clipboard.rs`)
//! rather than a second `x11_clipboard::Clipboard` of this module's own: one
//! selection owner per process, and — the point — no blocking X round trip on
//! the UI thread.
//!
//! `sync_clipboard` is called from `wayland/mod.rs` after user callbacks
//! to commit pending clipboard changes to the system clipboard.

use azul_layout::managers::clipboard::ClipboardManager;

use super::super::super::common::debug_server::LogCategory;
use crate::{log_debug, log_error, log_info, log_trace, log_warn};

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

    // XWayland fallback, through the X11 backend's clipboard WORKER rather
    // than a second `x11_clipboard::Clipboard` of our own. Same mechanism,
    // minus the four synchronous X round trips this used to spend on the UI
    // thread — see `x11/clipboard.rs::write_to_clipboard`.
    super::super::x11::clipboard::write_to_clipboard(text)
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

    // XWayland fallback, through the X11 backend's clipboard WORKER. This was
    // the LAST blocking clipboard call on the Wayland UI thread: a three-second
    // `Clipboard::load` right here, reached by every Ctrl+V on a session whose
    // compositor had not announced a selection. The X11 module does the read on
    // its worker and gives up after `PASTE_UI_DEADLINE`.
    super::super::x11::clipboard::get_clipboard_content().ok_or(ClipboardError::ReadFailed)
}

/// Why a clipboard operation could not be completed.
///
/// `InitFailed` and `EncodingError` went away with the inline
/// `x11_clipboard::Clipboard`: there is no connection to fail to open here any
/// more, and the X11 module decodes the bytes. What is left is what this
/// module can still decide.
#[derive(Debug)]
pub(crate) enum ClipboardError {
    /// Neither the native selection nor the XWayland fallback took the text.
    WriteFailed,
    /// Nothing answered — no compositor selection and no XWayland owner.
    ReadFailed,
}
// --- Native primary selection (middle-click paste) ---

/// Text we currently offer on the native Wayland PRIMARY selection.
/// `Some` = we own it: `events::primary_selection_source_send` serves the
/// pasting client from here, and `primary_selection_source_cancelled` clears
/// it when another client takes over.
static NATIVE_PRIMARY: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// The text served to pasting clients while we own the primary selection.
pub(super) fn native_primary_text() -> Option<String> {
    NATIVE_PRIMARY.lock().ok().and_then(|g| g.clone())
}

/// Primary-selection ownership lost.
pub(super) fn clear_native_primary() {
    if let Ok(mut g) = NATIVE_PRIMARY.lock() {
        *g = None;
    }
}

/// Claim the primary selection for `text` — the Wayland half of the X11
/// select-to-copy idiom (`x11/clipboard.rs::write_to_primary`).
///
/// Selecting text claims PRIMARY without touching CLIPBOARD: an explicit copy
/// is what owns CLIPBOARD, and clobbering it on every selection would destroy
/// whatever the user copied.
pub(crate) fn write_to_primary(text: &str) -> Result<(), ClipboardError> {
    if let Ok(mut g) = NATIVE_PRIMARY.lock() {
        *g = Some(text.to_owned());
    }
    if with_wayland_window(|w| w.wayland_set_primary_selection()) == Some(true) {
        return Ok(());
    }
    // We did NOT take the selection, so stop answering as if we had. Ownership
    // is only tracked for a selection we hold: `primary_selection_source_cancelled`
    // is what clears this cell when another client takes over, and it can only
    // arrive for a source we created. Leaving the text parked here would make
    // every later middle click paste OUR last selection, for the rest of the
    // session, no matter what the user selected somewhere else.
    clear_native_primary();

    // No compositor support (GNOME shipped zwp_primary_selection_v1 only in
    // 42): try XWayland, which shares the X PRIMARY selection with the rest of
    // the session and does track ownership. Queued to the X11 worker, so this
    // stays off the UI thread.
    super::super::x11::clipboard::write_to_primary(text)
        .map_err(|_| ClipboardError::WriteFailed)
}

/// Read the primary selection — the middle-click paste source.
///
/// Answers locally when we own it: a `receive()` on our OWN offer would
/// deadlock the single-threaded event loop, because the `send` event that
/// serves it cannot dispatch while we block on the pipe.
pub(crate) fn get_primary_content() -> Option<String> {
    if let Some(text) = native_primary_text() {
        return Some(text);
    }
    if let Some(text) = with_wayland_window(|w| w.read_wayland_primary_selection()).flatten() {
        return Some(text);
    }
    // XWayland fallback, same as the CLIPBOARD path and for the same reason:
    // a compositor without zwp_primary_selection_v1 still has an X PRIMARY
    // selection if XWayland is running.
    super::super::x11::clipboard::get_primary_content()
}

#[cfg(test)]
mod tests {
    /// No BLOCKING clipboard call may survive on the Wayland UI path.
    ///
    /// Both halves of this module used to make one: `Clipboard::store` (four
    /// synchronous X round trips) on every copy and `Clipboard::load` with a
    /// three-second deadline on every paste that the native path did not
    /// answer. Both now go through the X11 backend's worker, which the UI
    /// thread waits on for `PASTE_UI_DEADLINE` at most.
    ///
    /// NEGATIVE CONTROL: restore either inline `x11_clipboard` call.
    #[test]
    fn nothing_here_talks_to_x11_synchronously() {
        let source = include_str!("clipboard.rs");
        // Comments discuss what was removed, by name. Scan the CODE.
        let body: String = source
            .split_once("mod tests {")
            .map_or(source, |(before, _)| before)
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        for blocking in ["x11_clipboard::Clipboard", ".load(", ".store("] {
            assert!(
                !body.contains(blocking),
                "`{blocking}` is back on the Wayland UI thread — route it through \
                 x11::clipboard's worker instead"
            );
        }
        for fallback in [
            "x11::clipboard::get_clipboard_content",
            "x11::clipboard::write_to_clipboard",
            "x11::clipboard::get_primary_content",
            "x11::clipboard::write_to_primary",
        ] {
            assert!(
                body.contains(fallback),
                "the XWayland fallback `{fallback}` must still exist, just off \
                 the UI thread"
            );
        }
    }
}
