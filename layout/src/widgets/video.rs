//! Video-playback widget — a "dumb widget" identical in architecture to the
//! [`CameraWidget`](super::camera) / [`ScreenCaptureWidget`](super::screencap),
//! only the source differs (a video URL/file decoded via vk-video).
//! SUPER_PLAN_2 §4 P6, widget pivot.
//!
//! `VideoWidget::create(config).dom()` → an `<img>` a background decode thread
//! keeps fed; each frame goes through [`super::capture_common::present_frame`]
//! (GL-texture install-once / re-upload + recomposite). Shared core in
//! `capture_common`; this widget is its config + worker. Test-pattern worker
//! (scrolling SMPTE colour bars) stands in for the real vk-video decode worker.

use alloc::vec::Vec;

use azul_core::callbacks::Update;
use azul_core::dom::{ComponentEventFilter, DatasetMergeCallbackType, Dom, EventFilter};
use azul_core::refany::{OptionRefAny, RefAny};
use azul_core::resources::{ImageRef, RawImageFormat};
use azul_core::task::{ThreadId, ThreadReceiver};
use azul_core::video::{VideoConfig, VideoFrame};

use super::capture_common::{
    invoke_on_frame, present_frame, OnVideoFrame, OnVideoFrameCallback, OptionOnVideoFrame,
};
use crate::callbacks::{Callback, CallbackInfo, CallbackType};
use crate::thread::{
    Thread, ThreadCallback, ThreadReceiveMsg, ThreadSender, ThreadWriteBackMsg, WriteBackCallback,
};

/// Default decode size for the test pattern (the real decoder reports the
/// stream's actual size).
const DEFAULT_W: u32 = 1280;
const DEFAULT_H: u32 = 720;

/// Live state for one video widget, carried across relayout by
/// [`merge_video_state`].
pub struct VideoWidgetState {
    /// The requested playback configuration (source + autoplay/loop).
    pub config: VideoConfig,
    /// `true` once the decode thread has been started.
    pub started: bool,
    /// The stable external GL texture id once installed.
    pub gl_texture_id: Option<u32>,
    /// Optional user hook invoked with each decoded frame (effects / save /
    /// send). Re-set on every fresh build (see [`merge_video_state`]).
    pub on_frame: OptionOnVideoFrame,
    /// Optional pre-decoded frames to replay (a `RefAny` holding a
    /// `Vec<VideoFrame>`); when set, the replay worker cycles these instead of
    /// the built-in test pattern. Carried forward by [`merge_video_state`].
    pub frames: OptionRefAny,
    /// Streaming source — a `RefAny` holding a `String` (URL, fetched via an HTTP
    /// range request) or a `Vec<u8>` (raw MP4 bytes) — handed to the decode worker
    /// as its thread init. Set via [`VideoWidget::with_source`].
    pub source: OptionRefAny,
    /// The off-main-thread streaming decode worker (mirrors the map widget's
    /// `fetch_callback`). Set via [`VideoWidget::dom_with_decoder`]. When present,
    /// AfterMount spawns it on a background `Thread` instead of the replay /
    /// test-pattern workers, so the VK decode runs off the main thread.
    pub decode_callback: Option<ThreadCallback>,
}

/// A video-playback widget. `create(config).dom()` yields an `<img>` the
/// decode thread keeps fed.
#[repr(C)]
pub struct VideoWidget {
    /// Source URL + autoplay/loop + format.
    pub config: VideoConfig,
    /// Optional per-frame user hook (effects / save / send - azul-meet).
    pub on_frame: OptionOnVideoFrame,
    /// Optional pre-decoded frames to replay (a `RefAny` holding a
    /// `Vec<VideoFrame>`); set via [`with_frames`](Self::with_frames). When
    /// present the widget cycles these instead of the test pattern.
    pub frames: OptionRefAny,
    /// Streaming source for [`dom_with_decoder`](Self::dom_with_decoder) — a
    /// `RefAny` holding a `String` (URL) or `Vec<u8>` (MP4 bytes). Set via
    /// [`with_source`](Self::with_source).
    pub source: OptionRefAny,
}

impl VideoWidget {
    /// Create a video widget for the given config.
    pub fn create(config: VideoConfig) -> Self {
        Self {
            config,
            on_frame: OptionOnVideoFrame::None,
            frames: OptionRefAny::None,
            source: OptionRefAny::None,
        }
    }

