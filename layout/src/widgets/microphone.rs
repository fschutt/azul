//! Microphone-capture widget (SUPER_PLAN_2 §4 P7) - a "dumb widget" with the
//! same architecture as the camera/screencap/video widgets, only the medium is
//! audio (no GL texture).
//!
//! `MicrophoneWidget::create(config).with_on_frame(data, cb).dom()` yields an
//! invisible node that, on `AfterMount`, starts a background capture thread.
//! Each captured [`AudioFrame`] flows through the writeback to the user's
//! `on_frame` hook (the backreference DI pattern), so app code can save,
//! process, or **send** the audio over the network (the azul-meet audio seam) -
//! all via the public API, no globals. The mic permission is the existing
//! `Capability::Microphone`.
//!
//! This tick uses a self-contained **test-tone** worker (a 440 Hz sine, no
//! platform deps); the real AVAudioEngine / AAudio / cpal capture worker
//! (dll-side) swaps in later.

use alloc::vec::Vec;

use azul_core::audio::{AudioConfig, AudioFrame};

use super::capture_common::{mic_backend, terminate_requested};
use azul_core::callbacks::Update;
use azul_core::dom::{ComponentEventFilter, DatasetMergeCallbackType, Dom, EventFilter};
use azul_core::refany::{OptionRefAny, RefAny};
use azul_core::task::{ThreadId, ThreadReceiver};
use azul_css::impl_option_inner; // for impl_widget_callback!'s impl_option!
use azul_css::F32Vec;

use crate::callbacks::{Callback, CallbackInfo, CallbackType};
use crate::thread::{
    Thread, ThreadCallback, ThreadReceiveMsg, ThreadSender, ThreadWriteBackMsg, WriteBackCallback,
};

// --- User hook: on_frame (backreference DI, FFI-exposed) ---

/// User hook fired once per captured audio chunk - the backreference DI pattern
/// (see `architecture.md`).
///
/// The widget's private writeback invokes it with each
/// [`AudioFrame`] so application code can save it, apply effects, or send it
/// over the network (azul-meet). Returns `Update` like any callback. Wired via
/// [`MicrophoneWidget::with_on_frame`].
pub type OnAudioFrameCallbackType = extern "C" fn(RefAny, CallbackInfo, AudioFrame) -> Update;
impl_widget_callback!(
    OnAudioFrame,
    OptionOnAudioFrame,
    OnAudioFrameCallback,
    OnAudioFrameCallbackType
);

// Host-invoker plumbing for managed-FFI bindings - see core/src/host_invoker.rs.
azul_core::impl_managed_callback! {
    wrapper:        OnAudioFrameCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: ON_AUDIO_FRAME_INVOKER,
    invoker_ty:     AzOnAudioFrameCallbackInvoker,
    thunk_fn:       az_on_audio_frame_callback_thunk,
    setter_fn:      AzApp_setOnAudioFrameCallbackInvoker,
    from_handle_fn: AzOnAudioFrameCallback_createFromHostHandle,
    extra_args:     [ frame: AudioFrame ],
}

/// Invoke the optional `on_frame` hook with `frame`, returning the user's
/// `Update` (`DoNothing` when no hook is set).
fn invoke_on_audio_frame(
    hook: &OptionOnAudioFrame,
    info: &CallbackInfo,
    frame: AudioFrame,
) -> Update {
    match hook {
        OptionOnAudioFrame::Some(h) => (h.callback.cb)(h.refany.clone(), *info, frame),
        OptionOnAudioFrame::None => Update::DoNothing,
    }
}

/// Init data handed to the capture worker thread.
struct MicThreadInit {
    sample_rate: u32,
    channels: u16,
}

/// Live state for one microphone widget, carried across relayout by
/// [`merge_microphone_state`].
#[derive(Debug)]
pub struct MicrophoneWidgetState {
    /// The requested capture configuration (rate + channels).
    pub config: AudioConfig,
    /// `true` once the capture thread has been started.
    pub started: bool,
    /// Optional user hook invoked with each captured frame (save / effects /
    /// send). Re-set on every fresh build (see [`merge_microphone_state`]).
    pub on_frame: OptionOnAudioFrame,
}

/// A microphone-capture widget. `create(config).with_on_frame(..).dom()` yields
/// an invisible node a background capture thread feeds.
#[repr(C)]
#[derive(Debug)]
pub struct MicrophoneWidget {
    /// Requested capture config (sample rate, channels).
    pub config: AudioConfig,
    /// Optional per-frame user hook (save / effects / send - azul-meet).
    pub on_frame: OptionOnAudioFrame,
}

