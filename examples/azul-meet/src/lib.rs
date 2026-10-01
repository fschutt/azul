//! AzMeet: video meetings over azul.iroh.
//!
//! With a meeting server (the `meet` Worker, azul-apps `cf-workers/meet`) AzMeet opens a start
//! screen: "New meeting" asks the server for a room and shows its link; "Join with a link" takes
//! that link. Either way the app announces its iroh ticket to the room every 20 seconds, reads
//! everyone else's every 2 seconds, and dials the peers whose endpoint id is higher than its own.
//! Every HTTP request runs on an azul `Thread` and resumes on the UI thread, so no callback waits
//! on the network. The start screen's "Meeting server" field holds the Worker's address: prefilled
//! with the one saved last time, else `AZMEET_WORKER`, else the built-in default; a new address
//! is used for every request from Enter or leaving the field on, checked with `GET /health`, and
//! saved (in the per-user config folder, `AzMeet/settings.txt`) once it answers.
//!
//! Video (see `video_wire.rs`): each captured camera or screen frame, in every rendition someone
//! shows, goes through an H.264 `VideoEncoder` where one works (VideoToolbox on Apple; found out at start by encoding a
//! test frame and decoding it back), else it is JPEG-encoded; the window says which ("Video:
//! H.264 (VideoToolbox)" / "Video: JPEG (no encoder)"). H.264 packets go to every peer that
//! decodes H.264 as reliable, ordered iroh messages, since a P-frame needs every packet since the
//! last keyframe; JPEG frames go to the others on the latest-wins frame path. Each peer's tracks
//! have their own `VideoDecoder`: after a missing packet it decodes nothing until the next
//! keyframe and asks the sender for one, which the sender forces (at most twice a second, and
//! every 3 seconds anyway, and for every new peer). A peer whose link falls 24 packets behind is
//! paused and resumes at a keyframe.
//!
//! Audio (see `audio.rs`): the microphone's chunks are cut into 20 ms mono 16-bit PCM packets and
//! sent on their own track, three packets to a frame (iroh frames are latest-wins, so a skipped
//! frame loses nothing). Each peer's packets go into its own jitter buffer, which a playout thread
//! drains every 20 ms into that peer's `AudioSink`; the UI thread only pushes packets. Mute and
//! deafen travel as a two-byte control message and show in the people list ("Ben · connected ·
//! muted"). Leave disconnects every peer, stops announcing, removes this participant from the
//! room on the meeting server, and returns to the start screen.
//!
//! Rooms of three and more (see `routes.rs`): every participant sends everyone a small report (a
//! `ConnectionSync`: its uplink, estimated from iroh's path statistics, how stable it is, and the
//! tiles it shows) on connect, on every change and every 2 seconds. Every side feeds the same
//! reports to `IrohLoadBalancer` and gets the same plan: up to the mesh cap everyone sends to
//! everyone; above it the best-connected peers form the backbone, and every other participant
//! uploads its media once, to its backbone parent, which passes it on (audio frames, video
//! packets, and the keyframe requests and acknowledgements that travel back toward the origin).
//! Each tile asks for the rendition its role and laid-out height need (`IrohTileRole`: grid tile,
//! stage, thumbnail); a sender encodes the smallest and the largest rendition asked for, one encoder
//! each, and nothing nobody shows. The network panel shows the plan, the routes, this side's report
//! and one line per peer.
//!
//! Without a reachable meeting server it runs the in-process demo: two participants, Ada in a
//! CPU-rendered window and Ben in a GPU-rendered one, linked by two iroh endpoints.
//!
//! Environment:
//! - `AZMEET_WORKER`: the meeting server when none was saved from the start screen, e.g.
//!   `http://127.0.0.1:8787` (the local mock); else the `PRODUCTION_WORKER` constant, set at build
//!   time with `AZMEET_DEFAULT_WORKER=<url>`, else `http://127.0.0.1:8787`. A headless run
//!   (`AZ_BACKEND=headless`) neither reads nor writes the saved one, so the variable always wins
//!   there. Only when nothing is saved or set and the built-in default does not answer does the
//!   in-process demo open.
//! - `AZMEET_NAME`: the name others see (default: `$USER`).
//! - `AZMEET_AUTOCREATE=1`: create a meeting at start and print `AZMEET_LINK <link>` on stdout.
//! - `AZMEET_JOIN=<link>`: join that meeting at start.
//! - `AZMEET_RELAY`: `off`, `default` or a relay URL (default: off for a meeting server on this
//!   machine, the public iroh relays otherwise).
//! - `AZMEET_TEST_TONE=1`: a 440 Hz tone replaces the microphone, which starts unmuted.
//! - `AZMEET_TEST_PATTERN=1`: moving colour bars replace the camera (and the screen share), the
//!   camera starts on, and a "Drop a video packet" button drops the next packet before it leaves.
//! - `AZMEET_VIDEO_CODEC=jpeg`: send JPEG even where H.264 works.
//! - `AZMEET_MESH_CAP=<n>`: rooms of up to n people send everything directly (default 4; the
//!   design's value is 8).
//! - `AZMEET_UPLINK_KBPS=<kbit/s>`: report this uplink instead of the estimate.
//! - `AZMEET_NO_FORWARD=1`: never forward other people's media; `AZMEET_ON_BATTERY=1`: report
//!   running on battery (either ranks this side last for the backbone).
//! - `AZMEET_LAYOUT=speaker` and `AZMEET_STAGE=<name>`: start in speaker view with that participant
//!   on the stage (else the first by name); the toolbar switches between grid and speaker view.
//! - `AZ_BACKEND=headless`: no audio device, camera or screen is opened: the microphone is the
//!   tone (muted until switched on, unless `AZMEET_TEST_TONE=1`), received audio is counted, not
//!   played, and the camera and the screen share are test patterns (off until switched on, unless
//!   `AZMEET_TEST_PATTERN=1`).

mod args;
mod audio;
mod chat;
mod pace;
mod rooms;
mod routes;
mod speaker;
mod tiles;
mod ui;
mod video_wire;

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex, MutexGuard, PoisonError, Weak},
};

use azul::{
    app::RendererOptions,
    audio::{AudioConfig, AudioDeviceList, AudioDeviceListResult, AudioFrame, AudioSink},
    callbacks::{
        CallbackInfo, CallbackType, TimerCallbackInfo, TimerCallbackReturn, UpdateImageType,
    },
    camera::CameraConfig,
    css::{LogicalSize, PhysicalPositionI32, Srgb, WindowPosition},
    dom::{Callback, ClipboardContent, DomNodeId, NodeId, VirtualKeyCode},
    error::{HttpError, ResultRawImageDecodeImageError, ResultU8VecEncodeImageError},
    file::FilePath,
    http::{HttpGetResult, HttpMethod, HttpRequestConfig},
    image::{ImageRef, RawImage, RawImageData, RawImageFormat},
    iroh::{
        IrohConfig, IrohEndpoint, IrohEvent, IrohEventKind, IrohLoadBalancer, IrohPeerCapacity,
        IrohRelayMode, IrohTileRole,
    },
    json::{Json, JsonKeyValue},
    option::{OptionRendererOptions, OptionString},
    prelude::*,
    screen::ScreenCaptureConfig,
    str::String as AzString,
    task::{Thread, ThreadId, ThreadReceiver, ThreadSender, Timer, TimerId},
    time::{Duration, SystemTimeDiff},
    url::Url,
    vec::{F32Vec, StyledTextRunVec, U8Vec, U8VecRef},
    video::VideoFrame,
    widgets::{
        ButtonType, CameraWidget, ConsumerFrame, FrameConsumer, MicrophoneWidget,
        OnTextInputReturn, ProgressBar, ScreenCaptureWidget, TextInputState, TextInputValid,
        Titlebar,
    },
    video::{VideoDecoder, VideoEncoder},
    window::{HwAcceleration, PlatformCapability, Vsync, WindowDecorations},
};
use rooms::{Dialed, PeerRecord, Relay, RoomKey};
use video_wire::{Codec, Control, Message};

/// The protocol name: peers of an older wire format (M3's, without renditions and forwarding)
/// cannot connect.
const ALPN: &str = "azmeet/3";
const CAMERA_TRACK: u32 = 1;
const SCREEN_TRACK: u32 = 2;
/// The audio track: 20 ms PCM packets, three to a frame (`audio.rs`).
const AUDIO_TRACK: u32 = 3;
/// The rate the microphone (or the test tone) is asked for.
const MIC_RATE: u32 = 48_000;
const TONE_HZ: f32 = 440.0;
const FEED_W: u32 = 320;
const FEED_H: u32 = 180;
/// The pixel format video travels in: NV12 (4:2:0 YCbCr in two planes, the
/// camera's own format), which the H.264 encoder takes and the decoder gives
/// without a conversion, and which a tile shows through the GPU's YUV shader
/// (or the CPU rasterizer's fused convert of the rows it paints). The real
/// matrix and range travel with every frame (`VideoFrame::format`).
const VIDEO_FORMAT: RawImageFormat = RawImageFormat::NV12Rec709Video;
const JPEG_QUALITY: u8 = 75;
/// The H.264 encoder's target bitrate for the 320x180 probe frame (renditions use
/// `IrohLoadBalancer::rendition_kbps`).
const VIDEO_KBPS: u32 = 400;
/// Audio one participant sends one peer (kbit/s): 48 kHz 16-bit PCM, every packet sent three
/// times.
const AUDIO_KBPS: u64 = 2304;
/// What the camera and the screen tracks are called in the window, by `track_slot`.
const SOURCES: [&str; 2] = ["camera", "screen"];
/// How often the statistics are gathered (and the report re-sent, and the plan re-made).
const STATS_EVERY_MS: u64 = 2000;

/// The meeting server when `AZMEET_WORKER` is not set: the deployed `meet` Worker (azul-apps
/// `cf-workers/meet/README.md`, "Deploy"), baked in at build time with
/// `AZMEET_DEFAULT_WORKER=https://...`. Empty means no meeting server, so the demo runs.
const PRODUCTION_WORKER: &str = match option_env!("AZMEET_DEFAULT_WORKER") {
    Some(url) => url,
    None => "",
};
/// How often a participant in a room reads the peers list.
const ROOM_POLL_MS: u64 = 2000;
/// Polls between two announcements (the Worker keeps a record for 120 seconds).
const REANNOUNCE_POLLS: u32 = 10;
/// Polls after which a request that never answered is given up.
const STUCK_REQUEST_POLLS: u32 = 8;
const HTTP_TIMEOUT_SECS: u64 = 5;

/// One connected peer.
struct Remote {
    /// Connection handle on this endpoint.
    handle: u64,
    /// The peer's endpoint id.
    node_id: String,
    /// The peer's key in the routing plan (`routes::peer_key` of its endpoint id).
    key: u64,
    /// Which of the peer's tracks (camera, screen) have a tile.
    tracks: [bool; 2],
    /// Muted and deafened, from the peer's last control message; `None` until the first.
    state: Option<audio::PeerState>,
    /// Whether the peer decodes H.264, from its caps message; `None` until that arrives, and the
    /// peer gets JPEG until then.
    h264: Option<bool>,
    /// Whether the peer encodes H.264, from its caps message.
    encodes: Option<bool>,
    /// The peer's last `ConnectionSync`; the peer is part of the plan from its first one.
    sync: Option<routes::Sync>,
    /// Direct (else relayed) and the RTT in ms of the connection, from the last statistics.
    path: Option<(bool, f64)>,
    /// The laid-out height of the peer's camera and screen tiles, by `track_slot`.
    tile_height: [Option<f32>; 2],
    /// The same tiles' size in device pixels: what the decoders hand frames
    /// out at (scaled once, by the decoder, not by the renderer every paint).
    tile_px: [Option<(u32, u32)>; 2],
    /// Whether some of each tile shows in the window (`CallbackInfo::is_node_visible`): a tile
    /// scrolled out of the tiles pane, or in a minimized window, asks for no stream.
    tile_visible: [bool; 2],
    /// How far this side ran ahead of the peer on each of its own H.264 streams, by (track,
    /// rendition height).
    sent: BTreeMap<(u32, u16), video_wire::SendWindow>,
    /// The peer's video streams as they reach this side (to show, or to pass on), by (track,
    /// rendition height).
    received: BTreeMap<(u32, u16), VideoIn>,
}

impl Remote {
    fn new(handle: u64, node_id: String) -> Self {
        Remote {
            handle,
            key: routes::peer_key(&node_id),
            node_id,
            tracks: [false; 2],
            state: None,
            h264: None,
            encodes: None,
            sync: None,
            path: None,
            tile_height: [None; 2],
            tile_px: [None; 2],
            tile_visible: [true; 2],
            sent: BTreeMap::new(),
            received: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Stage {
    /// The start screen: "New meeting" and "Join with a link".
    Start,
    /// A new room or a link is being looked up.
    Opening,
    /// In the room: announcing and polling.
    InRoom,
    /// The room is gone from the server; the peers already connected stay.
    Ended,
}

/// A participant's side of a meeting-server room.
struct RoomSession {
    worker: String,
    name: String,
    stage: Stage,
    join_text: String,
    /// This endpoint's id and ticket (the ticket arrives with the `Ready` event).
    node_id: String,
    ticket: String,
    room_id: String,
    code: String,
    link: String,
    copied: bool,
    /// The peers list of the last poll.
    peers: Vec<PeerRecord>,
    dialed: BTreeMap<String, Dialed>,
    polls: u32,
    announced_at: Option<u32>,
    /// A request is in flight; `busy_polls` counts the polls it has been.
    busy: bool,
    busy_polls: u32,
    /// The last request failed and `MeetState::notice` says why.
    trouble: bool,
    /// Counts the meetings entered and left. A request carries the session it was sent in, and
    /// an answer from an earlier session (a meeting since left) is ignored.
    session: u32,
    /// The meeting server field as typed; `worker` takes it on Enter or when the field loses
    /// focus.
    server_text: String,
    /// Under the field: whether the meeting server answers.
    server_status: String,
    /// The meeting server answered its last check.
    server_ok: bool,
    /// Counts the checks of the meeting server; the answer to an older one is ignored.
    checks: u32,
    /// Where the iroh endpoint relays, chosen for the meeting server's host.
    relay: Relay,
}

impl RoomSession {
    fn new(worker: String, name: String, relay: Relay) -> Self {
        RoomSession {
            session: 0,
            server_text: worker.clone(),
            server_status: String::new(),
            server_ok: false,
            checks: 0,
            relay,
            worker,
            name,
            stage: Stage::Start,
            join_text: String::new(),
            node_id: String::new(),
            ticket: String::new(),
            room_id: String::new(),
            code: String::new(),
            link: String::new(),
            copied: false,
            peers: Vec::new(),
            dialed: BTreeMap::new(),
            polls: 0,
            announced_at: None,
            busy: false,
            busy_polls: 0,
            trouble: false,
        }
    }

    fn enter(&mut self, found: RoomInfo) {
        self.session = self.session.wrapping_add(1);
        self.stage = Stage::InRoom;
        self.room_id = found.room;
        self.code = found.code;
        self.link = found.link;
        self.copied = false;
        self.peers.clear();
        self.dialed.clear();
        self.polls = 0;
        self.announced_at = None;
    }

    fn leave(&mut self) {
        self.session = self.session.wrapping_add(1);
        self.stage = Stage::Start;
        self.room_id.clear();
        self.code.clear();
        self.link.clear();
        self.peers.clear();
        self.dialed.clear();
        self.announced_at = None;
        self.busy = false;
    }

    /// The announcement to send right away, when the ticket is already known.
    fn first_job(&mut self) -> Option<HttpJob> {
        if self.stage != Stage::InRoom || self.busy || self.ticket.is_empty() {
            return None;
        }
        self.busy = true;
        self.busy_polls = 0;
        self.announced_at = Some(self.polls);
        Some(HttpJob::announce(self))
    }

    /// What this poll sends: an announcement when one is due, else a read of the peers list.
    fn next_job(&mut self) -> Option<HttpJob> {
        if self.stage != Stage::InRoom {
            return None;
        }
        self.polls = self.polls.wrapping_add(1);
        if self.busy {
            self.busy_polls += 1;
            if self.busy_polls < STUCK_REQUEST_POLLS {
                return None;
            }
            self.busy = false;
        }
        if self.ticket.is_empty() {
            return None;
        }
        let polls = self.polls;
        let due = self
            .announced_at
            .map_or(true, |at| polls.wrapping_sub(at) >= REANNOUNCE_POLLS);
        self.busy = true;
        self.busy_polls = 0;
        if due {
            self.announced_at = Some(polls);
            Some(HttpJob::announce(self))
        } else {
            Some(HttpJob::poll(self))
        }
    }
}

struct MeetState {
    name: String,
    /// The other participant of the in-process demo.
    peer_name: String,
    backend: &'static str,
    /// The demo's meeting name, shown in its header.
    meeting: String,
    endpoint: Option<IrohEndpoint>,
    /// The demo's second endpoint, dialed once this one is ready.
    guest: Option<IrohEndpoint>,
    remotes: Vec<Remote>,
    link_status: String,
    /// One line under the header: why the demo runs, or what the meeting server said.
    notice: String,
    /// When (`now_ms`) the statistics were gathered last.
    stats_at_ms: u64,
    /// The statistics text the overlay showed last: the window changes for the statistics only
    /// while the overlay is open and its text moved.
    stats_shown: Vec<String>,
    /// How often the pump runs (`pace.rs`).
    pace: pace::PumpPace,
    mic_on: bool,
    cam_on: bool,
    screen_on: bool,
    mic_level: f32,
    /// When (`now_ms`) the level meter last moved: it moves at most every
    /// `METER_INTERVAL_MS`, not with every 20 ms audio chunk.
    meter_moved_ms: Option<u64>,
    meter_bar: Option<DomNodeId>,
    mics: Vec<String>,
    speakers: Vec<String>,
    devices_requested: bool,
    /// Some when a meeting server is in use.
    room: Option<RoomSession>,
    /// This participant hears nobody: received audio is dropped.
    deafened: bool,
    /// The microphone is the test tone (`AZMEET_TEST_TONE=1`, and every headless run).
    tone_mic: bool,
    /// The tone while the microphone is on, and when it started.
    tone: Option<(audio::ToneSource, std::time::Instant)>,
    packetizer: audio::Packetizer,
    /// Received audio, shared with the playout thread; started with the first packet.
    playout: Option<Arc<Mutex<Playout>>>,
    /// Received audio may be played on a device (false in a headless run).
    play_audio: bool,
    /// This participant's clock for the video rules (keyframe spacing, keyframe requests).
    clock: std::time::Instant,
    /// What this machine does with H.264 (`probe_video`); the encoder turns to `Err` when it stops
    /// working.
    video: VideoSupport,
    /// The sending side of this side's video, one per (track, rendition height).
    video_out: BTreeMap<(u32, u16), VideoOut>,
    /// The camera and the screen share are test patterns (`AZMEET_TEST_PATTERN=1`, and every
    /// headless run).
    pattern_video: bool,
    /// When each test pattern's next frame is due, by `track_slot`.
    pattern_clocks: [video_wire::PatternClock; 2],
    /// Shows the "Drop a video packet" button (`AZMEET_TEST_PATTERN=1`).
    video_debug: bool,
    /// The renditions whose next packet is dropped instead of sent (the button).
    drop_video: BTreeSet<(u32, u16)>,
    /// This participant's key in the routing plan (`routes::peer_key` of its endpoint id).
    me: u64,
    /// Rooms of up to this many people send everything directly (`AZMEET_MESH_CAP`).
    mesh_cap: u32,
    /// The uplink and stability this side reports.
    capacity: routes::CapacityEstimator,
    /// Never forward other people's media (`AZMEET_NO_FORWARD=1`).
    opted_out: bool,
    /// Report running on battery (`AZMEET_ON_BATTERY=1`).
    on_battery: bool,
    /// What this side last told the others (its `ConnectionSync`).
    sync: routes::Sync,
    /// Who sends whose media to whom, from everyone's reports.
    plan: routes::Plan,
    view: ViewMode,
    /// Who is on the stage in speaker view (`AZMEET_STAGE`); empty for the first by name.
    stage_name: String,
    /// Device pixels per logical pixel, from the window.
    scale: f32,
    /// What this side passes on for others.
    relay: Relaying,
    /// The call's chat: every message written here or received (`chat.rs`).
    chat: chat::ChatLog,
    /// Who is on the stage of the speaker view, from the levels of the audio each peer sends.
    speaker: speaker::ActiveSpeaker,
    /// What the call's side panel shows.
    panel: SidePanel,
    /// The chat field as typed.
    chat_draft: String,
    /// The settings screen is open, at this category (`ui::SETTINGS_CATEGORIES`).
    settings_open: bool,
    settings_category: usize,
    /// The devices picked in the settings: an index into "System default" + the microphones /
    /// speakers, and into `CAMERAS` (the camera's facing).
    mic_choice: usize,
    speaker_choice: usize,
    camera_choice: usize,
    /// The video quality this side asks for (`ui::QUALITY_LABELS`): automatic, data saver, low.
    quality: usize,
    /// The app theme (0 flat, 1 flora) and the mode (0 system, 1 light, 2 dark) the settings
    /// show.
    theme_index: usize,
    mode_index: usize,
}

impl MeetState {
    fn new(name: &str, peer_name: &str, backend: &'static str) -> Self {
        MeetState {
            name: name.to_string(),
            peer_name: peer_name.to_string(),
            backend,
            meeting: String::new(),
            endpoint: None,
            guest: None,
            remotes: Vec::new(),
            link_status: String::from("binding"),
            notice: String::new(),
            stats_at_ms: 0,
            stats_shown: Vec::new(),
            pace: pace::PumpPace::new(),
            mic_on: false,
            cam_on: false,
            screen_on: false,
            mic_level: 0.0,
            meter_moved_ms: None,
            meter_bar: None,
            mics: Vec::new(),
            speakers: Vec::new(),
            devices_requested: false,
            room: None,
            deafened: false,
            tone_mic: false,
            tone: None,
            packetizer: audio::Packetizer::new(),
            playout: None,
            play_audio: true,
            clock: std::time::Instant::now(),
            video: VideoSupport {
                encoder: Err(String::from("not probed")),
                decodes_h264: false,
            },
            video_out: BTreeMap::new(),
            pattern_video: false,
            pattern_clocks: [
                video_wire::PatternClock::new(video_wire::PATTERN_FPS),
                video_wire::PatternClock::new(video_wire::PATTERN_FPS),
            ],
            video_debug: false,
            drop_video: BTreeSet::new(),
            me: 0,
            mesh_cap: routes::DEFAULT_MESH_CAP,
            capacity: routes::CapacityEstimator::new(None),
            opted_out: false,
            on_battery: false,
            sync: routes::Sync::default(),
            plan: routes::Plan::default(),
            view: ViewMode::Grid,
            stage_name: String::new(),
            scale: 1.0,
            relay: Relaying::default(),
            chat: chat::ChatLog::new(),
            speaker: speaker::ActiveSpeaker::new(),
            panel: SidePanel::People,
            chat_draft: String::new(),
            settings_open: false,
            settings_category: 0,
            mic_choice: 0,
            speaker_choice: 0,
            camera_choice: 0,
            quality: 0,
            theme_index: 0,
            mode_index: 0,
        }
    }
}

/// What the call's side panel shows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum SidePanel {
    /// Everyone in the call, with their microphone and camera.
    People,
    /// The chat.
    Chat,
    /// The statistics: devices, video, audio, network.
    Statistics,
    /// Nothing: the tiles take the window.
    Closed,
}

/// How the call shows the others.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum ViewMode {
    /// Every camera in an equal grid tile.
    Grid,
    /// One participant on a large stage, the others as thumbnails.
    Speaker,
}

/// What this side passes on for others, as a backbone peer or a leaf's parent.
#[derive(Default)]
struct Relaying {
    /// How far this side ran ahead of a child on a passed-on H.264 stream, by (child connection,
    /// origin, track, height).
    windows: BTreeMap<(u64, u64, u32, u16), video_wire::SendWindow>,
    /// The frame lane of each origin whose frames this side passes on (`routes::frame_track`).
    lanes: BTreeMap<u64, u32>,
    /// When this side last asked upstream for a keyframe of a passed-on stream, by (origin, track,
    /// height).
    asked: BTreeMap<(u64, u32, u16), u64>,
    /// H.264 packets (messages) and audio and JPEG frames passed on, one per child, and keyframe
    /// requests passed on toward their origin.
    packets: u64,
    frames: u64,
    requests: u64,
}

struct Room {
    peers: Vec<RefAny>,
}

const METER_FLOOR_DB: f32 = -60.0;

fn mic_level_percent(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let mean_square = samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32;
    let rms = mean_square.sqrt();
    let db = 20.0 * rms.max(1e-6).log10();
    ((db - METER_FLOOR_DB) / -METER_FLOOR_DB * 100.0).clamp(0.0, 100.0)
}

/// The height of a tile's box until it is laid out, by its role: a gallery tile, the stage, a
/// filmstrip tile (the CallShell's 176 px wide cell at 16:9). A tile asks for the rendition of
/// its height (`IrohTileRole::rendition_height`).
const TILE_H: f32 = 200.0;
const STAGE_H: f32 = 320.0;
const FILMSTRIP_H: f32 = 99.0;

fn track_slot(track: u32) -> Option<usize> {
    match track {
        CAMERA_TRACK => Some(0),
        SCREEN_TRACK => Some(1),
        _ => None,
    }
}

/// The marker of the tile showing `track` of the peer behind connection `handle`.
fn tile_marker(handle: u64, track: u32) -> String {
    format!("azmeet-peer-{handle}-track-{track}")
}

extern "C" fn on_devices_enumerated(
    mut data: RefAny,
    _info: CallbackInfo,
    result: RefAny,
) -> Update {
    let Some(answer) = AudioDeviceListResult::downcast(result).into_option() else {
        return Update::DoNothing;
    };
    let mic_slice: &[AzString] = answer.devices.inputs.as_ref();
    let mics: Vec<String> = mic_slice.iter().map(|s| s.as_str().to_string()).collect();
    let spk_slice: &[AzString] = answer.devices.outputs.as_ref();
    let speakers: Vec<String> = spk_slice.iter().map(|s| s.as_str().to_string()).collect();
    eprintln!(
        "[azmeet] {} mic(s), {} speaker(s) detected",
        mics.len(),
        speakers.len()
    );
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        s.mics = mics;
        s.speakers = speakers;
    }
    Update::RefreshDom
}

