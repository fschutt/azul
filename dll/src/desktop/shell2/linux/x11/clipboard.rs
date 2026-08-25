//! X11 clipboard integration using x11-clipboard crate
//!
//! Reads and writes both the CLIPBOARD (Ctrl+C/V) and PRIMARY (select /
//! middle-click) selections, on a worker thread — see [`worker`].
//!
//! # Multi-flavor reads, and why they are probes rather than a `TARGETS` query
//!
//! ICCCM says to ask for `TARGETS` first and then convert each target in turn.
//! That is not reachable through `x11-clipboard` 0.9.3: its `Clipboard::load`
//! rejects any reply whose `type_` differs from the target it asked for
//! (`Error::UnexpectedType`), and a `TARGETS` conversion answers with type
//! `ATOM` by definition — so the enumeration always errors before it returns.
//!
//! [`load_payload`] therefore *probes*: it converts a fixed, rank-ordered list
//! of the targets azul has a codec for, and keeps every one that answers. A
//! target the owner does not offer answers with no property and costs one
//! cheap round trip; a **dead owner costs the full timeout**, so the probe
//! stops at the first target that times out rather than paying it eight times.
//!
//! What this loses is flavors nothing here knows about: a private format
//! cannot be carried through as `RichItem::Unknown` when the only way to learn
//! its name is the `TARGETS` list. Lifting that needs either an
//! `x11-clipboard` that tolerates a differently-typed reply, or a direct
//! `x11rb` connection of our own.
//!
//! # Writes are single-flavor, and cannot be otherwise here
//!
//! There is no multi-flavor write through this crate at all. Its owner state
//! is a `HashMap<selection, (target, value)>` — **one** target per selection —
//! and its `SelectionRequest` handler answers a `TARGETS` query with exactly
//! that one target. So `store()` called twice does not add a second flavor, it
//! replaces the first.
//!
//! A copy therefore publishes plain text and nothing else: it is the flavor
//! every X client can paste, and offering RTF *instead* would break every
//! plain-text target to style one. Styled text survives a copy on macOS (see
//! `macos/clipboard.rs`), and on X11 it does not. Lifting this needs a
//! selection owner that serves several targets, which is a rewrite of the
//! owner loop rather than a call-site change.
//!
//! INCR is handled inside `Clipboard::load` and is mandatory in practice —
//! anything past roughly 256 KB arrives that way, and a screenshot always
//! will. Its `SizeHint::AtLeast` lower bound is *not* reachable either: the
//! crate reads it only to `reserve` the buffer and never reports it, so the
//! size guard here is a post-hoc length check rather than the pre-read
//! rejection Windows and macOS get.

use std::sync::mpsc::{self, Sender, SyncSender};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Duration;

use rich_clipboard::{ClipboardItem, ClipboardPayload, Flavor, Platform};
use x11_clipboard::Clipboard;

use super::super::super::common::clipboard::MAX_FLAVOR_BYTES;
use super::super::super::common::debug_server::LogCategory;
use crate::log_warn;

/// The selection targets a payload read probes for, richest first.
///
/// Ordered by `Flavor::read_rank` so the expensive round trips happen in the
/// order the decode policy would prefer them anyway. Kept deliberately short:
/// every entry is one X round trip, and each is a target azul has a codec for.
///
/// `text/plain;charset=utf-8` is last and is the one that must always be
/// tried — it is what every X client offers and the floor every paste falls
/// back to. `UTF8_STRING` is its pre-MIME spelling, still what older toolkits
/// and terminals publish.
const PROBE_TARGETS: &[&str] = &[
    "text/uri-list",
    "text/rtf",
    "text/html",
    "image/png",
    "text/plain;charset=utf-8",
    "UTF8_STRING",
];

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

/// The text this process last put on the clipboard.
///
/// It is what makes the copy SAFE the instant the user presses Ctrl+X, before
/// any X traffic has happened: [`get_clipboard_content`] falls back to it when
/// the selection read comes back with nothing, so a Cut can never delete text
/// that then turns out to be unpasteable.
static LAST_WRITTEN: Mutex<Option<String>> = Mutex::new(None);

/// The same, for PRIMARY. A separate cell because the two selections carry
/// DIFFERENT text: PRIMARY follows every selection gesture, CLIPBOARD only an
/// explicit copy, and answering a middle click out of the copy cell would
/// paste whatever was last copied instead of what is selected.
static LAST_PRIMARY: Mutex<Option<String>> = Mutex::new(None);

