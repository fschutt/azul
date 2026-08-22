//! Shared core for the "video-ish" widgets (camera / screencap / video).
//!
//! All three are identical in architecture (RefAny dataset + AfterMount
//! background capture/decode thread + writeback that uploads each frame into a
//! stable external GL texture + recomposites). Only the *config* and the
//! *worker* differ. This module holds the duplicated pieces - the [`VideoFrame`]
//! the worker produces and [`present_frame`], the GL writeback core - so each
//! widget is a thin config+worker wrapper and there's a single place for GL
//! fixes + the real platform workers (AVFoundation / ScreenCaptureKit /
//! vk-video) to plug in.
//!
//! NOTE: GL code - compile-verified here; the actual texture rendering must be
//! verified on a machine with a window + GPU.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use azul_core::resources::UpdateImageType;
use azul_core::callbacks::Update;
use azul_core::gl::gl::{RGBA, TEXTURE_2D, UNSIGNED_BYTE};
use azul_core::gl::{GlContextPtr, OptionU8VecRef, U8VecRef};
use azul_core::geom::PhysicalSizeU32;
use azul_core::refany::RefAny;
use azul_core::resources::ImageRef;
use azul_core::task::{OptionThreadSendMsg, ThreadId, ThreadReceiver, ThreadSendMsg};
use azul_core::video::{ConsumerFrame, FrameConsumer, VideoFrame};
use azul_css::impl_option_inner; // brought into scope for impl_widget_callback!'s impl_option!
use azul_css::props::basic::ColorU;

use crate::callbacks::CallbackInfo;
use crate::image_scale::{self, ResampleFn, SrcImage};
use crate::thread::{
    ThreadReceiveMsg, ThreadSender, ThreadWriteBackMsg, WriteBackCallback, WriteBackCallbackType,
};

/// User hook fired once per captured/decoded frame - the backreference
/// dependency-injection pattern (see `architecture.md`).
///
/// A capture widget's
/// private writeback invokes it with each [`VideoFrame`], so application code
/// can apply effects, save the frame into its own data model, or send it over
/// the network (azul-meet). Returns `Update` like any callback. Wired via
/// `CameraWidget::with_on_frame` / `ScreenCaptureWidget::with_on_frame` /
/// `VideoWidget::with_on_frame`.
pub type OnVideoFrameCallbackType = extern "C" fn(RefAny, CallbackInfo, VideoFrame) -> Update;
impl_widget_callback!(
    OnVideoFrame,
    OptionOnVideoFrame,
    OnVideoFrameCallback,
    OnVideoFrameCallbackType
);

// Host-invoker plumbing for managed-FFI bindings - see core/src/host_invoker.rs.
azul_core::impl_managed_callback! {
    wrapper:        OnVideoFrameCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: ON_VIDEO_FRAME_INVOKER,
    invoker_ty:     AzOnVideoFrameCallbackInvoker,
    thunk_fn:       az_on_video_frame_callback_thunk,
    setter_fn:      AzApp_setOnVideoFrameCallbackInvoker,
    from_handle_fn: AzOnVideoFrameCallback_createFromHostHandle,
    extra_args:     [ frame: VideoFrame ],
}

/// User hook fired once per CONSUMER per captured frame with that consumer's
/// cut ([`ConsumerFrame`]: the [`FrameConsumer`] it was cut for + the frame
/// at its size). Register consumers with `CameraWidget::with_consumer` /
/// `ScreenCaptureWidget::with_consumer`; route on `frame.consumer.id`
/// ("client Bob" gets his 500x200, the recorder its 1280x720, from ONE
/// capture). Returns `Update` like any callback.
pub type OnConsumerFrameCallbackType = extern "C" fn(RefAny, CallbackInfo, ConsumerFrame) -> Update;
impl_widget_callback!(
    OnConsumerFrame,
    OptionOnConsumerFrame,
    OnConsumerFrameCallback,
    OnConsumerFrameCallbackType
);

azul_core::impl_managed_callback! {
    wrapper:        OnConsumerFrameCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: ON_CONSUMER_FRAME_INVOKER,
    invoker_ty:     AzOnConsumerFrameCallbackInvoker,
    thunk_fn:       az_on_consumer_frame_callback_thunk,
    setter_fn:      AzApp_setOnConsumerFrameCallbackInvoker,
    from_handle_fn: AzOnConsumerFrameCallback_createFromHostHandle,
    extra_args:     [ frame: ConsumerFrame ],
}

/// Invoke a capture widget's optional `on_consumer_frame` hook with one
/// consumer's cut, returning the user's `Update` (`DoNothing` when unset).
pub fn invoke_on_consumer_frame(
    hook: &OptionOnConsumerFrame,
    info: &mut CallbackInfo,
    frame: ConsumerFrame,
) -> Update {
    match hook {
        OptionOnConsumerFrame::Some(h) => (h.callback.cb)(h.refany.clone(), *info, frame),
        OptionOnConsumerFrame::None => Update::DoNothing,
    }
}

/// Invoke a capture widget's optional `on_frame` hook with `frame`, returning
/// the user's `Update` (`DoNothing` when no hook is set). Shared by all three
/// capture widgets' writebacks.
pub fn invoke_on_frame(
    hook: &OptionOnVideoFrame,
    info: &mut CallbackInfo,
    frame: &VideoFrame,
) -> Update {
    match hook {
        OptionOnVideoFrame::Some(h) => {
            (h.callback.cb)(h.refany.clone(), *info, frame.clone())
        }
        OptionOnVideoFrame::None => Update::DoNothing,
    }
}

/// Present `frame` for a video-ish widget.
///
/// ONE path on every backend: install the frame as a raw RGBA `ImageRef` on
/// the widget's node via `change_node_image` → the content chokepoint
/// (`LayoutWindow::apply_content_change`), which patches the display list in
/// place and lets damage fall out of `ImageRef` identity. The widget never
/// branches on the renderer — that branch was the shipped bug: a CPU-rendered
/// window can still EXPOSE a GL context, so the widget took the GL path and
/// sent texture-only updates (`update_all_image_callbacks` → `ReRender`) that
/// the CPU rasterizer never saw; camera/screenshare tiles froze on their
/// placeholder. On GPU backends the `WebRender` translator re-uploads the
/// changed raster `ImageRef` — the backend decides texture vs raster, the
/// widget cannot know or care.
///
/// `current_id` is passed through unchanged (widgets store it; the GL texture
/// pool it used to name is gone).
pub fn present_frame(
    info: &mut CallbackInfo,
    dataset: RefAny,
    current_id: Option<u32>,
    frame: &VideoFrame,
) -> Option<u32> {
    present_frame_pixels(info, dataset, current_id, frame.bytes.clone(), frame.width, frame.height)
}

/// [`present_frame`] taking the pixels BY VALUE: the writebacks `mem::take`
/// them out of the frame `RefAny` (dropped right after) instead of cloning
/// a full frame on the main thread.
///
/// `premultiplied_alpha: true` because every
/// capture backend forces alpha 255, for which straight == premultiplied —
/// `load_rgba8` then skips its per-pixel multiply.
pub fn present_frame_pixels(
    info: &mut CallbackInfo,
    dataset: RefAny,
    current_id: Option<u32>,
    bytes: azul_css::U8Vec,
    width: u32,
    height: u32,
) -> Option<u32> {
    use azul_core::resources::{RawImage, RawImageData, RawImageFormat};

    if let Some(img) = ImageRef::new_rawimage(RawImage {
        pixels: RawImageData::U8(bytes),
        width: width as usize,
        height: height as usize,
        premultiplied_alpha: true,
        data_format: RawImageFormat::RGBA8,
        tag: b"azul-capture-frame".to_vec().into(),
    }) {
        if let Some(node) = info.get_node_id_of_root_dataset(dataset) {
            if let Some(nid) = node.node.into_crate_internal() {
                info.change_node_image(node.dom, nid, img, UpdateImageType::Content);
            }
        }
    }
    current_id
}

/// Upload tightly-packed RGBA8 pixels into the GL texture `texture_id`.
#[allow(clippy::cast_possible_wrap)] // bounded graphics/coord/counter/fixed-point cast
pub fn upload_rgba(gl: &GlContextPtr, texture_id: u32, frame: &VideoFrame) {
    gl.bind_texture(TEXTURE_2D, texture_id);
    gl.tex_image_2d(
        TEXTURE_2D,
        0,
        RGBA as i32,
        frame.width as i32,
        frame.height as i32,
        0,
        RGBA,
        UNSIGNED_BYTE,
        OptionU8VecRef::Some(U8VecRef::from(frame.bytes.as_ref())),
    );
}

/// What a capture backend is asked to open: the source, the size the frames
/// should have, the frame rate, and (screens) whether to leave the app's own
/// windows out of the picture.
///
/// `width` x `height` is the size the WIDGET needs — the covering size of
/// every consumer ([`required_capture_size`]) — not a constant. A backend
/// delivers the smallest size it can that covers the request (the camera's
/// next session preset up, the screen stream scaled down by the OS) and
/// reports the actual size with every frame; it never delivers less than it
/// can and is never asked for more than someone will look at.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CaptureRequest {
    /// Camera device index / display index (0 = default).
    pub index: u32,
    /// A specific window to capture by platform id (`0` = the whole source).
    /// Screens only; cameras ignore it.
    pub window: u64,
    /// Wanted frame width in px (`0` = the backend's default).
    pub width: u32,
    /// Wanted frame height in px (`0` = the backend's default).
    pub height: u32,
    /// Wanted frame rate (`0` = the backend's default, 30 on every backend
    /// that has one). A 300x200 tile does not need 30 fps.
    pub fps: u32,
    /// Screens: exclude this process's own windows from the capture. Without
    /// it a shared desktop that shows the sharing app loops: every tile
    /// repaint is a screen change, which emits a frame, which repaints the
    /// tile, ... — a steady 30 fps on an idle desktop.
    pub exclude_self: bool,
}

impl CaptureRequest {
    /// `index` at `width` x `height`, everything else default.
    #[must_use]
    pub const fn new(index: u32, width: u32, height: u32) -> Self {
        Self {
            index,
            window: 0,
            width,
            height,
            fps: 0,
            exclude_self: true,
        }
    }

    /// The same request at another size.
    #[must_use]
    pub const fn with_size(self, width: u32, height: u32) -> Self {
        Self {
            width,
            height,
            ..self
        }
    }

    /// `fps`, or the backend default when the request says 0.
    #[must_use]
    pub const fn fps_or(&self, default: u32) -> u32 {
        if self.fps > 0 {
            self.fps
        } else {
            default
        }
    }
}

/// What one blocking read of a capture source produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureRead {
    /// A new frame: tightly-packed RGBA8 of this size is in `out`.
    Frame {
        /// Delivered width in px.
        width: u32,
        /// Delivered height in px.
        height: u32,
    },
    /// Nothing new within the backend's wait: an idle screen, a camera that
    /// stalled (sleep/wake, a Continuity camera reconnecting). NOT the end of
    /// the stream — the worker keeps polling, and presents nothing, so an
    /// unchanged picture costs no repaint.
    Idle,
    /// The source is gone (device unplugged, stream stopped, error): the
    /// worker closes and exits.
    Ended,
}