extern "C" fn layout_first(data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode makes a light / dark switch rebuild the window.
    let _mode = info.get_mode();
    peer_layout(data, 0)
}

extern "C" fn layout_second(data: RefAny, info: LayoutCallbackInfo) -> Dom {
    let _mode = info.get_mode();
    peer_layout(data, 1)
}

fn room_peer(data: &RefAny, index: usize) -> Option<RefAny> {
    let mut data = data.clone();
    let room = data.downcast_ref::<Room>()?;
    room.peers.get(index).cloned()
}

fn peer_layout(data: RefAny, index: usize) -> Dom {
    match room_peer(&data, index) {
        Some(peer) => meet_layout(peer),
        None => Dom::create_body(),
    }
}

/// The capture consumer cutting `track`'s frames at the `height` rendition (16:9).
fn feed_consumer(track: u32, height: u16) -> FrameConsumer {
    FrameConsumer::create(
        consumer_id(track, height),
        routes::rendition_width(height),
        u32::from(height),
    )
}

/// A capture consumer's id: the track in the low byte, the rendition height above it.
fn consumer_id(track: u32, height: u16) -> u32 {
    (track & 0xff) | u32::from(height) << 8
}

/// The (track, rendition height) of a capture consumer's id.
fn consumer_stream(id: u32) -> (u32, u16) {
    (id & 0xff, (id >> 8) as u16)
}

fn short_id(id: &str) -> &str {
    id.get(..10).unwrap_or(id)
}

/// The name to show for the peer with endpoint id `node_id`.
fn remote_name(s: &MeetState, node_id: &str) -> String {
    if let Some(room) = &s.room {
        return room
            .peers
            .iter()
            .find(|p| p.node_id == node_id)
            .map(|p| p.name.clone())
            .unwrap_or_else(|| short_id(node_id).to_string());
    }
    if s.peer_name.is_empty() {
        short_id(node_id).to_string()
    } else {
        s.peer_name.clone()
    }
}

/// The window's screen: the settings while open, the lobby before a meeting, else the call.
fn ui_screen(s: &MeetState) -> ui::UiScreen {
    if s.settings_open {
        return ui::UiScreen::Settings;
    }
    match &s.room {
        Some(room) if matches!(room.stage, Stage::Start | Stage::Opening) => ui::UiScreen::Lobby,
        _ => ui::UiScreen::Call,
    }
}

/// The people panel: this side, then everyone the meeting server lists (and connected peers it
/// no longer lists), or in the demo everyone connected.
fn people(s: &MeetState) -> Vec<ui::PersonView> {
    let now = now_ms(s);
    let me = my_state(s);
    let mut rows = vec![ui::PersonView {
        name: format!("{} (you)", s.name),
        status: String::from(if s.cam_on { "camera on" } else { "camera off" }),
        muted: me.muted,
        deafened: me.deafened,
        speaking: false,
    }];
    let person = |name: String, status: &str, r: Option<&Remote>| ui::PersonView {
        name,
        status: status.to_string(),
        muted: r.and_then(|r| r.state).is_some_and(|state| state.muted),
        deafened: r.and_then(|r| r.state).is_some_and(|state| state.deafened),
        speaking: r.is_some_and(|r| s.speaker.is_speaking(r.key, now)),
    };
    match &s.room {
        Some(room) => {
            for p in &room.peers {
                let remote = s.remotes.iter().find(|r| r.node_id == p.node_id);
                let status = if remote.is_some() {
                    "connected"
                } else if rooms::dials(&room.node_id, &p.node_id) {
                    "connecting"
                } else {
                    "waiting for them to connect"
                };
                rows.push(person(p.name.clone(), status, remote));
            }
            // Connected peers whose record expired from the server stay listed.
            for r in &s.remotes {
                if !room.peers.iter().any(|p| p.node_id == r.node_id) {
                    rows.push(person(short_id(&r.node_id).to_string(), "connected", Some(r)));
                }
            }
        }
        None => {
            for r in &s.remotes {
                rows.push(person(remote_name(s, &r.node_id), "connected", Some(r)));
            }
        }
    }
    rows
}

/// The view of `tile`: this side's own, or a peer's (its stream's image node once a stream of it
/// arrives, its microphone, whether it speaks).
fn tile_view(s: &MeetState, tile: tiles::Tile, now: u64) -> ui::TileView {
    let track = match tile.kind {
        tiles::TileKind::Camera => CAMERA_TRACK,
        tiles::TileKind::Screen => SCREEN_TRACK,
    };
    if tile.key == s.me {
        return ui::TileView {
            kind: tile.kind,
            me: true,
            name: format!("{} (you)", s.name),
            marker: None,
            muted: !s.mic_on,
            speaking: false,
        };
    }
    let remote = s.remotes.iter().find(|r| r.key == tile.key);
    ui::TileView {
        kind: tile.kind,
        me: false,
        name: remote.map_or_else(|| name_of(s, tile.key), |r| remote_name(s, &r.node_id)),
        marker: remote.and_then(|r| {
            let slot = track_slot(track)?;
            r.tracks[slot].then(|| tile_marker(r.handle, track))
        }),
        muted: remote
            .and_then(|r| r.state)
            .is_some_and(|state| state.muted),
        speaking: s.speaker.is_speaking(tile.key, now),
    }
}

/// The statistics panel: the devices, the video (codec, link, every stream), the audio and the
/// network plan.
fn stats_sections(s: &MeetState) -> Vec<ui::StatSection> {
    let mut video = vec![codec_status(s), s.link_status.clone()];
    video.extend(video_lines(s));
    let source = if s.tone_mic {
        format!("{TONE_HZ} Hz test tone")
    } else {
        String::from("microphone")
    };
    let mut audio = vec![if s.mic_on {
        format!(
            "Sending: {source}, 16-bit PCM, 20 ms packets, {} so far",
            s.packetizer.next_sequence()
        )
    } else {
        String::from("Sending: nothing (muted)")
    }];
    if s.deafened {
        audio.push(String::from("Deafened: nothing is played"));
    }
    audio.extend(audio_lines(s));
    let section = |title: &str, lines: Vec<String>| ui::StatSection {
        title: title.to_string(),
        lines,
    };
    vec![
        section("Microphones", s.mics.clone()),
        section("Speakers", s.speakers.clone()),
        section("Video", video),
        section("Audio", audio),
        section("Network", network_lines(s)),
    ]
}

/// The cameras the settings offer: by facing (there is no camera list API).
const CAMERAS: [&str; 3] = ["Front camera", "Back camera", "External camera"];

/// "System default", then `devices`.
fn device_choices(devices: &[String]) -> Vec<String> {
    let mut choices = vec![String::from("System default")];
    choices.extend(devices.iter().cloned());
    choices
}

/// Everything the window shows, from the state.
fn snapshot(s: &MeetState) -> ui::CallView {
    let now = now_ms(s);
    let arr = arrangement(s);
    let title = match &s.room {
        Some(room) if !room.code.is_empty() => format!("AzMeet · meeting {} · {}", room.code, s.name),
        Some(_) => format!("AzMeet · {}", s.name),
        None if s.endpoint.is_some() => {
            format!("AzMeet · meeting {} · {} ({})", s.meeting, s.name, s.backend)
        }
        None => format!("AzMeet · meeting {}", s.meeting),
    };
    let camera_renditions = my_renditions(s, CAMERA_TRACK);
    let lobby = s.room.as_ref().map(|room| ui::LobbyView {
        opening: room.stage == Stage::Opening,
        server_text: room.server_text.clone(),
        server_status: room.server_status.clone(),
        server_ok: room.server_ok,
        join_text: room.join_text.clone(),
    });
    ui::CallView {
        screen: ui_screen(s),
        title,
        notice: s.notice.clone(),
        name: s.name.clone(),
        lobby,
        stage: arr.stage.map(|tile| tile_view(s, tile, now)),
        tiles: arr.tiles.iter().map(|tile| tile_view(s, *tile, now)).collect(),
        panel: match s.panel {
            SidePanel::People => ui::PanelView::People,
            SidePanel::Chat => ui::PanelView::Chat,
            SidePanel::Statistics => ui::PanelView::Statistics,
            SidePanel::Closed => ui::PanelView::Closed,
        },
        people: people(s),
        chat: s
            .chat
            .messages()
            .iter()
            .map(|m| ui::ChatLine {
                name: m.name.clone(),
                text: m.text.clone(),
                mine: m.mine,
            })
            .collect(),
        chat_unread: s.chat.unread(),
        chat_draft: s.chat_draft.clone(),
        stats: if s.panel == SidePanel::Statistics {
            stats_sections(s)
        } else {
            Vec::new()
        },
        link: s.room.as_ref().map(|room| room.link.clone()).unwrap_or_default(),
        copied: s.room.as_ref().is_some_and(|room| room.copied),
        mic: s.mic_on,
        cam: s.cam_on,
        screen_on: s.screen_on,
        deafened: s.deafened,
        cam_culled: s.cam_on && !s.remotes.is_empty() && camera_renditions.is_empty(),
        speaker_view: s.view == ViewMode::Speaker,
        in_room: s.room.is_some(),
        tone_mic: s.tone_mic,
        pattern_video: s.pattern_video,
        screen_renditions: my_renditions(s, SCREEN_TRACK),
        camera_renditions,
        mic_level: s.mic_level,
        video_debug: s.video_debug,
        settings: ui::SettingsView {
            category: s.settings_category,
            mics: device_choices(&s.mics),
            mic_choice: s.mic_choice,
            speakers: device_choices(&s.speakers),
            speaker_choice: s.speaker_choice,
            cameras: CAMERAS.iter().map(|c| c.to_string()).collect(),
            camera_choice: s.camera_choice,
            quality: s.quality,
            theme: s.theme_index,
            mode: s.mode_index,
            server: s
                .room
                .as_ref()
                .map(|room| room.worker.clone())
                .unwrap_or_else(|| String::from("none (local demo)")),
            name: s.name.clone(),
            codec: codec_status(s),
        },
    }
}

/// The app's callbacks, as the window wires them (`ui::Actions`).
const ACTIONS: ui::Actions = ui::Actions {
    mic: mic_toggle,
    cam: cam_toggle,
    share: screen_toggle,
    deafen: deafen_toggle,
    view: view_toggle,
    leave: on_leave,
    panel: on_panel,
    settings: on_settings_open,
    settings_back: on_settings_back,
    settings_category: on_settings_category,
    copy_link: on_copy_link,
    drop_packet: on_drop_video_packet,
    chat_text: on_chat_text,
    chat_key: on_chat_key,
    chat_send: on_chat_send,
    name_text: on_name_text,
    server_text: on_server_text,
    server_key: on_server_key,
    server_blur: on_server_blur,
    join_text: on_join_text,
    new_meeting: on_new_meeting,
    join: on_join,
    mic_choice: on_mic_choice,
    speaker_choice: on_speaker_choice,
    camera_choice: on_camera_choice,
    quality: on_quality,
    theme: on_theme,
    mode: on_mode,
    key: on_key,
};

fn meet_layout(mut data: RefAny) -> Dom {
    let Some(view) = data.downcast_ref::<MeetState>().map(|s| snapshot(&s)) else {
        return Dom::create_body();
    };

    let first_layout = data
        .downcast_mut::<MeetState>()
        .map(|mut s| {
            let first = !s.devices_requested;
            s.devices_requested = true;
            first
        })
        .unwrap_or(false);
    if first_layout {
        let _request = AudioDeviceList::enumerate(data.clone(), on_devices_enumerated);
    }
    ui::meet_view(&view, &data, &ACTIONS)
}

extern "C" fn meter_mounted(mut data: RefAny, info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        s.meter_bar = Some(info.get_hit_node());
    }
    Update::DoNothing
}

extern "C" fn meter_unmounted(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        s.meter_bar = None;
    }
    Update::DoNothing
}

/// A frame the camera or the screen widget cut for one of its consumers (one per rendition someone
/// shows): sent to whoever gets that rendition.
extern "C" fn send_feed_frame(
    mut data: RefAny,
    _info: CallbackInfo,
    frame: ConsumerFrame,
) -> Update {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        let (track, height) = consumer_stream(frame.consumer.id);
        send_video(&mut s, track, height, frame.frame);
    }
    Update::DoNothing
}

