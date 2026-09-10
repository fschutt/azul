//! Runtime side of the request / resume primitive.
//!
//! Every OS-facing operation the browser can only answer asynchronously is
//! split in two on every target:
//!
//! * **request** - `FileDialog::open_file(.., data, on_result) -> RequestId` registers `{RequestId,
//!   data, on_result}` here and returns the id.
//! * **resume** - once the result exists the *runtime* invokes `on_result(data, CallbackInfo,
//!   result)` as a fresh, ordinary callback activation. A resume may issue the next request, so a
//!   chain of awaits becomes a chain of resumes and the app's callbacks form a state machine over
//!   its own `RefAny`.
//!
//! This module is the one place where requests are parked. On desktop the
//! request function usually performs the (modal, blocking) OS call itself and
//! calls [`complete`] straight away; the completion is still queued rather
//! than invoked, which is what gives every target the same ordering
//! guarantee: *the result callback never runs re-entrantly inside the
//! requesting activation.* Mobile pickers, whose OS delegates answer later,
//! use [`defer`] with a poll closure the pump asks once per frame.
//!
//! The pump - [`take_completed`] - is driven by the shells after every
//! activation and from the per-frame timer/thread tick, and by the E2E runner
//! in the same places. `LayoutWindow::run_completed_requests` turns the drained
//! entries into callback invocations.
//!
//! ## Why a process-wide queue and not `CallbackInfo::push_change`
//!
//! The request functions are ordinary static API functions - they take no
//! `CallbackInfo`, because a `RequestId`-returning signature has to be
//! callable from anywhere (a timer, a thread writeback, another resume). The
//! queue is therefore owned by the runtime, not by an activation. It is the
//! single choke point a web host replaces: a lifted build classifies
//! [`complete_erased`] / [`defer`] / [`take_completed`] as boundary imports
//! and services them from JavaScript, and nothing else changes.
//!
//! ## Delivery order and ownership
//!
//! * FIFO. Nested completions (a resume that issues and immediately completes another request) are
//!   appended to the end, never recursed into.
//! * Entries are delivered by whichever window pumps next. Synchronous completions (every desktop
//!   dialog / file / http call) are pumped by the requesting window right after the requesting
//!   activation returns, so in practice a request resumes on the window that issued it.
//! * The queue holds a clone of the app's `data` until delivery, keeping the `RefAny` alive across
//!   the gap. There is no cancellation in v1.

use alloc::{boxed::Box, vec::Vec};

use azul_core::{refany::RefAny, task::RequestId};

use crate::callbacks::ResumeCallback;

/// A request whose result exists and whose callback has not run yet.
pub struct CompletedRequest {
    pub request_id: RequestId,
    /// The app's context, exactly as submitted to the request function.
    pub data: RefAny,
    pub callback: ResumeCallback,
    /// The per-operation result struct, type-erased.
    pub result: RefAny,
}

impl core::fmt::Debug for CompletedRequest {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CompletedRequest")
            .field("request_id", &self.request_id)
            .field("callback", &self.callback)
            .finish_non_exhaustive()
    }
}

/// Asked by the pump once per frame; returns the type-erased result struct
/// once the platform has answered, `None` while the request is still open.
pub type PollFn = Box<dyn FnMut() -> Option<RefAny> + Send>;

/// A request the platform answers asynchronously (a mobile file picker whose
/// delegate fires later).
pub struct PendingRequest {
    pub request_id: RequestId,
    pub data: RefAny,
    pub callback: ResumeCallback,
    pub poll: PollFn,
}

impl core::fmt::Debug for PendingRequest {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("PendingRequest")
            .field("request_id", &self.request_id)
            .field("callback", &self.callback)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "std")]
mod queue {
    use std::sync::Mutex;

    use super::{CompletedRequest, PendingRequest};

    pub(super) struct RequestQueue {
        pub completed: Vec<CompletedRequest>,
        pub pending: Vec<PendingRequest>,
    }

    pub(super) static REQUEST_QUEUE: Mutex<RequestQueue> = Mutex::new(RequestQueue {
        completed: Vec::new(),
        pending: Vec::new(),
    });

