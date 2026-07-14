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
//! This tick uses a self-contained **test-pattern** worker (colour cycle, no
//! platform deps); the real AVFoundation/Camera2 worker (dll-side) swaps in
//! later.

use alloc::vec::Vec;

use azul_core::callbacks::Update;
use azul_core::camera::CameraConfig;
use azul_core::dom::{ComponentEventFilter, DatasetMergeCallbackType, Dom, EventFilter};
use azul_core::refany::{OptionRefAny, RefAny};
use azul_core::resources::{ImageRef, RawImageFormat};
use azul_core::task::{ThreadId, ThreadReceiver};

use azul_core::video::VideoFrame;

use super::capture_common::{
    camera_backend, invoke_on_frame, present_frame, OnVideoFrame, OnVideoFrameCallback,
    OptionOnVideoFrame,
};
use crate::callbacks::{Callback, CallbackInfo, CallbackType};
use crate::thread::{
    Thread, ThreadCallback, ThreadReceiveMsg, ThreadSender, ThreadWriteBackMsg, WriteBackCallback,
};

/// Init data handed to the capture worker thread.
struct CameraThreadInit {
    width: u32,
    height: u32,
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
}

impl CameraWidget {
    /// Create a camera widget for the given capture config.
    #[must_use] pub const fn create(config: CameraConfig) -> Self {
        Self {
            config,
            on_frame: OptionOnVideoFrame::None,
        }
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
        let state = CameraWidgetState {
            config: self.config,
            started: false,
            gl_texture_id: None,
            on_frame: self.on_frame,
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
            .with_merge_callback(azul_core::dom::DatasetMergeCallback::from_ptr(merge_camera_state))
            .with_callback(
                EventFilter::Component(ComponentEventFilter::AfterMount),
                dataset,
                Callback::from_ptr(camera_on_after_mount),
            )
    }
}

/// Frame dimensions for a config (0 -> a sane default).
const fn frame_dims(config: &CameraConfig) -> (u32, u32) {
    let w = if config.width > 0 { config.width } else { 640 };
    let h = if config.height > 0 { config.height } else { 480 };
    (w, h)
}

/// `AfterMount`: start the background capture thread exactly once.
extern "C" fn camera_on_after_mount(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let dims = {
        let Some(mut s) = data.downcast_mut::<CameraWidgetState>() else {
            return Update::DoNothing;
        };
        if s.started {
            return Update::DoNothing;
        }
        s.started = true;
        frame_dims(&s.config)
    };

    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(CameraThreadInit {
                width: dims.0,
                height: dims.1,
            }),
            data.clone(),
            ThreadCallback::new(camera_worker),
        ),
    );
    Update::DoNothing
}

/// Background worker (test pattern): a colour-cycling solid frame ~30x/s until
/// the widget unmounts. The real AVFoundation/Camera2 capture loop replaces it.
#[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/counter/fixed-point cast
extern "C" fn camera_worker(mut init: RefAny, mut sender: ThreadSender, _recv: ThreadReceiver) {
    let (w, h) = init
        .downcast_ref::<CameraThreadInit>()
        .map_or((640, 480), |i| (i.width, i.height));

    // Real platform capture if the dll registered a camera backend (v4l2 /
    // AVFoundation / Media Foundation); otherwise the colour-cycle test pattern.
    if let Some(backend) = camera_backend() {
        let handle = (backend.open)(0, w, h);
        if handle != 0 {
            let mut buf: alloc::vec::Vec<u8> = alloc::vec::Vec::new();
            loop {
                let (fw, fh) = (backend.read)(handle, &mut buf);
                if fw == 0 || fh == 0 {
                    break;
                }
                let frame = VideoFrame {
                    width: fw,
                    height: fh,
                    bytes: buf.clone().into(),
                };
                if !sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
                    WriteBackCallback::new(camera_writeback),
                    RefAny::new(frame),
                ))) {
                    break;
                }
            }
            (backend.close)(handle);
            return;
        }
    }

    let px = (w as usize) * (h as usize);
    let mut tick: u32 = 0;
    loop {
        let color = [
            (tick % 256) as u8,
            (tick.wrapping_mul(2) % 256) as u8,
            (tick.wrapping_mul(3) % 256) as u8,
            255u8,
        ];
        let mut bytes = Vec::with_capacity(px * 4);
        for _ in 0..px {
            bytes.extend_from_slice(&color);
        }
        let frame = VideoFrame {
            width: w,
            height: h,
            bytes: bytes.into(),
        };
        let sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
            WriteBackCallback::new(camera_writeback),
            RefAny::new(frame),
        )));
        if !sent {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(33));
        tick = tick.wrapping_add(8);
    }
}