fn apply_link_event(s: &mut MeetState, event: &IrohEvent) -> bool {
    match event.kind {
        IrohEventKind::Ready => {
            eprintln!("[azmeet] {}: iroh endpoint ready", s.name);
            if let Some(room) = s.room.as_mut() {
                room.ticket = event.text.as_str().to_string();
            }
            s.link_status = String::from("waiting for a peer");
            if let Some(guest) = s.guest.take() {
                guest.connect(event.text.clone());
            }
            true
        }
        IrohEventKind::PeerConnected => {
            let node_id = event.text.as_str().to_string();
            if !accepts_peers(s) {
                // Not in a meeting (after Leave): a peer that still dials is refused.
                if let Some(endpoint) = s.endpoint.as_ref() {
                    endpoint.disconnect(event.peer);
                }
                eprintln!(
                    "[azmeet] {}: refused {} (not in a meeting)",
                    s.name,
                    short_id(&node_id)
                );
                return false;
            }
            eprintln!("[azmeet] {}: connected to {}", s.name, short_id(&node_id));
            s.link_status = format!("connected to {}", short_id(&node_id));
            if !s.remotes.iter().any(|r| r.handle == event.peer) {
                s.remotes.push(Remote::new(event.peer, node_id));
            }
            send_state(s, &[event.peer]);
            // What this side decodes and encodes; until the peer's answer arrives it gets JPEG.
            send_caps(s, &[event.peer]);
            // The new peer's tiles change what this side shows: everyone hears the new report.
            network_changed(s, true);
            true
        }
        IrohEventKind::PeerDisconnected => {
            let Some(pos) = s.remotes.iter().position(|r| r.handle == event.peer) else {
                return false;
            };
            let gone = s.remotes.remove(pos);
            drop_audio(s, Some(gone.key));
            s.speaker.forget(gone.key);
            forget_relaying(s, &gone);
            // Dial again on the next poll if the peer is still listed.
            if let Some(room) = s.room.as_mut() {
                room.dialed.remove(&gone.node_id);
            }
            eprintln!(
                "[azmeet] {}: {} disconnected: {}",
                s.name,
                short_id(&gone.node_id),
                event.text.as_str()
            );
            s.link_status = format!("disconnected: {}", event.text.as_str());
            network_changed(s, false);
            true
        }
        IrohEventKind::Error => {
            eprintln!("[azmeet] {}: iroh error: {}", s.name, event.text.as_str());
            s.link_status = format!("error: {}", event.text.as_str());
            true
        }
        IrohEventKind::Message => {
            let Some(audio::Control::State(state)) = audio::decode_control(event.data.as_ref())
            else {
                return false;
            };
            let Some(remote) = s.remotes.iter_mut().find(|r| r.handle == event.peer) else {
                return false;
            };
            if remote.state == Some(state) {
                return false;
            }
            remote.state = Some(state);
            let node_id = remote.node_id.clone();
            let label = format!("{} · connected", remote_name(s, &node_id));
            eprintln!(
                "[azmeet] {}: {}",
                s.name,
                audio::person_line(&label, Some(state))
            );
            true
        }
        _ => false,
    }
}

/// Whether a peer may connect: always in the demo; with a meeting server only in a meeting.
fn accepts_peers(s: &MeetState) -> bool {
    s.room.as_ref().map_or(true, |room| {
        matches!(room.stage, Stage::InRoom | Stage::Ended)
    })
}

/// Shows `picture` in the tile of `track` of the peer behind connection `peer`, once that tile
/// exists (it is laid out after the track's first packet).
fn show_picture(info: &mut TimerCallbackInfo, peer: u64, track: u32, picture: RawImage) {
    let Some(image) = ImageRef::create_rawimage(picture).into_option() else {
        return;
    };
    let marker = tile_marker(peer, track);
    let Some(node) = info
        .callback_info
        .get_node_id_by_marker(AzString::from(marker.as_str()))
        .into_option()
    else {
        return;
    };
    let index = node.node.into_raw();
    if index > 0 {
        info.callback_info.change_node_image(
            node.dom,
            NodeId { inner: index - 1 },
            image,
            UpdateImageType::Content,
        );
    }
}

extern "C" fn pump_link(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let Some(endpoint) = data
        .downcast_ref::<MeetState>()
        .and_then(|s| s.endpoint.clone())
    else {
        return TimerCallbackReturn::continue_unchanged();
    };
    let mut refresh = false;
    // The newest picture of each (peer, track), shown once every event is taken in.
    let mut pictures: BTreeMap<(u64, u32), RawImage> = BTreeMap::new();
    let mut had_events = false;
    while let Some(event) = endpoint.recv().into_option() {
        had_events = true;
        let Some(mut s) = data.downcast_mut::<MeetState>() else {
            continue;
        };
        refresh |= match event.kind {
            IrohEventKind::Frame | IrohEventKind::Message => {
                receive_item(&mut s, &endpoint, &event, &mut pictures)
            }
            _ => apply_link_event(&mut s, &event),
        };
    }
    // The codecs work on their own threads: what they finished since the last pump comes out
    // here - packets to send, pictures to show.
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        drain_decoders(&mut s, &mut pictures);
        drain_encoders(&mut s, &endpoint);
    }
    for ((peer, track), picture) in pictures {
        show_picture(&mut info, peer, track, picture);
    }
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        pump_pattern(&mut s);
    }
    pump_tone(&mut data, &mut info);
    let mut rearm = None;
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        let now = now_ms(&s);
        if !s.remotes.is_empty() && now.saturating_sub(s.stats_at_ms) >= STATS_EVERY_MS {
            s.stats_at_ms = now;
            network_tick(&mut s, &endpoint, &mut info.callback_info);
            let lines: Vec<String> = s
                .remotes
                .iter()
                .map(|r| {
                    let stats = endpoint.peer_stats(r.handle);
                    format!(
                        "{}: {} · RTT {:.1} ms · sent {} · received {} · skipped {}",
                        remote_name(&s, &r.node_id),
                        if stats.direct { "direct" } else { "relayed" },
                        stats.rtt_us as f64 / 1000.0,
                        stats.frames_sent,
                        stats.frames_received,
                        stats.frames_skipped
                    )
                })
                .collect();
            s.link_status = lines.join("; ");
            eprintln!("[azmeet] {}: {}", s.name, s.link_status);
            for line in video_log_lines(&s) {
                eprintln!("[azmeet] {}: {line}", s.name);
            }
            for line in network_lines(&s) {
                eprintln!("[azmeet] {}: {line}", s.name);
            }
            // The statistics show only in their overlay: nothing on screen changes for them
            // while it is closed, or while its text stands still.
            if s.panel == SidePanel::Statistics {
                let shown = stats_lines(&s);
                if shown != s.stats_shown {
                    s.stats_shown = shown;
                    refresh = true;
                }
            }
        }
        // Media flows (or is due) when something arrived, this side sends audio or video to
        // someone, or a stream is being decoded: the pump runs fast; else it slows down.
        let busy = had_events
            || (!s.remotes.is_empty() && (s.mic_on || !my_streams(&s).is_empty()));
        rearm = s.pace.after_pump(busy, now);
    }
    if let Some(interval) = rearm {
        // A timer keeps its interval: a new pace is a new timer, and this one ends.
        let get_time = info.callback_info.get_system_time_fn();
        info.callback_info.add_timer(
            TimerId::unique(),
            Timer::create(data.clone(), pump_link, get_time)
                .with_interval(Duration::System(SystemTimeDiff::from_millis(interval))),
        );
        return if refresh {
            TimerCallbackReturn::terminate_and_refresh_dom()
        } else {
            TimerCallbackReturn::terminate_unchanged()
        };
    }
    if refresh {
        TimerCallbackReturn::continue_and_refresh_dom()
    } else {
        TimerCallbackReturn::continue_unchanged()
    }
}

/// What the statistics overlay shows: the codec, the link, every stream sent and received, the
/// audio, the network plan.
fn stats_lines(s: &MeetState) -> Vec<String> {
    let mut lines = vec![codec_status(s), s.link_status.clone()];
    lines.extend(video_lines(s));
    lines.extend(audio_lines(s));
    lines.extend(network_lines(s));
    lines
}

// ==== Audio: packets out on the audio track, jitter buffers in, a playout thread ====

/// Received audio, shared by the UI thread (which pushes each peer's packets) and the playout
/// thread (which takes one turn from every peer's jitter buffer each 20 ms). Either holds the
/// lock only to move packets, never while a device plays.
struct Playout {
    /// One jitter buffer per origin (a peer's key), whichever way its audio comes.
    peers: BTreeMap<u64, audio::JitterBuffer>,
    /// Play through an `AudioSink` per peer. False in a headless run: the buffers are drained
    /// and counted, and no device is opened.
    play: bool,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Starts the playout thread. It ends once the returned handle (kept in `MeetState`) is gone.
fn start_playout(play: bool) -> Arc<Mutex<Playout>> {
    let shared = Arc::new(Mutex::new(Playout {
        peers: BTreeMap::new(),
        play,
    }));
    let weak = Arc::downgrade(&shared);
    let spawned = std::thread::Builder::new()
        .name(String::from("azmeet-playout"))
        .spawn(move || playout_loop(weak));
    if let Err(e) = spawned {
        eprintln!("[azmeet] could not start the audio playout thread: {e}");
    }
    shared
}

/// Every 20 ms, one turn (a packet, or silence) from each peer's jitter buffer, played through
/// that peer's own `AudioSink`, opened at the peer's rate, so the system mixes the peers. A sink's
/// `play` may block (an ALSA write does); that paces this thread, never the UI thread.
fn playout_loop(shared: Weak<Mutex<Playout>>) {
    let started = std::time::Instant::now();
    let mut clock = audio::PlayoutClock::new();
    let mut sinks: BTreeMap<u64, (u32, AudioSink)> = BTreeMap::new();
    loop {
        let turns = clock.due(started.elapsed().as_millis() as u64);
        let mut out: Vec<(u64, u32, Vec<i16>)> = Vec::new();
        let (play, live) = {
            let Some(strong) = shared.upgrade() else {
                return;
            };
            let mut playout = lock(&strong);
            for _ in 0..turns {
                for (handle, jitter) in playout.peers.iter_mut() {
                    if let Some(samples) = jitter.pop() {
                        out.push((*handle, jitter.sample_rate(), samples));
                    }
                }
            }
            let play = playout.play;
            let live: BTreeSet<u64> = playout.peers.keys().copied().collect();
            (play, live)
        };
        // Close the sinks of peers that are gone, outside the lock.
        sinks.retain(|handle, _| live.contains(handle));
        if play {
            for (handle, rate, samples) in out {
                let reopen = sinks
                    .get(&handle)
                    .map_or(true, |(open_rate, _)| *open_rate != rate);
                if reopen {
                    let sink = AudioSink::open(AudioConfig {
                        sample_rate: rate,
                        channels: 1,
                    });
                    eprintln!(
                        "[azmeet] playing connection {handle} at {rate} Hz: {}",
                        if sink.is_open() {
                            "output open"
                        } else {
                            "no audio output"
                        }
                    );
                    sinks.insert(handle, (rate, sink));
                }
                if let Some((_, sink)) = sinks.get(&handle) {
                    sink.play(AudioFrame {
                        sample_rate: rate,
                        channels: 1,
                        samples: F32Vec::from(audio::pcm_to_f32(&samples)),
                    });
                }
            }
        }
        let wait = clock.wait_ms(started.elapsed().as_millis() as u64);
        std::thread::sleep(std::time::Duration::from_millis(wait.max(1)));
    }
}

/// Whether this run may open devices (microphone, speakers, camera, screen): not under
/// `AZ_BACKEND=headless`, where a test must never capture or play anything real.
fn devices_allowed() -> bool {
    std::env::var("AZ_BACKEND").map_or(true, |backend| {
        !backend.trim().eq_ignore_ascii_case("headless")
    })
}

/// The audio settings of this run: `AZMEET_TEST_TONE=1` makes the microphone a tone and starts
/// it unmuted; a headless run uses the tone too (muted until switched on) and plays nothing.
fn configure_audio(s: &mut MeetState) {
    let devices = devices_allowed();
    let tone = std::env::var("AZMEET_TEST_TONE").is_ok_and(|v| v.trim() == "1");
    s.tone_mic = tone || !devices;
    s.play_audio = devices;
    if tone {
        s.mic_on = true;
    }
    sync_mic(s);
    if !devices {
        eprintln!(
            "[azmeet] {}: no audio device is opened: the microphone is a {TONE_HZ} Hz test tone and \
             received audio is counted, not played",
            s.name
        );
    } else if tone {
        eprintln!(
            "[azmeet] {}: the microphone is a {TONE_HZ} Hz test tone (AZMEET_TEST_TONE=1)",
            s.name
        );
    }
}

/// Starts or stops the test tone with the microphone; a mute forgets the unfinished packet.
fn sync_mic(s: &mut MeetState) {
    if !s.mic_on {
        s.packetizer.reset();
        s.tone = None;
    } else if s.tone_mic && s.tone.is_none() {
        s.tone = Some((
            audio::ToneSource::new(TONE_HZ, MIC_RATE),
            std::time::Instant::now(),
        ));
    }
}

/// Sends captured audio as 20 ms packets on the audio track to whom the plan says: everyone in the
/// full mesh, the backbone parent for a leaf.
fn send_audio(s: &mut MeetState, sample_rate: u32, channels: u16, samples: &[f32]) {
    if !s.mic_on {
        return;
    }
    if s.remotes.is_empty() {
        // Nobody listens: the first frame to the next peer starts with fresh audio.
        s.packetizer.reset();
        return;
    }
    let Some(endpoint) = s.endpoint.clone() else {
        return;
    };
    let targets = handles_of(s, &s.plan.children(s.me, s.me));
    for frame in s.packetizer.push(sample_rate, channels, samples) {
        for handle in &targets {
            endpoint.send_frame(*handle, AUDIO_TRACK, frame.clone());
        }
    }
}

/// Takes a frame of `origin`'s audio (from it directly, or passed on) into its jitter buffer.
/// Returns whether the active speaker changed (the speaker view's stage).
fn receive_audio(s: &mut MeetState, origin: u64, bytes: &[u8]) -> bool {
    if !s.remotes.iter().any(|r| r.key == origin) {
        return false;
    }
    let Some(wire) = audio::decode_frame(bytes) else {
        return false;
    };
    // Who speaks: the newest packet's level (a frame repeats the two before it). Measured even
    // while deafened, so the stage still follows the conversation.
    let now = now_ms(s);
    let level = wire
        .packets
        .last()
        .map_or(speaker::SILENCE_DB, |packet| speaker::level_db(&packet.samples));
    let stage_moved = s.speaker.observe(origin, level, now);
    if s.deafened {
        return stage_moved;
    }
    let play = s.play_audio;
    let shared = s.playout.get_or_insert_with(|| start_playout(play));
    let mut playout = lock(shared);
    let jitter = playout
        .peers
        .entry(origin)
        .or_insert_with(|| audio::JitterBuffer::new(audio::TARGET_PACKETS, audio::MAX_PACKETS));
    for packet in wire.packets {
        jitter.push(wire.sample_rate, packet);
    }
    stage_moved
}

/// The active speaker changed: in the speaker view the stage (and with it what this side asks
/// for) moves; in the gallery nothing on screen does. True when the window changes.
fn speaker_moved(s: &mut MeetState) -> bool {
    if let Some(key) = s.speaker.current() {
        eprintln!("[azmeet] {}: {} is speaking", s.name, name_of(s, key));
    }
    if s.view != ViewMode::Speaker {
        return false;
    }
    for r in s.remotes.iter_mut() {
        // Until the tiles are laid out again, their new boxes' heights count.
        r.tile_height = [None; 2];
        r.tile_px = [None; 2];
    }
    network_changed(s, false);
    true
}

/// Forgets the received audio of the peer with key `origin` (it left), or everyone's.
fn drop_audio(s: &MeetState, origin: Option<u64>) {
    let Some(shared) = s.playout.as_ref() else {
        return;
    };
    let mut playout = lock(shared);
    match origin {
        Some(origin) => {
            playout.peers.remove(&origin);
        }
        None => playout.peers.clear(),
    }
}

/// This participant's audio state, as its control message tells the others.
fn my_state(s: &MeetState) -> audio::PeerState {
    audio::PeerState {
        muted: !s.mic_on,
        deafened: s.deafened,
    }
}

/// Sends `message` to the peers behind `handles`, reliably and in order.
fn send_message_to(s: &MeetState, handles: &[u64], message: &[u8]) {
    let Some(endpoint) = s.endpoint.as_ref() else {
        return;
    };
    for handle in handles {
        endpoint.send_message(*handle, message.to_vec());
    }
}

/// The connection handles of every connected peer.
fn all_peers(s: &MeetState) -> Vec<u64> {
    s.remotes.iter().map(|r| r.handle).collect()
}

/// Tells the peers behind `handles` this participant's audio state.
fn send_state(s: &MeetState, handles: &[u64]) {
    send_message_to(s, handles, &audio::encode_state(my_state(s)));
}

fn send_state_to_all(s: &MeetState) {
    send_state(s, &all_peers(s));
}

// ==== Chat: one reliable message to every peer (the rules are in chat.rs) ====

/// This side writes `text` in the chat: listed, and sent to every connected peer. False for an
/// empty message.
fn send_chat(s: &mut MeetState, text: &str) -> bool {
    let now = now_ms(s);
    let Some(bytes) = s.chat.compose(s.me, &s.name, text, now) else {
        return false;
    };
    send_message_to(s, &all_peers(s), &bytes);
    true
}

/// A chat message from the peer with key `from`: listed (and printed for scripts as
/// `AZMEET_CHAT <name>: <text>`). True when the window changes.
fn receive_chat(s: &mut MeetState, from: u64, bytes: &[u8]) -> bool {
    let name = name_of(s, from);
    let open = s.panel == SidePanel::Chat;
    if !s.chat.receive(from, &name, bytes, open) {
        return false;
    }
    if let Some(message) = s.chat.messages().last() {
        println!("AZMEET_CHAT {}: {}", message.name, message.text);
        eprintln!("[azmeet] {}: chat from {}: {}", s.name, message.name, message.text);
    }
    true
}

/// One line per connected peer whose audio arrived: what its jitter buffer took in and played,
/// whether it waits to fill up (the peer went quiet), and who passes it on.
fn audio_lines(s: &MeetState) -> Vec<String> {
    let Some(shared) = s.playout.as_ref() else {
        return Vec::new();
    };
    let playout = lock(shared);
    let lines: Vec<String> = s
        .remotes
        .iter()
        .filter_map(|r| {
            let jitter = playout.peers.get(&r.key)?;
            let mut line = audio::audio_line(
                &remote_name(s, &r.node_id),
                &jitter.stats(),
                jitter.buffered(),
            );
            if !jitter.is_playing() {
                line.push_str(", filling up");
            }
            if let Some(via) = s.plan.parent(s.me, r.key).filter(|p| *p != r.key) {
                line.push_str(&format!(", via {}", name_of(s, via)));
            }
            Some(line)
        })
        .collect();
    lines
}

/// How often the level meter may move: 10 times a second reads as live, and each move repaints
/// the meter (50 moves a second, one per audio chunk, repainted it for every chunk).
const METER_INTERVAL_MS: u64 = 100;

/// The level meter's new value, when it moved by half a percent or more, the meter is shown, and
/// it last moved [`METER_INTERVAL_MS`] or more ago.
fn meter_change(s: &mut MeetState, samples: &[f32]) -> Option<(DomNodeId, f32)> {
    let level = mic_level_percent(samples).round();
    if (s.mic_level - level).abs() < 0.5 {
        return None;
    }
    let now = now_ms(s);
    if s
        .meter_moved_ms
        .is_some_and(|at| now.saturating_sub(at) < METER_INTERVAL_MS)
    {
        return None;
    }
    s.meter_moved_ms = Some(now);
    s.mic_level = level;
    s.meter_bar.map(|bar| (bar, level))
}

fn show_level(mut info: CallbackInfo, bar: DomNodeId, level: f32) {
    ProgressBar::update_progress(info, bar, level);
    info.set_accessibility_value(bar, format!("{level:.0}%"));
}

/// Every pump while the microphone is the tone: the samples due since the last pump, sent like
/// microphone audio.
fn pump_tone(data: &mut RefAny, info: &mut TimerCallbackInfo) {
    let meter = data.downcast_mut::<MeetState>().and_then(|mut guard| {
        let s = &mut *guard;
        let (source, started) = s.tone.as_mut()?;
        let samples = source.take(started.elapsed().as_millis() as u64);
        let rate = source.sample_rate();
        if samples.is_empty() {
            return None;
        }
        send_audio(s, rate, 1, &samples);
        meter_change(s, &samples)
    });
    if let Some((bar, level)) = meter {
        show_level(info.callback_info, bar, level);
    }
}

// ==== Video: H.264 where an encoder works, JPEG otherwise (the rules are in video_wire.rs) ====

/// What this machine does with H.264, found once at start by [`probe_video`].
#[derive(Clone)]
struct VideoSupport {
    /// The H.264 encoder's backend ("VideoToolbox") when it encodes here; else why JPEG is sent.
    encoder: Result<String, String>,
    /// This machine decodes H.264: tested with a keyframe where it also encodes, else the
    /// platform's word, judged again on the first stream (`ReceiveTrack::decoder_is_inert`).
    decodes_h264: bool,
}

/// The sending side of one local video track (the camera or the screen share).
struct VideoOut {
    /// The H.264 encoder, opened with the first frame some peer takes as H.264.
    encoder: Option<VideoEncoder>,
    /// The frame size the encoder was opened for.
    size: (u32, u32),
    health: video_wire::EncoderHealth,
    keyframes: video_wire::KeyframePolicy,
    /// The `seq` of the last H.264 packet and of the last JPEG frame; the `frame_no` of the last
    /// frame captured while someone listened. They go on across a stop, so a peer sees one stream.
    h264_seq: u32,
    jpeg_seq: u32,
    frame_no: u32,
    h264_packets: u64,
    jpeg_frames: u64,
    /// Packets dropped with the "Drop a video packet" button.
    dropped: u64,
    /// The `frame_no` of every frame in the encoder whose packet has not come out yet, oldest
    /// first (the encoder works on its own thread).
    in_encoder: std::collections::VecDeque<u32>,
}

impl VideoOut {
    fn new() -> Self {
        VideoOut {
            encoder: None,
            size: (0, 0),
            health: video_wire::EncoderHealth::default(),
            keyframes: video_wire::KeyframePolicy::new(),
            h264_seq: 0,
            jpeg_seq: 0,
            frame_no: 0,
            h264_packets: 0,
            jpeg_frames: 0,
            dropped: 0,
            in_encoder: std::collections::VecDeque::new(),
        }
    }