    pub(super) fn with_queue<R>(f: impl FnOnce(&mut RequestQueue) -> R) -> R {
        // A poisoned lock only means a callback panicked while the queue was
        // held; the queue itself is still a plain Vec pair, so keep serving.
        let mut guard = REQUEST_QUEUE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut guard)
    }
}

/// Registers a request whose result is already known and returns its id.
///
/// This is what every desktop request function calls after its synchronous
/// OS call: the callback runs on the next pump, never inside the caller.
pub fn complete<T: 'static>(data: RefAny, on_result: ResumeCallback, result: T) -> RequestId {
    complete_erased(data, on_result, RefAny::new(result))
}

/// [`complete`] with an already type-erased result struct.
#[must_use]
pub fn complete_erased(data: RefAny, on_result: ResumeCallback, result: RefAny) -> RequestId {
    let request_id = RequestId::unique();
    #[cfg(feature = "std")]
    queue::with_queue(|q| {
        q.completed.push(CompletedRequest {
            request_id,
            data,
            callback: on_result,
            result,
        });
    });
    #[cfg(not(feature = "std"))]
    {
        // Without std there is no runtime pump; the request is acknowledged
        // but never resumed. Every request function documents that no_std
        // builds have no OS-facing implementation.
        drop((data, on_result, result));
    }
    request_id
}

/// Registers a request the platform will answer later. `poll` is asked once
/// per frame until it returns `Some(result)`.
#[must_use]
pub fn defer(data: RefAny, on_result: ResumeCallback, poll: PollFn) -> RequestId {
    let request_id = RequestId::unique();
    #[cfg(feature = "std")]
    queue::with_queue(|q| {
        q.pending.push(PendingRequest {
            request_id,
            data,
            callback: on_result,
            poll,
        });
    });
    #[cfg(not(feature = "std"))]
    drop((data, on_result, poll));
    request_id
}

/// Drains every completed request, polling the pending ones first. Returns
/// them in the order they completed; an empty vector means nothing to do.
///
/// Called by the shells' pump. Resumes issued *during* delivery land in the
/// queue again and are picked up by the next call, which is how nested
/// completions stay FIFO instead of recursing.
#[must_use]
pub fn take_completed() -> Vec<CompletedRequest> {
    #[cfg(feature = "std")]
    {
        queue::with_queue(|q| {
            let mut still_pending = Vec::with_capacity(q.pending.len());
            for mut p in q.pending.drain(..) {
                match (p.poll)() {
                    Some(result) => q.completed.push(CompletedRequest {
                        request_id: p.request_id,
                        data: p.data,
                        callback: p.callback,
                        result,
                    }),
                    None => still_pending.push(p),
                }
            }
            q.pending = still_pending;
            core::mem::take(&mut q.completed)
        })
    }
    #[cfg(not(feature = "std"))]
    {
        Vec::new()
    }
}

/// `true` while any request is completed-but-undelivered or still pending.
/// Shells use it to keep their frame loop ticking until the queue is empty.
#[must_use]
pub fn has_work() -> bool {
    #[cfg(feature = "std")]
    {
        queue::with_queue(|q| !q.completed.is_empty() || !q.pending.is_empty())
    }
    #[cfg(not(feature = "std"))]
    {
        false
    }
}

/// Number of requests waiting on a platform answer - a debug probe for tests
/// that assert the queue does not grow without bound.
#[must_use]
pub fn pending_count() -> usize {
    #[cfg(feature = "std")]
    {
        queue::with_queue(|q| q.pending.len())
    }
    #[cfg(not(feature = "std"))]
    {
        0
    }
}

/// Deterministic answers for request functions under end-to-end tests.
///
/// A picker or a permission prompt cannot run inside an automated test - a
/// hanging modal is the worst outcome an e2e run can have - so every request
/// function asks this store FIRST. The store is *armed* under an e2e run
/// (`AZ_E2E` / `AZ_E2E_TEST` set, or `AZ_BACKEND=headless`; explicitly with
/// [`arm`]) and disarmed in production, where it costs one relaxed load.
///
/// * A **mocked** operation resumes immediately with the canned answer; the resume path is the
///   normal one, so a mocked test exercises the whole request / resume machinery except the OS call
///   itself.
/// * An **unmocked** picker under an armed store resolves as *cancelled* and is recorded (and
///   printed) as an unmocked request, so the scenario can assert on it
///   (`assert_no_unmocked_requests`) instead of hanging.
/// * Reads (`FilePath::read_*`) are served from the canned table when the path is registered and
///   from the real file system otherwise: files a scenario created are real, virtual `e2e://`
///   documents are canned.
/// * `FileDialog::save_bytes` never shows a dialog while armed: the bytes are recorded in
///   [`saved_files`] for `assert_saved_file`.
///
/// The JSON scenario op `{"op": "mock", "set": {...}}` fills the store; the
/// same op is what the browser lane maps onto `window.__az_e2e_mock`.
#[cfg(feature = "std")]
pub mod mock {
    use alloc::{
        collections::{BTreeMap, VecDeque},
        string::String,
        vec::Vec,
    };
    use std::sync::Mutex;

