//! AzMeet: video meetings over azul.iroh.
//!
//! With a meeting server (the `meet` Worker, azul-apps `cf-workers/meet`) AzMeet opens a start
//! screen: "New meeting" asks the server for a room and shows its link; "Join with a link" takes
//! that link. Either way the app announces its iroh ticket to the room every 20 seconds, reads
//! everyone else's every 2 seconds, and dials the peers whose endpoint id is higher than its own.
//! Every HTTP request runs on an azul `Thread` and resumes on the UI thread, so no callback waits
//! on the network.
//!
//! Video (see `video_wire.rs`): each captured camera or screen frame (320x180) goes through an
//! H.264 `VideoEncoder` where one works (VideoToolbox on Apple; found out at start by encoding a
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
//! Without a reachable meeting server it runs the in-process demo: two participants, Ada in a
//! CPU-rendered window and Ben in a GPU-rendered one, linked by two iroh endpoints.
//!
//! Environment:
//! - `AZMEET_WORKER`: the meeting server, e.g. `http://127.0.0.1:8787` (the local mock); else the
//!   `PRODUCTION_WORKER` constant, set at build time with `AZMEET_DEFAULT_WORKER=<url>`.
//! - `AZMEET_NAME`: the name others see (default: `$USER`).
//! - `AZMEET_AUTOCREATE=1`: create a meeting at start and print `AZMEET_LINK <link>` on stdout.
//! - `AZMEET_JOIN=<link>`: join that meeting at start.
//! - `AZMEET_RELAY`: `off`, `default` or a relay URL (default: off for a meeting server on this
//!   machine, the public iroh relays otherwise).
//! - `AZMEET_TEST_TONE=1`: a 440 Hz tone replaces the microphone, which starts unmuted.
//! - `AZMEET_TEST_PATTERN=1`: moving colour bars replace the camera (and the screen share), the
//!   camera starts on, and a "Drop a video packet" button drops the next packet before it leaves.
//! - `AZMEET_VIDEO_CODEC=jpeg`: send JPEG even where H.264 works.
//! - `AZ_BACKEND=headless`: no audio device, camera or screen is opened: the microphone is the
//!   tone (muted until switched on, unless `AZMEET_TEST_TONE=1`), received audio is counted, not
//!   played, and the camera and the screen share are test patterns (off until switched on, unless
//!   `AZMEET_TEST_PATTERN=1`).

mod audio;
mod rooms;
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
    dom::{Callback, ClipboardContent, DomNodeId, NodeId},
    error::{HttpError, ResultRawImageDecodeImageError, ResultU8VecEncodeImageError},
    http::{HttpGetResult, HttpMethod, HttpRequestConfig},
    image::{ImageRef, RawImage, RawImageData, RawImageFormat, VideoDecoder, VideoEncoder},
    iroh::{IrohConfig, IrohEndpoint, IrohEvent, IrohEventKind, IrohRelayMode},
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
    },
    window::{HwAcceleration, PlatformCapability, Vsync},
};
use rooms::{Dialed, PeerRecord, Relay, RoomKey};
use video_wire::{Codec, Control, Message};

/// The protocol name: peers of an older wire format (M2's MJPEG frames) cannot connect.
const ALPN: &str = "azmeet/2";
const CAMERA_TRACK: u32 = 1;
const SCREEN_TRACK: u32 = 2;
/// The audio track: 20 ms PCM packets, three to a frame (`audio.rs`).
const AUDIO_TRACK: u32 = 3;
/// The rate the microphone (or the test tone) is asked for.
const MIC_RATE: u32 = 48_000;
const TONE_HZ: f32 = 440.0;
const FEED_W: u32 = 320;
const FEED_H: u32 = 180;
const JPEG_QUALITY: u8 = 75;
/// The H.264 encoder's target bitrate for a 320x180 feed.
const VIDEO_KBPS: u32 = 400;
/// What the camera and the screen tracks are called in the window, by `track_slot`.
const SOURCES: [&str; 2] = ["camera", "screen"];
const PUMP_MS: u64 = 15;
const STATS_EVERY_TICKS: u32 = 130;

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
    /// Which of the peer's tracks (camera, screen) have a tile.
    tracks: [bool; 2],
    /// Muted and deafened, from the peer's last control message; `None` until the first.
    state: Option<audio::PeerState>,
    /// Whether the peer decodes H.264, from its caps message; `None` until that arrives, and the
    /// peer gets JPEG until then.
    h264: Option<bool>,
    /// How far this side ran ahead of the peer on each local video track (H.264), by `track_slot`.
    sent: [video_wire::SendWindow; 2],
    /// The peer's camera and screen tracks as they arrive, by `track_slot`.
    received: [VideoIn; 2],
}