    /// The track stopped (camera or share off, or Leave): the encoder closes, and the next start
    /// opens a fresh one, which begins with a keyframe.
    fn stop(&mut self) {
        self.encoder = None;
        self.in_encoder.clear();
        self.health = video_wire::EncoderHealth::default();
    }
}

/// The receiving side of one of a peer's video streams (a track in one rendition).
struct VideoIn {
    rules: video_wire::ReceiveTrack,
    /// Opened with the first H.264 packet to decode.
    decoder: Option<VideoDecoder>,
    /// The codec of the packet that arrived last.
    seen: Option<Codec>,
    /// The connection the last packet came through: the origin's own, or a forwarder's.
    via: u64,
    /// This side shows the stream; else it only passes it on.
    shown: bool,
    /// The size the decoder was last asked to hand frames out at (the tile's device pixels).
    output_size: Option<(u32, u32)>,
}

impl VideoIn {
    fn new() -> Self {
        VideoIn {
            rules: video_wire::ReceiveTrack::new(),
            decoder: None,
            seen: None,
            via: 0,
            shown: false,
            output_size: None,
        }
    }
}

/// Milliseconds on this participant's clock.
fn now_ms(s: &MeetState) -> u64 {
    s.clock.elapsed().as_millis() as u64
}

/// A video frame as an image to encode or show, in its own format (NV12,
/// BGRA8 or RGBA8). Video is opaque, for which straight == premultiplied: no
/// per-pixel multiply when it is loaded, no conversion.
fn frame_image(frame: VideoFrame) -> RawImage {
    RawImage {
        pixels: RawImageData::U8(frame.bytes),
        width: frame.width as usize,
        height: frame.height as usize,
        premultiplied_alpha: true,
        data_format: frame.format,
        tag: U8Vec::create(),
    }
}

/// Frame `index` of the test pattern, `width` x `height`.
fn pattern_frame(index: u32, width: u32, height: u32) -> VideoFrame {
    VideoFrame {
        width,
        height,
        bytes: U8Vec::from(video_wire::test_pattern(width, height, index)),
        format: RawImageFormat::RGBA8,
    }
}

/// Finds out once, at start, what this machine does with H.264: an encoder that opens must turn
/// a test frame into a keyframe, and a decoder must turn that keyframe back into a picture.
/// `AZMEET_VIDEO_CODEC=jpeg` switches H.264 off.
fn probe_video() -> VideoSupport {
    if std::env::var("AZMEET_VIDEO_CODEC").is_ok_and(|v| v.trim().eq_ignore_ascii_case("jpeg")) {
        return VideoSupport {
            encoder: Err(String::from("H.264 switched off")),
            decodes_h264: false,
        };
    }
    let backend = VideoEncoder::backend_name().as_str().to_string();
    let keyframe = if backend == "none" {
        None
    } else {
        probe_encode()
    };
    let decodes_h264 = match &keyframe {
        Some(keyframe) => probe_decode(keyframe),
        None => PlatformCapability::video_codec().available,
    };
    VideoSupport {
        encoder: keyframe
            .map(|_| backend)
            .ok_or_else(|| String::from("no encoder")),
        decodes_h264,
    }
}

/// A keyframe of the test pattern from a fresh H.264 encoder; `None` where nothing, or no
/// keyframe, comes out. `VideoEncoder::open` hands out an open handle that never yields a packet
/// where no backend is built in, so opening proves nothing.
fn probe_encode() -> Option<Vec<u8>> {
    let mut encoder = VideoEncoder::open(FEED_W, FEED_H, false, VIDEO_KBPS);
    if !encoder.is_open() {
        return None;
    }
    let mut chunk = Vec::new();
    for index in 0..3 {
        encoder.encode(pattern_frame(index, FEED_W, FEED_H), true);
        // The encoder works on its own thread: wait for this frame's packets.
        encoder.flush();
        while let Some(packet) = encoder.recv_packet().into_option() {
            chunk.extend_from_slice(packet.as_slice());
        }
        if !chunk.is_empty() {
            break;
        }
    }
    encoder.close();
    video_wire::h264_is_keyframe(&chunk).then_some(chunk)
}

/// Whether a fresh decoder turns `keyframe` back into a picture.
fn probe_decode(keyframe: &[u8]) -> bool {
    let mut decoder = VideoDecoder::open(false);
    if !decoder.is_open() {
        return false;
    }
    decoder.set_output_format(VIDEO_FORMAT);
    decoder.decode(U8Vec::from(keyframe.to_vec()));
    // The decoder works on its own thread: `flush` waits for the picture.
    let decoded = decoder.flush().into_option().is_some();
    decoder.close();
    decoded
}

/// The video settings of this run: `AZMEET_TEST_PATTERN=1` makes the camera a test pattern,
/// switches it on, and shows the "Drop a video packet" button; a headless run uses test patterns
/// too (off until switched on), so it never opens a camera or a screen.
fn configure_video(s: &mut MeetState, support: &VideoSupport) {
    let pattern = std::env::var("AZMEET_TEST_PATTERN").is_ok_and(|v| v.trim() == "1");
    let devices = devices_allowed();
    s.video = support.clone();
    s.pattern_video = pattern || !devices;
    s.video_debug = pattern;
    if pattern {
        s.cam_on = true;
    }
    eprintln!(
        "[azmeet] {}: {}; decodes H.264: {}",
        s.name,
        codec_status(s),
        if s.video.decodes_h264 { "yes" } else { "no" }
    );
    if !devices {
        eprintln!(
            "[azmeet] {}: no camera or screen is opened: both are test patterns",
            s.name
        );
    } else if pattern {
        eprintln!(
            "[azmeet] {}: the camera is a test pattern (AZMEET_TEST_PATTERN=1)",
            s.name
        );
    }
}

/// The codec line: H.264 and its backend (and JPEG to the peers that cannot decode it), or why
/// JPEG.
fn codec_status(s: &MeetState) -> String {
    let jpeg_to: Vec<String> = s
        .remotes
        .iter()
        .filter(|r| r.h264 == Some(false))
        .map(|r| remote_name(s, &r.node_id))
        .collect();
    video_wire::codec_line(s.video.encoder.as_deref().map_err(String::as_str), &jpeg_to)
}

/// Sends a captured frame (NV12 from the camera or the screen, RGBA from the test pattern) of the
/// `rendition` of `track` to whom the plan says gets it: as H.264 to those assigned H.264
/// (reliable messages, so nothing between two keyframes goes missing), as JPEG to the others (a
/// latest-wins frame each: every JPEG stands alone). A leaf sends each rendition once, to its
/// backbone parent. The frame is handed to the encoder as it is (moved, not copied, unless JPEG
/// peers need it too).
fn send_video(s: &mut MeetState, track: u32, rendition: u16, frame: VideoFrame) {
    let (width, frame_height) = (frame.width, frame.height);
    if track_slot(track).is_none() || s.remotes.is_empty() {
        return;
    }
    let Some(endpoint) = s.endpoint.clone() else {
        return;
    };
    let mut h264_peers = stream_targets(s, track, rendition, true);
    let mut jpeg_peers = stream_targets(s, track, rendition, false);
    if h264_peers.is_empty() && jpeg_peers.is_empty() {
        return;
    }
    // H.264 wants even sides; 16 pixels is the smallest VideoToolbox takes.
    let fits = width % 2 == 0 && frame_height % 2 == 0 && width >= 16 && frame_height >= 16;
    if !fits || s.video.encoder.is_err() {
        jpeg_peers.append(&mut h264_peers);
    }
    let out = s
        .video_out
        .entry((track, rendition))
        .or_insert_with(VideoOut::new);
    out.frame_no = out.frame_no.wrapping_add(1);
    // The encoder takes the frame by value: it is moved there, and copied
    // only when JPEG peers need it as well.
    let mut frame = Some(frame);
    if !h264_peers.is_empty() {
        let for_encoder = if jpeg_peers.is_empty() {
            frame.take()
        } else {
            frame.clone()
        };
        let sent = for_encoder
            .map(|f| send_h264(s, &endpoint, track, rendition, f, &h264_peers))
            .unwrap_or(false);
        if !sent {
            // No working encoder for this frame: these peers get JPEG (from the next frame on
            // when this one went into the encoder that just failed).
            jpeg_peers.extend(h264_peers);
        }
    }
    if !jpeg_peers.is_empty() {
        if let Some(frame) = frame {
            send_jpeg(s, &endpoint, track, rendition, frame, &jpeg_peers);
        }
    }
}

/// Hands the frame to the H.264 encoder of the `rendition` of `track` (it encodes on its own
/// thread) and sends whatever packets it has ready to the peers that get the stream
/// ([`drain_h264`]). `peers` (connection handles) are this frame's H.264 receivers: a new one, or
/// one that fell behind, makes the frame a keyframe. False when no encoder works (it did not
/// open, gives nothing back, or ignores keyframe requests): H.264 is given up, the frame goes as
/// JPEG, and everyone hears that this side no longer encodes it.
fn send_h264(
    s: &mut MeetState,
    endpoint: &IrohEndpoint,
    track: u32,
    rendition: u16,
    frame: VideoFrame,
    peers: &[u64],
) -> bool {
    let (width, height) = (frame.width, frame.height);
    let key = (track, rendition);
    let now = now_ms(s);
    // A new peer, or one that fell behind, starts at a keyframe.
    let resync = s.remotes.iter().any(|r| {
        peers.contains(&r.handle) && r.sent.get(&key).map_or(true, |w| w.wants_keyframe())
    });
    let opened = {
        let Some(out) = s.video_out.get_mut(&key) else {
            return false;
        };
        if resync {
            out.keyframes.request();
        }
        let resized = out.encoder.is_some() && out.size != (width, height);
        if out.keyframes.must_reopen() || resized {
            out.encoder = None;
            out.in_encoder.clear();
            out.keyframes.reopened();
        }
        if out.encoder.is_none() {
            let kbps = IrohLoadBalancer::rendition_kbps(u32::from(rendition));
            let encoder = VideoEncoder::open(width, height, false, kbps);
            if encoder.is_open() {
                out.encoder = Some(encoder);
                out.size = (width, height);
                out.health = video_wire::EncoderHealth::default();
                // A new encoder begins with a keyframe.
                out.keyframes.request();
            }
        }
        let frame_no = out.frame_no;
        match out.encoder.as_ref() {
            None => false,
            Some(encoder) => {
                let force = out.keyframes.should_force(now);
                if encoder.encode(frame, force) {
                    out.health.submitted();
                    out.in_encoder.push_back(frame_no);
                } else {
                    // The encoder still works on earlier frames: this one is dropped, and a
                    // keyframe it was to carry is forced on the next frame it takes.
                    out.keyframes.not_taken();
                }
                true
            }
        }
    };
    if !opened {
        give_up_h264(s, "the H.264 encoder did not open");
        return false;
    }
    drain_h264(s, endpoint, key)
}

/// Sends the packets the encoder of `key` (track, rendition height) has ready to the peers that
/// get that stream as H.264 now (each through its [`video_wire::SendWindow`]), numbered in the
/// order they come out. Called after every frame handed to it, and on every pump: the encoder
/// works on its own thread, so a frame's packets come out a little later. False when the
/// encoder turned out not to work (see [`send_h264`]).
fn drain_h264(s: &mut MeetState, endpoint: &IrohEndpoint, key: (u32, u16)) -> bool {
    let (track, rendition) = key;
    let now = now_ms(s);
    let (packets, failure) = {
        let Some(out) = s.video_out.get_mut(&key) else {
            return true;
        };
        let Some(encoder) = out.encoder.as_mut() else {
            return true;
        };
        let mut packets = Vec::new();
        while let Some(chunk) = encoder.recv_packet().into_option() {
            out.health.produced();
            let keyframe = video_wire::h264_is_keyframe(chunk.as_slice());
            out.keyframes.on_output(keyframe, now);
            out.h264_seq = out.h264_seq.wrapping_add(1);
            let frame_no = out.in_encoder.pop_front().unwrap_or(out.frame_no);
            let header = video_wire::Header {
                codec: Codec::H264,
                keyframe,
                track,
                seq: out.h264_seq,
                frame_no,
                height: rendition,
            };
            packets.push((header, video_wire::encode_packet(&header, chunk.as_slice())));
        }
        let failure = if out.health.is_inert() {
            Some("the H.264 encoder gives no packets back")
        } else if out.keyframes.is_broken() {
            Some("the H.264 encoder ignores keyframe requests")
        } else {
            None
        };
        (packets, failure)
    };
    if let Some(why) = failure {
        give_up_h264(s, why);
        return false;
    }
    if packets.is_empty() {
        return true;
    }
    let peers = stream_targets(s, track, rendition, true);
    for (header, packet) in packets {
        if !header.keyframe && s.drop_video.remove(&key) {
            if let Some(out) = s.video_out.get_mut(&key) {
                out.dropped += 1;
            }
            eprintln!(
                "[azmeet] {}: dropped H.264 packet {} of the {} on purpose",
                s.name,
                header.seq,
                rendition_label(track, rendition)
            );
            continue;
        }
        if let Some(out) = s.video_out.get_mut(&key) {
            out.h264_packets += 1;
        }
        for r in s.remotes.iter_mut().filter(|r| peers.contains(&r.handle)) {
            let window = r
                .sent
                .entry(key)
                .or_insert_with(video_wire::SendWindow::new);
            if window.offer(header.seq, header.keyframe) {
                endpoint.send_message(r.handle, packet.clone());
            }
        }
    }
    true
}

/// Every pump: the packets every encoder made since the last one go out.
fn drain_encoders(s: &mut MeetState, endpoint: &IrohEndpoint) {
    let keys: Vec<(u32, u16)> = s
        .video_out
        .iter()
        .filter(|(_, out)| out.encoder.is_some())
        .map(|(key, _)| *key)
        .collect();
    for key in keys {
        drain_h264(s, endpoint, key);
    }
}

/// No H.264 encoder works here (`why`): every encoder closes, JPEG from now on, and every peer
/// hears that this side no longer encodes H.264.
fn give_up_h264(s: &mut MeetState, why: &str) {
    eprintln!("[azmeet] {}: {why}: sending JPEG from now on", s.name);
    for out in s.video_out.values_mut() {
        out.stop();
    }
    s.video.encoder = Err(String::from(why));
    send_caps(s, &all_peers(s));
}

/// The connection handles that get this side's `rendition` of `track` as H.264 (`h264`) or as
/// JPEG, by the current plan: everyone assigned that stream in the full mesh, the backbone parent
/// for a leaf.
fn stream_targets(s: &MeetState, track: u32, rendition: u16, h264: bool) -> Vec<u64> {
    let assigned = assignment(s, s.me, track);
    let stream = routes::Stream {
        height: rendition,
        h264,
    };
    handles_of(s, &s.plan.next_hops(s.me, s.me, stream, &assigned))
}

/// Sends the frame as JPEG to `peers`, a latest-wins frame each, on the rendition's own frame track.
fn send_jpeg(
    s: &mut MeetState,
    endpoint: &IrohEndpoint,
    track: u32,
    rendition: u16,
    frame: VideoFrame,
    peers: &[u64],
) {
    let key = (track, rendition);
    // `encode_jpeg` takes any frame format (NV12 is converted to RGB for it).
    let ResultU8VecEncodeImageError::Ok(jpeg) = frame_image(frame).encode_jpeg(JPEG_QUALITY)
    else {
        return;
    };
    let header = {
        let Some(out) = s.video_out.get_mut(&key) else {
            return;
        };
        out.jpeg_seq = out.jpeg_seq.wrapping_add(1);
        video_wire::Header {
            codec: Codec::Jpeg,
            keyframe: true,
            track,
            seq: out.jpeg_seq,
            frame_no: out.frame_no,
            height: rendition,
        }
    };
    if s.drop_video.remove(&key) {
        if let Some(out) = s.video_out.get_mut(&key) {
            out.dropped += 1;
        }
        eprintln!(
            "[azmeet] {}: dropped JPEG frame {} of the {} on purpose",
            s.name,
            header.seq,
            rendition_label(track, rendition)
        );
        return;
    }
    if let Some(out) = s.video_out.get_mut(&key) {
        out.jpeg_frames += 1;
    }
    let packet = video_wire::encode_packet(&header, jpeg.as_slice());
    let frame_track = routes::frame_track(0, track, rendition);
    for handle in peers {
        endpoint.send_frame(*handle, frame_track, packet.clone());
    }
}

/// A video packet of `origin`'s stream that reached this side through connection `via` (the
/// origin's own, or a forwarder's): through the stream's rules, which acknowledge it and ask for
/// keyframes back the way it came, then, when this side shows that rendition, its decoder; the
/// newest picture goes into `pictures`. A stream this side only passes on is followed the same way
/// (the sender's window waits for the acknowledgements) but not decoded. Returns whether a tile
/// appeared.
fn take_video(
    s: &mut MeetState,
    endpoint: &IrohEndpoint,
    origin: u64,
    via: u64,
    header: &video_wire::Header,
    payload: &[u8],
    pictures: &mut BTreeMap<(u64, u32), RawImage>,
) -> bool {
    let Some(slot) = track_slot(header.track) else {
        return false;
    };
    let now = now_ms(s);
    let mine = shows(s, origin, header.track, header.height);
    let Some(remote) = s.remotes.iter_mut().find(|r| r.key == origin) else {
        return false;
    };
    let handle = remote.handle;
    let tile_px = remote.tile_px[slot];
    let new_tile = mine && !remote.tracks[slot];
    if mine {
        remote.tracks[slot] = true;
    }
    let input = remote
        .received
        .entry((header.track, header.height))
        .or_insert_with(VideoIn::new);
    if mine && !input.shown {
        // Shown from now on: a fresh decoder, which starts at a keyframe.
        input.rules = video_wire::ReceiveTrack::new();
        input.decoder = None;
    }
    input.shown = mine;
    input.seen = Some(header.codec);
    input.via = via;
    let verdict = input.rules.on_packet(header, now);
    if verdict.restart {
        input.decoder = None;
    }
    let mut controls = Vec::new();
    if let Some(seq) = verdict.ack {
        controls.push(video_wire::encode_received(
            header.track,
            seq,
            header.height,
        ));
    }
    if verdict.request_keyframe {
        controls.push(video_wire::encode_keyframe_request(
            header.track,
            header.height,
        ));
    }
    let picture = if mine && verdict.decode {
        decode_picture(input, header.codec, payload, tile_px)
    } else {
        None
    };
    let inert = mine && input.rules.decoder_is_inert();
    if inert {
        input.decoder = None;
    }
    for control in controls {
        send_control_up(s, endpoint, origin, via, header.track, &control);
    }
    if let Some(picture) = picture {
        pictures.insert((handle, header.track), picture);
    }
    if inert && s.video.decodes_h264 {
        s.video.decodes_h264 = false;
        eprintln!(
            "[azmeet] {}: the H.264 decoder gives no pictures back: asking everyone for JPEG",
            s.name
        );
        send_caps(s, &all_peers(s));
    }
    new_tile
}

/// Decodes one packet of a shown stream: a JPEG's picture at once; an H.264 packet goes to the
/// stream's decoder, whose picture comes out on a later pump ([`drain_decoders`]), as NV12 at the
/// size of the tile that shows it (`tile_px`, device pixels): scaled once, on the GPU, and never
/// converted to RGB on the CPU.
fn decode_picture(
    input: &mut VideoIn,
    codec: Codec,
    payload: &[u8],
    tile_px: Option<(u32, u32)>,
) -> Option<RawImage> {
    let (picture, pictures) = match codec {
        Codec::Jpeg => match RawImage::decode_image_bytes_any(U8VecRef::from(payload)) {
            ResultRawImageDecodeImageError::Ok(image) => (Some(image), 1),
            _ => (None, 0),
        },
        Codec::H264 => {
            if input.decoder.is_none() {
                // A fresh decoder (the first packet, a restart, a stream shown again) knows no
                // size yet: the one the previous decoder was told is not its own, and keeping it
                // left every restarted stream at the stream's size, scaled by the renderer.
                input.output_size = None;
            }
            let decoder = input.decoder.get_or_insert_with(|| {
                let decoder = VideoDecoder::open(false);
                decoder.set_output_format(VIDEO_FORMAT);
                decoder
            });
            if input.output_size != tile_px {
                // Applied by the decoder at the stream's next keyframe.
                let (w, h) = tile_px.unwrap_or((0, 0));
                decoder.set_output_size(w, h);
                input.output_size = tile_px;
            }
            // Decoded on the decoder's own thread: the picture comes out on a later pump
            // (`drain_decoders`).
            decoder.decode(U8Vec::from(payload.to_vec()));
            (None, 0)
        }
    };
    input.rules.decoded(pictures);
    picture
}

/// Every pump: the pictures every decoder finished since the last one (they decode on their own
/// threads); the newest of each shown stream goes onto its tile.
fn drain_decoders(s: &mut MeetState, pictures: &mut BTreeMap<(u64, u32), RawImage>) {
    for r in s.remotes.iter_mut() {
        let handle = r.handle;
        for ((track, _), input) in r.received.iter_mut() {
            if !input.shown {
                continue;
            }
            let Some(decoder) = input.decoder.as_mut() else {
                continue;
            };
            let mut newest = None;
            let mut count = 0;
            while let Some(frame) = decoder.recv_frame().into_option() {
                count += 1;
                newest = Some(frame);
            }
            if count > 0 {
                input.rules.decoded(count);
            }
            if let Some(frame) = newest {
                pictures.insert((handle, *track), frame_image(frame));
            }
        }
    }
}

/// A control about this side's own stream, from the peer with key `requester`, arriving on
/// connection `conn`; `via` is set when a forwarder passed it on. True when the window changes.
fn apply_video_control(
    s: &mut MeetState,
    conn: u64,
    requester: u64,
    via: Option<u64>,
    control: Control,
) -> bool {
    match control {
        Control::KeyframeRequest { track, height } => {
            let Some(slot) = track_slot(track) else {
                return false;
            };
            // A request without a height (none named) asks every rendition of the track.
            for ((out_track, out_height), out) in s.video_out.iter_mut() {
                if *out_track == track && (height == 0 || *out_height == height) {
                    out.keyframes.request();
                }
            }
            let who = name_of(s, requester);
            let what = if height == 0 {
                String::from(SOURCES[slot])
            } else {
                rendition_label(track, height)
            };
            match via {
                Some(through) => eprintln!(
                    "[azmeet] {}: {who} asked for a keyframe ({what}, via {})",
                    s.name,
                    name_of_handle(s, through)
                ),
                None => eprintln!("[azmeet] {}: {who} asked for a keyframe ({what})", s.name),
            }
            false
        }
        Control::Received { track, seq, height } => {
            if let Some(remote) = s.remotes.iter_mut().find(|r| r.handle == conn) {
                if let Some(window) = remote.sent.get_mut(&(track, height)) {
                    window.acked(seq);
                }
            }
            false
        }
        Control::Caps { h264, encodes } => {
            let Some(remote) = s.remotes.iter_mut().find(|r| r.handle == conn) else {
                return false;
            };
            if remote.h264 == Some(h264) && remote.encodes == Some(encodes) {
                return false;
            }
            if remote.h264 != Some(h264) {
                // The peer's H.264 starts afresh, at a keyframe.
                remote.sent.clear();
            }
            remote.h264 = Some(h264);
            remote.encodes = Some(encodes);
            let node_id = remote.node_id.clone();
            let yes = |on: bool| if on { "yes" } else { "no" };
            eprintln!(
                "[azmeet] {}: {} decodes H.264: {}, encodes it: {}",
                s.name,
                remote_name(s, &node_id),
                yes(h264),
                yes(encodes)
            );
            true
        }
    }
}

/// Every pump while the camera or the screen share is a test pattern: the frame due now, in every
/// rendition someone shows, sent like a captured one.
fn pump_pattern(s: &mut MeetState) {
    if !s.pattern_video || s.remotes.is_empty() {
        return;
    }
    let elapsed = now_ms(s);
    for (slot, track) in [(0usize, CAMERA_TRACK), (1, SCREEN_TRACK)] {
        let heights = my_renditions(s, track);
        if heights.is_empty() {
            continue;
        }
        let Some(index) = s.pattern_clocks[slot].next(elapsed) else {
            continue;
        };
        for height in heights {
            let width = routes::rendition_width(height);
            // The screen's bars sit half a frame further on, so the two tiles differ.
            let shift = slot as u32 * width / 2 / video_wire::PATTERN_STEP;
            let frame = pattern_frame(index.wrapping_add(shift), width, u32::from(height));
            send_video(s, track, height, frame);
        }
    }
}

/// The devices panel's video lines: what each rendition of each local track sent, and what arrived
/// of each stream this side shows ("via Ben" when a forwarder passed it on).
fn video_lines(s: &MeetState) -> Vec<String> {
    let mut lines = Vec::new();
    for ((track, height), out) in &s.video_out {
        if out.h264_packets + out.jpeg_frames + out.dropped > 0 {
            lines.push(video_wire::send_line(
                &rendition_label(*track, *height),
                out.h264_packets,
                out.jpeg_frames,
                &out.keyframes.stats(),
                out.dropped,
            ));
        }
    }
    for r in &s.remotes {
        let name = remote_name(s, &r.node_id);
        for ((track, height), input) in &r.received {
            let stats = input.rules.stats();
            let Some(codec) = input.rules.codec().or(input.seen) else {
                continue;
            };
            // A rendition this side no longer shows keeps no line.
            if !input.shown || stats.packets == 0 || !shows(s, r.key, *track, *height) {
                continue;
            }
            let mut source = rendition_label(*track, *height);
            if input.via != r.handle {
                source.push_str(&format!(" via {}", name_of_handle(s, input.via)));
            }
            lines.push(video_wire::video_line(&name, &source, codec, &stats));
        }
    }
    lines
}

/// The periodic log: the video lines, how far this side runs ahead of each peer on each stream,
/// what arrived late, and the streams it passes on.
fn video_log_lines(s: &MeetState) -> Vec<String> {
    let mut lines = vec![codec_status(s)];
    lines.extend(video_lines(s));
    for r in &s.remotes {
        let name = remote_name(s, &r.node_id);
        for ((track, height), window) in &r.sent {
            if window.in_flight() + window.skipped() as usize > 0 {
                lines.push(format!(
                    "{} to {name}: {} in flight, {} not sent",
                    rendition_label(*track, *height),
                    window.in_flight(),
                    window.skipped()
                ));
            }
        }
        for ((track, height), input) in &r.received {
            let late = input.rules.stats().late;
            if late > 0 {
                lines.push(format!(
                    "{} from {name}: {late} late",
                    rendition_label(*track, *height)
                ));
            }
        }
    }
    for ((child, origin, track, height), window) in &s.relay.windows {
        if window.in_flight() + window.skipped() as usize > 0 {
            lines.push(format!(
                "{}'s {} passed on to {}: {} in flight, {} not sent",
                name_of(s, *origin),
                rendition_label(*track, *height),
                name_of_handle(s, *child),
                window.in_flight(),
                window.skipped()
            ));
        }
    }
    lines
}

/// The "Drop a video packet" button (`AZMEET_TEST_PATTERN=1`): the next packet of every rendition
/// this side sends is not sent, so everyone who gets one sees a gap.
extern "C" fn on_drop_video_packet(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        s.drop_video = my_streams(s);
    }
    Update::DoNothing
}

