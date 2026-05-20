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
}

/// A video-playback widget. `create(config).dom()` yields an `<img>` the
/// decode thread keeps fed.
#[repr(C)]
pub struct VideoWidget {
    /// Source URL + autoplay/loop + format.
    pub config: VideoConfig,
    /// Optional per-frame user hook (effects / save / send - azul-meet).
    pub on_frame: OptionOnVideoFrame,
}

impl VideoWidget {
    /// Create a video widget for the given config.
    pub fn create(config: VideoConfig) -> Self {
        Self {
            config,
            on_frame: OptionOnVideoFrame::None,
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

    /// Build the widget's DOM: a single `<img>` node, fed by a background
    /// decode thread started on mount.
    pub fn dom(self) -> Dom {
        let state = VideoWidgetState {
            config: self.config,
            started: false,
            gl_texture_id: None,
            on_frame: self.on_frame,
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
}

/// AfterMount: start the background decode thread exactly once.
extern "C" fn video_on_after_mount(mut data: RefAny, mut info: CallbackInfo) -> Update {
    {
        let mut s = match data.downcast_mut::<VideoWidgetState>() {
            Some(s) => s,
            None => return Update::DoNothing,
        };
        if s.started {
            return Update::DoNothing;
        }
        s.started = true;
    }
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(()),
            data.clone(),
            ThreadCallback::new(video_test_worker),
        ),
    );
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

/// Writeback (main thread): hand the decoded frame to the shared GL presenter
/// and store the (stable) texture id.
extern "C" fn video_writeback(
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
        }
    }
    new_data
}
