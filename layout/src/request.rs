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
//! Under this crate's own unit tests the queue is per THREAD instead (see
//! `queue`): tests run side by side, and a drainer takes everything, so a
//! shared queue let one test swallow the completion of the test beside it.
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
    use super::{CompletedRequest, PendingRequest};

    pub(super) struct RequestQueue {
        pub completed: Vec<CompletedRequest>,
        pub pending: Vec<PendingRequest>,
    }

    /// The runtime's queue: ONE for the process (the module doc says why).
    #[cfg(not(test))]
    static REQUEST_QUEUE: std::sync::Mutex<RequestQueue> = std::sync::Mutex::new(RequestQueue {
        completed: Vec::new(),
        pending: Vec::new(),
    });

    #[cfg(not(test))]
    pub(super) fn with_queue<R>(f: impl FnOnce(&mut RequestQueue) -> R) -> R {
        // A poisoned lock only means a callback panicked while the queue was
        // held; the queue itself is still a plain Vec pair, so keep serving.
        let mut guard = REQUEST_QUEUE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut guard)
    }

    // Under this crate's own tests: one queue per THREAD. The harness runs
    // tests side by side on its threads and every drainer takes everything
    // (`take_completed`; the e2e runner's pump inside many runner tests), so
    // a shared queue let a test swallow its neighbour's completion. Each
    // test issues its requests on its own thread - a `complete` there, or a
    // `defer` whose poll its own pump runs (an http worker thread only
    // answers the poll's channel) - so it sees exactly its own. The dll and
    // every product build keep the process-wide queue above.
    #[cfg(test)]
    std::thread_local! {
        static REQUEST_QUEUE: core::cell::RefCell<RequestQueue> =
            const {
                core::cell::RefCell::new(RequestQueue {
                    completed: Vec::new(),
                    pending: Vec::new(),
                })
            };
    }

    #[cfg(test)]
    pub(super) fn with_queue<R>(f: impl FnOnce(&mut RequestQueue) -> R) -> R {
        REQUEST_QUEUE.with(|queue| f(&mut queue.borrow_mut()))
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
/// [`arm`]) and disarmed in production, where it costs one relaxed load. A
/// bare headless launch (no script) still sends unmocked HTTP requests: no
/// modal can hang on one, and headless apps need the network. A scripted run
/// sends the unmocked ones `AZ_E2E_ALLOW_HTTP` names (comma-separated URL
/// patterns, `http://127.0.0.1:*`: the local stack it drives the app against)
/// and refuses the rest.
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
/// * No capture or playback device (microphone, camera, screen, audio output) is opened while
///   armed: each gets the synthetic stand-in the run asked for (`AZ_SYNTHETIC_DEVICES=camera,..`
///   at launch, or the `mock` op), else nothing, recorded like an unmocked request
///   ([`device`]).
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
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct MockHttpResponse {
        pub status: u16,
        pub body: Vec<u8>,
        pub content_type: AzString,
    }

    /// What a mocked HTTP request resolves to.
    #[derive(Debug, Clone, PartialEq, Eq)]
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

    /// A capture or playback device, which a headless / e2e run never opens
    /// for real: no camera light, no microphone, no permission prompt.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum DeviceKind {
        /// `MicrophoneWidget` capture. Stand-in: the widget's 440 Hz test tone.
        Microphone,
        /// `CameraWidget` capture. Stand-in: a colour-cycle pattern.
        Camera,
        /// `ScreenCaptureWidget` capture. Stand-in: a moving band.
        Screen,
        /// `AudioSink::open`. Stand-in: a sink that counts the frames it is
        /// given and plays nothing.
        AudioSink,
    }

    impl DeviceKind {
        /// Every kind, in index order.
        pub const ALL: [DeviceKind; 4] = [
            DeviceKind::Microphone,
            DeviceKind::Camera,
            DeviceKind::Screen,
            DeviceKind::AudioSink,
        ];

        const fn index(self) -> usize {
            self as usize
        }

        /// The name in `AZ_SYNTHETIC_DEVICES` and in the `mock` op.
        #[must_use]
        pub const fn name(self) -> &'static str {
            match self {
                DeviceKind::Microphone => "microphone",
                DeviceKind::Camera => "camera",
                DeviceKind::Screen => "screen",
                DeviceKind::AudioSink => "audio_sink",
            }
        }

        /// The kind called `name` (see [`DeviceKind::name`]).
        #[must_use]
        pub fn from_name(name: &str) -> Option<DeviceKind> {
            DeviceKind::ALL.into_iter().find(|kind| kind.name() == name)
        }

        /// What the `mock` op calls the synthetic stand-in.
        #[must_use]
        pub const fn stand_in(self) -> &'static str {
            match self {
                DeviceKind::Microphone => "tone",
                DeviceKind::Camera | DeviceKind::Screen => "pattern",
                DeviceKind::AudioSink => "count",
            }
        }

        /// The operation an unavailable device is recorded as (see
        /// [`unmocked_requests`]).
        #[must_use]
        pub const fn operation(self) -> &'static str {
            match self {
                DeviceKind::Microphone => "MicrophoneWidget capture",
                DeviceKind::Camera => "CameraWidget capture",
                DeviceKind::Screen => "ScreenCaptureWidget capture",
                DeviceKind::AudioSink => "AudioSink::open",
            }
        }

        /// What a headless run says when `self` is opened without a stand-in:
        /// that nothing is opened, and how to ask for the synthetic one. The
        /// stderr line of [`record_unavailable_device`], and the reason a
        /// closed handle (`AudioSink::error_message`) gives.
        #[must_use]
        pub fn unavailable_message(self) -> String {
            format!(
                "{}: not available in a headless run, no device is opened. A synthetic \
                 stand-in: AZ_SYNTHETIC_DEVICES={} or the mock op {{\"{}\": \"{}\"}}",
                self.operation(),
                self.name(),
                self.name(),
                self.stand_in()
            )
        }
    }

    /// What opening a device resolves to.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum MockDevice {
        /// Not an e2e or headless run: open the real device.
        Real,
        /// A headless run that asked for this device's synthetic stand-in
        /// (`AZ_SYNTHETIC_DEVICES`, or the `mock` op): use it, never the OS.
        Synthetic,
        /// A headless run that did not: nothing is opened ("not available in
        /// a headless run"), and the attempt is recorded.
        Unavailable,
    }

    #[derive(Default)]
    struct MockState {
        /// `None` = decide from the environment on first use.
        armed: Option<bool>,
        /// Whether a SCRIPT drives this run (`AZ_E2E` / `AZ_E2E_TEST`, or
        /// [`arm`]) rather than a bare headless launch; `None` = decide from
        /// the environment on first use.
        scripted: Option<bool>,
        file_open: VecDeque<Option<AzString>>,
        file_open_multi: VecDeque<Vec<AzString>>,
        color_pick: VecDeque<Option<ColorU>>,
        save_file: VecDeque<Option<AzString>>,
        save_bytes_accept: Option<bool>,
        file_reads: BTreeMap<String, Vec<u8>>,
        /// `(url pattern, answer)`: an exact URL, a prefix ending in `*`, or
        /// `*` for everything; first match wins.
        http: Vec<(String, MockHttp)>,
        /// The URL patterns (as in `http`) a scripted run fetches for real,
        /// unmocked: `AZ_E2E_ALLOW_HTTP`, the local stack a script drives the
        /// app against; `None` = read the variable on first use.
        http_allowed: Option<Vec<String>>,
        audio_devices: Option<(Vec<AzString>, Vec<AzString>)>,
        video_decode_none: bool,
        saved_files: Vec<SavedFile>,
        unmocked: Vec<String>,
        /// Which devices get their synthetic stand-in, by
        /// [`DeviceKind::index`]; `None` = read `AZ_SYNTHETIC_DEVICES` on
        /// first use.
        synthetic_devices: Option<[bool; 4]>,
        /// Which unavailable devices were already reported on stderr.
        devices_reported: [bool; 4],
    }

    static STATE: Mutex<MockState> = Mutex::new(MockState {
        armed: None,
        scripted: None,
        file_open: VecDeque::new(),
        file_open_multi: VecDeque::new(),
        color_pick: VecDeque::new(),
        save_file: VecDeque::new(),
        save_bytes_accept: None,
        file_reads: BTreeMap::new(),
        http: Vec::new(),
        http_allowed: None,
        audio_devices: None,
        video_decode_none: false,
        saved_files: Vec::new(),
        unmocked: Vec::new(),
        synthetic_devices: None,
        devices_reported: [false; 4],
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

    /// A script drives the run (`AZ_E2E` / `AZ_E2E_TEST`), as opposed to a
    /// bare headless launch.
    fn env_scripted() -> bool {
        let set = |k: &str| std::env::var(k).map(|v| !v.is_empty()).unwrap_or(false);
        set("AZ_E2E") || set("AZ_E2E_TEST")
    }

    fn scripted_in(s: &mut MockState) -> bool {
        *s.scripted.get_or_insert_with(env_scripted)
    }

    /// The devices `AZ_SYNTHETIC_DEVICES` gives a synthetic stand-in.
    fn env_synthetic_devices() -> [bool; 4] {
        std::env::var("AZ_SYNTHETIC_DEVICES")
            .map(|list| parse_synthetic_devices(&list))
            .unwrap_or([false; 4])
    }

    /// `AZ_SYNTHETIC_DEVICES`: comma-separated [`DeviceKind::name`]s, or
    /// `all`. An unknown name is reported and ignored.
    fn parse_synthetic_devices(list: &str) -> [bool; 4] {
        let mut synthetic = [false; 4];
        for name in list.split(',').map(str::trim).filter(|n| !n.is_empty()) {
            if name == "all" {
                synthetic = [true; 4];
            } else if let Some(kind) = DeviceKind::from_name(name) {
                synthetic[kind.index()] = true;
            } else {
                eprintln!(
                    "[azul][e2e] AZ_SYNTHETIC_DEVICES: unknown device {name:?} (known: \
                     microphone, camera, screen, audio_sink, all)"
                );
            }
        }
        synthetic
    }

    fn synthetic_in(s: &mut MockState) -> &mut [bool; 4] {
        s.synthetic_devices.get_or_insert_with(env_synthetic_devices)
    }

    /// Arm the store explicitly (an e2e host that is not driven by the
    /// environment variables): a scripted run.
    pub fn arm() {
        with(|s| {
            s.armed = Some(true);
            s.scripted = Some(true);
        });
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
            let (armed, scripted) = (s.armed, s.scripted);
            *s = MockState::default();
            s.armed = armed;
            s.scripted = scripted;
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

    /// Give `kind` its synthetic stand-in in this run (`true`), or leave it
    /// unavailable (`false`, the default unless `AZ_SYNTHETIC_DEVICES` names
    /// it). Only an armed store consults this: a normal run always opens the
    /// real device.
    pub fn set_synthetic_device(kind: DeviceKind, synthetic: bool) {
        with(|s| synthetic_in(s)[kind.index()] = synthetic);
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
        with(|s| take_http_in(s, url))
    }

    fn take_http_in(s: &mut MockState, url: &str) -> Answer<MockHttp> {
        if !armed_in(s) {
            return Answer::NotArmed;
        }
        let found = s.http.iter().find(|(pattern, _)| url_matches(pattern, url));
        let taken = found.map(|(_, canned)| canned.clone());
        // No modal can hang on an HTTP request: without a script to keep
        // deterministic, an unmocked request goes out (see the tests); a
        // scripted run sends the ones `AZ_E2E_ALLOW_HTTP` names (a local
        // stack).
        if taken.is_none() && (!scripted_in(s) || http_allowed_in(s, url)) {
            return Answer::NotArmed;
        }
        answer(s, taken, &alloc::format!("http {url}"))
    }

    /// Whether `url` is what `pattern` names: an exact URL, a prefix ending
    /// in `*`, or `*` for everything.
    fn url_matches(pattern: &str, url: &str) -> bool {
        pattern == "*"
            || pattern == url
            || pattern
                .strip_suffix('*')
                .is_some_and(|prefix| url.starts_with(prefix))
    }

    /// `AZ_E2E_ALLOW_HTTP`'s patterns: comma-separated, blanks dropped
    /// (`http://127.0.0.1:*,http://localhost:*`).
    fn parse_http_allowed(value: &str) -> Vec<String> {
        value
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
            .map(String::from)
            .collect()
    }

    fn http_allowed_in(s: &mut MockState, url: &str) -> bool {
        s.http_allowed
            .get_or_insert_with(|| {
                std::env::var("AZ_E2E_ALLOW_HTTP")
                    .map(|v| parse_http_allowed(&v))
                    .unwrap_or_default()
            })
            .iter()
            .any(|pattern| url_matches(pattern, url))
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

    /// What opening `kind` resolves to: the real device in a normal run;
    /// under an armed store (a headless or e2e run) never that - the
    /// synthetic stand-in the run asked for, else nothing. Asking records
    /// nothing; the caller reports an attempt to open an unavailable device
    /// with [`record_unavailable_device`].
    #[must_use]
    pub fn device(kind: DeviceKind) -> MockDevice {
        with(|s| device_in(s, kind))
    }

    fn device_in(s: &mut MockState, kind: DeviceKind) -> MockDevice {
        if !armed_in(s) {
            MockDevice::Real
        } else if synthetic_in(s)[kind.index()] {
            MockDevice::Synthetic
        } else {
            MockDevice::Unavailable
        }
    }

    /// A headless run tried to open `kind`, which has no stand-in: say so on
    /// stderr (once per kind) and record it like an unmocked request, so a
    /// scenario can assert on it (`assert_unmocked_request`).
    pub fn record_unavailable_device(kind: DeviceKind) {
        with(|s| record_unavailable_in(s, kind));
    }

    fn record_unavailable_in(s: &mut MockState, kind: DeviceKind) {
        let reported = &mut s.devices_reported[kind.index()];
        if !*reported {
            *reported = true;
            eprintln!("[azul][e2e] {}", kind.unavailable_message());
        }
        s.unmocked.push(String::from(kind.operation()));
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

    #[cfg(test)]
    mod network_tests {
        use super::{parse_http_allowed, take_http_in, Answer, MockHttp, MockState};

        fn store(scripted: bool) -> MockState {
            MockState {
                armed: Some(true),
                scripted: Some(scripted),
                ..MockState::default()
            }
        }

        /// A bare headless launch (`AZ_BACKEND=headless`, no script) arms the
        /// store so a picker or a permission prompt can never block it. An
        /// HTTP request is neither: it has its own timeout, and a headless
        /// app (CI screenshots, two AzMeet clients meeting through a local
        /// server) needs the network. Unmocked, it goes out.
        #[test]
        fn a_headless_run_without_a_script_sends_its_http_requests() {
            let mut s = store(false);
            assert_eq!(take_http_in(&mut s, "http://127.0.0.1:8787/rooms"), Answer::NotArmed);
            assert!(s.unmocked.is_empty(), "nothing to report: the request went out");
        }

        /// A scripted run against a local stack (the meeting server, S3, the
        /// token server on 127.0.0.1): `AZ_E2E_ALLOW_HTTP` names what goes
        /// out for real; every other unmocked request is still refused.
        #[test]
        fn a_scripted_run_sends_the_http_requests_its_allow_list_names() {
            let mut s = store(true);
            s.http_allowed = Some(vec!["http://127.0.0.1:*".into()]);
            assert_eq!(
                take_http_in(&mut s, "http://127.0.0.1:8790/rooms"),
                Answer::NotArmed
            );
            assert_eq!(
                take_http_in(&mut s, "https://example.org/feed.xml"),
                Answer::Unmocked
            );
            assert_eq!(s.unmocked, ["http https://example.org/feed.xml"]);
            assert_eq!(
                parse_http_allowed(" http://127.0.0.1:*, ,http://localhost:8790/*,"),
                ["http://127.0.0.1:*", "http://localhost:8790/*"]
            );
        }

        /// A scripted run keeps its determinism: an unmocked request is
        /// refused and recorded, and a canned answer is used in both runs.
        #[test]
        fn a_scripted_run_refuses_an_unmocked_http_request_and_both_take_canned_answers() {
            let mut s = store(true);
            assert_eq!(take_http_in(&mut s, "http://x/"), Answer::Unmocked);
            assert_eq!(s.unmocked, ["http http://x/"]);
            for scripted in [true, false] {
                let mut s = store(scripted);
                s.http.push(("http://x/*".into(), MockHttp::Error("canned".into())));
                assert_eq!(
                    take_http_in(&mut s, "http://x/rooms"),
                    Answer::Mocked(MockHttp::Error("canned".into()))
                );
            }
        }
    }

    #[cfg(test)]
    mod device_tests {
        use super::{
            device_in, parse_synthetic_devices, record_unavailable_in, DeviceKind, MockDevice,
            MockState,
        };

        fn store(armed: bool, synthetic: [bool; 4]) -> MockState {
            MockState {
                armed: Some(armed),
                scripted: Some(false),
                synthetic_devices: Some(synthetic),
                ..MockState::default()
            }
        }

        /// Outside a headless / e2e run every device is the real one, even
        /// when a stand-in was named: the store is not consulted.
        #[test]
        fn a_run_that_is_not_headless_opens_the_real_devices() {
            let mut s = store(false, [true; 4]);
            for kind in DeviceKind::ALL {
                assert_eq!(device_in(&mut s, kind), MockDevice::Real, "{kind:?}");
            }
        }

        /// A headless run opens no microphone, camera, screen or audio
        /// output: each is unavailable unless the run asked for its
        /// synthetic stand-in, and then it gets exactly that one.
        #[test]
        fn a_headless_run_opens_no_device_unless_it_asked_for_a_synthetic_one() {
            let mut s = store(true, [false; 4]);
            for kind in DeviceKind::ALL {
                assert_eq!(device_in(&mut s, kind), MockDevice::Unavailable, "{kind:?}");
            }
            let mut s = store(true, [false, true, false, true]);
            assert_eq!(
                device_in(&mut s, DeviceKind::Microphone),
                MockDevice::Unavailable
            );
            assert_eq!(device_in(&mut s, DeviceKind::Camera), MockDevice::Synthetic);
            assert_eq!(device_in(&mut s, DeviceKind::Screen), MockDevice::Unavailable);
            assert_eq!(
                device_in(&mut s, DeviceKind::AudioSink),
                MockDevice::Synthetic
            );
        }

        /// Opening an unavailable device is a clear status, not a silent
        /// blank: every attempt is recorded (a scenario can assert on it),
        /// and stderr says so once per device.
        #[test]
        fn an_unavailable_device_is_recorded_every_time_it_is_opened() {
            let mut s = store(true, [false; 4]);
            record_unavailable_in(&mut s, DeviceKind::Camera);
            record_unavailable_in(&mut s, DeviceKind::Camera);
            record_unavailable_in(&mut s, DeviceKind::AudioSink);
            assert_eq!(
                s.unmocked,
                [
                    "CameraWidget capture",
                    "CameraWidget capture",
                    "AudioSink::open"
                ]
            );
            assert_eq!(s.devices_reported, [false, true, false, true]);
        }

        /// `AZ_SYNTHETIC_DEVICES` names the devices a headless run gets a
        /// stand-in for: a comma-separated list of names, or `all`; an
        /// unknown name adds nothing.
        #[test]
        fn az_synthetic_devices_names_the_stand_ins_of_a_headless_run() {
            assert_eq!(
                parse_synthetic_devices("microphone, audio_sink"),
                [true, false, false, true]
            );
            assert_eq!(parse_synthetic_devices("camera,screen"), [false, true, true, false]);
            assert_eq!(parse_synthetic_devices("all"), [true; 4]);
            assert_eq!(parse_synthetic_devices(""), [false; 4]);
            assert_eq!(parse_synthetic_devices("speaker"), [false; 4]);
            for kind in DeviceKind::ALL {
                assert_eq!(DeviceKind::from_name(kind.name()), Some(kind));
            }
        }
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
        // The queue is per thread under tests; drain anything an earlier test
        // on this harness thread left.
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

    /// The test harness runs tests side by side on its threads, and every
    /// drainer takes EVERYTHING (`take_completed`; the e2e runner's pump runs
    /// inside many runner tests): with one process-wide queue a test's
    /// completion could be swallowed by the test running beside it, and the
    /// http tests' worker-thread answers made that window as long as a
    /// transfer. Test B drains while test A's completion waits; A must still
    /// find it.
    #[test]
    fn a_tests_completion_is_not_drained_by_a_test_running_beside_it() {
        use std::sync::mpsc::channel;

        let (completed_tx, completed_rx) = channel::<()>();
        let (drained_tx, drained_rx) = channel::<()>();

        let test_a = std::thread::spawn(move || {
            let id = complete(RefAny::new(()), ResumeCallback::create(noop), 7u32);
            completed_tx.send(()).expect("test B listens");
            drained_rx.recv().expect("test B drained");
            take_completed().iter().any(|c| c.request_id == id)
        });
        let test_b = std::thread::spawn(move || {
            completed_rx.recv().expect("test A completed");
            let _ = take_completed();
            drained_tx.send(()).expect("test A listens");
        });
        test_b.join().expect("test B ran");
        assert!(
            test_a.join().expect("test A ran"),
            "test A's completion is still in the queue for test A after test B drained"
        );
    }
}