// ==== Rooms of three and more: reports, the plan, forwarding (the rules are in routes.rs) ====

/// The name of the participant with key `key`: this side's own, a peer's, or a short key.
fn name_of(s: &MeetState, key: u64) -> String {
    if key == s.me {
        return s.name.clone();
    }
    match s.remotes.iter().find(|r| r.key == key) {
        Some(r) => remote_name(s, &r.node_id),
        None => format!("{key:016x}")[..8].to_string(),
    }
}

/// The name of the peer behind connection `handle`.
fn name_of_handle(s: &MeetState, handle: u64) -> String {
    match s.remotes.iter().find(|r| r.handle == handle) {
        Some(r) => remote_name(s, &r.node_id),
        None => format!("connection {handle}"),
    }
}

/// The connection to the participant with key `key`, when connected.
fn handle_of(s: &MeetState, key: u64) -> Option<u64> {
    s.remotes.iter().find(|r| r.key == key).map(|r| r.handle)
}

/// The connections to the participants `keys` that are connected.
fn handles_of(s: &MeetState, keys: &[u64]) -> Vec<u64> {
    keys.iter().filter_map(|key| handle_of(s, *key)).collect()
}

/// "camera 360p", "screen 90p"; the track's name alone for no rendition.
fn rendition_label(track: u32, height: u16) -> String {
    let source = track_slot(track).map_or("video", |slot| SOURCES[slot]);
    if height == 0 {
        source.to_string()
    } else {
        format!("{source} {height}p")
    }
}

/// The last report of the participant with key `key`: what this side told the others, or what a
/// peer told this side.
fn sync_of(s: &MeetState, key: u64) -> Option<&routes::Sync> {
    if key == s.me {
        return Some(&s.sync);
    }
    s.remotes.iter().find(|r| r.key == key)?.sync.as_ref()
}

/// The height `viewer` shows `origin`'s `track` at, from its report; 0 when it does not show it.
fn want_of(s: &MeetState, viewer: u64, origin: u64, track: u32) -> u16 {
    sync_of(s, viewer).map_or(0, |sync| sync.want(origin, track))
}

/// Whether the participant with key `key` decodes H.264: this side's own probe, a peer's caps
/// message; `None` while a peer's caps have not arrived.
fn decodes_h264_of(s: &MeetState, key: u64) -> Option<bool> {
    if key == s.me {
        return Some(s.video.decodes_h264);
    }
    s.remotes.iter().find(|r| r.key == key).and_then(|r| r.h264)
}

fn encodes_h264_of(s: &MeetState, key: u64) -> bool {
    if key == s.me {
        return s.video.encoder.is_ok();
    }
    s.remotes
        .iter()
        .any(|r| r.key == key && r.encodes == Some(true))
}

/// Which stream each of `peers` gets of `origin`'s `track` (`routes::assign`), in the codec
/// `video_wire::wire_codec` picks.
fn assignment_among(
    s: &MeetState,
    peers: &[u64],
    origin: u64,
    track: u32,
) -> BTreeMap<u64, routes::Stream> {
    let sender_h264 = encodes_h264_of(s, origin);
    let viewers: Vec<routes::Viewer> = peers
        .iter()
        .copied()
        .filter(|viewer| *viewer != origin)
        .filter_map(|viewer| {
            // A viewer whose caps have not arrived gets nothing yet - not JPEG.
            let codec = video_wire::wire_codec(sender_h264, decodes_h264_of(s, viewer))?;
            Some(routes::Viewer {
                key: viewer,
                need: want_of(s, viewer, origin, track),
                h264: codec == Codec::H264,
            })
        })
        .collect();
    routes::assign(&viewers, sender_h264)
}

/// Which stream each participant of the plan gets of `origin`'s `track`.
fn assignment(s: &MeetState, origin: u64, track: u32) -> BTreeMap<u64, routes::Stream> {
    assignment_among(s, s.plan.peers(), origin, track)
}

/// Whether this side gets (and shows) the `height` rendition of `origin`'s `track`.
fn shows(s: &MeetState, origin: u64, track: u32, height: u16) -> bool {
    assignment(s, origin, track)
        .get(&s.me)
        .is_some_and(|got| got.height == height)
}

/// The heights of this side's `track` someone shows, smallest first; none while the track is off.
fn my_renditions(s: &MeetState, track: u32) -> Vec<u16> {
    let on = match track {
        CAMERA_TRACK => s.cam_on,
        SCREEN_TRACK => s.screen_on,
        _ => false,
    };
    if !on || s.remotes.is_empty() {
        return Vec::new();
    }
    routes::heights(&assignment(s, s.me, track))
}

/// This side's (track, height) streams someone shows.
fn my_streams(s: &MeetState) -> BTreeSet<(u32, u16)> {
    let mut out = BTreeSet::new();
    for track in [CAMERA_TRACK, SCREEN_TRACK] {
        for height in my_renditions(s, track) {
            out.insert((track, height));
        }
    }
    out
}

/// Closes the encoders of the renditions nobody shows any more (a paused encoder costs nothing;
/// the next start opens a fresh one, which begins with a keyframe), and the decoders of the
/// renditions this side no longer shows: a tile that moved to another rung left its old
/// decoder open for as long as the call lasted (a VideoToolbox session, its output buffers at
/// the tile's size and its XPC connection each), and the sender may have stopped that stream
/// altogether, so no packet ever came to close it. A stream shown again gets a fresh decoder,
/// which starts at a keyframe (`take_video`).
fn stop_culled(s: &mut MeetState) {
    let shown = my_streams(s);
    for (key, out) in s.video_out.iter_mut() {
        if !shown.contains(key) {
            out.stop();
        }
    }
    let stale: Vec<(usize, (u32, u16))> = s
        .remotes
        .iter()
        .enumerate()
        .flat_map(|(i, r)| {
            r.received
                .iter()
                .filter(|(_, input)| input.decoder.is_some())
                .filter(|((track, height), _)| !shows(s, r.key, *track, *height))
                .map(move |(key, _)| (i, *key))
        })
        .collect();
    for (i, key) in stale {
        if let Some(input) = s.remotes[i].received.get_mut(&key) {
            input.decoder = None;
            input.shown = false;
        }
    }
}

/// The participant pinned to the stage: the one `AZMEET_STAGE` names, while connected.
fn pinned_key(s: &MeetState) -> Option<u64> {
    if s.stage_name.is_empty() {
        return None;
    }
    s.remotes
        .iter()
        .find(|r| remote_name(s, &r.node_id).eq_ignore_ascii_case(&s.stage_name))
        .map(|r| r.key)
}

/// What the call view shows (`tiles::arrange`): a shared screen on the stage, else in speaker
/// view the pinned participant, the active speaker or the first one; the other tiles in the
/// filmstrip (with a stage) or the gallery.
fn arrangement(s: &MeetState) -> tiles::Arrangement {
    let others: Vec<tiles::Participant> = s
        .remotes
        .iter()
        .map(|r| tiles::Participant {
            key: r.key,
            sharing: r.sync.as_ref().is_some_and(|sync| sync.sends_screen),
        })
        .collect();
    let view = match s.view {
        ViewMode::Grid => tiles::View::Gallery,
        ViewMode::Speaker => tiles::View::Speaker,
    };
    tiles::arrange(
        s.me,
        s.screen_on,
        &others,
        view,
        pinned_key(s),
        s.speaker.current(),
    )
}

/// The peer on the stage, when the stage shows a camera (the speaker view).
fn stage_key(s: &MeetState) -> Option<u64> {
    arrangement(s)
        .stage
        .filter(|tile| tile.kind == tiles::TileKind::Camera)
        .map(|tile| tile.key)
}

/// A tile's role in the arrangement and the height of its box until it is laid out; `None` for
/// a picture the view does not show (a screen nobody shares).
fn tile_role(arr: &tiles::Arrangement, tile: tiles::Tile) -> Option<(IrohTileRole, f32)> {
    if arr.stage == Some(tile) {
        return Some((IrohTileRole::Stage, STAGE_H));
    }
    if !arr.tiles.contains(&tile) {
        return None;
    }
    Some(if arr.stage.is_some() {
        (IrohTileRole::Filmstrip, FILMSTRIP_H)
    } else {
        (IrohTileRole::Gallery, TILE_H)
    })
}

/// What this side shows: for every peer's camera and screen, the rendition its tile needs, from
/// the tile's role and its laid-out height (the box's height until it is laid out); nothing for a
/// tile that is hidden, drawn tiny, or not shown at all (`tiles::tile_need`).
fn my_wants(s: &MeetState) -> Vec<routes::Want> {
    let room_size = u32::try_from(s.remotes.len() + 1).unwrap_or(u32::MAX);
    let arr = arrangement(s);
    let mut wants = Vec::new();
    for r in &s.remotes {
        for (slot, track, kind) in [
            (0usize, CAMERA_TRACK, tiles::TileKind::Camera),
            (1, SCREEN_TRACK, tiles::TileKind::Screen),
        ] {
            let tile = tiles::Tile { key: r.key, kind };
            let need = tile_role(&arr, tile).map_or(0, |(role, box_height)| {
                let height = tiles::tile_need(
                    r.tile_visible[slot],
                    r.tile_height[slot],
                    box_height,
                    s.scale,
                );
                role.rendition_height(height, s.scale, room_size)
            });
            wants.push(routes::Want {
                origin: r.key,
                track,
                height: u16::try_from(need).unwrap_or(u16::MAX),
            });
        }
    }
    wants
}

