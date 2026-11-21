---
slug: system/realtime-media
title: Realtime Media
language: en
canonical_slug: system/realtime-media
audience: external
maturity: beta
guide_order: 124
topic_only: false
short_desc: Camera/mic capture, audio playback, and media between apps over iroh (AzMeet - rooms, audio, leave, forwarding and renditions for rooms of three and more)
prerequisites: [events/callbacks, data/background-tasks]
tracked_files:
  - layout/src/widgets/capture_common.rs
  - layout/src/widgets/microphone.rs
  - core/src/audio.rs
  - core/src/video.rs
  - dll/src/desktop/extra/audio/mod.rs
  - dll/src/desktop/extra/iroh/engine.rs
  - dll/src/desktop/extra/iroh/loadbalancer.rs
  - examples/azul-meet/src/lib.rs
  - examples/azul-meet/src/audio.rs
  - examples/azul-meet/src/rooms.rs
  - examples/azul-meet/src/routes.rs
  - examples/azul-meet/src/video_wire.rs
  - examples/azul-meet/scripts/meet-e2e.mjs
  - examples/azul-meet/scripts/two-clients.mjs
  - examples/azul-meet/scripts/three-clients.mjs
last_generated_rev: 754b7f00e088960c14db598f64fa200dacc28bf1
generated_at: 2026-05-21T00:00:00Z
default-search-keys:
  - MicrophoneWidget
  - AudioSink
  - OnAudioFrameCallback
  - AudioFrame
  - VideoFrame
  - CameraWidget
  - VideoEncoder
  - VideoDecoder
  - backend_name
  - IrohEndpoint
  - broadcast_frame
  - send_message
  - AzMeet
  - jitter buffer
  - IrohLoadBalancer
  - IrohTileRole
  - simulcast
  - backbone
---

# Realtime Media

## Introduction

