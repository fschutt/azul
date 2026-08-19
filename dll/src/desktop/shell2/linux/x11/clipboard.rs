//! X11 clipboard integration using x11-clipboard crate
//!
//! This module provides clipboard synchronization between azul-layout's ClipboardManager
//! and the X11 system clipboard (both PRIMARY and CLIPBOARD selections).

use std::sync::mpsc::{self, Sender, SyncSender};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use azul_layout::managers::clipboard::ClipboardManager;
use x11_clipboard::Clipboard;

use super::super::super::common::debug_server::LogCategory;
use crate::{log_error, log_warn};

/// How long the UI thread waits for a selection read before giving up.
///
/// An X11 selection read is a round trip to ANOTHER process: when the owner is
/// gone or wedged — routine after the owning app exits — it blocks for the full
/// timeout. That used to happen ON THE UI THREAD, twice, at 3 s each, so Ctrl+V
/// could freeze the event loop for 6 s (caret blink, tweens and rendering all
/// stall). Blocking calls on the UI thread are forbidden: the read now runs on
/// a worker thread and the UI thread waits only this long — long enough for any
/// owner that is actually alive (a real transfer is a few milliseconds), short
/// enough that a dead one costs a hitch instead of a freeze.
const PASTE_UI_DEADLINE: Duration = Duration::from_millis(400);

/// Per-selection deadline INSIDE the worker thread. Bounded so the worker
/// cannot hold the clipboard mutex (shared with `write_to_clipboard`) for long.
const SELECTION_LOAD_TIMEOUT: Duration = Duration::from_millis(600);

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
/// If the clipboard manager has pending copy content, it's written to
/// both the CLIPBOARD and PRIMARY X11 selections.
///
/// TODO(superplan): this flush path is now redundant — the copy/cut/paste
/// shortcuts and the `SetCopyContent`/`SetCutContent` callbacks both write to
/// the OS clipboard directly through `common/event.rs`
/// (`set_system_clipboard` → `write_to_clipboard`), so no run loop calls
/// `sync_clipboard`. The macOS + Windows backends already dropped their dead
/// copies; this one (plus the `x11/mod.rs` + `linux/mod.rs` `sync_clipboard`
/// wrappers, owned by another group) should be removed in a follow-up.
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

/// Claim ONLY the PRIMARY selection (the X11 select/middle-click-paste idiom).
///
/// Selecting text claims PRIMARY without touching CLIPBOARD — an explicit copy
/// is what owns CLIPBOARD, and clobbering it on every selection would destroy
/// whatever the user copied.
///
/// Handed to the worker rather than performed inline: this runs on every mouse
/// release that ends a selection, and the clipboard mutex can be held by an
/// in-flight read. `Ok` therefore means "queued", not "owned".
pub fn write_to_primary(text: &str) -> Result<(), ClipboardError> {
    let sender = worker().ok_or(ClipboardError::InitFailed)?;
    sender
        .send(ClipboardJob::ClaimPrimary(text.to_string()))
        .map_err(|_| ClipboardError::WriteFailed)
}

/// Which selection a read targets.
#[derive(Debug, Copy, Clone)]
enum SelectionKind {
    /// CLIPBOARD (Ctrl+C/V), falling back to PRIMARY when it is empty.
    Clipboard,
    /// PRIMARY only (middle-click paste).
    Primary,
}

/// Work handed to the clipboard thread.
enum ClipboardJob {
    /// Read a selection and answer on `reply`.
    Load {
        kind: SelectionKind,
        reply: SyncSender<Option<String>>,
    },
    /// Take ownership of PRIMARY with this text (fire and forget).
    ClaimPrimary(String),
}

/// Handle to the (lazily spawned) clipboard worker thread.
///
/// One long-lived thread rather than one per paste: a slow owner must not be
/// able to pile up threads, and serializing the reads keeps the clipboard mutex
/// held by at most one of them.
fn worker() -> Option<MutexGuard<'static, Sender<ClipboardJob>>> {
    static WORKER: OnceLock<Mutex<Sender<ClipboardJob>>> = OnceLock::new();
    let m = WORKER.get_or_init(|| {
        let (tx, rx) = mpsc::channel::<ClipboardJob>();
        let _ = std::thread::Builder::new()
            .name("azul-x11-clipboard".to_string())
            .spawn(move || {
                while let Ok(job) = rx.recv() {
                    match job {
                        // A reply nobody is waiting for any more (the UI thread
                        // hit its deadline and dropped the receiver) is
                        // discarded, not an error.
                        ClipboardJob::Load { kind, reply } => {
                            let _ = reply.try_send(load_selection(kind));
                        }
                        ClipboardJob::ClaimPrimary(text) => claim_primary(&text),
                    }
                }
            });
        Mutex::new(tx)
    });
    m.lock().ok()
}

/// Worker-thread side of a PRIMARY claim.
fn claim_primary(text: &str) {
    let Some(guard) = clipboard() else {
        return;
    };
    let Some(clipboard) = guard.as_ref() else {
        return;
    };
    if clipboard
        .store(
            clipboard.setter.atoms.primary,
            clipboard.setter.atoms.utf8_string,
            text.as_bytes(),
        )
        .is_err()
    {
        log_warn!(
            LogCategory::Resources,
            "[X11] failed to claim the PRIMARY selection"
        );
    }
}

