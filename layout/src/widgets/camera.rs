//! Camera-preview widget - a "dumb widget" (like [`MapWidget`](super::map))
//! that owns a background capture thread + a GL-texture `ImageRef`, with **no**
//! camera-specific logic in the core framework (SUPER_PLAN_2 §4 P6, widget
//! pivot - see the MASTER PLAN in `MOBILE_SESSION_LOG.md`).
//!
//! `CameraWidget::create(config).dom()` -> a static `<img>` whose pixels a
//! background thread keeps fed. On `AfterMount` the capture thread starts
//! (`CallbackInfo::add_thread`); each frame goes through
//! [`super::capture_common::present_frame`], which uploads it into a stable
//! external GL texture + recomposites - no relayout, no display-list rebuild.
//! The shared thread/writeback/GL core lives in `capture_common`; this widget
//! is just its config + worker.
//!
//! ONE CAPTURE, MANY CONSUMERS. The camera is opened at the smallest size
//! that covers everyone who wants frames — the on-screen tile (its device
//! size, reported by layout through `NodeResized`), every
//! [`FrameConsumer`] registered with [`CameraWidget::with_consumer`] ("client
//! Bob wants 500x200"), and the configured size when an `on_frame` hook
//! wants the frame as captured. Each captured frame is cut to every
//! consumer's size OFF the main thread (`capture_common::run_capture_loop`
//! + `image_scale::fan_out`) and delivered through
//! [`CameraWidget::with_on_consumer_frame`]; the tile gets a frame at
//! exactly its own size. Without a platform backend a colour-cycle test
//! pattern runs through the same loop.

use alloc::vec::Vec;

use azul_core::callbacks::Update;
use azul_core::camera::CameraConfig;
use azul_core::dom::{ComponentEventFilter, DatasetMergeCallbackType, Dom, EventFilter};
use azul_core::refany::{OptionRefAny, RefAny};
use azul_core::resources::{ImageRef, RawImageFormat};
use azul_core::task::{ThreadId, ThreadReceiver, ThreadSendMsg};
use azul_core::video::{FrameConsumer, FrameConsumerVec};
use azul_css::AzString;

use super::capture_common::{
    camera_backend, frame_resampler, present_captured, preview_size_for_node, run_capture_loop,
    send_capture_targets, test_pattern_vtable, CaptureRequest, CaptureSession, CaptureTargets,
    CapturedFrames, OnConsumerFrame, OnConsumerFrameCallback, OnVideoFrame, OnVideoFrameCallback,
    OptionOnConsumerFrame, OptionOnVideoFrame, TestPattern, REOPEN_COOLDOWN_MS,
};
use crate::callbacks::{Callback, CallbackInfo, CallbackType};
use crate::thread::{Thread, ThreadCallback, ThreadSender};

/// Init data handed to the capture worker thread.
struct CameraThreadInit {
    /// Camera device index.
    index: u32,
    /// Requested frame rate (0 = backend default).
    fps: u32,
    /// The size to capture when nothing else is known (`frame_dims`).
    width: u32,
    /// See `width`.
    height: u32,
    /// The size the capture never drops below (see [`capture_floor`]).
    floor: Option<(u32, u32)>,
    /// Who wants frames at mount time (the tile's size if already laid out,
    /// the registered consumers, whether the `on_frame` hook is set).
    targets: CaptureTargets,
}

impl CameraThreadInit {
    /// An init with only a size (the test pattern's size without a backend).
    const fn sized(width: u32, height: u32) -> Self {
        Self {
            index: 0,
            fps: 0,
            width,
            height,
            floor: None,
            targets: CaptureTargets {
                preview: None,
                consumers: Vec::new(),
                wants_source: false,
            },
        }
    }
}

/// Live state for one camera widget, carried across relayout by
/// [`merge_camera_state`].
#[derive(Debug)]
pub struct CameraWidgetState {
    /// The requested capture configuration (the control POD).
    pub config: CameraConfig,
    /// `true` once the capture thread has been started.
    pub started: bool,
    /// The stable external GL texture id once the first frame installed it.
    pub gl_texture_id: Option<u32>,
    /// Optional user hook invoked with each captured frame (effects / save /
    /// send). Re-set on every fresh build (see [`merge_camera_state`]).
    pub on_frame: OptionOnVideoFrame,
    /// Every registered consumer (see [`FrameConsumer`]). Re-set on every
    /// fresh build; a change is pushed to the running worker.
    pub consumers: FrameConsumerVec,
    /// Optional hook receiving every consumer's cut of every frame.
    pub on_consumer_frame: OptionOnConsumerFrame,
    /// The MARKER stamped on the tile node (`Dom::with_marker`); the frame
    /// writeback resolves it back to the node via
    /// `CallbackInfo::get_node_id_by_marker`. Adopted forward on merge so the
    /// persistent allocation always names the CURRENT build's node.
    pub marker: AzString,
    /// The capture worker, once started (`NodeResized` messages it).
    pub thread_id: Option<ThreadId>,
    /// The main->worker sender, cloned at mount so the merge callback (which
    /// has no `CallbackInfo`) can push a changed consumer list.
    pub control: Option<std::sync::mpsc::Sender<ThreadSendMsg>>,
    /// The tile's last reported device size (so a resize that does not change
    /// it sends nothing).
    pub preview: Option<(u32, u32)>,
}

