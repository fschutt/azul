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
//!
//! Every window here is its OWN `wl_display` connection (`WaylandWindow::new`
//! calls `wl_display_connect` once per window), so the seat, the data device
//! and the input serial a `set_selection` needs are all per-window. A
//! clipboard request therefore has to name the window it came from:
//! [`route_selection`] is what turns "who is asking" into "whose seat", and
//! nothing in this module may reach for whichever window the registry handed
//! back first.

use rich_clipboard::{ClipboardItem, ClipboardPayload, Flavor, Platform};

use super::super::super::common::debug_server::LogCategory;
use crate::{
    desktop::shell2::linux::registry::LinuxWindowId, log_debug, log_info, log_warn, plog_info,
};

/// Read content from Wayland system clipboard
///
/// Returns the clipboard text content if available. `asking` names the window
/// whose callback wants it — see [`route_selection`].
pub fn get_clipboard_content(asking: Option<LinuxWindowId>) -> Option<String> {
    read_from_clipboard(asking).ok()
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

/// Do we still hold a payload for a selection we took?
pub(super) fn owns_native_copy() -> bool {
    NATIVE_COPY.lock().is_ok_and(|g| g.is_some())
}

/// Ownership lost (source cancelled) — stop serving / short-circuiting reads.
pub(super) fn clear_native_copy() {
    if let Ok(mut g) = NATIVE_COPY.lock() {
        *g = None;
    }
}

// --- Which window's seat a clipboard request belongs to ---

/// One live Wayland window, as the clipboard router sees it.
///
/// Everything a `set_selection` depends on is per-window, because each window
/// is its own `wl_display` connection: its own `wl_seat`, its own
/// `wl_data_device`, its own `last_input_serial`. A serial from window A is
/// not a serial window B may present, and a compositor that validates it
/// (KWin, Mutter) drops the request without a word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SeatWindow {
    /// The registry key — the `wl_surface` pointer, the same number
    /// `PlatformWindow::registry_window_id` derives.
    pub id: LinuxWindowId,
    /// Does this window hold its seat's keyboard focus?
    pub focused: bool,
    /// The most recent pointer-button / key-press serial this window saw.
    pub last_input_serial: u32,
}

/// Pick the window whose seat a clipboard request belongs to.
///
/// THE law of this module: a copy or a paste belongs to the window that asked
/// for it, never to whichever window the registry enumerated first. The
/// registry is a `HashMap`, so "first" is not even stable between runs — with
/// two windows open, a copy in window B went out on window A's data device
/// with window A's serial, and a compositor that checks the serial ignored it.
///
/// The order of preference:
///
/// 1. the window that asked, when the caller knows it
///    (`PlatformWindow::registry_window_id` at the call site);
/// 2. otherwise the window holding keyboard focus — one seat has one focus,
///    and that window's serial is the live one;
/// 3. otherwise the window with the highest input serial, i.e. the one the
///    user touched last. Ties break on the lowest id, so the answer is
///    deterministic rather than hash-ordered.
pub(super) fn route_selection(
    asking: Option<LinuxWindowId>,
    windows: &[SeatWindow],
) -> Option<LinuxWindowId> {
    if let Some(id) = asking {
        if windows.iter().any(|w| w.id == id) {
            return Some(id);
        }
    }
    // Newest input first; the lowest id breaks a dead heat, so the answer is
    // deterministic rather than whatever the registry's `HashMap` yielded.
    windows
        .iter()
        .filter(|w| w.focused)
        .max_by_key(|w| (w.last_input_serial, std::cmp::Reverse(w.id)))
        .or_else(|| {
            windows
                .iter()
                .max_by_key(|w| (w.last_input_serial, std::cmp::Reverse(w.id)))
        })
        .map(|w| w.id)
}

/// Every live Wayland window, as [`route_selection`] needs to see it.
///
/// `skip` drops one id from the walk — the window that is mid-teardown in
/// [`hand_off_selection`], which must never be offered the selection back.
fn seat_windows(skip: Option<LinuxWindowId>) -> Vec<SeatWindow> {
    use crate::desktop::shell2::linux::{registry, LinuxWindow};

    let mut candidates: Vec<SeatWindow> = Vec::new();
    for id in registry::get_all_window_ids() {
        if Some(id) == skip {
            continue;
        }
        let Some(ptr) = (unsafe { registry::get_window(id) }) else {
            continue;
        };
        if let LinuxWindow::Wayland(w) = unsafe { &*ptr } {
            candidates.push(SeatWindow {
                id,
                focused: w.common.current_window_state().window_focused,
                last_input_serial: w.last_input_serial,
            });
        }
    }
    candidates
}

