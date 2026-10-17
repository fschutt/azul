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
//! the UI thread, so no callback waits on the network.
//!
//! Two settings say where AzMeet connects, both in Settings > Meetings and both changeable only
//! outside a meeting. The start screen shows neither; it only says when the meetings service
//! cannot be used ("Connection settings" opens them).
//! - The meetings service (the meet Worker): rooms, members, room keys, the schedule, the chat as
//!   ciphertext and the iroh tickets of a call. `--worker` (this run only), else the one saved
//!   last time, else the shared Azlin config's (azul-appkit `azlin_config`: `AZMEET_WORKER`, else
//!   `endpoints.meet` of the config file `AZLIN_CONFIG` names, else of `~/.azlin/config.json`),
//!   else one built in at build time, else the config profile's (`local`, the default: the local
//!   stack's `http://127.0.0.1:8790`; `production` names none yet) - and nothing else: with none,
//!   the settings ask for one. A new address is used for every request from Enter or leaving the
//!   field on, checked with `GET /health`, and saved once it answers.
//! - The iroh relay, which carries a connection only when two peers cannot reach each other
//!   directly: "Default" (n0's public relays), "Azlin" (`rooms::AZLIN_RELAY`) or "Custom" (a
//!   company's own, by its address). `--relay` (this run only), else the one picked in the
//!   settings, else the shared config's (`AZMEET_RELAY`, `endpoints.relay`; Azlin's relay for
//!   Azlin's trial and production profiles), else none for a meetings service on this computer
//!   and n0's for any other. "No relay" is not offered: `--relay off` is for tests. A new relay
//!   binds the iroh endpoint anew.
//!
//! The sound check (`soundcheck.rs`), in the waiting room and in Settings > Audio & Video: "Test
//! speakers" plays a short chime, "Record a test clip" records three seconds of the microphone
//! and plays them back, on the playout thread's own sink. stdout says `AZMEET_SOUND_CHECK
//! <speakers|recording|playing|ended <outcome>>`.
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
//! - `--worker <url>` (`AZMEET_WORKER`): the meetings service, e.g. `http://127.0.0.1:8790` (the
//!   local stack's `wrangler dev`). `--worker` wins over the one saved in the settings; the
//!   variable and the shared config only count when none was saved. Else the `PRODUCTION_WORKER`
//!   constant, set at build time with `AZMEET_DEFAULT_WORKER=<url>`, else the config profile's
//!   address, else none: the settings ask for one. A headless run (`AZ_BACKEND=headless`)
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
//! - `--relay <off|default|azlin|url>` (`AZMEET_RELAY`): this run's relay, over the one picked in
//!   the settings and the shared config file's `endpoints.relay` (the local stack names its
//!   `iroh-relay --dev` there; Azlin's trial and production profiles name Azlin's relay); with
//!   none of them, off for a meetings service on this machine and the public iroh relays
//!   otherwise. `off` is for tests: the settings do not offer it.
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
//!
//! The code by concern: this file holds the state, the window's snapshot, the link's events and
//! the start; the call's audio (`call_audio.rs`), video (`call_video.rs`) and forwarding
//! (`forwarding.rs`), the meetings service's requests (`service.rs`), Settings > Meetings'
//! connection (`connection.rs`), the data tree (`data_tree.rs`), this device's identity
//! (`device_identity.rs`), the room view (`room_view.rs`), the other controls (`controls.rs`),
//! direct calls (`direct_call.rs`) and chat over the peers (`peer_chat.rs`) are modules of their
//! own that read this one with `use super::*`.

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
mod direct;
mod direct_call;
mod identity;
mod ids;
/// A room's invite key from the invite secret of its link (CRYPTO.md section 4); public, since
/// AzCalendar's meeting links are made with it too (it includes the file).
pub mod invite;
mod keys;
mod l10n;
#[cfg(test)]
mod l10n_tests;
mod pace;
mod peer_chat;
mod rate;
mod roomlist;
mod rooms;
mod routes;
mod speaker;
mod soundcheck;
mod store;
mod tiles;
mod times;
mod ui;
mod video_wire;
// The app by concern (each reads the crate root with `use super::*`).
mod call_audio;
mod call_video;
mod connection;
mod controls;
mod data_tree;
mod device_identity;
mod forwarding;
mod room_view;
mod service;

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
use azul_appkit::{
    azlin_config,
    l10n::{t, t_args, t_text, Arg, Phrase, Text},
    ui as kit,
};
use rooms::{Dialed, PeerRecord, Relay, RoomKey};
use video_wire::{Codec, Control, Message};
use self::{call_audio::*, call_video::*, connection::*, controls::*, data_tree::*, device_identity::*, forwarding::*, room_view::*, service::*};

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

