//! Video-playback widget - a "dumb widget" identical in architecture to the
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

use azul_core::callbacks::{Update, VirtualViewCallbackInfo, VirtualViewReturn};
use azul_core::dom::{ComponentEventFilter, DatasetMergeCallbackType, Dom, EventFilter, OptionDom};
use azul_core::geom::LogicalPosition;
use azul_core::refany::{OptionRefAny, RefAny};
use azul_core::resources::{ImageRef, RawImage, RawImageData, RawImageFormat};
use azul_core::task::{ThreadId, ThreadReceiver, ThreadSendMsg};
use azul_core::video::{VideoConfig, VideoFrame};

use super::capture_common::{
    invoke_on_frame, OnVideoFrame, OnVideoFrameCallback, OptionOnVideoFrame,
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
#[derive(Debug)]
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
    /// The off-main-thread streaming decode worker (mirrors the map widget's
    /// `fetch_callback`). Set via [`VideoWidget::dom_with_decoder`]. When present,
    /// `AfterMount` spawns it on a background `Thread` instead of the replay /
    /// test-pattern workers, so the VK decode runs off the main thread.
    pub decode_callback: Option<ThreadCallback>,
    /// The latest decoded frame to display, as a CPU `ImageRef` (RGBA8). The
    /// `VirtualView` render callback ([`video_widget_render`]) reads this on each
    /// re-render; [`video_writeback`] stores it and triggers an in-place
    /// `VirtualView` re-render - so the frame renders on cpurender AND webrender,
    /// exactly like the map widget's tile cache. (Replaces the GL `present_frame`
    /// path for video; camera/screencap still use `present_frame`.)
    pub current_frame: Option<ImageRef>,
    /// The decode worker's `ThreadId` (set by `AfterMount`). Lets the resize callback
    /// message the running worker (`info.get_thread(id).sender.send(..)`) so it can
    /// re-target the decoder to the new physical-pixel size - a cheap image swap, no
    /// relayout. Carried across relayout by [`merge_video_state`].
    pub thread_id: Option<ThreadId>,
    /// Clone of the worker's main→worker `Sender` (set by `AfterMount`, carried by
    /// merge). Lets [`merge_video_state`] - which has no `CallbackInfo` - push a
    /// seek to the running worker when `config.timestamp` changes (scrubbing).
    pub seek_sender: Option<std::sync::mpsc::Sender<ThreadSendMsg>>,
}

/// A video-playback widget. `create(config).dom()` yields an `<img>` the
/// decode thread keeps fed.
#[repr(C)]
#[derive(Debug)]
pub struct VideoWidget {
    /// Source URL + autoplay/loop + format.
    pub config: VideoConfig,
    /// Optional per-frame user hook (effects / save / send - azul-meet).
    pub on_frame: OptionOnVideoFrame,
    /// Optional pre-decoded frames to replay (a `RefAny` holding a
    /// `Vec<VideoFrame>`); set via [`with_frames`](Self::with_frames). When
    /// present the widget cycles these instead of the test pattern.
    pub frames: OptionRefAny,
}

impl VideoWidget {
    /// Create a video widget for the given config.
    #[must_use] pub const fn create(config: VideoConfig) -> Self {
        Self {
            config,
            on_frame: OptionOnVideoFrame::None,
            frames: OptionRefAny::None,
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
    #[must_use]
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
    #[must_use] pub fn with_frames(mut self, frames: RefAny) -> Self {
        self.frames = Some(frames).into();
        self
    }

    fn build_dom(self, decode_cb: Option<ThreadCallback>) -> Dom {
        let state = VideoWidgetState {
            config: self.config,
            started: false,
            gl_texture_id: None,
            on_frame: self.on_frame,
            frames: self.frames,
            decode_callback: decode_cb,
            current_frame: None,
            thread_id: None,
            seek_sender: None,
        };
        let dataset = RefAny::new(state);
        let vv_data = dataset.clone();

        // The body is a VirtualView (exactly like the map widget): its render
        // callback re-reads `current_frame` from the dataset each re-render and
        // builds the `<img>`, so streamed frames render on BOTH cpurender and
        // webrender. The background decode worker is started on AfterMount and
        // `WriteBack`s frames into `current_frame` + triggers a VirtualView
        // re-render in place (no DOM rebuild) — see `video_writeback`. The caller
        // sizes the outer node via `.with_css(...)` on the returned Dom.
        Dom::create_div()
            .with_dataset(OptionRefAny::Some(dataset.clone()))
            .with_merge_callback(azul_core::dom::DatasetMergeCallback::from_ptr(merge_video_state))
            .with_callback(
                EventFilter::Component(ComponentEventFilter::AfterMount),
                dataset.clone(),
                Callback::from_ptr(video_on_after_mount),
            )
            // Window/layout resize → re-target the decoder to the new physical size
            // (a cheap image swap, no relayout). See `video_on_resize`.
            .with_callback(
                EventFilter::Component(ComponentEventFilter::NodeResized),
                dataset,
                Callback::from_ptr(video_on_resize),
            )
            .with_child(
                Dom::create_virtual_view(
                    vv_data,
                    azul_core::callbacks::VirtualViewCallback::create(video_widget_render),
                )
                .with_css("width: 100%; height: 100%; overflow: hidden;"),
            )
    }

    /// Build the widget's DOM: a single `<img>` node a background thread keeps
    /// fed. Replays pre-decoded [`with_frames`](Self::with_frames) if given, else
    /// shows the built-in test pattern.
    #[must_use] pub fn dom(self) -> Dom {
        self.build_dom(None)
    }

    /// Build the widget's DOM and wire a background **streaming** decode worker -
    /// mirrors `MapWidget::dom_with_fetch`. `cb` runs on a framework `Thread` OFF
    /// the main thread: it reads the `VideoConfig` (its typed `VideoSource` -
    /// URL / file / bytes), runs the VK decode incrementally (no up-front decode),
    /// and `WriteBack`s frames to the `<img>` paced by wall-clock (dropping late
    /// frames). The standard worker is
    /// `azul_dll::desktop::extra::video_codec::stream::video_decode_worker`; wrap
    /// it in a `ThreadCallback` to pass it here.
    #[must_use] pub fn dom_with_decoder(self, cb: ThreadCallback) -> Dom {
        self.build_dom(Some(cb))
    }
}

/// `VirtualView` render callback (mirrors `map_widget_render`): build the `<img>`
/// for the latest decoded frame, re-read from the widget's dataset on every
/// re-render. The decode worker stores frames into `current_frame` and triggers
/// the re-render in place (see [`video_writeback`]), so this renders on both the
/// CPU and GPU renderers with no DOM rebuild.
extern "C" fn video_widget_render(
    mut data: RefAny,
    info: VirtualViewCallbackInfo,
) -> VirtualViewReturn {
    let bounds = info.get_bounds().get_logical_size();
    if std::env::var("AZ_VIDEO_FRAMELOG").is_ok() {
        eprintln!("[vrender] bounds {}x{}", bounds.width, bounds.height);
    }
    // Defensive (like map_widget_render): a non-finite / non-positive box (layout
    // not yet settled, e.g. flex-grow before the parent height resolves) would
    // produce a garbage `<img>` size — render nothing until it settles.
    let dom = if !bounds.width.is_finite()
        || !bounds.height.is_finite()
        || bounds.width <= 0.0
        || bounds.height <= 0.0
    {
        OptionDom::None
    } else {
        data.downcast_ref::<VideoWidgetState>().map_or(OptionDom::None, |s| {
            s.current_frame.as_ref().map_or_else(
                || {
                    // Poster / "no signal" placeholder. Returning None here
                    // rendered NOTHING, so a decoder that never produced a
                    // frame (missing video-native feature, non-x86_64 target,
                    // Vulkan init failure, network stall) was
                    // indistinguishable from a black video — the shipped
                    // azul-video "black frame" bug. A dead pipeline must be
                    // visibly dead.
                    OptionDom::Some(Dom::create_div().with_css(
                        "width: 100%; height: 100%; background: #2a2a30; \
                         border: 1px solid #44444c;",
                    ))
                },
                |img| {
                    OptionDom::Some(
                        Dom::create_image(img.clone()).with_css("width: 100%; height: 100%;"),
                    )
                },
            )
        })
    };
    VirtualViewReturn {
        dom,
        materialized: azul_core::geom::LogicalRect::new(LogicalPosition::zero(), bounds),
        virtual_rect: azul_core::geom::LogicalRect::new(LogicalPosition::zero(), bounds),
    }
}