/// Park what we are about to put on PRIMARY. Both writers go through here:
/// `write_to_clipboard` stores to PRIMARY as well as CLIPBOARD, and
/// `write_to_primary` stores to PRIMARY alone.
fn park_primary(text: &str) {
    if let Ok(mut parked) = LAST_PRIMARY.lock() {
        *parked = Some(text.to_owned());
    }
}

/// Write text to the X11 clipboard.
///
/// Writes to both CLIPBOARD (Ctrl+C/V) and PRIMARY (middle-click) selections.
///
/// NOT on the UI thread any more. `Clipboard::store` is four synchronous X
/// round trips per copy — `set_selection_owner(..).check()` and
/// `get_selection_owner(..).reply()`, for CLIPBOARD and again for PRIMARY —
/// plus a write lock shared with the in-flight read on the getter thread. That
/// ran on the UI thread on every Ctrl+C and every Ctrl+X, and blocking calls
/// on the UI thread are forbidden.
///
/// The return value could not simply be dropped: `set_system_clipboard()` in
/// `common/event.rs` gates whether Cut DELETES the selection on it, so an
/// unconditional `Ok` would let a failed copy destroy the user's text. What
/// makes `Ok` truthful without waiting is [`LAST_WRITTEN`]: the text is parked
/// in-process FIRST, so it is already retrievable by the time this returns,
/// and only the X handoff is left to the worker.
pub fn write_to_clipboard(text: &str) -> Result<(), ClipboardError> {
    let sender = worker().ok_or(ClipboardError::InitFailed)?;
    // A copy claims PRIMARY too, so both cells answer for it.
    park_primary(text);
    commit_copy(&LAST_WRITTEN, &sender, text)
}

/// The UI-thread half of a copy: park the text, then queue the X handoff.
///
/// Taking a `Sender` rather than doing the work is the whole point — a channel
/// send cannot block on the X server no matter how wedged the selection
/// machinery is.
fn commit_copy(
    park: &Mutex<Option<String>>,
    sender: &Sender<ClipboardJob>,
    text: &str,
) -> Result<(), ClipboardError> {
    // Park BEFORE queueing: this is the step that makes `Ok` true.
    match park.lock() {
        Ok(mut parked) => *parked = Some(text.to_owned()),
        Err(_) => return Err(ClipboardError::WriteFailed),
    }
    sender
        .send(ClipboardJob::Store(text.to_owned()))
        .map_err(|_| ClipboardError::WriteFailed)
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
    park_primary(text);
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
#[derive(Debug)]
enum ClipboardJob {
    /// Read a selection and answer on `reply`.
    Load {
        kind: SelectionKind,
        reply: SyncSender<Option<String>>,
    },
    /// Read EVERY target the selection offers that we have a codec for, and
    /// answer on `reply`. Same worker and same deadline as `Load` — it is
    /// several round trips rather than one, which is exactly why it must not
    /// happen on the UI thread.
    LoadPayload {
        reply: SyncSender<Option<ClipboardPayload>>,
    },
    /// Take ownership of PRIMARY with this text (fire and forget).
    ClaimPrimary(String),
    /// Take ownership of CLIPBOARD *and* PRIMARY with this text — the four
    /// X round trips that used to happen on the UI thread.
    Store(String),
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
                        ClipboardJob::LoadPayload { reply } => {
                            let _ = reply.try_send(load_selection_payload());
                        }
                        ClipboardJob::ClaimPrimary(text) => claim_primary(&text),
                        ClipboardJob::Store(text) => store_both_selections(&text),
                    }
                }
            });
        Mutex::new(tx)
    });
    m.lock().ok()
}

