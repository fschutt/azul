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

use azul_core::{
    callbacks::{Update, VirtualViewCallbackInfo, VirtualViewReturn},
    dom::{ComponentEventFilter, DatasetMergeCallbackType, Dom, EventFilter, OptionDom},
    geom::LogicalPosition,
    refany::{OptionRefAny, RefAny},
    resources::{ImageRef, RawImage, RawImageData, RawImageFormat},
    task::{ThreadId, ThreadReceiver, ThreadSendMsg},
    video::{VideoConfig, VideoFrame, VideoPhase, VideoStatus},
};

use super::capture_common::{
    invoke_on_frame, terminate_requested, OnVideoFrame, OnVideoFrameCallback, OptionOnVideoFrame,
};

use crate::{
    callbacks::{Callback, CallbackInfo, CallbackType},
    http::{HttpClient, OptionHttpClient},
    thread::{
        OptionThreadPool, Thread, ThreadCallback, ThreadPool, ThreadReceiveMsg, ThreadSender,
        ThreadWriteBackMsg, WriteBackCallback,
    },
    widgets::themes::{style_kit, OptionUiTheme, UiTheme},
};

/// Default decode size for the test pattern (the real decoder reports the
/// stream's actual size).
const DEFAULT_W: u32 = 1280;
const DEFAULT_H: u32 = 720;

/// How much media time passes between two position reports while a video
/// plays: the web's `timeupdate` budget, the same one the media-player
/// manager throttles `TimeUpdate` to. A progress bar needs no more, and a
/// report per frame would rebuild the app's UI at the frame rate.
pub const STATUS_INTERVAL_S: f32 = crate::managers::media_player::TIME_UPDATE_INTERVAL_S;

/// Slack when matching the clock to a frame's presentation time, so a clock
/// that lands a hair before a frame (`n / fps` rounds) still shows it.
const PTS_TOLERANCE_S: f32 = 0.001;
/// How far ahead of its clock the decode worker keeps frames while playing.
pub const DECODE_LOOKAHEAD_S: f32 = 2.0;
/// How far behind its clock the worker keeps frames (a short seek back
/// replays them without a decode).
pub const DECODE_KEEP_BEHIND_S: f32 = 0.5;
/// A seek this far past the decoded frames restarts the decode at the
/// target's keyframe instead of decoding every frame up to it.
pub const DECODE_JUMP_S: f32 = 3.0;
/// How far (in frames) the clock may run past the newest decoded frame
/// before playback STALLS: one late frame is a hitch, not a wait.
pub const STALL_SLACK_FRAMES: f32 = 2.0;
/// How long a stall lasts before the app is told the video is loading
/// (buffering) rather than playing.
pub const STALL_REPORT_S: f64 = 0.3;
/// How much is decoded past the frame a stall waits on before playback goes
/// on: a download that delivers a frame at a time would otherwise play a
/// frame, wait, play a frame.
pub const RESUME_LEAD_S: f32 = 0.5;

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
    /// Optional user hook invoked with every [`VideoStatus`] the decode
    /// worker reports. Re-set on every fresh build (see [`merge_video_state`]).
    pub on_status: OptionOnVideoStatus,
    /// The status the decode worker last reported ([`VideoStatus::loading`]
    /// until it reports one). Written by [`video_status_writeback`], never by
    /// a rebuild.
    pub status: VideoStatus,
    /// Optional pre-decoded frames to replay (a `RefAny` holding a
    /// `Vec<VideoFrame>`); when set, the replay worker cycles these instead of
    /// the built-in test pattern. Carried forward by [`merge_video_state`].
    pub frames: OptionRefAny,
    /// The off-main-thread streaming decode worker (mirrors the map widget's
    /// `fetch_callback`). Installed on mount from [`register_video_decoder`]
    /// unless the widget replays [`VideoWidget::with_frames`]. When present,
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
    /// The user's `on_mount` hook, copied from the builder so mount can fire
    /// it. Carried across relayout.
    pub on_mount: OptionVideoMount,
    /// What the decode worker runs on, as the `on_mount` hook last returned it.
    /// Written on mount, never by a rebuild (see [`merge_video_state`]).
    pub setup: VideoSetup,
    /// The theme the widget was built in: the render callback draws the
    /// "no signal" poster in it. Adopted from every rebuild (see
    /// [`merge_video_state`]).
    pub theme: UiTheme,
    /// Whether the widget was built UNPINNED, following the app theme: the
    /// poster then carries every theme's block (`@theme(..)`) and `theme`
    /// is only the structure it was built in. Adopted from every rebuild.
    pub follows_app_theme: bool,
}

/// The runtime-installed streaming decode worker every video picks up when it
/// mounts. Registered once by the dll, where the worker and its decoder
/// dependencies live.
static VIDEO_DECODER: std::sync::OnceLock<ThreadCallback> = std::sync::OnceLock::new();

/// Install the framework-owned streaming decode worker. The first registration
/// wins; returns `false` when one was already installed.
pub fn register_video_decoder(cb: ThreadCallback) -> bool {
    VIDEO_DECODER.set(cb).is_ok()
}

/// Whether a streaming decode worker has been installed.
#[must_use]
pub fn has_video_decoder() -> bool {
    VIDEO_DECODER.get().is_some()
}

/// The decode worker's thread-init: what to play, and the connection pool to
/// download it through.
#[derive(Debug, Clone)]
pub struct VideoDecodeInit {
    pub config: VideoConfig,
    /// From the widget's [`VideoSetup`]. `None` opens a connection of its own.
    pub client: OptionHttpClient,
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
    /// Optional hook fired when the widget is mounted, returning the
    /// [`VideoSetup`] its decode worker runs on.
    pub on_mount: OptionVideoMount,
    /// Optional hook fired with every [`VideoStatus`] the decoder reports.
    pub on_status: OptionOnVideoStatus,
    /// The widget theme, or `None` to follow the app theme (`AppConfig::with_theme`). The
    /// picture is the source's own; the theme draws the widget's chrome - the
    /// "no signal" poster shown until the first frame. A DOM-level choice:
    /// switching it rebuilds the widget.
    pub theme: OptionUiTheme,
}

impl VideoWidget {
    /// Create a video widget for the given config.
    #[must_use]
    pub const fn create(config: VideoConfig) -> Self {
        Self {
            config,
            on_frame: OptionOnVideoFrame::None,
            frames: OptionRefAny::None,
            on_mount: OptionVideoMount::None,
            on_status: OptionOnVideoStatus::None,
            theme: OptionUiTheme::None,
        }
    }

    /// Pick the widget theme. Unset (`None`), the widget follows the
    /// app theme (`AppConfig::with_theme`, flat by default).
    #[inline]
    pub const fn set_theme(&mut self, theme: UiTheme) {
        self.theme = OptionUiTheme::Some(theme);
    }

    /// [`Self::set_theme`] for the builder chain.
    #[inline]
    #[must_use]
    pub const fn with_theme(mut self, theme: UiTheme) -> Self {
        self.set_theme(theme);
        self
    }

    /// Set a hook fired whenever the video's pipeline reports a
    /// [`VideoStatus`]: loading, the first frame on screen (held as a poster
    /// or playing), the position about four times a second while it plays,
    /// the end, or a failure with a message for the user. This is where an
    /// app drives its play button, its time display and its error text.
    pub fn set_on_status<C: Into<OnVideoStatusCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_status = Some(OnVideoStatus {
            refany: data,
            callback: callback.into(),
        })
        .into();
    }

    /// Builder form of [`set_on_status`](Self::set_on_status).
    #[must_use]
    pub fn with_on_status<C: Into<OnVideoStatusCallback>>(
        mut self,
        data: RefAny,
        callback: C,
    ) -> Self {
        self.set_on_status(data, callback);
        self
    }

    /// Set a hook fired when the widget is mounted. It receives the widget's
    /// current [`VideoSetup`] and returns the one to use: this is where an app
    /// hands the decoder a shared `HttpClient` or `ThreadPool`. Without a hook
    /// the decoder downloads over its own connection on its own thread.
    ///
    /// The decoder runs for as long as the video plays, so a pool keeps one
    /// worker busy per playing video.
    pub fn set_on_mount<C: Into<VideoMountCallback>>(&mut self, data: RefAny, callback: C) {
        self.on_mount = Some(VideoMount {
            refany: data,
            callback: callback.into(),
        })
        .into();
    }

    /// Builder form of [`set_on_mount`](Self::set_on_mount).
    #[must_use]
    pub fn with_on_mount<C: Into<VideoMountCallback>>(mut self, data: RefAny, callback: C) -> Self {
        self.set_on_mount(data, callback);
        self
    }

    /// Set a hook invoked with every decoded frame - for live effects, saving
    /// frames into your data model, or sending them over the network
    /// (azul-meet).
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
    #[must_use]
    pub fn with_frames(mut self, frames: RefAny) -> Self {
        self.frames = Some(frames).into();
        self
    }

    /// Build the widget's DOM: an `<img>` a background thread keeps fed.
    ///
    /// Replays pre-decoded [`with_frames`](Self::with_frames) if given. Otherwise
    /// the widget streams `config.source` through the decode worker the runtime
    /// registered ([`register_video_decoder`]), installed when the widget mounts;
    /// without one it shows the built-in test pattern. Pools for the decoder come
    /// from the `on_mount` hook, never from here: building the `Dom` only
    /// describes the UI.
    ///
    /// Rendering goes through the theme modules (as `Button::dom` does).
    /// Unpinned (`theme: None`), the widget follows the APP theme: built in
    /// the structure (and marker) of the theme its DOM is built for, its
    /// poster carrying every theme's block.
    #[must_use]
    pub fn dom(self) -> Dom {
        match self.theme.into_option() {
            Some(UiTheme::Flora) => crate::widgets::themes::flora::video(self),
            Some(UiTheme::Flat) => crate::widgets::themes::flat::video(self),
            None => self.build_in(UiTheme::current(), true),
        }
    }

    /// Builds the widget in `theme` - what `themes::flat::video` /
    /// `themes::flora::video` call. The theme travels in the widget state, so
    /// the render callback draws the poster in it.
    #[must_use]
    pub(crate) fn build(self, theme: UiTheme) -> Dom {
        self.build_in(theme, false)
    }

    /// [`Self::build`] in the structure of `theme`; `follows_app_theme`: the
    /// poster carries every theme's block rather than `theme`'s alone.
    fn build_in(self, theme: UiTheme, follows_app_theme: bool) -> Dom {
        let state = VideoWidgetState {
            config: self.config,
            started: false,
            gl_texture_id: None,
            on_frame: self.on_frame,
            on_status: self.on_status,
            status: VideoStatus::loading(),
            frames: self.frames,
            decode_callback: None,
            current_frame: None,
            thread_id: None,
            seek_sender: None,
            on_mount: self.on_mount,
            setup: VideoSetup::new(),
            theme,
            follows_app_theme,
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
            .with_ids_and_classes(azul_core::dom::IdOrClassVec::from_vec(alloc::vec![
                style_kit::marker(theme)
            ]))
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
}

/// The "no signal" poster in `theme` - the video widget's only chrome.
fn poster_style(theme: UiTheme) -> azul_css::dynamic_selector::CssPropertyWithConditionsVec {
    match theme {
        UiTheme::Flat => crate::widgets::themes::flat::video_poster_style(),
        UiTheme::Flora => crate::widgets::themes::flora::video_poster_style(),
    }
}

/// The poster of a widget state: its theme's, or - built unpinned - every
/// theme's block of it (`themes::theme_blocks::follow_props`).
fn state_poster_style(
    s: &VideoWidgetState,
) -> azul_css::dynamic_selector::CssPropertyWithConditionsVec {
    if s.follows_app_theme {
        crate::widgets::themes::theme_blocks::follow_props(
            poster_style(UiTheme::Flat).as_slice(),
            poster_style(UiTheme::Flora).as_slice(),
        )
    } else {
        poster_style(s.theme)
    }
}

/// The `<img>` one decoded frame renders as, filling the widget.
///
/// Marked DECORATIVE (`role: Nothing`, no name): the frame is the picture
/// behind the player, and the player's accessible surface is its controls,
/// which the app names ("Play video", "Seek"). An `<img>` with no
/// accessibility info at all is absent from the tree without saying so, and
/// a named one would be announced again on every decoded frame.
fn frame_image(img: ImageRef) -> Dom {
    Dom::create_image(img)
        .with_css("width: 100%; height: 100%;")
        .with_accessibility_info(azul_core::a11y::AccessibilityInfo {
            role: azul_core::a11y::AccessibilityRole::Nothing,
            ..Default::default()
        })
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
        data.downcast_ref::<VideoWidgetState>()
            .map_or(OptionDom::None, |s| {
                s.current_frame.as_ref().map_or_else(
                    || {
                        // Poster / "no signal" placeholder. Returning None here
                        // rendered NOTHING, so a decoder that never produced a
                        // frame (missing video-native feature, non-x86_64 target,
                        // Vulkan init failure, network stall) was
                        // indistinguishable from a black video — the shipped
                        // azul-video "black frame" bug. A dead pipeline must be
                        // visibly dead. Drawn in the widget's theme.
                        OptionDom::Some(Dom::create_div().with_css_props(state_poster_style(&s)))
                    },
                    |img| OptionDom::Some(frame_image(img.clone())),
                )
            })
    };
    VirtualViewReturn {
        dom,
        materialized: azul_core::geom::LogicalRect::new(LogicalPosition::zero(), bounds),
        virtual_rect: azul_core::geom::LogicalRect::new(LogicalPosition::zero(), bounds),
    }
}