/// `AfterMount`: start the background decode thread exactly once.
extern "C" fn video_on_after_mount(mut data: RefAny, mut info: CallbackInfo) -> Update {
    // Mark started exactly once; pull out the streaming decode worker (if any),
    // its source, and any pre-decoded replay frames.
    let (decode_cb, config, frames) = {
        let Some(mut s) = data.downcast_mut::<VideoWidgetState>() else {
            return Update::DoNothing;
        };
        if s.started {
            return Update::DoNothing;
        }
        s.started = true;
        let frames = match &s.frames {
            OptionRefAny::Some(f) => Some(f.clone()),
            OptionRefAny::None => None,
        };
        (s.decode_callback.clone(), s.config.clone(), frames)
    };
    // Priority: off-main streaming decode worker > replay pre-decoded frames >
    // built-in test pattern. All feed the same WriteBack -> video_writeback path.
    if let Some(cb) = decode_cb {
        // The worker's thread-init is the `VideoConfig` itself: it matches on
        // `config.source` (typed — no RefAny downcast) and reads `config.timestamp`.
        let init = RefAny::new(config);
        let tid = ThreadId::unique();
        let thread = Thread::create(init, data.clone(), cb);
        // Grab the main→worker sender BEFORE add_thread moves the Thread, so the
        // merge callback can push seeks to the worker (scrubbing).
        let seek_sender = thread.clone_sender();
        info.add_thread(tid, thread);
        // Remember the worker's id (resize messaging) + sender (seek messaging).
        if let Some(mut s) = data.downcast_mut::<VideoWidgetState>() {
            s.thread_id = Some(tid);
            s.seek_sender = seek_sender;
        }
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

/// `NodeResized`: the video box changed physical size (window resize / relayout). Tell
/// the running decode worker the new target size via its `ThreadSender` so it scales
/// frames to fit OFF the main thread - the UI then does a cheap image swap with no
/// interpolation. This is a message, NOT a relayout: returns `DoNothing`.
///
/// The size is in DEVICE pixels (`capture_common::preview_size_for_node`):
/// the logical size undersized a Retina video by 2x.
extern "C" fn video_on_resize(mut data: RefAny, info: CallbackInfo) -> Update {
    let tid = match data.downcast_ref::<VideoWidgetState>() {
        Some(s) => s.thread_id,
        None => return Update::DoNothing,
    };
    let Some(tid) = tid else {
        return Update::DoNothing;
    };
    let Some(target) = super::capture_common::preview_size_for_node(&info) else {
        return Update::DoNothing;
    };
    if let Some(thread) = info.get_thread(&tid) {
        // Best-effort resize notification: if the decode worker has already
        // exited, the send fails and there is nothing to do here.
        let _ = thread.send_message(ThreadSendMsg::Custom(RefAny::new(target)));
    }
    Update::DoNothing
}

/// Background worker (test pattern): SMPTE-style colour bars scrolling
/// horizontally ~30x/s. Replaced by the real vk-video decode worker later.
#[allow(clippy::cast_possible_truncation)] // bounded layout/render numeric cast
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
/// `WriteBack` -> [`video_writeback`] -> [`super::capture_common::present_frame`]
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

/// Writeback (main thread): store the decoded frame as the widget's
/// `current_frame` (a CPU `ImageRef`) and re-render the `VirtualView` in place so it
/// re-reads it - exactly like `map_tile_writeback`.
///
/// Renders on cpurender AND
/// webrender (no GL `present_frame`, no DOM rebuild).
#[must_use] pub extern "C" fn video_writeback(
    mut writeback_data: RefAny,
    mut frame_data: RefAny,
    mut info: CallbackInfo,
) -> Update {
    let hook = writeback_data.downcast_ref::<VideoWidgetState>().map_or_else(|| OptionOnVideoFrame::None, |s| s.on_frame.clone());
    let mut user_update = Update::DoNothing;
    match frame_data.downcast_ref::<VideoFrame>() {
        Some(frame) => {
            // Guard against dimensions whose RGBA byte count overflows `usize`
            // before `ImageRef::new_rawimage` validates it against the buffer:
            // `width * height * 4` wraps in release (e.g. 2^31 x 2^31 -> 0) so an
            // empty buffer would spuriously "match" and store a bogus frame. A
            // `checked_mul` that overflows drops the frame — the hook is still
            // notified below, exactly as for a byte-count mismatch.
            let fits = (frame.width as usize)
                .checked_mul(frame.height as usize)
                .and_then(|px| px.checked_mul(4))
                .is_some();
            if fits {
                if let Some(img) = ImageRef::new_rawimage(RawImage {
                    pixels: RawImageData::U8(frame.bytes.clone()),
                    width: frame.width as usize,
                    height: frame.height as usize,
                    premultiplied_alpha: false,
                    data_format: RawImageFormat::RGBA8,
                    tag: b"azul-video-frame".to_vec().into(),
                }) {
                    if let Some(mut s) = writeback_data.downcast_mut::<VideoWidgetState>() {
                        s.current_frame = Some(img);
                    }
                }
            }
            user_update = invoke_on_frame(&hook, &mut info, &frame);
        }
        None => return Update::DoNothing,
    }
    // Re-render the VirtualView(s) in place so the content callback re-reads the
    // freshly-stored `current_frame` (NOT RefreshDom — that would rebuild the DOM
    // and orphan the worker's dataset clone). Same trick as `map_tile_writeback`.
    info.trigger_all_virtual_view_rerender();
    user_update
}

/// Carry live state forward across relayout.
#[allow(clippy::float_cmp)] // intentional exact compare: change-detection / identity fast-path / cache-key match
extern "C" fn merge_video_state(mut new_data: RefAny, mut old_data: RefAny) -> RefAny {
    // Return the OLD allocation, adopting config forward — the same rule
    // `merge_map_tile_cache` documents. The decode worker holds a clone of
    // the OLD RefAny (handed over at spawn) and writes every decoded frame
    // into it; returning `new_data` re-pointed the DOM at a fresh allocation
    // nobody wrote to, so the picture froze on whatever frame existed at
    // merge time — the AzVideo demo hit it on the FIRST timeline click
    // (its callback returns RefreshDom).
    let merged_into_old = {
        let new_guard = new_data.downcast_ref::<VideoWidgetState>();
        let old_guard = old_data.downcast_mut::<VideoWidgetState>();
        if let (Some(new_g), Some(mut old_g)) = (new_guard, old_guard) {
            // Scrubbing: a changed `config.timestamp` across this relayout → tell the
            // worker to seek. Cheap wall-clock reposition (the worker already has the
            // decoded frames), result comes back as an image swap — no re-decode here.
            if old_g.config.timestamp != new_g.config.timestamp {
                if let Some(snd) = old_g.seek_sender.as_ref() {
                    drop(snd.send(ThreadSendMsg::Custom(RefAny::new(new_g.config.timestamp))));
                }
            }
            // Input-source change → tell the worker to re-init the decode (it
            // re-resolves/demuxes/decodes the new source); the frame swaps in when ready.
            if old_g.config.source != new_g.config.source {
                if let Some(snd) = old_g.seek_sender.as_ref() {
                    drop(snd.send(ThreadSendMsg::Custom(RefAny::new(new_g.config.source.clone()))));
                }
            }
            // Adopt the app-driven config; keep every worker-facing field
            // (frames, current_frame, thread_id, seek_sender, started) in the
            // allocation the worker actually writes to.
            old_g.config = new_g.config.clone();
            old_g.on_frame = new_g.on_frame.clone();
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
#[allow(
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::float_cmp,
    clippy::items_after_statements,
    clippy::let_and_return
)]
mod autotest_generated {
    use std::{
        collections::BTreeMap,
        panic::{catch_unwind, AssertUnwindSafe},
        sync::{
            atomic::{AtomicUsize, Ordering},
            mpsc::{channel, Receiver, Sender},
            Arc, Mutex, PoisonError,
        },
    };

    use azul_core::{
        callbacks::{HidpiAdjustedBounds, VirtualViewCallbackReason},
        dom::{DomId, DomNodeId, NodeType},
        geom::{LogicalSize, OptionLogicalPosition},
        gl::OptionGlContextPtr,
        hit_test::ScrollPosition,
        resources::{DecodedImage, DpiScaleFactor, ImageCache, RendererResources},
        styled_dom::NodeHierarchyItemId,
        task::{
            OptionThreadSendMsg, ThreadReceiverDestructorCallback, ThreadReceiverInner,
            ThreadRecvCallback,
        },
        video::VideoSource,
        window::{MonitorVec, RawWindowHandle, WindowTheme},
    };
    use azul_css::{system::SystemStyle, AzString};
    use rust_fontconfig::FcFontCache;

    use super::*;
    #[cfg(feature = "icu")]
    use crate::icu::IcuLocalizerHandle;
    use crate::{
        callbacks::{CallbackChange, CallbackInfoRefData, ExternalSystemCallbacks},
        thread::{
            ThreadCallbackType, ThreadSendCallback, ThreadSenderDestructorCallback,
            ThreadSenderInner,
        },
        widgets::capture_common::OnVideoFrameCallbackType,
        window::LayoutWindow,
        window_state::FullWindowState,
    };

    // ==================================================================
    // Config fixtures
    // ==================================================================

    /// A config with an explicit source + scrub position (everything else
    /// pinned so a test only varies what it names).
    fn config(source: VideoSource, timestamp: f32) -> VideoConfig {
        VideoConfig {
            source,
            timestamp,
            autoplay: true,
            looping: false,
            output_format: RawImageFormat::BGRA8,
        }
    }

    fn url_source(host: &str, path: &str) -> VideoSource {
        VideoSource::Url(azul_core::url::Url::from_parts("https", host, 443, path))
    }

    fn file_source(path: &'static str) -> VideoSource {
        VideoSource::File(AzString::from_const_str(path))
    }

    fn bytes_source(bytes: Vec<u8>) -> VideoSource {
        VideoSource::Bytes(bytes.into())
    }

    /// Representative + hostile configs: every `VideoSource` variant (empty and
    /// large payloads), a non-ASCII path, and every f32 boundary a scrub
    /// position can take (NaN / ±inf / ±0 / MIN / MAX).
    fn all_configs() -> Vec<VideoConfig> {
        vec![
            VideoConfig::default(),
            config(url_source("example.com", "/clip.mp4"), 0.0),
            config(url_source("", ""), f32::MAX),
            config(file_source("/tmp/clip.mp4"), -1.0),
            // unicode: emoji + CJK + RTL + a combining mark in the path.
            config(
                file_source("/tmp/\u{1F3AC}-\u{5F71}\u{7247}-\u{0631}\u{0645}\u{0632}-e\u{0301}.mp4"),
                f32::NAN,
            ),
            config(bytes_source(Vec::new()), f32::INFINITY),
            config(bytes_source(vec![0xFF; 8192]), f32::NEG_INFINITY),
            config(bytes_source(vec![0x00]), f32::MIN),
            VideoConfig {
                source: file_source("x"),
                timestamp: -0.0,
                autoplay: false,
                looping: true,
                output_format: RawImageFormat::R8,
            },
        ]
    }

    /// `VideoConfig` is only `PartialEq`, so a NaN scrub position never compares
    /// equal to itself - compare the timestamp bit-exactly instead.
    fn assert_same_config(actual: &VideoConfig, expected: &VideoConfig) {
        assert_eq!(actual.source, expected.source, "source must round-trip");
        assert_eq!(
            actual.timestamp.to_bits(),
            expected.timestamp.to_bits(),
            "timestamp must survive bit-exactly (NaN included)"
        );
        assert_eq!(actual.autoplay, expected.autoplay);
        assert_eq!(actual.looping, expected.looping);
        assert_eq!(actual.output_format, expected.output_format);
    }

    const CONST_CONFIG: VideoConfig = VideoConfig {
        source: VideoSource::File(AzString::from_const_str("/tmp/const-clip.mp4")),
        timestamp: 2.5,
        autoplay: false,
        looping: true,
        output_format: RawImageFormat::RGBA8,
    };

    /// Compile-time proof that `create` really is a `const fn` - the `const`
    /// qualifier is part of the public API, so a non-const `create` must break
    /// this file.
    const CONST_WIDGET: VideoWidget = VideoWidget::create(CONST_CONFIG);

    // ==================================================================
    // State fixtures
    // ==================================================================

    /// A freshly-built widget state (exactly what `build_dom` stores).
    fn base_state(config: VideoConfig) -> VideoWidgetState {
        VideoWidgetState {
            config,
            started: false,
            gl_texture_id: None,
            on_frame: OptionOnVideoFrame::None,
            frames: OptionRefAny::None,
            decode_callback: None,
            current_frame: None,
            thread_id: None,
            seek_sender: None,
        }
    }

    fn state(config: VideoConfig) -> RefAny {
        RefAny::new(base_state(config))
    }

    /// Everything a test needs to know about a `VideoWidgetState`, read out in
    /// one borrow (`downcast_ref` takes `&mut self`, so overlapping reads would
    /// otherwise have to nest).
    #[derive(Debug, Clone, PartialEq)]
    struct StateSummary {
        started: bool,
        gl_texture_id: Option<u32>,
        has_hook: bool,
        has_frames: bool,
        decode_cb: Option<usize>,
        current_frame_id: Option<u64>,
        thread_id: Option<ThreadId>,
        has_seek_sender: bool,
    }

    fn read_state(data: &mut RefAny) -> StateSummary {
        let s = data
            .downcast_ref::<VideoWidgetState>()
            .expect("payload must still be a VideoWidgetState");
        StateSummary {
            started: s.started,
            gl_texture_id: s.gl_texture_id,
            has_hook: matches!(s.on_frame, OptionOnVideoFrame::Some(_)),
            has_frames: matches!(s.frames, OptionRefAny::Some(_)),
            decode_cb: s.decode_callback.as_ref().map(|c| c.cb as usize),
            current_frame_id: s.current_frame.as_ref().map(|i| i.id),
            thread_id: s.thread_id,
            has_seek_sender: s.seek_sender.is_some(),
        }
    }

    fn read_config(data: &mut RefAny) -> VideoConfig {
        data.downcast_ref::<VideoWidgetState>()
            .expect("payload must still be a VideoWidgetState")
            .config
            .clone()
    }

    /// The `(width, height)` of every frame in the state's replay list, or
    /// `None` when there is no list / it does not hold a `Vec<VideoFrame>`.
    fn state_frames(data: &mut RefAny) -> Option<Vec<(u32, u32)>> {
        let inner = {
            let s = data.downcast_ref::<VideoWidgetState>()?;
            match &s.frames {
                OptionRefAny::Some(f) => Some(f.clone()),
                OptionRefAny::None => None,
            }
        };
        let mut inner = inner?;
        let v = inner.downcast_ref::<Vec<VideoFrame>>()?;
        Some(v.iter().map(|f| (f.width, f.height)).collect())
    }

    /// The `(width, height)` of a widget's `frames` `RefAny` (same shape as
    /// `state_frames`, but for the builder-side `VideoWidget`).
    fn widget_frames(widget: &VideoWidget) -> Option<Vec<(u32, u32)>> {
        let OptionRefAny::Some(f) = &widget.frames else {
            return None;
        };
        let mut f = f.clone();
        let v = f.downcast_ref::<Vec<VideoFrame>>()?;
        Some(v.iter().map(|fr| (fr.width, fr.height)).collect())
    }

    // ---- frames / images --------------------------------------------------

    /// A tightly-packed RGBA frame (`width * height * 4` bytes).
    fn frame(width: u32, height: u32) -> VideoFrame {
        let px = (width as usize) * (height as usize);
        VideoFrame {
            width,
            height,
            bytes: vec![7u8; px * 4].into(),
        }
    }

    /// A frame whose declared dimensions need NOT match its byte count.
    fn frame_raw(width: u32, height: u32, bytes: Vec<u8>) -> VideoFrame {
        VideoFrame {
            width,
            height,
            bytes: bytes.into(),
        }
    }

    /// A zero-allocation stand-in for an already-decoded frame.
    fn placeholder_image(tag: &[u8]) -> ImageRef {
        ImageRef::null_image(4, 4, RawImageFormat::BGRA8, tag.to_vec())
    }

    /// `(width, height)` of the raw CPU image a writeback stored, or `None` if
    /// the stored image is not a raw one.
    fn raw_dims(img: &ImageRef) -> Option<(usize, usize)> {
        match img.get_data() {
            DecodedImage::Raw((descriptor, _)) => Some((descriptor.width, descriptor.height)),
            _ => None,
        }
    }

    fn current_frame_dims(data: &mut RefAny) -> Option<(usize, usize)> {
        let s = data.downcast_ref::<VideoWidgetState>()?;
        raw_dims(s.current_frame.as_ref()?)
    }

    // ---- frame hook -------------------------------------------------------

    /// Records every frame the widget's `on_frame` hook is handed, and answers
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
        // A distinct body so the linker cannot fold this onto `record_frame`
        // and make the fn-pointer identity assertions vacuous.
        core::hint::black_box(Update::DoNothing)
    }

    fn frame_log(reply: Update) -> RefAny {
        RefAny::new(FrameLog {
            seen: Vec::new(),
            reply,
        })
    }

    fn logged_frames(data: &mut RefAny) -> Vec<(u32, u32, usize)> {
        data.downcast_ref::<FrameLog>()
            .expect("payload must still be a FrameLog")
            .seen
            .clone()
    }

    fn hook_into(log: &RefAny) -> OptionOnVideoFrame {
        Some(OnVideoFrame {
            refany: log.clone(),
            callback: (record_frame as OnVideoFrameCallbackType).into(),
        })
        .into()
    }

    // ---- thread workers ---------------------------------------------------

    /// A decode worker that returns immediately. Used wherever a test must let
    /// `AfterMount` really spawn a `Thread`: the framework's thread destructor
    /// *joins*, so only a worker that returns on its own can be joined safely.
    extern "C" fn noop_decode_worker(_: RefAny, _: ThreadSender, _: ThreadReceiver) {}

    extern "C" fn other_noop_worker(_: RefAny, _: ThreadSender, _: ThreadReceiver) {
        core::hint::black_box(());
    }

    // ==================================================================
    // CallbackInfo harness
    // ==================================================================

    /// Runs `f` against a real `CallbackInfo` over an empty `LayoutWindow` (no
    /// GL context, no laid-out nodes). Returns `f`'s value plus every
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

    fn count_virtual_view_rerenders(changes: &[CallbackChange]) -> usize {
        changes
            .iter()
            .filter(|c| matches!(c, CallbackChange::UpdateAllVirtualViews))
            .count()
    }

    /// The `ThreadId` of the single `AddThread` change, or `None`.
    fn added_thread_id(changes: &[CallbackChange]) -> Option<ThreadId> {
        changes.iter().find_map(|c| match c {
            CallbackChange::AddThread { thread_id, .. } => Some(*thread_id),
            _ => None,
        })
    }

    // ==================================================================
    // VirtualViewCallbackInfo harness
    // ==================================================================

    /// Runs `f` against a `VirtualViewCallbackInfo` reporting `w x h` bounds.
    fn with_virtual_view_info<R>(w: f32, h: f32, f: impl FnOnce(VirtualViewCallbackInfo) -> R) -> R {
        let fonts = FcFontCache::default();
        let images = ImageCache::default();
        let size = LogicalSize::new(w, h);
        let info = VirtualViewCallbackInfo::new(
            VirtualViewCallbackReason::InitialRender,
            &fonts,
            &images,
            WindowTheme::LightMode,
            HidpiAdjustedBounds {
                logical_size: size,
                hidpi_factor: DpiScaleFactor::new(1.0),
            },
            azul_core::geom::LogicalRect::new(LogicalPosition::zero(), size),
            azul_core::geom::LogicalRect::new(LogicalPosition::zero(), size),
            LogicalPosition::zero(),
        );
        f(info)
    }

    /// The `ImageRef` id of the `<img>` a render pass emitted, or `None` when it
    /// emitted no DOM at all.
    fn rendered_image_id(ret: &VirtualViewReturn) -> Option<u64> {
        let OptionDom::Some(dom) = &ret.dom else {
            return None;
        };
        match dom.root.get_node_type() {
            NodeType::Image(img) => Some(img.id),
            other => panic!("the video VirtualView must render an <img>, got {other:?}"),
        }
    }

    fn rendered_nothing(ret: &VirtualViewReturn) -> bool {
        matches!(ret.dom, OptionDom::None)
    }

    // ==================================================================
    // Worker harness
    // ==================================================================

    /// One frame a worker pushed, summarised so the (multi-megabyte) pixel
    /// buffer never has to be cloned into the log.
    #[derive(Debug, Clone, PartialEq, Eq)]
    struct SentFrame {
        width: u32,
        height: u32,
        len: usize,
        /// Distinct RGBA pixels in the first scanline, in first-seen order
        /// (capped, so a pathological frame cannot blow up the log).
        row0_palette: Vec<[u8; 4]>,
        /// Every scanline is byte-identical to the first.
        rows_identical: bool,
        /// The whole pixel buffer - captured only for frames small enough to
        /// compare byte-for-byte (the replay fixtures).
        small_bytes: Option<Vec<u8>>,
    }

    fn summarise(f: &VideoFrame) -> SentFrame {
        let bytes = f.bytes.as_ref();
        let row_len = (f.width as usize).saturating_mul(4);
        let mut row0_palette: Vec<[u8; 4]> = Vec::new();
        if row_len > 0 && bytes.len() >= row_len {
            for px in bytes[..row_len].chunks_exact(4) {
                let px = [px[0], px[1], px[2], px[3]];
                if row0_palette.len() < 32 && !row0_palette.contains(&px) {
                    row0_palette.push(px);
                }
            }
        }
        let rows_identical = row_len == 0
            || bytes.len() < row_len
            || bytes.chunks_exact(row_len).all(|row| row == &bytes[..row_len]);
        SentFrame {
            width: f.width,
            height: f.height,
            len: bytes.len(),
            row0_palette,
            rows_identical,
            small_bytes: (bytes.len() <= 4096).then(|| bytes.to_vec()),
        }
    }

    /// Guarded by `WORKER_GATE`: a worker's send callback is a plain C fn
    /// pointer, so it has nowhere but a static to put its result.
    static WORKER_LOG: Mutex<Vec<SentFrame>> = Mutex::new(Vec::new());
    static WORKER_GATE: Mutex<()> = Mutex::new(());
    /// How many more sends are accepted before the harness reports "the main
    /// thread is gone" - the only signal these workers ever stop on.
    static ACCEPT_BUDGET: AtomicUsize = AtomicUsize::new(0);

    extern "C" fn record_then_maybe_stop(
        _sender: *const core::ffi::c_void,
        msg: ThreadReceiveMsg,
    ) -> bool {
        let ThreadReceiveMsg::WriteBack(mut wb) = msg else {
            return false;
        };
        if let Some(f) = wb.refany.downcast_ref::<VideoFrame>() {
            WORKER_LOG
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .push(summarise(&f));
        }
        let left = ACCEPT_BUDGET.load(Ordering::SeqCst);
        if left == 0 {
            return false;
        }
        ACCEPT_BUDGET.store(left - 1, Ordering::SeqCst);
        true
    }

    extern "C" fn sender_drop_noop(_: *mut ThreadSenderInner) {}
    extern "C" fn receiver_drop_noop(_: *mut ThreadReceiverInner) {}
    extern "C" fn recv_nothing(_: *const core::ffi::c_void) -> OptionThreadSendMsg {
        OptionThreadSendMsg::None
    }
    /// A receiver that answers *every* poll with "terminate now".
    extern "C" fn recv_terminate(_: *const core::ffi::c_void) -> OptionThreadSendMsg {
        OptionThreadSendMsg::Some(ThreadSendMsg::TerminateThread)
    }

    fn logging_sender() -> (Receiver<ThreadReceiveMsg>, ThreadSender) {
        let (tx, rx) = channel::<ThreadReceiveMsg>();
        let sender = ThreadSender::new(ThreadSenderInner {
            ptr: Box::new(tx),
            send_fn: ThreadSendCallback {
                cb: record_then_maybe_stop,
            },
            destructor: ThreadSenderDestructorCallback {
                cb: sender_drop_noop,
            },
        });
        (rx, sender)
    }

    fn receiver(terminate: bool) -> (Sender<ThreadSendMsg>, ThreadReceiver) {
        let (tx, rx) = channel::<ThreadSendMsg>();
        let cb: extern "C" fn(*const core::ffi::c_void) -> OptionThreadSendMsg =
            if terminate { recv_terminate } else { recv_nothing };
        let receiver = ThreadReceiver::new(ThreadReceiverInner {
            ptr: Box::new(rx),
            recv_fn: ThreadRecvCallback { cb },
            destructor: ThreadReceiverDestructorCallback {
                cb: receiver_drop_noop,
            },
        });
        (tx, receiver)
    }

    /// Runs `worker` in-process against a sender that accepts `accept` frames
    /// and then reports failure, and returns everything it managed to send.
    /// `terminate` decides whether its receiver answers every poll with
    /// `TerminateThread`.
    fn run_worker(
        worker: ThreadCallbackType,
        init: RefAny,
        accept: usize,
        terminate: bool,
    ) -> Vec<SentFrame> {
        let _gate = WORKER_GATE
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        WORKER_LOG
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
        ACCEPT_BUDGET.store(accept, Ordering::SeqCst);

        let (_rx, sender) = logging_sender();
        let (_tx, recv) = receiver(terminate);
        worker(init, sender, recv);

        WORKER_LOG
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The SMPTE bars `video_test_worker` hard-codes (duplicated here on
    /// purpose: the palette is observable output, so a silent change to it must
    /// break a test).
    const EXPECTED_BARS: [[u8; 4]; 7] = [
        [235, 235, 235, 255],
        [235, 235, 16, 255],
        [16, 235, 235, 255],
        [16, 235, 16, 255],
        [235, 16, 235, 255],
        [235, 16, 16, 255],
        [16, 16, 235, 255],
    ];

    // ==================================================================
    // Seek-channel helpers (merge_video_state)
    // ==================================================================

    fn custom_f32(msg: &ThreadSendMsg) -> Option<f32> {
        let ThreadSendMsg::Custom(r) = msg else {
            return None;
        };
        let mut r = r.clone();
        let out = r.downcast_ref::<f32>().map(|v| *v);
        out
    }

    fn custom_source(msg: &ThreadSendMsg) -> Option<VideoSource> {
        let ThreadSendMsg::Custom(r) = msg else {
            return None;
        };
        let mut r = r.clone();
        let out = r.downcast_ref::<VideoSource>().map(|v| (*v).clone());
        out
    }

    // ==================================================================
    // VideoWidget::create  (constructor)
    // ==================================================================

    #[test]
    fn create_stores_the_config_verbatim_and_leaves_every_hook_unset() {
        for cfg in all_configs() {
            let widget = VideoWidget::create(cfg.clone());
            assert_same_config(&widget.config, &cfg);
            assert!(
                matches!(widget.on_frame, OptionOnVideoFrame::None),
                "a fresh widget has no frame hook"
            );
            assert!(
                matches!(widget.frames, OptionRefAny::None),
                "a fresh widget has no replay list"
            );
        }
    }

    #[test]
    fn create_is_usable_in_a_const_context() {
        let widget = CONST_WIDGET;
        assert_same_config(&widget.config, &CONST_CONFIG);
        assert!(matches!(widget.on_frame, OptionOnVideoFrame::None));
        assert!(matches!(widget.frames, OptionRefAny::None));
    }

    // ==================================================================
    // VideoWidget::set_on_frame / with_on_frame  (constructor)
    // ==================================================================

    #[test]
    fn with_on_frame_installs_the_hook_and_keeps_the_config() {
        for cfg in all_configs() {
            let widget = VideoWidget::create(cfg.clone())
                .with_on_frame(frame_log(Update::DoNothing), record_frame as OnVideoFrameCallbackType);

            assert_same_config(&widget.config, &cfg);
            let OptionOnVideoFrame::Some(hook) = &widget.on_frame else {
                panic!("with_on_frame must install a hook");
            };
            assert_eq!(
                hook.callback.cb as usize,
                record_frame as OnVideoFrameCallbackType as usize,
                "the installed hook must be exactly the one handed in"
            );
            assert!(
                matches!(widget.frames, OptionRefAny::None),
                "with_on_frame must not invent a replay list"
            );
        }
    }

    #[test]
    fn set_on_frame_twice_keeps_only_the_last_hook() {
        let mut widget = VideoWidget::create(VideoConfig::default());
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
        let mut data = hook.refany.clone();
        assert_eq!(
            data.downcast_ref::<usize>().map(|v| *v),
            Some(1),
            "the second hook's data must replace the first hook's, not merge with it"
        );
    }

    #[test]
    fn set_on_frame_accepts_a_refany_that_is_also_the_widgets_replay_list() {
        // Aliasing the same RefAny into two slots must not panic or deadlock -
        // both are plain shared handles.
        let shared = RefAny::new(vec![frame(1, 1)]);
        let widget = VideoWidget::create(VideoConfig::default())
            .with_frames(shared.clone())
            .with_on_frame(shared, record_frame as OnVideoFrameCallbackType);

        assert!(matches!(widget.on_frame, OptionOnVideoFrame::Some(_)));
        assert_eq!(widget_frames(&widget), Some(vec![(1, 1)]));
    }

    // ==================================================================
    // VideoWidget::with_frames  (constructor)
    // ==================================================================

    #[test]
    fn with_frames_stores_the_list_and_keeps_everything_else() {
        for cfg in all_configs() {
            let widget = VideoWidget::create(cfg.clone())
                .with_frames(RefAny::new(vec![frame(2, 3), frame(4, 5)]));

            assert_same_config(&widget.config, &cfg);
            assert_eq!(widget_frames(&widget), Some(vec![(2, 3), (4, 5)]));
            assert!(
                matches!(widget.on_frame, OptionOnVideoFrame::None),
                "with_frames must not invent a hook"
            );
        }
    }

    #[test]
    fn with_frames_twice_keeps_only_the_last_list() {
        let widget = VideoWidget::create(VideoConfig::default())
            .with_frames(RefAny::new(vec![frame(1, 1)]))
            .with_frames(RefAny::new(vec![frame(9, 9), frame(8, 8)]));
        assert_eq!(widget_frames(&widget), Some(vec![(9, 9), (8, 8)]));
    }

    #[test]
    fn with_frames_accepts_an_empty_and_a_wrong_typed_payload_without_complaint() {
        // Documented: a `RefAny` that does not carry a `Vec<VideoFrame>` is
        // accepted here and only *skipped* later, by the replay worker.
        let empty = VideoWidget::create(VideoConfig::default())
            .with_frames(RefAny::new(Vec::<VideoFrame>::new()));
        assert_eq!(widget_frames(&empty), Some(Vec::new()));

        let foreign =
            VideoWidget::create(VideoConfig::default()).with_frames(RefAny::new(0_u32));
        assert!(
            matches!(foreign.frames, OptionRefAny::Some(_)),
            "the builder stores whatever it is given"
        );
        assert_eq!(
            widget_frames(&foreign),
            None,
            "...but it is not a frame list"
        );
    }

    #[test]
    fn builder_order_does_not_matter() {
        let a = VideoWidget::create(config(file_source("/a.mp4"), 1.0))
            .with_frames(RefAny::new(vec![frame(3, 3)]))
            .with_on_frame(frame_log(Update::DoNothing), record_frame as OnVideoFrameCallbackType);
        let b = VideoWidget::create(config(file_source("/a.mp4"), 1.0))
            .with_on_frame(frame_log(Update::DoNothing), record_frame as OnVideoFrameCallbackType)
            .with_frames(RefAny::new(vec![frame(3, 3)]));

        assert_same_config(&a.config, &b.config);
        assert_eq!(widget_frames(&a), widget_frames(&b));
        assert!(matches!(a.on_frame, OptionOnVideoFrame::Some(_)));
        assert!(matches!(b.on_frame, OptionOnVideoFrame::Some(_)));
    }

    // ==================================================================
    // VideoWidget::dom / dom_with_decoder / build_dom
    // ==================================================================

    #[test]
    fn dom_builds_a_div_with_one_virtual_view_child() {
        let dom = VideoWidget::create(VideoConfig::default()).dom();

        assert!(
            matches!(dom.root.get_node_type(), NodeType::Div),
            "the widget root is a plain div (the <img> lives in the VirtualView)"
        );
        assert_eq!(dom.children.as_slice().len(), 1, "one VirtualView child");
        assert!(
            matches!(
                dom.children.as_slice()[0].root.get_node_type(),
                NodeType::VirtualView
            ),
            "the child must be the VirtualView the decode worker re-renders"
        );
    }

    #[test]
    fn dom_wires_after_mount_node_resized_a_dataset_and_a_merge_callback() {
        let dom = VideoWidget::create(VideoConfig::default()).dom();

        let events: Vec<EventFilter> = dom
            .root
            .get_callbacks()
            .as_ref()
            .iter()
            .map(|c| c.event)
            .collect();
        assert_eq!(events.len(), 2, "exactly two component callbacks");
        assert!(events.contains(&EventFilter::Component(ComponentEventFilter::AfterMount)));
        assert!(events.contains(&EventFilter::Component(ComponentEventFilter::NodeResized)));
        assert!(
            dom.root.get_merge_callback().is_some(),
            "live state must survive relayout"
        );
        assert!(
            dom.root.get_dataset().is_some(),
            "the widget div must carry its VideoWidgetState"
        );
    }

    #[test]
    fn dom_stores_a_pristine_state_for_every_config() {
        for cfg in all_configs() {
            let dom = VideoWidget::create(cfg.clone()).dom();
            let mut dataset = dom
                .root
                .get_dataset()
                .cloned()
                .expect("the node must carry its VideoWidgetState");

            assert_same_config(&read_config(&mut dataset), &cfg);
            assert_eq!(
                read_state(&mut dataset),
                StateSummary {
                    started: false,
                    gl_texture_id: None,
                    has_hook: false,
                    has_frames: false,
                    decode_cb: None,
                    current_frame_id: None,
                    thread_id: None,
                    has_seek_sender: false,
                },
                "dom() must not start anything - AfterMount does that"
            );
        }
    }

    #[test]
    fn dom_moves_the_hook_and_the_replay_list_into_the_state() {
        let dom = VideoWidget::create(VideoConfig::default())
            .with_frames(RefAny::new(vec![frame(6, 7)]))
            .with_on_frame(frame_log(Update::DoNothing), record_frame as OnVideoFrameCallbackType)
            .dom();

        let mut dataset = dom.root.get_dataset().cloned().expect("dataset");
        let summary = read_state(&mut dataset);
        assert!(summary.has_hook, "dom() must carry the user hook forward");
        assert!(summary.has_frames);
        assert_eq!(state_frames(&mut dataset), Some(vec![(6, 7)]));
    }

    #[test]
    fn dom_with_decoder_records_exactly_the_worker_it_was_given() {
        let dom = VideoWidget::create(VideoConfig::default())
            .dom_with_decoder(ThreadCallback::new(noop_decode_worker));

        let mut dataset = dom.root.get_dataset().cloned().expect("dataset");
        assert_eq!(
            read_state(&mut dataset).decode_cb,
            Some(noop_decode_worker as ThreadCallbackType as usize)
        );

        // A different worker must be distinguishable (no fn-pointer folding).
        let other = VideoWidget::create(VideoConfig::default())
            .dom_with_decoder(ThreadCallback::new(other_noop_worker));
        let mut other_dataset = other.root.get_dataset().cloned().expect("dataset");
        assert_ne!(
            read_state(&mut other_dataset).decode_cb,
            read_state(&mut dataset).decode_cb
        );
    }

    #[test]
    fn dom_and_dom_with_decoder_agree_on_everything_but_the_worker() {
        let plain = VideoWidget::create(config(bytes_source(vec![1, 2, 3]), -0.5)).dom();
        let with_cb = VideoWidget::create(config(bytes_source(vec![1, 2, 3]), -0.5))
            .dom_with_decoder(ThreadCallback::new(noop_decode_worker));

        assert_eq!(
            plain.children.as_slice().len(),
            with_cb.children.as_slice().len()
        );
        let mut a = plain.root.get_dataset().cloned().expect("dataset");
        let mut b = with_cb.root.get_dataset().cloned().expect("dataset");
        assert_same_config(&read_config(&mut a), &read_config(&mut b));

        let (sa, sb) = (read_state(&mut a), read_state(&mut b));
        assert_eq!(sa.decode_cb, None);
        assert!(sb.decode_cb.is_some());
        assert_eq!(
            StateSummary {
                decode_cb: None,
                ..sb
            },
            sa,
            "only the decode callback may differ"
        );
    }

    #[test]
    fn dom_survives_a_huge_in_memory_source_without_copying_it_into_the_tree() {
        // 4 MiB of "MP4 bytes": the widget must move them into the state, not
        // choke on them.
        let widget = VideoWidget::create(config(bytes_source(vec![0xAB; 4 * 1024 * 1024]), 0.0));
        let dom = widget.dom();
        let mut dataset = dom.root.get_dataset().cloned().expect("dataset");
        match read_config(&mut dataset).source {
            VideoSource::Bytes(b) => assert_eq!(b.as_ref().len(), 4 * 1024 * 1024),
            other => panic!("the source must survive verbatim, got {other:?}"),
        }
    }

    // ==================================================================
    // video_widget_render  (VirtualView callback)
    // ==================================================================

    #[test]
    fn render_with_non_finite_or_empty_bounds_emits_no_dom() {
        let mut s = base_state(VideoConfig::default());
        s.current_frame = Some(placeholder_image(b"ready"));
        let dataset = RefAny::new(s);

        for (w, h) in [
            (0.0_f32, 0.0_f32),
            (0.0, 600.0),
            (800.0, 0.0),
            (-800.0, -600.0),
            (-1.0, 600.0),
            (f32::NAN, 600.0),
            (800.0, f32::NAN),
            (f32::INFINITY, 600.0),
            (800.0, f32::NEG_INFINITY),
        ] {
            let ret = with_virtual_view_info(w, h, |info| video_widget_render(dataset.clone(), info));
            assert!(
                rendered_nothing(&ret),
                "bounds {w}x{h} must render nothing until layout settles - even with a frame ready"
            );
        }
    }

    #[test]
    fn render_with_a_wrong_typed_dataset_emits_no_dom() {
        let dataset = RefAny::new(0_u32);
        let ret =
            with_virtual_view_info(800.0, 600.0, |info| video_widget_render(dataset.clone(), info));
        assert!(rendered_nothing(&ret));
    }

    #[test]
    fn render_before_the_first_frame_emits_the_no_signal_poster() {
        // PIN FLIPPED (2026-07-31, deliberately): rendering NOTHING before
        // the first frame made a dead decode pipeline (missing feature,
        // unsupported target, Vulkan init failure, network stall)
        // indistinguishable from a black video — the shipped azul-video
        // "black frame" bug. A decoder that has produced no frame must be
        // VISIBLY "no signal".
        let dataset = state(VideoConfig::default());
        let ret =
            with_virtual_view_info(800.0, 600.0, |info| video_widget_render(dataset.clone(), info));
        assert!(
            !rendered_nothing(&ret),
            "no decoded frame yet -> a visible no-signal poster, NOT an \
             invisible tile"
        );
    }

    #[test]
    fn render_emits_the_stored_frame_as_an_image() {
        let img = placeholder_image(b"azul-video-frame");
        let expected_id = img.id;
        let mut s = base_state(VideoConfig::default());
        s.current_frame = Some(img);
        let dataset = RefAny::new(s);

        let ret =
            with_virtual_view_info(800.0, 600.0, |info| video_widget_render(dataset.clone(), info));
        assert_eq!(
            rendered_image_id(&ret),
            Some(expected_id),
            "the <img> must show exactly the frame the writeback stored"
        );
    }

    #[test]
    fn render_reports_the_bounds_back_as_the_scroll_size() {
        let dataset = state(VideoConfig::default());
        let ret =
            with_virtual_view_info(640.0, 480.0, |info| video_widget_render(dataset.clone(), info));

        assert_eq!(ret.materialized.size.width, 640.0);
        assert_eq!(ret.materialized.size.height, 480.0);
        assert_eq!(ret.virtual_rect.size.width, 640.0);
        assert_eq!(ret.virtual_rect.size.height, 480.0);
        assert_eq!((ret.materialized.origin.x, ret.materialized.origin.y), (0.0, 0.0));
        assert_eq!(
            (ret.virtual_rect.origin.x, ret.virtual_rect.origin.y),
            (0.0, 0.0)
        );
    }

    #[test]
    fn render_echoes_even_a_nan_bound_into_the_scroll_size() {
        // The early-out only suppresses the DOM: the reported scroll size is
        // still whatever layout handed in, NaN included.
        let dataset = state(VideoConfig::default());
        let ret = with_virtual_view_info(f32::NAN, 480.0, |info| {
            video_widget_render(dataset.clone(), info)
        });
        assert!(rendered_nothing(&ret));
        assert!(ret.materialized.size.width.is_nan());
        assert_eq!(ret.materialized.size.height, 480.0);
    }

    #[test]
    fn render_is_pure_and_repeatable() {
        let img = placeholder_image(b"stable");
        let expected_id = img.id;
        let mut s = base_state(VideoConfig::default());
        s.current_frame = Some(img);
        s.started = true;
        let mut dataset = RefAny::new(s);

        for _ in 0..8 {
            let ret = with_virtual_view_info(320.0, 240.0, |info| {
                video_widget_render(dataset.clone(), info)
            });
            assert_eq!(rendered_image_id(&ret), Some(expected_id));
        }
        let summary = read_state(&mut dataset);
        assert!(summary.started, "render must not touch the live state");
        assert_eq!(summary.current_frame_id, Some(expected_id));
    }

    #[test]
    fn render_with_the_smallest_positive_bounds_still_emits_the_image() {
        let img = placeholder_image(b"tiny");
        let expected_id = img.id;
        let mut s = base_state(VideoConfig::default());
        s.current_frame = Some(img);
        let dataset = RefAny::new(s);

        for (w, h) in [(f32::MIN_POSITIVE, f32::MIN_POSITIVE), (1.0, 1.0), (f32::MAX, f32::MAX)] {
            let ret = with_virtual_view_info(w, h, |info| video_widget_render(dataset.clone(), info));
            assert_eq!(
                rendered_image_id(&ret),
                Some(expected_id),
                "{w}x{h} is finite and positive, so the frame must render"
            );
        }
    }

    // ==================================================================
    // video_on_after_mount
    //
    // NOTE: the default (test-pattern) mount path is deliberately NOT driven
    // here. `video_test_worker` never reads its receiver, so it ignores
    // `ThreadSendMsg::TerminateThread`; the framework's thread destructor
    // *joins* that worker and would hang the test binary forever (see the
    // report). Only workers that return on their own are mounted below.
    // ==================================================================

    #[test]
    fn after_mount_ignores_a_dataset_that_is_not_a_video_state() {
        let (update, changes) =
            with_callback_info(|info| video_on_after_mount(RefAny::new(0_u32), info));

        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "a foreign dataset must not start a decode thread"
        );
    }

    #[test]
    fn after_mount_is_a_no_op_once_the_decode_thread_has_started() {
        let mut s = base_state(VideoConfig::default());
        s.started = true;
        s.thread_id = Some(ThreadId::unique());
        s.current_frame = Some(placeholder_image(b"kept"));
        let mut data = RefAny::new(s);
        let before = read_state(&mut data);

        let (update, changes) = with_callback_info(|info| video_on_after_mount(data.clone(), info));

        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "AfterMount must start the decode thread at most once"
        );
        assert_eq!(
            read_state(&mut data),
            before,
            "a re-mount must not disturb the running state"
        );
    }

    #[test]
    fn after_mount_spawns_the_streaming_decoder_and_remembers_its_id_and_sender() {
        let mut s = base_state(config(file_source("/tmp/clip.mp4"), 12.5));
        s.decode_callback = Some(ThreadCallback::new(noop_decode_worker));
        let mut data = RefAny::new(s);

        let (update, changes) = with_callback_info(|info| video_on_after_mount(data.clone(), info));

        assert_eq!(update, Update::DoNothing, "mounting never triggers relayout");
        assert_eq!(changes.len(), 1, "exactly one thread is spawned");
        let tid = added_thread_id(&changes).expect("the decode worker must be added as a Thread");

        let summary = read_state(&mut data);
        assert!(summary.started);
        assert_eq!(
            summary.thread_id,
            Some(tid),
            "the state must remember the very thread id it registered (resize messaging)"
        );
        assert!(
            summary.has_seek_sender,
            "the merge callback needs the worker's sender to push seeks"
        );
    }

    #[test]
    fn after_mount_only_ever_spawns_one_decode_thread() {
        let mut s = base_state(VideoConfig::default());
        s.decode_callback = Some(ThreadCallback::new(noop_decode_worker));
        let mut data = RefAny::new(s);

        let (_, first) = with_callback_info(|info| video_on_after_mount(data.clone(), info));
        let first_id = read_state(&mut data).thread_id;
        let (_, second) = with_callback_info(|info| video_on_after_mount(data.clone(), info));

        assert_eq!(first.len(), 1);
        assert!(second.is_empty(), "the second AfterMount must be a no-op");
        assert_eq!(
            read_state(&mut data).thread_id,
            first_id,
            "the recorded thread id must not be re-rolled"
        );
    }

    #[test]
    fn after_mount_replay_path_spawns_a_worker_but_records_no_id_or_sender() {
        // ADVERSARIAL: the replay path spawns a `Thread` like the streaming path
        // does, but stores neither its `ThreadId` nor its sender - so resize
        // re-targeting and scrub/seek messaging are silently dead for replayed
        // clips (see the report). An EMPTY frame list is used so the worker
        // returns immediately and can be joined.
        let mut s = base_state(VideoConfig::default());
        s.frames = OptionRefAny::Some(RefAny::new(Vec::<VideoFrame>::new()));
        let mut data = RefAny::new(s);

        let (update, changes) = with_callback_info(|info| video_on_after_mount(data.clone(), info));

        assert_eq!(update, Update::DoNothing);
        assert_eq!(changes.len(), 1, "the replay worker is still spawned");
        let summary = read_state(&mut data);
        assert!(summary.started);
        assert_eq!(summary.thread_id, None);
        assert!(!summary.has_seek_sender);
    }

    #[test]
    fn after_mount_replay_path_accepts_a_wrong_typed_frame_list() {
        // A `RefAny` that is not a `Vec<VideoFrame>` must not panic the mount -
        // the worker just returns.
        let mut s = base_state(VideoConfig::default());
        s.frames = OptionRefAny::Some(RefAny::new("not a frame list"));
        let mut data = RefAny::new(s);

        let (update, changes) = with_callback_info(|info| video_on_after_mount(data.clone(), info));

        assert_eq!(update, Update::DoNothing);
        assert_eq!(changes.len(), 1);
        assert!(read_state(&mut data).started);
    }

    #[test]
    fn after_mount_prefers_the_streaming_decoder_over_a_replay_list() {
        // Documented priority: decode worker > replay frames > test pattern.
        // A NON-empty replay list is safe here precisely because it must NOT be
        // used (the replay worker would otherwise loop forever).
        let mut s = base_state(VideoConfig::default());
        s.decode_callback = Some(ThreadCallback::new(noop_decode_worker));
        s.frames = OptionRefAny::Some(RefAny::new(vec![frame(2, 2), frame(2, 2)]));
        let mut data = RefAny::new(s);

        let (_, changes) = with_callback_info(|info| video_on_after_mount(data.clone(), info));

        assert_eq!(changes.len(), 1);
        let summary = read_state(&mut data);
        assert!(
            summary.thread_id.is_some() && summary.has_seek_sender,
            "only the streaming path records an id + sender, so it is the one that ran"
        );
        assert!(summary.has_frames, "the replay list is kept, just unused");
    }

    // ==================================================================
    // video_on_resize
    // ==================================================================

    #[test]
    fn resize_ignores_a_dataset_that_is_not_a_video_state() {
        let (update, changes) = with_callback_info(|info| video_on_resize(RefAny::new(0_u32), info));
        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
    }

    #[test]
    fn resize_before_the_worker_started_is_a_no_op() {
        let mut data = state(VideoConfig::default());
        let before = read_state(&mut data);

        let (update, changes) = with_callback_info(|info| video_on_resize(data.clone(), info));

        assert_eq!(
            update,
            Update::DoNothing,
            "resize is a message, never a relayout"
        );
        assert!(changes.is_empty(), "no worker -> nothing to tell");
        assert_eq!(read_state(&mut data), before);
    }

    #[test]
    fn resize_with_an_unknown_node_is_a_no_op() {
        // The state knows a thread id, but the hit node has no laid-out size in
        // this (empty) window: the callback must bail instead of messaging a
        // bogus target size.
        let mut s = base_state(VideoConfig::default());
        s.started = true;
        s.thread_id = Some(ThreadId::unique());
        let mut data = RefAny::new(s);
        let before = read_state(&mut data);

        let (update, changes) = with_callback_info(|info| video_on_resize(data.clone(), info));

        assert_eq!(update, Update::DoNothing);
        assert!(changes.is_empty());
        assert_eq!(read_state(&mut data), before);
    }

    #[test]
    fn resize_with_a_thread_id_that_no_longer_exists_is_a_no_op() {
        // A worker that already exited: `get_thread` returns None and the
        // best-effort send is simply skipped.
        let mut s = base_state(VideoConfig::default());
        s.thread_id = Some(ThreadId::unique());
        s.started = true;
        let data = RefAny::new(s);

        for _ in 0..4 {
            let (update, changes) = with_callback_info(|info| video_on_resize(data.clone(), info));
            assert_eq!(update, Update::DoNothing);
            assert!(changes.is_empty());
        }
    }

    // ==================================================================
    // video_test_worker
    // ==================================================================

    #[test]
    fn test_worker_stops_as_soon_as_the_main_thread_stops_receiving() {
        let sent = run_worker(video_test_worker, RefAny::new(()), 0, false);

        assert_eq!(
            sent.len(),
            1,
            "the worker must stop after the first rejected send, not spin"
        );
        assert_eq!((sent[0].width, sent[0].height), (1280, 720));
        assert_eq!(
            sent[0].len,
            1280 * 720 * 4,
            "a frame is exactly width * height * 4 tightly-packed RGBA bytes"
        );
    }

    #[test]
    fn test_worker_emits_seven_opaque_smpte_bars_in_order() {
        let sent = run_worker(video_test_worker, RefAny::new(()), 0, false);
        let f = &sent[0];

        assert!(
            f.rows_identical,
            "the bars scroll horizontally only - every scanline must be identical"
        );
        assert_eq!(
            f.row0_palette,
            EXPECTED_BARS.to_vec(),
            "tick 0 must emit the seven SMPTE bars left-to-right, all fully opaque"
        );
    }

    #[test]
    fn test_worker_ignores_terminate_and_scrolls_the_pattern() {
        // ADVERSARIAL: the worker never polls its receiver, so `TerminateThread`
        // - the message the framework's thread destructor sends before joining -
        // is ignored outright. The only thing that stops it is a failed send.
        let sent = run_worker(video_test_worker, RefAny::new(()), 3, true);

        assert_eq!(
            sent.len(),
            4,
            "3 accepted + 1 rejected: TerminateThread did not stop the worker"
        );
        for f in &sent {
            assert_eq!(f.len, 1280 * 720 * 4);
            assert!(f.rows_identical);
        }
        // tick advances by 2 per frame and the shift is `tick / 4`, so frames
        // 0+1 share a phase and frame 2 is rotated by exactly one bar.
        assert_eq!(sent[0].row0_palette, sent[1].row0_palette);
        assert_eq!(sent[2].row0_palette, sent[3].row0_palette);
        assert_ne!(
            sent[1].row0_palette, sent[2].row0_palette,
            "the pattern must actually scroll"
        );
        assert_eq!(
            sent[2].row0_palette[0], EXPECTED_BARS[1],
            "one tick of scroll rotates the palette by one bar"
        );
    }

    #[test]
    fn test_worker_ignores_its_init_payload_entirely() {
        // The test pattern is fixed-size: no init data can change it (or crash it).
        for init in [
            RefAny::new(()),
            RefAny::new(0_u32),
            RefAny::new(VideoConfig::default()),
            RefAny::new(vec![frame(1, 1)]),
        ] {
            let sent = run_worker(video_test_worker, init, 0, false);
            assert_eq!(sent.len(), 1);
            assert_eq!((sent[0].width, sent[0].height), (1280, 720));
        }
    }

    // ==================================================================
    // video_replay_worker
    // ==================================================================

    #[test]
    fn replay_worker_returns_immediately_for_a_wrong_typed_init() {
        for init in [
            RefAny::new(0_u32),
            RefAny::new("not a frame list"),
            RefAny::new(VideoConfig::default()),
            RefAny::new(frame(1, 1)),
        ] {
            let sent = run_worker(video_replay_worker, init, 8, false);
            assert!(
                sent.is_empty(),
                "a payload that is not a Vec<VideoFrame> must be skipped, not guessed at"
            );
        }
    }

    #[test]
    fn replay_worker_returns_immediately_for_an_empty_frame_list() {
        // Boundary: an empty list would make `idx % frames.len()` divide by
        // zero - the worker must bail first.
        let sent = run_worker(
            video_replay_worker,
            RefAny::new(Vec::<VideoFrame>::new()),
            8,
            false,
        );
        assert!(sent.is_empty());
    }

    #[test]
    fn replay_worker_sends_the_caller_frames_byte_for_byte() {
        let frames = vec![
            frame_raw(2, 1, vec![1, 2, 3, 4, 5, 6, 7, 8]),
            frame_raw(1, 2, vec![9, 10, 11, 12, 13, 14, 15, 16]),
        ];
        let sent = run_worker(video_replay_worker, RefAny::new(frames.clone()), 2, false);

        assert_eq!(sent.len(), 3, "2 accepted + 1 rejected");
        for (i, s) in sent.iter().enumerate() {
            let expected = &frames[i % frames.len()];
            assert_eq!((s.width, s.height), (expected.width, expected.height));
            assert_eq!(
                s.small_bytes.as_deref(),
                Some(expected.bytes.as_ref()),
                "frame {i} must be replayed verbatim - no re-encoding"
            );
        }
    }

    #[test]
    fn replay_worker_cycles_the_list_and_never_indexes_out_of_bounds() {
        let frames = vec![frame_raw(1, 1, vec![0; 4]), frame_raw(2, 2, vec![1; 16])];
        let sent = run_worker(video_replay_worker, RefAny::new(frames), 5, false);

        assert_eq!(sent.len(), 6);
        let widths: Vec<u32> = sent.iter().map(|s| s.width).collect();
        assert_eq!(widths, vec![1, 2, 1, 2, 1, 2], "the list must wrap around");
    }

    #[test]
    fn replay_worker_forwards_degenerate_frames_unchanged() {
        // A decoder can hand back a 0x0 frame or one whose byte count does not
        // match its dimensions; the replay worker is a pipe, not a validator -
        // it must not panic, truncate, or drop them.
        let frames = vec![
            frame_raw(0, 0, Vec::new()),
            frame_raw(u32::MAX, u32::MAX, Vec::new()),
            frame_raw(1, 1, vec![0xAB; 3]),
        ];
        let sent = run_worker(video_replay_worker, RefAny::new(frames), 2, false);

        assert_eq!(sent.len(), 3);
        assert_eq!((sent[0].width, sent[0].height, sent[0].len), (0, 0, 0));
        assert_eq!(
            (sent[1].width, sent[1].height, sent[1].len),
            (u32::MAX, u32::MAX, 0)
        );
        assert_eq!(sent[2].small_bytes.as_deref(), Some(&[0xAB, 0xAB, 0xAB][..]));
    }

    // ==================================================================
    // video_writeback
    // ==================================================================

    #[test]
    fn writeback_stores_the_frame_and_rerenders_the_virtual_view() {
        let mut data = state(VideoConfig::default());
        let frame_data = RefAny::new(frame(4, 3));

        let (update, changes) =
            with_callback_info(|info| video_writeback(data.clone(), frame_data.clone(), info));

        assert_eq!(update, Update::DoNothing, "no hook -> no user update");
        assert_eq!(
            count_virtual_view_rerenders(&changes),
            1,
            "the VirtualView must be re-rendered in place (never RefreshDom)"
        );
        assert_eq!(
            current_frame_dims(&mut data),
            Some((4, 3)),
            "the decoded frame becomes the widget's current CPU image"
        );
    }

    #[test]
    fn writeback_invokes_the_hook_with_the_exact_frame_and_returns_its_update() {
        let mut log = frame_log(Update::RefreshDom);
        let mut s = base_state(VideoConfig::default());
        s.on_frame = hook_into(&log);
        let mut data = RefAny::new(s);
        let frame_data = RefAny::new(frame(2, 2));

        let (update, changes) =
            with_callback_info(|info| video_writeback(data.clone(), frame_data.clone(), info));

        assert_eq!(update, Update::RefreshDom, "the hook's Update must win");
        assert_eq!(logged_frames(&mut log), vec![(2, 2, 16)]);
        assert_eq!(count_virtual_view_rerenders(&changes), 1);
        assert_eq!(current_frame_dims(&mut data), Some((2, 2)));
    }

    #[test]
    fn writeback_ignores_frame_data_of_the_wrong_type() {
        let mut log = frame_log(Update::RefreshDom);
        let mut s = base_state(VideoConfig::default());
        s.on_frame = hook_into(&log);
        let mut data = RefAny::new(s);

        let (update, changes) =
            with_callback_info(|info| video_writeback(data.clone(), RefAny::new(0_u32), info));

        assert_eq!(update, Update::DoNothing);
        assert!(
            changes.is_empty(),
            "no frame -> no re-render is scheduled at all"
        );
        assert!(
            logged_frames(&mut log).is_empty(),
            "the user hook must not fire without a frame"
        );
        assert_eq!(read_state(&mut data).current_frame_id, None);
    }

    #[test]
    fn writeback_survives_a_writeback_dataset_that_is_not_a_video_state() {
        let (update, changes) = with_callback_info(|info| {
            video_writeback(RefAny::new(0_u32), RefAny::new(frame(1, 1)), info)
        });

        assert_eq!(
            update,
            Update::DoNothing,
            "a foreign dataset means no hook and nowhere to store - but no panic"
        );
        assert_eq!(
            count_virtual_view_rerenders(&changes),
            1,
            "the re-render is still scheduled (documented cost of a stale dataset)"
        );
    }

    #[test]
    fn writeback_rejects_a_frame_whose_bytes_do_not_match_its_dimensions() {
        // A malformed/hostile frame: the image build must fail cleanly instead
        // of indexing out of bounds or allocating.
        let mut data = state(VideoConfig::default());

        for bogus in [
            frame_raw(u32::MAX, 1, Vec::new()),
            frame_raw(4, 4, vec![0; 4 * 4 * 4 - 1]),
            frame_raw(4, 4, vec![0; 4 * 4 * 4 + 1]),
            frame_raw(1, 1, Vec::new()),
            frame_raw(0, 0, vec![0; 4]),
        ] {
            let payload = RefAny::new(bogus);
            let (update, changes) =
                with_callback_info(|info| video_writeback(data.clone(), payload.clone(), info));

            assert_eq!(update, Update::DoNothing);
            assert_eq!(
                count_virtual_view_rerenders(&changes),
                1,
                "a rejected frame still costs a re-render"
            );
            assert_eq!(
                read_state(&mut data).current_frame_id,
                None,
                "a rejected frame must never become the displayed image"
            );
        }
    }

    #[test]
    fn writeback_accepts_an_empty_zero_by_zero_frame() {
        // Boundary: 0x0 with 0 bytes is internally consistent, so it is accepted
        // as a (degenerate) image rather than rejected.
        let mut data = state(VideoConfig::default());
        let empty = RefAny::new(frame_raw(0, 0, Vec::new()));

        let (update, _) =
            with_callback_info(|info| video_writeback(data.clone(), empty.clone(), info));

        assert_eq!(update, Update::DoNothing);
        assert_eq!(current_frame_dims(&mut data), Some((0, 0)));
    }

    #[test]
    fn writeback_replaces_the_previous_frame_every_time() {
        let mut s = base_state(VideoConfig::default());
        s.current_frame = Some(placeholder_image(b"old"));
        let mut data = RefAny::new(s);
        let old_id = read_state(&mut data).current_frame_id.expect("seeded");

        let f1 = RefAny::new(frame(2, 2));
        let (_, _) = with_callback_info(|info| video_writeback(data.clone(), f1.clone(), info));
        let id1 = read_state(&mut data).current_frame_id.expect("stored");
        assert_ne!(id1, old_id, "the stale placeholder must be replaced");

        let f2 = RefAny::new(frame(3, 3));
        let (_, _) = with_callback_info(|info| video_writeback(data.clone(), f2.clone(), info));
        let id2 = read_state(&mut data).current_frame_id.expect("stored");
        assert_ne!(id2, id1, "every frame installs a fresh image");
        assert_eq!(current_frame_dims(&mut data), Some((3, 3)));
    }

    #[test]
    fn writeback_keeps_the_last_good_frame_when_a_later_one_is_malformed() {
        let mut data = state(VideoConfig::default());
        let good = RefAny::new(frame(2, 2));
        let (_, _) = with_callback_info(|info| video_writeback(data.clone(), good.clone(), info));
        let good_id = read_state(&mut data).current_frame_id.expect("stored");

        let bad = RefAny::new(frame_raw(1024, 1024, vec![0; 16]));
        let (update, _) =
            with_callback_info(|info| video_writeback(data.clone(), bad.clone(), info));

        assert_eq!(update, Update::DoNothing);
        assert_eq!(
            read_state(&mut data).current_frame_id,
            Some(good_id),
            "a corrupt frame must not blank the picture"
        );
    }

    #[test]
    fn writeback_still_notifies_the_hook_for_a_frame_it_cannot_display() {
        // The hook is the user's data path (save / send), so it fires even when
        // the frame is unusable as an image - documented here so a change is
        // deliberate.
        let mut log = frame_log(Update::RefreshDom);
        let mut s = base_state(VideoConfig::default());
        s.on_frame = hook_into(&log);
        let mut data = RefAny::new(s);
        let bogus = RefAny::new(frame_raw(64, 64, vec![0; 3]));

        let (update, _) =
            with_callback_info(|info| video_writeback(data.clone(), bogus.clone(), info));

        assert_eq!(update, Update::RefreshDom);
        assert_eq!(logged_frames(&mut log), vec![(64, 64, 3)]);
        assert_eq!(read_state(&mut data).current_frame_id, None);
    }

    #[test]
    fn writeback_survives_dimensions_whose_byte_count_overflows_usize() {
        // ADVERSARIAL: a decoder reporting 2^31 x 2^31 makes the raw-image path
        // compute `width * height * 4` in usize -> 2^64, which overflows. In a
        // debug build that is an arithmetic-overflow panic; in release it wraps
        // and the empty buffer may be *accepted*. Neither is a graceful
        // rejection (see the report) - what must hold in both modes is that the
        // widget never ends up displaying a bogus image.
        let mut data = state(VideoConfig::default());
        let huge = RefAny::new(frame_raw(1_u32 << 31, 1_u32 << 31, Vec::new()));

        let (result, _) = with_callback_info(|info| {
            catch_unwind(AssertUnwindSafe(|| {
                video_writeback(data.clone(), huge.clone(), info)
            }))
        });

        match result {
            Ok(update) => {
                assert_eq!(update, Update::DoNothing);
                assert_eq!(
                    read_state(&mut data).current_frame_id,
                    None,
                    "an overflowing frame must not become the displayed image"
                );
            }
            Err(_) => eprintln!(
                "NOTE: video_writeback panicked (usize overflow of width*height*4) for a \
                 2^31 x 2^31 frame - see the autotest report"
            ),
        }
    }

    // ==================================================================
    // merge_video_state
    // ==================================================================

    /// An `(old, new)` pair plus the seek channel `old` hands forward.
    fn merge_pair(
        old_cfg: VideoConfig,
        new_cfg: VideoConfig,
    ) -> (RefAny, RefAny, Receiver<ThreadSendMsg>) {
        let (tx, rx) = channel::<ThreadSendMsg>();
        let mut old = base_state(old_cfg);
        old.started = true;
        old.thread_id = Some(ThreadId::unique());
        old.seek_sender = Some(tx);
        (RefAny::new(base_state(new_cfg)), RefAny::new(old), rx)
    }

    #[test]
    fn merge_takes_the_live_state_from_old_and_the_config_from_new() {
        let log = frame_log(Update::DoNothing);
        let tid = ThreadId::unique();
        let (tx, _rx) = channel::<ThreadSendMsg>();

        let mut new = base_state(config(file_source("/new.mp4"), 3.0));
        new.on_frame = hook_into(&log);
        new.frames = OptionRefAny::Some(RefAny::new(vec![frame(1, 1)]));

        let mut old = base_state(config(file_source("/old.mp4"), 3.0));
        old.started = true;
        old.gl_texture_id = Some(9);
        old.frames = OptionRefAny::Some(RefAny::new(vec![frame(7, 7), frame(8, 8)]));
        old.decode_callback = Some(ThreadCallback::new(noop_decode_worker));
        old.current_frame = Some(placeholder_image(b"live"));
        old.thread_id = Some(tid);
        old.seek_sender = Some(tx);
        let old_frame_id = old.current_frame.as_ref().map(|i| i.id);

        let mut merged = merge_video_state(RefAny::new(new), RefAny::new(old));

        assert_same_config(
            &read_config(&mut merged),
            &config(file_source("/new.mp4"), 3.0),
        );
        let summary = read_state(&mut merged);
        assert!(summary.has_hook, "the fresh build's hook wins");
        assert!(summary.started, "'already running' must carry forward");
        assert_eq!(summary.gl_texture_id, Some(9));
        assert_eq!(summary.decode_cb, Some(noop_decode_worker as ThreadCallbackType as usize));
        assert_eq!(summary.current_frame_id, old_frame_id, "no visible flicker");
        assert_eq!(summary.thread_id, Some(tid));
        assert!(summary.has_seek_sender);
        assert_eq!(
            state_frames(&mut merged),
            Some(vec![(7, 7), (8, 8)]),
            "the OLD replay list wins - a fresh build cannot swap the clip"
        );
    }

    #[test]
    fn merge_leaves_the_new_state_alone_when_the_old_one_is_foreign() {
        let mut new = base_state(config(file_source("/new.mp4"), 1.0));
        new.frames = OptionRefAny::Some(RefAny::new(vec![frame(5, 5)]));
        let mut merged = merge_video_state(RefAny::new(new), RefAny::new(0_u32));

        let summary = read_state(&mut merged);
        assert!(!summary.started, "nothing to carry forward");
        assert_eq!(summary.thread_id, None);
        assert_eq!(
            state_frames(&mut merged),
            Some(vec![(5, 5)]),
            "with no old state the new build's own list survives"
        );
    }

    #[test]
    fn merge_returns_a_foreign_new_dataset_untouched() {
        let old = state(VideoConfig::default());
        let mut merged = merge_video_state(RefAny::new(77_u32), old);
        assert_eq!(
            merged.downcast_ref::<u32>().map(|v| *v),
            Some(77),
            "merge must hand back exactly the payload it was given"
        );
    }

    #[test]
    fn merge_of_a_dataset_with_itself_does_not_panic() {
        // The same RefAny on both sides: the mutable + shared borrows overlap,
        // so the merge is skipped rather than aliasing. Either way the state
        // must survive intact.
        let mut s = base_state(config(file_source("/self.mp4"), 4.0));
        s.started = true;
        s.gl_texture_id = Some(5);
        let mut data = RefAny::new(s);
        let before = read_state(&mut data);

        let mut merged = merge_video_state(data.clone(), data.clone());

        assert_eq!(read_state(&mut merged), before);
        assert_eq!(read_state(&mut data), before);
    }

    #[test]
    fn merge_pushes_a_seek_when_the_scrub_position_changed() {
        let (new, old, rx) = merge_pair(
            config(file_source("/clip.mp4"), 0.0),
            config(file_source("/clip.mp4"), 42.25),
        );
        let _merged = merge_video_state(new, old);

        let msgs: Vec<ThreadSendMsg> = rx.try_iter().collect();
        assert_eq!(msgs.len(), 1, "one seek, no source re-init");
        assert_eq!(
            custom_f32(&msgs[0]),
            Some(42.25),
            "the worker must be told the NEW timestamp"
        );
    }

    #[test]
    fn merge_stays_quiet_when_nothing_changed() {
        let (new, old, rx) = merge_pair(
            config(file_source("/clip.mp4"), 7.5),
            config(file_source("/clip.mp4"), 7.5),
        );
        let _merged = merge_video_state(new, old);
        assert!(
            rx.try_iter().next().is_none(),
            "an unchanged config must not wake the decode worker"
        );
    }

    #[test]
    fn merge_treats_negative_zero_and_zero_as_the_same_position() {
        let (new, old, rx) = merge_pair(
            config(file_source("/clip.mp4"), -0.0),
            config(file_source("/clip.mp4"), 0.0),
        );
        let _merged = merge_video_state(new, old);
        assert!(
            rx.try_iter().next().is_none(),
            "-0.0 == 0.0 is the same scrub position"
        );
    }

    #[test]
    fn merge_seeks_on_every_relayout_while_the_timestamp_is_nan() {
        // ADVERSARIAL: `NaN != NaN`, so an unchanged NaN scrub position looks
        // like a change on every single relayout and floods the worker with
        // seeks (see the report). Pinned here as the current behaviour.
        let (new, old, rx) = merge_pair(
            config(file_source("/clip.mp4"), f32::NAN),
            config(file_source("/clip.mp4"), f32::NAN),
        );
        let _merged = merge_video_state(new, old);

        let msgs: Vec<ThreadSendMsg> = rx.try_iter().collect();
        assert_eq!(msgs.len(), 1);
        assert!(
            custom_f32(&msgs[0]).is_some_and(f32::is_nan),
            "the spurious seek carries the NaN straight through to the worker"
        );
    }

    #[test]
    fn merge_pushes_the_new_source_when_the_input_changed() {
        let (new, old, rx) = merge_pair(
            config(file_source("/old.mp4"), 1.0),
            config(url_source("cdn.example", "/new.mp4"), 1.0),
        );
        let _merged = merge_video_state(new, old);

        let msgs: Vec<ThreadSendMsg> = rx.try_iter().collect();
        assert_eq!(msgs.len(), 1, "one re-init, no seek");
        assert_eq!(
            custom_source(&msgs[0]),
            Some(url_source("cdn.example", "/new.mp4"))
        );
    }

    #[test]
    fn merge_sends_the_seek_before_the_source_when_both_changed() {
        let (new, old, rx) = merge_pair(
            config(file_source("/old.mp4"), 0.0),
            config(bytes_source(vec![1, 2, 3]), 9.0),
        );
        let _merged = merge_video_state(new, old);

        let msgs: Vec<ThreadSendMsg> = rx.try_iter().collect();
        assert_eq!(msgs.len(), 2);
        assert_eq!(custom_f32(&msgs[0]), Some(9.0));
        assert_eq!(custom_source(&msgs[1]), Some(bytes_source(vec![1, 2, 3])));
    }

    #[test]
    fn merge_notices_a_source_change_that_only_differs_in_unicode() {
        let (new, old, rx) = merge_pair(
            config(file_source("/tmp/\u{1F3AC}.mp4"), 0.0),
            config(file_source("/tmp/\u{1F3AB}.mp4"), 0.0),
        );
        let _merged = merge_video_state(new, old);

        let msgs: Vec<ThreadSendMsg> = rx.try_iter().collect();
        assert_eq!(msgs.len(), 1, "distinct emoji are distinct sources");
        assert_eq!(
            custom_source(&msgs[0]),
            Some(file_source("/tmp/\u{1F3AB}.mp4"))
        );
    }

    #[test]
    fn merge_without_a_seek_sender_drops_the_seek_silently() {
        // Nothing to send to (replay / test-pattern mounts never record a
        // sender): the merge must still carry the state, not panic.
        let new = RefAny::new(base_state(config(file_source("/clip.mp4"), 5.0)));
        let mut old_state = base_state(config(file_source("/clip.mp4"), 0.0));
        old_state.started = true;
        let mut merged = merge_video_state(new, RefAny::new(old_state));

        let summary = read_state(&mut merged);
        assert!(summary.started);
        assert!(!summary.has_seek_sender);
        assert_eq!(read_config(&mut merged).timestamp, 5.0);
    }

    #[test]
    fn merge_survives_a_worker_whose_channel_is_already_closed() {
        let (new, old, rx) = merge_pair(
            config(file_source("/a.mp4"), 0.0),
            config(file_source("/b.mp4"), 1.0),
        );
        drop(rx); // the worker exited and its receiver is gone

        let mut merged = merge_video_state(new, old);

        let summary = read_state(&mut merged);
        assert!(
            summary.has_seek_sender,
            "a dead sender is still carried forward - the send just fails"
        );
        assert!(summary.started);
    }

    #[test]
    fn merge_is_idempotent_across_repeated_relayouts() {
        let (tx, rx) = channel::<ThreadSendMsg>();
        let tid = ThreadId::unique();
        let mut live = base_state(config(file_source("/clip.mp4"), 2.0));
        live.started = true;
        live.gl_texture_id = Some(3);
        live.thread_id = Some(tid);
        live.seek_sender = Some(tx);
        live.current_frame = Some(placeholder_image(b"live"));
        let mut carried = RefAny::new(live);

        for _ in 0..5 {
            let fresh = RefAny::new(base_state(config(file_source("/clip.mp4"), 2.0)));
            carried = merge_video_state(fresh, carried);
        }

        let summary = read_state(&mut carried);
        assert!(summary.started);
        assert_eq!(summary.gl_texture_id, Some(3));
        assert_eq!(summary.thread_id, Some(tid));
        assert!(summary.current_frame_id.is_some(), "the picture never blanks");
        assert!(
            rx.try_iter().next().is_none(),
            "a stable config must never seek, however many relayouts happen"
        );
    }
}
