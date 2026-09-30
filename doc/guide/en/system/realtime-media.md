---
slug: system/realtime-media
title: Realtime Media
language: en
canonical_slug: system/realtime-media
audience: external
maturity: beta
guide_order: 124
topic_only: false
short_desc: Camera/mic capture, audio playback, and media between apps over iroh (AzMeet - rooms, audio, leave)
prerequisites: [events/callbacks, data/background-tasks]
tracked_files:
  - layout/src/widgets/capture_common.rs
  - layout/src/widgets/microphone.rs
  - core/src/audio.rs
  - core/src/video.rs
  - dll/src/desktop/extra/audio/mod.rs
  - dll/src/desktop/extra/iroh/engine.rs
  - examples/azul-meet/src/lib.rs
  - examples/azul-meet/src/audio.rs
  - examples/azul-meet/src/rooms.rs
  - examples/azul-meet/scripts/two-clients.mjs
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
// sink.is_open(), sink.frames_played(), sink.close()
```

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
server it shows **New meeting** and **Join with a link**; in a meeting it shows the
invite link, the people, a tile per camera or screen, and the buttons **Mute**,
**Deafen**, **Start video**, **Share screen** and **Leave**.

### Rooms

The meeting server is the `meet` Cloudflare Worker (azul-apps `cf-workers/meet`,
with a local mock, `dev-server.mjs`). It never sees any media; it only lets the
apps find each other:

| Request | What AzMeet uses it for |
|---|---|
| `POST /rooms` | New meeting: a room id (the credential), a short code, the link `azlin://meet/<room>` |
| `GET /rooms/<id or code>?format=json` | Joining: resolves a pasted link or code |
| `POST /rooms/<id>/peers` with `{node_id, ticket, name}` | Announces this app's iroh ticket, every 20 s |
| `GET /rooms/<id>/peers?except=<node_id>` | Reads everyone else's ticket, every 2 s |
| `DELETE /rooms/<id>/peers/<node_id>` | Leaving: off the list at once |

Of each pair of participants the one with the lower endpoint id dials, so two
peers that find each other in the same poll open one connection. Every request
runs on an azul `Thread` and resumes on the UI thread, so no callback waits on the
network. A participant that stops announcing drops off after 120 s, a room after
a day without announcements.

### Video

Each camera or screen frame is cut to 320x180 and sent on track 1 (camera) or 2
(screen); each peer's pictures land in that peer's own tiles. The rules live in
`examples/azul-meet/src/video_wire.rs` (plain Rust with unit tests).

1. **Codec.** At start AzMeet opens a `VideoEncoder`, encodes a test frame and
   decodes the result with a `VideoDecoder`. An open handle proves nothing (where
   no backend is built in, `open` hands out a handle that never yields a packet),
   so only a keyframe that comes back as a picture counts. Where it does
   (VideoToolbox on macOS and iOS), frames go out as H.264; elsewhere as JPEG. The
   devices panel says which: `Video: H.264 (VideoToolbox)`, `Video: JPEG (no
   encoder)`. On connecting, each side sends `[5][flags]` (bit 0: decodes H.264),
   and a peer that cannot decode H.264 gets JPEG from a sender that encodes it:
   `Video: H.264 (VideoToolbox); JPEG to Ben (no H.264 decoder)`.
2. **Packets.** Every packet carries a 16-byte header:

   ```text
   [2][version 1][codec: 1 JPEG, 2 H.264][flags: bit 0 keyframe]
   [track u32][seq u32][frame_no u32]      then a JPEG file or H.264 Annex B
   ```

   `seq` counts one codec's packets on one track, so a gap is a missing packet;
   `frame_no` counts the frames captured and may jump. H.264 packets are sent as
   messages; JPEG ones as latest-wins frames.
3. **Loss.** A receiver decodes in order. After a gap in `seq` it decodes nothing
   until the next keyframe (an IDR slice) and sends `[3][track]`, a keyframe
   request, again after a second while it still waits. The sender forces a
   keyframe on a request (several requests within half a second share one), for
   every new peer, and every 3 seconds anyway. An encoder that answers a forced
   keyframe with a P-frame is closed and opened again: a new encoder starts with a
   keyframe.