impl MicrophoneWidget {
    /// Create a microphone widget for the given capture config.
    #[must_use] pub const fn create(config: AudioConfig) -> Self {
        Self {
            config,
            on_frame: OptionOnAudioFrame::None,
        }
    }

    /// Set a hook invoked with every captured audio chunk - for saving,
    /// effects, or sending over the network (azul-meet). The backreference DI
    /// pattern (see `architecture.md`).
    pub fn set_on_frame<C: Into<OnAudioFrameCallback>>(&mut self, data: RefAny, on_frame: C) {
        self.on_frame = Some(OnAudioFrame {
            refany: data,
            callback: on_frame.into(),
        })
        .into();
    }

    /// Builder form of [`set_on_frame`](Self::set_on_frame).
    #[must_use]
    pub fn with_on_frame<C: Into<OnAudioFrameCallback>>(
        mut self,
        data: RefAny,
        on_frame: C,
    ) -> Self {
        self.set_on_frame(data, on_frame);
        self
    }

    /// Build the widget's DOM: a single invisible node, fed by a background
    /// capture thread started on mount. Place it anywhere in your tree - the
    /// capture lives as long as the node is mounted (unmount stops it).
    #[must_use] pub fn dom(self) -> Dom {
        let state = MicrophoneWidgetState {
            config: self.config,
            started: false,
            on_frame: self.on_frame,
        };
        let dataset = RefAny::new(state);

        Dom::create_div()
            .with_dataset(OptionRefAny::Some(dataset.clone()))
            .with_merge_callback(azul_core::dom::DatasetMergeCallback::from_ptr(merge_microphone_state))
            .with_callback(
                EventFilter::Component(ComponentEventFilter::AfterMount),
                dataset,
                Callback::from_ptr(mic_on_after_mount),
            )
    }
}

/// `AfterMount`: start the background capture thread exactly once.
extern "C" fn mic_on_after_mount(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let (rate, channels) = {
        let Some(mut s) = data.downcast_mut::<MicrophoneWidgetState>() else {
            return Update::DoNothing;
        };
        if s.started {
            return Update::DoNothing;
        }
        s.started = true;
        let rate = if s.config.sample_rate > 0 {
            s.config.sample_rate
        } else {
            48_000
        };
        let channels = s.config.channels.max(1);
        (rate, channels)
    };

    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(MicThreadInit {
                sample_rate: rate,
                channels,
            }),
            data.clone(),
            ThreadCallback::new(mic_worker),
        ),
    );
    Update::DoNothing
}

