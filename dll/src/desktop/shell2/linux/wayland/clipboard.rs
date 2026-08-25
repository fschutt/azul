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

use rich_clipboard::{ClipboardPayload, Flavor, Platform};

use super::super::super::common::debug_server::LogCategory;
use crate::{log_debug, log_warn};

/// Read content from Wayland system clipboard
///
/// Returns the clipboard text content if available.
pub fn get_clipboard_content() -> Option<String> {
    read_from_clipboard().ok()
}

// --- Native wl_data_device clipboard (MWA-B3) ---

/// What we currently offer on the native Wayland selection. `Some` = we own
/// the selection: `events::data_source_send` serves the pasting client the
/// representation it asked for, and `events::data_source_cancelled` clears it
/// when another client takes the selection over.
///
/// A whole payload rather than one `String`, because Wayland is the only Linux
/// transport here that can publish a real fan-out: `wl_data_source.offer` is
/// called once per mime type and `send` names the one the peer picked. So a
/// copy of styled text offers `text/rtf`, `text/html` *and* `text/plain` at
/// once, and the peer chooses — which is exactly what makes a paste into
/// LibreOffice keep its styling. (The X11 fallback below cannot do this: its
/// selection owner serves one target. See `x11/clipboard.rs`.)
static NATIVE_COPY: std::sync::Mutex<Option<ClipboardPayload>> = std::sync::Mutex::new(None);

/// The bytes to serve for one requested mime type, while we own the selection.
///
/// Matched by resolved [`Flavor`], not by string equality: a peer that asks
/// for `UTF8_STRING` or `text/plain` must be served the payload's
/// `text/plain;charset=utf-8` bytes — they are one flavor under three
/// spellings, and a strict match would answer an empty pipe.
pub(super) fn native_copy_bytes(mime: &str) -> Option<Vec<u8>> {
    let guard = NATIVE_COPY.lock().ok()?;
    let payload = guard.as_ref()?;
    let want = Flavor::from_mime(mime);
    payload
        .items()
        .iter()
        .find(|i| Flavor::from_mime(&i.native) == want)
        .map(|i| i.bytes.clone())
}

/// Every mime type to advertise for the selection we are about to take.
pub(super) fn native_copy_mimes() -> Vec<String> {
    let Ok(guard) = NATIVE_COPY.lock() else {
        return Vec::new();
    };
    let Some(payload) = guard.as_ref() else {
        return Vec::new();
    };
    let mut mimes: Vec<String> = payload.items().iter().map(|i| i.native.clone()).collect();
    // The pre-MIME spellings every older toolkit and terminal still asks for.
    // Advertised only alongside real plain text, and served through the
    // flavor match in `native_copy_bytes`.
    if payload
        .items()
        .iter()
        .any(|i| Flavor::from_mime(&i.native) == Flavor::PlainText)
    {
        for legacy in ["UTF8_STRING", "text/plain"] {
            if !mimes.iter().any(|m| m == legacy) {
                mimes.push(legacy.to_owned());
            }
        }
    }
    mimes
}

/// The text served to pasting clients while we own the selection.
///
/// The plain-text reading of [`NATIVE_COPY`], for the callers that only ever
/// wanted a string.
pub(super) fn native_copy_text() -> Option<String> {
    let guard = NATIVE_COPY.lock().ok()?;
    let payload = guard.as_ref()?;
    rich_clipboard::decode_payload(payload)
        .ok()?
        .plain_text()
        .map(str::to_owned)
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
    let payload = rich_clipboard::encode(
        &rich_clipboard::RichItem::Text(text.to_owned()),
        Platform::Unix,
    )
    .map_err(|_| ClipboardError::WriteFailed)?;
    write_payload(&payload).map_err(|_| ClipboardError::WriteFailed)
}

/// Publish every flavor of a payload to the Wayland selection.
///
/// The native path takes them all; the XWayland fallback can only carry plain
/// text (see [`x11::clipboard`](super::super::x11::clipboard)), so a fall back
/// is also a loss of fidelity — logged, because a copy that silently drops its
/// styling is the kind of thing that gets reported as "paste is broken".
pub(crate) fn write_payload(payload: &ClipboardPayload) -> Result<(), ClipboardError> {
    // MWA-B3: native wl_data_device first — works on pure Wayland sessions
    // (no XWayland). Park the payload, then take the seat selection; pasting
    // clients pull the representation they want through data_source_send.
    if let Ok(mut g) = NATIVE_COPY.lock() {
        *g = Some(payload.clone());
    }
    if with_wayland_window(|w| w.wayland_set_selection()) == Some(true) {
        log_debug!(
            LogCategory::Resources,
            "[Wayland Clipboard] native wl_data_source selection taken, offering {} flavor(s)",
            payload.len()
        );
        return Ok(());
    }
    clear_native_copy();

    // XWayland fallback, through the X11 backend's clipboard WORKER rather
    // than a second `x11_clipboard::Clipboard` of our own. Same mechanism,
    // minus the four synchronous X round trips this used to spend on the UI
    // thread — see `x11/clipboard.rs::write_to_clipboard`.
    let text = rich_clipboard::decode_payload(payload)
        .ok()
        .and_then(|item| item.plain_text().map(str::to_owned))
        .ok_or(ClipboardError::WriteFailed)?;
    if payload.len() > 1 {
        log_warn!(
            LogCategory::Resources,
            "[Wayland Clipboard] no compositor selection — falling back to XWayland, which \
             carries plain text only. {} of {} flavor(s) will not be published.",
            payload.len() - 1,
            payload.len()
        );
    }
    super::super::x11::clipboard::write_to_clipboard(&text)
        .map_err(|_| ClipboardError::WriteFailed)
}

/// Read every flavor the Wayland selection offers.
///
/// Answers from our own payload when we own the selection: a `receive()` on
/// our OWN offer would deadlock the single-threaded event loop, because the
/// `send` event that serves it cannot dispatch while we block on the pipe.
pub(crate) fn read_payload() -> Option<ClipboardPayload> {
    if let Ok(guard) = NATIVE_COPY.lock() {
        if let Some(payload) = guard.as_ref() {
            return Some(payload.clone());
        }
    }
    if let Some(Some(payload)) = with_wayland_window(|w| w.read_wayland_selection_payload()) {
        return Some(payload);
    }
    // XWayland fallback: single-flavor, on the X11 worker.
    super::super::x11::clipboard::read_payload()
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
