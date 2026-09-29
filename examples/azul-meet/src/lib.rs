//! AzMeet: video meetings over azul.iroh.
//!
//! With a meeting server (the `meet` Worker, azul-apps `cf-workers/meet`) AzMeet opens a start
//! screen: "New meeting" asks the server for a room and shows its link; "Join with a link" takes
//! that link. Either way the app announces its iroh ticket to the room every 20 seconds, reads
//! everyone else's every 2 seconds, and dials the peers whose endpoint id is higher than its own.
//! Every HTTP request runs on an azul `Thread` and resumes on the UI thread, so no callback waits
//! on the network. Once connected, every captured frame is JPEG-encoded and sent to each peer as
//! one QUIC stream, and each peer's frames are decoded into its own tiles.
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

mod rooms;

use std::collections::{BTreeMap, BTreeSet};

use azul::{
    app::RendererOptions,
    audio::{AudioConfig, AudioDeviceList, AudioDeviceListResult, AudioFrame},
    callbacks::{CallbackInfo, TimerCallbackInfo, TimerCallbackReturn, UpdateImageType},
    camera::CameraConfig,
    css::{LogicalSize, PhysicalPositionI32, Srgb, WindowPosition},
    dom::{Callback, ClipboardContent, DomNodeId, NodeId},
    error::{HttpError, ResultRawImageDecodeImageError, ResultU8VecEncodeImageError},
    http::{HttpGetResult, HttpMethod, HttpRequestConfig},
    image::{ImageRef, RawImage, RawImageData, RawImageFormat},
    iroh::{IrohConfig, IrohEndpoint, IrohEvent, IrohEventKind, IrohRelayMode},
    json::{Json, JsonKeyValue},
    option::{OptionRendererOptions, OptionString},
    prelude::*,
    screen::ScreenCaptureConfig,
    str::String as AzString,
    task::{Thread, ThreadId, ThreadReceiver, ThreadSender, Timer, TimerId},
    time::{Duration, SystemTimeDiff},
    url::Url,
    vec::{StyledTextRunVec, U8Vec, U8VecRef},
    widgets::{
        ButtonType, CameraWidget, ConsumerFrame, FrameConsumer, MicrophoneWidget,
        OnTextInputReturn, ProgressBar, ScreenCaptureWidget, TextInputState, TextInputValid,
    },
    window::{HwAcceleration, Vsync},
};
use rooms::{Dialed, PeerRecord, Relay, RoomKey};

const ALPN: &str = "azmeet/mjpeg/1";
const CAMERA_TRACK: u32 = 1;
const SCREEN_TRACK: u32 = 2;
const FEED_W: u32 = 320;
const FEED_H: u32 = 180;
const JPEG_QUALITY: u8 = 75;
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
}

