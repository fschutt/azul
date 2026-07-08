---
slug: realtime-media
title: Realtime Media and Devices
language: en
canonical_slug: realtime-media
audience: external
maturity: beta
guide_order: 280
topic_only: false
short_desc: Camera/mic capture, audio playback, and streaming A/V frames to a peer (the azul-meet pattern)
prerequisites: [callbacks, background-tasks]
tracked_files:
  - layout/src/widgets/capture_common.rs
  - layout/src/widgets/microphone.rs
  - core/src/audio.rs
  - core/src/video.rs
  - dll/src/desktop/extra/audio/mod.rs
  - examples/azul-meet/src/main.rs
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
---

# Realtime Media and Devices

## Introduction

Azul exposes camera / screen / microphone **capture** and audio **playback** as
ordinary widgets and handles - no globals, no manager singletons. Each capture
source is a "dumb widget" that owns a background worker and hands you each frame
through a callback hook; playback is a handle you keep in your own application
`State`. Tying them together is the `azul-meet` example (a loopback audio call):
capture -> hook -> serialize -> [transport] -> deserialize -> playback.

> **Transport is your choice.** Capture and playback are transport-agnostic: a
> hook hands you decoded frames and `AudioSink` plays frames you hand it, so what
> carries the bytes between peers is entirely up to you. A first-class,
> browser-and-native peer-to-peer transport (the **AzMeet** conferencing layer)
> is being designed separately; until it lands, serialize frames yourself and
> send them over whatever transport your app already uses.

The architecture follows the framework's backreference dependency-injection
pattern (see [architecture](architecture.md)): a widget takes a `RefAny` (a
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

## Playing audio (`AudioSink`)

Playback is a handle, not a widget - you usually play audio you *received*, not
audio bound to a node. `AudioSink` follows the same C-ABI handle convention as
`Db` / `Pdf`: open it, keep it in your `State`, feed it frames, drop it to stop.

```rust
let sink = AudioSink::open(AudioConfig { sample_rate: 48_000, channels: 1 });
// ... later, for each frame you want to hear:
sink.play(frame);            // queues the samples to the output
// sink.is_open(), sink.frames_played(), sink.close()
```

## Streaming frames to a peer

Capture hands you a decoded frame; playback takes a frame. The only thing between
two peers is **your serialization + a transport of your choice**. A frame becomes
bytes, the bytes travel, and the far side turns them back into a frame:

```rust
// on_frame: capture -> serialize -> send over your transport
let bytes = frame_to_bytes(&frame);
s.transport.send(s.peer.clone(), bytes);

// recv (Timer tick or worker): receive -> deserialize -> play
while let Some(bytes) = s.transport.poll_recv() {
    if let Some(frame) = bytes_to_frame(bytes.as_ref()) {
        s.sink.play(frame);
    }
}
```

`frame_to_bytes` / `bytes_to_frame` are yours (a length-prefixed struct, or the
encoded codec bytes from `VideoEncoder` below). For a full keyframe that exceeds
the network MTU you chunk it into sequenced messages and reassemble on the far
side; a few-KB audio frame fits in a single message.

### The transport seam

`s.transport` above is deliberately abstract. The realtime-media APIs stop at the
**serialize/deserialize seam** so you can drop in whatever moves bytes between
peers — and the trade-offs there (raw datagrams vs. congestion-controlled QUIC,
direct peer-to-peer vs. relayed, native-only vs. also-in-the-browser) are exactly
what the **AzMeet** conferencing transport is being designed to standardize.
Until that ships as a first-class API, wire the seam to your own transport.

## Putting it together: the azul-meet pattern

`examples/azul-meet` wires the full loop as a **loopback** call (it sends to
itself, so the whole round-trip runs on one machine, no network required):

1. A `MicrophoneWidget` captures audio; its `on_frame` serializes the
   `AudioFrame` and sends it to the peer.
2. A recv `Timer` drains the transport, deserializes each message back into an
   `AudioFrame`, and `AudioSink::play`s it.

A real two-party call is the same code with `peer` set to the remote endpoint.
See `examples/azul-meet/src/main.rs` for the complete app (serialization +
Timer + State).

## What is on-device

The widget/handle surfaces above are cross-platform and always present. The
actual hardware backends are platform-specific and only run on a real device:

- **Capture** (camera, screen, microphone): AVFoundation / ScreenCaptureKit /
  AVAudioEngine on Apple, Camera2 / MediaProjection / AAudio on Android. The
  current desktop builds use stand-in workers (a test pattern / test tone) so
  the API + plumbing are exercisable without hardware.
- **Audio output** (`AudioSink`): rodio/cpal on desktop, AVAudioEngine / AAudio
  on mobile.
- **Video encode/decode** (`VideoEncoder` / `VideoDecoder`):
  `VideoEncoder::open(w, h, h265, bitrate_kbps)` -> `encode(VideoFrame, force_keyframe)
  -> bytes`; `VideoDecoder::open(h265)` -> `decode(bytes) -> Option<VideoFrame>`.
  `VideoEncoder::backend_name()` reports the platform-native codec the build
  selects: **gpu-video** (Vulkan Video) on Linux/Windows desktop, **VideoToolbox**
  on Apple (Vulkan Video can't build there - no MoltenVK video), **MediaCodec**
  on Android. The handles + the selection are exposed cross-platform; the codec
  FFI itself is the on-device part. Use these at the `azul-meet`
  serialize/deserialize seam (your transport carries the encoded bytes).

## Testing without hardware

The synthetic-event harness (`layout/tests/synthetic_events.rs`) injects
sensor / gamepad / geolocation / audio / video events through the same channels
a real device uses, so you can exercise the capture + event paths in CI. See
[e2e-testing](e2e-testing.md).

## See also

- [callbacks](callbacks.md) - the hook + `RefAny` mechanism.
- [background-tasks](background-tasks.md) - the `Thread` that drives capture.
- [timers](timers.md) - polling your transport for received frames each frame.
- [Mobile](mobile.md) - shipping this on iOS / Android.