    /// Set a hook invoked with every decoded frame - for live effects, saving
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
    pub fn with_on_frame<C: Into<OnVideoFrameCallback>>(
        mut self,
        data: RefAny,
        on_frame: C,
    ) -> Self {
        self.set_on_frame(data, on_frame);
        self
    }

    /// Replay a list of already-decoded frames instead of the built-in test
    /// pattern: `frames` is a [`RefAny`] holding a `Vec<VideoFrame>`. The
    /// background worker cycles them through the shared GL presenter (the same
    /// `present_frame` path the camera/screencap widgets use), so callers that
    /// decode a clip up front (e.g. `decode_mp4_h264_bytes`) get real pixels on
    /// screen. The `RefAny` must carry a `Vec<VideoFrame>`, else playback is
    /// skipped and the test pattern shows instead.
    pub fn with_frames(mut self, frames: RefAny) -> Self {
        self.frames = Some(frames).into();
        self
    }

    /// Set the streaming source for [`dom_with_decoder`](Self::dom_with_decoder):
    /// a [`RefAny`] holding a `String` (a URL, fetched via an HTTP range request)
    /// or a `Vec<u8>` (raw MP4 bytes). The decode worker reads this as its thread
    /// init.
    pub fn with_source(mut self, source: RefAny) -> Self {
        self.source = Some(source).into();
        self
    }

    fn build_dom(self, decode_cb: Option<ThreadCallback>) -> Dom {
        let state = VideoWidgetState {
            config: self.config,
            started: false,
            gl_texture_id: None,
            on_frame: self.on_frame,
            frames: self.frames,
            source: self.source,
            decode_callback: decode_cb,
        };
        let dataset = RefAny::new(state);

        let placeholder = ImageRef::null_image(
            DEFAULT_W as usize,
            DEFAULT_H as usize,
            RawImageFormat::BGRA8,
            b"azul-video-placeholder".to_vec(),
        );

        Dom::create_image(placeholder)
            .with_dataset(OptionRefAny::Some(dataset.clone()))
            .with_merge_callback(merge_video_state as DatasetMergeCallbackType)
            .with_callback(
                EventFilter::Component(ComponentEventFilter::AfterMount),
                dataset,
                Callback::from(video_on_after_mount as CallbackType),
            )
    }

    /// Build the widget's DOM: a single `<img>` node a background thread keeps
    /// fed. Replays pre-decoded [`with_frames`](Self::with_frames) if given, else
    /// shows the built-in test pattern.
    pub fn dom(self) -> Dom {
        self.build_dom(None)
    }

    /// Build the widget's DOM and wire a background **streaming** decode worker —
    /// mirrors `MapWidget::dom_with_fetch`. `cb` runs on a framework `Thread` OFF
    /// the main thread: it reads the [`with_source`](Self::with_source) `RefAny`
    /// (URL or MP4 bytes), runs the VK decode incrementally (no up-front decode),
    /// and `WriteBack`s frames to the `<img>` paced by wall-clock (dropping late
    /// frames). The standard worker is
    /// `azul_dll::desktop::extra::video_codec::stream::video_decode_worker`; wrap
    /// it in a `ThreadCallback` to pass it here.
    pub fn dom_with_decoder(self, cb: ThreadCallback) -> Dom {
        self.build_dom(Some(cb))
    }
}

/// AfterMount: start the background decode thread exactly once.
extern "C" fn video_on_after_mount(mut data: RefAny, mut info: CallbackInfo) -> Update {
    // Mark started exactly once; pull out the streaming decode worker (if any),
    // its source, and any pre-decoded replay frames.
    let (decode_cb, source, frames) = {
        let mut s = match data.downcast_mut::<VideoWidgetState>() {
            Some(s) => s,
            None => return Update::DoNothing,
        };
        if s.started {
            return Update::DoNothing;
        }
        s.started = true;
        let source = match &s.source {
            OptionRefAny::Some(src) => Some(src.clone()),
            OptionRefAny::None => None,
        };
        let frames = match &s.frames {
            OptionRefAny::Some(f) => Some(f.clone()),
            OptionRefAny::None => None,
        };
        (s.decode_callback.clone(), source, frames)
    };
    // Priority: off-main streaming decode worker > replay pre-decoded frames >
    // built-in test pattern. All feed the same
    // WriteBack -> video_writeback -> present_frame path.
    if let Some(cb) = decode_cb {
        let init = source.unwrap_or_else(|| RefAny::new(()));
        info.add_thread(ThreadId::unique(), Thread::create(init, data.clone(), cb));
    } else if let Some(frames) = frames {
        info.add_thread(
            ThreadId::unique(),
            Thread::create(frames, data.clone(), ThreadCallback::new(video_replay_worker)),
        );
    } else {
        info.add_thread(
            ThreadId::unique(),
            Thread::create(
                RefAny::new(()),
                data.clone(),
                ThreadCallback::new(video_test_worker),
            ),
        );
    }
    Update::DoNothing
}