/// Recomputes this side's report; true when it changed.
fn refresh_sync(s: &mut MeetState) -> bool {
    let sync = routes::Sync {
        uplink_kbps: s.capacity.uplink_kbps(),
        stability_permille: s.capacity.stability_permille(),
        on_battery: s.on_battery,
        relay_only: s.capacity.relay_only(),
        opted_out: s.opted_out,
        sends_audio: s.mic_on,
        sends_camera: s.cam_on,
        sends_screen: s.screen_on,
        wants: my_wants(s),
    };
    let changed = sync != s.sync;
    s.sync = sync;
    changed
}

/// Tells the peers behind `handles` this side's report.
fn send_sync(s: &MeetState, handles: &[u64]) {
    send_message_to(s, handles, &routes::encode_sync(&s.sync));
}

/// Tells the peers behind `handles` whether this side decodes and encodes H.264.
fn send_caps(s: &MeetState, handles: &[u64]) {
    let caps = video_wire::encode_caps(s.video.decodes_h264, s.video.encoder.is_ok());
    send_message_to(s, handles, &caps);
}

/// Something this side reports may have changed (a peer came or went, a toggle, the layout, the
/// statistics): tells everyone when the report changed (or anyway, with `always`), plans again, and
/// closes the encoders nobody needs.
fn network_changed(s: &mut MeetState, always: bool) {
    if refresh_sync(s) || always {
        send_sync(s, &all_peers(s));
    }
    replan(s);
    stop_culled(s);
}

/// A report as `IrohLoadBalancer` takes it.
fn capacity_of(key: u64, sync: &routes::Sync) -> IrohPeerCapacity {
    let mut capacity = IrohPeerCapacity::create(key, sync.uplink_kbps);
    capacity.stability = f32::from(sync.stability_permille) / 1000.0;
    capacity.on_battery = sync.on_battery;
    capacity.relay_only = sync.relay_only;
    capacity.opted_out = sync.opted_out;
    capacity
}

/// What the room's streams add up to when every viewer gets its own copy (kbit/s): audio from
/// every open microphone to everyone else, and every video delivery at its rendition's bitrate.
/// The load balancer grows the backbone until it carries 1.5 times this.
fn fanout_kbps(s: &MeetState, peers: &[u64]) -> u64 {
    let viewers = u64::try_from(peers.len().saturating_sub(1)).unwrap_or(u64::MAX);
    let mut total = 0;
    for origin in peers {
        let Some(sync) = sync_of(s, *origin) else {
            continue;
        };
        if sync.sends_audio {
            total += AUDIO_KBPS * viewers;
        }
        for (track, on) in [
            (CAMERA_TRACK, sync.sends_camera),
            (SCREEN_TRACK, sync.sends_screen),
        ] {
            if on {
                total += assignment_among(s, peers, *origin, track)
                    .values()
                    .map(|stream| {
                        u64::from(IrohLoadBalancer::rendition_kbps(u32::from(stream.height)))
                    })
                    .sum::<u64>();
            }
        }
    }
    total
}

/// Plans again from every report: this side's own and each peer's last. Every side feeds the same
/// reports to the load balancer and gets the same plan. True when the plan changed.
fn replan(s: &mut MeetState) -> bool {
    let mut peers = vec![s.me];
    peers.extend(s.remotes.iter().filter(|r| r.sync.is_some()).map(|r| r.key));
    let mut balancer = IrohLoadBalancer::create();
    balancer.set_mesh_cap(s.mesh_cap);
    balancer.set_peer(capacity_of(s.me, &s.sync));
    for r in &s.remotes {
        if let Some(sync) = r.sync.as_ref() {
            balancer.set_peer(capacity_of(r.key, sync));
        }
    }
    let count = balancer.select_backbone(fanout_kbps(s, &peers));
    let backbone: Vec<u64> = (0..count)
        .filter_map(|index| balancer.backbone_peer(index).into_option())
        .collect();
    let plan = routes::Plan::new(&peers, &backbone);
    if plan == s.plan {
        return false;
    }
    s.plan = plan;
    let view: &MeetState = s;
    let name = |key: u64| name_of(view, key);
    eprintln!(
        "[azmeet] {}: {}",
        view.name,
        routes::plan_line(&view.plan, view.mesh_cap, &name)
    );
    eprintln!(
        "[azmeet] {}: {}",
        view.name,
        routes::routes_line(&view.plan, &name)
    );
    true
}

/// The laid-out height of every peer's video tiles and the window's scale, so each tile asks for
/// the rendition it is drawn at.
fn measure_tiles(s: &mut MeetState, info: &mut CallbackInfo) {
    let dpi = info.get_current_window_state().size.dpi;
    if dpi > 0 {
        s.scale = dpi as f32 / 96.0;
    }
    let scale = s.scale.max(0.5);
    for r in s.remotes.iter_mut() {
        for (slot, track) in [(0usize, CAMERA_TRACK), (1, SCREEN_TRACK)] {
            let marker = tile_marker(r.handle, track);
            let node = info
                .get_node_id_by_marker(AzString::from(marker.as_str()))
                .into_option();
            // A tile not laid out yet counts as visible (its box's height asks).
            r.tile_visible[slot] = match &node {
                Some(node) => info.is_node_visible(node.clone()),
                None => true,
            };
            let size = node.and_then(|node| info.get_node_size(node).into_option());
            r.tile_height[slot] = size.map(|size| size.height);
            r.tile_px[slot] = size.and_then(|size| {
                let w = (size.width * scale).round() as u32;
                let h = (size.height * scale).round() as u32;
                (w > 0 && h > 0).then_some((w, h))
            });
        }
    }
}

/// Every 2 seconds: the tiles' heights, the paths' statistics into the uplink estimate, the report
/// to everyone (a peer that missed one gets it again), and the plan.
fn network_tick(s: &mut MeetState, endpoint: &IrohEndpoint, info: &mut CallbackInfo) {
    measure_tiles(s, info);
    let now = now_ms(s);
    let paths: Vec<(u64, routes::PathSample)> = s
        .remotes
        .iter_mut()
        .map(|r| {
            let stats = endpoint.peer_stats(r.handle);
            r.path = Some((stats.direct, stats.rtt_us as f64 / 1000.0));
            let sample = routes::PathSample {
                bytes_sent: stats.bytes_sent,
                lost_packets: stats.lost_packets,
                rtt_us: stats.rtt_us,
                cwnd_bytes: stats.cwnd_bytes,
                direct: stats.direct,
            };
            (r.handle, sample)
        })
        .collect();
    s.capacity.sample(now, &paths);
    network_changed(s, true);
}

/// The routing settings of this run (see the module docs): the mesh cap, a pinned uplink, the
/// forwarding opt-out, battery, and the view.
fn configure_network(s: &mut MeetState) {
    let setting = |key: &str| {
        std::env::var(key)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    s.mesh_cap = setting("AZMEET_MESH_CAP")
        .and_then(|v| v.parse().ok())
        .unwrap_or(routes::DEFAULT_MESH_CAP);
    let pinned = setting("AZMEET_UPLINK_KBPS")
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|kbps| *kbps > 0);
    s.capacity = routes::CapacityEstimator::new(pinned);
    s.opted_out = setting("AZMEET_NO_FORWARD").is_some_and(|v| v == "1");
    s.on_battery = setting("AZMEET_ON_BATTERY").is_some_and(|v| v == "1");
    s.view = if setting("AZMEET_LAYOUT").is_some_and(|v| v.eq_ignore_ascii_case("speaker")) {
        ViewMode::Speaker
    } else {
        ViewMode::Grid
    };
    s.stage_name = setting("AZMEET_STAGE").unwrap_or_default();
    if let Some(endpoint) = s.endpoint.as_ref() {
        s.me = routes::peer_key(endpoint.endpoint_id().as_str());
    }
    refresh_sync(s);
    eprintln!(
        "[azmeet] {}: mesh cap {}, reporting up {}{}{}",
        s.name,
        s.mesh_cap,
        routes::kbps_label(s.sync.uplink_kbps),
        if pinned.is_some() {
            " (AZMEET_UPLINK_KBPS)"
        } else {
            " until measured"
        },
        if s.opted_out {
            ", not forwarding for others"
        } else {
            ""
        }
    );
}

/// A peer's report arrived: plan again. True when the window changes.
fn apply_sync(s: &mut MeetState, conn: u64, sync: routes::Sync) -> bool {
    let Some(remote) = s.remotes.iter_mut().find(|r| r.handle == conn) else {
        return false;
    };
    if remote.sync.as_ref() == Some(&sync) {
        return false;
    }
    remote.sync = Some(sync);
    replan(s);
    stop_culled(s);
    true
}

/// Media that reached this side: as its origin sent it, or already wrapped by a forwarder.
#[derive(Clone, Copy)]
enum Carried<'a> {
    Direct(&'a [u8]),
    Relayed(&'a [u8]),
}

impl Carried<'_> {
    /// The bytes to pass on: a forwarder's envelope as it came, or the origin's bytes wrapped.
    fn envelope(self, track: u32, origin: u64) -> Vec<u8> {
        match self {
            Carried::Direct(inner) => routes::encode_relay(track, origin, origin, inner),
            Carried::Relayed(envelope) => envelope.to_vec(),
        }
    }
}

/// A frame or message from connection `event.peer`: media (the peer's own, or passed on for
/// someone), a control, a report (`ConnectionSync`), or the audio state. True when the window
/// changes.
fn receive_item(
    s: &mut MeetState,
    endpoint: &IrohEndpoint,
    event: &IrohEvent,
    pictures: &mut BTreeMap<(u64, u32), RawImage>,
) -> bool {
    let conn = event.peer;
    let bytes = event.data.as_slice();
    let Some(sender) = s.remotes.iter().find(|r| r.handle == conn).map(|r| r.key) else {
        return false;
    };
    if let Some(relayed) = routes::decode_relay(bytes) {
        return receive_relayed(s, endpoint, conn, relayed, bytes, pictures);
    }
    if event.kind == IrohEventKind::Frame && event.track == AUDIO_TRACK {
        let stage_moved = receive_audio(s, sender, bytes);
        pass_on_audio(s, endpoint, sender, conn, Carried::Direct(bytes));
        return stage_moved && speaker_moved(s);
    }
    if let Some(sync) = routes::decode_sync(bytes) {
        return apply_sync(s, conn, sync);
    }
    if bytes.first() == Some(&chat::KIND_CHAT) {
        return receive_chat(s, sender, bytes);
    }
    match video_wire::decode_message(bytes) {
        Some(Message::Packet(header, payload)) => {
            let new_tile = take_video(s, endpoint, sender, conn, &header, payload, pictures);
            pass_on_video(s, endpoint, sender, conn, &header, Carried::Direct(bytes));
            new_tile
        }
        Some(Message::Control(control)) => apply_video_control(s, conn, sender, None, control),
        // A frame of no known kind, or a malformed one.
        None if event.kind == IrohEventKind::Frame => false,
        None => apply_link_event(s, event),
    }
}

/// An item a forwarder passed on through connection `conn` (`envelope` is all of it): someone's
/// media, a keyframe request on its way to the origin, or a child's acknowledgement of a stream
/// this side passes on.
fn receive_relayed(
    s: &mut MeetState,
    endpoint: &IrohEndpoint,
    conn: u64,
    relayed: routes::Relayed<'_>,
    envelope: &[u8],
    pictures: &mut BTreeMap<(u64, u32), RawImage>,
) -> bool {
    let origin = relayed.origin;
    if origin == s.me {
        // A viewer further down asks for a keyframe of this side's own stream.
        return match video_wire::decode_control(relayed.inner) {
            Some(control @ Control::KeyframeRequest { .. }) => {
                apply_video_control(s, conn, relayed.from, Some(conn), control)
            }
            _ => false,
        };
    }
    if relayed.track == AUDIO_TRACK {
        let stage_moved = receive_audio(s, origin, relayed.inner);
        pass_on_audio(s, endpoint, origin, conn, Carried::Relayed(envelope));
        return stage_moved && speaker_moved(s);
    }
    match video_wire::decode_message(relayed.inner) {
        Some(Message::Packet(header, payload)) => {
            let new_tile = take_video(s, endpoint, origin, conn, &header, payload, pictures);
            pass_on_video(
                s,
                endpoint,
                origin,
                conn,
                &header,
                Carried::Relayed(envelope),
            );
            new_tile
        }
        Some(Message::Control(Control::KeyframeRequest { track, height })) => {
            let request = PassedRequest {
                origin,
                requester: relayed.from,
                track,
                height,
            };
            pass_request_on(s, endpoint, conn, &request, envelope);
            false
        }
        Some(Message::Control(Control::Received { track, seq, height })) => {
            if let Some(window) = s.relay.windows.get_mut(&(conn, origin, track, height)) {
                window.acked(seq);
            }
            false
        }
        _ => false,
    }
}

/// The frame lane of `origin`'s passed-on frames (`routes::frame_track`), from 1.
fn relay_lane(s: &mut MeetState, origin: u64) -> u32 {
    let next = u32::try_from(s.relay.lanes.len() + 1).unwrap_or(u32::MAX);
    *s.relay.lanes.entry(origin).or_insert(next)
}

/// The connections of `keys`, without the origin and the connection the item came through.
fn relay_targets(s: &MeetState, origin: u64, came_from: u64, keys: &[u64]) -> Vec<u64> {
    let origin_handle = handle_of(s, origin);
    handles_of(s, keys)
        .into_iter()
        .filter(|handle| *handle != came_from && Some(*handle) != origin_handle)
        .collect()
}

/// `origin`'s audio frame passed on to this side's children in origin's tree, on origin's lane.
fn pass_on_audio(
    s: &mut MeetState,
    endpoint: &IrohEndpoint,
    origin: u64,
    came_from: u64,
    carried: Carried<'_>,
) {
    let children = s.plan.children(s.me, origin);
    let targets = relay_targets(s, origin, came_from, &children);
    if targets.is_empty() {
        return;
    }
    let track = routes::frame_track(relay_lane(s, origin), AUDIO_TRACK, 0);
    let envelope = carried.envelope(AUDIO_TRACK, origin);
    s.relay.frames += targets.len() as u64;
    for handle in targets {
        endpoint.send_frame(handle, track, envelope.clone());
    }
}

/// A packet of `origin`'s video passed on to the children with a viewer of its stream below them
/// (`routes::Plan::next_hops`): H.264 as a message through a window per child (a child that falls
/// behind resumes at a keyframe, which is asked for upstream), JPEG as a frame on origin's lane.
fn pass_on_video(
    s: &mut MeetState,
    endpoint: &IrohEndpoint,
    origin: u64,
    came_from: u64,
    header: &video_wire::Header,
    carried: Carried<'_>,
) {
    let stream = routes::Stream {
        height: header.height,
        h264: header.codec == Codec::H264,
    };
    let assigned = assignment(s, origin, header.track);
    let hops = s.plan.next_hops(s.me, origin, stream, &assigned);
    let targets = relay_targets(s, origin, came_from, &hops);
    if targets.is_empty() {
        return;
    }
    let envelope = carried.envelope(header.track, origin);
    match header.codec {
        Codec::H264 => {
            let mut resync = false;
            for handle in targets {
                let window = s
                    .relay
                    .windows
                    .entry((handle, origin, header.track, header.height))
                    .or_insert_with(video_wire::SendWindow::new);
                if window.offer(header.seq, header.keyframe) {
                    endpoint.send_message(handle, envelope.clone());
                    s.relay.packets += 1;
                }
                resync |= window.wants_keyframe();
            }
            if resync {
                ask_upstream(s, endpoint, origin, came_from, header.track, header.height);
            }
        }
        Codec::Jpeg => {
            let track = routes::frame_track(relay_lane(s, origin), header.track, header.height);
            s.relay.frames += targets.len() as u64;
            for handle in targets {
                endpoint.send_frame(handle, track, envelope.clone());
            }
        }
    }
}

/// A child waits for a keyframe of a stream this side passes on: ask the way the stream comes
/// (through `via`), at most once per `video_wire::REQUEST_RETRY_MS`.
fn ask_upstream(
    s: &mut MeetState,
    endpoint: &IrohEndpoint,
    origin: u64,
    via: u64,
    track: u32,
    height: u16,
) {
    let now = now_ms(s);
    let key = (origin, track, height);
    let recent = s
        .relay
        .asked
        .get(&key)
        .is_some_and(|at| now.saturating_sub(*at) < video_wire::REQUEST_RETRY_MS);
    if recent {
        return;
    }
    s.relay.asked.insert(key, now);
    let request = video_wire::encode_keyframe_request(track, height);
    send_control_up(s, endpoint, origin, via, track, &request);
}

/// A control about `origin`'s stream, sent back through `via`, the connection the stream comes
/// through: as it is when that is the origin, else wrapped, naming the origin and this side.
fn send_control_up(
    s: &MeetState,
    endpoint: &IrohEndpoint,
    origin: u64,
    via: u64,
    track: u32,
    control: &[u8],
) {
    if handle_of(s, origin) == Some(via) {
        endpoint.send_message(via, control.to_vec());
    } else {
        endpoint.send_message(via, routes::encode_relay(track, origin, s.me, control));
    }
}

/// A keyframe request on its way from `requester` to `origin`.
struct PassedRequest {
    origin: u64,
    requester: u64,
    track: u32,
    height: u16,
}

/// Passes a keyframe request on toward its origin, through the connection this side gets that
/// stream from (else its parent in the origin's tree), as it came (`envelope`).
fn pass_request_on(
    s: &mut MeetState,
    endpoint: &IrohEndpoint,
    came_from: u64,
    request: &PassedRequest,
    envelope: &[u8],
) {
    let through = s
        .remotes
        .iter()
        .find(|r| r.key == request.origin)
        .and_then(|r| r.received.get(&(request.track, request.height)))
        .map(|input| input.via);
    let upstream = through.or_else(|| {
        s.plan
            .parent(s.me, request.origin)
            .and_then(|parent| handle_of(s, parent))
    });
    let Some(upstream) = upstream.filter(|handle| *handle != came_from) else {
        return;
    };
    endpoint.send_message(upstream, envelope.to_vec());
    s.relay.requests += 1;
    eprintln!(
        "[azmeet] {}: passing {}'s keyframe request for {}'s {} on to {}",
        s.name,
        name_of(s, request.requester),
        name_of(s, request.origin),
        rendition_label(request.track, request.height),
        name_of_handle(s, upstream)
    );
}

/// A peer left: the windows of what this side passed on to it or for it go.
fn forget_relaying(s: &mut MeetState, gone: &Remote) {
    s.relay
        .windows
        .retain(|(child, origin, _, _), _| *child != gone.handle && *origin != gone.key);
    s.relay
        .asked
        .retain(|(origin, _, _), _| *origin != gone.key);
}

/// The network panel: the plan, every origin's route, this side's role and report, what it passed
/// on, and one line per peer. Empty while nobody else is here.
fn network_lines(s: &MeetState) -> Vec<String> {
    if s.remotes.is_empty() {
        return Vec::new();
    }
    let name = |key: u64| name_of(s, key);
    let mut lines = vec![
        routes::plan_line(&s.plan, s.mesh_cap, &name),
        routes::routes_line(&s.plan, &name),
        routes::role_line(&s.plan, s.me, &name),
        report_line(s),
    ];
    if s.relay.packets + s.relay.frames + s.relay.requests > 0 {
        lines.push(format!(
            "Forwarded: {} packets, {} frames, {} keyframe requests passed on",
            s.relay.packets, s.relay.frames, s.relay.requests
        ));
    }
    for r in &s.remotes {
        lines.push(routes::peer_line(&peer_row(s, r)));
    }
    lines
}

