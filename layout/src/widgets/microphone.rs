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

use super::capture_common::mic_backend;
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
            .with_merge_callback(merge_microphone_state as DatasetMergeCallbackType)
            .with_callback(
                EventFilter::Component(ComponentEventFilter::AfterMount),
                dataset,
                Callback::from(mic_on_after_mount as CallbackType),
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
extern "C" fn mic_worker(mut init: RefAny, mut sender: ThreadSender, _recv: ThreadReceiver) {
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

    let frames_per_chunk = (rate as usize / 50).max(1); // ~20 ms
    let step = 2.0 * core::f32::consts::PI * 440.0 / rate as f32;
    let mut phase: f32 = 0.0;
    loop {
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
