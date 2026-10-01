//! Video encode/decode (SUPER_PLAN_2 P7/P8) - `VideoEncoder` / `VideoDecoder`.
//!
//! For azul-meet's video leg: compress captured `VideoFrame`s before
//! `Udp::send_chunked`, and decompress received bitstreams back into frames for
//! a display widget. Like `AudioSink` / `Db`, these are C-ABI handles the app
//! holds in its own State (no globals).
//!
//! **Native-per-platform backend** (per the user directive + the vk-video
//! research): the codec used is whatever is native to the platform -
//!   - desktop Linux / Windows: **gpu-video** (Vulkan Video, H.264/H.265),
//!   - Apple (macOS / iOS): **VideoToolbox** (Vulkan Video can't build on Apple - no MoltenVK
//!     video),
//!   - Android: **MediaCodec**,
//!   - anything else: none (encode/decode no-op).
//! [`VideoEncoder::backend_name`] reports the selection. The handles are
//! honest: `open` returns an invalid handle (`is_open()` false) wherever this
//! build has no working engine (today: encode only via VideoToolbox, decode via
//! VideoToolbox or Vulkan Video on x86_64 + `video-native`; no H.265 anywhere),
//! so an open handle always produces output. `VideoEncodeCheck` and
//! `PlatformCapability::video_codec` answer from the same engine checks.

use core::ffi::c_void;

use azul_core::video::{OptionVideoFrame, VideoFrame};
use azul_css::{impl_option_inner, AzString, U8Vec};

// MP4 -> H.264 Annex-B demux (the elementary stream gpu-video needs). Behind
// `video-native`; pure Rust + unit-tested, no GPU required.
#[cfg(feature = "video-native")]
pub mod demux;

// MP4 container I/O for apps (an editor's seek-and-decode, an export's mux):
// `Mp4Demuxer` / `Mp4Muxer`. The handles are always present; the `mp4` crate
// behind them is `video-native`'s.
pub mod container;
// Surfaced at the module level so `unified::video_codec`'s glob re-export
// exposes `azul_dll::unified::video_codec::{Mp4Demuxer, Mp4Muxer}`.
pub use container::{Mp4Demuxer, Mp4Muxer};

// Streaming decode worker for the VideoWidget: runs the VK decode on a background
// framework Thread (off-main), exactly like the map's tile_fetch_worker. The
// hardware decode inside is video-native-gated; the worker fn is always present.
pub mod stream;
// `video_widget_dom` is the FFI `VideoWidget::dom()` entry point (wires the
// streaming worker), surfaced at the module level so `unified::video_codec`'s
// glob re-export exposes `azul_dll::unified::video_codec::video_widget_dom`.
pub use stream::{ensure_video_decoder, video_widget_dom};

// File -> frames pipeline (demux + feed through VideoDecoder). Behind
// `video-native`; the decode step is the only hardware-gated part.
#[cfg(feature = "video-native")]
pub mod pipeline;

// Real Vulkan Video H.264 decoder (Linux + Windows). Behind `video-native`; the
// gpu-video wiring + NV12->RGBA CPU conversion live here. Other platforms keep
// the stub (Apple: VideoToolbox / Android: MediaCodec land later).
#[cfg(az_gpu_video)]
mod decode_vulkan;

// Real VideoToolbox H.264 encoder + decoder (macOS/iOS). Every framework
// symbol is dlopen'd at runtime (no build-time link — loads on any macOS
// version), so this needs only `libloading` and is NOT behind `video-native`:
// azul-meet's `link-static` build gets the real codec too.
#[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "libloading"))]
mod videotoolbox;

// Hardware-decode capability probe + driver-provisioning planner (always built;
// no extra crate deps). Drives `capability::video_codec()` and the "install the
// drivers for me?" flow.
pub mod provision;

// The one-call startup readiness check + its outcome — the FFI/DLL surface an
// app uses at launch to verify the box is ready for hardware video decode.
pub use provision::{VideoProvisionOutcome, VideoStartupCheck};

/// The native codec backend this build selects, by target OS.
fn backend() -> &'static str {
    if cfg!(any(target_os = "ios", target_os = "macos")) {
        "VideoToolbox"
    } else if cfg!(target_os = "android") {
        "MediaCodec"
    } else if cfg!(any(target_os = "linux", target_os = "windows")) {
        "gpu-video"
    } else {
        "none"
    }
}

/// Whether this build contains a real DECODE engine that `DecoderInner` can
/// hold (Vulkan Video on x86_64 Linux/Windows behind `video-native`, or the
/// dlopen'd VideoToolbox on Apple behind `libloading`). Without one,
/// `VideoDecoder::open` hands out an invalid handle.
const fn decode_engine_compiled() -> bool {
    cfg!(az_gpu_video)
        || cfg!(all(
            any(target_os = "macos", target_os = "ios"),
            feature = "libloading"
        ))
}

/// The H.264 ENCODE engine of this BUILD on this machine: `Ok(backend)` when
/// one is compiled in and loads here, `Err(why not)` otherwise. Only
/// VideoToolbox (Apple + `libloading`) exists: gpu-video encode and MediaCodec
/// are not wired. `VideoEncoder::open` and `VideoEncodeCheck` both answer
/// from here, so neither claims an encoder that cannot give a packet back.
pub(crate) fn encode_engine() -> Result<&'static str, String> {
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "libloading"))]
    {
        if videotoolbox::is_available() {
            Ok("VideoToolbox")
        } else {
            Err(String::from(
                "the VideoToolbox framework did not load (see the [video] log lines)",
            ))
        }
    }
    #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "libloading")))]
    {
        let why = if cfg!(any(target_os = "macos", target_os = "ios")) {
            "this build has no `libloading` feature, so the VideoToolbox backend is compiled out"
        } else if cfg!(target_os = "android") {
            "the MediaCodec backend is not implemented yet"
        } else if cfg!(any(target_os = "linux", target_os = "windows")) {
            "gpu-video ENCODE is not wired yet on Linux/Windows (decode only)"
        } else {
            "there is no native video backend on this OS"
        };
        Err(String::from(why))
    }
}

/// The H.264 DECODE engine of this BUILD on this machine (see
/// [`encode_engine`]). Whether a Vulkan Video device decodes is only known
/// when a decoder opens, which `VideoDecoder::open` checks.
pub(crate) fn decode_engine() -> Result<&'static str, String> {
    if !decode_engine_compiled() {
        return Err(decode_engine_missing_reason());
    }
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "libloading"))]
    {
        if videotoolbox::is_available() {
            Ok("VideoToolbox")
        } else {
            Err(String::from(
                "the VideoToolbox framework did not load (see the [video] log lines)",
            ))
        }
    }
    #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "libloading")))]
    {
        Ok("Vulkan Video")
    }
}