/// Background worker (test pattern): SMPTE-style colour bars scrolling
/// horizontally ~30×/s. Replaced by the real vk-video decode worker later.
extern "C" fn video_test_worker(_init: RefAny, mut sender: ThreadSender, _recv: ThreadReceiver) {
    const BARS: [[u8; 3]; 7] = [
        [235, 235, 235],
        [235, 235, 16],
        [16, 235, 235],
        [16, 235, 16],
        [235, 16, 235],
        [235, 16, 16],
        [16, 16, 235],
    ];
    let (w, h) = (DEFAULT_W as usize, DEFAULT_H as usize);
    let mut tick: u32 = 0;
    loop {
        let shift = (tick as usize / 4) % 7;
        let mut bytes = Vec::with_capacity(w * h * 4);
        for _y in 0..h {
            for x in 0..w {
                let c = BARS[((x * 7 / w) + shift) % 7];
                bytes.extend_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
        let frame = VideoFrame {
            width: w as u32,
            height: h as u32,
            bytes: bytes.into(),
        };
        let sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
            WriteBackCallback::new(video_writeback),
            RefAny::new(frame),
        )));
        if !sent {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(33));
        tick = tick.wrapping_add(2);
    }
}

/// Background worker (replay): cycle a caller-supplied `Vec<VideoFrame>` (e.g. a
/// clip decoded up front via `decode_mp4_h264_bytes`) ~30x/s through the same
/// WriteBack -> [`video_writeback`] -> [`super::capture_common::present_frame`]
/// path as the test pattern, so real decoded pixels land in the shared GL
/// texture. `init` is the `RefAny` handed to
/// [`VideoWidget::with_frames`](VideoWidget::with_frames); if it doesn't hold a
/// non-empty `Vec<VideoFrame>` the worker just returns.
extern "C" fn video_replay_worker(mut init: RefAny, mut sender: ThreadSender, _recv: ThreadReceiver) {
    let frames: Vec<VideoFrame> = match init.downcast_ref::<Vec<VideoFrame>>() {
        Some(f) => f.clone(),
        None => return,
    };
    if frames.is_empty() {
        return;
    }
    let mut idx: usize = 0;
    loop {
        let frame = frames[idx % frames.len()].clone();
        let sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::new(
            WriteBackCallback::new(video_writeback),
            RefAny::new(frame),
        )));
        if !sent {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(33));
        idx = idx.wrapping_add(1);
    }
}

/// Writeback (main thread): hand the decoded frame to the shared GL presenter
/// and store the (stable) texture id.
pub extern "C" fn video_writeback(
    mut writeback_data: RefAny,
    mut frame_data: RefAny,
    mut info: CallbackInfo,
) -> Update {
    let (current, hook) = match writeback_data.downcast_ref::<VideoWidgetState>() {
        Some(s) => (s.gl_texture_id, s.on_frame.clone()),
        None => (None, OptionOnVideoFrame::None),
    };
    let mut user_update = Update::DoNothing;
    let new_id = match frame_data.downcast_ref::<VideoFrame>() {
        Some(frame) => {
            let id = present_frame(&mut info, writeback_data.clone(), current, &frame);
            user_update = invoke_on_frame(&hook, &mut info, &frame);
            id
        }
        None => return Update::DoNothing,
    };
    if let Some(mut s) = writeback_data.downcast_mut::<VideoWidgetState>() {
        s.gl_texture_id = new_id;
    }
    user_update
}

/// Carry live state forward across relayout.
extern "C" fn merge_video_state(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
    {
        let new_guard = new_data.downcast_mut::<VideoWidgetState>();
        let old_guard = old_data.downcast_ref::<VideoWidgetState>();
        if let (Some(mut new_g), Some(old_g)) = (new_guard, old_guard) {
            new_g.started = old_g.started;
            new_g.gl_texture_id = old_g.gl_texture_id;
            new_g.frames = old_g.frames.clone();
            new_g.source = old_g.source.clone();
            new_g.decode_callback = old_g.decode_callback.clone();
        }
    }
    new_data
}