/// A platform frame-capture backend (camera / screen), registered by the dll at
/// startup so the cross-platform capture widgets can pull **real** frames
/// instead of their built-in test pattern.
///
/// The dll provides one per OS (v4l2 on
/// Linux, `AVFoundation` on macOS, Media Foundation on Windows, `ScreenCaptureKit` /
/// `PipeWire` / DXGI for screens, ...). These are plain Rust fn pointers - the dll
/// links azul-layout statically, so registering + calling is a Rust-to-Rust
/// call, no `extern "C"`/trait-object dance.
#[derive(Debug, Clone, Copy)]
pub struct CaptureVTable {
    /// Open the source described by the request. Returns an opaque handle,
    /// or `0` on failure (the worker then falls back to the test pattern).
    pub open: fn(request: &CaptureRequest) -> u64,
    /// Block (bounded, ~1 s) for the next frame, writing tightly-packed RGBA8
    /// into `out` (resized as needed). See [`CaptureRead`] for the three
    /// outcomes — a timeout is `Idle`, never `Ended`.
    pub read: fn(handle: u64, out: &mut Vec<u8>) -> CaptureRead,
    /// Close + free the source.
    pub close: fn(handle: u64),
    /// Change the delivered size / fps of a RUNNING source without a
    /// close + open (macOS: a live session-preset switch, an SCStream
    /// `updateConfiguration`). `None`, or a `false` return, makes the worker
    /// reopen instead — the universal fallback, so a backend without it is
    /// merely slower to follow a resize, never wrong.
    pub reconfigure: Option<fn(handle: u64, request: &CaptureRequest) -> bool>,
}

static CAMERA_BACKEND: std::sync::OnceLock<CaptureVTable> = std::sync::OnceLock::new();
static SCREEN_BACKEND: std::sync::OnceLock<CaptureVTable> = std::sync::OnceLock::new();
static FRAME_RESAMPLER: std::sync::OnceLock<ResampleFn> = std::sync::OnceLock::new();

/// Register a platform-accelerated whole-frame scaler (the dll registers
/// Accelerate/vImage on macOS). It must be a pure function with
/// [`image_scale::resample_rgba`]'s contract — same inputs, same output
/// within rounding — because the fan-out may run it per consumer on any
/// thread. First registration wins; without one the portable scaler is used.
pub fn register_frame_resampler(resample: ResampleFn) {
    let _ = FRAME_RESAMPLER.set(resample);
}

/// The whole-frame scaler the capture fan-out uses: the registered
/// platform one, else [`image_scale::resample_rgba`].
#[must_use]
pub fn frame_resampler() -> ResampleFn {
    FRAME_RESAMPLER
        .get()
        .copied()
        .unwrap_or(image_scale::resample_rgba)
}

/// Register the platform **camera** capture backend (called once by the dll at
/// startup; the first registration wins). Without it, `CameraWidget` shows its
/// test pattern.
pub fn register_camera_backend(vtable: CaptureVTable) {
    let _ = CAMERA_BACKEND.set(vtable);
}

/// Register the platform **screen** capture backend (for `ScreenCaptureWidget`).
pub fn register_screen_backend(vtable: CaptureVTable) {
    let _ = SCREEN_BACKEND.set(vtable);
}

/// The registered camera backend, if the dll provided one for this platform.
pub fn camera_backend() -> Option<CaptureVTable> {
    CAMERA_BACKEND.get().copied()
}

/// The registered screen-capture backend, if any.
pub fn screen_backend() -> Option<CaptureVTable> {
    SCREEN_BACKEND.get().copied()
}

/// A platform **audio**-capture backend (microphone), registered by the dll so
/// `MicrophoneWidget` can pull real samples instead of the test tone.
///
/// Like
/// [`CaptureVTable`] but yields interleaved `f32` audio rather than RGBA video.
#[derive(Debug, Clone, Copy)]
pub struct AudioCaptureVTable {
    /// Open the default mic at `sample_rate` x `channels`. Opaque handle, or
    /// `0` on failure.
    pub open: fn(sample_rate: u32, channels: u16) -> u64,
    /// Block for the next chunk, writing interleaved `f32` into `out` (resized).
    /// Returns the frame count (`out.len() / channels`), or `0` on error / EOF
    /// (the worker then stops + closes).
    pub read: fn(handle: u64, out: &mut Vec<f32>) -> u32,
    /// Close + free the source.
    pub close: fn(handle: u64),
}

static MIC_BACKEND: std::sync::OnceLock<AudioCaptureVTable> = std::sync::OnceLock::new();

/// Register the platform microphone-capture backend (called once by the dll).
pub fn register_mic_backend(vtable: AudioCaptureVTable) {
    let _ = MIC_BACKEND.set(vtable);
}

/// The registered mic-capture backend, if the dll provided one for this platform.
pub fn mic_backend() -> Option<AudioCaptureVTable> {
    MIC_BACKEND.get().copied()
}

/// Poll the main->worker channel and report whether the worker was asked to
/// stop.
///
/// Every capture worker (camera / screencap / microphone) sits in a
/// `loop { read_device(); send_frame(); }`. Before this existed, none of them
/// ever looked at their `ThreadReceiver`, so `ThreadSendMsg::TerminateThread`
/// was never observed and the only way out was `sender.send()` failing — which
/// does NOT happen at shutdown, because the main thread still owns the
/// receiving end while it waits. The result was the 2 s grace period in
/// `crate::thread::default_thread_destructor_fn` expiring and the worker being
/// DETACHED:
///
/// ```text
/// [azul][thread] a background thread did not acknowledge TerminateThread
/// within 2000ms and was DETACHED rather than joined.
/// ```
///
/// (Reported twice from azul-meet on macOS after using camera + screenshare —
/// one line per capture worker.)
///
/// `ThreadReceiver::recv` is a `try_recv` under the hood, so this never blocks.
/// Non-terminate messages are drained and ignored: these workers have no other
/// commands, and leaving them queued would hide a `TerminateThread` sent behind
/// them.
#[must_use]
pub fn terminate_requested(recv: &mut ThreadReceiver) -> bool {
    loop {
        match recv.recv() {
            OptionThreadSendMsg::Some(ThreadSendMsg::TerminateThread) => return true,
            OptionThreadSendMsg::Some(_) => {}
            OptionThreadSendMsg::None => return false,
        }
    }
}

// ============================================================================
// The shared capture loop: one capture, many consumers
// ============================================================================

/// Who wants frames of which size — the main thread's view, sent to the
/// worker as `ThreadSendMsg::Custom(RefAny::new(CaptureTargets))` whenever it
/// changes (the node was laid out / resized, the app registered a consumer).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CaptureTargets {
    /// The on-screen tile's size in DEVICE pixels (logical size x hidpi), once
    /// layout has produced one. `None` before the first layout: the captured
    /// frame is shown as-is.
    pub preview: Option<(u32, u32)>,
    /// Every registered consumer (see [`FrameConsumer`]).
    pub consumers: Vec<FrameConsumer>,
    /// The widget has an `on_frame` hook, which receives the frame AS
    /// CAPTURED — so the capture may not shrink below the configured size
    /// and the source frame must travel to the main thread.
    pub wants_source: bool,
}

/// Drain the main->worker channel into `targets`; `true` if the worker was
/// asked to stop. The generalisation of [`terminate_requested`] for the
/// video workers: a `Custom` message carrying [`CaptureTargets`] replaces the
/// current targets (the LAST one wins, so a burst of resizes costs one
/// reconfigure), a terminate is reported, anything else is ignored.
#[must_use]
pub fn poll_capture_control(recv: &mut ThreadReceiver, targets: &mut CaptureTargets) -> bool {
    loop {
        match recv.recv() {
            OptionThreadSendMsg::Some(ThreadSendMsg::TerminateThread) => return true,
            OptionThreadSendMsg::Some(ThreadSendMsg::Custom(mut payload)) => {
                if let Some(t) = payload.downcast_ref::<CaptureTargets>() {
                    *targets = t.clone();
                }
            }
            OptionThreadSendMsg::Some(ThreadSendMsg::Tick) => {}
            OptionThreadSendMsg::None => return false,
        }
    }
}

/// The size the device should capture at: the covering size of everything
/// that wants frames.
///
/// * `floor` — the configured size when the app set one explicitly, or the
///   widget's default when an `on_frame` hook wants the source frame (the
///   hook sees what the config promised, so the capture never shrinks below
///   it);
/// * the preview tile (device px) and every consumer;
/// * `fallback` when none of the above is known yet (before the first
///   layout, no hook, no consumers).
///
/// "Client Bob wants 500x200, the preview is 100x200, no hook" -> 500x200.
/// "Only a 300x200 preview" -> 300x200: the camera is told to capture a
/// small frame instead of 1080p that is then thrown away.
#[must_use]
pub fn required_capture_size(
    floor: Option<(u32, u32)>,
    targets: &CaptureTargets,
    fallback: (u32, u32),
) -> (u32, u32) {
    let sizes = floor
        .into_iter()
        .chain(targets.preview)
        .chain(targets.consumers.iter().map(|c| (c.width, c.height)));
    image_scale::covering_size(sizes).unwrap_or(fallback)
}

/// Should a running source be reconfigured / reopened for `required`?
///
/// * `requested` — the size the source was last opened / reconfigured for;
/// * `delivered` — the size its frames actually have (backends snap up to a
///   preset, so this is often larger than `requested`);
/// * `required` — [`required_capture_size`] now.
///
/// Reopen when the consumers need MORE than the source delivers (quality), or
/// when they need so much less than was requested (both axes at most half)
/// that the device is doing 4x the work anyone looks at (cost). Comparing the
/// shrink case against `requested` rather than `delivered` is what makes a
/// 300x200 tile fed by the 640x480 minimum preset NOT reopen forever: the
/// request is already as small as it can be.
#[must_use]
pub const fn needs_reopen(
    requested: (u32, u32),
    delivered: (u32, u32),
    required: (u32, u32),
) -> bool {
    let more = (required.0 > delivered.0 || required.1 > delivered.1)
        && (required.0 > requested.0 || required.1 > requested.1);
    let much_less = required.0 * 2 <= requested.0 && required.1 * 2 <= requested.1;
    more || much_less
}

/// The worker's choice of preview cut: `Some(size)` when the tile's device
/// size is known AND differs from the captured frame (a same-size preview is
/// the source frame itself, shown without a copy).
#[must_use]
pub fn preview_cut_size(targets: &CaptureTargets, captured: (u32, u32)) -> Option<(u32, u32)> {
    targets
        .preview
        .filter(|&(w, h)| w > 0 && h > 0 && (w, h) != captured)
}

/// What one captured frame became after the worker's fan-out — the payload
/// of every capture writeback. Built off the main thread; the writeback
/// ([`present_captured`]) only hands things out.
#[derive(Debug)]
pub struct CapturedFrames {
    /// The frame as captured: present when the `on_frame` hook wants it, or
    /// when there is no preview cut (then it IS what goes on screen).
    pub source: Option<VideoFrame>,
    /// The on-screen tile's cut (consumer 0), at the tile's device size.
    pub preview: Option<VideoFrame>,
    /// Every registered consumer's cut.
    pub consumers: Vec<ConsumerFrame>,
    /// Back-pressure latch: set by the worker when it queues this payload,
    /// cleared by the writeback. While it is set the worker DROPS new frames
    /// instead of queueing them, so at most one frame is ever in flight — a
    /// busy main thread sees the newest frame late, never a growing backlog.
    pub in_flight: Arc<AtomicBool>,
}