/// Says why a codec handle did not open: once per distinct reason, so a
/// caller that retries does not flood stderr.
fn say_not_open(what: &str, why: &str) {
    static SAID: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let line = format!("{what}: {why}");
    let mut said = SAID.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if !said.contains(&line) {
        eprintln!("[azul][video] {line} — the handle is invalid (is_open() = false)");
        said.push(line);
    }
}

/// One line naming why a DECODE handle can never produce output on this build
/// (feature compiled out / wrong target / backend not implemented). A handle
/// that opens fine and then yields nothing forever is indistinguishable from
/// "no data yet" — same say-so-when-inert contract as
/// `shell2::run::warn_about_inert_env_knobs`.
fn decode_engine_missing_reason() -> String {
    if cfg!(target_os = "android") {
        "the MediaCodec backend is not implemented yet".to_string()
    } else if cfg!(any(target_os = "macos", target_os = "ios")) {
        "this build has no `libloading` feature, so the VideoToolbox backend is compiled out"
            .to_string()
    } else if !cfg!(feature = "video-native") {
        "this build has no `video-native` feature. Rebuild with: cargo build -p azul-dll \
         --features build-dll,video-native"
            .to_string()
    } else {
        format!(
            "H.264 decode requires x86_64 linux/windows (this target: {}-{})",
            std::env::consts::ARCH,
            std::env::consts::OS
        )
    }
}

// ---------------------------------------------------------------------------
// The codec thread: engines run off the thread that holds the handle
// ---------------------------------------------------------------------------

/// How many frames may wait for an encoder. A frame that finds the queue
/// full is not taken (`VideoEncoder::encode` returns false): a realtime
/// encoder that falls behind drops frames instead of queueing latency.
#[cfg_attr(
    not(all(any(target_os = "macos", target_os = "ios"), feature = "libloading")),
    allow(dead_code)
)]
const ENCODE_QUEUE_FRAMES: usize = 3;

/// How long `flush` waits for a codec thread to work through its queue.
const FLUSH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

/// Locks `mutex`, through a poisoned lock (a panicked codec thread leaves
/// its queue readable).
fn lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Where a codec thread's jobs wait: a bounded queue (frames to encode) or
/// an unbounded one (chunks to decode: a decoder needs every one).
enum JobQueue<J> {
    Bounded(std::sync::mpsc::SyncSender<J>),
    Unbounded(std::sync::mpsc::Sender<J>),
}

/// A codec engine's own thread. The engine is made ON the thread and never
/// leaves it (no engine has to be `Send`); jobs run on it in the order they
/// were queued; the handle only queues jobs and takes results out of shared
/// queues. So the thread that holds a `VideoEncoder` / `VideoDecoder` - the
/// UI thread, in a call - never encodes, decodes or copies a picture. Idle,
/// the thread sleeps in `recv` (no CPU); dropped, the handle stops it and
/// joins it.
struct CodecThread<J: Send + 'static> {
    jobs: Option<JobQueue<J>>,
    /// Set when the handle goes: jobs still queued are skipped.
    stop: std::sync::Arc<core::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl<J: Send + 'static> CodecThread<J> {
    /// Starts the thread `name`: `open` makes the engine on it, and its
    /// answer comes back before this returns (so a handle is open exactly
    /// when its engine is); then `run` takes every job. `bound`: the queue's
    /// length, `None` for no bound.
    fn spawn<E, O, R>(
        name: &str,
        bound: Option<usize>,
        open: O,
        mut run: R,
    ) -> Result<Self, String>
    where
        E: 'static,
        O: FnOnce() -> Result<E, String> + Send + 'static,
        R: FnMut(&mut E, J) + Send + 'static,
    {
        let (jobs, rx) = match bound {
            Some(n) => {
                let (tx, rx) = std::sync::mpsc::sync_channel(n.max(1));
                (JobQueue::Bounded(tx), rx)
            }
            None => {
                let (tx, rx) = std::sync::mpsc::channel();
                (JobQueue::Unbounded(tx), rx)
            }
        };
        let stop = std::sync::Arc::new(core::sync::atomic::AtomicBool::new(false));
        let stopped = std::sync::Arc::clone(&stop);
        let (opened_tx, opened_rx) = std::sync::mpsc::channel::<Result<(), String>>();
        let thread = std::thread::Builder::new()
            .name(name.to_string())
            .spawn(move || {
                let mut engine = match open() {
                    Ok(engine) => engine,
                    Err(why) => {
                        let _ = opened_tx.send(Err(why));
                        return;
                    }
                };
                let _ = opened_tx.send(Ok(()));
                while let Ok(job) = rx.recv() {
                    if stopped.load(core::sync::atomic::Ordering::Acquire) {
                        break;
                    }
                    run(&mut engine, job);
                }
            })
            .map_err(|e| format!("the codec thread did not start: {e}"))?;
        let answer = opened_rx
            .recv()
            .unwrap_or_else(|_| Err(String::from("the codec thread ended while opening")));
        match answer {
            Ok(()) => Ok(CodecThread {
                jobs: Some(jobs),
                stop,
                thread: Some(thread),
            }),
            Err(why) => {
                let _ = thread.join();
                Err(why)
            }
        }
    }

    /// Queues `job` without waiting: false when a bounded queue is full or
    /// the thread is gone.
    fn offer(&self, job: J) -> bool {
        match self.jobs.as_ref() {
            Some(JobQueue::Bounded(tx)) => tx.try_send(job).is_ok(),
            Some(JobQueue::Unbounded(tx)) => tx.send(job).is_ok(),
            None => false,
        }
    }

    /// Queues `job`, waiting for room in a bounded queue.
    fn queue(&self, job: J) -> bool {
        match self.jobs.as_ref() {
            Some(JobQueue::Bounded(tx)) => tx.send(job).is_ok(),
            Some(JobQueue::Unbounded(tx)) => tx.send(job).is_ok(),
            None => false,
        }
    }

    /// Waits until every job queued before this call has run: `barrier`
    /// makes the job that answers on the sender it is given. False on a
    /// timeout ([`FLUSH_TIMEOUT`]) or a gone thread.
    fn wait_idle(&self, barrier: impl FnOnce(std::sync::mpsc::Sender<()>) -> J) -> bool {
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        self.queue(barrier(done_tx)) && done_rx.recv_timeout(FLUSH_TIMEOUT).is_ok()
    }

    /// The thread the engine runs on.
    #[cfg(test)]
    fn thread_id(&self) -> Option<std::thread::ThreadId> {
        self.thread.as_ref().map(|t| t.thread().id())
    }
}

impl<J: Send + 'static> Drop for CodecThread<J> {
    fn drop(&mut self) {
        // Skip what is still queued, close the queue (the thread's `recv`
        // ends) and wait for the job in hand.
        self.stop.store(true, core::sync::atomic::Ordering::Release);
        self.jobs = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// One job for an encoder's codec thread.
#[cfg_attr(
    not(all(any(target_os = "macos", target_os = "ios"), feature = "libloading")),
    allow(dead_code)
)]
enum EncodeJob {
    /// Encode this frame (forcing a keyframe when asked), stamped with its
    /// own time in microseconds (`encode_at`) or the wall clock (`None`).
    Frame(VideoFrame, bool, Option<i64>),
    /// Answer once every frame queued before is encoded.
    Flush(std::sync::mpsc::Sender<()>),
}