/// Background worker (test tone): a 440 Hz sine in ~20 ms chunks until the
/// widget unmounts. The real `AVAudioEngine` / `AAudio` / cpal capture loop
/// replaces it (dll-side).
#[allow(clippy::cast_precision_loss)] // bounded graphics/coord/counter/fixed-point cast
extern "C" fn mic_worker(
    mut init: RefAny,
    mut sender: ThreadSender,
    mut recv: ThreadReceiver,
) {
    let (rate, channels) = init
        .downcast_ref::<MicThreadInit>()
        .map_or((48_000, 1), |i| (i.sample_rate, i.channels));

    // Real platform capture if the dll registered a mic backend (ALSA on
    // Linux); otherwise the 440 Hz test tone below.
    if let Some(backend) = mic_backend() {
        let handle = (backend.open)(rate, channels);
        if handle != 0 {
            let mut buf: Vec<f32> = Vec::new();
            loop {
                // See `capture_common::terminate_requested`: an ALSA read is
                // not interruptible from the terminate channel, so the check
                // has to happen between reads.
                if terminate_requested(&mut recv) {
                    break;
                }
                let frames = (backend.read)(handle, &mut buf);
                if frames == 0 {
                    break;
                }
                let frame = AudioFrame {
                    sample_rate: rate,
                    channels,
                    samples: F32Vec::from_vec(buf.clone()),
                };
                if !sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
                    WriteBackCallback::new(mic_writeback),
                    RefAny::new(frame),
                ))) {
                    break;
                }
            }
            (backend.close)(handle);
            return;
        }
    }

    // Reaching here means a MicrophoneWidget is live and about to feed a
    // synthetic 440 Hz TEST TONE instead of the microphone — the most
    // misleading fallback in the tree if unannounced. Say why, once.
    {
        static TEST_TONE_ANNOUNCE: std::sync::Once = std::sync::Once::new();
        let have_backend = mic_backend().is_some();
        TEST_TONE_ANNOUNCE.call_once(|| {
            if have_backend {
                eprintln!(
                    "[azul][microphone] the platform microphone backend failed to open \
                     (device missing/busy or libasound unavailable — see lines above) \
                     — feeding a synthetic 440 Hz TEST TONE instead of the microphone"
                );
            } else {
                eprintln!(
                    "[azul][microphone] no microphone backend is registered in this \
                     build/OS — feeding a synthetic 440 Hz TEST TONE instead of the \
                     microphone"
                );
            }
        });
    }

    let frames_per_chunk = (rate as usize / 50).max(1); // ~20 ms
    let step = 2.0 * core::f32::consts::PI * 440.0 / rate as f32;
    let mut phase: f32 = 0.0;
    loop {
        if terminate_requested(&mut recv) {
            break;
        }
        let mut samples = Vec::with_capacity(frames_per_chunk * channels as usize);
        for _ in 0..frames_per_chunk {
            let s = phase.sin() * 0.2;
            phase += step;
            if phase > 2.0 * core::f32::consts::PI {
                phase -= 2.0 * core::f32::consts::PI;
            }
            for _ in 0..channels {
                samples.push(s);
            }
        }
        let frame = AudioFrame {
            sample_rate: rate,
            channels,
            samples: F32Vec::from_vec(samples),
        };
        let sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
            WriteBackCallback::new(mic_writeback),
            RefAny::new(frame),
        )));
        if !sent {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

/// Writeback (main thread): hand the captured frame to the user's `on_frame`
/// hook. No GL - audio has no texture.
extern "C" fn mic_writeback(
    mut writeback_data: RefAny,
    mut frame_data: RefAny,
    info: CallbackInfo,
) -> Update {
    let hook = match writeback_data.downcast_ref::<MicrophoneWidgetState>() {
        Some(s) => s.on_frame.clone(),
        None => return Update::DoNothing,
    };
    frame_data.downcast_ref::<AudioFrame>().map_or(Update::DoNothing, |frame| invoke_on_audio_frame(&hook, &info, frame.clone()))
}

/// Carry live state forward across relayout (config + started; the `on_frame`
/// hook is taken from the fresh build).
extern "C" fn merge_microphone_state(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
    {
        let new_guard = new_data.downcast_mut::<MicrophoneWidgetState>();
        let old_guard = old_data.downcast_ref::<MicrophoneWidgetState>();
        if let (Some(mut new_g), Some(old_g)) = (new_guard, old_guard) {
            new_g.started = old_g.started;
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
        dom::{DomId, DomNodeId, NodeType},
        geom::OptionLogicalPosition,
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        resources::RendererResources,
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
        thread::{
            ThreadSendCallback, ThreadSenderDestructorCallback, ThreadSenderInner,
            WriteBackCallbackType,
        },
        window::LayoutWindow,
        window_state::FullWindowState,
    };

    // ------------------------------------------------------------------
    // Helpers
    // ------------------------------------------------------------------

    /// An `AudioConfig` with the given rate + channel count.
    const fn cfg(sample_rate: u32, channels: u16) -> AudioConfig {
        AudioConfig {
            sample_rate,
            channels,
        }
    }

    /// An interleaved `AudioFrame`. `AudioFrame` has no `Default`, so every test
    /// spells out its rate / channel count / samples.
    fn frame(sample_rate: u32, channels: u16, samples: Vec<f32>) -> AudioFrame {
        AudioFrame {
            sample_rate,
            channels,
            samples: F32Vec::from_vec(samples),
        }
    }

    /// A `MicrophoneWidgetState` payload with no `on_frame` hook.
    fn state(config: AudioConfig, started: bool) -> RefAny {
        RefAny::new(MicrophoneWidgetState {
            config,
            started,
            on_frame: OptionOnAudioFrame::None,
        })
    }

    /// `(config, started, has_hook)` of a `MicrophoneWidgetState` payload.
    fn read_state(data: &mut RefAny) -> (AudioConfig, bool, bool) {
        let s = data
            .downcast_ref::<MicrophoneWidgetState>()
            .expect("payload must still be a MicrophoneWidgetState");
        (
            s.config,
            s.started,
            matches!(s.on_frame, OptionOnAudioFrame::Some(_)),
        )
    }

    // ---- frame hook -------------------------------------------------------

    /// Records every frame a widget's `on_frame` hook is handed, verbatim.
    struct FrameLog {
        seen: Vec<(u32, u16, Vec<f32>)>,
    }

    extern "C" fn record_frame(mut data: RefAny, _: CallbackInfo, frame: AudioFrame) -> Update {
        if let Some(mut log) = data.downcast_mut::<FrameLog>() {
            log.seen.push((
                frame.sample_rate,
                frame.channels,
                frame.samples.as_ref().to_vec(),
            ));
        }
        Update::RefreshDom
    }

    extern "C" fn frame_do_nothing(_: RefAny, _: CallbackInfo, _: AudioFrame) -> Update {
        Update::DoNothing
    }

    /// The frames recorded by a `FrameLog` payload.
    fn logged_frames(data: &mut RefAny) -> Vec<(u32, u16, Vec<f32>)> {
        data.downcast_ref::<FrameLog>()
            .expect("payload must still be a FrameLog")
            .seen
            .clone()
    }

    fn new_log() -> RefAny {
        RefAny::new(FrameLog { seen: Vec::new() })
    }

    /// An `on_frame` hook that records into `log`.
    fn hook_into(log: &RefAny) -> OptionOnAudioFrame {
        Some(OnAudioFrame {
            refany: log.clone(),
            callback: (record_frame as OnAudioFrameCallbackType).into(),
        })
        .into()
    }

    /// A `MicrophoneWidgetState` whose `on_frame` hook writes into `log`.
    fn state_with_hook(config: AudioConfig, started: bool, log: &RefAny) -> RefAny {
        RefAny::new(MicrophoneWidgetState {
            config,
            started,
            on_frame: hook_into(log),
        })
    }

    // ---- CallbackInfo harness --------------------------------------------

    /// Runs `f` against a real `CallbackInfo` over an empty `LayoutWindow` (no GL
    /// context). Returns `f`'s value plus every `CallbackChange` the callback
    /// recorded.
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

    // ---- mic_worker harness ----------------------------------------------

    /// One frame `mic_worker` handed to its sender.
    #[derive(Debug, Clone, PartialEq)]
    struct SentFrame {
        sample_rate: u32,
        channels: u16,
        samples: Vec<f32>,
        /// The writeback fn pointer the worker attached, as an address.
        writeback: usize,
    }

    /// Everything `mic_worker` pushed. Guarded by `WORKER_GATE` - the worker's send
    /// callback is a plain C fn pointer, so it has nowhere else to put its result.
    static WORKER_LOG: Mutex<Vec<SentFrame>> = Mutex::new(Vec::new());
    static WORKER_GATE: Mutex<()> = Mutex::new(());

    /// Records the frame, then reports the send as *failed* - i.e. "the main thread
    /// is gone", the only signal `mic_worker` has to stop. A worker that ignored it
    /// would hang this test forever (the tone loop is unbounded).
    extern "C" fn record_and_stop(
        _sender: *const core::ffi::c_void,
        msg: ThreadReceiveMsg,
    ) -> bool {
        if let ThreadReceiveMsg::WriteBack(mut wb) = msg {
            let writeback = wb.callback.cb as usize;
            if let Some(f) = wb.refany.downcast_ref::<AudioFrame>() {
                WORKER_LOG
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(SentFrame {
                        sample_rate: f.sample_rate,
                        channels: f.channels,
                        samples: f.samples.as_ref().to_vec(),
                        writeback,
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

    /// A `ThreadReceiver` that never delivers anything (`mic_worker` ignores it).
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

    /// Runs `mic_worker` with `init` against a sender that rejects the first frame.
    ///
    /// Returns `(frames the worker managed to send, took_the_test_tone_path)`. The
    /// flag is read *after* the run on purpose: `MIC_BACKEND` is a `OnceLock`, so
    /// "still unregistered afterwards" proves it was unregistered *during* the run
    /// — and another test in this binary (`capture_common`) may register one at any
    /// time. Assertions about the 440 Hz tone are gated on it; the path-independent
    /// invariants are always checked.
    fn run_worker(init: RefAny) -> (Vec<SentFrame>, bool) {
        let _gate = WORKER_GATE
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        WORKER_LOG
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();

        let (_rx, sender) = stopped_sender();
        let (_tx, receiver) = silent_receiver();
        mic_worker(init, sender, receiver);

        let sent = WORKER_LOG
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone();
        (sent, mic_backend().is_none())
    }

    /// The tone worker's chunk length: ~20 ms of interleaved samples, never empty
    /// in the frame dimension.
    fn expected_chunk_len(sample_rate: u32, channels: u16) -> usize {
        (sample_rate as usize / 50).max(1) * channels as usize
    }

    // ------------------------------------------------------------------
    // invoke_on_audio_frame
    // ------------------------------------------------------------------

    #[test]
    fn invoke_without_a_hook_is_donothing_even_for_a_degenerate_frame() {
        let (update, _) = with_callback_info(|info| {
            // 0 channels + huge rate + no samples: nothing may divide by the channel
            // count or index the (empty) sample buffer.
            invoke_on_audio_frame(
                &OptionOnAudioFrame::None,
                &info,
                frame(u32::MAX, 0, Vec::new()),
            )
        });
        assert_eq!(update, Update::DoNothing);
    }

    #[test]
    fn invoke_forwards_the_frame_verbatim_and_returns_the_hooks_update() {
        let mut log = new_log();
        let hook = hook_into(&log);
        let samples = vec![-1.0_f32, 0.0, 1.0, 0.5];

        let (update, _) = with_callback_info(|info| {
            invoke_on_audio_frame(&hook, &info, frame(44_100, 2, samples.clone()))
        });

        assert_eq!(update, Update::RefreshDom, "the hook's Update must win");
        assert_eq!(logged_frames(&mut log), vec![(44_100, 2, samples)]);
    }

    #[test]
    fn invoke_passes_nan_infinite_and_negative_zero_samples_through_untouched() {
        // The widget is a transport, not a filter: hostile float payloads must reach
        // the user hook exactly as captured, with no normalisation and no panic.
        let mut log = new_log();
        let hook = hook_into(&log);
        let hostile = vec![f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -0.0, f32::MIN, f32::MAX];

        let (update, _) =
            with_callback_info(|info| invoke_on_audio_frame(&hook, &info, frame(0, 1, hostile)));

        assert_eq!(update, Update::RefreshDom);
        let seen = logged_frames(&mut log);
        assert_eq!(seen.len(), 1);
        let (rate, channels, samples) = &seen[0];
        assert_eq!((*rate, *channels), (0, 1), "a 0 Hz frame is forwarded as-is");
        assert!(samples[0].is_nan(), "NaN must not be normalised");
        assert_eq!(samples[1], f32::INFINITY);
        assert_eq!(samples[2], f32::NEG_INFINITY);
        assert!(
            samples[3] == 0.0 && samples[3].is_sign_negative(),
            "-0.0 must keep its sign bit"
        );
        assert_eq!(samples[4], f32::MIN);
        assert_eq!(samples[5], f32::MAX);
    }

    #[test]
    fn invoke_forwards_a_frame_whose_sample_count_contradicts_its_channel_count() {
        // 65535 channels but 3 samples: `frame_count()` must floor to 0 rather than
        // divide by zero or wrap, and the hook still sees the frame.
        let mut log = new_log();
        let hook = hook_into(&log);
        let bogus = frame(u32::MAX, u16::MAX, vec![0.1, 0.2, 0.3]);
        assert_eq!(bogus.frame_count(), 0);

        let (update, _) = with_callback_info(|info| invoke_on_audio_frame(&hook, &info, bogus));

        assert_eq!(update, Update::RefreshDom);
        assert_eq!(logged_frames(&mut log), vec![(u32::MAX, u16::MAX, vec![0.1, 0.2, 0.3])]);
    }

    // ------------------------------------------------------------------
    // MicrophoneWidget::create / set_on_frame / with_on_frame
    // ------------------------------------------------------------------

    #[test]
    fn create_stores_the_config_verbatim_and_leaves_the_hook_unset() {
        for (rate, channels) in [
            (0, 0),
            (1, 1),
            (48_000, 2),
            (u32::MAX, u16::MAX),
            (u32::MAX, 0),
            (0, u16::MAX),
        ] {
            let widget = MicrophoneWidget::create(cfg(rate, channels));
            assert_eq!(
                widget.config,
                cfg(rate, channels),
                "create must not normalise the config"
            );
            assert!(
                matches!(widget.on_frame, OptionOnAudioFrame::None),
                "a fresh widget has no frame hook"
            );
        }

        let default = MicrophoneWidget::create(AudioConfig::default());
        assert_eq!(default.config, cfg(48_000, 1));
    }

    #[test]
    fn with_on_frame_installs_the_hook_keeps_the_config_and_shares_the_user_data() {
        let data = new_log();
        let widget = MicrophoneWidget::create(cfg(u32::MAX, u16::MAX))
            .with_on_frame(data.clone(), record_frame as OnAudioFrameCallbackType);

        assert_eq!(
            widget.config,
            cfg(u32::MAX, u16::MAX),
            "the builder must not touch the config"
        );
        let OptionOnAudioFrame::Some(hook) = &widget.on_frame else {
            panic!("with_on_frame must install a hook");
        };
        assert_eq!(
            hook.callback.cb as usize,
            record_frame as OnAudioFrameCallbackType as usize
        );
        assert_eq!(
            hook.refany, data,
            "the widget must hold the caller's RefAny, not a fresh allocation"
        );
    }

    #[test]
    fn set_on_frame_twice_keeps_only_the_last_hook_and_releases_the_first_payload() {
        let first = new_log();
        let second = new_log();
        let mut widget = MicrophoneWidget::create(cfg(8_000, 1));
        widget.set_on_frame(first.clone(), record_frame as OnAudioFrameCallbackType);
        widget.set_on_frame(second.clone(), frame_do_nothing as OnAudioFrameCallbackType);

        let OptionOnAudioFrame::Some(hook) = &widget.on_frame else {
            panic!("hook must still be set");
        };
        assert_eq!(
            hook.callback.cb as usize,
            frame_do_nothing as OnAudioFrameCallbackType as usize,
            "the second set_on_frame must replace the first"
        );
        assert_eq!(hook.refany, second);
        assert_ne!(hook.refany, first, "the first payload must have been dropped");
        assert_eq!(widget.config, cfg(8_000, 1));
    }

    #[test]
    fn set_on_frame_accepts_the_same_refany_for_both_hooks() {
        // Re-registering the *same* payload must not free it (a double-drop would
        // show up as a corrupt downcast here).
        let mut data = new_log();
        let mut widget = MicrophoneWidget::create(cfg(48_000, 2));
        widget.set_on_frame(data.clone(), record_frame as OnAudioFrameCallbackType);
        widget.set_on_frame(data.clone(), record_frame as OnAudioFrameCallbackType);

        assert!(matches!(widget.on_frame, OptionOnAudioFrame::Some(_)));
        assert!(logged_frames(&mut data).is_empty());
    }

    // ------------------------------------------------------------------
    // MicrophoneWidget::dom
    // ------------------------------------------------------------------

    #[test]
    fn dom_is_one_div_with_one_after_mount_callback_a_dataset_and_a_merge_callback() {
        let dom = MicrophoneWidget::create(cfg(48_000, 2)).dom();

        assert_eq!(dom.root.get_node_type(), &NodeType::Div);
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
        assert_eq!(
            callbacks.as_ref()[0].callback.cb,
            mic_on_after_mount as CallbackType as usize
        );

        let merge = dom
            .root
            .get_merge_callback()
            .expect("state must survive relayout");
        assert_eq!(
            merge.cb as usize,
            merge_microphone_state as DatasetMergeCallbackType as usize
        );

        let mut dataset = dom
            .root
            .get_dataset()
            .cloned()
            .expect("the node must carry its MicrophoneWidgetState");
        assert_eq!(read_state(&mut dataset), (cfg(48_000, 2), false, false));
    }

    #[test]
    fn dom_shares_one_state_between_the_dataset_and_the_after_mount_callback() {
        let dom = MicrophoneWidget::create(cfg(48_000, 1)).dom();
        let mut dataset = dom.root.get_dataset().cloned().expect("dataset");
        let mut callback_data = dom.root.get_callbacks().as_ref()[0].refany.clone();

        assert_eq!(
            callback_data, dataset,
            "AfterMount must see the very state the dataset carries"
        );

        {
            let mut s = dataset
                .downcast_mut::<MicrophoneWidgetState>()
                .expect("state");
            s.started = true;
        }
        assert!(
            read_state(&mut callback_data).1,
            "a write through the dataset must be visible to the callback"
        );
    }

    #[test]
    fn dom_carries_an_extreme_config_and_the_hook_into_the_state_unnormalised() {
        // Normalisation (0 -> 48 kHz, channels.max(1)) happens on AfterMount, not at
        // build time - the state must record exactly what the user asked for.
        for (rate, channels) in [(0, 0), (1, u16::MAX), (u32::MAX, 1)] {
            let dom = MicrophoneWidget::create(cfg(rate, channels))
                .with_on_frame(new_log(), record_frame as OnAudioFrameCallbackType)
                .dom();
            let mut dataset = dom.root.get_dataset().cloned().expect("dataset");
            assert_eq!(read_state(&mut dataset), (cfg(rate, channels), false, true));
        }
    }

    #[test]
    fn dom_built_twice_yields_two_independent_states() {
        let a = MicrophoneWidget::create(cfg(8_000, 1)).dom();
        let b = MicrophoneWidget::create(cfg(8_000, 1)).dom();
        let mut a_ds = a.root.get_dataset().cloned().expect("dataset");
        let mut b_ds = b.root.get_dataset().cloned().expect("dataset");

        assert_ne!(a_ds, b_ds, "two widgets must not share one capture state");
        {
            let mut s = a_ds.downcast_mut::<MicrophoneWidgetState>().expect("state");
            s.started = true;
        }
        assert!(!read_state(&mut b_ds).1, "the second widget is untouched");
    }

    // ------------------------------------------------------------------
    // mic_on_after_mount
    //
    // NOTE: the *first* mount is deliberately not exercised - it spawns a real
    // capture thread, and `ThreadInner`'s destructor joins that thread while its
    // receiver is still alive. `mic_worker` never reads its receiver and only
    // stops when a send fails, so the join would hang the test binary forever
    // (see the report). Only the guard paths below can be driven safely.
    // ------------------------------------------------------------------

    #[test]
    fn after_mount_ignores_a_dataset_that_is_not_a_microphone_state() {
        let (update, changes) =
            with_callback_info(|info| mic_on_after_mount(RefAny::new(0_u32), info));

        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "a foreign dataset must not start a capture thread"
        );
    }

    #[test]
    fn after_mount_is_a_no_op_once_the_capture_thread_has_started() {
        let mut data = state(cfg(0, 0), true);
        let (update, changes) = with_callback_info(|info| mic_on_after_mount(data.clone(), info));

        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "AfterMount must start the capture thread at most once"
        );
        assert_eq!(
            read_state(&mut data),
            (cfg(0, 0), true, false),
            "a re-mount must not rewrite the state"
        );
    }

    #[test]
    fn after_mount_starts_nothing_while_the_state_is_borrowed_elsewhere() {
        // A live shared borrow makes `downcast_mut` fail. The guard must bail out
        // (no thread, no panic) instead of unwrapping.
        let data = state(cfg(48_000, 2), false);
        let mut probe = data.clone();
        let guard = probe
            .downcast_ref::<MicrophoneWidgetState>()
            .expect("shared borrow");

        let (update, changes) = with_callback_info(|info| mic_on_after_mount(data.clone(), info));

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "a borrowed state must not be mounted");
        assert!(!guard.started, "the state must still be untouched");
        drop(guard);

        let mut after = data;
        assert_eq!(read_state(&mut after), (cfg(48_000, 2), false, false));
    }

    // ------------------------------------------------------------------
    // mic_worker
    // ------------------------------------------------------------------

    #[test]
    fn worker_stops_after_the_first_rejected_send_and_tags_frames_with_its_init() {
        let (sent, tone_path) = run_worker(RefAny::new(MicThreadInit {
            sample_rate: 8_000,
            channels: 2,
        }));

        // Path-independent: a rejected send stops the loop, and every frame carries
        // the requested format plus the mic writeback.
        assert!(
            sent.len() <= 1,
            "the worker must stop after the first rejected send, not spin"
        );
        for f in &sent {
            assert_eq!((f.sample_rate, f.channels), (8_000, 2));
            assert_eq!(f.writeback, mic_writeback as WriteBackCallbackType as usize);
        }

        if !tone_path {
            return; // a platform backend is registered: not the test tone
        }
        assert_eq!(sent.len(), 1);
        let samples = &sent[0].samples;
        assert_eq!(
            samples.len(),
            expected_chunk_len(8_000, 2),
            "~20 ms of interleaved stereo at 8 kHz"
        );
        assert!(
            samples.iter().all(|s| s.is_finite() && s.abs() <= 0.2),
            "the tone must stay finite and inside +/-0.2"
        );
        assert_eq!(samples[0], 0.0, "the tone starts at phase 0");
        for pair in samples.chunks_exact(2) {
            assert_eq!(pair[0], pair[1], "both channels carry the same mono tone");
        }
    }

    #[test]
    fn worker_with_a_foreign_init_falls_back_to_48khz_mono() {
        let (sent, tone_path) = run_worker(RefAny::new(0_u64));

        for f in &sent {
            assert_eq!(
                (f.sample_rate, f.channels),
                (48_000, 1),
                "a bad init must not panic - it defaults"
            );
        }
        if !tone_path {
            return;
        }
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].samples.len(), expected_chunk_len(48_000, 1));
    }

    #[test]
    fn worker_with_a_zero_sample_rate_emits_one_finite_chunk_instead_of_dividing_by_zero() {
        // rate 0 makes the phase step `2*PI*440/0.0` = +inf. The chunk length still
        // has to clamp to >= 1 frame, the emitted samples still have to be finite,
        // and the worker still has to terminate.
        let (sent, tone_path) = run_worker(RefAny::new(MicThreadInit {
            sample_rate: 0,
            channels: 1,
        }));

        // Path-independent: whatever produced the frame, it carries the requested
        // format.
        for f in &sent {
            assert_eq!((f.sample_rate, f.channels), (0, 1));
        }
        if !tone_path {
            return;
        }
        // Tone-path only, like every sibling worker test. `capture_common`'s
        // `register_mic_backend_is_first_wins_and_passes_f32_samples_through`
        // installs a process-wide `OnceLock` backend whose `read` deliberately
        // yields `[NaN, inf, -inf, -0.0]` to prove the vtable passes samples
        // through untouched. Once that test has run, `mic_worker` takes the
        // backend branch and never evaluates a phase step at all — so asserting
        // finiteness ABOVE the gate made this test fail depending on which other
        // test happened to run first in the same process. (It passed under
        // `cargo nextest`, which forks per test, and failed under `cargo test`,
        // which does not.)
        assert!(
            sent.iter().all(|f| f.samples.iter().all(|s| s.is_finite())),
            "an infinite phase step must not leak NaN/inf into the samples"
        );
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].samples, vec![0.0_f32], "one frame, at phase 0");
    }

    #[test]
    fn worker_with_zero_channels_emits_an_empty_chunk_and_stops() {
        let (sent, tone_path) = run_worker(RefAny::new(MicThreadInit {
            sample_rate: 48_000,
            channels: 0,
        }));

        for f in &sent {
            assert_eq!(f.channels, 0);
        }
        if !tone_path {
            return;
        }
        assert_eq!(sent.len(), 1);
        assert!(
            sent[0].samples.is_empty(),
            "0 channels interleaves 0 samples per frame"
        );
        // The frame such a worker produces must still be safe to inspect.
        assert_eq!(frame(48_000, 0, sent[0].samples.clone()).frame_count(), 0);
    }

    #[test]
    fn worker_chunk_length_clamps_to_one_frame_for_sub_50hz_rates() {
        // `rate / 50` truncates to 0 below 50 Hz; without the `.max(1)` the worker
        // would emit empty chunks forever.
        for (rate, channels, expected) in [
            (1_u32, 1_u16, 1_usize),
            (49, 1, 1),
            (50, 1, 1),
            (99, 2, 2),
            (100, 2, 4),
            (100, 3, 6),
        ] {
            let (sent, tone_path) = run_worker(RefAny::new(MicThreadInit {
                sample_rate: rate,
                channels,
            }));
            if !tone_path {
                return;
            }
            assert_eq!(sent.len(), 1, "rate {rate} must emit exactly one chunk");
            assert_eq!(
                sent[0].samples.len(),
                expected,
                "rate {rate} x {channels} ch must clamp to >= 1 frame"
            );
            assert_eq!(expected_chunk_len(rate, channels), expected);
            assert!(
                sent[0].samples.iter().all(|s| s.is_finite() && s.abs() <= 0.2),
                "a phase step larger than a full period must still yield bounded samples"
            );
        }
    }

    // ------------------------------------------------------------------
    // mic_writeback
    // ------------------------------------------------------------------

    #[test]
    fn writeback_hands_the_frame_to_the_hook_and_returns_its_update() {
        let mut log = new_log();
        let data = state_with_hook(cfg(44_100, 2), true, &log);
        let frame_data = RefAny::new(frame(44_100, 2, vec![0.25, -0.25, 0.5, -0.5]));

        let (update, _) =
            with_callback_info(|info| mic_writeback(data.clone(), frame_data.clone(), info));

        assert_eq!(update, Update::RefreshDom, "the hook's Update must win");
        assert_eq!(
            logged_frames(&mut log),
            vec![(44_100, 2, vec![0.25, -0.25, 0.5, -0.5])]
        );
    }

    #[test]
    fn writeback_without_a_hook_is_a_no_op() {
        let data = state(cfg(48_000, 1), true);
        let frame_data = RefAny::new(frame(48_000, 1, vec![0.0; 8]));

        let (update, changes) =
            with_callback_info(|info| mic_writeback(data.clone(), frame_data.clone(), info));

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty(), "audio has no texture - nothing to change");
    }

    #[test]
    fn writeback_ignores_frame_data_of_the_wrong_type() {
        let mut log = new_log();
        let data = state_with_hook(cfg(48_000, 1), true, &log);

        let (update, changes) =
            with_callback_info(|info| mic_writeback(data.clone(), RefAny::new(0_u32), info));

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert!(
            logged_frames(&mut log).is_empty(),
            "the user hook must not fire without a frame"
        );
    }

    #[test]
    fn writeback_survives_a_writeback_dataset_that_is_not_a_microphone_state() {
        let (update, changes) = with_callback_info(|info| {
            mic_writeback(RefAny::new(0_u32), RefAny::new(frame(8_000, 1, vec![0.0])), info)
        });

        assert_eq!(
            update,
            Update::DoNothing,
            "a foreign dataset means no hook - but no panic either"
        );
        assert!(changes.is_empty());
    }

    #[test]
    fn writeback_forwards_a_malformed_frame_to_the_hook_untouched() {
        // Hostile payload: a rate/channel count that no device produces, no samples,
        // NaN-free but nonsensical. The writeback is a transport - it must neither
        // validate nor panic.
        let mut log = new_log();
        let data = state_with_hook(cfg(48_000, 2), true, &log);
        let bogus = RefAny::new(frame(u32::MAX, u16::MAX, Vec::new()));

        let (update, _) =
            with_callback_info(|info| mic_writeback(data.clone(), bogus.clone(), info));

        assert_eq!(update, Update::RefreshDom);
        assert_eq!(logged_frames(&mut log), vec![(u32::MAX, u16::MAX, Vec::new())]);
    }

    #[test]
    fn writeback_is_a_no_op_while_the_state_is_mutably_borrowed() {
        let mut log = new_log();
        let data = state_with_hook(cfg(48_000, 1), true, &log);
        let mut probe = data.clone();
        let guard = probe
            .downcast_mut::<MicrophoneWidgetState>()
            .expect("exclusive borrow");

        let frame_data = RefAny::new(frame(48_000, 1, vec![0.1]));
        let (update, changes) =
            with_callback_info(|info| mic_writeback(data.clone(), frame_data.clone(), info));

        assert_eq!(update, Update::DoNothing, "a blocked downcast must not panic");
        assert!(changes.is_empty());
        drop(guard);
        assert!(logged_frames(&mut log).is_empty());
    }

    // ------------------------------------------------------------------
    // merge_microphone_state
    // ------------------------------------------------------------------

    #[test]
    fn merge_takes_started_from_old_and_everything_else_from_new() {
        let log = new_log();
        let new_data = state_with_hook(cfg(44_100, 2), false, &log);
        let old_data = state(cfg(8_000, 1), true);

        let mut merged = merge_microphone_state(new_data, old_data);

        assert_eq!(
            read_state(&mut merged),
            (cfg(44_100, 2), true, true),
            "config + hook come from the fresh build, 'started' from the old state"
        );
    }

    #[test]
    fn merge_takes_started_from_old_even_when_that_clears_it() {
        // The old state is authoritative for the thread flag in both directions -
        // otherwise a remount could start a second capture thread.
        let new_data = state(cfg(48_000, 1), true);
        let old_data = state(cfg(48_000, 1), false);

        let mut merged = merge_microphone_state(new_data, old_data);
        assert!(!read_state(&mut merged).1);
    }

    #[test]
    fn merge_returns_the_new_allocation_itself_not_a_copy() {
        let new_data = state(cfg(48_000, 1), false);
        let handle = new_data.clone();

        let merged = merge_microphone_state(new_data, state(cfg(48_000, 1), true));

        assert_eq!(merged, handle, "merge must hand back the same state object");
    }

    #[test]
    fn merge_leaves_the_new_state_alone_when_the_old_one_is_foreign() {
        let new_data = state(cfg(48_000, 2), true);
        let mut merged = merge_microphone_state(new_data, RefAny::new(0_u32));

        assert_eq!(
            read_state(&mut merged),
            (cfg(48_000, 2), true, false),
            "nothing to carry forward from a foreign payload"
        );
    }

    #[test]
    fn merge_returns_a_foreign_new_dataset_untouched() {
        let old_data = state(cfg(48_000, 1), true);
        let mut merged = merge_microphone_state(RefAny::new(77_u32), old_data);

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
        let mut data = state_with_hook(cfg(48_000, 2), true, &new_log());
        let mut merged = merge_microphone_state(data.clone(), data.clone());

        assert_eq!(read_state(&mut merged), (cfg(48_000, 2), true, true));
        assert_eq!(read_state(&mut data), (cfg(48_000, 2), true, true));
    }

    #[test]
    fn a_rebuilt_dom_merges_the_running_thread_flag_forward() {
        // The relayout round trip, through the callbacks `dom()` actually wires:
        // mount marks `started`, the fresh build starts at `false`, and the merge
        // carries the flag across so AfterMount cannot start a second thread.
        let old = MicrophoneWidget::create(cfg(48_000, 1)).dom();
        let mut old_ds = old.root.get_dataset().cloned().expect("dataset");
        {
            let mut s = old_ds
                .downcast_mut::<MicrophoneWidgetState>()
                .expect("state");
            s.started = true;
        }

        let new = MicrophoneWidget::create(cfg(44_100, 2))
            .with_on_frame(new_log(), record_frame as OnAudioFrameCallbackType)
            .dom();
        let new_ds = new.root.get_dataset().cloned().expect("dataset");
        let merge = new.root.get_merge_callback().expect("merge callback");

        let mut merged = (merge.cb)(new_ds, old_ds);

        assert_eq!(
            read_state(&mut merged),
            (cfg(44_100, 2), true, true),
            "the rebuilt widget keeps its new config + hook but inherits the thread"
        );
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
    fn mic_worker_acknowledges_terminate_within_the_grace_budget() {
        use crate::thread::{Thread, ThreadCallback};

        let t = Thread::create(
            RefAny::new(MicThreadInit {
                sample_rate: 8_000,
                channels: 1,
            }),
            RefAny::new(0_usize),
            ThreadCallback::new(mic_worker),
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
            "mic_worker did not acknowledge TerminateThread within 2000ms — at shutdown it \
             would be DETACHED rather than joined"
        );
    }
}
