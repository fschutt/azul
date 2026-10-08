//! AzMeet: video meetings and chat rooms over azul.iroh, end-to-end encrypted.
//!
//! AzMeet talks to a meeting server (the `meet` Worker, azul-apps `cf-workers/meet`) that it does
//! not trust with anything readable (`CRYPTO.md`): every device has its own identity (an Ed25519
//! and an X25519 key from one seed in the OS keyring, `identity.rs`), every room a link with an
//! invite secret in its fragment that never reaches the server, every message is sealed with a
//! room key sealed in turn to each member, and every iroh ticket is signed by its member's device.
//! The rules are `chatroom.rs` (pure), the primitives `crypto.rs`.
//!
//! The start screen: "New meeting" mints a room (its id and invite secret made here, registered
//! with the server), "Schedule" one with a start and an end, "New chat room" one that is kept
//! between calls; "Join with a link or a code" looks one up - a link joins, a code knocks and
//! waits for a member to let this device in. Under them "Your rooms": every room this device is
//! in (`meet/rooms.json`, `roomlist.rs`) with its unread count; one opens in the room view, its
//! chat, its members with their safety codes, and "Join call". A meeting opens in the waiting room
//! first (`ui.rs`): the camera preview, the switches, the devices, the name, the meeting's code,
//! link and times, who is in the call already, and "Start meeting" / "Join now" / "Ask to join".
//! Only then does the app announce its signed iroh ticket to the room every sixth of the
//! server's peer TTL (20 seconds), read everyone else's every 2 seconds, keep the tickets a
//! member signed, and dial the peers whose endpoint id is higher than its own. `--screen waiting`
//! opens a new meeting's waiting room (a preview of one when no meeting server answers); with
//! `--shot <png>` that is a screenshot. Every HTTP request runs on an azul `Thread` and resumes on
//! the UI thread, so no callback waits on the network. The start screen's "Meeting server" field
//! holds the Worker's address: `--worker`, else the one saved last time, else the shared Azlin
//! config's (azul-appkit `azlin_config`: `AZMEET_WORKER`, else `endpoints.meet` of the config
//! file `AZLIN_CONFIG` names, else of `~/.azlin/config.json`), else one built in at build time,
//! else the config profile's (`local`, the default: the local stack's `http://127.0.0.1:8790`;
//! `production` names none yet) - and nothing else: with none, the start screen asks for one,
//! and an unreachable one is an error with a Retry button. A new address is used for every
//! request from Enter or leaving the field on, checked with `GET /health`, and saved once it
//! answers. The relays come the same way (`--relay`, `AZMEET_RELAY`, `endpoints.relay`, the
//! profile's).
//!
//! Settings: the gear at the top right of every screen (or Mod+,) opens azul-appkit's settings
//! page, the one every Azlin app shares (AzMail's File > Options): AzMeet's Audio & Video (the
//! devices, mirror my video, the video quality), Meetings (the name, joining with the microphone
//! or the camera off) and Recording, then the kit's Appearance, Data, Shortcuts and About.
//!
//! Files (see `store.rs`): in the Azlin data tree (azul-appkit's data root: `--data-dir`,
//! `AZLIN_DATA`, else `<data dir>/Azlin`), `meet/settings.json` keeps the app theme and mode and
//! AzMeet's settings (the kit writes it: `AZMEET_SETTINGS_SAVED`); `meet/rooms.json` the rooms
//! this device is in (their invite secrets sealed with this device's key); each meeting has a
//! folder `meet/<meeting>/` with `meeting.json` (its link, server, when this side joined, who was
//! there) and `chat.jsonl` (the chat as read here, one message per line: the user's own record of
//! it). Every write runs on an azul Thread through azul-storage's `LocalDrive`; stdout says
//! `AZMEET_SAVED <key>` for each. The meeting server keeps the chat history only as ciphertext.
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
//! Chat (`chatroom.rs`): a room's chat is kept by the meeting server as ciphertext, read every 2
//! seconds while the room is open (in a call or the room view) and every 15 seconds otherwise
//! (the unread counts of "Your rooms"). stdout, for scripts: `AZMEET_CHAT <name>: <text>` per
//! message from someone else, `AZMEET_KEY <room> epoch=<n> key=<id> members=<n> by=<who>` for
//! every room key made or taken, `AZMEET_MEMBER <room> <joined|left|knocking> <name> <safety
//! code>`, `AZMEET_HISTORY <room> <n>` once a room's history is read, `AZMEET_IDENTITY <device>
//! <keyring|file|session>` and `AZMEET_SAFETY <code>` once this device's identity is loaded.
//!
//! Switches (`args.rs`, `--help` lists them): each AzMeet switch also reads its `AZMEET_*`
//! environment variable when it is not given (`1` for a switch without a value), so older scripts
//! keep working; the switch wins.
//! - `--worker <url>` (`AZMEET_WORKER`): the meeting server, e.g. `http://127.0.0.1:8790` (the
//!   local stack's `wrangler dev`). `--worker` wins over the one saved from the start screen; the
//!   variable and the shared config only count when none was saved. Else the `PRODUCTION_WORKER`
//!   constant, set at build time with `AZMEET_DEFAULT_WORKER=<url>`, else the config profile's
//!   address, else none: the start screen asks for one. A headless run (`AZ_BACKEND=headless`)
//!   keeps no files unless it is given a data root (`--data-dir`, `AZLIN_DATA`), so without one
//!   nothing saved outranks the configuration there.
//! - `--identity-file <path>` (`AZMEET_IDENTITY_FILE`): keep this device's seed in that file
//!   (made with mode 0600) instead of the system keyring - for tests (a headless run's keyring
//!   lives in memory) and unattended machines.
//! - `--starts-at <time>` / `--ends-at <time>` (`AZMEET_STARTS_AT`, `AZMEET_ENDS_AT`, RFC 3339):
//!   with `--autocreate`, the meeting's times.
//! - `--chat-room` (`AZMEET_CHAT_ROOM=1`): with `--autocreate`, a chat room instead of a meeting,
//!   opened in the room view; `--open <link>` (`AZMEET_OPEN`): open that room's view at start
//!   (joining it when this device is not in it yet).
//! - `--name <name>` (`AZMEET_NAME`, under the name typed last time): the name others see
//!   (default: `$USER`).
//! - `--autocreate` (`AZMEET_AUTOCREATE=1`): create a meeting at start, enter it without the
//!   waiting room and print `AZMEET_LINK <link>` on stdout (`--screen call` does the same;
//!   `--screen waiting` stops in the new meeting's waiting room, or in a preview of one when no
//!   meeting server answers).
//! - `--join <link>` (`AZMEET_JOIN`): join that meeting at start, without the waiting room.
//! - `--waiting-room` (`AZMEET_WAITING_ROOM=1`): with `--join` / `--autocreate`, stop in the
//!   waiting room (stdout `AZMEET_WAITING <link>`) until "Join now" / "Start meeting" is clicked.
//! - `--relay <off|default|url>` (`AZMEET_RELAY`, then the shared config file's
//!   `endpoints.relay`, as the local stack names its `iroh-relay --dev`); with none of them, off
//!   for a meeting server on this machine and the public iroh relays otherwise (every profile's
//!   built-in relay is the public one).
//! - `--relay-only` (`AZMEET_RELAY_ONLY=1`): never a direct path - no UDP socket, no hole
//!   punching, every packet through the relay (`IrohConfig::with_relay_only`); stderr says
//!   `relay only` with the endpoint, the statistics say `relayed` per peer.
//! - `--test-tone` (`AZMEET_TEST_TONE=1`): a 440 Hz tone replaces the microphone, which starts
//!   unmuted.
//! - `--no-echo-cancel` (`AZMEET_ECHO_CANCEL=0`): send the microphone as it is (with headphones);
//!   by default, while received audio plays on a device, the echo of what plays is cancelled from
//!   the microphone (`EchoCanceller`).
//! - `--test-pattern` (`AZMEET_TEST_PATTERN=1`): moving colour bars replace the camera (and the
//!   screen share), the camera starts on, and a "Drop a video packet" button drops the next packet
//!   before it leaves.
//! - `--video-codec jpeg` (`AZMEET_VIDEO_CODEC=jpeg`): send JPEG even where H.264 works.
//! - `--mesh-cap <n>` (`AZMEET_MESH_CAP`): rooms of up to n people send everything directly
//!   (default 4; the design's value is 8).
//! - `--uplink-kbps <kbit/s>` (`AZMEET_UPLINK_KBPS`): report this uplink instead of the estimate.
//! - `--no-forward` (`AZMEET_NO_FORWARD=1`): never forward other people's media; `--on-battery`
//!   (`AZMEET_ON_BATTERY=1`): report running on battery (either ranks this side last for the
//!   backbone).
//! - `--layout speaker` (`AZMEET_LAYOUT`) and `--stage <name>` (`AZMEET_STAGE`): start in speaker
//!   view with that participant pinned to the stage (else the active speaker, else the first); the
//!   controls switch between the gallery and the speaker view.
//! - `--panel <people|chat|statistics|closed>` (`AZMEET_PANEL`): what the side panel shows at
//!   start (the scripts open the statistics, whose lines they read).
//! - `AZ_BACKEND=headless` (the engine's, no switch): no audio device, camera or screen is
//!   opened: the microphone is the tone (muted until switched on, unless `--test-tone`), received
//!   audio is counted, not played, and the camera and the screen share are test patterns (off until
//!   switched on, unless `--test-pattern`).

/// What AzMeet's About says (azul-appkit's facts, azul's AboutDialog shows them).
pub(crate) const ABOUT: azul_appkit::AboutInfo = azul_appkit::AboutInfo {
    name: "AzMeet",
    version: env!("CARGO_PKG_VERSION"),
    summary: "Video meetings and chat rooms over azul.iroh, end-to-end encrypted: camera, screen \
              sharing, the people and a chat the meeting server cannot read; each meeting's \
              record and chat are files in the Azlin data tree.",
    license: "MIT",
    app_folder: store::APP_FOLDER,
};

mod args;
mod audio;
mod chat;
mod chatroom;
mod crypto;
mod identity;
mod ids;
/// A room's invite key from the invite secret of its link (CRYPTO.md section 4); public, since
/// AzCalendar's meeting links are made with it too (it includes the file).
pub mod invite;
mod keys;
mod pace;
mod rate;
mod roomlist;
mod rooms;
mod routes;
mod speaker;
mod store;
mod tiles;
mod ui;
mod video_wire;

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex, MutexGuard, PoisonError, Weak},
};

use azul::{
    audio::{
        AudioConfig, AudioDecoder, AudioDeviceList, AudioDeviceListResult, AudioEncoder,
        AudioFrame, AudioSink, EchoCanceller,
    },
    callbacks::{CallbackInfo, TimerCallbackInfo, TimerCallbackReturn, UpdateImageType},
    css::{DarkLightMode, LogicalSize},
    dom::{Callback, ClipboardContent, DomNodeId, NodeId, VirtualKeyCode},
    error::{
        HttpError, KeyringResult, ResultRawImageDecodeImageError, ResultU8VecEncodeImageError,
    },
    file::FilePath,
    http::{HttpGetResult, HttpMethod, HttpRequestConfig},
    image::{ImageRef, RawImage, RawImageData, RawImageFormat},
    iroh::{
        IrohConfig, IrohEndpoint, IrohEvent, IrohEventKind, IrohLoadBalancer, IrohPeerCapacity,
        IrohRelayMode, IrohTileRole,
    },
    json::Json,
    option::{OptionDarkLightMode, OptionKeyringResult, OptionString},
    prelude::*,
    str::String as AzString,
    task::{Thread, ThreadId, ThreadReceiver, ThreadSender, Timer, TimerId},
    time::{Duration, SystemTimeDiff},
    url::Url,
    vec::{F32Vec, StyledTextRunVec, U8Vec, U8VecRef},
    video::{VideoDecoder, VideoEncoder, VideoFrame},
    widgets::{
        CheckBoxState, ConsumerFrame, FrameConsumer, LevelMeter, LevelMeterThrottle,
        OnTextInputReturn, SegmentedState, TextInput, TextInputState, TextInputValid,
    },
    window::{PlatformCapability, WindowDecorations},
};
use azul_appkit::{azlin_config, ui as kit};
use rooms::{Dialed, PeerRecord, Relay, RoomKey};
use video_wire::{Codec, Control, Message};

/// The protocol name: peers of an older wire format (`azmeet/3`, whose chat travelled over the
/// iroh connections unsigned) cannot connect.
const ALPN: &str = "azmeet/4";
const CAMERA_TRACK: u32 = 1;
const SCREEN_TRACK: u32 = 2;
/// The audio track: 20 ms Opus or PCM packets, three to a frame (`audio.rs`).
const AUDIO_TRACK: u32 = 3;
/// The rate the microphone (or the test tone) is asked for.
const MIC_RATE: u32 = 48_000;
/// How long after it played an echo can still reach the microphone (ms): the output's and the
/// input's buffers, a laptop's speaker to its microphone, the room.
const ECHO_TAIL_MS: u32 = 300;
const TONE_HZ: f32 = 440.0;
const FEED_W: u32 = 320;
const FEED_H: u32 = 180;
/// The size of the test pattern's still in the own tile and the waiting room's preview.
const PATTERN_STILL: (u32, u32) = (640, 360);
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

/// The meeting server when nothing else names one: the deployed `meet` Worker (azul-apps
/// `cf-workers/meet/README.md`, "Deploy"), baked in at build time with
/// `AZMEET_DEFAULT_WORKER=https://...`. Empty means none: the start screen asks for one.
const PRODUCTION_WORKER: &str = match option_env!("AZMEET_DEFAULT_WORKER") {
    Some(url) => url,
    None => "",
};
/// How often a participant in a room reads the peers list.
const ROOM_POLL_MS: u64 = 2000;
/// Polls between two announcements until the Worker says its peer TTL (then a sixth of it).
const REANNOUNCE_POLLS: u32 = 10;
/// Polls after which a request that never answered is given up.
const STUCK_REQUEST_POLLS: u32 = 8;
const HTTP_TIMEOUT_SECS: u64 = 5;
/// How often the chat rooms are looked at for a request to send (`chat_tick`).
const CHAT_TICK_MS: u64 = 250;
/// How often a room is read while it is open (a call, the room view), and otherwise (the unread
/// counts of the start screen).
const CHAT_OPEN_SYNC_MS: u64 = 2000;
const CHAT_IDLE_SYNC_MS: u64 = 15_000;
/// How often the start screen asks a meeting server that did not answer again by itself.
const SERVER_RETRY_MS: u64 = 10_000;
/// What a chat line says for a member whose name does not open.
const UNKNOWN_NAME: &str = "Someone";

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
    /// Whether the peer decodes Opus audio, from its caps message; `None` until that arrives,
    /// and everyone gets 16-bit PCM until every peer said yes.
    opus: Option<bool>,
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
            opus: None,
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
    /// The room is found (or made): the waiting room shows it, nothing is announced yet.
    Waiting,
    /// In the room: announcing and polling.
    InRoom,
    /// The room is gone from the server; the peers already connected stay.
    Ended,
}

/// A participant's side of a meeting-server room.
struct RoomSession {
    worker: String,
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
    /// The meeting the waiting room shows (`Stage::Waiting`), entered with "Join now".
    waiting: Option<RoomInfo>,
    /// Who is in the waiting room's meeting already (its peers list, read every poll while
    /// waiting, nothing announced); `None` until the first answer.
    waiting_people: Option<Vec<String>>,
    /// A read of that list is in flight; `waiting_polls` counts the polls it has been.
    waiting_busy: bool,
    waiting_polls: u32,
    /// The waiting room is a preview: `--screen waiting` with no meeting server answering (a
    /// screenshot's), so its meeting exists only here and "Join now" says so.
    preview: bool,
    /// The meeting being opened or waited for was made by this side ("New meeting"), not looked
    /// up: the waiting room says "Start meeting".
    created: bool,
    /// The meeting being opened is entered at once, without the waiting room (`AZMEET_JOIN`,
    /// `AZMEET_AUTOCREATE`, `--screen call`: scripts).
    straight_in: bool,
    /// The invite secret of the link being looked up (its `#` fragment); `None` for a code, which
    /// knocks.
    join_secret: Option<String>,
    /// The room being made ("New meeting", "Schedule", "New chat room"): what `POST /rooms`
    /// registers.
    minting: Option<Minting>,
    /// The waiting room's knock went out ("Ask to join"): the meeting is entered once a member lets
    /// this device in.
    asked: bool,
    /// Polls between two announcements: a sixth of the Worker's peer TTL once it said it.
    reannounce_polls: u32,
    /// When (`wall_ms`) the meeting server was checked last, for the start screen's own retries.
    checked_at: u64,
    /// The start screen's "Schedule" form: the start (local time, `YYYY-MM-DD HH:MM`) and the
    /// minutes.
    schedule_start: String,
    schedule_minutes: String,
}

/// A room this side is making: its invite (the id and the secret made here, CRYPTO.md section 4),
/// its kind and its times (RFC 3339, as sent).
struct Minting {
    invite: crypto::Invite,
    kind: chatroom::RoomKind,
    times: Option<(String, String)>,
}