/// Engine-side encoder state: exists only with a live engine (see
/// [`EncoderInner::open_h264`]), so an open `VideoEncoder` always encodes.
/// Never built where this build has no encode engine.
#[cfg_attr(
    not(all(any(target_os = "macos", target_os = "ios"), feature = "libloading")),
    allow(dead_code)
)]
struct EncoderInner {
    frames_encoded: u64,
    /// Encoded chunks the codec thread produced, not yet pulled with
    /// `recv_packet`.
    packets: std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<U8Vec>>>,
    /// The engine's own thread (VideoToolbox's H.264 session lives on it).
    thread: CodecThread<EncodeJob>,
}

impl EncoderInner {
    /// A live H.264 encoder for `width` x `height` on a codec thread of its
    /// own, or why none opens. Only called once [`encode_engine`] said this
    /// build has one.
    fn open_h264(width: u32, height: u32, bitrate_kbps: u32) -> Result<EncoderInner, String> {
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "libloading"))]
        {
            let packets = std::sync::Arc::new(std::sync::Mutex::new(
                std::collections::VecDeque::new(),
            ));
            let out = std::sync::Arc::clone(&packets);
            let thread = CodecThread::spawn(
                "azul-video-encode",
                Some(ENCODE_QUEUE_FRAMES),
                move || {
                    videotoolbox::VtEncoder::open(width, height, bitrate_kbps).ok_or_else(|| {
                        format!(
                            "VideoToolbox could not create a {width}x{height} H.264 session (see \
                             the [video] log lines)"
                        )
                    })
                },
                move |vt: &mut videotoolbox::VtEncoder, job: EncodeJob| match job {
                    EncodeJob::Frame(frame, force_keyframe, micros) => {
                        // NV12 goes into a pooled buffer as it is, BGRA as it
                        // is, RGBA swizzled (see `VtEncoder::encode`), which
                        // hands this frame's packets back at once.
                        let chunk = match micros {
                            Some(micros) => vt.encode_at(&frame, force_keyframe, micros),
                            None => vt.encode(&frame, force_keyframe),
                        };
                        if !chunk.is_empty() {
                            lock(&out).push_back(U8Vec::from_vec(chunk));
                        }
                    }
                    EncodeJob::Flush(done) => {
                        let _ = done.send(());
                    }
                },
            )?;
            Ok(EncoderInner {
                frames_encoded: 0,
                packets,
                thread,
            })
        }
        #[cfg(not(all(any(target_os = "macos", target_os = "ios"), feature = "libloading")))]
        {
            let _ = (width, height, bitrate_kbps);
            Err(String::from("this build has no H.264 encode engine"))
        }
    }
}

/// The decode engine of this build: Vulkan Video (x86_64 Linux / Windows
/// with `video-native`) or VideoToolbox (Apple), whichever opened. Made and
/// used on one thread: a decoder's codec thread, or the caller's for
/// [`VideoDecoder::open_on_this_thread`].
struct DecodeEngine {
    #[cfg(az_gpu_video)]
    vulkan: Option<decode_vulkan::VulkanVideoDecoder>,
    #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "libloading"))]
    vt: Option<videotoolbox::VtDecoder>,
}

impl DecodeEngine {
    /// A live H.264 decode engine, or why none opens here. Only called once
    /// [`decode_engine`] said this build has one.
    fn open_h264() -> Result<DecodeEngine, String> {
        let engine = DecodeEngine {
            #[cfg(az_gpu_video)]
            vulkan: decode_vulkan::VulkanVideoDecoder::open_h264(),
            #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "libloading"))]
            vt: videotoolbox::VtDecoder::open_h264(),
        };
        if engine.is_live() {
            Ok(engine)
        } else {
            Err(String::from(
                "the H.264 decoder did not open on this machine (see the [video] log lines)",
            ))
        }
    }

    /// Whether a real decode engine is behind this state.
    fn is_live(&self) -> bool {
        #[cfg(az_gpu_video)]
        if self.vulkan.is_some() {
            return true;
        }
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "libloading"))]
        if self.vt.is_some() {
            return true;
        }
        false
    }

    /// Feeds one Annex-B chunk; the pictures that came out go into `out`.
    fn decode(&mut self, data: &[u8], out: &mut std::collections::VecDeque<VideoFrame>) {
        #[cfg(az_gpu_video)]
        if let Some(vulkan) = self.vulkan.as_mut() {
            out.extend(vulkan.decode(data));
        }
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "libloading"))]
        if let Some(vt) = self.vt.as_mut() {
            out.extend(vt.decode(data));
        }
        let _ = (data, out);
    }

    /// End of stream: the pictures held back for reordering go into `out`.
    fn flush(&mut self, out: &mut std::collections::VecDeque<VideoFrame>) {
        #[cfg(az_gpu_video)]
        if let Some(vulkan) = self.vulkan.as_mut() {
            out.extend(vulkan.flush());
        }
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "libloading"))]
        if let Some(vt) = self.vt.as_mut() {
            out.extend(vt.flush());
        }
        let _ = out;
    }

    fn set_output_format(&mut self, format: azul_core::resources::RawImageFormat) {
        #[cfg(az_gpu_video)]
        if let Some(vulkan) = self.vulkan.as_mut() {
            vulkan.set_output_format(format);
        }
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "libloading"))]
        if let Some(vt) = self.vt.as_mut() {
            vt.set_output_format(format);
        }
        let _ = format;
    }

    fn set_output_size(&mut self, width: u32, height: u32) {
        #[cfg(all(any(target_os = "macos", target_os = "ios"), feature = "libloading"))]
        if let Some(vt) = self.vt.as_mut() {
            vt.set_output_size(width, height);
        }
        let _ = (width, height);
    }

    /// Runs one job of a decoder's codec thread; pictures go into `ready`.
    fn run(
        &mut self,
        job: DecodeJob,
        ready: &std::sync::Mutex<std::collections::VecDeque<VideoFrame>>,
    ) {
        match job {
            DecodeJob::Chunk(data) => {
                let mut out = std::collections::VecDeque::new();
                self.decode(data.as_ref(), &mut out);
                lock(ready).extend(out);
            }
            DecodeJob::Format(format) => self.set_output_format(format),
            DecodeJob::Size(width, height) => self.set_output_size(width, height),
            DecodeJob::Flush(done) => {
                let mut out = std::collections::VecDeque::new();
                self.flush(&mut out);
                lock(ready).extend(out);
                let _ = done.send(());
            }
        }
    }
}

/// One job for a decoder's codec thread, in the order the handle was told.
enum DecodeJob {
    Chunk(U8Vec),
    Format(azul_core::resources::RawImageFormat),
    Size(u32, u32),
    /// End of stream: flush the engine, then answer.
    Flush(std::sync::mpsc::Sender<()>),
}