/// Run `f` against the live `WaylandWindow` that owns this request.
///
/// The clipboard entry points are free functions called from the shared event
/// pipeline on the main thread, so the raw registry pointer is valid for the
/// duration of the call. WHICH pointer is [`route_selection`]'s decision.
fn with_wayland_window<R>(
    asking: Option<LinuxWindowId>,
    f: impl FnOnce(&mut super::WaylandWindow) -> R,
) -> Option<R> {
    use crate::desktop::shell2::linux::{registry, LinuxWindow};

    let candidates = seat_windows(None);
    let chosen = route_selection(asking, &candidates)?;
    let ptr = unsafe { registry::get_window(chosen) }?;
    match unsafe { &mut *ptr } {
        LinuxWindow::Wayland(w) => Some(f(w)),
        LinuxWindow::X11(_) => None,
    }
}

/// Write string to Wayland clipboard
pub(crate) fn write_to_clipboard(
    asking: Option<LinuxWindowId>,
    text: &str,
) -> Result<(), ClipboardError> {
    let payload = rich_clipboard::encode(
        &rich_clipboard::RichItem::Text(text.to_owned()),
        Platform::Unix,
    )
    .map_err(|_| ClipboardError::WriteFailed)?;
    write_payload(asking, &payload).map_err(|_| ClipboardError::WriteFailed)
}

/// Publish every flavor of a payload to the Wayland selection.
///
/// The native path takes them all; the XWayland fallback can only carry plain
/// text (see [`x11::clipboard`](super::super::x11::clipboard)), so a fall back
/// is also a loss of fidelity — logged, because a copy that silently drops its
/// styling is the kind of thing that gets reported as "paste is broken".
pub(crate) fn write_payload(
    asking: Option<LinuxWindowId>,
    payload: &ClipboardPayload,
) -> Result<(), ClipboardError> {
    // MWA-B3: native wl_data_device first — works on pure Wayland sessions
    // (no XWayland). Park the payload, then take the seat selection ON THE
    // ASKING WINDOW's data device, with that window's serial; pasting clients
    // pull the representation they want through data_source_send.
    if let Ok(mut g) = NATIVE_COPY.lock() {
        *g = Some(payload.clone());
    }
    if with_wayland_window(asking, |w| w.wayland_set_selection()) == Some(true) {
        log_debug!(
            LogCategory::Resources,
            "[Wayland Clipboard] native wl_data_source selection taken, offering {} flavor(s)",
            payload.len()
        );
        return Ok(());
    }
    clear_native_copy();
    xwayland_write_fallback(payload)
}

/// XWayland fallback, through the X11 backend's clipboard WORKER rather than a
/// second `x11_clipboard::Clipboard` of our own. Same mechanism, minus the four
/// synchronous X round trips this used to spend on the UI thread — see
/// `x11/clipboard.rs::write_to_clipboard`.
fn xwayland_write_fallback(payload: &ClipboardPayload) -> Result<(), ClipboardError> {
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
    super::super::x11::clipboard::write_to_clipboard(&text).map_err(|_| ClipboardError::WriteFailed)
}

/// Read every flavor the Wayland selection offers.
///
/// Answers from our own payload when we own the selection: a `receive()` on
/// our OWN offer would deadlock the single-threaded event loop, because the
/// `send` event that serves it cannot dispatch while we block on the pipe.
pub(crate) fn read_payload(asking: Option<LinuxWindowId>) -> Option<ClipboardPayload> {
    if let Ok(guard) = NATIVE_COPY.lock() {
        if let Some(payload) = guard.as_ref() {
            return Some(payload.clone());
        }
    }
    if let Some(Some(payload)) = with_wayland_window(asking, |w| w.read_wayland_selection_payload())
    {
        return Some(payload);
    }
    // XWayland fallback: single-flavor, on the X11 worker.
    super::super::x11::clipboard::read_payload()
}

