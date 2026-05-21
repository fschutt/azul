---
slug: realtime-media
title: Realtime Media and Devices
language: en
canonical_slug: realtime-media
audience: external
maturity: beta
guide_order: 280
topic_only: false
short_desc: Camera/mic capture, audio playback, and sharing A/V over UDP (the azul-meet pattern)
prerequisites: [callbacks, background-tasks]
tracked_files:
  - layout/src/widgets/capture_common.rs
  - layout/src/widgets/microphone.rs
  - core/src/audio.rs
  - core/src/video.rs
  - dll/src/desktop/extra/audio/mod.rs
  - dll/src/desktop/extra/udp/mod.rs
  - examples/azul-meet/src/main.rs
last_generated_rev: 754b7f00e088960c14db598f64fa200dacc28bf1
generated_at: 2026-05-21T00:00:00Z
default-search-keys:
  - MicrophoneWidget
  - AudioSink
  - Udp
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

Azul exposes camera / screen / microphone **capture**, audio **playback**, and a
UDP **transport** as ordinary widgets and handles - no globals, no manager
singletons. Each capture source is a "dumb widget" that owns a background
worker and hands you each frame through a callback hook; playback and transport
are handles you keep in your own application `State`. Tying them together is the
`azul-meet` example (a loopback audio call): capture -> hook -> UDP -> playback.

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
        // through an effect, or encode + send it (see "Sharing over UDP").
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

## Sharing over UDP (`Udp`)

`Udp` is a thin, non-blocking wrapper over a UDP socket, again as a handle in
your `State`. UDP is connectionless and lossy by design, which is exactly the
fault-tolerant model you want for realtime A/V (a dropped packet is a dropped
frame, not a stall). You serialize your own payload into the `U8Vec`.

```rust
let udp = Udp::bind("0.0.0.0:9000".into());     // non-blocking
let port = udp.local_addr();                    // learn the OS-assigned port
udp.send_to(peer_addr, my_bytes);               // -> usize bytes sent
// Poll from a Timer or thread:
if let OptionU8Vec::Some(bytes) = udp.recv() { /* deserialize + use */ }
```

Payloads larger than the network MTU (a full video keyframe) use the built-in
chunking: `Udp::send_chunked` splits the message into sequenced datagrams and
`Udp::recv_chunked` reassembles them, tolerating reorder + loss (an incomplete
message is dropped, never retransmitted). Audio chunks (a few KB) fit in a
single datagram, so use `send_to` / `recv` for those.

## Putting it together: the azul-meet pattern

`examples/azul-meet` wires the full loop as a UDP loopback (it sends to its own
port, so the whole round-trip runs on one machine):

1. A `MicrophoneWidget` captures audio; its `on_frame` serializes the
   `AudioFrame` and `Udp::send_to`s it to the peer.
2. A recv `Timer` drains `Udp::recv()`, deserializes each datagram back into an
   `AudioFrame`, and `AudioSink::play`s it.

```rust
// on_frame: capture -> send
let bytes = frame_to_bytes(&frame);
s.udp.send_to(s.peer.clone(), bytes);

// recv Timer tick: receive -> play
while let OptionU8Vec::Some(bytes) = s.udp.recv() {
    if let Some(frame) = bytes_to_frame(bytes.as_ref()) {
        s.sink.play(frame);
    }
}
```

A real two-party call is the same code with `peer` set to the remote address.
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
  serialize/deserialize seam (`send_chunked` carries the encoded bytes).

## Testing without hardware

The synthetic-event harness (`layout/tests/synthetic_events.rs`) injects
sensor / gamepad / geolocation / audio / video events through the same channels
a real device uses, so you can exercise the capture + event paths in CI. See
[e2e-testing](e2e-testing.md).

## See also

- [callbacks](callbacks.md) - the hook + `RefAny` mechanism.
- [background-tasks](background-tasks.md) - the `Thread` that drives capture.
- [timers](timers.md) - polling `Udp::recv` each frame.
- [Mobile Deployment](mobile-deployment.md) - shipping this on iOS / Android.