/// Where a decoder's engine runs.
enum DecodeRunner {
    /// On a codec thread of its own ([`VideoDecoder::open`]).
    Thread(CodecThread<DecodeJob>),
    /// On the thread that holds the handle
    /// ([`VideoDecoder::open_on_this_thread`]: a worker that is off the UI
    /// thread already, like the `<video>` widget's).
    Here(DecodeEngine),
}

struct DecoderInner {
    /// Chunks handed to `decode` so far.
    frames_decoded: u64,
    /// Decoded frames not yet pulled with `recv_frame` / `next_frame`,
    /// whichever backend produced them.
    ready: std::sync::Arc<std::sync::Mutex<std::collections::VecDeque<VideoFrame>>>,
    runner: DecodeRunner,
}

impl DecoderInner {
    /// A live H.264 decoder: on a codec thread of its own (`threaded`), else
    /// on the caller's thread; or why none opens here.
    fn open_h264(threaded: bool) -> Result<DecoderInner, String> {
        let ready = std::sync::Arc::new(std::sync::Mutex::new(std::collections::VecDeque::new()));
        let runner = if threaded {
            let into = std::sync::Arc::clone(&ready);
            DecodeRunner::Thread(CodecThread::spawn(
                "azul-video-decode",
                None,
                DecodeEngine::open_h264,
                move |engine: &mut DecodeEngine, job: DecodeJob| engine.run(job, &into),
            )?)
        } else {
            DecodeRunner::Here(DecodeEngine::open_h264()?)
        };
        Ok(DecoderInner {
            frames_decoded: 0,
            ready,
            runner,
        })
    }

    /// Hands `job` to the engine: queued on its thread, or run right here.
    fn submit(&mut self, job: DecodeJob) -> bool {
        match &mut self.runner {
            DecodeRunner::Thread(thread) => thread.offer(job),
            DecodeRunner::Here(engine) => {
                engine.run(job, &self.ready);
                true
            }
        }
    }
}

