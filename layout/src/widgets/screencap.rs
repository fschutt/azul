//! Screen-capture widget — a "dumb widget" identical in architecture to the
//! [`CameraWidget`](super::camera), only the source differs (a display /
//! window). SUPER_PLAN_2 §4 P6, widget pivot.
//!
//! `ScreenCaptureWidget::create(config).dom()` → an `<img>` a background
//! capture thread keeps fed; each frame goes through
//! [`super::capture_common::present_frame`] (GL-texture install-once /
//! re-upload + recomposite). The shared core lives in `capture_common`; this
//! widget is its config + worker. Test-pattern worker (a moving band) stands
//! in for the real ScreenCaptureKit / MediaProjection / PipeWire worker.
//!
//! ONE CAPTURE, MANY CONSUMERS — exactly as the camera widget: the stream is
//! opened at the covering size of the tile (device px, via `NodeResized`)
//! and every registered [`FrameConsumer`]; each frame is cut per consumer
//! off the main thread (`capture_common::run_capture_loop`). The request
//! also carries `config.source` (display index / window id) and
//! `config.fps`, and asks the backend to leave this app's own windows out
//! of the picture (`exclude_self`), so sharing the desktop that shows the
//! sharing app does not loop at 30 fps.

use alloc::vec::Vec;

use azul_core::callbacks::Update;
use azul_core::dom::{ComponentEventFilter, DatasetMergeCallbackType, Dom, EventFilter};
use azul_core::refany::{OptionRefAny, RefAny};
use azul_core::resources::{ImageRef, RawImageFormat};
use azul_core::screencap::{ScreenCaptureConfig, ScreenCaptureSource};
use azul_core::task::{ThreadId, ThreadReceiver, ThreadSendMsg};
use azul_core::video::{FrameConsumer, FrameConsumerVec};

use super::capture_common::{
    frame_resampler, present_captured, preview_size_for_node, run_capture_loop, screen_backend,
    send_capture_targets, test_pattern_vtable, CaptureRequest, CaptureSession, CaptureTargets,
    CapturedFrames, OnConsumerFrame, OnConsumerFrameCallback, OnVideoFrame, OnVideoFrameCallback,
    OptionOnConsumerFrame, OptionOnVideoFrame, TestPattern, REOPEN_COOLDOWN,
};
use crate::callbacks::{Callback, CallbackInfo, CallbackType};
use crate::thread::{Thread, ThreadCallback, ThreadSender};

/// Init data handed to the capture worker thread.
struct ScreencapThreadInit {
    /// The request's source + fps + exclusion (size filled in per reopen).
    request: CaptureRequest,
    /// Who wants frames at mount time.
    targets: CaptureTargets,
    /// The size the capture never drops below (an `on_frame` hook expects
    /// frames "as configured": the default size).
    floor: Option<(u32, u32)>,
}

/// Default capture size for the test pattern (the real backend reports the
/// source's actual size).
const DEFAULT_W: u32 = 1280;
const DEFAULT_H: u32 = 720;

/// Live state for one screencap widget, carried across relayout by
/// [`merge_screencap_state`].
#[derive(Debug)]
pub struct ScreenCaptureWidgetState {
    /// The requested capture configuration (the control POD).
    pub config: ScreenCaptureConfig,
    /// `true` once the capture thread has been started.
    pub started: bool,
    /// The stable external GL texture id once installed.
    pub gl_texture_id: Option<u32>,
    /// Optional user hook invoked with each captured frame (effects / save /
    /// send). Re-set on every fresh build (see [`merge_screencap_state`]).
    pub on_frame: OptionOnVideoFrame,
    /// Every registered consumer (see [`FrameConsumer`]). Re-set on every
    /// fresh build; a change is pushed to the running worker.
    pub consumers: FrameConsumerVec,
    /// Optional hook receiving every consumer's cut of every frame.
    pub on_consumer_frame: OptionOnConsumerFrame,
    /// The capture worker, once started (`NodeResized` messages it).
    pub thread_id: Option<ThreadId>,
    /// The main->worker sender, cloned at mount so the merge callback can
    /// push a changed consumer list without a `CallbackInfo`.
    pub control: Option<std::sync::mpsc::Sender<ThreadSendMsg>>,
    /// The tile's last reported device size.
    pub preview: Option<(u32, u32)>,
}

impl ScreenCaptureWidgetState {
    /// The worker's view of who wants frames, from this state.
    #[must_use]
    pub fn targets(&self) -> CaptureTargets {
        CaptureTargets {
            preview: self.preview,
            consumers: self.consumers.as_ref().to_vec(),
            wants_source: self.on_frame.is_some(),
        }
    }
}

/// A screen-capture widget. `create(config).dom()` yields an `<img>` the
/// capture thread keeps fed.
#[repr(C)]
#[derive(Debug)]
pub struct ScreenCaptureWidget {
    /// What to capture + fps + format.
    pub config: ScreenCaptureConfig,
    /// Optional per-frame user hook (effects / save / send - azul-meet).
    pub on_frame: OptionOnVideoFrame,
    /// Consumers of the captured frames beyond the on-screen tile: each gets
    /// its own cut of every frame at its requested size.
    pub consumers: FrameConsumerVec,
    /// Optional hook receiving each consumer's cut (see `consumers`).
    pub on_consumer_frame: OptionOnConsumerFrame,
}

impl ScreenCaptureWidget {
    /// Create a screencap widget for the given config.
    #[must_use] pub const fn create(config: ScreenCaptureConfig) -> Self {
        Self {
            config,
            on_frame: OptionOnVideoFrame::None,
            consumers: FrameConsumerVec::from_const_slice(&[]),
            on_consumer_frame: OptionOnConsumerFrame::None,
        }
    }

    /// Register a consumer of the captured frames (see
    /// `CameraWidget::add_consumer` - identical semantics: one capture at
    /// the covering size, one cut per consumer, same-id replaces).
    pub fn add_consumer(&mut self, consumer: FrameConsumer) {
        let mut all: Vec<FrameConsumer> = self.consumers.as_ref().to_vec();
        all.retain(|c| c.id != consumer.id);
        all.push(consumer);
        self.consumers = all.into();
    }

    /// Builder form of [`add_consumer`](Self::add_consumer).
    #[must_use]
    pub fn with_consumer(mut self, consumer: FrameConsumer) -> Self {
        self.add_consumer(consumer);
        self
    }

    /// Set the hook that receives every consumer's cut of every captured
    /// frame (route on `frame.consumer.id`).
    pub fn set_on_consumer_frame<C: Into<OnConsumerFrameCallback>>(&mut self, data: RefAny, on_consumer_frame: C) {
        self.on_consumer_frame = Some(OnConsumerFrame {
            refany: data,
            callback: on_consumer_frame.into(),
        })
        .into();
    }

    /// Builder form of [`set_on_consumer_frame`](Self::set_on_consumer_frame).
    #[must_use]
    pub fn with_on_consumer_frame<C: Into<OnConsumerFrameCallback>>(
        mut self,
        data: RefAny,
        on_consumer_frame: C,
    ) -> Self {
        self.set_on_consumer_frame(data, on_consumer_frame);
        self
    }

    /// Set a hook invoked with every captured frame - for live effects, saving
    /// frames into your data model, or sending them over the network
    /// (azul-meet). The backreference DI pattern (see `architecture.md`).
    pub fn set_on_frame<C: Into<OnVideoFrameCallback>>(&mut self, data: RefAny, on_frame: C) {
        self.on_frame = Some(OnVideoFrame {
            refany: data,
            callback: on_frame.into(),
        })
        .into();
    }

    /// Builder form of [`set_on_frame`](Self::set_on_frame).
    #[must_use]
    pub fn with_on_frame<C: Into<OnVideoFrameCallback>>(
        mut self,
        data: RefAny,
        on_frame: C,
    ) -> Self {
        self.set_on_frame(data, on_frame);
        self
    }

    /// Build the widget's DOM: a single `<img>` node, fed by a background
    /// capture thread started on mount.
    #[must_use] pub fn dom(self) -> Dom {
        let state = ScreenCaptureWidgetState {
            config: self.config,
            started: false,
            gl_texture_id: None,
            on_frame: self.on_frame,
            consumers: self.consumers,
            on_consumer_frame: self.on_consumer_frame,
            thread_id: None,
            control: None,
            preview: None,
        };
        let dataset = RefAny::new(state);

        let placeholder = ImageRef::null_image(
            DEFAULT_W as usize,
            DEFAULT_H as usize,
            RawImageFormat::BGRA8,
            b"azul-screencap-placeholder".to_vec(),
        );

        Dom::create_image(placeholder)
            .with_dataset(OptionRefAny::Some(dataset.clone()))
            .with_merge_callback(azul_core::dom::DatasetMergeCallback::from_ptr(merge_screencap_state))
            .with_callback(
                EventFilter::Component(ComponentEventFilter::AfterMount),
                dataset.clone(),
                Callback::from_ptr(screencap_on_after_mount),
            )
            // The tile's device size feeds the stream size + the preview cut.
            .with_callback(
                EventFilter::Component(ComponentEventFilter::NodeResized),
                dataset,
                Callback::from_ptr(screencap_on_resize),
            )
    }
}