/// Writeback (main thread): hand the frame to the shared GL presenter and
/// store the (stable) texture id back in the widget's state.
extern "C" fn camera_writeback(
    mut writeback_data: RefAny,
    mut frame_data: RefAny,
    mut info: CallbackInfo,
) -> Update {
    let (current, hook) = writeback_data.downcast_ref::<CameraWidgetState>().map_or_else(|| (None, OptionOnVideoFrame::None), |s| (s.gl_texture_id, s.on_frame.clone()));
    let mut user_update = Update::DoNothing;
    let new_id = match frame_data.downcast_ref::<VideoFrame>() {
        Some(frame) => {
            let id = present_frame(&mut info, writeback_data.clone(), current, &frame);
            user_update = invoke_on_frame(&hook, &mut info, &frame);
            id
        }
        None => return Update::DoNothing,
    };
    if let Some(mut s) = writeback_data.downcast_mut::<CameraWidgetState>() {
        s.gl_texture_id = new_id;
    }
    user_update
}

/// Carry live state forward across relayout (config from the fresh build,
/// thread / texture from the previous frame).
extern "C" fn merge_camera_state(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
    {
        let new_guard = new_data.downcast_mut::<CameraWidgetState>();
        let old_guard = old_data.downcast_ref::<CameraWidgetState>();
        if let (Some(mut new_g), Some(old_g)) = (new_guard, old_guard) {
            new_g.started = old_g.started;
            new_g.gl_texture_id = old_g.gl_texture_id;
        }
    }
    new_data
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

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        thread::{ThreadSendCallback, ThreadSenderDestructorCallback, ThreadSenderInner},
        widgets::capture_common::OnVideoFrameCallbackType,
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
            log.seen.push((frame.width, frame.height, frame.bytes.as_ref().len()));
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
    extern "C" fn record_and_stop(_sender: *const core::ffi::c_void, msg: ThreadReceiveMsg) -> bool {
        if let ThreadReceiveMsg::WriteBack(mut wb) = msg {
            if let Some(f) = wb.refany.downcast_ref::<VideoFrame>() {
                let bytes = f.bytes.as_ref();
                let tick0 = bytes
                    .chunks_exact(4)
                    .all(|px| px == &[0u8, 0, 0, 255][..]);
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
            send_fn: ThreadSendCallback { cb: record_and_stop },
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
        assert_eq!(frame_dims(&cfg(1280, 0)), (1280, 480), "only height defaults");
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
                assert_eq!(widget.config, config, "create must not normalise the config");
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

        assert_eq!(widget.config, config, "the builder must not touch the config");
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
            hook.callback.cb as usize,
            frame_do_nothing as OnVideoFrameCallbackType as usize,
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
        let (w, h, format, _) = placeholder_of(&CameraWidget::create(cfg(u32::MAX, u32::MAX)).dom());
        assert_eq!((w, h), (u32::MAX as usize, u32::MAX as usize));
        assert_eq!(format, RawImageFormat::BGRA8);
    }

    #[test]
    fn dom_wires_exactly_one_after_mount_callback_a_dataset_and_a_merge_callback() {
        let dom = CameraWidget::create(cfg(64, 48)).dom();

        assert_eq!(dom.children.as_ref().len(), 0, "the widget is a single node");

        let callbacks = dom.root.get_callbacks();
        assert_eq!(
            callbacks.as_ref().len(),
            1,
            "exactly one callback: the AfterMount capture-thread starter"
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
        let (update, changes) = with_callback_info(|info| camera_on_after_mount(data.clone(), info));

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
        let Some(sent) = run_worker(RefAny::new(CameraThreadInit {
            width: 2,
            height: 3,
        })) else {
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
        assert_eq!((w, h), (640, 480), "a bad init must not panic - it defaults");
        assert_eq!(bytes, 640 * 480 * 4);
    }

    #[test]
    fn worker_with_zero_dims_sends_an_empty_frame_instead_of_hanging() {
        // camera_on_after_mount always routes through frame_dims, but the worker itself
        // does not - a 0x0 init must still terminate and emit a well-formed empty frame.
        let Some(sent) = run_worker(RefAny::new(CameraThreadInit {
            width: 0,
            height: 0,
        })) else {
            return;
        };

        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0], (0, 0, 0, true));
    }

    // ------------------------------------------------------------------
    // camera_writeback
    // ------------------------------------------------------------------

    #[test]
    fn writeback_invokes_the_hook_with_the_frame_and_returns_its_update() {
        let mut log = RefAny::new(FrameLog { seen: Vec::new() });
        let mut data = state_with_hook(cfg(2, 2), &log);
        let frame_data = RefAny::new(frame(2, 2));

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

        let (update, changes) = with_callback_info(|info| {
            camera_writeback(data.clone(), RefAny::new(0_u32), info)
        });

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
            camera_writeback(RefAny::new(0_u32), RefAny::new(frame(1, 1)), info)
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
        let frame_data = RefAny::new(frame(2, 2));

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
        let bogus = RefAny::new(VideoFrame {
            width: u32::MAX,
            height: 1,
            bytes: Vec::<u8>::new().into(),
        });

        let (update, changes) =
            with_callback_info(|info| camera_writeback(data.clone(), bogus.clone(), info));

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "a rejected frame must not touch the DOM");
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
}