impl RoomSession {
    fn new(worker: String, relay: Relay) -> Self {
        RoomSession {
            waiting: None,
            waiting_people: None,
            waiting_busy: false,
            waiting_polls: 0,
            preview: false,
            created: false,
            straight_in: false,
            join_secret: None,
            minting: None,
            asked: false,
            reannounce_polls: REANNOUNCE_POLLS,
            checked_at: 0,
            schedule_start: String::new(),
            schedule_minutes: String::from("60"),
            session: 0,
            server_text: worker.clone(),
            server_status: String::new(),
            server_ok: false,
            checks: 0,
            relay,
            worker,
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
        self.waiting = None;
        self.close_waiting();
        self.straight_in = false;
        self.link = found.share_link();
        self.room_id = found.room;
        self.code = found.code;
        self.copied = false;
        self.peers.clear();
        self.dialed.clear();
        self.polls = 0;
        self.announced_at = None;
    }

    fn leave(&mut self) {
        self.session = self.session.wrapping_add(1);
        self.stage = Stage::Start;
        self.waiting = None;
        self.close_waiting();
        self.created = false;
        self.straight_in = false;
        self.minting = None;
        self.join_secret = None;
        self.room_id.clear();
        self.code.clear();
        self.link.clear();
        self.peers.clear();
        self.dialed.clear();
        self.announced_at = None;
        self.busy = false;
    }

    /// The waiting room is left (joined, Back, or the meeting left): who was in its meeting is
    /// forgotten, and a read of that still in flight is not waited for.
    fn close_waiting(&mut self) {
        self.waiting_people = None;
        self.waiting_busy = false;
        self.waiting_polls = 0;
        self.preview = false;
        self.asked = false;
    }

    /// In the waiting room: a read of its meeting's peers list (who is in it already), nothing
    /// announced; one at a time, a stuck one given up after `STUCK_REQUEST_POLLS`. None for a
    /// preview, whose meeting no server knows.
    fn waiting_job(&mut self) -> Option<HttpJob> {
        if self.stage != Stage::Waiting || self.preview || self.waiting.is_none() {
            return None;
        }
        if self.waiting_busy {
            self.waiting_polls += 1;
            if self.waiting_polls < STUCK_REQUEST_POLLS {
                return None;
            }
        }
        self.waiting_busy = true;
        self.waiting_polls = 0;
        Some(HttpJob::waiting_peers(self))
    }

    /// The announcement to send right away, when the ticket is already known and this device is a
    /// member of the room (`signer`: its identity and the server's time; the Worker takes a signed
    /// announcement from a member only).
    fn first_job(&mut self, signer: Option<(&crypto::Identity, u64)>) -> Option<HttpJob> {
        let (me, ts) = signer?;
        if self.stage != Stage::InRoom || self.busy || self.ticket.is_empty() {
            return None;
        }
        self.busy = true;
        self.busy_polls = 0;
        self.announced_at = Some(self.polls);
        Some(HttpJob::announce(self, me, ts))
    }

    /// What this poll sends: an announcement when one is due and this device can sign one (it is
    /// a member), else a read of the peers list; in the waiting room only that read
    /// (`waiting_job`).
    fn next_job(&mut self, signer: Option<(&crypto::Identity, u64)>) -> Option<HttpJob> {
        if self.stage == Stage::Waiting {
            return self.waiting_job();
        }
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
        let every = self.reannounce_polls.max(1);
        let due = self
            .announced_at
            .map_or(true, |at| polls.wrapping_sub(at) >= every);
        self.busy = true;
        self.busy_polls = 0;
        match signer {
            Some((me, ts)) if due => {
                self.announced_at = Some(polls);
                Some(HttpJob::announce(self, me, ts))
            }
            _ => Some(HttpJob::poll(self)),
        }
    }
}

/// A pending connection: a peer dialed this side before its signed announcement was read here
/// (CRYPTO.md section 10). Nothing is sent to it, nothing it sends is taken, until a read of the
/// peers list confirms it; it is dropped after `PENDING_POLLS` reads that do not.
struct Pending {
    handle: u64,
    node_id: String,
    polls: u32,
    /// What it said meanwhile about itself (its capabilities, its report, its call state): a peer
    /// says each once, when it took the connection, so dropping them leaves this side sending it
    /// only audio and never learning what it decodes. Replayed when it is confirmed.
    held: Vec<Vec<u8>>,
}

/// Reads of the peers list a pending connection may wait for its announcement.
const PENDING_POLLS: u32 = 3;

/// The messages a pending connection's [`Pending::held`] keeps (the newest).
const PENDING_HELD: usize = 32;

struct MeetState {
    name: String,
    endpoint: Option<IrohEndpoint>,
    remotes: Vec<Remote>,
    /// Connections waiting for their signed announcement (`Pending`).
    pending: Vec<Pending>,
    link_status: String,
    /// One line under the header: what the meeting server said, what a room needs.
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
    /// When the level meter may move: at most every `METER_INTERVAL_MS`, not with every 20 ms
    /// audio chunk, and by half a percent or more (azul's `LevelMeterThrottle`).
    meter: LevelMeterThrottle,
    meter_bar: Option<DomNodeId>,
    mics: Vec<String>,
    speakers: Vec<String>,
    devices_requested: bool,
    /// The meeting server, the start screen's form, the waiting room and the call: there once the
    /// window is up (`start`).
    room: Option<RoomSession>,
    /// This participant hears nobody: received audio is dropped.
    deafened: bool,
    /// The microphone is the test tone (`AZMEET_TEST_TONE=1`, and every headless run).
    tone_mic: bool,
    /// The tone while the microphone is on, and when it started.
    tone: Option<(audio::ToneSource, std::time::Instant)>,
    packetizer: audio::Packetizer,
    /// This side's Opus encoder, opened with the first audio sent at a rate (mono): the rate it
    /// was opened for, and the encoder (`None` where it does not open; not asked again at that
    /// rate).
    opus_out: Option<(u32, Option<AudioEncoder>)>,
    /// Numbers this side's Opus packets and puts three in a frame.
    opus_framer: audio::OpusFramer,
    /// The codec this side's audio went out in last (`audio::CODEC_OPUS` / `CODEC_PCM16`).
    audio_codec_out: Option<u8>,
    /// This machine decodes Opus (an `AudioDecoder` opens here): said in the caps.
    decodes_opus: bool,
    /// Each peer's Opus decoder and which of its packets were decoded, by origin key.
    opus_in: BTreeMap<u64, (AudioDecoder, audio::OpusOrder)>,
    /// The codec of each origin's last audio frame: a change starts its jitter buffer over (the
    /// two codecs number their packets apart).
    audio_codec_in: BTreeMap<u64, u8>,
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
    /// A still of the test pattern, made once with `pattern_video`: the own camera tile and the
    /// waiting room's preview show it as a camera would show its picture.
    pattern_still: Option<ImageRef>,
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
    /// This device's identity (CRYPTO.md section 3): `None` until the keyring (or the identity
    /// file) answered; nothing is signed, sealed or joined before.
    identity: Option<crypto::Identity>,
    /// Where the identity's seed lives.
    identity_source: Option<identity::Source>,
    /// What the keyring was asked last (its answer arrives as a window event).
    keyring: Option<KeyringStep>,
    /// The start's own steps (`--join`, `--autocreate`, `--open`) wait for the identity.
    autostart_due: bool,
    /// Every room this device is in, and the one being looked at, by room id (`chatroom.rs`):
    /// its members, keys and the chat, ciphertext on the meeting server.
    chats: BTreeMap<String, chatroom::ChatRoom>,
    /// `meet/rooms.json` as this device knows it (`roomlist.rs`).
    index: roomlist::RoomIndex,
    /// The room the room view shows; `None` outside it.
    open_room: Option<String>,
    /// The room view's "Copy link" was clicked.
    room_copied: bool,
    /// Who is on the stage of the speaker view, from the levels of the audio each peer sends.
    speaker: speaker::ActiveSpeaker,
    /// What the call's side panel shows.
    panel: SidePanel,
    /// The chat field as typed.
    chat_draft: String,
    /// This participant's azul-appkit kit (`make_kit`): the settings page (open or not, its
    /// category) and `meet/settings.json` - the one copy of the settings, which the kit writes.
    kit: RefAny,
    /// AzMeet's settings changed in the kit since they were written last (`save_prefs`).
    prefs_unsaved: bool,
    /// The devices picked in the settings: an index into "System default" + the microphones /
    /// speakers, and into `CAMERAS` (the camera's facing).
    mic_choice: usize,
    speaker_choice: usize,
    camera_choice: usize,
    /// The microphone and the speaker remembered, by name (`None`: the system's default): the
    /// pickers find them again once the devices are listed.
    mic_name: Option<String>,
    speaker_name: Option<String>,
    /// The video quality this side asks for (`ui::QUALITY_LABELS`): automatic, data saver, low.
    quality: usize,
    /// This side's own camera is shown mirrored.
    mirror: bool,
    /// The waiting room starts with the microphone off, with the camera off.
    join_muted: bool,
    join_camera_off: bool,
    /// The meeting this side is in, as `meet/<meeting>/meeting.json` says; `None` outside one.
    record: Option<store::MeetingRecord>,
    /// The files that changed since they were written last (`flush_files`).
    unsaved: Unsaved,
    /// The rooms whose files of an earlier visit are being read back (`on_history_read`): their
    /// chat and record are not written over meanwhile.
    reading_history: BTreeSet<String>,
}

/// What the keyring was asked (`identity.rs`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum KeyringStep {
    /// The seed, at start.
    Get,
    /// A new seed, kept.
    Store,
}

/// The files of the data tree that changed since the last [`flush_files`] (the settings file is
/// the kit's: [`remember`]).
#[derive(Debug, Default, Clone)]
struct Unsaved {
    record: bool,
    /// The rooms whose `chat.jsonl` changed.
    chats: BTreeSet<String>,
    /// `meet/rooms.json`.
    index: bool,
}