/// "You report: up 1 Mbps (pinned; measured 100 Mbps) · stability 100%".
fn report_line(s: &MeetState) -> String {
    let mut line = format!("You report: up {}", routes::kbps_label(s.sync.uplink_kbps));
    match (s.capacity.is_pinned(), s.capacity.measured_kbps()) {
        (true, Some(measured)) => line.push_str(&format!(
            " (pinned; measured {})",
            routes::kbps_label(measured)
        )),
        (true, None) => line.push_str(" (pinned)"),
        (false, Some(_)) => line.push_str(" (measured)"),
        (false, None) => line.push_str(" (not measured yet)"),
    }
    line.push_str(&format!(" · stability {}%", s.sync.stability_permille / 10));
    if s.sync.relay_only {
        line.push_str(" · relay only");
    }
    if s.sync.opted_out {
        line.push_str(" · not forwarding for others");
    }
    if s.sync.on_battery {
        line.push_str(" · on battery");
    }
    line
}

/// The network panel's line about peer `r`: its path, its part in the plan, what it reports, what
/// this side sends it (its own streams, then what it passes on for others), and what this side
/// gets through it.
fn peer_row(s: &MeetState, r: &Remote) -> routes::PeerRow {
    let me = s.me;
    let mut own = Vec::new();
    let mut passed = Vec::new();
    let mut from = Vec::new();
    for origin in s.plan.peers().iter().copied() {
        let Some(sync) = sync_of(s, origin) else {
            continue;
        };
        let origin_name = name_of(s, origin);
        let through_r = origin != me && s.plan.parent(me, origin) == Some(r.key);
        let mut sends = |item: String| {
            if origin == me {
                own.push(item);
            } else {
                passed.push(format!("{origin_name} {item}"));
            }
        };
        if sync.sends_audio {
            if s.plan.children(me, origin).contains(&r.key) {
                sends(String::from("audio"));
            }
            if through_r {
                from.push(format!("{origin_name} audio"));
            }
        }
        for (slot, track, on) in [
            (0usize, CAMERA_TRACK, sync.sends_camera),
            (1, SCREEN_TRACK, sync.sends_screen),
        ] {
            if !on {
                continue;
            }
            let assigned = assignment(s, origin, track);
            for stream in routes::streams(&assigned) {
                if s.plan
                    .next_hops(me, origin, stream, &assigned)
                    .contains(&r.key)
                {
                    sends(routes::stream_label(SOURCES[slot], stream));
                }
            }
            if through_r {
                if let Some(stream) = assigned.get(&me) {
                    from.push(format!(
                        "{origin_name} {}",
                        routes::stream_label(SOURCES[slot], *stream)
                    ));
                }
            }
        }
    }
    passed.sort();
    from.sort();
    own.extend(passed);
    routes::PeerRow {
        name: remote_name(s, &r.node_id),
        path: r.path,
        backbone: r.sync.as_ref().map(|_| s.plan.is_backbone(r.key)),
        uplink_kbps: r.sync.as_ref().map(|sync| sync.uplink_kbps),
        to: own,
        from,
    }
}

// ==== Meeting server: requests on an azul Thread, answers on the UI thread ====

/// The callback a finished request resumes into, on the UI thread.
type ResumeFn = extern "C" fn(RefAny, CallbackInfo, RefAny) -> Update;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Verb {
    Get,
    Post,
    Delete,
}

/// One request to the meeting server.
struct HttpJob {
    verb: Verb,
    url: String,
    body: String,
    on_result: ResumeFn,
    /// The room session the request was sent in (see `RoomSession::session`).
    session: u32,
}

impl HttpJob {
    fn create_room(room: &RoomSession) -> Self {
        HttpJob {
            verb: Verb::Post,
            url: format!("{}/rooms", room.worker),
            body: String::from("{}"),
            on_result: on_room_opened,
            session: room.session,
        }
    }

    fn look_up(room: &RoomSession, key: &RoomKey) -> Self {
        HttpJob {
            verb: Verb::Get,
            url: format!("{}/rooms/{}?format=json", room.worker, key.as_str()),
            body: String::new(),
            on_result: on_room_opened,
            session: room.session,
        }
    }

    fn announce(room: &RoomSession) -> Self {
        let body = Json::object(vec![
            JsonKeyValue::create("node_id", Json::string(room.node_id.as_str())),
            JsonKeyValue::create("ticket", Json::string(room.ticket.as_str())),
            JsonKeyValue::create("name", Json::string(room.name.as_str())),
        ]);
        HttpJob {
            verb: Verb::Post,
            url: format!("{}/rooms/{}/peers", room.worker, room.room_id),
            body: body.to_string().as_str().to_string(),
            on_result: on_announced,
            session: room.session,
        }
    }

    fn poll(room: &RoomSession) -> Self {
        HttpJob {
            verb: Verb::Get,
            url: format!(
                "{}/rooms/{}/peers?except={}",
                room.worker, room.room_id, room.node_id
            ),
            body: String::new(),
            on_result: on_peers,
            session: room.session,
        }
    }

    /// Whether the meeting server answers (`GET /health`); the answer is matched to the check by
    /// `session`, which holds the check's number here.
    fn health(room: &RoomSession) -> Self {
        HttpJob {
            verb: Verb::Get,
            url: format!("{}/health", room.worker),
            body: String::new(),
            on_result: on_health,
            session: room.checks,
        }
    }

    /// Takes this participant off the room's list (Leave).
    fn leave(room: &RoomSession) -> Self {
        HttpJob {
            verb: Verb::Delete,
            url: format!(
                "{}/rooms/{}/peers/{}",
                room.worker, room.room_id, room.node_id
            ),
            body: String::new(),
            on_result: on_left,
            session: room.session,
        }
    }
}

struct HttpThreadInit {
    job: HttpJob,
    /// The participant's `MeetState`, handed back to `job.on_result` in a `Reply`.
    app: RefAny,
}

/// What a finished request resumes with: the participant's `MeetState` and the room session the
/// request was sent in.
struct Reply {
    app: RefAny,
    session: u32,
}

/// The participant and the session of a resumed request.
fn reply_parts(mut data: RefAny) -> Option<(RefAny, u32)> {
    let reply = data.downcast_ref::<Reply>()?;
    Some((reply.app.clone(), reply.session))
}

/// Runs one request on a worker thread. `http_request` blocks here, then queues its answer,
/// which the UI thread delivers to `on_result` on its next pump (the 15 ms link timer).
extern "C" fn http_thread(mut init: RefAny, _sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((verb, url, body, on_result, reply)) =
        init.downcast_ref::<HttpThreadInit>().map(|i| {
            (
                i.job.verb,
                i.job.url.clone(),
                i.job.body.clone(),
                i.job.on_result,
                Reply {
                    app: i.app.clone(),
                    session: i.job.session,
                },
            )
        })
    else {
        return;
    };
    let method = match verb {
        Verb::Get => HttpMethod::Get,
        Verb::Post => HttpMethod::Post,
        Verb::Delete => HttpMethod::Delete,
    };
    let _request = HttpRequestConfig::create()
        .with_timeout(HTTP_TIMEOUT_SECS)
        .with_user_agent("AzMeet/0.1")
        .with_header("accept", "application/json")
        .http_request(
            method,
            url.as_str(),
            U8Vec::from(body.into_bytes()),
            "application/json",
            RefAny::new(reply),
            on_result,
        );
}

fn spawn_http(info: &mut CallbackInfo, app: RefAny, job: HttpJob) {
    let init = RefAny::new(HttpThreadInit { job, app });
    info.add_thread(
        ThreadId::unique(),
        Thread::create(init, RefAny::new(()), http_thread),
    );
}

fn http_error_text(e: &HttpError) -> String {
    match e {
        HttpError::Timeout => String::from("timed out"),
        HttpError::InvalidUrl(s)
        | HttpError::ConnectionFailed(s)
        | HttpError::TlsError(s)
        | HttpError::IoError(s)
        | HttpError::Other(s) => s.as_str().to_string(),
        other => format!("{other:?}"),
    }
}

/// The status and JSON body of a finished request, or why there is none.
fn http_answer(result: RefAny) -> Result<(u16, Option<Json>), String> {
    let answer = HttpGetResult::downcast(result)
        .into_option()
        .ok_or_else(|| String::from("no answer"))?;
    let response = answer
        .result
        .into_result()
        .map_err(|e| http_error_text(&e))?;
    let json = response
        .body_as_string()
        .into_option()
        .and_then(|body| Json::parse(body).into_result().ok());
    Ok((response.status_code, json))
}

fn json_text(json: &Json, key: &str) -> Option<String> {
    let value = json.get_key(key).into_option()?;
    let text = value.as_string().into_option()?;
    Some(text.as_str().to_string())
}

/// What `POST /rooms` and `GET /rooms/<key>` answer.
struct RoomInfo {
    room: String,
    code: String,
    link: String,
}

fn room_info(json: &Json) -> Option<RoomInfo> {
    let room = json_text(json, "room")?;
    Some(RoomInfo {
        code: json_text(json, "code").unwrap_or_default(),
        link: json_text(json, "link")
            .unwrap_or_else(|| format!("{}{room}", rooms::APP_LINK_PREFIX)),
        room,
    })
}

fn peers_from(json: &Json) -> Vec<PeerRecord> {
    let Some(list) = json.get_key("peers").into_option() else {
        return Vec::new();
    };
    (0..list.len())
        .filter_map(|i| list.get_index(i).into_option())
        .filter_map(|p| {
            Some(PeerRecord {
                node_id: json_text(&p, "node_id")?,
                ticket: json_text(&p, "ticket")?,
                name: json_text(&p, "name").unwrap_or_default(),
            })
        })
        .collect()
}

/// What to tell the user about a failed request.
fn server_trouble(worker: &str, answer: &Result<(u16, Option<Json>), String>) -> String {
    match answer {
        Ok((429, _)) => {
            String::from("Too many attempts from this network; try again in a few minutes.")
        }
        Ok((status, json)) => {
            let message = json
                .as_ref()
                .and_then(|j| json_text(j, "message"))
                .unwrap_or_default();
            format!("The meeting server answered {status}. {message}")
        }
        Err(e) => format!("The meeting server at {worker} is unreachable: {e}"),
    }
}

/// The room is gone from the server: back to the start screen, unless people are connected.
fn meeting_gone(s: &mut MeetState) {
    let connected = !s.remotes.is_empty();
    if let Some(room) = s.room.as_mut() {
        if connected {
            room.stage = Stage::Ended;
            room.busy = false;
        } else {
            room.leave();
        }
    }
    s.notice = if connected {
        String::from(
            "This meeting's link has expired: everyone here stays connected, nobody new can join.",
        )
    } else {
        String::from("This meeting has ended, or the link is wrong.")
    };
}

/// The answer to `POST /rooms` (a new meeting) or `GET /rooms/<key>` (joining a link).
extern "C" fn on_room_opened(data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some((mut data, session)) = reply_parts(data) else {
        return Update::DoNothing;
    };
    let answer = http_answer(result);
    let follow_up = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let Some(room) = s.room.as_mut() else {
            return Update::DoNothing;
        };
        if room.session != session || room.stage != Stage::Opening {
            return Update::DoNothing;
        }
        room.busy = false;
        let found = match &answer {
            Ok((200 | 201, Some(json))) => room_info(json),
            _ => None,
        };
        match found {
            Some(found) => {
                room.enter(found);
                println!("AZMEET_ROOM {}", room.room_id);
                println!("AZMEET_LINK {}", room.link);
                if !room.code.is_empty() {
                    println!("AZMEET_CODE {}", room.code);
                }
                eprintln!(
                    "[azmeet] {}: in meeting {} ({})",
                    s.name, room.code, room.room_id
                );
                s.notice.clear();
                s.link_status = String::from("waiting for others to join");
                room.first_job()
            }
            None => {
                room.stage = Stage::Start;
                s.notice = match &answer {
                    Ok((404, _)) => String::from("This meeting has ended, or the link is wrong."),
                    Ok((200 | 201, _)) => {
                        String::from("The meeting server sent an answer AzMeet cannot read.")
                    }
                    other => server_trouble(&room.worker, other),
                };
                None
            }
        }
    };
    if let Some(job) = follow_up {
        spawn_http(&mut info, data.clone(), job);
    }
    Update::RefreshDom
}

/// The answer to an announcement; a successful one is followed by a read of the peers list.
extern "C" fn on_announced(data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some((mut data, session)) = reply_parts(data) else {
        return Update::DoNothing;
    };
    let answer = http_answer(result);
    let (follow_up, update) = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let Some(room) = s.room.as_mut() else {
            return Update::DoNothing;
        };
        if room.session != session || room.stage != Stage::InRoom {
            // An answer from a meeting since left.
            return Update::DoNothing;
        }
        room.busy = false;
        match &answer {
            Ok((200, _)) => {
                let update = if room.trouble {
                    room.trouble = false;
                    s.notice.clear();
                    Update::RefreshDom
                } else {
                    Update::DoNothing
                };
                room.busy = true;
                room.busy_polls = 0;
                (Some(HttpJob::poll(room)), update)
            }
            Ok((404, _)) => {
                meeting_gone(s);
                (None, Update::RefreshDom)
            }
            other => {
                room.announced_at = None;
                room.trouble = true;
                s.notice = server_trouble(&room.worker, other);
                (None, Update::RefreshDom)
            }
        }
    };
    if let Some(job) = follow_up {
        spawn_http(&mut info, data.clone(), job);
    }
    update
}

/// The peers list: dial who this side should dial.
extern "C" fn on_peers(data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let Some((mut data, session)) = reply_parts(data) else {
        return Update::DoNothing;
    };
    let answer = http_answer(result);
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let Some(room) = s.room.as_mut() else {
        return Update::DoNothing;
    };
    if room.session != session || room.stage != Stage::InRoom {
        // An answer from a meeting since left.
        return Update::DoNothing;
    }
    room.busy = false;
    match answer {
        Ok((200, Some(json))) => {
            let listed = peers_from(&json);
            let diff = rooms::diff_peers(&room.peers, &listed, &room.node_id);
            for p in &diff.joined {
                eprintln!(
                    "[azmeet] {}: {} joined ({})",
                    s.name,
                    p.name,
                    short_id(&p.node_id)
                );
            }
            for p in &diff.left {
                eprintln!("[azmeet] {}: {} left", s.name, p.name);
            }
            let connected: BTreeSet<String> = s.remotes.iter().map(|r| r.node_id.clone()).collect();
            let plan =
                rooms::plan_dials(&room.node_id, &listed, &connected, &room.dialed, room.polls);
            if let Some(endpoint) = s.endpoint.as_ref() {
                for p in &plan {
                    eprintln!(
                        "[azmeet] {}: dialing {} ({})",
                        s.name,
                        p.name,
                        short_id(&p.node_id)
                    );
                    let _dialing = endpoint.connect(p.ticket.as_str());
                    room.dialed.insert(
                        p.node_id.clone(),
                        Dialed {
                            ticket: p.ticket.clone(),
                            at_poll: room.polls,
                        },
                    );
                }
            }
            let mut changed = !diff.is_empty() || !plan.is_empty();
            room.peers = listed;
            if room.trouble {
                room.trouble = false;
                s.notice.clear();
                changed = true;
            }
            if changed {
                Update::RefreshDom
            } else {
                Update::DoNothing
            }
        }
        Ok((404, _)) => {
            meeting_gone(s);
            Update::RefreshDom
        }
        other => {
            room.trouble = true;
            s.notice = server_trouble(&room.worker, &other);
            Update::RefreshDom
        }
    }
}

// ==== The meeting server field (start screen) ====

/// The meeting server field as typed.
extern "C" fn on_server_text(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        if let Some(room) = s.room.as_mut() {
            room.server_text = state.get_text().as_str().to_string();
        }
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

/// Enter in the meeting server field takes its address.
extern "C" fn on_server_key(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let key = info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option();
    let update = match key {
        Some(VirtualKeyCode::Return | VirtualKeyCode::NumpadEnter) => {
            let text = state.get_text().as_str().to_string();
            commit_server(&mut data, &mut info, &text)
        }
        _ => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// Leaving the meeting server field takes its address.
extern "C" fn on_server_blur(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: TextInputState,
) -> Update {
    let text = state.get_text().as_str().to_string();
    commit_server(&mut data, &mut info, &text)
}

/// The meeting server field was left with `text` (Enter, or it lost focus): a new address is the
/// meeting server for every request from now on, and is checked (`GET /health`); it is saved once
/// it answers. The same address is checked again only when its last check failed.
fn commit_server(data: &mut RefAny, info: &mut CallbackInfo, text: &str) -> Update {
    let job = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let Some(room) = s.room.as_mut() else {
            return Update::DoNothing;
        };
        if room.stage != Stage::Start {
            return Update::DoNothing;
        }
        room.server_text = text.to_string();
        let Some(server) = rooms::normalize_server(text) else {
            room.server_ok = false;
            room.server_status = String::from(
                "That is no meeting server address: it starts with http:// or https://.",
            );
            return Update::RefreshDom;
        };
        if server == room.worker && room.server_ok {
            return Update::DoNothing;
        }
        room.server_text = server.clone();
        room.worker = server;
        room.checks = room.checks.wrapping_add(1);
        room.server_ok = false;
        room.server_status = format!("Asking {} ...", room.worker);
        let job = HttpJob::health(room);
        rebind_for_server(s);
        job
    };
    spawn_http(info, data.clone(), job);
    Update::RefreshDom
}

/// A meeting server on another host may need other relays (none for one on this machine, the
/// public ones otherwise): the endpoint is bound again when that choice changes. Only on the start
/// screen, so no peer is connected.
fn rebind_for_server(s: &mut MeetState) {
    let Some(room) = s.room.as_ref() else {
        return;
    };
    let relay = relay_for(&room.worker);
    if relay == room.relay {
        return;
    }
    let endpoint = bind_endpoint(&relay);
    if !endpoint.is_bound() {
        let reason = bind_failure(&endpoint);
        eprintln!(
            "[azmeet] {}: no iroh endpoint with relays {relay:?}: {reason}",
            s.name
        );
        return;
    }
    let node_id = endpoint.endpoint_id().as_str().to_string();
    eprintln!(
        "[azmeet] {}: endpoint {} (relays {relay:?}) for the meeting server {}",
        s.name,
        short_id(&node_id),
        room.worker
    );
    s.me = routes::peer_key(&node_id);
    s.endpoint = Some(endpoint);
    if let Some(room) = s.room.as_mut() {
        room.node_id = node_id;
        // The new endpoint's ticket arrives with its `Ready` event.
        room.ticket.clear();
        room.relay = relay;
    }
}

/// The answer to a meeting server check: the line under the field says whether it answers, and a
/// server that answers is remembered (not in a headless run: a test never touches the user's
/// settings).
extern "C" fn on_health(data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let Some((mut data, check)) = reply_parts(data) else {
        return Update::DoNothing;
    };
    let answer = http_answer(result);
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let Some(room) = s.room.as_mut() else {
        return Update::DoNothing;
    };
    if room.checks != check {
        // The answer to an address typed over since.
        return Update::DoNothing;
    }
    if !matches!(answer, Ok((200, _))) {
        room.server_ok = false;
        room.server_status = server_trouble(&room.worker, &answer);
        return Update::RefreshDom;
    }
    room.server_ok = true;
    room.server_status = String::from("The meeting server answers.");
    if devices_allowed() {
        match save_server(&room.worker) {
            Ok(()) => eprintln!(
                "[azmeet] {}: remembered the meeting server {}",
                s.name, room.worker
            ),
            Err(e) => {
                room.server_status =
                    format!("The meeting server answers, but it could not be remembered: {e}");
            }
        }
    }
    Update::RefreshDom
}

/// The file the meeting server is remembered in: `AzMeet/settings.txt` in the per-user config
/// folder (`FilePath::get_config_dir`).
fn settings_path() -> Option<std::path::PathBuf> {
    let dir = FilePath::get_config_dir().into_option()?;
    let dir = dir.as_string().as_str().to_string();
    if dir.is_empty() {
        return None;
    }
    Some(
        std::path::Path::new(&dir)
            .join("AzMeet")
            .join("settings.txt"),
    )
}

/// The meeting server saved last time; `None` without a settings file, or with one that cannot be
/// read, is too long, or names no meeting server.
fn saved_server() -> Option<String> {
    use std::io::Read;
    let file = std::fs::File::open(settings_path()?).ok()?;
    let mut text = String::new();
    file.take(rooms::MAX_SETTINGS_BYTES as u64 + 1)
        .read_to_string(&mut text)
        .ok()?;
    rooms::decode_settings(&text)
}

/// Remembers `server`: written to a temporary file next to the settings file, then renamed over
/// it, so the settings file is never half written.
fn save_server(server: &str) -> Result<(), String> {
    let path = settings_path().ok_or_else(|| String::from("there is no per-user config folder"))?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let temp = path.with_extension("txt.tmp");
    std::fs::write(&temp, rooms::encode_settings(server)).map_err(|e| e.to_string())?;
    std::fs::rename(&temp, &path).map_err(|e| e.to_string())
}

/// The answer to the leave request; nothing waits on it.
extern "C" fn on_left(data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let name = match reply_parts(data) {
        Some((mut app, _)) => {
            let name = app.downcast_ref::<MeetState>().map(|s| s.name.clone());
            name.unwrap_or_default()
        }
        None => String::new(),
    };
    match http_answer(result) {
        Ok((200, _)) => {
            eprintln!("[azmeet] {name}: the meeting server no longer lists this participant")
        }
        Ok((404, _)) => eprintln!("[azmeet] {name}: the meeting had already ended on the server"),
        Ok((status, _)) => eprintln!(
            "[azmeet] {name}: the meeting server answered {status} to leaving; the record expires \
             by itself"
        ),
        Err(e) => eprintln!(
            "[azmeet] {name}: could not reach the meeting server to leave ({e}); the record \
             expires by itself"
        ),
    }
    Update::DoNothing
}

/// Every 2 seconds in a room: announce when due, else read the peers list.
extern "C" fn room_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let job = match data.downcast_mut::<MeetState>() {
        Some(mut s) => s.room.as_mut().and_then(|room| room.next_job()),
        None => None,
    };
    if let Some(job) = job {
        spawn_http(&mut info.callback_info, data.clone(), job);
    }
    TimerCallbackReturn::continue_unchanged()
}

fn begin_new_meeting(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let job = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let Some(room) = s.room.as_mut() else {
            return Update::DoNothing;
        };
        if room.stage != Stage::Start || room.busy {
            return Update::DoNothing;
        }
        room.stage = Stage::Opening;
        room.busy = true;
        s.notice = String::from("Asking the meeting server for a new meeting...");
        HttpJob::create_room(room)
    };
    spawn_http(info, data.clone(), job);
    Update::RefreshDom
}