/// The writeback core shared by the camera and screen widgets: release the
/// back-pressure latch, run the user hooks, put the preview (else the source
/// frame) on the widget's node. Returns the strongest `Update` a hook asked
/// for.
pub fn present_captured(
    info: &mut CallbackInfo,
    dataset: RefAny,
    on_frame: &OptionOnVideoFrame,
    on_consumer_frame: &OptionOnConsumerFrame,
    captured: &mut CapturedFrames,
) -> Update {
    captured.in_flight.store(false, Ordering::Release);
    let mut update = Update::DoNothing;
    if let Some(source) = captured.source.as_ref() {
        update.max_self(invoke_on_frame(on_frame, info, source));
    }
    for cut in core::mem::take(&mut captured.consumers) {
        update.max_self(invoke_on_consumer_frame(on_consumer_frame, info, cut));
    }
    let shown = captured.preview.take().or_else(|| captured.source.take());
    if let Some(frame) = shown {
        let _texture_id: Option<u32> =
            present_frame_pixels(info, dataset, None, frame.bytes, frame.width, frame.height);
    }
    update
}

/// The on-screen tile's size in DEVICE pixels for the callback's hit node:
/// the laid-out logical size times the window's hidpi factor. `None` before
/// layout. Logical pixels would undersize a Retina preview by 2x.
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded layout/render numeric cast
pub fn preview_size_for_node(info: &CallbackInfo) -> Option<(u32, u32)> {
    let size = info.get_node_size(info.get_hit_node())?;
    let dpi = info
        .get_current_window_state()
        .size
        .get_hidpi_factor()
        .inner
        .get()
        .max(0.5);
    let w = (size.width * dpi).round().max(1.0) as u32;
    let h = (size.height * dpi).round().max(1.0) as u32;
    Some((w, h))
}

/// Send `targets` to the capture worker `thread_id`. Best effort: a worker
/// that already exited has nothing to resize, and a merge that changes the
/// consumer list resends through the state's cloned sender.
pub fn send_capture_targets(info: &CallbackInfo, thread_id: ThreadId, targets: CaptureTargets) {
    if let Some(thread) = info.get_thread(&thread_id) {
        let _delivered: bool = thread.send_message(ThreadSendMsg::Custom(RefAny::new(targets)));
    }
}

/// Everything a widget's worker needs to run [`run_capture_loop`].
#[derive(Debug, Clone, Copy)]
pub struct CaptureSession {
    /// The platform backend, if the dll registered one.
    pub backend: Option<CaptureVTable>,
    /// The built-in generator shown when there is no backend or it fails to
    /// open (so a widget is never blank).
    pub test_pattern: CaptureVTable,
    /// Source / fps / exclusion; the size is filled in per reopen.
    pub request: CaptureRequest,
    /// See [`required_capture_size`].
    pub floor: Option<(u32, u32)>,
    /// See [`required_capture_size`].
    pub fallback: (u32, u32),
    /// The widget's writeback (its `extern "C"` wrapper around
    /// [`present_captured`]).
    pub writeback: WriteBackCallbackType,
    /// The scaler for the fan-out ([`frame_resampler`]).
    pub resample: ResampleFn,
    /// Minimum time between two reopens ([`REOPEN_COOLDOWN`] for the
    /// widgets; tests set zero).
    pub reopen_cooldown: std::time::Duration,
}

/// How long the worker waits for layout to report a preview size before it
/// opens the device at the fallback size. Layout runs right after mount, so
/// this is normally a few ms; it bounds the wait when a tile is never laid
/// out (display: none).
const PREVIEW_SIZE_GRACE: std::time::Duration = std::time::Duration::from_millis(150);
/// Minimum time between two reopens, so a window-edge drag (hundreds of
/// `NodeResized`s) costs at most one device restart per second.
pub const REOPEN_COOLDOWN: std::time::Duration = std::time::Duration::from_millis(1000);

/// THE capture worker: one loop for the camera and the screen widgets.
///
/// 1. Learn the targets (wait briefly for the first preview size so the
///    device is opened at the size the tile needs, not at a default that is
///    reopened a frame later).
/// 2. Open the backend at [`required_capture_size`]; on failure the test
///    pattern.
/// 3. Per frame: poll the control channel (new targets / terminate),
///    reconfigure or reopen when [`needs_reopen`] says so (rate-limited), read
///    a frame, drop it if the previous one is still in flight, else cut the
///    preview and every consumer from it OFF the main thread
///    ([`image_scale::fan_out`]) and queue one [`CapturedFrames`] writeback.
/// 4. `Ended` / a dead main thread / terminate -> close + return.
#[allow(clippy::too_many_lines)] // one loop, documented step by step above
pub fn run_capture_loop(
    session: CaptureSession,
    mut targets: CaptureTargets,
    sender: &mut ThreadSender,
    recv: &mut ThreadReceiver,
) {
    // 1. Wait (briefly) for the preview size if we do not have one.
    if targets.preview.is_none() {
        let deadline = std::time::Instant::now() + PREVIEW_SIZE_GRACE;
        while targets.preview.is_none() && std::time::Instant::now() < deadline {
            if poll_capture_control(recv, &mut targets) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    // 2. Open.
    let mut required = required_capture_size(session.floor, &targets, session.fallback);
    let mut request = session.request.with_size(required.0, required.1);
    let (mut backend, mut handle) = open_with_fallback(&session, &request);
    if handle == 0 {
        return;
    }
    let mut requested = required;
    let mut delivered: Option<(u32, u32)> = None;
    let mut last_open = std::time::Instant::now();
    let in_flight = Arc::new(AtomicBool::new(false));
    let mut buf: Vec<u8> = Vec::new();

    // 3. Frames.
    loop {
        if poll_capture_control(recv, &mut targets) {
            break;
        }
        required = required_capture_size(session.floor, &targets, session.fallback);
        if needs_reopen(requested, delivered.unwrap_or(requested), required)
            && last_open.elapsed() >= session.reopen_cooldown
        {
            request = request.with_size(required.0, required.1);
            let reconfigured = backend
                .reconfigure
                .is_some_and(|reconfigure| reconfigure(handle, &request));
            if !reconfigured {
                (backend.close)(handle);
                let (b, h) = open_with_fallback(&session, &request);
                if h == 0 {
                    return;
                }
                backend = b;
                handle = h;
            }
            requested = required;
            delivered = None;
            last_open = std::time::Instant::now();
        }

        let (fw, fh) = match (backend.read)(handle, &mut buf) {
            CaptureRead::Frame { width, height } if width > 0 && height > 0 => (width, height),
            CaptureRead::Frame { .. } | CaptureRead::Ended => break,
            CaptureRead::Idle => continue,
        };
        delivered = Some((fw, fh));
        if in_flight.load(Ordering::Acquire) {
            // The main thread has not presented the previous frame yet: drop
            // this one (the next read brings a newer one) rather than queue it.
            continue;
        }
        let captured = cut_frame(&targets, &mut buf, fw, fh, session.resample, &in_flight);
        in_flight.store(true, Ordering::Release);
        let sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
            WriteBackCallback::new(session.writeback),
            RefAny::new(captured),
        )));
        if !sent {
            break;
        }
    }

    // 4. Done.
    (backend.close)(handle);
}

/// Open the platform backend, else the test pattern. `(vtable, handle)`;
/// handle `0` if even the test pattern refuses (a zero-sized request).
fn open_with_fallback(session: &CaptureSession, request: &CaptureRequest) -> (CaptureVTable, u64) {
    if let Some(backend) = session.backend {
        let handle = (backend.open)(request);
        if handle != 0 {
            return (backend, handle);
        }
    }
    let pattern = session.test_pattern;
    let handle = (pattern.open)(request);
    (pattern, handle)
}

/// Cut the preview and every consumer from the captured frame in `buf`
/// (RGBA8 `fw` x `fh`), moving the pixels out only when the source frame
/// itself must travel (hook wants it, or no preview cut).
fn cut_frame(
    targets: &CaptureTargets,
    buf: &mut Vec<u8>,
    fw: u32,
    fh: u32,
    resample: ResampleFn,
    in_flight: &Arc<AtomicBool>,
) -> CapturedFrames {
    let src = SrcImage {
        bytes: buf.as_slice(),
        format: azul_core::resources::RawImageFormat::RGBA8,
        width: fw,
        height: fh,
    };
    let preview = preview_cut_size(targets, (fw, fh)).and_then(|(pw, ph)| {
        let rgba = image_scale::cut(&src, pw, ph, resample);
        (!rgba.is_empty()).then(|| VideoFrame::new(pw, ph, rgba.into()))
    });
    let consumers = image_scale::fan_out(&src, &targets.consumers, resample);
    let source = (targets.wants_source || preview.is_none())
        .then(|| VideoFrame::new(fw, fh, core::mem::take(buf).into()));
    CapturedFrames {
        source,
        preview,
        consumers,
        in_flight: in_flight.clone(),
    }
}

// ----------------------------------------------------------------------------
// Test patterns: the built-in generators behind the same vtable, so the
// worker has ONE loop whether or not a platform backend exists.
// ----------------------------------------------------------------------------

/// Which built-in pattern a capture widget shows without a backend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestPattern {
    /// A solid frame whose colour cycles per frame (the camera widget's).
    ColourCycle,
    /// A light band scrolling down a dark frame (the screen widget's).
    MovingBand,
}

struct TestPatternState {
    kind: TestPattern,
    width: u32,
    height: u32,
    tick: u32,
}

/// Interval between two test-pattern frames (~30 fps).
const TEST_PATTERN_FRAME: std::time::Duration = std::time::Duration::from_millis(33);

fn test_pattern_open(request: &CaptureRequest, kind: TestPattern) -> u64 {
    if request.width == 0 || request.height == 0 {
        return 0;
    }
    Box::into_raw(Box::new(TestPatternState {
        kind,
        width: request.width,
        height: request.height,
        tick: 0,
    })) as u64
}

#[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/counter/fixed-point cast
fn test_pattern_read(handle: u64, out: &mut Vec<u8>) -> CaptureRead {
    // SAFETY: `handle` is a `Box<TestPatternState>` from `test_pattern_open`,
    // alive until `test_pattern_close`; the worker never reads after close.
    let Some(state) = (unsafe { (handle as *mut TestPatternState).as_mut() }) else {
        return CaptureRead::Ended;
    };
    if state.tick > 0 {
        std::thread::sleep(TEST_PATTERN_FRAME);
    }
    let (w, h) = (state.width as usize, state.height as usize);
    out.clear();
    out.reserve(w * h * 4);
    match state.kind {
        TestPattern::ColourCycle => {
            let tick = state.tick;
            let color = [
                (tick % 256) as u8,
                (tick.wrapping_mul(2) % 256) as u8,
                (tick.wrapping_mul(3) % 256) as u8,
                255u8,
            ];
            for _ in 0..w * h {
                out.extend_from_slice(&color);
            }
            state.tick = state.tick.wrapping_add(8);
        }
        TestPattern::MovingBand => {
            let band = (state.tick as usize) % h.max(1);
            for y in 0..h {
                let v = if y.abs_diff(band) < 8 { 235u8 } else { 28u8 };
                for _ in 0..w {
                    out.extend_from_slice(&[v, v, v, 255]);
                }
            }
            state.tick = state.tick.wrapping_add(12);
        }
    }
    CaptureRead::Frame {
        width: state.width,
        height: state.height,
    }
}