Azul exposes camera / screen / microphone **capture** and audio **playback** as
ordinary widgets and handles - no globals, no manager singletons. Each capture
source is a "dumb widget" that owns a background worker and hands you each frame
through a callback hook; playback is a handle you keep in your own application
`State`. Between two apps the bytes travel over `IrohEndpoint` (see
[networking](../data/networking.md#peer-to-peer-connections)): media frames on
numbered tracks, control data as messages.

```text
capture -> hook -> encode -> IrohEndpoint -> decode -> jitter buffer -> playback
```

The `examples/azul-meet` app (AzMeet) puts it all together: meetings joined by a
link, video and audio between every participant, mute state, and leaving. See
[AzMeet](#azmeet-meetings-audio-leaving) below.

> **Capture and playback do not know about the network.** A hook hands you
> frames and `AudioSink` plays frames you hand it, so any transport works.
> `IrohEndpoint` is the one azul ships: direct QUIC between native apps, relayed
> when no direct path exists. A browser participant needs a relay bridge or
> WebRTC, which AzMeet does not have yet.

The architecture follows the framework's backreference dependency-injection
pattern (see [architecture](../architecture.md)): a widget takes a `RefAny` (a
reference to your data) plus a callback, and invokes the callback with the
captured frame so you can store, process, or send it. You never reach into a
global; the data flows back to *your* state.

## Capturing video frames (camera / screen / video)

The `CameraWidget`, `ScreenCaptureWidget`, and video-playback widget share one
hook: `set_on_frame` / `with_on_frame`, invoked once per decoded frame with a
[`VideoFrame`] (`{ width, height, bytes }`, RGBA). Mount the widget anywhere in
your DOM; the capture lives as long as the node is mounted.

```rust
let camera = CameraWidget::create(CameraConfig::default())
    .with_on_frame(state.clone(), on_video_frame)
    .dom();

extern "C" fn on_video_frame(mut data: RefAny, _info: CallbackInfo, frame: VideoFrame) -> Update {
    if let Some(mut s) = data.downcast_mut::<MyState>() {
        // frame.bytes is RGBA, frame.width x frame.height. Save it, run it
        // through an effect, or encode + send it (see "Streaming frames to a peer").
        s.last_frame_bytes = frame.bytes.len();
    }
    Update::RefreshDom
}
```

The widget renders a GPU-texture preview itself; your hook is purely a data tap.

### One capture, many sizes (consumers)

The preview tile, a remote participant and a recorder rarely want the same
size. Register each as a **consumer**: the device is opened ONCE at the
smallest size that covers everyone (the tile's own device-pixel size, which
the widget learns from layout, plus every consumer), and every captured frame
is cut to each consumer's size off the main thread — so the camera never
captures more than the largest consumer can use, and nothing is sent bigger
than it was asked for.

```rust
// "Client Bob wants 500x200; the local preview is 100x200": the camera is
// opened at 500x200 and each frame is sampled twice, from one capture.
let camera = CameraWidget::create(CameraConfig::default())
    .with_consumer(FrameConsumer::create(BOB, 500, 200))
    .with_on_consumer_frame(state.clone(), on_consumer_frame)
    .dom();

extern "C" fn on_consumer_frame(mut data: RefAny, _info: CallbackInfo, cut: ConsumerFrame) -> Update {
    // cut.consumer.id tells you who this is for; cut.frame is RGBA at
    // cut.consumer.width x cut.consumer.height.
    if cut.consumer.id == BOB {
        send_to_bob(&cut.frame);
    }
    Update::DoNothing
}
```

`on_frame` still receives the frame as captured (at the configured size or
larger) when you set it; with consumers alone the full-size frame never
leaves the capture thread. The same applies to `ScreenCaptureWidget`, whose
request also honours `config.source` (display index / window id),
`config.fps`, and leaves your own windows out of a shared desktop so a share
that shows the meeting window does not loop.

On macOS the per-consumer cut runs through Accelerate/vImage; elsewhere the
portable scaler (`image_scale`) does the same work. Both are pure functions of
the frame, so the cuts can run in parallel later without an API change.

## Capturing audio (microphone)

`MicrophoneWidget` is the audio twin of the capture widgets - same shape, no GL.
It mounts an invisible node, starts a capture thread on mount, and calls your
`on_frame` hook with each [`AudioFrame`] (`{ sample_rate, channels, samples }`,
interleaved `f32`).

```rust
let mic = MicrophoneWidget::create(AudioConfig { sample_rate: 48_000, channels: 1 })
    .with_on_frame(state.clone(), on_audio_frame)
    .dom();

extern "C" fn on_audio_frame(mut data: RefAny, _info: CallbackInfo, frame: AudioFrame) -> Update {
    if let Some(mut s) = data.downcast_mut::<MyState>() {
        s.captured += frame.frame_count();
    }
    Update::RefreshDom
}
```

Do not assume a chunk length: the test tone delivers about 20 ms, a platform
backend whatever its device hands it. If your wire format wants fixed packets,
re-cut the chunks (AzMeet's `Packetizer` does, see below). Capture stops when the
node unmounts, so muting is simply not rendering the widget.

## Playing audio (`AudioSink`)

Playback is a handle, not a widget - you usually play audio you *received*, not
audio bound to a node. `AudioSink` follows the same C-ABI handle convention as
`Db` / `Pdf`: open it, keep it, feed it frames, drop it to stop.

```rust
let sink = AudioSink::open(AudioConfig { sample_rate: 48_000, channels: 1 });
// ... later, for each frame you want to hear:
sink.play(frame);            // queues the samples to the output
// sink.is_open(), sink.error_message(), sink.frames_played(), sink.close()
```

The sink is open only if an output device opened. With no device, no audio
backend in this build, or a device that refuses the format, `open` returns a
closed handle: `is_open()` is `false`, `play` does nothing and
`error_message()` says why, in words you can show the user. `frames_played`
counts only the frames a device took, so a frame dropped because the device
queue was full is not counted.

`play` queues the samples, and how it waits depends on the backend: an ALSA write
blocks while the device buffer is full, AVAudioEngine keeps at most 8 buffers in
flight (about 160 ms of 20 ms frames) and drops beyond that, cpal holds up to 4
seconds. So feed a stream from a thread of its own, one packet per packet length,
not from a UI callback; and open one sink per stream at that stream's rate - the
system mixes several open sinks.

## Sending media between apps

`IrohEndpoint` has two ways to send bytes to a connected peer, and media needs
both:

- **Frames** (`send_frame`, `broadcast_frame`) travel on numbered tracks, each
  frame on its own QUIC stream. They are *latest-wins* at both ends: a frame that
  has not left yet is replaced by the next frame of its track, and the receiver
  keeps only the newest frame of each track until `recv` takes it. For video whose
  every frame stands alone (JPEG) that is right: a late frame is worthless. For
  audio, where every packet counts, repeat the last few packets in every frame, so
  a replaced frame loses nothing.
- **Messages** (`send_message`) are reliable and ordered: control data such as
  "my microphone is off", and video from an inter-frame codec (H.264). A P-frame
  needs every packet since the last keyframe, and frames cannot promise that: one
  may be replaced before it leaves, and since each frame is its own stream a large
  keyframe loses the race to the small P-frame after it and is then dropped on
  arrival as the older one.

```rust,ignore
// A capture hook: encode the frame and send it to everyone on its track.
extern "C" fn on_consumer_frame(mut data: RefAny, _info: CallbackInfo, cut: ConsumerFrame) -> Update {
    if let ResultU8VecEncodeImageError::Ok(jpeg) = rgba_image(&cut.frame).encode_jpeg(75) {
        endpoint_of(&mut data).broadcast_frame(cut.consumer.id, jpeg);
    }
    Update::DoNothing
}

// A timer: everything that arrived since the last tick.
while let Some(event) = endpoint.recv().into_option() {
    match event.kind {
        IrohEventKind::Frame if event.track == AUDIO_TRACK => buffer_audio(event.peer, event.data),
        IrohEventKind::Frame => show_tile(event.peer, event.track, event.data),
        IrohEventKind::Message => apply_control(event.peer, event.data),
        _ => {}
    }
}
```

Nothing arrives unless you poll `recv`; AzMeet does it every 15 ms.

## AzMeet: meetings, audio, leaving

`examples/azul-meet` is a meeting app on the public API. Started with a meeting
server it shows **New meeting**, **New chat room**, **Schedule** (a meeting with a
start and a length), **Join with a link or a code** and **Your rooms** (with their
unread counts); a room opens in its room view - its chat, its members with their
safety codes, **Join call**, **Leave room**. In a call it shows the invite link, the
people, a tile per camera or screen, and the buttons **Mute**, **Deafen**, **Start
video**, **Share screen**, **Speaker view** / **Grid view** and **Leave**. Every
room is end-to-end encrypted: the meeting server keeps nothing it can read (see
*End-to-end encryption* below).

### Rooms

The meeting server is the `meet` Cloudflare Worker (azul-apps `cf-workers/meet`,
with a local mock, `dev-server.mjs`). It never sees any media; it only lets the
apps find each other:

| Request | What AzMeet uses it for |
|---|---|
| `POST /rooms` with `{room, invite_key, kind, starts_at, ends_at}` | A new room: the id and the invite secret are made in the app, the server keeps the invite key (public) and gives a short code |
| `GET /rooms/<id or code>?format=json` | Joining: resolves a pasted link or code |
| `PUT /rooms/<id>/members/<device>`, `POST .../members/<device>/admit`, `DELETE .../members/<device>` | This device's signed member record (with the link's proof, or a knock), letting a knock in, leaving the room |
| `POST /rooms/<id>/keys`, `POST /rooms/<id>/messages` | A room key sealed to each member; a sealed message |
| `GET /rooms/<id>/sync?for=<device>&after=<seq>&keys_after=<seq>` | The members, the key copies sealed to this device and the messages since the last read: every 2 s for the room on screen, every 15 s for the others |
| `POST /rooms/<id>/peers` with `{node_id, ticket, device, sig}` | Announces this app's iroh ticket, signed by its device, every sixth of the server's peer TTL |
| `GET /rooms/<id>/peers?except=<node_id>` | Reads everyone else's ticket, every 2 s; only a member's signed one is dialled or taken |
| `DELETE /rooms/<id>/peers/<node_id>` | Leaving the call: off the list at once |

Of each pair of participants the one with the lower endpoint id dials, so two
peers that find each other in the same poll open one connection. Every request
runs on an azul `Thread` and resumes on the UI thread, so no callback waits on the
network. A participant that stops announcing drops off after 120 s, a room after
a day without announcements.

The start screen has a **Meeting server** field, prefilled with `--worker`, else
the address saved last time, else the shared Azlin config's (azul-appkit
`azlin_config`: `AZMEET_WORKER`, else `endpoints.meet` of the file `AZLIN_CONFIG`
names or of `~/.azlin/config.json`), else one built in at build time
(`AZMEET_DEFAULT_WORKER=<url>`), else the config profile's (`local`, the default:
the local stack's `http://127.0.0.1:8790`), else none - the field asks for one.
Pressing Enter or leaving the field makes its address the meeting server for every
request from then on, checks it with `GET /health`, and saves it (`meet/settings.json`
in the Azlin data tree) once it answers. The line under the field says whether it
answers, and where the address came from; a server that does not answer is asked
again every 10 seconds, and **Retry** asks now. A headless run (`AZ_BACKEND=headless`)
without a data root of its own neither reads nor writes the saved address, so the
configuration always wins in tests. The relays come the same way: `--relay`,
`AZMEET_RELAY`, `endpoints.relay`, the profile's (`local`: `http://127.0.0.1:3340`,
`production`: n0's).

### End-to-end encryption

The design is `examples/azul-meet/CRYPTO.md`; the meeting server and the relay are
not trusted with anything readable.

- **The device.** Every device has an Ed25519 key (it signs) and an X25519 key (keys
  are sealed to it), both from one seed in the OS keyring (`--identity-file` keeps it
  in a file instead, mode 0600). Its **safety code** - 20 digits from both public
  keys - is what two people compare; the room view lists every member's, with
  **Mark verified**.
- **The link.** A room's id and its **invite secret** are made in the app; the secret
  travels only in the link's fragment (`azlin://meet/<room>#<secret>`). The server
  keeps the invite key derived from it (public), and a member record carries the
  key's proof. Without the secret - with only the code - a device **knocks**, and a
  member lets it in with **Admit**; its first room key brings it the secret.
  Members' names are sealed with a key from the secret.
- **Room keys.** A message is sealed (XChaCha20-Poly1305, padded to 128 bytes) with
  the room's newest key sealed to exactly the members listed now, and signed. When
  the members change (someone leaves, someone is let in) the next message is under a
  new key, sealed to those who are there: one who left reads nothing after.
- **Calls.** An iroh ticket is announced signed by its member's device; a connection
  from an endpoint no member announced is held until a read of the list confirms it,
  and dropped otherwise.
- **What the server holds:** room ids, codes, times, public keys, signatures, the
  sealed names, the sealed key copies and the sealed messages, their sizes in steps
  and their times - no message, no name (but a knock's), no secret.

stdout, for scripts: `AZMEET_IDENTITY <device> <keyring|file|session>`,
`AZMEET_SAFETY <code>`, `AZMEET_KEY <room> epoch=<n> key=<id> members=<n> by=<who>`,
`AZMEET_MEMBER <room> <joined|left|knocking> <name> <code>`, `AZMEET_HISTORY <room>
<n>`, `AZMEET_KNOCK`, `AZMEET_ADMITTED`, `AZMEET_VERIFIED`, `AZMEET_LEFT_ROOM`,
`AZMEET_TIMES <start> <end>`, `AZMEET_CHAT <name>: <text>`.

### Video

Each camera or screen frame is sent on track 1 (camera) or 2 (screen), in the
renditions the viewers' tiles ask for (see *Rooms of three and more*: 90, 180, 360
or 720 lines, 16:9, at most two per track); each peer's pictures land in that
peer's own tiles. The rules live in
`examples/azul-meet/src/video_wire.rs` (plain Rust with unit tests).

1. **Codec.** At start AzMeet opens a `VideoEncoder`, encodes a test frame and
   decodes the result with a `VideoDecoder`. An open handle proves nothing (where
   no backend is built in, `open` hands out a handle that never yields a packet),
   so only a keyframe that comes back as a picture counts. Where it does
   (VideoToolbox on macOS and iOS), frames go out as H.264; elsewhere as JPEG. The
   devices panel says which: `Video: H.264 (VideoToolbox)`, `Video: JPEG (no
   encoder)`. On connecting, each side sends `[5][flags]` (bit 0: decodes H.264,
   bit 1: encodes it), and a peer that cannot decode H.264 gets JPEG from a sender
   that encodes it:
   `Video: H.264 (VideoToolbox); JPEG to Ben (no H.264 decoder)`.
2. **Packets.** Every packet carries a 20-byte header:

   ```text
   [2][version 2][codec: 1 JPEG, 2 H.264][flags: bit 0 keyframe]
   [track u32][seq u32][frame_no u32][height u16][0 u16]
   then a JPEG file or H.264 Annex B
   ```

   `height` names the rendition: each rendition of a track is a stream of its own,
   with its own encoder, numbers and keyframes. `seq` counts one codec's packets on
   one rendition, so a gap is a missing packet; `frame_no` counts the frames
   captured and may jump. H.264 packets are sent as messages; JPEG ones as
   latest-wins frames, each rendition on its own frame track.
3. **Loss.** A receiver decodes in order. After a gap in `seq` it decodes nothing
   until the next keyframe (an IDR slice) and sends `[3][track][height]`, a
   keyframe request, again after a second while it still waits. The sender forces a
   keyframe on a request (several requests within half a second share one), for
   every new peer, and every 3 seconds anyway. An encoder that answers a forced
   keyframe with a P-frame is closed and opened again: a new encoder starts with a
   keyframe.
4. **A slow link.** Messages are never dropped, so a link slower than the video
   would queue without end. The receiver acknowledges H.264 packets
   (`[4][track][seq][height]`, every fifth and every keyframe); a sender more than 24 packets ahead of a peer
   pauses that peer and resumes it at a keyframe once it caught up.
5. **Decoding.** Each peer's tracks have their own `VideoDecoder`. One that is
   given 30 packets from a keyframe on and returns no picture does not work here:
   AzMeet tells everyone it decodes no H.264, and gets JPEG.

The devices panel shows both directions, per rendition: `Sending camera 360p: 450
H.264 packets, 0 JPEG frames, 8 keyframes, 2 on request, 5 periodic, 0 reopens, 0
dropped on purpose` and `Video from Ben (camera 360p): H.264, decoded 300,
keyframes 12, gaps 1, dropped 3, keyframe requests 1` (`camera 90p via Ben` when a
forwarder passes it on).

### Audio

The audio path lives in `examples/azul-meet/src/audio.rs` (plain Rust with unit
tests) and `lib.rs`:

1. **Packets.** The microphone's chunks are mixed down to mono, converted to
   16-bit PCM and cut into 20 ms packets at the microphone's rate (960 samples at
   48 kHz), each with the next sequence number.
2. **Frames.** Every new packet is sent in a frame together with the two before
   it, on track 3, so a replaced or overwritten frame loses nothing while the next
   one arrives:

   ```text
   [version 1][codec 1 = PCM s16][count][0][sample rate u32]
   count x ([sequence u32][length u16][length x i16])     little endian, oldest first
   ```

   Raw PCM at 48 kHz is 768 kbit/s, about 2.3 Mbit/s with the repeats: fine on a
   local network, heavy on a phone. Opus (codec 2) is the next step.
3. **A jitter buffer per peer.** It keeps packets in sequence order, drops copies
   and packets whose turn has passed, and starts to play once it holds three
   (60 ms). A missing packet plays as silence: when later packets are already
   there the gap is skipped, otherwise its turn waits. After 200 ms without
   packets (the peer muted) it fills up again before playing; past 200 ms of
   backlog it drops the oldest.
4. **A playout thread.** Every 20 ms a thread of its own takes one turn from every
   peer's buffer and plays it through that peer's `AudioSink`, opened at that
   peer's rate. The UI thread only pushes packets into the buffers, under a lock
   it holds for microseconds, and never waits on a device.

The devices panel shows what is sent and what arrived, per peer:
`Audio from Ben: 250 packets, 247 played, 3 silent, 0 late, 3 buffered`.

### Mute, deafen, leave

- **Mute** stops the microphone; **Deafen** stops playing the others and drops
  what is buffered. Each change, and every new connection, sends a two-byte
  message (`[1][flags]`: bit 0 muted, bit 1 deafened), and the people list shows
  it: `Ben · connected · muted`, `Ada (you) · deafened`.
- **Leave** disconnects from every peer, stops announcing and polling, takes this
  participant off the room with `DELETE`, and returns to the start screen. A peer
  that still dials afterwards is refused, and the answer to a request sent before
  leaving is ignored.

### Rooms of three and more

Sending everything to everyone (a full mesh) costs every participant one upload
per person. AzMeet follows the routes design (azul-apps
`planning/engines/iroh-routes.md`): cull what nobody shows, then route the rest
over the room's best connections. The rules live in
`examples/azul-meet/src/routes.rs` (plain Rust with unit tests); the choice of
forwarders is azul's `IrohLoadBalancer`.

1. **Reports.** Every participant sends everyone a `ConnectionSync` (kind 6) on
   connect, on every change and every 2 seconds: its uplink (estimated from
   `IrohEndpoint::peer_stats`: the bytes sent per second, or what one path's
   congestion window allows per round trip, reported in steps and only after
   three intervals in a row), its stability (intervals without a loss spike or an
   RTT jump), whether it is relay-only, on battery or opted out of forwarding,
   whether it sends audio, camera and screen, and the rendition each of its tiles
   asks for.
2. **The plan.** Every side feeds the same reports to the load balancer and so
   gets the same plan:

   ```rust
   let mut balancer = IrohLoadBalancer::create();
   balancer.set_mesh_cap(mesh_cap); // --mesh-cap, default 4 (the design says 8)
   for (key, report) in reports {
       let mut capacity = IrohPeerCapacity::create(key, report.uplink_kbps);
       capacity.stability = report.stability;
       balancer.set_peer(capacity);
   }
   let n = balancer.select_backbone(fanout_kbps); // grows until it carries 1.5x
   let backbone: Vec<u64> = (0..n)
       .filter_map(|i| balancer.backbone_peer(i).into_option())
       .collect();
   ```

   Up to the mesh cap everyone forwards, so the plan is the full mesh. Above it
   `max(ceil(sqrt N), ceil(N / 8))` peers, best score first, form the backbone;
   every other participant is a leaf attached to a backbone peer (round robin in
   key order). A leaf uploads its media once, to its parent, and receives
   everything through it; the parent passes a leaf's media to the other backbone
   peers, and every backbone peer passes what it gets to its own leaves.
3. **Renditions.** Each tile asks for `IrohTileRole::rendition_height` of its role
   (grid tile: `Gallery`, the speaker view's stage: `Stage`, its thumbnails:
   `Filmstrip`), its laid-out height (`CallbackInfo::get_node_size` of the tile)
   and the window's scale. A sender encodes the smallest and the largest height
   asked for, one `VideoEncoder` each at `IrohLoadBalancer::rendition_kbps`, and
   the camera widget gets one capture consumer per rendition; each viewer gets the
   smallest encoded rendition at least as tall as its tile, in H.264 where both
   ends do it. Nothing nobody shows is encoded (**Stop video (not shown to anyone,
   not being sent)**).
4. **Forwarding.** A forwarder passes each child only the streams someone at or
   below that child gets. Passed-on items travel in an envelope (kind 7:
   `[7][1][track u32][origin u64][from u64]` then the original bytes): audio and
   JPEG as frames, one frame track per origin; H.264 as messages, through a
   window per child like the sender's own. Acknowledgements and keyframe requests
   go back the way a stream came, and a forwarder passes a request on toward the
   origin: `Ben: passing Cleo's keyframe request for Ada's camera 90p on to Ada`,
   then `Ada: Cleo asked for a keyframe (camera 90p, via Ben)`.
5. **Network panel.** The devices panel's *Network* column shows the plan
   (`Network: 3 people, mesh cap 2: backbone Ben, Cleo; Ada uploads to Ben`),
   every origin's route (`Routes: Ada: Ada>Ben, Ben>Cleo | ...`), this side's part
   (`You: leaf, uploading once to Ben`) and report, what it passed on
   (`Forwarded: ...`), and a line per peer: direct or relayed and the RTT, backbone
   or leaf, its reported uplink, what this side sends it and gets through it.

### Run it

```sh
# the meeting server (azul-apps): its dev server, or the whole local stack
node cf-workers/meet/dev-server.mjs          # http://127.0.0.1:8787
local/up.sh && . local/state/env             # wrangler dev on :8790, a relay on :3340

# two participants (azul)
cargo run --release -p AzMeet -- --worker http://127.0.0.1:8787 --name Ada
cargo run --release -p AzMeet -- --worker http://127.0.0.1:8787 --name Ben
```

Ada clicks **New meeting** and **Copy link**; Ben pastes the link and clicks
**Join**. Each lands in the meeting's waiting room first: the camera preview, the
microphone and camera switches, the devices, the name, the meeting's code, link and
times, and who is in it already. With only the code Ben's button says **Ask to
join**, and Ada lets him in from the people panel. Without a reachable meeting
server the start screen says so, with **Retry**.

Every setting is a switch (`AzMeet --help`). Each also reads its `AZMEET_*`
environment variable when the switch is not given (`1` for a switch without a
value), so older scripts keep working; the switch wins.

| Switch (variable) | Meaning |
|---|---|
| `--worker <url>` (`AZMEET_WORKER`) | The meeting server, e.g. `http://127.0.0.1:8787`; the switch wins over the one saved from the start screen, the variable (and the Azlin config) does not |
| `--identity-file <path>` (`AZMEET_IDENTITY_FILE`) | Keep this device's key in that file (mode 0600), not the system keyring - a headless run's keyring lives in memory |
| `--open <link>` (`AZMEET_OPEN`) | Open that room's view at start (joining it, or knocking with a code) |
| `--chat-room`, `--starts-at <time>`, `--ends-at <time>` (`AZMEET_CHAT_ROOM=1`, `AZMEET_STARTS_AT`, `AZMEET_ENDS_AT`) | With `--autocreate`: a chat room instead of a meeting; a meeting's times (RFC 3339) |
| `--name <name>` (`AZMEET_NAME`) | The name the others see |
| `--autocreate`, `--join <link>` (`AZMEET_AUTOCREATE=1`, `AZMEET_JOIN`) | Start in a meeting without a click; the link is printed as `AZMEET_LINK <link>` |
| `--waiting-room` (`AZMEET_WAITING_ROOM=1`) | With those, stop in the waiting room first (`AZMEET_WAITING <link>`) |
| `--relay <off\|default\|url>` (`AZMEET_RELAY`) | The iroh relays (else the Azlin config's; else off for a meeting server on this machine) |
| `--relay-only` (`AZMEET_RELAY_ONLY=1`) | Never a direct path: no UDP socket, every packet through the relay (`IrohConfig::with_relay_only`); stdout `AZMEET_TRANSPORT relay-only <url>`, `AZMEET_PATH <peer> relayed` |
| `--test-tone` (`AZMEET_TEST_TONE=1`) | A 440 Hz tone replaces the microphone, unmuted from the start |
| `--test-pattern` (`AZMEET_TEST_PATTERN=1`) | Moving colour bars replace the camera (on from the start) and the screen; a **Drop a video packet** button drops the next packet |
| `--no-echo-cancel` (`AZMEET_ECHO_CANCEL=0`) | Send the microphone as it is (with headphones) |
| `--video-codec jpeg` (`AZMEET_VIDEO_CODEC`) | Send JPEG even where H.264 works |
| `--mesh-cap <n>` (`AZMEET_MESH_CAP`) | Rooms of up to n people send everything directly (default 4) |
| `--uplink-kbps <kbit/s>` (`AZMEET_UPLINK_KBPS`) | Report this uplink instead of the estimate |
| `--no-forward`, `--on-battery` (`AZMEET_NO_FORWARD=1`, `AZMEET_ON_BATTERY=1`) | Never forward for others; report running on battery |
| `--layout speaker`, `--stage <name>` (`AZMEET_LAYOUT`, `AZMEET_STAGE`) | Start in speaker view with that participant on the stage |
| `--panel <people\|chat\|statistics\|closed>` (`AZMEET_PANEL`) | What the call's side panel shows at start |

`--screen waiting` opens a new meeting's waiting room (a preview of one when no
meeting server answers), and with azul-appkit's `--shot <png>` writes it as a
screenshot: `AzMeet --screen waiting --test-pattern --theme flora --shot waiting.png`.

### Test it

`examples/azul-meet/scripts/two-clients.mjs` starts the mock meeting server and
two headless AzMeet processes, then checks through each app's debug server
(`AZ_DEBUG`) that they connect, that each counts at least a second of the other's
audio, that each decodes two seconds of the other's test pattern with a keyframe,
that a packet dropped on purpose (**Drop a video packet**) makes the receiver ask
for a keyframe and decode again (with H.264; with JPEG it costs nothing), that a
mute shows on the other side, and that **Leave** takes a participant off the room
at once. `--require-h264` fails a run that fell back to JPEG.

`examples/azul-meet/scripts/three-clients.mjs` runs three headless participants
with `--mesh-cap 2` and pinned uplinks (Ada 1 Mbps, Ben 50, Cleo 10; Cleo in
speaker view with Ben on the stage), so the backbone is Ben and Cleo and Ada is
Ben's leaf. It checks that every window shows the same plan and routes, that
everyone hears and sees both others at the planned renditions (360p for grid and
stage tiles, 90p for Cleo's thumbnail of Ada) and through the planned forwarder,
that Ada sends two renditions and Ben passes only the 90p on to Cleo, that a
keyframe request of Cleo's reaches Ada through Ben, and that the two left are
back in the full mesh when Cleo leaves. Both scripts share their helpers in
`meet-e2e.mjs`.

`scripts/azmeet_e2e.py` runs the call twice. First on this machine without a relay:
Ben waits in the waiting room (it must say "Ada is in this meeting"), switches his
microphone and camera, opens the settings and joins; both decode each other's
video, chat both ways, keep the meeting's files and Ada rejoins. Then through a
local relay with nothing direct: iroh's own relay server in dev mode
(`iroh-relay --dev`, plain HTTP on 127.0.0.1; `scripts/iroh_relay_dev.py` starts it
and reads its metrics), both apps with `--relay <its url> --relay-only`, the same
waiting room, video and chat, and the proof that the relay carried the call: each
side prints `AZMEET_PATH <other> relayed` and never `direct`, its statistics say
so, and the relay's own byte counters grew both ways. Build the relay once, outside
the repository: `cargo install iroh-relay@1.2.0 --locked --features server --root
~/.cache/azul/iroh-relay`; without it (and without `--relay-url`) the relay phase is
skipped and says so. Its `crypto` phase runs three devices in one encrypted chat
room: Ben opens the link, Cleo knocks with the code and Ada admits her, each room key
(1 for two, 2 for three, 3 once Ben left) is checked on stdout and in the meeting
server's database (who each key is sealed to, which key each message is under), every
safety code against the public keys there, and no value of any table - nor what it
decodes to - holds a message, a name, the invite secret or a device seed; Cleo
restarts and reads the history back from the ciphertext, and Ada schedules a meeting
that Cleo joins from the start screen. `--worker-url`, `--sqld-url`,
`--sqld-token-file` and `--relay-url` run it against a running stack.

A headless test must never open a real device. Under `AZ_BACKEND=headless` only
`AudioDeviceList::enumerate` is answered by the mock store (see
[e2e-testing](../debugging/e2e-testing.md)); `MicrophoneWidget`, `AudioSink::open`,
`CameraWidget` and `ScreenCaptureWidget` would still reach the hardware. So AzMeet
checks for itself: in a headless run its microphone is the test tone (no
`MicrophoneWidget` is mounted), received audio is drained and counted, never
played, and the camera and the screen share are test patterns (no capture widget
is mounted). Do the same in your own app.

### Not yet

- Opus and its loss concealment; echo cancellation and noise suppression.
- Routing: the plan is computed on every side from direct reports, not by one
  planner over gossip; there is no backup parent, no per-hop budget, and no
  hysteresis beyond the sticky uplink steps (a rendition switch costs a keyframe).
  Uplink fitting (a weak publisher stepping its top rendition down) and
  "active speaker" audio culling are not done.
- Video: bitrate that follows the link, HEVC / AV1, and H.264 encode outside Apple
  (Media Foundation, VAAPI / Vulkan Video, MediaCodec); until then those
  platforms send JPEG.
- Signed announcements on the meeting server; browser participants.

## What is on-device

The widget and handle surfaces above are cross-platform and always present. The
hardware backends are platform-specific:

- **Microphone**: ALSA on Linux, cpal (WASAPI) on Windows, AVAudioEngine on macOS
  and iOS, AAudio on Android.
- **Audio output** (`AudioSink`): ALSA on Linux, cpal on Windows, AVAudioEngine on
  macOS and iOS, AAudio on Android. A sink whose device does not open is closed,
  and `error_message()` says why.
- **Camera**: V4L2 on Linux, Media Foundation on Windows, AVFoundation on macOS
  and iOS, Camera2 on Android. **Screen**: the ScreenCast portal and PipeWire on
  Linux, DXGI desktop duplication on Windows, ScreenCaptureKit on macOS.
- Where no capture backend opens, the widgets fall back to a test pattern (video)
  or a 440 Hz test tone (audio) and say so once, so the plumbing runs without
  hardware.
- **Video encode/decode** (`VideoEncoder` / `VideoDecoder`), submit + poll:
  `VideoEncoder::open(w, h, h265, bitrate_kbps)` -> `encode(VideoFrame, force_keyframe)
  -> bool` (accepted) then drain `recv_packet() -> Option<U8Vec>`;
  `VideoDecoder::open(h265)` -> `decode(bytes) -> bool` then drain
  `recv_frame() -> Option<VideoFrame>`. Hardware and browser (WebCodecs)
  codecs are output-callback shaped, so one submitted frame can yield zero
  or several packets, possibly later - poll from a timer.
  `VideoEncoder::backend_name()` reports the platform-native codec the build
  selects: **gpu-video** (Vulkan Video) on Linux/Windows desktop, **VideoToolbox**
  on Apple (Vulkan Video can't build there - no MoltenVK video), **MediaCodec**
  on Android. The handles + the selection are exposed cross-platform; the codec
  FFI itself is the on-device part. Today only VideoToolbox encodes; elsewhere
  `open` hands out a handle whose `recv_packet` never yields, so check with a test
  frame before relying on it (AzMeet does). Send the packets as messages, not
  frames (see "Sending media between apps").

## Testing without hardware

The synthetic-event harness (`layout/tests/synthetic_events.rs`) injects
sensor / gamepad / geolocation / audio / video events through the same channels
a real device uses, so you can exercise the capture + event paths in CI. See
[e2e-testing](../debugging/e2e-testing.md). For two apps talking to each other,
`examples/azul-meet/scripts/two-clients.mjs` (above) is the pattern: two
headless processes, a test tone instead of a microphone, and assertions on what
each window shows.

## See also

- [networking](../data/networking.md#peer-to-peer-connections) - `IrohEndpoint`: tickets, frames, messages.
- [callbacks](../events/callbacks.md) - the hook + `RefAny` mechanism.
- [background-tasks](../data/background-tasks.md) - the `Thread` that drives capture.
- [timers](../animations/timers.md) - polling your transport for received frames each frame.
- [Mobile](../deploying/mobile.md) - shipping this on iOS / Android.