    use azul_css::{props::basic::color::ColorU, AzString};

    /// A canned HTTP answer.
    #[derive(Debug, Clone)]
    pub struct MockHttpResponse {
        pub status: u16,
        pub body: Vec<u8>,
        pub content_type: AzString,
    }

    /// What a mocked HTTP request resolves to.
    #[derive(Debug, Clone)]
    pub enum MockHttp {
        Response(MockHttpResponse),
        /// The transport failed (an `HttpError::Other` with this message).
        Error(AzString),
    }

    /// One `FileDialog::save_bytes` export recorded under an armed store.
    #[derive(Debug, Clone)]
    pub struct SavedFile {
        pub name: AzString,
        pub mime: AzString,
        pub bytes: Vec<u8>,
    }

    /// The store's verdict for one request.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Answer<T> {
        /// Not an e2e run: perform the real OS call.
        NotArmed,
        /// Resume with this canned answer.
        Mocked(T),
        /// An e2e run with no answer queued: resolve as cancelled, loudly.
        Unmocked,
    }

    #[derive(Default)]
    struct MockState {
        /// `None` = decide from the environment on first use.
        armed: Option<bool>,
        file_open: VecDeque<Option<AzString>>,
        file_open_multi: VecDeque<Vec<AzString>>,
        color_pick: VecDeque<Option<ColorU>>,
        save_file: VecDeque<Option<AzString>>,
        save_bytes_accept: Option<bool>,
        file_reads: BTreeMap<String, Vec<u8>>,
        /// `(url pattern, answer)`: an exact URL, a prefix ending in `*`, or
        /// `*` for everything; first match wins.
        http: Vec<(String, MockHttp)>,
        audio_devices: Option<(Vec<AzString>, Vec<AzString>)>,
        video_decode_none: bool,
        saved_files: Vec<SavedFile>,
        unmocked: Vec<String>,
    }

    static STATE: Mutex<MockState> = Mutex::new(MockState {
        armed: None,
        file_open: VecDeque::new(),
        file_open_multi: VecDeque::new(),
        color_pick: VecDeque::new(),
        save_file: VecDeque::new(),
        save_bytes_accept: None,
        file_reads: BTreeMap::new(),
        http: Vec::new(),
        audio_devices: None,
        video_decode_none: false,
        saved_files: Vec::new(),
        unmocked: Vec::new(),
    });

    fn with<R>(f: impl FnOnce(&mut MockState) -> R) -> R {
        let mut guard = STATE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut guard)
    }

    fn env_armed() -> bool {
        let set = |k: &str| std::env::var(k).map(|v| !v.is_empty()).unwrap_or(false);
        set("AZ_E2E")
            || set("AZ_E2E_TEST")
            || std::env::var("AZ_BACKEND").as_deref() == Ok("headless")
    }

    fn armed_in(s: &mut MockState) -> bool {
        *s.armed.get_or_insert_with(env_armed)
    }

    /// Arm the store explicitly (an e2e host that is not driven by the
    /// environment variables).
    pub fn arm() {
        with(|s| s.armed = Some(true));
    }

    /// Disarm the store: every request performs its real OS call again.
    pub fn disarm() {
        with(|s| s.armed = Some(false));
    }

    /// Whether requests are being answered from the store.
    #[must_use]
    pub fn is_armed() -> bool {
        with(armed_in)
    }

    /// Forget every queued answer, canned read and record; the armed state
    /// stays.
    pub fn reset() {
        with(|s| {
            let armed = s.armed;
            *s = MockState::default();
            s.armed = armed;
        });
    }

    /// The answer a scenario queued for `op`, or - when the queue is empty -
    /// the recorded "unmocked" fallback.
    fn answer<T>(s: &mut MockState, taken: Option<T>, op: &str) -> Answer<T> {
        if let Some(a) = taken {
            Answer::Mocked(a)
        } else {
            unmocked(s, op);
            Answer::Unmocked
        }
    }

    fn unmocked(s: &mut MockState, op: &str) {
        eprintln!("[azul][e2e] unmocked request under e2e: {op} (resolving as cancelled)");
        s.unmocked.push(String::from(op));
    }

    // ---- setters (the `mock` scenario op) ---------------------------------

    /// Queue the answer of the next `FileDialog::open_file` /
    /// `open_directory` (`None` = the user cancelled).
    pub fn push_file_open(path: Option<AzString>) {
        with(|s| s.file_open.push_back(path));
    }

    /// Queue the answer of the next `FileDialog::open_multiple_files`.
    pub fn push_file_open_multi(paths: Vec<AzString>) {
        with(|s| s.file_open_multi.push_back(paths));
    }

    /// Queue the answer of the next `ColorPickerDialog::open`.
    pub fn push_color_pick(color: Option<ColorU>) {
        with(|s| s.color_pick.push_back(color));
    }

    /// Queue the path the next `FileDialog::save_file` resolves to.
    pub fn push_save_file(path: Option<AzString>) {
        with(|s| s.save_file.push_back(path));
    }

    /// Whether `FileDialog::save_bytes` reports success while armed (default
    /// `true`); the bytes are recorded either way.
    pub fn set_save_bytes_accept(accept: bool) {
        with(|s| s.save_bytes_accept = Some(accept));
    }

    /// Serve `bytes` for every `FilePath::read_*` of `path`.
    pub fn set_file_read(path: impl Into<String>, bytes: Vec<u8>) {
        with(|s| {
            s.file_reads.insert(path.into(), bytes);
        });
    }

    /// Answer HTTP requests whose URL matches `pattern` (exact, `prefix*`,
    /// or `*`). Patterns are tried in registration order.
    pub fn add_http(pattern: impl Into<String>, answer: MockHttp) {
        with(|s| s.http.push((pattern.into(), answer)));
    }

    /// The device lists `AudioDeviceList::enumerate` resumes with.
    pub fn set_audio_devices(outputs: Vec<AzString>, inputs: Vec<AzString>) {
        with(|s| s.audio_devices = Some((outputs, inputs)));
    }

    /// Make `DecodedVideo::decode_mp4_h264` resume with `video: None`
    /// without touching a codec.
    pub fn set_video_decode_none(mocked: bool) {
        with(|s| s.video_decode_none = mocked);
    }

    // ---- consumers (the request functions) ---------------------------------

    /// `op` names the caller for the unmocked record, e.g.
    /// `"FileDialog::open_file"`.
    #[must_use]
    pub fn take_file_open(op: &str) -> Answer<Option<AzString>> {
        with(|s| {
            if !armed_in(s) {
                return Answer::NotArmed;
            }
            let taken = s.file_open.pop_front();
            answer(s, taken, op)
        })
    }

    #[must_use]
    pub fn take_file_open_multi() -> Answer<Vec<AzString>> {
        with(|s| {
            if !armed_in(s) {
                return Answer::NotArmed;
            }
            let taken = s.file_open_multi.pop_front();
            answer(s, taken, "FileDialog::open_multiple_files")
        })
    }

    #[must_use]
    pub fn take_color_pick() -> Answer<Option<ColorU>> {
        with(|s| {
            if !armed_in(s) {
                return Answer::NotArmed;
            }
            let taken = s.color_pick.pop_front();
            answer(s, taken, "ColorPickerDialog::open")
        })
    }

    #[must_use]
    pub fn take_save_file() -> Answer<Option<AzString>> {
        with(|s| {
            if !armed_in(s) {
                return Answer::NotArmed;
            }
            let taken = s.save_file.pop_front();
            answer(s, taken, "FileDialog::save_file")
        })
    }

    /// Canned bytes for `path`, if a scenario registered them. `None` means
    /// "read the real file" - reads never count as unmocked.
    #[must_use]
    pub fn take_file_read(path: &str) -> Option<Vec<u8>> {
        with(|s| {
            if !armed_in(s) {
                return None;
            }
            s.file_reads.get(path).cloned()
        })
    }

    #[must_use]
    pub fn take_http(url: &str) -> Answer<MockHttp> {
        with(|s| {
            if !armed_in(s) {
                return Answer::NotArmed;
            }
            let found = s.http.iter().find(|(pattern, _)| {
                pattern == "*"
                    || pattern == url
                    || pattern
                        .strip_suffix('*')
                        .is_some_and(|prefix| url.starts_with(prefix))
            });
            let taken = found.map(|(_, canned)| canned.clone());
            answer(s, taken, &alloc::format!("http {url}"))
        })
    }

    #[must_use]
    pub fn take_audio_devices() -> Answer<(Vec<AzString>, Vec<AzString>)> {
        with(|s| {
            if !armed_in(s) {
                return Answer::NotArmed;
            }
            let taken = s.audio_devices.clone();
            answer(s, taken, "AudioDeviceList::enumerate")
        })
    }

    /// `true` when an armed store asked for `decode_mp4_h264` to resume with
    /// `video: None`.
    #[must_use]
    pub fn video_decode_mocked() -> bool {
        with(|s| armed_in(s) && s.video_decode_none)
    }

    /// Records an export while armed; `None` when not armed (show the real
    /// dialog), `Some(accepted)` otherwise.
    #[must_use]
    pub fn record_saved_file(name: &AzString, mime: &AzString, bytes: &[u8]) -> Option<bool> {
        with(|s| {
            if !armed_in(s) {
                return None;
            }
            s.saved_files.push(SavedFile {
                name: name.clone(),
                mime: mime.clone(),
                bytes: bytes.to_vec(),
            });
            Some(s.save_bytes_accept.unwrap_or(true))
        })
    }

    // ---- records (the assertions) --------------------------------------------

    /// Every export recorded since the last [`reset`].
    #[must_use]
    pub fn saved_files() -> Vec<SavedFile> {
        with(|s| s.saved_files.clone())
    }

    /// Every request that ran without an answer since the last [`reset`].
    #[must_use]
    pub fn unmocked_requests() -> Vec<String> {
        with(|s| s.unmocked.clone())
    }
}