fn test_pattern_close(handle: u64) {
    if handle != 0 {
        // SAFETY: the handle came from `Box::into_raw` in `test_pattern_open`
        // and is closed exactly once by the worker.
        drop(unsafe { Box::from_raw(handle as *mut TestPatternState) });
    }
}

fn colour_cycle_open(request: &CaptureRequest) -> u64 {
    test_pattern_open(request, TestPattern::ColourCycle)
}
fn moving_band_open(request: &CaptureRequest) -> u64 {
    test_pattern_open(request, TestPattern::MovingBand)
}

/// The vtable of a built-in test pattern.
#[must_use]
pub const fn test_pattern_vtable(kind: TestPattern) -> CaptureVTable {
    CaptureVTable {
        open: match kind {
            TestPattern::ColourCycle => colour_cycle_open,
            TestPattern::MovingBand => moving_band_open,
        },
        read: test_pattern_read,
        close: test_pattern_close,
        reconfigure: None,
    }
}

#[cfg(test)]
#[allow(clippy::too_many_lines)] // table-driven cases; splitting them hides the case list
mod autotest_generated {
    use std::{
        collections::{BTreeMap, HashMap},
        panic::{catch_unwind, AssertUnwindSafe},
        rc::Rc,
        sync::{Arc, Mutex, PoisonError},
    };