/// Worker-thread side of a copy: take CLIPBOARD (Ctrl+C/V) and PRIMARY
/// (middle-click). Blocking — never call this on the UI thread.
fn store_both_selections(text: &str) {
    let Some(guard) = clipboard() else {
        return;
    };
    let Some(clipboard) = guard.as_ref() else {
        return;
    };
    for selection in [
        clipboard.setter.atoms.clipboard,
        clipboard.setter.atoms.primary,
    ] {
        if clipboard
            .store(selection, clipboard.setter.atoms.utf8_string, text.as_bytes())
            .is_err()
        {
            log_warn!(
                LogCategory::Resources,
                "[X11] failed to take a selection for the copied text — pasting inside this \
                 app still works from the parked copy, other apps will not see it"
            );
        }
    }
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

/// Worker-thread side of a multi-flavor read. Blocking — never call this on
/// the UI thread.
///
/// Probes [`PROBE_TARGETS`] in order (see the module docs for why this is not
/// a `TARGETS` enumeration) and keeps every target that answers with bytes.
///
/// **Stops at the first target that errors.** An owner that is gone or wedged
/// makes every probe pay the full [`SELECTION_LOAD_TIMEOUT`], and paying that
/// six times over is how a paste turns into a multi-second stall on the
/// clipboard worker (and, through the mutex it holds, on the next copy).
fn load_selection_payload() -> Option<ClipboardPayload> {
    let guard = clipboard()?;
    let clipboard = guard.as_ref()?;

    let mut payload = ClipboardPayload::new(Platform::Unix);
    let mut seen: Vec<Flavor<'static>> = Vec::new();

    for target in PROBE_TARGETS {
        // BEFORE the transfer, not after: `UTF8_STRING` and
        // `text/plain;charset=utf-8` are one flavor under two names, and
        // fetching the second only to discard it costs a whole X round trip
        // in the case that happens on almost every paste.
        let flavor = Flavor::from_mime(target);
        if seen.contains(&flavor) {
            continue;
        }
        let Ok(atom) = clipboard.getter.get_atom(target) else {
            continue;
        };
        let loaded = clipboard.load(
            clipboard.getter.atoms.clipboard,
            atom,
            clipboard.getter.atoms.property,
            SELECTION_LOAD_TIMEOUT,
        );
        let bytes = match loaded {
            Ok(bytes) => bytes,
            Err(_) => {
                // The owner did not answer. Every remaining probe would pay
                // the same timeout, so stop and use whatever already came
                // back — the ranked order means that is the best of them.
                log_warn!(
                    LogCategory::Resources,
                    "[X11] selection owner stopped answering at target `{target}` — using the \
                     {} flavor(s) already read",
                    payload.len()
                );
                break;
            }
        };
        if bytes.is_empty() {
            // Target not offered. Cheap, and not an error.
            continue;
        }
        if bytes.len() as u64 > MAX_FLAVOR_BYTES {
            // Post-hoc, not pre-read: `Clipboard::load` never reports the
            // INCR lower bound it saw, so there is nothing to reject on
            // before the transfer. See the module docs.
            log_warn!(
                LogCategory::Resources,
                "[X11] dropping selection target `{target}`: {} bytes exceeds the \
                 {MAX_FLAVOR_BYTES}-byte cap",
                bytes.len()
            );
            continue;
        }

        seen.push(flavor);
        payload.push(ClipboardItem::new(*target, bytes));
    }

    (!payload.is_empty()).then_some(payload)
}

/// Read every flavor the CLIPBOARD selection offers.
///
/// Non-blocking by UI-thread standards, like [`get_clipboard_content`]: the
/// probes run on the clipboard worker and this gives up after
/// [`PASTE_UI_DEADLINE`].
///
/// Falls back to the text this process last copied when the selection answers
/// with nothing, for the same reason [`get_clipboard_content`] does: a copy is
/// parked in-process before the X handoff is even queued, so a Ctrl+C followed
/// immediately by Ctrl+V must still paste.
pub fn read_payload() -> Option<ClipboardPayload> {
    let (reply, answer) = mpsc::sync_channel(1);
    {
        let sender = worker()?;
        sender.send(ClipboardJob::LoadPayload { reply }).ok()?;
    }
    let from_selection = match answer.recv_timeout(PASTE_UI_DEADLINE) {
        Ok(payload) => payload,
        Err(_) => {
            log_warn!(
                LogCategory::Resources,
                "[X11] selection owner did not answer within {PASTE_UI_DEADLINE:?} — pasting \
                 nothing"
            );
            None
        }
    };
    if let Some(payload) = from_selection {
        return Some(payload);
    }
    let parked = LAST_WRITTEN.lock().ok().and_then(|g| g.clone())?;
    rich_clipboard::encode(&rich_clipboard::RichItem::Text(parked), Platform::Unix).ok()
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
    resolve_paste(
        request(SelectionKind::Clipboard),
        LAST_WRITTEN.lock().ok().and_then(|g| g.clone()),
    )
}

/// What a paste resolves to: the live X selection, or — only when that answered
/// with nothing — the text this process last copied.
///
/// The fallback is what lets [`write_to_clipboard`] return `Ok` without waiting
/// on the X handoff. It is second, never first: another application that owns
/// the selection must win, or Ctrl+C elsewhere followed by Ctrl+V here would
/// paste our own stale text.
fn resolve_paste(from_selection: Option<String>, parked: Option<String>) -> Option<String> {
    from_selection.filter(|s| !s.is_empty()).or(parked)
}

/// Read the PRIMARY selection only — the middle-click paste source.
///
/// Same fallback as [`get_clipboard_content`], and for the same reason: the
/// claim is queued to the worker now, so a middle click a few milliseconds
/// after the selection gesture can beat the X handoff. Without the fallback
/// that paste would come back empty.
pub fn get_primary_content() -> Option<String> {
    resolve_paste(
        request(SelectionKind::Primary),
        LAST_PRIMARY.lock().ok().and_then(|g| g.clone()),
    )
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
    use std::sync::Mutex;
    use std::{sync::mpsc, time::Duration, time::Instant};

    use super::{
        await_selection_reply, commit_copy, resolve_paste, ClipboardJob, PASTE_UI_DEADLINE,
        SELECTION_LOAD_TIMEOUT,
    };

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

    /// A copy hands the four X round trips to the worker and returns — even
    /// when nothing is draining the queue, which is the worst the X server can
    /// do to us. `Clipboard::store` is `set_selection_owner(..).check()` plus
    /// `get_selection_owner(..).reply()`, twice (CLIPBOARD and PRIMARY), and
    /// all of it used to run on the UI thread on every Ctrl+C.
    ///
    /// NEGATIVE CONTROL: drop the `sender.send(...)` from `commit_copy` — the
    /// `try_recv` below reports `Empty` and the copy never reaches X at all.
    #[test]
    fn a_copy_queues_the_selection_handoff_instead_of_performing_it() {
        let park = Mutex::new(None);
        let (tx, rx) = mpsc::channel::<ClipboardJob>();

        let started = Instant::now();
        assert!(commit_copy(&park, &tx, "hello").is_ok());
        let elapsed = started.elapsed();

        assert!(
            elapsed < Duration::from_millis(50),
            "the UI thread spent {elapsed:?} on a copy"
        );
        match rx.try_recv() {
            Ok(ClipboardJob::Store(text)) => assert_eq!(text, "hello"),
            other => panic!("the copy never reached the worker: {other:?}"),
        }
    }

    /// `Ok` from a copy is what `set_system_clipboard` gates the Cut DELETION
    /// on, so it has to mean "this text is safe" — which is only true because
    /// the copy is parked in-process before the queueing.
    ///
    /// NEGATIVE CONTROL: remove the `*parked = Some(...)` from `commit_copy` —
    /// the assert below sees `None` and a Cut would have deleted text nothing
    /// was holding.
    #[test]
    fn a_copy_is_retrievable_before_any_x_traffic_has_happened() {
        let park = Mutex::new(None);
        let (tx, _rx) = mpsc::channel::<ClipboardJob>();

        assert!(commit_copy(&park, &tx, "cut me").is_ok());

        assert_eq!(
            resolve_paste(None, park.lock().unwrap().clone()),
            Some("cut me".to_string()),
            "the worker has not run yet, so only the parked copy can answer"
        );
    }

    /// CLIPBOARD and PRIMARY carry DIFFERENT text — PRIMARY follows every
    /// selection gesture, CLIPBOARD only an explicit copy — so they need
    /// separate parked cells. One shared cell would answer a middle click
    /// with whatever was last COPIED instead of what is selected.
    ///
    /// NEGATIVE CONTROL: make `write_to_primary` park into `LAST_WRITTEN`, or
    /// make `get_primary_content` read `LAST_WRITTEN`.
    #[test]
    fn the_two_selections_do_not_share_a_parked_copy() {
        use super::{LAST_PRIMARY, LAST_WRITTEN};
        *LAST_WRITTEN.lock().unwrap() = Some("copied".to_string());
        *LAST_PRIMARY.lock().unwrap() = Some("selected".to_string());

        assert_eq!(
            resolve_paste(None, LAST_WRITTEN.lock().unwrap().clone()),
            Some("copied".to_string())
        );
        assert_eq!(
            resolve_paste(None, LAST_PRIMARY.lock().unwrap().clone()),
            Some("selected".to_string())
        );
    }

    /// The parked copy is a FALLBACK. Another application that owns the
    /// selection wins, or Ctrl+C elsewhere then Ctrl+V here would paste our
    /// own stale text.
    ///
    /// NEGATIVE CONTROL: swap the order in `resolve_paste` to
    /// `parked.or(from_selection)`.
    #[test]
    fn a_live_selection_owner_beats_the_parked_copy() {
        assert_eq!(
            resolve_paste(Some("theirs".into()), Some("ours".into())),
            Some("theirs".to_string())
        );
        assert_eq!(
            resolve_paste(None, Some("ours".into())),
            Some("ours".to_string())
        );
        // An owner that answers with an EMPTY string is answering with
        // nothing, and must not shadow the fallback either.
        assert_eq!(
            resolve_paste(Some(String::new()), Some("ours".into())),
            Some("ours".to_string())
        );
        assert_eq!(resolve_paste(None, None), None);
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