/// The backend request for a config: `source` -> display index / window id,
/// `fps`, and this app's own windows excluded (the feedback-loop fix).
const fn capture_request(config: &ScreenCaptureConfig) -> CaptureRequest {
    let (index, window) = match config.source {
        ScreenCaptureSource::PrimaryDisplay => (0, 0),
        ScreenCaptureSource::Display(i) => (i, 0),
        ScreenCaptureSource::Window(id) => (0, id),
    };
    CaptureRequest {
        index,
        window,
        width: 0,
        height: 0,
        fps: config.fps,
        exclude_self: true,
    }
}

/// The size the stream never drops below: the default size when an
/// `on_frame` hook expects frames as before; otherwise the tile and the
/// consumers decide (a 320x180 share tile streams 320x180, not 1280x720).
const fn capture_floor(wants_source: bool) -> Option<(u32, u32)> {
    if wants_source {
        Some((DEFAULT_W, DEFAULT_H))
    } else {
        None
    }
}

/// `AfterMount`: start the background capture thread exactly once, telling
/// it who wants frames (the tile's size if layout already produced one).
extern "C" fn screencap_on_after_mount(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let preview = preview_size_for_node(&info);
    let init = {
        let Some(mut s) = data.downcast_mut::<ScreenCaptureWidgetState>() else {
            return Update::DoNothing;
        };
        if s.started {
            return Update::DoNothing;
        }
        s.started = true;
        s.preview = preview;
        ScreencapThreadInit {
            request: capture_request(&s.config),
            targets: s.targets(),
            floor: capture_floor(s.on_frame.is_some()),
        }
    };
    let tid = ThreadId::unique();
    let thread = Thread::create(RefAny::new(init), data.clone(), ThreadCallback::new(screencap_worker));
    let control = thread.clone_sender();
    info.add_thread(tid, thread);
    if let Some(mut s) = data.downcast_mut::<ScreenCaptureWidgetState>() {
        s.thread_id = Some(tid);
        s.control = control;
    }
    Update::DoNothing
}

/// `NodeResized`: the tile's device size changed — tell the worker (see
/// `camera_on_resize`). A message, not a relayout: returns `DoNothing`.
extern "C" fn screencap_on_resize(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(preview) = preview_size_for_node(&info) else {
        return Update::DoNothing;
    };
    let (tid, targets) = {
        let Some(mut s) = data.downcast_mut::<ScreenCaptureWidgetState>() else {
            return Update::DoNothing;
        };
        if s.preview == Some(preview) {
            return Update::DoNothing;
        }
        s.preview = Some(preview);
        (s.thread_id, s.targets())
    };
    if let Some(tid) = tid {
        send_capture_targets(&info, tid, targets);
    }
    Update::DoNothing
}

/// Background worker: the shared capture loop over the registered screen
/// backend (`ScreenCaptureKit` / `PipeWire` / X11 / DXGI), else the moving-band
/// test pattern — see `capture_common::run_capture_loop`.
extern "C" fn screencap_worker(
    mut init: RefAny,
    mut sender: ThreadSender,
    mut recv: ThreadReceiver,
) {
    let (targets, request, floor) = match init.downcast_ref::<ScreencapThreadInit>() {
        Some(i) => (i.targets.clone(), i.request, i.floor),
        None => (CaptureTargets::default(), CaptureRequest::new(0, 0, 0), None),
    };
    let session = CaptureSession {
        backend: screen_backend(),
        test_pattern: test_pattern_vtable(TestPattern::MovingBand),
        request,
        floor,
        fallback: (DEFAULT_W, DEFAULT_H),
        writeback: screencap_writeback,
        resample: frame_resampler(),
        reopen_cooldown: REOPEN_COOLDOWN,
    };
    run_capture_loop(session, targets, &mut sender, &mut recv);
}

/// Writeback (main thread): run the hooks and put the preview cut (else the
/// captured frame) on the node — `capture_common::present_captured`.
extern "C" fn screencap_writeback(
    mut writeback_data: RefAny,
    mut frame_data: RefAny,
    mut info: CallbackInfo,
) -> Update {
    let (on_frame, on_consumer_frame) = writeback_data
        .downcast_ref::<ScreenCaptureWidgetState>()
        .map_or_else(
            || (OptionOnVideoFrame::None, OptionOnConsumerFrame::None),
            |s| (s.on_frame.clone(), s.on_consumer_frame.clone()),
        );
    let Some(mut captured) = frame_data.downcast_mut::<CapturedFrames>() else {
        return Update::DoNothing;
    };
    present_captured(&mut info, writeback_data.clone(), &on_frame, &on_consumer_frame, &mut captured)
}

/// Carry live state forward across relayout.
extern "C" fn merge_screencap_state(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
    // Return the OLD allocation (the one live capture backends may hold a
    // clone of), adopting config forward — the merge_map_tile_cache rule.
    // Returning new_data re-points the DOM at a fresh allocation; today the
    // frame writeback survives that only because present_frame finds its
    // node by RefAny TYPE id, which also means two widgets of the same type
    // collide. Keeping the persistent allocation makes dataset identity
    // stable so that search can become an identity lookup.
    let merged_into_old = {
        let new_guard = new_data.downcast_ref::<ScreenCaptureWidgetState>();
        let old_guard = old_data.downcast_mut::<ScreenCaptureWidgetState>();
        if let (Some(new_g), Some(mut old_g)) = (new_guard, old_guard) {
            let worker_cares = old_g.consumers != new_g.consumers
                || old_g.on_frame.is_some() != new_g.on_frame.is_some();
            old_g.config = new_g.config;
            old_g.on_frame = new_g.on_frame.clone();
            old_g.consumers = new_g.consumers.clone();
            old_g.on_consumer_frame = new_g.on_consumer_frame.clone();
            if worker_cares {
                let targets = old_g.targets();
                if let Some(snd) = old_g.control.as_ref() {
                    drop(snd.send(ThreadSendMsg::Custom(RefAny::new(targets))));
                }
            }
            true
        } else {
            // Foreign / mismatched payloads (one side is not this widget's
            // state): hand back the NEW payload untouched — there is no
            // persistent allocation to preserve, and returning a
            // wrong-typed old dataset would poison the node.
            false
        }
    };
    if merged_into_old {
        old_data
    } else {
        new_data
    }
}