fn begin_join(data: &mut RefAny, info: &mut CallbackInfo, text: &str) -> Update {
    let job = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let Some(room) = s.room.as_mut() else {
            return Update::DoNothing;
        };
        if room.stage != Stage::Start || room.busy {
            return Update::DoNothing;
        }
        let Some(key) = rooms::parse_room_link(text) else {
            s.notice = String::from(
                "That is not a meeting link. Paste an azlin://meet/... link, the meeting's web \
                 address, or its code.",
            );
            return Update::RefreshDom;
        };
        room.stage = Stage::Opening;
        room.busy = true;
        s.notice = String::from("Looking up the meeting...");
        HttpJob::look_up(room, &key)
    };
    spawn_http(info, data.clone(), job);
    Update::RefreshDom
}

extern "C" fn on_new_meeting(mut data: RefAny, mut info: CallbackInfo) -> Update {
    begin_new_meeting(&mut data, &mut info)
}

extern "C" fn on_join(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let text = data
        .downcast_ref::<MeetState>()
        .and_then(|s| s.room.as_ref().map(|room| room.join_text.clone()))
        .unwrap_or_default();
    begin_join(&mut data, &mut info, &text)
}

extern "C" fn on_join_text(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        if let Some(room) = s.room.as_mut() {
            room.join_text = state.get_text().as_str().to_string();
        }
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_copy_link(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let link = {
        let Some(mut s) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let Some(room) = s.room.as_mut() else {
            return Update::DoNothing;
        };
        room.copied = true;
        room.link.clone()
    };
    info.set_clipboard_content(ClipboardContent {
        plain_text: AzString::from(link.as_str()),
        styled_runs: StyledTextRunVec::create(),
        html: OptionString::None,
    });
    Update::RefreshDom
}

/// `AZMEET_JOIN` / `AZMEET_AUTOCREATE`: start in a meeting without a click.
fn autostart(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    if let Some(link) = std::env::var("AZMEET_JOIN")
        .ok()
        .filter(|l| !l.trim().is_empty())
    {
        if let Some(mut s) = data.downcast_mut::<MeetState>() {
            if let Some(room) = s.room.as_mut() {
                room.join_text = link.clone();
            }
        }
        return begin_join(data, info, &link);
    }
    if std::env::var("AZMEET_AUTOCREATE").is_ok_and(|v| v.trim() == "1") {
        return begin_new_meeting(data, info);
    }
    Update::DoNothing
}

extern "C" fn startup_first(data: RefAny, info: CallbackInfo) -> Update {
    start_pumping(data, info, 0)
}

extern "C" fn startup_second(data: RefAny, info: CallbackInfo) -> Update {
    start_pumping(data, info, 1)
}

fn start_pumping(data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    let Some(mut peer) = room_peer(&data, index) else {
        return Update::DoNothing;
    };
    let get_time = info.get_system_time_fn();
    info.add_timer(
        TimerId::unique(),
        Timer::create(peer.clone(), pump_link, get_time)
            .with_interval(Duration::System(SystemTimeDiff::from_millis(pace::BUSY_MS))),
    );
    let in_rooms = peer
        .downcast_ref::<MeetState>()
        .is_some_and(|s| s.room.is_some());
    if !in_rooms {
        return Update::DoNothing;
    }
    let get_time = info.get_system_time_fn();
    info.add_timer(
        TimerId::unique(),
        Timer::create(peer.clone(), room_tick, get_time)
            .with_interval(Duration::System(SystemTimeDiff::from_millis(ROOM_POLL_MS))),
    );
    autostart(&mut peer, &mut info)
}

fn renderer(hw_accel: HwAcceleration) -> OptionRendererOptions {
    OptionRendererOptions::Some(RendererOptions {
        vsync: Vsync::Enabled,
        srgb: Srgb::DontCare,
        hw_accel,
        ..RendererOptions::default()
    })
}

/// A chunk from the microphone: sent to the peers, and shown on the level meter.
extern "C" fn mic_on_frame(mut data: RefAny, info: CallbackInfo, frame: AudioFrame) -> Update {
    let samples: &[f32] = frame.samples.as_ref();
    let meter = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        send_audio(s, frame.sample_rate, frame.channels, samples);
        meter_change(s, samples)
    };
    if let Some((bar, level)) = meter {
        show_level(info, bar, level);
    }
    Update::DoNothing
}

extern "C" fn mic_toggle(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        s.mic_on = !s.mic_on;
        sync_mic(s);
        send_state_to_all(s);
        network_changed(s, false);
    }
    Update::RefreshDom
}

extern "C" fn deafen_toggle(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        s.deafened = !s.deafened;
        if s.deafened {
            if let Some(shared) = s.playout.as_ref() {
                for jitter in lock(shared).peers.values_mut() {
                    jitter.clear();
                }
            }
        }
        send_state_to_all(s);
    }
    Update::RefreshDom
}

/// Leaves the meeting: disconnects every peer, stops announcing and polling, and returns to the
/// start screen. Returns the request that takes this participant off the room's list at once
/// (without it the meeting server drops the record after its peer TTL).
fn leave_meeting(s: &mut MeetState) -> Option<HttpJob> {
    if let Some(endpoint) = s.endpoint.as_ref() {
        for r in &s.remotes {
            endpoint.disconnect(r.handle);
        }
    }
    s.remotes.clear();
    drop_audio(s, None);
    s.packetizer.reset();
    for out in s.video_out.values_mut() {
        out.stop();
    }
    s.drop_video.clear();
    s.plan = routes::Plan::default();
    s.relay = Relaying::default();
    s.chat = chat::ChatLog::new();
    s.speaker = speaker::ActiveSpeaker::new();
    s.link_status = String::from("not in a meeting");
    s.notice = String::from("You left the meeting.");
    let room = s.room.as_mut()?;
    // An ended meeting is gone from the server already.
    let job = if room.stage == Stage::InRoom && !room.node_id.is_empty() {
        Some(HttpJob::leave(room))
    } else {
        None
    };
    eprintln!("[azmeet] {}: left meeting {}", s.name, room.code);
    room.leave();
    job
}

extern "C" fn on_leave(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let job = match data.downcast_mut::<MeetState>() {
        Some(mut guard) => leave_meeting(&mut guard),
        None => return Update::DoNothing,
    };
    if let Some(job) = job {
        spawn_http(&mut info, data.clone(), job);
    }
    Update::RefreshDom
}

/// The camera on or off: the encoders of its renditions close with it (`stop_culled`), and the
/// others hear it in the next report.
extern "C" fn cam_toggle(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        s.cam_on = !s.cam_on;
        network_changed(s, false);
    }
    Update::RefreshDom
}
extern "C" fn screen_toggle(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        s.screen_on = !s.screen_on;
        network_changed(s, false);
    }
    Update::RefreshDom
}

/// Grid or speaker view: the tiles change size, so what this side asks for changes.
extern "C" fn view_toggle(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        s.view = match s.view {
            ViewMode::Grid => ViewMode::Speaker,
            ViewMode::Speaker => ViewMode::Grid,
        };
        // Until the tiles are laid out again, the new boxes' heights count.
        for r in s.remotes.iter_mut() {
            r.tile_height = [None; 2];
            r.tile_px = [None; 2];
        }
        network_changed(s, false);
    }
    Update::RefreshDom
}

fn gen_link() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(
        "{:03x}-{:04x}-{:03x}",
        (n & 0xfff) as u16,
        ((n >> 12) & 0xffff) as u16,
        ((n >> 28) & 0xfff) as u16,
    )
}

fn bind_endpoint(relay: &Relay) -> IrohEndpoint {
    let config = IrohConfig::create(ALPN);
    let config = match relay {
        Relay::Off => config.with_relay_mode(IrohRelayMode::Disabled),
        Relay::Default => config.with_relay_mode(IrohRelayMode::Default),
        Relay::Custom(url) => config
            .with_relay_mode(IrohRelayMode::Custom)
            .with_relay_url(url.as_str()),
    };
    IrohEndpoint::bind(config)
}

fn bind_failure(endpoint: &IrohEndpoint) -> String {
    match endpoint.recv().into_option() {
        Some(event) if event.kind == IrohEventKind::Error => event.text.as_str().to_string(),
        _ => String::from("the iroh endpoint did not bind"),
    }
}

/// The name others see: `AZMEET_NAME`, else the login name.
fn display_name() -> String {
    ["AZMEET_NAME", "USER", "USERNAME"]
        .into_iter()
        .find_map(|key| {
            std::env::var(key)
                .ok()
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
        })
        .unwrap_or_else(|| String::from("Guest"))
}

/// Host (IPv6 without brackets) and port of an http(s) address.
fn server_address(url: &str) -> Option<(String, u16)> {
    let parsed = Url::parse(url).into_result().ok()?;
    let host = parsed
        .host
        .as_str()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_string();
    if !(parsed.is_http() || parsed.is_https()) || host.is_empty() {
        return None;
    }
    Some((host, parsed.effective_port()))
}

/// Whether something accepts a TCP connection at the meeting server's address, within 2.5 s.
/// Runs before any window opens, so it blocks no callback.
fn probe(url: &str) -> Result<(), String> {
    use std::net::{TcpStream, ToSocketAddrs};
    let (host, port) = server_address(url).ok_or_else(|| String::from("not an http(s) address"))?;
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let outcome = (host.as_str(), port)
            .to_socket_addrs()
            .map_err(|e| e.to_string())
            .and_then(|addrs| {
                let mut last = String::from("no address");
                for addr in addrs {
                    match TcpStream::connect_timeout(&addr, std::time::Duration::from_millis(800)) {
                        Ok(_) => return Ok(()),
                        Err(e) => last = e.to_string(),
                    }
                }
                Err(last)
            });
        let _ = tx.send(outcome);
    });
    rx.recv_timeout(std::time::Duration::from_millis(2500))
        .unwrap_or_else(|_| Err(String::from("no answer")))
}

/// The meeting server at start (`rooms::server_prefill`: the one saved last time, else
/// `AZMEET_WORKER`, else the built-in default), where it came from, and whether it accepts a
/// connection. A headless run never reads the user's settings, so `AZMEET_WORKER` wins there.
fn meeting_server() -> (String, rooms::ServerSource, Result<(), String>) {
    let saved = if devices_allowed() {
        saved_server()
    } else {
        None
    };
    let env = std::env::var("AZMEET_WORKER").ok();
    let built_in = if PRODUCTION_WORKER.is_empty() {
        rooms::LOCAL_WORKER
    } else {
        PRODUCTION_WORKER
    };
    let (url, source) = rooms::server_prefill(saved.as_deref(), env.as_deref(), built_in);
    let answer = probe(&url);
    (url, source, answer)
}

pub fn start() {
    let (worker, source, answer) = meeting_server();
    if rooms::opens_demo(source, answer.is_ok()) {
        let why = answer.err().unwrap_or_default();
        start_demo(&format!(
            "Local demo: no meeting server is set, and none answers at {worker} ({why}). Start the \
             meet Worker's dev server, or AzMeet with AZMEET_WORKER=<url>, to meet other people."
        ));
    } else {
        start_rooms(worker, answer);
    }
}

/// The relays for a meeting server at `worker`: `AZMEET_RELAY`, else none for one on this machine.
fn relay_for(worker: &str) -> Relay {
    let host = server_address(worker)
        .map(|(host, _)| host)
        .unwrap_or_default();
    rooms::relay_choice(std::env::var("AZMEET_RELAY").ok().as_deref(), &host)
}

/// One window with the start screen, talking to the meeting server at `worker`; `answer` says
/// whether it accepted a connection at start.
fn start_rooms(worker: String, answer: Result<(), String>) {
    let relay = relay_for(&worker);
    let name = display_name();
    let endpoint = bind_endpoint(&relay);
    let mut me = MeetState::new(&name, "", "");
    let mut room = RoomSession::new(worker.clone(), name.clone(), relay.clone());
    room.server_ok = answer.is_ok();
    room.server_status = match &answer {
        Ok(()) => String::from("The meeting server answers."),
        Err(e) => format!(
            "The meeting server at {worker} does not answer ({e}). Type another one and press \
             Enter."
        ),
    };
    if endpoint.is_bound() {
        room.node_id = endpoint.endpoint_id().as_str().to_string();
        eprintln!(
            "[azmeet] {name}: endpoint {} (relays {relay:?}), meeting server {worker}",
            short_id(&room.node_id)
        );
        me.endpoint = Some(endpoint);
        me.link_status = String::from("not in a meeting");
    } else {
        let reason = bind_failure(&endpoint);
        eprintln!("[azmeet] {name}: no iroh endpoint: {reason}");
        me.notice = format!("No network link, so nobody can be reached: {reason}");
        me.link_status = reason;
    }
    me.room = Some(room);
    configure_audio(&mut me);
    configure_video(&mut me, &probe_video());
    configure_network(&mut me);
    run(vec![RefAny::new(me)], false);
}

/// The in-process demo: two participants linked by two iroh endpoints, or one without a link.
/// `notice` says why there is no meeting server; both windows show it.
fn start_demo(notice: &str) {
    let meeting = gen_link();
    let notice = notice.to_string();
    eprintln!("[azmeet] {notice}");
    let video = probe_video();
    let ada_link = bind_endpoint(&Relay::Off);
    let ben_link = bind_endpoint(&Relay::Off);
    let peers = if ada_link.is_bound() && ben_link.is_bound() {
        eprintln!(
            "[azmeet] meeting {meeting}: Ada (CPU window, camera) and Ben (GPU window, screen share) over iroh"
        );
        let mut ada = MeetState::new("Ada", "Ben", "CPU");
        ada.meeting = meeting.clone();
        ada.notice = notice.clone();
        ada.guest = Some(ben_link.clone());
        ada.endpoint = Some(ada_link);
        ada.cam_on = true;
        let mut ben = MeetState::new("Ben", "Ada", "GPU");
        ben.meeting = meeting.clone();
        ben.notice = notice;
        ben.endpoint = Some(ben_link);
        ben.screen_on = true;
        configure_audio(&mut ada);
        configure_audio(&mut ben);
        configure_video(&mut ada, &video);
        configure_video(&mut ben, &video);
        configure_network(&mut ada);
        configure_network(&mut ben);
        vec![RefAny::new(ada), RefAny::new(ben)]
    } else {
        let failure = bind_failure(&ada_link);
        eprintln!("[azmeet] joined meeting {meeting} without a peer link: {failure}");
        let mut solo = MeetState::new("You", "", "");
        solo.meeting = meeting;
        solo.notice = notice;
        solo.link_status = failure;
        configure_audio(&mut solo);
        configure_video(&mut solo, &video);
        configure_network(&mut solo);
        vec![RefAny::new(solo)]
    };
    let linked = peers.len() == 2;
    run(peers, linked);
}

fn run(peers: Vec<RefAny>, linked: bool) {
    let mut app = App::create(RefAny::new(Room { peers }), AppConfig::create());
    let mut first = WindowCreateOptions::create(layout_first);
    first.window_state.flags.decorations = WindowDecorations::NoTitle;
    first.create_callback = Some(Callback::create(startup_first)).into();
    if linked {
        first.window_state.size.dimensions = LogicalSize::create(740.0, 640.0);
        first.window_state.title = AzString::from("AzMeet · Ada (CPU)");
        first.window_state.position =
            WindowPosition::Initialized(PhysicalPositionI32 { x: 20, y: 40 });
        first.renderer = renderer(HwAcceleration::Disabled);
        let mut second = WindowCreateOptions::create(layout_second);
        second.window_state.flags.decorations = WindowDecorations::NoTitle;
        second.create_callback = Some(Callback::create(startup_second)).into();
        second.window_state.size.dimensions = LogicalSize::create(740.0, 640.0);
        second.window_state.title = AzString::from("AzMeet · Ben (GPU)");
        second.window_state.position =
            WindowPosition::Initialized(PhysicalPositionI32 { x: 960, y: 40 });
        second.renderer = renderer(HwAcceleration::Enabled);
        app.add_window(second);
    } else {
        first.window_state.size.dimensions = LogicalSize::create(1100.0, 720.0);
        first.window_state.title = AzString::from("AzMeet");
    }
    app.run(first);
}

#[cfg(target_os = "android")]
#[ctor::ctor]
fn azul_android_init() {
    start();
}