/// The same as [`write_payload`], on a window the caller already holds.
///
/// `with_wayland_window` reaches the window through the registry and makes a
/// `&mut` out of a raw pointer. Every one of these calls happens inside a
/// `&mut WaylandWindow` method - `poll_event` dispatching a Ctrl+C - so when
/// the routed window IS the caller, which is always the case in a
/// single-window app, that is two live `&mut` to one object. A window that
/// has itself in hand does not need the registry at all.
pub(crate) fn write_payload_on(
    window: &mut super::WaylandWindow,
    payload: &ClipboardPayload,
) -> Result<(), ClipboardError> {
    if let Ok(mut g) = NATIVE_COPY.lock() {
        *g = Some(payload.clone());
    }
    if window.wayland_set_selection() {
        return Ok(());
    }
    xwayland_write_fallback(payload)
}

/// The same as [`read_payload`], on a window the caller already holds. See
/// [`write_payload_on`] for why that matters.
pub(crate) fn read_payload_on(window: &mut super::WaylandWindow) -> Option<ClipboardPayload> {
    if let Ok(guard) = NATIVE_COPY.lock() {
        if let Some(payload) = guard.as_ref() {
            return Some(payload.clone());
        }
    }
    if let Some(payload) = window.read_wayland_selection_payload() {
        return Some(payload);
    }
    super::super::x11::clipboard::read_payload()
}

/// Read string from Wayland clipboard
fn read_from_clipboard(asking: Option<LinuxWindowId>) -> Result<String, ClipboardError> {
    // MWA-B3: if we own the selection, answer locally (a receive() on our
    // own offer would deadlock the single-threaded event loop: the send
    // event that serves it can't dispatch while we block on the pipe).
    if let Some(text) = native_copy_text() {
        return Ok(text);
    }
    // Native path: another client's offer, received through a pipe.
    if let Some(Some(text)) = with_wayland_window(asking, |w| w.read_wayland_selection()) {
        return Ok(text);
    }

    // XWayland fallback, through the X11 backend's clipboard WORKER. This was
    // the LAST blocking clipboard call on the Wayland UI thread: a three-second
    // `Clipboard::load` right here, reached by every Ctrl+V on a session whose
    // compositor had not announced a selection. The X11 module does the read on
    // its worker and gives up after `PASTE_UI_DEADLINE`.
    super::super::x11::clipboard::get_clipboard_content().ok_or(ClipboardError::ReadFailed)
}

// --- One offer, one transfer ---

/// One pipe transfer from a data offer, as a seam.
///
/// The paste policy below is the part worth pinning in a test, and it has to
/// be testable without a compositor — so the only thing that touches
/// libwayland is this one method, implemented for real by [`OfferPipe`] and by
/// a counting stub in the tests.
pub(super) trait OfferTransport {
    /// Ask the source for `mime` and return what came down the pipe. Empty on
    /// any failure: a source is under no obligation to answer.
    fn receive(&mut self, mime: &str) -> Vec<u8>;
}

/// The real transport: `wl_data_offer.receive` into a pipe, drained on the
/// Wayland transfer worker with a UI-thread deadline
/// (`events::receive_offer_bytes`).
pub(super) struct OfferPipe<'a> {
    pub window: &'a super::WaylandWindow,
    pub offer: *mut super::defines::wl_data_offer,
}

impl OfferTransport for OfferPipe<'_> {
    fn receive(&mut self, mime: &str) -> Vec<u8> {
        unsafe { super::events::receive_offer_bytes(self.window, self.offer, mime) }
    }
}

/// The ONE mime type to ask an offer for, chosen from what it advertised.
///
/// `Flavor::read_rank` is the ranking the decoder itself uses — richest first,
/// because plain text is derivable from rich text and never the reverse — so
/// the flavor picked here is the flavor `decode_payload` would have kept
/// anyway. Ties keep the source's own ordering (`min_by_key` returns the first
/// minimum), which is what makes an `UTF8_STRING` beat a later `text/plain`.
///
/// Flavors this build has no codec for (`Flavor::Other`) and pure metadata
/// (`Preferred DropEffect`, `public.url-name`) are not worth a pipe.
pub(super) fn best_offered_mime(offered: &[String]) -> Option<&str> {
    offered
        .iter()
        .filter(|m| {
            let flavor = Flavor::from_mime(m.as_str());
            flavor.is_content() && !matches!(flavor, Flavor::Other(_))
        })
        .min_by_key(|m| Flavor::from_mime(m.as_str()).read_rank())
        .map(String::as_str)
}