// ============================================================================
// Generated adversarial tests
// ============================================================================

#[cfg(test)]
#[allow(clippy::too_many_lines, clippy::cast_possible_truncation)]
mod autotest_generated {
    use std::{
        collections::BTreeMap,
        panic::{catch_unwind, AssertUnwindSafe},
        sync::{
            mpsc::{channel, Receiver, Sender},
            Arc, Mutex, PoisonError,
        },
    };

    use azul_core::{
        dom::{DomId, DomNodeId, NodeType},
        geom::OptionLogicalPosition,
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        resources::{DecodedImage, RendererResources},
        styled_dom::NodeHierarchyItemId,
        task::{
            OptionThreadSendMsg, ThreadReceiverDestructorCallback, ThreadReceiverInner,
            ThreadRecvCallback,
        },
        video::VideoFrame,
        window::{MonitorVec, RawWindowHandle},
    };
    use azul_css::system::SystemStyle;
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        thread::{ThreadReceiveMsg, ThreadSendCallback, ThreadSenderDestructorCallback, ThreadSenderInner},
        widgets::capture_common::OnVideoFrameCallbackType,
        window::LayoutWindow,
        window_state::FullWindowState,
    };

    // ------------------------------------------------------------------
    // Config fixtures
    // ------------------------------------------------------------------

    const fn cfg(
        source: ScreenCaptureSource,
        fps: u32,
        output_format: RawImageFormat,
    ) -> ScreenCaptureConfig {
        ScreenCaptureConfig {
            source,
            fps,
            output_format,
        }
    }

    /// Representative + extreme configs: both payload boundaries of each
    /// carrying `ScreenCaptureSource` variant, `fps` at 0 / 1 / `u32::MAX`, and
    /// a format that is deliberately *not* the widget's placeholder format.
    const ALL_CONFIGS: [ScreenCaptureConfig; 8] = [
        cfg(ScreenCaptureSource::PrimaryDisplay, 0, RawImageFormat::BGRA8),
        cfg(
            ScreenCaptureSource::PrimaryDisplay,
            u32::MAX,
            RawImageFormat::RGBA8,
        ),
        cfg(ScreenCaptureSource::Display(0), 1, RawImageFormat::BGRA8),
        cfg(
            ScreenCaptureSource::Display(u32::MAX),
            60,
            RawImageFormat::R8,
        ),
        cfg(ScreenCaptureSource::Window(0), 0, RawImageFormat::BGRA8),
        cfg(
            ScreenCaptureSource::Window(u64::MAX),
            u32::MAX,
            RawImageFormat::R8,
        ),
        cfg(
            ScreenCaptureSource::Window(u32::MAX as u64),
            30,
            RawImageFormat::RGBA8,
        ),
        cfg(ScreenCaptureSource::Display(1), 240, RawImageFormat::BGRA8),
    ];

    const DEFAULT_CFG: ScreenCaptureConfig = ALL_CONFIGS[0];

    /// Compile-time proof that `create` really is a `const fn` (its `const`
    /// qualifier is part of the public API - a non-const `create` would make
    /// this fn fail to compile).
    const fn const_create(config: ScreenCaptureConfig) -> ScreenCaptureWidget {
        ScreenCaptureWidget::create(config)
    }

    // ------------------------------------------------------------------
    // State fixtures
    // ------------------------------------------------------------------

    /// A `ScreenCaptureWidgetState` payload with no `on_frame` hook.
    fn state(
        config: ScreenCaptureConfig,
        started: bool,
        gl_texture_id: Option<u32>,
    ) -> RefAny {
        RefAny::new(ScreenCaptureWidgetState {
            config,
            started,
            gl_texture_id,
            on_frame: OptionOnVideoFrame::None,
            consumers: FrameConsumerVec::from_const_slice(&[]),
            on_consumer_frame: OptionOnConsumerFrame::None,
            thread_id: None,
            control: None,
            preview: None,
        })
    }

    /// The writeback payload the worker queues for one captured `frame`.
    fn captured(frame: VideoFrame) -> RefAny {
        RefAny::new(CapturedFrames {
            source: Some(frame),
            preview: None,
            consumers: Vec::new(),
            in_flight: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
        })
    }

    /// `(config, started, gl_texture_id, has_hook)` of a `ScreenCaptureWidgetState`.
    fn read_state(data: &mut RefAny) -> (ScreenCaptureConfig, bool, Option<u32>, bool) {
        let s = data
            .downcast_ref::<ScreenCaptureWidgetState>()
            .expect("payload must still be a ScreenCaptureWidgetState");
        (
            s.config,
            s.started,
            s.gl_texture_id,
            matches!(s.on_frame, OptionOnVideoFrame::Some(_)),
        )
    }

    /// The placeholder image behind an `<img>` `Dom` root: `(w, h, format, tag)`.
    fn placeholder_of(dom: &Dom) -> (usize, usize, RawImageFormat, Vec<u8>) {
        let NodeType::Image(image) = dom.root.get_node_type() else {
            panic!("ScreenCaptureWidget::dom must build an image node");
        };
        match image.get_data() {
            DecodedImage::NullImage {
                width,
                height,
                format,
                tag,
            } => (*width, *height, *format, tag.clone()),
            _ => panic!("the placeholder must be a NullImage (no decode, no allocation)"),
        }
    }

    // ---- frame hook -------------------------------------------------------

    /// Records every frame a widget's `on_frame` hook is handed, and replies
    /// with a caller-chosen `Update`.
    struct FrameLog {
        seen: Vec<(u32, u32, usize)>,
        reply: Update,
    }

    extern "C" fn record_frame(mut data: RefAny, _: CallbackInfo, frame: VideoFrame) -> Update {
        let mut reply = Update::DoNothing;
        if let Some(mut log) = data.downcast_mut::<FrameLog>() {
            log.seen
                .push((frame.width, frame.height, frame.bytes.as_ref().len()));
            reply = log.reply;
        }
        reply
    }

    extern "C" fn frame_do_nothing(_: RefAny, _: CallbackInfo, _: VideoFrame) -> Update {
        // A distinct body so the linker cannot fold this onto `record_frame` and
        // make the fn-pointer identity assertions vacuous.
        core::hint::black_box(Update::DoNothing)
    }

    fn frame_log(reply: Update) -> RefAny {
        RefAny::new(FrameLog {
            seen: Vec::new(),
            reply,
        })
    }

    /// The frames recorded by a `FrameLog` payload.
    fn logged_frames(data: &mut RefAny) -> Vec<(u32, u32, usize)> {
        data.downcast_ref::<FrameLog>()
            .expect("payload must still be a FrameLog")
            .seen
            .clone()
    }

    /// A `ScreenCaptureWidgetState` whose `on_frame` hook writes into `log`.
    fn state_with_hook(config: ScreenCaptureConfig, log: &RefAny) -> RefAny {
        RefAny::new(ScreenCaptureWidgetState {
            config,
            started: true,
            gl_texture_id: None,
            on_frame: Some(OnVideoFrame {
                refany: log.clone(),
                callback: (record_frame as OnVideoFrameCallbackType).into(),
            })
            .into(),
            consumers: FrameConsumerVec::from_const_slice(&[]),
            on_consumer_frame: OptionOnConsumerFrame::None,
            thread_id: None,
            control: None,
            preview: None,
        })
    }

    /// A tightly-packed RGBA frame (`width * height * 4` bytes).
    fn frame(width: u32, height: u32) -> VideoFrame {
        let px = (width as usize) * (height as usize);
        VideoFrame {
            width,
            height,
            bytes: vec![7u8; px * 4].into(),
        }
    }

    /// A frame whose declared dimensions need not match its byte count.
    fn frame_raw(width: u32, height: u32, bytes: Vec<u8>) -> VideoFrame {
        VideoFrame {
            width,
            height,
            bytes: bytes.into(),
        }
    }

    // ---- CallbackInfo harness --------------------------------------------

    /// Runs `f` against a real `CallbackInfo` over an empty `LayoutWindow` (no GL
    /// context -> the widget's CPU present path). Returns `f`'s value plus every
    /// `CallbackChange` the callback recorded.
    fn with_callback_info<R>(f: impl FnOnce(CallbackInfo) -> R) -> (R, Vec<CallbackChange>) {
        let layout_window =
            LayoutWindow::new(FcFontCache::default()).expect("LayoutWindow::new failed");
        let renderer_resources = RendererResources::default();
        let previous_window_state: Option<FullWindowState> = None;
        let current_window_state = FullWindowState::default();
        let gl_context = OptionGlContextPtr::None;
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

        let info = CallbackInfo::new(
            &ref_data,
            &changes,
            DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::NONE,
            },
            OptionLogicalPosition::None,
            OptionLogicalPosition::None,
        );

        let out = f(info);
        let recorded = core::mem::take(&mut *changes.lock().expect("change log poisoned"));
        (out, recorded)
    }

    // ---- screencap_worker harness ----------------------------------------

    /// One frame `screencap_worker` pushed, summarised so the (multi-megabyte)
    /// pixel buffer never has to be cloned into the log.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct SentFrame {
        width: u32,
        height: u32,
        len: usize,
        /// The first byte of every scanline (that row's test-pattern value).
        row_values: Vec<u8>,
        /// Every pixel of every scanline is `[v, v, v, 255]` for that row's `v`.
        rows_uniform_opaque: bool,
    }

    /// Everything `screencap_worker` managed to send. Guarded by `WORKER_GATE` -
    /// the worker's send callback is a plain C fn pointer, so it has nowhere else
    /// to put its result.
    static WORKER_LOG: Mutex<Vec<SentFrame>> = Mutex::new(Vec::new());
    static WORKER_GATE: Mutex<()> = Mutex::new(());

    /// Records the frame, then reports the send as *failed* - i.e. "the main
    /// thread is gone", the only signal `screencap_worker` has to stop. A worker
    /// that ignored it would hang this test binary forever (and grow ~3.7 MB per
    /// 33 ms while doing so).
    extern "C" fn record_and_stop(_sender: *const core::ffi::c_void, msg: ThreadReceiveMsg) -> bool {
        if let ThreadReceiveMsg::WriteBack(mut wb) = msg {
            if let Some(c) = wb.refany.downcast_ref::<CapturedFrames>() {
                let Some(f) = c.preview.as_ref().or(c.source.as_ref()) else {
                    return false;
                };
                let bytes = f.bytes.as_ref();
                let stride = (f.width as usize) * 4;
                let mut row_values = Vec::new();
                let mut rows_uniform_opaque = true;
                if stride > 0 {
                    for row in bytes.chunks_exact(stride) {
                        let v = row[0];
                        row_values.push(v);
                        if !row.chunks_exact(4).all(|px| px == &[v, v, v, 255][..]) {
                            rows_uniform_opaque = false;
                        }
                    }
                }
                WORKER_LOG
                    .lock()
                    .unwrap_or_else(PoisonError::into_inner)
                    .push(SentFrame {
                        width: f.width,
                        height: f.height,
                        len: bytes.len(),
                        row_values,
                        rows_uniform_opaque,
                    });
            }
        }
        false
    }

    extern "C" fn sender_drop_noop(_: *mut ThreadSenderInner) {}
    extern "C" fn receiver_drop_noop(_: *mut ThreadReceiverInner) {}
    extern "C" fn recv_nothing(_: *const core::ffi::c_void) -> OptionThreadSendMsg {
        OptionThreadSendMsg::None
    }

    /// A `ThreadSender` whose every `send` is recorded and then rejected.
    fn stopped_sender() -> (Receiver<ThreadReceiveMsg>, ThreadSender) {
        let (tx, rx) = channel::<ThreadReceiveMsg>();
        let sender = ThreadSender::new(ThreadSenderInner {
            ptr: Box::new(tx),
            send_fn: ThreadSendCallback { cb: record_and_stop },
            destructor: ThreadSenderDestructorCallback {
                cb: sender_drop_noop,
            },
        });
        (rx, sender)
    }

    /// A `ThreadReceiver` that never delivers anything (the worker ignores it).
    fn silent_receiver() -> (Sender<ThreadSendMsg>, ThreadReceiver) {
        let (tx, rx) = channel::<ThreadSendMsg>();
        let receiver = ThreadReceiver::new(ThreadReceiverInner {
            ptr: Box::new(rx),
            recv_fn: ThreadRecvCallback { cb: recv_nothing },
            destructor: ThreadReceiverDestructorCallback {
                cb: receiver_drop_noop,
            },
        });
        (tx, receiver)
    }

    /// Runs `screencap_worker` with `init` against a sender that rejects the
    /// first frame, and returns everything the worker managed to send.
    ///
    /// `None` when a real platform screen backend is registered in this process
    /// (`capture_common`'s own tests register one into the same process-global
    /// `OnceLock`) - the worker is then not the test pattern these assertions
    /// describe. The check *after* the run is the load-bearing one: a `OnceLock`
    /// is monotone, so "still unset afterwards" proves it was unset throughout.
    fn run_worker(init: RefAny) -> Option<Vec<SentFrame>> {
        let _gate = WORKER_GATE.lock().unwrap_or_else(PoisonError::into_inner);
        if screen_backend().is_some() {
            return None;
        }
        WORKER_LOG
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();

        let (_rx, sender) = stopped_sender();
        let (_tx, receiver) = silent_receiver();
        screencap_worker(init, sender, receiver);

        if screen_backend().is_some() {
            return None; // registered by a parallel test mid-run
        }
        Some(
            WORKER_LOG
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clone(),
        )
    }

    // ------------------------------------------------------------------
    // ScreenCaptureWidget::create
    // ------------------------------------------------------------------

    #[test]
    fn create_stores_the_config_verbatim_and_leaves_the_hook_unset() {
        for config in ALL_CONFIGS {
            let widget = ScreenCaptureWidget::create(config);
            assert_eq!(
                widget.config, config,
                "create must not normalise or clamp the config"
            );
            assert!(
                matches!(widget.on_frame, OptionOnVideoFrame::None),
                "a fresh widget has no frame hook"
            );
        }
    }

    #[test]
    fn create_preserves_the_full_source_payload_width() {
        // A `as u32` anywhere in the widget would collapse a u64 window handle.
        let widget = ScreenCaptureWidget::create(cfg(
            ScreenCaptureSource::Window(u64::MAX),
            0,
            RawImageFormat::BGRA8,
        ));
        match widget.config.source {
            ScreenCaptureSource::Window(h) => assert_eq!(h, u64::MAX),
            other => panic!("expected Window(u64::MAX), got {other:?}"),
        }

        let widget = ScreenCaptureWidget::create(cfg(
            ScreenCaptureSource::Display(u32::MAX),
            u32::MAX,
            RawImageFormat::BGRA8,
        ));
        match widget.config.source {
            ScreenCaptureSource::Display(i) => assert_eq!(i, u32::MAX),
            other => panic!("expected Display(u32::MAX), got {other:?}"),
        }
        assert_eq!(widget.config.fps, u32::MAX, "fps must not be clamped");
    }

    #[test]
    fn create_is_usable_from_a_const_fn() {
        for config in ALL_CONFIGS {
            let widget = const_create(config);
            assert_eq!(widget.config, config);
            assert!(matches!(widget.on_frame, OptionOnVideoFrame::None));
        }
    }

    // ------------------------------------------------------------------
    // ScreenCaptureWidget::set_on_frame / with_on_frame
    // ------------------------------------------------------------------

    #[test]
    fn set_on_frame_installs_the_hook_without_touching_the_config() {
        for config in ALL_CONFIGS {
            let mut widget = ScreenCaptureWidget::create(config);
            widget.set_on_frame(
                frame_log(Update::DoNothing),
                record_frame as OnVideoFrameCallbackType,
            );

            assert_eq!(widget.config, config, "the hook must not alter the config");
            let OptionOnVideoFrame::Some(hook) = &widget.on_frame else {
                panic!("set_on_frame must install a hook");
            };
            assert_eq!(
                hook.callback.cb as usize,
                record_frame as OnVideoFrameCallbackType as usize,
                "the stored fn pointer must be exactly the one that was passed in"
            );
        }
    }

    #[test]
    fn set_on_frame_twice_keeps_only_the_last_hook() {
        let mut widget = ScreenCaptureWidget::create(DEFAULT_CFG);
        widget.set_on_frame(
            RefAny::new(0_usize),
            record_frame as OnVideoFrameCallbackType,
        );
        widget.set_on_frame(
            RefAny::new(1_usize),
            frame_do_nothing as OnVideoFrameCallbackType,
        );

        let OptionOnVideoFrame::Some(hook) = &widget.on_frame else {
            panic!("hook must still be set");
        };
        assert_eq!(
            hook.callback.cb as usize,
            frame_do_nothing as OnVideoFrameCallbackType as usize,
            "the second set_on_frame must replace the first, not stack"
        );
        assert_eq!(
            hook.refany.clone().downcast_ref::<usize>().map(|v| *v),
            Some(1),
            "the replacement's payload must come with it"
        );
    }

    #[test]
    fn set_on_frame_shares_the_users_payload_rather_than_copying_it() {
        // The backreference DI pattern only works if the widget holds a handle to
        // the *same* allocation the caller kept.
        let mut log = frame_log(Update::DoNothing);
        let mut widget = ScreenCaptureWidget::create(DEFAULT_CFG);
        widget.set_on_frame(log.clone(), record_frame as OnVideoFrameCallbackType);

        let OptionOnVideoFrame::Some(hook) = &widget.on_frame else {
            panic!("hook must be set");
        };
        let mut stored = hook.refany.clone();
        {
            let mut inner = stored
                .downcast_mut::<FrameLog>()
                .expect("the widget must hold a FrameLog");
            inner.seen.push((1, 2, 3));
        }
        assert_eq!(
            logged_frames(&mut log),
            vec![(1, 2, 3)],
            "the widget must share the caller's payload, not clone it"
        );
    }

    #[test]
    fn with_on_frame_is_exactly_create_plus_set_on_frame() {
        for config in ALL_CONFIGS {
            let built = ScreenCaptureWidget::create(config).with_on_frame(
                frame_log(Update::RefreshDom),
                record_frame as OnVideoFrameCallbackType,
            );
            let mut manual = ScreenCaptureWidget::create(config);
            manual.set_on_frame(
                frame_log(Update::RefreshDom),
                record_frame as OnVideoFrameCallbackType,
            );

            assert_eq!(built.config, config, "the builder must not touch the config");
            assert_eq!(built.config, manual.config);

            let (OptionOnVideoFrame::Some(a), OptionOnVideoFrame::Some(b)) =
                (&built.on_frame, &manual.on_frame)
            else {
                panic!("both forms must install a hook");
            };
            assert_eq!(a.callback.cb as usize, b.callback.cb as usize);
        }
    }

    // ------------------------------------------------------------------
    // ScreenCaptureWidget::dom
    // ------------------------------------------------------------------

    #[test]
    fn dom_placeholder_is_always_1280x720_bgra8_whatever_the_config_asks_for() {
        // The placeholder is a fixed-size stand-in: the *real* size is whatever
        // the backend reports at runtime. So neither the requested source nor the
        // requested output format may leak into it.
        for config in ALL_CONFIGS {
            let (w, h, format, tag) = placeholder_of(&ScreenCaptureWidget::create(config).dom());
            assert_eq!(
                (w, h),
                (1280, 720),
                "the placeholder size is fixed, not derived from {config:?}"
            );
            assert_eq!(
                format,
                RawImageFormat::BGRA8,
                "output_format is a *capture* request; the placeholder stays BGRA8"
            );
            assert_eq!(tag, b"azul-screencap-placeholder".to_vec());
        }
    }

    #[test]
    fn dom_placeholder_is_a_null_image_that_allocates_no_pixels() {
        // 1280 * 720 * 4 bytes would be ~3.7 MB per widget if the placeholder were
        // a real raw image; a NullImage is only a descriptor.
        let dom = ScreenCaptureWidget::create(DEFAULT_CFG).dom();
        let NodeType::Image(image) = dom.root.get_node_type() else {
            panic!("the widget must build an image node");
        };
        assert!(
            matches!(image.get_data(), DecodedImage::NullImage { .. }),
            "the placeholder must not decode or allocate"
        );
    }

    #[test]
    fn dom_wires_exactly_one_after_mount_callback_a_dataset_and_a_merge_callback() {
        let dom = ScreenCaptureWidget::create(DEFAULT_CFG).dom();

        assert_eq!(dom.children.as_ref().len(), 0, "the widget is a single node");

        let callbacks = dom.root.get_callbacks();
        assert_eq!(
            callbacks.as_ref().len(),
            1,
            "exactly one callback: the AfterMount capture-thread starter"
        );
        assert_eq!(
            callbacks.as_ref()[0].event,
            EventFilter::Component(ComponentEventFilter::AfterMount),
            "the thread must start on AfterMount, not on any input event"
        );
        assert_eq!(
            callbacks.as_ref()[0].callback.cb,
            screencap_on_after_mount as CallbackType as usize,
            "the wired callback must be screencap_on_after_mount"
        );

        let merge = dom
            .root
            .get_merge_callback()
            .expect("state must survive relayout");
        assert_eq!(
            merge.cb as usize,
            merge_screencap_state as DatasetMergeCallbackType as usize,
            "the merge callback must be merge_screencap_state"
        );
    }

    #[test]
    fn dom_seeds_the_dataset_with_the_config_and_a_not_yet_started_thread() {
        for config in ALL_CONFIGS {
            let dom = ScreenCaptureWidget::create(config).dom();
            let mut dataset = dom
                .root
                .get_dataset()
                .cloned()
                .expect("the node must carry its ScreenCaptureWidgetState");
            let (stored, started, texture, has_hook) = read_state(&mut dataset);

            assert_eq!(stored, config, "dom() must not rewrite the config");
            assert!(!started, "the capture thread only starts on AfterMount");
            assert_eq!(texture, None, "no texture exists before the first frame");
            assert!(!has_hook, "no hook was set on this widget");
        }
    }

    #[test]
    fn dom_moves_the_on_frame_hook_into_the_dataset() {
        let dom = ScreenCaptureWidget::create(DEFAULT_CFG)
            .with_on_frame(
                frame_log(Update::DoNothing),
                record_frame as OnVideoFrameCallbackType,
            )
            .dom();

        let mut dataset = dom.root.get_dataset().cloned().expect("dataset");
        let (_, _, _, has_hook) = read_state(&mut dataset);
        assert!(has_hook, "dom() must carry the user hook into the state");
    }

    #[test]
    fn dom_gives_the_after_mount_callback_the_very_same_state_the_node_carries() {
        // `dom()` hands the callback a *clone* of the dataset. If that clone did
        // not share the payload, AfterMount would flip `started` on a copy and the
        // capture thread would be started again on every mount.
        let dom = ScreenCaptureWidget::create(DEFAULT_CFG).dom();
        let mut node_ds = dom.root.get_dataset().cloned().expect("dataset");
        let mut cb_ds = dom.root.get_callbacks().as_ref()[0].refany.clone();

        {
            let mut s = cb_ds
                .downcast_mut::<ScreenCaptureWidgetState>()
                .expect("the callback's payload must be the widget state");
            s.started = true;
            s.gl_texture_id = Some(1234);
        }

        let (_, started, texture, _) = read_state(&mut node_ds);
        assert!(
            started,
            "the callback and the node must share one state, not two copies"
        );
        assert_eq!(texture, Some(1234));
    }

    #[test]
    fn two_widgets_built_from_one_config_get_independent_state() {
        let a = ScreenCaptureWidget::create(cfg(
            ScreenCaptureSource::Display(0),
            30,
            RawImageFormat::BGRA8,
        ))
        .dom();
        let b = ScreenCaptureWidget::create(cfg(
            ScreenCaptureSource::Window(7),
            60,
            RawImageFormat::RGBA8,
        ))
        .dom();

        let mut da = a.root.get_dataset().cloned().expect("dataset a");
        let mut db = b.root.get_dataset().cloned().expect("dataset b");
        {
            let mut s = da
                .downcast_mut::<ScreenCaptureWidgetState>()
                .expect("state a");
            s.started = true;
        }

        let (config_a, started_a, _, _) = read_state(&mut da);
        let (config_b, started_b, _, _) = read_state(&mut db);
        assert!(started_a);
        assert!(
            !started_b,
            "two widgets must not share one global capture state"
        );
        assert_eq!(config_a.source, ScreenCaptureSource::Display(0));
        assert_eq!(config_b.source, ScreenCaptureSource::Window(7));
    }

    // ------------------------------------------------------------------
    // screencap_on_after_mount
    //
    // NOTE: the *first* mount (started == false) is deliberately not exercised.
    // It calls `Thread::create`, which spawns a real OS thread running
    // `screencap_worker`; nothing in a unit test drains that thread's channel, so
    // the worker would loop forever pushing ~3.7 MB frames while the `Thread`
    // destructor waits to join it. Only the guard paths below can be driven
    // safely (this mirrors the camera widget's test module).
    // ------------------------------------------------------------------

    #[test]
    fn after_mount_ignores_a_dataset_that_is_not_a_screencap_state() {
        for foreign in [RefAny::new(0_u32), RefAny::new(DEFAULT_CFG)] {
            // The second case is the plausible mistake: handing the *config* POD
            // instead of the widget state.
            let (update, changes) =
                with_callback_info(|info| screencap_on_after_mount(foreign.clone(), info));

            assert_eq!(update, Update::DoNothing);
            assert!(
                changes.is_empty(),
                "a foreign dataset must not start a capture thread: {changes:?}"
            );
        }
    }

    #[test]
    fn after_mount_is_a_no_op_once_the_thread_has_started() {
        let log = frame_log(Update::RefreshDom);
        let mut data = state_with_hook(DEFAULT_CFG, &log);
        {
            let mut s = data
                .downcast_mut::<ScreenCaptureWidgetState>()
                .expect("state");
            s.gl_texture_id = Some(3);
        }

        // Repeated mounts (relayout re-runs AfterMount) must stay inert.
        for _ in 0..3 {
            let (update, changes) =
                with_callback_info(|info| screencap_on_after_mount(data.clone(), info));
            assert_eq!(update, Update::DoNothing);
            assert!(
                changes.is_empty(),
                "AfterMount must start the capture thread at most once: {changes:?}"
            );
        }

        let (config, started, texture, has_hook) = read_state(&mut data);
        assert_eq!(config, DEFAULT_CFG, "a re-mount must not rewrite the config");
        assert!(started);
        assert_eq!(texture, Some(3), "a re-mount must not drop the texture");
        assert!(has_hook, "a re-mount must not drop the user hook");
    }

    // ------------------------------------------------------------------
    // capture_request / capture_floor — the config reaches the backend
    // ------------------------------------------------------------------

    #[test]
    fn the_config_source_and_fps_reach_the_backend_request_with_self_excluded() {
        // `config.source` and `config.fps` used to be ignored: the worker
        // opened display 0 at a hard-coded size and the backend ran at a
        // hard-coded 30 fps.
        let mut cfg = ScreenCaptureConfig::default();
        cfg.fps = 15;
        cfg.source = ScreenCaptureSource::Display(2);
        let r = capture_request(&cfg);
        assert_eq!((r.index, r.window, r.fps), (2, 0, 15));
        assert!(r.exclude_self, "a share never shows the sharing app to itself");
        cfg.source = ScreenCaptureSource::Window(0xABCD);
        assert_eq!(capture_request(&cfg).window, 0xABCD);
        cfg.source = ScreenCaptureSource::PrimaryDisplay;
        assert_eq!(capture_request(&cfg).index, 0);
        assert_eq!(capture_floor(false), None, "no hook: the tile + consumers size the stream");
        assert_eq!(capture_floor(true), Some((DEFAULT_W, DEFAULT_H)));
    }

    // ------------------------------------------------------------------
    // screencap_worker
    // ------------------------------------------------------------------

    #[test]
    fn worker_stops_as_soon_as_the_main_thread_stops_receiving() {
        let Some(sent) = run_worker(RefAny::new(())) else {
            return; // a platform screen backend is registered: not the test pattern
        };

        assert_eq!(
            sent.len(),
            1,
            "the worker must stop after the first rejected send, not spin"
        );
        assert_eq!(
            (sent[0].width, sent[0].height),
            (DEFAULT_W, DEFAULT_H),
            "the test pattern is emitted at the widget's default capture size"
        );
        assert_eq!(
            sent[0].len,
            (DEFAULT_W as usize) * (DEFAULT_H as usize) * 4,
            "the frame must be tightly-packed RGBA8: w * h * 4 bytes"
        );
    }

    #[test]
    fn worker_emits_the_documented_band_pattern_on_its_first_frame() {
        let Some(sent) = run_worker(RefAny::new(())) else {
            return;
        };
        let f = &sent[0];

        assert!(
            f.rows_uniform_opaque,
            "every pixel must be an opaque grey [v, v, v, 255]"
        );
        assert_eq!(
            f.row_values.len(),
            DEFAULT_H as usize,
            "one value per scanline"
        );
        // tick 0 => band == 0, so rows 0..8 are the bright band (|y - 0| < 8).
        assert!(
            f.row_values[..8].iter().all(|&v| v == 235),
            "rows 0..8 are the bright band, got {:?}",
            &f.row_values[..8]
        );
        assert!(
            f.row_values[8..].iter().all(|&v| v == 28),
            "every row below the band is dark grey"
        );
    }

    #[test]
    fn worker_ignores_its_init_payload_entirely() {
        // ADVERSARIAL: the test-pattern worker takes NO input - not the widget's
        // config, not its fps, not its source. A caller cannot influence the
        // frames by handing it a different init, and a garbage init must not
        // panic.
        let Some(unit) = run_worker(RefAny::new(())) else {
            return;
        };
        let Some(text) = run_worker(RefAny::new("not an init struct")) else {
            return;
        };
        let Some(widget_state) = run_worker(state(
            cfg(
                ScreenCaptureSource::Window(u64::MAX),
                u32::MAX,
                RawImageFormat::R8,
            ),
            true,
            Some(u32::MAX),
        )) else {
            return;
        };

        assert_eq!(unit, text, "a foreign init must not change the frames");
        assert_eq!(
            unit, widget_state,
            "even a full widget state (fps = u32::MAX, R8) must not change the \
             test pattern - it is hard-coded"
        );
    }

    // ------------------------------------------------------------------
    // screencap_writeback
    // ------------------------------------------------------------------

    #[test]
    fn writeback_invokes_the_hook_with_the_frame_and_returns_its_update() {
        for reply in [
            Update::DoNothing,
            Update::RefreshDom,
            Update::RefreshDomAllWindows,
        ] {
            let mut log = frame_log(reply);
            let mut data = state_with_hook(DEFAULT_CFG, &log);
            let frame_data = captured(frame(2, 2));

            let (update, _) = with_callback_info(|info| {
                screencap_writeback(data.clone(), frame_data.clone(), info)
            });

            assert_eq!(update, reply, "the user hook's Update must be returned as-is");
            assert_eq!(logged_frames(&mut log), vec![(2, 2, 16)]);
            let (_, _, texture, _) = read_state(&mut data);
            assert_eq!(
                texture, None,
                "without a GL context no texture id is ever installed"
            );
        }
    }

    #[test]
    fn writeback_ignores_frame_data_of_the_wrong_type() {
        let mut log = frame_log(Update::RefreshDom);
        let mut data = state_with_hook(DEFAULT_CFG, &log);

        let (update, changes) =
            with_callback_info(|info| screencap_writeback(data.clone(), RefAny::new(0_u32), info));

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "no frame -> no image change");
        assert!(
            logged_frames(&mut log).is_empty(),
            "the user hook must not fire without a frame"
        );
    }

    #[test]
    fn writeback_survives_a_writeback_dataset_that_is_not_a_screencap_state() {
        let (update, changes) = with_callback_info(|info| {
            screencap_writeback(RefAny::new(0_u32), captured(frame(1, 1)), info)
        });

        assert_eq!(
            update,
            Update::DoNothing,
            "a foreign dataset means no hook and no texture - but no panic either"
        );
        assert!(
            changes.is_empty(),
            "no node owns that dataset, so nothing may be installed: {changes:?}"
        );
    }

    #[test]
    fn writeback_keeps_a_preexisting_texture_id_on_the_cpu_path() {
        for current in [Some(0_u32), Some(42), Some(u32::MAX)] {
            let mut data = state(DEFAULT_CFG, true, current);
            let frame_data = captured(frame(2, 2));

            let (update, _) = with_callback_info(|info| {
                screencap_writeback(data.clone(), frame_data.clone(), info)
            });

            assert_eq!(update, Update::DoNothing, "no hook -> no user update");
            let (_, _, texture, _) = read_state(&mut data);
            assert_eq!(
                texture, current,
                "the stable texture id must survive the writeback unchanged"
            );
        }
    }

    #[test]
    fn writeback_rejects_a_frame_whose_bytes_do_not_match_its_dimensions() {
        // A malformed/hostile backend frame: the image upload must fail cleanly
        // instead of indexing out of bounds or allocating ~17 GB.
        for (w, h, bytes) in [
            (u32::MAX, 1_u32, Vec::new()),
            (4, 4, vec![0_u8; 63]),
            (4, 4, vec![0_u8; 65]),
            (2, 2, Vec::new()),
        ] {
            let mut data = state(DEFAULT_CFG, true, None);
            let bogus = captured(frame_raw(w, h, bytes.clone()));

            let (update, changes) =
                with_callback_info(|info| screencap_writeback(data.clone(), bogus.clone(), info));

            assert_eq!(update, Update::DoNothing);
            assert!(
                changes.is_empty(),
                "a {w}x{h} frame with {} bytes must not touch the DOM: {changes:?}",
                bytes.len()
            );
            let (_, _, texture, _) = read_state(&mut data);
            assert_eq!(texture, None, "a rejected frame must not invent a texture id");
        }
    }

    #[test]
    fn writeback_hands_even_a_rejected_frame_to_the_user_hook() {
        // FOOTGUN worth pinning: `present_frame` and `invoke_on_frame` are
        // independent. A frame the image pipeline rejects still reaches user code,
        // so `on_frame` is NOT a "this frame was valid" signal.
        let mut log = frame_log(Update::RefreshDom);
        let mut data = state_with_hook(DEFAULT_CFG, &log);
        let bogus = captured(frame_raw(u32::MAX, 1, Vec::new()));

        let (update, changes) =
            with_callback_info(|info| screencap_writeback(data.clone(), bogus.clone(), info));

        assert_eq!(update, Update::RefreshDom);
        assert!(changes.is_empty(), "the frame itself was rejected");
        assert_eq!(
            logged_frames(&mut log),
            vec![(u32::MAX, 1, 0)],
            "the hook sees the raw frame, dimensions and all, unvalidated"
        );
    }

    #[test]
    fn writeback_accepts_a_zero_sized_frame_without_panicking() {
        // 0 * 0 * 4 == 0 == len(bytes), so a 0x0 frame passes the length check and
        // is installed as a degenerate image. Pin that it stays panic-free and
        // leaves the texture id alone.
        let mut data = state(DEFAULT_CFG, true, Some(2));
        let empty = captured(frame_raw(0, 0, Vec::new()));

        let (update, _) =
            with_callback_info(|info| screencap_writeback(data.clone(), empty.clone(), info));

        assert_eq!(update, Update::DoNothing);
        let (_, _, texture, _) = read_state(&mut data);
        assert_eq!(texture, Some(2));
    }

    #[test]
    fn writeback_survives_dimensions_whose_byte_count_overflows_usize() {
        // ADVERSARIAL: a backend reporting 2^31 x 2^31 makes the CPU present path
        // compute `width * height * 4` in usize -> 2^64, which overflows. In a
        // debug build that is an arithmetic-overflow panic; in release it wraps to
        // 0 and the empty buffer is *accepted* as a valid 2^31 x 2^31 image.
        // Neither is a graceful rejection (see the autotest report) - what must
        // hold in both modes is that the widget's stored texture id is never
        // corrupted and the process is still usable afterwards.
        let mut data = state(DEFAULT_CFG, true, Some(11));
        let huge = captured(frame_raw(1_u32 << 31, 1_u32 << 31, Vec::new()));

        let (result, _) = with_callback_info(|info| {
            catch_unwind(AssertUnwindSafe(|| {
                screencap_writeback(data.clone(), huge.clone(), info)
            }))
        });

        match result {
            Ok(update) => {
                assert_eq!(update, Update::DoNothing);
                let (_, _, texture, _) = read_state(&mut data);
                assert_eq!(texture, Some(11), "the texture id must not be corrupted");
            }
            Err(_) => eprintln!(
                "NOTE: screencap_writeback panicked (usize overflow of width*height*4) for a \
                 2^31 x 2^31 frame - a malformed capture backend can take the process down"
            ),
        }
    }

    // ------------------------------------------------------------------
    // merge_screencap_state
    // ------------------------------------------------------------------

    #[test]
    fn merge_takes_the_thread_state_from_old_and_everything_else_from_new() {
        let fresh = cfg(
            ScreenCaptureSource::Window(u64::MAX),
            60,
            RawImageFormat::RGBA8,
        );
        let log = frame_log(Update::DoNothing);
        let new_data = state_with_hook(fresh, &log);
        let old_data = state(
            cfg(ScreenCaptureSource::Display(3), 1, RawImageFormat::R8),
            true,
            Some(9),
        );

        let mut merged = merge_screencap_state(new_data, old_data);
        let (config, started, texture, has_hook) = read_state(&mut merged);

        assert_eq!(config, fresh, "the fresh build's config wins");
        assert!(has_hook, "the fresh build's hook wins");
        assert!(started, "'thread already running' must carry forward");
        assert_eq!(texture, Some(9), "the stable texture id must carry forward");
    }

    #[test]
    fn merge_lets_the_old_thread_state_overwrite_a_fresh_builds_claim() {
        // The old state is authoritative for `started` / `gl_texture_id` in BOTH
        // directions: a fresh build that (wrongly) claims to be running is reset,
        // so the thread is started exactly once per real mount.
        let new_data = RefAny::new(ScreenCaptureWidgetState {
            config: DEFAULT_CFG,
            started: true,
            gl_texture_id: Some(77),
            on_frame: OptionOnVideoFrame::None,
            consumers: FrameConsumerVec::from_const_slice(&[]),
            on_consumer_frame: OptionOnConsumerFrame::None,
            thread_id: None,
            control: None,
            preview: None,
        });
        let old_data = state(DEFAULT_CFG, false, None);

        let mut merged = merge_screencap_state(new_data, old_data);
        let (_, started, texture, _) = read_state(&mut merged);

        assert!(!started, "the old state wins for `started`, in both directions");
        assert_eq!(texture, None, "and for the texture id too");
    }

    #[test]
    fn merge_returns_the_persistent_old_payload_not_a_copy() {
        // PIN FLIPPED (2026-07-31, deliberately): merge used to return the
        // NEW allocation, which orphaned the allocation live capture
        // backends hold a clone of (the frame writeback then wrote into a
        // dataset nobody rendered — the frozen-picture family). The rule is
        // now the map widget's: adopt config forward, return the OLD
        // (persistent) allocation.
        let old_data = state(DEFAULT_CFG, true, Some(5));
        let mut kept = old_data.clone();

        let mut merged = merge_screencap_state(state(DEFAULT_CFG, false, None), old_data);
        {
            let mut s = merged
                .downcast_mut::<ScreenCaptureWidgetState>()
                .expect("merged state");
            s.gl_texture_id = Some(1);
        }

        let (_, started, texture, _) = read_state(&mut kept);
        assert!(
            started,
            "the persistent allocation keeps its worker-facing fields"
        );
        assert_eq!(
            texture,
            Some(1),
            "merge must hand back the OLD allocation — the one live capture \
             backends hold a clone of"
        );
    }

    #[test]
    fn merge_leaves_the_new_state_alone_when_the_old_one_is_foreign() {
        let new_data = state(DEFAULT_CFG, false, None);
        let mut merged = merge_screencap_state(new_data, RefAny::new(0_u32));

        let (config, started, texture, _) = read_state(&mut merged);
        assert_eq!(config, DEFAULT_CFG);
        assert!(!started, "nothing to carry forward from a foreign payload");
        assert_eq!(texture, None);
    }

    #[test]
    fn merge_returns_a_foreign_new_dataset_untouched() {
        let old_data = state(DEFAULT_CFG, true, Some(1));
        let mut merged = merge_screencap_state(RefAny::new(77_u32), old_data);

        assert_eq!(
            merged.downcast_ref::<u32>().map(|v| *v),
            Some(77),
            "merge must hand back exactly the payload it was given"
        );
    }

    #[test]
    fn merge_of_a_dataset_with_itself_does_not_panic() {
        // The same RefAny on both sides: the mutable + shared borrow overlap, so
        // the merge is skipped rather than aliasing. Either way the state must
        // survive intact.
        let mut data = state(DEFAULT_CFG, true, Some(5));
        let mut merged = merge_screencap_state(data.clone(), data.clone());

        let (config, started, texture, _) = read_state(&mut merged);
        assert_eq!(config, DEFAULT_CFG);
        assert!(started);
        assert_eq!(texture, Some(5));
        assert_eq!(read_state(&mut data), (DEFAULT_CFG, true, Some(5), false));
    }

    /// REGRESSION (B3): a capture worker must ACKNOWLEDGE `TerminateThread`.
    ///
    /// Reported from azul-meet on macOS after using camera + screenshare — the
    /// framework printed, twice (once per capture worker):
    ///
    /// ```text
    /// [azul][thread] a background thread did not acknowledge TerminateThread
    /// within 2000ms and was DETACHED rather than joined.
    /// ```
    ///
    /// Root cause: the worker took its receiver as `_recv` and never polled it,
    /// so the terminate message was never observed. The only exit was
    /// `sender.send()` returning false, which does NOT happen at shutdown
    /// because the main thread still owns the receiving end while it waits out
    /// the grace period. Fixed by `capture_common::terminate_requested`.
    ///
    /// The budget here is the framework's own:
    /// `THREAD_TERMINATE_GRACE_STEPS (200) * 10ms = 2000ms`.
    #[test]
    fn screencap_worker_acknowledges_terminate_within_the_grace_budget() {
        use crate::thread::{Thread, ThreadCallback};

        let t = Thread::create(
            RefAny::new(0_usize),
            RefAny::new(0_usize),
            ThreadCallback::new(screencap_worker),
        );
        assert!(
            t.send_message(ThreadSendMsg::TerminateThread),
            "the worker holds its receiver alive, so the send must succeed"
        );
        let finished = || {
            t.ptr
                .lock()
                .expect("thread mutex must not be poisoned")
                .is_finished()
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_millis(2_000);
        while !finished() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            finished(),
            "screencap_worker did not acknowledge TerminateThread within 2000ms — at shutdown it \
             would be DETACHED rather than joined"
        );
    }
}