    use azul_core::{
        dom::{Dom, DomId, DomNodeId, NodeId, NodeType},
        geom::{LogicalRect, OptionLogicalPosition},
        gl::{GenericGlContext, OptionGlContextPtr, GLvoid},
        hit_test::ScrollPosition,
        refany::OptionRefAny,
        resources::{DecodedImage, RendererResources},
        styled_dom::{NodeHierarchyItemId, StyledDom},
        window::{MonitorVec, RawWindowHandle, RendererType},
    };
    use azul_css::system::SystemStyle;
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        solver3::{display_list::DisplayList, layout_tree::LayoutTree},
        window::{DomLayoutResult, LayoutWindow},
        window_state::FullWindowState,
    };

    // ------------------------------------------------------------------
    // Fake GL drivers
    //
    // Every field of `GenericGlContext` is a `*mut c_void` entry point, and
    // gl-context-loader null-checks each one before transmuting + calling it
    // (returning a default instead). So an all-zero context is a SAFE no-op
    // "driver never loaded" GL, and a context with only the three entry points
    // this module actually uses filled in is a safe *recording* driver: we can
    // observe exactly which GL calls `upload_rgba` / `present_frame` emit, with
    // which arguments, entirely off-GPU.
    // ------------------------------------------------------------------

    /// The texture name the recording driver hands out from `glGenTextures`.
    const RECORDED_TEXTURE_ID: u32 = 42;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum GlCall {
        GenTextures {
            n: i32,
        },
        BindTexture {
            target: u32,
            texture: u32,
        },
        TexImage2d {
            target: u32,
            level: i32,
            internal_format: i32,
            width: i32,
            height: i32,
            border: i32,
            format: u32,
            ty: u32,
            /// `false` = the `NULL` pixel pointer `Texture::allocate_rgba8` uses,
            /// `true` = a real pixel upload (what `upload_rgba` does).
            has_pixels: bool,
        },
    }

    static GL_LOG: Mutex<Vec<GlCall>> = Mutex::new(Vec::new());
    /// Serializes the tests that use the (process-global) recording driver.
    static GL_SERIAL: Mutex<()> = Mutex::new(());

    fn gl_log_push(call: GlCall) {
        GL_LOG
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(call);
    }

    extern "system" fn rec_gen_textures(n: i32, out: *mut u32) {
        gl_log_push(GlCall::GenTextures { n });
        // The caller (gl-context-loader) always passes a `Vec<GLuint>` of len `n`.
        for i in 0..n.max(0) {
            // SAFETY: `out` addresses `n` writable `GLuint`s (a `vec![0; n]`).
            unsafe { out.add(i as usize).write(RECORDED_TEXTURE_ID + i as u32) };
        }
    }

    extern "system" fn rec_bind_texture(target: u32, texture: u32) {
        gl_log_push(GlCall::BindTexture { target, texture });
    }

    #[allow(clippy::too_many_arguments)] // must mirror glTexImage2D exactly
    extern "system" fn rec_tex_image_2d(
        target: u32,
        level: i32,
        internal_format: i32,
        width: i32,
        height: i32,
        border: i32,
        format: u32,
        ty: u32,
        pixels: *const GLvoid,
    ) {
        gl_log_push(GlCall::TexImage2d {
            target,
            level,
            internal_format,
            width,
            height,
            border,
            format,
            ty,
            has_pixels: !pixels.is_null(),
        });
    }

    /// A GL context whose entry points are all `NULL` (driver never loaded).
    fn null_gl_context() -> GlContextPtr {
        // SAFETY: every field of `GenericGlContext` is a raw pointer, for which
        // the all-zero (NULL) bit pattern is valid.
        let ctx: GenericGlContext = unsafe { core::mem::zeroed() };
        GlContextPtr::new(RendererType::Software, Rc::new(ctx))
    }

    /// A GL context that records the calls this module makes (and nothing else:
    /// `glTexParameteri` / `glGetIntegerv` / `glDeleteTextures` stay NULL, i.e.
    /// safe no-ops).
    fn recording_gl_context() -> GlContextPtr {
        // SAFETY: as above — NULL is a valid value for every field; the three we
        // overwrite get fn pointers with exactly the signatures gl-context-loader
        // transmutes them back to.
        let mut ctx: GenericGlContext = unsafe { core::mem::zeroed() };
        ctx.glGenTextures = rec_gen_textures as *const () as *mut azul_core::gl::c_void;
        ctx.glBindTexture = rec_bind_texture as *const () as *mut azul_core::gl::c_void;
        ctx.glTexImage2D = rec_tex_image_2d as *const () as *mut azul_core::gl::c_void;
        GlContextPtr::new(RendererType::Software, Rc::new(ctx))
    }

    /// Runs `f` against the recording driver and returns the GL calls it made.
    fn with_recorded_gl<R>(f: impl FnOnce(GlContextPtr) -> R) -> (R, Vec<GlCall>) {
        let _serial = GL_SERIAL.lock().unwrap_or_else(PoisonError::into_inner);
        GL_LOG
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        let out = f(recording_gl_context());
        let log = GL_LOG
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        (out, log)
    }

    // ------------------------------------------------------------------
    // CallbackInfo harness (mirrors the other widget test modules)
    // ------------------------------------------------------------------

    /// A `DomLayoutResult` with an *empty* layout tree: the code under test only
    /// walks `styled_dom.node_data`, so no real layout (and no font) is needed.
    fn layout_result(styled_dom: StyledDom) -> DomLayoutResult {
        DomLayoutResult {
            styled_dom,
            layout_tree: LayoutTree {
                nodes: Vec::new(),
                warm: Vec::new(),
                cold: Vec::new(),
                root: 0,
                dom_to_layout: BTreeMap::new(),
                children_arena: Vec::new(),
                children_offsets: Vec::new(),
                subtree_needs_intrinsic: Vec::new(),
            },
            calculated_positions: Vec::new(),
            viewport: LogicalRect::zero(),
            display_list: Arc::new(DisplayList::default()),
            scroll_ids: HashMap::new(),
            scroll_id_to_node_id: HashMap::new(),
        }
    }

    /// Invokes `f` with a `CallbackInfo` over a window holding `styled` (or no
    /// layout results at all, when `styled` is `None`) and the given GL context.
    /// Returns `f`'s value plus every `CallbackChange` the callback recorded.
    fn with_callback_info<R>(
        styled: Option<StyledDom>,
        gl_context: OptionGlContextPtr,
        f: impl FnOnce(&mut CallbackInfo) -> R,
    ) -> (R, Vec<CallbackChange>) {
        let mut layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        if let Some(sd) = styled {
            layout_window
                .layout_results
                .insert(DomId::ROOT_ID, layout_result(sd));
        }

        let renderer_resources = RendererResources::default();
        let previous_window_state: Option<FullWindowState> = None;
        let current_window_state = FullWindowState::default();
        let scroll_states: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> =
            BTreeMap::new();
        let window_handle = RawWindowHandle::Unsupported;
        let system_callbacks = ExternalSystemCallbacks::rust_internal();

        let ref_data = CallbackInfoRefData {
            layout_window: &layout_window,
            renderer_resources: &renderer_resources,
            previous_window_state: &previous_window_state,
            current_window_state: &current_window_state,
            gl_context: &gl_context,
            current_scroll_manager: &scroll_states,
            current_window_handle: &window_handle,
            system_callbacks: &system_callbacks,
            system_style: Arc::new(SystemStyle::default()),
            monitors: Arc::new(Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
            ctx: OptionRefAny::None,
        };

        let changes: Arc<Mutex<Vec<CallbackChange>>> = Arc::new(Mutex::new(Vec::new()));

        let mut info = CallbackInfo::new(
            &ref_data,
            &changes,
            DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(0))),
            },
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );

        let out = f(&mut info);
        let recorded = core::mem::take(&mut *changes.lock().expect("change log poisoned"));
        (out, recorded)
    }

    // ------------------------------------------------------------------
    // Fixtures
    // ------------------------------------------------------------------

    /// The dataset type a capture widget stores on its node.
    #[derive(Debug, Default)]
    struct CamState {
        _texture_id: Option<u32>,
    }

    /// A *different* dataset type, to prove the node lookup is type-scoped.
    #[derive(Debug, Default)]
    struct OtherState {
        _unused: u8,
    }

    /// A `div`, carrying `ds` as its dataset when there is one.
    fn div_with(ds: Option<RefAny>) -> Dom {
        let d = Dom::create_node(NodeType::Div);
        match ds {
            Some(r) => d.with_dataset(OptionRefAny::Some(r)),
            None => d,
        }
    }

    /// `body(0) -> div(1) -> div(2)`, where a `Some(ds)` gives that div a dataset.
    fn dom_with_datasets(first: Option<RefAny>, second: Option<RefAny>) -> StyledDom {
        let dom = Dom::create_node(NodeType::Body)
            .with_child(div_with(first))
            .with_child(div_with(second));
        let styled = StyledDom::create_from_dom(dom);
        assert_eq!(
            styled.node_hierarchy.as_ref().len(),
            3,
            "fixture must flatten to exactly body + 2 divs"
        );
        styled
    }

    /// A `width` x `height` RGBA8 frame with a deterministic (tightly-packed) ramp.
    /// Only ever called with tiny dimensions — `width * height * 4` is allocated.
    fn frame(width: u32, height: u32) -> VideoFrame {
        let len = (width as usize) * (height as usize) * 4;
        let bytes: Vec<u8> = (0..len).map(|i| (i % 251) as u8).collect();
        VideoFrame::new(width, height, bytes.into())
    }

    /// A frame whose *declared* dimensions need not match its byte count.
    fn frame_raw(width: u32, height: u32, bytes: Vec<u8>) -> VideoFrame {
        VideoFrame::new(width, height, bytes.into())
    }

    /// Every image installed on a node, as `(dom, node index, image, update type)`.
    fn image_installs(
        changes: &[CallbackChange],
    ) -> Vec<(DomId, usize, &ImageRef, UpdateImageType)> {
        changes
            .iter()
            .filter_map(|c| match c {
                CallbackChange::ChangeNodeImage {
                    dom_id,
                    node_id,
                    image,
                    update_type,
                } => Some((*dom_id, node_id.index(), image, *update_type)),
                _ => None,
            })
            .collect()
    }

    /// How many "recomposite, don't relayout" requests the callback made.
    fn recomposites(changes: &[CallbackChange]) -> usize {
        changes
            .iter()
            .filter(|c| matches!(c, CallbackChange::UpdateAllImageCallbacks))
            .count()
    }

    // ==================================================================
    // invoke_on_frame
    // ==================================================================

    /// Payload of the `on_frame` hook: records every frame it is handed.
    #[derive(Debug)]
    struct HookLog {
        seen: Vec<(u32, u32, usize, Option<u8>)>,
        reply: Update,
    }

    extern "C" fn hook_record(mut data: RefAny, _: CallbackInfo, frame: VideoFrame) -> Update {
        let mut reply = Update::DoNothing;
        if let Some(mut log) = data.downcast_mut::<HookLog>() {
            let bytes = frame.bytes.as_ref();
            log.seen
                .push((frame.width, frame.height, bytes.len(), bytes.first().copied()));
            reply = log.reply;
        }
        reply
    }

    /// A hook that writes through the `CallbackInfo` it was handed (by value).
    extern "C" fn hook_recomposite(_: RefAny, mut info: CallbackInfo, _: VideoFrame) -> Update {
        info.update_all_image_callbacks();
        Update::RefreshDomAllWindows
    }

    fn hook(cb: OnVideoFrameCallbackType, data: RefAny) -> OptionOnVideoFrame {
        OptionOnVideoFrame::Some(OnVideoFrame {
            refany: data,
            callback: cb.into(),
        })
    }

    fn hook_seen(data: &mut RefAny) -> Vec<(u32, u32, usize, Option<u8>)> {
        data.downcast_ref::<HookLog>()
            .expect("payload must still be a HookLog")
            .seen
            .clone()
    }

    #[test]
    fn invoke_on_frame_without_a_hook_is_do_nothing_and_touches_nothing() {
        let (update, changes) = with_callback_info(None, OptionGlContextPtr::None, |info| {
            invoke_on_frame(&OptionOnVideoFrame::None, info, &frame(2, 2))
        });
        assert_eq!(
            update,
            Update::DoNothing,
            "an unset on_frame hook must be a no-op"
        );
        assert!(
            changes.is_empty(),
            "an unset hook must not record any change, got {changes:?}"
        );
    }

    #[test]
    fn invoke_on_frame_returns_the_hooks_update_verbatim() {
        for reply in [
            Update::DoNothing,
            Update::RefreshDom,
            Update::RefreshDomAllWindows,
        ] {
            let data = RefAny::new(HookLog {
                seen: Vec::new(),
                reply,
            });
            let h = hook(hook_record, data);
            let (update, _) = with_callback_info(None, OptionGlContextPtr::None, |info| {
                invoke_on_frame(&h, info, &frame(1, 1))
            });
            assert_eq!(
                update, reply,
                "invoke_on_frame must return the user's Update unchanged"
            );
        }
    }

    #[test]
    fn invoke_on_frame_forwards_every_frame_into_the_hooks_shared_refany() {
        let mut data = RefAny::new(HookLog {
            seen: Vec::new(),
            reply: Update::RefreshDom,
        });
        let h = hook(hook_record, data.clone());

        // The hook is handed a *clone* of its RefAny on every invocation — the
        // backreference DI pattern only works if that clone shares the payload.
        with_callback_info(None, OptionGlContextPtr::None, |info| {
            for (w, hgt) in [(1_u32, 1_u32), (2, 3), (4, 4)] {
                invoke_on_frame(&h, info, &frame(w, hgt));
            }
        });

        assert_eq!(
            hook_seen(&mut data),
            vec![
                (1, 1, 4, Some(0)),
                (2, 3, 24, Some(0)),
                (4, 4, 64, Some(0)),
            ],
            "every frame must reach the hook, in order, with its bytes intact"
        );
    }

    #[test]
    fn invoke_on_frame_forwards_degenerate_frames_unvalidated_and_without_panicking() {
        let mut data = RefAny::new(HookLog {
            seen: Vec::new(),
            reply: Update::DoNothing,
        });
        let h = hook(hook_record, data.clone());

        with_callback_info(None, OptionGlContextPtr::None, |info| {
            // 0x0, dimensions that disagree with the byte count, and dimensions
            // whose tight-packing size (w*h*4) overflows usize. `invoke_on_frame`
            // must hand all of them to the hook as-is: it is a pure forwarder and
            // must never multiply the dimensions out.
            invoke_on_frame(&h, info, &frame_raw(0, 0, Vec::new()));
            invoke_on_frame(&h, info, &frame_raw(9, 9, vec![7, 8, 9]));
            invoke_on_frame(&h, info, &frame_raw(u32::MAX, u32::MAX, Vec::new()));
            invoke_on_frame(&h, info, &frame_raw(u32::MAX, 1, vec![255]));
        });

        assert_eq!(
            hook_seen(&mut data),
            vec![
                (0, 0, 0, None),
                (9, 9, 3, Some(7)),
                (u32::MAX, u32::MAX, 0, None),
                (u32::MAX, 1, 1, Some(255)),
            ],
            "invoke_on_frame must forward frames verbatim, without validating them"
        );
    }

    #[test]
    fn invoke_on_frame_hook_writes_through_the_shared_callback_info() {
        // `invoke_on_frame` passes `*info` (CallbackInfo is Copy) — the copy must
        // still write into the *caller's* transaction container.
        let h = hook(hook_recomposite, RefAny::new(OtherState::default()));
        let (update, changes) = with_callback_info(None, OptionGlContextPtr::None, |info| {
            invoke_on_frame(&h, info, &frame(1, 1))
        });

        assert_eq!(update, Update::RefreshDomAllWindows);
        assert_eq!(
            recomposites(&changes),
            1,
            "a change made by the hook must be visible to the widget's writeback"
        );
    }

    // ==================================================================
    // upload_rgba
    // ==================================================================

    #[test]
    fn upload_rgba_forwards_the_texture_id_and_the_rgba8_constants() {
        for id in [0_u32, 1, 7, u32::MAX] {
            let ((), log) = with_recorded_gl(|gl| upload_rgba(&gl, id, &frame(2, 2)));
            assert_eq!(
                log,
                vec![
                    GlCall::BindTexture {
                        target: TEXTURE_2D,
                        texture: id,
                    },
                    GlCall::TexImage2d {
                        target: TEXTURE_2D,
                        level: 0,
                        internal_format: RGBA as i32,
                        width: 2,
                        height: 2,
                        border: 0,
                        format: RGBA,
                        ty: UNSIGNED_BYTE,
                        has_pixels: true,
                    },
                ],
                "upload_rgba must bind exactly texture {id} and upload tightly-packed RGBA8"
            );
        }
    }

    #[test]
    fn upload_rgba_zero_sized_frame_is_forwarded_as_a_0x0_upload() {
        let ((), log) = with_recorded_gl(|gl| upload_rgba(&gl, 3, &frame_raw(0, 0, Vec::new())));
        assert_eq!(
            log,
            vec![
                GlCall::BindTexture {
                    target: TEXTURE_2D,
                    texture: 3,
                },
                GlCall::TexImage2d {
                    target: TEXTURE_2D,
                    level: 0,
                    internal_format: RGBA as i32,
                    width: 0,
                    height: 0,
                    border: 0,
                    format: RGBA,
                    ty: UNSIGNED_BYTE,
                    has_pixels: true,
                },
            ],
            "a 0x0 frame must still be a well-formed (if empty) glTexImage2D, not a panic"
        );
    }

    #[test]
    fn upload_rgba_dimensions_above_i32_max_wrap_to_negative_glsizei() {
        // glTexImage2D takes GLsizei (= i32), so a u32 dimension > i32::MAX is a
        // lossy cast. Assert the *exact* wrapped value: GL then rejects the call
        // with GL_INVALID_VALUE (the frame is dropped) — the cast must never be a
        // debug-mode overflow panic or UB.
        let cases: [(u32, u32, i32, i32); 4] = [
            (i32::MAX as u32, 1, i32::MAX, 1),
            (i32::MAX as u32 + 1, 1, i32::MIN, 1),
            (u32::MAX, u32::MAX, -1, -1),
            (u32::MAX - 1, 2, -2, 2),
        ];

        for (w, h, want_w, want_h) in cases {
            // Empty byte buffer: the huge dimensions must never be multiplied out
            // (that would be a several-exabyte allocation), only cast.
            let ((), log) = with_recorded_gl(|gl| upload_rgba(&gl, 1, &frame_raw(w, h, Vec::new())));
            let tex = log
                .iter()
                .find_map(|c| match c {
                    GlCall::TexImage2d { width, height, .. } => Some((*width, *height)),
                    _ => None,
                })
                .expect("upload_rgba must always call glTexImage2D");
            assert_eq!(
                tex,
                (want_w, want_h),
                "{w}x{h} must cast to GLsizei {want_w}x{want_h}"
            );
        }
    }

    #[test]
    fn upload_rgba_against_an_unloaded_driver_is_a_silent_no_op() {
        // is_gl_usable() == false (all entry points NULL): the loader must swallow
        // every call rather than jumping through a NULL function pointer.
        let gl = null_gl_context();
        upload_rgba(&gl, 0, &frame(2, 2));
        upload_rgba(&gl, u32::MAX, &frame_raw(u32::MAX, u32::MAX, Vec::new()));
        upload_rgba(&gl, 1, &frame_raw(0, 0, Vec::new()));
    }

    // ==================================================================
    // present_frame — CPU (no GL context)
    // ==================================================================

    #[test]
    fn present_frame_without_gl_installs_a_raw_image_on_the_dataset_node() {
        let ds = RefAny::new(CamState::default());
        let styled = dom_with_datasets(Some(ds.clone()), None);

        let (id, changes) = with_callback_info(Some(styled), OptionGlContextPtr::None, |info| {
            present_frame(info, ds.clone(), None, &frame(4, 4))
        });

        // The CPU path never allocates a GL texture, so it must hand back the id it
        // was given (None) rather than inventing one.
        assert_eq!(id, None, "the cpurender path must not invent a texture id");

        let installs = image_installs(&changes);
        assert_eq!(installs.len(), 1, "exactly one image install per frame");
        let (dom_id, node_idx, image, update_type) = installs[0];
        assert_eq!(dom_id, DomId::ROOT_ID);
        assert_eq!(node_idx, 1, "the image must land on the dataset's node");
        assert_eq!(update_type, UpdateImageType::Content);
        match image.get_data() {
            DecodedImage::Raw((descriptor, _)) => {
                assert_eq!(
                    (descriptor.width, descriptor.height),
                    (4, 4),
                    "the installed image must keep the frame's dimensions"
                );
            }
            other => panic!("cpurender must install a raw image, got {other:?}"),
        }
        assert_eq!(
            recomposites(&changes),
            0,
            "the CPU path swaps the node's image instead of recompositing a texture"
        );
    }

    #[test]
    fn present_frame_without_gl_returns_the_current_id_verbatim() {
        for current in [None, Some(0_u32), Some(1), Some(u32::MAX)] {
            let ds = RefAny::new(CamState::default());
            let styled = dom_with_datasets(Some(ds.clone()), None);
            let (id, changes) =
                with_callback_info(Some(styled), OptionGlContextPtr::None, |info| {
                    present_frame(info, ds.clone(), current, &frame(2, 2))
                });
            assert_eq!(
                id, current,
                "the cpurender path must round-trip current_id ({current:?}) untouched"
            );
            assert_eq!(
                image_installs(&changes).len(),
                1,
                "the CPU path re-installs the image on *every* frame"
            );
        }
    }

    #[test]
    fn present_frame_without_gl_and_without_a_matching_dataset_installs_nothing() {
        // Node carries `OtherState`, the widget looks for `CamState`.
        let node_ds = RefAny::new(OtherState::default());
        let styled = dom_with_datasets(Some(node_ds), None);
        let search = RefAny::new(CamState::default());

        let (id, changes) = with_callback_info(Some(styled), OptionGlContextPtr::None, |info| {
            present_frame(info, search.clone(), Some(9), &frame(2, 2))
        });

        assert_eq!(id, Some(9), "a failed node lookup must not lose the id");
        assert!(
            changes.is_empty(),
            "no node owns the dataset, so nothing may be installed: {changes:?}"
        );
    }

    #[test]
    fn present_frame_without_gl_and_without_any_layout_result_installs_nothing() {
        let ds = RefAny::new(CamState::default());
        let (id, changes) = with_callback_info(None, OptionGlContextPtr::None, |info| {
            present_frame(info, ds.clone(), Some(3), &frame(2, 2))
        });
        assert_eq!(id, Some(3));
        assert!(
            changes.is_empty(),
            "an empty window must not be written to: {changes:?}"
        );
    }

    #[test]
    fn present_frame_without_gl_rejects_a_frame_whose_byte_count_disagrees_with_its_size() {
        // A backend that lies about the frame size (or a short read) must not be
        // able to install a bogus image — RawImage validates len == w*h*4.
        for (w, h, bytes) in [
            (4_u32, 4_u32, vec![0_u8; 3]),        // far too short
            (4, 4, vec![0_u8; 63]),               // one byte short
            (4, 4, vec![0_u8; 65]),               // one byte long
            (2, 2, Vec::new()),                   // no pixels at all
        ] {
            let ds = RefAny::new(CamState::default());
            let styled = dom_with_datasets(Some(ds.clone()), None);
            let (id, changes) =
                with_callback_info(Some(styled), OptionGlContextPtr::None, |info| {
                    present_frame(info, ds.clone(), Some(5), &frame_raw(w, h, bytes.clone()))
                });

            assert_eq!(id, Some(5), "a rejected frame must not disturb the id");
            assert!(
                changes.is_empty(),
                "a {w}x{h} frame with {} bytes must be rejected, not installed: {changes:?}",
                bytes.len()
            );
        }
    }

    #[test]
    fn present_frame_without_gl_installs_a_degenerate_image_for_a_0x0_frame() {
        // 0*0*4 == 0 == len(bytes), so a 0x0 frame passes RawImage's length check
        // and IS installed (as a 0x0 image). Pin the behaviour: it must at least
        // not panic and must not corrupt the returned id.
        let ds = RefAny::new(CamState::default());
        let styled = dom_with_datasets(Some(ds.clone()), None);
        let (id, changes) = with_callback_info(Some(styled), OptionGlContextPtr::None, |info| {
            present_frame(info, ds.clone(), Some(2), &frame_raw(0, 0, Vec::new()))
        });

        assert_eq!(id, Some(2));
        let installs = image_installs(&changes);
        assert_eq!(installs.len(), 1);
        match installs[0].2.get_data() {
            DecodedImage::Raw((descriptor, _)) => {
                assert_eq!((descriptor.width, descriptor.height), (0, 0));
            }
            other => panic!("expected a raw image, got {other:?}"),
        }
    }

    #[test]
    fn present_frame_without_gl_survives_dimensions_whose_byte_count_overflows_usize() {
        // ADVERSARIAL: a backend reporting 2^31 x 2^31 makes the CPU path compute
        // `width * height * 4` in usize inside `RawImage::into_loaded_image_source`
        // -> 2^64, which overflows.
        //
        // Today that is an arithmetic-overflow PANIC in a debug build (and a
        // silent wrap to 0 in release, which then *accepts* the empty byte buffer
        // as a valid 2^31 x 2^31 image). Neither is a graceful rejection — see the
        // autotest report. What must hold in *both* modes is the one invariant we
        // can still assert: the caller's texture id is never corrupted, and no GL
        // work is attempted.
        let ds = RefAny::new(CamState::default());
        let styled = dom_with_datasets(Some(ds.clone()), None);

        let (result, _changes) = with_callback_info(Some(styled), OptionGlContextPtr::None, |info| {
            catch_unwind(AssertUnwindSafe(|| {
                present_frame(
                    info,
                    ds.clone(),
                    Some(11),
                    &frame_raw(1_u32 << 31, 1_u32 << 31, Vec::new()),
                )
            }))
        });

        match result {
            Ok(id) => assert_eq!(
                id,
                Some(11),
                "the cpurender path must always hand back current_id"
            ),
            Err(_) => eprintln!(
                "NOTE: present_frame panicked (usize overflow of width*height*4) for a \
                 2^31 x 2^31 frame — a malformed capture backend can take the process down"
            ),
        }
    }

    #[test]
    fn present_frame_installs_exactly_one_image_when_two_nodes_share_a_dataset_type() {
        // Two capture widgets of the same state type in one DOM: the lookup scores
        // candidates by RefAny instance id, so *which* node wins is an internal
        // detail — but it must pick exactly ONE, and it must be a node that
        // actually owns a dataset (never the body at index 0, never both).
        let styled = dom_with_datasets(
            Some(RefAny::new(CamState::default())),
            Some(RefAny::new(CamState::default())),
        );
        let search = RefAny::new(CamState::default());

        let (id, changes) = with_callback_info(Some(styled), OptionGlContextPtr::None, |info| {
            present_frame(info, search.clone(), Some(4), &frame(2, 2))
        });

        assert_eq!(id, Some(4));
        let installs = image_installs(&changes);
        assert_eq!(
            installs.len(),
            1,
            "a frame must never be installed on two nodes at once: {changes:?}"
        );
        assert!(
            installs[0].1 == 1 || installs[0].1 == 2,
            "the image must land on a node that owns a dataset, not on node {}",
            installs[0].1
        );
    }

    #[test]
    fn present_frame_matches_datasets_by_type_id_not_by_identity() {
        // FOOTGUN: the lookup compares *type ids*, so a completely unrelated
        // RefAny of the same type finds the node. Two capture widgets sharing a
        // state type would therefore fight over one node.
        let node_ds = RefAny::new(CamState::default());
        let styled = dom_with_datasets(Some(node_ds), None);

        let unrelated = RefAny::new(CamState::default()); // a different allocation
        let (id, changes) = with_callback_info(Some(styled), OptionGlContextPtr::None, |info| {
            present_frame(info, unrelated.clone(), None, &frame(2, 2))
        });

        assert_eq!(id, None);
        assert_eq!(
            image_installs(&changes).len(),
            1,
            "an unrelated RefAny of the same type still resolves to the node"
        );
    }

    // ==================================================================
    // present_frame — with a GL context PRESENT (the trap case)
    //
    // A CPU-rendered window can still EXPOSE a GL context. The old code
    // branched on it inside the WIDGET and sent texture-only updates the CPU
    // rasterizer never saw (frozen camera tiles). The contract now: ONE path,
    // no GL calls, always a raw-image ChangeNodeImage through the chokepoint.
    // ==================================================================

    #[test]
    fn present_frame_with_gl_still_installs_a_raw_image_and_touches_no_gl() {
        let ds = RefAny::new(CamState::default());
        let styled = dom_with_datasets(Some(ds.clone()), None);

        let ((id, changes), log) = with_recorded_gl(|gl| {
            with_callback_info(Some(styled), OptionGlContextPtr::Some(gl), |info| {
                present_frame(info, ds.clone(), None, &frame(4, 4))
            })
        });

        assert_eq!(id, None, "current_id passes through unchanged (no texture pool)");
        assert!(
            log.is_empty(),
            "the widget must not branch on the GL context — no GL call is ever made: {log:?}"
        );

        let installs = image_installs(&changes);
        assert_eq!(installs.len(), 1, "exactly one ChangeNodeImage per frame");
        assert_eq!(installs[0].1, 1);
        assert_eq!(installs[0].3, UpdateImageType::Content);
        match installs[0].2.get_data() {
            DecodedImage::Raw((descriptor, _)) => {
                assert_eq!(
                    (descriptor.width, descriptor.height),
                    (4, 4),
                    "the installed raw image must be sized like the frame"
                );
            }
            other => panic!("a RAW image must be installed on every backend, got {other:?}"),
        }
        assert_eq!(
            recomposites(&changes),
            0,
            "no texture-only recomposite: the chokepoint's paint tier drives the repaint"
        );
    }

    #[test]
    fn present_frame_with_gl_steady_state_reinstalls_the_frame_not_a_texture() {
        // Re-installing per frame is CORRECT now: the chokepoint patches the
        // display list in place (no rebuild), and the ImageRef identity change
        // is exactly what makes the CPU diff damage the tile.
        let ds = RefAny::new(CamState::default());
        let styled = dom_with_datasets(Some(ds.clone()), None);

        let ((id, changes), log) = with_recorded_gl(|gl| {
            with_callback_info(Some(styled), OptionGlContextPtr::Some(gl), |info| {
                present_frame(info, ds.clone(), Some(RECORDED_TEXTURE_ID), &frame(4, 4))
            })
        });

        assert_eq!(
            id,
            Some(RECORDED_TEXTURE_ID),
            "a stored id must survive the writeback unchanged"
        );
        assert!(log.is_empty(), "steady state makes no GL calls either: {log:?}");
        assert_eq!(image_installs(&changes).len(), 1);
        assert_eq!(recomposites(&changes), 0);
    }

    #[test]
    fn present_frame_with_gl_round_trips_extreme_texture_ids() {
        for current in [Some(0_u32), Some(u32::MAX)] {
            let ds = RefAny::new(CamState::default());
            let styled = dom_with_datasets(Some(ds.clone()), None);

            let ((id, changes), log) = with_recorded_gl(|gl| {
                with_callback_info(Some(styled), OptionGlContextPtr::Some(gl), |info| {
                    present_frame(info, ds.clone(), current, &frame(1, 1))
                })
            });

            assert_eq!(
                id, current,
                "a stored texture id must survive the writeback unchanged"
            );
            assert!(log.is_empty(), "no GL call for id {current:?}: {log:?}");
            assert_eq!(image_installs(&changes).len(), 1);
        }
    }

    #[test]
    fn present_frame_with_gl_without_a_matching_node_installs_nothing() {
        // The node lookup fails (no dataset of that type): nothing is
        // installed, nothing is allocated, and the id still passes through.
        let styled = dom_with_datasets(Some(RefAny::new(OtherState::default())), None);
        let search = RefAny::new(CamState::default());

        let ((id, changes), log) = with_recorded_gl(|gl| {
            with_callback_info(Some(styled), OptionGlContextPtr::Some(gl), |info| {
                present_frame(info, search.clone(), None, &frame(2, 2))
            })
        });

        assert_eq!(id, None);
        assert!(
            changes.is_empty(),
            "nothing may be installed when no node owns the dataset: {changes:?}"
        );
        assert!(log.is_empty(), "and no GL resource may leak: {log:?}");
    }

    // ==================================================================
    // One capture, many consumers: the policy functions
    // ==================================================================

    fn targets(preview: Option<(u32, u32)>, consumers: &[(u32, u32, u32)], wants_source: bool) -> CaptureTargets {
        CaptureTargets {
            preview,
            consumers: consumers.iter().map(|&(id, w, h)| FrameConsumer::new(id, w, h)).collect(),
            wants_source,
        }
    }

    #[test]
    fn the_required_capture_size_covers_the_preview_every_consumer_and_the_floor() {
        // "client Bob wants 500x200, the local preview is 100x200" -> 500x200
        let t = targets(Some((100, 200)), &[(7, 500, 200)], false);
        assert_eq!(required_capture_size(None, &t, (640, 480)), (500, 200));
        // an explicit config size is a floor the capture never drops below
        assert_eq!(required_capture_size(Some((640, 480)), &t, (1, 1)), (640, 480));
        // only a small preview -> capture SMALL (not the 1080p default)
        let t = targets(Some((300, 200)), &[], false);
        assert_eq!(required_capture_size(None, &t, (1920, 1080)), (300, 200));
        // nothing known yet -> the fallback
        assert_eq!(required_capture_size(None, &CaptureTargets::default(), (640, 480)), (640, 480));
    }

    #[test]
    fn needs_reopen_follows_quality_and_cost_but_never_loops_on_a_preset_snap() {
        // Steady state: a 300x200 tile fed by the 640x480 minimum preset.
        // The request is as small as it gets — this must NOT reopen forever.
        assert!(!needs_reopen((300, 200), (640, 480), (300, 200)));
        // A consumer that fits inside what is delivered needs nothing.
        assert!(!needs_reopen((300, 200), (640, 480), (500, 200)));
        // More than delivered AND more than requested -> reopen (quality).
        assert!(needs_reopen((300, 200), (640, 480), (800, 200)));
        assert!(needs_reopen((300, 200), (640, 480), (300, 600)));
        // Far less than requested on BOTH axes -> reopen (cost).
        assert!(needs_reopen((1280, 720), (1280, 720), (300, 200)));
        // A modest shrink is hysteresis, not a reopen.
        assert!(!needs_reopen((640, 480), (640, 480), (400, 300)));
        assert!(!needs_reopen((640, 480), (640, 480), (320, 300)), "one axis halved is not enough");
    }

    #[test]
    fn the_preview_is_cut_only_when_its_size_differs_from_the_captured_frame() {
        let same = targets(Some((640, 480)), &[], false);
        assert_eq!(preview_cut_size(&same, (640, 480)), None, "same size = show the source, no copy");
        let smaller = targets(Some((320, 240)), &[], false);
        assert_eq!(preview_cut_size(&smaller, (640, 480)), Some((320, 240)));
        assert_eq!(preview_cut_size(&targets(Some((0, 240)), &[], false), (640, 480)), None);
        assert_eq!(preview_cut_size(&targets(None, &[], false), (640, 480)), None);
    }

    // ==================================================================
    // The shared capture loop over a FAKE backend (no device, no display):
    // the covering size reaches `open`, one frame is fanned out per
    // consumer, back-pressure drops frames, a bigger consumer reopens.
    // ==================================================================

    use std::sync::mpsc::{channel, Receiver, Sender};

    use azul_core::task::{
        ThreadReceiverDestructorCallback, ThreadReceiverInner, ThreadRecvCallback,
    };

    use crate::thread::{ThreadSendCallback, ThreadSenderDestructorCallback, ThreadSenderInner};

    /// The fake backend's diary: every `open` request size, the close count,
    /// and how many frames to deliver before `Ended`. One loop test at a time
    /// (`LOOP_GATE`), because the vtable fns are plain fn pointers with
    /// nowhere else to keep state.
    struct FakeBackend {
        opens: Vec<(u32, u32)>,
        closes: u32,
        frames_left: u32,
        /// Sent into the worker's control channel by the Nth read (1-based).
        inject_on_read: Option<(u32, CaptureTargets)>,
        reads: u32,
        control_tx: Option<Sender<ThreadSendMsg>>,
    }
    static FAKE: Mutex<Option<FakeBackend>> = Mutex::new(None);
    static LOOP_GATE: Mutex<()> = Mutex::new(());

    fn fake_open(r: &CaptureRequest) -> u64 {
        let mut g = FAKE.lock().unwrap_or_else(PoisonError::into_inner);
        let fake = g.as_mut().expect("fake backend installed");
        fake.opens.push((r.width, r.height));
        u64::from(fake.opens.len() as u32) // never 0
    }
    fn fake_read(handle: u64, out: &mut Vec<u8>) -> CaptureRead {
        let mut g = FAKE.lock().unwrap_or_else(PoisonError::into_inner);
        let fake = g.as_mut().expect("fake backend installed");
        fake.reads += 1;
        if let Some((n, t)) = fake.inject_on_read.as_ref() {
            if *n == fake.reads {
                if let Some(tx) = fake.control_tx.as_ref() {
                    drop(tx.send(ThreadSendMsg::Custom(RefAny::new(t.clone()))));
                }
            }
        }
        if fake.frames_left == 0 {
            return CaptureRead::Ended;
        }
        fake.frames_left -= 1;
        // The backend "snaps" to what was last opened: deliver that size,
        // solid blue.
        let (w, h) = fake.opens.last().copied().unwrap_or((1, 1));
        let _ = handle;
        out.clear();
        out.extend((0..w * h).flat_map(|_| [10u8, 20, 200, 255]));
        CaptureRead::Frame { width: w, height: h }
    }
    fn fake_close(_handle: u64) {
        let mut g = FAKE.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(fake) = g.as_mut() {
            fake.closes += 1;
        }
    }
    const FAKE_VTABLE: CaptureVTable = CaptureVTable {
        open: fake_open,
        read: fake_read,
        close: fake_close,
        reconfigure: None,
    };

    extern "C" fn loop_writeback(_: RefAny, _: RefAny, _: CallbackInfo) -> Update {
        Update::DoNothing
    }

    // Real channels behind the FFI sender/receiver (the framework's own
    // default callbacks are private; these are their bodies).
    extern "C" fn chan_send(sender: *const core::ffi::c_void, msg: ThreadReceiveMsg) -> bool {
        unsafe { &*sender.cast::<Sender<ThreadReceiveMsg>>() }
            .send(msg)
            .is_ok()
    }
    extern "C" fn chan_recv(receiver: *const core::ffi::c_void) -> OptionThreadSendMsg {
        unsafe { &*receiver.cast::<Receiver<ThreadSendMsg>>() }
            .try_recv()
            .ok()
            .into()
    }
    extern "C" fn sender_noop_drop(_: *mut ThreadSenderInner) {}
    extern "C" fn receiver_noop_drop(_: *mut ThreadReceiverInner) {}

    /// Run the shared loop over the fake backend. Returns every payload the
    /// worker queued (as `(preview, source, consumer cuts)` summaries) plus
    /// the backend diary.
    fn run_fake_loop(
        initial: CaptureTargets,
        frames: u32,
        inject_on_read: Option<(u32, CaptureTargets)>,
        floor: Option<(u32, u32)>,
    ) -> (Vec<(Option<(u32, u32)>, Option<(u32, u32)>, Vec<(u32, u32, u32)>)>, Vec<(u32, u32)>, u32) {
        let _gate = LOOP_GATE.lock().unwrap_or_else(PoisonError::into_inner);
        let (wb_tx, wb_rx) = channel::<ThreadReceiveMsg>();
        let (ctl_tx, ctl_rx) = channel::<ThreadSendMsg>();
        *FAKE.lock().unwrap_or_else(PoisonError::into_inner) = Some(FakeBackend {
            opens: Vec::new(),
            closes: 0,
            frames_left: frames,
            inject_on_read,
            reads: 0,
            control_tx: Some(ctl_tx.clone()),
        });
        let mut sender = ThreadSender::new(ThreadSenderInner {
            ptr: Box::new(wb_tx),
            send_fn: ThreadSendCallback { cb: chan_send },
            destructor: ThreadSenderDestructorCallback { cb: sender_noop_drop },
        });
        let mut receiver = ThreadReceiver::new(ThreadReceiverInner {
            ptr: Box::new(ctl_rx),
            recv_fn: ThreadRecvCallback { cb: chan_recv },
            destructor: ThreadReceiverDestructorCallback { cb: receiver_noop_drop },
        });
        let session = CaptureSession {
            backend: Some(FAKE_VTABLE),
            test_pattern: test_pattern_vtable(TestPattern::ColourCycle),
            request: CaptureRequest::new(0, 0, 0),
            floor,
            fallback: (640, 480),
            writeback: loop_writeback,
            resample: image_scale::resample_rgba,
            reopen_cooldown: std::time::Duration::ZERO,
        };
        run_capture_loop(session, initial, &mut sender, &mut receiver);

        let mut queued = Vec::new();
        while let Ok(ThreadReceiveMsg::WriteBack(mut wb)) = wb_rx.try_recv() {
            let Some(mut c) = wb.refany.downcast_mut::<CapturedFrames>() else {
                panic!("every capture writeback carries CapturedFrames");
            };
            let preview = c.preview.as_ref().map(|f| (f.width, f.height));
            let source = c.source.as_ref().map(|f| (f.width, f.height));
            let cuts = c.consumers.iter().map(|x| (x.consumer.id, x.frame.width, x.frame.height)).collect();
            // Release the latch the way the real writeback does — the NEXT
            // test's loop must not see a stale `true`.
            c.in_flight.store(false, Ordering::Release);
            queued.push((preview, source, cuts));
        }
        let fake = FAKE
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take()
            .expect("fake backend still installed");
        drop(ctl_tx);
        (queued, fake.opens, fake.closes)
    }

    #[test]
    fn the_loop_opens_at_the_covering_size_and_fans_one_frame_out_per_consumer() {
        // Preview 4x3 + Bob 2x3 -> the device is opened at 4x3 (not 640x480),
        // and the ONE frame read becomes: a preview at 4x3 (= the captured
        // size -> shown as the source, no cut), Bob's 2x3 cut, no separate
        // source (no on_frame hook).
        let initial = targets(Some((4, 3)), &[(7, 2, 3)], false);
        let (queued, opens, closes) = run_fake_loop(initial, 1, None, None);
        assert_eq!(opens, vec![(4, 3)], "opened ONCE at the covering size of every consumer");
        assert_eq!(closes, 1, "closed exactly once on Ended");
        assert_eq!(queued.len(), 1);
        let (preview, source, cuts) = &queued[0];
        assert_eq!(*preview, None, "a same-size preview is the source frame itself");
        assert_eq!(*source, Some((4, 3)), "…so the source travels to be shown");
        assert_eq!(cuts, &vec![(7, 2, 3)], "Bob gets his 2x3 from the same frame");
    }

    #[test]
    fn the_loop_cuts_a_smaller_preview_and_keeps_the_source_only_for_the_hook() {
        // A 1280x720 floor (an on_frame hook wants the configured frame) with
        // a 160x90 tile: the device is opened at the floor, the preview is a
        // CUT, and the source travels for the hook.
        let initial = targets(Some((160, 90)), &[], true);
        let (queued, opens, _) = run_fake_loop(initial, 1, None, Some((1280, 720)));
        assert_eq!(opens, vec![(1280, 720)]);
        let (preview, source, cuts) = &queued[0];
        assert_eq!(*preview, Some((160, 90)), "the tile gets a frame at ITS device size");
        assert_eq!(*source, Some((1280, 720)), "the hook gets the frame as captured");
        assert!(cuts.is_empty());

        // Without the hook the source stays on the worker side.
        let initial = targets(Some((160, 90)), &[], false);
        let (queued, _, _) = run_fake_loop(initial, 1, None, Some((1280, 720)));
        let (preview, source, _) = &queued[0];
        assert_eq!(*preview, Some((160, 90)));
        assert_eq!(*source, None, "no hook, a preview cut -> the 3.7 MB source never crosses threads");
    }

    #[test]
    fn back_pressure_keeps_at_most_one_frame_in_flight() {
        // Nothing presents the frames in this test (no main thread), so the
        // latch stays set after the first send: 5 frames are read, ONE is
        // queued, the rest are dropped instead of piling up in the channel.
        let initial = targets(Some((4, 3)), &[], false);
        let (queued, _, closes) = run_fake_loop(initial, 5, None, None);
        assert_eq!(queued.len(), 1, "frames read while one is in flight are dropped, not queued");
        assert_eq!(closes, 1);
    }

    #[test]
    fn a_bigger_consumer_reopens_the_source_at_the_new_covering_size() {
        // Opened for a 4x3 preview; the 2nd read injects "Bob wants 16x12":
        // the next iteration reopens at 16x12 (more than delivered AND more
        // than requested) — exactly one close + open.
        let initial = targets(Some((4, 3)), &[], false);
        let bigger = targets(Some((4, 3)), &[(7, 16, 12)], false);
        let (_, opens, closes) = run_fake_loop(initial, 3, Some((2, bigger)), None);
        assert_eq!(opens, vec![(4, 3), (16, 12)], "reopened once, at the new covering size");
        assert_eq!(closes, 2, "the first source was closed before the reopen, the second on Ended");
    }

    #[test]
    fn a_test_pattern_refuses_a_zero_size_and_cycles_its_colour() {
        let vt = test_pattern_vtable(TestPattern::ColourCycle);
        assert_eq!((vt.open)(&CaptureRequest::new(0, 0, 0)), 0, "a zero-sized pattern is not a frame");
        let h = (vt.open)(&CaptureRequest::new(0, 2, 3));
        assert_ne!(h, 0);
        let mut out = Vec::new();
        assert_eq!((vt.read)(h, &mut out), CaptureRead::Frame { width: 2, height: 3 });
        assert_eq!(out.len(), 2 * 3 * 4);
        assert!(out.chunks_exact(4).all(|px| px == [0, 0, 0, 255]), "tick 0 is opaque black");
        (vt.close)(h);
        let vt = test_pattern_vtable(TestPattern::MovingBand);
        let h = (vt.open)(&CaptureRequest::new(0, 3, 20));
        assert_eq!((vt.read)(h, &mut out), CaptureRead::Frame { width: 3, height: 20 });
        assert_eq!(out.len(), 3 * 20 * 4);
        (vt.close)(h);
    }

    // ==================================================================
    // Backend registries (CaptureVTable / AudioCaptureVTable)
    //
    // The three registries are process-global `OnceLock`s, so each is exercised
    // by exactly ONE test (registering from two tests would race). Each backend
    // fn body is deliberately distinct so the linker cannot fold them onto one
    // address and make the identity assertions vacuous.
    // ==================================================================

    fn open_a(r: &CaptureRequest) -> u64 {
        u64::from(r.index) + u64::from(r.width) * 3 + u64::from(r.height)
    }
    fn read_a(handle: u64, out: &mut Vec<u8>) -> CaptureRead {
        out.clear();
        out.extend_from_slice(&[1, 2, 3, 4]);
        CaptureRead::Frame {
            width: handle as u32,
            height: 1,
        }
    }
    fn close_a(_handle: u64) {}

    fn open_b(r: &CaptureRequest) -> u64 {
        u64::from(r.index) * 7 + u64::from(r.width) + u64::from(r.height) * 11
    }
    fn read_b(_handle: u64, out: &mut Vec<u8>) -> CaptureRead {
        out.push(9);
        CaptureRead::Ended
    }
    fn close_b(_handle: u64) {
        // distinct body: the linker must not fold this onto close_a
        let _ = core::hint::black_box(1_u8);
    }

    fn vtable_a() -> CaptureVTable {
        CaptureVTable {
            open: open_a,
            read: read_a,
            close: close_a,
            reconfigure: None,
        }
    }
    fn vtable_b() -> CaptureVTable {
        CaptureVTable {
            open: open_b,
            read: read_b,
            close: close_b,
            reconfigure: None,
        }
    }

    fn same_vtable(a: CaptureVTable, b: CaptureVTable) -> bool {
        a.open as usize == b.open as usize
            && a.read as usize == b.read as usize
            && a.close as usize == b.close as usize
    }

    #[test]
    fn register_camera_backend_is_first_wins_and_never_overwritten() {
        let before = camera_backend();

        register_camera_backend(vtable_a());
        let first = camera_backend().expect("a backend is registered after the first call");

        // A second registration must be silently ignored, not panic and not swap
        // the vtable out from under a running capture worker.
        register_camera_backend(vtable_b());
        register_camera_backend(vtable_b());
        let after = camera_backend().expect("the backend must still be there");

        assert!(
            same_vtable(first, after),
            "the first registration must win; a later one must not replace it"
        );
        if let Some(pre) = before {
            assert!(
                same_vtable(pre, after),
                "a backend registered before this test must not have been replaced"
            );
        } else {
            assert!(
                same_vtable(vtable_a(), after),
                "camera_backend() must hand back exactly the vtable that was registered"
            );
            // The registered fn pointers must actually be callable through the vtable.
            let r = CaptureRequest::new(1, 2, 3);
            assert_eq!((after.open)(&r), open_a(&r));
            let r = CaptureRequest::new(u32::MAX, u32::MAX, u32::MAX);
            assert_eq!((after.open)(&r), open_a(&r));
            let mut buf = vec![0_u8; 8];
            assert_eq!(
                (after.read)(u64::from(u32::MAX), &mut buf),
                CaptureRead::Frame {
                    width: u32::MAX,
                    height: 1
                }
            );
            assert_eq!(buf, vec![1, 2, 3, 4], "read must be able to resize `out`");
            (after.close)(0);
            (after.close)(u64::MAX);
        }
    }

    #[test]
    fn register_screen_backend_is_independent_of_the_camera_backend() {
        let before = screen_backend();
        register_screen_backend(vtable_b());
        let after = screen_backend().expect("a screen backend is registered");

        if let Some(pre) = before {
            assert!(same_vtable(pre, after), "first registration wins");
        } else {
            assert!(
                same_vtable(vtable_b(), after),
                "the screen registry must hand back the screen vtable"
            );
            // Registering into the screen slot must not have leaked into the
            // camera slot (they are separate OnceLocks).
            if let Some(cam) = camera_backend() {
                assert!(
                    !same_vtable(cam, vtable_b()),
                    "the camera registry must not pick up the screen vtable"
                );
            }
            // `Ended` is the documented end-of-stream signal.
            let mut buf = Vec::new();
            assert_eq!((after.read)(0, &mut buf), CaptureRead::Ended);
        }
    }

    fn mic_open(sample_rate: u32, channels: u16) -> u64 {
        u64::from(sample_rate) * 2 + u64::from(channels)
    }
    fn mic_read(handle: u64, out: &mut Vec<f32>) -> u32 {
        out.clear();
        // NaN / inf / subnormal samples must survive the vtable boundary untouched.
        out.extend_from_slice(&[f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.0]);
        (handle % 3) as u32
    }
    fn mic_close(_handle: u64) {}

    fn mic_open_other(sample_rate: u32, channels: u16) -> u64 {
        u64::from(sample_rate) ^ u64::from(channels)
    }
    fn mic_read_other(_handle: u64, out: &mut Vec<f32>) -> u32 {
        out.push(1.0);
        0
    }
    fn mic_close_other(_handle: u64) {
        let _ = core::hint::black_box(2_u8);
    }

    #[test]
    fn register_mic_backend_is_first_wins_and_passes_f32_samples_through() {
        let before = mic_backend();

        register_mic_backend(AudioCaptureVTable {
            open: mic_open,
            read: mic_read,
            close: mic_close,
        });
        register_mic_backend(AudioCaptureVTable {
            open: mic_open_other,
            read: mic_read_other,
            close: mic_close_other,
        });

        let vt = mic_backend().expect("a mic backend is registered");

        if before.is_none() {
            assert_eq!(
                vt.open as usize, mic_open as usize,
                "the first mic registration must win"
            );

            // Boundary sample rates / channel counts must go through untouched.
            assert_eq!((vt.open)(0, 0), 0);
            assert_eq!((vt.open)(u32::MAX, u16::MAX), mic_open(u32::MAX, u16::MAX));

            let mut samples = Vec::new();
            let frames = (vt.read)(4, &mut samples);
            assert_eq!(frames, 1, "the frame count must be the vtable's, verbatim");
            assert_eq!(samples.len(), 4);
            assert!(samples[0].is_nan(), "a NaN sample must not be normalised");
            assert_eq!(samples[1], f32::INFINITY);
            assert_eq!(samples[2], f32::NEG_INFINITY);
            assert!(
                samples[3] == 0.0 && samples[3].is_sign_negative(),
                "-0.0 must keep its sign bit"
            );

            // `0` is the documented EOF/error return.
            assert_eq!((vt.read)(3, &mut samples), 0);
            (vt.close)(u64::MAX);
        }
    }
}