/// A hardware video encoder handle. `open(...)` selects the native backend for
/// the platform; `encode` turns a `VideoFrame` (RGBA) into an encoded chunk.
#[repr(C)]
pub struct VideoEncoder {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

impl Clone for VideoEncoder {
    fn clone(&self) -> Self {
        VideoEncoder {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}
impl Default for VideoEncoder {
    fn default() -> Self {
        VideoEncoder {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

impl VideoEncoder {
    /// Open an encoder for `width` x `height`, H.265 if `h265` else H.264, at
    /// `bitrate_kbps`. Uses the platform-native backend ([`backend_name`]).
    /// Returns an invalid handle (`is_open()` false) wherever this build
    /// cannot encode: no engine compiled in (Linux, Windows, Android today),
    /// the engine does not load or refuses the size, or H.265 (no backend
    /// implements it yet). An open handle gives packets back.
    pub fn open(width: u32, height: u32, h265: bool, bitrate_kbps: u32) -> VideoEncoder {
        let engine = if h265 {
            Err(String::from("H.265 encode is not wired yet (H.264 only)"))
        } else {
            encode_engine()
        };
        let inner = match engine.and_then(|_| EncoderInner::open_h264(width, height, bitrate_kbps))
        {
            Ok(inner) => Box::new(inner),
            Err(why) => {
                say_not_open("VideoEncoder::open", &why);
                return VideoEncoder::default();
            }
        };
        VideoEncoder {
            ptr: Box::into_raw(inner) as *mut c_void,
            run_destructor: true,
        }
    }

    /// The native codec backend selected for this platform ("VideoToolbox",
    /// "MediaCodec", "gpu-video", or "none").
    pub fn backend_name() -> AzString {
        AzString::from_const_str(backend())
    }

    /// Whether the encoder opened (a backend exists for this platform).
    pub fn is_open(&self) -> bool {
        !self.ptr.is_null()
    }

    /// Submit one `VideoFrame` (NV12, BGRA8 or RGBA8) for encoding.
    /// `force_keyframe` requests an IDR. The frame is handed to the
    /// encoder's own thread and this returns at once: `true` if the frame
    /// was taken, `false` when the encoder is not open or still has three
    /// frames to work through (a realtime encoder that falls behind drops
    /// frames rather than queue latency). The
    /// encoded chunks (Annex-B for H.264/H.265) come out of
    /// [`recv_packet`](Self::recv_packet), in frame order, once encoded -
    /// later than this call; [`flush`](Self::flush) waits for them.
    /// Hardware and browser (WebCodecs) encoders are output-callback shaped,
    /// so submit and poll are separate steps on every target.
    pub fn encode(&self, frame: VideoFrame, force_keyframe: bool) -> bool {
        self.submit(frame, force_keyframe, None)
    }

    /// Submit one `VideoFrame` stamped with its own presentation time,
    /// `timestamp_us` microseconds from the start of the stream - what an
    /// EXPORT does: it renders frames far faster than they play, and the
    /// rate control spends bits by the time between the stamps, so a frame
    /// [`encode`](Self::encode) stamps with the wall clock would get a
    /// fraction of the bitrate. Stamps must rise; one encoder takes one kind
    /// of stamp. Otherwise as [`encode`](Self::encode).
    pub fn encode_at(&self, frame: VideoFrame, timestamp_us: u64, force_keyframe: bool) -> bool {
        self.submit(
            frame,
            force_keyframe,
            Some(i64::try_from(timestamp_us).unwrap_or(i64::MAX)),
        )
    }

    /// [`encode`](Self::encode) / [`encode_at`](Self::encode_at): the wall
    /// clock's stamp when `micros` is `None`.
    fn submit(&self, frame: VideoFrame, force_keyframe: bool, micros: Option<i64>) -> bool {
        let Some(inner) = (unsafe { (self.ptr as *mut EncoderInner).as_mut() }) else {
            return false;
        };
        let taken = inner
            .thread
            .offer(EncodeJob::Frame(frame, force_keyframe, micros));
        if taken {
            inner.frames_encoded = inner.frames_encoded.wrapping_add(1);
        }
        taken
    }

    /// Pull the next encoded chunk, or `None` when nothing is ready yet.
    /// Drain it from a timer (or after each [`encode`](Self::encode)): the
    /// chunks of a frame come out once its encoder thread has them.
    pub fn recv_packet(&mut self) -> azul_css::corety::OptionU8Vec {
        match unsafe { (self.ptr as *mut EncoderInner).as_mut() } {
            Some(inner) => lock(&inner.packets).pop_front().into(),
            None => azul_css::corety::OptionU8Vec::None,
        }
    }

    /// Wait until every frame handed to [`encode`](Self::encode) so far is
    /// encoded, so [`recv_packet`](Self::recv_packet) gives back all of their
    /// packets (end of stream, a probe, a test). Blocks the caller for as
    /// long as that takes (one hardware encode per queued frame).
    pub fn flush(&self) {
        if let Some(inner) = unsafe { (self.ptr as *const EncoderInner).as_ref() } {
            let _ = inner.thread.wait_idle(EncodeJob::Flush);
        }
    }

    /// The thread this handle's engine runs on (`None`: the caller's).
    #[cfg(test)]
    pub(crate) fn engine_thread(&self) -> Option<std::thread::ThreadId> {
        unsafe { (self.ptr as *const EncoderInner).as_ref() }
            .and_then(|inner| inner.thread.thread_id())
    }

    /// Frames submitted to [`encode`](Self::encode) so far (stub progress).
    pub fn frames_encoded(&self) -> u64 {
        unsafe { (self.ptr as *const EncoderInner).as_ref() }
            .map(|i| i.frames_encoded)
            .unwrap_or(0)
    }

    /// Release the encoder. (Drop does this too.)
    pub fn close(&mut self) {
        self.drop_inner();
    }

    fn drop_inner(&mut self) {
        if self.run_destructor && !self.ptr.is_null() {
            unsafe {
                drop(Box::from_raw(self.ptr as *mut EncoderInner));
            }
        }
        self.ptr = core::ptr::null_mut();
        self.run_destructor = false;
    }
}

impl Drop for VideoEncoder {
    fn drop(&mut self) {
        self.drop_inner();
    }
}

/// Engine-side recorder state: the gstreamer subprocess + its stdin (raw RGBA in).
struct RecorderInner {
    child: Option<std::process::Child>,
    stdin: Option<std::process::ChildStdin>,
    width: u32,
    height: u32,
    frames: u64,
}

/// A **software** screen/frame recorder: feed it RGBA [`VideoFrame`]s and it muxes
/// them into an MP4 via a gstreamer `x264enc` subprocess (`fdsrc ! rawvideoparse !
/// videoconvert ! video/x-raw,format=I420 ! x264enc ! mp4mux ! filesink`). This is
/// the fallback when there's no hardware encode (see [`provision::VideoEncodeCheck`]
/// — true on this GTX 960). A C-ABI handle like [`VideoEncoder`].
#[repr(C)]
pub struct ScreenRecorder {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

impl Clone for ScreenRecorder {
    fn clone(&self) -> Self {
        ScreenRecorder {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}
impl Default for ScreenRecorder {
    fn default() -> Self {
        ScreenRecorder {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

impl ScreenRecorder {
    /// Start recording RGBA frames of `width`x`height` at `fps` to the MP4 at
    /// `path` (software x264). Returns an invalid handle (`is_recording()` false)
    /// if gstreamer (`gst-launch-1.0`) isn't installed.
    pub fn start(path: AzString, width: u32, height: u32, fps: u32) -> ScreenRecorder {
        use std::process::{Command, Stdio};
        let fps = fps.max(1);
        let child = Command::new("gst-launch-1.0")
            .args([
                "-q",
                "fdsrc",
                "fd=0",
                "!",
                "rawvideoparse",
                &format!("width={}", width),
                &format!("height={}", height),
                "format=rgba",
                &format!("framerate={}/1", fps),
                "!",
                "videoconvert",
                "!",
                "video/x-raw,format=I420",
                "!",
                "x264enc",
                "!",
                "mp4mux",
                "!",
                "filesink",
                &format!("location={}", path.as_str()),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        let mut child = match child {
            Ok(c) => c,
            Err(e) => {
                eprintln!(
                    "[azul][video] ScreenRecorder::start: failed to spawn gst-launch-1.0 ({e}) — \
                     recording DISABLED (handle invalid, is_recording() = false). Install \
                     gstreamer with the x264enc + mp4mux plugins"
                );
                return ScreenRecorder::default();
            }
        };
        let stdin = child.stdin.take();
        let inner = Box::new(RecorderInner {
            child: Some(child),
            stdin,
            width,
            height,
            frames: 0,
        });
        ScreenRecorder {
            ptr: Box::into_raw(inner) as *mut c_void,
            run_destructor: true,
        }
    }

    /// Whether recording is active (the gstreamer subprocess started).
    pub fn is_recording(&self) -> bool {
        !self.ptr.is_null()
    }

    /// Feed one RGBA `VideoFrame` (its `width`x`height` must match `start`). Returns
    /// false if not recording, the frame is too small, or the encoder has exited.
    pub fn write_frame(&self, frame: VideoFrame) -> bool {
        use std::io::Write;
        if let Some(inner) = unsafe { (self.ptr as *mut RecorderInner).as_mut() } {
            let need = (inner.width as usize) * (inner.height as usize) * 4;
            let bytes = frame.bytes.as_ref();
            if bytes.len() < need {
                return false;
            }
            if let Some(si) = inner.stdin.as_mut() {
                if si.write_all(&bytes[..need]).is_ok() {
                    inner.frames = inner.frames.wrapping_add(1);
                    return true;
                }
            }
        }
        false
    }

    /// Frames written so far.
    pub fn frames_written(&self) -> u64 {
        unsafe { (self.ptr as *const RecorderInner).as_ref() }
            .map(|i| i.frames)
            .unwrap_or(0)
    }

    /// Finish the recording and resume `on_result` with a
    /// [`ScreenRecordingResult`]: closes the encoder's input so it finalizes
    /// the MP4, waits for it, and releases the handle. Finalizing is
    /// asynchronous by nature (MediaRecorder's `dataavailable` on web, the
    /// gstreamer exit here), which is why the answer is delivered as a
    /// resume rather than returned. Same contract as every request function:
    /// the callback never runs re-entrantly inside the requesting activation.
    pub fn finish(
        &mut self,
        data: azul_core::refany::RefAny,
        on_result: azul_layout::callbacks::ResumeCallback,
    ) -> azul_core::task::RequestId {
        let was_recording = self.is_recording();
        let ok = self.finish_blocking();
        let error = if ok {
            None
        } else if was_recording {
            Some(AzString::from_const_str(
                "the encoder process did not exit cleanly",
            ))
        } else {
            Some(AzString::from_const_str("no recording in progress"))
        };
        azul_layout::request::complete(
            data,
            on_result,
            ScreenRecordingResult {
                ok,
                error: error.into(),
            },
        )
    }

    /// The synchronous finalize behind [`Self::finish`]: close the encoder's
    /// input so it finalizes the MP4, wait for it, and release the handle.
    /// Returns true if gstreamer exited cleanly. (Drop does a best-effort
    /// finalize too, if you don't call this.)
    pub fn finish_blocking(&mut self) -> bool {
        let ok = if let Some(inner) = unsafe { (self.ptr as *mut RecorderInner).as_mut() } {
            drop(inner.stdin.take()); // EOF → gst writes the moov atom + exits
            match inner.child.take() {
                Some(mut c) => c.wait().map(|s| s.success()).unwrap_or(false),
                None => false,
            }
        } else {
            false
        };
        self.drop_inner();
        ok
    }

    fn drop_inner(&mut self) {
        if self.run_destructor && !self.ptr.is_null() {
            unsafe {
                drop(Box::from_raw(self.ptr as *mut RecorderInner));
            }
        }
        self.ptr = core::ptr::null_mut();
        self.run_destructor = false;
    }
}

/// Result of [`ScreenRecorder::finish`]. `ok` is `true` when the recording
/// was finalized; `error` says why not otherwise.
#[repr(C)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenRecordingResult {
    pub ok: bool,
    pub error: azul_css::corety::OptionString,
}

azul_css::impl_option!(
    ScreenRecordingResult,
    OptionScreenRecordingResult,
    copy = false,
    [Debug, Clone, PartialEq, Eq]
);

impl ScreenRecordingResult {
    /// Downcast the `result` RefAny delivered to a `ResumeCallback`.
    pub fn downcast(mut result: azul_core::refany::RefAny) -> OptionScreenRecordingResult {
        result.downcast_ref::<Self>().map(|r| r.clone()).into()
    }
}

impl Drop for ScreenRecorder {
    fn drop(&mut self) {
        if self.run_destructor && !self.ptr.is_null() {
            if let Some(inner) = unsafe { (self.ptr as *mut RecorderInner).as_mut() } {
                drop(inner.stdin.take());
                if let Some(mut c) = inner.child.take() {
                    let _ = c.wait();
                }
            }
        }
        self.drop_inner();
    }
}

/// A hardware video decoder handle. Feed it encoded chunks with `decode`; it
/// returns decoded `VideoFrame`s as they become available.
#[repr(C)]
pub struct VideoDecoder {
    pub ptr: *mut c_void,
    pub run_destructor: bool,
}

impl Clone for VideoDecoder {
    fn clone(&self) -> Self {
        VideoDecoder {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}
impl Default for VideoDecoder {
    fn default() -> Self {
        VideoDecoder {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

impl VideoDecoder {
    /// Open a decoder (H.265 if `h265` else H.264) using the platform-native
    /// backend, on a thread of its own: [`decode`](Self::decode) only queues
    /// the chunk, and the pictures come out of
    /// [`recv_frame`](Self::recv_frame) once that thread has decoded them.
    /// Returns an invalid handle (`is_open()` false) wherever this build
    /// cannot decode: no engine compiled in, the engine does not open on
    /// this machine (no Vulkan Video device, VideoToolbox not loadable), or
    /// H.265 (no backend implements it yet).
    pub fn open(h265: bool) -> VideoDecoder {
        Self::open_with(h265, true)
    }

    /// [`Self::open`] for a caller that is off the UI thread already (the
    /// `<video>` widget's worker): the engine runs on the caller's thread,
    /// and `decode` hands the chunk's pictures out before it returns.
    pub(crate) fn open_on_this_thread(h265: bool) -> VideoDecoder {
        Self::open_with(h265, false)
    }

    fn open_with(h265: bool, threaded: bool) -> VideoDecoder {
        let engine = if h265 {
            Err(String::from("H.265 decode is not wired yet (H.264 only)"))
        } else {
            decode_engine()
        };
        let inner = match engine.and_then(|_| DecoderInner::open_h264(threaded)) {
            Ok(inner) => Box::new(inner),
            Err(why) => {
                say_not_open("VideoDecoder::open", &why);
                return VideoDecoder::default();
            }
        };
        VideoDecoder {
            ptr: Box::into_raw(inner) as *mut c_void,
            run_destructor: true,
        }
    }

    /// Whether the decoder opened.
    pub fn is_open(&self) -> bool {
        !self.ptr.is_null()
    }

    /// The thread this handle's engine runs on (`None`: the caller's).
    #[cfg(test)]
    pub(crate) fn engine_thread(&self) -> Option<std::thread::ThreadId> {
        match unsafe { (self.ptr as *const DecoderInner).as_ref() }.map(|inner| &inner.runner) {
            Some(DecodeRunner::Thread(thread)) => thread.thread_id(),
            _ => None,
        }
    }

    /// Submit one encoded chunk (Annex-B H.264). Returns `true` if the chunk
    /// was taken (the decoder is open). It is decoded on the decoder's own
    /// thread; the frames come out of [`recv_frame`](Self::recv_frame) once
    /// decoded - later than this call, and decode is pipelined and B-frame
    /// reordered, so one chunk can yield zero or several frames. The same
    /// output-callback shape as WebCodecs, so submit and poll are separate
    /// steps on every target.
    pub fn decode(&self, data: U8Vec) -> bool {
        let Some(inner) = (unsafe { (self.ptr as *mut DecoderInner).as_mut() }) else {
            return false;
        };
        inner.frames_decoded = inner.frames_decoded.wrapping_add(1);
        inner.submit(DecodeJob::Chunk(data))
    }

    /// Hand decoded frames out in `format`: an NV12 variant (the decoder's
    /// own 4:2:0 - no conversion; a tile shows it through the GPU's YUV
    /// shader or the CPU rasterizer's fused convert), BGRA8, or RGBA8 (the
    /// default). Applies from the next picture. Backends that cannot give
    /// the format hand out RGBA8; every frame says its format
    /// (`VideoFrame::format`).
    pub fn set_output_format(&self, format: azul_core::resources::RawImageFormat) {
        if let Some(inner) = unsafe { (self.ptr as *mut DecoderInner).as_mut() } {
            let _ = inner.submit(DecodeJob::Format(format));
        }
    }

    /// Hand decoded frames out at `width` x `height` (`0 x 0`: the stream's
    /// own size) - the size of the tile that shows them, so a 720p stream in
    /// a small tile is scaled once, by the decoder (on the GPU with
    /// VideoToolbox), instead of by the renderer on every paint. Applies
    /// from the next picture. Backends that cannot scale hand out the
    /// stream's size.
    pub fn set_output_size(&self, width: u32, height: u32) {
        if let Some(inner) = unsafe { (self.ptr as *mut DecoderInner).as_mut() } {
            let _ = inner.submit(DecodeJob::Size(width, height));
        }
    }

    /// Pull the next decoded frame, or `None` when nothing is ready yet.
    /// Drain it from a timer (or after each [`decode`](Self::decode)).
    pub fn recv_frame(&mut self) -> OptionVideoFrame {
        self.next_frame()
    }

    /// Pull the next already-decoded frame without feeding more input. Several
    /// may be waiting (pipelining + B-frame reordering, or the decoder's
    /// thread got ahead of the caller); loop `next_frame` until it returns
    /// `None`.
    pub fn next_frame(&self) -> OptionVideoFrame {
        match unsafe { (self.ptr as *const DecoderInner).as_ref() } {
            Some(inner) => lock(&inner.ready).pop_front().into(),
            None => OptionVideoFrame::None,
        }
    }

    /// Flush the decoder at end-of-stream: waits until every chunk handed to
    /// [`decode`](Self::decode) is decoded and the frames held back for
    /// reordering are out, and returns the first frame not pulled yet (drain
    /// the rest with [`next_frame`](Self::next_frame)).
    pub fn flush(&self) -> OptionVideoFrame {
        let Some(inner) = (unsafe { (self.ptr as *mut DecoderInner).as_mut() }) else {
            return OptionVideoFrame::None;
        };
        match &mut inner.runner {
            DecodeRunner::Thread(thread) => {
                let _ = thread.wait_idle(DecodeJob::Flush);
            }
            DecodeRunner::Here(engine) => {
                let mut out = std::collections::VecDeque::new();
                engine.flush(&mut out);
                lock(&inner.ready).extend(out);
            }
        }
        lock(&inner.ready).pop_front().into()
    }

    /// Release the decoder. (Drop does this too.)
    pub fn close(&mut self) {
        self.drop_inner();
    }

    fn drop_inner(&mut self) {
        if self.run_destructor && !self.ptr.is_null() {
            unsafe {
                drop(Box::from_raw(self.ptr as *mut DecoderInner));
            }
        }
        self.ptr = core::ptr::null_mut();
        self.run_destructor = false;
    }
}

impl Drop for VideoDecoder {
    fn drop(&mut self) {
        self.drop_inner();
    }
}

#[cfg(test)]
mod honest_handle_tests {
    use azul_core::video::{OptionVideoFrame, VideoFrame};
    use azul_css::{corety::OptionU8Vec, U8Vec};

    use super::{decode_engine_compiled, provision::VideoEncodeCheck, VideoDecoder, VideoEncoder};
    use crate::desktop::extra::capability::PlatformCapability;

    const W: u32 = 320;
    const H: u32 = 240;

    /// Frame `i` of eight vertical colour bars moving 4 px per frame.
    fn bars(i: u32) -> VideoFrame {
        const COLOURS: [[u8; 3]; 8] = [
            [235, 235, 235],
            [235, 235, 16],
            [16, 235, 235],
            [16, 235, 16],
            [235, 16, 235],
            [235, 16, 16],
            [16, 16, 235],
            [16, 16, 16],
        ];
        let mut bytes = Vec::with_capacity((W * H * 4) as usize);
        for _y in 0..H {
            for x in 0..W {
                let bar = (((x + i * 4) % W) * 8 / W) as usize;
                bytes.extend_from_slice(&COLOURS[bar]);
                bytes.push(255);
            }
        }
        VideoFrame::new(W, H, U8Vec::from_vec(bytes))
    }

    /// Encodes eight frames (the first a forced keyframe) and returns every
    /// chunk the encoder gave back.
    fn encode_some(encoder: &mut VideoEncoder) -> Vec<Vec<u8>> {
        let mut chunks = Vec::new();
        for i in 0..8 {
            assert!(encoder.encode(bars(i), i == 0), "an open encoder takes frames");
            // The codec thread encodes it; `flush` waits until it has.
            encoder.flush();
            while let OptionU8Vec::Some(chunk) = encoder.recv_packet() {
                chunks.push(chunk.as_ref().to_vec());
            }
        }
        chunks
    }

    /// An encoder and a decoder do their work on a thread of their own: the
    /// thread that holds the handle - the UI thread, in a call - only queues
    /// frames and chunks and takes packets and pictures out. Encoding and
    /// decoding on it cost every frame a VideoToolbox round trip (and the
    /// pixel copies around it) inside a UI callback.
    #[test]
    fn an_open_codec_runs_its_engine_on_a_thread_of_its_own() {
        let here = std::thread::current().id();
        let encoder = VideoEncoder::open(W, H, false, 400);
        if encoder.is_open() {
            let engine = encoder
                .engine_thread()
                .expect("an open encoder has an engine thread");
            assert_ne!(engine, here, "the encoder encodes on the caller's thread");
        }
        let decoder = VideoDecoder::open(false);
        if decoder.is_open() {
            let engine = decoder
                .engine_thread()
                .expect("an open decoder has an engine thread");
            assert_ne!(engine, here, "the decoder decodes on the caller's thread");
        }
    }

    /// `encode` hands the frame over and returns; what comes out is pulled
    /// with `recv_packet` once the codec thread has it, and `flush` waits for
    /// every frame handed over before it. The first frame (forced) is a
    /// keyframe, as before.
    #[test]
    fn a_flushed_encoder_has_given_back_the_packets_of_every_frame_it_took() {
        let mut encoder = VideoEncoder::open(W, H, false, 400);
        if !encoder.is_open() {
            return;
        }
        assert!(encoder.encode(bars(0), true));
        encoder.flush();
        let mut first = Vec::new();
        while let OptionU8Vec::Some(chunk) = encoder.recv_packet() {
            first.extend_from_slice(chunk.as_ref());
        }
        assert!(!first.is_empty(), "the keyframe came out by the flush");
        // An IDR slice (NAL type 5) is in it.
        assert!(
            first.windows(5).any(|w| w[..4] == [0, 0, 0, 1] && w[4] & 0x1f == 5),
            "the forced first frame is a keyframe"
        );
    }

    /// `VideoEncoder::open` hands out an open handle only where this build
    /// encodes: an engine is compiled in and works on this machine. An open
    /// handle gives packets back; everywhere else (no engine in this build,
    /// or it does not load here) the handle is not open, and
    /// `VideoEncodeCheck` says the same.
    #[test]
    fn an_encoder_handle_is_open_only_where_it_gives_packets_back() {
        let check = VideoEncodeCheck::run();
        let mut encoder = VideoEncoder::open(W, H, false, 400);
        if !encoder.is_open() {
            assert!(
                !check.hw_encode_ready,
                "no encoder opens, so the check must not say ready: {}",
                check.detail.as_str()
            );
            return;
        }
        let chunks = encode_some(&mut encoder);
        assert!(
            !chunks.is_empty(),
            "an open encoder must give packets back (backend {})",
            VideoEncoder::backend_name().as_str()
        );
        assert!(
            check.hw_encode_ready,
            "an encoder works, so the check must say ready: {}",
            check.detail.as_str()
        );
    }

    /// An export stamps every frame with its own time (`encode_at`); an
    /// open encoder takes stamped frames and gives packets back, one that
    /// did not open takes none.
    #[test]
    fn an_encoder_takes_frames_stamped_with_their_own_times() {
        assert!(
            !VideoEncoder::default().encode_at(bars(0), 0, true),
            "an encoder that did not open takes no frame"
        );
        let mut encoder = VideoEncoder::open(W, H, false, 400);
        if !encoder.is_open() {
            return;
        }
        let mut chunks = 0usize;
        for i in 0..8u32 {
            assert!(encoder.encode_at(bars(i), u64::from(i) * 40_000, i == 0));
            while let OptionU8Vec::Some(_) = encoder.recv_packet() {
                chunks += 1;
            }
        }
        assert!(chunks > 0, "stamped frames come back as packets");
        assert_eq!(encoder.frames_encoded(), 8);
    }

    /// No backend implements H.265 yet: neither handle opens for it.
    #[test]
    fn no_codec_handle_opens_for_h265_which_no_backend_implements() {
        assert!(!VideoEncoder::open(W, H, true, 400).is_open());
        assert!(!VideoDecoder::open(true).is_open());
    }

    /// `VideoDecoder::open` hands out an open handle only where this build
    /// has a decode engine that opened here, and
    /// `PlatformCapability::video_codec` never reports decode a build without
    /// an engine cannot do (whatever the GPU could).
    #[test]
    fn a_decoder_handle_is_open_only_where_this_build_decodes() {
        let capability = PlatformCapability::video_codec();
        let decoder = VideoDecoder::open(false);
        if !decode_engine_compiled() {
            assert!(!decoder.is_open(), "no decode engine in this build");
            assert!(
                !capability.available,
                "no decode engine in this build: {}",
                capability.reason.as_str()
            );
        }
        if decoder.is_open() {
            assert!(
                capability.available,
                "a decoder opened, so the capability must say so: {}",
                capability.reason.as_str()
            );
        }
    }

    /// Where both handles open, a picture survives the round trip: what the
    /// encoder gives back, the decoder turns into frames of the same size.
    #[test]
    fn an_open_encoder_and_decoder_round_trip_a_picture() {
        let mut encoder = VideoEncoder::open(W, H, false, 400);
        let mut decoder = VideoDecoder::open(false);
        if !encoder.is_open() || !decoder.is_open() {
            return;
        }
        for chunk in encode_some(&mut encoder) {
            assert!(decoder.decode(U8Vec::from_vec(chunk)));
        }
        // The codec thread decodes them; `flush` waits until it has.
        let mut frames = Vec::new();
        if let OptionVideoFrame::Some(frame) = decoder.flush() {
            frames.push(frame);
        }
        while let OptionVideoFrame::Some(frame) = decoder.recv_frame() {
            frames.push(frame);
        }
        assert!(!frames.is_empty(), "the decoder turned no chunk into a picture");
        for frame in &frames {
            assert_eq!((frame.width, frame.height), (W, H));
        }
    }
}

#[cfg(test)]
mod screenrec_tests {
    use azul_css::{AzString, U8Vec};

    use super::{ScreenRecorder, VideoFrame};

    // End-to-end: record synthetic RGBA frames → a real MP4 via the gst x264 sink.
    // Skips cleanly if gstreamer isn't installed (e.g. minimal CI).
    #[test]
    fn screen_recorder_smoke() {
        let path = "/tmp/azul_screenrec_test.mp4";
        let _ = std::fs::remove_file(path);
        let mut r = ScreenRecorder::start(AzString::from(path), 64, 48, 30);
        if !r.is_recording() {
            eprintln!("gstreamer unavailable — skipping ScreenRecorder smoke test");
            return;
        }
        for f in 0..24u32 {
            let mut buf = vec![0u8; 64 * 48 * 4];
            for px in buf.chunks_exact_mut(4) {
                px[0] = (f * 10) as u8;
                px[1] = 120;
                px[2] = 64;
                px[3] = 255;
            }
            let frame = VideoFrame::new(64, 48, U8Vec::from_vec(buf));
            assert!(r.write_frame(frame), "write_frame {}", f);
        }
        assert_eq!(r.frames_written(), 24);
        assert!(r.finish_blocking(), "finish (gst exit)");
        let sz = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        assert!(sz > 0, "mp4 should be non-empty (got {} bytes)", sz);
        eprintln!("ScreenRecorder smoke: wrote {}-byte mp4", sz);
    }
}

#[cfg(test)]
mod codec_thread_tests {
    use std::sync::{mpsc, Arc, Mutex, PoisonError};

    use super::CodecThread;

    enum Job {
        Number(u32),
        Done(mpsc::Sender<()>),
    }

    /// The engine is made on the codec thread, and every job runs there, in
    /// the order it was queued.
    #[test]
    fn a_codec_thread_makes_its_engine_and_runs_its_jobs_on_itself_in_order() {
        let seen: Arc<Mutex<Vec<(u32, std::thread::ThreadId)>>> = Arc::default();
        let log = Arc::clone(&seen);
        let made_on: Arc<Mutex<Option<std::thread::ThreadId>>> = Arc::default();
        let made = Arc::clone(&made_on);
        let thread = CodecThread::spawn(
            "azul-test-codec",
            None,
            move || {
                *made.lock().unwrap_or_else(PoisonError::into_inner) =
                    Some(std::thread::current().id());
                Ok(0_u32)
            },
            move |count: &mut u32, job: Job| match job {
                Job::Number(n) => {
                    *count += 1;
                    log.lock()
                        .unwrap_or_else(PoisonError::into_inner)
                        .push((n, std::thread::current().id()));
                }
                Job::Done(done) => {
                    let _ = done.send(());
                }
            },
        )
        .expect("the codec thread starts");
        for n in 0..5_u32 {
            assert!(thread.offer(Job::Number(n)));
        }
        assert!(thread.wait_idle(Job::Done), "the jobs ran");
        let engine = thread.thread_id().expect("an engine thread");
        assert_ne!(engine, std::thread::current().id());
        assert_eq!(
            *made_on.lock().unwrap_or_else(PoisonError::into_inner),
            Some(engine),
            "the engine was made on its thread"
        );
        let seen = seen.lock().unwrap_or_else(PoisonError::into_inner).clone();
        assert_eq!(
            seen.iter().map(|(n, _)| *n).collect::<Vec<_>>(),
            vec![0, 1, 2, 3, 4]
        );
        assert!(seen.iter().all(|(_, t)| *t == engine));
    }

    /// An engine that does not open is an error, and no thread is left.
    #[test]
    fn an_engine_that_does_not_open_is_an_error_not_a_thread() {
        let opened = CodecThread::<u32>::spawn(
            "azul-test-codec",
            None,
            || Err::<u32, String>(String::from("no device")),
            |_: &mut u32, _: u32| {},
        );
        assert_eq!(opened.err().as_deref(), Some("no device"));
    }

    /// A bounded queue that is full takes no more job: the caller drops the
    /// frame instead of waiting behind a slow encoder.
    #[test]
    fn a_full_bounded_queue_refuses_the_next_job() {
        let (gate_tx, gate_rx) = mpsc::channel::<()>();
        let (started_tx, started_rx) = mpsc::channel::<()>();
        let thread = CodecThread::spawn(
            "azul-test-codec",
            Some(1),
            || Ok(()),
            move |_: &mut (), _: u32| {
                let _ = started_tx.send(());
                // Held here until the gate opens (or closes).
                let _ = gate_rx.recv();
            },
        )
        .expect("the codec thread starts");
        assert!(thread.offer(0), "an empty queue takes a job");
        started_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("the engine took the first job");
        assert!(thread.offer(1), "the queue has room for one");
        assert!(!thread.offer(2), "a full queue takes no more");
        // Closing the gate lets the engine through; dropping the handle
        // joins the thread.
        drop(gate_tx);
        drop(thread);
    }
}