#[cfg(all(test, feature = "std"))]
mod tests {
    use core::sync::atomic::{AtomicUsize, Ordering};

    use azul_core::callbacks::Update;

    use super::*;
    use crate::callbacks::CallbackInfo;

    extern "C" fn noop(_: RefAny, _: CallbackInfo, _: RefAny) -> Update {
        Update::DoNothing
    }

    #[test]
    fn ids_are_unique_and_valid() {
        let a = RequestId::unique();
        let b = RequestId::unique();
        assert!(a.is_valid() && b.is_valid());
        assert_ne!(a, b);
        assert!(!RequestId::invalid().is_valid());
    }

    #[test]
    fn complete_is_delivered_fifo_and_defer_waits_for_its_poll() {
        // Other tests share the process-wide queue; drain whatever they left.
        let _ = take_completed();

        let cb = ResumeCallback::create(noop);
        let first = complete(RefAny::new(1u8), cb.clone(), 10u32);
        let second = complete(RefAny::new(2u8), cb.clone(), 20u32);

        static POLLS: AtomicUsize = AtomicUsize::new(0);
        let deferred = defer(
            RefAny::new(3u8),
            cb,
            Box::new(|| {
                if POLLS.fetch_add(1, Ordering::SeqCst) == 0 {
                    None
                } else {
                    Some(RefAny::new(30u32))
                }
            }),
        );
        assert!(has_work());
        assert_eq!(pending_count(), 1);

        let batch = take_completed();
        let ids: Vec<RequestId> = batch.iter().map(|c| c.request_id).collect();
        assert_eq!(ids, vec![first, second]);
        assert_eq!(pending_count(), 1, "poll answered None on the first pump");

        let batch = take_completed();
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].request_id, deferred);
        let mut result = batch.into_iter().next().unwrap().result;
        assert_eq!(result.downcast_ref::<u32>().map(|r| *r), Some(30));
        assert_eq!(pending_count(), 0);
        assert!(!has_work());
    }
}