// --- User hook: on_mount (backreference DI, FFI-exposed) ---

/// What a video's decode worker runs on. Handed to the `on_mount` hook, which
/// returns the setup the widget uses from then on.
///
/// The default shares nothing: the worker downloads over its own connection on
/// its own thread, which is also what a widget without an `on_mount` hook does.
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct VideoSetup {
    /// Connection pool for the download. `None`: a connection of its own.
    pub http_client: OptionHttpClient,
    /// Workers to run the decoder on. `None`: a thread of its own.
    pub thread_pool: OptionThreadPool,
}

impl Default for VideoSetup {
    fn default() -> Self {
        Self::new()
    }
}

impl azul_core::host_invoker::HostOut for VideoSetup {
    /// Shares nothing: owns no pool, no thread.
    fn unwritten() -> Self {
        Self::new()
    }
}

impl VideoSetup {
    /// A setup that shares nothing
    #[must_use]
    pub const fn new() -> Self {
        Self {
            http_client: OptionHttpClient::None,
            thread_pool: OptionThreadPool::None,
        }
    }

    /// Download the video through a shared connection pool
    #[must_use]
    pub fn with_http_client(mut self, client: HttpClient) -> Self {
        self.http_client = OptionHttpClient::Some(client);
        self
    }

    /// Run the decoder on a shared thread pool
    #[must_use]
    pub fn with_thread_pool(mut self, pool: ThreadPool) -> Self {
        self.thread_pool = OptionThreadPool::Some(pool);
        self
    }
}

/// User hook fired when the video widget is mounted: receives the widget's
/// current [`VideoSetup`] and returns the one its decode worker should use.
pub type VideoMountCallbackType = extern "C" fn(RefAny, CallbackInfo, VideoSetup) -> VideoSetup;
impl_widget_callback!(
    VideoMount,
    OptionVideoMount,
    VideoMountCallback,
    VideoMountCallbackType
);
azul_core::impl_managed_callback! {
    wrapper:        VideoMountCallback,
    info_ty:        CallbackInfo,
    return_ty:      VideoSetup,
    default_ret:    VideoSetup::new(),
    invoker_static: VIDEO_MOUNT_INVOKER,
    invoker_ty:     AzVideoMountCallbackInvoker,
    thunk_fn:       az_video_mount_callback_thunk,
    setter_fn:      AzApp_setVideoMountCallbackInvoker,
    from_handle_fn: AzVideoMountCallback_createFromHostHandle,
    from_handle_byref_fn: AzVideoMountCallback_createFromHostHandleByref,
    extra_args:     [ setup: VideoSetup ],
}

// --- User hook: on_status (backreference DI, FFI-exposed) ---

/// User hook fired with every [`VideoStatus`] the video's decoder reports.
/// Returns `Update` like any callback: `RefreshDom` to redraw the app's
/// controls from the new status.
pub type OnVideoStatusCallbackType = extern "C" fn(RefAny, CallbackInfo, VideoStatus) -> Update;
impl_widget_callback!(
    OnVideoStatus,
    OptionOnVideoStatus,
    OnVideoStatusCallback,
    OnVideoStatusCallbackType
);
azul_core::impl_managed_callback! {
    wrapper:        OnVideoStatusCallback,
    info_ty:        CallbackInfo,
    return_ty:      Update,
    default_ret:    Update::DoNothing,
    invoker_static: ON_VIDEO_STATUS_INVOKER,
    invoker_ty:     AzOnVideoStatusCallbackInvoker,
    thunk_fn:       az_on_video_status_callback_thunk,
    setter_fn:      AzApp_setOnVideoStatusCallbackInvoker,
    from_handle_fn: AzOnVideoStatusCallback_createFromHostHandle,
    from_handle_byref_fn: AzOnVideoStatusCallback_createFromHostHandleByref,
    extra_args:     [ status: VideoStatus ],
}

/// Everything a video needs before its worker starts, done once it is in the
/// tree rather than while `layout()` describes it: the registered decode worker
/// (unless the widget replays frames it was given), then whatever the app's
/// `on_mount` hook wants the worker to run on.
fn mount_video(data: &mut RefAny, info: &CallbackInfo) {
    let (hook, setup) = {
        let Some(mut s) = data.downcast_mut::<VideoWidgetState>() else {
            return;
        };
        let replays = matches!(s.frames, OptionRefAny::Some(_));
        if !s.started && !replays && s.decode_callback.is_none() {
            s.decode_callback = VIDEO_DECODER.get().cloned();
        }
        (s.on_mount.clone(), s.setup.clone())
    };
    let OptionVideoMount::Some(hook) = hook else {
        return;
    };
    // The state is released while the hook runs: it is app code and may reach
    // back into this widget.
    let setup = hook.callback.invoke(hook.refany, *info, setup);
    if let Some(mut s) = data.downcast_mut::<VideoWidgetState>() {
        s.setup = setup;
    }
}