/// Worker-thread side of a read. Blocking — never call this on the UI thread.
fn load_selection(kind: SelectionKind) -> Option<String> {
    let guard = clipboard()?;
    let clipboard = guard.as_ref()?;

    // CLIPBOARD first, PRIMARY as the fallback — both inside ONE request, so
    // the UI thread's single deadline covers the whole chain.
    let targets = match kind {
        SelectionKind::Clipboard => vec![
            clipboard.getter.atoms.clipboard,
            clipboard.getter.atoms.primary,
        ],
        SelectionKind::Primary => vec![clipboard.getter.atoms.primary],
    };

    for selection in targets {
        let Ok(data) = clipboard.load(
            selection,
            clipboard.getter.atoms.utf8_string,
            clipboard.getter.atoms.property,
            SELECTION_LOAD_TIMEOUT,
        ) else {
            continue;
        };
        if let Ok(s) = String::from_utf8(data) {
            if !s.is_empty() {
                return Some(s);
            }
        }
    }

    None
}

/// Hand a read to the worker and wait at most [`PASTE_UI_DEADLINE`] for it.
fn request(kind: SelectionKind) -> Option<String> {
    let (reply, answer) = mpsc::sync_channel(1);
    {
        let sender = worker()?;
        sender.send(ClipboardJob::Load { kind, reply }).ok()?;
    }
    await_selection_reply(&answer, PASTE_UI_DEADLINE)
}

/// Wait for the worker's answer, but never longer than `deadline`.
///
/// This is the whole point of the worker: an X11 selection read is a round trip
/// to ANOTHER process, and when that process is gone or wedged the read blocks
/// for its full timeout. On the UI thread, twice, at 3 s each, an ordinary
/// Ctrl+V froze the event loop — caret blink, tweens and rendering with it — for
/// six seconds. Giving up costs a paste; not giving up costs the frame loop.
fn await_selection_reply(
    answer: &mpsc::Receiver<Option<String>>,
    deadline: Duration,
) -> Option<String> {
    match answer.recv_timeout(deadline) {
        Ok(text) => text,
        Err(_) => {
            log_warn!(
                LogCategory::Resources,
                "[X11] selection owner did not answer within {:?} — pasting nothing",
                deadline
            );
            None
        }
    }
}

/// Read content from X11 system clipboard
///
/// Attempts to read from CLIPBOARD selection first, falls back to PRIMARY.
/// Returns the clipboard text content if available.
///
/// Non-blocking by UI-thread standards: the actual selection transfer runs on
/// the clipboard worker thread and this call gives up after
/// [`PASTE_UI_DEADLINE`] with an empty result.
pub fn get_clipboard_content() -> Option<String> {
    request(SelectionKind::Clipboard)
}

/// Read the PRIMARY selection only — the middle-click paste source.
pub fn get_primary_content() -> Option<String> {
    request(SelectionKind::Primary)
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

#[cfg(test)]
mod tests {
    use std::{sync::mpsc, time::Duration, time::Instant};

    use super::{await_selection_reply, PASTE_UI_DEADLINE, SELECTION_LOAD_TIMEOUT};

    /// A selection owner that is gone or wedged — routine the moment the app
    /// that copied the text exits — must cost a paste, not the frame loop.
    /// This read used to happen ON the UI thread, twice, at 3 s each.
    ///
    /// NEGATIVE CONTROL: `answer.recv()` in place of
    /// `answer.recv_timeout(deadline)` — the sender is still alive, so the
    /// call never returns and this test hangs instead of passing.
    #[test]
    fn a_dead_selection_owner_costs_only_the_deadline() {
        // Run the wait on a worker so a regression fails this test instead of
        // hanging the suite: the sender below stays alive for the whole test,
        // so a blocking receive would never return.
        let (keep_alive, answer) = mpsc::sync_channel::<Option<String>>(1);
        let (done_tx, done_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let started = Instant::now();
            let got = await_selection_reply(&answer, Duration::from_millis(80));
            let _ = done_tx.send((got, started.elapsed()));
        });

        let (got, elapsed) = done_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("a silent selection owner must not hold the caller forever");
        assert_eq!(got, None);
        assert!(
            elapsed < Duration::from_secs(2),
            "the UI thread waited {elapsed:?} on a silent owner"
        );
        drop(keep_alive);
    }

    /// An owner that IS alive answers in a few milliseconds and its text must
    /// come through untouched.
    #[test]
    fn a_live_selection_owner_is_pasted() {
        let (reply, answer) = mpsc::sync_channel(1);
        reply.send(Some("hello".to_string())).unwrap();

        assert_eq!(
            await_selection_reply(&answer, Duration::from_secs(5)),
            Some("hello".to_string())
        );
    }

    /// The budget the UI thread is allowed to spend on a paste. Six seconds of
    /// frozen event loop is what this replaced; anything approaching a second
    /// is a visible stall, not a hitch.
    ///
    /// NEGATIVE CONTROL: restore `PASTE_UI_DEADLINE` to
    /// `Duration::from_secs(3)`.
    #[test]
    fn the_ui_paste_budget_stays_sub_second() {
        assert!(
            PASTE_UI_DEADLINE <= Duration::from_millis(500),
            "PASTE_UI_DEADLINE = {PASTE_UI_DEADLINE:?}"
        );
        assert!(
            SELECTION_LOAD_TIMEOUT <= Duration::from_millis(1000),
            "the worker must not hold the clipboard mutex for seconds"
        );
    }
}