4. **A slow link.** Messages are never dropped, so a link slower than the video
   would queue without end. The receiver acknowledges H.264 packets (`[4][track][seq]`,
   every fifth and every keyframe); a sender more than 24 packets ahead of a peer
   pauses that peer and resumes it at a keyframe once it caught up.
5. **Decoding.** Each peer's tracks have their own `VideoDecoder`. One that is
   given 30 packets from a keyframe on and returns no picture does not work here:
   AzMeet tells everyone it decodes no H.264, and gets JPEG.

The devices panel shows both directions: `Sending camera: 450 H.264 packets, 0
JPEG frames, 8 keyframes, 2 on request, 5 periodic, 0 reopens, 0 dropped on
purpose` and `Video from Ben (camera): H.264, decoded 300, keyframes 12, gaps 1,
dropped 3, keyframe requests 1`.

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

### Run it

```sh
# the meeting server (azul-apps)
node cf-workers/meet/dev-server.mjs

# two participants (azul)
AZMEET_WORKER=http://127.0.0.1:8787 AZMEET_NAME=Ada cargo run --release -p AzMeet
AZMEET_WORKER=http://127.0.0.1:8787 AZMEET_NAME=Ben cargo run --release -p AzMeet
```

Ada clicks **New meeting** and **Copy link**; Ben pastes the link and clicks
**Join**. Without a reachable meeting server AzMeet opens its in-process demo
instead: two windows, one per participant, linked by two endpoints.

| Variable | Meaning |
|---|---|
| `AZMEET_WORKER` | The meeting server, e.g. `http://127.0.0.1:8787` |
| `AZMEET_NAME` | The name the others see |
| `AZMEET_AUTOCREATE=1`, `AZMEET_JOIN=<link>` | Start in a meeting without a click; the link is printed as `AZMEET_LINK <link>` |
| `AZMEET_RELAY` | `off`, `default` or a relay URL (off for a meeting server on this machine) |
| `AZMEET_TEST_TONE=1` | A 440 Hz tone replaces the microphone, unmuted from the start |
| `AZMEET_TEST_PATTERN=1` | Moving colour bars replace the camera (on from the start) and the screen; a **Drop a video packet** button drops the next packet |
| `AZMEET_VIDEO_CODEC=jpeg` | Send JPEG even where H.264 works |

### Test it

`examples/azul-meet/scripts/two-clients.mjs` starts the mock meeting server and
two headless AzMeet processes, then checks through each app's debug server
(`AZ_DEBUG`) that they connect, that each counts at least a second of the other's
audio, that each decodes two seconds of the other's test pattern with a keyframe,
that a packet dropped on purpose (**Drop a video packet**) makes the receiver ask
for a keyframe and decode again (with H.264; with JPEG it costs nothing), that a
mute shows on the other side, and that **Leave** takes a participant off the room
at once. `--require-h264` fails a run that fell back to JPEG.

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
- `IrohLoadBalancer`: today every participant sends to every other one (a full
  mesh), which does not scale past a handful of people.
- Video: renditions per tile size (a simulcast ladder), bitrate that follows the
  link, HEVC / AV1, and H.264 encode outside Apple (Media Foundation, VAAPI /
  Vulkan Video, MediaCodec); until then those platforms send JPEG.
- Signed announcements on the meeting server; browser participants.

## What is on-device

The widget and handle surfaces above are cross-platform and always present. The
hardware backends are platform-specific:

- **Microphone**: ALSA on Linux, cpal (WASAPI) on Windows, AVAudioEngine on macOS
  and iOS, AAudio on Android.
- **Audio output** (`AudioSink`): ALSA on Linux, cpal on Windows, AVAudioEngine on
  macOS and iOS, AAudio on Android. A sink whose device does not open still counts
  frames, and says so once.
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