/// Read a clipboard offer with EXACTLY ONE transfer.
///
/// A `wl_data_offer.receive` is a pipe fed by a foreign process, and the UI
/// thread waits `events::PASTE_UI_DEADLINE` for it. This used to run once per
/// advertised flavor there was a codec for, so a paste from a source offering
/// RTF, HTML, plain text and a PNG cost four deadlines back to back — over a
/// second of frozen event loop, for a payload of which `decode_payload` then
/// kept exactly one flavor. Rank first, transfer once: the UI thread spends
/// one deadline, the same budget `x11/clipboard.rs` already holds itself to.
///
/// The structure this fits into: the libwayland half (allocate the pipe,
/// marshal `receive`, flush) must stay on the loop thread, because a proxy
/// call from another thread would race the single-threaded dispatch; only the
/// DRAIN is handed to the long-lived `azul-wayland-transfer` worker, and the
/// loop waits on it with `recv_timeout(PASTE_UI_DEADLINE)`. One wait, bounded,
/// per paste — which is the most a synchronous `SystemChange::PasteFromClipboard`
/// can give. (Making a paste fully non-blocking means making it resumable
/// across pump iterations, i.e. a request in the `azul_layout::request` queue,
/// which is a change to the cross-platform clipboard seam and not to this
/// backend.)
///
/// `None` when nothing worth asking for was advertised, or the source answered
/// with nothing; the caller's unconditional plain-text read is the fallback,
/// and it is the only path on which a second transfer can happen.
pub(super) fn read_offer_payload(
    offered: &[String],
    transport: &mut impl OfferTransport,
) -> Option<ClipboardPayload> {
    let mime = best_offered_mime(offered)?;
    let bytes = transport.receive(mime);
    if bytes.is_empty() {
        return None;
    }
    let mut payload = ClipboardPayload::new(Platform::Unix);
    payload.push(ClipboardItem::new(mime, bytes));
    Some(payload)
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

// --- Outliving the window, and outliving the process ---

/// Has this session seen a clipboard-manager protocol on the compositor?
///
/// Set from `events::registry_global_handler` when `zwlr_data_control_manager_v1`
/// or `ext_data_control_manager_v1` is advertised. The global is deliberately
/// NOT bound: data-control is the protocol a clipboard MANAGER speaks, and a
/// toolkit that bound it would be pretending to be one.
static DATA_CONTROL_SEEN: std::sync::Mutex<bool> = std::sync::Mutex::new(false);

/// Note one advertised global, for the clipboard-manager question only.
///
/// wlroots' `zwlr_data_control_manager_v1` and its upstream successor
/// `ext_data_control_manager_v1` are what a clipboard manager watches the
/// selection through. Their presence means something out there is able to keep
/// a selection alive after we are gone.
pub(super) fn note_global(interface: &str) {
    if matches!(
        interface,
        "zwlr_data_control_manager_v1" | "ext_data_control_manager_v1"
    ) {
        if let Ok(mut g) = DATA_CONTROL_SEEN.lock() {
            *g = true;
        }
    }
}

/// Hand the selection on when the window that owns it goes away.
///
/// A Wayland selection lives exactly as long as the client that owns the
/// `wl_data_source`, and every window here is its own client connection — so
/// closing the window a user copied from destroyed the app's own clipboard
/// while the app was still running. The payload is process-global
/// ([`NATIVE_COPY`]), so a surviving window can simply take the selection
/// again.
///
/// ACROSS the process there is no such handoff to make. X11 has one — ICCCM
/// `CLIPBOARD_MANAGER` / `SAVE_TARGETS` — and Wayland deliberately has none:
/// persistence is a clipboard manager's job (Klipper, cliphist,
/// wl-clip-persist, GNOME's own), and a manager does it by watching the
/// selection through data-control. A toolkit that implemented that would be a
/// second manager fighting the first. So the correct behaviour is exactly
/// this: keep the selection alive as long as the process has a window to hold
/// it, and, when the process itself goes, say whether anything out there can
/// carry it.
pub(super) fn hand_off_selection(closing: LinuxWindowId) {
    use crate::desktop::shell2::linux::{registry, LinuxWindow};

    if !owns_native_copy() {
        return;
    }
    let candidates = seat_windows(Some(closing));
    let Some(heir) = route_selection(None, &candidates) else {
        // Last window out. Nothing in this process can hold the selection any
        // more; whether it survives is now entirely a clipboard manager's call.
        if DATA_CONTROL_SEEN.lock().is_ok_and(|g| *g) {
            log_debug!(
                LogCategory::Resources,
                "[Wayland Clipboard] last window closed while owning the selection — a \
                 data-control clipboard manager is present and can keep it"
            );
        } else {
            log_info!(
                LogCategory::Resources,
                "[Wayland Clipboard] last window closed while owning the selection, and this \
                 compositor advertises no clipboard-manager protocol — the selection dies with \
                 this process. Wayland has no SAVE_TARGETS handoff; a clipboard manager is what \
                 makes a copy outlive the app."
            );
        }
        clear_native_copy();
        return;
    };
    let took = unsafe { registry::get_window(heir) }
        .map(|ptr| match unsafe { &mut *ptr } {
            LinuxWindow::Wayland(w) => w.wayland_set_selection(),
            LinuxWindow::X11(_) => false,
        })
        .unwrap_or(false);
    plog_info!(
        "[wl-clipboard] selection handed from closing window {} to window {}: {}",
        closing,
        heir,
        took
    );
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
///
/// Takes the window directly. The selection belongs to the seat of the window
/// whose pointer ended the gesture, and that window is already on the stack at
/// the only call site — going back through the registry for it would pick the
/// wrong window AND take a second `&mut` to one that is already borrowed.
pub(super) fn write_to_primary(
    window: &mut super::WaylandWindow,
    text: &str,
) -> Result<(), ClipboardError> {
    if let Ok(mut g) = NATIVE_PRIMARY.lock() {
        *g = Some(text.to_owned());
    }
    if window.wayland_set_primary_selection() {
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
    super::super::x11::clipboard::write_to_primary(text).map_err(|_| ClipboardError::WriteFailed)
}

/// Read the primary selection — the middle-click paste source.
///
/// Answers locally when we own it: a `receive()` on our OWN offer would
/// deadlock the single-threaded event loop, because the `send` event that
/// serves it cannot dispatch while we block on the pipe.
///
/// Takes the window directly, for the reason [`write_to_primary`] does.
pub(super) fn get_primary_content(window: &mut super::WaylandWindow) -> Option<String> {
    if let Some(text) = native_primary_text() {
        return Some(text);
    }
    if let Some(text) = window.read_wayland_primary_selection() {
        return Some(text);
    }
    // XWayland fallback, same as the CLIPBOARD path and for the same reason:
    // a compositor without zwp_primary_selection_v1 still has an X PRIMARY
    // selection if XWayland is running.
    super::super::x11::clipboard::get_primary_content()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn win(id: LinuxWindowId, focused: bool, last_input_serial: u32) -> SeatWindow {
        SeatWindow {
            id,
            focused,
            last_input_serial,
        }
    }

    /// The asking window wins outright — over the focused one, over the one
    /// listed first, over the one with the newest serial.
    #[test]
    fn the_window_that_asked_owns_the_request() {
        let a = win(10, true, 900);
        let b = win(20, false, 5);
        assert_eq!(route_selection(Some(20), &[a, b]), Some(20));
        assert_eq!(route_selection(Some(10), &[b, a]), Some(10));
        // An id that is no longer live falls through rather than answering
        // with a window that is not there.
        assert_eq!(route_selection(Some(999), &[a, b]), Some(10));
    }

    /// With no asker named, the seat's keyboard focus decides; only with
    /// nothing focused does the most recent input serial break the tie. Never
    /// enumeration order.
    #[test]
    fn an_anonymous_request_follows_focus_then_the_last_input() {
        let stale = win(10, false, 900);
        let focused = win(20, true, 5);
        assert_eq!(route_selection(None, &[stale, focused]), Some(20));
        assert_eq!(route_selection(None, &[focused, stale]), Some(20));

        // Nothing focused: the window the user touched last, which is the only
        // one holding a serial a compositor will still honour.
        let recent = win(30, false, 901);
        assert_eq!(route_selection(None, &[stale, recent]), Some(30));
        assert_eq!(route_selection(None, &[recent, stale]), Some(30));

        // Deterministic on a dead heat, rather than hash-ordered.
        assert_eq!(
            route_selection(None, &[win(30, false, 0), win(10, false, 0)]),
            Some(10)
        );
        assert_eq!(
            route_selection(None, &[win(10, false, 0), win(30, false, 0)]),
            Some(10)
        );

        assert_eq!(route_selection(None, &[]), None);
        assert_eq!(route_selection(Some(10), &[]), None);
    }

    /// A paste opens ONE pipe, for the richest flavor the offer advertised.
    ///
    /// The mime list below is what a LibreOffice / browser copy actually
    /// advertises. Before the fix this cost four `wl_data_offer.receive`
    /// transfers, each with its own 400 ms UI-thread deadline.
    #[test]
    fn one_offer_costs_one_transfer() {
        struct Counting {
            asked: Vec<String>,
        }
        impl OfferTransport for Counting {
            fn receive(&mut self, mime: &str) -> Vec<u8> {
                self.asked.push(mime.to_owned());
                b"{\\rtf1}".to_vec()
            }
        }

        let offered: Vec<String> = [
            "text/plain;charset=utf-8",
            "UTF8_STRING",
            "STRING",
            "TEXT",
            "text/html",
            "text/rtf",
            "image/png",
            "application/x-libreoffice-internal",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();

        let mut transport = Counting { asked: Vec::new() };
        let payload =
            read_offer_payload(&offered, &mut transport).expect("the source answered with bytes");

        assert_eq!(
            transport.asked,
            vec!["text/rtf".to_owned()],
            "a paste must cost ONE pipe transfer, for the richest flavor on offer"
        );
        assert_eq!(payload.len(), 1);
    }

    /// The flavor picked is the one the decoder ranks highest, and ties keep
    /// the source's own ordering.
    #[test]
    fn the_transferred_mime_is_the_richest_on_offer() {
        let pick = |mimes: &[&str]| {
            let owned: Vec<String> = mimes.iter().map(|s| (*s).to_owned()).collect();
            best_offered_mime(&owned).map(str::to_owned)
        };

        assert_eq!(pick(&["text/plain", "text/html"]), Some("text/html".into()));
        assert_eq!(pick(&["text/html", "text/rtf"]), Some("text/rtf".into()));
        assert_eq!(
            pick(&["text/plain", "text/uri-list"]),
            Some("text/uri-list".into())
        );
        // Two spellings of one flavor: the source's order decides, and it is
        // still one transfer.
        assert_eq!(
            pick(&["UTF8_STRING", "text/plain;charset=utf-8"]),
            Some("UTF8_STRING".into())
        );
        // Nothing decodable on offer is not worth a pipe at all.
        assert_eq!(pick(&["application/x-vendor-blob"]), None);
        assert_eq!(pick(&[]), None);
    }

    /// A source that never answers costs one transfer, not a retry per
    /// flavor: the caller's plain-text read is the only second chance.
    #[test]
    fn a_silent_source_still_costs_one_transfer() {
        struct Silent {
            asked: usize,
        }
        impl OfferTransport for Silent {
            fn receive(&mut self, _mime: &str) -> Vec<u8> {
                self.asked += 1;
                Vec::new()
            }
        }
        let offered: Vec<String> = ["text/rtf", "text/html", "text/plain"]
            .iter()
            .map(|s| (*s).to_owned())
            .collect();
        let mut transport = Silent { asked: 0 };
        assert!(read_offer_payload(&offered, &mut transport).is_none());
        assert_eq!(transport.asked, 1);
    }

    /// Only the two clipboard-manager protocols count as "something can keep
    /// this selection alive".
    #[test]
    fn only_data_control_counts_as_a_clipboard_manager() {
        note_global("wl_data_device_manager");
        note_global("zwp_primary_selection_device_manager_v1");
        assert!(
            !DATA_CONTROL_SEEN.lock().is_ok_and(|g| *g),
            "an ordinary selection global is not a clipboard manager"
        );
        note_global("zwlr_data_control_manager_v1");
        assert!(DATA_CONTROL_SEEN.lock().is_ok_and(|g| *g));
    }

    /// One source file with its comments and its test module stripped, so a
    /// scan reads the CODE rather than the prose about it.
    fn code_of(source: &str) -> String {
        source
            .split_once("mod tests {")
            .map_or(source, |(before, _)| before)
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// This module's own code.
    fn clipboard_code() -> String {
        code_of(include_str!("clipboard.rs"))
    }

    /// `WaylandWindow`'s code — the paste transfer loop lives there, the law
    /// about it lives here.
    fn wayland_window_code() -> String {
        code_of(include_str!("mod.rs"))
    }

    /// A clipboard request belongs to the window that asked for it.
    ///
    /// Every Wayland window here is its OWN `wl_display` connection
    /// (`WaylandWindow::new` calls `wl_display_connect` once per window), so
    /// the seat, the `wl_data_device` and the `last_input_serial` that
    /// `wl_data_device.set_selection` requires are all per-window. A copy made
    /// in window B has to go out on B's data device with B's serial; sending
    /// A's makes a compositor that validates the serial (KWin, Mutter) drop
    /// the request without a word.
    ///
    /// RED TODAY: `with_wayland_window` takes no window at all. It walks
    /// `registry::get_all_window_ids()` — a `HashMap`, so not even in a stable
    /// order — and returns on the FIRST Wayland window it finds.
    #[test]
    fn a_clipboard_request_is_routed_to_the_window_that_asked() {
        let code = clipboard_code();
        assert!(
            code.contains("route_selection("),
            "the clipboard has no window router: a copy or a paste still goes out on whichever \
             window the registry happened to enumerate first"
        );
        assert!(
            !code.contains("return Some(f(w));"),
            "a window is still being returned from INSIDE the registry walk — that IS the \
             first-window-wins bug"
        );
        assert!(
            code.contains("asking: Option<LinuxWindowId>"),
            "the clipboard entry points must be able to name the window that asked"
        );
    }

    /// A paste asks the source for exactly ONE mime type.
    ///
    /// `wl_data_offer.receive` is a pipe fed by a foreign process and the UI
    /// thread waits `events::PASTE_UI_DEADLINE` (400 ms) on each one. The
    /// offer already advertised its mime list and `Flavor::read_rank` already
    /// says which single flavor `decode_payload` would keep — so the choice
    /// costs nothing and the transfer happens once.
    ///
    /// RED TODAY: `read_wayland_selection_payload` loops `for mime in
    /// &offered` and transfers EVERY advertised flavor it has a codec for.
    /// A LibreOffice copy advertises `text/rtf`, `text/html`,
    /// `text/plain;charset=utf-8` and `image/png`: four transfers, up to
    /// 1.6 s of frozen event loop, of which the decoder then keeps one.
    #[test]
    fn a_paste_transfers_exactly_one_mime_type() {
        assert!(
            !wayland_window_code().contains("for mime in &offered"),
            "the paste path still opens one pipe per advertised mime type"
        );
        assert!(
            clipboard_code().contains("fn read_offer_payload("),
            "the one-transfer paste policy must live behind a seam that is testable without a \
             compositor"
        );
    }

    /// A selection outlives the window that took it, and the app says
    /// honestly whether it can outlive the process.
    ///
    /// A Wayland selection lives exactly as long as the client that owns the
    /// `wl_data_source`, and every window here is its own client — so closing
    /// the window a user copied from destroyed the app's own clipboard while
    /// the app was still running. That part is this toolkit's to fix. The
    /// part that is NOT is persistence past the process: Wayland has no
    /// ICCCM `SAVE_TARGETS` handoff, and a clipboard manager does the job by
    /// watching the selection through `zwlr_data_control_manager_v1` /
    /// `ext_data_control_manager_v1`. Binding those would make this toolkit a
    /// second manager; noticing them is what lets it tell the truth.
    ///
    /// RED TODAY: neither interface is named anywhere in the backend and no
    /// handoff exists.
    #[test]
    fn the_selection_outlives_the_window_and_names_its_successor() {
        let code = clipboard_code();
        assert!(
            code.contains("fn hand_off_selection("),
            "closing the window that owns the selection must hand it to a surviving window"
        );
        assert!(
            code.contains("zwlr_data_control_manager_v1")
                && code.contains("ext_data_control_manager_v1"),
            "the clipboard-manager protocols must at least be NOTICED, so the app can say \
             whether the selection is able to outlive it"
        );
        assert!(
            !code.contains("wl_registry_bind"),
            "binding data-control would make this toolkit a second clipboard manager"
        );
    }

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
        // Comments discuss what was removed, by name. Scan the CODE.
        let body = clipboard_code();

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
                "the XWayland fallback `{fallback}` must still exist, just off the UI thread"
            );
        }
    }
}
