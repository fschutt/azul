# Phase 0 — platform API validation (per-subsystem)

Source-of-truth audit of `dll/src/desktop/extra/*` (device backends) + the
relevant `shell2/*` paths, done before fixing. Goal: confirm the *intended* data
path per OS, find where the wiring is wrong, and decide the "feature unavailable"
contract. References the bug reports in `scripts/problems/problems-{windows,linux,macos}.txt`.

> TL;DR of the two headline conclusions:
> 1. **Camera is NOT an NV12 problem.** Every backend already converts to **RGBA8**
>    before it reaches the CPU compositor (see table). The macOS "white screen,
>    no video" is far more likely the **C1 resize panic** (now fixed) or the
>    compositor first-present path (R2/R3), not the pixel format. The one real
>    format risk is the macOS **BGRA→RGBA byte-swap** in the AVFoundation delegate.
> 2. **The "unavailable" signalling is inconsistent.** Only `keyring` and
>    `biometric` expose a real result enum with an `Unavailable` variant. `camera`,
>    `audio`, `udp`, `sensors`, `gamepad`, `geolocation`, `video_codec` all use
>    **sentinel returns** (`0` handle / null ptr / empty `Option`) with no way for
>    the caller to distinguish "no device" from "platform not supported" from
>    "transient error". That is the Phase 2 work item (item c).

---

## Camera — `extra/camera/`

| OS | backend | requested format | conversion | output to compositor | device-absent |
|----|---------|------------------|------------|----------------------|---------------|
| Linux | libv4l2 (`v4l2.rs`) | `V4L2_PIX_FMT_RGB24` | libv4l2 converts NV12/YUYV→RGB24 internally; `rgb24_to_rgba()` (v4l2.rs:472) expands →RGBA | **RGBA8** | dlopen `libv4l2.so.0` fails → `open()` returns `0` |
| Windows | nokhwa (`windows.rs`) | `RgbAFormat` | none (nokhwa decodes) | **RGBA8** | `open()` returns `0` |
| macOS | AVFoundation (`avfoundation.rs`) | `kCVPixelFormatType_32BGRA` | **BGRA→RGBA byte-swap** in delegate `didOutputSampleBuffer` (avfoundation.rs:77) | **RGBA8** | `open()` returns `0` |
| Android | NDK Camera2 (`android.rs`) | `AIMAGE_FORMAT_YUV_420_888` | **BT.601 YUV→RGBA** in `read()` (android.rs:184) | **RGBA8** | `open()` returns `0` |

- Entry points (`camera/mod.rs`): `ensure_camera_backend()` registers a backend
  once (OnceLock); `open(index,w,h)->u64`, `read(handle,out)->(u32,u32)`,
  `close(handle)`. **No panics**; `0`/`(0,0)` on every failure.
- **The "NV12 → CPU RGB" the user flagged is real but already handled** — just
  spread across 4 different code paths with no shared, tested converter and **no
  logging of the negotiated format/dimensions**. Action (Phase 1/2): log
  `[camera] opened dev=N WxH src_fmt=.. -> RGBA` on open and `[camera] frame WxH`
  per N frames; add a single documented note that the compositor contract is
  RGBA8 top-down, stride = w*4.
- **macOS suspect:** verify the BGRA→RGBA swap (avfoundation.rs:77-82) actually
  matches what the CPU compositor samples; a wrong swap shows as colour-swapped
  (blue faces), a missing frame shows as the white screen → but white screen here
  is most likely C1.

## Audio — `extra/audio/`

- **Uniform format: interleaved `f32`, sample-rate passed through.** mic vtable:
  `mic_open(rate,channels)->u64`, `mic_read(handle,&mut Vec<f32>)->u32` (frame
  count, `0` = no data), `mic_close`. Sink: `AudioSink::open(AudioConfig)`,
  `play(AudioFrame)`, `is_open()` (null ptr = closed).
- Backends: cpal (macOS/Windows — converts I16/U16→f32 on the fly), ALSA
  (`libasound.so.2`, FLOAT_LE), AAudio (Android PCM_FLOAT), AVFoundation (iOS).
- Device-absent: cpal `default_input_device()`/ALSA dlopen → returns `0`/`None`,
  no panic. **No "unavailable" signal** — a self-test sees `mic_read()==0` for
  both "no mic" and "mic silent".
- Self-test mic-dots contract: read packets, compute RMS over the f32 frame,
  print a dot-bar. `mic_read` spins ~120×8ms internally before returning 0.

## UDP — `extra/udp/mod.rs`  (self-test loopback target)