impl MeetState {
    fn new(name: &str, kit: RefAny) -> Self {
        MeetState {
            kit,
            prefs_unsaved: false,
            name: name.to_string(),
            endpoint: None,
            remotes: Vec::new(),
            pending: Vec::new(),
            link_status: String::from("binding"),
            notice: String::new(),
            stats_at_ms: 0,
            stats_shown: Vec::new(),
            pace: pace::PumpPace::new(),
            mic_on: false,
            cam_on: false,
            screen_on: false,
            mic_level: 0.0,
            meter: LevelMeterThrottle::create(METER_INTERVAL_MS),
            meter_bar: None,
            mics: Vec::new(),
            speakers: Vec::new(),
            devices_requested: false,
            room: None,
            deafened: false,
            tone_mic: false,
            tone: None,
            packetizer: audio::Packetizer::new(),
            opus_out: None,
            opus_framer: audio::OpusFramer::new(),
            audio_codec_out: None,
            decodes_opus: AudioDecoder::create(AudioConfig {
                sample_rate: audio::OPUS_RATE,
                channels: 1,
            })
            .is_open(),
            opus_in: BTreeMap::new(),
            audio_codec_in: BTreeMap::new(),
            playout: None,
            play_audio: true,
            clock: std::time::Instant::now(),
            video: VideoSupport {
                encoder: Err(String::from("not probed")),
                decodes_h264: false,
            },
            video_out: BTreeMap::new(),
            pattern_video: false,
            pattern_still: None,
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
            identity: None,
            identity_source: None,
            keyring: None,
            autostart_due: true,
            chats: BTreeMap::new(),
            index: roomlist::RoomIndex::default(),
            open_room: None,
            room_copied: false,
            speaker: speaker::ActiveSpeaker::new(),
            panel: SidePanel::People,
            chat_draft: String::new(),
            mic_choice: 0,
            speaker_choice: 0,
            camera_choice: 0,
            mic_name: None,
            speaker_name: None,
            quality: 0,
            mirror: true,
            join_muted: false,
            join_camera_off: false,
            record: None,
            unsaved: Unsaved::default(),
            reading_history: BTreeSet::new(),
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
        // The pickers show the devices remembered, where they are still there.
        s.mic_choice = device_index(&mics, s.mic_name.as_deref());
        s.speaker_choice = device_index(&speakers, s.speaker_name.as_deref());
        s.mics = mics;
        s.speakers = speakers;
    }
    Update::RefreshDom
}

/// The picker index of the device named `name` among `devices` ("System default" is 0, the
/// devices follow); 0 for no name, or a device no longer there.
fn device_index(devices: &[String], name: Option<&str>) -> usize {
    name.and_then(|name| devices.iter().position(|d| d == name))
        .map_or(0, |i| i + 1)
}

/// The device name at picker index `index` (`device_index`'s inverse); `None` for the system's
/// default.
fn device_name(devices: &[String], index: usize) -> Option<String> {
    index.checked_sub(1).and_then(|i| devices.get(i)).cloned()
}

extern "C" fn layout(data: RefAny, info: LayoutCallbackInfo) -> Dom {
    // Reading the mode makes a light / dark switch rebuild the window.
    let _mode = info.get_mode();
    meet_layout(data)
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

/// The name to show for the peer with endpoint id `node_id`: its member's, from the signed
/// announcement read last.
fn remote_name(s: &MeetState, node_id: &str) -> String {
    s.room
        .as_ref()
        .and_then(|room| room.peers.iter().find(|p| p.node_id == node_id))
        .map(|p| p.name.clone())
        .unwrap_or_else(|| short_id(node_id).to_string())
}

/// A member's name as the window shows it: the one its record carries, else [`UNKNOWN_NAME`].
fn member_name(name: &str) -> String {
    if name.trim().is_empty() {
        String::from(UNKNOWN_NAME)
    } else {
        name.to_string()
    }
}

/// The window's screen: the settings while open, the room view of an open room, the start screen
/// before a meeting, the waiting room for a meeting not joined yet, else the call.
fn ui_screen(s: &MeetState) -> ui::UiScreen {
    if kit::settings_open(&s.kit) {
        return ui::UiScreen::Settings;
    }
    match s.room.as_ref().map(|room| room.stage) {
        Some(Stage::Start | Stage::Opening) if s.open_room.is_some() => ui::UiScreen::Room,
        Some(Stage::Start | Stage::Opening) | None => ui::UiScreen::Lobby,
        Some(Stage::Waiting) => ui::UiScreen::Waiting,
        Some(Stage::InRoom | Stage::Ended) => ui::UiScreen::Call,
    }
}

/// The room of the call (or the waiting room), when there is one.
fn call_room_id(s: &MeetState) -> Option<String> {
    let room = s.room.as_ref()?;
    match room.stage {
        Stage::InRoom | Stage::Ended if !room.room_id.is_empty() => Some(room.room_id.clone()),
        Stage::Waiting => room.waiting.as_ref().map(|found| found.room.clone()),
        _ => None,
    }
}

/// The room whose chat is on the screen: the room view's, else the call's.
fn active_room_id(s: &MeetState) -> Option<String> {
    s.open_room.clone().or_else(|| call_room_id(s))
}

/// The members of a room as the window lists them: this device first, then by name; with their
/// safety codes and whether the user verified them.
fn member_rows(s: &MeetState, chat: &chatroom::ChatRoom) -> Vec<ui::MemberRow> {
    let me = s.identity.as_ref().map(|i| i.device().to_string()).unwrap_or_default();
    let mut rows: Vec<ui::MemberRow> = chat
        .members()
        .map(|m| ui::MemberRow {
            room: chat.room.clone(),
            device: m.device.clone(),
            name: if m.device == me { s.name.clone() } else { member_name(&m.name) },
            code: m.safety_code.clone(),
            verified: s.index.is_verified(&m.device),
            me: m.device == me,
        })
        .collect();
    rows.sort_by(|a, b| b.me.cmp(&a.me).then_with(|| a.name.cmp(&b.name)));
    rows
}

/// The devices knocking at a room, oldest first.
fn knock_rows(s: &MeetState, chat: &chatroom::ChatRoom) -> Vec<ui::MemberRow> {
    let me = s.identity.as_ref().map(|i| i.device().to_string()).unwrap_or_default();
    chat.knocks()
        .filter(|k| k.device != me)
        .map(|k| ui::MemberRow {
            room: chat.room.clone(),
            device: k.device.clone(),
            name: member_name(&k.name),
            code: k.safety_code.clone(),
            verified: s.index.is_verified(&k.device),
            me: false,
        })
        .collect()
}

/// The people panel: this side, then everyone the meeting server lists with a member's signed
/// announcement (and connected peers it no longer lists), each with its safety code.
fn people(s: &MeetState) -> Vec<ui::PersonView> {
    let now = now_ms(s);
    let me = my_state(s);
    let chat = call_room_id(s).and_then(|id| s.chats.get(&id));
    let my_code = s.identity.as_ref().map(crypto::Identity::safety_code);
    let mut rows = vec![ui::PersonView {
        name: s.name.clone(),
        me: true,
        status: String::from(if s.cam_on { "camera on" } else { "camera off" }),
        muted: me.muted,
        deafened: me.deafened,
        speaking: false,
        code: my_code,
        verified: false,
    }];
    let person = |name: String, status: &str, r: Option<&Remote>, device: Option<&str>| {
        let member = device.and_then(|d| chat.and_then(|c| c.member(d)));
        ui::PersonView {
            name,
            me: false,
            status: status.to_string(),
            muted: r.and_then(|r| r.state).is_some_and(|state| state.muted),
            deafened: r.and_then(|r| r.state).is_some_and(|state| state.deafened),
            speaking: r.is_some_and(|r| s.speaker.is_speaking(r.key, now)),
            code: member.map(|m| m.safety_code.clone()),
            verified: device.is_some_and(|d| s.index.is_verified(d)),
        }
    };
    if let Some(room) = &s.room {
        for p in &room.peers {
            let remote = s.remotes.iter().find(|r| r.node_id == p.node_id);
            let status = if remote.is_some() {
                "connected"
            } else if rooms::dials(&room.node_id, &p.node_id) {
                "connecting"
            } else {
                "waiting for them to connect"
            };
            rows.push(person(p.name.clone(), status, remote, p.device.as_deref()));
        }
        // Connected peers whose record expired from the server stay listed.
        for r in &s.remotes {
            if !room.peers.iter().any(|p| p.node_id == r.node_id) {
                rows.push(person(short_id(&r.node_id).to_string(), "connected", Some(r), None));
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
            name: s.name.clone(),
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
    // How packets may travel ("Transport: relay-only http://..."), then the plan and the peers.
    let mut network: Vec<String> = transport_line(s).into_iter().collect();
    network.extend(network_lines(s));
    let section = |title: &str, lines: Vec<String>| ui::StatSection {
        title: title.to_string(),
        lines,
    };
    vec![
        section("People", roster_lines(s)),
        section("Microphones", s.mics.clone()),
        section("Speakers", s.speakers.clone()),
        section("Video", video),
        section("Audio", audio),
        section("Network", network),
    ]
}

/// The statistics' people lines: "Ada (you)", then "Ben · connected · muted" per participant
/// (what the scripts read).
fn roster_lines(s: &MeetState) -> Vec<String> {
    people(s)
        .into_iter()
        .map(|person| {
            let label = if person.status.is_empty() || person.me {
                ui::shown_name(&person.name, person.me)
            } else {
                format!("{} · {}", person.name, person.status)
            };
            audio::person_line(
                &label,
                Some(audio::PeerState {
                    muted: person.muted,
                    deafened: person.deafened,
                }),
            )
        })
        .collect()
}

/// The cameras the settings offer: by facing (there is no camera list API).
const CAMERAS: [&str; 3] = ["Front camera", "Back camera", "External camera"];

/// "System default", then `devices`.
fn device_choices(devices: &[String]) -> Vec<String> {
    let mut choices = vec![String::from("System default")];
    choices.extend(devices.iter().cloned());
    choices
}

/// "Thu 9 Oct 2026, 14:00-15:00 · starts in 25 min": a meeting's times in this computer's time
/// zone, and where it stands now.
fn times_text(starts: u64, ends: u64) -> String {
    let offset = chrono::Local::now().offset().local_minus_utc();
    let now = azul_storage::time::now_unix();
    format!(
        "{} · {}",
        rooms::when_text(starts, ends, offset),
        rooms::meeting_status(starts, ends, now)
    )
}

/// A room as the start screen lists it: its kind and code, its times, its unread count.
fn room_row(s: &MeetState, entry: &roomlist::RoomEntry) -> ui::RoomRow {
    let chat = s.chats.get(&entry.room);
    let kind = chatroom::RoomKind::parse(Some(entry.kind.as_str()));
    let code = chat
        .map(|c| c.code.clone())
        .filter(|c| !c.is_empty())
        .unwrap_or_else(|| entry.code.clone());
    let title = match kind {
        chatroom::RoomKind::Chat => format!("Chat room {code}"),
        chatroom::RoomKind::Meeting => format!("Meeting {code}"),
    };
    let times = match (entry.starts_at, entry.ends_at) {
        (Some(a), Some(b)) => Some(times_text(a, b)),
        _ => None,
    };
    let members = chat.map_or(0, |c| c.members().count());
    let mut detail = match chat.map(|c| c.state) {
        Some(chatroom::Membership::Knocking) => String::from("waiting to be let in"),
        Some(chatroom::Membership::Closed) => String::from("ended"),
        _ if members > 0 => format!("{members} member{}", if members == 1 { "" } else { "s" }),
        _ => String::new(),
    };
    if let Some(times) = times {
        if !detail.is_empty() {
            detail.push_str(" · ");
        }
        detail.push_str(&times);
    }
    ui::RoomRow {
        room: entry.room.clone(),
        title,
        detail,
        unread: chat.map_or(0, chatroom::ChatRoom::unread),
    }
}

/// The room view of the open room.
fn room_page(s: &MeetState) -> Option<ui::RoomPage> {
    let id = s.open_room.as_ref()?;
    let chat = s.chats.get(id)?;
    let me = s.identity.as_ref()?;
    let kind = match chat.kind {
        chatroom::RoomKind::Chat => "Chat room",
        chatroom::RoomKind::Meeting => "Meeting",
    };
    let status = match chat.state {
        chatroom::Membership::Member => {
            String::from("End-to-end encrypted: only the members read it.")
        }
        chatroom::Membership::Knocking => String::from(
            "Waiting for a member to let you in. Compare your safety code with theirs.",
        ),
        chatroom::Membership::Joining => String::from("Joining..."),
        chatroom::Membership::Leaving => String::from("Leaving..."),
        chatroom::Membership::Left => String::from("You left this room."),
        chatroom::Membership::Closed => String::from("This room is closed."),
        chatroom::Membership::Outside => String::from("You are not in this room."),
    };
    let key_line = match chat.current_key(me.device()) {
        Some((epoch, _, holders)) => format!(
            "Room key {epoch}, held by the {holders} member{} now.",
            if holders == 1 { "" } else { "s" }
        ),
        None => String::from("A room key is made with the first message."),
    };
    Some(ui::RoomPage {
        title: format!("{kind} {}", chat.code),
        times: match (chat.starts_at, chat.ends_at) {
            (Some(a), Some(b)) => Some(times_text(a, b)),
            _ => None,
        },
        link: chat.link(rooms::APP_LINK_PREFIX),
        copied: s.room_copied,
        my_code: me.safety_code(),
        status,
        members: member_rows(s, chat),
        knocks: knock_rows(s, chat),
        can_call: chat.state == chatroom::Membership::Member,
        unreadable: chat.unreadable(),
        before_join: chat.before_join(),
        key_line,
    })
}

/// The chat lines of `chat`, oldest first.
fn chat_lines(s: &MeetState, chat: Option<&chatroom::ChatRoom>) -> Vec<ui::ChatLine> {
    chat.map(|c| {
        c.messages()
            .iter()
            .map(|m| ui::ChatLine {
                name: if m.mine { s.name.clone() } else { member_name(&m.name) },
                text: m.text.clone(),
                mine: m.mine,
                sending: m.seq.is_none(),
            })
            .collect()
    })
    .unwrap_or_default()
}

/// Everything the window shows, from the state.
fn snapshot(s: &MeetState) -> ui::CallView {
    let now = now_ms(s);
    let arr = arrangement(s);
    // The meeting's code: the room's once in it, the waiting room's before.
    let code = s.room.as_ref().map(|room| match &room.waiting {
        Some(found) if room.code.is_empty() => found.code.as_str(),
        _ => room.code.as_str(),
    });
    let title = match code {
        Some(code) if !code.is_empty() => format!("AzMeet · meeting {code} · {}", s.name),
        _ => format!("AzMeet · {}", s.name),
    };
    let camera_renditions = my_renditions(s, CAMERA_TRACK);
    let rooms: Vec<ui::RoomRow> = match &s.identity {
        Some(me) => s
            .index
            .rooms_of(me.device())
            .into_iter()
            .map(|entry| room_row(s, entry))
            .collect(),
        None => Vec::new(),
    };
    let lobby = s.room.as_ref().map(|room| ui::LobbyView {
        opening: room.stage == Stage::Opening,
        server_text: room.server_text.clone(),
        server_status: room.server_status.clone(),
        server_ok: room.server_ok,
        server_unset: room.worker.is_empty(),
        join_text: room.join_text.clone(),
        schedule_start: room.schedule_start.clone(),
        schedule_minutes: room.schedule_minutes.clone(),
        rooms,
        identity_ready: s.identity.is_some(),
    });
    let waiting = s.room.as_ref().and_then(|room| {
        room.waiting.as_ref().map(|found| {
            // With only the code (not a meeting this side made, a preview's): "Ask to join".
            let knock = (found.invite.is_none() && !room.created).then_some(room.asked);
            ui::WaitingView {
                code: found.code.clone(),
                link: found.share_link(),
                copied: room.copied,
                created: room.created,
                people: room.waiting_people.clone(),
                times: match (found.starts_at, found.ends_at) {
                    (Some(a), Some(b)) => Some(times_text(a, b)),
                    _ => None,
                },
                knock,
            }
        })
    });
    let call_chat = call_room_id(s).and_then(|id| s.chats.get(&id));
    let active_chat = active_room_id(s).and_then(|id| s.chats.get(&id));
    let knocks = call_chat.map(|c| knock_rows(s, c)).unwrap_or_default();
    let chat_note = active_chat.map_or_else(String::new, |c| {
        let mut note = String::from("End-to-end encrypted");
        if c.before_join() > 0 {
            note.push_str(&format!(
                " · {} earlier message{} sealed before you joined",
                c.before_join(),
                if c.before_join() == 1 { "" } else { "s" }
            ));
        }
        if c.unreadable() > 0 {
            note.push_str(&format!(" · {} could not be read", c.unreadable()));
        }
        note
    });
    let identity = match (&s.identity, &s.identity_source) {
        (Some(me), Some(source)) => format!("{} ({})", me.safety_code(), source.describe()),
        _ => String::from("loading..."),
    };
    ui::CallView {
        room_page: room_page(s),
        knocks,
        chat_note,
        screen: ui_screen(s),
        title,
        notice: s.notice.clone(),
        name: s.name.clone(),
        lobby,
        waiting,
        stage: arr.stage.map(|tile| tile_view(s, tile, now)),
        tiles: arr.tiles.iter().map(|tile| tile_view(s, *tile, now)).collect(),
        panel: match s.panel {
            SidePanel::People => ui::PanelView::People,
            SidePanel::Chat => ui::PanelView::Chat,
            SidePanel::Statistics => ui::PanelView::Statistics,
            SidePanel::Closed => ui::PanelView::Closed,
        },
        people: people(s),
        chat: chat_lines(s, active_chat),
        chat_unread: call_chat.map_or(0, chatroom::ChatRoom::unread) as u32,
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
        pattern_still: s.pattern_still.clone(),
        mirror: s.mirror,
        screen_renditions: my_renditions(s, SCREEN_TRACK),
        camera_renditions,
        mic_level: s.mic_level,
        video_debug: s.video_debug,
        settings: ui::SettingsView {
            mics: device_choices(&s.mics),
            mic_choice: s.mic_choice,
            speakers: device_choices(&s.speakers),
            speaker_choice: s.speaker_choice,
            cameras: CAMERAS.iter().map(|c| c.to_string()).collect(),
            camera_choice: s.camera_choice,
            quality: s.quality,
            mirror: s.mirror,
            join_muted: s.join_muted,
            join_camera_off: s.join_camera_off,
            server: s
                .room
                .as_ref()
                .map(|room| room.worker.clone())
                .filter(|worker| !worker.is_empty())
                .unwrap_or_else(|| String::from("none set")),
            name: s.name.clone(),
            codec: codec_status(s),
            recordings: files_root().map_or_else(
                || String::from("nowhere: this run keeps no files"),
                |root| {
                    format!(
                        "{} (each meeting's own folder)",
                        azul_appkit::data::local_path(root, store::APP_FOLDER).display()
                    )
                },
            ),
            identity,
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
    copy_link: on_copy_link,
    drop_packet: on_drop_video_packet,
    chat_text: on_chat_text,
    chat_key: on_chat_key,
    chat_send: on_chat_send,
    chat_blur: on_chat_blur,
    name_text: on_name_text,
    name_blur: on_name_blur,
    server_text: on_server_text,
    server_key: on_server_key,
    server_blur: on_server_blur,
    join_text: on_join_text,
    new_meeting: on_new_meeting,
    join: on_join,
    join_now: on_join_now,
    waiting_back: on_waiting_back,
    mic_choice: on_mic_choice,
    speaker_choice: on_speaker_choice,
    camera_choice: on_camera_choice,
    quality: on_quality,
    mirror: on_mirror,
    join_muted: on_join_muted,
    join_camera_off: on_join_camera_off,
    key: on_key,
    keyring: on_keyring_result,
    retry_server: on_retry_server,
    new_chat_room: on_new_chat_room,
    schedule: on_schedule,
    schedule_start: on_schedule_start,
    schedule_minutes: on_schedule_minutes,
    open_room: on_open_room,
    room_back: on_room_back,
    room_leave: on_room_leave,
    room_call: on_room_call,
    room_copy: on_room_copy,
    admit: on_admit,
    verify: on_verify,
};

fn meet_layout(mut data: RefAny) -> Dom {
    let Some((view, kit_ref)) = data
        .downcast_ref::<MeetState>()
        .map(|s| (snapshot(&s), s.kit.clone()))
    else {
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
    ui::meet_view(&view, &data, &kit_ref, &ACTIONS)
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
            if !peer_announced(s, &node_id) {
                // Its signed announcement has not been read here yet: nothing goes to it, nothing
                // it sends is taken, until a read of the peers list confirms it (CRYPTO.md 10).
                eprintln!(
                    "[azmeet] {}: {} connected; waiting for its signed announcement",
                    s.name,
                    short_id(&node_id)
                );
                if !s.pending.iter().any(|p| p.handle == event.peer) {
                    s.pending.push(Pending {
                        handle: event.peer,
                        node_id,
                        polls: 0,
                        held: Vec::new(),
                    });
                }
                return false;
            }
            accept_peer(s, event.peer, node_id)
        }
        IrohEventKind::PeerDisconnected => {
            s.pending.retain(|p| p.handle != event.peer);
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
        IrohEventKind::Message => apply_peer_state(s, event.peer, event.data.as_ref()),
        _ => false,
    }
}

/// A peer's call state (`audio::Control::State`: microphone, camera, hand) from connection `conn`.
/// True when it changed.
fn apply_peer_state(s: &mut MeetState, conn: u64, bytes: &[u8]) -> bool {
    let Some(audio::Control::State(state)) = audio::decode_control(bytes) else {
        return false;
    };
    let Some(remote) = s.remotes.iter_mut().find(|r| r.handle == conn) else {
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

/// A message a connection sent while it was pending ([`Pending::held`]), taken now that it is a
/// peer: its report, its capabilities, its call state. Media it sent then is gone (a keyframe
/// request brings the picture back).
fn replay_held(s: &mut MeetState, conn: u64, bytes: &[u8]) -> bool {
    let Some(sender) = s.remotes.iter().find(|r| r.handle == conn).map(|r| r.key) else {
        return false;
    };
    if let Some(sync) = routes::decode_sync(bytes) {
        return apply_sync(s, conn, sync);
    }
    match video_wire::decode_message(bytes) {
        Some(Message::Control(control)) => apply_video_control(s, conn, sender, None, control),
        Some(Message::Packet(..)) => false,
        None => apply_peer_state(s, conn, bytes),
    }
}

/// Whether a peer may connect: only in a meeting.
fn accepts_peers(s: &MeetState) -> bool {
    s.room
        .as_ref()
        .is_some_and(|room| matches!(room.stage, Stage::InRoom | Stage::Ended))
}

/// Whether the endpoint `node_id` is in the peers list as read last: a member's device signed its
/// announcement (`on_peers` keeps no other).
fn peer_announced(s: &MeetState, node_id: &str) -> bool {
    s.room
        .as_ref()
        .is_some_and(|room| room.peers.iter().any(|p| p.node_id == node_id))
}

/// The peer `node_id` behind connection `handle` is in the call: its state, what this side codes,
/// and a new report for everyone.
fn accept_peer(s: &mut MeetState, handle: u64, node_id: String) -> bool {
    eprintln!("[azmeet] {}: connected to {}", s.name, short_id(&node_id));
    s.link_status = format!("connected to {}", short_id(&node_id));
    if !s.remotes.iter().any(|r| r.handle == handle) {
        s.remotes.push(Remote::new(handle, node_id));
    }
    send_state(s, &[handle]);
    // What this side decodes and encodes; until the peer's answer arrives it gets JPEG.
    send_caps(s, &[handle]);
    // The new peer's tiles change what this side shows: everyone hears the new report.
    network_changed(s, true);
    true
}

/// After a read of the peers list: a pending connection whose announcement is there now joins
/// the call; one still unconfirmed after [`PENDING_POLLS`] reads is dropped. True when the window
/// changes.
fn settle_pending(s: &mut MeetState) -> bool {
    let pending = std::mem::take(&mut s.pending);
    let mut changed = false;
    for mut p in pending {
        if peer_announced(s, &p.node_id) {
            changed |= accept_peer(s, p.handle, p.node_id);
            for bytes in std::mem::take(&mut p.held) {
                changed |= replay_held(s, p.handle, &bytes);
            }
            continue;
        }
        p.polls += 1;
        if p.polls >= PENDING_POLLS {
            eprintln!(
                "[azmeet] {}: dropped {}: no member signed its announcement",
                s.name,
                short_id(&p.node_id)
            );
            if let Some(endpoint) = s.endpoint.as_ref() {
                endpoint.disconnect(p.handle);
            }
        } else {
            s.pending.push(p);
        }
    }
    changed
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
            note_people(&mut s);
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
        // The chat that arrived, the people met: into the meeting's folder.
        flush_files(&mut s, &mut info.callback_info);
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
    /// Cancels the echo of what plays from the microphone (at `MIC_RATE`): fed every turn the
    /// playout plays, used by `send_audio`. `None` when nothing plays on a device, or turned off
    /// (`AZMEET_ECHO_CANCEL=0`).
    echo: Option<SharedEcho>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The echo canceller, so the playout state can move to its thread. The generated handle is a raw
/// pointer and therefore not `Send`; the library keeps the canceller's state behind a `Mutex`
/// (`audio/echo.rs`, `EchoCanceller::stream`).
struct SharedEcho(EchoCanceller);

// SAFETY: the canceller's state lives behind a Mutex inside the library, and AzMeet reaches it
// only while holding the `Playout` mutex - the handle is never used from two threads at once,
// and it is dropped (closed) exactly once, with the `Playout`.
unsafe impl Send for SharedEcho {}

/// Starts the playout thread. It ends once the returned handle (kept in `MeetState`) is gone.
fn start_playout(play: bool) -> Arc<Mutex<Playout>> {
    let cancel = setting("AZMEET_ECHO_CANCEL").map_or(true, |v| v != "0");
    let echo = (play && cancel)
        .then(|| EchoCanceller::create(MIC_RATE, ECHO_TAIL_MS))
        .filter(EchoCanceller::is_open)
        .map(SharedEcho);
    let shared = Arc::new(Mutex::new(Playout {
        peers: BTreeMap::new(),
        play,
        echo,
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
                // What this turn plays at the microphone's rate, mixed: the echo canceller's far
                // end (peers at another rate play uncancelled - there is no resampler).
                let mut mix: Vec<f32> = Vec::new();
                for (handle, jitter) in playout.peers.iter_mut() {
                    if let Some(samples) = jitter.pop() {
                        if jitter.sample_rate() == MIC_RATE {
                            if mix.len() < samples.len() {
                                mix.resize(samples.len(), 0.0);
                            }
                            for (m, v) in mix.iter_mut().zip(&samples) {
                                *m += audio::from_pcm16(*v);
                            }
                        }
                        out.push((*handle, jitter.sample_rate(), samples));
                    }
                }
                if let (Some(echo), false) = (playout.echo.as_ref().map(|e| &e.0), mix.is_empty()) {
                    let _ = echo.far_end(AudioFrame {
                        sample_rate: MIC_RATE,
                        channels: 1,
                        samples: F32Vec::from(mix),
                    });
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

/// The audio settings of this run: `--test-tone` (`AZMEET_TEST_TONE=1`) makes the microphone a
/// tone and starts it unmuted; a headless run uses the tone too (muted until switched on) and
/// plays nothing.
fn configure_audio(s: &mut MeetState) {
    let devices = devices_allowed();
    let tone = setting_on("AZMEET_TEST_TONE");
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
            "[azmeet] {}: the microphone is a {TONE_HZ} Hz test tone (--test-tone)",
            s.name
        );
    }
}

/// Starts or stops the test tone with the microphone; a mute forgets the unfinished packet.
fn sync_mic(s: &mut MeetState) {
    if !s.mic_on {
        s.packetizer.reset();
        s.opus_framer.reset();
        s.tone = None;
    } else if s.tone_mic && s.tone.is_none() {
        s.tone = Some((
            audio::ToneSource::new(TONE_HZ, MIC_RATE),
            std::time::Instant::now(),
        ));
    }
}

/// Sends captured audio as 20 ms packets on the audio track to whom the plan says: everyone in the
/// full mesh, the backbone parent for a leaf. As Opus (about 32 kbit/s) when this side encodes it
/// and every peer decodes it (`audio::send_opus`; forwarders pass one stream on), else as 16-bit
/// PCM.
fn send_audio(s: &mut MeetState, sample_rate: u32, channels: u16, samples: &[f32]) {
    if !s.mic_on {
        return;
    }
    if s.remotes.is_empty() {
        // Nobody listens: the first frame to the next peer starts with fresh audio.
        s.packetizer.reset();
        s.opus_framer.reset();
        return;
    }
    let Some(endpoint) = s.endpoint.clone() else {
        return;
    };
    let targets = handles_of(s, &s.plan.children(s.me, s.me));
    // The echo of what this side plays, removed (mono from here on).
    let cleaned = cancel_echo(s, sample_rate, channels, samples);
    let (channels, samples) = match cleaned.as_deref() {
        Some(mono) => (1, mono),
        None => (channels, samples),
    };
    let peers_opus: Vec<Option<bool>> = s.remotes.iter().map(|r| r.opus).collect();
    let opus = audio::send_opus(opus_encoder(s, sample_rate).is_some(), &peers_opus);
    let codec = if opus {
        audio::CODEC_OPUS
    } else {
        audio::CODEC_PCM16
    };
    if s.audio_codec_out != Some(codec) {
        s.audio_codec_out = Some(codec);
        // For scripts: the codec this side's voice goes out in from now on.
        println!("AZMEET_AUDIO {}", if opus { "opus" } else { "pcm" });
    }
    let frames = if opus {
        // Should PCM come back (a peer without Opus joins), it starts with fresh audio.
        s.packetizer.reset();
        opus_frames(s, sample_rate, channels, samples)
    } else {
        s.opus_framer.reset();
        s.packetizer.push(sample_rate, channels, samples)
    };
    for frame in frames {
        for handle in &targets {
            endpoint.send_frame(*handle, AUDIO_TRACK, frame.clone());
        }
    }
}

/// The microphone (`samples`, interleaved `channels`) mixed down to mono with the echo of what
/// this side plays removed; `None` when no echo canceller runs (nothing plays on a device, it is
/// turned off) - then the microphone goes out as it is.
fn cancel_echo(
    s: &MeetState,
    sample_rate: u32,
    channels: u16,
    samples: &[f32],
) -> Option<Vec<f32>> {
    let shared = s.playout.as_ref()?;
    let playout = lock(shared);
    let echo = &playout.echo.as_ref()?.0;
    let cleaned = echo.process(AudioFrame {
        sample_rate,
        channels: 1,
        samples: F32Vec::from(audio::mix_to_mono(channels, samples)),
    });
    let mono: &[f32] = cleaned.samples.as_ref();
    Some(mono.to_vec())
}

/// This side's Opus encoder for mono audio at `sample_rate`, opened on first use at that rate;
/// `None` where it does not open (then it is not asked again at that rate).
fn opus_encoder(s: &mut MeetState, sample_rate: u32) -> Option<&mut AudioEncoder> {
    let stale = s
        .opus_out
        .as_ref()
        .map_or(true, |(rate, _)| *rate != sample_rate);
    if stale {
        let encoder = AudioEncoder::create(
            AudioConfig {
                sample_rate,
                channels: 1,
            },
            audio::OPUS_KBPS,
        );
        let open = encoder.is_open();
        s.opus_out = Some((sample_rate, open.then_some(encoder)));
    }
    s.opus_out
        .as_mut()
        .and_then(|(_, encoder)| encoder.as_mut())
}

/// `samples` mixed down to mono, through this side's Opus encoder: a frame (the new packet and
/// the two before it) for every packet it completes.
fn opus_frames(
    s: &mut MeetState,
    sample_rate: u32,
    channels: u16,
    samples: &[f32],
) -> Vec<Vec<u8>> {
    let mono = audio::mix_to_mono(channels, samples);
    let mut packets = Vec::new();
    if let Some(encoder) = opus_encoder(s, sample_rate) {
        let taken = encoder.encode(AudioFrame {
            sample_rate,
            channels: 1,
            samples: F32Vec::from(mono),
        });
        if taken {
            while let Some(packet) = encoder.recv_packet().into_option() {
                packets.push(packet.as_slice().to_vec());
            }
        }
    }
    packets
        .into_iter()
        .map(|packet| s.opus_framer.push(packet))
        .collect()
}

/// The packets of `origin`'s Opus frame this side has not decoded yet, decoded (in order, each
/// once: `audio::OpusOrder`) by that peer's own decoder into 48 kHz PCM packets.
fn decode_opus(s: &mut MeetState, origin: u64, bytes: &[u8]) -> Vec<audio::Packet> {
    let Some(packets) = audio::decode_opus_frame(bytes) else {
        return Vec::new();
    };
    let (decoder, order) = s.opus_in.entry(origin).or_insert_with(|| {
        let decoder = AudioDecoder::create(AudioConfig {
            sample_rate: audio::OPUS_RATE,
            channels: 1,
        });
        (decoder, audio::OpusOrder::new())
    });
    let mut decoded = Vec::new();
    for packet in packets {
        if !order.take(packet.sequence) {
            continue;
        }
        let Some(frame) = decoder.decode(U8Vec::from(packet.data)).into_option() else {
            continue;
        };
        let samples: &[f32] = frame.samples.as_ref();
        decoded.push(audio::Packet {
            sequence: packet.sequence,
            samples: samples
                .iter()
                .map(|sample| audio::to_pcm16(*sample))
                .collect(),
        });
    }
    decoded
}

/// Takes a frame of `origin`'s audio (from it directly, or passed on; Opus or 16-bit PCM) into its
/// jitter buffer. Returns whether the active speaker changed (the speaker view's stage).
fn receive_audio(s: &mut MeetState, origin: u64, bytes: &[u8]) -> bool {
    if !s.remotes.iter().any(|r| r.key == origin) {
        return false;
    }
    let codec = audio::frame_codec(bytes).unwrap_or(audio::CODEC_PCM16);
    let (sample_rate, packets) = if codec == audio::CODEC_OPUS {
        (audio::OPUS_RATE, decode_opus(s, origin, bytes))
    } else {
        match audio::decode_frame(bytes) {
            Some(wire) => (wire.sample_rate, wire.packets),
            None => return false,
        }
    };
    // The two codecs number their packets apart: a switch starts the jitter buffer over.
    let switched = s
        .audio_codec_in
        .insert(origin, codec)
        .is_some_and(|before| before != codec);
    if switched {
        if let Some(shared) = s.playout.as_ref() {
            lock(shared).peers.remove(&origin);
        }
    }
    // Who speaks: the newest packet's level (a frame repeats the two before it). Measured even
    // while deafened, so the stage still follows the conversation.
    let Some(newest) = packets.last() else {
        return false;
    };
    let now = now_ms(s);
    let level = speaker::level_db(&newest.samples);
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
    for packet in packets {
        jitter.push(sample_rate, packet);
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

/// Forgets the received audio of the peer with key `origin` (it left), or everyone's: its jitter
/// buffer, and its Opus decoder (a peer that comes back numbers its packets from scratch).
fn drop_audio(s: &mut MeetState, origin: Option<u64>) {
    match origin {
        Some(origin) => {
            s.opus_in.remove(&origin);
            s.audio_codec_in.remove(&origin);
        }
        None => {
            s.opus_in.clear();
            s.audio_codec_in.clear();
        }
    }
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

// ==== Chat: the room's end-to-end encrypted chat on the meeting server (`chatroom.rs`) ====

/// This side writes `text` in the chat of the room on screen (the room view's, else the call's):
/// listed at once, sealed and sent by the next `chat_tick`. False for an empty message, or no
/// room to write in.
fn send_chat(s: &mut MeetState, text: &str) -> bool {
    let Some(id) = active_room_id(s) else {
        return false;
    };
    let Some(me) = s.identity.as_ref() else {
        return false;
    };
    let Some(chat) = s.chats.get_mut(&id) else {
        return false;
    };
    let sent = chat.send(me, text, wall_ms()).is_some();
    if sent {
        s.unsaved.chats.insert(id);
    }
    sent
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

/// The level meter's new value (azul's `LevelMeter` scale: the RMS on -60..0 dB), when the
/// meter's throttle lets it move (by half a percent or more, [`METER_INTERVAL_MS`] or more after
/// the last move) and the meter is shown.
fn meter_change(s: &mut MeetState, samples: &[f32]) -> Option<(DomNodeId, f32)> {
    let level = LevelMeter::level_of(AudioFrame {
        sample_rate: MIC_RATE,
        channels: 1,
        samples: F32Vec::from(samples.to_vec()),
    })
    .round();
    let now = now_ms(s);
    let level = s.meter.next(level, now).into_option()?;
    s.mic_level = level;
    s.meter_bar.map(|bar| (bar, level))
}

/// Moves the meter in place (its segments and its accessibility value; no rebuild).
fn show_level(info: CallbackInfo, bar: DomNodeId, level: f32) {
    LevelMeter::update_level(info, bar, level);
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
    /// What the encoder may spend: the rendition's ladder rate, lowered while a receiver's link
    /// does not keep up ([`adapt_rates`]).
    rate: rate::RateControl,
}

impl VideoOut {
    /// The sending side of a rendition `rendition` lines tall.
    fn new(rendition: u16) -> Self {
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
            rate: rate::RateControl::new(IrohLoadBalancer::rendition_kbps(u32::from(rendition))),
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
/// `--video-codec jpeg` (`AZMEET_VIDEO_CODEC=jpeg`) switches H.264 off.
fn probe_video() -> VideoSupport {
    if setting("AZMEET_VIDEO_CODEC").is_some_and(|v| v.eq_ignore_ascii_case("jpeg")) {
        return VideoSupport {
            encoder: Err(String::from("H.264 switched off")),
            decodes_h264: false,
        };
    }
    let backend = VideoEncoder::backend_name().as_str().to_string();
    let probed = if backend == "none" {
        None
    } else {
        probe_encode()
    };
    let decodes_h264 = match &probed {
        Some((keyframe, _)) => probe_decode(keyframe),
        None => PlatformCapability::video_codec().available,
    };
    VideoSupport {
        // "VideoToolbox, hardware": the codec line says where the encoding runs.
        encoder: probed
            .map(|(_, hardware)| {
                format!("{backend}, {}", if hardware { "hardware" } else { "software" })
            })
            .ok_or_else(|| String::from("no encoder")),
        decodes_h264,
    }
}

/// A keyframe of the test pattern from a fresh H.264 encoder, and whether that encoder runs in
/// hardware; `None` where nothing, or no keyframe, comes out. `VideoEncoder::open` hands out an
/// open handle that never yields a packet where no backend is built in, so opening proves
/// nothing.
fn probe_encode() -> Option<(Vec<u8>, bool)> {
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
    let hardware = encoder.is_hardware();
    encoder.close();
    video_wire::h264_is_keyframe(&chunk).then_some((chunk, hardware))
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

/// The video settings of this run: `--test-pattern` (`AZMEET_TEST_PATTERN=1`) makes the camera a
/// test pattern, switches it on, and shows the "Drop a video packet" button; a headless run uses
/// test patterns too (off until switched on), so it never opens a camera or a screen.
fn configure_video(s: &mut MeetState, support: &VideoSupport) {
    let pattern = setting_on("AZMEET_TEST_PATTERN");
    let devices = devices_allowed();
    s.video = support.clone();
    s.pattern_video = pattern || !devices;
    if s.pattern_video && s.pattern_still.is_none() {
        let (width, height) = PATTERN_STILL;
        s.pattern_still =
            ImageRef::create_rawimage(frame_image(pattern_frame(0, width, height))).into_option();
    }
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
            "[azmeet] {}: the camera is a test pattern (--test-pattern)",
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
        .or_insert_with(|| VideoOut::new(rendition));
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
            // A re-opened encoder keeps the rate the link was found to carry.
            let kbps = out.rate.rate_kbps();
            let encoder = VideoEncoder::open(width, height, false, kbps);
            if encoder.is_open() {
                // For scripts (`scripts/azmeet_cpu.py`): where this rendition encodes.
                println!(
                    "AZMEET_ENCODER {} {width}x{height} {}",
                    rendition_label(track, rendition).replace(' ', "-"),
                    if encoder.is_hardware() {
                        "hardware"
                    } else {
                        "software"
                    }
                );
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
    adapt_rates(s);
}

/// Every pump: each H.264 encoder's rate follows the worst queue (packets sent, not yet
/// acknowledged) among the peers that get its stream now ([`rate::RateControl`]); a new rate goes
/// to the encoder, which spends it from its next frame on - no restart, no keyframe.
fn adapt_rates(s: &mut MeetState) {
    let now = now_ms(s);
    let keys: Vec<(u32, u16)> = s
        .video_out
        .iter()
        .filter(|(_, out)| out.encoder.is_some())
        .map(|(key, _)| *key)
        .collect();
    for key in keys {
        let (track, rendition) = key;
        let peers = stream_targets(s, track, rendition, true);
        let in_flight: Vec<usize> = s
            .remotes
            .iter()
            .filter(|r| peers.contains(&r.handle))
            .filter_map(|r| r.sent.get(&key))
            .map(|window| window.in_flight())
            .collect();
        let Some(out) = s.video_out.get_mut(&key) else {
            continue;
        };
        for queued in in_flight {
            out.rate.observe(queued);
        }
        let Some(kbps) = out.rate.tick(now) else {
            continue;
        };
        if let Some(encoder) = out.encoder.as_ref() {
            let _ = encoder.set_bitrate(kbps);
        }
        // For scripts: the rate each rendition spends now.
        println!(
            "AZMEET_RATE {} {kbps}",
            rendition_label(track, rendition).replace(' ', "-")
        );
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
        Control::Caps {
            h264,
            encodes,
            opus,
        } => {
            let Some(remote) = s.remotes.iter_mut().find(|r| r.handle == conn) else {
                return false;
            };
            remote.opus = Some(opus);
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
                "[azmeet] {}: {} decodes H.264: {}, encodes it: {}, decodes Opus: {}",
                s.name,
                remote_name(s, &node_id),
                yes(h264),
                yes(encodes),
                yes(opus)
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
            let mut line = video_wire::send_line(
                &rendition_label(*track, *height),
                out.h264_packets,
                out.jpeg_frames,
                &out.keyframes.stats(),
                out.dropped,
            );
            if out.encoder.is_some() {
                line.push_str(&format!(", H.264 at {}", out.rate.label()));
            }
            lines.push(line);
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
                // No higher than the quality picked in the settings.
                role.rendition_height(height, s.scale, room_size)
                    .min(QUALITY_CAPS[s.quality.min(QUALITY_CAPS.len() - 1)])
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

/// Tells the peers behind `handles` whether this side decodes and encodes H.264, and whether it
/// decodes Opus.
fn send_caps(s: &MeetState, handles: &[u64]) {
    let caps = video_wire::encode_caps(
        s.video.decodes_h264,
        s.video.encoder.is_ok(),
        s.decodes_opus,
    );
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
    // The peers whose path turned direct or relayed since the last statistics.
    let mut turned: Vec<(String, bool)> = Vec::new();
    let paths: Vec<(u64, routes::PathSample)> = s
        .remotes
        .iter_mut()
        .map(|r| {
            let stats = endpoint.peer_stats(r.handle);
            if r.path.map(|(direct, _)| direct) != Some(stats.direct) {
                turned.push((r.node_id.clone(), stats.direct));
            }
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
    // For scripts: each peer's path whenever it turns (`AZMEET_PATH Ben relayed`); with
    // `--relay-only` it never says `direct`.
    for (node_id, direct) in turned {
        println!(
            "AZMEET_PATH {} {}",
            remote_name(s, &node_id),
            if direct { "direct" } else { "relayed" }
        );
    }
    s.capacity.sample(now, &paths);
    network_changed(s, true);
}

/// The routing settings of this run (see the module docs): the mesh cap, a pinned uplink, the
/// forwarding opt-out, battery, and the view.
fn configure_network(s: &mut MeetState) {
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
    s.panel = match setting("AZMEET_PANEL").as_deref() {
        Some("chat") => SidePanel::Chat,
        Some("statistics") => SidePanel::Statistics,
        Some("closed") => SidePanel::Closed,
        _ => SidePanel::People,
    };
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
            " (--uplink-kbps)"
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

/// A peer's report arrived: plan again. True when the window changes: a screen shared or no
/// longer shared moves the tiles (`arrangement`), and a change in what this side must capture
/// changes the capture widgets' consumers (`my_streams`, built into the DOM). Nothing else in a
/// report (the uplink, the stability, the tiles the peer shows) changes a pixel here, so it no
/// longer rebuilds the whole window - every peer re-sends its report on each change and every 2
/// seconds, and each one that differed used to.
fn apply_sync(s: &mut MeetState, conn: u64, sync: routes::Sync) -> bool {
    let streams = my_streams(s);
    let Some(remote) = s.remotes.iter_mut().find(|r| r.handle == conn) else {
        return false;
    };
    if remote.sync.as_ref() == Some(&sync) {
        return false;
    }
    let shared_before = remote.sync.as_ref().is_some_and(|before| before.sends_screen);
    let sharing_moved = shared_before != sync.sends_screen;
    remote.sync = Some(sync);
    replan(s);
    stop_culled(s);
    sharing_moved || my_streams(s) != streams
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
        // A connection waiting for its signed announcement: what it says about itself is kept
        // for when it is confirmed (`settle_pending`); its media is not taken.
        let about_itself = routes::decode_sync(bytes).is_some()
            || matches!(video_wire::decode_message(bytes), Some(Message::Control(_)))
            || matches!(
                audio::decode_control(bytes),
                Some(audio::Control::State(_))
            );
        if about_itself {
            if let Some(p) = s.pending.iter_mut().find(|p| p.handle == conn) {
                if p.held.len() == PENDING_HELD {
                    p.held.remove(0);
                }
                p.held.push(bytes.to_vec());
            }
        }
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

/// What AzMeet calls itself on the meeting server.
const USER_AGENT: &str = concat!("AzMeet/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Verb {
    Get,
    Post,
    Put,
    Delete,
}

impl Verb {
    /// The verb of a request's method name.
    fn of(method: &str) -> Verb {
        match method {
            "POST" => Verb::Post,
            "PUT" => Verb::Put,
            "DELETE" => Verb::Delete,
            _ => Verb::Get,
        }
    }
}

/// One request to the meeting server.
struct HttpJob {
    verb: Verb,
    url: String,
    body: Vec<u8>,
    /// More headers: a signed request's (CRYPTO.md section 11).
    headers: Vec<(&'static str, String)>,
    on_result: ResumeFn,
    /// The room session the request was sent in (see `RoomSession::session`).
    session: u32,
    /// A room's chat request: the room, and what the request is (`on_chat_answer`).
    chat: Option<(String, chatroom::CallKind)>,
}

impl HttpJob {
    fn get(url: String, on_result: ResumeFn, session: u32) -> Self {
        HttpJob {
            verb: Verb::Get,
            url,
            body: Vec::new(),
            headers: Vec::new(),
            on_result,
            session,
            chat: None,
        }
    }

    /// `POST /rooms` with the room this side minted: its id, its invite key, its kind and times
    /// (the Worker's "links made offline" registration; CRYPTO.md section 4).
    fn create_room(room: &RoomSession, minting: &Minting) -> Self {
        let mut body = serde_json::json!({
            "room": minting.invite.room(),
            "invite_key": minting.invite.invite_key(),
            "kind": minting.kind.as_str(),
        });
        if let Some((starts, ends)) = &minting.times {
            body["starts_at"] = serde_json::json!(starts);
            body["ends_at"] = serde_json::json!(ends);
        }
        HttpJob {
            verb: Verb::Post,
            url: format!("{}/rooms", room.worker),
            body: body.to_string().into_bytes(),
            headers: Vec::new(),
            on_result: on_room_opened,
            session: room.session,
            chat: None,
        }
    }

    fn look_up(room: &RoomSession, key: &RoomKey) -> Self {
        HttpJob::get(
            format!("{}/rooms/{}?format=json", room.worker, key.as_str()),
            on_room_opened,
            room.session,
        )
    }

    /// This endpoint's ticket in the room, signed by this device (CRYPTO.md section 10): its
    /// name is the member record's, sealed, so none goes here.
    fn announce(room: &RoomSession, me: &crypto::Identity, ts: u64) -> Self {
        let sig = me.sign(&crypto::peer_input(&room.room_id, &room.node_id, &room.ticket));
        let body = serde_json::json!({
            "node_id": room.node_id,
            "ticket": room.ticket,
            "name": "",
            "device": me.device(),
            "sig": sig,
        })
        .to_string()
        .into_bytes();
        let path = format!("/rooms/{}/peers", room.room_id);
        let headers = Vec::from(me.request_headers("POST", &path, ts, &body));
        HttpJob {
            verb: Verb::Post,
            url: format!("{}{path}", room.worker),
            body,
            headers,
            on_result: on_announced,
            session: room.session,
            chat: None,
        }
    }

    fn poll(room: &RoomSession) -> Self {
        HttpJob::get(
            format!(
                "{}/rooms/{}/peers?except={}",
                room.worker, room.room_id, room.node_id
            ),
            on_peers,
            room.session,
        )
    }

    /// The waiting room's read of its meeting's peers list (this side is not announced there).
    fn waiting_peers(room: &RoomSession) -> Self {
        let meeting = room
            .waiting
            .as_ref()
            .map_or("", |found| found.room.as_str());
        HttpJob::get(
            format!(
                "{}/rooms/{}/peers?except={}",
                room.worker, meeting, room.node_id
            ),
            on_waiting_peers,
            room.session,
        )
    }

    /// Whether the meeting server answers (`GET /health`); the answer is matched to the check by
    /// `session`, which holds the check's number here.
    fn health(room: &RoomSession) -> Self {
        HttpJob::get(format!("{}/health", room.worker), on_health, room.checks)
    }

    /// Takes this endpoint off the room's list (Leave), signed by the device that announced it.
    fn leave(room: &RoomSession, me: &crypto::Identity, ts: u64) -> Self {
        let path = format!("/rooms/{}/peers/{}", room.room_id, room.node_id);
        HttpJob {
            verb: Verb::Delete,
            url: format!("{}{path}", room.worker),
            body: Vec::new(),
            headers: Vec::from(me.request_headers("DELETE", &path, ts, b"")),
            on_result: on_left,
            session: room.session,
            chat: None,
        }
    }

    /// A room's chat request (`chatroom::Call`), signed when it changes something.
    fn chat(
        chat: &chatroom::ChatRoom,
        me: &crypto::Identity,
        call: chatroom::Call,
        now_ms: u64,
    ) -> Self {
        let headers = if call.signed {
            let ts = chat.server_now(now_ms);
            Vec::from(me.request_headers(call.method, &call.path, ts, &call.body))
        } else {
            Vec::new()
        };
        HttpJob {
            verb: Verb::of(call.method),
            url: call.url(&chat.server),
            body: call.body,
            headers,
            on_result: on_chat_answer,
            session: 0,
            chat: Some((chat.room.clone(), call.what)),
        }
    }
}

struct HttpThreadInit {
    job: HttpJob,
    /// The participant's `MeetState`, handed back to `job.on_result` in a `Reply`.
    app: RefAny,
}

/// What a finished request resumes with: the participant's `MeetState`, the room session the
/// request was sent in, and for a chat request its room and what it was.
struct Reply {
    app: RefAny,
    session: u32,
    chat: Option<(String, chatroom::CallKind)>,
}

/// The participant and the session of a resumed request.
fn reply_parts(mut data: RefAny) -> Option<(RefAny, u32)> {
    let reply = data.downcast_ref::<Reply>()?;
    Some((reply.app.clone(), reply.session))
}

/// The participant, the room and what the request was, of a resumed chat request.
fn reply_chat(mut data: RefAny) -> Option<(RefAny, String, chatroom::CallKind)> {
    let reply = data.downcast_ref::<Reply>()?;
    let (room, what) = reply.chat.clone()?;
    Some((reply.app.clone(), room, what))
}

/// Runs one request on a worker thread. `http_request` blocks here, then queues its answer,
/// which the UI thread delivers to `on_result` on its next pump (the 15 ms link timer).
extern "C" fn http_thread(mut init: RefAny, _sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((verb, url, body, headers, on_result, reply)) =
        init.downcast_ref::<HttpThreadInit>().map(|i| {
            (
                i.job.verb,
                i.job.url.clone(),
                i.job.body.clone(),
                i.job.headers.clone(),
                i.job.on_result,
                Reply {
                    app: i.app.clone(),
                    session: i.job.session,
                    chat: i.job.chat.clone(),
                },
            )
        })
    else {
        return;
    };
    let method = match verb {
        Verb::Get => HttpMethod::Get,
        Verb::Post => HttpMethod::Post,
        Verb::Put => HttpMethod::Put,
        Verb::Delete => HttpMethod::Delete,
    };
    let mut config = HttpRequestConfig::create()
        .with_timeout(HTTP_TIMEOUT_SECS)
        .with_user_agent(USER_AGENT)
        .with_header("accept", "application/json");
    for (name, value) in headers {
        config = config.with_header(name, value);
    }
    let _request = config.http_request(
        method,
        url.as_str(),
        U8Vec::from(body),
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

/// The status and the body text of a finished request; no status when nothing answered.
fn http_reply(result: RefAny) -> (Option<u16>, String) {
    let Some(answer) = HttpGetResult::downcast(result).into_option() else {
        return (None, String::new());
    };
    match answer.result.into_result() {
        Ok(response) => (
            Some(response.status_code),
            response
                .body_as_string()
                .into_option()
                .map(|body| body.as_str().to_string())
                .unwrap_or_default(),
        ),
        Err(_) => (None, String::new()),
    }
}

fn json_text(json: &Json, key: &str) -> Option<String> {
    let value = json.get_key(key).into_option()?;
    let text = value.as_string().into_option()?;
    Some(text.as_str().to_string())
}

/// What `POST /rooms` and `GET /rooms/<key>` answer, and the invite this side holds for it.
#[derive(Clone)]
struct RoomInfo {
    room: String,
    code: String,
    /// The Worker's app link (`azlin://meet/<room>`), without a fragment.
    link: String,
    kind: chatroom::RoomKind,
    /// The meeting's times, seconds since 1970.
    starts_at: Option<u64>,
    ends_at: Option<u64>,
    /// The invite key the room was registered with; `None`: not an encrypted room.
    invite_key: Option<String>,
    /// The invite of the link this side holds; `None`: joined with the code (a knock).
    invite: Option<crypto::Invite>,
}

impl RoomInfo {
    /// The link others join with: with the invite secret when this side holds it.
    fn share_link(&self) -> String {
        rooms::link_with_secret(&self.link, self.invite.as_ref().map(crypto::Invite::secret))
    }
}

fn room_info(json: &Json) -> Option<RoomInfo> {
    let room = json_text(json, "room")?;
    let time = |key: &str| {
        json_text(json, key)
            .as_deref()
            .and_then(azul_storage::time::parse_iso8601)
    };
    Some(RoomInfo {
        code: json_text(json, "code").unwrap_or_default(),
        link: json_text(json, "link")
            .unwrap_or_else(|| format!("{}{room}", rooms::APP_LINK_PREFIX)),
        kind: chatroom::RoomKind::parse(json_text(json, "kind").as_deref()),
        starts_at: time("starts_at"),
        ends_at: time("ends_at"),
        invite_key: json_text(json, "invite_key").filter(|k| crypto::is_device(k)),
        invite: None,
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
                device: json_text(&p, "device"),
                sig: json_text(&p, "sig"),
            })
        })
        .collect()
}

/// The peers of `listed` whose announcement a member of `chat` signed, each with that member's
/// name (CRYPTO.md section 10); every other is left out, and logged once per read.
fn verified_peers(
    chat: Option<&chatroom::ChatRoom>,
    listed: Vec<PeerRecord>,
    me: &str,
) -> Vec<PeerRecord> {
    let Some(chat) = chat else {
        return Vec::new();
    };
    listed
        .into_iter()
        .filter_map(|p| {
            let member =
                chat.verify_peer(&p.node_id, &p.ticket, p.device.as_deref(), p.sig.as_deref());
            match member {
                Some(m) => Some(PeerRecord {
                    name: member_name(&m.name),
                    ..p
                }),
                None => {
                    if p.node_id != me {
                        eprintln!(
                            "[azmeet] left out {}: no member signed its announcement",
                            short_id(&p.node_id)
                        );
                    }
                    None
                }
            }
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

/// The answer to `POST /rooms` (a new meeting) or `GET /rooms/<key>` (joining a link): the
/// waiting room shows the meeting (a script's `AZMEET_JOIN` / `AZMEET_AUTOCREATE` enters it at
/// once).
/// `found` checked against what this side knows of it (CRYPTO.md section 4): a room this side
/// minted must come back under the id it made, a link's secret must derive the key the room was
/// registered with, and a room without an invite key (an older AzMeet's, not encrypted) is
/// refused. The invite this side holds goes with it; a code holds none (a knock).
fn check_room(
    mut found: RoomInfo,
    minting: Option<Minting>,
    secret: Option<String>,
) -> Result<RoomInfo, String> {
    let Some(key) = found.invite_key.clone() else {
        return Err(String::from(
            "This meeting was made by an older AzMeet and is not end-to-end encrypted, so this \
             AzMeet does not join it.",
        ));
    };
    let invite = match (minting, secret) {
        (Some(minting), _) => Some(minting.invite),
        (None, Some(secret)) => Some(crypto::Invite::new(&found.room, &secret).ok_or_else(|| {
            String::from("That link's secret is damaged: ask for the link again.")
        })?),
        (None, None) => None,
    };
    if let Some(invite) = &invite {
        if invite.room() != found.room || invite.invite_key() != key {
            return Err(String::from(
                "The meeting server sent a room that does not match this link.",
            ));
        }
    }
    found.invite = invite;
    Ok(found)
}

/// The chat room of `found` on this device: made when it is not here yet (with the invite of the
/// link, this device's name), its code, kind and times as the meeting server said.
fn ensure_chat(s: &mut MeetState, found: &RoomInfo) {
    let worker = s.room.as_ref().map(|r| r.worker.clone()).unwrap_or_default();
    let name = s.name.clone();
    let chat = s.chats.entry(found.room.clone()).or_insert_with(|| {
        chatroom::ChatRoom::new(
            &worker,
            &found.room,
            &found.code,
            found.kind,
            found.invite.clone(),
            &name,
        )
    });
    chat.kind = found.kind;
    chat.starts_at = found.starts_at;
    chat.ends_at = found.ends_at;
    if !found.code.is_empty() {
        chat.code = found.code.clone();
    }
}

/// `meet/rooms.json` learns what this device knows of room `id` now: kept on the next
/// `flush_files` when it changed.
fn remember_room(s: &mut MeetState, id: &str) {
    let Some(me) = s.identity.as_ref() else {
        return;
    };
    let Some(chat) = s.chats.get(id) else {
        return;
    };
    if !matches!(
        chat.state,
        chatroom::Membership::Member
            | chatroom::Membership::Knocking
            | chatroom::Membership::Joining
    ) {
        return;
    }
    let existing = s.index.get(me.device(), id).cloned();
    // The invite secret is sealed once: a new seal (a new nonce) would rewrite the file each time.
    let invite = existing
        .as_ref()
        .and_then(|e| e.invite.clone())
        .or_else(|| chat.invite().and_then(|i| me.seal_local(id, i.secret()).ok()));
    let entry = roomlist::RoomEntry {
        room: id.to_string(),
        code: chat.code.clone(),
        kind: chat.kind.as_str().to_string(),
        server: chat.server.clone(),
        starts_at: chat.starts_at,
        ends_at: chat.ends_at,
        device: me.device().to_string(),
        invite,
        member: chat.state != chatroom::Membership::Knocking,
        read_seq: chat.read_seq,
        departed: chat.departed().clone(),
        joined: existing.map_or_else(azul_storage::time::now_unix, |e| e.joined),
    };
    if s.index.upsert(entry) {
        s.unsaved.index = true;
    }
}

/// The meeting `found` was opened: a chat room opens in the room view (joined), a meeting in its
/// waiting room - or at once for a script (`AZMEET_JOIN`, `AZMEET_AUTOCREATE`). The request to
/// send next, if any.
fn open_found(
    s: &mut MeetState,
    info: &mut CallbackInfo,
    app: &RefAny,
    found: RoomInfo,
) -> Option<HttpJob> {
    ensure_chat(s, &found);
    if found.kind == chatroom::RoomKind::Chat {
        if let Some(room) = s.room.as_mut() {
            room.stage = Stage::Start;
            room.created = false;
        }
        println!("AZMEET_LINK {}", found.share_link());
        if !found.code.is_empty() {
            println!("AZMEET_CODE {}", found.code);
        }
        open_room_view(s, &found.room, true);
        return None;
    }
    let straight_in = s.room.as_ref().is_some_and(|room| room.straight_in);
    if straight_in {
        apply_join_defaults(s);
        return enter_room(s, info, app, found);
    }
    // For scripts: the waiting room is up, for this link.
    println!("AZMEET_WAITING {}", found.share_link());
    if let (Some(a), Some(b)) = (found.starts_at, found.ends_at) {
        println!(
            "AZMEET_TIMES {} {}",
            azul_storage::time::iso8601(a),
            azul_storage::time::iso8601(b)
        );
    }
    eprintln!(
        "[azmeet] {}: in the waiting room of meeting {} ({}){}",
        s.name,
        found.code,
        found.room,
        if found.invite.is_none() {
            ", with its code: a member lets this device in"
        } else {
            ""
        }
    );
    let room = s.room.as_mut()?;
    room.stage = Stage::Waiting;
    room.copied = false;
    room.waiting = Some(found);
    room.close_waiting();
    // Who is in the meeting already: asked now, then every poll while waiting.
    let job = room.waiting_job();
    s.notice.clear();
    apply_join_defaults(s);
    job
}

/// The answer to `POST /rooms` (a room this side minted) or `GET /rooms/<key>` (a link or a code
/// looked up): checked against the link (`check_room`), then opened (`open_found`).
extern "C" fn on_room_opened(data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some((mut data, session)) = reply_parts(data) else {
        return Update::DoNothing;
    };
    let answer = http_answer(result);
    let app = data.clone();
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
        let minting = room.minting.take();
        let secret = room.join_secret.take();
        let worker = room.worker.clone();
        let found = match &answer {
            Ok((200 | 201, Some(json))) => room_info(json),
            _ => None,
        };
        match found.map(|found| check_room(found, minting, secret)) {
            Some(Ok(found)) => open_found(s, &mut info, &app, found),
            Some(Err(why)) => {
                if let Some(room) = s.room.as_mut() {
                    room.stage = Stage::Start;
                }
                eprintln!("[azmeet] {}: {why}", s.name);
                s.notice = why;
                None
            }
            None => {
                if let Some(room) = s.room.as_mut() {
                    room.stage = Stage::Start;
                }
                s.notice = match &answer {
                    Ok((404, _)) => String::from("This meeting has ended, or the link is wrong."),
                    Ok((200 | 201, _)) => {
                        String::from("The meeting server sent an answer AzMeet cannot read.")
                    }
                    other => server_trouble(&worker, other),
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

/// The answer to an announcement; a successful one is followed by a read of the peers list, and
/// says how long the Worker keeps a ticket (the next announcement comes after a sixth of that).
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
            Ok((200, json)) => {
                let ttl = json
                    .as_ref()
                    .and_then(|j| j.get_key("peer_ttl_seconds").into_option())
                    .and_then(|v| {
                        v.as_int()
                            .into_option()
                            .map(|n| n as f64)
                            .or_else(|| v.as_float().into_option())
                    })
                    .filter(|ttl| *ttl >= 1.0);
                if let Some(ttl) = ttl {
                    room.reannounce_polls = rooms::reannounce_polls(ttl, ROOM_POLL_MS);
                }
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

/// The peers list: only the announcements a member's device signed count (CRYPTO.md section 10);
/// dial who this side should dial, and let in the connections that waited for theirs.
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
            let me = room.node_id.clone();
            let listed = verified_peers(s.chats.get(&room.room_id), peers_from(&json), &me);
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
            changed |= settle_pending(s);
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

/// The waiting room's read of its meeting's peers list: who is in the call already ("Ada is in
/// this meeting"), by the names of the members whose devices signed the announcements (a device
/// with only the code reads no names: "Someone"). An answer for a waiting room since left is
/// ignored; a meeting that ended says so.
extern "C" fn on_waiting_peers(data: RefAny, _info: CallbackInfo, result: RefAny) -> Update {
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
    if room.session != session || room.stage != Stage::Waiting {
        return Update::DoNothing;
    }
    room.waiting_busy = false;
    match answer {
        Ok((200, Some(json))) => {
            let meeting = room.waiting.as_ref().map(|found| found.room.clone()).unwrap_or_default();
            let me = room.node_id.clone();
            let names: Vec<String> = verified_peers(s.chats.get(&meeting), peers_from(&json), &me)
                .into_iter()
                .filter(|p| p.node_id != me)
                .map(|p| p.name)
                .collect();
            if room.waiting_people.as_ref() == Some(&names) {
                return Update::DoNothing;
            }
            eprintln!(
                "[azmeet] {}: in the waiting room: {}",
                s.name,
                ui::who_is_here(&names)
            );
            room.waiting_people = Some(names);
            Update::RefreshDom
        }
        Ok((404, _)) => {
            s.notice = String::from("This meeting has ended, or the link is wrong.");
            Update::RefreshDom
        }
        _ => Update::DoNothing,
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
    let endpoint = bind_endpoint(&relay, relay_only());
    println!(
        "AZMEET_TRANSPORT {}",
        rooms::transport_label(&relay, relay_only())
    );
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
/// server that answers is remembered in `meet/settings.json` (`flush_files`).
extern "C" fn on_health(data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
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
    let worker = room.worker.clone();
    remember(s, |prefs| prefs.server = Some(worker));
    save_prefs(s, &mut info);
    Update::RefreshDom
}

// ==== The data tree: the settings, each meeting's record and chat (`store.rs`) ====

/// The data root this run keeps its files in: azul-appkit's (`--data-dir`, `AZLIN_DATA`, else
/// `<data dir>/Azlin`). `None` in a headless run that was given none: a test never touches the
/// user's files.
fn files_root() -> Option<&'static std::path::Path> {
    static ROOT: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();
    ROOT.get_or_init(|| {
        let flag = launch_args().kit.data_dir.as_deref();
        let given = flag.is_some() || std::env::var_os(azul_appkit::data::DATA_VAR).is_some();
        (devices_allowed() || given).then(|| store::data_root(flag))
    })
    .as_deref()
}

/// `meet/settings.json` as it was at start (the defaults without one), read once before any
/// window opens: what decides the name, the meeting server and the look before a kit exists.
fn saved_settings() -> &'static azul_appkit::AppSettings {
    static SAVED: std::sync::OnceLock<azul_appkit::AppSettings> = std::sync::OnceLock::new();
    SAVED.get_or_init(|| files_root().map(store::load_settings).unwrap_or_default())
}

/// A participant's azul-appkit kit: the settings page and `meet/settings.json` (the kit reads it
/// now and writes it on every change). A headless run without a data root of its own
/// (`files_root` is `None`) gives the kit an empty folder of this process in the temp folder, so
/// a test never reads or writes the user's settings.
fn make_kit() -> RefAny {
    let mut kit_args = launch_args().kit.clone();
    if files_root().is_none() {
        let scratch = std::env::temp_dir().join(format!("azmeet-{}", std::process::id()));
        kit_args.data_dir = Some(scratch);
    }
    kit::create_kit(
        args::SPEC,
        ABOUT,
        &keys::SHORTCUTS,
        &ui::APP_CATEGORIES,
        kit_args,
    )
}

/// What the kit's settings remember (`store::Prefs`).
fn kit_prefs(kit_ref: &RefAny) -> store::Prefs {
    let mut kit_ref = kit_ref.clone();
    kit_ref
        .downcast_ref::<kit::Kit>()
        .map(|k| store::Prefs::read(&k.settings))
        .unwrap_or_default()
}

/// Changes what AzMeet remembers besides the theme and the mode (`store::Prefs`) in the kit's
/// settings - the one copy, which the kit's own Appearance writes too; [`save_prefs`] writes it.
fn remember(s: &mut MeetState, change: impl FnOnce(&mut store::Prefs)) {
    let changed = {
        let mut kit_ref = s.kit.clone();
        let Some(mut k) = kit_ref.downcast_mut::<kit::Kit>() else {
            return;
        };
        let mut prefs = store::Prefs::read(&k.settings);
        let before = prefs.clone();
        change(&mut prefs);
        if prefs != before {
            prefs.write(&mut k.settings);
        }
        prefs != before
    };
    if changed {
        s.prefs_unsaved = true;
    }
}

/// Has the kit write `meet/settings.json` on its file thread (`AZMEET_SETTINGS_SAVED`) when
/// [`remember`] changed something since the last time. Not in a headless run without a data root
/// of its own (`files_root` is `None`): its kit only remembers.
fn save_prefs(s: &mut MeetState, info: &mut CallbackInfo) {
    if !std::mem::take(&mut s.prefs_unsaved) {
        return;
    }
    if files_root().is_some() {
        kit::save_settings(&s.kit, info);
    }
}

/// The folder name of the meeting this side is in: the meeting server's room id.
fn meeting_name(s: &MeetState) -> String {
    s.room
        .as_ref()
        .map(|room| room.room_id.clone())
        .unwrap_or_default()
}

/// The record of the meeting just entered (`meeting.json`): this side first. Its link is the bare
/// one: the invite secret stays out of the plain files (`meet/rooms.json` keeps it sealed).
fn enter_record(s: &mut MeetState) {
    let (link, server) = match &s.room {
        Some(room) => (
            rooms::link_with_secret(&room.link, None),
            room.worker.clone(),
        ),
        None => (String::new(), String::new()),
    };
    s.record = Some(store::MeetingRecord {
        meeting: meeting_name(s),
        link,
        server,
        joined: azul_storage::time::now_unix(),
        people: vec![s.name.clone()],
    });
    s.unsaved.record = true;
}

/// Everyone met in this meeting goes into its record, once, by their member's name (a peer whose
/// announcement no member signed is not in the call).
fn note_people(s: &mut MeetState) {
    let names: Vec<String> = match &s.room {
        Some(room) => s
            .remotes
            .iter()
            .filter_map(|r| {
                room.peers
                    .iter()
                    .find(|p| p.node_id == r.node_id)
                    .map(|p| p.name.clone())
            })
            .collect(),
        None => Vec::new(),
    };
    let Some(record) = s.record.as_mut() else {
        return;
    };
    for name in names {
        if record.met(&name) {
            s.unsaved.record = true;
        }
    }
}

/// Writes what changed since the last call into the data tree, on the save thread (never here):
/// the meeting's record, each changed room's chat, the room list (the settings are the kit's:
/// [`save_prefs`]). Nothing in a run without a data root. Called on every pump: without a change
/// it returns at once.
fn flush_files(s: &mut MeetState, info: &mut CallbackInfo) {
    let mut unsaved = std::mem::take(&mut s.unsaved);
    let meeting = meeting_name(s);
    // The rooms whose earlier visit's files are on their way back: written once they are in
    // (`on_history_read`), never the new ones over them.
    if unsaved.record && s.reading_history.contains(&meeting) {
        s.unsaved.record = true;
        unsaved.record = false;
    }
    let reading: Vec<String> = unsaved
        .chats
        .iter()
        .filter(|id| s.reading_history.contains(*id))
        .cloned()
        .collect();
    for id in reading {
        unsaved.chats.remove(&id);
        s.unsaved.chats.insert(id);
    }
    if !unsaved.record && unsaved.chats.is_empty() && !unsaved.index {
        return;
    }
    let Some(root) = files_root() else {
        return;
    };
    let mut files = Vec::new();
    if unsaved.record {
        if let (Some(key), Some(record)) = (store::meeting_key(&meeting), s.record.as_ref()) {
            files.push((key, record.to_json().into_bytes()));
        }
    }
    for id in &unsaved.chats {
        if let (Some(key), Some(chat)) = (store::chat_key(id), s.chats.get(id)) {
            files.push((key, store::chat_lines(chat.messages()).into_bytes()));
        }
    }
    if unsaved.index {
        files.push((store::index_key(), s.index.to_json().into_bytes()));
    }
    store::save(info, root, files);
}

/// Reads the files an earlier visit to room `room` left (`meet/<room>/chat.jsonl` and
/// `meeting.json`) on a Thread; `on_history_read` lists that chat in its places.
fn read_history(s: &mut MeetState, info: &mut CallbackInfo, app: RefAny, room: &str) {
    let Some(root) = files_root() else {
        return;
    };
    if !s.reading_history.insert(room.to_string()) {
        return;
    }
    store::read_meeting(info, root, room, app, on_history_read);
}

/// The earlier visit's files are in: its chat is listed in its places (`AZMEET_CHAT_RESTORED
/// <n>`, the messages the file held; one the meeting server still has is listed once), the
/// people it met join the record; then the files are written whole.
extern "C" fn on_history_read(mut data: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some((meeting, chat, record)) = msg
        .downcast_ref::<store::Earlier>()
        .map(|e| (e.meeting.clone(), e.chat.clone(), e.record.clone()))
    else {
        return Update::DoNothing;
    };
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    s.reading_history.remove(&meeting);
    let me = s
        .identity
        .as_ref()
        .map(|i| i.device().to_string())
        .unwrap_or_default();
    let earlier = chat
        .map(|text| store::parse_chat(&text, &me))
        .unwrap_or_default();
    if !earlier.is_empty() {
        println!("AZMEET_CHAT_RESTORED {}", earlier.len());
        if let Some(room) = s.chats.get_mut(&meeting) {
            room.restore(earlier);
        }
        s.unsaved.chats.insert(meeting.clone());
    }
    if meeting_name(s) == meeting {
        let people = record
            .map(|text| store::record_people(&text))
            .unwrap_or_default();
        if let Some(current) = s.record.as_mut() {
            for name in people {
                if current.met(&name) {
                    s.unsaved.record = true;
                }
            }
        }
    }
    flush_files(s, &mut info);
    Update::RefreshDom
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

/// This computer's clock, milliseconds since 1970 (what the meeting server's times are in).
fn wall_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// This computer's time zone (seconds east of UTC) at `unix` seconds: summer time where it is
/// summer time then.
fn local_offset_at(unix: u64) -> i32 {
    use chrono::TimeZone;
    i64::try_from(unix)
        .ok()
        .and_then(|t| chrono::Local.timestamp_opt(t, 0).single())
        .map_or_else(
            || chrono::Local::now().offset().local_minus_utc(),
            |t| t.offset().local_minus_utc(),
        )
}

/// What the start screen says while no meeting server is set.
fn no_server_notice() -> String {
    String::from(
        "No meeting server is set: type its address under Meeting server and press Enter, or \
         start AzMeet with --worker <url>.",
    )
}

/// A check of the meeting server (`GET /health`), counted and timed.
fn check_server(room: &mut RoomSession, now: u64) -> HttpJob {
    room.checks = room.checks.wrapping_add(1);
    room.checked_at = now;
    room.server_status = format!("Asking {} ...", room.worker);
    HttpJob::health(room)
}

/// On the start screen, a meeting server that did not answer is asked again every
/// [`SERVER_RETRY_MS`]: an outage ends by itself, not with a restart.
fn server_retry(s: &mut MeetState) -> Option<HttpJob> {
    let room = s.room.as_mut()?;
    if room.stage != Stage::Start || room.server_ok || room.worker.is_empty() {
        return None;
    }
    let now = wall_ms();
    if now.saturating_sub(room.checked_at) < SERVER_RETRY_MS {
        return None;
    }
    Some(check_server(room, now))
}

/// The signer of this device's announcements in the call's room: its identity and the meeting
/// server's time, once the room's chat says this device is a member (the Worker takes an
/// announcement from a member only).
fn call_signer_ts(s: &MeetState) -> Option<u64> {
    let id = s.room.as_ref().map(|room| room.room_id.as_str())?;
    s.chats
        .get(id)
        .filter(|c| c.state == chatroom::Membership::Member)
        .map(|c| c.server_now(wall_ms()))
}

/// The ticket this endpoint announces names its addresses as they are now: the `Ready` event's
/// may predate its home relay (or a new network), and without the relay in the ticket a peer
/// behind NAT cannot be reached where no address lookup runs (azcloud's lesson, 10267afec). A
/// changed ticket is announced at once; a peer not connected yet dials the new one.
fn refresh_ticket(s: &mut MeetState) {
    let (Some(endpoint), Some(room)) = (s.endpoint.as_ref(), s.room.as_mut()) else {
        return;
    };
    if room.ticket.is_empty() {
        // Not ready yet: the `Ready` event brings the first one.
        return;
    }
    let now = endpoint.ticket().as_str().to_string();
    if !now.is_empty() && now != room.ticket {
        eprintln!("[azmeet] {}: this endpoint's addresses changed: announcing them", s.name);
        room.ticket = now;
        room.announced_at = None;
    }
}

/// Every 2 seconds: in a room, announce when due (once this device is a member there), else read
/// the peers list; on the start screen, ask a meeting server that did not answer again now and
/// then.
extern "C" fn room_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let job = match data.downcast_mut::<MeetState>() {
        Some(mut guard) => {
            let s = &mut *guard;
            refresh_ticket(s);
            let ts = call_signer_ts(s);
            let signer = s.identity.as_ref().zip(ts);
            let job = s.room.as_mut().and_then(|room| room.next_job(signer));
            job.or_else(|| server_retry(s))
        }
        None => None,
    };
    if let Some(job) = job {
        spawn_http(&mut info.callback_info, data.clone(), job);
    }
    TimerCallbackReturn::continue_unchanged()
}

/// Every [`CHAT_TICK_MS`]: each room's next request (`ChatRoom::next_call`), on its Thread.
extern "C" fn chat_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let jobs = match data.downcast_mut::<MeetState>() {
        Some(mut guard) => chat_jobs(&mut guard),
        None => Vec::new(),
    };
    for job in jobs {
        spawn_http(&mut info.callback_info, data.clone(), job);
    }
    TimerCallbackReturn::continue_unchanged()
}

/// The requests the rooms send now: the room on screen is read every [`CHAT_OPEN_SYNC_MS`], the
/// others every [`CHAT_IDLE_SYNC_MS`] (their unread counts).
fn chat_jobs(s: &mut MeetState) -> Vec<HttpJob> {
    let open = active_room_id(s);
    let now = wall_ms();
    let Some(me) = s.identity.as_ref() else {
        return Vec::new();
    };
    let mut jobs = Vec::new();
    for (id, chat) in s.chats.iter_mut() {
        let every = if open.as_deref() == Some(id.as_str()) {
            CHAT_OPEN_SYNC_MS
        } else {
            CHAT_IDLE_SYNC_MS
        };
        if let Some(call) = chat.next_call(me, now, every) {
            jobs.push(HttpJob::chat(chat, me, call, now));
        }
    }
    jobs
}

/// A room's answer: into its `ChatRoom`, and what that changed out to stdout, the files and the
/// window (`apply_chat_changes`).
extern "C" fn on_chat_answer(data: RefAny, mut info: CallbackInfo, result: RefAny) -> Update {
    let Some((mut app, room, what)) = reply_chat(data) else {
        return Update::DoNothing;
    };
    let (status, body) = http_reply(result);
    let handle = app.clone();
    let (update, follow_up) = {
        let Some(mut guard) = app.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let changes = {
            let (Some(me), Some(chat)) = (s.identity.as_ref(), s.chats.get_mut(&room)) else {
                return Update::DoNothing;
            };
            chat.on_answer(me, &what, status, &body, wall_ms())
        };
        apply_chat_changes(s, &mut info, &handle, &room, changes)
    };
    if let Some(job) = follow_up {
        spawn_http(&mut info, handle, job);
    }
    update
}

/// The name of the member `device` of room `room`, as the window shows it.
fn chat_member_name(s: &MeetState, room: &str, device: &str) -> String {
    s.chats
        .get(room)
        .and_then(|c| c.member(device))
        .map(|m| member_name(&m.name))
        .unwrap_or_else(|| short_id(device).to_string())
}

/// What a room's answer changed: the lines for scripts (`AZMEET_CHAT`, `AZMEET_KEY`,
/// `AZMEET_MEMBER`, `AZMEET_HISTORY`, `AZMEET_ADMITTED`), what is on screen marked read, the files,
/// the room list, a knock let in entering its meeting, a room left forgotten. The window's
/// update, and a request to send.
fn apply_chat_changes(
    s: &mut MeetState,
    info: &mut CallbackInfo,
    app: &RefAny,
    id: &str,
    changes: chatroom::Changes,
) -> (Update, Option<HttpJob>) {
    for m in &changes.arrived {
        let name = member_name(&m.name);
        println!("AZMEET_CHAT {name}: {}", m.text);
        eprintln!("[azmeet] {}: chat from {name}: {}", s.name, m.text);
    }
    for k in &changes.keys {
        let by = if k.made_here {
            String::from("me")
        } else {
            chat_member_name(s, id, &k.sender)
        };
        println!(
            "AZMEET_KEY {id} epoch={} key={} members={} by={by}",
            k.epoch, k.key_id, k.members
        );
    }
    for (what, list) in [
        ("joined", &changes.joined),
        ("left", &changes.left),
        ("knocking", &changes.knocking),
    ] {
        for m in list.iter() {
            println!(
                "AZMEET_MEMBER {id} {what} {} {}",
                member_name(&m.name),
                m.safety_code
            );
        }
    }
    if let Some(n) = changes.history {
        println!("AZMEET_HISTORY {id} {n}");
    }
    if changes.admitted {
        println!("AZMEET_ADMITTED {id}");
    }
    if let Some(problem) = &changes.problem {
        eprintln!("[azmeet] {}: room {}: {problem}", s.name, short_id(id));
        if active_room_id(s).as_deref() == Some(id) {
            s.notice = problem.clone();
        }
    }
    // What is on screen is read.
    let on_screen = s.open_room.as_deref() == Some(id)
        || (call_room_id(s).as_deref() == Some(id) && s.panel == SidePanel::Chat);
    if on_screen {
        if let Some(chat) = s.chats.get_mut(id) {
            chat.mark_read();
        }
    }
    if !changes.arrived.is_empty() || changes.history.is_some() || changes.shown {
        s.unsaved.chats.insert(id.to_string());
    }
    let mut follow_up = None;
    match s.chats.get(id).map(|c| c.state) {
        Some(chatroom::Membership::Left) => forget_room(s, id),
        Some(chatroom::Membership::Member) => {
            remember_room(s, id);
            follow_up = enter_admitted(s, info, app, id);
        }
        Some(_) => remember_room(s, id),
        None => {}
    }
    flush_files(s, info);
    let update = if changes.shown || follow_up.is_some() {
        Update::RefreshDom
    } else {
        Update::DoNothing
    };
    (update, follow_up)
}

/// A knock was let in: the waiting room that asked enters its meeting now, with the link the
/// room's first key brought.
fn enter_admitted(
    s: &mut MeetState,
    info: &mut CallbackInfo,
    app: &RefAny,
    id: &str,
) -> Option<HttpJob> {
    let invite = s.chats.get(id).and_then(|c| c.invite().cloned())?;
    let room = s.room.as_mut()?;
    let waiting_here = room.waiting.as_ref().map(|found| found.room.as_str()) == Some(id);
    if room.stage != Stage::Waiting || !room.asked || !waiting_here {
        return None;
    }
    let mut found = room.waiting.take()?;
    found.invite = Some(invite);
    eprintln!("[azmeet] {}: let in: into the meeting", s.name);
    enter_room(s, info, app, found)
}

/// Room `id` was left: off the room list, out of memory, its view closed.
fn forget_room(s: &mut MeetState, id: &str) {
    if let Some(me) = s.identity.as_ref() {
        if s.index.remove(me.device(), id) {
            s.unsaved.index = true;
        }
    }
    s.chats.remove(id);
    if s.open_room.as_deref() == Some(id) {
        s.open_room = None;
        s.room_copied = false;
    }
    s.notice = String::from("You left the room.");
    println!("AZMEET_LEFT_ROOM {id}");
}

/// Makes a new room here - its id and its invite secret (CRYPTO.md section 4) - and registers it
/// with the meeting server; its waiting room (a chat room's view) opens with the answer
/// (`straight_in`: a meeting is entered at once - a script's). `times`: RFC 3339 start and end.
fn begin_new_room(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    straight_in: bool,
    kind: chatroom::RoomKind,
    times: Option<(String, String)>,
) -> Update {
    let job = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        if s.identity.is_none() {
            s.notice = String::from("This device's key is not loaded yet: a moment, please.");
            return Update::RefreshDom;
        }
        let Some(room) = s.room.as_mut() else {
            return Update::DoNothing;
        };
        if room.stage != Stage::Start || room.busy {
            return Update::DoNothing;
        }
        if room.worker.is_empty() {
            s.notice = no_server_notice();
            return Update::RefreshDom;
        }
        let invite = crypto::random_id()
            .ok()
            .and_then(|id| crypto::Invite::generate(&id).ok());
        let Some(invite) = invite else {
            s.notice = String::from("The system's random source failed: no room can be made.");
            return Update::RefreshDom;
        };
        room.stage = Stage::Opening;
        room.busy = true;
        room.created = true;
        room.straight_in = straight_in;
        room.join_secret = None;
        let minting = Minting { invite, kind, times };
        let job = HttpJob::create_room(room, &minting);
        room.minting = Some(minting);
        s.open_room = None;
        s.notice = String::from(match kind {
            chatroom::RoomKind::Chat => "Making a new chat room...",
            chatroom::RoomKind::Meeting => "Asking the meeting server for a new meeting...",
        });
        job
    };
    spawn_http(info, data.clone(), job);
    Update::RefreshDom
}

/// Looks the meeting of `text` (a link or a code) up; its waiting room (a chat room's view) opens
/// with the answer (`straight_in`: a meeting is entered at once - a script's). A bare link of a
/// room this device is in uses the link it keeps.
fn begin_join(data: &mut RefAny, info: &mut CallbackInfo, text: &str, straight_in: bool) -> Update {
    let job = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        if s.identity.is_none() {
            s.notice = String::from("This device's key is not loaded yet: a moment, please.");
            return Update::RefreshDom;
        }
        let Some(room) = s.room.as_mut() else {
            return Update::DoNothing;
        };
        if room.stage != Stage::Start || room.busy {
            return Update::DoNothing;
        }
        if room.worker.is_empty() {
            s.notice = no_server_notice();
            return Update::RefreshDom;
        }
        let Some(link) = rooms::read_room_link(text) else {
            s.notice = String::from(
                "That is not a meeting link. Paste an azlin://meet/... link, the meeting's web \
                 address, or its code.",
            );
            return Update::RefreshDom;
        };
        let kept = match &link.key {
            RoomKey::Id(id) => s
                .chats
                .get(id)
                .and_then(|c| c.invite().map(|i| i.secret().to_string())),
            RoomKey::Code(_) => None,
        };
        room.stage = Stage::Opening;
        room.busy = true;
        room.created = false;
        room.straight_in = straight_in;
        room.minting = None;
        room.join_secret = link.secret.clone().or(kept);
        s.open_room = None;
        s.notice = String::from("Looking up the meeting...");
        HttpJob::look_up(room, &link.key)
    };
    spawn_http(info, data.clone(), job);
    Update::RefreshDom
}

extern "C" fn on_new_meeting(mut data: RefAny, mut info: CallbackInfo) -> Update {
    begin_new_room(&mut data, &mut info, false, chatroom::RoomKind::Meeting, None)
}

extern "C" fn on_new_chat_room(mut data: RefAny, mut info: CallbackInfo) -> Update {
    begin_new_room(&mut data, &mut info, false, chatroom::RoomKind::Chat, None)
}

/// The times the "Schedule" form asks for: the start in this computer's time zone, the length in
/// minutes; RFC 3339 in UTC, as the Worker takes them.
fn schedule_times(start: &str, minutes: &str) -> Result<(String, String), String> {
    let today = local_offset_at(azul_storage::time::now_unix());
    let first_guess = rooms::parse_local_start(start, today)
        .ok_or_else(|| String::from("Type the start as 2026-10-09 14:00 (your time)."))?;
    // The offset of the day it starts on (summer time or not), not today's.
    let starts =
        rooms::parse_local_start(start, local_offset_at(first_guess)).unwrap_or(first_guess);
    let minutes = minutes
        .trim()
        .parse::<u64>()
        .ok()
        .filter(|m| (1..=10_080).contains(m))
        .ok_or_else(|| String::from("Type how long the meeting lasts, in minutes (1 to 10080)."))?;
    Ok((
        azul_storage::time::iso8601(starts),
        azul_storage::time::iso8601(starts + minutes * 60),
    ))
}

/// "Schedule": a meeting with the form's start and length.
extern "C" fn on_schedule(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let times = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let (start, minutes) = s
            .room
            .as_ref()
            .map(|room| (room.schedule_start.clone(), room.schedule_minutes.clone()))
            .unwrap_or_default();
        match schedule_times(&start, &minutes) {
            Ok(times) => times,
            Err(why) => {
                s.notice = why;
                return Update::RefreshDom;
            }
        }
    };
    begin_new_room(
        &mut data,
        &mut info,
        false,
        chatroom::RoomKind::Meeting,
        Some(times),
    )
}

extern "C" fn on_schedule_start(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        if let Some(room) = s.room.as_mut() {
            room.schedule_start = state.get_text().as_str().to_string();
        }
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_schedule_minutes(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        if let Some(room) = s.room.as_mut() {
            room.schedule_minutes = state.get_text().as_str().to_string();
        }
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

extern "C" fn on_join(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let text = data
        .downcast_ref::<MeetState>()
        .and_then(|s| s.room.as_ref().map(|room| room.join_text.clone()))
        .unwrap_or_default();
    begin_join(&mut data, &mut info, &text, false)
}

/// Enters the meeting `found`: from the waiting room's "Join now" / "Start meeting", or at once
/// for a script. This device joins the room's chat (its member record goes out); announcing
/// starts once it is a member (the announcement to send right away, when it can, is returned),
/// and the meeting's record and the chat of an earlier visit come in.
fn enter_room(
    s: &mut MeetState,
    info: &mut CallbackInfo,
    app: &RefAny,
    found: RoomInfo,
) -> Option<HttpJob> {
    ensure_chat(s, &found);
    let id = found.room.clone();
    if let Some(chat) = s.chats.get_mut(&id) {
        chat.join();
    }
    let times = found.starts_at.zip(found.ends_at);
    let room = s.room.as_mut()?;
    room.enter(found);
    println!("AZMEET_ROOM {}", room.room_id);
    println!("AZMEET_LINK {}", room.link);
    if !room.code.is_empty() {
        println!("AZMEET_CODE {}", room.code);
    }
    if let Some((a, b)) = times {
        println!(
            "AZMEET_TIMES {} {}",
            azul_storage::time::iso8601(a),
            azul_storage::time::iso8601(b)
        );
    }
    eprintln!(
        "[azmeet] {}: in meeting {} ({})",
        s.name, room.code, room.room_id
    );
    s.notice.clear();
    s.link_status = String::from("waiting for others to join");
    s.open_room = None;
    s.room_copied = false;
    remember_room(s, &id);
    enter_record(s);
    // Back in a meeting this side was in before: its chat and people come back.
    read_history(s, info, app.clone(), &id);
    flush_files(s, info);
    // The name typed in the waiting room, should its field still have the focus.
    save_prefs(s, info);
    // The others hear this side's microphone and camera as the waiting room left them.
    network_changed(s, false);
    let ts = call_signer_ts(s);
    let signer = s.identity.as_ref().zip(ts);
    s.room.as_mut()?.first_job(signer)
}

/// A meeting was found: the microphone and the camera start as the settings' Meetings say (on,
/// unless "Join with the microphone off" / "... the camera off"). Not where they are a test tone
/// or a test pattern (a headless run, `--test-tone`, `--test-pattern`): those keep the state
/// their switches gave them, so a script knows what it gets.
fn apply_join_defaults(s: &mut MeetState) {
    if !devices_allowed() {
        return;
    }
    if !setting_on("AZMEET_TEST_TONE") {
        s.mic_on = !s.join_muted;
        sync_mic(s);
    }
    if !setting_on("AZMEET_TEST_PATTERN") {
        s.cam_on = !s.join_camera_off;
    }
}

/// The waiting room's "Join now" / "Start meeting": into the meeting it shows; with only the
/// meeting's code, "Ask to join": a knock, and the meeting is entered once a member lets this
/// device in (`enter_admitted`).
extern "C" fn on_join_now(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let app = data.clone();
    let job = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        if let Some(room) = s.room.as_ref().filter(|room| room.preview) {
            // The preview's meeting exists only here: nobody could join it.
            s.notice = format!(
                "This waiting room is a preview: no meeting server answers at {}. Start one (the \
                 meet Worker's dev server) or pick another on the start screen to meet people.",
                room.worker
            );
            return Update::RefreshDom;
        }
        let knock = s
            .room
            .as_ref()
            .filter(|room| room.stage == Stage::Waiting && !room.asked)
            .and_then(|room| room.waiting.as_ref())
            .filter(|found| found.invite.is_none())
            .map(|found| found.room.clone());
        if let Some(id) = knock {
            if let Some(chat) = s.chats.get_mut(&id) {
                chat.join();
            }
            if let Some(room) = s.room.as_mut() {
                room.asked = true;
            }
            remember_room(s, &id);
            println!("AZMEET_KNOCK {id}");
            s.notice = String::from(
                "Asked to join: someone in the meeting lets you in. Compare your safety code with \
                 theirs.",
            );
            return Update::RefreshDom;
        }
        let found = s
            .room
            .as_mut()
            .filter(|room| {
                room.stage == Stage::Waiting
                    && room.waiting.as_ref().is_some_and(|f| f.invite.is_some())
            })
            .and_then(|room| room.waiting.take());
        let Some(found) = found else {
            return Update::DoNothing;
        };
        enter_room(s, &mut info, &app, found)
    };
    if let Some(job) = job {
        spawn_http(&mut info, data.clone(), job);
    }
    Update::RefreshDom
}

/// The waiting room's Back: to the start screen without joining (a meeting this side made
/// expires on the server by itself; the devices close with the waiting room).
extern "C" fn on_waiting_back(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let Some(room) = s.room.as_mut().filter(|room| room.stage == Stage::Waiting) else {
        return Update::DoNothing;
    };
    // A new session: the answer to a read of this waiting room's peers still in flight is
    // ignored, whatever waiting room opens next.
    room.session = room.session.wrapping_add(1);
    room.stage = Stage::Start;
    room.waiting = None;
    room.close_waiting();
    room.created = false;
    room.copied = false;
    s.notice.clear();
    Update::RefreshDom
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

/// `text` to the clipboard.
fn copy_text(info: &mut CallbackInfo, text: &str) {
    info.set_clipboard_content(ClipboardContent {
        plain_text: AzString::from(text),
        styled_runs: StyledTextRunVec::create(),
        html: OptionString::None,
    });
}

/// "Copy link": the meeting's link with its invite secret (the waiting room's before it is
/// entered) to the clipboard.
extern "C" fn on_copy_link(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let link = {
        let Some(mut s) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let Some(room) = s.room.as_mut() else {
            return Update::DoNothing;
        };
        let link = match &room.waiting {
            Some(found) if room.link.is_empty() => found.share_link(),
            _ => room.link.clone(),
        };
        if link.is_empty() {
            return Update::DoNothing;
        }
        room.copied = true;
        link
    };
    copy_text(&mut info, &link);
    Update::RefreshDom
}

/// The times `--starts-at` / `--ends-at` ask for (both or neither).
fn launch_times() -> Option<(String, String)> {
    setting("AZMEET_STARTS_AT").zip(setting("AZMEET_ENDS_AT"))
}

/// `--open <link>`: the room view of that room - one this device is in at once, another looked
/// up and joined.
fn open_link(data: &mut RefAny, info: &mut CallbackInfo, text: &str) -> Update {
    let known = match rooms::parse_room_link(text) {
        Some(RoomKey::Id(id)) => data
            .downcast_mut::<MeetState>()
            .map(|mut s| {
                let here = s.chats.contains_key(&id);
                if here {
                    open_room_view(&mut s, &id, true);
                }
                here
            })
            .unwrap_or(false),
        _ => false,
    };
    if known {
        return Update::RefreshDom;
    }
    begin_join(data, info, text, false)
}

/// What the command line asked to start with, once this device's identity is loaded (nothing is
/// joined or made before): `--open <link>`, `--join <link>` / `--autocreate` / `--screen call`
/// (a meeting without a click, past the waiting room unless `--waiting-room`; a chat room with
/// `--chat-room`), `--screen waiting` (a new meeting's waiting room, or a preview of one when no
/// meeting server answers). Once.
fn autostart(data: &mut RefAny, info: &mut CallbackInfo) -> Update {
    let due = data
        .downcast_mut::<MeetState>()
        .is_some_and(|mut s| s.identity.is_some() && std::mem::take(&mut s.autostart_due));
    if !due {
        return Update::DoNothing;
    }
    let straight_in = !setting_on("AZMEET_WAITING_ROOM");
    if let Some(link) = setting("AZMEET_OPEN") {
        return open_link(data, info, &link);
    }
    if let Some(link) = setting("AZMEET_JOIN") {
        if let Some(mut s) = data.downcast_mut::<MeetState>() {
            if let Some(room) = s.room.as_mut() {
                room.join_text = link.clone();
            }
        }
        return begin_join(data, info, &link, straight_in);
    }
    let server_ok = data
        .downcast_ref::<MeetState>()
        .is_some_and(|s| s.room.as_ref().is_some_and(|room| room.server_ok));
    let kind = if setting_on("AZMEET_CHAT_ROOM") {
        chatroom::RoomKind::Chat
    } else {
        chatroom::RoomKind::Meeting
    };
    match launch_args().screen {
        args::Screen::Call => begin_new_room(data, info, straight_in, kind, launch_times()),
        args::Screen::Waiting if server_ok => {
            begin_new_room(data, info, false, kind, launch_times())
        }
        args::Screen::Waiting => preview_waiting_room(data),
        _ if setting_on("AZMEET_AUTOCREATE") => {
            begin_new_room(data, info, straight_in, kind, launch_times())
        }
        _ => Update::DoNothing,
    }
}

/// `--screen waiting` with no meeting server answering (a screenshot of the waiting room): the
/// waiting room of a meeting that exists only here - a code made up here, its link, nobody in
/// it; "Join now" says that no meeting server answers. stdout: `AZMEET_WAITING <link>`, as for a
/// real one.
fn preview_waiting_room(data: &mut RefAny) -> Update {
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
    let code = gen_link();
    let found = RoomInfo {
        room: code.clone(),
        link: format!("{}{code}", rooms::APP_LINK_PREFIX),
        code,
        kind: chatroom::RoomKind::Meeting,
        starts_at: None,
        ends_at: None,
        invite_key: None,
        invite: None,
    };
    println!("AZMEET_WAITING {}", found.link);
    eprintln!(
        "[azmeet] {}: a preview of the waiting room (meeting {}): no meeting server answers at {}",
        s.name, found.code, room.worker
    );
    room.stage = Stage::Waiting;
    room.created = true;
    room.copied = false;
    room.waiting = Some(found);
    room.close_waiting();
    room.preview = true;
    s.notice = if room.worker.is_empty() {
        String::from("A preview of the waiting room: no meeting server is set.")
    } else {
        format!(
            "A preview of the waiting room: no meeting server answers at {}.",
            room.worker
        )
    };
    apply_join_defaults(s);
    Update::RefreshDom
}

// ==== This device's identity (`identity.rs`, CRYPTO.md section 3) ====

/// A keyring answer, by name (never with the secret).
fn keyring_outcome(result: &KeyringResult) -> &'static str {
    match result {
        KeyringResult::Stored => "stored",
        KeyringResult::Retrieved(_) => "retrieved",
        KeyringResult::Deleted => "deleted",
        KeyringResult::NotFound => "not found",
        KeyringResult::Denied => "denied",
        KeyringResult::Unavailable => "unavailable",
        KeyringResult::Error => "error",
    }
}

/// This device's identity: from the identity file (`--identity-file`) at once, else the keyring
/// is asked (its answer is the window's `KeyringResult` event: `on_keyring_result`).
fn load_identity(data: &mut RefAny, info: &mut CallbackInfo) {
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return;
    };
    let s = &mut *guard;
    if s.identity.is_some() || s.keyring.is_some() {
        return;
    }
    match setting("AZMEET_IDENTITY_FILE") {
        Some(path) => match identity::load_or_create(std::path::Path::new(&path)) {
            Ok((me, created)) => identity_ready(s, me, identity::Source::File(path), created),
            Err(why) => {
                eprintln!("[azmeet] {}: {why}", s.name);
                session_identity(s, &format!("was not used: the identity file {why}"));
            }
        },
        None => {
            info.keyring_get(identity::KEYRING_KEY);
            s.keyring = Some(KeyringStep::Get);
        }
    }
}

/// An identity for this run only (`why`: what the keyring said).
fn session_identity(s: &mut MeetState, why: &str) {
    match crypto::Identity::generate() {
        Ok(me) => {
            s.notice = format!(
                "The system keyring {why}: this device's key lives only until AzMeet quits, and \
                 its rooms do not come back after."
            );
            identity_ready(s, me, identity::Source::Session(why.to_string()), true);
        }
        Err(e) => {
            s.notice = format!("No device key can be made ({e}): AzMeet cannot join a room.");
        }
    }
}

/// The identity is here: stdout says it (`AZMEET_IDENTITY <device> <keyring|file|session>`,
/// `AZMEET_SAFETY <code>`), and the rooms of `meet/rooms.json` come back.
fn identity_ready(
    s: &mut MeetState,
    me: crypto::Identity,
    source: identity::Source,
    created: bool,
) {
    println!("AZMEET_IDENTITY {} {}", me.device(), source.word());
    println!("AZMEET_SAFETY {}", me.safety_code());
    eprintln!(
        "[azmeet] {}: this device {} ({}){}",
        s.name,
        short_id(me.device()),
        source.describe(),
        if created { ", made now" } else { "" }
    );
    s.identity = Some(me);
    s.identity_source = Some(source);
    restore_rooms(s);
}

/// The rooms `meet/rooms.json` lists for this device: back in memory, read on the next ticks. A
/// room whose invite secret does not open here (another device's seal) is left out.
fn restore_rooms(s: &mut MeetState) {
    let Some(me) = s.identity.as_ref() else {
        return;
    };
    let worker = s.room.as_ref().map(|r| r.worker.clone()).unwrap_or_default();
    let entries: Vec<roomlist::RoomEntry> = s
        .index
        .rooms_of(me.device())
        .into_iter()
        .cloned()
        .collect();
    for entry in entries {
        if s.chats.contains_key(&entry.room) {
            continue;
        }
        let invite = entry
            .invite
            .as_deref()
            .and_then(|sealed| me.open_local(&entry.room, sealed))
            .and_then(|secret| crypto::Invite::new(&entry.room, &secret));
        if entry.member && invite.is_none() {
            eprintln!(
                "[azmeet] {}: room {}: its link does not open on this device; left out",
                s.name,
                short_id(&entry.room)
            );
            continue;
        }
        let server = if entry.server.is_empty() {
            worker.clone()
        } else {
            entry.server.clone()
        };
        let mut chat = chatroom::ChatRoom::new(
            &server,
            &entry.room,
            &entry.code,
            chatroom::RoomKind::parse(Some(entry.kind.as_str())),
            invite,
            &s.name,
        );
        chat.state = if entry.member {
            chatroom::Membership::Member
        } else {
            chatroom::Membership::Knocking
        };
        chat.read_seq = entry.read_seq;
        chat.starts_at = entry.starts_at;
        chat.ends_at = entry.ends_at;
        chat.restore_departed(entry.departed.clone());
        s.chats.insert(entry.room.clone(), chat);
    }
}

/// The keyring answered (a window event): the seed read, or a new one made and stored, or - when
/// the keyring cannot keep one - a seed for this run only. Then what the command line asked for
/// starts (`autostart`).
extern "C" fn on_keyring_result(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let OptionKeyringResult::Some(result) = info.get_keyring_result() else {
        return Update::DoNothing;
    };
    let ready = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        match (s.keyring.take(), result) {
            (Some(KeyringStep::Get), KeyringResult::Retrieved(secret)) => {
                match crypto::Identity::from_secret_json(secret.as_str()) {
                    Ok(me) => identity_ready(s, me, identity::Source::Keyring, false),
                    Err(e) => session_identity(s, &format!("holds a key AzMeet cannot read ({e})")),
                }
                true
            }
            (Some(KeyringStep::Get), KeyringResult::NotFound) => {
                match crypto::Identity::generate() {
                    Ok(me) => {
                        let json = me.to_secret_json();
                        info.keyring_store(identity::KEYRING_KEY, json.as_str(), false);
                        s.keyring = Some(KeyringStep::Store);
                        identity_ready(s, me, identity::Source::Keyring, true);
                    }
                    Err(e) => {
                        s.notice =
                            format!("No device key can be made ({e}): AzMeet cannot join a room.");
                    }
                }
                true
            }
            (Some(KeyringStep::Get), other) => {
                session_identity(s, &format!("is {}", keyring_outcome(&other)));
                true
            }
            (Some(KeyringStep::Store), KeyringResult::Stored) => {
                eprintln!("[azmeet] {}: this device's key is in the system keyring", s.name);
                false
            }
            (Some(KeyringStep::Store), other) => {
                let why = format!("did not keep it ({})", keyring_outcome(&other));
                s.notice = format!(
                    "The system keyring {why}: this device's key lives only until AzMeet quits."
                );
                s.identity_source = Some(identity::Source::Session(why));
                false
            }
            (None, _) => false,
        }
    };
    if ready {
        let _ = autostart(&mut data, &mut info);
    }
    Update::RefreshDom
}

// ==== The room view: a room's chat, members, safety codes, outside a call ====

/// A row's button (a room of the list, a member, a knock): the app and the row's key.
struct RowClick {
    app: RefAny,
    key: String,
}

/// The app and the key of a row's click.
fn row_click(mut data: RefAny) -> Option<(RefAny, String)> {
    let click = data.downcast_ref::<RowClick>()?;
    Some((click.app.clone(), click.key.clone()))
}

/// The room view of room `id`: its chat read now (and every 2 seconds while it is open), and with
/// `join` this device joins it when it is not in it.
fn open_room_view(s: &mut MeetState, id: &str, join: bool) {
    let Some(chat) = s.chats.get_mut(id) else {
        return;
    };
    if join {
        chat.join();
    }
    chat.mark_read();
    s.open_room = Some(id.to_string());
    s.room_copied = false;
    s.notice.clear();
    println!("AZMEET_OPEN {id}");
    remember_room(s, id);
}

/// A room of the start screen's list: its view.
extern "C" fn on_open_room(data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, room)) = row_click(data) else {
        return Update::DoNothing;
    };
    let Some(mut guard) = app.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    open_room_view(&mut guard, &room, false);
    Update::RefreshDom
}

/// The room view's Back: to the start screen.
extern "C" fn on_room_back(mut data: RefAny, _info: CallbackInfo) -> Update {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        s.open_room = None;
        s.room_copied = false;
    }
    Update::RefreshDom
}

/// "Leave room": this device leaves the room (its signed leave goes out next); the others' next
/// message is under a key it has no copy of (CRYPTO.md section 7).
extern "C" fn on_room_leave(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let Some(id) = s.open_room.clone() else {
        return Update::DoNothing;
    };
    if let Some(chat) = s.chats.get_mut(&id) {
        chat.leave();
        println!("AZMEET_LEAVING_ROOM {id}");
        s.notice = String::from("Leaving the room...");
    }
    Update::RefreshDom
}

/// The room view's "Join call": the room's waiting room, then its call.
extern "C" fn on_room_call(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let job = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let Some(id) = s.open_room.clone() else {
            return Update::DoNothing;
        };
        let Some(chat) = s.chats.get(&id) else {
            return Update::DoNothing;
        };
        if chat.state != chatroom::Membership::Member {
            s.notice = String::from("Only a member of the room joins its call.");
            return Update::RefreshDom;
        }
        let found = RoomInfo {
            room: id.clone(),
            code: chat.code.clone(),
            link: format!("{}{id}", rooms::APP_LINK_PREFIX),
            kind: chat.kind,
            starts_at: chat.starts_at,
            ends_at: chat.ends_at,
            invite_key: chat.invite().map(crypto::Invite::invite_key),
            invite: chat.invite().cloned(),
        };
        let Some(room) = s.room.as_mut().filter(|room| room.stage == Stage::Start) else {
            return Update::DoNothing;
        };
        room.session = room.session.wrapping_add(1);
        room.created = false;
        room.straight_in = false;
        room.stage = Stage::Waiting;
        room.copied = false;
        room.waiting = Some(found);
        room.close_waiting();
        let job = room.waiting_job();
        s.open_room = None;
        s.notice.clear();
        apply_join_defaults(s);
        job
    };
    if let Some(job) = job {
        spawn_http(&mut info, data.clone(), job);
    }
    Update::RefreshDom
}

/// The room view's "Copy link": the room's link with its invite secret.
extern "C" fn on_room_copy(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let link = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let Some(link) = s
            .open_room
            .as_ref()
            .and_then(|id| s.chats.get(id))
            .map(|chat| chat.link(rooms::APP_LINK_PREFIX))
        else {
            return Update::DoNothing;
        };
        s.room_copied = true;
        link
    };
    copy_text(&mut info, &link);
    Update::RefreshDom
}

/// "Admit" beside a knock (`<room> <device>`): the device is let in, and gets a key at once.
extern "C" fn on_admit(data: RefAny, _info: CallbackInfo) -> Update {
    let Some((mut app, key)) = row_click(data) else {
        return Update::DoNothing;
    };
    let Some((room, device)) = key.split_once(' ') else {
        return Update::DoNothing;
    };
    let Some(mut guard) = app.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    if let Some(chat) = s.chats.get_mut(room) {
        chat.admit(device);
        println!("AZMEET_ADMITTING {room} {device}");
    }
    Update::RefreshDom
}

/// "Verified" beside a member (`<room> <device>`): the user compared its safety code; kept by
/// device in `meet/rooms.json`, so it holds in every room.
extern "C" fn on_verify(data: RefAny, mut info: CallbackInfo) -> Update {
    let Some((mut app, key)) = row_click(data) else {
        return Update::DoNothing;
    };
    let Some((room, device)) = key.split_once(' ') else {
        return Update::DoNothing;
    };
    let Some(mut guard) = app.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    let name = chat_member_name(s, room, device);
    if s.index.verify(device, &name) {
        s.unsaved.index = true;
        println!("AZMEET_VERIFIED {device}");
    }
    flush_files(s, &mut info);
    Update::RefreshDom
}

/// The start screen's Retry: the meeting server is asked again now.
extern "C" fn on_retry_server(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let job = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let Some(room) = s.room.as_mut().filter(|room| room.stage == Stage::Start) else {
            return Update::DoNothing;
        };
        if room.worker.is_empty() {
            room.server_status = no_server_notice();
            return Update::RefreshDom;
        }
        check_server(room, wall_ms())
    };
    spawn_http(&mut info, data.clone(), job);
    Update::RefreshDom
}

/// The window is up: the pumps start (the iroh link, the room's polls, the chat rooms), this
/// device's identity is loaded, and what the command line asked for starts once it is.
extern "C" fn startup(mut data: RefAny, mut info: CallbackInfo) -> Update {
    // azul-appkit's `--shot <png>`: the window as a PNG after its settle time, then the process
    // ends (`AzMeet --screen waiting --test-pattern --shot waiting.png`).
    if let Some(kit_ref) = data.downcast_ref::<MeetState>().map(|s| s.kit.clone()) {
        kit::on_window_created(&kit_ref, &mut info);
    }
    let get_time = info.get_system_time_fn();
    info.add_timer(
        TimerId::unique(),
        Timer::create(data.clone(), pump_link, get_time)
            .with_interval(Duration::System(SystemTimeDiff::from_millis(pace::BUSY_MS))),
    );
    let get_time = info.get_system_time_fn();
    info.add_timer(
        TimerId::unique(),
        Timer::create(data.clone(), room_tick, get_time)
            .with_interval(Duration::System(SystemTimeDiff::from_millis(ROOM_POLL_MS))),
    );
    let get_time = info.get_system_time_fn();
    info.add_timer(
        TimerId::unique(),
        Timer::create(data.clone(), chat_tick, get_time)
            .with_interval(Duration::System(SystemTimeDiff::from_millis(CHAT_TICK_MS))),
    );
    load_identity(&mut data, &mut info);
    autostart(&mut data, &mut info)
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

/// Leaves the call: disconnects every peer, stops announcing and polling, and returns to the
/// start screen, where the room stays in "Your rooms" (its chat goes on; "Leave room" in its view
/// leaves the room itself). Returns the signed request that takes this endpoint off the room's
/// list at once (without it the meeting server drops the record after its peer TTL).
fn leave_meeting(s: &mut MeetState) -> Option<HttpJob> {
    if let Some(endpoint) = s.endpoint.as_ref() {
        for r in &s.remotes {
            endpoint.disconnect(r.handle);
        }
        for p in &s.pending {
            endpoint.disconnect(p.handle);
        }
    }
    s.remotes.clear();
    s.pending.clear();
    drop_audio(s, None);
    s.packetizer.reset();
    s.opus_framer.reset();
    for out in s.video_out.values_mut() {
        out.stop();
    }
    s.drop_video.clear();
    s.plan = routes::Plan::default();
    s.relay = Relaying::default();
    s.record = None;
    s.unsaved.record = false;
    s.speaker = speaker::ActiveSpeaker::new();
    s.link_status = String::from("not in a meeting");
    s.notice = String::from("You left the meeting.");
    let ts = call_signer_ts(s).unwrap_or_else(wall_ms);
    let room = s.room.as_mut()?;
    // An ended meeting is gone from the server already.
    let job = match s.identity.as_ref() {
        Some(me) if room.stage == Stage::InRoom && !room.node_id.is_empty() => {
            Some(HttpJob::leave(room, me, ts))
        }
        _ => None,
    };
    eprintln!("[azmeet] {}: left meeting {}", s.name, room.code);
    room.leave();
    job
}

extern "C" fn on_leave(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let job = match data.downcast_mut::<MeetState>() {
        Some(mut guard) => {
            // The chat as it stands goes into the meeting's folder before it is cleared.
            flush_files(&mut guard, &mut info);
            leave_meeting(&mut guard)
        }
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

// ==== The window's other controls: side panel, chat, settings, devices, shortcuts ====

/// The side panel's tabs (`ui::PanelView` order): people, chat, statistics. Opening the chat reads
/// it; opening the statistics shows them as they are now.
extern "C" fn on_panel(mut data: RefAny, _info: CallbackInfo, state: SegmentedState) -> Update {
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    s.panel = match state.selected_index {
        1 => SidePanel::Chat,
        2 => SidePanel::Statistics,
        _ => SidePanel::People,
    };
    if s.panel == SidePanel::Chat {
        if let Some(id) = call_room_id(s) {
            if s.chats.get_mut(&id).is_some_and(chatroom::ChatRoom::mark_read) {
                remember_room(s, &id);
            }
        }
    }
    if s.panel == SidePanel::Statistics {
        s.stats_shown = stats_lines(s);
    }
    Update::RefreshDom
}

/// The chat field as typed.
extern "C" fn on_chat_text(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        s.chat_draft = state.get_text().as_str().to_string();
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

/// Enter in the chat field sends the message.
extern "C" fn on_chat_key(
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
            let field = info.get_hit_node();
            send_draft(&mut data, &mut info, Some(field), &text)
        }
        _ => Update::DoNothing,
    };
    OnTextInputReturn {
        update,
        valid: TextInputValid::Yes,
    }
}

/// The chat's Send button (the field beside it, `ui::chat`, lost the focus first and handed
/// over its text: `on_chat_blur`).
extern "C" fn on_chat_send(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let text = data
        .downcast_ref::<MeetState>()
        .map(|s| s.chat_draft.clone())
        .unwrap_or_default();
    let field = info.get_previous_sibling(info.get_hit_node()).into_option();
    send_draft(&mut data, &mut info, field, &text)
}

/// The chat field lost the focus: the draft as the field holds it (deletions included).
extern "C" fn on_chat_blur(mut data: RefAny, _info: CallbackInfo, state: TextInputState) -> Update {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        s.chat_draft = state.get_text().as_str().to_string();
    }
    Update::DoNothing
}

/// Sends `text` to the chat and empties the field (`field`: the chat field, when known);
/// nothing for an empty message. The meeting's `chat.jsonl` is written again.
fn send_draft(
    data: &mut RefAny,
    info: &mut CallbackInfo,
    field: Option<DomNodeId>,
    text: &str,
) -> Update {
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return Update::DoNothing;
    };
    let s = &mut *guard;
    if !send_chat(s, text) {
        return Update::DoNothing;
    }
    s.chat_draft.clear();
    flush_files(s, info);
    // Re-rendering the field with an empty draft is not enough: what the user typed outranks
    // the DOM until the app SETS the text.
    if let Some(field) = field {
        TextInput::set_text_in(*info, field, AzString::from(""));
    }
    Update::RefreshDom
}

/// The name field (the waiting room, the settings' Meetings): the name others see (each room's
/// member record carries it, sealed, once the field is left: `on_name_blur`), remembered - and
/// written when the field is left or the meeting is entered, not with every key.
extern "C" fn on_name_text(
    mut data: RefAny,
    _info: CallbackInfo,
    state: TextInputState,
) -> OnTextInputReturn {
    let text = state.get_text().as_str().trim().to_string();
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        if !text.is_empty() {
            s.name = text.clone();
            remember(s, |prefs| prefs.name = Some(text));
        }
    }
    OnTextInputReturn {
        update: Update::DoNothing,
        valid: TextInputValid::Yes,
    }
}

/// The name field was left: the name typed is written into `meet/settings.json`, and every room
/// gets this device's record again with it (sealed with each room's link).
extern "C" fn on_name_blur(
    mut data: RefAny,
    mut info: CallbackInfo,
    _state: TextInputState,
) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        for chat in s.chats.values_mut() {
            chat.set_name(&s.name);
        }
        save_prefs(s, &mut info);
    }
    Update::DoNothing
}

/// The gear (and Mod+, through the kit's keys): azul-appkit's settings page.
extern "C" fn on_settings_open(mut data: RefAny, _info: CallbackInfo) -> Update {
    let Some(kit_ref) = data.downcast_ref::<MeetState>().map(|s| s.kit.clone()) else {
        return Update::DoNothing;
    };
    kit::open_settings(&kit_ref, None);
    Update::RefreshDom
}

/// The microphone picked (the waiting room, the settings), remembered by its name.
/// (`MicrophoneWidget` and `AudioSink` open the system's default device on every platform
/// today; the pick is kept for when they take one.)
extern "C" fn on_mic_choice(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        s.mic_choice = index;
        s.mic_name = device_name(&s.mics, index);
        let name = s.mic_name.clone();
        remember(s, |prefs| prefs.microphone = name);
        save_prefs(s, &mut info);
    }
    Update::RefreshDom
}

/// The speaker picked (see `on_mic_choice`).
extern "C" fn on_speaker_choice(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        s.speaker_choice = index;
        s.speaker_name = device_name(&s.speakers, index);
        let name = s.speaker_name.clone();
        remember(s, |prefs| prefs.speaker = name);
        save_prefs(s, &mut info);
    }
    Update::RefreshDom
}

/// The camera picked: the camera widget opens the one facing that way; remembered.
extern "C" fn on_camera_choice(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        s.camera_choice = index.min(CAMERAS.len() - 1);
        let camera = s.camera_choice;
        remember(s, |prefs| prefs.camera = camera);
        save_prefs(s, &mut info);
    }
    Update::RefreshDom
}

/// The highest rendition this side asks for, by the quality picked in the settings.
const QUALITY_CAPS: [u32; 3] = [720, 360, 180];

/// The video quality picked in the settings: what this side asks for changes at once, and it is
/// remembered.
extern "C" fn on_quality(mut data: RefAny, mut info: CallbackInfo, index: usize) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        s.quality = index.min(QUALITY_CAPS.len() - 1);
        let quality = s.quality;
        remember(s, |prefs| prefs.quality = quality);
        save_prefs(s, &mut info);
        network_changed(s, false);
    }
    Update::RefreshDom
}

/// "Mirror my video": this side's own picture turned as a mirror shows it, or not; remembered.
extern "C" fn on_mirror(mut data: RefAny, mut info: CallbackInfo, state: CheckBoxState) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        s.mirror = state.checked;
        remember(s, |prefs| prefs.mirror = state.checked);
        save_prefs(s, &mut info);
    }
    Update::RefreshDom
}

/// "Join with the microphone off": how the waiting room's switch starts; remembered.
extern "C" fn on_join_muted(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: CheckBoxState,
) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        s.join_muted = state.checked;
        remember(s, |prefs| prefs.join_muted = state.checked);
        save_prefs(s, &mut info);
    }
    Update::DoNothing
}

/// "Join with the camera off": how the waiting room's switch starts; remembered.
extern "C" fn on_join_camera_off(
    mut data: RefAny,
    mut info: CallbackInfo,
    state: CheckBoxState,
) -> Update {
    if let Some(mut guard) = data.downcast_mut::<MeetState>() {
        let s = &mut *guard;
        s.join_camera_off = state.checked;
        remember(s, |prefs| prefs.join_camera_off = state.checked);
        save_prefs(s, &mut info);
    }
    Update::DoNothing
}

/// The mode at `index` (`azul_appkit::ModePref` order: system, light, dark) as azul takes it.
fn mode_option(index: usize) -> OptionDarkLightMode {
    match index {
        1 => OptionDarkLightMode::Some(DarkLightMode::Light),
        2 => OptionDarkLightMode::Some(DarkLightMode::Dark),
        _ => OptionDarkLightMode::None,
    }
}

/// The window's keys: the kit's first (Mod+, the settings, F1 the shortcuts, Escape closes
/// them), then AzMeet's (`keys::SHORTCUTS`, the rule in `keys::command_for`): Ctrl / Cmd + D the
/// microphone, Ctrl / Cmd + E the camera.
extern "C" fn on_key(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let kit_ref = data.downcast_ref::<MeetState>().map(|s| s.kit.clone());
    if let Some(update) = kit_ref.and_then(|k| kit::handle_key(&k, &mut info)) {
        return update;
    }
    let key = match info
        .get_current_keyboard_state()
        .current_virtual_keycode
        .into_option()
    {
        Some(VirtualKeyCode::D) => keys::Key::D,
        Some(VirtualKeyCode::E) => keys::Key::E,
        _ => keys::Key::Other,
    };
    match keys::command_for(key, info.get_key_modifiers().primary_down()) {
        Some(keys::Command::ToggleMic) => mic_toggle(data, info),
        Some(keys::Command::ToggleCamera) => cam_toggle(data, info),
        None => Update::DoNothing,
    }
}

/// The code of a waiting room's preview (`abc-defg-hij`, `--screen waiting` with no meeting
/// server): a made-up meeting no server knows, so no key and no secret go with it.
fn gen_link() -> String {
    let n = azul_storage::ids::random_seed();
    format!(
        "{:03x}-{:04x}-{:03x}",
        (n & 0xfff) as u16,
        ((n >> 12) & 0xffff) as u16,
        ((n >> 28) & 0xfff) as u16,
    )
}

/// This side's iroh endpoint with the relays of `relay`; `relay_only` (`--relay-only`) binds no
/// UDP socket at all, so no direct path forms and every packet goes through the relay.
fn bind_endpoint(relay: &Relay, relay_only: bool) -> IrohEndpoint {
    let config = IrohConfig::create(ALPN);
    let config = match relay {
        Relay::Off => config.with_relay_mode(IrohRelayMode::Disabled),
        Relay::Default => config.with_relay_mode(IrohRelayMode::Default),
        Relay::Custom(url) => config
            .with_relay_mode(IrohRelayMode::Custom)
            .with_relay_url(url.as_str()),
    };
    IrohEndpoint::bind(config.with_relay_only(relay_only))
}

/// `--relay-only` (`AZMEET_RELAY_ONLY=1`): never a direct path.
fn relay_only() -> bool {
    setting_on("AZMEET_RELAY_ONLY")
}

/// What the statistics' Network section starts with: how this side's packets may travel.
fn transport_line(s: &MeetState) -> Option<String> {
    let room = s.room.as_ref()?;
    Some(format!(
        "Transport: {}",
        rooms::transport_label(&room.relay, relay_only())
    ))
}

fn bind_failure(endpoint: &IrohEndpoint) -> String {
    match endpoint.recv().into_option() {
        Some(event) if event.kind == IrohEventKind::Error => event.text.as_str().to_string(),
        _ => String::from("the iroh endpoint did not bind"),
    }
}

/// The name others see: `--name`, else the one typed in the lobby last time, else
/// `AZMEET_NAME`, else the login name.
fn display_name() -> String {
    if let Some(name) = launch_args().name.clone().filter(|n| !n.trim().is_empty()) {
        return name.trim().to_string();
    }
    if let Some(name) = store::Prefs::read(saved_settings()).name {
        return name;
    }
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

/// Where the Azlin services are for this run, as azul-appkit's shared config resolves them
/// (`azlin_config::resolve_endpoints`): the profile's addresses (`local`, the default: the local
/// stack's; `production`: n0's relays, no meeting server yet), under the config file's
/// `endpoints` (`AZLIN_CONFIG`, else `~/.azlin/config.json`), under the environment
/// (`AZMEET_WORKER`, `AZMEET_RELAY`, `AZLIN_PROFILE`), under the switches (`--worker`,
/// `--relay`). A `--shot` run (a screenshot fixture) reads no config file, as the kit does.
fn azlin_endpoints() -> &'static azlin_config::EffectiveEndpoints {
    static RESOLVED: std::sync::OnceLock<azlin_config::EffectiveEndpoints> =
        std::sync::OnceLock::new();
    RESOLVED.get_or_init(|| {
        let args = launch_args();
        let flags = azlin_config::EndpointFlags {
            meet: args.switch("AZMEET_WORKER").map(String::from),
            relay: args.switch("AZMEET_RELAY").map(String::from),
            ..azlin_config::EndpointFlags::default()
        };
        let home = FilePath::get_home_dir()
            .into_option()
            .map(|dir| std::path::PathBuf::from(dir.inner.as_str()));
        let path = if args.kit.shot.is_some() {
            None
        } else {
            azlin_config::config_path(
                std::env::var(azlin_config::CONFIG_VAR).ok().as_deref(),
                home.as_deref(),
            )
        };
        let loaded = path.map(|path| {
            let (config, problem) = azlin_config::AzlinConfig::load(&path);
            if let Some(problem) = problem {
                eprintln!("[azmeet] {}: {problem}", path.display());
            }
            (path, config)
        });
        let file = loaded
            .as_ref()
            .map(|(path, config)| (path.as_path(), &config.endpoints));
        let env = |var: &str| std::env::var(var).ok();
        let resolved = azlin_config::resolve_endpoints(file, &env, &flags);
        for endpoint in [azlin_config::Endpoint::Meet, azlin_config::Endpoint::Relay] {
            for (source, raw, why) in &resolved.get(endpoint).rejected {
                eprintln!(
                    "[azmeet] {} from {} passed over: {raw:?} {why}",
                    endpoint.key(),
                    source.label()
                );
            }
        }
        resolved
    })
}

/// The shared config's value of `endpoint` as AzMeet takes it: not a profile's built-in address
/// in a screenshot run (`--shot` renders the same everywhere), and not the switch's (AzMeet weighs
/// its switches itself). With where it came from.
fn shared_endpoint(endpoint: azlin_config::Endpoint) -> Option<(&'static str, String)> {
    let resolved = azlin_endpoints().get(endpoint);
    let value = resolved.value.as_deref()?;
    let shot = launch_args().kit.shot.is_some();
    match resolved.source {
        azlin_config::Source::Env(_) | azlin_config::Source::File(_) => {}
        azlin_config::Source::Profile(_) | azlin_config::Source::BuiltIn if !shot => {}
        _ => return None,
    }
    Some((value, resolved.source.label()))
}

/// The meeting server at start, where it came from (for the log and the start screen), and
/// whether it accepts a connection: `--worker` (this run only), else the one saved last time
/// (the start screen's field), else the shared Azlin config's (`AZMEET_WORKER`, else its file's
/// `endpoints.meet`), else one built in at build time (`AZMEET_DEFAULT_WORKER`), else the config
/// profile's (`local`: the local stack's `http://127.0.0.1:8790`), else none - the start screen
/// asks for one. A headless run without a data root of its own reads no settings, so nothing
/// saved outranks the configuration there.
fn meeting_server() -> (String, String, Result<(), String>) {
    let saved = store::Prefs::read(saved_settings()).server;
    let flag = launch_args().switch("AZMEET_WORKER");
    let shared = shared_endpoint(azlin_config::Endpoint::Meet).filter(|_| {
        // A meeting server built in at build time outranks a profile's.
        !matches!(
            azlin_endpoints().get(azlin_config::Endpoint::Meet).source,
            azlin_config::Source::Profile(_) | azlin_config::Source::BuiltIn
        ) || PRODUCTION_WORKER.is_empty()
    });
    let (url, source) = rooms::server_choice(
        flag,
        saved.as_deref(),
        shared.as_ref().map(|(value, _)| *value),
        PRODUCTION_WORKER,
    );
    let from = match source {
        rooms::ServerSource::CommandLine => String::from("--worker"),
        rooms::ServerSource::Saved => String::from("saved on the start screen"),
        rooms::ServerSource::Environment => shared.map(|(_, label)| label).unwrap_or_default(),
        rooms::ServerSource::BuiltIn => String::from("built in (AZMEET_DEFAULT_WORKER)"),
        rooms::ServerSource::Unset => String::from("none set"),
    };
    let answer = if url.is_empty() {
        Err(String::from("none is set"))
    } else {
        probe(&url)
    };
    (url, from, answer)
}

/// The command line this run was started with (`args.rs`), read once by [`start`].
static ARGS: std::sync::OnceLock<args::Args> = std::sync::OnceLock::new();

fn launch_args() -> &'static args::Args {
    ARGS.get_or_init(args::Args::default)
}

/// AzMeet's setting `var` (`AZMEET_RELAY`): its switch (`--relay`), else the environment
/// variable (older scripts); trimmed, `None` when unset or blank (`args.rs`).
fn setting(var: &str) -> Option<String> {
    launch_args().setting(var)
}

/// Whether the setting `var` is on: its switch (`--test-tone`) was given, or the environment
/// variable is `1` (`AZMEET_TEST_TONE=1`).
fn setting_on(var: &str) -> bool {
    launch_args().on(var)
}

/// The app theme and the mode of this run: `--theme` / `--mode` (this run only), else the saved
/// ones.
fn launch_look() -> (azul_appkit::Theme, azul_appkit::ModePref) {
    saved_settings().effective(&launch_args().kit)
}

/// The settings screen the command line asked for, and what the settings remember, on a
/// participant's state.
fn apply_launch_args(s: &mut MeetState) {
    if launch_args().screen == args::Screen::Settings {
        kit::open_settings(&s.kit, None);
    }
    let prefs = kit_prefs(&s.kit);
    s.quality = prefs.quality.min(QUALITY_CAPS.len() - 1);
    s.camera_choice = prefs.camera.min(CAMERAS.len() - 1);
    s.mirror = prefs.mirror;
    s.join_muted = prefs.join_muted;
    s.join_camera_off = prefs.join_camera_off;
    s.mic_name = prefs.microphone;
    s.speaker_name = prefs.speaker;
}

pub fn start() {
    match args::parse(std::env::args().skip(1)) {
        Ok(parsed) if parsed.help => {
            print!("{}", args::usage());
            return;
        }
        Ok(parsed) => {
            let _ = ARGS.set(parsed);
        }
        Err(why) => {
            eprintln!("AzMeet: {why}");
            std::process::exit(2);
        }
    }
    let (worker, from, answer) = meeting_server();
    start_rooms(worker, &from, answer);
}

/// The relays for a meeting server at `worker`: `--relay`, else the shared Azlin config's
/// (`AZMEET_RELAY`, its file's `endpoints.relay`), else none for a meeting server on this machine
/// and the public iroh relays for any other.
fn relay_for(worker: &str) -> Relay {
    let host = server_address(worker)
        .map(|(host, _)| host)
        .unwrap_or_default();
    // A relay someone named (the environment, the config file); a profile's built-in one is the
    // public relays, which the rule below picks anyway for a meeting server elsewhere - and a
    // meeting server on this machine is a local test, which reaches no public relay.
    let named = || {
        let resolved = azlin_endpoints().get(azlin_config::Endpoint::Relay);
        match resolved.source {
            azlin_config::Source::Env(_) | azlin_config::Source::File(_) => resolved.value.clone(),
            _ => None,
        }
    };
    let setting = launch_args()
        .switch("AZMEET_RELAY")
        .map(String::from)
        .or_else(named);
    rooms::relay_choice(setting.as_deref(), &host)
}

/// The window with the start screen, talking to the meeting server at `worker` (none when
/// empty; `from`: where its address came from); `answer` says whether it accepted a connection
/// at start. An unreachable or missing server is said under the field, with a Retry button:
/// nothing else opens instead.
fn start_rooms(worker: String, from: &str, answer: Result<(), String>) {
    let relay = relay_for(&worker);
    let name = display_name();
    if relay_only() && relay == Relay::Off {
        eprintln!(
            "[azmeet] {name}: --relay-only needs a relay (--relay <url> or --relay default): \
             nothing can carry a packet"
        );
    }
    let endpoint = bind_endpoint(&relay, relay_only());
    // For scripts: how this side's packets may travel (the relay phase of azmeet_e2e.py).
    println!(
        "AZMEET_TRANSPORT {}",
        rooms::transport_label(&relay, relay_only())
    );
    let mut me = MeetState::new(&name, make_kit());
    let mut room = RoomSession::new(worker.clone(), relay.clone());
    room.server_ok = answer.is_ok();
    room.checked_at = wall_ms();
    room.server_status = match &answer {
        Ok(()) => String::from("The meeting server answers."),
        Err(_) if worker.is_empty() => no_server_notice(),
        Err(e) => format!(
            "The meeting server at {worker} ({from}) does not answer ({e}). AzMeet asks again \
             every {} seconds; Retry asks now, or type another one and press Enter.",
            SERVER_RETRY_MS / 1000
        ),
    };
    if worker.is_empty() {
        me.notice = no_server_notice();
    }
    if endpoint.is_bound() {
        room.node_id = endpoint.endpoint_id().as_str().to_string();
        eprintln!(
            "[azmeet] {name}: endpoint {} (relays {relay:?}{}), meeting server {} ({from})",
            short_id(&room.node_id),
            if relay_only() { ", relay only" } else { "" },
            if worker.is_empty() { "none" } else { worker.as_str() }
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
    // The rooms this device is in (their chats come back once its identity is loaded).
    me.index = files_root().map(store::load_index).unwrap_or_default();
    configure_audio(&mut me);
    configure_video(&mut me, &probe_video());
    configure_network(&mut me);
    apply_launch_args(&mut me);
    run(me);
}

/// The app with its one window.
fn run(me: MeetState) {
    let (theme, mode) = launch_look();
    let mut config = AppConfig::create()
        .with_theme(AzString::from(theme.name()))
        .with_mode(mode_option(mode.index()));
    // The kit's icons: Haiku's under flora, Material under flat.
    kit::add_kit_icons(&mut config);
    let app = App::create(RefAny::new(me), config);
    let mut window = WindowCreateOptions::create(layout);
    window.window_state.flags.decorations = WindowDecorations::NoTitle;
    window.create_callback = Some(Callback::create(startup)).into();
    // azul-appkit's `--size WxH`, else 1100 x 720.
    let (width, height) = launch_args().kit.size.unwrap_or((1100.0, 720.0));
    window.window_state.size.dimensions = LogicalSize::create(width, height);
    window.window_state.title = AzString::from("AzMeet");
    app.run(window);
}

#[cfg(target_os = "android")]
#[ctor::ctor]
fn azul_android_init() {
    start();
}