impl CameraWidgetState {
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

/// A camera-preview widget. `create(config).dom()` yields an `<img>` the
/// capture thread keeps fed.
#[repr(C)]
#[derive(Debug)]
pub struct CameraWidget {
    /// Requested capture config (camera facing, resolution, fps, format).
    pub config: CameraConfig,
    /// Optional per-frame user hook (effects / save / send - azul-meet).
    pub on_frame: OptionOnVideoFrame,
    /// Consumers of the captured frames beyond the on-screen tile: each gets
    /// its own cut of every frame at its requested size.
    pub consumers: FrameConsumerVec,
    /// Optional hook receiving each consumer's cut (see `consumers`).
    pub on_consumer_frame: OptionOnConsumerFrame,
}

impl CameraWidget {
    /// Create a camera widget for the given capture config.
    #[must_use]
    pub const fn create(config: CameraConfig) -> Self {
        Self {
            config,
            on_frame: OptionOnVideoFrame::None,
            consumers: FrameConsumerVec::from_const_slice(&[]),
            on_consumer_frame: OptionOnConsumerFrame::None,
        }
    }

    /// Register a consumer of the captured frames: every frame is cut to
    /// `consumer`'s size (off the main thread) and handed to the
    /// `on_consumer_frame` hook with `consumer.id`. The camera is opened at
    /// the smallest size covering every consumer and the tile, so "client
    /// Bob wants 500x200 while the local preview is 100x200" captures ONE
    /// 500x200 frame and samples it twice - nothing larger is captured, and
    /// nothing larger than asked is sent. A consumer with a zero size or the
    /// reserved preview id 0 is ignored.
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
    /// frame (route on `frame.consumer.id`; send Bob his, record the other).
    pub fn set_on_consumer_frame<C: Into<OnConsumerFrameCallback>>(
        &mut self,
        data: RefAny,
        on_consumer_frame: C,
    ) {
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
    #[must_use]
    pub fn dom(self) -> Dom {
        let marker = super::capture_common::next_capture_marker("camera");
        let state = CameraWidgetState {
            config: self.config,
            started: false,
            gl_texture_id: None,
            on_frame: self.on_frame,
            consumers: self.consumers,
            on_consumer_frame: self.on_consumer_frame,
            marker: marker.clone(),
            thread_id: None,
            control: None,
            preview: None,
        };
        let dataset = RefAny::new(state);

        let (w, h) = frame_dims(&self.config);
        let placeholder = ImageRef::null_image(
            w as usize,
            h as usize,
            RawImageFormat::BGRA8,
            b"azul-camera-placeholder".to_vec(),
        );

        Dom::create_image(placeholder)
            .with_dataset(OptionRefAny::Some(dataset.clone()))
            .with_marker(Some(marker).into())
            .with_merge_callback(azul_core::dom::DatasetMergeCallback::from_ptr(merge_camera_state))
            .with_callback(
                EventFilter::Component(ComponentEventFilter::AfterMount),
                dataset.clone(),
                Callback::from_ptr(camera_on_after_mount),
            )
            // The tile's device size feeds the capture size + the preview
            // cut — see `camera_on_resize`.
            .with_callback(
                EventFilter::Component(ComponentEventFilter::NodeResized),
                dataset,
                Callback::from_ptr(camera_on_resize),
            )
    }
}

/// Frame dimensions for a config (0 -> a sane default).
const fn frame_dims(config: &CameraConfig) -> (u32, u32) {
    let w = if config.width > 0 { config.width } else { 640 };
    let h = if config.height > 0 {
        config.height
    } else {
        480
    };
    (w, h)
}

/// The size the capture never drops below: the configured size when the app
/// set one explicitly (either axis), or the default when an `on_frame` hook
/// expects frames as the config promised. `None` otherwise — then the tile
/// and the consumers alone decide, and a 300x200 tile captures 300x200.
const fn capture_floor(config: &CameraConfig, wants_source: bool) -> Option<(u32, u32)> {
    if config.width > 0 || config.height > 0 || wants_source {
        Some(frame_dims(config))
    } else {
        None
    }
}

/// `AfterMount`: start the background capture thread exactly once, telling
/// it who wants frames (the tile's size if layout already produced one).
extern "C" fn camera_on_after_mount(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let preview = preview_size_for_node(&info);
    let init = {
        let Some(mut s) = data.downcast_mut::<CameraWidgetState>() else {
            return Update::DoNothing;
        };
        if s.started {
            return Update::DoNothing;
        }
        s.started = true;
        s.preview = preview;
        let (width, height) = frame_dims(&s.config);
        CameraThreadInit {
            index: 0,
            fps: s.config.fps,
            width,
            height,
            floor: capture_floor(&s.config, s.on_frame.is_some()),
            targets: s.targets(),
        }
    };

    let tid = ThreadId::unique();
    let thread = Thread::create(
        RefAny::new(init),
        data.clone(),
        ThreadCallback::new(camera_worker),
    );
    // Grab the main->worker sender BEFORE add_thread moves the Thread, so the
    // merge callback can push a changed consumer list without a CallbackInfo.
    let control = thread.clone_sender();
    info.add_thread(tid, thread);
    if let Some(mut s) = data.downcast_mut::<CameraWidgetState>() {
        s.thread_id = Some(tid);
        s.control = control;
    }
    Update::DoNothing
}

/// `NodeResized`: the tile's device size changed — tell the worker, which
/// cuts the preview at the new size and reopens the camera smaller/larger
/// when that pays (see `capture_common::needs_reopen`). A message, not a
/// relayout: returns `DoNothing`.
extern "C" fn camera_on_resize(mut data: RefAny, info: CallbackInfo) -> Update {
    let Some(preview) = preview_size_for_node(&info) else {
        return Update::DoNothing;
    };
    let (tid, targets) = {
        let Some(mut s) = data.downcast_mut::<CameraWidgetState>() else {
            return Update::DoNothing;
        };
        if s.preview == Some(preview) {
            return Update::DoNothing;
        }
        s.preview = Some(preview);
        (s.thread_id, s.targets())
    };
    if let Some(tid) = tid {
        // Best effort: a worker that already exited has nothing to resize.
        send_capture_targets(&info, tid, targets);
    }
    Update::DoNothing
}

/// Background worker: the shared capture loop over the registered camera
/// backend (v4l2 / `AVFoundation` / Media Foundation / Camera2), else the
/// colour-cycle test pattern — see `capture_common::run_capture_loop`.
extern "C" fn camera_worker(mut init: RefAny, mut sender: ThreadSender, mut recv: ThreadReceiver) {
    let (targets, session) = match init.downcast_ref::<CameraThreadInit>() {
        Some(i) => (
            i.targets.clone(),
            camera_session(i.index, i.fps, (i.width, i.height), i.floor),
        ),
        None => (
            CaptureTargets::default(),
            camera_session(0, 0, (640, 480), None),
        ),
    };
    run_capture_loop(session, targets, &mut sender, &mut recv);
}

/// The camera's capture session: platform backend + colour-cycle fallback.
fn camera_session(
    index: u32,
    fps: u32,
    fallback: (u32, u32),
    floor: Option<(u32, u32)>,
) -> CaptureSession {
    CaptureSession {
        backend: camera_backend(),
        test_pattern: test_pattern_vtable(TestPattern::ColourCycle),
        request: CaptureRequest {
            index,
            window: 0,
            width: 0,
            height: 0,
            fps,
            exclude_self: false,
        },
        floor,
        fallback,
        writeback: camera_writeback,
        resample: frame_resampler(),
        reopen_cooldown_ms: REOPEN_COOLDOWN_MS,
    }
}

/// Writeback (main thread): run the hooks and put the preview cut (else the
/// captured frame) on the node — `capture_common::present_captured`.
extern "C" fn camera_writeback(
    mut writeback_data: RefAny,
    mut frame_data: RefAny,
    mut info: CallbackInfo,
) -> Update {
    let (on_frame, on_consumer_frame, marker) = writeback_data
        .downcast_ref::<CameraWidgetState>()
        .map_or_else(
            || {
                (
                    OptionOnVideoFrame::None,
                    OptionOnConsumerFrame::None,
                    AzString::from_const_str(""),
                )
            },
            |s| (s.on_frame.clone(), s.on_consumer_frame.clone(), s.marker.clone()),
        );
    let Some(mut captured) = frame_data.downcast_mut::<CapturedFrames>() else {
        return Update::DoNothing;
    };
    present_captured(
        &mut info,
        marker,
        &on_frame,
        &on_consumer_frame,
        &mut captured,
    )
}

/// Carry live state forward across relayout (config from the fresh build,
/// thread / texture from the previous frame).
extern "C" fn merge_camera_state(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
    // Return the OLD allocation (the one live capture backends may hold a
    // clone of), adopting config forward — the merge_map_tile_cache rule.
    // Returning new_data would re-point the DOM at a fresh allocation the
    // running worker's writeback clone never sees.
    let merged_into_old = {
        let new_guard = new_data.downcast_ref::<CameraWidgetState>();
        let old_guard = old_data.downcast_mut::<CameraWidgetState>();
        if let (Some(new_g), Some(mut old_g)) = (new_guard, old_guard) {
            let worker_cares = old_g.consumers != new_g.consumers
                || old_g.on_frame.is_some() != new_g.on_frame.is_some();
            old_g.config = new_g.config;
            old_g.on_frame = new_g.on_frame.clone();
            old_g.consumers = new_g.consumers.clone();
            old_g.on_consumer_frame = new_g.on_consumer_frame.clone();
            // The reconciled NODE carries the NEW build's marker string, so
            // the surviving allocation must resolve by that string too.
            old_g.marker = new_g.marker.clone();
            // A changed consumer list / hook presence reaches the RUNNING
            // worker now (it reopens or refans as needed) — there is no
            // CallbackInfo here, hence the cloned sender.
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
        sync::{
            mpsc::{channel, Receiver, Sender},
            Arc, Mutex,
        },
    };

    use azul_core::{
        camera::CameraFacing,
        dom::{DomId, DomNodeId, NodeType},
        geom::OptionLogicalPosition,
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        resources::{DecodedImage, RendererResources},
        styled_dom::NodeHierarchyItemId,
        task::{
            OptionThreadSendMsg, ThreadReceiverDestructorCallback, ThreadReceiverInner,
            ThreadRecvCallback, ThreadSendMsg,
        },
        window::{MonitorVec, RawWindowHandle},
    };
    use azul_css::system::SystemStyle;
    use rust_fontconfig::FcFontCache;

    use azul_core::video::{ConsumerFrame, VideoFrame};

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        thread::{
            ThreadReceiveMsg, ThreadSendCallback, ThreadSenderDestructorCallback, ThreadSenderInner,
        },
        widgets::capture_common::{OnConsumerFrameCallbackType, OnVideoFrameCallbackType},
        window::LayoutWindow,
        window_state::FullWindowState,
    };

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    const ALL_FACINGS: [CameraFacing; 3] = [
        CameraFacing::Front,
        CameraFacing::Back,
        CameraFacing::External,
    ];

    /// A config with explicit dimensions (everything else fixed).
    fn cfg(width: u32, height: u32) -> CameraConfig {
        CameraConfig {
            facing: CameraFacing::Front,
            width,
            height,
            fps: 30,
            output_format: RawImageFormat::BGRA8,
        }
    }

    /// A `CameraWidgetState` payload with no `on_frame` hook.
    fn state(config: CameraConfig, started: bool, gl_texture_id: Option<u32>) -> RefAny {
        RefAny::new(CameraWidgetState {
            config,
            started,
            gl_texture_id,
            on_frame: OptionOnVideoFrame::None,
            consumers: FrameConsumerVec::from_const_slice(&[]),
            on_consumer_frame: OptionOnConsumerFrame::None,
            marker: "azul-camera-test".into(),
            thread_id: None,
            control: None,
            preview: None,
        })
    }

    /// The writeback payload the worker queues for one captured `frame`
    /// (no preview cut, no consumers) — what `run_capture_loop` sends when
    /// the tile's size is unknown.
    fn captured(frame: VideoFrame) -> RefAny {
        RefAny::new(CapturedFrames {
            source: Some(frame),
            preview: None,
            consumers: Vec::new(),
            in_flight: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
        })
    }

    /// `(config, started, gl_texture_id, has_hook)` of a `CameraWidgetState` payload.
    fn read_state(data: &mut RefAny) -> (CameraConfig, bool, Option<u32>, bool) {
        let s = data
            .downcast_ref::<CameraWidgetState>()
            .expect("payload must still be a CameraWidgetState");
        (
            s.config,
            s.started,
            s.gl_texture_id,
            matches!(s.on_frame, OptionOnVideoFrame::Some(_)),
        )
    }

    /// The placeholder image behind an `<img>` `Dom` root: `(width, height, format, tag)`.
    fn placeholder_of(dom: &Dom) -> (usize, usize, RawImageFormat, Vec<u8>) {
        let NodeType::Image(image) = dom.root.get_node_type() else {
            panic!("CameraWidget::dom must build an image node");
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

    /// Records every frame a widget's `on_frame` hook is handed.
    struct FrameLog {
        seen: Vec<(u32, u32, usize)>,
    }

    extern "C" fn record_frame(mut data: RefAny, _: CallbackInfo, frame: VideoFrame) -> Update {
        if let Some(mut log) = data.downcast_mut::<FrameLog>() {
            log.seen
                .push((frame.width, frame.height, frame.bytes.as_ref().len()));
        }
        Update::RefreshDom
    }

    extern "C" fn frame_do_nothing(_: RefAny, _: CallbackInfo, _: VideoFrame) -> Update {
        Update::DoNothing
    }

    /// The frames recorded by a `FrameLog` payload.
    fn logged_frames(data: &mut RefAny) -> Vec<(u32, u32, usize)> {
        data.downcast_ref::<FrameLog>()
            .expect("payload must still be a FrameLog")
            .seen
            .clone()
    }

    /// A `CameraWidgetState` whose `on_frame` hook writes into `log`.
    fn state_with_hook(config: CameraConfig, log: &RefAny) -> RefAny {
        RefAny::new(CameraWidgetState {
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
            marker: "azul-camera-test".into(),
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

    // ---- CallbackInfo harness --------------------------------------------

    /// Runs `f` against a real `CallbackInfo` over an empty `LayoutWindow` (no GL
    /// context -> the widgets' CPU present path). Returns `f`'s value plus every
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

    // ---- camera_worker harness -------------------------------------------

    /// Every frame `camera_worker` pushed: `(width, height, bytes, all pixels are the
    /// tick-0 colour)`. Guarded by `WORKER_GATE` - the worker's send callback is a
    /// plain C fn pointer, so it has nowhere else to put its result.
    static WORKER_LOG: Mutex<Vec<(u32, u32, usize, bool)>> = Mutex::new(Vec::new());
    static WORKER_GATE: Mutex<()> = Mutex::new(());

    /// Records the frame, then reports the send as *failed* - i.e. "the main thread is
    /// gone", the only signal `camera_worker` has to stop. A worker that ignores it
    /// would hang this test forever.
    extern "C" fn record_and_stop(
        _sender: *const core::ffi::c_void,
        msg: ThreadReceiveMsg,
    ) -> bool {
        if let ThreadReceiveMsg::WriteBack(mut wb) = msg {
            if let Some(c) = wb.refany.downcast_ref::<CapturedFrames>() {
                let Some(f) = c.preview.as_ref().or(c.source.as_ref()) else {
                    return false;
                };
                let bytes = f.bytes.as_ref();
                let tick0 = bytes.chunks_exact(4).all(|px| px == &[0u8, 0, 0, 255][..]);
                WORKER_LOG
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push((f.width, f.height, bytes.len(), tick0));
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
            send_fn: ThreadSendCallback {
                cb: record_and_stop,
            },
            destructor: ThreadSenderDestructorCallback {
                cb: sender_drop_noop,
            },
        });
        (rx, sender)
    }

    /// A `ThreadReceiver` that never delivers anything (`camera_worker` ignores it).
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

    /// Runs `camera_worker` with `init` against a sender that rejects the first frame,
    /// and returns everything the worker managed to send. `None` when a real platform
    /// backend is registered in this process (then the worker is not the test pattern
    /// these assertions describe).
    fn run_worker(init: RefAny) -> Option<Vec<(u32, u32, usize, bool)>> {
        let _gate = WORKER_GATE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if camera_backend().is_some() {
            return None;
        }
        WORKER_LOG
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();

        let (_rx, sender) = stopped_sender();
        let (_tx, receiver) = silent_receiver();
        camera_worker(init, sender, receiver);

        let sent = WORKER_LOG
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        Some(sent)
    }

    // ------------------------------------------------------------------
    // frame_dims  (numeric / boundary)
    // ------------------------------------------------------------------

    #[test]
    fn frame_dims_substitutes_the_default_for_a_zero_dimension() {
        assert_eq!(frame_dims(&cfg(0, 0)), (640, 480));
        assert_eq!(frame_dims(&cfg(0, 720)), (640, 720), "only width defaults");
        assert_eq!(
            frame_dims(&cfg(1280, 0)),
            (1280, 480),
            "only height defaults"
        );
        assert_eq!(frame_dims(&CameraConfig::default()), (640, 480));
    }

    #[test]
    fn frame_dims_passes_nonzero_dimensions_through_unclamped() {
        assert_eq!(frame_dims(&cfg(1, 1)), (1, 1), "1px is not 'unset'");
        assert_eq!(frame_dims(&cfg(u32::MAX, u32::MAX)), (u32::MAX, u32::MAX));
        assert_eq!(frame_dims(&cfg(u32::MAX, 0)), (u32::MAX, 480));
    }

    #[test]
    fn frame_dims_ignores_facing_fps_and_format() {
        for facing in ALL_FACINGS {
            for fps in [0, 1, u32::MAX] {
                let config = CameraConfig {
                    facing,
                    width: 0,
                    height: 0,
                    fps,
                    output_format: RawImageFormat::R8,
                };
                assert_eq!(frame_dims(&config), (640, 480));
            }
        }
    }

    #[test]
    fn frame_dims_is_usable_in_const_context() {
        const CONFIG: CameraConfig = CameraConfig {
            facing: CameraFacing::Back,
            width: 0,
            height: 4096,
            fps: 0,
            output_format: RawImageFormat::BGRA8,
        };
        const DIMS: (u32, u32) = frame_dims(&CONFIG);
        assert_eq!(DIMS, (640, 4096));
    }

    // ------------------------------------------------------------------
    // CameraWidget::create / set_on_frame / with_on_frame
    // ------------------------------------------------------------------

    #[test]
    fn create_stores_the_config_verbatim_and_leaves_the_hook_unset() {
        for facing in ALL_FACINGS {
            for (w, h, fps) in [(0, 0, 0), (1, 1, 1), (u32::MAX, u32::MAX, u32::MAX)] {
                let config = CameraConfig {
                    facing,
                    width: w,
                    height: h,
                    fps,
                    output_format: RawImageFormat::RGBA8,
                };
                let widget = CameraWidget::create(config);
                assert_eq!(
                    widget.config, config,
                    "create must not normalise the config"
                );
                assert!(
                    matches!(widget.on_frame, OptionOnVideoFrame::None),
                    "a fresh widget has no frame hook"
                );
            }
        }
    }

    #[test]
    fn with_on_frame_installs_the_hook_and_keeps_the_config() {
        let config = cfg(320, 240);
        let widget = CameraWidget::create(config).with_on_frame(
            RefAny::new(FrameLog { seen: Vec::new() }),
            record_frame as OnVideoFrameCallbackType,
        );

        assert_eq!(
            widget.config, config,
            "the builder must not touch the config"
        );
        let OptionOnVideoFrame::Some(hook) = &widget.on_frame else {
            panic!("with_on_frame must install a hook");
        };
        assert_eq!(
            hook.callback.cb as usize,
            record_frame as OnVideoFrameCallbackType as usize
        );
    }

    #[test]
    fn set_on_frame_twice_keeps_only_the_last_hook() {
        let mut widget = CameraWidget::create(cfg(2, 2));
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
            hook.callback.cb as usize, frame_do_nothing as OnVideoFrameCallbackType as usize,
            "the second set_on_frame must replace the first"
        );
    }

    // ------------------------------------------------------------------
    // CameraWidget::dom
    // ------------------------------------------------------------------

    #[test]
    fn dom_placeholder_uses_the_defaulted_dims_and_is_always_bgra8() {
        let (w, h, format, tag) = placeholder_of(&CameraWidget::create(cfg(0, 0)).dom());
        assert_eq!((w, h), (640, 480), "a 0-sized config falls back to 640x480");
        assert_eq!(format, RawImageFormat::BGRA8);
        assert_eq!(tag, b"azul-camera-placeholder".to_vec());

        // The requested output format is a *capture* request - the placeholder is
        // BGRA8 regardless.
        let config = CameraConfig {
            output_format: RawImageFormat::R8,
            ..cfg(320, 240)
        };
        let (w, h, format, _) = placeholder_of(&CameraWidget::create(config).dom());
        assert_eq!((w, h), (320, 240));
        assert_eq!(format, RawImageFormat::BGRA8);
    }

    #[test]
    fn dom_with_extreme_dims_builds_a_null_image_without_allocating() {
        // u32::MAX x u32::MAX pixels is ~7e19 bytes - a NullImage reserves no memory,
        // so this must stay a cheap, panic-free descriptor.
        let (w, h, format, _) =
            placeholder_of(&CameraWidget::create(cfg(u32::MAX, u32::MAX)).dom());
        assert_eq!((w, h), (u32::MAX as usize, u32::MAX as usize));
        assert_eq!(format, RawImageFormat::BGRA8);
    }

    #[test]
    fn dom_wires_exactly_one_after_mount_callback_a_dataset_and_a_merge_callback() {
        let dom = CameraWidget::create(cfg(64, 48)).dom();

        assert_eq!(
            dom.children.as_ref().len(),
            0,
            "the widget is a single node"
        );

        let callbacks = dom.root.get_callbacks();
        assert_eq!(
            callbacks.as_ref().len(),
            2,
            "two callbacks: the AfterMount capture-thread starter + the NodeResized re-targeter"
        );
        assert_eq!(
            callbacks.as_ref()[0].event,
            EventFilter::Component(ComponentEventFilter::AfterMount)
        );
        assert!(
            dom.root.get_merge_callback().is_some(),
            "state must survive relayout"
        );

        let mut dataset = dom
            .root
            .get_dataset()
            .cloned()
            .expect("the node must carry its CameraWidgetState");
        let (config, started, texture, has_hook) = read_state(&mut dataset);
        assert_eq!(config, cfg(64, 48));
        assert!(!started, "the thread only starts on AfterMount");
        assert_eq!(texture, None);
        assert!(!has_hook);
    }

    #[test]
    fn dom_moves_the_on_frame_hook_into_the_dataset() {
        let dom = CameraWidget::create(cfg(8, 8))
            .with_on_frame(
                RefAny::new(FrameLog { seen: Vec::new() }),
                record_frame as OnVideoFrameCallbackType,
            )
            .dom();

        let mut dataset = dom.root.get_dataset().cloned().expect("dataset");
        let (_, _, _, has_hook) = read_state(&mut dataset);
        assert!(has_hook, "dom() must carry the user hook into the state");
    }

    // ------------------------------------------------------------------
    // camera_on_after_mount
    //
    // NOTE: the *first* mount is deliberately not exercised - it spawns a real
    // capture thread whose `Thread` destructor joins a worker that never reads its
    // receiver, which would hang the test binary (see the report). Only the guard
    // paths below can be driven safely.
    // ------------------------------------------------------------------

    #[test]
    fn after_mount_ignores_a_dataset_that_is_not_a_camera_state() {
        let (update, changes) =
            with_callback_info(|info| camera_on_after_mount(RefAny::new(0_u32), info));

        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "a foreign dataset must not start a capture thread"
        );
    }

    #[test]
    fn after_mount_is_a_no_op_once_the_thread_has_started() {
        let mut data = state(cfg(0, 0), true, Some(3));
        let (update, changes) =
            with_callback_info(|info| camera_on_after_mount(data.clone(), info));

        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "AfterMount must start the capture thread at most once"
        );
        let (_, started, texture, _) = read_state(&mut data);
        assert!(started);
        assert_eq!(texture, Some(3), "a re-mount must not drop the texture");
    }

    // ------------------------------------------------------------------
    // camera_worker
    // ------------------------------------------------------------------

    #[test]
    fn worker_stops_as_soon_as_the_main_thread_stops_receiving() {
        let Some(sent) = run_worker(RefAny::new(CameraThreadInit::sized(2, 3))) else {
            return; // a platform backend is registered: not the test pattern
        };

        assert_eq!(
            sent.len(),
            1,
            "the worker must stop after the first rejected send, not spin"
        );
        assert_eq!(
            sent[0],
            (2, 3, 2 * 3 * 4, true),
            "the first test-pattern frame is w*h*4 opaque-black RGBA bytes"
        );
    }

    #[test]
    fn worker_with_a_foreign_init_falls_back_to_640x480() {
        let Some(sent) = run_worker(RefAny::new("not a CameraThreadInit")) else {
            return;
        };

        assert_eq!(sent.len(), 1);
        let (w, h, bytes, _) = sent[0];
        assert_eq!(
            (w, h),
            (640, 480),
            "a bad init must not panic - it defaults"
        );
        assert_eq!(bytes, 640 * 480 * 4);
    }

    #[test]
    fn worker_with_zero_dims_terminates_without_a_frame_instead_of_hanging() {
        // camera_on_after_mount always routes through frame_dims, but the worker itself
        // does not - a 0x0 init must still terminate. A zero-sized frame is not a
        // frame: the test pattern refuses to open and the loop returns.
        let Some(sent) = run_worker(RefAny::new(CameraThreadInit::sized(0, 0))) else {
            return;
        };

        assert!(sent.is_empty(), "nothing to capture at 0x0: {sent:?}");
    }

    // ------------------------------------------------------------------
    // One capture, many consumers (widget side)
    // ------------------------------------------------------------------

    extern "C" fn record_consumer_frame(
        mut data: RefAny,
        _: CallbackInfo,
        cut: ConsumerFrame,
    ) -> Update {
        if let Some(mut log) = data.downcast_mut::<FrameLog>() {
            log.seen.push((
                cut.consumer.id,
                cut.frame.width,
                cut.frame.bytes.as_ref().len(),
            ));
        }
        Update::RefreshDom
    }

    #[test]
    fn consumers_and_their_hook_reach_the_dataset_and_the_worker_targets() {
        let log = RefAny::new(FrameLog { seen: Vec::new() });
        let dom = CameraWidget::create(cfg(0, 0))
            .with_consumer(FrameConsumer::new(7, 500, 200)) // client Bob
            .with_consumer(FrameConsumer::new(7, 640, 360)) // Bob changed his mind: replaces
            .with_consumer(FrameConsumer::new(8, 1280, 720)) // the recorder
            .with_on_consumer_frame(log, record_consumer_frame as OnConsumerFrameCallbackType)
            .dom();
        let mut dataset = dom.root.get_dataset().cloned().expect("dataset");
        let s = dataset
            .downcast_ref::<CameraWidgetState>()
            .expect("camera state");
        assert_eq!(
            s.consumers.as_ref(),
            &[
                FrameConsumer::new(7, 640, 360),
                FrameConsumer::new(8, 1280, 720)
            ],
            "a same-id consumer replaces the earlier one"
        );
        assert!(s.on_consumer_frame.is_some());
        let t = s.targets();
        assert_eq!(t.consumers.len(), 2);
        assert!(
            !t.wants_source,
            "no on_frame hook: the captured frame stays on the worker"
        );
        assert_eq!(t.preview, None, "not laid out yet");
    }

    #[test]
    fn the_capture_floor_is_the_config_size_only_when_set_or_when_a_hook_wants_the_source() {
        // No explicit size, no hook: the tile and the consumers decide — a
        // 300x200 tile captures 300x200, not the 640x480 default.
        assert_eq!(capture_floor(&cfg(0, 0), false), None);
        // A hook sees frames "as configured": the default is the floor.
        assert_eq!(capture_floor(&cfg(0, 0), true), Some((640, 480)));
        // An explicit size is a floor on its own.
        assert_eq!(capture_floor(&cfg(1280, 720), false), Some((1280, 720)));
        assert_eq!(capture_floor(&cfg(0, 720), false), Some((640, 720)));
    }

    #[test]
    fn writeback_routes_each_consumer_cut_to_the_consumer_hook_and_shows_the_preview() {
        let mut log = RefAny::new(FrameLog { seen: Vec::new() });
        let mut data = state(cfg(0, 0), true, None);
        if let Some(mut s) = data.downcast_mut::<CameraWidgetState>() {
            s.on_consumer_frame = Some(OnConsumerFrame {
                refany: log.clone(),
                callback: (record_consumer_frame as OnConsumerFrameCallbackType).into(),
            })
            .into();
        }
        let latch = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let payload = RefAny::new(CapturedFrames {
            source: None,
            preview: Some(frame(4, 3)),
            consumers: vec![
                ConsumerFrame::new(FrameConsumer::new(7, 2, 1), frame(2, 1)),
                ConsumerFrame::new(FrameConsumer::new(8, 3, 3), frame(3, 3)),
            ],
            in_flight: latch.clone(),
        });

        let (update, changes) =
            with_callback_info(|info| camera_writeback(data.clone(), payload.clone(), info));

        assert_eq!(
            update,
            Update::RefreshDom,
            "the consumer hook's Update wins"
        );
        assert_eq!(
            logged_frames(&mut log),
            vec![(7, 2, 2 * 4), (8, 3, 3 * 3 * 4)],
            "one hook call per consumer, each with ITS cut"
        );
        assert!(
            !latch.load(std::sync::atomic::Ordering::Acquire),
            "the writeback releases the back-pressure latch"
        );
        // The harness has no DOM, so present_frame_pixels finds no node to
        // install the preview on; the image change itself is exercised by
        // the headless capture-tile test.
        let _ = changes;
    }

    #[test]
    fn resize_without_a_laid_out_node_sends_nothing_and_stores_nothing() {
        let mut data = state(cfg(0, 0), true, None);
        let (update, changes) = with_callback_info(|info| camera_on_resize(data.clone(), info));
        assert_eq!(
            update,
            Update::DoNothing,
            "a resize is a message, never a relayout"
        );
        assert!(changes.is_empty());
        let s = data.downcast_ref::<CameraWidgetState>().expect("state");
        assert_eq!(s.preview, None, "no node size -> no preview size");
    }

    #[test]
    fn merge_carries_consumers_forward_and_pushes_them_to_the_running_worker() {
        let (tx, rx) = std::sync::mpsc::channel::<ThreadSendMsg>();
        let mut old_data = state(cfg(0, 0), true, Some(9));
        if let Some(mut s) = old_data.downcast_mut::<CameraWidgetState>() {
            s.control = Some(tx);
            s.thread_id = Some(ThreadId::unique());
        }
        let new_data = CameraWidget::create(cfg(0, 0))
            .with_consumer(FrameConsumer::new(7, 500, 200))
            .dom()
            .root
            .get_dataset()
            .cloned()
            .expect("dataset");

        let mut merged = merge_camera_state(new_data, old_data);
        let s = merged.downcast_ref::<CameraWidgetState>().expect("state");
        assert_eq!(s.consumers.as_ref(), &[FrameConsumer::new(7, 500, 200)]);
        assert!(
            s.started && s.gl_texture_id == Some(9),
            "thread state carries forward"
        );
        drop(s);
        let msg = rx.try_recv().expect("the running worker is told about Bob");
        let ThreadSendMsg::Custom(mut payload) = msg else {
            panic!("a consumer change is a Custom(CaptureTargets) message");
        };
        let t = payload
            .downcast_ref::<CaptureTargets>()
            .expect("CaptureTargets");
        assert_eq!(t.consumers, vec![FrameConsumer::new(7, 500, 200)]);

        // An unchanged rebuild sends nothing (hundreds of relayouts must not
        // spam the worker).
        let same = CameraWidget::create(cfg(0, 0))
            .with_consumer(FrameConsumer::new(7, 500, 200))
            .dom()
            .root
            .get_dataset()
            .cloned()
            .expect("dataset");
        let _ = merge_camera_state(same, merged);
        assert!(rx.try_recv().is_err(), "no change, no message");
    }

    // ------------------------------------------------------------------
    // camera_writeback
    // ------------------------------------------------------------------

    #[test]
    fn writeback_invokes_the_hook_with_the_frame_and_returns_its_update() {
        let mut log = RefAny::new(FrameLog { seen: Vec::new() });
        let mut data = state_with_hook(cfg(2, 2), &log);
        let frame_data = captured(frame(2, 2));

        let (update, _) =
            with_callback_info(|info| camera_writeback(data.clone(), frame_data.clone(), info));

        assert_eq!(update, Update::RefreshDom, "the hook's Update must win");
        assert_eq!(logged_frames(&mut log), vec![(2, 2, 16)]);
        let (_, _, texture, _) = read_state(&mut data);
        assert_eq!(
            texture, None,
            "without a GL context no texture id is ever installed"
        );
    }

    #[test]
    fn writeback_ignores_frame_data_of_the_wrong_type() {
        let mut log = RefAny::new(FrameLog { seen: Vec::new() });
        let mut data = state_with_hook(cfg(2, 2), &log);

        let (update, changes) =
            with_callback_info(|info| camera_writeback(data.clone(), RefAny::new(0_u32), info));

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "no frame -> no image change");
        assert!(
            logged_frames(&mut log).is_empty(),
            "the user hook must not fire without a frame"
        );
    }

    #[test]
    fn writeback_survives_a_writeback_dataset_that_is_not_a_camera_state() {
        let (update, _) = with_callback_info(|info| {
            camera_writeback(RefAny::new(0_u32), captured(frame(1, 1)), info)
        });

        assert_eq!(
            update,
            Update::DoNothing,
            "a foreign dataset means no hook and no texture - but no panic either"
        );
    }

    #[test]
    fn writeback_keeps_a_preexisting_texture_id_on_the_cpu_path() {
        let mut data = state(cfg(2, 2), true, Some(42));
        let frame_data = captured(frame(2, 2));

        let (update, _) =
            with_callback_info(|info| camera_writeback(data.clone(), frame_data.clone(), info));

        assert_eq!(update, Update::DoNothing, "no hook -> no user update");
        let (_, _, texture, _) = read_state(&mut data);
        assert_eq!(texture, Some(42), "the texture id must stay stable");
    }

    #[test]
    fn writeback_rejects_a_frame_whose_bytes_do_not_match_its_dimensions() {
        // A malformed/hostile frame (huge dims, no pixels): the image upload must fail
        // cleanly instead of indexing out of bounds or allocating.
        let mut data = state(cfg(2, 2), true, None);
        let bogus = captured(VideoFrame {
            width: u32::MAX,
            height: 1,
            bytes: Vec::<u8>::new().into(),
        });

        let (update, changes) =
            with_callback_info(|info| camera_writeback(data.clone(), bogus.clone(), info));

        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "a rejected frame must not touch the DOM"
        );
        let (_, _, texture, _) = read_state(&mut data);
        assert_eq!(texture, None);
    }

    // ------------------------------------------------------------------
    // merge_camera_state
    // ------------------------------------------------------------------

    #[test]
    fn merge_takes_the_thread_state_from_old_and_everything_else_from_new() {
        let log = RefAny::new(FrameLog { seen: Vec::new() });
        let new_data = state_with_hook(cfg(1920, 1080), &log);
        let old_data = state(cfg(320, 240), true, Some(9));

        let mut merged = merge_camera_state(new_data, old_data);
        let (config, started, texture, has_hook) = read_state(&mut merged);

        assert_eq!(config, cfg(1920, 1080), "the fresh build's config wins");
        assert!(has_hook, "the fresh build's hook wins");
        assert!(started, "'thread already running' must carry forward");
        assert_eq!(texture, Some(9), "the stable texture id must carry forward");
    }

    #[test]
    fn merge_leaves_the_new_state_alone_when_the_old_one_is_foreign() {
        let new_data = state(cfg(640, 480), false, None);
        let mut merged = merge_camera_state(new_data, RefAny::new(0_u32));

        let (config, started, texture, _) = read_state(&mut merged);
        assert_eq!(config, cfg(640, 480));
        assert!(!started, "nothing to carry forward from a foreign payload");
        assert_eq!(texture, None);
    }

    #[test]
    fn merge_returns_a_foreign_new_dataset_untouched() {
        let old_data = state(cfg(640, 480), true, Some(1));
        let mut merged = merge_camera_state(RefAny::new(77_u32), old_data);

        assert_eq!(
            merged.downcast_ref::<u32>().map(|v| *v),
            Some(77),
            "merge must hand back exactly the payload it was given"
        );
    }

    #[test]
    fn merge_of_a_dataset_with_itself_does_not_panic() {
        // The same RefAny on both sides: the mutable + shared borrow overlap, so the
        // merge is skipped rather than aliasing. Either way the state must survive.
        let mut data = state(cfg(800, 600), true, Some(5));
        let mut merged = merge_camera_state(data.clone(), data.clone());

        let (config, started, texture, _) = read_state(&mut merged);
        assert_eq!(config, cfg(800, 600));
        assert!(started);
        assert_eq!(texture, Some(5));
        assert_eq!(read_state(&mut data), (cfg(800, 600), true, Some(5), false));
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
    fn camera_worker_acknowledges_terminate_within_the_grace_budget() {
        use crate::thread::{Thread, ThreadCallback};

        let t = Thread::create(
            RefAny::new(CameraThreadInit::sized(8, 8)),
            RefAny::new(0_usize),
            ThreadCallback::new(camera_worker),
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
            "camera_worker did not acknowledge TerminateThread within 2000ms — at shutdown it \
             would be DETACHED rather than joined"
        );
    }
}