/// The meetings service when nothing else names one: the deployed `meet` Worker (azul-apps
/// `cf-workers/meet/README.md`, "Deploy"), baked in at build time with
/// `AZMEET_DEFAULT_WORKER=https://...`. Empty means none: the shared Azlin config's profile
/// names one, else the settings ask for one.
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
/// What a chat line says for a member whose name does not open (a key of AzMeet's resources).
const UNKNOWN_NAME: &str = "azmeet-someone";

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
    /// The meeting server has not answered the check of its address yet (at the start, or
    /// after a new address): the start screen says nothing about it meanwhile.
    server_pending: bool,
    /// Counts the checks of the meeting server; the answer to an older one is ignored.
    checks: u32,
    /// Where the iroh endpoint relays (`relay_for`): `--relay`, the settings' choice, the shared
    /// config, else by the meetings service's host.
    relay: Relay,
    /// Settings > Meetings: the custom relay field as typed; `relay_custom` while "Custom" is
    /// picked and no address of it is used yet (the field shows, nothing is rebound);
    /// `relay_status` why what was typed is no relay address.
    relay_text: String,
    relay_custom: bool,
    relay_status: String,
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
    /// The call is a direct one (`direct_call.rs`): no room on the meetings service, nothing
    /// announced, polled or left there.
    direct: bool,
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
            relay_text: match rooms::RelayChoice::of(&relay) {
                Some(rooms::RelayChoice::Custom) => match &relay {
                    Relay::Custom(url) => url.clone(),
                    _ => String::new(),
                },
                _ => String::new(),
            },
            relay_custom: false,
            relay_status: String::new(),
            server_status: String::new(),
            server_ok: false,
            server_pending: false,
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
            direct: false,
        }
    }

    /// Into a direct call: the call screen, with no room of the meetings service behind it.
    fn enter_direct(&mut self) {
        self.session = self.session.wrapping_add(1);
        self.stage = Stage::InRoom;
        self.direct = true;
        self.waiting = None;
        self.close_waiting();
        self.straight_in = false;
        self.room_id.clear();
        self.code.clear();
        self.link.clear();
        self.copied = false;
        self.peers.clear();
        self.dialed.clear();
        self.polls = 0;
        self.announced_at = None;
        self.busy = false;
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
        self.direct = false;
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
        if self.direct {
            return None;
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
    /// Direct calls: the screen, the call being made or coming in, the people in one, the recent
    /// calls (`direct.rs`, `direct_call.rs`).
    direct: direct::DirectState,
    /// What the peers' chat records changed, by room, until the pump shows it
    /// (`peer_chat::take_changes`).
    peer_chat: Vec<(String, chatroom::Changes)>,
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
    /// The sound check of the waiting room and the settings (`soundcheck.rs`): the speaker
    /// test's chime, the test clip being recorded or played back.
    sound_check: soundcheck::SoundCheck,
    /// The sound check's timer runs (`sound_check_tick`); it ends with the check, so an idle
    /// window has no timer of its own for it.
    sound_check_ticking: bool,
    /// The seconds the recording button showed last: the window changes once a second while
    /// recording, not with every tick.
    sound_check_shown: u64,
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
            direct: direct::DirectState::default(),
            peer_chat: Vec::new(),
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
            sound_check: soundcheck::SoundCheck::new(),
            sound_check_ticking: false,
            sound_check_shown: 0,
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
    azul_appkit::l10n::begin_layout(&info);
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
        .or_else(|| s.direct.name_of(node_id))
        .unwrap_or_else(|| short_id(node_id).to_string())
}

/// A member's name as the window shows it: the one its record carries, else [`UNKNOWN_NAME`].
fn member_name(name: &str) -> String {
    if name.trim().is_empty() {
        t(UNKNOWN_NAME)
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
        Some(Stage::Start | Stage::Opening) | None if s.direct.open => ui::UiScreen::Direct,
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
        status: t(if s.cam_on { "azmeet-camera-on" } else { "azmeet-camera-off" }),
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
            let status = t(if remote.is_some() {
                "azmeet-status-connected"
            } else if rooms::dials(&room.node_id, &p.node_id) {
                "azmeet-status-connecting"
            } else {
                "azmeet-status-waiting"
            });
            rows.push(person(p.name.clone(), &status, remote, p.device.as_deref()));
        }
        // Connected peers whose record expired from the server stay listed.
        for r in &s.remotes {
            if !room.peers.iter().any(|p| p.node_id == r.node_id) {
                rows.push(person(short_id(&r.node_id).to_string(), &t("azmeet-status-connected"), Some(r), None));
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
        section(&t("azmeet-stats-people"), roster_lines(s)),
        section(&t("azmeet-stats-microphones"), s.mics.clone()),
        section(&t("azmeet-stats-speakers"), s.speakers.clone()),
        section(&t("azmeet-stats-video"), video),
        section(&t("azmeet-stats-audio"), audio),
        section(&t("azmeet-stats-network"), network),
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

/// The cameras the settings offer: by facing (there is no camera list API); keys of AzMeet's
/// resources.
const CAMERAS: [&str; 3] = ["azmeet-camera-front", "azmeet-camera-back", "azmeet-camera-external"];

/// "System default", then `devices`.
fn device_choices(devices: &[String]) -> Vec<String> {
    let mut choices = vec![t("azmeet-system-default")];
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
        times::when_text(starts, ends, offset),
        times::meeting_status(starts, ends, now)
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
    let title = room_title(kind, &code);
    let times = match (entry.starts_at, entry.ends_at) {
        (Some(a), Some(b)) => Some(times_text(a, b)),
        _ => None,
    };
    let members = chat.map_or(0, |c| c.members().count());
    let mut detail = match chat.map(|c| c.state) {
        Some(chatroom::Membership::Knocking) => t("azmeet-row-knocking"),
        Some(chatroom::Membership::Closed) => t("azmeet-row-ended"),
        _ if members > 0 => t_args("azmeet-row-members", &[("count", Arg::from(members))]),
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

/// A room's title: "Chat room 4K7-29Q", "Meeting 4K7-29Q".
fn room_title(kind: chatroom::RoomKind, code: &str) -> String {
    let key = match kind {
        chatroom::RoomKind::Chat => "azmeet-chat-room-code",
        chatroom::RoomKind::Meeting => "azmeet-meeting-code",
    };
    t_args(key, &[("code", Arg::from(code))])
}

/// The room view of the open room.
fn room_page(s: &MeetState) -> Option<ui::RoomPage> {
    let id = s.open_room.as_ref()?;
    let chat = s.chats.get(id)?;
    let me = s.identity.as_ref()?;
    let status = t(match chat.state {
        chatroom::Membership::Member => "azmeet-room-member",
        chatroom::Membership::Knocking => "azmeet-room-knocking",
        chatroom::Membership::Joining => "azmeet-room-joining",
        chatroom::Membership::Leaving => "azmeet-room-leaving",
        chatroom::Membership::Left => "azmeet-room-left",
        chatroom::Membership::Closed => "azmeet-room-closed",
        chatroom::Membership::Outside => "azmeet-room-outside",
    });
    let key_line = match chat.current_key(me.device()) {
        Some((epoch, _, holders)) => t_args(
            "azmeet-room-key",
            &[("epoch", Arg::from(epoch.to_string())), ("count", Arg::from(holders))],
        ),
        None => t("azmeet-room-key-later"),
    };
    Some(ui::RoomPage {
        title: room_title(chat.kind, &chat.code),
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
        Some(code) if !code.is_empty() => {
            t_args("azmeet-title-meeting", &[("code", Arg::from(code)), ("name", Arg::from(&s.name))])
        }
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
        server_ok: room.server_ok,
        server_pending: room.server_pending,
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
                local: room.preview,
            }
        })
    });
    let call_chat = call_room_id(s).and_then(|id| s.chats.get(&id));
    let active_chat = active_room_id(s).and_then(|id| s.chats.get(&id));
    let knocks = call_chat.map(|c| knock_rows(s, c)).unwrap_or_default();
    let chat_note = active_chat.map_or_else(String::new, |c| {
        let mut note = t("azmeet-chat-encrypted");
        if c.before_join() > 0 {
            note.push_str(" · ");
            note.push_str(&t_args("azmeet-chat-sealed-before", &[("count", Arg::from(c.before_join()))]));
        }
        if c.unreadable() > 0 {
            note.push_str(" · ");
            note.push_str(&t_args("azmeet-chat-unreadable", &[("count", Arg::from(c.unreadable()))]));
        }
        note
    });
    let identity = match (&s.identity, &s.identity_source) {
        (Some(me), Some(source)) => format!("{} ({})", me.safety_code(), source.describe()),
        _ => t("azmeet-identity-loading"),
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
        sound_check: sound_check_state(s, now),
        sound_check_note: ui::sound_check_note(s.sound_check.outcome()),
        settings: ui::SettingsView {
            mics: device_choices(&s.mics),
            mic_choice: s.mic_choice,
            speakers: device_choices(&s.speakers),
            speaker_choice: s.speaker_choice,
            cameras: CAMERAS.iter().map(|c| t(c)).collect(),
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
                .unwrap_or_else(|| t("azmeet-server-none-set")),
            server_text: s.room.as_ref().map(|room| room.server_text.clone()).unwrap_or_default(),
            server_status: s.room.as_ref().map(|room| room.server_status.clone()).unwrap_or_default(),
            server_ok: s.room.as_ref().is_some_and(|room| room.server_ok),
            server_unset: s.room.as_ref().map_or(true, |room| room.worker.is_empty()),
            server_editable: s.room.as_ref().is_some_and(|room| room.stage == Stage::Start),
            relay_choice: s.room.as_ref().map_or(0, |room| {
                if room.relay_custom {
                    rooms::RelayChoice::Custom.index()
                } else {
                    rooms::RelayChoice::of(&room.relay).map_or(0, rooms::RelayChoice::index)
                }
            }),
            relay_off: s.room.as_ref().is_some_and(|room| room.relay == Relay::Off),
            relay_text: s.room.as_ref().map(|room| room.relay_text.clone()).unwrap_or_default(),
            relay_status: s.room.as_ref().map(|room| room.relay_status.clone()).unwrap_or_default(),
            relay_shown: s.room.as_ref().map_or_else(String::new, |room| relay_words(&room.relay)),
            name: s.name.clone(),
            codec: codec_status(s),
            recordings: files_root().map_or_else(
                || t("azmeet-recordings-nowhere"),
                |root| {
                    t_args(
                        "azmeet-recordings-folder",
                        &[("path", Arg::from(azul_appkit::data::local_path(root, store::APP_FOLDER).display().to_string()))],
                    )
                },
            ),
            identity,
        },
        direct: direct_call::view(s),
        incoming: direct_call::incoming_view(s),
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
    preview_retry: on_preview_retry,
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
    test_speakers: on_test_speakers,
    test_mic: on_test_mic,
    connection_settings: on_connection_settings,
    relay_choice: on_relay_choice,
    relay_text: on_relay_text,
    relay_key: on_relay_key,
    relay_blur: on_relay_blur,
    direct_open: direct_call::on_open,
    direct_back: direct_call::on_back,
    direct_form: direct_call::on_form,
    direct_copy: direct_call::on_copy,
    direct_text: direct_call::on_text,
    direct_call: direct_call::on_call,
    direct_recent: direct_call::on_recent,
    direct_cancel: direct_call::on_cancel,
    direct_accept: direct_call::on_accept,
    direct_decline: direct_call::on_decline,
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
            // For scripts: how another AzMeet calls this one directly (`direct_call.rs`).
            println!("AZMEET_DIRECT_ADDRESS {}", event.text.as_str());
            if let Some(room) = s.room.as_mut() {
                room.ticket = event.text.as_str().to_string();
            }
            s.link_status = String::from("waiting for a peer");
            true
        }
        IrohEventKind::PeerConnected => {
            let node_id = event.text.as_str().to_string();
            // A direct call's connection (made, expected, or someone calling).
            if let Some(changed) = direct_call::on_connected(s, event.peer, &node_id) {
                return changed;
            }
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
            let direct_changed = direct_call::on_disconnected(s, event.peer);
            let Some(pos) = s.remotes.iter().position(|r| r.handle == event.peer) else {
                return direct_changed;
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
            // The last other person of a direct call left: the call is over.
            direct_call::end_if_alone(s);
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
        // This side's chat messages to the peers of the call.
        peer_chat::flush(&mut s, &endpoint);
    }
    // What the peers' chat records changed: the lines, the files, the window.
    let chat_changes = data
        .downcast_mut::<MeetState>()
        .map(|mut s| peer_chat::take_changes(&mut s))
        .unwrap_or_default();
    for (room, changes) in chat_changes {
        let app = data.clone();
        let applied = data
            .downcast_mut::<MeetState>()
            .map(|mut s| apply_chat_changes(&mut s, &mut info.callback_info, &app, &room, changes));
        if let Some((update, follow_up)) = applied {
            refresh |= matches!(update, Update::RefreshDom);
            if let Some(job) = follow_up {
                spawn_http(&mut info.callback_info, app, job);
            }
        }
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
        // The chat that arrived, the people met: into the meeting's folder. A direct call taken
        // on the way is a recent call in the settings.
        flush_files(&mut s, &mut info.callback_info);
        save_prefs(&mut s, &mut info.callback_info);
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
        s.sound_check.feed(rate, 1, &samples);
        meter_change(s, &samples)
    });
    if let Some((bar, level)) = meter {
        show_level(info.callback_info, bar, level);
    }
}

// ==== The sound check: the speaker test and the test clip (the rules are in soundcheck.rs) ====

/// How often the sound check looks at the clock while it runs: the recording's countdown and the
/// end of what plays need no finer step.
const SOUND_CHECK_TICK_MS: u64 = 100;

/// The sound check as the window shows it at `now`.
fn sound_check_state(s: &MeetState, now: u64) -> ui::SoundCheckState {
    match s.sound_check.phase() {
        soundcheck::Phase::Idle => ui::SoundCheckState::Idle,
        soundcheck::Phase::Recording { .. } => ui::SoundCheckState::Recording {
            seconds_left: s.sound_check.seconds_left(now),
        },
        soundcheck::Phase::Playing { clip: true, .. } => ui::SoundCheckState::PlayingClip,
        soundcheck::Phase::Playing { clip: false, .. } => ui::SoundCheckState::PlayingChime,
    }
}

/// Hands the sound check's packets to the playout thread, in place of any it still plays.
fn play_check(s: &mut MeetState, rate: u32, packets: Vec<Vec<i16>>) {
    let play = s.play_audio;
    let shared = s.playout.get_or_insert_with(|| start_playout(play));
    let mut playout = lock(shared);
    playout.check = packets.into();
    playout.check_rate = rate;
}

/// Drops what the sound check still had to play.
fn stop_check_playback(s: &MeetState) {
    if let Some(shared) = s.playout.as_ref() {
        lock(shared).check.clear();
    }
}

/// Starts the sound check's timer unless it runs already. Called after the state's borrow is
/// given back: the timer holds a clone of `data`.
fn start_sound_check_timer(data: &RefAny, info: &mut CallbackInfo, needed: bool) {
    if !needed {
        return;
    }
    let get_time = info.get_system_time_fn();
    info.add_timer(
        TimerId::unique(),
        Timer::create(data.clone(), sound_check_tick, get_time)
            .with_interval(Duration::System(SystemTimeDiff::from_millis(SOUND_CHECK_TICK_MS))),
    );
}

/// Marks the sound check's timer as running; true when it has to be started.
fn claim_sound_check_timer(s: &mut MeetState) -> bool {
    !std::mem::replace(&mut s.sound_check_ticking, true)
}

/// "Test speakers": the chime plays through the speaker, in place of any check that runs.
extern "C" fn on_test_speakers(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let needed = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let now = now_ms(s);
        let packets = s.sound_check.start_chime(MIC_RATE, now);
        play_check(s, MIC_RATE, packets);
        sync_mic(s);
        println!("AZMEET_SOUND_CHECK speakers");
        claim_sound_check_timer(s)
    };
    start_sound_check_timer(&data, &mut info, needed);
    Update::RefreshDom
}

/// "Record a test clip": the microphone is recorded for a few seconds (opened for it while it is
/// muted) and then played back. A click while it records or plays the clip back stops it.
extern "C" fn on_test_mic(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let needed = {
        let Some(mut guard) = data.downcast_mut::<MeetState>() else {
            return Update::DoNothing;
        };
        let s = &mut *guard;
        let now = now_ms(s);
        let busy = matches!(
            s.sound_check.phase(),
            soundcheck::Phase::Recording { .. } | soundcheck::Phase::Playing { clip: true, .. }
        );
        if busy {
            s.sound_check.cancel();
            stop_check_playback(s);
            sync_mic(s);
            return Update::RefreshDom;
        }
        s.sound_check.start_recording(now);
        s.sound_check_shown = s.sound_check.seconds_left(now);
        // A chime still playing stops: the clip must not record it.
        stop_check_playback(s);
        sync_mic(s);
        println!("AZMEET_SOUND_CHECK recording");
        claim_sound_check_timer(s)
    };
    start_sound_check_timer(&data, &mut info, needed);
    Update::RefreshDom
}

/// Every [`SOUND_CHECK_TICK_MS`] while the sound check runs: the tone microphone's samples (a
/// headless run's), the countdown, the recorded clip handed to the playout, the end. The timer
/// ends with the check.
extern "C" fn sound_check_tick(mut data: RefAny, mut info: TimerCallbackInfo) -> TimerCallbackReturn {
    let tone = data
        .downcast_ref::<MeetState>()
        .is_some_and(|s| s.tone_mic && s.sound_check.records());
    if tone {
        pump_tone(&mut data, &mut info);
    }
    let Some(mut guard) = data.downcast_mut::<MeetState>() else {
        return TimerCallbackReturn::terminate_unchanged();
    };
    let s = &mut *guard;
    let now = now_ms(s);
    let changed = match s.sound_check.tick(now) {
        soundcheck::Tick::Nothing => {
            let left = s.sound_check.seconds_left(now);
            left != std::mem::replace(&mut s.sound_check_shown, left)
        }
        soundcheck::Tick::Play(rate, packets) => {
            play_check(s, rate, packets);
            sync_mic(s);
            println!("AZMEET_SOUND_CHECK playing");
            true
        }
        soundcheck::Tick::Ended => {
            sync_mic(s);
            println!("AZMEET_SOUND_CHECK ended {:?}", s.sound_check.outcome());
            true
        }
    };
    if s.sound_check.phase() == soundcheck::Phase::Idle {
        s.sound_check_ticking = false;
        return if changed {
            TimerCallbackReturn::terminate_and_refresh_dom()
        } else {
            TimerCallbackReturn::terminate_unchanged()
        };
    }
    if changed {
        TimerCallbackReturn::continue_and_refresh_dom()
    } else {
        TimerCallbackReturn::continue_unchanged()
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
/// `AZMEET_NAME`, else the account's full name (what the system's login window shows), else
/// the login name.
fn display_name() -> String {
    if let Some(name) = launch_args().name.clone().filter(|n| !n.trim().is_empty()) {
        return name.trim().to_string();
    }
    if let Some(name) = store::Prefs::read(saved_settings()).name {
        return name;
    }
    let set = |key: &str| {
        std::env::var(key)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
    };
    set("AZMEET_NAME")
        .or_else(account_full_name)
        .or_else(|| set("USER"))
        .or_else(|| set("USERNAME"))
        .unwrap_or_else(|| t("azmeet-guest"))
}

/// The account's full name: on macOS what `id -F` says, elsewhere the first field of the
/// account's GECOS entry (`getent passwd`); `None` when the system keeps none.
fn account_full_name() -> Option<String> {
    let output = if cfg!(target_os = "macos") {
        std::process::Command::new("id").arg("-F").output().ok()?
    } else if cfg!(unix) {
        let user = std::env::var("USER").ok()?;
        std::process::Command::new("getent").args(["passwd", &user]).output().ok()?
    } else {
        return None;
    };
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    if cfg!(target_os = "macos") {
        full_name_of(text.trim())
    } else {
        full_name_of(text.trim().split(':').nth(4)?)
    }
}

/// A full name as the account keeps it: the GECOS field's first part (`Felix Schütt,Room 1`),
/// trimmed; `None` when empty.
fn full_name_of(gecos: &str) -> Option<String> {
    let name = gecos.split(',').next()?.trim();
    (!name.is_empty()).then(|| name.to_string())
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

/// The meetings service at start and where it came from (for the log and the settings), asked
/// nothing yet: `--worker` (this run only), else the one saved last time
/// (the settings' field), else the shared Azlin config's (`AZMEET_WORKER`, else its file's
/// `endpoints.meet`), else one built in at build time (`AZMEET_DEFAULT_WORKER`), else the config
/// profile's (`local`: the local stack's `http://127.0.0.1:8790`), else none - the settings ask
/// for one. A headless run without a data root of its own reads no settings, so nothing
/// saved outranks the configuration there.
fn meeting_server() -> (String, String) {
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
        rooms::ServerSource::Saved => t("azmeet-server-saved"),
        rooms::ServerSource::Environment => shared.map(|(_, label)| label).unwrap_or_default(),
        rooms::ServerSource::BuiltIn => t("azmeet-server-built-in"),
        rooms::ServerSource::Unset => t("azmeet-server-none-set"),
    };
    (url, from)
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
    s.direct.recents = direct::recents_from_text(prefs.direct_calls.as_deref().unwrap_or_default());
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
    // The words first: the meeting server's first check is said before the window or its kit
    // exists, in the language `--language` or the settings file chose (the system's otherwise).
    let language = launch_args()
        .kit
        .language
        .unwrap_or_else(|| saved_settings().language());
    crate::l10n::speak(language);
    let (worker, from) = meeting_server();
    start_rooms(worker, &from);
}

/// The relays for a meetings service at `worker`: `--relay` (this run only), else `saved` (the
/// one picked in the settings), else the shared Azlin config's (`AZMEET_RELAY`, its file's
/// `endpoints.relay`, Azlin's relay for Azlin's profiles), else none for a meetings service on
/// this machine and the public iroh relays for any other (`rooms::relay_setting_of`,
/// `rooms::relay_choice`).
fn relay_for(worker: &str, saved: Option<&str>) -> Relay {
    let host = server_address(worker)
        .map(|(host, _)| host)
        .unwrap_or_default();
    // A relay the environment or the config file names, or the profile's unless that is n0's
    // default (Azlin's profiles name Azlin's relay): `rooms::shared_relay`.
    let resolved = azlin_endpoints().get(azlin_config::Endpoint::Relay);
    let shared = match resolved.source {
        azlin_config::Source::Env(_) | azlin_config::Source::File(_) => {
            rooms::shared_relay(resolved.value.as_deref(), false)
        }
        azlin_config::Source::Profile(_) => rooms::shared_relay(resolved.value.as_deref(), true),
        _ => None,
    };
    let setting = rooms::relay_setting_of(launch_args().switch("AZMEET_RELAY"), saved, shared);
    rooms::relay_choice(setting, &host)
}

/// The room session the window starts with, for the meetings service at `worker` (`from`:
/// where its address came from) and the relays `relay`.
fn room_at_start(worker: String, relay: Relay, from: &str) -> RoomSession {
    let mut room = RoomSession::new(worker, relay);
    // Never checked: the first room tick (and the window's start) asks at once.
    room.checked_at = 0;
    room.server_pending = !room.worker.is_empty();
    room.server_status = if room.worker.is_empty() {
        no_server_notice()
    } else {
        t_args(
            "azmeet-asking-server-from",
            &[("server", Arg::from(room.worker.as_str())), ("from", Arg::from(from))],
        )
    };
    room
}

/// The window with the start screen, talking to the meeting server at `worker` (none when
/// empty; `from`: where its address came from). The server is asked once the window is up
/// (`startup`); an unreachable or missing one is said on the start screen and, with a Retry
/// button, under the settings' field: nothing else opens instead.
fn start_rooms(worker: String, from: &str) {
    let relay = relay_for(&worker, store::Prefs::read(saved_settings()).relay.as_deref());
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
    let mut room = room_at_start(worker.clone(), relay.clone(), from);
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
        me.notice = t_args("azmeet-no-network", &[("why", Arg::from(reason.as_str()))]);
        me.link_status = reason;
    }
    me.room = Some(room);
    // The rooms this device is in (their chats come back once its identity is loaded).
    me.index = files_root().map(store::load_index).unwrap_or_default();
    configure_audio(&mut me);
    configure_video(&mut me, &probe_video());
    configure_network(&mut me);
    apply_launch_args(&mut me);
    open_devices_at_start(&mut me);
    run(me);
}

/// The app with its one window.
fn run(me: MeetState) {
    let (theme, mode) = launch_look();
    let mut config = AppConfig::create()
        .with_theme(AzString::from(theme.name()))
        .with_mode(mode_option(mode.index()));
    crate::l10n::register(&mut config);
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

#[cfg(test)]
mod header_tests;
#[cfg(test)]
mod startup_tests;
#[cfg(test)]
mod guest_tests;

#[cfg(test)]
mod service_down_tests {
    use super::*;

    /// The start screen with no meetings service set.
    fn lobby_without_a_service() -> MeetState {
        let mut s = MeetState::new("Ada", make_kit());
        s.room = Some(RoomSession::new(String::new(), rooms::Relay::Default));
        s
    }

    #[test]
    fn without_a_meetings_service_a_new_meeting_opens_its_waiting_room_with_the_camera_on() {
        let mut s = lobby_without_a_service();
        let _ = open_local_meeting(&mut s, t("azmeet-preview-no-server"));
        let room = s.room.as_ref().expect("a room");
        assert_eq!(room.stage, Stage::Waiting, "the waiting room opens");
        let view = snapshot(&s);
        assert!(view.cam, "its camera preview is on");
        let waiting = view.waiting.expect("the waiting room's facts");
        assert!(waiting.local, "it says the meeting is made here, with Retry");
        assert!(waiting.code.contains('-'), "a code made here: {}", waiting.code);
    }

    #[test]
    fn the_name_is_the_accounts_full_name_without_its_extra_fields() {
        assert_eq!(full_name_of("Felix Schütt").as_deref(), Some("Felix Schütt"));
        assert_eq!(full_name_of(" Felix Schütt ,Room 1,,").as_deref(), Some("Felix Schütt"));
        assert_eq!(full_name_of(""), None);
        assert_eq!(full_name_of(",,,"), None);
    }

    #[test]
    fn the_start_screen_shows_the_camera_at_once_even_with_no_meetings_service() {
        let mut s = lobby_without_a_service();
        open_devices_at_start(&mut s);
        let view = snapshot(&s);
        assert!(matches!(view.screen, ui::UiScreen::Lobby), "the start screen");
        assert!(view.cam, "its camera preview is on from the start");
        assert!(view.mic, "and the microphone's level shows");
    }
}