impl Remote {
    fn new(handle: u64, node_id: String) -> Self {
        Remote {
            handle,
            node_id,
            tracks: [false; 2],
            state: None,
            h264: None,
            sent: [video_wire::SendWindow::new(), video_wire::SendWindow::new()],
            received: [VideoIn::new(), VideoIn::new()],
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
}

impl RoomSession {
    fn new(worker: String, name: String) -> Self {
        RoomSession {
            session: 0,
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
    ticks: u32,
    mic_on: bool,
    cam_on: bool,
    screen_on: bool,
    mic_level: f32,
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
    /// The sending side of the camera and screen tracks, by `track_slot`.
    video_out: [VideoOut; 2],
    /// The camera and the screen share are test patterns (`AZMEET_TEST_PATTERN=1`, and every
    /// headless run).
    pattern_video: bool,
    /// When each test pattern's next frame is due, by `track_slot`.
    pattern_clocks: [video_wire::PatternClock; 2],
    /// Shows the "Drop a video packet" button (`AZMEET_TEST_PATTERN=1`).
    video_debug: bool,
    /// The next video packet is dropped instead of sent (the button).
    drop_next_video: bool,
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
            ticks: 0,
            mic_on: false,
            cam_on: false,
            screen_on: false,
            mic_level: 0.0,
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
            video_out: [VideoOut::new(), VideoOut::new()],
            pattern_video: false,
            pattern_clocks: [
                video_wire::PatternClock::new(video_wire::PATTERN_FPS),
                video_wire::PatternClock::new(video_wire::PATTERN_FPS),
            ],
            video_debug: false,
            drop_next_video: false,
        }
    }
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

const TILE: &str = "width: 300px; height: 200px; margin: 8px; border-radius: 10px; background: \
                    #2b2b38; display: flex; align-items: center; justify-content: center; color: \
                    #99a; font-size: 17px; overflow: hidden;";
const BTN: &str = "padding: 10px 18px; margin: 0 6px; border-radius: 8px; background: #3a3a4a; \
                   color: #e6e6f0; font-size: 14px; white-space: nowrap; flex-shrink: 0;";
const BTN_ON: &str = "padding: 10px 18px; margin: 0 6px; border-radius: 8px; background: #2f6db0; \
                      color: #ffffff; font-size: 14px; white-space: nowrap; flex-shrink: 0;";
const BTN_LEAVE: &str = "padding: 10px 18px; margin: 0 6px 0 24px; border-radius: 8px; \
                         background: #b03a3a; color: #ffffff; font-size: 14px; white-space: \
                         nowrap; flex-shrink: 0;";
const NOTICE: &str = "padding: 6px 12px; font-size: 13px; color: #f0b060; background: #15151c;";

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

fn remote_video_tile(marker: &str) -> Dom {
    Dom::create_div().with_css(TILE).with_child(
        Dom::create_image(ImageRef::null_image(
            FEED_W as usize,
            FEED_H as usize,
            RawImageFormat::RGBA8,
            U8VecRef::from(&[][..]),
        ))
        .with_marker(OptionString::Some(AzString::from(marker)))
        .with_css("width: 100%; height: 100%;"),
    )
}

fn participant(name: &str) -> Dom {
    Dom::create_div()
        .with_css(TILE)
        .with_child(Dom::create_span_with_text(name))
}

fn device_col(title: &str, devices: &[String]) -> Dom {
    let mut col =
        Dom::create_div().with_css("display: flex; flex-direction: column; margin: 0 28px;");
    col = col.with_child(
        Dom::create_span_with_text(title)
            .with_css("font-size: 13px; color: #8890a8; margin-bottom: 4px;"),
    );
    if devices.is_empty() {
        col = col.with_child(
            Dom::create_span_with_text("(none detected)").with_css("font-size: 13px; color: #667;"),
        );
    } else {
        for d in devices {
            col = col.with_child(
                Dom::create_span_with_text(d.as_str())
                    .with_css("font-size: 13px; color: #ccd; padding: 2px 0;"),
            );
        }
    }
    col
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

extern "C" fn layout_first(data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    peer_layout(data, 0)
}

extern "C" fn layout_second(data: RefAny, _info: LayoutCallbackInfo) -> Dom {
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

fn feed_consumer(track: u32) -> FrameConsumer {
    FrameConsumer::create(track, FEED_W, FEED_H)
}

/// What the room parts of the window show.
struct RoomView {
    stage: Stage,
    worker: String,
    join_text: String,
    link: String,
    copied: bool,
    /// "Ada (you)", then one "<name> · <status>" row per other participant.
    roster: Vec<String>,
}

struct LayoutSnapshot {
    header: String,
    notice: String,
    name: String,
    linked: bool,
    /// The solo fallback without any link shows placeholder participants.
    placeholders: bool,
    /// Markers of the remote video tiles to show.
    remote_tiles: Vec<String>,
    /// Labels of the remote participants with no video tile yet.
    waiting: Vec<String>,
    link_status: String,
    mic: bool,
    cam: bool,
    screen: bool,
    mic_level: f32,
    mics: Vec<String>,
    speakers: Vec<String>,
    room: Option<RoomView>,
    deafened: bool,
    /// The microphone is the test tone, so no `MicrophoneWidget` is mounted.
    tone_mic: bool,
    /// "Audio from Ben: ..." for every connected peer whose audio arrived.
    audio_lines: Vec<String>,
    /// Audio packets sent since the start.
    packets_sent: u32,
    /// "Video: H.264 (VideoToolbox)" or "Video: JPEG (no encoder)".
    codec_line: String,
    /// "Sending camera: ..." per local track, "Video from Ben (camera): ..." per peer track.
    video_lines: Vec<String>,
    /// The camera and the screen share are test patterns, so no capture widget is mounted.
    pattern_video: bool,
    /// Show the "Drop a video packet" button.
    video_debug: bool,
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

fn roster(s: &MeetState, room: &RoomSession) -> Vec<String> {
    let me = format!("{} (you)", s.name);
    let mut rows = vec![audio::person_line(&me, Some(my_state(s)))];
    for p in &room.peers {
        let remote = s.remotes.iter().find(|r| r.node_id == p.node_id);
        let status = if remote.is_some() {
            "connected"
        } else if rooms::dials(&room.node_id, &p.node_id) {
            "connecting"
        } else {
            "waiting for them to connect"
        };
        let label = format!("{} · {}", p.name, status);
        rows.push(audio::person_line(&label, remote.and_then(|r| r.state)));
    }
    // Connected peers whose record expired from the server stay listed.
    for r in &s.remotes {
        if !room.peers.iter().any(|p| p.node_id == r.node_id) {
            let label = format!("{} · connected", short_id(&r.node_id));
            rows.push(audio::person_line(&label, r.state));
        }
    }
    rows
}

fn snapshot(s: &MeetState) -> LayoutSnapshot {
    let mut remote_tiles = Vec::new();
    let mut waiting = Vec::new();
    for r in &s.remotes {
        let shown: Vec<String> = [CAMERA_TRACK, SCREEN_TRACK]
            .into_iter()
            .filter(|track| track_slot(*track).is_some_and(|slot| r.tracks[slot]))
            .map(|track| tile_marker(r.handle, track))
            .collect();
        if shown.is_empty() {
            waiting.push(format!(
                "{} · waiting for video",
                remote_name(s, &r.node_id)
            ));
        } else {
            remote_tiles.extend(shown);
        }
    }
    if s.room.is_none() && s.endpoint.is_some() && s.remotes.is_empty() {
        waiting.push(format!("{} · waiting for video", s.peer_name));
    }
    let header = match &s.room {
        Some(room) if !room.code.is_empty() => {
            format!("AzMeet · meeting {} · {}", room.code, s.name)
        }
        Some(_) => format!("AzMeet · {}", s.name),
        None if s.endpoint.is_some() => format!(
            "AzMeet · meeting {} · {} ({})",
            s.meeting, s.name, s.backend
        ),
        None => format!("AzMeet · meeting {}", s.meeting),
    };
    LayoutSnapshot {
        header,
        notice: s.notice.clone(),
        name: s.name.clone(),
        linked: s.endpoint.is_some(),
        placeholders: s.room.is_none() && s.endpoint.is_none(),
        remote_tiles,
        waiting,
        link_status: s.link_status.clone(),
        mic: s.mic_on,
        cam: s.cam_on,
        screen: s.screen_on,
        mic_level: s.mic_level,
        mics: s.mics.clone(),
        speakers: s.speakers.clone(),
        room: s.room.as_ref().map(|room| RoomView {
            stage: room.stage,
            worker: room.worker.clone(),
            join_text: room.join_text.clone(),
            link: room.link.clone(),
            copied: room.copied,
            roster: roster(s, room),
        }),
        deafened: s.deafened,
        tone_mic: s.tone_mic,
        audio_lines: audio_lines(s),
        packets_sent: s.packetizer.next_sequence(),
        codec_line: codec_status(s),
        video_lines: video_lines(s),
        pattern_video: s.pattern_video,
        video_debug: s.video_debug,
    }
}

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

    match &view.room {
        Some(room) if matches!(room.stage, Stage::Start | Stage::Opening) => {
            start_layout(&view, room, &data)
        }
        _ => call_layout(&view, &data),
    }
}

/// The start screen: "New meeting" and "Join with a link".
fn start_layout(view: &LayoutSnapshot, room: &RoomView, data: &RefAny) -> Dom {
    let opening = room.stage == Stage::Opening;
    let mut card = Dom::create_div().with_css(
        "display: flex; flex-direction: column; width: 520px; padding: 28px; border-radius: \
         14px; background: #17171f;",
    );
    card = card.with_child(
        Dom::create_span_with_text("AzMeet")
            .with_css("font-size: 26px; font-weight: bold; margin-bottom: 4px;"),
    );
    card = card.with_child(
        Dom::create_span_with_text(format!("Meeting server {}", room.worker).as_str())
            .with_css("font-size: 13px; color: #8890a8; margin-bottom: 22px;"),
    );
    card = card.with_child(
        Button::with_type(
            if opening {
                "Please wait..."
            } else {
                "New meeting"
            },
            ButtonType::Primary,
        )
        .with_on_click(data.clone(), on_new_meeting)
        .dom()
        .with_css("margin-bottom: 26px;"),
    );
    card = card.with_child(
        Dom::create_span_with_text("Join with a link")
            .with_css("font-size: 14px; color: #ccd; margin-bottom: 6px;"),
    );
    card = card.with_child(
        Dom::create_div()
            .with_css("display: flex; flex-direction: row; align-items: center;")
            .with_child(
                TextInput::create()
                    .with_text(room.join_text.as_str())
                    .with_placeholder("azlin://meet/... or a code like xq4-8kd-2nm")
                    .with_on_text_input(data.clone(), on_join_text)
                    .dom()
                    .with_css("flex-grow: 1; margin-right: 8px;"),
            )
            .with_child(
                Button::create("Join")
                    .with_on_click(data.clone(), on_join)
                    .dom(),
            ),
    );
    if !view.notice.is_empty() {
        card = card.with_child(
            Dom::create_span_with_text(view.notice.as_str())
                .with_css("margin-top: 16px; font-size: 13px; color: #f0b060;"),
        );
    }
    card = card.with_child(
        Dom::create_span_with_text(format!("Others see you as {}", view.name).as_str())
            .with_css("margin-top: 22px; font-size: 13px; color: #8890a8;"),
    );
    Dom::create_body()
        .with_css(
            "display: flex; align-items: center; justify-content: center; height: 100%; margin: \
             0; background: #0e0e14; font-family: sans-serif; color: #e6e6f0;",
        )
        .with_child(card)
}

/// The call: header, invite link and people (in a room), tiles, controls, devices.
fn call_layout(view: &LayoutSnapshot, data: &RefAny) -> Dom {
    let self_tile = if view.cam && !view.pattern_video {
        Dom::create_div().with_css(TILE).with_child(
            CameraWidget::create(CameraConfig::default())
                .with_consumer(feed_consumer(CAMERA_TRACK))
                .with_on_consumer_frame(data.clone(), send_feed_frame)
                .dom()
                .with_css("width: 100%; height: 100%;"),
        )
    } else if view.cam {
        participant("You · test pattern")
    } else {
        participant("You · camera off")
    };

    let mut grid = Dom::create_div().with_css(
        "display: flex; flex-wrap: wrap; flex-grow: 1; align-content: flex-start; \
         justify-content: center; padding: 12px;",
    );
    grid = grid.with_child(self_tile);
    if view.screen && view.pattern_video {
        grid = grid.with_child(participant("Your screen · test pattern"));
    } else if view.screen {
        grid = grid.with_child(
            Dom::create_div().with_css(TILE).with_child(
                ScreenCaptureWidget::create(ScreenCaptureConfig::default())
                    .with_consumer(feed_consumer(SCREEN_TRACK))
                    .with_on_consumer_frame(data.clone(), send_feed_frame)
                    .dom()
                    .with_css("width: 100%; height: 100%;"),
            ),
        );
    }
    if view.placeholders {
        grid = grid
            .with_child(participant("Alice"))
            .with_child(participant("Bob"))
            .with_child(participant("Carol"));
    }
    for label in &view.waiting {
        grid = grid.with_child(participant(label));
    }
    for marker in &view.remote_tiles {
        grid = grid.with_child(remote_video_tile(marker));
    }
    if view.room.is_some() && view.waiting.is_empty() && view.remote_tiles.is_empty() {
        grid = grid.with_child(participant("Waiting for others to join"));
    }

    let mut toolbar = Dom::create_div()
        .with_css("display: flex; justify-content: center; padding: 14px; background: #15151c;")
        .with_child(toolbar_button(
            if view.mic { "Mute" } else { "Unmute mic" },
            if view.mic { BTN_ON } else { BTN },
            data,
            mic_toggle,
        ))
        .with_child(toolbar_button(
            if view.deafened { "Undeafen" } else { "Deafen" },
            if view.deafened { BTN_ON } else { BTN },
            data,
            deafen_toggle,
        ))
        .with_child(toolbar_button(
            if view.cam {
                "Stop video"
            } else {
                "Start video"
            },
            if view.cam { BTN_ON } else { BTN },
            data,
            cam_toggle,
        ))
        .with_child(toolbar_button(
            if view.screen {
                "Stop share"
            } else {
                "Share screen"
            },
            if view.screen { BTN_ON } else { BTN },
            data,
            screen_toggle,
        ));
    if view.video_debug {
        toolbar = toolbar.with_child(toolbar_button(
            "Drop a video packet",
            BTN,
            data,
            on_drop_video_packet,
        ));
    }
    if view.room.is_some() {
        toolbar = toolbar.with_child(toolbar_button("Leave", BTN_LEAVE, data, on_leave));
    }

    let link_line = if view.linked {
        format!("{FEED_W}x{FEED_H} over iroh · {}", view.link_status)
    } else {
        view.link_status.clone()
    };
    let mut video_col = vec![view.codec_line.clone(), link_line];
    video_col.extend(view.video_lines.iter().cloned());
    let devices_panel = Dom::create_div()
        .with_css(
            "display: flex; justify-content: center; padding: 10px 12px 16px 12px; background: \
             #0e0e14; border-top: 1px solid #222;",
        )
        .with_child(device_col("Microphones", &view.mics))
        .with_child(device_col("Speakers", &view.speakers))
        .with_child(device_col("Video", &video_col))
        .with_child(device_col("Audio", &audio_col(view)));

    let mut body = Dom::create_body().with_css(
        "display: flex; flex-direction: column; height: 100%; margin: 0; background: #0e0e14; \
         font-family: sans-serif; color: #e6e6f0;",
    );
    body = body.with_child(
        Dom::create_span_with_text(view.header.as_str())
            .with_css("padding: 12px; font-size: 18px; background: #15151c;"),
    );
    if !view.notice.is_empty() {
        body = body.with_child(Dom::create_span_with_text(view.notice.as_str()).with_css(NOTICE));
    }
    if let Some(room) = &view.room {
        body = body.with_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; padding: 8px 12px; \
                     background: #15151c; border-top: 1px solid #222;",
                )
                .with_child(
                    Dom::create_span_with_text("Invite")
                        .with_css("font-size: 13px; color: #8890a8; margin-right: 10px;"),
                )
                .with_child(
                    Dom::create_span_with_text(room.link.as_str())
                        .with_css("font-size: 13px; color: #ccd; flex-grow: 1; overflow: hidden;"),
                )
                .with_child(
                    Button::create(if room.copied { "Copied" } else { "Copy link" })
                        .with_on_click(data.clone(), on_copy_link)
                        .dom(),
                ),
        );
        let mut people = Dom::create_div().with_css(
            "display: flex; flex-direction: row; flex-wrap: wrap; padding: 6px 12px; background: \
             #15151c;",
        );
        for row in &room.roster {
            people = people.with_child(
                Dom::create_span_with_text(row.as_str())
                    .with_css("font-size: 13px; color: #ccd; margin-right: 18px;"),
            );
        }
        body = body.with_child(people);
    }
    if view.mic && !view.tone_mic {
        body = body.with_child(
            MicrophoneWidget::create(AudioConfig {
                sample_rate: MIC_RATE,
                channels: 1,
            })
            .with_on_frame(data.clone(), mic_on_frame)
            .dom()
            .with_css("width: 1px; height: 1px; overflow: hidden;"),
        );
    }
    if view.mic {
        body = body.with_child(
            Dom::create_div()
                .with_css(
                    "display: flex; flex-direction: row; align-items: center; padding: 6px 12px; \
                     background: #15151c;",
                )
                .with_child(Dom::create_span_with_text("Mic level").with_css(
                    "font-size: 13px; color: #8890a8; margin-right: 10px; white-space: nowrap;",
                ))
                .with_child(
                    ProgressBar::create(view.mic_level)
                        .dom()
                        .with_css("width: 200px;")
                        .with_callback(
                            EventFilter::Component(ComponentEventFilter::AfterMount),
                            data.clone(),
                            meter_mounted,
                        )
                        .with_callback(
                            EventFilter::Component(ComponentEventFilter::BeforeUnmount),
                            data.clone(),
                            meter_unmounted,
                        ),
                ),
        );
    }
    body.with_child(grid)
        .with_child(toolbar)
        .with_child(devices_panel)
}

/// A toolbar button: `label` in the `style` box, `on_click` on mouse-up.
fn toolbar_button(label: &str, style: &str, data: &RefAny, on_click: CallbackType) -> Dom {
    Dom::create_div()
        .with_css(style)
        .with_child(Dom::create_span_with_text(label))
        .with_callback(
            EventFilter::Hover(HoverEventFilter::MouseUp),
            data.clone(),
            on_click,
        )
}

/// The devices panel's audio column: what is sent, then one line per peer heard.
fn audio_col(view: &LayoutSnapshot) -> Vec<String> {
    let source = if view.tone_mic {
        format!("{TONE_HZ} Hz test tone")
    } else {
        String::from("microphone")
    };
    let sending = if view.mic {
        format!(
            "Sending: {source}, 16-bit PCM, 20 ms packets, {} so far",
            view.packets_sent
        )
    } else {
        String::from("Sending: nothing (muted)")
    };
    let mut lines = vec![sending];
    if view.deafened {
        lines.push(String::from("Deafened: nothing is played"));
    }
    lines.extend(view.audio_lines.iter().cloned());
    lines
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

/// A frame the camera or the screen widget cut for its consumer: sent to every peer.
extern "C" fn send_feed_frame(
    mut data: RefAny,
    _info: CallbackInfo,
    frame: ConsumerFrame,
) -> Update {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        send_video(
            &mut s,
            frame.consumer.id,
            frame.frame.width,
            frame.frame.height,
            &frame.frame.bytes,
        );
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
            // What this side decodes; until the peer's answer arrives it gets JPEG.
            send_message_to(
                s,
                &[event.peer],
                &video_wire::encode_caps(s.video.decodes_h264),
            );
            true
        }
        IrohEventKind::PeerDisconnected => {
            let Some(pos) = s.remotes.iter().position(|r| r.handle == event.peer) else {
                return false;
            };
            let gone = s.remotes.remove(pos);
            drop_audio(s, Some(gone.handle));
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
    while let Some(event) = endpoint.recv().into_option() {
        let Some(mut s) = data.downcast_mut::<MeetState>() else {
            continue;
        };
        if event.kind == IrohEventKind::Frame && event.track == AUDIO_TRACK {
            receive_audio(&mut s, &event);
            continue;
        }
        let video = match event.kind {
            IrohEventKind::Frame | IrohEventKind::Message => {
                video_wire::decode_message(event.data.as_slice())
            }
            _ => None,
        };
        match video {
            Some(Message::Packet(header, payload)) => {
                let (new_tile, picture) = receive_video(&mut s, event.peer, &header, payload);
                refresh |= new_tile;
                if let Some(picture) = picture {
                    pictures.insert((event.peer, header.track), picture);
                }
            }
            Some(Message::Control(control)) => {
                refresh |= apply_video_control(&mut s, event.peer, control);
            }
            // A frame of no known track, or a malformed one.
            None if event.kind == IrohEventKind::Frame => {}
            None => refresh |= apply_link_event(&mut s, &event),
        }
    }
    for ((peer, track), picture) in pictures {
        show_picture(&mut info, peer, track, picture);
    }
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        pump_pattern(&mut s);
    }
    pump_tone(&mut data, &mut info);
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        s.ticks = s.ticks.wrapping_add(1);
        if s.ticks % STATS_EVERY_TICKS == 0 && !s.remotes.is_empty() {
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
            refresh = true;
        }
    }
    if refresh {
        TimerCallbackReturn::continue_and_refresh_dom()
    } else {
        TimerCallbackReturn::continue_unchanged()
    }
}

// ==== Audio: packets out on the audio track, jitter buffers in, a playout thread ====

/// Received audio, shared by the UI thread (which pushes each peer's packets) and the playout
/// thread (which takes one turn from every peer's jitter buffer each 20 ms). Either holds the
/// lock only to move packets, never while a device plays.
struct Playout {
    /// One jitter buffer per connection handle.
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

/// Sends captured audio to every connected peer as 20 ms packets on the audio track.
fn send_audio(s: &mut MeetState, sample_rate: u32, channels: u16, samples: &[f32]) {
    if !s.mic_on {
        return;
    }
    if s.remotes.is_empty() {
        // Nobody listens: the first frame to the next peer starts with fresh audio.
        s.packetizer.reset();
        return;
    }
    let Some(endpoint) = s.endpoint.as_ref() else {
        return;
    };
    for frame in s.packetizer.push(sample_rate, channels, samples) {
        endpoint.broadcast_frame(AUDIO_TRACK, frame);
    }
}

/// Takes a frame of a peer's audio track into that peer's jitter buffer.
fn receive_audio(s: &mut MeetState, frame: &IrohEvent) {
    if s.deafened || !s.remotes.iter().any(|r| r.handle == frame.peer) {
        return;
    }
    let Some(wire) = audio::decode_frame(frame.data.as_ref()) else {
        return;
    };
    let play = s.play_audio;
    let shared = s.playout.get_or_insert_with(|| start_playout(play));
    let mut playout = lock(shared);
    let jitter = playout
        .peers
        .entry(frame.peer)
        .or_insert_with(|| audio::JitterBuffer::new(audio::TARGET_PACKETS, audio::MAX_PACKETS));
    for packet in wire.packets {
        jitter.push(wire.sample_rate, packet);
    }
}

/// Forgets a peer's received audio (it left, or everyone did).
fn drop_audio(s: &MeetState, handle: Option<u64>) {
    let Some(shared) = s.playout.as_ref() else {
        return;
    };
    let mut playout = lock(shared);
    match handle {
        Some(handle) => {
            playout.peers.remove(&handle);
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

/// One line per connected peer whose audio arrived: what its jitter buffer took in and played,
/// and whether it waits to fill up (the peer went quiet).
fn audio_lines(s: &MeetState) -> Vec<String> {
    let Some(shared) = s.playout.as_ref() else {
        return Vec::new();
    };
    let playout = lock(shared);
    let lines: Vec<String> = s
        .remotes
        .iter()
        .filter_map(|r| {
            let jitter = playout.peers.get(&r.handle)?;
            let mut line = audio::audio_line(
                &remote_name(s, &r.node_id),
                &jitter.stats(),
                jitter.buffered(),
            );
            if !jitter.is_playing() {
                line.push_str(", filling up");
            }
            Some(line)
        })
        .collect();
    lines
}

/// The level meter's new value, when it moved by half a percent or more and the meter is shown.
fn meter_change(s: &mut MeetState, samples: &[f32]) -> Option<(DomNodeId, f32)> {
    let level = mic_level_percent(samples).round();
    if (s.mic_level - level).abs() < 0.5 {
        return None;
    }
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
        }
    }

    /// The track stopped (camera or share off, or Leave): the encoder closes, and the next start
    /// opens a fresh one, which begins with a keyframe.
    fn stop(&mut self) {
        self.encoder = None;
        self.health = video_wire::EncoderHealth::default();
    }
}

/// The receiving side of one of a peer's video tracks.
struct VideoIn {
    rules: video_wire::ReceiveTrack,
    /// Opened with the first H.264 packet to decode.
    decoder: Option<VideoDecoder>,
    /// The codec of the packet that arrived last.
    seen: Option<Codec>,
}

impl VideoIn {
    fn new() -> Self {
        VideoIn {
            rules: video_wire::ReceiveTrack::new(),
            decoder: None,
            seen: None,
        }
    }
}

/// Milliseconds on this participant's clock.
fn now_ms(s: &MeetState) -> u64 {
    s.clock.elapsed().as_millis() as u64
}

/// An RGBA picture as an image to encode or show.
fn rgba_image(width: u32, height: u32, bytes: U8Vec) -> RawImage {
    RawImage {
        pixels: RawImageData::U8(bytes),
        width: width as usize,
        height: height as usize,
        premultiplied_alpha: false,
        data_format: RawImageFormat::RGBA8,
        tag: U8Vec::create(),
    }
}

/// Frame `index` of the test pattern at the feed size.
fn pattern_frame(index: u32) -> VideoFrame {
    VideoFrame {
        width: FEED_W,
        height: FEED_H,
        bytes: U8Vec::from(video_wire::test_pattern(FEED_W, FEED_H, index)),
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
        encoder.encode(pattern_frame(index), true);
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
    decoder.decode(U8Vec::from(keyframe.to_vec()));
    let decoded = decoder.recv_frame().into_option().is_some();
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

/// Sends a captured frame (RGBA, `width` x `height`) of `track` to every connected peer: as H.264
/// to the peers that decode it (reliable messages, so nothing between two keyframes goes
/// missing), as JPEG to the others (a latest-wins frame each: every JPEG stands alone).
fn send_video(s: &mut MeetState, track: u32, width: u32, height: u32, rgba: &U8Vec) {
    let Some(slot) = track_slot(track) else {
        return;
    };
    if s.remotes.is_empty() {
        return;
    }
    let Some(endpoint) = s.endpoint.clone() else {
        return;
    };
    let out = &mut s.video_out[slot];
    out.frame_no = out.frame_no.wrapping_add(1);
    // H.264 wants even sides; 16 pixels is the smallest VideoToolbox takes.
    let fits = width % 2 == 0 && height % 2 == 0 && width >= 16 && height >= 16;
    let h264 = fits && s.video.encoder.is_ok();
    let mut h264_peers = Vec::new();
    let mut jpeg_peers = Vec::new();
    for r in &s.remotes {
        if h264 && r.h264 == Some(true) {
            h264_peers.push(r.handle);
        } else {
            jpeg_peers.push(r.handle);
        }
    }
    if !h264_peers.is_empty() && !send_h264(s, &endpoint, track, (width, height), rgba, &h264_peers)
    {
        // No working encoder for this frame: these peers get it as JPEG.
        jpeg_peers.extend(h264_peers);
    }
    if !jpeg_peers.is_empty() {
        send_jpeg(s, &endpoint, track, (width, height), rgba, &jpeg_peers);
    }
}

/// Encodes the frame with the track's H.264 encoder and sends each packet to those of `peers`
/// whose window has room ([`video_wire::SendWindow`]). False when no encoder works (it did not
/// open, gives nothing back, or ignores keyframe requests): H.264 is given up, the frame goes as
/// JPEG.
fn send_h264(
    s: &mut MeetState,
    endpoint: &IrohEndpoint,
    track: u32,
    (width, height): (u32, u32),
    rgba: &U8Vec,
    peers: &[u64],
) -> bool {
    let Some(slot) = track_slot(track) else {
        return false;
    };
    let now = now_ms(s);
    // A new peer, or one that fell behind, starts at a keyframe.
    let resync = s
        .remotes
        .iter()
        .any(|r| peers.contains(&r.handle) && r.sent[slot].wants_keyframe());
    let (packets, failure) = {
        let out = &mut s.video_out[slot];
        if resync {
            out.keyframes.request();
        }
        let resized = out.encoder.is_some() && out.size != (width, height);
        if out.keyframes.must_reopen() || resized {
            out.encoder = None;
            out.keyframes.reopened();
        }
        if out.encoder.is_none() {
            let encoder = VideoEncoder::open(width, height, false, VIDEO_KBPS);
            if encoder.is_open() {
                out.encoder = Some(encoder);
                out.size = (width, height);
                out.health = video_wire::EncoderHealth::default();
                // A new encoder begins with a keyframe.
                out.keyframes.request();
            }
        }
        let frame_no = out.frame_no;
        let mut packets = Vec::new();
        let failure = match out.encoder.as_mut() {
            None => Some("the H.264 encoder did not open"),
            Some(encoder) => {
                let force = out.keyframes.should_force(now);
                let frame = VideoFrame {
                    width,
                    height,
                    bytes: rgba.clone(),
                };
                encoder.encode(frame, force);
                out.health.submitted();
                while let Some(chunk) = encoder.recv_packet().into_option() {
                    out.health.produced();
                    let keyframe = video_wire::h264_is_keyframe(chunk.as_slice());
                    out.keyframes.on_output(keyframe, now);
                    out.h264_seq = out.h264_seq.wrapping_add(1);
                    let header = video_wire::Header {
                        codec: Codec::H264,
                        keyframe,
                        track,
                        seq: out.h264_seq,
                        frame_no,
                    };
                    packets.push((header, video_wire::encode_packet(&header, chunk.as_slice())));
                }
                if out.health.is_inert() {
                    Some("the H.264 encoder gives no packets back")
                } else if out.keyframes.is_broken() {
                    Some("the H.264 encoder ignores keyframe requests")
                } else {
                    None
                }
            }
        };
        if failure.is_some() {
            out.encoder = None;
        }
        (packets, failure)
    };
    if let Some(why) = failure {
        eprintln!("[azmeet] {}: {why}: sending JPEG from now on", s.name);
        s.video.encoder = Err(String::from(why));
        return false;
    }
    for (header, packet) in packets {
        if s.drop_next_video && !header.keyframe {
            s.drop_next_video = false;
            s.video_out[slot].dropped += 1;
            eprintln!(
                "[azmeet] {}: dropped H.264 packet {} of the {} on purpose",
                s.name, header.seq, SOURCES[slot]
            );
            continue;
        }
        s.video_out[slot].h264_packets += 1;
        for r in s.remotes.iter_mut().filter(|r| peers.contains(&r.handle)) {
            if r.sent[slot].offer(header.seq, header.keyframe) {
                endpoint.send_message(r.handle, packet.clone());
            }
        }
    }
    true
}

/// Sends the frame as JPEG to `peers`, a latest-wins frame each.
fn send_jpeg(
    s: &mut MeetState,
    endpoint: &IrohEndpoint,
    track: u32,
    (width, height): (u32, u32),
    rgba: &U8Vec,
    peers: &[u64],
) {
    let Some(slot) = track_slot(track) else {
        return;
    };
    let ResultU8VecEncodeImageError::Ok(jpeg) =
        rgba_image(width, height, rgba.clone()).encode_jpeg(JPEG_QUALITY)
    else {
        return;
    };
    let out = &mut s.video_out[slot];
    out.jpeg_seq = out.jpeg_seq.wrapping_add(1);
    let header = video_wire::Header {
        codec: Codec::Jpeg,
        keyframe: true,
        track,
        seq: out.jpeg_seq,
        frame_no: out.frame_no,
    };
    if s.drop_next_video {
        s.drop_next_video = false;
        s.video_out[slot].dropped += 1;
        eprintln!(
            "[azmeet] {}: dropped JPEG frame {} of the {} on purpose",
            s.name, header.seq, SOURCES[slot]
        );
        return;
    }
    s.video_out[slot].jpeg_frames += 1;
    let packet = video_wire::encode_packet(&header, jpeg.as_slice());
    for handle in peers {
        endpoint.send_frame(*handle, track, packet.clone());
    }
}

/// A video packet from the peer behind connection `peer`: through the track's rules (which may
/// acknowledge it or ask for a keyframe), then its decoder. Returns whether the track just got
/// its tile, and the newest picture to show in it.
fn receive_video(
    s: &mut MeetState,
    peer: u64,
    header: &video_wire::Header,
    payload: &[u8],
) -> (bool, Option<RawImage>) {
    let Some(slot) = track_slot(header.track) else {
        return (false, None);
    };
    let Some(endpoint) = s.endpoint.clone() else {
        return (false, None);
    };
    let now = now_ms(s);
    let Some(remote) = s.remotes.iter_mut().find(|r| r.handle == peer) else {
        return (false, None);
    };
    let new_tile = !remote.tracks[slot];
    remote.tracks[slot] = true;
    let input = &mut remote.received[slot];
    input.seen = Some(header.codec);
    let verdict = input.rules.on_packet(header, now);
    if verdict.restart {
        input.decoder = None;
    }
    if let Some(seq) = verdict.ack {
        endpoint.send_message(peer, video_wire::encode_received(header.track, seq));
    }
    if verdict.request_keyframe {
        endpoint.send_message(peer, video_wire::encode_keyframe_request(header.track));
    }
    if !verdict.decode {
        return (new_tile, None);
    }
    let (picture, pictures) = match header.codec {
        Codec::Jpeg => match RawImage::decode_image_bytes_any(U8VecRef::from(payload)) {
            ResultRawImageDecodeImageError::Ok(image) => (Some(image), 1),
            _ => (None, 0),
        },
        Codec::H264 => {
            let decoder = input
                .decoder
                .get_or_insert_with(|| VideoDecoder::open(false));
            decoder.decode(U8Vec::from(payload.to_vec()));
            let mut newest = None;
            let mut count = 0;
            while let Some(frame) = decoder.recv_frame().into_option() {
                count += 1;
                newest = Some(frame);
            }
            let picture = newest.map(|frame| rgba_image(frame.width, frame.height, frame.bytes));
            (picture, count)
        }
    };
    input.rules.decoded(pictures);
    let inert = input.rules.decoder_is_inert();
    if inert {
        input.decoder = None;
    }
    if inert && s.video.decodes_h264 {
        s.video.decodes_h264 = false;
        eprintln!(
            "[azmeet] {}: the H.264 decoder gives no pictures back: asking everyone for JPEG",
            s.name
        );
        send_message_to(s, &all_peers(s), &video_wire::encode_caps(false));
    }
    (new_tile, picture)
}

/// A video control message from the peer behind connection `peer`. True when the window changes.
fn apply_video_control(s: &mut MeetState, peer: u64, control: Control) -> bool {
    match control {
        Control::KeyframeRequest { track } => {
            let Some(slot) = track_slot(track) else {
                return false;
            };
            s.video_out[slot].keyframes.request();
            let node_id = s
                .remotes
                .iter()
                .find(|r| r.handle == peer)
                .map(|r| r.node_id.clone())
                .unwrap_or_default();
            let who = remote_name(s, &node_id);
            eprintln!(
                "[azmeet] {}: {who} asked for a keyframe ({})",
                s.name, SOURCES[slot]
            );
            false
        }
        Control::Received { track, seq } => {
            let slot = track_slot(track);
            let remote = s.remotes.iter_mut().find(|r| r.handle == peer);
            if let (Some(slot), Some(remote)) = (slot, remote) {
                remote.sent[slot].acked(seq);
            }
            false
        }
        Control::Caps { h264 } => {
            let Some(remote) = s.remotes.iter_mut().find(|r| r.handle == peer) else {
                return false;
            };
            if remote.h264 == Some(h264) {
                return false;
            }
            remote.h264 = Some(h264);
            // The peer's H.264 starts afresh, at a keyframe.
            remote.sent = [video_wire::SendWindow::new(), video_wire::SendWindow::new()];
            let node_id = remote.node_id.clone();
            eprintln!(
                "[azmeet] {}: {} decodes H.264: {}",
                s.name,
                remote_name(s, &node_id),
                if h264 { "yes" } else { "no" }
            );
            true
        }
    }
}

/// Every pump while the camera or the screen share is a test pattern: the frame due now, sent like
/// a captured one.
fn pump_pattern(s: &mut MeetState) {
    if !s.pattern_video || s.remotes.is_empty() {
        return;
    }
    let elapsed = now_ms(s);
    for (slot, track, on) in [(0, CAMERA_TRACK, s.cam_on), (1, SCREEN_TRACK, s.screen_on)] {
        if !on {
            continue;
        }
        let Some(index) = s.pattern_clocks[slot].next(elapsed) else {
            continue;
        };
        // The screen's bars sit half a frame further on, so the two tiles differ.
        let frame =
            pattern_frame(index.wrapping_add(slot as u32 * FEED_W / 2 / video_wire::PATTERN_STEP));
        send_video(s, track, frame.width, frame.height, &frame.bytes);
    }
}

/// The devices panel's video lines: what each local track sent, and what arrived on each peer's
/// tracks.
fn video_lines(s: &MeetState) -> Vec<String> {
    let mut lines = Vec::new();
    for (slot, out) in s.video_out.iter().enumerate() {
        if out.h264_packets + out.jpeg_frames + out.dropped > 0 {
            lines.push(video_wire::send_line(
                SOURCES[slot],
                out.h264_packets,
                out.jpeg_frames,
                &out.keyframes.stats(),
                out.dropped,
            ));
        }
    }
    for r in &s.remotes {
        for (slot, input) in r.received.iter().enumerate() {
            let stats = input.rules.stats();
            let Some(codec) = input.rules.codec().or(input.seen) else {
                continue;
            };
            if stats.packets > 0 {
                let name = remote_name(s, &r.node_id);
                lines.push(video_wire::video_line(&name, SOURCES[slot], codec, &stats));
            }
        }
    }
    lines
}

/// The periodic log: the video lines, and per peer and track how far this side runs ahead and
/// what arrived late.
fn video_log_lines(s: &MeetState) -> Vec<String> {
    let mut lines = vec![codec_status(s)];
    lines.extend(video_lines(s));
    for r in &s.remotes {
        let name = remote_name(s, &r.node_id);
        for slot in 0..SOURCES.len() {
            let window = &r.sent[slot];
            let late = r.received[slot].rules.stats().late;
            if window.in_flight() + window.skipped() as usize + late as usize > 0 {
                lines.push(format!(
                    "{} to {name}: {} in flight, {} not sent; from {name}: {late} late",
                    SOURCES[slot],
                    window.in_flight(),
                    window.skipped()
                ));
            }
        }
    }
    lines
}

/// The "Drop a video packet" button (`AZMEET_TEST_PATTERN=1`): the next packet is not sent, so
/// the others see a gap.
extern "C" fn on_drop_video_packet(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        s.drop_next_video = true;
    }
    Update::DoNothing
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
            .with_interval(Duration::System(SystemTimeDiff::from_millis(PUMP_MS))),
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
    for out in s.video_out.iter_mut() {
        out.stop();
    }
    s.drop_next_video = false;
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
extern "C" fn cam_toggle(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        s.cam_on = !s.cam_on;
        if let (false, Some(slot)) = (s.cam_on, track_slot(CAMERA_TRACK)) {
            s.video_out[slot].stop();
        }
    }
    Update::RefreshDom
}
extern "C" fn screen_toggle(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        s.screen_on = !s.screen_on;
        if let (false, Some(slot)) = (s.screen_on, track_slot(SCREEN_TRACK)) {
            s.video_out[slot].stop();
        }
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

/// The meeting server to use, or the line the demo shows instead.
fn meeting_server() -> Result<String, String> {
    let configured = std::env::var("AZMEET_WORKER")
        .ok()
        .map(|url| url.trim().to_string())
        .filter(|url| !url.is_empty())
        .or_else(|| (!PRODUCTION_WORKER.is_empty()).then(|| PRODUCTION_WORKER.to_string()));
    let Some(url) = configured else {
        return Err(String::from(
            "Local demo: no meeting server is set. Start AzMeet with AZMEET_WORKER=<url> to meet \
             other people.",
        ));
    };
    let url = url.trim_end_matches('/').to_string();
    match probe(&url) {
        Ok(()) => Ok(url),
        Err(e) => Err(format!(
            "Local demo: the meeting server at {url} does not answer ({e})."
        )),
    }
}

pub fn start() {
    match meeting_server() {
        Ok(worker) => start_rooms(worker),
        Err(reason) => start_demo(&reason),
    }
}

/// One window with the start screen, talking to the meeting server at `worker`.
fn start_rooms(worker: String) {
    let host = server_address(&worker)
        .map(|(host, _)| host)
        .unwrap_or_default();
    let relay = rooms::relay_choice(std::env::var("AZMEET_RELAY").ok().as_deref(), &host);
    let name = display_name();
    let endpoint = bind_endpoint(&relay);
    let mut me = MeetState::new(&name, "", "");
    let mut room = RoomSession::new(worker.clone(), name.clone());
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
        vec![RefAny::new(solo)]
    };
    let linked = peers.len() == 2;
    run(peers, linked);
}

fn run(peers: Vec<RefAny>, linked: bool) {
    let mut app = App::create(RefAny::new(Room { peers }), AppConfig::create());
    let mut first = WindowCreateOptions::create(layout_first);
    first.create_callback = Some(Callback::create(startup_first)).into();
    if linked {
        first.window_state.size.dimensions = LogicalSize::create(740.0, 640.0);
        first.window_state.title = AzString::from("AzMeet · Ada (CPU)");
        first.window_state.position =
            WindowPosition::Initialized(PhysicalPositionI32 { x: 20, y: 40 });
        first.renderer = renderer(HwAcceleration::Disabled);
        let mut second = WindowCreateOptions::create(layout_second);
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