impl RoomSession {
    fn new(worker: String, name: String) -> Self {
        RoomSession {
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
    let mut rows = vec![format!("{} (you)", s.name)];
    for p in &room.peers {
        let status = if s.remotes.iter().any(|r| r.node_id == p.node_id) {
            "connected"
        } else if rooms::dials(&room.node_id, &p.node_id) {
            "connecting"
        } else {
            "waiting for them to connect"
        };
        rows.push(format!("{} · {}", p.name, status));
    }
    // Connected peers whose record expired from the server stay listed.
    for r in &s.remotes {
        if !room.peers.iter().any(|p| p.node_id == r.node_id) {
            rows.push(format!("{} · connected", short_id(&r.node_id)));
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
    let self_tile = if view.cam {
        Dom::create_div().with_css(TILE).with_child(
            CameraWidget::create(CameraConfig::default())
                .with_consumer(feed_consumer(CAMERA_TRACK))
                .with_on_consumer_frame(data.clone(), send_feed_frame)
                .dom()
                .with_css("width: 100%; height: 100%;"),
        )
    } else {
        Dom::create_div()
            .with_css(TILE)
            .with_child(Dom::create_span_with_text("You · camera off"))
    };

    let mut grid = Dom::create_div().with_css(
        "display: flex; flex-wrap: wrap; flex-grow: 1; align-content: flex-start; \
         justify-content: center; padding: 12px;",
    );
    grid = grid.with_child(self_tile);
    if view.screen {
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

    let toolbar = Dom::create_div()
        .with_css("display: flex; justify-content: center; padding: 14px; background: #15151c;")
        .with_child(
            Dom::create_div()
                .with_css(if view.mic { BTN_ON } else { BTN })
                .with_child(Dom::create_span_with_text(if view.mic {
                    "Mute"
                } else {
                    "Unmute mic"
                }))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::MouseUp),
                    data.clone(),
                    mic_toggle,
                ),
        )
        .with_child(
            Dom::create_div()
                .with_css(if view.cam { BTN_ON } else { BTN })
                .with_child(Dom::create_span_with_text(if view.cam {
                    "Stop video"
                } else {
                    "Start video"
                }))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::MouseUp),
                    data.clone(),
                    cam_toggle,
                ),
        )
        .with_child(
            Dom::create_div()
                .with_css(if view.screen { BTN_ON } else { BTN })
                .with_child(Dom::create_span_with_text(if view.screen {
                    "Stop share"
                } else {
                    "Share screen"
                }))
                .with_callback(
                    EventFilter::Hover(HoverEventFilter::MouseUp),
                    data.clone(),
                    screen_toggle,
                ),
        );

    let link_line = if view.linked {
        format!("{FEED_W}x{FEED_H} JPEG over iroh · {}", view.link_status)
    } else {
        view.link_status.clone()
    };
    let devices_panel = Dom::create_div()
        .with_css(
            "display: flex; justify-content: center; padding: 10px 12px 16px 12px; background: \
             #0e0e14; border-top: 1px solid #222;",
        )
        .with_child(device_col("Microphones", &view.mics))
        .with_child(device_col("Speakers", &view.speakers))
        .with_child(device_col("Video link", &[link_line]));

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
    if view.mic {
        body = body.with_child(
            MicrophoneWidget::create(AudioConfig {
                sample_rate: 48_000,
                channels: 1,
            })
            .with_on_frame(data.clone(), mic_on_frame)
            .dom()
            .with_css("width: 1px; height: 1px; overflow: hidden;"),
        );
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

extern "C" fn send_feed_frame(
    mut data: RefAny,
    _info: CallbackInfo,
    frame: ConsumerFrame,
) -> Update {
    let track = frame.consumer.id;
    if track_slot(track).is_none() {
        return Update::DoNothing;
    }
    let Some(endpoint) = data
        .downcast_ref::<MeetState>()
        .filter(|s| !s.remotes.is_empty())
        .and_then(|s| s.endpoint.clone())
    else {
        return Update::DoNothing;
    };
    let image = RawImage {
        pixels: RawImageData::U8(frame.frame.bytes.clone()),
        width: frame.frame.width as usize,
        height: frame.frame.height as usize,
        premultiplied_alpha: false,
        data_format: RawImageFormat::RGBA8,
        tag: U8Vec::create(),
    };
    if let ResultU8VecEncodeImageError::Ok(jpeg) = image.encode_jpeg(JPEG_QUALITY) {
        endpoint.broadcast_frame(track, jpeg);
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
            eprintln!("[azmeet] {}: connected to {}", s.name, short_id(&node_id));
            s.link_status = format!("connected to {}", short_id(&node_id));
            if !s.remotes.iter().any(|r| r.handle == event.peer) {
                s.remotes.push(Remote {
                    handle: event.peer,
                    node_id,
                    tracks: [false; 2],
                });
            }
            true
        }
        IrohEventKind::PeerDisconnected => {
            let Some(pos) = s.remotes.iter().position(|r| r.handle == event.peer) else {
                return false;
            };
            let gone = s.remotes.remove(pos);
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
        _ => false,
    }
}

fn show_remote_frame(data: &mut RefAny, info: &mut TimerCallbackInfo, frame: &IrohEvent) -> bool {
    let Some(slot) = track_slot(frame.track) else {
        return false;
    };
    let Some(shown) = data.downcast_mut::<MeetState>().and_then(|mut s| {
        let remote = s.remotes.iter_mut().find(|r| r.handle == frame.peer)?;
        let shown = remote.tracks[slot];
        remote.tracks[slot] = true;
        Some(shown)
    }) else {
        return false;
    };
    if !shown {
        return true;
    }
    let ResultRawImageDecodeImageError::Ok(decoded) =
        RawImage::decode_image_bytes_any(U8VecRef::from(frame.data.as_ref()))
    else {
        return false;
    };
    let Some(image) = ImageRef::create_rawimage(decoded).into_option() else {
        return false;
    };
    let marker = tile_marker(frame.peer, frame.track);
    let Some(node) = info
        .callback_info
        .get_node_id_by_marker(AzString::from(marker.as_str()))
        .into_option()
    else {
        return false;
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
    false
}

extern "C" fn pump_link(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let Some(endpoint) = data
        .downcast_ref::<MeetState>()
        .and_then(|s| s.endpoint.clone())
    else {
        return TimerCallbackReturn::continue_unchanged();
    };
    let mut refresh = false;
    let mut frames = Vec::new();
    while let Some(event) = endpoint.recv().into_option() {
        if event.kind == IrohEventKind::Frame {
            frames.push(event);
        } else if let Some(mut s) = data.downcast_mut::<MeetState>() {
            refresh |= apply_link_event(&mut s, &event);
        }
    }
    for frame in &frames {
        refresh |= show_remote_frame(&mut data, &mut info, frame);
    }
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
            refresh = true;
        }
    }
    if refresh {
        TimerCallbackReturn::continue_and_refresh_dom()
    } else {
        TimerCallbackReturn::continue_unchanged()
    }
}

// ==== Meeting server: requests on an azul Thread, answers on the UI thread ====

/// The callback a finished request resumes into, on the UI thread.
type ResumeFn = extern "C" fn(RefAny, CallbackInfo, RefAny) -> Update;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Verb {
    Get,
    Post,
}

/// One request to the meeting server.
struct HttpJob {
    verb: Verb,
    url: String,
    body: String,
    on_result: ResumeFn,
}

impl HttpJob {
    fn create_room(worker: &str) -> Self {
        HttpJob {
            verb: Verb::Post,
            url: format!("{worker}/rooms"),
            body: String::from("{}"),
            on_result: on_room_opened,
        }
    }

    fn look_up(worker: &str, key: &RoomKey) -> Self {
        HttpJob {
            verb: Verb::Get,
            url: format!("{worker}/rooms/{}?format=json", key.as_str()),
            body: String::new(),
            on_result: on_room_opened,
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
        }
    }
}

struct HttpThreadInit {
    job: HttpJob,
    /// The participant's `MeetState`, handed back to `job.on_result`.
    app: RefAny,
}

/// Runs one request on a worker thread. `http_request` blocks here, then queues its answer,
/// which the UI thread delivers to `on_result` on its next pump (the 15 ms link timer).
extern "C" fn http_thread(mut init: RefAny, _sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((verb, url, body, on_result, app)) = init.downcast_ref::<HttpThreadInit>().map(|i| {
        (
            i.job.verb,
            i.job.url.clone(),
            i.job.body.clone(),
            i.job.on_result,
            i.app.clone(),
        )
    }) else {
        return;
    };
    let method = match verb {
        Verb::Get => HttpMethod::Get,
        Verb::Post => HttpMethod::Post,
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
            app,
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
extern "C" fn on_room_opened(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let answer = http_answer(result);
    let follow_up = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let Some(room) = s.room.as_mut() else {
            return Update::DoNothing;
        };
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
extern "C" fn on_announced(mut data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let answer = http_answer(result);
    let (follow_up, update) = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let Some(room) = s.room.as_mut() else {
            return Update::DoNothing;
        };
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
extern "C" fn on_peers(mut data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
    let answer = http_answer(result);
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let Some(room) = s.room.as_mut() else {
        return Update::DoNothing;
    };
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
        HttpJob::create_room(&room.worker)
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
        HttpJob::look_up(&room.worker, &key)
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

extern "C" fn mic_on_frame(mut data: RefAny, mut info: CallbackInfo, frame: AudioFrame) -> Update {
    let level = mic_level_percent(frame.samples.as_ref()).round();
    let bar = {
        let Some(mut s) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        if (s.mic_level - level).abs() < 0.5 {
            return Update::DoNothing;
        }
        s.mic_level = level;
        let Some(bar) = s.meter_bar else {
            return Update::DoNothing;
        };
        bar
    };
    ProgressBar::update_progress(info, bar, level);
    info.set_accessibility_value(bar, format!("{level:.0}%"));
    Update::DoNothing
}

extern "C" fn mic_toggle(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        s.mic_on = !s.mic_on;
    }
    Update::RefreshDom
}
extern "C" fn cam_toggle(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        s.cam_on = !s.cam_on;
    }
    Update::RefreshDom
}
extern "C" fn screen_toggle(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        s.screen_on = !s.screen_on;
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

/// The meeting server to use, or why there is none.
fn meeting_server() -> Result<String, String> {
    let configured = std::env::var("AZMEET_WORKER")
        .ok()
        .map(|url| url.trim().to_string())
        .filter(|url| !url.is_empty())
        .or_else(|| (!PRODUCTION_WORKER.is_empty()).then(|| PRODUCTION_WORKER.to_string()));
    let Some(url) = configured else {
        return Err(String::from("no meeting server is set (AZMEET_WORKER)"));
    };
    let url = url.trim_end_matches('/').to_string();
    match probe(&url) {
        Ok(()) => Ok(url),
        Err(e) => Err(format!("the meeting server at {url} is unreachable ({e})")),
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
    run(vec![RefAny::new(me)], false);
}

/// The in-process demo: two participants linked by two iroh endpoints, or one without a link.
fn start_demo(reason: &str) {
    let meeting = gen_link();
    let notice =
        format!("Local demo, no meeting server: {reason}. Set AZMEET_WORKER to meet other people.");
    eprintln!("[azmeet] {notice}");
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
        vec![RefAny::new(ada), RefAny::new(ben)]
    } else {
        let failure = bind_failure(&ada_link);
        eprintln!("[azmeet] joined meeting {meeting} without a peer link: {failure}");
        let mut solo = MeetState::new("You", "", "");
        solo.meeting = meeting;
        solo.notice = notice;
        solo.link_status = failure;
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