/// `AfterMount`: start the background decode thread exactly once.
extern "C" fn video_on_after_mount(mut data: RefAny, mut info: CallbackInfo) -> Update {
    mount_video(&mut data, &info);
    // Mark started exactly once; pull out the streaming decode worker (if any),
    // its source, any pre-decoded replay frames, and what to run on.
    let (decode_cb, config, frames, setup) = {
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
        (
            s.decode_callback.clone(),
            s.config.clone(),
            frames,
            s.setup.clone(),
        )
    };
    let spawn = |init: RefAny, writeback: RefAny, cb: ThreadCallback| match &setup.thread_pool {
        OptionThreadPool::Some(pool) => pool.create_thread(init, writeback, cb),
        OptionThreadPool::None => Thread::create(init, writeback, cb),
    };
    // Priority: off-main streaming decode worker > replay pre-decoded frames >
    // built-in test pattern. All feed the same WriteBack -> video_writeback path.
    if let Some(cb) = decode_cb {
        // The worker matches on `config.source` (typed — no RefAny downcast),
        // reads `config.timestamp`, and downloads through `client`.
        let init = RefAny::new(VideoDecodeInit {
            config,
            client: setup.http_client.clone(),
        });
        let tid = ThreadId::unique();
        let thread = spawn(init, data.clone(), cb);
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
            spawn(
                frames,
                data.clone(),
                ThreadCallback::new(video_replay_worker),
            ),
        );
    } else {
        info.add_thread(
            ThreadId::unique(),
            spawn(
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
///
/// Stops when it is told to (`TerminateThread`: its `<video>` left the DOM or
/// the window closed) or when the main thread stops receiving.
#[allow(clippy::cast_possible_truncation)] // bounded layout/render numeric cast
extern "C" fn video_test_worker(_init: RefAny, mut sender: ThreadSender, mut recv: ThreadReceiver) {
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
        if terminate_requested(&mut recv) {
            break;
        }
        let shift = (tick as usize / 4) % 7;
        let mut bytes = Vec::with_capacity(w * h * 4);
        for _y in 0..h {
            for x in 0..w {
                let c = BARS[((x * 7 / w) + shift) % 7];
                bytes.extend_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
        let frame = VideoFrame::new(w as u32, h as u32, bytes.into());
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
/// non-empty `Vec<VideoFrame>` the worker just returns. Like the test
/// pattern, it stops when it is told to (`TerminateThread`).
extern "C" fn video_replay_worker(
    mut init: RefAny,
    mut sender: ThreadSender,
    mut recv: ThreadReceiver,
) {
    let frames: Vec<VideoFrame> = match init.downcast_ref::<Vec<VideoFrame>>() {
        Some(f) => f.clone(),
        None => return,
    };
    if frames.is_empty() {
        return;
    }
    let mut idx: usize = 0;
    loop {
        if terminate_requested(&mut recv) {
            break;
        }
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

#[must_use]
/// Writeback (main thread): store the decoded frame as the widget's
/// `current_frame` (a CPU `ImageRef`) and re-render the `VirtualView` in place so it
/// re-reads it - exactly like `map_tile_writeback`.
///
/// Renders on cpurender AND
/// webrender (no GL `present_frame`, no DOM rebuild).
pub extern "C" fn video_writeback(
    mut writeback_data: RefAny,
    mut frame_data: RefAny,
    mut info: CallbackInfo,
) -> Update {
    let hook = writeback_data
        .downcast_ref::<VideoWidgetState>()
        .map_or_else(|| OptionOnVideoFrame::None, |s| s.on_frame.clone());
    let mut user_update = Update::DoNothing;
    match frame_data.downcast_ref::<VideoFrame>() {
        Some(frame) => {
            // Guard against dimensions whose RGBA byte count overflows `usize`
            // before `ImageRef::new_rawimage` validates it against the buffer:
            // `width * height * 4` wraps in release (e.g. 2^31 x 2^31 -> 0) so an
            // empty buffer would spuriously "match" and store a bogus frame. A
            // `checked_mul` that overflows drops the frame — the hook is still
            // notified below, exactly as for a byte-count mismatch.
            // `expected_len` is `None` on overflow and for a format frames
            // do not use.
            let fits = frame.expected_len().is_some();
            if fits {
                // A decoded frame is opaque, for which straight ==
                // premultiplied: no per-pixel multiply. It is shown in the
                // format it was decoded in (NV12 / BGRA8 / RGBA8).
                if let Some(img) = ImageRef::new_rawimage(RawImage {
                    pixels: RawImageData::U8(frame.bytes.clone()),
                    width: frame.width as usize,
                    height: frame.height as usize,
                    premultiplied_alpha: true,
                    data_format: frame.format,
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

#[must_use]
/// Writeback (main thread): the decode worker reported a [`VideoStatus`].
///
/// Stores it as the widget's `status` and hands it to the app's `on_status`
/// hook, returning the hook's `Update` (`DoNothing` without a hook). A status
/// is not a frame: nothing is re-rendered here, the app redraws its own
/// controls if it asks to.
pub extern "C" fn video_status_writeback(
    mut writeback_data: RefAny,
    mut status_data: RefAny,
    info: CallbackInfo,
) -> Update {
    let Some(status) = status_data
        .downcast_ref::<VideoStatus>()
        .map(|s| (*s).clone())
    else {
        return Update::DoNothing;
    };
    let hook = match writeback_data.downcast_mut::<VideoWidgetState>() {
        Some(mut s) => {
            s.status = status.clone();
            s.on_status.clone()
        }
        None => OptionOnVideoStatus::None,
    };
    match hook {
        OptionOnVideoStatus::Some(h) => h.callback.invoke(h.refany, info, status),
        OptionOnVideoStatus::None => Update::DoNothing,
    }
}

/// Carry live state forward across relayout.
#[allow(clippy::float_cmp)] // intentional exact compare: change-detection / identity fast-path /
                            // cache-key match
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
                    drop(snd.send(ThreadSendMsg::Custom(RefAny::new(
                        new_g.config.source.clone(),
                    ))));
                }
            }
            // Transport: a flipped `paused` holds or resumes the running
            // worker, the way a changed timestamp seeks it.
            if old_g.config.paused != new_g.config.paused {
                if let Some(snd) = old_g.seek_sender.as_ref() {
                    let transport = if new_g.config.paused {
                        VideoTransport::Pause
                    } else {
                        VideoTransport::Resume
                    };
                    drop(snd.send(ThreadSendMsg::Custom(RefAny::new(transport))));
                }
            }
            // Adopt the app-driven config; keep every worker-facing field
            // (frames, current_frame, thread_id, seek_sender, started, the
            // reported status) in the allocation the worker actually writes to.
            old_g.config = new_g.config.clone();
            old_g.on_frame = new_g.on_frame.clone();
            old_g.on_status = new_g.on_status.clone();
            // The hook is adopted; `setup` and the decode worker were installed on
            // mount and belong to the running widget, so they stay.
            old_g.on_mount = new_g.on_mount.clone();
            // The app's theme is adopted like its config: the poster follows it.
            old_g.theme = new_g.theme;
            old_g.follows_app_theme = new_g.follows_app_theme;
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
// Transport: what the decode worker plays when, testable without a decoder
// ============================================================================

/// Main → decode-worker transport message, sent by [`merge_video_state`] as
/// `ThreadSendMsg::Custom(RefAny::new(VideoTransport::..))` when the app
/// flips [`VideoConfig::paused`], the way a changed `timestamp` is sent as an
/// `f32` seek.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum VideoTransport {
    /// Hold on the frame on screen.
    Pause,
    /// Play on from the frame on screen; from the start once the video ended.
    Resume,
}

/// Where the media clock of a [`VideoPlayback`] stands.
#[derive(Debug, Copy, Clone, PartialEq)]
enum PlaybackClock {
    /// Stopped at `base_s`: the poster, a pause, or no frame yet.
    Held,
    /// Running since `since_s` (on the caller's clock) from `base_s`.
    Running { since_s: f64 },
    /// Playing, but WAITING at `base_s` since `since_s` for frames that are
    /// not decoded yet (a download slower than the video, a decoder catching
    /// up after a resume): the clock stands until [`RESUME_LEAD_S`] is
    /// decoded past it.
    Stalled { since_s: f64 },
    /// Stopped at the end of a video that does not loop.
    Ended,
}

/// The decode worker's transport and presentation schedule: which decoded
/// frame is due, and what to tell the app.
///
/// Kept apart from the decoder so its rules are testable without one: the
/// caller hands in the time (`now_s`, seconds on any monotonic clock) instead
/// of the schedule reading a clock itself. Frames may arrive in any order —
/// `VideoToolbox` hands them back in DECODE order — and are kept in
/// PRESENTATION order. The clock starts with the first frame on screen, not
/// with the download, so a slow network never eats the start of the video;
/// and it never runs ahead of the frames: a playing video whose next frames
/// are not decoded yet STALLS (reported as `Loading` once the wait lasts
/// [`STALL_REPORT_S`]) and plays on once [`RESUME_LEAD_S`] is decoded.
#[derive(Debug)]
pub struct VideoPlayback {
    /// Decoded frames, sorted by presentation time: `(pts_s, frame)`.
    frames: Vec<(f32, VideoFrame)>,
    /// Length in seconds, `0.0` while unknown.
    duration_s: f32,
    /// Media position when the clock was last stopped or (re)started.
    base_s: f32,
    clock: PlaybackClock,
    /// Index of the frame the last [`tick`](Self::tick) handed out; `None`
    /// makes the next one hand out whichever frame is due.
    presented: Option<usize>,
    /// The status the last [`tick`](Self::tick) handed out; `None` makes the
    /// next one report whatever the state is.
    reported: Option<VideoStatus>,
    /// Every frame of the stream is in `frames`: the last one is the end.
    complete: bool,
    looping: bool,
    /// The app asked to hold.
    paused: bool,
    /// One frame's time, from the worker ([`set_frame_interval`](Self::set_frame_interval));
    /// 1/30 s until then. Says whether the frame due at a position is held.
    frame_s: f32,
    /// Held, how much is kept decoded past the poster ([`set_preroll`](Self::set_preroll)).
    preroll_s: f32,
}

/// What one [`VideoPlayback::tick`] asks of the decode worker.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VideoTick {
    /// Present the frame at this index of [`VideoPlayback::frame`].
    pub present: Option<usize>,
    /// Tell the app, through the widget's `on_status` hook.
    pub status: Option<VideoStatus>,
}

/// A position a caller handed in, made usable: NaN, infinities and negative
/// times all mean the start.
const fn sanitize_position(position_s: f32) -> f32 {
    if position_s.is_finite() && position_s > 0.0 {
        position_s
    } else {
        0.0
    }
}

impl VideoPlayback {
    /// A schedule that starts at `start_s`. `paused` holds the first frame as
    /// a poster until [`resume`](Self::resume); `looping` wraps the end back
    /// to the start.
    #[must_use]
    pub const fn new(start_s: f32, paused: bool, looping: bool) -> Self {
        Self {
            frames: Vec::new(),
            duration_s: 0.0,
            base_s: sanitize_position(start_s),
            clock: PlaybackClock::Held,
            presented: None,
            reported: None,
            complete: false,
            looping,
            paused,
            frame_s: 1.0 / 30.0,
            preroll_s: 0.0,
        }
    }

    /// The video is `duration_s` long (`0.0`, NaN or negative: unknown).
    pub const fn set_duration(&mut self, duration_s: f32) {
        self.duration_s = sanitize_position(duration_s);
    }

    /// Held, keep `seconds` decoded past the poster, so a resume plays at
    /// once: for an app that holds a video only to start it in step with
    /// something else (its sound). A poster nobody may ever play keeps none.
    pub const fn set_preroll(&mut self, seconds: f32) {
        self.preroll_s = sanitize_position(seconds);
    }

    /// The newest presentation time decoded, if any frame is.
    fn newest(&self) -> Option<f32> {
        self.frames.last().map(|(pts, _)| *pts)
    }

    /// Whether playback can run from `position_s` without waiting: the frame
    /// due there is held, and every frame is in or [`RESUME_LEAD_S`] is
    /// decoded past it.
    fn has_lead(&self, position_s: f32) -> bool {
        self.holds_frame_at(position_s)
            && (self.complete
                || self.newest().is_some_and(|newest| {
                    newest + PTS_TOLERANCE_S >= position_s + RESUME_LEAD_S
                }))
    }

    /// One frame lasts `frame_s` (ignored unless positive and finite).
    pub const fn set_frame_interval(&mut self, frame_s: f32) {
        if frame_s > 0.0 && frame_s.is_finite() {
            self.frame_s = frame_s;
        }
    }

    /// The frames held, in presentation order.
    #[must_use]
    pub const fn frames_held(&self) -> usize {
        self.frames.len()
    }

    /// The frame on screen, if a tick presented one.
    #[must_use]
    pub fn on_screen(&self) -> Option<&VideoFrame> {
        self.presented.and_then(|i| self.frame(i))
    }

    /// Whether the frame due at `position_s` is held: the first held frame
    /// is at or before it (within one frame).
    fn holds_frame_at(&self, position_s: f32) -> bool {
        self.frames
            .first()
            .is_some_and(|(first, _)| *first <= position_s + self.frame_s + PTS_TOLERANCE_S)
    }

    /// Whether the worker should decode another access unit now.
    /// `decoded_until_s` is the presentation time of the newest frame it
    /// decoded since it last (re)started, `None` before the first. Held, the
    /// schedule wants the poster - the frame at its position - and nothing
    /// after it (but its [`preroll`](Self::set_preroll)); playing or waiting
    /// on frames, [`DECODE_LOOKAHEAD_S`] ahead of the clock; ended, or with
    /// every frame in, nothing. This is what keeps a `<video>` nobody plays
    /// from holding its whole clip decoded.
    #[must_use]
    pub fn wants_frame(&self, now_s: f64, decoded_until_s: Option<f32>) -> bool {
        if self.complete {
            return false;
        }
        let Some(decoded_until) = decoded_until_s else {
            return true;
        };
        match self.clock {
            PlaybackClock::Held => self.base_s + self.preroll_s > decoded_until + PTS_TOLERANCE_S,
            PlaybackClock::Running { .. } | PlaybackClock::Stalled { .. } => {
                decoded_until < self.position(now_s) + DECODE_LOOKAHEAD_S
            }
            PlaybackClock::Ended => false,
        }
    }

    /// The position the worker must restart decoding for, from the keyframe
    /// at or before it: the frame due there is not held (a seek back past the
    /// kept frames, a loop wrap), or it lies more than [`DECODE_JUMP_S`] past
    /// what the worker decoded (a seek far ahead). `None` otherwise, and
    /// before the first frame (the worker is decoding from the start).
    #[must_use]
    pub fn restart_wanted(&self, now_s: f64, decoded_until_s: Option<f32>) -> Option<f32> {
        if self.frames.is_empty() || self.clock == PlaybackClock::Ended {
            return None;
        }
        let position = self.position(now_s);
        if !self.holds_frame_at(position) {
            return Some(position);
        }
        match decoded_until_s {
            Some(until) if !self.complete && position > until + DECODE_JUMP_S => Some(position),
            _ => None,
        }
    }

    /// The worker restarted its decode (a seek past the frames held, a loop
    /// wrap): the end is not in until it is handed in again, and the old
    /// run's frames go - kept, frames AHEAD of the clock from the old run
    /// would hide that the new run has not decoded the frames the clock is
    /// at (the picture on screen stays until the next one is presented).
    pub fn restart_decode(&mut self) {
        self.complete = false;
        self.frames.clear();
        self.presented = None;
    }

    /// Drop the frames more than [`DECODE_KEEP_BEHIND_S`] behind the clock,
    /// never the one on screen; how many went.
    pub fn trim(&mut self, now_s: f64) -> usize {
        let keep_from = self.position(now_s) - DECODE_KEEP_BEHIND_S;
        let mut n = self.frames.partition_point(|(pts, _)| *pts < keep_from);
        if let Some(on_screen) = self.presented {
            n = n.min(on_screen);
        }
        if n == 0 {
            return 0;
        }
        self.frames.drain(..n);
        self.presented = self.presented.map(|i| i - n);
        n
    }

    /// The length in seconds, `0.0` while unknown.
    #[must_use]
    pub const fn duration(&self) -> f32 {
        self.duration_s
    }

    /// Every frame has been handed in: the last one is the end of the video.
    pub const fn finish(&mut self) {
        self.complete = true;
    }

    /// The decoded frame at `index`, in presentation order.
    #[must_use]
    pub fn frame(&self, index: usize) -> Option<&VideoFrame> {
        self.frames.get(index).map(|(_, frame)| frame)
    }

    /// Present the due frame again on the next tick (the output size
    /// changed).
    pub const fn invalidate(&mut self) {
        self.presented = None;
    }

    /// Where the media clock stands at `now_s`.
    #[must_use]
    pub const fn position(&self, now_s: f64) -> f32 {
        match self.clock {
            PlaybackClock::Running { since_s } => {
                let elapsed = now_s - since_s;
                let elapsed = if elapsed > 0.0 { elapsed } else { 0.0 };
                self.base_s + elapsed as f32
            }
            PlaybackClock::Held | PlaybackClock::Stalled { .. } | PlaybackClock::Ended => {
                self.base_s
            }
        }
    }

    /// Stop the clock where it stands at `now_s`, and restart it from there.
    const fn rebase(&mut self, position_s: f32, now_s: f64) {
        self.base_s = position_s;
        self.clock = PlaybackClock::Running { since_s: now_s };
    }

    /// `position_s` clamped to the end of the video, when the end is known.
    const fn clamp_to_end(&self, position_s: f32) -> f32 {
        if self.duration_s > 0.0 && position_s > self.duration_s {
            self.duration_s
        } else {
            position_s
        }
    }

    /// Add a decoded frame, shown from `pts_s` on. Frames may arrive in any
    /// order; the frame on screen stays the one on screen.
    pub fn push_frame(&mut self, pts_s: f32, frame: VideoFrame) {
        let pts_s = sanitize_position(pts_s);
        let at = self
            .frames
            .partition_point(|(pts, _)| *pts < pts_s - PTS_TOLERANCE_S);
        if let Some(slot) = self.frames.get_mut(at) {
            if (slot.0 - pts_s).abs() <= PTS_TOLERANCE_S {
                // Decoded again (a restart from a keyframe): the newer copy.
                slot.1 = frame;
                return;
            }
        }
        self.frames.insert(at, (pts_s, frame));
        if let Some(on_screen) = self.presented {
            if at <= on_screen {
                self.presented = Some(on_screen + 1);
            }
        }
    }

    /// Hold on the frame on screen.
    pub fn pause(&mut self, now_s: f64) {
        if let PlaybackClock::Running { .. } | PlaybackClock::Stalled { .. } = self.clock {
            self.base_s = self.clamp_to_end(self.position(now_s));
            self.clock = PlaybackClock::Held;
        }
        self.paused = true;
        self.reported = None;
    }

    /// Play on from the frame on screen, or from the start once the video
    /// ended. Before the first frame this only arms playback: the clock
    /// starts with the first frame. Without [`RESUME_LEAD_S`] decoded past
    /// the frame on screen, playback waits for it (stalled) rather than
    /// running ahead of the decoder.
    pub fn resume(&mut self, now_s: f64) {
        if self.clock == PlaybackClock::Ended {
            self.base_s = 0.0;
            self.clock = PlaybackClock::Held;
        }
        self.paused = false;
        if self.clock == PlaybackClock::Held && !self.frames.is_empty() {
            self.clock = if self.has_lead(self.base_s) {
                PlaybackClock::Running { since_s: now_s }
            } else {
                PlaybackClock::Stalled { since_s: now_s }
            };
        }
        self.reported = None;
    }

    /// Jump to `position_s`, clamped into the video; a held video stays held,
    /// a playing one plays on from there.
    pub fn seek(&mut self, position_s: f32, now_s: f64) {
        let target = self.clamp_to_end(sanitize_position(position_s));
        self.base_s = target;
        match self.clock {
            PlaybackClock::Running { .. } | PlaybackClock::Stalled { .. } => {
                self.clock = PlaybackClock::Running { since_s: now_s };
            }
            PlaybackClock::Ended => {
                if self.duration_s <= 0.0 || target < self.duration_s {
                    self.clock = PlaybackClock::Held;
                }
            }
            PlaybackClock::Held => {}
        }
        self.reported = None;
    }

    /// Advance to `now_s`: which frame is due, and what the app should hear.
    #[must_use]
    pub fn tick(&mut self, now_s: f64) -> VideoTick {
        let Some(newest) = self.newest() else {
            // Nothing decoded yet (or since a restart): a playing clock waits
            // where it stands.
            let phase = match self.clock {
                PlaybackClock::Running { .. } => {
                    self.base_s = self.position(now_s);
                    self.clock = PlaybackClock::Stalled { since_s: now_s };
                    VideoPhase::Playing
                }
                PlaybackClock::Stalled { .. } => self.phase_at(now_s),
                PlaybackClock::Held | PlaybackClock::Ended => VideoPhase::Loading,
            };
            let status = self.report(phase, self.base_s);
            return VideoTick {
                present: None,
                status,
            };
        };
        // The clock starts with the first frame on screen, not with the
        // download.
        if !self.paused && self.clock == PlaybackClock::Held {
            self.clock = PlaybackClock::Running { since_s: now_s };
        }
        // A wait for the decoder ends once a lead is decoded past the frame
        // it waits on.
        if let PlaybackClock::Stalled { .. } = self.clock {
            if self.has_lead(self.base_s) {
                self.clock = PlaybackClock::Running { since_s: now_s };
            }
        }
        let mut position = self.position(now_s);
        if let PlaybackClock::Running { .. } = self.clock {
            let end = if self.duration_s > 0.0 {
                self.duration_s
            } else {
                newest
            };
            if !self.complete && position > newest + STALL_SLACK_FRAMES * self.frame_s {
                // Playback outran the decoder (the download): wait on the
                // newest frame - or, after a seek past it, at the target.
                position = newest.max(self.base_s);
                self.base_s = position;
                self.clock = PlaybackClock::Stalled { since_s: now_s };
            } else if self.complete && position >= end {
                if self.looping && end > 0.0 {
                    position %= end;
                    self.rebase(position, now_s);
                    // The position jumped back: say so now.
                    self.reported = None;
                } else {
                    position = end;
                    self.base_s = end;
                    self.clock = PlaybackClock::Ended;
                    self.paused = true;
                }
            }
        }
        // The frame due is not held (a seek back past the kept frames, a
        // loop wrap): keep the picture - not the stale frame after it - while
        // the worker decodes from its keyframe again (`restart_wanted`).
        if self.clock != PlaybackClock::Ended && !self.holds_frame_at(position) {
            if let PlaybackClock::Running { .. } = self.clock {
                // Nothing to show at the clock: it waits for the frames there.
                self.base_s = position;
                self.clock = PlaybackClock::Stalled { since_s: now_s };
            }
            let phase = self.phase_at(now_s);
            let status = self.report(phase, position);
            return VideoTick {
                present: None,
                status,
            };
        }
        let index = if self.clock == PlaybackClock::Ended {
            self.frames.len() - 1
        } else {
            self.frames
                .partition_point(|(pts, _)| *pts <= position + PTS_TOLERANCE_S)
                .saturating_sub(1)
        };
        let present = if self.presented == Some(index) {
            None
        } else {
            self.presented = Some(index);
            Some(index)
        };
        let phase = self.phase_at(now_s);
        let status = self.report(phase, position);
        VideoTick { present, status }
    }

    /// What the app hears of the clock at `now_s`: a wait for the decoder is
    /// still `Playing` for a moment (a hitch), then `Loading` (buffering).
    fn phase_at(&self, now_s: f64) -> VideoPhase {
        match self.clock {
            PlaybackClock::Held => VideoPhase::Paused,
            PlaybackClock::Running { .. } => VideoPhase::Playing,
            PlaybackClock::Stalled { since_s } => {
                if now_s - since_s >= STALL_REPORT_S {
                    VideoPhase::Loading
                } else {
                    VideoPhase::Playing
                }
            }
            PlaybackClock::Ended => VideoPhase::Ended,
        }
    }

    /// The status to hand out, if the app has not heard it yet: a new phase,
    /// a new length, a seek or a transport change, or - while playing - a
    /// position [`STATUS_INTERVAL_S`] on from the last report.
    fn report(&mut self, phase: VideoPhase, position_s: f32) -> Option<VideoStatus> {
        let due = self.reported.as_ref().is_none_or(|last| {
            last.phase != phase
                || last.duration_s != self.duration_s
                || (phase == VideoPhase::Playing
                    && (position_s - last.position_s).abs() >= STATUS_INTERVAL_S)
        });
        if !due {
            return None;
        }
        let status = VideoStatus::create(phase, position_s, self.duration_s);
        self.reported = Some(status.clone());
        Some(status)
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
        window::{MonitorVec, RawWindowHandle, DarkLightMode},
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
            paused: false,
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
                file_source(
                    "/tmp/\u{1F3AC}-\u{5F71}\u{7247}-\u{0631}\u{0645}\u{0632}-e\u{0301}.mp4",
                ),
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
                paused: true,
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
        assert_eq!(actual.paused, expected.paused);
        assert_eq!(actual.output_format, expected.output_format);
    }

    const CONST_CONFIG: VideoConfig = VideoConfig {
        source: VideoSource::File(AzString::from_const_str("/tmp/const-clip.mp4")),
        timestamp: 2.5,
        autoplay: false,
        looping: true,
        paused: true,
        output_format: RawImageFormat::RGBA8,
    };

    /// Compile-time proof that `create` really is a `const fn` - the `const`
    /// qualifier is part of the public API, so a non-const `create` must break
    /// this file.
    const CONST_WIDGET: VideoWidget = VideoWidget::create(CONST_CONFIG);

    // ==================================================================
    // State fixtures
    // ==================================================================

    /// A freshly-built widget state (exactly what `dom` stores).
    fn base_state(config: VideoConfig) -> VideoWidgetState {
        VideoWidgetState {
            config,
            started: false,
            gl_texture_id: None,
            on_frame: OptionOnVideoFrame::None,
            on_status: OptionOnVideoStatus::None,
            status: VideoStatus::loading(),
            frames: OptionRefAny::None,
            decode_callback: None,
            current_frame: None,
            thread_id: None,
            seek_sender: None,
            on_mount: OptionVideoMount::None,
            setup: VideoSetup::new(),
            theme: crate::widgets::themes::UiTheme::Flat,
            follows_app_theme: false,
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
        VideoFrame::new(width, height, vec![7u8; px * 4].into())
    }

    /// A frame whose declared dimensions need NOT match its byte count.
    fn frame_raw(width: u32, height: u32, bytes: Vec<u8>) -> VideoFrame {
        VideoFrame::new(width, height, bytes.into())
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
            ctx: core::cell::RefCell::new(OptionRefAny::None),
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
    fn with_virtual_view_info<R>(
        w: f32,
        h: f32,
        f: impl FnOnce(VirtualViewCallbackInfo) -> R,
    ) -> R {
        let fonts = FcFontCache::default();
        let images = ImageCache::default();
        let size = LogicalSize::new(w, h);
        let info = VirtualViewCallbackInfo::new(
            VirtualViewCallbackReason::InitialRender,
            &fonts,
            &images,
            DarkLightMode::Light,
            azul_core::window::WindowFrame::Normal,
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
            || bytes
                .chunks_exact(row_len)
                .all(|row| row == &bytes[..row_len]);
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
        let cb: extern "C" fn(*const core::ffi::c_void) -> OptionThreadSendMsg = if terminate {
            recv_terminate
        } else {
            recv_nothing
        };
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
        let _gate = WORKER_GATE.lock().unwrap_or_else(PoisonError::into_inner);
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
            assert!(
                matches!(widget.on_status, OptionOnVideoStatus::None),
                "a fresh widget has no status hook"
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
            let widget = VideoWidget::create(cfg.clone()).with_on_frame(
                frame_log(Update::DoNothing),
                record_frame as OnVideoFrameCallbackType,
            );

            assert_same_config(&widget.config, &cfg);
            let OptionOnVideoFrame::Some(hook) = &widget.on_frame else {
                panic!("with_on_frame must install a hook");
            };
            assert_eq!(
                hook.callback.cb as usize, record_frame as OnVideoFrameCallbackType as usize,
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
            hook.callback.cb as usize, frame_do_nothing as OnVideoFrameCallbackType as usize,
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

        let foreign = VideoWidget::create(VideoConfig::default()).with_frames(RefAny::new(0_u32));
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
            .with_on_frame(
                frame_log(Update::DoNothing),
                record_frame as OnVideoFrameCallbackType,
            );
        let b = VideoWidget::create(config(file_source("/a.mp4"), 1.0))
            .with_on_frame(
                frame_log(Update::DoNothing),
                record_frame as OnVideoFrameCallbackType,
            )
            .with_frames(RefAny::new(vec![frame(3, 3)]));

        assert_same_config(&a.config, &b.config);
        assert_eq!(widget_frames(&a), widget_frames(&b));
        assert!(matches!(a.on_frame, OptionOnVideoFrame::Some(_)));
        assert!(matches!(b.on_frame, OptionOnVideoFrame::Some(_)));
    }

    // ==================================================================
    // VideoWidget::dom / mount
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
            .with_on_frame(
                frame_log(Update::DoNothing),
                record_frame as OnVideoFrameCallbackType,
            )
            .dom();

        let mut dataset = dom.root.get_dataset().cloned().expect("dataset");
        let summary = read_state(&mut dataset);
        assert!(summary.has_hook, "dom() must carry the user hook forward");
        assert!(summary.has_frames);
        assert_eq!(state_frames(&mut dataset), Some(vec![(6, 7)]));
    }

    #[test]
    fn mounting_installs_the_registered_decoder_and_building_the_dom_never_does() {
        // The decoder is a process-wide registration (first one wins), so the
        // test accepts whichever worker is installed.
        let _ = register_video_decoder(ThreadCallback::new(noop_decode_worker));
        let registered = VIDEO_DECODER.get().expect("registered").cb as usize;

        let dom = VideoWidget::create(VideoConfig::default()).dom();
        let mut dataset = dom.root.get_dataset().cloned().expect("dataset");
        assert_eq!(
            read_state(&mut dataset).decode_cb,
            None,
            "building the Dom only describes the UI"
        );

        with_callback_info(|info| mount_video(&mut dataset, &info));
        assert_eq!(read_state(&mut dataset).decode_cb, Some(registered));
        let setup = dataset
            .downcast_ref::<VideoWidgetState>()
            .map(|s| s.setup.clone());
        assert_eq!(
            setup,
            Some(VideoSetup::new()),
            "without a hook nothing is shared"
        );
    }

    #[test]
    fn a_widget_that_replays_frames_is_not_given_the_decoder() {
        let _ = register_video_decoder(ThreadCallback::new(noop_decode_worker));
        let dom = VideoWidget::create(VideoConfig::default())
            .with_frames(RefAny::new(vec![frame(2, 2)]))
            .dom();
        let mut dataset = dom.root.get_dataset().cloned().expect("dataset");

        with_callback_info(|info| mount_video(&mut dataset, &info));
        assert_eq!(
            read_state(&mut dataset).decode_cb,
            None,
            "with_frames asked for a replay, so the replay worker must run"
        );
    }

    extern "C" fn pooled_video_setup(_: RefAny, _: CallbackInfo, setup: VideoSetup) -> VideoSetup {
        setup.with_thread_pool(ThreadPool::create(1))
    }

    #[test]
    fn the_video_mount_hook_decides_the_setup_and_a_rebuild_keeps_it() {
        let build = || {
            VideoWidget::create(VideoConfig::default())
                .with_on_mount(
                    RefAny::new(()),
                    pooled_video_setup as VideoMountCallbackType,
                )
                .dom()
        };
        let mut mounted = build().root.get_dataset().cloned().expect("dataset");
        with_callback_info(|info| mount_video(&mut mounted, &info));
        let pool = match mounted
            .downcast_ref::<VideoWidgetState>()
            .map(|s| s.setup.thread_pool.clone())
        {
            Some(OptionThreadPool::Some(pool)) => pool,
            _ => panic!("the hook's pool must be installed"),
        };

        let rebuilt = build().root.get_dataset().cloned().expect("dataset");
        let mut kept = merge_video_state(rebuilt, mounted);
        let kept_pool = kept
            .downcast_ref::<VideoWidgetState>()
            .map(|s| s.setup.thread_pool.clone());
        assert_eq!(
            kept_pool,
            Some(OptionThreadPool::Some(pool)),
            "the same pool, not a new one"
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
            let ret =
                with_virtual_view_info(w, h, |info| video_widget_render(dataset.clone(), info));
            assert!(
                rendered_nothing(&ret),
                "bounds {w}x{h} must render nothing until layout settles - even with a frame ready"
            );
        }
    }

    #[test]
    fn render_with_a_wrong_typed_dataset_emits_no_dom() {
        let dataset = RefAny::new(0_u32);
        let ret = with_virtual_view_info(800.0, 600.0, |info| {
            video_widget_render(dataset.clone(), info)
        });
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
        let ret = with_virtual_view_info(800.0, 600.0, |info| {
            video_widget_render(dataset.clone(), info)
        });
        assert!(
            !rendered_nothing(&ret),
            "no decoded frame yet -> a visible no-signal poster, NOT an invisible tile"
        );
    }

    #[test]
    fn render_emits_the_stored_frame_as_an_image() {
        let img = placeholder_image(b"azul-video-frame");
        let expected_id = img.id;
        let mut s = base_state(VideoConfig::default());
        s.current_frame = Some(img);
        let dataset = RefAny::new(s);

        let ret = with_virtual_view_info(800.0, 600.0, |info| {
            video_widget_render(dataset.clone(), info)
        });
        assert_eq!(
            rendered_image_id(&ret),
            Some(expected_id),
            "the <img> must show exactly the frame the writeback stored"
        );
    }

    #[test]
    fn render_reports_the_bounds_back_as_the_scroll_size() {
        let dataset = state(VideoConfig::default());
        let ret = with_virtual_view_info(640.0, 480.0, |info| {
            video_widget_render(dataset.clone(), info)
        });

        assert_eq!(ret.materialized.size.width, 640.0);
        assert_eq!(ret.materialized.size.height, 480.0);
        assert_eq!(ret.virtual_rect.size.width, 640.0);
        assert_eq!(ret.virtual_rect.size.height, 480.0);
        assert_eq!(
            (ret.materialized.origin.x, ret.materialized.origin.y),
            (0.0, 0.0)
        );
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

        for (w, h) in [
            (f32::MIN_POSITIVE, f32::MIN_POSITIVE),
            (1.0, 1.0),
            (f32::MAX, f32::MAX),
        ] {
            let ret =
                with_virtual_view_info(w, h, |info| video_widget_render(dataset.clone(), info));
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

        assert_eq!(
            update,
            Update::DoNothing,
            "mounting never triggers relayout"
        );
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
        let (update, changes) =
            with_callback_info(|info| video_on_resize(RefAny::new(0_u32), info));
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

    /// THREADS8 (PR #476 engine backlog 4): a `<video>` that leaves the DOM
    /// has its worker told to stop (`managers::thread_owner`), and a window
    /// that closes tells every worker the same. The test-pattern worker never
    /// read its receiver: it ran on until the 2 s grace expired and was
    /// DETACHED ("did not acknowledge TerminateThread"), and every resize /
    /// per-frame `Tick` message piled up unread in its channel meanwhile.
    #[test]
    fn the_test_pattern_worker_stops_when_it_is_told_to_terminate() {
        let sent = run_worker(video_test_worker, RefAny::new(()), 3, true);

        assert!(
            sent.is_empty(),
            "a worker told to stop before its first frame sends nothing (sent {})",
            sent.len()
        );
    }

    #[test]
    fn test_worker_scrolls_the_pattern() {
        let sent = run_worker(video_test_worker, RefAny::new(()), 3, false);

        assert_eq!(sent.len(), 4, "3 accepted + 1 rejected");
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

    /// THREADS8: the replay worker, like the test pattern, ran on after its
    /// `<video>` left the DOM until the grace period detached it.
    #[test]
    fn the_replay_worker_stops_when_it_is_told_to_terminate() {
        let frames = vec![frame_raw(1, 1, vec![0; 4]), frame_raw(2, 2, vec![1; 16])];
        let sent = run_worker(video_replay_worker, RefAny::new(frames), 3, true);

        assert!(
            sent.is_empty(),
            "a worker told to stop before its first frame sends nothing (sent {})",
            sent.len()
        );
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
        assert_eq!(
            sent[2].small_bytes.as_deref(),
            Some(&[0xAB, 0xAB, 0xAB][..])
        );
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
                "NOTE: video_writeback panicked (usize overflow of width*height*4) for a 2^31 x \
                 2^31 frame - see the autotest report"
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
        assert_eq!(
            summary.decode_cb,
            Some(noop_decode_worker as ThreadCallbackType as usize)
        );
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
        assert!(
            summary.current_frame_id.is_some(),
            "the picture never blanks"
        );
        assert!(
            rx.try_iter().next().is_none(),
            "a stable config must never seek, however many relayouts happen"
        );
    }

    // ==================================================================
    // Transport + status: merge_video_state, video_status_writeback
    // ==================================================================

    fn custom_transport(msg: &ThreadSendMsg) -> Option<VideoTransport> {
        let ThreadSendMsg::Custom(r) = msg else {
            return None;
        };
        let mut r = r.clone();
        let out = r.downcast_ref::<VideoTransport>().map(|v| *v);
        out
    }

    fn held(cfg: VideoConfig, paused: bool) -> VideoConfig {
        VideoConfig { paused, ..cfg }
    }

    /// Records every status the widget's `on_status` hook is handed, and
    /// answers with a caller-chosen `Update`.
    struct StatusLog {
        seen: Vec<VideoStatus>,
        reply: Update,
    }

    extern "C" fn record_status(mut data: RefAny, _: CallbackInfo, status: VideoStatus) -> Update {
        let mut reply = Update::DoNothing;
        if let Some(mut log) = data.downcast_mut::<StatusLog>() {
            log.seen.push(status);
            reply = log.reply;
        }
        reply
    }

    fn status_log(reply: Update) -> RefAny {
        RefAny::new(StatusLog {
            seen: Vec::new(),
            reply,
        })
    }

    fn logged_statuses(data: &mut RefAny) -> Vec<VideoStatus> {
        data.downcast_ref::<StatusLog>()
            .expect("payload must still be a StatusLog")
            .seen
            .clone()
    }

    fn status_hook_into(log: &RefAny) -> OptionOnVideoStatus {
        Some(OnVideoStatus {
            refany: log.clone(),
            callback: (record_status as OnVideoStatusCallbackType).into(),
        })
        .into()
    }

    fn stored_status(data: &mut RefAny) -> Option<VideoStatus> {
        data.downcast_ref::<VideoWidgetState>()
            .map(|s| s.status.clone())
    }

    #[test]
    fn merge_pauses_and_resumes_the_worker_when_paused_flips() {
        let mp4 = || config(file_source("/clip.mp4"), 0.0);
        let (new, old, rx) = merge_pair(held(mp4(), false), held(mp4(), true));
        let merged = merge_video_state(new, old);

        let msgs: Vec<ThreadSendMsg> = rx.try_iter().collect();
        assert_eq!(msgs.len(), 1, "one transport message, no seek");
        assert_eq!(custom_transport(&msgs[0]), Some(VideoTransport::Pause));

        let fresh = RefAny::new(base_state(held(mp4(), false)));
        let _merged = merge_video_state(fresh, merged);
        let msgs: Vec<ThreadSendMsg> = rx.try_iter().collect();
        assert_eq!(msgs.len(), 1);
        assert_eq!(
            custom_transport(&msgs[0]),
            Some(VideoTransport::Resume),
            "clearing `paused` resumes the running worker"
        );
    }

    #[test]
    fn merge_stays_quiet_while_paused_is_unchanged() {
        let mp4 = || config(file_source("/clip.mp4"), 0.0);
        let (new, old, rx) = merge_pair(held(mp4(), true), held(mp4(), true));
        let _merged = merge_video_state(new, old);
        assert!(
            rx.try_iter().next().is_none(),
            "a held video that stays held must not wake the worker"
        );
    }

    #[test]
    fn merge_adopts_the_fresh_status_hook_and_keeps_the_reported_status() {
        let log = status_log(Update::DoNothing);
        let mut new = base_state(VideoConfig::default());
        new.on_status = status_hook_into(&log);
        let mut old = base_state(VideoConfig::default());
        old.status = VideoStatus::create(VideoPhase::Playing, 3.0, 10.0);

        let mut merged = merge_video_state(RefAny::new(new), RefAny::new(old));

        let hooked = merged
            .downcast_ref::<VideoWidgetState>()
            .is_some_and(|s| matches!(s.on_status, OptionOnVideoStatus::Some(_)));
        assert!(hooked, "the fresh build's hook wins");
        assert_eq!(
            stored_status(&mut merged),
            Some(VideoStatus::create(VideoPhase::Playing, 3.0, 10.0)),
            "a rebuild must not forget what the worker reported"
        );
    }

    #[test]
    fn a_fresh_widget_is_loading_and_carries_its_status_hook() {
        let dom = VideoWidget::create(VideoConfig::default())
            .with_on_status(
                status_log(Update::DoNothing),
                record_status as OnVideoStatusCallbackType,
            )
            .dom();
        let mut dataset = dom.root.get_dataset().cloned().expect("dataset");
        assert_eq!(stored_status(&mut dataset), Some(VideoStatus::loading()));
        let hooked = dataset
            .downcast_ref::<VideoWidgetState>()
            .is_some_and(|s| matches!(s.on_status, OptionOnVideoStatus::Some(_)));
        assert!(hooked, "dom() must carry the status hook into the state");
    }

    #[test]
    fn a_reported_status_is_stored_and_handed_to_the_hook() {
        let mut log = status_log(Update::RefreshDom);
        let mut s = base_state(VideoConfig::default());
        s.on_status = status_hook_into(&log);
        let mut data = RefAny::new(s);
        let poster = VideoStatus::create(VideoPhase::Paused, 0.0, 10.0);
        let payload = RefAny::new(poster.clone());

        let (update, changes) =
            with_callback_info(|info| video_status_writeback(data.clone(), payload.clone(), info));

        assert_eq!(update, Update::RefreshDom, "the hook's Update must win");
        assert_eq!(logged_statuses(&mut log), vec![poster.clone()]);
        assert_eq!(stored_status(&mut data), Some(poster));
        assert_eq!(
            count_virtual_view_rerenders(&changes),
            0,
            "a status is not a frame: nothing to re-render"
        );
    }

    #[test]
    fn a_status_without_a_hook_is_still_stored() {
        let mut data = state(VideoConfig::default());
        let failed = VideoStatus::failed(AzString::from_const_str("no network"));
        let payload = RefAny::new(failed.clone());

        let (update, _) =
            with_callback_info(|info| video_status_writeback(data.clone(), payload.clone(), info));

        assert_eq!(update, Update::DoNothing);
        assert_eq!(stored_status(&mut data), Some(failed));
    }

    #[test]
    fn a_status_payload_of_the_wrong_type_is_ignored() {
        let mut log = status_log(Update::RefreshDom);
        let mut s = base_state(VideoConfig::default());
        s.on_status = status_hook_into(&log);
        let mut data = RefAny::new(s);

        let (update, _) = with_callback_info(|info| {
            video_status_writeback(data.clone(), RefAny::new(0_u32), info)
        });

        assert_eq!(update, Update::DoNothing);
        assert!(logged_statuses(&mut log).is_empty());
        assert_eq!(stored_status(&mut data), Some(VideoStatus::loading()));
    }

    // ==================================================================
    // VideoPlayback: the worker's transport, with the time handed in
    // ==================================================================

    /// A 1 x 1 frame whose width tags it, so a test can tell which one is
    /// on screen.
    fn tagged(tag: u32) -> VideoFrame {
        frame(tag, 1)
    }

    /// A completely decoded 1 s clip of 30 frames; frame `n` is tagged
    /// `n + 1`.
    fn clip(paused: bool, looping: bool) -> VideoPlayback {
        let mut pb = VideoPlayback::new(0.0, paused, looping);
        pb.set_duration(1.0);
        for n in 0..30_u32 {
            pb.push_frame(n as f32 / 30.0, tagged(n + 1));
        }
        pb.finish();
        pb
    }

    /// The tag of the frame a tick presented, if it presented one.
    fn shown(pb: &VideoPlayback, tick: &VideoTick) -> Option<u32> {
        tick.present.and_then(|i| pb.frame(i)).map(|f| f.width)
    }

    fn phase_of(tick: &VideoTick) -> Option<VideoPhase> {
        tick.status.as_ref().map(|s| s.phase)
    }

    fn position_of(tick: &VideoTick) -> Option<f32> {
        tick.status.as_ref().map(|s| s.position_s)
    }

    // ---- the decode gate: the worker holds a window of frames, not the clip

    /// The idle memory spike (2026-09-30): a `<video>` nobody had played held
    /// its whole clip decoded - 300 frames, ~280 MB - because the worker
    /// decoded on while held. Held, the schedule wants the poster and then
    /// nothing until the clock runs.
    #[test]
    fn a_held_video_wants_its_poster_and_no_frame_after_it() {
        let mut pb = VideoPlayback::new(0.0, true, false);
        pb.set_duration(1.0);
        assert!(pb.wants_frame(0.0, None), "the poster");
        pb.push_frame(0.0, tagged(1));
        let _ = pb.tick(0.0);
        assert!(
            !pb.wants_frame(5.0, Some(0.0)),
            "held on its poster, the rest of the clip stays undecoded"
        );
    }

    /// Playing, the worker keeps [`DECODE_LOOKAHEAD_S`] of frames ahead of
    /// the clock and no more.
    #[test]
    fn a_playing_video_decodes_one_lookahead_ahead_of_its_clock() {
        let mut pb = VideoPlayback::new(0.0, false, false);
        pb.set_duration(10.0);
        pb.push_frame(0.0, tagged(1));
        let _ = pb.tick(0.0); // the clock starts with the first frame
        assert!(
            pb.wants_frame(0.5, Some(1.0)),
            "1.0 s decoded at 0.5 s: under the lookahead"
        );
        let full = 0.5 + DECODE_LOOKAHEAD_S + 0.1;
        assert!(!pb.wants_frame(0.5, Some(full)), "the lookahead is full");
        assert!(pb.wants_frame(3.0, Some(full)), "the clock moved on: room again");
    }

    /// Frames the clock has passed are dropped, except the one on screen.
    #[test]
    fn frames_behind_the_clock_are_dropped_and_the_one_on_screen_stays() {
        let mut pb = clip(false, false); // 30 frames, 1 s
        let _ = pb.tick(0.0);
        let tick = pb.tick(0.9);
        assert_eq!(shown(&pb, &tick), Some(28), "frame 28 (0.9 s) is on screen");
        let dropped = pb.trim(0.9);
        // Everything more than DECODE_KEEP_BEHIND_S (0.5 s) behind 0.9 s:
        // the 12 frames before 0.4 s.
        assert!((11..=13).contains(&dropped), "frames behind the clock go: {dropped}");
        assert_eq!(pb.frames_held(), 30 - dropped);
        assert_eq!(pb.on_screen().map(|f| f.width), Some(28), "the frame on screen stays");
        let again = pb.tick(0.9);
        assert_eq!(again.present, None, "and is still the one on screen");
    }

    /// A loop wrap (or a seek back past the kept frames): the schedule shows
    /// nothing new - not the stale frame after the target - and asks the
    /// worker to decode from the target's keyframe again.
    #[test]
    fn a_loop_wrap_asks_for_the_start_to_be_decoded_again() {
        let mut pb = clip(false, true);
        let _ = pb.tick(0.0);
        let _ = pb.tick(0.95);
        let _ = pb.trim(0.95); // the start of the clip is gone
        assert_eq!(pb.restart_wanted(0.95, Some(0.97)), None);
        let wrapped = pb.tick(1.02); // past the 1 s end: wraps to ~0.02 s
        assert!(pb.position(1.02) < 0.5, "the clock wrapped");
        assert_eq!(wrapped.present, None, "no frame at the wrapped position: keep the picture");
        let want = pb.restart_wanted(1.02, Some(0.97));
        assert!(want.is_some_and(|p| p < 0.5), "decode from the start again: {want:?}");
    }

    /// A seek far ahead of the decoder restarts at the target's keyframe
    /// instead of decoding every frame up to it.
    #[test]
    fn a_seek_far_ahead_of_the_decoder_restarts_at_the_target() {
        let mut pb = VideoPlayback::new(0.0, false, false);
        pb.set_duration(10.0);
        for n in 0..15_u32 {
            pb.push_frame(n as f32 / 30.0, tagged(n + 1));
        }
        let _ = pb.tick(0.0);
        assert_eq!(pb.restart_wanted(0.1, Some(0.47)), None);
        pb.seek(8.0, 0.1);
        assert_eq!(pb.restart_wanted(0.1, Some(0.47)), Some(8.0));
    }

    /// A frame decoded again for a time that already has one replaces it
    /// (a restart re-decodes the frames between its keyframe and the target).
    #[test]
    fn a_frame_decoded_again_replaces_the_one_at_its_time() {
        let mut pb = VideoPlayback::new(0.0, true, false);
        pb.push_frame(0.5, tagged(1));
        pb.push_frame(0.5, tagged(2));
        assert_eq!(pb.frames_held(), 1);
        assert_eq!(pb.frame(0).map(|f| f.width), Some(2));
    }

    #[test]
    fn a_video_that_does_not_autoplay_holds_its_first_frame_as_a_poster() {
        let mut pb = clip(true, false);

        let first = pb.tick(0.0);
        assert_eq!(shown(&pb, &first), Some(1), "the first frame is the poster");
        assert_eq!(phase_of(&first), Some(VideoPhase::Paused));

        let later = pb.tick(5.0);
        assert_eq!(later.present, None, "a held video shows nothing new");
        assert_eq!(later.status, None, "and has nothing new to say");
        assert_eq!(pb.position(5.0), 0.0, "its clock never started");
    }

    #[test]
    fn the_clock_starts_with_the_first_frame_not_with_the_download() {
        let mut pb = VideoPlayback::new(0.0, false, false);
        pb.set_duration(1.0);

        let loading = pb.tick(0.0);
        assert_eq!(loading.present, None, "no frame, nothing to show");
        assert_eq!(phase_of(&loading), Some(VideoPhase::Loading));
        assert_eq!(
            loading.status.as_ref().map(|s| s.duration_s),
            Some(1.0),
            "the length is known before the pixels"
        );

        // The download took three seconds: playback starts at 0, not at 3.
        pb.push_frame(0.0, tagged(1));
        pb.push_frame(0.5, tagged(2));
        let first = pb.tick(3.0);
        assert_eq!(shown(&pb, &first), Some(1));
        assert_eq!(phase_of(&first), Some(VideoPhase::Playing));
        assert_eq!(pb.position(3.25), 0.25);
    }

    #[test]
    fn resume_plays_from_the_poster_and_pause_freezes_the_position() {
        let mut pb = clip(true, false);
        let _poster = pb.tick(0.0);

        pb.resume(10.0);
        let playing = pb.tick(10.5);
        assert_eq!(phase_of(&playing), Some(VideoPhase::Playing));
        assert_eq!(shown(&pb, &playing), Some(16), "0.5 s in is frame 15");

        pb.pause(10.75);
        let paused = pb.tick(11.0);
        assert_eq!(phase_of(&paused), Some(VideoPhase::Paused));
        assert_eq!(position_of(&paused), Some(0.75));
        assert_eq!(pb.position(99.0), 0.75, "a held clock stands still");

        pb.resume(20.0);
        assert_eq!(pb.position(20.125), 0.875, "and runs on from where it stood");
    }

    #[test]
    fn pausing_before_the_first_frame_keeps_the_video_held_when_it_arrives() {
        let mut pb = VideoPlayback::new(0.0, false, false);
        pb.pause(0.0);
        pb.push_frame(0.0, tagged(1));
        let tick = pb.tick(1.0);
        assert_eq!(shown(&pb, &tick), Some(1));
        assert_eq!(phase_of(&tick), Some(VideoPhase::Paused));
    }

    #[test]
    fn frames_are_shown_in_presentation_order_whatever_order_they_decode_in() {
        // An I P B B group in DECODE order, the order VideoToolbox hands the
        // frames back in: the P frame is decoded second and shown last.
        let mut pb = VideoPlayback::new(0.0, true, false);
        pb.push_frame(0.0, tagged(1));
        pb.push_frame(0.3, tagged(4));
        pb.push_frame(0.1, tagged(2));
        pb.push_frame(0.2, tagged(3));
        pb.finish();

        let order: Vec<u32> = (0..4).filter_map(|i| pb.frame(i)).map(|f| f.width).collect();
        assert_eq!(order, vec![1, 2, 3, 4]);

        pb.seek(0.15, 0.0);
        let tick = pb.tick(0.0);
        assert_eq!(
            shown(&pb, &tick),
            Some(2),
            "0.15 s is the first B frame, not the P frame decoded before it"
        );
    }

    #[test]
    fn a_frame_decoded_after_the_one_on_screen_does_not_move_the_picture() {
        let mut pb = VideoPlayback::new(0.3, true, false);
        pb.push_frame(0.3, tagged(4));
        let first = pb.tick(0.0);
        assert_eq!(shown(&pb, &first), Some(4));

        // A B frame that is shown earlier arrives later.
        pb.push_frame(0.1, tagged(2));
        let again = pb.tick(0.0);
        assert_eq!(again.present, None, "the frame on screen is still the right one");
    }

    #[test]
    fn a_video_that_ends_holds_its_last_frame_and_says_so_once() {
        let mut pb = clip(false, false);
        let _start = pb.tick(0.0);

        let end = pb.tick(2.0);
        assert_eq!(shown(&pb, &end), Some(30));
        assert_eq!(phase_of(&end), Some(VideoPhase::Ended));
        assert_eq!(position_of(&end), Some(1.0));

        let after = pb.tick(3.0);
        assert_eq!(after.status, None, "Ended is reported once");
        assert_eq!(after.present, None);
    }

    #[test]
    fn playing_after_the_end_starts_over() {
        let mut pb = clip(false, false);
        let _start = pb.tick(0.0);
        let _end = pb.tick(2.0);

        pb.resume(5.0);
        let again = pb.tick(5.0);
        assert_eq!(shown(&pb, &again), Some(1));
        assert_eq!(phase_of(&again), Some(VideoPhase::Playing));
        assert_eq!(position_of(&again), Some(0.0));
    }

    #[test]
    fn a_looping_video_wraps_to_the_start() {
        let mut pb = clip(false, true);
        let _start = pb.tick(0.0);

        let wrapped = pb.tick(1.25);
        assert_eq!(phase_of(&wrapped), Some(VideoPhase::Playing));
        assert_eq!(position_of(&wrapped), Some(0.25), "the wrap is reported at once");
        assert_eq!(shown(&pb, &wrapped), Some(8), "0.25 s is frame 7");
    }

    #[test]
    fn a_seek_moves_the_picture_and_is_reported_at_once() {
        let mut pb = clip(true, false);
        let _poster = pb.tick(0.0);

        pb.seek(0.5, 1.0);
        let tick = pb.tick(1.0);
        assert_eq!(shown(&pb, &tick), Some(16));
        assert_eq!(
            phase_of(&tick),
            Some(VideoPhase::Paused),
            "a seek keeps a held video held"
        );
        assert_eq!(position_of(&tick), Some(0.5));
    }

    #[test]
    fn a_seek_is_clamped_into_the_video() {
        let mut pb = clip(true, false);
        pb.seek(f32::NAN, 0.0);
        assert_eq!(pb.position(0.0), 0.0);
        pb.seek(-3.0, 0.0);
        assert_eq!(pb.position(0.0), 0.0);
        pb.seek(f32::INFINITY, 0.0);
        assert_eq!(pb.position(0.0), 0.0);
        pb.seek(99.0, 0.0);
        assert_eq!(pb.position(0.0), 1.0, "past the end is the end");
    }

    #[test]
    fn position_reports_come_about_four_times_a_second() {
        let mut pb = clip(false, false);
        let mut reports = 0;
        for step in 0..100_u32 {
            // 0.99 s of playback in 10 ms steps
            if pb.tick(f64::from(step) * 0.01).status.is_some() {
                reports += 1;
            }
        }
        assert!(
            (3..=5).contains(&reports),
            "one report per {STATUS_INTERVAL_S} s of playback, got {reports}"
        );
    }

    #[test]
    fn playback_that_outruns_the_decoder_waits_for_it() {
        let mut pb = VideoPlayback::new(0.0, false, false);
        pb.set_duration(10.0);
        pb.push_frame(0.0, tagged(1));
        pb.push_frame(0.5, tagged(2));
        let _start = pb.tick(0.0);

        let starved = pb.tick(2.0);
        assert_eq!(shown(&pb, &starved), Some(2));
        assert_eq!(
            pb.position(2.0),
            0.5,
            "the clock waits at the newest decoded frame"
        );
        assert_ne!(
            phase_of(&starved),
            Some(VideoPhase::Ended),
            "a video still decoding has not ended"
        );
    }

    /// A download slower than the video: the clock waits at the newest frame
    /// and, once that lasts, the app hears `Loading` (buffering) - not a
    /// `Playing` whose picture stands still; it plays on from where it stood
    /// once half a second is decoded ahead, not frame by frame.
    #[test]
    fn a_stall_that_lasts_reports_loading_and_plays_on_with_a_lead() {
        let frame_s = 1.0_f32 / 30.0;
        let mut pb = VideoPlayback::new(0.0, false, false);
        pb.set_duration(10.0);
        pb.push_frame(0.0, tagged(1));
        pb.push_frame(frame_s, tagged(2));
        let start = pb.tick(0.0);
        assert_eq!(phase_of(&start), Some(VideoPhase::Playing));

        // The download stops: the clock passes the newest frame and waits there.
        let late = pb.tick(0.2);
        assert_eq!(pb.position(0.2), frame_s, "the clock waits at the newest frame");
        assert_ne!(
            phase_of(&late),
            Some(VideoPhase::Loading),
            "a moment's wait is not buffering yet"
        );
        let stalled = pb.tick(0.2 + STALL_REPORT_S + 0.05);
        assert_eq!(
            phase_of(&stalled),
            Some(VideoPhase::Loading),
            "a wait that lasts is buffering"
        );
        assert_eq!(pb.position(9.0), frame_s, "the clock stands while buffering");

        // Frames arrive, less than the lead: still waiting.
        for n in 2..10_u32 {
            pb.push_frame(n as f32 * frame_s, tagged(n + 1));
        }
        let _ = pb.tick(1.0);
        assert_eq!(pb.position(1.5), frame_s, "less than the lead: still waiting");

        // Half a second decoded ahead: plays on from where it stood.
        for n in 10..30_u32 {
            pb.push_frame(n as f32 * frame_s, tagged(n + 1));
        }
        let resumed = pb.tick(2.0);
        assert_eq!(phase_of(&resumed), Some(VideoPhase::Playing));
        assert!(
            (pb.position(2.1) - (frame_s + 0.1)).abs() < 1e-4,
            "on from where it stood: {}",
            pb.position(2.1)
        );
    }

    /// A paused or a seeked video does not stall: a pause holds the frame on
    /// screen as the poster, a seek waits for the frames at its target.
    #[test]
    fn a_pause_during_a_stall_holds_and_a_resume_waits_for_the_lead_again() {
        let frame_s = 1.0_f32 / 30.0;
        let mut pb = VideoPlayback::new(0.0, false, false);
        pb.set_duration(10.0);
        pb.push_frame(0.0, tagged(1));
        let _ = pb.tick(0.0);
        let _ = pb.tick(1.0); // stalled at 0.0
        pb.pause(1.1);
        let held = pb.tick(2.0);
        assert_eq!(phase_of(&held), Some(VideoPhase::Paused));
        assert_eq!(pb.position(5.0), 0.0);
        // Resumed with no frame ahead: waits (and after a moment says so).
        pb.resume(3.0);
        let _ = pb.tick(3.0);
        let waiting = pb.tick(3.0 + STALL_REPORT_S + 0.05);
        assert_eq!(phase_of(&waiting), Some(VideoPhase::Loading));
        assert_eq!(pb.position(4.0), 0.0);
        for n in 1..30_u32 {
            pb.push_frame(n as f32 * frame_s, tagged(n + 1));
        }
        let playing = pb.tick(5.0);
        assert_eq!(phase_of(&playing), Some(VideoPhase::Playing));
    }

    /// An app that holds a video only to start it in step with its sound (a
    /// media center fading its menus out first): held, the video decodes a
    /// lead past its poster, so the resume plays at once - no wait for the
    /// decoder while the sound already runs.
    #[test]
    fn a_video_held_to_start_soon_decodes_its_lead_and_starts_at_once() {
        let frame_s = 1.0_f32 / 30.0;
        let mut pb = VideoPlayback::new(0.0, true, false);
        pb.set_duration(10.0);
        pb.set_preroll(RESUME_LEAD_S);
        assert!(pb.wants_frame(0.0, None), "the poster");
        pb.push_frame(0.0, tagged(1));
        let poster = pb.tick(0.0);
        assert_eq!(phase_of(&poster), Some(VideoPhase::Paused));
        assert!(pb.wants_frame(1.0, Some(0.0)), "held, it decodes its lead");
        for n in 1..=15_u32 {
            pb.push_frame(n as f32 * frame_s, tagged(n + 1));
        }
        assert!(
            !pb.wants_frame(1.0, Some(15.0 * frame_s)),
            "and no more than the lead"
        );
        pb.resume(2.0);
        let playing = pb.tick(2.0);
        assert_eq!(phase_of(&playing), Some(VideoPhase::Playing));
        assert!(
            (pb.position(2.25) - 0.25).abs() < 1e-4,
            "it starts at once: {}",
            pb.position(2.25)
        );
    }

    /// A decoded frame is the picture BEHIND the player's controls, which the
    /// app names ("Play video", "Seek"): the frame is marked decorative
    /// (`role: Nothing`, no name) so it is neither absent-and-unexplained nor
    /// announced once per frame.
    #[test]
    fn a_decoded_frame_is_a_decorative_image() {
        let dom = frame_image(placeholder_image(b"frame"));
        assert!(matches!(dom.root.get_node_type(), NodeType::Image(_)));
        let info = dom
            .root
            .accessibility
            .as_ref()
            .expect("a frame image must declare accessibility, or it is absent from the tree");
        assert_eq!(info.role, azul_core::a11y::AccessibilityRole::Nothing);
        assert!(info.accessibility_name.as_ref().is_none());
    }

    // ==================================================================
    // Theme: the video's own chrome is its "no signal" poster
    // ==================================================================

    /// The poster a render pass emits for a widget state in `theme` that has
    /// no decoded frame yet.
    fn poster_of(theme: crate::widgets::themes::UiTheme) -> Dom {
        let mut s = base_state(VideoConfig::default());
        s.theme = theme;
        let dataset = RefAny::new(s);
        let ret = with_virtual_view_info(320.0, 180.0, |info| {
            video_widget_render(dataset.clone(), info)
        });
        match ret.dom {
            OptionDom::Some(d) => d,
            OptionDom::None => panic!("no frame yet must still render the poster"),
        }
    }

    #[test]
    fn a_video_without_a_theme_follows_the_app_theme_flat_by_default() {
        use crate::widgets::themes::{theme_checks as tc, OptionUiTheme};
        let w = VideoWidget::create(VideoConfig::default());
        assert_eq!(w.theme, OptionUiTheme::None);
        assert!(tc::has_class(&w.dom(), "__azul-theme-flat"));
        let dom = {
            let _app = azul_core::app_theme::ThemeScope::enter(AzString::from_const_str("flora"));
            VideoWidget::create(VideoConfig::default()).dom()
        };
        assert!(tc::has_class(&dom, "__azul-theme-flora"), "built for flora, it is flora's");
    }

    /// An UNPINNED video's poster carries every theme's block: under either
    /// app theme it paints that theme's poster, in light and in dark.
    #[test]
    fn an_unpinned_videos_poster_carries_every_themes_block() {
        use azul_css::{
            dynamic_selector::{CssPropertyWithConditions, DynamicSelectorContext, ThemeCondition},
            props::property::CssProperty,
        };

        use crate::widgets::themes::UiTheme;

        let dom = VideoWidget::create(VideoConfig::default()).dom();
        let dataset = dom.root.get_dataset().cloned().expect("the widget state");
        let ret = with_virtual_view_info(320.0, 180.0, |info| {
            video_widget_render(dataset.clone(), info)
        });
        let poster = match ret.dom {
            OptionDom::Some(d) => d,
            OptionDom::None => panic!("no frame yet must still render the poster"),
        };
        let decls: Vec<CssPropertyWithConditions> = poster
            .root
            .style
            .iter_inline_properties()
            .map(|(p, c)| CssPropertyWithConditions {
                property: p.clone(),
                apply_if: c.clone(),
            })
            .collect();
        for theme in [UiTheme::Flat, UiTheme::Flora] {
            let own: Vec<CssPropertyWithConditions> = poster_of(theme)
                .root
                .style
                .iter_inline_properties()
                .map(|(p, c)| CssPropertyWithConditions {
                    property: p.clone(),
                    apply_if: c.clone(),
                })
                .collect();
            for dark in [false, true] {
                let ctx = DynamicSelectorContext {
                    mode: if dark {
                        azul_css::system::DarkLightMode::Dark
                    } else {
                        azul_css::system::DarkLightMode::Light
                    },
                    ..Default::default()
                }
                .with_app_theme(theme.name());
                let bg = |v: &[CssPropertyWithConditions]| {
                    v.iter()
                        .filter(|p| p.matches(&ctx))
                        .filter(|p| matches!(p.property, CssProperty::BackgroundContent(_)))
                        .next_back()
                        .map(|p| p.property.clone())
                };
                assert_eq!(bg(&decls), bg(&own), "{theme:?} dark={dark}");
            }
        }
    }

    #[test]
    fn set_theme_and_with_theme_agree_and_reach_the_widget_state() {
        use crate::widgets::themes::{theme_checks as tc, OptionUiTheme, UiTheme};
        let mut a = VideoWidget::create(VideoConfig::default());
        a.set_theme(UiTheme::Flora);
        assert_eq!(a.theme, OptionUiTheme::Some(UiTheme::Flora));
        let b = VideoWidget::create(VideoConfig::default()).with_theme(UiTheme::Flora);
        assert_eq!(b.theme, a.theme);
        let dom = b.dom();
        assert!(tc::has_class(&dom, "__azul-theme-flora"));
        let mut dataset = dom.root.get_dataset().cloned().expect("the widget state");
        let theme = dataset.downcast_ref::<VideoWidgetState>().map(|s| s.theme);
        assert_eq!(
            theme,
            Some(UiTheme::Flora),
            "the render callback reads the theme off the state"
        );
    }

    #[test]
    fn a_flat_poster_is_the_established_dark_screen_in_both_modes() {
        use crate::widgets::themes::{theme_checks as tc, UiTheme};
        let p = poster_of(UiTheme::Flat);
        for dark in [false, true] {
            assert_eq!(
                tc::background(&p, dark).and_then(|x| tc::bg_color(&x)),
                Some(azul_css::props::basic::color::ColorU::rgb(42, 42, 48)),
                "dark={dark}"
            );
            assert_eq!(
                tc::border_top_color(&p, dark, None),
                Some(azul_css::props::basic::color::ColorU::rgb(68, 68, 76)),
                "dark={dark}"
            );
        }
    }

    #[test]
    fn a_flora_poster_is_an_ink_panel_by_day_and_by_night() {
        use azul_css::props::basic::color::ColorU;

        use crate::widgets::themes::{theme_checks as tc, UiTheme};
        let p = poster_of(UiTheme::Flora);
        assert_eq!(
            tc::background(&p, false).and_then(|x| tc::bg_color(&x)),
            Some(ColorU::rgb(33, 31, 27))
        );
        assert_eq!(
            tc::background(&p, true).and_then(|x| tc::bg_color(&x)),
            Some(ColorU::rgb(20, 20, 20))
        );
        assert_eq!(tc::border_top_color(&p, false, None), Some(ColorU::rgb(68, 63, 53)));
        assert_eq!(tc::border_top_color(&p, true, None), Some(ColorU::rgb(54, 54, 54)));
        tc::assert_theme_invariants("video poster Flora", &p);
    }

    #[test]
    fn a_rebuild_adopts_the_new_theme() {
        use crate::widgets::themes::UiTheme;
        let old = state(VideoConfig::default());
        let mut fresh = base_state(VideoConfig::default());
        fresh.theme = UiTheme::Flora;
        let mut merged = merge_video_state(RefAny::new(fresh), old);
        let theme = merged.downcast_ref::<VideoWidgetState>().map(|s| s.theme);
        assert_eq!(theme, Some(UiTheme::Flora));
    }

    #[test]
    fn the_theme_changes_the_poster_not_the_accessibility_tree() {
        use crate::widgets::themes::{theme_checks as tc, UiTheme};
        assert_eq!(
            tc::a11y_outline(&poster_of(UiTheme::Flat)),
            tc::a11y_outline(&poster_of(UiTheme::Flora))
        );
    }

    /// R5: the widget's STRUCTURE (display, overflow, ...) is its base -
    /// declared once, outside every `@theme(<name>)` block, so it holds under
    /// flat, flora and any theme to come: the widget's own nodes, and the
    /// poster an unpinned widget's render pass draws.
    #[test]
    fn a_video_declares_its_structure_once_for_every_theme() {
        use crate::widgets::themes::{
            theme_blocks::checks::{under, BOTH},
            theme_checks::assert_structure_is_shared,
        };
        for t in BOTH {
            let dom = under(t, || VideoWidget::create(VideoConfig::default()).dom());
            assert_structure_is_shared(&format!("video, built for {}", t.name()), &dom, &[]);

            let dataset = dom.root.get_dataset().cloned().expect("the widget state");
            let ret = under(t, || {
                with_virtual_view_info(320.0, 180.0, |info| {
                    video_widget_render(dataset.clone(), info)
                })
            });
            let poster = match ret.dom {
                OptionDom::Some(d) => d,
                OptionDom::None => panic!("no frame yet must still render the poster"),
            };
            assert_structure_is_shared(
                &format!("video poster, built for {}", t.name()),
                &poster,
                &[],
            );
        }
    }
}