```
Udp::bind(local_addr: AzString) -> Udp        // ptr=null on bind failure
.is_open() -> bool                            // !ptr.is_null()
.local_addr() -> AzString                     // "" on error
.send_to(remote_addr: AzString, data: U8Vec) -> usize   // bytes sent, 0 on err
.recv() -> OptionU8Vec                        // None on err / would-block
.send_chunked(remote_addr, data: U8Vec) -> usize        // datagram count
.recv_chunked() -> OptionU8Vec                // reassembles chunked messages
.close(&mut self)
```
- Wraps `std::net::UdpSocket`. Non-blocking recv (returns `None` when empty).
  **No panics.** Loopback test: `bind 127.0.0.1:0` ×2, `send_to(b.local_addr())`,
  poll `a`/`b` `.recv()` with a short sleep+retry. `send_chunked`/`recv_chunked`
  is the path a synthetic "video frame" (>1 datagram) exercises.

## Sensors — `extra/sensors/`  (push-based, not read-once)

- `ensure_started()` + `poll()` → **void**. Readings are *pushed* via
  `push_sensor_reading()` into the event stream, NOT returned. Linux = iio sysfs
  (`/sys/bus/iio/devices`, graceful if absent), Windows = WinRT
  (`Accelerometer::GetDefault` etc., `.ok()`-guarded), Apple/Android = stubs.
- **No availability return** — desktop without an accelerometer just never pushes.
  The self-test must `poll()` in a short loop and report "N readings seen / none
  (unavailable on this target)". This is the gap that produced the desktop
  "spirit-level crashes" (C4): a consumer that *assumes* a reading exists.

## Gamepad — `extra/gamepad/`

- `ensure_started()` + `poll()` → void; desktop uses **gilrs** (thread-local,
  lazy-init, `Gilrs::new().ok()` → early-return if unavailable). State pushed via
  `push_gamepad_state()`. iOS/Android pending.
- **C5 Linux double-free (`double free in tcache2`)** is almost certainly inside
  **gilrs / its evdev+udev handling**, not azul code (azul only calls
  `next_event()`/`gamepads()`). Action: pin/upgrade gilrs, reproduce under ASan,
  and wrap `poll()` so a backend failure logs + disables the backend instead of
  letting the process abort. Confirm gilrs version in `Cargo.lock`.

## Geolocation — `extra/geolocation/`

- `apply_diff_events(&[GeolocationDiffEvent])` → void; `probe_last_fix() ->
  Option<LocationFix>` currently returns **`None` on every platform** (stub).
  Linux=geoclue/D-Bus, others pending. Self-test reports "unavailable / not
  implemented" cleanly.

## Keyring + Biometric — `extra/keyring/`, `extra/biometric/`  (the good model)

- These already have the contract the others lack:
  - `KeyringResult` = `Stored | Retrieved(AzString) | Deleted | NotFound |
    Unavailable | Denied | Error` — pushed async via `push_keyring_result()`.
  - `BiometricResult` = `Authenticated | Cancelled | Failed | Unavailable | Error`.
  - `biometric::probe_availability() -> BiometricKind` (`NotAvailable | Fingerprint
    | Face`) + `availability_cached()`.
- **Use this `*::Unavailable` enum pattern as the template** for the capability
  contract added in Phase 2 (see below). Keyring/biometric are async (worker
  thread → push), so the self-test gates them behind `--interactive`.

## Video codec — `extra/video_codec/mod.rs`

- **Stub on all platforms.** `backend()` reports `VideoToolbox`(Apple) /
  `MediaCodec`(Android) / `gpu-video`(Linux/Win) / `none`. `VideoEncoder::open`
  returns an invalid handle when `backend()=="none"`; `encode()` returns empty
  `U8Vec`; `VideoDecoder::decode()` returns `OptionVideoFrame::None`. `is_open()`
  is the only availability probe. Self-test: report backend name + `is_open()`.

---

## Cross-cutting conclusions → feeds Phase 1/2

1. **Capability probe (Phase 2, item c).** Add a per-subsystem `*_available()
   -> bool` (or a small `Capability { available, backend, reason }`) so the API
   can answer "is this feature usable on this target/device" *without* attempting
   an operation and getting an ambiguous `0`. Surface it through the C-ABI. Model
   it on `biometric::probe_availability`. Subsystems lacking it: camera, audio,
   udp(always), sensors, gamepad, geolocation, video_codec.
2. **No panics on absence — already mostly true.** The audit found the `extra/*`
   entry paths are already sentinel-based, *not* panicking. So C2/C4/C5 crashes
   are most likely (a) in the **demo apps** that `unwrap()` a sentinel, or (b) in
   **third-party crates** (gilrs double-free C5; cpal/WinRT worker threads C2).
   Action: audit the demo crates for `unwrap()` on these returns, and wrap
   third-party worker threads so a panic is caught + logged, never aborts.
3. **Logging gap.** None of these backends log the negotiated format, the device
   index, or the unavailable reason. Phase 1 adds `plog_*!` traces at every
   open/read/close + a one-shot capabilities dump at startup. The
   `azul-self-test` binary is what makes those traces visible per-OS.
4. **Push vs pull.** sensors/gamepad/geolocation are push-based; the self-test
   drives a `poll()` loop and observes, it cannot "read once".
